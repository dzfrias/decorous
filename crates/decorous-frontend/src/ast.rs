use std::{borrow::Cow, fmt, path::Path};

use heck::ToSnekCase;
use itertools::Itertools;
use rslint_parser::SyntaxNode;

use crate::{css::ast::Css, location::Location};

/// The abstract syntax tree representation of decorous source code.
///
/// This struct has three main components (all with respective getter methods):
/// 1. [`nodes`](Self::nodes), for holding HTML markup.
/// 2. [`script`](Self::script), for holding JavaScript.
/// 3. [`css`](Self::css), for holding CSS.
///
/// One of the best uses of `DecorousAst` is to hold data, read some parts, and eventually use
/// [`into_components()`](DecorousAst::into_components()) when the time has come to apply more
/// complex transformations that require ownership.
#[derive(Debug)]
pub struct DecorousAst<'a> {
    pub nodes: Vec<Node<'a, Location>>,
    pub script: Option<SyntaxNode>,
    pub css: Option<Css>,
    pub wasm: Option<Code<'a>>,
    pub comptime: Option<Code<'a>>,
}

/// A node of the [AST](DecorousAst).
///
/// It contains [metadata](`Self::metadata()`) (of type `T`), and the
/// actual node data, retrieved by [`node_type()`](`Self::node_type()`).
#[derive(Debug, Clone, PartialEq)]
pub struct Node<'a, T> {
    pub metadata: T,
    pub node_type: NodeType<'a, T>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeType<'a, T> {
    Element(Element<'a, T>),
    Text(Text<'a>),
    Comment(Comment<'a>),
    SpecialBlock(SpecialBlock<'a, T>),
    Mustache(Mustache),
}

#[derive(Debug, Clone, PartialEq, Hash)]
pub struct Mustache(pub SyntaxNode);

impl fmt::Display for Mustache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::ops::Deref for Mustache {
    type Target = SyntaxNode;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Hash, Copy)]
pub struct Text<'a>(pub &'a str);

impl fmt::Display for Text<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<'a> std::ops::Deref for Text<'a> {
    type Target = &'a str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Hash, Copy)]
pub struct Comment<'a>(pub &'a str);

impl<'a> std::ops::Deref for Comment<'a> {
    type Target = &'a str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Element<'a, T> {
    pub tag: &'a str,
    pub attrs: Vec<Attribute<'a>>,
    pub children: Vec<Node<'a, T>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SpecialBlock<'a, T> {
    For(ForBlock<'a, T>),
    If(IfBlock<'a, T>),
    Use(UseBlock<'a>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForBlock<'a, T> {
    pub binding: &'a str,
    pub index: Option<&'a str>,
    pub expr: SyntaxNode,
    pub inner: Vec<Node<'a, T>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfBlock<'a, T> {
    pub expr: SyntaxNode,
    pub inner: Vec<Node<'a, T>>,
    pub else_block: Option<Vec<Node<'a, T>>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseBlock<'a> {
    pub path: &'a Path,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Attribute<'a> {
    EventHandler(EventHandler<'a>),
    Binding(&'a str),
    KeyValue(&'a str, Option<AttributeValue<'a>>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventHandler<'a> {
    pub event: &'a str,
    pub expr: SyntaxNode,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue<'a> {
    Literal(Cow<'a, str>),
    JavaScript(SyntaxNode),
}

#[derive(Debug, Clone, PartialEq)]
pub enum CollapsedChildrenType<'a> {
    Text(&'a str),
    Html(String),
}

#[derive(Debug)]
pub struct Code<'a> {
    pub lang: &'a str,
    pub body: &'a str,
    pub offset: usize,
    pub comptime: bool,
}
impl<'a, T> Element<'a, T> {
    pub fn descendents(&'a self) -> NodeIter<'a, T> {
        NodeIter::new(&self.children)
    }

    pub fn descendents_mut<F>(&'a mut self, f: &mut F)
    where
        F: FnMut(&mut Node<'a, T>),
    {
        for child in &mut self.children {
            f(child);
            if let NodeType::Element(elem) = &mut child.node_type {
                elem.descendents_mut(f);
            }
        }
    }

    pub fn has_immediate_mustache(&self) -> bool {
        self.children
            .iter()
            .any(|child| matches!(child.node_type, NodeType::Mustache(_)))
    }

    pub fn js_valid_tag_name(&self) -> Cow<'a, str> {
        if self.tag.contains('-') {
            Cow::Owned(self.tag.to_snek_case())
        } else {
            Cow::Borrowed(self.tag)
        }
    }
}

impl<'a, T> Node<'a, T> {
    pub fn new(ty: NodeType<'a, T>, metadata: T) -> Self {
        Self {
            metadata,
            node_type: ty,
        }
    }

    /// Recursively cast each the metadata field of each node into a new type. The provided
    /// function receives the previous metadata of the node, and should return the corresponding new
    /// metadata.
    pub fn cast_meta<U, F>(self, transfer_func: &mut F) -> Node<'a, U>
    where
        F: FnMut(&Node<'a, T>) -> U,
    {
        macro_rules! cast_children {
            ($children_vec:expr, $transfer:expr) => {
                $children_vec
                    .into_iter()
                    .map(|node| node.cast_meta($transfer))
                    .collect()
            };
        }

        let new_meta = transfer_func(&self);

        // A bit verbose, but it's the only way to do a full type cast throughout the entire AST
        match self.node_type {
            NodeType::SpecialBlock(block) => Node {
                metadata: new_meta,
                node_type: NodeType::SpecialBlock(match block {
                    SpecialBlock::If(if_block) => SpecialBlock::If(IfBlock {
                        expr: if_block.expr,
                        inner: cast_children!(if_block.inner, transfer_func),
                        else_block: if_block
                            .else_block
                            .map(|nodes| cast_children!(nodes, transfer_func)),
                    }),
                    SpecialBlock::For(for_block) => SpecialBlock::For(ForBlock {
                        inner: cast_children!(for_block.inner, transfer_func),
                        binding: for_block.binding,
                        index: for_block.index,
                        expr: for_block.expr,
                    }),
                    SpecialBlock::Use(use_block) => SpecialBlock::Use(use_block),
                }),
            },
            NodeType::Element(elem) => Node {
                metadata: new_meta,
                node_type: NodeType::Element(Element {
                    tag: elem.tag,
                    attrs: elem.attrs,
                    children: cast_children!(elem.children, transfer_func),
                }),
            },
            NodeType::Text(text) => Node {
                metadata: new_meta,
                node_type: NodeType::Text(text),
            },
            NodeType::Comment(comment) => Node {
                metadata: new_meta,
                node_type: NodeType::Comment(comment),
            },
            NodeType::Mustache(syntax_node) => Node {
                metadata: new_meta,
                node_type: NodeType::Mustache(syntax_node),
            },
        }
    }
}

impl<'a, T> IfBlock<'a, T> {
    pub fn inner_recursive(&'a self) -> NodeIter<'a, T> {
        NodeIter::new(&self.inner)
    }

    pub fn else_recursive(&'a self) -> Option<NodeIter<'a, T>> {
        self.else_block
            .as_ref()
            .map(|else_block| NodeIter::new(else_block))
    }
}

impl<'a> DecorousAst<'a> {
    /// Creates a recursive iterator over the nodes of the template.
    pub fn iter_nodes(&'a self) -> NodeIter<'a, Location> {
        NodeIter::new(&self.nodes)
    }
}

pub fn traverse_with<'a, T, F, G>(nodes: &'a [Node<'a, T>], predicate: &mut F, body_func: &mut G)
where
    F: FnMut(&Element<'a, T>) -> bool,
    G: FnMut(&Node<'a, T>),
{
    for node in nodes {
        body_func(node);
        if let NodeType::Element(elem) = &node.node_type {
            if !predicate(elem) {
                continue;
            }
            traverse_with(&elem.children, predicate, body_func);
        }
    }
}

pub fn traverse_mut<'a, T, F>(nodes: &mut [Node<'a, T>], f: &mut F)
where
    F: FnMut(&mut Node<'a, T>),
{
    for node in nodes {
        f(node);
        if let NodeType::Element(elem) = &mut node.node_type {
            traverse_mut(&mut elem.children, f);
        }
    }
}

#[derive(Debug)]
pub struct NodeIter<'a, T> {
    stack: Vec<&'a Node<'a, T>>,
}

impl<'a, T> NodeIter<'a, T> {
    pub fn new(nodes: &'a [Node<'a, T>]) -> NodeIter<'a, T> {
        let mut stack = Vec::with_capacity(nodes.len());
        stack.extend(nodes.iter().rev());
        Self { stack }
    }
}

impl<'a, T> Iterator for NodeIter<'a, T> {
    type Item = &'a Node<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.stack.pop().map(|node| {
            if let NodeType::Element(elem) = &node.node_type {
                self.stack.extend(elem.children.iter().rev());
            }

            node
        })
    }
}

impl<'a, T> fmt::Display for Node<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.node_type {
            NodeType::Text(t) => write!(f, "{t}"),
            NodeType::Comment(Comment(c)) => write!(f, "<!--{c}-->"),
            NodeType::Element(elem) => write!(f, "{elem}"),
            NodeType::Mustache(js) => write!(f, "{{{js}}}"),
            NodeType::SpecialBlock(block) => write!(f, "{block}"),
        }
    }
}

impl<'a, T> fmt::Display for Element<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "<{}{}{}>{}</{0}>",
            self.tag,
            (!self.children.is_empty())
                .then_some(" ")
                .unwrap_or_default(),
            self.attrs.iter().join(" "),
            self.children
                .iter()
                .map(|elem| format!("  {elem}"))
                .join("")
        )
    }
}

impl<'a> fmt::Display for Attribute<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Attribute::KeyValue(key, Some(val)) => write!(f, "{key}={val}"),
            Attribute::KeyValue(key, None) => write!(f, "{key}"),
            Attribute::EventHandler(event_handler) => write!(f, "{event_handler}"),
            Attribute::Binding(binding) => write!(f, "bind:{binding}"),
        }
    }
}

impl<'a> fmt::Display for AttributeValue<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AttributeValue::Literal(literal) => write!(f, "\"{literal}\""),
            AttributeValue::JavaScript(js) => write!(f, "{{{js}}}"),
        }
    }
}

impl<'a> fmt::Display for EventHandler<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "on:{}={{{}}}", self.event, self.expr)
    }
}

impl<'a, T> fmt::Display for SpecialBlock<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpecialBlock::If(if_block) => write!(f, "{if_block}"),
            SpecialBlock::For(for_block) => write!(f, "{for_block}"),
            SpecialBlock::Use(use_block) => write!(f, "{use_block}"),
        }
    }
}

impl<'a, T> fmt::Display for IfBlock<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{#if {}}}\n{}\n{{/if}}",
            self.expr,
            self.inner.iter().map(|elem| format!("  {elem}")).join(""),
        )
    }
}

impl<'a, T> fmt::Display for ForBlock<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{#for {} in {}}}\n{}\n{{/for}}",
            self.index.map_or_else(
                || self.binding.to_owned(),
                |idx| format!("{idx}, {}", self.binding)
            ),
            self.expr,
            self.inner.iter().map(|elem| format!("  {elem}")).join(""),
        )
    }
}

impl<'a> fmt::Display for UseBlock<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{#use \"{}\"}}", self.path.display())
    }
}
