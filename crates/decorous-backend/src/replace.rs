use std::collections::HashMap;

use decorous_frontend::utils;
use rslint_parser::{ast::AssignExpr, AstNode, SmolStr, SyntaxNode, SyntaxNodeExt};
use rslint_text_edit::{apply_indels, Indel, TextRange};

pub fn replace_namerefs(syntax_node: &SyntaxNode, toplevel_vars: &HashMap<SmolStr, u64>) -> String {
    let unbound_refs = utils::get_unbound_refs(syntax_node);
    let mut node_text = syntax_node.to_string();

    let mut indels = vec![];
    for unbound in unbound_refs.into_iter().filter_map(|n| n.ident_token()) {
        if unbound
            .parent()
            .parent()
            .and_then(|parent| parent.try_to::<AssignExpr>())
            .is_some()
        {
            continue;
        };
        let Some(var_idx) = toplevel_vars.get(unbound.text()).map(|idx| *idx) else {
            continue;
        };
        let replacement = format!("ctx[{}]", var_idx);
        let local_offset = unbound.text_range().start() - syntax_node.text_range().start();
        let indel = Indel::replace(
            TextRange::new(local_offset, local_offset + unbound.text_range().len()),
            replacement,
        );
        indels.push(indel);
    }
    indels.extend(replace_assignments_indels(syntax_node, toplevel_vars));
    apply_indels(&indels, &mut node_text);

    node_text
}

pub fn replace_assignments(
    syntax_node: &SyntaxNode,
    toplevel_vars: &HashMap<SmolStr, u64>,
) -> String {
    let mut node_text = syntax_node.to_string();
    let indels = replace_assignments_indels(syntax_node, toplevel_vars);
    apply_indels(&indels, &mut node_text);

    node_text
}

fn replace_assignments_indels(
    syntax_node: &SyntaxNode,
    toplevel_vars: &HashMap<SmolStr, u64>,
) -> Vec<Indel> {
    let unbound_refs = utils::get_unbound_refs(syntax_node);

    let mut indels = vec![];
    for name_ref in unbound_refs {
        let Some(assignment) = name_ref.syntax().parent().and_then(|parent| parent.try_to::<AssignExpr>()) else {
            continue;
        };
        let Some(name) = name_ref.ident_token() else {
            continue;
        };
        let Some(idx) = toplevel_vars.get(name.text()) else {
            continue;
        };
        let replacement = format!("__schedule_update({}, {})", *idx, assignment);
        let local_offset = assignment.range().start() - syntax_node.text_range().start();
        let indel = Indel::replace(
            TextRange::new(local_offset, local_offset + assignment.range().len()),
            replacement,
        );
        indels.push(indel);
    }

    indels
}
