use std::rc::Rc;

use decorous_frontend::{
    ast::{Attribute, AttributeValue, Mustache, Node, NodeType, SpecialBlock},
    Component, FragmentMetadata,
};
use rslint_parser::{SmolStr, SyntaxNode};

use crate::prerender::node_analyzer::NodeAnalyzer;

pub type IdVec<'a, T> = Vec<(&'a FragmentMetadata, T)>;
pub type IdSlice<'a, T> = &'a [(&'a FragmentMetadata, T)];

#[derive(Debug, Clone)]
pub struct ReactiveData<'ast> {
    mustaches: IdVec<'ast, SyntaxNode>,
    key_values: IdVec<'ast, Rc<[(SmolStr, SyntaxNode)]>>,
    event_listeners: IdVec<'ast, Rc<[(SmolStr, SyntaxNode)]>>,
    special_blocks: IdVec<'ast, &'ast SpecialBlock<'ast, FragmentMetadata>>,
}

#[derive(Debug)]
pub struct ReactivityAnalyzer<'ast> {
    reactive_data: ReactiveData<'ast>,
}

impl ReactivityAnalyzer<'_> {
    pub fn new() -> Self {
        Self {
            reactive_data: ReactiveData {
                mustaches: vec![],
                special_blocks: vec![],
                key_values: vec![],
                event_listeners: vec![],
            },
        }
    }
}

impl<'a> NodeAnalyzer<'a> for ReactivityAnalyzer<'a> {
    type AccumulatedOutput = ReactiveData<'a>;

    fn visit(&mut self, node: &'a Node<'_, FragmentMetadata>, _component: &'a Component) {
        match node.node_type() {
            NodeType::Element(elem) => {
                // PERF: small vec?
                let mut kvs = vec![];
                let mut listeners = vec![];
                for attr in elem.attrs() {
                    match attr {
                        Attribute::KeyValue(key, Some(AttributeValue::JavaScript(js))) => {
                            kvs.push((SmolStr::new(key), js.clone()));
                        }
                        Attribute::EventHandler(event_handler) => listeners.push((
                            SmolStr::new(event_handler.event()),
                            event_handler.expr().clone(),
                        )),

                        _ => {}
                    };
                }

                if !kvs.is_empty() {
                    self.reactive_data
                        .key_values
                        .push((node.metadata(), kvs.into()));
                }
                if !listeners.is_empty() {
                    self.reactive_data
                        .event_listeners
                        .push((node.metadata(), listeners.into()));
                }
            }
            NodeType::Mustache(Mustache(js)) => {
                self.reactive_data
                    .mustaches
                    .push((node.metadata(), js.clone()));
            }
            NodeType::SpecialBlock(block) => self
                .reactive_data
                .special_blocks
                .push((node.metadata(), block)),
            _ => {}
        };
    }

    fn accumulated_output(self) -> Self::AccumulatedOutput {
        self.reactive_data
    }
}

impl<'ast> ReactiveData<'ast> {
    pub fn mustaches(&self) -> IdSlice<SyntaxNode> {
        self.mustaches.as_ref()
    }

    pub fn special_blocks(&self) -> IdSlice<&SpecialBlock<'_, FragmentMetadata>> {
        self.special_blocks.as_ref()
    }

    pub fn key_values(&self) -> IdSlice<Rc<[(SmolStr, SyntaxNode)]>> {
        self.key_values.as_ref()
    }

    pub fn event_listeners(&self) -> IdSlice<Rc<[(SmolStr, SyntaxNode)]>> {
        self.event_listeners.as_ref()
    }

    pub fn flat_listeners(
        &self,
    ) -> impl Iterator<Item = (&FragmentMetadata, &SmolStr, &SyntaxNode)> + '_ {
        self.event_listeners()
            .iter()
            .flat_map(|(meta, listeners)| listeners.iter().map(move |(ev, expr)| (*meta, ev, expr)))
    }

    pub fn flat_kvs(
        &self,
    ) -> impl Iterator<Item = (&FragmentMetadata, &SmolStr, &SyntaxNode)> + '_ {
        self.key_values()
            .iter()
            .flat_map(|(meta, kvs)| kvs.iter().map(move |(k, expr)| (*meta, k, expr)))
    }
}
