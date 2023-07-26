use std::{
    fs::{self, File},
    io::{self, BufWriter, Stdout, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::{Parser as ArgParser, ValueEnum};
use decorous_backend::{
    css_render::CssRenderer,
    dom_render::DomRenderer,
    prerender::{HtmlPrerenderer, Prerenderer},
    render, Metadata, RenderBackend,
};
use decorous_frontend::{errors::Report, location::Location, parse, Component};
use superfmt::{
    style::{Color, Modifiers, Style},
    Formatter,
};

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "kebab-case")]
enum RenderMethod {
    Dom,
    Prerender,
}

#[derive(Debug, ArgParser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// The decor file to compile.
    #[arg(value_name = "PATH")]
    input: PathBuf,

    /// The name of the output file to generate.
    #[arg(short, long, value_name = "PATH", default_value = "out.js")]
    out: PathBuf,
    #[arg(short, long, default_value = "prerender")]
    render_method: RenderMethod,
    /// Write the compiled output to stdout.
    #[arg(long, conflicts_with_all = ["out", "html", "css"])]
    stdout: bool,

    /// The name of the HTML file to generate
    #[arg(long,
          value_name = "PATH",
          num_args = 0..=1,
          require_equals = true,
          default_missing_value = "index.html",
          default_value = None,
          default_value_if("render_method", "prerender", Some("out.html")),
    )]
    html: Option<PathBuf>,
    /// The name of the css file to generate
    #[arg(long, value_name = "PATH", default_value = "out.css")]
    css: PathBuf,
}

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
    match args.render_method {
        RenderMethod::Prerender if args.stdout => {
            let mut stdout = BufWriter::new(io::stdout());
            buf_render::<Prerenderer, _>(&component, &mut stdout, &mut formatter, &metadata)?;
        }
        RenderMethod::Prerender => {
            let html_out = args.html.unwrap_or("./out.html".into());
            let mut html = BufWriter::new(File::create(&html_out).with_context(|| {
                format!("problem creating {} for prerendering", html_out.display())
            })?);
            buf_render::<HtmlPrerenderer, _>(&component, &mut html, &mut formatter, &metadata)?;
            let mut js = BufWriter::new(File::create(&args.out).with_context(|| {
                format!("problem creating {} for prerendering", args.out.display())
            })?);
            buf_render::<Prerenderer, _>(&component, &mut js, &mut formatter, &metadata)?;
        }
        RenderMethod::Dom if args.stdout => {
            let mut stdout = BufWriter::new(io::stdout());
            buf_render::<DomRenderer, _>(&component, &mut stdout, &mut formatter, &metadata)?;

            if let Some(html) = &args.html {
                write_html(
                    &args.out,
                    html,
                    component.css().is_some().then_some(&args.css),
                    &metadata,
                )?;
            }
        }
        RenderMethod::Dom => {
            let mut f = BufWriter::new(
                File::create(&args.out)
                    .with_context(|| format!("problem creating {}", args.out.display()))?,
            );
            buf_render::<DomRenderer, _>(&component, &mut f, &mut formatter, &metadata)?;

            if let Some(html) = &args.html {
                write_html(
                    &args.out,
                    html,
                    component.css().is_some().then_some(&args.css),
                    &metadata,
                )?;
            }
        }
    }

    if component.css().is_some() {
        if !args.stdout {
            let mut f = BufWriter::new(
                File::create(args.css)
                    .with_context(|| format!("problem creating {}", args.out.display()))?,
            );
            buf_render::<CssRenderer, _>(&component, &mut f, &mut formatter, &metadata)?;
        } else {
            let mut stdout = BufWriter::new(io::stdout());
            buf_render::<CssRenderer, _>(&component, &mut stdout, &mut formatter, &metadata)?;
        }
    }

    Ok(())
}

fn write_html(
    script: impl AsRef<Path>,
    html: impl AsRef<Path>,
    css: Option<impl AsRef<Path>>,
    metadata: &Metadata,
) -> Result<()> {
    let mut f = File::create(html)?;
    if let Some(css) = css {
        write!(
            f,
            include_str!("./template_css.html"),
            name = metadata.name,
            css = css.as_ref().display(),
            script = script.as_ref().display()
        )
    } else {
        write!(
            f,
            include_str!("./template.html"),
            script.as_ref().display(),
            name = metadata.name,
        )
    }
    .with_context(|| format!("problem writing html to {}", script.as_ref().display()))
}

fn buf_render<T: RenderBackend, W: Write>(
    component: &Component<'_>,
    writer: &mut BufWriter<W>,
    formatter: &mut Formatter<'_, Stdout>,
    metadata: &Metadata,
) -> Result<()> {
    render::<T, _>(component, writer, metadata).context("problem rendering")?;
    writer
        .flush()
        .context("problem flushing buffered writer while rendering")?;
    formatter.write_with_context("rendered!\n", Color::Green)?;
    Ok(())
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

fn fmt_report<T: io::Write>(input: &str, report: &Report<Location>, out: &mut T) -> io::Result<()> {
    let mut formatter = Formatter::new(out);
    for err in report.errors() {
        let lines = input.lines().enumerate();
        // Minus one because location_line is 1-indexed
        let line_no = err.fragment().line() - 1;

        formatter.writeln_with_context(format_args!("error: {}", err.err_type()), Color::Red)?;
        // Write the error description
        if let Some(help_line) = err
            .help()
            .and_then(|help| help.corresponding_line())
            .filter(|ln| ln < &line_no)
        {
            let (_, line) = lines
                .clone()
                .find(|(n, _)| *n as u32 == help_line - 1)
                .expect("should be in lines");
            write!(formatter, "{help_line}| {} ", line)?;
            formatter.writeln_with_context("<--- this line", Color::Yellow)?;
            if help_line + 1 != line_no {
                formatter.writeln("...")?;
            }
        }
        let (i, line) = lines
            .clone()
            .find(|(n, _)| (*n as u32) + 1 == line_no)
            .expect("line should be in input");

        writeln!(formatter, "{}| {line}", i + 1)?;
        // Plus one because line_no is 0 indexed, so we need to get the actual line number
        let line_no_len = count_digits(line_no + 1) as usize;
        let col = err.fragment().column() + line_no_len + 2;
        formatter.writeln_with_context(
            format_args!("{arrow:>col$}", arrow = "^"),
            Style::default()
                .fg(Color::Yellow)
                .modifiers(Modifiers::BOLD),
        )?;

        if let Some(help) = err.help() {
            formatter.writeln_with_context(format_args!("help: {help}"), Modifiers::BOLD)?;
        }
        writeln!(formatter)?;
    }
    Ok(())
}

fn count_digits(num: u32) -> u32 {
    num.checked_ilog10().unwrap_or(0) + 1
}
