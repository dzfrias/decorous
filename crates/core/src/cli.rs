use std::{fmt::Display, path::PathBuf};

use anyhow::Context;
use clap::{Parser as ArgParser, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum RenderMethod {
    Dom,
    Prerender,
}

#[derive(Debug, ArgParser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// The decor file to compile.
    #[arg(value_name = "PATH")]
    pub input: PathBuf,

    /// The base name of the output file(s) to generate.
    #[arg(short, long, value_name = "NAME", default_value = "out")]
    pub out: String,
    #[arg(short, long, default_value = "prerender")]
    pub render_method: RenderMethod,

    /// Generate a full index.html file instead of just a fragment (or none at all).
    #[arg(long)]
    pub html: bool,

    /// Pass a build argument(s) to a given WASM compiler.
    #[arg(
        short = 'B',
        long,
        value_name = "LANG>=<ARGS", // HACK
        value_parser = parse_key_val,
        number_of_values = 1,
    )]
    pub build_args: Vec<(String, String)>,
}

/// Parse a single key-value pair
fn parse_key_val(s: &str) -> Result<(String, String), anyhow::Error> {
    let pos = s
        .find('=')
        .with_context(|| format!("invalid LANG=ARGS: no `=` found in `{s}`"))?;
    Ok((s[..pos].to_owned(), s[pos + 1..].to_owned()))
}

impl Display for RenderMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dom => write!(f, "dom"),
            Self::Prerender => write!(f, "prerender"),
        }
    }
}
