use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum ParseErrorType {
    #[error("expected: {0}")]
    ExpectedCharacter(char),
    #[error("expected media query name")]
    ExpectedMediaQueryName,
    #[error("expected selector")]
    ExpectedSelector,
}

#[derive(Debug, PartialEq, Error, Clone)]
#[error("parse error: {err_type}")]
pub struct ParseError<T> {
    fragment: T,
    #[source]
    err_type: ParseErrorType,
}

impl<T> ParseError<T> {
    pub fn new(err_type: ParseErrorType, fragment: T) -> Self {
        Self { fragment, err_type }
    }
}
