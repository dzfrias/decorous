use std::io::{self, Write};

use decorous_frontend::{ast::PreprocCss, css::ast::*, Component};
use superfmt::{ContextBuilder, Formatter};

use crate::{Metadata, RenderBackend};

pub struct CssRenderer;

impl RenderBackend for CssRenderer {
    fn render<T: io::Write>(
        out: &mut T,
        component: &Component,
        _metadata: &Metadata,
    ) -> io::Result<()> {
        match component.css() {
            Some(PreprocCss::Preproc(css)) => write!(out, "{css}"),
            Some(PreprocCss::NoPreproc(css)) => render(css, out, component),
            None => Ok(()),
        }
    }
}

fn render<T: io::Write>(css: &Css<'_>, out: &mut T, component: &Component) -> io::Result<()> {
    let mut formatter = Formatter::new(out);
    for rule in css.rules() {
        write_rule(rule, &mut formatter, component)?;
    }
    Ok(())
}

fn write_rule<T: io::Write>(
    rule: &Rule<'_>,
    formatter: &mut Formatter<'_, T>,
    component: &Component,
) -> io::Result<()> {
    match rule {
        Rule::At(at_rule) => {
            if let Some(contents) = at_rule.contents() {
                formatter
                    .write(format_args!(
                        "@{} {} ",
                        at_rule.name(),
                        at_rule.additional()
                    ))?
                    .begin_context(
                        ContextBuilder::new()
                            .starts_with("{\n")
                            .ends_with("}\n")
                            .prepend("  ")
                            .build(),
                    )?;
                for rule in contents {
                    write_rule(rule, formatter, component)?;
                }
                formatter.pop_ctx()?;
            } else {
                formatter.writeln(format_args!(
                    "@{} {};",
                    at_rule.name(),
                    at_rule.additional()
                ))?;
            }
        }
        Rule::Regular(regular) => {
            formatter.write(regular.selector())?.begin_context(
                ContextBuilder::default()
                    .prepend("  ")
                    .starts_with(" {\n")
                    .ends_with("}\n")
                    .build(),
            )?;
            for decl in regular.declarations() {
                write_decl(decl, formatter, component)?;
            }
            formatter.pop_ctx()?;
        }
    }

    Ok(())
}

fn write_decl<T: io::Write>(
    decl: &Declaration<'_>,
    f: &mut Formatter<'_, T>,
    component: &Component,
) -> io::Result<()> {
    f.write(format_args!("{}: ", decl.name()))?;
    for val in decl.values() {
        write_value(val, f, component)?;
    }
    f.write(";\n")?;

    Ok(())
}

fn write_value<T: io::Write>(
    value: &Value<'_>,
    out: &mut Formatter<'_, T>,
    component: &Component,
) -> io::Result<()> {
    match value {
        Value::Css(css) => write!(out, "{css}"),
        // TODO: In component, get all CSS mustaches and assign them unique ID.
        // This should be var(--decor-{id}). In codegen, use this to assign inline styles
        // to element. This would have to be done on the root element.
        Value::Mustache(node) => {
            write!(
                out,
                "var(--decor-{})",
                component
                    .declared_vars()
                    .css_mustaches()
                    .get(node)
                    .expect("all mustaches should be in css_mustaches variable")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use decorous_frontend::parse;

    use super::*;

    fn make_component(input: &str) -> Component {
        Component::new(parse(input).expect("should be valid input"))
    }

    #[test]
    fn mustaches_are_properly_turned_into_var_usages() {
        let mut out = vec![];
        let input = "---css body { color: {color}; } ---";
        let component = make_component(input);
        CssRenderer::render(&mut out, &component, &Metadata { name: "test" })
            .expect("render should not fail");
        insta::assert_snapshot!(String::from_utf8(out).unwrap());
    }
}
