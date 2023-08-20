use std::fs;

use anyhow::{Context, Result};
use indicatif::HumanBytes;

use crate::{cli::Cache, utils};

pub fn cache(args: &Cache) -> Result<()> {
    let loc = utils::get_cache_base().context("could not get cache base")?;
    if !loc.exists() {
        anyhow::bail!("cache does not exist yet!");
    }
    let size = utils::dir_size(&loc).context("error getting size of dir")?;
    if args.clean {
        fs::remove_dir_all(&loc).context("problem removing cache")?;
        fs::create_dir(&loc).context("problem re-creating cache dir after clean")?;
        println!("Cleaned cache! {} bytes saved!", HumanBytes(size));
        return Ok(());
    }
    let items = fs::read_dir(&loc)
        .context("error reading directory")?
        .count();
    println!(
        "decorous cache info\n\nlocation: {}\nsize: {}\nnumber of entries: {items}",
        loc.display(),
        HumanBytes(size),
    );
    Ok(())
}
