use crate::expr::Expr;

#[derive(Debug)]
pub enum Stmt<'a> {
    Let {
        name: &'a str,
        rhs: Expr<'a>,
    },
    Fn {
        name: &'a str,
        args: Vec<&'a str>,
        body: Vec<Stmt<'a>>,
    },
    Return(Option<Box<Expr<'a>>>),
    Expr(Expr<'a>),
    If {
        cond: Box<Expr<'a>>,
        then: Vec<Stmt<'a>>,
        else_: Option<Vec<Stmt<'a>>>,
    },
}
