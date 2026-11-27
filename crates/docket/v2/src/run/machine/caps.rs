//! The early capability check (spec 12): before a run for a connection takes its first step, each power its text
//! writes where a guarded Effect is handled or performed is a `Lets` line for its driver at that point, or stands
//! under a test of one.

use super::*;
use crate::run::norms::NormDecl;
use crate::syntax::lex::At;
use std::collections::BTreeSet;

/// A point as the text gives it: a literal, a variable bound there, or neither.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Point {
    Lit(String),
    Var(String),
    Any,
}

/// The (point, power) pairs the tests around a place have shown to be `Lets` lines.
type Guards = BTreeSet<(Point, String)>;

/// The arm or `perform` of a guarded Effect whose answer or arguments a constructor written now is part of.
#[derive(Clone, Debug, PartialEq)]
struct Place {
    effect: String,
    point: Point,
    at: At,
}

struct Walk<'a> {
    syms: &'a BTreeMap<String, Sym>,
    norms: &'a [NormDecl],
    lets: String,
    /// Each guarded Effect and the place of its point field.
    guarded: BTreeMap<String, usize>,
    /// Each function walked, with the guards it was walked under.
    seen: BTreeSet<(String, Vec<(Point, String)>)>,
    /// The pairs no test covers, to be found among the `Lets` lines.
    owed: Vec<(Place, String)>,
}

impl<N: Norms> Machine<N> {
    /// The check for a run of `party`'s Act `act` bound to driver `d`: its body and the functions it calls, every root arm
    /// and every `Does` of a guarded Effect. `Err` lists each pair missing, with its Effect, its place, the
    /// driver's name, the point and the power.
    pub(super) fn early(&mut self, d: usize, party: &Value, act: &Instance) -> Result<(), String> {
        let body = self.body_of(act).map(|(b, _, _)| b);
        let Some(mut w) = self.walk() else { return Ok(()) };
        if let Some(b) = &body {
            w.expr(b, &Guards::new(), None);
        }
        for o in &self.loaded.ons {
            if let Some(&k) = w.guarded.get(&o.target) {
                let place = Place { effect: o.target.clone(), point: o.pats.get(k).map_or(Point::Any, pat_point), at: o.at };
                let g = o.cond.as_ref().map_or_else(Guards::new, |c| w.tests(c));
                w.expr(&o.body, &g, Some(&place));
            }
        }
        for (e, s) in w.syms {
            if let (Some(&k), Sym::Effect { does: Some(b), fields, .. }) = (w.guarded.get(e), s) {
                let place = Place { effect: e.clone(), point: Point::Var(fields[k].name.clone()), at: b.at };
                w.expr(b, &Guards::new(), Some(&place));
            }
        }
        let owed = std::mem::take(&mut w.owed);
        self.missing(d, party, owed)
    }

    /// The check for a filling (8.6) of `party`'s run bound to driver `d`: its own text and the functions it calls.
    pub(super) fn early_filling(&mut self, d: usize, party: &Value, e: &Expr) -> Result<(), String> {
        let Some(mut w) = self.walk() else { return Ok(()) };
        w.expr(e, &Guards::new(), None);
        let owed = std::mem::take(&mut w.owed);
        self.missing(d, party, owed)
    }

    fn walk(&self) -> Option<Walk<'_>> {
        let (lets, pt) = self.loaded.lets_fact.clone()?;
        Some(Walk { syms: &self.loaded.syms, norms: &self.loaded.norms, lets, guarded: guarded(&self.loaded.syms, &pt), seen: BTreeSet::new(), owed: Vec::new() })
    }

    /// The pairs of `owed` that are no `Lets` line for driver `d` as it carries for `party`.
    fn missing(&mut self, d: usize, party: &Value, owed: Vec<(Place, String)>) -> Result<(), String> {
        let Some((_, pt)) = self.loaded.lets_fact.clone() else { return Ok(()) };
        let name = self.drivers[d].name_for(party);
        let mut missing = Vec::new();
        for (place, power) in owed {
            let points = match &place.point {
                Point::Lit(t) => vec![Value::Text(t.clone())],
                _ => self.instances(&pt, &["p".to_string()])?.into_iter().filter_map(|i| i.args.into_iter().next()).collect(),
            };
            for p in points {
                if !self.loaded.lets.lets(&name, &p, &Value::Text(power.clone())) {
                    let d: Vec<String> = name.iter().map(Value::to_string).collect();
                    missing.push(format!("{} at {}: Lets({}, {p}, {power:?}) is not a line of the loaded text", place.effect, place.at, d.join(", ")));
                }
            }
        }
        missing.dedup();
        if missing.is_empty() { Ok(()) } else { Err(missing.join("; ")) }
    }
}

/// The Effects whose result or a field's type has a constructor that needs a power, each with its point field.
fn guarded(syms: &BTreeMap<String, Sym>, pt: &str) -> BTreeMap<String, usize> {
    let needs = |t: &Type| match t {
        Type::Named(p, _) => matches!(syms.get(&p.to_string()), Some(Sym::Sum { ctors, .. })
            if ctors.iter().any(|c| matches!(syms.get(c), Some(Sym::Ctor { needs: Some(_), .. })))),
        _ => false,
    };
    syms.iter()
        .filter_map(|(n, s)| match s {
            Sym::Effect { fields, result, .. } if needs(result) || fields.iter().any(|f| needs(&f.ty)) => {
                let k = fields.iter().position(|f| matches!(&f.ty, Type::Named(p, _) if p.to_string() == pt))?;
                Some((n.clone(), k))
            }
            _ => None,
        })
        .collect()
}

fn pat_point(p: &Pat) -> Point {
    match p {
        Pat::Text(t) => Point::Lit(t.clone()),
        Pat::Bind(v) => Point::Var(v.clone()),
        _ => Point::Any,
    }
}

fn expr_point(e: &Expr) -> Point {
    match &e.e {
        E::Text(ps) => match &ps[..] {
            [] => Point::Lit(String::new()),
            [Piece::Words(w)] => Point::Lit(w.clone()),
            _ => Point::Any,
        },
        E::Var(v) => Point::Var(v.clone()),
        _ => Point::Any,
    }
}

/// The variables a pattern binds.
fn binds(p: &Pat, out: &mut Vec<String>) {
    match p {
        Pat::Bind(v) => out.push(v.clone()),
        Pat::Ctor(_, ps) | Pat::Tuple(ps) => ps.iter().for_each(|q| binds(q, out)),
        Pat::List(ps, rest) => {
            ps.iter().for_each(|q| binds(q, out));
            if let Some(Some(r)) = rest {
                out.push(r.clone());
            }
        }
        _ => {}
    }
}

/// `g` without what it says of variables that `names` binds anew.
fn unbind(g: &Guards, names: &[String]) -> Guards {
    g.iter().filter(|(p, _)| !matches!(p, Point::Var(v) if names.contains(v))).cloned().collect()
}

impl Walk<'_> {
    fn expr(&mut self, e: &Expr, g: &Guards, at: Option<&Place>) {
        match &e.e {
            E::Apply(p, xs) => {
                let n = p.to_string();
                match self.syms.get(&n) {
                    Some(Sym::Ctor { needs: Some(w), .. }) => self.need(at, w, g),
                    Some(Sym::Fn { body, .. }) => self.function(&n, body, g),
                    _ => {}
                }
                xs.iter().for_each(|x| self.expr(x, g, at));
            }
            E::Name(p) => {
                if let Some(Sym::Ctor { needs: Some(w), .. }) = self.syms.get(&p.to_string()) {
                    self.need(at, w, g);
                }
            }
            E::Perform(p, xs) => {
                let n = p.to_string();
                let place = self.guarded.get(&n).map(|&k| Place { effect: n.clone(), point: xs.get(k).map_or(Point::Any, expr_point), at: e.at });
                xs.iter().for_each(|x| self.expr(x, g, place.as_ref().or(at)));
            }
            E::Handle(body, ret, arms) => {
                self.expr(body, g, at);
                if let Some((v, r)) = ret {
                    self.expr(r, &unbind(g, std::slice::from_ref(v)), at);
                }
                for a in arms {
                    let mut names = vec![a.k.clone()];
                    a.pats.iter().for_each(|p| binds(p, &mut names));
                    let mut g2 = unbind(g, &names);
                    if let Some(c) = &a.cond {
                        g2.extend(self.tests(c));
                    }
                    let n = a.effect.to_string();
                    let place = self.guarded.get(&n).map(|&k| Place { effect: n.clone(), point: a.pats.get(k).map_or(Point::Any, pat_point), at: a.at });
                    self.expr(&a.body, &g2, place.as_ref().or(at));
                }
            }
            E::If(c, a, b) => {
                let mut g2 = g.clone();
                g2.extend(self.tests(c));
                self.expr(a, &g2, at);
                if let Some(b) = b {
                    self.expr(b, g, at);
                }
            }
            E::Match(x, cases) => {
                self.expr(x, g, at);
                for c in cases {
                    let mut names = Vec::new();
                    binds(&c.pat, &mut names);
                    let mut g2 = unbind(g, &names);
                    if let Some(t) = &c.cond {
                        g2.extend(self.tests(t));
                    }
                    self.expr(&c.body, &g2, at);
                }
            }
            E::Bind(x, p, rest) => {
                self.expr(p, g, at);
                self.expr(rest, &unbind(g, std::slice::from_ref(x)), at);
            }
            E::ParEach(x, l, _, b) | E::Iterate(x, l, _, b) => {
                self.expr(l, g, at);
                self.expr(b, &unbind(g, std::slice::from_ref(x)), at);
            }
            E::Text(ps) => ps.iter().for_each(|p| {
                if let Piece::Hole(h) = p {
                    self.expr(h, g, at)
                }
            }),
            E::Seq(a, b) | E::Bin(_, a, b) => {
                self.expr(a, g, at);
                self.expr(b, g, at);
            }
            E::Field(a, _) | E::Neg(a) | E::Not(a) | E::Holds(a) | E::Violated(a) | E::Enabled(a) | E::Quant(_, _, a) | E::Agg(_, a) | E::Return(a) | E::Resume(_, a) | E::Do(a) => self.expr(a, g, at),
            E::Foreach(_, v, c) => {
                self.expr(v, g, at);
                if let Some(c) = c {
                    self.expr(c, g, at);
                }
            }
            E::Tuple(xs) | E::List(xs) | E::Par(xs) => xs.iter().for_each(|x| self.expr(x, g, at)),
            E::Lit(_) | E::Var(_) | E::SelfParty | E::This | E::Gap => {}
        }
    }

    /// A function the text calls: its own arms and `perform`s are checked under the caller's tests on literal
    /// points (a variable there is another binding); a constructor it writes outside them is no arm's (the boundary
    /// covers it).
    fn function(&mut self, n: &str, body: &Expr, g: &Guards) {
        let g: Guards = g.iter().filter(|(p, _)| matches!(p, Point::Lit(_))).cloned().collect();
        if self.seen.insert((n.to_string(), g.iter().cloned().collect())) {
            self.expr(body, &g, None);
        }
    }

    fn need(&mut self, at: Option<&Place>, power: &str, g: &Guards) {
        let Some(place) = at else { return };
        if place.point != Point::Any && g.contains(&(place.point.clone(), power.to_string())) {
            return;
        }
        let pair = (place.clone(), power.to_string());
        if !self.owed.contains(&pair) {
            self.owed.push(pair);
        }
    }

    /// The pairs a condition shows to be `Lets` lines when it holds: its `Lets` atoms, and those of a fact whose
    /// every rule has one, through `&&`, `Exists` and `Holds`.
    fn tests(&self, c: &Expr) -> Guards {
        let mut out = Vec::new();
        self.atoms(c, &mut out, true);
        out.into_iter().filter_map(|(p, w)| if let Point::Lit(w) = w { Some((p, w)) } else { None }).collect()
    }

    /// Each `Lets` atom's point and power as written; with `rules`, also what a tested fact's rules give.
    fn atoms(&self, c: &Expr, out: &mut Vec<(Point, Point)>, rules: bool) {
        match &c.e {
            E::Bin(Op::And, a, b) => {
                self.atoms(a, out, rules);
                self.atoms(b, out, rules);
            }
            E::Quant(Quant::Exists, vs, b) => {
                // a pair on a variable the quantifier binds says nothing of the variable of that name outside
                let mut inner = Vec::new();
                self.atoms(b, &mut inner, rules);
                let bound = |p: &Point| matches!(p, Point::Var(v) if vs.contains(v));
                out.extend(inner.into_iter().filter(|(p, w)| !bound(p) && !bound(w)));
            }
            E::Holds(b) => self.atoms(b, out, rules),
            E::Apply(p, xs) if p.to_string() == self.lets => {
                if let [.., point, power] = &xs[..] {
                    out.push((expr_point(point), expr_point(power)));
                }
            }
            E::Apply(p, xs) if rules => out.extend(self.by_rules(&p.to_string(), xs).into_iter().map(|(p, w)| (p, Point::Lit(w)))),
            _ => {}
        }
    }

    /// For a test of fact `f` with arguments `xs`: the pairs every rule of `f` tests with a `Lets` atom, its fields
    /// given as `xs`.
    fn by_rules(&self, f: &str, xs: &[Expr]) -> Guards {
        let Some(Sym::Norm { kind: Norm::Fact, fields: Fields::Named(fs), .. }) = self.syms.get(f) else { return Guards::new() };
        let rules: Vec<&Expr> = self
            .norms
            .iter()
            .filter(|d| match &d.decl.what {
                DeclKind::Norm { name, .. } => crate::check::names::qualify(&d.module, name) == f,
                DeclKind::Extend { name, .. } => name.to_string() == f,
                _ => false,
            })
            .flat_map(|d| match &d.decl.what {
                DeclKind::Norm { clauses, .. } | DeclKind::Extend { clauses, .. } => clauses.iter().filter_map(|c| if let Clause::HoldsWhen(e) = c { Some(e) } else { None }).collect(),
                _ => Vec::new(),
            })
            .collect();
        let field = |p: Point| match p {
            // a rule's variable that is no field is its own, bound in the rule
            Point::Var(v) => fs.iter().position(|f| f.name == v).and_then(|k| xs.get(k)).map_or(Point::Any, expr_point),
            other => other,
        };
        let mut all: Option<Guards> = None;
        for r in rules {
            let mut own = Vec::new();
            self.atoms(r, &mut own, false);
            let own: Guards = own
                .into_iter()
                .filter_map(|(p, w)| match field(w) {
                    Point::Lit(w) => Some((field(p), w)),
                    _ => None,
                })
                .collect();
            all = Some(match all {
                None => own,
                Some(a) => a.intersection(&own).cloned().collect(),
            });
        }
        all.unwrap_or_default().into_iter().filter(|(p, _)| *p != Point::Any).collect()
    }
}
