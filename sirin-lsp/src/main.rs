//! Sirin Language Server (stdio).
//!
//! v1 scope: diagnostics (parse + type errors) on open/change, hover (type or
//! signature of the symbol under the cursor), and go-to-definition — including
//! definitions in local modules reachable through `use`.
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

#[derive(Clone)]
struct Def {
    name: String,
    /// hover text, e.g. `fn area(f: Forma) -> float` or `raio: float`
    detail: String,
    /// byte range of the defining name token
    start: usize,
    end: usize,
    /// file the definition lives in (None = current document)
    file: Option<PathBuf>,
}

fn rough_expr_type(e: &sirin_parser::expr::Expr<'_>) -> Option<&'static str> {
    use sirin_parser::expr::Expr;
    match e {
        Expr::Int(_) => Some("int"),
        Expr::Float(_) => Some("float"),
        Expr::Str(_) => Some("str"),
        Expr::Boolean(_) => Some("bool"),
        _ => None,
    }
}

fn collect_defs(stmts: &[Spanned<Stmt<'_>>], file: Option<&PathBuf>, out: &mut Vec<Def>) {
    for s in stmts {
        collect_stmt_defs(&s.node, file, out);
    }
}

fn collect_stmt_defs(stmt: &Stmt<'_>, file: Option<&PathBuf>, out: &mut Vec<Def>) {
    let def = |name: &Spanned<&str>, detail: String| Def {
        name: name.node.to_string(),
        detail,
        start: name.span.start,
        end: name.span.end,
        file: file.cloned(),
    };
    match stmt {
        Stmt::Let { name, ty, rhs } | Stmt::CopyLet { name, ty, rhs } => {
            let detail = match ty {
                Some(t) => format!("{}: {}", name.node, type_str(t)),
                None => match rough_expr_type(&rhs.node) {
                    Some(t) => format!("{}: {}", name.node, t),
                    None => format!("{} (inferido)", name.node),
                },
            };
            out.push(def(name, detail));
        }
        Stmt::Fn { name, args, return_type, body, async_ } => {
            let sig = fn_signature(name.node, args, return_type);
            let sig = if *async_ { format!("async {}", sig) } else { sig };
            out.push(def(name, sig));
            for (aname, aty) in args {
                out.push(def(aname, format!("{}: {}", aname.node, type_str(aty))));
            }
            collect_defs(body, file, out);
        }
        Stmt::AbstractFn { name, args, return_type } => {
            out.push(def(name, format!("abstract {}", fn_signature(name.node, args, return_type))));
        }
        Stmt::If { then, else_, .. } => {
            collect_defs(then, file, out);
            if let Some(e) = else_ { collect_defs(e, file, out); }
        }
        Stmt::IfLet { name, then, else_, .. } => {
            out.push(def(name, format!("{} (binding)", name.node)));
            collect_defs(then, file, out);
            if let Some(e) = else_ { collect_defs(e, file, out); }
        }
        Stmt::TryAssign { name, .. } => {
            out.push(def(name, format!("{} (Ok binding)", name.node)));
        }
        Stmt::While { body, .. } => collect_defs(body, file, out),
        Stmt::For { var, body, .. } => {
            out.push(def(var, format!("{}: int", var.node)));
            collect_defs(body, file, out);
        }
        Stmt::Match { arms, .. } => {
            for arm in arms {
                for b in &arm.binds {
                    out.push(def(b, format!("{} (payload)", b.node)));
                }
                collect_defs(&arm.body, file, out);
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
            out.push(def(name, format!("enum {} {{ {} }}", name.node, vs)));
            for (v, tys) in variants {
                let d = if tys.is_empty() {
                    format!("{}.{}", name.node, v.node)
                } else {
                    format!("{}.{}({})", name.node, v.node,
                        tys.iter().map(type_str).collect::<Vec<_>>().join(", "))
                };
                out.push(def(v, d));
            }
        }
        Stmt::Class { name, fields, methods, .. } => {
            out.push(def(name, format!("class {}", name.node)));
            for f in fields {
                out.push(def(&f.name, format!("{}: {}", f.name.node, type_str(&f.ty))));
            }
            collect_defs(methods, file, out);
        }
        Stmt::Interface { name, .. } => {
            out.push(def(name, format!("interface {}", name.node)));
        }
        Stmt::Impl { methods, .. } => collect_defs(methods, file, out),
        Stmt::Init { args, body } => {
            for (aname, aty) in args {
                out.push(def(aname, format!("{}: {}", aname.node, type_str(aty))));
            }
            collect_defs(body, file, out);
        }
        Stmt::Default { body } | Stmt::Spawn { body } => collect_defs(body, file, out),
        Stmt::TypeAlias { name, ty } => {
            out.push(def(name, format!("type {} = {}", name.node, type_str(ty))));
        }
        _ => {}
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

// ── Analysis (parse + typecheck + index) ──────────────────────────────────────

struct Analysis {
    diagnostics: Vec<Diagnostic>,
    /// defs in this document plus defs from `use`d local modules
    defs: Vec<Def>,
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
            return Analysis { diagnostics, defs };
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
            collect_defs(&mstmts, Some(&mp), &mut defs);
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

    collect_defs(&stmts, None, &mut defs);
    Analysis { diagnostics, defs }
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
