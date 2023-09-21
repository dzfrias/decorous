use std::{
    fs::{self, File},
    io::{self, BufWriter},
    path::{Path, PathBuf},
};

use anyhow::anyhow;
use decorous_backend::{
    dom_render::{CsrOptions, CsrRenderer},
    Ctx as RenderCtx, JsFile, RenderBackend, Result, UseInfo, UseResolver,
};
use decorous_errors::{ErrStream, Source};
use decorous_frontend::{Component, Ctx as ParseCtx, Parser};

use crate::build::{compile_wasm::MainCompiler, global_ctx::GlobalCtx, preprocessor::Preproc};

pub struct Resolver<'a> {
    pub global_ctx: &'a GlobalCtx<'a>,
    pub compiler: &'a MainCompiler<'a>,
}

impl UseResolver for Resolver<'_> {
    fn resolve(&self, path: &Path) -> Result<UseInfo> {
        let contents = fs::read_to_string(path)?;
        let stem = path.file_stem().unwrap().to_string_lossy();

        let preproc = Preproc::new(self.global_ctx.config, self.global_ctx.args.color);
        let executor = MainCompiler::new(self.global_ctx);
        let ctx = ParseCtx {
            preprocessor: &preproc,
            executor: &executor,
            errs: ErrStream::new(
                Box::new(io::stderr()),
                Source {
                    name: stem.to_string(),
                    src: &contents,
                },
            ),
        };
        let parser = Parser::new(&contents).with_ctx(ctx.clone());
        let ast = parser.parse().map_err(|err| anyhow!(err))?;
        let mut component = Component::new(ast, ctx);
        component.run_passes()?;

        let name: PathBuf = format!("{}_{stem}.mjs", self.global_ctx.args.out).into();
        let mut f = BufWriter::new(File::create(&name)?);
        let mut renderer = CsrRenderer::new();
        renderer.with_options(CsrOptions { modularize: true });
        renderer.render(
            &component,
            JsFile::new(&mut f),
            &RenderCtx {
                name: &stem,
                wasm_compiler: self.compiler,
                use_resolver: self,
                errs: self.global_ctx.errs.clone(),
                index_html: None,
            },
        )?;

        Ok(UseInfo { loc: name })
    }
}
