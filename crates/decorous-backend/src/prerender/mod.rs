mod render_ast;

use std::{borrow::Cow, collections::HashMap, io};

use crate::{
    codegen_utils, css_render,
    render_out::{write_html, write_js},
    CodeInfo, Ctx, RenderBackend, RenderOut, Result,
};
use decorous_frontend::{utils, Component};
use heck::ToSnekCase;
use render_ast::*;
use rslint_parser::AstNode;

#[derive(Default)]
pub struct Prerenderer;

impl RenderBackend for Prerenderer {
    type Options = ();

    fn with_options(&mut self, _options: Self::Options) {}

    fn render<T: RenderOut>(&self, component: &Component, mut out: T, ctx: &Ctx<'_>) -> Result<()> {
        if let Some(wasm) = component.wasm.as_ref() {
            let wasm_prelude = ctx.wasm_compiler.compile(CodeInfo {
                lang: wasm.lang,
                body: wasm.body,
                exports: &component.exports,
            })?;
            out.write_js(wasm_prelude.as_bytes())?;
        }

        let mut output = Output::default();
        let mut state = State {
            component,
            id_overwrites: HashMap::new(),
            style_cache: None,
            uses: vec![],
        };

        for node in &component.fragment_tree {
            node.render(&mut state, &mut output, &());
        }

        let html = unsafe { String::from_utf8_unchecked(output.html) };
        if let Some(info) = &ctx.index_html {
            if component.css.is_some() {
                write_html!(
                    out,
                    include_str!("./templates/index_css.html"),
                    script = format!("{}.js", info.basename),
                    html = html,
                    css = format!("{}.css", info.basename),
                )?;
            } else {
                write_html!(
                    out,
                    include_str!("./templates/index.html"),
                    script = format!("{}.js", info.basename),
                    html = html
                )?;
            }
        } else {
            out.write_html(html.as_bytes())?;
        }

        if let Some(css) = component.css.as_ref() {
            let mut css_out = vec![];
            css_render::render_css(css, &mut css_out, component)?;
            out.write_css(&css_out)?;
        }

        for use_decl in &component.uses {
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

        let has_reactive_variables = !component.declared_vars.all_vars().is_empty();

        if has_reactive_variables {
            let vars = (component.declared_vars.all_vars().len() + 7) / 8;
            write_js!(
                out,
                "const dirty = new Uint8Array(new ArrayBuffer({vars}));"
            )?;
        }

        // Hoists
        for hoist in &component.hoist {
            write_js!(out, "{hoist}")?;
        }
        out.write_js(&output.hoists)?;

        if !output.elements.is_empty() {
            // Write elements
            let elems = unsafe { String::from_utf8_unchecked(output.elements) };
            write_js!(
                out,
                concat!(
                    "const elems = {{{}}}\n",
                    include_str!("./templates/replace.js")
                ),
                elems
            )?;
        }

        if !output.ctx_init.is_empty()
            || !component.declared_vars.is_empty()
            || !component.toplevel_nodes.is_empty()
        {
            write_ctx_init(&mut out, component, &output.ctx_init)?;

            write_js!(out, "const ctx = __init_ctx();")?;
            if has_reactive_variables {
                write_js!(out, "let updating = false;")?;
            }
        }

        if !output.updates.is_empty() || !component.declared_vars.all_reactive_blocks().is_empty() {
            write_update(&mut out, component, &output.updates)?;
            write_js!(
                out,
                "dirty.fill(255);
__update(dirty, true);
dirty.fill(0);"
            )?;
        }

        if has_reactive_variables {
            write_js!(out, include_str!("./templates/schedule_update.js"))?;
        }

        Ok(())
    }
}

impl Prerenderer {
    pub fn new() -> Self {
        Self
    }
}

fn write_ctx_init<T: RenderOut>(
    out: &mut T,
    component: &Component<'_>,
    body: &[u8],
) -> io::Result<()> {
    write_js!(out, "function __init_ctx() {{")?;
    for (arrow_expr, (idx, scope_id)) in component.declared_vars.all_arrow_exprs() {
        write_js!(out, "  let __closure{idx} = {};", {
            codegen_utils::replace_assignments(
                arrow_expr.syntax(),
                &utils::get_unbound_refs(arrow_expr.syntax()),
                &component.declared_vars,
                *scope_id,
            )
        })?;
    }
    for node in &component.toplevel_nodes {
        if node.substitute_assign_refs {
            let replacement = codegen_utils::replace_assignments(
                &node.node,
                &utils::get_unbound_refs(&node.node),
                &component.declared_vars,
                None,
            );
            let _ = write_js!(out, "  {replacement}");
        } else {
            let _ = write_js!(out, "  {}", node.node);
        }
    }
    out.write_js(body)?;
    for (block, id) in component.declared_vars.all_reactive_blocks() {
        let replaced = codegen_utils::replace_assignments(
            block,
            &utils::get_unbound_refs(block),
            &component.declared_vars,
            None,
        );
        write_js!(out, "  let __reactive{id} = () => {{ {replaced} }};")?;
    }

    let mut ctx = vec![Cow::Borrowed("undefined"); component.declared_vars.len()];
    for (name, idx) in component.declared_vars.all_vars() {
        ctx[*idx as usize] = Cow::Borrowed(name);
    }
    for (idx, _) in component.declared_vars.all_arrow_exprs().values() {
        ctx[*idx as usize] = Cow::Owned(format!("__closure{idx}"));
    }
    for idx in component.declared_vars.all_bindings().values() {
        ctx[*idx as usize] = Cow::Owned(format!("__binding{idx}"));
    }
    for idx in component.declared_vars.all_reactive_blocks().values() {
        ctx[*idx as usize] = Cow::Owned(format!("__reactive{idx}"));
    }
    write_js!(out, "  return [{}];\n}}", ctx.join(","))?;

    Ok(())
}

fn write_update<T: RenderOut>(
    out: &mut T,
    component: &Component<'_>,
    body: &[u8],
) -> io::Result<()> {
    write_js!(out, "function __update(dirty, initial) {{")?;
    for (block, id) in component.declared_vars.all_reactive_blocks() {
        let unbound = utils::get_unbound_refs(block);
        let dirty = codegen_utils::calc_dirty(&unbound, &component.declared_vars, None);
        write_js!(out, "  if ({dirty}) {{ ctx[{id}](); }}")?;
    }
    out.write_js(body)?;
    write_js!(out, "}}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use decorous_errors::Source;
    use decorous_frontend::{Component, Parser};
    use std::{fmt::Write, io::Write as IoWrite};

    use super::*;

    fn make_component(input: &str) -> Component {
        let parser = Parser::new(input);
        let ctx = decorous_frontend::Ctx {
            errs: decorous_errors::stderr(Source {
                src: input,
                name: "TEST".to_owned(),
            }),
            ..Default::default()
        };
        let mut c = Component::new(parser.parse().expect("should be valid input"), ctx);
        c.run_passes().unwrap();
        c
    }

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
        ($($input:expr),+) => {
            $(
                let component = make_component($input);
                let mut out = TestOut::default();
                let renderer = Prerenderer::new();
                renderer.render(&component, &mut out, &Ctx::default()).unwrap();
                let mut output = format!("{}\n---\n{}", String::from_utf8(out.js).unwrap(), String::from_utf8(out.html).unwrap());
                if !out.css.is_empty() {
                    write!(output, "\n---\n{}", String::from_utf8(out.css).unwrap()).unwrap();
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
