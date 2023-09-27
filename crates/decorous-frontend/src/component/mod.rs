mod declared_vars;
mod fragment;
mod globals;
mod passes;

use std::path::Path;

#[cfg(not(debug_assertions))]
use rand::Rng;
use rslint_parser::{
    ast::{ArrowExpr, Decl, ExportDecl, FnDecl, ImportDecl, LabelledStmt, VarDecl},
    AstNode, SmolStr, SyntaxNode, SyntaxNodeExt,
};

use crate::{
    ast::{Attribute, Code, DecorousAst, Node, NodeIter, NodeType, SpecialBlock},
    component::passes::{DepAnalysisPass, IsolateCssPass, Pass, StaticPass},
    css::ast::Css,
    location::Location,
    utils, Ctx,
};
pub use declared_vars::{DeclaredVariables, Scope};
pub use fragment::FragmentMetadata;

#[derive(Debug)]
pub struct Component<'a> {
    pub fragment_tree: Vec<Node<'a, FragmentMetadata>>,
    pub declared_vars: DeclaredVariables,
    pub toplevel_nodes: Vec<ToplevelNodeData>,
    pub hoist: Vec<SyntaxNode>,
    pub exports: Vec<SmolStr>,
    pub uses: Vec<&'a Path>,
    pub css: Option<Css>,
    pub wasm: Option<Code<'a>>,
    pub comptime: Option<Code<'a>>,
    pub component_id: u8,

    ctx: Ctx<'a>,
    current_id: u32,
}

#[derive(Debug)]
pub struct ToplevelNodeData {
    pub node: SyntaxNode,
    pub substitute_assign_refs: bool,
}

// Public methods of component
impl<'a> Component<'a> {
    pub fn new(ast: DecorousAst<'a>, ctx: Ctx<'a>) -> Self {
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
            uses: vec![],
            ctx,

            css: None,
            comptime: None,
            wasm: None,
        };
        c.compute(ast);

        c
    }

    pub fn run_passes(&mut self) -> anyhow::Result<()> {
        let isolate_pass = IsolateCssPass::new();
        let static_pass = StaticPass::new();
        let dep_pass = DepAnalysisPass::new();
        isolate_pass.run(self)?;
        static_pass.run(self)?;
        dep_pass.run(self)?;

        Ok(())
    }

    pub fn descendents(&'a self) -> NodeIter<'a, FragmentMetadata> {
        NodeIter::new(&self.fragment_tree)
    }
}

// Private methods of Component
impl<'a> Component<'a> {
    fn compute(&mut self, ast: DecorousAst<'a>) {
        if let Some(script) = ast.script {
            self.extract_toplevel_data(script);
        }
        self.css = ast.css;
        self.wasm = ast.wasm;
        self.comptime = ast.comptime;
        self.build_fragment_tree(ast.nodes);
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
            } else if let Some(labl_stmt) = child.try_to::<LabelledStmt>() {
                if labl_stmt.label().unwrap().ident_token().unwrap().text() != "$" {
                    self.toplevel_nodes.push(ToplevelNodeData {
                        node: child,
                        substitute_assign_refs: false,
                    });
                    continue;
                }
                let Some(stmt) = labl_stmt.stmt() else {
                    continue;
                };

                self.toplevel_nodes.push(ToplevelNodeData {
                    node: stmt.syntax().clone(),
                    substitute_assign_refs: false,
                });
                self.declared_vars
                    .insert_reactive_block(stmt.syntax().clone());
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
                let fragment = FragmentMetadata::new(id, None, node.metadata, current_scope_id);
                // Set scope after generating metadata because a {#for}'s scope can't contain
                // itself
                if matches!(node.node_type, NodeType::SpecialBlock(SpecialBlock::For(_))) {
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
        node: &mut Node<'a, FragmentMetadata>,
        parent_id: Option<u32>,
        scope_stack: &mut Vec<Scope>,
    ) {
        node.metadata.set_parent_id(parent_id);

        let id = node.metadata.id();
        let scope = node.metadata.scope();
        match &mut node.node_type {
            NodeType::Element(elem) => {
                for attr in &elem.attrs {
                    match attr {
                        Attribute::EventHandler(handler) => {
                            if let Some(arrow_expr) = handler
                                .expr
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

                elem.children.iter_mut().for_each(|child| {
                    self.get_special_vars(child, Some(id), scope_stack);
                });
            }

            NodeType::SpecialBlock(block) => match block {
                SpecialBlock::If(if_block) => {
                    if_block.inner.iter_mut().for_each(|child| {
                        self.get_special_vars(child, Some(id), scope_stack);
                    });
                    if let Some(else_block) = &mut if_block.else_block {
                        for n in else_block.iter_mut() {
                            self.get_special_vars(n, Some(id), scope_stack);
                        }
                    }
                }
                SpecialBlock::For(for_block) => {
                    scope_stack.push(Scope::new());
                    let var_id = self.declared_vars.generate_id();
                    for scope in scope_stack.iter_mut() {
                        scope.add(SmolStr::new(for_block.binding), var_id);
                    }
                    for_block.inner.iter_mut().for_each(|child| {
                        self.get_special_vars(child, Some(id), scope_stack);
                    });
                    let scope = scope_stack.pop().unwrap();
                    self.declared_vars.insert_scope(id, scope);
                }
                SpecialBlock::Use(use_block) => self.uses.push(use_block.path),
            },

            _ => {}
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

    use decorous_errors::Source;
    use itertools::Itertools;

    use super::*;
    use crate::Parser;

    fn make_component(source: &str) -> Component<'_> {
        let parser = Parser::new(source);
        let ast = parser.parse().unwrap();
        let mut c = Component::new(
            ast,
            Ctx {
                errs: decorous_errors::stderr(Source {
                    src: source,
                    name: "TEST".to_owned(),
                }),
                ..Default::default()
            },
        );
        c.run_passes().unwrap();
        c
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
        insta::assert_debug_snapshot!(component.toplevel_nodes);
    }

    #[test]
    fn hoists_imports() {
        let component = make_component("---js import data from \"data\"---");
        insta::assert_debug_snapshot!(component.hoist);
    }

    #[test]
    fn can_extract_closures_from_html() {
        let component = make_component("#button[@click={() => console.log(\"hello\")}]/button");
        insta::assert_debug_snapshot!(component.declared_vars);
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
        insta::assert_yaml_snapshot!(component.declared_vars.all_scopes().iter().sorted_by(|(a, _), (b, _)| a.cmp(b)).collect_vec(), {
            "[][1].env" => insta::sorted_redaction()
        });
    }

    #[test]
    fn can_extract_nested_scopes() {
        let component =
            make_component("{#for i in [1, 2, 3]} {#for x in [2, 3, 4]} eee {/for} {/for}");
        insta::assert_yaml_snapshot!(component.declared_vars.all_scopes().iter().sorted_by(|(a, _), (b, _)| a.cmp(b)).collect_vec(), {
            "[][1].env" => insta::sorted_redaction()
        });
    }

    #[test]
    fn does_not_hoist_var_if_mutated_in_script() {
        let component = make_component("---js let x = 0; let mutate_x = () => x = 4;---");
        assert!(component.hoist.is_empty());
    }

    #[test]
    fn hoists_var_if_never_mutated() {
        let component = make_component("---js let x = 0--- {x}");
        insta::assert_debug_snapshot!(component.hoist);
    }

    #[test]
    fn assigns_classes_to_nodes() {
        let component = make_component("---css p { color: red; } --- #p:Hello!");
        insta::assert_debug_snapshot!(component.fragment_tree);
    }

    #[test]
    fn merges_previously_assigned_class_in_reassignment() {
        let component = make_component("---css p { color: red; } --- #p[class=\"green\"]:Hello!");
        insta::assert_debug_snapshot!(component.fragment_tree);
    }

    #[test]
    fn merges_previously_assigned_class_in_reassignment_with_js() {
        let component = make_component("---css p { color: red; } --- #p[class={\"green\"}]:Hello!");
        insta::assert_debug_snapshot!(component.fragment_tree);
    }

    #[test]
    fn modifies_css_selectors_to_use_component_id() {
        let component = make_component("---css p:has(span) { color: red; } ---");
        insta::assert_debug_snapshot!(component.css);
    }

    #[test]
    fn assigns_ids_to_mustaches_in_css() {
        let component = make_component("---css p { color: {color}; } ---");
        insta::assert_debug_snapshot!(component.declared_vars.css_mustaches());
    }

    #[test]
    fn does_not_hoist_with_binding() {
        let component = make_component("---js let x = 0; --- #input[:x:]/input");
        assert!(component.hoist.is_empty())
    }

    #[test]
    fn bindings_are_put_into_declared_vars() {
        let component = make_component("---js let x = 0; --- #input[:x:]/input");
        insta::assert_debug_snapshot!(component.declared_vars)
    }

    #[test]
    fn hoists_exports() {
        let component = make_component("---js export function x() { console.log(\"hi\") } ---");
        assert!(component.toplevel_nodes.is_empty());
        insta::assert_debug_snapshot!(component.hoist)
    }

    #[test]
    fn can_get_exports_from_script() {
        let component = make_component(
            "---js export function x() { console.log(\"hi\"); }; export let l = 1 ---",
        );
        insta::assert_debug_snapshot!(component.exports)
    }

    #[test]
    fn can_extract_reactive_blocks() {
        let component = make_component("---js $: $: { let y = 4; }; ---");
        insta::assert_debug_snapshot!(component.declared_vars);
    }

    #[test]
    fn reactive_blocks_are_still_included_as_toplevel_nodes() {
        let component = make_component("---js let z = 0; $: let x = z + 1; $: { let y = 4; } --- #button[@click={() => z = 33}]:hi");
        insta::assert_debug_snapshot!(component.toplevel_nodes);
    }

    #[test]
    fn globals_are_not_subject_to_hoist_optimizations() {
        let component = make_component(
            "---js let z = document.getElementById(\"id\"); console.log(\"hi\"); --- {z}",
        );
        insta::assert_debug_snapshot!(component);
    }

    #[test]
    fn checks_all_edges_relating_to_dependents_of_hoist_graph() {
        let component = make_component(
            "---js let x = 0; let y = x + 1; let z = y + 1; --- #button[@click={() => x = 1}]:Wow!",
        );
        assert!(component.hoist.is_empty());
    }

    #[test]
    fn prunes_unused_variables() {
        let component = make_component("---js let x = 0; let y = x; ---");
        assert!(component.toplevel_nodes.is_empty());
        assert!(component.hoist.is_empty());
        assert!(component.declared_vars.is_empty());
    }

    #[test]
    fn used_variables_cause_dependencies_to_be_used_as_well() {
        let component = make_component(
            "---js let x = 0; function y() { return x; } --- #button[@click={y}]:Hi",
        );
        assert!(component.toplevel_nodes.is_empty());
        insta::assert_debug_snapshot!(component.hoist);
    }
}
