use std::io::{self, Write};

use decorous_frontend::{css::ast::*, Component};
use itertools::Itertools;
use superfmt::{ContextBuilder, Formatter};

pub fn render_css<T: io::Write>(css: &Css, out: &mut T, component: &Component) -> io::Result<()> {
    let mut formatter = Formatter::new(out);
    for rule in &css.rules {
        write_rule(rule, &mut formatter, component)?;
    }
    Ok(())
}

fn write_rule<T: io::Write>(
    rule: &Rule,
    formatter: &mut Formatter<'_, T>,
    component: &Component,
) -> io::Result<()> {
    match rule {
        Rule::At(at_rule) => {
            if let Some(contents) = &at_rule.contents {
                formatter
                    .write(format_args!("@{} {} ", at_rule.name, at_rule.additional))?
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
                formatter.writeln(format_args!("@{} {};", at_rule.name, at_rule.additional))?;
            }
        }
        Rule::Regular(regular) => {
            formatter
                .write(regular.selector.iter().join(", "))?
                .begin_context(
                    ContextBuilder::default()
                        .prepend("  ")
                        .starts_with(" {\n")
                        .ends_with("}\n")
                        .build(),
                )?;
            for decl in &regular.declarations {
                write_decl(decl, formatter, component)?;
            }
            formatter.pop_ctx()?;
        }
    }

    Ok(())
}

fn write_decl<T: io::Write>(
    decl: &Declaration,
    f: &mut Formatter<'_, T>,
    component: &Component,
) -> io::Result<()> {
    f.write(format_args!("{}: ", decl.name))?;
    for val in &decl.values {
        write_value(val, f, component)?;
    }
    f.write(";\n")?;

    Ok(())
}

fn write_value<T: io::Write>(
    value: &Value,
    out: &mut Formatter<'_, T>,
    component: &Component,
) -> io::Result<()> {
    match value {
        Value::Css(css) => write!(out, "{css}"),
        Value::Mustache(node) => {
            write!(
                out,
                "var(--decor-{})",
                component
                    .declared_vars
                    .css_mustaches()
                    .get(node)
                    .expect("all mustaches should be in css_mustaches variable")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use decorous_errors::Source;
    use decorous_frontend::Parser;

    use super::*;

    fn make_component(input: &str) -> Component {
        let parser = Parser::new(input);
        let mut c = Component::new(
            parser.parse().expect("should be valid input"),
            decorous_frontend::Ctx {
                errs: decorous_errors::stderr(Source {
                    src: input,
                    name: "TEST".to_owned(),
                }),
                ..Default::default()
            },
        );
        c.run_passes().unwrap();
        c
    }

    #[test]
    fn mustaches_are_properly_turned_into_var_usages() {
        let mut out = vec![];
        let input = "---css body { color: {color}; } ---";
        let component = make_component(input);
        render_css(component.css.as_ref().unwrap(), &mut out, &component).unwrap();
        insta::assert_snapshot!(String::from_utf8(out).unwrap());
    }
}
