#[derive(Debug)]
pub enum TokenKind {
    Number(i64),
    Plus,
    Menus,
    Asterisk,
    Slash,
    LeftParen,
    RightParen,
    Eof,
    Bad
}

#[derive(Debug)]
pub struct TextSpan {
    start: usize,
    end: usize,
    literal: String,
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

#[derive(Debug)]
pub struct Token {
    kind: TokenKind,
    span: TextSpan,
}

impl Token {
    pub fn new(kind: TokenKind, span: TextSpan) -> Self {
        Self { kind, span } // return Self
    }
}

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

        let start = self.cur_pos;
        let c = self.current_char();
        let mut kind = TokenKind::Bad;
        if Self::is_number_start(&c) {
            let number: i64 = self.consume_number();
            kind = TokenKind::Number(number)
        }

        let end = self.cur_pos;
        let literal = self.input[start..end].to_string();
        let span = TextSpan::new(start, end, literal);
        Some(Token::new(kind, span)) // return Token
    }

    fn is_number_start(c: &char) -> bool {
        c.is_digit(10) // return bool
    }

    fn current_char(&self) -> char {
        self.input.chars().nth(self.cur_pos).unwrap() // return char
    }

    fn consume(&mut self) -> Option<char> {
        if self.cur_pos >= self.input.len() {
            return None
        }
        let c = self.current_char();
        self.cur_pos += 1;
        Some(c) // return Option<char>
    }

    fn consume_number(&mut self) -> i64 {
        let mut number: i64 = 0;
        while let Some(c) = self.consume() {
            if Self::is_number_start(&c) {
                number = number * 10 + c.to_digit(10).unwrap() as i64;
            } else {
                break;
            }
        }
        number // return i64
    }
}
