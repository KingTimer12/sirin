use std::collections::HashMap;

use crate::ast::{ASTBinaryOperatorKind, ASTVisitor};

pub struct ASTEvaluator {
    pub last_value: Option<i64>,
    pub variables: HashMap<String, i64>
}

impl ASTEvaluator {
    pub fn new() -> Self {
        Self { last_value: None, variables: HashMap::new() }
    }
}

impl ASTVisitor for ASTEvaluator {
    fn visit_number_expression(&mut self, number: &super::ASTNumberExpression) {
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
    
    fn visit_error(&mut self, span: &super::lexer::TextSpan) {
        todo!()
    }

    fn visit_let_statement(&mut self, let_statement: &super::ASTLetStatement) {
        self.visit_expression(&let_statement.initializer);
        self.variables.insert(let_statement.identifier.span.literal.clone(), self.last_value.unwrap());
    }
    
    fn visit_variable_expression(&mut self, variable_expression: &super::ASTVariableExpression) {
        let literal = &variable_expression.identifier.span.literal;
        self.last_value = Some(*self.variables.get(literal).unwrap())
    }
    
}
