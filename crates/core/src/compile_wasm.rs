use std::{
    borrow::Cow,
    env,
    ffi::OsStr,
    fs::{self, File},
    io::{self, Write},
    path::PathBuf,
    process::Command,
    str,
};

use anyhow::{bail, Context, Result};
use scopeguard::defer;

use crate::config::{Config, ScriptOrFile};

/// The core of WASM compiling. Beware! This function has a lot of side effects...
pub fn compile_wasm<'a>(
    lang: &'a str,
    body: &'a str,
    name: &'a str,
    out_name: &str,
    config: &Config,
) -> Result<String> {
    let config = config
        .compilers
        .get(lang)
        .with_context(|| format!("unsupported language: {lang}"))?;
    let path: PathBuf = format!("{name}.{}", config.ext_override.unwrap_or(lang)).into();

    {
        let mut f = File::create(&path)?;
        f.write_all(body.as_bytes())?;
    }

    let _guard = scopeguard::guard(&path, |p| {
        fs::remove_file(p)
            .unwrap_or_else(|_| panic!("error removing \"{}\"! Remove it manually!", p.display()));
    });

    match fs::create_dir("out") {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
        Err(err) => bail!(err),
    }
    let shell = env::var_os("SHELL")
        .map(Cow::Owned)
        .unwrap_or(Cow::Borrowed(OsStr::new("bash")));
    let (status, stdout, stderr) = match &config.script {
        ScriptOrFile::File(file) => {
            let out = Command::new(shell)
                .arg(file)
                .arg(&path)
                .arg(out_name)
                .output()?;
            (out.status, out.stdout, out.stderr)
        }
        ScriptOrFile::Script(script) => {
            {
                let mut f = File::create("__tmp.sh")?;
                f.write_all(script.as_bytes())?;
            }
            defer! {
                fs::remove_file("__tmp.sh").expect("error removing \"__tmp.sh\"! Remove it manually!");
            }
            let out = Command::new(shell)
                .arg("__tmp.sh")
                .arg(&path)
                .arg(out_name)
                .output()?;
            (out.status, out.stdout, out.stderr)
        }
    };
    if !status.success() {
        bail!(
            "failed to compile to WebAssembly:\n{}",
            str::from_utf8(&stderr)?
        );
    }

    Ok(String::from_utf8(stdout)?)
}
