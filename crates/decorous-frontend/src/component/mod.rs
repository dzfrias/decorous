mod fragment;

use rslint_parser::{
    ast::{Pattern, VarDecl},
    SmolStr, SyntaxNode, SyntaxNodeExt,
};

use self::fragment::FragmentMetadata;
use crate::ast::{DecorousAst, Location, Node, NodeType, SpecialBlock};

#[derive(Debug)]
pub struct Component<'a> {
    fragment_tree: Vec<Node<'a, FragmentMetadata>>,
    declared_vars: Vec<SmolStr>,

    current_id: u64,
}

// Public methods of component
impl<'a> Component<'a> {
    pub fn new(ast: DecorousAst<'a>) -> Self {
        let mut c = Self {
            fragment_tree: vec![],
            declared_vars: vec![],
            current_id: 0,
        };
        c.compute(ast);
        c
    }

    pub fn declared_vars(&self) -> &[SmolStr] {
        self.declared_vars.as_ref()
    }
}

// Private methods of Component
impl<'a> Component<'a> {
    fn compute(&mut self, ast: DecorousAst<'a>) {
        let (nodes, script, _css) = ast.into_components();
        if let Some(script) = script {
            self.extract_toplevel_vars(script);
        }
        self.build_fragment_tree(nodes);
    }

    fn extract_toplevel_vars(&mut self, script: SyntaxNode) {
        // Only go to top level assignments
        for child in script.children() {
            if !child.is::<VarDecl>() {
                continue;
            }

            let var_decl = child.to::<VarDecl>();
            for pattern in var_decl.declared().filter_map(|decl| decl.pattern()) {
                match pattern {
                    Pattern::SinglePattern(single) => {
                        let Some(name) = single.name() else {
                            continue;
                        };
                        let Some(ident) = name.ident_token() else {
                            continue;
                        };
                        self.declared_vars.push(ident.text().to_owned());
                    }
                    // TODO: More assignment types?
                    _ => {}
                }
            }
        }
    }

    fn build_fragment_tree(&mut self, ast: Vec<Node<'a, Location>>) {
        fn build_fragment_tree_from_node<'a>(
            node: &'a mut Node<'_, FragmentMetadata>,
            parent_id: u64,
        ) {
            node.metadata_mut().set_parent_id(Some(parent_id));

            let id = node.metadata().id();
            match node.node_type_mut() {
                NodeType::Element(elem) => elem
                    .children_mut()
                    .iter_mut()
                    .for_each(|child| build_fragment_tree_from_node(child, id)),
                NodeType::SpecialBlock(block) => match block {
                    SpecialBlock::If(if_block) => if_block
                        .inner_mut()
                        .iter_mut()
                        .for_each(|child| build_fragment_tree_from_node(child, id)),
                    SpecialBlock::For(for_block) => for_block
                        .inner_mut()
                        .iter_mut()
                        .for_each(|child| build_fragment_tree_from_node(child, id)),
                },
                _ => {}
            }
        }

        self.fragment_tree = ast
            .into_iter()
            .map(|node| {
                node.cast_meta(&mut |loc| {
                    let id = self.generate_id();
                    FragmentMetadata::new(id, None, loc)
                })
            })
            .map(|mut node| {
                let id = node.metadata().id();
                build_fragment_tree_from_node(&mut node, id);
                node.metadata_mut().set_parent_id(None);
                node
            })
            .collect();
    }

    fn generate_id(&mut self) -> u64 {
        let old = self.current_id;
        self.current_id += 1;
        old
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;

    fn make_component(source: &str) -> Component<'_> {
        let parser = Parser::new(source);
        let ast = parser.parse();
        assert!(ast.1.is_empty());
        Component::new(ast.0)
    }

    #[test]
    fn can_extract_toplevel_variables() {
        let component = make_component("<script>let x, z = (3, 2); const y = 55;</script>");
        assert_eq!(
            &[SmolStr::from("x"), SmolStr::from("z"), SmolStr::from("y")],
            component.declared_vars()
        );
    }

    #[test]
    fn can_build_fragment_tree() {
        let component = make_component("<div><span>hello</span><span>hello2</span></div>");
        insta::assert_debug_snapshot!(component);
    }
}
