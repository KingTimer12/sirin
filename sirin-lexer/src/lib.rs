pub mod error;
pub mod span;
pub mod token;

#[cfg(test)]
mod tests {
    use logos::Logos;

    use crate::{error::LexingError, token::Tokens};

    fn tok(src: &str) -> Vec<crate::span::SpannedToken<'_>> {
        Tokens::tokenize(src, "test")
    }

    fn tok_no_ws(src: &str) -> Vec<crate::span::SpannedToken<'_>> {
        Tokens::tokenize(src, "test")
            .into_iter()
            .filter(|t| t.node != Tokens::Whitespace)
            .collect()
    }

    // --- int / float literals ---

    #[test]
    fn test_tokens_lexer_int() {
        let src = "a = 5";
        let tokens = tok(src);
        // [Ident("a"), Whitespace, Assign, Whitespace, Integer(5)]
        assert_eq!(tokens[0].node, Tokens::Ident("a"));
        assert_eq!(&src[tokens[0].span.start..tokens[0].span.end], "a");
        assert_eq!(tokens[2].node, Tokens::Assign);
        assert_eq!(&src[tokens[2].span.start..tokens[2].span.end], "=");
        assert_eq!(tokens[4].node, Tokens::Integer(5));
        assert_eq!(&src[tokens[4].span.start..tokens[4].span.end], "5");
    }

    #[test]
    fn test_tokens_lexer_float() {
        let src = "a = 0.4";
        let tokens = tok(src);
        assert_eq!(tokens[0].node, Tokens::Ident("a"));
        assert_eq!(&src[tokens[0].span.start..tokens[0].span.end], "a");
        assert_eq!(tokens[2].node, Tokens::Assign);
        assert_eq!(&src[tokens[2].span.start..tokens[2].span.end], "=");
        assert_eq!(tokens[4].node, Tokens::Float(0.4));
        assert_eq!(&src[tokens[4].span.start..tokens[4].span.end], "0.4");
    }

    #[test]
    fn test_tokens_lexer_float_scientific() {
        let tokens = tok("1.5e3");
        assert_eq!(tokens[0].node, Tokens::Float(1500.0));
    }

    // --- errors: raw lexer (tokenize drops errors) ---

    #[test]
    fn test_error_non_ascii() {
        let mut text = Tokens::lexer("ñ");
        assert_eq!(
            text.next(),
            Some(Err(LexingError::NonAsciiCharacter {
                char: 'ñ',
                byte: 'ñ' as u8,
            }))
        );
    }

    #[test]
    fn test_error_integer_overflow() {
        let mut text = Tokens::lexer("99999999999999999999");
        assert_eq!(
            text.next(),
            Some(Err(LexingError::InvalidInteger {
                value: "99999999999999999999".to_owned(),
                reason: "value too large for i64".to_owned(),
            }))
        );
    }

    #[test]
    fn test_error_unknown_char() {
        let mut text = Tokens::lexer("@");
        assert_eq!(
            text.next(),
            Some(Err(LexingError::NonAsciiCharacter {
                char: '@',
                byte: b'@',
            }))
        );
    }

    // --- math operators ---

    #[test]
    fn test_math_operators() {
        let tokens = tok_no_ws("+ - * /");
        assert_eq!(tokens[0].node, Tokens::Plus);
        assert_eq!(tokens[1].node, Tokens::Minus);
        assert_eq!(tokens[2].node, Tokens::Multiply);
        assert_eq!(tokens[3].node, Tokens::Divide);
    }

    #[test]
    fn test_arithmetic_expression() {
        let tokens = tok_no_ws("x = 10 + 3 * 2");
        assert_eq!(tokens[0].node, Tokens::Ident("x"));
        assert_eq!(tokens[1].node, Tokens::Assign);
        assert_eq!(tokens[2].node, Tokens::Integer(10));
        assert_eq!(tokens[3].node, Tokens::Plus);
        assert_eq!(tokens[4].node, Tokens::Integer(3));
        assert_eq!(tokens[5].node, Tokens::Multiply);
        assert_eq!(tokens[6].node, Tokens::Integer(2));
    }

    // --- function declaration ---

    #[test]
    fn test_fn_declaration() {
        let src = "fn soma(a: int, b: int) -> int {\n  a + b\n}";
        let tokens = tok_no_ws(src);
        assert_eq!(tokens[0].node, Tokens::Fn);
        assert_eq!(tokens[1].node, Tokens::Ident("soma"));
        assert_eq!(tokens[2].node, Tokens::LParen);
        assert_eq!(tokens[3].node, Tokens::Ident("a"));
        assert_eq!(tokens[4].node, Tokens::Colon);
        assert_eq!(tokens[5].node, Tokens::IntType);
        assert_eq!(tokens[6].node, Tokens::Comma);
        assert_eq!(tokens[7].node, Tokens::Ident("b"));
        assert_eq!(tokens[8].node, Tokens::Colon);
        assert_eq!(tokens[9].node, Tokens::IntType);
        assert_eq!(tokens[10].node, Tokens::RParen);
        assert_eq!(tokens[11].node, Tokens::Arrow);
        assert_eq!(tokens[12].node, Tokens::IntType);
        assert_eq!(tokens[13].node, Tokens::BlockStart);
        assert_eq!(tokens[14].node, Tokens::Ident("a"));
        assert_eq!(tokens[15].node, Tokens::Plus);
        assert_eq!(tokens[16].node, Tokens::Ident("b"));
        assert_eq!(tokens[17].node, Tokens::BlockEnd);
    }

    #[test]
    fn test_fn_keyword_not_ident() {
        let tokens = tok("fn");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].node, Tokens::Fn);
    }

    #[test]
    fn test_type_keywords() {
        let tokens = tok_no_ws("int float");
        assert_eq!(tokens[0].node, Tokens::IntType);
        assert_eq!(tokens[1].node, Tokens::FloatType);
    }

    // --- parens and blocks ---

    #[test]
    fn test_parens() {
        let tokens = tok("(a)");
        assert_eq!(tokens[0].node, Tokens::LParen);
        assert_eq!(tokens[1].node, Tokens::Ident("a"));
        assert_eq!(tokens[2].node, Tokens::RParen);
    }

    #[test]
    fn test_block_delimiters() {
        let tokens = tok("{}");
        assert_eq!(tokens[0].node, Tokens::BlockStart);
        assert_eq!(tokens[1].node, Tokens::BlockEnd);
    }

    // --- arrow ---

    #[test]
    fn test_arrow() {
        let tokens = tok("->");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].node, Tokens::Arrow);
    }

    // --- whitespace ---

    #[test]
    fn test_whitespace_tabs_newlines() {
        let tokens = tok("a\t\nb");
        // tokenize keeps whitespace tokens
        assert_eq!(tokens[0].node, Tokens::Ident("a"));
        assert_eq!(tokens[1].node, Tokens::Whitespace);
        assert_eq!(tokens[2].node, Tokens::Ident("b"));
    }

    // --- fat arrow / single-line functions ---

    #[test]
    fn test_fat_arrow_token() {
        let tokens = tok("=>");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].node, Tokens::FatArrow);
    }

    #[test]
    fn test_fn_single_line_int() {
        let tokens = tok_no_ws("fn dobrar(x: int) => x * 2");
        assert_eq!(tokens[0].node, Tokens::Fn);
        assert_eq!(tokens[1].node, Tokens::Ident("dobrar"));
        assert_eq!(tokens[2].node, Tokens::LParen);
        assert_eq!(tokens[3].node, Tokens::Ident("x"));
        assert_eq!(tokens[4].node, Tokens::Colon);
        assert_eq!(tokens[5].node, Tokens::IntType);
        assert_eq!(tokens[6].node, Tokens::RParen);
        assert_eq!(tokens[7].node, Tokens::FatArrow);
        assert_eq!(tokens[8].node, Tokens::Ident("x"));
        assert_eq!(tokens[9].node, Tokens::Multiply);
        assert_eq!(tokens[10].node, Tokens::Integer(2));
    }

    #[test]
    fn test_fn_single_line_float() {
        let tokens = tok_no_ws("fn metade(x: float) => x / 2.0");
        assert_eq!(tokens[0].node, Tokens::Fn);
        assert_eq!(tokens[1].node, Tokens::Ident("metade"));
        assert_eq!(tokens[2].node, Tokens::LParen);
        assert_eq!(tokens[3].node, Tokens::Ident("x"));
        assert_eq!(tokens[4].node, Tokens::Colon);
        assert_eq!(tokens[5].node, Tokens::FloatType);
        assert_eq!(tokens[6].node, Tokens::RParen);
        assert_eq!(tokens[7].node, Tokens::FatArrow);
        assert_eq!(tokens[8].node, Tokens::Ident("x"));
        assert_eq!(tokens[9].node, Tokens::Divide);
        assert_eq!(tokens[10].node, Tokens::Float(2.0));
    }

    #[test]
    fn test_fn_single_line_two_params() {
        let tokens = tok_no_ws("fn soma(a: int, b: int) => a + b");
        assert_eq!(tokens[0].node, Tokens::Fn);
        assert_eq!(tokens[7].node, Tokens::Ident("b"));
        assert_eq!(tokens[8].node, Tokens::Colon);
        assert_eq!(tokens[9].node, Tokens::IntType);
        assert_eq!(tokens[10].node, Tokens::RParen);
        assert_eq!(tokens[11].node, Tokens::FatArrow);
        assert_eq!(tokens[12].node, Tokens::Ident("a"));
        assert_eq!(tokens[13].node, Tokens::Plus);
        assert_eq!(tokens[14].node, Tokens::Ident("b"));
    }

    #[test]
    fn test_fat_arrow_not_confused_with_arrow() {
        let tokens = tok_no_ws("-> =>");
        assert_eq!(tokens[0].node, Tokens::Arrow);
        assert_eq!(tokens[1].node, Tokens::FatArrow);
    }

    // --- negative float ---

    #[test]
    fn test_negative_float() {
        let tokens = tok("-1.5");
        assert_eq!(tokens[0].node, Tokens::Float(-1.5));
    }
}
