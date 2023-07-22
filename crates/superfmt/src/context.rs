use std::borrow::Cow;

#[derive(Default, Clone)]
pub struct Context {
    pub(super) starts_with: Cow<'static, str>,
    pub(super) ends_with: Cow<'static, str>,
    pub(super) prepend: Cow<'static, str>,
    pub(super) append: Cow<'static, str>,
}

#[derive(Default)]
pub struct ContextBuilder {
    starts_with: Option<Cow<'static, str>>,
    prepend: Option<Cow<'static, str>>,
    ends_with: Option<Cow<'static, str>>,
    append: Option<Cow<'static, str>>,
}

impl ContextBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn starts_with<T: Into<Cow<'static, str>>>(mut self, starts_with: T) -> Self {
        self.starts_with = Some(starts_with.into());
        self
    }

    #[must_use]
    pub fn ends_with<T: Into<Cow<'static, str>>>(mut self, ends_with: T) -> Self {
        self.ends_with = Some(ends_with.into());
        self
    }

    #[must_use]
    pub fn prepend<T: Into<Cow<'static, str>>>(mut self, prepend: T) -> Self {
        self.prepend = Some(prepend.into());
        self
    }

    #[must_use]
    pub fn append<T: Into<Cow<'static, str>>>(mut self, append: T) -> Self {
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
