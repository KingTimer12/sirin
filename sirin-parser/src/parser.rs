use chumsky::{
    IterParser, Parser,
    error::Simple,
    extra::Err,
    pratt::{infix, left, prefix},
    primitive::{choice, end, just},
    recursive::recursive,
    select,
};
use sirin_diagnostics::span::Span;
use sirin_lexer::token::Tokens;

use crate::{
    expr::{BinOp, Expr},
    span::Spanned,
    stmt::Stmt,
    types::Type,
};

fn sp(s: chumsky::span::SimpleSpan) -> Span {
    Span {
        start: s.start,
        end: s.end,
        file: String::new(),
    }
}

pub fn parser<'a>()
-> impl Parser<'a, &'a [Tokens<'a>], Vec<Spanned<Stmt<'a>>>, Err<Simple<'a, Tokens<'a>>>> {
    let ident_expr = select! { Tokens::Ident(n) => Expr::Var(n) };
    let ident_name = select! { Tokens::Ident(n) => n };
    let spanned_name = ident_name
        .clone()
        .map_with(|n, extra| Spanned::new(n, sp(extra.span())));

    let ty = choice((
        just(Tokens::IntType).to(Type::Int),
        just(Tokens::FloatType).to(Type::Float),
        just(Tokens::StringType).to(Type::Str),
        just(Tokens::BoolType).to(Type::Bool),
    ));

    let expr = recursive(|p| {
        let call = ident_name
            .then(
                p.clone()
                    .separated_by(just(Tokens::Comma))
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
            )
            .map_with(|(name, args), extra| Spanned::new(Expr::Call(name, args), sp(extra.span())));

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

            parenthesized
                .or(integer)
                .or(float)
                .or(string)
                .or(boolean)
                .or(call)
                .or(ident)
        };

        atom.pratt((
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
        let var = spanned_name
            .clone()
            .then_ignore(just(Tokens::Assign))
            .then(expr.clone())
            .map_with(|(name, rhs), extra| Spanned::new(Stmt::Let { name, rhs }, sp(extra.span())));

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

        var.or(r#return).or(r#if).or(r#fn).or(expr
            .clone()
            .map_with(|e, extra| Spanned::new(Stmt::Expr(e), sp(extra.span()))))
    });

    stmt.repeated().collect::<Vec<_>>().then_ignore(end())
}
