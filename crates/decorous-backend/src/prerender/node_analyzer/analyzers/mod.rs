mod elems_analyzer;
mod id_analyzer;

use std::collections::HashMap;

use decorous_frontend::Component;
pub use elems_analyzer::*;
pub use id_analyzer::*;

use super::NodeAnalyzer;

#[derive(Debug, Clone)]
pub struct Analysis {
    id_overwrites: IdOverwrites,
    elem_data: HashMap<u32, ElementData>,
}

impl Analysis {
    pub fn analyze(component: &Component) -> Self {
        let mut id_analyzer = IdAnalyzer::new();
        let mut element_analyzer = ElemsAnalyzer::new();
        for node in component.descendents() {
            id_analyzer.visit(node, &component);
            element_analyzer.visit(node, &component);
        }

        Self {
            id_overwrites: id_analyzer.accumulated_output(),
            elem_data: element_analyzer.accumulated_output(),
        }
    }

    pub fn elem_data(&self) -> &HashMap<u32, ElementData> {
        &self.elem_data
    }

    pub fn id_overwrites(&self) -> &IdOverwrites {
        &self.id_overwrites
    }
}
