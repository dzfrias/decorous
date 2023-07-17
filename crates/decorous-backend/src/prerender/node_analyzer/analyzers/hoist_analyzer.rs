use decorous_frontend::{
    ast::{IfBlock, Node, NodeType, SpecialBlock},
    Component, FragmentMetadata,
};

use crate::prerender::node_analyzer::NodeAnalyzer;

#[derive(Debug, Clone, Default)]
pub struct Hoistables<'ast> {
    if_blocks: Vec<(u32, &'ast IfBlock<'ast, FragmentMetadata>)>,
}

#[derive(Debug, Clone, Default)]
pub struct HoistAnalyzer<'ast> {
    hoistables: Hoistables<'ast>,
}
impl HoistAnalyzer<'_> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<'ast> Hoistables<'ast> {
    pub fn if_blocks(&self) -> &[(u32, &IfBlock<'_, FragmentMetadata>)] {
        self.if_blocks.as_ref()
    }
}

impl<'ast> NodeAnalyzer<'ast> for HoistAnalyzer<'ast> {
    type AccumulatedOutput = Hoistables<'ast>;

    fn visit(&mut self, node: &'ast Node<'_, FragmentMetadata>, _component: &'ast Component) {
        if let NodeType::SpecialBlock(SpecialBlock::If(if_block)) = node.node_type() {
            self.hoistables
                .if_blocks
                .push((node.metadata().id(), if_block));
        }
    }

    fn accumulated_output(self) -> Self::AccumulatedOutput {
        self.hoistables
    }
}
