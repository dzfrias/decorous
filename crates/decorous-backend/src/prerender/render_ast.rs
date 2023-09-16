use std::{
    borrow::Cow,
    collections::HashMap,
    fmt::{Display, Write as FmtWrite},
    io::Write,
};

use crate::{
    codegen_utils,
    dom_render::{render_fragment as dom_render_fragment, State as DomRenderState},
};
use decorous_frontend::{
    ast::{
        Attribute, AttributeValue, Comment, Element, ForBlock, IfBlock, Mustache, Node, NodeType,
        SpecialBlock, Text, UseBlock,
    },
    utils, Component, FragmentMetadata,
};
use heck::ToSnekCase;
use rslint_parser::{SmolStr, SyntaxNode};

#[derive(Debug, Default)]
pub struct Output {
    pub html: Vec<u8>,
    pub elements: Vec<u8>,
    pub ctx_init: Vec<u8>,
    pub updates: Vec<u8>,
    pub hoists: Vec<u8>,
}

impl Output {
    fn write_html(&mut self, b: impl Display) {
        let _ = write!(self.html, "{b}");
    }

    fn write_ctx_initln(&mut self, b: impl Display) {
        let _ = writeln!(self.ctx_init, "  {b}");
    }

    fn write_updateln(&mut self, b: impl Display) {
        let _ = writeln!(self.updates, "  {b}");
    }

    fn write_element(&mut self, key: impl Display, val: impl Display) {
        let _ = write!(self.elements, "\"{key}\": {val}, ");
    }
}

#[derive(Debug)]
pub struct State<'ast> {
    pub component: &'ast Component<'ast>,
    pub id_overwrites: HashMap<u32, SmolStr>,
    pub style_cache: Option<String>,
    pub uses: Vec<Cow<'ast, str>>,
}

impl<'ast> State<'ast> {
    fn use_style_cache(&mut self) -> &str {
        if let Some(ref style) = self.style_cache {
            style.as_str()
        } else {
            let style = {
                // The minimum length of each part of the eventual style
                const MIN_LEN: usize = "--decor-0: ${}; ".len();
                let mut style = String::with_capacity(
                    self.component.declared_vars().css_mustaches().len() * MIN_LEN,
                );
                for (mustache, id) in self.component.declared_vars().css_mustaches() {
                    crate::codegen_utils::force_write!(style, "--decor-{id}: ${{{mustache}}}; ");
                }
                style
            };
            self.style_cache = Some(style);
            self.style_cache.as_ref().unwrap().as_str()
        }
    }
}

macro_rules! with_id {
    ($id:expr, $state:expr, $exec:expr) => {
        #[allow(unused)]
        if let Some(id) = $state.id_overwrites.get(&$id).cloned() {
            #[allow(clippy::redundant_closure_call)]
            $exec(&id);
        } else {
            #[allow(clippy::redundant_closure_call)]
            $exec($id);
        }
    };
}

pub trait Render<'ast> {
    type Metadata;

    fn render(&'ast self, state: &mut State<'ast>, out: &mut Output, meta: &Self::Metadata);
}

impl<'ast> Render<'ast> for Node<'ast, FragmentMetadata> {
    type Metadata = ();

    fn render(&'ast self, state: &mut State<'ast>, out: &mut Output, _meta: &Self::Metadata) {
        match self.node_type() {
            NodeType::Element(elem) => elem.render(state, out, self.metadata()),
            NodeType::Text(t) => t.render(state, out, self.metadata()),
            NodeType::Comment(c) => c.render(state, out, self.metadata()),
            NodeType::SpecialBlock(block) => block.render(state, out, self.metadata()),
            NodeType::Mustache(m) => m.render(state, out, self.metadata()),
        }
    }
}

impl<'ast> Render<'ast> for Text<'ast> {
    type Metadata = FragmentMetadata;

    fn render(&'ast self, _state: &mut State<'ast>, out: &mut Output, _meta: &Self::Metadata) {
        out.write_html(self);
    }
}

impl<'ast> Render<'ast> for Element<'ast, FragmentMetadata> {
    type Metadata = FragmentMetadata;

    fn render(&'ast self, state: &mut State<'ast>, out: &mut Output, meta: &Self::Metadata) {
        let id = meta.id();

        let js_tag_name = self.js_valid_tag_name();
        if state.uses.contains(&js_tag_name) {
            out.write_html(format_args!("<span id=\"{id}\"></span>"));
            out.write_element(
                id,
                format_args!("replace(document.getElementById(\"{id}\"))"),
            );
            out.write_ctx_initln(format_args!(
                "__decor_{js_tag_name}(elems[\"{id}\"].parentNode, elems[\"{id}\"])",
            ));

            return;
        }

        out.write_html(format_args!("<{}", self.tag()));
        let mut overwritten = false;
        let mut has_dynamic = false;
        let mut has_style = false;
        for attr in self.attrs() {
            attr.render(state, out, meta);
            match attr {
                Attribute::KeyValue(key, Some(AttributeValue::Literal(literal)))
                    if *key == "id" =>
                {
                    state.id_overwrites.insert(meta.id(), SmolStr::new(literal));
                    overwritten = true;
                }
                Attribute::KeyValue(key, Some(AttributeValue::JavaScript(_)))
                    if *key == "style" =>
                {
                    has_style = true;
                    has_dynamic = true;
                }
                Attribute::KeyValue(key, Some(AttributeValue::Literal(_))) if *key == "style" => {
                    has_style = true;
                }
                Attribute::KeyValue(_, Some(AttributeValue::JavaScript(_)))
                | Attribute::EventHandler(_)
                | Attribute::Binding(_) => has_dynamic = true,
                Attribute::KeyValue(_, None | Some(AttributeValue::Literal(_))) => {}
            }
        }
        if meta.parent_id().is_none() && !state.component.declared_vars().css_mustaches().is_empty()
        {
            has_dynamic = true;
        }

        let inline_styles_candidate = meta.parent_id().is_none()
            && !state.component.declared_vars().css_mustaches().is_empty();
        if !has_style && inline_styles_candidate {
            let style = state.use_style_cache();
            let new_js = rslint_parser::parse_text(&format!("`{style}`"), 0).syntax();
            render_dyn_attr(meta, state, out, "style", &new_js);
        }

        if !overwritten && has_dynamic {
            out.write_html(format_args!(" id=\"{id}\""));
        }
        out.write_html(">");
        for child in self.children() {
            child.render(state, out, &());
        }
        out.write_html(format_args!("</{}>", self.tag()));
    }
}

impl<'ast> Render<'ast> for Comment<'ast> {
    type Metadata = FragmentMetadata;

    fn render(&'ast self, _state: &mut State<'ast>, out: &mut Output, _meta: &Self::Metadata) {
        out.write_html(format_args!("<!--{}-->", self.0));
    }
}

impl<'ast> Render<'ast> for Mustache {
    type Metadata = FragmentMetadata;

    fn render(&'ast self, state: &mut State<'ast>, out: &mut Output, meta: &Self::Metadata) {
        let id = meta.id();
        out.write_html(format_args!("<span id=\"{id}\"></span>"));
        out.write_element(
            id,
            format_args!("replace(document.getElementById(\"{id}\"))"),
        );

        let unbound = utils::get_unbound_refs(&self.0);
        let dirty_indices =
            codegen_utils::calc_dirty(&unbound, state.component.declared_vars(), meta.scope());
        let replaced = codegen_utils::replace_namerefs(
            &self.0,
            &unbound,
            state.component.declared_vars(),
            meta.scope(),
        );
        if dirty_indices.is_empty() {
            out.write_updateln(format_args!("if (initial) elems[{id}].data = {replaced};"));
        } else {
            out.write_updateln(format_args!(
                "if ({dirty_indices}) elems[{id}].data = {replaced};"
            ));
        }
    }
}

impl<'ast> Render<'ast> for SpecialBlock<'ast, FragmentMetadata> {
    type Metadata = FragmentMetadata;

    fn render(&'ast self, state: &mut State<'ast>, out: &mut Output, meta: &Self::Metadata) {
        match self {
            Self::If(block) => block.render(state, out, meta),
            SpecialBlock::For(block) => block.render(state, out, meta),
            SpecialBlock::Use(use_decl) => use_decl.render(state, out, meta),
        }
    }
}

impl<'ast> Render<'ast> for UseBlock<'ast> {
    type Metadata = FragmentMetadata;

    fn render(&'ast self, state: &mut State<'ast>, _out: &mut Output, _meta: &Self::Metadata) {
        let Some(name) = self.path().file_stem() else {
            return;
        };

        state.uses.push(match <&str>::try_from(name) {
            Ok(s) => {
                if s.contains('-') {
                    s.to_snek_case().into()
                } else {
                    s.into()
                }
            }
            Err(_err) => todo!("error"),
        });
    }
}

impl<'ast> Render<'ast> for IfBlock<'ast, FragmentMetadata> {
    type Metadata = FragmentMetadata;

    fn render(&'ast self, state: &mut State<'ast>, out: &mut Output, meta: &Self::Metadata) {
        let id = meta.id();
        let unbound = utils::get_unbound_refs(self.expr());
        let replaced = codegen_utils::replace_namerefs(
            self.expr(),
            &unbound,
            state.component.declared_vars(),
            meta.scope(),
        );

        out.write_html(format_args!("<span id=\"{id}\"></span>"));

        out.write_element(
            id,
            format_args!("replace(document.getElementById(\"{id}\"))"),
        );
        out.write_element(format_args!("{id}_block"), "null");

        let state = DomRenderState {
            component: state.component,
            name: meta.id().to_string().into(),
            root: Some(meta.id()),
            uses: vec![],
        };
        let _ = dom_render_fragment(self.inner(), state.clone(), &mut out.hoists);

        if let Some(else_block) = self.else_block() {
            out.write_element(format_args!("{id}_on"), "true");
            out.write_updateln(format_args!(
                include_str!("./templates/if.js"),
                replaced = replaced,
                id = id
            ));
            // Write else block to hoists
            let state = DomRenderState {
                component: state.component,
                name: format!("{}_else", meta.id()).into(),
                root: Some(meta.id()),
                uses: vec![],
            };
            let _ = dom_render_fragment(else_block, state, &mut out.hoists);
        } else {
            out.write_updateln(format_args!("if ({replaced}) {{ if (elems[\"{id}_block\"]) {{ elems[\"{id}_block\"].u(dirty); }} else {{ elems[\"{id}_block\"] = create_{id}_block(elems[\"{id}\"].parentNode, elems[\"{id}\"]); }} }} else if (elems[\"{id}_block\"]) {{ elems[\"{id}_block\"].d(); elems[\"{id}_block\"] = null; }}"));
        }
    }
}

impl<'ast> Render<'ast> for ForBlock<'ast, FragmentMetadata> {
    type Metadata = FragmentMetadata;

    fn render(&'ast self, state: &mut State<'ast>, out: &mut Output, meta: &Self::Metadata) {
        let id = meta.id();
        let unbound = utils::get_unbound_refs(self.expr());
        let replaced = codegen_utils::replace_namerefs(
            self.expr(),
            &unbound,
            state.component.declared_vars(),
            meta.scope(),
        );
        let var_idx = state
            .component
            .declared_vars()
            .all_scopes()
            .get(&meta.id())
            .expect("BUG: for block should have an assigned scope")
            .get(self.binding())
            .expect("BUG: for block's scope should contain the binding");

        out.write_html(format_args!("<span id=\"{id}\"></span>"));
        out.write_element(
            id,
            format_args!("replace(document.getElementById(\"{id}\"))"),
        );
        out.write_element(format_args!("{id}_block"), "[]");

        let state = DomRenderState {
            component: state.component,
            name: meta.id().to_string().into(),
            root: Some(meta.id()),
            uses: vec![],
        };
        let _ = dom_render_fragment(self.inner(), state, &mut out.hoists);

        out.write_updateln(format_args!("let i = 0; for (const v of ({replaced})) {{ ctx[{var_idx}] = v; if (i >= elems[\"{id}_block\"].length) {{ elems[\"{id}_block\"][i] = create_{id}_block(elems[\"{id}\"].parentNode, elems[\"{id}\"]); }} elems[\"{id}_block\"][i].u(dirty); i += 1; }} elems[\"{id}_block\"].slice(i).forEach((b) => b.d()); elems[\"{id}_block\"].length = i;"));
    }
}

impl<'ast> Render<'ast> for Attribute<'ast> {
    type Metadata = FragmentMetadata;

    fn render(&'ast self, state: &mut State<'ast>, out: &mut Output, meta: &Self::Metadata) {
        let id = meta.id();
        let inline_styles_candidate = meta.parent_id().is_none()
            && !state.component.declared_vars().css_mustaches().is_empty();

        match self {
            Attribute::KeyValue(key, Some(AttributeValue::Literal(literal))) => {
                if *key == "style" && inline_styles_candidate {
                    let style = state.use_style_cache();
                    let new_js =
                        rslint_parser::parse_text(&format!("`{literal} {style}`"), 0).syntax();
                    render_dyn_attr(meta, state, out, "style", &new_js);
                }
                out.write_html(format_args!(" {key}=\"{literal}\""));
            }
            Attribute::KeyValue(key, None) => {
                out.write_html(format_args!(" {key}=\"\""));
            }
            Attribute::EventHandler(evt_handler) => {
                with_id!(id, state, |id| {
                    let replaced = codegen_utils::replace_assignments(
                        evt_handler.expr(),
                        &utils::get_unbound_refs(evt_handler.expr()),
                        state.component.declared_vars(),
                        None,
                    );

                    out.write_element(id, format_args!("document.getElementById(\"{id}\")"));
                    out.write_ctx_initln(format_args!(
                        "elems[\"{id}\"].addEventListener(\"{}\", {replaced});",
                        evt_handler.event()
                    ));
                });
            }
            Attribute::Binding(binding) => {
                with_id!(id, state, |id| {
                    out.write_element(id, format_args!("document.getElementById(\"{id}\")"));
                    let binding_id = state
                        .component
                        .declared_vars()
                        .get_binding(*binding)
                        .expect("BUG: every binding should have an id in declared vars");
                    let Some(var_id) = state.component.declared_vars().get_var(*binding, None)
                    else {
                        todo!("unbound var lint")
                    };

                    out.write_ctx_initln(format_args!("elems[\"{id}\"].value = {binding};"));
                    out.write_ctx_initln(format_args!("let __binding{binding_id} = (ev) => __schedule_update({var_id}, {binding} = ev.target.value);"));
                    out.write_ctx_initln(format_args!(
                        "elems[\"{id}\"].addEventListener(\"input\", __binding{binding_id});"
                    ));

                    let dirty_idx = ((var_id + 7) / 8).saturating_sub(1) as usize;
                    let bitmask = 1 << (var_id % 8);
                    out.write_updateln(format_args!(
                        "if (dirty[{dirty_idx}] & {bitmask}) elems[\"{id}\"].value = ctx[{var_id}];"
                    ));
                });
            }
            Attribute::KeyValue(key, Some(AttributeValue::JavaScript(js))) => {
                let js = if *key == "style" && inline_styles_candidate {
                    let style = state.use_style_cache();
                    rslint_parser::parse_text(&format!("`${{{js}}} {style}`"), 0).syntax()
                } else {
                    js.clone()
                };
                render_dyn_attr(meta, state, out, key, &js);
            }
        }
    }
}

fn render_dyn_attr(
    meta: &FragmentMetadata,
    state: &mut State,
    out: &mut Output,
    key: &str,
    js: &SyntaxNode,
) {
    with_id!(meta.id(), state, |id| {
        out.write_element(id, format_args!("document.getElementById(\"{id}\")"));
        let unbound = utils::get_unbound_refs(js);
        let dirty_indices =
            codegen_utils::calc_dirty(&unbound, state.component.declared_vars(), meta.scope());
        let replaced = codegen_utils::replace_namerefs(
            js,
            &unbound,
            state.component.declared_vars(),
            meta.scope(),
        );
        if dirty_indices.is_empty() {
            out.write_updateln(format_args!(
                "if (initial) elems[\"{id}\"].setAttribute(\"{key}\", {replaced});"
            ));
        } else {
            out.write_updateln(format_args!(
                "if ({dirty_indices}) elems[\"{id}\"].setAttribute(\"{key}\", {replaced});"
            ));
        }
    });
}
