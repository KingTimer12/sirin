pub mod eval;
pub mod expr;
pub mod parser;
pub mod span;
pub mod stmt;
pub mod types;

use chumsky::span::SimpleSpan;
use logos::Logos;
use sirin_lexer::token::Tokens;

pub fn lex(src: &str) -> Vec<(Tokens<'_>, SimpleSpan)> {
    Tokens::lexer(src)
        .spanned()
        .filter_map(|(t, span)| t.ok().map(|t| (t, SimpleSpan::from(span))))
        .filter(|(t, _)| !matches!(t, Tokens::Whitespace))
        .collect()
}

#[cfg(test)]
mod tests {
    use chumsky::Parser;
    use chumsky::input::Input as _;
    use chumsky::span::SimpleSpan;

    use crate::{
        expr::{BinOp, Expr},
        parser::parser,
        stmt::Stmt,
        types::Type,
    };

    #[test]
    fn test_program_fn_and_call() {
        let src = "fn soma(a: int, b: int) -> int {\n  return a + b\n}\n\nx = soma(1, 2)";
        let tokens = crate::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");

        assert_eq!(stmts.len(), 2);

        match &stmts[0].node {
            Stmt::Fn {
                name,
                args,
                return_type,
                body,
            } => {
                assert_eq!(name.node, "soma");
                assert_eq!(args.len(), 2);
                assert_eq!(args[0].0.node, "a");
                assert_eq!(args[0].1, Type::Int);
                assert_eq!(args[1].0.node, "b");
                assert_eq!(args[1].1, Type::Int);
                assert_eq!(*return_type, Some(Type::Int));
                assert_eq!(body.len(), 1);
                match &body[0].node {
                    Stmt::Return {
                        value: Some(expr), ..
                    } => {
                        assert!(matches!(expr.node, Expr::BinOp(BinOp::Add, _, _)));
                    }
                    _ => panic!("expected return with binop"),
                }
            }
            _ => panic!("expected fn declaration"),
        }

        match &stmts[1].node {
            Stmt::Let { name, rhs } => {
                assert_eq!(name.node, "x");
                match &rhs.node {
                    Expr::Call(fn_name, args) => {
                        assert_eq!(*fn_name, "soma");
                        assert_eq!(args.len(), 2);
                        assert!(matches!(args[0].node, Expr::Int(1)));
                        assert!(matches!(args[1].node, Expr::Int(2)));
                    }
                    _ => panic!("expected call expression"),
                }
            }
            _ => panic!("expected let statement"),
        }
    }

    #[test]
    fn test_fn_no_return_type() {
        let src = "fn noop(x: bool) {\n  return\n}";
        let tokens = crate::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");

        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Fn {
                name,
                args,
                return_type,
                body,
            } => {
                assert_eq!(name.node, "noop");
                assert_eq!(args[0].0.node, "x");
                assert_eq!(args[0].1, Type::Bool);
                assert_eq!(*return_type, None);
                assert_eq!(body.len(), 1);
                assert!(matches!(
                    body[0].node,
                    Stmt::Return {
                        value: None,
                        cond: None
                    }
                ));
            }
            _ => panic!("expected fn declaration"),
        }
    }

    #[test]
    fn test_fat_arrow_fn() {
        let src = "fn dobro(x: int) => x + x";
        let tokens = crate::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");

        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Fn {
                name, args, body, ..
            } => {
                assert_eq!(name.node, "dobro");
                assert_eq!(args[0].0.node, "x");
                assert_eq!(args[0].1, Type::Int);
                assert_eq!(body.len(), 1);
                assert!(matches!(
                    &body[0].node,
                    Stmt::Return { value: Some(e), .. } if matches!(e.node, Expr::BinOp(BinOp::Add, _, _))
                ));
            }
            _ => panic!("expected fn declaration"),
        }
    }

    #[test]
    fn test_if_else() {
        let src = "if (x > 0) { y = 1 } else { y = 0 }";
        let tokens = crate::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");

        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::If { cond, then, else_ } => {
                assert!(matches!(cond.node, Expr::BinOp(BinOp::Gt, _, _)));
                assert_eq!(then.len(), 1);
                assert!(else_.is_some());
                assert_eq!(else_.as_ref().unwrap().len(), 1);
            }
            _ => panic!("expected if statement"),
        }
    }
}
