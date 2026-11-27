//! Checking a kind whole: every error kept, not only the first (N-101: the library and a project's program loaded,
//! "every failing line reported"). Where a statement fails, what it binds is unknown from there on (`Any`, and
//! marked), so the rest is still checked and nothing that only follows from the first error is reported again: a
//! value unknown so is matched loosely, its arms checked with their names unknown too.

use super::{Cx, Delta, Excs};
use crate::syntax::{Bound, Expr, Pat, Prog};
use crate::types::Type;
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;

/// The errors kept while one kind is checked, and the names made unknown by them.
#[derive(Default)]
pub struct Collect {
    errors: RefCell<Vec<String>>,
    /// Above 0: a trial (an arm checked before its type is known), whose errors are its own to give back.
    quiet: Cell<u32>,
    unknown: RefCell<BTreeSet<String>>,
}

impl Collect {
    /// The errors kept, in the order found.
    pub fn take(&self) -> Vec<String> {
        std::mem::take(&mut self.errors.borrow_mut())
    }
}

impl Cx<'_> {
    fn collecting(&self) -> Option<&Collect> {
        self.all.filter(|a| a.quiet.get() == 0)
    }

    /// E kept, when checking all (and checking goes on); otherwise E is the error.
    pub(super) fn keep(&self, e: String) -> Result<(), String> {
        match self.collecting() {
            Some(a) => {
                a.errors.borrow_mut().push(e);
                Ok(())
            }
            None => Err(e),
        }
    }

    /// F run with nothing kept: its first error is its result, as when not checking all.
    pub(super) fn quietly<T>(&self, f: impl FnOnce() -> T) -> T {
        let Some(a) = self.all else { return f() };
        a.quiet.set(a.quiet.get() + 1);
        let r = f();
        a.quiet.set(a.quiet.get() - 1);
        r
    }

    /// The names X binds are unknown from here on (their statement failed).
    pub(super) fn poison(&self, x: &Bound) {
        let Some(a) = self.all else { return };
        let mut u = a.unknown.borrow_mut();
        match x {
            Bound::Var(v) => {
                u.insert(v.clone());
            }
            Bound::Tuple(vs) => u.extend(vs.iter().cloned()),
        }
    }

    pub(super) fn is_poisoned(&self, vs: &[String]) -> bool {
        self.all.is_some_and(|a| vs.iter().all(|v| a.unknown.borrow().contains(v)))
    }

    /// Whether E is a name made unknown by an error (and typed so here).
    pub(super) fn unknown(&self, e: &Expr, d: &Delta) -> bool {
        let Some(a) = self.collecting() else { return false };
        let mut e = e;
        while let Expr::Field(inner, _) = e {
            e = inner;
        }
        matches!(e, Expr::Var(v) if a.unknown.borrow().contains(v) && d.get(v) == Some(&Type::Any))
    }

    /// The arms of a match on an unknown value: each body checked, the names its pattern binds unknown too.
    pub(super) fn loose_match(&self, arms: &mut [(Pat, Prog)], want: Option<&Type>, d: &Delta) -> Result<(Option<Type>, Excs), String> {
        let mut result = want.cloned();
        let mut excs = Excs::new();
        for (pat, body) in arms.iter_mut() {
            let mut d2 = d.clone();
            let mut names = Vec::new();
            pat_names(pat, &mut names);
            for n in &names {
                d2.insert(n.clone(), Type::Any);
            }
            self.poison(&Bound::Tuple(names));
            match self.prog(body, result.as_ref(), &d2) {
                Ok((t, x)) => {
                    excs.extend(x);
                    if result.is_none() {
                        result = t;
                    }
                }
                Err(e) => self.keep(e)?,
            }
        }
        Ok((result.or(Some(Type::Any)), excs))
    }

    /// The end of checking a kind: R, or, when checking all, the first error kept (all of them are in the Collect).
    pub(super) fn finish(&self, r: Result<(), String>) -> Result<(), String> {
        let Some(a) = self.all else { return r };
        if let Err(e) = r {
            a.errors.borrow_mut().push(e);
        }
        match a.errors.borrow().first() {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }
}

/// The names pattern P binds.
fn pat_names(p: &Pat, out: &mut Vec<String>) {
    match p {
        Pat::Var(v) => out.push(v.clone()),
        Pat::Con { args, .. } | Pat::Tuple(args) => args.iter().for_each(|a| pat_names(a, out)),
        Pat::One(p) => pat_names(p, out),
        Pat::Wild | Pat::Nil | Pat::None => {}
    }
}
