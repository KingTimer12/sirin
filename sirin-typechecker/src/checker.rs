use std::collections::{HashMap, HashSet};

use sirin_diagnostics::report_error;
use sirin_parser::{
    expr::{BinOp, Expr},
    span::Spanned,
    stmt::{ImplTarget, Stmt},
    types::Type,
};

use crate::{env::{Env, OwnershipState}, error::CheckerError};

// ── Class registry ────────────────────────────────────────────────────────────

struct FieldInfo {
    ty: Type,
}

struct MethodInfo {
    return_type: Type,
}

struct ClassInfo {
    fields: Vec<(String, FieldInfo)>,  // ordered
    methods: HashMap<String, MethodInfo>,
    extends: Option<String>,
}

impl ClassInfo {
    fn field_type(&self, name: &str) -> Option<&Type> {
        self.fields.iter().find(|(n, _)| n.as_str() == name).map(|(_, f)| &f.ty)
    }
}

struct InterfaceMethodInfo {
    name: String,
}

// ── Checker ───────────────────────────────────────────────────────────────────

pub struct Checker<'a> {
    pub src: &'a str,
    pub(crate) env: Env<'a>,
    classes: HashMap<String, ClassInfo>,
    interfaces: HashMap<String, Vec<InterfaceMethodInfo>>,
    primitive_impls: HashMap<String, HashMap<String, MethodInfo>>,
    imported_modules: HashSet<String>,
    /// return types of functions declared in imported local modules
    fns: HashMap<String, sirin_parser::types::Type>,
    /// names of private (_-prefixed) functions from imported modules
    module_private_fns: HashSet<String>,
}

impl<'a> Checker<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            env: Env::new(),
            classes: HashMap::new(),
            interfaces: HashMap::new(),
            primitive_impls: HashMap::new(),
            imported_modules: HashSet::new(),
            fns: HashMap::new(),
            module_private_fns: HashSet::new(),
        }
    }

    /// Register exported symbols from a parsed local module before checking the main file.
    /// Private symbols (name starts with `_`) are tracked but not exported.
    pub fn import_module(&mut self, stmts: &[Spanned<Stmt<'_>>]) {
        for s in stmts {
            match &s.node {
                Stmt::Fn { name, return_type, .. } => {
                    let fn_name = name.node.to_string();
                    if fn_name.starts_with('_') {
                        self.module_private_fns.insert(fn_name);
                    } else {
                        self.fns.insert(fn_name, return_type.clone().unwrap_or(Type::Void));
                    }
                }
                Stmt::Class { name, fields, methods, extends, .. } => {
                    if name.node.starts_with('_') {
                        continue;
                    }
                    let mut method_infos: HashMap<String, MethodInfo> = HashMap::new();
                    for m in methods {
                        if let Stmt::Fn { name: mn, return_type, .. } | Stmt::AbstractFn { name: mn, return_type, .. } = &m.node {
                            method_infos.insert(mn.node.to_string(), MethodInfo {
                                return_type: return_type.clone().unwrap_or(Type::Void),
                            });
                        }
                    }
                    self.classes.insert(name.node.to_string(), ClassInfo {
                        fields: fields.iter()
                            .map(|f| (f.name.node.to_string(), FieldInfo { ty: f.ty.clone() }))
                            .collect(),
                        methods: method_infos,
                        extends: extends.as_ref().map(|e| e.node.to_string()),
                    });
                }
                _ => {}
            }
        }
    }

    fn impl_target_type(target: &ImplTarget) -> Type {
        match target {
            ImplTarget::Named(n) => Type::Named(n.to_string()),
            ImplTarget::Int  => Type::Int,
            ImplTarget::Float => Type::Float,
            ImplTarget::Str  => Type::Str,
            ImplTarget::Bool => Type::Bool,
            ImplTarget::U8   => Type::U8,
            ImplTarget::U16  => Type::U16,
            ImplTarget::U32  => Type::U32,
            ImplTarget::U64  => Type::U64,
            ImplTarget::I8   => Type::I8,
            ImplTarget::I16  => Type::I16,
            ImplTarget::I32  => Type::I32,
            ImplTarget::I64  => Type::I64,
        }
    }

    fn prim_key(ty: &Type) -> Option<&'static str> {
        match ty {
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
            _                     => None,
        }
    }

    pub fn check_stmt(&mut self, stmt: &Spanned<Stmt<'a>>) -> Result<(), CheckerError<'a>> {
        match &stmt.node {
            Stmt::Let { name, ty: declared, rhs } => {
                match self.check_expr(rhs) {
                    Ok(rhs_ty) => {
                        let env_ty = if let Some(decl) = declared {
                            // int literals produce Type::Int; allow assign to any integer width.
                            // array/vec literals produce Array(Int); allow Array(U8) etc.
                            let coll_compat = match (&rhs_ty, decl) {
                                (Type::Array(ri) | Type::Vec(ri),
                                 Type::Array(di) | Type::Vec(di)) => {
                                    ri.is_integer() && di.is_integer()
                                }
                                (Type::Set(ri), Type::Set(di)) => ri.is_integer() && di.is_integer(),
                                _ => false,
                            };
                            let ok = rhs_ty == *decl
                                || (decl.is_integer() && rhs_ty.is_integer())
                                || rhs_ty == Type::Void
                                || coll_compat
                                // Named types: allow when names match
                                || matches!((&rhs_ty, decl), (Type::Named(a), Type::Named(b)) if a == b);
                            if !ok {
                                self.env.define(name.node, Type::Void);
                                return Err(CheckerError::TypeError(decl.clone(), rhs_ty));
                            }
                            decl.clone()
                        } else {
                            if let Expr::Var(src_name) = &rhs.node {
                                if !rhs_ty.is_copy() {
                                    self.env.mark_moved(src_name, name.node.to_string());
                                }
                            }
                            rhs_ty
                        };
                        self.env.define(name.node, env_ty);
                        Ok(())
                    }
                    Err(e) => {
                        self.env.define(name.node, Type::Void);
                        Err(e)
                    }
                }
            }
            Stmt::CopyLet { name, ty: declared, rhs } => {
                match self.check_expr(rhs) {
                    Ok(rhs_ty) => {
                        let env_ty = if let Some(decl) = declared {
                            let coll_compat = match (&rhs_ty, decl) {
                                (Type::Array(ri) | Type::Vec(ri),
                                 Type::Array(di) | Type::Vec(di)) => {
                                    ri.is_integer() && di.is_integer()
                                }
                                (Type::Set(ri), Type::Set(di)) => ri.is_integer() && di.is_integer(),
                                _ => false,
                            };
                            let ok = rhs_ty == *decl
                                || (decl.is_integer() && rhs_ty.is_integer())
                                || rhs_ty == Type::Void
                                || coll_compat;
                            if !ok {
                                self.env.define(name.node, Type::Void);
                                return Err(CheckerError::TypeError(decl.clone(), rhs_ty));
                            }
                            decl.clone()
                        } else {
                            rhs_ty
                        };
                        self.env.define(name.node, env_ty);
                        Ok(())
                    }
                    Err(e) => {
                        self.env.define(name.node, Type::Void);
                        Err(e)
                    }
                }
            }
            Stmt::Fn { name: fn_decl_name, args, return_type, body } => {
                self.fns.insert(
                    fn_decl_name.node.to_string(),
                    return_type.clone().unwrap_or(Type::Void),
                );
                self.env.push_scope();

                for (arg_name, arg_ty) in args {
                    self.env.define(arg_name.node, arg_ty.clone());
                }

                self.env.set_return(return_type.clone());

                for stmt in body {
                    self.check_stmt(stmt)?;
                }

                self.env.pop_scope();
                self.env.set_return(None);
                Ok(())
            }
            Stmt::If { cond, then, else_ } => {
                let cond_ty = self.check_expr(cond)?;
                if cond_ty != Type::Bool {
                    return Err(CheckerError::TypeError(Type::Bool, cond_ty));
                }

                self.env.push_scope();
                for stmt in then {
                    self.check_stmt(stmt)?;
                }
                self.env.pop_scope();

                if let Some(else_stmts) = else_ {
                    self.env.push_scope();
                    for stmt in else_stmts {
                        self.check_stmt(stmt)?;
                    }
                    self.env.pop_scope();
                }

                Ok(())
            }
            Stmt::Return { value, cond } => {
                if let Some(cond_expr) = cond {
                    let cond_ty = self.check_expr(cond_expr)?;
                    if cond_ty != Type::Bool {
                        return Err(CheckerError::TypeError(Type::Bool, cond_ty));
                    }
                }

                let return_ty = match value {
                    Some(expr) => self.check_expr(expr)?,
                    None => Type::Void,
                };

                match self.env.get_return() {
                    None => Err(CheckerError::ReturnOutsideFn),
                    Some(expected) => {
                        if return_ty != expected {
                            Err(CheckerError::TypeError(expected, return_ty))
                        } else {
                            Ok(())
                        }
                    }
                }
            }
            Stmt::Expr(expr) => {
                self.check_expr(expr)?;
                Ok(())
            }

            // ── Class declaration ─────────────────────────────────────────────
            Stmt::Class { name, fields, methods, is_, extends, .. } => {
                let class_name = name.node;

                // Register with fields first (methods resolved after)
                self.classes.insert(class_name.to_string(), ClassInfo {
                    fields: fields.iter()
                        .map(|f| (f.name.node.to_string(), FieldInfo { ty: f.ty.clone() }))
                        .collect(),
                    methods: HashMap::new(),
                    extends: extends.as_ref().map(|e| e.node.to_string()),
                });

                // collect inherited fields (parent chain already registered)
                let inherited: Vec<(String, Type)> = {
                    let mut acc = vec![];
                    let mut cur = extends.as_ref().map(|e| e.node.to_string());
                    while let Some(pname) = cur {
                        if let Some(pci) = self.classes.get(pname.as_str()) {
                            for (fname, finfo) in &pci.fields {
                                acc.push((fname.clone(), finfo.ty.clone()));
                            }
                            cur = pci.extends.clone();
                        } else {
                            break;
                        }
                    }
                    acc
                };

                let mut method_infos: HashMap<String, MethodInfo> = HashMap::new();

                for method_stmt in methods {
                    match &method_stmt.node {
                        Stmt::Fn { name: mname, args, return_type, body } => {
                            self.env.push_scope();
                            self.env.define("self", Type::Named(class_name.to_string()));
                            for f in fields {
                                self.env.define(f.name.node, f.ty.clone());
                            }
                            for (fname, fty) in &inherited {
                                self.env.define(Box::leak(fname.clone().into_boxed_str()), fty.clone());
                            }
                            for (aname, aty) in args {
                                self.env.define(aname.node, aty.clone());
                            }
                            self.env.set_return(return_type.clone());
                            for s in body {
                                self.check_stmt(s)?;
                            }
                            self.env.pop_scope();
                            self.env.set_return(None);
                            method_infos.insert(mname.node.to_string(), MethodInfo {
                                return_type: return_type.clone().unwrap_or(Type::Void),
                            });
                        }
                        Stmt::Init { args, body } => {
                            self.env.push_scope();
                            self.env.define("self", Type::Named(class_name.to_string()));
                            for f in fields {
                                self.env.define(f.name.node, f.ty.clone());
                            }
                            for (fname, fty) in &inherited {
                                self.env.define(Box::leak(fname.clone().into_boxed_str()), fty.clone());
                            }
                            for (aname, aty) in args {
                                self.env.define(aname.node, aty.clone());
                            }
                            for s in body {
                                self.check_stmt(s)?;
                            }
                            self.env.pop_scope();
                        }
                        Stmt::Default { body } => {
                            self.env.push_scope();
                            self.env.define("self", Type::Named(class_name.to_string()));
                            for f in fields {
                                self.env.define(f.name.node, f.ty.clone());
                            }
                            for (fname, fty) in &inherited {
                                self.env.define(Box::leak(fname.clone().into_boxed_str()), fty.clone());
                            }
                            for s in body {
                                self.check_stmt(s)?;
                            }
                            self.env.pop_scope();
                        }
                        Stmt::AbstractFn { name: mname, return_type, .. } => {
                            method_infos.insert(mname.node.to_string(), MethodInfo {
                                return_type: return_type.clone().unwrap_or(Type::Void),
                            });
                        }
                        _ => {}
                    }
                }

                if let Some(ci) = self.classes.get_mut(class_name) {
                    ci.methods = method_infos;
                }

                // Validate that all interface methods are implemented
                for iface in is_ {
                    if let Some(iface_methods) = self.interfaces.get(iface.node) {
                        for im in iface_methods {
                            if let Some(ci) = self.classes.get(class_name) {
                                if !ci.methods.contains_key(&im.name) {
                                    report_error(
                                        &iface.span.file,
                                        self.src,
                                        &iface.span,
                                        "interface not implemented",
                                        &format!(
                                            "`{}` requires `{}` but `{}` does not define it",
                                            iface.node, im.name, class_name
                                        ),
                                    );
                                    return Err(CheckerError::MissingInterfaceMethod {
                                        class: class_name.to_string(),
                                        interface: iface.node.to_string(),
                                        method: im.name.clone(),
                                    });
                                }
                            }
                        }
                    }
                }
                Ok(())
            }

            Stmt::Interface { name, methods } => {
                let infos = methods.iter()
                    .map(|m| InterfaceMethodInfo { name: m.name.node.to_string() })
                    .collect();
                self.interfaces.insert(name.node.to_string(), infos);
                Ok(())
            }

            Stmt::Impl { target, methods } => {
                let self_ty = Self::impl_target_type(target);

                // For named targets, collect field types to inject into method scope
                let class_fields: Vec<(String, Type)> = if let ImplTarget::Named(cls) = target {
                    self.classes.get(*cls)
                        .map(|ci| ci.fields.iter().map(|(n, f)| (n.clone(), f.ty.clone())).collect())
                        .unwrap_or_default()
                } else {
                    vec![]
                };

                for method_stmt in methods {
                    if let Stmt::Fn { name: mname, args, return_type, body } = &method_stmt.node {
                        self.env.push_scope();
                        self.env.define("self", self_ty.clone());
                        for (fname, fty) in &class_fields {
                            self.env.define(
                                // SAFETY: field names are &'a str stored in ClassInfo — we need
                                // to leak to get 'a lifetime; ClassInfo owns String, not &str
                                // so we use a workaround: store in env as owned key
                                Box::leak(fname.clone().into_boxed_str()),
                                fty.clone(),
                            );
                        }
                        for (aname, aty) in args {
                            self.env.define(aname.node, aty.clone());
                        }
                        self.env.set_return(return_type.clone());
                        for s in body {
                            self.check_stmt(s)?;
                        }
                        self.env.pop_scope();
                        self.env.set_return(None);

                        let ret = return_type.clone().unwrap_or(Type::Void);
                        let method_info = MethodInfo { return_type: ret };

                        match target {
                            ImplTarget::Named(cls) => {
                                if let Some(ci) = self.classes.get_mut(*cls) {
                                    ci.methods.insert(mname.node.to_string(), method_info);
                                }
                            }
                            _ => {
                                if let Some(key) = Self::prim_key(&self_ty) {
                                    self.primitive_impls
                                        .entry(key.to_string())
                                        .or_default()
                                        .insert(mname.node.to_string(), method_info);
                                }
                            }
                        }
                    }
                }
                Ok(())
            }

            Stmt::Use { path } => {
                let module = path.join(".");
                self.imported_modules.insert(module);
                Ok(())
            }

            _ => Ok(()),
        }
    }

    fn check_expr(&mut self, expr: &Spanned<Expr<'a>>) -> Result<Type, CheckerError<'a>> {
        match &expr.node {
            Expr::Int(_) => Ok(Type::Int),
            Expr::Float(_) => Ok(Type::Float),
            Expr::Str(_) => Ok(Type::Str),
            Expr::Boolean(_) => Ok(Type::Bool),
            Expr::BinOp(op, left, right) => {
                let lhs_ty = self.check_expr(left)?;
                let rhs_ty = self.check_expr(right)?;

                // suprimir cascata de erros anteriores
                if lhs_ty == Type::Void || rhs_ty == Type::Void {
                    return Ok(Type::Void);
                }

                if lhs_ty != rhs_ty {
                    // mixing any integer widths (U8, U16, …, Int) is allowed
                    if !(lhs_ty.is_integer() && rhs_ty.is_integer()) {
                        report_error(
                            &expr.span.file,
                            self.src,
                            &expr.span,
                            "incompatible types",
                            &format!("expected `{:?}`, found `{:?}`", lhs_ty, rhs_ty),
                        );
                        return Err(CheckerError::TypeError(lhs_ty, rhs_ty));
                    }
                }

                match op {
                    BinOp::Add => {
                        if lhs_ty.is_integer() {
                            return Ok(lhs_ty);
                        }
                        match lhs_ty {
                            Type::Float => Ok(Type::Float),
                            Type::Str => Ok(Type::Str),
                            _ => Err(CheckerError::InvalidOperation { op: op.clone(), ty: lhs_ty }),
                        }
                    }
                    BinOp::Sub | BinOp::Mul | BinOp::Div => {
                        if lhs_ty.is_integer() {
                            return Ok(lhs_ty);
                        }
                        match lhs_ty {
                            Type::Float => Ok(Type::Float),
                            _ => Err(CheckerError::InvalidOperation { op: op.clone(), ty: lhs_ty }),
                        }
                    }
                    BinOp::Eq
                    | BinOp::NotEq
                    | BinOp::Gt
                    | BinOp::Lt
                    | BinOp::GtEq
                    | BinOp::LtEq => Ok(Type::Bool),
                    BinOp::And | BinOp::Or => Ok(Type::Bool),
                }
            }
            Expr::Neg(rhs) => self.check_expr(rhs),
            Expr::Not(rhs) => {
                let ty = self.check_expr(rhs)?;
                if ty != Type::Bool {
                    return Err(CheckerError::TypeError(Type::Bool, ty));
                }
                Ok(Type::Bool)
            }
            Expr::Var(name) => {
                if let Some(state) = self.env.get_ownership(name) {
                    if let OwnershipState::Moved { to } = state {
                        let moved_to = to.clone();
                        report_error(
                            &expr.span.file,
                            self.src,
                            &expr.span,
                            "use after move",
                            &format!("`{}` was moved to `{}`", name, moved_to),
                        );
                        return Err(CheckerError::UseAfterMove { var: name, moved_to });
                    }
                }
                self.env.get(name).cloned().ok_or_else(|| {
                    report_error(
                        &expr.span.file,
                        self.src,
                        &expr.span,
                        "undeclared variable",
                        &format!("`{}` is not declared in this scope", name),
                    );
                    CheckerError::NameError(name)
                })
            }
            Expr::Call(name, args) => match *name {
                "Vec" | "Array" => {
                    // Optional capacity arg must be integer
                    for arg in args {
                        let ty = self.check_expr(arg)?;
                        if ty != Type::Void && !ty.is_integer() {
                            report_error(
                                &arg.span.file, self.src, &arg.span,
                                "invalid constructor argument",
                                &format!("capacity must be an integer, found `{:?}`", ty),
                            );
                            return Err(CheckerError::TypeError(Type::Int, ty));
                        }
                    }
                    Ok(Type::Void) // declared type wins in typed-let
                }
                "Map" | "Set" => {
                    for arg in args { self.check_expr(arg)?; }
                    Ok(Type::Void)
                }
                "print" | "println" => {
                    if !self.imported_modules.contains("sirin.io") {
                        return Err(CheckerError::ModuleNotImported {
                            module: "sirin.io".to_string(),
                            function: name.to_string(),
                        });
                    }
                    for arg in args { self.check_expr(arg)?; }
                    Ok(Type::Void)
                }
                "readln" => {
                    if !self.imported_modules.contains("sirin.io") {
                        return Err(CheckerError::ModuleNotImported {
                            module: "sirin.io".to_string(),
                            function: "readln".to_string(),
                        });
                    }
                    Ok(Type::Str)
                }
                _ => {
                    if self.module_private_fns.contains(*name) {
                        return Err(CheckerError::PrivateAccess {
                            name: name.to_string(),
                            module: "módulo importado".to_string(),
                        });
                    }
                    if let Some(ret) = self.fns.get(*name) {
                        return Ok(ret.clone());
                    }
                    Ok(Type::Int) // TODO: resolução de fn local
                }
            },
            Expr::Array(items) => {
                if items.is_empty() {
                    return Ok(Type::Array(Box::new(Type::Void)));
                }
                let inner = self.check_expr(&items[0])?;
                for item in &items[1..] {
                    let ty = self.check_expr(item)?;
                    if ty != inner && !(ty.is_integer() && inner.is_integer()) {
                        report_error(
                            &item.span.file, self.src, &item.span,
                            "inconsistent type in array literal",
                            &format!("expected `{:?}`, found `{:?}`", inner, ty),
                        );
                        return Err(CheckerError::TypeError(inner, ty));
                    }
                }
                Ok(Type::Array(Box::new(inner)))
            }
            Expr::Index(base, idx) => {
                let base_ty = self.check_expr(base)?;
                self.check_expr(idx)?;
                match base_ty {
                    Type::Array(inner) | Type::Vec(inner) => Ok(*inner),
                    _ => Ok(Type::Int),
                }
            }
            Expr::MethodCall(obj, method, args) => {
                let obj_ty = self.check_expr(obj)?;

                // Cascade suppression: if object has error type, skip arg checks
                if obj_ty == Type::Void {
                    for arg in args { self.check_expr(arg)?; }
                    return Ok(Type::Void);
                }

                match &obj_ty {
                    Type::Vec(inner) | Type::Array(inner) => match *method {
                        "push" => {
                            if let Some(val) = args.first() {
                                let got = self.check_expr(val)?;
                                if got != Type::Void && got != **inner
                                    && !(got.is_integer() && inner.is_integer())
                                {
                                    report_error(
                                        &val.span.file, self.src, &val.span,
                                        "wrong type for push",
                                        &format!("expected `{:?}`, found `{:?}`", inner, got),
                                    );
                                    return Err(CheckerError::TypeError(*inner.clone(), got));
                                }
                            }
                            Ok(Type::Void)
                        }
                        _ => {
                            for arg in args { self.check_expr(arg)?; }
                            Ok(*inner.clone())
                        }
                    },
                    Type::Map(key_ty, val_ty) => match *method {
                        "insert" => {
                            if let Some(k) = args.first() {
                                let got = self.check_expr(k)?;
                                if got != Type::Void && got != **key_ty
                                    && !(got.is_integer() && key_ty.is_integer())
                                {
                                    report_error(
                                        &k.span.file, self.src, &k.span,
                                        "wrong map key type",
                                        &format!("expected `{:?}`, found `{:?}`", key_ty, got),
                                    );
                                    return Err(CheckerError::TypeError(*key_ty.clone(), got));
                                }
                            }
                            if let Some(v) = args.get(1) {
                                let got = self.check_expr(v)?;
                                if got != Type::Void && got != **val_ty
                                    && !(got.is_integer() && val_ty.is_integer())
                                {
                                    report_error(
                                        &v.span.file, self.src, &v.span,
                                        "wrong map value type",
                                        &format!("expected `{:?}`, found `{:?}`", val_ty, got),
                                    );
                                    return Err(CheckerError::TypeError(*val_ty.clone(), got));
                                }
                            }
                            Ok(Type::Void)
                        }
                        "get" => {
                            if let Some(k) = args.first() {
                                let got = self.check_expr(k)?;
                                if got != Type::Void && got != **key_ty
                                    && !(got.is_integer() && key_ty.is_integer())
                                {
                                    report_error(
                                        &k.span.file, self.src, &k.span,
                                        "wrong map key type",
                                        &format!("expected `{:?}`, found `{:?}`", key_ty, got),
                                    );
                                    return Err(CheckerError::TypeError(*key_ty.clone(), got));
                                }
                            }
                            Ok(*val_ty.clone())
                        }
                        _ => {
                            for arg in args { self.check_expr(arg)?; }
                            Ok(*val_ty.clone())
                        }
                    },
                    Type::Set(inner) => match *method {
                        "insert" | "contains" => {
                            if let Some(val) = args.first() {
                                let got = self.check_expr(val)?;
                                if got != Type::Void && got != **inner
                                    && !(got.is_integer() && inner.is_integer())
                                {
                                    report_error(
                                        &val.span.file, self.src, &val.span,
                                        "wrong type for set",
                                        &format!("expected `{:?}`, found `{:?}`", inner, got),
                                    );
                                    return Err(CheckerError::TypeError(*inner.clone(), got));
                                }
                            }
                            if *method == "insert" { Ok(Type::Void) } else { Ok(Type::Bool) }
                        }
                        _ => {
                            for arg in args { self.check_expr(arg)?; }
                            Ok(Type::Void)
                        }
                    },
                    // User-defined class method calls
                    Type::Named(cls) => {
                        for arg in args { self.check_expr(arg)?; }
                        if let Some(class_info) = self.classes.get(cls.as_str()) {
                            if let Some(mi) = class_info.methods.get(*method) {
                                return Ok(mi.return_type.clone());
                            }
                        }
                        Ok(Type::Void)
                    }
                    _ => {
                        for arg in args { self.check_expr(arg)?; }
                        // primitive impl lookup
                        if let Some(key) = Self::prim_key(&obj_ty) {
                            if let Some(prim_methods) = self.primitive_impls.get(key) {
                                if let Some(mi) = prim_methods.get(*method) {
                                    return Ok(mi.return_type.clone());
                                }
                            }
                        }
                        Ok(Type::Void)
                    }
                }
            }

            // ── Class / OOP expressions ───────────────────────────────────────
            Expr::FieldAccess(obj, field) => {
                let obj_ty = self.check_expr(obj)?;
                if let Type::Named(cls) = &obj_ty {
                    let mut current = Some(cls.clone());
                    while let Some(cls_name) = current {
                        if let Some(class_info) = self.classes.get(cls_name.as_str()) {
                            if let Some(ty) = class_info.field_type(field) {
                                return Ok(ty.clone());
                            }
                            current = class_info.extends.clone();
                        } else {
                            break;
                        }
                    }
                }
                Ok(Type::Void)
            }
            Expr::New(name, args) => {
                for arg in args { self.check_expr(arg)?; }
                Ok(Type::Named(name.to_string()))
            }
            Expr::NewDefault(name) => Ok(Type::Named(name.to_string())),
            Expr::NewFields(name, fields) => {
                for (_, val) in fields { self.check_expr(val)?; }
                Ok(Type::Named(name.to_string()))
            }
        }
    }
}
