//! The written forms of Part I and Part II (spec 2, 5): what the reader gives and the checker and machine read.

use super::lex::At;

/// A name as written, maybe qualified: `Wanted`, `H.Lets`, `std.hooks`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Path(pub Vec<String>);

impl Path {
    pub fn one(n: &str) -> Path {
        Path(vec![n.to_string()])
    }
    /// The last part: the name itself, without its module.
    pub fn last(&self) -> &str {
        self.0.last().map(String::as_str).unwrap_or("")
    }
}

impl std::fmt::Display for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.join("."))
    }
}

/// One file: `module a.b`, its imports, its declarations (5.6).
#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub name: Option<Path>,
    pub imports: Vec<Import>,
    pub decls: Vec<Decl>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Import {
    pub path: Path,
    pub alias: Option<String>,
    pub at: At,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Decl {
    pub at: At,
    pub what: DeclKind,
}

/// The four eFLINT kinds of declaration (5.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Norm {
    Fact,
    Act,
    Event,
    Duty,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DeclKind {
    /// `Fact F(x: A)`, `Fact F Identified by String`, `Act`, `Event`, `Duty`, with their clauses (5.3).
    Norm { kind: Norm, name: String, fields: Fields, clauses: Vec<Clause> },
    /// `Extend Fact M.F` with clauses added to a declaration, maybe of another module.
    Extend { kind: Norm, name: Path, clauses: Vec<Clause> },
    /// `+i.` at the top of a module: `i` holds while the module is loaded (5.5).
    Holds(Expr),
    /// `Effect E<X>(fields) -> T Does { … }` (5.1).
    Effect { name: String, params: Vec<String>, fields: Vec<Field>, result: Type, does: Option<Expr> },
    /// `Call C(fields) -> T Template "…"`: a prompt with holes, whose answer is a T.
    Call { name: String, fields: Vec<Field>, result: Type, template: Vec<Piece> },
    /// `type D<X> = | A(fields) Needs "w" | …`.
    Sum { name: String, params: Vec<String>, variants: Vec<Variant> },
    /// `type D { f: T, … }`.
    Record { name: String, params: Vec<String>, fields: Vec<Field> },
    /// `fn f<X>(fields) -> T { … }`.
    Fn { name: String, params: Vec<String>, fields: Vec<Field>, result: Type, body: Expr },
    /// `on X(pats) where c do p` (8.7).
    On { target: Path, pats: Vec<Pat>, cond: Option<Expr>, body: Expr },
}

impl DeclKind {
    /// The name the declaration gives, if it gives one (an `Extend`, a `+i.` and an `on` give none).
    pub fn name(&self) -> Option<&str> {
        match self {
            DeclKind::Norm { name, .. } | DeclKind::Effect { name, .. } | DeclKind::Call { name, .. } | DeclKind::Sum { name, .. } | DeclKind::Record { name, .. } | DeclKind::Fn { name, .. } => Some(name),
            DeclKind::Extend { .. } | DeclKind::Holds(_) | DeclKind::On { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Fields {
    /// `Identified by T`: one unnamed value.
    By(Type),
    /// `(x: A, y: B)`; none written for an Act or Event with no fields.
    Named(Vec<Field>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: Type,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    pub name: String,
    pub fields: Vec<Field>,
    /// `Needs "w"`: a value built with it is guarded (12).
    pub needs: Option<String>,
}

/// An eFLINT clause, read as eFLINT reads it (5.3), with `Prefer`, `Returns` and `Does` of Part II.
#[derive(Clone, Debug, PartialEq)]
pub enum Clause {
    HoldsWhen(Expr),
    ViolatedWhen(Expr),
    ConditionedBy(Expr),
    DerivedFrom(Expr),
    Creates(Vec<Effectful>),
    Terminates(Vec<Effectful>),
    Obfuscates(Vec<Effectful>),
    SyncsWith(Vec<Effectful>),
    Prefer(Expr),
    Returns(Type),
    Does(Expr),
}

/// One instance of a `Creates`, `Terminates`, `Obfuscates` or `Syncs with` list, and its `When`.
#[derive(Clone, Debug, PartialEq)]
pub struct Effectful {
    pub inst: Expr,
    pub when: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Type {
    /// A declared or built-in name with its arguments: `Int`, `List<T>`, `M.Verdict`, `X`.
    Named(Path, Vec<Type>),
    /// `(T, …, T)`; `()` is `Unit`.
    Tuple(Vec<Type>),
    /// `Program<T ! ρ>`.
    Program(Box<Type>, Row),
}

/// `{E, …, ..r}`; `Program<T>` written without one has an open row of its own (6.1).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Row {
    pub effects: Vec<Path>,
    pub rest: Option<String>,
    /// No row was written.
    pub implied: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    Int(i64),
    Bool(bool),
    Unit,
}

/// A part of a text literal: words, or a `{{ e }}` hole.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Words(String),
    Hole(Expr),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    And,
    Or,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quant {
    Exists,
    Forall,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Agg {
    Count,
    Sum,
    Max,
    Min,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub at: At,
    pub e: E,
}

/// One form for conditions and programs: the checker tells which an expression is (7).
#[derive(Clone, Debug, PartialEq)]
pub enum E {
    Lit(Lit),
    Text(Vec<Piece>),
    /// A name standing alone as written; after loading, a declaration by its qualified name (a constructor or a
    /// declaration with no fields).
    Name(Path),
    /// A variable, as loading resolves it: bound in scope, or bound by enumeration in a clause or a `where` (5.3).
    Var(String),
    /// `Ident(args)`, read by what `Ident` declares (5.2).
    Apply(Path, Vec<Expr>),
    Field(Box<Expr>, String),
    Bin(Op, Box<Expr>, Box<Expr>),
    /// `-e`.
    Neg(Box<Expr>),
    Not(Box<Expr>),
    /// `Holds(i)`, `Violated(i)`, `Enabled(i)`.
    Holds(Box<Expr>),
    Violated(Box<Expr>),
    Enabled(Box<Expr>),
    /// `Exists x, y : c`, `Forall x : c`.
    Quant(Quant, Vec<String>, Box<Expr>),
    /// `Foreach x, y : v When c`: the list of `v` for each way `c` holds, in the order of values (6.2).
    Foreach(Vec<String>, Box<Expr>, Option<Box<Expr>>),
    /// `Count(e)`, `Sum(e)` of a `Foreach`.
    Agg(Agg, Box<Expr>),
    Tuple(Vec<Expr>),
    List(Vec<Expr>),
    SelfParty,
    This,
    /// `x <- p; q`.
    Bind(String, Box<Expr>, Box<Expr>),
    /// `p; q`.
    Seq(Box<Expr>, Box<Expr>),
    Return(Box<Expr>),
    Perform(Path, Vec<Expr>),
    Handle(Box<Expr>, Option<(String, Box<Expr>)>, Vec<Arm>),
    Resume(String, Box<Expr>),
    Match(Box<Expr>, Vec<Case>),
    If(Box<Expr>, Box<Expr>, Option<Box<Expr>>),
    /// `par(p, q)`.
    Par(Vec<Expr>),
    /// `par x in e up to n { p }`.
    ParEach(String, Box<Expr>, Option<i64>, Box<Expr>),
    /// `iterate x = e up to n { p }`.
    Iterate(String, Box<Expr>, Option<i64>, Box<Expr>),
    Do(Box<Expr>),
    /// `...` (8.6).
    Gap,
}

/// A handler's arm: `E(pats), k where c => p`.
#[derive(Clone, Debug, PartialEq)]
pub struct Arm {
    pub at: At,
    pub effect: Path,
    pub pats: Vec<Pat>,
    pub k: String,
    pub cond: Option<Expr>,
    pub body: Expr,
}

/// A `match` arm: `| pat where c => p`.
#[derive(Clone, Debug, PartialEq)]
pub struct Case {
    pub pat: Pat,
    pub cond: Option<Expr>,
    pub body: Expr,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Pat {
    Any,
    Bind(String),
    Lit(Lit),
    Text(String),
    /// `C(pats)`, or `C` alone where `C` is a constructor or an instance's type.
    Ctor(Path, Vec<Pat>),
    Tuple(Vec<Pat>),
    /// `[p, …, ..rest]`: the rest bound to a name, or `..` / `.._` for any.
    List(Vec<Pat>, Option<Option<String>>),
}
