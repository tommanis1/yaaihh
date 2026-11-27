//! The fork (N-101 posts 11 and 12, docket-core2's proposal at posts 22 and 23, docket-core's answer): `step fork A
//! c(...)` and `let agent x = fork A in {...}`; and a scoped agent coordd could not start (Input::StartFailed).

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The kinds and calls of agent-base that the forks below lean on (none of them), and the kinds under test.
fn defs_with(extra: &[(&str, &str)]) -> (Defs, Vec<String>) {
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = [("decision.call", include_str!("../../../../../agent-base/calls/decision.call"))]
        .iter()
        .flat_map(|(f, t)| syntax::parse_calls(f, t, &mut named).unwrap())
        .map(|c| (c.name.clone(), c))
        .collect();
    let files: Vec<(String, String)> = extra.iter().map(|(f, t)| (f.to_string(), t.to_string())).collect();
    Defs::build(&files, calls, named, 200)
}

struct Run {
    m: Machine,
    log: Vec<Line>,
    clock: u32,
}

impl Run {
    fn new(kinds: &[(&str, &str)]) -> Run {
        let (d, errors) = defs_with(kinds);
        assert!(errors.is_empty(), "{errors:?}");
        Run { m: Machine::load(d), log: Vec::new(), clock: 0 }
    }
    fn go(&mut self, i: Input) -> Out {
        self.clock += 1;
        let out = self.m.apply(i, format!("2026-11-15T18:00:{:02}Z", self.clock));
        self.log.extend(out.lines.iter().cloned());
        out
    }
    fn open(&mut self, kind: &str, form: Value) -> Out {
        self.go(Input::Open { kind: kind.into(), under: None, opener: "owner".into(), form, text: None, names: vec![], told: vec![] })
    }
    fn take(&mut self, agent: &str) -> Delivery {
        let due = self.m.due(agent);
        assert_eq!(due.len(), 1, "{agent}: {due:?}");
        let d = due[0].clone();
        self.go(Input::Given { t: d.t, to: agent.into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None });
        d
    }
    fn answer(&mut self, t: u64, by: &str, head: Value) -> Out {
        self.go(Input::Answer { t, by: by.into(), head, text: Text::New("as said".into()) })
    }
    fn replays(&self, kinds: &[(&str, &str)]) {
        let (d, _) = defs_with(kinds);
        let mut m2 = Machine::load(d);
        assert_eq!(m2.replay(&self.log), Vec::<String>::new());
        assert_eq!(m2.view(200), self.m.view(200));
    }
}

fn forked(out: &Out) -> Vec<(u64, String, String)> {
    out.lines.iter().filter_map(|l| if let Line::Forked { t, agent, from, .. } = l { Some((*t, agent.clone(), from.clone())) } else { None }).collect()
}

/// Tovrin's `piece`, cut down (plan/tovrin-program/on-library/tovrin.kind): the implementer a fork of the designer,
/// named because later steps go back to it; and the one-step form beside it.
const FORK_KINDS: [(&str, &str); 1] = [(
    "fork.kind",
    "kind piece\n  letter P\n  form designer: Agent, plan: Text\n  result Text\n  pattern\n    let agent a = fork designer in {\n      c <- step a \"Build {{ plan }}\" -> Commit(c: Text);\n      match c { Commit(c) =>\n        h <- step a \"Hand in {{ c }}\" -> Handed;\n        return c }\n    }\n\nkind once\n  letter O\n  form designer: Agent, plan: Text\n  result Text\n  pattern\n    c <- step fork designer \"Build {{ plan }}\" -> Commit(c: Text);\n    match c { Commit(c) => return c }\n\nkind cut\n  letter K\n  form designer: Agent, plan: Text\n  result Text\n  pattern\n    layer on \"line\" {\n      c <- step fork designer \"Build {{ plan }}\" -> Commit(c: Text);\n      match c { Commit(c) => return c }\n    }\n\nkind later\n  letter L\n  form designer: Agent, plan: Text\n  result Text\n  pattern\n    let agent a = fork designer in {\n      g <- step owner \"Go on {{ plan }}?\" -> Go;\n      c <- step a \"Build {{ plan }}\" -> Commit(c: Text);\n      match c { Commit(c) => return c }\n    }\n\nkind slowly\n  letter S\n  form designer: Agent, plan: Text\n  result Text\n  pattern\n    g <- step owner \"Go on {{ plan }}?\" -> Go;\n    c <- step fork designer \"Build {{ plan }}\" -> Commit(c: Text);\n    match c { Commit(c) => return c }\n",
)];

#[test]
fn let_agent_fork_makes_one_agent_from_a_copy_that_later_steps_go_back_to() {
    let mut r = Run::new(&FORK_KINDS);
    let out = r.open("piece", json!({"designer": "f150-2", "plan": "the median"}));
    // made when the block is entered: the record names whose conversation it copies, coordd is told to fork it
    assert_eq!(forked(&out), vec![(200, "f200-1".to_string(), "f150-2".to_string())]);
    assert!(out.effects.contains(&Effect::Fork { agent: "f200-1".into(), t: 200, call: syntax::INLINE.into(), layer: None, from: "f150-2".into() }), "{:?}", out.effects);
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::Start { .. })), "a fork is not also a fresh start: {:?}", out.effects);
    assert!(!out.lines.iter().any(|l| matches!(l, Line::Agent { what, .. } if what == "made")), "{:?}", out.lines);
    // the designer is never given the step: it goes on unchanged
    assert!(r.m.due("f150-2").is_empty());
    let d = r.take("f200-1");
    let out = r.answer(d.t, "f200-1", json!({"head": "Commit", "c": "abc"}));
    assert!(forked(&out).is_empty(), "the second step goes to the same fork, made once: {:?}", out.lines);
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::Fork { .. } | Effect::Start { .. })), "{:?}", out.effects);
    let d = r.take("f200-1");
    let out = r.answer(d.t, "f200-1", json!({"head": "Handed"}));
    // the block's end ends it, as any let agent's
    assert!(out.effects.contains(&Effect::End { agent: "f200-1".into() }), "{:?}", out.effects);
    assert!(matches!(&r.m.tickets[&200].status, Status::Done(v) if v == "abc"), "{:?}", r.m.tickets[&200].status);
    r.replays(&FORK_KINDS);
}

#[test]
fn step_fork_makes_an_agent_for_that_step_alone() {
    let mut r = Run::new(&FORK_KINDS);
    let out = r.open("once", json!({"designer": "designer", "plan": "the median"}));
    assert_eq!(forked(&out), vec![(200, "f200-1".to_string(), "designer".to_string())]);
    assert!(out.effects.contains(&Effect::Fork { agent: "f200-1".into(), t: 200, call: syntax::INLINE.into(), layer: None, from: "designer".into() }), "{:?}", out.effects);
    assert!(r.m.due("designer").is_empty());
    let d = r.take("f200-1");
    let out = r.answer(d.t, "f200-1", json!({"head": "Commit", "c": "abc"}));
    // ended once answered, as `fresh` is
    assert!(out.effects.contains(&Effect::End { agent: "f200-1".into() }), "{:?}", out.effects);
    assert!(matches!(&r.m.tickets[&200].status, Status::Done(v) if v == "abc"));
    r.replays(&FORK_KINDS);
}

#[test]
fn a_fork_in_a_layer_starts_in_its_view() {
    let mut r = Run::new(&FORK_KINDS);
    let out = r.open("cut", json!({"designer": "designer", "plan": "the median"}));
    assert!(out.effects.contains(&Effect::Fork { agent: "f200-1".into(), t: 200, call: syntax::INLINE.into(), layer: Some("K-200.l1".into()), from: "designer".into() }), "{:?}", out.effects);
}

/// On a slow ticket the fork's step halts before anything is started; approved, the fork is made then.
#[test]
fn a_halted_fork_is_made_when_its_step_is_approved() {
    let mut r = Run::new(&FORK_KINDS);
    r.open("slowly", json!({"designer": "designer", "plan": "the median"}));
    assert!(r.go(Input::Slow { t: 200, on: true, by: "owner".into() }).reply.is_ok());
    let out = r.answer(200, "owner", json!({"head": "Go"}));
    assert_eq!(forked(&out), vec![(200, "f200-1".to_string(), "designer".to_string())]);
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::Fork { .. })), "halted, not started: {:?}", out.effects);
    let n = r.m.tickets[&200].halts.iter().find(|h| matches!(h.what, Halting::Give { .. })).map(|h| h.n).expect("a give halt");
    let out = r.go(Input::Approve { t: 200, n, by: "owner".into() });
    assert!(out.effects.contains(&Effect::Fork { agent: "f200-1".into(), t: 200, call: syntax::INLINE.into(), layer: None, from: "designer".into() }), "{:?}", out.effects);
}

#[test]
fn what_a_fork_copies_is_an_agent_the_program_knows() {
    let bad = [
        ("a.kind", "kind a\n  form plan: Text\n  result Text\n  pattern\n    c <- step fork paper-research \"Build {{ plan }}\" -> Done;\n    return plan\n"),
        ("b.kind", "kind b\n  form plan: Text\n  result Text\n  pattern\n    let agent x = fork owner in { c <- step x \"Build {{ plan }}\" -> Done; return plan }\n"),
        ("c.kind", "kind c\n  form plan: Text\n  result Text\n  pattern\n    c <- step fork plan \"Build {{ plan }}\" -> Done;\n    return plan\n"),
        // the agent a let names is not in scope in its own `= fork`
        ("e.kind", "kind e\n  form plan: Text\n  result Text\n  pattern\n    let agent x = fork x in { c <- step x \"Build {{ plan }}\" -> Done; return plan }\n"),
    ];
    let (_, errors) = defs_with(&bad);
    let of = |k: &str| errors.iter().find(|e| e.contains(&format!("kind {k} refused"))).unwrap_or_else(|| panic!("{k}: {errors:#?}"));
    assert!(of("a").contains("`fork paper-research`: paper-research is not bound here"), "{}", of("a"));
    assert!(of("b").contains("the owner has no conversation to copy"), "{}", of("b"));
    assert!(of("c").contains("`fork plan`: this is a Text, not an agent"), "{}", of("c"));
    assert!(of("e").contains("`fork x`: x is not bound here"), "{}", of("e"));
}

#[test]
fn the_forms_print_as_written_and_parse_back() {
    let (d, errors) = defs_with(&FORK_KINDS);
    assert!(errors.is_empty(), "{errors:?}");
    let piece = crate::print::prog(&d.kinds["piece"].pattern, 0);
    assert!(piece.contains("let agent a = fork designer in {"), "{piece}");
    let once = crate::print::prog(&d.kinds["once"].pattern, 0);
    assert!(once.contains("step fork designer \"Build {{ plan }}\" -> Commit(c: Text)"), "{once}");
    for text in [piece, once] {
        assert_eq!(crate::print::prog(&syntax::parse_prog(&text, 1).unwrap(), 0), text);
    }
}

fn closed_with(out: &Out, t: u64) -> Option<Raised> {
    out.lines.iter().find_map(|l| match l {
        Line::Closed { t: c, raised, .. } if *c == t => raised.clone(),
        _ => None,
    })
}

/// coordd could not start the fork (no session to copy, a Codex agent, roster failed): the step owed to it raises
/// NoWorker {agent, why} at once, instead of waiting on an agent that never started.
#[test]
fn a_fork_that_could_not_be_started_raises_no_worker_on_its_step() {
    let mut r = Run::new(&FORK_KINDS);
    r.open("once", json!({"designer": "designer", "plan": "the median"}));
    let out = r.go(Input::StartFailed { t: 200, agent: "f200-1".into(), why: "designer has no Claude Code session to fork".into() });
    assert_eq!(out.reply, Ok("f200-1 could not be started: designer has no Claude Code session to fork. Its step on O-200 raises NoWorker.".into()));
    let raised = closed_with(&out, 200).expect("closed raising");
    assert_eq!(raised.exc, "NoWorker");
    assert_eq!(raised.value["agent"], json!("f200-1"));
    assert_eq!(raised.value["why"], json!("designer has no Claude Code session to fork"));
    r.replays(&FORK_KINDS);
}

/// A let agent's start failed while its ticket awaits someone else: its next step raises when it is reached.
#[test]
fn a_let_agent_that_could_not_be_started_fails_its_next_step() {
    let mut r = Run::new(&FORK_KINDS);
    r.open("later", json!({"designer": "designer", "plan": "the median"}));
    let out = r.go(Input::StartFailed { t: 200, agent: "f200-1".into(), why: "roster fork failed".into() });
    assert_eq!(out.reply, Ok("f200-1 could not be started: roster fork failed. Its next step raises NoWorker.".into()));
    assert!(closed_with(&out, 200).is_none());
    let out = r.answer(200, "owner", json!({"head": "Go"}));
    let raised = closed_with(&out, 200).expect("closed raising");
    assert_eq!((raised.exc.as_str(), &raised.value["agent"]), ("NoWorker", &json!("f200-1")));
    assert!(r.m.due("f200-1").is_empty());
    r.replays(&FORK_KINDS);
}
