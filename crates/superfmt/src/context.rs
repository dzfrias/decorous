#[derive(Debug)]
pub struct Context {
    pub(super) starts_with: &'static str,
    pub(super) ends_with: &'static str,
    pub(super) prepend: &'static str,
    pub(super) append: &'static str,
}

#[derive(Debug, Default)]
pub struct ContextBuilder {
    starts_with: Option<&'static str>,
    prepend: Option<&'static str>,
    ends_with: Option<&'static str>,
    append: Option<&'static str>,
}

impl ContextBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn starts_with(mut self, starts_with: &'static str) -> Self {
        self.starts_with = Some(starts_with.into());
        self
    }

    pub fn ends_with(mut self, ends_with: &'static str) -> Self {
        self.ends_with = Some(ends_with.into());
        self
    }

    pub fn prepend(mut self, prepend: &'static str) -> Self {
        self.prepend = Some(prepend.into());
        self
    }

    pub fn append(mut self, append: &'static str) -> Self {
        self.append = Some(append.into());
        self
    }

    pub fn build(self) -> Context {
        Context {
            starts_with: self.starts_with.unwrap_or_default(),
            prepend: self.prepend.unwrap_or_default(),
            ends_with: self.ends_with.unwrap_or_default(),
            append: self.append.unwrap_or_default(),
        }
    }
}
