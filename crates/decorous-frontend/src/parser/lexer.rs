use bitflags::bitflags;
use harpoon::{Harpoon, Span};

use crate::location::Location;

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct Allowed: u32 {
        #[allow(clippy::unreadable_literal)]
        const COLON    = 0b00000001;
        #[allow(clippy::unreadable_literal)]
        const LBRACKET = 0b00000010;
        #[allow(clippy::unreadable_literal)]
        const RBRACKET = 0b00000100;
        #[allow(clippy::unreadable_literal)]
        const EQUALS   = 0b00001000;
        #[allow(clippy::unreadable_literal)]
        const AT       = 0b00010000;
        #[allow(clippy::unreadable_literal)]
        const QUOTES   = 0b00100000;
        #[allow(clippy::unreadable_literal)]
        const IN       = 0b01000000;
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
    SpecialBlockStart(&'src str),
    SpecialBlockEnd(&'src str),
    SpecialExtender(&'src str),
    Mustache(&'src str),
    Comment(&'src str),
    CodeBlockIndicator,

    Rbrace,
    Quotes(&'src str),
    Ident(&'src str),
    Lbracket,
    Rbracket,
    Colon,
    Equals,
    At,
    In,

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
            Some('/') if self.harpoon.peek_equals("//") => self.consume_comment(),
            Some('/') => return self.consume_elem_end(),
            Some('{') if self.harpoon.peek_equals("{#") => self.consume_special_block_start(),
            Some('{') if self.harpoon.peek_equals("{/") => self.consume_special_block_end(),
            Some('{') if self.harpoon.peek_equals("{:") => self.consume_special_extender(),
            Some('{') => return self.consume_mustache(),
            Some('-') if self.harpoon.peek_equals("---") => {
                self.harpoon.consume_n(2);
                token1!(CodeBlockIndicator, self.harpoon.offset())
            }
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

    pub fn text_until_str(&mut self, until: &str) -> &'src str {
        let first = until.chars().next().expect("`until` be length one or more");
        let span = self.harpoon.harpoon(|h| loop {
            h.consume_while(|c| c != first);
            if h.try_consume(until) || h.peek().is_none() {
                break;
            }
            h.consume();
        });

        span.text().strip_suffix(until).unwrap_or(span.text())
    }

    pub fn text_until(&mut self, until: char) -> &'src str {
        let span = self.harpoon.harpoon(|h| h.consume_while(|c| c != until));
        // Consume the `until` char
        self.harpoon.consume();

        span.text()
    }

    pub fn allow(&mut self, allow: Allowed) {
        self.allowed |= allow;
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
            Some('i') if self.harpoon.peek_equals("in") && self.allowed.intersects(Allowed::IN) => {
                self.harpoon.consume();
                token1!(In, self.harpoon.offset())
            }
            Some(c) if c.is_alphabetic() => return self.consume_ident(),
            Some('{') => return self.consume_mustache(),
            Some('"') => return self.consume_quotes(),
            Some('=') => token1!(Equals, self.harpoon.offset()),
            Some(':') => token1!(Colon, self.harpoon.offset()),
            Some('@') => token1!(At, self.harpoon.offset()),
            Some(']') => token1!(Rbracket, self.harpoon.offset()),
            Some('}') => token1!(Rbrace, self.harpoon.offset()),
            Some('-') if self.harpoon.peek_equals("---") => {
                self.harpoon.consume_n(2);
                token1!(CodeBlockIndicator, self.harpoon.offset())
            }

            Some(c) => Token {
                kind: TokenKind::Invalid(c),
                loc: Location::new(self.harpoon.offset(), 1),
            },
            None => Token {
                kind: TokenKind::Eof,
                loc: Location::new(self.harpoon.offset(), 0),
            },
        };

        self.harpoon.consume();

        tok
    }

    fn consume_text(&mut self) -> Token<'src> {
        let span = self.harpoon.harpoon(|h| loop {
            h.consume_while(|c| !matches!(c, '{' | '#' | '/' | '-' | '\\'));
            // Escape
            if h.peek().is_some_and(|c| c == '\\') {
                h.consume();
                h.consume();
            }
            if h.peek().is_none()
                || matches!(h.peek().unwrap(), '{' | '#' | '/')
                || h.peek_equals("---")
            {
                break;
            }
        });

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

        let mut unclosed = false;
        let contents = self.harpoon.harpoon(|h| {
            let mut rbraces_needed = 1;
            while rbraces_needed > 0 {
                h.consume();
                match h.peek() {
                    Some('{') => {
                        rbraces_needed += 1;
                    }
                    Some('}') => rbraces_needed -= 1,
                    Some(_) => {}
                    None => {
                        unclosed = true;
                        return;
                    }
                }
            }
        });
        self.harpoon.consume();

        if unclosed {
            return Token {
                kind: TokenKind::Text(contents.text()),
                loc: span_to_loc(contents),
            };
        }

        Token {
            kind: TokenKind::Mustache(contents.text()),
            loc: span_to_loc_with_enclosing(contents),
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
            loc: span_to_loc_with_enclosing(contents),
        }
    }

    fn consume_special_block_start(&mut self) -> Token<'src> {
        debug_assert_eq!(Some('{'), self.harpoon.consume());
        debug_assert_eq!(Some('#'), self.harpoon.consume());

        let name = self.harpoon.harpoon(|h| h.consume_while(is_html_ident));

        Token {
            kind: TokenKind::SpecialBlockStart(name.text()),
            loc: span_to_loc(name),
        }
    }

    fn consume_special_block_end(&mut self) -> Token<'src> {
        debug_assert_eq!(Some('{'), self.harpoon.consume());
        debug_assert_eq!(Some('/'), self.harpoon.consume());

        let name = self.harpoon.harpoon(|h| h.consume_while(is_html_ident));

        Token {
            kind: TokenKind::SpecialBlockEnd(name.text()),
            loc: span_to_loc(name),
        }
    }

    fn consume_special_extender(&mut self) -> Token<'src> {
        debug_assert_eq!(Some('{'), self.harpoon.consume());
        debug_assert_eq!(Some(':'), self.harpoon.consume());

        let name = self.harpoon.harpoon(|h| h.consume_while(is_html_ident));

        Token {
            kind: TokenKind::SpecialExtender(name.text()),
            loc: span_to_loc(name),
        }
    }

    fn consume_comment(&mut self) -> Token<'src> {
        debug_assert_eq!(Some('/'), self.harpoon.consume());
        debug_assert_eq!(Some('/'), self.harpoon.consume());

        let comment = self.harpoon.harpoon(|h| h.consume_while(|c| c != '\n'));

        Token {
            kind: TokenKind::Comment(comment.text()),
            loc: span_to_loc(comment),
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

fn span_to_loc_with_enclosing(span: Span) -> Location {
    Location::new(span.start() - 1, span.len() + 2)
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
            TokenKind::In => "the in keyword",
            TokenKind::Quotes(_) => "quoted text",
            TokenKind::Ident(_) => "an identifier",
            TokenKind::Lbracket => "a left bracket",
            TokenKind::Rbracket => "a right bracket",
            TokenKind::Colon => "a colon",
            TokenKind::Equals => "an equals sign",
            TokenKind::At => "an at symbol",
            TokenKind::SpecialBlockStart(_) => "the start of a special block",
            TokenKind::SpecialExtender(_) => "a special block extender",
            TokenKind::SpecialBlockEnd(_) => "the end of a special block",
            TokenKind::Rbrace => "an rbrace",
            TokenKind::Comment(_) => "a comment",
            TokenKind::CodeBlockIndicator => "a code block indicator",
            TokenKind::Invalid(_) => "INVALID",
            TokenKind::Eof => "eof",
        }
    }
}
