use crate::span::Spanned;

#[derive(Debug, Clone)]
pub enum Expr<'a> {
    Int(i64),
    Float(f64),
    Str(&'a str),
    Boolean(bool),
    Var(&'a str),
    Await(Box<Spanned<Expr<'a>>>),
    Neg(Box<Spanned<Expr<'a>>>),
    Not(Box<Spanned<Expr<'a>>>),
    BinOp(BinOp, Box<Spanned<Expr<'a>>>, Box<Spanned<Expr<'a>>>),
    Call(&'a str, Vec<Spanned<Expr<'a>>>),
    Array(Vec<Spanned<Expr<'a>>>),
    Index(Box<Spanned<Expr<'a>>>, Box<Spanned<Expr<'a>>>),
    MethodCall(Box<Spanned<Expr<'a>>>, &'a str, Vec<Spanned<Expr<'a>>>),
    FieldAccess(Box<Spanned<Expr<'a>>>, &'a str),
    New(&'a str, Vec<Spanned<Expr<'a>>>),
    NewDefault(&'a str),
    NewFields(&'a str, Vec<(&'a str, Spanned<Expr<'a>>)>),
    // anonymous object literal: { nome: "Julius", idade: 24 }
    ObjectLiteral(Vec<(&'a str, Spanned<Expr<'a>>)>),
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
