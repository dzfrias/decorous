use std::{ffi::OsStr, fs::File, io, path::PathBuf};

use anyhow::Result;
use clap::Parser as ArgParser;
use clap_stdin::FileOrStdin;
use decorous_backend::render;
use decorous_frontend::{Component, Parser};

#[derive(Debug, ArgParser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    input: FileOrStdin,

    /// The name of the output file to generate, or "-" to output to stdout
    #[arg(short, long, default_value = "./out.js")]
    out: PathBuf,
}

fn main() -> Result<()> {
    let args = Cli::parse();
    let parser = Parser::new(&args.input);
    let (ast, errs) = parser.parse();
    let errors_len = errs.len();
    for err in errs {
        eprintln!("{err}");
    }
    if errors_len > 0 {
        anyhow::bail!("\ndecorous failed with {errors_len} errors");
    }
    let component = Component::new(ast);
    if args.out.as_os_str() == OsStr::new("-") {
        render(&component, &mut io::stdout())?;
    } else {
        let mut f = File::create(args.out)?;
        render(&component, &mut f)?;
    }
    Ok(())
}
