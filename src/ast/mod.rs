use termion::color;

use self::lexer::{TextSpan, Token};

pub mod evaluator;
pub mod lexer;
pub mod parser;

pub struct Ast {
    pub statements: Vec<ASTStatement>,
}

impl Ast {
    pub fn new() -> Self {
        Self {
            statements: Vec::new(),
        }
    }

    pub fn add_statement(&mut self, statement: ASTStatement) {
        self.statements.push(statement)
    }

    pub fn visit(&self, visitor: &mut dyn ASTVisitor) {
        for stmt in &self.statements {
            visitor.visit_statement(stmt)
        }
    }

    pub fn visualize(&self) -> () {
        let mut printer = ASTPrinter::new();
        self.visit(&mut printer);
        println!("{}", printer.result)
    }
}

pub trait ASTVisitor {
    fn do_visit_statement(&mut self, stmt: &ASTStatement) {
        match &stmt.kind {
            ASTStatementKind::Expression(expr) => {
                self.visit_expression(expr);
            }
            ASTStatementKind::LetStatement(expr) => {
                self.visit_let_statement(expr);
            }
        }
    }
    fn visit_let_statement(&mut self, let_statement: &ASTLetStatement) {
        self.visit_expression(&let_statement.initializer)
    }
    fn visit_statement(&mut self, stmt: &ASTStatement) {
        self.do_visit_statement(stmt)
    }

    // Expression
    fn do_visit_expression(&mut self, expr: &ASTExpression) {
        match &expr.kind {
            ASTExpressionKind::Number(number) => self.visit_number_expression(number),
            ASTExpressionKind::Binary(expr) => self.visit_binary_expression(expr),
            ASTExpressionKind::Parenthesized(expr) => self.visit_parenthesized_expression(expr),
            ASTExpressionKind::Variable(expr) => self.visit_variable_expression(expr),
            ASTExpressionKind::Error(span) => self.visit_error(span),
        }
    }

    // Generic Expression
    fn visit_expression(&mut self, expr: &ASTExpression) {
        self.do_visit_expression(expr)
    }

    // Binary Case
    fn visit_binary_expression(&mut self, binary_expr: &ASTBinaryExpression) {
        self.visit_expression(&binary_expr.left);
        self.visit_expression(&binary_expr.right)
    }

    // Parenthesized Expression
    fn visit_parenthesized_expression(&mut self, expr: &ASTParenthesizedExpression) {
        self.do_visit_expression(&expr.expression)
    }

    // Error
    fn visit_error(&mut self, span: &TextSpan);

    // Number Expression
    fn visit_number_expression(&mut self, number: &ASTNumberExpression);

    // Variable Expression
    fn visit_variable_expression(&mut self, variable_expression: &ASTVariableExpression);
}

pub struct ASTPrinter {
    indent: usize,
    result: String,
}

impl ASTPrinter {
    const NUM_COLOR: color::LightYellow = color::LightYellow;
    const KEYWOLD_COLOR: color::Magenta = color::Magenta;
    const TEXT_COLOR: color::LightWhite = color::LightWhite;
    const VAR_COLOR: color::White = color::White;
    const RESET_COLOR: color::Reset = color::Reset;

    pub fn new() -> Self {
        Self {
            indent: 0,
            result: String::new(),
        }
    }
    
    fn add_whitespace(&mut self) {
        self.result.push_str(" ");
    }

    fn add_newline(&mut self) {
        self.result.push_str("\n");
    }

    fn add_number(&mut self, number: i64) {
        let string = format!(
            "{}{}{}",
            Self::NUM_COLOR.fg_str(),
            number,
            Self::RESET_COLOR.fg_str()
        );
        self.result.push_str(&string)
    }

    fn add_color(&mut self, value: &str, color: &str) {
        let string = format!(
            "{}{}{}",
            color,
            value,
            Self::RESET_COLOR.fg_str()
        );
        self.result.push_str(&string)
    }

    fn add_text(&mut self, text: &str) {
        self.add_color(text, Self::TEXT_COLOR.fg_str())
    }

    fn add_keyword(&mut self, word: &str) {
        self.add_color(word, Self::KEYWOLD_COLOR.fg_str())
    }


}

impl ASTVisitor for ASTPrinter {
    fn visit_let_statement(&mut self, let_statement: &ASTLetStatement) {
        self.add_keyword("let");
        self.add_whitespace();
        self.add_text(&let_statement.identifier.span.literal);
        self.add_whitespace();
        self.add_text("=");
        self.add_whitespace();
        self.visit_expression(&let_statement.initializer);
        self.add_newline()
    }

    fn visit_number_expression(&mut self, number: &ASTNumberExpression) {
        self.add_number(number.number)
    }

    fn visit_error(&mut self, span: &TextSpan) {
        self.add_text(&span.literal)
    }

    fn visit_binary_expression(&mut self, binary_expr: &ASTBinaryExpression) {
        self.visit_expression(&binary_expr.left);
        self.add_whitespace();
        self.add_text(&binary_expr.operator.token.span.literal);
        self.add_whitespace();
        self.visit_expression(&binary_expr.right)
    }

    fn visit_parenthesized_expression(&mut self, expr: &ASTParenthesizedExpression) {
        self.add_text("(");
        self.visit_expression(&expr.expression);
        self.add_text(")")
    }

    fn visit_variable_expression(&mut self, variable_expression: &ASTVariableExpression) {
        self.add_color(variable_expression.identifier(), Self::VAR_COLOR.fg_str())
    }
}

pub enum ASTExpressionKind {
    Number(ASTNumberExpression),
    Binary(ASTBinaryExpression),
    Parenthesized(ASTParenthesizedExpression),
    Variable(ASTVariableExpression),
    Error(TextSpan),
}

// Variable

pub struct ASTVariableExpression {
    identifier: Token,
}

impl ASTVariableExpression {
    pub fn identifier(&self) -> &str {
        &self.identifier.span.literal
    }
}

// Binary

#[derive(Debug)]
pub enum ASTBinaryOperatorKind {
    Add,
    Subtract,
    Multiply,
    Divide,
}

#[derive(Debug)]
pub struct ASTBinaryOperator {
    kind: ASTBinaryOperatorKind,
    token: Token,
}

impl ASTBinaryOperator {
    pub fn new(kind: ASTBinaryOperatorKind, token: Token) -> Self {
        Self { kind, token }
    }

    pub fn precedence(&self) -> u8 {
        match self.kind {
            ASTBinaryOperatorKind::Add => 1,
            ASTBinaryOperatorKind::Subtract => 1,
            ASTBinaryOperatorKind::Multiply => 2,
            ASTBinaryOperatorKind::Divide => 2,
        }
    }
}

pub struct ASTBinaryExpression {
    left: Box<ASTExpression>,
    right: Box<ASTExpression>,
    operator: ASTBinaryOperator,
}

// Number

pub struct ASTNumberExpression {
    number: i64,
}

// Parenthesized

pub struct ASTParenthesizedExpression {
    expression: Box<ASTExpression>,
}

pub struct ASTExpression {
    kind: ASTExpressionKind,
}

impl ASTExpression {
    pub fn new(kind: ASTExpressionKind) -> Self {
        Self { kind }
    }

    pub fn number(number: i64) -> Self {
        Self::new(ASTExpressionKind::Number(ASTNumberExpression { number }))
    }

    pub fn error(span: TextSpan) -> Self {
        Self::new(ASTExpressionKind::Error(span))
    }

    pub fn binary(operator: ASTBinaryOperator, left: ASTExpression, right: ASTExpression) -> Self {
        Self::new(ASTExpressionKind::Binary(ASTBinaryExpression {
            left: Box::new(left),
            right: Box::new(right),
            operator,
        }))
    }

    pub fn parenthesized(expr: ASTExpression) -> Self {
        Self::new(ASTExpressionKind::Parenthesized(
            ASTParenthesizedExpression {
                expression: Box::new(expr),
            },
        ))
    }

    pub fn identifier(identifier: Token) -> Self {
        Self::new(ASTExpressionKind::Variable(
            ASTVariableExpression {
                identifier
            }
        ))
    }
}

pub struct ASTLetStatement {
    identifier: Token,
    initializer: ASTExpression,
}

pub enum ASTStatementKind {
    Expression(ASTExpression),
    LetStatement(ASTLetStatement),
}

pub struct ASTStatement {
    kind: ASTStatementKind,
}

impl ASTStatement {
    pub fn new(kind: ASTStatementKind) -> Self {
        Self { kind }
    }

    pub fn expression(expr: ASTExpression) -> Self {
        Self::new(ASTStatementKind::Expression(expr))
    }

    pub fn let_statement(identifier: Token, initializer: ASTExpression) -> Self {
        Self::new(ASTStatementKind::LetStatement(ASTLetStatement {
            identifier,
            initializer,
        }))
    }
}
