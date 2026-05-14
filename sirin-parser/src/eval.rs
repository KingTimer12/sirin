use crate::expr::{BinOp, Expr};

impl Expr<'_> {
    pub fn eval(&self) -> i64 {
        match self {
            Expr::Int(n) => *n,
            Expr::Neg(rhs) => -rhs.eval(),
            Expr::BinOp(op, lhs, rhs) => match op {
                BinOp::Add => lhs.eval() + rhs.eval(),
                BinOp::Sub => lhs.eval() - rhs.eval(),
                BinOp::Mul => lhs.eval() * rhs.eval(),
                BinOp::Div => lhs.eval() / rhs.eval(),
                _ => unimplemented!(),
            },
            _ => unimplemented!(),
        }
    }
}
