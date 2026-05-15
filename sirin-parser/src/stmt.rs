use crate::{expr::Expr, span::Spanned, types::Type};

#[derive(Debug)]
pub enum Stmt<'a> {
    Let {
        name: Spanned<&'a str>,
        rhs:  Spanned<Expr<'a>>,
    },
    CopyLet {
        name: Spanned<&'a str>,
        rhs:  Spanned<Expr<'a>>,
    },
    Fn {
        name:        Spanned<&'a str>,
        args:        Vec<(Spanned<&'a str>, Type)>,
        return_type: Option<Type>,
        body:        Vec<Spanned<Stmt<'a>>>,
    },
    Return {
        value: Option<Box<Spanned<Expr<'a>>>>,
        cond:  Option<Box<Spanned<Expr<'a>>>>,
    },
    If {
        cond:  Box<Spanned<Expr<'a>>>,
        then:  Vec<Spanned<Stmt<'a>>>,
        else_: Option<Vec<Spanned<Stmt<'a>>>>,
    },
    Expr(Spanned<Expr<'a>>),
}
