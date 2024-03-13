use crate::ast::{lexer::Lexer, parser::Parser, Ast};

mod ast;

fn main() {
    let input = "2 + 2 * 2";

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
    ast.visualize()
}
