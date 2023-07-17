use std::{collections::HashMap, rc::Rc};

use decorous_frontend::{
    ast::{Attribute, AttributeValue, Mustache, Node, NodeType, SpecialBlock},
    Component, FragmentMetadata,
};
use rslint_parser::{SmolStr, SyntaxNode};

use crate::prerender::node_analyzer::NodeAnalyzer;

#[derive(Debug, PartialEq, Clone, Hash)]
pub enum ReactiveAttribute {
    KeyValue(SmolStr, SyntaxNode),
    EventListener(SmolStr, SyntaxNode),
}

#[derive(Debug, Clone)]
pub enum ReactiveData<'ast> {
    Mustache(SyntaxNode),
    AttributeCollection(Rc<[ReactiveAttribute]>),
    SpecialBlock(&'ast SpecialBlock<'ast, FragmentMetadata>),
}

#[derive(Debug, Default)]
pub struct ReactivityAnalyzer<'a> {
    elems: HashMap<u32, ReactiveData<'a>>,
}

impl ReactivityAnalyzer<'_> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<'a> NodeAnalyzer<'a> for ReactivityAnalyzer<'a> {
    type AccumulatedOutput = HashMap<u32, ReactiveData<'a>>;

    fn visit(&mut self, node: &'a Node<'_, FragmentMetadata>, _component: &'a Component) {
        let data = match node.node_type() {
            NodeType::Element(elem) => {
                // PERF: small vec?
                let mut attrs = vec![];
                for attr in elem.attrs() {
                    let data = match attr {
                        Attribute::KeyValue(key, Some(AttributeValue::JavaScript(js))) => {
                            ReactiveAttribute::KeyValue(SmolStr::new(key), js.clone())
                        }
                        Attribute::EventHandler(event_handler) => ReactiveAttribute::EventListener(
                            SmolStr::new(event_handler.event()),
                            event_handler.expr().clone(),
                        ),
                        _ => continue,
                    };
                    attrs.push(data);
                }

                if attrs.is_empty() {
                    return;
                }
                ReactiveData::AttributeCollection(attrs.into())
            }
            NodeType::Mustache(Mustache(js)) => ReactiveData::Mustache(js.clone()),
            NodeType::SpecialBlock(block) => ReactiveData::SpecialBlock(block),
            _ => return,
        };

        self.elems.insert(node.metadata().id(), data);
    }

    fn accumulated_output(self) -> Self::AccumulatedOutput {
        self.elems
    }
}
