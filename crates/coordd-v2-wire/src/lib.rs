//! The requests a connection sends and the replies it gets, one JSON object per line.

// Ten requests and a terminal's bytes. Every command a party has is an act its library declares, sent as `act`;
// the frame has no request of its own for any of them. Each request gets exactly one `done` or `no`, in the order
// the requests were sent; `line`, `bytes` and `delta` replies come between them.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

/// A request, as `{"do": "act", "name": "Submit", "args": [...]}`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "do", rename_all = "lowercase")]
pub enum Request {
    /// The record from line `from` on, then each line as it is written, for as long as the connection is open; its
    /// `done` comes before its first line.
    Watch { from: u64 },
    /// At most `n` lines before line `before`, oldest first.
    Older { before: u64, n: u64 },
    /// Perform the Act or Event `name` with `args`, as the connection's party.
    Act { name: String, args: Vec<Json> },
    /// A driver's answer to the `effect` line at `line`.
    Answer { line: u64, answer: Answer },
    /// The answer to `query` in the facts that hold now.
    Query { query: String },
    /// Every instance of Fact or Duty `name`, a full name, that holds now: `Foreach` over its fields, as sugar.
    Holding { name: String },
    /// The values of `expr`, a `Foreach x, ... : value When condition` in the facts that hold now, in the order of values.
    #[serde(rename = "inst_query")]
    InstQuery { expr: String },
    /// For each act instance in `target` (one, or a list), whether `actor` may perform it now, and why not.
    #[serde(rename = "enabled_for")]
    EnabledFor { actor: Json, target: Json },
    /// What of `types` (every Fact and Duty type when none) holds now, then a [`Reply::Delta`] after each change; a
    /// [`Reply::Reset`] in their place for a reader too far behind.
    Subscribe {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        types: Option<Vec<String>>,
    },
    /// Why instance `inst` holds, or an Act or Event is enabled, now: a tree of rules and premises; it changes no run's key.
    Explain { inst: String },
    /// `data` (base64) typed into terminal `session`; from the first of these on a connection, the session's output
    /// comes back as [`Reply::Bytes`].
    Bytes { session: String, data: String },
}

/// A driver's answer to an `effect` line: its value with what it cost by dimension (spec 10), or why it could not
/// be carried out, for which the machine performs `Fault` in the run.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum Answer {
    Value {
        value: Json,
        #[serde(default)]
        costs: Vec<(String, i64)>,
    },
    Failed {
        failed: String,
    },
}

/// A reply, as `{"is": "done", "value": ...}`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "is", rename_all = "lowercase")]
pub enum Reply {
    /// The line at place `at` of the record.
    Line { at: u64, line: Json },
    /// What the request gave. For `act` and `answer` the lines it wrote come as [`Reply::Line`] to every watcher.
    Done { value: Json },
    /// The request was not taken, and why. A refused act is not this: it is a line of the record.
    No { why: String },
    /// Output of terminal `session`, base64.
    Bytes { session: String, data: String },
    /// What moved in a subscription's types from change `since` to `seq`; one for all the changes a slow reader missed.
    Delta {
        since: u64,
        #[serde(flatten)]
        delta: StepDelta,
    },
    /// What of a subscription's types holds now, as its `done` gives it with `seq`: for a reader so far behind that the
    /// changes it missed are gone, which starts again from this; the deltas after it go on from its `seq`.
    Reset { value: Json },
}

/// What one change moved in the facts and duties that hold, each instance as the record writes it: the one delta
/// coordd-v2 works out per change, for every subscriber and projection alike.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct StepDelta {
    pub seq: u64,
    pub created_facts: Vec<Json>,
    pub terminated_facts: Vec<Json>,
    pub new_duties: Vec<Json>,
    pub terminated_duties: Vec<Json>,
    pub violations: Vec<Json>,
    /// Violated duties that still hold and are violated no more.
    #[serde(default)]
    pub cleared_violations: Vec<Json>,
}

impl StepDelta {
    /// Whether nothing moved.
    pub fn is_empty(&self) -> bool {
        self.created_facts.is_empty() && self.terminated_facts.is_empty() && self.new_duties.is_empty() && self.terminated_duties.is_empty() && self.violations.is_empty() && self.cleared_violations.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn each_request_and_reply_is_one_object_named_by_its_tag() {
        let reqs = [
            (Request::Watch { from: 3 }, json!({"do": "watch", "from": 3})),
            (Request::Older { before: 9, n: 2 }, json!({"do": "older", "before": 9, "n": 2})),
            (Request::Act { name: "Submit".into(), args: vec![json!("w1")] }, json!({"do": "act", "name": "Submit", "args": ["w1"]})),
            (Request::Answer { line: 4, answer: Answer::Value { value: json!(true), costs: vec![("tokens".into(), 120)] } }, json!({"do": "answer", "line": 4, "answer": {"value": true, "costs": [["tokens", 120]]}})),
            (Request::Answer { line: 4, answer: Answer::Failed { failed: "gone".into() } }, json!({"do": "answer", "line": 4, "answer": {"failed": "gone"}})),
            (Request::Query { query: "Holds(Green(c1))".into() }, json!({"do": "query", "query": "Holds(Green(c1))"})),
            (Request::Holding { name: "t.Green".into() }, json!({"do": "holding", "name": "t.Green"})),
            (Request::InstQuery { expr: "Foreach x : x When t.Green(x)".into() }, json!({"do": "inst_query", "expr": "Foreach x : x When t.Green(x)"})),
            (Request::EnabledFor { actor: json!("ann"), target: json!({"inst": "t.Go", "args": []}) }, json!({"do": "enabled_for", "actor": "ann", "target": {"inst": "t.Go", "args": []}})),
            (Request::Subscribe { types: Some(vec!["t.Green".into()]) }, json!({"do": "subscribe", "types": ["t.Green"]})),
            (Request::Subscribe { types: None }, json!({"do": "subscribe"})),
            (Request::Explain { inst: "Green(c1)".into() }, json!({"do": "explain", "inst": "Green(c1)"})),
            (Request::Bytes { session: "s".into(), data: "aGk=".into() }, json!({"do": "bytes", "session": "s", "data": "aGk="})),
        ];
        for (r, j) in reqs {
            assert_eq!(serde_json::to_value(&r).unwrap(), j);
            assert_eq!(serde_json::from_value::<Request>(j).unwrap(), r);
        }
        assert_eq!(serde_json::to_value(Reply::No { why: "x".into() }).unwrap(), json!({"is": "no", "why": "x"}));
        let d = Reply::Delta { since: 2, delta: StepDelta { seq: 4, created_facts: vec![json!("a")], ..StepDelta::default() } };
        let j = json!({"is": "delta", "seq": 4, "since": 2, "created_facts": ["a"], "terminated_facts": [], "new_duties": [], "terminated_duties": [], "violations": [], "cleared_violations": []});
        assert_eq!(serde_json::to_value(&d).unwrap(), j);
        assert_eq!(serde_json::from_value::<Reply>(j).unwrap(), d);
        let r = Reply::Reset { value: json!({"facts": {}, "seq": 9}) };
        assert_eq!(serde_json::to_value(&r).unwrap(), json!({"is": "reset", "value": {"facts": {}, "seq": 9}}));
        assert!(serde_json::from_value::<Request>(json!({"do": "send", "text": "x"})).is_err(), "no request but the eleven");
    }
}
