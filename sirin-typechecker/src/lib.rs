pub mod checker;
pub mod env;
pub mod error;

#[cfg(test)]
mod tests {
    use chumsky::Parser;
    use chumsky::input::Input as _;
    use chumsky::span::SimpleSpan;
    use sirin_parser::{parser::parser, types::Type};

    use crate::{checker::Checker, error::CheckerError};

    #[test]
    fn test_fn_return_simple() {
        let src = "fn soma(a: int, b: int) -> int {\n    return a + b\n}";
        let tokens = sirin_parser::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");
        let mut checker = Checker::new(src);
        for stmt in &stmts {
            checker.check_stmt(stmt).expect("unexpected type error");
        }
    }

    #[test]
    fn test_fn_early_return_conditional() {
        let src = "fn divide(a: int, b: int) -> int {\n    return 0 if b == 0\n    return a\n}";
        let tokens = sirin_parser::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");
        let mut checker = Checker::new(src);
        for stmt in &stmts {
            checker.check_stmt(stmt).expect("unexpected type error");
        }
    }

    #[test]
    fn test_fn_wrong_return_type() {
        let src = "fn quebrado(a: int) -> str {\n    return a\n}";
        let tokens = sirin_parser::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");
        let mut checker = Checker::new(src);
        let err = checker.check_stmt(&stmts[0]).expect_err("expected type error");
        assert!(
            matches!(err, CheckerError::TypeError(Type::Str, Type::Int)),
            "expected TypeError(Str, Int), got {err:?}"
        );
    }

    #[test]
    fn test_use_after_move_str() {
        let src = "x = \"oi\"\ny = x\nz = x";
        let tokens = sirin_parser::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");
        let mut checker = Checker::new(src);
        checker.check_stmt(&stmts[0]).expect("ok");
        checker.check_stmt(&stmts[1]).expect("ok");
        let err = checker.check_stmt(&stmts[2]).expect_err("expected use-after-move");
        assert!(
            matches!(err, CheckerError::UseAfterMove { var: "x", .. }),
            "expected UseAfterMove for x, got {err:?}"
        );
    }

    #[test]
    fn test_copy_type_no_move() {
        let src = "x = 1\ny = x\nz = x";
        let tokens = sirin_parser::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");
        let mut checker = Checker::new(src);
        for stmt in &stmts {
            checker.check_stmt(stmt).expect("int é Copy, não deve mover");
        }
    }

    #[test]
    fn test_copy_let_preserves_source() {
        // z := y — y ainda existe após a cópia
        let src = "x = \"oi\"\ny = x\nz := y\nw = y";
        let tokens = sirin_parser::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");
        let mut checker = Checker::new(src);
        for stmt in &stmts {
            checker.check_stmt(stmt).expect("CopyLet não deve mover y");
        }
    }

    #[test]
    fn test_return_outside_fn() {
        let src = "return 10";
        let tokens = sirin_parser::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");
        let mut checker = Checker::new(src);
        let err = checker.check_stmt(&stmts[0]).expect_err("expected error");
        assert!(
            matches!(err, CheckerError::ReturnOutsideFn),
            "expected ReturnOutsideFn, got {err:?}"
        );
    }

    // x: u8 = 5 — typed-let com inteiro explícito
    #[test]
    fn test_explicit_int_type_u8() {
        let src = "x: u8 = 5";
        let tokens = sirin_parser::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");
        let mut checker = Checker::new(src);
        checker.check_stmt(&stmts[0]).expect("u8 deve ser tipo válido");
    }

    // arr: Array[int] — reconhecimento do tipo array via parâmetro de função
    #[test]
    fn test_array_type() {
        let src = "fn f(arr: Array[int]) -> int {\n    return 0\n}";
        let tokens = sirin_parser::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");
        let mut checker = Checker::new(src);
        checker.check_stmt(&stmts[0]).expect("Array[int] deve ser tipo válido");
    }

    // m: Map[str, int] — tipo map com dois parâmetros
    #[test]
    fn test_map_type() {
        let src = "fn f(m: Map[str, int]) -> int {\n    return 0\n}";
        let tokens = sirin_parser::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");
        let mut checker = Checker::new(src);
        checker.check_stmt(&stmts[0]).expect("Map[str, int] deve ser tipo válido");
    }

    // erro: x: u8 = "texto" — typed-let com tipo incompatível
    #[test]
    fn test_explicit_int_type_incompatible() {
        let src = "x: u8 = \"texto\"";
        let tokens = sirin_parser::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");
        let mut checker = Checker::new(src);
        let err = checker.check_stmt(&stmts[0]).expect_err("esperava erro de tipo");
        assert!(
            matches!(err, CheckerError::TypeError(Type::U8, Type::Str)),
            "esperava TypeError(U8, Str), got {err:?}"
        );
    }
}
