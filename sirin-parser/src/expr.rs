use crate::stmt::Stmt;

#[derive(Debug)]
pub enum Expr<'a> {
    Int(i64),
    Float(f64),
    Boolean(bool),
    Str(&'a str),
    Var(&'a str),
    Neg(Box<Expr<'a>>),
    BinOp(BinOp, Box<Expr<'a>>, Box<Expr<'a>>),
    Call(&'a str, Vec<Expr<'a>>),
    If(Box<Expr<'a>>, Vec<Stmt<'a>>, Vec<Stmt<'a>>),
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
