pub mod eval;
pub mod expr;
pub mod parser;
pub mod stmt;
pub mod types;

#[cfg(test)]
mod tests {
    use chumsky::Parser;
    use logos::Logos;
    use sirin_lexer::token::Tokens;

    use crate::{
        expr::{BinOp, Expr},
        parser::parser,
        stmt::Stmt,
        types::Type,
    };

    fn lex(src: &str) -> Vec<Tokens<'_>> {
        Tokens::lexer(src)
            .filter_map(|t| t.ok())
            .filter(|t| !matches!(t, Tokens::Whitespace))
            .collect()
    }

    #[test]
    fn test_program_fn_and_call() {
        let src = "fn soma(a: int, b: int) -> int {\n  return a + b\n}\n\nx = soma(1, 2)";
        let tokens = lex(src);
        let stmts = parser().parse(&tokens).into_result().expect("parse failed");

        assert_eq!(stmts.len(), 2);

        match &stmts[0] {
            Stmt::Fn { name, args, return_type, body } => {
                assert_eq!(*name, "soma");
                assert_eq!(args.len(), 2);
                assert_eq!(args[0], ("a", Type::Int));
                assert_eq!(args[1], ("b", Type::Int));
                assert_eq!(*return_type, Some(Type::Int));
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

    #[test]
    fn test_fn_no_return_type() {
        let src = "fn noop(x: bool) {\n  return\n}";
        let tokens = lex(src);
        let stmts = parser().parse(&tokens).into_result().expect("parse failed");

        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Fn { name, args, return_type, body } => {
                assert_eq!(*name, "noop");
                assert_eq!(args[0], ("x", Type::Bool));
                assert_eq!(*return_type, None);
                assert_eq!(body.len(), 1);
                assert!(matches!(body[0], Stmt::Return(None)));
            }
            _ => panic!("expected fn declaration"),
        }
    }

    #[test]
    fn test_fat_arrow_fn() {
        let src = "fn dobro(x: int) => x + x";
        let tokens = lex(src);
        let stmts = parser().parse(&tokens).into_result().expect("parse failed");

        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Fn { name, args, body, .. } => {
                assert_eq!(*name, "dobro");
                assert_eq!(args[0], ("x", Type::Int));
                assert_eq!(body.len(), 1);
                assert!(matches!(&body[0], Stmt::Return(Some(e)) if matches!(e.as_ref(), Expr::BinOp(BinOp::Add, _, _))));
            }
            _ => panic!("expected fn declaration"),
        }
    }

    #[test]
    fn test_if_else() {
        let src = "if (x > 0) { y = 1 } else { y = 0 }";
        let tokens = lex(src);
        let stmts = parser().parse(&tokens).into_result().expect("parse failed");

        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::If { cond, then, else_ } => {
                assert!(matches!(cond.as_ref(), Expr::BinOp(BinOp::Gt, _, _)));
                assert_eq!(then.len(), 1);
                assert!(else_.is_some());
                assert_eq!(else_.as_ref().unwrap().len(), 1);
            }
            _ => panic!("expected if statement"),
        }
    }
}
