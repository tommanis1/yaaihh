//! Resolving names inside declarations and later texts (spec 5.2, 5.3; load check 2): each a variable or one declaration.

use super::names::{LoadError, Loaded, Sym, qualify};
use crate::syntax::lex::At;
use crate::syntax::tree::*;
use std::collections::{BTreeMap, BTreeSet};

type Res<T> = Result<T, (At, String)>;
type Vars = BTreeSet<String>;

/// Resolve the names of `e`, a text given after loading (a program to run, a gap's filling), in module `module`'s
/// scope, with `vars` bound.
pub fn resolve(l: &Loaded, module: &str, vars: &[String], e: &mut Expr) -> Result<(), LoadError> {
    let none = Vec::new();
    let sc = Scope { module, imports: l.imports.get(module).unwrap_or(&none), syms: &l.syms };
    sc.expr(e, &vars.iter().cloned().collect(), false).map_err(|(at, why)| LoadError { module: module.to_string(), at, why })
}

/// One module's view of the declarations: its own, its imports', the language's.
pub struct Scope<'a> {
    pub module: &'a str,
    pub imports: &'a [Import],
    pub syms: &'a BTreeMap<String, Sym>,
}

impl Scope<'_> {
    /// The qualified name `p` stands for in this module, if any: its own, an import's (by alias or by path), or the
    /// language's.
    fn find(&self, p: &Path) -> Option<String> {
        let parts = &p.0;
        if parts.len() == 1 {
            let own = qualify(self.module, &parts[0]);
            if self.syms.contains_key(&own) {
                return Some(own);
            }
            return self.syms.contains_key(&parts[0]).then(|| parts[0].clone());
        }
        let (head, last) = (parts[..parts.len() - 1].join("."), parts.last().unwrap());
        let module = self.imports.iter().find(|i| i.alias.as_deref() == Some(head.as_str()) || (i.alias.is_none() && i.path.to_string() == head)).map(|i| i.path.to_string()).or_else(|| (head == self.module).then(|| head.clone()))?;
        let q = qualify(&module, last);
        self.syms.contains_key(&q).then_some(q)
    }

    pub fn must(&self, p: &mut Path, at: At) -> Res<()> {
        match self.find(p) {
            Some(q) => {
                *p = Path(vec![q]);
                Ok(())
            }
            None => Err((at, format!("{p} stands for no declaration"))),
        }
    }

    pub fn decl(&self, d: &mut Decl) -> Res<()> {
        let at = d.at;
        let names = |fs: &[Field]| fs.iter().map(|f| f.name.clone()).collect::<Vars>();
        match &mut d.what {
            DeclKind::Norm { fields, clauses, .. } => {
                let vars = match fields {
                    Fields::By(t) => {
                        self.ty(t, at, &[])?;
                        Vars::new()
                    }
                    Fields::Named(fs) => {
                        self.fields(fs, at, &[])?;
                        names(fs)
                    }
                };
                self.clauses(clauses, at, &vars)
            }
            DeclKind::Extend { name, clauses, .. } => {
                self.must(name, at)?;
                let vars = match self.syms.get(&name.0[0]) {
                    Some(Sym::Norm { fields: Fields::Named(fs), .. }) => names(fs),
                    Some(Sym::Norm { .. }) => Vars::new(),
                    _ => return Err((at, format!("{name} is no Fact, Act, Event or Duty to extend"))),
                };
                self.clauses(clauses, at, &vars)
            }
            DeclKind::Holds(e) => self.expr(e, &Vars::new(), false),
            DeclKind::Effect { params, fields, result, does, .. } => {
                self.fields(fields, at, params)?;
                self.ty(result, at, params)?;
                let vars = names(fields);
                does.as_mut().map_or(Ok(()), |b| self.expr(b, &vars, false))
            }
            DeclKind::Call { fields, result, template, .. } => {
                self.fields(fields, at, &[])?;
                self.ty(result, at, &[])?;
                self.pieces(template, &names(fields), false)
            }
            DeclKind::Sum { params, variants, .. } => variants.iter_mut().try_for_each(|v| self.fields(&mut v.fields, at, params)),
            DeclKind::Record { params, fields, .. } => self.fields(fields, at, params),
            DeclKind::Fn { params, fields, result, body, .. } => {
                self.fields(fields, at, params)?;
                self.ty(result, at, params)?;
                self.expr(body, &names(fields), false)
            }
            DeclKind::On { target, pats, cond, body } => {
                self.must(target, at)?;
                let mut vars = Vars::new();
                for p in pats.iter_mut() {
                    self.pat(p, at, &mut vars)?;
                }
                self.condition(cond.as_mut(), &mut vars)?;
                self.expr(body, &vars, false)
            }
        }
    }

    /// A `where`: its small-letter names that stand for nothing are bound by enumeration, and bound after it.
    fn condition(&self, c: Option<&mut Expr>, vars: &mut Vars) -> Res<()> {
        if let Some(c) = c {
            self.expr(c, vars, true)?;
            free_names(c, vars);
        }
        Ok(())
    }

    fn clauses(&self, cs: &mut [Clause], at: At, vars: &Vars) -> Res<()> {
        for c in cs {
            match c {
                Clause::HoldsWhen(e) | Clause::ViolatedWhen(e) | Clause::ConditionedBy(e) | Clause::DerivedFrom(e) | Clause::Prefer(e) => self.expr(e, vars, true)?,
                Clause::Does(e) => self.expr(e, vars, false)?,
                Clause::Creates(l) | Clause::Terminates(l) | Clause::Obfuscates(l) | Clause::SyncsWith(l) => {
                    for x in l {
                        let mut v = vars.clone();
                        if let Some(w) = &mut x.when {
                            self.expr(w, &v, true)?;
                            free_names(w, &mut v);
                        }
                        self.expr(&mut x.inst, &v, true)?;
                    }
                }
                Clause::Returns(t) => self.ty(t, at, &[])?,
            }
        }
        Ok(())
    }

    fn fields(&self, fs: &mut [Field], at: At, params: &[String]) -> Res<()> {
        fs.iter_mut().try_for_each(|f| self.ty(&mut f.ty, at, params))
    }

    fn ty(&self, t: &mut Type, at: At, params: &[String]) -> Res<()> {
        match t {
            Type::Named(p, args) => {
                if !(p.0.len() == 1 && params.contains(&p.0[0])) {
                    self.must(p, at)?;
                }
                args.iter_mut().try_for_each(|a| self.ty(a, at, params))
            }
            Type::Tuple(ts) => ts.iter_mut().try_for_each(|a| self.ty(a, at, params)),
            Type::Program(t, row) => {
                self.ty(t, at, params)?;
                row.effects.iter_mut().try_for_each(|e| self.must(e, at))
            }
        }
    }

    fn pieces(&self, ps: &mut [Piece], vars: &Vars, en: bool) -> Res<()> {
        ps.iter_mut().try_for_each(|p| match p {
            Piece::Hole(e) => self.expr(e, vars, en),
            Piece::Words(_) => Ok(()),
        })
    }

    fn pat(&self, p: &mut Pat, at: At, vars: &mut Vars) -> Res<()> {
        match p {
            Pat::Bind(n) => {
                vars.insert(n.clone());
                Ok(())
            }
            Pat::Ctor(path, ps) => {
                self.must(path, at)?;
                ps.iter_mut().try_for_each(|q| self.pat(q, at, vars))
            }
            Pat::Tuple(ps) => ps.iter_mut().try_for_each(|q| self.pat(q, at, vars)),
            Pat::List(ps, rest) => {
                if let Some(Some(n)) = rest {
                    vars.insert(n.clone());
                }
                ps.iter_mut().try_for_each(|q| self.pat(q, at, vars))
            }
            Pat::Any | Pat::Lit(_) | Pat::Text(_) => Ok(()),
        }
    }

    /// Resolve every name in `e`. A name bound in `vars` is a variable; `a.b` with `a` a variable is a field; with
    /// `en` (a clause or a `where`) a small-letter name that stands for nothing is bound by enumeration (5.3); any
    /// other name that stands for nothing is refused.
    fn expr(&self, e: &mut Expr, vars: &Vars, en: bool) -> Res<()> {
        let at = e.at;
        let with = |more: &[String]| {
            let mut v = vars.clone();
            v.extend(more.iter().cloned());
            v
        };
        match &mut e.e {
            E::Name(p) => {
                let small = p.0.len() == 1 && p.0[0].starts_with(|c: char| c.is_ascii_lowercase());
                if vars.contains(&p.0[0]) || (small && en && self.find(p).is_none()) {
                    let mut x = Expr { at, e: E::Var(p.0[0].clone()) };
                    for f in &p.0[1..] {
                        x = Expr { at, e: E::Field(Box::new(x), f.clone()) };
                    }
                    *e = x;
                    return Ok(());
                }
                match self.find(p) {
                    Some(q) => {
                        *p = Path(vec![q]);
                        Ok(())
                    }
                    None => Err((at, format!("{p} stands for no declaration and no variable"))),
                }
            }
            E::Var(_) => Ok(()),
            E::Apply(p, args) | E::Perform(p, args) => {
                self.must(p, at)?;
                args.iter_mut().try_for_each(|a| self.expr(a, vars, en))
            }
            E::Text(ps) => self.pieces(ps, vars, en),
            E::Field(x, _) | E::Neg(x) | E::Not(x) | E::Holds(x) | E::Violated(x) | E::Enabled(x) | E::Agg(_, x) | E::Return(x) | E::Do(x) => self.expr(x, vars, en),
            E::Bin(_, a, b) | E::Seq(a, b) => {
                self.expr(a, vars, en)?;
                self.expr(b, vars, en)
            }
            E::Quant(_, vs, body) => self.expr(body, &with(vs), en),
            E::Foreach(vs, v, c) => {
                let inner = with(vs);
                self.expr(v, &inner, en)?;
                c.as_mut().map_or(Ok(()), |c| self.expr(c, &inner, en))
            }
            E::Tuple(xs) | E::List(xs) | E::Par(xs) => xs.iter_mut().try_for_each(|a| self.expr(a, vars, en)),
            E::Bind(x, p, rest) => {
                self.expr(p, vars, en)?;
                self.expr(rest, &with(std::slice::from_ref(x)), en)
            }
            E::Handle(body, ret, arms) => {
                self.expr(body, vars, en)?;
                if let Some((x, r)) = ret {
                    self.expr(r, &with(std::slice::from_ref(x)), en)?;
                }
                for a in arms.iter_mut() {
                    self.must(&mut a.effect, a.at)?;
                    let mut v = vars.clone();
                    for p in a.pats.iter_mut() {
                        self.pat(p, a.at, &mut v)?;
                    }
                    v.insert(a.k.clone());
                    self.condition(a.cond.as_mut(), &mut v)?;
                    self.expr(&mut a.body, &v, en)?;
                }
                Ok(())
            }
            E::Resume(k, x) => {
                if !vars.contains(k) {
                    return Err((at, format!("`resume {k}`: {k} is no arm's continuation here")));
                }
                self.expr(x, vars, en)
            }
            E::Match(on, cases) => {
                self.expr(on, vars, en)?;
                for c in cases.iter_mut() {
                    let mut v = vars.clone();
                    self.pat(&mut c.pat, at, &mut v)?;
                    self.condition(c.cond.as_mut(), &mut v)?;
                    self.expr(&mut c.body, &v, en)?;
                }
                Ok(())
            }
            E::If(c, a, b) => {
                self.expr(c, vars, en)?;
                self.expr(a, vars, en)?;
                b.as_mut().map_or(Ok(()), |b| self.expr(b, vars, en))
            }
            E::ParEach(x, l, _, body) | E::Iterate(x, l, _, body) => {
                self.expr(l, vars, en)?;
                self.expr(body, &with(std::slice::from_ref(x)), en)
            }
            E::Lit(_) | E::SelfParty | E::This | E::Gap => Ok(()),
        }
    }
}

/// The variables of a condition that it binds: each `Var` standing in it outside a quantifier.
fn free_names(e: &Expr, out: &mut Vars) {
    match &e.e {
        E::Var(x) => {
            out.insert(x.clone());
        }
        E::Apply(_, xs) | E::Tuple(xs) | E::List(xs) => xs.iter().for_each(|x| free_names(x, out)),
        E::Bin(_, a, b) => {
            free_names(a, out);
            free_names(b, out);
        }
        E::Not(x) | E::Holds(x) | E::Violated(x) | E::Enabled(x) | E::Neg(x) | E::Field(x, _) => free_names(x, out),
        _ => {}
    }
}
