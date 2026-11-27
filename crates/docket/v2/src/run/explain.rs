//! Justification trees (Pontelli, Son and El-Khatib, TPLP 2009) for instances and refused steps, worked out only when asked.

use super::value::{Env, Instance};

/// A place in the loaded text: its module, and the line and column there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub module: String,
    pub line: u32,
    pub col: u32,
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.module, self.line, self.col)
    }
}

/// Why an instance's truth is what it is: a finite tree whose leaves are stated facts, steps and values.
#[derive(Clone, Debug, PartialEq)]
pub enum Explain {
    /// It holds: stated, or by a rule whose premises are met.
    True { inst: Instance, by: Holds },
    /// It does not hold: ended by a step (over what `overridden` would give), or each rule fails.
    False { inst: Instance, ended: Option<Done>, overridden: Option<Box<Holds>>, rules: Vec<Rule> },
    /// It is already being explained on this path: its truth, not explained again here.
    OnPath { inst: Instance, holds: bool },
    /// The well-founded reading leaves it unknown: it reads itself under `Not` through `cycle`, whose last link leads
    /// back to `inst`; `cycle` is empty when it is unknown only by reading an instance that is.
    Unknown { inst: Instance, cycle: Vec<Link> },
}

impl Explain {
    /// The instance explained.
    pub fn inst(&self) -> &Instance {
        match self {
            Explain::True { inst, .. } | Explain::False { inst, .. } | Explain::OnPath { inst, .. } | Explain::Unknown { inst, .. } => inst,
        }
    }

    /// Its truth: `None` when it has no single answer.
    pub fn holds(&self) -> Option<bool> {
        match self {
            Explain::True { .. } => Some(true),
            Explain::False { .. } => Some(false),
            Explain::OnPath { holds, .. } => Some(*holds),
            Explain::Unknown { .. } => None,
        }
    }
}

/// What makes an instance hold.
#[derive(Clone, Debug, PartialEq)]
pub enum Holds {
    /// Stored by a committed step.
    Stated(Done),
    /// Stated by its module (`+i.`) at `at`.
    Postulated { at: Span },
    /// A rule whose premises are all met.
    Rule(Rule),
    /// No proof found in `tries` bindings per rule that does not stand on the instance itself, or, `cut`, that the budget let finish.
    Unproved { tries: usize, cut: bool },
}

/// A committed step: its number from 1 (0 for one before any this state saw) and the Act or Event it performed.
#[derive(Clone, Debug, PartialEq)]
pub struct Done {
    pub step: usize,
    pub act: Option<Instance>,
}

/// A rule read for one instance, with its place, text and bindings: all its premises if it holds, else the failing ones.
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub at: Span,
    pub text: String,
    pub env: Env,
    pub premises: Vec<Premise>,
    /// The explanation's budget ran out before all its premises were asked.
    pub cut: bool,
}

/// One premise as written, its bindings and truth (false where it turns on an unknown instance, as it meets no
/// condition), the parts that decide it, and why each instance it reads holds or not.
#[derive(Clone, Debug, PartialEq)]
pub struct Premise {
    pub at: Span,
    pub text: String,
    pub env: Env,
    pub holds: bool,
    /// The explanation's budget ran out here: its parts and instances were not looked at.
    pub cut: bool,
    pub parts: Vec<Premise>,
    pub under: Vec<Explain>,
}

/// One link of a cycle: `inst` reads the next link's instance by rule `at` (`text`), under `Not` when `negated`.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub inst: Instance,
    pub at: Span,
    pub text: String,
    pub negated: bool,
}

/// Why Act or Event `act` is not enabled: its failing `Conditioned by` conjuncts and the syncs that are not enabled.
#[derive(Clone, Debug, PartialEq)]
pub struct WhyFailed {
    pub act: Instance,
    pub failing: Vec<Premise>,
    pub syncs: Vec<WhyFailed>,
    /// It is explained once already in this answer, where another act syncs with it.
    pub again: bool,
    /// The explanation's budget ran out before all its conditions and syncs were asked.
    pub cut: bool,
}

impl WhyFailed {
    /// Whether it says the act is not enabled, or may: its budget ran out.
    pub fn refuses(&self) -> bool {
        self.again || self.cut || !self.failing.is_empty() || !self.syncs.is_empty()
    }
}
