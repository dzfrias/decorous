mod dep_analysis;
mod isolate_css;
mod run_static;

use crate::Component;
pub use dep_analysis::*;
pub use isolate_css::*;
pub use run_static::*;

pub trait Pass {
    fn run(self, component: &mut Component);
}
