use crate::Context;
use std::{
    fmt::Display,
    io::{self, Write},
};

pub struct Formatter<'a, T>
where
    T: Write,
{
    writer: &'a mut T,
    ctx: Vec<Context>,

    should_prepend: bool,
    ignore_next_append: bool,
}

impl<'a, T> Formatter<'a, T>
where
    T: Write,
{
    pub fn new(out: &'a mut T) -> Self {
        Self {
            writer: out,
            ctx: vec![],

            should_prepend: false,
            ignore_next_append: false,
        }
    }

    pub fn begin_context(&mut self, context: impl Into<Context>) -> io::Result<&mut Self> {
        let context = context.into();
        let starts_with = context.starts_with.to_string();
        self.ignore_next_append = true;
        write!(self, "{}", starts_with)?;
        self.ctx.push(context);
        if starts_with.ends_with('\n') || starts_with.is_empty() {
            self.should_prepend = true;
        }
        Ok(self)
    }

    pub fn writeln(&mut self, ln: impl Display) -> io::Result<&mut Self> {
        writeln!(self, "{ln}")?;
        Ok(self)
    }

    pub fn write(&mut self, display: impl Display) -> io::Result<&mut Self> {
        write!(self, "{display}")?;
        Ok(self)
    }

    pub fn write_with_context(
        &mut self,
        display: impl Display,
        context: impl Into<Context>,
    ) -> io::Result<&mut Self> {
        self.begin_context(context)?;
        self.write(display)?;
        self.pop_ctx()
    }

    pub fn writeln_with_context(
        &mut self,
        display: impl Display,
        context: impl Into<Context>,
    ) -> io::Result<&mut Self> {
        self.begin_context(context)?;
        self.writeln(display)?;
        self.pop_ctx()
    }

    pub fn pop_ctx(&mut self) -> io::Result<&mut Self> {
        if let Some(ctx) = self.ctx.pop() {
            self.ignore_next_append = true;
            write!(self, "{}", ctx.ends_with)?;
        }
        Ok(self)
    }

    pub fn write_all<I, U>(&mut self, iter: I, sep: &str) -> io::Result<&mut Self>
    where
        I: IntoIterator<Item = U>,
        U: Display,
    {
        let mut iter = iter.into_iter();
        if let Some(first) = iter.next() {
            write!(self, "{first}")?;
            for rest in iter {
                write!(self, "{sep}{rest}")?;
            }
        }
        Ok(self)
    }

    pub fn write_all_trailing<I, U>(&mut self, iter: I, sep: &str) -> io::Result<&mut Self>
    where
        I: IntoIterator<Item = U>,
        U: Display,
    {
        for i in iter {
            write!(self, "{i}{sep}")?;
        }
        Ok(self)
    }

    pub fn write_all_ln<I, U>(&mut self, iter: I, sep: &str) -> io::Result<&mut Self>
    where
        I: IntoIterator<Item = U>,
        U: Display,
    {
        self.write_all(iter, sep)?;
        writeln!(self)?;
        Ok(self)
    }
}

impl<'a, T> Write for Formatter<'a, T>
where
    T: Write,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.should_prepend {
            for ctx in &self.ctx {
                self.writer.write_all(ctx.prepend.to_string().as_bytes())?;
            }
            self.should_prepend = false;
        }
        if self.ctx.last().is_some() && buf.last().is_some_and(|u| u == &b'\n') {
            self.should_prepend = true;
        }

        if let Some(ctx) = self.ctx.last() {
            if buf.last().is_some_and(|u| u == &b'\n') && !self.ignore_next_append {
                self.writer.write_all(&buf[..buf.len() - 1])?;
                self.writer.write_all(ctx.append.to_string().as_bytes())?;
                self.writer.write_all(b"\n")?;

                return Ok(buf.len());
            }
        }

        self.ignore_next_append = false;

        let write = self.writer.write(buf)?;

        Ok(write)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}
