use rslint_parser::SyntaxNode;

#[derive(Debug)]
pub struct DecorousAst<'a> {
    nodes: Vec<Node<'a, Location>>,
    script: Option<SyntaxNode>,
    css: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node<'a, T> {
    metadata: T,
    node_type: NodeType<'a, T>,
}

#[derive(Debug, PartialEq, Eq, Clone, Hash, Copy)]
pub struct Location {
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeType<'a, T> {
    Element(Element<'a, T>),
    Text(&'a str),
    Comment(&'a str),
    SpecialBlock(SpecialBlock<'a, T>),
    Mustache(SyntaxNode),
    Error,
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
    // TODO: Implement
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
}

impl<'a, T> Node<'a, T> {
    pub fn new(ty: NodeType<'a, T>, metadata: T) -> Self {
        Self {
            metadata,
            node_type: ty,
        }
    }

    pub fn node_type(&self) -> &NodeType<'a, T> {
        &self.node_type
    }

    pub fn node_type_mut(&mut self) -> &mut NodeType<'a, T> {
        &mut self.node_type
    }

    pub fn metadata(&self) -> &T {
        &self.metadata
    }

    pub fn metadata_mut(&mut self) -> &mut T {
        &mut self.metadata
    }

    pub fn error(metadata: T) -> Self {
        Self::new(NodeType::Error, metadata)
    }

    pub fn cast_meta<U, F>(self, transfer_func: &mut F) -> Node<'a, U>
    where
        F: FnMut(T) -> U,
    {
        match self.node_type {
            NodeType::SpecialBlock(block) => Node {
                metadata: transfer_func(self.metadata),
                node_type: NodeType::SpecialBlock(match block {
                    SpecialBlock::If(if_block) => SpecialBlock::If(IfBlock {
                        expr: if_block.expr,
                        inner: if_block
                            .inner
                            .into_iter()
                            .map(|node| node.cast_meta(transfer_func))
                            .collect(),
                        else_block: if_block.else_block.map(|nodes| {
                            nodes
                                .into_iter()
                                .map(|node| node.cast_meta(transfer_func))
                                .collect()
                        }),
                    }),
                    SpecialBlock::For(for_block) => SpecialBlock::For(ForBlock {
                        inner: for_block
                            .inner
                            .into_iter()
                            .map(|node| node.cast_meta(transfer_func))
                            .collect(),
                        binding: for_block.binding,
                        index: for_block.index,
                        expr: for_block.expr,
                    }),
                }),
            },
            NodeType::Element(elem) => Node {
                metadata: transfer_func(self.metadata),
                node_type: NodeType::Element(Element {
                    tag: elem.tag,
                    attrs: elem.attrs,
                    children: elem
                        .children
                        .into_iter()
                        .map(|child| child.cast_meta(transfer_func))
                        .collect(),
                }),
            },
            NodeType::Text(text) => Node {
                metadata: transfer_func(self.metadata),
                node_type: NodeType::Text(text),
            },
            NodeType::Comment(comment) => Node {
                metadata: transfer_func(self.metadata),
                node_type: NodeType::Comment(comment),
            },
            NodeType::Mustache(syntax_node) => Node {
                metadata: transfer_func(self.metadata),
                node_type: NodeType::Mustache(syntax_node),
            },
            NodeType::Error => Node {
                metadata: transfer_func(self.metadata),
                node_type: NodeType::Error,
            },
        }
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

    pub fn start(&self) -> usize {
        self.start
    }

    pub fn end(&self) -> usize {
        self.end
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

    pub fn expr(&self) -> &SyntaxNode {
        &self.expr
    }

    pub fn inner(&self) -> &[Node<'a, T>] {
        self.inner.as_ref()
    }

    pub fn else_block(&self) -> Option<&[Node<'a, T>]> {
        self.else_block.as_ref().map(|block| block.as_slice())
    }

    pub fn inner_mut(&mut self) -> &mut Vec<Node<'a, T>> {
        &mut self.inner
    }
}

impl<'a> DecorousAst<'a> {
    pub fn new(
        nodes: Vec<Node<'a, Location>>,
        script: Option<SyntaxNode>,
        css: Option<&'a str>,
    ) -> Self {
        Self { nodes, script, css }
    }

    pub fn nodes(&self) -> &[Node<'_, Location>] {
        self.nodes.as_ref()
    }

    pub fn script(&self) -> Option<&SyntaxNode> {
        self.script.as_ref()
    }

    pub fn css(&self) -> Option<&'a str> {
        self.css
    }

    pub fn iter_nodes(&'a self) -> NodeIter<'a, Location> {
        let nodes = self.nodes().iter().collect::<Vec<&'a Node<'a, Location>>>();
        NodeIter::new(nodes)
    }

    pub fn into_components(self) -> (Vec<Node<'a, Location>>, Option<SyntaxNode>, Option<&'a str>) {
        (self.nodes, self.script, self.css)
    }
}

#[derive(Debug)]
pub struct NodeIter<'a, T> {
    stack: Vec<&'a Node<'a, T>>,
}

impl<'a, T> NodeIter<'a, T> {
    fn new(node: Vec<&'a Node<'a, T>>) -> NodeIter<'a, T> {
        let mut stack = Vec::new();
        stack.extend(node);
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
