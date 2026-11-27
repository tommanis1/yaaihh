//! A record made before a call changed its parameters replays as it was made (docket-try's S-5 and S-6, recorded
//! under `cut(branch)`, replayed after the draft's `cut()`: their results lost `by` and `why`).

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const KINDS: &str = "\
kind slow
  letter S
  form branch: Ref
  result Green(top: Ref) | Red(log: Text) | Stopped(by: Text, why: Text)
  pattern
    c <- step runner cut(branch);
    match c { Cut(first) => try { step runner cases(first) } handle { Ended(e) => return Stopped(e.by, e.why) } }
";

const CASES: &str = "\
call cases(cut: Ref) -> Green(top: Ref) | Red(log: Text)
  template
    Build {{ cut }}.
";

fn machine(cut: &str) -> Machine {
    let mut named = BTreeMap::new();
    let text = format!("{cut}\n  template\n    Cut the line.\n\n{CASES}");
    let calls: BTreeMap<String, CallSig> = syntax::parse_calls("r.call", &text, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
    let (d, errors) = Defs::build(&[("r.kind".to_string(), KINDS.to_string())], calls, named, 200);
    // under today's `cut()` the kind itself no longer loads; its tickets' recorded programs still replay
    let _ = errors;
    Machine::load(d)
}

fn answer(m: &mut Machine, log: &mut Vec<Line>, t: u64, head: Value, at: &str) {
    let out = m.apply(Input::Answer { t, by: "runner".into(), head, text: Text::New(String::new()) }, at.into());
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    log.extend(out.lines);
}

#[test]
fn a_record_from_before_a_call_lost_a_parameter_replays_with_its_fields() {
    let mut m = machine("call cut(branch: Ref) -> Cut(first: Ref)");
    let mut log = Vec::new();
    let out = m.apply(Input::Open { kind: "slow".into(), under: None, opener: "owner".into(), form: json!({"branch": "landed"}), text: None, names: vec![], told: vec![] }, "2026-11-15T18:00:01Z".into());
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    log.extend(out.lines);
    let t = *m.tickets.keys().max().unwrap();
    answer(&mut m, &mut log, t, json!({"head": "Cut", "first": "abc"}), "2026-11-15T18:00:02Z");
    answer(&mut m, &mut log, t, json!({"head": "Ended", "by": "owner", "why": "a slow cases ended"}), "2026-11-15T18:00:03Z");
    let done = json!({"head": "Stopped", "by": "owner", "why": "a slow cases ended"});
    assert!(matches!(&m.tickets[&t].status, Status::Done(v) if *v == done), "{:?}", m.tickets[&t].status);
    // today the call takes nothing: a record from before the calls went on it replays as made, the ticket marked as
    // no longer checking
    let mut m2 = machine("call cut() -> Cut(first: Ref)");
    assert_eq!(m2.replay(&super::without_calls(&log)), Vec::<String>::new());
    assert!(matches!(&m2.tickets[&t].status, Status::Done(v) if *v == done), "{:?}", m2.tickets[&t].status);
    assert!(m2.tickets[&t].broken.as_deref().is_some_and(|b| b.contains("no longer checks")), "{:?}", m2.tickets[&t].broken);
    // one made since replays against the calls on it: as made, and nothing marked
    let mut m3 = machine("call cut() -> Cut(first: Ref)");
    assert_eq!(m3.replay(&log), Vec::<String>::new());
    assert!(matches!(&m3.tickets[&t].status, Status::Done(v) if *v == done), "{:?}", m3.tickets[&t].status);
    assert_eq!(m3.tickets[&t].broken, None);
}
