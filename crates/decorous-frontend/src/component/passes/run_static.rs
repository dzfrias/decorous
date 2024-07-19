use rslint_parser::SmolStr;

use crate::{component::passes::Pass, Component, ToplevelNodeData};

pub struct StaticPass;

impl StaticPass {
    pub fn new() -> Self {
        Self
    }
}

impl Pass for StaticPass {
    fn run(self, component: &mut Component) -> anyhow::Result<()> {
        let Some(code) = component.comptime.as_ref() else {
            return Ok(());
        };

        let js_env = component.ctx.executor.execute(code)?;
        for decl in js_env.items() {
            let syntax_node =
                rslint_parser::parse_text(&format!("let {} = {};", decl.name, decl.value), 0);
            // PERF: Ring buffer?
            component.toplevel_nodes.insert(
                0,
                ToplevelNodeData {
                    node: syntax_node.syntax(),
                    substitute_assign_refs: true,
                },
            );
            component.declared_vars.insert_var(SmolStr::new(&decl.name));
        }

        Ok(())
    }
}
