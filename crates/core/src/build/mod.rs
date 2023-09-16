mod compile_wasm;
mod global_ctx;
mod preprocessor;
mod resolver;

use std::{
    borrow::Cow,
    fs::{self, File},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{ensure, Context, Result};
use decorous_backend::{
    dom_render::{CsrOptions, CsrRenderer},
    prerender::Prerenderer,
    Ctx as RenderCtx, HtmlInfo, RenderBackend, RenderOut,
};
use decorous_errors::{DiagnosticBuilder, DynErrStream, Severity, Source};
use decorous_frontend::{Component, Ctx as ParseCtx, Parser};
use notify::{
    event::{DataChange, ModifyKind},
    EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};

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
    let compiler = MainCompiler::new(&global_ctx);
    let metadata = RenderCtx {
        name: {
            &args
                .input
                .file_stem()
                .expect("file name should never be .. or /, if read was successful")
                .to_string_lossy()
        },
        index_html: if global_ctx.args.html {
            Some(HtmlInfo {
                basename: global_ctx.args.out.clone(),
            })
        } else {
            None
        },
        wasm_compiler: &compiler,
        use_resolver: &Resolver {
            global_ctx: &global_ctx,
            compiler: &compiler,
        },
        errs: global_ctx.errs.clone(),
    };

    let component = parse_component(&input, &global_ctx)?;
    warn_on_unused_wasm(&global_ctx, &component)?;
    render_all(&global_ctx, &component, &metadata)?;

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

fn warn_on_unused_wasm(global_ctx: &GlobalCtx, component: &Component<'_>) -> Result<()> {
    if global_ctx.args.strip && component.wasm().is_none() {
        global_ctx.errs.emit(
            DiagnosticBuilder::new("no WebAssembly to strip", 0)
                .severity(Severity::Warning)
                .build(),
        );
    }
    if !global_ctx.args.build_args.is_empty() && component.wasm().is_none() {
        global_ctx.errs.emit(
            DiagnosticBuilder::new("no WebAssembly to compile - build args do nothing", 0)
                .severity(Severity::Warning)
                .build(),
        );
    }
    if global_ctx.args.optimize.is_some() && component.wasm().is_none() {
        global_ctx.errs.emit(
            DiagnosticBuilder::new("no WebAssembly to optimize", 0)
                .severity(Severity::Warning)
                .build(),
        );
    }

    Ok(())
}

fn render_all(
    global_ctx: &GlobalCtx,
    component: &Component<'_>,
    metadata: &RenderCtx<'_>,
) -> Result<()> {
    let js_name = if global_ctx.args.modularize {
        format!("{}.mjs", global_ctx.args.out)
    } else {
        format!("{}.js", global_ctx.args.out)
    };

    pub struct Out<'a> {
        js: BufWriter<File>,
        html: Option<BufWriter<File>>,
        css: Option<BufWriter<File>>,
        base: &'a str,
        index_html: bool,
    }

    impl RenderOut for Out<'_> {
        fn write_js(&mut self, buf: &[u8]) -> io::Result<()> {
            self.js.write_all(buf)
        }

        fn write_css(&mut self, buf: &[u8]) -> io::Result<()> {
            match &mut self.css {
                Some(css) => css.write_all(buf),
                None => {
                    let f = File::create(format!("{}.css", self.base))?;
                    self.css = Some(BufWriter::new(f));
                    self.css.as_mut().unwrap().write_all(buf)
                }
            }
        }

        fn write_html(&mut self, buf: &[u8]) -> io::Result<()> {
            match &mut self.html {
                Some(html) => html.write_all(buf),
                None => {
                    let f = if self.index_html {
                        File::create("index.html")?
                    } else {
                        File::create(format!("{}.html", self.base))?
                    };
                    self.html = Some(BufWriter::new(f));
                    self.html.as_mut().unwrap().write_all(buf)
                }
            }
        }

        fn js_handle(&mut self) -> &mut dyn io::Write {
            &mut self.js
        }
    }

    let mut out = Out {
        js: BufWriter::new(File::create(&js_name)?),
        html: None,
        css: None,
        base: &global_ctx.args.out,
        index_html: global_ctx.args.html,
    };
    match global_ctx.args.render_method {
        RenderMethod::Csr => {
            let mut csr_renderer = CsrRenderer::new();
            csr_renderer.with_options(CsrOptions {
                modularize: global_ctx.args.modularize,
            });
            csr_renderer.render(component, &mut out, metadata)?;
        }
        RenderMethod::Prerender => {
            let prerenderer = Prerenderer::new();
            prerenderer.render(component, &mut out, metadata)?;
        }
    }

    if out.html.is_some() {
        let html_name = if global_ctx.args.html {
            Cow::Borrowed(Path::new("index.html"))
        } else {
            Cow::Owned(PathBuf::from(format!("{}.html", global_ctx.args.out)))
        };
        println!(
            "{}",
            FinishLog::default()
                .with_main_message("HTML")
                .with_file(&html_name)
                .enable_color(global_ctx.args.color)
        );
    }

    println!(
        "{}",
        FinishLog::default()
            .with_main_message("JavaScript")
            .with_sub_message(global_ctx.args.render_method.to_string())
            .enable_color(global_ctx.args.color)
            .with_file(js_name)
    );

    if let Some(mut html) = out.html {
        html.flush()?;
    }
    if let Some(mut css) = out.css {
        css.flush()?;
    }
    out.js.flush()?;

    Ok(())
}

fn parse_component<'a>(input: &'a str, global_ctx: &GlobalCtx<'a>) -> Result<Component<'a>> {
    let preproc = Preproc::new(global_ctx.config, global_ctx.args.color);
    let parser = Parser::new(input).with_ctx(ParseCtx {
        preprocessor: &preproc,
        errs: global_ctx.errs.clone(),
    });
    let component = match parser.parse() {
        Ok(ast) => Component::new(ast, global_ctx.errs.clone()),
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
    Ok(component)
}
