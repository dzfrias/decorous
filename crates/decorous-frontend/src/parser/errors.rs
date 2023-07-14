use std::{fmt, io};

use nom_locate::LocatedSpan;
use thiserror::Error;

use crate::ast::Location;

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
    #[error("cannot have non-toplevel script")]
    CannotHaveTwoScripts,
    #[error("cannot have two style tags")]
    CannotHaveTwoStyleTags,
    #[error("parse error in JavaScript: {}", 0.to_string())]
    JavaScriptDiagnostics {
        errors: Vec<rslint_errors::Diagnostic>,
        offset: usize,
    },
    #[error("byte processing error: {}", 0.to_string())]
    Nom(nom::error::ErrorKind),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError<T> {
    fragment: T,
    help: Option<Help>,
    err_type: ParseErrorType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Help {
    corresponding_line: Option<u32>,
    message: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report<T> {
    errors: Vec<ParseError<T>>,
}

impl<T> Report<T> {
    pub fn errors(&self) -> &[ParseError<T>] {
        self.errors.as_ref()
    }
}

impl Report<LocatedSpan<&str>> {
    pub fn format<T: io::Write>(&self, input: &str, out: &mut T) -> io::Result<()> {
        for err in self.errors() {
            let lines = input.lines().enumerate();
            // Minus one because location_line is 1-indexed
            let line_no = err.fragment().location_line() - 1;

            // Write the error description
            writeln!(out, "error: {}", err.err_type())?;
            if let Some(help_line) = err
                .help()
                .and_then(|help| help.corresponding_line())
                .filter(|ln| ln <= &line_no)
            {
                let (_, line) = lines
                    .clone()
                    .skip_while(|(n, _)| *n as u32 != help_line - 1)
                    .next()
                    .expect("should be in lines");
                writeln!(out, "{help_line}| {} <-- this line", line)?;
                if help_line + 1 != line_no {
                    writeln!(out, "...")?;
                }
            }
            let (i, line) = lines
                .clone()
                .skip_while(|(n, _)| (*n as u32) + 1 < line_no)
                .next()
                .expect("line should be in input");

            writeln!(out, "{}| {line}", i + 1)?;
            // Plus one because line_no is 0 indexed, so we need to get the actual line number
            let line_no_len = count_digits(line_no + 1) as usize;
            let col = err.fragment().get_column() + line_no_len + 2;
            writeln!(out, "{arrow:>col$}", arrow = "^")?;

            if let Some(help) = err.help() {
                writeln!(out, "help: {help}")?;
            }
            writeln!(out)?;
        }
        Ok(())
    }
}

impl From<Report<LocatedSpan<&str>>> for Report<Location> {
    fn from(report: Report<LocatedSpan<&str>>) -> Self {
        let mut new_report = Vec::with_capacity(report.errors.len());
        for err in report.errors {
            new_report.push(ParseError::new(err.fragment.into(), err.err_type, err.help));
        }
        Self { errors: new_report }
    }
}

fn count_digits(num: u32) -> u32 {
    num.checked_ilog10().unwrap_or(0) + 1
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
