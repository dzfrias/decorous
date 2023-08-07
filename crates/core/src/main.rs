mod cli;
mod compile_wasm;
mod config;
mod fmt_report;
mod preprocessor;

use std::{
    borrow::Cow,
    env,
    fs::{self, File},
    io::{self, BufWriter, Write},
    time::Instant,
};

use anyhow::{ensure, Context, Result};
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

use crate::{compile_wasm::MainCompiler, preprocessor::Preproc};

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

// ANSI green text
pub const FINISHED: &str = "\x1b[32;1mDONE\x1b[0m";

fn main() -> Result<()> {
    let start = Instant::now();

    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    let args = Cli::parse();

    ensure!(
        !(args.render_method == RenderMethod::Prerender && args.modularize),
        "component cannot be both modularized and prerendered!"
    );

    let config = get_config()?;
    let input = fs::read_to_string(&args.input).context("error reading provided input file")?;

    let metadata = Metadata {
        name: {
            &args
                .input
                .file_stem()
                .expect("file name should never be .. or /, if read was successful")
                .to_string_lossy()
        },
        modularize: args.modularize,
    };
    let component = parse_component(&input, &config)?;

    let js_name = if args.modularize {
        format!("{}.mjs", args.out)
    } else {
        format!("{}.js", args.out)
    };

    render_js(&args, config, &component, &metadata, &js_name)?;
    render_html(&args, &component, &metadata, &js_name)?;
    if component.css().is_some() {
        render_css(&args, component, metadata)?;
    }

    let mods = {
        let mut mods = Vec::with_capacity(2);
        let opt = args
            .optimize
            .map(|opt| Cow::Owned(opt.to_string()))
            .unwrap_or(Cow::Borrowed("debug"));
        mods.push(opt);
        if args.modularize {
            mods.push(Cow::Borrowed("modularized"))
        }
        mods
    };

    println!(
        "  {FINISHED} compiled in ~{:.2?} ({})",
        start.elapsed(),
        mods.join(" + ")
    );

    #[cfg(feature = "dhat-heap")]
    println!();

    Ok(())
}

fn render_css(
    args: &Cli,
    component: Component<'_>,
    metadata: Metadata<'_>,
) -> Result<(), anyhow::Error> {
    let name = format!("{}.css", args.out);
    render::<CssRenderer, _>(
        &component,
        &mut BufWriter::new(
            File::create(&name).with_context(|| format!("problem creating {name}"))?,
        ),
        &metadata,
    )?;
    println!("  {FINISHED} CSS: (\x1b[34m{}.css\x1b[0m)", args.out);
    Ok(())
}

fn render_js(
    args: &Cli,
    config: Config,
    component: &Component<'_>,
    metadata: &Metadata<'_>,
    js_name: &str,
) -> Result<()> {
    let mut out = BufWriter::new(File::create(js_name).context("error creating out file")?);
    let mut wasm_compiler = MainCompiler::new(&config, &args.out, &args.build_args, args.optimize);
    match args.render_method {
        RenderMethod::Csr => {
            render_with_wasm::<DomRenderer, _, _>(
                component,
                &mut out,
                metadata,
                &mut wasm_compiler,
            )
            .context("error during rendering")?;
        }
        RenderMethod::Prerender => {
            render_with_wasm::<Prerenderer, _, _>(
                component,
                &mut out,
                metadata,
                &mut wasm_compiler,
            )
            .context("error during rendering")?;
        }
    }
    println!(
        "  {FINISHED} JavaScript: {} (\x1b[34m{}.js\x1b[0m)",
        args.render_method, args.out
    );
    out.flush()
        .context("problem flushing buffered writer while rendering")?;

    Ok(())
}

fn get_config() -> Result<Config> {
    let source = env::current_dir().context("error reading current dir")?;
    let config_path = source.ancestors().find_map(|p| {
        let joined = p.join(&"decor.toml");
        joined.exists().then_some(joined)
    });
    if let Some(p) = config_path {
        let contents = fs::read_to_string(p).context("error reading config file")?;
        let cfg = toml::from_str::<Config>(&contents).context("error parsing config")?;
        let mut default = Config::default();
        default.merge(cfg);
        Ok(default)
    } else {
        Ok(Config::default())
    }
}

fn render_html(args: &Cli, component: &Component, meta: &Metadata, js_name: &str) -> Result<()> {
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(no_escape);
    handlebars.register_template_string("index", include_str!("./templates/template.html"))?;
    match args.render_method {
        RenderMethod::Csr => {
            if !args.html {
                return Ok(());
            }
            let out = File::create("index.html").context("problem creating index.html")?;

            let body = json!({
                "script": js_name,
                "css": component.css().is_some().then(|| format!("{}.css", args.out)),
                "name": meta.name,
                "html": None::<&str>,
            });

            handlebars.render_template_to_write(
                include_str!("./templates/template.html"),
                &body,
                out,
            )?;

            println!("  {FINISHED} HTML (\x1b[34mindex.html\x1b[0m)");

            Ok(())
        }
        RenderMethod::Prerender => {
            if args.html {
                let mut out = vec![];
                render::<HtmlPrerenderer, _>(component, &mut out, meta)
                    .context("error when rendering HTML")?;

                let body = json!({
                    "script": js_name,
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

                println!("  {FINISHED} HTML: prerender (\x1b[34mindex.html\x1b[0m)");
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
            println!("  {FINISHED} HTML: prerender (\x1b[34m{html}\x1b[0m)");

            Ok(())
        }
    }
}

fn parse_component<'a>(input: &'a str, config: &Config) -> Result<Component<'a>> {
    let preproc = Preproc::new(config);
    let component = match parse_with_preprocessor(input, &preproc) {
        Ok(ast) => Ok(Component::new(ast)),
        Err(report) => {
            fmt_report(input, &report, &mut io::stderr())?;
            anyhow::bail!("\nthe decorous parser failed");
        }
    };
    println!("  {FINISHED} parsed");
    component
}
