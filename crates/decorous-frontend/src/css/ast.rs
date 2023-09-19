use std::{borrow::Cow, fmt};

use itertools::Itertools;
use rslint_parser::{SmolStr, SyntaxNode};

#[derive(Debug)]
pub struct Css {
    pub rules: Vec<Rule>,
}

#[derive(Debug, PartialEq)]
pub enum Rule {
    At(AtRule),
    Regular(RegularRule),
}

#[derive(Debug, PartialEq)]
pub struct RegularRule {
    pub selector: Vec<Selector>,
    pub declarations: Vec<Declaration>,
}

#[derive(Debug, PartialEq)]
pub struct AtRule {
    pub name: SmolStr,
    pub additional: SmolStr,
    pub contents: Option<Vec<Rule>>,
}

#[derive(Debug, PartialEq)]
pub struct Selector {
    pub parts: Vec<SelectorPart>,
}

#[derive(Debug, PartialEq)]
pub struct SelectorPart {
    pub text: Option<SmolStr>,
    pub pseudoes: Vec<Pseudo>,
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
    pub name: SmolStr,
    pub values: Vec<Value>,
}

#[derive(Debug, PartialEq)]
pub enum Value {
    Mustache(SyntaxNode),
    Css(SmolStr),
}

// ---Display impls---

impl fmt::Display for SelectorPart {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}{}",
            self.text.as_ref().unwrap_or(&Cow::default()),
            self.pseudoes.iter().join("")
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
        write!(f, "{}: {};", self.name, self.values.iter().join(" "))
    }
}

impl fmt::Display for AtRule {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(contents) = &self.contents {
            write!(
                f,
                "@{} {} {{ {} }}",
                self.name,
                self.additional,
                contents.iter().join(" ")
            )
        } else {
            write!(f, "@{} {};", self.name, self.additional)
        }
    }
}

impl fmt::Display for RegularRule {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} {{ {} }}",
            self.selector.iter().join(", "),
            self.declarations.iter().join("; ")
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
        write!(f, "{}", self.parts.iter().join(" "))
    }
}

impl fmt::Display for Css {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.rules.iter().join("\n"))
    }
}
