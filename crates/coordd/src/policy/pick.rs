//! The picker (paper-research N-568, part 2; consult-lead's one-interpreter.md §4.2): a decision instance decided
//! without clingo. Each instance is solved alone today (solve.rs: one `live(dN)` at a time, `chosen` reserved, so no
//! policy text reads another instance's choice), so a decision is a pick over its allowed options:
//!
//! - the options (the `Options` fact's instances, or those listed in place), and for each its exclusions, every one
//!   named: a `Requires` line it fails, an `Only` whose `When` holds and whose condition it fails, a caller-given field
//!   it lacks, the call's `where` (models.rs), a failed call on it;
//! - for each allowed option its cost per preference (`Prefer`), by level, highest level first;
//! - the optimal ones (the least costs, compared from the highest level down), the first by name taken, as solve.rs
//!   takes it (no judge is run on a tie there either); the runner-up the best of the rest; the preference that decided
//!   the highest level where the two differ.
//!
//! It reads the same declarations compile.rs compiles and numbers its requirements and rules the same way, so its
//! [`Outcome`] can be compared field by field with the solver's. The harness in `pick_tests` compared the two on the recorded
//! `decided` lines (N-568 p13); coordd answers with it as the coord's [`super::Decider`] says (N-568 p14): `both`
//! sets its answer beside the solver's ([`beside`]), `picker` answers with it alone.
//!
//! The semantics mirror compile.rs's program, not eFLINT's prose: a literal reading a call field the call does not
//! have fails (so a comparison with it is false, `!=` too); `_` in a negated fact means no instance at all; a fact read
//! as a value (`price(option)`) with several values gives each (a `minimise` sums the distinct ones, as clingo's weak
//! constraint tuples do); numbers come before strings in a comparison, as clingo orders its terms.

use super::compile::Scope;
use super::solve::{Cost, Exclusion, Instance, Outcome};
use super::syntax::{At, Cond, Decl, DeclKind, Derivation, Expr, Lit, Op, Rank};
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A term: clingo's order puts numbers before strings.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum V {
    Int(i64),
    Str(String),
}

impl V {
    fn of_json(v: &Value) -> V {
        match v {
            Value::Number(n) => n.as_i64().map_or_else(|| V::Str(n.to_string()), V::Int),
            Value::String(s) => V::Str(s.clone()),
            other => V::Str(other.to_string()),
        }
    }

    /// A call field's value: a whole number is a number (solve.rs `open`).
    fn of_field(s: &str) -> V {
        s.parse::<i64>().map_or_else(|_| V::Str(s.to_string()), V::Int)
    }

    fn text(&self) -> String {
        match self {
            V::Int(n) => n.to_string(),
            V::Str(s) => s.clone(),
        }
    }
}

type Tuples = BTreeSet<Vec<V>>;

/// What one evaluation reads: the facts, the option judged, the call's fields, the free names bound so far (a
/// `Derived from` line's).
struct Env<'a> {
    db: &'a HashMap<String, Tuples>,
    option: Option<&'a V>,
    call: &'a BTreeMap<String, V>,
}

type Bind = BTreeMap<String, V>;

impl Env<'_> {
    fn tuples(&self, f: &str) -> impl Iterator<Item = &Vec<V>> {
        self.db.get(f).into_iter().flatten()
    }

    /// Each value E may take under B, with the bindings that give it (a fact read as a value may give several; a
    /// call field the call lacks gives none).
    fn values(&self, e: &Expr, b: &Bind) -> Vec<(V, Bind)> {
        match e {
            Expr::Option => self.option.map(|o| vec![(o.clone(), b.clone())]).unwrap_or_default(),
            Expr::Str(s) => vec![(V::Str(s.clone()), b.clone())],
            Expr::Int(n) => vec![(V::Int(*n), b.clone())],
            Expr::Call(f) => self.call.get(f).map(|v| vec![(v.clone(), b.clone())]).unwrap_or_default(),
            Expr::Field(x) => b.get(x).map(|v| vec![(v.clone(), b.clone())]).unwrap_or_default(),
            Expr::Any => Vec::new(),
            Expr::App(f, args) => {
                let mut out = Vec::new();
                for (b2, t) in self.matches(f, args, b, args.len() + 1) {
                    out.push((t[args.len()].clone(), b2));
                }
                out
            }
        }
    }

    /// The tuples of F (of ARITY) that ARGS (a prefix of its arguments) match under B, with the bindings each gives.
    fn matches(&self, f: &str, args: &[Expr], b: &Bind, arity: usize) -> Vec<(Bind, Vec<V>)> {
        // the arguments' candidate values first (a fact as a value inside an argument binds there)
        let mut out = Vec::new();
        for t in self.tuples(f).filter(|t| t.len() == arity) {
            let mut binds = vec![b.clone()];
            for (a, v) in args.iter().zip(t) {
                let mut next = Vec::new();
                for bb in binds {
                    match a {
                        Expr::Any => next.push(bb),
                        Expr::Field(x) if !bb.contains_key(x) => {
                            let mut nb = bb.clone();
                            nb.insert(x.clone(), v.clone());
                            next.push(nb);
                        }
                        _ => next.extend(self.values(a, &bb).into_iter().filter(|(w, _)| w == v).map(|(_, nb)| nb)),
                    }
                }
                binds = next;
                if binds.is_empty() {
                    break;
                }
            }
            out.extend(binds.into_iter().map(|bb| (bb, t.clone())));
        }
        out
    }

    /// Every binding under which COND holds, from B: positive literals first, in order, negations last (compile.rs
    /// `Vars::cond`); a negation's facts read as values bind as positives do.
    fn holds(&self, cond: &Cond, b: &Bind) -> Vec<Bind> {
        let mut binds = vec![b.clone()];
        for l in cond.iter().filter(|l| !matches!(l, Lit::Not(..))) {
            let mut next = Vec::new();
            for bb in &binds {
                match l {
                    Lit::Holds(f, args) => next.extend(self.matches(f, args, bb, args.len()).into_iter().map(|(nb, _)| nb)),
                    Lit::Cmp(x, op, y) => {
                        for (vx, bx) in self.values(x, bb) {
                            for (vy, by) in self.values(y, &bx) {
                                if cmp(&vx, *op, &vy) {
                                    next.push(by);
                                }
                            }
                        }
                    }
                    Lit::Not(..) => unreachable!(),
                }
            }
            binds = next;
        }
        for l in cond.iter().filter(|l| matches!(l, Lit::Not(..))) {
            let Lit::Not(f, args) = l else { unreachable!() };
            let mut next = Vec::new();
            for bb in &binds {
                // the facts read as values inside it bind positively (compile.rs puts their literals in the body)
                for vb in self.value_args(args, bb) {
                    if self.matches(f, &vb.0, &vb.1, args.len()).is_empty() {
                        next.push(vb.1);
                    }
                }
            }
            binds = next;
        }
        binds
    }

    /// ARGS with each fact-as-value replaced by each value it takes (and its bindings): what a negated literal is
    /// tested with.
    fn value_args(&self, args: &[Expr], b: &Bind) -> Vec<(Vec<Expr>, Bind)> {
        let mut out = vec![(Vec::new(), b.clone())];
        for a in args {
            let mut next = Vec::new();
            for (done, bb) in out {
                match a {
                    Expr::App(..) | Expr::Call(_) => {
                        for (v, nb) in self.values(a, &bb) {
                            let mut d = done.clone();
                            d.push(match v {
                                V::Int(n) => Expr::Int(n),
                                V::Str(s) => Expr::Str(s),
                            });
                            next.push((d, nb));
                        }
                    }
                    _ => {
                        let mut d = done.clone();
                        d.push(a.clone());
                        next.push((d, bb));
                    }
                }
            }
            out = next;
        }
        out
    }

    fn any(&self, cond: &Cond) -> bool {
        !self.holds(cond, &Bind::new()).is_empty()
    }
}

fn cmp(a: &V, op: Op, b: &V) -> bool {
    let o = a.cmp(b);
    match op {
        Op::Eq => o == Ordering::Equal,
        Op::Ne => o != Ordering::Equal,
        Op::Lt => o == Ordering::Less,
        Op::Le => o != Ordering::Greater,
        Op::Gt => o == Ordering::Greater,
        Op::Ge => o != Ordering::Less,
    }
}

/// A `Requires` line, numbered as compile.rs numbers it.
struct Req {
    decision: String,
    cond: Cond,
    rule: String,
    why: Option<String>,
}

/// A `Prefer` or `Only`, numbered as compile.rs numbers them (one count for both).
struct Rule {
    decision: String,
    scope: Scope,
    when: Cond,
    level: Option<i64>,
    what: RuleKind,
    text: String,
    at: At,
    why: Option<String>,
}

enum RuleKind {
    Prefer(Rank),
    Only(Cond),
}

/// A decision type's options.
struct Kind {
    options: String,
    listed: Vec<String>,
}

/// The policy read for picking: its facts (supplied, stated, derived), decision types, requirements and rules.
pub struct Picker {
    db: HashMap<String, Tuples>,
    facts: BTreeMap<String, Vec<String>>,
    kinds: BTreeMap<String, Kind>,
    reqs: Vec<Req>,
    rules: Vec<Rule>,
}

fn why_of(d: &Decl) -> Option<String> {
    [&d.doc, &d.comment].into_iter().find(|t| !t.trim().is_empty()).map(|t| t.replace('\n', " "))
}

impl Picker {
    /// SCOPES' declarations (as `compile::compile` takes them) with the supplied FACTS (as `Solver::set_facts` takes
    /// them). The policy must compile; what compile.rs refuses is not checked again here.
    pub fn new(scopes: &[(Scope, Vec<Decl>)], facts: &[(String, Vec<Value>)]) -> Result<Picker, String> {
        let mut p = Picker { db: HashMap::new(), facts: BTreeMap::new(), kinds: BTreeMap::new(), reqs: Vec::new(), rules: Vec::new() };
        for (_, decls) in scopes {
            for d in decls {
                match &d.kind {
                    DeclKind::Fact { name, ids, .. } => {
                        p.facts.insert(name.clone(), ids.clone());
                    }
                    DeclKind::Decision { name, options, .. } => {
                        let listed = if options.contains('|') { options.split('|').map(|o| o.trim().to_string()).collect() } else { Vec::new() };
                        p.kinds.insert(name.clone(), Kind { options: options.clone(), listed });
                    }
                    _ => {}
                }
            }
        }
        for (f, args) in facts {
            p.db.entry(f.clone()).or_default().insert(args.iter().map(V::of_json).collect());
        }
        let mut derived: Vec<(String, Derivation)> = Vec::new();
        let mut position: BTreeMap<i64, i64> = BTreeMap::new();
        for (scope, decls) in scopes {
            for d in decls {
                match &d.kind {
                    DeclKind::Instance { fact, args } => {
                        let t = args.iter().map(|a| match a {
                            Expr::Str(s) => V::Str(s.clone()),
                            Expr::Int(n) => V::Int(*n),
                            other => V::Str(format!("{other:?}")),
                        });
                        p.db.entry(fact.clone()).or_default().insert(t.collect());
                    }
                    DeclKind::Fact { name, derived: ds, .. } | DeclKind::Extend { name, derived: ds } => derived.extend(ds.iter().map(|x| (name.clone(), x.clone()))),
                    DeclKind::Decision { name, requires, .. } => {
                        for (at, cond, text) in requires {
                            p.reqs.push(Req { decision: name.clone(), cond: cond.clone(), rule: format!("{text} ({at})"), why: why_of(d) });
                        }
                    }
                    DeclKind::Prefer { decision, when, rank } => {
                        let level = rank_of(scope) * 1000 + (1000 - *position.entry(rank_of(scope)).and_modify(|x| *x += 1).or_insert(0));
                        p.rules.push(Rule { decision: decision.clone(), scope: scope.clone(), when: when.clone(), level: Some(level), what: RuleKind::Prefer(rank.clone()), text: d.text.clone(), at: d.at.clone(), why: why_of(d) });
                    }
                    DeclKind::Only { decision, when, cond } => {
                        p.rules.push(Rule { decision: decision.clone(), scope: scope.clone(), when: when.clone(), level: None, what: RuleKind::Only(cond.clone()), text: d.text.clone(), at: d.at.clone(), why: why_of(d) });
                    }
                    _ => {}
                }
            }
        }
        p.derive(&derived)?;
        Ok(p)
    }

    /// The `Derived from` lines to their fixpoint, stratum by stratum: a fact read under `not` is whole before it is
    /// read so (compile.rs refuses only a fact derived from its own absence; a longer cycle through `not` is refused
    /// here, as it has no single answer).
    fn derive(&mut self, derived: &[(String, Derivation)]) -> Result<(), String> {
        let heads: BTreeSet<&str> = derived.iter().map(|(n, _)| n.as_str()).collect();
        // stratum of each derived fact: at least that of what it reads, one more than what it reads under `not`
        let mut stratum: BTreeMap<&str, usize> = heads.iter().map(|h| (*h, 0)).collect();
        for _ in 0..=heads.len() {
            let mut changed = false;
            for (n, d) in derived {
                for l in &d.when {
                    let (f, step) = match l {
                        Lit::Holds(f, _) => (f.as_str(), 0),
                        Lit::Not(f, _) => (f.as_str(), 1),
                        Lit::Cmp(..) => continue,
                    };
                    if let Some(s) = stratum.get(f).copied() {
                        let want = s + step;
                        if stratum[n.as_str()] < want {
                            stratum.insert(n.as_str(), want);
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
            if stratum.values().any(|s| *s > heads.len()) {
                return Err("the derived facts read each other's absence in a cycle: no single answer".into());
            }
        }
        let top = stratum.values().copied().max().unwrap_or(0);
        let none = BTreeMap::new();
        for s in 0..=top {
            loop {
                let mut new: Vec<(String, Vec<V>)> = Vec::new();
                {
                    let env = Env { db: &self.db, option: None, call: &none };
                    for (n, d) in derived.iter().filter(|(n, _)| stratum[n.as_str()] == s) {
                        for b in env.holds(&d.when, &Bind::new()) {
                            let mut heads: Vec<Vec<V>> = vec![Vec::new()];
                            for a in &d.args {
                                let mut next = Vec::new();
                                for h in heads {
                                    for (v, _) in env.values(a, &b) {
                                        let mut h2 = h.clone();
                                        h2.push(v);
                                        next.push(h2);
                                    }
                                }
                                heads = next;
                            }
                            for h in heads {
                                if !self.db.get(n).is_some_and(|t| t.contains(&h)) {
                                    new.push((n.clone(), h));
                                }
                            }
                        }
                    }
                }
                if new.is_empty() {
                    break;
                }
                for (n, h) in new {
                    self.db.entry(n).or_default().insert(h);
                }
            }
        }
        Ok(())
    }

    /// Every instance of fact FACT that holds, its arguments as text (solve.rs `holding`).
    pub fn holding(&self, fact: &str) -> Vec<Vec<String>> {
        self.db.get(fact).into_iter().flatten().map(|t| t.iter().map(V::text).collect()).collect()
    }

    /// INST decided with the options FAILED excluded (their calls failed, in the order they did): the outcome as
    /// solve.rs gives it, and the options tied with the runner-up (more than one: clingo's runner-up is any of them).
    pub fn pick(&self, inst: &Instance, failed: &[String]) -> Result<(Outcome, Vec<String>), String> {
        let kind = self.kinds.get(&inst.kind).ok_or_else(|| format!("no decision type `{}`", inst.kind))?;
        let call: BTreeMap<String, V> = inst.call.iter().map(|(k, v)| (k.clone(), V::of_field(v))).collect();
        let options: BTreeSet<V> = if kind.listed.is_empty() {
            self.db.get(&kind.options).into_iter().flatten().filter(|t| t.len() == 1).map(|t| t[0].clone()).collect()
        } else {
            kind.listed.iter().map(|o| V::Str(o.clone())).collect()
        };
        // the guard of a rule of SCOPE (compile.rs `guard`): its decision, an agent's file its agent, its `When`
        let guarded = |r: &Rule, o: &V| -> Vec<Bind> {
            if r.decision != inst.kind {
                return Vec::new();
            }
            if let Scope::Agent(a) = &r.scope {
                if call.get("agent") != Some(&V::Str(a.clone())) {
                    return Vec::new();
                }
            }
            Env { db: &self.db, option: Some(o), call: &call }.holds(&r.when, &Bind::new())
        };
        // the fields an option has, for the caller's given ones (compile.rs `field/4`)
        let has_field = |o: &V, f: &str, v: &str| -> bool {
            let want = V::Str(v.to_string());
            if !kind.listed.is_empty() {
                return f == "option" && *o == want;
            }
            if f == kind.options && *o == want {
                return true;
            }
            self.facts.get(f).is_some_and(|ids| ids.len() == 2 && ids[0] == kind.options) && self.db.get(f).is_some_and(|t| t.contains(&vec![o.clone(), want.clone()]))
        };
        let mut excluded: Vec<Exclusion> = Vec::new();
        let mut out_of: BTreeSet<V> = BTreeSet::new();
        let ex = |excluded: &mut Vec<Exclusion>, out_of: &mut BTreeSet<V>, o: &V, reason: &str, rule: String, why: Option<String>| {
            let e = Exclusion { option: o.text(), reason: reason.into(), rule, why };
            if !excluded.contains(&e) {
                excluded.push(e);
            }
            out_of.insert(o.clone());
        };
        for o in &options {
            let env = Env { db: &self.db, option: Some(o), call: &call };
            if failed.contains(&o.text()) {
                ex(&mut excluded, &mut out_of, o, "failed", "the call failed on this model".into(), None);
            }
            let mut fields: Vec<&str> = Vec::new();
            for g in &inst.given {
                if !fields.contains(&g.field.as_str()) {
                    fields.push(&g.field);
                }
            }
            for f in fields {
                let g = inst.given.iter().find(|g| g.field == f).unwrap();
                if !inst.given.iter().filter(|g| g.field == f).all(|g| has_field(o, f, &g.value)) {
                    ex(&mut excluded, &mut out_of, o, "given", format!("{} = {} (given by {} at {})", g.field, g.value, g.by, g.at), None);
                }
            }
            for q in self.reqs.iter().filter(|q| q.decision == inst.kind) {
                if !env.any(&q.cond) {
                    ex(&mut excluded, &mut out_of, o, "requires", q.rule.clone(), q.why.clone());
                }
            }
            for r in &self.rules {
                if let RuleKind::Only(cond) = &r.what {
                    if !guarded(r, o).is_empty() && !env.any(cond) {
                        ex(&mut excluded, &mut out_of, o, "only", format!("{} ({})", r.text.replace('\n', " "), r.at), r.why.clone());
                    }
                }
            }
        }
        // the call's `where` (models.rs): named whether or not it is an option
        for m in &inst.unmet {
            let o = V::Str(m.clone());
            ex(&mut excluded, &mut out_of, &o, "where", format!("where {}", inst.call.get("where").map_or("", String::as_str)), None);
        }
        excluded.sort_by(|a, b| (&a.option, &a.reason, &a.rule).cmp(&(&b.option, &b.reason, &b.rule)));
        let allowed: Vec<&V> = options.iter().filter(|o| !out_of.contains(*o)).collect();
        if allowed.is_empty() {
            let fault = if inst.given.is_empty() { "no option satisfies the hard rules" } else { "no_route: no option has the fields the caller gave" };
            return Ok((Outcome { excluded, fault: Some(fault.into()), ..Default::default() }, Vec::new()));
        }
        // each allowed option's cost per preference (compile.rs `pen/4`: distinct weights summed)
        let prefs: Vec<&Rule> = self.rules.iter().filter(|r| r.decision == inst.kind && r.level.is_some()).collect();
        let costs_of = |o: &V| -> Vec<Cost> {
            let env = Env { db: &self.db, option: Some(o), call: &call };
            let mut out: Vec<Cost> = prefs
                .iter()
                .map(|r| {
                    let mut weights: BTreeSet<i64> = BTreeSet::new();
                    let binds = guarded(r, o);
                    match &r.what {
                        RuleKind::Prefer(Rank::Order(conds)) if !binds.is_empty() => {
                            let k = conds.iter().position(|c| env.any(c)).unwrap_or(conds.len());
                            if k > 0 {
                                weights.insert(k as i64);
                            }
                        }
                        RuleKind::Prefer(Rank::Minimise(e) | Rank::Maximise(e)) => {
                            let max = matches!(&r.what, RuleKind::Prefer(Rank::Maximise(_)));
                            for b in &binds {
                                for (v, _) in env.values(e, b) {
                                    if let V::Int(n) = v {
                                        weights.insert(if max { -n } else { n });
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    Cost { rule: format!("{} ({})", r.text.replace('\n', " "), r.at), level: r.level.unwrap(), cost: weights.iter().sum(), why: r.why.clone() }
                })
                .collect();
            out.sort_by(|a, b| b.level.cmp(&a.level));
            out
        };
        let key = |cs: &[Cost]| -> Vec<(i64, i64)> {
            let mut by: BTreeMap<i64, i64> = BTreeMap::new();
            for c in cs {
                *by.entry(c.level).or_default() += c.cost;
            }
            // the highest level first, the least cost best
            by.into_iter().rev().map(|(l, c)| (-l, c)).collect()
        };
        let ranked: Vec<(&V, Vec<Cost>)> = allowed.iter().map(|o| (*o, costs_of(o))).collect();
        let best = |set: &[&(&V, Vec<Cost>)]| -> Vec<String> {
            let Some(min) = set.iter().map(|(_, c)| key(c)).min() else { return Vec::new() };
            let mut v: Vec<String> = set.iter().filter(|(_, c)| key(c) == min).map(|(o, _)| o.text()).collect();
            v.sort();
            v
        };
        let all: Vec<&(&V, Vec<Cost>)> = ranked.iter().collect();
        let optimal = best(&all);
        let winner = optimal[0].clone();
        let costs = ranked.iter().find(|(o, _)| o.text() == winner).map(|(_, c)| c.clone()).unwrap_or_default();
        let rest: Vec<&(&V, Vec<Cost>)> = ranked.iter().filter(|(o, _)| o.text() != winner).collect();
        let ties = best(&rest);
        let runner_up = ties.first().map(|r| (r.clone(), ranked.iter().find(|(o, _)| o.text() == *r).map(|(_, c)| c.clone()).unwrap_or_default()));
        let decided_by = runner_up.as_ref().and_then(|(_, rc)| {
            let mut levels: Vec<i64> = costs.iter().chain(rc).map(|c| c.level).collect();
            levels.sort_unstable_by(|a, b| b.cmp(a));
            levels.dedup();
            let at = |cs: &[Cost], l: i64| cs.iter().filter(|c| c.level == l).map(|c| c.cost).sum::<i64>();
            let l = levels.into_iter().find(|&l| at(&costs, l) != at(rc, l))?;
            let r = self.rules.iter().find(|r| r.level == Some(l))?;
            Some(Cost { rule: format!("{} ({})", r.text.replace('\n', " "), r.at), level: l, cost: at(rc, l) - at(&costs, l), why: r.why.clone() })
        });
        Ok((Outcome { chosen: Some(winner), optimal, excluded, costs, runner_up, decided_by, fault: None, grounds: Vec::new() }, ties))
    }
}

fn sorted(mut e: Vec<Exclusion>) -> Vec<Exclusion> {
    e.sort_by(|a, b| (&a.option, &a.reason, &a.rule).cmp(&(&b.option, &b.reason, &b.rule)));
    e
}

/// Where the picker's outcome P (with TIES, the options tied for runner-up) differs from the solver's S: one line
/// each. The runner-up may be any of the tied ones (clingo gives the first model it finds); the exclusions are
/// compared as sets (their order among one option's reasons of one kind is clingo's model order).
pub fn differences(s: &Outcome, p: &Outcome, ties: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    if s.chosen != p.chosen {
        out.push(format!("chosen: clingo {:?}, picker {:?}", s.chosen, p.chosen));
    }
    if s.optimal != p.optimal {
        out.push(format!("optimal: clingo {:?}, picker {:?}", s.optimal, p.optimal));
    }
    let (se, pe) = (sorted(s.excluded.clone()), sorted(p.excluded.clone()));
    if se != pe {
        let only_s: Vec<_> = se.iter().filter(|e| !pe.contains(e)).map(|e| format!("{} {} {}", e.option, e.reason, e.rule)).collect();
        let only_p: Vec<_> = pe.iter().filter(|e| !se.contains(e)).map(|e| format!("{} {} {}", e.option, e.reason, e.rule)).collect();
        out.push(format!("excluded: clingo only {only_s:?}, picker only {only_p:?}"));
    }
    if s.costs != p.costs {
        out.push(format!("costs: clingo {:?}, picker {:?}", s.costs, p.costs));
    }
    match (&s.runner_up, &p.runner_up) {
        (None, None) => {}
        (Some((sn, sc)), Some((_, pc))) if ties.contains(sn) && sc == pc => {}
        (a, b) => out.push(format!("runner-up: clingo {a:?}, picker {b:?} (tied: {ties:?})")),
    }
    if s.decided_by != p.decided_by {
        out.push(format!("decided by: clingo {:?}, picker {:?}", s.decided_by, p.decided_by));
    }
    if s.fault != p.fault {
        out.push(format!("fault: clingo {:?}, picker {:?}", s.fault, p.fault));
    }
    out
}

/// A picker call F with a panic in it as its fault: in `both` a picker fault leaves the solver's answer standing, and
/// in `picker` it is the decision's fault, never the policy thread's end.
pub fn guarded<R>(f: impl FnOnce() -> Result<R, String>) -> Result<R, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|p| {
        let why = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_default();
        Err(format!("the picker panicked: {why}"))
    })
}

/// In `both` (paper-research N-568 p14): the picker's answer to INST, with the options FAILED excluded in the order
/// their calls failed, beside the solver's answer SOLVED to the same question. None when they are the same in every
/// field `differences` compares; otherwise the one journal line naming the decision, the instance and each difference
/// (a picker fault, a policy it cannot read, a panic in it among them). The solver's answer stands either way.
pub fn beside(picker: Result<&Picker, &str>, inst: &Instance, failed: &[String], solved: Result<&Outcome, &str>) -> Option<String> {
    let picked = picker.map_err(str::to_string).and_then(|p| guarded(|| p.pick(inst, failed)));
    let diffs = match (solved, &picked) {
        (Ok(s), Ok((p, ties))) => differences(s, p, ties),
        (Ok(_), Err(e)) => vec![format!("the picker faulted: {e}")],
        (Err(e), Ok((p, _))) => vec![format!("the solver faulted ({e}); the picker chose {:?}", p.chosen)],
        (Err(_), Err(_)) => Vec::new(),
    };
    if diffs.is_empty() {
        return None;
    }
    let instance = serde_json::to_string(inst).unwrap_or_default();
    Some(format!("coordd: decider both: {} {instance} failed {failed:?}: {}", inst.kind, diffs.join("; ")))
}

/// In `both`: what the picker holds of FACT beside what the solver holds (`roles_held`), compared as sets; the
/// journal line when they differ, as [`beside`].
pub fn holding_beside(picker: Result<&Picker, &str>, fact: &str, solved: &[Vec<String>]) -> Option<String> {
    let picked = picker.map_err(str::to_string).and_then(|p| guarded(|| Ok(p.holding(fact))));
    let line = match picked {
        Ok(p) => {
            let (s, p): (BTreeSet<&Vec<String>>, BTreeSet<&Vec<String>>) = (solved.iter().collect(), p.iter().collect());
            if s == p {
                return None;
            }
            format!("solver only {:?}, picker only {:?}", s.difference(&p).collect::<Vec<_>>(), p.difference(&s).collect::<Vec<_>>())
        }
        Err(e) => format!("the picker faulted: {e}"),
    };
    Some(format!("coordd: decider both: holding {fact}: {line}"))
}

fn rank_of(s: &Scope) -> i64 {
    match s {
        Scope::Global => 0,
        Scope::Project => 1,
        Scope::Agent(_) => 2,
    }
}

#[cfg(test)]
#[path = "pick_tests.rs"]
mod pick_tests;
