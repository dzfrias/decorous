use std::rc::Rc;

use decorous_frontend::{
    ast::{Attribute, AttributeValue, Mustache, Node, NodeType, SpecialBlock},
    Component, FragmentMetadata,
};
use rslint_parser::{SmolStr, SyntaxNode};

use crate::prerender::node_analyzer::NodeAnalyzer;

#[derive(Debug, Clone)]
pub struct ReactiveData<'ast> {
    mustaches: Vec<(u32, SyntaxNode)>,
    key_values: Vec<(u32, Rc<[(SmolStr, SyntaxNode)]>)>,
    event_listeners: Vec<(u32, Rc<[(SmolStr, SyntaxNode)]>)>,
    special_blocks: Vec<(u32, &'ast SpecialBlock<'ast, FragmentMetadata>)>,
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
                        .push((node.metadata().id(), kvs.into()));
                }
                if !listeners.is_empty() {
                    self.reactive_data
                        .event_listeners
                        .push((node.metadata().id(), listeners.into()));
                }
            }
            NodeType::Mustache(Mustache(js)) => {
                self.reactive_data
                    .mustaches
                    .push((node.metadata().id(), js.clone()));
            }
            NodeType::SpecialBlock(block) => self
                .reactive_data
                .special_blocks
                .push((node.metadata().id(), block)),
            _ => {}
        };
    }

    fn accumulated_output(self) -> Self::AccumulatedOutput {
        self.reactive_data
    }
}

impl<'ast> ReactiveData<'ast> {
    pub fn mustaches(&self) -> &[(u32, SyntaxNode)] {
        self.mustaches.as_ref()
    }

    pub fn special_blocks(&self) -> &[(u32, &SpecialBlock<'_, FragmentMetadata>)] {
        self.special_blocks.as_ref()
    }

    pub fn key_values(&self) -> &[(u32, Rc<[(SmolStr, SyntaxNode)]>)] {
        self.key_values.as_ref()
    }

    pub fn event_listeners(&self) -> &[(u32, Rc<[(SmolStr, SyntaxNode)]>)] {
        self.event_listeners.as_ref()
    }

    pub fn flat_listeners(&self) -> impl Iterator<Item = (&u32, &SmolStr, &SyntaxNode)> + '_ {
        self.event_listeners()
            .iter()
            .map(|(id, listeners)| listeners.iter().map(move |(ev, expr)| (id, ev, expr)))
            .flatten()
    }

    pub fn flat_kvs(&self) -> impl Iterator<Item = (&u32, &SmolStr, &SyntaxNode)> + '_ {
        self.key_values()
            .iter()
            .map(|(id, kvs)| kvs.iter().map(move |(k, expr)| (id, k, expr)))
            .flatten()
    }
}
