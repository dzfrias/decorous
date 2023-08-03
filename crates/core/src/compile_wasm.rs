use std::{
    fs::{self, File},
    io::{self, Write},
    path::PathBuf,
    process::Command,
    str,
};

use anyhow::{bail, Context, Error, Result};
use decorous_backend::{dom_render::DomRenderer, prerender::Prerenderer, CodeInfo, WasmCompiler};
use itertools::Itertools;
use scopeguard::defer;
use which::which;

use crate::config::{Config, ScriptOrFile};

#[derive(Debug)]
pub struct MainCompiler<'a> {
    config: &'a Config<'a>,
    build_args: &'a [(String, String)],
    out_name: &'a str,
}

impl<'a> MainCompiler<'a> {
    pub fn new(
        config: &'a Config<'a>,
        out_name: &'a str,
        build_args: &'a [(String, String)],
    ) -> Self {
        Self {
            config,
            build_args,
            out_name,
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
                let path: PathBuf = format!("__tmp.{}", config.ext_override.unwrap_or(lang)).into();

                {
                    let mut f = File::create(&path)?;
                    f.write_all(body.as_bytes())?;
                }

                let _guard = scopeguard::guard(&path, |p| {
                    fs::remove_file(p).unwrap_or_else(|_| {
                        panic!("error removing \"{}\"! Remove it manually!", p.display())
                    });
                });

                match fs::create_dir("out") {
                    Ok(()) => {}
                    Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(err) => bail!(err),
                }

                let python = get_python()?;
                let build_args = get_build_args(self.build_args, lang)?;

                let (status, stdout, stderr) = match &config.script {
                    ScriptOrFile::File(file) => {
                        let out = Command::new(python)
                            .arg(file)
                            .env("DECOR_INPUT", &path)
                            .env("DECOR_OUT", self.out_name)
                            .env("DECOR_EXPORTS", exports.iter().join(" "))
                            .args(&build_args)
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
                        let out = Command::new(python)
                            .arg("__tmp.py")
                            .env("DECOR_INPUT", &path)
                            .env("DECOR_OUT", self.out_name)
                            .env("DECOR_EXPORTS", exports.iter().join(" "))
                            .args(&build_args)
                            .output()?;
                        (out.status, out.stdout, out.stderr)
                    }
                };

                if !status.success() {
                    bail!(
                        "failed to compile to WebAssembly:\n{}\nwith stdout:\n{}",
                        str::from_utf8(&stderr)?,
                        str::from_utf8(&stdout)?,
                    );
                }

                out.write_all(&stdout)?;

                Ok(())
            }
        }
    };
}

compile_for!(DomRenderer);
compile_for!(Prerenderer);

fn get_build_args(build_args: &[(String, String)], lang: &str) -> Result<Vec<String>> {
    let args = build_args
        .iter()
        .find_map(|(l, args)| if l == lang { Some(args.as_str()) } else { None });
    Ok(if let Some(args) = args {
        shlex::split(args)
            .with_context(|| format!("error parsing build args for language: {}", lang))?
    } else {
        vec![]
    })
}

fn get_python() -> Result<PathBuf> {
    match which("python") {
        Ok(bin) => Ok(bin),
        Err(which::Error::CannotFindBinaryPath) => match which("python3") {
            Ok(bin) => Ok(bin),
            Err(_) => bail!("python not found in PATH! Make sure to install it!"),
        },
        Err(_) => bail!("python not found in PATH! Make sure to install it!"),
    }
}
