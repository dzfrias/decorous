use std::{fmt::Write, io};

mod html_render;
mod node_analyzer;

use decorous_frontend::{utils, Component};

use crate::{codegen_utils, replace};

use self::{
    html_render::HtmlFmt,
    node_analyzer::analyzers::{Analysis, ElementAttribute, ElementData},
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

    let (elems, ctx_init_body, update_body) = (
        render_elements(&analysis),
        render_ctx_init(component, &analysis),
        render_update_body(component, &analysis),
    );

    // Write finished segments
    write!(
        js_out,
        include_str!("./template.js"),
        dirty_items = ((component.declared_vars().all_vars().len() + 7) / 8),
        elems = elems,
        ctx_body = ctx_init_body,
        update_body = update_body
    )?;

    Ok(())
}

fn render_elements(analysis: &Analysis) -> String {
    let mut out = String::new();
    write!(out, "{{").expect("string formatting should not fail");
    for (id, data) in sort_if_testing!(analysis.elem_data().iter(), |a, b| a.0.cmp(b.0)) {
        let id = analysis.id_overwrites().try_get(*id);
        match data {
            ElementData::Mustache(_) => {
                write!(out, "\"{id}\":replace(document.getElementById(\"{id}\")),")
                    .expect("string formatting should not fail")
            }
            ElementData::AttributeCollection(_) => {
                write!(out, "\"{id}\":document.getElementById(\"{id}\"),")
                    .expect("string formatting should not fail")
            }
        }
    }
    write!(out, "}}").expect("string formatting should not fail");
    out
}

fn render_ctx_init(component: &Component, analysis: &Analysis) -> String {
    let mut out = String::new();

    for node in component.toplevel_nodes() {
        if node.substitute_assign_refs {
            let replacement = replace::replace_assignments(
                &node.node,
                &utils::get_unbound_refs(&node.node),
                component.declared_vars(),
            );
            writeln!(out, "{}", replacement).expect("string formatting should not fail");
        } else {
            writeln!(out, "{}", node.node).expect("string formatting should not fail");
        }
    }

    for (id, event, expr) in sort_if_testing!(analysis.elem_data().iter(), |a, b| a.0.cmp(b.0))
        .filter_map(|(id, data)| match data {
            ElementData::Mustache(_) => None,
            ElementData::AttributeCollection(elems) => {
                Some(elems.iter().filter_map(move |attr| match attr {
                    ElementAttribute::KeyValue(_, _) => None,
                    ElementAttribute::EventListener(expr, handler) => Some((id, expr, handler)),
                }))
            }
        })
        .flatten()
    {
        let id = analysis.id_overwrites().try_get(*id);
        let replaced = replace::replace_assignments(
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

    let mut ctx = vec![""; component.declared_vars().all_vars().len()];
    for (name, idx) in component.declared_vars().all_vars() {
        ctx[*idx as usize] = name;
    }
    writeln!(out, "return [{}];", ctx.join(",")).expect("string format should not fail");

    out
}

fn render_update_body(component: &Component, analysis: &Analysis) -> String {
    let mut out = String::new();

    for (idx, js) in sort_if_testing!(analysis.elem_data().iter(), |a, b| a.0.cmp(b.0)).filter_map(
        |(id, data)| match data {
            ElementData::Mustache(js) => Some((id, js)),
            ElementData::AttributeCollection(_) => None,
        },
    ) {
        let unbound = utils::get_unbound_refs(js);
        let dirty_indices = codegen_utils::calc_dirty(&unbound, component.declared_vars());
        let replaced = replace::replace_namerefs(js, &unbound, component.declared_vars());

        if dirty_indices.is_empty() {
            writeln!(out, "elems[{idx}].data = {replaced}")
                .expect("string formatting should not fail");
        } else {
            writeln!(out, "if ({dirty_indices}) elems[{idx}].data = {replaced};",)
                .expect("writing to string format should not fail");
        }
    }

    for (id, attr, js) in analysis
        .elem_data()
        .iter()
        .filter_map(|(id, data)| match data {
            ElementData::Mustache(_) => None,
            ElementData::AttributeCollection(attrs) => {
                Some(attrs.iter().filter_map(move |attr| match attr {
                    ElementAttribute::KeyValue(attr, expr) => Some((id, attr, expr)),
                    ElementAttribute::EventListener(_, _) => None,
                }))
            }
        })
        .flatten()
    {
        let id = analysis.id_overwrites().try_get(*id);
        let unbound = utils::get_unbound_refs(js);
        let dirty_indices = codegen_utils::calc_dirty(&unbound, component.declared_vars());
        let replaced = replace::replace_namerefs(js, &unbound, component.declared_vars());
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
