use duct::cmd;
use std::borrow::Cow;

use decorous_frontend::{location::Location, Override, PreprocessError, Preprocessor};

use crate::config::{Config, PreprocTarget};

#[derive(Debug)]
pub struct Preproc<'a> {
    config: &'a Config<'a>,
}

impl<'a> Preproc<'a> {
    pub fn new(config: &'a Config<'a>) -> Self {
        Self { config }
    }
}

impl Preprocessor for Preproc<'_> {
    fn preprocess(&self, lang: &str, body: &str) -> Result<Override, PreprocessError> {
        let Some(cfg) = &self.config.preprocessors.get(lang) else {
            return Ok(Override::None);
        };

        let mut to_pipe = Cow::Borrowed(body);
        for comp in &cfg.pipeline {
            let out = cmd!("echo", to_pipe.as_ref())
                .pipe(cmd!("sh", "-c", comp))
                .read()
                .map_err(|err| {
                    PreprocessError::new(
                        Location::default(),
                        Cow::Owned(format!("error preprocessing this code block: {err}")),
                    )
                })?;
            to_pipe = Cow::Owned(out);
        }

        match to_pipe {
            Cow::Owned(s) => Ok(if cfg.target == PreprocTarget::Js {
                Override::Js(s)
            } else {
                Override::Css(s)
            }),
            Cow::Borrowed(_) => Ok(Override::None),
        }
    }
}
