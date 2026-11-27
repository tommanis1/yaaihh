//! Justification trees over the loaded rules and Σ: each premise asked as `cond` asks it, but never noted into a state hash.

use super::lower::conjuncts;
use super::rules::{Derivation, Read, Rules, ors, shown, span, unify, walk};
use super::shape::{self, Shapes};
use super::{Core, ask, truth, ways};
use crate::run::explain::{Done, Explain, Holds, Link, Premise, Rule, WhyFailed};
use crate::run::value::{Env, Instance};
use crate::syntax::form;
use crate::syntax::tree::{Clause, E, Expr, Op, Quant};
use eflint::ast::Tagged;
use eflint::replay::{Replay, Taken};
use std::collections::{BTreeMap, BTreeSet};

type R<T> = Result<T, String>;

/// How many questions one explanation puts to the state before it marks the rest cut: a bound on its cost and size.
pub(super) const BUDGET: usize = 4096;
/// How many bindings of a rule's body, or witnesses of an `Exists`, are tried for a proof that does not stand on itself.
pub(super) const TRIES: usize = 8;

/// One explanation's reader: rules, committed steps and the steps each instance changed at, the instances on its
/// path, whether a proof stood on it or was cut, the budget left, and whether each act whose refusal it gave refuses.
pub(super) struct Asker<'c> {
    r: &'c mut Replay,
    shapes: &'c Shapes,
    rules: Rules<'c>,
    done: &'c [Taken],
    changes: &'c BTreeMap<Tagged, (Vec<usize>, Vec<usize>)>,
    path: Vec<Instance>,
    looped: bool,
    cut: bool,
    left: usize,
    tries: usize,
    refusals: BTreeMap<Instance, bool>,
}

impl<'c> Asker<'c> {
    /// An explanation of CORE's state that asks at most BUDGET questions and tries TRIES bindings for each proof.
    pub(super) fn of(core: &'c mut Core, budget: usize, tries: usize) -> R<Asker<'c>> {
        let Core { loaded, decls, done, changes, at, .. } = core;
        let Some((r, shapes)) = loaded else { return Err("no text is loaded".into()) };
        let (rules, done) = (Rules::of(decls), &done[..*at]);
        Ok(Asker { r, shapes, rules, done, changes, path: Vec::new(), looped: false, cut: false, left: budget, tries, refusals: BTreeMap::new() })
    }

    /// One question's worth of the budget: false, and the explanation marked cut, when none is left.
    fn spend(&mut self) -> bool {
        if self.left == 0 {
            self.cut = true;
            return false;
        }
        self.left -= 1;
        true
    }

    /// Whether C holds under ENV, false where it turns on an unknown instance (it meets no condition); `None` when the
    /// budget is spent.
    fn ask(&mut self, c: &Expr, env: &Env) -> R<Option<bool>> {
        if !self.spend() {
            return Ok(None);
        }
        Ok(Some(truth(self.r, self.shapes, c, env)?.unwrap_or(false)))
    }

    /// At most N bindings of C's free variables under which it holds, ENV alone if it has none and holds; `None` when the budget is spent.
    fn bindings(&mut self, c: &Expr, env: &Env, n: usize) -> R<Option<Vec<Env>>> {
        if !self.spend() {
            return Ok(None);
        }
        Ok(Some(match ways(self.r, self.shapes, c, env, n, u64::MAX) {
            Ok((envs, _, _)) => envs,
            Err(_) => match ask(self.r, self.shapes, c, env) {
                Ok((true, _)) => vec![env.clone()],
                _ => Vec::new(),
            },
        }))
    }

    /// The last step of the viewed state that made TE hold (MADE) or ended it, searched for in Core's index.
    fn changed(&self, te: &Tagged, made: bool) -> Done {
        let at = self.done.len();
        let steps = self.changes.get(te).map_or(&[][..], |(m, e)| if made { m } else { e });
        let step = steps[..steps.partition_point(|&n| n <= at)].last().copied().unwrap_or(0);
        Done { step, act: step.checked_sub(1).and_then(|k| self.done[k].executed.first()).map(shape::listed) }
    }

    /// Why I holds, does not, or has no single answer.
    pub(super) fn explain(&mut self, i: &Instance) -> R<Explain> {
        let te = self.shapes.instance(i)?;
        let holds = self.r.holds(&te)?;
        if self.r.cfg.state.unknown().contains(&te) {
            return Ok(Explain::Unknown { inst: i.clone(), cycle: self.unknown(i).unwrap_or_default() });
        }
        let stored = self.r.cfg.state.contents().get(&te).filter(|info| !info.derived).map(|info| info.value);
        self.path.push(i.clone());
        let out = match (holds, stored) {
            (true, Some(true)) => Ok(Explain::True { inst: i.clone(), by: Holds::Stated(self.changed(&te, true)) }),
            (true, _) => self.proof(i).map(|by| Explain::True { inst: i.clone(), by: by.unwrap_or(Holds::Unproved { tries: self.tries, cut: false }) }),
            (false, Some(false)) => {
                let ended = Some(self.changed(&te, false));
                let overridden = self.proof(i)?.map(Box::new);
                Ok(Explain::False { inst: i.clone(), ended, overridden, rules: Vec::new() })
            }
            (false, _) => self.refutation(i).map(|rules| Explain::False { inst: i.clone(), ended: None, overridden: None, rules }),
        };
        self.path.pop();
        out
    }

    /// A cycle through `Not` from I back to itself, looked for only where I's type reads itself falling.
    fn unknown(&self, i: &Instance) -> Option<Vec<Link>> {
        let mut seen = BTreeSet::from([(i.ty.clone(), false)]);
        let mut todo = vec![(i.ty.clone(), false)];
        while let Some((ty, neg)) = todo.pop() {
            for (_, d) in self.rules.derivations(&ty) {
                let mut reads = Vec::new();
                self.rules.reads(d.whole(), false, &mut reads);
                for (read, negated) in reads {
                    let next = match read {
                        Read::Domain(t, _) => (t, neg || negated),
                        Read::Atom(a) => {
                            let E::Apply(p, _) = &(if let E::Holds(x) = &a.e { x } else { a }).e else { continue };
                            (p.to_string(), neg || negated)
                        }
                    };
                    if next == (i.ty.clone(), true) {
                        let mut path = Vec::new();
                        let mut seen = BTreeSet::from([(i.clone(), false)]);
                        return walk(&self.rules, i, i, false, &mut path, &mut seen).then_some(path);
                    }
                    if seen.insert(next.clone()) {
                        todo.push(next);
                    }
                }
            }
        }
        None
    }

    /// What makes I hold: its module's statement, or the first rule met by a whole proof that does not stand on itself;
    /// `Unproved` if a rule is met but no such proof is found, and `None` if no rule is met.
    fn proof(&mut self, i: &Instance) -> R<Option<Holds>> {
        if let Some(at) = self.rules.postulate(i) {
            return Ok(Some(Holds::Postulated { at }));
        }
        let (mut met, mut stood, mut cut) = (false, false, false);
        for (m, d) in self.rules.derivations(&i.ty) {
            let (env, body) = match d {
                Derivation::When(e) => (self.rules.head(i), Some(e)),
                Derivation::From(_, v, w) => match unify(&self.rules, v, i) {
                    Some(env) => (env, w),
                    None => continue,
                },
            };
            let Some(body) = body else {
                return Ok(Some(Holds::Rule(Rule { at: span(m, d.whole().at), text: d.text(), env, premises: Vec::new(), cut: false })));
            };
            let Some(envs) = self.bindings(body, &env, self.tries)? else {
                cut = true;
                continue;
            };
            for env in envs {
                met = true;
                let was = (std::mem::replace(&mut self.looped, false), std::mem::replace(&mut self.cut, false));
                let (premises, _) = self.premises(body, &env, m, true)?;
                let (l, c) = (std::mem::replace(&mut self.looped, was.0), std::mem::replace(&mut self.cut, was.1));
                if !l && !c {
                    return Ok(Some(Holds::Rule(Rule { at: span(m, body.at), text: d.text(), env: shown(body, &env), premises, cut: false })));
                }
                (stood, cut) = (stood || l, cut || c);
            }
        }
        // a proof that stands on the path, or that the budget cut, leaves whoever asked for this one standing on it too
        (self.looped, self.cut) = (self.looped || stood, self.cut || cut);
        Ok((met || cut).then_some(Holds::Unproved { tries: self.tries, cut }))
    }

    /// Each rule for I with the premises that fail, and each failing `Conditioned by` of its type.
    fn refutation(&mut self, i: &Instance) -> R<Vec<Rule>> {
        let mut out = Vec::new();
        for (m, d) in self.rules.derivations(&i.ty) {
            let (env, body) = match d {
                Derivation::When(e) => (self.rules.head(i), e),
                Derivation::From(_, v, Some(w)) => match unify(&self.rules, v, i) {
                    Some(env) => (env, w),
                    None => {
                        out.push(Rule { at: span(m, v.at), text: d.text(), env: Env::new(), premises: Vec::new(), cut: false });
                        continue;
                    }
                },
                Derivation::From(_, v, None) => {
                    out.push(Rule { at: span(m, v.at), text: d.text(), env: Env::new(), premises: Vec::new(), cut: false });
                    continue;
                }
            };
            let (premises, cut) = self.premises(body, &env, m, false)?;
            out.push(Rule { at: span(m, body.at), text: d.text(), env: shown(body, &env), premises, cut });
        }
        let env = self.rules.head(i);
        for (m, c) in self.rules.of_type(&i.ty) {
            if let Clause::ConditionedBy(e) = c {
                let (premises, cut) = self.premises(e, &env, m, false)?;
                if !premises.is_empty() || cut {
                    out.push(Rule { at: span(m, e.at), text: format!("Conditioned by {}", form::expr(e)), env: shown(e, &env), premises, cut });
                }
            }
        }
        Ok(out)
    }

    /// Why Act or Event ACT is not enabled: its failing `Conditioned by` conjuncts and the syncs that are not enabled,
    /// each synced act explained once in the answer.
    pub(super) fn refused(&mut self, act: &Instance) -> R<WhyFailed> {
        self.shapes.instance(act)?;
        let env = self.rules.head(act);
        self.path.push(act.clone());
        let mut out = WhyFailed { act: act.clone(), failing: Vec::new(), syncs: Vec::new(), again: false, cut: false };
        for (m, c) in self.rules.of_type(&act.ty) {
            match c {
                Clause::ConditionedBy(e) => {
                    let (premises, cut) = self.premises(e, &env, m, false)?;
                    out.failing.extend(premises);
                    out.cut |= cut;
                }
                Clause::SyncsWith(xs) => {
                    for x in xs {
                        let on = match &x.when {
                            Some(w) => self.ask(w, &env)?,
                            None => Some(true),
                        };
                        let Some(on) = on else {
                            out.cut = true;
                            continue;
                        };
                        let Some(j) = self.rules.ground(&x.inst, &env).filter(|j| on && !self.path.contains(j)) else { continue };
                        match self.refusals.get(&j).copied() {
                            Some(true) => out.syncs.push(WhyFailed { act: j, failing: Vec::new(), syncs: Vec::new(), again: true, cut: false }),
                            Some(false) => {}
                            None if !self.spend() => out.cut = true,
                            None => {
                                let w = self.refused(&j)?;
                                self.refusals.insert(j, w.refuses());
                                if w.refuses() {
                                    out.syncs.push(w);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        self.path.pop();
        Ok(out)
    }

    /// C's conjuncts as premises when C's truth under ENV is WANT (known to hold when WANT): all if it holds, else the
    /// failing ones; where none fails alone, C under each binding of the first conjunct's other variables that holds,
    /// and else C whole. With it, whether the budget cut any of it.
    fn premises(&mut self, c: &Expr, env: &Env, m: &str, want: bool) -> R<(Vec<Premise>, bool)> {
        let was = std::mem::replace(&mut self.cut, false);
        let out = self.asked(c, env, m, want);
        let cut = self.cut;
        self.cut |= was;
        Ok((out?, cut))
    }

    /// What `premises` gives, the budget's cuts left in `cut`.
    fn asked(&mut self, c: &Expr, env: &Env, m: &str, want: bool) -> R<Vec<Premise>> {
        if !want {
            match self.ask(c, env)? {
                None | Some(true) => return Ok(Vec::new()),
                Some(false) => {}
            }
        }
        let mut out = Vec::new();
        for k in conjuncts(c) {
            let holds = match want {
                true => true,
                false => match self.ask(k, env)? {
                    Some(h) => h,
                    None => return Ok(out),
                },
            };
            if want || !holds {
                out.push(self.premise(k, env, m, holds)?);
            }
        }
        if out.is_empty() {
            for k in conjuncts(c) {
                let Some(envs) = self.bindings(k, env, self.tries)? else { return Ok(out) };
                if envs.first().is_some_and(|e| e.len() > env.len()) {
                    for e in envs {
                        out.push(self.premise(c, &e, m, false)?);
                    }
                    break;
                }
            }
        }
        if out.is_empty() {
            out.push(self.premise(c, env, m, false)?);
        }
        Ok(out)
    }

    /// Premise C under ENV, whose truth is HOLDS: the parts that decide it and why the instances it reads hold or not,
    /// cut where the budget ran out; an instance it names on the path stands on it, cut or not.
    fn premise(&mut self, c: &Expr, env: &Env, m: &str, holds: bool) -> R<Premise> {
        let mut p = Premise { at: span(m, c.at), text: form::expr(c), env: shown(c, env), holds, cut: false, parts: Vec::new(), under: Vec::new() };
        if !self.spend() {
            if self.is_atom(c) && self.rules.ground(c, env).is_some_and(|i| self.path.contains(&i)) {
                self.looped = true;
            }
            p.cut = true;
            return Ok(p);
        }
        match &c.e {
            E::Bin(Op::And, ..) => {
                for k in conjuncts(c) {
                    let h = match holds {
                        true => true,
                        false => match self.ask(k, env)? {
                            Some(h) => h,
                            None => {
                                p.cut = true;
                                break;
                            }
                        },
                    };
                    if holds || !h {
                        p.parts.push(self.premise(k, env, m, h)?);
                    }
                }
            }
            E::Bin(Op::Or, ..) if holds => {
                let mut met = Vec::new();
                for k in ors(c) {
                    match self.ask(k, env)? {
                        Some(true) => met.push((k, env.clone())),
                        Some(false) => {}
                        None => {
                            p.cut = true;
                            break;
                        }
                    }
                }
                p.parts.extend(self.apart(met, m)?);
            }
            E::Bin(Op::Or, ..) => {
                for k in ors(c) {
                    p.parts.push(self.premise(k, env, m, false)?);
                }
            }
            E::Not(x) => p.parts.push(self.premise(x, env, m, !holds)?),
            E::Holds(_) | E::Apply(..) if self.is_atom(c) => match self.atom(c, env, holds)? {
                Some(under) => p.under = under,
                None => p.cut = true,
            },
            E::Quant(Quant::Exists, _, b) if holds => match self.bindings(b, env, self.tries)? {
                Some(ws) => {
                    let witnesses = ws.into_iter().map(|e| (&**b, e)).collect();
                    p.parts.extend(self.apart(witnesses, m)?);
                }
                None => p.cut = true,
            },
            E::Quant(Quant::Forall, _, b) if !holds => {
                let not = Expr { at: b.at, e: E::Not(b.clone()) };
                match self.bindings(&not, env, 1)? {
                    Some(es) => {
                        for e in es {
                            p.parts.push(self.premise(b, &e, m, false)?);
                        }
                    }
                    None => p.cut = true,
                }
            }
            _ => {}
        }
        Ok(p)
    }

    /// The first of the premises TRIED (each holds; at most `tries`) whose explanation is whole and does not stand on
    /// the path; else the first, standing on it or cut as it is.
    fn apart(&mut self, tried: Vec<(&Expr, Env)>, m: &str) -> R<Option<Premise>> {
        let (mut first, mut stood, mut cut) = (None, false, false);
        for (c, env) in tried.into_iter().take(self.tries) {
            let was = (std::mem::replace(&mut self.looped, false), std::mem::replace(&mut self.cut, false));
            let p = self.premise(c, &env, m, true)?;
            let (l, k) = (std::mem::replace(&mut self.looped, was.0), std::mem::replace(&mut self.cut, was.1));
            if !l && !k {
                return Ok(Some(p));
            }
            (stood, cut) = (stood || l, cut || k);
            first.get_or_insert(p);
        }
        (self.looped, self.cut) = (self.looped || stood, self.cut || cut);
        Ok(first)
    }

    fn is_atom(&self, c: &Expr) -> bool {
        let x = if let E::Holds(x) = &c.e { x } else { c };
        matches!(&x.e, E::Apply(p, _) if self.rules.types.contains_key(&p.to_string()))
    }

    /// Why the instance atom C names under ENV holds or not (HOLDS), on the path or explained; with unbound variables,
    /// the first instance that holds. `None` when the budget is spent before that instance is found.
    fn atom(&mut self, c: &Expr, env: &Env, holds: bool) -> R<Option<Vec<Explain>>> {
        let insts: Vec<Instance> = match self.rules.ground(c, env) {
            Some(i) => vec![i],
            None if holds => match self.bindings(c, env, 1)? {
                Some(es) => es.iter().filter_map(|e| self.rules.ground(c, e)).collect(),
                None => return Ok(None),
            },
            None => Vec::new(),
        };
        let mut out = Vec::new();
        for i in insts {
            if self.path.contains(&i) {
                self.looped = true;
                out.push(Explain::OnPath { inst: i, holds });
            } else if let Ok(x) = self.explain(&i) {
                out.push(x);
            }
        }
        Ok(Some(out))
    }
}
