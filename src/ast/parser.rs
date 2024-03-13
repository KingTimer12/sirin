use super::{lexer::{Lexer, Token, TokenKind}, ASTExpression, ASTExpressionKind, ASTStatement};

pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>, current: usize) -> Self {
        Self { tokens, current } // return Parser
    }

    pub fn empty() -> Self {
        Self::new(Vec::new(), 0) // return Parser
    }

    pub fn from_input(input: &str) -> Self {
        let mut lexer = Lexer::new(input);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next_token() {
            tokens.push(token)
        }
        Self::new(tokens, 0) // return Parser
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
        let token = self.consume()?;
        match token.kind {
            TokenKind::Number(number) => {
                Some(ASTExpression::number(number))
            },
            _ => {
                None
            }
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
