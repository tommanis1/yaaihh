//! A question to the owner looked at first by the `during` around the asker's step (ask_minded.rs; paper-research
//! N-101 p384): held while the look runs, asked when it rests, kept from the owner when it stops the asker.

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const CALLS: &str = "call design(words: Text) -> Design(text: Text) | Ask(question: Text)
  task design
  asks Ask
  template
    Design {{ words }}.
";

/// The designer's step under a minder that looks at acts WHICH (`u.is == WHICH`); HANDLER, a `when ask` of the kind.
fn kinds(which: &str, handler: &str) -> String {
    format!(
        "kind change
  form words: Text
  result Text
  pattern
    let agent d in {{
      let agent m in {{
        r <- during {{
          step d design(words)
        }} when act(u) where u.by == d and u.is == \"{which}\" do {{
          k <- step m \"Is this for the owner: {{{{ u.text }}}}\" -> Rest | Say(text: Text);
          match k {{
            Rest => return none
            Say(text) => stop d; post text naming [d]
          }}
        }};
        match r {{
          Design(text) => return text
        }}
      }}
    }}
{handler}"
    )
}

fn defs_of(kinds: &str) -> Defs {
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = syntax::parse_calls("design.call", CALLS, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
    let (d, errors) = Defs::build(&[("change.kind".to_string(), kinds.to_string())], calls, named, 200);
    assert!(errors.is_empty(), "{errors:?}");
    d
}

struct Run {
    m: Machine,
    kinds: String,
    log: Vec<Line>,
    clock: u32,
}

impl Run {
    fn new(kinds: String) -> Run {
        let mut r = Run { m: Machine::load(defs_of(&kinds)), kinds, log: Vec::new(), clock: 0 };
        r.go(Input::Open { kind: "change".into(), under: None, opener: "owner".into(), form: json!({"words": "drop short buffers"}), text: None, names: vec![], told: vec![] }).reply.unwrap();
        let d = r.designer();
        r.give(&d);
        r
    }
    fn go(&mut self, i: Input) -> Out {
        self.clock += 1;
        let out = self.m.apply(i, format!("2026-11-16T12:00:{:02}Z", self.clock));
        self.log.extend(out.lines.iter().cloned());
        out
    }
    fn designer(&self) -> String {
        match &self.m.tickets[&200].status {
            Status::Awaits { agent, .. } => agent.clone(),
            s => panic!("{s:?}"),
        }
    }
    fn give(&mut self, agent: &str) -> Vec<Delivery> {
        let due = self.m.due(agent);
        for d in &due {
            self.go(Input::Given { t: d.t, to: agent.into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Loc { file: "t.jsonl".into(), offset: 1, len: 1 }) });
        }
        due
    }
    fn answer(&mut self, t: u64, by: &str, head: Value) -> Out {
        self.go(Input::Answer { t, by: by.into(), head, text: Text::New("as said".into()) })
    }
    /// The look running under the change: its ticket and the minder its step is owed to.
    fn look(&self) -> (u64, String) {
        let (t, x) = self.m.tickets.range(HIDDEN..).rev().find(|(_, x)| x.is_open() && x.handling.as_deref() == Some("act")).expect("a look runs");
        match &x.status {
            Status::Awaits { agent, .. } => (*t, agent.clone()),
            s => panic!("{s:?}"),
        }
    }
    fn asks(&self) -> Vec<u64> {
        self.m.tickets.values().filter(|x| x.kind == "ask").map(|x| x.t).collect()
    }
    fn replays(&self) {
        let text: Vec<String> = self.log.iter().map(|l| serde_json::to_string(l).unwrap()).collect();
        let lines: Vec<Line> = text.iter().map(|s| serde_json::from_str(s).unwrap()).collect();
        let mut m2 = Machine::load(defs_of(&self.kinds));
        assert_eq!(m2.replay(&lines), Vec::<String>::new());
        for t in self.m.tickets.keys() {
            assert_eq!(m2.view(*t), self.m.view(*t), "ticket {t}");
        }
    }
}

#[test]
fn a_question_is_looked_at_first_and_asked_when_the_look_rests() {
    let mut r = Run::new(kinds("ask", ""));
    let d = r.designer();
    r.answer(200, &d, json!({"head": "Ask", "question": "drop or report?"})).reply.unwrap();
    // held, not asked: no ask ticket; a look under the change, holding it, its step the minder's
    assert!(r.asks().is_empty(), "{:?}", r.asks());
    let (look, m) = r.look();
    assert_eq!(r.m.tickets[&look].held_ask.as_ref().map(|q| q.ask.text.as_str()), Some("drop or report?"));
    assert!(r.m.due(&m).iter().any(|x| x.t == look), "{:?}", r.m.due(&m));
    // meanwhile its turn's end does not answer the step, and a second question is refused
    r.go(Input::TurnEnded { agent: d.clone(), delivery: None, text: Loc { file: "t.jsonl".into(), offset: 2, len: 1 }, later: vec![], stood: None, go: None });
    assert!(matches!(&r.m.tickets[&200].status, Status::Awaits { agent, forming: None, .. } if *agent == d), "{:?}", r.m.tickets[&200].status);
    let out = r.answer(200, &d, json!({"head": "Ask", "question": "and the logs?"}));
    assert!(out.lines.iter().any(|l| matches!(l, Line::Refused { why, .. } if why.contains("not answered yet"))), "{:?}", out.lines);
    // the look rests: the question is asked, the owner answers, and it goes back to the designer
    r.give(&m);
    r.answer(look, &m, json!({"head": "Rest"})).reply.unwrap();
    let a = *r.asks().first().expect("asked");
    assert_eq!(r.m.tickets[&a].ask.as_ref().map(|q| q.text.as_str()), Some("drop or report?"));
    let ViewStatus::Awaits { agent, .. } = r.m.view(a).unwrap().status else { panic!() };
    assert_eq!(agent, "owner");
    r.answer(a, "owner", json!({"head": "Answer", "text": "report"})).reply.unwrap();
    assert!(r.give(&d).iter().any(|x| x.id == "C-200.s1.again"));
    r.replays();
}

#[test]
fn a_look_that_stops_the_asker_keeps_the_question_from_the_owner() {
    let mut r = Run::new(kinds("ask", ""));
    let d = r.designer();
    r.answer(200, &d, json!({"head": "Ask", "question": "is a list a list?"})).reply.unwrap();
    let (look, m) = r.look();
    r.give(&m);
    let out = r.answer(look, &m, json!({"head": "Say", "text": "answered in the design: yes"}));
    assert!(out.effects.contains(&Effect::Interrupt { agent: d.clone() }), "{:?}", out.effects);
    // never asked; the minder's words are the designer's, and its step stays owed
    assert!(r.asks().is_empty(), "{:?}", r.asks());
    assert!(r.m.tickets[&200].posts.iter().any(|p| p.names.contains(&d) && matches!(&p.text, Text::New(t) if t == "answered in the design: yes")), "{:?}", r.m.tickets[&200].posts);
    assert!(matches!(&r.m.tickets[&200].status, Status::Awaits { agent, .. } if *agent == d));
    // the step is free again: a later question is looked at in its turn
    r.answer(200, &d, json!({"head": "Ask", "question": "drop or report?"})).reply.unwrap();
    assert!(r.m.tickets[&r.look().0].held_ask.is_some());
    r.replays();
}

#[test]
fn a_question_the_minder_does_not_look_at_is_asked_at_once() {
    let mut r = Run::new(kinds("check", ""));
    let d = r.designer();
    r.answer(200, &d, json!({"head": "Ask", "question": "drop or report?"})).reply.unwrap();
    assert_eq!(r.asks().len(), 1);
    assert!(r.m.tickets.range(HIDDEN..).all(|(_, x)| x.handling.as_deref() != Some("act")));
    r.replays();
}

#[test]
fn a_waiting_coord_ask_stopped_by_the_look_is_told_it_was_not_sent() {
    let handler = "  when ask(q) do\n    a <- step owner \"{{ q.text }}\" -> Answer(text: Text);\n    reply q a\n";
    let mut r = Run::new(kinds("ask", handler));
    r.m.defs.edit().block_ask = true;
    let d = r.designer();
    let out = r.go(Input::Ask { by: d.clone(), text: "may I skip the tests?".into(), context: None });
    assert!(out.reply.as_ref().is_ok_and(|s| s.starts_with("Held")), "{:?}", out.reply);
    // one at a time: a second waiting ask is refused while the first is held
    let out = r.go(Input::Ask { by: d.clone(), text: "and the docs?".into(), context: None });
    assert!(out.reply.as_ref().is_err_and(|e| e.contains("one at a time")), "{:?}", out.reply);
    let (look, m) = r.look();
    r.give(&m);
    let out = r.answer(look, &m, json!({"head": "Say", "text": "no: the plan says they run"}));
    assert!(out.effects.iter().any(|e| matches!(e, Effect::Answered { agent, text } if *agent == d && text.starts_with("Not sent to the owner"))), "{:?}", out.effects);
    assert!(r.asks().is_empty());
    // a later one the look rests on is asked, still waiting
    r.go(Input::Ask { by: d.clone(), text: "skip the docs?".into(), context: None }).reply.unwrap();
    let (look, m) = r.look();
    r.give(&m);
    r.answer(look, &m, json!({"head": "Rest"})).reply.unwrap();
    let a = *r.asks().first().expect("asked");
    assert!(r.m.tickets[&a].ask.as_ref().is_some_and(|q| q.waiting && q.text == "skip the docs?"));
    r.replays();
}
