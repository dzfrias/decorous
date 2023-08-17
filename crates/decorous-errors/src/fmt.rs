use std::io;

use crate::Report;

pub fn report(report: Report, src_name: &str, src: &str) -> io::Result<()> {
    for diagnostic in report.diagnostics() {
        let mut builder =
            ariadne::Report::build(diagnostic.severity.into(), src_name, diagnostic.offset)
                .with_message(&diagnostic.msg);

        if let Some(note) = diagnostic.note.as_ref() {
            builder.set_note(note);
        }

        for helper in &diagnostic.helpers {
            builder.add_label(
                ariadne::Label::new((src_name, helper.span.clone())).with_message(&helper.msg),
            )
        }

        let report = builder.finish();
        report.eprint((src_name, ariadne::Source::from(src)))?;
    }

    Ok(())
}
