use std::num::ParseIntError;

use crate::token::Tokens;

#[derive(Default, Debug, Clone, PartialEq)]
pub enum LexingError {
    InvalidInteger {
        value: String,
        reason: String,
    },
    InvalidFloat {
        value: String,
    },
    NonAsciiCharacter {
        char: char,
        byte: u8,
    },
    #[default]
    Other,
}

impl From<ParseIntError> for LexingError {
    fn from(err: ParseIntError) -> Self {
        use std::num::IntErrorKind::*;
        let reason = match err.kind() {
            PosOverflow => "value too large for i64".to_owned(),
            NegOverflow => "value too small for i64".to_owned(),
            Empty => "empty integer literal".to_owned(),
            InvalidDigit => "invalid digit in integer literal".to_owned(),
            _ => format!("{err}"),
        };
        LexingError::InvalidInteger {
            value: String::new(),
            reason,
        }
    }
}

impl From<std::num::ParseFloatError> for LexingError {
    fn from(_: std::num::ParseFloatError) -> Self {
        LexingError::InvalidFloat {
            value: String::new(),
        }
    }
}

impl LexingError {
    pub fn from_lexer<'a>(lex: &mut logos::Lexer<'a, Tokens<'a>>) -> Self {
        let slice = lex.slice();
        let char = slice.chars().next().unwrap_or('\0');
        let byte = char as u8;
        LexingError::NonAsciiCharacter { char, byte }
    }
}