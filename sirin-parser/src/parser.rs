use chumsky::{
    IterParser, Parser,
    error::Simple,
    extra::Err,
    pratt::{infix, left, prefix},
    primitive::{choice, end, just},
    recursive::recursive,
    select,
};
use sirin_lexer::token::Tokens;

use crate::{
    expr::{BinOp, Expr},
    stmt::Stmt,
    types::Type,
};

pub fn parser<'a>() -> impl Parser<'a, &'a [Tokens<'a>], Vec<Stmt<'a>>, Err<Simple<'a, Tokens<'a>>>>
{
    let ident_expr = select! { Tokens::Ident(n) => Expr::Var(n) };
    let ident_name = select! { Tokens::Ident(n) => n };

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
            .map(|(name, args)| Expr::Call(name, args));

        let atom = {
            let parenthesized = p
                .clone()
                .delimited_by(just(Tokens::LParen), just(Tokens::RParen));
            let integer = select! { Tokens::Integer(n) => Expr::Int(n) };
            let float = select! { Tokens::Float(n)   => Expr::Float(n) };
            let string = select! { Tokens::Str(n)     => Expr::Str(n) };
            let boolean = select! { Tokens::Boolean(n) => Expr::Boolean(n) };

            parenthesized
                .or(integer)
                .or(float)
                .or(string)
                .or(boolean)
                .or(call)
                .or(ident_expr)
        };

        atom.pratt((
            // prefix
            prefix(6, just(Tokens::Minus), |_, rhs, _| Expr::Neg(Box::new(rhs))), // -x
            prefix(6, just(Tokens::Not), |_, rhs, _| Expr::Not(Box::new(rhs))),   // !x
            // product — precedência 5
            infix(left(5), just(Tokens::Multiply), |lhs, _, rhs, _| {
                Expr::BinOp(BinOp::Mul, Box::new(lhs), Box::new(rhs)) // x * y
            }),
            infix(left(5), just(Tokens::Divide), |lhs, _, rhs, _| {
                Expr::BinOp(BinOp::Div, Box::new(lhs), Box::new(rhs)) // x / y
            }),
            // sum — precedência 4
            infix(left(4), just(Tokens::Plus), |lhs, _, rhs, _| {
                Expr::BinOp(BinOp::Add, Box::new(lhs), Box::new(rhs)) // x + y
            }),
            infix(left(4), just(Tokens::Minus), |lhs, _, rhs, _| {
                Expr::BinOp(BinOp::Sub, Box::new(lhs), Box::new(rhs)) // x - y
            }),
            // comparison — precedência 3
            infix(left(3), just(Tokens::Eq), |lhs, _, rhs, _| {
                Expr::BinOp(BinOp::Eq, Box::new(lhs), Box::new(rhs)) // x == y
            }),
            infix(left(3), just(Tokens::NotEq), |lhs, _, rhs, _| {
                Expr::BinOp(BinOp::NotEq, Box::new(lhs), Box::new(rhs)) // x != y
            }),
            infix(left(3), just(Tokens::Gt), |lhs, _, rhs, _| {
                Expr::BinOp(BinOp::Gt, Box::new(lhs), Box::new(rhs)) // x > y
            }),
            infix(left(3), just(Tokens::Lt), |lhs, _, rhs, _| {
                Expr::BinOp(BinOp::Lt, Box::new(lhs), Box::new(rhs)) // x < y
            }),
            infix(left(3), just(Tokens::GtEq), |lhs, _, rhs, _| {
                Expr::BinOp(BinOp::GtEq, Box::new(lhs), Box::new(rhs)) // x >= y
            }),
            infix(left(3), just(Tokens::LtEq), |lhs, _, rhs, _| {
                Expr::BinOp(BinOp::LtEq, Box::new(lhs), Box::new(rhs)) // x <= y
            }),
            // logical — precedência 2
            infix(left(2), just(Tokens::And), |lhs, _, rhs, _| {
                Expr::BinOp(BinOp::And, Box::new(lhs), Box::new(rhs)) // x and y
            }),
            infix(left(2), just(Tokens::Or), |lhs, _, rhs, _| {
                Expr::BinOp(BinOp::Or, Box::new(lhs), Box::new(rhs)) // x or y
            }),
        ))
    });

    let stmt = recursive(|decl| {
        let var = ident_name
            .then_ignore(just(Tokens::Assign))
            .then(expr.clone())
            .map(|(name, rhs)| Stmt::Let { name, rhs });

        let r#return = just(Tokens::Return)
            .ignore_then(expr.clone().or_not())
            .then(
                just(Tokens::If)
                    .ignore_then(expr.clone())
                    .or_not()
            )
            .map(|(value, cond)| Stmt::Return {
                value: value.map(Box::new),
                cond:  cond.map(Box::new),
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
            .map(|((cond, then), else_)| Stmt::If {
                cond: Box::new(cond),
                then,
                else_,
            });

        let fn_body = choice((
            just(Tokens::FatArrow)
                .ignore_then(expr.clone())
                .map(|body| vec![Stmt::Return {
                    value: Some(Box::new(body)),
                    cond: None,
                }]),
            decl.clone()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Tokens::BlockStart), just(Tokens::BlockEnd)),
        ));

        let r#fn = just(Tokens::Fn)
            .ignore_then(ident_name)
            .then(
                ident_name
                    .then_ignore(just(Tokens::Colon))
                    .then(ty.clone())
                    .separated_by(just(Tokens::Comma))
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
            )
            .then(just(Tokens::Arrow).ignore_then(ty.clone()).or_not())
            .then(fn_body)
            .map(|(((name, args), return_type), body)| Stmt::Fn {
                name,
                args,
                return_type,
                body,
            });

        var.or(r#return)
            .or(r#if)
            .or(r#fn)
            .or(expr.clone().map(Stmt::Expr))
    });

    stmt.repeated().collect::<Vec<_>>().then_ignore(end())
}
