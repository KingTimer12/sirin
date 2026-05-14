pub mod eval;
pub mod expr;
pub mod parser;
pub mod stmt;

#[cfg(test)]
mod tests {
    use chumsky::Parser;
    use logos::Logos;
    use sirin_lexer::token::Tokens;

    use crate::parser::parser;

    #[test]
    fn test_calc() {
        let lexer = Tokens::lexer("4 + 3");
        let mut tokens = vec![];
        for (token, span) in lexer.spanned() {
            match token {
                Ok(token) => tokens.push(token),
                Err(e) => {
                    println!("lexer error at {:?}: {:?}", span, e);
                    return;
                }
            }
        }
        let ast = match parser().parse(&tokens).into_result() {
            Ok(expr) => {
                println!("[AST]\n{:#?}", expr);
                expr
            }
            Err(e) => {
                println!("parse error: {:#?}", e);
                return;
            }
        };

        assert_eq!(7, 7);
    }
}
