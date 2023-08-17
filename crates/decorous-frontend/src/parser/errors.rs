use std::{borrow::Cow, fmt, ops::Range};

use decorous_errors::{DiagnosticBuilder, Severity};
use nom_locate::LocatedSpan;
use thiserror::Error;

use crate::{css, location::Location, PreprocessError};

/// Describes possible parsing errors of the [`parse`](crate::parse) function.
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
    corresponding_span: Option<Range<usize>>,
    message: &'static str,
}

/// A full report of [`ParseError`]s.
///
/// This is usually produced along the [`parse`](crate::parse) function.
#[derive(Debug, Clone)]
pub struct Report(pub decorous_errors::Report);

impl From<ParseError<LocatedSpan<&str>>> for Report {
    fn from(err: ParseError<LocatedSpan<&str>>) -> Self {
        let mut diagnostic = DiagnosticBuilder::new(
            err.to_string(),
            Severity::Error,
            err.fragment().location_offset(),
        )
        .build();
        if let Some(help) = err.help() {
            diagnostic.note = Some(Cow::Borrowed(help.message()));
            if let Some(span) = help.corresponding_span() {
                diagnostic.helpers.push(decorous_errors::Helper {
                    msg: Cow::Borrowed("from here"),
                    span: span.clone(),
                })
            }
        }
        diagnostic.helpers.push(decorous_errors::Helper {
            msg: Cow::Borrowed("here"),
            span: err.fragment().location_offset()..err.fragment().location_offset() + 1,
        });
        Self(decorous_errors::Report::new(vec![diagnostic]))
    }
}

impl nom::error::ParseError<LocatedSpan<&str>> for Report {
    fn from_error_kind(fragment: LocatedSpan<&str>, kind: nom::error::ErrorKind) -> Self {
        Self::from(ParseError {
            fragment,
            err_type: ParseErrorType::Nom(kind),
            help: None,
        })
    }

    fn append(input: LocatedSpan<&str>, kind: nom::error::ErrorKind, mut other: Self) -> Self {
        other.0.add_diagnostic(
            DiagnosticBuilder::new(
                ParseErrorType::Nom(kind).to_string(),
                Severity::Error,
                input.location_offset(),
            )
            .build(),
        );
        other
    }

    fn from_char(input: LocatedSpan<&str>, c: char) -> Self {
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
    pub fn with_span(span: Range<usize>, message: &'static str) -> Self {
        Self {
            corresponding_span: Some(span),
            message,
        }
    }

    /// Creates a new `Help`, with no corresponding line.
    pub fn with_message(message: &'static str) -> Self {
        Self {
            corresponding_span: None,
            message,
        }
    }

    pub fn corresponding_span(&self) -> Option<&Range<usize>> {
        self.corresponding_span.as_ref()
    }

    pub fn message(&self) -> &'static str {
        self.message
    }
}

impl fmt::Display for Help {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}
