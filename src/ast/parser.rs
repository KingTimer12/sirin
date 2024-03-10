use super::{lexer::{Lexer, Token, TokenKind}, ASTExpression, ASTExpressionKind, ASTStatement};

pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>, current: usize) -> Self {
        Self { tokens, current }
    }

    pub fn empty() -> Self {
        Self::new(Vec::new(), 0)
    }

    pub fn from_input(input: &str) -> Self {
        let mut lexer = Lexer::new(input);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next_token() {
            tokens.push(token)
        }
        Self::new(tokens, 0)
    }

    pub fn next_statement(&mut self) -> Option<ASTStatement> {
        return self.parse_statement()
    }

    fn parse_statement(&mut self) -> Option<ASTStatement> {
        let token = self.current()?;
        let expr = self.parse_expression()?;
        Some(ASTStatement::expression(expr))
    }

    fn parse_expression(&mut self) -> Option<ASTExpression> {
        let token = self.current()?;
        return match token.kind {
            TokenKind::Number(number) => {
                Some(ASTExpression::number(number))
            },
            _ => {
                None
            }
        }
    }

    fn peek(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.current + offset)
    }

    fn current(&mut self) -> Option<&Token> {
        self.peek(0)
    }
}
