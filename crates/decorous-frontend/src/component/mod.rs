mod declared_vars;
mod fragment;

use std::mem;

use rslint_parser::{
    ast::{ArrowExpr, FnDecl, ImportDecl, VarDecl},
    AstNode, SmolStr, SyntaxNode, SyntaxNodeExt,
};

use crate::{
    ast::{
        Attribute, AttributeValue, DecorousAst, Location, Node, NodeIter, NodeType, SpecialBlock,
    },
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

    referenced: Vec<SmolStr>,

    current_id: u32,
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
            referenced: vec![],
            current_id: 0,
        };
        c.compute(ast);
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
}

// Private methods of Component
impl<'a> Component<'a> {
    fn compute(&mut self, ast: DecorousAst<'a>) {
        let (nodes, script, _css) = ast.into_components();
        let mut declared = vec![];
        if let Some(script) = script {
            let all_declared_vars = self.extract_toplevel_data(script);
            declared.extend(all_declared_vars);
        }
        self.build_fragment_tree(nodes, declared);
    }

    fn extract_toplevel_data(&mut self, script: SyntaxNode) -> Vec<(VarDecl, Vec<SmolStr>)> {
        let mut all_declared_vars = vec![];
        // Only go to top level assignments
        for child in script.children() {
            self.apply_refs(&child);

            if child.is::<VarDecl>() {
                let var_decl = child.to::<VarDecl>();
                let mut declared = vec![];
                for decl in var_decl.declared() {
                    let idents = utils::get_idents_from_pattern(decl.pattern().unwrap());
                    if let Some(val) = decl.value() {
                        let len_ref = self.referenced.len();
                        self.apply_refs(val.syntax());
                        // If there were any mutated variables, mark this variable as referenced.
                        if len_ref != self.referenced.len() {
                            self.referenced.extend(idents.clone());
                        }
                    }
                    for ident in idents {
                        declared.push(ident);
                    }
                }
                self.toplevel_nodes.push(ToplevelNodeData {
                    node: child,
                    substitute_assign_refs: true,
                });

                all_declared_vars.push((var_decl, declared));
            } else if child.is::<FnDecl>() {
                let fn_decl = child.to::<FnDecl>();
                let Some(ident) = fn_decl.name().and_then(|name| name.ident_token()) else {
                    continue;
                };

                self.declared_vars.insert_var(ident.text().clone());
                self.toplevel_nodes.push(ToplevelNodeData {
                    node: child,
                    substitute_assign_refs: true,
                });
            } else if child.is::<ImportDecl>() {
                self.hoist.push(child);
            } else {
                self.toplevel_nodes.push(ToplevelNodeData {
                    node: child,
                    substitute_assign_refs: false,
                });
            }
        }

        all_declared_vars
    }

    fn build_fragment_tree(
        &mut self,
        ast: Vec<Node<'a, Location>>,
        all_declared_vars: Vec<(VarDecl, Vec<SmolStr>)>,
    ) {
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

            let id = node.metadata().id();
            self.build_fragment_tree_from_node(&mut node, id, &mut current_scopes);
            node.metadata_mut().set_parent_id(None);

            fragment_tree.push(node);
        }
        self.handle_unreferenced_vars(all_declared_vars);
        self.fragment_tree = fragment_tree;
    }

    fn build_fragment_tree_from_node(
        &mut self,
        node: &mut Node<'_, FragmentMetadata>,
        parent_id: u32,
        current_scopes: &mut Vec<Scope>,
    ) {
        node.metadata_mut().set_parent_id(Some(parent_id));

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
                                self.apply_refs(arrow_expr.syntax());
                                self.declared_vars.insert_arrow_expr(arrow_expr, scope);
                            }
                        }
                        Attribute::KeyValue(_, Some(AttributeValue::JavaScript(js))) => {
                            self.apply_refs(js);
                        }
                        _ => continue,
                    }
                }

                elem.children_mut().iter_mut().for_each(|child| {
                    self.build_fragment_tree_from_node(child, id, current_scopes);
                });
            }

            NodeType::Mustache(js) => {
                self.apply_refs(js);
            }

            NodeType::SpecialBlock(block) => match block {
                SpecialBlock::If(if_block) => {
                    if_block.inner_mut().iter_mut().for_each(|child| {
                        self.build_fragment_tree_from_node(child, id, current_scopes);
                    });
                    self.apply_refs(if_block.expr());
                    if let Some(else_block) = if_block.else_block_mut() {
                        for n in else_block.iter_mut() {
                            self.build_fragment_tree_from_node(n, id, current_scopes);
                        }
                    }
                }
                SpecialBlock::For(for_block) => {
                    current_scopes.push(Scope::new());
                    let var_id = self.declared_vars.generate_id();
                    for scope in current_scopes.iter_mut() {
                        scope.add(SmolStr::new(for_block.binding()), var_id);
                    }
                    for_block.inner_mut().iter_mut().for_each(|child| {
                        self.build_fragment_tree_from_node(child, id, current_scopes);
                    });
                    self.apply_refs(for_block.expr());
                    let scope = current_scopes.pop().unwrap();
                    self.declared_vars.insert_scope(id, scope);
                }
            },

            _ => {}
        }
    }

    fn handle_unreferenced_vars(&mut self, all_declared_vars: Vec<(VarDecl, Vec<SmolStr>)>) {
        let ref_vars = mem::take(&mut self.referenced);
        for (decl, declared) in all_declared_vars {
            if declared.iter().any(|v| ref_vars.contains(v)) {
                for declared in declared {
                    self.declared_vars.insert_var(declared);
                }
                continue;
            }

            self.hoist.push(decl.syntax().clone());
            let pos = self
                .toplevel_nodes
                .iter()
                .position(|node| &node.node == decl.syntax())
                .expect("all var decls should have a corresponding toplevel node");
            self.toplevel_nodes.remove(pos);
        }
    }

    fn apply_refs(&mut self, syntax_node: &SyntaxNode) {
        for unbound in utils::get_unbound_refs(syntax_node) {
            let tok = unbound.ident_token().unwrap();
            let ident = tok.text();
            if !utils::is_from_assignment(&unbound) {
                continue;
            }
            self.referenced.push(ident.clone());
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
                (SmolStr::from("x"), 1),
                (SmolStr::from("z"), 2),
                (SmolStr::from("y"), 3),
                (SmolStr::from("l"), 4)
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
}
