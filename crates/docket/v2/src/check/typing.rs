//! Typing (spec 7; load checks 3, 6 and 8): every program of a loaded text is given a type and a row, the rows of
//! `fn`s found in one pass, since no `fn` stands on a cycle.

use super::names::{LoadError, Loaded, Sym};
use super::types::{Res, Row, Ty, Unify, builtin, con, con1};
use crate::syntax::lex::At;
use crate::syntax::tree::*;
use std::collections::{BTreeMap, BTreeSet};

mod expr;

/// What typing finds of a loaded text: the row of each Act's or Event's `Does` and of each reaction's body, in the
/// order of `Loaded::ons`, for check 4 of 14; and for each gap, by its site, the type its place needs and the row
/// of the declaration it stands in, for its filling (8.6).
#[derive(Clone, Debug, Default)]
pub struct Rows {
    pub acts: BTreeMap<String, BTreeSet<String>>,
    pub ons: Vec<BTreeSet<String>>,
    pub gaps: BTreeMap<String, (Ty, BTreeSet<String>)>,
}

type Env = BTreeMap<String, Ty>;

/// An arm's continuation that may still be resumed here: what it takes and what the handler gives.
type Ks = BTreeMap<String, (Ty, Ty)>;

/// An Act's or Event's parts that typing reads: its module, fields, result and `Does`.
struct ActDecl {
    module: String,
    fields: Vec<Field>,
    result: Option<Type>,
    does: Option<Expr>,
}

struct Typer<'a> {
    u: Unify<'a>,
    acts: BTreeMap<String, ActDecl>,
    /// Each `fn`'s and Act's body row, with the row variable it was found with.
    rows: BTreeMap<String, (Row, usize)>,
    busy: Vec<String>,
    errors: Vec<LoadError>,
    module: String,
    /// The gaps of each declaration being typed, innermost last, with the type each place needs.
    open: Vec<Vec<(String, Ty)>>,
    gaps: Vec<(String, Ty, BTreeSet<String>)>,
}

/// Type every program of `l`: each `fn`, each `Does`, each reaction; `Err` lists every declaration that fails.
pub fn check(l: &Loaded) -> Result<Rows, Vec<LoadError>> {
    let mut t = Typer::new(l);
    let mut out = Rows::default();
    for (n, s) in &l.syms {
        match s {
            Sym::Fn { .. } => {
                t.row_of(n, At::default());
            }
            Sym::Effect { params, fields, result, does: Some(b), module } => {
                t.module = module.clone();
                let ps: BTreeMap<String, Ty> = params.iter().map(|p| (p.clone(), Ty::Rigid(p.clone()))).collect();
                let v = t.u.row_var();
                let env: Env = fields.iter().map(|f| (f.name.clone(), t.u.ty(&f.ty, &ps, Some(v)))).collect();
                let want = t.u.ty(result, &ps, Some(v));
                t.open.push(Vec::new());
                let r = t.prog(b, &env, &Ks::new()).and_then(|(got, r)| t.u.unify(&want, &got, tail(b)).map(|_| r));
                t.leave(r.as_ref().ok());
                t.note(r.map(|_| ()));
            }
            Sym::Call { fields, template, .. } => {
                let env: Env = fields.iter().map(|f| (f.name.clone(), t.u.ty(&f.ty, &BTreeMap::new(), None))).collect();
                for p in template {
                    if let Piece::Hole(h) = p {
                        let r = t.expr(h, &mut env.clone()).map(|_| ());
                        t.note(r);
                    }
                }
            }
            _ => {}
        }
    }
    let names: Vec<String> = t.acts.keys().cloned().collect();
    for n in names {
        let r = t.row_of(&n, At::default());
        out.acts.insert(n, r.effs);
    }
    for o in &l.ons {
        t.module = o.module.clone();
        t.open.push(Vec::new());
        let r = t.reaction(o);
        t.leave(r.as_ref().ok());
        out.ons.push(r.as_ref().map(|r| r.effs.clone()).unwrap_or_default());
        t.note(r.map(|_| ()));
    }
    out.gaps = t.gaps.iter().map(|(site, ty, r)| (site.clone(), (t.u.found(ty), r.clone()))).collect();
    if t.errors.is_empty() { Ok(out) } else { Err(t.errors) }
}

/// Whether condition `c` reads what holds (a fact, `Holds`, a quantifier): then the normative core reads it whole,
/// as the machine gives it.
pub fn reads_facts(syms: &BTreeMap<String, Sym>, c: &Expr) -> bool {
    match &c.e {
        E::Holds(_) | E::Violated(_) | E::Enabled(_) | E::Quant(..) => true,
        E::Apply(p, _) => matches!(syms.get(&p.to_string()), Some(Sym::Norm { kind: Norm::Fact | Norm::Duty, .. })),
        E::Not(x) => reads_facts(syms, x),
        E::Bin(Op::And | Op::Or, a, b) => reads_facts(syms, a) || reads_facts(syms, b),
        _ => false,
    }
}

/// Type filling `e` (8.6) at gap `site` of `l`, in `module`: it has the type its place needs; `Ok` gives the
/// Effects it may perform that the row around the gap does not hold.
pub fn filling(l: &Loaded, module: &str, site: &str, e: &Expr) -> Result<BTreeSet<String>, String> {
    let (want, around) = l.rows.gaps.get(site).ok_or(format!("{site} is no gap of the loaded text"))?;
    let mut t = Typer::new(l);
    t.module = module.to_string();
    let want = t.u.renew(want, &mut BTreeMap::new());
    let said = |(at, why): (At, String)| format!("{at}: {why}");
    let (got, r) = t.prog(e, &Env::new(), &Ks::new()).map_err(said)?;
    t.u.unify(&want, &got, e.at).map_err(said)?;
    match t.errors.first() {
        Some(err) => Err(err.to_string()),
        None => Ok(r.effs.difference(around).cloned().collect()),
    }
}

impl<'a> Typer<'a> {
    fn new(l: &'a Loaded) -> Self {
        let mut acts = BTreeMap::new();
        for d in &l.norms {
            if let DeclKind::Norm { kind: Norm::Act | Norm::Event, name, fields, clauses } = &d.decl.what {
                let fields = if let Fields::Named(fs) = fields { fs.clone() } else { Vec::new() };
                let result = clauses.iter().find_map(|c| if let Clause::Returns(t) = c { Some(t.clone()) } else { None });
                let does = clauses.iter().find_map(|c| if let Clause::Does(b) = c { Some(b.clone()) } else { None });
                acts.insert(super::names::qualify(&d.module, name), ActDecl { module: d.module.clone(), fields, result, does });
            }
        }
        Typer { u: Unify::new(&l.syms), acts, rows: BTreeMap::new(), busy: Vec::new(), errors: Vec::new(), module: String::new(), open: Vec::new(), gaps: Vec::new() }
    }
}

impl Typer<'_> {
    /// The end of a declaration's typing: its gaps take its row.
    fn leave(&mut self, r: Option<&Row>) {
        let effs = r.map(|r| r.effs.clone()).unwrap_or_default();
        for (site, ty) in self.open.pop().unwrap_or_default() {
            self.gaps.push((site, ty, effs.clone()));
        }
    }

    fn note(&mut self, r: Res<()>) {
        if let Err((at, why)) = r {
            self.errors.push(LoadError { module: self.module.clone(), at, why });
        }
    }

    /// The body row of `fn` or Act `n`, typing it the first time; a call back into one being typed is a cycle (3).
    fn row_of(&mut self, n: &str, at: At) -> Row {
        if let Some((r, _)) = self.rows.get(n) {
            return r.clone();
        }
        if self.busy.iter().any(|b| b == n) {
            let cycle = self.busy.join(", ");
            self.errors.push(LoadError { module: self.module.clone(), at, why: format!("{n} calls itself through {cycle}: functions and Acts do not recurse") });
            return Row::default();
        }
        self.busy.push(n.to_string());
        let outer = std::mem::take(&mut self.module);
        self.open.push(Vec::new());
        let v = self.u.row_var();
        let r = match self.u.syms.get(n) {
            Some(Sym::Fn { params, fields, result, body, module, .. }) => {
                self.module = module.clone();
                let ps: BTreeMap<String, Ty> = params.iter().map(|p| (p.clone(), Ty::Rigid(p.clone()))).collect();
                let env: Env = fields.iter().map(|f| (f.name.clone(), self.u.ty(&f.ty, &ps, Some(v)))).collect();
                self.fn_body(&ps, result, body, &env, v)
            }
            _ => match self.acts.get(n) {
                Some(ActDecl { module, fields, result, does: Some(b) }) => {
                    let (module, b) = (module.clone(), b.clone());
                    self.module = module;
                    let env: Env = fields.iter().map(|f| (f.name.clone(), self.u.ty(&f.ty, &BTreeMap::new(), Some(v)))).collect();
                    let want = result.as_ref().map_or(con("Unit"), |t| self.u.ty(t, &BTreeMap::new(), Some(v)));
                    self.prog(&b, &env, &Ks::new()).and_then(|(got, r)| self.u.unify(&want, &got, tail(&b)).map(|_| r))
                }
                _ => Ok(Row::default()),
            },
        };
        let r = match r {
            Ok(r) => r,
            Err((at, why)) => {
                self.errors.push(LoadError { module: self.module.clone(), at, why });
                Row::default()
            }
        };
        self.leave(Some(&r));
        self.module = outer;
        self.busy.pop();
        self.rows.insert(n.to_string(), (r.clone(), v));
        r
    }

    /// A `fn`'s body: a program giving its result, or, for a result `Program<T ! ρ>`, a program giving `T` that
    /// performs no Effect outside a closed `ρ`.
    fn fn_body(&mut self, ps: &BTreeMap<String, Ty>, result: &Type, body: &Expr, env: &Env, v: usize) -> Res<Row> {
        let (got, r) = self.prog(body, env, &Ks::new())?;
        match self.u.ty(result, ps, Some(v)) {
            Ty::Prog(t, declared) => {
                self.u.unify(&t, &got, tail(body))?;
                if declared.rest.is_none() {
                    self.u.unify(&Ty::Prog(t.clone(), declared), &Ty::Prog(t, r.clone()), tail(body))?;
                }
                Ok(r)
            }
            want => self.u.unify(&want, &got, tail(body)).map(|_| r),
        }
    }

    /// A reaction's body (8.7): a root arm's gives the Effect's result; another's may give anything.
    fn reaction(&mut self, o: &super::names::On) -> Res<Row> {
        let mut env = Env::new();
        let (fields, want) = match self.u.syms.get(&o.target) {
            Some(Sym::Effect { params, fields, result, .. }) => {
                let ps: BTreeMap<String, Ty> = params.iter().map(|p| (p.clone(), self.u.rigid(p))).collect();
                let v = self.u.row_var();
                (fields.iter().map(|f| self.u.ty(&f.ty, &ps, Some(v))).collect::<Vec<_>>(), Some(self.u.ty(result, &ps, Some(v))))
            }
            Some(Sym::Norm { fields: Fields::Named(fs), .. }) => (fs.iter().map(|f| self.u.ty(&f.ty, &BTreeMap::new(), None)).collect(), None),
            _ => return Err((o.at, format!("a reaction to {}, which is no Fact, Act, Event or Effect with fields", o.target))),
        };
        if fields.len() != o.pats.len() {
            return Err((o.at, format!("{} has {} fields, and {} patterns are written", o.target, fields.len(), o.pats.len())));
        }
        for (p, t) in o.pats.iter().zip(&fields) {
            self.pat(p, t, &mut env, o.at)?;
        }
        if let Some(c) = &o.cond {
            self.whole(c)?;
            self.cond(c, &mut env)?;
        }
        let (got, r) = self.prog(&o.body, &env, &Ks::new())?;
        if let Some(w) = want {
            self.u.unify(&w, &got, tail(&o.body))?;
        }
        Ok(r)
    }

    /// `Γ ⊢ p : T ! ρ`.
    fn prog(&mut self, e: &Expr, env: &Env, ks: &Ks) -> Res<(Ty, Row)> {
        let at = e.at;
        let none = Row::default();
        match &e.e {
            E::Return(x) => Ok((self.expr(x, &mut env.clone())?, none)),
            E::Bind(x, p, rest) => {
                let (t, r) = self.prog(p, env, ks)?;
                let mut env2 = env.clone();
                env2.insert(x.clone(), t);
                let (t2, r2) = self.prog(rest, &env2, ks)?;
                Ok((t2, r.union(&r2)))
            }
            E::Seq(a, b) => {
                let (ta, r) = self.prog(a, env, ks)?;
                if matches!(self.u.found(&ta), Ty::Prog(..)) {
                    return Err((a.at, "a program built and dropped: run it with `do`, or bind it to a name".into()));
                }
                let (t, r2) = self.prog(b, env, ks)?;
                Ok((t, r.union(&r2)))
            }
            E::Perform(p, xs) => {
                let n = p.to_string();
                if n == "Charge" {
                    return Err((at, "a program performs no Charge: drivers state the costs (10)".into()));
                }
                let (fields, result) = self.effect_sig(&n, false).ok_or((at, format!("{n} is no Effect")))?;
                self.args(&n, &fields, xs, &mut env.clone(), at)?;
                Ok((result, Row::one(&n)))
            }
            E::Apply(p, xs) => {
                let n = p.to_string();
                if let Some(a) = self.acts.get(&n) {
                    let fields: Vec<Ty> = a.fields.iter().map(|f| self.u.ty(&f.ty, &BTreeMap::new(), None)).collect();
                    let result = a.result.as_ref().map_or(con("Unit"), |t| self.u.ty(t, &BTreeMap::new(), None));
                    self.args(&n, &fields, xs, &mut env.clone(), at)?;
                    let r = self.row_of(&n, at);
                    return Ok((result, Row::one("Refused").union(&r)));
                }
                match self.u.syms.get(&n) {
                    Some(Sym::Fn { result: Type::Program(..), .. }) | None => Ok((self.expr(e, &mut env.clone())?, none)),
                    Some(Sym::Fn { .. }) => self.call(&n, xs, env, at),
                    _ => Ok((self.expr(e, &mut env.clone())?, none)),
                }
            }
            E::Do(x) => match self.expr(x, &mut env.clone()).map(|t| self.u.found(&t))? {
                Ty::Prog(t, r) => Ok((*t, r)),
                t => Err((at, format!("`do` of a {t}, which is no program"))),
            },
            E::Handle(body, ret, arms) => self.handle(body, ret, arms, env, ks),
            E::Resume(k, x) => {
                let (want, u) = ks.get(k).cloned().ok_or((at, format!("`resume {k}` inside a `par`, an `iterate` or another arm, where {k} cannot be resumed (7)")))?;
                let got = self.expr(x, &mut env.clone())?;
                self.u.unify(&want, &got, x.at)?;
                Ok((u, none))
            }
            E::Match(x, cases) => {
                let t = self.expr(x, &mut env.clone())?;
                let u = self.u.meta();
                let mut row = none;
                for c in cases {
                    let mut env2 = env.clone();
                    self.pat(&c.pat, &t, &mut env2, at)?;
                    if let Some(w) = &c.cond {
                        self.whole(w)?;
                        self.cond(w, &mut env2.clone())?;
                    }
                    let (ti, ri) = self.prog(&c.body, &env2, ks)?;
                    self.u.unify(&u, &ti, tail(&c.body))?;
                    row = row.union(&ri);
                }
                Ok((u, row))
            }
            E::If(c, a, b) => {
                if reads_facts(self.u.syms, c) {
                    self.whole(c)?;
                }
                self.cond(c, &mut env.clone())?;
                let (ta, ra) = self.prog(a, env, ks)?;
                match b {
                    Some(b) => {
                        // the type of the branches that are not `Never`
                        let (tb, rb) = self.prog(b, env, ks)?;
                        let u = self.u.meta();
                        self.u.unify(&u, &ta, a.at)?;
                        self.u.unify(&u, &tb, tail(b))?;
                        Ok((u, ra.union(&rb)))
                    }
                    None => {
                        self.u.unify(&con("Unit"), &ta, a.at).map_err(|(at, why)| (at, format!("an `if` without `else` gives (): {why}")))?;
                        Ok((ta, ra))
                    }
                }
            }
            E::Par(xs) => {
                let mut ts = Vec::new();
                let mut row = none;
                for x in xs {
                    let (t, r) = self.prog(x, env, &Ks::new())?;
                    ts.push(t);
                    row = row.union(&r);
                }
                Ok((Ty::Tuple(ts), row))
            }
            E::ParEach(x, l, _, b) => {
                let a = self.u.meta();
                let got = self.expr(l, &mut env.clone())?;
                self.u.unify(&con1("List", a.clone()), &got, l.at)?;
                let mut env2 = env.clone();
                env2.insert(x.clone(), a);
                let (t, r) = self.prog(b, &env2, &Ks::new())?;
                Ok((con1("List", t), r))
            }
            E::Iterate(x, init, n, b) => {
                let s = self.expr(init, &mut env.clone())?;
                let mut env2 = env.clone();
                env2.insert(x.clone(), s.clone());
                let (t, r) = self.prog(b, &env2, &Ks::new())?;
                let out = self.u.meta();
                self.u.unify(&Ty::Con("Loop".into(), vec![s, out.clone()]), &t, b.at)?;
                Ok((out, if n.is_some() { r.union(&Row::one("Exhausted")) } else { r }))
            }
            E::Gap => {
                let t = self.u.meta();
                let site = format!("{}:{at}", self.module);
                if let Some(top) = self.open.last_mut() {
                    top.push((site, t.clone()));
                }
                Ok((t, Row::one("Gap").union(&Row::one("Unfit"))))
            }
            _ => Ok((self.expr(e, &mut env.clone())?, none)),
        }
    }

    fn handle(&mut self, body: &Expr, ret: &Option<(String, Box<Expr>)>, arms: &[Arm], env: &Env, ks: &Ks) -> Res<(Ty, Row)> {
        let (t, r) = self.prog(body, env, ks)?;
        let (u, mut row) = match ret {
            Some((x, pr)) => {
                let mut env2 = env.clone();
                env2.insert(x.clone(), t);
                self.prog(pr, &env2, ks)?
            }
            None => (t, Row::default()),
        };
        let mut total = BTreeSet::new();
        for a in arms {
            let n = a.effect.to_string();
            let (fields, result) = self.effect_sig(&n, true).ok_or((a.at, format!("a handler arm names {n}, which is no Effect (14, check 8)")))?;
            if fields.len() != a.pats.len() {
                return Err((a.at, format!("{n} has {} fields, and {} patterns are written", fields.len(), a.pats.len())));
            }
            let mut env2 = env.clone();
            for (p, ft) in a.pats.iter().zip(&fields) {
                self.pat(p, ft, &mut env2, a.at)?;
            }
            match &a.cond {
                Some(c) => {
                    self.whole(c)?;
                    self.cond(c, &mut env2)?
                }
                None => {
                    total.insert(n.clone());
                }
            }
            if resumes(&a.body, &a.k) > 1 {
                return Err((a.at, format!("`resume {}` twice on one path through the arm for {n} (7)", a.k)));
            }
            let (ui, ri) = self.prog(&a.body, &env2, &[(a.k.clone(), (result, u.clone()))].into())?;
            self.u.unify(&u, &ui, tail(&a.body))?;
            row = row.union(&ri);
        }
        Ok((u, r.minus(&total).union(&row)))
    }

    /// A call of a `fn` whose result is no program: its result, and its body's row with the arguments' rows put
    /// for its row variable.
    fn call(&mut self, n: &str, xs: &[Expr], env: &Env, at: At) -> Res<(Ty, Row)> {
        let (fields, result, own) = self.fn_sig(n);
        self.args(n, &fields, xs, &mut env.clone(), at)?;
        Ok((result, self.callee_row(n, own, at)))
    }

    /// The body row of `fn` `n` at a call, its row variable given as the arguments fixed `own`.
    pub(super) fn callee_row(&mut self, n: &str, own: usize, at: At) -> Row {
        let row = self.row_of(n, at);
        match self.rows.get(n) {
            Some((_, v)) => row.subst(*v, &self.u.rows[own].clone()),
            None => row,
        }
    }

    /// A `fn`'s fields and result, its type parameters new types to find and its row a new variable (returned).
    pub(super) fn fn_sig(&mut self, n: &str) -> (Vec<Ty>, Ty, usize) {
        let Some(Sym::Fn { params, fields, result, .. }) = self.u.syms.get(n) else { unreachable!("{n} is a fn") };
        let ps = self.u.fresh(params);
        let v = self.u.row_var();
        (fields.iter().map(|f| self.u.ty(&f.ty, &ps, Some(v))).collect(), self.u.ty(result, &ps, Some(v)), v)
    }

    /// An Effect's fields and result; its type parameters kept abstract for an arm, new types to find for a
    /// `perform`.
    fn effect_sig(&mut self, n: &str, arm: bool) -> Option<(Vec<Ty>, Ty)> {
        match self.u.syms.get(n) {
            Some(Sym::Effect { params, fields, result, .. }) => {
                let ps: BTreeMap<String, Ty> = if arm { params.iter().map(|p| (p.clone(), self.u.rigid(p))).collect() } else { self.u.fresh(params) };
                let v = self.u.row_var();
                Some((fields.iter().map(|f| self.u.ty(&f.ty, &ps, Some(v))).collect(), self.u.ty(result, &ps, Some(v))))
            }
            Some(Sym::Builtin) if matches!(n, "Charge" | "Now" | "Refused" | "Unfit" | "Exhausted" | "Fault") => builtin(&mut self.u, n),
            _ => None,
        }
    }
}

/// How often `resume k` can come on one path through `e`.
fn resumes(e: &Expr, k: &str) -> usize {
    match &e.e {
        E::Resume(j, x) => usize::from(j == k) + resumes(x, k),
        E::Seq(a, b) | E::Bind(_, a, b) => resumes(a, k) + resumes(b, k),
        E::If(_, a, b) => resumes(a, k).max(b.as_ref().map_or(0, |b| resumes(b, k))),
        E::Match(_, cs) => cs.iter().map(|c| resumes(&c.body, k)).max().unwrap_or(0),
        E::Handle(b, r, _) => resumes(b, k) + r.as_ref().map_or(0, |(_, r)| resumes(r, k)),
        E::Return(x) | E::Do(x) => resumes(x, k),
        _ => 0,
    }
}

/// Where a block gives its type: its last expression, past each `;` and bind.
fn tail(e: &Expr) -> At {
    match &e.e {
        E::Seq(_, b) | E::Bind(_, _, b) => tail(b),
        _ => e.at,
    }
}
