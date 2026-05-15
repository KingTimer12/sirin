pub mod checker;
pub mod env;
pub mod error;

#[cfg(test)]
mod tests {
    use chumsky::Parser;
    use logos::Logos;
    use sirin_lexer::token::Tokens;
    use sirin_parser::{parser::parser, types::Type};

    use crate::{checker::Checker, env::Env, error::CheckerError};

    fn lex(src: &str) -> Vec<Tokens<'_>> {
        Tokens::lexer(src)
            .filter_map(|t| t.ok())
            .filter(|t| !matches!(t, Tokens::Whitespace))
            .collect()
    }

    // Testa fn com return simples: retorna int, esperado int → ok
    #[test]
    fn test_fn_return_simple() {
        let src = "fn soma(a: int, b: int) -> int {\n    return a + b\n}";
        let tokens = lex(src);
        let stmts = parser().parse(&tokens).into_result().expect("parse failed");
        let mut checker = Checker { env: Env::new() };
        for stmt in &stmts {
            checker.check_stmt(stmt).expect("unexpected type error");
        }
    }

    // Testa early return condicional: return 0 if b == 0, depois return a → ok
    #[test]
    fn test_fn_early_return_conditional() {
        let src = "fn divide(a: int, b: int) -> int {\n    return 0 if b == 0\n    return a\n}";
        let tokens = lex(src);
        let stmts = parser().parse(&tokens).into_result().expect("parse failed");
        let mut checker = Checker { env: Env::new() };
        for stmt in &stmts {
            checker.check_stmt(stmt).expect("unexpected type error");
        }
    }

    // Testa tipo errado no return: fn espera str mas recebe int → TypeError
    #[test]
    fn test_fn_wrong_return_type() {
        let src = "fn quebrado(a: int) -> str {\n    return a\n}";
        let tokens = lex(src);
        let stmts = parser().parse(&tokens).into_result().expect("parse failed");
        let mut checker = Checker { env: Env::new() };
        let err = checker
            .check_stmt(&stmts[0])
            .expect_err("expected type error");
        assert!(
            matches!(err, CheckerError::TypeError(Type::Str, Type::Int)),
            "expected TypeError(Str, Int), got {err:?}"
        );
    }

    // Testa return fora de função → ReturnOutsideFn
    #[test]
    fn test_return_outside_fn() {
        let src = "return 10";
        let tokens = lex(src);
        let stmts = parser().parse(&tokens).into_result().expect("parse failed");
        let mut checker = Checker { env: Env::new() };
        let err = checker.check_stmt(&stmts[0]).expect_err("expected error");
        assert!(
            matches!(err, CheckerError::ReturnOutsideFn),
            "expected ReturnOutsideFn, got {err:?}"
        );
    }
}
