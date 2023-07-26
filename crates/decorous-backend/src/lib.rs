pub(crate) mod codegen_utils;
pub mod css_render;
pub mod dom_render;
pub mod prerender;

use decorous_frontend::Component;
use std::io;

#[derive(Debug)]
pub struct Metadata<'name> {
    pub name: &'name str,
}

pub trait RenderBackend {
    fn render<T: io::Write>(
        out: &mut T,
        component: &Component,
        metadata: &Metadata,
    ) -> io::Result<()>;
}

pub fn render<B, T>(component: &Component, out: &mut T, metadata: &Metadata) -> io::Result<()>
where
    T: io::Write,
    B: RenderBackend,
{
    <B as RenderBackend>::render(out, component, metadata)
}
