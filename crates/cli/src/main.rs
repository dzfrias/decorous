use std::io::Write;
use std::{fs::File, io, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser as ArgParser, ValueEnum};
use clap_stdin::FileOrStdin;
use decorous_backend::dom_render::render as dom_render;
use decorous_backend::prerender::Renderer as Prerenderer;
use decorous_frontend::{parse, Component};

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
    let component = parse_component(&args.input)?;

    match args.render_method {
        RenderMethod::Prerender if args.stdout => {
            let renderer = Prerenderer::new(&component);
            renderer.render(&mut io::stdout(), &mut io::stdout())?;
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
        }
        RenderMethod::Dom if args.stdout => {
            dom_render(&component, &mut io::stdout())
                .context("problem dom rendering component to stdout")?;

            if let Some(ref html) = args.html {
                let mut f = File::create(html)?;
                write!(f, include_str!("./template.html"), args.out.display())?;
            }
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
        }
    }

    Ok(())
}

fn parse_component(input: &str) -> Result<Component> {
    match parse(input) {
        Ok(ast) => Ok(Component::new(ast)),
        Err(report) => {
            report.format(input, &mut io::stderr())?;
            anyhow::bail!("\nthe decorous parser failed");
        }
    }
}
