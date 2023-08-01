use thiserror::Error;

use crate::errors::Help;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum ParseErrorType {
    #[error("expected: {0}")]
    ExpectedCharacter(char),
    #[error("expected media query name")]
    ExpectedMediaQueryName,
    #[error("expected selector")]
    ExpectedSelector,
    #[error("parse error in JavaScript: {}", 0.to_string())]
    JavaScriptParseError(Vec<rslint_errors::Diagnostic>),
}

#[derive(Debug, PartialEq, Error, Clone)]
#[error("parse error: {err_type}")]
pub struct ParseError<T> {
    fragment: T,
    help: Option<Help>,
    #[source]
    err_type: ParseErrorType,
}

impl<T> ParseError<T> {
    pub fn new(err_type: ParseErrorType, fragment: T, help: Option<Help>) -> Self {
        Self {
            fragment,
            err_type,
            help,
        }
    }

    pub fn fragment(&self) -> &T {
        &self.fragment
    }

    pub fn err_type(&self) -> &ParseErrorType {
        &self.err_type
    }

    pub fn help(&self) -> Option<&Help> {
        self.help.as_ref()
    }
}
