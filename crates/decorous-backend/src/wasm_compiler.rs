use std::io;

use crate::RenderBackend;

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

    fn compile<W>(&mut self, lang: &str, body: &str, out: &mut W) -> Result<(), Self::Err>
    where
        W: io::Write;
}
