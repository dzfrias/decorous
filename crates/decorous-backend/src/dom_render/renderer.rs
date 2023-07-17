use decorous_frontend::{
    ast::{Attribute, AttributeValue, CollapsedChildrenType, Node, NodeType},
    utils, DeclaredVariables, FragmentMetadata,
};
use std::{borrow::Cow, io};

use crate::codegen_utils;

pub trait Renderer<T>
where
    T: io::Write,
{
    fn init(&self, f: &mut T, toplevel_vars: &DeclaredVariables) -> io::Result<()>;
    fn create(&self, f: &mut T, toplevel_vars: &DeclaredVariables) -> io::Result<()>;
    fn mount(&self, f: &mut T, toplevel_vars: &DeclaredVariables) -> io::Result<()>;
    fn update(&self, f: &mut T, toplevel_vars: &DeclaredVariables) -> io::Result<()>;
    fn detach(&self, f: &mut T, toplevel_vars: &DeclaredVariables) -> io::Result<()>;
}

impl<'a, T> Renderer<T> for Node<'a, FragmentMetadata>
where
    T: io::Write,
{
    fn init(&self, f: &mut T, _toplevel_vars: &DeclaredVariables) -> io::Result<()> {
        match self.node_type() {
            NodeType::Element(element) => {
                writeln!(f, "let e{};", self.metadata().id())?;

                if element.inner_collapsed().is_some() {
                    return Ok(());
                }
                for child in element.children() {
                    child.init(f, _toplevel_vars)?;
                }
                Ok(())
            }
            NodeType::Text(_) | NodeType::Mustache(_) => {
                writeln!(f, "let e{};", self.metadata().id())
            }
            NodeType::Comment(_) => Ok(()),
            NodeType::SpecialBlock(_) => todo!(),
        }
    }

    fn create(&self, f: &mut T, toplevel_vars: &DeclaredVariables) -> io::Result<()> {
        match self.node_type() {
            NodeType::Element(element) => {
                writeln!(
                    f,
                    "e{} = document.createElement(\"{}\");",
                    self.metadata().id(),
                    element.tag()
                )?;

                for attr in element.attrs() {
                    match attr {
                        Attribute::KeyValue(key, value) => match value {
                            Some(AttributeValue::Literal(literal)) => {
                                let (s, use_quotes) = collapse_whitespace(literal);
                                writeln!(
                                    f,
                                    "e{}.setAttribute(\"{key}\", {}{}{1});",
                                    use_quotes.then_some("\"").unwrap_or_default(),
                                    s,
                                    self.metadata().id(),
                                )?;
                            }
                            Some(AttributeValue::JavaScript(js)) => writeln!(
                                f,
                                "e{}.setAttribute(\"{key}\", {});",
                                self.metadata().id(),
                                codegen_utils::replace_namerefs(
                                    js,
                                    &utils::get_unbound_refs(js),
                                    toplevel_vars
                                )
                            )?,
                            None => writeln!(
                                f,
                                "e{}.setAttribute(\"{key}\", \"\");",
                                self.metadata().id(),
                            )?,
                        },
                        Attribute::EventHandler(event_handler) => {
                            writeln!(
                                f,
                                "e{}.addEventListener(\"{}\", {});",
                                self.metadata().id(),
                                event_handler.event(),
                                codegen_utils::replace_namerefs(
                                    event_handler.expr(),
                                    &utils::get_unbound_refs(event_handler.expr()),
                                    toplevel_vars
                                )
                            )?;
                        }
                        Attribute::Binding(_binding) => todo!(),
                    }
                }

                if let Some(collapsed) = element.inner_collapsed() {
                    return match collapsed {
                        CollapsedChildrenType::Text(t) => {
                            let (s, use_qutoes) = collapse_whitespace(t);
                            writeln!(
                                f,
                                "e{}.textContent = {}{s}{1};",
                                self.metadata().id(),
                                use_qutoes.then_some("\"").unwrap_or_default()
                            )
                        }
                        CollapsedChildrenType::Html(html) => {
                            writeln!(f, "e{}.innerHtml = \"{html}\"", self.metadata().id())
                        }
                    };
                }
                for child in element.children() {
                    child.create(f, toplevel_vars)?;
                }

                Ok(())
            }
            NodeType::Text(text) => {
                let (s, use_qutoes) = collapse_whitespace(text);
                writeln!(
                    f,
                    "e{} = document.createTextNode({}{s}{1});",
                    self.metadata().id(),
                    use_qutoes.then_some("\"").unwrap_or_default()
                )
            }
            NodeType::Mustache(mustache) => {
                let new_text = codegen_utils::replace_namerefs(
                    mustache,
                    &utils::get_unbound_refs(mustache),
                    toplevel_vars,
                );
                writeln!(
                    f,
                    "e{} = document.createTextNode({new_text});",
                    self.metadata().id()
                )
            }
            NodeType::Comment(_) => Ok(()),
            NodeType::SpecialBlock(_) => todo!(),
        }
    }

    fn mount(&self, f: &mut T, _toplevel_vars: &DeclaredVariables) -> io::Result<()> {
        if matches!(self.node_type(), NodeType::Comment(_)) {
            return Ok(());
        }

        if let Some(parent_id) = self.metadata().parent_id() {
            writeln!(f, "e{parent_id}.appendChild(e{});", self.metadata().id())?;
        } else {
            writeln!(f, "target.appendChild(e{});", self.metadata().id())?;
        }

        match self.node_type() {
            NodeType::Element(element) if element.inner_collapsed().is_none() => {
                for child in element.children() {
                    child.mount(f, _toplevel_vars)?;
                }
            }
            NodeType::SpecialBlock(_) => todo!(),
            _ => {}
        }

        Ok(())
    }

    fn update(&self, f: &mut T, toplevel_vars: &DeclaredVariables) -> io::Result<()> {
        match self.node_type() {
            NodeType::Element(elem) => {
                for attr in elem.attrs() {
                    // TODO: Update to use dirty
                    if let Attribute::KeyValue(key, Some(AttributeValue::JavaScript(js))) = attr {
                        let unbound = utils::get_unbound_refs(js);
                        let dirty_indices = codegen_utils::calc_dirty(&unbound, toplevel_vars);
                        let replacement =
                            codegen_utils::replace_namerefs(js, &unbound, toplevel_vars);
                        writeln!(
                            f,
                            "if ({dirty_indices}) e{}.setAttribute(\"{key}\", {replacement});",
                            self.metadata().id(),
                        )?;
                    }
                }

                if elem.inner_collapsed().is_none() {
                    for child in elem.children() {
                        child.update(f, toplevel_vars)?;
                    }
                }
                Ok(())
            }
            NodeType::Mustache(mustache) => {
                let unbound = utils::get_unbound_refs(mustache);
                let dirty_indices = codegen_utils::calc_dirty(&unbound, toplevel_vars);
                let new_text = codegen_utils::replace_namerefs(mustache, &unbound, toplevel_vars);
                writeln!(
                    f,
                    "if ({dirty_indices}) e{}.data = {new_text};",
                    self.metadata().id()
                )
            }
            _ => Ok(()),
        }
    }

    fn detach(&self, f: &mut T, _toplevel_vars: &DeclaredVariables) -> io::Result<()> {
        if matches!(self.node_type(), NodeType::Comment(_)) {
            return Ok(());
        }

        // Filter out non-root components
        if self.metadata().parent_id().is_some() {
            return Ok(());
        }

        writeln!(f, "e{}.parentNode.removeChild(e{0});", self.metadata().id())
    }
}

// Boolean also returned because I really didn't want to allocate a new String here... maybe
// there's a better way? The bool denotes if the caller should use quotes when surrounding the
// text.
fn collapse_whitespace(input: &str) -> (Cow<str>, bool) {
    if input == "\n" {
        (Cow::Borrowed(" "), true)
    } else if input.contains('\n') {
        (Cow::Owned(format!("String.raw`{input}`")), false)
    } else {
        (Cow::Borrowed(input), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use decorous_frontend::ast::{Element, Location, Mustache, Text};
    use rslint_parser::{parse_text, SmolStr};

    macro_rules! test_lifecycle {
        ($node:expr, $cycle_func:ident, $expected:expr, $unbound_refs:expr) => {
            let mut out = Vec::new();
            assert!($node.$cycle_func(&mut out, $unbound_refs).is_ok());
            assert_eq!($expected, String::from_utf8(out).unwrap());
        };
    }

    #[test]
    fn can_write_basic_text_nodes() {
        let node = Node::new(
            NodeType::Text(Text("hello")),
            FragmentMetadata::new(0, None, Location::default()),
        );

        test_lifecycle!(node, init, "let e0;\n", &DeclaredVariables::new());
        test_lifecycle!(
            node,
            create,
            "e0 = document.createTextNode(\"hello\");\n",
            &DeclaredVariables::new()
        );
        test_lifecycle!(
            node,
            mount,
            "target.appendChild(e0);\n",
            &DeclaredVariables::new()
        );
        test_lifecycle!(
            node,
            detach,
            "e0.parentNode.removeChild(e0);\n",
            &DeclaredVariables::new()
        );
    }

    #[test]
    fn basic_elements_and_html_are_collapsed() {
        let node = Node::new(
            NodeType::Element(Element::new(
                "div",
                vec![],
                vec![
                    Node::new(
                        NodeType::Text(Text("text")),
                        FragmentMetadata::new(1, Some(0), Location::default()),
                    ),
                    Node::new(
                        NodeType::Element(Element::new("div", vec![], vec![])),
                        FragmentMetadata::new(2, Some(0), Location::default()),
                    ),
                ],
            )),
            FragmentMetadata::new(0, None, Location::default()),
        );

        test_lifecycle!(node, init, "let e0;\n", &DeclaredVariables::new());
        test_lifecycle!(
            node,
            create,
            "e0 = document.createElement(\"div\");\ne0.innerHtml = \"text<div></div>\"\n",
            &DeclaredVariables::new()
        );
        test_lifecycle!(
            node,
            mount,
            "target.appendChild(e0);\n",
            &DeclaredVariables::new()
        );
        test_lifecycle!(
            node,
            detach,
            "e0.parentNode.removeChild(e0);\n",
            &DeclaredVariables::new()
        );
    }

    #[test]
    fn can_write_mustache_tags() {
        let js = parse_text("(hi, hi)", 0).syntax().first_child().unwrap();
        let node = Node::new(
            NodeType::Mustache(Mustache(js)),
            FragmentMetadata::new(0, None, Location::default()),
        );

        let declared = {
            let mut vars = DeclaredVariables::new();
            vars.insert_var(SmolStr::new("hi"));
            vars
        };
        test_lifecycle!(node, init, "let e0;\n", &declared);
        test_lifecycle!(
            node,
            create,
            "e0 = document.createTextNode((ctx[0], ctx[0]));\n",
            &declared
        );
        test_lifecycle!(node, mount, "target.appendChild(e0);\n", &declared);
        test_lifecycle!(
            node,
            update,
            "if (dirty[0] & 1) e0.data = (ctx[0], ctx[0]);\n",
            &declared
        );
        test_lifecycle!(node, detach, "e0.parentNode.removeChild(e0);\n", &declared);
    }

    #[test]
    fn text_with_only_one_text_node_as_child_is_collapsed() {
        let node = Node::new(
            NodeType::Element(Element::new(
                "span",
                vec![],
                vec![Node::new(
                    NodeType::Text(Text("hello")),
                    FragmentMetadata::new(1, Some(0), Location::default()),
                )],
            )),
            FragmentMetadata::new(0, None, Location::default()),
        );

        test_lifecycle!(node, init, "let e0;\n", &DeclaredVariables::new());
        test_lifecycle!(
            node,
            create,
            "e0 = document.createElement(\"span\");\ne0.textContent = \"hello\";\n",
            &DeclaredVariables::new()
        );
        test_lifecycle!(
            node,
            mount,
            "target.appendChild(e0);\n",
            &DeclaredVariables::new()
        );
        test_lifecycle!(
            node,
            detach,
            "e0.parentNode.removeChild(e0);\n",
            &DeclaredVariables::new()
        );
    }

    #[test]
    fn dirty_items_are_in_conditional() {
        let js = parse_text("(hello, test)", 0)
            .syntax()
            .first_child()
            .unwrap();
        let node = Node::new(
            NodeType::Mustache(Mustache(js)),
            FragmentMetadata::new(0, None, Location::default()),
        );

        let declared = {
            let mut vars = DeclaredVariables::new();
            vars.insert_var(SmolStr::new("hello"));
            vars.insert_var(SmolStr::new("test"));
            vars
        };
        test_lifecycle!(
            node,
            update,
            "if (dirty[0] & 3) e0.data = (ctx[0], ctx[1]);\n",
            &declared
        );
    }
}
