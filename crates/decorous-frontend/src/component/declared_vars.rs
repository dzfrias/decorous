#[cfg(test)]
use serde::Serialize;
use std::{borrow::Borrow, collections::HashMap, hash::Hash};

use rslint_parser::{ast::ArrowExpr, SmolStr, SyntaxNode};

#[derive(Debug, Clone, Default)]
pub struct DeclaredVariables {
    vars: HashMap<SmolStr, u32>,
    arrow_exprs: HashMap<ArrowExpr, (u32, Option<u32>)>,
    bindings: HashMap<SmolStr, u32>,
    scopes: HashMap<u32, Scope>,
    css_mustaches: HashMap<SyntaxNode, u32>,
    reactive_blocks: HashMap<SyntaxNode, u32>,
    current_id: u32,
    css_current: u32,
}

impl DeclaredVariables {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_var(&mut self, var: SmolStr) {
        let id = self.generate_id();
        self.vars.insert(var, id);
    }

    pub fn insert_arrow_expr(&mut self, var: ArrowExpr, scope_id: Option<u32>) {
        let id = self.generate_id();
        self.arrow_exprs.insert(var, (id, scope_id));
    }

    pub fn insert_css_mustache(&mut self, node: SyntaxNode) {
        let id = self.generate_css_id();
        self.css_mustaches.insert(node, id);
    }

    pub fn insert_scope(&mut self, scope_id: u32, scope: Scope) {
        self.scopes.insert(scope_id, scope);
    }

    pub fn insert_binding(&mut self, name: SmolStr) {
        let id = self.generate_id();
        self.bindings.insert(name, id);
    }

    pub fn insert_reactive_block(&mut self, block: SyntaxNode) {
        let id = self.generate_id();
        self.reactive_blocks.insert(block, id);
    }

    pub fn get_var<K>(&self, var: &K, scope_id: Option<u32>) -> Option<u32>
    where
        SmolStr: Borrow<K>,
        K: Hash + Eq + ?Sized,
    {
        match self.vars.get(var) {
            Some(v) => Some(*v),
            None => self.scopes.get(&scope_id?)?.get(var),
        }
    }

    pub fn get_arrow_expr(&self, arrow_expr: &ArrowExpr) -> Option<(u32, Option<u32>)> {
        self.arrow_exprs.get(arrow_expr).copied()
    }

    pub fn get_binding<K>(&self, var: &K) -> Option<u32>
    where
        SmolStr: Borrow<K>,
        K: Hash + Eq + ?Sized,
    {
        self.bindings.get(var).cloned()
    }

    pub fn get_reactive_block(&self, block: &SyntaxNode) -> Option<u32> {
        self.reactive_blocks.get(block).cloned()
    }

    pub fn all_vars(&self) -> &HashMap<SmolStr, u32> {
        &self.vars
    }

    pub fn all_arrow_exprs(&self) -> &HashMap<ArrowExpr, (u32, Option<u32>)> {
        &self.arrow_exprs
    }

    pub fn all_scopes(&self) -> &HashMap<u32, Scope> {
        &self.scopes
    }

    pub fn all_bindings(&self) -> &HashMap<SmolStr, u32> {
        &self.bindings
    }

    pub fn all_reactive_blocks(&self) -> &HashMap<SyntaxNode, u32> {
        &self.reactive_blocks
    }

    pub fn is_scope_var<K>(&self, var: &K, scope_id: u32) -> bool
    where
        SmolStr: Borrow<K>,
        K: Hash + Eq + ?Sized,
    {
        let Some(scope) = self.all_scopes().get(&scope_id) else {
            return false;
        };
        scope.get(var).is_some()
    }

    pub fn len(&self) -> usize {
        self.vars.len()
            + self.arrow_exprs.len()
            + self.scopes.values().map(|s| s.env.len()).sum::<usize>()
            + self.bindings.len()
            + self.reactive_blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn remove_var<K>(&mut self, var: &K) -> bool
    where
        SmolStr: Borrow<K>,
        K: Hash + Eq + ?Sized,
    {
        if let Some(removed_id) = self.vars.remove(var) {
            for id in self
                .vars
                .values_mut()
                .chain(self.arrow_exprs.values_mut().map(|(id, _)| id))
                .chain(self.bindings.values_mut())
                .filter(|id| **id > removed_id)
            {
                *id -= 1;
            }
            true
        } else {
            false
        }
    }

    pub(crate) fn generate_id(&mut self) -> u32 {
        let old = self.current_id;
        self.current_id += 1;
        old
    }

    fn generate_css_id(&mut self) -> u32 {
        let old = self.css_current;
        self.css_current += 1;
        old
    }

    pub fn css_mustaches(&self) -> &HashMap<SyntaxNode, u32> {
        &self.css_mustaches
    }
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(test, derive(Serialize))]
pub struct Scope {
    env: HashMap<SmolStr, u32>,
}

impl Scope {
    pub fn new() -> Self {
        Self {
            env: HashMap::new(),
        }
    }

    pub fn get<K>(&self, k: &K) -> Option<u32>
    where
        SmolStr: Borrow<K>,
        K: Hash + Eq + ?Sized,
    {
        self.env.get(k).copied()
    }

    pub fn add(&mut self, k: SmolStr, id: u32) {
        self.env.insert(k, id);
    }
}
