use sirin_diagnostics::report_error;
use sirin_parser::{
    expr::{BinOp, Expr},
    span::Spanned,
    stmt::Stmt,
    types::Type,
};

use crate::{env::{Env, OwnershipState}, error::CheckerError};

pub struct Checker<'a> {
    pub src: &'a str,
    pub(crate) env: Env<'a>,
}

impl<'a> Checker<'a> {
    pub fn new(src: &'a str) -> Self {
        Self { src, env: Env::new() }
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
                                || coll_compat;
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
            Stmt::Fn { args, return_type, body, .. } => {
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
                _ => Ok(Type::Int), // TODO: resolução de fn
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
                    _ => {
                        for arg in args { self.check_expr(arg)?; }
                        Ok(Type::Void)
                    }
                }
            }
        }
    }
}
