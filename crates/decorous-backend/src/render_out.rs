use std::{fmt, io};

macro_rules! write_fmt {
    ($name:ident, $method:ident) => {
        fn $name(&mut self, fmt: fmt::Arguments<'_>) -> io::Result<()> {
            struct Adapter<'a, T: ?Sized + 'a> {
                inner: &'a mut T,
                error: io::Result<()>,
            }

            impl<T: RenderOut + ?Sized> fmt::Write for Adapter<'_, T> {
                fn write_str(&mut self, s: &str) -> fmt::Result {
                    match self.inner.$method(s.as_bytes()) {
                        Ok(()) => Ok(()),
                        Err(e) => {
                            self.error = Err(e);
                            Err(fmt::Error)
                        }
                    }
                }
            }

            let mut output = Adapter {
                inner: self,
                error: Ok(()),
            };
            match fmt::write(&mut output, fmt) {
                Ok(()) => {
                    use ::std::fmt::Write;
                    match output.write_str("\n") {
                        Ok(()) => Ok(()),
                        Err(..) => {
                            if output.error.is_err() {
                                output.error
                            } else {
                                Err(io::Error::new(io::ErrorKind::Other, "formatter error"))
                            }
                        }
                    }
                }
                Err(..) => {
                    if output.error.is_err() {
                        output.error
                    } else {
                        Err(io::Error::new(io::ErrorKind::Other, "formatter error"))
                    }
                }
            }
        }
    };
}

pub trait RenderOut {
    fn write_js(&mut self, buf: &[u8]) -> io::Result<()>;
    fn write_html(&mut self, buf: &[u8]) -> io::Result<()>;
    fn write_css(&mut self, buf: &[u8]) -> io::Result<()>;

    fn js_handle(&mut self) -> &mut dyn io::Write;

    write_fmt!(write_js_fmt, write_js);
    write_fmt!(write_css_fmt, write_css);
    write_fmt!(write_html_fmt, write_html);
}

impl<T> RenderOut for &mut T
where
    T: RenderOut,
{
    fn write_js(&mut self, buf: &[u8]) -> io::Result<()> {
        (*self).write_js(buf)
    }

    fn write_css(&mut self, buf: &[u8]) -> io::Result<()> {
        (*self).write_css(buf)
    }

    fn write_html(&mut self, buf: &[u8]) -> io::Result<()> {
        (*self).write_html(buf)
    }

    fn js_handle(&mut self) -> &mut dyn io::Write {
        (*self).js_handle()
    }
}

pub struct JsFile<T>(T)
where
    T: io::Write;

impl<T> JsFile<T>
where
    T: io::Write,
{
    pub fn new(inner: T) -> Self {
        Self(inner)
    }
}

impl<T> RenderOut for JsFile<T>
where
    T: io::Write,
{
    fn js_handle(&mut self) -> &mut dyn io::Write {
        &mut self.0
    }

    fn write_css(&mut self, _buf: &[u8]) -> io::Result<()> {
        panic!("cannot write css to js-only file")
    }

    fn write_html(&mut self, _buf: &[u8]) -> io::Result<()> {
        panic!("cannot write html to js-only file")
    }

    fn write_js(&mut self, buf: &[u8]) -> io::Result<()> {
        self.0.write_all(buf)
    }
}

macro_rules! write_js {
    ($out:expr, $($arg:tt)*) => {
        $out.write_js_fmt(format_args!($($arg)*))
    };
}

pub(crate) use write_js;

macro_rules! write_html {
    ($out:expr, $($arg:tt)*) => {
        $out.write_html_fmt(format_args!($($arg)*))
    };
}

pub(crate) use write_html;
