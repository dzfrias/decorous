pub mod analyzers;

use decorous_frontend::{ast::Node, Component, FragmentMetadata};

pub trait NodeAnalyzer {
    type AccumulatedOutput;

    fn visit(&mut self, node: &Node<'_, FragmentMetadata>, component: &Component);
    fn accumulated_output(self) -> Self::AccumulatedOutput;
}
