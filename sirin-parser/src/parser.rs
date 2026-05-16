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
    stmt::Stmt,
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

    // Boxed: cloned 3 times (Array/Vec/Set) — Arc clone, O(1)
    let bracket = ty_atom
        .clone()
        .delimited_by(just(Tokens::LBracket), just(Tokens::RBracket))
        .boxed();

    let map_bracket = ty_atom
        .clone()
        .then_ignore(just(Tokens::Comma))
        .then(ty_atom.clone())
        .delimited_by(just(Tokens::LBracket), just(Tokens::RBracket));

    // Boxed: used in arg and return-type positions; stops type from infecting stmt chain
    let ty = choice((
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
        ty_atom,
    )).boxed();

    // Postfix ops collected per-token; stored with their span for correct tree spans
    enum Postfix<'b> {
        Index(Spanned<Expr<'b>>, Span),
        Method(&'b str, Vec<Spanned<Expr<'b>>>, Span),
    }

    let expr = recursive(|p| {
        let call = ident_name
            .then(
                p.clone()
                    .separated_by(just(Tokens::Comma))
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
            )
            .map_with(|(name, args), extra| Spanned::new(Expr::Call(name, args), sp(extra.span())));

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

            // Array literal: [expr, expr, ...]
            let array_lit = p
                .clone()
                .separated_by(just(Tokens::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Tokens::LBracket), just(Tokens::RBracket))
                .map_with(|items, extra| Spanned::new(Expr::Array(items), sp(extra.span())));

            // Collection constructors: Vec(n), Map(), Set(), Array(n)
            // These use reserved type-keyword tokens, not Ident, so need separate parsers.
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

            array_lit
                .or(parenthesized)
                .or(integer)
                .or(float)
                .or(string)
                .or(boolean)
                .or(vec_ctor)
                .or(map_ctor)
                .or(set_ctor)
                .or(array_ctor)
                .or(call)
                .or(ident)
        }.boxed();

        // Postfix: index [expr] and method call .name(args), left-associative
        let postfix_index = p
            .clone()
            .delimited_by(just(Tokens::LBracket), just(Tokens::RBracket))
            .map_with(|idx, extra| Postfix::Index(idx, sp(extra.span())));

        let postfix_method = just(Tokens::Dot)
            .ignore_then(ident_name.clone())
            .then(
                p.clone()
                    .separated_by(just(Tokens::Comma))
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
            )
            .map_with(|(name, args), extra| Postfix::Method(name, args, sp(extra.span())));

        let primary = atom
            .then(postfix_index.or(postfix_method).repeated().collect::<Vec<_>>())
            .map(|(base, ops)| {
                ops.into_iter().fold(base, |acc, op| match op {
                    Postfix::Index(idx, span) => {
                        Spanned::new(Expr::Index(Box::new(acc), Box::new(idx)), span)
                    }
                    Postfix::Method(name, args, span) => {
                        Spanned::new(Expr::MethodCall(Box::new(acc), name, args), span)
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
            .map_with(|(name, rhs), extra| Spanned::new(Stmt::Let { name, ty: None, rhs }, sp(extra.span())));

        let copy_var = spanned_name
            .clone()
            .then_ignore(just(Tokens::ColonAssign))
            .then(expr.clone())
            .map_with(|(name, rhs), extra| Spanned::new(Stmt::CopyLet { name, ty: None, rhs }, sp(extra.span())));

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

        let r#fn = just(Tokens::Fn)
            .ignore_then(spanned_name.clone())
            .then(
                arg.separated_by(just(Tokens::Comma))
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
            )
            .then(just(Tokens::Arrow).ignore_then(ty.clone()).or_not())
            .then(fn_body)
            .map_with(|(((name, args), return_type), body), extra| {
                Spanned::new(
                    Stmt::Fn {
                        name,
                        args,
                        return_type,
                        body,
                    },
                    sp(extra.span()),
                )
            });

        // Boxed: prevents exponential type growth per stmt variant.
        // typed_* must precede untyped — both start with ident, disambiguated by ':' vs '='/'  :='
        typed_copy_var.or(typed_var).or(copy_var).or(var).or(r#return).or(r#if).or(r#fn).or(expr
            .clone()
            .map_with(|e, extra| Spanned::new(Stmt::Expr(e), sp(extra.span())))).boxed()
    });

    stmt.repeated().collect::<Vec<_>>().then_ignore(end())
}
