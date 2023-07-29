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

    /// The name of the HTML file to generate
    #[arg(long,
          value_name = "PATH",
          num_args = 0..=1,
          require_equals = true,
          default_missing_value = "index.html",
          default_value = None,
          default_value_if("render_method", "prerender", Some("out.html")),
    )]
    pub html: Option<PathBuf>,
    /// The name of the css file to generate
    #[arg(long, value_name = "PATH", default_value = "out.css")]
    pub css: PathBuf,
}
