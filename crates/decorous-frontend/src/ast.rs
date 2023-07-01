use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Node<'a> {
    loc: Location,
    node_type: NodeType<'a>,
}

#[derive(Debug, PartialEq, Eq, Clone, Hash, Copy)]
pub struct Location {
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeType<'a> {
    Element(Element<'a>),
    Text(&'a str),
    Comment(&'a str),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Element<'a> {
    tag: &'a str,
    attrs: HashMap<&'a str, Option<&'a str>>,
    children: Vec<Node<'a>>,
}

impl<'a> Element<'a> {
    pub fn new(
        tag: &'a str,
        attrs: HashMap<&'a str, Option<&'a str>>,
        children: Vec<Node<'a>>,
    ) -> Self {
        Self {
            tag,
            attrs,
            children,
        }
    }

    pub fn tag(&self) -> &str {
        self.tag
    }

    pub fn attrs(&self) -> &HashMap<&'a str, Option<&'a str>> {
        &self.attrs
    }

    pub fn children(&self) -> &[Node<'_>] {
        self.children.as_ref()
    }
}

impl<'a> Node<'a> {
    pub fn new(ty: NodeType<'a>, loc: Location) -> Self {
        Self { loc, node_type: ty }
    }

    pub fn node_type(&self) -> &NodeType<'a> {
        &self.node_type
    }

    pub fn loc(&self) -> Location {
        self.loc
    }
}

impl Location {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}
