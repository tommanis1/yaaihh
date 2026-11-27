//! The seam to the normative stratum (spec 1, 8.2 steps 2, 3, 5): the machine asks, an eFLINT evaluator answers.
//! The machine never computes what holds; the evaluator never sees a program, a handler or a run.

use super::explain::{Explain, WhyFailed};
use super::value::{Env, Instance, Value};
use crate::syntax::tree::{Decl, Expr};

/// A normative declaration as loaded: every name in it qualified by its module (`gate.a.Submitted`), so that the
/// evaluator needs no scope. Variables stay as written.
#[derive(Clone, Debug, PartialEq)]
pub struct NormDecl {
    pub module: String,
    pub decl: Decl,
}

/// What a step would do, computed in Σ and not yet committed (8.2 step 3): the instances it creates and ends, its
/// syncs' included. The evaluator reads it again at `commit`; the machine reads only these lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub act: Instance,
    pub created: Vec<Instance>,
    pub terminated: Vec<Instance>,
}

/// What commits moved since last asked: instances in and out, types to read whole, duties reached; `all` = read every type.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Moved {
    pub all: bool,
    pub made: Vec<Instance>,
    pub ended: Vec<Instance>,
    pub whole: Vec<String>,
    pub duties: Vec<(Instance, bool)>,
}

/// Why a step is refused by the normative core (8.2 steps 2 and 4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Why {
    /// Not enabled: a condition fails, or an instance it syncs with is disabled.
    Policy(String),
    /// The closing rule (9): the step would end or clear a duty its actor holds.
    Closes(String),
}

/// The normative core. Every call reads or changes Σ, the stored facts closed under the loaded rules.
pub trait Norms {
    /// The loaded text's normative part, whole: Facts, Acts, Events, Duties with their clauses, `Extend`s, and the
    /// modules' own instances (5.5). A reload replaces the rules and keeps Σ's stored facts.
    fn load(&mut self, decls: &[NormDecl]) -> Result<(), String>;
    /// Whether instance `i` holds in Σ.
    fn holds(&mut self, i: &Instance) -> bool;
    /// Whether a condition with every variable bound by `env` holds in Σ: facts, `Not`, `Exists`, `Forall`, `Count`,
    /// comparisons, arithmetic and the host table (6.3), as written.
    fn cond(&mut self, c: &Expr, env: &Env) -> Result<bool, String>;
    /// Every way to bind the free variables of `c` (those `env` does not bind) so that it holds, in the order of
    /// values (6.2): for reactions (8.7), a `where` that binds by enumeration, and `Foreach`.
    fn each(&mut self, c: &Expr, env: &Env) -> Result<Vec<Env>, String>;
    /// Steps 2 to 4 of 8.2 for Act or Event `act` performed by `party` (none for an Event from a driver): what it
    /// would do, or why it is refused. Σ is unchanged either way.
    fn step(&mut self, act: &Instance, party: Option<&Value>) -> Result<Step, Why>;
    /// Step 5 of 8.2: Σ becomes Σ′. Refused while a past state is viewed.
    fn commit(&mut self, step: Step);
    /// Read Σ as it stood after the first `k` committed steps, or as it stands with `None` (11): the facts those
    /// steps stored, closed under the rules now loaded.
    fn view(&mut self, k: Option<usize>);
    /// What was read since the last call, each read with its answer, hashed with the rules of the types read and
    /// the host table's version: `state_hash` of a replay key (11). Zeros when nothing was read.
    fn reads(&mut self) -> [u8; 32];
    /// What moved since this was last asked ([`Moved`]); the first ask starts the count and answers `all`.
    fn moved(&mut self) -> Moved {
        Moved { all: true, ..Moved::default() }
    }
    /// Stop counting what moves, until `moved` is asked again.
    fn unwatch(&mut self) {}
    /// From now on `each` refuses once it has tried more than STEPS bindings (none: no cap), counts within it too.
    fn cap(&mut self, _steps: Option<u64>) {}
    /// Why instance `i` holds, does not, or has no single answer in Σ now; read as no run reads, so no state hash holds it.
    fn explain(&mut self, i: &Instance) -> Result<Explain, String> {
        Err(format!("this core gives no reasons: {i}"))
    }
    /// Why Act or Event `act` is not enabled in Σ as it stands, read as `explain` reads.
    fn why_refused(&mut self, act: &Instance) -> Result<WhyFailed, String> {
        Err(format!("this core gives no reasons: {act}"))
    }
}
