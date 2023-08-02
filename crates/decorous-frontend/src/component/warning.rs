use rslint_parser::SmolStr;
use thiserror::Error;

use crate::location::Location;

#[derive(Debug, Error)]
pub enum WarningType {
    #[error("unbound variable used: {0}")]
    UnboundVariable(SmolStr),
}

#[derive(Debug)]
pub struct Warning {
    inner: WarningType,
    loc: Location,
}

impl Warning {
    pub fn new(inner: WarningType, loc: Location) -> Self {
        Self { inner, loc }
    }

    pub fn inner(&self) -> &WarningType {
        &self.inner
    }

    pub fn loc(&self) -> Location {
        self.loc
    }
}
