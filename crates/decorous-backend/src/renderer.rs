use decorous_frontend::{
    ast::{Attribute, AttributeValue, CollapsedChildrenType, Node, NodeType},
    FragmentMetadata,
};
use rslint_parser::SmolStr;
use std::{collections::HashMap, io};

use crate::replace;

pub trait Renderer<T>
where
    T: io::Write,
{
    fn init(&self, f: &mut T, toplevel_vars: &HashMap<SmolStr, u64>) -> io::Result<()>;
    fn create(&self, f: &mut T, toplevel_vars: &HashMap<SmolStr, u64>) -> io::Result<()>;
    fn mount(&self, f: &mut T, toplevel_vars: &HashMap<SmolStr, u64>) -> io::Result<()>;
    fn update(&self, f: &mut T, toplevel_vars: &HashMap<SmolStr, u64>) -> io::Result<()>;
    fn detach(&self, f: &mut T, toplevel_vars: &HashMap<SmolStr, u64>) -> io::Result<()>;
}

impl<'a, T> Renderer<T> for Node<'a, FragmentMetadata>
where
    T: io::Write,
{
    fn init(&self, f: &mut T, toplevel_vars: &HashMap<SmolStr, u64>) -> io::Result<()> {
        match self.node_type() {
            NodeType::Element(element) => {
                writeln!(f, "let e{};", self.metadata().id())?;

                if element.inner_collapsed().is_some() {
                    return Ok(());
                }
                for child in element.children() {
                    child.init(f, toplevel_vars)?;
                }
                Ok(())
            }
            NodeType::Text(_) | NodeType::Mustache(_) => {
                writeln!(f, "let e{};", self.metadata().id())
            }
            NodeType::Comment(_) => Ok(()),
            NodeType::SpecialBlock(_) => todo!(),
            NodeType::Error => panic!("should not have an error node during rendering phase"),
        }
    }

    fn create(&self, f: &mut T, toplevel_vars: &HashMap<SmolStr, u64>) -> io::Result<()> {
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
                                writeln!(
                                    f,
                                    "e{}.setAttribute(\"{key}\", \"{literal}\");",
                                    self.metadata().id(),
                                )?;
                            }
                            Some(AttributeValue::JavaScript(js)) => writeln!(
                                f,
                                "e{}.setAttribute(\"{key}\", {});",
                                self.metadata().id(),
                                replace::replace_namerefs(&js, toplevel_vars)
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
                                replace::replace_namerefs(event_handler.expr(), toplevel_vars)
                            )?;
                        }
                        Attribute::Binding(_binding) => todo!(),
                    }
                }

                if let Some(collapsed) = element.inner_collapsed() {
                    return match collapsed {
                        CollapsedChildrenType::Text(t) => {
                            writeln!(f, "e{}.textContent = \"{t}\";", self.metadata().id())
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
                writeln!(
                    f,
                    "e{} = document.createTextNode(\"{}\");",
                    self.metadata().id(),
                    text
                )
            }
            NodeType::Mustache(mustache) => {
                let new_text = replace::replace_namerefs(mustache, toplevel_vars);
                writeln!(
                    f,
                    "e{} = document.createTextNode({new_text});",
                    self.metadata().id()
                )
            }
            NodeType::Comment(_) => Ok(()),
            NodeType::Error => panic!("should not have an error node during rendering phase"),
            NodeType::SpecialBlock(_) => todo!(),
        }
    }

    fn mount(&self, f: &mut T, toplevel_vars: &HashMap<SmolStr, u64>) -> io::Result<()> {
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
                    child.mount(f, toplevel_vars)?;
                }
            }
            NodeType::Error => panic!("should not have an error node during rendering phase"),
            NodeType::SpecialBlock(_) => todo!(),
            _ => {}
        }

        Ok(())
    }

    fn update(&self, f: &mut T, toplevel_vars: &HashMap<SmolStr, u64>) -> io::Result<()> {
        match self.node_type() {
            NodeType::Element(elem) => {
                for attr in elem.attrs() {
                    match attr {
                        Attribute::KeyValue(key, Some(AttributeValue::JavaScript(js))) => writeln!(
                            f,
                            "e{}.setAttribute(\"{key}\", {});",
                            self.metadata().id(),
                            replace::replace_namerefs(js, toplevel_vars)
                        )?,
                        _ => {}
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
                let new_text = replace::replace_namerefs(mustache, toplevel_vars);
                writeln!(f, "e{}.data = {new_text};", self.metadata().id())
            }
            _ => Ok(()),
        }
    }

    fn detach(&self, f: &mut T, _toplevel_vars: &HashMap<SmolStr, u64>) -> io::Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use decorous_frontend::ast::{Element, Location};
    use rslint_parser::parse_text;

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
            NodeType::Text("hello"),
            FragmentMetadata::new(0, None, Location::new(0, 1)),
        );

        test_lifecycle!(node, init, "let e0;\n", &HashMap::new());
        test_lifecycle!(
            node,
            create,
            "e0 = document.createTextNode(\"hello\");\n",
            &HashMap::new()
        );
        test_lifecycle!(node, mount, "target.appendChild(e0);\n", &HashMap::new());
        test_lifecycle!(
            node,
            detach,
            "e0.parentNode.removeChild(e0);\n",
            &HashMap::new()
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
                        NodeType::Text("text"),
                        FragmentMetadata::new(1, Some(0), Location::new(0, 0)),
                    ),
                    Node::new(
                        NodeType::Element(Element::new("div", vec![], vec![])),
                        FragmentMetadata::new(2, Some(0), Location::new(0, 0)),
                    ),
                ],
            )),
            FragmentMetadata::new(0, None, Location::new(0, 0)),
        );

        test_lifecycle!(node, init, "let e0;\n", &HashMap::new());
        test_lifecycle!(
            node,
            create,
            "e0 = document.createElement(\"div\");\ne0.innerHtml = \"text<div></div>\"\n",
            &HashMap::new()
        );
        test_lifecycle!(node, mount, "target.appendChild(e0);\n", &HashMap::new());
        test_lifecycle!(
            node,
            detach,
            "e0.parentNode.removeChild(e0);\n",
            &HashMap::new()
        );
    }

    #[test]
    fn can_write_mustache_tags() {
        let js = parse_text("(hi, hi)", 0).syntax().first_child().unwrap();
        let node = Node::new(
            NodeType::Mustache(js),
            FragmentMetadata::new(0, None, Location::new(0, 0)),
        );

        let declared = HashMap::from([(SmolStr::new("hi"), 0)]);
        test_lifecycle!(node, init, "let e0;\n", &declared);
        test_lifecycle!(
            node,
            create,
            "e0 = document.createTextNode((ctx[0], ctx[0]));\n",
            &declared
        );
        test_lifecycle!(node, mount, "target.appendChild(e0);\n", &declared);
        test_lifecycle!(node, update, "e0.data = (ctx[0], ctx[0]);\n", &declared);
        test_lifecycle!(node, detach, "e0.parentNode.removeChild(e0);\n", &declared);
    }

    #[test]
    fn text_with_only_one_text_node_as_child_is_collapsed() {
        let node = Node::new(
            NodeType::Element(Element::new(
                "span",
                vec![],
                vec![Node::new(
                    NodeType::Text("hello"),
                    FragmentMetadata::new(1, Some(0), Location::new(0, 0)),
                )],
            )),
            FragmentMetadata::new(0, None, Location::new(0, 0)),
        );

        test_lifecycle!(node, init, "let e0;\n", &HashMap::new());
        test_lifecycle!(
            node,
            create,
            "e0 = document.createElement(\"span\");\ne0.textContent = \"hello\";\n",
            &HashMap::new()
        );
        test_lifecycle!(node, mount, "target.appendChild(e0);\n", &HashMap::new());
        test_lifecycle!(
            node,
            detach,
            "e0.parentNode.removeChild(e0);\n",
            &HashMap::new()
        );
    }
}
