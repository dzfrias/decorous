use std::ops::Range;

/// Represents a location with respect to an input string. Everything is positioned based on
/// **utf-8** character lengths, **not** code points.
#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub struct Location {
    offset: usize,
    length: usize,
}

impl Location {
    pub fn new(offset: usize, length: usize) -> Self {
        Self { offset, length }
    }

    /// Constructs a new location from a span. The only information needed here is the offset
    /// of where the location should start. Using this constructor, the length is always 1.
    ///
    /// # Panics
    ///
    /// Panics if the offset is greater than the length of the source code.
    pub fn from_source(offset: usize, _source: &str) -> Self {
        Self { offset, length: 1 }
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn length(&self) -> usize {
        self.length
    }
}

impl From<usize> for Location {
    fn from(value: usize) -> Self {
        Self {
            offset: value,
            length: 1,
        }
    }
}

impl From<Location> for Range<usize> {
    fn from(value: Location) -> Self {
        Self {
            start: value.offset(),
            end: value.offset() + value.length(),
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
                offset: 14,
                length: 1
            },
            Location::from_source(offset, source)
        );
    }
}
