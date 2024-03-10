pub mod lexer;
pub mod parser;

pub struct Ast {
    pub statements: Vec<ASTStatement>
}

impl Ast {
    pub fn new(statements: Vec<ASTStatement>) -> Self {
        Self { statements }
    }

    pub fn add_statement(&mut self, statement: ASTStatement) {
        self.statements.push(statement)
    }
    
}

pub enum ASTExpressionKind {
    Number(i64)
}

pub struct ASTExpression {
    kind: ASTExpressionKind
}

impl ASTExpression {
    pub fn new(kind: ASTExpressionKind) -> Self {
        Self { kind }
    }

    pub fn number(num: i64) -> Self {
        Self::new(ASTExpressionKind::Number(num))
    }
}

pub enum ASTStatementKind {
    Expression(ASTExpression)
}

pub struct ASTStatement {
    kind: ASTStatementKind
}

impl ASTStatement {
    pub fn new(kind: ASTStatementKind) -> Self {
        Self { kind }
    }

    pub fn expression(expr: ASTExpression) -> Self {
        Self::new(ASTStatementKind::Expression(expr))
    }
}