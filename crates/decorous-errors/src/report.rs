use std::{borrow::Cow, ops::Range};

#[derive(Debug, Clone)]
pub struct Report {
    diagnostics: Vec<Diagnostic>,
}

impl Report {
    pub fn new(diagnostics: Vec<Diagnostic>) -> Self {
        Self { diagnostics }
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        self.diagnostics.as_ref()
    }

    pub fn add_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub msg: Cow<'static, str>,
    pub severity: Severity,
    pub helpers: Vec<Helper>,
    pub offset: usize,
    pub note: Option<Cow<'static, str>>,
}

#[derive(Debug, Clone)]
pub struct Helper {
    pub msg: Cow<'static, str>,
    pub span: Range<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Debug)]
pub struct DiagnosticBuilder {
    msg: Cow<'static, str>,
    severity: Severity,
    offset: usize,
    helpers: Vec<Helper>,
    note: Option<Cow<'static, str>>,
}

impl From<Severity> for ariadne::ReportKind<'_> {
    fn from(value: Severity) -> Self {
        match value {
            Severity::Warning => ariadne::ReportKind::Warning,
            Severity::Error => ariadne::ReportKind::Error,
        }
    }
}

impl Diagnostic {
    pub fn builder(
        msg: impl Into<Cow<'static, str>>,
        severity: Severity,
        offset: usize,
    ) -> DiagnosticBuilder {
        DiagnosticBuilder::new(msg, severity, offset)
    }
}

impl DiagnosticBuilder {
    pub fn new(msg: impl Into<Cow<'static, str>>, severity: Severity, offset: usize) -> Self {
        Self {
            msg: msg.into(),
            severity,
            offset,
            helpers: vec![],
            note: None,
        }
    }

    pub fn note(mut self, note: impl Into<Cow<'static, str>>) -> Self {
        self.note = Some(note.into());
        self
    }

    pub fn add_helper(mut self, helper: Helper) -> Self {
        self.helpers.push(helper);
        self
    }

    pub fn build(self) -> Diagnostic {
        Diagnostic {
            msg: self.msg,
            severity: self.severity,
            helpers: self.helpers,
            offset: self.offset,
            note: self.note,
        }
    }
}

impl Extend<Diagnostic> for Report {
    fn extend<T: IntoIterator<Item = Diagnostic>>(&mut self, iter: T) {
        self.diagnostics.extend(iter);
    }
}
