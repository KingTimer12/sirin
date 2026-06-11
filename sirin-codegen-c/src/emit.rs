use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::Hasher;

use sirin_parser::{
    expr::{BinOp, Expr},
    span::Spanned,
    stmt::{BindPattern, ImplTarget, Stmt},
    types::Type,
};

pub struct Emitter {
    output: String,
    depth: usize,
    vars: HashMap<String, Type>,
    fns: HashMap<String, Type>,
    current_fn: Option<String>,
    current_fn_params: Vec<String>,
    /// return type of the function whose body is currently being emitted — used to
    /// type `Ok`/`Err` literals and the `?=` Err-propagation `return`.
    current_ret: Option<Type>,
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
    /// true when `use sirin.net` was seen
    net_imported: bool,
    /// set of async fn names declared in this scope
    async_fns: HashSet<String>,
    /// async fn name → ordered (param_name, param_type)
    async_fn_params: HashMap<String, Vec<(String, Type)>>,
    /// counter for unique spawn function/struct names
    spawn_count: usize,
    /// top-level C declarations for spawn helper structs and functions
    spawn_decls: String,
    /// inline C declarations for named-type collections (Vec[ClassName], etc.)
    /// key = "Vec_ClassName", value = full C typedef + impl
    named_collection_decls: RefCell<HashMap<String, String>>,
    /// names of module-level (global) variables — emitted as file-scope C globals;
    /// assignments to these emit as `name = val` instead of redeclaring
    globals: HashSet<String>,
    /// resource locals/params in the current fn that own an external handle and
    /// need a Drop close-call (TcpStream/TcpListener/UdpSocket) on return paths.
    live_drops: Vec<(String, &'static str)>,
    /// heap-string locals in the current fn freed on return paths (Drop).
    live_heap_strs: Vec<String>,
    /// heap-string locals hoisted above a TCO loop label: declared once as NULL,
    /// reassigned (not redeclared) each iteration, previous buffer freed first.
    hoisted_heap_strs: HashSet<String>,
    /// vars whose ownership escapes the current fn body — never auto-dropped.
    current_escaped: HashSet<String>,
    /// resource `::clone` nodes hoisted to a named temp before the consuming
    /// statement. Keyed by the `Expr::Clone` node address → temp var name; the
    /// Clone emit arm returns the temp instead of re-emitting the clone call.
    clone_temps: RefCell<HashMap<usize, String>>,
    /// counter for synthetic resource-clone temp names with non-`Var` operands.
    clone_count: RefCell<usize>,
    /// clone-temp names already declared in the current fn (collision avoidance).
    clone_names: RefCell<HashSet<String>>,
}

/// Drop function for an external-resource type, or None for plain values.
fn resource_close_fn(ty: &Type) -> Option<&'static str> {
    match ty {
        Type::Named(n) => match n.as_str() {
            "TcpStream"   => Some("sirin_tcp_stream_close"),
            "TcpListener" => Some("sirin_tcp_listener_close"),
            "UdpSocket"   => Some("sirin_udp_socket_close"),
            _ => None,
        },
        _ => None,
    }
}

/// Runtime deep-clone function for a resource type (duplicates the OS handle),
/// or None for plain values. Resources wrap an fd, so a struct bit-copy would
/// alias the handle — `::clone`/`:=` must `dup()` it instead.
fn resource_clone_fn(ty: &Type) -> Option<&'static str> {
    match ty {
        Type::Named(n) => match n.as_str() {
            "TcpStream"   => Some("sirin_tcp_stream_clone"),
            "TcpListener" => Some("sirin_tcp_listener_clone"),
            _ => None,
        },
        _ => None,
    }
}

/// True if `e` is an addressable lvalue we can pass to a `*_clone(&x)` runtime fn.
fn is_addressable(e: &Expr<'_>) -> bool {
    matches!(e, Expr::Var(_) | Expr::FieldAccess(..) | Expr::Index(..))
}

/// Map a Sirin function name to the C identifier used to emit it.
///
/// A user function literally named `main` would clobber the program entry
/// point that codegen synthesizes as `int main(void)`. Rename it to a stable
/// `main_<hash>` so both can coexist (the synthesized entry then calls it).
fn c_fn_name(name: &str) -> String {
    if name == "main" {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        h.write(b"main");
        format!("main_{:016x}", h.finish())
    } else {
        name.to_string()
    }
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

fn to_snake_case(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 { out.push('_'); }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// Like collection_suffix but handles Type::Named by deriving Pascal/snake from the class name.
/// Unique, C-identifier-safe suffix for a type — used to monomorphize `Try[T]`
/// without collisions (`Try[int]` → `int`, `Try[int?]` → `opt_int`).
fn type_mangle(ty: &Type) -> String {
    match ty {
        Type::Nullable(t) => format!("opt_{}", type_mangle(t)),
        Type::Try(t)      => format!("try_{}", type_mangle(t)),
        Type::Vec(t)      => format!("vec_{}", type_mangle(t)),
        Type::Array(t)    => format!("arr_{}", type_mangle(t)),
        Type::Set(t)      => format!("set_{}", type_mangle(t)),
        Type::Map(k, v)   => format!("map_{}_{}", type_mangle(k), type_mangle(v)),
        Type::Channel(t)  => format!("chan_{}", type_mangle(t)),
        Type::Named(n)    => to_snake_case(n),
        Type::Void        => "void".to_string(),
        // Structural types hash their shape so distinct structs/signatures don't collide
        // (e.g. `Try[Request]` vs `Try[{method,path}]` must mangle differently).
        Type::Struct(fields) => {
            let mut sig = String::new();
            for (n, t) in fields {
                sig.push_str(n);
                sig.push(':');
                sig.push_str(&type_mangle(t));
                sig.push(';');
            }
            let mut h = std::collections::hash_map::DefaultHasher::new();
            h.write(sig.as_bytes());
            format!("obj_{:016x}", h.finish())
        }
        Type::Func(args, ret) => {
            let mut sig = String::new();
            for a in args {
                sig.push_str(&type_mangle(a));
                sig.push(',');
            }
            sig.push_str("->");
            sig.push_str(&type_mangle(ret));
            let mut h = std::collections::hash_map::DefaultHasher::new();
            h.write(sig.as_bytes());
            format!("fn_{:016x}", h.finish())
        }
        _                 => collection_suffix(ty).1.to_string(), // primitives
    }
}

fn collection_suffix_for(ty: &Type) -> (String, String) {
    if let Type::Named(name) = ty {
        return (name.clone(), to_snake_case(name));
    }
    let (a, b) = collection_suffix(ty);
    (a.to_string(), b.to_string())
}

fn named_vec_impl(pascal: &str, snake: &str, c_type: &str) -> String {
    format!(
        "typedef struct {{ {c}* ptr; size_t len; size_t cap; }} SirinVec{P};\n\
static SirinVec{P} sirin_vec_{s}_new(size_t ic) {{\n    size_t cap = ic > 0 ? ic : 4;\n\
    {c}* buf = ({c}*)malloc(cap * sizeof({c}));\n    if (!buf) {{ fprintf(stderr, \"sirin: out of memory\\n\"); exit(1); }}\n\
    SirinVec{P} v; v.ptr = buf; v.len = 0; v.cap = cap; return v;\n}}\n\
static void sirin_vec_{s}_push(SirinVec{P}* v, {c} val) {{\n\
    if (v->len == v->cap) {{ v->cap *= 2; v->ptr = ({c}*)realloc(v->ptr, v->cap * sizeof({c}));\n\
        if (!v->ptr) {{ fprintf(stderr, \"sirin: out of memory\\n\"); exit(1); }} }}\n\
    v->ptr[v->len++] = val;\n}}\n\
static {c} sirin_vec_{s}_get(SirinVec{P}* v, size_t i) {{\n\
    if (i >= v->len) {{ fprintf(stderr, \"sirin: vec index out of bounds\\n\"); exit(1); }}\n\
    return v->ptr[i];\n}}\n\
static void sirin_vec_{s}_free(SirinVec{P}* v) {{ free(v->ptr); v->ptr = NULL; v->len = 0; v->cap = 0; }}\n",
        P = pascal, s = snake, c = c_type
    )
}

fn named_array_impl(pascal: &str, snake: &str, c_type: &str) -> String {
    format!(
        "typedef struct {{ {c}* ptr; size_t len; size_t cap; }} SirinArray{P};\n\
static SirinArray{P} sirin_array_{s}_new(size_t ic) {{\n    size_t cap = ic > 0 ? ic : 4;\n\
    {c}* buf = ({c}*)malloc(cap * sizeof({c}));\n    if (!buf) {{ fprintf(stderr, \"sirin: out of memory\\n\"); exit(1); }}\n\
    SirinArray{P} v; v.ptr = buf; v.len = 0; v.cap = cap; return v;\n}}\n\
static void sirin_array_{s}_push(SirinArray{P}* v, {c} val) {{\n\
    if (v->len == v->cap) {{ v->cap *= 2; v->ptr = ({c}*)realloc(v->ptr, v->cap * sizeof({c}));\n\
        if (!v->ptr) {{ fprintf(stderr, \"sirin: out of memory\\n\"); exit(1); }} }}\n\
    v->ptr[v->len++] = val;\n}}\n\
static {c} sirin_array_{s}_get(SirinArray{P}* v, size_t i) {{\n\
    if (i >= v->len) {{ fprintf(stderr, \"sirin: array index out of bounds\\n\"); exit(1); }}\n\
    return v->ptr[i];\n}}\n\
static void sirin_array_{s}_free(SirinArray{P}* v) {{ free(v->ptr); v->ptr = NULL; v->len = 0; v->cap = 0; }}\n",
        P = pascal, s = snake, c = c_type
    )
}

fn named_set_impl(pascal: &str, snake: &str, c_type: &str) -> String {
    format!(
        "typedef struct {{ {c}* ptr; size_t len; size_t cap; }} SirinSet{P};\n\
static SirinSet{P} sirin_set_{s}_new(void) {{\n    size_t cap = 4;\n\
    {c}* p = ({c}*)malloc(cap * sizeof({c}));\n    if (!p) {{ fprintf(stderr, \"sirin: out of memory\\n\"); exit(1); }}\n\
    SirinSet{P} s; s.ptr = p; s.len = 0; s.cap = cap; return s;\n}}\n\
static int sirin_set_{s}_contains(SirinSet{P}* s, {c} val) {{\n\
    for (size_t i = 0; i < s->len; i++) if (memcmp(&s->ptr[i], &val, sizeof({c})) == 0) return 1;\n    return 0;\n}}\n\
static void sirin_set_{s}_insert(SirinSet{P}* s, {c} val) {{\n\
    if (sirin_set_{s}_contains(s, val)) return;\n\
    if (s->len == s->cap) {{ s->cap *= 2; s->ptr = ({c}*)realloc(s->ptr, s->cap * sizeof({c}));\n\
        if (!s->ptr) {{ fprintf(stderr, \"sirin: out of memory\\n\"); exit(1); }} }}\n\
    s->ptr[s->len++] = val;\n}}\n\
static void sirin_set_{s}_free(SirinSet{P}* s) {{ free(s->ptr); s->ptr = NULL; s->len = 0; s->cap = 0; }}\n",
        P = pascal, s = snake, c = c_type
    )
}

fn named_map_impl(pascal: &str, snake: &str, c_type: &str) -> String {
    format!(
        "typedef struct {{ char** keys; {c}* vals; size_t len; size_t cap; }} SirinMapStr{P};\n\
static SirinMapStr{P} sirin_map_str_{s}_new(void) {{\n    size_t cap = 4;\n\
    char** ks = (char**)malloc(cap * sizeof(char*));\n\
    {c}* vs = ({c}*)malloc(cap * sizeof({c}));\n\
    if (!ks || !vs) {{ fprintf(stderr, \"sirin: out of memory\\n\"); exit(1); }}\n\
    SirinMapStr{P} m; m.keys = ks; m.vals = vs; m.len = 0; m.cap = cap; return m;\n}}\n\
static void sirin_map_str_{s}_insert(SirinMapStr{P}* m, const char* key, {c} val) {{\n\
    for (size_t i = 0; i < m->len; i++) if (strcmp(m->keys[i], key) == 0) {{ m->vals[i] = val; return; }}\n\
    if (m->len == m->cap) {{ m->cap *= 2;\n\
        m->keys = (char**)realloc(m->keys, m->cap * sizeof(char*));\n\
        m->vals = ({c}*)realloc(m->vals, m->cap * sizeof({c}));\n\
        if (!m->keys || !m->vals) {{ fprintf(stderr, \"sirin: out of memory\\n\"); exit(1); }} }}\n\
    size_t kl = strlen(key); char* k = (char*)malloc(kl + 1); memcpy(k, key, kl); k[kl] = 0;\n\
    m->keys[m->len] = k; m->vals[m->len] = val; m->len++;\n}}\n\
static {c} sirin_map_str_{s}_get(SirinMapStr{P}* m, const char* key) {{\n\
    for (size_t i = 0; i < m->len; i++) if (strcmp(m->keys[i], key) == 0) return m->vals[i];\n\
    fprintf(stderr, \"sirin: map key not found\\n\"); exit(1);\n}}\n\
static void sirin_map_str_{s}_free(SirinMapStr{P}* m) {{\n\
    for (size_t i = 0; i < m->len; i++) free(m->keys[i]);\n\
    free(m->keys); free(m->vals); m->keys = NULL; m->vals = NULL; m->len = 0; m->cap = 0;\n}}\n",
        P = pascal, s = snake, c = c_type
    )
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
        Type::Vec(inner) | Type::Array(inner) | Type::Set(inner)
            if matches!(inner.as_ref(), Type::Named(_)) => return None,
        Type::Map(_, val)
            if matches!(val.as_ref(), Type::Named(_)) => return None,
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
    pub named_collection_decls: HashMap<String, String>,
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
            current_ret: None,
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
            net_imported: false,
            async_fns: HashSet::new(),
            async_fn_params: HashMap::new(),
            spawn_count: 0,
            spawn_decls: String::new(),
            named_collection_decls: RefCell::new(HashMap::new()),
            globals: HashSet::new(),
            live_drops: Vec::new(),
            live_heap_strs: Vec::new(),
            hoisted_heap_strs: HashSet::new(),
            current_escaped: HashSet::new(),
            clone_temps: RefCell::new(HashMap::new()),
            clone_count: RefCell::new(0),
            clone_names: RefCell::new(HashSet::new()),
        }
    }

    /// Create a child emitter for emitting spawn function bodies.
    /// Inherits type information but starts with fresh output/vars.
    fn fork_for_spawn(&self) -> Self {
        Self {
            output: String::new(),
            depth: 1,
            // seed with global var types so inference works inside spawn bodies
            vars: self.vars.iter()
                .filter(|(k, _)| self.globals.contains(*k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            fns: self.fns.clone(),
            current_fn: None,
            current_fn_params: Vec::new(),
            current_ret: None,
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
            net_imported: self.net_imported,
            async_fns: self.async_fns.clone(),
            async_fn_params: self.async_fn_params.clone(),
            spawn_count: self.spawn_count,
            spawn_decls: String::new(),
            named_collection_decls: RefCell::new(self.named_collection_decls.borrow().clone()),
            globals: self.globals.clone(),
            live_drops: Vec::new(),
            live_heap_strs: Vec::new(),
            hoisted_heap_strs: HashSet::new(),
            current_escaped: HashSet::new(),
            clone_temps: RefCell::new(HashMap::new()),
            clone_count: RefCell::new(0),
            clone_names: RefCell::new(HashSet::new()),
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
        self.named_collection_decls.borrow_mut().extend(exports.named_collection_decls.clone());
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
                    let m = path.join(".");
                    if m == "sirin.io"  { self.io_imported  = true; }
                    if m == "sirin.net" { self.net_imported  = true; }
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
            named_collection_decls: self.named_collection_decls.borrow().clone(),
            io_imported: self.io_imported,
        };

        (self.output, exports)
    }

    /// Like `emit_program_and_prefix` but returns raw `(body, defines_prefix, io_imported, async_imported, net_imported)`.
    /// Lets the caller inject module code between the includes and main body.
    pub fn emit_body_and_prefix<'a>(
        mut self,
        stmts: &'a [Spanned<Stmt<'a>>],
    ) -> (String, String, bool, bool, bool, String) {
        self.emit_top_level(stmts);
        let prefix = self.defines_prefix();
        let named_decls = self.named_collection_decls_string();
        (self.output, prefix, self.io_imported, self.async_imported, self.net_imported, named_decls)
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
        Expr::NewFields(_, fields) | Expr::ObjectLiteral(fields) => {
            for (_, v) in fields { collect_expr_vars(&v.node, out); }
        }
        Expr::Some(inner) | Expr::Ok(inner) | Expr::Err(inner) | Expr::Clone(inner) => {
            collect_expr_vars(&inner.node, out)
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
        Stmt::IfLet { expr, then, else_, .. } => {
            collect_expr_vars(&expr.node, out);
            for s in then { collect_stmt_vars(&s.node, out); }
            if let Some(els) = else_ {
                for s in els { collect_stmt_vars(&s.node, out); }
            }
        }
        Stmt::TryAssign { rhs, .. } => collect_expr_vars(&rhs.node, out),
        Stmt::Fn { body, .. } => {
            for s in body { collect_stmt_vars(&s.node, out); }
        }
        Stmt::Spawn { body } => {
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

/// Collect variable names whose ownership *escapes* the current scope — i.e. they
/// are moved into another binding/aggregate, returned, sent on a channel, or freed
/// manually. Such vars must NOT be auto-freed (would double-free / use-after-free).
/// Borrowing reads (function args, method receivers, printing) do NOT escape.
fn collect_escaped_expr<'a>(e: &Expr<'a>, out: &mut HashSet<String>) {
    let mark = |o: &mut HashSet<String>, x: &Expr<'a>| {
        if let Expr::Var(v) = x { o.insert(v.to_string()); }
    };
    match e {
        Expr::Array(items) => {
            for it in items { mark(out, &it.node); collect_escaped_expr(&it.node, out); }
        }
        Expr::ObjectLiteral(fs) | Expr::NewFields(_, fs) => {
            for (_, val) in fs { mark(out, &val.node); collect_escaped_expr(&val.node, out); }
        }
        Expr::New(_, args) => {
            for a in args { mark(out, &a.node); collect_escaped_expr(&a.node, out); }
        }
        Expr::MethodCall(obj, method, args) => {
            if *method == "free" { mark(out, &obj.node); }
            if matches!(*method, "push" | "insert" | "send") {
                for a in args { mark(out, &a.node); }
            }
            collect_escaped_expr(&obj.node, out);
            for a in args { collect_escaped_expr(&a.node, out); }
        }
        Expr::Call(_, args) => { for a in args { collect_escaped_expr(&a.node, out); } }
        // `Some(v)`/`Ok(v)`/`Err(v)` move v into the boxed/tagged value.
        Expr::Some(inner) | Expr::Ok(inner) | Expr::Err(inner) => {
            mark(out, &inner.node);
            collect_escaped_expr(&inner.node, out);
        }
        // `a::clone` reads a and makes a fresh copy — a is NOT moved.
        Expr::Clone(inner) => collect_escaped_expr(&inner.node, out),
        Expr::BinOp(_, l, r) => { collect_escaped_expr(&l.node, out); collect_escaped_expr(&r.node, out); }
        Expr::Neg(x) | Expr::Not(x) | Expr::Await(x) => collect_escaped_expr(&x.node, out),
        Expr::Index(b, i) => { collect_escaped_expr(&b.node, out); collect_escaped_expr(&i.node, out); }
        Expr::FieldAccess(o, _) => collect_escaped_expr(&o.node, out),
        _ => {}
    }
}

fn collect_escaped_stmt<'a>(s: &Stmt<'a>, out: &mut HashSet<String>) {
    match s {
        // `=` rhs that is a bare var is a move; `:=` clones so its rhs is NOT moved.
        Stmt::Let { rhs, .. } => {
            if let Expr::Var(v) = &rhs.node { out.insert(v.to_string()); }
            collect_escaped_expr(&rhs.node, out);
        }
        Stmt::CopyLet { rhs, .. } => collect_escaped_expr(&rhs.node, out),
        Stmt::Expr(e) => collect_escaped_expr(&e.node, out),
        Stmt::Return { value, cond } => {
            if let Some(v) = value {
                if let Expr::Var(n) = &v.node { out.insert(n.to_string()); }
                collect_escaped_expr(&v.node, out);
            }
            if let Some(c) = cond { collect_escaped_expr(&c.node, out); }
        }
        Stmt::If { cond, then, else_ } => {
            collect_escaped_expr(&cond.node, out);
            for st in then { collect_escaped_stmt(&st.node, out); }
            if let Some(e) = else_ { for st in e { collect_escaped_stmt(&st.node, out); } }
        }
        Stmt::IfLet { expr, then, else_, .. } => {
            collect_escaped_expr(&expr.node, out);
            for st in then { collect_escaped_stmt(&st.node, out); }
            if let Some(e) = else_ { for st in e { collect_escaped_stmt(&st.node, out); } }
        }
        Stmt::TryAssign { rhs, .. } => collect_escaped_expr(&rhs.node, out),
        // A coroutine may outlive the spawning scope and aliases captured values,
        // so conservatively treat every var it references as escaped (never free).
        Stmt::Spawn { body } => {
            let mut used = HashSet::new();
            for st in body { collect_stmt_vars(&st.node, &mut used); }
            out.extend(used);
            for st in body { collect_escaped_stmt(&st.node, out); }
        }
        _ => {}
    }
}

/// True if `body`'s last top-level statement is a `return` (so trailing Drop code
/// would be unreachable).
fn ends_with_return<'a>(body: &[Spanned<Stmt<'a>>]) -> bool {
    matches!(body.last().map(|s| &s.node), Some(Stmt::Return { .. }))
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
        Expr::Some(inner) | Expr::Ok(inner) | Expr::Err(inner) | Expr::Clone(inner) => {
            expr_has_await(&inner.node)
        }
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

                // TCO-hoisted heap string: declared above `inicio:`. Free the previous
                // iteration's buffer, then reassign (no redeclaration → no leak).
                if self.hoisted_heap_strs.contains(name.node) {
                    let value = self.emit_expr(&rhs.node);
                    self.output.push_str(&format!(
                        "{}if ({} != NULL) {{ sirin_cstr_free({}); }}\n",
                        self.indent(), name.node, name.node));
                    self.output.push_str(&format!(
                        "{}{} = {};\n", self.indent(), name.node, value));
                    return;
                }

                // Hoist any resource `::clone` in the rhs to a named temp first.
                self.hoist_resource_clones(&rhs.node);

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
                            let (_, snake) = collection_suffix_for(inner);
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

                // Typed JSON deserialize: `x: SomeStruct = jsontext.to_object()`
                // Compiler knows the target shape, so emit field-by-field extraction.
                if let (Expr::MethodCall(obj, "to_object", _), Type::Struct(fields)) = (&rhs.node, ty) {
                    let src = self.emit_expr(&obj.node);
                    let cty = self.type_to_c(ty);
                    let tmp = format!("__json_{}", name.node);
                    self.output.push_str(&format!(
                        "{}const char* {} = {};\n", self.indent(), tmp, src));
                    let inits = fields.iter().map(|(fname, fty)| {
                        let getter = match fty {
                            Type::Str   => "sirin_json_get_str",
                            Type::Float => "sirin_json_get_float",
                            Type::Bool  => "sirin_json_get_bool",
                            t if t.is_integer() => "sirin_json_get_int",
                            _ => "sirin_json_get_str",
                        };
                        format!(".{} = {}({}, \"{}\")", fname, getter, tmp, fname)
                    }).collect::<Vec<_>>().join(", ");
                    let is_global = self.globals.contains(name.node);
                    let lhs = if is_global {
                        format!("{}{}", self.indent(), name.node)
                    } else {
                        format!("{}{} {}", self.indent(), cty, name.node)
                    };
                    self.vars.insert(name.node.to_string(), ty.clone());
                    self.output.push_str(&format!("{} = ({}){{ {} }};\n", lhs, cty, inits));
                    return;
                }

                // Globals are declared at file scope → assign without a type prefix.
                let is_global = self.globals.contains(name.node);
                let lhs = if is_global {
                    format!("{}{}", self.indent(), name.node)
                } else {
                    format!("{}{} {}", self.indent(), self.type_to_c(ty), name.node)
                };

                // Collection constructor (Vec/Array/Map/Set) with declared type
                // → use type-specific constructor instead of generic fallback
                if let Expr::Call(ctor, ctor_args) = &rhs.node {
                    let line: Option<String> = match (*ctor, ty) {
                        ("Vec", Type::Vec(inner)) => {
                            let (_, snake) = collection_suffix_for(inner);
                            let cap = ctor_args.first()
                                .map(|a| self.emit_expr(&a.node))
                                .unwrap_or_else(|| "4".to_string());
                            Some(format!("{} = sirin_vec_{}_new({});\n", lhs, snake, cap))
                        }
                        ("Array", Type::Array(inner)) => {
                            let (_, snake) = collection_suffix_for(inner);
                            let cap = ctor_args.first()
                                .map(|a| self.emit_expr(&a.node))
                                .unwrap_or_else(|| "4".to_string());
                            Some(format!("{} = sirin_array_{}_new({});\n", lhs, snake, cap))
                        }
                        ("Map", Type::Map(_, val)) => {
                            let (_, vs) = collection_suffix_for(val);
                            Some(format!("{} = sirin_map_str_{}_new();\n", lhs, vs))
                        }
                        ("Set", Type::Set(inner)) => {
                            let (_, snake) = collection_suffix_for(inner);
                            Some(format!("{} = sirin_set_{}_new();\n", lhs, snake))
                        }
                        _ => None,
                    };
                    if let Some(l) = line {
                        self.vars.insert(name.node.to_string(), ty.clone());
                        self.output.push_str(&l);
                        return;
                    }
                }

                // `Ok`/`Err` typed against the declared `T!` so the C struct matches.
                let mut value = self
                    .emit_try_ctor(ty, &rhs.node)
                    .unwrap_or_else(|| self.emit_expr(&rhs.node));
                // `:=` is a real deep copy. For strings, clone the buffer so the new
                // binding owns its own memory (otherwise free would double-free).
                if matches!(stmt, Stmt::CopyLet { .. }) && matches!(ty, Type::Str) {
                    value = format!("sirin_str_clone({})", value);
                }
                // `:=` on a resource lvalue duplicates the OS handle (consistent with
                // `::clone`); cloning a freshly-returned resource would leak, so only
                // when the rhs is an existing addressable value.
                if matches!(stmt, Stmt::CopyLet { .. }) && is_addressable(&rhs.node) {
                    if let Some(cf) = resource_clone_fn(ty) {
                        value = format!("{}(&{})", cf, value);
                    }
                }
                self.vars.insert(name.node.to_string(), ty.clone());
                // Track external-resource locals so return paths can Drop (close) them.
                if !is_global {
                    if let Some(cf) = resource_close_fn(ty) {
                        self.live_drops.push((name.node.to_string(), cf));
                    }
                }
                self.output.push_str(&format!("{} = {};\n", lhs, value));
            }
            Stmt::Return { cond, value } => {
                if let Some(v) = value { self.hoist_resource_clones(&v.node); }
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
                    // Drop live resources/heap-strings before leaving the function.
                    let drops = self.return_drops(value.as_ref().map(|v| &**v));
                    match (cond, value) {
                        (Some(c), Some(v)) => {
                            let cond_str = self.emit_expr(&c.node);
                            let val_str = self.emit_ret_value(&v.node);
                            if drops.is_empty() {
                                self.output.push_str(&format!(
                                    "{}if ({}) return {};\n", self.indent(), cond_str, val_str));
                            } else {
                                self.output.push_str(&format!(
                                    "{}if ({}) {{\n", self.indent(), cond_str));
                                self.depth += 1;
                                for d in &drops {
                                    self.output.push_str(&format!("{}{}\n", self.indent(), d));
                                }
                                self.output.push_str(&format!(
                                    "{}return {};\n", self.indent(), val_str));
                                self.depth -= 1;
                                self.output.push_str(&format!("{}}}\n", self.indent()));
                            }
                        }
                        (Some(c), None) => {
                            let cond_str = self.emit_expr(&c.node);
                            if drops.is_empty() {
                                self.output.push_str(&format!(
                                    "{}if ({}) return;\n", self.indent(), cond_str));
                            } else {
                                self.output.push_str(&format!(
                                    "{}if ({}) {{\n", self.indent(), cond_str));
                                self.depth += 1;
                                for d in &drops {
                                    self.output.push_str(&format!("{}{}\n", self.indent(), d));
                                }
                                self.output.push_str(&format!("{}return;\n", self.indent()));
                                self.depth -= 1;
                                self.output.push_str(&format!("{}}}\n", self.indent()));
                            }
                        }
                        (None, Some(v)) => {
                            let val_str = self.emit_ret_value(&v.node);
                            for d in &drops {
                                self.output.push_str(&format!("{}{}\n", self.indent(), d));
                            }
                            self.output
                                .push_str(&format!("{}return {};\n", self.indent(), val_str));
                        }
                        (None, None) => {
                            for d in &drops {
                                self.output.push_str(&format!("{}{}\n", self.indent(), d));
                            }
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
                self.fns.insert(fn_name.to_string(), ret_ty.clone());

                let params = args
                    .iter()
                    .map(|(pname, ty)| format!("{} {}", self.type_to_c(ty), pname.node))
                    .collect::<Vec<_>>()
                    .join(", ");

                // Mark where this fn starts so body-level spawn_decls can be spliced before it
                let fn_start_pos = self.output.len();
                let spawn_decls_before = self.spawn_decls.len();

                self.output.push_str(&format!(
                    "{}{} {}({}) {{\n",
                    self.indent(),
                    ret,
                    c_fn_name(fn_name),
                    params
                ));

                self.depth += 1;

                let outer_vars = self.vars.clone();
                let outer_fn = self.current_fn.take();
                let outer_params = std::mem::take(&mut self.current_fn_params);
                let outer_ret = self.current_ret.replace(ret_ty.clone());
                let outer_drops = std::mem::take(&mut self.live_drops);
                let outer_heap_strs = std::mem::take(&mut self.live_heap_strs);
                let outer_hoisted = std::mem::take(&mut self.hoisted_heap_strs);
                let outer_escaped = std::mem::take(&mut self.current_escaped);

                // Ownership that escapes this body must never be auto-dropped.
                let mut escaped = HashSet::new();
                for s in body { collect_escaped_stmt(&s.node, &mut escaped); }
                self.current_escaped = escaped;

                // Clone-temp bookkeeping is per-function.
                self.clone_temps.borrow_mut().clear();
                self.clone_names.borrow_mut().clear();

                // Params are borrowed (the ownership model never moves fn args), so a
                // resource passed in is owned by the caller — never auto-closed here.
                // Only resource *locals* created in the body are dropped on return.
                for (pname, ty) in args {
                    self.vars.insert(pname.node.to_string(), ty.clone());
                }

                if tco {
                    // Hoist heap-string locals above the loop label so each iteration
                    // frees the previous buffer before reassigning (no leak per loop).
                    for s in body {
                        if let Stmt::Let { name, rhs, .. } | Stmt::CopyLet { name, rhs, .. } = &s.node {
                            let is_cl = matches!(&s.node, Stmt::CopyLet { .. });
                            if !self.globals.contains(name.node)
                                && self.is_heap_str_rhs(is_cl, &rhs.node)
                            {
                                self.output.push_str(&format!(
                                    "{}{} {} = NULL;\n",
                                    self.indent(), self.type_to_c(&Type::Str), name.node));
                                self.hoisted_heap_strs.insert(name.node.to_string());
                                self.vars.insert(name.node.to_string(), Type::Str);
                                self.live_heap_strs.push(name.node.to_string());
                            }
                        }
                    }
                    self.output.push_str(&format!("{}inicio:\n", self.indent()));
                    self.current_fn = Some(fn_name.to_string());
                    self.current_fn_params =
                        args.iter().map(|(pname, _)| pname.node.to_string()).collect();
                }

                for s in body {
                    self.emit_stmt(&s.node);
                }

                // Drop owned heap-string locals on fall-through (skip TCO loops and
                // bodies ending in `return`, where the free would be unreachable).
                if !tco && !ends_with_return(body) {
                    let refs: Vec<&Spanned<Stmt>> = body.iter().collect();
                    self.emit_str_drops(&refs);
                }

                self.vars = outer_vars;
                self.current_fn = outer_fn;
                self.current_fn_params = outer_params;
                self.current_ret = outer_ret;
                self.live_drops = outer_drops;
                self.live_heap_strs = outer_heap_strs;
                self.hoisted_heap_strs = outer_hoisted;
                self.current_escaped = outer_escaped;
                self.depth -= 1;

                self.output.push_str(&format!("{}}}\n", self.indent()));

                // Splice any spawn_decls generated inside this fn's body to appear before the fn
                if self.spawn_decls.len() > spawn_decls_before {
                    let new_decls = self.spawn_decls[spawn_decls_before..].to_string();
                    self.spawn_decls.truncate(spawn_decls_before);
                    let fn_code = self.output[fn_start_pos..].to_string();
                    self.output.truncate(fn_start_pos);
                    self.output.push_str(&new_decls);
                    self.output.push_str(&fn_code);
                }
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
            Stmt::IfLet { pattern, name, expr, then, else_ } => {
                // `Some` → NULL-check a pointer; `Ok`/`Err` → check the Try tag.
                let scrut_ty = self.get_expr(&expr.node);
                let scrut_cty = self.type_to_c(&scrut_ty);
                if expr_has_await(&expr.node) {
                    self.output.push_str(&format!("{}sirin_yield();\n", self.indent()));
                }
                let val = self.emit_expr(&expr.node);
                let id = self.spawn_count;
                self.spawn_count += 1;
                let tmp = format!("__pat{}", id);
                self.output.push_str(&format!(
                    "{}{} {} = {};\n", self.indent(), scrut_cty, tmp, val));

                // (condition, bound-name type, how to read the payload from `tmp`)
                let (cond, bind_ty, payload) = match pattern {
                    BindPattern::Some => {
                        let inner = match &scrut_ty {
                            Type::Nullable(i) => (**i).clone(),
                            _ => Type::Int,
                        };
                        (format!("{} != NULL", tmp), inner, format!("*{}", tmp))
                    }
                    BindPattern::Ok => {
                        let inner = match &scrut_ty {
                            Type::Try(i) => (**i).clone(),
                            _ => Type::Int,
                        };
                        (format!("{}.ok", tmp), inner, format!("{}.value", tmp))
                    }
                    BindPattern::Err => {
                        (format!("!{}.ok", tmp), Type::Str, format!("{}.err", tmp))
                    }
                };
                let bind_cty = self.type_to_c(&bind_ty);
                self.output.push_str(&format!("{}if ({}) {{\n", self.indent(), cond));

                self.depth += 1;
                self.output.push_str(&format!(
                    "{}{} {} = {};\n", self.indent(), bind_cty, name.node, payload));
                self.vars.insert(name.node.to_string(), bind_ty);
                for s in then {
                    self.emit_stmt(&s.node);
                }
                self.depth -= 1;

                if let Some(else_body) = else_ {
                    self.output.push_str(&format!("{}}} else {{\n", self.indent()));
                    self.depth += 1;
                    for s in else_body {
                        self.emit_stmt(&s.node);
                    }
                    self.depth -= 1;
                }

                self.output.push_str(&format!("{}}}\n", self.indent()));
            }
            Stmt::TryAssign { name, rhs } => {
                // `name ?= f()` → eval Try; on Err, propagate via the current fn's Try
                // return; otherwise bind the Ok payload.
                let rhs_ty = self.get_expr(&rhs.node);
                let inner_ty = match &rhs_ty {
                    Type::Try(i) => (**i).clone(),
                    _ => Type::Void,
                };
                let try_cty = self.type_to_c(&rhs_ty);
                let inner_cty = self.type_to_c(&inner_ty);
                if expr_has_await(&rhs.node) {
                    self.output.push_str(&format!("{}sirin_yield();\n", self.indent()));
                }
                let val = self.emit_expr(&rhs.node);
                let id = self.spawn_count;
                self.spawn_count += 1;
                let tmp = format!("__try{}", id);
                self.output.push_str(&format!(
                    "{}{} {} = {};\n", self.indent(), try_cty, tmp, val));

                // Build the propagated Err of the current fn's return type.
                let ret_cty = self
                    .current_ret
                    .as_ref()
                    .map(|t| self.type_to_c(t))
                    .unwrap_or_else(|| try_cty.clone());
                self.output.push_str(&format!(
                    "{}if (!{}.ok) return ({}){{ .ok = 0, .err = {}.err }};\n",
                    self.indent(), tmp, ret_cty, tmp));
                self.output.push_str(&format!(
                    "{}{} {} = {}.value;\n", self.indent(), inner_cty, name.node, tmp));
                self.vars.insert(name.node.to_string(), inner_ty);
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
                self.hoist_resource_clones(&expr.node);
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
                // Propagate used_types + named_collection_decls back
                let sub_used = sub.used_types.into_inner();
                self.used_types.borrow_mut().extend(sub_used);
                let sub_named = sub.named_collection_decls.into_inner();
                self.named_collection_decls.borrow_mut().extend(sub_named);
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
                            _       => format!("{}({})", c_fn_name(name), args_str),
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
                        let (_, s) = collection_suffix_for(&inner);
                        format!("sirin_vec_{}_get(&{}, {})", s, b, i)
                    }
                    // Array is stack-allocated; use C direct subscript
                    Type::Array(_) => format!("{}[{}]", b, i),
                    _ => format!("{}[{}]", b, i),
                }
            }
            Expr::MethodCall(obj, method, args) => {
                // Static net constructors: TcpStream.connect(addr, port)
                if matches!(&obj.node, Expr::Var("TcpStream") | Expr::NewDefault("TcpStream")) {
                    if *method == "connect" {
                        let args_str = args.iter().map(|a| self.emit_expr(&a.node)).collect::<Vec<_>>().join(", ");
                        return format!("sirin_tcp_stream_connect({})", args_str);
                    }
                }
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
                        let (_, s) = collection_suffix_for(&inner);
                        match *method {
                            "push" => format!("sirin_vec_{}_push(&{}, {})", s, obj_str, args_str),
                            "get"  => format!("sirin_vec_{}_get(&{}, {})",  s, obj_str, args_str),
                            "len"  => format!("(int64_t)({}.len)", obj_str),
                            "free" => format!("sirin_vec_{}_free(&{})", s, obj_str),
                            _      => format!("sirin_vec_{}_get(&{}, {})",  s, obj_str, args_str),
                        }
                    }
                    Type::Array(inner) => {
                        let (_, s) = collection_suffix_for(&inner);
                        match *method {
                            "push" => format!("sirin_array_{}_push(&{}, {})", s, obj_str, args_str),
                            "get"  => format!("sirin_array_{}_get(&{}, {})",  s, obj_str, args_str),
                            "len"  => format!("(int64_t)({}.len)", obj_str),
                            "free" => format!("sirin_array_{}_free(&{})", s, obj_str),
                            _      => format!("sirin_array_{}_get(&{}, {})",  s, obj_str, args_str),
                        }
                    }
                    Type::Map(_, val) => {
                        let (_, vs) = collection_suffix(&val);
                        match *method {
                            "insert" | "set" => format!("sirin_map_str_{}_insert(&{}, {})", vs, obj_str, args_str),
                            "len"      => format!("(int64_t)({}.len)", obj_str),
                            "keys_at"  => format!("sirin_map_str_{}_key_at(&{}, {})", vs, obj_str, args_str),
                            _          => format!("sirin_map_str_{}_get_opt(&{}, {})", vs, obj_str, args_str),
                        }
                    }
                    Type::Set(inner) => {
                        let (_, s) = collection_suffix_for(&inner);
                        match *method {
                            "insert"   => format!("sirin_set_{}_insert(&{}, {})",   s, obj_str, args_str),
                            "contains" => format!("sirin_set_{}_contains(&{}, {})", s, obj_str, args_str),
                            _          => format!("{}.{}({})", obj_str, method, args_str),
                        }
                    }
                    Type::Named(cls) => {
                        // Net type static constructor: TcpStream.connect(addr, port)
                        if matches!(&obj.node, Expr::NewDefault("TcpStream") | Expr::Var("TcpStream")) {
                            if *method == "connect" {
                                return format!("sirin_tcp_stream_connect({})", args_str);
                            }
                        }
                        // Net type methods
                        match cls.as_str() {
                            "TcpListener" => match *method {
                                "accept" => return format!("sirin_tcp_listener_accept(&{})", obj_str),
                                "close"  => return format!("sirin_tcp_listener_close(&{})", obj_str),
                                _ => {}
                            },
                            "TcpStream" => match *method {
                                "read"  => return format!("sirin_tcp_stream_read(&{})", obj_str),
                                "write" => return format!("sirin_tcp_stream_write(&{}, {})", obj_str, args_str),
                                "close" => return format!("sirin_tcp_stream_close(&{})", obj_str),
                                _ => {}
                            },
                            "UdpSocket" => match *method {
                                "recv_from" => return format!("sirin_udp_socket_recv_from(&{})", obj_str),
                                "send_to"   => return format!("sirin_udp_socket_send_to(&{}, {})", obj_str, args_str),
                                "close"     => return format!("sirin_udp_socket_close(&{})", obj_str),
                                _ => {}
                            },
                            _ => {}
                        }
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
                    Type::Str => {
                        let one = || args.first().map(|a| self.emit_expr(&a.node)).unwrap_or_default();
                        match *method {
                            "len"         => format!("sirin_str_len({})", obj_str),
                            "char_at"     => format!("sirin_str_char_at({}, {})", obj_str, args_str),
                            "slice"       => format!("sirin_str_slice({}, {})", obj_str, args_str),
                            "index_of"    => format!("sirin_str_index_of({}, {})", obj_str, args_str),
                            "contains"    => format!("sirin_str_contains({}, {})", obj_str, args_str),
                            "starts_with" => format!("sirin_str_starts_with({}, {})", obj_str, args_str),
                            "ends_with"   => format!("sirin_str_ends_with({}, {})", obj_str, args_str),
                            "trim"        => format!("sirin_str_trim({})", obj_str),
                            "to_int"      => format!("sirin_str_to_int({})", obj_str),
                            "to_float"    => format!("sirin_str_to_float({})", obj_str),
                            "to_upper"    => format!("sirin_str_to_upper({})", obj_str),
                            "to_lower"    => format!("sirin_str_to_lower({})", obj_str),
                            "replace"     => format!("sirin_str_replace({}, {})", obj_str, args_str),
                            "split"       => {
                                self.used_types.borrow_mut().insert("SIRIN_USE_VEC_STR".to_string());
                                format!("sirin_str_split({}, {})", obj_str, one())
                            }
                            // typed deserialize handled in `let` binding; raw passthrough otherwise
                            "to_object"   => obj_str.clone(),
                            _ => {
                                if self.prim_methods.get("str").map_or(false, |s| s.contains(*method)) {
                                    if args_str.is_empty() {
                                        format!("str_{}({})", method, obj_str)
                                    } else {
                                        format!("str_{}({}, {})", method, obj_str, args_str)
                                    }
                                } else {
                                    format!("{}.{}({})", obj_str, method, args_str)
                                }
                            }
                        }
                    }
                    Type::Nullable(inner) => {
                        let cty = format!("{}*", self.type_to_c(&inner));
                        match *method {
                            "unwrap" => format!(
                                "({{ {cty} __u = ({obj_str}); if (__u == NULL) {{ fprintf(stderr, \"sirin: unwrap on None\\n\"); exit(1); }} *__u; }})"),
                            "unwrap_or" => format!("({{ {cty} __u = ({obj_str}); __u ? *__u : ({args_str}); }})"),
                            "is_some" => format!("(({obj_str}) != NULL)"),
                            "is_none" => format!("(({obj_str}) == NULL)"),
                            _ => obj_str.clone(),
                        }
                    }
                    Type::Try(inner) => {
                        let tname = self.type_to_c(&Type::Try(inner));
                        match *method {
                            "unwrap" => format!(
                                "({{ {tname} __u = ({obj_str}); if (!__u.ok) {{ fprintf(stderr, \"sirin: unwrap on Err: %s\\n\", __u.err); exit(1); }} __u.value; }})"),
                            "unwrap_or" => format!("({{ {tname} __u = ({obj_str}); __u.ok ? __u.value : ({args_str}); }})"),
                            "is_ok"  => format!("(({obj_str}).ok)"),
                            "is_err" => format!("(!({obj_str}).ok)"),
                            _ => obj_str.clone(),
                        }
                    }
                    _ => {
                        // `int.to_str()` → heap decimal string.
                        if *method == "to_str" && obj_ty.is_integer() {
                            return format!("sirin_int_to_str({})", obj_str);
                        }
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
                match *name {
                    "TcpListener" => format!("sirin_tcp_listener_bind({})", args_str),
                    "UdpSocket"   => format!("sirin_udp_socket_bind({})", args_str),
                    _             => format!("{}_new({})", name, args_str),
                }
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
            Expr::ObjectLiteral(fields) => {
                let struct_ty = self.get_expr(expr);
                let cty = self.type_to_c(&struct_ty);
                let inits = fields.iter()
                    .map(|(fname, fval)| format!(".{} = {}", fname, self.emit_expr(&fval.node)))
                    .collect::<Vec<_>>().join(", ");
                format!("({}){{ {} }}", cty, inits)
            }
            // Option uses a pointer: `Some(v)` heap-boxes the value, `None` is NULL.
            Expr::Some(inner) => {
                let inner_ty = self.get_expr(&inner.node);
                let cty = self.type_to_c(&inner_ty);
                let val = self.emit_expr(&inner.node);
                format!("({{ {cty}* __sv = malloc(sizeof({cty})); *__sv = ({val}); __sv; }})")
            }
            Expr::None => "NULL".to_string(),
            // `Ok(v)` self-types from v. `Err(msg)` has unknown value type here, so it
            // lowers to the `void`-payload Try; `return`/typed-let re-emit it with the
            // proper target type (see emit_try_ctor).
            Expr::Ok(inner) => {
                let inner_ty = self.get_expr(&inner.node);
                let tname = self.type_to_c(&Type::Try(Box::new(inner_ty)));
                let v = self.emit_expr(&inner.node);
                format!("({tname}){{ .ok = 1, .value = ({v}) }}")
            }
            Expr::Err(inner) => {
                let tname = self.type_to_c(&Type::Try(Box::new(Type::Void)));
                let m = self.emit_expr(&inner.node);
                format!("({tname}){{ .ok = 0, .err = ({m}) }}")
            }
            // `a::clone` — deep copy. Strings get their own buffer; resource types
            // duplicate the OS handle via the runtime; plain copy types and (for now)
            // data structs/collections pass through (matches `:=` semantics).
            Expr::Clone(inner) => {
                // If this clone was hoisted to a named temp, return that temp.
                let key = expr as *const Expr as usize;
                if let Some(t) = self.clone_temps.borrow().get(&key) {
                    return t.clone();
                }
                let v = self.emit_expr(&inner.node);
                let inner_ty = self.get_expr(&inner.node);
                if matches!(inner_ty, Type::Str) {
                    format!("sirin_str_clone({})", v)
                } else if let Some(cf) = resource_clone_fn(&inner_ty) {
                    if is_addressable(&inner.node) {
                        format!("{}(&{})", cf, v)
                    } else {
                        v // fresh resource value, not aliased — nothing to dup
                    }
                } else {
                    v
                }
            }
            Expr::Neg(expr) => format!("(-{})", self.emit_expr(&expr.node)),
            Expr::Not(expr) => format!("(!{})", self.emit_expr(&expr.node)),
            Expr::BinOp(op, lhs, rhs) => {
                let l = self.emit_expr(&lhs.node);
                let r = self.emit_expr(&rhs.node);
                // String equality: pointer compare is wrong → use strcmp
                if matches!(op, BinOp::Eq | BinOp::NotEq)
                    && (matches!(self.get_expr(&lhs.node), Type::Str)
                        || matches!(self.get_expr(&rhs.node), Type::Str))
                {
                    let cmp = if matches!(op, BinOp::Eq) { "==" } else { "!=" };
                    return format!("(strcmp({}, {}) {} 0)", l, r, cmp);
                }
                // String concatenation allocates a fresh buffer.
                if matches!(op, BinOp::Add) && matches!(self.get_expr(&lhs.node), Type::Str) {
                    return format!("sirin_str_concat({}, {})", l, r);
                }
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

    /// Emit an `Ok(v)`/`Err(m)` literal with an explicit target `Try[T]` type so the
    /// C struct type matches the binding/return (needed for `Err`, whose value type is
    /// unknown from the expression alone). Returns None if not an Ok/Err on a Try target.
    fn emit_try_ctor(&self, target: &Type, e: &Expr<'_>) -> Option<String> {
        if !matches!(target, Type::Try(_)) {
            return None;
        }
        let tname = self.type_to_c(target);
        match e {
            Expr::Ok(v) => {
                let vs = self.emit_expr(&v.node);
                Some(format!("({tname}){{ .ok = 1, .value = ({vs}) }}"))
            }
            Expr::Err(m) => {
                let ms = self.emit_expr(&m.node);
                Some(format!("({tname}){{ .ok = 0, .err = ({ms}) }}"))
            }
            _ => None,
        }
    }

    /// Emit a return/let value, typing `Ok`/`Err` against the current return type.
    fn emit_ret_value(&self, v: &Expr<'_>) -> String {
        if let Some(rt) = &self.current_ret {
            if let Some(s) = self.emit_try_ctor(rt, v) {
                return s;
            }
        }
        self.emit_expr(v)
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
            Type::Try(inner) => {
                let mangle = type_mangle(inner);
                let cinner = self.type_to_c(inner);
                self.register_try_type(&mangle, &cinner);
                format!("SirinTry_{mangle}")
            }
            Type::Vec(inner) => {
                let (pascal, snake) = collection_suffix_for(inner);
                if let Type::Named(_) = inner.as_ref() {
                    let ct = self.type_to_c(inner);
                    self.register_named_collection("Vec", &pascal, &snake, &ct);
                }
                format!("SirinVec{pascal}")
            }
            Type::Array(inner) => {
                let (pascal, snake) = collection_suffix_for(inner);
                if let Type::Named(_) = inner.as_ref() {
                    let ct = self.type_to_c(inner);
                    self.register_named_collection("Array", &pascal, &snake, &ct);
                }
                format!("SirinArray{pascal}")
            }
            Type::Set(inner) => {
                let (pascal, snake) = collection_suffix_for(inner);
                if let Type::Named(_) = inner.as_ref() {
                    let ct = self.type_to_c(inner);
                    self.register_named_collection("Set", &pascal, &snake, &ct);
                }
                format!("SirinSet{pascal}")
            }
            Type::Map(_, val) => {
                let (pascal, snake) = collection_suffix_for(val);
                if let Type::Named(_) = val.as_ref() {
                    let ct = self.type_to_c(val);
                    self.register_named_collection("Map", &pascal, &snake, &ct);
                }
                format!("SirinMapStr{pascal}")
            }
            Type::Named(n) => match n.as_str() {
                "TcpListener" => "SirinTcpListener".to_string(),
                "TcpStream"   => "SirinTcpStream".to_string(),
                "UdpSocket"   => "SirinUdpSocket".to_string(),
                "UdpPacket"   => "SirinUdpPacket".to_string(),
                _             => n.clone(),
            },
            Type::Struct(fields) => {
                // Synthetic struct: name derived from the (sorted) field signature so
                // two literals of the same shape share one C typedef (structural typing).
                let mut sig = String::new();
                let mut body = String::new();
                for (fname, fty) in fields {
                    let ct = self.type_to_c(fty);
                    sig.push_str(&format!("{}:{};", fname, ct));
                    body.push_str(&format!("    {} {};\n", ct, fname));
                }
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                hasher.write(sig.as_bytes());
                let sname = format!("__sirin_obj_{:016x}", hasher.finish());
                self.register_anon_struct(&sname, &body);
                sname
            }
            Type::Channel(_) => "SirinChannel*".to_string(),
            Type::Func(args, ret) => {
                // Render as a function-pointer typedef so the name works uniformly as a
                // param type, struct field, local, or cast — `R (*__sirin_fn_x)(A, B)`.
                let ret_c = self.type_to_c(ret);
                let arg_cs: Vec<String> = args.iter().map(|a| self.type_to_c(a)).collect();
                let sig = format!("{}({})", ret_c, arg_cs.join(","));
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                hasher.write(sig.as_bytes());
                let fname = format!("__sirin_fn_{:016x}", hasher.finish());
                self.register_fn_type(&fname, &ret_c, &arg_cs);
                fname
            }
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
                _ => {
                    // Calling through a function-typed local/param yields its return type.
                    if let Some(Type::Func(_, ret)) = self.vars.get(*name) {
                        return (**ret).clone();
                    }
                    self.fns.get(*name).cloned().unwrap_or(Type::Void)
                }
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
                // Static net constructors: TcpStream.connect(...)
                if matches!(&obj.node, Expr::Var("TcpStream") | Expr::NewDefault("TcpStream")) {
                    if *method == "connect" { return Type::Named("TcpStream".to_string()); }
                }
                if *method == "to_str" { return Type::Str; }
                let obj_ty = self.get_expr(&obj.node);
                match &obj_ty {
                    Type::Channel(inner) => match *method {
                        "send" | "free" => Type::Void,
                        "recv" => *inner.clone(),
                        _ => Type::Void,
                    },
                    Type::Vec(inner) | Type::Array(inner) => match *method {
                        "push" | "free" => Type::Void,
                        "len" => Type::Int,
                        _ => *inner.clone(),
                    },
                    Type::Map(_, v) => match *method {
                        "insert" | "set" => Type::Void,
                        "len"            => Type::Int,
                        "keys_at"        => Type::Nullable(Box::new(Type::Str)),
                        _                => Type::Nullable(v.clone()),
                    },
                    Type::Set(_) => match *method {
                        "insert" => Type::Void,
                        "contains" => Type::Bool,
                        _ => Type::Void,
                    },
                    Type::Str => match *method {
                        "len" | "index_of" | "to_int" => Type::Int,
                        "to_float" => Type::Float,
                        "contains" | "starts_with" | "ends_with" => Type::Bool,
                        "char_at" | "slice" | "trim" | "to_upper"
                            | "to_lower" | "replace" => Type::Str,
                        "split" => Type::Vec(Box::new(Type::Str)),
                        _ => Type::Void,
                    },
                    Type::Nullable(inner) => match *method {
                        "unwrap" | "unwrap_or" => (**inner).clone(),
                        "is_some" | "is_none"  => Type::Bool,
                        _ => Type::Void,
                    },
                    Type::Try(inner) => match *method {
                        "unwrap" | "unwrap_or" => (**inner).clone(),
                        "is_ok" | "is_err"     => Type::Bool,
                        _ => Type::Void,
                    },
                    Type::Named(cls) => {
                        // Net static constructor
                        if matches!(&obj.node, Expr::NewDefault("TcpStream") | Expr::Var("TcpStream")) && *method == "connect" {
                            return Type::Named("TcpStream".to_string());
                        }
                        match cls.as_str() {
                            "TcpListener" => match *method {
                                "accept" => return Type::Named("TcpStream".to_string()),
                                _ => return Type::Void,
                            },
                            "TcpStream" => match *method {
                                "read" => return Type::Str,
                                _ => return Type::Void,
                            },
                            "UdpSocket" => match *method {
                                "recv_from" => return Type::Named("UdpPacket".to_string()),
                                _ => return Type::Void,
                            },
                            _ => {}
                        }
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
                if let Type::Struct(fields) = &obj_ty {
                    if let Some((_, ty)) = fields.iter().find(|(n, _)| n.as_str() == *field) {
                        return ty.clone();
                    }
                    return Type::Void;
                }
                if let Type::Named(cls) = &obj_ty {
                    // UdpPacket field types
                    if cls == "UdpPacket" {
                        return match *field {
                            "data" | "addr" => Type::Str,
                            "port"          => Type::Int,
                            _               => Type::Void,
                        };
                    }
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
            Expr::ObjectLiteral(fields) => {
                let mut tys: Vec<(String, Type)> = fields.iter()
                    .map(|(n, v)| (n.to_string(), self.get_expr(&v.node)))
                    .collect();
                tys.sort_by(|a, b| a.0.cmp(&b.0));
                Type::Struct(tys)
            }
            Expr::Some(inner) => Type::Nullable(Box::new(self.get_expr(&inner.node))),
            Expr::None => Type::Nullable(Box::new(Type::Void)),
            Expr::Ok(inner) => Type::Try(Box::new(self.get_expr(&inner.node))),
            Expr::Err(_) => Type::Try(Box::new(Type::Void)),
            Expr::Clone(inner) => self.get_expr(&inner.node),
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
        let net_h = if self.net_imported { "#include \"sirin_net.h\"\n" } else { "" };
        let program = format!("{}#include \"sirin_runtime.h\"\n{}{}{}\n{}", prefix, async_h, net_h, io, self.output);
        (program, prefix)
    }

    pub fn emit_program_tcc<'a>(mut self, stmts: &'a [Spanned<Stmt<'a>>]) -> (String, String) {
        self.emit_top_level(stmts);
        let prefix = self.defines_prefix();
        let io = if self.io_imported { "#include <stdio.h>\n" } else { "" };
        let async_h = if self.async_imported { "#include \"sirin_async.h\"\n#include <stdlib.h>\n" } else { "" };
        let net_h = if self.net_imported { "#include \"sirin_net.h\"\n" } else { "" };
        let program = format!("{}typedef long long int64_t;\n{}{}{}\n{}", prefix, async_h, net_h, io, self.output);
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
                    if m == "sirin.net"   { self.net_imported   = true; }
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

        // Module-level Let/CopyLet that are referenced inside a function (or a
        // spawn body) become file-scope C globals so the function can share them.
        // Vars only used in `main` stay local — preserves array-literal locals etc.
        let mut fn_refs = std::collections::HashSet::new();
        for s in &fn_stmts { collect_stmt_vars(&s.node, &mut fn_refs); }
        for s in &rest {
            if let Stmt::Spawn { .. } = &s.node { collect_stmt_vars(&s.node, &mut fn_refs); }
        }
        let mut emitted_global = false;
        for s in &rest {
            if let Stmt::Let { name, ty: declared, rhs }
                 | Stmt::CopyLet { name, ty: declared, rhs } = &s.node {
                if !fn_refs.contains(name.node) { continue; }
                let inferred = self.get_expr(&rhs.node);
                let ty = declared.clone().unwrap_or(inferred);
                let cty = self.type_to_c(&ty);
                self.globals.insert(name.node.to_string());
                self.vars.insert(name.node.to_string(), ty);
                self.output.push_str(&format!("static {} {};\n", cty, name.node));
                emitted_global = true;
            }
        }
        if emitted_global { self.output.push('\n'); }

        for s in &fn_stmts    { self.emit_stmt(&s.node); }

        // Remember where to insert spawn declarations (after fns, before main)
        let spawn_insert_pos = self.output.len();

        // A user `fn main` was emitted as `main_<hash>` (see c_fn_name); the
        // synthesized entry below calls it so it still runs as the program entry.
        let user_main = fn_stmts.iter().any(|s| matches!(
            &s.node, Stmt::Fn { name, .. } if name.node == "main"
        ));

        if !rest.is_empty() || user_main {
            if self.async_imported {
                self.output.push_str("int main(void) {\n    sirin_loop_init();\n");
                if self.net_imported {
                    self.output.push_str("    sirin_net_init();\n");
                }
            } else if self.net_imported {
                self.output.push_str("int main(void) {\n    sirin_net_init();\n");
            } else {
                self.output.push_str("int main(void) {\n");
            }
            self.depth += 1;
            for s in &rest { self.emit_stmt(&s.node); }
            // Drop owned heap-string locals before main exits (vars captured by a
            // spawn are treated as escaped, so coroutines never see freed memory).
            self.emit_str_drops(&rest);
            if user_main {
                self.output.push_str(&format!("{}{}();\n", self.indent(), c_fn_name("main")));
            }
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

    fn named_collection_decls_string(&self) -> String {
        let map = self.named_collection_decls.borrow();
        if map.is_empty() { return String::new(); }
        // Emit by dependency category: collections first, then anon structs, then the
        // `Try[T]` tagged structs that wrap them, then function-pointer typedefs that
        // reference both. Within a category, sort for stable output.
        fn rank(key: &str) -> u8 {
            if key.starts_with("Struct_") { 1 }
            else if key.starts_with("Try_") { 2 }
            else if key.starts_with("FnType_") { 3 }
            else { 0 }
        }
        let mut entries: Vec<(&String, &String)> = map.iter().collect();
        entries.sort_by(|a, b| rank(a.0).cmp(&rank(b.0)).then(a.1.cmp(b.1)));
        let mut out = "#include <string.h>\n".to_string();
        for (_, e) in entries { out.push_str(e); }
        out
    }

    fn register_named_collection(&self, kind: &str, pascal: &str, snake: &str, c_type: &str) {
        let key = format!("{kind}_{pascal}");
        if self.named_collection_decls.borrow().contains_key(&key) { return; }
        let decl = match kind {
            "Vec"   => named_vec_impl(pascal, snake, c_type),
            "Array" => named_array_impl(pascal, snake, c_type),
            "Set"   => named_set_impl(pascal, snake, c_type),
            "Map"   => named_map_impl(pascal, snake, c_type),
            _       => return,
        };
        self.named_collection_decls.borrow_mut().insert(key, decl);
    }

    /// A let-binding owns heap string memory when its RHS allocates one. Literals,
    /// params, field reads and plain fn-call returns are NOT owned here — conservative
    /// so auto-free never touches a static literal (which would crash).
    fn is_heap_str_rhs<'a>(&self, is_copylet: bool, rhs: &Expr<'a>) -> bool {
        if is_copylet {
            return matches!(self.get_expr(rhs), Type::Str); // `:=` clones → owned heap
        }
        match rhs {
            Expr::Call(n, _) if *n == "readln" => true,
            // `x = a::clone` on a string allocates a fresh owned buffer.
            Expr::Clone(inner) => matches!(self.get_expr(&inner.node), Type::Str),
            // `.await` is transparent for ownership: unwrap and inspect the inner call.
            Expr::Await(inner) => self.is_heap_str_rhs(is_copylet, &inner.node),
            Expr::MethodCall(obj, m, _) => {
                let obj_ty = self.get_expr(&obj.node);
                // str-producing string ops allocate a new buffer…
                (matches!(obj_ty, Type::Str)
                    && matches!(*m, "to_upper" | "to_lower" | "slice" | "trim" | "replace" | "char_at"))
                // …as does TcpStream.read() (malloc'd const char*)…
                || (matches!(&obj_ty, Type::Named(n) if n == "TcpStream") && *m == "read")
                // …and `int.to_str()` (heap decimal string).
                || (*m == "to_str" && obj_ty.is_integer())
            }
            // String concatenation allocates a fresh owned buffer.
            Expr::BinOp(BinOp::Add, lhs, _) => matches!(self.get_expr(&lhs.node), Type::Str),
            _ => false,
        }
    }

    /// Hoist resource `::clone` sub-expressions in `e` into named temp vars,
    /// emitting `T _clone_x = sirin_T_clone(&x);` before the consuming statement.
    /// The Clone emit arm then returns the temp (looked up by node address).
    fn hoist_resource_clones<'a>(&mut self, e: &'a Expr<'a>) {
        match e {
            Expr::Clone(inner) => {
                let inner_ty = self.get_expr(&inner.node);
                if let (Some(cf), true) =
                    (resource_clone_fn(&inner_ty), is_addressable(&inner.node))
                {
                    let key = e as *const Expr as usize;
                    if !self.clone_temps.borrow().contains_key(&key) {
                        let operand = self.emit_expr(&inner.node);
                        let base = match &inner.node {
                            Expr::Var(v) => format!("_clone_{}", v),
                            _ => {
                                let n = *self.clone_count.borrow();
                                *self.clone_count.borrow_mut() = n + 1;
                                format!("_clone_t{}", n)
                            }
                        };
                        // Disambiguate if the name is already taken in this fn.
                        let mut temp = base.clone();
                        while self.clone_names.borrow().contains(&temp) {
                            let n = *self.clone_count.borrow();
                            *self.clone_count.borrow_mut() = n + 1;
                            temp = format!("{}_{}", base, n);
                        }
                        self.clone_names.borrow_mut().insert(temp.clone());
                        let cty = self.type_to_c(&inner_ty);
                        self.output.push_str(&format!(
                            "{}{} {} = {}(&{});\n", self.indent(), cty, temp, cf, operand));
                        self.clone_temps.borrow_mut().insert(key, temp);
                    }
                }
                self.hoist_resource_clones(&inner.node);
            }
            Expr::MethodCall(obj, _, args) => {
                self.hoist_resource_clones(&obj.node);
                for a in args { self.hoist_resource_clones(&a.node); }
            }
            Expr::Call(_, args) | Expr::New(_, args) => {
                for a in args { self.hoist_resource_clones(&a.node); }
            }
            Expr::Some(i) | Expr::Ok(i) | Expr::Err(i) | Expr::Await(i)
            | Expr::Neg(i) | Expr::Not(i) => self.hoist_resource_clones(&i.node),
            Expr::BinOp(_, l, r) => {
                self.hoist_resource_clones(&l.node);
                self.hoist_resource_clones(&r.node);
            }
            Expr::Index(b, i) => {
                self.hoist_resource_clones(&b.node);
                self.hoist_resource_clones(&i.node);
            }
            Expr::FieldAccess(o, _) => self.hoist_resource_clones(&o.node),
            Expr::Array(items) => {
                for it in items { self.hoist_resource_clones(&it.node); }
            }
            Expr::ObjectLiteral(fs) | Expr::NewFields(_, fs) => {
                for (_, v) in fs { self.hoist_resource_clones(&v.node); }
            }
            _ => {}
        }
    }

    /// Drop statements (no indent) for resources/heap-strings live at a return.
    /// Skips the var being returned (moved out) and anything that escapes.
    fn return_drops(&self, value: Option<&Spanned<Expr<'_>>>) -> Vec<String> {
        let moved = match value.map(|v| &v.node) {
            Some(Expr::Var(x)) => Some(*x),
            _ => None,
        };
        let mut out = Vec::new();
        for (name, cf) in &self.live_drops {
            if Some(name.as_str()) == moved || self.current_escaped.contains(name) { continue; }
            out.push(format!("{}(&{});", cf, name));
        }
        for name in &self.live_heap_strs {
            if Some(name.as_str()) == moved || self.current_escaped.contains(name) { continue; }
            out.push(format!("sirin_cstr_free({});", name));
        }
        out
    }

    /// Emit `sirin_cstr_free` for owned heap-string locals in `body` that don't
    /// escape. Conservative: only top-level Let/CopyLet declared exactly once,
    /// non-global, non-escaping, heap-allocating. Misses leak; never double-frees.
    fn emit_str_drops<'a>(&mut self, body: &[&Spanned<Stmt<'a>>]) {
        let mut escaped = HashSet::new();
        for s in body { collect_escaped_stmt(&s.node, &mut escaped); }

        let mut decl_count: HashMap<&str, u32> = HashMap::new();
        for s in body {
            if let Stmt::Let { name, .. } | Stmt::CopyLet { name, .. } = &s.node {
                *decl_count.entry(name.node).or_default() += 1;
            }
        }
        for s in body {
            if let Stmt::Let { name, rhs, .. } | Stmt::CopyLet { name, rhs, .. } = &s.node {
                let is_cl = matches!(&s.node, Stmt::CopyLet { .. });
                if self.globals.contains(name.node) { continue; }
                if escaped.contains(name.node) { continue; }
                if decl_count.get(name.node).copied().unwrap_or(0) != 1 { continue; }
                if self.is_heap_str_rhs(is_cl, &rhs.node) {
                    self.output.push_str(&format!(
                        "{}sirin_cstr_free({});\n", self.indent(), name.node));
                }
            }
        }
    }

    /// Register the monomorphized `Try[T]` tagged struct: `{ ok, value, err }`.
    fn register_try_type(&self, snake: &str, c_inner: &str) {
        let key = format!("Try_{snake}");
        if self.named_collection_decls.borrow().contains_key(&key) { return; }
        // `void` inner (from a bare `Err` placeholder) has no value field.
        let value_field = if c_inner == "void" {
            String::new()
        } else {
            format!("    {c_inner} value;\n")
        };
        let decl = format!(
            "typedef struct {{\n    int ok;\n{value_field}    const char* err;\n}} SirinTry_{snake};\n"
        );
        self.named_collection_decls.borrow_mut().insert(key, decl);
    }

    /// Register a synthetic typedef for an anonymous object-literal struct.
    fn register_anon_struct(&self, name: &str, body: &str) {
        let key = format!("Struct_{name}");
        if self.named_collection_decls.borrow().contains_key(&key) { return; }
        let decl = format!("typedef struct {{\n{}}} {};\n", body, name);
        self.named_collection_decls.borrow_mut().insert(key, decl);
    }

    /// Register a function-pointer typedef: `typedef R (*name)(A, B);`.
    fn register_fn_type(&self, name: &str, ret_c: &str, arg_cs: &[String]) {
        let key = format!("FnType_{name}");
        if self.named_collection_decls.borrow().contains_key(&key) { return; }
        let args = if arg_cs.is_empty() { "void".to_string() } else { arg_cs.join(", ") };
        let decl = format!("typedef {} (*{})({});\n", ret_c, name, args);
        self.named_collection_decls.borrow_mut().insert(key, decl);
    }

    pub fn finish(self) -> String {
        let prefix = self.defines_prefix();
        let named = self.named_collection_decls_string();
        let io = if self.io_imported { "#include <stdio.h>\n" } else { "" };
        let async_h = if self.async_imported { "#include \"sirin_async.h\"\n#include <stdlib.h>\n" } else { "" };
        let net_h = if self.net_imported { "#include \"sirin_net.h\"\n" } else { "" };
        format!("{}#include \"sirin_runtime.h\"\n{}{}{}{}\n{}", prefix, async_h, net_h, io, named, self.output)
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
