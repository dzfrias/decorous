#[derive(Debug, PartialEq, Hash)]
pub struct Span<'a> {
    text: &'a str,
    start: usize,
}

impl<'a> Span<'a> {
    pub fn new(text: &'a str, offset: usize) -> Self {
        Self {
            text,
            start: offset,
        }
    }

    pub fn start(&self) -> usize {
        self.start
    }

    pub fn end(&self) -> usize {
        self.start + self.text().len()
    }

    pub fn text(&self) -> &'a str {
        self.text
    }
}
