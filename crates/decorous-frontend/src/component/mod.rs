mod declared_vars;
mod fragment;

use rslint_parser::{
    ast::{ArrowExpr, FnDecl, ImportDecl, VarDecl},
    SyntaxNode, SyntaxNodeExt,
};

use crate::{
    ast::{Attribute, DecorousAst, Location, Node, NodeType, SpecialBlock},
    utils,
};
pub use declared_vars::DeclaredVariables;
pub use fragment::FragmentMetadata;

#[derive(Debug)]
pub struct Component<'a> {
    fragment_tree: Vec<Node<'a, FragmentMetadata>>,
    declared_vars: DeclaredVariables,
    toplevel_nodes: Vec<ToplevelNodeData>,
    hoist: Vec<SyntaxNode>,

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
}

// Private methods of Component
impl<'a> Component<'a> {
    fn compute(&mut self, ast: DecorousAst<'a>) {
        let (nodes, script, _css) = ast.into_components();
        if let Some(script) = script {
            self.extract_toplevel_data(script);
        }
        self.build_fragment_tree(nodes);
    }

    fn extract_toplevel_data(&mut self, script: SyntaxNode) {
        // Only go to top level assignments
        for child in script.children() {
            if child.is::<VarDecl>() {
                let var_decl = child.to::<VarDecl>();
                for pattern in var_decl.declared().filter_map(|decl| decl.pattern()) {
                    let idents = utils::get_idents_from_pattern(pattern);
                    for ident in idents {
                        self.declared_vars.insert_var(ident);
                    }
                }
                self.toplevel_nodes.push(ToplevelNodeData {
                    node: child,
                    substitute_assign_refs: true,
                });
            } else if child.is::<FnDecl>() {
                let fn_decl = child.to::<FnDecl>();
                let Some(ident) = fn_decl.name().and_then(|name| name.ident_token()) else {
                    continue;
                };

                self.declared_vars.insert_var(ident.text().to_owned());
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
    }

    fn build_fragment_tree(&mut self, ast: Vec<Node<'a, Location>>) {
        let mut fragment_tree = vec![];

        for node in ast {
            let mut node = node.cast_meta(&mut |node| {
                let id = self.generate_elem_id();
                FragmentMetadata::new(id, None, *node.metadata())
            });

            let id = node.metadata().id();
            self.build_fragment_tree_from_node(&mut node, id);
            node.metadata_mut().set_parent_id(None);

            fragment_tree.push(node);
        }
        self.fragment_tree = fragment_tree;
    }

    fn build_fragment_tree_from_node(
        &mut self,
        node: &mut Node<'_, FragmentMetadata>,
        parent_id: u32,
    ) {
        node.metadata_mut().set_parent_id(Some(parent_id));

        let id = node.metadata().id();
        match node.node_type_mut() {
            NodeType::Element(elem) => {
                for attr in elem.attrs() {
                    if let Attribute::EventHandler(handler) = attr {
                        if let Some(first_child) = handler.expr().first_child() {
                            if first_child.is::<ArrowExpr>() {
                                self.declared_vars.insert_arrow_expr(first_child.to());
                            }
                        }
                    }
                }

                elem.children_mut()
                    .iter_mut()
                    .for_each(|child| self.build_fragment_tree_from_node(child, id));
            }
            NodeType::SpecialBlock(block) => match block {
                SpecialBlock::If(if_block) => if_block
                    .inner_mut()
                    .iter_mut()
                    .for_each(|child| self.build_fragment_tree_from_node(child, id)),
                SpecialBlock::For(for_block) => for_block
                    .inner_mut()
                    .iter_mut()
                    .for_each(|child| self.build_fragment_tree_from_node(child, id)),
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

    use rslint_parser::SmolStr;

    use super::*;
    use crate::{ParseError, Parser};

    fn make_component(source: &str) -> Component<'_> {
        let parser = Parser::new(source);
        let ast = parser.parse();
        assert_eq!(Vec::<ParseError>::new(), ast.1);
        Component::new(ast.0)
    }

    #[test]
    fn can_extract_toplevel_variables() {
        let component = make_component(
            "<script>let x, z = (3, 2); const y = 55; const [...l] = thing</script>",
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
        let component = make_component("<div><span>hello</span><span>hello2</span></div>");
        insta::assert_debug_snapshot!(component.fragment_tree);
    }

    #[test]
    fn can_get_all_declared_items_with_proper_substitution() {
        let component =
            make_component("<script>let x = 3; function func() { return 33; }; variable;</script>");
        insta::assert_debug_snapshot!(component.toplevel_nodes());
    }

    #[test]
    fn hoists_imports() {
        let component = make_component("<script>let x = 3; let y = 4; import data from \"data\"");
        insta::assert_debug_snapshot!(component.hoist());
    }

    #[test]
    fn can_extract_closures_from_html() {
        let component = make_component("<button on:click={() => console.log(\"hello\")}></button>");
        insta::assert_debug_snapshot!(component.declared_vars());
    }
}
