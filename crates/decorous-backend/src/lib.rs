pub(crate) mod codegen_utils;
pub mod css_render;
pub mod dom_render;
pub mod prerender;
mod use_resolver;
mod wasm_compiler;

use decorous_frontend::Component;
use std::fmt::Debug;
use std::io;
pub use use_resolver::*;
pub use wasm_compiler::*;

pub use crate::wasm_compiler::CodeInfo;

#[derive(Debug)]
pub struct Options<'name, C, R> {
    pub name: &'name str,
    pub modularize: bool,
    pub wasm_compiler: C,
    pub use_resolver: R,
}

pub trait RenderBackend {
    fn render<T: io::Write, C, R>(
        out: &mut T,
        component: &Component,
        metadata: &Options<'_, C, R>,
    ) -> io::Result<()>
    where
        C: WasmCompiler<Self>,
        R: UseResolver;
}

pub fn render<B, T, C, R>(
    component: &Component,
    out: &mut T,
    metadata: &Options<'_, C, R>,
) -> io::Result<()>
where
    T: io::Write,
    B: RenderBackend,
    C: WasmCompiler<B>,
    R: UseResolver,
{
    <B as RenderBackend>::render(out, component, metadata)
}
