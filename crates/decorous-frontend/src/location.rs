use nom_locate::LocatedSpan;

/// Represents a location with respect to an input string. Everything is positioned based on
/// **utf-8** character lengths, **not** code points.
#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub struct Location {
    offset: usize,
    length: usize,
    line: u32,
    column: usize,
}

impl Location {
    pub fn from_spans<'a>(span1: LocatedSpan<&'a str>, span2: LocatedSpan<&'a str>) -> Self {
        Self {
            offset: span1.location_offset(),
            length: span2.location_offset() - span1.location_offset(),
            line: span1.location_line(),
            column: span1.get_column(),
        }
    }

    /// Constructs a new location from a span. The only information needed here is the offset
    /// of where the location should start. Using this constructor, the length is always 1.
    ///
    /// # Panics
    ///
    /// Panics if the offset is greater than the length of the source code.
    pub fn from_source(offset: usize, source: &str) -> Self {
        if offset > source.len() {
            panic!("offset should not be greater than source length");
        }
        let mut line = 1;
        let mut col = 0;
        for b in source.as_bytes().iter().take(offset) {
            col += 1;
            const NEWLINE: u8 = 0x0A;
            if *b == NEWLINE {
                line += 1;
                col = 0;
            }
        }
        Self {
            offset,
            length: 1,
            line,
            column: col,
        }
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn length(&self) -> usize {
        self.length
    }

    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn column(&self) -> usize {
        self.column
    }
}

impl<'a> From<LocatedSpan<&'a str>> for Location {
    fn from(span: LocatedSpan<&'a str>) -> Self {
        Self {
            offset: span.location_offset(),
            length: 1,
            line: span.location_line(),
            column: span.get_column(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_retrieve_location_from_just_offset() {
        let offset = 14;
        let source = "hello world\nhi";
        assert_eq!(
            Location {
                column: 2,
                line: 2,
                offset: 14,
                length: 1
            },
            Location::from_source(offset, source)
        );
    }
}
