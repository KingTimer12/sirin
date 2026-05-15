use crate::span::Spanned;

#[derive(Debug, Clone)]
pub enum Expr<'a> {
    Int(i64),
    Float(f64),
    Str(&'a str),
    Boolean(bool),
    Var(&'a str),
    Neg(Box<Spanned<Expr<'a>>>),
    Not(Box<Spanned<Expr<'a>>>),
    BinOp(BinOp, Box<Spanned<Expr<'a>>>, Box<Spanned<Expr<'a>>>),
    Call(&'a str, Vec<Spanned<Expr<'a>>>),
}

#[derive(Debug, Clone)]
pub enum BinOp {
    Add,   // +
    Sub,   // -
    Mul,   // *
    Div,   // /
    Gt,    // >
    Lt,    // <
    GtEq,  // >=
    LtEq,  // <=
    Eq,    // ==
    NotEq, // !=
    And,   // and
    Or,    // or
}
