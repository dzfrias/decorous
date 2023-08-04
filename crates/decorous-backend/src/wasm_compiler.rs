use std::io;

use crate::RenderBackend;
use rslint_parser::SmolStr;

#[derive(Debug, Clone, Hash)]
pub struct CodeInfo<'a> {
    pub lang: &'a str,
    pub body: &'a str,
    pub exports: &'a [SmolStr],
}
/// The trait for anything that takes WebAssembly input and compiles it to JavaScript.
///
/// After implementing it on a type, use the [`render_with_wasm`](super::render_with_wasm)
/// function to hook the compiler into the renderer.
///
/// It is generic over a [`RenderBackend`], which allows for different written code depending on
/// the backend.
pub trait WasmCompiler<B>
where
    B: RenderBackend,
{
    type Err;

    fn compile<W>(&mut self, info: CodeInfo, out: &mut W) -> Result<(), Self::Err>
    where
        W: io::Write;
}
