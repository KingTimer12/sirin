use std::{cell::RefCell, fs, path::Path, rc::Rc};

use crate::{
    ast::{evaluator::ASTEvaluator, lexer::Lexer, parser::Parser, Ast},
    diagnostics::DiagnosticsBagCell,
};

mod ast;
mod diagnostics;
mod text;

fn main() {
    let input = fs::read_to_string(Path::new("examples/main.sn")).expect("Should have been able to read the file");
    let text = &text::SourceText::new(input.clone());

    let mut lexer = Lexer::new(&input);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next_token() {
        tokens.push(token)
    }
    // println!("{:?}", tokens);
    let diagnostics_bag: DiagnosticsBagCell =
        Rc::new(RefCell::new(diagnostics::DiagnosticsBag::new()));

    let mut ast = Ast::new();
    let mut parser = Parser::new(tokens, Rc::clone(&diagnostics_bag));
    while let Some(stmt) = parser.next_statement() {
        ast.add_statement(stmt);
    }
    ast.visualize();
    let diagnostics_binding = diagnostics_bag.borrow();
    if diagnostics_binding.diagnostics.len() > 0 {
        let diagnostics_printer =
            diagnostics::printer::DiagnosticsPrinter::new(text, &diagnostics_binding.diagnostics);
        return diagnostics_printer.print();
    }
    let mut eval = ASTEvaluator::new();
    ast.visit(&mut eval);
    println!("{:?}", eval.last_value);
}
