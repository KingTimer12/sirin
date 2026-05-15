use sirin_parser::{
    expr::{BinOp, Expr},
    stmt::Stmt,
    types::Type,
};

use crate::{env::Env, error::CheckerError};

pub struct Checker<'a> {
    pub(crate) env: Env<'a>,
}

impl<'a> Checker<'a> {
    pub fn check_stmt(&mut self, stmt: &'a Stmt<'a>) -> Result<(), CheckerError<'a>> {
        match stmt {
            Stmt::Let { name, rhs } => {
                // infere o tipo do lado direito
                let ty = self.check_expr(rhs)?;
                // registra a variável com o tipo inferido
                self.env.define(name, ty);
                Ok(())
            }
            Stmt::Fn {
                name,
                args,
                return_type,
                body,
            } => {
                self.env.push_scope();

                // registra os parâmetros no escopo
                for (arg_name, arg_ty) in args {
                    self.env.define(arg_name, arg_ty.clone());
                }

                // define o retorno esperado
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

                // escopo do then
                self.env.push_scope();
                for stmt in then {
                    self.check_stmt(stmt)?;
                }
                self.env.pop_scope();

                // escopo do else
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
                // verifica a condição se existir
                if let Some(cond_expr) = cond {
                    let cond_ty = self.check_expr(cond_expr)?;
                    if cond_ty != Type::Bool {
                        return Err(CheckerError::TypeError(Type::Bool, cond_ty));
                    }
                }

                // verifica o tipo do valor
                let return_ty = match value {
                    Some(expr) => self.check_expr(expr)?,
                    None => Type::Void,
                };

                match self.env.get_return() {
                    None => Err(CheckerError::ReturnOutsideFn),
                    Some(expected) => {
                        if return_ty != expected {
                            Err(CheckerError::TypeError(expected.clone(), return_ty))
                        } else {
                            Ok(())
                        }
                    }
                }
            }
            _ => Err(CheckerError::GenericError(format!(
                "expressão não suportada pelo typechecker ainda"
            ))),
        }
    }

    fn check_expr(&self, expr: &'a Expr<'a>) -> Result<Type, CheckerError<'a>> {
        match expr {
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
                        Type::Str => Ok(Type::Str), // concatenação
                        _ => Err(CheckerError::InvalidOperation {
                            op: op.clone(),
                            ty: lhs_ty,
                        }),
                    },
                    BinOp::Sub | BinOp::Mul | BinOp::Div => match lhs_ty {
                        Type::Int | Type::Float => Ok(lhs_ty),
                        _ => Err(CheckerError::InvalidOperation {
                            op: op.clone(),
                            ty: lhs_ty,
                        }),
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

            Expr::Var(name) => self
                .env
                .get(name)
                .cloned()
                .ok_or(CheckerError::NameError(name)),

            _ => Ok(Type::Int),
        }
    }
}
