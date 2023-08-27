use std::io::Write;

use anyhow::Error;
use rslint_parser::SmolStr;

#[derive(Debug, Clone, Hash)]
pub struct CodeInfo<'a> {
    pub lang: &'a str,
    pub body: &'a str,
    pub exports: &'a [SmolStr],
}

/// The trait for anything that takes WebAssembly input and compiles it to JavaScript.
pub trait WasmCompiler {
    fn compile(&self, info: CodeInfo, out: &mut dyn Write) -> Result<(), Error>;
}

pub struct NullCompiler;

impl WasmCompiler for NullCompiler {
    fn compile(&self, _info: CodeInfo, _out: &mut dyn Write) -> Result<(), Error> {
        Ok(())
    }
}

impl<T> WasmCompiler for &T
where
    T: WasmCompiler,
{
    fn compile(&self, info: CodeInfo, out: &mut dyn Write) -> Result<(), Error> {
        (*self).compile(info, out)
    }
}
