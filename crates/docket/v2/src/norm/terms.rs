//! A condition as eFLINT reads it: each variable the machine binds given as its value, each other variable typed by
//! the field it stands in (5.3).

use super::host;
use super::lower::mentions;
use super::shape::{BOOL, Shapes};
use crate::run::value::{Env, Value};
use crate::syntax::form;
use crate::syntax::lex::At;
use crate::syntax::tree::{Agg, E, Expr, Lit, Op, Piece, Quant};
use eflint::ast::{Args, BinOp, Id, Quant as Q, Term, UnOp, Var, id};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

type R<T> = Result<T, String>;

/// Quantified variables in scope, each with the place of its quantifier: two quantifiers may bind one name to
/// different types.
type Scope = Vec<(String, At)>;

fn binder(x: &str, scope: &Scope) -> Option<At> {
    scope.iter().rev().find(|(n, _)| n == x).map(|(_, at)| *at)
}

fn bx(t: Term) -> Box<Term> {
    Box::new(t)
}

/// What a condition's variables are: given by the machine, a declaration's fields, or found where they stand.
pub struct Lower<'a> {
    pub shapes: &'a Shapes,
    env: &'a Env,
    given: BTreeSet<String>,
    /// The eFLINT type of each variable no quantifier binds.
    vars: BTreeMap<String, Id>,
    /// The eFLINT type of each quantified variable, by its quantifier's place.
    scoped: BTreeMap<(At, String), Id>,
    /// The quantifiers `typed` is inside.
    scope: RefCell<Scope>,
    /// The variables neither given nor bound by a quantifier, in the order they first stand.
    pub free: Vec<String>,
    /// Each head field the rule being lowered sets equal to an expression, with that expression and its term (6.2).
    pub put: RefCell<BTreeMap<String, (Expr, Term)>>,
}

impl<'a> Lower<'a> {
    /// A condition's variables: ENV's bound by the machine, FIELDS a declaration's.
    pub fn new(shapes: &'a Shapes, env: &'a Env, fields: &[Var]) -> Lower<'a> {
        let vars = fields.iter().map(|v| (v.dec.to_string(), v.dom.clone())).collect();
        let given = fields.iter().map(|v| v.dec.to_string()).collect();
        Lower { shapes, env, given, vars, scoped: BTreeMap::new(), scope: RefCell::default(), free: Vec::new(), put: RefCell::default() }
    }

    /// E's variables typed where they stand, then E as a condition.
    pub fn cond(&mut self, e: &Expr) -> R<Term> {
        self.types(e)?;
        self.truth(e)
    }

    /// Type E's variables: an argument of a declared type or a host function by its field, one of arithmetic or an
    /// order as `Int`, one side of `==` by the other; again until no type is found, so a type found late reaches back.
    pub fn types(&mut self, e: &Expr) -> R<()> {
        loop {
            let known = self.vars.len() + self.scoped.len();
            self.find(e, &mut Vec::new())?;
            if self.vars.len() + self.scoped.len() == known {
                return Ok(());
            }
        }
    }

    /// The eFLINT types of what P's arguments stand in, if P is a declared type or a host function. The argument of
    /// a type identified by a primitive is of that type: its instance is the primitive (5.4).
    fn fields_of(&self, p: &str) -> Option<Vec<Option<Id>>> {
        match self.shapes.by.get(p) {
            Some(s) if s.by.is_none() => Some(s.fields.iter().map(|f| Some(f.dom.clone())).collect()),
            Some(_) => Some(vec![Some(id(p))]),
            None => host::arg_types(p).map(|ts| ts.into_iter().map(Some).collect()),
        }
    }

    fn find(&mut self, e: &Expr, bound: &mut Scope) -> R<()> {
        let var = |x: &Expr| match &x.e {
            E::Var(v) => Some(v.clone()),
            _ => None,
        };
        match &e.e {
            E::Var(x) => {
                if !self.env.contains_key(x) && !self.given.contains(x) && binder(x, bound).is_none() && !self.free.contains(x) {
                    self.free.push(x.clone());
                }
            }
            E::Apply(p, args) => {
                let tys = self.fields_of(&p.to_string()).unwrap_or_default();
                for (k, a) in args.iter().enumerate() {
                    if let (Some(x), Some(Some(t))) = (var(a), tys.get(k)) {
                        self.put(&x, t, bound)?;
                    }
                    self.find(a, bound)?;
                }
            }
            E::Bin(op, a, b) => {
                match op {
                    Op::And | Op::Or => {}
                    Op::Eq | Op::Ne => {
                        if let (Some(x), Some(y)) = (var(a), var(b)) {
                            for (to, from) in [(&x, &y), (&y, &x)] {
                                if let Some(t) = self.ty_of(from, bound) {
                                    self.put(to, &t, bound)?;
                                }
                            }
                        }
                    }
                    _ => {
                        for x in [var(a), var(b)].into_iter().flatten() {
                            self.put(&x, &id("int"), bound)?;
                        }
                    }
                }
                self.find(a, bound)?;
                self.find(b, bound)?;
            }
            E::Quant(_, xs, c) | E::Foreach(xs, _, Some(c)) => {
                bound.extend(xs.iter().map(|x| (x.clone(), e.at)));
                if let E::Foreach(_, v, _) = &e.e {
                    self.find(v, bound)?;
                }
                self.find(c, bound)?;
                bound.truncate(bound.len() - xs.len());
            }
            E::Not(x) | E::Neg(x) | E::Holds(x) | E::Violated(x) | E::Enabled(x) | E::Agg(_, x) | E::Field(x, _) => self.find(x, bound)?,
            _ => {}
        }
        Ok(())
    }

    fn put(&mut self, x: &str, t: &Id, bound: &Scope) -> R<()> {
        let at = binder(x, bound);
        if at.is_none() && (self.env.contains_key(x) || self.given.contains(x)) {
            return Ok(());
        }
        match self.ty_of(x, bound) {
            Some(was) if was != *t => Err(format!("{x} stands in a field of {was} and in one of {t}")),
            _ => {
                match at {
                    Some(at) => self.scoped.insert((at, x.to_string()), t.clone()),
                    None => self.vars.insert(x.to_string(), t.clone()),
                };
                Ok(())
            }
        }
    }

    /// The type found for X where SCOPE is in scope.
    fn ty_of(&self, x: &str, scope: &Scope) -> Option<Id> {
        match binder(x, scope) {
            Some(at) => self.scoped.get(&(at, x.to_string())).cloned(),
            None => self.vars.get(x).cloned(),
        }
    }

    /// Whether X is a variable of the condition here, not one the machine gave a value.
    fn is_var(&self, x: &str) -> bool {
        binder(x, &self.scope.borrow()).is_some() || !self.env.contains_key(x)
    }

    /// The type of a variable not bound by the machine, if found.
    fn var_ty(&self, e: &Expr) -> Option<Id> {
        match &e.e {
            E::Var(x) if self.is_var(x) => self.ty_of(x, &self.scope.borrow()),
            _ => None,
        }
    }

    /// E with the variables XS of the quantifier at AT in scope.
    pub fn within<T>(&self, at: At, xs: &[String], e: impl FnOnce() -> R<T>) -> R<T> {
        let n = xs.len();
        self.scope.borrow_mut().extend(xs.iter().map(|x| (x.clone(), at)));
        let got = e();
        let mut scope = self.scope.borrow_mut();
        let keep = scope.len() - n;
        scope.truncate(keep);
        got
    }

    /// Variables XS as eFLINT's, each of its type.
    pub fn bound(&self, xs: &[String]) -> R<Vec<Var>> {
        xs.iter()
            .map(|x| match self.ty_of(x, &self.scope.borrow()) {
                Some(t) => Ok(Var { dom: t.clone(), dec: id(x) }),
                None => Err(format!("{x} stands in no field, so it has no type (5.3)")),
            })
            .collect()
    }

    /// E where a truth value is wanted: a `Bool` field's variable holds when its text is `true`.
    pub fn truth(&self, e: &Expr) -> R<Term> {
        Ok(match &e.e {
            E::Bin(Op::And, a, b) => Term::Bin(BinOp::And, bx(self.truth(a)?), bx(self.truth(b)?)),
            E::Bin(Op::Or, a, b) => Term::Bin(BinOp::Or, bx(self.truth(a)?), bx(self.truth(b)?)),
            E::Not(x) => Term::Un(UnOp::Not, bx(self.truth(x)?)),
            _ if self.var_ty(e).is_some_and(|t| &*t == BOOL) => Term::Bin(BinOp::Eq, bx(self.term(e)?), bx(Term::StringLit(id("true")))),
            _ => self.term(e)?,
        })
    }

    pub fn term(&self, e: &Expr) -> R<Term> {
        self.typed(e, None)
    }

    /// E as a term; WANT, where known, is the eFLINT type of what it stands in, so a truth value of a `Bool` field is
    /// its text.
    pub fn typed(&self, e: &Expr, want: Option<&Id>) -> R<Term> {
        let text_of_bool = want.is_some_and(|w| &**w == BOOL);
        Ok(match &e.e {
            E::Lit(Lit::Int(n)) => Term::IntLit(*n),
            E::Lit(Lit::Bool(b)) if text_of_bool => Term::StringLit(id(&b.to_string())),
            E::Lit(Lit::Bool(b)) => Term::BoolLit(*b),
            E::Text(ps) => match ps.as_slice() {
                [] => Term::StringLit(id("")),
                [Piece::Words(w)] => Term::StringLit(id(w)),
                _ => return Err(format!("{}: a text with holes is no condition's", form::expr(e))),
            },
            E::Name(p) => self.apply(&p.to_string(), &[])?,
            E::Var(x) if binder(x, &self.scope.borrow()).is_none() && self.put.borrow().contains_key(x) => self.put_term(x)?,
            E::Var(x) => match self.env.get(x) {
                Some(v) if !self.is_var(x) => self.value(v, want)?,
                _ => Term::Ref(self.bound(std::slice::from_ref(x))?.remove(0)),
            },
            E::Apply(p, args) => self.apply(&p.to_string(), args)?,
            E::Field(x, f) => {
                let ty = match (&x.e, self.var_ty(x)) {
                    (_, Some(t)) => t.to_string(),
                    (E::Var(v), None) => match self.env.get(v) {
                        Some(Value::Inst(i)) => i.ty.clone(),
                        _ => String::new(),
                    },
                    _ => String::new(),
                };
                let field = self.shapes.by.get(&ty).and_then(|s| s.fields.iter().find(|v| &*v.dec == f));
                match field {
                    Some(v) => Term::Project(bx(self.term(x)?), v.clone()),
                    None => return Err(format!("{}: no field {f} is known here", form::expr(e))),
                }
            }
            E::Bin(Op::And | Op::Or, ..) | E::Not(_) => self.truth(e)?,
            E::Bin(op, a, b) => {
                let o = match op {
                    Op::Eq => BinOp::Eq,
                    Op::Ne => BinOp::Neq,
                    Op::Lt => BinOp::Le,
                    Op::Le => BinOp::Leq,
                    Op::Gt => BinOp::Ge,
                    Op::Ge => BinOp::Geq,
                    Op::Add => BinOp::Add,
                    Op::Sub => BinOp::Sub,
                    Op::Mul => BinOp::Mult,
                    Op::Div => BinOp::Div,
                    Op::Rem => BinOp::Mod,
                    Op::And | Op::Or => return self.truth(e),
                };
                let (ta, tb) = (self.var_ty(a), self.var_ty(b));
                Term::Bin(o, bx(self.typed(a, tb.as_ref())?), bx(self.typed(b, ta.as_ref())?))
            }
            E::Neg(x) => Term::Bin(BinOp::Sub, bx(Term::IntLit(0)), bx(self.term(x)?)),
            E::Holds(x) => Term::Un(UnOp::Present, bx(self.term(x)?)),
            E::Violated(x) => Term::Un(UnOp::Violated, bx(self.term(x)?)),
            E::Enabled(x) => Term::Un(UnOp::Enabled, bx(self.term(x)?)),
            E::Quant(q, xs, c) => {
                let q = if *q == Quant::Exists { Q::Exists } else { Q::Forall };
                self.within(e.at, xs, || Ok(Term::Q(q, self.bound(xs)?, bx(self.truth(c)?))))?
            }
            E::Agg(a, x) => match &x.e {
                E::Foreach(xs, v, Some(c)) => {
                    let q = match a {
                        Agg::Count => Q::Count,
                        Agg::Sum => Q::Sum,
                        Agg::Max => Q::Max,
                        Agg::Min => Q::Min,
                    };
                    self.within(x.at, xs, || Ok(Term::Q(q, self.bound(xs)?, bx(Term::Bin(BinOp::When, bx(self.term(v)?), bx(self.truth(c)?))))))?
                }
                _ => return Err(format!("{}: `Count` and its kind take a `Foreach` with a `When` (6.2)", form::expr(e))),
            },
            _ => return Err(format!("{} is not read by the normative core", form::expr(e))),
        })
    }

    /// Head field X as the expression it is set equal to, refused under a quantifier that binds a variable of it.
    fn put_term(&self, x: &str) -> R<Term> {
        let put = self.put.borrow();
        let (e, t) = &put[x];
        match self.scope.borrow().iter().find(|(v, _)| mentions(e, v)) {
            Some((v, _)) => Err(format!("{x} stands for {} where a quantifier binds {v} again (6.2)", form::expr(e))),
            None => Ok(t.clone()),
        }
    }

    /// P applied to ARGS: an instance of a declared type, or a host function's answer.
    fn apply(&self, p: &str, args: &[Expr]) -> R<Term> {
        let Some(wants) = self.fields_of(p) else {
            return Err(format!("{p} is no Fact, Act, Event or Duty, and no function of the host table that a condition reads (6.3)"));
        };
        if wants.len() != args.len() {
            return Err(format!("{p} takes {} fields, {} given", wants.len(), args.len()));
        }
        let ts = args.iter().zip(&wants).map(|(a, w)| self.typed(a, w.as_ref())).collect::<R<Vec<_>>>()?;
        Ok(Term::App(id(p), Args::Pos(ts)))
    }

    /// A value the machine bound, as a literal term.
    fn value(&self, v: &Value, want: Option<&Id>) -> R<Term> {
        Ok(match v {
            Value::Int(n) => Term::IntLit(*n),
            Value::Text(s) => Term::StringLit(id(s)),
            Value::Bool(b) if want.is_some_and(|w| &**w == BOOL) => Term::StringLit(id(&b.to_string())),
            Value::Bool(b) => Term::BoolLit(*b),
            Value::Inst(i) => {
                let wants = self.fields_of(&i.ty).ok_or_else(|| format!("{} is no Fact, Act, Event or Duty", i.ty))?;
                let ts = i.args.iter().zip(wants.iter().chain(std::iter::repeat(&None))).map(|(a, w)| self.value(a, w.as_ref())).collect::<R<Vec<_>>>()?;
                Term::App(id(&i.ty), Args::Pos(ts))
            }
            other => return Err(format!("{other} is no value a condition reads (6.2)")),
        })
    }
}
