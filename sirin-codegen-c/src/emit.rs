use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use sirin_parser::{
    expr::{BinOp, Expr},
    span::Spanned,
    stmt::{ImplTarget, Stmt},
    types::Type,
};

pub struct Emitter {
    output: String,
    depth: usize,
    vars: HashMap<String, Type>,
    fns: HashMap<String, Type>,
    current_fn: Option<String>,
    current_fn_params: Vec<String>,
    current_class: Option<String>,
    /// constructor context: field_name → full C lhs ("self.nome" | "self.base.nome")
    class_field_paths: HashMap<String, String>,
    /// method context: field_name → full C access ("self->nome" | "self->base.nome")
    method_field_paths: HashMap<String, String>,
    /// class name → ordered (field_name, field_type) — includes inherited fields
    classes: HashMap<String, Vec<(String, Type)>>,
    /// class name → parent class name
    class_parents: HashMap<String, String>,
    /// class name → method name → return type
    class_methods: HashMap<String, HashMap<String, Type>>,
    /// #define names for each collection type used in emitted output
    used_types: RefCell<HashSet<String>>,
    /// primitive type key (e.g. "int") → set of impl'd method names
    prim_methods: HashMap<String, HashSet<String>>,
    /// true when `use sirin.io` was seen
    io_imported: bool,
    /// true when `use sirin.async` was seen
    async_imported: bool,
    /// set of async fn names declared in this scope
    async_fns: HashSet<String>,
    /// async fn name → ordered (param_name, param_type)
    async_fn_params: HashMap<String, Vec<(String, Type)>>,
    /// counter for unique spawn function/struct names
    spawn_count: usize,
    /// top-level C declarations for spawn helper structs and functions
    spawn_decls: String,
}

// True if any Stmt::Return in stmts (recursively) has value = Expr::Call(fn_name, _)
fn has_tail_call<'a>(stmts: &[Spanned<Stmt<'a>>], fn_name: &str) -> bool {
    stmts.iter().any(|s| match &s.node {
        Stmt::Return { value: Some(v), .. } => {
            matches!(&v.node, Expr::Call(name, _) if *name == fn_name)
        }
        Stmt::If { then, else_, .. } => {
            has_tail_call(then, fn_name)
                || else_.as_deref().map_or(false, |e| has_tail_call(e, fn_name))
        }
        _ => false,
    })
}

// True if expr contains any call to fn_name anywhere in the tree
fn expr_has_recursive_call<'a>(expr: &Expr<'a>, fn_name: &str) -> bool {
    match expr {
        Expr::Call(name, args) => {
            *name == fn_name || args.iter().any(|a| expr_has_recursive_call(&a.node, fn_name))
        }
        Expr::BinOp(_, lhs, rhs) => {
            expr_has_recursive_call(&lhs.node, fn_name)
                || expr_has_recursive_call(&rhs.node, fn_name)
        }
        Expr::Neg(inner) | Expr::Not(inner) => expr_has_recursive_call(&inner.node, fn_name),
        Expr::Array(items) => items.iter().any(|i| expr_has_recursive_call(&i.node, fn_name)),
        Expr::Index(base, idx) => {
            expr_has_recursive_call(&base.node, fn_name)
                || expr_has_recursive_call(&idx.node, fn_name)
        }
        Expr::MethodCall(obj, _, args) => {
            expr_has_recursive_call(&obj.node, fn_name)
                || args.iter().any(|a| expr_has_recursive_call(&a.node, fn_name))
        }
        _ => false,
    }
}

// Err if any non-tail recursive call to fn_name exists in stmts
fn check_no_non_tail_recursive<'a>(
    stmts: &[Spanned<Stmt<'a>>],
    fn_name: &str,
) -> Result<(), String> {
    let err = || {
        format!(
            "recursion in '{}' is not in tail position — Sirin does not support recursion without TCO",
            fn_name
        )
    };
    for s in stmts {
        match &s.node {
            Stmt::Return { value: Some(v), cond } => {
                // Recursive call in condition is never tail position
                if let Some(c) = cond {
                    if expr_has_recursive_call(&c.node, fn_name) {
                        return Err(err());
                    }
                }
                match &v.node {
                    // Top-level call to self: tail position — but args must not recurse
                    Expr::Call(name, args) if *name == fn_name => {
                        for arg in args {
                            if expr_has_recursive_call(&arg.node, fn_name) {
                                return Err(err());
                            }
                        }
                    }
                    // Anything else that recurses = non-tail
                    other => {
                        if expr_has_recursive_call(other, fn_name) {
                            return Err(err());
                        }
                    }
                }
            }
            Stmt::Return { value: None, cond } => {
                if let Some(c) = cond {
                    if expr_has_recursive_call(&c.node, fn_name) {
                        return Err(err());
                    }
                }
            }
            Stmt::If { cond, then, else_ } => {
                if expr_has_recursive_call(&cond.node, fn_name) {
                    return Err(err());
                }
                check_no_non_tail_recursive(then, fn_name)?;
                if let Some(els) = else_ {
                    check_no_non_tail_recursive(els, fn_name)?;
                }
            }
            Stmt::Let { rhs, .. } | Stmt::CopyLet { rhs, .. } => {
                if expr_has_recursive_call(&rhs.node, fn_name) {
                    return Err(err());
                }
            }
            Stmt::Expr(e) => {
                if expr_has_recursive_call(&e.node, fn_name) {
                    return Err(err());
                }
            }
            Stmt::Fn { .. } => {} // nested fn has its own scope
            _ => {}
        }
    }
    Ok(())
}

/// Returns (CamelCase, snake_case) suffix for a collection element/key/val type.
/// Used to construct C struct names (SirinVecU8) and function names (sirin_vec_u8_new).
fn collection_suffix(ty: &Type) -> (&'static str, &'static str) {
    match ty {
        Type::Int | Type::I64 => ("Int",   "int"),
        Type::U8              => ("U8",    "u8"),
        Type::U16             => ("U16",   "u16"),
        Type::U32             => ("U32",   "u32"),
        Type::U64             => ("U64",   "u64"),
        Type::I8              => ("I8",    "i8"),
        Type::I16             => ("I16",   "i16"),
        Type::I32             => ("I32",   "i32"),
        Type::Float           => ("Float", "float"),
        Type::Str             => ("Str",   "str"),
        Type::Bool            => ("Bool",  "bool"),
        _                     => ("Int",   "int"),
    }
}

fn integer_rank(ty: &Type) -> u8 {
    match ty {
        Type::U8  => 1,
        Type::I8  => 2,
        Type::U16 => 3,
        Type::I16 => 4,
        Type::U32 => 5,
        Type::I32 => 6,
        Type::U64 => 7,
        Type::I64 | Type::Int => 8,
        _ => 0,
    }
}

/// Maps a Sirin collection type to its conditional-compilation define name.
fn ty_to_define(ty: &Type) -> Option<&'static str> {
    match ty {
        Type::Vec(inner) => Some(match collection_suffix(inner).0 {
            "Int"   => "SIRIN_USE_VEC_INT",
            "U8"    => "SIRIN_USE_VEC_U8",
            "U16"   => "SIRIN_USE_VEC_U16",
            "U32"   => "SIRIN_USE_VEC_U32",
            "U64"   => "SIRIN_USE_VEC_U64",
            "I8"    => "SIRIN_USE_VEC_I8",
            "I16"   => "SIRIN_USE_VEC_I16",
            "I32"   => "SIRIN_USE_VEC_I32",
            "I64"   => "SIRIN_USE_VEC_I64",
            "Float" => "SIRIN_USE_VEC_FLOAT",
            "Bool"  => "SIRIN_USE_VEC_BOOL",
            "Str"   => "SIRIN_USE_VEC_STR",
            _       => return None,
        }),
        Type::Array(inner) => Some(match collection_suffix(inner).0 {
            "Int"   => "SIRIN_USE_ARRAY_INT",
            "U8"    => "SIRIN_USE_ARRAY_U8",
            "U16"   => "SIRIN_USE_ARRAY_U16",
            "U32"   => "SIRIN_USE_ARRAY_U32",
            "U64"   => "SIRIN_USE_ARRAY_U64",
            "I8"    => "SIRIN_USE_ARRAY_I8",
            "I16"   => "SIRIN_USE_ARRAY_I16",
            "I32"   => "SIRIN_USE_ARRAY_I32",
            "I64"   => "SIRIN_USE_ARRAY_I64",
            "Float" => "SIRIN_USE_ARRAY_FLOAT",
            "Bool"  => "SIRIN_USE_ARRAY_BOOL",
            _       => return None,
        }),
        Type::Set(inner) => Some(match collection_suffix(inner).0 {
            "Int"   => "SIRIN_USE_SET_INT",
            "U8"    => "SIRIN_USE_SET_U8",
            "U16"   => "SIRIN_USE_SET_U16",
            "U32"   => "SIRIN_USE_SET_U32",
            "U64"   => "SIRIN_USE_SET_U64",
            "I8"    => "SIRIN_USE_SET_I8",
            "I16"   => "SIRIN_USE_SET_I16",
            "I32"   => "SIRIN_USE_SET_I32",
            "I64"   => "SIRIN_USE_SET_I64",
            "Float" => "SIRIN_USE_SET_FLOAT",
            "Bool"  => "SIRIN_USE_SET_BOOL",
            _       => return None,
        }),
        Type::Map(_, val) => Some(match collection_suffix(val).0 {
            "Int"   => "SIRIN_USE_MAP_STR_INT",
            "Str"   => "SIRIN_USE_MAP_STR_STR",
            "Float" => "SIRIN_USE_MAP_STR_FLOAT",
            _       => return None,
        }),
        _ => None,
    }
}

/// Symbols exported from a compiled module, absorbed by the main-program emitter.
pub struct ModuleExports {
    pub fns: HashMap<String, Type>,
    pub classes: HashMap<String, Vec<(String, Type)>>,
    pub class_methods: HashMap<String, HashMap<String, Type>>,
    pub prim_methods: HashMap<String, HashSet<String>>,
    pub used_types: HashSet<String>,
    pub io_imported: bool,
}

impl Emitter {
    pub fn new() -> Self {
        Self {
            output: String::new(),
            depth: 0,
            vars: HashMap::new(),
            fns: HashMap::new(),
            current_fn: None,
            current_fn_params: Vec::new(),
            current_class: None,
            class_field_paths: HashMap::new(),
            method_field_paths: HashMap::new(),
            classes: HashMap::new(),
            class_parents: HashMap::new(),
            class_methods: HashMap::new(),
            used_types: RefCell::new(HashSet::new()),
            prim_methods: HashMap::new(),
            io_imported: false,
            async_imported: false,
            async_fns: HashSet::new(),
            async_fn_params: HashMap::new(),
            spawn_count: 0,
            spawn_decls: String::new(),
        }
    }

    /// Create a child emitter for emitting spawn function bodies.
    /// Inherits type information but starts with fresh output/vars.
    fn fork_for_spawn(&self) -> Self {
        Self {
            output: String::new(),
            depth: 1,
            vars: HashMap::new(),
            fns: self.fns.clone(),
            current_fn: None,
            current_fn_params: Vec::new(),
            current_class: None,
            class_field_paths: HashMap::new(),
            method_field_paths: HashMap::new(),
            classes: self.classes.clone(),
            class_parents: self.class_parents.clone(),
            class_methods: self.class_methods.clone(),
            used_types: RefCell::new(self.used_types.borrow().clone()),
            prim_methods: self.prim_methods.clone(),
            io_imported: self.io_imported,
            async_imported: true,
            async_fns: self.async_fns.clone(),
            async_fn_params: self.async_fn_params.clone(),
            spawn_count: self.spawn_count,
            spawn_decls: String::new(),
        }
    }

    /// Merge exported symbols from a module into this emitter before emitting the main program.
    pub fn absorb_exports(&mut self, exports: &ModuleExports) {
        for (k, v) in &exports.fns {
            self.fns.insert(k.clone(), v.clone());
        }
        for (k, v) in &exports.classes {
            self.classes.insert(k.clone(), v.clone());
        }
        for (k, v) in &exports.class_methods {
            self.class_methods.insert(k.clone(), v.clone());
        }
        for (k, v) in &exports.prim_methods {
            self.prim_methods.entry(k.clone()).or_default().extend(v.clone());
        }
        self.used_types.borrow_mut().extend(exports.used_types.clone());
        if exports.io_imported {
            self.io_imported = true;
        }
    }

    /// Emit only the functions/classes from a module (no `main`, no `#include`).
    /// Returns `(c_body, exports)` where exports can be absorbed by the main emitter.
    pub fn emit_module<'a>(mut self, stmts: &'a [Spanned<Stmt<'a>>]) -> (String, ModuleExports) {
        let mut class_stmts = vec![];
        let mut fn_stmts = vec![];

        for s in stmts {
            match &s.node {
                Stmt::Use { path } => {
                    if path.join(".") == "sirin.io" {
                        self.io_imported = true;
                    }
                }
                Stmt::Class { .. } | Stmt::Impl { .. } => class_stmts.push(s),
                Stmt::Interface { name, .. } => {
                    self.output.push_str(&format!("/* interface {} */\n\n", name.node));
                }
                Stmt::Fn { .. } => fn_stmts.push(s),
                _ => {}
            }
        }

        for s in &class_stmts { self.emit_stmt(&s.node); }
        for s in &fn_stmts    { self.emit_stmt(&s.node); }

        let exports = ModuleExports {
            fns: self.fns.iter()
                .filter(|(k, _)| !k.starts_with('_'))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            classes: self.classes.clone(),
            class_methods: self.class_methods.clone(),
            prim_methods: self.prim_methods.clone(),
            used_types: self.used_types.borrow().clone(),
            io_imported: self.io_imported,
        };

        (self.output, exports)
    }

    /// Like `emit_program_and_prefix` but returns raw `(body, defines_prefix, io_imported)`.
    /// Lets the caller inject module code between the includes and main body.
    pub fn emit_body_and_prefix<'a>(
        mut self,
        stmts: &'a [Spanned<Stmt<'a>>],
    ) -> (String, String, bool, bool) {
        self.emit_top_level(stmts);
        let prefix = self.defines_prefix();
        (self.output, prefix, self.io_imported, self.async_imported)
    }

} // end impl Emitter (helpers below)

fn collect_expr_vars<'a>(expr: &Expr<'a>, out: &mut std::collections::HashSet<String>) {
    match expr {
        Expr::Var(n) => { out.insert(n.to_string()); }
        Expr::Call(_, args) | Expr::New(_, args) => {
            for a in args { collect_expr_vars(&a.node, out); }
        }
        Expr::MethodCall(obj, _, args) => {
            collect_expr_vars(&obj.node, out);
            for a in args { collect_expr_vars(&a.node, out); }
        }
        Expr::BinOp(_, l, r) => {
            collect_expr_vars(&l.node, out);
            collect_expr_vars(&r.node, out);
        }
        Expr::Neg(e) | Expr::Not(e) | Expr::Await(e) => collect_expr_vars(&e.node, out),
        Expr::Index(b, i) => {
            collect_expr_vars(&b.node, out);
            collect_expr_vars(&i.node, out);
        }
        Expr::FieldAccess(obj, _) => collect_expr_vars(&obj.node, out),
        Expr::Array(items) => { for i in items { collect_expr_vars(&i.node, out); } }
        Expr::NewFields(_, fields) => {
            for (_, v) in fields { collect_expr_vars(&v.node, out); }
        }
        _ => {}
    }
}

fn collect_stmt_vars<'a>(stmt: &Stmt<'a>, out: &mut std::collections::HashSet<String>) {
    match stmt {
        Stmt::Let { rhs, .. } | Stmt::CopyLet { rhs, .. } => collect_expr_vars(&rhs.node, out),
        Stmt::Expr(e) => collect_expr_vars(&e.node, out),
        Stmt::Return { value, cond } => {
            if let Some(v) = value { collect_expr_vars(&v.node, out); }
            if let Some(c) = cond { collect_expr_vars(&c.node, out); }
        }
        Stmt::If { cond, then, else_ } => {
            collect_expr_vars(&cond.node, out);
            for s in then { collect_stmt_vars(&s.node, out); }
            if let Some(els) = else_ {
                for s in els { collect_stmt_vars(&s.node, out); }
            }
        }
        Stmt::Fn { body, .. } => {
            for s in body { collect_stmt_vars(&s.node, out); }
        }
        _ => {}
    }
}

fn collect_body_vars<'a>(stmts: &[sirin_parser::span::Spanned<Stmt<'a>>]) -> std::collections::HashSet<String> {
    let mut vars = std::collections::HashSet::new();
    for s in stmts { collect_stmt_vars(&s.node, &mut vars); }
    vars
}

/// Emit sirin_yield() before an expression if it contains .await.
fn expr_has_await(expr: &Expr<'_>) -> bool {
    match expr {
        Expr::Await(_) => true,
        Expr::MethodCall(obj, _, args) => {
            expr_has_await(&obj.node) || args.iter().any(|a| expr_has_await(&a.node))
        }
        Expr::BinOp(_, l, r) => expr_has_await(&l.node) || expr_has_await(&r.node),
        Expr::Neg(e) | Expr::Not(e) => expr_has_await(&e.node),
        Expr::Index(b, i) => expr_has_await(&b.node) || expr_has_await(&i.node),
        Expr::Array(items) => items.iter().any(|i| expr_has_await(&i.node)),
        Expr::Call(_, args) => args.iter().any(|a| expr_has_await(&a.node)),
        _ => false,
    }
}

/// Returns (void_cast, typed_cast) for a Channel inner type.
fn channel_cast(ty: &Type) -> (&'static str, &'static str) {
    match ty {
        Type::Str             => ("(void*)", "(const char*)"),
        Type::Float           => ("(void*)(uintptr_t)(uint64_t)", "(double)(uint64_t)(uintptr_t)"),
        Type::Bool            => ("(void*)(intptr_t)", "(int)(intptr_t)"),
        Type::U8 | Type::U16 | Type::U32 | Type::U64
                              => ("(void*)(uintptr_t)", "(uint64_t)(uintptr_t)"),
        _                     => ("(void*)(intptr_t)", "(int64_t)(intptr_t)"),
    }
}

impl Emitter {
    /// Emit an auto-spawn for `async_fn_name(args).await`.
    /// Builds a struct + wrapper fn in spawn_decls, then emits sirin_spawn inline.
    fn emit_async_spawn<'a>(&mut self, fn_name: &str, call_args: &[sirin_parser::span::Spanned<Expr<'a>>]) {
        let id = self.spawn_count;
        self.spawn_count += 1;

        let params = self.async_fn_params.get(fn_name).cloned().unwrap_or_default();
        let struct_name = format!("_AsyncArgs_{}", id);
        let wrapper_name = format!("_async_fn_{}", id);

        let mut decl = String::new();
        decl.push_str(&format!("typedef struct {{\n"));
        for (pname, ty) in &params {
            decl.push_str(&format!("    {} {};\n", self.type_to_c(ty), pname));
        }
        decl.push_str(&format!("}} {};\n\n", struct_name));

        decl.push_str(&format!("static void {}(void* _arg) {{\n", wrapper_name));
        if !params.is_empty() {
            decl.push_str(&format!("    {}* _s = ({}*)_arg;\n", struct_name, struct_name));
        }
        let call_params = params.iter().map(|(p, _)| format!("_s->{}", p)).collect::<Vec<_>>().join(", ");
        decl.push_str(&format!("    {}({});\n", fn_name, call_params));
        if !params.is_empty() {
            decl.push_str("    free(_arg);\n");
        }
        decl.push_str("}\n\n");
        self.spawn_decls.push_str(&decl);

        if params.is_empty() {
            self.output.push_str(&format!("{}sirin_spawn({}, NULL);\n", self.indent(), wrapper_name));
        } else {
            let arg_vals: Vec<String> = call_args.iter().map(|a| self.emit_expr(&a.node)).collect();
            self.output.push_str(&format!(
                "{}{}* _as_{} = ({}*)malloc(sizeof({}));\n",
                self.indent(), struct_name, id, struct_name, struct_name
            ));
            for (i, (pname, _)) in params.iter().enumerate() {
                self.output.push_str(&format!(
                    "{}_as_{}->{} = {};\n",
                    self.indent(), id, pname, arg_vals[i]
                ));
            }
            self.output.push_str(&format!(
                "{}sirin_spawn({}, _as_{});\n",
                self.indent(), wrapper_name, id
            ));
        }
    }

    /// Returns (fn_prefix, c_self_type) for an ImplTarget.
    fn impl_target_info(target: &ImplTarget) -> (&'static str, &'static str) {
        match target {
            ImplTarget::Named(_)          => ("", ""),  // handled separately
            ImplTarget::Int | ImplTarget::I64 => ("int",   "int64_t"),
            ImplTarget::Float             => ("float", "double"),
            ImplTarget::Str               => ("str",   "const char*"),
            ImplTarget::Bool              => ("bool",  "int"),
            ImplTarget::U8                => ("u8",    "uint8_t"),
            ImplTarget::U16               => ("u16",   "uint16_t"),
            ImplTarget::U32               => ("u32",   "uint32_t"),
            ImplTarget::U64               => ("u64",   "uint64_t"),
            ImplTarget::I8                => ("i8",    "int8_t"),
            ImplTarget::I16               => ("i16",   "int16_t"),
            ImplTarget::I32               => ("i32",   "int32_t"),
        }
    }

    pub fn emit_stmt<'a>(&mut self, stmt: &'a Stmt<'a>) {
        match stmt {
            Stmt::Let { name, ty: declared, rhs } | Stmt::CopyLet { name, ty: declared, rhs } => {
                // yield before await expressions in let rhs
                if expr_has_await(&rhs.node) {
                    self.output.push_str(&format!("{}sirin_yield();\n", self.indent()));
                }
                // Inside a constructor body, assignments to class fields become <path> = val
                if let Some(path) = self.class_field_paths.get(name.node).cloned() {
                    let val = self.emit_expr(&rhs.node);
                    self.output.push_str(&format!("{}{} = {};\n", self.indent(), path, val));
                    return;
                }

                let inferred = self.get_expr(&rhs.node);
                let ty = declared.as_ref().unwrap_or(&inferred);

                // Array literal [a, b, c]:
                //   Array[T] → stack: T name[N] = {v1, v2, ...};
                //   Vec[T]   → dynamic: sirin_vec_T_new + repeated push
                if let Expr::Array(items) = &rhs.node {
                    self.vars.insert(name.node.to_string(), ty.clone());
                    match ty {
                        Type::Array(inner) => {
                            let c_elem = self.type_to_c(inner);
                            let vals = items.iter()
                                .map(|i| self.emit_expr(&i.node))
                                .collect::<Vec<_>>()
                                .join(", ");
                            self.output.push_str(&format!(
                                "{}{} {}[{}] = {{{}}};\n",
                                self.indent(), c_elem, name.node, items.len(), vals
                            ));
                        }
                        Type::Vec(inner) => {
                            let (_, snake) = collection_suffix(inner);
                            let c_ty = self.type_to_c(ty);
                            self.output.push_str(&format!(
                                "{}{} {} = sirin_vec_{}_new({});\n",
                                self.indent(), c_ty, name.node, snake, items.len()
                            ));
                            for item in items {
                                let val = self.emit_expr(&item.node);
                                self.output.push_str(&format!(
                                    "{}sirin_vec_{}_push(&{}, {});\n",
                                    self.indent(), snake, name.node, val
                                ));
                            }
                        }
                        _ => {
                            // fallback: treat as Array[int]
                            let vals = items.iter()
                                .map(|i| self.emit_expr(&i.node))
                                .collect::<Vec<_>>()
                                .join(", ");
                            self.output.push_str(&format!(
                                "{}int64_t {}[{}] = {{{}}};\n",
                                self.indent(), name.node, items.len(), vals
                            ));
                        }
                    }
                    return;
                }

                // Collection constructor (Vec/Array/Map/Set) with declared type
                // → use type-specific constructor instead of generic fallback
                if let Expr::Call(ctor, ctor_args) = &rhs.node {
                    let line: Option<String> = match (*ctor, ty) {
                        ("Vec", Type::Vec(inner)) => {
                            let (_, snake) = collection_suffix(inner);
                            let cap = ctor_args.first()
                                .map(|a| self.emit_expr(&a.node))
                                .unwrap_or_else(|| "4".to_string());
                            Some(format!(
                                "{}{} {} = sirin_vec_{}_new({});\n",
                                self.indent(), self.type_to_c(ty), name.node, snake, cap
                            ))
                        }
                        ("Array", Type::Array(inner)) => {
                            let (_, snake) = collection_suffix(inner);
                            let cap = ctor_args.first()
                                .map(|a| self.emit_expr(&a.node))
                                .unwrap_or_else(|| "4".to_string());
                            Some(format!(
                                "{}{} {} = sirin_array_{}_new({});\n",
                                self.indent(), self.type_to_c(ty), name.node, snake, cap
                            ))
                        }
                        ("Map", Type::Map(_, val)) => {
                            let (_, vs) = collection_suffix(val);
                            Some(format!(
                                "{}{} {} = sirin_map_str_{}_new();\n",
                                self.indent(), self.type_to_c(ty), name.node, vs
                            ))
                        }
                        ("Set", Type::Set(inner)) => {
                            let (_, snake) = collection_suffix(inner);
                            Some(format!(
                                "{}{} {} = sirin_set_{}_new();\n",
                                self.indent(), self.type_to_c(ty), name.node, snake
                            ))
                        }
                        _ => None,
                    };
                    if let Some(l) = line {
                        self.vars.insert(name.node.to_string(), ty.clone());
                        self.output.push_str(&l);
                        return;
                    }
                }

                let value = self.emit_expr(&rhs.node);
                self.vars.insert(name.node.to_string(), ty.clone());
                self.output.push_str(&format!(
                    "{}{} {} = {};\n",
                    self.indent(),
                    self.type_to_c(ty),
                    name.node,
                    value
                ));
            }
            Stmt::Return { cond, value } => {
                // Detect TCO: return value is a direct call to the current function
                let tco_new_vals: Option<Vec<String>> =
                    match (value, self.current_fn.as_deref()) {
                        (Some(v), Some(fn_name)) => match &v.node {
                            Expr::Call(name, args) if *name == fn_name => {
                                Some(args.iter().map(|a| self.emit_expr(&a.node)).collect())
                            }
                            _ => None,
                        },
                        _ => None,
                    };

                if let Some(new_vals) = tco_new_vals {
                    let params = self.current_fn_params.clone();
                    if let Some(c) = cond {
                        let cond_str = self.emit_expr(&c.node);
                        self.output
                            .push_str(&format!("{}if ({}) {{\n", self.indent(), cond_str));
                        self.depth += 1;
                        for (p, v) in params.iter().zip(new_vals.iter()) {
                            self.output
                                .push_str(&format!("{}{} = {};\n", self.indent(), p, v));
                        }
                        self.output
                            .push_str(&format!("{}goto inicio;\n", self.indent()));
                        self.depth -= 1;
                        self.output.push_str(&format!("{}}}\n", self.indent()));
                    } else {
                        for (p, v) in params.iter().zip(new_vals.iter()) {
                            self.output
                                .push_str(&format!("{}{} = {};\n", self.indent(), p, v));
                        }
                        self.output
                            .push_str(&format!("{}goto inicio;\n", self.indent()));
                    }
                } else {
                    match (cond, value) {
                        (Some(c), Some(v)) => {
                            let cond_str = self.emit_expr(&c.node);
                            let val_str = self.emit_expr(&v.node);
                            self.output.push_str(&format!(
                                "{}if ({}) return {};\n",
                                self.indent(),
                                cond_str,
                                val_str
                            ));
                        }
                        (Some(c), None) => {
                            let cond_str = self.emit_expr(&c.node);
                            self.output.push_str(&format!(
                                "{}if ({}) return;\n",
                                self.indent(),
                                cond_str
                            ));
                        }
                        (None, Some(v)) => {
                            let val_str = self.emit_expr(&v.node);
                            self.output
                                .push_str(&format!("{}return {};\n", self.indent(), val_str));
                        }
                        (None, None) => {
                            self.output.push_str(&format!("{}return;\n", self.indent()));
                        }
                    }
                }
            }
            Stmt::Fn {
                name,
                args,
                return_type,
                body,
                async_,
                ..
            } => {
                let fn_name = name.node;

                if *async_ {
                    self.async_fns.insert(fn_name.to_string());
                    self.async_fn_params.insert(
                        fn_name.to_string(),
                        args.iter().map(|(pname, ty)| (pname.node.to_string(), ty.clone())).collect(),
                    );
                }

                if let Err(e) = check_no_non_tail_recursive(body, fn_name) {
                    eprintln!("error: {}", e);
                    std::process::exit(1);
                }

                let tco = has_tail_call(body, fn_name);

                let ret_ty = return_type.clone().unwrap_or(Type::Void);
                let ret = self.type_to_c(&ret_ty);
                self.fns.insert(fn_name.to_string(), ret_ty);

                let params = args
                    .iter()
                    .map(|(pname, ty)| format!("{} {}", self.type_to_c(ty), pname.node))
                    .collect::<Vec<_>>()
                    .join(", ");

                self.output.push_str(&format!(
                    "{}{} {}({}) {{\n",
                    self.indent(),
                    ret,
                    fn_name,
                    params
                ));

                self.depth += 1;

                let outer_vars = self.vars.clone();
                let outer_fn = self.current_fn.take();
                let outer_params = std::mem::take(&mut self.current_fn_params);

                if tco {
                    self.output.push_str(&format!("{}inicio:\n", self.indent()));
                    self.current_fn = Some(fn_name.to_string());
                    self.current_fn_params =
                        args.iter().map(|(pname, _)| pname.node.to_string()).collect();
                }

                for (pname, ty) in args {
                    self.vars.insert(pname.node.to_string(), ty.clone());
                }
                for s in body {
                    self.emit_stmt(&s.node);
                }

                self.vars = outer_vars;
                self.current_fn = outer_fn;
                self.current_fn_params = outer_params;
                self.depth -= 1;

                self.output.push_str(&format!("{}}}\n", self.indent()));
            }
            Stmt::If { cond, then, else_ } => {
                let cond_str = self.emit_expr(&cond.node);
                self.output
                    .push_str(&format!("{}if ({}) {{\n", self.indent(), cond_str));

                self.depth += 1;
                for s in then {
                    self.emit_stmt(&s.node);
                }
                self.depth -= 1;

                if let Some(else_body) = else_ {
                    self.output
                        .push_str(&format!("{}}} else {{\n", self.indent()));
                    self.depth += 1;
                    for s in else_body {
                        self.emit_stmt(&s.node);
                    }
                    self.depth -= 1;
                }

                self.output.push_str(&format!("{}}}\n", self.indent()));
            }
            Stmt::Expr(expr) => {
                // async fn call.await → auto-spawn the fn as a coroutine
                if let Expr::Await(inner) = &expr.node {
                    if let Expr::Call(name, call_args) = &inner.node {
                        if self.async_fns.contains(*name) {
                            self.emit_async_spawn(name, call_args);
                            return;
                        }
                    }
                }
                // yield before await expressions at statement level
                if expr_has_await(&expr.node) {
                    self.output.push_str(&format!("{}sirin_yield();\n", self.indent()));
                }
                let val = self.emit_expr(&expr.node);
                self.output.push_str(&format!("{}{};\n", self.indent(), val));
            }

            Stmt::Spawn { body } => {
                let id = self.spawn_count;
                self.spawn_count += 1;

                // Collect vars referenced in body that exist in enclosing scope
                let used = collect_body_vars(body);
                let mut captured: Vec<(String, Type)> = used
                    .iter()
                    .filter_map(|v| self.vars.get(v.as_str()).map(|t| (v.clone(), t.clone())))
                    .collect();
                captured.sort_by(|a, b| a.0.cmp(&b.0));

                let struct_name = format!("_SpawnArgs_{}", id);
                let fn_name = format!("_spawn_fn_{}", id);

                // Build struct + fn source using a child emitter for the body
                let mut decl = String::new();
                decl.push_str(&format!("typedef struct {{\n"));
                for (name, ty) in &captured {
                    decl.push_str(&format!("    {} {};\n", self.type_to_c(ty), name));
                }
                decl.push_str(&format!("}} {};\n\n", struct_name));

                decl.push_str(&format!("static void {}(void* _arg) {{\n", fn_name));
                if !captured.is_empty() {
                    decl.push_str(&format!("    {}* _args = ({}*)_arg;\n", struct_name, struct_name));
                    for (name, ty) in &captured {
                        decl.push_str(&format!("    {} {} = _args->{};\n", self.type_to_c(ty), name, name));
                    }
                }

                // Emit body using a child emitter that has the captured vars in scope
                let mut sub = self.fork_for_spawn();
                for (name, ty) in &captured {
                    sub.vars.insert(name.clone(), ty.clone());
                }
                for s in body {
                    sub.emit_stmt(&s.node);
                }
                // Propagate used_types back
                let sub_used = sub.used_types.into_inner();
                self.used_types.borrow_mut().extend(sub_used);
                decl.push_str(&sub.output);
                decl.push_str("}\n\n");

                self.spawn_decls.push_str(&decl);

                // Inline spawn call in the enclosing scope (main or fn body)
                if captured.is_empty() {
                    self.output.push_str(&format!(
                        "{}sirin_spawn({}, NULL);\n",
                        self.indent(),
                        fn_name
                    ));
                } else {
                    self.output.push_str(&format!(
                        "{}{}* _args_{} = ({}*)malloc(sizeof({}));\n",
                        self.indent(), struct_name, id, struct_name, struct_name
                    ));
                    for (name, _) in &captured {
                        self.output.push_str(&format!(
                            "{}_args_{}->{} = {};\n",
                            self.indent(), id, name, name
                        ));
                    }
                    self.output.push_str(&format!(
                        "{}sirin_spawn({}, _args_{});\n",
                        self.indent(), fn_name, id
                    ));
                }
            }

            Stmt::Class { name, abstract_, extends, is_, fields, methods } => {
                let cls = name.node;

                // Own fields only (for struct body)
                let own_fields: Vec<(String, Type)> = fields.iter()
                    .map(|f| (f.name.node.to_string(), f.ty.clone()))
                    .collect();

                // Full field list: inherited first, then own — for type resolution
                let inherited_fields: Vec<(String, Type)> = extends
                    .as_ref()
                    .and_then(|p| self.classes.get(p.node).cloned())
                    .unwrap_or_default();
                let all_fields: Vec<(String, Type)> = inherited_fields.iter()
                    .chain(own_fields.iter())
                    .cloned()
                    .collect();
                self.classes.insert(cls.to_string(), all_fields.clone());

                if let Some(p) = extends {
                    self.class_parents.insert(cls.to_string(), p.node.to_string());
                }

                // Build field path maps for constructor and method contexts
                let mut ctor_paths: HashMap<String, String> = HashMap::new();
                let mut meth_paths: HashMap<String, String> = HashMap::new();
                for (fname, _) in &own_fields {
                    ctor_paths.insert(fname.clone(), format!("self.{}", fname));
                    meth_paths.insert(fname.clone(), format!("self->{}", fname));
                }
                for (fname, _) in &inherited_fields {
                    ctor_paths.insert(fname.clone(), format!("self.base.{}", fname));
                    meth_paths.insert(fname.clone(), format!("self->base.{}", fname));
                }

                // Emit typedef struct
                self.output.push_str("typedef struct {\n");
                if let Some(p) = extends {
                    self.output.push_str(&format!("    {} base;\n", p.node));
                }
                for f in fields {
                    self.output.push_str(&format!("    {} {};\n", self.type_to_c(&f.ty), f.name.node));
                }
                self.output.push_str(&format!("}} {};\n\n", cls));

                // Register method return types
                let mut meth_ret: HashMap<String, Type> = HashMap::new();
                for m in methods {
                    if let Stmt::Fn { name: mn, return_type, .. } = &m.node {
                        meth_ret.insert(mn.node.to_string(), return_type.clone().unwrap_or(Type::Void));
                    }
                }
                self.class_methods.insert(cls.to_string(), meth_ret);

                self.current_class = Some(cls.to_string());

                // Emit each method
                for m in methods {
                    match &m.node {
                        Stmt::Default { body } => {
                            if *abstract_ { continue; }
                            self.output.push_str(&format!("{} {}_default(void) {{\n", cls, cls));
                            self.depth += 1;
                            self.output.push_str(&format!("{}{} self = {{0}};\n", self.indent(), cls));
                            self.class_field_paths = ctor_paths.clone();
                            for s in body { self.emit_stmt(&s.node); }
                            self.class_field_paths.clear();
                            self.output.push_str(&format!("{}return self;\n", self.indent()));
                            self.depth -= 1;
                            self.output.push_str("}\n\n");
                        }
                        Stmt::Init { args, body } => {
                            if *abstract_ { continue; }
                            let params = args.iter()
                                .map(|(n, t)| format!("{} {}", self.type_to_c(t), n.node))
                                .collect::<Vec<_>>().join(", ");
                            self.output.push_str(&format!("{} {}_new({}) {{\n", cls, cls, params));
                            self.depth += 1;
                            self.output.push_str(&format!("{}{} self = {{0}};\n", self.indent(), cls));
                            self.class_field_paths = ctor_paths.clone();
                            for s in body { self.emit_stmt(&s.node); }
                            self.class_field_paths.clear();
                            self.output.push_str(&format!("{}return self;\n", self.indent()));
                            self.depth -= 1;
                            self.output.push_str("}\n\n");
                        }
                        Stmt::Fn { name: mname, args, return_type, body, .. } => {
                            let ret = return_type.as_ref().map_or("void".to_string(), |t| self.type_to_c(t));
                            let extra_params = args.iter()
                                .map(|(n, t)| format!("{} {}", self.type_to_c(t), n.node))
                                .collect::<Vec<_>>().join(", ");
                            let full_params = if extra_params.is_empty() {
                                format!("{}* self", cls)
                            } else {
                                format!("{}* self, {}", cls, extra_params)
                            };
                            self.output.push_str(&format!("{} {}_{}({}) {{\n", ret, cls, mname.node, full_params));
                            self.vars.insert("self".to_string(), Type::Named(cls.to_string()));
                            self.method_field_paths = meth_paths.clone();
                            self.depth += 1;
                            for s in body { self.emit_stmt(&s.node); }
                            self.depth -= 1;
                            self.method_field_paths.clear();
                            self.vars.remove("self");
                            self.output.push_str("}\n\n");
                        }
                        Stmt::AbstractFn { .. } => {} // no body
                        _ => {}
                    }
                }

                self.current_class = None;

                // Interface vtables — emit comments for now
                for iface in is_ {
                    self.output.push_str(&format!("/* {} implements {} */\n\n", cls, iface.node));
                }
            }

            Stmt::Impl { target, methods } => {
                match target {
                    ImplTarget::Named(cls) => {
                        // Methods for a named class — identical to class-body methods
                        let cls_str = cls.to_string();
                        let meth_paths: HashMap<String, String> = self.classes
                            .get(*cls)
                            .map(|fields| {
                                fields.iter().map(|(fname, _)| {
                                    (fname.clone(), format!("self->{}", fname))
                                }).collect()
                            })
                            .unwrap_or_default();

                        for m in methods {
                            if let Stmt::Fn { name: mname, args, return_type, body, .. } = &m.node {
                                let ret = return_type.as_ref().map_or("void".to_string(), |t| self.type_to_c(t));
                                let extra_params = args.iter()
                                    .map(|(n, t)| format!("{} {}", self.type_to_c(t), n.node))
                                    .collect::<Vec<_>>().join(", ");
                                let full_params = if extra_params.is_empty() {
                                    format!("{}* self", cls_str)
                                } else {
                                    format!("{}* self, {}", cls_str, extra_params)
                                };
                                self.output.push_str(&format!("{} {}_{}({}) {{\n", ret, cls_str, mname.node, full_params));
                                self.vars.insert("self".to_string(), Type::Named(cls_str.clone()));
                                self.method_field_paths = meth_paths.clone();
                                self.depth += 1;
                                for s in body { self.emit_stmt(&s.node); }
                                self.depth -= 1;
                                self.method_field_paths.clear();
                                self.vars.remove("self");
                                self.output.push_str("}\n\n");
                            }
                        }
                    }
                    _ => {
                        let (prefix, c_self_ty) = Self::impl_target_info(target);
                        for m in methods {
                            if let Stmt::Fn { name: mname, args, return_type, body, .. } = &m.node {
                                let ret = return_type.as_ref().map_or("void".to_string(), |t| self.type_to_c(t));
                                let extra_params = args.iter()
                                    .map(|(n, t)| format!("{} {}", self.type_to_c(t), n.node))
                                    .collect::<Vec<_>>().join(", ");
                                let full_params = if extra_params.is_empty() {
                                    format!("{} self", c_self_ty)
                                } else {
                                    format!("{} self, {}", c_self_ty, extra_params)
                                };
                                self.output.push_str(&format!(
                                    "{} {}_{}({}) {{\n",
                                    ret, prefix, mname.node, full_params
                                ));
                                // register self type so body expressions resolve correctly
                                let self_ty = self.prim_prefix_to_type(prefix);
                                self.vars.insert("self".to_string(), self_ty);
                                self.depth += 1;
                                for s in body { self.emit_stmt(&s.node); }
                                self.depth -= 1;
                                self.vars.remove("self");
                                self.output.push_str("}\n\n");

                                // track for call-site routing
                                self.prim_methods
                                    .entry(prefix.to_string())
                                    .or_default()
                                    .insert(mname.node.to_string());
                            }
                        }
                    }
                }
            }

            _ => {}
        }
    }

    fn prim_prefix_to_type(&self, prefix: &str) -> Type {
        match prefix {
            "int"   => Type::Int,
            "float" => Type::Float,
            "str"   => Type::Str,
            "bool"  => Type::Bool,
            "u8"    => Type::U8,
            "u16"   => Type::U16,
            "u32"   => Type::U32,
            "u64"   => Type::U64,
            "i8"    => Type::I8,
            "i16"   => Type::I16,
            "i32"   => Type::I32,
            "i64"   => Type::I64,
            _       => Type::Void,
        }
    }

    fn emit_expr<'a>(&self, expr: &'a Expr<'a>) -> String {
        match expr {
            Expr::Await(inner) => self.emit_expr(&inner.node),
            Expr::Int(x) => format!("{}", x),
            Expr::Float(x) => format!("{}", x),
            Expr::Boolean(x) => (if *x { "1" } else { "0" }).to_string(),
            Expr::Str(x) => format!("\"{}\"", x),
            Expr::Var(name) => {
                if let Some(path) = self.method_field_paths.get(*name) {
                    path.clone()
                } else {
                    name.to_string()
                }
            }
            Expr::Call(name, args) => {
                match *name {
                    "print" | "println" => {
                        let newline = *name == "println";
                        if let Some(arg) = args.first() {
                            let arg_str = self.emit_expr(&arg.node);
                            let arg_ty = self.get_expr(&arg.node);
                            let (fmt, needs_addr) = match &arg_ty {
                                Type::Str                         => ("%s", false),
                                Type::Int | Type::I64             => ("%lld", false),
                                Type::U8 | Type::U16 | Type::U32  => ("%u", false),
                                Type::U64                         => ("%llu", false),
                                Type::I8 | Type::I16 | Type::I32  => ("%d", false),
                                Type::Float                       => ("%f", false),
                                Type::Bool                        => ("%d", false),
                                Type::Named(_)                    => ("%p", true),
                                _                                 => ("%d", false),
                            };
                            let fmt_str = if newline {
                                format!("{}\\n", fmt)
                            } else {
                                fmt.to_string()
                            };
                            let arg_c = if needs_addr {
                                format!("&{}", arg_str)
                            } else {
                                arg_str
                            };
                            format!("printf(\"{}\", {})", fmt_str, arg_c)
                        } else if newline {
                            "printf(\"\\n\")".to_string()
                        } else {
                            "printf(\"\")".to_string()
                        }
                    }
                    "readln" => "sirin_readln()".to_string(),
                    _ => {
                        let args_str = args
                            .iter()
                            .map(|a| self.emit_expr(&a.node))
                            .collect::<Vec<_>>()
                            .join(", ");
                        // Fallback constructors when no declared type is available.
                        match *name {
                            "Vec"   => format!("sirin_vec_int_new({})", args_str),
                            "Array" => format!("sirin_array_int_new({})", args_str),
                            "Map"   => "sirin_map_str_int_new()".to_string(),
                            "Set"   => "sirin_set_int_new()".to_string(),
                            _       => format!("{}({})", name, args_str),
                        }
                    }
                }
            }
            Expr::Array(items) => {
                // Reached only outside a typed-let (rare); emit placeholder
                let inner = items.iter().map(|i| self.emit_expr(&i.node)).collect::<Vec<_>>().join(", ");
                format!("/* array: [{}] */", inner)
            }
            Expr::Index(base, idx) => {
                let b = self.emit_expr(&base.node);
                let i = self.emit_expr(&idx.node);
                match self.get_expr(&base.node) {
                    Type::Vec(inner) => {
                        let (_, s) = collection_suffix(&inner);
                        format!("sirin_vec_{}_get(&{}, {})", s, b, i)
                    }
                    // Array is stack-allocated; use C direct subscript
                    Type::Array(_) => format!("{}[{}]", b, i),
                    _ => format!("{}[{}]", b, i),
                }
            }
            Expr::MethodCall(obj, method, args) => {
                let obj_str = self.emit_expr(&obj.node);
                let obj_ty  = self.get_expr(&obj.node);
                let args_str = args
                    .iter()
                    .map(|a| self.emit_expr(&a.node))
                    .collect::<Vec<_>>()
                    .join(", ");
                match obj_ty {
                    Type::Channel(inner) => match *method {
                        "send" => {
                            let (to_void, _) = channel_cast(&inner);
                            let val = args_str;
                            format!("sirin_channel_send({}, {}{})", obj_str, to_void, val)
                        }
                        "recv" => {
                            let (_, from_void) = channel_cast(&inner);
                            format!("{}sirin_channel_recv({})", from_void, obj_str)
                        }
                        "free" => format!("sirin_channel_free({})", obj_str),
                        _ => format!("{}.{}({})", obj_str, method, args_str),
                    },
                    Type::Vec(inner) => {
                        let (_, s) = collection_suffix(&inner);
                        match *method {
                            "push" => format!("sirin_vec_{}_push(&{}, {})", s, obj_str, args_str),
                            _      => format!("sirin_vec_{}_get(&{}, {})",  s, obj_str, args_str),
                        }
                    }
                    Type::Array(inner) => {
                        let (_, s) = collection_suffix(&inner);
                        match *method {
                            "push" => format!("sirin_array_{}_push(&{}, {})", s, obj_str, args_str),
                            _      => format!("sirin_array_{}_get(&{}, {})",  s, obj_str, args_str),
                        }
                    }
                    Type::Map(_, val) => {
                        let (_, vs) = collection_suffix(&val);
                        match *method {
                            "insert" => format!("sirin_map_str_{}_insert(&{}, {})", vs, obj_str, args_str),
                            _        => format!("sirin_map_str_{}_get(&{}, {})",    vs, obj_str, args_str),
                        }
                    }
                    Type::Set(inner) => {
                        let (_, s) = collection_suffix(&inner);
                        match *method {
                            "insert"   => format!("sirin_set_{}_insert(&{}, {})",   s, obj_str, args_str),
                            "contains" => format!("sirin_set_{}_contains(&{}, {})", s, obj_str, args_str),
                            _          => format!("{}.{}({})", obj_str, method, args_str),
                        }
                    }
                    Type::Named(cls) => {
                        // self is already a pointer in method context; other objects need &
                        let is_self_ptr = matches!(&obj.node, Expr::Var("self"))
                            && !self.method_field_paths.is_empty();
                        let receiver = if is_self_ptr {
                            obj_str.clone()
                        } else {
                            format!("&{}", obj_str)
                        };
                        if args_str.is_empty() {
                            format!("{}_{}({})", cls, method, receiver)
                        } else {
                            format!("{}_{}({}, {})", cls, method, receiver, args_str)
                        }
                    }
                    _ => {
                        // Check primitive impl methods registered via `impl T`
                        let prefix = match &obj_ty {
                            Type::Int | Type::I64 => Some("int"),
                            Type::Float           => Some("float"),
                            Type::Str             => Some("str"),
                            Type::Bool            => Some("bool"),
                            Type::U8              => Some("u8"),
                            Type::U16             => Some("u16"),
                            Type::U32             => Some("u32"),
                            Type::U64             => Some("u64"),
                            Type::I8              => Some("i8"),
                            Type::I16             => Some("i16"),
                            Type::I32             => Some("i32"),
                            _ => None,
                        };
                        if let Some(pfx) = prefix {
                            if self.prim_methods.get(pfx).map_or(false, |s| s.contains(*method)) {
                                return if args_str.is_empty() {
                                    format!("{}_{}({})", pfx, method, obj_str)
                                } else {
                                    format!("{}_{} ({}, {})", pfx, method, obj_str, args_str)
                                };
                            }
                        }
                        format!("{}.{}({})", obj_str, method, args_str)
                    }
                }
            }
            Expr::FieldAccess(obj, field) => {
                match &obj.node {
                    // Inside a method, self is a pointer — use the path map to handle inheritance
                    Expr::Var("self") if !self.method_field_paths.is_empty() => {
                        if let Some(path) = self.method_field_paths.get(*field) {
                            path.clone()
                        } else {
                            format!("self->{}", field)
                        }
                    }
                    _ => format!("{}.{}", self.emit_expr(&obj.node), field),
                }
            }
            Expr::New(name, args) => {
                if *name == "Channel" { return "sirin_channel_new()".to_string(); }
                let args_str = args.iter().map(|a| self.emit_expr(&a.node)).collect::<Vec<_>>().join(", ");
                format!("{}_new({})", name, args_str)
            }
            Expr::NewDefault(name) => {
                if *name == "Channel" { return "sirin_channel_new()".to_string(); }
                format!("{}_default()", name)
            }
            Expr::NewFields(name, fields) => {
                let inits = fields.iter()
                    .map(|(fname, fval)| format!(".{} = {}", fname, self.emit_expr(&fval.node)))
                    .collect::<Vec<_>>().join(", ");
                format!("({}){{ {} }}", name, inits)
            }
            Expr::Neg(expr) => format!("(-{})", self.emit_expr(&expr.node)),
            Expr::Not(expr) => format!("(!{})", self.emit_expr(&expr.node)),
            Expr::BinOp(op, lhs, rhs) => {
                let l = self.emit_expr(&lhs.node);
                let r = self.emit_expr(&rhs.node);
                let op = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    BinOp::Eq => "==",
                    BinOp::NotEq => "!=",
                    BinOp::Lt => "<",
                    BinOp::Gt => ">",
                    BinOp::LtEq => "<=",
                    BinOp::GtEq => ">=",
                    BinOp::And => "&&",
                    BinOp::Or => "||",
                };
                format!("({} {} {})", l, op, r)
            }
        }
    }

    fn type_to_c(&self, ty: &Type) -> String {
        let name = match ty {
            Type::Int | Type::I64 => "int64_t".to_string(),
            Type::U64 => "uint64_t".to_string(),
            Type::I32 => "int32_t".to_string(),
            Type::U32 => "uint32_t".to_string(),
            Type::I16 => "int16_t".to_string(),
            Type::U16 => "uint16_t".to_string(),
            Type::I8 => "int8_t".to_string(),
            Type::U8 => "uint8_t".to_string(),
            Type::Float => "double".to_string(),
            Type::Str => "const char*".to_string(),
            Type::Bool => "int".to_string(),
            Type::Void => "void".to_string(),
            Type::Nullable(inner) => format!("{}*", self.type_to_c(inner)),
            Type::Vec(inner)   => format!("SirinVec{}", collection_suffix(inner).0),
            Type::Array(inner) => format!("SirinArray{}", collection_suffix(inner).0),
            Type::Set(inner)   => format!("SirinSet{}", collection_suffix(inner).0),
            Type::Map(_, val)  => format!("SirinMapStr{}", collection_suffix(val).0),
            Type::Named(n) => n.clone(),
            Type::Channel(_) => "SirinChannel*".to_string(),
        };
        if let Some(define) = ty_to_define(ty) {
            self.used_types.borrow_mut().insert(define.to_string());
        }
        name
    }

    fn get_expr<'a>(&self, expr: &'a Expr<'a>) -> Type {
        match expr {
            Expr::Await(inner) => self.get_expr(&inner.node),
            Expr::Int(_) => Type::Int,
            Expr::Float(_) => Type::Float,
            Expr::Str(_) => Type::Str,
            Expr::Boolean(_) => Type::Bool,
            Expr::Var(name) => self.vars.get(*name).cloned().unwrap_or(Type::Int),
            Expr::Neg(inner) => self.get_expr(&inner.node),
            Expr::Not(_) => Type::Bool,
            Expr::BinOp(op, lhs, rhs) => match op {
                BinOp::Eq
                | BinOp::NotEq
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::LtEq
                | BinOp::GtEq
                | BinOp::And
                | BinOp::Or => Type::Bool,
                _ => {
                    let l = self.get_expr(&lhs.node);
                    let r = self.get_expr(&rhs.node);
                    // integer promotion: wider type wins
                    if l.is_integer() && r.is_integer() && integer_rank(&r) > integer_rank(&l) {
                        r
                    } else {
                        l
                    }
                }
            },
            Expr::Call(name, _) => match *name {
                "print" | "println" => Type::Void,
                "readln"            => Type::Str,
                _                   => self.fns.get(*name).cloned().unwrap_or(Type::Void),
            },
            Expr::Array(items) => {
                let inner = items.first().map_or(Type::Int, |i| self.get_expr(&i.node));
                Type::Array(Box::new(inner))
            }
            Expr::Index(base, _) => match self.get_expr(&base.node) {
                Type::Array(inner) | Type::Vec(inner) => *inner,
                other => other,
            },
            Expr::MethodCall(obj, method, _) => {
                let obj_ty = self.get_expr(&obj.node);
                match &obj_ty {
                    Type::Channel(inner) => match *method {
                        "send" | "free" => Type::Void,
                        "recv" => *inner.clone(),
                        _ => Type::Void,
                    },
                    Type::Vec(inner) | Type::Array(inner) => {
                        if *method == "push" { Type::Void } else { *inner.clone() }
                    }
                    Type::Map(_, v) => {
                        if *method == "insert" { Type::Void } else { *v.clone() }
                    }
                    Type::Set(_) => match *method {
                        "insert" => Type::Void,
                        "contains" => Type::Bool,
                        _ => Type::Void,
                    },
                    Type::Named(cls) => {
                        self.class_methods
                            .get(cls.as_str())
                            .and_then(|m| m.get(*method))
                            .cloned()
                            .unwrap_or(Type::Void)
                    }
                    _ => Type::Void,
                }
            }
            Expr::FieldAccess(obj, field) => {
                let obj_ty = self.get_expr(&obj.node);
                if let Type::Named(cls) = &obj_ty {
                    if let Some(fields) = self.classes.get(cls.as_str()) {
                        if let Some((_, ty)) = fields.iter().find(|(n, _)| n.as_str() == *field) {
                            return ty.clone();
                        }
                    }
                }
                Type::Void
            }
            Expr::New(name, _) | Expr::NewDefault(name) | Expr::NewFields(name, _) => {
                Type::Named(name.to_string())
            }
        }
    }

    pub fn emit_program<'a>(mut self, stmts: &'a [Spanned<Stmt<'a>>]) -> String {
        self.emit_top_level(stmts);
        self.finish()
    }

    /// Like `emit_program` but also returns the defines prefix so the caller
    /// can prepend it to the runtime source before separate compilation.
    pub fn emit_program_and_prefix<'a>(mut self, stmts: &'a [Spanned<Stmt<'a>>]) -> (String, String) {
        self.emit_top_level(stmts);
        let prefix = self.defines_prefix();
        let io = if self.io_imported { "#include <stdio.h>\n" } else { "" };
        let async_h = if self.async_imported { "#include \"sirin_async.h\"\n#include <stdlib.h>\n" } else { "" };
        let program = format!("{}#include \"sirin_runtime.h\"\n{}{}\n{}", prefix, async_h, io, self.output);
        (program, prefix)
    }

    pub fn emit_program_tcc<'a>(mut self, stmts: &'a [Spanned<Stmt<'a>>]) -> (String, String) {
        self.emit_top_level(stmts);
        let prefix = self.defines_prefix();
        let io = if self.io_imported { "#include <stdio.h>\n" } else { "" };
        let async_h = if self.async_imported { "#include \"sirin_async.h\"\n#include <stdlib.h>\n" } else { "" };
        let program = format!("{}typedef long long int64_t;\n{}{}\n{}", prefix, async_h, io, self.output);
        (program, prefix)
    }

    /// Emits function declarations at global scope and wraps all other
    /// top-level statements inside `int main(void)`.
    fn emit_top_level<'a>(&mut self, stmts: &'a [Spanned<Stmt<'a>>]) {
        let mut class_stmts = vec![];
        let mut fn_stmts    = vec![];
        let mut rest        = vec![];

        for s in stmts {
            match &s.node {
                Stmt::Use { path } => {
                    let m = path.join(".");
                    if m == "sirin.io"    { self.io_imported    = true; }
                    if m == "sirin.async" { self.async_imported = true; }
                }
                Stmt::Class { .. }     => class_stmts.push(s),
                Stmt::Impl { .. }      => class_stmts.push(s),
                Stmt::Interface { name, .. } => {
                    self.output.push_str(&format!("/* interface {} */\n\n", name.node));
                }
                Stmt::Fn { .. }        => fn_stmts.push(s),
                _                      => rest.push(s),
            }
        }

        for s in &class_stmts { self.emit_stmt(&s.node); }
        for s in &fn_stmts    { self.emit_stmt(&s.node); }

        // Remember where to insert spawn declarations (after fns, before main)
        let spawn_insert_pos = self.output.len();

        if !rest.is_empty() {
            if self.async_imported {
                self.output.push_str("int main(void) {\n    sirin_loop_init();\n");
            } else {
                self.output.push_str("int main(void) {\n");
            }
            self.depth += 1;
            for s in &rest { self.emit_stmt(&s.node); }
            self.depth -= 1;
            if self.async_imported {
                self.output.push_str("    sirin_loop_run();\n");
            }
            self.output.push_str("    return 0;\n}\n");
        }

        // Splice spawn declarations before main
        if !self.spawn_decls.is_empty() {
            let after = self.output[spawn_insert_pos..].to_string();
            self.output.truncate(spawn_insert_pos);
            self.output.push_str(&self.spawn_decls);
            self.output.push_str(&after);
            self.spawn_decls.clear();
        }
    }

    fn indent(&self) -> String {
        "    ".repeat(self.depth)
    }

    fn defines_prefix(&self) -> String {
        let mut v: Vec<String> = self.used_types.borrow().iter().cloned().collect();
        v.sort();
        v.iter().map(|d| format!("#define {}\n", d)).collect()
    }

    pub fn finish(self) -> String {
        let prefix = self.defines_prefix();
        let io = if self.io_imported { "#include <stdio.h>\n" } else { "" };
        let async_h = if self.async_imported { "#include \"sirin_async.h\"\n#include <stdlib.h>\n" } else { "" };
        format!("{}#include \"sirin_runtime.h\"\n{}{}\n{}", prefix, async_h, io, self.output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chumsky::Parser;
    use chumsky::input::Input as _;
    use chumsky::span::SimpleSpan;

    fn emit(src: &str) -> String {
        let tokens = sirin_parser::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = sirin_parser::parser::parser()
            .parse(tokens.as_slice().split_token_span(eoi))
            .into_result()
            .expect("parse failed");
        Emitter::new().emit_program(&stmts)
    }

    #[test]
    fn test_simple_class_method_pointer() {
        let c = emit("class Animal {\n    nome: str\n    init(n: str) { nome = n }\n    fn descrever() -> str => nome\n}");
        assert!(c.contains("Animal* self"), "method must take pointer self");
        assert!(c.contains("self->nome"), "field access must use ->");
        assert!(c.contains("Animal Animal_new("), "constructor returns by value");
    }

    #[test]
    fn test_inheritance_struct_has_base() {
        let c = emit(
            "class Animal {\n    nome: str\n    init(n: str) { nome = n }\n}\nclass Cachorro extends Animal {\n    raca: str\n    init(n: str, r: str) { nome = n\nraca = r }\n}"
        );
        assert!(c.contains("Animal base;"), "Cachorro struct must embed Animal base");
        assert!(c.contains("self.base.nome = n"), "inherited field assignment goes through base");
        assert!(c.contains("self.raca = r"), "own field assignment is direct");
    }

    #[test]
    fn test_method_call_takes_address() {
        let c = emit(
            "class Animal {\n    nome: str\n    init(n: str) { nome = n }\n    fn descrever() -> str => nome\n}\na: Animal = Animal(\"Rex\")\nb: str = a.descrever()"
        );
        assert!(c.contains("Animal_descrever(&a)"), "method call on local var must pass &var");
    }

    #[test]
    fn test_interface_emits_comment() {
        let c = emit("interface Corredor {\n    fn correr() -> str\n}");
        assert!(c.contains("/* interface Corredor */"), "interface must emit a comment");
    }

    #[test]
    fn test_impl_int_dobrar_eh_par() {
        let c = emit(
            "impl int {\n    fn dobrar() -> int => self * 2\n    fn eh_par() -> bool => self == 0\n}"
        );
        assert!(c.contains("int64_t int_dobrar(int64_t self)"), "impl int fn must use int64_t self");
        assert!(c.contains("int int_eh_par(int64_t self)"),     "bool return maps to int in C");
    }

    #[test]
    fn test_impl_str_vazio() {
        let c = emit("impl str {\n    fn vazio() -> bool => self == 0\n}");
        assert!(c.contains("int str_vazio(const char* self)"), "impl str fn must use const char* self");
    }

    #[test]
    fn test_impl_named_adds_method() {
        let c = emit(
            "class Animal {\n    nome: str\n    init(n: str) { nome = n }\n}\nimpl Animal {\n    fn cumprimentar() -> str => nome\n}"
        );
        assert!(c.contains("const char* Animal_cumprimentar(Animal* self)"), "impl for named type emits pointer self");
        assert!(c.contains("self->nome"), "field access inside impl method uses ->");
    }

    #[test]
    fn test_prim_method_call_routes_correctly() {
        let c = emit(
            "impl int {\n    fn dobrar() -> int => self * 2\n}\nx: int = 5\ny: int = x.dobrar()"
        );
        assert!(c.contains("int_dobrar(x)"), "x.dobrar() must emit int_dobrar(x)");
    }

    #[test]
    fn test_class_is_emits_comment() {
        let c = emit(
            "interface Ave { fn voar() -> str }\nclass Pato implements Ave {\n    fn voar() -> str => \"bate asas\"\n}"
        );
        assert!(c.contains("/* Pato implements Ave */"), "implements clause must emit comment");
    }
}
