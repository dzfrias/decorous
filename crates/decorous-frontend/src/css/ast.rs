use std::{borrow::Cow, fmt};

use itertools::Itertools;
use rslint_parser::{SmolStr, SyntaxNode};

#[derive(Debug)]
pub struct Css {
    rules: Vec<Rule>,
}

#[derive(Debug, PartialEq)]
pub enum Rule {
    At(AtRule),
    Regular(RegularRule),
}

#[derive(Debug, PartialEq)]
pub struct RegularRule {
    selector: Vec<Selector>,
    declarations: Vec<Declaration>,
}

#[derive(Debug, PartialEq)]
pub struct AtRule {
    name: SmolStr,
    additional: SmolStr,
    contents: Option<Vec<Rule>>,
}

#[derive(Debug, PartialEq)]
pub struct Selector {
    parts: Vec<SelectorPart>,
}

#[derive(Debug, PartialEq)]
pub struct SelectorPart {
    text: Option<SmolStr>,
    pseudoes: Vec<Pseudo>,
}

#[derive(Debug, PartialEq)]
pub enum Pseudo {
    Element(SmolStr),
    Class {
        name: SmolStr,
        value: Option<SmolStr>,
    },
}

#[derive(Debug, PartialEq)]
pub struct Declaration {
    name: SmolStr,
    values: Vec<Value>,
}

#[derive(Debug, PartialEq)]
pub enum Value {
    Mustache(SyntaxNode),
    Css(SmolStr),
}

impl Declaration {
    pub fn new(name: SmolStr, values: Vec<Value>) -> Self {
        Self { name, values }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn values(&self) -> &[Value] {
        self.values.as_ref()
    }

    pub fn values_mut(&mut self) -> &mut Vec<Value> {
        &mut self.values
    }

    pub fn name_mut(&mut self) -> &mut SmolStr {
        &mut self.name
    }
}

impl SelectorPart {
    pub fn new(text: Option<SmolStr>, pseudoes: Vec<Pseudo>) -> Self {
        Self { text, pseudoes }
    }

    pub fn text(&self) -> Option<&SmolStr> {
        self.text.as_ref()
    }

    pub fn pseudoes(&self) -> &[Pseudo] {
        self.pseudoes.as_ref()
    }

    pub fn text_mut(&mut self) -> Option<&mut SmolStr> {
        self.text.as_mut()
    }

    pub fn pseudoes_mut(&mut self) -> &mut Vec<Pseudo> {
        &mut self.pseudoes
    }
}

impl Selector {
    pub fn new(parts: Vec<SelectorPart>) -> Self {
        Self { parts }
    }

    pub fn parts_mut(&mut self) -> &mut Vec<SelectorPart> {
        &mut self.parts
    }

    pub fn parts(&self) -> &[SelectorPart] {
        self.parts.as_ref()
    }
}

impl RegularRule {
    pub fn new(selector: Vec<Selector>, declarations: Vec<Declaration>) -> Self {
        Self {
            selector,
            declarations,
        }
    }

    pub fn selector(&self) -> &[Selector] {
        &self.selector
    }

    pub fn declarations(&self) -> &[Declaration] {
        self.declarations.as_ref()
    }

    pub fn selector_mut(&mut self) -> &mut Vec<Selector> {
        &mut self.selector
    }

    pub fn declarations_mut(&mut self) -> &mut Vec<Declaration> {
        &mut self.declarations
    }
}

impl Css {
    pub fn new(rules: Vec<Rule>) -> Self {
        Self { rules }
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    pub fn rules_mut(&mut self) -> &mut Vec<Rule> {
        &mut self.rules
    }
}

impl AtRule {
    pub fn new(name: SmolStr, additional: SmolStr, contents: Option<Vec<Rule>>) -> Self {
        Self {
            name,
            additional,
            contents,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn additional(&self) -> &str {
        &self.additional
    }

    pub fn contents(&self) -> Option<&[Rule]> {
        self.contents.as_deref()
    }

    pub fn contents_mut(&mut self) -> &mut Option<Vec<Rule>> {
        &mut self.contents
    }

    pub fn additional_mut(&mut self) -> &mut SmolStr {
        &mut self.additional
    }

    pub fn name_mut(&mut self) -> &mut SmolStr {
        &mut self.name
    }
}

// ---Display impls---

impl fmt::Display for SelectorPart {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}{}",
            self.text().unwrap_or(&Cow::default()),
            self.pseudoes().iter().join("")
        )
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Mustache(s) => write!(f, "{s}"),
            Self::Css(css) => write!(f, "{css}"),
        }
    }
}

impl fmt::Display for Pseudo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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

impl fmt::Display for Declaration {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {};", self.name, self.values().iter().join(" "))
    }
}

impl fmt::Display for AtRule {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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

impl fmt::Display for RegularRule {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} {{ {} }}",
            self.selector().iter().join(", "),
            self.declarations().iter().join("; ")
        )
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::At(at_rule) => write!(f, "{at_rule}"),
            Self::Regular(regular) => write!(f, "{regular}"),
        }
    }
}

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.parts().iter().join(" "))
    }
}

impl fmt::Display for Css {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.rules().iter().join("\n"))
    }
}
