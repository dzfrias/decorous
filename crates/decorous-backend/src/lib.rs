pub(crate) mod codegen_utils;
pub mod css_render;
pub mod dom_render;
pub mod prerender;
mod render_out;
mod use_resolver;
mod wasm_compiler;

use std::io;

use decorous_errors::{DynErrStream, Source};
use decorous_frontend::Component;
pub use render_out::{JsFile, RenderOut};
pub use use_resolver::*;
pub use wasm_compiler::*;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, RenderError>;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("error: {0}")]
    Other(#[from] anyhow::Error),
}

pub trait RenderBackend {
    type Options;

    fn with_options(&mut self, options: Self::Options);
    fn render<T: RenderOut>(&self, component: &Component, out: T, ctx: &Ctx) -> Result<()>;
}

#[derive(Debug)]
pub struct HtmlInfo {
    pub basename: String,
}

pub struct Ctx<'a> {
    pub name: &'a str,
    pub index_html: Option<HtmlInfo>,
    pub wasm_compiler: &'a dyn WasmCompiler,
    pub use_resolver: &'a dyn UseResolver,
    pub errs: DynErrStream<'a>,
}

impl Default for Ctx<'_> {
    fn default() -> Self {
        Self {
            name: "test",
            index_html: None,
            wasm_compiler: &NullCompiler,
            use_resolver: &NullResolver,
            errs: decorous_errors::stderr(Source {
                src: "",
                name: "OPTIONS".to_owned(),
            }),
        }
    }
}
