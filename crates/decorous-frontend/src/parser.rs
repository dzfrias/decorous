use lightningcss::stylesheet::StyleSheet;
use rslint_parser::SyntaxNode;
use std::{collections::VecDeque, str::Chars};
use thiserror::Error;

use crate::ast::{
    Attribute, AttributeValue, DecorousAst, Element, EventHandler, ForBlock, IfBlock, Location,
    Node, NodeType, SpecialBlock,
};

#[derive(Debug)]
pub struct Parser<'a> {
    // Source
    source: &'a str,
    input: Chars<'a>,
    // Not using `Peekable` because access to `as_str()` of `input` is crucial.
    peek_buffer: Option<char>,

    // A queue we maintain of the closed tags we find. This allows us to find invalid closing tag
    // errors
    close_tag_queue: VecDeque<&'a str>,

    // Current character index
    index: usize,
    // Current character index, with proper utf8 length taken into account. We need this because
    // we take a slice of `source`, which operates in bytes, not code points
    slice_index: usize,

    errors: Vec<ParseError<'a>>,
}

#[derive(Debug, Error, PartialEq)]
pub enum ParseError<'a> {
    #[error("invalid closing tag, expected {0}")]
    InvalidClosingTag(String, Location),
    #[error("invalid character, expected {0}, got {1}")]
    ExpectedCharacter(char, char, Location),
    #[error("invalid character, expected {0:?}, got {1}")]
    ExpectedCharacterAny(Vec<char>, char, Location),
    #[error("expected end of comment")]
    ExpectedEndOfComment(Location),
    #[error("invalid closing block, expected {0}, got {1}")]
    InvalidClosingBlock(String, String, Location),
    #[error("expected string {0}, got {1}")]
    ExpectedString(String, String, Location),
    #[error("unrecognized special block: \"{0}\"")]
    UnrecognizedSpecialBlock(String, Location),
    #[error("cannot have non-toplevel script")]
    CannotHaveNonTopLevelScript(Location),
    #[error("cannot have two scripts")]
    CannotHaveTwoScripts(Location),
    #[error("error parsing css: {0}")]
    CssParseError(lightningcss::error::ParserError<'a>),
    #[error("cannot have two style tags")]
    CannotHaveTwoStyleTags(Location),
    #[error("cannot have non-toplevel style tag")]
    CannotHaveNonTopLevelStyle(Location),
}

#[derive(Debug)]
enum CallingContext<'a> {
    Toplevel {
        script: Option<SyntaxNode>,
        css: Option<StyleSheet<'a, 'a>>,
    },
}

#[derive(Debug)]
enum ElementResult<'a> {
    Script(SyntaxNode),
    Node(Node<'a>),
    Css(StyleSheet<'a, 'a>),
}

/// Zero-copy parser that takes in decorous HTML syntax.
impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            source: input,
            input: input.chars(),
            peek_buffer: None,

            close_tag_queue: VecDeque::new(),

            index: 0,
            slice_index: 0,

            errors: vec![],
        }
    }

    /// Parse the received input into a collection of nodes. Also, return the errors found while
    /// parsing.
    pub fn parse(mut self) -> (DecorousAst<'a>, Vec<ParseError<'a>>) {
        let mut ctx = CallingContext::Toplevel {
            script: None,
            css: None,
        };
        let nodes = self.parse_nodes(Some(&mut ctx));
        let (script, css) = match ctx {
            CallingContext::Toplevel { script, css } => (script, css),
        };
        let ast = DecorousAst::new(nodes, script, css);

        (ast, self.errors)
    }

    fn parse_nodes(&mut self, mut ctx: Option<&mut CallingContext<'a>>) -> Vec<Node<'a>> {
        let mut nodes = Vec::new();

        while self.peek().is_some() {
            // We want to parse '\n', so keep it.
            self.consume_while(|c| c.is_whitespace() && c != '\n');

            match self.peek() {
                Some('<') => {
                    // Should be the '<' we just peeked
                    self.consume();

                    let peek = self.peek();
                    if peek.is_some_and(|c| c == '/') {
                        // Should be the '/' we just peeked.
                        self.consume();
                        self.skip_whitespace();

                        let close = self.parse_tag();

                        self.skip_whitespace();
                        self.expect_consume('>');

                        // Add to the closing tag queue
                        self.close_tag_queue.push_back(close);
                        break;
                    } else if peek.is_some_and(|c| c == '!') {
                        let start = self.slice_index;
                        let start_idx = self.index;
                        if let Some(comment) = self.try_parse_comment() {
                            nodes.push(comment);
                        } else {
                            // start - 1 is safe because we know we've consumed a < up to this
                            // point
                            let text = &self.source[(start - 1)
                                ..self.source.len() - self.source[self.slice_index..].len()];
                            nodes.push(Node::new(
                                NodeType::Text(text),
                                Location::new(start_idx - 1, self.index.saturating_sub(1)),
                            ));
                        }
                    } else {
                        let start = self.index;
                        let node = self.parse_element();

                        let node = match node {
                            ElementResult::Node(n) => n,
                            ElementResult::Script(script) => {
                                if let Some(ref mut ctx) = ctx {
                                    match ctx {
                                        CallingContext::Toplevel {
                                            script: ctx_script, ..
                                        } => {
                                            if ctx_script.is_some() {
                                                self.errors.push(ParseError::CannotHaveTwoScripts(
                                                    Location::new(start, self.index),
                                                ));
                                                nodes.push(Node::error(Location::new(
                                                    start, self.index,
                                                )));
                                                continue;
                                            }
                                            *ctx_script = Some(script);
                                            continue;
                                        }
                                    }
                                } else {
                                    self.errors.push(ParseError::CannotHaveNonTopLevelScript(
                                        Location::new(start, self.index),
                                    ));
                                    nodes.push(Node::error(Location::new(start, self.index)));
                                    continue;
                                }
                            }
                            ElementResult::Css(stylesheet) => {
                                if let Some(ref mut ctx) = ctx {
                                    match ctx {
                                        CallingContext::Toplevel { css, .. } => {
                                            if css.is_some() {
                                                self.errors.push(
                                                    ParseError::CannotHaveTwoStyleTags(
                                                        Location::new(start, self.index),
                                                    ),
                                                );
                                                nodes.push(Node::error(Location::new(
                                                    start, self.index,
                                                )));
                                                continue;
                                            }
                                            *css = Some(stylesheet);
                                            continue;
                                        }
                                    }
                                } else {
                                    self.errors.push(ParseError::CannotHaveNonTopLevelStyle(
                                        Location::new(start, self.index),
                                    ));
                                    nodes.push(Node::error(Location::new(start, self.index)));
                                    continue;
                                }
                            }
                        };

                        // If no closing tag was found (<br/>, for example), just push the tag
                        if self.close_tag_queue.is_empty() {
                            nodes.push(node);
                            continue;
                        }

                        let NodeType::Element(elem) = node.node_type() else {
                            unreachable!("should never parse anything other than an element")
                        };

                        // Validate that the closing tag matches the opening one
                        let tag = elem.tag();
                        let Some(close_tag) = self.close_tag_queue.pop_front() else {
                            unreachable!("checked if empty at top of block");
                        };
                        if close_tag != tag {
                            self.errors
                                .push(ParseError::InvalidClosingTag(tag.to_owned(), node.loc()));
                            nodes.push(Node::error(node.loc()));
                            continue;
                        }

                        nodes.push(node);
                    }
                }
                Some('{') => {
                    // Should be the {
                    self.consume();
                    // Accounting for the { we consumed
                    let start = self.index - 1;

                    match self.peek() {
                        Some('#') => {
                            let node = self.parse_special_block();
                            nodes.push(node);
                        }
                        Some('/') => {
                            break;
                        }
                        _ => {
                            let mustache = self.parse_mustache();
                            nodes.push(Node::new(
                                NodeType::Mustache(mustache),
                                Location::new(start, self.index - 1),
                            ))
                        }
                    }
                }
                _ => {
                    // Parse text node if not tag
                    nodes.push(self.parse_text_node());
                }
            }
        }

        nodes
    }

    fn consume_while<F>(&mut self, predicate: F) -> &'a str
    where
        F: Fn(char) -> bool,
    {
        let start = self.slice_index;
        while self.peek().is_some_and(|c| predicate(c)) {
            self.consume();
        }

        // Get the text we just consumed. Importantly, we use `self.slice_index`, as we're indexing
        // an &str, which means we could invalidate byte boundaries if we just counted by
        // `self.index`
        &self.source[start..self.source.len() - self.source[self.slice_index..].len()]
    }

    fn consume(&mut self) -> Option<char> {
        if let Some(peek) = self.peek_buffer {
            // index keeps track of how many characters we've consumed
            self.index += 1;
            self.slice_index += peek.len_utf8();
            self.peek_buffer = None;
            return Some(peek);
        }

        let c = self.input.next();
        if let Some(c) = c {
            self.index += 1;
            // Slice index should keep track of the total bytes we've consumed.
            self.slice_index += c.len_utf8();
        }
        c
    }

    fn expect_consume(&mut self, expected: char) -> bool {
        let c = self.consume();
        if !c.is_some_and(|c| c == expected) {
            self.errors.push(ParseError::ExpectedCharacter(
                expected,
                c.unwrap_or('\0'),
                Location::new(self.index, self.index),
            ));
            false
        } else {
            true
        }
    }

    fn peek(&mut self) -> Option<char> {
        if let Some(peek) = self.peek_buffer {
            return Some(peek);
        }

        let next = self.input.next()?;

        self.peek_buffer = Some(next);
        Some(next)
    }

    fn skip_whitespace(&mut self) -> &'a str {
        self.consume_while(|c| c.is_whitespace())
    }

    fn parse_text_node(&mut self) -> Node<'a> {
        self.consume_while(|c| c.is_whitespace() && c != '\n');
        let mut start = self.index;
        let mut text = self.consume_while(|c| c != '<' && c != '{');
        if text.get(0..=0).is_some_and(|c| c == "\n") {
            start = self.index.saturating_sub(1);
            text = &text[..1];
        }

        let node_type = NodeType::Text(text);
        Node::new(
            node_type,
            Location::new(start, self.index.saturating_sub(1)),
        )
    }

    fn parse_tag(&mut self) -> &'a str {
        // `is_digit(36)` checks if it's a-z, A-Z, or 0-9
        self.consume_while(|c| c.is_digit(36))
    }

    fn parse_element(&mut self) -> ElementResult<'a> {
        let start = self.index;
        let tag = self.parse_tag();
        if tag.is_empty() {
            return ElementResult::Node(Node::new(NodeType::Text("<"), Location::char(start - 1)));
        } else if tag == "script" {
            let script = self.parse_script_tag();
            return ElementResult::Script(script);
        } else if tag == "style" {
            return match self.parse_style_tag() {
                Some(css) => ElementResult::Css(css),
                None => ElementResult::Node(Node::error(Location::new(start, self.index))),
            };
        }
        self.skip_whitespace();
        let attrs = self.parse_attrs();
        self.consume_while(|c| c.is_whitespace() && c != '\n');
        let children = if self.peek() == Some('/') {
            self.consume();
            self.expect_consume('>');
            vec![]
        } else {
            self.parse_nodes(None)
        };

        ElementResult::Node(Node::new(
            NodeType::Element(Element::new(tag, attrs, children)),
            Location::new(start - 1, self.index - 1),
        ))
    }

    fn parse_attrs(&mut self) -> Vec<Attribute<'a>> {
        let mut attrs = vec![];

        while self.peek().is_some_and(|c| c != '>' && c != '/') {
            self.skip_whitespace();
            let name = self.consume_while(|c| !is_control_or_delim(c));
            match name {
                "on" => {
                    self.expect_consume(':');
                    let event = self.parse_tag();
                    self.skip_whitespace();
                    self.expect_consume('=');
                    self.skip_whitespace();
                    self.expect_consume('{');
                    let mustache = self.parse_mustache();
                    attrs.push(Attribute::EventHandler(EventHandler::new(event, mustache)));
                }
                "bind" => {
                    self.expect_consume(':');
                    let binding = self.parse_tag();
                    self.skip_whitespace();
                    attrs.push(Attribute::Binding(binding));
                }
                _ => {
                    self.skip_whitespace();
                    let value = if self.peek().is_some_and(|c| c == '=') {
                        // Consume the '='
                        self.consume();
                        self.skip_whitespace();
                        let s = self.parse_attr_value();
                        self.skip_whitespace();
                        Some(s)
                    } else {
                        None
                    };

                    attrs.push(Attribute::KeyValue(name, value));
                }
            }
        }

        if self.peek() == Some('/') {
            self.consume();
        }
        // Should be the '>'
        self.consume();
        attrs
    }

    fn parse_attr_value(&mut self) -> AttributeValue<'a> {
        match self.peek() {
            Some(c) if c == '"' || c == '\'' => {
                self.consume();
                let value = self.consume_while(|str_char| str_char != c);
                self.consume();
                AttributeValue::Literal(value)
            }
            Some('{') => {
                // Should be the {
                self.consume();
                let expr = self.parse_mustache();
                AttributeValue::JavaScript(expr)
            }
            c => {
                self.errors.push(ParseError::ExpectedCharacterAny(
                    vec!['{', '"', '\''],
                    c.unwrap_or('\0'),
                    Location::char(self.index),
                ));
                AttributeValue::Literal("")
            }
        }
    }

    fn parse_mustache(&mut self) -> SyntaxNode {
        let start = self.slice_index;
        // Keep track of rbraces needed to close out mustache tag. This is needed because the inner
        // JavaScript could potentially have curly braces
        let mut rbraces_needed = 1;
        while rbraces_needed > 0 {
            let c = self.peek();
            match c {
                Some('{') => {
                    self.consume();
                    rbraces_needed += 1;
                }
                Some('}') => {
                    rbraces_needed -= 1;
                    if rbraces_needed != 0 {
                        self.consume();
                    }
                }
                None | Some(_) => {
                    self.consume();
                }
            }
        }

        // This is not extracted into a function, because it is very much tied to the current state
        // of the parser. A misuse of this could be bad and hard to debug
        let expr = &self.source[start..self.source.len() - self.source[self.slice_index..].len()];

        // Should be the last rbrace needed
        self.expect_consume('}');

        // Turn into rslint JavaScript syntax tree
        rslint_parser::parse_text(expr, 0).syntax()
    }

    fn try_parse_comment(&mut self) -> Option<Node<'a>> {
        // Start includes the < already consumed
        let start = self.index - 1;
        let (_, found) = self.try_consume("!--");
        if !found {
            // Returns None so calling function knows that there was no comment. A text node should
            // be created instead
            return None;
        }

        let start_slice = self.slice_index;
        let mut found = false;
        while !found && self.peek().is_some() {
            self.consume_while(|c| c != '-');
            let (_, successful) = self.try_consume("-->");
            found = successful;
        }

        let text =
            &self.source[start_slice..self.source.len() - self.source[self.slice_index..].len()];
        if !text.ends_with("-->") {
            self.errors
                .push(ParseError::ExpectedEndOfComment(Location::new(
                    start,
                    self.index - 1,
                )));
            return None;
        }
        Some(Node::new(
            // This slice is safe because we checked the above
            NodeType::Comment(&text[..text.len() - 3]),
            Location::new(start, self.index - 1),
        ))
    }

    fn try_consume(&mut self, text: &str) -> (&'a str, bool) {
        let start = self.slice_index;
        let pieces_needed = text.chars();
        for needed in pieces_needed {
            if !self.consume().is_some_and(|c| c == needed) {
                let t =
                    &self.source[start..self.source.len() - self.source[self.slice_index..].len()];
                return (t, false);
            }
        }

        (
            &self.source[start..self.source.len() - self.source[self.slice_index..].len()],
            true,
        )
    }

    fn parse_special_block(&mut self) -> Node<'a> {
        let start = self.index - 1;
        self.expect_consume('#');
        let block_type = self.consume_while(|c| !c.is_whitespace() && c != '}');

        match block_type {
            "for" => {
                let mut error = false;

                self.skip_whitespace();
                let mut ident =
                    self.consume_while(|c| !c.is_whitespace() && c != ',' && c.is_digit(36));
                self.skip_whitespace();
                let mut index_ident = None;
                if self.peek().is_some_and(|c| c == ',') {
                    // Should be the ','
                    self.consume();
                    self.skip_whitespace();
                    // First one found should be index ident
                    index_ident = Some(ident);
                    ident = self.consume_while(|c| !c.is_whitespace() && c.is_digit(36));
                }
                self.skip_whitespace();
                {
                    let start = self.index;
                    let (got, found) = self.try_consume("in");
                    if !found {
                        let mut full_got = got.to_owned();
                        full_got.push_str(self.consume_while(|c| !c.is_whitespace() && c != '}'));
                        self.errors.push(ParseError::ExpectedString(
                            "in".to_owned(),
                            full_got,
                            Location::new(start, self.index),
                        ));
                        error = true;
                    }
                }
                self.skip_whitespace();
                let expr = self.parse_mustache();
                let inner = self.parse_nodes(None);
                self.expect_consume('/');
                {
                    let start = self.index;
                    let (got, found) = self.try_consume(block_type);
                    if !found {
                        let mut full_got = got.to_owned();
                        full_got.push_str(self.consume_while(|c| c != '}'));
                        self.errors.push(ParseError::InvalidClosingBlock(
                            "for".to_owned(),
                            full_got,
                            Location::new(start, self.index),
                        ));
                        error = true;
                    }
                }
                self.expect_consume('}');
                if error {
                    Node::error(Location::new(start, self.index))
                } else {
                    Node::new(
                        NodeType::SpecialBlock(SpecialBlock::For(ForBlock::new(
                            ident,
                            index_ident,
                            expr,
                            inner,
                        ))),
                        Location::new(start, self.index),
                    )
                }
            }
            "if" => {
                let mut error = false;
                self.skip_whitespace();
                let expr = self.parse_mustache();
                let inner = self.parse_nodes(None);
                self.expect_consume('/');
                {
                    let start = self.index;
                    let (got, found) = self.try_consume(block_type);
                    if !found {
                        let mut full_got = got.to_owned();
                        full_got.push_str(self.consume_while(|c| c != '}'));
                        self.errors.push(ParseError::InvalidClosingBlock(
                            "if".to_owned(),
                            full_got,
                            Location::new(start, self.index),
                        ));
                        error = true;
                    }
                }
                self.expect_consume('}');
                if error {
                    Node::error(Location::new(start, self.index))
                } else {
                    Node::new(
                        NodeType::SpecialBlock(SpecialBlock::If(IfBlock::new(expr, inner, None))),
                        Location::new(start, self.index),
                    )
                }
            }
            _ => {
                self.consume_while(|c| c != '}');
                self.consume();
                self.consume_while(|c| c != '}');
                self.consume();
                self.errors.push(ParseError::UnrecognizedSpecialBlock(
                    block_type.to_owned(),
                    Location::new(start, self.index),
                ));
                Node::error(Location::new(start, self.index))
            }
        }
    }

    fn consume_until_end_tag(&mut self, name: &'static str) -> &'a str {
        let start = self.slice_index;
        let mut end_found = false;
        while !end_found && self.peek().is_some() {
            if self.consume() == Some('<') {
                if self.consume() == Some('/') {
                    let rest = self.consume_while(|c| c.is_ascii_alphabetic());
                    if rest == name {
                        self.expect_consume('>');
                        end_found = true
                    }
                }
            }
        }

        let consumed =
            &self.source[start..self.source.len() - self.source[self.slice_index..].len()];

        consumed
            .strip_suffix(&format!("</{name}>"))
            .unwrap_or(consumed)
    }

    fn parse_script_tag(&mut self) -> SyntaxNode {
        self.expect_consume('>');
        let source = self.consume_until_end_tag("script");
        rslint_parser::parse_text(source, 0).syntax()
    }

    fn parse_style_tag(&mut self) -> Option<StyleSheet<'a, 'a>> {
        self.expect_consume('>');
        let source = self.consume_until_end_tag("style");
        let result = StyleSheet::parse(source, lightningcss::stylesheet::ParserOptions::default());

        match result {
            Ok(css) => Some(css),
            Err(err) => {
                self.errors.push(ParseError::CssParseError(err.kind));
                return None;
            }
        }
    }
}

fn is_control_or_delim(ch: char) -> bool {
    match ch {
        '\u{007F}' => true,
        c if c >= '\u{0000}' && c <= '\u{001F}' => true,
        c if c >= '\u{0080}' && c <= '\u{009F}' => true,
        ' ' | '"' | '\'' | '>' | '/' | '=' | ':' => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! insta_test_all {
        ($($input:expr),+) => {
            $(
                let parser = Parser::new($input);
                let (tree, errs) = parser.parse();
                assert_eq!(Vec::<ParseError>::new(), errs);
                ::insta::assert_debug_snapshot!(tree);
             )+
        };
    }

    macro_rules! insta_test_all_err {
        ($($input_and_errs:expr),+) => {
            $(
                let parser = Parser::new($input_and_errs.0);
                let (tree, errs) = parser.parse();
                assert_eq!($input_and_errs.1, errs);
                ::insta::assert_debug_snapshot!(tree);
             )+
        };
    }

    #[test]
    fn can_parse_text_node() {
        insta_test_all!(
            "hello",
            "   hello",
            "hello hello",
            "你",
            "你好",
            "<",
            ">",
            "<>hello"
        );
    }

    #[test]
    fn can_parse_basic_elements() {
        insta_test_all!(
            "<p>你好</p>",
            "<tagname><p>inner</p></tagname>",
            "<p>
                <p>hello</p>
            </p>
            "
        );
    }

    #[test]
    fn parses_basic_attrs() {
        insta_test_all!(
            "<p attr1='hello'></p>",
            "<p attr=\"attribute\"></p>",
            "<p ></p>",
            "<p attr=\"你好\"></p>",
            "<p attr=\"one\"     attr2=\"two\">text</p>",
            "<p attr></p>"
        );
    }

    #[test]
    fn reports_unmatched_tag_errors() {
        insta_test_all_err!((
            "<p></invalid>",
            vec![ParseError::InvalidClosingTag(
                "p".to_owned(),
                Location::new(0, 12)
            )]
        ));
    }

    #[test]
    fn continues_parsing_if_unmatched_tag_found() {
        insta_test_all_err!(
            (
                "<p></invalid>
            <span>hello</span>",
                vec![ParseError::InvalidClosingTag(
                    "p".to_owned(),
                    Location::new(0, 12)
                )],
            ),
            (
                "
                <span>hello
                <p></p>
                <span></span>
                ",
                Vec::<ParseError>::new()
            )
        );
    }

    #[test]
    fn reports_invalid_closing_tag_angle_bracket() {
        insta_test_all_err!((
            "<p></p!",
            vec![ParseError::ExpectedCharacter('>', '!', Location::new(7, 7))]
        ));
    }

    #[test]
    fn can_parse_mustache_in_event_handle_attr() {
        insta_test_all!(
            "<p on:click={hello}></p>",
            "<p on:click={ () => { console.log(\"hi\") } }></p>"
        );
    }

    #[test]
    fn can_parse_bindings() {
        insta_test_all!("<p bind:value></p>");
    }

    #[test]
    fn can_parse_attrs_with_mustache_values() {
        insta_test_all!(
            "<p attr={value}></p>",
            "<p attr={ () => { console.log(value) } }></p>"
        );
    }

    #[test]
    fn can_parse_comments() {
        insta_test_all!("<!-- hello -->", "<!--hello--><p></p>", "<!-");
    }

    #[test]
    fn gives_error_when_end_of_comment_not_found() {
        insta_test_all_err!(
            (
                "<!-- hello",
                vec![ParseError::ExpectedEndOfComment(Location::new(0, 9))]
            ),
            (
                "<!-- hello<p>hello</p>",
                vec![ParseError::ExpectedEndOfComment(Location::new(0, 21))]
            )
        );
    }

    #[test]
    fn can_parse_mustache_tags() {
        insta_test_all!(
            "   {hello}",
            "{ () => { console.log(\"HI\") } }",
            "hello, {hello}"
        );
    }

    #[test]
    fn can_parse_if_block() {
        insta_test_all!("{#if x == 3}<p>hello</p>{/if}");
    }

    #[test]
    fn can_parse_for_block() {
        insta_test_all!(
            "{#for i in [1, 2, 3]}<p>{i}</p>{/for}",
            "{#for i, elem in [1, 2, 3]}<p>Index: {i}. Elem: {elem}</p>{/for}",
            "{#for i in [1, 2, 3]}<p>{i}</p>{/for}
            <p>Text</p>",
            "{#for i in [1, 2, 3]}
                {#if i == 1}
                    <span>{i}</span>
                {/if}
            {/for}"
        );
    }

    #[test]
    fn errors_on_invalid_closing_special_block() {
        insta_test_all_err!(
            (
                "{#for i in [1, 2, 3]}{i}{/if}",
                vec![ParseError::InvalidClosingBlock(
                    "for".to_owned(),
                    "if".to_owned(),
                    Location::new(26, 28)
                )]
            ),
            (
                "{#if i == 3}{i}{/for}",
                vec![ParseError::InvalidClosingBlock(
                    "if".to_owned(),
                    "for".to_owned(),
                    Location::new(17, 20)
                )]
            )
        );
    }

    #[test]
    fn error_when_no_in_token_found_in_for_block() {
        insta_test_all_err!((
            "{#for i bin [1, 2, 3]}{i}{/for}",
            vec![ParseError::ExpectedString(
                "in".to_owned(),
                "bin".to_owned(),
                Location::new(8, 11)
            )]
        ));
    }

    #[test]
    fn error_on_unrecognized_special_block() {
        insta_test_all_err!((
            "{#what}stuff{/what}",
            vec![ParseError::UnrecognizedSpecialBlock(
                "what".to_owned(),
                Location::new(0, 19)
            )]
        ));
    }

    #[test]
    fn can_parse_self_closing_tag() {
        insta_test_all!("<p/>", "<br/>");
    }

    #[test]
    fn can_parse_script() {
        insta_test_all!(
            "<script>x = 3;</script>",
            "<script>x = 3;</script><p>hello</p>"
        );
    }

    #[test]
    fn cannot_have_scripts_in_invalid_positions() {
        insta_test_all_err!(
            (
                "<p><script>x = 3;</script></p>",
                vec![ParseError::CannotHaveNonTopLevelScript(Location::new(
                    4, 26
                ))]
            ),
            (
                "<script>x = 3;</script><script>x = 3;</script>",
                vec![ParseError::CannotHaveTwoScripts(Location::new(24, 46))]
            )
        );
    }

    #[test]
    fn can_parse_css() {
        insta_test_all!(
            "<style>hello { color: red; }</style>",
            "<style>hello { color: red; }</style><p>Hello</p>",
            "<style>hello { color: red; }</style><script>x = 3;</script><p>hello</p>"
        );
    }
}
