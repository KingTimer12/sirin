use std::{cell::RefCell, rc::Rc};

use crate::ast::lexer::{TextSpan, Token, TokenKind};

mod printer;

pub enum DiagnosticsKind {
    Error,
    Warning,
}

pub struct Diagnostics {
    pub message: String,
    pub span: TextSpan,
    pub kind: DiagnosticsKind,
}

impl Diagnostics {
    pub fn new(message: String, span: TextSpan, kind: DiagnosticsKind) -> Self {
        Self {
            message,
            span,
            kind,
        }
    }
}

pub type DiagnosticsBagCell = Rc<RefCell<DiagnosticsBag>>;

pub struct DiagnosticsBag {
    pub diagnostics: Vec<Diagnostics>,
}

impl DiagnosticsBag {
    pub fn new() -> Self {
        Self {
            diagnostics: vec![],
        }
    }

    pub fn report_error(&mut self, message: String, span: TextSpan) {
        let error = Diagnostics::new(message, span, DiagnosticsKind::Error);
        self.diagnostics.push(error)
    }

    pub fn report_warning(&mut self, message: String, span: TextSpan) {
        let warn = Diagnostics::new(message, span, DiagnosticsKind::Warning);
        self.diagnostics.push(warn)
    }

    pub fn report_unexpected_token(&mut self, expected: &TokenKind, token: &Token) {
        self.report_error(
            format!("Expected -> <{}> | Found -> <{}>", expected, token.kind),
            token.span.clone(),
        )
    }

    pub fn report_expected_expression(&mut self, token: &Token) {
        self.report_error(
            format!("Expected -> Expression | Found -> <{}>", token.kind),
            token.span.clone(),
        )
    }
}
