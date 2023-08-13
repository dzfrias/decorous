use indicatif::ProgressBar;
use std::{
    borrow::Cow,
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    str,
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Error, Result};
use binaryen::{CodegenConfig, Module};
use decorous_backend::{dom_render::DomRenderer, prerender::Prerenderer, CodeInfo, WasmCompiler};
use itertools::Itertools;
use scopeguard::defer;
use shlex::Shlex;
use which::which;

use crate::{
    cli::OptimizationLevel,
    config::{Config, ScriptOrFile},
    FINISHED,
};

#[derive(Debug)]
pub struct MainCompiler<'a> {
    config: &'a Config,
    build_args: &'a [(String, String)],
    out_name: &'a str,
    opt_level: Option<OptimizationLevel>,
    strip: bool,
}

impl<'a> MainCompiler<'a> {
    pub fn new(
        config: &'a Config,
        out_name: &'a str,
        build_args: &'a [(String, String)],
        opt_level: Option<OptimizationLevel>,
        strip: bool,
    ) -> Self {
        Self {
            config,
            build_args,
            out_name,
            opt_level,
            strip,
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

                fs::write(&path, body)?;

                defer! {
                    // No .expect() to save on format! call
                    fs::remove_file(&path).unwrap_or_else(|_| {
                        panic!("error removing \"{}\"! Remove it manually!", path.display())
                    });
                }

                match fs::create_dir(self.out_name) {
                    Ok(()) => {}
                    Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(err) => bail!(err),
                }

                let python = self.get_python().context("python not found in $PATH! Make sure to install it!")?;
                let mut build_args = self.build_args_for_lang(lang);

                let file_loc = match &config.script {
                    ScriptOrFile::File(file) => file.as_path(),
                    ScriptOrFile::Script(script) => {
                        fs::write("__tmp.py", script)?;
                        Path::new("__tmp.py")
                    },
                };
                // This defer! cannot be used in the above match statement, as it executes when a
                // scope ends, and match arms have individual scopes
                defer! {
                    if matches!(&config.script, ScriptOrFile::Script(_)) {
                        fs::remove_file("__tmp.py").expect("error removing \"__tmp.py\"! Remove it manually!");
                    }
                }

                let script_out = Command::new(python.as_ref())
                    .arg(file_loc)
                    .env("DECOR_INPUT", &path)
                    .env("DECOR_OUT", self.out_name)
                    .env("DECOR_EXPORTS", exports.iter().join(" "))
                    .args(&mut build_args)
                    .output()?;
                let (status, stdout, stderr) = (script_out.status, script_out.stdout, script_out.stderr);

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

                let wasm_files = fs::read_dir(self.out_name)?
                    .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                    .filter(|path| match path.extension() {
                        Some(ext) if ext == OsStr::new("wasm") => true,
                        _ => false,
                    })
                    .collect_vec();

                if let Some(opt) = self.opt_level {
                    for path in &wasm_files {
                        let spinner = ProgressBar::new_spinner().with_message(format!("Optimizing WebAssembly ({opt})..."));
                        spinner.enable_steady_tick(Duration::from_micros(100));
                        optimize(&path, opt)?;
                        spinner.finish_with_message(
                            format!("{FINISHED} optimized WebAssembly: {opt} (\x1b[34m{}\x1b[0m)", path.display())
                        );
                    }
                }

                if self.strip {
                    for path in &wasm_files {
                        let spinner = ProgressBar::new_spinner().with_message(format!("Stripping WebAssembly..."));
                        spinner.enable_steady_tick(Duration::from_micros(100));
                        strip(&path)?;
                        spinner.finish_with_message(
                            format!("{FINISHED} stripped WebAssembly (\x1b[34m{}\x1b[0m)", path.display())
                        );
                    }
                }


                Ok(())
            }
        }
    };
}

compile_for!(DomRenderer);
compile_for!(Prerenderer);

fn strip(file: impl AsRef<Path>) -> Result<()> {
    let mut module = walrus::Module::from_file(&file)?;
    let to_remove = module.customs.iter().map(|(id, _)| id).collect_vec();
    for id in to_remove {
        module.customs.delete(id);
    }
    module.emit_wasm_file(file)?;

    Ok(())
}

fn optimize(path: impl AsRef<Path>, level: OptimizationLevel) -> Result<()> {
    let (shrink, speed) = match level {
        OptimizationLevel::SpeedMinor => (0, 1),
        OptimizationLevel::SpeedMedium => (0, 2),
        OptimizationLevel::SpeedMajor => (0, 3),
        OptimizationLevel::SpeedAggressive => (0, 4),
        OptimizationLevel::Size => (1, 2),
        OptimizationLevel::SizeAggressive => (2, 2),
    };
    let path = path.as_ref();
    let contents = fs::read(path)?;
    // Uses wasm-opt (https://github.com/WebAssembly/binaryen) optimizations
    let mut module = Module::read(&contents)
        .map_err(|_err| anyhow!("could not optimize .wasm file: {}", path.display()))?;
    let config = CodegenConfig {
        shrink_level: shrink,
        optimization_level: speed,
        debug_info: false,
    };
    module.optimize(&config);
    let out = module.write();
    fs::write(path, out)?;

    Ok(())
}
