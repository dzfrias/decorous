use std::{borrow::Cow, fmt};

use itertools::Itertools;
use rslint_parser::SyntaxNode;

#[derive(Debug)]
pub struct Css<'a> {
    rules: Vec<Rule<'a>>,
}

#[derive(Debug, PartialEq)]
pub enum Rule<'a> {
    At(AtRule<'a>),
    Regular(RegularRule<'a>),
}

#[derive(Debug, PartialEq)]
pub struct RegularRule<'a> {
    selector: Selector<'a>,
    declarations: Vec<Declaration<'a>>,
}

#[derive(Debug, PartialEq)]
pub struct AtRule<'a> {
    name: &'a str,
    additional: &'a str,
    contents: Option<Vec<Rule<'a>>>,
}

#[derive(Debug, PartialEq)]
pub struct Selector<'a> {
    parts: Vec<SelectorPart<'a>>,
}

#[derive(Debug, PartialEq)]
pub struct SelectorPart<'a> {
    text: Option<Cow<'a, str>>,
    pseudoes: Vec<Pseudo<'a>>,
}

#[derive(Debug, PartialEq)]
pub enum Pseudo<'a> {
    Element(&'a str),
    Class {
        name: &'a str,
        value: Option<&'a str>,
    },
}

#[derive(Debug, PartialEq)]
pub struct Declaration<'a> {
    name: &'a str,
    values: Vec<Value<'a>>,
}

#[derive(Debug, PartialEq)]
pub enum Value<'a> {
    Mustache(SyntaxNode),
    Css(&'a str),
}

impl<'a> Declaration<'a> {
    pub fn new(name: &'a str, values: Vec<Value<'a>>) -> Self {
        Self { name, values }
    }

    pub fn name(&self) -> &str {
        self.name
    }

    pub fn values(&self) -> &[Value<'_>] {
        self.values.as_ref()
    }

    pub fn values_mut(&mut self) -> &mut Vec<Value<'a>> {
        &mut self.values
    }

    pub fn name_mut(&mut self) -> &mut &'a str {
        &mut self.name
    }
}

impl<'a> SelectorPart<'a> {
    pub fn new(text: Option<Cow<'a, str>>, pseudoes: Vec<Pseudo<'a>>) -> Self {
        Self { text, pseudoes }
    }

    pub fn text(&self) -> Option<&Cow<'a, str>> {
        self.text.as_ref()
    }

    pub fn pseudoes(&self) -> &[Pseudo<'_>] {
        self.pseudoes.as_ref()
    }

    pub fn text_mut(&mut self) -> &mut Option<Cow<'a, str>> {
        &mut self.text
    }

    pub fn pseudoes_mut(&mut self) -> &mut Vec<Pseudo<'a>> {
        &mut self.pseudoes
    }
}

impl<'a> Selector<'a> {
    pub fn new(parts: Vec<SelectorPart<'a>>) -> Self {
        Self { parts }
    }

    pub fn parts_mut(&mut self) -> &mut Vec<SelectorPart<'a>> {
        &mut self.parts
    }

    pub fn parts(&self) -> &[SelectorPart<'_>] {
        self.parts.as_ref()
    }
}

impl<'a> RegularRule<'a> {
    pub fn new(selector: Selector<'a>, declarations: Vec<Declaration<'a>>) -> Self {
        Self {
            selector,
            declarations,
        }
    }

    pub fn selector(&self) -> &Selector<'a> {
        &self.selector
    }

    pub fn declarations(&self) -> &[Declaration<'_>] {
        self.declarations.as_ref()
    }

    pub fn selector_mut(&mut self) -> &mut Selector<'a> {
        &mut self.selector
    }

    pub fn declarations_mut(&mut self) -> &mut Vec<Declaration<'a>> {
        &mut self.declarations
    }
}

impl<'a> Css<'a> {
    pub fn new(rules: Vec<Rule<'a>>) -> Self {
        Self { rules }
    }

    pub fn rules(&self) -> &[Rule<'a>] {
        &self.rules
    }

    pub fn rules_mut(&mut self) -> &mut Vec<Rule<'a>> {
        &mut self.rules
    }
}

impl<'a> AtRule<'a> {
    pub fn new(name: &'a str, additional: &'a str, contents: Option<Vec<Rule<'a>>>) -> Self {
        Self {
            name,
            additional,
            contents,
        }
    }

    pub fn name(&self) -> &str {
        self.name
    }

    pub fn additional(&self) -> &str {
        self.additional
    }

    pub fn contents(&self) -> Option<&[Rule<'a>]> {
        self.contents.as_deref()
    }

    pub fn contents_mut(&mut self) -> &mut Option<Vec<Rule<'a>>> {
        &mut self.contents
    }

    pub fn additional_mut(&mut self) -> &mut &'a str {
        &mut self.additional
    }

    pub fn name_mut(&mut self) -> &mut &'a str {
        &mut self.name
    }
}

// ---Display impls---

impl<'a> fmt::Display for SelectorPart<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}",
            self.text().unwrap_or(&Cow::default()),
            self.pseudoes().iter().join("")
        )
    }
}

impl<'a> fmt::Display for Value<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mustache(s) => write!(f, "{s}"),
            Self::Css(css) => write!(f, "{css}"),
        }
    }
}

impl<'a> fmt::Display for Pseudo<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Element(elem) => write!(f, "::{elem}"),
            Self::Class {
                name,
                value: Some(value),
            } => write!(f, ":{name}({value})"),
            Self::Class { name, value: None } => {
                write!(f, ":{name}")
            }
        }
    }
}

impl<'a> fmt::Display for Declaration<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {};", self.name, self.values().iter().join(" "))
    }
}

impl<'a> fmt::Display for AtRule<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(contents) = self.contents() {
            write!(
                f,
                "@{} {} {{ {} }}",
                self.name(),
                self.additional(),
                contents.iter().join(" ")
            )
        } else {
            write!(f, "@{} {};", self.name(), self.additional())
        }
    }
}

impl<'a> fmt::Display for RegularRule<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {{ {} }}",
            self.selector(),
            self.declarations().iter().join("; ")
        )
    }
}

impl<'a> fmt::Display for Rule<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::At(at_rule) => write!(f, "{at_rule}"),
            Self::Regular(regular) => write!(f, "{regular}"),
        }
    }
}

impl<'a> fmt::Display for Selector<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.parts().iter().join(" "))
    }
}

impl<'a> fmt::Display for Css<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.rules().iter().join("\n"))
    }
}
