mod id_analyzer;
mod reactivity_analyzer;

use std::collections::HashMap;

use decorous_frontend::Component;
pub use id_analyzer::*;
pub use reactivity_analyzer::*;

use super::NodeAnalyzer;

#[derive(Debug, Clone)]
pub struct Analysis {
    id_overwrites: IdOverwrites,
    elem_data: HashMap<u32, ReactiveData>,
}

impl Analysis {
    pub fn analyze(component: &Component) -> Self {
        let mut id_analyzer = IdAnalyzer::new();
        let mut element_analyzer = ReactivityAnalyzer::new();
        for node in component.descendents() {
            id_analyzer.visit(node, &component);
            element_analyzer.visit(node, &component);
        }

        Self {
            id_overwrites: id_analyzer.accumulated_output(),
            elem_data: element_analyzer.accumulated_output(),
        }
    }

    pub fn reactive_data(&self) -> &HashMap<u32, ReactiveData> {
        &self.elem_data
    }

    pub fn id_overwrites(&self) -> &IdOverwrites {
        &self.id_overwrites
    }
}
