use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct UseInfo {
    pub loc: PathBuf,
}

pub trait UseResolver {
    fn resolve(&self, path: impl AsRef<Path>) -> io::Result<UseInfo>;
}

pub struct NullResolver;

impl UseResolver for NullResolver {
    fn resolve(&self, path: impl AsRef<Path>) -> io::Result<UseInfo> {
        Ok(UseInfo {
            loc: path.as_ref().to_path_buf(),
        })
    }
}

impl<T> UseResolver for &T
where
    T: UseResolver,
{
    fn resolve(&self, path: impl AsRef<Path>) -> io::Result<UseInfo> {
        (*self).resolve(path)
    }
}
