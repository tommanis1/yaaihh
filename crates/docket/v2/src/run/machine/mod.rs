//! The machine (spec 8): runs as CEK states with deep handlers, acts through the normative core, Effects to drivers.

use super::driver::{Driver, Reply};
use super::host;
use super::norms::{Norms, Why};
use super::record::{Line, Refusal};
use super::value::{Closure, Env, Instance, Prog, Value};
use crate::check::names::{Loaded, On, Sym};
use crate::syntax::tree::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

mod act;
mod caps;
mod drive;
mod eval;
mod gap;
mod give;
mod handle;
mod lost;
use handle::Start;
mod par;
mod query;
pub use query::{Enabled, Holds};
mod replay;
mod reload;
mod restore;
mod runs;
mod values;

pub type RunId = usize;

/// What the machine does next in a run: evaluate an expression, give a value to the top frame, or perform an Effect.
#[derive(Clone, Debug)]
enum Ctl {
    Eval(Expr, Env, Arc<str>),
    Give(Value),
    Perform(String, Vec<Value>),
    /// A connection's Act, performed once as its run's program (8.8).
    Act(Instance),
    /// `Charge` going on outward from where an arm took it (10).
    Outward(String, Vec<Value>, Start),
}

/// What waits for a value: the context `K` of 8.3, one frame at a time.
#[derive(Clone, Debug)]
enum Frame {
    Bind(String, Expr, Env, Arc<str>),
    Seq(Expr, Env, Arc<str>),
    /// A handler (8.3): its `return` clause and arms, the scope they close over, their module.
    Handle(Option<(String, Expr)>, Vec<Arm>, Env, Arc<str>),
    /// Values of sub-expressions being gathered, then used by `then`.
    Gather { done: Vec<Value>, todo: VecDeque<Expr>, env: Env, m: Arc<str>, then: Then },
    If(Expr, Option<Expr>, Env, Arc<str>),
    Match(Vec<Case>, Env, Arc<str>),
    Loop { x: String, body: Expr, n: Option<i64>, round: i64, env: Env, m: Arc<str> },
    /// A `par` waiting for its branches: their runs, the values come, what is not begun yet, the bound.
    Par { runs: Vec<Option<RunId>>, got: Vec<Option<Value>>, rest: VecDeque<(usize, Expr, Env)>, up_to: usize, list: bool, m: Arc<str> },
    /// The bottom of an arm taken in this run: below it is what was outside the handler.
    ArmEnd(usize),
    /// An arm of a handler in run `X` at frame `h`, taken for this run (a branch under `X`).
    Across(usize, RunId, usize),
    /// A root arm's body (8.7): searches from it go to the root arms loaded before arm `i`; its value resumes,
    /// or for a `Charge` (its arguments here) the root arms before it are tried next (10).
    Root(usize, Option<Vec<Value>>),
    /// Whatever comes, give this value: an arm's value kept while `Charge` goes on outward.
    Keep(Value),
    /// The costs still to charge after a driver's answer, and the answer (10).
    Charges(VecDeque<(String, i64)>, Value),
    /// A gap's answer, to be checked and run (8.6) in the scope of the gap's module: the module and the site.
    Fill(Arc<str>, String),
    /// An Act's or Event's `Does` body gives the act's value.
    Body,
}

#[derive(Clone, Debug)]
enum Then {
    Apply(String),
    /// A `fn`'s arguments: the program values made already, and the places the gathered values fill.
    ApplyFn(String, Vec<Option<Value>>),
    Perform(String),
    Tuple,
    List,
    Bin(Op),
    Neg,
    Not,
    Field(String),
    Text(Vec<Piece>),
    Resume(String),
    Do,
    ParList(String, Option<i64>, Expr),
    Iter(String, Option<i64>, Expr),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Ready,
    /// Waiting for the answer to the `effect` line at this index.
    Resting(usize),
    /// Waiting for its `par`'s branches.
    Waiting,
    Done,
    Raised,
    Cancelled,
}

struct Run {
    name: String,
    party: Value,
    frames: Vec<Frame>,
    ctl: Option<Ctl>,
    parent: Option<RunId>,
    state: State,
    driver: Option<usize>,
    /// The folder of the working tree it is bound to, or empty.
    folder: String,
    /// Which branch of its parent's `par` it is.
    place: usize,
    /// What it has read since its previous `effect` line (11).
    reads: Sha256,
    /// The old run it follows while replaying (11).
    cursor: Option<replay::Cursor>,
    /// What began it, for a reload to begin its like (11).
    origin: Origin,
}

/// What began a run.
#[derive(Clone)]
enum Origin {
    /// A connection, by this Act.
    Act(Instance),
    /// A module's reaction to the Act of the `performed` line at `at`, the `k`-th run that module began for it.
    On { module: String, at: usize, k: usize },
    Branch,
}

/// An arm's `k` set aside: the frames of the run it was taken in, from the handler up; or none when the handler
/// stands in another run (then only a tail `resume` is possible, see `Across`).
struct Cont {
    frames: Option<Vec<Frame>>,
    resumed: bool,
    /// The arguments of the `Charge` it took, which goes on outward when the arm ends (10).
    charge: Option<Vec<Value>>,
}

pub struct Machine<N: Norms> {
    pub loaded: Loaded,
    pub norms: N,
    pub record: Vec<Line>,
    runs: Vec<Run>,
    conts: Vec<Cont>,
    drivers: Vec<Box<dyn Driver>>,
    /// Each Effect a driver carries out: that driver, and the dimensions it measures.
    carriers: BTreeMap<String, (usize, Vec<String>)>,
    /// The engine's clock, for `Now` (13).
    pub clock: Box<dyn FnMut() -> i64>,
    /// A working tree's head as it stands, from its folder: the commit, or the tree's hash if it has changes (11).
    pub head: Box<dyn FnMut(&str) -> String>,
    /// Put a working tree, from its folder, at a head (11).
    pub put: Box<dyn FnMut(&str, &str) -> Result<(), String>>,
    /// The past state the core reads now, by its count of committed steps (11).
    viewing: Option<usize>,
    /// The replayed runs whose tree is put back: one per run and its branches (11).
    live_trees: std::collections::BTreeSet<RunId>,
    /// The record being replayed exactly, whose answers every Effect takes (11).
    restoring: Option<Arc<[Line]>>,
    /// The `effect` lines of runs a reload replaced, unanswered, each with the replacement that may take it over (11).
    held: BTreeMap<usize, RunId>,
}

/// A fault of the machine or of an unchecked text: the step could not be taken at all.
pub type Fault = String;

impl<N: Norms> Machine<N> {
    /// A machine on a loaded text, a `loaded` line written; `Err` if two drivers carry one Effect or a reaction's goes unanswered (14).
    pub fn new(loaded: Loaded, mut norms: N, drivers: Vec<Box<dyn Driver>>) -> Result<Self, Fault> {
        norms.load(&loaded.norms)?;
        let mut carriers = BTreeMap::new();
        for (k, d) in drivers.iter().enumerate() {
            for (e, dims) in d.effects() {
                if let Some((j, _)) = carriers.insert(e.clone(), (k, dims)) {
                    let named = |i: usize| drivers[i].name().iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ");
                    return Err(format!("{e} is carried by two drivers, {} and {}: one Effect has one carrier", named(j), named(k)));
                }
            }
        }
        let hash = loaded.hash;
        let m = Machine { loaded, norms, record: vec![Line::Loaded { hash }], runs: Vec::new(), conts: Vec::new(), drivers, carriers, clock: Box::new(|| 0), head: Box::new(|_| String::new()), put: Box::new(|_, _| Ok(())), viewing: None, live_trees: Default::default(), restoring: None, held: Default::default() };
        match m.lost_reaction() {
            Some(why) => Err(why),
            None => Ok(m),
        }
    }
}

/// Who performs an Act or Event: a run, in the text of a module (none: a connection's Act); or a driver.
enum By {
    Run(RunId, Arc<str>),
    Driver(usize),
    /// A driver's Event as the record has it, replayed exactly: its source is not asked again (11).
    Record,
}

/// What a frame is to a search for a handler.
enum Probe {
    Handle(Vec<Arm>, Env, Arc<str>),
    Across(RunId, usize),
    Root(usize),
    Other,
}

enum Named {
    Value(Value),
    Act(Instance),
}

fn lit(l: &Lit) -> Value {
    match l {
        Lit::Int(n) => Value::Int(*n),
        Lit::Bool(b) => Value::Bool(*b),
        Lit::Unit => Value::Unit,
    }
}

fn e_of(at: crate::syntax::lex::At, e: E) -> Expr {
    Expr { at, e }
}
