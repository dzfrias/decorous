use nom_locate::LocatedSpan;

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

    pub fn from_source(offset: usize, source: &str) -> Self {
        if offset > source.len() {
            panic!("offset should not be greater than source length");
        }
        let mut line = 1;
        let mut col = 0;
        for b in source.as_bytes().iter().take(offset) {
            col += 1;
            if *b == 0x0A {
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
    fn can_retrive_location_from_just_offset() {
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
