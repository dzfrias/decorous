mod render_fragment;

use decorous_frontend::{utils, Component};
use itertools::Itertools;
use rslint_parser::AstNode;
use std::{borrow::Cow, io};

use crate::{codegen_utils, Metadata, RenderBackend};
pub(crate) use render_fragment::render_fragment;

pub struct DomRenderer;

impl RenderBackend for DomRenderer {
    fn render<T: io::Write>(
        out: &mut T,
        component: &Component,
        metadata: &Metadata,
    ) -> io::Result<()> {
        render(component, out, metadata)
    }
}

fn render<T: io::Write>(
    component: &Component,
    render_to: &mut T,
    metadata: &Metadata,
) -> io::Result<()> {
    // Hoisted syntax nodes should come first
    for hoist in component.hoist() {
        writeln!(render_to, "{hoist}")?;
    }

    writeln!(
        render_to,
        "const dirty = new Uint8Array(new ArrayBuffer({}));",
        // Ceiling division to get the amount of bytes needed in the ArrayBuffer
        ((component.declared_vars().len() + 7) / 8)
    )?;
    render_fragment(
        component.fragment_tree(),
        None,
        component.declared_vars(),
        "main",
        render_to,
    )?;

    writeln!(render_to, "function __init_ctx() {{")?;
    writeln!(
        render_to,
        "{}",
        component
            .toplevel_nodes()
            .iter()
            .map(|node| {
                if node.substitute_assign_refs {
                    codegen_utils::replace_assignments(
                        &node.node,
                        &utils::get_unbound_refs(&node.node),
                        component.declared_vars(),
                        None,
                    )
                } else {
                    node.node.to_string()
                }
            })
            .join("\n")
    )?;

    for (arrow_expr, (idx, scope)) in component.declared_vars().all_arrow_exprs() {
        writeln!(
            render_to,
            "let __closure{idx} = {};",
            codegen_utils::replace_assignments(
                arrow_expr.syntax(),
                &utils::get_unbound_refs(arrow_expr.syntax()),
                component.declared_vars(),
                *scope
            )
        )?;
    }

    for (name, id) in component.declared_vars().all_bindings() {
        if let Some(var_id) = component.declared_vars().get_var(name, None) {
            writeln!(
                render_to,
                "let __binding{id} = (ev) => __schedule_update({var_id}, {name} = ev.target.value);"
            )?;
        } else {
            todo!("unbound var lint");
        }
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
    writeln!(render_to, "return [{}];", ctx.join(","))?;
    writeln!(render_to, "}}")?;

    writeln!(render_to, "const ctx = __init_ctx();")?;
    writeln!(
        render_to,
        "const fragment = create_main_block(document.getElementById(\"{}\"));",
        metadata.name
    )?;
    writeln!(render_to, "let updating = false;")?;
    writeln!(
        render_to,
        "function __schedule_update(ctx_idx, val) {{
ctx[ctx_idx] = val;
dirty[Math.max(Math.ceil(ctx_idx / 8) - 1, 0)] |= 1 << (ctx_idx % 8);
if (updating) return;
updating = true;
Promise.resolve().then(() => {{
fragment.u(dirty);
updating = false;
dirty.fill(0);
}});
}}"
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use decorous_frontend::parse;

    use super::*;

    fn make_component(input: &str) -> Component {
        Component::new(parse(input).expect("should be valid input"))
    }

    macro_rules! test_render {
        ($input:expr) => {
            let component = make_component($input);
            let mut out = vec![];
            render(&component, &mut out, &Metadata { name: "test" }).unwrap();

            insta::assert_snapshot!(String::from_utf8(out).unwrap());
        };
    }

    #[test]
    fn basic_render_works() {
        test_render!("---js let x = 3; function remake_x() { x = 44; } --- #p {`${x}hello`} /p #button[@click={remake_x}]:Click me");
    }

    #[test]
    fn render_with_attrs_works() {
        test_render!("---js let x = 3; function remake_x() { x = 44; } --- #div[class={x}]/div");
    }

    #[test]
    fn render_with_event_listeners_works() {
        test_render!(
            "---js let x = 3; --- #p {x} /p #button[@click={() => {if (x == 3) { x = 44; }} }]Clickme /button"
        );
    }

    #[test]
    fn imports_are_hoisted_out_of_context_init() {
        test_render!("---js import data from \"data\"; let x = 3; --- #p {x} /p");
    }

    #[test]
    fn everything_in_script_tag_is_in_context_init() {
        test_render!("---js let x = 3; x = 4; ---");
    }

    #[test]
    fn can_write_basic_text_nodes() {
        test_render!("hello");
    }

    #[test]
    fn basic_elements_and_html_are_collapsed() {
        test_render!("#div text #div/div /div");
    }

    #[test]
    fn can_write_mustache_tags() {
        test_render!("---js let x = 0; --- {(x, x)}");
    }

    #[test]
    fn text_with_only_one_text_node_as_child_is_collapsed() {
        test_render!("#span:hello");
    }

    #[test]
    fn dirty_items_are_in_conditional() {
        test_render!("---js let hello = 0; let test = 1; --- {(hello, test)}");
    }

    #[test]
    fn can_render_else_blocks() {
        test_render!("---js let hello = 0; --- {#if hello == 0} wow {:else} woah {/if}");
    }

    #[test]
    fn can_render_for_blocks() {
        test_render!("{#for i in [1, 2, 3]} {i} {/for}");
    }

    #[test]
    fn closures_with_scoped_var_as_part_of_body_take_the_scoped_var_as_argument() {
        test_render!("{#for i in [1, 2, 3]} #button[@click={() => console.log(i)}]:Click {/for}");
    }

    #[test]
    fn mustaches_with_no_reactives_are_not_updated() {
        test_render!("{1 + 1} #p[class={11}]:Woah");
    }

    #[test]
    fn reactive_css_applies_to_root_element() {
        test_render!("---js let color = \"red\"; --- ---css p { color: {color}; } ---");
    }

    #[test]
    fn dirty_items_from_reactive_css_are_merged_into_one() {
        test_render!(
            "---js let color = \"red\"; let bg = \"green\" --- ---css p { color: {color}; background: {bg}; } --- #button[@click={() => { color = 1; bg = 3; }}]:Click"
        );
    }

    #[test]
    fn can_render_bindings() {
        test_render!("---js let x = 0; --- #input[:x:]/input");
    }
}
