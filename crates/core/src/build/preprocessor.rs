use duct::cmd;
use std::borrow::Cow;
use tempdir::TempDir;

use decorous_frontend::{location::Location, Override, PreprocessError, Preprocessor};

use crate::{
    config::{Config, PreprocTarget},
    indicators::{FinishLog, Spinner},
};

#[derive(Debug)]
pub struct Preproc<'a> {
    config: &'a Config,
    enable_color: bool,
}

impl<'a> Preproc<'a> {
    pub fn new(config: &'a Config, enable_color: bool) -> Self {
        Self {
            config,
            enable_color,
        }
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
            let spinner = Spinner::new("Running preprocessor");
            let temp = TempDir::new(lang).map_err(|err| {
                PreprocessError::new(
                    Location::default(),
                    Cow::Owned(format!(
                        "error creating temporary directory for preprocessing: {err}"
                    )),
                )
            })?;
            let out = cmd!("echo", to_pipe.as_ref())
                .pipe(cmd!("sh", "-c", comp))
                .dir(temp.path())
                .stdout_capture()
                .unchecked()
                .run()
                .map_err(|err| {
                    PreprocessError::new(
                        Location::default(),
                        Cow::Owned(format!("error preprocessing this code block: {err}")),
                    )
                })?;
            let stdout = String::from_utf8(out.stdout).map_err(|err| {
                PreprocessError::new(
                    Location::default(),
                    Cow::Owned(format!(
                        "preprocessor for {lang} stdout was not valid UTF-8: {err}"
                    )),
                )
            })?;
            if !out.status.success() {
                // Re-print stdout. stderr is already not redirected
                return Err(PreprocessError::new(
                    Location::default(),
                    Cow::Owned(format!("error preprocessing this code block:\n{stdout}")),
                ));
            }
            to_pipe = Cow::Owned(stdout);
            spinner.finish(
                FinishLog::default()
                    .enable_color(self.enable_color)
                    .with_main_message("preprocessor")
                    .with_sub_message(format!(
                        "{} - {lang}",
                        match cfg.target {
                            PreprocTarget::Js => "JavaScript",
                            PreprocTarget::Css => "CSS",
                        }
                    ))
                    .with_mod(format!("{}/{len}", i + 1))
                    .to_string(),
            );
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
