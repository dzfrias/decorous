use crate::Result;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct UseInfo {
    pub loc: PathBuf,
}

pub trait UseResolver {
    fn resolve(&self, path: &Path) -> Result<UseInfo>;
}

pub struct NullResolver;

impl UseResolver for NullResolver {
    fn resolve(&self, path: &Path) -> Result<UseInfo> {
        Ok(UseInfo {
            loc: path.to_path_buf(),
        })
    }
}

impl<T> UseResolver for &T
where
    T: UseResolver,
{
    fn resolve(&self, path: &Path) -> Result<UseInfo> {
        (*self).resolve(path)
    }
}
