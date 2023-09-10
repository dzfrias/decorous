mod span;

use arrayvec::ArrayVec;
use itertools::{EitherOrBoth, Itertools};
pub use span::Span;
use std::{collections::VecDeque, ops::Deref, str::Chars};

#[derive(Debug, Clone, PartialEq)]
pub struct Peeked<const SIZE: usize>(ArrayVec<char, SIZE>);

impl<const SIZE: usize> Deref for Peeked<SIZE> {
    type Target = [char];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
pub struct Harpoon<'a> {
    source: &'a str,
    chars: Chars<'a>,

    peek_buf: VecDeque<char>,
    current: Option<char>,

    idx: usize,
}

impl<'a> Harpoon<'a> {
    pub fn new(input: &'a str) -> Self {
        let harpoon = Self {
            source: input,
            chars: input.chars(),
            current: None,
            peek_buf: VecDeque::new(),
            idx: 0,
        };
        harpoon
    }

    pub fn consume(&mut self) -> Option<char> {
        if let Some(next) = self.peek_buf.pop_front() {
            self.idx += next.len_utf8();
            self.current = Some(next);
            return self.current;
        }

        let next = self.chars.next();
        if let Some(next) = next {
            self.idx += next.len_utf8();
        }
        self.current = next;
        self.current
    }

    pub fn current(&self) -> Option<char> {
        self.current
    }

    pub fn peek(&mut self) -> Option<char> {
        self.peek_n_const::<1>().first().copied()
    }

    pub fn peek_is(&mut self, expected: char) -> bool {
        self.peek().is_some_and(|c| c == expected)
    }

    pub fn peek_is_any(&mut self, expecteds: &str) -> bool {
        self.peek().is_some_and(|c| expecteds.contains(c))
    }

    pub fn peek_n_const<const N: usize>(&mut self) -> Peeked<N> {
        let mut arrvec = ArrayVec::new();
        let remaining = N.saturating_sub(self.peek_buf.len());
        for _ in 0..remaining {
            let Some(next) = self.chars.next() else {
                break;
            };
            self.peek_buf.push_back(next);
        }
        for i in self.peek_buf.iter().take(N) {
            arrvec.push(*i);
        }
        Peeked(arrvec)
    }

    pub fn peek_n(&mut self, n: usize) -> impl Iterator<Item = char> + '_ {
        let remaining = n.saturating_sub(self.peek_buf.len());
        for _ in 0..remaining {
            let Some(next) = self.chars.next() else {
                break;
            };
            self.peek_buf.push_back(next);
        }
        self.peek_buf.iter().take(n).cloned()
    }

    pub fn consume_while<F>(&mut self, mut f: F)
    where
        F: FnMut(char) -> bool,
    {
        while self.peek().is_some_and(&mut f) {
            self.consume();
        }
    }

    pub fn consume_until(&mut self, stopper: char) {
        self.consume_while(|c| c != stopper);
    }

    pub fn try_consume(&mut self, s: &str) -> bool {
        if self.peek_equals(s) {
            self.consume_n(s.chars().count());
            true
        } else {
            false
        }
    }

    pub fn peek_equals(&mut self, s: &str) -> bool {
        !self
            .peek_n(s.len())
            .zip_longest(s.chars())
            .any(|either| !matches!(either, EitherOrBoth::Both(a, b) if a == b))
    }

    pub fn consume_n(&mut self, n: usize) {
        for _ in 0..n {
            self.consume();
        }
    }

    pub fn offset(&self) -> usize {
        self.idx
    }

    pub fn harpoon<F>(&mut self, mut f: F) -> Span<'a>
    where
        F: FnMut(&mut Harpoon),
    {
        let start = self.idx;
        f(self);
        let t = &self.source[start..self.source.len() - self.source[self.idx..].len()];
        Span::new(t, start)
    }

    pub fn source(&self) -> &'a str {
        self.source
    }
}

impl Clone for Harpoon<'_> {
    fn clone(&self) -> Self {
        Self {
            source: self.source().clone(),
            chars: self.source()[self.offset()..].chars(),
            peek_buf: VecDeque::new(),
            current: self.current(),
            idx: self.offset(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peek_stops_at_length_of_peek_buffer() {
        let mut harpoon = Harpoon::new("1234");
        assert_eq!(Some('1'), harpoon.peek());
        assert_eq!(Some('1'), harpoon.peek());
        harpoon.consume();
        assert_eq!(&['2', '3', '4'], &harpoon.peek_n_const::<3>()[..]);
        assert_eq!(&['2', '3', '4'], &harpoon.peek_n_const::<3>()[..]);
        assert_eq!(&['2', '3', '4'], &harpoon.peek_n_const::<4>()[..]);
    }

    #[test]
    fn harpoon_is_right_exclusive() {
        let mut harpoon = Harpoon::new("1234");
        let span = harpoon.harpoon(|h| {
            assert_eq!(Some('1'), h.consume());
            assert_eq!(Some('2'), h.consume());
        });
        assert_eq!(0, span.start());
        assert_eq!(2, span.end());
        assert_eq!("12", span.text());
    }

    #[test]
    fn consume_while_doesnt_take_failing_char() {
        let mut harpoon = Harpoon::new("1234");
        harpoon.consume_while(|c| c != '4');
        assert_eq!(Some('4'), harpoon.consume());
    }

    #[test]
    fn try_consume_consumes_nothing_if_no_match() {
        let mut harpoon = Harpoon::new("1234");
        assert!(!harpoon.try_consume("33"));
        assert_eq!(Some('1'), harpoon.consume());
    }
}
