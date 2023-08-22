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
use decorous_errors::{DiagnosticBuilder, Report, Severity};
use itertools::Itertools;
use scopeguard::defer;
use shlex::Shlex;
use tempdir::TempDir;
use wasm_opt::OptimizationOptions;
use which::which;

use crate::{
    cli::OptimizationLevel,
    config::{Config, ScriptOrFile, WasmFeature},
    indicators::{FinishLog, Spinner},
    utils,
};

#[derive(Debug)]
pub struct MainCompiler<'a> {
    config: &'a Config,
    build_args: &'a str,
    out_name: &'a str,
    input_path: &'a Path,
    opt_level: Option<OptimizationLevel>,
    strip: bool,
    enable_color: bool,
}

impl<'a> MainCompiler<'a> {
    pub fn new(
        config: &'a Config,
        out_name: &'a str,
        build_args: &'a str,
        opt_level: Option<OptimizationLevel>,
        strip: bool,
        enable_color: bool,
        input_path: &'a Path,
    ) -> Self {
        Self {
            config,
            build_args,
            out_name,
            opt_level,
            strip,
            enable_color,
            input_path,
        }
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
                let dir = TempDir::new(lang).context("error creating temp dir for compiler")?;
                let path: PathBuf = dir.path().join(format!("__tmp.{}", config.ext_override.as_deref().unwrap_or(lang)));

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
                let outdir = fs::canonicalize(self.out_name).unwrap();

                let python = self.get_python().context("python not found in $PATH! Make sure to install it!")?;
                let mut build_args = Shlex::new(self.build_args);

                let file_loc = match &config.script {
                    ScriptOrFile::File(file) => Cow::Owned(fs::canonicalize(file.as_path()).context("error getting absolute path of script")?),
                    ScriptOrFile::Script(script) => {
                        fs::write(dir.path().join("__tmp.py"), script)?;
                        Cow::Borrowed(Path::new("__tmp.py"))
                    },
                };
                // This defer! cannot be used in the above match statement, as it executes when a
                // scope ends, and match arms have individual scopes
                defer! {
                    if matches!(&config.script, ScriptOrFile::Script(_)) {
                        fs::remove_file(dir.path().join("__tmp.py")).expect("error removing \"__tmp.py\"! Remove it manually!");
                    }
                }

                let cache_path = if config.use_cache {
                    gen_cache(self.input_path)?
                } else {
                    PathBuf::new()
                };
                let script_out = Command::new(python.as_ref())
                    .arg(file_loc.as_ref())
                    .env("DECOR_INPUT", &path)
                    .env("DECOR_OUT", self.out_name)
                    .env("DECOR_OUT_DIR", outdir)
                    .env("DECOR_EXPORTS", exports.iter().join(" "))
                    .env("DECOR_CACHE", &cache_path)
                    .current_dir(dir.path())
                    .args(&mut build_args)
                    .output()?;
                let (status, stdout, stderr) = (script_out.status, script_out.stdout, script_out.stderr);
                if cache_path != Path::new("") && fs::read_dir(&cache_path).context("error reading cache dir")?.count() == 0 {
                    fs::remove_dir(&cache_path).context("error removing cache dir - should be empty")?;
                }

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
                        let mut args = Shlex::new(self.build_args).join(" ");
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
        );
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

fn gen_cache(path: impl AsRef<Path>) -> Result<PathBuf> {
    let base = utils::get_cache_base().context("could not get cache base")?;

    let hash = match path.as_ref().to_string_lossy() {
        Cow::Owned(path) => sha256::digest(path),
        Cow::Borrowed(path) => sha256::digest(path),
    };
    let cache_dir = base.join(hash);
    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir)?;
    }

    Ok(cache_dir)
}
