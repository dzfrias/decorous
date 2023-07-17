pub mod analyzers;

use decorous_frontend::{ast::Node, Component, FragmentMetadata};

pub trait NodeAnalyzer<'a> {
    type AccumulatedOutput;

    fn visit(&mut self, node: &'a Node<'_, FragmentMetadata>, component: &'a Component);
    fn accumulated_output(self) -> Self::AccumulatedOutput;
}
