use rslint_parser::SyntaxNode;
use thiserror::Error;

use crate::{ast::Code, css::ast::Css};

macro_rules! setter {
    ($name:ident, $field:ident: $field_type:ty) => {
        pub fn $name(&mut self, $field: $field_type) -> Result<(), AlreadySetError> {
            if self.$field.is_some() {
                return Err(AlreadySetError);
            }

            self.$field = Some($field);

            Ok(())
        }
    };
}

#[derive(Debug, Error)]
#[error("field already set")]
pub struct AlreadySetError;

#[derive(Debug, Default)]
pub struct CodeBlocks<'ast> {
    script: Option<SyntaxNode>,
    css: Option<Css>,
    wasm: Option<Code<'ast>>,
    comptime: Option<Code<'ast>>,
}

impl<'ast> CodeBlocks<'ast> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn into_parts(
        self,
    ) -> (
        Option<SyntaxNode>,
        Option<Css>,
        Option<Code<'ast>>,
        Option<Code<'ast>>,
    ) {
        (self.script, self.css, self.wasm, self.comptime)
    }

    setter!(set_script, script: SyntaxNode);
    setter!(set_css, css: Css);
    setter!(set_wasm, wasm: Code<'ast>);
    setter!(set_static_wasm, comptime: Code<'ast>);
}
