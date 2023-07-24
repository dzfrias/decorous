use harpoon::Harpoon;

use crate::css::ast::{AtRule, Pseudo};

use super::ast::{Css, Declaration, RegularRule, Rule, Selector, SelectorPart, Value};

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

    pub fn parse(mut self) -> Css<'a> {
        let mut rules = vec![];
        while self.harpoon.peek().is_some() {
            rules.push(self.parse_rule());
            self.skip_whitespace();
        }
        Css::new(rules)
    }

    fn parse_rule(&mut self) -> Rule<'a> {
        if self.harpoon.peek_is('@') {
            return Rule::At(self.parse_at_rule());
        }

        let selector = self.parse_selector();
        self.expect_consume('{');
        let mut declarations = vec![];
        self.skip_whitespace();
        while !self.harpoon.peek_is('}') && !self.harpoon.peek().is_none() {
            declarations.push(self.parse_declaration());
            self.skip_whitespace();
        }
        self.expect_consume('}');

        Rule::Regular(RegularRule::new(selector, declarations))
    }

    fn parse_at_rule(&mut self) -> AtRule<'a> {
        debug_assert_eq!(Some('@'), self.harpoon.consume());
        let name = self
            .harpoon
            .harpoon(|h| {
                h.consume_while(|c| !c.is_whitespace());
            })
            .text();
        self.skip_whitespace();
        let additional = self
            .harpoon
            .harpoon(|h| {
                h.consume_while(|c| !matches!(c, '{' | ';'));
            })
            .text();
        if self.harpoon.peek_is(';') {
            debug_assert_eq!(Some(';'), self.harpoon.consume());
            return AtRule::new(name, additional, None);
        }

        self.expect_consume('{');
        self.skip_whitespace();
        let mut rules = vec![];
        while !self.harpoon.peek_is('}') && !self.harpoon.peek().is_none() {
            rules.push(self.parse_rule());
            self.skip_whitespace();
        }
        self.expect_consume('}');

        AtRule::new(name, additional, Some(rules))
    }

    fn parse_selector(&mut self) -> Selector<'a> {
        let mut parts = vec![];
        parts.push(self.parse_selector_part());
        self.skip_whitespace();
        while self.harpoon.peek_is(',') {
            debug_assert_eq!(Some(','), self.harpoon.consume());
            self.skip_whitespace();
            parts.push(self.parse_selector_part());
            self.skip_whitespace();
        }

        Selector::new(parts)
    }

    fn parse_selector_part(&mut self) -> SelectorPart<'a> {
        fn parse_any<'a>(harpoon: &mut Harpoon<'a>) -> &'a str {
            harpoon
                .harpoon(|harpoon| {
                    harpoon.consume_while(|c| !matches!(c, '{' | ':' | ',') && !c.is_whitespace())
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
                pseudoes.push(Pseudo::Element(parse_any(&mut self.harpoon)));
            } else {
                let class_name = self
                    .harpoon
                    .harpoon(|harpoon| {
                        harpoon.consume_while(|c| {
                            !c.is_whitespace() && !matches!(c, '{' | ':' | '(' | ',')
                        })
                    })
                    .text();
                let value = if self.harpoon.peek_is('(') {
                    debug_assert_eq!(Some('('), self.harpoon.consume());
                    let v = self
                        .harpoon
                        .harpoon(|harpoon| harpoon.consume_until(')'))
                        .text();
                    self.expect_consume(')');
                    Some(v)
                } else {
                    None
                };
                pseudoes.push(Pseudo::Class {
                    name: class_name,
                    value,
                });
            }
        }

        SelectorPart::new(text, pseudoes)
    }

    fn parse_declaration(&mut self) -> Declaration<'a> {
        let name = self
            .harpoon
            .harpoon(|harpoon| harpoon.consume_until(':'))
            .text();
        self.expect_consume(':');
        self.skip_whitespace();
        let mut values = vec![];
        values.push(self.parse_value());
        self.skip_whitespace();
        while !self.harpoon.peek_is(';') && !self.harpoon.peek().is_none() {
            values.push(self.parse_value());
            self.skip_whitespace();
        }
        self.expect_consume(';');
        Declaration::new(name, values)
    }

    fn expect_consume(&mut self, expected: char) -> bool {
        if self.harpoon.peek_is(expected) {
            self.harpoon.consume();
            true
        } else {
            todo!("error");
        }
    }

    fn parse_value(&mut self) -> Value<'a> {
        if self.harpoon.peek_is('{') {
            debug_assert_eq!(Some('{'), self.harpoon.consume());
            let contents = self.harpoon.harpoon(|h| h.consume_until('}')).text();
            self.expect_consume('}');
            Value::Mustache(contents)
        } else {
            let t = self
                .harpoon
                .harpoon(|h| h.consume_while(|c| !matches!(c, ';' | '{')))
                .text();
            Value::Css(t)
        }
    }

    fn skip_whitespace(&mut self) {
        self.harpoon.consume_while(|c| c.is_whitespace())
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
            "p::after, span.yellow { color: green; }"
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
}
