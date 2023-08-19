use decorous_errors::{DiagnosticBuilder, Report, Severity};
use std::{
    borrow::Cow,
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    str,
};

use anyhow::{bail, Context, Error, Result};
use decorous_backend::{dom_render::DomRenderer, prerender::Prerenderer, CodeInfo, WasmCompiler};
use itertools::Itertools;
use scopeguard::defer;
use shlex::Shlex;
use wasm_opt::OptimizationOptions;
use which::which;

use crate::{
    cli::OptimizationLevel,
    config::{Config, ScriptOrFile, WasmFeature},
    indicators::{FinishLog, Spinner},
};

#[derive(Debug)]
pub struct MainCompiler<'a> {
    config: &'a Config,
    build_args: &'a [(String, String)],
    out_name: &'a str,
    opt_level: Option<OptimizationLevel>,
    strip: bool,
    enable_color: bool,
}

impl<'a> MainCompiler<'a> {
    pub fn new(
        config: &'a Config,
        out_name: &'a str,
        build_args: &'a [(String, String)],
        opt_level: Option<OptimizationLevel>,
        strip: bool,
        enable_color: bool,
    ) -> Self {
        Self {
            config,
            build_args,
            out_name,
            opt_level,
            strip,
            enable_color,
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
                warn_unused_deps(&config.deps)?;
                let path: PathBuf = format!("__tmp.{}", config.ext_override.as_deref().unwrap_or(lang)).into();

                let spinner = Spinner::new(format!("Building WebAssembly ({lang})..."));

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

                spinner.finish(FinishLog::default()
                    .with_main_message("WebAssembly")
                    .with_sub_message(format!("{lang}{}", {
                        let mut args = self.build_args_for_lang(lang).join(" ");
                        if !args.is_empty() {
                            args.insert_str(0, " `");
                            args.push('`')
                        }
                        args
                    }))
                    .with_file(self.out_name)
                    .enable_color(self.enable_color)
                    .to_string()
                );

                let wasm_files = fs::read_dir(self.out_name)?
                    .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                    .filter(|path| match path.extension() {
                        Some(ext) if ext == OsStr::new("wasm") => true,
                        _ => false,
                    })
                    .collect_vec();

                if let Some(opt) = self.opt_level {
                    for path in &wasm_files {
                        let spinner = Spinner::new(format!("Optimizing WebAssembly ({opt})..."));
                        optimize(path, opt, &config.features).context("problem optimizing WebAssembly")?;
                        spinner.finish(
                            FinishLog::default()
                               .with_main_message("optimized WebAssembly")
                               .with_sub_message(opt.to_string())
                               .with_file(path)
                               .enable_color(self.enable_color)
                               .to_string()
                        );
                    }
                }

                if self.strip {
                    for path in &wasm_files {
                        let spinner = Spinner::new("Stripping WebAssembly...");
                        strip(&path).context("problem stripping WebAssembly binary")?;
                        spinner.finish(
                            FinishLog::default()
                               .with_main_message("stripped WebAssembly")
                               .with_file(path)
                               .enable_color(self.enable_color)
                               .to_string()
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

fn warn_unused_deps(deps: &[String]) -> Result<()> {
    let mut report = Report::new();
    for bin in deps.iter().filter(|b| which(b).is_err()) {
        report.add_diagnostic(
            DiagnosticBuilder::new(
                format!("script dependency not found: {bin}"),
                Severity::Warning,
                0,
            )
            .build(),
        )
    }
    if !report.is_empty() {
        decorous_errors::fmt::report(&report, "", "")?;
    }

    Ok(())
}

fn optimize(
    path: impl AsRef<Path>,
    level: OptimizationLevel,
    features: &[WasmFeature],
) -> Result<()> {
    let enabled_featues = features.iter().map(|feat| feat.0).collect();

    let mut opts = match level {
        OptimizationLevel::SpeedMinor => OptimizationOptions::new_opt_level_1(),
        OptimizationLevel::SpeedMedium => OptimizationOptions::new_opt_level_2(),
        OptimizationLevel::SpeedMajor => OptimizationOptions::new_opt_level_3(),
        OptimizationLevel::SpeedAggressive => OptimizationOptions::new_opt_level_4(),
        OptimizationLevel::Size => OptimizationOptions::new_optimize_for_size(),
        OptimizationLevel::SizeAggressive => {
            OptimizationOptions::new_optimize_for_size_aggressively()
        }
    };
    let path = path.as_ref();
    opts.features.enabled = enabled_featues;
    opts.run(path, path)?;

    Ok(())
}
