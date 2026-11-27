//! A step kept only if it is enabled and closes no duty of its holder, and a kept step taken back.

use super::*;
use crate::eval::{Assign, Delta, Store, reads, undecided_why};

/// Why [`Replay::step`] did not keep a transition.
#[derive(Clone, PartialEq, Debug)]
pub enum Refusal {
    /// It is not enabled: a condition fails, or an instance it syncs with is not enabled.
    Policy,
    /// It would end this duty, held by the holder named, or clear its violation.
    Closes(Tagged),
    /// It could not be worked out.
    Error(Vec<String>),
    /// Whether it is enabled turns on instances the well-founded model leaves unknown: the knots, the reads that close
    /// their cycles with their places, and the instances.
    Undecided(String),
}

/// A transition [`Replay::step`] kept: the acts and events it performed, the instances it made hold, those that held
/// and that it ended or made unknown (`Obfuscates`), and the stored facts it changed as they were before, so
/// [`Replay::undo`] can take it back and [`Replay::redo`] make it again.
#[derive(Clone, Debug)]
pub struct Taken {
    pub executed: Vec<Tagged>,
    pub created: Vec<Tagged>,
    pub terminated: Vec<Tagged>,
    pub(super) store: Store,
    pub(super) was: Vec<(Tagged, Option<Info>)>,
}

impl Replay {
    /// Perform act or event E, its instance given whole, and keep it only if it is enabled and, HOLDER given, no duty
    /// whose holder is HOLDER holds before it and not after, or is violated before and holds unviolated after (the
    /// whole transition, what it syncs with included). A holder is a value: a duty whose first field holds it is
    /// HOLDER's, whatever that field's type. The step is worked out on the state as it stands, put in place, checked
    /// and taken back if refused, so a refusal leaves the state as it was. The duties read are HOLDER's of the types
    /// the step touches: read whole before and after it, or, where the plan keeps them up to date, those its changes
    /// reach.
    pub fn step(&mut self, e: &Event, holder: Option<&Elem>) -> Result<Taken, Refusal> {
        let phrases = self.phrases(e).map_err(|m| Refusal::Error(vec![m]))?;
        let [p] = &phrases[..] else { return Err(Refusal::Error(vec!["one act or event, one phrase".into()])) };
        let info = transition_in_place(&mut self.cfg, &mut self.stale, p).map_err(|outs| {
            let mut a = Answer::default();
            take_outputs(outs, &mut a);
            Refusal::Error(a.errors)
        })?;
        // read again on the upper bound where any instance is unknown: a step it would take otherwise, enabled or with
        // other effects (a `When` an unknown instance decides), is undecided
        let may = match self.cfg.state.unknown().is_empty() {
            true => None,
            false => Some(transition_may(&mut self.cfg, &mut self.stale, p).map_err(|outs| {
                let mut a = Answer::default();
                take_outputs(outs, &mut a);
                Refusal::Error(a.errors)
            })?),
        };
        let undecided = || Refusal::Undecided(undecided_why(&self.cfg.spec, &self.cfg.state));
        if info.disabled {
            return Err(if may.is_some_and(|m| !m.disabled) { undecided() } else { Refusal::Policy });
        }
        if may.is_some_and(|m| m.disabled || m.assignments != info.assignments) {
            return Err(undecided());
        }
        let changed: BTreeSet<Id> = info.assignments.keys().map(|te| self.cfg.spec.chase(&te.dom)).collect();
        let touched = self.touched(&changed);
        let plan = self.plan();
        let before = match holder {
            Some(h) if plan.is_none() && !touched.is_empty() => self.duties_among(h, &touched).map_err(|m| Refusal::Error(vec![m]))?,
            _ => Vec::new(),
        };
        // the stored facts the transition changes, as they were: a `Var` or `Function` type's others too
        let mut was: BTreeMap<Tagged, Option<Info>> = BTreeMap::new();
        for te in info.assignments.keys() {
            if self.cfg.spec.decls.get(&te.dom).is_some_and(|t| t.restriction.is_some()) {
                for k in self.cfg.state.instances_of(&te.dom) {
                    let i = self.cfg.state.contents().get(&k).cloned();
                    was.entry(k).or_insert(i);
                }
            }
            was.entry(te.clone()).or_insert_with(|| self.cfg.state.contents().get(te).cloned());
        }
        // what the step makes hold, and what it ends that held: ending an instance that does not hold changes nothing
        let changes = |made: bool| {
            let state = &self.cfg.state;
            info.assignments.iter().filter(|(te, a)| (**a == Assign::True) == made && state.holds(te) != made).map(|(te, _)| te.clone()).collect()
        };
        let (created, terminated) = (changes(true), changes(false));
        let held = was.keys().map(|te| (te.clone(), self.cfg.state.holds(te))).collect();
        assign(&self.cfg.spec, &info.assignments, &mut self.cfg.state);
        let delta = self.changed(held);
        let mut executed = Vec::new();
        collect_executed(&info, &mut executed);
        let taken = Taken { executed, created, terminated, store: info.assignments.clone(), was: was.into_iter().collect() };
        let refused = match (holder, plan) {
            (Some(h), Some(plan)) if !touched.is_empty() => self.closes(&plan, h, &touched, &delta, &taken).err(),
            _ if before.is_empty() => None,
            _ => self.kept(before, &touched).err(),
        };
        match refused {
            None => Ok(taken),
            Some(r) => {
                self.undo(&taken);
                Err(r)
            }
        }
    }

    /// Whether each of BEFORE, duties of the types TOUCHED as they were before a step put in place, still holds, and
    /// is violated if it was.
    fn kept(&mut self, before: Vec<DutyState>, touched: &BTreeSet<Id>) -> Result<(), Refusal> {
        let read = reads(&self.cfg.spec, touched.clone());
        freshen(&mut self.cfg, &mut self.stale, Some(&read)).map_err(|e| Refusal::Error(vec![e.message()]))?;
        let mut ev = Ev::new(&self.cfg.spec, &self.cfg.state);
        for d in before {
            match ev.violation_condition(&d.duty) {
                Ok(v) if self.cfg.state.holds(&d.duty) && (v.contains(&true) || !d.violated) => {}
                Ok(_) => return Err(self.closing(d.duty)),
                Err(e) => return Err(Refusal::Error(vec![e.message()])),
            }
        }
        Ok(())
    }

    /// [`Replay::kept`] for a step put in place whose changes are DELTA: the duties of H's of the types TOUCHED that
    /// PLAN keeps up to date are read where the changes reach them, held before from DELTA, violated before only where
    /// that decides; the others whole. The state before is read with the step taken back, then made again. As
    /// [`Replay::kept`] names the least duty that fails, so does this.
    fn closes(&mut self, plan: &Plan, h: &Elem, touched: &BTreeSet<Id>, delta: &Delta, taken: &Taken) -> Result<(), Refusal> {
        let (mut whole, mut reached) = (BTreeSet::new(), BTreeSet::new());
        for d in touched {
            match plan.duties_reached(&self.cfg.spec, &self.cfg.state, &self.stale, d, delta) {
                Some(tes) => reached.extend(tes.into_iter().filter(|te| held_by(te, h))),
                None => {
                    whole.insert(d.clone());
                }
            }
        }
        let mut ev = Ev::new(&self.cfg.spec, &self.cfg.state);
        let (mut failed, mut open): (Option<Tagged>, Vec<Tagged>) = (None, Vec::new());
        for te in reached {
            if !delta.get(&te).copied().unwrap_or_else(|| self.cfg.state.holds(&te)) {
                continue;
            }
            if !self.cfg.state.holds(&te) {
                failed = least(failed, Some(te));
            } else if !ev.violation_condition(&te).map_err(|e| Refusal::Error(vec![e.message()]))?.contains(&true) {
                open.push(te);
            }
        }
        open.retain(|te| failed.as_ref().is_none_or(|f| te < f));
        if !whole.is_empty() || !open.is_empty() {
            self.undo(taken);
            let before = self.before(h, &whole, &open);
            self.redo(taken);
            let (before, violated) = before.map_err(|m| Refusal::Error(vec![m]))?;
            failed = least(failed, violated);
            match self.kept(before, &whole) {
                Err(Refusal::Closes(te)) => failed = least(failed, Some(te)),
                other => other?,
            }
        }
        failed.map_or(Ok(()), |te| Err(self.closing(te)))
    }

    /// The refusal of a step that ends duty TE or clears its violation: undecided where TE is left unknown by it.
    fn closing(&self, te: Tagged) -> Refusal {
        match self.cfg.state.unknown().contains(&te) {
            true => Refusal::Undecided(undecided_why(&self.cfg.spec, &self.cfg.state)),
            false => Refusal::Closes(te),
        }
    }

    /// Before a step: H's duties of the types WHOLE, and the least of OPEN that is violated.
    fn before(&mut self, h: &Elem, whole: &BTreeSet<Id>, open: &[Tagged]) -> Result<(Vec<DutyState>, Option<Tagged>), String> {
        let read = reads(&self.cfg.spec, open.iter().map(|te| te.dom.clone()).collect());
        freshen(&mut self.cfg, &mut self.stale, Some(&read)).map_err(|e| e.message())?;
        let mut ev = Ev::new(&self.cfg.spec, &self.cfg.state);
        let mut violated = None;
        for te in open {
            if ev.violation_condition(te).map_err(|e| e.message())?.contains(&true) {
                violated = least(violated, Some(te.clone()));
            }
        }
        Ok((self.duties_among(h, whole)?, violated))
    }

    /// Take back a transition [`Replay::step`] kept, or what [`Replay::keep_stored`] left out: the stored facts it
    /// changed as they were, their types declared now or not. What changed, as [`Replay::changed`] says.
    pub fn undo(&mut self, t: &Taken) -> Delta {
        let held = t.was.iter().map(|(te, _)| (te.clone(), self.cfg.state.holds(te))).collect();
        for (te, old) in t.was.iter().rev() {
            match old {
                Some(i) => self.cfg.state.insert(te.clone(), i.clone()),
                None => self.cfg.state.remove(te),
            }
        }
        self.changed(held)
    }

    /// Make again what [`Replay::undo`] took back. What changed, as [`Replay::changed`] says.
    pub fn redo(&mut self, t: &Taken) -> Delta {
        let held = t.was.iter().map(|(te, _)| (te.clone(), self.cfg.state.holds(te))).collect();
        assign(&self.cfg.spec, &t.store, &mut self.cfg.state);
        self.changed(held)
    }

    /// How a step's changes reach the derived types, where they are read guided and ranging: worked out again when the
    /// specification is not the one it was worked out for.
    pub(super) fn plan(&mut self) -> Option<Rc<Plan>> {
        let spec = &self.cfg.spec;
        if !(spec.reading.guided && spec.reading.ranging) {
            return None;
        }
        if !self.plan.as_ref().is_some_and(|p| p.fits(spec)) {
            self.plan = Some(Rc::new(Plan::of(spec)));
        }
        self.plan.clone()
    }

    /// The derived types brought up to date after the stored facts in HELD, each with whether it held, changed: as the
    /// plan says, or all made stale where there is none. What changed, with whether it held.
    pub(super) fn changed(&mut self, held: Vec<(Tagged, bool)>) -> Delta {
        let Some(plan) = self.plan() else {
            self.stale.extend(derived_types(&self.cfg.spec));
            return Delta::new();
        };
        let (spec, state) = (&self.cfg.spec, &mut self.cfg.state);
        // the same truth is no change, but for an instance a rule may work out once a stored fact no longer says
        let mut delta: Delta = held
            .into_iter()
            .filter(|(te, h)| state.holds(te) != *h || spec.find(&te.dom).is_some_and(|t| !t.derivation.is_empty()))
            .collect();
        plan.propagate(spec, state, &mut self.stale, &mut delta);
        delta
    }
    /// The duties that hold whose holder, their first field, is HOLDER, each with whether it is violated, in the order
    /// of values.
    pub fn duties_of(&mut self, holder: &Elem) -> Result<Vec<DutyState>, String> {
        let all = self.duty_types();
        self.duties_among(holder, &all)
    }

    pub(super) fn duty_types(&self) -> BTreeSet<Id> {
        self.cfg.spec.decls.iter().filter(|(_, t)| matches!(t.kind, Kind::Duty { .. })).map(|(d, _)| d.clone()).collect()
    }

    /// The duty types a change to CHANGED touches: those whose rule, or `Violated when`, reads one of them, directly or
    /// through derived types.
    fn touched(&self, changed: &BTreeSet<Id>) -> BTreeSet<Id> {
        let spec = &self.cfg.spec;
        self.duty_types().into_iter().filter(|d| !reads(spec, BTreeSet::from([d.clone()])).is_disjoint(changed)).collect()
    }

    /// [`Replay::duties_of`] among the duty types TYPES, which are worked out first, with what they read, if stale.
    fn duties_among(&mut self, holder: &Elem, types: &BTreeSet<Id>) -> Result<Vec<DutyState>, String> {
        let read = reads(&self.cfg.spec, types.clone());
        freshen(&mut self.cfg, &mut self.stale, Some(&read)).map_err(|e| e.message())?;
        let state = &self.cfg.state;
        let held: Vec<Tagged> =
            types.iter().flat_map(|d| state.instances_of(d).collect::<Vec<_>>()).filter(|te| state.holds(te) && held_by(te, holder)).collect();
        let (spec, state) = (&self.cfg.spec, &self.cfg.state);
        let mut ev = Ev::new(spec, state);
        let mut may = (!state.unknown().is_empty()).then(|| Ev::upper(spec, state));
        let mut out = Vec::new();
        for te in held {
            let violated = ev.violation_condition(&te).map_err(|e| e.message())?.contains(&true);
            let undecided = match may.as_mut() {
                Some(m) if !violated => m.violation_condition(&te).map_err(|e| e.message())?.contains(&true),
                _ => false,
            };
            out.push(DutyState { duty: te, violated, undecided });
        }
        out.sort_by(|a, b| a.duty.cmp(&b.duty));
        Ok(out)
    }
}

/// Whether H holds duty TE: its first field is H, whatever that field's type.
fn held_by(te: &Tagged, h: &Elem) -> bool {
    matches!(&te.elem, Elem::Prod(args) if args.first().map(|a| &a.elem) == Some(h))
}

/// The lesser of two duties, either missing.
fn least(a: Option<Tagged>, b: Option<Tagged>) -> Option<Tagged> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}
