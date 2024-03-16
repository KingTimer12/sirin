use std::cell::{Ref, RefMut};

use crate::diagnostics::{DiagnosticsBag, DiagnosticsBagCell};

use super::{
    lexer::{Token, TokenKind},
    ASTBinaryOperator, ASTBinaryOperatorKind, ASTExpression, ASTStatement,
};

pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
    diagnostics_bag: DiagnosticsBagCell,
}

impl Parser {
    pub fn new(tokens: Vec<Token>, diagnostics_bag: DiagnosticsBagCell) -> Self {
        Self {
            tokens: tokens
                .iter()
                .filter(|token| token.kind != TokenKind::Whitespace)
                .map(|token| token.clone())
                .collect(),
            current: 0,
            diagnostics_bag,
        } // return Parser
    }

    pub fn next_statement(&mut self) -> Option<ASTStatement> {
        if self.is_at_end() {
            return None;
        }
        Some(self.parse_statement()) // return Option<ASTStatement>
    }

    fn is_at_end(&self) -> bool {
        self.current().kind == TokenKind::Eof
    }

    fn parse_statement(&mut self) -> ASTStatement {
        let expr = self.parse_expression();
        ASTStatement::expression(expr) // ASTStatement
    }

    fn parse_expression(&mut self) -> ASTExpression {
        self.parse_binary_expression(0) // return ASTExpression
    }

    fn parse_binary_expression(&mut self, precedence: u8) -> ASTExpression {
        let mut left = self.parse_primary_expression();

        while let Some(operator) = self.parse_operator() {
            self.consume();
            let operator_precedence = operator.precedence();
            if operator_precedence < precedence {
                break;
            }
            let right = self.parse_binary_expression(operator_precedence);
            left = ASTExpression::binary(operator, left, right)
        }

        left // return ASTExpression
    }

    // Operator Expression

    fn parse_operator(&mut self) -> Option<ASTBinaryOperator> {
        let token = self.current();
        let kind = match token.kind {
            TokenKind::Plus => Some(ASTBinaryOperatorKind::Add),
            TokenKind::Minus => Some(ASTBinaryOperatorKind::Subtract),
            TokenKind::Asterisk => Some(ASTBinaryOperatorKind::Multiply),
            TokenKind::Slash => Some(ASTBinaryOperatorKind::Divide),
            _ => None,
        };
        kind.map(|kind| ASTBinaryOperator::new(kind, token.clone())) // return Option<ASTBinaryOperator>
    }

    // Generic Expression

    fn parse_primary_expression(&mut self) -> ASTExpression {
        let token = self.consume();
        match token.kind {
            TokenKind::Number(number) => ASTExpression::number(number),
            TokenKind::LeftParen => {
                let expr = self.parse_expression();
                let token = self.consume();
                if token.kind != TokenKind::RightParen {
                    panic!("Error")
                }
                ASTExpression::parenthesized(expr)
            }
            _ => {
                self.diagnostics_bag
                    .borrow_mut()
                    .report_expected_expression(&token);
                ASTExpression::error(token.span.clone())
            }
        } // return ASTExpression
    }

    fn peek(&self, offset: isize) -> &Token {
        let mut index = (self.current as isize + offset) as usize;
        if index >= self.tokens.len() {
            index = self.tokens.len() - 1;
        }
        self.tokens.get(index).unwrap() // return &Token
    }

    fn current(&mut self) -> &Token {
        self.peek(0) // return &Token
    }

    fn consume(&mut self) -> &Token {
        self.current += 1;
        self.peek(-1) // return &Token
    }

    fn consume_and_check(&mut self, kind: TokenKind) -> &Token {
        let token = self.consume();
        if token.kind != kind {
            self.diagnostics_bag
                .borrow_mut()
                .report_unexpected_token(&kind, token);
        }
        token
    }
}
