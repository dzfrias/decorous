pub(crate) mod codegen_utils;
pub mod dom_render;
pub mod prerender;

use decorous_frontend::Component;
use std::io;

pub trait RenderBackend {
    fn render<T: io::Write>(out: &mut T, component: &Component) -> io::Result<()>;
}

pub fn render<B, T>(component: &Component, out: &mut T) -> io::Result<()>
where
    T: io::Write,
    B: RenderBackend,
{
    <B as RenderBackend>::render(out, component)
}
