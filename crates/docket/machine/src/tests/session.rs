//! A call's question, `asks C` (N-101; the owner: "3 no same session"): asked, not taken, and answered in the asker's
//! session.

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const CALLS: &str = "call design(words: Text) -> Design(text: Text) | Ask(question: Text)
  task design
  asks Ask
  template
    Design {{ words }}.
";

/// A designer made for the step alone (`fresh`, ended when its step is answered); HANDLER, a `when ask` of the kind.
fn kinds(handler: &str) -> String {
    format!(
        "kind change
  form words: Text
  result Text
  pattern
    d <- step fresh design(words);
    match d {{
      Design(text) => return text
    }}
{handler}"
    )
}

fn defs_of(calls: &str, kinds: &str) -> (Defs, Vec<String>) {
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = syntax::parse_calls("design.call", calls, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
    Defs::build(&[("change.kind".to_string(), kinds.to_string())], calls, named, 200)
}

struct Run {
    m: Machine,
    kinds: String,
    log: Vec<Line>,
    clock: u32,
}

const DESIGNER: &str = "f200-1";

impl Run {
    fn new(kinds: String) -> Run {
        let (d, errors) = defs_of(CALLS, &kinds);
        assert!(errors.is_empty(), "{errors:?}");
        let mut r = Run { m: Machine::load(d), kinds, log: Vec::new(), clock: 0 };
        let out = r.go(Input::Open { kind: "change".into(), under: None, opener: "owner".into(), form: json!({"words": "drop short buffers"}), text: None, names: vec![], told: vec![] });
        assert!(out.effects.contains(&Effect::Start { agent: DESIGNER.into(), t: 200, call: "design".into(), layer: None }), "{out:?}");
        r.give(DESIGNER);
        r
    }
    fn go(&mut self, i: Input) -> Out {
        self.clock += 1;
        let out = self.m.apply(i, format!("2026-11-15T21:00:{:02}Z", self.clock));
        self.log.extend(out.lines.iter().cloned());
        out
    }
    /// Everything due to AGENT, given to it; their ids.
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
    /// The ask ticket opened last.
    fn ask(&self) -> u64 {
        let (t, x) = self.m.tickets.range(..HIDDEN).next_back().unwrap();
        assert_eq!(x.kind, "ask");
        *t
    }
    fn replays(&self) {
        let text: Vec<String> = self.log.iter().map(|l| serde_json::to_string(l).unwrap()).collect();
        let lines: Vec<Line> = text.iter().map(|s| serde_json::from_str(s).unwrap()).collect();
        let mut m2 = Machine::load(defs_of(CALLS, &self.kinds).0);
        assert_eq!(m2.replay(&lines), Vec::<String>::new());
        for t in self.m.tickets.keys() {
            assert_eq!(m2.view(*t), self.m.view(*t), "ticket {t}");
        }
    }
}

#[test]
fn a_question_answered_is_asked_and_the_answer_goes_back_to_the_same_agent() {
    let mut r = Run::new(kinds(""));
    let out = r.answer(200, DESIGNER, json!({"head": "Ask", "question": "drop or report?"}));
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    // not taken: the fresh designer is not ended, and still owes the step
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::End { .. })), "{:?}", out.effects);
    assert!(matches!(&r.m.tickets[&200].status, Status::Awaits { agent, .. } if agent == DESIGNER));
    // no kind above takes asks: the owner is asked, on an ask ticket under the change
    let a = r.ask();
    assert_eq!((r.m.tickets[&a].under, r.m.tickets[&a].ask.as_ref().map(|q| q.text.as_str())), (Some(200), Some("drop or report?")));
    let ViewStatus::Awaits { agent, .. } = r.m.view(a).unwrap().status else { panic!() };
    assert_eq!(agent, "owner");
    // the designer's turn ending meanwhile does not answer the step
    r.go(Input::TurnEnded { agent: DESIGNER.into(), delivery: None, text: Loc { file: "t.jsonl".into(), offset: 2, len: 1 }, later: vec![], stood: None, go: None });
    assert!(matches!(&r.m.tickets[&200].status, Status::Awaits { agent, forming: None, .. } if agent == DESIGNER));
    // the owner answers: the words and the step's line again go to the same agent
    r.answer(a, "owner", json!({"head": "Answer", "text": "report"})).reply.unwrap();
    let due = r.give(DESIGNER);
    assert!(due.iter().any(|d| matches!(&d.what, What::Words { text: Text::New(w), .. } if w == "The answer to your question \"drop or report?\": report")), "{due:?}");
    assert!(due.iter().any(|d| d.id == "C-200.s1.again"), "{due:?}");
    // its next answer answers the step; only then does the fresh agent end
    let out = r.answer(200, DESIGNER, json!({"head": "Design", "text": "report short buffers"}));
    assert!(out.effects.contains(&Effect::End { agent: DESIGNER.into() }), "{:?}", out.effects);
    assert_eq!(r.m.view(200).unwrap().status, ViewStatus::Done { result: json!("report short buffers") });
    r.replays();
}

#[test]
fn a_kind_that_takes_asks_answers_the_question() {
    let mut r = Run::new(kinds("  when ask(q) do\n    a <- step owner \"from the lead's side: {{ q.text }}\" -> Answer(text: Text);\n    reply q a\n"));
    r.answer(200, DESIGNER, json!({"head": "Ask", "question": "drop or report?"})).reply.unwrap();
    let a = r.ask();
    assert!(r.log.iter().any(|l| matches!(l, Line::Opened { t, program, .. } if *t == a && program.contains("from the lead's side"))));
    r.answer(a, "owner", json!({"head": "Answer", "text": "report"})).reply.unwrap();
    assert!(r.give(DESIGNER).iter().any(|d| d.id == "C-200.s1.again"));
    r.replays();
}

#[test]
fn a_second_question_while_the_first_is_open_is_refused() {
    let mut r = Run::new(kinds(""));
    r.answer(200, DESIGNER, json!({"head": "Ask", "question": "drop or report?"})).reply.unwrap();
    let out = r.answer(200, DESIGNER, json!({"head": "Ask", "question": "and the logs?"}));
    assert!(out.lines.iter().any(|l| matches!(l, Line::Refused { why, .. } if why.contains("not answered yet"))), "{:?}", out.lines);
    assert_eq!(r.m.tickets.values().filter(|x| x.kind == "ask").count(), 1);
    r.replays();
}

#[test]
fn the_checker_on_a_call_that_asks() {
    // the program sees the answers without the question: an arm on it is refused
    let k = kinds("").replace("      Design(text) => return text\n", "      Design(text) => return text\n      Ask(q) => return q\n");
    let (_, errors) = defs_of(CALLS, &k);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("Ask"), "{errors:?}");
    // the question is one of the answers, with one Text field
    let mut named = BTreeMap::new();
    let e = syntax::parse_calls("d.call", &CALLS.replace("asks Ask", "asks Question"), &mut named).unwrap_err();
    assert!(e.contains("`asks Question`: Question is none of the call's answers"), "{e}");
    let e = syntax::parse_calls("d.call", &CALLS.replace("Ask(question: Text)", "Ask(question: Text, why: Text)"), &mut named).unwrap_err();
    assert!(e.contains("`asks Ask`: the question has one field, of type Text"), "{e}");
}

/// A step's Ask whose question is the agent's answer by place (as forming gives it) goes on as that place, for coordd
/// to read when it gives the question: never printed as a record (paper-research N-101 p524: the lead was given
/// `{loc = {file = …}}`).
#[test]
fn a_question_by_place_goes_on_as_its_place() {
    let place = json!({"loc": {"file": "/s/t.jsonl", "offset": 170519, "len": 3801}});
    let mut r = Run::new(kinds(""));
    r.answer(200, DESIGNER, json!({"head": "Ask", "question": place})).reply.unwrap();
    let a = r.ask();
    assert_eq!(r.m.tickets[&a].ask.as_ref().map(|q| q.words.clone()), Some(place.clone()));
    // what a `when ask(q)` reads as `q.text`: the place, not a printed record
    assert_eq!(r.m.tickets[&a].form["text"], place, "{:?}", r.m.tickets[&a].form);
    // answered later: the words go back without quoting a question the machine cannot read
    r.go(Input::TurnEnded { agent: DESIGNER.into(), delivery: None, text: Loc { file: "t.jsonl".into(), offset: 2, len: 1 }, later: vec![], stood: None, go: None });
    r.answer(a, "owner", json!({"head": "Answer", "text": "report"})).reply.unwrap();
    let due = r.give(DESIGNER);
    assert!(due.iter().any(|d| matches!(&d.what, What::Words { text: Text::New(w), .. } if w == "The answer to your question: report")), "{due:?}");
    r.replays();
}

/// A question the designer opens itself (`coord ask`) while it holds the step is that step's question (paper-research
/// N-101 p524, 2: C-37's designer asked Q-39, then answered the step with Ask three times, and each opened Q-39 again).
/// The step waits on it: a turn's end does not answer the step, an Ask is taken as the same question (no ask ticket),
/// and when the question closes the step is given again.
#[test]
fn a_question_the_holder_opened_is_the_steps_question() {
    let question = "kind question\n  form ask: Text, context: Text?\n  result Answer(text: Text)\n  pattern\n    a <- step owner \"{{ ask }}\" -> Answer(text: Text);\n    return a\n";
    let mut r = Run::new(format!("{}{question}", kinds("")));
    let out = r.go(Input::Open { kind: "question".into(), under: None, opener: DESIGNER.into(), form: json!({"ask": "drop or report?", "context": null}), text: None, names: vec![], told: vec![] });
    assert!(matches!(&out.lines[0], Line::Open { asked_on: Some(200), .. }), "{:?}", out.lines);
    let q = r.m.tickets.range(..HIDDEN).next_back().map(|(t, _)| *t).unwrap();
    assert!(r.m.tickets[&q].ask.as_ref().is_some_and(|a| a.own && a.held == 200));
    // its turn ends with no answer: the step is not answered, nor given again, while the question is open
    r.go(Input::TurnEnded { agent: DESIGNER.into(), delivery: Some("C-200.s1".into()), text: Loc { file: "t.jsonl".into(), offset: 2, len: 1 }, later: vec![], stood: None, go: None });
    assert!(matches!(&r.m.tickets[&200].status, Status::Awaits { agent, forming: None, .. } if agent == DESIGNER));
    assert!(!r.m.due(DESIGNER).iter().any(|d| d.t == 200), "{:?}", r.m.due(DESIGNER));
    // the step answered with Ask: the same question, so no ask ticket, and no refusal
    let out = r.answer(200, DESIGNER, json!({"head": "Ask", "question": "drop or report?"}));
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(!out.lines.iter().any(|l| matches!(l, Line::Refused { .. } | Line::Opened { .. })), "{:?}", out.lines);
    assert_eq!(r.m.tickets.values().filter(|x| x.kind == "ask").count(), 0);
    // the owner answers: the question's close reaches the designer, and the step again
    r.answer(q, "owner", json!({"head": "Answer", "text": "report"})).reply.unwrap();
    let due = r.give(DESIGNER);
    assert!(due.iter().any(|d| d.id == "C-200.s1.again"), "{due:?}");
    assert!(due.iter().any(|d| d.t == q && matches!(d.what, What::Closed { .. })), "{due:?}");
    let out = r.answer(200, DESIGNER, json!({"head": "Design", "text": "report short buffers"}));
    assert!(out.effects.contains(&Effect::End { agent: DESIGNER.into() }), "{:?}", out.effects);
    r.replays();
    // an older line, without the step it was asked on: the question stands apart, as before
    let old: Vec<Line> = r.log.iter().map(|l| match l {
        Line::Open { .. } => serde_json::from_value(serde_json::to_value(l).map(|mut v| { v.as_object_mut().unwrap().remove("asked_on"); v }).unwrap()).unwrap(),
        _ => l.clone(),
    }).collect();
    let mut m2 = Machine::load(defs_of(CALLS, &r.kinds).0);
    m2.replay(&old);
    assert!(m2.tickets[&q].ask.is_none());
}
