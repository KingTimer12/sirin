use sirin_diagnostics::report_error;
use sirin_parser::{
    expr::{BinOp, Expr},
    span::Spanned,
    stmt::Stmt,
    types::Type,
};

use crate::{env::Env, error::CheckerError};

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
            Stmt::Let { name, rhs } => {
                let ty = self.check_expr(rhs)?;
                self.env.define(name.node, ty);
                Ok(())
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

    fn check_expr(&self, expr: &Spanned<Expr<'a>>) -> Result<Type, CheckerError<'a>> {
        match &expr.node {
            Expr::Int(_) => Ok(Type::Int),
            Expr::Float(_) => Ok(Type::Float),
            Expr::Str(_) => Ok(Type::Str),
            Expr::Boolean(_) => Ok(Type::Bool),
            Expr::BinOp(op, left, right) => {
                let lhs_ty = self.check_expr(left)?;
                let rhs_ty = self.check_expr(right)?;

                if lhs_ty != rhs_ty {
                    return Err(CheckerError::TypeError(lhs_ty, rhs_ty));
                }

                match op {
                    BinOp::Add => match lhs_ty {
                        Type::Int | Type::Float => Ok(lhs_ty),
                        Type::Str => Ok(Type::Str),
                        _ => Err(CheckerError::InvalidOperation { op: op.clone(), ty: lhs_ty }),
                    },
                    BinOp::Sub | BinOp::Mul | BinOp::Div => match lhs_ty {
                        Type::Int | Type::Float => Ok(lhs_ty),
                        _ => Err(CheckerError::InvalidOperation { op: op.clone(), ty: lhs_ty }),
                    },
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
            Expr::Var(name) => self.env.get(name).cloned().ok_or_else(|| {
                report_error(
                    &expr.span.file,
                    self.src,
                    &expr.span,
                    "variável não declarada",
                    &format!("`{}` não foi declarada nesse escopo", name),
                );
                CheckerError::NameError(name)
            }),
            Expr::Call(_, _) => Ok(Type::Int), // TODO: resolução de fn
        }
    }
}
