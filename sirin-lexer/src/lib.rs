pub mod error;
pub mod token;

#[cfg(test)]
mod tests {
    use crate::{error::LexingError, token::Tokens};
    use logos::Logos;

    #[test]
    fn test_tokens_lexer_int() {
        let mut text = Tokens::lexer("a = 5");

        assert_eq!(text.next(), Some(Ok(Tokens::Ident("a"))));
        assert_eq!(text.slice(), "a");
        assert_eq!(text.next(), Some(Ok(Tokens::Whitespace)));
        assert_eq!(text.next(), Some(Ok(Tokens::Eq)));
        assert_eq!(text.slice(), "=");
        assert_eq!(text.next(), Some(Ok(Tokens::Whitespace)));
        assert_eq!(text.next(), Some(Ok(Tokens::Integer(5))));
        assert_eq!(text.slice(), "5");
    }

    #[test]
    fn test_tokens_lexer_float() {
        let mut text = Tokens::lexer("a = 0.4");

        assert_eq!(text.next(), Some(Ok(Tokens::Ident("a"))));
        assert_eq!(text.slice(), "a");
        assert_eq!(text.next(), Some(Ok(Tokens::Whitespace)));
        assert_eq!(text.next(), Some(Ok(Tokens::Eq)));
        assert_eq!(text.slice(), "=");
        assert_eq!(text.next(), Some(Ok(Tokens::Whitespace)));
        assert_eq!(text.next(), Some(Ok(Tokens::Float(0.4))));
        assert_eq!(text.slice(), "0.4");
    }

    #[test]
    fn test_tokens_lexer_float_scientific() {
        let mut text = Tokens::lexer("1.5e3");
        assert_eq!(text.next(), Some(Ok(Tokens::Float(1500.0))));
    }

    #[test]
    fn test_error_non_ascii() {
        let mut text = Tokens::lexer("ñ");
        // byte = 'ñ' as u8 truncates to 241 (U+00F1)
        assert_eq!(
            text.next(),
            Some(Err(LexingError::NonAsciiCharacter {
                char: 'ñ',
                byte: 'ñ' as u8
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
        // '@' is ASCII so from_lexer fires — NonAsciiCharacter, not Other
        assert_eq!(
            text.next(),
            Some(Err(LexingError::NonAsciiCharacter {
                char: '@',
                byte: b'@'
            }))
        );
    }

    // --- math operators ---

    #[test]
    fn test_math_operators() {
        let tokens: Vec<_> = Tokens::lexer("+ - * /").collect();
        assert_eq!(tokens[0], Ok(Tokens::Plus));
        assert_eq!(tokens[2], Ok(Tokens::Minus));
        assert_eq!(tokens[4], Ok(Tokens::Multiply));
        assert_eq!(tokens[6], Ok(Tokens::Divide));
    }

    #[test]
    fn test_arithmetic_expression() {
        let src = "x = 10 + 3 * 2";
        let tokens: Vec<_> = Tokens::lexer(src)
            .filter(|t| !matches!(t, Ok(Tokens::Whitespace)))
            .collect();
        assert_eq!(tokens[0], Ok(Tokens::Ident("x")));
        assert_eq!(tokens[1], Ok(Tokens::Eq));
        assert_eq!(tokens[2], Ok(Tokens::Integer(10)));
        assert_eq!(tokens[3], Ok(Tokens::Plus));
        assert_eq!(tokens[4], Ok(Tokens::Integer(3)));
        assert_eq!(tokens[5], Ok(Tokens::Multiply));
        assert_eq!(tokens[6], Ok(Tokens::Integer(2)));
    }

    // --- function declaration ---

    #[test]
    fn test_fn_declaration() {
        let src = r#"
        fn soma(a: int, b: int) -> int {
          a + b
        }
        "#;
        let tokens: Vec<_> = Tokens::lexer(src)
            .filter(|t| !matches!(t, Ok(Tokens::Whitespace)))
            .collect();
        assert_eq!(tokens[0], Ok(Tokens::Fn));
        assert_eq!(tokens[1], Ok(Tokens::Ident("soma"))); // add
        assert_eq!(tokens[2], Ok(Tokens::LParen));
        assert_eq!(tokens[3], Ok(Tokens::Ident("a"))); // a
        assert_eq!(tokens[4], Ok(Tokens::Colon));
        assert_eq!(tokens[5], Ok(Tokens::IntType));
        assert_eq!(tokens[7], Ok(Tokens::Ident("b"))); // b
        assert_eq!(tokens[8], Ok(Tokens::Colon));
        assert_eq!(tokens[9], Ok(Tokens::IntType));
        assert_eq!(tokens[10], Ok(Tokens::RParen));
        assert_eq!(tokens[11], Ok(Tokens::Arrow));
        assert_eq!(tokens[12], Ok(Tokens::IntType));
        assert_eq!(tokens[13], Ok(Tokens::BlockStart));
        assert_eq!(tokens[14], Ok(Tokens::Ident("a"))); // a
        assert_eq!(tokens[15], Ok(Tokens::Plus)); // +
        assert_eq!(tokens[16], Ok(Tokens::Ident("b"))); // b
        assert_eq!(tokens[17], Ok(Tokens::BlockEnd));
    }

    #[test]
    fn test_fn_keyword_not_ident() {
        let mut text = Tokens::lexer("fn");
        assert_eq!(text.next(), Some(Ok(Tokens::Fn)));
        assert_eq!(text.next(), None);
    }

    #[test]
    fn test_type_keywords() {
        let tokens: Vec<_> = Tokens::lexer("int float")
            .filter(|t| !matches!(t, Ok(Tokens::Whitespace)))
            .collect();
        assert_eq!(tokens[0], Ok(Tokens::IntType));
        assert_eq!(tokens[1], Ok(Tokens::FloatType));
    }

    // --- parens and blocks ---

    #[test]
    fn test_parens() {
        let tokens: Vec<_> = Tokens::lexer("(a)").collect();
        assert_eq!(tokens[0], Ok(Tokens::LParen));
        assert_eq!(tokens[1], Ok(Tokens::Ident("a")));
        assert_eq!(tokens[2], Ok(Tokens::RParen));
    }

    #[test]
    fn test_block_delimiters() {
        let tokens: Vec<_> = Tokens::lexer("{}").collect();
        assert_eq!(tokens[0], Ok(Tokens::BlockStart));
        assert_eq!(tokens[1], Ok(Tokens::BlockEnd));
    }

    // --- arrow ---

    #[test]
    fn test_arrow() {
        let mut text = Tokens::lexer("->");
        assert_eq!(text.next(), Some(Ok(Tokens::Arrow)));
        assert_eq!(text.next(), None);
    }

    // --- whitespace variants ---

    #[test]
    fn test_whitespace_tabs_newlines() {
        let tokens: Vec<_> = Tokens::lexer("a\t\nb").collect();
        assert_eq!(tokens[0], Ok(Tokens::Ident("a")));
        assert_eq!(tokens[1], Ok(Tokens::Whitespace));
        assert_eq!(tokens[2], Ok(Tokens::Ident("b")));
    }

    // --- fat arrow / single-line functions ---

    #[test]
    fn test_fat_arrow_token() {
        let mut text = Tokens::lexer("=>");
        assert_eq!(text.next(), Some(Ok(Tokens::FatArrow)));
        assert_eq!(text.next(), None);
    }

    #[test]
    fn test_fn_single_line_int() {
        // fn dobrar(x: int) => x * 2
        let src = "fn dobrar(x: int) => x * 2";
        let tokens: Vec<_> = Tokens::lexer(src)
            .filter(|t| !matches!(t, Ok(Tokens::Whitespace)))
            .collect();
        assert_eq!(tokens[0], Ok(Tokens::Fn));
        assert_eq!(tokens[1], Ok(Tokens::Ident("dobrar")));    // dobrar
        assert_eq!(tokens[2], Ok(Tokens::LParen));
        assert_eq!(tokens[3], Ok(Tokens::Ident("x")));    // x
        assert_eq!(tokens[4], Ok(Tokens::Colon));
        assert_eq!(tokens[5], Ok(Tokens::IntType));
        assert_eq!(tokens[6], Ok(Tokens::RParen));
        assert_eq!(tokens[7], Ok(Tokens::FatArrow));
        assert_eq!(tokens[8], Ok(Tokens::Ident("x")));    // x
        assert_eq!(tokens[9], Ok(Tokens::Multiply));
        assert_eq!(tokens[10], Ok(Tokens::Integer(2)));
    }

    #[test]
    fn test_fn_single_line_float() {
        // fn metade(x: float) => x / 2.0
        let src = "fn metade(x: float) => x / 2.0";
        let tokens: Vec<_> = Tokens::lexer(src)
            .filter(|t| !matches!(t, Ok(Tokens::Whitespace)))
            .collect();
        assert_eq!(tokens[0], Ok(Tokens::Fn));
        assert_eq!(tokens[1], Ok(Tokens::Ident("metade")));    // metade
        assert_eq!(tokens[2], Ok(Tokens::LParen));
        assert_eq!(tokens[3], Ok(Tokens::Ident("x")));    // x
        assert_eq!(tokens[4], Ok(Tokens::Colon));
        assert_eq!(tokens[5], Ok(Tokens::FloatType));
        assert_eq!(tokens[6], Ok(Tokens::RParen));
        assert_eq!(tokens[7], Ok(Tokens::FatArrow));
        assert_eq!(tokens[8], Ok(Tokens::Ident("x")));    // x
        assert_eq!(tokens[9], Ok(Tokens::Divide));
        assert_eq!(tokens[10], Ok(Tokens::Float(2.0)));
    }

    #[test]
    fn test_fn_single_line_two_params() {
        // fn soma(a: int, b: int) => a + b
        let src = "fn soma(a: int, b: int) => a + b";
        let tokens: Vec<_> = Tokens::lexer(src)
            .filter(|t| !matches!(t, Ok(Tokens::Whitespace)))
            .collect();
        assert_eq!(tokens[0], Ok(Tokens::Fn));
        assert_eq!(tokens[7], Ok(Tokens::Ident("b")));    // b
        assert_eq!(tokens[8], Ok(Tokens::Colon));
        assert_eq!(tokens[9], Ok(Tokens::IntType));
        assert_eq!(tokens[10], Ok(Tokens::RParen));
        assert_eq!(tokens[11], Ok(Tokens::FatArrow));
        assert_eq!(tokens[12], Ok(Tokens::Ident("a")));   // a
        assert_eq!(tokens[13], Ok(Tokens::Plus));
        assert_eq!(tokens[14], Ok(Tokens::Ident("b")));   // b
    }

    #[test]
    fn test_fat_arrow_not_confused_with_arrow() {
        let tokens: Vec<_> = Tokens::lexer("-> =>").collect();
        assert_eq!(tokens[0], Ok(Tokens::Arrow));
        assert_eq!(tokens[2], Ok(Tokens::FatArrow));
    }

    // --- negative integer ---

    #[test]
    fn test_negative_float() {
        let mut text = Tokens::lexer("-1.5");
        assert_eq!(text.next(), Some(Ok(Tokens::Float(-1.5))));
    }
}
