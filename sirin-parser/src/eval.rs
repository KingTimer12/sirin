use crate::expr::{BinOp, Expr};

impl Expr<'_> {
    pub fn eval(&self) -> i64 {
        match self {
            Expr::Int(n) => *n,
            Expr::Neg(rhs) => -rhs.node.eval(),
            Expr::BinOp(op, lhs, rhs) => match op {
                BinOp::Add => lhs.node.eval() + rhs.node.eval(),
                BinOp::Sub => lhs.node.eval() - rhs.node.eval(),
                BinOp::Mul => lhs.node.eval() * rhs.node.eval(),
                BinOp::Div => lhs.node.eval() / rhs.node.eval(),
                _ => unimplemented!(),
            },
            _ => unimplemented!(),
        }
    }
}
