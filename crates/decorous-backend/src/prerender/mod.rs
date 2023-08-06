use std::{
    borrow::Cow,
    io::{self, Write},
};

mod html_render;
mod node_analyzer;

use decorous_frontend::{ast::SpecialBlock, utils, Component};
use itertools::Itertools;
use lazy_format::lazy_format;
use rslint_parser::AstNode;
use superfmt::{ContextBuilder, Formatter};

pub use self::html_render::HtmlPrerenderer;
use self::node_analyzer::analyzers::Analysis;
use crate::{
    codegen_utils, dom_render::render_fragment as dom_render_fragment, Metadata, RenderBackend,
};

#[derive(Debug, PartialEq, Eq)]
enum WriteStatus {
    Something,
    Nothing,
}

impl WriteStatus {
    #[must_use]
    fn wrote_something(&self) -> bool {
        matches!(self, Self::Something)
    }
}

pub struct Prerenderer;

impl RenderBackend for Prerenderer {
    fn render<T: io::Write>(
        out: &mut T,
        component: &Component,
        _metadata: &Metadata,
    ) -> io::Result<()> {
        render(component, out)
    }
}

fn render<T>(component: &Component, js_out: &mut T) -> io::Result<()>
where
    T: io::Write,
{
    let analysis = Analysis::analyze(component);

    let has_reactive_variables = !component.declared_vars().all_vars().is_empty();

    if has_reactive_variables {
        let vars = (component.declared_vars().all_vars().len() + 7) / 8;
        writeln!(
            js_out,
            "const dirty = new Uint8Array(new ArrayBuffer({vars}));",
        )?;
    }

    render_hoists(component, &analysis, js_out)?;
    let status = render_elements(&analysis, js_out)?;
    if status.wrote_something() {
        write!(js_out, include_str!("./templates/replace.js"))?;
    }
    let status = render_ctx_init(component, &analysis, js_out)?;
    if has_reactive_variables && status.wrote_something() {
        writeln!(
            js_out,
            "const ctx = __init_ctx();
let updating = false;"
        )?;
    } else if status.wrote_something() {
        writeln!(js_out, "const ctx = __init_ctx();")?;
    }
    let status = render_update_body(component, &analysis, js_out)?;
    if status.wrote_something() {
        writeln!(
            js_out,
            "dirty.fill(255);
__update(dirty, true);
dirty.fill(0);"
        )?;
    }
    // If there are no reactive variables, nothing can be updated, so remove this.
    if has_reactive_variables {
        write!(js_out, include_str!("./templates/schedule_update.js"))?;
    }

    Ok(())
}

fn render_hoists<'a, T: io::Write>(
    component: &Component<'a>,
    analysis: &Analysis<'a>,
    out: &mut T,
) -> io::Result<()> {
    let mut formatter = Formatter::new(out);

    formatter.write_all_trailing(component.hoist(), "\n")?;

    for (meta, block) in analysis.reactive_data().special_blocks() {
        match block {
            SpecialBlock::If(if_block) => {
                dom_render_fragment(
                    if_block.inner(),
                    Some(meta.id()),
                    component.declared_vars(),
                    &meta.id().to_string(),
                    out,
                )?;
                if let Some(else_block) = if_block.else_block() {
                    dom_render_fragment(
                        else_block,
                        Some(meta.id()),
                        component.declared_vars(),
                        &format!("{}_else", meta.id()),
                        out,
                    )?;
                }
            }
            SpecialBlock::For(for_block) => {
                dom_render_fragment(
                    for_block.inner(),
                    Some(meta.id()),
                    component.declared_vars(),
                    &meta.id().to_string(),
                    out,
                )?;
            }
        }
    }

    Ok(())
}

fn render_elements<T: io::Write>(analysis: &Analysis, out: &mut T) -> io::Result<WriteStatus> {
    let mut formatter = Formatter::new(out);

    if analysis.reactive_data().mustaches().is_empty()
        && analysis.reactive_data().key_values().is_empty()
        && analysis.reactive_data().event_listeners().is_empty()
        && analysis.reactive_data().bindings().is_empty()
        && analysis.reactive_data().special_blocks().is_empty()
    {
        return Ok(WriteStatus::Nothing);
    }

    formatter
        .write("const elems = ")?
        .begin_context(
            ContextBuilder::default()
                .starts_with("{")
                .ends_with("};\n")
                .build(),
        )?
        // For mustache tags, replace the marked tags (<span>s with id's) with text nodes.
        .write_all_trailing(
            analysis
                .reactive_data()
                .mustaches()
                .iter()
                .map(|(meta, _)| {
                    let id = analysis.id_overwrites().try_get(meta.id());
                    lazy_format!("\"{id}\":replace(document.getElementById(\"{id}\"))")
                }),
            ",",
        )?
        // For reactive key-value attributes, just get element by its generated id
        .write_all_trailing(
            analysis
                .reactive_data()
                .key_values()
                .iter()
                .chain(analysis.reactive_data().event_listeners())
                .map(|(meta, _)| meta)
                .chain(
                    analysis
                        .reactive_data()
                        .bindings()
                        .iter()
                        .map(|(meta, _)| meta),
                )
                .unique_by(|meta| meta.id())
                .map(|meta| {
                    let id = analysis.id_overwrites().try_get(meta.id());
                    lazy_format!("\"{id}\":document.getElementById(\"{id}\")")
                }),
            ",",
        )?;

    for (meta, block) in analysis.reactive_data().special_blocks() {
        let id = analysis.id_overwrites().try_get(meta.id());
        match *block {
            SpecialBlock::If(block) => {
                // {id} acts as an anchor for the eventual id_block that will be attached to the
                // DOM. It's an empty text node. {id}_block is the reference to the DOM rendered if
                // block. It is null when the condition is false (or the else block).
                write!(
                    formatter,
                    "\"{id}\":replace(document.getElementById(\"{id}\")),\"{id}_block\":null,"
                )?;
                if block.else_block().is_some() {
                    // Designates if the if block is in it's main or else body. True if main, false
                    // if else
                    write!(formatter, "\"{id}_on\":true,")?;
                }
            }
            SpecialBlock::For(_) => {
                // Much like the if block, {id} acts as the anchor point for all the rendered
                // {#for} block children on the DOM. {id}_block holds these children
                write!(
                    formatter,
                    "\"{id}\":replace(document.getElementById(\"{id}\")),\"{id}_block\":[],"
                )?;
            }
        }
    }

    formatter.pop_ctx()?;

    Ok(WriteStatus::Something)
}

fn render_ctx_init<T: io::Write>(
    component: &Component,
    analysis: &Analysis,
    out: &mut T,
) -> io::Result<WriteStatus> {
    let mut formatter = Formatter::new(out);

    if component.declared_vars().is_empty()
        && component.toplevel_nodes().is_empty()
        && analysis.reactive_data().event_listeners().is_empty()
    {
        return Ok(WriteStatus::Nothing);
    }

    formatter.write("function __init_ctx() ")?.begin_context(
        ContextBuilder::default()
            .starts_with("{\n")
            .ends_with("}\n")
            .prepend("  ")
            .build(),
    )?;

    // Write anonymous functions. Arrow exprs are pulled from the template (they're usually from
    // event listeners).
    formatter.write_all_trailing(
        component
            .declared_vars()
            .all_arrow_exprs()
            .iter()
            .map(|(arrow_expr, (idx, scope_id))| {
                lazy_format!(
                    "let __closure{idx} = {};",
                    codegen_utils::replace_assignments(
                        arrow_expr.syntax(),
                        &utils::get_unbound_refs(arrow_expr.syntax()),
                        component.declared_vars(),
                        *scope_id
                    )
                )
            }),
        "\n",
    )?;

    for node in component.toplevel_nodes() {
        if node.substitute_assign_refs {
            let replacement = codegen_utils::replace_assignments(
                &node.node,
                &utils::get_unbound_refs(&node.node),
                component.declared_vars(),
                None,
            );
            writeln!(formatter, "{}", replacement)?;
        } else {
            writeln!(formatter, "{}", node.node)?;
        }
    }

    for (meta, event, expr) in analysis.reactive_data().flat_listeners() {
        let id = analysis.id_overwrites().try_get(meta.id());
        let replaced = codegen_utils::replace_assignments(
            expr,
            &utils::get_unbound_refs(expr),
            component.declared_vars(),
            None,
        );
        writeln!(
            formatter,
            "elems[\"{id}\"].addEventListener(\"{event}\", {replaced});"
        )?;
    }

    for (meta, binding) in analysis.reactive_data().bindings() {
        let elem_id = analysis.id_overwrites().try_get(meta.id());
        let id = component
            .declared_vars()
            .get_binding(binding)
            .expect("BUG: every binding should have an id in declared vars");
        let Some(var_id) = component.declared_vars().get_var(binding, None) else {
            todo!("unbound var lint")
        };
        writeln!(formatter, "elems[\"{elem_id}\"].value = {binding};")?;
        writeln!(
            formatter,
            "let __binding{id} = (ev) => __schedule_update({var_id}, {binding} = ev.target.value);"
        )?;
        writeln!(
            formatter,
            "elems[\"{elem_id}\"].addEventListener(\"input\", __binding{id});"
        )?;
    }

    for (block, id) in component.declared_vars().all_reactive_blocks() {
        let replaced = codegen_utils::replace_assignments(
            block,
            &utils::get_unbound_refs(block),
            component.declared_vars(),
            None,
        );
        writeln!(formatter, "let __reactive{id} = () => {{ {replaced} }};")?;
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
    writeln!(formatter, "return [{}];", ctx.join(","))?;

    formatter.pop_ctx()?;

    Ok(WriteStatus::Something)
}

fn render_update_body<T: io::Write>(
    component: &Component,
    analysis: &Analysis,
    out: &mut T,
) -> io::Result<WriteStatus> {
    let mut formatter = Formatter::new(out);

    if analysis.reactive_data().mustaches().is_empty()
        && analysis.reactive_data().bindings().is_empty()
        && analysis.reactive_data().special_blocks().is_empty()
        && analysis.reactive_data().key_values().is_empty()
    {
        return Ok(WriteStatus::Nothing);
    }

    formatter
        .write("function __update(dirty, initial) ")?
        .begin_context(
            ContextBuilder::default()
                .starts_with("{\n")
                .ends_with("}\n")
                .prepend("  ")
                .build(),
        )?;

    for (block, id) in component.declared_vars().all_reactive_blocks() {
        let unbound = utils::get_unbound_refs(block);
        let dirty = codegen_utils::calc_dirty(&unbound, component.declared_vars(), None);
        writeln!(formatter, "if ({dirty}) {{ ctx[{id}](); }}")?;
    }

    for (meta, js) in analysis.reactive_data().mustaches() {
        let unbound = utils::get_unbound_refs(js);
        let dirty_indices =
            codegen_utils::calc_dirty(&unbound, component.declared_vars(), meta.scope());
        let replaced =
            codegen_utils::replace_namerefs(js, &unbound, component.declared_vars(), meta.scope());

        let id = meta.id();
        if dirty_indices.is_empty() {
            writeln!(formatter, "if (initial) elems[{id}].data = {replaced};")?;
        } else {
            writeln!(
                formatter,
                "if ({dirty_indices}) elems[{id}].data = {replaced};"
            )?;
        }
    }

    for (meta, binding) in analysis.reactive_data().bindings() {
        let Some(var_id) = component.declared_vars().get_var(binding, None) else {
            todo!("unbound var lint");
        };
        let dirty_idx = ((var_id + 7) / 8).saturating_sub(1) as usize;
        let bitmask = 1 << (var_id % 8);
        writeln!(
            formatter,
            "if (dirty[{dirty_idx}] & {bitmask}) elems[\"{}\"].value = ctx[{var_id}];",
            meta.id()
        )?;
    }

    for (meta, special_block) in analysis.reactive_data().special_blocks() {
        match special_block {
            SpecialBlock::If(if_block) => {
                let unbound = utils::get_unbound_refs(if_block.expr());
                let replaced = codegen_utils::replace_namerefs(
                    if_block.expr(),
                    &unbound,
                    component.declared_vars(),
                    meta.scope(),
                );

                let id = meta.id();
                if if_block.else_block().is_some() {
                    writeln!(
                        formatter,
                        include_str!("./templates/if.js"),
                        replaced = replaced,
                        id = id
                    )?;
                } else {
                    writeln!(
                        formatter,
                        "if ({replaced}) {{ if (elems[\"{id}_block\"]) {{ elems[\"{id}_block\"].u(dirty); }} else {{ elems[\"{id}_block\"] = create_{id}_block(elems[\"{id}\"].parentNode, elems[\"{id}\"]); }} }} else if (elems[\"{id}_block\"]) {{ elems[\"{id}_block\"].d(); elems[\"{id}_block\"] = null; }}"
                    )?;
                }
            }
            SpecialBlock::For(block) => {
                let unbound = utils::get_unbound_refs(block.expr());
                let replaced = codegen_utils::replace_namerefs(
                    block.expr(),
                    &unbound,
                    component.declared_vars(),
                    meta.scope(),
                );
                let var_idx = component
                    .declared_vars()
                    .all_scopes()
                    .get(&meta.id())
                    .expect("BUG: for block should have an assigned scope")
                    .get(block.binding())
                    .expect("BUG: for block's scope should contain the binding");
                let id = meta.id();
                writeln!(formatter, "let i = 0; for (const v of ({replaced})) {{ ctx[{var_idx}] = v; if (i >= elems[\"{id}_block\"].length) {{ elems[\"{id}_block\"][i] = create_{id}_block(elems[\"{id}\"].parentNode, elems[\"{id}\"]); }} elems[\"{id}_block\"][i].u(dirty); i += 1; }} elems[\"{id}_block\"].slice(i).forEach((b) => b.d()); elems[\"{id}_block\"].length = i;")?;
            }
        }
    }

    for (meta, attr, js) in analysis.reactive_data().flat_kvs() {
        let id = analysis.id_overwrites().try_get(meta.id());
        let unbound = utils::get_unbound_refs(js);
        let dirty_indices =
            codegen_utils::calc_dirty(&unbound, component.declared_vars(), meta.scope());
        let replaced =
            codegen_utils::replace_namerefs(js, &unbound, component.declared_vars(), meta.scope());
        if dirty_indices.is_empty() {
            writeln!(
                formatter,
                "if (initial) elems[\"{id}\"].setAttribute(\"{attr}\", {replaced});"
            )?;
        } else {
            writeln!(
                formatter,
                "if ({dirty_indices}) elems[\"{id}\"].setAttribute(\"{attr}\", {replaced});"
            )?;
        }
    }

    formatter.pop_ctx()?;

    Ok(WriteStatus::Something)
}

#[cfg(test)]
mod tests {
    use crate::css_render::CssRenderer;
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
                render(&component, &mut js_out).unwrap();
                <HtmlPrerenderer as RenderBackend>::render(&mut html_out, &component, &Metadata { name: "test" }).unwrap();
                <CssRenderer as RenderBackend>::render(&mut css_out, &component, &Metadata { name: "test" }).unwrap();
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
}
