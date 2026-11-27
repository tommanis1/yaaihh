//! Types and rows inside the checker (spec 6.1, 7): their forms, unification, and a declared type read in scope.

use super::names::Sym;
use crate::syntax::lex::At;
use crate::syntax::tree::{Fields, Type};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub type Res<T> = Result<T, (At, String)>;

#[derive(Clone, Debug, PartialEq)]
pub enum Ty {
    /// A named type and its arguments: a primitive, `List`, `Option`, `Call`, `Loop`, a declared `type`, a fact type.
    Con(String, Vec<Ty>),
    Tuple(Vec<Ty>),
    Prog(Box<Ty>, Row),
    /// A type parameter of the declaration being checked, or of an arm's Effect: one type, kept abstract.
    Rigid(String),
    /// A type still to be found.
    Meta(usize),
}

/// A row (6.1): the Effects named, and maybe the row variable `rest.0` less the Effects `rest.1`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Row {
    pub effs: BTreeSet<String>,
    pub rest: Option<(usize, BTreeSet<String>)>,
}

impl Row {
    pub fn one(e: &str) -> Row {
        Row { effs: [e.to_string()].into(), rest: None }
    }
    pub fn union(mut self, o: &Row) -> Row {
        self.effs.extend(o.effs.iter().cloned());
        self.rest = match (self.rest, &o.rest) {
            (Some((v, a)), Some((w, b))) if v == *w => Some((v, a.intersection(b).cloned().collect())),
            (Some(r), _) => Some(r),
            (None, r) => r.clone(),
        };
        self
    }
    pub fn minus(mut self, h: &BTreeSet<String>) -> Row {
        self.effs.retain(|e| !h.contains(e));
        if let Some((_, m)) = &mut self.rest {
            m.extend(h.iter().cloned());
        }
        self
    }
    /// The row with variable `v` given as `by`.
    pub fn subst(self, v: usize, by: &Row) -> Row {
        match self.rest {
            Some((w, m)) if w == v => Row { effs: self.effs, rest: None }.union(&by.clone().minus(&m)),
            _ => self,
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let list = |f: &mut fmt::Formatter<'_>, ts: &[Ty]| ts.iter().enumerate().try_for_each(|(k, t)| write!(f, "{}{t}", if k > 0 { ", " } else { "" }));
        match self {
            Ty::Con(n, a) if a.is_empty() => write!(f, "{n}"),
            Ty::Con(n, a) => {
                write!(f, "{n}<")?;
                list(f, a)?;
                write!(f, ">")
            }
            Ty::Tuple(ts) => {
                write!(f, "(")?;
                list(f, ts)?;
                write!(f, ")")
            }
            Ty::Prog(t, r) => write!(f, "Program<{t} ! {{{}}}>", r.effs.iter().cloned().chain(r.rest.iter().map(|_| "..".to_string())).collect::<Vec<_>>().join(", ")),
            Ty::Rigid(n) => write!(f, "{n}"),
            Ty::Meta(_) => write!(f, "_"),
        }
    }
}

pub fn con(n: &str) -> Ty {
    Ty::Con(n.to_string(), Vec::new())
}

pub fn con1(n: &str, t: Ty) -> Ty {
    Ty::Con(n.to_string(), vec![t])
}

/// The types found so far and the rows given to callees' row variables, for one loaded text.
pub struct Unify<'a> {
    pub syms: &'a BTreeMap<String, Sym>,
    metas: Vec<Option<Ty>>,
    pub rows: Vec<Row>,
    rigid: usize,
}

impl<'a> Unify<'a> {
    pub fn new(syms: &'a BTreeMap<String, Sym>) -> Self {
        Unify { syms, metas: Vec::new(), rows: Vec::new(), rigid: 0 }
    }
    pub fn meta(&mut self) -> Ty {
        self.metas.push(None);
        Ty::Meta(self.metas.len() - 1)
    }
    pub fn row_var(&mut self) -> usize {
        self.rows.push(Row::default());
        self.rows.len() - 1
    }
    /// A fresh abstract type for parameter `x` of an arm's Effect.
    pub fn rigid(&mut self, x: &str) -> Ty {
        self.rigid += 1;
        Ty::Rigid(format!("{x}#{}", self.rigid))
    }

    /// `t` with each type found put in, all the way down.
    pub fn found(&self, t: &Ty) -> Ty {
        match t {
            Ty::Meta(i) => self.metas[*i].as_ref().map_or(t.clone(), |u| self.found(u)),
            Ty::Con(n, a) => Ty::Con(n.clone(), a.iter().map(|x| self.found(x)).collect()),
            Ty::Tuple(ts) => Ty::Tuple(ts.iter().map(|x| self.found(x)).collect()),
            Ty::Prog(x, r) => Ty::Prog(Box::new(self.found(x)), r.clone()),
            Ty::Rigid(_) => t.clone(),
        }
    }

    fn occurs(&self, i: usize, t: &Ty) -> bool {
        match self.found(t) {
            Ty::Meta(j) => i == j,
            Ty::Con(_, a) | Ty::Tuple(a) => a.iter().any(|x| self.occurs(i, x)),
            Ty::Prog(x, _) => self.occurs(i, &x),
            Ty::Rigid(_) => false,
        }
    }

    /// The primitive a fact type is identified by, through other fact types (`Fact Party Identified by String`).
    fn ident(&self, n: &str) -> Option<String> {
        match self.syms.get(n) {
            Some(Sym::Norm { fields: Fields::By(Type::Named(p, _)), .. }) => {
                let p = p.to_string();
                if matches!(p.as_str(), "String" | "Int" | "Bool") { Some(p) } else { self.ident(&p) }
            }
            _ => None,
        }
    }

    /// Make `want` and `got` one type. A row given where a callee's row variable stands is added to it; a closed
    /// row wanted holds every Effect of the one given.
    pub fn unify(&mut self, want: &Ty, got: &Ty, at: At) -> Res<()> {
        let (a, b) = (self.found(want), self.found(got));
        let no = |me: &Self| Err((at, format!("{} wanted, {} given", me.found(want), me.found(got))));
        match (&a, &b) {
            // `Never` given fits any type wanted and fixes none; a value given where `Never` is wanted is no fit
            (_, Ty::Con(n, _)) if n == "Never" => Ok(()),
            (Ty::Meta(i), Ty::Meta(j)) if i == j => Ok(()),
            (Ty::Meta(i), t) | (t, Ty::Meta(i)) => {
                if self.occurs(*i, t) {
                    return no(self);
                }
                self.metas[*i] = Some(t.clone());
                Ok(())
            }
            (Ty::Con(n, x), Ty::Con(m, y)) if n == m && x.len() == y.len() => x.iter().zip(y).try_for_each(|(p, q)| self.unify(p, q, at)),
            (Ty::Con(n, x), Ty::Con(m, y)) if x.is_empty() && y.is_empty() && (self.ident(n).as_ref() == Some(m) || self.ident(m).as_ref() == Some(n)) => Ok(()),
            (Ty::Tuple(x), Ty::Tuple(y)) if x.len() == y.len() => x.iter().zip(y).try_for_each(|(p, q)| self.unify(p, q, at)),
            (Ty::Prog(x, r), Ty::Prog(y, s)) => {
                self.unify(x, y, at)?;
                self.rows_fit(r, s, at)
            }
            (Ty::Rigid(x), Ty::Rigid(y)) if x == y => Ok(()),
            _ => no(self),
        }
    }

    fn rows_fit(&mut self, want: &Row, got: &Row, at: At) -> Res<()> {
        match &want.rest {
            Some((v, _)) => {
                let r = std::mem::take(self.rows.get_mut(*v).ok_or((at, format!("row variable {v} is not known here")))?).union(got);
                self.rows[*v] = r;
                Ok(())
            }
            None => match got.effs.iter().find(|e| !want.effs.contains(*e)) {
                Some(e) => Err((at, format!("a program that may perform {e}, where only {{{}}} may be", want.effs.iter().cloned().collect::<Vec<_>>().join(", ")))),
                None if got.rest.is_some() => Err((at, format!("a program that may perform more than {{{}}}", want.effs.iter().cloned().collect::<Vec<_>>().join(", ")))),
                None => Ok(()),
            },
        }
    }

    /// Declared type `t` in the checker's form: `params` give the type parameters, `row` the variable an open row
    /// stands for (6.1: one per declaration); with none, an open row is closed (a fact's fields hold no program).
    pub fn ty(&self, t: &Type, params: &BTreeMap<String, Ty>, row: Option<usize>) -> Ty {
        match t {
            Type::Named(p, args) => {
                let n = p.to_string();
                if let Some(x) = params.get(&n) {
                    return x.clone();
                }
                Ty::Con(n, args.iter().map(|a| self.ty(a, params, row)).collect())
            }
            Type::Tuple(ts) if ts.is_empty() => con("Unit"),
            Type::Tuple(ts) => Ty::Tuple(ts.iter().map(|a| self.ty(a, params, row)).collect()),
            Type::Program(x, r) => {
                let rest = row.filter(|_| r.rest.is_some() || r.implied).map(|v| (v, BTreeSet::new()));
                Ty::Prog(Box::new(self.ty(x, params, row)), Row { effs: r.effects.iter().map(|e| e.to_string()).collect(), rest })
            }
        }
    }

    /// `t` with each type still to be found given a new one here: a type kept from another check (a gap's place).
    pub fn renew(&mut self, t: &Ty, seen: &mut BTreeMap<usize, Ty>) -> Ty {
        match t {
            Ty::Meta(i) => seen.entry(*i).or_insert_with(|| self.meta()).clone(),
            Ty::Con(n, a) => Ty::Con(n.clone(), a.iter().map(|x| self.renew(x, seen)).collect()),
            Ty::Tuple(ts) => Ty::Tuple(ts.iter().map(|x| self.renew(x, seen)).collect()),
            Ty::Prog(x, r) => Ty::Prog(Box::new(self.renew(x, seen)), Row { effs: r.effs.clone(), rest: None }),
            Ty::Rigid(_) => t.clone(),
        }
    }

    /// Each of `params` as a new type to be found.
    pub fn fresh(&mut self, params: &[String]) -> BTreeMap<String, Ty> {
        params.iter().map(|p| (p.clone(), self.meta())).collect()
    }
}

/// The language's own constructors, Effects and host functions (13, 6.3): their fields and result.
pub fn builtin(u: &mut Unify, n: &str) -> Option<(Vec<Ty>, Ty)> {
    let (s, i, b) = (con("String"), con("Int"), con("Bool"));
    let (t, r) = (u.meta(), u.meta());
    let list = con1("List", t.clone());
    Some(match n {
        "Some" => (vec![t.clone()], con1("Option", t)),
        "None" => (vec![], con1("Option", t)),
        "Again" => (vec![t.clone()], Ty::Con("Loop".into(), vec![t, r])),
        "Done" => (vec![r.clone()], Ty::Con("Loop".into(), vec![t, r])),
        "Charge" => (vec![s, i], con("Unit")),
        "Now" => (vec![], i),
        "Refused" => (vec![s.clone(), s], con("Never")),
        "Unfit" | "Fault" => (vec![s], con("Never")),
        "Exhausted" => (vec![], con("Never")),
        "matches_glob" | "contains" => (vec![s.clone(), s], b),
        "diff_lines" => (vec![s.clone(), s], i),
        "added_lines" | "removed_lines" | "str_len" => (vec![s], i),
        "to_int" => (vec![s], con1("Option", i)),
        "is_subsequence" => (vec![list.clone(), list], b),
        "first" => (vec![list], con1("Option", t)),
        _ => return None,
    })
}
