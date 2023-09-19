mod dep_analysis;
mod isolate_css;

use crate::Component;
pub use dep_analysis::*;
pub use isolate_css::*;

pub trait Pass {
    fn run(self, component: &mut Component);
}
