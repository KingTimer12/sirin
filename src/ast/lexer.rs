use std::fmt::Display;

#[derive(Debug, PartialEq, Clone)]
pub enum TokenKind {
    Number(i64),
    Plus,
    Minus,
    Asterisk,
    Slash,
    LeftParen,
    RightParen,
    Eof,
    Bad,
    Whitespace,
    Let,
    Id,
    Equals
}

/* DISPLAY */

impl Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenKind::Number(_) => write!(f, "Number"),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Asterisk => write!(f, "*"),
            TokenKind::Slash => write!(f, "/"),
            TokenKind::LeftParen => write!(f, "("),
            TokenKind::RightParen => write!(f, ")"),
            TokenKind::Bad => write!(f, "Bad"),
            TokenKind::Whitespace => write!(f, "Whitespace"),
            TokenKind::Eof => write!(f, "Eof"),
            TokenKind::Let => write!(f, "Let"),
            TokenKind::Id => write!(f, "Identifier"),
            TokenKind::Equals => write!(f, "="),
        }
    }
}

/* SECTION - TEXT SPAN */

#[derive(Debug, PartialEq, Clone)]
pub struct TextSpan {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) literal: String,
}

impl TextSpan {
    pub fn new(start: usize, end: usize, literal: String) -> Self {
        Self {
            start,
            end,
            literal,
        } // return Self
    }

    
    pub fn length(&self) -> usize {
        self.end - self.start // return usize
    }
    
}

/* TEXT SPAN */

/* SECTION - TOKEN */

#[derive(Debug, PartialEq, Clone)]
pub struct Token {
    pub(crate) kind: TokenKind, // Type of token (Ex: Whitespace, Plus, Minus, etc)
    pub(crate) span: TextSpan, // Informations about token
}

impl Token {
    pub fn new(kind: TokenKind, span: TextSpan) -> Self {
        Self { kind, span } // return Self
    }
}

/* TOKEN */

/* SECTION - LEXER */

pub struct Lexer<'a> {
    input: &'a str,
    cur_pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input, cur_pos: 0 } // return Self
    }

    pub fn next_token(&mut self) -> Option<Token> {
        if self.cur_pos > self.input.len() {return None;}
        if self.cur_pos == self.input.len() {
            let eof_char: char = '\0';
            self.cur_pos += 1;
            return Some(Token::new(TokenKind::Eof, TextSpan::new(0, 0, eof_char.to_string())))
        }

        let c = self.current_char();
        c.map(|c| {
            let start = self.cur_pos;
            let mut kind = TokenKind::Bad;
            if Self::is_number_start(&c) {
                let number: i64 = self.consume_number();
                kind = TokenKind::Number(number)
            } else if Self::is_whitespace(&c) {
                self.consume();
                kind = TokenKind::Whitespace
            } else if Self::is_identifier_start(&c) {
                let identifier = self.consume_identifier();
                kind = match identifier.as_str() {
                    "let" => TokenKind::Let,
                    _ => TokenKind::Id
                }
            } else {
                kind = self.consume_punctuation();
            }

            let end = self.cur_pos;
            let literal = self.input[start..end].to_string();
            let span = TextSpan::new(start, end, literal);
            Token::new(kind, span) // return Token
        }) // return Option<Token>
    }

    fn is_whitespace(c: &char) -> bool {
        c.is_whitespace() // return bool
    }

    fn is_number_start(c: &char) -> bool {
        c.is_digit(10) // return bool
    }

    fn is_identifier_start(c: &char) -> bool {
        c.is_alphabetic()
    }

    fn current_char(&self) -> Option<char> {
        self.input.chars().nth(self.cur_pos) // return Option<char>
    }

    fn consume(&mut self) -> Option<char> {
        if self.cur_pos >= self.input.len() {
            return None
        }
        let c = self.current_char();
        self.cur_pos += 1;
        c // return Option<char>
    }

    fn consume_number(&mut self) -> i64 {
        let mut number: i64 = 0;
        while let Some(c) = self.current_char() {
            if Self::is_number_start(&c) {
                self.consume().unwrap();
                number = number * 10 + c.to_digit(10).unwrap() as i64;
            } else {
                break;
            }
        }
        number // return i64
    }

    fn consume_punctuation(&mut self) -> TokenKind {
        let c = self.consume().unwrap();
        match c {
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Asterisk,
            '/' => TokenKind::Slash,
            '(' => TokenKind::LeftParen,
            ')' => TokenKind::RightParen,
            '=' => TokenKind::Equals,
            _ => TokenKind::Bad
        }
    }

    fn consume_identifier(&mut self) -> String {
        let mut identifier = String::new();
        while let Some(c) = self.current_char() {
            if !Self::is_identifier_start(&c) {
                break;
            }
            self.consume().unwrap();
            identifier.push(c);
        }
        identifier
    }
}

/* LEXER */
