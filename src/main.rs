use crate::ast::{evaluator::ASTEvaluator, lexer::Lexer, parser::Parser, Ast};

mod ast;

fn main() {
    let input = "(800 + 1000000 * 90000000000 / 5) * 0";

    let mut lexer = Lexer::new(input);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next_token() {
        tokens.push(token)
    }
    println!("{:?}", tokens);
    let mut ast = Ast::new();
    let mut parser = Parser::new(tokens);
    while let Some(stmt) = parser.next_statement() {
        ast.add_statement(stmt);
    }
    ast.visualize();
    let mut eval = ASTEvaluator::new();
    ast.visit(&mut eval);
    println!("{:?}", eval.last_value);
}
