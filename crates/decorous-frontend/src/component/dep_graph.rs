use std::collections::HashMap;

use petgraph::{graph::NodeIndex, Graph};
use rslint_parser::{ast::VarDecl, AstNode, SmolStr, SyntaxNode};
use smallvec::SmallVec;

use crate::utils;

/// A directed acyclic graph containing the variables declared in a script along
/// with their dependencies. This is used for optimizations.
#[derive(Debug, Clone)]
pub struct DepGraph {
    graph: Graph<Declaration, ()>,
    var_lookup: HashMap<SmolStr, NodeIndex>,
}

#[derive(Debug, Clone)]
pub struct Declaration {
    pub decl: VarDecl,
    pub declared_vars: SmallVec<[SmolStr; 1]>,
    pub mutated: bool,
}

impl DepGraph {
    pub fn new(decls: &[VarDecl]) -> Self {
        let mut graph = Graph::new();
        let mut var_lookup = HashMap::new();

        for var_decl in decls {
            let node = Declaration {
                decl: var_decl.clone(),
                declared_vars: SmallVec::new(),
                mutated: false,
            };
            let idx = graph.add_node(node);
            for pat in var_decl.declared().filter_map(|d| d.pattern()) {
                for ident in utils::get_idents_from_pattern(pat) {
                    var_lookup.insert(ident.clone(), idx);
                    graph[idx].declared_vars.push(ident);
                }
            }
        }

        let mut s = Self { graph, var_lookup };
        s.compute_edges();
        s
    }

    pub fn mark_mutated(&mut self, ident: &str) -> bool {
        let target = self.var_lookup.get(ident);
        let Some(target) = target else {
            return false;
        };
        self.mark_neighbors_mutated(*target);
        true
    }

    pub fn mark_mutated_from_node(&mut self, node: &SyntaxNode) {
        for unbound in utils::get_unbound_refs(node)
            .iter()
            .filter(|unbd| utils::is_from_assignment(unbd))
        {
            let tok = unbound.ident_token().unwrap();
            let ident = tok.text();
            self.mark_mutated(ident);
        }
    }

    pub fn get_unmutated(&self) -> impl Iterator<Item = &Declaration> + '_ {
        self.graph.raw_nodes().iter().filter_map(|node| {
            if !node.weight.mutated {
                Some(&node.weight)
            } else {
                None
            }
        })
    }

    fn mark_neighbors_mutated(&mut self, target: NodeIndex) {
        self.graph[target].mutated = true;
        let mut edges = self.graph.neighbors(target).detach();
        while let Some(i) = edges.next_node(&self.graph) {
            self.mark_neighbors_mutated(i);
        }
    }

    fn compute_edges(&mut self) {
        for i in self.graph.node_indices() {
            let decl = &self.graph[i];
            let deps = utils::get_unbound_refs(decl.decl.syntax());
            for dep in deps {
                let tok = dep.ident_token().unwrap();
                let ident = tok.text();
                let Some(origin) = self.var_lookup.get(ident) else {
                    continue;
                };
                self.graph.add_edge(*origin, i, ());
            }
        }
    }
}
