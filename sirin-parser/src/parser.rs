use chumsky::{
    IterParser, Parser,
    error::Simple,
    extra::Err,
    primitive::{choice, just},
    recursive::recursive,
    select,
};
use sirin_lexer::token::Tokens;

use crate::{
    expr::{BinOp, Expr},
    stmt::Stmt,
};

pub fn parser<'a>() -> impl Parser<'a, &'a [Tokens<'a>], Stmt<'a>, Err<Simple<'a, Tokens<'a>>>> {
    let ident_expr = select! { Tokens::Ident(n) => Expr::Var(n) };
    let ident_name = select! { Tokens::Ident(n) => n };

    let expr = recursive(|p| {
        // call -> soma(x, y)
        let call = ident_name
            .then(
                p.clone()
                    .separated_by(just(Tokens::Comma))
                    .collect::<Vec<_>>()
                    .delimited_by(just(Tokens::LParen), just(Tokens::RParen)),
            )
            .map(|(name, args)| Expr::Call(name, args));

        let atom = {
            // parenthesized -> ()
            let parenthesized = p
                .clone()
                .delimited_by(just(Tokens::LParen), just(Tokens::RParen));

            let integer = select! { Tokens::Integer(n) => Expr::Int(n) }; // 4
            let float = select! { Tokens::Float(n) => Expr::Float(n) }; // 4.3
            let string = select! { Tokens::Str(n) => Expr::Str(n) }; // "str"
            let boolean = select! { Tokens::Boolean(n) => Expr::Boolean(n) }; //true ou false

            parenthesized
                .or(integer)
                .or(float)
                .or(string)
                .or(boolean)
                .or(call)
                .or(ident_expr)
        };

        // -x | -soma(x, y)
        let unary = just(Tokens::Minus)
            .repeated()
            .foldr(atom, |_op, rhs| Expr::Neg(Box::new(rhs)));

        // x * y | x / y
        let product = unary.clone().foldl(
            choice((
                just(Tokens::Multiply).to(BinOp::Mul),
                just(Tokens::Divide).to(BinOp::Div),
            ))
            .then(unary)
            .repeated(),
            |lhs, (op, rhs)| Expr::BinOp(op, Box::new(lhs), Box::new(rhs)),
        );

        // x + y // x - y
        let sum = product.clone().foldl(
            choice((
                just(Tokens::Plus).to(BinOp::Add),
                just(Tokens::Minus).to(BinOp::Sub),
            ))
            .then(product)
            .repeated(),
            |lhs, (op, rhs)| Expr::BinOp(op, Box::new(lhs), Box::new(rhs)),
        );

        // x == y // x != y // x >= y // x <= y // x > y // x < y
        let comparison = sum.clone().foldl(
            choice((
                just(Tokens::Eq).to(BinOp::Eq),
                just(Tokens::NotEq).to(BinOp::NotEq),
                just(Tokens::GtEq).to(BinOp::GtEq),
                just(Tokens::LtEq).to(BinOp::LtEq),
                just(Tokens::Gt).to(BinOp::Gt),
                just(Tokens::Lt).to(BinOp::Lt),
            ))
            .then(sum)
            .repeated(),
            |lhs, (op, rhs)| Expr::BinOp(op, Box::new(lhs), Box::new(rhs)),
        );

        // x and y | x or y
        let logical = comparison.clone().foldl(
            choice((
                just(Tokens::And).to(BinOp::And),
                just(Tokens::Or).to(BinOp::Or),
            ))
            .then(comparison)
            .repeated(),
            |lhs, (op, rhs)| Expr::BinOp(op, Box::new(lhs), Box::new(rhs)),
        );

        logical
    });

    recursive(|decl| {
        let var = ident_name
            .then_ignore(just(Tokens::Assign))
            .then(expr.clone())
            .map(|(name, rhs)| Stmt::Let { name, rhs });

        let r#return = just(Tokens::Return)
            .ignore_then(expr.clone().or_not())
            .map(|value| Stmt::Return(value.map(Box::new)));

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

        var
    })
}
