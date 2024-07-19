use std::{
    borrow::Cow,
    cell::Cell,
    collections::HashMap,
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    str,
};

use anyhow::{bail, Context, Error, Result};
use decorous_backend::{CodeInfo, JsDecl, JsEnv, WasmCompiler};
use decorous_errors::{DiagnosticBuilder, Severity};
use decorous_frontend::{ast::Code, CodeExecutor};
use itertools::Itertools;
use scopeguard::defer;
use tempdir::TempDir;
use wasi_common::pipe::WritePipe;
use wasm_opt::OptimizationOptions;
use wasmtime::*;
use wasmtime_wasi::sync::WasiCtxBuilder;
use which::which;

use crate::{
    build::global_ctx::GlobalCtx,
    cli::OptimizationLevel,
    config::{ScriptOrFile, WasmFeature},
    indicators::{FinishLog, Spinner},
    utils,
};

pub struct MainCompiler<'a> {
    global_ctx: &'a GlobalCtx<'a>,
    comptime: Cell<bool>,
}

impl<'a> MainCompiler<'a> {
    pub fn new(global_ctx: &'a GlobalCtx<'a>) -> Self {
        Self {
            global_ctx,
            comptime: false.into(),
        }
    }

    fn get_python(&self) -> Option<Cow<'_, Path>> {
        if let Some(py) = &self.global_ctx.config.python {
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

    fn warn_unused_deps(&self, deps: &[String]) {
        for bin in deps.iter().filter(|b| which(b).is_err()) {
            self.global_ctx.errs.emit(
                DiagnosticBuilder::new(format!("script dependency not found: {bin}"), 0)
                    .severity(Severity::Warning)
                    .build(),
            );
        }
    }
}

impl WasmCompiler for MainCompiler<'_> {
    fn compile(
        &self,
        CodeInfo {
            lang,
            body,
            exports,
        }: CodeInfo,
    ) -> Result<String, Error> {
        let config = self
            .global_ctx
            .config
            .compilers
            .get(lang)
            .with_context(|| format!("unsupported language: {lang}"))?;
        self.warn_unused_deps(&config.deps);
        let dir = TempDir::new(lang).context("error creating temp dir for compiler")?;
        let path: PathBuf = dir.path().join(format!(
            "__tmp.{}",
            config.ext_override.as_deref().unwrap_or(lang)
        ));

        let msg = if !self.comptime.get() {
            format!("Building WebAssembly ({lang})...")
        } else {
            format!("Building static WebAssembly ({lang})...")
        };
        let spinner = Spinner::new(msg);

        fs::write(&path, body)?;

        defer! {
            // No .expect() to save on format! call
            fs::remove_file(&path).unwrap_or_else(|_| {
                panic!("error removing \"{}\"! Remove it manually!", path.display())
            });
        }

        match fs::create_dir(&self.global_ctx.args.out) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                fs::remove_dir_all(&self.global_ctx.args.out)
                    .context("error removing previous outdir")?;
                fs::create_dir(&self.global_ctx.args.out)
                    .context("error creating out directory")?;
            }
            Err(err) => bail!(err),
        }
        let outdir = fs::canonicalize(&self.global_ctx.args.out).unwrap();

        let python = self
            .get_python()
            .context("python not found in $PATH! Make sure to install it!")?;
        let file_loc = match &config.script {
            ScriptOrFile::File(file) => Cow::Owned(
                fs::canonicalize(file.as_path())
                    .context("error getting absolute path of script")?,
            ),
            ScriptOrFile::Script(script) => {
                fs::write(dir.path().join("__tmp.py"), script)?;
                Cow::Borrowed(Path::new("__tmp.py"))
            }
        };
        // This defer! cannot be used in the above match statement, as it executes when a
        // scope ends, and match arms have individual scopes
        defer! {
            if matches!(&config.script, ScriptOrFile::Script(_)) {
                fs::remove_file(dir.path().join("__tmp.py")).expect("error removing \"__tmp.py\"! Remove it manually!");
            }
        }

        let input_path = fs::canonicalize(&self.global_ctx.args.input)
            .context("error getting abs path of input")?;
        let cache_path = if config.use_cache {
            gen_cache(&input_path)?
        } else {
            PathBuf::new()
        };
        let script_out = Command::new(python.as_ref())
            .arg(file_loc.as_ref())
            .env("DECOR_INPUT", &path)
            .env("DECOR_OUT", &self.global_ctx.args.out)
            .env("DECOR_OUT_DIR", outdir)
            .env("DECOR_EXPORTS", exports.iter().join(" "))
            .env("DECOR_CACHE", &cache_path)
            .env(
                "DECOR_COMPTIME",
                self.comptime.get().then_some("1").unwrap_or_default(),
            )
            .current_dir(dir.path())
            .args(&self.global_ctx.args.build_args)
            .output()?;
        let (status, stdout, stderr) = (script_out.status, script_out.stdout, script_out.stderr);
        if cache_path != Path::new("")
            && fs::read_dir(&cache_path)
                .context("error reading cache dir")?
                .count()
                == 0
        {
            fs::remove_dir(&cache_path).context("error removing cache dir - should be empty")?;
        }

        if !status.success() {
            bail!(
                "failed to compile to WebAssembly:\n{}\nwith stdout:\n{}",
                str::from_utf8(&stderr)?,
                str::from_utf8(&stdout)?,
            );
        }

        let stdout = String::from_utf8(stdout).context("error converting script out to utf-8")?;

        spinner.finish(
            FinishLog::default()
                .with_main_message("WebAssembly")
                .with_sub_message(format!("{lang}{}", {
                    let mut args = String::new();
                    if !self.global_ctx.args.build_args.is_empty() {
                        args.push('`');
                        args.push_str(&self.global_ctx.args.build_args.join(" "));
                        args.insert_str(0, " `");
                        args.push('`');
                    }
                    args
                }))
                .with_file(&self.global_ctx.args.out)
                .enable_color(self.global_ctx.args.color)
                .to_string(),
        );

        let wasm_files = fs::read_dir(&self.global_ctx.args.out)?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| matches!(path.extension(), Some(ext) if ext == OsStr::new("wasm")))
            .collect_vec();

        if let Some(opt) = self.global_ctx.args.optimize {
            for path in &wasm_files {
                let spinner = Spinner::new(format!("Optimizing WebAssembly ({opt})..."));
                optimize(path, opt, &config.features).context("problem optimizing WebAssembly")?;
                spinner.finish(
                    FinishLog::default()
                        .with_main_message("optimized WebAssembly")
                        .with_sub_message(opt.to_string())
                        .with_file(path)
                        .enable_color(self.global_ctx.args.color)
                        .to_string(),
                );
            }
        }

        if self.global_ctx.args.strip {
            for path in &wasm_files {
                let spinner = Spinner::new("Stripping WebAssembly...");
                strip(path).context("problem stripping WebAssembly binary")?;
                spinner.finish(
                    FinishLog::default()
                        .with_main_message("stripped WebAssembly")
                        .with_file(path)
                        .enable_color(self.global_ctx.args.color)
                        .to_string(),
                );
            }
        }

        Ok(stdout)
    }

    fn compile_comptime(&self, info: CodeInfo) -> Result<JsEnv> {
        self.comptime.set(true);
        self.compile(info)?;
        self.comptime.set(false);

        let outdir =
            fs::canonicalize(&self.global_ctx.args.out).expect("outdir should have been created");
        let wasm_path = fs::read_dir(&outdir)?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .find(|path| matches!(path.extension(), Some(ext) if ext == OsStr::new("wasm")))
            .context("no WebAssembly file outputted from static compiler")?;

        // Run wasi module
        let (stdout, _stderr) = {
            let engine = Engine::default();
            let mut linker = Linker::new(&engine);
            wasmtime_wasi::add_to_linker(&mut linker, |s| s).unwrap();
            let stdout = WritePipe::new_in_memory();
            let stderr = WritePipe::new_in_memory();
            let wasi = WasiCtxBuilder::new()
                .stdout(Box::new(stdout.clone()))
                .stderr(Box::new(stderr.clone()))
                .build();
            let mut store = Store::new(&engine, wasi);
            let module = Module::from_file(&engine, wasm_path)?;
            linker.module(&mut store, "", &module)?;
            linker
                .get_default(&mut store, "")?
                .typed::<(), ()>(&store)?
                .call(&mut store, ())?;
            // Dropped so stdout and stderr can be acquired
            drop(store);
            (
                stdout.try_into_inner().unwrap().into_inner(),
                stderr.try_into_inner().unwrap().into_inner(),
            )
        };

        fs::remove_dir_all(outdir).context("error removing outdir")?;

        let out = serde_json::from_slice::<HashMap<String, serde_json::Value>>(&stdout)
            .context("error deserializing static code block stdout")?;

        Ok(out
            .into_iter()
            .map(|(name, value)| JsDecl {
                name,
                value: value.to_string(),
            })
            .collect())
    }
}

impl CodeExecutor for MainCompiler<'_> {
    fn execute(&self, code: &Code) -> Result<decorous_frontend::JsEnv> {
        self.comptime.set(true);
        let info = CodeInfo {
            lang: code.lang,
            body: code.body,
            exports: &[],
        };
        self.compile(info)?;
        self.comptime.set(false);

        let outdir =
            fs::canonicalize(&self.global_ctx.args.out).expect("outdir should have been created");
        let wasm_path = fs::read_dir(&outdir)?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .find(|path| matches!(path.extension(), Some(ext) if ext == OsStr::new("wasm")))
            .context("no WebAssembly file outputted from static compiler")?;

        // Run wasi module
        let (stdout, _stderr) = {
            let engine = Engine::default();
            let mut linker = Linker::new(&engine);
            wasmtime_wasi::add_to_linker(&mut linker, |s| s).unwrap();
            let stdout = WritePipe::new_in_memory();
            let stderr = WritePipe::new_in_memory();
            let wasi = WasiCtxBuilder::new()
                .stdout(Box::new(stdout.clone()))
                .stderr(Box::new(stderr.clone()))
                .build();
            let mut store = Store::new(&engine, wasi);
            let module = Module::from_file(&engine, wasm_path)?;
            linker.module(&mut store, "", &module)?;
            linker
                .get_default(&mut store, "")?
                .typed::<(), ()>(&store)?
                .call(&mut store, ())?;
            // Dropped so stdout and stderr can be acquired
            drop(store);
            (
                stdout.try_into_inner().unwrap().into_inner(),
                stderr.try_into_inner().unwrap().into_inner(),
            )
        };

        fs::remove_dir_all(outdir).context("error removing outdir")?;

        let out = serde_json::from_slice::<HashMap<String, serde_json::Value>>(&stdout)
            .context("error deserializing static code block stdout")?;

        Ok(out
            .into_iter()
            .map(|(name, value)| decorous_frontend::JsDecl {
                name,
                value: value.to_string(),
            })
            .collect())
    }
}

fn strip(file: impl AsRef<Path>) -> Result<()> {
    let mut module = walrus::Module::from_file(&file)?;
    let to_remove = module.customs.iter().map(|(id, _)| id).collect_vec();
    for id in to_remove {
        module.customs.delete(id);
    }
    module.emit_wasm_file(file)?;

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
