mod cli;
mod compile_wasm;
mod config;
mod indicators;
mod preprocessor;

use std::{
    borrow::Cow,
    env,
    fs::{self, File},
    io::{BufWriter, Write},
    path::Path,
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
use decorous_errors::{DiagnosticBuilder, Report, Severity};
use decorous_frontend::{parse_with_preprocessor, Component};
use handlebars::{no_escape, Handlebars};
use merge::Merge;
use notify::{
    event::{DataChange, ModifyKind},
    EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use serde_json::json;

use crate::{cli::Color, compile_wasm::MainCompiler, indicators::FinishLog, preprocessor::Preproc};

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() -> Result<()> {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    let args = Cli::parse();

    ensure!(
        !(args.render_method == RenderMethod::Prerender && args.modularize),
        "component cannot be both modularized and prerendered!"
    );

    let config = get_config()?;
    let enable_color = match args.color {
        Color::Auto => atty::is(atty::Stream::Stdout),
        Color::Never => false,
        Color::Always => true,
    };

    compile(&args, &config, enable_color)?;

    if args.watch {
        watch(args, config, enable_color)?;
    }

    #[cfg(feature = "dhat-heap")]
    println!();

    Ok(())
}

fn watch(args: Cli, config: Config, enable_color: bool) -> Result<(), anyhow::Error> {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = RecommendedWatcher::new(tx, notify::Config::default())
        .context("error creating up watcher")?;
    watcher
        .watch(&args.input, RecursiveMode::NonRecursive)
        .context("error watching input file")?;
    for res in rx {
        let event = res?;
        debug_assert_eq!(1, event.paths.len(), "watching invalid targets!");
        match event.kind {
            EventKind::Modify(ModifyKind::Data(DataChange::Content)) => {
                println!();
                compile(&args, &config, enable_color)?;
            }
            EventKind::Remove(_) => {
                println!("Input file removed... exiting process");
                break;
            }
            _ => {}
        }
    }

    Ok(())
}

fn compile(args: &Cli, config: &Config, enable_color: bool) -> Result<(), anyhow::Error> {
    let start = Instant::now();
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
    let component = parse_component(&input, config, &args.input, enable_color)?;
    let js_name = if args.modularize {
        format!("{}.mjs", args.out)
    } else {
        format!("{}.js", args.out)
    };
    render_js(args, config, &component, &metadata, &js_name, enable_color)?;
    render_html(args, &component, &metadata, &js_name, enable_color)?;
    if component.css().is_some() {
        render_css(args, component, metadata, enable_color)?;
    }

    {
        let mut log = FinishLog::default();
        log.with_main_message(format!("compiled in ~{:.2?}", start.elapsed()))
            .enable_color(enable_color)
            .with_mod(
                args.optimize
                    .map_or(Cow::Borrowed("debug"), |opt| opt.to_string().into()),
            );
        if args.modularize {
            log.with_mod("modularized");
        }
        println!("{log}");
    }

    Ok(())
}

fn render_css(
    args: &Cli,
    component: Component<'_>,
    metadata: Metadata<'_>,
    enable_color: bool,
) -> Result<(), anyhow::Error> {
    let name = format!("{}.css", args.out);
    render::<CssRenderer, _>(
        &component,
        &mut BufWriter::new(
            File::create(&name).with_context(|| format!("problem creating {name}"))?,
        ),
        &metadata,
    )?;
    println!(
        "{}",
        FinishLog::default()
            .with_main_message("CSS")
            .enable_color(enable_color)
            .with_file(format!("{}.css", args.out))
    );

    Ok(())
}

fn render_js(
    args: &Cli,
    config: &Config,
    component: &Component<'_>,
    metadata: &Metadata<'_>,
    js_name: &str,
    enable_color: bool,
) -> Result<()> {
    let mut report = Report::new();
    if args.strip && component.wasm().is_none() {
        report.add_diagnostic(
            DiagnosticBuilder::new("no WebAssembly to strip", Severity::Warning, 0).build(),
        );
    }
    if !args.build_args.is_empty() && component.wasm().is_none() {
        report.add_diagnostic(
            DiagnosticBuilder::new(
                "no WebAssembly to compile - build args do nothing",
                Severity::Warning,
                0,
            )
            .build(),
        );
    }
    if args.optimize.is_some() && component.wasm().is_none() {
        report.add_diagnostic(
            DiagnosticBuilder::new("no WebAssembly to optimize", Severity::Warning, 0).build(),
        );
    }
    if !report.is_empty() {
        let input_name = args.input.to_string_lossy();
        decorous_errors::fmt::report(&report, &input_name, "...")?;
    }

    let mut out = BufWriter::new(File::create(js_name).context("error creating out file")?);
    let mut wasm_compiler = MainCompiler::new(
        config,
        &args.out,
        &args.build_args,
        args.optimize,
        args.strip,
        enable_color,
    );
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
        "{}",
        FinishLog::default()
            .with_main_message("JavaScript")
            .with_sub_message(args.render_method.to_string())
            .enable_color(enable_color)
            .with_file(js_name)
    );
    out.flush()
        .context("problem flushing buffered writer while rendering")?;

    Ok(())
}

fn get_config() -> Result<Config> {
    let source = env::current_dir().context("error reading current dir")?;
    let config_path = source.ancestors().find_map(|p| {
        let joined = p.join("decor.toml");
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

fn render_html(
    args: &Cli,
    component: &Component,
    meta: &Metadata,
    js_name: &str,
    enable_color: bool,
) -> Result<()> {
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

            println!(
                "{}",
                FinishLog::default()
                    .with_main_message("HTML")
                    .with_file("index.html")
                    .enable_color(enable_color)
            );

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

                println!(
                    "{}",
                    FinishLog::default()
                        .with_main_message("HTML")
                        .with_sub_message("prerender")
                        .with_file("index.html")
                        .enable_color(enable_color)
                );
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
            println!(
                "{}",
                FinishLog::default()
                    .with_main_message("HTML")
                    .with_sub_message("prerender")
                    .with_file(html)
                    .enable_color(enable_color)
            );

            Ok(())
        }
    }
}

fn parse_component<'a>(
    input: &'a str,
    config: &Config,
    file: impl AsRef<Path>,
    enable_color: bool,
) -> Result<Component<'a>> {
    let file_name = file.as_ref().to_string_lossy();
    let preproc = Preproc::new(config);
    let component = match parse_with_preprocessor(input, &preproc) {
        Ok(ast) => {
            let c = Component::new(ast);
            if !c.report().is_empty() {
                decorous_errors::fmt::report(c.report(), &file_name, input)?;
            }
            Ok(c)
        }
        Err(report) => {
            decorous_errors::fmt::report(&report, &file_name, input)?;
            anyhow::bail!("\nthe decorous parser failed");
        }
    };
    println!(
        "{}",
        FinishLog::default()
            .with_main_message("parsed")
            .enable_color(enable_color)
    );
    component
}
