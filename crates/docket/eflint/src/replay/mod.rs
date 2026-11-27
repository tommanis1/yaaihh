//! Replay: perform a record of events in order, each on the state the one before it left.
//!
//! A record is an ordered list of [`Event`]s: acts (with actor and objects), events, and facts the
//! surrounding system supplies (a lapse of time, a measurement, a role). After each, the
//! [`Answer`] says whether it was performed, what it performed and the answers to its questions;
//! [`Replay::duties`] gives the duties held and whether each is violated. [`Replay::step`] keeps an act or event only
//! if it is enabled and ends no duty its holder holds, and [`Replay::undo`] takes a kept one back.

use crate::ast::*;
use crate::compile::{Reading, Spec};
use crate::eval::{assign, derived_types, saturate_only_in, Ev, Info, Plan, State};
use crate::interp::{freshen, step_in_place, step_with, transition_in_place, transition_may, Config, Output, Violation};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

mod ask;
mod checked;
mod moves;

pub use self::checked::*;
pub use self::moves::{Moved, Moves};

#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    Str(String),
    Int(i64),
    /// The fields of an instance of a composite type.
    List(Vec<Value>),
}

#[derive(Clone, PartialEq, Debug)]
pub enum Event {
    Act { name: String, actor: Value, objects: Vec<Value> },
    Event { name: String, objects: Vec<Value> },
    /// `holds`: `Some(true)` creates the fact, `Some(false)` terminates it, `None` forgets it.
    Fact { name: String, objects: Vec<Value>, holds: Option<bool> },
    /// Any text of phrases, for what the three forms above do not cover (queries, declarations).
    Phrase(String),
}

#[derive(Clone, PartialEq, Debug)]
pub struct DutyState {
    pub duty: Tagged,
    /// Surely violated: not where its violation turns on unknown instances.
    pub violated: bool,
    /// Violated only once unknown instances are taken to hold: neither violated nor kept.
    pub undecided: bool,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Answer {
    /// Whether the event was performed (false: an error, the state is unchanged).
    pub ok: bool,
    pub errors: Vec<String>,
    /// The acts and events performed, with those they synchronise with.
    pub executed: Vec<Tagged>,
    /// Acts that were performed although they were not enabled.
    pub disabled: Vec<Tagged>,
    /// The answers of the queries in a phrase event.
    pub queries: Vec<bool>,
    pub instances: Vec<Vec<Tagged>>,
}

#[derive(Clone)]
pub struct Replay {
    pub cfg: Config,
    /// The derived types whose instances in `cfg.state` may be out of date: `perform` works a derived type out only
    /// when a phrase, `state` or `duties` reads it.
    stale: BTreeSet<Id>,
    /// How a step's changes reach the derived types, for the specification it was last worked out for.
    plan: Option<Rc<Plan>>,
    /// The bindings `ways` has tried its terms under ([`Replay::tried`]).
    tried: u64,
}

fn collect_executed(info: &crate::eval::TransInfo, out: &mut Vec<Tagged>) {
    out.push(info.tagged.clone());
    for s in &info.syncs {
        collect_executed(s, out);
    }
}

impl Replay {
    /// Load a specification from text (no `#include`): its phrases performed in order, as a record's lines are.
    pub fn from_text(text: &str) -> Result<Replay, String> {
        Replay::from_text_as(text, Reading::default())
    }

    /// [`Replay::from_text`], its types worked out as READING says.
    pub fn from_text_as(text: &str, reading: Reading) -> Result<Replay, String> {
        let mut replay = Replay { cfg: Config::default(), stale: BTreeSet::new(), plan: None, tried: 0 };
        replay.cfg.spec.reading = reading;
        let a = replay.perform(&Event::Phrase(text.to_string()));
        if a.ok {
            Ok(replay)
        } else {
            Err(a.errors.join("\n"))
        }
    }

    /// The term for a value where an instance of type `ty` is expected.
    fn term(&self, ty: &Id, v: &Value) -> Result<Term, String> {
        let Some(tspec) = self.cfg.spec.find(ty) else { return Err(format!("undeclared type: {}", ty)) };
        match (v, &tspec.domain) {
            (Value::List(vs), Domain::Products(xs)) => {
                if vs.len() != xs.len() {
                    return Err(format!("{} has {} fields, {} given", ty, xs.len(), vs.len()));
                }
                let args: Result<Vec<Term>, String> =
                    xs.iter().zip(vs).map(|(x, v)| self.term(&self.cfg.spec.type_of(x), v)).collect();
                Ok(Term::App(ty.clone(), Args::Pos(args?)))
            }
            (Value::Str(s), Domain::AnyString | Domain::Strings(_)) => {
                Ok(Term::App(ty.clone(), Args::Pos(vec![Term::StringLit(id(s))])))
            }
            (Value::Int(i), Domain::AnyInt | Domain::Ints(_)) => {
                Ok(Term::App(ty.clone(), Args::Pos(vec![Term::IntLit(*i)])))
            }
            (Value::List(_), _) => Err(format!("{} is not a composite type, a list was given", ty)),
            (Value::Str(s), _) => Err(format!("{} does not take the string {:?}", ty, s)),
            (Value::Int(i), _) => Err(format!("{} does not take the number {}", ty, i)),
        }
    }

    fn instance(&self, name: &str, objects: &[Value]) -> Result<Term, String> {
        let ty = id(name);
        let Some(tspec) = self.cfg.spec.find(&ty) else { return Err(format!("undeclared type: {}", name)) };
        match &tspec.domain {
            Domain::Products(_) => self.term(&ty, &Value::List(objects.to_vec())),
            _ if objects.len() == 1 => self.term(&ty, &objects[0]),
            _ => Err(format!("{} is a plain type and takes one value, {} given", name, objects.len())),
        }
    }

    fn phrases(&self, e: &Event) -> Result<Vec<Phrase>, String> {
        let kind_of = |name: &str| self.cfg.spec.find(&id(name)).map(|t| t.kind.clone());
        match e {
            Event::Act { name, actor, objects } => {
                if !matches!(kind_of(name), Some(Kind::Act { .. })) {
                    return Err(format!("{} is not an act", name));
                }
                let mut all = vec![actor.clone()];
                all.extend(objects.iter().cloned());
                Ok(vec![Phrase::Trigger(Vec::new(), self.instance(name, &all)?)])
            }
            Event::Event { name, objects } => {
                if !matches!(kind_of(name), Some(Kind::Event { .. })) {
                    return Err(format!("{} is not an event", name));
                }
                Ok(vec![Phrase::Trigger(Vec::new(), self.instance(name, objects)?)])
            }
            Event::Fact { name, objects, holds } => {
                let t = self.instance(name, objects)?;
                Ok(vec![match holds {
                    Some(true) => Phrase::Create(Vec::new(), t),
                    Some(false) => Phrase::Terminate(Vec::new(), t),
                    None => Phrase::Obfuscate(Vec::new(), t),
                }])
            }
            Event::Phrase(text) => {
                let statements = crate::parser::parse_statements(text)?;
                statements
                    .into_iter()
                    .map(|i| match i {
                        Statement::Phrase(p) => Ok(p),
                        _ => Err("#include and #require are not events".to_string()),
                    })
                    .collect()
            }
        }
    }

    /// Perform one event and answer whether it was performed, what it performed and its questions' answers: no state
    /// is compared, no duty read and no transition listed. The state is changed in place and a derived fact is worked
    /// out only when something reads it, so a line costs the same however long the record before it.
    pub fn perform(&mut self, e: &Event) -> Answer {
        let mut a = Answer { ok: true, ..Answer::default() };
        let phrases = match self.phrases(e) {
            Ok(p) => p,
            Err(msg) => {
                a.ok = false;
                a.errors.push(msg);
                return a;
            }
        };
        for p in &phrases {
            let (outs, done) = step_in_place(&mut self.cfg, &mut self.stale, p);
            take_outputs(outs, &mut a);
            if !done {
                a.ok = false;
                break;
            }
        }
        a
    }

    /// `perform` the long way: the state copied, and every derived type but the acts and events nobody reads worked
    /// out again from the stored facts. The answers `perform` must give (tests).
    pub fn perform_from_nothing(&mut self, e: &Event) -> Answer {
        if let Err(err) = freshen(&mut self.cfg, &mut self.stale, None) {
            return Answer { ok: false, errors: vec![err.message()], ..Answer::default() };
        }
        let mut a = Answer { ok: true, ..Answer::default() };
        match self.phrases(e) {
            Err(msg) => {
                a.ok = false;
                a.errors.push(msg);
            }
            Ok(phrases) => {
                for p in &phrases {
                    let (outs, next) = step_with(&self.cfg, p, false);
                    take_outputs(outs, &mut a);
                    match next {
                        Some(cfg) => self.cfg = cfg,
                        None => {
                            a.ok = false;
                            break;
                        }
                    }
                }
            }
        }
        self.stale = crate::eval::lazy_types(&self.cfg.spec);
        a
    }

    /// The state with every derived type worked out.
    pub fn state(&mut self) -> Result<&State, String> {
        freshen(&mut self.cfg, &mut self.stale, None).map_err(|e| e.message())?;
        Ok(&self.cfg.state)
    }

    /// A derived instance that holds in the state as worked out so far and not with every derived type worked out
    /// again from the stored facts, GUIDED or trying every binding, or the other way round; `None` where the two agree.
    pub fn unlike_whole(&mut self, guided: bool) -> Result<Option<Tagged>, String> {
        freshen(&mut self.cfg, &mut self.stale, None).map_err(|e| e.message())?;
        let spec = Spec { reading: Reading { guided, ..self.cfg.spec.reading }, ..self.cfg.spec.clone() };
        let types = derived_types(&spec);
        let mut whole = self.cfg.state.clone();
        whole.retain_underived_of(&types);
        saturate_only_in(&spec, &mut whole, &types).map_err(|e| e.message())?;
        let held = |s: &State| -> BTreeSet<Tagged> { types.iter().flat_map(|d| s.instances_of(d).filter(|te| s.holds(te)).collect::<Vec<_>>()).collect() };
        Ok(held(&self.cfg.state).symmetric_difference(&held(&whole)).next().cloned())
    }

    /// The duties that hold, each with whether it is violated. The duty types, and what their rules and violations
    /// read, are worked out first if stale.
    pub fn duties(&mut self) -> Vec<DutyState> {
        let duty_types = self.duty_types();
        if !self.stale.is_empty() {
            let read = crate::eval::reads(&self.cfg.spec, duty_types.clone());
            if freshen(&mut self.cfg, &mut self.stale, Some(&read)).is_err() {
                return Vec::new();
            }
        }
        let state = &self.cfg.state;
        let mut held: Vec<Tagged> =
            duty_types.iter().flat_map(|d| state.instances_of(d).filter(|te| state.holds(te)).collect::<Vec<_>>()).collect();
        held.sort();
        self.cfg.duties = held;
        let (spec, state) = (&self.cfg.spec, &self.cfg.state);
        let mut ev = Ev::new(spec, state);
        let mut may = (!state.unknown().is_empty()).then(|| Ev::upper(spec, state));
        let violated = |ev: &mut Ev, te: &Tagged| ev.violation_condition(te).is_ok_and(|vs| vs.into_iter().any(|v| v));
        self.cfg
            .duties
            .iter()
            .map(|te| {
                let sure = violated(&mut ev, te);
                let undecided = !sure && may.as_mut().is_some_and(|m| violated(m, te));
                DutyState { duty: te.clone(), violated: sure, undecided }
            })
            .collect()
    }
}

/// What a step reported, into an answer.
fn take_outputs(outs: Vec<Output>, a: &mut Answer) {
    for o in outs {
        match o {
            Output::CompilationError(errs) => a.errors.extend(errs),
            Output::NotTriggerable(d) => {
                a.ok = false;
                a.errors.push(format!("not a triggerable (act- or event-) type: {}", d))
            }
            Output::Runtime(err) => a.errors.push(err.message()),
            Output::Executed(info) => collect_executed(&info, &mut a.executed),
            Output::Violation(Violation::Trigger(info)) => a.disabled.push(info.tagged),
            Output::Violation(Violation::Invariant(_)) | Output::Violation(Violation::Duty(_)) => {}
            Output::Query(b) => a.queries.push(b),
            // a reader of two values takes an unknown instance as not holding
            Output::Unknown => a.queries.push(false),
            Output::InstQuery(tes) => a.instances.push(tes),
        }
    }
}
