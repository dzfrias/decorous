use harpoon::Harpoon;
use rslint_parser::AstNode;

use super::{
    ast::{AtRule, Css, Declaration, Pseudo, RegularRule, Rule, Selector, SelectorPart, Value},
    error::{ParseError, ParseErrorType},
};
use crate::{errors::Help, location::Location};

pub type Result<T> = std::result::Result<T, ParseError<Location>>;

#[derive(Debug)]
pub struct Parser<'a> {
    harpoon: Harpoon<'a>,
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            harpoon: Harpoon::new(source),
        }
    }

    pub fn parse(mut self) -> Result<Css> {
        let mut rules = vec![];
        self.skip_whitespace();
        while self.harpoon.peek().is_some() {
            rules.push(self.parse_rule()?);
            self.skip_whitespace();
        }
        Ok(Css { rules })
    }

    fn parse_rule(&mut self) -> Result<Rule> {
        if self.harpoon.peek_is('@') {
            return Ok(Rule::At(self.parse_at_rule()?));
        }

        let selector = self.parse_selector()?;
        self.expect_consume('{')?;
        let mut declarations = vec![];
        self.skip_whitespace();
        while !self.harpoon.peek_is('}') && self.harpoon.peek().is_some() {
            declarations.push(self.parse_declaration()?);
            self.skip_whitespace();
        }
        self.expect_consume('}')?;

        Ok(Rule::Regular(RegularRule {
            selector,
            declarations,
        }))
    }

    fn parse_at_rule(&mut self) -> Result<AtRule> {
        debug_assert_eq!(Some('@'), self.harpoon.consume());
        let name = self
            .harpoon
            .harpoon(|h| {
                h.consume_while(|c| !c.is_whitespace());
            })
            .text();
        if name.is_empty() {
            return Err(ParseError::new(
                ParseErrorType::ExpectedMediaQueryName,
                Location::from_source(self.harpoon.offset() - 1, self.harpoon.source()),
                None,
            ));
        }
        self.skip_whitespace();
        let additional = self
            .harpoon
            .harpoon(|h| {
                h.consume_while(|c| !matches!(c, '{' | ';'));
            })
            .text();
        if self.harpoon.peek_is(';') {
            debug_assert_eq!(Some(';'), self.harpoon.consume());
            return Ok(AtRule {
                name: name.into(),
                additional: additional.into(),
                contents: None,
            });
        }

        self.expect_consume('{')?;
        self.skip_whitespace();
        let mut rules = vec![];
        while !self.harpoon.peek_is('}') && self.harpoon.peek().is_some() {
            rules.push(self.parse_rule()?);
            self.skip_whitespace();
        }
        self.expect_consume('}')?;

        Ok(AtRule {
            name: name.into(),
            additional: additional.into(),
            contents: Some(rules),
        })
    }

    fn parse_selector(&mut self) -> Result<Vec<Selector>> {
        let mut selectors = vec![];

        {
            let mut parts = vec![];
            while !self.harpoon.peek_is_any(",{") && self.harpoon.peek().is_some() {
                parts.push(self.parse_selector_part()?);
                self.skip_whitespace();
            }
            selectors.push(Selector { parts })
        }
        while self.harpoon.peek_is(',') {
            debug_assert_eq!(Some(','), self.harpoon.consume());
            let mut parts = vec![];
            while !self.harpoon.peek_is_any(",{") && self.harpoon.peek().is_some() {
                parts.push(self.parse_selector_part()?);
                self.skip_whitespace();
            }
            selectors.push(Selector { parts })
        }

        Ok(selectors)
    }

    fn parse_selector_part(&mut self) -> Result<SelectorPart> {
        fn parse_any<'a>(harpoon: &mut Harpoon<'a>) -> &'a str {
            harpoon
                .harpoon(|harpoon| {
                    harpoon.consume_while(|c| !matches!(c, '{' | ':' | ',') && !c.is_whitespace());
                })
                .text()
        }

        let text = if !self.harpoon.peek_is(':') {
            Some(parse_any(&mut self.harpoon))
        } else {
            None
        };

        let mut pseudoes = vec![];
        while self.harpoon.peek_is(':') {
            debug_assert_eq!(Some(':'), self.harpoon.consume());
            if self.harpoon.peek_is(':') {
                debug_assert_eq!(Some(':'), self.harpoon.consume());
                pseudoes.push(Pseudo::Element(parse_any(&mut self.harpoon).into()));
            } else {
                let class_name = self
                    .harpoon
                    .harpoon(|harpoon| {
                        harpoon.consume_while(|c| {
                            !c.is_whitespace() && !matches!(c, '{' | ':' | '(' | ',')
                        });
                    })
                    .text();
                let value = if self.harpoon.peek_is('(') {
                    debug_assert_eq!(Some('('), self.harpoon.consume());
                    let v = self
                        .harpoon
                        .harpoon(|harpoon| harpoon.consume_until(')'))
                        .text();
                    self.expect_consume(')')?;
                    Some(v)
                } else {
                    None
                };
                pseudoes.push(Pseudo::Class {
                    name: class_name.into(),
                    value: value.map(|v| v.into()),
                });
            }
        }

        Ok(SelectorPart {
            text: text.map(|t| t.into()),
            pseudoes,
        })
    }

    fn parse_declaration(&mut self) -> Result<Declaration> {
        let name = self
            .harpoon
            .harpoon(|harpoon| harpoon.consume_until(':'))
            .text();
        let offset = self.harpoon.offset();
        self.expect_consume(':')?;
        self.skip_whitespace();
        let mut values = vec![];
        values.push(self.parse_value()?);
        self.skip_whitespace();
        while !self.harpoon.peek_is_any(";{}:") && self.harpoon.peek().is_some() {
            values.push(self.parse_value()?);
            self.skip_whitespace();
        }
        self.expect_consume_with_help(
            ';',
            Some(Help::with_span(
                offset..offset + 1,
                "declaration needs a closing semicolon",
            )),
        )?;
        Ok(Declaration {
            name: name.into(),
            values,
        })
    }

    fn expect_consume(&mut self, expected: char) -> Result<()> {
        self.expect_consume_with_help(expected, None)
    }

    fn expect_consume_with_help(&mut self, expected: char, help: Option<Help>) -> Result<()> {
        if self.harpoon.peek_is(expected) {
            self.harpoon.consume();
            Ok(())
        } else {
            Err(ParseError::new(
                ParseErrorType::ExpectedCharacter(expected),
                Location::from_source(self.harpoon.offset(), self.harpoon.source()),
                help,
            ))
        }
    }

    fn parse_value(&mut self) -> Result<Value> {
        if self.harpoon.peek_is('{') {
            debug_assert_eq!(Some('{'), self.harpoon.consume());
            let offset = self.harpoon.offset();
            let contents = self.harpoon.harpoon(|h| h.consume_until('}')).text();
            self.expect_consume('}')?;
            let res = rslint_parser::parse_expr(contents, 0).ok().map_err(|err| {
                ParseError::new(
                    ParseErrorType::JavaScriptParseError(err),
                    Location::from_source(offset, self.harpoon.source()),
                    None,
                )
            })?;
            Ok(Value::Mustache(res.syntax().clone()))
        } else {
            let t = self
                .harpoon
                .harpoon(|h| h.consume_while(|c| !matches!(c, ';' | '{' | '}' | ':')))
                .text();
            Ok(Value::Css(t.into()))
        }
    }

    fn skip_whitespace(&mut self) {
        self.harpoon.consume_while(|c| c.is_whitespace());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! parser_test {
        ($($input:expr),+) => {
            $(
                let parser = Parser::new($input);
                insta::assert_debug_snapshot!(parser.parse());
             )+
        };
    }

    #[test]
    fn can_parse_basic_rules() {
        parser_test!(
            "p { color: green; }",
            "p { color:green; background: red      ; }",
            "p { color: green; background: red; } h1 { color: red; }"
        );
    }

    #[test]
    fn can_parse_more_complex_selectors() {
        parser_test!(
            "p.green { color: green; }",
            "p.green:hover { color: green; }",
            "p.green:has(h1, h2) { color: green; }",
            "p.green:has(h1, h2):hover { color: green; }",
            "p.green:has(h1, h2):hover::after { color: green; }",
            "p::after { color: green; }",
            "p::after, span.yellow { color: green; }",
            "p p { color: red; }"
        );
    }

    #[test]
    fn can_parse_mustache_tags() {
        parser_test!(
            "p { color: {color}; }",
            "p { color: {color} yellow blue; }",
            "p { width: {w}px; }"
        );
    }

    #[test]
    fn can_parse_more_complex_declaration_values() {
        parser_test!(
            "p { color: rgba(1, 2, 3, 4); }",
            "p { font-family: \"Fira Mono\", monospace; }",
            "p { box-shadow: 3px 3px red, 3px 3px olive; }"
        );
    }

    #[test]
    fn can_parse_at_rules() {
        parser_test!(
            "@import \"style.css\";",
            "@media (hover: hover) { p { color: green; } }",
            "@media (hover: hover) {}"
        );
    }

    #[test]
    fn parser_throws_errors_on_invalid_input() {
        parser_test!(
            "@ \"style.css\";",
            "@media (hover: hover)  p { color: green; } }",
            "p.green { color: green color: red; }",
            "p  color: green; }",
            "p { color: green; ",
            "p { color: {###}; }"
        );
    }
}
