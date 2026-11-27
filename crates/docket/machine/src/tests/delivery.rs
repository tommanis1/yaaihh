//! The message runtime (0.2 A1–A3, delivery.rs; docket-core2's plan N-1397 post 1, consult-3's five points post 2 and
//! the answers post 3; the hold's two cases, N-1324 post 29): agent-base's `delivery.kind` and `next.call` run on
//! the delivery events, one at a time per agent, and their acts are facts that a replay rebuilds. All the runtime
//! writes is `event` lines, so the engine before it (train 26) replays its records as recorded (`on_train_26`).

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const DELIVERY_KIND: &str = include_str!("../../../../../agent-base/kinds/delivery.kind");
/// delivery.kind before I-1480, byte for byte (92b11b2d): `free` asks the watcher's `next`. The program a delivery
/// ticket open at install still holds until its agent's next run opens one on the text loaded then
/// (`delivery_ticket`), and the one every record before it replays with; the cases of `next` run on it.
const NEXT_KIND: &str = include_str!("delivery_next.kind");
/// delivery.kind before his posts waited for the turn's end too, byte for byte (train-70's tip 222cfca6; the owner,
/// N-1104 p716, his yes p923): `arrived` puts his post into a working agent's turn at its next step (N-342). What a
/// delivery ticket opened before holds, as for NEXT_KIND; the cases of his post put in run on it. coordd's legs read
/// this copy too (tests/delivery.rs `his_into`).
const INTO_KIND: &str = include_str!("delivery_into.kind");
const NEXT_CALL: &str = include_str!("../../../../../agent-base/calls/next.call");
/// fold.call: `fold`, the call delivery.kind's `free` steps, and the `choose` coordd runs for it (proto-mind's head,
/// N-1446 p2105).
const FOLD_CALL: &str = include_str!("../../../../../agent-base/calls/fold.call");

/// A place for posts: it rests, and the owner's post goes to whom he typed it to (no route call).
const TALK: &str = "\
/// a place for posts, for the delivery tests
kind talk
  letter N
  form to: Agent
  pattern
    ...
  when owner_post(p) do
    tell p.naming p
";

/// A step to an agent, for the one-step case (I-1480): the act keeps a message to one.
const ASKS: &str = "\
/// a step to an agent, for the delivery tests
kind asks
  letter Q
  form to: Agent
  result Fine
  pattern
    step to \"Say which branch.\" -> Fine
";

/// A program's post that only informs an agent and asks into its turn (`post … informing`, as the brain's), for the
/// digest's whole posts (I-1480).
const NOTE_TO: &str = "\
/// a note into an agent's turn, for the delivery tests
kind note_to
  form w: Agent
  result ()
  pattern
    _ <- post \"He is on the brain: answer his post first.\" informing w;
    return ()
";

/// An item, for whoever works on it (`coord item start`, `Input::Started`: I-1480's hold and `own`).
const ITEM: &str = "\
/// an item, for the delivery tests
kind item
  letter I
  form to: Agent
  pattern
    ...
";

fn defs_with(delivery: &str) -> Defs {
    let mut named = BTreeMap::new();
    let files = [
        ("shape.call", include_str!("../../../../../agent-base/calls/shape.call")),
        ("route.call", include_str!("../../../../../agent-base/calls/route.call")),
        ("clarify.call", include_str!("../../../../../agent-base/calls/clarify.call")),
        ("gather.call", include_str!("../../../../../agent-base/calls/gather.call")),
        ("question.call", include_str!("../../../../../agent-base/calls/question.call")),
        ("claims.call", include_str!("../../../../../agent-base/calls/claims.call")),
        ("next.call", NEXT_CALL),
        ("fold.call", FOLD_CALL),
    ];
    let calls: BTreeMap<String, CallSig> = files.iter().flat_map(|(f, t)| syntax::parse_calls(f, t, &mut named).unwrap()).map(|c| (c.name.clone(), c)).collect();
    let kinds = [
        ("base.kind", include_str!("../../../../../agent-base/kinds/base.kind")),
        ("question.kind", include_str!("../../../../../agent-base/kinds/question.kind")),
        ("talk.kind", TALK),
        ("asks.kind", ASKS),
        ("note_to.kind", NOTE_TO),
        ("item.kind", ITEM),
        ("delivery.kind", delivery),
    ];
    let files: Vec<(String, String)> = kinds.iter().map(|(f, t)| (f.to_string(), t.to_string())).collect();
    let (d, errors) = Defs::build(&files, calls, named, 200);
    assert!(errors.is_empty(), "{errors:?}");
    d
}

struct Run {
    m: Machine,
    log: Vec<Line>,
    clock: u32,
}

impl Run {
    fn new() -> Run {
        Run::with(DELIVERY_KIND)
    }
    /// A run of the delivery program KIND.
    fn with(kind: &str) -> Run {
        Run { m: Machine::load(defs_with(kind)), log: Vec::new(), clock: 0 }
    }
    fn go(&mut self, i: Input) -> Out {
        self.clock += 1;
        let out = self.m.apply(i, format!("2026-11-20T09:{:02}:{:02}Z", self.clock / 60, self.clock % 60));
        self.log.extend(out.lines.iter().cloned());
        out
    }
    fn ok(&mut self, i: Input) -> Out {
        let out = self.go(i);
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        out
    }
    /// A thread for posts to TO: N-200, N-201, ...
    fn talk(&mut self, to: &str) -> u64 {
        let out = self.ok(Input::Open { kind: "talk".into(), under: None, opener: "owner".into(), form: json!({"to": to}), text: None, names: vec![to.into()], told: vec![] });
        let t = *self.m.tickets.range(..HIDDEN).next_back().unwrap().0;
        assert!(out.reply.unwrap().contains(&self.m.id(t)));
        t
    }
    /// BY's post on T to TO: its number.
    fn post(&mut self, t: u64, by: &str, to: &str, text: &str) -> u32 {
        self.ok(Input::Post { t, by: by.into(), text: Text::New(text.into()), names: vec![to.into()], told: vec![], inform: vec![], answers: None, kind: None });
        self.m.tickets[&t].last()
    }
    fn event(&mut self, name: &str, value: Value) -> Out {
        self.ok(Input::Event { name: name.into(), value })
    }
    fn free(&mut self, agent: &str) -> Out {
        self.event("free", json!({"agent": agent, "doing": "N-1 · the median"}))
    }
    /// The owner's hold on agents' mail switched ON or off (MAIL_HOLD, line 327): the machine's reply.
    fn hold(&mut self, on: bool) -> String {
        self.event(MAIL_HOLD, json!({"by": "owner", "on": on})).reply.unwrap()
    }
    /// The ids run C's `next` was offered, each with why it is kept (none: not kept).
    fn offered(&self, c: u64) -> Vec<(String, Option<String>)> {
        self.m.tickets[&c].form["waiting"].as_array().unwrap().iter().map(|i| (i["id"].as_str().unwrap().to_string(), i["kept"].as_str().map(str::to_string))).collect()
    }
    /// The open run of AGENT's delivery that waits on the watcher's `next`.
    fn next_run(&self, agent: &str) -> u64 {
        let runs: Vec<u64> = self.m.tickets.range(DESK..).filter(|(_, x)| x.is_open() && matches!(&x.status, Status::Awaits { agent: w, owed, .. } if w == WATCHER && owed.call == "next") && x.form["agent"] == json!(agent)).map(|(t, _)| *t).collect();
        assert_eq!(runs.len(), 1, "{runs:?}");
        runs[0]
    }
    /// The watcher's answer to run C's `next`: written as a `delivery_decided` event.
    fn answer_next(&mut self, c: u64, head: Value) -> Out {
        let out = self.ok(Input::Answer { t: c, by: WATCHER.into(), head, text: Text::New(String::new()) });
        assert!(matches!(out.lines.first(), Some(Line::Event { name, value, .. }) if name == syntax::DECIDED && value["run"] == json!(c)), "{out:?}");
        out
    }
    /// The acts done, from their record lines: agent, act, items, rest.
    fn acted(&self) -> Vec<(String, String, Vec<String>, Vec<String>)> {
        let ids = |v: &Value| -> Vec<String> { v.as_array().map(|xs| xs.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default() };
        self.log.iter().filter_map(|l| match l {
            Line::Event { name, value, .. } if name == syntax::ACTED => Some((value["agent"].as_str()?.to_string(), value["act"].as_str()?.to_string(), ids(&value["items"]), ids(&value["rest"]))),
            _ => None,
        }).collect()
    }
    /// The delivery tickets opened, from their record lines: number and program.
    fn opened(&self) -> Vec<(u64, String)> {
        self.log.iter().filter_map(|l| match l {
            Line::Event { name, value, .. } if name == syntax::DELIVERY_OPENED => Some((value["t"].as_u64()?, value["program"].as_str()?.to_string())),
            _ => None,
        }).collect()
    }
    fn waiting_ids(&self, agent: &str) -> Vec<String> {
        self.m.waiting(agent).iter().map(|i| i["id"].as_str().unwrap().to_string()).collect()
    }
    /// coordd's `given` for a message the program handed: one line per ticket, up to its last post handed.
    fn given(&mut self, agent: &str, t: u64, upto: u32) {
        let id = self.m.id(t);
        self.ok(Input::Given { t, to: agent.into(), delivery: format!("{id}.p{upto}"), upto, skipped: Default::default(), place: Some(Loc { file: "w.jsonl".into(), offset: 10 * u64::from(upto), len: 5 }) });
    }
    /// A new machine that replays the record: it agrees with it. Every line the runtime wrote is an `event` line
    /// that names no ticket of its own, and the engine before it replays the record with no problem.
    fn replayed(&self) -> Machine {
        let mut m = Machine::load(defs_with(DELIVERY_KIND));
        assert_eq!(m.replay(&self.log), Vec::<String>::new());
        self.only_events();
        assert_eq!(on_train_26(&self.log), Vec::<String>::new());
        m
    }
    /// No line of the record is a delivery ticket's or its runs' but the runtime's `event` lines, which open nothing
    /// and are on no ticket, and coordd's `start_failed` for a copy it could not start, which no run makes.
    fn only_events(&self) {
        for l in &self.log {
            if let Line::Event { name, fired, counted, on, .. } = l {
                if syntax::delivery_event(name) || name == syntax::DECIDED || syntax::delivery_output(name) {
                    assert!(fired.is_empty() && counted.is_empty() && on.is_none(), "{l:?}");
                }
                continue;
            }
            if matches!(l, Line::StartFailed { .. }) {
                continue;
            }
            let t = serde_json::to_value(l).unwrap().get("t").and_then(Value::as_u64);
            assert!(t.is_none_or(|t| t < DESK), "{l:?}");
        }
    }
}

/// What the engine before the runtime (train 26, cb0acab7) finds replaying LOG: it reads an `event` line that opens
/// nothing and is on no ticket as recorded, whatever its name, and takes every `event` line for an input. Here each
/// of the runtime's lines is renamed so that this engine reads it so; train 26's own replay is the old-engine test.
fn on_train_26(log: &[Line]) -> Vec<String> {
    let old: Vec<Line> = log
        .iter()
        .map(|l| match l {
            Line::Event { at, name, value, fired, counted, on: None } if syntax::delivery_event(name) || name == syntax::DECIDED || syntax::delivery_output(name) => {
                Line::Event { at: at.clone(), name: format!("old_{name}"), value: value.clone(), fired: fired.clone(), counted: counted.clone(), on: None }
            }
            l => l.clone(),
        })
        .collect();
    let mut d = defs_with(DELIVERY_KIND);
    d.kinds.remove("delivery");
    Machine::load(d).replay(&old)
}

#[test]
fn the_delivery_program_of_agent_base_checks() {
    let d = defs_with(DELIVERY_KIND);
    let k = &d.kinds["delivery"];
    for ev in ["arrived", "free", "stopped"] {
        assert!(k.handlers.iter().any(|h| h.ev == ev), "{ev}");
    }
    // its calls run for the task `deliver`, the one the policy names a model for: `fold` and the `choose` coordd runs
    // for it (fold.call), and `next`, which the programs recorded on the text before name (next.call)
    let mut named = BTreeMap::new();
    syntax::parse_types("system", syntax::SYSTEM_TYPES, &mut named).unwrap();
    let files = [("next.call".to_string(), NEXT_CALL.to_string()), ("fold.call".to_string(), FOLD_CALL.to_string())];
    let (calls, errors) = syntax::parse_call_files(&files, &mut named);
    assert!(errors.is_empty(), "{errors:?}");
    for c in ["next", "fold", "choose"] {
        assert_eq!(calls[c].attr("task"), Some("deliver"), "{c}");
    }
    // the handlers' bodies print and read back as they are (a run runs its handler as printed, live and on replay)
    for h in k.handlers.iter().filter(|h| syntax::delivery_event(&h.ev)) {
        let text = print::prog(&h.body, 0);
        assert_eq!(syntax::parse_prog(&text, 1).map(|p| print::prog(&p, 0)), Ok(text));
    }
}

#[test]
fn the_owners_post_goes_at_once_and_alone_the_rest_named() {
    let mut r = Run::new();
    let a = r.talk("w1");
    let b = r.talk("w1");
    let x = r.post(a, "x", "w1", "@w1 the build is green");
    let o = r.post(b, "owner", "w1", "@w1 stop and look at the median");
    let out = r.free("w1");
    let (ia, ib) = (format!("{}.p{x}", r.m.id(a)), format!("{}.p{o}", r.m.id(b)));
    assert!(out.effects.contains(&Effect::Hand { to: "w1".into(), items: vec![ib.clone()], rest: vec![ia.clone()], digest: vec![] }), "{out:?}");
    assert_eq!(r.acted(), vec![("w1".into(), "deliver".into(), vec![ib.clone()], vec![ia.clone()])]);
    // no call was made, and nothing is open
    assert!(!r.m.deciding("w1"));
    // in flight until coordd's given: not offered again meanwhile
    assert_eq!(r.waiting_ids("w1"), vec![ia.clone()]);
    r.given("w1", b, o);
    assert_eq!(r.waiting_ids("w1"), vec![ia]);
    assert!(r.m.resume_effects().iter().all(|e| !matches!(e, Effect::Hand { .. })));
    r.replayed();
}

#[test]
fn free_asks_next_and_hands_its_choice_as_one_message() {
    let mut r = Run::with(NEXT_KIND);
    let a = r.talk("w1");
    let b = r.talk("w1");
    let p1 = r.post(a, "x", "w1", "@w1 noted");
    let p2 = r.post(b, "y", "w1", "@w1 which branch?");
    r.free("w1");
    let c = r.next_run("w1");
    assert!(r.m.deciding("w1"));
    let (i1, i2) = (format!("{}.p{p1}", r.m.id(a)), format!("{}.p{p2}", r.m.id(b)));
    // the step carries what waits, as the call's arguments
    let args = &r.m.tickets[&c].form["waiting"];
    assert_eq!(args.as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap()).collect::<Vec<_>>(), vec![i1.as_str(), i2.as_str()]);
    // an answer out of shape is refused and recorded nowhere: the run still waits (coordd answers Give(everything))
    let bad = r.go(Input::Answer { t: c, by: WATCHER.into(), head: json!({"head": "Maybe"}), text: Text::New(String::new()) });
    assert!(bad.reply.is_err() && bad.lines.is_empty(), "{bad:?}");
    assert!(r.m.deciding("w1"));
    let out = r.answer_next(c, json!({"head": "Give", "now": [i2], "rest": [i1, "N-999.p1"]}));
    // an id that waits for nothing is left out
    assert!(out.effects.contains(&Effect::Hand { to: "w1".into(), items: vec![i2.clone()], rest: vec![i1.clone()], digest: vec![] }), "{out:?}");
    assert!(!r.m.deciding("w1"));
    r.given("w1", b, p2);
    assert_eq!(r.waiting_ids("w1"), vec![i1]);
    r.replayed();
}

#[test]
fn rest_keeps_what_waits_and_a_kept_post_waits_below_the_read_mark() {
    let mut r = Run::with(NEXT_KIND);
    let a = r.talk("w1");
    let p1 = r.post(a, "x", "w1", "@w1 one");
    let p2 = r.post(a, "x", "w1", "@w1 two");
    r.free("w1");
    let c = r.next_run("w1");
    r.answer_next(c, json!({"head": "Rest", "why": "it ends its turn to read the log first"}));
    let (i1, i2) = (format!("{}.p{p1}", r.m.id(a)), format!("{}.p{p2}", r.m.id(a)));
    assert_eq!(r.m.kept("w1"), BTreeMap::from([(i1.clone(), "it ends its turn to read the log first".to_string()), (i2.clone(), "it ends its turn to read the log first".to_string())]));
    // given post 2 by hand (typed into its terminal, say): post 1, kept, still waits below the read mark
    r.given("w1", a, p2);
    assert_eq!(r.waiting_ids("w1"), vec![i1.clone()]);
    assert_eq!(r.m.waiting("w1")[0]["kept"], json!("it ends its turn to read the log first"));
    // the next free offers it, and once handed it is kept no more
    r.free("w1");
    let c = r.next_run("w1");
    r.answer_next(c, json!({"head": "Give", "now": [i1], "rest": []}));
    assert!(r.m.kept("w1").is_empty());
    r.replayed();
}

/// consult-3, N-1397 post 2, 1: the queue is whole. Three events during one run give three runs, in order; a replay
/// gives the same three.
#[test]
fn three_events_during_one_run_give_three_runs_in_order() {
    let mut r = Run::with(NEXT_KIND);
    let a = r.talk("w1");
    let p1 = r.post(a, "x", "w1", "@w1 one");
    r.free("w1");
    let c = r.next_run("w1");
    let mut items = Vec::new();
    for (by, text) in [("y", "@w1 two"), ("z", "@w1 three"), ("v", "@w1 four")] {
        let n = r.post(a, by, "w1", text);
        let id = format!("{}.p{n}", r.m.id(a));
        let out = r.event("arrived", json!({"agent": "w1", "item": id}));
        assert!(out.reply.as_ref().unwrap().starts_with("Waits behind"), "{out:?}");
        items.push(id);
    }
    assert_eq!(r.m.delivery_queue("w1"), vec!["arrived", "arrived", "arrived"]);
    let i1 = format!("{}.p{p1}", r.m.id(a));
    r.answer_next(c, json!({"head": "Give", "now": [i1], "rest": []}));
    assert!(r.m.delivery_queue("w1").is_empty());
    let runs: Vec<u64> = r.m.tickets.range(DESK..).filter(|(_, x)| x.kind == "arrived").map(|(t, _)| *t).collect();
    assert_eq!(runs.len(), 3, "{runs:?}");
    // each kept in the order its event came, none merged
    let kept: Vec<Vec<String>> = r.acted().into_iter().filter(|a| a.1 == "keep").map(|a| a.2).collect();
    assert_eq!(kept, items.iter().map(|i| vec![i.clone()]).collect::<Vec<_>>());
    let m = r.replayed();
    assert_eq!(m.kept("w1"), r.m.kept("w1"));
}

/// consult-3, N-1397 post 2, 2: the owner's post does not wait behind an open `next`. The run is ended, his event
/// runs at once, and the late answer to the withdrawn step is refused and recorded nowhere.
#[test]
fn the_owners_post_ends_an_open_next_and_goes_at_once() {
    let mut r = Run::with(NEXT_KIND);
    let a = r.talk("w1");
    let b = r.talk("w1");
    let p1 = r.post(a, "x", "w1", "@w1 noted");
    r.free("w1");
    let c = r.next_run("w1");
    let o = r.post(b, "owner", "w1", "@w1 stop: the release is tonight");
    let out = r.free("w1");
    assert!(out.reply.as_ref().unwrap().contains("the owner's post goes first"), "{out:?}");
    let (i1, io) = (format!("{}.p{p1}", r.m.id(a)), format!("{}.p{o}", r.m.id(b)));
    assert!(out.effects.contains(&Effect::Hand { to: "w1".into(), items: vec![io.clone()], rest: vec![i1.clone()], digest: vec![] }), "{out:?}");
    assert!(matches!(&r.m.tickets[&c].status, Status::Failed(e) if e.exc == "Cancelled"));
    let lines = r.log.len();
    let late = r.go(Input::Answer { t: c, by: WATCHER.into(), head: json!({"head": "Give", "now": [i1], "rest": []}), text: Text::New(String::new()) });
    assert!(late.reply.is_err() && late.lines.is_empty(), "{late:?}");
    assert_eq!(r.log.len(), lines);
    // the agents' post waits for the next free
    r.given("w1", b, o);
    assert_eq!(r.waiting_ids("w1"), vec![i1]);
    r.replayed();
}

/// consult-3, N-1397 post 2, 3: a start with an open `next`: the run is open after the replay, its step still owed to
/// the watcher (asked again), and its answer is taken. N-1324 post 29, 2: the kept fact comes back from the record.
#[test]
fn a_start_keeps_the_open_next_and_what_was_kept() {
    let mut r = Run::with(NEXT_KIND);
    let a = r.talk("w1");
    let p1 = r.post(a, "x", "w1", "@w1 one");
    r.free("w1");
    let c = r.next_run("w1");
    r.answer_next(c, json!({"head": "Rest", "why": "held by the owner since 09:00"}));
    let p2 = r.post(a, "y", "w1", "@w1 two");
    r.free("w1");
    let c2 = r.next_run("w1");
    // the stop: a new machine replays the record
    let mut m = r.replayed();
    let i1 = format!("{}.p{p1}", r.m.id(a));
    assert_eq!(m.kept("w1"), BTreeMap::from([(i1.clone(), "held by the owner since 09:00".to_string())]));
    assert!(m.deciding("w1"));
    assert!(matches!(&m.tickets[&c2].status, Status::Awaits { agent, owed, .. } if agent == WATCHER && owed.call == "next"));
    // what the run offers is what waited when it began: the kept post too, once
    let offered: Vec<String> = m.tickets[&c2].form["waiting"].as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap().to_string()).collect();
    let i2 = format!("{}.p{p2}", r.m.id(a));
    assert_eq!(offered, vec![i1.clone(), i2.clone()]);
    let out = m.apply(Input::Answer { t: c2, by: WATCHER.into(), head: json!({"head": "Give", "now": [i1, i2], "rest": []}), text: Text::New(String::new()) }, "2026-11-20T10:00:00Z".into());
    assert!(out.effects.contains(&Effect::Hand { to: "w1".into(), items: vec![i1, i2], rest: vec![], digest: vec![] }), "{out:?}");
    assert!(m.kept("w1").is_empty());
}

/// consult-3, N-1397 post 2, 4: the text changed with an event queued: the open run ends under its own text, the
/// queued event runs on the agent's next delivery ticket, opened on the new text, and the old one closes.
#[test]
fn a_text_change_with_an_event_queued_moves_it_to_the_next_ticket() {
    let mut r = Run::with(NEXT_KIND);
    let a = r.talk("w1");
    let p1 = r.post(a, "x", "w1", "@w1 one");
    r.free("w1");
    let c = r.next_run("w1");
    let d = r.m.tickets[&c].under.unwrap();
    let p2 = r.post(a, "y", "w1", "@w1 two");
    let i2 = format!("{}.p{p2}", r.m.id(a));
    r.event("arrived", json!({"agent": "w1", "item": i2}));
    // the program now keeps an agent's post with other words
    let changed = NEXT_KIND.replace("\"comes when its turn ends\"", "\"waits for the turn's end\"");
    let new = defs_with(&changed);
    *r.m.defs.edit() = new;
    let i1 = format!("{}.p{p1}", r.m.id(a));
    r.answer_next(c, json!({"head": "Give", "now": [i1], "rest": []}));
    let opened = r.opened();
    assert_eq!(opened.len(), 2, "{opened:?}");
    assert_eq!(opened[0].0, d);
    assert!(opened[1].1.contains("waits for the turn's end"));
    assert!(!r.m.tickets[&d].is_open());
    let run = r.m.tickets.range(DESK..).filter(|(_, x)| x.kind == "arrived").next_back().map(|(_, x)| x.under).unwrap();
    assert_eq!(run, Some(opened[1].0));
    assert_eq!(r.m.kept("w1"), BTreeMap::from([(i2, "waits for the turn's end".to_string())]));
    // a replay under today's text runs each ticket on the text it recorded
    let mut m = Machine::load(defs_with(&changed));
    assert_eq!(m.replay(&r.log), Vec::<String>::new());
    r.only_events();
    assert_eq!(on_train_26(&r.log), Vec::<String>::new());
}

/// N-1324 post 29, 1: the reply to #7 does not answer a kept #6. Once #6 is handed, the next reply is its answer.
#[test]
fn a_reply_does_not_answer_what_was_kept_from_it() {
    let mut r = Run::with(NEXT_KIND);
    let a = r.talk("w1");
    let six = r.post(a, "x", "w1", "@w1 six");
    // #6 is kept (Rest), #7 comes and is handed
    r.free("w1");
    let c = r.next_run("w1");
    r.answer_next(c, json!({"head": "Rest", "why": "held by the owner since 09:00"}));
    let seven = r.post(a, "owner", "w1", "@w1 seven");
    let out = r.free("w1");
    let (i6, i7) = (format!("{}.p{six}", r.m.id(a)), format!("{}.p{seven}", r.m.id(a)));
    assert!(out.effects.contains(&Effect::Hand { to: "w1".into(), items: vec![i7.clone()], rest: vec![i6.clone()], digest: vec![] }), "{out:?}");
    r.given("w1", a, seven);
    let d7 = format!("{}.p{seven}", r.m.id(a));
    r.ok(Input::TurnEnded { agent: "w1".into(), delivery: Some(d7), text: Loc { file: "w.jsonl".into(), offset: 900, len: 40 }, later: vec![], stood: None, go: None });
    let x = &r.m.tickets[&a];
    let reply = x.posts.last().unwrap().clone();
    assert_eq!(reply.by, "w1");
    assert!(x.answers(&reply, seven) && !x.answers(&reply, six));
    // its author is not named as answered, and #6 still waits
    assert!(!reply.names.contains(&"x".to_string()), "{:?}", reply.names);
    assert_eq!(r.waiting_ids("w1"), vec![i6.clone()]);
    // released: the next free hands #6, and the reply after it is #6's answer
    r.free("w1");
    let c = r.next_run("w1");
    r.answer_next(c, json!({"head": "Give", "now": [i6.clone()], "rest": []}));
    r.given("w1", a, six);
    r.ok(Input::TurnEnded { agent: "w1".into(), delivery: Some(i6), text: Loc { file: "w.jsonl".into(), offset: 1000, len: 40 }, later: vec![], stood: None, go: None });
    let x = &r.m.tickets[&a];
    let reply = x.posts.last().unwrap().clone();
    assert_eq!(reply.by, "w1");
    assert!(x.answers(&reply, six));
    assert_eq!(reply.names, vec!["x".to_string()]);
    // written as its own post, under #6, before its turn's end: train 26, which knows nothing kept, counts #6 answered by
    // the reply to #7 and would write no reply at the turn's end; it replays the post as written
    assert_eq!(reply.answers, Some(six));
    assert!(matches!(r.log.iter().rev().find(|l| matches!(l, Line::Post { .. })), Some(Line::Post { by, answers: Some(n), .. }) if by == "w1" && *n == six));
    // the thread goes on, and the post numbers stay as train 26 gives them: the owner's next post there, handed
    let eight = r.post(a, "owner", "w1", "@w1 eight");
    r.free("w1");
    r.given("w1", a, eight);
    r.replayed();
}

/// consult-3, N-1397 post 9: no double hand-over at the end of a start (or a compaction). His post that comes while
/// the agent is starting is handed by `arrived`; the `free` at the start's end, before coordd's `given` (the post
/// still queued in Claude Code), and the one after it, do not hand it again.
#[test]
fn a_post_handed_while_starting_is_not_handed_again_by_free() {
    let mut r = Run::with(NEXT_KIND);
    let a = r.talk("w1");
    let b = r.talk("w1");
    let x = r.post(b, "x", "w1", "@w1 noted");
    let o = r.post(a, "owner", "w1", "@w1 the release is tonight");
    let (ix, io) = (format!("{}.p{x}", r.m.id(b)), format!("{}.p{o}", r.m.id(a)));
    let out = r.event("arrived", json!({"agent": "w1", "item": io}));
    assert!(out.effects.contains(&Effect::Hand { to: "w1".into(), items: vec![io.clone()], rest: vec![], digest: vec![] }), "{out:?}");
    // the start ends with his post still queued: free offers the rest only
    r.free("w1");
    let c = r.next_run("w1");
    let offered: Vec<String> = r.m.tickets[&c].form["waiting"].as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap().to_string()).collect();
    assert_eq!(offered, vec![ix.clone()]);
    // a decision that names his post anyway hands only what waits
    let out = r.answer_next(c, json!({"head": "Give", "now": [io, ix], "rest": []}));
    assert!(out.effects.contains(&Effect::Hand { to: "w1".into(), items: vec![ix.clone()], rest: vec![], digest: vec![] }), "{out:?}");
    r.given("w1", a, o);
    let hands = r.acted().iter().filter(|a| a.1 == "deliver" && a.2.contains(&io)).count();
    assert_eq!(hands, 1);
    // nothing waits now: the next free decides nothing
    assert_eq!(r.free("w1").reply, Ok("Nothing to decide for w1.".into()));
    r.replayed();
}

/// A thread reads in order: a post handed brings the posts of its thread before it that wait, in their order, so none
/// is named at the foot and then passed by the read mark of the `given` that covers it. A kept one stays kept, below
/// the mark, until a `deliver` names it.
#[test]
fn a_post_handed_brings_the_posts_before_it_on_its_thread_but_not_a_kept_one() {
    let mut r = Run::with(NEXT_KIND);
    let a = r.talk("w1");
    let b = r.talk("w1");
    let a1 = r.post(a, "x", "w1", "@w1 one");
    let b1 = r.post(b, "z", "w1", "@w1 elsewhere");
    r.free("w1");
    let c = r.next_run("w1");
    r.answer_next(c, json!({"head": "Rest", "why": "it reads the log first"}));
    let a2 = r.post(a, "y", "w1", "@w1 two");
    let a3 = r.post(a, "owner", "w1", "@w1 three");
    let (ia1, ib1, ia2, ia3) = (format!("{}.p{a1}", r.m.id(a)), format!("{}.p{b1}", r.m.id(b)), format!("{}.p{a2}", r.m.id(a)), format!("{}.p{a3}", r.m.id(a)));
    // his post goes at once with y's before it on his thread; x's, kept, stays kept and is named at the foot
    let out = r.free("w1");
    assert!(out.effects.contains(&Effect::Hand { to: "w1".into(), items: vec![ia2.clone(), ia3.clone()], rest: vec![ia1.clone(), ib1.clone()], digest: vec![] }), "{out:?}");
    assert_eq!(r.acted().last().map(|a| a.2.clone()), Some(vec![ia2.clone(), ia3.clone()]));
    r.given("w1", a, a3);
    assert_eq!(r.waiting_ids("w1"), vec![ib1.clone(), ia1.clone()]);
    // `next` names a later post of a thread first and an earlier one after it: the thread's order is kept
    let a4 = r.post(a, "v", "w1", "@w1 four");
    let ia4 = format!("{}.p{a4}", r.m.id(a));
    r.free("w1");
    let c = r.next_run("w1");
    let out = r.answer_next(c, json!({"head": "Give", "now": [ia4, ia1], "rest": [ib1]}));
    assert!(out.effects.contains(&Effect::Hand { to: "w1".into(), items: vec![ia1.clone(), ia4.clone()], rest: vec![ib1.clone()], digest: vec![] }), "{out:?}");
    assert_eq!(r.m.kept("w1"), BTreeMap::from([(ib1.clone(), "it reads the log first".to_string())]));
    r.given("w1", a, a4);
    assert_eq!(r.waiting_ids("w1"), vec![ib1]);
    r.replayed();
}

/// `stopped`: reported, never a post; the item still waits.
#[test]
fn a_post_for_a_stopped_agent_is_reported() {
    let mut r = Run::new();
    let a = r.talk("w1");
    let o = r.post(a, "owner", "w1", "@w1 are you there?");
    let io = format!("{}.p{o}", r.m.id(a));
    let posts = r.m.tickets[&a].posts.len();
    r.event("stopped", json!({"agent": "w1", "item": io, "why": "w1 is stopped"}));
    assert_eq!(r.m.reported("w1"), BTreeMap::from([(io.clone(), "w1 is stopped".to_string())]));
    assert_eq!(r.m.tickets[&a].posts.len(), posts);
    assert_eq!(r.waiting_ids("w1"), vec![io]);
    r.replayed();
}

/// The owner's hold on agents' mail (line 327, its arms in delivery.kind; consult-3 N-1324 p32, paper-research N-1396
/// p180). His post written before the hold that had not gone yet goes at the first `free` under it, alone, as it would
/// without it (paper-research N-1396 p235, a). x's post to idle w1 is kept with the hold's words, and no `next` is
/// asked. y's, arrived in a turn, is kept the same, not for the turn's end. His post written after the hold goes at
/// once, alone, and neither held post is named at its foot. A replay rebuilds the hold and both keeps. His Release lets
/// both go: the next `free` offers them to `next`, kept by nothing.
#[test]
fn the_hold_keeps_an_agents_post_his_goes_and_release_lets_what_it_kept_go() {
    let mut r = Run::with(NEXT_KIND);
    let a = r.talk("w1");
    let b = r.talk("w1");
    // his post from before the hold, not gone yet
    let p = r.post(b, "owner", "w1", "@w1 first, the logs");
    let ip = format!("{}.p{p}", r.m.id(b));
    r.hold(true);
    let why = String::from("the owner holds agents' mail since 09:00 UTC");
    let x = r.post(a, "x", "w1", "@w1 the build is green");
    let ix = format!("{}.p{x}", r.m.id(a));
    let out = r.free("w1");
    assert!(out.effects.contains(&Effect::Hand { to: "w1".into(), items: vec![ip.clone()], rest: vec![], digest: vec![] }), "his from before the hold goes, alone: {out:?}");
    assert!(!r.m.deciding("w1"), "no next for what the hold keeps");
    assert_eq!(r.m.kept("w1"), BTreeMap::from([(ix.clone(), why.clone())]));
    assert_eq!(r.acted(), vec![("w1".into(), "keep".into(), vec![ix.clone()], vec![]), ("w1".into(), "deliver".into(), vec![ip], vec![])]);
    r.given("w1", b, p);
    // in a turn: its arrived keeps it with the hold's words
    let y = r.post(a, "y", "w1", "@w1 and the docs?");
    let iy = format!("{}.p{y}", r.m.id(a));
    r.event("arrived", json!({"agent": "w1", "item": iy}));
    assert_eq!(r.m.kept("w1").get(&iy), Some(&why));
    // his post, written after the hold: at once, alone, nothing held at its foot
    let o = r.post(b, "owner", "w1", "@w1 stop and look at the median");
    let io = format!("{}.p{o}", r.m.id(b));
    let out = r.free("w1");
    assert!(out.effects.contains(&Effect::Hand { to: "w1".into(), items: vec![io.clone()], rest: vec![], digest: vec![] }), "{out:?}");
    assert!(!r.m.deciding("w1"));
    r.given("w1", b, o);
    let m = r.replayed();
    assert_eq!((m.kept("w1"), m.mail_hold.is_some()), (r.m.kept("w1"), true));
    // Release: kept no more, and the next free offers both to next
    let said = r.hold(false);
    assert!(said.ends_with("The 2 things it kept are decided again."), "{said}");
    assert!(r.m.kept("w1").is_empty(), "{:?}", r.m.kept("w1"));
    r.free("w1");
    let c = r.next_run("w1");
    assert_eq!(r.offered(c), vec![(ix.clone(), None), (iy.clone(), None)]);
    let out = r.answer_next(c, json!({"head": "Give", "now": [ix, iy], "rest": []}));
    assert!(out.effects.contains(&Effect::Hand { to: "w1".into(), items: vec![ix, iy], rest: vec![], digest: vec![] }), "{out:?}");
    let m = r.replayed();
    assert_eq!((m.kept("w1").len(), m.mail_hold.is_none()), (0, true));
}

/// The hold and a thread's order (line 327). The state changes between the read and the act: x's post, held, stands
/// before his on their thread, and his `arrived` runs before any event for x's. `deliver` hands his alone and keeps
/// x's with the hold's words, so his `given` passing x's post loses nothing. Release leaves it kept below the read mark,
/// now for the hold's end, and the next `free` offers it to `next`. A `next` open when the hold goes on gives what the
/// hold keeps to nobody: it is kept.
#[test]
fn a_held_post_before_his_on_its_thread_is_kept_and_not_carried_along_and_waits_past_the_release() {
    let mut r = Run::with(NEXT_KIND);
    let a = r.talk("w1");
    r.hold(true);
    let x = r.post(a, "x", "w1", "@w1 the build is green");
    let o = r.post(a, "owner", "w1", "@w1 stop and look at the median");
    let (ix, io) = (format!("{}.p{x}", r.m.id(a)), format!("{}.p{o}", r.m.id(a)));
    let out = r.event("arrived", json!({"agent": "w1", "item": io}));
    assert!(out.effects.contains(&Effect::Hand { to: "w1".into(), items: vec![io.clone()], rest: vec![], digest: vec![] }), "{out:?}");
    assert_eq!(r.acted(), vec![("w1".into(), "deliver".into(), vec![io.clone()], vec![]), ("w1".into(), "keep".into(), vec![ix.clone()], vec![])]);
    r.given("w1", a, o);
    assert_eq!(r.waiting_ids("w1"), vec![ix.clone()], "below the read mark, by its keep");
    r.clock += 120;
    let said = r.hold(false);
    assert!(said.ends_with("The one thing it kept is decided again."), "{said}");
    let ended = String::from("the owner held agents' mail from 09:00 to 09:02 UTC");
    assert_eq!(r.m.kept("w1"), BTreeMap::from([(ix.clone(), ended.clone())]));
    r.free("w1");
    let c = r.next_run("w1");
    assert_eq!(r.offered(c), vec![(ix.clone(), Some(ended))]);
    let out = r.answer_next(c, json!({"head": "Give", "now": [ix], "rest": []}));
    assert!(out.effects.contains(&Effect::Hand { to: "w1".into(), items: vec![ix.clone()], rest: vec![], digest: vec![] }), "{out:?}");
    assert!(r.m.kept("w1").is_empty());
    r.given("w1", a, x);
    // a next open when the hold goes on: what it gives is kept, with the hold's words, not handed
    let z = r.post(a, "z", "w1", "@w1 one more");
    let iz = format!("{}.p{z}", r.m.id(a));
    r.free("w1");
    let c = r.next_run("w1");
    r.hold(true);
    let out = r.answer_next(c, json!({"head": "Give", "now": [iz], "rest": []}));
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::Hand { .. })), "{out:?}");
    assert_eq!(r.m.kept("w1"), BTreeMap::from([(iz.clone(), String::from("the owner holds agents' mail since 09:02 UTC"))]));
    let m = r.replayed();
    assert_eq!(m.kept("w1"), r.m.kept("w1"));
}

/// The Docket's act chips (paper-research N-1396 p223, p247; consult-3 p250): each agent's desk keeps its last acts,
/// the newest first, as its `delivery_acted` lines say them (act, items, rest, why, when), ACTS_KEPT of them; each act
/// tells coordd (`Effect::Acted`); a machine replayed from the record keeps the same, and writes no line more.
#[test]
fn the_desk_keeps_its_last_acts_as_the_record_says_and_a_replay_keeps_the_same() {
    let mut r = Run::with(NEXT_KIND);
    let a = r.talk("w1");
    // a keep, with its why: the watcher's next rests, keeping all that waits; the desk keeps its first ACT_IDS ids
    // and counts the others
    let ids: Vec<String> = (0..ACT_IDS + 3).map(|i| format!("{}.p{}", r.m.id(a), r.post(a, "x", "w1", &format!("@w1 one {i}")))).collect();
    r.free("w1");
    let c = r.next_run("w1");
    let out = r.answer_next(c, json!({"head": "Rest", "why": "it reads the log first"}));
    assert!(out.effects.contains(&Effect::Acted { agent: "w1".into() }), "{:?}", out.effects);
    let acts = r.m.last_acts("w1");
    assert_eq!((acts[0].act.as_str(), acts[0].items.clone(), acts[0].more_items, acts[0].why.as_deref()), ("keep", ids[..ACT_IDS].to_vec(), 3, Some("it reads the log first")));
    // more reports than the desk keeps: the newest ACTS_KEPT, the newest first
    let b = r.talk("w2");
    let mut items = Vec::new();
    for i in 0..machine::ACTS_KEPT + 2 {
        let n = r.post(b, "owner", "w2", &format!("@w2 are you there? {i}"));
        let id = format!("{}.p{n}", r.m.id(b));
        let out = r.event("stopped", json!({"agent": "w2", "item": id, "why": format!("w2 is stopped {i}")}));
        assert!(out.effects.contains(&Effect::Acted { agent: "w2".into() }), "{:?}", out.effects);
        items.push(id);
    }
    let acts = r.m.last_acts("w2");
    assert_eq!(acts.len(), machine::ACTS_KEPT);
    assert_eq!(acts[0].items, vec![items.last().unwrap().clone()]);
    assert_eq!(acts[0].why.as_deref(), Some(format!("w2 is stopped {}", machine::ACTS_KEPT + 1).as_str()));
    assert!(acts.iter().all(|x| x.act == "report") && acts.windows(2).all(|w| w[0].at >= w[1].at), "{acts:?}");
    // as the record says them, the newest first: the record names every id
    assert_eq!(r.acted().iter().find(|x| x.0 == "w1").map(|x| x.2.clone()), Some(ids.clone()));
    for agent in ["w1", "w2"] {
        let first = |xs: Vec<String>| (xs.iter().take(ACT_IDS).cloned().collect::<Vec<_>>(), xs.len().saturating_sub(ACT_IDS));
        let lines: Vec<_> = r.acted().into_iter().filter(|(x, ..)| x == agent).rev().take(machine::ACTS_KEPT).map(|(_, act, items, rest)| (act, first(items), first(rest))).collect();
        let kept: Vec<_> = r.m.last_acts(agent).into_iter().map(|x| (x.act, (x.items, x.more_items), (x.rest, x.more_rest))).collect();
        assert_eq!(kept, lines, "{agent}");
    }
    // a replay keeps the same acts, at the same times
    let lines = r.log.len();
    let m = r.replayed();
    for agent in ["w1", "w2", "w3"] {
        assert_eq!(m.last_acts(agent), r.m.last_acts(agent), "{agent}");
    }
    assert_eq!(r.log.len(), lines, "nothing more written");
}

/// A delivery event names its agent, and a delivery program must be loaded; a run's acts reach only its own agent.
#[test]
fn delivery_events_are_refused_without_their_agent_or_a_program() {
    let mut r = Run::with(NEXT_KIND);
    assert!(r.go(Input::Event { name: "free".into(), value: json!({}) }).reply.is_err());
    assert!(r.go(Input::Event { name: "arrived".into(), value: json!({"agent": "w1"}) }).reply.is_err());
    // the runtime's own record lines are not sent
    for name in [syntax::DECIDED, syntax::DELIVERY_OPENED, syntax::ACTED] {
        assert!(r.go(Input::Event { name: name.into(), value: json!({"agent": "w1"}) }).reply.is_err(), "{name}");
    }
    let mut m = Machine::load({
        let mut d = defs_with(DELIVERY_KIND);
        d.kinds.remove("delivery");
        d
    });
    assert!(m.apply(Input::Event { name: "free".into(), value: json!({"agent": "w1"}) }, "t".into()).reply.is_err());
    assert!(!m.takes_event("free", &json!({"agent": "w1"})));
    assert!(r.m.takes_event("free", &json!({"agent": "w1"})));
    // nothing waits: no run, no ticket
    let out = r.free("w1");
    assert_eq!(out.reply, Ok("Nothing to decide for w1.".into()));
    assert!(r.m.tickets.range(HIDDEN..).next().is_none());
}

/// The record the old-engine test replays with train 26's machine (a cb0acab7 branch, `way_back.jsonl` beside this
/// file): what this engine writes for the cases above, in one record.
const WAY_BACK: &str = include_str!("way_back.jsonl");

/// The cases above in one record: his post at once, a `next` given and one rested, an event waiting behind a run, his
/// post ending an open `next`, a reply (its turn's end with the tools and model it used: insight (1)), a stopped agent;
/// the talk kind's owner_post runs after the runtime's tickets, numbered as train 26 numbers them.
fn way_back_run() -> Run {
    let mut r = Run::with(NEXT_KIND);
    let a = r.talk("w1");
    let b = r.talk("w1");
    let c = r.talk("w2");
    // his post at once, alone, the rest named
    let x = r.post(a, "x", "w1", "@w1 the build is green");
    let o = r.post(b, "owner", "w1", "@w1 stop and look at the median");
    r.free("w1");
    r.given("w1", b, o);
    // free asks next, which gives one and names the other; the turn ends with a reply
    let y = r.post(b, "y", "w1", "@w1 which branch?");
    r.free("w1");
    let run = r.next_run("w1");
    let (ix, iy) = (format!("{}.p{x}", r.m.id(a)), format!("{}.p{y}", r.m.id(b)));
    r.answer_next(run, json!({"head": "Give", "now": [iy], "rest": [ix.clone()]}));
    r.given("w1", b, y);
    // what the turn did, as coordd's follower tells it (insight (1)): the way back reads past `tools` and `model`
    r.m.turn_worked = Some(("w1".into(), Worked { tools: BTreeMap::from([("Bash".into(), 1), ("mcp__coord__reply".into(), 1)]), model: Some("claude-opus-5-5".into()) }));
    r.ok(Input::TurnEnded { agent: "w1".into(), delivery: Some(iy), text: Loc { file: "w.jsonl".into(), offset: 500, len: 40 }, later: vec![], stood: None, go: None });
    r.m.turn_worked = None;
    // an agent's post comes during the next run and waits behind it; the run rests, and the post is kept
    r.free("w1");
    let run = r.next_run("w1");
    let z = r.post(a, "z", "w1", "@w1 one more");
    let iz = format!("{}.p{z}", r.m.id(a));
    r.event("arrived", json!({"agent": "w1", "item": iz.clone()}));
    r.answer_next(run, json!({"head": "Rest", "why": "it reads the log first"}));
    // his post ends an open next and goes at once; the turn it starts replies
    r.free("w1");
    r.next_run("w1");
    let o2 = r.post(b, "owner", "w1", "@w1 the release is tonight");
    r.free("w1");
    r.given("w1", b, o2);
    let io2 = format!("{}.p{o2}", r.m.id(b));
    r.ok(Input::TurnEnded { agent: "w1".into(), delivery: Some(io2), text: Loc { file: "w.jsonl".into(), offset: 600, len: 40 }, later: vec![], stood: None, go: None });
    // his post on a, handed while x and z are kept there, and the turn's reply; then they are handed, and the reply to
    // them is the agent's own post (`kept_reply`)
    let o3 = r.post(a, "owner", "w1", "@w1 and the median?");
    let io3 = format!("{}.p{o3}", r.m.id(a));
    r.free("w1");
    r.given("w1", a, o3);
    r.ok(Input::TurnEnded { agent: "w1".into(), delivery: Some(io3), text: Loc { file: "w.jsonl".into(), offset: 700, len: 40 }, later: vec![], stood: None, go: None });
    r.free("w1");
    let run = r.next_run("w1");
    r.answer_next(run, json!({"head": "Give", "now": [ix, iz], "rest": []}));
    r.given("w1", a, z);
    r.ok(Input::TurnEnded { agent: "w1".into(), delivery: Some(iz), text: Loc { file: "w.jsonl".into(), offset: 800, len: 40 }, later: vec![], stood: None, go: None });
    // a stopped agent: reported
    let p = r.post(c, "owner", "w2", "@w2 are you there?");
    let ip = format!("{}.p{p}", r.m.id(c));
    r.event("stopped", json!({"agent": "w2", "item": ip, "why": "w2 is stopped"}));
    r
}

#[test]
fn the_way_back_record_is_what_this_engine_writes() {
    let r = way_back_run();
    let text: String = r.log.iter().map(|l| format!("{}\n", serde_json::to_string(l).unwrap())).collect();
    println!("way_back.jsonl, as this engine writes it:\n{text}");
    r.replayed();
    assert!(r.log.iter().any(|l| matches!(l, Line::Event { name, .. } if name == syntax::DECIDED)));
    assert!(r.log.iter().any(|l| matches!(l, Line::Opened { t, kind, .. } if *t >= HIDDEN && *t < DESK && kind == "owner_post")));
    assert!(r.log.iter().any(|l| matches!(l, Line::Post { by, answers: Some(_), .. } if by == "w1")));
    assert_eq!(r.m.kept("w1"), BTreeMap::new());
    assert!(text == WAY_BACK, "way_back.jsonl is not what this engine writes (printed above)");
}

/// What a post is, on its line (I-1480, paper-research N-1481 p1; proto-engine p14): each kind as the record writes it,
/// an untyped post's line as before, a line from before read as untyped, and a typed ask naming whom it is for.
#[test]
fn a_typed_posts_line_says_what_it_is_and_an_untyped_ones_is_as_before() {
    let words = [
        (PostKind::Ask { to: vec!["w2".into()] }, json!({"is": "ask", "to": ["w2"]})),
        (PostKind::Read { to: vec!["w2".into()], head: "core2-posted 149feba4".into() }, json!({"is": "read", "to": ["w2"], "head": "core2-posted 149feba4"})),
        (PostKind::Blocker { to: vec!["w2".into()] }, json!({"is": "blocker", "to": ["w2"]})),
        (PostKind::Hold { on: "I-1474".into() }, json!({"is": "hold", "on": "I-1474"})),
        (PostKind::Result, json!({"is": "result"})),
        (PostKind::Fyi, json!({"is": "fyi"})),
    ];
    for (k, v) in &words {
        assert_eq!(serde_json::to_value(k).unwrap(), *v);
        assert_eq!(serde_json::from_value::<PostKind>(v.clone()).unwrap(), *k);
        assert_eq!(k.is(), v["is"].as_str().unwrap());
    }
    assert_eq!(words.iter().map(|(k, _)| k.to().len()).collect::<Vec<_>>(), [1, 1, 1, 0, 0, 0]);
    let mut r = Run::new();
    let t = r.talk("w2");
    // w3 asks w2, typed, naming nobody with @: the line names w2 and says what it is
    r.ok(Input::Post { t, by: "w3".into(), text: Text::New("check the lock".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: Some(PostKind::Ask { to: vec!["w2".into()] }) });
    let n = r.m.tickets[&t].last();
    let line = r.log.iter().rev().find(|l| matches!(l, Line::Post { .. })).unwrap().clone();
    let Line::Post { names, kind, .. } = &line else { unreachable!() };
    assert_eq!((names.clone(), kind.clone()), (vec!["w2".to_string()], Some(PostKind::Ask { to: vec!["w2".into()] })));
    let written = serde_json::to_value(&line).unwrap();
    assert_eq!(written["kind"], json!({"is": "ask", "to": ["w2"]}), "{written}");
    let p = r.m.tickets[&t].posts.iter().find(|p| p.n == n).unwrap();
    assert_eq!(p.kind, Some(PostKind::Ask { to: vec!["w2".into()] }));
    assert_eq!(serde_json::to_value(p).unwrap()["kind"], json!({"is": "ask", "to": ["w2"]}));
    // an untyped post: its line and its post have no `kind`, as every post before
    r.post(t, "w3", "w2", "and the runner");
    let line = r.log.iter().rev().find(|l| matches!(l, Line::Post { .. })).unwrap();
    assert!(serde_json::to_value(line).unwrap().get("kind").is_none());
    let last = r.m.tickets[&t].posts.last().unwrap();
    assert_eq!((last.kind.clone(), serde_json::to_value(last).unwrap().get("kind").cloned()), (None, None));
    // a line from before reads as untyped; the record replays as written
    let old: Line = serde_json::from_value(json!({"ev": "post", "at": "2026-11-20T09:00:09Z", "t": t, "by": "w3", "text": "old words", "names": ["w2"]})).unwrap();
    assert!(matches!(old, Line::Post { kind: None, .. }));
    let mut again = Machine::load(defs_with(DELIVERY_KIND));
    assert_eq!(again.replay(&r.log), Vec::<String>::new());
    assert_eq!(again.tickets[&t].posts.iter().find(|p| p.n == n).unwrap().kind, Some(PostKind::Ask { to: vec!["w2".into()] }));
}

/// W3's post on T typed KIND (None: untyped), naming NAMES and informing INFORM as coordd and policy would: its id.
fn typed(r: &mut Run, t: u64, by: &str, names: &[&str], inform: &[&str], kind: Option<PostKind>, text: &str) -> String {
    let s = |xs: &[&str]| xs.iter().map(|a| a.to_string()).collect::<Vec<String>>();
    r.ok(Input::Post { t, by: by.into(), text: Text::New(text.into()), names: s(names), told: vec![], inform: s(inform), answers: None, kind });
    format!("{}.p{}", r.m.id(t), r.m.tickets[&t].last())
}

/// What waits for AGENT as ID: whether it goes now, and why (I-1480's `push` and `why`); None when it is not waiting.
fn pushed(r: &Run, agent: &str, id: &str) -> Option<(bool, Option<String>)> {
    push_in(&r.m, agent, id)
}

/// `pushed`, in machine M: whether item ID goes to AGENT now, and why.
fn push_in(m: &Machine, agent: &str, id: &str) -> Option<(bool, Option<String>)> {
    m.waiting(agent).into_iter().find(|i| i["id"] == json!(id)).map(|i| (i["push"] == json!(true), i["why"].as_str().map(str::to_string)))
}

/// What post N of T changes by the era [`ERA_CLOSES`] (`Machine::close_changes`): agent, why before and by it, and
/// where and how the two closes part.
fn changes(r: &Run, t: u64, n: u32) -> Vec<(String, Option<&'static str>, Option<&'static str>, u32, &'static str)> {
    r.m.close_changes(t, n).into_iter().map(|k| (k.agent, k.before, k.by, k.at, k.how)).collect()
}

/// Whether line L begins the era [`ERA_CLOSES`].
fn closes_era(l: &Line) -> bool {
    matches!(l, Line::Event { name, value, .. } if name == ERA && value["era"] == json!(ERA_CLOSES))
}

/// LOG as the rule before the era [`ERA_CLOSES`] reads it: the era's line taken out, replayed with no problem line.
fn before_the_close(log: &[Line]) -> Machine {
    let before: Vec<Line> = log.iter().filter(|l| !closes_era(l)).cloned().collect();
    let mut m = Machine::load(defs_with(DELIVERY_KIND));
    assert_eq!(m.replay(&before), Vec::<String>::new());
    m
}

/// I-1480 rule 1 (paper-research N-1481 p1, p4): a post goes to the agent now only when it must act, typed so or
/// named with an untyped @; an fyi, typed or naming nobody, is pulled; a hold reaches whoever works on its item.
#[test]
fn an_ask_read_blocker_or_untyped_at_goes_now_and_an_fyi_is_pulled() {
    let mut r = Run::new();
    let a = r.talk("w3");
    let to = |a: &str| vec![a.to_string()];
    let ask = typed(&mut r, a, "w3", &[], &[], Some(PostKind::Ask { to: to("w2") }), "check the lock");
    let at = typed(&mut r, a, "w3", &["w2"], &[], None, "@w2 and the runner");
    let fyi_at = typed(&mut r, a, "w3", &["w2"], &[], Some(PostKind::Fyi), "fyi: the lock passed, @w2");
    let read = typed(&mut r, a, "w3", &[], &[], Some(PostKind::Read { to: to("w2"), head: "core2-x 1234abcd".into() }), "read the fix");
    let blocker = typed(&mut r, a, "w3", &[], &[], Some(PostKind::Blocker { to: to("w2") }), "the tower is yours to free");
    let result = typed(&mut r, a, "w4", &[], &["w2"], Some(PostKind::Result), "61 is cut");
    let nameless = typed(&mut r, a, "w4", &[], &["w2"], None, "the notes are up");
    let s = |w: &str| Some((true, Some(w.to_string())));
    assert_eq!(pushed(&r, "w2", &ask), s("ask"));
    assert_eq!(pushed(&r, "w2", &at), s("named"));
    assert_eq!(pushed(&r, "w2", &fyi_at), Some((false, None)), "an @ on a typed fyi is the writer's mistake: pulled");
    assert_eq!(pushed(&r, "w2", &read), s("read"));
    assert_eq!(pushed(&r, "w2", &blocker), s("blocker"));
    assert_eq!(pushed(&r, "w2", &result), Some((false, None)), "a result for nobody waiting on it is pulled");
    assert_eq!(pushed(&r, "w2", &nameless), Some((false, None)), "an untyped post naming nobody is an fyi");
    // a hold on an item w2 works on (`coord item start`) reaches it, named for it wherever it is posted
    let item = r.talk("w2");
    r.m.tickets.edit().get_mut(&item).unwrap().started.push("w2".into());
    let on = r.m.id(item);
    let hold = typed(&mut r, a, "lead", &[], &[], Some(PostKind::Hold { on: on.clone() }), "hold the merge");
    assert_eq!(pushed(&r, "w2", &hold), s("hold"));
    assert!(r.m.tickets[&a].posts.last().unwrap().names.contains(&"w2".to_string()));
    // the item is w2's own, the thread is not: its digest lists its own first (N-1481 p25)
    let mine = typed(&mut r, item, "w4", &[], &["w2"], Some(PostKind::Fyi), "on the item");
    let own = |id: &str| r.m.waiting("w2").into_iter().find(|i| i["id"] == json!(id)).map(|i| i["own"] == json!(true));
    assert_eq!((own(&mine), own(&ask)), (Some(true), Some(false)));
    // a step goes now, as it woke before
    assert!(r.m.waiting("w2").iter().all(|i| i["what"] == json!("post") || i["push"] == json!(true)));
    assert_eq!(Machine::load(defs_with(DELIVERY_KIND)).replay(&r.log), Vec::<String>::new());
}

/// I-1480's answer rule (N-1481 p7, p8, p12): an answer goes now to whoever asked its writer something, by what it
/// answers; to the author of an fyi it answered it is pulled. One reply answering an ask and an fyi together goes now
/// to the asker alone, and the asker's "thanks" back is pulled: no ping-pong.
#[test]
fn an_answer_goes_now_to_whoever_asked_and_is_pulled_for_an_fyi() {
    let mut r = Run::new();
    let b = r.talk("w3");
    let asked = typed(&mut r, b, "w2", &["w3"], &[], None, "@w3 which branch?");
    let fyi = typed(&mut r, b, "w4", &["w3"], &[], Some(PostKind::Fyi), "fyi @w3: the docs moved");
    let upto = r.m.tickets[&b].last();
    r.given("w3", b, upto);
    // w3's turn answers both with one post naming nobody: the rule for it names both authors
    let reply = typed(&mut r, b, "w3", &[], &[], None, "main; noted");
    let names = r.m.tickets[&b].posts.last().unwrap().names.clone();
    assert_eq!(names, ["w2", "w4"], "{asked} {fyi}");
    assert_eq!(pushed(&r, "w2", &reply), Some((true, Some("reply".into()))));
    assert_eq!(pushed(&r, "w4", &reply), Some((false, None)), "an answer to an fyi is pulled");
    // its asker's thanks, an answer to an answer, asks nothing: pulled
    let upto = r.m.tickets[&b].last();
    r.given("w2", b, upto);
    let thanks = typed(&mut r, b, "w2", &[], &[], None, "thanks");
    assert_eq!(r.m.tickets[&b].posts.last().unwrap().names, ["w3"]);
    assert_eq!(pushed(&r, "w3", &thanks), Some((false, None)));
    // typed: a result that answers w2's ask goes to it by the same rule, however it was sent
    let asked = typed(&mut r, b, "w2", &[], &[], Some(PostKind::Ask { to: vec!["w3".into()] }), "and the release?");
    let upto = r.m.tickets[&b].last();
    r.given("w3", b, upto);
    let done = typed(&mut r, b, "w3", &[], &[], Some(PostKind::Result), "it is in 61");
    assert_eq!(pushed(&r, "w2", &done), Some((true, Some("reply".into()))), "{asked}");
    assert_eq!(Machine::load(defs_with(DELIVERY_KIND)).replay(&r.log), Vec::<String>::new());
}

/// The waits-on edge (paper-research N-1481 p20, proto-reader p18): X's ask to Y on N-12 brings X Y's first result
/// there, once, though it answers nothing (Y was not handed the ask yet); Y's result elsewhere, and its second one
/// there, are pulled. So the rule before the era [`ERA_CLOSES`] handed them, and the record with its era line taken
/// out replays so. From the era (consult-3 N-1479 p127, proto-engine p130) a result Y wrote before it was handed the
/// ask is no answer to it: the first begins the era and is pulled with the rest, and the ask stays open.
#[test]
fn a_result_goes_once_to_whoever_waits_on_its_writer_there_and_no_further() {
    let mut r = Run::new();
    let c = r.talk("w3");
    let d = r.talk("w3");
    let ask = typed(&mut r, c, "w2", &[], &[], Some(PostKind::Ask { to: vec!["w3".into()] }), "run the replay");
    let elsewhere = typed(&mut r, d, "w3", &[], &["w2"], Some(PostKind::Result), "the snapshot is done");
    assert!(r.m.era < ERA_CLOSES, "{elsewhere} goes as before");
    let first = typed(&mut r, c, "w3", &[], &["w2"], Some(PostKind::Result), "the replay: 0 problem lines");
    assert_eq!(r.m.era, ERA_CLOSES, "{first} goes otherwise: it begins the era");
    let second = typed(&mut r, c, "w3", &[], &["w2"], Some(PostKind::Result), "and cav-paper: 0");
    for p in [&elsewhere, &first, &second] {
        assert_eq!(pushed(&r, "w2", p), Some((false, None)), "{p}: w3 was not handed {ask}");
    }
    assert_eq!(r.m.ledger("w3").into_iter().find(|o| o.post == ask).map(|o| o.answer), Some(None));
    let old = before_the_close(&r.log);
    assert_eq!(old.era, 0);
    assert_eq!(push_in(&old, "w2", &elsewhere), Some((false, None)));
    assert_eq!(push_in(&old, "w2", &first), Some((true, Some("result".into()))));
    assert_eq!(push_in(&old, "w2", &second), Some((false, None)));
}

/// Delivery closes by the ledger's rule from the era ERA_CLOSES (paper-research N-1479 p107; proto-reader's N-5, p103;
/// consult-3's case, p105): X's ask, then Y's post that only passes it on ("@w4 check this"), then Y's result. The pass
/// is not Y's answer to X: it is pulled into X's digest, where the rule before had it go as Y's reply, and it begins
/// the era, the line just after it. The edge waits on past it, so the result goes to X now: it is the post the ledger
/// takes as the answer, and so it goes as Y's reply ("reply"; from the era a post that closes the ask is the reply the
/// edge waited on, so the edge's own why, "result", is the rule before's). The record replays so; with the era's line
/// taken out, as the rule before handed: the pass as X's reply, the result pulled.
#[test]
fn a_pass_on_is_no_answer_and_the_result_after_it_goes_to_the_asker_from_the_era_of_the_close() {
    let mut r = Run::new();
    let c = r.talk("w3");
    let ask = typed(&mut r, c, "w2", &[], &[], Some(PostKind::Ask { to: vec!["w3".into()] }), "run the replay");
    let upto = r.m.tickets[&c].last();
    r.given("w3", c, upto);
    assert!(r.m.era < ERA_CLOSES, "nothing before the pass goes otherwise");
    let at = r.log.len();
    let pass = typed(&mut r, c, "w3", &["w4"], &["w2"], None, "@w4 check this");
    let n = r.m.tickets[&c].last();
    assert_eq!(changes(&r, c, n), [("w2".to_string(), Some("reply"), None, n, "hand-off")]);
    assert!(matches!(r.log[at], Line::Post { .. }) && r.log.iter().position(closes_era) == Some(r.log.len() - 1), "the era's line just after the pass: {:?}", &r.log[at..]);
    assert_eq!(r.m.era, ERA_CLOSES);
    assert_eq!(pushed(&r, "w2", &pass), Some((false, None)), "the pass is not w3's answer to w2: into its digest");
    let owed = |r: &Run| r.m.ledger("w3").into_iter().find(|o| o.post == ask).map(|o| o.answer);
    assert_eq!(owed(&r), Some(None), "{pass} passes it on");
    let done = typed(&mut r, c, "w3", &[], &["w2"], Some(PostKind::Result), "the replay: 0 problem lines");
    assert_eq!(changes(&r, c, n + 1), [("w2".to_string(), None, Some("reply"), n, "hand-off")], "pulled before: the pass closed the edge");
    assert_eq!(pushed(&r, "w2", &done), Some((true, Some("reply".into()))), "the edge waits on past the pass");
    assert_eq!(owed(&r), Some(Some(done.clone())));
    let m = r.replayed();
    assert_eq!((push_in(&m, "w2", &pass), push_in(&m, "w2", &done)), (Some((false, None)), Some((true, Some("reply".into())))), "replayed in the era");
    let old = before_the_close(&r.log);
    assert_eq!((old.era, push_in(&old, "w2", &pass), push_in(&old, "w2", &done)), (0, Some((true, Some("reply".into()))), Some((false, None))), "with no era line, as the rule before handed");
}

/// A result written before its writer was handed the ask (consult-3, N-1479 p127): from the era ERA_CLOSES it is no
/// answer to the ask. It is pulled for the asker and leaves the edge open, where the rule before had it go and close
/// the edge, and it begins the era. The result written once the ask was handed goes to the asker as its writer's reply.
#[test]
fn a_result_written_before_its_writer_was_handed_the_ask_is_pulled_and_the_next_goes() {
    let mut r = Run::new();
    let c = r.talk("w3");
    let ask = typed(&mut r, c, "w2", &[], &[], Some(PostKind::Ask { to: vec!["w3".into()] }), "run the replay on 63");
    let early = typed(&mut r, c, "w3", &[], &["w2"], Some(PostKind::Result), "the replay on 62: 0 problem lines");
    let n = r.m.tickets[&c].last();
    assert_eq!(changes(&r, c, n), [("w2".to_string(), Some("result"), None, n, "unhanded")]);
    assert_eq!(r.m.era, ERA_CLOSES, "{early} goes otherwise: it begins the era");
    assert_eq!(pushed(&r, "w2", &early), Some((false, None)), "w3 had not been handed {ask}");
    let upto = r.m.tickets[&c].last();
    r.given("w3", c, upto);
    let done = typed(&mut r, c, "w3", &[], &["w2"], Some(PostKind::Result), "the replay on 63: 0 problem lines");
    assert_eq!(changes(&r, c, r.m.tickets[&c].last()), [], "the rule before took it as the reply too");
    assert_eq!(pushed(&r, "w2", &done), Some((true, Some("reply".into()))));
    assert_eq!(r.m.ledger("w3").into_iter().find(|o| o.post == ask).map(|o| o.answer), Some(Some(done.clone())));
    r.replayed();
}

/// The edge's own why from the era [`ERA_CLOSES`] (proto-reader N-1479 p342, paper-research p358): an ask that did not
/// wake its writer's answerer. On a slowed thread w2's "@w3 …" is halted for his approval, and his card lets it go
/// waking nobody (`wakes` []: routed, its `told` only). It still asks w3, whose name its writer typed (`asks`), so once
/// w3 is handed it, by his own post to w3 there, w3's result answers it by the ledger's rule; but it is no reply, since
/// it never woke w3 (`answered_first`). So the result goes to w2 by the waits-on edge, as "result".
#[test]
fn from_the_era_a_result_to_an_ask_that_did_not_wake_its_writer_goes_by_the_edge() {
    let mut r = Run::new();
    r.ok(Input::Event { name: ERA.into(), value: json!({"era": ERA_CLOSES}) });
    let c = r.talk("w3");
    r.ok(Input::Slow { t: c, on: true, by: "owner".into() });
    let ask = typed(&mut r, c, "w2", &["w3"], &[], None, "@w3 run the replay");
    let n = r.m.tickets[&c].last();
    let card = r.m.tickets[&c].halts.iter().find(|h| h.what.name() == "deliver").map(|h| h.n).expect("his card");
    r.ok(Input::HaltEdit { t: c, n: card, by: "owner".into(), value: json!({"wakes": []}) });
    r.ok(Input::Approve { t: c, n: card, by: "owner".into() });
    let m = r.m.tickets[&c].posts.iter().find(|p| p.n == n).unwrap();
    assert!(m.routed && m.wakes().next().is_none() && m.visible() && m.names == ["w3"], "{m:?}");
    let his = r.post(c, "owner", "w3", "@w3 w2 asks you something on this thread");
    r.free("w3");
    r.given("w3", c, his);
    let done = typed(&mut r, c, "w3", &[], &["w2"], Some(PostKind::Result), "the replay: 0 problem lines");
    assert_eq!(r.m.era, ERA_CLOSES);
    assert_eq!(pushed(&r, "w2", &done), Some((true, Some("result".into()))), "by the edge: {ask} never woke w3");
    r.replayed();
}

/// The rule Reply closes by the ledger's rule (paper-research N-1479 p136, on proto-reader's N-9): w3, handed his p1,
/// passes it on mid-turn ("@w4 …", p2), and its turn ends saying yes. Under the delivery program the rule before took
/// that turn as answered by p2 and replied nowhere, so its message is written first as w3's own post (`kept_reply`):
/// on his thread, naming him, the answer to p1, which it closes in `asked`. The turn's end then adds no Replied line,
/// so the engine before replays the record as written.
#[test]
fn a_turn_that_passed_his_post_on_ends_with_its_answer_to_him() {
    let mut r = Run::new();
    let c = r.talk("w3");
    let id = r.m.id(c);
    let his = r.post(c, "owner", "w3", "@w3 can we keep the d h m breakdown on the accounts tab everywhere?");
    r.free("w3");
    r.given("w3", c, his);
    let pass = typed(&mut r, c, "w3", &["w4"], &[], None, "@w4 please make every duration on the tab d h m");
    assert_eq!(r.m.asked(c, his), vec![("w3".to_string(), None)], "{pass} passes it on");
    let at = r.log.len();
    r.ok(Input::TurnEnded { agent: "w3".into(), delivery: Some(format!("{id}.p{his}")), text: Loc { file: "w.jsonl".into(), offset: 900, len: 40 }, later: vec![], stood: None, go: None });
    let yes = r.m.tickets[&c].last();
    let p = r.m.tickets[&c].posts.last().unwrap();
    assert_eq!((p.by.as_str(), p.names.clone(), p.answers), ("w3", vec!["owner".to_string()], Some(his)), "{:?}", &r.log[at..]);
    assert!(matches!(&r.log[at], Line::Post { t, by, .. } if *t == c && by == "w3"), "written first: {:?}", &r.log[at..]);
    assert!(!r.log[at..].iter().any(|l| matches!(l, Line::Replied { .. })), "{:?}", &r.log[at..]);
    assert_eq!(r.m.asked(c, his), vec![("w3".to_string(), Some(format!("{id}.p{yes}")))]);
    r.replayed();
}

/// consult-3's N-1479 p340 steps, on the reply tool's further id (N-1397 p58's `to` of "p180, p179"): w3, handed w2's
/// p1 and p2 in a turn before and its p3 and p4 in this one (its mark before is 2), answers with the reply tool, `to
/// "p4, p3"` (p6), while his p5, "@w3 …", waits unhanded; p5 is then put into the turn (his post goes in at its next
/// step on the text before p716, `INTO_KIND`; an agent's waits for the turn's end, `arrived`), and the turn ends with a
/// message. The Reply holds what the
/// turn was handed, not what was put in while it had its own (`Machine::turn_given`): up to p4, which p6 answers by
/// either rule, so it posts nowhere, as the rule before did, and his p5 is left as at a turn's end, open in `asked`.
/// Had it held p5, the rule before would count the turn answered by p6's further id p3, and the ledger's rule would
/// name him; `kept_reply` takes the rule before's own test, so the message would be written first.
#[test]
fn a_turn_answered_by_the_tools_further_id_replies_as_before_with_his_post_put_in() {
    let mut r = Run::with(INTO_KIND);
    let c = r.talk("w3");
    let id = r.m.id(c);
    r.post(c, "w2", "w3", "@w3 the bench on 62?");
    let two = r.post(c, "w2", "w3", "@w3 and on 63?");
    r.free("w3");
    r.given("w3", c, two);
    // that turn ends on Nothing: written, its reply nothing (Err "No step was answered.")
    let quiet = r.go(Input::TurnEnded { agent: "w3".into(), delivery: Some(format!("{id}.p{two}")), text: Loc { file: "w.jsonl".into(), offset: 100, len: 0 }, later: vec![], stood: None, go: None });
    assert!(quiet.lines.iter().any(|l| matches!(l, Line::TurnEnded { .. })), "{quiet:?}");
    let three = r.post(c, "w2", "w3", "@w3 run the replay on 64");
    let four = r.post(c, "w2", "w3", "@w3 and the way back on 63");
    r.free("w3");
    r.given("w3", c, four);
    let five = r.post(c, "owner", "w3", "@w3 and cav-paper's too");
    let his = format!("{id}.p{five}");
    assert_eq!(hand(&r.event("arrived", json!({"agent": "w3", "item": his.clone()}))), Some((vec![his.clone()], vec![], vec![])), "his goes into the running turn");
    r.ok(Input::Post { t: c, by: "w3".into(), text: Text::New("64: 0 problem lines; 63: 2".into()), names: vec!["w2".into()], told: vec![], inform: vec![], answers: Some(four), kind: None });
    let six = r.m.tickets[&c].last();
    r.ok(Input::AnswersAlso { t: c, n: six, also: vec![format!("{id}.p{three}")] });
    let x = &r.m.tickets[&c];
    let p6 = x.posts.iter().find(|p| p.n == six).unwrap();
    assert_eq!((five, six, x.answers_upto(p6), x.also_here(p6)), (5, 6, 4, vec![three]), "consult-3's p340 steps");
    r.ok(Input::GivenInto { t: c, to: "w3".into(), delivery: his, upto: five, skipped: Default::default(), place: None });
    let rows: Vec<(String, u32, u32)> = r.m.tickets[&c].given.iter().filter(|g| g.to == "w3").map(|g| (g.delivery.clone(), g.before, g.upto)).collect();
    assert_eq!(rows, [(format!("{id}.p{two}"), 0, two), (format!("{id}.p{four}"), two, four), (format!("{id}.p{five}"), six, five)]);
    assert_eq!(r.m.reply_names("w3", &[]), None, "the Reply holds up to p{four}, which p{six} answers");
    let at = r.log.len();
    r.go(Input::TurnEnded { agent: "w3".into(), delivery: Some(format!("{id}.p{four}")), text: Loc { file: "w.jsonl".into(), offset: 900, len: 40 }, later: vec![], stood: None, go: None });
    assert_eq!(r.m.tickets[&c].last(), six, "nothing written first: {:?}", &r.log[at..]);
    assert!(!r.log[at..].iter().any(|l| matches!(l, Line::Replied { .. })), "{:?}", &r.log[at..]);
    assert_eq!(r.m.asked(c, five), vec![("w3".to_string(), None)], "p{five} left as at a turn's end");
    r.replayed();
}

/// The same answer with w2's p5 handed by a second giving of the turn, not put in (consult-3 N-1479 p340, proto-reader
/// p342): w3, handed w2's p3 and p4 (its mark before is 2), answers with the reply tool, `to "p4, p3"` (p6); p5, "@w3
/// …", is then given to it before the turn ends, among what the turn holds, and the turn ends with a message. The rule
/// before counted the turn answered by p6's further id p3 and replied nowhere. By the ledger's rule p5 is still w2's
/// (p6 answers up to p4), so the message goes to w2, written first as w3's own post (`kept_reply`), under p5, naming
/// w2: the turn's end adds no Replied line, which the engine before would not make.
#[test]
fn a_turn_answered_by_the_tools_further_id_ends_with_its_answer_to_a_post_given_after_it() {
    let mut r = Run::new();
    let c = r.talk("w3");
    let id = r.m.id(c);
    r.post(c, "w2", "w3", "@w3 the bench on 62?");
    let two = r.post(c, "w2", "w3", "@w3 and on 63?");
    r.free("w3");
    r.given("w3", c, two);
    let quiet = r.go(Input::TurnEnded { agent: "w3".into(), delivery: Some(format!("{id}.p{two}")), text: Loc { file: "w.jsonl".into(), offset: 100, len: 0 }, later: vec![], stood: None, go: None });
    assert!(quiet.lines.iter().any(|l| matches!(l, Line::TurnEnded { .. })), "{quiet:?}");
    let three = r.post(c, "w2", "w3", "@w3 run the replay on 64");
    let four = r.post(c, "w2", "w3", "@w3 and the way back on 63");
    r.free("w3");
    r.given("w3", c, four);
    let five = r.post(c, "w2", "w3", "@w3 and cav-paper's too");
    r.ok(Input::Post { t: c, by: "w3".into(), text: Text::New("64: 0 problem lines; 63: 2".into()), names: vec!["w2".into()], told: vec![], inform: vec![], answers: Some(four), kind: None });
    let six = r.m.tickets[&c].last();
    r.ok(Input::AnswersAlso { t: c, n: six, also: vec![format!("{id}.p{three}")] });
    let x = &r.m.tickets[&c];
    let p6 = x.posts.iter().find(|p| p.n == six).unwrap();
    assert_eq!((five, six, x.answers_upto(p6), x.also_here(p6)), (5, 6, 4, vec![three]), "consult-3's p340 steps");
    r.given("w3", c, five);
    let rows: Vec<(String, u32, u32)> = r.m.tickets[&c].given.iter().filter(|g| g.to == "w3").map(|g| (g.delivery.clone(), g.before, g.upto)).collect();
    assert_eq!(rows, [(format!("{id}.p{two}"), 0, two), (format!("{id}.p{four}"), two, four), (format!("{id}.p{five}"), four, five)]);
    assert_eq!(r.m.reply_names("w3", &[]), Some((c, vec!["w2".to_string()])), "the Reply holds up to p{five}, which p{six} does not answer");
    let at = r.log.len();
    r.ok(Input::TurnEnded { agent: "w3".into(), delivery: Some(format!("{id}.p{four}")), text: Loc { file: "w.jsonl".into(), offset: 900, len: 40 }, later: vec![], stood: None, go: None });
    let p = r.m.tickets[&c].posts.last().unwrap();
    assert_eq!((p.n, p.by.as_str(), p.names.clone(), p.answers), (six + 1, "w3", vec!["w2".to_string()], Some(five)), "{:?}", &r.log[at..]);
    assert!(matches!(&r.log[at], Line::Post { t, by, .. } if *t == c && by == "w3"), "written first: {:?}", &r.log[at..]);
    assert!(!r.log[at..].iter().any(|l| matches!(l, Line::Replied { .. })), "no Replied line, which the engine before would not make: {:?}", &r.log[at..]);
    r.replayed();
}

/// A delivery program that gives what goes now and digests the rest (`deliver … digest PULL`, I-1480 rule 2).
const DIGESTS: &str = "\
/// what goes now, and the rest by first lines (I-1480's test program)
kind delivery
  form agent: Agent
  pattern
    ...
  when arrived(e) do
    keep [e.item] \"comes when its turn ends\"
  when free(e) do
    match e.push {
      [] => return ()
      now => deliver e.agent now digest e.pull
    }
  when stopped(e) do
    report [e.item] e.why
";

/// `deliver … digest PULL` (I-1480 rule 2, N-1481 p25's test): an agent idle through twenty fyis on one thread is
/// woken by none; the ask that comes next goes, with the twenty given by first lines, each once; with no ask, nothing.
#[test]
fn twenty_fyis_wake_nothing_and_go_by_first_lines_with_the_next_ask() {
    let mut r = Run::with(DIGESTS);
    let a = r.talk("w3");
    let fyis: Vec<String> = (1..=20).map(|n| typed(&mut r, a, "w3", &[], &["w2"], Some(PostKind::Fyi), &format!("step {n} done"))).collect();
    let out = r.free("w2");
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::Hand { .. })), "{out:?}");
    assert_eq!(r.waiting_ids("w2"), fyis);
    let ask = typed(&mut r, a, "w4", &[], &[], Some(PostKind::Ask { to: vec!["w2".into()] }), "check the lock");
    let out = r.free("w2");
    assert!(out.effects.contains(&Effect::Hand { to: "w2".into(), items: vec![ask.clone()], rest: vec![], digest: fyis.clone() }), "{out:?}");
    assert!(r.waiting_ids("w2").is_empty(), "being handed, none waits");
    let acted = r.log.iter().rev().find_map(|l| match l { Line::Event { name, value, .. } if name == syntax::ACTED => Some(value.clone()), _ => None }).unwrap();
    assert_eq!((acted["items"].clone(), acted["digest"].as_array().map(Vec::len)), (json!([ask]), Some(20)));
    // coordd gives the message: one line up to the ask covers the twenty too, and nothing waits again
    let upto = r.m.tickets[&a].last();
    r.given("w2", a, upto);
    assert!(r.waiting_ids("w2").is_empty() && r.m.hands("w2").is_empty());
    assert_eq!(Machine::load(defs_with(DIGESTS)).replay(&r.log), Vec::<String>::new());
    // the act prints as it parses
    let k = syntax::parse_kinds("delivery.kind", DIGESTS, &BTreeMap::new()).unwrap().remove(0);
    let free = k.handlers.iter().find(|h| h.ev == "free").unwrap();
    let printed = crate::print::prog(&free.body, 0);
    assert!(printed.contains("deliver e.agent now digest e.pull"), "{printed}");
}

/// A turn is named by the item it began with, not by a digest's post on a thread numbered lower (docket-edge5's audit,
/// N-1446 p3263: N-1104 p1160 named the turns of six agents it went to only as a digest line, their item N-1446 p3020;
/// proto-mind p3271, paper-research p3273). `deliver … digest` gives the digest's thread a `given` of its own, and what
/// a turn holds is sorted as text, so the first of it was the digest's post. The turn's `turn_began` line (coordd's:
/// the delivery given first in its session) names the item, and so now does its end line (`turn_delivery`). With no
/// `turn_began` for the turn (a coordd before it), the first, as before. A replay makes the same.
#[test]
fn a_turn_is_named_by_the_item_it_began_with_not_by_a_digests_post_on_a_thread_numbered_lower() {
    let mut r = Run::with(DIGESTS);
    // the thread numbered lower first: w3's fyi there to w2 wakes nothing; the ask on the other goes, the fyi in its digest
    let chat = r.talk("w3");
    let a = r.talk("w2");
    let fyi = typed(&mut r, chat, "w3", &[], &["w2"], Some(PostKind::Fyi), "build 92 is in");
    let ask = typed(&mut r, a, "w4", &[], &[], Some(PostKind::Ask { to: vec!["w2".into()] }), "check the lock");
    assert!(fyi < ask, "{fyi} sorts before {ask}");
    let out = r.free("w2");
    assert_eq!(hand(&out), Some((vec![ask.clone()], vec![], vec![fyi.clone()])), "{out:?}");
    // coordd gives the message: a `given` for each thread, the item's and the digest's
    let (upto_a, upto_chat) = (r.m.tickets[&a].last(), r.m.tickets[&chat].last());
    r.given("w2", a, upto_a);
    r.given("w2", chat, upto_chat);
    let given = r.m.given_since("w2");
    assert_eq!(given, vec![fyi.clone(), ask.clone()], "sorted as text: the digest's first");
    assert_eq!(r.m.turn_delivery("w2", &given), Some(fyi.clone()), "with no `turn_began`, the first, as before");
    // its turn begins with the item, and its end line is named by it
    r.event(TURN_BEGAN, json!({"agent": "w2", "delivery": ask}));
    let delivery = r.m.turn_delivery("w2", &given);
    assert_eq!(delivery, Some(ask.clone()));
    let text = Loc { file: "w.jsonl".into(), offset: 1000, len: 5 };
    let out = r.go(Input::TurnEnded { agent: "w2".into(), delivery, text, later: vec![], stood: None, go: None });
    let named: Vec<Option<String>> = out.lines.iter().filter_map(|l| match l { Line::TurnEnded { delivery, .. } => Some(delivery.clone()), _ => None }).collect();
    assert_eq!(named, vec![Some(ask.clone())], "{out:?}");
    assert_eq!(Machine::load(defs_with(DIGESTS)).replay(&r.log), Vec::<String>::new());
}

/// The message ID was handed as one, alone: the Hand effect OUT holds, if any, with its items, rest and digest.
fn hand(out: &Out) -> Option<(Vec<String>, Vec<String>, Vec<String>)> {
    out.effects.iter().find_map(|e| match e {
        Effect::Hand { items, rest, digest, .. } => Some((items.clone(), rest.clone(), digest.clone())),
        _ => None,
    })
}

impl Run {
    /// An item the owner opens for AGENT, which starts it (`coord item start`, on the record): its number.
    fn item(&mut self, agent: &str) -> u64 {
        self.ok(Input::Open { kind: "item".into(), under: None, opener: "owner".into(), form: json!({"to": agent}), text: None, names: vec![agent.into()], told: vec![] });
        let t = *self.m.tickets.range(..HIDDEN).next_back().unwrap().0;
        self.ok(Input::Started { t, by: agent.into() });
        t
    }
    /// AGENT's turn, given T up to UPTO, ends saying something, its message typing NAMED with `@` (coordd's
    /// `reply_named`): the Reply's post, by id.
    fn turn_end(&mut self, agent: &str, t: u64, upto: u32, named: &[&str]) -> String {
        self.m.reply_named = (!named.is_empty()).then(|| (agent.to_string(), named.iter().map(|a| a.to_string()).collect()));
        let id = self.m.id(t);
        let text = Loc { file: "w.jsonl".into(), offset: 1000 + 10 * u64::from(self.clock), len: 5 };
        let out = self.ok(Input::TurnEnded { agent: agent.into(), delivery: Some(format!("{id}.p{upto}")), text, later: vec![], stood: None, go: None });
        assert!(out.lines.iter().any(|l| matches!(l, Line::Replied { t: u, .. } if *u == t)), "{out:?}");
        format!("{id}.p{}", self.m.tickets[&t].last())
    }
}

/// p4's (a) under paper-research's (ii) (N-1481 p40, consult-3 p38): an `@` its writer typed always asks, in a post
/// that answers too; only the names the rule gave a post naming nobody answer and ask nothing (its line's
/// `answering`). On N-12: an ask answered and asked again in plain words; an fyi answered with an ask.
#[test]
fn an_at_its_writer_typed_asks_and_only_the_names_the_rule_gave_answer() {
    let mut r = Run::new();
    let a = r.talk("w3");
    let named = |w: &str| Some((true, Some(w.to_string())));
    let p1 = typed(&mut r, a, "w2", &["w3"], &[], None, "@w3 which branch?");
    assert_eq!(pushed(&r, "w3", &p1), named("named"));
    let upto = r.m.tickets[&a].last();
    r.given("w3", a, upto);
    let p2 = typed(&mut r, a, "w3", &[], &[], None, "main");
    let last = |r: &Run| r.m.tickets[&a].posts.last().map(|p| (p.names.clone(), p.answering.clone())).unwrap();
    let w2 = vec!["w2".to_string()];
    assert_eq!(last(&r), (w2.clone(), w2.clone()), "the rule names w2: it answers w2");
    assert_eq!(pushed(&r, "w2", &p2), named("reply"));
    let upto = r.m.tickets[&a].last();
    r.given("w2", a, upto);
    let p3 = typed(&mut r, a, "w2", &["w3"], &[], None, "@w3 thanks, now run the full set on main");
    assert_eq!(last(&r), (vec!["w3".to_string()], vec![]), "w3 typed");
    assert_eq!(pushed(&r, "w3", &p3), named("named"), "it answers w3's post and asks it again");
    // the line says it: `answering` only where the rule named
    let lines: Vec<Vec<String>> = r.log.iter().filter_map(|l| match l { Line::Post { t, answering, .. } if *t == a => Some(answering.clone()), _ => None }).collect();
    assert_eq!(lines, [vec![], w2.clone(), vec![]]);
    // an fyi of w3's, answered with an ask in plain words
    let fyi = typed(&mut r, a, "w3", &["w2"], &[], Some(PostKind::Fyi), "fyi @w2 the lock passed");
    assert_eq!(pushed(&r, "w2", &fyi), Some((false, None)));
    let upto = r.m.tickets[&a].last();
    r.given("w2", a, upto);
    let cut = typed(&mut r, a, "w2", &["w3"], &[], None, "@w3 good, now cut it");
    assert_eq!(pushed(&r, "w3", &cut), named("named"));
    r.replayed();
}

/// The turn's end's Reply (proto-reader N-1481 p42, paper-research p43): the names it gives answer and ask nothing,
/// the `@`s its message typed ask, and a name given both ways asks; the Replied line says which (`answering`). A
/// record whose Replied lines have none, as 61 wrote them, replays as it is.
#[test]
fn a_turns_reply_asks_whom_its_message_typed_and_answers_whom_the_rule_named() {
    let mut r = Run::new();
    let a = r.talk("w3");
    let replied = |r: &Run| r.log.iter().rev().find_map(|l| match l { Line::Replied { names, answering, .. } => Some((names.clone(), answering.clone())), _ => None }).unwrap();
    let (w2, w3) = (vec!["w2".to_string()], vec!["w3".to_string()]);
    let _ = typed(&mut r, a, "w2", &["w3"], &[], None, "@w3 which branch?");
    let upto = r.m.tickets[&a].last();
    r.given("w3", a, upto);
    let main = r.turn_end("w3", a, upto, &[]);
    assert_eq!(replied(&r), (w2.clone(), w2.clone()));
    assert_eq!(pushed(&r, "w2", &main), Some((true, Some("reply".into()))));
    let upto = r.m.tickets[&a].last();
    r.given("w2", a, upto);
    let thanks = r.turn_end("w2", a, upto, &[]);
    assert_eq!(replied(&r), (w3.clone(), w3.clone()));
    assert_eq!(pushed(&r, "w3", &thanks), Some((false, None)), "\"thanks\" asks nothing: no ping-pong");
    // on another thread, w2's turn ends "thanks, @w3 now run the full set": w3 by the rule and typed, so asked
    let b = r.talk("w3");
    let _ = typed(&mut r, b, "w2", &["w3"], &[], None, "@w3 which runner?");
    let upto = r.m.tickets[&b].last();
    r.given("w3", b, upto);
    let _ = r.turn_end("w3", b, upto, &[]);
    let upto = r.m.tickets[&b].last();
    r.given("w2", b, upto);
    let again = r.turn_end("w2", b, upto, &["w3"]);
    assert_eq!(replied(&r), (w3.clone(), vec![]));
    assert_eq!(pushed(&r, "w3", &again), Some((true, Some("named".into()))));
    r.replayed();
    // a record from before `answering`: no line has it, and it replays with no problem
    assert!(r.log.iter().any(|l| matches!(l, Line::Replied { answering, .. } if !answering.is_empty())));
    let old: Vec<Line> = r.log.iter().map(|l| {
        let mut v = serde_json::to_value(l).unwrap();
        v.as_object_mut().unwrap().remove("answering");
        serde_json::from_value(v).unwrap()
    }).collect();
    assert!(old.iter().all(|l| !serde_json::to_string(l).unwrap().contains("\"answering\"")));
    assert_eq!(Machine::load(defs_with(DELIVERY_KIND)).replay(&old), Vec::<String>::new());
}

/// A moved post is the same post on its new ticket: what it is (`kind`) and whom it answers (`answering`) go with it
/// (proto-reader N-1481 p34, p39).
#[test]
fn a_moved_post_keeps_what_it_is_and_whom_it_answers() {
    let mut r = Run::new();
    let a = r.talk("w3");
    let b = r.talk("w3");
    let _ = typed(&mut r, a, "w2", &["w3"], &[], None, "@w3 which branch?");
    let upto = r.m.tickets[&a].last();
    r.given("w3", a, upto);
    let main = typed(&mut r, a, "w3", &[], &[], None, "main");
    let ask = typed(&mut r, a, "w3", &[], &[], Some(PostKind::Ask { to: vec!["w2".into()] }), "and the release?");
    for id in [&main, &ask] {
        r.ok(Input::Move { t: a, n: post_id(id).unwrap().1, to: Some(b), by: "owner".into() });
    }
    let moved: Vec<(Option<PostKind>, Vec<String>)> = r.m.tickets[&b].posts.iter().filter(|p| p.from.is_some()).map(|p| (p.kind.clone(), p.answering.clone())).collect();
    assert_eq!(moved, [(None, vec!["w2".to_string()]), (Some(PostKind::Ask { to: vec!["w2".into()] }), vec![])]);
    r.replayed();
}

/// As DIGESTS, with a step before its act: what it digests is read before the owner's hold goes on.
const DIGESTS_LATE: &str = "\
/// what goes now, and the rest by first lines, after a step (I-1480's test program)
kind delivery
  form agent: Agent
  pattern
    ...
  when arrived(e) do
    keep [e.item] \"comes when its turn ends\"
  when free(e) do
    _ <- step watcher next(e.doing, e.waiting);
    match e.push {
      [] => return ()
      now => deliver e.agent now digest e.pull
    }
  when stopped(e) do
    report [e.item] e.why
";

/// proto-reader N-1481 p39, 2: a post the hold keeps, named in the digest and before a handed post on its thread, is
/// kept with the hold's words, as when it is brought along, so the `given` passing it loses nothing.
#[test]
fn a_held_post_in_the_digest_before_his_on_its_thread_is_kept() {
    let mut r = Run::with(DIGESTS_LATE);
    let a = r.talk("w1");
    let x = typed(&mut r, a, "x", &[], &["w1"], Some(PostKind::Fyi), "the build is green");
    let o = r.post(a, "owner", "w1", "@w1 stop and look at the median");
    let o = format!("{}.p{o}", r.m.id(a));
    assert_eq!((pushed(&r, "w1", &x), pushed(&r, "w1", &o)), (Some((false, None)), Some((true, Some("owner".into())))));
    r.free("w1");
    let c = r.next_run("w1");
    r.hold(true);
    let out = r.answer_next(c, json!({"head": "Rest", "why": "read before the hold"}));
    assert_eq!(hand(&out), Some((vec![o.clone()], vec![], vec![])), "{out:?}");
    assert!(r.m.kept("w1").get(&x).is_some_and(|why| why.starts_with("the owner holds agents' mail since")), "{:?}", r.m.kept("w1"));
    let mut m = Machine::load(defs_with(DIGESTS_LATE));
    assert_eq!(m.replay(&r.log), Vec::<String>::new());
    assert_eq!(m.kept("w1"), r.m.kept("w1"));
}

/// His post that only informs the agent, and a program's post into its turn (the brain's), are never cut to a first
/// line (N-1481 p44): with the message that goes, whole and first, as `next` gave them; with nothing that goes they
/// wait, waking nothing. An agent's fyi beside them goes in the digest.
#[test]
fn his_post_and_the_brains_in_a_digest_go_whole_with_the_message() {
    let mut r = Run::with(DIGESTS);
    let a = r.talk("w3");
    let b = r.talk("lead");
    r.ok(Input::Post { t: b, by: "owner".into(), text: Text::New("when does it reach prod".into()), names: vec!["lead".into()], told: vec!["lead".into()], inform: vec!["w2".into()], answers: None, kind: None });
    let n = r.m.tickets[&b].last();
    r.ok(Input::Notified { t: b, event: "post".into(), to: vec![], inform: vec!["w2".into()], post: Some(n), provenance: json!({}) });
    let his = format!("{}.p{n}", r.m.id(b));
    r.ok(Input::Open { kind: "note_to".into(), under: None, opener: "owner".into(), form: json!({"w": "w2"}), text: None, names: vec!["lead".into()], told: vec![] });
    let n = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    let note = r.m.tickets[&n].posts.iter().find(|p| p.head.is_none()).map(|p| format!("{}.p{}", r.m.id(n), p.n)).unwrap();
    let fyi = typed(&mut r, a, "w4", &[], &["w2"], Some(PostKind::Fyi), "the bench ran");
    for i in [&his, &note, &fyi] {
        assert_eq!(pushed(&r, "w2", i), Some((false, None)), "{i}");
    }
    assert_eq!(hand(&r.free("w2")), None, "nothing goes: they wait");
    let ask = typed(&mut r, a, "w3", &[], &[], Some(PostKind::Ask { to: vec!["w2".into()] }), "check the lock");
    let out = r.free("w2");
    let (items, rest, digest) = hand(&out).unwrap_or_else(|| panic!("{out:?}"));
    assert_eq!((items.len(), items.last(), rest, digest), (3, Some(&ask), vec![], vec![fyi]), "{items:?}");
    assert!(items.contains(&his) && items.contains(&note), "{items:?}");
    assert_eq!(Machine::load(defs_with(DIGESTS)).replay(&r.log), Vec::<String>::new());
}

/// The brain's lines on agent-base's text (76; fresh-lead-3 N-1540 p18): `free`'s `told` holds a program's post into
/// the agent's turn (`post … informing`, as the brain's) that does not go now, apart from `push`, so `fold` never
/// orders it. Alone it goes at once, whole; beside what goes now to an idle agent, after it; beside his posts, after
/// them, the rest named at the foot (gemini-advisor N-1540 p23's text). A replay reads the same.
#[test]
fn the_brains_line_is_frees_told_and_goes_whole_alone_or_after_what_goes_and_his() {
    let mut r = Run::new();
    let note = |r: &mut Run| -> (u64, u32, String) {
        r.ok(Input::Open { kind: "note_to".into(), under: None, opener: "owner".into(), form: json!({"w": "w2"}), text: None, names: vec!["lead".into()], told: vec![] });
        let t = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
        let n = r.m.tickets[&t].posts.iter().find(|p| p.head.is_none()).unwrap().n;
        (t, n, format!("{}.p{n}", r.m.id(t)))
    };
    let (t1, p1, n1) = note(&mut r);
    let item = r.m.waiting("w2").into_iter().find(|i| i["id"] == json!(n1)).unwrap();
    assert_eq!((item["push"].clone(), item["into_turn"].clone()), (json!(false), json!(true)));
    // alone: at once, whole
    assert_eq!(hand(&r.free("w2")), Some((vec![n1.clone()], vec![], vec![])));
    r.given("w2", t1, p1);
    // beside an ask to idle w2: after it
    let a = r.talk("w3");
    let ask = typed(&mut r, a, "w3", &[], &[], Some(PostKind::Ask { to: vec!["w2".into()] }), "check the lock");
    let (t2, p2, n2) = note(&mut r);
    assert_eq!(hand(&r.free("w2")), Some((vec![ask.clone(), n2.clone()], vec![], vec![])));
    r.given("w2", t2, p2);
    r.given("w2", a, r.m.tickets[&a].last());
    // beside his: after his, the ask at the foot
    let b = r.talk("w3");
    let more = typed(&mut r, b, "w3", &[], &[], Some(PostKind::Ask { to: vec!["w2".into()] }), "and the replay");
    let c = r.talk("w2");
    let his = r.post(c, "owner", "w2", "@w2 stop and look at the median");
    let his = format!("{}.p{his}", r.m.id(c));
    let (_, _, n3) = note(&mut r);
    assert_eq!(hand(&r.free("w2")), Some((vec![his, n3], vec![more], vec![])));
    r.replayed();
}


/// I-1480's test 2 (N-1481 p1), with p4's untyped @: an ask to an idle agent goes at once, and once; so does an
/// untyped @w2. agent-base's delivery.kind: no `next`, no call.
#[test]
fn an_ask_to_an_idle_agent_goes_at_once_and_once() {
    let mut r = Run::new();
    let a = r.talk("w3");
    let ask = typed(&mut r, a, "w3", &[], &[], Some(PostKind::Ask { to: vec!["w2".into()] }), "check the lock");
    let out = r.free("w2");
    assert_eq!(hand(&out), Some((vec![ask.clone()], vec![], vec![])), "{out:?}");
    assert!(!r.m.tickets.range(DESK..).any(|(_, x)| matches!(&x.status, Status::Awaits { agent, .. } if agent == WATCHER)), "no `next` is asked");
    // raised again before coordd gives it: nothing more is handed
    assert_eq!(hand(&r.free("w2")), None);
    let upto = r.m.tickets[&a].last();
    r.given("w2", a, upto);
    assert_eq!(hand(&r.free("w2")), None);
    // p4: an untyped @w2 is an ask to it
    let at = typed(&mut r, a, "w3", &["w2"], &[], None, "@w2 and the runner");
    assert_eq!(hand(&r.free("w2")), Some((vec![at], vec![], vec![])));
    r.replayed();
}

/// I-1480's test 3: a hold on the item a working agent started reaches it at its turn's end, not in the turn; so
/// does an ask, and the lead's post naming it no longer goes into its turn ("only the turn end", N-1481 p15).
#[test]
fn a_hold_on_a_working_agents_item_and_its_leads_ask_wait_for_its_turns_end() {
    let mut r = Run::new();
    let a = r.talk("w3");
    let item = r.item("w2");
    let on = r.m.id(item);
    let hold = typed(&mut r, a, "w3", &[], &[], Some(PostKind::Hold { on }), "hold the merge: the lock fails");
    r.event("arrived", json!({"agent": "w2", "item": hold.clone()}));
    let lead = typed(&mut r, a, "lead", &["w2"], &[], None, "@w2 look at the lock first");
    let out = r.event("arrived", json!({"agent": "w2", "item": lead.clone(), "above": ["lead"]}));
    assert_eq!(hand(&out), None, "{out:?}");
    assert_eq!(r.m.kept("w2"), BTreeMap::from([(hold.clone(), "comes when its turn ends".into()), (lead.clone(), "comes when its turn ends".into())]));
    // its turn ends: both go, as one message
    let out = r.free("w2");
    assert_eq!(hand(&out), Some((vec![hold, lead], vec![], vec![])), "{out:?}");
    r.replayed();
}

/// I-1480's test 4: fyis on an idle agent's own item wake nothing and come in its digest, by first lines, with the
/// next thing that goes, each listed once.
#[test]
fn fyis_on_an_idle_agents_own_item_wake_nothing_and_come_in_its_digest() {
    let mut r = Run::new();
    let a = r.talk("w3");
    let item = r.item("w2");
    let fyis: Vec<String> = ["the bench ran", "the numbers are in", "61 is cut"].iter().map(|t| typed(&mut r, item, "w4", &[], &["w2"], Some(PostKind::Fyi), t)).collect();
    let arrived = r.event("arrived", json!({"agent": "w2", "item": fyis[0].clone()}));
    assert_eq!(hand(&arrived), None);
    assert_eq!(hand(&r.free("w2")), None, "an fyi starts no turn");
    assert!(r.m.waiting("w2").iter().all(|i| i["own"] == json!(true) && i["push"] == json!(false)));
    let ask = typed(&mut r, a, "w3", &[], &[], Some(PostKind::Ask { to: vec!["w2".into()] }), "check the lock");
    let out = r.free("w2");
    assert_eq!(hand(&out), Some((vec![ask], vec![], fyis.clone())), "{out:?}");
    let last = post_id(fyis.last().unwrap()).unwrap().1;
    r.given("w2", item, last);
    let upto = r.m.tickets[&a].last();
    r.given("w2", a, upto);
    assert!(r.waiting_ids("w2").is_empty(), "each fyi is listed once");
    r.replayed();
}

/// I-1480 rule 3, test 5: his posts go as before, at once and alone, the rest named at the foot, as the program before
/// it gives them (`NEXT_KIND`); and into a running turn, on the text before p716 (`INTO_KIND`). Only a digest of what
/// is pulled is new beside them. On agent-base's text his post to a working agent waits for the turn's end, as every
/// post that goes to it now does (the owner, N-1104 p716: "only the turn end"; his yes p923); to an idle one it goes as
/// before.
#[test]
fn his_posts_go_as_the_program_before_gave_them() {
    let run = |kind: &str| {
        let mut r = Run::with(kind);
        let a = r.talk("w2");
        let b = r.talk("w2");
        // on another thread: one before his on his would come with it, as the thread reads in order
        let ask = typed(&mut r, b, "w3", &[], &[], Some(PostKind::Ask { to: vec!["w2".into()] }), "check the lock");
        let his = r.post(a, "owner", "w2", "@w2 stop and look at the median");
        let his = format!("{}.p{his}", r.m.id(a));
        let idle = hand(&r.free("w2"));
        let upto = r.m.tickets[&a].last();
        r.given("w2", a, upto);
        let more = r.post(a, "owner", "w2", "@w2 and the tests");
        let more = format!("{}.p{more}", r.m.id(a));
        let working = hand(&r.event("arrived", json!({"agent": "w2", "item": more.clone()})));
        let kept = r.m.kept("w2").get(&more).cloned();
        ((idle, working), (ask, his, more), kept)
    };
    let ((idle, working), (ask, his, more), kept) = run(INTO_KIND);
    assert_eq!(idle, Some((vec![his.clone()], vec![ask.clone()], vec![])), "his alone, the ask named at the foot");
    assert_eq!((working.clone(), kept), (Some((vec![more.clone()], vec![], vec![])), None), "his into the running turn");
    assert_eq!(run(NEXT_KIND).0, (idle.clone(), working), "as before");
    let ((idle_now, working_now), _, kept_now) = run(DELIVERY_KIND);
    assert_eq!(idle_now, idle, "to an idle agent as before");
    assert_eq!((working_now, kept_now.as_deref()), (None, Some("comes when its turn ends")), "kept for the turn's end");
}

/// His hold on agents' mail (line 327) on agent-base's text, as `the_hold_keeps_an_agents_post_his_goes_and_release_
/// lets_what_it_kept_go` shows it on the text before (proto-reader N-1481 p102): under it, an agent's ask and an fyi
/// to idle w2 are kept with the hold's words (`free`'s `keep e.held why`) while his post goes, alone, with nothing held
/// at its foot; in the turn it starts, an ask's `arrived` keeps it with the hold's words, not for the turn's end. A
/// replay rebuilds the hold and the keeps. His Release lets the three go: the next `free` gives both asks as one
/// message, the fyi as its digest.
#[test]
fn under_his_hold_an_ask_and_an_fyi_are_kept_his_post_goes_and_release_gives_the_asks_with_the_fyi_as_digest() {
    let mut r = Run::new();
    let a = r.talk("w2");
    let b = r.talk("w2");
    r.hold(true);
    let why = String::from("the owner holds agents' mail since 09:00 UTC");
    let ask = typed(&mut r, a, "w3", &[], &[], Some(PostKind::Ask { to: vec!["w2".into()] }), "check the lock");
    let fyi = typed(&mut r, a, "w4", &[], &["w2"], Some(PostKind::Fyi), "the bench ran");
    let his = r.post(b, "owner", "w2", "@w2 stop and look at the median");
    let his = format!("{}.p{his}", r.m.id(b));
    let out = r.free("w2");
    assert_eq!(hand(&out), Some((vec![his.clone()], vec![], vec![])), "his alone, nothing held at its foot: {out:?}");
    assert_eq!(r.m.kept("w2"), BTreeMap::from([(ask.clone(), why.clone()), (fyi.clone(), why.clone())]));
    assert_eq!(r.acted(), vec![("w2".into(), "keep".into(), vec![ask.clone(), fyi.clone()], vec![]), ("w2".into(), "deliver".into(), vec![his.clone()], vec![])]);
    let upto = r.m.tickets[&b].last();
    r.given("w2", b, upto);
    // in the turn his post started: an ask's arrived keeps it with the hold's words
    let at = typed(&mut r, a, "w3", &["w2"], &[], None, "@w2 and the runner");
    let out = r.event("arrived", json!({"agent": "w2", "item": at.clone()}));
    assert_eq!(hand(&out), None, "{out:?}");
    assert_eq!(r.m.kept("w2").get(&at), Some(&why));
    let m = r.replayed();
    assert_eq!((m.kept("w2"), m.mail_hold.is_some()), (r.m.kept("w2"), true));
    // Release: kept no more; its turn has ended, and the next free gives the asks, the fyi by its first line
    let said = r.hold(false);
    assert!(said.ends_with("The 3 things it kept are decided again."), "{said}");
    assert!(r.m.kept("w2").is_empty(), "{:?}", r.m.kept("w2"));
    let out = r.free("w2");
    assert_eq!(hand(&out), Some((vec![ask, at], vec![], vec![fyi])), "{out:?}");
    let upto = r.m.tickets[&a].last();
    r.given("w2", a, upto);
    assert!(r.waiting_ids("w2").is_empty(), "{:?}", r.waiting_ids("w2"));
    let m = r.replayed();
    assert_eq!((m.kept("w2").len(), m.mail_hold.is_none()), (0, true));
}

/// The act keeps a message to one step (I-1480), as coordd's answer to `next` did on the text before (clerk.rs's
/// `next_ok`, gone with c's coordd half): two steps due at once go one per message, the second named first at the
/// foot. Only an act with a digest does it.
#[test]
fn a_message_holds_one_step_and_names_the_next_at_its_foot() {
    let mut r = Run::with(DIGESTS);
    let a = r.talk("w2");
    let ids = |r: &Run| r.m.waiting("w2").into_iter().filter(|i| i["what"] == json!("step")).map(|i| i["id"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    // two steps to w2, due at once, would be two answers in one turn
    for _ in 0..2 {
        r.ok(Input::Open { kind: "asks".into(), under: Some(a), opener: "w3".into(), form: json!({"to": "w2"}), text: None, names: vec![], told: vec![] });
    }
    let steps = ids(&r);
    assert_eq!(steps.len(), 2, "{:?}", r.m.waiting("w2"));
    let out = r.free("w2");
    assert_eq!(hand(&out), Some((vec![steps[0].clone()], vec![steps[1].clone()], vec![])), "{out:?}");
    assert_eq!(Machine::load(defs_with(DIGESTS)).replay(&r.log), Vec::<String>::new());
}

/// A delivery whose `free` fails: its run closes Failed, for a closed run's form (paper-research N-1446 p1960).
const FAILS: &str = "\
/// one agent's delivery whose free fails, for the delivery tests
kind delivery
  form agent: Agent
  raises Broken
  pattern
    ...
  when arrived(e) do
    keep [e.item] \"comes when its turn ends\"
  when free(e) do
    raise Broken(e.doing)
  when stopped(e) do
    report [e.item] e.why
";

/// A closed run keeps what it was given as text (`form_text`; paper-research N-1446 p1960), and is read as it was
/// given: its view (`coord show` of it), and a repair the record holds, which runs it again on that form as values. A
/// repair live is refused, as for any run: its kind is its event's, with no program of its own (`kind_of`).
#[test]
fn a_closed_run_keeps_its_form_as_text_and_runs_again_on_it_as_given() {
    let mut r = Run::with(FAILS);
    let a = r.talk("w1");
    r.post(a, "x", "w1", "@w1 the build is green");
    r.free("w1");
    let c = *r.m.tickets.range(DESK..).filter(|(_, x)| x.is_run()).next_back().unwrap().0;
    let x = &r.m.tickets[&c];
    assert!(matches!(&x.status, Status::Failed(e) if e.exc == "Broken"), "{:?}", x.status);
    assert_eq!(x.form, Value::Null, "closed, its form is its text");
    let given: Value = serde_json::from_str(x.form_text.as_deref().unwrap()).unwrap();
    assert_eq!((&given["agent"], &given["doing"]), (&json!("w1"), &json!("N-1 · the median")));
    assert_eq!(x.form_of().into_owned(), given);
    // its view is the one its form as given makes
    let mut y = x.clone();
    (y.form, y.form_text) = (given.clone(), None);
    assert_eq!(r.m.view(c), Some(TicketView::of(&r.m, &y)));
    assert_eq!(r.m.view(c).unwrap().form["doing"], json!("N-1 · the median"));
    // repaired live: refused, and still kept as text
    let out = r.go(Input::Repair { t: c, by: "dev".into(), program: "return doing".into(), why: "its form".into() });
    assert!(out.reply.is_err() && out.lines.is_empty(), "{out:?}");
    assert!(r.m.tickets[&c].form_text.is_some());
    assert_eq!(Machine::load(defs_with(FAILS)).replay(&r.log), Vec::<String>::new());
    // a record's repair runs it again on its form, read back from its text
    let mut log = r.log.clone();
    log.push(Line::Edited { at: "2026-11-20T10:00:00Z".into(), t: c, by: "dev".into(), why: "its form".into(), what: "repair".into(), program: Some("return doing".into()), kind: None, form: None, calls: BTreeMap::new() });
    let mut m = Machine::load(defs_with(FAILS));
    m.replay(&log);
    let x = &m.tickets[&c];
    assert_eq!(x.status, Status::Done(json!("N-1 · the median")), "repaired, it runs on what it was given");
    assert_eq!((x.form_text.is_some(), &x.form), (true, &Value::Null), "closed again, its form is its text again");
}

/// What `fold` reads, as a program reads it (paper-research N-1446 p2092, p2097; proto-mind N-1511 p20, p28): a test
/// program that keeps a post reaching its agent only as one on its thread by a reason of its own, and at `free` gives
/// what goes now at once when it came while the agent was idle, else keeps it by its work's place, or by how far its
/// session ran on since.
const FACTS: &str = "\
/// what fold reads, for the delivery tests
kind delivery
  form agent: Agent
  pattern
    ...
  when arrived(e) do
    match e.item {
      i where i.member => keep [i] \"on a thread it is on\"
      i => keep [i] \"comes when its turn ends\"
    }
  when free(e) do
    match e.push {
      [] => return ()
      now where e.worked == false => deliver e.agent now digest e.pull
      now where e.since == \"-\" => keep now e.place
      now => keep now e.since
    }
  when stopped(e) do
    report [e.item] e.why
";

/// Why item ID waits for AGENT, kept; None: it is not kept, or does not wait.
fn kept_as(r: &Run, agent: &str, id: &str) -> Option<String> {
    r.m.waiting(agent).into_iter().find(|i| i["id"] == json!(id)).and_then(|i| i["kept"].as_str().map(str::to_string))
}

/// The member fact (paper-research N-1446 p2092, p2097; N-1511 p20): on N-1446's shape, the lock talk among others
/// reaches a reader only as one on the thread (policy's inform, notify.eflint's thread line), and its item says so;
/// the head's posts to it, his post its route informs it of, and a program's post into its turn do not. A program
/// keeps the first by a reason of its own. No line kind is new: a replay reads the same.
#[test]
fn a_post_that_reaches_an_agent_only_as_one_on_its_thread_is_a_member_item_and_the_heads_posts_are_not() {
    let mut r = Run::with(FACTS);
    let a = r.talk("w3");
    // the lock talk: w4 and w5 by name, w2 and w3 informed as on the thread
    let lock = typed(&mut r, a, "w4", &["w5"], &["w2", "w3"], None, "the lock is yours, w5");
    let freed = typed(&mut r, a, "w5", &[], &["w2", "w3", "w4"], Some(PostKind::Fyi), "the lock is free");
    // the head's posts: a read for w2, and its line by @
    let read = typed(&mut r, a, "w3", &[], &["w4", "w5"], Some(PostKind::Read { to: vec!["w2".into()], head: "core-x 1234abcd".into() }), "read core-x");
    let line = typed(&mut r, a, "w3", &["w2"], &["w4", "w5"], None, "@w2 its line is green");
    // his post told to w3, informing w2 by its route's others (`Line::Told`): the route's word, not membership
    r.ok(Input::Post { t: a, by: "owner".into(), text: Text::New("when is it in".into()), names: vec!["w3".into()], told: vec!["w3".into()], inform: vec!["w2".into()], answers: None, kind: None });
    let n = r.m.tickets[&a].last();
    r.ok(Input::Notified { t: a, event: "post".into(), to: vec![], inform: vec!["w2".into()], post: Some(n), provenance: json!({}) });
    let his = format!("{}.p{n}", r.m.id(a));
    // a program's post into w2's turn (`post … informing`)
    r.ok(Input::Open { kind: "note_to".into(), under: None, opener: "owner".into(), form: json!({"w": "w2"}), text: None, names: vec!["lead".into()], told: vec![] });
    let n = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    let note = r.m.tickets[&n].posts.iter().find(|p| p.head.is_none()).map(|p| format!("{}.p{}", r.m.id(n), p.n)).unwrap();
    let member = |r: &Run, id: &str| r.m.waiting("w2").into_iter().find(|i| i["id"] == json!(id)).map(|i| i["member"] == json!(true));
    assert_eq!((member(&r, &lock), member(&r, &freed)), (Some(true), Some(true)), "the lock talk reaches w2 only as one on the thread");
    assert_eq!((member(&r, &read), member(&r, &line), member(&r, &his), member(&r, &note)), (Some(false), Some(false), Some(false), Some(false)));
    // w5, whom the lock talk names: its own, not a member's
    assert_eq!(r.m.waiting("w5").into_iter().find(|i| i["id"] == json!(lock)).map(|i| i["member"] == json!(true)), Some(false));
    // w2 at work: the program keeps what arrives by the fact
    for i in [&lock, &read] {
        r.event("arrived", json!({"agent": "w2", "item": i}));
    }
    assert_eq!(kept_as(&r, "w2", &lock).as_deref(), Some("on a thread it is on"));
    assert_eq!(kept_as(&r, "w2", &read).as_deref(), Some("comes when its turn ends"));
    assert_eq!(Machine::load(defs_with(FACTS)).replay(&r.log), Vec::<String>::new());
}

/// `free`'s facts for `fold` (paper-research N-1446 p2092, p2097): whether what goes now came while the agent worked,
/// how far its session ran on since its work was given, and that work's place, as coordd says them, read by the
/// program and recorded with the event, so a replay reads the same. A `free` without them, from a record or a coordd
/// before them, reads as an idle agent's.
#[test]
fn free_says_whether_what_goes_came_while_it_worked_where_its_work_was_given_and_how_far_it_ran_since() {
    let mut r = Run::with(FACTS);
    let a = r.talk("w3");
    let free = |r: &mut Run, worked: bool, since: &str| r.event("free", json!({"agent": "w2", "doing": "N-1 · the median", "worked": worked, "since": since, "place": "N-1.p4"}));
    let ask = typed(&mut r, a, "w3", &[], &[], Some(PostKind::Ask { to: vec!["w2".into()] }), "check the lock");
    // idle as it came: it goes at once, with no call
    let out = free(&mut r, false, "-");
    assert_eq!(hand(&out), Some((vec![ask.clone()], vec![], vec![])), "{out:?}");
    let upto = r.m.tickets[&a].last();
    r.given("w2", a, upto);
    // came while it worked: kept by where its work was given, or by how far its session ran on since
    let next = typed(&mut r, a, "w3", &[], &[], Some(PostKind::Ask { to: vec!["w2".into()] }), "and the replay");
    let out = free(&mut r, true, "-");
    assert_eq!(hand(&out), None, "{out:?}");
    assert_eq!(kept_as(&r, "w2", &next).as_deref(), Some("N-1.p4"));
    let since = "212 KB of its session (about 54k tokens)";
    let late = typed(&mut r, a, "w3", &[], &[], Some(PostKind::Ask { to: vec!["w2".into()] }), "and the saver");
    free(&mut r, true, since);
    assert_eq!(kept_as(&r, "w2", &late).as_deref(), Some(since));
    let acted: Vec<Value> = r.log.iter().filter_map(|l| match l { Line::Event { name, value, .. } if name == "free" => Some(value.clone()), _ => None }).collect();
    assert_eq!(acted.last().map(|v| (v["worked"].clone(), v["since"].clone(), v["place"].clone())), Some((json!(true), json!(since), json!("N-1.p4"))), "recorded as coordd said them");
    // a free without them: what goes now goes, as an idle agent's
    let out = r.free("w2");
    let (items, ..) = hand(&out).unwrap_or_else(|| panic!("{out:?}"));
    assert!(items.contains(&late) && items.contains(&next), "{items:?}");
    assert_eq!(Machine::load(defs_with(FACTS)).replay(&r.log), Vec::<String>::new());
}

const FORK: &str = "\
/// fold's Fork as the act, for the delivery tests
kind delivery
  form agent: Agent
  pattern
    ...
  when arrived(e) do
    keep [e.item] \"comes when its turn ends\"
  when free(e) do
    match e.push {
      [] => return ()
      now where e.worked == false => deliver e.agent now digest e.pull
      now where e.forkable => fork e.agent at e.place with now digest e.pull
      now => deliver e.agent now digest e.pull
    }
  when stopped(e) do
    report [e.item] e.why
";

/// `fork A at PLACE with ITEMS` (part 4, docket-core N-1446 p2096; paper-research p2092, p2097): the message `deliver`
/// would make, its line saying `fork` with `at`, its hand carrying PLACE for coordd to cut the agent's session after
/// PLACE's given line. A PLACE `free` names none of ("-") makes it a `deliver`, said on its line. It prints as it is
/// written, a desk from before it reads back, and a replay reads the same. Where `free` says it cannot fork
/// (`forkable` false, as coordd reads it; gemini-advisor N-1446 p2400, A4), the program delivers, and the act's line
/// is a plain `deliver`.
#[test]
fn a_fork_hands_what_deliver_would_with_the_place_its_session_goes_on_from() {
    let p = crate::syntax::parse_prog("fork e.agent at e.place with now listing rest digest e.pull", 1).unwrap();
    assert_eq!(crate::print::prog(&p, 0), "fork e.agent at e.place with now listing rest digest e.pull");
    assert_eq!(crate::print::node(&p).0, "fork");
    assert_eq!(crate::print::prog(&crate::syntax::parse_prog("deliver e.agent now digest e.pull", 1).unwrap(), 0), "deliver e.agent now digest e.pull");
    assert!(crate::syntax::parse_prog("fork e.agent e.place with now", 1).is_err(), "`at` names the place");
    let mut r = Run::with(FORK);
    let a = r.talk("w3");
    let free_as = |r: &mut Run, place: &str, forkable: bool| r.event("free", json!({"agent": "w2", "doing": "N-1 · the median", "worked": true, "since": "212 KB of its session (about 54k tokens)", "place": place, "forkable": forkable}));
    let free = |r: &mut Run, place: &str| free_as(r, place, true);
    let ask = typed(&mut r, a, "w3", &[], &[], Some(PostKind::Ask { to: vec!["w2".into()] }), "check the lock");
    let out = free(&mut r, "N-1.p4");
    assert_eq!(hand(&out), Some((vec![ask.clone()], vec![], vec![])), "the message deliver would make: {out:?}");
    let acted = |r: &Run| r.log.iter().rev().find_map(|l| match l { Line::Event { name, value, .. } if name == syntax::ACTED => Some(value.clone()), _ => None }).unwrap();
    let line = acted(&r);
    assert_eq!((line["act"].clone(), line["at"].clone(), line["items"].clone()), (json!("fork"), json!("N-1.p4"), json!([ask.clone()])), "{line}");
    assert_eq!(r.m.hands("w2").iter().map(|h| h.fork.clone()).collect::<Vec<_>>(), vec![Some("N-1.p4".to_string())], "coordd reads the place from the hand");
    let upto = r.m.tickets[&a].last();
    r.given("w2", a, upto);
    assert!(r.m.hands("w2").is_empty(), "given, as a deliver's is");
    // no work given: a deliver, said so
    let next = typed(&mut r, a, "w3", &[], &[], Some(PostKind::Ask { to: vec!["w2".into()] }), "and the replay");
    let out = free(&mut r, "-");
    assert_eq!(hand(&out), Some((vec![next.clone()], vec![], vec![])), "{out:?}");
    let line = acted(&r);
    assert_eq!((line["act"].clone(), line.get("at").cloned(), line["why"].clone()), (json!("deliver"), None, json!("fork with no place: delivered")), "{line}");
    assert_eq!(r.m.hands("w2").iter().map(|h| h.fork.clone()).collect::<Vec<_>>(), vec![None]);
    // it cannot fork at its place: the program delivers, a plain `deliver`
    let upto = r.m.tickets[&a].last();
    r.given("w2", a, upto);
    let last = typed(&mut r, a, "w3", &[], &[], Some(PostKind::Ask { to: vec!["w2".into()] }), "and the saver");
    let out = free_as(&mut r, "N-1.p4", false);
    assert_eq!(hand(&out), Some((vec![last.clone()], vec![], vec![])), "{out:?}");
    let line = acted(&r);
    assert_eq!((line["act"].clone(), line.get("at").cloned(), line.get("why").cloned()), (json!("deliver"), None, None), "{line}");
    assert_eq!(r.m.hands("w2").iter().map(|h| h.fork.clone()).collect::<Vec<_>>(), vec![None]);
    // a hand as desks wrote it before: no `fork`, and none written for a deliver's
    let old: crate::machine::Hand = serde_json::from_value(json!({"items": [ask], "rest": [], "digest": []})).unwrap();
    assert_eq!(old.fork, None);
    assert!(serde_json::to_value(&r.m.hands("w2")[0]).unwrap().get("fork").is_none());
    assert_eq!(Machine::load(defs_with(FORK)).replay(&r.log), Vec::<String>::new());
}

/// His post routed to another reaches one on its thread by its first line from the era [`ERA_QUIET`] (paper-research
/// N-1446 p2367, 1; gemini-advisor's audit p2365, case 2: N-1104.p976, his post to paper-research, went whole and
/// first in docket-core's brief N-1446.p2331, and 14 more so): on his chat w2 took part, so his post to the lead
/// informs w2 (`others_on`); the lead's ask to w2 elsewhere goes now, and his post goes with it in the digest, not
/// whole. Before the era it went whole and first (coordd's `his_post_that_only_informs_it_holds_nothing_back`).
#[test]
fn from_the_era_quiet_his_post_routed_to_another_goes_to_one_on_its_thread_by_its_first_line() {
    let mut r = Run::new();
    r.event(ERA, json!({"era": ERA_QUIET}));
    let chat = r.talk("lead");
    let joined = typed(&mut r, chat, "w2", &["lead"], &[], None, "@lead I am on it");
    let n = r.post(chat, "owner", "lead", "@lead the build notes, please");
    let his = format!("{}.p{n}", r.m.id(chat));
    assert_eq!(pushed(&r, "w2", &his), Some((false, None)), "his post to the lead only informs w2, who wrote {joined}");
    let b = r.talk("w2");
    let ask = typed(&mut r, b, "lead", &[], &[], Some(PostKind::Ask { to: vec!["w2".into()] }), "cut 75");
    let out = r.free("w2");
    assert_eq!(hand(&out), Some((vec![ask.clone()], vec![], vec![his.clone()])), "his post by its first line: {out:?}");
    r.given("w2", b, r.m.tickets[&b].last());
    r.given("w2", chat, n);
    assert!(r.waiting_ids("w2").is_empty());
    r.replayed();
}

/// From the era [`ERA_QUIET`] a post brought along before a handed one on its thread goes whole only when it is for
/// the agent; one that only informs it goes by its first line, though the act names no digest (paper-research N-1446
/// p2367, 2; the audit's case 1: N-1104.p982, his 93-byte answer to fresh-lead-3, came with p959 to p981 whole, 23
/// posts and 17.8 KB of his talk with paper-research, by `his => deliver e.agent his listing e.waiting`). w2 took part
/// on his chat; four posts between him and the lead inform it; then his post to w2 goes at once, alone, with the four
/// as its digest, and the `given` up to it leaves nothing waiting: none is lost behind the read mark.
#[test]
fn from_the_era_quiet_the_posts_before_his_on_its_thread_that_only_inform_go_by_their_first_lines() {
    let mut r = Run::new();
    r.event(ERA, json!({"era": ERA_QUIET}));
    let chat = r.talk("lead");
    typed(&mut r, chat, "w2", &["lead"], &[], None, "@lead I am on it");
    r.given("w2", chat, r.m.tickets[&chat].last());
    let mut talk: Vec<String> = Vec::new();
    for k in 1..=2 {
        let n = r.post(chat, "owner", "lead", &format!("@lead the notes, part {k}"));
        talk.push(format!("{}.p{n}", r.m.id(chat)));
        talk.push(typed(&mut r, chat, "lead", &["owner"], &["w2"], None, &format!("part {k} is up")));
    }
    assert!(talk.iter().all(|i| pushed(&r, "w2", i) == Some((false, None))), "each only informs w2: {:?}", r.m.waiting("w2"));
    let n = r.post(chat, "owner", "w2", "@w2 yes, go");
    let his = format!("{}.p{n}", r.m.id(chat));
    let out = r.free("w2");
    assert_eq!(hand(&out), Some((vec![his.clone()], vec![], talk.clone())), "{out:?}");
    r.given("w2", chat, n);
    assert!(r.waiting_ids("w2").is_empty(), "the four went by their first lines");
    r.replayed();
}

/// From the era [`ERA_QUIET`] a typed fyi wakes nobody (paper-research N-1446 p2367, 5a; the audit's case 5a:
/// docket-edge6's `--fyi` lock notices p2259, p2265, p2276 and p2302 each woke paper-research and docket-core for a
/// Nothing turn): w2's ask to w3, then w3's fyi naming nobody, which the rule names w2 on as its answer, is pulled, not
/// w2's reply; nor does an fyi policy told w2 of wake it. An ask after them goes now, with both in its digest.
#[test]
fn from_the_era_quiet_an_fyi_wakes_nobody_not_as_a_reply_nor_told() {
    let mut r = Run::new();
    r.event(ERA, json!({"era": ERA_QUIET}));
    let a = r.talk("w3");
    let asked = typed(&mut r, a, "w2", &["w3"], &[], None, "@w3 is the lock free?");
    r.given("w3", a, r.m.tickets[&a].last());
    let free = typed(&mut r, a, "w3", &[], &[], Some(PostKind::Fyi), "the replay lock is free");
    assert_eq!(r.m.tickets[&a].posts.last().unwrap().names, ["w2"], "the rule names {asked}'s author");
    assert_eq!(pushed(&r, "w2", &free), Some((false, None)), "an fyi is no reply that wakes");
    r.ok(Input::Post { t: a, by: "w3".into(), text: Text::New("the lock is mine".into()), names: vec![], told: vec!["w2".into()], inform: vec![], answers: None, kind: Some(PostKind::Fyi) });
    let told = format!("{}.p{}", r.m.id(a), r.m.tickets[&a].last());
    assert_eq!(pushed(&r, "w2", &told), Some((false, None)), "nor told");
    let out = r.free("w2");
    assert!(hand(&out).is_none(), "nothing wakes w2: {out:?}");
    let ask = typed(&mut r, a, "w4", &[], &[], Some(PostKind::Ask { to: vec!["w2".into()] }), "check the lock");
    let out = r.free("w2");
    assert_eq!(hand(&out), Some((vec![ask], vec![], vec![free, told])), "{out:?}");
    r.replayed();
}
/// From the era [`ERA_QUIET`] another agent's post the agent answered waits no more (paper-research N-1446 p2367, 3;
/// the audit's case 3: N-1446's p2295 came while paper-research worked and was kept for its turn's end; it answered it
/// in p2298, the reply tool's `to`, and 5 s later was handed it whole, for a Nothing turn): w3's ask to w2 arrives while
/// it works and is kept; w2 answers it with the reply tool; at its turn's end nothing waits and nothing is handed. His
/// post, answered by naming him, still goes: it goes until handed.
#[test]
fn from_the_era_quiet_a_post_the_agent_answered_waits_no_more() {
    let mut r = Run::new();
    r.event(ERA, json!({"era": ERA_QUIET}));
    let a = r.talk("w2");
    let xn = r.post(a, "w3", "w2", "@w2 the lock?");
    let x = format!("{}.p{xn}", r.m.id(a));
    r.event("arrived", json!({"agent": "w2", "item": x}));
    assert_eq!(r.m.kept("w2").keys().cloned().collect::<Vec<_>>(), vec![x.clone()], "kept for its turn's end");
    r.ok(Input::Post { t: a, by: "w2".into(), text: Text::New("free at 05:23".into()), names: vec!["w3".into()], told: vec![], inform: vec![], answers: Some(xn), kind: None });
    assert!(r.waiting_ids("w2").is_empty(), "{x} answered by the reply tool");
    let out = r.free("w2");
    assert!(hand(&out).is_none(), "nothing handed at its turn's end: {out:?}");
    // his post, answered by naming him, goes all the same
    let hn = r.post(a, "owner", "w2", "@w2 and the notes?");
    typed(&mut r, a, "w2", &["owner"], &[], None, "@owner on it");
    let his = format!("{}.p{hn}", r.m.id(a));
    assert_eq!(r.waiting_ids("w2"), vec![his.clone()]);
    let out = r.free("w2");
    assert_eq!(hand(&out).map(|h| h.0), Some(vec![his]));
    r.replayed();
}

/// From the era [`ERA_QUIET`] an ask the agent was not handed still goes, though a post of the agent's after it names its
/// writer (paper-research N-1446 p2430: "you post an ask to me at 09:00 while I work; at 09:01 I post something else on
/// N-1446 that names you, written without having seen yours"): the name proves nothing of what it read. w4's ask to w2
/// arrives while it works and is kept; w2 posts "@w4 …", written without it; the ask still waits, and its turn's end
/// hands it.
#[test]
fn from_the_era_quiet_an_ask_the_agent_was_not_handed_goes_though_a_post_of_its_names_its_writer() {
    let mut r = Run::new();
    r.event(ERA, json!({"era": ERA_QUIET}));
    let a = r.talk("w2");
    let yn = r.post(a, "w4", "w2", "@w2 and the stills?");
    let y = format!("{}.p{yn}", r.m.id(a));
    r.event("arrived", json!({"agent": "w2", "item": y}));
    assert_eq!(r.m.kept("w2").keys().cloned().collect::<Vec<_>>(), vec![y.clone()], "kept for its turn's end");
    typed(&mut r, a, "w2", &["w4"], &[], None, "@w4 up at 06:00");
    assert_eq!(r.waiting_ids("w2"), vec![y.clone()], "written without it: it still waits");
    let out = r.free("w2");
    assert_eq!(hand(&out).map(|h| h.0), Some(vec![y]), "handed at its turn's end: {out:?}");
    r.replayed();
}

/// From the era [`ERA_QUIET`] a typed result answers an ask only as any post does (gemini-advisor N-1446 p2365, 4;
/// paper-research p2371 (c); proto-mind p2444): docket-core's N-1446.p2293, a result naming paper-research and
/// answering its p2284, went to docket-edge5, which had asked docket-core something on that thread before, as
/// docket-core's reply, and woke it (research's record line 176701). Here w2's ask to w3 and w4's, both handed to w3,
/// then w3's result naming w4 and answering w4's ask: it goes to w4 as the reply, and is pulled for w2, whose ask stays
/// open in w3's ledger. w3's next result, naming nobody, goes to w2 as the reply it waits on, as such a result did
/// before. The record replays so; with the era's line taken out, the first goes to w2 as the reply, as before.
#[test]
fn from_the_era_quiet_a_result_to_another_is_no_answer_to_an_ask_it_does_not_name() {
    let mut r = Run::new();
    r.event(ERA, json!({"era": ERA_QUIET}));
    let c = r.talk("w3");
    let lock = typed(&mut r, c, "w2", &[], &[], Some(PostKind::Ask { to: vec!["w3".into()] }), "is the replay lock free?");
    let gate = typed(&mut r, c, "w4", &[], &[], Some(PostKind::Ask { to: vec!["w3".into()] }), "run 73's gate");
    let g = r.m.tickets[&c].last();
    r.given("w3", c, g);
    r.ok(Input::Post { t: c, by: "w3".into(), text: Text::New("@w4 73's gate is green".into()), names: vec!["w4".into()], told: vec![], inform: vec!["w2".into()], answers: Some(g), kind: Some(PostKind::Result) });
    let green = format!("{}.p{}", r.m.id(c), r.m.tickets[&c].last());
    assert_eq!(pushed(&r, "w4", &green), Some((true, Some("reply".into()))), "it answers {gate}");
    assert_eq!(pushed(&r, "w2", &green), Some((false, None)), "it neither names w2 nor answers {lock}: into w2's digest");
    let owed = |r: &Run, ask: &str| r.m.ledger("w3").into_iter().find(|o| o.post == ask).map(|o| o.answer);
    assert_eq!((owed(&r, &lock), owed(&r, &gate)), (Some(None), Some(Some(green.clone()))));
    let free = typed(&mut r, c, "w3", &[], &["w2", "w4"], Some(PostKind::Result), "the lock is free");
    assert_eq!((pushed(&r, "w2", &free), pushed(&r, "w4", &free)), (Some((true, Some("reply".into()))), Some((false, None))), "a result naming nobody answers as before");
    assert_eq!(owed(&r, &lock), Some(Some(free.clone())));
    let m = r.replayed();
    assert_eq!(push_in(&m, "w2", &green), Some((false, None)), "replayed in the era");
    let before: Vec<Line> = r.log.iter().filter(|l| !matches!(l, Line::Event { name, value, .. } if name == ERA && value["era"] == json!(ERA_QUIET))).cloned().collect();
    let mut old = Machine::load(defs_with(DELIVERY_KIND));
    assert_eq!(old.replay(&before), Vec::<String>::new());
    assert_eq!(push_in(&old, "w2", &green), Some((true, Some("reply".into()))), "with no era line, as the rule before handed");
}

/// The copy (fresh-lead-3 N-1104 p1154, p1159; his yes p1160; paper-research N-1446 p3020, 1), agent-base's
/// delivery.kind as it is: his post arriving while w1 works, where coordd read that a copy answers it
/// (`copy_answers`, the policy) and that its turn's start can be cut (`forkable`, `began`), makes a copy of w1 cut
/// there (the act `fork`, `Effect::Copy`; where its turn began is its `turn_began` line's) and hands it his post on
/// its own (`pN.told`), kept for w1. The copy's last
/// message answers his post and waits for w1 whole; his next post goes to the same copy. At w1's turn's end, while
/// the copy still answers, everything waits; then w1 is given his posts and the copy's answer as one message, and the
/// copy is stopped (`Effect::End`), holding nothing more. Where coordd read no copy answers, or that the turn cannot
/// be cut, his post waits as before. A replay makes the same.
#[test]
fn his_post_while_it_works_goes_to_a_copy_cut_where_its_turn_began_and_its_turns_end_hands_over_the_exchange() {
    let mut r = Run::new();
    let arrived = |r: &mut Run, agent: &str, item: &str, forkable: bool, copy_answers: bool| r.event("arrived", json!({"agent": agent, "item": item, "forkable": forkable, "copy_answers": copy_answers}));
    // coordd's line as each turn begins: the delivery it began with, where a copy is cut
    let began = |r: &mut Run, agent: &str, delivery: &str| r.event(TURN_BEGAN, json!({"agent": agent, "delivery": delivery}));
    assert!(r.go(Input::Event { name: TURN_BEGAN.into(), value: json!({"delivery": "N-1.p4"}) }).reply.is_err(), "it names its agent");
    let copies = |out: &Out| -> Vec<(String, u64, String, String, String)> {
        out.effects.iter().filter_map(|e| match e { Effect::Copy { agent, t, call, from, place } => Some((agent.clone(), *t, call.clone(), from.clone(), place.clone())), _ => None }).collect()
    };
    // the policy says no copy answers, or its turn cannot be cut: kept for its turn's end, as before
    let b = r.talk("w2");
    began(&mut r, "w2", "N-1.p4");
    let q1 = r.post(b, "owner", "w2", "@w2 noted");
    let q2 = r.post(b, "owner", "w2", "@w2 and the lock");
    let (j1, j2) = (format!("{}.p{q1}", r.m.id(b)), format!("{}.p{q2}", r.m.id(b)));
    let out = arrived(&mut r, "w2", &j1, true, false);
    assert_eq!(copies(&out), vec![], "{out:?}");
    let out = arrived(&mut r, "w2", &j2, false, true);
    assert_eq!(copies(&out), vec![], "{out:?}");
    assert_eq!((kept_as(&r, "w2", &j1).as_deref(), kept_as(&r, "w2", &j2).as_deref()), (Some("comes when its turn ends"), Some("comes when its turn ends")));
    assert_eq!(r.m.copy_of("w2"), None);
    // his post while w1 works: a copy of w1, cut where its turn began, is handed it on its own
    let a = r.talk("w1");
    began(&mut r, "w1", "N-1.p4");
    assert_eq!(r.m.began("w1").as_deref(), Some("N-1.p4"));
    let o1 = r.post(a, "owner", "w1", "@w1 how far is the median?");
    let i1 = format!("{}.p{o1}", r.m.id(a));
    let out = arrived(&mut r, "w1", &i1, true, true);
    let made = copies(&out);
    assert_eq!(made.len(), 1, "{out:?}");
    let (copy, d, call, from, place) = made[0].clone();
    assert_eq!((call.as_str(), from.as_str(), place.as_str()), ("copy", "w1", "N-1.p4"));
    assert_eq!(r.m.tickets[&d].kind, "delivery", "a scoped agent of w1's delivery ticket");
    assert!(copy.starts_with('f'), "{copy}");
    assert_eq!(r.m.copy_of("w1"), Some(copy.clone()));
    assert!(out.effects.contains(&Effect::Deliver { to: copy.clone() }), "{out:?}");
    let acts: Vec<(String, String, Vec<String>, Vec<String>)> = r.acted().into_iter().filter(|(w, ..)| w == "w1").collect();
    assert_eq!(acts.iter().map(|(_, act, items, _)| (act.as_str(), items.clone())).collect::<Vec<_>>(), vec![("fork", vec![]), ("deliver", vec![i1.clone()]), ("keep", vec![i1.clone()])]);
    let told1 = format!("{i1}.{TOLD}");
    assert_eq!(r.waiting_ids(&copy), vec![told1.clone()], "his post, on its own");
    assert_eq!(kept_as(&r, "w1", &i1).as_deref(), Some("its copy answers it now; it comes when its turn ends"));
    assert!(r.m.copy_owed(&copy));
    // the copy's delivery hands it his post; its turn's last message answers it and waits for w1 whole
    let out = r.event("free", json!({"agent": copy, "doing": "-"}));
    assert_eq!(hand(&out), Some((vec![told1.clone()], vec![], vec![])), "{out:?}");
    r.ok(Input::Given { t: a, to: copy.clone(), delivery: told1.clone(), upto: 0, skipped: Default::default(), place: Some(Loc { file: "c.jsonl".into(), offset: 10, len: 5 }) });
    assert!(!r.m.copy_owed(&copy));
    let text = Loc { file: "c.jsonl".into(), offset: 100, len: 5 };
    let out = r.ok(Input::TurnEnded { agent: copy.clone(), delivery: Some(told1.clone()), text, later: vec![], stood: None, go: None });
    assert!(out.lines.iter().any(|l| matches!(l, Line::Replied { t, .. } if *t == a)), "{out:?}");
    let c1 = r.m.tickets[&a].last();
    let reply = r.m.tickets[&a].posts.iter().find(|p| p.n == c1).unwrap().clone();
    assert_eq!(reply.by, copy);
    assert_eq!(r.m.reply_names(&copy, &[]), None, "answered: the rule has nothing more to post");
    assert_eq!(reply.names, vec!["owner".to_string()], "the rule Reply names him: his post was handed to it");
    assert!(r.m.tickets[&a].answers(&reply, o1), "his post is answered: no more \"Asked, no answer yet\"");
    let told_c1 = format!("{}.p{c1}.{TOLD}", r.m.id(a));
    assert!(r.waiting_ids("w1").contains(&told_c1), "{:?}", r.waiting_ids("w1"));
    // his next post goes to the same copy
    let o2 = r.post(a, "owner", "w1", "@w1 and the lock?");
    let i2 = format!("{}.p{o2}", r.m.id(a));
    let out = arrived(&mut r, "w1", &i2, true, true);
    assert_eq!(copies(&out), vec![], "{out:?}");
    assert_eq!(r.m.copy_of("w1"), Some(copy.clone()));
    assert_eq!(r.waiting_ids(&copy), vec![format!("{i2}.{TOLD}")]);
    // its turn ends while the copy still answers: everything waits for it
    let free = |r: &mut Run, answering: bool| r.event("free", json!({"agent": "w1", "doing": "N-1 · the median", "worked": true, "answering": answering}));
    let out = free(&mut r, true);
    assert_eq!(hand(&out), None, "{out:?}");
    assert_eq!(kept_as(&r, "w1", &i1).as_deref(), Some("its copy is still answering the owner"));
    // then its posts and the copy's answer, whole, as one message, and the copy stops
    let out = free(&mut r, false);
    let (items, ..) = hand(&out).unwrap_or_else(|| panic!("{out:?}"));
    for i in [&i1, &i2, &told_c1] {
        assert!(items.contains(i), "{i} in {items:?}");
    }
    assert!(out.effects.contains(&Effect::End { agent: copy.clone() }), "{out:?}");
    assert_eq!(r.m.copy_of("w1"), None);
    assert!(!r.m.copy_owed(&copy), "a stopped copy holds nothing");
    assert_eq!(r.acted().last().map(|(_, act, ..)| act.clone()), Some("stop".to_string()));
    // his post in its next turn: a new copy, cut where that turn began
    began(&mut r, "w1", "N-1.p9");
    let o3 = r.post(a, "owner", "w1", "@w1 one more");
    let i3 = format!("{}.p{o3}", r.m.id(a));
    let out = arrived(&mut r, "w1", &i3, true, true);
    let again = copies(&out);
    assert!(again.len() == 1 && again[0].0 != copy && again[0].4 == "N-1.p9", "{again:?}");
    let m = r.replayed();
    assert_eq!(m.copy_of("w1"), r.m.copy_of("w1"));
    assert_eq!(m.waiting(&again[0].0), r.m.waiting(&again[0].0));
}

/// The record of a turn coordd began before its giving's place was read (try-80; proto-mind's copy proof,
/// paper-research N-1446 p3297, p3298, p3314): the giving with no place, `turn_began` naming none, the giving's place,
/// its `delivered` line with `began: true`, then the turn's `turn_began` line again, naming it (coordd
/// `ticket_delivered`). The machine reads them as it reads every `turn_began` line: no `began` after the first, the
/// giving after the second, and his post while the turn runs goes to a copy cut there. So an engine before reads them
/// the same. A replay makes the same.
#[test]
fn a_turn_whose_first_turn_began_line_named_none_began_with_the_delivery_its_second_names() {
    let mut r = Run::new();
    let copies = |out: &Out| -> Vec<(String, String)> { out.effects.iter().filter_map(|e| match e { Effect::Copy { from, place, .. } => Some((from.clone(), place.clone())), _ => None }).collect() };
    // the owner's thread to w1, handed; coordd's `given` before the session shows it, with no place
    let a = r.talk("w1");
    let p = r.post(a, "owner", "w1", "Four sums, then the numbers");
    let d = format!("{}.p{p}", r.m.id(a));
    let out = r.free("w1");
    assert_eq!(hand(&out), Some((vec![d.clone()], vec![], vec![])), "{out:?}");
    r.ok(Input::Given { t: a, to: "w1".into(), delivery: d.clone(), upto: p, skipped: Default::default(), place: None });
    r.event(TURN_BEGAN, json!({"agent": "w1", "delivery": null}));
    assert_eq!(r.m.began("w1"), None);
    // the giving's place, its `delivered` line, and the turn's line again
    let place = Loc { file: "w.jsonl".into(), offset: 118176, len: 494 };
    r.ok(Input::Given { t: a, to: "w1".into(), delivery: d.clone(), upto: p, skipped: Default::default(), place: Some(place.clone()) });
    r.event(DELIVERED, json!({"t": a, "to": "w1", "delivery": d, "at": "2026-11-25T03:25:12Z", "place": place, "began": true}));
    assert_eq!(r.m.began("w1"), None, "the `delivered` line alone names no turn's start");
    r.event(TURN_BEGAN, json!({"agent": "w1", "delivery": d}));
    assert_eq!(r.m.began("w1").as_deref(), Some(d.as_str()));
    // his post while it runs: a copy of w1, cut at that giving
    let o = r.post(a, "owner", "w1", "@w1 what is 7 times 6?");
    let item = format!("{}.p{o}", r.m.id(a));
    let out = r.event("arrived", json!({"agent": "w1", "item": item, "forkable": true, "copy_answers": true}));
    assert_eq!(copies(&out), vec![("w1".to_string(), d.clone())], "{out:?}");
    let m = r.replayed();
    assert_eq!((m.began("w1"), m.copy_of("w1")), (r.m.began("w1"), r.m.copy_of("w1")));
}

/// A copy coordd could not start (proto-mind's copy proof on engine 81, p3335: the session's channel tag carried no
/// id, and no entry was found to cut it at; paper-research N-1446 p3336): coordd's `start_failed`, naming the copy's
/// delivery ticket, drops it from w1's copies, with his post it was handed, and is written. His next post while w1
/// works makes a new copy, and a replay of the record makes the same. Before, the line was not written
/// (`delivery_line`), and a replay kept the copy that never ran and handed it his next post.
#[test]
fn a_copy_coordd_could_not_start_is_dropped_on_the_record_and_his_next_post_makes_another() {
    let mut r = Run::new();
    let copies = |out: &Out| -> Vec<(String, u64)> { out.effects.iter().filter_map(|e| match e { Effect::Copy { agent, t, .. } => Some((agent.clone(), *t)), _ => None }).collect() };
    let arrived = |r: &mut Run, item: &str| r.event("arrived", json!({"agent": "w1", "item": item, "forkable": true, "copy_answers": true}));
    let a = r.talk("w1");
    r.event(TURN_BEGAN, json!({"agent": "w1", "delivery": "N-1.p4"}));
    let o1 = r.post(a, "owner", "w1", "@w1 what is 7 times 6?");
    let i1 = format!("{}.p{o1}", r.m.id(a));
    let made = copies(&arrived(&mut r, &i1));
    assert_eq!(made.len(), 1, "{made:?}");
    let (copy, d) = made[0].clone();
    assert_eq!(r.m.copy_of("w1"), Some(copy.clone()));
    assert_eq!(r.waiting_ids(&copy), vec![format!("{i1}.{TOLD}")]);
    // coordd's Job::Start could not start it: its line, on the copy's delivery ticket, is written
    let out = r.ok(Input::StartFailed { t: d, agent: copy.clone(), why: "no entry of w.jsonl after N-1.p4's given line holds its message".into() });
    assert!(out.lines.iter().any(|l| matches!(l, Line::StartFailed { t, agent, .. } if *t == d && *agent == copy)), "{out:?}");
    assert_eq!(r.m.copy_of("w1"), None);
    assert_eq!(r.waiting_ids(&copy), Vec::<String>::new(), "nothing waits for a copy that never ran");
    // his next post while w1 works: a new copy
    let o2 = r.post(a, "owner", "w1", "@w1 and 8 times 9?");
    let i2 = format!("{}.p{o2}", r.m.id(a));
    let again = copies(&arrived(&mut r, &i2));
    assert!(again.len() == 1 && again[0].0 != copy, "{again:?}");
    assert_eq!(r.m.copy_of("w1"), Some(again[0].0.clone()));
    let m = r.replayed();
    assert_eq!(m.copy_of("w1"), r.m.copy_of("w1"));
    assert_eq!((m.waiting(&copy), m.waiting(&again[0].0)), (r.m.waiting(&copy), r.m.waiting(&again[0].0)));
    assert!(!m.tickets[&d].kept.contains(&copy), "{:?}", m.tickets[&d].kept);
}

/// The owner's urgent hand (N-1788): his post waiting for w1 goes as one message now, not an idle hand (so into a
/// running turn too), whatever the program would have done; another's post is not his to hurry; with nothing of his
/// waiting, or what waits already on its way, it says so; given, the hand goes; a replay reads the same.
#[test]
fn the_owners_urgent_hand_takes_his_waiting_words_now() {
    let mut r = Run::with(NEXT_KIND);
    let a = r.talk("w1");
    let px = r.post(a, "x", "w1", "@w1 the log");
    assert!(r.go(Input::Urgent { agent: "w1".into(), by: "owner".into() }).reply.is_err(), "x's post is not his");
    let p = r.post(a, "owner", "w1", "@w1 stop and look at the lock");
    let his = format!("{}.p{p}", r.m.id(a));
    assert!(r.go(Input::Urgent { agent: "w1".into(), by: "w1".into() }).reply.is_err(), "only the owner");
    let out = r.ok(Input::Urgent { agent: "w1".into(), by: "owner".into() });
    assert!(out.effects.iter().any(|e| matches!(e, Effect::Hand { to, items, .. } if to == "w1" && *items == vec![his.clone()])), "{out:?}");
    let hands = r.m.hands("w1");
    assert_eq!(hands.iter().map(|h| (h.items.clone(), h.idle)).collect::<Vec<_>>(), vec![(vec![his.clone()], false)]);
    assert!(r.acted().iter().any(|(ag, act, items, _)| ag == "w1" && act == "urgent" && *items == vec![his.clone()]));
    assert!(r.go(Input::Urgent { agent: "w1".into(), by: "owner".into() }).reply.is_err(), "on its way already");
    r.given("w1", a, p);
    assert!(r.m.hands("w1").is_empty(), "given, the hand goes");
    assert!(px < p);
    assert_eq!(Machine::load(defs_with(NEXT_KIND)).replay(&r.log), Vec::<String>::new());
}
