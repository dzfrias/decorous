use std::{fmt::Display, path::PathBuf, time::Duration};

use clap::{builder::ArgPredicate, Args, Parser, Subcommand, ValueEnum};
use humantime::parse_duration;

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Build a decorous file.
    Build(Build),
    /// Interact with the decorous cache. Run with no args to print information.
    Cache(Cache),
}

#[derive(Debug, Args)]
pub struct Build {
    /// The decor file to compile.
    #[arg(value_name = "PATH")]
    pub input: PathBuf,

    /// The base name of the output file(s) to generate.
    #[arg(short, long, value_name = "NAME", default_value = "out")]
    pub out: String,
    #[arg(
        short,
        long,
        default_value = "prerender",
        default_value_if("modularize", ArgPredicate::IsPresent, "csr")
    )]
    pub render_method: RenderMethod,

    #[arg(short = 'O', default_value = None)]
    pub optimize: Option<OptimizationLevel>,
    /// Strip custom sections from the WebAssembly file.
    #[arg(long)]
    pub strip: bool,

    /// Generate a full index.html file instead of just a fragment (or none at all).
    #[arg(long)]
    pub html: bool,
    /// Generate an ES6 compliant module for the output.
    #[arg(short, long)]
    pub modularize: bool,
    /// Pass build argument(s) the detected WASM compiler.
    #[arg(short = 'B', long, value_delimiter = ' ', value_name = "ARGS")]
    pub build_args: Vec<String>,

    /// Watch the input file for changes, recompiling if found.
    #[arg(short, long)]
    pub watch: bool,
    /// Control output colorization.
    #[arg(short, long, default_value = "auto", value_name = "WHEN")]
    pub color: Color,
}

#[derive(Debug, Args)]
pub struct Cache {
    /// Clean the cache.
    #[arg(short = 'x', long)]
    pub clean: bool,
    /// Evict cache entries that are older than the given time.
    #[arg(long,
          value_name = "TIME",
          value_parser = parse_duration,
          conflicts_with = "clean"
    )]
    pub evict: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
#[clap(rename_all = "kebab-case")]
pub enum Color {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum RenderMethod {
    Csr,
    Prerender,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OptimizationLevel {
    #[clap(name = "1")]
    SpeedMinor,
    #[clap(name = "2")]
    SpeedMedium,
    #[clap(name = "3")]
    SpeedMajor,
    #[clap(name = "4")]
    SpeedAggressive,
    #[clap(name = "s")]
    Size,
    #[clap(name = "z")]
    SizeAggressive,
}

impl Display for RenderMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Csr => write!(f, "csr"),
            Self::Prerender => write!(f, "prerender"),
        }
    }
}

impl Display for OptimizationLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OptimizationLevel::SpeedMinor => write!(f, "O1"),
            OptimizationLevel::SpeedMedium => write!(f, "O2"),
            OptimizationLevel::SpeedMajor => write!(f, "O3"),
            OptimizationLevel::SpeedAggressive => write!(f, "O4"),
            OptimizationLevel::Size => write!(f, "Os"),
            OptimizationLevel::SizeAggressive => write!(f, "Oz"),
        }
    }
}
