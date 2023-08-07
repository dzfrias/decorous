pub(crate) mod codegen_utils;
pub mod css_render;
pub mod dom_render;
pub mod prerender;
mod wasm_compiler;

use decorous_frontend::Component;
use std::fmt::Debug;
use std::io;
use thiserror::Error;
pub use wasm_compiler::WasmCompiler;

pub use crate::wasm_compiler::CodeInfo;

#[derive(Debug)]
pub struct Metadata<'name> {
    pub name: &'name str,
    pub modularize: bool,
}

pub trait RenderBackend {
    fn render<T: io::Write>(
        out: &mut T,
        component: &Component,
        metadata: &Metadata,
    ) -> io::Result<()>;
}

pub fn render<B, T>(component: &Component, out: &mut T, metadata: &Metadata) -> io::Result<()>
where
    T: io::Write,
    B: RenderBackend,
{
    <B as RenderBackend>::render(out, component, metadata)
}

#[derive(Debug, Error)]
pub enum WasmRenderError<T> {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("wasm error: {0}")]
    Wasm(T),
}

pub fn render_with_wasm<B, W, T>(
    component: &Component,
    out: &mut W,
    metadata: &Metadata,
    compiler: &mut T,
) -> Result<(), WasmRenderError<T::Err>>
where
    T: WasmCompiler<B>,
    B: RenderBackend,
    W: io::Write,
{
    if let Some(wasm) = component.wasm() {
        compiler
            .compile(
                CodeInfo {
                    lang: wasm.lang(),
                    body: wasm.body(),
                    exports: component.exports(),
                },
                out,
            )
            .map_err(WasmRenderError::Wasm)?;
    }
    <B as RenderBackend>::render(out, component, metadata)?;

    Ok(())
}
