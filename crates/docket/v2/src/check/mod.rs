//! Loading a text (spec 14): it is checked whole, and loaded whole or not at all.

pub mod names;
pub mod resolve;
pub mod types;
pub mod typing;

pub use names::{LoadError, Loaded};
pub use resolve::resolve;

/// Load modules as one text: every name resolved (check 2), then every program typed (checks 3, 6, 8).
pub fn load(texts: Vec<crate::syntax::tree::Module>) -> Result<Loaded, LoadError> {
    let mut l = names::load(texts)?;
    l.rows = typing::check(&l).map_err(|es| {
        let n = es.len();
        let mut first = es.into_iter().next().unwrap_or_else(|| unreachable!("a failed check has an error"));
        if n > 1 {
            first.why = format!("{} (and {} more)", first.why, n - 1);
        }
        first
    })?;
    Ok(l)
}
