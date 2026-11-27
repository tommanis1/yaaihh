//! The record: the nine kinds of line, in the order written, and nothing else (spec 8.1).

use super::value::{Instance, Value};

/// Why an Act or Event did not take effect (8.2): the step of 8.2 that refused it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Step 1: an Act's actor is not the run's party, or an Event is performed by text of another module.
    Source,
    /// Step 2: not enabled.
    Policy(String),
    /// Step 4: the closing rule (9).
    Closes(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    /// An Act or Event took effect; `party` is none for an Event a driver performed.
    Performed { act: Instance, party: Option<Value>, run: Option<String> },
    Refused { act: Instance, party: Option<Value>, run: Option<String>, why: Refusal },
    /// An Effect reached a driver; `head` is its run's tree's head then, and `key` its replay key (11).
    Effect { op: String, args: Vec<Value>, run: String, head: String, key: [u8; 32] },
    /// The driver's answer to the `Effect` line at `line`.
    Answered { line: usize, value: Value },
    Charged { run: String, dim: String, n: i64 },
    Began { run: String, by: Value },
    Ended { run: String, value: Value },
    Raised { run: String, effect: String, args: Vec<Value> },
    Cancelled { run: String, by: String },
    /// A replayed run's prefix ended: it did what old run `old` did in that run's lines before line `upto` (11).
    Prefix { run: String, old: String, upto: usize },
    /// The loaded text changed; its hash.
    Loaded { hash: [u8; 32] },
}

impl Line {
    /// The run the line is of, if it is of one.
    pub fn run(&self) -> Option<&str> {
        match self {
            Line::Performed { run, .. } | Line::Refused { run, .. } => run.as_deref(),
            Line::Effect { run, .. } | Line::Charged { run, .. } | Line::Began { run, .. } | Line::Ended { run, .. } | Line::Raised { run, .. } | Line::Cancelled { run, .. } | Line::Prefix { run, .. } => Some(run),
            Line::Answered { .. } | Line::Loaded { .. } => None,
        }
    }
}

impl std::fmt::Display for Line {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let p = |p: &Option<Value>| p.as_ref().map_or("a driver".to_string(), Value::to_string);
        let r = |r: &Option<String>| r.clone().unwrap_or_else(|| "-".into());
        let vs = |v: &[Value]| v.iter().map(Value::to_string).collect::<Vec<_>>().join(", ");
        let hex = |h: &[u8; 32]| h[..6].iter().map(|b| format!("{b:02x}")).collect::<String>();
        match self {
            Line::Performed { act, party, run } => write!(f, "performed({act}, {}, {})", p(party), r(run)),
            Line::Refused { act, party, run, why } => write!(f, "refused({act}, {}, {}, {why:?})", p(party), r(run)),
            Line::Effect { op, args, run, head, key } => write!(f, "effect({op}({}), {run}, {head:?}, {})", vs(args), hex(key)),
            Line::Answered { line, value } => write!(f, "answered({line}, {value})"),
            Line::Charged { run, dim, n } => write!(f, "charged({run}, {dim:?}, {n})"),
            Line::Began { run, by } => write!(f, "began({run}, {by})"),
            Line::Ended { run, value } => write!(f, "ended({run}, {value})"),
            Line::Raised { run, effect, args } => write!(f, "raised({run}, {effect}({}))", vs(args)),
            Line::Cancelled { run, by } => write!(f, "cancelled({run}, {by})"),
            Line::Prefix { run, old, upto } => write!(f, "prefix({run}, {old}, {upto})"),
            Line::Loaded { hash } => write!(f, "loaded({})", hex(hash)),
        }
    }
}
