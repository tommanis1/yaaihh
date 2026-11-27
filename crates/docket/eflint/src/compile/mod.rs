//! Specifications, the types of the static phase, and the free variables of a term.
//!
//! A phrase is checked against the specification before it runs: names are resolved, literals and
//! instances are converted to the type an operator or parameter expects, and variables that are
//! used but not bound become quantified (`Exists` in conditions and queries, `Foreach` in
//! effects, derivations and triggers). Declarations update the specification.

use crate::ast::*;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

mod decls;
mod host;
mod terms;

pub use self::host::{Host, HostTable};

#[derive(Clone, PartialEq)]
pub struct Spec {
    pub decls: BTreeMap<Id, Rc<TypeSpec>>,
    pub aliases: BTreeMap<Id, Id>,
    pub reading: Reading,
    pub host: Host,
    /// Types worked out for one instance when it is asked about and never listed: a caller names a type here when a
    /// rule of it leaves a field to its type's instances, which are not to be enumerated.
    pub tests: BTreeSet<Id>,
    /// The file whose clauses are being read, for the places of its rules.
    pub file: Rc<str>,
    /// Where each rule, condition and violation condition is written, for what is said about it.
    pub lines: Rc<Vec<Line>>,
    /// The graph of its derived types, made when first read (`eval::known`).
    pub graph: crate::eval::KnownGraph,
}

/// A compiled rule, condition or violation condition of a type, with the file and line it is written at.
#[derive(Clone, PartialEq, Debug)]
pub struct Line {
    pub ty: Id,
    pub term: Term,
    pub file: Rc<str>,
    pub line: usize,
}

/// How a specification's types are worked out, chosen by its caller; both off is how the reference reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Reading {
    /// A derived type found from the instances of the facts its rule names, not by trying every combination of its
    /// fields (`eval::guided_bindings`); the same instances either way.
    pub guided: bool,
    /// A variable of a type whose instances cannot be listed (`Identified by String` or `Int`, the built-in `string`
    /// and `int`) ranges over that type's instances that hold and the values fields of that type take in instances
    /// that hold, so a value stands in a fact without being declared first; and an act's or event's instance is there
    /// for any valid fields, so its conditions alone say whether it is enabled.
    pub ranging: bool,
}

fn fact_spec(domain: Domain, actor: bool) -> Rc<TypeSpec> {
    Rc::new(TypeSpec {
        kind: Kind::Fact { invariant: false, actor },
        domain,
        domain_constraint: Term::BoolLit(true),
        restriction: None,
        derivation: Vec::new(),
        closed: None,
        conditions: Vec::new(),
    })
}

impl Default for Spec {
    fn default() -> Self {
        Self::new()
    }
}

impl Spec {
    /// The empty specification holds the built-in types `int`, `string` and `actor`.
    pub fn new() -> Spec {
        let mut decls = BTreeMap::new();
        decls.insert(id("int"), fact_spec(Domain::AnyInt, false));
        decls.insert(id("string"), fact_spec(Domain::AnyString, false));
        decls.insert(id("actor"), fact_spec(Domain::AnyString, true));
        Spec {
            decls,
            aliases: BTreeMap::new(),
            reading: Reading::default(),
            host: Host::default(),
            tests: BTreeSet::new(),
            file: Rc::from(""),
            lines: Rc::new(Vec::new()),
            graph: Default::default(),
        }
    }

    /// `FILE:LINE` of the clause of type TY that compiled to TERM, if it was read with its place.
    pub fn place(&self, ty: &Id, term: &Term) -> Option<String> {
        let l = self.lines.iter().find(|l| l.ty == *ty && l.term == *term)?;
        Some(format!("{}:{}", l.file, l.line))
    }

    /// Follow placeholder declarations to the type they stand for.
    pub fn chase(&self, d: &Id) -> Id {
        let mut tried: Vec<Id> = Vec::new();
        let mut cur = d.clone();
        loop {
            if tried.contains(&cur) {
                return cur;
            }
            match self.aliases.get(&cur) {
                Some(next) => {
                    tried.push(cur);
                    cur = next.clone();
                }
                None => return cur,
            }
        }
    }

    pub fn find(&self, d: &Id) -> Option<&Rc<TypeSpec>> {
        self.decls.get(&self.chase(d))
    }

    pub fn type_of(&self, v: &Var) -> Id {
        self.chase(&v.dom)
    }

    pub fn enumerable(&self, d: &Domain) -> bool {
        match d {
            Domain::Strings(_) | Domain::Ints(_) => true,
            Domain::Products(vs) => {
                vs.iter().all(|v| self.find(&self.type_of(v)).map_or(false, |t| self.enumerable(&t.domain)))
            }
            Domain::AnyString | Domain::AnyInt | Domain::Time => false,
        }
    }

    pub fn triggerable(&self, d: &Id) -> bool {
        matches!(self.find(d).map(|t| &t.kind), Some(Kind::Act { .. }) | Some(Kind::Event { .. }))
    }

    pub fn invariants(&self) -> Vec<Id> {
        self.decls
            .iter()
            .filter(|(_, t)| matches!(t.kind, Kind::Fact { invariant: true, .. }))
            .map(|(d, _)| d.clone())
            .collect()
    }

    /// The parameters of a type: the fields of a composite type, or the type itself.
    pub fn params(d: &Id, t: &TypeSpec) -> Vec<Var> {
        match &t.domain {
            Domain::Products(xs) => xs.clone(),
            _ => vec![Var::plain(d)],
        }
    }
}

/// The variables a term uses without binding them.
pub fn free(spec: &Spec, t: &Term) -> BTreeSet<Var> {
    let mut out = BTreeSet::new();
    free_into(spec, t, &mut out);
    out
}

fn free_into(spec: &Spec, t: &Term, out: &mut BTreeSet<Var>) {
    match t {
        Term::BoolLit(_) | Term::IntLit(_) | Term::StringLit(_) | Term::CurrentTime => {}
        Term::Ref(v) => {
            out.insert(v.clone());
        }
        Term::App(d, args) => match spec.find(d).map(|t| &t.domain) {
            Some(Domain::Products(xs)) => {
                let replaced: Vec<(&Var, &Term)> = match args {
                    Args::Mods(ms) => ms.iter().map(|(v, t)| (v, t)).collect(),
                    Args::Pos(ts) => xs.iter().zip(ts.iter()).collect(),
                };
                for x in xs {
                    if !replaced.iter().any(|(v, _)| *v == x) {
                        out.insert(x.clone());
                    }
                }
                for (_, t) in replaced {
                    free_into(spec, t, out);
                }
            }
            _ => match args {
                Args::Pos(ts) => ts.iter().for_each(|t| free_into(spec, t, out)),
                Args::Mods(ms) => ms.iter().for_each(|(_, t)| free_into(spec, t, out)),
            },
        },
        Term::Un(_, a) | Term::Project(a, _) | Term::Tag(a, _) => free_into(spec, a, out),
        Term::Bin(_, a, b) => {
            free_into(spec, a, out);
            free_into(spec, b, out);
        }
        Term::Q(_, xs, a) => {
            let mut inner = BTreeSet::new();
            free_into(spec, a, &mut inner);
            for x in xs {
                inner.remove(x);
            }
            out.extend(inner);
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Ty {
    Strings,
    Ints,
    Bool,
    Tagged(Id),
}

fn show_ty(t: &Ty) -> String {
    match t {
        Ty::Strings => "TyStrings".into(),
        Ty::Ints => "TyInts".into(),
        Ty::Bool => "TyBool".into(),
        Ty::Tagged(d) => format!("TyTagged {}", show_string(d)),
    }
}

/// A phrase after the static phase.
#[derive(Clone, Debug)]
pub enum CPhrase {
    Trigger(Vec<Var>, Term),
    Create(Vec<Var>, Term),
    Terminate(Vec<Var>, Term),
    Obfuscate(Vec<Var>, Term),
    Query(Term),
    InstQuery(bool, Vec<Var>, Term),
    OnlyDecls,
    Par(Box<CPhrase>, Box<CPhrase>),
}

type S<T> = Result<T, Vec<String>>;

fn err<T>(s: String) -> S<T> {
    Err(vec![s])
}

fn bx(t: Term) -> Box<Term> {
    Box::new(t)
}

fn without(fs: BTreeSet<Var>, bound: &[Var]) -> Vec<Var> {
    fs.into_iter().filter(|v| !bound.contains(v)).collect()
}

fn exists(unbound: Vec<Var>, t: Term) -> Term {
    if unbound.is_empty() {
        t
    } else {
        Term::Q(Quant::Exists, unbound, bx(t))
    }
}

/// Whether instances of the first domain can stand where the second is expected.
fn match_domain(from: &Domain, to: &Domain) -> bool {
    use Domain::*;
    match (from, to) {
        (AnyString, AnyString) | (Strings(_), AnyString) => true,
        (Strings(a), Strings(b)) => a.iter().all(|s| b.contains(s)),
        (AnyInt, AnyInt) | (Ints(_), AnyInt) | (AnyInt, Ints(_)) => true,
        (Ints(a), Ints(b)) => a.iter().all(|i| b.contains(i)),
        (Time, Time) | (Time, AnyInt) => true,
        (Products(a), Products(b)) => a == b,
        _ => false,
    }
}

pub struct Static<'a> {
    pub spec: &'a mut Spec,
}
