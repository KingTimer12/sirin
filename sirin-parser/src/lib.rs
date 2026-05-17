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
            Stmt::Let { name, rhs, .. } => {
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

    // ── Class / OOP tests ────────────────────────────────────────────────────

    #[test]
    fn test_class_simple() {
        parse!(stmts, "class Animal { nome: str }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Class { name, abstract_, extends, implements, fields, methods } => {
                assert_eq!(name.node, "Animal");
                assert!(!abstract_);
                assert!(extends.is_none());
                assert!(implements.is_empty());
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].name.node, "nome");
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
        parse!(stmts, "abstract class Animal { nome: str }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Class { abstract_, .. } => assert!(*abstract_),
            _ => panic!("expected abstract class"),
        }
    }

    #[test]
    fn test_class_extends() {
        parse!(stmts, "class Cachorro extends Animal { }");
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
        parse!(stmts, "class Cachorro implements Corredor { }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Class { implements, .. } => {
                assert_eq!(implements.len(), 1);
                assert_eq!(implements[0].node, "Corredor");
            }
            _ => panic!("expected class with implements"),
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
        parse!(stmts, "class Animal { nome: str init(n: str) { nome = n } }");
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
        parse!(stmts, "abstract class Animal { abstract fn falar() -> str }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Class { abstract_, methods, .. } => {
                assert!(*abstract_);
                assert_eq!(methods.len(), 1);
                match &methods[0].node {
                    Stmt::AbstractFn { name, args, return_type } => {
                        assert_eq!(name.node, "falar");
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
        parse!(stmts, "class Animal { fn descrever() -> str => nome }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Class { methods, .. } => {
                assert_eq!(methods.len(), 1);
                match &methods[0].node {
                    Stmt::Fn { name, return_type, .. } => {
                        assert_eq!(name.node, "descrever");
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
        parse!(stmts, "class Cachorro extends Animal implements Corredor { }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Class { extends, implements, .. } => {
                assert_eq!(extends.as_ref().unwrap().node, "Animal");
                assert_eq!(implements.len(), 1);
                assert_eq!(implements[0].node, "Corredor");
            }
            _ => panic!("expected class with extends+implements"),
        }
    }

    #[test]
    fn test_interface() {
        parse!(stmts, "interface Corredor { fn correr() -> str }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Interface { name, methods } => {
                assert_eq!(name.node, "Corredor");
                assert_eq!(methods.len(), 1);
                assert_eq!(methods[0].name.node, "correr");
                assert!(methods[0].args.is_empty());
                assert_eq!(methods[0].return_type, Some(Type::Str));
            }
            _ => panic!("expected interface"),
        }
    }

    #[test]
    fn test_field_access() {
        parse!(stmts, "x = obj.campo");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Let { name, rhs, .. } => {
                assert_eq!(name.node, "x");
                match &rhs.node {
                    Expr::FieldAccess(base, field) => {
                        assert!(matches!(base.node, Expr::Var("obj")));
                        assert_eq!(*field, "campo");
                    }
                    _ => panic!("expected field access"),
                }
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_method_call_on_object() {
        parse!(stmts, "x = obj.metodo(1)");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Let { rhs, .. } => match &rhs.node {
                Expr::MethodCall(base, method, args) => {
                    assert!(matches!(base.node, Expr::Var("obj")));
                    assert_eq!(*method, "metodo");
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
        parse!(stmts, r#"x = Animal { nome: "Rex" }"#);
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Let { rhs, .. } => match &rhs.node {
                Expr::NewFields(name, fields) => {
                    assert_eq!(*name, "Animal");
                    assert_eq!(fields.len(), 1);
                    assert_eq!(fields[0].0, "nome");
                    assert!(matches!(fields[0].1.node, Expr::Str("Rex")));
                }
                _ => panic!("expected NewFields"),
            },
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_self_field_access() {
        parse!(stmts, "class Foo { fn get() -> str => self.nome }");
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
                                        assert_eq!(*field, "nome");
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
        parse!(stmts, "class Node { filho: Filho }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0].node {
            Stmt::Class { fields, .. } => {
                assert_eq!(fields.len(), 1);
                assert!(matches!(&fields[0].ty, Type::Named(n) if n == "Filho"));
            }
            _ => panic!("expected class"),
        }
    }
}
