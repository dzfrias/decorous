use std::fmt::{self, Write};

use decorous_frontend::DeclaredVariables;
use rslint_parser::ast::NameRef;

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
        let mut all = String::new();
        for (i, (idx, bitmask)) in self.0.iter().enumerate() {
            write!(all, "dirty[{idx}] & {bitmask}")?;
            if i != self.0.len() - 1 {
                all.push_str(" || ")
            }
        }
        write!(f, "{all}")?;

        Ok(())
    }
}

/// Returns an vector of (DIRTY_IDX, BITMASK). DIRTY_IDX is the index in the u8 buffer on the
/// JavaScript side. BITMASK is a bit mask for the changed variables in the corresponding u8.
/// For example, if the 9th variable had to be dirty, this would return [(1, 0b1)]. Or if the
/// 9th and tenth were dirty, it work be [(1, 0b11)].
pub fn calc_dirty(unbound: &[NameRef], declared: &DeclaredVariables) -> DirtyIndices {
    let mut dirty_indices = DirtyIndices::new();
    for unbound in unbound {
        let Some(ident) = unbound.ident_token().map(|tok| tok.text().clone()) else {
                    continue;
                };
        let Some(idx) = declared.get_var(&ident) else {
                    continue;
                };
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
