pub mod eval;
pub mod expr;
pub mod parser;
pub mod stmt;

#[cfg(test)]
mod tests {
    use chumsky::Parser;
    use logos::Logos;
    use sirin_lexer::token::Tokens;

    use crate::{
        expr::{BinOp, Expr},
        parser::parser,
        stmt::Stmt,
    };

    fn lex(src: &str) -> Vec<Tokens<'_>> {
        Tokens::lexer(src)
            .filter_map(|t| t.ok())
            .filter(|t| !matches!(t, Tokens::Whitespace))
            .collect()
    }

    #[test]
    fn test_program_fn_and_call() {
        let src = "fn soma(a, b) {\n  return a + b\n}\n\nx = soma(1, 2)";
        let tokens = lex(src);
        let stmts = parser().parse(&tokens).into_result().expect("parse failed");

        assert_eq!(stmts.len(), 2);

        // fn soma(a, b) { return a + b }
        match &stmts[0] {
            Stmt::Fn { name, args, body } => {
                assert_eq!(*name, "soma");
                assert_eq!(*args, vec!["a", "b"]);
                assert_eq!(body.len(), 1);
                match &body[0] {
                    Stmt::Return(Some(expr)) => {
                        assert!(matches!(expr.as_ref(), Expr::BinOp(BinOp::Add, _, _)));
                    }
                    _ => panic!("expected return with binop"),
                }
            }
            _ => panic!("expected fn declaration"),
        }

        // x = soma(1, 2)
        match &stmts[1] {
            Stmt::Let { name, rhs } => {
                assert_eq!(*name, "x");
                match rhs {
                    Expr::Call(fn_name, args) => {
                        assert_eq!(*fn_name, "soma");
                        assert_eq!(args.len(), 2);
                        assert!(matches!(args[0], Expr::Int(1)));
                        assert!(matches!(args[1], Expr::Int(2)));
                    }
                    _ => panic!("expected call expression"),
                }
            }
            _ => panic!("expected let statement"),
        }
    }
}
