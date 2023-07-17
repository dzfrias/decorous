use std::collections::HashMap;

use decorous_frontend::{
    ast::{Attribute, AttributeValue, Node, NodeType},
    Component, FragmentMetadata,
};
use rslint_parser::SmolStr;

use crate::prerender::node_analyzer::NodeAnalyzer;

#[derive(Debug, Clone)]
pub struct IdOverwrites(HashMap<u32, SmolStr>);

impl IdOverwrites {
    pub fn try_get(&self, id: u32) -> SmolStr {
        self.0
            .get(&id)
            .cloned()
            .unwrap_or(SmolStr::new(id.to_string()))
    }
}

#[derive(Debug, Default)]
pub struct IdAnalyzer {
    overwritten: HashMap<u32, SmolStr>,
}

impl IdAnalyzer {
    pub fn new() -> Self {
        Self::default()
    }
}

impl NodeAnalyzer<'_> for IdAnalyzer {
    type AccumulatedOutput = IdOverwrites;

    fn visit(&mut self, node: &Node<'_, FragmentMetadata>, _component: &Component) {
        let NodeType::Element(elem) = node.node_type() else {
            return;
        };

        for attr in elem.attrs() {
            match attr {
                Attribute::KeyValue(key, Some(AttributeValue::Literal(literal)))
                    if *key == "id" =>
                {
                    self.overwritten
                        .insert(node.metadata().id(), SmolStr::new(literal));
                }
                _ => {}
            }
        }
    }

    fn accumulated_output(self) -> Self::AccumulatedOutput {
        IdOverwrites(self.overwritten)
    }
}
