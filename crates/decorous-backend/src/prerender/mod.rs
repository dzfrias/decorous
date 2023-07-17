use std::{borrow::Cow, fmt::Write, io};

mod html_render;
mod node_analyzer;
mod render_if;

use decorous_frontend::{ast::SpecialBlock, utils, Component};
use rslint_parser::AstNode;

use crate::codegen_utils;

use self::{
    html_render::HtmlFmt,
    node_analyzer::analyzers::{Analysis, ReactiveAttribute, ReactiveData},
    render_if::render_if_block,
};

macro_rules! sort_if_testing {
    ($iter:expr, $sort_by:expr) => {{
        #[cfg(test)]
        use ::itertools::Itertools;
        #[cfg(test)]
        let new = $iter.sorted_by($sort_by);
        #[cfg(not(test))]
        let new = $iter;

        new
    }};
}

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

    for (id, if_block) in analysis.hoistables().if_blocks() {
        let renderered = render_if_block(*if_block, *id, component.declared_vars());
        out.push_str(&renderered.0);
    }

    out
}

fn render_elements(analysis: &Analysis) -> String {
    let mut out = String::new();
    write!(out, "{{").expect("string formatting should not fail");
    for (id, data) in sort_if_testing!(analysis.reactive_data().iter(), |a, b| a.0.cmp(b.0)) {
        let id = analysis.id_overwrites().try_get(*id);
        match data {
            // Replace mustache tags and special blocks with an empty text node. For mustache tags,
            // this allows us to manipulate its contents by reference. For special blocks like `if`
            // and `for`, this creates a marker point for where to insert the contents of the
            // special block, as they have to be generated at runtime by JavaScript.
            ReactiveData::Mustache(_) => {
                write!(out, "\"{id}\":replace(document.getElementById(\"{id}\")),")
                    .expect("string formatting should not fail")
            }
            ReactiveData::AttributeCollection(_) => {
                write!(out, "\"{id}\":document.getElementById(\"{id}\"),")
                    .expect("string formatting should not fail")
            }
            ReactiveData::SpecialBlock(_) => {
                write!(out, "\"{id}\":replace(document.getElementById(\"{id}\")),")
                    .expect("string formatting should not fail");
                write!(out, "\"{id}_block\":null,").expect("string formatting should not fail")
            }
        }
    }
    write!(out, "}}").expect("string formatting should not fail");
    out
}

fn render_ctx_init(component: &Component, analysis: &Analysis) -> String {
    let mut out = String::new();

    for (arrow_expr, idx) in component.declared_vars().all_arrow_exprs() {
        writeln!(
            out,
            "let __closure{idx} = {};",
            codegen_utils::replace_assignments(
                arrow_expr.syntax(),
                &utils::get_unbound_refs(arrow_expr.syntax()),
                component.declared_vars()
            )
        )
        .expect("string format should not fail");
    }
    for node in component.toplevel_nodes() {
        if node.substitute_assign_refs {
            let replacement = codegen_utils::replace_assignments(
                &node.node,
                &utils::get_unbound_refs(&node.node),
                component.declared_vars(),
            );
            writeln!(out, "{}", replacement).expect("string formatting should not fail");
        } else {
            writeln!(out, "{}", node.node).expect("string formatting should not fail");
        }
    }

    for (id, event, expr) in sort_if_testing!(analysis.reactive_data().iter(), |a, b| a.0.cmp(b.0))
        .filter_map(|(id, data)| match data {
            ReactiveData::Mustache(_) | ReactiveData::SpecialBlock(_) => None,
            ReactiveData::AttributeCollection(elems) => {
                Some(elems.iter().filter_map(move |attr| match attr {
                    ReactiveAttribute::KeyValue(_, _) => None,
                    ReactiveAttribute::EventListener(expr, handler) => Some((id, expr, handler)),
                }))
            }
        })
        .flatten()
    {
        let id = analysis.id_overwrites().try_get(*id);
        let replaced = codegen_utils::replace_assignments(
            expr,
            &utils::get_unbound_refs(expr),
            component.declared_vars(),
        );
        writeln!(
            out,
            "elems[\"{id}\"].addEventListener(\"{event}\", {replaced});"
        )
        .expect("writing to format string should not fail");
    }

    let mut ctx = vec![Cow::Borrowed(""); component.declared_vars().len()];
    for (name, idx) in component.declared_vars().all_vars() {
        ctx[*idx as usize] = Cow::Borrowed(name);
    }
    for idx in component.declared_vars().all_arrow_exprs().values() {
        ctx[*idx as usize] = Cow::Owned(format!("__closure{idx}"));
    }
    writeln!(out, "return [{}];", ctx.join(",")).expect("string format should not fail");

    out
}

fn render_update_body(component: &Component, analysis: &Analysis) -> String {
    let mut out = String::new();

    for (idx, js) in sort_if_testing!(analysis.reactive_data().iter(), |a, b| a.0.cmp(b.0))
        .filter_map(|(id, data)| match data {
            ReactiveData::Mustache(js) => Some((id, js)),
            ReactiveData::AttributeCollection(_) => None,
            ReactiveData::SpecialBlock(_) => None,
        })
    {
        let unbound = utils::get_unbound_refs(js);
        let dirty_indices = codegen_utils::calc_dirty(&unbound, component.declared_vars());
        let replaced = codegen_utils::replace_namerefs(js, &unbound, component.declared_vars());

        if dirty_indices.is_empty() {
            writeln!(out, "elems[{idx}].data = {replaced}")
                .expect("string formatting should not fail");
        } else {
            writeln!(out, "if ({dirty_indices}) elems[{idx}].data = {replaced};",)
                .expect("writing to string format should not fail");
        }
    }

    for (id, special_block) in sort_if_testing!(analysis.reactive_data().iter(), |a, b| a
        .0
        .cmp(b.0))
    .filter_map(|(id, data)| match data {
        ReactiveData::Mustache(_) | ReactiveData::AttributeCollection(_) => None,
        ReactiveData::SpecialBlock(block) => Some((*id, *block)),
    }) {
        match special_block {
            SpecialBlock::If(if_block) => {
                let unbound = utils::get_unbound_refs(if_block.expr());
                let replaced = codegen_utils::replace_namerefs(
                    if_block.expr(),
                    &unbound,
                    component.declared_vars(),
                );

                writeln!(
                    out,
                    "if ({replaced}) {{ if (elems[\"{id}_block\"]) {{ elems[\"{id}_block\"].u(dirty); }} else {{ elems[\"{id}_block\"] = create_{id}_block(elems[\"{id}\"].parentNode, elems[\"{id}\"]); }} }} else if (elems[\"{id}_block\"]) {{ elems[\"{id}_block\"].d(); elems[\"{id}_block\"] = null; }}"
                )
                .expect("string formatting should not fail");
            }
            SpecialBlock::For(_) => todo!(),
        }
    }

    for (id, attr, js) in sort_if_testing!(analysis.reactive_data().iter(), |a, b| a.0.cmp(b.0))
        .filter_map(|(id, data)| match data {
            ReactiveData::Mustache(_) => None,
            ReactiveData::AttributeCollection(attrs) => {
                Some(attrs.iter().filter_map(move |attr| match attr {
                    ReactiveAttribute::KeyValue(attr, expr) => Some((id, attr, expr)),
                    ReactiveAttribute::EventListener(_, _) => None,
                }))
            }
            ReactiveData::SpecialBlock(_) => None,
        })
        .flatten()
    {
        let id = analysis.id_overwrites().try_get(*id);
        let unbound = utils::get_unbound_refs(js);
        let dirty_indices = codegen_utils::calc_dirty(&unbound, component.declared_vars());
        let replaced = codegen_utils::replace_namerefs(js, &unbound, component.declared_vars());
        if dirty_indices.is_empty() {
            writeln!(out, "elems[\"{id}\"].setAttribute(\"{attr}\", {replaced});")
                .expect("string formatting should not fail");
        } else {
            writeln!(
                out,
                "if ({dirty_indices}) elems[\"{id}\"].setAttribute(\"{attr}\", {replaced});",
            )
            .expect("writing to string format should not fail");
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
}
