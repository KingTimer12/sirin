use std::collections::HashMap;

use sirin_parser::{
    expr::{BinOp, Expr},
    span::Spanned,
    stmt::Stmt,
    types::Type,
};

pub struct Emitter {
    output: String,
    depth: usize,
    vars: HashMap<String, Type>,
    fns: HashMap<String, Type>,
    current_fn: Option<String>,
    current_fn_params: Vec<String>,
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

impl Emitter {
    pub fn new() -> Self {
        Self {
            output: String::new(),
            depth: 0,
            vars: HashMap::new(),
            fns: HashMap::new(),
            current_fn: None,
            current_fn_params: Vec::new(),
        }
    }

    pub fn emit_stmt<'a>(&mut self, stmt: &'a Stmt<'a>) {
        match stmt {
            Stmt::Let { name, ty: declared, rhs } | Stmt::CopyLet { name, ty: declared, rhs } => {
                let inferred = self.get_expr(&rhs.node);
                let ty = declared.as_ref().unwrap_or(&inferred);

                // Array literal [a, b, c] → type-specific new + repeated push
                if let Expr::Array(items) = &rhs.node {
                    let (kind, snake) = match ty {
                        Type::Array(inner) => ("array", collection_suffix(inner).1),
                        Type::Vec(inner)   => ("vec",   collection_suffix(inner).1),
                        _ => ("array", "int"),
                    };
                    let c_ty = self.type_to_c(ty);
                    self.vars.insert(name.node.to_string(), ty.clone());
                    self.output.push_str(&format!(
                        "{}{} {} = sirin_{}_{}_new({});\n",
                        self.indent(), c_ty, name.node, kind, snake, items.len()
                    ));
                    for item in items {
                        let val = self.emit_expr(&item.node);
                        self.output.push_str(&format!(
                            "{}sirin_{}_{}_push(&{}, {});\n",
                            self.indent(), kind, snake, name.node, val
                        ));
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
            } => {
                let fn_name = name.node;

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
                let val = self.emit_expr(&expr.node);
                self.output.push_str(&format!("{}{};\n", self.indent(), val));
            }
        }
    }

    fn emit_expr<'a>(&self, expr: &'a Expr<'a>) -> String {
        match expr {
            Expr::Int(x) => format!("{}", x),
            Expr::Float(x) => format!("{}", x),
            Expr::Boolean(x) => (if *x { "1" } else { "0" }).to_string(),
            Expr::Str(x) => format!("\"{}\"", x),
            Expr::Var(name) => name.to_string(),
            Expr::Call(name, args) => {
                let args_str = args
                    .iter()
                    .map(|a| self.emit_expr(&a.node))
                    .collect::<Vec<_>>()
                    .join(", ");
                // Fallback constructors when no declared type is available.
                // emit_stmt handles the typed case; these defaults use Int/Int.
                match *name {
                    "Vec"   => format!("sirin_vec_int_new({})", args_str),
                    "Array" => format!("sirin_array_int_new({})", args_str),
                    "Map"   => "sirin_map_str_int_new()".to_string(),
                    "Set"   => "sirin_set_int_new()".to_string(),
                    _       => format!("{}({})", name, args_str),
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
                    Type::Array(inner) => {
                        let (_, s) = collection_suffix(&inner);
                        format!("sirin_array_{}_get(&{}, {})", s, b, i)
                    }
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
                    _ => format!("{}.{}({})", obj_str, method, args_str),
                }
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
        match ty {
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
            Type::Vec(inner) => format!("SirinVec{}", collection_suffix(inner).0),
            Type::Array(inner) => format!("SirinArray{}", collection_suffix(inner).0),
            Type::Set(inner) => format!("SirinSet{}", collection_suffix(inner).0),
            Type::Map(_, val) => format!("SirinMapStr{}", collection_suffix(val).0),
        }
    }

    fn get_expr<'a>(&self, expr: &'a Expr<'a>) -> Type {
        match expr {
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
            Expr::Call(name, _) => self.fns.get(*name).cloned().unwrap_or(Type::Void),
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
                    _ => Type::Void,
                }
            }
        }
    }

    pub fn emit_program<'a>(mut self, stmts: &'a [Spanned<Stmt<'a>>]) -> String {
        self.emit_top_level(stmts);
        self.finish()
    }

    /// Like `emit_program` but uses inline typedefs instead of `#include <stdint.h>`.
    /// Use this when passing the output directly to TCC's `compile_string`.
    pub fn emit_program_tcc<'a>(mut self, stmts: &'a [Spanned<Stmt<'a>>]) -> String {
        self.emit_top_level(stmts);
        format!("typedef long long int64_t;\n\n{}", self.output)
    }

    /// Emits function declarations at global scope and wraps all other
    /// top-level statements inside `int main(void)`.
    fn emit_top_level<'a>(&mut self, stmts: &'a [Spanned<Stmt<'a>>]) {
        let (fns, rest): (Vec<_>, Vec<_>) = stmts
            .iter()
            .partition(|s| matches!(s.node, Stmt::Fn { .. }));

        for s in &fns {
            self.emit_stmt(&s.node);
        }

        if !rest.is_empty() {
            self.output.push_str("int main(void) {\n");
            self.depth += 1;
            for s in &rest {
                self.emit_stmt(&s.node);
            }
            self.depth -= 1;
            self.output.push_str("    return 0;\n}\n");
        }
    }

    fn indent(&self) -> String {
        "    ".repeat(self.depth)
    }

    pub fn finish(self) -> String {
        format!("#include \"sirin_runtime.h\"\n\n{}", self.output)
    }
}
