use rslint_parser::SyntaxNode;

#[derive(Debug, Clone, PartialEq)]
pub struct DecorousAst<'a> {
    nodes: Vec<Node<'a>>,
    script: Option<SyntaxNode>,
}

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
    SpecialBlock(SpecialBlock<'a>),
    Mustache(SyntaxNode),
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Element<'a> {
    tag: &'a str,
    attrs: Vec<Attribute<'a>>,
    children: Vec<Node<'a>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SpecialBlock<'a> {
    For(ForBlock<'a>),
    If(IfBlock<'a>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForBlock<'a> {
    binding: &'a str,
    index: Option<&'a str>,
    expr: SyntaxNode,
    inner: Vec<Node<'a>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfBlock<'a> {
    expr: SyntaxNode,
    inner: Vec<Node<'a>>,
    else_block: Option<Vec<Node<'a>>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Attribute<'a> {
    EventHandler(EventHandler<'a>),
    Binding(&'a str),
    KeyValue(&'a str, Option<AttributeValue<'a>>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventHandler<'a> {
    event: &'a str,
    expr: SyntaxNode,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue<'a> {
    Literal(&'a str),
    JavaScript(SyntaxNode),
}

impl<'a> Element<'a> {
    pub fn new(tag: &'a str, attrs: Vec<Attribute<'a>>, children: Vec<Node<'a>>) -> Self {
        Self {
            tag,
            attrs,
            children,
        }
    }

    pub fn tag(&self) -> &str {
        self.tag
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

    pub fn error(loc: Location) -> Self {
        Self::new(NodeType::Error, loc)
    }
}

impl Location {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn char(idx: usize) -> Self {
        Self {
            start: idx,
            end: idx,
        }
    }
}

impl<'a> ForBlock<'a> {
    pub fn new(
        binding: &'a str,
        index: Option<&'a str>,
        expr: SyntaxNode,
        inner: Vec<Node<'a>>,
    ) -> Self {
        Self {
            binding,
            index,
            expr,
            inner,
        }
    }
}

impl<'a> EventHandler<'a> {
    pub fn new(event: &'a str, expr: SyntaxNode) -> Self {
        Self { event, expr }
    }

    pub fn event(&self) -> &str {
        self.event
    }

    pub fn expr(&self) -> &SyntaxNode {
        &self.expr
    }
}

impl<'a> IfBlock<'a> {
    pub fn new(expr: SyntaxNode, inner: Vec<Node<'a>>, else_block: Option<Vec<Node<'a>>>) -> Self {
        Self {
            expr,
            inner,
            else_block,
        }
    }
}

impl<'a> DecorousAst<'a> {
    pub fn new(nodes: Vec<Node<'a>>, script: Option<SyntaxNode>) -> Self {
        Self { nodes, script }
    }
}
