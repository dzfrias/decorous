use std::{
    borrow::Cow,
    collections::HashMap,
    fmt::{Display, Write as FmtWrite},
    io::{self, Write},
};

use crate::{
    codegen_utils,
    dom_render::{render_fragment as dom_render_fragment, State as DomRenderState},
    Options,
};
use decorous_frontend::{
    ast::{
        Attribute, AttributeValue, Comment, Element, ForBlock, IfBlock, Mustache, Node, NodeType,
        SpecialBlock, Text, UseBlock,
    },
    utils, Component, FragmentMetadata,
};
use heck::ToSnekCase;
use rslint_parser::{AstNode, SmolStr, SyntaxNode};

pub fn render(
    component: &Component,
    out: &mut impl io::Write,
    html_out: &mut impl io::Write,
    metadata: &Options,
) -> io::Result<()> {
    if let Some(wasm) = component.wasm() {
        let _ = metadata.wasm_compiler.compile(
            crate::CodeInfo {
                lang: wasm.lang(),
                body: wasm.body(),
                exports: component.exports(),
            },
            out,
        );
    }

    let mut output = Output::default();
    let mut state = State {
        component,
        id_overwrites: HashMap::new(),
        style_cache: None,
        uses: vec![],
    };

    for node in component.fragment_tree() {
        node.render(&mut state, &mut output, &());
    }

    html_out.write_all(&output.html)?;

    let has_reactive_variables = !component.declared_vars().all_vars().is_empty();

    if has_reactive_variables {
        let vars = (component.declared_vars().all_vars().len() + 7) / 8;
        writeln!(
            out,
            "const dirty = new Uint8Array(new ArrayBuffer({vars}));",
        )?;
    }

    for use_decl in component.uses() {
        let Some(stem) = use_decl.file_stem() else {
            continue;
        };
        let use_info = metadata.use_resolver.resolve(use_decl)?;
        writeln!(
            out,
            "import __decor_{} from \"./{}\";",
            stem.to_string_lossy().to_snek_case(),
            use_info.loc.display(),
        )?;
    }
    for hoist in component.hoist() {
        writeln!(out, "{hoist}")?;
    }
    out.write_all(&output.hoists)?;

    if let Some(comptime) = component.comptime() {
        match metadata.wasm_compiler.compile_comptime(crate::CodeInfo {
            lang: comptime.lang(),
            body: comptime.body(),
            exports: component.exports(),
        }) {
            Ok(env) => {
                for decl in env.items() {
                    writeln!(out, "const {} = {};", decl.name, decl.value)?;
                }
            }
            Err(_err) => todo!("error"),
        }
    }

    if !output.elements.is_empty() {
        // Write elements
        let elems = unsafe { String::from_utf8_unchecked(output.elements) };
        write!(
            out,
            concat!(
                "const elems = {{{}}}\n",
                include_str!("./templates/replace.js")
            ),
            elems
        )?;
    }

    // Write ctx init
    if !output.ctx_init.is_empty()
        || !component.declared_vars().is_empty()
        || !component.toplevel_nodes().is_empty()
    {
        writeln!(out, "function __init_ctx() {{")?;
        for (arrow_expr, (idx, scope_id)) in component.declared_vars().all_arrow_exprs() {
            writeln!(out, "  let __closure{idx} = {};", {
                codegen_utils::replace_assignments(
                    arrow_expr.syntax(),
                    &utils::get_unbound_refs(arrow_expr.syntax()),
                    component.declared_vars(),
                    *scope_id,
                )
            })?
        }
        for node in component.toplevel_nodes() {
            if node.substitute_assign_refs {
                let replacement = codegen_utils::replace_assignments(
                    &node.node,
                    &utils::get_unbound_refs(&node.node),
                    component.declared_vars(),
                    None,
                );
                let _ = writeln!(out, "  {replacement}");
            } else {
                let _ = writeln!(out, "  {}", node.node);
            }
        }
        out.write_all(&output.ctx_init)?;
        for (block, id) in component.declared_vars().all_reactive_blocks() {
            let replaced = codegen_utils::replace_assignments(
                block,
                &utils::get_unbound_refs(block),
                component.declared_vars(),
                None,
            );
            writeln!(out, "  let __reactive{id} = () => {{ {replaced} }};")?;
        }

        let mut ctx = vec![Cow::Borrowed("undefined"); component.declared_vars().len()];
        for (name, idx) in component.declared_vars().all_vars() {
            ctx[*idx as usize] = Cow::Borrowed(name);
        }
        for (idx, _) in component.declared_vars().all_arrow_exprs().values() {
            ctx[*idx as usize] = Cow::Owned(format!("__closure{idx}"));
        }
        for idx in component.declared_vars().all_bindings().values() {
            ctx[*idx as usize] = Cow::Owned(format!("__binding{idx}"));
        }
        for idx in component.declared_vars().all_reactive_blocks().values() {
            ctx[*idx as usize] = Cow::Owned(format!("__reactive{idx}"));
        }
        writeln!(out, "  return [{}];\n}}", ctx.join(","))?;

        if has_reactive_variables {
            writeln!(out, "const ctx = __init_ctx();\nlet updating = false;")?;
        } else {
            writeln!(out, "const ctx = __init_ctx();")?;
        }
    }

    if !output.updates.is_empty() || !component.declared_vars().all_reactive_blocks().is_empty() {
        writeln!(out, "function __update(dirty, initial) {{")?;

        for (block, id) in component.declared_vars().all_reactive_blocks() {
            let unbound = utils::get_unbound_refs(block);
            let dirty = codegen_utils::calc_dirty(&unbound, component.declared_vars(), None);
            writeln!(out, "  if ({dirty}) {{ ctx[{id}](); }}")?;
        }

        out.write_all(&output.updates)?;

        writeln!(out, "}}")?;

        writeln!(
            out,
            "dirty.fill(255);
__update(dirty, true);
dirty.fill(0);"
        )?;
    }

    if has_reactive_variables {
        write!(out, include_str!("./templates/schedule_update.js"))?;
    }

    Ok(())
}

#[derive(Debug, Default)]
struct Output {
    html: Vec<u8>,
    elements: Vec<u8>,
    ctx_init: Vec<u8>,
    updates: Vec<u8>,
    hoists: Vec<u8>,
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
struct State<'ast> {
    component: &'ast Component<'ast>,
    id_overwrites: HashMap<u32, SmolStr>,
    style_cache: Option<String>,
    uses: Vec<Cow<'ast, str>>,
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
            $exec(&id);
        } else {
            $exec($id);
        }
    };
}

trait Render<'ast> {
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
                _ => {}
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
                    ))
                });
            }
            Attribute::KeyValue(key, Some(AttributeValue::JavaScript(js))) => {
                let js = if *key == "style" && inline_styles_candidate {
                    let style = state.use_style_cache();
                    let new_js =
                        rslint_parser::parse_text(&format!("`${{{js}}} {style}`"), 0).syntax();
                    new_js
                } else {
                    js.clone()
                };
                render_dyn_attr(meta, state, out, key, &js);
            }
        }
    }
}

fn render_dyn_attr<'ast>(
    meta: &FragmentMetadata,
    state: &mut State<'ast>,
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
            &js,
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

#[cfg(test)]
mod tests {
    use crate::css_render::render_css;
    use decorous_frontend::{parse, Component};
    use std::fmt::Write;

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
                let mut css_out = Vec::new();
                render(&component, &mut js_out, &mut html_out, &Options::default()).unwrap();
                if let Some(css) = component.css() {
                    render_css(css, &mut css_out, &component).unwrap();
                }
                let mut output = format!("{}\n---\n{}", String::from_utf8(js_out).unwrap(), String::from_utf8(html_out).unwrap());
                if component.css().is_some() {
                    write!(output, "\n---\n{}", String::from_utf8(css_out).unwrap()).unwrap();
                }
                insta::assert_snapshot!(output);
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
        test_render!(
            "---js let x = 3;--- #p[id=\"custom\" @click={() => console.log(1 + 1)} class={1 + 1}] Hello, {x}! /p"
        );
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
        test_render!("---js let x = 0; let y = 0; --- #p {x} and {y} and {x + y} /p #button[@click={() => { x = 3; y = 3; }}]:Hi");
    }

    #[test]
    fn can_render_if_else() {
        test_render!(
            "---js let x = 0; --- {#if x == 0} wow {/if}",
            "---js let x = 0; --- {#if x == 0} wow {:else} wow!! {/if}"
        );
    }

    #[test]
    fn can_render_for() {
        test_render!("{#for i in [1, 2, 3]} {i} {/for}");
    }

    #[test]
    fn reactive_css_applies_to_root_elements() {
        test_render!("---css p { color: {color}; } --- #div #p:Hello /div");
    }

    #[test]
    fn reactive_css_is_merged_with_existing_inline_styles() {
        test_render!("---js let color = \"blue\" --- ---css p { color: {color}; } --- #p[style=\"background: green;\"] {color} /p", "---js let color = \"blue\" --- ---css p { color: {color}; } --- #p[style={`background: green;`}] {color} /p");
    }

    #[test]
    fn can_render_bindings() {
        test_render!("---js let x = 0; --- #input[:x:]/input");
    }

    #[test]
    fn does_not_get_duplicate_elems() {
        test_render!(
            "---js let x = 0; --- #input[:x: @click={() => console.log(\"hello\")}]/input"
        );
    }

    #[test]
    fn can_render_reactive_blocks() {
        test_render!("---js let x = 0; let y = 0; $: y = x + 1; --- #input[:x:]/input");
    }

    #[test]
    fn can_have_resolver_for_use_path() {
        test_render!("{#use \"./hello.decor\"} #p:Hello #hello /hello");
    }

    #[test]
    fn dashes_in_use_block_are_turned_into_underscores() {
        test_render!("{#use \"./hello-world.decor\"} #hello-world /hello-world");
    }
}
