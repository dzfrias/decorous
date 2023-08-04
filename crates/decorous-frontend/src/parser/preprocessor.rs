use std::borrow::Cow;
use thiserror::Error;

use crate::location::Location;

#[derive(Debug, Error, Clone, PartialEq)]
#[error("preprocessing error: {msg}")]
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
