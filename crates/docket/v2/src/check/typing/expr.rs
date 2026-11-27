//! `Γ ⊢ e : T` (spec 7, 5.2): expressions, which perform nothing, conditions, patterns, and arguments.

use super::*;

impl Typer<'_> {
    /// The type of expression `e`. A variable not yet bound is one a condition binds by enumeration (5.3).
    pub(super) fn expr(&mut self, e: &Expr, env: &mut Env) -> Res<Ty> {
        let at = e.at;
        Ok(match &e.e {
            E::Lit(l) => lit(l),
            E::Text(ps) => {
                for p in ps {
                    if let Piece::Hole(h) = p {
                        self.expr(h, env)?;
                    }
                }
                con("String")
            }
            E::Var(x) => match env.get(x) {
                Some(t) => t.clone(),
                None => {
                    let t = self.u.meta();
                    env.insert(x.clone(), t.clone());
                    t
                }
            },
            E::Name(p) => self.apply(&p.to_string(), &[], env, at)?,
            E::Apply(p, xs) => self.apply(&p.to_string(), xs, env, at)?,
            E::Field(x, f) => {
                let t = self.expr(x, env)?;
                self.field(&t, f).ok_or_else(|| (at, format!("{} has no field {f}", self.u.found(&t))))?
            }
            E::Bin(Op::And | Op::Or, ..) | E::Not(_) => {
                self.cond(e, env)?;
                con("Bool")
            }
            E::Bin(op, a, b) => {
                let (ta, tb) = (self.expr(a, env)?, self.expr(b, env)?);
                if matches!(op, Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Rem) {
                    self.u.unify(&con("Int"), &ta, a.at)?;
                    self.u.unify(&con("Int"), &tb, b.at)?;
                    con("Int")
                } else {
                    self.u.unify(&ta, &tb, at)?;
                    con("Bool")
                }
            }
            E::Neg(x) => {
                let t = self.expr(x, env)?;
                self.u.unify(&con("Int"), &t, at)?;
                con("Int")
            }
            E::Holds(x) | E::Violated(x) | E::Enabled(x) => {
                self.whole(e)?;
                match &x.e {
                    E::Apply(p, xs) if matches!(self.u.syms.get(&p.to_string()), Some(Sym::Norm { .. })) => {
                        self.instance(&p.to_string(), xs, env, at)?;
                    }
                    E::Name(p) if matches!(self.u.syms.get(&p.to_string()), Some(Sym::Norm { .. })) => {
                        self.instance(&p.to_string(), &[], env, at)?;
                    }
                    _ => return Err((at, "`Holds`, `Violated` and `Enabled` take an instance".into())),
                }
                con("Bool")
            }
            E::Quant(_, vs, b) => {
                self.whole(e)?;
                let mut inner = env.clone();
                for v in vs {
                    let m = self.u.meta();
                    inner.insert(v.clone(), m);
                }
                self.cond(b, &mut inner)?;
                con("Bool")
            }
            E::Foreach(vs, v, c) => {
                self.whole(e)?;
                let mut inner = env.clone();
                for x in vs {
                    let m = self.u.meta();
                    inner.insert(x.clone(), m);
                }
                if let Some(c) = c {
                    self.cond(c, &mut inner)?;
                }
                con1("List", self.expr(v, &mut inner)?)
            }
            E::Agg(a, x) => {
                let t = self.expr(x, env)?;
                let elem = if *a == Agg::Count { self.u.meta() } else { con("Int") };
                self.u.unify(&con1("List", elem), &t, at)?;
                con("Int")
            }
            E::Tuple(xs) => Ty::Tuple(xs.iter().map(|x| self.expr(x, env)).collect::<Res<_>>()?),
            E::List(xs) => {
                let a = self.u.meta();
                for x in xs {
                    let t = self.expr(x, env)?;
                    self.u.unify(&a, &t, x.at)?;
                }
                con1("List", a)
            }
            E::SelfParty | E::This => self.u.meta(),
            _ => return Err((at, "a program where a value is wanted: a program is given to a `fn` or run with `do`".into())),
        })
    }

    /// `Ident(args)` as a value (5.2).
    fn apply(&mut self, n: &str, xs: &[Expr], env: &mut Env, at: At) -> Res<Ty> {
        match self.u.syms.get(n) {
            Some(Sym::Ctor { ty, fields, .. }) => {
                let (ty, fields) = (ty.clone(), fields.clone());
                let ps = match self.u.syms.get(&ty) {
                    Some(Sym::Sum { params, .. }) => params.clone(),
                    _ => Vec::new(),
                };
                let (map, row) = (self.u.fresh(&ps), self.u.row_var());
                let fs: Vec<Ty> = fields.iter().map(|f| self.u.ty(&f.ty, &map, Some(row))).collect();
                self.args(n, &fs, xs, env, at)?;
                Ok(Ty::Con(ty, ps.iter().map(|p| map[p].clone()).collect()))
            }
            Some(Sym::Record { params, fields }) => {
                let (ps, fields) = (params.clone(), fields.clone());
                let (map, row) = (self.u.fresh(&ps), self.u.row_var());
                let fs: Vec<Ty> = fields.iter().map(|f| self.u.ty(&f.ty, &map, Some(row))).collect();
                self.args(n, &fs, xs, env, at)?;
                Ok(Ty::Con(n.to_string(), ps.iter().map(|p| map[p].clone()).collect()))
            }
            Some(Sym::Norm { kind: Norm::Fact | Norm::Duty, .. }) => self.instance(n, xs, env, at),
            Some(Sym::Norm { .. }) => Err((at, format!("{n} is an Act or Event: performing it is a statement of a program, not a value"))),
            Some(Sym::Effect { .. }) => Err((at, format!("{n} is an Effect: it is performed with `perform`"))),
            Some(Sym::Call { fields, result, .. }) => {
                let (fields, result) = (fields.clone(), result.clone());
                let fs: Vec<Ty> = fields.iter().map(|f| self.u.ty(&f.ty, &BTreeMap::new(), None)).collect();
                self.args(n, &fs, xs, env, at)?;
                Ok(con1("Call", self.u.ty(&result, &BTreeMap::new(), None)))
            }
            Some(Sym::Fn { result: Type::Program(..), .. }) => {
                // a program built and not run (8.5): its row is its body's, the arguments' rows put in
                let (fs, result, own) = self.fn_sig(n);
                self.args(n, &fs, xs, env, at)?;
                let row = self.callee_row(n, own, at);
                let Ty::Prog(t, declared) = result else { unreachable!("a fn whose result is a program") };
                Ok(Ty::Prog(t, if declared.rest.is_some() { row } else { declared }))
            }
            Some(Sym::Fn { .. }) => {
                let (t, r) = self.call(n, xs, env, at)?;
                if !r.effs.is_empty() || r.rest.is_some() {
                    return Err((at, format!("{n} may perform {}: a call of it is a statement of a program, not a value", Ty::Prog(Box::new(t), r))));
                }
                Ok(t)
            }
            Some(Sym::Builtin) if !matches!(n, "Charge" | "Now" | "Refused" | "Unfit" | "Exhausted" | "Fault") => {
                let (fs, result) = builtin(&mut self.u, n).ok_or((at, format!("{n} is a type, not a value")))?;
                self.args(n, &fs, xs, env, at)?;
                Ok(result)
            }
            _ => Err((at, format!("{n} has no value"))),
        }
    }

    /// An instance of fact, Act or Event `n` with arguments `xs`: its type.
    fn instance(&mut self, n: &str, xs: &[Expr], env: &mut Env, at: At) -> Res<Ty> {
        let fs: Vec<Ty> = match self.u.syms.get(n) {
            Some(Sym::Norm { fields: Fields::Named(fs), .. }) => fs.iter().map(|f| f.ty.clone()).collect::<Vec<_>>().iter().map(|t| self.u.ty(t, &BTreeMap::new(), None)).collect(),
            Some(Sym::Norm { fields: Fields::By(t), .. }) if !xs.is_empty() => vec![self.u.ty(&t.clone(), &BTreeMap::new(), None)],
            _ => Vec::new(),
        };
        self.args(n, &fs, xs, env, at)?;
        Ok(con(n))
    }

    /// The type of field `f` of a record or of an instance of type `t`.
    fn field(&mut self, t: &Ty, f: &str) -> Option<Ty> {
        let Ty::Con(n, args) = self.u.found(t) else { return None };
        match self.u.syms.get(&n) {
            Some(Sym::Record { params, fields }) => {
                let map: BTreeMap<String, Ty> = params.iter().cloned().zip(args).collect();
                let row = self.u.row_var();
                fields.iter().find(|x| x.name == f).map(|x| self.u.ty(&x.ty, &map, Some(row)))
            }
            Some(Sym::Norm { fields: Fields::Named(fs), .. }) => fs.iter().find(|x| x.name == f).map(|x| self.u.ty(&x.ty, &BTreeMap::new(), None)),
            _ => None,
        }
    }

    /// A condition (5.2): a fact as written holds or not; its unbound variables are bound by enumeration.
    /// A condition the normative core reads whole calls no `fn`, which the core cannot run.
    pub(super) fn whole(&self, c: &Expr) -> Res<()> {
        let go = |x: &Expr| self.whole(x);
        match &c.e {
            E::Apply(p, _) if matches!(self.u.syms.get(&p.to_string()), Some(Sym::Fn { .. })) => {
                Err((c.at, format!("{p} is called in a condition the normative core reads whole, and the core runs no `fn`: bind its value first")))
            }
            E::Apply(_, xs) | E::Tuple(xs) | E::List(xs) => xs.iter().try_for_each(go),
            E::Field(x, _) | E::Not(x) | E::Neg(x) | E::Holds(x) | E::Violated(x) | E::Enabled(x) | E::Agg(_, x) | E::Quant(_, _, x) => go(x),
            E::Bin(_, a, b) => go(a).and_then(|_| go(b)),
            E::Foreach(_, v, w) => go(v).and_then(|_| w.as_deref().map_or(Ok(()), go)),
            _ => Ok(()),
        }
    }

    pub(super) fn cond(&mut self, e: &Expr, env: &mut Env) -> Res<()> {
        match &e.e {
            E::Bin(Op::And | Op::Or, a, b) => {
                self.cond(a, env)?;
                self.cond(b, env)
            }
            E::Not(x) => self.cond(x, &mut env.clone()),
            E::Apply(p, xs) if matches!(self.u.syms.get(&p.to_string()), Some(Sym::Norm { kind: Norm::Fact | Norm::Duty, .. })) => self.instance(&p.to_string(), xs, env, e.at).map(|_| ()),
            E::Name(p) if matches!(self.u.syms.get(&p.to_string()), Some(Sym::Norm { kind: Norm::Fact | Norm::Duty, .. })) => Ok(()),
            _ => {
                let t = self.expr(e, env)?;
                self.u.unify(&con("Bool"), &t, e.at)
            }
        }
    }

    /// Pattern `p` against a value of type `t`: the variables it binds.
    pub(super) fn pat(&mut self, p: &Pat, t: &Ty, env: &mut Env, at: At) -> Res<()> {
        match p {
            Pat::Any => Ok(()),
            Pat::Bind(v) => {
                env.insert(v.clone(), t.clone());
                Ok(())
            }
            Pat::Lit(l) => self.u.unify(t, &lit(l), at),
            Pat::Text(_) => self.u.unify(t, &con("String"), at),
            Pat::Ctor(q, ps) => {
                let n = q.to_string();
                let (fs, whole) = match self.u.syms.get(&n) {
                    Some(Sym::Ctor { ty, fields, .. }) => {
                        let (ty, fields) = (ty.clone(), fields.clone());
                        let params = match self.u.syms.get(&ty) {
                            Some(Sym::Sum { params, .. }) => params.clone(),
                            _ => Vec::new(),
                        };
                        let (map, row) = (self.u.fresh(&params), self.u.row_var());
                        (fields.iter().map(|f| self.u.ty(&f.ty, &map, Some(row))).collect(), Ty::Con(ty, params.iter().map(|x| map[x].clone()).collect()))
                    }
                    Some(Sym::Norm { fields: Fields::Named(fs), .. }) => (fs.clone().iter().map(|f| self.u.ty(&f.ty, &BTreeMap::new(), None)).collect(), con(&n)),
                    Some(Sym::Builtin) => builtin(&mut self.u, &n).filter(|_| matches!(n.as_str(), "Some" | "None" | "Again" | "Done")).ok_or((at, format!("{n} is no constructor")))?,
                    _ => return Err((at, format!("{n} is no constructor"))),
                };
                self.u.unify(t, &whole, at)?;
                if ps.len() != fs.len() {
                    return Err((at, format!("{n} has {} fields, and {} patterns are written", fs.len(), ps.len())));
                }
                ps.iter().zip(&fs).try_for_each(|(p, f)| self.pat(p, f, env, at))
            }
            Pat::Tuple(ps) => {
                let ts: Vec<Ty> = ps.iter().map(|_| self.u.meta()).collect();
                self.u.unify(t, &Ty::Tuple(ts.clone()), at)?;
                ps.iter().zip(&ts).try_for_each(|(p, x)| self.pat(p, x, env, at))
            }
            Pat::List(ps, rest) => {
                let a = self.u.meta();
                self.u.unify(t, &con1("List", a.clone()), at)?;
                if let Some(Some(r)) = rest {
                    env.insert(r.clone(), con1("List", a.clone()));
                }
                ps.iter().try_for_each(|p| self.pat(p, &a, env, at))
            }
        }
    }

    /// Arguments `xs` for the fields `fs` of `n`. A field of program type takes a program written there as a
    /// value (8.5), or a value that is one.
    pub(super) fn args(&mut self, n: &str, fs: &[Ty], xs: &[Expr], env: &mut Env, at: At) -> Res<()> {
        if fs.len() != xs.len() {
            return Err((at, format!("{n} takes {} arguments, and {} are given", fs.len(), xs.len())));
        }
        for (want, x) in fs.iter().zip(xs) {
            let value = matches!(&x.e, E::Var(_) | E::Field(..)) || matches!(&x.e, E::Apply(p, _) | E::Name(p) if matches!(self.u.syms.get(&p.to_string()), Some(Sym::Fn { result: Type::Program(..), .. })));
            let got = if matches!(self.u.found(want), Ty::Prog(..)) && !value {
                let (t, r) = self.prog(x, env, &Ks::new())?;
                Ty::Prog(Box::new(t), r)
            } else {
                self.expr(x, env)?
            };
            self.u.unify(want, &got, x.at)?;
        }
        Ok(())
    }
}

fn lit(l: &Lit) -> Ty {
    con(match l {
        Lit::Int(_) => "Int",
        Lit::Bool(_) => "Bool",
        Lit::Unit => "Unit",
    })
}
