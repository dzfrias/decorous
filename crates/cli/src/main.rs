use std::{
    fs::File,
    io::{self, Write},
    path::PathBuf,
};

use anyhow::{Context, Result};
use clap::Parser as ArgParser;
use clap_stdin::FileOrStdin;
use decorous_backend::render;
use decorous_frontend::{Component, Parser};

#[derive(Debug, ArgParser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// The decor file to compile.
    #[arg(value_name = "PATH")]
    input: FileOrStdin,

    /// The name of the output file to generate.
    #[arg(short, long, value_name = "PATH", default_value = "./out.js")]
    out: PathBuf,
    /// Write the compiled output to stdout.
    #[arg(long, conflicts_with_all = ["out", "html"])]
    stdout: bool,

    /// Generate an HTML file along with the compiled JS file. With no argument, it is index.html.
    #[arg(long, value_name = "PATH", num_args = 0..=1, require_equals = true, default_missing_value = "index.html", default_value = None)]
    html: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Cli::parse();

    let component = parse_component(&args.input)?;

    if args.stdout {
        // Write the file to stdout
        render(&component, &mut io::stdout()).context("problem rendering component to stdout")?;
    } else {
        let mut f = File::create(&args.out)?;
        render(&component, &mut f).context("problem rendering component to file")?;
    }

    if let Some(html) = args.html {
        let mut f = File::create(html)?;
        write!(f, include_str!("./template.html"), args.out.display())?;
    }

    Ok(())
}

fn parse_component(input: &str) -> Result<Component> {
    let parser = Parser::new(input);
    let (ast, errs) = parser.parse();
    let errors_len = errs.len();
    for err in errs {
        eprintln!("{err}");
    }
    if errors_len > 0 {
        anyhow::bail!("\ndecorous failed with {errors_len} errors");
    }
    Ok(Component::new(ast))
}
