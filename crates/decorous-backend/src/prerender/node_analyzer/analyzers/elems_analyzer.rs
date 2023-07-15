use std::{collections::HashMap, rc::Rc};

use decorous_frontend::{
    ast::{Attribute, AttributeValue, Mustache, Node, NodeType},
    Component, FragmentMetadata,
};
use rslint_parser::{SmolStr, SyntaxNode};

use crate::prerender::node_analyzer::NodeAnalyzer;

#[derive(Debug, PartialEq, Clone, Hash)]
pub enum ElementAttribute {
    KeyValue(SmolStr, SyntaxNode),
    EventListener(SmolStr, SyntaxNode),
}

#[derive(Debug, PartialEq, Clone, Hash)]
pub enum ElementData {
    Mustache(SyntaxNode),
    AttributeCollection(Rc<[ElementAttribute]>),
}

#[derive(Debug, Default)]
pub struct ElemsAnalyzer {
    elems: HashMap<u32, ElementData>,
}

impl ElemsAnalyzer {
    pub fn new() -> Self {
        Self::default()
    }
}

impl NodeAnalyzer for ElemsAnalyzer {
    type AccumulatedOutput = HashMap<u32, ElementData>;

    fn visit(&mut self, node: &Node<'_, FragmentMetadata>, _component: &Component) {
        match node.node_type() {
            NodeType::Element(elem) => {
                // PERF: small vec?
                let mut attrs = vec![];
                for attr in elem.attrs() {
                    let data = match attr {
                        Attribute::KeyValue(key, Some(AttributeValue::JavaScript(js))) => {
                            ElementAttribute::KeyValue(SmolStr::new(key), js.clone())
                        }
                        Attribute::EventHandler(event_handler) => ElementAttribute::EventListener(
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
                self.elems.insert(
                    node.metadata().id(),
                    ElementData::AttributeCollection(attrs.into()),
                );
            }
            NodeType::Mustache(Mustache(js)) => {
                self.elems
                    .insert(node.metadata().id(), ElementData::Mustache(js.clone()));
            }
            _ => {}
        }
    }

    fn accumulated_output(self) -> Self::AccumulatedOutput {
        self.elems
    }
}
