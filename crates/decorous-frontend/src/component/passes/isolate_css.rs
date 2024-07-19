use std::borrow::Cow;

use crate::{
    ast::{traverse_mut, Attribute, AttributeValue, NodeType},
    component::passes::Pass,
    css::ast::{RegularRule, Rule, Value},
    Component, DeclaredVariables,
};

#[derive(Debug)]
pub struct IsolateCssPass {
    component_id: u8,
}

impl IsolateCssPass {
    pub fn new() -> Self {
        Self { component_id: 0 }
    }

    fn run_css_passes(&self, rules: &mut [Rule], declared_vars: &mut DeclaredVariables) {
        for rule in rules {
            let rule = match rule {
                Rule::At(at_rule) => {
                    if let Some(contents) = &mut at_rule.contents {
                        self.run_css_passes(contents, declared_vars);
                    }
                    continue;
                }
                Rule::Regular(rule) => rule,
            };

            self.modify_selectors(rule);
            self.assign_css_mustaches(rule, declared_vars);
        }
    }

    fn modify_selectors(&self, rule: &mut RegularRule) {
        for sel in &mut rule.selector {
            for part in &mut sel.parts {
                let new_text = part.text.as_ref().map_or_else(
                    || format!(".decor-{}", self.component_id),
                    |t| format!("{t}.decor-{}", self.component_id),
                );
                if let Some(s) = &mut part.text {
                    *s = new_text.into();
                }
            }
        }
    }

    // TODO: Move to somewhere else? declared vars should be formed by now
    #[allow(clippy::unused_self)]
    fn assign_css_mustaches(&self, rule: &mut RegularRule, declared_vars: &mut DeclaredVariables) {
        for decl in &rule.declarations {
            for mustache in decl.values.iter().filter_map(|val| match val {
                Value::Mustache(m) => Some(m),
                Value::Css(_) => None,
            }) {
                declared_vars.insert_css_mustache(mustache.clone());
            }
        }
    }

    fn assign_node_classes(&self, component: &mut Component) {
        traverse_mut(&mut component.fragment_tree, &mut |node| {
            let NodeType::Element(elem) = &mut node.node_type else {
                return;
            };
            let mut has_class = false;
            for attr in &mut elem.attrs {
                match attr {
                    Attribute::KeyValue(key, None) if key == &"class" => {
                        *attr = Attribute::KeyValue(
                            "class",
                            Some(AttributeValue::Literal(Cow::Owned(format!(
                                "decor-{}",
                                self.component_id
                            )))),
                        );
                        has_class = true;
                    }
                    Attribute::KeyValue(key, Some(AttributeValue::Literal(lit)))
                        if key == &"class" =>
                    {
                        *attr = Attribute::KeyValue(
                            "class",
                            Some(AttributeValue::Literal(Cow::Owned(format!(
                                "{} decor-{}",
                                lit, self.component_id
                            )))),
                        );
                        has_class = true;
                    }
                    Attribute::KeyValue(key, Some(AttributeValue::JavaScript(js)))
                        if key == &"class" =>
                    {
                        let new_js = format!("`${{{js}}} decor-{}`", self.component_id);
                        let parsed = rslint_parser::parse_text(&new_js, 0).syntax();
                        *attr =
                            Attribute::KeyValue("class", Some(AttributeValue::JavaScript(parsed)));
                        has_class = true;
                    }
                    _ => {}
                }
            }
            if !has_class {
                elem.attrs.push(Attribute::KeyValue(
                    "class",
                    Some(AttributeValue::Literal(Cow::Owned(format!(
                        "decor-{}",
                        self.component_id
                    )))),
                ));
            }
        });
    }
}

impl Pass for IsolateCssPass {
    fn run(mut self, component: &mut Component) -> anyhow::Result<()> {
        {
            let Some(css) = &mut component.css else {
                return Ok(());
            };
            self.component_id = component.component_id;
            self.run_css_passes(&mut css.rules, &mut component.declared_vars);
        }

        self.assign_node_classes(component);

        Ok(())
    }
}
