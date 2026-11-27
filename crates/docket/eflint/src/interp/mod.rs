//! Running phrases: one phrase takes a configuration (specification, state) to the next and
//! reports what happened.

use crate::ast::*;
use crate::compile::{CPhrase, Spec, Static};
use crate::eval::*;
use std::collections::BTreeSet;

mod in_place;
mod report;

pub use self::in_place::*;
pub use self::report::*;

#[derive(Clone)]
pub struct Config {
    pub spec: Spec,
    pub state: State,
    /// Every instance of an act or event type that could be triggered, with whether it is enabled.
    pub transitions: Vec<(Tagged, bool)>,
    /// The duties that hold.
    pub duties: Vec<Tagged>,
}

impl Default for Config {
    fn default() -> Self {
        Config { spec: Spec::new(), state: State::default(), transitions: Vec::new(), duties: Vec::new() }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Violation {
    Duty(Tagged),
    /// A disabled act was performed.
    Trigger(TransInfo),
    Invariant(Id),
}

#[derive(Clone, PartialEq, Debug)]
pub enum Output {
    CompilationError(Vec<String>),
    NotTriggerable(Id),
    Runtime(RtErr),
    Executed(TransInfo),
    Violation(Violation),
    Query(bool),
    /// A query that reads an instance left unknown and neither holds nor fails.
    Unknown,
    InstQuery(Vec<Tagged>),
}

fn tell_violations(info: &TransInfo, outs: &mut Vec<Output>) {
    if info.is_action() {
        if info.forced {
            outs.push(Output::Violation(Violation::Trigger(info.clone())));
        }
    } else {
        for s in &info.syncs {
            tell_violations(s, outs);
        }
    }
}

fn taggeds_rev(vs: Vec<Value>) -> Vec<Tagged> {
    vs.into_iter().rev().filter_map(|v| if let Value::Tagged(t) = v { Some(t) } else { None }).collect()
}

/// The assignments a compiled phrase makes; `None` when it failed.
fn sem_phrase(p: &CPhrase, spec: &Spec, state: &State, outs: &mut Vec<Output>) -> Option<Store> {
    let mut ev = Ev::new(spec, state);
    let fail = |outs: &mut Vec<Output>, e: RtErr| -> Option<Store> {
        outs.push(Output::Runtime(e));
        None
    };
    match p {
        CPhrase::OnlyDecls => Some(Store::new()),
        CPhrase::Query(t) => match truth(spec, state, t) {
            Err(e) => fail(outs, e),
            Ok(b) => {
                outs.push(b.map_or(Output::Unknown, Output::Query));
                Some(Store::new())
            }
        },
        CPhrase::InstQuery(must_hold, vs, t) => {
            let term = if *must_hold {
                Term::Bin(BinOp::When, Box::new(t.clone()), Box::new(Term::Un(UnOp::Present, Box::new(t.clone()))))
            } else {
                t.clone()
            };
            match ev.foreach(vs, &mut |ev| Ok(taggeds_rev(ev.eval(&term)?))) {
                Err(e) => fail(outs, e),
                Ok(tes) => {
                    outs.push(Output::InstQuery(tes));
                    Some(Store::new())
                }
            }
        }
        CPhrase::Create(vs, t) | CPhrase::Terminate(vs, t) | CPhrase::Obfuscate(vs, t) => {
            let kind = match p {
                CPhrase::Create(..) => EffKind::Create,
                CPhrase::Terminate(..) => EffKind::Terminate,
                _ => EffKind::Obfuscate,
            };
            match ev.eval_effect(&Effect { kind, vars: vs.clone(), term: t.clone() }) {
                Err(e) => fail(outs, e),
                Ok(store) => Some(store),
            }
        }
        CPhrase::Trigger(vs, t) => {
            let tes = match ev.foreach(vs, &mut |ev| Ok(taggeds_rev(ev.eval(t)?))) {
                Err(e) => return fail(outs, e),
                Ok(tes) => tes,
            };
            if let Some(first) = tes.first() {
                if !spec.triggerable(&first.dom) {
                    outs.push(Output::NotTriggerable(first.dom.clone()));
                    return Some(Store::new());
                }
            }
            let mut infos = Vec::new();
            for te in &tes {
                match ev.instantiate_trans(&BTreeSet::new(), te) {
                    Err(e) => return fail(outs, e),
                    Ok(mut one) => match one.pop() {
                        Some(info) if one.is_empty() => infos.push(info),
                        _ => {
                            return fail(outs, RtErr::Internal(format!("{} is not a single transition", te)));
                        }
                    },
                }
            }
            let mut store = Store::new();
            for info in &infos {
                outs.push(Output::Executed(info.clone()));
                tell_violations(info, outs);
            }
            for info in infos.iter().rev() {
                store = store_union(info.assignments.clone(), store);
            }
            Some(store)
        }
        CPhrase::Par(a, b) => {
            let s1 = sem_phrase(a, spec, state, outs);
            let s2 = sem_phrase(b, spec, state, outs);
            match (s1, s2) {
                (Some(x), Some(y)) => Some(store_union(x, y)),
                _ => None,
            }
        }
    }
}

/// Missing input makes a check yield nothing rather than fail.
fn ignore_missing<T>(r: R<Vec<T>>) -> R<Vec<T>> {
    match r {
        Err(e) if e.is_missing_input() => Ok(Vec::new()),
        other => other,
    }
}

/// Apply assignments, re-derive, and collect the duties, violations and possible transitions. Not FULL: the duties
/// only; no violation is reported and no transition listed (`Replay::perform`, a record replayed for one question at
/// its end).
fn new_context(spec: Spec, state: &State, store: &Store, outs: &mut Vec<Output>, full: bool) -> Option<Config> {
    let lazy = if full { BTreeSet::new() } else { lazy_types(&spec) };
    let state2 = match rebase_and_saturate_but(&spec, make_assignments(&spec, store, state), &lazy) {
        Ok(s) => s,
        Err(e) => {
            outs.push(Output::Runtime(e));
            return None;
        }
    };
    let duties: Vec<Tagged> = state2
        .holding()
        .filter(|te| matches!(spec.find(&te.dom).map(|t| &t.kind), Some(Kind::Duty { .. })))
        .cloned()
        .collect();
    if !full {
        return Some(Config { spec, state: state2, transitions: Vec::new(), duties });
    }
    let mut ev = Ev::new(&spec, &state2);
    let mut violations = Vec::new();
    let checked: R<()> = (|| {
        for te in &duties {
            for v in ignore_missing(ev.violation_condition(te))? {
                if v {
                    violations.push(Violation::Duty(te.clone()));
                }
            }
        }
        for d in spec.invariants() {
            let x = Var::plain(&d);
            let term = Term::Q(
                Quant::Exists,
                vec![x.clone()],
                Box::new(Term::Un(UnOp::Present, Box::new(Term::Ref(x)))),
            );
            for v in ignore_missing(ev.eval(&term))? {
                if v == Value::Bool(false) {
                    violations.push(Violation::Invariant(d.clone()));
                }
            }
        }
        Ok(())
    })();
    if let Err(e) = checked {
        outs.push(Output::Runtime(e));
        return None;
    }
    outs.extend(violations.into_iter().map(Output::Violation));
    let mut transitions = Vec::new();
    for (d, t) in &spec.decls {
        if !matches!(t.kind, Kind::Act { .. } | Kind::Event { .. }) {
            continue;
        }
        let found = ignore_missing((|| {
            // an instance that does not hold is not enabled; a closed type holds only for the
            // instances in the state, so the guided path asks those and leaves out the
            // combinations that would be listed as not enabled
            let tes = if spec.reading.guided && t.is_closed() {
                state2.instances_of(d).filter(|te| state2.holds(te)).collect()
            } else {
                ev.every_possible_subs(&Var::plain(d))?
            };
            bind(tes, |te| {
                let en = ev.is_enabled(&te)?;
                Ok(en.into_iter().rev().map(|e| (te.clone(), e)).collect())
            })
        })());
        match found {
            Ok(f) => transitions.extend(f),
            Err(e) => {
                outs.push(Output::Runtime(e));
                return None;
            }
        }
    }
    drop(ev);
    Some(Config { spec, state: state2, transitions, duties })
}

/// Every type a phrase names.
fn phrase_mentions(p: &Phrase, out: &mut BTreeSet<Id>) {
    match p {
        Phrase::Trigger(xs, t) | Phrase::Create(xs, t) | Phrase::Terminate(xs, t) | Phrase::Obfuscate(xs, t) | Phrase::InstQuery(_, xs, t) => {
            out.extend(xs.iter().map(|v| v.dom.clone()));
            mentions(t, out);
        }
        Phrase::Query(t) => mentions(t, out),
        Phrase::Par(ps) => ps.iter().for_each(|p| phrase_mentions(p, out)),
        // a declaration may read any type: all of them
        Phrase::Decls(_) => {
            out.insert(Id::from("*"));
        }
    }
}

/// Run one phrase. The configuration is unchanged (`None`) when the phrase fails.
pub fn step(cfg: &Config, p: &Phrase) -> (Vec<Output>, Option<Config>) {
    step_with(cfg, p, true)
}

/// `step`; not FULL, the next configuration lists no transitions and no violation is reported.
pub fn step_with(cfg: &Config, p: &Phrase, full: bool) -> (Vec<Output>, Option<Config>) {
    let mut outs = Vec::new();
    let mut spec = cfg.spec.clone();
    let compiled = match (Static { spec: &mut spec }).phrase(p) {
        Ok(c) => c,
        Err(errs) => {
            outs.push(Output::CompilationError(errs));
            return (outs, None);
        }
    };
    // not full, the acts and events no other rule reads were left underived: those the phrase names are derived now
    let derived;
    let state = if full {
        &cfg.state
    } else {
        let mut named = BTreeSet::new();
        phrase_mentions(p, &mut named);
        let lazy = lazy_types(&spec);
        let needed = if named.contains("*") { lazy } else { needed_lazy(&spec, &lazy, named) };
        if needed.is_empty() {
            &cfg.state
        } else {
            match saturate_only(&spec, cfg.state.clone(), &needed) {
                Ok(s) => {
                    derived = s;
                    &derived
                }
                Err(e) => {
                    outs.push(Output::Runtime(e));
                    return (outs, None);
                }
            }
        }
    };
    let Some(store) = sem_phrase(&compiled, &spec, state, &mut outs) else {
        return (outs, None);
    };
    let next = new_context(spec, state, &store, &mut outs, full);
    (outs, next)
}
