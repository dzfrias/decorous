use std::fmt::{self, Display};

use bitflags::bitflags;

#[derive(Debug, Clone, PartialEq, Copy, Default)]
pub struct Style {
    fg: Option<Color>,
    bg: Option<Color>,

    modifiers: Modifiers,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Color {
    Reset,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    Default,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Rgb(u8, u8, u8),
}

bitflags! {
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Modifiers: u32 {
        const BOLD = 0b00000001;
        const ITALIC = 0b00000010;
        const DIMMED = 0b00000100;
        const UNDERLINED = 0b00001000;
        const BLINKING = 0b00010000;
        const REVERSED = 0b00100000;
        const HIDDEN = 0b01000000;
        const STRUCKTHROUGH = 0b10000000;
    }
}

impl Style {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset() -> Self {
        Self {
            fg: Some(Color::Reset),
            bg: Some(Color::Reset),
            modifiers: Modifiers::empty(),
        }
    }

    pub fn fg(mut self, fg: Color) -> Self {
        self.fg = Some(fg);
        self
    }

    pub fn bg(mut self, bg: Color) -> Self {
        self.bg = Some(bg);
        self
    }

    pub fn modifiers(mut self, modifiers: Modifiers) -> Self {
        self.modifiers = modifiers;
        self
    }
}

impl Display for Style {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.fg.is_none() && self.bg.is_none() && self.modifiers.is_empty() {
            return Ok(());
        }

        write!(f, "\x1b[")?;
        if let Some(fg) = self.fg {
            write!(f, "{}", fg.ansi_fg_code())?;
        }
        if let Some(bg) = self.bg {
            if self.fg.is_some() {
                write!(f, ";")?;
            }
            write!(f, "{}", bg.ansi_bg_code())?;
        }

        if !self.modifiers.is_empty() && (self.bg.is_some() || self.fg.is_some()) {
            write!(f, ";")?;
        }

        let mut mods = self.modifiers.into_iter();
        if let Some(m) = mods.next() {
            let code = m
                .try_to_ansi_code()
                .expect("bitflags iter should only yield single flags");
            write!(f, "{code}")?;
        }
        for m in mods {
            let code = m
                .try_to_ansi_code()
                .expect("bitflags iter should only yield single flags");
            write!(f, ";{code}")?;
        }

        write!(f, "m")
    }
}

impl Color {
    fn ansi_fg_code(&self) -> u8 {
        match self {
            Color::Reset => 0,
            Color::Black => 30,
            Color::Red => 31,
            Color::Green => 32,
            Color::Yellow => 33,
            Color::Blue => 34,
            Color::Magenta => 35,
            Color::Cyan => 36,
            Color::White => 37,
            Color::Rgb(_, _, _) => 38,
            Color::Default => 39,
            Color::BrightBlack => 90,
            Color::BrightRed => 91,
            Color::BrightGreen => 92,
            Color::BrightYellow => 93,
            Color::BrightBlue => 94,
            Color::BrightMagenta => 95,
            Color::BrightCyan => 96,
            Color::BrightWhite => 97,
        }
    }

    fn ansi_bg_code(&self) -> u8 {
        if matches!(self, Color::Reset) {
            return 0;
        }
        self.ansi_fg_code() + 10
    }
}

impl Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\x1b[{}m", self.ansi_fg_code())
    }
}

impl Modifiers {
    fn try_to_ansi_code(&self) -> Result<u8, u32> {
        let val = match self {
            &Self::BOLD => 1,
            &Self::DIMMED => 2,
            &Self::ITALIC => 3,
            &Self::UNDERLINED => 4,
            &Self::BLINKING => 5,
            &Self::REVERSED => 7,
            &Self::HIDDEN => 8,
            &Self::STRUCKTHROUGH => 9,

            _ => return Err(self.bits()),
        };
        Ok(val)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_strings_have_no_trailing_semicolons() {
        let styles = [
            Style::default().fg(Color::Red),
            Style::default().fg(Color::Red).bg(Color::White),
            Style::default().bg(Color::Red),
            Style::default().modifiers(Modifiers::BOLD | Modifiers::ITALIC),
            Style::default()
                .fg(Color::Red)
                .modifiers(Modifiers::BOLD | Modifiers::ITALIC),
        ];
        let expecteds = [
            "\x1b[31m",
            "\x1b[31;47m",
            "\x1b[41m",
            "\x1b[1;3m",
            "\x1b[31;1;3m",
        ];

        for (style, expected) in styles.into_iter().zip(expecteds) {
            assert_eq!(expected, style.to_string().as_str())
        }
    }
}
