use std::{
    fs::{self, File},
    io::{self, BufWriter},
    path::{Path, PathBuf},
};

use decorous_backend::{dom_render::render, Options, UseInfo, UseResolver};
use decorous_frontend::{parse_with_preprocessor, Component};

use crate::{
    build::{compile_wasm::MainCompiler, preprocessor::Preproc},
    cli::Build,
    config::Config,
};

#[derive(Debug)]
pub struct Resolver<'a> {
    pub config: &'a Config,
    pub args: &'a Build,
    pub enable_color: bool,
    pub compiler: &'a MainCompiler<'a>,
}

impl UseResolver for Resolver<'_> {
    fn resolve(&self, path: &Path) -> io::Result<UseInfo> {
        let contents = fs::read_to_string(path)?;
        // TODO: Error handling. Make everything a report
        let ast = parse_with_preprocessor(&contents, &Preproc::new(self.config, self.enable_color))
            .unwrap();
        let component = Component::new(ast);

        let stem = path.file_stem().unwrap().to_string_lossy();
        let name: PathBuf = format!("{}_{stem}.mjs", self.args.out).into();
        let mut f = BufWriter::new(File::create(&name)?);
        render(
            &component,
            &mut f,
            &Options {
                name: &stem,
                modularize: true,
                wasm_compiler: self.compiler,
                use_resolver: self,
            },
        )?;

        Ok(UseInfo { loc: name })
    }
}
