use std::{
    borrow::Cow,
    fmt::{self, Display},
    path::{Path, PathBuf},
    time::Duration,
};

use indicatif::ProgressBar;

#[derive(Debug, Default)]
pub struct FinishLog {
    main_msg: Cow<'static, str>,
    sub_msg: Option<Cow<'static, str>>,
    files: Vec<PathBuf>,
    mods: Vec<Cow<'static, str>>,
    enable_color: bool,
}

impl FinishLog {
    const FINISHED: &str = "DONE";
    const FINISHED_COLOR: &str = "\x1b[32;1mDONE\x1b[0m";

    pub fn with_main_message<T>(&mut self, msg: T) -> &mut Self
    where
        T: Into<Cow<'static, str>>,
    {
        self.main_msg = msg.into();
        self
    }

    pub fn enable_color(&mut self, color: bool) -> &mut Self {
        self.enable_color = color;
        self
    }

    pub fn with_sub_message<T>(&mut self, msg: T) -> &mut Self
    where
        T: Into<Cow<'static, str>>,
    {
        self.sub_msg = Some(msg.into());
        self
    }

    pub fn with_file(&mut self, file: impl AsRef<Path>) -> &mut Self {
        self.files.push(file.as_ref().to_path_buf());
        self
    }

    pub fn with_mod<T>(&mut self, m: T) -> &mut Self
    where
        T: Into<Cow<'static, str>>,
    {
        self.mods.push(m.into());
        self
    }
}

impl Display for FinishLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}",
            if self.enable_color {
                Self::FINISHED_COLOR
            } else {
                Self::FINISHED
            },
            self.main_msg
        )?;
        if let Some(sub_msg) = self.sub_msg.as_ref() {
            write!(f, ": {}", sub_msg)?;
        }
        if !self.mods.is_empty() {
            write!(f, " [{}]", self.mods.join(" + "))?;
        }
        if !self.files.is_empty() {
            write!(f, " (")?;
            for file in &self.files {
                if self.enable_color {
                    write!(f, "\x1b[34m{}\x1b[0m", file.display())?;
                } else {
                    write!(f, "{}", file.display())?;
                }
            }
            write!(f, ")")?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Spinner(ProgressBar);

impl Spinner {
    const SPINNER_TICK: Duration = Duration::from_micros(500);

    pub fn new<T>(msg: T) -> Self
    where
        T: Into<Cow<'static, str>>,
    {
        let bar = ProgressBar::new_spinner().with_message(msg);
        bar.enable_steady_tick(Self::SPINNER_TICK);
        Self(bar)
    }

    pub fn finish(&self, finish_log: String) {
        self.0.suspend(|| {
            println!("{finish_log}");
        });
        self.0.finish_and_clear();
    }
}
