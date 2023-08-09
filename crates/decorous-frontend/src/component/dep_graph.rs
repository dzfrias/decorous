use std::collections::HashMap;

use petgraph::{graph::NodeIndex, Direction, Graph};
use rslint_parser::{ast::Decl, AstNode, SmolStr, SyntaxNode};
use smallvec::SmallVec;

use crate::utils;

/// A directed acyclic graph containing the variables declared in a script along
/// with their dependencies. This is used for optimizations.
#[derive(Debug, Clone, Default)]
pub struct DepGraph {
    // Directed graph from variable declarations to their dependents (NOT dependencies)
    graph: Graph<Declaration, ()>,
    var_lookup: HashMap<SmolStr, NodeIndex>,
}

#[derive(Debug, Clone)]
pub struct Declaration {
    pub decl: Decl,
    pub declared_vars: SmallVec<[SmolStr; 1]>,
    pub mutated: bool,
    pub used: bool,
}

impl DepGraph {
    pub fn new(decls: &[Decl]) -> Self {
        let mut graph = Graph::new();
        let mut var_lookup = HashMap::new();

        for decl in decls
            .iter()
            .filter(|decl| matches!(decl, Decl::FnDecl(_) | Decl::VarDecl(_)))
        {
            let node = Declaration {
                decl: decl.clone(),
                declared_vars: SmallVec::new(),
                mutated: false,
                used: false,
            };
            let idx = graph.add_node(node);
            match decl {
                Decl::VarDecl(var_decl) => {
                    for pat in var_decl.declared().filter_map(|d| d.pattern()) {
                        for ident in utils::get_idents_from_pattern(pat) {
                            var_lookup.insert(ident.clone(), idx);
                            graph[idx].declared_vars.push(ident);
                        }
                    }
                }
                Decl::FnDecl(fn_decl) => {
                    let name = fn_decl.name().unwrap();
                    let tok = name.ident_token().unwrap();
                    let ident = tok.text();
                    var_lookup.insert(ident.clone(), idx);
                    graph[idx].declared_vars.push(ident.clone());
                }
                _ => unreachable!(),
            }
        }

        let mut s = Self { graph, var_lookup };
        s.compute_edges();
        s
    }

    pub fn mark_used(&mut self, ident: &str) -> bool {
        let target = self.var_lookup.get(ident);
        let Some(target) = target else {
            return false;
        };
        self.mark_neighbors_used(*target);
        true
    }

    pub fn mark_used_from_node(&mut self, node: &SyntaxNode) {
        for unbound in utils::get_unbound_refs(node) {
            let tok = unbound.ident_token().unwrap();
            let ident = tok.text();
            self.mark_used(ident);
        }
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

    pub fn get_unused(&self) -> impl Iterator<Item = &Declaration> + '_ {
        self.graph.raw_nodes().iter().filter_map(|node| {
            if !node.weight.used {
                Some(&node.weight)
            } else {
                None
            }
        })
    }

    fn mark_neighbors_mutated(&mut self, target: NodeIndex) {
        self.graph[target].mutated = true;
        self.graph[target].used = true;
        let mut edges = self.graph.neighbors(target).detach();
        while let Some(i) = edges.next_node(&self.graph) {
            self.mark_neighbors_mutated(i);
        }
    }

    fn mark_neighbors_used(&mut self, target: NodeIndex) {
        self.graph[target].used = true;
        // Get edges that point to this node
        let mut edges = self
            .graph
            .neighbors_directed(target, Direction::Incoming)
            .detach();
        while let Some(i) = edges.next_node(&self.graph) {
            self.mark_neighbors_used(i);
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
