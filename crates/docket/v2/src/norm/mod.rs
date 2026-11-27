//! The normative core (spec 1, 8.2, 9): what holds is the state of eFLINT's reader, which reads the loaded Facts, Acts,
//! Events and Duties whole, beneath the machine.

mod host;
mod lower;
mod shape;
mod terms;
mod rules;
mod why;
#[cfg(test)]
mod why_tests;

pub use rules::cycle;

use self::shape::Shapes;
use self::terms::Lower;
use crate::run::explain::{Explain, WhyFailed};
use crate::run::norms::{Moved, NormDecl, Norms, Step, Why};
use crate::run::value::{Env, Instance, Value};
use crate::syntax::form;
use crate::syntax::tree::Expr;
use eflint::ast::{Id, Tagged, Term, id};
use eflint::compile::{Host, Reading, Spec};
use eflint::eval::{mentions, reads};
use eflint::replay::{Moves, Refusal, Replay, Taken};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

/// How eFLINT works the loaded types out: a variable ranges over what holds (6.2), a derived type is found from the
/// facts its rule names.
const READING: Reading = Reading { guided: true, ranging: true };

/// eFLINT's reader beneath the machine.
pub struct Core {
    loaded: Option<(Replay, Shapes)>,
    /// The loaded declarations as written: what an explanation reads its rules from.
    decls: Vec<NormDecl>,
    /// Each type's declarations, extensions and stated instances in their written form, hashed: what a read of the
    /// type answers by besides the state.
    rules: BTreeMap<Id, [u8; 32]>,
    /// The step `step` worked out and took back, for `commit` to make again.
    pending: Option<(Instance, Taken)>,
    /// Every committed step in order, for `view` to take back and make again; a reload keeps them.
    done: Vec<Taken>,
    /// Each instance's steps of `done` (numbered from 1, in order) that made it and that ended it: an explanation's
    /// question of which step last did either, answered for any viewed state by a search, never a pass over the steps.
    changes: BTreeMap<Tagged, (Vec<usize>, Vec<usize>)>,
    /// What each reload left out (the instances of the types its text does not declare), after the number of steps
    /// it followed: taken back and made again with those steps.
    left: Vec<(usize, Taken)>,
    /// How many of `done` the state stands after: all of them unless a past state is viewed.
    at: usize,
    /// What was read since `reads` last answered; `None` when nothing was.
    read: Option<Sha256>,
    /// Whether each commit and load checks Σ against the whole derivation, and whether that is worked out guided
    /// ([`Core::checked`], [`Core::checked_guided`]).
    check: Option<bool>,
    /// What moved since `moved` was last asked, while anyone counts.
    moves: Option<Moves>,
    /// The bindings `each` may try ([`Norms::cap`]).
    cap: Option<u64>,
}

impl Default for Core {
    fn default() -> Core {
        Core { loaded: None, decls: Vec::new(), rules: BTreeMap::new(), pending: None, done: Vec::new(), changes: BTreeMap::new(), left: Vec::new(), at: 0, read: None, check: None, moves: None, cap: None }
    }
}

/// Note in MOVES, if anyone counts, what DELTA moved in R.
fn noted(r: &mut Replay, moves: &mut Option<Moves>, delta: eflint::eval::Delta) {
    if let Some(m) = moves {
        r.note(&delta, m);
    }
}

impl Core {
    /// A core for tests and the gate, never the daemon's build, that at each commit and load checks what holds as worked
    /// out step by step against every derived type worked out again from the stored facts trying every binding, and
    /// panics at the first instance they part on.
    pub fn checked() -> Core {
        Core { check: Some(false), ..Core::default() }
    }

    /// [`Core::checked`] against the whole worked out guided, as the core reads: the same bindings the steps find, far
    /// faster on a large state, so a fault in finding them is not seen.
    pub fn checked_guided() -> Core {
        Core { check: Some(true), ..Core::default() }
    }

    /// The loaded text as eFLINT reads it, every derived type worked out: for a reading of it beside the machine (a
    /// check, an export).
    pub fn eflint(&mut self) -> Option<Result<(&Spec, &eflint::eval::State), String>> {
        let (r, _) = self.loaded.as_mut()?;
        if let Err(e) = r.state() {
            return Some(Err(e));
        }
        Some(Ok((&r.cfg.spec, &r.cfg.state)))
    }

    /// The bindings `each` has tried its conditions under on the text loaded last ([`Replay::tried`]): what an `each`
    /// costs.
    pub fn tried(&self) -> u64 {
        self.loaded.as_ref().map_or(0, |(r, _)| r.tried())
    }

    fn check(&mut self) {
        let (Some((r, _)), Some(guided)) = (&mut self.loaded, self.check) else { return };
        match r.unlike_whole(guided) {
            Ok(None) => {}
            Ok(Some(te)) => panic!("after step {}, {} holds worked out step by step or worked out whole, not both", self.done.len(), shape::listed(&te)),
            Err(e) => panic!("after step {}, worked out whole: {e}", self.done.len()),
        }
    }
}

/// What every state hash starts from: the host table's version (6.3).
fn fresh() -> Sha256 {
    let mut h = Sha256::new();
    h.update(crate::run::host::VERSION.to_le_bytes());
    h
}

/// A read: its question, its answer, and the rules of the types it read.
fn note(read: &mut Option<Sha256>, rules: &BTreeMap<Id, [u8; 32]>, spec: &Spec, named: BTreeSet<Id>, what: String) {
    let read = read.get_or_insert_with(fresh);
    read.update(what.as_bytes());
    for d in reads(spec, named) {
        if let Some(h) = rules.get(&d) {
            read.update(h);
        }
    }
}

fn named(t: &Term) -> BTreeSet<Id> {
    let mut out = BTreeSet::new();
    mentions(t, &mut out);
    out
}

/// Whether C holds under ENV, and C as eFLINT reads it: what `cond` asks, before it is noted.
fn ask(r: &mut Replay, shapes: &Shapes, c: &Expr, env: &Env) -> Result<(bool, Term), String> {
    let t = lowered(shapes, c, env)?;
    Ok((r.ask(&t)?, t))
}

/// Whether C holds under ENV, `None` where it turns on instances the well-founded reading leaves unknown.
fn truth(r: &mut Replay, shapes: &Shapes, c: &Expr, env: &Env) -> Result<Option<bool>, String> {
    r.truth(&lowered(shapes, c, env)?)
}

/// C under ENV as eFLINT reads it.
fn lowered(shapes: &Shapes, c: &Expr, env: &Env) -> Result<Term, String> {
    lower::enumerated(shapes, &|x| env.contains_key(x), c)?;
    Lower::new(shapes, env, &[]).cond(c)
}

/// Every way C holds under ENV, at most N, refused past STEPS bindings tried: each binding of its free variables, its
/// row of values, and C as eFLINT reads it.
fn ways(r: &mut Replay, shapes: &Shapes, c: &Expr, env: &Env, n: usize, steps: u64) -> Result<(Vec<Env>, Vec<Vec<eflint::ast::Tagged>>, Term), String> {
    lower::enumerated(shapes, &|x| env.contains_key(x), c)?;
    let mut lw = Lower::new(shapes, env, &[]);
    let t = lw.cond(c)?;
    let xs = lw.bound(&lw.free)?;
    let rows = r.ways_bounded(&xs, &t, n, steps)?;
    let bind = |row: &Vec<eflint::ast::Tagged>| {
        let mut e = env.clone();
        e.extend(lw.free.iter().cloned().zip(row.iter().map(shape::value)));
        e
    };
    Ok((rows.iter().map(bind).collect(), rows, t))
}

/// Condition C in its written form, with the values ENV gives the variables it names.
fn asked(c: &Expr, env: &Env) -> String {
    let given: String = env.iter().filter(|(k, _)| lower::mentions(c, k)).map(|(k, v)| format!(" {k} = {v}")).collect();
    format!("{}{given}", form::expr(c))
}

impl Norms for Core {
    fn load(&mut self, decls: &[NormDecl]) -> Result<(), String> {
        self.view(None);
        let mut shapes = Shapes::of(decls)?;
        let lowered = lower::decls(&mut shapes, decls)?;
        let mut rules: BTreeMap<Id, Sha256> = BTreeMap::new();
        for d in decls {
            if let Some(ty) = lower::type_of(d) {
                rules.entry(id(&ty)).or_default().update(format!("{}\n", form::decl(&d.decl.what)));
            }
        }
        let tests = shapes.tests.iter().map(|t| id(t)).collect();
        let spec = Spec { reading: READING, host: Host(Some(Rc::new(host::Table))), tests, ..Spec::new() };
        let mut r = Replay::declare(lowered, spec)?;
        if let Some((old, _)) = &self.loaded {
            if let Some(gone) = r.keep_stored(old) {
                self.left.push((self.done.len(), gone));
            }
        }
        self.loaded = Some((r, shapes));
        if let Some(m) = &mut self.moves {
            m.all = true;
        }
        self.decls = decls.to_vec();
        self.rules = rules.into_iter().map(|(t, h)| (t, h.finalize().into())).collect();
        self.pending = None;
        self.check();
        Ok(())
    }

    fn holds(&mut self, i: &Instance) -> bool {
        let Some((r, shapes)) = &mut self.loaded else { return false };
        let got = shapes.instance(i).and_then(|te| r.holds(&te));
        let answer = got.as_ref().map_or_else(|e| format!("error {e}"), bool::to_string);
        note(&mut self.read, &self.rules, &r.cfg.spec, BTreeSet::from([id(&i.ty)]), format!("holds {i}: {answer}\n"));
        got.unwrap_or(false)
    }

    fn cond(&mut self, c: &Expr, env: &Env) -> Result<bool, String> {
        let Some((r, shapes)) = &mut self.loaded else { return Err("no text is loaded".into()) };
        let (got, t) = ask(r, shapes, c, env)?;
        note(&mut self.read, &self.rules, &r.cfg.spec, named(&t), format!("cond {}: {got}\n", asked(c, env)));
        Ok(got)
    }

    fn each(&mut self, c: &Expr, env: &Env) -> Result<Vec<Env>, String> {
        let Some((r, shapes)) = &mut self.loaded else { return Err("no text is loaded".into()) };
        let (envs, rows, t) = ways(r, shapes, c, env, usize::MAX, self.cap.unwrap_or(u64::MAX))?;
        let written: Vec<String> = rows.iter().map(|row| row.iter().map(|te| shape::value(te).to_string()).collect::<Vec<_>>().join(", ")).collect();
        note(&mut self.read, &self.rules, &r.cfg.spec, named(&t), format!("each {}: {}\n", asked(c, env), written.join("; ")));
        Ok(envs)
    }

    fn step(&mut self, act: &Instance, party: Option<&Value>) -> Result<Step, Why> {
        let Some((r, shapes)) = &mut self.loaded else { return Err(Why::Policy("no text is loaded".into())) };
        let event = shapes.event(act).map_err(Why::Policy)?;
        let holder = party.map(|p| shape::elem(shapes, p)).transpose().map_err(Why::Policy)?;
        let got = r.step(&event, holder.as_ref()).map_err(|refused| match refused {
            Refusal::Policy => Why::Policy("not enabled".into()),
            Refusal::Closes(d) => Why::Closes(shape::listed(&d).to_string()),
            Refusal::Error(es) => Why::Policy(es.join("; ")),
            Refusal::Undecided(why) => Why::Policy(why),
        });
        let outcome = match &got {
            Ok(_) => "kept".to_string(),
            Err(Why::Policy(w)) => format!("refused: {w}"),
            Err(Why::Closes(d)) => format!("refused: it closes {d}"),
        };
        note(&mut self.read, &self.rules, &r.cfg.spec, BTreeSet::from([id(&act.ty)]), format!("step {act}: {outcome}\n"));
        let taken = got?;
        let step = Step { act: act.clone(), created: taken.created.iter().map(shape::listed).collect(), terminated: taken.terminated.iter().map(shape::listed).collect() };
        r.undo(&taken);
        self.pending = Some((act.clone(), taken));
        Ok(step)
    }

    /// The step worked out last, made again; any other is the caller's fault, and the state would not be the one it
    /// believes.
    fn commit(&mut self, step: Step) {
        assert_eq!(self.at, self.done.len(), "commit of {} while a past state is viewed", step.act);
        match (&mut self.loaded, self.pending.take()) {
            (Some((r, _)), Some((act, taken))) if act == step.act => {
                let d = r.redo(&taken);
                noted(r, &mut self.moves, d);
                let n = self.done.len() + 1;
                taken.created.iter().for_each(|x| self.changes.entry(x.clone()).or_default().0.push(n));
                taken.terminated.iter().for_each(|x| self.changes.entry(x.clone()).or_default().1.push(n));
                self.done.push(taken);
                self.at += 1;
            }
            _ => panic!("commit of {}, which is not the step worked out last", step.act),
        }
        self.check();
    }

    /// The committed steps taken back from the last or made again up to K, each with what the reloads after it left
    /// out; a step worked out on another state is dropped.
    fn view(&mut self, k: Option<usize>) {
        let k = k.unwrap_or(self.done.len());
        assert!(k <= self.done.len(), "a view after step {k}, of {} committed", self.done.len());
        let Some((r, _)) = &mut self.loaded else { return };
        if k != self.at {
            self.pending = None;
        }
        while self.at > k {
            for (_, gone) in self.left.iter().rev().filter(|(n, _)| *n == self.at) {
                let d = r.undo(gone);
                noted(r, &mut self.moves, d);
            }
            self.at -= 1;
            let d = r.undo(&self.done[self.at]);
            noted(r, &mut self.moves, d);
        }
        while self.at < k {
            let d = r.redo(&self.done[self.at]);
            noted(r, &mut self.moves, d);
            self.at += 1;
            for (_, gone) in self.left.iter().filter(|(n, _)| *n == self.at) {
                let d = r.redo(gone);
                noted(r, &mut self.moves, d);
            }
        }
    }

    fn reads(&mut self) -> [u8; 32] {
        self.read.take().map_or([0; 32], |h| h.finalize().into())
    }

    fn moved(&mut self) -> Moved {
        let all = Moved { all: true, ..Moved::default() };
        let (Some(m), Some((r, _))) = (self.moves.replace(Moves::default()), &mut self.loaded) else { return all };
        let Ok(got) = r.moved(m) else { return all };
        let listed = |tes: &[eflint::ast::Tagged]| tes.iter().map(shape::listed).collect();
        let duties = got.duties.iter().map(|(te, v)| (shape::listed(te), *v)).collect();
        Moved { all: got.all, made: listed(&got.made), ended: listed(&got.ended), whole: got.whole.iter().map(|t| t.to_string()).collect(), duties }
    }

    fn unwatch(&mut self) {
        self.moves = None;
    }

    fn cap(&mut self, steps: Option<u64>) {
        self.cap = steps;
    }

    fn explain(&mut self, i: &Instance) -> Result<Explain, String> {
        why::Asker::of(self, why::BUDGET, why::TRIES)?.explain(i)
    }

    fn why_refused(&mut self, act: &Instance) -> Result<WhyFailed, String> {
        why::Asker::of(self, why::BUDGET, why::TRIES)?.refused(act)
    }
}
