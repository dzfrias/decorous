mod cli;
mod compile_wasm;
mod config;
mod fmt_report;

use std::{
    fs::{self, File},
    io::{self, BufWriter, Write},
};

use anyhow::{Context, Result};
use clap::Parser;
use cli::{Cli, RenderMethod};
use compile_wasm::compile_wasm;
use config::Config;
use decorous_backend::{
    css_render::CssRenderer,
    dom_render::DomRenderer,
    prerender::{HtmlPrerenderer, Prerenderer},
    render, Metadata,
};
use decorous_frontend::{parse, Component};
use fmt_report::fmt_report;
use handlebars::{no_escape, Handlebars};
use serde_json::json;
use superfmt::{
    style::{Color, Modifiers},
    Formatter,
};

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() -> Result<()> {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    let args = Cli::parse();
    // TODO: Search for config
    let config = Config::default();
    let mut stdout = io::stdout();
    let mut formatter = Formatter::new(&mut stdout);

    let input = fs::read_to_string(&args.input).context("error reading provided input file")?;
    let file_name = args
        .input
        .file_stem()
        .expect("file name should never be .. or /, if read was successful")
        .to_string_lossy();
    let metadata = Metadata { name: &file_name };

    formatter.writeln_with_context("parsing...", Modifiers::BOLD)?;
    let component = parse_component(&input)?;
    formatter.writeln_with_context("parsed!", Color::Green)?;

    formatter.writeln_with_context("rendering...", Modifiers::BOLD)?;

    render_js(
        BufWriter::new(File::create(format!("{}.js", args.out))?),
        &component,
        args.render_method,
        &metadata,
    )?;
    let wasm_ext = component
        .wasm()
        .map(|w| compile_wasm(w.lang(), w.body(), metadata.name, &args.out, &config))
        .transpose()?;
    render_html(&args, &component, &metadata, wasm_ext.as_deref())?;
    if component.css().is_some() {
        let name = format!("{}.css", args.out);
        render::<CssRenderer, _>(
            &component,
            &mut BufWriter::new(
                File::create(&name).with_context(|| format!("problem creating {name}"))?,
            ),
            &metadata,
        )?;
    }

    formatter.writeln_with_context("rendered!", Color::Green)?;

    #[cfg(feature = "dhat-heap")]
    println!();

    Ok(())
}

fn render_js<T: io::Write>(
    mut out: BufWriter<T>,
    component: &Component,
    render_method: RenderMethod,
    meta: &Metadata,
) -> Result<()> {
    match render_method {
        RenderMethod::Dom => {
            render::<DomRenderer, _>(component, &mut out, meta).context("problem dom rendering")?;
        }
        RenderMethod::Prerender => {
            render::<Prerenderer, _>(component, &mut out, meta).context("problem prerendering")?;
        }
    }

    out.flush()
        .context("problem flushing buffered writer while rendering")
}

fn render_html(
    args: &Cli,
    component: &Component,
    meta: &Metadata,
    wasm_ext: Option<&str>,
) -> Result<()> {
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(no_escape);
    handlebars.register_template_string("index", include_str!("./templates/template.html"))?;
    match args.render_method {
        RenderMethod::Dom => {
            if !args.html {
                return Ok(());
            }
            let out = File::create("index.html").context("problem creating index.html")?;

            let body = json!({
                "script": format!("{}.js", args.out),
                "css": component.css().is_some().then(|| format!("{}.css", args.out)),
                "name": meta.name,
                "wasm_ext": wasm_ext,
                "html": None::<&str>,
            });

            handlebars.render_template_to_write(
                include_str!("./templates/template.html"),
                &body,
                out,
            )?;

            Ok(())
        }
        RenderMethod::Prerender => {
            if args.html {
                let mut out = vec![];
                render::<HtmlPrerenderer, _>(component, &mut out, meta)?;

                let body = json!({
                    "script": format!("{}.js", args.out),
                    "css": component.css().is_some().then(|| format!("{}.css", args.out)),
                    "name": meta.name,
                    "wasm_ext": wasm_ext,
                    // SAFETY: HtmlPrerenderer only produces valid UTF-8
                    "html": Some(unsafe {
                        std::str::from_utf8_unchecked(&out)
                    }),
                });

                handlebars.render_template_to_write(
                    include_str!("./templates/template.html"),
                    &body,
                    File::create("index.html").context("problem creating index.html")?,
                )?;

                return Ok(());
            }

            let html = format!("{}.html", args.out);

            render::<HtmlPrerenderer, _>(
                component,
                &mut BufWriter::new(
                    File::create(&html).with_context(|| format!("problem creating {}", html))?,
                ),
                meta,
            )?;

            Ok(())
        }
    }
}

fn parse_component(input: &str) -> Result<Component> {
    match parse(input) {
        Ok(ast) => Ok(Component::new(ast)),
        Err(report) => {
            fmt_report(input, &report, &mut io::stderr())?;
            anyhow::bail!("\nthe decorous parser failed");
        }
    }
}
