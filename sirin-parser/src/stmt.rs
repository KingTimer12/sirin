use crate::{expr::Expr, types::Type};

#[derive(Debug)]
pub enum Stmt<'a> {
    Let {
        name: &'a str,
        rhs: Expr<'a>,
    },
    Fn {
        name: &'a str,
        args: Vec<(&'a str, Type)>,
        return_type: Option<Type>, 
        body: Vec<Stmt<'a>>,
    },
    Return {
        value: Option<Box<Expr<'a>>>,
        cond:  Option<Box<Expr<'a>>>,
    },
    Expr(Expr<'a>),
    If {
        cond: Box<Expr<'a>>,
        then: Vec<Stmt<'a>>,
        else_: Option<Vec<Stmt<'a>>>,
    },
}
