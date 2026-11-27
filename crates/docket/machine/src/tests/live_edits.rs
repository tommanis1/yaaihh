//! Live edits (paper-research N-101 p424, on docket-core's p423): at a reload every open ticket of an edited kind
//! takes the new text for what it has not run, the place it stands at carried from the old pattern to the new one;
//! what is in flight stays when the new text has its node and is withdrawn when it does not; a ticket whose new
//! remainder reads a name it never bound keeps its old text, and the reply says why. Each case ends with a restart
//! on kind files changed again: the record restores the same state.

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const CALLS: &str = "\
call one(words: Text) -> Said(text: Text)
  template
    One {{ words }}.

call two(text: Text) -> Said(text: Text)
  template
    Two {{ text }}.

call three(text: Text) -> Said(text: Text)
  template
    Three {{ text }}.

call zero() -> Said(text: Text)
  template
    Zero.

call round(x: Text) -> Green(top: Text) | Red(why: Text)
  template
    Round {{ x }}.
";

const CHANGE: &str = "\
kind change
  form words: Text
  result Text
  pattern
    a <- step ann one(words);
    b <- step ann two(a.text);
    return b.text
";

const LOOP: &str = "\
kind loop
  form words: Text
  result Text
  raises Exhausted, Stalled
  pattern
    iterate x = words up to 5 {
      g <- step ann round(x);
      match g {
        Green(top) => return Done(top)
        Red(why) => return Again(why)
      }
    }
";

/// Kind files changed again under the restart: what a ticket runs comes from the record, never from these.
fn later() -> String {
    ["change", "loop", "fan", "holey", "scoped", "tried"]
        .iter()
        .map(|k| format!("kind {k}\n  form words: Text\n  result Text\n  raises Exhausted, Stalled\n  pattern\n    q <- step ann zero();\n    return q.text\n"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn defs(kinds: &str) -> Defs {
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = syntax::parse_calls("t.call", CALLS, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
    let (d, errors) = Defs::build(&[("t.kind".to_string(), kinds.to_string())], calls, named, 300);
    assert!(errors.is_empty(), "{errors:#?}");
    d
}

struct Run {
    m: Machine,
    log: Vec<Line>,
    clock: u32,
}

impl Run {
    fn new(kinds: &str) -> Run {
        Run { m: Machine::load(defs(kinds)), log: Vec::new(), clock: 0 }
    }
    fn go(&mut self, i: Input) -> Out {
        self.clock += 1;
        let out = self.m.apply(i, format!("2026-11-16T14:{:02}:{:02}Z", self.clock / 60, self.clock % 60));
        self.log.extend(out.lines.iter().cloned());
        out
    }
    fn open(&mut self, kind: &str) -> u64 {
        self.go(Input::Open { kind: kind.into(), under: None, opener: "owner".into(), form: json!({"words": "w0"}), text: None, names: vec!["ann".into()], told: vec![] }).reply.unwrap();
        let t = *self.m.tickets.range(..HIDDEN).filter(|(_, x)| x.kind == kind).next_back().unwrap().0;
        self.give();
        t
    }
    /// What is due to ann and to the agents the tickets made, handed to them.
    fn give(&mut self) {
        let mut agents = vec![String::from("ann")];
        agents.extend(self.m.tickets.values().flat_map(|x| x.kept.iter().chain(&x.live).cloned()));
        for a in agents {
            for d in self.m.due(&a) {
                self.go(Input::Given { t: d.t, to: a.clone(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Loc { file: "t.jsonl".into(), offset: 1, len: 1 }) });
            }
        }
    }
    fn answer(&mut self, t: u64, head: Value) -> Out {
        let by = match &self.m.tickets[&t].status {
            Status::Awaits { agent, .. } => agent.clone(),
            _ => String::from("ann"),
        };
        let out = self.go(Input::Answer { t, by, head, text: Text::New("as said".into()) });
        self.give();
        out
    }
    fn said(&mut self, t: u64, text: &str) {
        let r = self.answer(t, json!({"head": "Said", "text": text})).reply;
        assert!(r.is_ok(), "{r:?}");
    }
    /// The kinds read again as KINDS (coordd swaps the definitions, records the reload, then has each open ticket of
    /// an edited kind take the new text): each one's reply.
    fn reload(&mut self, kinds: &str) -> BTreeMap<u64, Result<String, String>> {
        *self.m.defs.edit() = defs(kinds);
        self.go(Input::Reload { by: "owner".into(), why: "edited".into(), commit: None, kinds: vec!["change".into(), "loop".into()], calls: vec![] }).reply.unwrap();
        let open: Vec<u64> = self.m.tickets.values().filter(|x| x.is_open() && x.t < HIDDEN).map(|x| x.t).collect();
        let out = open.into_iter().map(|t| (t, self.go(Input::Retext { t, by: "owner".into(), why: "edited".into() }).reply)).collect();
        self.give();
        out
    }
    fn owed(&self, t: u64) -> (u32, String, Value) {
        match &self.m.tickets[&t].status {
            Status::Awaits { owed, .. } => (owed.step, owed.call.clone(), owed.args.clone()),
            s => panic!("{s:?}"),
        }
    }
    /// A restart on kind files changed again: the record replays with no difference, to the same tickets.
    fn restart(&self) {
        let lines: Vec<Line> = self.log.iter().map(|l| serde_json::from_str(&serde_json::to_string(l).unwrap()).unwrap()).collect();
        let mut m2 = Machine::load(defs(&later()));
        assert_eq!(m2.replay(&lines), Vec::<String>::new());
        for t in self.m.tickets.keys() {
            assert_eq!(m2.view(*t), self.m.view(*t), "ticket {t}");
            assert_eq!(m2.tickets[t].k, self.m.tickets[t].k, "ticket {t}'s frames");
        }
    }
}

fn loops(kinds: &str) -> String {
    format!("{kinds}\n{LOOP}")
}

/// A ticket before the edited place takes it: it awaits the same step, and after it runs what the new text has.
#[test]
fn a_ticket_before_the_edit_takes_it() {
    let mut r = Run::new(&loops(CHANGE));
    let t = r.open("change");
    assert_eq!(r.owed(t).1, "one");
    let edited = CHANGE.replace("    b <- step ann two(a.text);\n", "    c <- step ann three(a.text);\n    b <- step ann two(c.text);\n");
    let said = r.reload(&loops(&edited));
    assert_eq!(said[&t], Ok("took the new text".into()));
    assert!(r.log.iter().any(|l| matches!(l, Line::Edited { t: u, what, program: Some(p), calls, .. } if *u == t && what == "reload" && p.contains("three(a.text)") && calls.contains_key("three"))), "the record has the text it runs and its calls");
    // the step given stays, as given
    assert_eq!(r.owed(t).0, 1);
    r.said(t, "a1");
    assert_eq!(r.owed(t), (2, "three".into(), json!({"text": "a1"})));
    r.said(t, "c1");
    assert_eq!(r.owed(t), (3, "two".into(), json!({"text": "c1"})));
    r.said(t, "b1");
    assert_eq!(r.m.tickets[&t].status, Status::Done(json!("b1")));
    r.restart();
}

/// A ticket past the edited place goes on in the new text; what it had done stays as it was, and its picture shows
/// it done in the new text.
#[test]
fn a_ticket_past_the_edit_goes_on_in_the_new_text_with_its_history() {
    let mut r = Run::new(&loops(CHANGE));
    let t = r.open("change");
    r.said(t, "a1");
    assert_eq!(r.owed(t).1, "two");
    let posts = r.m.tickets[&t].posts.clone();
    let edited = CHANGE.replace("one(words)", "one(\"something else\")").replace("    return b.text\n", "    c <- step ann three(b.text);\n    return c.text\n");
    assert_eq!(r.reload(&loops(&edited))[&t], Ok("took the new text".into()));
    assert_eq!(r.m.tickets[&t].posts, posts, "history unchanged");
    assert_eq!(r.owed(t).0, 2);
    let run = r.m.view(t).unwrap().run;
    assert_eq!((run[0].text.as_str(), run[0].state.as_str()), ("a <- step ann one(\"something else\")", "done"), "{run:#?}");
    assert_eq!(run[1].state, "now");
    assert_eq!(run[2].state, "to_come");
    r.said(t, "b1");
    assert_eq!(r.owed(t), (3, "three".into(), json!({"text": "b1"})));
    r.said(t, "c1");
    assert_eq!(r.m.tickets[&t].status, Status::Done(json!("c1")));
    r.restart();
}

/// A ticket inside an `iterate` finishes the round it is in and takes the new body from the next round.
#[test]
fn a_ticket_inside_an_iterate_gets_the_new_body_from_the_next_round() {
    let mut r = Run::new(&loops(CHANGE));
    let t = r.open("loop");
    assert_eq!(r.owed(t).1, "round");
    let edited = LOOP.replace("      g <- step ann round(x);\n", "      p <- step ann two(x);\n      g <- step ann round(p.text);\n");
    assert_eq!(r.reload(&format!("{CHANGE}\n{edited}"))[&t], Ok("took the new text".into()));
    assert_eq!(r.owed(t).1, "round", "the round it is in goes on where it stands");
    r.answer(t, json!({"head": "Red", "why": "r1"})).reply.unwrap();
    assert_eq!(r.owed(t), (2, "two".into(), json!({"text": "r1"})), "the next round is the new body");
    r.said(t, "p1");
    assert_eq!(r.owed(t), (3, "round".into(), json!({"x": "p1"})));
    r.answer(t, json!({"head": "Green", "top": "g1"})).reply.unwrap();
    assert_eq!(r.m.tickets[&t].status, Status::Done(json!("g1")));
    r.restart();
}

/// A step in flight whose node the new text removed is withdrawn, its agent told, and the ticket goes on from what
/// follows it in the new text.
#[test]
fn a_step_in_flight_whose_node_is_gone_is_withdrawn_and_the_ticket_goes_on() {
    let mut r = Run::new(&loops(CHANGE));
    let t = r.open("change");
    r.said(t, "a1");
    assert_eq!(r.owed(t).1, "two");
    let edited = CHANGE.replace("    b <- step ann two(a.text);\n    return b.text\n", "    return a.text\n");
    let said = r.reload(&loops(&edited));
    assert_eq!(said[&t], Ok("took the new text; step 2 (two) of ann withdrawn: its statement is gone".into()));
    let x = &r.m.tickets[&t];
    assert!(x.withdrawn.iter().any(|w| w.0 == "ann" && w.1 == 2 && w.2 == "two" && w.3), "{:?}", x.withdrawn);
    assert_eq!(x.status, Status::Done(json!("a1")));
    assert!(r.log.iter().any(|l| matches!(l, Line::Given { delivery, .. } if delivery.ends_with(".s2.withdrawn") || delivery.contains("withdrawn"))), "ann is told: {:#?}", r.log.iter().filter(|l| matches!(l, Line::Given { .. })).collect::<Vec<_>>());
    // her late answer is no longer taken
    assert!(r.answer(t, json!({"head": "Said", "text": "b1"})).reply.is_err());
    r.restart();
}

/// A ticket whose new remainder reads a name bound by a statement it is already past keeps its old text, and the
/// reply says which name and line; it then runs as before.
#[test]
fn a_remainder_reading_a_name_never_bound_keeps_the_old_text() {
    let mut r = Run::new(&loops(CHANGE));
    let t = r.open("change");
    r.said(t, "a1");
    let edited = CHANGE.replace("    b <- step ann two(a.text);\n    return b.text\n", "    z <- step ann zero();\n    b <- step ann two(a.text);\n    return z.text\n");
    let said = r.reload(&loops(&edited));
    let e = said[&t].clone().unwrap_err();
    assert!(e.starts_with("kept its old text: `z` (line 8 of the new text) is not bound where it stands"), "{e}");
    assert!(!r.log.iter().any(|l| matches!(l, Line::Edited { what, .. } if what == "reload")), "nothing recorded for it");
    // a ticket opened now runs the new text; this one the old
    r.said(t, "b1");
    assert_eq!(r.m.tickets[&t].status, Status::Done(json!("b1")));
    r.restart();
}

/// A call whose answers changed does not withdraw the step in flight: the answer is taken by the new arms.
#[test]
fn a_step_in_flight_takes_its_answer_by_the_new_arms() {
    let calls_then = CALLS.replace("call two(text: Text) -> Said(text: Text)", "call two(text: Text) -> Said(text: Text) | Nope(why: Text)");
    let mk = |calls: &str, kinds: &str| {
        let mut named = BTreeMap::new();
        let calls: BTreeMap<String, CallSig> = syntax::parse_calls("t.call", calls, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
        let (d, errors) = Defs::build(&[("t.kind".to_string(), kinds.to_string())], calls, named, 300);
        assert!(errors.is_empty(), "{errors:#?}");
        d
    };
    let mut r = Run { m: Machine::load(mk(CALLS, &loops(CHANGE))), log: Vec::new(), clock: 0 };
    let t = r.open("change");
    r.said(t, "a1");
    let edited = CHANGE.replace("    return b.text\n", "    match b {\n      Said(text) => return text\n      Nope(why) => return why\n    }\n");
    *r.m.defs.edit() = mk(&calls_then, &loops(&edited));
    r.go(Input::Reload { by: "owner".into(), why: "edited".into(), commit: None, kinds: vec!["change".into()], calls: vec!["two".into()] }).reply.unwrap();
    assert_eq!(r.go(Input::Retext { t, by: "owner".into(), why: "edited".into() }).reply, Ok("took the new text".into()));
    r.answer(t, json!({"head": "Nope", "why": "not now"})).reply.unwrap();
    assert_eq!(r.m.tickets[&t].status, Status::Done(json!("not now")));
    let lines: Vec<Line> = r.log.iter().map(|l| serde_json::from_str(&serde_json::to_string(l).unwrap()).unwrap()).collect();
    let mut m2 = Machine::load(defs(&later()));
    assert_eq!(m2.replay(&lines), Vec::<String>::new());
    assert_eq!(m2.view(t), r.m.view(t));
}

const FAN: &str = "\
kind fan
  form words: Text
  result Text
  pattern
    (x, y) <- par ((a <- step ann one(words); return a.text), (b <- step ann two(words); return b.text));
    return y
";

/// A ticket waiting on the branches of a `par`: each open branch takes the new text of its branch.
#[test]
fn the_open_branches_of_a_par_take_the_new_text_of_theirs() {
    let mut r = Run::new(&loops(&format!("{CHANGE}\n{FAN}")));
    let t = r.open("fan");
    let kids = r.m.tickets[&t].children.clone();
    assert_eq!(kids.len(), 2);
    r.said(kids[0], "one done");
    let edited = FAN.replace("(b <- step ann two(words); return b.text)", "(b <- step ann two(words); c <- step ann three(b.text); return c.text)");
    let said = r.reload(&loops(&format!("{CHANGE}\n{edited}")));
    assert_eq!(said[&t], Ok("took the new text".into()));
    r.said(kids[1], "two done");
    assert_eq!(r.owed(kids[1]), (2, "three".into(), json!({"text": "two done"})));
    r.said(kids[1], "three done");
    assert_eq!(r.m.tickets[&t].status, Status::Done(json!("three done")));
    r.restart();
}

const HOLEY: &str = "\
kind holey
  form words: Text
  result Text
  pattern
    a <- step ann one(words);
    ...
";

/// A ticket resting at a hole the new text keeps rests there still (what was inserted before it is behind it); one
/// whose hole is gone goes on from what follows.
#[test]
fn a_hole_kept_rests_and_a_hole_gone_goes_on() {
    let mut r = Run::new(&loops(&format!("{CHANGE}\n{HOLEY}")));
    let t = r.open("holey");
    r.said(t, "a1");
    assert!(matches!(r.m.tickets[&t].status, Status::Rests { .. }));
    let kept = HOLEY.replace("    ...\n", "    b <- step ann two(a.text);\n    ...\n");
    assert_eq!(r.reload(&loops(&format!("{CHANGE}\n{kept}")))[&t], Ok("took the new text".into()));
    assert!(matches!(&r.m.tickets[&t].status, Status::Rests { scope, .. } if scope.contains_key("a") && !scope.contains_key("b")), "{:?}", r.m.tickets[&t].status);
    let gone = kept.replace("    b <- step ann two(a.text);\n    ...\n", "    b <- step ann two(a.text);\n    return a.text\n");
    assert_eq!(r.reload(&loops(&format!("{CHANGE}\n{gone}")))[&t], Ok("took the new text; its hole is gone".into()));
    assert_eq!(r.m.tickets[&t].status, Status::Done(json!("a1")));
    r.restart();
}

const SCOPED: &str = "\
kind scoped
  form words: Text
  result Text
  pattern
    a <- step ann one(words);
    r <- let agent m in {
      b <- step m two(a.text);
      return b.text
    };
    return r
";

/// A `let agent` block the new text no longer has: the step in flight in it is withdrawn, its agent ends, and the
/// ticket goes on after the last statement before it that the new text kept.
#[test]
fn a_let_agent_gone_ends_its_agent_and_withdraws_its_step() {
    let mut r = Run::new(&loops(&format!("{CHANGE}\n{SCOPED}")));
    let t = r.open("scoped");
    r.said(t, "a1");
    let Status::Awaits { agent, owed, .. } = r.m.tickets[&t].status.clone() else { panic!() };
    assert_eq!(owed.call, "two");
    let edited = SCOPED.replace("    r <- let agent m in {\n      b <- step m two(a.text);\n      return b.text\n    };\n    return r\n", "    return a.text\n");
    let said = r.reload(&loops(&format!("{CHANGE}\n{edited}")));
    assert_eq!(said[&t], Ok(format!("took the new text; step 2 (two) of {agent} withdrawn: its statement is gone")));
    let x = &r.m.tickets[&t];
    assert!(x.kept.is_empty(), "{:?}", x.kept);
    assert!(r.log.iter().any(|l| matches!(l, Line::Agent { agent: a, what, .. } if *a == agent && what == "ended")), "its agent ends");
    assert_eq!(x.status, Status::Done(json!("a1")));
    r.restart();
}

const TRIED: &str = "\
kind tried
  form words: Text
  result Text
  pattern
    try {
      a <- step ann one(words);
      return a.text
    } handle {
      NoWorker(e) => return \"nobody\"
    }
";

/// A ticket inside a `try`: its frame takes the new arms, and the body goes on in the new text.
#[test]
fn a_ticket_inside_a_try_takes_the_new_body_and_arms() {
    let mut r = Run::new(&loops(&format!("{CHANGE}\n{TRIED}")));
    let t = r.open("tried");
    let edited = TRIED.replace("      return a.text\n", "      b <- step ann two(a.text);\n      return b.text\n").replace("      NoWorker(e) => return \"nobody\"\n", "      NoWorker(e) => return \"nobody\"\n      Silent(e) => return \"silent\"\n");
    assert_eq!(r.reload(&loops(&format!("{CHANGE}\n{edited}")))[&t], Ok("took the new text".into()));
    assert!(r.m.tickets[&t].k.iter().any(|f| matches!(f, machine::Frame::Try { arms, .. } if arms.iter().any(|h| h.exc == "Silent"))), "{:?}", r.m.tickets[&t].k);
    r.said(t, "a1");
    assert_eq!(r.owed(t), (2, "two".into(), json!({"text": "a1"})));
    r.said(t, "b1");
    assert_eq!(r.m.tickets[&t].status, Status::Done(json!("b1")));
    r.restart();
}

const CHILD: &str = "\
kind child
  form words: Text
  result Text
  pattern
    a <- step ann one(words);
    return a.text
";

/// `child` with a field more: its form changed.
const CHILD2: &str = "\
kind child
  form words: Text, more: Text
  result Text
  pattern
    a <- step ann one(more);
    return a.text
";

const PARENT: &str = "\
kind parent
  form words: Text
  result Text
  pattern
    c <- open child(words);
    d <- step ann two(c);
    return d.text
";

const LATE: &str = "\
kind late
  form words: Text
  result Text
  pattern
    a <- step ann one(words);
    c <- open child(a.text);
    return c
";

impl Run {
    /// The newest ticket of KIND.
    fn last(&self, kind: &str) -> u64 {
        *self.m.tickets.range(..HIDDEN).filter(|(_, x)| x.kind == kind).next_back().unwrap().0
    }
    /// The definitions read again as KINDS, no open ticket taking a new text: the reload's line only.
    fn reread(&mut self, kinds: &str) {
        *self.m.defs.edit() = defs(kinds);
        self.go(Input::Reload { by: "owner".into(), why: "edited".into(), commit: None, kinds: vec!["child".into()], calls: vec![] }).reply.unwrap();
    }
    /// A restart on KINDS: what the replay says, and the machine it makes.
    fn restart_on(&self, kinds: &str) -> (Vec<String>, Machine) {
        let lines: Vec<Line> = self.log.iter().map(|l| serde_json::from_str(&serde_json::to_string(l).unwrap()).unwrap()).collect();
        let mut m2 = Machine::load(defs(kinds));
        (m2.replay(&lines), m2)
    }
}

/// A kind's form may change while none of its tickets is open (N-101 p460): a closed ticket that opened one, and an
/// open one past where it did, replay with the kind as it was where they open it, unmarked, as they ran.
#[test]
fn the_tickets_that_opened_a_kind_whose_form_changed_replay_unmarked() {
    let kinds = format!("{PARENT}\n{CHILD}");
    let mut r = Run::new(&kinds);
    let closed = r.open("parent");
    let c1 = r.last("child");
    r.said(c1, "c1");
    r.said(closed, "d1");
    assert_eq!(r.m.tickets[&closed].status, Status::Done(json!("d1")));
    let open = r.open("parent");
    let c2 = r.last("child");
    r.said(c2, "c2");
    assert_eq!(r.owed(open).1, "two");
    // the files are read together: parent's text opens a child as it is now
    let changed = format!("{}\n{CHILD2}", PARENT.replace("open child(words)", "open child(words, words)"));
    r.reread(&changed);
    // past its open: it would open no child again
    assert!(r.m.opens_after_reload(open, &[]).is_empty());
    let (problems, m2) = r.restart_on(&changed);
    assert_eq!(problems, Vec::<String>::new());
    for t in [closed, open, c1, c2] {
        assert_eq!(m2.tickets[&t].broken, None, "ticket {t}");
        assert_eq!(m2.view(t), r.m.view(t), "ticket {t}");
    }
    assert_eq!(m2.tickets[&open].loose, ["child".to_string()].into());
    assert!(!m2.tickets[&open].unchecked);
    // the view shows each child's form as its own text has it, not as the kind is now
    assert_eq!(m2.own_kind(&m2.tickets[&c1]).unwrap().form.len(), 1);
}

/// An open ticket whose old text would still open a kind whose form changed: the machine says so (coordd refuses the
/// reload and names it); when its own kind is edited in the same reload and it takes the new text, nothing it runs
/// opens the kind as it was. On a restart with no reload, the files changed under it, it no longer checks.
#[test]
fn an_open_ticket_that_would_open_a_changed_kind_as_it_was_is_found() {
    let kinds = format!("{LATE}\n{CHILD}");
    let mut r = Run::new(&kinds);
    let t = r.open("late");
    assert_eq!(r.owed(t).1, "one");
    // the files are read together: late's text opens a child as it is now
    let late2 = LATE.replace("open child(a.text)", "open child(a.text, words)");
    *r.m.defs.edit() = defs(&format!("{late2}\n{CHILD2}"));
    assert!(r.m.opens_after_reload(t, &["late".into()]).is_empty(), "it takes late's new text");
    assert_eq!(r.m.opens_after_reload(t, &[]), ["child".to_string()].into(), "were it to keep its own");
    // a new text the ticket cannot take (it reads a name bound before where it stands): its old text would still open it
    let unbound = late2.replace("    a <- step ann one(words);\n", "    z <- step ann zero();\n    a <- step ann one(z.text);\n").replace("open child(a.text, words)", "open child(a.text, z.text)");
    *r.m.defs.edit() = defs(&format!("{unbound}\n{CHILD2}"));
    assert_eq!(r.m.opens_after_reload(t, &["late".into()]), ["child".to_string()].into());
    // a restart on the files changed with no reload: no reload says child changed after its text was checked, so it
    // no longer checks, as before
    *r.m.defs.edit() = defs(&kinds);
    let (_, m2) = r.restart_on(&format!("{late2}\n{CHILD2}"));
    assert!(m2.tickets[&t].broken.as_deref().is_some_and(|b| b.contains("no longer checks")), "{:?}", m2.tickets[&t].broken);
    assert!(m2.tickets[&t].unchecked);
    assert!(m2.tickets[&t].loose.is_empty());
}

/// R's record as a start reads it (each line written and read back).
fn record(r: &Run) -> Vec<Line> {
    r.log.iter().map(|l| serde_json::from_str(&serde_json::to_string(l).unwrap()).unwrap()).collect()
}

/// Every input of LINES (and its end) a cut, the record replayed on KINDS: a replay to the cut gone on with the rest
/// (`replay_to`, `replay_on`, `replay_end`; coordd's saved replay, I-1509) is the whole record's replay, ticket for
/// ticket and problem for problem, or it is refused. The cuts refused.
fn cut_everywhere(kinds: &str, lines: &[Line]) -> Vec<usize> {
    let mut whole = Machine::load(defs(kinds));
    let problems = whole.replay(lines);
    let mut refused = Vec::new();
    for k in (0..=lines.len()).filter(|k| *k == lines.len() || lines[*k].is_input()) {
        let mut m = Machine::load(defs(kinds));
        let mut p = m.replay_to(&lines[..k], &mut |_, _, _| {});
        match m.replay_on(&lines[k..], k, &mut |_, _, _| {}) {
            Ok(more) => {
                p.extend(more);
                m.replay_end();
                assert_eq!(p, problems, "cut at line {k}");
                assert_eq!(m.tickets, whole.tickets, "cut at line {k}");
            }
            Err(_) => refused.push(k),
        }
    }
    refused
}

/// A replay cut before a reload of a kind that lines before the cut took as no reload changed after them (a parent
/// whose recorded text opens child as it was) cannot go on from the cut: the reload makes those lines check against
/// child as it was, which they did not. Cut anywhere else, it goes on as the whole record's replay.
#[test]
fn a_replay_cut_before_a_reload_its_lines_looked_past_is_refused_and_any_other_goes_on_as_the_whole() {
    let kinds = format!("{PARENT}\n{CHILD}");
    let mut r = Run::new(&kinds);
    let closed = r.open("parent");
    let c1 = r.last("child");
    r.said(c1, "c1");
    r.said(closed, "d1");
    let changed = format!("{}\n{CHILD2}", PARENT.replace("open child(words)", "open child(words, words)"));
    r.reread(&changed);
    let after = r.open("parent");
    r.said(r.last("child"), "c2");
    r.said(after, "d2");
    let lines = record(&r);
    let reload = lines.iter().position(|l| matches!(l, Line::Reload { .. })).unwrap();
    let refused = cut_everywhere(&changed, &lines);
    assert!(!refused.is_empty() && refused.iter().all(|k| *k > 0 && *k <= reload), "refused {refused:?}, the reload at {reload}");
}

/// A replay cut after a ticket's recorded program was found no longer to check (the files changed with no reload) and
/// before the log closes it cannot go on from the cut: whether it is `unchecked` reads how the log leaves it.
#[test]
fn a_replay_cut_before_the_close_of_a_ticket_it_found_no_longer_checks_is_refused() {
    let kinds = format!("{LATE}\n{CHILD}");
    let mut r = Run::new(&kinds);
    let t = r.open("late");
    r.said(t, "x");
    r.said(r.last("child"), "y");
    assert!(!r.m.tickets[&t].is_open(), "{:?}", r.m.tickets[&t].status);
    let lines = record(&r);
    let close = lines.iter().position(|l| matches!(l, Line::Closed { t: x, .. } if *x == t)).unwrap();
    let changed = format!("{}\n{CHILD2}", LATE.replace("open child(a.text)", "open child(a.text, words)"));
    let refused = cut_everywhere(&changed, &lines);
    assert!(!refused.is_empty() && refused.iter().all(|k| *k > 0 && *k <= close), "refused {refused:?}, the close at {close}");
}

/// A cut inside an input's group (an output first in what follows it) cannot go on: the group's outputs are the
/// input's, run before the cut.
#[test]
fn a_replay_cut_inside_an_inputs_group_is_refused() {
    let kinds = format!("{PARENT}\n{CHILD}");
    let mut r = Run::new(&kinds);
    r.open("parent");
    let lines = record(&r);
    let inside = lines.iter().position(|l| !l.is_input()).unwrap();
    let mut m = Machine::load(defs(&kinds));
    m.replay_to(&lines[..inside], &mut |_, _, _| {});
    let why = m.replay_on(&lines[inside..], inside, &mut |_, _, _| {}).unwrap_err();
    assert!(why.contains("is an output of an input before the cut"), "{why}");
}

/// A reload's answer for each ticket that took the new text stays on it, with the steps it withdrew (TicketView.reloaded,
/// docket-edge's N-235), and so after a restart.
#[test]
fn what_a_reload_did_stays_on_the_ticket() {
    let mut r = Run::new(&loops(CHANGE));
    let t = r.open("change");
    r.said(t, "a1");
    let edited = CHANGE.replace("    b <- step ann two(a.text);\n    return b.text\n", "    return a.text\n");
    let said = r.reload(&loops(&edited));
    assert!(said[&t].as_ref().is_ok_and(|s| s.contains("withdrawn")), "{said:?}");
    let v = r.m.view(t).unwrap().reloaded.unwrap();
    assert_eq!((v.by.as_str(), v.why.as_str(), &v.said), ("owner", "edited", said[&t].as_ref().unwrap()));
    assert_eq!(v.withdrawn, vec![view::Withdrew { on: r.m.id(t), step: 2, call: "two".into(), worker: Some("ann".into()) }]);
    r.restart();
}

/// A step in flight replaced by another in its place (docket-core's review, N-101 p545): the ticket withdraws it and
/// runs the new one, then what the new text kept after it, which reads what the new one bound.
#[test]
fn a_step_replaced_in_its_place_runs_the_new_one() {
    let mut r = Run::new(&loops(CHANGE));
    let t = r.open("change");
    assert_eq!(r.owed(t).1, "one");
    let edited = CHANGE.replace("    a <- step ann one(words);\n", "    a <- step ann three(words);\n");
    let said = r.reload(&loops(&edited));
    assert_eq!(said[&t], Ok("took the new text; step 1 (one) of ann withdrawn: its statement is gone".into()));
    assert_eq!(r.owed(t), (2, "three".into(), json!({"text": "w0"})));
    r.said(t, "a1");
    assert_eq!(r.owed(t), (3, "two".into(), json!({"text": "a1"})));
    r.said(t, "b1");
    assert_eq!(r.m.tickets[&t].status, Status::Done(json!("b1")));
    r.restart();
}

const INLINE: &str = "\
kind change
  form words: Text
  result Text
  pattern
    a <- step ann \"One {{ words }}.\" -> Said(text: Text);
    b <- step ann \"Two {{ a.text }}.\" -> Said(text: Text);
    return b.text
";

/// docket-edge's first try (paper-research N-101 p605): inline steps, a new one put first and the step the ticket
/// stands at reading it. The new step does not take the place of the one it was put before, and the words read
/// `z`, which nothing bound for this ticket: it keeps its old text.
#[test]
fn an_inline_step_put_first_and_read_by_where_it_stands_keeps_the_old_text() {
    let mut r = Run::new(&loops(INLINE));
    let t = r.open("change");
    r.said(t, "a1");
    let edited = INLINE.replace("    a <- step ann", "    z <- step ann \"Zero {{ words }}.\" -> Said(text: Text);\n    a <- step ann").replace("Two {{ a.text }}", "Two {{ z.text }}");
    let said = r.reload(&loops(&edited));
    let e = said[&t].clone().unwrap_err();
    assert!(e.starts_with("kept its old text: `z` (line 7 of the new text) is not bound where it stands"), "{e}");
    r.said(t, "b1");
    assert_eq!(r.m.tickets[&t].status, Status::Done(json!("b1")));
    r.restart();
}

/// docket-edge's second try: a new inline step put just before where the ticket stands, the return reading it. The
/// ticket stays at its own step, past the new one: it keeps its old text. With the new step's words read by nothing
/// and the step it stands at edited, it takes the new text and stays at that step.
#[test]
fn an_inline_step_put_just_before_where_it_stands_does_not_take_its_place() {
    let mut r = Run::new(&loops(INLINE));
    let t = r.open("change");
    r.said(t, "a1");
    let edited = INLINE.replace("    b <- step ann", "    y <- step ann \"Why {{ a.text }}.\" -> Said(text: Text);\n    b <- step ann").replace("return b.text", "return y.text");
    let said = r.reload(&loops(&edited));
    let e = said[&t].clone().unwrap_err();
    assert!(e.starts_with("kept its old text: `y` (line 8 of the new text) is not bound where it stands"), "{e}");
    let edited = INLINE.replace("    b <- step ann \"Two", "    y <- step ann \"Why {{ a.text }}.\" -> Said(text: Text);\n    b <- step ann \"Second");
    let said = r.reload(&loops(&edited));
    assert_eq!(said[&t], Ok("took the new text".into()));
    assert_eq!(r.owed(t).0, 2);
    r.said(t, "b1");
    assert_eq!(r.m.tickets[&t].status, Status::Done(json!("b1")));
    r.restart();
}

impl Run {
    /// What a reload to KINDS (editing `change`) would do to each open ticket, read with those definitions in place
    /// and nothing changed (`Machine::reach`, as coordd's Reach reads it): each one's case and words, by ticket.
    fn reach(&mut self, kinds: &str, calls: &[(&str, &str)]) -> BTreeMap<u64, (ReachCase, String)> {
        let (changes, lines) = (self.m.changes(), self.log.len());
        let calls: Vec<(String, String)> = calls.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        let edited: Vec<String> = if kinds.is_empty() { Vec::new() } else { vec!["change".into()] };
        let defs = if kinds.is_empty() { Defs::clone(&self.m.defs) } else { defs(kinds) };
        let (out, _) = self.m.with_defs(defs, |m| m.reach("2026-11-16T15:00:00Z", &edited, &calls, &|_, _| false));
        assert_eq!((self.m.changes(), self.log.len()), (changes, lines), "a reach changes nothing");
        out.into_iter().map(|r| (r.t, (r.case, r.said))).collect()
    }
}

/// The reach (N-1396 p301; consult-3 p306): before a reload, what each open ticket of the edited kind would do, in the
/// words the reload then says. One whose step ahead is edited is `ahead`; one standing at the edited step, given, keeps
/// what it was given (`given`); one whose node the edit removes has its step withdrawn (`withdrawn`), the reload's own
/// words; an edit above where a ticket stands, with nothing changed ahead of it, is `past`: a step put in above, its
/// lines shifting the rest, changes nothing below it.
#[test]
fn the_reach_says_what_a_reload_does_to_each_ticket_in_its_words_and_changes_nothing() {
    let mut r = Run::new(&loops(CHANGE));
    let first = r.open("change");
    let second = r.open("change");
    r.said(second, "a1");
    let edited = CHANGE.replace("two(a.text)", "two(words)");
    let reach = r.reach(&loops(&edited), &[]);
    assert_eq!(reach[&first], (ReachCase::Ahead, "took the new text; what it has not run holds what the edit changes (line 6 of the new text)".into()));
    let (case, said) = &reach[&second];
    assert_eq!(*case, ReachCase::Given);
    assert!(said.starts_with("took the new text; step 2 (two) keeps the words it was made with (owed since 2026-11-16T14:00:05Z and given to ann)"), "{said}");
    // a step put in above both, its lines shifting theirs: nothing they have not run changed
    let shifted = CHANGE.replace("  pattern\n", "  pattern\n    z <- step ann zero();\n");
    let reach = r.reach(&loops(&shifted), &[]);
    assert!(reach.values().all(|(c, s)| *c == ReachCase::Past && s.ends_with("nothing the edit changes is ahead of it")), "{reach:?}");
    // the step the second stands at gone: withdrawn, as the reload says it
    let gone = CHANGE.replace("    b <- step ann two(a.text);\n    return b.text\n", "    return a.text\n");
    let reach = r.reach(&loops(&gone), &[]);
    assert_eq!(reach[&second].0, ReachCase::Withdrawn);
    let said = r.reload(&loops(&gone));
    assert!(reach[&second].1.starts_with(said[&second].as_ref().unwrap()), "{:?} {:?}", reach[&second], said[&second]);
    assert!(reach[&first].1.starts_with(said[&first].as_ref().unwrap()));
    r.restart();
}

/// consult-3 p306, 1: a kind edit of the words of an inline step that is owed and not yet given does not reach that
/// step: `apply_carry` remakes its head and scope, never `owed`, so its template stays as it was made. The reach says
/// `given` ("not yet given"), and the reload leaves the owed template as it was.
#[test]
fn an_inline_step_owed_and_not_given_keeps_its_words_and_the_reach_says_so() {
    let mut r = Run::new(&loops(INLINE));
    r.go(Input::Open { kind: "change".into(), under: None, opener: "owner".into(), form: json!({"words": "w0"}), text: None, names: vec!["ann".into()], told: vec![] }).reply.unwrap();
    let t = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    let before = match &r.m.tickets[&t].status {
        Status::Awaits { owed, taken: false, .. } => owed.template.clone(),
        s => panic!("{s:?}"),
    };
    assert_eq!(before.as_deref(), Some("One w0."));
    let edited = INLINE.replace("\"One {{ words }}.\"", "\"First {{ words }}.\"");
    let reach = r.reach(&loops(&edited), &[]);
    let (case, said) = &reach[&t];
    assert_eq!(*case, ReachCase::Given);
    assert!(said.contains("step 1 (") && said.contains("not yet given); the next time its statement runs, it takes the new ones"), "{said}");
    assert_eq!(r.reload(&loops(&edited))[&t], Ok("took the new text".into()));
    assert!(matches!(&r.m.tickets[&t].status, Status::Awaits { owed, .. } if owed.template == before), "{:?}", r.m.tickets[&t].status);
    r.restart();
}

/// A call edit (its words, not its signature): a ticket standing at a step of it that was given keeps what it was
/// sent; one with a step of it ahead takes the new words; the owner's once-edit for a step of it on a ticket stands
/// (`own`); a ticket whose text names the call and has no step of it left is `past`.
#[test]
fn a_call_edit_reaches_the_steps_still_to_be_sent_and_not_the_owners_own() {
    let mut r = Run::new(&loops(CHANGE));
    let ahead = r.open("change");
    let given = r.open("change");
    r.said(given, "a1");
    let own = r.open("change");
    r.go(Input::Template { t: own, call: "two".into(), scope: "step".into(), text: "Mine {{ text }}.".into(), by: "owner".into(), why: "mine".into() }).reply.unwrap();
    let past = r.open("change");
    r.said(past, "a1");
    r.said(past, "b1");
    let reach = r.reach("", &[("two", "two")]);
    assert_eq!(reach[&ahead], (ReachCase::Ahead, "its step of two ahead (two) (line 6) is rendered with the new words when it is sent".into()));
    assert_eq!(reach[&given].0, ReachCase::Given);
    assert!(reach[&given].1.contains("a reminder of it, or the step given again after its process was lost, is rendered from the files and reads the new words"), "{:?}", reach[&given]);
    assert_eq!(reach[&own].0, ReachCase::Own, "{:?}", reach[&own]);
    assert!(reach[&own].1.starts_with("its step of two ahead (line 6) goes with the owner's own prompt for it (since 2026-11-16T14:"), "{:?}", reach[&own]);
    assert!(!reach.contains_key(&past), "closed: not named");
    let reach = r.reach("", &[("one", "one")]);
    assert_eq!(reach[&given], (ReachCase::Past, "no step of one is ahead of it".into()));
}

/// consult-3 p308, 2: an edit that only takes a statement out of what a ticket has still to run reaches it. `pairs`
/// lists the nodes it matches, and a node taken out is matched with none: neither edited nor added in the new text.
/// The reach reads it in the ticket's old frames: `ahead`, naming its old line; saved, the ticket no longer runs it.
/// A ticket past it is `past`.
#[test]
fn an_edit_that_only_takes_out_a_step_ahead_of_a_ticket_reaches_it() {
    const SPARE: &str = "kind change\n  form words: Text\n  result Text\n  pattern\n    a <- step ann one(words);\n    _ <- step ann two(a.text);\n    b <- step ann two(a.text);\n    return b.text\n";
    let mut r = Run::new(&loops(SPARE));
    let before = r.open("change");
    let after = r.open("change");
    r.said(after, "a1");
    r.said(after, "n1");
    let edited = SPARE.replace("    _ <- step ann two(a.text);\n", "");
    let reach = r.reach(&loops(&edited), &[]);
    assert_eq!(reach[&before], (ReachCase::Ahead, "took the new text; what it has still to run no longer has line 6 of its old text".into()));
    assert_eq!(reach[&after], (ReachCase::Past, "took the new text; nothing the edit changes is ahead of it".into()));
    let said = r.reload(&loops(&edited));
    assert_eq!((said[&before].clone(), said[&after].clone()), (Ok("took the new text".into()), Ok("took the new text".into())));
    r.said(before, "a1");
    assert_eq!(r.owed(before).0, 2, "the step taken out is not run");
    r.said(before, "b1");
    assert_eq!(r.m.tickets[&before].status, Status::Done(json!("b1")));
    r.restart();
}

/// consult-3 p317, 4: a once-edit of a call (`step` scope) goes with the next step of it only; the rounds of a loop
/// after it take the new words. A `loop` ticket stands at round 1's `round`, given, with his edit for `round` unused:
/// the step ahead is one line, in the loop's body, and runs again, so the reach says the next goes with his prompt and
/// the ones after it with the new words, not that the step goes with his prompt.
#[test]
fn a_once_edit_of_a_call_in_a_loop_covers_its_next_round_only_and_the_reach_says_so() {
    let mut r = Run::new(&loops(CHANGE));
    let t = r.open("loop");
    r.go(Input::Template { t, call: "round".into(), scope: "step".into(), text: "Mine {{ x }}.".into(), by: "owner".into(), why: "mine".into() }).reply.unwrap();
    let reach = r.reach("", &[("round", "round")]);
    let (case, said) = &reach[&t];
    assert_eq!(*case, ReachCase::Given, "the step it stands at was given: {said}");
    assert!(said.contains("of its steps of round ahead (line 7), the next goes with the owner's own prompt for it (since 2026-11-16T14:"), "{said}");
    assert!(said.ends_with("and the ones after it with the new words"), "{said}");
}

/// consult-3 p330: the steps of a call ahead count the rounds a loop has left, not its body alone. At round 1 of
/// `up to 2`, given, one step of `round` is ahead, and his once-edit covers it. At round 1 of `up to 1` none is: the
/// body runs no more, and the reach names only the step given. A step later in this round that the next round runs
/// again is two steps, on one line.
#[test]
fn a_once_edit_in_a_loop_counts_the_rounds_it_has_left() {
    let round = "      g <- step ann round(x);\n";
    let rounds = |n: u32, body: &str| format!("{CHANGE}\n{}", LOOP.replace("up to 5", &format!("up to {n}")).replace(round, body));
    let once = |r: &mut Run, t: u64| {
        r.go(Input::Template { t, call: "round".into(), scope: "step".into(), text: "Mine {{ x }}.".into(), by: "owner".into(), why: "mine".into() }).reply.unwrap();
        r.reach("", &[("round", "round")]).remove(&t).unwrap()
    };
    let mut r = Run::new(&rounds(2, round));
    let t = r.open("loop");
    let (case, said) = once(&mut r, t);
    assert_eq!(case, ReachCase::Given, "{said}");
    assert!(said.contains("its step of round ahead (line 7) goes with the owner's own prompt for it (since 2026-11-16T14:"), "{said}");
    assert!(!said.contains("the ones after it"), "{said}");
    let mut r = Run::new(&rounds(1, round));
    let t = r.open("loop");
    let (case, said) = once(&mut r, t);
    assert_eq!(case, ReachCase::Given, "{said}");
    assert!(said.contains("step 1 (round)") && !said.contains("round ahead"), "its last round: no step of round ahead: {said}");
    let mut r = Run::new(&rounds(2, "      a <- step ann one(x);\n      g <- step ann round(a.text);\n"));
    let t = r.open("loop");
    assert_eq!(r.owed(t).1, "one");
    let (case, said) = once(&mut r, t);
    assert_eq!(case, ReachCase::Own, "{said}");
    assert!(said.starts_with("of its steps of round ahead (line 8), the next goes with the owner's own prompt for it (since 2026-11-16T14:"), "{said}");
    assert!(said.ends_with("and the ones after it with the new words"), "{said}");
}
