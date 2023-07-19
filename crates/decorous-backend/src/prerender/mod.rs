use std::{borrow::Cow, fmt::Write, io};

mod html_render;
mod node_analyzer;

use decorous_frontend::{ast::SpecialBlock, utils, Component};
use rslint_parser::AstNode;

use crate::{
    codegen_utils::{self, force_write, force_writeln},
    dom_render::render_fragment as dom_render_fragment,
};

use self::{html_render::HtmlFmt, node_analyzer::analyzers::Analysis};

pub fn render<T, U>(component: &Component, js_out: &mut T, html_out: &mut U) -> io::Result<()>
where
    T: io::Write,
    U: io::Write,
{
    for node in component.fragment_tree() {
        node.html_fmt(html_out, &())?;
    }

    let analysis = Analysis::analyze(component);

    let (elems, ctx_init_body, update_body, hoists) = (
        render_elements(&analysis),
        render_ctx_init(component, &analysis),
        render_update_body(component, &analysis),
        render_hoists(component, &analysis),
    );

    // Write finished segments
    write!(
        js_out,
        include_str!("./templates/main.js"),
        dirty_items = ((component.declared_vars().all_vars().len() + 7) / 8),
        elems = elems,
        ctx_body = ctx_init_body,
        update_body = update_body,
        hoistables = hoists,
    )?;

    Ok(())
}

fn render_hoists<'a>(component: &Component<'a>, analysis: &Analysis<'a>) -> String {
    let mut out = String::new();

    // TODO: For blocks here
    for (meta, block) in analysis.reactive_data().special_blocks() {
        match block {
            SpecialBlock::If(if_block) => {
                let rendered = dom_render_fragment(
                    if_block.inner(),
                    Some(meta.id()),
                    component.declared_vars(),
                    &meta.id().to_string(),
                );
                out.push_str(&rendered);
                if let Some(else_block) = if_block.else_block() {
                    let rendered = dom_render_fragment(
                        else_block,
                        Some(meta.id()),
                        component.declared_vars(),
                        &format!("{}_else", meta.id()),
                    );
                    out.push_str(&rendered);
                }
            }
            SpecialBlock::For(for_block) => {
                let rendered = dom_render_fragment(
                    for_block.inner(),
                    Some(meta.id()),
                    component.declared_vars(),
                    &meta.id().to_string(),
                );
                out.push_str(&rendered);
            }
        }
    }

    out
}

fn render_elements(analysis: &Analysis) -> String {
    let mut out = String::new();
    force_write!(out, "{{");
    // Replace mustache tags and special blocks with an empty text node. For mustache tags,
    // this allows us to manipulate its contents by reference. For special blocks like `if`
    // and `for`, this creates a marker point for where to insert the contents of the
    // special block, as they have to be generated at runtime by JavaScript.
    for (meta, _) in analysis.reactive_data().mustaches() {
        let id = analysis.id_overwrites().try_get(meta.id());
        force_write!(out, "\"{id}\":replace(document.getElementById(\"{id}\")),");
    }
    for (meta, _) in analysis
        .reactive_data()
        .key_values()
        .iter()
        .chain(analysis.reactive_data().event_listeners())
    {
        let id = analysis.id_overwrites().try_get(meta.id());
        force_write!(out, "\"{id}\":document.getElementById(\"{id}\"),");
    }
    for (meta, block) in analysis.reactive_data().special_blocks() {
        let id = analysis.id_overwrites().try_get(meta.id());
        match *block {
            SpecialBlock::If(block) => {
                // This is an anchor point to attach the special block to
                force_write!(out, "\"{id}\":replace(document.getElementById(\"{id}\")),");
                // {id}_block is the actual fragment that's attached to the DOM
                force_write!(out, "\"{id}_block\":null,");
                if block.else_block().is_some() {
                    // Designates if the if block is in it's main or else body. True if main, false
                    // if else
                    force_write!(out, "\"{id}_on\":true,");
                }
            }
            SpecialBlock::For(_) => {
                force_write!(out, "\"{id}\":replace(document.getElementById(\"{id}\")),");
                // {id}_block has the list of nodes of the for block
                force_write!(out, "\"{id}_block\":[],");
            }
        }
    }
    force_write!(out, "}}");
    out
}

fn render_ctx_init(component: &Component, analysis: &Analysis) -> String {
    let mut out = String::new();

    for (arrow_expr, (idx, scope_id)) in component.declared_vars().all_arrow_exprs() {
        force_writeln!(
            out,
            "let __closure{idx} = {};",
            codegen_utils::replace_assignments(
                arrow_expr.syntax(),
                &utils::get_unbound_refs(arrow_expr.syntax()),
                component.declared_vars(),
                *scope_id
            )
        );
    }
    for node in component.toplevel_nodes() {
        if node.substitute_assign_refs {
            let replacement = codegen_utils::replace_assignments(
                &node.node,
                &utils::get_unbound_refs(&node.node),
                component.declared_vars(),
                None,
            );
            force_writeln!(out, "{}", replacement);
        } else {
            force_writeln!(out, "{}", node.node);
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
        force_writeln!(
            out,
            "elems[\"{id}\"].addEventListener(\"{event}\", {replaced});"
        );
    }

    let mut ctx = vec![Cow::Borrowed("undefined"); component.declared_vars().len()];
    for (name, idx) in component.declared_vars().all_vars() {
        ctx[*idx as usize] = Cow::Borrowed(name);
    }
    for (idx, _) in component.declared_vars().all_arrow_exprs().values() {
        ctx[*idx as usize] = Cow::Owned(format!("__closure{idx}"));
    }
    force_writeln!(out, "return [{}];", ctx.join(","));

    out
}

fn render_update_body(component: &Component, analysis: &Analysis) -> String {
    let mut out = String::new();

    for (meta, js) in analysis.reactive_data().mustaches() {
        let unbound = utils::get_unbound_refs(js);
        let dirty_indices =
            codegen_utils::calc_dirty(&unbound, component.declared_vars(), meta.scope());
        let replaced =
            codegen_utils::replace_namerefs(js, &unbound, component.declared_vars(), meta.scope());

        let id = meta.id();
        if dirty_indices.is_empty() {
            force_writeln!(out, "elems[{id}].data = {replaced}");
        } else {
            force_writeln!(out, "if ({dirty_indices}) elems[{id}].data = {replaced};");
        }
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
                    writeln!(out, include_str!("./templates/if.js"), replaced = replaced, id = id)
                } else {
                    writeln!(
                        out,
                        "if ({replaced}) {{ if (elems[\"{id}_block\"]) {{ elems[\"{id}_block\"].u(dirty); }} else {{ elems[\"{id}_block\"] = create_{id}_block(elems[\"{id}\"].parentNode, elems[\"{id}\"]); }} }} else if (elems[\"{id}_block\"]) {{ elems[\"{id}_block\"].d(); elems[\"{id}_block\"] = null; }}"
                    )
                }
                .expect("string formatting should not fail");
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
                force_writeln!(out, "({replaced}).forEach((v, i) => {{ if (i >= elems[\"{id}_block\"].length) {{ elems[\"{id}_block\"][i] = create_{id}_block(elems[\"{id}\"].parentNode, elems[\"{id}\"]); }} ctx[{var_idx}] = v; elems[\"{id}_block\"][i].u(dirty) }})");
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
            force_writeln!(out, "elems[\"{id}\"].setAttribute(\"{attr}\", {replaced});");
        } else {
            force_writeln!(
                out,
                "if ({dirty_indices}) elems[\"{id}\"].setAttribute(\"{attr}\", {replaced});"
            );
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use decorous_frontend::{parse, Component};

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
                render(&component, &mut js_out, &mut html_out).unwrap();
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
        test_render!("---js let x = 0; let y = 0; --- #p {x} and {y} and {x + y} /p");
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
}
