use super::{
    lexer::{Token, TokenKind},
    ASTBinaryOperator, ASTBinaryOperatorKind, ASTExpression, ASTStatement,
};

pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens: tokens
                .iter()
                .filter(|token| token.kind != TokenKind::Whitespace)
                .map(|token| token.clone())
                .collect(),
            current: 0,
        } // return Parser
    }

    pub fn next_statement(&mut self) -> Option<ASTStatement> {
        self.parse_statement() // return Option<ASTStatement>
    }

    fn parse_statement(&mut self) -> Option<ASTStatement> {
        let token = self.current()?;
        if token.kind == TokenKind::Eof {
            return None;
        }
        let expr = self.parse_expression()?;
        Some(ASTStatement::expression(expr)) // Option<ASTStatement>
    }

    fn parse_expression(&mut self) -> Option<ASTExpression> {
        self.parse_binary_expression(0) // return Option<ASTExpression>
    }
    
    fn parse_binary_expression(&mut self, precedence: u8) -> Option<ASTExpression> {
        let mut left = self.parse_primary_expression()?;

        while let Some(operator) = self.parse_operator() {
            self.consume();
            let operator_precedence = operator.precedence();
            if operator_precedence < precedence {
                break;
            }
            let right = self.parse_binary_expression(operator_precedence)?;
            left = ASTExpression::binary(operator, left, right)
        }

        Some(left) // return Option<ASTExpression>
    }

    // Operator Expression

    fn parse_operator(&mut self) -> Option<ASTBinaryOperator> {
        let token = self.current()?;
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

    fn parse_primary_expression(&mut self) -> Option<ASTExpression> {
        let token = self.consume()?;
        match token.kind {
            TokenKind::Number(number) => Some(ASTExpression::number(number)),
            TokenKind::LeftParen => {
                let expr = self.parse_expression()?;
                let token = self.consume()?;
                if token.kind  != TokenKind::RightParen {
                    panic!("Error")
                }
                Some(
                    ASTExpression::parenthesized(expr)
                )
            },
            _ => None,
        } // return ASTExpression
    }

    fn peek(&self, offset: isize) -> Option<&Token> {
        self.tokens.get((self.current as isize + offset) as usize) // return Option<&Token>
    }

    fn current(&mut self) -> Option<&Token> {
        self.peek(0) // return Option<&Token>
    }

    fn consume(&mut self) -> Option<&Token> {
        self.current += 1;
        let token = self.peek(-1)?;
        Some(token) // return Option<&Token>
    }
}
