use anyhow::Error;
use rslint_parser::SmolStr;

#[derive(Debug, Clone, Hash)]
pub struct CodeInfo<'a> {
    pub lang: &'a str,
    pub body: &'a str,
    pub exports: &'a [SmolStr],
}

#[derive(Debug, Default, Clone)]
pub struct JsEnv(Vec<JsDecl>);

#[derive(Debug, Clone)]
pub struct JsDecl {
    pub name: String,
    pub value: String,
}

/// The trait for anything that takes WebAssembly input and compiles it to JavaScript.
pub trait WasmCompiler {
    fn compile(&self, info: CodeInfo) -> Result<String, Error>;
    fn compile_comptime(&self, info: CodeInfo) -> Result<JsEnv, Error>;
}

pub struct NullCompiler;

impl WasmCompiler for NullCompiler {
    fn compile(&self, _info: CodeInfo) -> Result<String, Error> {
        Ok(String::new())
    }

    fn compile_comptime(&self, _info: CodeInfo) -> Result<JsEnv, Error> {
        Ok(JsEnv::default())
    }
}

impl<T> WasmCompiler for &T
where
    T: WasmCompiler,
{
    fn compile(&self, info: CodeInfo) -> Result<String, Error> {
        (*self).compile(info)
    }

    fn compile_comptime(&self, info: CodeInfo) -> Result<JsEnv, Error> {
        (*self).compile_comptime(info)
    }
}

impl JsEnv {
    pub fn add(&mut self, decl: JsDecl) {
        self.0.push(decl);
    }

    pub fn items(&self) -> &[JsDecl] {
        &self.0
    }
}

impl FromIterator<JsDecl> for JsEnv {
    fn from_iter<T: IntoIterator<Item = JsDecl>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}
