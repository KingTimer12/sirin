pub mod aliases;
pub mod eval;
pub mod expr;
pub mod parser;
pub mod span;
pub mod stmt;
pub mod types;

use chumsky::Parser as _;
use chumsky::error::{Rich, RichPattern, RichReason};
use chumsky::input::Input as _;
use chumsky::span::SimpleSpan;
use logos::Logos;
use sirin_diagnostics::Diagnostic;
use sirin_lexer::token::Tokens;

use crate::{span::Spanned, stmt::Stmt};

/// Tokenize `src`, dropping whitespace. Invalid tokens are skipped here —
/// `parse` reports them, so call it rather than feeding these to the parser
/// directly when the user should see errors.
pub fn lex(src: &str) -> Vec<(Tokens<'_>, SimpleSpan)> {
    Tokens::lexer(src)
        .spanned()
        .filter_map(|(t, span)| t.ok().map(|t| (t, SimpleSpan::from(span))))
        .filter(|(t, _)| !matches!(t, Tokens::Whitespace))
        .collect()
}

/// Every invalid token in `src`, as a diagnostic.
pub fn lex_diagnostics(src: &str) -> Vec<Diagnostic> {
    Tokens::lexer(src)
        .spanned()
        .filter_map(|(t, span)| t.err().map(|e| (e, span)))
        .map(|(e, span)| {
            let d = Diagnostic::error(e.title(), span).with_label(e.to_string());
            match e.help() {
                Some(help) => d.with_help(help),
                None => d,
            }
        })
        .collect()
}

/// Parse the tokens `lex` produced for `src`. Lexer errors are reported first:
/// a skipped token would otherwise surface as a confusing syntax error.
pub fn parse<'a>(
    src: &str,
    tokens: &'a [(Tokens<'a>, SimpleSpan)],
) -> Result<Vec<Spanned<Stmt<'a>>>, Vec<Diagnostic>> {
    let lex_errors = lex_diagnostics(src);
    if !lex_errors.is_empty() {
        return Err(lex_errors);
    }
    let eoi = SimpleSpan::from(src.len()..src.len());
    parser::parser()
        .parse(tokens.split_token_span(eoi))
        .into_result()
        .map_err(|errors| errors.iter().map(|e| syntax_diagnostic(src, e)).collect())
}

/// Longest "expected ..." list worth showing; past this it is noise.
const MAX_EXPECTED: usize = 6;

fn syntax_diagnostic(src: &str, err: &Rich<'_, Tokens<'_>>) -> Diagnostic {
    let span = err.span();
    let mut range = span.start..span.end;

    let patterns = match err.reason() {
        RichReason::ExpectedFound { expected, .. } => expected,
        RichReason::Custom(msg) => {
            return Diagnostic::error("syntax error", range).with_label(msg.clone());
        }
    };
    let expected = expected_names(patterns);
    let expects = |name: &str| expected.iter().any(|e| e == name);

    let title = match err.found() {
        Some(tok) => format!("unexpected {tok}"),
        None => {
            // Point at the last character instead of an empty span past the end.
            if range.is_empty() && range.start > 0 {
                let last = src[..range.start].char_indices().last().map_or(0, |(i, _)| i);
                range = last..range.start;
            }
            "unexpected end of file".to_string()
        }
    };

    let before = src[..span.start].trim_end();
    let after_keyword = |kw: &str| {
        before.strip_suffix(kw).is_some_and(|rest| {
            !rest.ends_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
        })
    };
    let help = if err.found().is_none() && expects("`}`") {
        Some("a `{` block is still open — check that every `{` has a matching `}`")
    } else if expects("`(`") && (after_keyword("if") || after_keyword("while")) {
        Some("conditions must be wrapped in parentheses, e.g. `if (x > 0) { ... }`")
    } else {
        None
    };

    let label = match summarize_expected(&expected).as_slice() {
        [] => "this is not valid here".to_string(),
        [one] => format!("expected {one}"),
        [init @ .., last] => format!("expected {} or {}", init.join(", "), last),
    };

    let d = Diagnostic::error(title, range).with_label(label);
    match help {
        Some(h) => d.with_help(h),
        None => d,
    }
}

fn expected_names(patterns: &[RichPattern<'_, Tokens<'_>>]) -> Vec<String> {
    let mut names: Vec<String> = patterns
        .iter()
        .filter_map(|p| match p {
            RichPattern::Token(t) => Some(t.to_string()),
            RichPattern::Label(l) => Some(format!("{l}")),
            RichPattern::Identifier(i) => Some(format!("`{i}`")),
            RichPattern::EndOfInput => Some("end of file".to_string()),
            RichPattern::Any => Some("more input".to_string()),
            _ => None, // `SomethingElse` and future variants add nothing readable
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Trim a long list of alternatives down to what helps: grammar-level labels
/// ("expression", "statement") and closing delimiters say more than dozens of
/// operators and keywords that could also have continued the code.
fn summarize_expected(names: &[String]) -> Vec<String> {
    let is_label = |n: &str| !n.starts_with('`');
    let is_closer = |n: &str| matches!(n, "`}`" | "`)`" | "`]`" | "`,`");
    if names.iter().any(|n| is_label(n)) {
        return names.iter().filter(|n| is_label(n) || is_closer(n)).cloned().collect();
    }
    if names.len() <= MAX_EXPECTED {
        return names.to_vec();
    }
    let mut key = names[..MAX_EXPECTED].to_vec();
    key.push("...".to_string());
    key
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

    // $stmts is an ident from the call site (not hygiene-hidden).
    macro_rules! parse {
        ($stmts:ident, $src:expr) => {
            let _src_str: &str = $src;
            let _toks = crate::lex(_src_str);
            let _eoi = SimpleSpan::from(_src_str.len().._src_str.len());
            let $stmts = parser()
                .parse(_toks.as_slice().split_token_span(_eoi))
                .into_result()
                .expect("parse failed");
        };
    }

    #[test]
    fn test_program_fn_and_call() {
        let src = "fn sum(a: int, b: int) -> int {\n  return a + b\n}\n\nx = sum(1, 2)";
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
                ..
            } => {
                assert_eq!(name.node, "sum");
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
            Stmt::Let { name, rhs, .. } => {
                assert_eq!(name.node, "x");
                match &rhs.node {
                    Expr::Call(fn_name, args) => {
                        assert_eq!(*fn_name, "sum");
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
                ..
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
        let src = "fn double(x: int) => x + x";
        let tokens = crate::lex(src);
        let eoi = SimpleSpan::from(src.len()..src.len());
        let stmts = parser().parse(tokens.as_slice().split_token_span(eoi)).into_result().expect("parse failed");

        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Fn {
                name, args, body, ..
            } => {
                assert_eq!(name.node, "double");
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

    // ── Class / OOP tests ────────────────────────────────────────────────────

    #[test]
    fn test_class_simple() {
        parse!(stmts, "class Animal { name: str }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Class { name, abstract_, extends, is_, fields, methods } => {
                assert_eq!(name.node, "Animal");
                assert!(!abstract_);
                assert!(extends.is_none());
                assert!(is_.is_empty());
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].name.node, "name");
                assert_eq!(fields[0].ty, Type::Str);
                assert!(!fields[0].mutable);
                assert!(!fields[0].private);
                assert!(methods.is_empty());
            }
            _ => panic!("expected class"),
        }
    }

    #[test]
    fn test_class_abstract() {
        parse!(stmts, "abstract class Animal { name: str }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Class { abstract_, .. } => assert!(*abstract_),
            _ => panic!("expected abstract class"),
        }
    }

    #[test]
    fn test_class_extends() {
        parse!(stmts, "class Dog extends Animal { }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Class { extends, .. } => {
                assert_eq!(extends.as_ref().unwrap().node, "Animal");
            }
            _ => panic!("expected class with extends"),
        }
    }

    #[test]
    fn test_class_implements() {
        // `implements` still accepted as alias for `is`
        parse!(stmts, "class Dog implements Runner { }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Class { is_, .. } => {
                assert_eq!(is_.len(), 1);
                assert_eq!(is_[0].node, "Runner");
            }
            _ => panic!("expected class with is"),
        }
    }

    #[test]
    fn test_class_mutable_field() {
        parse!(stmts, "class Counter { mut count: int }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Class { fields, .. } => {
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].name.node, "count");
                assert!(fields[0].mutable);
                assert_eq!(fields[0].ty, Type::Int);
            }
            _ => panic!("expected class with mutable field"),
        }
    }

    #[test]
    fn test_class_private_field() {
        parse!(stmts, "class Foo { _secret: int }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Class { fields, .. } => {
                assert!(fields[0].private);
            }
            _ => panic!("expected class with private field"),
        }
    }

    #[test]
    fn test_class_default_block() {
        parse!(stmts, "class Counter { mut count: int default { count = 0 } }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Class { fields, methods, .. } => {
                assert_eq!(fields.len(), 1);
                assert_eq!(methods.len(), 1);
                assert!(matches!(methods[0].node, Stmt::Default { .. }));
                match &methods[0].node {
                    Stmt::Default { body } => assert_eq!(body.len(), 1),
                    _ => panic!(),
                }
            }
            _ => panic!("expected class with default"),
        }
    }

    #[test]
    fn test_class_init_block() {
        parse!(stmts, "class Animal { name: str init(n: str) { name = n } }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Class { fields, methods, .. } => {
                assert_eq!(fields.len(), 1);
                assert_eq!(methods.len(), 1);
                match &methods[0].node {
                    Stmt::Init { args, body } => {
                        assert_eq!(args.len(), 1);
                        assert_eq!(args[0].0.node, "n");
                        assert_eq!(args[0].1, Type::Str);
                        assert_eq!(body.len(), 1);
                    }
                    _ => panic!("expected init"),
                }
            }
            _ => panic!("expected class with init"),
        }
    }

    #[test]
    fn test_class_abstract_method() {
        parse!(stmts, "abstract class Animal { abstract fn speak() -> str }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Class { abstract_, methods, .. } => {
                assert!(*abstract_);
                assert_eq!(methods.len(), 1);
                match &methods[0].node {
                    Stmt::AbstractFn { name, args, return_type } => {
                        assert_eq!(name.node, "speak");
                        assert!(args.is_empty());
                        assert_eq!(*return_type, Some(Type::Str));
                    }
                    _ => panic!("expected abstract fn"),
                }
            }
            _ => panic!("expected abstract class"),
        }
    }

    #[test]
    fn test_class_regular_method() {
        parse!(stmts, "class Animal { fn describe() -> str => name }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Class { methods, .. } => {
                assert_eq!(methods.len(), 1);
                match &methods[0].node {
                    Stmt::Fn { name, return_type, .. } => {
                        assert_eq!(name.node, "describe");
                        assert_eq!(*return_type, Some(Type::Str));
                    }
                    _ => panic!("expected fn method"),
                }
            }
            _ => panic!("expected class with method"),
        }
    }

    #[test]
    fn test_class_extends_implements() {
        parse!(stmts, "class Dog extends Animal implements Runner { }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Class { extends, is_, .. } => {
                assert_eq!(extends.as_ref().unwrap().node, "Animal");
                assert_eq!(is_.len(), 1);
                assert_eq!(is_[0].node, "Runner");
            }
            _ => panic!("expected class with extends+implements"),
        }
    }

    #[test]
    fn test_interface() {
        parse!(stmts, "interface Runner { fn run() -> str }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Interface { name, methods } => {
                assert_eq!(name.node, "Runner");
                assert_eq!(methods.len(), 1);
                assert_eq!(methods[0].name.node, "run");
                assert!(methods[0].args.is_empty());
                assert_eq!(methods[0].return_type, Some(Type::Str));
            }
            _ => panic!("expected interface"),
        }
    }

    #[test]
    fn test_field_access() {
        parse!(stmts, "x = obj.field");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Let { name, rhs, .. } => {
                assert_eq!(name.node, "x");
                match &rhs.node {
                    Expr::FieldAccess(base, field) => {
                        assert!(matches!(base.node, Expr::Var("obj")));
                        assert_eq!(*field, "field");
                    }
                    _ => panic!("expected field access"),
                }
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_method_call_on_object() {
        parse!(stmts, "x = obj.method(1)");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Let { rhs, .. } => match &rhs.node {
                Expr::MethodCall(base, method, args) => {
                    assert!(matches!(base.node, Expr::Var("obj")));
                    assert_eq!(*method, "method");
                    assert_eq!(args.len(), 1);
                    assert!(matches!(args[0].node, Expr::Int(1)));
                }
                _ => panic!("expected method call"),
            },
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_new_with_args() {
        parse!(stmts, "x = Animal(5)");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Let { rhs, .. } => match &rhs.node {
                Expr::New(name, args) => {
                    assert_eq!(*name, "Animal");
                    assert_eq!(args.len(), 1);
                    assert!(matches!(args[0].node, Expr::Int(5)));
                }
                _ => panic!("expected New"),
            },
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_new_default() {
        parse!(stmts, "x = Animal()");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Let { rhs, .. } => match &rhs.node {
                Expr::NewDefault(name) => assert_eq!(*name, "Animal"),
                _ => panic!("expected NewDefault"),
            },
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_new_fields() {
        parse!(stmts, r#"x = Animal { name: "Rex" }"#);
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Let { rhs, .. } => match &rhs.node {
                Expr::NewFields(name, fields) => {
                    assert_eq!(*name, "Animal");
                    assert_eq!(fields.len(), 1);
                    assert_eq!(fields[0].0, "name");
                    assert!(matches!(fields[0].1.node, Expr::Str("Rex")));
                }
                _ => panic!("expected NewFields"),
            },
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_self_field_access() {
        parse!(stmts, "class Foo { fn get() -> str => self.name }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Class { methods, .. } => {
                match &methods[0].node {
                    Stmt::Fn { body, .. } => {
                        match &body[0].node {
                            Stmt::Return { value: Some(v), .. } => {
                                match &v.node {
                                    Expr::FieldAccess(base, field) => {
                                        assert!(matches!(base.node, Expr::Var("self")));
                                        assert_eq!(*field, "name");
                                    }
                                    _ => panic!("expected field access on self"),
                                }
                            }
                            _ => panic!("expected return"),
                        }
                    }
                    _ => panic!("expected fn"),
                }
            }
            _ => panic!("expected class"),
        }
    }

    #[test]
    fn test_named_type_in_field() {
        parse!(stmts, "class Node { child: Child }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Class { fields, .. } => {
                assert_eq!(fields.len(), 1);
                assert!(matches!(&fields[0].ty, Type::Named(n) if n == "Child"));
            }
            _ => panic!("expected class"),
        }
    }
}
