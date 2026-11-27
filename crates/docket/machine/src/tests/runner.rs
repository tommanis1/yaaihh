//! The runner's side of the machine (runner.rs; N-101, docket-core5).

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const CALLS: &str = "\
call cases(cut: Ref) -> Green(top: Ref) | Red(log: Text)
  template
    Build {{ cut }}.

call cases2(cut: Ref) -> Green(top: Ref) | Ended(by: Text, why: Text)
  template
    Build {{ cut }}.

call look(cut: Ref) -> Seen
  template
    Look at {{ cut }}.
";

const KINDS: &str = "\
kind build
  letter B
  form cut: Ref
  result Green(top: Ref) | Red(log: Text) | Stopped(why: Text)
  pattern
    try { step runner cases(cut) } handle { Ended(e) => return Stopped(e.why) }

kind bare
  letter R
  form cut: Ref
  result Green(top: Ref) | Red(log: Text)
  pattern
    step runner cases(cut)

kind declared
  letter D
  form cut: Ref
  result Green(top: Ref) | Ended(by: Text, why: Text)
  pattern
    step runner cases2(cut)

kind piece
  letter P
  form cut: Ref
  result Seen
  pattern
    let agent a in {
      r <- during { step a look(cut) } when act(u) where u.is == \"edit\" do { post u.text };
      return r
    }

kind agentstep
  letter A
  form cut: Ref
  result Seen
  pattern
    step owner look(cut)
";

fn machine() -> Machine {
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = syntax::parse_calls("r.call", CALLS, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
    let (d, errors) = Defs::build(&[("r.kind".to_string(), KINDS.to_string())], calls, named, 200);
    assert!(errors.is_empty(), "{errors:#?}");
    Machine::load(d)
}

struct Run {
    m: Machine,
    log: Vec<Line>,
    clock: u32,
}

impl Run {
    fn go(&mut self, i: Input) -> Out {
        self.clock += 1;
        let out = self.m.apply(i, format!("2026-11-15T20:00:{:02}Z", self.clock));
        self.log.extend(out.lines.iter().cloned());
        out
    }
    fn open(&mut self, kind: &str) -> u64 {
        let out = self.go(Input::Open { kind: kind.into(), under: None, opener: "owner".into(), form: json!({"cut": "c3d4e5f"}), text: None, names: vec![], told: vec![] });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        *self.m.tickets.keys().max().unwrap()
    }
    fn answer(&mut self, t: u64, by: &str, head: Value) -> Out {
        self.go(Input::Answer { t, by: by.into(), head, text: Text::New(String::new()) })
    }
}

fn run() -> Run {
    Run { m: machine(), log: Vec::new(), clock: 0 }
}

#[test]
fn the_runners_ended_raises_ended_where_the_call_does_not_declare_it() {
    let mut r = run();
    let t = r.open("build");
    assert!(matches!(&r.m.tickets[&t].status, Status::Awaits { agent, .. } if agent == "runner"));
    let out = r.answer(t, "runner", json!({"head": "Ended", "by": "lead", "why": "over the bound"}));
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    // handled in the pattern: the ticket ends with what the handler returns
    assert!(matches!(&r.m.tickets[&t].status, Status::Done(v) if v == &json!({"head": "Stopped", "why": "over the bound"})), "{:?}", r.m.tickets[&t].status);
    assert!(!r.log.iter().any(|l| matches!(l, Line::Refused { .. })), "not refused: {:?}", r.log);
    // the log replays as it was made
    let mut m2 = machine();
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
}

#[test]
fn ended_not_handled_fails_the_ticket_with_ended() {
    let mut r = run();
    let t = r.open("bare");
    r.answer(t, "runner", json!({"head": "Ended", "by": "lead", "why": "w"}));
    assert!(matches!(&r.m.tickets[&t].status, Status::Failed(x) if x.exc == "Ended" && x.value["by"] == "lead" && x.value["why"] == "w"), "{:?}", r.m.tickets[&t].status);
    assert!(r.log.iter().any(|l| matches!(l, Line::Closed { raised: Some(x), .. } if x.exc == "Ended")), "{:?}", r.log);
}

#[test]
fn a_call_that_declares_ended_takes_it_as_its_answer() {
    let mut r = run();
    let t = r.open("declared");
    r.answer(t, "runner", json!({"head": "Ended", "by": "owner", "why": "w"}));
    assert!(matches!(&r.m.tickets[&t].status, Status::Done(v) if v["head"] == "Ended" && v["by"] == "owner"), "{:?}", r.m.tickets[&t].status);
}

#[test]
fn only_the_runner_raises_ended() {
    let mut r = run();
    let t = r.open("agentstep");
    let out = r.answer(t, "owner", json!({"head": "Ended", "by": "x", "why": "y"}));
    assert!(out.reply.is_err(), "an Ended that does not fit is refused: {:?}", out.reply);
    assert!(r.m.tickets[&t].is_open());
}

#[test]
fn an_act_is_recorded_on_the_ticket_its_agent_owes_and_its_handler_takes_it() {
    let mut r = run();
    let t = r.open("piece");
    let a = match &r.m.tickets[&t].status {
        Status::Awaits { agent, .. } => agent.clone(),
        s => panic!("{s:?}"),
    };
    // an edit, given as the event act (the runner's and coordd's way), its other fields kept
    let out = r.go(Input::Event { name: "act".into(), value: json!({"by": a, "is": "edit", "text": "Edit src/a.rs", "tool": "Edit", "files": ["src/a.rs"]}) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let line = out.lines.first().cloned().unwrap();
    assert!(matches!(&line, Line::Act { t: Some(u), agent, is, text, more, .. } if *u == t && agent == &a && is == "edit" && text == "Edit src/a.rs" && more.get("tool") == Some(&json!("Edit"))), "{line:?}");
    assert!(out.lines.len() > 1, "the during's handler ran: {:?}", out.lines);
    // the line as the record has it: flat, its extra fields beside the rest
    let v = serde_json::to_value(&line).unwrap();
    assert_eq!((v["ev"].clone(), v["t"].clone(), v["files"].clone()), (json!("act"), json!(t), json!(["src/a.rs"])), "{v}");
    assert_eq!(serde_json::from_value::<Line>(v).unwrap(), line);
    // a check: the `where` does not hold; still recorded
    let out = r.go(Input::Act { agent: a.clone(), is: "submit".into(), text: "quick a:layer/X".into(), more: Default::default(), t: None });
    assert!(matches!(out.lines.as_slice(), [Line::Act { is, .. }] if is == "submit"), "{:?}", out.lines);
    // an agent that owes nothing: recorded with no ticket, nothing runs
    let out = r.go(Input::Act { agent: "someone".into(), is: "edit".into(), text: "Write x".into(), more: Default::default(), t: None });
    assert_eq!(out.reply, Ok("Recorded.".into()));
    assert!(matches!(out.lines.as_slice(), [Line::Act { t: None, .. }]), "{:?}", out.lines);
    // an act names its agent and what it is
    assert!(r.go(Input::Act { agent: String::new(), is: "edit".into(), text: String::new(), more: Default::default(), t: None }).reply.is_err());
    let mut m2 = machine();
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
}

/// An act with a ticket given (an edit coordd found in a layer, coordd found.rs): on the ticket its agent owes a step
/// on all the same; on the given one only when it owes none (a minder's); none when no such ticket is.
#[test]
fn a_given_ticket_is_an_acts_only_when_its_agent_owes_none() {
    let mut r = run();
    let t = r.open("piece");
    let a = match &r.m.tickets[&t].status {
        Status::Awaits { agent, .. } => agent.clone(),
        s => panic!("{s:?}"),
    };
    let u = r.open("bare");
    let act = |agent: &str, t: Option<u64>| Input::Act { agent: agent.into(), is: "edit".into(), text: "found at turn end: x".into(), more: Default::default(), t };
    let out = r.go(act(&a, Some(u)));
    assert!(matches!(out.lines.first(), Some(Line::Act { t: Some(x), .. }) if *x == t), "owed: {:?}", out.lines);
    let out = r.go(act("minder", Some(u)));
    assert!(matches!(out.lines.as_slice(), [Line::Act { t: Some(x), .. }] if *x == u), "given: {:?}", out.lines);
    let out = r.go(act("minder", Some(987_654)));
    assert!(matches!(out.lines.as_slice(), [Line::Act { t: None, .. }]), "no such ticket: {:?}", out.lines);
    let mut m2 = machine();
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
}

#[test]
fn the_runners_events_are_kept_on_their_ticket_though_nothing_takes_them() {
    let mut r = run();
    let t = r.open("bare");
    let id = r.m.id(t);
    for (name, value) in [
        ("job_start", json!({"ticket": id, "name": id, "id": "760526", "job": "cases-B-1-O2/check", "reserved_gb": 0.1, "bound": null})),
        ("jobs", json!({"ticket": id, "name": id, "jobs": [{"id": "760526", "wall_s": 5.0, "mem_gb": 0.2, "peak_gb": 0.3}]})),
        ("job_measure", json!({"ticket": id, "name": id, "id": "760526", "phase": "measure", "wall_s": 2.3, "wall_ms": 2347, "current_mb": 1, "peak_mb": 3})),
        ("job_end", json!({"ticket": id, "name": id, "id": "760526", "how": "ended", "wall_s": 9.0})),
    ] {
        let out = r.go(Input::Event { name: name.into(), value });
        assert_eq!(out.reply, Ok("Recorded.".into()), "{name}");
        assert!(matches!(&out.lines[..], [Line::Event { on: Some(u), fired, .. }] if *u == t && fired.is_empty()), "{name}: {:?}", out.lines);
    }
    // an agent's check's job, and one of nobody's: kept on no ticket
    let out = r.go(Input::Event { name: "job_start".into(), value: json!({"name": "impl.c20261115201502", "ticket": "impl.c20261115201502", "id": "7"}) });
    assert!(matches!(&out.lines[..], [Line::Event { on: None, .. }]), "{:?}", out.lines);
    let out = r.go(Input::Event { name: "job_end".into(), value: json!({"name": null, "id": "8"}) });
    assert!(matches!(&out.lines[..], [Line::Event { on: None, .. }]), "{:?}", out.lines);
    // still owed; and after the ticket closed, kept on it all the same
    assert!(r.m.tickets[&t].is_open());
    r.answer(t, "runner", json!({"head": "Green", "top": "abc"}));
    let out = r.go(Input::Event { name: "job_end".into(), value: json!({"ticket": id, "id": "9", "how": "done"}) });
    assert!(matches!(&out.lines[..], [Line::Event { on: Some(u), .. }] if *u == t), "{:?}", out.lines);
    // another project event nothing takes is still refused, and not written
    assert!(r.go(Input::Event { name: "deploy".into(), value: json!({}) }).reply.is_err());
    let mut m2 = machine();
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
}

#[test]
fn a_refused_step_is_kept_on_its_ticket_and_said_to_the_lead() {
    let mut r = run();
    let t = r.open("bare");
    let id = r.m.id(t);
    let refused = |agent: &str| json!({"name": id, "kind": "step_refused", "id": "4", "call": "hand_in", "to": "runner", "by": "t", "by_uid": 1000, "by_cgroup": format!("/user.slice/coordd-research.service/agents/{agent}/main"), "agent": agent, "args": "{}", "why": "steps come from the program, through coordd; this caller is in coordd's agents/f55-2"});
    // no lead: the post names the owner
    let posts = r.m.tickets[&t].posts.len();
    let out = r.go(Input::Event { name: "step_refused".into(), value: { let mut v = refused("f55-2"); v["ticket"] = json!(id); v } });
    assert_eq!(out.reply, Ok("Recorded.".into()));
    assert!(matches!(&out.lines[..], [Line::Event { on: Some(u), .. }, Line::Posted { t: w, by, step: None, .. }, ..] if *u == t && *w == t && by == &id), "{:?}", out.lines);
    let x = &r.m.tickets[&t];
    assert_eq!(x.posts.len(), posts + 1);
    let p = x.posts.last().unwrap();
    assert_eq!(p.names, vec!["owner".to_string()]);
    assert_eq!(p.text, Text::New(format!("f55-2 ran the runner's hand_in for {id} itself; the runner refused it: steps come from the program, through coordd; this caller is in coordd's agents/f55-2.")));
    // with a lead: the lead
    r.go(Input::Role { agent: "paper-research".into(), role: "lead".into(), also: vec![] }).reply.unwrap();
    r.go(Input::Event { name: "step_refused".into(), value: { let mut v = refused("f55-2"); v["ticket"] = json!(id); v } });
    assert_eq!(r.m.tickets[&t].posts.last().unwrap().names, vec!["paper-research".to_string()]);
    // still owed: the step is the runner's as before
    assert!(matches!(r.m.tickets[&t].status, Status::Awaits { .. }));
    let mut m2 = machine();
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    // with no agent, by the user who ran it
    let text = crate::runner::refused_text("C-1", &json!({"call": "cut", "by": "t", "why": "steps come from the program."}));
    assert_eq!(text, "t ran the runner's cut for C-1 itself; the runner refused it: steps come from the program.");
}

#[test]
fn a_role_line_says_who_is_the_runner_and_the_lead() {
    let mut r = run();
    let out = r.go(Input::Role { agent: "runner".into(), role: "runner".into(), also: vec![] });
    assert_eq!(out.reply, Ok("runner is runner.".into()));
    r.go(Input::Role { agent: "paper-research".into(), role: "lead".into(), also: vec![] });
    let v = serde_json::to_value(&r.log[1]).unwrap();
    assert_eq!(v, json!({"ev": "role", "at": "2026-11-15T20:00:02Z", "agent": "paper-research", "role": "lead"}));
    assert_eq!(r.m.roles.get("paper-research").map(String::as_str), Some("lead"));
    assert!(r.go(Input::Role { agent: "x".into(), role: String::new(), also: vec![] }).reply.is_err());
    let mut m2 = machine();
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert_eq!(m2.roles, r.m.roles);
}

#[test]
fn an_order_is_a_typed_line_opened_with_its_constructor_as_text() {
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = syntax::parse_calls("order.call", include_str!("../../../../../agent-base/calls/order.call"), &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
    let (d, errors) = Defs::build(&[("order.kind".to_string(), include_str!("../../../../../agent-base/kinds/order.kind").to_string())], calls, named, 200);
    assert!(errors.is_empty(), "{errors:#?}");
    let mut r = Run { m: Machine::load(d.clone()), log: Vec::new(), clock: 0 };
    let out = r.go(Input::Open { kind: "order".into(), under: None, opener: "lead".into(), form: json!({"order": "SetBound(runtime, run-seconds, 12)"}), text: None, names: vec!["runner".into()], told: vec![] });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let want = json!({"head": "SetBound", "kind": "runtime", "measure": "run-seconds", "amount": 12});
    assert!(matches!(&out.lines[0], Line::Open { form, .. } if form["order"] == want), "{:?}", out.lines[0]);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Closed { result: Some(v), .. } if *v == want)), "{:?}", out.lines);
    for (text, ok) in [("Spare(r1)", true), ("Overrule(runtime, run-seconds)", true), ("SetBound(runtime, run-seconds, twelve)", false), ("Spare", false), ("Halt(x)", false)] {
        let out = r.go(Input::Open { kind: "order".into(), under: None, opener: "owner".into(), form: json!({"order": text}), text: None, names: vec!["runner".into()], told: vec![] });
        assert_eq!(out.reply.is_ok(), ok, "{text}: {:?}", out.reply);
    }
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
}

// ---- step_args (N-101 post 55, E) ------------------------------------------------------------------------------

#[test]
fn a_steps_arguments_follow_its_step_line() {
    let mut r = run();
    let t = r.open("bare");
    let i = r.log.iter().position(|l| matches!(l, Line::Step { t: u, step: 1, call, .. } if *u == t && call == "cases")).expect("a step line");
    assert!(matches!(&r.log[i + 1], Line::StepArgs { t: u, step: 1, args, .. } if *u == t && args == &json!({"cut": "c3d4e5f"})), "{:?}", r.log);
    let v = serde_json::to_value(&r.log[i + 1]).unwrap();
    assert_eq!((v["ev"].as_str(), v["t"].as_u64(), v["step"].as_u64(), &v["args"]), (Some("step_args"), Some(t), Some(1), &json!({"cut": "c3d4e5f"})));
    r.answer(t, "runner", json!({"head": "Green", "top": "abc"}));
    let mut m2 = machine();
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert!(matches!(&m2.tickets[&t].status, Status::Done(v) if v["top"] == "abc"));
}

#[test]
fn a_log_from_before_step_args_replays_as_it_is() {
    let mut r = run();
    let t = r.open("bare");
    r.answer(t, "runner", json!({"head": "Green", "top": "abc"}));
    // the same log as one written before E: no step_args line
    let old: Vec<Line> = r.log.iter().filter(|l| !matches!(l, Line::StepArgs { .. })).cloned().collect();
    assert!(old.len() < r.log.len());
    let mut m2 = machine();
    assert_eq!(m2.replay(&old), Vec::<String>::new());
    assert!(matches!(&m2.tickets[&t].status, Status::Done(v) if v["top"] == "abc"), "{:?}", m2.tickets[&t].status);
    assert!(m2.tickets[&t].broken.is_none());
}

/// A role the policy gives an agent besides its own (`also`, coordd's `roles_held`) is the machine's answer to who
/// holds it: the record says it, and a replay holds it as well (paper-research N-101 p508: one way of asking).
#[test]
fn a_role_the_policy_gives_is_held_as_the_record_says() {
    let mut r = run();
    let out = r.go(Input::Role { agent: "orch".into(), role: "orchestrator".into(), also: vec!["lead".into()] });
    assert_eq!(out.reply, Ok("orch is orchestrator, and lead.".into()));
    let v = serde_json::to_value(&r.log[0]).unwrap();
    assert_eq!(v, json!({"ev": "role", "at": "2026-11-15T20:00:01Z", "agent": "orch", "role": "orchestrator", "also": ["lead"]}));
    assert_eq!(r.m.holders("lead"), vec!["orch".to_string()]);
    assert_eq!(r.m.holders("orchestrator"), vec!["orch".to_string()]);
    assert_eq!(r.m.roles_of("orch"), vec!["orchestrator".to_string(), "lead".to_string()]);
    assert!(r.go(Input::Role { agent: "orch".into(), role: "orchestrator".into(), also: vec![String::new()] }).reply.is_err());
    let mut m2 = machine();
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert_eq!(m2.holders("lead"), vec!["orch".to_string()]);
    // the policy no longer gives it: a line with none, and it holds its own only
    r.go(Input::Role { agent: "orch".into(), role: "orchestrator".into(), also: vec![] }).reply.unwrap();
    assert!(r.m.holders("lead").is_empty());
    // a line from before `also` reads as none
    let old: Line = serde_json::from_value(json!({"ev": "role", "at": "2026-11-15T20:00:01Z", "agent": "orch", "role": "lead"})).unwrap();
    assert!(matches!(old, Line::Role { ref also, .. } if also.is_empty()));
}
