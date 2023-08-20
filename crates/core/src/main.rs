mod build;
mod cache;
mod cli;
mod config;
mod indicators;
mod utils;

use anyhow::Result;
use clap::Parser;
use cli::Cli;

use crate::cli::Command;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() -> Result<()> {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    let args = Cli::parse();

    match args.command {
        Command::Build(args) => {
            build::build(&args)?;
        }
        Command::Cache(args) => {
            cache::cache(&args)?;
        }
    }

    #[cfg(feature = "dhat-heap")]
    println!();

    Ok(())
}
