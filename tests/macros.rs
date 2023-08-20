/// Sets up a decorous test with an automatic temporary directory created and
/// cleaned up.
#[allow(unused_macros)]
macro_rules! decor_test {
    ($name:ident, $input:expr, $func:expr, $subcmd:expr) => {
        #[test]
        fn $name() {
            use ::std::{fs::File, io::Write};

            let mut dir = TempDir::new(stringify!($name)).expect("could not create temp dir");
            let mut f =
                File::create(dir.path().join("input.decor")).expect("could not create temp file");
            f.write_all($input.as_bytes())
                .expect("could not write to temp file");
            drop(f);
            let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
            cmd.current_dir(dir.path());
            cmd.arg($subcmd);
            if $subcmd == "build" {
                cmd.arg("input.decor");
            }
            $func(&mut dir, cmd);
            dir.close().expect("could not close temp dir");
        }
    };
    ($name:ident, $input:expr, $func:expr) => {
        decor_test!($name, $input, $func, "build");
    };
}

#[allow(unused_macros)]
macro_rules! decor_test_multiple {
    ($name:ident, $input:expr, $compare:expr, $($func_name:ident: $func:expr),+) => {
        #[test]
        fn $name() {
            use ::std::{fs::File, io::Write};

            $(
                let $func_name = {
                    let mut dir = TempDir::new(concat!(stringify!($name), stringify!($func_name))).expect("could not create temp dir");
                    let mut f = File::create(dir.path().join("input.decor"))
                        .expect("could not create temp file");
                    f.write_all($input.as_bytes())
                        .expect("could not write to temp file");
                    drop(f);
                    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
                    cmd.current_dir(dir.path());
                    cmd.arg("build").arg("input.decor");
                    let res = $func(&mut dir, cmd);
                    dir.close().expect("could not close temp dir");
                    res
                };
             )+

            $compare($($func_name),+);
        }
    };
}

#[allow(unused_imports)]
pub(crate) use decor_test;
#[allow(unused_imports)]
pub(crate) use decor_test_multiple;

/// Takes a snapshot of the current directory.
#[allow(unused_macros)]
macro_rules! assert_all {
    ($dir:expr$(, ignore:$ignore:expr)?) => {
        use ::std::{fmt::Write, fs, io::Read, path::Path};
        use ::itertools::Itertools;
        fn __write_dir(dir: impl AsRef<Path>, out: &mut String) {
            for path in fs::read_dir(dir)
                .expect("error reading dir")
                .filter_map(|p| match p {
                    Ok(entry) => Some(entry.path()),
                    Err(_) => None,
                })
                .sorted()
            {
                let name = path.file_name().unwrap().to_string_lossy();

                $(
                    if $ignore.iter().any(|p| path.ends_with(p)) {
                        writeln!(out, "\n---{}---\nIGNORED", name).expect("error writing to String");
                        continue;
                    }
                 )?

                if !path.is_file() || path.is_symlink() {
                    __write_dir(&path, out);
                    continue;
                }
                writeln!(out, "\n---{}---", name).expect("error writing to String");
                let mut f = File::open(path).expect("error opening file");
                if let Err(err) = f.read_to_string(out) {
                    writeln!(out, "COULD NOT BE READ: {err}").expect("error writing to String");
                }
            }
        }

        let mut all = String::new();
        __write_dir($dir, &mut all);
        insta::assert_snapshot!(all);
    };
}

#[allow(unused_imports)]
pub(crate) use assert_all;
