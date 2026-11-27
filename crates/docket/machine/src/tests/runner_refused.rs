//! A step the runner refused to take (coordd's `RUNNER step` exited refused; N-159 post 43): it raises RunnerRefused
//! with the runner's message, so it does not stand waiting in silence; a handler takes it as any exception.

use crate::*;
use serde_json::json;
use std::collections::BTreeMap;

const CALLS: &str = "\
call accepted(commit: Ref) -> Accepted(top: Ref)
  template
    Accept {{ commit }}.
";

const KINDS: &str = "\
kind plain
  letter B
  form commit: Ref
  result Accepted(top: Ref)
  pattern
    step runner accepted(commit)

kind handled
  letter H
  form commit: Ref
  result Accepted(top: Ref) | NotTaken(why: Text)
  pattern
    try { step runner accepted(commit) } handle { RunnerRefused(e) => return NotTaken(e.why) }
";

fn machine() -> Machine {
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = syntax::parse_calls("r.call", CALLS, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
    let (d, errors) = Defs::build(&[("r.kind".to_string(), KINDS.to_string())], calls, named, 200);
    assert!(errors.is_empty(), "{errors:#?}");
    Machine::load(d)
}

fn open(m: &mut Machine, log: &mut Vec<Line>, kind: &str) -> u64 {
    let out = m.apply(Input::Open { kind: kind.into(), under: None, opener: "owner".into(), form: json!({"commit": "f5d6af7"}), text: None, names: vec![], told: vec![] }, "2026-11-15T22:00:01Z".into());
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    log.extend(out.lines);
    *m.tickets.keys().max().unwrap()
}

const WHY: &str = "coord-runner step: refused: f5d6af7 is no commit of a";

#[test]
fn a_refused_runner_step_raises_runner_refused_with_the_runners_message() {
    let mut m = machine();
    let mut log = Vec::new();
    let t = open(&mut m, &mut log, "plain");
    // another step than the one owed: refused, nothing changes
    let out = m.apply(Input::RunnerRefused { t, step: 2, why: WHY.into() }, "2026-11-15T22:00:02Z".into());
    assert!(out.reply.is_err() && out.lines.is_empty(), "{:?}", out.reply);
    let out = m.apply(Input::RunnerRefused { t, step: 1, why: WHY.into() }, "2026-11-15T22:00:03Z".into());
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    log.extend(out.lines);
    let Status::Failed(r) = &m.tickets[&t].status else { panic!("{:?}", m.tickets[&t].status) };
    assert_eq!((r.exc.as_str(), &r.value), ("RunnerRefused", &json!({"head": "RunnerRefused", "call": "accepted", "why": WHY})));
    // handled: the handler gets the runner's message
    let h = open(&mut m, &mut log, "handled");
    let out = m.apply(Input::RunnerRefused { t: h, step: 1, why: WHY.into() }, "2026-11-15T22:00:04Z".into());
    log.extend(out.lines);
    assert!(matches!(&m.tickets[&h].status, Status::Done(v) if *v == json!({"head": "NotTaken", "why": WHY})), "{:?}", m.tickets[&h].status);
    let mut m2 = machine();
    assert_eq!(m2.replay(&log), Vec::<String>::new());
    assert_eq!(m2.tickets[&t].status, m.tickets[&t].status);
}
