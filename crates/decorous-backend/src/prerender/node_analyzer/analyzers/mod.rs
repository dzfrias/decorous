mod hoist_analyzer;
mod id_analyzer;
mod reactivity_analyzer;

use std::collections::HashMap;

use decorous_frontend::Component;
pub use hoist_analyzer::*;
pub use id_analyzer::*;
pub use reactivity_analyzer::*;

use super::NodeAnalyzer;

#[derive(Debug, Clone)]
pub struct Analysis<'a> {
    id_overwrites: IdOverwrites,
    elem_data: HashMap<u32, ReactiveData<'a>>,
    hoistables: Hoistables<'a>,
}

impl<'a> Analysis<'a> {
    pub fn analyze(component: &'a Component) -> Self {
        let mut id_analyzer = IdAnalyzer::new();
        let mut element_analyzer = ReactivityAnalyzer::new();
        let mut hoist_analyzer = HoistAnalyzer::new();
        for node in component.descendents() {
            id_analyzer.visit(node, component);
            element_analyzer.visit(node, component);
            hoist_analyzer.visit(node, component);
        }

        Self {
            id_overwrites: id_analyzer.accumulated_output(),
            elem_data: element_analyzer.accumulated_output(),
            hoistables: hoist_analyzer.accumulated_output(),
        }
    }

    pub fn reactive_data(&self) -> &HashMap<u32, ReactiveData> {
        &self.elem_data
    }

    pub fn id_overwrites(&self) -> &IdOverwrites {
        &self.id_overwrites
    }

    pub fn hoistables(&self) -> &Hoistables<'a> {
        &self.hoistables
    }
}
