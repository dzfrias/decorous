mod id_analyzer;
mod reactivity_analyzer;

use decorous_frontend::Component;
pub use id_analyzer::*;
pub use reactivity_analyzer::*;

use super::NodeAnalyzer;

#[derive(Debug, Clone)]
pub struct Analysis<'a> {
    id_overwrites: IdOverwrites,
    elem_data: ReactiveData<'a>,
}

impl<'a> Analysis<'a> {
    pub fn analyze(component: &'a Component) -> Self {
        let mut id_analyzer = IdAnalyzer::new();
        let mut element_analyzer = ReactivityAnalyzer::new();
        for node in component.descendents() {
            id_analyzer.visit(node, component);
            element_analyzer.visit(node, component);
        }

        Self {
            id_overwrites: id_analyzer.accumulated_output(),
            elem_data: element_analyzer.accumulated_output(),
        }
    }

    pub fn reactive_data(&self) -> &ReactiveData<'a> {
        &self.elem_data
    }

    pub fn id_overwrites(&self) -> &IdOverwrites {
        &self.id_overwrites
    }
}
