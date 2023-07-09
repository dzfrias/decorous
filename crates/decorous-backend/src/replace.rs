use decorous_frontend::DeclaredVariables;
use rslint_parser::{
    ast::{ArrowExpr, AssignExpr, NameRef},
    AstNode, SyntaxNode, SyntaxNodeExt,
};
use rslint_text_edit::{apply_indels, Indel, TextRange};

pub fn replace_namerefs(
    syntax_node: &SyntaxNode,
    name_refs: &[NameRef],
    toplevel_vars: &DeclaredVariables,
) -> String {
    let mut node_text = syntax_node.to_string();

    if let Some(first_child) = syntax_node.first_child() {
        if first_child.is::<ArrowExpr>() {
            let expr = first_child.to();
            let Some(idx) = toplevel_vars.get_arrow_expr(&expr) else {
                return node_text;
            };

            node_text.drain(..);
            node_text.push_str(&format!("ctx[{idx}]"));
            return node_text;
        }
    }

    let mut indels = vec![];
    for unbound in name_refs.iter().filter_map(|n| n.ident_token()) {
        if unbound
            .parent()
            .parent()
            .and_then(|parent| parent.try_to::<AssignExpr>())
            .is_some()
        {
            continue;
        };
        let Some(var_idx) = toplevel_vars.get_var(unbound.text()) else {
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
    indels.extend(replace_assignments_indels(
        syntax_node,
        name_refs,
        toplevel_vars,
    ));
    apply_indels(&indels, &mut node_text);

    node_text
}

pub fn replace_assignments(
    syntax_node: &SyntaxNode,
    name_refs: &[NameRef],
    toplevel_vars: &DeclaredVariables,
) -> String {
    let mut node_text = syntax_node.to_string();
    let indels = replace_assignments_indels(syntax_node, name_refs, toplevel_vars);
    apply_indels(&indels, &mut node_text);

    node_text
}

fn replace_assignments_indels(
    syntax_node: &SyntaxNode,
    name_refs: &[NameRef],
    toplevel_vars: &DeclaredVariables,
) -> Vec<Indel> {
    let mut indels = vec![];
    for name_ref in name_refs {
        let Some(assignment) = name_ref.syntax().parent().and_then(|parent| parent.try_to::<AssignExpr>()) else {
            continue;
        };
        let Some(name) = name_ref.ident_token() else {
            continue;
        };
        let Some(idx) = toplevel_vars.get_var(name.text()) else {
            continue;
        };
        let replacement = format!("__schedule_update({}, {})", idx, assignment);
        let local_offset = assignment.range().start() - syntax_node.text_range().start();
        let indel = Indel::replace(
            TextRange::new(local_offset, local_offset + assignment.range().len()),
            replacement,
        );
        indels.push(indel);
    }

    indels
}
