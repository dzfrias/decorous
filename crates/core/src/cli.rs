use std::path::PathBuf;

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
}
