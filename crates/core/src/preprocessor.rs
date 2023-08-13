use duct::cmd;
use duct_sh::sh_dangerous;
use indicatif::ProgressBar;
use std::{borrow::Cow, time::Duration};

use decorous_frontend::{location::Location, Override, PreprocessError, Preprocessor};

use crate::{
    config::{Config, PreprocTarget},
    FINISHED,
};

#[derive(Debug)]
pub struct Preproc<'a> {
    config: &'a Config,
}

impl<'a> Preproc<'a> {
    pub fn new(config: &'a Config) -> Self {
        Self { config }
    }
}

impl Preprocessor for Preproc<'_> {
    fn preprocess(&self, lang: &str, body: &str) -> Result<Override, PreprocessError> {
        let Some(cfg) = &self.config.preprocessors.get(lang) else {
            return Ok(Override::None);
        };

        let mut to_pipe = Cow::Borrowed(body);
        let len = cfg.pipeline.len();
        for (i, comp) in cfg.pipeline.iter().enumerate() {
            let spinner = ProgressBar::new_spinner().with_message("Running preprocessor...");
            spinner.enable_steady_tick(Duration::from_micros(100));
            let out = cmd!("echo", to_pipe.as_ref())
                .pipe(sh_dangerous(comp))
                .read()
                .map_err(|err| {
                    PreprocessError::new(
                        Location::default(),
                        Cow::Owned(format!("error preprocessing this code block: {err}")),
                    )
                })?;
            to_pipe = Cow::Owned(out);
            spinner.finish_with_message(format!(
                "{FINISHED} preprocessor: `{comp}` ({} [{}/{len}])",
                match cfg.target {
                    PreprocTarget::Js => "JavaScript",
                    PreprocTarget::Css => "CSS",
                },
                i + 1,
            ));
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
