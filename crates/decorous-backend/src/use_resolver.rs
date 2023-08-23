use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct UseInfo {
    pub loc: PathBuf,
}

pub trait UseResolver {
    fn resolve<W: io::Write>(&self, out: &mut W, path: impl AsRef<Path>) -> io::Result<UseInfo>;
}

pub struct NullResolver;

impl UseResolver for NullResolver {
    fn resolve<W: io::Write>(&self, _out: &mut W, path: impl AsRef<Path>) -> io::Result<UseInfo> {
        Ok(UseInfo {
            loc: path.as_ref().to_path_buf(),
        })
    }
}
