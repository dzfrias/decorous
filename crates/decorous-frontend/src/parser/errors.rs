use std::fmt;

use nom_locate::LocatedSpan;
use smallvec::{smallvec, SmallVec};
use thiserror::Error;

use crate::{css, location::Location, PreprocessError};

/// Describes possible parsing errors of the [`parse`](crate::parse) function.
#[derive(Debug, Error, PartialEq, Clone)]
pub enum ParseErrorType {
    #[error("invalid closing tag, expected {0}")]
    InvalidClosingTag(String),
    #[error("unclosed tag: {0} from line {1}")]
    UnclosedTag(String, u32),
    #[error("invalid character, expected {0}")]
    ExpectedCharacter(char),
    #[error("invalid character, expected {0:?}, got {1}")]
    ExpectedCharacterAny(Vec<char>, char),
    #[error("expected closing tag. If you meant to escape the slash, use '\\/'")]
    ExpectedClosingTag,
    #[error("cannot have more than one script block")]
    CannotHaveTwoScripts,
    #[error("cannot have more than one style block")]
    CannotHaveTwoStyles,
    #[error("cannot have more than one WebAssembly block")]
    CannotHaveTwoWasmBlocks,
    #[error("parse error in JavaScript: {title}")]
    JavaScriptDiagnostics { title: String },
    #[error("invalid special block type: {0}. Only `for` and `if` are accepted.")]
    InvalidSpecialBlockType(String),
    // Boxed because this enum variant would otherwise be very large.
    #[error("css parsing error: {0}")]
    CssParsingError(Box<css::error::ParseError<Location>>),
    #[error("{0}")]
    PreprocError(Box<PreprocessError>),
    #[error("byte processing error: {}", 0.to_string())]
    Nom(nom::error::ErrorKind),
}

/// A parsing error, with extra metadata. The root of this struct is in
/// [`ParseErrorType`](crate::errors::ParseErrorType).
///
/// For more on parsing, see the [`parse`](crate::parse) function.
///
/// `ParseError` can have arbitrary metadata, retrieved by the
/// [`fragment()`](ParseError::fragment()) method. Along with metadata, `ParseError` can have
/// attached [`Help`] data.
#[derive(Debug, Clone, PartialEq, Error)]
#[error("parser error: {err_type}")]
pub struct ParseError<T> {
    fragment: T,
    help: Option<Help>,
    err_type: ParseErrorType,
}

/// An error help message, commonly created alongside a [`ParseError`].
#[derive(Debug, Clone, PartialEq)]
pub struct Help {
    corresponding_line: Option<u32>,
    message: &'static str,
}

/// A full report of [`ParseError`]s.
///
/// This is usually produced along the [`parse`](crate::parse) function.
#[derive(Debug, Clone, PartialEq)]
pub struct Report<T> {
    errors: SmallVec<[ParseError<T>; 1]>,
}

impl<T> Report<T> {
    pub fn errors(&self) -> &[ParseError<T>] {
        self.errors.as_ref()
    }
}

impl From<Report<LocatedSpan<&str>>> for Report<Location> {
    fn from(report: Report<LocatedSpan<&str>>) -> Self {
        let mut new_report = SmallVec::with_capacity(report.errors.len());
        for err in report.errors {
            new_report.push(ParseError::new(err.fragment.into(), err.err_type, err.help));
        }
        Self { errors: new_report }
    }
}

impl<T> From<ParseError<T>> for Report<T> {
    fn from(err: ParseError<T>) -> Self {
        Self {
            errors: smallvec![err],
        }
    }
}

impl<T> nom::error::ParseError<T> for Report<T> {
    fn from_error_kind(fragment: T, kind: nom::error::ErrorKind) -> Self {
        Self::from(ParseError {
            fragment,
            err_type: ParseErrorType::Nom(kind),
            help: None,
        })
    }

    fn append(input: T, kind: nom::error::ErrorKind, mut other: Self) -> Self {
        other.errors.push(ParseError {
            fragment: input,
            err_type: ParseErrorType::Nom(kind),
            help: None,
        });
        other
    }

    fn from_char(input: T, c: char) -> Self {
        Self::from(ParseError {
            fragment: input,
            err_type: ParseErrorType::ExpectedCharacter(c),
            help: None,
        })
    }
}

impl<T> ParseError<T> {
    pub fn new(fragment: T, err_type: ParseErrorType, help: Option<Help>) -> Self {
        Self {
            fragment,
            err_type,
            help,
        }
    }

    pub fn err_type(&self) -> &ParseErrorType {
        &self.err_type
    }

    pub fn fragment(&self) -> &T {
        &self.fragment
    }

    pub fn help(&self) -> Option<&Help> {
        self.help.as_ref()
    }
}

impl Help {
    pub fn with_line(line: u32, message: &'static str) -> Self {
        Self {
            corresponding_line: Some(line),
            message,
        }
    }

    /// Creates a new `Help`, with no corresponding line.
    pub fn with_message(message: &'static str) -> Self {
        Self {
            corresponding_line: None,
            message,
        }
    }

    pub fn corresponding_line(&self) -> Option<u32> {
        self.corresponding_line
    }

    pub fn message(&self) -> &str {
        self.message
    }
}

impl fmt::Display for Help {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}
