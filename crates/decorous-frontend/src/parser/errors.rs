use thiserror::Error;

#[derive(Debug, Error, PartialEq, Clone)]
pub enum ParseErrorType {
    #[error("invalid closing tag, expected {0}")]
    InvalidClosingTag(String),
    #[error("unclosed tag: {0}")]
    UnclosedTag(String),
    #[error("invalid character, expected {0}")]
    ExpectedCharacter(char),
    #[error("invalid character, expected {0:?}, got {1}")]
    ExpectedCharacterAny(Vec<char>, char),
    #[error("expected closing tag. If you meant to escape the slash, use '\\/'")]
    ExpectedClosingTag,
    #[error("cannot have non-toplevel script")]
    CannotHaveTwoScripts,
    #[error("cannot have two style tags")]
    CannotHaveTwoStyleTags,
    #[error("javascript parsing error: {}", 0.to_string())]
    JavaScriptParseError(rslint_parser::ParserError),
    #[error("byte processing error: {}", 0.to_string())]
    Nom(nom::error::ErrorKind),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError<T> {
    fragment: T,
    err_type: ParseErrorType,
}

impl<T> ParseError<T> {
    pub fn new(fragment: T, err_type: ParseErrorType) -> Self {
        Self { fragment, err_type }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report<T> {
    errors: Vec<ParseError<T>>,
}

impl<T> From<ParseError<T>> for Report<T> {
    fn from(err: ParseError<T>) -> Self {
        Self { errors: vec![err] }
    }
}

impl<T> nom::error::ParseError<T> for Report<T> {
    fn from_error_kind(fragment: T, kind: nom::error::ErrorKind) -> Self {
        Self::from(ParseError {
            fragment,
            err_type: ParseErrorType::Nom(kind),
        })
    }

    fn append(input: T, kind: nom::error::ErrorKind, mut other: Self) -> Self {
        other.errors.push(ParseError {
            fragment: input,
            err_type: ParseErrorType::Nom(kind),
        });
        other
    }

    fn from_char(input: T, c: char) -> Self {
        Self::from(ParseError {
            fragment: input,
            err_type: ParseErrorType::ExpectedCharacter(c),
        })
    }
}
