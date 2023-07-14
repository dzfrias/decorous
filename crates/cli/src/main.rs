use std::{
    fs::File,
    io::{self, Write},
    path::PathBuf,
};

use anyhow::{Context, Result};
use clap::{Parser as ArgParser, ValueEnum};
use clap_stdin::FileOrStdin;
use decorous_backend::{dom_render::render as dom_render, prerender::Renderer as Prerenderer};
use decorous_frontend::{ast::Location, errors::Report, parse, Component};

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
    input: FileOrStdin,

    /// The name of the output file to generate.
    #[arg(short, long, value_name = "PATH", default_value = "./out.js")]
    out: PathBuf,
    #[arg(short, long, default_value = "prerender")]
    render_method: RenderMethod,
    /// Write the compiled output to stdout.
    #[arg(long, conflicts_with_all = ["out", "html"])]
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
}

fn main() -> Result<()> {
    let args = Cli::parse();
    println!("\x1b[1mparsing...\x1b[0m");
    let component = parse_component(&args.input)?;
    println!("\x1b[1;32mparsed!\x1b[0m");

    println!("\x1b[1mrendering...\x1b[0m");
    match args.render_method {
        RenderMethod::Prerender if args.stdout => {
            let renderer = Prerenderer::new(&component);
            renderer.render(&mut io::stdout(), &mut io::stdout())?;
            println!("\x1b[1;32mrendered\x1b[0m to stdout!");
        }
        RenderMethod::Prerender => {
            let html_out = args.html.unwrap_or("./out.html".into());
            let mut html = File::create(&html_out).with_context(|| {
                format!("problem creating {} for prerendering", html_out.display())
            })?;
            let mut js = File::create(&args.out).with_context(|| {
                format!("problem creating {} for prerendering", args.out.display())
            })?;

            let renderer = Prerenderer::new(&component);
            renderer.render(&mut js, &mut html)?;
            println!(
                "\x1b[1;32mrendered\x1b[0m to {} and {}!",
                html_out.display(),
                args.out.display()
            );
        }
        RenderMethod::Dom if args.stdout => {
            dom_render(&component, &mut io::stdout())
                .context("problem dom rendering component to stdout")?;

            if let Some(ref html) = args.html {
                let mut f = File::create(html)?;
                write!(f, include_str!("./template.html"), args.out.display())?;
            }
            println!("\x1b[1;32mrendered\x1b[0m to stdout!");
        }
        RenderMethod::Dom => {
            let mut f = File::create(&args.out).with_context(|| {
                format!("problem dom rendering component to {}", args.out.display())
            })?;
            dom_render(&component, &mut f).with_context(|| {
                format!("problem dom rendering component to {}", args.out.display())
            })?;

            if let Some(ref html) = args.html {
                let mut f = File::create(html)?;
                write!(f, include_str!("./template.html"), args.out.display())?;
            }
            println!("\x1b[1;32mrendered\x1b[0m to {}!", args.out.display());
        }
    }

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
    for err in report.errors() {
        let lines = input.lines().enumerate();
        // Minus one because location_line is 1-indexed
        let line_no = err.fragment().line() - 1;

        // Write the error description
        writeln!(out, "\n\x1b[1;31merror: {}\x1b[0m", err.err_type())?;
        if let Some(help_line) = err
            .help()
            .and_then(|help| help.corresponding_line())
            .filter(|ln| ln < &line_no)
        {
            let (_, line) = lines
                .clone()
                .skip_while(|(n, _)| *n as u32 != help_line - 1)
                .next()
                .expect("should be in lines");
            writeln!(out, "{help_line}| {} \x1b[1;33m<--- this line\x1b[0m", line,)?;
            if help_line + 1 != line_no {
                writeln!(out, "...")?;
            }
        }
        let (i, line) = lines
            .clone()
            .skip_while(|(n, _)| (*n as u32) + 1 < line_no)
            .next()
            .expect("line should be in input");

        writeln!(out, "{}| {line}", i + 1)?;
        // Plus one because line_no is 0 indexed, so we need to get the actual line number
        let line_no_len = count_digits(line_no + 1) as usize;
        let col = err.fragment().column() + line_no_len + 2;
        writeln!(out, "\x1b[1;33m{arrow:>col$}\x1b[0m", arrow = "^")?;

        if let Some(help) = err.help() {
            writeln!(out, "\x1b[1mhelp: {help}\x1b[0m")?;
        }
        writeln!(out)?;
    }
    Ok(())
}

fn count_digits(num: u32) -> u32 {
    num.checked_ilog10().unwrap_or(0) + 1
}
