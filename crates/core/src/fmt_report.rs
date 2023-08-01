use std::io::{self, Write};

use decorous_frontend::{errors::Report, location::Location};
use itertools::Itertools;
use superfmt::{
    style::{Color, Modifiers, Style},
    Formatter,
};

pub fn fmt_report<T: io::Write>(
    input: &str,
    report: &Report<Location>,
    out: &mut T,
) -> io::Result<()> {
    let mut formatter = Formatter::new(out);
    for err in report.errors() {
        let lines = input.lines().enumerate();
        // Minus one because location_line is 1-indexed
        let line_no = err.fragment().line() - 1;

        formatter.writeln_with_context(format_args!("error: {}", err.err_type()), Color::Red)?;
        // Write the error description
        if let Some(help_line) = err
            .help()
            .and_then(|help| help.corresponding_line())
            .filter(|ln| ln < &(line_no + 1))
        {
            let (_, line) = lines
                .clone()
                .find(|(n, _)| *n as u32 == help_line - 1)
                .expect("should be in lines");
            write!(formatter, "{help_line}| {} ", line)?;
            formatter.writeln_with_context("<--- this line", Color::Yellow)?;
            if help_line + 1 != line_no + 1 {
                formatter.writeln("...")?;
            }
        }
        let (i, line) = lines
            .clone()
            .find_or_last(|(n, _)| (*n as u32) == line_no)
            .unwrap();

        writeln!(formatter, "{}| {line}", i + 1)?;
        // Plus one because line_no is 0 indexed, so we need to get the actual line number
        let line_no_len = count_digits(line_no + 1) as usize;
        let col = err.fragment().column() + line_no_len + 2;
        formatter.writeln_with_context(
            format_args!("{arrow:>col$}", arrow = "^"),
            Style::default()
                .fg(Color::Yellow)
                .modifiers(Modifiers::BOLD),
        )?;

        if let Some(help) = err.help() {
            formatter.writeln_with_context(format_args!("help: {help}"), Modifiers::BOLD)?;
        }
        writeln!(formatter)?;
    }
    Ok(())
}

fn count_digits(num: u32) -> u32 {
    num.checked_ilog10().unwrap_or(0) + 1
}
