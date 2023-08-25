mod render_fragment;

use decorous_frontend::{utils, Component};
use itertools::Itertools;
use rslint_parser::AstNode;
use std::{borrow::Cow, io};

use crate::{codegen_utils, Options, RenderBackend, UseResolver, WasmCompiler};
pub(crate) use render_fragment::render_fragment;

pub struct DomRenderer;

impl RenderBackend for DomRenderer {
    fn render<T: io::Write, C, R>(
        out: &mut T,
        component: &Component,
        metadata: &Options<C, R>,
    ) -> io::Result<()>
    where
        C: WasmCompiler<DomRenderer>,
        R: UseResolver,
    {
        render(component, out, metadata)
    }
}

fn render<T: io::Write, C, R>(
    component: &Component,
    render_to: &mut T,
    metadata: &Options<C, R>,
) -> io::Result<()>
where
    C: WasmCompiler<DomRenderer>,
    R: UseResolver,
{
    if let Some(wasm) = component.wasm() {
        let _ = metadata.wasm_compiler.compile(
            crate::CodeInfo {
                lang: wasm.lang(),
                body: wasm.body(),
                exports: component.exports(),
            },
            render_to,
        );
    }

    for use_decl in component.uses() {
        let Some(stem) = use_decl.file_stem() else {
            continue;
        };
        let use_info = metadata.use_resolver.resolve(use_decl)?;
        writeln!(
            render_to,
            "import __decor_{} from \"./{}\";",
            // FIX: Make sure it is a valid JavaScript ident
            stem.to_string_lossy(),
            use_info.loc.display(),
        )?;
    }
    // Hoisted syntax nodes should come first
    for hoist in component.hoist() {
        writeln!(render_to, "{hoist}")?;
    }

    render_init_ctx(render_to, component)?;

    if metadata.modularize {
        writeln!(render_to, "export default function initialize(target) {{")?;
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

    writeln!(render_to, "const ctx = __init_ctx();")?;
    if metadata.modularize {
        writeln!(render_to, "const fragment = create_main_block(target);")?;
    } else {
        writeln!(
            render_to,
            "const fragment = create_main_block(document.getElementById(\"{}\"));",
            metadata.name
        )?;
    }
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

    if metadata.modularize {
        writeln!(render_to, "}}")?;
    }

    Ok(())
}

fn render_init_ctx<W: io::Write>(out: &mut W, component: &Component<'_>) -> io::Result<()> {
    writeln!(out, "function __init_ctx() {{")?;
    writeln!(
        out,
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
            out,
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
                out,
                "let __binding{id} = (ev) => __schedule_update({var_id}, {name} = ev.target.value);"
            )?;
        } else {
            todo!("unbound var lint");
        }
    }
    for (block, id) in component.declared_vars().all_reactive_blocks() {
        let replaced = codegen_utils::replace_assignments(
            block,
            &utils::get_unbound_refs(block),
            component.declared_vars(),
            None,
        );
        writeln!(out, "let __reactive{id} = () => {{ {replaced} }};")?;
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
    writeln!(out, "return [{}];", ctx.join(","))?;
    writeln!(out, "}}")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{NullCompiler, NullResolver};
    use decorous_frontend::parse;

    use super::*;

    fn make_component(input: &str) -> Component {
        Component::new(parse(input).expect("should be valid input"))
    }

    macro_rules! test_render {
        ($input:expr$(, $metadata:expr)?) => {
            let component = make_component($input);
            let mut out = vec![];
            #[allow(unused)]
            let mut metadata = Options { name: "test", modularize: false, use_resolver: NullResolver, wasm_compiler: NullCompiler };
            $(
                metadata = $metadata;
             )?
            render(
                &component,
                &mut out,
                &metadata
            )
            .unwrap();

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
        test_render!("---js let x = 0; --- {(x, x)} #button[@click={() => { x = 3; }}]:Hi");
    }

    #[test]
    fn text_with_only_one_text_node_as_child_is_collapsed() {
        test_render!("#span:hello");
    }

    #[test]
    fn dirty_items_are_in_conditional() {
        test_render!("---js let hello = 0; let test = 1; --- {(hello, test)} #button[@click={() => { test = 3; hello = 3; }}]:Hi");
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

    #[test]
    fn can_render_reactive_blocks() {
        test_render!("---js let x = 0; let y = 0; $: y = x + 1; --- #input[:x:]/input");
    }

    #[test]
    fn can_render_modularize() {
        test_render!(
            "---js let x = 0; --- #p {x} /p",
            Options {
                name: "test",
                modularize: true,
                wasm_compiler: NullCompiler,
                use_resolver: NullResolver
            }
        );
    }

    #[test]
    fn can_have_resolver_for_use_path() {
        test_render!("{#use \"./hello.decor\"} #p:Hello");
    }
}
