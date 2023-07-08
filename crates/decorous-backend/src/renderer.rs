use decorous_frontend::{
    ast::{Attribute, AttributeValue, CollapsedChildrenType, Node, NodeType},
    utils, FragmentMetadata,
};
use itertools::Itertools;
use rslint_parser::SmolStr;
use std::{collections::HashMap, io};

use crate::replace;

pub trait Renderer<T>
where
    T: io::Write,
{
    fn init(&self, f: &mut T, toplevel_vars: &HashMap<SmolStr, u32>) -> io::Result<()>;
    fn create(&self, f: &mut T, toplevel_vars: &HashMap<SmolStr, u32>) -> io::Result<()>;
    fn mount(&self, f: &mut T, toplevel_vars: &HashMap<SmolStr, u32>) -> io::Result<()>;
    fn update(&self, f: &mut T, toplevel_vars: &HashMap<SmolStr, u32>) -> io::Result<()>;
    fn detach(&self, f: &mut T, toplevel_vars: &HashMap<SmolStr, u32>) -> io::Result<()>;
}

impl<'a, T> Renderer<T> for Node<'a, FragmentMetadata>
where
    T: io::Write,
{
    fn init(&self, f: &mut T, toplevel_vars: &HashMap<SmolStr, u32>) -> io::Result<()> {
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

    fn create(&self, f: &mut T, toplevel_vars: &HashMap<SmolStr, u32>) -> io::Result<()> {
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
                                    "e{}.setAttribute(\"{key}\", String.raw`{literal}`);",
                                    self.metadata().id(),
                                )?;
                            }
                            Some(AttributeValue::JavaScript(js)) => writeln!(
                                f,
                                "e{}.setAttribute(\"{key}\", {});",
                                self.metadata().id(),
                                replace::replace_namerefs(
                                    &js,
                                    &utils::get_unbound_refs(&js),
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
                                replace::replace_namerefs(
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
                            writeln!(
                                f,
                                "e{}.textContent = String.raw`{t}`;",
                                self.metadata().id(),
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
                writeln!(
                    f,
                    "e{} = document.createTextNode(String.raw`{text}`);",
                    self.metadata().id(),
                )
            }
            NodeType::Mustache(mustache) => {
                let new_text = replace::replace_namerefs(
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
            NodeType::Error => panic!("should not have an error node during rendering phase"),
            NodeType::SpecialBlock(_) => todo!(),
        }
    }

    fn mount(&self, f: &mut T, toplevel_vars: &HashMap<SmolStr, u32>) -> io::Result<()> {
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

    fn update(&self, f: &mut T, toplevel_vars: &HashMap<SmolStr, u32>) -> io::Result<()> {
        match self.node_type() {
            NodeType::Element(elem) => {
                for attr in elem.attrs() {
                    match attr {
                        Attribute::KeyValue(key, Some(AttributeValue::JavaScript(js))) => writeln!(
                            f,
                            "e{}.setAttribute(\"{key}\", {});",
                            self.metadata().id(),
                            replace::replace_namerefs(
                                js,
                                &utils::get_unbound_refs(js),
                                toplevel_vars
                            )
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
                let unbound = utils::get_unbound_refs(mustache);
                let mut dirty_indices: Vec<(usize, u8)> = vec![];
                for unbound in &unbound {
                    let Some(ident) = unbound.ident_token().map(|tok| tok.text().to_owned()) else {
                        continue;
                    };
                    let Some(idx) = toplevel_vars.get(&ident) else {
                        continue;
                    };
                    // Get the byte index for the dirty bitmap. Need to subtract one because
                    // ceiling division only results in 0 if x == 0.
                    let dirty_idx = ((idx + 7) / 8).saturating_sub(1) as usize;

                    // Modulo 8 so it wraps every byte. The byte is tracked by dirty_idx
                    let bitmask = 1 << (*idx % 8);
                    if let Some(pos) = dirty_indices.iter().position(|(idx, _)| *idx == dirty_idx) {
                        dirty_indices[pos].1 |= bitmask;
                    } else {
                        dirty_indices.push((dirty_idx, bitmask))
                    }
                }
                let new_text = replace::replace_namerefs(mustache, &unbound, toplevel_vars);
                writeln!(
                    f,
                    "if ({}) e{}.data = {new_text};",
                    dirty_indices
                        .iter()
                        .map(|(idx, bitmask)| format!("dirty[{idx}] & {bitmask}"))
                        .join(" || "),
                    self.metadata().id()
                )
            }
            _ => Ok(()),
        }
    }

    fn detach(&self, f: &mut T, _toplevel_vars: &HashMap<SmolStr, u32>) -> io::Result<()> {
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
            "e0 = document.createTextNode(String.raw`hello`);\n",
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
            "e0 = document.createElement(\"span\");\ne0.textContent = String.raw`hello`;\n",
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
    fn dirty_items_are_in_conditional() {
        let js = parse_text("(hello, test)", 0)
            .syntax()
            .first_child()
            .unwrap();
        let node = Node::new(
            NodeType::Mustache(js),
            FragmentMetadata::new(0, None, Location::new(0, 0)),
        );

        let declared = HashMap::from([(SmolStr::new("hello"), 0), (SmolStr::new("test"), 1)]);
        test_lifecycle!(
            node,
            update,
            "if (dirty[0] & 3) e0.data = (ctx[0], ctx[1]);\n",
            &declared
        );
    }
}
