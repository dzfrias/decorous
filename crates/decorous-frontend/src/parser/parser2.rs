#![allow(dead_code)]

use rslint_parser::SyntaxNode;

use crate::{
    ast::{
        Attribute, AttributeValue, DecorousAst, Element, EventHandler, Mustache, Node, NodeType,
        Text,
    },
    errors::{ParseError, ParseErrorType},
    lexer::{Allowed, Lexer, Token, TokenKind},
    location::Location,
};

type Result<T> = std::result::Result<T, ParseError<Location>>;

#[derive(Debug)]
pub struct Parser<'src> {
    lexer: Lexer<'src>,
    current_token: Token<'src>,
}

macro_rules! expect {
    ($self:expr, $kind:ident(_)) => {{
        $self.next_token();
        let TokenKind::$kind(binding) = $self.current_token.kind else {
            todo!("error");
        };
        Ok(binding)
    }};
    ($self:expr, $kind:ident) => {{
        $self.next_token();
        let TokenKind::$kind = $self.current_token.kind else {
            todo!("error");
        };
        Ok(())
    }};
}

impl<'src> Parser<'src> {
    pub fn new(lexer: Lexer<'src>) -> Self {
        let mut parser = Self {
            lexer,
            current_token: Token {
                kind: TokenKind::Eof,
                loc: Location::default(),
            },
        };

        parser.next_token();
        parser
    }

    pub fn parse(mut self) -> Result<DecorousAst<'src>> {
        let mut nodes = vec![];
        while self.current_token.kind != TokenKind::Eof {
            let node = self.parse_node()?;
            nodes.push(node);
        }

        Ok(DecorousAst::new(nodes, None, None, None, None))
    }

    fn next_token(&mut self) {
        self.current_token = self.lexer.next_token();
    }

    fn next_token_allow(&mut self, allow: Allowed) {
        self.current_token = self.lexer.next_token_allow(allow);
    }

    fn expect_next_token(&mut self, expected: TokenKind) -> Result<()> {
        self.next_token();
        if self.current_token.kind != expected {
            todo!("error");
        }
        Ok(())
    }

    fn current_offset(&self) -> usize {
        self.current_token.loc.offset()
    }

    fn parse_node(&mut self) -> Result<Node<'src, Location>> {
        let begin_loc = self.current_offset();
        let ty = match self.current_token.kind {
            TokenKind::ElemBegin(_) => NodeType::Element(self.parse_elem()?),
            TokenKind::Mustache(_) => NodeType::Mustache(self.parse_mustache()?),
            TokenKind::Text(t) => NodeType::Text(Text(t)),

            t => todo!("error, got: {t:?}"),
        };
        self.next_token();

        Ok(Node::new(
            ty,
            Location::new(begin_loc, self.current_offset() - begin_loc),
        ))
    }

    fn parse_elem(&mut self) -> Result<Element<'src, Location>> {
        let TokenKind::ElemBegin(tag_name) = self.current_token.kind else {
            panic!("should be called with ElemBegin");
        };

        let attrs = if self.lexer.peek_token_allow(Allowed::LBRACKET).kind == TokenKind::Lbracket {
            self.next_token_allow(Allowed::LBRACKET);
            self.parse_attrs()?
        } else {
            vec![]
        };

        self.next_token_allow(Allowed::COLON);
        if self.current_token.kind == TokenKind::Colon {
            // Plus one because of the colon token
            let loc = self.current_offset() + 1;
            let text = expect!(self, Text(_))?;
            return Ok(Element::new(
                tag_name,
                attrs,
                vec![Node::new(
                    NodeType::Text(Text(text)),
                    Location::new(loc, text.len()),
                )],
            ));
        }

        let mut children = vec![];
        loop {
            let node = self.parse_node()?;
            children.push(node);

            // Try to close the tag
            if let TokenKind::ElemEnd(end_name) = self.current_token.kind {
                if end_name == tag_name {
                    break;
                } else {
                    return Err(ParseError::new(
                        Location::new(self.current_offset(), self.current_token.loc.length()),
                        ParseErrorType::InvalidClosingTag(tag_name.to_owned()),
                        None,
                    ));
                }
            }
        }

        Ok(Element::new(tag_name, attrs, children))
    }

    fn parse_mustache(&mut self) -> Result<Mustache> {
        let TokenKind::Mustache(js_text) = self.current_token.kind else {
            panic!("should be called with Mustache");
        };

        self.js_expr(js_text).map(Mustache)
    }

    fn js_expr(&self, js_text: &str) -> Result<SyntaxNode> {
        let parse = rslint_parser::parse_expr(js_text, 0);
        if parse.errors().is_empty() {
            Ok(parse.syntax())
        } else {
            let error = parse.ok().expect_err("should have errors").swap_remove(0);
            let range = error.primary.unwrap().span.range;
            Err(ParseError::new(
                Location::new(self.current_offset() + range.start, range.len()),
                ParseErrorType::JavaScriptDiagnostics { title: error.title },
                None,
            ))
        }
    }

    fn parse_attrs(&mut self) -> Result<Vec<Attribute<'src>>> {
        assert_eq!(TokenKind::Lbracket, self.current_token.kind);

        self.lexer.attrs_mode(true);
        let mut attrs = vec![];
        self.next_token();
        while self.current_token.kind != TokenKind::Rbracket {
            let attr = self.parse_attr()?;
            attrs.push(attr);
            self.next_token();
        }
        self.lexer.attrs_mode(false);

        Ok(attrs)
    }

    fn parse_attr(&mut self) -> Result<Attribute<'src>> {
        match self.current_token.kind {
            TokenKind::At => self.parse_event_handler(),
            TokenKind::Ident(_) => self.parse_generic_attr(),
            TokenKind::Colon => self.parse_binding(),
            a => todo!("error, {a:?}"),
        }
    }

    fn parse_event_handler(&mut self) -> Result<Attribute<'src>> {
        assert_eq!(TokenKind::At, self.current_token.kind);

        let event = expect!(self, Ident(_))?;
        expect!(self, Equals)?;
        let expr_text = expect!(self, Mustache(_))?;

        Ok(Attribute::EventHandler(EventHandler::new(
            event,
            self.js_expr(expr_text)?,
        )))
    }

    fn parse_generic_attr(&mut self) -> Result<Attribute<'src>> {
        let TokenKind::Ident(key) = self.current_token.kind else {
            panic!("should be called with Ident");
        };

        if self.lexer.peek_token().kind != TokenKind::Equals {
            return Ok(Attribute::KeyValue(key, None));
        }

        // Equals
        self.next_token();
        self.next_token();

        let attr = match self.current_token.kind {
            TokenKind::Quotes(quotes) => {
                Attribute::KeyValue(key, Some(AttributeValue::Literal(quotes.into())))
            }
            TokenKind::Mustache(mustache) => Attribute::KeyValue(
                key,
                Some(AttributeValue::JavaScript(self.js_expr(mustache)?)),
            ),
            a => todo!("error, {a:?}"),
        };

        Ok(attr)
    }

    fn parse_binding(&mut self) -> Result<Attribute<'src>> {
        assert_eq!(TokenKind::Colon, self.current_token.kind);

        let bind = expect!(self, Ident(_))?;
        expect!(self, Colon)?;

        Ok(Attribute::Binding(bind))
    }
}
