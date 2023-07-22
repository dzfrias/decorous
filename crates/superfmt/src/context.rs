#[cfg(feature = "style")]
use crate::style::{Color, Style};
use std::fmt::Display;

pub struct Context {
    pub(super) starts_with: Box<dyn Display>,
    pub(super) ends_with: Box<dyn Display>,
    pub(super) prepend: Box<dyn Display>,
    pub(super) append: Box<dyn Display>,
}

#[derive(Default)]
pub struct ContextBuilder {
    starts_with: Option<Box<dyn Display>>,
    prepend: Option<Box<dyn Display>>,
    ends_with: Option<Box<dyn Display>>,
    append: Option<Box<dyn Display>>,
}

impl Context {
    #[cfg(feature = "style")]
    pub fn style(s: Style) -> Self {
        Self {
            starts_with: Box::new(s),
            ends_with: Box::new(Style::reset()),
            prepend: Box::new(""),
            append: Box::new(""),
        }
    }

    #[cfg(feature = "style")]
    pub fn color(c: Color) -> Self {
        Self {
            starts_with: Box::new(c),
            ends_with: Box::new(Color::Reset),
            prepend: Box::new(""),
            append: Box::new(""),
        }
    }
}

impl ContextBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn starts_with<T: Display + 'static>(mut self, starts_with: T) -> Self {
        self.starts_with = Some(Box::new(starts_with));
        self
    }

    #[must_use]
    pub fn ends_with<T: Display + 'static>(mut self, ends_with: T) -> Self {
        self.ends_with = Some(Box::new(ends_with));
        self
    }

    #[must_use]
    pub fn prepend<T: Display + 'static>(mut self, prepend: T) -> Self {
        self.prepend = Some(Box::new(prepend));
        self
    }

    #[must_use]
    pub fn append<T: Display + 'static>(mut self, append: T) -> Self {
        self.append = Some(Box::new(append));
        self
    }

    pub fn build(self) -> Context {
        Context {
            starts_with: self.starts_with.unwrap_or(Box::new("")),
            prepend: self.prepend.unwrap_or(Box::new("")),
            ends_with: self.ends_with.unwrap_or(Box::new("")),
            append: self.append.unwrap_or(Box::new("")),
        }
    }
}
