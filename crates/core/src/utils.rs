use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub fn get_cache_base() -> Option<PathBuf> {
    #[cfg(not(target_os = "macos"))]
    let base = dirs_next::cache_dir()?.join("decorous");
    #[cfg(target_os = "macos")]
    let base = dirs_next::home_dir()?.join(".cache/decorous");

    Some(base)
}

// Taken from https://docs.rs/fs_extra/latest/fs_extra/dir/fn.get_size.html
pub fn dir_size<P>(path: P) -> io::Result<u64>
where
    P: AsRef<Path>,
{
    let path_metadata = path.as_ref().symlink_metadata()?;

    let mut size_in_bytes = 0;

    if path_metadata.is_dir() {
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            let entry_metadata = entry.metadata()?;

            if entry_metadata.is_dir() {
                size_in_bytes += dir_size(entry.path())?;
            } else {
                size_in_bytes += entry_metadata.len();
            }
        }
    } else {
        size_in_bytes = path_metadata.len();
    }

    Ok(size_in_bytes)
}
