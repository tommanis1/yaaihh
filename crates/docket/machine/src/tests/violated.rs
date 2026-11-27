//! A duty broken at a step's answer: `Violated` (N-101 post 57, point 2; docket-core's five changes, N-149.p2). The
//! policy is put each agent's answer before it is taken (`asked`/`decided`, question `violated`); [] takes it, a duty
//! broken raises Violated at the step.

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const KINDS: &str = "\
kind change
  letter C
  form task: Text
  result Text
  pattern
    let agent w in {
      try {
        r <- step w \"Do {{ task }}\" -> Done(c: Text);
        match r { Done(c) => return c }
      } handle { Violated(v) => return \"broken\" }
    }

kind watched
  letter W
  form task: Text
  result Text
  pattern
    let agent w in {
      r <- during { step w \"Do {{ task }}\" -> Done(c: Text) } when act(u) where u.by == w do { stop w };
      return task
    }

kind mine
  letter M
  form task: Text
  result Text
  pattern
    r <- step owner \"Do {{ task }}\" -> Done(c: Text);
    match r { Done(c) => return c }
";

/// The owner's step in a try that handles Violated: never raised there.
const OWNERS: &str = "\
kind owners
  letter O
  form task: Text
  result Text
  pattern
    try {
      r <- step owner \"Do {{ task }}\" -> Done(c: Text);
      match r { Done(c) => return c }
    } handle { Violated(v) => return \"broken\" }
";

const W: &str = "f200-1";

fn defs(kinds: &str, duties: bool) -> (Defs, Vec<String>) {
    Defs::build_with(&[("v.kind".to_string(), kinds.to_string())], BTreeMap::new(), BTreeMap::new(), 200, duties)
}

struct Run {
    m: Machine,
    log: Vec<Line>,
    clock: u32,
}

impl Run {
    fn new(kind: &str) -> Run {
        let (d, errors) = defs(KINDS, true);
        assert!(errors.is_empty(), "{errors:#?}");
        let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
        r.go(Input::Open { kind: kind.into(), under: None, opener: "owner".into(), form: json!({"task": "drop short buffers"}), text: None, names: vec![], told: vec![] }).reply.unwrap();
        r
    }
    fn go(&mut self, i: Input) -> Out {
        self.clock += 1;
        let out = self.m.apply(i, format!("2026-11-15T22:00:{:02}Z", self.clock));
        self.log.extend(out.lines.iter().cloned());
        out
    }
    fn give(&mut self, agent: &str) -> Vec<Delivery> {
        let due = self.m.due(agent);
        for d in &due {
            self.go(Input::Given { t: d.t, to: agent.into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Loc { file: "t.jsonl".into(), offset: 1, len: 1 }) });
        }
        due
    }
    fn answer(&mut self, by: &str, head: Value) -> Out {
        self.go(Input::Answer { t: 200, by: by.into(), head, text: Text::New("as said".into()) })
    }
    fn decided(&mut self, answer: Value) -> Out {
        self.go(Input::Decided { t: 200, step: 1, question: "violated".into(), answer, provenance: json!({"test": true}) })
    }
    fn asking(&self) -> bool {
        matches!(&self.m.tickets[&200].status, Status::Asking { role, .. } if role == "violated")
    }
    fn replays(&self) {
        let text: Vec<String> = self.log.iter().map(|l| serde_json::to_string(l).unwrap()).collect();
        let lines: Vec<Line> = text.iter().map(|s| serde_json::from_str(s).unwrap()).collect();
        let mut m2 = Machine::load(defs(KINDS, true).0);
        assert_eq!(m2.replay(&lines), Vec::<String>::new());
        for t in self.m.tickets.keys() {
            assert_eq!(m2.view(*t), self.m.view(*t), "ticket {t}");
        }
        assert_eq!(m2.due(W), self.m.due(W));
    }
}

#[test]
fn an_answer_that_breaks_a_duty_raises_violated_at_the_step() {
    let mut r = Run::new("change");
    r.give(W);
    let out = r.answer(W, json!({"head": "Done", "c": "abc123"}));
    // put to the policy, not taken
    assert!(out.lines.iter().any(|l| matches!(l, Line::Asked { t: 200, step: 1, question, role, .. } if question == "violated" && role == W)), "{:?}", out.lines);
    let q = out.effects.iter().find_map(|e| if let Effect::Ask { question: Question::Violated { agent, call, answered }, .. } = e { Some((agent.clone(), call.clone(), answered.clone())) } else { None });
    let (agent, _, answered) = q.unwrap_or_else(|| panic!("{:?}", out.effects));
    assert_eq!((agent.as_str(), answered), (W, json!({"head": "Done", "c": "abc123"})));
    assert!(r.asking());
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::End { .. })), "{:?}", out.effects);
    // a second answer while the question is open is not taken
    let again = r.answer(W, json!({"head": "Done", "c": "def456"}));
    assert!(again.reply.is_err() || again.lines.iter().any(|l| matches!(l, Line::Refused { .. })), "{again:?}");
    assert!(r.asking());
    // a duty broken: the answer posted, Violated raised at the step and handled there
    let out = r.decided(json!([{"duty": "hands-in", "holder": W, "claimant": "owner", "work": "C-200"}]));
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert_eq!(r.m.view(200).unwrap().status, ViewStatus::Done { result: json!("broken") });
    assert!(r.m.tickets[&200].posts.iter().any(|p| p.by == W && p.step == Some(1)));
    // nothing owed is left for the agent: no `.again` notice of the step
    let due = r.give(W);
    assert!(!due.iter().any(|d| d.id.ends_with(".again")), "{due:?}");
    r.replays();
}

#[test]
fn an_answer_that_breaks_nothing_is_taken() {
    let mut r = Run::new("change");
    r.give(W);
    r.answer(W, json!({"head": "Done", "c": "abc123"}));
    let out = r.decided(json!([]));
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert_eq!(r.m.view(200).unwrap().status, ViewStatus::Done { result: json!("abc123") });
    assert!(!r.give(W).iter().any(|d| d.id.ends_with(".again")));
    // a decided line that does not fit, or for no open question, is refused
    assert!(r.decided(json!([])).reply.is_err());
    r.replays();
}

#[test]
fn an_answer_not_shaped_as_duties_is_refused_and_the_question_stays() {
    let mut r = Run::new("change");
    r.give(W);
    r.answer(W, json!({"head": "Done", "c": "abc123"}));
    assert!(r.decided(json!({"duty": "hands-in"})).reply.is_err());
    assert!(r.asking());
    r.decided(json!([])).reply.unwrap();
    r.replays();
}

#[test]
fn the_owners_answer_is_not_put_to_the_policy() {
    let mut r = Run::new("mine");
    let out = r.answer("owner", json!({"head": "Done", "c": "abc123"}));
    assert!(!out.lines.iter().any(|l| matches!(l, Line::Asked { .. })), "{:?}", out.lines);
    assert_eq!(r.m.view(200).unwrap().status, ViewStatus::Done { result: json!("abc123") });
    r.replays();
}

#[test]
fn stopping_the_agent_drops_the_question() {
    let mut r = Run::new("watched");
    r.give(W);
    r.answer(W, json!({"head": "Done", "c": "abc123"}));
    assert!(r.asking());
    r.go(Input::Act { agent: W.into(), is: "edit".into(), text: "wrote".into(), more: Default::default(), t: None });
    // the answer counts as never given: the step is owed as before
    assert!(matches!(&r.m.tickets[&200].status, Status::Awaits { agent, forming: None, .. } if agent == W), "{:?}", r.m.tickets[&200].status);
    assert!(r.m.tickets[&200].pending.is_none());
    assert!(r.decided(json!([])).reply.is_err());
    r.replays();
}

#[test]
fn on_a_slow_thread_the_question_comes_before_the_take_halt() {
    let mut r = Run::new("change");
    r.give(W);
    r.go(Input::Slow { t: 200, on: true, by: "owner".into() }).reply.unwrap();
    let out = r.answer(W, json!({"head": "Done", "c": "abc123"}));
    assert!(out.lines.iter().any(|l| matches!(l, Line::Asked { question, .. } if question == "violated")), "{:?}", out.lines);
    assert!(!out.lines.iter().any(|l| matches!(l, Line::Halted { .. })), "{:?}", out.lines);
    let out = r.decided(json!([]));
    let halted: Vec<&str> = out.lines.iter().filter_map(|l| if let Line::Halted { what, .. } = l { Some(what.as_str()) } else { None }).collect();
    assert_eq!(halted, vec!["take"], "{:?}", out.lines);
    r.replays();
}

#[test]
fn the_checker_takes_a_violated_arm_only_around_an_agents_step_with_duties() {
    // with duties, a try around an agent's step may handle Violated
    assert_eq!(defs(KINDS, true).1, Vec::<String>::new());
    // without, the arm can never be taken
    let (_, errors) = defs(KINDS, false);
    assert!(errors.iter().any(|e| e.contains("kind change refused") && e.contains("Violated")), "{errors:#?}");
    // the owner's step never raises it
    let (_, errors) = defs(OWNERS, true);
    assert!(errors.iter().any(|e| e.contains("kind owners refused") && e.contains("Violated")), "{errors:#?}");
}

/// An ask the ticket stopped asking (cancelled while it was asked; S-1099511628020 on Tovrin-dev, 17 Nov): coordd gives
/// it up and answers null, which is a record that it ended unanswered, never an answer while the ticket asks it
/// (docket-core4, paper-research N-101 p1664).
#[test]
fn an_ask_that_ended_with_its_ticket_is_recorded_unanswered() {
    let mut r = Run::new("change");
    r.give(W);
    r.answer(W, json!({"head": "Done", "c": "abc123"}));
    assert!(r.asking() && r.m.asks_violated(200, 1));
    // while the ticket asks it, null is no answer
    assert!(r.decided(Value::Null).reply.is_err());
    assert!(r.asking());
    r.go(Input::Cancel { t: 200, by: "owner".into(), why: "the block it watched ended".into() }).reply.unwrap();
    assert!(!r.m.asks_violated(200, 1));
    let out = r.decided(Value::Null);
    let said = out.reply.clone().unwrap();
    assert!(said.contains("ended unanswered"), "{said}");
    assert!(out.lines.iter().any(|l| matches!(l, Line::Decided { t: 200, step: 1, question, answer, .. } if question == "violated" && answer.is_null())), "{:?}", out.lines);
    // a late answer of duties is refused as before
    assert!(r.decided(json!([])).reply.is_err());
    r.replays();
}
