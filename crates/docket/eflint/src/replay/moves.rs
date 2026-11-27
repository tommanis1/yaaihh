//! What committed changes moved, gathered as they come: instances, stale types they reached, duties whose violation they reached.

use super::*;
use crate::eval::{Delta, reads};

/// What the changes noted so far moved; `all` when there is no telling but by reading every type.
#[derive(Clone, Debug, Default)]
pub struct Moves {
    pub all: bool,
    /// Each instance a change may have moved, with whether it held before the first.
    was: BTreeMap<Tagged, bool>,
    /// The types a change reached whose instances it did not say: to be read whole.
    whole: BTreeSet<Id>,
    /// The duties whose violation a change may have changed.
    duties: BTreeSet<Tagged>,
}

/// [`Moves`] read on the state as it stands.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Moved {
    pub all: bool,
    pub made: Vec<Tagged>,
    pub ended: Vec<Tagged>,
    /// The types to read whole; no instance of them is in `made` or `ended`.
    pub whole: BTreeSet<Id>,
    /// Each duty reached that is not of a `whole` type, with whether it holds violated now.
    pub duties: Vec<(Tagged, bool)>,
}

impl Replay {
    /// Note in M what DELTA (from `undo`/`redo` just now) moved: only the types it reaches, never any other.
    pub fn note(&mut self, delta: &Delta, m: &mut Moves) {
        let Some(plan) = self.plan() else {
            m.all = true;
            return;
        };
        let spec = &self.cfg.spec;
        for (te, was) in delta {
            m.was.entry(te.clone()).or_insert(*was);
        }
        let changed: BTreeSet<Id> = delta.keys().map(|te| spec.chase(&te.dom)).collect();
        for t in plan.reached(&changed) {
            let duty = match spec.decls.get(&t).map(|d| &d.kind) {
                Some(Kind::Fact { .. }) => false,
                Some(Kind::Duty { .. }) => true,
                _ => continue,
            };
            if self.stale.contains(&t) {
                m.whole.insert(t);
                continue;
            }
            if !duty {
                continue;
            }
            match plan.duties_reached(spec, &self.cfg.state, &self.stale, &t, delta) {
                Some(tes) => m.duties.extend(tes),
                None => {
                    m.whole.insert(t);
                }
            }
        }
    }

    /// M read on the state now: what came, what went, which duties reached hold violated; only M's types worked out.
    pub fn moved(&mut self, m: Moves) -> Result<Moved, String> {
        if m.all {
            return Ok(Moved { all: true, ..Moved::default() });
        }
        let ty = |spec: &crate::compile::Spec, te: &Tagged| spec.chase(&te.dom);
        let asked: BTreeSet<Id> = m.whole.iter().cloned().chain(m.duties.iter().map(|te| ty(&self.cfg.spec, te))).collect();
        let read = reads(&self.cfg.spec, asked);
        freshen(&mut self.cfg, &mut self.stale, Some(&read)).map_err(|e| e.message())?;
        let (spec, state) = (&self.cfg.spec, &self.cfg.state);
        let mut out = Moved { whole: m.whole, ..Moved::default() };
        for (te, was) in m.was {
            if out.whole.contains(&ty(spec, &te)) || state.holds(&te) == was {
                continue;
            }
            if was { out.ended.push(te) } else { out.made.push(te) }
        }
        let mut ev = Ev::new(spec, state);
        for te in m.duties.into_iter().filter(|te| !out.whole.contains(&ty(spec, te))) {
            let violated = state.holds(&te) && ev.violation_condition(&te).map_err(|e| e.message())?.contains(&true);
            out.duties.push((te, violated));
        }
        Ok(out)
    }
}
