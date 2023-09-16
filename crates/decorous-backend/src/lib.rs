pub(crate) mod codegen_utils;
pub mod css_render;
pub mod dom_render;
pub mod prerender;
mod render_out;
mod use_resolver;
mod wasm_compiler;

use std::{collections::HashMap, io};

use decorous_errors::{DynErrStream, Source};
use decorous_frontend::Component;
pub use render_out::{JsFile, RenderOut};
use rslint_parser::SmolStr;
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

#[derive(Debug, Default)]
pub struct Linker {
    defns: HashMap<SmolStr, String>,
}

impl Linker {
    pub fn define(&mut self, name: SmolStr, out_path: String) {
        self.defns.insert(name, out_path);
    }

    pub fn resolve(&self, name: &SmolStr) -> Option<&String> {
        self.defns.get(name)
    }

    pub fn definitions(&self) -> &HashMap<SmolStr, String> {
        &self.defns
    }
}

pub trait RenderBackend {
    fn add_linker(&mut self, linker: Linker);
    fn render<T: RenderOut>(&self, component: &Component, out: T, ctx: &Options) -> Result<()>;
}

#[derive(Debug)]
pub struct HtmlInfo {
    pub basename: String,
}

pub struct Options<'a> {
    pub name: &'a str,
    pub modularize: bool,
    pub index_html: Option<HtmlInfo>,
    pub wasm_compiler: &'a dyn WasmCompiler,
    pub use_resolver: &'a dyn UseResolver,
    pub errs: DynErrStream<'a>,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Self {
            name: "test",
            modularize: false,
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
