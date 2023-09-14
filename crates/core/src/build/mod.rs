mod compile_wasm;
mod global_ctx;
mod preprocessor;
mod resolver;

use std::{
    borrow::Cow,
    fs::{self, File},
    io::{self, BufWriter, Write},
    time::Instant,
};

use anyhow::{ensure, Context, Result};
use decorous_backend::{
    css_render::render_css as render_css_backend, dom_render::render as dom_render,
    prerender::render as prerender, Options,
};
use decorous_errors::{DiagnosticBuilder, DynErrStream, Report, Severity, Source};
use decorous_frontend::{Component, Ctx, Parser};
use handlebars::{no_escape, Handlebars};
use notify::{
    event::{DataChange, ModifyKind},
    EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use serde_json::json;

use crate::{
    build::{global_ctx::GlobalCtx, resolver::Resolver},
    cli::{Build, RenderMethod},
    config::Config,
    indicators::FinishLog,
    utils,
};
use compile_wasm::MainCompiler;
use preprocessor::Preproc;

pub fn build(args: &Build) -> Result<()> {
    ensure!(
        !(args.render_method == RenderMethod::Prerender && args.modularize),
        "component cannot be both modularized and prerendered!"
    );

    let config = utils::get_config()?;
    compile(args, &config)?;

    if args.watch {
        watch(args, &config)?;
    }

    Ok(())
}

fn compile(args: &Build, config: &Config) -> Result<(), anyhow::Error> {
    let start = Instant::now();

    let input = fs::read_to_string(&args.input).context("error reading provided input file")?;
    let errs = DynErrStream::new(
        Box::new(io::stderr()),
        Source {
            src: &input,
            name: args.input.to_string_lossy().to_string(),
        },
    );
    let global_ctx = GlobalCtx { config, args, errs };
    let compiler = MainCompiler::new(config, args);
    let metadata = Options {
        name: {
            &args
                .input
                .file_stem()
                .expect("file name should never be .. or /, if read was successful")
                .to_string_lossy()
        },
        modularize: args.modularize,
        wasm_compiler: &compiler,
        use_resolver: &Resolver {
            global_ctx: &global_ctx,
            compiler: &compiler,
        },
    };

    let component = parse_component(&input, &global_ctx)?;
    warn_on_unused_wasm(&global_ctx, &component)?;
    render_all(&global_ctx, &component, &metadata)?;
    if component.css().is_some() {
        render_css(&global_ctx, &component)?;
    }

    {
        let mut log = FinishLog::default();
        log.with_main_message(format!("compiled in ~{:.2?}", start.elapsed()))
            .enable_color(args.color)
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

fn watch(args: &Build, config: &Config) -> Result<(), anyhow::Error> {
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
                compile(args, config)?;
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

fn render_css(global_ctx: &GlobalCtx, component: &Component<'_>) -> Result<(), anyhow::Error> {
    let Some(css) = component.css() else {
        return Ok(());
    };
    let name = format!("{}.css", global_ctx.args.out);
    render_css_backend(
        css,
        &mut BufWriter::new(
            File::create(&name).with_context(|| format!("problem creating {name}"))?,
        ),
        component,
    )
    .context("problem rendering css")?;
    println!(
        "{}",
        FinishLog::default()
            .with_main_message("CSS")
            .enable_color(global_ctx.args.color)
            .with_file(format!("{}.css", global_ctx.args.out))
    );

    Ok(())
}

fn warn_on_unused_wasm(global_ctx: &GlobalCtx, component: &Component<'_>) -> Result<()> {
    let mut report = Report::new();
    if global_ctx.args.strip && component.wasm().is_none() {
        report.add_diagnostic(
            DiagnosticBuilder::new("no WebAssembly to strip", 0)
                .severity(Severity::Warning)
                .build(),
        );
    }
    if !global_ctx.args.build_args.is_empty() && component.wasm().is_none() {
        report.add_diagnostic(
            DiagnosticBuilder::new("no WebAssembly to compile - build args do nothing", 0)
                .severity(Severity::Warning)
                .build(),
        );
    }
    if global_ctx.args.optimize.is_some() && component.wasm().is_none() {
        report.add_diagnostic(
            DiagnosticBuilder::new("no WebAssembly to optimize", 0)
                .severity(Severity::Warning)
                .build(),
        );
    }
    if !report.is_empty() {
        let input_name = global_ctx.args.input.to_string_lossy();
        decorous_errors::fmt::report(&report, &input_name, "...")?;
    }

    Ok(())
}

fn render_all(
    global_ctx: &GlobalCtx,
    component: &Component<'_>,
    metadata: &Options<'_>,
) -> Result<()> {
    let js_name = if global_ctx.args.modularize {
        format!("{}.mjs", global_ctx.args.out)
    } else {
        format!("{}.js", global_ctx.args.out)
    };
    let mut out = BufWriter::new(File::create(&js_name).context("error creating out file")?);
    match global_ctx.args.render_method {
        RenderMethod::Csr => {
            dom_render(component, &mut out, metadata).context("error during rendering")?;
            if global_ctx.args.html {
                render_index_html(global_ctx, component, metadata, &js_name, None)?;
            }
        }
        RenderMethod::Prerender => {
            if global_ctx.args.html {
                let mut html_out = vec![];
                prerender(component, &mut out, &mut html_out, metadata)
                    .context("error during rendering")?;
                render_index_html(global_ctx, component, metadata, &js_name, unsafe {
                    Some(std::str::from_utf8_unchecked(&html_out))
                })?;
            } else {
                let html_name = format!("{}.html", global_ctx.args.out);
                let mut html_out = &mut BufWriter::new(
                    File::create(&html_name)
                        .with_context(|| format!("problem creating {}", html_name))?,
                );
                prerender(component, &mut out, &mut html_out, metadata)
                    .context("error during rendering")?;
                println!(
                    "{}",
                    FinishLog::default()
                        .with_main_message("HTML")
                        .with_sub_message("prerender")
                        .with_file(&html_name)
                        .enable_color(global_ctx.args.color)
                );
            }
        }
    }
    println!(
        "{}",
        FinishLog::default()
            .with_main_message("JavaScript")
            .with_sub_message(global_ctx.args.render_method.to_string())
            .enable_color(global_ctx.args.color)
            .with_file(js_name)
    );
    out.flush()
        .context("problem flushing buffered writer while rendering")?;

    Ok(())
}

fn render_index_html(
    global_ctx: &GlobalCtx,
    component: &Component,
    meta: &Options<'_>,
    js_name: &str,
    to_render: Option<&str>,
) -> Result<()> {
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(no_escape);
    handlebars.register_template_string("index", include_str!("./templates/template.html"))?;

    let out = File::create("index.html").context("problem creating index.html")?;
    let body = json!({
        "script": js_name,
        "css": component.css().is_some().then(|| format!("{}.css", global_ctx.args.out)),
        "name": meta.name,
        "html": to_render,
    });

    handlebars.render_template_to_write(include_str!("./templates/template.html"), &body, out)?;

    println!(
        "{}",
        FinishLog::default()
            .with_main_message("HTML")
            .with_file("index.html")
            .enable_color(global_ctx.args.color)
    );

    Ok(())
}

fn parse_component<'a>(input: &'a str, global_ctx: &GlobalCtx) -> Result<Component<'a>> {
    let file_name = global_ctx.args.input.to_string_lossy();
    let preproc = Preproc::new(global_ctx.config, global_ctx.args.color);
    let parser = Parser::new(input).with_ctx(Ctx {
        preprocessor: &preproc,
        errs: global_ctx.errs.clone(),
    });
    let component = match parser.parse() {
        Ok(ast) => {
            let c = Component::new(ast);
            // TODO: Use error stream, would make this useless
            if !c.report().is_empty() {
                decorous_errors::fmt::report(c.report(), &file_name, input)?;
            }
            Ok(c)
        }
        Err(err) => {
            let diagnostic = err.into();
            global_ctx.errs.emit(diagnostic);
            anyhow::bail!("\nthe decorous parser failed");
        }
    };
    println!(
        "{}",
        FinishLog::default()
            .with_main_message("parsed")
            .enable_color(global_ctx.args.color)
    );
    component
}
