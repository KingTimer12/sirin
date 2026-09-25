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
    UnexpectedCharacter {
        char: char,
    },
    UnterminatedString,
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
        match char {
            '"' => LexingError::UnterminatedString,
            c if c.is_ascii() => LexingError::UnexpectedCharacter { char: c },
            c => LexingError::NonAsciiCharacter { char: c, byte: c as u8 },
        }
    }

    /// Short headline for the error, e.g. for a diagnostic title.
    pub fn title(&self) -> &'static str {
        match self {
            LexingError::InvalidInteger { .. } => "invalid integer literal",
            LexingError::InvalidFloat { .. } => "invalid float literal",
            LexingError::NonAsciiCharacter { .. } => "non-ASCII character outside a string",
            LexingError::UnexpectedCharacter { .. } => "unexpected character",
            LexingError::UnterminatedString => "unterminated string literal",
            LexingError::Other => "invalid token",
        }
    }

    /// Suggestion on how to fix the error, when there is an obvious one.
    pub fn help(&self) -> Option<&'static str> {
        match self {
            LexingError::InvalidInteger { .. } => Some(
                "integer literals must fit in a signed 64-bit integer \
                 (-9223372036854775808 to 9223372036854775807)",
            ),
            LexingError::NonAsciiCharacter { .. } => Some(
                "identifiers and operators must be ASCII; non-ASCII text is only \
                 allowed inside string literals and comments",
            ),
            LexingError::UnterminatedString => Some("add a closing `\"` to end the string"),
            _ => None,
        }
    }
}

impl std::fmt::Display for LexingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LexingError::InvalidInteger { value, reason } if value.is_empty() => {
                write!(f, "invalid integer literal: {reason}")
            }
            LexingError::InvalidInteger { value, reason } => {
                write!(f, "`{value}` is not a valid integer: {reason}")
            }
            LexingError::InvalidFloat { value } if value.is_empty() => {
                write!(f, "invalid float literal")
            }
            LexingError::InvalidFloat { value } => write!(f, "`{value}` is not a valid float"),
            LexingError::NonAsciiCharacter { char, .. } => {
                write!(f, "`{char}` (U+{:04X}) cannot appear here", *char as u32)
            }
            LexingError::UnexpectedCharacter { char } if char.is_ascii_control() => {
                write!(f, "control character U+{:04X} is not allowed here", *char as u32)
            }
            LexingError::UnexpectedCharacter { char } => {
                write!(f, "`{char}` is not a valid token in Sirin")
            }
            LexingError::UnterminatedString => write!(f, "this string is never closed"),
            LexingError::Other => write!(f, "this text could not be read as a token"),
        }
    }
}