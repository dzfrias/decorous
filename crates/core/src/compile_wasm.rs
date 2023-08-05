use indicatif::ProgressBar;
use std::{
    borrow::Cow,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    str,
    time::Duration,
};

use anyhow::{bail, Context, Error, Result};
use decorous_backend::{dom_render::DomRenderer, prerender::Prerenderer, CodeInfo, WasmCompiler};
use itertools::Itertools;
use scopeguard::defer;
use shlex::Shlex;
use which::which;

use crate::{
    config::{Config, ScriptOrFile},
    FINISHED,
};

#[derive(Debug)]
pub struct MainCompiler<'a> {
    config: &'a Config,
    build_args: &'a [(String, String)],
    out_name: &'a str,
}

impl<'a> MainCompiler<'a> {
    pub fn new(config: &'a Config, out_name: &'a str, build_args: &'a [(String, String)]) -> Self {
        Self {
            config,
            build_args,
            out_name,
        }
    }

    fn build_args_for_lang(&self, lang: &str) -> Shlex {
        let args =
            self.build_args
                .iter()
                .find_map(|(l, args)| if l == lang { Some(args.as_str()) } else { None });
        args.map_or_else(|| Shlex::new(""), Shlex::new)
    }

    fn get_python(&self) -> Option<Cow<'_, Path>> {
        if let Some(py) = &self.config.python {
            return Some(Cow::Borrowed(py));
        }

        match which("python") {
            Ok(bin) => Some(bin.into()),
            Err(which::Error::CannotFindBinaryPath) => {
                which("python3").map_or(None, |bin| Some(bin.into()))
            }
            Err(_) => None,
        }
    }
}

macro_rules! compile_for {
    ($backend:ty) => {
        impl WasmCompiler<$backend> for MainCompiler<'_> {
            type Err = Error;

            fn compile<W>(&mut self, CodeInfo { lang, body, exports }: CodeInfo, out: &mut W) -> Result<(), Error>
            where
                W: io::Write,
            {
                let config = self
                    .config
                    .compilers
                    .get(lang)
                    .with_context(|| format!("unsupported language: {lang}"))?;
                let path: PathBuf = format!("__tmp.{}", config.ext_override.as_deref().unwrap_or(lang)).into();

                let spinner = ProgressBar::new_spinner().with_message(format!("Building WebAssembly ({lang})..."));
                spinner.enable_steady_tick(Duration::from_micros(100));

                {
                    let mut f = File::create(&path)?;
                    f.write_all(body.as_bytes())?;
                }

                let _guard = scopeguard::guard(&path, |p| {
                    fs::remove_file(p).unwrap_or_else(|_| {
                        panic!("error removing \"{}\"! Remove it manually!", p.display())
                    });
                });

                match fs::create_dir(self.out_name) {
                    Ok(()) => {}
                    Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(err) => bail!(err),
                }

                let python = self.get_python().context("python not found in PATH! Make sure to install it!")?;
                let mut build_args = self.build_args_for_lang(lang);

                let (status, stdout, stderr) = match &config.script {
                    ScriptOrFile::File(file) => {
                        let out = Command::new(python.as_ref())
                            .arg(file)
                            .env("DECOR_INPUT", &path)
                            .env("DECOR_OUT", self.out_name)
                            .env("DECOR_EXPORTS", exports.iter().join(" "))
                            .args(&mut build_args)
                            .output()?;
                        (out.status, out.stdout, out.stderr)
                    }
                    ScriptOrFile::Script(script) => {
                        {
                            let mut f = File::create("__tmp.py")?;
                            f.write_all(script.as_bytes())?;
                        }
                        defer! {
                            fs::remove_file("__tmp.py").expect("error removing \"__tmp.py\"! Remove it manually!");
                        }
                        let out = Command::new(python.as_ref())
                            .arg("__tmp.py")
                            .env("DECOR_INPUT", &path)
                            .env("DECOR_OUT", self.out_name)
                            .env("DECOR_EXPORTS", exports.iter().join(" "))
                            .args(&mut build_args)
                            .output()?;
                        (out.status, out.stdout, out.stderr)
                    }
                };

                if build_args.had_error {
                    bail!("error parsing build args for language: {lang}");
                }

                if !status.success() {
                    bail!(
                        "failed to compile to WebAssembly:\n{}\nwith stdout:\n{}",
                        str::from_utf8(&stderr)?,
                        str::from_utf8(&stdout)?,
                    );
                }

                out.write_all(&stdout)?;

                spinner.finish_with_message(format!("{FINISHED} WebAssembly: {lang}{} (\x1b[34m{}/\x1b[0m)", {
                    let mut args = self.build_args_for_lang(lang).join(" ");
                    if !args.is_empty() {
                        args.insert_str(0, " `");
                        args.push('`')
                    }
                    args
                }, self.out_name));

                Ok(())
            }
        }
    };
}

compile_for!(DomRenderer);
compile_for!(Prerenderer);
