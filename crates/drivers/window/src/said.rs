//! What the seated party's window says on the driver's socket, one JSON line each, and the value a decision answers.

use docket_v2::run::value::Value;
use serde::Deserialize;

/// One line from the window.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Said {
    /// Words put on `on`, said to each of `to` (none: to nobody), written at `at` seconds since 1970.
    Words { text: String, on: String, to: Vec<String>, at: i64 },
    /// The answer to the question carried at `line`.
    Decision { line: u64, answer: Answer },
}

/// Held words passed, as left or changed, or not and why; a spend let go on under a new limit, or not and why.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    Approved(String),
    Rejected(String),
    Granted(i64),
    Denied(String),
}

impl Answer {
    /// Its value for the Effect `op` its line asks; Err when it answers another Effect.
    pub fn value(&self, op: &str) -> Result<Value, String> {
        let (of, name, arg) = match self {
            Answer::Approved(text) => (crate::APPROVAL, "std.control.Approved", Value::text(text)),
            Answer::Rejected(why) => (crate::APPROVAL, "std.control.Rejected", Value::text(why)),
            Answer::Granted(limit) => (crate::BUDGET, "std.budgets.Granted", Value::Int(*limit)),
            Answer::Denied(why) => (crate::BUDGET, "std.budgets.Denied", Value::text(why)),
        };
        if of != op {
            return Err(format!("{} answers {of}, and that line asks {op}", name.rsplit('.').next().unwrap_or(name)));
        }
        Ok(Value::ctor(name, vec![arg]))
    }
}
