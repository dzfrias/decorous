pub(crate) mod codegen_utils;
pub mod css_render;
pub mod dom_render;
pub mod prerender;
pub mod render_error;
mod use_resolver;
mod wasm_compiler;

use decorous_errors::{DynErrStream, Source};
pub use use_resolver::*;
pub use wasm_compiler::*;

pub use crate::wasm_compiler::CodeInfo;

pub struct Options<'a> {
    pub name: &'a str,
    pub modularize: bool,
    pub wasm_compiler: &'a dyn WasmCompiler,
    pub use_resolver: &'a dyn UseResolver,
    pub errs: DynErrStream<'a>,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Self {
            name: "test",
            modularize: false,
            wasm_compiler: &NullCompiler,
            use_resolver: &NullResolver,
            errs: decorous_errors::stderr(Source {
                src: "",
                name: "OPTIONS".to_owned(),
            }),
        }
    }
}
