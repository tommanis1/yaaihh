//! His messages held to go later (`Request::Schedule`, [`crate::SCHEDULE_PROTOCOL`]): a post or a message he wrote,
//! kept by coordd in a file of its own until he sends it, its time comes, or a thread it waits on closes.

use crate::Request;
use serde::{Deserialize, Serialize};

/// When a held message goes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "is", rename_all = "snake_case")]
pub enum When {
    /// Only when he presses Send now.
    Hold,
    /// At AT (RFC 3339, UTC) or the first time coordd looks after it.
    At { at: String },
    /// Once thread T (`N-12`) is closed.
    Closes { t: String },
}

/// One held message: its REQ as its Send would have made it, where its box was (PLACE, the box's place: an agent's
/// name or `thread:N-12`), when he held it (MADE, RFC 3339), and why it did not go when it was due (FAILED).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Held {
    pub id: u64,
    pub made: String,
    pub when: When,
    pub place: String,
    pub req: Request,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
}

impl Held {
    /// The words it sends.
    pub fn text(&self) -> &str {
        match &self.req {
            Request::Post { text, .. } | Request::Say { text, .. } => text,
            Request::Msg { body, .. } => body,
            _ => "",
        }
    }
}

/// What he does with his held messages. Each is answered with the whole list as JSON (`Vec<Held>`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum Sched {
    /// Hold REQ (a `Post`, `Msg` or `Say` of his) from the box at PLACE until WHEN.
    Add { req: Box<Request>, when: When, place: String },
    List,
    /// Send ID now.
    Send { id: u64 },
    /// Drop ID unsent.
    Drop { id: u64 },
}

/// Whether REQ is one a box of his sends, so it may be held.
pub fn holdable(req: &Request) -> bool {
    matches!(req, Request::Post { .. } | Request::Msg { .. } | Request::Say { .. })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A held post reads back as it was written, with the words it sends.
    #[test]
    fn a_held_post_reads_back_whole() {
        let req = Request::Post { kind: None, t: "N-12".into(), text: "later".into(), to: None, answers: None, wait: false, also: Vec::new(), typed: None, split_from: None, on: Vec::new(), comments: Vec::new() };
        let h = Held { id: 3, made: "2026-11-27T10:00:00Z".into(), when: When::Closes { t: "N-9".into() }, place: "thread:N-12".into(), req, failed: None };
        let back: Held = serde_json::from_str(&serde_json::to_string(&h).unwrap()).unwrap();
        assert_eq!((back.id, back.when.clone(), back.text()), (3, When::Closes { t: "N-9".into() }, "later"));
        let add = serde_json::to_value(Request::Schedule { act: Sched::Add { req: Box::new(back.req), when: When::Hold, place: "w1".into() } }).unwrap();
        assert_eq!((add["op"].as_str(), add["act"]["do"].as_str(), add["act"]["when"]["is"].as_str()), (Some("schedule"), Some("add"), Some("hold")));
        assert!(matches!(serde_json::from_value::<Request>(add), Ok(Request::Schedule { act: Sched::Add { .. } })));
    }
}
