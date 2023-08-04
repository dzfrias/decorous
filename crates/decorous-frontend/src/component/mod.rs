mod declared_vars;
mod fragment;

use std::{borrow::Cow, collections::HashSet};

#[cfg(not(debug_assertions))]
use rand::Rng;
use rslint_parser::{
    ast::{ArrowExpr, Decl, ExportDecl, FnDecl, ImportDecl, VarDecl},
    AstNode, SmolStr, SyntaxNode, SyntaxNodeExt,
};

use crate::{
    ast::{
        traverse_mut, Attribute, AttributeValue, Code, DecorousAst, Node, NodeIter, NodeType,
        SpecialBlock,
    },
    css::{self, ast::Css},
    location::Location,
    utils,
};
pub use declared_vars::{DeclaredVariables, Scope};
pub use fragment::FragmentMetadata;

#[derive(Debug)]
pub struct Component<'a> {
    fragment_tree: Vec<Node<'a, FragmentMetadata>>,
    declared_vars: DeclaredVariables,
    toplevel_nodes: Vec<ToplevelNodeData>,
    hoist: Vec<SyntaxNode>,
    exports: Vec<SmolStr>,
    component_id: u8,
    current_id: u32,

    css: Option<Css>,
    wasm: Option<Code<'a>>,
}

#[derive(Debug)]
pub struct ToplevelNodeData {
    pub node: SyntaxNode,
    pub substitute_assign_refs: bool,
}

// Public methods of component
impl<'a> Component<'a> {
    pub fn new(ast: DecorousAst<'a>) -> Self {
        let mut c = Self {
            fragment_tree: vec![],
            declared_vars: DeclaredVariables::new(),
            toplevel_nodes: vec![],
            hoist: vec![],
            exports: vec![],
            current_id: 0,
            #[cfg(not(debug_assertions))]
            component_id: rand::thread_rng().gen(),
            #[cfg(debug_assertions)]
            component_id: 0,

            css: None,
            wasm: None,
        };
        c.compute(ast);
        c.hoist_unmutated_vars();
        c
    }

    pub fn declared_vars(&self) -> &DeclaredVariables {
        &self.declared_vars
    }

    pub fn fragment_tree(&self) -> &[Node<'_, FragmentMetadata>] {
        self.fragment_tree.as_ref()
    }

    pub fn toplevel_nodes(&self) -> &[ToplevelNodeData] {
        self.toplevel_nodes.as_ref()
    }

    pub fn hoist(&self) -> &[SyntaxNode] {
        self.hoist.as_ref()
    }

    pub fn descendents(&'a self) -> NodeIter<'a, FragmentMetadata> {
        NodeIter::new(self.fragment_tree())
    }

    pub fn component_id(&self) -> u8 {
        self.component_id
    }

    pub fn css(&self) -> Option<&Css> {
        self.css.as_ref()
    }

    pub fn wasm(&self) -> Option<&Code<'_>> {
        self.wasm.as_ref()
    }

    pub fn exports(&self) -> &[SmolStr] {
        self.exports.as_ref()
    }
}

// Private methods of Component
impl<'a> Component<'a> {
    fn compute(&mut self, ast: DecorousAst<'a>) {
        let (mut nodes, script, css, wasm) = ast.into_components();
        if let Some(script) = script {
            self.extract_toplevel_data(script);
        }
        if let Some(mut css) = css {
            self.isolate_css(&mut css, &mut nodes);
            self.css = Some(css);
        }
        self.wasm = wasm;
        self.build_fragment_tree(nodes);
    }

    fn isolate_css(&mut self, css: &mut Css, nodes: &mut [Node<'a, Location>]) {
        self.modify_selectors(css.rules_mut());
        self.assign_css_mustaches(css.rules_mut());
        self.assign_node_classes(nodes);
    }

    fn assign_css_mustaches(&mut self, rules: &[css::ast::Rule]) {
        use css::ast::*;
        for rule in rules {
            let rule = match rule {
                Rule::At(at_rule) => {
                    if let Some(contents) = at_rule.contents() {
                        self.assign_css_mustaches(contents);
                    }
                    continue;
                }
                Rule::Regular(rule) => rule,
            };

            for decl in rule.declarations() {
                for mustache in decl.values().iter().filter_map(|val| match val {
                    Value::Mustache(m) => Some(m),
                    Value::Css(_) => None,
                }) {
                    self.declared_vars.insert_css_mustache(mustache.clone());
                }
            }
        }
    }

    fn assign_node_classes(&self, nodes: &mut [Node<'_, Location>]) {
        traverse_mut(nodes, &mut |node| {
            let NodeType::Element(elem) = node.node_type_mut() else {
            return;
        };
            let mut has_class = false;
            for attr in elem.attrs_mut() {
                match attr {
                    Attribute::KeyValue(key, None) if key == &"class" => {
                        *attr = Attribute::KeyValue(
                            "class",
                            Some(AttributeValue::Literal(Cow::Owned(format!(
                                "decor-{}",
                                self.component_id
                            )))),
                        );
                        has_class = true;
                    }
                    Attribute::KeyValue(key, Some(AttributeValue::Literal(lit)))
                        if key == &"class" =>
                    {
                        *attr = Attribute::KeyValue(
                            "class",
                            Some(AttributeValue::Literal(Cow::Owned(format!(
                                "{} decor-{}",
                                lit, self.component_id
                            )))),
                        );
                        has_class = true;
                    }
                    Attribute::KeyValue(key, Some(AttributeValue::JavaScript(js)))
                        if key == &"class" =>
                    {
                        let new_js = format!("`${{{js}}} decor-{}`", self.component_id);
                        let parsed = rslint_parser::parse_text(&new_js, 0).syntax();
                        *attr =
                            Attribute::KeyValue("class", Some(AttributeValue::JavaScript(parsed)));
                        has_class = true;
                    }
                    _ => {}
                }
            }
            if !has_class {
                elem.attrs_mut().push(Attribute::KeyValue(
                    "class",
                    Some(AttributeValue::Literal(Cow::Owned(format!(
                        "decor-{}",
                        self.component_id
                    )))),
                ));
            }
        });
    }

    fn modify_selectors(&mut self, rules: &mut [css::ast::Rule]) {
        use css::ast::*;
        for rule in rules {
            let rule = match rule {
                Rule::At(at_rule) => {
                    if let Some(contents) = at_rule.contents_mut() {
                        self.modify_selectors(contents);
                    }
                    continue;
                }
                Rule::Regular(rule) => rule,
            };

            let selector = rule.selector_mut();
            for sel in selector {
                for part in sel.parts_mut() {
                    let new_text = if let Some(t) = part.text() {
                        format!("{t}.decor-{}", self.component_id)
                    } else {
                        format!(".decor-{}", self.component_id)
                    };
                    part.text_mut().map(|s| *s = new_text.into());
                }
            }
        }
    }

    fn extract_toplevel_data(&mut self, script: SyntaxNode) {
        // Only go to top level assignments
        for child in script.children() {
            if let Some(var_decl) = child.try_to::<VarDecl>() {
                for decl in var_decl.declared() {
                    let idents = utils::get_idents_from_pattern(decl.pattern().unwrap());
                    for ident in idents {
                        self.declared_vars.insert_var(ident);
                    }
                }
                self.toplevel_nodes.push(ToplevelNodeData {
                    node: child,
                    substitute_assign_refs: true,
                });
            } else if let Some(fn_decl) = child.try_to::<FnDecl>() {
                let Some(ident) = fn_decl.name().and_then(|name| name.ident_token()) else {
                    continue;
                };

                self.declared_vars.insert_var(ident.text().clone());
                self.toplevel_nodes.push(ToplevelNodeData {
                    node: child,
                    substitute_assign_refs: true,
                });
            } else if child.is::<ImportDecl>() || child.is::<ExportDecl>() {
                if let Some(decl) = child.try_to::<ExportDecl>().and_then(|exp| exp.decl()) {
                    match decl {
                        Decl::FnDecl(decl) => {
                            let tok = decl.name().unwrap().ident_token().unwrap();
                            let name = tok.text();
                            self.exports.push(name.clone());
                        }
                        Decl::ClassDecl(decl) => {
                            let tok = decl.name().unwrap().ident_token().unwrap();
                            let name = tok.text();
                            self.exports.push(name.clone());
                        }
                        Decl::VarDecl(decl) => {
                            for pat in decl.declared().filter_map(|d| d.pattern()) {
                                self.exports.extend(utils::get_idents_from_pattern(pat));
                            }
                        }
                        _ => {}
                    }
                }
                self.hoist.push(child);
            } else {
                self.toplevel_nodes.push(ToplevelNodeData {
                    node: child,
                    substitute_assign_refs: false,
                });
            }
        }
    }

    fn build_fragment_tree(&mut self, ast: Vec<Node<'a, Location>>) {
        let mut fragment_tree = vec![];

        let mut current_scopes = vec![];
        for node in ast {
            let mut current_scope_id = None;
            let mut node = node.cast_meta(&mut |node| {
                let id = self.generate_elem_id();
                let fragment = FragmentMetadata::new(id, None, *node.metadata(), current_scope_id);
                // Set scope after generating metadata because a {#for}'s scope can't contain
                // itself
                if matches!(
                    node.node_type(),
                    NodeType::SpecialBlock(SpecialBlock::For(_))
                ) {
                    current_scope_id = Some(id);
                }
                fragment
            });

            self.get_special_vars(&mut node, None, &mut current_scopes);

            fragment_tree.push(node);
        }
        self.fragment_tree = fragment_tree;
    }

    fn get_special_vars(
        &mut self,
        node: &mut Node<'_, FragmentMetadata>,
        parent_id: Option<u32>,
        scope_stack: &mut Vec<Scope>,
    ) {
        node.metadata_mut().set_parent_id(parent_id);

        let id = node.metadata().id();
        let scope = node.metadata().scope();
        match node.node_type_mut() {
            NodeType::Element(elem) => {
                for attr in elem.attrs() {
                    match attr {
                        Attribute::EventHandler(handler) => {
                            if let Some(arrow_expr) = handler
                                .expr()
                                .first_child()
                                .and_then(|child| child.try_to::<ArrowExpr>())
                            {
                                self.declared_vars.insert_arrow_expr(arrow_expr, scope);
                            }
                        }
                        Attribute::Binding(binding) => {
                            let name = SmolStr::new(binding);
                            self.declared_vars.insert_binding(name);
                        }
                        Attribute::KeyValue(_, _) => continue,
                    }
                }

                elem.children_mut().iter_mut().for_each(|child| {
                    self.get_special_vars(child, Some(id), scope_stack);
                });
            }

            NodeType::SpecialBlock(block) => match block {
                SpecialBlock::If(if_block) => {
                    if_block.inner_mut().iter_mut().for_each(|child| {
                        self.get_special_vars(child, Some(id), scope_stack);
                    });
                    if let Some(else_block) = if_block.else_block_mut() {
                        for n in else_block.iter_mut() {
                            self.get_special_vars(n, Some(id), scope_stack);
                        }
                    }
                }
                SpecialBlock::For(for_block) => {
                    scope_stack.push(Scope::new());
                    let var_id = self.declared_vars.generate_id();
                    for scope in scope_stack.iter_mut() {
                        scope.add(SmolStr::new(for_block.binding()), var_id);
                    }
                    for_block.inner_mut().iter_mut().for_each(|child| {
                        self.get_special_vars(child, Some(id), scope_stack);
                    });
                    let scope = scope_stack.pop().unwrap();
                    self.declared_vars.insert_scope(id, scope);
                }
            },

            _ => {}
        }
    }

    fn hoist_unmutated_vars(&mut self) {
        fn remove_mutated(
            s: &SyntaxNode,
            unmutated: &mut HashSet<(VarDecl, Vec<SmolStr>)>,
        ) -> bool {
            let old_len = unmutated.len();
            let unbound = utils::get_unbound_refs(s);
            for unbound in unbound
                .iter()
                .filter(|nref| utils::is_from_assignment(nref))
            {
                let tok = unbound.ident_token().unwrap();
                let text = tok.text();
                unmutated.retain(|(_, decls)| decls.contains(text));
            }
            old_len == unmutated.len()
        }

        let mut unmutated: HashSet<(VarDecl, Vec<SmolStr>)> = HashSet::new();
        for toplevel in self.toplevel_nodes() {
            let does_mutate = remove_mutated(&toplevel.node, &mut unmutated);
            if toplevel.node.is::<VarDecl>() {
                let var_decl: VarDecl = toplevel.node.to();
                let mut all_declared = vec![];
                for decl in var_decl.declared() {
                    all_declared.extend(utils::get_idents_from_pattern(decl.pattern().unwrap()));
                }
                // If the variable assignment has a mutation, that means the variable
                // itself is not a valid candidate for being hoisted
                if does_mutate {
                    continue;
                }
                unmutated.insert((var_decl, all_declared));
            }
        }
        for node in self.descendents() {
            match node.node_type() {
                NodeType::Element(elem) => {
                    for attr in elem.attrs() {
                        match attr {
                            Attribute::Binding(binding) => {
                                unmutated.retain(|(_, decls)| {
                                    decls.iter().all(|decl| &decl.as_str() != binding)
                                });
                            }
                            Attribute::EventHandler(evt_handler) => {
                                remove_mutated(evt_handler.expr(), &mut unmutated);
                            }
                            Attribute::KeyValue(_, Some(AttributeValue::JavaScript(js))) => {
                                remove_mutated(js, &mut unmutated);
                            }
                            Attribute::KeyValue(_, _) => {}
                        }
                    }
                }
                NodeType::SpecialBlock(SpecialBlock::If(block)) => {
                    remove_mutated(block.expr(), &mut unmutated);
                }
                NodeType::SpecialBlock(SpecialBlock::For(block)) => {
                    remove_mutated(block.expr(), &mut unmutated);
                }
                NodeType::Mustache(js) => {
                    remove_mutated(js, &mut unmutated);
                }
                NodeType::Text(_) | NodeType::Comment(_) => {}
            }
        }

        for (decl, vars) in unmutated {
            for var in vars {
                self.declared_vars.remove_var(&var);
            }
            let pos = self
                .toplevel_nodes()
                .iter()
                .position(|node| &node.node == decl.syntax())
                .expect("VarDecl should be in toplevel nodes");
            self.toplevel_nodes.remove(pos);
            self.hoist.push(decl.syntax().clone());
        }
    }

    fn generate_elem_id(&mut self) -> u32 {
        let old = self.current_id;
        self.current_id += 1;
        old
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use itertools::Itertools;
    use rslint_parser::SmolStr;

    use super::*;
    use crate::parser::parse;

    fn make_component(source: &str) -> Component<'_> {
        let ast = parse(source);
        Component::new(ast.unwrap())
    }

    #[test]
    fn can_extract_toplevel_variables() {
        let component = make_component(
            "---js let x, z = (3, 2); let y = 55; let [...l] = thing--- #button[@click={() => { x = 0; z = 0; l = 0; y = 0; }}]:Click me",
        );
        assert_eq!(
            &(&[
                (SmolStr::from("x"), 0),
                (SmolStr::from("z"), 1),
                (SmolStr::from("y"), 2),
                (SmolStr::from("l"), 3)
            ]
            .into_iter()
            .collect::<HashMap<SmolStr, u32>>()),
            &component.declared_vars.all_vars()
        );
    }

    #[test]
    fn can_build_fragment_tree() {
        let component = make_component("#div #span:hello #span:hello2 /div");
        insta::assert_debug_snapshot!(component.fragment_tree);
    }

    #[test]
    fn can_get_all_declared_items_with_proper_substitution() {
        let component =
            make_component("---js let x = 3; function func() { x = 44; } variable; ---");
        insta::assert_debug_snapshot!(component.toplevel_nodes());
    }

    #[test]
    fn hoists_imports() {
        let component = make_component("---js import data from \"data\"---");
        insta::assert_debug_snapshot!(component.hoist());
    }

    #[test]
    fn can_extract_closures_from_html() {
        let component = make_component("#button[@click={() => console.log(\"hello\")}]/button");
        insta::assert_debug_snapshot!(component.declared_vars());
    }

    #[test]
    fn can_build_fragment_tree_with_scopes_of_for_blocks() {
        let component = make_component("{#for i in [1, 2, 3]} #div hello /div {/for}");
        insta::assert_debug_snapshot!(component.fragment_tree);
    }

    #[test]
    fn can_extract_scopes() {
        let component =
            make_component("{#for i in [1, 2, 3]} hello {/for} {#for i in [1, 2, 3]} hello {/for}");
        insta::assert_yaml_snapshot!(component.declared_vars().all_scopes().iter().sorted_by(|(a, _), (b, _)| a.cmp(b)).collect_vec(), {
            "[][1].env" => insta::sorted_redaction()
        });
    }

    #[test]
    fn can_extract_nested_scopes() {
        let component =
            make_component("{#for i in [1, 2, 3]} {#for x in [2, 3, 4]} eee {/for} {/for}");
        insta::assert_yaml_snapshot!(component.declared_vars().all_scopes().iter().sorted_by(|(a, _), (b, _)| a.cmp(b)).collect_vec(), {
            "[][1].env" => insta::sorted_redaction()
        });
    }

    #[test]
    fn does_not_hoist_var_if_mutated_in_script() {
        let component = make_component("---js let x = 0; let mutate_x = () => x = 4;---");
        assert!(component.hoist().is_empty());
    }

    #[test]
    fn hoists_var_if_never_mutated() {
        let component = make_component("---js let x = 0--- {x}");
        insta::assert_debug_snapshot!(component.hoist());
    }

    #[test]
    fn assigns_classes_to_nodes() {
        let component = make_component("---css p { color: red; } --- #p:Hello!");
        insta::assert_debug_snapshot!(component.fragment_tree());
    }

    #[test]
    fn merges_previously_assigned_class_in_reassignment() {
        let component = make_component("---css p { color: red; } --- #p[class=\"green\"]:Hello!");
        insta::assert_debug_snapshot!(component.fragment_tree());
    }

    #[test]
    fn merges_previously_assigned_class_in_reassignment_with_js() {
        let component = make_component("---css p { color: red; } --- #p[class={\"green\"}]:Hello!");
        insta::assert_debug_snapshot!(component.fragment_tree());
    }

    #[test]
    fn modifies_css_selectors_to_use_component_id() {
        let component = make_component("---css p:has(span) { color: red; } ---");
        insta::assert_debug_snapshot!(component.css());
    }

    #[test]
    fn assigns_ids_to_mustaches_in_css() {
        let component = make_component("---css p { color: {color}; } ---");
        insta::assert_debug_snapshot!(component.declared_vars().css_mustaches());
    }

    #[test]
    fn does_not_hoist_with_binding() {
        let component = make_component("---js let x = 0; --- #input[:x:]/input");
        assert!(component.hoist().is_empty())
    }

    #[test]
    fn bindings_are_put_into_declared_vars() {
        let component = make_component("---js let x = 0; --- #input[:x:]/input");
        insta::assert_debug_snapshot!(component.declared_vars())
    }

    #[test]
    fn hoists_exports() {
        let component = make_component("---js export function x() { console.log(\"hi\") } ---");
        assert!(component.toplevel_nodes().is_empty());
        insta::assert_debug_snapshot!(component.hoist())
    }

    #[test]
    fn can_get_exports_from_script() {
        let component = make_component(
            "---js export function x() { console.log(\"hi\"); }; export let l = 1 ---",
        );
        insta::assert_debug_snapshot!(component.exports())
    }
}
