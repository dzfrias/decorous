use decorous_frontend::ast::{Attribute, AttributeValue};
use rslint_parser::{SmolStr, SyntaxNode};
use std::collections::HashMap;
use std::fmt::Write;
use std::io;

use decorous_frontend::{
    ast::{Node, NodeType},
    utils, Component, FragmentMetadata,
};

use crate::{codegen_utils, replace};

#[derive(Debug)]
struct TransientNodeData<'a> {
    pub mustache_data: Vec<(usize, SyntaxNode)>,
    pub attr_data: Vec<AttrData<'a>>,
    pub event_listeners: Vec<Listener<'a>>,
}

#[derive(Debug)]
struct AttrData<'a> {
    pub attr: &'a str,
    pub js: SyntaxNode,
    pub elems_idx: usize,
}

#[derive(Debug)]
struct Listener<'a> {
    pub event: &'a str,
    pub js: SyntaxNode,
    pub elems_idx: usize,
}

#[derive(Debug)]
enum ElementData {
    Element { id: u32, child_pos: usize },
    Mustache { id: u32 },
    Attribute { id: u32 },
    Listener { id: u32 },
}

impl ElementData {
    pub fn id(&self) -> u32 {
        match self {
            Self::Element { id, .. } => *id,
            Self::Mustache { id } => *id,
            Self::Attribute { id } => *id,
            Self::Listener { id } => *id,
        }
    }
}

#[derive(Debug)]
pub struct Renderer<'a> {
    component: &'a Component<'a>,
    ctx_body: String,
    update_body: String,
    html: String,
    // Contains information for the `elems` array in the output JS. The first element of the tuple
    // corresponds to the id of the element. The second (if set) contains the index of the child
    // node to get a reference to. This corresponds to the index of a mustache tag in an element.
    elems: Vec<ElementData>,
    id_overwrites: HashMap<u32, SmolStr>,
}

impl<'a> Renderer<'a> {
    pub fn new(component: &'a Component<'a>) -> Self {
        Self {
            component,
            ctx_body: String::new(),
            update_body: String::new(),
            html: String::new(),
            elems: vec![],
            id_overwrites: HashMap::new(),
        }
    }

    pub fn render<T, U>(mut self, js_out: &mut T, html_out: &mut U) -> io::Result<()>
    where
        T: io::Write,
        U: io::Write,
    {
        self.build_render_sections();
        let elems = self.build_elems();

        // Write finished segments
        write!(
            js_out,
            include_str!("./template.js"),
            dirty_items = ((self.component.declared_vars().all_vars().len() + 7) / 8),
            elems = elems,
            ctx_body = self.ctx_body,
            update_body = self.update_body
        )?;
        write!(html_out, "{}", self.html)?;

        Ok(())
    }

    fn build_render_sections(&mut self) {
        self.start_ctx_body();
        for node in self.component.descendents() {
            if let Some(data) = self.get_node_data(node) {
                self.build_ctx_body(&data);
                self.build_update_body(&data);
            }
        }
        self.end_ctx_body();
        writeln!(self.html, "<script defer src=\"./out.js\"></script>")
            .expect("string format should not fail");
        self.build_html();
    }

    fn build_elems(&self) -> String {
        let mut elems = String::new();
        for element_data in &self.elems {
            let string_id = SmolStr::new(element_data.id().to_string());
            let id = self
                .id_overwrites
                .get(&element_data.id())
                .unwrap_or(&string_id);
            match element_data {
                ElementData::Element { child_pos, .. } => {
                    write!(
                        elems,
                        "replace(document.getElementById(\"{id}\").childNodes[{child_pos}]),"
                    )
                    .expect("string format should not fail");
                }
                ElementData::Mustache { .. } => {
                    write!(elems, "replace(document.getElementById(\"{id}\"))")
                        .expect("string format should not fail");
                }
                ElementData::Attribute { .. } | ElementData::Listener { .. } => {
                    write!(elems, "document.getElementById(\"{id}\"),")
                        .expect("string format should not fail");
                }
            }
        }
        elems
    }

    // One of the core methods of the renderer. It receives a node. If the node is not an element,
    // it returns None. Otherwise, it grabs some data about the node. This data contains
    // instructions for how to update the element. If it has not special mustache tags, this data
    // will contain nothing. With dynamic attributes, this method returns information about that
    // attribute, such as it's name, js-generated value, and index in the `elems` array (on the
    // Javascript side). With a dynamic text node, it contains that node's js-generated value and
    // it's index in the `elems` array.
    //
    // It also mutates `self.elems`.
    fn get_node_data(
        &mut self,
        node: &'a Node<'_, FragmentMetadata>,
    ) -> Option<TransientNodeData<'a>> {
        match node.node_type() {
            NodeType::Element(elem) => {
                let mut attr_data = vec![];
                let mut listeners = vec![];
                for attr in elem.attrs() {
                    match attr {
                        Attribute::KeyValue(key, Some(AttributeValue::JavaScript(js))) => {
                            self.elems.push(ElementData::Attribute {
                                id: node.metadata().id(),
                            });
                            let idx = self.elems.len() - 1;
                            attr_data.push(AttrData {
                                attr: *key,
                                js: js.clone(),
                                elems_idx: idx,
                            });
                        }
                        Attribute::EventHandler(event_handler) => {
                            self.elems.push(ElementData::Listener {
                                id: node.metadata().id(),
                            });
                            let idx = self.elems.len() - 1;
                            listeners.push(Listener {
                                event: event_handler.event(),
                                js: event_handler.expr().clone(),
                                elems_idx: idx,
                            });
                        }
                        // TODO: Possibly bindings?
                        _ => {}
                    }
                }

                let mut elems_indices = vec![];
                for (pos, mustache) in
                    elem.children()
                        .iter()
                        .enumerate()
                        .filter_map(|(idx, node)| match node.node_type() {
                            NodeType::Mustache(mustache) => Some((idx, mustache)),
                            NodeType::Text(_) | NodeType::Comment(_) | NodeType::Element(_) => None,
                            NodeType::SpecialBlock(_) => todo!("special blocks"),
                            NodeType::Error => panic!("should not have error nodes at this point"),
                        })
                {
                    self.elems.push(ElementData::Element {
                        id: node.metadata().id(),
                        child_pos: pos,
                    });
                    let idx = self.elems.len() - 1;
                    elems_indices.push((idx, mustache.clone()));
                }
                Some(TransientNodeData {
                    mustache_data: elems_indices,
                    attr_data,
                    event_listeners: listeners,
                })
            }
            NodeType::Mustache(mustache) if node.metadata().parent_id().is_none() => {
                self.elems.push(ElementData::Mustache {
                    id: node.metadata().id(),
                });
                Some(TransientNodeData {
                    mustache_data: vec![(self.elems.len() - 1, mustache.clone())],
                    attr_data: vec![],
                    event_listeners: vec![],
                })
            }
            _ => None,
        }
    }

    fn build_ctx_body(&mut self, data: &TransientNodeData<'_>) {
        for (idx, js) in &data.mustache_data {
            writeln!(self.ctx_body, "elems[{idx}].data = {js};")
                .expect("writing to string format should not fail");
        }

        for AttrData {
            attr,
            js,
            elems_idx,
        } in &data.attr_data
        {
            writeln!(
                self.ctx_body,
                "elems[{elems_idx}].setAttribute(\"{attr}\", {js});"
            )
            .expect("writing to format string should not fail");
        }

        for Listener {
            event,
            js,
            elems_idx,
        } in &data.event_listeners
        {
            let replaced = replace::replace_assignments(
                js,
                &utils::get_unbound_refs(js),
                self.component.declared_vars(),
            );
            writeln!(
                self.ctx_body,
                "elems[{elems_idx}].addEventListener(\"{event}\", {replaced});"
            )
            .expect("writing to format string should not fail");
        }
    }

    fn build_update_body(&mut self, data: &TransientNodeData<'_>) {
        for (idx, js) in &data.mustache_data {
            let unbound = utils::get_unbound_refs(js);
            let dirty_indices = codegen_utils::calc_dirty(&unbound, self.component.declared_vars());
            let replaced = replace::replace_namerefs(&js, &unbound, self.component.declared_vars());
            writeln!(
                self.update_body,
                "if ({dirty_indices}) elems[{idx}].data = {replaced};",
            )
            .expect("writing to string format should not fail");
        }

        for AttrData {
            attr,
            js,
            elems_idx,
        } in &data.attr_data
        {
            let unbound = utils::get_unbound_refs(js);
            let dirty_indices = codegen_utils::calc_dirty(&unbound, self.component.declared_vars());
            let replaced = replace::replace_namerefs(&js, &unbound, self.component.declared_vars());
            writeln!(
                self.update_body,
                "if ({dirty_indices}) elems[{elems_idx}].setAttribute(\"{attr}\", {replaced});",
            )
            .expect("writing to format string should not fail");
        }
    }

    fn start_ctx_body(&mut self) {
        for node in self.component.toplevel_nodes() {
            if node.substitute_assign_refs {
                let replacement = replace::replace_assignments(
                    &node.node,
                    &utils::get_unbound_refs(&node.node),
                    self.component.declared_vars(),
                );
                writeln!(self.ctx_body, "{}", replacement)
                    .expect("string formatting should not fail");
            } else {
                writeln!(self.ctx_body, "{}", node.node)
                    .expect("string formatting should not fail");
            }
        }
    }

    fn end_ctx_body(&mut self) {
        let mut ctx = vec![""; self.component.declared_vars().all_vars().len()];
        for (name, idx) in self.component.declared_vars().all_vars() {
            ctx[*idx as usize] = name;
        }
        writeln!(self.ctx_body, "return [{}];", ctx.join(","))
            .expect("string format should not fail");
    }

    // Builds the HTML part of the output
    fn build_html(&mut self) {
        for node in self.component.fragment_tree() {
            self.build_html_for_node(node);
        }
    }

    fn build_html_for_node(&mut self, node: &Node<'_, FragmentMetadata>) {
        match node.node_type() {
            NodeType::Text(t) => write!(self.html, "{t}").expect("string format should not fail"),
            NodeType::Comment(t) => {
                write!(self.html, "<!--{t}-->").expect("string format should not fail")
            }
            NodeType::Mustache(_) if node.metadata().parent_id().is_none() => {
                write!(self.html, "<span id=\"{}\"></span>", node.metadata().id())
                    .expect("string format should not fail")
            }
            // Write a span. This segments the childNodes of the element so that an actual text
            // node can be placed here.
            NodeType::Mustache(_) => {
                write!(self.html, "<span></span>").expect("string format should not fail")
            }
            NodeType::Element(elem) => {
                write!(self.html, "<{}", elem.tag()).expect("string format should not fail");
                let mut has_dynamic = false;
                for attr in elem.attrs() {
                    match attr {
                        Attribute::KeyValue(key, Some(AttributeValue::Literal(literal))) => {
                            if *key == "id" {
                                // The user has set this id, so use it from now on instead of a
                                // pre-generated one
                                self.id_overwrites
                                    .insert(node.metadata().id(), literal.into());
                            }
                            write!(self.html, " {key}=\"{literal}\"")
                                .expect("string format should not fail");
                        }
                        // Do nothing. Dynamic attributes can't be baked statically into the HTML
                        Attribute::KeyValue(_, Some(AttributeValue::JavaScript(_)))
                        | Attribute::EventHandler(_) => has_dynamic = true,
                        Attribute::KeyValue(key, None) => {
                            write!(self.html, " {key}=\"\" ")
                                .expect("string format should not fail");
                        }
                        Attribute::Binding(_) => todo!(),
                    }
                }
                if (elem.has_immediate_mustache() || has_dynamic)
                    && !self.id_overwrites.contains_key(&node.metadata().id())
                {
                    write!(self.html, " id=\"{}\"", node.metadata().id())
                        .expect("string format should not fail");
                }
                write!(self.html, ">").expect("string format should not fail");
                for child in elem.children() {
                    self.build_html_for_node(child);
                }
                write!(self.html, "</{}>", elem.tag()).expect("string format should not fail");
            }
            NodeType::SpecialBlock(_) => todo!("special blocks"),
            NodeType::Error => panic!("should not try to render with error nodes"),
        }
    }
}

#[cfg(test)]
mod tests {
    use decorous_frontend::parse;

    use super::*;

    fn make_component(input: &str) -> Component {
        Component::new(parse(input).expect("should be valid input"))
    }

    macro_rules! test_render {
        ($($input:expr),+) => {
            $(
                let component = make_component($input);
                let mut js_out = Vec::new();
                let mut html_out = Vec::new();
                let renderer = Renderer::new(&component);
                renderer.render(&mut js_out, &mut html_out).unwrap();
                insta::assert_snapshot!(format!("{}\n---\n{}", String::from_utf8(js_out).unwrap(), String::from_utf8(html_out).unwrap()));
             )+
        };
    }

    #[test]
    fn can_write_basic_html_from_fragment_tree_ignoring_mustache_tags() {
        test_render!("#p Hello /p", "#div #p Hi /p Hello, {name} /div");
    }

    #[test]
    fn can_write_basic_js() {
        test_render!(
            "---js let x = 3; --- #p Hello, {x}! /p #button[@click={() => x = 444}] Click Me /button",
            "---js let x = 3; --- #p Hello, {x}! /p #button[@click={() => x = 444}] Click Me /button #p {x} /p"
        );
    }

    #[test]
    fn custom_ids_do_not_get_overriden() {
        test_render!("---js let x = 3;--- #p[id=\"custom\"] Hello, {x}! /p");
    }

    #[test]
    fn can_create_dynamic_attributes() {
        test_render!(
            "---js let x = 3; --- #p[class={x + 3}] Text /p",
            "---js let x = 3; --- #p[class={x + 3}] Hello {x} /p"
        );
    }

    #[test]
    fn supports_toplevel_mustache_tags() {
        test_render!("---js let x = 3; --- {x}");
    }

    #[test]
    fn multiple_variables_are_properly_in_dirty_buffer() {
        test_render!("---js let x = 0; let y = 0; --- #p {x} and {y} and {x + y} /p");
    }
}
