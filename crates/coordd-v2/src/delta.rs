//! The one delta per change (`StepDelta`): narrowed, merged into a frame, applied; from two views only where no
//! machine says what moved.

use crate::machine::{Held, View};
use coordd_v2_wire::StepDelta;
use serde_json::Value as Json;
use std::collections::{BTreeMap, BTreeSet, HashMap};

type ByType = BTreeMap<String, Held>;

/// What went in and out between A and B; a type whose instances are the same is passed by.
fn moved(a: &ByType, b: &ByType) -> (Vec<Json>, Vec<Json>) {
    let none = Held::new();
    let (mut new, mut gone) = (Vec::new(), Vec::new());
    for t in a.keys().chain(b.keys()).collect::<BTreeSet<_>>() {
        let (x, y) = (a.get(t).unwrap_or(&none), b.get(t).unwrap_or(&none));
        if x != y {
            new.extend(y.iter().filter(|(k, _)| !x.contains_key(*k)).map(|(_, j)| j.clone()));
            gone.extend(x.iter().filter(|(k, _)| !y.contains_key(*k)).map(|(_, j)| j.clone()));
        }
    }
    (new, gone)
}

/// The delta from view A to view B, at change SEQ: for a machine that cannot say what a change moved, or a reader behind.
pub fn between(a: &View, b: &View, seq: u64) -> StepDelta {
    let (created_facts, terminated_facts) = moved(&a.facts, &b.facts);
    let (new_duties, terminated_duties) = moved(&a.duties, &b.duties);
    let (violations, mut cleared_violations) = moved(&a.violated, &b.violated);
    cleared_violations.retain(|v| !terminated_duties.contains(v));
    StepDelta { seq, created_facts, terminated_facts, new_duties, terminated_duties, violations, cleared_violations }
}

/// The type an instance is of, as the record writes it.
pub(crate) fn ty(j: &Json) -> &str {
    j["inst"].as_str().unwrap_or_default()
}

/// D narrowed to TYPES (every type when none).
pub fn only(d: &StepDelta, types: Option<&BTreeSet<String>>) -> StepDelta {
    let Some(ts) = types else { return d.clone() };
    let keep = |xs: &[Json]| xs.iter().filter(|j| ts.contains(ty(j))).cloned().collect();
    StepDelta { seq: d.seq, created_facts: keep(&d.created_facts), terminated_facts: keep(&d.terminated_facts), new_duties: keep(&d.new_duties), terminated_duties: keep(&d.terminated_duties), violations: keep(&d.violations), cleared_violations: keep(&d.cleared_violations) }
}

/// View V narrowed to TYPES (every type when none).
pub fn narrowed(v: &View, types: Option<&BTreeSet<String>>) -> View {
    let Some(ts) = types else { return v.clone() };
    let keep = |m: &ByType| m.iter().filter(|(t, _)| ts.contains(*t)).map(|(t, xs)| (t.clone(), xs.clone())).collect();
    View { facts: keep(&v.facts), duties: keep(&v.duties), violated: keep(&v.violated) }
}

/// What a change does to one instance, in the order [`apply`] reads a change.
#[derive(Clone, Copy, PartialEq)]
enum Move {
    End,
    Come,
    Violate,
    Clear,
}

/// One instance a frame touched: whether it held and was violated when the frame began (`None`: not known), and now.
struct Touch {
    j: Json,
    duty: bool,
    held: bool,
    was_violated: Option<bool>,
    holds: bool,
    violated: bool,
}

/// Changes merged by instance into one frame: each merged costs what it moved, and the delta is made when sent.
#[derive(Default)]
pub struct Frame {
    seq: u64,
    at: HashMap<String, usize>,
    touched: Vec<Touch>,
}

impl Frame {
    /// D after what is merged so far; D is read against the state the frame stands at, as every engine delta is.
    pub fn push(&mut self, d: &StepDelta) {
        self.seq = d.seq;
        let moves = [(&d.terminated_facts, false, Move::End), (&d.created_facts, false, Move::Come), (&d.terminated_duties, true, Move::End), (&d.new_duties, true, Move::Come), (&d.violations, true, Move::Violate), (&d.cleared_violations, true, Move::Clear)];
        for (xs, duty, m) in moves {
            xs.iter().for_each(|x| self.touch(x, duty, m));
        }
    }

    fn touch(&mut self, x: &Json, duty: bool, m: Move) {
        let touched = &mut self.touched;
        let i = *self.at.entry(x.to_string()).or_insert_with(|| {
            // the first move says what held before it: what comes did not, and was violated only if it ended
            let was_violated = match m {
                Move::Come | Move::Violate => Some(false),
                Move::Clear => Some(true),
                Move::End => None,
            };
            let held = m != Move::Come;
            touched.push(Touch { j: x.clone(), duty, held, was_violated, holds: held, violated: was_violated == Some(true) });
            touched.len() - 1
        });
        let t = &mut self.touched[i];
        match m {
            Move::End => (t.holds, t.violated) = (false, false),
            Move::Come => (t.holds, t.violated) = (true, false),
            Move::Violate => t.violated = true,
            Move::Clear => t.violated = false,
        }
    }

    /// The one delta from where the frame began to now: what came and went, and each held duty's violation that moved.
    pub fn delta(&self) -> StepDelta {
        let mut d = StepDelta { seq: self.seq, ..StepDelta::default() };
        for t in &self.touched {
            let to = match (t.duty, t.held, t.holds) {
                (false, false, true) => Some(&mut d.created_facts),
                (false, true, false) => Some(&mut d.terminated_facts),
                (true, false, true) => Some(&mut d.new_duties),
                (true, true, false) => Some(&mut d.terminated_duties),
                _ => None,
            };
            to.into_iter().for_each(|xs| xs.push(t.j.clone()));
            // a duty that ended takes its violation with it; one that holds says its own when it may have moved
            if t.duty && t.holds && t.was_violated != Some(t.violated) {
                (if t.violated { &mut d.violations } else { &mut d.cleared_violations }).push(t.j.clone());
            }
        }
        d
    }
}

/// V with D applied: its instances in or out, an ended duty's violation with it.
pub fn apply(v: &mut View, d: &StepDelta) {
    let put = |m: &mut ByType, xs: &[Json]| xs.iter().for_each(|x| drop(m.entry(ty(x).to_string()).or_default().insert(x.to_string(), x.clone())));
    let take = |m: &mut ByType, xs: &[Json]| xs.iter().for_each(|x| drop(m.get_mut(ty(x)).and_then(|h| h.remove(&x.to_string()))));
    take(&mut v.facts, &d.terminated_facts);
    put(&mut v.facts, &d.created_facts);
    put(&mut v.violated, &d.violations);
    take(&mut v.violated, &d.cleared_violations);
    take(&mut v.duties, &d.terminated_duties);
    take(&mut v.violated, &d.terminated_duties);
    put(&mut v.duties, &d.new_duties);
}

#[cfg(test)]
#[path = "delta_tests.rs"]
mod tests;
