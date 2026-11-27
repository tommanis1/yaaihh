//! The loaded rules as an explanation reads them: types, clauses, instances written in them, and cycles through `Not`.

use super::lower::{kids, mentions, type_of};
use crate::run::explain::{Link, Span};
use crate::run::norms::NormDecl;
use crate::run::value::{Env, Instance, Value};
use crate::syntax::form;
use crate::syntax::lex::At;
use crate::syntax::tree::{Clause, DeclKind, E, Expr, Fields, Lit, Op, Piece, Quant, Type};
use std::collections::{BTreeMap, BTreeSet};

/// Each loaded type's field names, whether a primitive identifies it, its fields' types, its clauses by module, and the modules' own facts.
pub(super) struct Rules<'d> {
    pub(super) types: BTreeMap<String, (Vec<String>, bool)>,
    field_types: BTreeMap<String, Vec<String>>,
    clauses: BTreeMap<String, Vec<(&'d str, &'d Clause)>>,
    postulates: Vec<(&'d str, At, &'d Expr)>,
}

impl<'d> Rules<'d> {
    pub(super) fn of(decls: &'d [NormDecl]) -> Rules<'d> {
        let mut out = Rules { types: BTreeMap::new(), field_types: BTreeMap::new(), clauses: BTreeMap::new(), postulates: Vec::new() };
        for d in decls {
            let Some(ty) = type_of(d) else { continue };
            match &d.decl.what {
                DeclKind::Norm { fields, clauses, .. } => {
                    let names = match fields {
                        Fields::Named(fs) => (fs.iter().map(|f| f.name.clone()).collect(), false),
                        Fields::By(_) => (Vec::new(), true),
                    };
                    if let Fields::Named(fs) = fields {
                        out.field_types.insert(ty.clone(), fs.iter().map(|f| if let Type::Named(p, _) = &f.ty { p.to_string() } else { String::new() }).collect());
                    }
                    out.types.insert(ty.clone(), names);
                    out.clauses.entry(ty).or_default().extend(clauses.iter().map(|c| (d.module.as_str(), c)));
                }
                DeclKind::Extend { clauses, .. } => out.clauses.entry(ty).or_default().extend(clauses.iter().map(|c| (d.module.as_str(), c))),
                DeclKind::Holds(e) => out.postulates.push((d.module.as_str(), d.decl.at, e)),
                _ => {}
            }
        }
        out
    }

    pub(super) fn of_type(&self, ty: &str) -> Vec<(&'d str, &'d Clause)> {
        self.clauses.get(ty).cloned().unwrap_or_default()
    }

    /// The fields of I's type bound to I's values.
    pub(super) fn head(&self, i: &Instance) -> Env {
        self.types.get(&i.ty).map_or_else(Env::new, |(names, _)| names.iter().cloned().zip(i.args.iter().cloned()).collect())
    }

    /// The value E gives under ENV when it is built of bound variables, literals and instances only.
    fn value(&self, e: &Expr, env: &Env) -> Option<Value> {
        match &e.e {
            E::Var(x) => env.get(x).cloned(),
            E::Lit(Lit::Int(n)) => Some(Value::Int(*n)),
            E::Lit(Lit::Bool(b)) => Some(Value::Bool(*b)),
            E::Text(ps) => ps.iter().map(|p| if let Piece::Words(w) = p { Some(w.as_str()) } else { None }).collect::<Option<String>>().map(Value::Text),
            E::Apply(..) => match self.ground(e, env)? {
                Instance { ty, mut args } if self.types.get(&ty).is_some_and(|(_, by)| *by) => args.pop(),
                i => Some(Value::Inst(i)),
            },
            _ => None,
        }
    }

    /// The instance atom E (`T(args)` or `Holds(T(args))` of a loaded type) is under ENV, if its values are known.
    pub(super) fn ground(&self, e: &Expr, env: &Env) -> Option<Instance> {
        let e = if let E::Holds(x) = &e.e { x } else { e };
        let E::Apply(p, args) = &e.e else { return None };
        let ty = p.to_string();
        self.types.contains_key(&ty).then_some(())?;
        Some(Instance { ty, args: args.iter().map(|a| self.value(a, env)).collect::<Option<_>>()? })
    }

    /// Where instance I is stated by its module, if it is.
    pub(super) fn postulate(&self, i: &Instance) -> Option<Span> {
        self.postulates.iter().find(|(_, _, e)| self.ground(e, &Env::new()).as_ref() == Some(i)).map(|(m, at, _)| span(m, *at))
    }

    /// What E reads of loaded types, each with whether it is read falling: under `Not`, as a `Forall`'s domain, or in a count.
    pub(super) fn reads<'e>(&self, e: &'e Expr, neg: bool, out: &mut Vec<(Read<'e>, bool)>) {
        match &e.e {
            E::Apply(p, _) if self.types.contains_key(&p.to_string()) => out.push((Read::Atom(e), neg)),
            E::Holds(x) => self.reads(x, neg, out),
            E::Not(x) => self.reads(x, !neg, out),
            E::Quant(Quant::Forall, xs, x) => {
                self.reads(x, neg, out);
                let mut domains: Vec<String> = xs.iter().filter_map(|v| self.type_of_var(x, v)).collect();
                domains.dedup();
                out.extend(domains.into_iter().map(|t| (Read::Domain(t, e), !neg)));
            }
            E::Agg(_, x) => self.reads(x, true, out),
            _ => kids(e).into_iter().for_each(|k| self.reads(k, neg, out)),
        }
    }

    /// The loaded type of variable V where E first gives it as a field of an instance atom.
    fn type_of_var(&self, e: &Expr, v: &str) -> Option<String> {
        if let E::Apply(p, args) = &e.e
            && let Some(k) = args.iter().position(|a| matches!(&a.e, E::Var(x) if x == v))
            && let Some(t) = self.field_types.get(&p.to_string()).and_then(|ts| ts.get(k))
        {
            return self.types.contains_key(t).then(|| t.clone());
        }
        kids(e).into_iter().find_map(|k| self.type_of_var(k, v))
    }

    /// The rules that derive an instance of TY: `Holds when` bodies, and `Derived from` with its head and body.
    pub(super) fn derivations(&self, ty: &str) -> Vec<(&'d str, Derivation<'d>)> {
        self.of_type(ty)
            .iter()
            .filter_map(|(m, c)| match c {
                Clause::HoldsWhen(e) => Some((*m, Derivation::When(e))),
                Clause::DerivedFrom(e) => Some((*m, match &e.e {
                    E::Foreach(_, v, w) => Derivation::From(e, v, w.as_deref()),
                    _ => Derivation::From(e, e, None),
                })),
                _ => None,
            })
            .collect()
    }
}

/// What a rule's body reads: an instance atom, or every instance of a type, as the `Forall` given ranges over them.
pub(super) enum Read<'e> {
    Atom(&'e Expr),
    Domain(String, &'e Expr),
}

/// One rule for a type: `Holds when` E, or `Derived from` a `Foreach` (its whole, its head, its body if any).
#[derive(Clone, Copy)]
pub(super) enum Derivation<'d> {
    When(&'d Expr),
    From(&'d Expr, &'d Expr, Option<&'d Expr>),
}

impl Derivation<'_> {
    pub(super) fn whole(&self) -> &Expr {
        match self {
            Derivation::When(e) | Derivation::From(e, _, _) => e,
        }
    }
    pub(super) fn text(&self) -> String {
        match self {
            Derivation::When(e) => format!("Holds when {}", form::expr(e)),
            Derivation::From(e, _, _) => format!("Derived from {}", form::expr(e)),
        }
    }
}

pub(super) fn span(module: &str, at: At) -> Span {
    Span { module: module.to_string(), line: at.line, col: at.col }
}

/// ENV's values of the variables E names.
pub(super) fn shown(e: &Expr, env: &Env) -> Env {
    env.iter().filter(|(k, _)| mentions(e, k)).map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// E split at its top `||`s.
pub(super) fn ors(e: &Expr) -> Vec<&Expr> {
    match &e.e {
        E::Bin(Op::Or, a, b) => [ors(a), ors(b)].concat(),
        _ => vec![e],
    }
}

/// The values of head V's variables that make it instance I, if any do.
pub(super) fn unify(rules: &Rules, v: &Expr, i: &Instance) -> Option<Env> {
    let E::Apply(p, args) = &v.e else { return None };
    if p.to_string() != i.ty || args.len() != i.args.len() {
        return None;
    }
    let mut env = Env::new();
    for (a, x) in args.iter().zip(&i.args) {
        match &a.e {
            E::Var(n) if env.get(n).is_none_or(|y| y == x) => {
                env.insert(n.clone(), x.clone());
            }
            _ if rules.value(a, &env).as_ref() == Some(x) => {}
            _ => return None,
        }
    }
    Some(env)
}

/// A cycle through `Not` from I back to itself in DECLS' rules (`Explain::Unknown`), read from the rules alone, loaded or not.
pub fn cycle(decls: &[NormDecl], i: &Instance) -> Option<Vec<Link>> {
    let rules = Rules::of(decls);
    let mut path = Vec::new();
    let mut seen = BTreeSet::from([(i.clone(), false)]);
    walk(&rules, i, i, false, &mut path, &mut seen).then_some(path)
}

pub(super) fn walk(rules: &Rules, start: &Instance, x: &Instance, neg: bool, path: &mut Vec<Link>, seen: &mut BTreeSet<(Instance, bool)>) -> bool {
    for (m, d) in rules.derivations(&x.ty) {
        let (env, body) = match d {
            Derivation::When(e) => (rules.head(x), Some(e)),
            Derivation::From(_, v, w) => match unify(rules, v, x) {
                Some(env) => (env, w),
                None => continue,
            },
        };
        let mut reads = Vec::new();
        if let Some(b) = body {
            rules.reads(b, false, &mut reads);
        }
        for (read, negated) in reads {
            let n = neg || negated;
            let a = match read {
                Read::Domain(t, q) if t == start.ty && n => {
                    path.push(Link { inst: x.clone(), at: span(m, q.at), text: d.text(), negated });
                    return true;
                }
                Read::Domain(..) => continue,
                Read::Atom(a) => a,
            };
            let Some(y) = rules.ground(a, &env) else { continue };
            path.push(Link { inst: x.clone(), at: span(m, a.at), text: d.text(), negated });
            if (&y == start && n) || (seen.insert((y.clone(), n)) && walk(rules, start, &y, n, path, seen)) {
                return true;
            }
            path.pop();
        }
    }
    false
}

