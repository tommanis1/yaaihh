//! A specification's derivation rules and stated facts as an answer set program for Clingo, after GPCE 2025's
//! translation of eFLINT, for model checking and scenario search offline: nothing the engine runs reads it.

mod write;

use self::write::*;
use crate::ast::*;
use crate::compile::{Spec, free};
use crate::eval::State;
use std::collections::BTreeMap;

pub use self::write::instance;

/// The program for SPEC with what STATE states. Each instance is a term named by its type, `holds(I)` what holds and
/// `dom(T, I)` the instances a variable of type T ranges over as the engine's ranging reading has them: all of an
/// enumerable type (its domain constraint met), else those that hold and the values holding instances give it. A rule
/// it cannot say (a host function, `CurrentTime`, `Violated`, `Enabled`) is left out, with a comment naming it and
/// where it is written.
pub fn export(spec: &Spec, state: &State) -> String {
    let mut x = Ex { spec, rules: Vec::new(), aux: 0, fresh: 0, fields: BTreeMap::new(), oks: BTreeMap::new() };
    for (d, t) in &spec.decls {
        x.domain(d, t);
    }
    for (te, i) in state.contents() {
        if i.value && !i.derived {
            x.rules.push(format!("holds({}).", instance(te)));
        }
    }
    for (d, t) in spec.decls.iter().filter(|(d, _)| !spec.tests.contains(*d)) {
        for c in &t.derivation {
            let term = match c {
                Derivation::HoldsWhen(x) | Derivation::Dv(_, x) => x,
            };
            // each rule's variables are its own, and a rule it cannot say leaves nothing of itself behind
            let (rules, oks) = (x.rules.len(), x.oks.clone());
            let made = x.rule(d, t, c);
            x.fields.clear();
            if let Err(why) = made {
                (x.rules.truncate(rules), x.oks = oks);
                let at = spec.place(d, term).map(|a| format!(" ({a})")).unwrap_or_default();
                x.rules.push(comment(&format!("not exported: a rule of {d}{at}: {why}")));
            }
        }
    }
    let mut out = String::from("% eFLINT's derivation rules and stated facts, exported by eflint-rs; acts' effects are left out.\n");
    for r in x.rules {
        out.push_str(&r);
        out.push('\n');
    }
    out.push_str("#show holds/1.\n");
    out
}

struct Ex<'a> {
    spec: &'a Spec,
    rules: Vec<String>,
    aux: usize,
    fresh: usize,
    /// The fresh variables standing for the fields of each composite variable the rule being made ranges over.
    fields: BTreeMap<Var, Vec<String>>,
    /// The types whose domain constraint has its `ok_` rules, or why it has none.
    oks: BTreeMap<Id, Result<(), String>>,
}

impl Ex<'_> {
    /// The instances of a primitive type D a variable ranges over, and the value each stands for (`raw`).
    fn domain(&mut self, d: &Id, t: &TypeSpec) {
        let (n, q) = (name(d), quote(d));
        match &t.domain {
            Domain::Strings(ss) => ss.iter().for_each(|s| self.rules.push(format!("dom({q}, {n}({})).", quote(s)))),
            Domain::Ints(is) => is.iter().for_each(|i| self.rules.push(format!("dom({q}, {n}({i}))."))),
            Domain::AnyString | Domain::AnyInt => {
                self.rules.push(format!("dom({q}, {n}(R)) :- holds({n}(R))."));
                self.occurring(d);
            }
            Domain::Products(fs) if !fs.is_empty() && !self.spec.enumerable(&t.domain) => {
                let vs: Vec<String> = (0..fs.len()).map(|j| format!("F{j}")).collect();
                self.rules.push(format!("dom({q}, {n}({vs})) :- holds({n}({vs})).", vs = vs.join(", ")));
                self.occurring(d);
            }
            Domain::Products(_) | Domain::Time => {}
        }
        if !matches!(t.domain, Domain::Products(_)) {
            self.rules.push(format!("raw({n}(R), R) :- dom({q}, {n}(R))."));
        }
    }

    /// The instances of D that stand in a field of a holding instance, as D ranges over them.
    fn occurring(&mut self, d: &Id) {
        let q = quote(d);
        for (p, u) in &self.spec.decls {
            let Domain::Products(fs) = &u.domain else { continue };
            let vs: Vec<String> = (0..fs.len()).map(|j| format!("F{j}")).collect();
            for (j, _) in fs.iter().enumerate().filter(|(_, f)| self.spec.chase(&f.dom) == *d) {
                self.rules.push(format!("dom({q}, F{j}) :- holds({}({})).", name(p), vs.join(", ")));
            }
        }
    }

    /// One derivation clause C of type D as rules, one for each alternative of its condition and D's conditions.
    fn rule(&mut self, d: &Id, t: &TypeSpec, c: &Derivation) -> R<()> {
        let (vars, head, cond): (Vec<Var>, Term, Term) = match (c, &t.domain) {
            (Derivation::HoldsWhen(x), Domain::Products(fs)) => (fs.clone(), Term::App(d.clone(), Args::Mods(Vec::new())), x.clone()),
            (Derivation::HoldsWhen(x), _) => (vec![Var::plain(d)], Term::Ref(Var::plain(d)), x.clone()),
            (Derivation::Dv(xs, Term::Bin(BinOp::When, a, b)), _) => (xs.clone(), (**a).clone(), (**b).clone()),
            (Derivation::Dv(xs, a), _) => (xs.clone(), a.clone(), Term::BoolLit(true)),
        };
        let mut lits = self.doms(&vars)?;
        let (i, _) = self.val(&head, &mut lits)?;
        let mut alts = vec![lits];
        for k in std::iter::once(&cond).chain(&t.conditions) {
            alts = cross(alts, self.alts(k)?);
        }
        for a in alts {
            self.rules.push(if a.is_empty() { format!("holds({i}).") } else { format!("holds({i}) :- {}.", a.join(", ")) });
        }
        Ok(())
    }

    /// What binds each of VARS: a primitive one by its type's instances, a composite one by its fields, each in turn.
    fn doms(&mut self, vars: &[Var]) -> R<Body> {
        let mut out = Vec::new();
        for v in vars {
            if let Some(gs) = self.dom_of(&self.spec.type_of(v), &var(v), &mut out)? {
                self.fields.insert(v.clone(), gs);
            }
        }
        Ok(out)
    }

    /// What binds each of VARS again, a composite one to the fields' variables it was bound by, so that what reads
    /// them reads the same.
    fn again(&mut self, vars: &[Var]) -> R<Body> {
        let mut out = Vec::new();
        for v in vars {
            let ty = self.spec.type_of(v);
            let Some(gs) = self.fields.get(v).cloned() else {
                out.extend(self.doms(std::slice::from_ref(v))?);
                continue;
            };
            let Some(Domain::Products(fs)) = self.spec.find(&ty).map(|t| t.domain.clone()) else { continue };
            if self.spec.enumerable(&Domain::Products(fs.clone())) {
                for (f, g) in fs.iter().zip(&gs) {
                    self.dom_of(&self.spec.type_of(f), g, &mut out)?;
                }
                self.ok(&ty, &var(v), &mut out)?;
            } else {
                out.push(format!("dom({}, {})", quote(&ty), var(v)));
            }
            out.push(format!("{} = {}({})", var(v), name(&ty), gs.join(", ")));
        }
        Ok(out)
    }

    /// OUT gains what binds TERM to an instance of type TY as a variable of it ranges; its fields' variables, where
    /// TY is composite.
    fn dom_of(&mut self, ty: &Id, term: &str, out: &mut Body) -> R<Option<Vec<String>>> {
        let Some(t) = self.spec.find(ty) else {
            out.push(format!("dom({}, {term})", quote(ty)));
            return Ok(None);
        };
        let enumerable = self.spec.enumerable(&t.domain);
        match t.domain.clone() {
            Domain::Products(fs) if enumerable => {
                let gs: Vec<String> = fs.iter().map(|_| self.fresh()).collect();
                for (f, g) in fs.iter().zip(&gs) {
                    self.dom_of(&self.spec.type_of(f), g, out)?;
                }
                out.push(match gs.is_empty() {
                    true => format!("{term} = {}", name(ty)),
                    false => format!("{term} = {}({})", name(ty), gs.join(", ")),
                });
                self.ok(ty, term, out)?;
                Ok((!gs.is_empty()).then_some(gs))
            }
            Domain::Products(fs) => {
                let gs: Vec<String> = fs.iter().map(|_| self.fresh()).collect();
                out.push(format!("dom({}, {term})", quote(ty)));
                out.push(format!("{term} = {}({})", name(ty), gs.join(", ")));
                Ok(Some(gs))
            }
            _ => {
                out.push(format!("dom({}, {term})", quote(ty)));
                if enumerable {
                    self.ok(ty, term, out)?;
                }
                Ok(None)
            }
        }
    }

    /// OUT gains that TERM, of the enumerable type TY, meets TY's domain constraint, where TY has one: `ok_` of it,
    /// whose rules bind it by TY's own domain.
    fn ok(&mut self, ty: &Id, term: &str, out: &mut Body) -> R<()> {
        let Some(t) = self.spec.find(ty) else { return Ok(()) };
        if t.domain_constraint == Term::BoolLit(true) {
            return Ok(());
        }
        let atom = |i: &str| format!("ok_{}({i})", name(ty));
        if !self.oks.contains_key(ty) {
            // marked first, so a constraint that reads its own type ends
            self.oks.insert(ty.clone(), Ok(()));
            let fields = std::mem::take(&mut self.fields);
            let made = (|| -> R<()> {
                let (head, bound) = match &t.domain {
                    Domain::Products(fs) => {
                        let head = format!("{}({})", name(ty), fs.iter().map(var).collect::<Vec<_>>().join(", "));
                        (if fs.is_empty() { name(ty) } else { head }, self.doms(fs)?)
                    }
                    _ => (var(&Var::plain(ty)), vec![format!("dom({}, {})", quote(ty), var(&Var::plain(ty)))]),
                };
                for a in self.alts(&t.domain_constraint)? {
                    self.rules.push(format!("{} :- {}.", atom(&head), [bound.clone(), a].concat().join(", ")));
                }
                Ok(())
            })();
            self.fields = fields;
            self.oks.insert(ty.clone(), made);
        }
        self.oks[ty].clone()?;
        out.push(atom(term));
        Ok(())
    }

    /// The variables of the rule being made kept as they are around M: a variable M binds again is M's own.
    fn scoped<T>(&mut self, m: impl FnOnce(&mut Self) -> R<T>) -> R<T> {
        let fields = self.fields.clone();
        let r = m(self);
        self.fields = fields;
        r
    }

    /// LITS gains what makes condition C hold: its literals where it has one alternative, else an atom of them.
    fn holding(&mut self, c: &Term, lits: &mut Body) -> R<()> {
        let alts = self.alts(c)?;
        match alts.as_slice() {
            [] => lits.push("#false".into()),
            [one] => lits.extend(one.iter().cloned()),
            _ => lits.push(self.aux(c, alts)?),
        }
        Ok(())
    }

    /// The alternatives of a condition, each the literals that make it hold.
    fn alts(&mut self, t: &Term) -> R<Vec<Body>> {
        Ok(match t {
            Term::BoolLit(b) => if *b { vec![Vec::new()] } else { Vec::new() },
            Term::Bin(BinOp::And, a, b) => cross(self.alts(a)?, self.alts(b)?),
            Term::Bin(BinOp::Or, a, b) => [self.alts(a)?, self.alts(b)?].concat(),
            Term::Bin(BinOp::When, a, b) => cross(self.alts(b)?, self.alts(a)?),
            Term::Bin(op @ (BinOp::Eq | BinOp::Neq | BinOp::Le | BinOp::Leq | BinOp::Ge | BinOp::Geq), a, b) => {
                let mut lits = Vec::new();
                // an order compares numbers, as the engine's does: each side its plain value
                let (x, y) = match op {
                    BinOp::Eq | BinOp::Neq => (self.val(a, &mut lits)?.0, self.val(b, &mut lits)?.0),
                    _ => (self.raw(a, &mut lits)?, self.raw(b, &mut lits)?),
                };
                let o = match op {
                    BinOp::Eq => "=",
                    BinOp::Neq => "!=",
                    BinOp::Le => "<",
                    BinOp::Leq => "<=",
                    BinOp::Ge => ">",
                    _ => ">=",
                };
                lits.push(format!("{x} {o} {y}"));
                vec![lits]
            }
            Term::Un(UnOp::Not, a) => {
                let inner = self.alts(a)?;
                // what never holds is not, and what holds with nothing more is never not
                if inner.is_empty() {
                    return Ok(vec![Vec::new()]);
                }
                if inner.iter().any(|b| b.is_empty()) {
                    return Ok(Vec::new());
                }
                let atom = match &inner[..] {
                    [one] if matches!(&one[..], [l] if l.starts_with("holds(") || l.starts_with("aux")) => one[0].clone(),
                    _ => self.aux(t, inner)?,
                };
                vec![vec![format!("not {atom}")]]
            }
            Term::Q(Quant::Exists, xs, a) => {
                // XS bound here are XS's own: an outer variable of the same name keeps its fields after
                let inner: Vec<Body> = self.scoped(|x| {
                    let bound = x.doms(xs)?;
                    Ok(x.alts(a)?.into_iter().map(|b| [bound.clone(), b].concat()).collect())
                })?;
                if inner.is_empty() {
                    return Ok(Vec::new());
                }
                vec![vec![self.aux(t, inner)?]]
            }
            Term::Q(Quant::Forall, xs, a) => {
                let none = Term::Un(UnOp::Not, Box::new(Term::Q(Quant::Exists, xs.clone(), Box::new(Term::Un(UnOp::Not, a.clone())))));
                self.alts(&none)?
            }
            Term::Un(UnOp::Violated | UnOp::Enabled, _) => return Err("Violated and Enabled are not exported".into()),
            Term::Un(UnOp::Present, _) | Term::Ref(_) | Term::App(..) | Term::Tag(..) | Term::Project(..) => {
                let a = if let Term::Un(_, a) = t { &**a } else { t };
                let mut lits = Vec::new();
                let (i, _) = self.val(a, &mut lits)?;
                lits.push(format!("holds({i})"));
                vec![lits]
            }
            other => return Err(format!("the condition {other:?} is not exported")),
        })
    }

    /// An atom that holds where ALTS do, over the variables free in WHOLE: a rule for each alternative, each variable
    /// given its domain so that the rule is safe alone.
    fn aux(&mut self, whole: &Term, alts: Vec<Body>) -> R<String> {
        let vars: Vec<Var> = free(self.spec, whole).into_iter().collect();
        self.aux += 1;
        let args: Vec<String> = vars.iter().map(var).collect();
        let atom = if args.is_empty() { format!("aux{}", self.aux) } else { format!("aux{}({})", self.aux, args.join(", ")) };
        for a in alts {
            let body = [self.again(&vars)?, a].concat();
            self.rules.push(if body.is_empty() { format!("{atom}.") } else { format!("{atom} :- {}.", body.join(", ")) });
        }
        Ok(atom)
    }

    /// A value: its term, and the type of the instance it is (`None`: a plain number or string); LITS gains what
    /// binds the fresh variables it uses.
    fn val(&mut self, t: &Term, lits: &mut Body) -> R<(String, Option<Id>)> {
        Ok(match t {
            Term::IntLit(i) => (i.to_string(), None),
            Term::StringLit(s) => (quote(s), None),
            Term::Ref(v) => (var(v), Some(self.spec.type_of(v))),
            Term::Tag(a, d) => {
                let r = self.raw(a, lits)?;
                (format!("{}({r})", name(&self.spec.chase(d))), Some(self.spec.chase(d)))
            }
            Term::Un(UnOp::Untag, a) => (self.raw(a, lits)?, None),
            Term::Bin(op @ (BinOp::Add | BinOp::Sub | BinOp::Mult | BinOp::Div | BinOp::Mod), a, b) => {
                let (x, y) = (self.raw(a, lits)?, self.raw(b, lits)?);
                let o = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mult => "*",
                    // Clingo's `/` and `\\` round towards zero; the engine's towards negative infinity
                    BinOp::Div => return Ok((format!("(({x} - ((({x} \\ {y}) + {y}) \\ {y})) / {y})"), None)),
                    _ => return Ok((format!("((({x} \\ {y}) + {y}) \\ {y})"), None)),
                };
                (format!("({x} {o} {y})"), None)
            }
            Term::App(d, args) if self.spec.find(d).is_some() => {
                let d = self.spec.chase(d);
                let fs = match &self.spec.find(&d).map(|t| t.domain.clone()) {
                    Some(Domain::Products(fs)) => fs.clone(),
                    _ => return Err(format!("{d} applied is not a composite type")),
                };
                let given: Vec<(Var, &Term)> = match args {
                    Args::Pos(ts) => fs.iter().cloned().zip(ts.iter()).collect(),
                    Args::Mods(ms) => ms.iter().map(|(v, t)| (v.clone(), t)).collect(),
                };
                let mut parts = Vec::new();
                for f in &fs {
                    parts.push(match given.iter().find(|(v, _)| v == f) {
                        Some((_, a)) => match self.val(a, lits)? {
                            (r, None) => format!("{}({r})", name(&self.spec.type_of(f))),
                            (i, Some(_)) => i,
                        },
                        None => var(f),
                    });
                }
                let n = name(&d);
                // an instance made meets its type's domain constraint, read with its fields as given
                let constraint = &self.spec.find(&d).expect("found above").domain_constraint;
                if *constraint != Term::BoolLit(true) {
                    let given: BTreeMap<Var, Term> = given.iter().map(|(v, a)| (v.clone(), (*a).clone())).collect();
                    let c = subst(self.spec, constraint, &given)?;
                    self.holding(&c, lits)?;
                }
                (if parts.is_empty() { n } else { format!("{n}({})", parts.join(", ")) }, Some(d))
            }
            Term::Project(a, f) => {
                let Term::Ref(x) = &**a else { return Err("a field of a value that is not a variable".into()) };
                let fs = match self.spec.find(&self.spec.type_of(x)).map(|t| t.domain.clone()) {
                    Some(Domain::Products(fs)) => fs,
                    _ => return Err("a field of a value that is not a composite instance".into()),
                };
                let j = fs.iter().position(|y| y == f).ok_or_else(|| format!("no field {}", f.dom))?;
                let g = self.fields.get(x).and_then(|gs| gs.get(j)).cloned().ok_or_else(|| format!("{} is not ranged over here", x.dom))?;
                (g, Some(self.spec.type_of(f)))
            }
            Term::Q(q @ (Quant::Count | Quant::Sum | Quant::Max | Quant::Min), xs, a) => {
                // one element for each binding of XS and each value it gives, as the engine counts: a count of
                // instances counts them, not their plain values
                let (inner, v) = self.scoped(|x| {
                    let mut inner = x.doms(xs)?;
                    let v = match q {
                        Quant::Count => x.val(a, &mut inner)?.0,
                        _ => x.raw(a, &mut inner)?,
                    };
                    Ok((inner, v))
                })?;
                let tuple = [vec![v], xs.iter().map(var).collect()].concat().join(", ");
                let set = if inner.is_empty() { format!("{{ {tuple} }}") } else { format!("{{ {tuple} : {} }}", inner.join(", ")) };
                let n = self.fresh();
                match q {
                    Quant::Count | Quant::Sum => {
                        lits.push(format!("{n} = {}{set}", if *q == Quant::Count { "#count" } else { "#sum" }));
                    }
                    _ => {
                        // the greatest or least of nothing is 0, as the engine has it, not #inf or #sup
                        let agg = if *q == Quant::Max { "#max" } else { "#min" };
                        let vars: Vec<Var> = free(self.spec, t).into_iter().collect();
                        self.aux += 1;
                        let k = self.aux;
                        let atom = |last: &str| format!("agg{k}({})", [vars.iter().map(var).collect(), vec![last.to_string()]].concat().join(", "));
                        let bound = self.again(&vars)?;
                        let given = |more: String| [bound.clone(), vec![more]].concat().join(", ");
                        self.rules.push(format!("{} :- {}.", atom("N"), given(format!("N = {agg}{set}, #count{set} > 0"))));
                        self.rules.push(format!("{} :- {}.", atom("0"), given(format!("#count{set} = 0"))));
                        lits.push(atom(&n));
                    }
                }
                (n, None)
            }
            Term::Bin(BinOp::When, a, b) => {
                let v = self.val(a, lits)?;
                self.holding(b, lits)?;
                v
            }
            Term::CurrentTime => return Err("CurrentTime is not exported".into()),
            Term::App(d, _) => return Err(format!("the host function {d} is not exported")),
            other => return Err(format!("the value {other:?} is not exported")),
        })
    }

    /// The plain number or string T stands for.
    fn raw(&mut self, t: &Term, lits: &mut Body) -> R<String> {
        if let Term::Tag(a, _) = t {
            // an instance made of a value stands for that value, whether a variable ranges over it or not
            return self.raw(a, lits);
        }
        match self.val(t, lits)? {
            (r, None) => Ok(r),
            (i, Some(_)) => {
                let r = self.fresh();
                lits.push(format!("raw({i}, {r})"));
                Ok(r)
            }
        }
    }

    fn fresh(&mut self) -> String {
        self.fresh += 1;
        format!("A{}", self.fresh)
    }
}
