#![allow(dead_code)]

use bitflags::bitflags;
use harpoon::{Harpoon, Span};

use crate::location::Location;

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct Allowed: u32 {
        const COLON    = 0b00000001;
        const LBRACKET = 0b00000010;
        const RBRACKET = 0b00000100;
        const EQUALS   = 0b00001000;
        const AT       = 0b00010000;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Token<'src> {
    pub kind: TokenKind<'src>,
    pub loc: Location,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TokenKind<'src> {
    Text(&'src str),
    ElemBegin(&'src str),
    ElemEnd(&'src str),
    Mustache(&'src str),
    Quotes(&'src str),
    Ident(&'src str),

    Lbracket,
    Rbracket,
    Colon,
    Equals,
    At,

    Invalid(char),
    Eof,
}

macro_rules! token1 {
    ($kind:ident, $offset:expr) => {{
        Token {
            kind: TokenKind::$kind,
            loc: Location::new($offset, 1),
        }
    }};
}

#[derive(Debug)]
pub struct Lexer<'src> {
    allowed: Allowed,
    attrs_mode: bool,
    harpoon: Harpoon<'src>,
}

impl<'src> Lexer<'src> {
    pub fn new(src: &'src str) -> Self {
        Self {
            harpoon: Harpoon::new(src),
            allowed: Allowed::empty(),
            attrs_mode: false,
        }
    }

    pub fn next_token(&mut self) -> Token<'src> {
        if self.attrs_mode {
            return self.next_token_attrs();
        }

        let tok = match self.harpoon.peek() {
            Some('#') => return self.consume_elem(),
            Some('/') => return self.consume_elem_end(),
            Some('{') => return self.consume_mustache(),
            Some(':') if self.allowed.intersects(Allowed::COLON) => {
                token1!(Colon, self.harpoon.offset())
            }
            Some('[') if self.allowed.intersects(Allowed::LBRACKET) => {
                token1!(Lbracket, self.harpoon.offset())
            }

            Some(_) => return self.consume_text(),
            None => token1!(Eof, self.harpoon.offset()),
        };

        self.harpoon.consume();

        tok
    }

    pub fn peek_token(&mut self) -> Token<'src> {
        let harpoon = self.harpoon.clone();
        let tok = self.next_token();
        self.harpoon = harpoon;
        tok
    }

    pub fn peek_token_allow(&mut self, allow: Allowed) -> Token<'src> {
        self.allowed |= allow;
        let tok = self.peek_token();
        self.allowed = Allowed::empty();
        tok
    }

    pub fn next_token_allow(&mut self, allow: Allowed) -> Token<'src> {
        self.allowed |= allow;
        let tok = self.next_token();
        self.allowed = Allowed::empty();
        tok
    }

    pub fn attrs_mode(&mut self, attrs_mode: bool) {
        self.attrs_mode = attrs_mode;
    }

    fn next_token_attrs(&mut self) -> Token<'src> {
        self.harpoon.consume_while(|c| c.is_whitespace());

        let tok = match self.harpoon.peek() {
            Some(c) if c.is_alphabetic() => return self.consume_ident(),
            Some('{') => return self.consume_mustache(),
            Some('"') => return self.consume_quotes(),
            Some('=') => token1!(Equals, self.harpoon.offset()),
            Some(':') => token1!(Colon, self.harpoon.offset()),
            Some('@') => token1!(At, self.harpoon.offset()),
            Some(']') => token1!(Rbracket, self.harpoon.offset()),

            Some(c) => Token {
                kind: TokenKind::Invalid(c),
                loc: Location::new(self.harpoon.offset(), 1),
            },
            None => token1!(Eof, self.harpoon.offset()),
        };

        self.harpoon.consume();

        tok
    }

    fn consume_text(&mut self) -> Token<'src> {
        let span = self
            .harpoon
            .harpoon(|h| h.consume_while(|c| !matches!(c, '{' | '#' | '/')));

        Token {
            kind: TokenKind::Text(span.text()),
            loc: span_to_loc(span),
        }
    }

    fn consume_elem(&mut self) -> Token<'src> {
        debug_assert_eq!(Some('#'), self.harpoon.consume());

        let elem = self.harpoon.harpoon(|h| h.consume_while(is_html_ident));

        Token {
            kind: TokenKind::ElemBegin(elem.text()),
            loc: span_to_loc(elem),
        }
    }

    fn consume_elem_end(&mut self) -> Token<'src> {
        debug_assert_eq!(Some('/'), self.harpoon.consume());

        let elem = self.harpoon.harpoon(|h| h.consume_while(is_html_ident));

        Token {
            kind: TokenKind::ElemEnd(elem.text()),
            loc: span_to_loc(elem),
        }
    }

    fn consume_mustache(&mut self) -> Token<'src> {
        debug_assert_eq!(Some('{'), self.harpoon.consume());

        // FIX: Balance { in JS
        let contents = self.harpoon.harpoon(|h| h.consume_while(|c| c != '}'));
        self.harpoon.consume();

        Token {
            kind: TokenKind::Mustache(contents.text()),
            loc: span_to_loc(contents),
        }
    }

    fn consume_ident(&mut self) -> Token<'src> {
        let ident = self.harpoon.harpoon(|h| h.consume_while(is_html_ident));

        Token {
            kind: TokenKind::Ident(ident.text()),
            loc: span_to_loc(ident),
        }
    }

    fn consume_quotes(&mut self) -> Token<'src> {
        debug_assert_eq!(Some('"'), self.harpoon.consume());

        // FIX: Allow escaped quotes
        let contents = self.harpoon.harpoon(|h| h.consume_while(|c| c != '"'));
        self.harpoon.consume();

        Token {
            kind: TokenKind::Quotes(contents.text()),
            loc: span_to_loc(contents),
        }
    }
}

impl<'src> Iterator for Lexer<'src> {
    type Item = Token<'src>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_token() {
            t if t.kind == TokenKind::Eof => None,
            t => Some(t),
        }
    }
}

fn span_to_loc(span: Span) -> Location {
    Location::new(span.start(), span.len())
}

fn is_html_ident(c: char) -> bool {
    c.is_digit(36) || matches!(c, '-' | '_')
}

impl TokenKind<'_> {
    pub fn display_kind(&self) -> &'static str {
        match self {
            TokenKind::Text(_) => "text",
            TokenKind::ElemBegin(_) => "an element beginning",
            TokenKind::ElemEnd(_) => "an element ending",
            TokenKind::Mustache(_) => "a JavaScript expression",
            TokenKind::Quotes(_) => "quoted text",
            TokenKind::Ident(_) => "an identifier",
            TokenKind::Lbracket => "a left bracket",
            TokenKind::Rbracket => "a right bracket",
            TokenKind::Colon => "a colon",
            TokenKind::Equals => "an equals sign",
            TokenKind::At => "an at symbol",
            TokenKind::Invalid(_) => "INVALID",
            TokenKind::Eof => "eof",
        }
    }
}
