use crate::renderer::Renderer;
use decorous_frontend::{utils, Component};
use itertools::Itertools;
use std::io;

mod renderer;
pub mod replace;

pub fn render<T: io::Write>(component: &Component, render_to: &mut T) -> io::Result<()> {
    macro_rules! render {
        ($lifecycle:ident) => {
            for node in component.fragment_tree() {
                node.$lifecycle(render_to, component.declared_vars())?;
            }
        };
    }

    // Hoisted syntax nodes should come first
    for hoist in component.hoist() {
        writeln!(render_to, "{hoist}")?;
    }

    writeln!(
        render_to,
        "const dirty = new Uint8Array(new ArrayBuffer({}));",
        // Ceiling division to get the amount of bytes needed in the ArrayBuffer
        ((component.declared_vars().len() + 8 - 1) / 8)
    )?;
    writeln!(render_to, "function make_fragment(ctx) {{")?;
    render!(init);

    writeln!(render_to, "return {{")?;
    writeln!(render_to, "c() {{")?;
    render!(create);
    writeln!(render_to, "}},")?;

    writeln!(render_to, "m(target) {{")?;
    render!(mount);
    writeln!(render_to, "}},")?;

    writeln!(render_to, "u(ctx, dirty) {{")?;
    render!(update);
    writeln!(render_to, "}},")?;

    writeln!(render_to, "d() {{")?;
    render!(detach);
    writeln!(render_to, "}},")?;

    writeln!(render_to, "}}")?;
    writeln!(render_to, "}}")?;

    writeln!(render_to, "function __init_ctx() {{")?;
    writeln!(
        render_to,
        "{}",
        component
            .toplevel_nodes()
            .iter()
            .map(|node| {
                if node.substitute_assign_refs {
                    replace::replace_assignments(
                        &node.node,
                        &utils::get_unbound_refs(&node.node),
                        component.declared_vars(),
                    )
                } else {
                    node.node.to_string()
                }
            })
            .join("\n")
    )?;

    let mut ctx = vec![""; component.declared_vars().len()];
    for (name, idx) in component.declared_vars() {
        ctx[*idx as usize] = name;
    }
    writeln!(render_to, "return [{}];", ctx.join(","))?;
    writeln!(render_to, "}}")?;

    writeln!(render_to, "const ctx = __init_ctx();")?;
    writeln!(render_to, "const fragment = make_fragment(ctx);")?;
    writeln!(render_to, "fragment.c();")?;
    writeln!(render_to, "fragment.m(document.getElementById(\"app\"));")?;
    writeln!(render_to, "let updating = false;")?;
    writeln!(
        render_to,
        "function __schedule_update(ctx_idx, val) {{
ctx[ctx_idx] = val;
dirty[Math.max(Math.ceil(ctx_idx / 8) - 1, 0)] |= 1 << (ctx_idx % 8);
if (updating) return;
updating = true;
Promise.resolve().then(() => {{
fragment.u(ctx, dirty);
updating = false;
dirty.fill(0);
}});
}}"
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use decorous_frontend::{ParseError, Parser};

    use super::*;

    fn make_component(input: &str) -> Component {
        let parser = Parser::new(input);
        let parse_res = parser.parse();
        assert_eq!(Vec::<ParseError>::new(), parse_res.1);
        Component::new(parse_res.0)
    }

    macro_rules! test_render {
        ($input:expr) => {
            let component = make_component($input);
            let mut out = vec![];
            render(&component, &mut out).unwrap();

            println!("{}", String::from_utf8(out.clone()).unwrap());
            insta::assert_snapshot!(String::from_utf8(out).unwrap());
        };
    }

    #[test]
    fn basic_render_works() {
        test_render!(
            "<script>let x = 3; function remake_x() { x = 44; }</script><p>{`${x}hello`}</p>"
        );
    }

    #[test]
    fn render_with_attrs_works() {
        test_render!(
            "<script>let x = 3; function remake_x() { x = 44; }</script><div class={x}></div>"
        );
    }

    #[test]
    fn render_with_event_listeners_works() {
        test_render!(
            "<script>let x = 3;</script><p>{x}</p><button on:click={() => {if (x == 3) { x = 44; }} }>Clickme</button>"
        );
    }

    #[test]
    fn imports_are_hoisted_out_of_context_init() {
        test_render!("<script>import data from \"data\"; let x = 3;</script><p>{x}</p>");
    }

    #[test]
    fn everything_in_script_tag_is_in_context_init() {
        test_render!("<script>let x = 3; x = 4;</script>");
    }
}
