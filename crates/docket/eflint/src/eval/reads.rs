//! What a rule reads: the derived types, the types a term mentions, and which derived types are worked out lazily.

use super::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Weak;

/// The types with a rule that works out their instances, tests (`Spec::tests`) aside.
pub fn derived_types(spec: &Spec) -> BTreeSet<Id> {
    spec.decls.iter().filter(|(d, t)| !t.derivation.is_empty() && !spec.tests.contains(*d)).map(|(d, _)| d.clone()).collect()
}

/// FIRST and every type their rules, conditions, effects, violations and fields name, transitively, each under the
/// name it is declared by: what a phrase naming FIRST may read.
pub fn reads(spec: &Spec, first: BTreeSet<Id>) -> BTreeSet<Id> {
    let mut seen = BTreeSet::new();
    let mut todo: Vec<Id> = first.into_iter().collect();
    while let Some(d) = todo.pop() {
        let d = spec.chase(&d);
        if !seen.insert(d.clone()) {
            continue;
        }
        if let Some(t) = spec.decls.get(&d) {
            todo.extend(mentions_of(t).iter().cloned());
            todo.extend(Spec::params(&d, t).iter().map(|v| v.dom.clone()));
        }
    }
    seen
}

thread_local! {
    /// Each declaration's [`type_mentions`], by the declaration read: a type given a rule for each instance put in has
    /// as many rules as instances, and is read at every step. The weak handle keeps the address from being reused.
    static MENTIONS: RefCell<HashMap<*const TypeSpec, (Weak<TypeSpec>, Rc<BTreeSet<Id>>)>> = RefCell::default();
}

/// T's mentions, read once while T stands.
fn mentions_of(t: &Rc<TypeSpec>) -> Rc<BTreeSet<Id>> {
    MENTIONS.with_borrow_mut(|m| {
        if let Some((w, got)) = m.get(&Rc::as_ptr(t))
            && w.strong_count() > 0
        {
            return got.clone();
        }
        if m.len() >= 4096 {
            m.retain(|_, (w, _)| w.strong_count() > 0);
        }
        let got = Rc::new(type_mentions(t));
        m.insert(Rc::as_ptr(t), (Rc::downgrade(t), got.clone()));
        got
    })
}

/// Every type a term names: applied, referred to, quantified over or projected on. Applying a type whose instances
/// are plain numbers or strings, `x(even)`, compiles to a tag of that type, so a tag names its type too.
pub fn mentions(t: &Term, out: &mut BTreeSet<Id>) {
    match t {
        Term::BoolLit(_) | Term::IntLit(_) | Term::StringLit(_) | Term::CurrentTime => {}
        Term::Bin(_, l, r) => {
            mentions(l, out);
            mentions(r, out);
        }
        Term::Un(_, x) => mentions(x, out),
        Term::Tag(x, d) => {
            out.insert(d.clone());
            mentions(x, out)
        }
        Term::Q(_, xs, x) => {
            out.extend(xs.iter().map(|v| v.dom.clone()));
            mentions(x, out);
        }
        Term::Project(x, v) => {
            out.insert(v.dom.clone());
            mentions(x, out);
        }
        Term::Ref(v) => {
            out.insert(v.dom.clone());
        }
        Term::App(d, args) => {
            out.insert(d.clone());
            match args {
                Args::Pos(ts) => ts.iter().for_each(|t| mentions(t, out)),
                Args::Mods(ms) => ms.iter().for_each(|(v, t)| {
                    out.insert(v.dom.clone());
                    mentions(t, out)
                }),
            }
        }
    }
}

/// Whether a term divides (`/`, `%`), which fails on zero.
pub(super) fn divides(t: &Term) -> bool {
    match t {
        Term::BoolLit(_) | Term::IntLit(_) | Term::StringLit(_) | Term::CurrentTime | Term::Ref(_) => false,
        Term::Bin(op, l, r) => matches!(op, BinOp::Div | BinOp::Mod) || divides(l) || divides(r),
        Term::Un(_, x) | Term::Tag(x, _) | Term::Q(_, _, x) | Term::Project(x, _) => divides(x),
        Term::App(_, args) => match args {
            Args::Pos(ts) => ts.iter().any(divides),
            Args::Mods(ms) => ms.iter().any(|(_, t)| divides(t)),
        },
    }
}

/// Whether a term refers to any of XS anywhere in it.
pub(super) fn names_any(t: &Term, xs: &[Var]) -> bool {
    match t {
        Term::BoolLit(_) | Term::IntLit(_) | Term::StringLit(_) | Term::CurrentTime => false,
        Term::Ref(v) => xs.contains(v),
        Term::Bin(_, l, r) => names_any(l, xs) || names_any(r, xs),
        Term::Un(_, x) | Term::Tag(x, _) | Term::Q(_, _, x) | Term::Project(x, _) => names_any(x, xs),
        Term::App(_, args) => match args {
            Args::Pos(ts) => ts.iter().any(|t| names_any(t, xs)),
            Args::Mods(ms) => ms.iter().any(|(_, t)| names_any(t, xs)),
        },
    }
}

/// The types T's rules and conditions name: its derivation, conditions, domain constraint, a duty's violation
/// conditions, and an act's or event's effects and syncs.
pub fn type_mentions(t: &TypeSpec) -> BTreeSet<Id> {
    let mut out = BTreeSet::new();
    for c in &t.derivation {
        match c {
            Derivation::HoldsWhen(x) => mentions(x, &mut out),
            Derivation::Dv(xs, x) => {
                out.extend(xs.iter().map(|v| v.dom.clone()));
                mentions(x, &mut out)
            }
        }
    }
    t.conditions.iter().for_each(|c| mentions(c, &mut out));
    mentions(&t.domain_constraint, &mut out);
    match &t.kind {
        Kind::Duty { violated_when, .. } => violated_when.iter().for_each(|c| mentions(c, &mut out)),
        Kind::Act { effects, syncs, .. } | Kind::Event { effects, syncs } => {
            effects.iter().for_each(|e| {
                out.extend(e.vars.iter().map(|v| v.dom.clone()));
                mentions(&e.term, &mut out)
            });
            syncs.iter().for_each(|y| {
                out.extend(y.vars.iter().map(|v| v.dom.clone()));
                mentions(&y.term, &mut out)
            });
        }
        Kind::Fact { .. } => {}
    }
    out
}

/// The derived acts and events no rule of another kind of type reads (no fact's or duty's derivation, condition or
/// violation, transitively): their instances matter only when one is triggered (`saturate_only` then) or the enabled
/// ones are listed. A record replayed for one question leaves them underived between triggers (`Replay::perform`).
pub fn lazy_types(spec: &Spec) -> BTreeSet<Id> {
    let mut lazy: BTreeSet<Id> = spec
        .decls
        .iter()
        .filter(|(_, t)| matches!(t.kind, Kind::Act { .. } | Kind::Event { .. }) && !t.derivation.is_empty())
        .map(|(d, _)| d.clone())
        .collect();
    loop {
        let mut read = BTreeSet::new();
        for (d, t) in &spec.decls {
            if lazy.contains(d) || matches!(t.kind, Kind::Act { .. } | Kind::Event { .. }) {
                continue;
            }
            read.extend(type_mentions(t));
        }
        let before = lazy.len();
        lazy.retain(|d| !read.contains(d));
        if lazy.len() == before {
            return lazy;
        }
    }
}

/// The lazy types to derive before a phrase naming FIRST is performed: those named, and those their rules name,
/// transitively.
pub fn needed_lazy(spec: &Spec, lazy: &BTreeSet<Id>, first: BTreeSet<Id>) -> BTreeSet<Id> {
    let mut seen = BTreeSet::new();
    let mut todo: Vec<Id> = first.into_iter().collect();
    while let Some(d) = todo.pop() {
        if !seen.insert(d.clone()) {
            continue;
        }
        if let Some(t) = spec.decls.get(&d) {
            todo.extend(type_mentions(t));
        }
    }
    seen.retain(|d| lazy.contains(d));
    seen
}

/// What working out T's instances reads: its rules, its conditions, its domain constraint and its fields' types.
/// Not its effects, syncs or violation conditions: those are read when it is triggered or asked, not derived.
pub(super) fn derivation_reads(spec: &Spec, d: &Id, t: &TypeSpec) -> BTreeSet<Id> {
    let mut out = BTreeSet::new();
    for c in &t.derivation {
        match c {
            Derivation::HoldsWhen(x) => mentions(x, &mut out),
            Derivation::Dv(xs, x) => {
                out.extend(xs.iter().map(|v| v.dom.clone()));
                mentions(x, &mut out)
            }
        }
    }
    t.conditions.iter().for_each(|c| mentions(c, &mut out));
    mentions(&t.domain_constraint, &mut out);
    out.extend(Spec::params(d, t).iter().map(|v| v.dom.clone()));
    out.iter().map(|x| spec.chase(x)).collect()
}
