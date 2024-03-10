mod ast;

fn main() {
    let input = "7 + 3";

    let mut lexer = ast::lexer::Lexer::new(input);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next_token() {
        tokens.push(token)
    }
    println!("{:?}", tokens)
}
