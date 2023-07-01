use std::{
    collections::{HashMap, VecDeque},
    str::Chars,
};
use thiserror::Error;

use crate::ast::{Element, Location, Node, NodeType};

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

    errors: Vec<ParseError>,
}

#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error("invalid closing tag, expected {0}")]
    InvalidClosingTag(String, Location),
    #[error("invalid character, expected {0}, got {1}")]
    ExpectedCharacter(char, char, Location),
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
    pub fn parse(mut self) -> (Vec<Node<'a>>, Vec<ParseError>) {
        let nodes = self.parse_nodes();

        (nodes, self.errors)
    }

    fn parse_nodes(&mut self) -> Vec<Node<'a>> {
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
                        let c = self.consume();
                        if c != Some('>') {
                            self.errors.push(ParseError::ExpectedCharacter(
                                '>',
                                c.unwrap_or('\0'),
                                Location::new(self.index, self.index),
                            ));
                        }

                        // Add to the closing tag queue
                        self.close_tag_queue.push_back(close);
                        break;
                    } else {
                        let node = self.parse_element();

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
                            continue;
                        }

                        nodes.push(node);
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
        let mut text = self.consume_while(|c| c != '<');
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

    fn parse_element(&mut self) -> Node<'a> {
        let start = self.index;
        let tag = self.parse_tag();
        self.skip_whitespace();
        let attrs = self.parse_attrs();
        let children = self.parse_nodes();

        Node::new(
            NodeType::Element(Element::new(tag, attrs, children)),
            Location::new(start - 1, self.index.saturating_sub(1)),
        )
    }

    fn parse_attrs(&mut self) -> HashMap<&'a str, Option<&'a str>> {
        let mut attrs = HashMap::new();

        while self.peek().is_some_and(|c| c != '>') {
            self.skip_whitespace();
            let name = self.consume_while(|c| !is_control_or_delim(c));
            self.skip_whitespace();
            let value = if self.peek().is_some_and(|c| c == '=') {
                self.consume();
                self.skip_whitespace();
                let s = self.parse_attr_value();
                self.skip_whitespace();
                Some(s)
            } else {
                None
            };

            attrs.insert(name, value);
        }

        // Should be the '>'
        self.consume();
        attrs
    }

    fn parse_attr_value(&mut self) -> &'a str {
        match self.peek() {
            Some(c) if c == '"' || c == '\'' => {
                self.consume();
                let v = self.consume_while(|str_char| str_char != c);
                self.consume();
                v
            }
            _ => {
                todo!("parse mustache tag");
            }
        }
    }
}

fn is_control_or_delim(ch: char) -> bool {
    match ch {
        '\u{007F}' => true,
        c if c >= '\u{0000}' && c <= '\u{001F}' => true,
        c if c >= '\u{0080}' && c <= '\u{009F}' => true,
        ' ' | '"' | '\'' | '>' | '/' | '=' => true,
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
        insta_test_all!("hello", "   hello", "hello hello", "你", "你好");
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
            "<p attr=\"one\"     attr2=\"two\">text</p>"
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
}
