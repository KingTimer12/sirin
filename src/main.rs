use std::{cell::RefCell, rc::Rc};

use crate::{
    ast::{evaluator::ASTEvaluator, lexer::Lexer, parser::Parser, Ast},
    diagnostics::DiagnosticsBagCell,
};

mod ast;
mod diagnostics;
mod text;

fn main() {
    let input = "(800 + 1000000 * 90000000000 / 5) * 0";

    let mut lexer = Lexer::new(input);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next_token() {
        tokens.push(token)
    }
    println!("{:?}", tokens);
    let diagnostics_bag: DiagnosticsBagCell =
        Rc::new(RefCell::new(diagnostics::DiagnosticsBag::new()));

    let mut ast = Ast::new();
    let mut parser = Parser::new(tokens, diagnostics_bag);
    while let Some(stmt) = parser.next_statement() {
        ast.add_statement(stmt);
    }
    ast.visualize();
    let mut eval = ASTEvaluator::new();
    ast.visit(&mut eval);
    println!("{:?}", eval.last_value);
}
