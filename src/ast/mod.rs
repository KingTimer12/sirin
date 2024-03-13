pub mod lexer;
pub mod parser;

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
    fn do_visit_expression(&mut self, expr: &ASTExpression) {
        match &expr.kind {
            ASTExpressionKind::Number(number) => {
                self.visit_number(number);
            }
        }
    }
    fn visit_expression(&mut self, expr: &ASTExpression) {
        self.do_visit_expression(expr)
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
        self.print_with_indent("Statement: ");
        self.indent += INCREMENT_INDENT_VALUE;
        ASTVisitor::do_visit_statement(self, stmt);
        self.indent -= INCREMENT_INDENT_VALUE
    }

    fn visit_expression(&mut self, expr: &ASTExpression) {
        self.print_with_indent("Expression: ");
        self.indent += INCREMENT_INDENT_VALUE;
        ASTVisitor::do_visit_expression(self, expr);
        self.indent -= INCREMENT_INDENT_VALUE
    }

    fn visit_number(&mut self, number: &ASTNumberExpression) {
        self.print_with_indent(&format!("Number: {}", number.number))
    }
}

pub enum ASTExpressionKind {
    Number(ASTNumberExpression),
}

pub struct ASTNumberExpression {
    number: i64
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