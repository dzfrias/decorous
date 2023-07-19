#[cfg(test)]
use serde::Serialize;
use std::{borrow::Borrow, collections::HashMap, hash::Hash};

use rslint_parser::{ast::ArrowExpr, SmolStr};

#[derive(Debug, Clone, Default)]
pub struct DeclaredVariables {
    vars: HashMap<SmolStr, u32>,
    arrow_exprs: HashMap<ArrowExpr, (u32, Option<u32>)>,
    scopes: HashMap<u32, Scope>,
    current_id: u32,
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

    pub fn insert_scope(&mut self, scope_id: u32, scope: Scope) {
        self.scopes.insert(scope_id, scope);
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

    pub fn all_vars(&self) -> &HashMap<SmolStr, u32> {
        &self.vars
    }

    pub fn all_arrow_exprs(&self) -> &HashMap<ArrowExpr, (u32, Option<u32>)> {
        &self.arrow_exprs
    }

    pub fn all_scopes(&self) -> &HashMap<u32, Scope> {
        &self.scopes
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
            + self.scopes.iter().map(|(_, s)| s.env.len()).sum::<usize>()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn generate_id(&mut self) -> u32 {
        let old = self.current_id;
        self.current_id += 1;
        old
    }
}

#[derive(Debug, Clone)]
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
