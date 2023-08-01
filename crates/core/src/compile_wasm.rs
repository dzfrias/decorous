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
use which::which;

use crate::config::{Config, ScriptOrFile};

/// The core of WASM compiling. Beware! This function has a lot of side effects...
pub fn compile_wasm<'a>(
    lang: &'a str,
    body: &'a str,
    name: &'a str,
    out_name: &str,
    build_args: &[(String, String)],
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
                let mut f = File::create("__tmp.py")?;
                f.write_all(script.as_bytes())?;
            }
            defer! {
                fs::remove_file("__tmp.py").expect("error removing \"__tmp.py\"! Remove it manually!");
            }
            let build_args = {
                let args =
                    build_args
                        .iter()
                        .find_map(|(l, args)| if l == lang { Some(args.as_str()) } else { None });
                if let Some(args) = args {
                    shlex::split(args).with_context(|| {
                        format!("error parsing build args for language: {}", lang)
                    })?
                } else {
                    vec![]
                }
            };
            let python = {
                match which("python") {
                    Ok(bin) => bin,
                    Err(which::Error::CannotFindBinaryPath) => match which("python3") {
                        Ok(bin) => bin,
                        Err(_) => bail!("python not found in PATH! Make sure to install it!"),
                    },
                    Err(_) => bail!("python not found in PATH! Make sure to install it!"),
                }
            };
            let out = Command::new(python)
                .arg("__tmp.py")
                .arg(&path)
                .arg(out_name)
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

    Ok(String::from_utf8(stdout)?)
}
