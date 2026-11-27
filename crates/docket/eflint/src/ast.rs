//! Abstract syntax of eFLINT: values, domains, terms, type declarations and phrases.

use std::fmt::Write;
use std::rc::Rc;

pub type Id = Rc<str>;

pub fn id(s: &str) -> Id {
    Rc::from(s)
}

/// A (possibly decorated) reference to a type, e.g. `person`, `person'`, `person2`.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Var {
    pub dom: Id,
    pub dec: Id,
}

impl Var {
    pub fn plain(d: &Id) -> Var {
        Var { dom: d.clone(), dec: id("") }
    }
}

/// The order of the variants is the order used for sorting instances.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum Elem {
    Str(Id),
    Int(i64),
    Prod(Rc<[Tagged]>),
}

/// An instance: a value together with the type it is an instance of.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Tagged {
    pub elem: Elem,
    pub dom: Id,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Domain {
    AnyString,
    AnyInt,
    Strings(Vec<Id>),
    Ints(Vec<i64>),
    Products(Vec<Var>),
    Time,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Args {
    Pos(Vec<Term>),
    Mods(Vec<(Var, Term)>),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BinOp {
    When,
    Or,
    And,
    Eq,
    Neq,
    Leq,
    Geq,
    Le,
    Ge,
    Sub,
    Add,
    Mult,
    Mod,
    Div,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Quant {
    Exists,
    Forall,
    Count,
    Sum,
    Max,
    Min,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnOp {
    Not,
    Present,
    Violated,
    Enabled,
    Untag,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Term {
    BoolLit(bool),
    IntLit(i64),
    StringLit(Id),
    Bin(BinOp, Box<Term>, Box<Term>),
    Un(UnOp, Box<Term>),
    Q(Quant, Vec<Var>, Box<Term>),
    Project(Box<Term>, Var),
    Tag(Box<Term>, Id),
    Ref(Var),
    App(Id, Args),
    CurrentTime,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Derivation {
    Dv(Vec<Var>, Term),
    HoldsWhen(Term),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EffKind {
    Create,
    Terminate,
    Obfuscate,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Effect {
    pub kind: EffKind,
    pub vars: Vec<Var>,
    pub term: Term,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Sync {
    pub vars: Vec<Var>,
    pub term: Term,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Kind {
    Fact { invariant: bool, actor: bool },
    Act { effects: Vec<Effect>, syncs: Vec<Sync>, physical: bool },
    Duty { enforcing: Vec<Id>, terminating: Vec<Id>, creating: Vec<Id>, violated_when: Vec<Term> },
    Event { effects: Vec<Effect>, syncs: Vec<Sync> },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Restriction {
    Var,
    Function,
}

#[derive(Clone, PartialEq, Debug)]
pub struct TypeSpec {
    pub kind: Kind,
    pub domain: Domain,
    pub domain_constraint: Term,
    pub restriction: Option<Restriction>,
    pub derivation: Vec<Derivation>,
    /// `None`: closed by default; `Some(false)`: open; `Some(true)`: closed again after being opened.
    pub closed: Option<bool>,
    pub conditions: Vec<Term>,
}

impl TypeSpec {
    pub fn is_closed(&self) -> bool {
        self.closed.unwrap_or(true)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Clause {
    ConditionedBy(Vec<Term>),
    Derivation(Vec<Derivation>),
    Effects(Vec<Effect>),
    Sync(Vec<Sync>),
    Violation(Vec<Term>),
    Enforcing(Vec<Id>),
    TerminatedBy(Vec<Id>),
    CreatedBy(Vec<Id>),
    Strings(Vec<Id>),
    Ints(Vec<i64>),
    Close,
    Open,
    /// A clause with where it is written: the file (empty: the one being read) and the line.
    At(Rc<str>, usize, Box<Clause>),
}

#[derive(Clone, PartialEq, Debug)]
pub enum Decl {
    Type(Id, TypeSpec),
    Ext(Id, Vec<Clause>),
    Placeholder(Id, Id),
}

#[derive(Clone, PartialEq, Debug)]
pub enum Phrase {
    Trigger(Vec<Var>, Term),
    Create(Vec<Var>, Term),
    Terminate(Vec<Var>, Term),
    Obfuscate(Vec<Var>, Term),
    Query(Term),
    InstQuery(bool, Vec<Var>, Term),
    Decls(Vec<Decl>),
    Par(Vec<Phrase>),
}

#[derive(Clone, PartialEq, Debug)]
pub enum Statement {
    Include(String),
    Require(String),
    Phrase(Phrase),
}

/// The four-part file format of eFLINT 1 (`#`, `##`, `###`, `####`).
#[derive(Clone, PartialEq, Debug)]
pub struct OldFile {
    pub decls: Vec<Decl>,
    pub refiner: Vec<(Id, Domain)>,
    pub initial: Vec<Effect>,
    pub scenario: Vec<Phrase>,
}

// ---------------------------------------------------------------------------------------------
// Printing

/// A string literal the way instances are printed: double quotes, backslash escapes.
pub fn show_string(s: &str) -> String {
    let mut out = String::from("\"");
    let mut numeric = false;
    for c in s.chars() {
        let was_numeric = numeric;
        numeric = false;
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 32 || (c as u32) >= 127 => {
                let _ = write!(out, "\\{}", c as u32);
                numeric = true;
            }
            c => {
                if was_numeric && c.is_ascii_digit() {
                    out.push_str("\\&");
                }
                out.push(c)
            }
        }
    }
    out.push('"');
    out
}

impl std::fmt::Display for Tagged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.elem {
            Elem::Str(s) => write!(f, "{}({})", self.dom, show_string(s)),
            Elem::Int(i) => write!(f, "{}({})", self.dom, i),
            Elem::Prod(args) => {
                write!(f, "{}(", self.dom)?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(",")?;
                    }
                    write!(f, "{}", a)?;
                }
                f.write_str(")")
            }
        }
    }
}

/// Terms, variables and domains written in the reference's structural form, as its error messages give them.
pub mod dump {
    use super::*;

    fn list<T>(xs: &[T], f: impl Fn(&T) -> String) -> String {
        format!("[{}]", xs.iter().map(f).collect::<Vec<_>>().join(","))
    }
    pub fn var(v: &Var) -> String {
        format!("Var {} {}", show_string(&v.dom), show_string(&v.dec))
    }
    fn vars(vs: &[Var]) -> String {
        list(vs, var)
    }
    fn p(t: &Term) -> String {
        match t {
            Term::CurrentTime => "CurrentTime".into(),
            _ => format!("({})", term(t)),
        }
    }
    pub fn term(t: &Term) -> String {
        match t {
            Term::BoolLit(b) => format!("BoolLit {}", if *b { "True" } else { "False" }),
            Term::IntLit(i) => format!("IntLit {}", i),
            Term::StringLit(s) => format!("StringLit {}", show_string(s)),
            Term::Bin(op, a, b) => format!("{:?} {} {}", op, p(a), p(b)),
            Term::Un(op, a) => format!("{:?} {}", op, p(a)),
            Term::Q(q, vs, a) => format!("{:?} {} {}", q, vars(vs), p(a)),
            Term::Project(a, v) => format!("Project {} ({})", p(a), var(v)),
            Term::Tag(a, d) => format!("Tag {} {}", p(a), show_string(d)),
            Term::Ref(v) => format!("Ref ({})", var(v)),
            Term::App(d, Args::Pos(ts)) => format!("App {} (Left {})", show_string(d), list(ts, term)),
            Term::App(d, Args::Mods(ms)) => format!(
                "App {} (Right {})",
                show_string(d),
                list(ms, |(v, t)| format!("Rename ({}) {}", var(v), p(t)))
            ),
            Term::CurrentTime => "CurrentTime".into(),
        }
    }
    fn strs(xs: &[Id]) -> String {
        list(xs, |s| show_string(s))
    }
    fn ints(xs: &[i64]) -> String {
        list(xs, |i| i.to_string())
    }
    pub fn domain(d: &Domain) -> String {
        match d {
            Domain::AnyString => "AnyString".into(),
            Domain::AnyInt => "AnyInt".into(),
            Domain::Time => "Time".into(),
            Domain::Strings(ss) => format!("Strings {}", strs(ss)),
            Domain::Ints(is) => format!("Ints {}", ints(is)),
            Domain::Products(vs) => format!("Products {}", vars(vs)),
        }
    }
}
