use std::fmt;

use itertools::Itertools;
use rslint_parser::SyntaxNode;

use crate::location::Location;

/// The collection of the three main parts of decorous syntax: the HTML-like template (`nodes`),
/// the script (`script`), and styling (`css`). The main way to obtain a `DecorousAst` is to use the
/// [`Parser`](super::Parser).
///
/// One of the best uses of `DecorousAst` is to hold data, read some parts, and eventually use
/// [`into_components()`](DecorousAst::into_components()) when the time has come to apply more
/// complex transformations that require ownership.
///
/// Note that by design, `DecorousAst` [`Node`]s only hold [`Location`]s for their metadata.
#[derive(Debug)]
pub struct DecorousAst<'a> {
    nodes: Vec<Node<'a, Location>>,
    script: Option<SyntaxNode>,
    css: Option<&'a str>,
}

/// A node of the AST. It contains [metadata](`Self::metadata()`) (of type `T`), and the
/// actual node data, retrieved by [`node_type()`](`Self::node_type()`). A node is commonly created
/// by the [parser](`super::Parser`), which produces an [abstract syntax tree](`DecorousAst`).
#[derive(Debug, Clone, PartialEq)]
pub struct Node<'a, T> {
    metadata: T,
    node_type: NodeType<'a, T>,
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
    tag: &'a str,
    attrs: Vec<Attribute<'a>>,
    children: Vec<Node<'a, T>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SpecialBlock<'a, T> {
    For(ForBlock<'a, T>),
    If(IfBlock<'a, T>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForBlock<'a, T> {
    binding: &'a str,
    index: Option<&'a str>,
    expr: SyntaxNode,
    inner: Vec<Node<'a, T>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfBlock<'a, T> {
    expr: SyntaxNode,
    inner: Vec<Node<'a, T>>,
    else_block: Option<Vec<Node<'a, T>>>,
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

#[derive(Debug, Clone, PartialEq)]
pub enum CollapsedChildrenType<'a> {
    Text(&'a str),
    Html(String),
}

impl<'a, T> Element<'a, T> {
    pub fn new(tag: &'a str, attrs: Vec<Attribute<'a>>, children: Vec<Node<'a, T>>) -> Self {
        Self {
            tag,
            attrs,
            children,
        }
    }

    pub fn tag(&self) -> &str {
        self.tag
    }

    pub fn children(&self) -> &[Node<'_, T>] {
        self.children.as_ref()
    }

    pub fn attrs(&self) -> &[Attribute<'_>] {
        self.attrs.as_ref()
    }

    pub fn children_mut(&mut self) -> &mut Vec<Node<'a, T>> {
        &mut self.children
    }

    pub fn descendents(&'a self) -> NodeIter<'a, T> {
        NodeIter::new(self.children())
    }

    /// Attempts to collapse the Element. This is useful for optimization, as `<span>Text</span>`
    /// can be represented in the DOM as `elem.textContent = "Text"` instead of as a tree. The same
    /// goes for regular html, as it can be rendered with `elem.innerHtml`. If the element cannot
    /// be collapsed, this method returns [`None`].
    ///
    /// [`CollapsedChildrenType`] denotes the type of the collapse.
    pub fn inner_collapsed(&self) -> Option<CollapsedChildrenType<'_>> {
        if self.children().len() == 1 {
            if let NodeType::Text(t) = *self.children().first().unwrap().node_type() {
                return Some(CollapsedChildrenType::Text(&t));
            }
        }

        // Test if the inner descendants are normal HTML (no mustaches or special blocks)
        if !self.children().is_empty()
            && self.descendents().all(|node| match node.node_type() {
                NodeType::Text(_) | NodeType::Comment(_) => true,
                // For elements, check if any attributes have mustache tags
                NodeType::Element(elem) => elem.attrs().iter().all(|attr| match attr {
                    Attribute::KeyValue(_, None) => true,
                    Attribute::KeyValue(_, Some(val)) => matches!(val, AttributeValue::Literal(_)),
                    Attribute::Binding(_) | Attribute::EventHandler(_) => false,
                }),
                NodeType::Mustache(_) | NodeType::SpecialBlock(_) => false,
            })
        {
            return Some(CollapsedChildrenType::Html(self.children().iter().join("")));
        }

        None
    }

    pub fn has_immediate_mustache(&self) -> bool {
        self.children()
            .iter()
            .any(|child| matches!(child.node_type(), NodeType::Mustache(_)))
    }
}

impl<'a, T> Node<'a, T> {
    /// Create a new node with some metadata. [`NodeType`] contains actual data about the node,
    /// such as its children (if it's an element) or the underlying expression (if it's in a
    /// mustache `{}` tag).
    ///
    /// ```
    /// # use decorous_frontend::ast::*;
    /// // A new text node with no metadata.
    /// let node = Node::new(NodeType::Text(Text("hello")), ());
    /// assert_eq!(&NodeType::Text(Text("hello")), node.node_type());
    /// ```
    pub fn new(ty: NodeType<'a, T>, metadata: T) -> Self {
        Self {
            metadata,
            node_type: ty,
        }
    }

    /// Obtain an shared reference to the node's type. See [`NodeType`] for more.
    pub fn node_type(&self) -> &NodeType<'a, T> {
        &self.node_type
    }

    /// Obtain an exclusive reference to the node's type. See [`NodeType`] for more.
    pub fn node_type_mut(&mut self) -> &mut NodeType<'a, T> {
        &mut self.node_type
    }

    /// Obtain a shared reference to the metadata of the node.
    pub fn metadata(&self) -> &T {
        &self.metadata
    }

    /// Obtain an exclusive reference to the metadata of the node.
    pub fn metadata_mut(&mut self) -> &mut T {
        &mut self.metadata
    }

    /// Recursively cast each the metadata field of each node into a new type. The provided
    /// function receives the previous metadata of the node, and should return the corresponding new
    /// metadata.
    ///
    /// ```
    /// # use decorous_frontend::ast::*;
    /// // Some random metadata
    /// let node = Node::new(NodeType::Text(Text("hello")), 11);
    /// assert_eq!(Node::new(NodeType::Text(Text("hello")), true), node.cast_meta(&mut |node| *node.metadata() > 10));
    /// ```
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

impl<'a, T> ForBlock<'a, T> {
    pub fn new(
        binding: &'a str,
        index: Option<&'a str>,
        expr: SyntaxNode,
        inner: Vec<Node<'a, T>>,
    ) -> Self {
        Self {
            binding,
            index,
            expr,
            inner,
        }
    }

    pub fn binding(&self) -> &str {
        self.binding
    }

    pub fn index(&self) -> Option<&str> {
        self.index
    }

    pub fn expr(&self) -> &SyntaxNode {
        &self.expr
    }

    pub fn inner(&self) -> &[Node<'_, T>] {
        self.inner.as_ref()
    }

    pub fn inner_mut(&mut self) -> &mut Vec<Node<'a, T>> {
        &mut self.inner
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

impl<'a, T> IfBlock<'a, T> {
    pub fn new(
        expr: SyntaxNode,
        inner: Vec<Node<'a, T>>,
        else_block: Option<Vec<Node<'a, T>>>,
    ) -> Self {
        Self {
            expr,
            inner,
            else_block,
        }
    }

    pub fn inner_recursive(&'a self) -> NodeIter<'a, T> {
        NodeIter::new(self.inner())
    }

    pub fn else_recursive(&'a self) -> Option<NodeIter<'a, T>> {
        self.else_block()
            .map(|else_block| NodeIter::new(else_block))
    }

    pub fn expr(&self) -> &SyntaxNode {
        &self.expr
    }

    pub fn inner(&self) -> &[Node<'a, T>] {
        self.inner.as_ref()
    }

    pub fn else_block(&self) -> Option<&[Node<'a, T>]> {
        self.else_block.as_deref()
    }

    pub fn inner_mut(&mut self) -> &mut Vec<Node<'a, T>> {
        &mut self.inner
    }

    pub fn else_block_mut(&mut self) -> Option<&mut [Node<'a, T>]> {
        self.else_block.as_deref_mut()
    }
}

impl<'a> DecorousAst<'a> {
    /// Create a new `DecorousAst`. Note that this is usually not something done by hand, as ASTs
    /// are usually produced by the [`Parser`](super::Parser). The components of the three passed
    /// in arguments can be retrieved with [`nodes()`](`Self::nodes()`), [`script()`](`Self::script`),
    /// and [`css()`](`Self::css()`).
    pub fn new(
        nodes: Vec<Node<'a, Location>>,
        script: Option<SyntaxNode>,
        css: Option<&'a str>,
    ) -> Self {
        Self { nodes, script, css }
    }

    /// Obtain a shared reference to the template AST.
    pub fn nodes(&self) -> &[Node<'_, Location>] {
        self.nodes.as_ref()
    }

    /// Gives a shared reference to the JavaScript AST, if it exists. The `DecorousAst` will have
    /// no JavaScript AST if no `<script>` tag is specified in the template.
    pub fn script(&self) -> Option<&SyntaxNode> {
        self.script.as_ref()
    }

    /// Gives a a shared reference to the CSS AST, if it exists. The `DecorousAst` will have
    /// no CSS AST if no `<style>` tag is specified in the template.
    pub fn css(&self) -> Option<&'a str> {
        self.css
    }

    /// Creates a recursive iterator over the nodes of the template.
    pub fn iter_nodes(&'a self) -> NodeIter<'a, Location> {
        NodeIter::new(self.nodes())
    }

    pub fn into_components(self) -> (Vec<Node<'a, Location>>, Option<SyntaxNode>, Option<&'a str>) {
        (self.nodes, self.script, self.css)
    }
}

pub fn traverse_with<'a, T, F, G>(nodes: &'a [Node<'a, T>], predicate: &mut F, body_func: &mut G)
where
    F: FnMut(&'a Element<'a, T>) -> bool,
    G: FnMut(&'a Node<'a, T>),
{
    for node in nodes {
        body_func(node);
        if let NodeType::Element(elem) = node.node_type() {
            if !predicate(elem) {
                continue;
            }
            traverse_with(elem.children(), predicate, body_func);
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
            if let NodeType::Element(elem) = node.node_type() {
                self.stack.extend(elem.children().iter().rev());
            }

            node
        })
    }
}

impl<'a, T> fmt::Display for Node<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.node_type() {
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
            "<{}{}>{}</{0}>",
            self.tag(),
            self.attrs().iter().join(" "),
            self.children()
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
        write!(f, "on:{}={{{}}}", self.event(), self.expr())
    }
}

impl<'a, T> fmt::Display for SpecialBlock<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpecialBlock::If(if_block) => write!(f, "{if_block}"),
            SpecialBlock::For(for_block) => write!(f, "{for_block}"),
        }
    }
}

impl<'a, T> fmt::Display for IfBlock<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{#if {}}}\n{}\n{{/if}}",
            self.expr(),
            self.inner().iter().map(|elem| format!("  {elem}")).join(""),
        )
    }
}

impl<'a, T> fmt::Display for ForBlock<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{#for {} in {}}}\n{}\n{{/for}}",
            self.index().map_or_else(
                || self.binding().to_owned(),
                |idx| format!("{idx}, {}", self.binding)
            ),
            self.expr(),
            self.inner().iter().map(|elem| format!("  {elem}")).join(""),
        )
    }
}
