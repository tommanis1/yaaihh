//! The stop (paper-research N-101 p934; machine/stop.rs): run 1's chain in small. A build opens a mend, which opens a
//! culprit, which opens a hand-up; the hand-up's first step reads `result of` a piece of work still open and raises
//! NoResult. Nothing above handles it, so the hand-up stops there, the lead is asked, and every ticket above it waits.

use super::*;
use crate::machine::HIDDEN;

const STOPPED: &str = include_str!("../../../../../agent-base/calls/stopped.call");

const CALLS: &str = "call assign(what: Text) -> Assigned\n  template\n    Assign {{ what }}.\n";

const KINDS: &str = "\
/// a piece of work on a red
kind work
  letter W
  form w: Text
  result Text
  pattern
    f <- step dev \"Fix {{ w }}\" -> Fixed;
    return w

/// a build of the cut
kind build
  letter B
  form on: Ticket
  result Text
  pattern
    m <- open mend(on);
    return m

/// a mend of a red build
kind mend
  letter M
  form on: Ticket
  result Text
  pattern
    c <- open culprit(on);
    return c

/// the culprit of a red
kind culprit
  letter C
  form on: Ticket
  result Text
  pattern
    h <- open handed_up(on);
    return h

/// the hand-up of a red nobody could place
kind handed_up
  letter H
  form on: Ticket
  result Text
  pattern
    a <- step orch assign(result of on);
    return \"assigned\"

/// a mend whose program handles NoResult
kind careful
  letter K
  form on: Ticket
  result Text
  pattern
    try { open culprit(on) } handle {
      NoResult(e) => return \"handled\"
    }

/// a change whose rounds run out: two designs, each sent back
kind rounds
  letter D
  form w: Text
  result Text
  raises Exhausted, Stalled
  pattern
    iterate seen = \"none\" up to 2 {
      g <- step dev \"Design {{ w }}, after {{ seen }}\" -> Good | Bad(why: Text);
      match g {
        Good => return Done(seen)
        Bad(why) => return Again(why)
      }
    }

/// a ticket whose program raises NoResult itself
kind raising
  letter R
  form on: Ticket
  result Text
  pattern
    raise NoResult(\"the program's own\")
";

/// The definitions: the test's kinds, with agent-base's `stopped` call when STOP.
fn defs(stop: bool) -> Defs {
    let mut named = BTreeMap::new();
    let mut files = vec![("t.call", CALLS)];
    if stop {
        files.push(("stopped.call", STOPPED));
    }
    let calls: BTreeMap<String, CallSig> = files.into_iter().flat_map(|(f, t)| syntax::parse_calls(f, t, &mut named).unwrap()).map(|c| (c.name.clone(), c)).collect();
    let (d, errors) = Defs::build(&[("t.kind".to_string(), KINDS.to_string())], calls, named, 200);
    assert!(errors.is_empty(), "{errors:#?}");
    d
}

fn open(r: &mut Run, kind: &str, form: Value) -> Out {
    let out = r.go(Input::Open { kind: kind.into(), under: None, opener: "owner".into(), form, text: None, names: vec!["orch".into()], told: vec![] });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    out
}

/// The run as far as the stop: the lead, the work W-200 (open, a step to dev), and the build B-201 on it, whose chain
/// stops at H-204.
fn stopped(stop: bool) -> (Run, Out) {
    let mut r = Run { m: Machine::load(defs(stop)), log: Vec::new(), clock: 0 };
    r.go(Input::Role { agent: "paper-research".into(), role: "lead".into(), also: vec![] }).reply.unwrap();
    open(&mut r, "work", json!({"w": "case-4"}));
    let out = open(&mut r, "build", json!({"on": "W-200"}));
    (r, out)
}

fn closed(r: &Run, t: u64) -> Option<Raised> {
    r.log.iter().find_map(|l| match l {
        Line::Closed { t: c, raised, .. } if *c == t => Some(raised.clone().unwrap_or(Raised { exc: String::new(), value: Value::Null })),
        _ => None,
    })
}

fn waits(r: &Run, t: u64) -> bool {
    matches!(r.m.tickets[&t].status, Status::Waits { .. })
}

/// The stop's step, as the one it is given is asked it: to whom, and its three arguments.
fn step_of(r: &Run, c: u64) -> (String, Value) {
    match &r.m.tickets[&c].status {
        Status::Awaits { agent, owed, .. } if owed.call == "stopped" => (agent.clone(), owed.args.clone()),
        s => panic!("{s:?}"),
    }
}

/// Run 1's chain: the hand-up's first step raises NoResult; nothing above handles it, so it stops, the lead is given
/// `stopped` with every ticket by label and id, and the build, mend and culprit still wait. Nothing closes, so the
/// policy is told of no close (run 1's `cut-build` would wait).
#[test]
fn a_failure_nothing_above_handles_stops_where_it_was_raised_and_goes_to_the_lead() {
    let (r, out) = stopped(true);
    let Status::Stopped { raised, to, by, .. } = &r.m.tickets[&204].status else { panic!("{:?}", r.m.tickets[&204].status) };
    assert_eq!((raised.exc.as_str(), raised.value["why"].as_str(), to.as_str(), *by), ("NoResult", Some("W-200 is open"), "paper-research", HIDDEN));
    assert!(out.lines.contains(&Line::Stopped { at: "2026-11-10T12:00:03Z".into(), t: 204, raised: raised.clone(), to: "paper-research".into() }), "{:?}", out.lines);
    for t in [201, 202, 203] {
        assert!(waits(&r, t), "{t}: {:?}", r.m.tickets[&t].status);
    }
    for t in 201..=204 {
        assert_eq!(closed(&r, t), None);
    }
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::Notify { event, .. } if event == "closed")), "{:?}", out.effects);
    let (agent, args) = step_of(&r, HIDDEN);
    assert_eq!(agent, "paper-research");
    assert_eq!(args, json!({
        "place": "the hand-up of a red nobody could place (H-204) stopped at its first step",
        "failure": "W-200 is open",
        "from": "It came from: the culprit of a red C-203, from a mend of a red build M-202, from a build of the cut B-201.",
    }));
    // shown on the ticket that stopped, waiting on the lead
    let v = r.m.view(204).unwrap();
    assert!(matches!(&v.status, ViewStatus::Stopped { to, by, .. } if to == "paper-research" && by == &r.m.id(HIDDEN)), "{:?}", v.status);
    assert!(v.waits_on.iter().any(|w| w.agent == "paper-research" && w.call == "stopped"), "{:?}", v.waits_on);
    // what the step gives, from the call's docs
    let files = vec![("stopped.call".to_string(), STOPPED.to_string())];
    let mut named = syntax::Named::new();
    let (cs, errors) = syntax::parse_call_files(&files, &mut named);
    assert!(errors.is_empty(), "{errors:?}");
    let c = &cs["stopped"];
    assert_eq!(crate::doc::gives(&c.sig.head, Some(&c.doc), &syntax::type_docs(&files), &named), "\
This step gives one of:
  Again  the cause is fixed; it runs again from where it stopped
  Ended  given up: this ticket ends failed, and so do those above it that do not handle it
    why  one sentence on why it is given up, which the owner reads");
}

/// Again runs the node it stopped at again. While the work is still open it stops again at the same place, and that
/// second stop goes to the owner; once the work is done, Again goes on: the step is given and the chain closes done.
#[test]
fn again_runs_the_node_again_and_a_second_stop_there_goes_to_the_owner() {
    let (mut r, _) = stopped(true);
    r.answer(HIDDEN, "paper-research", json!({"head": "Again"}));
    let Status::Stopped { to, by, .. } = &r.m.tickets[&204].status else { panic!("{:?}", r.m.tickets[&204].status) };
    assert_eq!((to.as_str(), *by), ("owner", HIDDEN + 1));
    assert_eq!(step_of(&r, HIDDEN + 1).0, "owner");
    // the cause fixed: the work is done
    r.answer(200, "dev", json!({"head": "Fixed"}));
    r.answer(HIDDEN + 1, "owner", json!({"head": "Again"}));
    let Status::Awaits { agent, owed, .. } = &r.m.tickets[&204].status else { panic!("{:?}", r.m.tickets[&204].status) };
    assert_eq!((agent.as_str(), owed.step), ("orch", 1));
    r.answer(204, "orch", json!({"head": "Assigned"}));
    for t in 201..=204 {
        assert!(matches!(&r.m.tickets[&t].status, Status::Done(v) if v == "assigned"), "{t}: {:?}", r.m.tickets[&t].status);
    }
    assert_eq!(Machine::load(defs(true)).replay(&r.log), Vec::<String>::new());
}

/// Where a ticket runs again from (`Ticket.last_node`) is kept only where a stop can read it (paper-research N-1446
/// p1936; research's copy, p1932: closed delivery runs kept what they were given a second time in it, 3.6 GB): the
/// hand-up stopped keeps its node, the stop's own hidden ticket, which handles the stop and never stops itself
/// (`stops_at`), keeps none while it waits on the lead, and once the chain is done no ticket keeps one, nor a layer's;
/// the record replays as it ran.
#[test]
fn only_a_ticket_that_can_stop_keeps_where_it_runs_again_from() {
    let (mut r, _) = stopped(true);
    assert!(r.m.tickets[&204].last_node.is_some(), "the hand-up keeps the node it stopped at");
    let hidden = &r.m.tickets[&HIDDEN];
    assert!(hidden.handling.is_some() && hidden.is_open(), "the stop's run handles the stop, and waits on the lead: {:?}", hidden.status);
    assert_eq!(hidden.last_node, None, "a run that handles keeps no node to run again from");
    r.answer(200, "dev", json!({"head": "Fixed"}));
    r.answer(HIDDEN, "paper-research", json!({"head": "Again"}));
    assert!(matches!(&r.m.tickets[&204].status, Status::Awaits { agent, .. } if agent == "orch"), "{:?}", r.m.tickets[&204].status);
    assert!(r.m.tickets[&204].last_node.is_some(), "at its step again, it keeps that node");
    r.answer(204, "orch", json!({"head": "Assigned"}));
    for t in [200, 201, 202, 203, 204, HIDDEN] {
        let x = &r.m.tickets[&t];
        assert!(!x.is_open(), "{t}: {:?}", x.status);
        assert_eq!((&x.last_node, x.layer_nodes.len()), (&None, 0), "{t}: closed, it keeps no node to run again from");
    }
    assert_eq!(Machine::load(defs(true)).replay(&r.log), Vec::<String>::new());
}

/// A stop whose step went to an agent that will never take it (Tovrin-dev, 17 Nov: S-1099511627813 and -814 at the
/// ended orch; paper-research N-101 p1473): the owner cancels the stop's hidden ticket, the ticket that stopped is
/// stopped again and asked of the owner, and the owner's Again runs its node again. Nothing above it is cancelled.
#[test]
fn a_stops_step_cancelled_by_the_owner_is_asked_of_the_owner() {
    let (mut r, _) = stopped(true);
    r.go(Input::Cancel { t: HIDDEN, by: "owner".into(), why: "its lead was ended".into() }).reply.unwrap();
    let Status::Stopped { to, by, .. } = &r.m.tickets[&204].status else { panic!("{:?}", r.m.tickets[&204].status) };
    assert_eq!((to.as_str(), *by), ("owner", HIDDEN + 1));
    assert_eq!(step_of(&r, HIDDEN + 1).0, "owner");
    for t in [201, 202, 203] {
        assert!(waits(&r, t), "{t}: {:?}", r.m.tickets[&t].status);
    }
    r.answer(200, "dev", json!({"head": "Fixed"}));
    r.answer(HIDDEN + 1, "owner", json!({"head": "Again"}));
    let Status::Awaits { agent, owed, .. } = &r.m.tickets[&204].status else { panic!("{:?}", r.m.tickets[&204].status) };
    assert_eq!((agent.as_str(), owed.step), ("orch", 1));
    assert_eq!(Machine::load(defs(true)).replay(&r.log), Vec::<String>::new());
}

/// Ended unwinds the failure as a raise does: the hand-up, the culprit, the mend and the build each close failed with
/// it, and the build's close is told to the policy.
#[test]
fn ended_unwinds_through_every_ticket_above() {
    let (mut r, _) = stopped(true);
    let out = r.go(Input::Answer { t: HIDDEN, by: "paper-research".into(), head: json!({"head": "Ended", "why": "the piece is dropped"}), text: Text::New("ended".into()) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    for t in [204, 203, 202, 201] {
        assert!(matches!(closed(&r, t), Some(x) if x.exc == "NoResult" && x.value["why"] == "W-200 is open"), "{t}: {:?}", closed(&r, t));
    }
    assert!(out.effects.iter().any(|e| matches!(e, Effect::Notify { t: 201, event, .. } if event == "closed")), "{:?}", out.effects);
    assert!(!r.log.iter().any(|l| matches!(l, Line::Stopped { t: u, .. } if *u != 204)));
    assert_eq!(Machine::load(defs(true)).replay(&r.log), Vec::<String>::new());
}

/// What does not stop: a cancel (and a stopped ticket cancelled ends its stop's step), the program's own `raise`, and
/// a failure a `try` above takes.
#[test]
fn a_cancel_a_programs_raise_and_a_handled_failure_still_unwind() {
    let (mut r, _) = stopped(true);
    r.go(Input::Cancel { t: 201, by: "owner".into(), why: "not needed".into() }).reply.unwrap();
    for t in [201, 202, 203, 204] {
        assert!(matches!(&r.m.tickets[&t].status, Status::Failed(x) if x.exc == "Cancelled"), "{t}: {:?}", r.m.tickets[&t].status);
    }
    assert!(!r.m.tickets[&HIDDEN].is_open());
    assert_eq!(r.log.iter().filter(|l| matches!(l, Line::Stopped { .. })).count(), 1);
    // the program's own raise of a name that would stop
    open(&mut r, "raising", json!({"on": "W-200"}));
    let x = &r.m.tickets[&205];
    assert!(matches!(&x.status, Status::Failed(e) if e.exc == "NoResult"), "{:?}", x.status);
    // a `try` two tickets above takes it: it unwinds to there
    open(&mut r, "careful", json!({"on": "W-200"}));
    assert!(matches!(&r.m.tickets[&206].status, Status::Done(v) if v == "handled"), "{:?}", r.m.tickets[&206].status);
    assert!(matches!(&r.m.tickets[&208].status, Status::Failed(e) if e.exc == "NoResult"), "{:?}", r.m.tickets[&208].status);
    assert_eq!(r.log.iter().filter(|l| matches!(l, Line::Stopped { .. })).count(), 1);
    assert_eq!(Machine::load(defs(true)).replay(&r.log), Vec::<String>::new());
}

/// Run 1's record, as the engine wrote it before the stop: the chain closed failed, each with the NoResult. It
/// replays as recorded, and the record of a stop replays as a stop.
#[test]
fn an_old_record_of_the_chain_replays_as_recorded() {
    // before: no `stopped` call, so nothing stops
    let (old, _) = stopped(false);
    for t in 201..=204 {
        assert!(matches!(closed(&old, t), Some(x) if x.exc == "NoResult"), "{t}");
    }
    let mut m = Machine::load(defs(true));
    assert_eq!(m.replay(&old.log), Vec::<String>::new());
    for t in 201..=204 {
        assert!(matches!(&m.tickets[&t].status, Status::Failed(x) if x.exc == "NoResult"), "{t}: {:?}", m.tickets[&t].status);
    }
    assert!(!m.tickets.contains_key(&HIDDEN));
    let (new, _) = stopped(true);
    let mut m = Machine::load(defs(true));
    assert_eq!(m.replay(&new.log), Vec::<String>::new());
    assert!(matches!(&m.tickets[&204].status, Status::Stopped { .. }));
    assert_eq!(step_of(&Run { m, log: Vec::new(), clock: 0 }, HIDDEN).0, "paper-research");
}

/// A step whose answer is not read twice stops its ticket there (paper-research N-101 p999, 2b): the lead is asked
/// Again or Ended, and the step is not handed to another agent. Again gives it to the agent the program names, afresh.
#[test]
fn a_step_answered_twice_and_read_neither_time_stops_and_again_gives_it_afresh() {
    let mut r = Run { m: Machine::load(defs(true)), log: Vec::new(), clock: 0 };
    r.go(Input::Role { agent: "paper-research".into(), role: "lead".into(), also: vec![] }).reply.unwrap();
    open(&mut r, "work", json!({"w": "case-4"}));
    for n in 0..2u64 {
        r.take("dev");
        r.go(Input::TurnEnded { agent: "dev".into(), delivery: None, text: Run::loc(2 + n), later: vec![], stood: None, go: None });
        r.go(Input::Formed { t: 200, step: 1, head: None, why: Some(format!("answer {} gives no outcome", n + 1)), not_taken: None, call: None }).reply.unwrap();
    }
    assert!(!r.log.iter().any(|l| matches!(l, Line::PassedUp { .. })), "{:?}", r.log);
    let Status::Stopped { raised, to, by, .. } = &r.m.tickets[&200].status else { panic!("{:?}", r.m.tickets[&200].status) };
    assert_eq!((raised.exc.as_str(), to.as_str(), *by), ("Malformed", "paper-research", HIDDEN));
    let (agent, args) = step_of(&r, HIDDEN);
    assert_eq!(agent, "paper-research");
    assert_eq!(args["failure"], json!("dev answered step 1 twice and neither answer was read as one of Fixed: answer 2 gives no outcome"));
    // Again: the step is dev's again, given afresh, with no reminder owed
    r.answer(HIDDEN, "paper-research", json!({"head": "Again"}));
    let Status::Awaits { agent, reminded, .. } = &r.m.tickets[&200].status else { panic!("{:?}", r.m.tickets[&200].status) };
    assert_eq!((agent.as_str(), *reminded), ("dev", None));
    let d = r.take("dev");
    assert!(matches!(&d.what, What::Step { again: false, .. }), "{d:?}");
    r.answer(200, "dev", json!({"head": "Fixed"}));
    assert!(matches!(&r.m.tickets[&200].status, Status::Done(v) if v == "case-4"), "{:?}", r.m.tickets[&200].status);
    assert_eq!(Machine::load(defs(true)).replay(&r.log), Vec::<String>::new());
}

/// A duty broken, as the lead is shown it at the stop: in words from its fields, not the policy's raw value
/// (docket-core3, N-101 p1047, 2).
#[test]
fn a_broken_duty_reads_in_words_on_the_stop() {
    let (r, _) = stopped(true);
    let v = json!({"head": "Violated", "answered": {"head": "Commit", "commit": {"a": "22329bbf00", "b": "5e9916d7c3cbc121bafc631132a490bef67dc14b"}}, "duty": "owes-typecheck", "holder": "about-note", "claimant": "runner", "work": "T-202"});
    assert_eq!(
        r.m.failure(&Raised { exc: "Violated".into(), value: v }),
        "about-note owes runner `owes-typecheck` on the work T-202 (M-202), and answered Commit a 22329bb b 5e9916d while it was still owed."
    );
}

/// A change whose rounds ran out (run 4, paper-research N-101 p1141, 1): it stops and the lead is given `stopped` with
/// the last round's word, instead of the change closing failed with nobody holding a step. Again is one more round,
/// from where the last left off; it running out again goes to the owner; Ended closes it failed as before.
#[test]
fn a_change_whose_rounds_ran_out_stops_and_again_is_one_more_round() {
    let mut r = Run { m: Machine::load(defs(true)), log: Vec::new(), clock: 0 };
    r.go(Input::Role { agent: "paper-research".into(), role: "lead".into(), also: vec![] }).reply.unwrap();
    open(&mut r, "rounds", json!({"w": "engines"}));
    let d = 200;
    for why in ["NoOrder: a", "NoOrder: b"] {
        r.answer(d, "dev", json!({"head": "Bad", "why": why}));
    }
    let Status::Stopped { raised, to, .. } = &r.m.tickets[&d].status else { panic!("{:?}", r.m.tickets[&d].status) };
    assert_eq!((raised.exc.as_str(), to.as_str()), ("Exhausted", "paper-research"));
    assert_eq!(closed(&r, d), None, "not closed failed");
    let (agent, args) = step_of(&r, HIDDEN);
    assert_eq!(agent, "paper-research");
    assert!(args["failure"].as_str().is_some_and(|f| f.contains("NoOrder: b")), "{args}");
    // Again: one round more, the last word in its step
    r.answer(HIDDEN, "paper-research", json!({"head": "Again"}));
    let Status::Awaits { agent, owed, .. } = &r.m.tickets[&d].status else { panic!("{:?}", r.m.tickets[&d].status) };
    assert_eq!(agent, "dev");
    assert!(owed.template.as_deref().unwrap_or_default().contains("after NoOrder: b") || owed.args.to_string().contains("NoOrder: b"), "{owed:?}");
    // it runs out again: the owner's; Ended closes it failed
    r.answer(d, "dev", json!({"head": "Bad", "why": "NoOrder: d"}));
    let Status::Stopped { to, by, .. } = r.m.tickets[&d].status.clone() else { panic!("{:?}", r.m.tickets[&d].status) };
    assert_eq!(to, "owner");
    r.go(Input::Answer { t: by, by: "owner".into(), head: json!({"head": "Ended", "why": "no design"}), text: Text::New("ended".into()) }).reply.unwrap();
    assert!(matches!(closed(&r, d), Some(x) if x.exc == "Exhausted"), "{:?}", closed(&r, d));
    assert_eq!(Machine::load(defs(true)).replay(&r.log), Vec::<String>::new());
}

/// A `for` over more than its most still closes failed: only an `iterate` whose rounds ran out stops.
#[test]
fn a_for_over_its_most_still_unwinds() {
    let kinds = "kind many\n  letter F\n  form w: Text\n  result Text\n  raises Exhausted\n  pattern\n    xs <- for x in [w, w, w] up to 2 { return x };\n    return w\n";
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = [("t.call", CALLS), ("stopped.call", STOPPED)].into_iter().flat_map(|(f, t)| syntax::parse_calls(f, t, &mut named).unwrap()).map(|c| (c.name.clone(), c)).collect();
    let (d, errors) = Defs::build(&[("t.kind".to_string(), kinds.to_string())], calls, named, 200);
    assert!(errors.is_empty(), "{errors:#?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    r.go(Input::Role { agent: "paper-research".into(), role: "lead".into(), also: vec![] }).reply.unwrap();
    open(&mut r, "many", json!({"w": "a"}));
    assert!(matches!(closed(&r, 200), Some(x) if x.exc == "Exhausted"), "{:?}", r.m.tickets[&200].status);
}
