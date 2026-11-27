//! Which calls an agent named in a step's answer is given later (paper-research N-101 p977, p980): for each call
//! and each place of its answer that holds an agent (`Assigned.agent`, `Picked.picks[].agent`), the calls of the
//! steps that agent is the worker of, in the order the programs reach them. Read off the checked kinds: a value is
//! followed from the answer through `<-`, a `match` arm's names, `x.f`, `find` and `for` over a list, a `return` out
//! of an `iterate`, `try` or `par`, and `open K(...)` into K's form and out of its result. An agent read from a
//! ticket at run time (`step by …` with `by` a culprit's answer) is not followed: nothing is said for it.

use crate::check::CallSig;
use crate::syntax::{Bound, Expr, Kind, Pat, Prog, Worker};
use crate::types::Type;
use std::collections::{BTreeMap, BTreeSet};

/// By the kind a step is in and its call, by place of its answer (`place`), the calls given to the agent there, in
/// order. By kind, since one call may lead to other calls in another program (`assign` gives the agent it names
/// `design` in `change`, `mend_given` in Tovrin's `handed_up`); two steps of one call in one kind are one entry.
pub type Given = BTreeMap<(String, String), BTreeMap<String, Vec<String>>>;

/// By the kind a step is in and its call, the lists of its answer whose entries name elements of a list of the same
/// answer (N-101 p1141, run 4's `stands_on`): each as (the list's place, the place of the names), read off a `par`
/// over the answer's list, `as p.name after p.stands_on` (`Design.parts[].stands_on`, `Design.parts[].name`).
pub type Names = BTreeMap<(String, String), BTreeSet<(String, String)>>;

/// A place in an answer as written: its alternative, then `.field` for a field and `[]` for a list's element
/// (`Picked.picks[].agent`). The one form, so coordd's reading of an answer names the places as this does.
pub fn place(segs: &[String]) -> String {
    let mut out = String::new();
    for (i, s) in segs.iter().enumerate() {
        match s.as_str() {
            "[]" => out.push_str("[]"),
            s if i == 0 => out.push_str(s),
            s => {
                out.push('.');
                out.push_str(s);
            }
        }
    }
    out
}

/// Where a value comes from: the answer of a step (in a kind, of a call), at a place in it.
type Origin = ((String, String), Vec<String>);

/// What a value holds, abstractly: at each path inside it, the answers' places it is.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Av(BTreeMap<Vec<String>, BTreeSet<Origin>>);

impl Av {
    fn from(o: Origin) -> Av {
        Av(BTreeMap::from([(Vec::new(), BTreeSet::from([o]))]))
    }

    fn union(mut self, b: Av) -> Av {
        for (p, os) in b.0 {
            self.0.entry(p).or_default().extend(os);
        }
        self
    }

    /// The value with each of its paths under PRE.
    fn under(self, pre: &[String]) -> Av {
        Av(self.0.into_iter().map(|(p, os)| (pre.iter().cloned().chain(p).collect(), os)).collect())
    }

    /// The part of the value at SEG: the answers' places one further, and what was put there.
    fn at(&self, seg: &str) -> Av {
        let mut out = Av::default();
        for (p, os) in &self.0 {
            match p.split_first() {
                None => {
                    let os = os.iter().map(|(s, q)| (s.clone(), q.iter().cloned().chain([seg.to_string()]).collect())).collect();
                    out.0.entry(Vec::new()).or_default().extend::<BTreeSet<Origin>>(os);
                }
                Some((s, rest)) if s == seg => out.0.entry(rest.to_vec()).or_default().extend(os.iter().cloned()),
                _ => {}
            }
        }
        out
    }

    /// Field F, written `x.f`: of a record, or of an alternative that has it (its name before it).
    fn field(&self, f: &str) -> Av {
        let mut out = self.at(f);
        for (p, os) in &self.0 {
            if let [c, g, rest @ ..] = p.as_slice() {
                if g == f && c.starts_with(char::is_uppercase) {
                    out.0.entry(rest.to_vec()).or_default().extend(os.iter().cloned());
                }
            }
        }
        out
    }

    /// Field I of constructor C (named F when known).
    fn con_field(&self, c: &str, i: usize, f: Option<&str>) -> Av {
        let pos = format!("#{i}");
        let name = f.map_or(pos.clone(), str::to_string);
        let mut out = self.at(c).at(&name);
        if f.is_some() {
            out = out.union(self.at(c).at(&pos));
        }
        out
    }
}

struct Walk<'a> {
    kinds: &'a BTreeMap<String, Kind>,
    calls: &'a BTreeMap<String, CallSig>,
    out: Given,
    names: Names,
    /// The kinds being followed, with what their form holds: one opened again inside itself gives nothing more.
    busy: BTreeSet<(String, Vec<Av>)>,
    done: BTreeMap<(String, Vec<Av>), Av>,
    /// The kind being followed now: where a step is.
    here: Vec<String>,
}

type Env = BTreeMap<String, Av>;

impl Walk<'_> {
    /// The place O names, its alternative put before it when the program read a field without one (`x.f` on the
    /// answer, where only one alternative has F).
    fn named(&self, o: &Origin) -> String {
        let ((_, call), segs) = o;
        if let (Some(Type::Variant(cs)), Some(first)) = (self.calls.get(call).map(|s| &s.head), segs.first()) {
            if !cs.iter().any(|(c, _)| c == first) {
                let with: Vec<&String> = cs.iter().filter(|(_, fs)| fs.iter().any(|(f, _)| f == first)).map(|(c, _)| c).collect();
                if let [c] = with.as_slice() {
                    return place(&std::iter::once((*c).clone()).chain(segs.iter().cloned()).collect::<Vec<_>>());
                }
            }
        }
        place(segs)
    }

    fn given(&mut self, worker: &Av, call: &str) {
        let Some(os) = worker.0.get(&Vec::new()) else { return };
        for o in os.clone() {
            let at = self.named(&o);
            let v = self.out.entry(o.0).or_default().entry(at).or_default();
            if !v.iter().any(|c| c == call) {
                v.push(call.to_string());
            }
        }
    }

    /// AFTER's entries name KEY's values, where both are of one answer: written down under that answer's step.
    fn named_by(&mut self, after: &Av, key: &Av) {
        let (Some(a), Some(k)) = (after.0.get(&Vec::new()), key.0.get(&Vec::new())) else { return };
        let mut found = Vec::new();
        // a constructor's field by its position too (`#1`, beside `parts`): only the named places are the answer's
        let named = |o: &&Origin| !o.1.iter().any(|s| s.starts_with('#'));
        for x in a.iter().filter(named) {
            for y in k.iter().filter(named).filter(|y| y.0 == x.0) {
                found.push((x.0.clone(), (self.named(x), self.named(y))));
            }
        }
        for (step, rule) in found {
            self.names.entry(step).or_default().insert(rule);
        }
    }

    fn expr(&self, e: &Expr, env: &Env) -> Av {
        match e {
            Expr::Var(x) => env.get(x).cloned().unwrap_or_default(),
            Expr::Field(e, f) => self.expr(e, env).field(f),
            Expr::Con { name, args, fields } => args.iter().enumerate().fold(Av::default(), |acc, (i, a)| {
                let f = fields.get(i).cloned().unwrap_or(format!("#{i}"));
                acc.union(self.expr(a, env).under(&[name.clone(), f]))
            }),
            Expr::List(xs) => xs.iter().fold(Av::default(), |acc, x| acc.union(self.expr(x, env).under(&["[]".into()]))),
            Expr::Tuple(xs) => xs.iter().enumerate().fold(Av::default(), |acc, (i, x)| acc.union(self.expr(x, env).under(&[format!("#{i}")]))),
            Expr::Record(fs) => fs.iter().fold(Av::default(), |acc, (f, x)| acc.union(self.expr(x, env).under(&[f.clone()]))),
            Expr::Find { e, .. } => self.expr(e, env).at("[]"),
            Expr::Append(a, b) => self.expr(a, env).union(self.expr(b, env)),
            _ => Av::default(),
        }
    }

    fn bind(&self, p: &Pat, v: &Av, env: &mut Env) {
        match p {
            Pat::Var(x) => {
                env.insert(x.clone(), v.clone());
            }
            Pat::Con { name, args, fields } => {
                for (i, a) in args.iter().enumerate() {
                    self.bind(a, &v.con_field(name, i, fields.get(i).map(String::as_str)), env);
                }
            }
            Pat::Tuple(ps) => {
                for (i, a) in ps.iter().enumerate() {
                    self.bind(a, &v.at(&format!("#{i}")), env);
                }
            }
            Pat::One(p) => self.bind(p, &v.at("[]"), env),
            Pat::Wild | Pat::Nil | Pat::None => {}
        }
    }

    /// What P gives, the steps it reaches written down.
    fn prog(&mut self, p: &Prog, env: &Env) -> Av {
        match p {
            Prog::Return { e, .. } => self.expr(e, env),
            Prog::Step { w, call, .. } => {
                let worker = match w {
                    Worker::Named(x) => env.get(x).cloned().unwrap_or_default(),
                    Worker::Expr(e) => self.expr(e, env),
                    _ => Av::default(),
                };
                self.given(&worker, call);
                Av::from(((self.here.last().cloned().unwrap_or_default(), call.clone()), Vec::new()))
            }
            Prog::Open { kind, args, .. } => {
                let args: Vec<Av> = args.iter().map(|a| self.expr(a, env)).collect();
                self.kind(kind, args)
            }
            Prog::Bind { x, first, rest } => {
                let v = self.prog(first, env);
                let mut env = env.clone();
                match x {
                    Bound::Var(x) => {
                        env.insert(x.clone(), v);
                    }
                    Bound::Tuple(xs) => {
                        for (i, x) in xs.iter().enumerate() {
                            env.insert(x.clone(), v.at(&format!("#{i}")));
                        }
                    }
                }
                self.prog(rest, &env)
            }
            Prog::Match { e, arms, .. } => {
                let v = self.expr(e, env);
                let mut out = Av::default();
                for (pat, body) in arms {
                    let mut env = env.clone();
                    self.bind(pat, &v, &mut env);
                    out = out.union(self.prog(body, &env));
                }
                out
            }
            Prog::Par { branches, each, .. } | Prog::Any { branches, each, .. } => match each {
                Some(each) => {
                    let mut env = env.clone();
                    env.insert(each.x.clone(), self.expr(&each.e, &env).at("[]"));
                    if let Some(o) = &each.order {
                        self.named_by(&self.expr(&o.after, &env), &self.expr(&o.key, &env));
                    }
                    branches.iter().fold(Av::default(), |acc, b| acc.union(self.prog(b, &env).under(&["[]".into()])))
                }
                None => {
                    let any = matches!(p, Prog::Any { .. });
                    branches.iter().enumerate().fold(Av::default(), |acc, (i, b)| {
                        let v = self.prog(b, env);
                        acc.union(if any { v } else { v.under(&[format!("#{i}")]) })
                    })
                }
            },
            Prog::Iterate { x, init, body, .. } => {
                // two rounds: the first with the initial value, the second with what an `Again` gives too; a third
                // would only follow the same answers again (one wrapping its value would grow it each round)
                let mut seen = self.expr(init, env);
                let mut out = Av::default();
                for _ in 0..2 {
                    let mut env = env.clone();
                    env.insert(x.clone(), seen.clone());
                    let v = self.prog(body, &env);
                    out = out.union(v.con_field("Done", 0, None)).union(v.at("Done").field_any());
                    seen = seen.union(v.con_field("Again", 0, None)).union(v.at("Again").field_any());
                }
                out
            }
            Prog::For { x, e, body, .. } => {
                let mut env = env.clone();
                env.insert(x.clone(), self.expr(e, &env).at("[]"));
                self.prog(body, &env).under(&["[]".into()])
            }
            Prog::Layer { body, .. } | Prog::During { body, .. } => self.prog(body, env),
            Prog::With { body, .. } => self.prog(body, env),
            Prog::LetAgent { x, from, body, .. } => {
                let mut env = env.clone();
                env.insert(x.clone(), from.as_ref().map(|e| self.expr(e, &env)).unwrap_or_default());
                self.prog(body, &env)
            }
            Prog::Try { body, arms, .. } => {
                let out = self.prog(body, env);
                arms.iter().fold(out, |acc, h| {
                    let mut env = env.clone();
                    if let Some(x) = &h.x {
                        env.remove(x);
                    }
                    acc.union(self.prog(&h.body, &env))
                })
            }
            _ => Av::default(),
        }
    }

    /// What kind K gives, opened with ARGS for its form; its steps and handlers written down.
    fn kind(&mut self, k: &str, args: Vec<Av>) -> Av {
        let key = (k.to_string(), args.clone());
        if let Some(v) = self.done.get(&key) {
            return v.clone();
        }
        let Some(kind) = self.kinds.get(k) else { return Av::default() };
        if !self.busy.insert(key.clone()) {
            return Av::default();
        }
        let env: Env = kind.form.iter().map(|(f, _)| f.clone()).zip(args.into_iter().chain(std::iter::repeat(Av::default()))).collect();
        self.here.push(k.to_string());
        let v = self.prog(&kind.pattern, &env);
        for h in &kind.handlers {
            let mut env = env.clone();
            env.remove(&h.x);
            self.prog(&h.body, &env);
        }
        self.here.pop();
        self.busy.remove(&key);
        self.done.insert(key, v.clone());
        v
    }
}

impl Av {
    /// The one field under a constructor whose field name is not known here (`Done(x)`, `Again(x)`).
    fn field_any(&self) -> Av {
        let mut out = Av::default();
        for (p, os) in &self.0 {
            if let [f, rest @ ..] = p.as_slice() {
                if !f.starts_with('#') {
                    out.0.entry(rest.to_vec()).or_default().extend(os.iter().cloned());
                }
            }
        }
        out
    }
}

/// For each kind and each call of a step in it, the places of its answer whose agent is later the worker of a step, with those steps' calls in the
/// order the programs reach them: each kind of KINDS followed from its start, and into each kind it opens.
pub fn agents_given(kinds: &BTreeMap<String, Kind>, calls: &BTreeMap<String, CallSig>) -> Given {
    walk(kinds, calls).out
}

/// For each kind and each call of a step in it, the lists of its answer whose entries name another list's elements
/// (`Names`): each kind of KINDS followed from its start, and into each kind it opens.
pub fn names_given(kinds: &BTreeMap<String, Kind>, calls: &BTreeMap<String, CallSig>) -> Names {
    walk(kinds, calls).names
}

fn walk<'a>(kinds: &'a BTreeMap<String, Kind>, calls: &'a BTreeMap<String, CallSig>) -> Walk<'a> {
    let mut w = Walk { kinds, calls, out: Given::new(), names: Names::new(), busy: BTreeSet::new(), done: BTreeMap::new(), here: Vec::new() };
    for (name, k) in kinds {
        w.kind(name, vec![Av::default(); k.form.len()]);
    }
    w
}
