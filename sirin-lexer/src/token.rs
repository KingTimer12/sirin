use logos::Logos;
use sirin_diagnostics::span::Span;

use crate::{error::LexingError, span::SpannedToken};

#[derive(Logos, Debug, PartialEq, Clone)]
#[logos(error(LexingError, LexingError::from_lexer))]
pub enum Tokens<'a> {
    #[regex(r"[ \t\n]+")]
    Whitespace,
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice())]
    Ident(&'a str),
    #[token(",")]
    Comma,
    #[token("=")]
    Assign,
    #[token("return")]
    Return,
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("and")]
    And,
    #[token("or")]
    Or,
    #[token("!")]
    Not,

    // Op
    #[token("==")]
    Eq,
    #[token("!=")]
    NotEq,
    #[token(">=")]
    GtEq,
    #[token("<=")]
    LtEq,
    #[token(">")]
    Gt,
    #[token("<")]
    Lt,

    // Math tokens
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Multiply,
    #[token("/")]
    Divide,

    // Function tokens
    #[token("fn")]
    Fn,
    #[token(":=")]
    ColonAssign,
    #[token(":")]
    Colon,
    #[token("->")]
    Arrow,
    #[token("=>")]
    FatArrow,
    #[token("{")]
    BlockStart,
    #[token("}")]
    BlockEnd,

    // Boolean literals
    #[token("true", |_| true)]
    #[token("false", |_| false)]
    Boolean(bool),

    // Types
    #[token("int")]
    IntType,
    #[token("bool")]
    BoolType,
    #[token("float")]
    FloatType,
    #[token("str")]
    StringType,

    // Explicit integer types
    #[token("u8")]
    U8Type,
    #[token("u16")]
    U16Type,
    #[token("u32")]
    U32Type,
    #[token("u64")]
    U64Type,
    #[token("i8")]
    I8Type,
    #[token("i16")]
    I16Type,
    #[token("i32")]
    I32Type,
    #[token("i64")]
    I64Type,

    // Collection types
    #[token("Array")]
    ArrayType,
    #[token("Vec")]
    VecType,
    #[token("Map")]
    MapType,
    #[token("Set")]
    SetType,

    #[token(".")]
    Dot,

    // Math and fn tokens
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,

    #[regex(r"[0-9]+", |lex| {
        let s = lex.slice();
        s.parse::<i64>().map_err(|err| {
            use std::num::IntErrorKind::*;
            let reason = match err.kind() {
                PosOverflow => "value too large for i64".to_owned(),
                NegOverflow => "value too small for i64".to_owned(),
                _ => format!("{err}"),
            };
            LexingError::InvalidInteger { value: s.to_owned(), reason }
        })
    })]
    Integer(i64),
    #[regex(r"[+-]?([0-9]+[.][0-9]*|[.][0-9]+)([eE][+-]?[0-9]+)?", |lex| lex.slice().parse())]
    Float(f64),
    #[regex(r#""[^"]*""#, |lex| {
        let s = lex.slice();
        &s[1..s.len()-1]
    })]
    Str(&'a str),
}

impl<'a> Tokens<'a> {
    pub fn tokenize(src: &'a str, file: &str) -> Vec<SpannedToken<'a>> {
        let mut lex = Self::lexer(src);
        let mut tokens = vec![];

        while let Some(tok) = lex.next() {
            let range = lex.span();
            if let Ok(token) = tok {
                tokens.push(SpannedToken {
                    node: token,
                    span: Span {
                        start: range.start,
                        end: range.end,
                        file: file.to_string(),
                    },
                });
            }
        }

        tokens
    }
}
