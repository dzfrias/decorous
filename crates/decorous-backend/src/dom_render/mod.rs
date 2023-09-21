mod render_fragment;

use decorous_frontend::{utils, Component};
use heck::ToSnekCase;
use itertools::Itertools;
use rslint_parser::AstNode;
use std::{borrow::Cow, io};

use crate::{
    codegen_utils, css_render,
    render_out::{write_html, write_js},
    CodeInfo, Ctx, RenderBackend, RenderOut, Result,
};
pub(crate) use render_fragment::{render_fragment, State};

#[derive(Debug, Default)]
pub struct CsrOptions {
    pub modularize: bool,
}

#[derive(Default)]
pub struct CsrRenderer {
    opts: CsrOptions,
}

impl CsrRenderer {
    pub fn new() -> Self {
        Self {
            opts: CsrOptions::default(),
        }
    }
}

impl RenderBackend for CsrRenderer {
    type Options = CsrOptions;

    fn with_options(&mut self, options: Self::Options) {
        self.opts = options;
    }

    fn render<T: RenderOut>(&self, component: &Component, mut out: T, ctx: &Ctx) -> Result<()> {
        if let Some(css) = component.css() {
            let mut css_out = vec![];
            css_render::render_css(css, &mut css_out, component)?;
            out.write_css(&css_out)?;
        }

        if let Some(info) = &ctx.index_html {
            if component.css().is_some() {
                write_html!(
                    out,
                    include_str!("./templates/index_css.html"),
                    name = ctx.name,
                    script = format!("{}.js", info.basename),
                    css = format!("{}.css", info.basename),
                )?;
            } else {
                write_html!(
                    out,
                    include_str!("./templates/index.html"),
                    name = ctx.name,
                    script = format!("{}.js", info.basename),
                )?;
            }
        }

        if let Some(wasm) = component.wasm() {
            let wasm_prelude = ctx.wasm_compiler.compile(CodeInfo {
                lang: wasm.lang,
                body: wasm.body,
                exports: component.exports(),
            })?;
            out.write_js(wasm_prelude.as_bytes())?;
        };

        for use_decl in component.uses() {
            let Some(stem) = use_decl.file_stem() else {
                continue;
            };
            let use_info = ctx.use_resolver.resolve(use_decl)?;
            write_js!(
                out,
                "import __decor_{} from \"./{}\";",
                stem.to_string_lossy().to_snek_case(),
                use_info.loc.display(),
            )?;
        }

        // Hoisted syntax nodes should come first
        for hoist in component.hoist() {
            write_js!(out, "{hoist}")?;
        }

        render_init_ctx(&mut out.js_handle(), component)?;

        if self.opts.modularize {
            write_js!(out, "export default function initialize(target) {{")?;
        }

        write_js!(
            out,
            "const dirty = new Uint8Array(new ArrayBuffer({}));",
            // Ceiling division to get the amount of bytes needed in the ArrayBuffer
            ((component.declared_vars().len() + 7) / 8)
        )?;

        let state = State {
            name: "main".into(),
            component,
            root: None,
            uses: vec![],
        };
        render_fragment(component.fragment_tree(), state, &mut out.js_handle())?;

        write_js!(out, "const ctx = __init_ctx();")?;
        if self.opts.modularize {
            write_js!(out, "const fragment = create_main_block(target);")?;
        } else {
            write_js!(
                out,
                "const fragment = create_main_block(document.getElementById(\"{}\"));",
                ctx.name
            )?;
        }
        write_js!(out, "let updating = false;")?;
        write_js!(
            out,
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

        if self.opts.modularize {
            write_js!(out, "}}")?;
        }

        Ok(())
    }
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
    use std::io::Write;

    use super::*;
    use crate::{NullCompiler, NullResolver};
    use decorous_errors::Source;
    use decorous_frontend::Parser;

    #[derive(Default)]
    struct TestOut {
        js: Vec<u8>,
        css: Vec<u8>,
        html: Vec<u8>,
    }

    impl RenderOut for TestOut {
        fn write_html(&mut self, buf: &[u8]) -> io::Result<()> {
            self.html.write_all(buf)
        }

        fn write_css(&mut self, buf: &[u8]) -> io::Result<()> {
            self.css.write_all(buf)
        }

        fn write_js(&mut self, buf: &[u8]) -> io::Result<()> {
            self.js.write_all(buf)
        }

        fn js_handle(&mut self) -> &mut dyn io::Write {
            &mut self.js
        }
    }

    macro_rules! test_render {
        ($input:expr) => {
            test_render!($input, Ctx::default(), CsrOptions::default())
        };
        ($input:expr, $metadata:expr) => {
            test_render!($input, $metadata, CsrOptions::default())
        };
        ($input:expr, $metadata:expr, $opts:expr) => {
            let parser = Parser::new($input);
            let errs = decorous_errors::stderr(Source {
                src: $input,
                name: "TEST".to_owned(),
            });
            let ctx = decorous_frontend::Ctx {
                errs,
                ..Default::default()
            };
            let mut component = Component::new(parser.parse().expect("should be valid input"), ctx);
            component.run_passes().unwrap();
            let mut out = TestOut::default();
            let mut renderer = CsrRenderer::new();
            renderer.with_options($opts);
            renderer.render(&component, &mut out, &$metadata).unwrap();

            insta::assert_snapshot!(String::from_utf8(out.js).unwrap());
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
        let src = "---js let x = 0; --- #p {x} /p";
        test_render!(
            src,
            Ctx {
                name: "test",
                wasm_compiler: &NullCompiler,
                use_resolver: &NullResolver,
                errs: decorous_errors::stderr(Source {
                    name: "TEST".to_owned(),
                    src
                }),
                index_html: None,
            },
            CsrOptions { modularize: true }
        );
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
