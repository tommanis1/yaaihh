//! No Effect is lost (spec 14, check 4): each one a reaction, a connection's program or a filling may perform is
//! answered by someone.

use super::*;
use std::collections::BTreeSet;

impl<N: Norms> Machine<N> {
    /// The Effects of `row` that nobody answers: no root arm without `where`, no `Does`, no driver, and a result
    /// other than `Never`.
    pub(super) fn lost(&self, row: &BTreeSet<String>) -> Vec<String> {
        row.iter()
            .filter(|e| {
                let own = match self.loaded.syms.get(*e) {
                    Some(Sym::Effect { does, result, .. }) => does.is_some() || matches!(result, Type::Named(n, _) if n.to_string() == "Never"),
                    _ => false,
                };
                let answered = own || self.carriers.contains_key(*e) || self.loaded.ons.iter().any(|o| o.target == **e && o.cond.is_none()) || matches!(e.as_str(), "Now" | "Refused" | "Unfit" | "Exhausted" | "Fault");
                !answered
            })
            .cloned()
            .collect()
    }

    /// A reaction whose body or root arm may perform an Effect that nobody answers, said (14, check 4).
    pub(super) fn lost_reaction(&self) -> Option<String> {
        self.loaded.ons.iter().zip(&self.loaded.rows.ons).find_map(|(o, row)| {
            let lost = self.lost(row);
            (!lost.is_empty()).then(|| format!("{}:{}: the reaction to {} may perform {}, which nobody answers", o.module, o.at, o.target, lost.join(", ")))
        })
    }
}
