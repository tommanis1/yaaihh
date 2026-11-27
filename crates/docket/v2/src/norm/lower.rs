//! A loaded text's normative declarations as eFLINT's, with the checks of 6.2 that eFLINT does not make: a rule that
//! binds a head field by no fact makes its type a test, and a test is never enumerated, created or ended.

use super::shape::{BOOL, Shape, Shapes};
use super::terms::Lower;
use crate::check::names::qualify;
use crate::run::norms::NormDecl;
use crate::run::value::Env;
use crate::syntax::tree::{Clause, DeclKind, E, Effectful, Expr, Norm, Op};
use eflint::ast as ef;
use std::collections::{BTreeMap, BTreeSet};

type R<T> = Result<T, String>;

fn bin(op: ef::BinOp, a: ef::Term, b: ef::Term) -> ef::Term {
    ef::Term::Bin(op, Box::new(a), Box::new(b))
}

/// The type D declares, extends or states an instance of.
pub fn type_of(d: &NormDecl) -> Option<String> {
    match &d.decl.what {
        DeclKind::Norm { name, .. } => Some(qualify(&d.module, name)),
        DeclKind::Extend { name, .. } => Some(name.to_string()),
        DeclKind::Holds(Expr { e: E::Apply(p, _), .. }) => Some(p.to_string()),
        _ => None,
    }
}

/// DECLS as eFLINT's: every type first, then every clause, so that a clause may name a type declared after it.
pub fn decls(shapes: &mut Shapes, decls: &[NormDecl]) -> R<Vec<ef::Decl>> {
    let of = |d: &NormDecl| match &d.decl.what {
        DeclKind::Norm { clauses, .. } | DeclKind::Extend { clauses, .. } => type_of(d).map(|t| (t, clauses.clone())),
        _ => None,
    };
    // a test binds nothing, so a type whose rule relies on one is a test too, until no more are found (6.2)
    let rules: Vec<(String, Vec<Clause>)> = decls.iter().filter_map(of).collect();
    loop {
        let mut found = Vec::new();
        for (ty, clauses) in rules.iter().filter(|(ty, _)| !shapes.tests.contains(ty)) {
            let fields = &shapes.get(ty)?.fields;
            if clauses.iter().any(|c| matches!(c, Clause::HoldsWhen(e) if !safe(shapes, fields, e))) {
                found.push(ty.clone());
            }
        }
        if found.is_empty() {
            break;
        }
        shapes.tests.extend(found);
    }
    let two = ef::Domain::Strings(vec![ef::id("false"), ef::id("true")]);
    let mut out = vec![ef::Decl::Type(ef::id(BOOL), spec(Norm::Fact, two))];
    let mut exts = Vec::new();
    for d in decls {
        let at = |why: String| format!("{}:{}: {why}", d.module, d.decl.at);
        if let Some((ty, clauses)) = of(d) {
            if let DeclKind::Norm { .. } = &d.decl.what {
                out.push(ef::Decl::Type(ef::id(&ty), type_spec(shapes.get(&ty).map_err(at)?)));
            }
            exts.push(ef::Decl::Ext(ef::id(&ty), clauses_of(shapes, &d.module, &ty, &clauses).map_err(at)?));
        } else if let DeclKind::Holds(e) = &d.decl.what {
            let t = Lower::new(shapes, &Env::new(), &[]).term(e).map_err(at)?;
            let ef::Term::App(ty, _) = &t else { return Err(at("a module's own fact is an instance (5.5)".into())) };
            exts.push(ef::Decl::Ext(ty.clone(), vec![ef::Clause::Derivation(vec![ef::Derivation::Dv(Vec::new(), t.clone())])]));
        }
    }
    out.extend(exts);
    Ok(out)
}

fn spec(kind: Norm, domain: ef::Domain) -> ef::TypeSpec {
    let kind = match kind {
        Norm::Fact => ef::Kind::Fact { invariant: false, actor: false },
        Norm::Act => ef::Kind::Act { effects: Vec::new(), syncs: Vec::new(), physical: false },
        Norm::Event => ef::Kind::Event { effects: Vec::new(), syncs: Vec::new() },
        Norm::Duty => ef::Kind::Duty { enforcing: Vec::new(), terminating: Vec::new(), creating: Vec::new(), violated_when: Vec::new() },
    };
    ef::TypeSpec { kind, domain, domain_constraint: ef::Term::BoolLit(true), restriction: None, derivation: Vec::new(), closed: None, conditions: Vec::new() }
}

fn type_spec(s: &Shape) -> ef::TypeSpec {
    let domain = match s.by.as_deref() {
        Some("int") => ef::Domain::AnyInt,
        Some(_) => ef::Domain::AnyString,
        None => ef::Domain::Products(s.fields.clone()),
    };
    spec(s.kind, domain)
}

/// The expressions a clause holds.
fn exprs(c: &Clause) -> Vec<&Expr> {
    match c {
        Clause::HoldsWhen(e) | Clause::ViolatedWhen(e) | Clause::ConditionedBy(e) | Clause::DerivedFrom(e) | Clause::Prefer(e) => vec![e],
        Clause::Creates(l) | Clause::Terminates(l) | Clause::Obfuscates(l) | Clause::SyncsWith(l) => l.iter().flat_map(|x| std::iter::once(&x.inst).chain(&x.when)).collect(),
        Clause::Returns(_) | Clause::Does(_) => Vec::new(),
    }
}

/// The clauses of type TY as eFLINT's, each with the module and line it is written at; the machine's (`Returns`,
/// `Does`) left out.
fn clauses_of(shapes: &Shapes, module: &str, ty: &str, clauses: &[Clause]) -> R<Vec<ef::Clause>> {
    let fields = &shapes.get(ty)?.fields;
    let env = Env::new();
    let mut out = Vec::new();
    for c in clauses {
        let mut lw = Lower::new(shapes, &env, fields);
        for e in exprs(c) {
            lw.types(e)?;
            enumerated(shapes, &|x| fields.iter().any(|f| &*f.dec == x), e)?;
        }
        let effects = |kind, l: &[Effectful]| -> R<ef::Clause> {
            let made = l.iter().map(|x| Ok(ef::Effect { kind, vars: Vec::new(), term: effectful(shapes, &lw, x)? }));
            Ok(ef::Clause::Effects(made.collect::<R<_>>()?))
        };
        let made = match c {
            Clause::HoldsWhen(e) => ef::Clause::Derivation(rule(&lw, fields, ty, e)?),
            Clause::DerivedFrom(e) => ef::Clause::Derivation(vec![match &e.e {
                E::Foreach(xs, v, Some(w)) => lw.within(e.at, xs, || Ok(ef::Derivation::Dv(lw.bound(xs)?, bin(ef::BinOp::When, lw.term(v)?, lw.truth(w)?))))?,
                _ => ef::Derivation::Dv(Vec::new(), lw.term(e)?),
            }]),
            Clause::ViolatedWhen(e) => ef::Clause::Violation(vec![lw.truth(e)?]),
            Clause::ConditionedBy(e) => ef::Clause::ConditionedBy(vec![lw.truth(e)?]),
            Clause::Creates(l) => effects(ef::EffKind::Create, l)?,
            Clause::Terminates(l) => effects(ef::EffKind::Terminate, l)?,
            Clause::Obfuscates(l) => effects(ef::EffKind::Obfuscate, l)?,
            Clause::SyncsWith(l) => ef::Clause::Sync(l.iter().map(|x| Ok(ef::Sync { vars: Vec::new(), term: effectful(shapes, &lw, x)? })).collect::<R<_>>()?),
            Clause::Prefer(_) => return Err(format!("{ty}: `Prefer` is not read yet")),
            Clause::Returns(_) | Clause::Does(_) => continue,
        };
        out.push(match exprs(c).first() {
            Some(e) => ef::Clause::At(module.into(), e.at.line as usize, Box::new(made)),
            None => made,
        });
    }
    Ok(out)
}

/// One instance of an effect or a sync, under its `When`; a test is never made or ended (6.2).
fn effectful(shapes: &Shapes, lw: &Lower, x: &Effectful) -> R<ef::Term> {
    if let E::Apply(p, _) = &x.inst.e
        && shapes.tests.contains(&p.to_string())
    {
        return Err(format!("{p} is a test (6.2): it is never created or ended"));
    }
    let inst = lw.term(&x.inst)?;
    Ok(match &x.when {
        Some(w) => bin(ef::BinOp::When, inst, lw.truth(w)?),
        None => inst,
    })
}

/// E split at its top `&&`s.
pub(super) fn conjuncts(e: &Expr) -> Vec<&Expr> {
    match &e.e {
        E::Bin(Op::And, a, b) => [conjuncts(a), conjuncts(b)].concat(),
        _ => vec![e],
    }
}

/// E as rules each of conjuncts: every top `&&` spread over the `||`s under it, so `a && (b || c)` is `a && b` and
/// `a && c`.
fn disjuncts(e: &Expr) -> Vec<Vec<&Expr>> {
    match &e.e {
        E::Bin(Op::Or, a, b) => [disjuncts(a), disjuncts(b)].concat(),
        E::Bin(Op::And, a, b) => {
            let right = disjuncts(b);
            disjuncts(a).into_iter().flat_map(|l| right.iter().map(move |r| [l.clone(), r.clone()].concat())).collect()
        }
        _ => vec![vec![e]],
    }
}

/// Whether P is a declared type that is no test: one whose instances bind its arguments.
fn binds(shapes: &Shapes, p: &str) -> bool {
    shapes.by.contains_key(p) && !shapes.tests.contains(p)
}

/// The variables that stand as an argument of a declared type that is no test, in a conjunct not under `Not`.
fn in_facts(shapes: &Shapes, parts: &[&Expr]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for p in parts {
        let p: &Expr = if let E::Holds(x) = &p.e { x } else { p };
        if let E::Apply(t, args) = &p.e
            && binds(shapes, &t.to_string())
        {
            out.extend(args.iter().filter_map(|a| if let E::Var(x) = &a.e { Some(x.clone()) } else { None }));
        }
    }
    out
}

/// A conjunct `f == x` (or `x == f`) that sets head field F, which no fact binds, to an expression over bound fields.
fn equated<'e>(p: &'e Expr, fields: &[ef::Var], bound: &BTreeSet<String>) -> Option<(String, &'e Expr)> {
    let E::Bin(Op::Eq, a, b) = &p.e else { return None };
    let head = |x: &Expr| match &x.e {
        E::Var(v) if fields.iter().any(|f| &*f.dec == v) && !bound.contains(v) => Some(v.clone()),
        _ => None,
    };
    let over_bound = |x: &Expr| !fields.iter().any(|f| !bound.contains(&*f.dec) && mentions(x, &f.dec));
    match (head(a), head(b)) {
        (Some(f), _) if over_bound(b) => Some((f, b)),
        (_, Some(f)) if over_bound(a) => Some((f, a)),
        _ => None,
    }
}

/// The parts of a condition.
pub(super) fn kids(e: &Expr) -> Vec<&Expr> {
    match &e.e {
        E::Apply(_, args) => args.iter().collect(),
        E::Bin(_, a, b) => vec![a, b],
        E::Foreach(_, v, c) => std::iter::once(&**v).chain(c.as_deref()).collect(),
        E::Not(x) | E::Neg(x) | E::Holds(x) | E::Violated(x) | E::Enabled(x) | E::Agg(_, x) | E::Field(x, _) | E::Quant(_, _, x) => vec![x],
        _ => Vec::new(),
    }
}

/// Whether variable X stands anywhere in E.
pub fn mentions(e: &Expr, x: &str) -> bool {
    matches!(&e.e, E::Var(v) if v == x) || kids(e).into_iter().any(|k| mentions(k, x))
}

/// Whether every head field stands in a fact of PARTS or is set equal to an expression (6.2).
fn covers(shapes: &Shapes, fields: &[ef::Var], parts: &[&Expr]) -> bool {
    let bound = in_facts(shapes, parts);
    let (set, _) = settings(shapes, fields, parts);
    fields.iter().all(|f| bound.contains(&*f.dec) || set.contains_key(&*f.dec))
}

/// The head fields PARTS set equal to an expression, each over fields a fact binds or set before it, and the rest.
fn settings<'e>(shapes: &Shapes, fields: &[ef::Var], parts: &[&'e Expr]) -> (BTreeMap<String, &'e Expr>, Vec<&'e Expr>) {
    let mut known = in_facts(shapes, parts);
    let (mut set, mut rest) = (BTreeMap::new(), parts.to_vec());
    while let Some((i, (f, x))) = rest.iter().enumerate().find_map(|(i, p)| equated(*p, fields, &known).map(|got| (i, got))) {
        rest.remove(i);
        known.insert(f.clone());
        set.insert(f, x);
    }
    (set, rest)
}

/// Whether a rule is safe (6.2): its body covers the head, or each of its disjuncts does.
fn safe(shapes: &Shapes, fields: &[ef::Var], e: &Expr) -> bool {
    covers(shapes, fields, &conjuncts(e)) || disjuncts(e).iter().all(|d| covers(shapes, fields, d))
}

/// A rule as eFLINT's: one derivation, or one for each disjunct when only the disjuncts cover the head.
fn rule(lw: &Lower, fields: &[ef::Var], ty: &str, e: &Expr) -> R<Vec<ef::Derivation>> {
    let whole = conjuncts(e);
    let rules = if covers(lw.shapes, fields, &whole) { vec![whole] } else { disjuncts(e) };
    rules.iter().map(|parts| one(lw, fields, ty, parts)).collect()
}

/// A rule of conjuncts PARTS as eFLINT's. A head field set equal to an expression is put in the head in its place
/// (6.2), and in the rest of its body, so the rule is a derivation from the rest of its body.
fn one(lw: &Lower, fields: &[ef::Var], ty: &str, parts: &[&Expr]) -> R<ef::Derivation> {
    let (set, rest) = settings(lw.shapes, fields, parts);
    let lowered = put_head(lw, fields, ty, &set).and_then(|()| rest.iter().map(|p| lw.truth(p)).collect::<R<Vec<_>>>());
    let head = fields.iter().map(|f| lw.put.borrow().get(&*f.dec).map_or(ef::Term::Ref(f.clone()), |(_, t)| t.clone())).collect();
    lw.put.borrow_mut().clear();
    let body = lowered?.into_iter().reduce(|a, b| bin(ef::BinOp::And, a, b)).unwrap_or(ef::Term::BoolLit(true));
    if set.is_empty() {
        return Ok(ef::Derivation::HoldsWhen(body));
    }
    Ok(ef::Derivation::Dv(Vec::new(), bin(ef::BinOp::When, ef::Term::App(ef::id(ty), ef::Args::Pos(head)), body)))
}

/// Each head field of SET lowered into `lw.put`, after the fields its expression names, so it reads their terms.
fn put_head(lw: &Lower, fields: &[ef::Var], ty: &str, set: &BTreeMap<String, &Expr>) -> R<()> {
    while lw.put.borrow().len() < set.len() {
        let waits = |x: &Expr| set.keys().any(|g| !lw.put.borrow().contains_key(g) && mentions(x, g));
        let next = fields.iter().find_map(|f| set.get(&*f.dec).filter(|x| !lw.put.borrow().contains_key(&*f.dec) && !waits(x)).map(|x| (f, *x)));
        let Some((f, x)) = next else { return Err(format!("{ty}: its head fields are set equal to each other in a ring (6.2)")) };
        let t = lw.typed(x, Some(&f.dom))?;
        lw.put.borrow_mut().insert(f.dec.to_string(), (x.clone(), t));
    }
    Ok(())
}

/// Refused if E enumerates a test (6.2): gives one of its fields a variable that is not GIVEN and stands as an
/// argument of no declared type that is no test, outside `Not`.
pub fn enumerated(shapes: &Shapes, given: &dyn Fn(&str) -> bool, e: &Expr) -> R<()> {
    fn walk<'e>(e: &'e Expr, not: bool, out: &mut Vec<(String, &'e [Expr], bool)>) {
        if let E::Apply(p, args) = &e.e {
            out.push((p.to_string(), args, not));
        }
        let not = not || matches!(e.e, E::Not(_));
        kids(e).into_iter().for_each(|k| walk(k, not, out));
    }
    let mut apps = Vec::new();
    walk(e, false, &mut apps);
    let var = |a: &Expr| if let E::Var(x) = &a.e { Some(x.clone()) } else { None };
    let binders: BTreeSet<String> = apps.iter().filter(|(p, _, not)| !not && binds(shapes, p)).flat_map(|(_, args, _)| args.iter().filter_map(var)).collect();
    for (p, args, _) in apps.iter().filter(|(p, _, _)| shapes.tests.contains(p)) {
        if let Some(x) = args.iter().filter_map(var).find(|x| !given(x) && !binders.contains(x)) {
            return Err(format!("{p} is a test (6.2): {x} would enumerate it; it is used only with all its fields given"));
        }
    }
    Ok(())
}
