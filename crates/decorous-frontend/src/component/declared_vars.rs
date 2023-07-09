use std::{borrow::Borrow, collections::HashMap, hash::Hash};

use rslint_parser::{ast::ArrowExpr, SmolStr};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DeclaredVariables {
    vars: HashMap<SmolStr, u32>,
    arrow_exprs: HashMap<ArrowExpr, u32>,
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

    pub fn insert_arrow_expr(&mut self, var: ArrowExpr) {
        let id = self.generate_id();
        self.arrow_exprs.insert(var, id);
    }

    pub fn get_var<K>(&self, var: &K) -> Option<u32>
    where
        SmolStr: Borrow<K>,
        K: Hash + Eq + ?Sized,
    {
        self.vars.get(var).copied()
    }

    pub fn get_arrow_expr(&self, arrow_expr: &ArrowExpr) -> Option<u32> {
        self.arrow_exprs.get(arrow_expr).copied()
    }

    pub fn all_vars(&self) -> &HashMap<SmolStr, u32> {
        &self.vars
    }

    pub fn all_arrow_exprs(&self) -> &HashMap<ArrowExpr, u32> {
        &self.arrow_exprs
    }

    pub fn len(&self) -> usize {
        self.vars.len() + self.arrow_exprs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn generate_id(&mut self) -> u32 {
        let old = self.current_id;
        self.current_id += 1;
        old
    }
}
