use crate::ast::{ASTBinaryOperatorKind, ASTVisitor};

pub struct ASTEvaluator {
    pub last_value: Option<i64>,
}

impl ASTEvaluator {
    pub fn new() -> Self {
        Self { last_value: None }
    }
}

impl ASTVisitor for ASTEvaluator {
    fn visit_number(&mut self, number: &super::ASTNumberExpression) {
        self.last_value = Some(number.number);
    }

    fn visit_binary_expression(&mut self, binary_expr: &super::ASTBinaryExpression) {
        self.visit_expression(&binary_expr.left);
        let left = self.last_value.unwrap();
        self.visit_expression(&binary_expr.right);
        let right = self.last_value.unwrap();
        self.last_value = Some(
            match binary_expr.operator.kind {
                ASTBinaryOperatorKind::Add => left + right,
                ASTBinaryOperatorKind::Subtract => left - right,
                ASTBinaryOperatorKind::Multiply => left * right,
                ASTBinaryOperatorKind::Divide => left / right,
            }
        )
    }
}
