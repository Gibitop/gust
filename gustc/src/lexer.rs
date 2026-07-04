use crate::diagnostic::Diagnostic;
use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub lexeme: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Identifier(String),
    Number(String),
    StringLiteral(String),
    Keyword(Keyword),
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Colon,
    Comma,
    Dot,
    Slash,
    Plus,
    PlusPlus,
    Equal,
    EqualEqual,
    Bang,
    BangEqual,
    AndAnd,
    OrOr,
    FatArrow,
    LessEqual,
    GreaterEqual,
    Less,
    Greater,
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    Else,
    Enum,
    False,
    Fn,
    For,
    From,
    If,
    Import,
    In,
    Let,
    Match,
    Mut,
    Return,
    Struct,
    True,
}

pub struct Lexer<'source> {
    source: &'source str,
    position: usize,
    diagnostics: Vec<Diagnostic>,
}

impl<'source> Lexer<'source> {
    pub fn new(source: &'source str) -> Self {
        Self {
            source,
            position: 0,
            diagnostics: Vec::new(),
        }
    }

    pub fn tokenize(mut self) -> (Vec<Token>, Vec<Diagnostic>) {
        let mut tokens = Vec::new();

        while !self.is_at_end() {
            self.skip_whitespace_and_comments();

            if self.is_at_end() {
                break;
            }

            tokens.push(self.next_token());
        }

        tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::new(self.position, self.position),
            lexeme: String::new(),
        });

        (tokens, self.diagnostics)
    }

    fn next_token(&mut self) -> Token {
        let start = self.position;
        let character = self.bump().expect("lexer should not be at end");

        match character {
            '(' => self.single(TokenKind::LeftParen, start),
            ')' => self.single(TokenKind::RightParen, start),
            '{' => self.single(TokenKind::LeftBrace, start),
            '}' => self.single(TokenKind::RightBrace, start),
            '[' => self.single(TokenKind::LeftBracket, start),
            ']' => self.single(TokenKind::RightBracket, start),
            ':' => self.single(TokenKind::Colon, start),
            ',' => self.single(TokenKind::Comma, start),
            '.' => self.single(TokenKind::Dot, start),
            '/' => self.single(TokenKind::Slash, start),
            '<' => {
                if self.match_character('=') {
                    self.token(TokenKind::LessEqual, start, self.position)
                } else {
                    self.single(TokenKind::Less, start)
                }
            }
            '>' => {
                if self.match_character('=') {
                    self.token(TokenKind::GreaterEqual, start, self.position)
                } else {
                    self.single(TokenKind::Greater, start)
                }
            }
            '+' => {
                if self.match_character('+') {
                    self.token(TokenKind::PlusPlus, start, self.position)
                } else {
                    self.single(TokenKind::Plus, start)
                }
            }
            '=' => {
                if self.match_character('=') {
                    self.token(TokenKind::EqualEqual, start, self.position)
                } else if self.match_character('>') {
                    self.token(TokenKind::FatArrow, start, self.position)
                } else {
                    self.single(TokenKind::Equal, start)
                }
            }
            '!' => {
                if self.match_character('=') {
                    self.token(TokenKind::BangEqual, start, self.position)
                } else {
                    self.single(TokenKind::Bang, start)
                }
            }
            '&' => {
                if self.match_character('&') {
                    self.token(TokenKind::AndAnd, start, self.position)
                } else {
                    let span = Span::new(start, self.position);
                    self.diagnostics
                        .push(Diagnostic::error(span, "unexpected character `&`"));
                    self.token(TokenKind::Identifier(String::new()), start, self.position)
                }
            }
            '|' => {
                if self.match_character('|') {
                    self.token(TokenKind::OrOr, start, self.position)
                } else {
                    let span = Span::new(start, self.position);
                    self.diagnostics
                        .push(Diagnostic::error(span, "unexpected character `|`"));
                    self.token(TokenKind::Identifier(String::new()), start, self.position)
                }
            }
            '"' => self.string_literal(start),
            character if character.is_ascii_digit() => self.number(start),
            character if is_identifier_start(character) => self.identifier(start),
            _ => {
                let span = Span::new(start, self.position);
                self.diagnostics.push(Diagnostic::error(
                    span,
                    format!("unexpected character `{character}`"),
                ));
                self.token(TokenKind::Identifier(String::new()), start, self.position)
            }
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            while matches!(self.peek(), Some(character) if character.is_whitespace()) {
                self.bump();
            }

            if self.peek() == Some('/') && self.peek_next() == Some('/') {
                while !matches!(self.peek(), None | Some('\n')) {
                    self.bump();
                }
                continue;
            }

            break;
        }
    }

    fn string_literal(&mut self, start: usize) -> Token {
        let mut value = String::new();
        let mut escaped = false;

        while let Some(character) = self.bump() {
            if escaped {
                value.push(match character {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    '"' => '"',
                    '\\' => '\\',
                    other => other,
                });
                escaped = false;
                continue;
            }

            match character {
                '\\' => escaped = true,
                '"' => {
                    return self.token(TokenKind::StringLiteral(value), start, self.position);
                }
                other => value.push(other),
            }
        }

        let span = Span::new(start, self.position);
        self.diagnostics
            .push(Diagnostic::error(span, "unterminated string literal"));
        self.token(TokenKind::StringLiteral(value), start, self.position)
    }

    fn number(&mut self, start: usize) -> Token {
        while matches!(self.peek(), Some(character) if character.is_ascii_digit()) {
            self.bump();
        }

        let lexeme = self.source[start..self.position].to_string();

        Token {
            kind: TokenKind::Number(lexeme.clone()),
            span: Span::new(start, self.position),
            lexeme,
        }
    }

    fn identifier(&mut self, start: usize) -> Token {
        while matches!(self.peek(), Some(character) if is_identifier_continue(character)) {
            self.bump();
        }

        let lexeme = self.source[start..self.position].to_string();
        let kind = match lexeme.as_str() {
            "else" => TokenKind::Keyword(Keyword::Else),
            "enum" => TokenKind::Keyword(Keyword::Enum),
            "false" => TokenKind::Keyword(Keyword::False),
            "fn" => TokenKind::Keyword(Keyword::Fn),
            "for" => TokenKind::Keyword(Keyword::For),
            "from" => TokenKind::Keyword(Keyword::From),
            "if" => TokenKind::Keyword(Keyword::If),
            "import" => TokenKind::Keyword(Keyword::Import),
            "in" => TokenKind::Keyword(Keyword::In),
            "let" => TokenKind::Keyword(Keyword::Let),
            "match" => TokenKind::Keyword(Keyword::Match),
            "mut" => TokenKind::Keyword(Keyword::Mut),
            "return" => TokenKind::Keyword(Keyword::Return),
            "struct" => TokenKind::Keyword(Keyword::Struct),
            "true" => TokenKind::Keyword(Keyword::True),
            _ => TokenKind::Identifier(lexeme.clone()),
        };

        Token {
            kind,
            span: Span::new(start, self.position),
            lexeme,
        }
    }

    fn single(&self, kind: TokenKind, start: usize) -> Token {
        self.token(kind, start, self.position)
    }

    fn token(&self, kind: TokenKind, start: usize, end: usize) -> Token {
        Token {
            kind,
            span: Span::new(start, end),
            lexeme: self.source[start..end].to_string(),
        }
    }

    fn match_character(&mut self, expected: char) -> bool {
        if self.peek() != Some(expected) {
            return false;
        }

        self.bump();
        true
    }

    fn bump(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.position += character.len_utf8();
        Some(character)
    }

    fn peek(&self) -> Option<char> {
        self.source[self.position..].chars().next()
    }

    fn peek_next(&self) -> Option<char> {
        let mut characters = self.source[self.position..].chars();
        characters.next()?;
        characters.next()
    }

    fn is_at_end(&self) -> bool {
        self.position >= self.source.len()
    }
}

fn is_identifier_start(character: char) -> bool {
    character == '_' || character.is_ascii_alphabetic()
}

fn is_identifier_continue(character: char) -> bool {
    is_identifier_start(character) || character.is_ascii_digit()
}
