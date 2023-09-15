use std::io;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, RenderError>;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("WebAssembly error error: {0}")]
    WebAssembly(#[from] anyhow::Error),
}
