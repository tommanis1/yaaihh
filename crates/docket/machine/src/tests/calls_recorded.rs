//! Calls on the record (paper-research N-101 p392; the owner, N-221.p3: a ticket's state is what the record says
//! happened, whichever text was loaded then; a restart restores it from the record and never re-derives past steps
//! from today's text). A ticket's open line carries the calls its program names, and it runs and replays against
//! those, whatever the call files say later.

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The call as it was: its Ask is the designer's question to the owner (`asks Ask`), never taken.
const OLD_CALLS: &str = "call design(words: Text) -> Design(text: Text) | Ask(question: Text)
  task design
  asks Ask
  template
    Design {{ words }}.
";

/// The call as edited: Ask is an answer like any, taken by the program.
const NEW_CALLS: &str = "call design(words: Text) -> Design(text: Text) | Ask(question: Text)
  task design
  template
    Design {{ words }}.
";

const OLD_KIND: &str = "kind change
  form words: Text
  result Text
  pattern
    d <- step fresh design(words);
    match d {
      Design(text) => return text
    }
";

const NEW_KIND: &str = "kind change
  form words: Text
  result Text
  pattern
    d <- step fresh design(words);
    match d {
      Design(text) => return text
      Ask(question) => return question
    }
";

fn defs_of(calls: &str, kind: &str) -> Defs {
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = syntax::parse_calls("design.call", calls, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
    let (d, errors) = Defs::build(&[("change.kind".to_string(), kind.to_string())], calls, named, 200);
    assert!(errors.is_empty(), "{errors:?}");
    d
}

struct Run {
    m: Machine,
    log: Vec<Line>,
    clock: u32,
}

impl Run {
    fn go(&mut self, i: Input) -> Out {
        self.clock += 1;
        let out = self.m.apply(i, format!("2026-11-16T13:00:{:02}Z", self.clock));
        self.log.extend(out.lines.iter().cloned());
        out
    }
    fn open(&mut self) -> (u64, String) {
        self.go(Input::Open { kind: "change".into(), under: None, opener: "owner".into(), form: json!({"words": "drop short buffers"}), text: None, names: vec![], told: vec![] }).reply.unwrap();
        let t = *self.m.tickets.range(..HIDDEN).filter(|(_, x)| x.kind == "change").next_back().unwrap().0;
        let Status::Awaits { agent, .. } = &self.m.tickets[&t].status else { panic!("{:?}", self.m.tickets[&t].status) };
        let agent = agent.clone();
        for d in self.m.due(&agent) {
            self.go(Input::Given { t: d.t, to: agent.clone(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Loc { file: "t.jsonl".into(), offset: 1, len: 1 }) });
        }
        (t, agent)
    }
    fn answer(&mut self, t: u64, by: &str, head: Value) -> Out {
        self.go(Input::Answer { t, by: by.into(), head, text: Text::New("as said".into()) })
    }
    fn asks(&self) -> Vec<u64> {
        self.m.tickets.values().filter(|x| x.kind == "ask").map(|x| x.t).collect()
    }
}

/// The record's lines as written to the log and read back.
fn reread(log: &[Line]) -> Vec<Line> {
    log.iter().map(|l| serde_json::from_str(&serde_json::to_string(l).unwrap()).unwrap()).collect()
}

#[test]
fn a_ticket_runs_and_replays_against_the_calls_it_was_opened_with() {
    let mut r = Run { m: Machine::load(defs_of(OLD_CALLS, OLD_KIND)), log: Vec::new(), clock: 0 };
    let (old, d) = r.open();
    assert!(matches!(&r.log[0], Line::Open { calls, .. } if calls.get("design").is_some_and(|c| c.asks.as_deref() == Some("Ask"))), "{:?}", r.log[0]);
    // the designer's question: asked, not taken
    r.answer(old, &d, json!({"head": "Ask", "question": "drop or report?"})).reply.unwrap();
    assert_eq!(r.asks().len(), 1);
    // the call and the kind are edited and read again (coordd swaps the definitions, then records the reload)
    *r.m.defs.edit() = defs_of(NEW_CALLS, NEW_KIND);
    r.go(Input::Reload { by: "owner".into(), why: "Ask is an answer now".into(), commit: None, kinds: vec!["change".into()], calls: vec!["design".into()] }).reply.unwrap();
    // the running ticket keeps its call: a second question is still a question (refused while the first is open)
    let a = r.asks()[0];
    r.answer(a, "owner", json!({"head": "Answer", "text": "report"})).reply.unwrap();
    r.answer(old, &d, json!({"head": "Ask", "question": "and the logs?"})).reply.unwrap();
    assert_eq!(r.asks().len(), 2, "asked again, not taken");
    assert!(r.m.tickets[&old].is_open());
    // a ticket opened now runs the edited ones: its Ask is taken as its result
    let (new, d2) = r.open();
    r.answer(new, &d2, json!({"head": "Ask", "question": "which buffers?"})).reply.unwrap();
    assert!(matches!(&r.m.tickets[&new].status, Status::Done(v) if v == "which buffers?"), "{:?}", r.m.tickets[&new].status);
    // a restart with today's files: the record replays as it happened, and the tickets are as they were
    let lines = reread(&r.log);
    let mut m2 = Machine::load(defs_of(NEW_CALLS, NEW_KIND));
    assert_eq!(m2.replay(&lines), Vec::<String>::new());
    for t in r.m.tickets.keys() {
        assert_eq!(m2.view(*t), r.m.view(*t), "ticket {t}");
    }
    // the same record without its calls (a log from before them) re-derives the old ticket from today's call
    let mut m3 = Machine::load(defs_of(NEW_CALLS, NEW_KIND));
    assert!(!m3.replay(&super::without_calls(&lines)).is_empty(), "today's call would have taken the old ticket's Ask");
}

/// A child a running ticket opens records its own calls on its `opened` line, and replays against them.
#[test]
fn a_child_records_the_calls_it_was_opened_with() {
    let parent = "kind outer\n  form words: Text\n  result Text\n  pattern\n    r <- open change(words);\n    return r\n";
    let defs = |calls: &str, kind: &str| defs_of(calls, &format!("{kind}{parent}"));
    let mut r = Run { m: Machine::load(defs(OLD_CALLS, OLD_KIND)), log: Vec::new(), clock: 0 };
    r.go(Input::Open { kind: "outer".into(), under: None, opener: "owner".into(), form: json!({"words": "drop short buffers"}), text: None, names: vec![], told: vec![] }).reply.unwrap();
    let child = *r.m.tickets.range(..HIDDEN).filter(|(_, x)| x.kind == "change").next_back().unwrap().0;
    assert!(r.log.iter().any(|l| matches!(l, Line::Opened { t, calls, .. } if *t == child && calls.contains_key("design"))), "{:?}", r.log);
    let Status::Awaits { agent, .. } = r.m.tickets[&child].status.clone() else { panic!() };
    for d in r.m.due(&agent) {
        r.go(Input::Given { t: d.t, to: agent.clone(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Loc { file: "t.jsonl".into(), offset: 1, len: 1 }) });
    }
    r.answer(child, &agent, json!({"head": "Ask", "question": "drop or report?"})).reply.unwrap();
    assert_eq!(r.asks().len(), 1);
    let mut m2 = Machine::load(defs(NEW_CALLS, NEW_KIND));
    assert_eq!(m2.replay(&reread(&r.log)), Vec::<String>::new());
    for t in r.m.tickets.keys() {
        assert_eq!(m2.view(*t), r.m.view(*t), "ticket {t}");
    }
}
