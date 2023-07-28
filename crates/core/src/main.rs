mod cli;
mod fmt_report;

use std::{
    fs::{self, File},
    io::{self, BufWriter, Write},
};

use anyhow::{Context, Result};
use clap::Parser;
use cli::Cli;
use cli::RenderMethod;
use decorous_backend::{
    css_render::CssRenderer,
    dom_render::DomRenderer,
    prerender::{HtmlPrerenderer, Prerenderer},
    render, Metadata,
};
use decorous_frontend::{parse, Component};
use fmt_report::fmt_report;
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
        BufWriter::new(File::create(&args.out)?),
        &component,
        args.render_method,
        &metadata,
    )?;

    'html: {
        match args.render_method {
            RenderMethod::Dom => {
                let Some(html) = args.html else {
                    break 'html;
                };
                let mut out = File::create(&html)
                    .with_context(|| format!("problem creating {}", html.display()))?;

                if component.css().is_some() {
                    write!(
                        out,
                        include_str!("./template_css.html"),
                        script = args.out.display(),
                        css = args.css.display(),
                        name = metadata.name
                    )
                    .context("problem writing dom css html template")?;
                    break 'html;
                }

                write!(
                    out,
                    include_str!("./template.html"),
                    args.out.display(),
                    name = metadata.name
                )
                .context("problem writing dom html template")?;
            }
            RenderMethod::Prerender => {
                let html = args.html.unwrap_or("out.html".into());

                render::<HtmlPrerenderer, _>(
                    &component,
                    &mut BufWriter::new(
                        File::create(&html)
                            .with_context(|| format!("problem creating {}", html.display()))?,
                    ),
                    &metadata,
                )?;
            }
        }
    }

    if component.css().is_some() {
        render::<CssRenderer, _>(
            &component,
            &mut BufWriter::new(
                File::create(&args.css)
                    .with_context(|| format!("problem creating {}", args.css.display()))?,
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

fn parse_component(input: &str) -> Result<Component> {
    match parse(input) {
        Ok(ast) => Ok(Component::new(ast)),
        Err(report) => {
            fmt_report(input, &report, &mut io::stderr())?;
            anyhow::bail!("\nthe decorous parser failed");
        }
    }
}
