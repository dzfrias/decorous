use std::{
    fmt::{self, Write},
    iter,
};

use decorous_frontend::DeclaredVariables;
use itertools::Itertools;
use rslint_parser::{
    ast::{ArrowExpr, ArrowExprParams, AssignExpr, NameRef},
    AstNode, SmolStr, SyntaxNode, SyntaxNodeExt,
};
use rslint_text_edit::{apply_indels, Indel, TextRange};

macro_rules! force_write {
    ($($arg:tt)*) => {
        write!($($arg)*).expect("write should not fail")
    };
}

pub(crate) use force_write;

macro_rules! sort_if_testing {
    ($to_sort:expr, $by:expr) => {{
        #[allow(unused)]
        use ::itertools::Itertools;
        #[cfg(test)]
        let iter = $to_sort.sorted_by($by);
        #[cfg(not(test))]
        let iter = $to_sort;

        iter
    }};
}

pub(crate) use sort_if_testing;

#[derive(Debug, Clone)]
pub struct DirtyIndices(pub(self) Vec<(usize, u8)>);

impl DirtyIndices {
    pub fn new() -> Self {
        Self(vec![])
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for DirtyIndices {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return write!(f, "true");
        }
        let mut all = String::new();
        for (i, (idx, bitmask)) in self.0.iter().enumerate() {
            write!(all, "dirty[{idx}] & {bitmask}")?;
            if i != self.0.len() - 1 {
                all.push_str(" || ");
            }
        }
        write!(f, "{all}")?;

        Ok(())
    }
}

/// Returns an vector of (DIRTY, BITMASK). DIRTY is the index in the u8 buffer on the
/// JavaScript side. BITMASK is a bit mask for the changed variables in the corresponding u8.
/// For example, if the 9th variable had to be dirty, this would return [(1, 0b1)]. Or if the
/// 9th and tenth were dirty, it work be [(1, 0b11)].
pub fn calc_dirty(
    unbound: &[NameRef],
    declared: &DeclaredVariables,
    scope_id: Option<u32>,
) -> DirtyIndices {
    let mut dirty_indices = DirtyIndices::new();
    for unbound in unbound {
        let Some(ident) = unbound.ident_token().map(|tok| tok.text().clone()) else {
            continue;
        };
        let Some(idx) = declared.get_var(&ident, scope_id) else {
            continue;
        };
        // If the variable is a "scoped" variable (i.e. declared in a {#for} block), do not
        // calculate it's dirty value. Scoped variables, by nature, can't be mutated, so they can
        // never be dirty.
        if scope_id.is_some_and(|id| declared.is_scope_var(&ident, id)) {
            continue;
        }
        // Get the byte index for the dirty bitmap. Need to subtract one because
        // ceiling division only results in 0 if x == 0.
        let dirty_idx = ((idx + 7) / 8).saturating_sub(1) as usize;

        // Modulo 8 so it wraps every byte. The byte is tracked by dirty_idx
        let bitmask = 1 << (idx % 8);
        if let Some(pos) = dirty_indices
            .0
            .iter()
            .position(|(idx, _)| *idx == dirty_idx)
        {
            dirty_indices.0[pos].1 |= bitmask;
        } else {
            dirty_indices.0.push((dirty_idx, bitmask));
        }
    }
    dirty_indices
}

pub fn replace_namerefs(
    syntax_node: &SyntaxNode,
    name_refs: &[NameRef],
    toplevel_vars: &DeclaredVariables,
    scope_id: Option<u32>,
) -> String {
    let mut node_text = syntax_node.to_string();

    if let Some(first_child) = syntax_node.first_child() {
        if first_child.is::<ArrowExpr>() {
            let expr = first_child.to();
            let Some((idx, _)) = toplevel_vars.get_arrow_expr(&expr) else {
                return node_text;
            };

            node_text.clear();
            force_write!(&mut node_text, "ctx[{idx}]");
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
        let Some(var_idx) = toplevel_vars.get_var(unbound.text(), scope_id) else {
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
        scope_id,
    ));
    apply_indels(&indels, &mut node_text);

    node_text
}

pub fn replace_assignments(
    syntax_node: &SyntaxNode,
    name_refs: &[NameRef],
    toplevel_vars: &DeclaredVariables,
    scope_id: Option<u32>,
) -> String {
    let mut node_text = syntax_node.to_string();
    let mut indels = vec![];

    if syntax_node.is::<ArrowExpr>() {
        let arrow_expr = syntax_node.to::<ArrowExpr>();

        let params = arrow_expr.params().expect("arrow expr should have params");
        let to_append = name_refs.iter().filter_map(|nref| {
            let tok = nref.ident_token().unwrap();
            let ident = tok.text();
            if !toplevel_vars.all_vars().contains_key(ident)
                && toplevel_vars.get_var(ident, scope_id).is_some()
            {
                Some(ident.clone())
            } else {
                None
            }
        });
        let all_params = match &params {
            ArrowExprParams::Name(n) => iter::once(n.ident_token().unwrap().text().clone())
                .chain(to_append)
                .join(", "),
            ArrowExprParams::ParameterList(plist) => plist
                .parameters()
                .map(|pat| SmolStr::new(pat.text()))
                .chain(to_append)
                .join(", "),
        };
        let replacement = format!("({all_params})");
        indels.push(Indel {
            insert: replacement,
            delete: params.range(),
        });
    }

    indels.extend(replace_assignments_indels(
        syntax_node,
        name_refs,
        toplevel_vars,
        scope_id,
    ));
    apply_indels(&indels, &mut node_text);

    node_text
}

fn replace_assignments_indels(
    syntax_node: &SyntaxNode,
    name_refs: &[NameRef],
    toplevel_vars: &DeclaredVariables,
    scope_id: Option<u32>,
) -> Vec<Indel> {
    let mut indels = vec![];
    for name_ref in name_refs {
        let Some(assignment) = name_ref
            .syntax()
            .parent()
            .and_then(|parent| parent.try_to::<AssignExpr>())
        else {
            continue;
        };
        let Some(name) = name_ref.ident_token() else {
            continue;
        };
        let Some(idx) = toplevel_vars.get_var(name.text(), scope_id) else {
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
