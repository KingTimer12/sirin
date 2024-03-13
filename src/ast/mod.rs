use self::lexer::Token;

pub mod lexer;
pub mod parser;
pub mod evaluator;

pub struct Ast {
    pub statements: Vec<ASTStatement>
}

impl Ast {
    pub fn new() -> Self {
        Self { statements: Vec::new() }
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
        let mut printer = ASTPrinter { indent: 0 };
        self.visit(&mut printer);
    }
    
}

pub trait ASTVisitor {
    fn do_visit_statement(&mut self, stmt: &ASTStatement) {
        match &stmt.kind {
            ASTStatementKind::Expression(expr) => {
                self.visit_expression(expr);
            }
        }
    }
    fn visit_statement(&mut self, stmt: &ASTStatement) {
        self.do_visit_statement(stmt)
    }

    // Expression
    fn do_visit_expression(&mut self, expr: &ASTExpression) {
        match &expr.kind {
            ASTExpressionKind::Number(number) => {
                self.visit_number(number);
            },
            ASTExpressionKind::Binary(expr) => {
                self.visit_binary_expression(expr);
            },
            ASTExpressionKind::Parenthesized(expr) => {
                self.visit_parenthesized_expression(expr);
            }
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

    fn visit_number(&mut self, number: &ASTNumberExpression);
}

pub struct ASTPrinter {
    indent: usize
}

impl ASTPrinter {
    fn print_with_indent(&mut self, text: &str) {
        println!("{}{}", " ".repeat(self.indent), text)
    }
}

const INCREMENT_INDENT_VALUE: usize = 2;

impl ASTVisitor for ASTPrinter {
    fn visit_statement(&mut self, stmt: &ASTStatement) {
        self.print_with_indent("Statement:");
        self.indent += INCREMENT_INDENT_VALUE;
        ASTVisitor::do_visit_statement(self, stmt);
        self.indent -= INCREMENT_INDENT_VALUE
    }

    fn visit_expression(&mut self, expr: &ASTExpression) {
        self.print_with_indent("Expression:");
        self.indent += INCREMENT_INDENT_VALUE;
        ASTVisitor::do_visit_expression(self, expr);
        self.indent -= INCREMENT_INDENT_VALUE
    }

    fn visit_number(&mut self, number: &ASTNumberExpression) {
        self.print_with_indent(&format!("Number: {}", number.number))
    }

    fn visit_binary_expression(&mut self, binary_expr: &ASTBinaryExpression) {
        self.print_with_indent("Binary Expression:");
        self.indent += INCREMENT_INDENT_VALUE;
        self.print_with_indent(&format!("Operator: {:?}", binary_expr.operator.kind));
        self.visit_expression(&binary_expr.left);
        self.visit_expression(&binary_expr.right);
        self.indent -= INCREMENT_INDENT_VALUE;
    }

    fn visit_parenthesized_expression(&mut self, expr: &ASTParenthesizedExpression) {
        self.print_with_indent("Parenthesized Expression:");
        self.indent += INCREMENT_INDENT_VALUE;
        self.visit_expression(&expr.expression);
        self.indent -= INCREMENT_INDENT_VALUE;
    }
}

pub enum ASTExpressionKind {
    Number(ASTNumberExpression),
    Binary(ASTBinaryExpression),
    Parenthesized(ASTParenthesizedExpression)
}

// Binary

#[derive(Debug)]
pub enum ASTBinaryOperatorKind {
    Add, Subtract, Multiply, Divide
}

#[derive(Debug)]
pub struct ASTBinaryOperator {
    kind: ASTBinaryOperatorKind,
    token: Token
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
            ASTBinaryOperatorKind::Divide => 2
        }
    }
}

pub struct ASTBinaryExpression {
    left: Box<ASTExpression>,
    right: Box<ASTExpression>,
    operator: ASTBinaryOperator
}

// Number

pub struct ASTNumberExpression {
    number: i64
}

// Parenthesized

pub struct ASTParenthesizedExpression {
    expression: Box<ASTExpression>
}

pub struct ASTExpression {
    kind: ASTExpressionKind
}

impl ASTExpression {
    pub fn new(kind: ASTExpressionKind) -> Self {
        Self { kind }
    }

    pub fn number(number: i64) -> Self {
        Self::new(ASTExpressionKind::Number(ASTNumberExpression { number }))
    }

    pub fn binary(operator: ASTBinaryOperator, left: ASTExpression, right: ASTExpression) -> Self {
        Self::new(ASTExpressionKind::Binary(ASTBinaryExpression { left: Box::new(left), right: Box::new(right), operator }))
    }

    pub fn parenthesized(expr: ASTExpression) -> Self {
        Self::new(ASTExpressionKind::Parenthesized(ASTParenthesizedExpression { expression: Box::new(expr) }))
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