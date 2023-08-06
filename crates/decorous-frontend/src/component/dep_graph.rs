use std::collections::HashMap;

use rslint_parser::{ast::VarDecl, AstNode, SmolStr, SyntaxNode};

use crate::utils;

/// A directed acyclic graph containing the variables declared in a script along
/// with their dependencies. This is used for optimizations.
#[derive(Debug, Clone)]
pub struct DepGraph {
    vertices: Vec<Declaration>,
    graph: HashMap<VarDecl, Vec<VarDecl>>,
}

#[derive(Debug, Clone)]
pub struct Declaration {
    pub decl: VarDecl,
    pub declared_vars: Vec<SmolStr>,
    pub mutated: bool,
}

impl DepGraph {
    pub fn new(decls: &[VarDecl]) -> Self {
        let mut vertices = Vec::with_capacity(decls.len());
        let mut graph = HashMap::with_capacity(decls.len());

        for var_decl in decls {
            let mut all_declared = vec![];
            for pat in var_decl.declared().filter_map(|d| d.pattern()) {
                all_declared.extend(utils::get_idents_from_pattern(pat));
            }
            let edge = Declaration {
                declared_vars: all_declared,
                decl: var_decl.clone(),
                mutated: false,
            };
            vertices.push(edge);
            graph.insert(var_decl.clone(), vec![]);
        }

        let mut s = Self { vertices, graph };
        s.compute_edges();
        s
    }

    pub fn mark_mutated(&mut self, ident: &str) -> bool {
        let target = self
            .vertices
            .iter_mut()
            .find(|v| v.declared_vars.iter().any(|var| var.as_str() == ident));
        // Vertex not found, ident not in scope
        let Some(target) = target else {
            return false;
        };
        target.mutated = true;
        for dependent in self.graph.get_mut(&target.decl).unwrap() {
            // Get the corresponding vertex
            let dependent = self
                .vertices
                .iter_mut()
                .find(|v| &v.decl == dependent)
                .unwrap();
            dependent.mutated = true;
        }
        true
    }

    pub fn mark_mutated_from_node(&mut self, node: &SyntaxNode) {
        for unbound in utils::get_unbound_refs(node)
            .iter()
            .filter(|unbd| utils::is_from_assignment(unbd))
        {
            let tok = unbound.ident_token().unwrap();
            let ident = tok.text();
            self.mark_mutated(&ident);
        }
    }

    pub fn get_unmutated(&self) -> impl Iterator<Item = &Declaration> + '_ {
        self.vertices.iter().filter(|v| !v.mutated)
    }

    fn compute_edges(&mut self) {
        for v in &self.vertices {
            let deps = utils::get_unbound_refs(v.decl.syntax());
            for e in deps.into_iter().filter_map(|dep| {
                let tok = dep.ident_token().unwrap();
                let ident = tok.text();
                let origin = self
                    .vertices
                    .iter()
                    .find(|v| v.declared_vars.contains(ident))?;
                Some(origin.decl.clone())
            }) {
                self.graph.get_mut(&e).unwrap().push(v.decl.clone());
            }
        }
    }
}
