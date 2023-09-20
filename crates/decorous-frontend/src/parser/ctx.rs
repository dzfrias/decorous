use decorous_errors::DynErrStream;
use std::{borrow::Cow, fmt, io};
use thiserror::Error;

use crate::{ast::Code, location::Location};

#[derive(Clone)]
pub struct Ctx<'a> {
    pub preprocessor: &'a dyn Preprocessor,
    pub executor: &'a dyn CodeExecutor,
    pub errs: DynErrStream<'a>,
}

impl fmt::Debug for Ctx<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ctx")
            .field("preprocessor", &"preproc")
            .field("preprocessor", &"exec")
            .field("errs", &self.errs)
            .finish()
    }
}

impl Default for Ctx<'_> {
    fn default() -> Self {
        Self {
            preprocessor: &NullPreproc,
            executor: &NullExecutor,
            errs: DynErrStream::new(
                Box::new(io::stderr()),
                decorous_errors::Source {
                    name: "CTX_DEFAULT".to_owned(),
                    src: "",
                },
            ),
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq)]
#[error("{msg}")]
#[non_exhaustive]
pub struct PreprocessError {
    pub loc: Location,
    pub msg: Cow<'static, str>,
}

impl PreprocessError {
    pub fn new(loc: Location, msg: Cow<'static, str>) -> Self {
        Self { loc, msg }
    }
}

#[derive(Debug)]
pub enum Override {
    Css(String),
    Js(String),
    None,
}

pub trait Preprocessor {
    fn preprocess(&self, lang: &str, body: &str) -> Result<Override, PreprocessError>;
}

impl<T> Preprocessor for &T
where
    T: Preprocessor,
{
    fn preprocess(&self, lang: &str, body: &str) -> Result<Override, PreprocessError> {
        (*self).preprocess(lang, body)
    }
}

pub struct NullPreproc;

impl Preprocessor for NullPreproc {
    fn preprocess(
        &self,
        _lang: &str,
        _body: &str,
    ) -> std::result::Result<Override, PreprocessError> {
        Ok(Override::None)
    }
}

pub trait CodeExecutor {
    fn execute(&self, code: &Code) -> Result<JsEnv, anyhow::Error>;
}

impl<T> CodeExecutor for &T
where
    T: CodeExecutor,
{
    fn execute(&self, code: &Code) -> Result<JsEnv, anyhow::Error> {
        (*self).execute(code)
    }
}

pub struct NullExecutor;

impl CodeExecutor for NullExecutor {
    fn execute(&self, _code: &Code) -> Result<JsEnv, anyhow::Error> {
        Ok(JsEnv::default())
    }
}

#[derive(Debug, Default, Clone)]
pub struct JsEnv(Vec<JsDecl>);

#[derive(Debug, Clone)]
pub struct JsDecl {
    pub name: String,
    pub value: String,
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
