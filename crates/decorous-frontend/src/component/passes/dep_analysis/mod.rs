mod dep_graph;

use decorous_errors::{DiagnosticBuilder, Severity};
use itertools::Itertools;
use rslint_parser::{ast::Decl, AstNode, SyntaxNodeExt};

use super::Pass;
use crate::{
    ast::{Attribute, AttributeValue, NodeType, SpecialBlock},
    component::globals::GLOBALS,
    Component,
};
use dep_graph::DepGraph;

#[derive(Debug)]
pub struct DepAnalysisPass;

impl DepAnalysisPass {
    pub fn new() -> Self {
        Self
    }
}

impl Pass for DepAnalysisPass {
    fn run(self, component: &mut Component) {
        let mut graph = DepGraph::new(
            &component
                .toplevel_nodes()
                .iter()
                .filter_map(|toplevel| toplevel.node.try_to::<Decl>())
                .collect_vec(),
        );

        for node in component.descendents() {
            match &node.node_type {
                NodeType::Element(elem) => {
                    for attr in &elem.attrs {
                        match attr {
                            Attribute::Binding(binding) => {
                                // Bindings are mutable
                                graph.mark_mutated(binding);
                            }
                            Attribute::EventHandler(evt_handler) => {
                                graph.mark_used_from_node(&evt_handler.expr);
                                graph.mark_mutated_from_node(&evt_handler.expr);
                            }
                            Attribute::KeyValue(_, Some(AttributeValue::JavaScript(js))) => {
                                graph.mark_used_from_node(js);
                                graph.mark_mutated_from_node(js);
                            }
                            Attribute::KeyValue(_, _) => {}
                        }
                    }
                }
                NodeType::SpecialBlock(SpecialBlock::If(block)) => {
                    graph.mark_used_from_node(&block.expr);
                    graph.mark_mutated_from_node(&block.expr);
                }
                NodeType::SpecialBlock(SpecialBlock::For(block)) => {
                    graph.mark_used_from_node(&block.expr);
                    graph.mark_mutated_from_node(&block.expr);
                }
                NodeType::Mustache(js) => {
                    graph.mark_used_from_node(js);
                    graph.mark_mutated_from_node(js);
                }
                NodeType::Text(_)
                | NodeType::Comment(_)
                | NodeType::SpecialBlock(SpecialBlock::Use(_)) => {}
            }
        }

        for mustache in component.declared_vars().css_mustaches().keys() {
            graph.mark_used_from_node(mustache);
            graph.mark_mutated_from_node(mustache);
        }

        for toplevel in component.toplevel_nodes() {
            graph.mark_mutated_from_node(&toplevel.node);
        }

        for v in graph.get_unused() {
            for var in &v.declared_vars {
                component.declared_vars.remove_var(var);
            }
            let pos = component
                .toplevel_nodes()
                .iter()
                .position(|node| &node.node == v.decl.syntax())
                .expect("VarDecl should be in toplevel nodes");
            component.toplevel_nodes.remove(pos);
        }

        for v in graph.get_unmutated() {
            for var in &v.declared_vars {
                component.declared_vars.remove_var(var);
            }
            let Some(pos) = component
                .toplevel_nodes()
                .iter()
                .position(|node| &node.node == v.decl.syntax())
            else {
                continue;
            };
            component.toplevel_nodes.remove(pos);
            component.hoist.push(v.decl.syntax().clone());
        }

        for unbound in graph
            .get_unbound()
            .iter()
            .filter(|v| !GLOBALS.contains(&v.as_str()))
        {
            component.errs.emit(
                DiagnosticBuilder::new(format!("possibly unbound variable: {unbound}"), 0)
                    .severity(Severity::Warning)
                    .build(),
            );
        }
    }
}
