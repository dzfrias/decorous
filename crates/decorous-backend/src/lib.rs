pub(crate) mod codegen_utils;
pub mod css_render;
pub mod dom_render;
pub mod prerender;
mod use_resolver;
mod wasm_compiler;

use decorous_frontend::Component;
use std::io;
pub use use_resolver::*;
pub use wasm_compiler::*;

pub use crate::wasm_compiler::CodeInfo;

pub struct Options<'a> {
    pub name: &'a str,
    pub modularize: bool,
    pub wasm_compiler: &'a dyn WasmCompiler,
    pub use_resolver: &'a dyn UseResolver,
}

pub trait RenderBackend {
    fn render<T: io::Write>(
        out: &mut T,
        component: &Component,
        metadata: &Options<'_>,
    ) -> io::Result<()>;
}

pub fn render<B, T>(component: &Component, out: &mut T, metadata: &Options<'_>) -> io::Result<()>
where
    T: io::Write,
    B: RenderBackend,
{
    <B as RenderBackend>::render(out, component, metadata)
}
