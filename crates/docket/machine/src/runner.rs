//! The runner's side of the machine (N-101, docket-core5): what a step owed to the runner may answer beyond its
//! call's answers.

use crate::line::Raised;
use crate::types::Type;
use serde_json::{Map, Value, json};

/// The worker a step owed to the project's runner names (`step runner c(...)`).
pub const RUNNER: &str = "runner";

/// Whether the answer RAW, by the worker AGENT of a step whose answers are HEAD, is the runner's `Ended(by, why)`
/// to a call that does not declare it (runner-contract.md, "Agreed", c: `RUNNER end JOB --by WHO WHY` ends the job and
/// its step answers Ended). It is then raised, untracked as Silent is, and not refused: no call file changes for it.
/// A call that declares `Ended` takes it as its answer.
pub fn ends(agent: &str, raw: &Value, head: &Type) -> bool {
    agent == RUNNER && raw.get("head").and_then(Value::as_str) == Some("Ended") && head.constructor("Ended").is_none()
}

/// The exception `Ended {by, why}` the runner's answer RAW raises.
pub fn ended(raw: &Value) -> Raised {
    let field = |k: &str| raw.get(k).cloned().unwrap_or(Value::Null);
    Raised { exc: "Ended".into(), value: json!({"head": "Ended", "by": field("by"), "why": field("why")}) }
}

/// An act given as the event `act` (`{by, is, text, ...}`, the runner's and coordd's way): its agent, what it is, its
/// text, and its other fields (`ticket` left out: the machine finds it).
pub fn act_of_event(value: &Value) -> (String, String, String, Map<String, Value>) {
    let mut more = value.as_object().cloned().unwrap_or_default();
    let take = |m: &mut Map<String, Value>, k: &str| m.remove(k).map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string)).unwrap_or_default();
    let agent = take(&mut more, "by");
    let is = take(&mut more, "is");
    let text = take(&mut more, "text");
    more.remove("ticket");
    more.remove("agent");
    (agent, is, text, more)
}

/// The value a handler of `act` gets: `{by, is, text, ticket, ...more}`.
pub fn act_value(agent: &str, is: &str, text: &str, ticket: Option<String>, more: &Map<String, Value>) -> Value {
    let mut v = more.clone();
    v.insert("by".into(), json!(agent));
    v.insert("is".into(), json!(is));
    v.insert("text".into(), json!(text));
    if let Some(id) = ticket {
        v.insert("ticket".into(), json!(id));
    }
    Value::Object(v)
}

/// The runner's events the record keeps whether or not a handler takes them (N-101 post 27, D and G): what it sees
/// while it holds a step (`outcome`), its jobs' start, measures and end (`job_measure`: the runner's answer to
/// `jobs`, one line per running job, runner-v2 1e7c2c5), `jobs` (coordd's own copy of that answer, in records
/// written before it), and `step_refused` (a step the runner refused because it did not come from the program
/// through coordd, N-169). On the ticket VALUE's `ticket` names when there is one; a job of an agent's own check, or
/// of nobody's, on none.
pub const RECORDED: [&str; 6] = ["outcome", "job_start", "job_measure", "job_end", "jobs", "step_refused"];

/// The post to the lead on ticket ID that the runner refused the step VALUE (`step_refused`: `call`, `by` the user
/// who ran it, `agent` the agent whose cgroup it ran in, if any, `why`), N-169: who tried what, and the runner's why.
pub fn refused_text(id: &str, value: &Value) -> String {
    let field = |k: &str| value.get(k).and_then(Value::as_str).filter(|s| !s.is_empty());
    let who = field("agent").or(field("by")).unwrap_or("someone");
    let call = field("call").unwrap_or("a step");
    let why = field("why").unwrap_or("it did not come from the program").trim_end_matches('.');
    format!("{who} ran the runner's {call} for {id} itself; the runner refused it: {why}.")
}

/// Whether event NAME is one the record keeps though nothing takes it.
pub fn recorded(name: &str) -> bool {
    RECORDED.contains(&name)
}

/// A constructor of CS written as text S (`SetBound(runtime, run-seconds, 12)`; `Spare(r1)`; `Rest`): the value, its
/// arguments by the constructor's fields in order and coerced to their types (the last field takes what is left, commas
/// and all); None when S names no constructor of CS.
pub fn constructor_text(cs: &[(String, Vec<(String, Type)>)], s: &str) -> Option<Result<Value, String>> {
    let s = s.trim();
    let (head, args) = match s.split_once('(') {
        Some((h, rest)) => (h.trim(), Some(rest.strip_suffix(')')?)),
        None => (s, None),
    };
    let (_, fs) = cs.iter().find(|(n, _)| n == head)?;
    let mut parts: Vec<String> = match args {
        Some(a) if !a.trim().is_empty() => a.splitn(fs.len().max(1), ',').map(|p| p.trim().to_string()).collect(),
        _ => Vec::new(),
    };
    if parts.len() != fs.len() {
        return Some(Err(format!("{head} takes {} ({}), not {}", fs.len(), fs.iter().map(|(f, _)| f.as_str()).collect::<Vec<_>>().join(", "), parts.len())));
    }
    let mut o = Map::new();
    o.insert("head".into(), json!(head));
    for ((f, t), p) in fs.iter().zip(parts.drain(..)) {
        match t.coerce(&json!(p)) {
            Ok(v) => o.insert(f.clone(), v),
            Err(e) => return Some(Err(format!("{head}.{f}: {e}"))),
        };
    }
    Some(Ok(Value::Object(o)))
}
