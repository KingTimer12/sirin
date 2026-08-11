//! Sirin Language Server (stdio).
//!
//! v1 scope: diagnostics (parse + type errors) on open/change, hover (type or
//! signature of the symbol under the cursor), go-to-definition — including
//! definitions in local modules reachable through `use` — and completion
//! (identifiers in scope, keywords, and members after `receiver.`).
//!
//! The server re-parses on demand; there is no incremental state beyond the
//! current text of each open document. Sirin files are small enough that a
//! full parse per keystroke is well under a millisecond.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chumsky::Parser as _;
use chumsky::input::Input as _;
use chumsky::span::SimpleSpan;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use sirin_parser::aliases::resolve_aliases;
use sirin_parser::parser::parser;
use sirin_parser::span::Spanned;
use sirin_parser::stmt::Stmt;
use sirin_parser::types::Type;
use sirin_typechecker::checker::Checker;

// ── Type / signature rendering ────────────────────────────────────────────────

fn type_str(t: &Type) -> String {
    match t {
        Type::Int => "int".into(),
        Type::Float => "float".into(),
        Type::Str => "str".into(),
        Type::Bool => "bool".into(),
        Type::Void => "void".into(),
        Type::U8 => "u8".into(),
        Type::U16 => "u16".into(),
        Type::U32 => "u32".into(),
        Type::U64 => "u64".into(),
        Type::I8 => "i8".into(),
        Type::I16 => "i16".into(),
        Type::I32 => "i32".into(),
        Type::I64 => "i64".into(),
        Type::Nullable(i) => format!("{}?", type_str(i)),
        Type::Try(i) => format!("{}!", type_str(i)),
        Type::Array(i) => format!("Array[{}]", type_str(i)),
        Type::Vec(i) => format!("Vec[{}]", type_str(i)),
        Type::Set(i) => format!("Set[{}]", type_str(i)),
        Type::Map(k, v) => format!("Map[{}, {}]", type_str(k), type_str(v)),
        Type::Channel(i) => format!("Channel[{}]", type_str(i)),
        Type::Named(n) => n.clone(),
        Type::Struct(fields) => {
            let fs = fields.iter()
                .map(|(n, t)| format!("{}: {}", n, type_str(t)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{ {} }}", fs)
        }
        Type::Func(args, ret) => {
            let a = args.iter().map(type_str).collect::<Vec<_>>().join(", ");
            format!("fn({}) -> {}", a, type_str(ret))
        }
    }
}

fn fn_signature(name: &str, args: &[(Spanned<&str>, Type)], ret: &Option<Type>) -> String {
    let a = args.iter()
        .map(|(n, t)| format!("{}: {}", n.node, type_str(t)))
        .collect::<Vec<_>>()
        .join(", ");
    match ret {
        Some(r) => format!("fn {}({}) -> {}", name, a, type_str(r)),
        None => format!("fn {}({})", name, a),
    }
}

// ── Definition index ──────────────────────────────────────────────────────────

/// What a definition is, so completion can label it and decide whether it is
/// reachable on its own (a function) or only through a receiver (a field).
#[derive(Clone, Copy, PartialEq)]
enum DefKind {
    Var,
    Param,
    Func,
    Method,
    Class,
    Interface,
    Enum,
    Variant,
    Field,
    TypeAlias,
}

#[derive(Clone)]
struct Def {
    name: String,
    /// hover text, e.g. `fn area(f: Forma) -> float` or `raio: float`
    detail: String,
    kind: DefKind,
    /// type that owns this member — `Some("Animal")`, `Some("int")` for an
    /// `impl int` method. `None` for anything reachable without a receiver.
    owner: Option<String>,
    /// declared or inferred type, used to resolve `receiver.` completions
    ty: Option<Type>,
    /// superclass name, on class defs
    extends: Option<String>,
    /// byte range of the whole declaration, on class defs — locates `self`
    scope: Option<(usize, usize)>,
    /// byte range of the defining name token
    start: usize,
    end: usize,
    /// file the definition lives in (None = current document)
    file: Option<PathBuf>,
}

impl Def {
    fn new(name: &Spanned<&str>, detail: String, kind: DefKind, file: Option<&PathBuf>) -> Def {
        Def {
            name: name.node.to_string(),
            detail,
            kind,
            owner: None,
            ty: None,
            extends: None,
            scope: None,
            start: name.span.start,
            end: name.span.end,
            file: file.cloned(),
        }
    }
}

/// Type of an initializer, as far as it can be told without the checker: enough
/// for `x = Animal("Rex")` to make `x.` complete Animal's members.
fn rough_expr_type(e: &sirin_parser::expr::Expr<'_>) -> Option<Type> {
    use sirin_parser::expr::Expr;
    match e {
        Expr::Int(_) => Some(Type::Int),
        Expr::Float(_) => Some(Type::Float),
        Expr::Str(_) => Some(Type::Str),
        Expr::Boolean(_) => Some(Type::Bool),
        Expr::New(n, _) | Expr::NewDefault(n) | Expr::NewFields(n, _) => {
            Some(Type::Named(n.to_string()))
        }
        _ => None,
    }
}

fn collect_defs(
    stmts: &[Spanned<Stmt<'_>>],
    file: Option<&PathBuf>,
    owner: Option<&str>,
    out: &mut Vec<Def>,
) {
    for s in stmts {
        collect_stmt_defs(&s.node, (s.span.start, s.span.end), file, owner, out);
    }
}

fn collect_stmt_defs(
    stmt: &Stmt<'_>,
    span: (usize, usize),
    file: Option<&PathBuf>,
    owner: Option<&str>,
    out: &mut Vec<Def>,
) {
    let def = |name: &Spanned<&str>, detail: String, kind: DefKind| {
        Def::new(name, detail, kind, file)
    };
    match stmt {
        Stmt::Let { name, ty, rhs } | Stmt::CopyLet { name, ty, rhs } => {
            let inferred = ty.clone().or_else(|| rough_expr_type(&rhs.node));
            let detail = match &inferred {
                Some(t) => format!("{}: {}", name.node, type_str(t)),
                None => format!("{} (inferido)", name.node),
            };
            let mut d = def(name, detail, DefKind::Var);
            d.ty = inferred;
            out.push(d);
        }
        Stmt::Fn { name, args, return_type, body, async_ } => {
            let sig = fn_signature(name.node, args, return_type);
            let sig = if *async_ { format!("async {}", sig) } else { sig };
            let kind = if owner.is_some() { DefKind::Method } else { DefKind::Func };
            let mut d = def(name, sig, kind);
            d.owner = owner.map(str::to_string);
            out.push(d);
            for (aname, aty) in args {
                let mut p = def(aname, format!("{}: {}", aname.node, type_str(aty)), DefKind::Param);
                p.ty = Some(aty.clone());
                out.push(p);
            }
            // Locals inside a method belong to nobody — only the method itself
            // is reached through a receiver.
            collect_defs(body, file, None, out);
        }
        Stmt::AbstractFn { name, args, return_type } => {
            let mut d = def(
                name,
                format!("abstract {}", fn_signature(name.node, args, return_type)),
                if owner.is_some() { DefKind::Method } else { DefKind::Func },
            );
            d.owner = owner.map(str::to_string);
            out.push(d);
        }
        Stmt::If { then, else_, .. } => {
            collect_defs(then, file, None, out);
            if let Some(e) = else_ { collect_defs(e, file, None, out); }
        }
        Stmt::IfLet { name, then, else_, .. } => {
            out.push(def(name, format!("{} (binding)", name.node), DefKind::Var));
            collect_defs(then, file, None, out);
            if let Some(e) = else_ { collect_defs(e, file, None, out); }
        }
        Stmt::TryAssign { name, .. } => {
            out.push(def(name, format!("{} (Ok binding)", name.node), DefKind::Var));
        }
        Stmt::While { body, .. } => collect_defs(body, file, None, out),
        Stmt::For { var, body, .. } => {
            let mut d = def(var, format!("{}: int", var.node), DefKind::Var);
            d.ty = Some(Type::Int);
            out.push(d);
            collect_defs(body, file, None, out);
        }
        Stmt::Match { arms, .. } => {
            for arm in arms {
                for b in &arm.binds {
                    out.push(def(b, format!("{} (payload)", b.node), DefKind::Var));
                }
                collect_defs(&arm.body, file, None, out);
            }
        }
        Stmt::Enum { name, variants } => {
            let vs = variants.iter()
                .map(|(v, tys)| if tys.is_empty() {
                    v.node.to_string()
                } else {
                    format!("{}({})", v.node, tys.iter().map(type_str).collect::<Vec<_>>().join(", "))
                })
                .collect::<Vec<_>>()
                .join(", ");
            let mut e = def(name, format!("enum {} {{ {} }}", name.node, vs), DefKind::Enum);
            e.ty = Some(Type::Named(name.node.to_string()));
            out.push(e);
            for (v, tys) in variants {
                let d = if tys.is_empty() {
                    format!("{}.{}", name.node, v.node)
                } else {
                    format!("{}.{}({})", name.node, v.node,
                        tys.iter().map(type_str).collect::<Vec<_>>().join(", "))
                };
                out.push(def(v, d, DefKind::Variant));
            }
        }
        Stmt::Class { name, fields, methods, extends, .. } => {
            let mut c = def(name, format!("class {}", name.node), DefKind::Class);
            c.ty = Some(Type::Named(name.node.to_string()));
            c.extends = extends.as_ref().map(|e| e.node.to_string());
            c.scope = Some(span);
            out.push(c);
            for f in fields {
                let mut d = def(
                    &f.name,
                    format!("{}: {}", f.name.node, type_str(&f.ty)),
                    DefKind::Field,
                );
                d.owner = Some(name.node.to_string());
                d.ty = Some(f.ty.clone());
                out.push(d);
            }
            collect_defs(methods, file, Some(name.node), out);
        }
        Stmt::Interface { name, methods } => {
            let mut i = def(name, format!("interface {}", name.node), DefKind::Interface);
            i.ty = Some(Type::Named(name.node.to_string()));
            out.push(i);
            for m in methods {
                let mut d = def(
                    &m.name,
                    fn_signature(m.name.node, &m.args, &m.return_type),
                    DefKind::Method,
                );
                d.owner = Some(name.node.to_string());
                out.push(d);
            }
        }
        // `impl int { .. }` hangs methods off the type's rendered name, the same
        // key `receiver.` completion looks up for a primitive receiver.
        Stmt::Impl { target, methods } => {
            let key = type_str(&impl_target_type(target));
            collect_defs(methods, file, Some(&key), out);
        }
        Stmt::Init { args, body } => {
            for (aname, aty) in args {
                let mut p = def(aname, format!("{}: {}", aname.node, type_str(aty)), DefKind::Param);
                p.ty = Some(aty.clone());
                out.push(p);
            }
            collect_defs(body, file, None, out);
        }
        Stmt::Default { body } | Stmt::Spawn { body } => collect_defs(body, file, None, out),
        Stmt::TypeAlias { name, ty } => {
            out.push(def(
                name,
                format!("type {} = {}", name.node, type_str(ty)),
                DefKind::TypeAlias,
            ));
        }
        _ => {}
    }
}

fn impl_target_type(t: &sirin_parser::stmt::ImplTarget<'_>) -> Type {
    use sirin_parser::stmt::ImplTarget as T;
    match t {
        T::Named(n) => Type::Named(n.to_string()),
        T::Int => Type::Int,
        T::Float => Type::Float,
        T::Str => Type::Str,
        T::Bool => Type::Bool,
        T::U8 => Type::U8,
        T::U16 => Type::U16,
        T::U32 => Type::U32,
        T::U64 => Type::U64,
        T::I8 => Type::I8,
        T::I16 => Type::I16,
        T::I32 => Type::I32,
        T::I64 => Type::I64,
    }
}

// ── Text position helpers ─────────────────────────────────────────────────────

fn offset_to_position(text: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut col16 = 0u32;
    for (i, c) in text.char_indices() {
        if i >= offset { break; }
        if c == '\n' {
            line += 1;
            col16 = 0;
        } else {
            col16 += c.len_utf16() as u32;
        }
    }
    Position { line, character: col16 }
}

fn position_to_offset(text: &str, pos: Position) -> usize {
    let mut line = 0u32;
    let mut col16 = 0u32;
    for (i, c) in text.char_indices() {
        if line == pos.line && col16 >= pos.character {
            return i;
        }
        if c == '\n' {
            if line == pos.line { return i; } // past end of target line
            line += 1;
            col16 = 0;
        } else {
            col16 += c.len_utf16() as u32;
        }
    }
    text.len()
}

fn span_to_range(text: &str, start: usize, end: usize) -> Range {
    Range {
        start: offset_to_position(text, start),
        end: offset_to_position(text, end),
    }
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Identifier that covers byte `offset` in `text`, with its byte range.
fn word_at(text: &str, offset: usize) -> Option<(String, usize, usize)> {
    if text.is_empty() { return None; }
    let bytes = text.as_bytes();
    let mut i = offset.min(text.len());
    // If the cursor sits right after the word, step back one.
    if i == text.len() || !is_ident_char(bytes[i] as char) {
        if i == 0 || !is_ident_char(bytes[i - 1] as char) { return None; }
        i -= 1;
    }
    let mut start = i;
    while start > 0 && is_ident_char(bytes[start - 1] as char) { start -= 1; }
    let mut end = i;
    while end < text.len() && is_ident_char(bytes[end] as char) { end += 1; }
    let word = &text[start..end];
    if word.chars().next().map_or(true, |c| c.is_ascii_digit()) { return None; }
    Some((word.to_string(), start, end))
}

/// Receiver of a `receiver.` member completion at `offset`, if the cursor sits
/// in a member position. Only a bare identifier receiver is recognised —
/// chains like `a.b.c` would need the checker to type intermediate steps.
fn dot_receiver(text: &str, offset: usize) -> Option<String> {
    let bytes = text.as_bytes();
    let mut start = offset.min(text.len());
    while start > 0 && is_ident_char(bytes[start - 1] as char) {
        start -= 1;
    }
    if start == 0 || bytes[start - 1] != b'.' {
        return None;
    }
    let dot = start - 1;
    let mut rstart = dot;
    while rstart > 0 && is_ident_char(bytes[rstart - 1] as char) {
        rstart -= 1;
    }
    if rstart == dot {
        return None;
    }
    Some(text[rstart..dot].to_string())
}

// ── Built-in members ──────────────────────────────────────────────────────────

/// Methods the checker implements directly on built-in types (see
/// `Checker::check_expr`'s `MethodCall` arm) — they have no `Stmt` to index.
fn builtin_methods(t: &Type) -> Vec<(String, String)> {
    let m = |sig: String| sig;
    match t {
        Type::Vec(i) | Type::Array(i) => vec![
            ("push".into(), m(format!("push(v: {})", type_str(i)))),
            ("len".into(), m("len() -> int".into())),
            ("free".into(), m("free()".into())),
        ],
        Type::Map(k, v) => vec![
            ("insert".into(), m(format!("insert(k: {}, v: {})", type_str(k), type_str(v)))),
            ("set".into(), m(format!("set(k: {}, v: {})", type_str(k), type_str(v)))),
            ("get".into(), m(format!("get(k: {}) -> {}?", type_str(k), type_str(v)))),
            ("len".into(), m("len() -> int".into())),
            ("keys_at".into(), m("keys_at(i: int) -> str?".into())),
        ],
        Type::Set(i) => vec![
            ("insert".into(), m(format!("insert(v: {})", type_str(i)))),
            ("contains".into(), m(format!("contains(v: {}) -> bool", type_str(i)))),
        ],
        Type::Channel(i) => vec![
            ("send".into(), m(format!("send(v: {})", type_str(i)))),
            ("recv".into(), m(format!("recv() -> {}", type_str(i)))),
            ("free".into(), m("free()".into())),
        ],
        Type::Str => vec![
            ("len".into(), m("len() -> int".into())),
            ("index_of".into(), m("index_of(s: str) -> int".into())),
            ("to_int".into(), m("to_int() -> int".into())),
            ("to_float".into(), m("to_float() -> float".into())),
            ("contains".into(), m("contains(s: str) -> bool".into())),
            ("starts_with".into(), m("starts_with(s: str) -> bool".into())),
            ("ends_with".into(), m("ends_with(s: str) -> bool".into())),
            ("char_at".into(), m("char_at(i: int) -> str".into())),
            ("slice".into(), m("slice(from: int, to: int) -> str".into())),
            ("trim".into(), m("trim() -> str".into())),
            ("to_upper".into(), m("to_upper() -> str".into())),
            ("to_lower".into(), m("to_lower() -> str".into())),
            ("replace".into(), m("replace(from: str, to: str) -> str".into())),
            ("split".into(), m("split(sep: str) -> Vec[str]".into())),
            ("to_object".into(), m("to_object()".into())),
        ],
        _ => vec![],
    }
}

const KEYWORDS: &[&str] = &[
    "fn", "async", "spawn", "await", "return", "if", "else", "while", "for", "in",
    "break", "continue", "enum", "match", "and", "or", "try", "class", "abstract",
    "extends", "implements", "interface", "impl", "is", "init", "default", "mut",
    "self", "use", "type", "true", "false",
];

const BUILTIN_TYPES: &[&str] = &[
    "int", "float", "str", "bool", "u8", "u16", "u32", "u64", "i8", "i16", "i32",
    "i64", "Array", "Vec", "Map", "Set",
];

const CONSTRUCTORS: &[&str] = &["Some", "None", "Ok", "Err"];

// ── Analysis (parse + typecheck + index) ──────────────────────────────────────

struct Analysis {
    diagnostics: Vec<Diagnostic>,
    /// defs in this document plus defs from `use`d local modules
    defs: Vec<Def>,
    parsed: bool,
}

fn simple_module_path(use_path: &[&str], from: &Path) -> Option<PathBuf> {
    if use_path.first().copied() == Some("sirin") { return None; }
    let dir = from.parent()?;
    let mut p = dir.to_path_buf();
    for seg in &use_path[..use_path.len().saturating_sub(1)] {
        p.push(seg);
    }
    p.push(format!("{}.sn", use_path.last()?));
    p.canonicalize().ok()
}

fn analyze(text: &str, file_path: Option<&Path>) -> Analysis {
    let mut diagnostics = Vec::new();
    let mut defs = Vec::new();

    let tokens = sirin_parser::lex(text);
    let eoi = SimpleSpan::from(text.len()..text.len());
    let parsed = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result();

    let mut stmts = match parsed {
        Ok(s) => s,
        Err(errors) => {
            for e in &errors {
                let span = e.span();
                diagnostics.push(Diagnostic {
                    range: span_to_range(text, span.start, span.end),
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("sirin".into()),
                    message: format!("erro de sintaxe: {:?}", e),
                    ..Default::default()
                });
            }
            return Analysis { diagnostics, defs, parsed: false };
        }
    };

    // Resolve local modules (one level of the dependency walk per module file —
    // enough for hover/goto and for the checker to know imported signatures).
    let mut alias_map: HashMap<String, Type> = HashMap::new();
    let mut checker = Checker::new(text);
    let mut visited: Vec<PathBuf> = Vec::new();
    if let Some(fp) = file_path {
        let mut queue: Vec<PathBuf> = stmts.iter()
            .filter_map(|s| match &s.node {
                Stmt::Use { path } => simple_module_path(path, fp),
                _ => None,
            })
            .collect();
        while let Some(mp) = queue.pop() {
            if visited.contains(&mp) { continue; }
            visited.push(mp.clone());
            let Ok(msrc) = std::fs::read_to_string(&mp) else { continue; };
            let mtokens = sirin_parser::lex(&msrc);
            let meoi = SimpleSpan::from(msrc.len()..msrc.len());
            let Ok(mut mstmts) = parser()
                .parse(mtokens.as_slice().split_token_span(meoi))
                .into_result()
            else { continue; };
            resolve_aliases(&mut mstmts, &mut alias_map);
            checker.import_module(&mstmts);
            collect_defs(&mstmts, Some(&mp), None, &mut defs);
            for s in &mstmts {
                if let Stmt::Use { path } = &s.node {
                    if let Some(dep) = simple_module_path(path, &mp) {
                        queue.push(dep);
                    }
                }
            }
        }
    }

    resolve_aliases(&mut stmts, &mut alias_map);

    for s in &stmts {
        if let Err(e) = checker.check_stmt(s) {
            diagnostics.push(Diagnostic {
                range: span_to_range(text, s.span.start, s.span.end),
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("sirin".into()),
                message: format!("erro de tipo: {:?}", e),
                ..Default::default()
            });
        }
    }

    collect_defs(&stmts, None, None, &mut defs);
    Analysis { diagnostics, defs, parsed: true }
}

/// `analyze`, but tolerant of the half-written line under the cursor.
///
/// Completion fires precisely when the document does not parse — `x.` and
/// `Anim` are both syntax errors — and a failed parse yields no defs at all.
/// Retrying with the current line blanked out (spaces, so every other byte
/// offset still lines up) recovers the rest of the file's index.
fn analyze_for_completion(text: &str, file_path: Option<&Path>, offset: usize) -> Analysis {
    let first = analyze(text, file_path);
    if first.parsed {
        return first;
    }

    let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[offset..].find('\n').map_or(text.len(), |i| offset + i);
    let mut patched = text.as_bytes().to_vec();
    for b in &mut patched[line_start..line_end] {
        *b = b' ';
    }
    let Ok(patched) = String::from_utf8(patched) else { return first };

    let retry = analyze(&patched, file_path);
    if retry.parsed { retry } else { first }
}

// ── Server ────────────────────────────────────────────────────────────────────

struct Backend {
    client: Client,
    docs: Mutex<HashMap<Url, String>>,
}

impl Backend {
    async fn on_change(&self, uri: Url, text: String) {
        let file_path = uri.to_file_path().ok();
        let analysis = analyze(&text, file_path.as_deref());
        self.docs.lock().await.insert(uri.clone(), text);
        self.client
            .publish_diagnostics(uri, analysis.diagnostics, None)
            .await;
    }

    /// Best matching def for `word` seen from byte `offset`: the closest
    /// definition at or before the cursor, falling back to any definition
    /// (functions/classes are used before their textual position).
    fn find_def<'d>(defs: &'d [Def], word: &str, offset: usize) -> Option<&'d Def> {
        defs.iter()
            .filter(|d| d.name == word && d.file.is_none() && d.start <= offset)
            .max_by_key(|d| d.start)
            .or_else(|| defs.iter().find(|d| d.name == word))
    }

    /// Type of the receiver in `receiver.` — a variable's type, or the class
    /// enclosing the cursor when the receiver is `self`.
    fn receiver_type(defs: &[Def], receiver: &str, offset: usize) -> Option<Type> {
        if receiver == "self" {
            return defs
                .iter()
                .filter(|d| {
                    d.kind == DefKind::Class
                        && d.scope.is_some_and(|(s, e)| s <= offset && offset <= e)
                })
                // Innermost enclosing class wins.
                .max_by_key(|d| d.scope.map(|(s, _)| s))
                .map(|d| Type::Named(d.name.clone()));
        }
        Self::find_def(defs, receiver, offset).and_then(|d| d.ty.clone())
    }

    /// Members reachable on `ty`: built-ins plus every indexed field/method
    /// owned by it, walking the `extends` chain for classes.
    fn members_of(defs: &[Def], ty: &Type) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        let mut seen: Vec<String> = Vec::new();

        for (name, detail) in builtin_methods(ty) {
            seen.push(name.clone());
            items.push(CompletionItem {
                label: name,
                kind: Some(CompletionItemKind::METHOD),
                detail: Some(detail),
                ..Default::default()
            });
        }

        let mut owner = Some(type_str(ty));
        let mut guard = 0;
        while let Some(key) = owner.take() {
            // Cyclic `extends` is a type error, not a reason to hang the server.
            guard += 1;
            if guard > 16 {
                break;
            }
            for d in defs.iter().filter(|d| d.owner.as_deref() == Some(key.as_str())) {
                if seen.contains(&d.name) {
                    continue;
                }
                seen.push(d.name.clone());
                items.push(completion_item(d));
            }
            owner = defs
                .iter()
                .find(|d| d.kind == DefKind::Class && d.name == key)
                .and_then(|d| d.extends.clone());
        }
        items
    }
}

fn completion_kind(k: DefKind) -> CompletionItemKind {
    match k {
        DefKind::Var => CompletionItemKind::VARIABLE,
        DefKind::Param => CompletionItemKind::VARIABLE,
        DefKind::Func => CompletionItemKind::FUNCTION,
        DefKind::Method => CompletionItemKind::METHOD,
        DefKind::Class => CompletionItemKind::CLASS,
        DefKind::Interface => CompletionItemKind::INTERFACE,
        DefKind::Enum => CompletionItemKind::ENUM,
        DefKind::Variant => CompletionItemKind::ENUM_MEMBER,
        DefKind::Field => CompletionItemKind::FIELD,
        DefKind::TypeAlias => CompletionItemKind::STRUCT,
    }
}

fn completion_item(d: &Def) -> CompletionItem {
    CompletionItem {
        label: d.name.clone(),
        kind: Some(completion_kind(d.kind)),
        detail: Some(d.detail.clone()),
        ..Default::default()
    }
}

fn simple_item(label: &str, kind: CompletionItemKind, detail: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(kind),
        detail: Some(detail.to_string()),
        ..Default::default()
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> LspResult<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "sirin-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    // `.` opens member completion; the client re-requests on
                    // every identifier character on its own.
                    trigger_characters: Some(vec![".".into()]),
                    ..Default::default()
                }),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "sirin-lsp pronto")
            .await;
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.on_change(params.text_document.uri, params.text_document.text)
            .await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        // FULL sync: the last change carries the whole document.
        if let Some(change) = params.content_changes.pop() {
            self.on_change(params.text_document.uri, change.text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.docs.lock().await.remove(&params.text_document.uri);
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let docs = self.docs.lock().await;
        let Some(text) = docs.get(&uri) else { return Ok(None); };

        let offset = position_to_offset(text, pos);
        let Some((word, wstart, wend)) = word_at(text, offset) else { return Ok(None); };

        let analysis = analyze(text, uri.to_file_path().ok().as_deref());
        let Some(def) = Self::find_def(&analysis.defs, &word, offset) else {
            return Ok(None);
        };

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("```sirin\n{}\n```", def.detail),
            }),
            range: Some(span_to_range(text, wstart, wend)),
        }))
    }

    async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let docs = self.docs.lock().await;
        let Some(text) = docs.get(&uri) else { return Ok(None); };

        let offset = position_to_offset(text, pos);
        let analysis = analyze_for_completion(text, uri.to_file_path().ok().as_deref(), offset);

        // `receiver.` — members only. An unresolved receiver returns nothing
        // rather than the global list, which would just be noise after a dot.
        if let Some(receiver) = dot_receiver(text, offset) {
            let items = match Self::receiver_type(&analysis.defs, &receiver, offset) {
                Some(ty) => Self::members_of(&analysis.defs, &ty),
                None => vec![],
            };
            return Ok(Some(CompletionResponse::Array(items)));
        }

        let mut items = Vec::new();
        // Keyed by the client-visible kind, so a variable and a parameter of the
        // same name collapse into one entry instead of two identical rows.
        let mut seen: Vec<(String, CompletionItemKind)> = Vec::new();
        for d in &analysis.defs {
            // Fields and methods need a receiver, so they are not in scope here.
            if d.owner.is_some() {
                continue;
            }
            // A name declared later in the file is still worth offering:
            // functions and classes are routinely used above their definition.
            let key = (d.name.clone(), completion_kind(d.kind));
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            items.push(completion_item(d));
        }

        for k in KEYWORDS {
            items.push(simple_item(k, CompletionItemKind::KEYWORD, "palavra-chave"));
        }
        for t in BUILTIN_TYPES {
            items.push(simple_item(t, CompletionItemKind::CLASS, "tipo embutido"));
        }
        for c in CONSTRUCTORS {
            items.push(simple_item(c, CompletionItemKind::CONSTRUCTOR, "construtor"));
        }

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let docs = self.docs.lock().await;
        let Some(text) = docs.get(&uri) else { return Ok(None); };

        let offset = position_to_offset(text, pos);
        let Some((word, _, _)) = word_at(text, offset) else { return Ok(None); };

        let analysis = analyze(text, uri.to_file_path().ok().as_deref());
        let Some(def) = Self::find_def(&analysis.defs, &word, offset) else {
            return Ok(None);
        };

        let location = match &def.file {
            None => Location {
                uri: uri.clone(),
                range: span_to_range(text, def.start, def.end),
            },
            Some(path) => {
                let Ok(dep_uri) = Url::from_file_path(path) else { return Ok(None); };
                let Ok(dep_text) = std::fs::read_to_string(path) else { return Ok(None); };
                Location {
                    uri: dep_uri,
                    range: span_to_range(&dep_text, def.start, def.end),
                }
            }
        };
        Ok(Some(GotoDefinitionResponse::Scalar(location)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "\
class Animal {
    nome: str
    fn descrever() -> str => nome
}

class Cachorro extends Animal {
    fn latir() -> str => \"au\"
}

a: Animal = Animal(\"Rex\")
v: Vec[u8] = Vec(10)
";

    fn labels(text: &str, receiver: &str, offset: usize) -> Vec<String> {
        let defs = analyze_for_completion(text, None, offset).defs;
        let ty = Backend::receiver_type(&defs, receiver, offset).expect("receiver type");
        Backend::members_of(&defs, &ty)
            .into_iter()
            .map(|i| i.label)
            .collect()
    }

    #[test]
    fn dot_receiver_finds_the_identifier_before_the_dot() {
        let src = "a.no";
        assert_eq!(dot_receiver(src, 4).as_deref(), Some("a"));
        assert_eq!(dot_receiver(src, 2).as_deref(), Some("a"));
        // Not a member position: no dot precedes the word.
        assert_eq!(dot_receiver("abc", 3), None);
        // A dot with no receiver (e.g. a float fragment) is not one either.
        assert_eq!(dot_receiver(".foo", 4), None);
    }

    #[test]
    fn members_include_inherited_ones() {
        let text = format!("{}c = Cachorro(\"Bob\")\nc.", SRC);
        let offset = text.len();
        let got = labels(&text, "c", offset);
        assert!(got.contains(&"latir".to_string()));
        assert!(got.contains(&"nome".to_string()), "herdado de Animal: {:?}", got);
        assert!(got.contains(&"descrever".to_string()));
    }

    #[test]
    fn members_of_builtin_collection_are_offered() {
        let text = format!("{}v.", SRC);
        let offset = text.len();
        let got = labels(&text, "v", offset);
        assert_eq!(got, vec!["push", "len", "free"]);
    }

    /// `x.` never parses, so the index has to survive a failed parse.
    #[test]
    fn completion_survives_the_syntax_error_it_is_triggered_by() {
        let text = format!("{}a.", SRC);
        let offset = text.len();
        assert!(!analyze(&text, None).parsed, "esperado erro de sintaxe");
        let got = labels(&text, "a", offset);
        assert!(got.contains(&"nome".to_string()), "{:?}", got);
    }

    #[test]
    fn self_resolves_to_the_enclosing_class() {
        let text = "class Animal {\n    nome: str\n    fn d() -> str {\n        self.\n    }\n}\n";
        let offset = text.find("self.").unwrap() + "self.".len();
        let got = labels(text, "self", offset);
        assert!(got.contains(&"nome".to_string()), "{:?}", got);
    }

    #[test]
    fn fields_are_not_offered_without_a_receiver() {
        let defs = analyze_for_completion(SRC, None, SRC.len()).defs;
        let field = defs.iter().find(|d| d.name == "nome").expect("campo nome");
        assert_eq!(field.owner.as_deref(), Some("Animal"));
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        docs: Mutex::new(HashMap::new()),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
