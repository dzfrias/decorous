mod cli;
mod compile_wasm;
mod config;
mod fmt_report;
mod preprocessor;

use std::{
    env,
    fs::{self, File},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::Parser;
use cli::{Cli, RenderMethod};
use config::Config;
use decorous_backend::{
    css_render::CssRenderer,
    dom_render::DomRenderer,
    prerender::{HtmlPrerenderer, Prerenderer},
    render, render_with_wasm, Metadata,
};
use decorous_frontend::{parse_with_preprocessor, Component};
use fmt_report::fmt_report;
use handlebars::{no_escape, Handlebars};
use merge::Merge;
use serde_json::json;
use superfmt::{
    style::{Color, Modifiers},
    Formatter,
};

use crate::{compile_wasm::MainCompiler, preprocessor::Preproc};

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() -> Result<()> {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    let args = Cli::parse();
    let config = {
        let config_path = get_config_path(&env::current_dir()?, "decor.toml");
        if let Some(p) = config_path {
            let contents = fs::read_to_string(p).context("error reading config file")?;
            let cfg = toml::from_str::<Config>(&contents)?;
            let mut default = Config::default();
            default.merge(cfg);
            default
        } else {
            Config::default()
        }
    };
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
    let component = parse_component(&input, &config)?;
    formatter.writeln_with_context("parsed!", Color::Green)?;

    formatter.writeln_with_context("rendering...", Modifiers::BOLD)?;

    let mut out = BufWriter::new(File::create(format!("{}.js", args.out))?);
    let mut wasm_compiler = MainCompiler::new(&config, &args.out, &args.build_args);
    match args.render_method {
        RenderMethod::Dom => {
            render_with_wasm::<DomRenderer, _, _>(
                &component,
                &mut out,
                &metadata,
                &mut wasm_compiler,
            )?;
        }
        RenderMethod::Prerender => {
            render_with_wasm::<Prerenderer, _, _>(
                &component,
                &mut out,
                &metadata,
                &mut wasm_compiler,
            )?;
        }
    }
    out.flush()
        .context("problem flushing buffered writer while rendering")?;
    drop(out);

    render_html(&args, &component, &metadata)?;
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

fn get_config_path(source: &Path, config_name: impl AsRef<Path>) -> Option<PathBuf> {
    source.ancestors().find_map(|p| {
        let joined = p.join(&config_name);
        joined.exists().then_some(joined)
    })
}

fn render_html(args: &Cli, component: &Component, meta: &Metadata) -> Result<()> {
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

fn parse_component<'a>(input: &'a str, config: &Config) -> Result<Component<'a>> {
    let preproc = Preproc::new(config);
    match parse_with_preprocessor(input, &preproc) {
        Ok(ast) => Ok(Component::new(ast)),
        Err(report) => {
            fmt_report(input, &report, &mut io::stderr())?;
            anyhow::bail!("\nthe decorous parser failed");
        }
    }
}
