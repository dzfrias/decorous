use decorous_errors::DynErrStream;

use crate::{cli::Build, config::Config};

pub struct GlobalCtx<'ctx> {
    pub config: &'ctx Config,
    pub args: &'ctx Build,
    pub errs: DynErrStream<'ctx>,
}
