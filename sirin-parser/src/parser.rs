use chumsky::{
    IterParser, Parser,
    error::Simple,
    extra::Err,
    input::MappedInput,
    pratt::{infix, left, prefix},
    primitive::{choice, end, just},
    recursive::recursive,
    select,
    span::SimpleSpan,
};
use sirin_diagnostics::span::Span;
use sirin_lexer::token::Tokens;

use crate::{
    expr::{BinOp, Expr},
    span::Spanned,
    stmt::{BindPattern, ClassField, ImplTarget, InterfaceMethod, Stmt},
    types::Type,
};

pub type TokenInput<'a> =
    MappedInput<'a, Tokens<'a>, SimpleSpan, &'a [(Tokens<'a>, SimpleSpan)]>;

fn sp(s: SimpleSpan) -> Span {
    Span {
        start: s.start,
        end: s.end,
        file: String::new(),
    }
}

pub fn parser<'a>()
-> impl Parser<'a, TokenInput<'a>, Vec<Spanned<Stmt<'a>>>, Err<Simple<'a, Tokens<'a>>>> {
    let ident_expr = select! { Tokens::Ident(n) => Expr::Var(n) };
    let ident_name = select! { Tokens::Ident(n) => n };
    let spanned_name = ident_name
        .clone()
        .map_with(|n, extra| Spanned::new(n, sp(extra.span())));

    // Boxed: erases 12-branch choice type so rustc stops re-proving Parser on each use
    let ty_atom = choice((
        just(Tokens::IntType).to(Type::Int),
        just(Tokens::FloatType).to(Type::Float),
        just(Tokens::StringType).to(Type::Str),
        just(Tokens::BoolType).to(Type::Bool),
        just(Tokens::U8Type).to(Type::U8),
        just(Tokens::U16Type).to(Type::U16),
        just(Tokens::U32Type).to(Type::U32),
        just(Tokens::U64Type).to(Type::U64),
        just(Tokens::I8Type).to(Type::I8),
        just(Tokens::I16Type).to(Type::I16),
        just(Tokens::I32Type).to(Type::I32),
        just(Tokens::I64Type).to(Type::I64),
    )).boxed();

    // ty_atom extended with Named (Ident) for collection element types
    let ty_atom_or_named = choice((
        ty_atom.clone(),
        ident_name.clone().map(|n: &str| Type::Named(n.to_string())),
    )).boxed();

    // Boxed: cloned 3 times (Array/Vec/Set) — Arc clone, O(1)
    let bracket = ty_atom_or_named
        .clone()
        .delimited_by(just(Tokens::LBracket), just(Tokens::RBracket))
        .boxed();

    let map_bracket = ty_atom_or_named
        .clone()
        .then_ignore(just(Tokens::Comma))
        .then(ty_atom_or_named.clone())
        .delimited_by(just(Tokens::LBracket), just(Tokens::RBracket));

    // Type grammar. Recursive so the `Option[..]`/`Try[..]` long forms can nest
    // arbitrarily (`Try[int?]`, `Option[Try[int]]`). Collection brackets stay shallow.
    // Named is the fallback for user-defined class types (any remaining Ident).
    let ty = recursive(|ty| {
        // Recursive inner — used only by Option[..]/Try[..] so they accept any type.
        let nest = ty
            .clone()
            .delimited_by(just(Tokens::LBracket), just(Tokens::RBracket))
            .boxed();

        // Anonymous struct type annotation: { idade: int, nome: str }. Field types are
        // the full recursive `ty`, so `{ headers: Map[str, str] }` is allowed. Fields
        // kept sorted by name for structural identity (matches literal inference).
        let struct_ty = ident_name
            .clone()
            .then_ignore(just(Tokens::Colon))
            .then(ty.clone())
            .separated_by(just(Tokens::Comma))
            .at_least(1)
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Tokens::BlockStart), just(Tokens::BlockEnd))
            .map(|fields: Vec<(&str, Type)>| {
                let mut tys: Vec<(String, Type)> =
                    fields.into_iter().map(|(n, t)| (n.to_string(), t)).collect();
                tys.sort_by(|a, b| a.0.cmp(&b.0));
                Type::Struct(tys)
            })
            .boxed();

        // Function type: `fn(T1, T2) -> R` — first-class handler/callback values.
        let func_ty = just(Tokens::Fn)
            .ignore_then(
                ty.clone()
                    .separated_by(just(Tokens::Comma))
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
            )
            .then_ignore(just(Tokens::Arrow))
            .then(ty.clone())
            .map(|(args, ret)| Type::Func(args, Box::new(ret)))
            .boxed();

        let core = choice((
            func_ty,
            struct_ty,
            just(Tokens::ArrayType)
                .ignore_then(bracket.clone())
                .map(|t| Type::Array(Box::new(t))),
            just(Tokens::VecType)
                .ignore_then(bracket.clone())
                .map(|t| Type::Vec(Box::new(t))),
            just(Tokens::SetType)
                .ignore_then(bracket.clone())
                .map(|t| Type::Set(Box::new(t))),
            just(Tokens::MapType)
                .ignore_then(map_bracket)
                .map(|(k, v)| Type::Map(Box::new(k), Box::new(v))),
            // Channel[T] — matched before Named to avoid "Channel" falling through
            select! { Tokens::Ident("Channel") => () }
                .ignore_then(bracket.clone())
                .map(|t| Type::Channel(Box::new(t))),
            // Long forms (aliases of `T?` / `T!`); recursive inner enables nesting.
            select! { Tokens::Ident("Option") => () }
                .ignore_then(nest.clone())
                .map(|t| Type::Nullable(Box::new(t))),
            select! { Tokens::Ident("Try") => () }
                .ignore_then(nest.clone())
                .map(|t| Type::Try(Box::new(t))),
            ty_atom,
            ident_name.clone().map(|n: &str| Type::Named(n.to_string())),
        ))
        .boxed();

        // Postfix sigils: `T?` → Nullable, `T!` → Try. Single level — stacking
        // (`int?!`) is a parse error; use the bracket long form to nest.
        core.then(
            choice((
                just(Tokens::Question).to(0u8),
                just(Tokens::Not).to(1u8),
            ))
            .or_not(),
        )
        .map(|(t, m)| match m {
            Some(0) => Type::Nullable(Box::new(t)),
            Some(1) => Type::Try(Box::new(t)),
            _ => t,
        })
    })
    .boxed();

    // Postfix ops collected per-token; stored with their span for correct tree spans
    enum Postfix<'b> {
        Index(Spanned<Expr<'b>>, Span),
        Field(&'b str, Span),
        Method(&'b str, Vec<Spanned<Expr<'b>>>, Span),
        Await(Span),
        Clone(Span),
    }

    let expr = recursive(|p| {
        // call/new: uppercase ident(args) → New/NewDefault; lowercase → Call
        let call = ident_name
            .then(
                p.clone()
                    .separated_by(just(Tokens::Comma))
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
            )
            .map_with(|(name, args), extra| {
                let is_upper = name.chars().next().map_or(false, |c| c.is_uppercase());
                let e = if is_upper {
                    if args.is_empty() { Expr::NewDefault(name) } else { Expr::New(name, args) }
                } else {
                    Expr::Call(name, args)
                };
                Spanned::new(e, sp(extra.span()))
            });

        // Boxed: 8-branch or-chain; type doubles with each .or()
        let atom = {
            let parenthesized = p
                .clone()
                .delimited_by(just(Tokens::LParen), just(Tokens::RParen));

            let integer = select! { Tokens::Integer(n) => Expr::Int(n) }
                .map_with(|e, extra| Spanned::new(e, sp(extra.span())));
            let float = select! { Tokens::Float(n) => Expr::Float(n) }
                .map_with(|e, extra| Spanned::new(e, sp(extra.span())));
            let string = select! { Tokens::Str(n) => Expr::Str(n) }
                .map_with(|e, extra| Spanned::new(e, sp(extra.span())));
            let boolean = select! { Tokens::Boolean(n) => Expr::Boolean(n) }
                .map_with(|e, extra| Spanned::new(e, sp(extra.span())));
            let ident = ident_expr.map_with(|e, extra| Spanned::new(e, sp(extra.span())));

            // self keyword parsed as Expr::Var("self") inside methods
            let self_expr = just(Tokens::SelfKw)
                .map_with(|_, extra| Spanned::new(Expr::Var("self"), sp(extra.span())));

            // Array literal: [expr, expr, ...]
            let array_lit = p
                .clone()
                .separated_by(just(Tokens::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Tokens::LBracket), just(Tokens::RBracket))
                .map_with(|items, extra| Spanned::new(Expr::Array(items), sp(extra.span())));

            // Struct-literal init: ClassName { field: val, ... }
            let new_fields = ident_name
                .clone()
                .then(
                    ident_name
                        .clone()
                        .then_ignore(just(Tokens::Colon))
                        .then(p.clone())
                        .separated_by(just(Tokens::Comma))
                        .allow_trailing()
                        .collect::<Vec<_>>()
                        .delimited_by(just(Tokens::BlockStart), just(Tokens::BlockEnd)),
                )
                .map_with(|(name, fields), extra| {
                    Spanned::new(Expr::NewFields(name, fields), sp(extra.span()))
                });

            // Anonymous object literal: { nome: "Julius", idade: 24 }
            // Distinct from a code block: only reachable in expression position.
            let obj_literal = ident_name
                .clone()
                .then_ignore(just(Tokens::Colon))
                .then(p.clone())
                .separated_by(just(Tokens::Comma))
                .at_least(1)
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Tokens::BlockStart), just(Tokens::BlockEnd))
                .map_with(|fields, extra| {
                    Spanned::new(Expr::ObjectLiteral(fields), sp(extra.span()))
                });

            // Collection constructors: Vec(n), Map(), Set(), Array(n)
            let ctor_args = || {
                p.clone()
                    .separated_by(just(Tokens::Comma))
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::LParen), just(Tokens::RParen))
            };
            let vec_ctor = just(Tokens::VecType)
                .ignore_then(ctor_args())
                .map_with(|a, e| Spanned::new(Expr::Call("Vec", a), sp(e.span())));
            let map_ctor = just(Tokens::MapType)
                .ignore_then(ctor_args())
                .map_with(|a, e| Spanned::new(Expr::Call("Map", a), sp(e.span())));
            let set_ctor = just(Tokens::SetType)
                .ignore_then(ctor_args())
                .map_with(|a, e| Spanned::new(Expr::Call("Set", a), sp(e.span())));
            let array_ctor = just(Tokens::ArrayType)
                .ignore_then(ctor_args())
                .map_with(|a, e| Spanned::new(Expr::Call("Array", a), sp(e.span())));

            // Option constructors: `Some(expr)` / `None` (dedicated tokens)
            let some_expr = just(Tokens::Some)
                .ignore_then(
                    p.clone()
                        .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
                )
                .map_with(|inner, extra| {
                    Spanned::new(Expr::Some(Box::new(inner)), sp(extra.span()))
                });
            let none_expr = just(Tokens::None)
                .map_with(|_, extra| Spanned::new(Expr::None, sp(extra.span())));

            // Try constructors: `Ok(expr)` / `Err(expr)`
            let ok_expr = just(Tokens::Ok)
                .ignore_then(
                    p.clone()
                        .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
                )
                .map_with(|inner, extra| {
                    Spanned::new(Expr::Ok(Box::new(inner)), sp(extra.span()))
                });
            let err_expr = just(Tokens::Err)
                .ignore_then(
                    p.clone()
                        .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
                )
                .map_with(|inner, extra| {
                    Spanned::new(Expr::Err(Box::new(inner)), sp(extra.span()))
                });

            // `f::try(args)` — readable call form; a fallible fn returns Try[T] anyway,
            // so this lowers to a plain call.
            let try_call = ident_name
                .then_ignore(just(Tokens::ColonColon))
                .then_ignore(just(Tokens::Try))
                .then(
                    p.clone()
                        .separated_by(just(Tokens::Comma))
                        .collect::<Vec<_>>()
                        .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
                )
                .map_with(|(name, args), extra| {
                    Spanned::new(Expr::Call(name, args), sp(extra.span()))
                });

            // new_fields before call so Ident { } is tried before Ident ()
            array_lit
                .or(parenthesized)
                .or(integer)
                .or(float)
                .or(string)
                .or(boolean)
                .or(some_expr)
                .or(none_expr)
                .or(ok_expr)
                .or(err_expr)
                .or(try_call)
                .or(vec_ctor)
                .or(map_ctor)
                .or(set_ctor)
                .or(array_ctor)
                .or(obj_literal)
                .or(new_fields)
                .or(call)
                .or(self_expr)
                .or(ident)
        }.boxed();

        // Postfix: index [expr] and method/field .name[(args)], left-associative
        let postfix_index = p
            .clone()
            .delimited_by(just(Tokens::LBracket), just(Tokens::RBracket))
            .map_with(|idx, extra| Postfix::Index(idx, sp(extra.span())));

        // method tried before field: .name(args) vs .name
        let postfix_method = just(Tokens::Dot)
            .ignore_then(ident_name.clone())
            .then(
                p.clone()
                    .separated_by(just(Tokens::Comma))
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
            )
            .map_with(|(name, args), extra| Postfix::Method(name, args, sp(extra.span())));

        let postfix_field = just(Tokens::Dot)
            .ignore_then(ident_name.clone())
            .map_with(|name, extra| Postfix::Field(name, sp(extra.span())));

        let postfix_await = just(Tokens::Dot)
            .ignore_then(just(Tokens::Await))
            .map_with(|_, extra| Postfix::Await(sp(extra.span())));

        // `expr::clone` — inline deep copy intrinsic (no args)
        let postfix_clone = just(Tokens::ColonColon)
            .ignore_then(select! { Tokens::Ident("clone") => () })
            .map_with(|_, extra| Postfix::Clone(sp(extra.span())));

        let primary = atom
            .then(
                postfix_index
                    .or(postfix_await)
                    .or(postfix_clone)
                    .or(postfix_method)
                    .or(postfix_field)
                    .repeated()
                    .collect::<Vec<_>>(),
            )
            .map(|(base, ops)| {
                ops.into_iter().fold(base, |acc, op| match op {
                    Postfix::Index(idx, span) => {
                        Spanned::new(Expr::Index(Box::new(acc), Box::new(idx)), span)
                    }
                    Postfix::Method(name, args, span) => {
                        Spanned::new(Expr::MethodCall(Box::new(acc), name, args), span)
                    }
                    Postfix::Field(name, span) => {
                        Spanned::new(Expr::FieldAccess(Box::new(acc), name), span)
                    }
                    Postfix::Await(span) => {
                        Spanned::new(Expr::Await(Box::new(acc)), span)
                    }
                    Postfix::Clone(span) => {
                        Spanned::new(Expr::Clone(Box::new(acc)), span)
                    }
                })
            })
            .boxed();

        primary.pratt((
            prefix(6, just(Tokens::Minus), |_, rhs, extra| {
                Spanned::new(Expr::Neg(Box::new(rhs)), sp(extra.span()))
            }),
            prefix(6, just(Tokens::Not), |_, rhs, extra| {
                Spanned::new(Expr::Not(Box::new(rhs)), sp(extra.span()))
            }),
            infix(left(5), just(Tokens::Multiply), |lhs, _, rhs, extra| {
                Spanned::new(
                    Expr::BinOp(BinOp::Mul, Box::new(lhs), Box::new(rhs)),
                    sp(extra.span()),
                )
            }),
            infix(left(5), just(Tokens::Divide), |lhs, _, rhs, extra| {
                Spanned::new(
                    Expr::BinOp(BinOp::Div, Box::new(lhs), Box::new(rhs)),
                    sp(extra.span()),
                )
            }),
            infix(left(4), just(Tokens::Plus), |lhs, _, rhs, extra| {
                Spanned::new(
                    Expr::BinOp(BinOp::Add, Box::new(lhs), Box::new(rhs)),
                    sp(extra.span()),
                )
            }),
            infix(left(4), just(Tokens::Minus), |lhs, _, rhs, extra| {
                Spanned::new(
                    Expr::BinOp(BinOp::Sub, Box::new(lhs), Box::new(rhs)),
                    sp(extra.span()),
                )
            }),
            infix(left(3), just(Tokens::Eq), |lhs, _, rhs, extra| {
                Spanned::new(
                    Expr::BinOp(BinOp::Eq, Box::new(lhs), Box::new(rhs)),
                    sp(extra.span()),
                )
            }),
            infix(left(3), just(Tokens::NotEq), |lhs, _, rhs, extra| {
                Spanned::new(
                    Expr::BinOp(BinOp::NotEq, Box::new(lhs), Box::new(rhs)),
                    sp(extra.span()),
                )
            }),
            infix(left(3), just(Tokens::Gt), |lhs, _, rhs, extra| {
                Spanned::new(
                    Expr::BinOp(BinOp::Gt, Box::new(lhs), Box::new(rhs)),
                    sp(extra.span()),
                )
            }),
            infix(left(3), just(Tokens::Lt), |lhs, _, rhs, extra| {
                Spanned::new(
                    Expr::BinOp(BinOp::Lt, Box::new(lhs), Box::new(rhs)),
                    sp(extra.span()),
                )
            }),
            infix(left(3), just(Tokens::GtEq), |lhs, _, rhs, extra| {
                Spanned::new(
                    Expr::BinOp(BinOp::GtEq, Box::new(lhs), Box::new(rhs)),
                    sp(extra.span()),
                )
            }),
            infix(left(3), just(Tokens::LtEq), |lhs, _, rhs, extra| {
                Spanned::new(
                    Expr::BinOp(BinOp::LtEq, Box::new(lhs), Box::new(rhs)),
                    sp(extra.span()),
                )
            }),
            infix(left(2), just(Tokens::And), |lhs, _, rhs, extra| {
                Spanned::new(
                    Expr::BinOp(BinOp::And, Box::new(lhs), Box::new(rhs)),
                    sp(extra.span()),
                )
            }),
            infix(left(2), just(Tokens::Or), |lhs, _, rhs, extra| {
                Spanned::new(
                    Expr::BinOp(BinOp::Or, Box::new(lhs), Box::new(rhs)),
                    sp(extra.span()),
                )
            }),
        ))
    });

    let stmt = recursive(|decl| {
        // name: Type = expr
        let typed_var = spanned_name
            .clone()
            .then_ignore(just(Tokens::Colon))
            .then(ty.clone())
            .then_ignore(just(Tokens::Assign))
            .then(expr.clone())
            .map_with(|((name, ty), rhs), extra| {
                Spanned::new(Stmt::Let { name, ty: Some(ty), rhs }, sp(extra.span()))
            });

        // name: Type := expr
        let typed_copy_var = spanned_name
            .clone()
            .then_ignore(just(Tokens::Colon))
            .then(ty.clone())
            .then_ignore(just(Tokens::ColonAssign))
            .then(expr.clone())
            .map_with(|((name, ty), rhs), extra| {
                Spanned::new(Stmt::CopyLet { name, ty: Some(ty), rhs }, sp(extra.span()))
            });

        let var = spanned_name
            .clone()
            .then_ignore(just(Tokens::Assign))
            .then(expr.clone())
            .map_with(|(name, rhs), extra| {
                Spanned::new(Stmt::Let { name, ty: None, rhs }, sp(extra.span()))
            });

        let copy_var = spanned_name
            .clone()
            .then_ignore(just(Tokens::ColonAssign))
            .then(expr.clone())
            .map_with(|(name, rhs), extra| {
                Spanned::new(Stmt::CopyLet { name, ty: None, rhs }, sp(extra.span()))
            });

        let r#return = just(Tokens::Return)
            .ignore_then(expr.clone().or_not())
            .then(just(Tokens::If).ignore_then(expr.clone()).or_not())
            .map_with(|(value, cond), extra| {
                Spanned::new(
                    Stmt::Return {
                        value: value.map(Box::new),
                        cond: cond.map(Box::new),
                    },
                    sp(extra.span()),
                )
            });

        // `if Some(name) = expr { .. }` / `if Ok(x) = r { .. }` / `if Err(e) = r { .. }`
        // Tried before `r#if`; unambiguous since it starts with a pattern keyword.
        let bind_pat = choice((
            just(Tokens::Some).to(BindPattern::Some),
            just(Tokens::Ok).to(BindPattern::Ok),
            just(Tokens::Err).to(BindPattern::Err),
        ));
        let if_let = just(Tokens::If)
            .ignore_then(bind_pat)
            .then(
                spanned_name
                    .clone()
                    .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
            )
            .then_ignore(just(Tokens::Assign))
            .then(expr.clone())
            .then(
                decl.clone()
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::BlockStart), just(Tokens::BlockEnd)),
            )
            .then(
                just(Tokens::Else)
                    .ignore_then(
                        decl.clone()
                            .repeated()
                            .collect::<Vec<_>>()
                            .delimited_by(just(Tokens::BlockStart), just(Tokens::BlockEnd)),
                    )
                    .or_not(),
            )
            .map_with(|((((pattern, name), opt), then), else_), extra| {
                Spanned::new(
                    Stmt::IfLet {
                        pattern,
                        name,
                        expr: Box::new(opt),
                        then,
                        else_,
                    },
                    sp(extra.span()),
                )
            });

        // `name ?= fallible()` — bind Ok value or propagate Err from the current fn.
        let try_assign = spanned_name
            .clone()
            .then_ignore(just(Tokens::QuestionAssign))
            .then(expr.clone())
            .map_with(|(name, rhs), extra| {
                Spanned::new(
                    Stmt::TryAssign { name, rhs: Box::new(rhs) },
                    sp(extra.span()),
                )
            });

        let r#if = just(Tokens::If)
            .ignore_then(
                expr.clone()
                    .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
            )
            .then(
                decl.clone()
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::BlockStart), just(Tokens::BlockEnd)),
            )
            .then(
                just(Tokens::Else)
                    .ignore_then(
                        decl.clone()
                            .repeated()
                            .collect::<Vec<_>>()
                            .delimited_by(just(Tokens::BlockStart), just(Tokens::BlockEnd)),
                    )
                    .or_not(),
            )
            .map_with(|((cond, then), else_), extra| {
                Spanned::new(
                    Stmt::If {
                        cond: Box::new(cond),
                        then,
                        else_,
                    },
                    sp(extra.span()),
                )
            });

        let block = decl
            .clone()
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Tokens::BlockStart), just(Tokens::BlockEnd));

        let r#while = just(Tokens::While)
            .ignore_then(
                expr.clone()
                    .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
            )
            .then(block.clone())
            .map_with(|(cond, body), extra| {
                Spanned::new(
                    Stmt::While { cond: Box::new(cond), body },
                    sp(extra.span()),
                )
            });

        let r#for = just(Tokens::For)
            .ignore_then(spanned_name.clone())
            .then_ignore(just(Tokens::In))
            .then(expr.clone())
            .then_ignore(just(Tokens::DotDot))
            .then(expr.clone())
            .then(block.clone())
            .map_with(|(((var, start), end), body), extra| {
                Spanned::new(
                    Stmt::For {
                        var,
                        start: Box::new(start),
                        end: Box::new(end),
                        body,
                    },
                    sp(extra.span()),
                )
            });

        let r#break = just(Tokens::Break)
            .map_with(|_, extra| Spanned::new(Stmt::Break, sp(extra.span())));
        let r#continue = just(Tokens::Continue)
            .map_with(|_, extra| Spanned::new(Stmt::Continue, sp(extra.span())));

        let fn_body = choice((
            just(Tokens::FatArrow)
                .ignore_then(expr.clone())
                .map_with(|body, extra| {
                    vec![Spanned::new(
                        Stmt::Return {
                            value: Some(Box::new(body)),
                            cond: None,
                        },
                        sp(extra.span()),
                    )]
                }),
            decl.clone()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Tokens::BlockStart), just(Tokens::BlockEnd)),
        ));

        let arg = spanned_name
            .clone()
            .then_ignore(just(Tokens::Colon))
            .then(ty.clone());

        let r#fn = just(Tokens::Async).or_not()
            .then_ignore(just(Tokens::Fn))
            .then(spanned_name.clone())
            .then(
                arg.clone()
                    .separated_by(just(Tokens::Comma))
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
            )
            .then(just(Tokens::Arrow).ignore_then(ty.clone()).or_not())
            .then(fn_body.clone())
            .map_with(|((((async_opt, name), args), return_type), body), extra| {
                Spanned::new(
                    Stmt::Fn {
                        async_: async_opt.is_some(),
                        name,
                        args,
                        return_type,
                        body,
                    },
                    sp(extra.span()),
                )
            });

        // `spawn { stmt* }` — block form (one coroutine, statements run in order)
        let spawn_block = decl.clone()
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Tokens::BlockStart), just(Tokens::BlockEnd));
        // `spawn stmt` — braceless sugar for a single-statement coroutine
        let spawn_single = decl.clone().map(|s| vec![s]);
        let spawn_stmt = just(Tokens::Spawn)
            .ignore_then(spawn_block.or(spawn_single))
            .map_with(|body, extra| {
                Spanned::new(Stmt::Spawn { body }, sp(extra.span()))
            });

        // ── Class body items ──────────────────────────────────────────────────

        #[allow(dead_code)]
        enum ClassItem<'b> {
            Field(ClassField<'b>),
            Method(Spanned<Stmt<'b>>),
        }

        // mut field
        let class_field_mut = just(Tokens::Mut)
            .ignore_then(spanned_name.clone())
            .then_ignore(just(Tokens::Colon))
            .then(ty.clone())
            .map(|(name, field_ty)| {
                let private = name.node.starts_with('_');
                ClassItem::Field(ClassField { name, ty: field_ty, mutable: true, private })
            });

        // immutable field
        let class_field_imm = spanned_name
            .clone()
            .then_ignore(just(Tokens::Colon))
            .then(ty.clone())
            .map(|(name, field_ty)| {
                let private = name.node.starts_with('_');
                ClassItem::Field(ClassField { name, ty: field_ty, mutable: false, private })
            });

        // abstract fn name(args) -> ret   (no body)
        let abstract_fn = just(Tokens::Abstract)
            .ignore_then(just(Tokens::Fn))
            .ignore_then(spanned_name.clone())
            .then(
                arg.clone()
                    .separated_by(just(Tokens::Comma))
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
            )
            .then(just(Tokens::Arrow).ignore_then(ty.clone()).or_not())
            .map_with(|((name, args), return_type), extra| {
                ClassItem::Method(Spanned::new(
                    Stmt::AbstractFn { name, args, return_type },
                    sp(extra.span()),
                ))
            });

        // init(args) { body }
        let init_item = just(Tokens::Init)
            .ignore_then(
                arg.clone()
                    .separated_by(just(Tokens::Comma))
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
            )
            .then(
                decl.clone()
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::BlockStart), just(Tokens::BlockEnd)),
            )
            .map_with(|(args, body), extra| {
                ClassItem::Method(Spanned::new(Stmt::Init { args, body }, sp(extra.span())))
            });

        // default { body }
        let default_item = just(Tokens::Default)
            .ignore_then(
                decl.clone()
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::BlockStart), just(Tokens::BlockEnd)),
            )
            .map_with(|body, extra| {
                ClassItem::Method(Spanned::new(Stmt::Default { body }, sp(extra.span())))
            });

        // regular fn in class body (uses fn_body — last use)
        let class_fn = just(Tokens::Fn)
            .ignore_then(spanned_name.clone())
            .then(
                arg.clone()
                    .separated_by(just(Tokens::Comma))
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
            )
            .then(just(Tokens::Arrow).ignore_then(ty.clone()).or_not())
            .then(fn_body)
            .map_with(|(((name, args), return_type), body), extra| {
                ClassItem::Method(Spanned::new(
                    Stmt::Fn { async_: false, name, args, return_type, body },
                    sp(extra.span()),
                ))
            });

        // keywords-first order; field_mut before field_imm to avoid consuming mut as ident
        // Boxed so the type is Clone + reusable in both class_stmt and impl_stmt
        let class_item = abstract_fn
            .or(class_fn)
            .or(init_item)
            .or(default_item)
            .or(class_field_mut)
            .or(class_field_imm)
            .boxed();
        let impl_item = class_item.clone();

        // [abstract] class Name [extends P] [is I1, I2 | implements I1, I2] { items }
        let is_clause = choice((just(Tokens::Is), just(Tokens::Implements)))
            .ignore_then(
                spanned_name
                    .clone()
                    .separated_by(just(Tokens::Comma))
                    .collect::<Vec<_>>(),
            )
            .or_not()
            .map(|opt| opt.unwrap_or_default());

        let class_stmt = choice((
            just(Tokens::Abstract).then_ignore(just(Tokens::Class)).to(true),
            just(Tokens::Class).to(false),
        ))
        .then(spanned_name.clone())
        .then(just(Tokens::Extends).ignore_then(spanned_name.clone()).or_not())
        .then(is_clause)
        .then(
            class_item
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Tokens::BlockStart), just(Tokens::BlockEnd)),
        )
        .map_with(|((((abstract_, name), extends), is_), items), extra| {
            let mut fields  = vec![];
            let mut methods = vec![];
            for item in items {
                match item {
                    ClassItem::Field(f)  => fields.push(f),
                    ClassItem::Method(m) => methods.push(m),
                }
            }
            Spanned::new(
                Stmt::Class { name, abstract_, extends, is_, fields, methods },
                sp(extra.span()),
            )
        });

        // ── Interface ────────────────────────────────────────────────────────

        let interface_method = just(Tokens::Fn)
            .ignore_then(spanned_name.clone())
            .then(
                arg.clone()
                    .separated_by(just(Tokens::Comma))
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
            )
            .then(just(Tokens::Arrow).ignore_then(ty.clone()).or_not())
            .map(|((name, args), return_type)| InterfaceMethod { name, args, return_type });

        let interface_stmt = just(Tokens::Interface)
            .ignore_then(spanned_name.clone())
            .then(
                interface_method
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::BlockStart), just(Tokens::BlockEnd)),
            )
            .map_with(|(name, methods), extra| {
                Spanned::new(Stmt::Interface { name, methods }, sp(extra.span()))
            });

        // impl Target { fn ... }
        let impl_target = choice((
            just(Tokens::IntType).to(ImplTarget::Int),
            just(Tokens::FloatType).to(ImplTarget::Float),
            just(Tokens::StringType).to(ImplTarget::Str),
            just(Tokens::BoolType).to(ImplTarget::Bool),
            just(Tokens::U8Type).to(ImplTarget::U8),
            just(Tokens::U16Type).to(ImplTarget::U16),
            just(Tokens::U32Type).to(ImplTarget::U32),
            just(Tokens::U64Type).to(ImplTarget::U64),
            just(Tokens::I8Type).to(ImplTarget::I8),
            just(Tokens::I16Type).to(ImplTarget::I16),
            just(Tokens::I32Type).to(ImplTarget::I32),
            just(Tokens::I64Type).to(ImplTarget::I64),
            ident_name.clone().map(ImplTarget::Named),
        ));

        let impl_stmt = just(Tokens::Impl)
            .ignore_then(impl_target)
            .then(
                impl_item
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::BlockStart), just(Tokens::BlockEnd)),
            )
            .map_with(|(target, items), extra| {
                let methods = items.into_iter()
                    .filter_map(|item| match item {
                        ClassItem::Method(m) => Some(m),
                        ClassItem::Field(_) => None,
                    })
                    .collect();
                Spanned::new(Stmt::Impl { target, methods }, sp(extra.span()))
            });

        // use sirin.io  /  use sirin.async  etc.
        // Segments can be keywords (async, spawn, await) in addition to plain idents.
        let use_segment = choice((
            ident_name.clone(),
            just(Tokens::Async).to("async"),
            just(Tokens::Spawn).to("spawn"),
            just(Tokens::Await).to("await"),
        ));
        // `type Name = <Type>` — structural type alias.
        let type_alias = just(Tokens::TypeKw)
            .ignore_then(spanned_name.clone())
            .then_ignore(just(Tokens::Assign))
            .then(ty.clone())
            .map_with(|(name, ty), extra| {
                Spanned::new(Stmt::TypeAlias { name, ty }, sp(extra.span()))
            });

        let use_stmt = just(Tokens::Use)
            .ignore_then(use_segment.clone())
            .then(
                just(Tokens::Dot)
                    .ignore_then(use_segment)
                    .repeated()
                    .collect::<Vec<_>>(),
            )
            .map_with(|(first, rest), extra| {
                let mut path = vec![first];
                path.extend(rest);
                Spanned::new(Stmt::Use { path }, sp(extra.span()))
            });

        // Boxed: prevents exponential type growth per stmt variant.
        // typed_* must precede untyped — both start with ident, disambiguated by ':' vs '='/'  :='
        typed_copy_var
            .or(typed_var)
            .or(try_assign)
            .or(copy_var)
            .or(var)
            .or(r#return)
            .or(if_let)
            .or(r#if)
            .or(r#while)
            .or(r#for)
            .or(r#break)
            .or(r#continue)
            .or(r#fn)
            .or(spawn_stmt)
            .or(class_stmt)
            .or(interface_stmt)
            .or(impl_stmt)
            .or(use_stmt)
            .or(type_alias)
            .or(expr
                .clone()
                .map_with(|e, extra| Spanned::new(Stmt::Expr(e), sp(extra.span()))))
            .boxed()
    });

    stmt.repeated().collect::<Vec<_>>().then_ignore(end())
}
