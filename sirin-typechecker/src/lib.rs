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
        let src = "fn sum(a: int, b: int) -> int {\n    return a + b\n}";
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
        let src = "fn broken(a: int) -> str {\n    return a\n}";
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
        let src = "x = \"hi\"\ny = x\nz = x";
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
            checker.check_stmt(stmt).expect("int is Copy, must not move");
        }
    }

    #[test]
    fn test_copy_let_preserves_source() {
        // z := y — y still exists after the copy
        let src = "x = \"hi\"\ny = x\nz := y\nw = y";
        let tokens = sirin_parser::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");
        let mut checker = Checker::new(src);
        for stmt in &stmts {
            checker.check_stmt(stmt).expect("CopyLet must not move y");
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

    // x: u8 = 5 — typed-let with an explicit integer type
    #[test]
    fn test_explicit_int_type_u8() {
        let src = "x: u8 = 5";
        let tokens = sirin_parser::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");
        let mut checker = Checker::new(src);
        checker.check_stmt(&stmts[0]).expect("u8 must be a valid type");
    }

    // arr: Array[int] — array type recognized via a function parameter
    #[test]
    fn test_array_type() {
        let src = "fn f(arr: Array[int]) -> int {\n    return 0\n}";
        let tokens = sirin_parser::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");
        let mut checker = Checker::new(src);
        checker.check_stmt(&stmts[0]).expect("Array[int] must be a valid type");
    }

    // m: Map[str, int] — map type with two parameters
    #[test]
    fn test_map_type() {
        let src = "fn f(m: Map[str, int]) -> int {\n    return 0\n}";
        let tokens = sirin_parser::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");
        let mut checker = Checker::new(src);
        checker.check_stmt(&stmts[0]).expect("Map[str, int] must be a valid type");
    }

    macro_rules! check_ok {
        ($src:expr) => {{
            let src = $src;
            let tokens = sirin_parser::lex(src);
            let eoi = SimpleSpan::from(src.len()..src.len());
            let stmts = parser().parse(tokens.as_slice().split_token_span(eoi))
                .into_result().expect("parse failed");
            let mut checker = Checker::new(src);
            for stmt in &stmts {
                checker.check_stmt(stmt).expect("unexpected error");
            }
        }};
    }

    #[test]
    fn test_impl_int_primitive() {
        check_ok!("impl int {\n    fn double() -> int => self * 2\n}");
    }

    #[test]
    fn test_impl_str_primitive() {
        check_ok!("impl str {\n    fn empty() -> bool => self == self\n}");
    }

    #[test]
    fn test_impl_named_adds_method() {
        check_ok!("class Animal {\n    name: str\n    init(n: str) { name = n }\n}\nimpl Animal {\n    fn greet() -> str => name\n}");
    }

    #[test]
    fn test_interface_missing_method_error() {
        let src = "interface Describable {\n    fn describe() -> str\n}\nclass Thing is Describable {\n    x: int\n}";
        let tokens = sirin_parser::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");
        let mut checker = Checker::new(src);
        let mut got_error = false;
        for stmt in &stmts {
            if let Err(e) = checker.check_stmt(stmt) {
                assert!(
                    matches!(e, CheckerError::MissingInterfaceMethod { ref method, .. } if method == "describe"),
                    "expected MissingInterfaceMethod for describe, got {e:?}"
                );
                got_error = true;
                break;
            }
        }
        assert!(got_error, "expected MissingInterfaceMethod error");
    }

    #[test]
    fn test_interface_satisfied() {
        check_ok!("interface Describable {\n    fn describe() -> str\n}\nclass Thing is Describable {\n    x: int\n    fn describe() -> str => \"ok\"\n}");
    }

    // error: x: u8 = "text" — typed-let with an incompatible type
    #[test]
    fn test_explicit_int_type_incompatible() {
        let src = "x: u8 = \"text\"";
        let tokens = sirin_parser::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");
        let mut checker = Checker::new(src);
        let err = checker.check_stmt(&stmts[0]).expect_err("expected a type error");
        assert!(
            matches!(err, CheckerError::TypeError(Type::U8, Type::Str)),
            "expected TypeError(U8, Str), got {err:?}"
        );
    }

    /// Check `src` and return the first error, if any.
    fn first_error(src: &str) -> Option<String> {
        let tokens = sirin_parser::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi))
            .into_result().expect("parse failed");
        let mut checker = Checker::new(src);
        for stmt in &stmts {
            if let Err(e) = checker.check_stmt(stmt) {
                return Some(checker.take_diagnostic(&e, &stmt.span).message());
            }
        }
        None
    }

    #[test]
    fn test_undeclared_function() {
        let msg = first_error("x = nosuch(1)").expect("expected an error");
        assert!(msg.contains("undeclared function"), "got: {msg}");
    }

    #[test]
    fn test_enum_variant_needs_enum_name() {
        let msg = first_error("enum Shape {
    Circle(float),
    Point
}
s = Circle(2.0)")
            .expect("expected an error");
        assert!(msg.contains("Shape.Circle(...)"), "got: {msg}");
    }

    #[test]
    fn test_qualified_enum_variant_ok() {
        check_ok!("enum Shape {
    Circle(float),
    Point
}
s = Shape.Circle(2.0)
p = Shape.Point");
    }

    // A `return` inside an `if` leaves the function, so it must not mark the
    // value as moved for the code after the `if`.
    #[test]
    fn test_return_in_branch_is_not_a_move() {
        check_ok!("fn f(acc: str, done: bool) -> str {
    if (done) {
        return acc
    }
    b = acc
    return b
}");
    }
}
