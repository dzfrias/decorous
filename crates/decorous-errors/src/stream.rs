use std::{cell::RefCell, io::Write, rc::Rc};

use crate::{Diagnostic, Severity};

#[derive(Debug, Clone)]
pub struct Source<'src> {
    pub name: String,
    pub src: &'src str,
}

pub struct ErrStreamInner<'src, W> {
    source: Source<'src>,
    inner: RefCell<W>,
}

pub struct ErrStream<'src, W> {
    inner: Rc<ErrStreamInner<'src, W>>,
}

pub type DynErrStream<'src> = ErrStream<'src, Box<dyn Write>>;

impl<W> Clone for ErrStream<'_, W> {
    fn clone(&self) -> Self {
        Self {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<'src, W> ErrStream<'src, W>
where
    W: Write,
{
    pub fn new(writer: W, source: Source<'src>) -> Self {
        Self {
            inner: ErrStreamInner::new(writer, source).into(),
        }
    }

    pub fn emit(&self, diagnostic: Diagnostic) {
        self.inner.emit(diagnostic);
    }
}

impl<'src, W: Write> ErrStreamInner<'src, W> {
    pub fn new(writer: W, source: Source<'src>) -> Self {
        Self {
            inner: writer.into(),
            source,
        }
    }

    pub fn emit(&self, diagnostic: Diagnostic) {
        let severity = match diagnostic.severity {
            Severity::Error => ariadne::ReportKind::Error,
            Severity::Warning => ariadne::ReportKind::Warning,
        };
        let mut builder =
            ariadne::Report::build(severity, self.source.name.as_str(), diagnostic.offset)
                .with_message(&diagnostic.msg);

        if let Some(note) = diagnostic.note.as_ref() {
            builder.set_note(note);
        }

        for helper in &diagnostic.helpers {
            builder.add_label(
                ariadne::Label::new((self.source.name.as_str(), helper.span.clone()))
                    .with_message(&helper.msg),
            );
        }

        let report = builder.finish();
        let mut out = vec![];
        report
            .write(
                (
                    self.source.name.as_str(),
                    ariadne::Source::from(self.source.src),
                ),
                &mut out,
            )
            .expect("in memory write should not fail");
        let _ = self.inner.borrow_mut().write_all(&out);
    }
}
