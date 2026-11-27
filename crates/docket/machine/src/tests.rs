//! The machine on the real kinds and calls (agent-base/kinds, agent-base/calls/claim.call and decision.call).

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const CALLS: [(&str, &str); 15] = [
    ("shape.call", include_str!("../../../../agent-base/calls/shape.call")),
    ("intake.call", include_str!("../../../../agent-base/calls/intake.call")),
    ("request.call", include_str!("../../../../agent-base/calls/request.call")),
    ("item.call", include_str!("../../../../agent-base/calls/item.call")),
    ("file.call", include_str!("../../../../agent-base/calls/file.call")),
    ("question.call", include_str!("../../../../agent-base/calls/question.call")),
    // before claim.call, whose `repair` names its Found (a file's types are read with its calls here)
    ("seek.call", include_str!("../../../../agent-base/calls/seek.call")),
    ("claim.call", include_str!("../../../../agent-base/calls/claim.call")),
    ("decision.call", include_str!("../../../../agent-base/calls/decision.call")),
    ("pick.call", include_str!("../../../../agent-base/calls/pick.call")),
    ("route.call", include_str!("../../../../agent-base/calls/route.call")),
    ("claims.call", include_str!("../../../../agent-base/calls/claims.call")),
    ("clarify.call", include_str!("../../../../agent-base/calls/clarify.call")),
    // the watcher's `asks`, gone from agent-base (proto-lead N-1479 p247): kept for the thread kinds before it, which
    // threads opened before hold and run (`defs_before_asks`, `defs_with_pick`)
    ("asks.call", include_str!("../testdata/asks-at-64.call")),
    ("gather.call", include_str!("../../../../agent-base/calls/gather.call")),
];
const KINDS: [(&str, &str); 10] = [
    ("base.kind", include_str!("../../../../agent-base/kinds/base.kind")),
    ("thread.kind", include_str!("../../../../agent-base/kinds/thread.kind")),
    ("request.kind", include_str!("../../../../agent-base/kinds/request.kind")),
    ("item.kind", include_str!("../../../../agent-base/kinds/item.kind")),
    ("filing.kind", include_str!("../../../../agent-base/kinds/filing.kind")),
    ("question.kind", include_str!("../../../../agent-base/kinds/question.kind")),
    ("decision.kind", include_str!("../../../../agent-base/kinds/decision.kind")),
    ("claim.kind", include_str!("../../../../agent-base/kinds/claim.kind")),
    ("fix.kind", include_str!("../../../../agent-base/kinds/fix.kind")),
    ("check.kind", include_str!("../../../../agent-base/kinds/check.kind")),
];

fn defs_with(extra: &[(&str, &str)]) -> (Defs, Vec<String>) {
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = CALLS.iter().flat_map(|(f, t)| syntax::parse_calls(f, t, &mut named).unwrap()).map(|c| (c.name.clone(), c)).collect();
    let mut files: Vec<(String, String)> = KINDS.iter().map(|(f, t)| (f.to_string(), t.to_string())).collect();
    files.extend(extra.iter().map(|(f, t)| (f.to_string(), t.to_string())));
    Defs::build(&files, calls, named, 200)
}

/// LOG as a record from before the calls went on it (no `calls` on its open, opened, edited and fired lines): what
/// a replay re-derives from today's calls (calls on the record, tests/calls_recorded.rs).
pub(crate) fn without_calls(log: &[Line]) -> Vec<Line> {
    log.iter()
        .map(|l| {
            let mut v = serde_json::to_value(l).unwrap();
            if let Some(o) = v.as_object_mut() {
                o.remove("calls");
                for f in o.get_mut("fired").and_then(Value::as_array_mut).into_iter().flatten() {
                    if let Some(f) = f.as_object_mut() {
                        f.remove("calls");
                    }
                }
            }
            serde_json::from_value(v).unwrap()
        })
        .collect()
}

fn defs() -> Defs {
    let (d, errors) = defs_with(&[]);
    assert!(errors.is_empty(), "{errors:?}");
    d
}

/// agent-base as it was before design-two-systems §16: the thread kind with its one-off (`give`, OneOff) and intake
/// answering it. What old logs were made with; they must replay under today's kinds.
fn defs_before_16() -> Defs {
    let mut named = BTreeMap::new();
    let old_intake = include_str!("../testdata/intake-before-16.call");
    let calls: BTreeMap<String, CallSig> = CALLS.iter().map(|(f, t)| if *f == "intake.call" { (*f, old_intake) } else { (*f, *t) }).flat_map(|(f, t)| syntax::parse_calls(f, t, &mut named).unwrap()).map(|c| (c.name.clone(), c)).collect();
    let old_thread = include_str!("../testdata/thread-before-16.kind");
    let files: Vec<(String, String)> = KINDS.iter().map(|(f, t)| (f.to_string(), if *f == "thread.kind" { old_thread } else { *t }.to_string())).collect();
    let (d, errors) = Defs::build(&files, calls, named, 200);
    assert!(errors.is_empty(), "{errors:?}");
    d
}

/// agent-base with the thread kind as it was until the owner's pick went (16 Nov, N-101.p1285): an Unsure waited on
/// `step owner pick`. What prod's open threads N-345 and N-356 were opened with.
fn defs_with_pick() -> Defs {
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = CALLS.iter().flat_map(|(f, t)| syntax::parse_calls(f, t, &mut named).unwrap()).map(|c| (c.name.clone(), c)).collect();
    let old_thread = include_str!("../testdata/thread-with-pick.kind");
    let files: Vec<(String, String)> = KINDS.iter().map(|(f, t)| (f.to_string(), if *f == "thread.kind" { old_thread } else { *t }.to_string())).collect();
    let (d, errors) = Defs::build(&files, calls, named, 200);
    assert!(errors.is_empty(), "{errors:?}");
    d
}

/// agent-base with the thread kind as it was before ls-opener (hs-ls N-1446 p73): no form `opener`, and an agent's
/// Named open posting its words again as the owner's (`post text as owner`). What N-1445 was opened with (research's
/// record lines 103218-103219); a thread opened so still has the opener's words as its #2 by the owner.
fn defs_before_opener() -> Defs {
    defs_before(&[("thread.kind", &thread_before_opener())])
}

/// agent-base with the thread kind as it was until the watcher's `asks` went (proto-lead N-1479 p247): its `when
/// agent_post` asks the watcher what an agent's post asks the owner, and `mark` puts the passages on the post. What every
/// thread opened before holds and runs, as research's N-1479 and N-1446 do.
fn defs_before_asks() -> Defs {
    defs_before(&[("thread.kind", include_str!("../testdata/thread-at-64.kind"))])
}

/// thread.kind's text as it was before ls-opener (`defs_before_opener`), for a run that binds other kinds beside it.
fn thread_before_opener() -> String {
    let thread = include_str!("../../../../agent-base/kinds/thread.kind");
    let (o, a, b) = (", opener: Agent?", "match opener {\n              none => post text naming to, names\n              o => post text as o naming to, names\n            }", "match opener {\n              none => post text on t naming typed\n              o => post text on t as o naming typed\n            }");
    assert_eq!((thread.matches(o).count(), thread.matches(a).count(), thread.matches(b).count()), (1, 2, 1), "thread.kind's form and three posts");
    thread.replace(o, "").replace(a, "post text as owner naming to, names").replace(b, "post text on t as owner naming typed")
}

/// agent-base with the fix kind as it was before `seek` (paper-research N-1396.p79): `repair` alone, then the reviews.
/// What every fix recorded before was opened with.
fn defs_before_seek() -> Defs {
    defs_before(&[("fix.kind", include_str!("../testdata/fix-before-seek.kind"))])
}

/// agent-base with the claim kind as it was before a missing lead went to the owner (paper-research N-1978 p2): a role
/// nobody fills fails the claim with NoWorker. What every claim opened before holds, and what a test of a failed ticket
/// needs.
fn defs_before_noworker() -> Defs {
    defs_before(&[("claim.kind", include_str!("../testdata/claim-before-noworker.kind"))])
}

/// agent-base with the kinds OLD in place of today's, by file name; a file of OLD today's lack is added, as `defs_with`
/// adds it (a binding beside an old kind).
fn defs_before(old: &[(&str, &str)]) -> Defs {
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = CALLS.iter().flat_map(|(f, t)| syntax::parse_calls(f, t, &mut named).unwrap()).map(|c| (c.name.clone(), c)).collect();
    let mut files: Vec<(String, String)> = KINDS.iter().map(|(f, t)| (f.to_string(), old.iter().find(|(o, _)| o == f).map_or(*t, |(_, o)| *o).to_string())).collect();
    files.extend(old.iter().filter(|(o, _)| !KINDS.iter().any(|(f, _)| f == o)).map(|(o, t)| (o.to_string(), t.to_string())));
    let (d, errors) = Defs::build(&files, calls, named, 200);
    assert!(errors.is_empty(), "{errors:?}");
    d
}

/// The way back from 0.3 to 38 (proto-reader N-1446 p335.3; proto-lead p346.4). proto-0.3's thread.kind and
/// item.kind are today's with L5's one clause after `letter` (06d870e5), and every open records its kind's program, so
/// a record written under 0.3 carries `drawn line`. 38 reads the clause and draws as 37 does. His message's thread
/// (opened by the `owner_message` binding, a fired program), a post on it and an item (an open line's program),
/// written under L5's kinds, replay under today's with no problem line, nothing broken and every view the same. In
/// proto-0.3, whose kinds are L5's already, the record is replayed under the kinds it was written with.
#[test]
fn a_record_written_under_l5s_drawn_line_replays_here() {
    let l5 = |file: &str, letter: &str| {
        let today = KINDS.iter().find(|(f, _)| *f == file).unwrap().1;
        assert!(today.contains(letter), "{file}");
        if today.contains("\n  drawn line") {
            return today.to_string();
        }
        today.replacen(letter, &format!("{letter}  drawn line   // a line of the outline where it split off (the kind says so, not a list of names: N-1446 p17)\n"), 1)
    };
    let (thread, item) = (l5("thread.kind", "  letter N\n"), l5("item.kind", "  letter I 3\n"));
    let d = defs_before(&[("thread.kind", &thread), ("item.kind", &item)]);
    assert_eq!((d.kinds["thread"].drawn, d.kinds["item"].drawn, d.kinds["request"].drawn), (Some(syntax::Drawn::Line), Some(syntax::Drawn::Line), None));
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    let a = r.say("lead", &["fixer"], "the release");
    let out = r.go(Input::Post { t: a, by: "lead".into(), text: Text::New("fixer, the lock is yours".into()), names: vec!["fixer".into()], told: vec![], inform: vec![], answers: None, kind: None });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let i = r.open_item("the stills");
    let programs: Vec<&str> = r
        .log
        .iter()
        .flat_map(|l| match l {
            Line::Open { program, .. } => vec![program.as_str()],
            Line::Event { fired, .. } => fired.iter().map(|f| f.program.as_str()).collect(),
            _ => Vec::new(),
        })
        .collect();
    assert!(programs.iter().filter(|p| p.contains("drawn line")).count() >= 2, "{programs:#?}");
    // in 38 today's kinds say no `drawn` and the record's programs do; in proto-0.3 both say `drawn line`
    let l5_today = KINDS.iter().find(|(f, _)| *f == "thread.kind").unwrap().1.contains("\n  drawn line");
    assert_eq!(defs().kinds["thread"].drawn, l5_today.then_some(syntax::Drawn::Line));
    r.replays();
    let mut m2 = Machine::load(defs());
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert_eq!([a, i].map(|t| m2.tickets[&t].broken.clone()), [None, None]);
}

struct Run {
    m: Machine,
    log: Vec<Line>,
    clock: u32,
}

impl Run {
    fn new() -> Run {
        Run { m: Machine::load(defs()), log: Vec::new(), clock: 0 }
    }
    /// A run under the kinds before §16 (`defs_before_16`): its log is an old one.
    fn before_16() -> Run {
        Run { m: Machine::load(defs_before_16()), log: Vec::new(), clock: 0 }
    }
    /// A run under thread.kind before the watcher's `asks` went (`defs_before_asks`): an agent's post where the owner
    /// takes part is read for what it asks him, and its passages marked.
    fn before_asks() -> Run {
        Run { m: Machine::load(defs_before_asks()), log: Vec::new(), clock: 0 }
    }
    /// A run under thread.kind before ls-opener (`defs_before_opener`): an agent's Named open posts its words again as
    /// the owner's #2.
    fn before_opener() -> Run {
        Run { m: Machine::load(defs_before_opener()), log: Vec::new(), clock: 0 }
    }
    fn go(&mut self, i: Input) -> Out {
        self.clock += 1;
        let at = format!("2026-11-10T12:00:{:02}Z", self.clock);
        self.at(i, &at)
    }
    /// INPUT at AT, a time as coordd stamps it: a record's own time (tests/tonight.rs), not the run's clock.
    fn at(&mut self, i: Input, at: &str) -> Out {
        let out = self.m.apply(i, at.to_string());
        self.log.extend(out.lines.iter().cloned());
        out
    }
    fn status(&self, t: u64) -> ViewStatus {
        self.m.view(t).unwrap().status
    }
    /// A Loc standing for an agent's turn output.
    fn loc(n: u64) -> Loc {
        Loc { file: "transcript.jsonl".into(), offset: n * 100, len: 50 }
    }
    fn open_claim(&mut self) -> u64 {
        let out = self.go(Input::Open {
            kind: "claim".into(),
            under: None,
            opener: "DEBUG".into(),
            form: json!({"what": "load fails", "evidence": "the agda MCP load errors", "where": "path:agda/x.agda:201", "branch": "main"}),
            text: None, names: vec![], told: vec![],
        });
        assert_eq!(out.reply, Ok("Opened C-200.".into()));
        assert!(out.effects.contains(&Effect::Ask { t: 200, step: 1, question: Question::Fills { role: "lead".into(), call: "assess".into() } }));
        200
    }
    fn decide(&mut self, t: u64, step: u32, who: &str) {
        let out = self.go(Input::Decided { t, step, question: "fills".into(), answer: json!(who), provenance: json!({}) });
        assert!(out.effects.contains(&Effect::Deliver { to: who.into() }), "{out:?}");
    }
    fn take(&mut self, agent: &str) -> Delivery {
        let due = self.m.due(agent);
        assert_eq!(due.len(), 1, "{due:?}");
        let d = due[0].clone();
        self.go(Input::Given { t: d.t, to: agent.into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Run::loc(1)) });
        assert!(self.m.due(agent).is_empty());
        d
    }
}

#[test]
fn the_kinds_check_and_their_bound() {
    let d = defs();
    assert_eq!(d.kinds.keys().cloned().collect::<Vec<_>>(), vec!["base", "check", "claim", "decision", "filing", "fix", "item", "question", "request", "thread"]);
    assert_eq!(check::bound(&d.kinds["filing"].pattern, &d.kinds), 1);
    assert_eq!(check::bound(&d.kinds["item"].pattern, &d.kinds), 0);
    // triage and the owner's decision there; the seek, the repair and three rounds of two reviews and a revision; the
    // owner's decision on an unclear review; the check
    assert_eq!(check::bound(&d.kinds["claim"].pattern, &d.kinds), 15);
}

impl Run {
    fn answer(&mut self, t: u64, by: &str, head: Value) {
        let out = self.go(Input::Answer { t, by: by.into(), head, text: Text::New("as said".into()) });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
    }
    /// The two reviewers of the round now open: their tickets and agents.
    fn reviewers(&self) -> Vec<(u64, String)> {
        self.m
            .tickets
            .values()
            .filter_map(|x| match &x.status {
                Status::Awaits { agent, .. } if x.kind == "part" => Some((x.t, agent.clone())),
                _ => None,
            })
            .collect()
    }
    /// A claim accepted, and its fix looked for and built (F-201's steps 1 and 2): its first round of reviews open.
    fn accept_and_repair(&mut self) {
        let c = self.open_claim();
        self.decide(c, 1, "orch");
        self.answer(c, "orch", json!({"head": "Accept"}));
        self.decide(201, 1, "dev");
        self.answer(201, "dev", json!({"head": "Nothing", "searched": "the loader in agda/"}));
        self.decide(201, 2, "dev");
        self.answer(201, "dev", json!({"head": "Commit", "commit": "abc"}));
    }
}

#[test]
fn a_claim_end_to_end_with_reviews_and_its_replay() {
    let mut r = Run::new();
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    let d = r.take("orch");
    assert_eq!(d.id, "C-200.s1");
    let What::Step { call, args, .. } = &d.what else { panic!() };
    assert_eq!(call, "assess");
    assert_eq!(args["where"], json!("path:agda/x.agda:201"));
    // the lead's turn ends; its output is formed into Accept
    let out = r.go(Input::TurnEnded { agent: "orch".into(), delivery: Some(d.id.clone()), text: Run::loc(2), later: vec![], stood: None, go: None });
    assert!(matches!(&out.effects[0], Effect::Form { t: 200, step: 1, .. }), "{out:?}");
    let out = r.go(Input::Formed { t: c, step: 1, head: Some(json!({"head": "Accept", "note": "see line 201"})), why: None, not_taken: None, call: None });
    assert!(out.effects.contains(&Effect::Ask { t: 201, step: 1, question: Question::Fills { role: "fixer".into(), call: "seek".into() } }));
    assert_eq!(r.m.view(c).unwrap().posts[0].text, Text::Loc { loc: Run::loc(2) });
    assert_eq!(r.m.view(c).unwrap().title, "load fails");
    // the fixer, through the explicit answer, first ill-typed
    r.decide(201, 1, "dev");
    let d = r.take("dev");
    assert_eq!(d.id, "F-201.s1");
    let What::Step { call, args, .. } = &d.what else { panic!() };
    assert_eq!((call.as_str(), &args["note"]), ("seek", &json!("see line 201")));
    let out = r.go(Input::Answer { t: 201, by: "dev".into(), head: json!({"head": "Extend"}), text: Text::New("done".into()) });
    assert!(out.reply.as_ref().is_err_and(|e| e.contains("2 more")), "{:?}", out.reply);
    assert_eq!(r.m.due("dev")[0].id, "F-201.s1.refused1");
    let found = json!({"head": "Extends", "pieces": "load at agda/x.agda:180, extended to read the MCP's error"});
    let out = r.go(Input::Answer { t: 201, by: "orch".into(), head: found.clone(), text: Text::New("done".into()) });
    assert!(out.reply.is_err() && out.lines.is_empty());
    r.answer(201, "dev", found.clone());
    // what seek found goes to repair, its step 2
    r.decide(201, 2, "dev");
    let d = r.take("dev");
    assert_eq!(d.id, "F-201.s2");
    let What::Step { call, args, .. } = &d.what else { panic!() };
    assert_eq!((call.as_str(), &args["found"], &args["note"]), ("repair", &found, &json!("see line 201")));
    let out = r.go(Input::Answer { t: 201, by: "dev".into(), head: json!({"head": "Commit", "commit": "abc"}), text: Text::New("fixed it".into()) });
    // round 1: two reviewers at once, each a part with a short-lived agent of its own, shown no posts
    assert!(out.effects.contains(&Effect::Start { agent: "f202-1".into(), t: 202, call: "fixes".into(), layer: None }), "{out:?}");
    assert!(out.effects.contains(&Effect::Start { agent: "f203-1".into(), t: 203, call: "better".into(), layer: None }));
    assert_eq!(r.reviewers(), vec![(202, "f202-1".to_string()), (203, "f203-1".to_string())]);
    assert_eq!(r.m.id(202), "P-202");
    let d = r.take("f202-1");
    let What::Step { unread, blind, args, .. } = &d.what else { panic!() };
    assert!(*blind && unread.is_empty());
    assert_eq!((&args["commit"], &args["branch"]), (&json!("abc"), &json!("main")));
    let out = r.go(Input::Answer { t: 202, by: "f202-1".into(), head: json!("Fixes"), text: Text::New("it does".into()) });
    assert!(out.effects.contains(&Effect::End { agent: "f202-1".into() }));
    assert!(matches!(r.status(201), ViewStatus::Waits { .. }), "the parent waits for every branch");
    r.answer(203, "f203-1", json!({"head": "Better", "how": "fix the cause in mean"}));
    // revised, then round 2 with new reviewers
    assert!(matches!(r.status(201), ViewStatus::Asking { ref call, .. } if call == "revise"));
    r.decide(201, 3, "dev");
    let What::Step { args, .. } = r.take("dev").what else { panic!() };
    assert_eq!(args, json!({"commit": "abc", "why": null, "how": "fix the cause in mean"}));
    r.answer(201, "dev", json!({"head": "Commit", "commit": "def"}));
    assert_eq!(r.reviewers(), vec![(204, "f204-1".to_string()), (205, "f205-1".to_string())]);
    r.answer(204, "f204-1", json!("Fixes"));
    r.answer(205, "f205-1", json!("Best"));
    assert_eq!(r.status(201), ViewStatus::Done { result: json!("def") });
    // the check goes to the runner, a program agent, which answers without a session
    assert_eq!(r.m.id(206), "K-206");
    assert!(matches!(r.status(206), ViewStatus::Awaits { ref agent, .. } if agent == "runner"));
    assert!(matches!(r.status(206), ViewStatus::Awaits { ref args, .. } if args == &json!({"commit": "def", "branch": "main"})), "{:?}", r.status(206));
    r.answer(206, "runner", json!("Green"));
    assert_eq!(r.status(c), ViewStatus::Done { result: json!({"head": "Fixed", "commit": "def"}) });
    assert!(r.m.tickets.values().all(|x| x.live.is_empty()));
    // one closing notice, to the claimant only
    assert_eq!(r.take("DEBUG").id, "C-200.c");
    assert!(r.m.due("dev").is_empty() && r.m.due("orch").is_empty());
    // a second claim gets the next number
    assert_eq!(r.go(Input::Open { kind: "claim".into(), under: None, opener: "DEBUG".into(), form: json!({"what": "w", "evidence": "e", "where": "p", "branch": "main"}), text: None, names: vec![], told: vec![] }).reply, Ok("Opened C-207.".into()));
    // the log replays to the same state, with no divergence
    let text: Vec<String> = r.log.iter().map(|l| serde_json::to_string(l).unwrap()).collect();
    let lines: Vec<Line> = text.iter().map(|s| serde_json::from_str(s).unwrap()).collect();
    let mut m2 = Machine::load(defs());
    assert_eq!(m2.replay(&lines), Vec::<String>::new());
    for t in 200..=207 {
        assert_eq!(m2.view(t), r.m.view(t), "ticket {t}");
    }
    // a log with a line dropped is caught
    let mut bad = lines.clone();
    let i = bad.iter().position(|l| matches!(l, Line::Opened { kind, .. } if kind == "fix")).unwrap();
    bad.remove(i);
    let mut m3 = Machine::load(defs());
    assert!(!m3.replay(&bad).is_empty());
    assert!(m3.view(200).unwrap().broken.is_some());
}

/// A fix recorded before `seek` replays on its own recorded program (its opened line keeps it): `repair` its step 1,
/// with FOUND none, its round of reviews, done, the same today; a fix opened today runs `seek`, then `repair` with what
/// it found (the end-to-end test). Its claim replays the same but for its bound, which counts its `open fix` by the fix kind loaded now (new_ticket's
/// `check::bound`), as it did before `seek`: shown only (the view), checked against nothing.
#[test]
fn a_fix_recorded_before_seek_replays_on_its_own_program() {
    let mut r = Run { m: Machine::load(defs_before_seek()), log: Vec::new(), clock: 0 };
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    r.answer(c, "orch", json!({"head": "Accept"}));
    r.decide(201, 1, "dev");
    let What::Step { call, args, .. } = r.take("dev").what else { panic!() };
    assert_eq!((call.as_str(), args.get("found").unwrap_or(&Value::Null)), ("repair", &Value::Null));
    r.answer(201, "dev", json!({"head": "Commit", "commit": "abc"}));
    let rv = r.reviewers();
    r.answer(rv[0].0, &rv[0].1, json!("Fixes"));
    r.answer(rv[1].0, &rv[1].1, json!("Best"));
    assert_eq!(r.status(201), ViewStatus::Done { result: json!("abc") });
    let text: Vec<String> = r.log.iter().map(|l| serde_json::to_string(l).unwrap()).collect();
    let lines: Vec<Line> = text.iter().map(|s| serde_json::from_str(s).unwrap()).collect();
    let mut m2 = Machine::load(defs());
    assert_eq!(m2.replay(&lines), Vec::<String>::new());
    for t in r.m.tickets.keys() {
        let (today, then) = (m2.view(*t).unwrap(), r.m.view(*t).unwrap());
        if *t == c {
            assert_eq!((then.bound, today.bound), (14, 15));
        }
        assert_eq!(TicketView { bound: 0, ..today }, TicketView { bound: 0, ..then }, "ticket {t}");
        assert!(m2.tickets[t].broken.is_none(), "{:?}", m2.tickets[t].broken);
    }
}

#[test]
fn a_revision_that_does_not_shrink_the_measure_stalls() {
    let mut r = Run::new();
    r.accept_and_repair();
    for (round, commit) in [(3, "b"), (4, "c")] {
        let rv = r.reviewers();
        r.answer(rv[0].0, &rv[0].1, json!({"head": "DoesNot", "why": "still divides by n-1"}));
        r.answer(rv[1].0, &rv[1].1, json!("Best"));
        r.decide(201, round, "dev");
        r.answer(201, "dev", json!({"head": "Commit", "commit": commit}));
    }
    let ViewStatus::Failed { raised } = r.status(201) else { panic!("{:?}", r.status(201)) };
    assert_eq!((raised.exc.as_str(), raised.value.clone()), ("Stalled", json!("c")));
    let ViewStatus::Failed { raised } = r.status(200) else { panic!() };
    assert_eq!(raised.exc, "Stalled");
}

#[test]
fn a_failed_branch_is_raised_once_every_branch_is_done() {
    let mut r = Run::new();
    r.accept_and_repair();
    let rv = r.reviewers();
    for _ in 0..3 {
        r.go(Input::Answer { t: rv[0].0, by: rv[0].1.clone(), head: json!("Perhaps"), text: Text::New("?".into()) });
    }
    assert!(matches!(r.status(rv[0].0), ViewStatus::Failed { .. }));
    assert!(matches!(r.status(201), ViewStatus::Waits { .. }));
    r.answer(rv[1].0, &rv[1].1, json!("Best"));
    let ViewStatus::Failed { raised } = r.status(201) else { panic!() };
    assert_eq!(raised.exc, "Malformed");
}

/// N-54: a `//` comment on a kind's `raises`, `goal` or `letter` line is cut, as on `form` and `result`; a name in
/// `raises` that is not one is refused as such, with its line.
#[test]
fn comments_on_raises_goal_and_letter_lines_are_cut() {
    let looping = "kind looping\n  letter L   // its ids\n  form    topic: Text            // what opening takes\n  result  Text                   // what it closes with\n  raises  Exhausted,             // or how it fails\n          Stalled                // and this\n  goal    green                  // policy's act\n  pattern\n    iterate c = \"a\" up to 2 {\n      r <- step x triage(c, c, \"p\");\n      match r { Accept(n) => return Again(c), Dismiss(w) => return Done(w) }\n    }\n";
    let (d, errors) = defs_with(&[("looping.kind", looping)]);
    assert!(errors.is_empty(), "{errors:?}");
    let k = &d.kinds["looping"];
    assert_eq!((k.raises.clone(), k.goals.clone(), k.letter), (vec!["Exhausted".to_string(), "Stalled".to_string()], vec!["green".to_string()], Some('L')));
    let bad = looping.replace("raises  Exhausted,", "raises  Exhausted Stalled,");
    let (_, errors) = defs_with(&[("looping.kind", &bad)]);
    assert!(errors.iter().any(|e| e.contains("looping.kind:5") && e.contains("`Exhausted Stalled` is not a name")), "{errors:?}");
}

#[test]
fn an_iterate_without_a_measure_is_exhausted() {
    let looping = "kind looping\n  result Text\n  raises Exhausted, Stalled\n  pattern\n    iterate c = \"a\" up to 2 {\n      r <- step x triage(c, c, \"p\");\n      match r { Accept(n) => return Again(c), Dismiss(w) => return Done(w) }\n    }\n";
    let (d, errors) = defs_with(&[("looping.kind", looping)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut m = Machine::load(d);
    m.apply(Input::Open { kind: "looping".into(), under: None, opener: "a".into(), form: json!({}), text: None, names: vec![], told: vec![] }, "t".into()).reply.unwrap();
    for _ in 0..2 {
        m.apply(Input::Answer { t: 200, by: "x".into(), head: json!("Accept"), text: Text::New(String::new()) }, "t".into()).reply.unwrap();
    }
    let ViewStatus::Failed { raised } = m.view(200).unwrap().status else { panic!() };
    assert_eq!(raised.exc, "Exhausted");
}

#[test]
fn remind_once_then_malformed_after_three() {
    let mut r = Run::new();
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    r.take("orch");
    r.go(Input::TurnEnded { agent: "orch".into(), delivery: None, text: Run::loc(2), later: vec![], stood: None, go: None });
    let out = r.go(Input::Formed { t: c, step: 1, head: None, why: None, not_taken: None, call: None });
    assert!(out.lines.iter().any(|l| matches!(l, Line::Reminded { .. })));
    let d = r.take("orch");
    assert_eq!(d.id, "C-200.s1.again");
    r.go(Input::TurnEnded { agent: "orch".into(), delivery: None, text: Run::loc(3), later: vec![], stood: None, go: None });
    let out = r.go(Input::Formed { t: c, step: 1, head: Some(json!({"head": "Maybe"})), why: None, not_taken: None, call: None });
    assert!(!out.lines.iter().any(|l| matches!(l, Line::Reminded { .. })));
    assert!(r.m.due("orch").is_empty());
    // a turn end with no taken step owed answers nothing and is not logged
    assert!(r.go(Input::TurnEnded { agent: "dev".into(), delivery: None, text: Run::loc(4), later: vec![], stood: None, go: None }).lines.is_empty());
    // the second not read passed it to the owner (no lead; paper-research N-101 p757): three answers that do not type
    assert!(matches!(r.status(c), ViewStatus::Awaits { agent, .. } if agent == "owner"), "{:?}", r.status(c));
    for _ in 0..3 {
        r.go(Input::Answer { t: c, by: "owner".into(), head: json!({"head": "Nope"}), text: Text::New("x".into()) });
    }
    let ViewStatus::Failed { raised } = r.status(c) else { panic!("{:?}", r.status(c)) };
    assert_eq!(raised.exc, "Malformed");
    assert_eq!(r.take("DEBUG").id, "C-200.c");
}

#[test]
fn the_text_of_an_answer_fills_its_one_text_field() {
    let mut r = Run::new();
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    r.go(Input::Answer { t: c, by: "orch".into(), head: json!({"head": "Dismiss"}), text: Text::New("not a defect".into()) }).reply.unwrap();
    assert_eq!(r.status(c), ViewStatus::Done { result: json!({"head": "Dismissed", "why": "not a defect"}) });
}

/// Nobody fills the lead: the claim's triage is the owner's decision (N-1978 p2); under the claim kind before that,
/// the claim fails with NoWorker, as every claim opened before it does.
#[test]
fn nobody_fills_the_role() {
    let mut r = Run::new();
    let c = r.open_claim();
    r.go(Input::Decided { t: c, step: 1, question: "fills".into(), answer: Value::Null, provenance: json!({}) });
    assert!(matches!(r.status(c), ViewStatus::Waits { .. }), "{:?}", r.status(c));
    let d = r.m.tickets.iter().find(|(_, x)| x.kind == "decision").map(|(t, _)| *t).expect("a decision opened");
    assert!(matches!(&r.m.tickets[&d].status, Status::Awaits { agent, .. } if agent == "owner"), "{:?}", r.m.tickets[&d].status);
    let mut r = Run { m: Machine::load(defs_before_noworker()), log: Vec::new(), clock: 0 };
    let c = r.open_claim();
    r.go(Input::Decided { t: c, step: 1, question: "fills".into(), answer: Value::Null, provenance: json!({}) });
    let ViewStatus::Failed { raised } = r.status(c) else { panic!() };
    assert_eq!(raised.exc, "NoWorker");
}

#[test]
fn posts_naming_nobody_where_nobody_reads() {
    let mut r = Run::new();
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    r.go(Input::Answer { t: c, by: "orch".into(), head: json!({"head": "Accept"}), text: Text::New("go".into()) }).reply.unwrap();
    r.decide(201, 1, "dev");
    let out = r.go(Input::Post { t: c, by: "DEBUG".into(), text: Text::New("also line 202".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None });
    let e = out.reply.unwrap_err();
    assert!(e.contains("F-201 awaits dev (seek)"), "{e}");
    // naming someone reaches them
    let out = r.go(Input::Post { t: c, by: "DEBUG".into(), text: Text::New("also line 202".into()), names: vec!["dev".into()], told: vec![], inform: vec![], answers: None, kind: None });
    assert!(out.effects.contains(&Effect::Deliver { to: "dev".into() }));
    let due = r.m.due("dev");
    assert_eq!(due.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), vec!["C-200.p2", "F-201.s1"]);
    // a post on the awaited ticket goes with its step
    r.go(Input::Post { t: 201, by: "orch".into(), text: Text::New("mind the tests".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
    let What::Step { unread, .. } = &r.m.due("dev")[1].what else { panic!() };
    assert_eq!(unread.len(), 1);
}

#[test]
fn forms_are_checked_and_coerced() {
    let mut r = Run::new();
    let out = r.go(Input::Open { kind: "claim".into(), under: None, opener: "a".into(), form: json!({"what": "x"}), text: None, names: vec![], told: vec![] });
    assert!(out.reply.unwrap_err().contains("evidence"));
    let out = r.go(Input::Open { kind: "clam".into(), under: None, opener: "a".into(), form: json!({}), text: None, names: vec![], told: vec![] });
    assert!(out.reply.unwrap_err().contains("claim"));
    assert_eq!(Type::Record(vec![("n".into(), Type::Int)]).coerce(&json!({"n": "3"})), Ok(json!({"n": 3})));
}

#[test]
fn ill_formed_kinds_are_refused_with_their_line() {
    let bad = [
        ("a.kind", "kind a\n  result Text\n  pattern\n    r <- step x triage(\"w\", \"e\", \"p\");\n    match r {\n      Dismiss(why) => return why\n    }\n"),
        ("b.kind", "kind b\n  result Text\n  pattern\n    r <- step x triage(\"w\", \"e\", \"p\");\n    match r {\n      _ => return \"x\"\n      Dismiss(why) => return why\n    }\n"),
        ("c.kind", "kind c\n  result Ref\n  pattern\n    open d()\nkind d\n  result Ref\n  pattern\n    open c()\n"),
        ("e.kind", "kind e\n  result Ref\n  pattern\n    raise Oops(\"x\")\n"),
    ];
    let (d, errors) = defs_with(&bad);
    let all = errors.join("\n");
    assert!(all.contains("kind a refused") && all.contains("does not cover"), "{all}");
    assert!(all.contains("kind b refused") && all.contains("never be taken"), "{all}");
    assert!(all.contains("kind c refused") && all.contains("opens itself"), "{all}");
    assert!(all.contains("kind e refused") && all.contains("Oops"), "{all}");
    assert_eq!(d.kinds.len(), KINDS.len());
}

#[test]
fn a_step_that_waited_too_long_is_withdrawn_and_fails_as_silent() {
    let mut r = Run::new();
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    r.take("orch");
    assert_eq!(r.m.awaiting().len(), 1);
    r.go(Input::Expired { t: c, step: 1, provenance: json!({}) }).reply.unwrap();
    let ViewStatus::Failed { raised } = r.status(c) else { panic!() };
    assert_eq!((raised.exc.as_str(), raised.value.clone()), ("Silent", json!("orch")));
    // the agent is told the step was withdrawn; an answer after it is refused
    assert_eq!(r.take("orch").id, "C-200.s1.withdrawn");
    assert!(r.go(Input::Answer { t: c, by: "orch".into(), head: json!("Accept"), text: Text::New(String::new()) }).reply.is_err());
    assert_eq!(r.take("DEBUG").id, "C-200.c");
}

#[test]
fn a_ticket_replays_under_the_program_it_was_opened_with() {
    // opened under an older claim whose raises were fewer than what fix raises today
    let old = defs().kinds["claim"].text.replace("raises NoFix, NeedsDecision, Exhausted, Stalled", "raises NoFix");
    let line = Line::Open { at: "t".into(), t: 200, kind: "claim".into(), under: None, opener: "a".into(), form: json!({"what": "w", "evidence": "e", "where": "p", "branch": "main"}), text: None, names: vec![], told: vec![], esc: vec![], program: old, calls: Default::default(), closed: None, asked_on: None };
    let mut r = Run::new();
    let out = r.m.apply(Input::Open { kind: "claim".into(), under: None, opener: "a".into(), form: json!({"what": "w", "evidence": "e", "where": "p", "branch": "main"}), text: None, names: vec![], told: vec![] }, "t".into());
    let mut lines = out.lines.clone();
    lines[0] = line;
    let mut m = Machine::load(defs());
    assert_eq!(m.replay(&lines), Vec::<String>::new());
}

#[test]
fn an_unchecked_act_is_logged_before_it_and_replays() {
    let mut r = Run::new();
    let out = r.go(Input::Unchecked { act: "open".into(), by: "DEBUG".into(), t: None, kind: "claim".into(), why: "policy/x.eflint: parse error".into() });
    assert_eq!((out.reply, out.effects), (Ok(String::new()), vec![]));
    let t = r.open_claim();
    assert!(matches!(&r.log[0], Line::Unchecked { act, why, .. } if act == "open" && why.contains("parse error")));
    assert!(matches!(&r.log[1], Line::Open { t: 200, .. }));
    let text: Vec<String> = r.log.iter().map(|l| serde_json::to_string(l).unwrap()).collect();
    let lines: Vec<Line> = text.iter().map(|s| serde_json::from_str(s).unwrap()).collect();
    let mut m2 = Machine::load(defs());
    assert_eq!(m2.replay(&lines), Vec::<String>::new());
    assert_eq!(m2.view(t), r.m.view(t));
}

#[test]
fn a_trailing_optional_argument_may_be_left_out() {
    let mut r = Run::new();
    // a claim as recorded before `branch`: it opens fix and check with one argument fewer
    let program = |s: &str| s.lines().skip_while(|l| l.starts_with("//")).collect::<Vec<_>>().join("\n") + "\n";
    let old = program(include_str!("../../../../agent-base/kinds/claim.kind"))
        .replace(", branch: Ref\n", "\n")
        .replace("open fix(what, evidence, where, note, branch)", "open fix(what, evidence, where, note)")
        .replace("open check(commit, branch)", "open check(commit)");
    let line = Line::Open { at: "t".into(), t: 200, kind: "claim".into(), under: None, opener: "a".into(), form: json!({"what": "w", "evidence": "e", "where": "p"}), text: None, names: vec![], told: vec![], esc: vec![], program: old, calls: Default::default(), closed: None, asked_on: None };
    let refused = |p: Vec<String>| p.iter().any(|e| e.contains("now refused"));
    assert!(!refused(r.m.replay(&[line])));
    // too few for a required parameter: the log is what happened, so it opens, marked broken, and fails NoCall at
    // its first step (Repair can give it a program)
    let bad = program(include_str!("../../../../agent-base/kinds/claim.kind")).replace("open check(commit, branch)", "open check()");
    let line = Line::Open { at: "t".into(), t: 300, kind: "claim".into(), under: None, opener: "a".into(), form: json!({"what": "w", "evidence": "e", "where": "p", "branch": "main"}), text: None, names: vec![], told: vec![], esc: vec![], program: bad, calls: Default::default(), closed: None, asked_on: None };
    assert!(!refused(r.m.replay(&[line])));
    let v = r.m.view(300).unwrap();
    assert!(v.broken.as_deref().is_some_and(|b| b.contains("no longer checks")), "{:?}", v.broken);
    let ViewStatus::Failed { raised } = v.status else { panic!("{:?}", v.status) };
    assert_eq!(raised.exc, "NoCall");
}

#[test]
fn a_fix_the_reviewers_cannot_judge_goes_to_the_owner() {
    for (owner_says, ends) in [(json!({"head": "Yes"}), "Fixed"), (json!({"head": "No", "why": "it only hides it"}), "NoFix")] {
        let mut r = Run::new();
        r.accept_and_repair();
        let rv = r.reviewers();
        r.answer(rv[0].0, &rv[0].1, json!("Unclear"));
        r.answer(rv[1].0, &rv[1].1, json!("Best"));
        // the fix raised NeedsDecision; the claim caught it and opened a decision for the owner, under the claim
        let ViewStatus::Failed { raised } = r.status(201) else { panic!("{:?}", r.status(201)) };
        assert_eq!(raised.exc, "NeedsDecision");
        let d = 204;
        assert_eq!((r.m.id(d), r.m.tickets[&d].parent), ("D-204".to_string(), Some(200)));
        // the claim waits on the owner, down its tree
        assert_eq!(r.m.view(200).unwrap().waits_on, vec![WaitsOn { ticket: "D-204".into(), agent: "owner".into(), call: "decide".into(), post: None, layer: None }]);
        assert!(matches!(r.status(d), ViewStatus::Awaits { ref agent, ref args, .. } if agent == "owner" && args["context"] == json!("abc")), "{:?}", r.status(d));
        r.answer(d, "owner", owner_says.clone());
        if ends == "Fixed" {
            r.answer(205, "runner", json!("Green"));
            assert_eq!(r.status(200), ViewStatus::Done { result: json!({"head": "Fixed", "commit": "abc"}) });
        } else {
            let ViewStatus::Failed { raised } = r.status(200) else { panic!("{:?}", r.status(200)) };
            assert_eq!((raised.exc.as_str(), raised.value.clone()), ("NoFix", json!("it only hides it")));
        }
        let lines: Vec<Line> = r.log.iter().map(|l| serde_json::from_str(&serde_json::to_string(l).unwrap()).unwrap()).collect();
        let mut m2 = Machine::load(defs());
        assert_eq!(m2.replay(&lines), Vec::<String>::new());
        assert_eq!(m2.view(200), r.m.view(200));
    }
}

#[test]
fn try_catches_what_it_names_and_lets_the_rest_through() {
    // a silent worker is caught; the step after the try runs with the arm's value
    let k = "kind patient\n  result Text\n  pattern\n    r <- try { v <- step x triage(\"w\", \"e\", \"p\"); return \"answered\" } handle {\n      Silent(a) => return a\n    };\n    return r\n";
    let (d, errors) = defs_with(&[("patient.kind", k)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut m = Machine::load(d);
    m.apply(Input::Open { kind: "patient".into(), under: None, opener: "a".into(), form: json!({}), text: None, names: vec![], told: vec![] }, "t".into()).reply.unwrap();
    m.apply(Input::Expired { t: 200, step: 1, provenance: json!({}) }, "t".into()).reply.unwrap();
    assert_eq!(m.view(200).unwrap().status, ViewStatus::Done { result: json!("x") });
    // what an arm does not name passes through, and must be in `raises`
    let k = "kind leaky\n  result Text\n  pattern\n    try { raise Boom(\"b\") } handle { Silent(a) => return a }\n";
    let (_, errors) = defs_with(&[("leaky.kind", k)]);
    assert!(errors.iter().any(|e| e.contains("Boom")), "{errors:?}");
    // an arm for an exception nothing in the try raises is refused, and so is one handled twice
    let k = "kind dead\n  result Text\n  pattern\n    try { return \"a\" } handle { Boom(x) => return \"b\" }\n";
    assert!(defs_with(&[("dead.kind", k)]).1.iter().any(|e| e.contains("never be taken")));
    let k = "kind twice\n  result Text\n  pattern\n    try { return \"a\" } handle { Silent(x) => return \"b\"  Silent(y) => return \"c\" }\n";
    assert!(defs_with(&[("twice.kind", k)]).1.iter().any(|e| e.contains("twice")));
}

#[test]
fn with_sets_the_context_for_its_body_and_the_tickets_opened_in_it() {
    let outer = "kind outer\n  result Text\n  pattern\n    a <- with who = \"dev\" in { c <- open inner(); return c };\n    b <- step x triage(a, \"e\", \"p\");\n    match b { Accept(n) => return a, Dismiss(w) => return w }\n";
    let inner = "kind inner\n  result Text\n  pattern\n    r <- step x triage(ctx.who, ctx.outer, \"p\");\n    match r { Accept(n) => return ctx.who, Dismiss(w) => return w }\n";
    let (d, errors) = defs_with(&[("outer.kind", outer), ("inner.kind", inner)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut m = Machine::load(d);
    m.apply(Input::Open { kind: "outer".into(), under: None, opener: "a".into(), form: json!({}), text: None, names: vec![], told: vec![] }, "t".into()).reply.unwrap();
    // the child sees the field, and its parent as ctx.outer
    assert_eq!(m.view(201).unwrap().ctx, json!({"who": "dev", "outer": "O-200"}));
    let ViewStatus::Awaits { args, .. } = m.view(201).unwrap().status else { panic!() };
    assert_eq!(args, json!({"what": "dev", "evidence": "O-200", "where": "p"}));
    m.apply(Input::Answer { t: 201, by: "x".into(), head: json!("Accept"), text: Text::New(String::new()) }, "t".into()).reply.unwrap();
    // after the with, the parent's context is as before
    assert_eq!(m.view(200).unwrap().ctx, json!({}));
    let ViewStatus::Awaits { args, .. } = m.view(200).unwrap().status else { panic!("{:?}", m.view(200).unwrap().status) };
    assert_eq!(args["what"], json!("dev"));
}

#[test]
fn a_note_must_name_who_reads_it() {
    let mut r = Run::new();
    let c = r.open_claim();
    // an agents' thread now (design part 9b), which ends as it opens, as a note did
    let note = |under: Option<u64>, names: Vec<String>| Input::Open { kind: "thread".into(), under, opener: "dev".into(), form: json!({"to": "orch", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "dev"}, "follows": null}), text: Some(Text::New("the build is slow today".into())), names, told: vec![] };
    // naming nobody: refused, and nothing is logged
    let out = r.go(note(None, vec![]));
    assert!(out.reply.as_ref().is_err_and(|e| e.contains("names nobody")) && out.lines.is_empty(), "{out:?}");
    // under an open ticket the refusal points there
    let out = r.go(note(Some(c), vec![]));
    assert!(out.reply.unwrap_err().contains("post on C-200 instead"));
    // naming someone: done at once, delivered to them, no closing notice to its opener
    let out = r.go(note(Some(c), vec!["orch".into()]));
    assert_eq!(out.reply, Ok("Opened N-201.".into()));
    assert!(out.effects.contains(&Effect::Deliver { to: "orch".into() }));
    assert_eq!(r.status(201), ViewStatus::Done { result: json!({"head": "Between"}) });
    let d = r.m.due("orch");
    assert!(d.iter().any(|d| d.id == "N-201.p1" && matches!(d.what, What::Posts { .. })), "{d:?}");
    assert!(r.m.due("dev").iter().all(|d| d.t != 201));
}

#[test]
fn the_lead_may_ask_the_owner_at_triage() {
    for (owner_says, then) in [(json!({"head": "Yes", "note": "skip the Nones"}), "fix"), (json!({"head": "No", "why": "not a defect"}), "dismissed")] {
        let mut r = Run::new();
        let c = r.open_claim();
        r.decide(c, 1, "orch");
        r.answer(c, "orch", json!({"head": "Decide", "question": "Should mean() skip None?"}));
        // a decision for the owner under the claim, with the claim's what as context
        assert!(matches!(r.status(201), ViewStatus::Awaits { ref agent, ref args, .. }
            if agent == "owner" && args == &json!({"question": "Should mean() skip None?", "context": "load fails"})), "{:?}", r.status(201));
        r.answer(201, "owner", owner_says.clone());
        if then == "fix" {
            // the owner's note goes to the fixer as the note
            r.decide(202, 1, "dev");
            let What::Step { call, args, .. } = r.take("dev").what else { panic!() };
            assert_eq!((call.as_str(), &args["note"]), ("seek", &json!("skip the Nones")));
        } else {
            assert_eq!(r.status(c), ViewStatus::Done { result: json!({"head": "Dismissed", "why": "not a defect"}) });
        }
        let lines: Vec<Line> = r.log.iter().map(|l| serde_json::from_str(&serde_json::to_string(l).unwrap()).unwrap()).collect();
        assert_eq!(Machine::load(defs()).replay(&lines), Vec::<String>::new());
    }
}

#[test]
fn a_close_tells_whom_notify_adds() {
    let mut r = Run::new();
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    let out = r.go(Input::Answer { t: c, by: "orch".into(), head: json!({"head": "Dismiss", "why": "not a defect"}), text: Text::New("no".into()) });
    assert!(out.effects.contains(&Effect::Notify { t: c, event: "closed".into(), actor: "orch".into(), post: None }), "{:?}", out.effects);
    // the opener is told anyway; the lead too, once policy says so
    let out = r.go(Input::Notified { t: c, event: "closed".into(), to: vec!["DEBUG".into(), "lead".into()], inform: vec![], post: None, provenance: json!({}) });
    assert!(out.effects.contains(&Effect::Deliver { to: "lead".into() }));
    assert_eq!(r.take("lead").id, "C-200.c");
    assert_eq!(r.take("DEBUG").id, "C-200.c");
    // asked again with nobody new: nothing logged
    assert!(r.go(Input::Notified { t: c, event: "closed".into(), to: vec!["lead".into()], inform: vec![], post: None, provenance: json!({}) }).lines.is_empty());
    // whom policy says inform is told without a turn started for it: due, marked so
    let out = r.go(Input::Notified { t: c, event: "closed".into(), to: vec![], inform: vec!["dev".into(), "lead".into()], post: None, provenance: json!({}) });
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::Deliver { .. })), "{:?}", out.effects);
    let due = r.m.due("dev");
    assert_eq!(due.iter().map(|d| (d.id.as_str(), d.inform)).collect::<Vec<_>>(), [("C-200.c", true)]);
    assert!(r.m.due("lead").is_empty(), "notified already");
    let lines: Vec<Line> = r.log.iter().map(|l| serde_json::from_str(&serde_json::to_string(l).unwrap()).unwrap()).collect();
    assert_eq!(Machine::load(defs()).replay(&lines), Vec::<String>::new());
}

#[test]
fn a_question_goes_to_the_lead_then_up_to_the_owner() {
    for up in [false, true] {
        let mut r = Run::new();
        let out = r.go(Input::Open { kind: "question".into(), under: None, opener: "dev".into(), form: json!({"ask": "Which branch do I fix on?"}), text: None, names: vec![], told: vec![] });
        assert_eq!(out.reply, Ok("Opened Q-200.".into()));
        assert!(out.effects.contains(&Effect::Ask { t: 200, step: 1, question: Question::Fills { role: "lead".into(), call: "answer".into() } }));
        r.decide(200, 1, "orch");
        if up {
            r.answer(200, "orch", json!({"head": "Up", "why": "it is the owner's call"}));
            assert!(matches!(r.status(200), ViewStatus::Awaits { ref agent, ref args, .. } if agent == "owner" && args["why"] == json!("it is the owner's call")));
            r.answer(200, "owner", json!({"head": "Answer", "text": "main"}));
        } else {
            r.answer(200, "orch", json!({"head": "Answer", "text": "main"}));
        }
        assert_eq!(r.status(200), ViewStatus::Done { result: json!({"head": "Answered", "text": "main"}) });
        // the asker is told once, by the answer
        let d = r.m.due("dev");
        assert!(d.len() == 1 && d[0].id == "Q-200.c", "{d:?}");
    }
}

#[test]
fn an_event_opens_what_the_bindings_on_it_name() {
    let binds = "// the owner's messages become questions\non owner_message(m) open question(m)\non owner_message(m)\n  open question(m, \"from the owner\")\n";
    let (mut d, errors) = defs_with(&[("binds.kind", binds)]);
    assert!(errors.is_empty(), "{errors:?}");
    d.bindings.retain(|b| b.kind != "thread");
    assert_eq!(d.bindings.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(), ["on owner_message(m) open question(m)", "on owner_message(m) open question(m, \"from the owner\")"]);
    let mut m = Machine::load(d.clone());
    // an event no binding is on changes nothing and is not logged
    let out = m.apply(Input::Event { name: "turn_end".into(), value: json!("x") }, "t".into());
    assert!(out.reply.is_err() && out.lines.is_empty());
    let out = m.apply(Input::Event { name: "owner_message".into(), value: json!("Which branch?") }, "t1".into());
    assert_eq!(out.reply, Ok("Opened Q-200, Q-201.".into()));
    assert!(out.effects.contains(&Effect::Ask { t: 201, step: 1, question: Question::Fills { role: "lead".into(), call: "answer".into() } }));
    let v = m.view(201).unwrap();
    assert_eq!((v.form.clone(), v.parent, v.opener.clone()), (json!({"ask": "Which branch?", "context": "from the owner"}), None, None));
    let mut log = out.lines.clone();
    let out = m.apply(Input::Decided { t: 200, step: 1, question: "fills".into(), answer: json!("orch"), provenance: json!({}) }, "t2".into());
    log.extend(out.lines);
    let out = m.apply(Input::Answer { t: 200, by: "orch".into(), head: json!({"head": "Answer", "text": "main"}), text: Text::New(String::new()) }, "t3".into());
    // nobody opened it: who hears of its close is policy's `notify`
    assert!(out.effects.contains(&Effect::Notify { t: 200, event: "closed".into(), actor: "orch".into(), post: None }), "{:?}", out.effects);
    log.extend(out.lines);
    // the log replays without the bindings: each ticket carries its program
    let (mut d2, _) = defs_with(&[]);
    d2.bindings.clear();
    let mut m2 = Machine::load(d2);
    assert_eq!(m2.replay(&log), Vec::<String>::new());
    assert_eq!(m2.view(200).unwrap().status, m.view(200).unwrap().status);
}

#[test]
fn ill_formed_bindings_are_refused_with_their_line() {
    let (_, errors) = defs_with(&[("b.kind", "on owner_message(m) open nothing(m)\non owner_message(m) open question(m, m, m)\non x(m) where \"a\" open question(m)\nevery 2 x open question(\"a\")\nevery 1h where m open question(\"a\")\n")]);
    assert_eq!(errors.len(), 5, "{errors:?}");
    assert!(errors.iter().any(|e| e.starts_with("b.kind:3: binding refused: `where` takes a Bool")), "{errors:?}");
    assert!(errors.iter().any(|e| e.starts_with("b.kind:4: `every 2 x`")), "{errors:?}");
    assert!(errors.iter().any(|e| e.starts_with("b.kind:5: a clock has no event")), "{errors:?}");
    assert!(errors.iter().any(|e| e.starts_with("b.kind:1: binding refused: there is no kind nothing")), "{errors:?}");
    assert!(errors.iter().any(|e| e.starts_with("b.kind:2: binding refused: a question takes 2")), "{errors:?}");
}

impl Run {
    /// A machine where the owner's message opens a filing, as before the thread kind (old logs replay so).
    fn filings() -> Run {
        let (mut d, errors) = defs_with(&[("f.kind", "on owner_message(m) open filing(m.text)")]);
        assert!(errors.is_empty(), "{errors:?}");
        d.bindings.retain(|b| b.kind != "thread");
        Run { m: Machine::load(d), log: Vec::new(), clock: 0 }
    }
}

/// The owner types TEXT: the filing it opens (the old binding, Run::filings), the clerk's step owed.
fn filing(r: &mut Run, text: &str) -> u64 {
    let out = r.go(Input::Event { name: "owner_message".into(), value: json!({"id": "m1", "from": "owner", "text": text}) });
    let t = parse_ticket(out.reply.as_ref().unwrap().trim_start_matches("Opened ").trim_end_matches('.')).unwrap();
    assert!(matches!(r.status(t), ViewStatus::Awaits { ref agent, .. } if agent == "clerk"), "{:?}", r.status(t));
    t
}

#[test]
fn file_posts_each_piece_as_the_owner_and_answers_where_it_names_the_result() {
    let mut r = Run::filings();
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    let q = 201;
    r.go(Input::Open { kind: "question".into(), under: None, opener: "dev".into(), form: json!({"ask": "Which branch?"}), text: None, names: vec![], told: vec![] });
    r.decide(q, 1, "orch");
    r.answer(q, "orch", json!({"head": "Up", "why": "the owner's call"}));
    let text = "main, please. And the claim: look at line 201 first.";
    let f = filing(&mut r, text);
    let (a, b) = (text.find("main").unwrap(), text.find("And").unwrap());
    let head = json!({"head": "Filed", "pieces": [
        {"ticket": "C-200", "at": b, "len": text.len() - b},
        {"ticket": "Q-201", "at": a, "len": b - 1 - a, "answer": "Answer"}]});
    let out = r.go(Input::Answer { t: f, by: "clerk".into(), head, text: Text::New(String::new()) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(matches!(r.status(f), ViewStatus::Done { .. }), "{:?}", r.status(f));
    // the post refers to the span of the filing's text; it is shown as the words
    let p = r.m.tickets[&c].posts.last().unwrap().clone();
    assert_eq!((p.by.as_str(), &p.text), ("owner", &Text::Part { part: Part { t: f, at: b as u64, len: (text.len() - b) as u64 } }));
    assert_eq!(r.m.view(c).unwrap().posts.last().unwrap().text, Text::New("And the claim: look at line 201 first.".into()));
    // the orchestrator awaiting the claim's step gets the post with it
    assert!(out.effects.contains(&Effect::Deliver { to: "orch".into() }));
    assert_eq!(r.status(q), ViewStatus::Done { result: json!({"head": "Answered", "text": "main, please."}) });
    // the owner is not told of a filing that went through
    assert!(r.m.due("owner").is_empty(), "{:?}", r.m.due("owner"));
    // and it replays
    let mut m = Machine::load(defs());
    assert_eq!(m.replay(&r.log), Vec::<String>::new());
    assert_eq!(m.view(q).unwrap().status, r.status(q));
}

#[test]
fn what_cannot_be_placed_fails_the_filing_and_nothing_is_filed() {
    let bad = |text: &str, pieces: Value| {
        let mut r = Run::filings();
        let c = r.open_claim();
        r.decide(c, 1, "orch");
        let f = filing(&mut r, text);
        r.answer(f, "clerk", json!({"head": "Filed", "pieces": pieces}));
        let ViewStatus::Failed { raised } = r.status(f) else { panic!("{:?}", r.status(f)) };
        assert_eq!(r.m.tickets[&c].posts.len(), 0);
        // it comes back to the owner
        assert!(r.m.due("owner").iter().any(|d| d.t == f), "{:?}", r.m.due("owner"));
        raised.value["why"].as_str().unwrap().to_string()
    };
    let why = bad("fix it. Also, deploy", json!([{"ticket": "C-200", "at": 0, "len": 7}]));
    assert_eq!(why, "no piece places \"Also, deploy\"");
    let why = bad("fix it", json!([{"ticket": "C-999", "at": 0, "len": 6}]));
    assert_eq!(why, "piece 1: there is no ticket C-999");
    let why = bad("fix it", json!([{"ticket": "C-200", "at": 2, "len": 60}]));
    assert!(why.starts_with("piece 1: bytes 2 to 62"), "{why}");
    let why = bad("yes", json!([{"ticket": "C-200", "at": 0, "len": 3, "answer": "Yes"}]));
    assert_eq!(why, "piece 1: C-200 awaits orch, not the owner");
    // the clerk's own Unplaced fails it too
    let mut r = Run::filings();
    let f = filing(&mut r, "hm");
    r.answer(f, "clerk", json!({"head": "Unplaced", "why": "it names no ticket"}));
    assert!(matches!(r.status(f), ViewStatus::Failed { ref raised } if raised.exc == "Unplaced"));
    assert_eq!(r.m.due("owner").len(), 1);
}

/// Agent AGENT's turn ended with its output at LOC, answering no step: the filing the binding opens, or the reply.
#[test]
fn a_part_of_a_text_is_shown_as_its_words() {
    // Machine::readable (docket-edge m950, finding 3): a part of a ticket's text, anywhere in a value, as its words
    let mut r = Run::new();
    let out = r.go(Input::Open { kind: "filing".into(), under: None, opener: "owner".into(), form: json!({"text": "fix it. Also, deploy"}), text: None, names: vec![], told: vec![] });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let f = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    let part = Text::Part { part: Part { t: f, at: 8, len: 12 } }.value();
    assert_eq!(r.m.readable(&json!({"a": [part], "n": 3})), json!({"a": ["Also, deploy"], "n": 3}));
    let gone = Text::Part { part: Part { t: 999, at: 0, len: 3 } }.value();
    assert!(r.m.readable(&gone).as_str().is_some_and(|s| s.contains("not there")), "{:?}", r.m.readable(&gone));
}

#[test]
fn a_post_on_a_note_goes_to_the_others_on_it() {
    let mut r = Run::new();
    let out = r.go(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("try the new build".into())), names: vec!["try2".into()], told: vec![] });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let n = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    assert!(!r.m.tickets[&n].is_open());
    // the agent answers on it, naming nobody, as its delivery's footer says: the owner gets it
    let out = r.go(Input::Post { t: n, by: "try2".into(), text: Text::New("it starts".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert_eq!(r.m.tickets[&n].posts.last().unwrap().names, vec!["owner".to_string()]);
    assert!(out.effects.contains(&Effect::Deliver { to: "owner".into() }), "{:?}", out.effects);
    // and it replays
    let mut m = Machine::load(defs());
    assert_eq!(m.replay(&r.log), Vec::<String>::new());
}

#[test]
fn a_runner_fault_opens_a_claim() {
    // the dev project's faults.kind (plan/core.md §8): the field names are the stub's, one line to change at cut-over
    let (d, errors) = defs_with(&[("faults.kind", "on job_fault(f) open claim(f.cause, f.excerpt, f.fingerprint, f.branch)\n")]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    let out = r.go(Input::Event { name: "job_fault".into(), value: json!({"event": "job_fault", "cause": "the build broke on main", "excerpt": "error[E0308]: mismatched types", "fingerprint": "job:build:E0308", "branch": "main", "notify": "owner"}) });
    assert_eq!(out.reply, Ok("Opened C-200.".into()));
    let x = &r.m.tickets[&200];
    assert_eq!((x.kind.as_str(), x.opener.clone(), x.form["where"].clone()), ("claim", None, json!("job:build:E0308")));
    assert!(out.effects.contains(&Effect::Ask { t: 200, step: 1, question: Question::Fills { role: "lead".into(), call: "assess".into() } }));
}

#[test]
fn every_call_file_of_agent_base_parses() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../agent-base/calls");
    let mut paths: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "call")).collect();
    paths.sort();
    let files: Vec<(String, String)> = paths.iter().map(|p| (p.display().to_string(), std::fs::read_to_string(p).unwrap())).collect();
    assert!(files.len() >= 7, "{files:?}");
    let mut named = BTreeMap::new();
    syntax::parse_types("system", syntax::SYSTEM_TYPES, &mut named).unwrap();
    let (calls, errors) = syntax::parse_call_files(&files, &mut named);
    assert!(errors.is_empty(), "{errors:#?}");
    // every call of every file, with its template where it has one; a decision is declared as a call is, by the
    // starts `parse_call_defs_each` reads (`call ` or `decision `), and is named `decide NAME` (claims.call's)
    let declared: usize = files.iter().map(|(_, t)| t.lines().filter(|l| l.starts_with("call ") || l.starts_with("decision ")).count()).sum();
    assert_eq!(calls.len(), declared);
    assert_eq!(calls[&format!("{}claims", syntax::DECIDE)].template, None);
    assert!(calls["place"].template.as_deref().is_some_and(|t| t.contains("{{")), "{:?}", calls["place"]);
    assert_eq!(calls["file"].template, None);
    assert_eq!(calls["place"].attr("task"), Some("clerk"));
    // the template keeps its own indentation below the common one
    let t = syntax::parse_call_defs("t.call", "call c(x: Text) -> Text\n  task t\n  template\n    a\n      b\n\n    {{ x }}\n\n", &named).unwrap();
    assert_eq!(t[0].template.as_deref(), Some("a\n  b\n\n{{ x }}"));
    assert!(syntax::parse_call_defs("t.call", "call c(x: Text) -> Text\n  task t\n  tempate\n", &named).unwrap_err().starts_with("t.call:3:"));
}

#[test]
fn a_type_may_name_one_declared_later() {
    let mut named = BTreeMap::new();
    syntax::parse_types("system", syntax::SYSTEM_TYPES, &mut named).unwrap();
    let a = ("a.call".to_string(), "type Piece {\n  kind: PieceKind\n  text: String\n}\ntype PieceKind = Question | Report\n\ncall c(t: Text) -> [Piece]\n  task t\n".to_string());
    let b = ("b.call".to_string(), "call d(x: Later) -> Text\n  task t\n".to_string());
    let c = ("c.call".to_string(), "type Later {\n  y: Int\n}\n".to_string());
    let (calls, errors) = syntax::parse_call_files(&[a, b, c], &mut named);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(named["Piece"], Type::Record(vec![("kind".into(), Type::Variant(vec![("Question".into(), vec![]), ("Report".into(), vec![])])), ("text".into(), Type::Text)]));
    assert_eq!(calls["d"].sig.params, vec![("x".to_string(), Type::Record(vec![("y".into(), Type::Int)]))]);
    // a cycle is refused with its line, and that file's calls are left out
    let (calls, errors) = syntax::parse_call_files(&[("e.call".into(), "type A {\n  b: B\n}\ntype B {\n  a: A\n}\ncall e() -> A\n  task t\n".into())], &mut BTreeMap::new());
    assert!(errors.iter().any(|e| e.contains("e.call:") && e.contains("names itself")), "{errors:?}");
    assert!(!calls.contains_key("e"));
    // a name no file declares is refused with its line
    let (_, errors) = syntax::parse_call_files(&[("f.call".into(), "type P {\n  k: Nowhere\n}\n".into())], &mut BTreeMap::new());
    assert!(errors.iter().any(|e| e.contains("f.call") && e.contains("no type `Nowhere`")), "{errors:?}");
}

#[test]
fn a_ticket_whose_call_is_gone_is_broken_not_sent() {
    // a claim is opened; then its calls' file goes bad, and coordd starts without its calls
    let mut r = Run::new();
    r.open_claim();
    let (mut d, _) = defs_with(&[]);
    d.calls.remove("assess");
    let (d, errors) = Defs::build(&KINDS.iter().map(|(f, t)| (f.to_string(), t.to_string())).collect::<Vec<_>>(), d.calls, d.named, 200);
    // the kind that needs it is refused, with why, where kind refusals are shown
    assert!(errors.iter().any(|e| e.contains("kind claim refused") && e.contains("assess")), "{errors:?}");
    let mut m = Machine::load(d);
    // a record from before the calls went on it (one that has them runs against its own: tests/calls_recorded.rs)
    let problems = m.replay(&without_calls(&r.log));
    assert!(!problems.is_empty());
    // the ticket is marked broken, so it shows; nothing is due to anyone for it
    assert!(m.view(200).is_none_or(|v| v.broken.is_some()), "{:?}", m.view(200));
    assert!(m.due("orch").is_empty());
}

#[test]
fn a_step_that_cannot_be_rendered_fails_at_once_with_nocall() {
    let mut r = Run::new();
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    let out = r.go(Input::Unrendered { t: c, step: 1, why: "the calls do not load".into() });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let ViewStatus::Failed { raised } = r.status(c) else { panic!("{:?}", r.status(c)) };
    assert_eq!((raised.exc.as_str(), &raised.value), ("NoCall", &json!({"call": "assess", "why": "the calls do not load"})));
    // nothing withdrawn from orch, which was never told; the opener hears of the failure
    assert!(r.m.due("orch").is_empty(), "{:?}", r.m.due("orch"));
    assert_eq!(r.m.due("DEBUG").len(), 1);
    // a second one for the same step changes nothing
    assert!(r.go(Input::Unrendered { t: c, step: 1, why: "again".into() }).reply.is_err());
    let mut m = Machine::load(defs());
    assert_eq!(m.replay(&r.log), Vec::<String>::new());
}

impl Run {
    fn cancel(&mut self, t: u64, by: &str, why: &str) -> Out {
        self.go(Input::Cancel { t, by: by.into(), why: why.into() })
    }
    fn replays(&self) {
        self.replays_with(&[]);
    }
    /// As `replays`, with kinds EXTRA beside agent-base's.
    fn replays_with(&self, extra: &[(&str, &str)]) {
        let text: Vec<String> = self.log.iter().map(|l| serde_json::to_string(l).unwrap()).collect();
        let lines: Vec<Line> = text.iter().map(|s| serde_json::from_str(s).unwrap()).collect();
        let mut m2 = Machine::load(defs_with(extra).0);
        assert_eq!(m2.replay(&lines), Vec::<String>::new());
        for t in self.m.tickets.keys() {
            assert_eq!(m2.view(*t), self.m.view(*t), "ticket {t}");
        }
    }
    /// A log made under the kinds before §16 (`Run::before_16`) replays under today's: the same lines, the same
    /// tickets, but for the hole's scope, which a Keep takes from today's thread kind. So does one made under the thread
    /// kind before the watcher's `asks` went (`Run::before_asks`), as research's threads opened on 64 replay on 65's:
    /// but for a wordless ticket's title, its kind's doc, which a kind read from its recorded text has not
    /// (`Defs::kind_as`), so it is the kind's name there.
    fn replays_today(&self) {
        let text: Vec<String> = self.log.iter().map(|l| serde_json::to_string(l).unwrap()).collect();
        let lines: Vec<Line> = text.iter().map(|s| serde_json::from_str(s).unwrap()).collect();
        let mut m2 = Machine::load(defs());
        assert_eq!(m2.replay(&lines), Vec::<String>::new());
        for t in self.m.tickets.keys() {
            // and for a step's answer type, which is its call's: intake's head has OneOff today
            fn untyped(ns: &mut [RunNode]) {
                for n in ns {
                    n.ty = None;
                    untyped(&mut n.nodes);
                }
            }
            let (mut a, mut b) = (m2.view(*t).unwrap(), self.m.view(*t).unwrap());
            untyped(&mut a.run);
            untyped(&mut b.run);
            if a.title == a.kind {
                a.title = b.title.clone();
            }
            assert_eq!(TicketView { hole: None, ..a }, TicketView { hole: None, ..b }, "ticket {t}");
            assert!(m2.tickets[t].broken.is_none(), "{:?}", m2.tickets[t].broken);
        }
    }
}

#[test]
fn cancel_withdraws_the_step_and_fails_the_ticket() {
    let mut r = Run::new();
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    r.take("orch");
    r.cancel(c, "owner", "not wanted").reply.unwrap();
    let ViewStatus::Failed { raised } = r.status(c) else { panic!() };
    assert_eq!((raised.exc.as_str(), raised.value.clone()), ("Cancelled", json!({"by": "owner", "why": "not wanted"})));
    // the agent is told, with by whom and why; an answer now is refused
    let d = r.take("orch");
    assert_eq!(d.id, "C-200.s1.withdrawn");
    assert_eq!(d.what, What::Withdrawn { step: 1, call: "assess".into(), by: Some("owner".into()), why: Some("not wanted".into()) });
    assert!(r.go(Input::Answer { t: c, by: "orch".into(), head: json!("Accept"), text: Text::New(String::new()) }).reply.is_err());
    assert_eq!(r.take("DEBUG").id, "C-200.c");
    // a closed ticket cannot be cancelled, and the refusal is not logged
    let n = r.log.len();
    assert!(r.cancel(c, "owner", "again").reply.is_err());
    assert_eq!(r.log.len(), n);
    r.replays();
}

#[test]
fn cancel_closes_the_open_tickets_below_and_ends_their_agents() {
    let mut r = Run::new();
    r.accept_and_repair();
    let revs = r.reviewers();
    assert_eq!(revs.len(), 2);
    let out = r.cancel(200, "orch", "duplicate of C-12");
    out.reply.unwrap();
    for t in [200, 201, revs[0].0, revs[1].0] {
        assert!(matches!(r.status(t), ViewStatus::Failed { .. }), "ticket {t}");
    }
    for (_, a) in &revs {
        assert!(out.effects.contains(&Effect::End { agent: a.clone() }), "{a}");
    }
    // the reviewers were never given their steps, so nothing is withdrawn from them
    for (_, a) in &revs {
        assert!(r.m.due(a).is_empty());
    }
    // one close of the claim only
    assert_eq!(r.log.iter().filter(|l| matches!(l, Line::Closed { t: 200, .. })).count(), 1);
    r.replays();
}

#[test]
fn a_cancelled_branch_fails_its_parent_once_the_others_are_done() {
    let mut r = Run::new();
    r.accept_and_repair();
    let revs = r.reviewers();
    r.cancel(revs[0].0, "orch", "enough").reply.unwrap();
    // the fix still waits for the other branch
    assert!(matches!(r.status(201), ViewStatus::Waits { .. }), "{:?}", r.status(201));
    let other = revs[1].0;
    let ViewStatus::Awaits { call, .. } = r.status(other) else { panic!() };
    let head = if call == "fixes" { json!("Fixes") } else { json!("Best") };
    r.answer(other, &revs[1].1, head);
    let ViewStatus::Failed { raised } = r.status(200) else { panic!("{:?}", r.status(200)) };
    assert_eq!(raised.exc, "Cancelled");
    r.replays();
}

impl Run {
    fn open_item(&mut self, title: &str) -> u64 {
        let out = self.go(Input::Open { kind: "item".into(), under: None, opener: "orch".into(), form: json!({"title": title}), text: None, names: vec![], told: vec![] });
        let reply = out.reply.unwrap();
        let t = parse_ticket(reply.trim_start_matches("Opened ").trim_end_matches('.')).unwrap();
        assert!(reply.starts_with("Opened I-"), "{reply}");
        t
    }
}

#[test]
fn a_hold_is_owed_but_never_delivered_reminded_or_expired() {
    let mut r = Run::new();
    let i = r.open_item("faster replay");
    let out = r.go(Input::Decided { t: i, step: 1, question: "fills".into(), answer: json!("orch"), provenance: json!({}) });
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::Deliver { .. })), "{out:?}");
    assert!(out.lines.iter().any(|l| matches!(l, Line::Step { hold: true, .. })));
    let ViewStatus::Awaits { agent, hold, .. } = r.status(i) else { panic!() };
    assert_eq!((agent.as_str(), hold), ("orch", true));
    assert!(r.m.due("orch").is_empty());
    assert_eq!(r.m.holds("orch"), vec![(i, 1, "close".to_string())]);
    assert!(r.m.owes("orch").is_empty());
    // never Silent: not awaiting as policy's `expired` sees it, and an expiry is refused
    assert!(r.m.awaiting().is_empty());
    assert!(r.go(Input::Expired { t: i, step: 1, provenance: json!({}) }).reply.is_err());
    // a turn's end does not answer it
    r.go(Input::TurnEnded { agent: "orch".into(), delivery: None, text: Run::loc(3), later: vec![], stood: None, go: None });
    assert!(matches!(r.status(i), ViewStatus::Awaits { .. }));
    // an answer that does not fit is refused and not counted
    let n = r.log.len();
    for _ in 0..3 {
        assert!(r.go(Input::Answer { t: i, by: "orch".into(), head: json!("Finished"), text: Text::New(String::new()) }).reply.is_err());
    }
    assert_eq!(r.log.len(), n);
    // answered by another agent (coordd asked `may edit`): the answer is its own
    r.go(Input::Answer { t: i, by: "dev".into(), head: json!("Done"), text: Text::New("landed in abc".into()) }).reply.unwrap();
    let ViewStatus::Done { result } = r.status(i) else { panic!() };
    assert_eq!(result, json!({"head": "Done", "why": "landed in abc", "by": null}));
    assert_eq!(r.m.view(i).unwrap().posts.last().unwrap().by, "dev");
    r.replays();
}

#[test]
fn a_hold_nobody_fills_is_held_by_nobody() {
    let mut r = Run::new();
    let i = r.open_item("old work");
    r.go(Input::Decided { t: i, step: 1, question: "fills".into(), answer: Value::Null, provenance: json!({}) }).reply.unwrap();
    let ViewStatus::Awaits { agent, .. } = r.status(i) else { panic!("{:?}", r.status(i)) };
    assert_eq!(agent, NOBODY);
    // a cancel withdraws nothing from anyone
    r.cancel(i, "owner", "stale").reply.unwrap();
    assert!(r.m.due(NOBODY).is_empty());
    r.replays();
}

#[test]
fn items_are_imported_with_their_numbers_and_tell_nobody() {
    let mut r = Run::new();
    let closing = Closing { by: "orch".into(), at: "2026-10-01T10:00:00Z".into(), head: json!({"head": "Done", "why": "landed"}), text: Some(Text::New("landed".into())) };
    // a closed item: closed at once, its program not run, nobody told
    let out = r.go(Input::Import { t: 143, kind: "item".into(), opener: "orch".into(), form: json!({"title": "old work"}), text: None, closed: Some(closing) });
    assert_eq!(out.reply, Ok("Imported I-143, closed.".into()));
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::Deliver { .. } | Effect::Notify { .. } | Effect::Ask { .. })), "{out:?}");
    assert!(matches!(r.status(143), ViewStatus::Done { .. }));
    assert!(r.m.due("orch").is_empty());
    // an open one: its hold is asked for, and delivered to nobody
    let out = r.go(Input::Import { t: 150, kind: "item".into(), opener: "dev".into(), form: json!({"title": "new work", "note": "vault/items/I-150.md"}), text: None, closed: None });
    assert_eq!(out.reply, Ok("Opened I-150.".into()));
    r.go(Input::Decided { t: 150, step: 1, question: "fills".into(), answer: json!("orch"), provenance: json!({}) }).reply.unwrap();
    assert!(r.m.due("orch").is_empty() && r.m.due("dev").is_empty());
    // a number in use is refused
    assert!(r.go(Input::Import { t: 150, kind: "item".into(), opener: "dev".into(), form: json!({"title": "again"}), text: None, closed: None }).reply.is_err());
    // started: once per agent, on an open item only
    r.go(Input::Started { t: 150, by: "dev".into() }).reply.unwrap();
    let n = r.log.len();
    r.go(Input::Started { t: 150, by: "dev".into() }).reply.unwrap();
    assert_eq!(r.log.len(), n);
    assert!(r.go(Input::Started { t: 143, by: "dev".into() }).reply.is_err());
    assert_eq!(r.m.view(150).unwrap().started, vec!["dev".to_string()]);
    // tickets opened after go on above the imported numbers, and the log replays
    let c = r.go(Input::Open { kind: "claim".into(), under: None, opener: "DEBUG".into(), form: json!({"what": "w", "evidence": "e", "where": "p", "branch": "main"}), text: None, names: vec![], told: vec![] });
    assert_eq!(c.reply, Ok("Opened C-200.".into()));
    r.replays();
}

/// An item closed while its vault note still says open is reopened (orch m14290, paper-research m1518): one line;
/// its close is held by the lead again, as when it was opened. Nothing but an item reopens.
#[test]
fn a_closed_item_is_reopened_and_held_again() {
    let mut r = Run::new();
    let i = r.open_item("faster replay");
    r.go(Input::Decided { t: i, step: 1, question: "fills".into(), answer: json!("orch"), provenance: json!({}) }).reply.unwrap();
    r.go(Input::Answer { t: i, by: "orch".into(), head: json!("Done"), text: Text::New("landed".into()) }).reply.unwrap();
    assert!(matches!(r.status(i), ViewStatus::Done { .. }));
    assert!(r.go(Input::Started { t: i, by: "dev".into() }).reply.is_err(), "closed");
    let out = r.go(Input::Reopen { t: i, by: "dev".into() });
    assert_eq!(out.reply, Ok(format!("{} is open again.", r.m.id(i))));
    assert!(matches!(out.lines.first(), Some(Line::Reopen { .. })), "{:?}", out.lines);
    assert!(r.m.tickets[&i].is_open());
    // its close is asked for again, and held by whoever policy says; nobody is handed anything
    let step = r.m.tickets[&i].steps;
    r.go(Input::Decided { t: i, step, question: "fills".into(), answer: json!("orch"), provenance: json!({}) }).reply.unwrap();
    let ViewStatus::Awaits { agent, hold, .. } = r.status(i) else { panic!("{:?}", r.status(i)) };
    assert_eq!((agent.as_str(), hold), ("orch", true));
    assert!(r.m.due("orch").is_empty());
    r.go(Input::Started { t: i, by: "dev".into() }).reply.unwrap();
    assert!(r.go(Input::Reopen { t: i, by: "dev".into() }).reply.is_err(), "open already");
    // a thread or a note stays closed
    let t = r.say("lead", &["lead"], "the parser");
    r.cancel(t, "owner", "done").reply.unwrap();
    assert!(r.go(Input::Reopen { t, by: "owner".into() }).reply.unwrap_err().contains("only an item"));
    r.replays();
}

/// An agent that starts an item is asked first what already does part of it (item.kind's `when act`, seek.call's
/// `seek`; paper-research N-1396.p79, consult-3 p189). The start goes to the item as an act, `is` "start", and its run
/// is raised by the starter: the step is the starter's own (`raised`, from it), with the title, and the note as where.
/// Its answer stands on the item as the starter's post. A second start by it asks nothing; another agent's start asks
/// that one; the lead's close still closes the item, with a seek still open, and that seek's answer comes after it.
#[test]
fn an_items_starter_is_asked_first_what_already_does_part_of_it() {
    let mut r = Run::new();
    let out = r.go(Input::Open { kind: "item".into(), under: None, opener: "orch".into(), form: json!({"title": "faster replay", "note": "vault/items/I-200.md"}), text: None, names: vec![], told: vec![] });
    let i = parse_ticket(out.reply.unwrap().trim_start_matches("Opened ").trim_end_matches('.')).unwrap();
    r.go(Input::Decided { t: i, step: 1, question: "fills".into(), answer: json!("orch"), provenance: json!({}) }).reply.unwrap();
    let out = r.go(Input::Started { t: i, by: "dev".into() });
    assert_eq!(out.reply, Ok(format!("dev works on {}.", r.m.id(i))));
    assert!(out.effects.contains(&Effect::Deliver { to: "dev".into() }), "{out:?}");
    // waiting for dev: its own step, from it
    let w = r.m.waiting("dev");
    assert_eq!(w.iter().map(|i| (i["what"].as_str(), i["from"].as_str(), i["raised"].as_bool())).collect::<Vec<_>>(), [(Some("step"), Some("dev"), Some(true))], "{w:?}");
    assert!(r.m.waiting("orch").is_empty(), "the lead's hold is never delivered");
    let d = r.take("dev");
    let What::Step { call, args, .. } = &d.what else { panic!("{d:?}") };
    assert_eq!((call.as_str(), &args["what"], &args["where"]), ("seek", &json!("faster replay"), &json!("vault/items/I-200.md")));
    let c = d.t;
    assert!(c >= HIDDEN && r.m.tickets[&c].under == Some(i) && r.m.tickets[&c].raised_by.as_deref() == Some("dev"), "{:?}", r.m.tickets[&c]);
    // its answer: dev's post on the item; the run is done, and the lead's hold stands
    r.answer(c, "dev", json!({"head": "Extends", "pieces": "replay at machine.rs:8379, made to skip what it has read"}));
    assert!(!r.m.tickets[&c].is_open());
    let p = r.m.view(i).unwrap().posts.last().cloned().unwrap();
    assert_eq!((p.by.as_str(), p.text), ("dev", Text::New("Already does part of it, to extend: replay at machine.rs:8379, made to skip what it has read".into())));
    assert!(matches!(r.status(i), ViewStatus::Awaits { agent, hold: true, .. } if agent == "orch"), "{:?}", r.status(i));
    // a second start by dev: nothing
    let n = r.log.len();
    assert_eq!(r.go(Input::Started { t: i, by: "dev".into() }).reply, Ok(format!("dev works on {} already.", r.m.id(i))));
    assert_eq!(r.log.len(), n);
    assert!(r.m.due("dev").is_empty());
    // another agent's start asks it; its Nothing stands as its post
    r.go(Input::Started { t: i, by: "w1".into() }).reply.unwrap();
    let d = r.take("w1");
    r.answer(d.t, "w1", json!({"head": "Nothing", "searched": "replay and skip in docket-machine"}));
    let p = r.m.view(i).unwrap().posts.last().cloned().unwrap();
    assert_eq!((p.by.as_str(), p.text), ("w1", Text::New("Nothing does part of it yet. Searched: replay and skip in docket-machine".into())));
    // w2 starts it and the lead closes it before w2 answers: the item closes; the seek is a hidden run, no child of
    // the item, so it stays w2's, and w2's answer stands on the closed item as its post
    r.go(Input::Started { t: i, by: "w2".into() }).reply.unwrap();
    let d = r.take("w2");
    r.answer(i, "orch", json!({"head": "Done", "why": "landed"}));
    assert_eq!(r.status(i), ViewStatus::Done { result: json!({"head": "Done", "why": "landed", "by": null}) });
    assert!(r.m.tickets[&d.t].is_open());
    r.answer(d.t, "w2", json!({"head": "Nothing", "searched": "replay in docket-machine"}));
    assert!(!r.m.tickets[&d.t].is_open());
    let p = r.m.view(i).unwrap().posts.last().cloned().unwrap();
    assert_eq!((p.by.as_str(), p.text), ("w2", Text::New("Nothing does part of it yet. Searched: replay in docket-machine".into())));
    assert!(r.m.due("w2").is_empty(), "{:?}", r.m.due("w2"));
    assert_eq!(r.m.view(i).unwrap().started, ["dev", "w1", "w2"]);
    r.replays();
}

/// An item opened before its `when act` (testdata/item-before-seek.kind) keeps its program: started today it asks
/// nothing, and its record replays under today's kinds; one opened today asks.
#[test]
fn an_item_opened_before_seek_asks_nothing_at_its_start() {
    let mut old = Run { m: Machine::load(defs_before(&[("item.kind", include_str!("../testdata/item-before-seek.kind"))])), log: Vec::new(), clock: 0 };
    let i = old.open_item("old work");
    old.go(Input::Decided { t: i, step: 1, question: "fills".into(), answer: json!("orch"), provenance: json!({}) }).reply.unwrap();
    let mut r = Run { m: Machine::load(defs()), log: old.log.clone(), clock: old.clock };
    assert_eq!(r.m.replay(&old.log), Vec::<String>::new());
    let out = r.go(Input::Started { t: i, by: "dev".into() });
    assert_eq!(out.reply, Ok(format!("dev works on {}.", r.m.id(i))));
    assert!(r.m.due("dev").is_empty() && !r.m.tickets.range(HIDDEN..).any(|(_, x)| x.under == Some(i)));
    let j = r.open_item("new work");
    r.go(Input::Decided { t: j, step: 1, question: "fills".into(), answer: json!("orch"), provenance: json!({}) }).reply.unwrap();
    r.go(Input::Started { t: j, by: "dev".into() }).reply.unwrap();
    assert_eq!(r.m.due("dev").len(), 1);
    r.replays();
}

/// A layered `during` whose fresh agent starts an item while it owes the during's step (consult-3 N-1396 p264, 2).
const LAID_WORK: [(&str, &str); 1] = [(
    "laid.kind",
    "kind laid\n  letter P\n  form task: Text\n  result Text\n  pattern\n    u <- layer {\n      let agent a in {\n        r <- during {\n          step a \"Do {{ task }}\" -> Done(c: Text)\n        } when act(u) where u.by == a and u.is == \"check\" do {\n          post u.text\n        };\n        match r { Done(c) => return c }\n      }\n    };\n    return u\n",
)];

/// The step an agent's own act raised gives way to the work it was on (consult-3 N-1396 p264, 2): a fresh agent owes
/// a layered `during`'s step and starts an item. Its seek is its own and due, but `owing` gives the work: the agent's
/// check lands on the work's ticket, where the during's `when act` reads it, and its layer is the work's. Once the
/// work is answered, `owing` gives the seek.
#[test]
fn an_agents_acts_stay_with_its_work_while_the_seek_its_start_raised_is_open() {
    let (d, errors) = defs_with(&LAID_WORK);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    r.open_form("laid", json!({"task": "median"}));
    let a = "f200-1";
    let work = r.take(a);
    assert_eq!((work.t, r.m.layer_of(work.t)), (200, Some("P-200.l1".to_string())));
    let i = r.open_item("faster replay");
    r.go(Input::Decided { t: i, step: 1, question: "fills".into(), answer: json!("orch"), provenance: json!({}) }).reply.unwrap();
    r.go(Input::Started { t: i, by: a.into() }).reply.unwrap();
    let seek = r.m.due(a);
    assert!(matches!(&seek[..], [s] if matches!(&s.what, What::Step { call, .. } if call == "seek")), "{seek:?}");
    let seek = seek[0].t;
    assert!(r.m.tickets[&seek].raised_for(a) && !r.m.tickets[&200].raised_for(a));
    assert_eq!(r.m.owing(a), Some("P-200".to_string()), "the work, not the seek above every ticket's id");
    // its check: on the work's ticket, where the during's `when act` posts what it said
    r.go(Input::Act { agent: a.into(), is: "check".into(), text: "quick a:layer/P-200.l1 green".into(), more: Default::default(), t: None }).reply.unwrap();
    let p = r.m.view(200).unwrap().posts.last().cloned().expect("the look's post");
    assert_eq!(p.text, Text::New("quick a:layer/P-200.l1 green".into()));
    // the work answered: the seek is what it owes now, and it owes it still
    r.answer(200, a, json!({"head": "Done", "c": "built"}));
    assert_eq!(r.m.owing(a), Some(r.m.id(seek)));
    r.replays_with(&LAID_WORK);
}

/// A step an agent's own act raised, put into its running turn and left there (consult-3 N-1396 p280): no turn's end
/// answers it as the oldest taken step owed, neither that turn's nor a later one's about something else. It comes
/// again, once, as its reminder (its why `RemindWhy::Left`), and the reminder's own turn answers it. The turn that held
/// only the seek is kept on the record, so the record replays with the reminder. No reading read its answer as none of
/// its outcomes, so its view has none (`read_none`, the picture's chip; consult-3 N-1396 p300, 1).
#[test]
fn a_seek_left_in_the_turn_it_was_put_into_comes_again_and_only_its_own_turn_answers_it() {
    let mut r = Run::new();
    let i = r.open_item("faster replay");
    r.go(Input::Decided { t: i, step: 1, question: "fills".into(), answer: json!("orch"), provenance: json!({}) }).reply.unwrap();
    r.go(Input::Started { t: i, by: "dev".into() }).reply.unwrap();
    let d = r.m.due("dev").remove(0);
    let c = d.t;
    r.go(Input::GivenInto { t: c, to: "dev".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Run::loc(1)) });
    // the turn began with nothing of its own (a background job's notice): its end answers nothing, and is kept
    let out = r.go(Input::TurnEnded { agent: "dev".into(), delivery: None, text: Run::loc(2), later: vec![], stood: None, go: None });
    assert!(out.lines.iter().any(|l| matches!(l, Line::TurnEnded { .. })), "{out:?}");
    assert!(out.lines.iter().any(|l| matches!(l, Line::Reminded { t, step: 1, .. } if *t == c)), "{out:?}");
    assert!(out.effects.contains(&Effect::Deliver { to: "dev".into() }), "{out:?}");
    let again = r.m.due("dev");
    assert!(matches!(&again[..], [a] if a.id == format!("{}.again", d.id) && matches!(&a.what, What::Step { again: true, left: true, why: None, .. })), "{again:?}");
    assert!(matches!(r.status(c), ViewStatus::Awaits { read_none: None, .. }), "{:?}", r.status(c));
    // a turn about something else before the reminder is given: it does not answer the seek
    r.go(Input::TurnEnded { agent: "dev".into(), delivery: None, text: Run::loc(3), later: vec![], stood: None, go: None });
    assert!(matches!(&r.m.tickets[&c].status, Status::Awaits { taken: true, forming: None, .. }), "{:?}", r.m.tickets[&c].status);
    // the reminder's own turn answers it
    let a = r.take("dev");
    assert!(matches!(r.status(c), ViewStatus::Awaits { read_none: None, .. }), "given again, still no reading: {:?}", r.status(c));
    r.go(Input::TurnEnded { agent: "dev".into(), delivery: Some(a.id.clone()), text: Run::loc(4), later: vec![], stood: None, go: None });
    assert!(matches!(&r.m.tickets[&c].status, Status::Awaits { forming: Some(_), .. }), "{:?}", r.m.tickets[&c].status);
    r.go(Input::Formed { t: c, step: 1, head: Some(json!({"head": "Nothing", "searched": "replay in docket-machine"})), why: None, not_taken: None, call: None });
    assert!(!r.m.tickets[&c].is_open());
    let p = r.m.view(i).unwrap().posts.last().cloned().unwrap();
    assert_eq!((p.by.as_str(), p.text), ("dev", Text::New("Nothing does part of it yet. Searched: replay in docket-machine".into())));
    r.replays();
}

/// A step no act of the agent's raised, put into a turn all the same (`free`'s `next` handing just as a turn began:
/// consult-3 N-1396 p300, 2): the one rule for every step put in. The turn began with a post, and its last message is
/// that post's reply; the step is not answered by the turn's end, not even as the oldest taken step owed, but comes
/// again (`RemindWhy::Left`), and its own turn answers it.
#[test]
fn a_step_put_into_a_turn_begun_with_a_post_is_left_there_and_the_post_has_the_reply() {
    let mut r = Run::new();
    let c = r.open_claim();
    let n = r.note("lead", "dev", "is the build green?");
    let p = r.give("dev", n);
    r.decide(c, 1, "dev");
    let d = r.m.due("dev").into_iter().find(|d| d.t == c).expect("the claim's step");
    r.go(Input::GivenInto { t: c, to: "dev".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Run::loc(1)) });
    let out = r.turn("dev", Some(&p), 2);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Reminded { t, step: 1, .. } if *t == c)), "{out:?}");
    let x = &r.m.tickets[&n];
    assert_eq!((x.posts.len(), x.posts.last().map(|q| q.by.as_str())), (2, Some("dev")), "the post has the reply");
    assert!(matches!(&r.m.tickets[&c].status, Status::Awaits { taken: true, forming: None, reminded: Some(false), .. }), "{:?}", r.m.tickets[&c].status);
    assert!(matches!(r.status(c), ViewStatus::Awaits { read_none: None, .. }), "{:?}", r.status(c));
    let again = r.m.due("dev");
    assert!(matches!(&again[..], [a] if a.id == format!("{}.again", d.id) && matches!(&a.what, What::Step { again: true, left: true, why: None, .. })), "{again:?}");
    let a = r.take("dev");
    r.turn("dev", Some(&a.id), 3);
    assert!(matches!(&r.m.tickets[&c].status, Status::Awaits { forming: Some(_), .. }), "its own turn answers it: {:?}", r.m.tickets[&c].status);
    r.replays();
}

/// A seek put into a turn that is cut (its process exited, or a new session began: `TurnCut`; consult-3 N-1396 p300,
/// 3): it comes again as at a turn's end without its answer, and the next turn, about something else, does not answer
/// it; the reminder then goes alone.
#[test]
fn a_seek_put_into_a_turn_that_is_cut_comes_again_and_the_next_turn_does_not_answer_it() {
    let mut r = Run::new();
    let i = r.open_item("faster replay");
    r.go(Input::Decided { t: i, step: 1, question: "fills".into(), answer: json!("orch"), provenance: json!({}) }).reply.unwrap();
    r.go(Input::Started { t: i, by: "dev".into() }).reply.unwrap();
    let d = r.m.due("dev").remove(0);
    let c = d.t;
    r.go(Input::GivenInto { t: c, to: "dev".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Run::loc(1)) });
    let out = r.go(Input::TurnCut { agent: "dev".into() });
    assert!(out.lines.iter().any(|l| matches!(l, Line::TurnCut { .. })), "{out:?}");
    assert!(out.lines.iter().any(|l| matches!(l, Line::Reminded { t, step: 1, .. } if *t == c)), "{out:?}");
    assert!(out.effects.contains(&Effect::Deliver { to: "dev".into() }), "{out:?}");
    // the next turn, about a post: its end is the post's reply, not the seek's answer
    let n = r.note("lead", "dev", "is the build green?");
    let p = r.give("dev", n);
    r.turn("dev", Some(&p), 2);
    let x = &r.m.tickets[&n];
    assert_eq!((x.posts.len(), x.posts.last().map(|q| q.by.as_str())), (2, Some("dev")), "the post has the reply");
    assert!(matches!(&r.m.tickets[&c].status, Status::Awaits { taken: true, forming: None, .. }), "{:?}", r.m.tickets[&c].status);
    let again = r.m.due("dev");
    assert!(matches!(&again[..], [a] if a.id == format!("{}.again", d.id) && matches!(&a.what, What::Step { again: true, left: true, .. })), "{again:?}");
    r.replays();
}

/// A post's id has one parse (`post_id`, consult-3 N-1397.p74; `post_ref` under it, its ticket as written, N-1396
/// p280): its number all digits, so a sign, a space or none names no post, wherever one is read (`Ticket::also_here`,
/// an `AnswersAlso`'s ids, the rule Reply's posts, coordd's `--re`, its star, labels, mentions and quote replies, the
/// client's requests, `place_of`).
#[test]
fn a_posts_id_is_its_ticket_and_a_number_of_digits_alone() {
    assert_eq!(post_id("N-412.p7"), Some((412, 7)));
    assert_eq!(post_id("n-412.p30"), Some((412, 30)));
    assert_eq!(post_ref("n-412.p30"), Some(("n-412", 30)));
    assert_eq!(post_ref(".p3"), Some(("", 3)), "the ticket is the reader's to check");
    assert_eq!(post_id(".p3"), None);
    for id in ["N-412.p+5", "N-412.p 5", "N-412.p", "N-412.p-5", "N-412.s3", "N-412.p5.taken_back", "N-412", "N-412.p99999999999"] {
        assert_eq!((post_id(id), post_ref(id)), (None, None), "{id}");
    }
    // a place: a post, or a ticket alone; anything else with a `.` names none (the star, a quote reply, `place_of`)
    assert_eq!(post_or_ticket("N-412.p7"), Some(("N-412", Some(7))));
    assert_eq!(post_or_ticket("N-412"), Some(("N-412", None)));
    for id in ["N-412.p+5", "N-412.p", "N-412.s3", "N-412.p5.taken_back"] {
        assert_eq!(post_or_ticket(id), None, "{id}");
    }
}

/// A step's delivery has one parse beside it (`step_id`, consult-3 N-1396 p280): the step itself, its reminder, its
/// give-back and a refusal of an answer to it give the step, and say which; a withdrawal's notice, a post and a number
/// not all digits give none. So `s1` is not `s12` (`answered_by_turn` read `s12` as step 1 by its first letters).
#[test]
fn a_steps_delivery_is_its_ticket_and_step_and_a_withdrawal_is_not_one() {
    for (id, step, itself) in [("N-412.s3", 3, true), ("N-412.s3.again", 3, false), ("C-12.s1.back", 1, false), ("N-412.s12.refused2", 12, false), ("A-1000004.s1", 1, true), ("N-412.s12", 12, true)] {
        assert_eq!(step_id(id).map(|(_, s, i)| (s, i)), Some((step, itself)), "{id}");
    }
    assert_eq!(step_id("N-412.s3").map(|(t, _, _)| t), Some(412));
    for id in ["N-412.s3.withdrawn", "N-412.p3", "N-412.s+3", "N-412.s", "N-412.s3.refused", "N-412.s3.refusedx", "N-412.s3.later", "N-412"] {
        assert_eq!(step_id(id), None, "{id}");
    }
}

#[test]
fn withdraw_puts_a_program_where_the_step_was() {
    let mut r = Run::new();
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    r.take("orch");
    let w = |r: &mut Run, p: &str| r.go(Input::Withdraw { t: c, by: "owner".into(), program: p.into(), why: "orch is away".into() });
    // refused, and not logged: a name not in scope, a value of the wrong type, a program that does not parse
    let n = r.log.len();
    let e = w(&mut r, "step dev assess(what, evidence, nowhere)").reply.unwrap_err();
    assert!(e.contains("nowhere") && e.contains("in scope: "), "{e}");
    assert!(w(&mut r, "return \"yes\"").reply.is_err());
    assert!(w(&mut r, "step dev (").reply.is_err());
    assert_eq!(r.log.len(), n);
    // another agent takes the step, with the step's own arguments
    assert_eq!(w(&mut r, "step dev assess(what, evidence, where)").reply, Ok("Step 1 of C-200 withdrawn.".into()));
    let d = r.take("orch");
    assert_eq!(d.what, What::Withdrawn { step: 1, call: "assess".into(), by: Some("owner".into()), why: Some("orch is away".into()) });
    let What::Step { call, args, .. } = r.take("dev").what else { panic!() };
    assert_eq!((call.as_str(), args["where"].clone()), ("assess", json!("path:agda/x.agda:201")));
    // and a value: the step is answered by the program itself
    w(&mut r, "return Dismiss(\"a duplicate of C-12\")").reply.unwrap();
    let ViewStatus::Done { result } = r.status(c) else { panic!("{:?}", r.status(c)) };
    assert_eq!(result, json!({"head": "Dismissed", "why": "a duplicate of C-12"}));
    r.replays();
}

#[test]
fn a_team_request_waits_on_its_approver() {
    let mut r = Run::new();
    let out = r.go(Input::Open { kind: "request".into(), under: None, opener: "dev".into(), form: json!({"who": "dev", "approver": "orch", "why": "three parsers at once"}), text: None, names: vec![], told: vec![] });
    let t = parse_ticket(out.reply.unwrap().trim_start_matches("Opened ").trim_end_matches('.')).unwrap();
    assert_eq!(r.m.id(t), format!("R-{t}"));
    let What::Step { call, args, .. } = r.take("orch").what else { panic!() };
    assert_eq!((call.as_str(), args["who"].clone(), args["n"].clone()), ("approve", json!("dev"), Value::Null));
    r.answer(t, "orch", json!({"head": "Granted", "n": 3}));
    let ViewStatus::Done { result } = r.status(t) else { panic!() };
    assert_eq!(result, json!({"head": "Granted", "n": 3}));
    assert_eq!(r.take("dev").id, format!("R-{t}.c"));
    r.replays();
}

#[test]
fn a_ticket_whose_goal_does_not_hold_fails_as_owed() {
    let k = "kind guarded\n  result Text\n  goal green(where)\n  pattern\n    r <- step x triage(\"w\", \"e\", \"p\");\n    return \"done\"\n";
    let (d, errors) = defs_with(&[("guarded.kind", k)]);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(d.kinds["guarded"].goals, vec!["green".to_string()]);
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    for (unmet, want) in [(json!([]), "done"), (json!(["green"]), "Owed")] {
        let out = r.go(Input::Open { kind: "guarded".into(), under: None, opener: "DEBUG".into(), form: json!({}), text: None, names: vec![], told: vec![] });
        let t = parse_ticket(out.reply.unwrap().trim_start_matches("Opened ").trim_end_matches('.')).unwrap();
        r.take("x");
        let out = r.go(Input::Answer { t, by: "x".into(), head: json!("Accept"), text: Text::New(String::new()) });
        assert!(out.effects.contains(&Effect::Ask { t, step: 1, question: Question::Holds { goals: vec!["green".into()] } }), "{out:?}");
        assert!(r.m.view(t).unwrap().closed_at.is_none());
        assert_eq!(r.m.resume_effects().iter().filter(|e| matches!(e, Effect::Ask { step: 1, .. })).count(), 1);
        // an answer to another ask (an earlier close's) is refused
        assert!(r.go(Input::Decided { t, step: 2, question: "holds".into(), answer: unmet.clone(), provenance: json!({}) }).reply.is_err());
        r.go(Input::Decided { t, step: 1, question: "holds".into(), answer: unmet, provenance: json!({}) }).reply.unwrap();
        match r.status(t) {
            ViewStatus::Done { result } => assert_eq!(result, json!(want)),
            ViewStatus::Failed { raised } => assert_eq!(raised.exc, want),
            s => panic!("{s:?}"),
        }
    }
    // a goal must be a name
    let (_, errors) = defs_with(&[("bad.kind", "kind bad\n  result ()\n  goal 3 > 2\n  pattern\n    return ()\n")]);
    assert!(errors.iter().any(|e| e.contains("a goal is a name")), "{errors:?}");
}

#[test]
fn a_failed_ticket_is_repaired_with_a_new_program() {
    // a claim that fails when nobody fills its lead: the claim kind before N-1978 p2
    let mut r = Run { m: Machine::load(defs_before_noworker()), log: Vec::new(), clock: 0 };
    let c = r.open_claim();
    r.go(Input::Decided { t: c, step: 1, question: "fills".into(), answer: Value::Null, provenance: json!({}) }).reply.unwrap();
    assert!(matches!(r.status(c), ViewStatus::Failed { .. }));
    assert_eq!(r.take("DEBUG").id, "C-200.c");
    let rp = |r: &mut Run, p: &str| r.go(Input::Repair { t: c, by: "owner".into(), program: p.into(), why: "no lead today".into() });
    // a program of the wrong type is refused, unlogged
    let n = r.log.len();
    let e = rp(&mut r, "return \"fine\"").reply.unwrap_err();
    assert!(e.contains("in scope: what: Text"), "{e}");
    assert_eq!(r.log.len(), n);
    // a program of the kind's type: it runs on, and the opener hears of its close again
    rp(&mut r, "v <- step orch assess(what, evidence, where);\n match v { Dismiss(why) => return Dismissed(why)  Accept(n) => return Dismissed(\"accepted, but no fixer\")  Decide(q) => return Dismissed(q) }").reply.unwrap();
    assert!(matches!(r.status(c), ViewStatus::Awaits { .. }));
    r.take("orch");
    r.answer(c, "orch", json!({"head": "Dismiss", "why": "not ours"}));
    let ViewStatus::Done { result } = r.status(c) else { panic!("{:?}", r.status(c)) };
    assert_eq!(result, json!({"head": "Dismissed", "why": "not ours"}));
    // its second close has a notice of its own
    assert_eq!(r.take("DEBUG").id, "C-200.c2");
    // a ticket that has not failed is not repaired
    assert!(rp(&mut r, "return Dismissed(\"x\")").reply.is_err());
    r.replays();
}

#[test]
fn a_note_becomes_a_claim_and_its_posts_carry_over() {
    let mut r = Run::new();
    let out = r.go(Input::Open { kind: "thread".into(), under: None, opener: "dev".into(), form: json!({"to": "dev", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "dev"}, "follows": null}), text: Some(Text::New("@orch the agda load fails again".into())), names: vec!["orch".into()], told: vec![] });
    let t = parse_ticket(out.reply.unwrap().trim_start_matches("Opened ").trim_end_matches('.')).unwrap();
    assert!(matches!(r.status(t), ViewStatus::Done { .. }));
    let out = r.go(Input::Rekind { t, by: "orch".into(), kind: "claim".into(), form: json!({"what": "load fails", "evidence": "the note", "where": "path:x", "branch": "main"}), why: "it is a defect".into() });
    assert_eq!(out.reply, Ok(format!("N-{t} is a claim now: C-{t}.")));
    assert!(matches!(r.status(t), ViewStatus::Asking { .. }));
    r.decide(t, 1, "orch");
    // the note's post comes with the first step (orch had been given it with the note already)
    let What::Step { call, .. } = r.take("orch").what else { panic!() };
    assert_eq!(call, "assess");
    assert_eq!(r.m.view(t).unwrap().posts.len(), 1);
    r.replays();
}

#[test]
fn a_ticket_its_parent_waits_for_keeps_its_promise_when_rekinded() {
    let mut r = Run::new();
    r.accept_and_repair();
    // F-201 waits on its reviewers; a decision returns something else
    let e = r.go(Input::Rekind { t: 201, by: "owner".into(), kind: "decision".into(), form: json!({"question": "q"}), why: "w".into() }).reply.unwrap_err();
    assert!(e.contains("C-200 waits for F-201"), "{e}");
}

#[test]
fn an_edit_changes_what_happens_with_the_answer() {
    // the claim kind before N-1978 p2, whose first step is a plain `step` (today's is in a `try`)
    let mut r = Run { m: Machine::load(defs_before_noworker()), log: Vec::new(), clock: 0 };
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    r.take("orch");
    let k = r.m.view(c).unwrap().continuation.unwrap();
    assert!(k.starts_with("// awaits orch: assess(...), step 1\n// in scope:"), "{k}");
    assert!(k.contains("\nanswer <- (in flight);\ntry {\n  verdict <- return answer;\n"), "{k}");
    // the printed continuation, after its hole, is a program an edit takes as it is
    let rest = k.split_once("answer <- (in flight);\n").unwrap().1.to_string();
    let e = |r: &mut Run, p: &str| r.go(Input::Edit { t: c, by: "owner".into(), program: p.into(), why: "today".into() });
    e(&mut r, &rest).reply.unwrap();
    // one that does not fit the kind's result is refused, unlogged
    let n = r.log.len();
    let no = e(&mut r, "return answer").reply.unwrap_err();
    assert!(no.contains("answer: Accept"), "{no}");
    assert_eq!(r.log.len(), n);
    // a new program for the answer; the step in flight is left alone
    e(&mut r, "match answer {\n  Accept(n) => return Dismissed(\"accepted, but not fixed now\")\n  Dismiss(why) => return Dismissed(why)\n  Decide(q) => return Dismissed(q)\n}").reply.unwrap();
    assert!(r.m.due("orch").is_empty());
    assert!(matches!(r.status(c), ViewStatus::Awaits { .. }));
    r.answer(c, "orch", json!({"head": "Accept"}));
    let ViewStatus::Done { result } = r.status(c) else { panic!("{:?}", r.status(c)) };
    assert_eq!(result, json!({"head": "Dismissed", "why": "accepted, but not fixed now"}));
    r.replays();
}

#[test]
fn the_continuation_of_a_round_says_what_an_edit_drops() {
    let mut r = Run::new();
    r.accept_and_repair();
    let k = r.m.view(201).unwrap().continuation.unwrap();
    assert!(k.contains("up to 2 more round(s) of this follow, which an\n// edit drops unless it writes the loop again"), "{k}");
}

#[test]
fn a_let_agent_lives_for_its_scope() {
    // two steps by the same agent, which an answer does not end; a caught raise inside leaves it alive; leaving the
    // scope ends it, and the step after goes elsewhere
    let k = "kind design\n  result Text\n  pattern\n    a <- let agent p in {\n      v <- step p triage(\"w\", \"e\", \"r\");\n      w <- try { u <- step p triage(\"w2\", \"e\", \"r\"); match u { Accept(n) => raise Nope(\"n\"), Dismiss(y) => return y } } handle { Nope(z) => return z };\n      u <- step p triage(w, \"e\", \"r\");\n      match u { Accept(n) => return \"kept\", Dismiss(y) => return y }\n    };\n    b <- step x triage(a, \"e\", \"r\");\n    match b { Accept(n) => return a, Dismiss(y) => return y }\n";
    let d = || {
        let (d, errors) = defs_with(&[("design.kind", k)]);
        assert!(errors.is_empty(), "{errors:?}");
        d
    };
    let mut r = Run { m: Machine::load(d()), log: Vec::new(), clock: 0 };
    let out = r.go(Input::Open { kind: "design".into(), under: None, opener: "a".into(), form: json!({}), text: None, names: vec![], told: vec![] });
    assert!(out.effects.contains(&Effect::Start { agent: "f200-1".into(), t: 200, call: "triage".into(), layer: None }), "{out:?}");
    let ans = |r: &mut Run, by: &str, head: Value| r.go(Input::Answer { t: 200, by: by.into(), head, text: Text::New(String::new()) });
    let out = ans(&mut r, "f200-1", json!("Accept"));
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::End { .. })), "{out:?}");
    let k1 = r.m.view(200).unwrap().continuation.unwrap();
    assert!(k1.contains("// (the agent f200-1 of a let agent ends here; an edit ends it now)"), "{k1}");
    assert!(k1.contains("// an edit ends the agent(s) f200-1 of the let agent scope(s) it leaves"), "{k1}");
    let out = ans(&mut r, "f200-1", json!("Accept"));
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::End { .. })), "{out:?}");
    let ViewStatus::Awaits { agent, args, .. } = r.status(200) else { panic!("{:?}", r.status(200)) };
    assert_eq!((agent.as_str(), args["what"].clone()), ("f200-1", json!("n")));
    let out = ans(&mut r, "f200-1", json!("Accept"));
    assert!(out.effects.contains(&Effect::End { agent: "f200-1".into() }), "{out:?}");
    let ViewStatus::Awaits { agent, args, .. } = r.status(200) else { panic!() };
    assert_eq!((agent.as_str(), args["what"].clone()), ("x", json!("kept")));
    ans(&mut r, "x", json!("Accept")).reply.unwrap();
    assert_eq!(r.status(200), ViewStatus::Done { result: json!("kept") });
    let mut m2 = Machine::load(d());
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    // a cancel inside the scope ends it with the ticket
    let mut r = Run { m: Machine::load(d()), log: Vec::new(), clock: 0 };
    r.go(Input::Open { kind: "design".into(), under: None, opener: "a".into(), form: json!({}), text: None, names: vec![], told: vec![] });
    let out = r.cancel(200, "a", "no longer");
    assert!(out.effects.contains(&Effect::End { agent: "f200-1".into() }), "{out:?}");
    // the bound variable is an agent
    let bad = "kind bad\n  result Text\n  pattern\n    let agent p in { return p }\n";
    assert!(defs_with(&[("bad.kind", bad)]).1.iter().any(|e| e.contains("Agent")), "{:?}", defs_with(&[("bad.kind", bad)]).1);
}

#[test]
fn any_takes_the_first_results_and_cancels_the_rest() {
    let br = |w: &str| format!("v <- step {w} triage(\"{w}\", \"e\", \"r\");\n      match v {{ Accept(n) => return \"{w}\", Dismiss(y) => return y }}");
    let k = format!("kind poll\n  result [Text]\n  pattern\n    r <- any 2 of (\n      {},\n      {},\n      {}\n    );\n    return r\n", br("a"), br("b"), br("c"));
    let d = || {
        let (d, errors) = defs_with(&[("poll.kind", k.as_str())]);
        assert!(errors.is_empty(), "{errors:?}");
        d
    };
    let open = |r: &mut Run| r.go(Input::Open { kind: "poll".into(), under: None, opener: "o".into(), form: json!({}), text: None, names: vec![], told: vec![] }).reply.unwrap();
    let ans = |r: &mut Run, t: u64, by: &str, head: Value| r.go(Input::Answer { t, by: by.into(), head, text: Text::New(String::new()) }).reply.unwrap();
    let mut r = Run { m: Machine::load(d()), log: Vec::new(), clock: 0 };
    open(&mut r);
    assert_eq!(r.status(200), ViewStatus::Waits { children: vec!["P-201".into(), "P-202".into(), "P-203".into()] });
    // a was given its step: it is told of the withdrawal
    r.take("a");
    ans(&mut r, 203, "c", json!({"head": "Dismiss", "why": "see"}));
    assert!(matches!(r.status(200), ViewStatus::Waits { .. }));
    ans(&mut r, 202, "b", json!("Accept"));
    // in the order they came
    assert_eq!(r.status(200), ViewStatus::Done { result: json!(["see", "b"]) });
    let ViewStatus::Failed { raised } = r.status(201) else { panic!("{:?}", r.status(201)) };
    assert_eq!(raised.exc, "Cancelled");
    assert!(!r.m.due("a").is_empty());
    let mut m2 = Machine::load(d());
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    // once two can no longer come, the first failure is raised and the rest are cancelled
    let mut r = Run { m: Machine::load(d()), log: Vec::new(), clock: 0 };
    open(&mut r);
    r.go(Input::Expired { t: 202, step: 1, provenance: json!({}) }).reply.unwrap();
    assert!(matches!(r.status(200), ViewStatus::Waits { .. }));
    r.go(Input::Expired { t: 201, step: 1, provenance: json!({}) }).reply.unwrap();
    let ViewStatus::Failed { raised } = r.status(200) else { panic!("{:?}", r.status(200)) };
    assert_eq!(raised.exc, "Silent");
    assert!(matches!(r.status(203), ViewStatus::Failed { .. }));
    // n from 1 to the number of branches; every branch of one type
    let bad = "kind bad\n  result [Text]\n  pattern\n    any 3 of (return \"a\", return \"b\")\n";
    assert!(defs_with(&[("bad.kind", bad)]).1.iter().any(|e| e.contains("any 3 of 2")), "{:?}", defs_with(&[("bad.kind", bad)]).1);
    let bad = "kind bad\n  result [Text]\n  pattern\n    any 1 of (return \"a\", return 2)\n";
    assert!(!defs_with(&[("bad.kind", bad)]).1.is_empty());
}

#[test]
fn bindings_filter_count_and_keep_time() {
    let binds = "on commit(c) where c.branch == \"landed\" and c.big open question(c.range)\nevery 3 of commit(c) where c.branch != \"wip\" open question(\"three\")\nevery 1h open question(\"hourly\")\n";
    let d = || {
        let (mut d, errors) = defs_with(&[("binds.kind", binds)]);
        assert!(errors.is_empty(), "{errors:?}");
        d.bindings.retain(|b| b.kind != "filing");
        d
    };
    let mut m = Machine::load(d());
    let mut log = Vec::new();
    let mut ev = |m: &mut Machine, v: Value, at: &str| {
        let out = m.apply(Input::Event { name: "commit".into(), value: v }, at.into());
        log.extend(out.lines.clone());
        out
    };
    // a `where` that does not hold opens nothing; an every-binding counts what its `where` lets through, logged
    let out = ev(&mut m, json!({"branch": "wip", "big": true, "range": "a..b"}), "2026-11-11T10:00:00Z");
    assert!(out.reply.is_err() && out.lines.is_empty(), "{out:?}");
    let out = ev(&mut m, json!({"branch": "landed", "big": false, "range": "a..b"}), "2026-11-11T10:00:01Z");
    assert_eq!(out.reply, Ok("Counted by every 3 of commit(c) where c.branch != \"wip\" open question(\"three\").".into()));
    assert_eq!(out.lines.len(), 1);
    ev(&mut m, json!({"branch": "x", "big": false, "range": "a..b"}), "2026-11-11T10:00:02Z").reply.unwrap();
    let out = ev(&mut m, json!({"branch": "landed", "big": true, "range": "c..d"}), "2026-11-11T10:00:03Z");
    assert_eq!(out.reply, Ok("Opened Q-200, Q-201.".into()));
    assert_eq!(m.view(200).unwrap().form["ask"], json!("c..d"));
    assert_eq!(m.view(201).unwrap().form["ask"], json!("three"));
    // a clock is due when it never fired, then again once its time has gone; coordd feeds it back as `every`
    assert_eq!(m.clocks_due("2026-11-11T10:00:04Z"), ["every 1h open question(\"hourly\")"]);
    let tick = |m: &mut Machine, at: &str| m.apply(Input::Event { name: "every".into(), value: json!("every 1h open question(\"hourly\")") }, at.into());
    let out = tick(&mut m, "2026-11-11T10:00:04Z");
    assert_eq!(out.reply, Ok("Opened Q-202.".into()));
    log.extend(out.lines);
    assert!(m.clocks_due("2026-11-11T10:59:59Z").is_empty());
    assert!(tick(&mut m, "2026-11-11T10:30:00Z").reply.is_err());
    assert_eq!(m.clocks_due("2026-11-11T11:00:04Z").len(), 1);
    // when it fires next, by the same rule (`clock_next`, a Reach's `clocks`; consult-3 N-1396 p317, 1 and 2): its
    // period after it fired, written as `secs` reads a time; now, once it is due; none for an `on` binding
    let clock = m.defs.bindings.iter().find(|b| b.text.starts_with("every 1h")).unwrap();
    assert_eq!(m.clock_next(clock, "2026-11-11T10:30:00Z").as_deref(), Some("2026-11-11T11:00:04Z"));
    assert_eq!(m.clock_next(clock, "2026-11-11T11:20:00Z").as_deref(), Some("2026-11-11T11:20:00Z"));
    let on = m.defs.bindings.iter().find(|b| b.text.starts_with("on commit")).unwrap();
    assert_eq!(m.clock_next(on, "2026-11-11T10:30:00Z"), None);
    assert_eq!([crate::at_secs(0), crate::at_secs(-1), crate::at_secs(951_868_799)], ["1970-01-01T00:00:00Z", "1969-12-31T23:59:59Z", "2000-02-29T23:59:59Z"]);
    // each ticket names the binding that opened it, and the clocks are a view with when each last fired (0.2 line 26);
    // an `on` or `every N of` binding is no clock
    let named = |m: &Machine, t: u64| m.view(t).unwrap().binding;
    assert_eq!(named(&m, 200).as_deref(), Some("on commit(c) where c.branch == \"landed\" and c.big open question(c.range)"));
    assert_eq!(named(&m, 201).as_deref(), Some("every 3 of commit(c) where c.branch != \"wip\" open question(\"three\")"));
    assert_eq!(named(&m, 202).as_deref(), Some("every 1h open question(\"hourly\")"));
    let clocks = m.clock_views();
    assert_eq!(clocks.len(), 1, "{clocks:?}");
    let c = &clocks[0];
    assert_eq!((c.binding.as_str(), c.kind.as_str(), c.every, c.when.clone(), c.by.clone(), c.file.as_str(), c.last.as_deref()), ("every 1h open question(\"hourly\")", "question", Some(3600), None, None, "binds.kind", Some("2026-11-11T10:00:04Z")));
    let mut v = m.view(202).unwrap();
    assert!(serde_json::to_value(&v).unwrap()["binding"].is_string());
    v.binding = None;
    assert!(serde_json::to_value(&v).unwrap().get("binding").is_none(), "sent only when it has one");
    // whose own program a file is: kinds/agents/NAME.kind
    assert_eq!(crate::view::written_by("/p/kinds/agents/paper-research.kind").as_deref(), Some("paper-research"));
    assert_eq!((crate::view::written_by("/p/kinds/binds.kind"), crate::view::written_by("binds.kind"), crate::view::written_by("/p/agents/x.kind")), (None, None, None));
    // the count and the clock come back from the log, and with them the binding each ticket names
    let mut m2 = Machine::load(d());
    assert_eq!(m2.replay(&log), Vec::<String>::new());
    assert_eq!((m2.counts.clone(), m2.clocks.clone()), (m.counts.clone(), m.clocks.clone()));
    assert_eq!(m2.clock_views(), m.clock_views());
    assert_eq!((200..=202).map(|t| named(&m2, t)).collect::<Vec<_>>(), (200..=202).map(|t| named(&m, t)).collect::<Vec<_>>());
    // counted since it last fired: the third fired, so it counts afresh (N-1444 p16)
    assert_eq!(m2.counts.values().copied().collect::<Vec<_>>(), [0]);
}

/// An `every N of` binding counts since it last fired and fires at N or more (N-1444 p16, consult-3 p6): N written,
/// or the counted event's own field, which may change between two firings; with `last.F or …` the last ticket it fired,
/// closed done with F, sets the next N, and without it a result that carries the same field sets nothing (proto-lead
/// N-1446 p57: the engine knows no kind's field by name); an event that carries no N counts and fires nothing; a
/// replay gives the same.
#[test]
fn a_count_binding_fires_at_n_since_it_last_fired_with_n_from_its_event_or_its_last_ticket() {
    let waits = "kind waits\n  form agent: Agent\n  result ()\n  pattern\n    v <- step agent intake(\"look\", agent);\n    return ()\n";
    let brainy = "kind brainy\n  form agent: Agent\n  result Checked(next_check_in: Int)\n  pattern\n    return Checked(3)\n";
    let w3 = "every last.next_check_in or e.every of turn_end(e) where e.agent == \"w3\" open brainy(e.agent)";
    let w4 = "every e.every of turn_end(e) where e.agent == \"w4\" open brainy(e.agent)";
    let binds = format!("every e.every of turn_end(e) where e.agent == \"w1\" open waits(e.agent)\n{w3}\n{w4}\n");
    let d = || {
        let (mut d, errors) = defs_with(&[("waits.kind", waits), ("brainy.kind", brainy), ("binds.kind", &binds)]);
        assert!(errors.is_empty(), "{errors:?}");
        d.bindings.retain(|b| b.ev == syntax::TURN_END);
        d
    };
    let mut m = Machine::load(d());
    assert!(m.bound(syntax::TURN_END) && !m.bound(syntax::OWNER_TO));
    let mut log = Vec::new();
    let mut end = |m: &mut Machine, agent: &str, every: Option<u64>| {
        let mut v = json!({"agent": agent, "text": "done"});
        if let Some(n) = every {
            v["every"] = json!(n);
        }
        let before = m.tickets.len();
        let out = m.apply(Input::Event { name: syntax::TURN_END.into(), value: v }, "2026-11-20T20:30:00Z".into());
        log.extend(out.lines.clone());
        (out, m.tickets.len() - before)
    };
    // N moves from 2 to 4 at count 2: the new N holds, so it fires at 4, not at 2 as a modulus would
    assert_eq!(end(&mut m, "w1", Some(2)).1, 0);
    assert_eq!(end(&mut m, "w1", Some(4)).1, 0, "2 of the new 4");
    assert_eq!(end(&mut m, "w1", Some(4)).1, 0);
    assert_eq!(end(&mut m, "w1", Some(4)).1, 1, "4 since it last fired");
    let w1 = "every e.every of turn_end(e) where e.agent == \"w1\" open waits(e.agent)";
    assert_eq!(m.counts[w1], 0, "it counts afresh once it fired");
    // N moves down to 1 below the count: it fires at once; an event with no N counts and fires nothing
    let (out, opened) = end(&mut m, "w1", None);
    assert_eq!((opened, out.lines.len(), m.counts[w1]), (0, 1, 1), "{out:?}");
    assert_eq!(end(&mut m, "w1", Some(1)).1, 1);
    // another agent's turn: its `where` does not hold, nothing is counted and no line written
    let (out, opened) = end(&mut m, "w2", Some(1));
    assert!(out.reply.is_err() && out.lines.is_empty() && opened == 0, "{out:?}");
    // `last.next_check_in or e.every`: before it fired, the event's N; its ticket closed done with next_check_in 3,
    // and that is the next N, whatever the event carries
    let t3 = m.defs.bindings.iter().find(|b| b.text == w3).map(|b| b.every.clone());
    assert_eq!(t3, Some(Every::Count(syntax::Times::Or(Box::new(syntax::Times::Last { f: "next_check_in".into() }), Box::new(syntax::Times::Field { x: "e".into(), f: "every".into() })))));
    assert_eq!(end(&mut m, "w3", Some(1)).1, 1);
    let last = m.tickets.values().rev().find(|x| x.kind == "brainy").unwrap();
    assert!(matches!(&last.status, Status::Done(r) if r["next_check_in"] == json!(3)), "{:?}", last.status);
    assert_eq!(end(&mut m, "w3", Some(1)).1, 0);
    assert_eq!(end(&mut m, "w3", Some(1)).1, 0);
    assert_eq!(end(&mut m, "w3", Some(1)).1, 1, "3, as its last ticket set it");
    // with no `last.`, a result that carries the same field paces nothing: N is the event's, 1, each time
    for _ in 0..3 {
        assert_eq!(end(&mut m, "w4", Some(1)).1, 1);
    }
    // a replay counts and opens the same, from the lines alone
    let mut m2 = Machine::load(d());
    assert_eq!(m2.replay(&log), Vec::<String>::new());
    assert_eq!(m2.counts, m.counts);
    assert_eq!(m2.tickets.keys().collect::<Vec<_>>(), m.tickets.keys().collect::<Vec<_>>());
    // N is a field of the event the binding counts, named by its own name, or of `last`, which no event is named
    let bad = "every x.every of turn_end(e) open waits(e.agent)\n";
    let (_, errors) = defs_with(&[("waits.kind", waits), ("bad.kind", bad)]);
    assert!(errors.iter().any(|e| e.contains("N is a field of the event counted, `e.every`")), "{errors:?}");
    let bad = "every last.every or 2 of turn_end(last) open waits(last.agent)\n";
    let (_, errors) = defs_with(&[("waits.kind", waits), ("bad.kind", bad)]);
    assert!(errors.iter().any(|e| e.contains("`last` is the last ticket this binding fired")), "{errors:?}");
}

#[test]
fn a_branch_is_edited_withdrawn_and_repaired_against_its_type() {
    let k = "kind two\n  result (Text, Text)\n  pattern\n    par(\n      v <- step a triage(\"w\", \"e\", \"r\"); match v { Accept(n) => return \"a\", Dismiss(y) => return y },\n      v <- step b triage(\"w\", \"e\", \"r\"); match v { Accept(n) => return \"b\", Dismiss(y) => return y }\n    )\n";
    let (d, errors) = defs_with(&[("two.kind", k)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut m = Machine::load(d);
    m.apply(Input::Open { kind: "two".into(), under: None, opener: "o".into(), form: json!({}), text: None, names: vec![], told: vec![] }, "t".into()).reply.unwrap();
    let edit = |m: &mut Machine, p: &str| m.apply(Input::Edit { t: 201, by: "owner".into(), program: p.into(), why: "w".into() }, "t".into()).reply;
    // a branch returns its own type, not the kind's
    assert!(edit(&mut m, "return 3").unwrap_err().contains("Text"));
    edit(&mut m, "return \"edited\"").unwrap();
    m.apply(Input::Withdraw { t: 201, by: "owner".into(), program: "return Accept(none)".into(), why: "w".into() }, "t".into()).reply.unwrap();
    assert_eq!(m.view(201).unwrap().status, ViewStatus::Done { result: json!("edited") });
    // a failed branch is repaired while the other still runs... here the other is done, so its parent took it up
    m.apply(Input::Expired { t: 202, step: 1, provenance: json!({}) }, "t".into()).reply.unwrap();
    assert!(matches!(m.view(200).unwrap().status, ViewStatus::Failed { .. }));
    assert!(m.apply(Input::Repair { t: 202, by: "owner".into(), program: "return \"x\"".into(), why: "w".into() }, "t".into()).reply.unwrap_err().contains("taken up"));
    // and while it still runs, it may be
    let mut m = Machine::load(defs_with(&[("two.kind", k)]).0);
    m.apply(Input::Open { kind: "two".into(), under: None, opener: "o".into(), form: json!({}), text: None, names: vec![], told: vec![] }, "t".into()).reply.unwrap();
    m.apply(Input::Expired { t: 202, step: 1, provenance: json!({}) }, "t".into()).reply.unwrap();
    m.apply(Input::Repair { t: 202, by: "owner".into(), program: "return \"fixed\"".into(), why: "w".into() }, "t".into()).reply.unwrap();
    m.apply(Input::Answer { t: 201, by: "a".into(), head: json!("Accept"), text: Text::New(String::new()) }, "t".into()).reply.unwrap();
    assert_eq!(m.view(200).unwrap().status, ViewStatus::Done { result: json!(["a", "fixed"]) });
}

#[test]
fn any_cancels_a_branch_before_it_runs_and_it_stays_cancelled() {
    let k = "kind quick\n  result [Text]\n  pattern\n    any 1 of (\n      return \"now\",\n      v <- step a triage(\"w\", \"e\", \"r\"); match v { Accept(n) => return \"a\", Dismiss(y) => return y }\n    )\n";
    let (d, errors) = defs_with(&[("quick.kind", k)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut m = Machine::load(d);
    let out = m.apply(Input::Open { kind: "quick".into(), under: None, opener: "o".into(), form: json!({}), text: None, names: vec![], told: vec![] }, "t".into());
    assert_eq!(m.view(200).unwrap().status, ViewStatus::Done { result: json!(["now"]) });
    assert!(matches!(m.view(202).unwrap().status, ViewStatus::Failed { .. }), "{:?}", m.view(202).unwrap().status);
    assert!(m.due("a").is_empty());
    assert_eq!(out.lines.iter().filter(|l| matches!(l, Line::Closed { t: 202, .. })).count(), 1);
}

#[test]
fn a_try_whose_raiser_has_run_is_edited_as_printed() {
    // the second step is inside the try, after the only raise: its continuation keeps an arm nothing left can take
    let k = "kind late\n  result Text\n  pattern\n    try {\n      v <- step a triage(\"w\", \"e\", \"r\");\n      w <- match v { Accept(n) => return \"a\", Dismiss(y) => raise Off(y) };\n      u <- step a triage(w, \"e\", \"r\");\n      return w\n    } handle { Off(z) => return z }\n";
    let (d, errors) = defs_with(&[("late.kind", k)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut m = Machine::load(d);
    m.apply(Input::Open { kind: "late".into(), under: None, opener: "o".into(), form: json!({}), text: None, names: vec![], told: vec![] }, "t".into()).reply.unwrap();
    m.apply(Input::Answer { t: 200, by: "a".into(), head: json!("Accept"), text: Text::New(String::new()) }, "t".into()).reply.unwrap();
    let c = m.view(200).unwrap().continuation.unwrap();
    assert!(c.contains("handle {"), "{c}");
    let rest = c.split_once("answer <- (in flight);\n").unwrap().1.to_string();
    m.apply(Input::Edit { t: 200, by: "owner".into(), program: rest, why: "as printed".into() }, "t".into()).reply.unwrap();
}

#[test]
fn a_kind_may_pad_its_ids() {
    let mut r = Run::new();
    let out = r.go(Input::Import { t: 7, kind: "item".into(), opener: "orch".into(), form: json!({"title": "old"}), text: None, closed: None });
    assert!(out.reply.as_ref().is_ok_and(|s| s.contains("I-007")), "{out:?}");
    assert_eq!(r.m.id(7), "I-007");
    assert_eq!(crate::parse_ticket("I-007"), Some(7));
    let bad = "kind odd\n  letter O x\n  result Text\n  pattern\n    return \"a\"\n";
    assert!(defs_with(&[("odd.kind", bad)]).1.iter().any(|e| e.contains("fewest digits")));
}

#[test]
fn replay_keeps_what_the_log_says_happened() {
    // a claim run to its close, then replayed against calls in which its first step's call is gone
    let mut r = Run::new();
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    r.answer(c, "orch", json!({"head": "Dismiss", "why": "not a defect"}));
    let ViewStatus::Done { result } = r.status(c) else { panic!() };
    let (mut d, _) = defs_with(&[]);
    d.calls.remove("assess");
    let mut m2 = Machine::load(d);
    // a record from before the calls went on it (one that has them runs against its own: tests/calls_recorded.rs)
    let problems = m2.replay(&without_calls(&r.log));
    assert!(!problems.is_empty());
    assert!(!problems.iter().any(|p| p.contains("line 1: the input is now refused")), "{problems:?}");
    // closed as the log says, marked
    let v = m2.view(c).unwrap();
    assert_eq!(v.status, ViewStatus::Done { result });
    assert!(v.broken.is_some());
}

#[test]
fn an_edit_ends_the_let_agents_of_the_scopes_it_leaves() {
    let k = "kind design\n  result Text\n  pattern\n    a <- let agent p in {\n      v <- step p triage(\"w\", \"e\", \"r\");\n      u <- step p triage(\"w2\", \"e\", \"r\");\n      return \"kept\"\n    };\n    return a\n";
    let (d, errors) = defs_with(&[("design.kind", k)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut m = Machine::load(d);
    m.apply(Input::Open { kind: "design".into(), under: None, opener: "o".into(), form: json!({}), text: None, names: vec![], told: vec![] }, "t".into()).reply.unwrap();
    // the agent the step in flight awaits ends once it answers
    let out = m.apply(Input::Edit { t: 200, by: "owner".into(), program: "return \"edited\"".into(), why: "w".into() }, "t".into());
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::End { .. })), "{out:?}");
    let out = m.apply(Input::Answer { t: 200, by: "f200-1".into(), head: json!("Accept"), text: Text::New(String::new()) }, "t".into());
    assert!(out.effects.contains(&Effect::End { agent: "f200-1".into() }), "{out:?}");
    assert_eq!(m.view(200).unwrap().status, ViewStatus::Done { result: json!("edited") });
}

#[test]
fn rekind_leaves_what_was_opened_under_the_ticket() {
    let mut r = Run::new();
    let i = r.open_item("work");
    let q = r.go(Input::Open { kind: "question".into(), under: Some(i), opener: "dev".into(), form: json!({"ask": "which?"}), text: None, names: vec![], told: vec![] });
    q.reply.unwrap();
    let qt = i + 1;
    r.go(Input::Rekind { t: i, by: "owner".into(), kind: "thread".into(), form: json!({"to": "dev", "typed": [], "names": [], "text": "not work", "addressed": {"head": "Named"}, "follows": null}), why: "not work".into() }).reply.unwrap();
    assert!(r.m.tickets[&qt].is_open());
    // a cancel does take them with it
    let mut r = Run::new();
    let i = r.open_item("work");
    r.go(Input::Open { kind: "question".into(), under: Some(i), opener: "dev".into(), form: json!({"ask": "which?"}), text: None, names: vec![], told: vec![] }).reply.unwrap();
    r.cancel(i, "owner", "stale").reply.unwrap();
    assert!(!r.m.tickets[&(i + 1)].is_open());
}

impl Run {
    /// A note from FROM to TO with TEXT; its number.
    fn note(&mut self, from: &str, to: &str, text: &str) -> u64 {
        let out = self.go(Input::Open { kind: "thread".into(), under: None, opener: from.into(), form: json!({"to": from, "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": from}, "follows": null}), text: Some(Text::New(text.into())), names: vec![to.into()], told: vec![] });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        *self.m.tickets.range(..HIDDEN).next_back().unwrap().0
    }
    /// AGENT is handed what is due to it on T (posts), as the surfaces report it; the delivery's id.
    fn give(&mut self, agent: &str, t: u64) -> String {
        let d = self.m.due(agent).into_iter().find(|d| d.t == t).unwrap_or_else(|| panic!("{agent} is owed nothing on {t}"));
        let out = self.go(Input::Given { t, to: agent.into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        d.id
    }
    fn turn(&mut self, agent: &str, delivery: Option<&str>, n: u64) -> Out {
        self.go(Input::TurnEnded { agent: agent.into(), delivery: delivery.map(String::from), text: Run::loc(n), later: vec![], stood: None, go: None })
    }
}

/// The rule Reply names every author of the posts told to the turn on the ticket, once each, in order, who can read a
/// reply (paper-research N-17 post 7, C; N-27 point 1); not the agent itself, not posts it had read before, not the
/// author of a post it was only informed of.
#[test]
fn a_reply_names_every_author_it_was_handed() {
    let mut r = Run::new();
    let n = r.note("owner", "orch", "is the build green?");
    let post = |r: &mut Run, by: &str, text: &str| {
        let out = r.go(Input::Post { t: n, by: by.into(), text: Text::New(text.into()), names: vec!["orch".into()], told: vec![], inform: vec![], answers: None, kind: None });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
    };
    post(&mut r, "dev", "and C-1's too?");
    post(&mut r, "lead", "and quickly");
    // a post orch is only informed of (N-27 point 1): handed, its author not named by the reply
    let out = r.go(Input::Post { t: n, by: "tidy".into(), text: Text::New("FYI dev, C-1 moved".into()), names: vec!["dev".into()], told: vec![], inform: vec!["orch".into()], answers: None, kind: None });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let d = r.give("orch", n);
    assert_eq!(r.m.due("orch").len(), 0);
    let out = r.turn("orch", Some(&d), 5);
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert_eq!(r.m.tickets[&n].posts.last().unwrap().names, ["owner", "dev", "lead"]);
    assert!(["owner", "dev", "lead"].iter().all(|a| out.effects.contains(&Effect::Deliver { to: (*a).into() })), "{:?}", out.effects);
    assert!(!out.effects.contains(&Effect::Deliver { to: "tidy".into() }), "{:?}", out.effects);
    // the next turn: only what is new, and only who can read a reply (a program agent cannot)
    post(&mut r, "runner", "t12 green");
    r.m.readers = Some(["owner", "dev", "orch"].into_iter().map(String::from).collect());
    assert_eq!(r.m.reply_names("orch", &[]), None, "not handed yet");
    r.give("orch", n);
    assert_eq!(r.m.reply_names("orch", &[]), Some((n, vec![])));
    r.m.readers = None;
    assert_eq!(r.m.reply_names("orch", &[]), Some((n, vec!["runner".to_string()])));
    r.replays();
}

/// An agent new to a long thread is handed what reaches it, on its place, and none of the thread's history
/// (paper-research N-1446 p2059: N-1446's 2,057 posts went whole into two new agents' first deliveries, 4 MB each; his
/// N-1104 p912). On a note with 2,000 posts more to orch, tidy names fix, which was never on it: fix is due that post
/// alone, on the note. orch, on it from the start, is due what reaches it since its mark, not tidy's post to fix; and
/// once fix is given its post, so is fix, not the post to orch (an agent with a mark too, N-1446 p2077, p2092).
#[test]
fn an_agent_new_to_a_long_thread_is_handed_the_post_that_reaches_it_on_its_place_alone() {
    let mut r = Run::new();
    let n = r.note("owner", "orch", "is the build green?");
    let post = |r: &mut Run, by: &str, text: String, names: &[&str]| {
        let out = r.go(Input::Post { t: n, by: by.into(), text: Text::New(text), names: names.iter().map(|a| a.to_string()).collect(), told: vec![], inform: vec![], answers: None, kind: None });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        r.m.tickets[&n].posts.last().unwrap().n
    };
    r.give("orch", n);
    for i in 0..2000 {
        post(&mut r, ["dev", "lead"][i % 2], format!("history {i}"), &["orch"]);
    }
    let last = post(&mut r, "tidy", "fix: the tower's disks are yours now".into(), &["fix"]);
    let shown = |r: &Run, agent: &str| -> (String, Vec<u32>) {
        let d = r.m.due(agent).into_iter().find(|d| d.t == n).unwrap_or_else(|| panic!("{agent} is owed nothing on {n}"));
        let What::Posts { unread } = &d.what else { panic!("{:?}", d.what) };
        (d.ticket.clone(), unread.iter().map(|p| p.n).collect())
    };
    let id = r.m.id(n);
    assert_eq!(shown(&r, "fix"), (id.clone(), vec![last]), "new to it: the post that names it, on its place, alone");
    assert_eq!(shown(&r, "orch").1.len(), 2000, "on it from the start: what reaches it since its mark, not the post to fix");
    r.give("fix", n);
    let more = post(&mut r, "dev", "one more, to orch".into(), &["orch"]);
    let named = post(&mut r, "lead", "fix, and this".into(), &["fix"]);
    assert_eq!(shown(&r, "fix"), (id, vec![named]), "on it now: what reaches it since its mark, not the post to orch");
    let _ = more;
    r.replays();
}

#[test]
fn a_note_is_answered_by_the_turn_it_wakes() {
    let mut r = Run::new();
    // the rule Reply: the turn given only the note, answering no step, says its last message there, naming the author
    let n = r.note("owner", "orch", "is the build green?");
    let d = r.give("orch", n);
    assert!(d.ends_with(".p1"), "{d}");
    let out = r.turn("orch", Some(&d), 5);
    assert_eq!(out.reply, Ok(format!("Replied on {}.", format!("{} as post 2", r.m.id(n)))));
    let p = r.m.tickets[&n].posts.last().unwrap().clone();
    assert_eq!((p.n, p.by.as_str(), p.auto, p.names.clone(), p.text), (2, "orch", true, vec!["owner".to_string()], Text::Loc { loc: Run::loc(5) }));
    assert!(out.effects.contains(&Effect::Deliver { to: "owner".into() }), "{:?}", out.effects);
    // told to the owner, and read for nothing: an ask to him is typed (the watcher's `asks` went, N-1479 p247)
    assert!(matches!(out.lines.as_slice(), [Line::TurnEnded { .. }, Line::Replied { n: 2, .. }]), "{:?}", out.lines);
    assert!(r.m.due("owner").iter().any(|d| d.t == n));

    // a reply to a reply names its author too (N-17 post 5): the owner's own reply is explicit, so set up two agents
    let m = r.note("orch", "dev", "can you look at C-1?");
    let d = r.give("dev", m);
    let out = r.turn("dev", Some(&d), 6);
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert_eq!(r.m.tickets[&m].posts.last().unwrap().names, vec!["orch".to_string()]);
    let d = r.give("orch", m);
    let out = r.turn("orch", Some(&d), 7);
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let p = r.m.tickets[&m].posts.last().unwrap();
    assert!(p.auto && p.names == vec!["dev".to_string()] && p.by == "orch", "{p:?}");
    assert!(out.effects.contains(&Effect::Deliver { to: "dev".into() }), "{:?}", out.effects);
    // the chain ends when an agent says Nothing: coordd passes an empty text, nothing is posted, nobody is woken
    let posts = r.m.tickets[&m].posts.len();
    let d = r.give("dev", m);
    let out = r.go(Input::TurnEnded { agent: "dev".into(), delivery: Some(d), text: Loc { len: 0, ..Run::loc(8) }, later: vec![], stood: None, go: None });
    assert!(matches!(out.lines.as_slice(), [Line::TurnEnded { .. }]), "kept, and no reply: {:?}", out.lines);
    assert_eq!(r.m.tickets[&m].posts.len(), posts);
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::Deliver { .. })), "{:?}", out.effects);
    assert!(r.m.due("dev").iter().all(|d| d.t != m) && r.m.due("orch").iter().all(|d| d.t != m));
    // an explicit post always wakes whom it names
    let out = r.go(Input::Post { t: m, by: "dev".into(), text: Text::New("one more thing".into()), names: vec!["orch".into()], told: vec![], inform: vec![], answers: None, kind: None });
    assert!(out.effects.contains(&Effect::Deliver { to: "orch".into() }));

    // and it replays: the replies come again from the turns, never twice
    let mut m2 = Machine::load(defs());
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert_eq!(m2.tickets[&n].posts.len(), 2);
}

#[test]
fn a_turn_that_spoke_itself_or_was_given_more_is_not_replied_for() {
    let mut r = Run::new();
    // the agent posted on the note itself in that turn: nothing more is said (no doubles by construction)
    let n = r.note("owner", "orch", "status?");
    let d = r.give("orch", n);
    r.go(Input::Post { t: n, by: "orch".into(), text: Text::New("green".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None });
    let out = r.turn("orch", Some(&d), 5);
    assert_eq!(out.reply, Err("No step was answered.".into()));
    assert_eq!(r.m.tickets[&n].posts.len(), 2);
    // the line is kept, so what the turn was given is counted afresh from it
    assert!(matches!(out.lines.as_slice(), [Line::TurnEnded { .. }]), "{:?}", out.lines);

    // two things given in one turn: nothing is posted, the agent uses coord post
    let a = r.note("owner", "orch", "first");
    let b = r.note("owner", "orch", "second");
    r.give("orch", a);
    let d = r.give("orch", b);
    let out = r.turn("orch", Some(&d), 6);
    assert_eq!(out.reply, Err("No step was answered.".into()));
    assert_eq!((r.m.tickets[&a].posts.len(), r.m.tickets[&b].posts.len()), (1, 1));
    // the next turn given one note is replied for again
    let c = r.note("owner", "orch", "third");
    let d = r.give("orch", c);
    assert!(r.turn("orch", Some(&d), 7).reply.is_ok());
    assert_eq!(r.m.tickets[&c].posts.len(), 2);

    // an empty turn says nothing
    let e = r.note("owner", "orch", "fourth");
    let d = r.give("orch", e);
    let out = r.go(Input::TurnEnded { agent: "orch".into(), delivery: Some(d), text: Loc { file: "t.jsonl".into(), offset: 0, len: 0 }, later: vec![], stood: None, go: None });
    assert!(out.reply.is_err());
    assert_eq!(r.m.tickets[&e].posts.len(), 1);

    let mut m2 = Machine::load(defs());
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());

    // a step's delivery is answered by the turn (formed), never replied for; nor is its closing notice
    let mut r = Run::new();
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    let d = r.take("orch");
    let out = r.go(Input::TurnEnded { agent: "orch".into(), delivery: Some(d.id.clone()), text: Run::loc(8), later: vec![], stood: None, go: None });
    assert!(matches!(out.lines.as_slice(), [Line::TurnEnded { .. }, Line::Forming { .. }]), "{:?}", out.lines);
    let n = r.note("owner", "orch", "closing soon");
    r.go(Input::Given { t: n, to: "orch".into(), delivery: format!("{}.c", r.m.id(n)), upto: 1, skipped: Default::default(), place: None });
    let out = r.turn("orch", Some(&format!("{}.c", r.m.id(n))), 9);
    assert!(out.reply.is_err() && r.m.tickets[&n].posts.len() == 1, "{:?}", out.reply);
}

#[test]
fn a_cancelled_branch_of_fresh_agents_is_quiet() {
    // live round (findings 14): a part's title is its parent's; a cancelled branch's stopped agent is owed no
    // withdrawal; its answer formed after the close changes nothing and is no error
    let br = "v <- step fresh triage(what, \"e\", \"r\");\n      match v { Accept(n) => return \"yes\", Dismiss(y) => return y }";
    let k = format!("kind poll\n  form what: Text\n  result [Text]\n  pattern\n    r <- any 1 of (\n      {br},\n      {br}\n    );\n    return r\n");
    let (d, errors) = defs_with(&[("poll.kind", k.as_str())]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    r.go(Input::Open { kind: "poll".into(), under: None, opener: "owner".into(), form: json!({"what": "raise or return 0?"}), text: None, names: vec![], told: vec![] }).reply.unwrap();
    assert_eq!(r.m.view(201).unwrap().title, "raise or return 0?");
    let agent = |r: &Run, t: u64| match r.status(t) {
        ViewStatus::Awaits { agent, .. } => agent,
        s => panic!("{s:?}"),
    };
    let (a, b) = (agent(&r, 201), agent(&r, 202));
    r.take(&b);
    let out = r.go(Input::Answer { t: 201, by: a.clone(), head: json!("Accept"), text: Text::New(String::new()) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(matches!(r.status(202), ViewStatus::Failed { .. }));
    assert!(out.effects.contains(&Effect::End { agent: b.clone() }), "{:?}", out.effects);
    assert!(r.m.due(&b).is_empty(), "{:?}", r.m.due(&b));
    let out = r.go(Input::Formed { t: 202, step: 1, head: Some(json!("Accept")), why: None, not_taken: None, call: None });
    assert_eq!((out.reply, out.lines.len()), (Ok(String::new()), 0));
    let mut m2 = Machine::load(defs_with(&[("poll.kind", k.as_str())]).0);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
}

#[test]
fn posts_on_one_ticket_are_one_thing_and_a_cut_turn_is_forgotten() {
    let mut r = Run::new();
    // two deliveries of posts on one note, in one turn: one thing; the reply names the authors of both (N-17 post 7, C)
    let n = r.note("owner", "orch", "first question");
    let d1 = r.give("orch", n);
    r.go(Input::Post { t: n, by: "dev".into(), text: Text::New("and from me".into()), names: vec!["orch".into()], told: vec![], inform: vec![], answers: None, kind: None });
    assert_eq!(r.m.reply_ticket("orch", &[]), Some(n));
    let d2 = r.give("orch", n);
    assert_ne!(d1, d2);
    let out = r.turn("orch", Some(&d1), 5);
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let p = r.m.tickets[&n].posts.last().unwrap();
    assert_eq!((p.by.as_str(), p.auto, p.names.clone()), ("orch", true, vec!["owner".to_string(), "dev".to_string()]));

    // the footer's question: alone, or with a delivery on another ticket, which makes it several
    let a = r.note("owner", "orch", "a");
    let b = r.note("owner", "orch", "b");
    let da = r.m.due("orch").into_iter().find(|d| d.t == a).unwrap().id;
    let db = r.m.due("orch").into_iter().find(|d| d.t == b).unwrap().id;
    assert_eq!(r.m.reply_ticket("orch", std::slice::from_ref(&da)), Some(a));
    assert_eq!(r.m.reply_ticket("orch", &[da.clone(), db.clone()]), None);

    // a turn cut short with a delivery given: forgotten (a line), so the next turn given one note is replied for
    r.give("orch", a);
    let out = r.go(Input::TurnCut { agent: "orch".into() });
    assert!(out.reply.is_ok() && matches!(out.lines.as_slice(), [Line::TurnCut { .. }]), "{out:?}");
    assert!(r.go(Input::TurnCut { agent: "orch".into() }).lines.is_empty(), "nothing to forget: no line");
    let d = r.give("orch", b);
    assert!(r.turn("orch", Some(&d), 6).reply.is_ok());
    assert_eq!(r.m.tickets[&b].posts.len(), 2);
    // a step among the deliveries makes it several
    assert_eq!(r.m.reply_ticket("orch", &["C-1.s1".into()]), None);

    let mut m2 = Machine::load(defs());
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
}

#[test]
fn what_was_given_and_never_read_is_due_again_once_then_said_on_the_ticket() {
    let mut r = Run::new();
    // a step and a note handed, and no turn began (paper-research m1005): both are due again, marked as handed before
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    let s = r.take("orch").id;
    let n = r.note("owner", "orch", "are you there?");
    let p = r.give("orch", n);
    assert!(r.m.due("orch").is_empty());
    let out = r.go(Input::Unread { agent: "orch".into() });
    assert!(out.reply.is_ok() && matches!(out.lines.as_slice(), [Line::Unread { .. }]), "{out:?}");
    let mut due: Vec<String> = r.m.due("orch").into_iter().map(|d| d.id).collect();
    due.sort();
    let mut want = vec![s.clone(), p.clone()];
    want.sort();
    assert_eq!(due, want);
    assert!(r.m.handed_before("orch", &s) && r.m.handed_before("orch", &p));
    assert!(r.m.given_since("orch").is_empty());
    assert!(r.go(Input::Unread { agent: "orch".into() }).lines.is_empty(), "nothing given since: no line");
    // handed again, and again not read: not a third time; the ticket says so, and the step waits for Silent
    r.give("orch", c);
    r.give("orch", n);
    let posts = r.m.tickets[&n].posts.len();
    let out = r.go(Input::Unread { agent: "orch".into() });
    assert!(out.reply.as_ref().is_ok_and(|s| s.contains("given twice")), "{out:?}");
    assert!(r.m.due("orch").is_empty());
    let last = r.m.tickets[&n].posts.last().unwrap();
    assert_eq!((r.m.tickets[&n].posts.len(), last.by.as_str()), (posts + 1, "coordd"));
    assert!(r.m.tickets[&c].posts.last().unwrap().text == Text::New(format!("orch was given {s} twice and did not read it.")));
    // and once it reads it, the step is answered as ever
    r.go(Input::Answer { t: c, by: "orch".into(), head: json!({"head": "Dismiss"}), text: Text::New("not a defect".into()) }).reply.unwrap();

    let mut m2 = Machine::load(defs());
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
}

/// coordd's own post ("AGENT was given D twice and did not read it.") is shown to nobody (paper-research N-412.p1844, 2;
/// consult-lead p1900, 2): it keeps its number, so a record replays the same, but it is in no delivery and not in the
/// view; the view's given rows say both givings were not read, for the line under the post.
#[test]
fn coordds_post_on_a_delivery_given_twice_unread_is_shown_to_nobody_and_its_givings_say_not_read() {
    let mut r = Run::new();
    let n = r.note("owner", "orch", "are you there?");
    let p = r.give("orch", n);
    r.go(Input::Unread { agent: "orch".into() }).reply.unwrap();
    assert_eq!(r.give("orch", n), p, "handed again");
    let out = r.go(Input::Unread { agent: "orch".into() });
    assert!(out.reply.as_ref().is_ok_and(|s| s.contains("given twice")), "{out:?}");
    let x = &r.m.tickets[&n];
    let c = x.posts.last().unwrap();
    assert_eq!((c.by.as_str(), c.n, c.visible()), ("coordd", 2, false), "kept at its number, shown to nobody");
    let v = TicketView::of(&r.m, x);
    assert_eq!(v.posts.iter().map(|q| (q.n, q.by.as_str())).collect::<Vec<_>>(), vec![(1, "owner")], "not in the view");
    let unread = |v: &TicketView| v.given.iter().filter(|g| g.to == "orch" && g.delivery == p).map(|g| g.unread).collect::<Vec<_>>();
    assert_eq!(unread(&v), vec![true, true], "handed twice, not read");
    // his post missed twice waits for orch's next turn's end, never left behind (0.2 A4, consult-3 N-818.p76, 2)
    assert!(r.m.due("orch").is_empty() && r.m.missed("orch"));
    // a post that only informs orch brings nothing along: it waits with them (research's N-1104.p411, 20 Nov 06:04)
    r.go(Input::Post { t: n, by: "tidy".into(), text: Text::New("FYI dev, C-1 moved".into()), names: vec!["dev".into()], told: vec![], inform: vec!["orch".into()], answers: None, kind: None }).reply.unwrap();
    assert!(r.m.due("orch").is_empty() && r.m.missed("orch"), "only informed: still held");
    // its post asks the owner nothing (the watcher's conversion): the owner's next words are not queued behind it
    r.nothing_asked();
    // a later post waking it: numbered after coordd's, and its delivery brings the missed one along
    r.post_on(n, "owner", "still there?", &["orch"]).reply.unwrap();
    // tidy and dev are on the note now: the watcher says whom his words are for
    r.route(json!({"head": "To", "agents": ["orch"]}));
    let d = r.m.due("orch").into_iter().find(|d| d.t == n).unwrap_or_else(|| {
        let posts: Vec<_> = r.m.tickets[&n].posts.iter().map(|p| (p.n, p.by.clone(), p.held.is_some(), p.visible(), p.names.clone(), p.told.clone(), p.inform.clone())).collect();
        let open: Vec<_> = r.m.tickets.values().filter(|x| x.handling.is_some() && x.is_open()).map(|x| (x.t, x.handling.clone(), format!("{:?}", x.status))).collect();
        panic!("the owner's new post is due: due {:?}; posts {posts:?}; handlers {open:?}", r.m.due("orch"))
    });
    match &d.what {
        What::Posts { unread } => assert_eq!(unread.iter().map(|q| (q.n, q.by.as_str())).collect::<Vec<_>>(), vec![(1, "owner"), (3, "tidy"), (4, "owner")]),
        w => panic!("{w:?}"),
    }
    // the row given now is read once the turn it began ends; the two before stay not read
    let p3 = r.give("orch", n);
    r.turn("orch", Some(&p3), 9).reply.unwrap();
    let v = TicketView::of(&r.m, &r.m.tickets[&n]);
    assert_eq!(unread(&v), vec![true, true]);
    assert!(v.given.iter().any(|g| g.delivery == p3 && !g.unread));
    // the replay makes the same, the kept set too
    let mut m2 = Machine::load(defs());
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert_eq!(m2.tickets[&n].unread_twice, r.m.tickets[&n].unread_twice);
    assert_eq!(TicketView::of(&m2, &m2.tickets[&n]).given, v.given);
}

// The owner's message (plan/design-two-systems.md section 3, the first slice): kind thread.

impl Run {
    /// The owner writes TEXT to TO, naming NAMES: the event coordd raises, and the thread it opens.
    fn say(&mut self, to: &str, names: &[&str], text: &str) -> u64 {
        let addressed = if names.is_empty() { "Unnamed" } else { "Named" };
        let out = self.go(Input::Event { name: "owner_message".into(), value: json!({"from": "owner", "to": to, "typed": [to], "names": names, "text": text, "addressed": {"head": addressed}}) });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        *self.m.tickets.range(..HIDDEN).next_back().unwrap().0
    }
    /// The watcher's verdict on thread T, as coordd answers it.
    fn intake(&mut self, t: u64, head: Value) -> Out {
        assert!(matches!(&self.m.tickets[&t].status, Status::Awaits { agent, owed, .. } if agent == "watcher" && owed.call == "intake"), "{:?}", self.m.tickets[&t].status);
        let out = self.go(Input::Answer { t, by: "watcher".into(), head, text: Text::New(String::new()) });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        out
    }
    /// AGENT is handed what is due to it, and its turn ends with an answer at N.
    /// Handed only automatic replies, it ends its turn with Nothing (an empty text), as its footer asks: since N-17
    /// post 5 a chain of replies ends so, not by the rule naming nobody.
    fn handed(&mut self, agent: &str, n: u64) -> Vec<Delivery> {
        let due = self.m.due(agent);
        for d in &due {
            self.go(Input::Given { t: d.t, to: agent.into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Run::loc(n)) });
        }
        let replies_only = !due.is_empty() && due.iter().all(|d| matches!(&d.what, What::Posts { unread } if unread.iter().all(|p| p.auto)));
        let text = if replies_only { Loc { len: 0, ..Run::loc(n + 1) } } else { Run::loc(n + 1) };
        self.go(Input::TurnEnded { agent: agent.into(), delivery: due.first().map(|d| d.id.clone()), text, later: vec![], stood: None, go: None });
        // its reply on a thread asks the owner nothing (the tests of design §16 answer `asks` themselves)
        self.nothing_asked();
        due
    }
    /// Every agent handed only replies ends its turn with Nothing (N-17 post 5: a reply names its author even when
    /// that is a reply too, so a chain ends only when an agent says nothing).
    fn quiet(&mut self) {
        loop {
            let mut agents: Vec<String> = self.m.tickets.keys().copied().collect::<Vec<_>>().into_iter().flat_map(|t| self.m.participants(t)).collect();
            agents.sort();
            agents.dedup();
            let due: Vec<(String, Vec<Delivery>)> = agents.into_iter().filter(|a| a != "owner").map(|a| { let d = self.m.due(&a); (a, d) }).filter(|(_, d)| !d.is_empty() && d.iter().all(|d| matches!(&d.what, What::Posts { unread } if unread.iter().all(|p| p.auto)))).collect();
            if due.is_empty() {
                return;
            }
            for (a, _) in due {
                self.handed(&a, 90);
            }
        }
    }
    /// AGENT is handed what is due to it and its turn ends, its reply's conversion left to the test.
    fn handed_asking(&mut self, agent: &str, n: u64) -> Vec<Delivery> {
        let due = self.m.due(agent);
        for d in &due {
            self.go(Input::Given { t: d.t, to: agent.into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Run::loc(n)) });
        }
        self.go(Input::TurnEnded { agent: agent.into(), delivery: due.first().map(|d| d.id.clone()), text: Run::loc(n + 1), later: vec![], stood: None, go: None });
        due
    }
    /// BY's post on thread T, as coordd sends it with agent-base's notify.eflint (paper-research m1237): the owner's
    /// tells everyone on it; an agent's informs the others.
    fn post_on(&mut self, t: u64, by: &str, text: &str, names: &[&str]) -> Out {
        let on: Vec<String> = self.m.participants(t).into_iter().filter(|a| a != by && a != "owner").collect();
        let (told, inform) = if by == "owner" { (on, vec![]) } else { (vec![], on) };
        self.go(Input::Post { t, by: by.into(), text: Text::New(text.into()), names: names.iter().map(|a| a.to_string()).collect(), told, inform, answers: None, kind: None })
    }
    /// The open ticket running an `owner_post` handler (design-two-systems §13), if any.
    fn handler(&self) -> Option<u64> {
        self.m.tickets.values().find(|x| x.handling.as_deref() == Some("owner_post") && x.is_open()).map(|x| x.t)
    }
    /// The open tickets converting an agent's post (`when agent_post`, design §16), each with the post's number.
    fn converting(&self) -> Vec<(u64, u32)> {
        self.m.tickets.values().filter(|x| x.handling.as_deref() == Some("agent_post") && x.is_open()).map(|x| (x.t, x.form["text"]["post"]["n"].as_u64().unwrap() as u32)).collect()
    }
    /// The watcher's answer to the `asks` step converting post N, as coordd gives it.
    fn asks(&mut self, n: u32, head: Value) -> Out {
        let (h, _) = *self.converting().iter().find(|(_, m)| *m == n).expect("post N is being converted");
        assert!(matches!(&self.m.tickets[&h].status, Status::Awaits { agent, owed, .. } if agent == "watcher" && owed.call == "asks"), "{:?}", self.m.tickets[&h].status);
        let out = self.go(Input::Answer { t: h, by: "watcher".into(), head, text: Text::New(String::new()) });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        out
    }
    /// Every agent's post being converted asks nothing.
    fn nothing_asked(&mut self) {
        for (_, n) in self.converting() {
            self.asks(n, json!({"head": "Nothing"}));
        }
    }
    /// The watcher's answer to the handler's `route` step, as coordd gives it.
    fn route(&mut self, head: Value) -> Out {
        let h = self.handler().expect("a handler waits");
        assert!(matches!(&self.m.tickets[&h].status, Status::Awaits { agent, owed, .. } if agent == "watcher" && owed.call == "route"), "{:?}", self.m.tickets[&h].status);
        let out = self.go(Input::Answer { t: h, by: "watcher".into(), head, text: Text::New(String::new()) });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        out
    }
    /// T, resting at its hole, is put to the minder; nobody fills the role, so it rests as it was.
    fn nobody_minds(&mut self, t: u64) {
        self.nothing_asked();
        self.quiet();
        let Status::Asking { owed, role } = &self.m.tickets[&t].status else { panic!("{:?}", self.m.tickets[&t].status) };
        assert_eq!((role.as_str(), owed.call.as_str()), ("minder", "shape"));
        let step = owed.step;
        self.go(Input::Decided { t, step, question: "fills".into(), answer: Value::Null, provenance: json!({}) }).reply.unwrap();
        assert_eq!(self.status(t), ViewStatus::Resting);
    }
    fn one_off(&mut self, to: &str, text: &str, n: u64) -> u64 {
        let t = self.say(to, &[], text);
        self.intake(t, json!({"head": "OneOff"}));
        self.handed(to, n);
        t
    }
}

#[test]
fn the_thread_kind_checks_and_its_binding() {
    let d = defs();
    assert_eq!(d.bindings.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(), ["on owner_message(m) open thread(m.to, m.typed, m.names, m.text, m.addressed, m.follows)"]);
    // intake only: an unsure intake no longer waits on the owner's pick (N-101.p1285)
    assert_eq!(check::bound(&d.kinds["thread"].pattern, &d.kinds), 1);
}

#[test]
fn a_message_naming_others_is_a_thread_that_rests() {
    let mut r = Run::new();
    let t = r.say("fixer", &["lead"], "let us talk about the parser");
    let x = &r.m.tickets[&t];
    assert_eq!(x.kind, "thread");
    assert_eq!((x.opener.clone(), r.m.view(t).unwrap().opener), (None, Some("owner".to_string())), "shown as the owner's, tells nobody on close");
    let p = &x.posts[0];
    assert_eq!((p.by.as_str(), p.names.clone()), ("owner", vec!["fixer".to_string(), "lead".to_string()]));
    assert_eq!(r.status(t), ViewStatus::Resting);
    assert!(!r.m.open().contains(&t), "resting is not open work");
    assert!(r.log.iter().any(|l| matches!(l, Line::Rests { t: u, .. } if *u == t)));
    // both are given the post; the fixer's answer is its reply there, naming the owner
    let due = r.handed("fixer", 10);
    assert_eq!(due.iter().map(|d| d.id.clone()).collect::<Vec<_>>(), [format!("{}.p1", r.m.id(t))]);
    let x = &r.m.tickets[&t];
    assert_eq!((x.posts.len(), x.posts[1].by.as_str(), x.posts[1].names.clone()), (2, "fixer", vec!["owner".to_string()]));
    // not quiet while the lead holds the post unanswered; its answer makes it so
    assert_eq!(r.status(t), ViewStatus::Resting);
    r.handed("lead", 15);
    r.nobody_minds(t);
    // the owner's post on it tells the others on it (policy's notify)
    let out = r.post_on(t, "owner", "and the lexer", &[]);
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let mut names = r.m.tickets[&t].posts.last().unwrap().told.clone();
    names.sort();
    assert_eq!(names, ["fixer", "lead"]);
    // an edit fills the hole; one that does not give the kind's result is refused
    let e = |r: &mut Run, p: &str| r.go(Input::Edit { t, by: "owner".into(), program: p.into(), why: "w".into() }).reply;
    assert!(e(&mut r, "return 3").is_err());
    assert!(e(&mut r, "return OneOff").is_ok());
    assert_eq!(r.status(t), ViewStatus::Done { result: json!({"head": "OneOff"}) });
    r.replays();
}

#[test]
fn a_one_off_gives_the_words_and_posts_nothing() {
    // there are no one-offs since §16: an old log's, replayed under today's kinds
    let mut r = Run::before_16();
    let t = r.say("fixer", &[], "what is 5+5");
    // nobody is given anything before the verdict
    assert!(r.m.due("fixer").is_empty());
    r.intake(t, json!({"head": "OneOff"}));
    assert_eq!(r.status(t), ViewStatus::Done { result: json!({"head": "OneOff"}) });
    let due = r.m.due("fixer");
    assert_eq!(due.len(), 1, "{due:?}");
    assert_eq!(due[0].id, format!("{}.w1", r.m.id(t)));
    assert!(matches!(&due[0].what, What::Words { text: Text::New(w), from } if w == "what is 5+5" && from == "owner"), "{:?}", due[0].what);
    r.handed("fixer", 10);
    let x = &r.m.tickets[&t];
    assert!(x.posts.is_empty(), "{:?}", x.posts);
    assert_eq!(x.answers, vec![("fixer".to_string(), Run::loc(11))]);
    assert!(r.m.due("fixer").is_empty());
    // given and not read: due again
    let t2 = r.say("fixer", &[], "and 6+6");
    r.intake(t2, json!({"head": "OneOff"}));
    let d = r.m.due("fixer")[0].clone();
    r.go(Input::Given { t: t2, to: "fixer".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None });
    assert!(r.m.due("fixer").is_empty());
    assert!(r.go(Input::Unread { agent: "fixer".into() }).reply.is_ok());
    assert_eq!(r.m.due("fixer").len(), 1);
    r.replays_today();
}

#[test]
fn words_placed_on_a_ticket_are_posted_there_as_the_owners() {
    let mut r = Run::new();
    let a = r.say("fixer", &["lead"], "the parser");
    r.handed("fixer", 10);
    let t = r.say("fixer", &[], "also the lexer");
    let out = r.intake(t, json!({"head": "Place", "ticket": r.m.id(a)}));
    // the lead is on it too, and the words name nobody: the watcher says whom they are for (§13)
    assert!(out.effects.contains(&Effect::Deliver { to: "watcher".into() }), "{:?}", out.effects);
    assert!(r.m.due("fixer").is_empty(), "held");
    let out = r.route(json!({"head": "To", "agents": ["fixer"]}));
    assert!(out.effects.contains(&Effect::Deliver { to: "fixer".into() }), "{:?}", out.effects);
    assert_eq!(r.status(t), ViewStatus::Done { result: json!({"head": "Moved", "to": r.m.id(a)}) });
    let p = r.m.tickets[&a].posts.last().unwrap().clone();
    assert_eq!((p.by.as_str(), p.names.clone(), p.text.clone()), ("owner", vec!["fixer".to_string()], Text::New("also the lexer".into())));
    assert!(r.m.tickets[&t].posts.is_empty());
    r.replays();
}

#[test]
fn a_post_moves_to_another_ticket_or_a_new_thread() {
    let mut r = Run::new();
    let a = r.say("fixer", &["lead"], "the parser");
    r.handed("fixer", 10);
    let b = r.say("lead", &["fixer"], "the release");
    r.handed("fixer", 20);
    // the fixer has read b's post 1 (and replied there); the lead has not read a's
    let out = r.go(Input::Move { t: b, n: 1, to: Some(a), by: "owner".into() });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let (xa, xb) = (&r.m.tickets[&a], &r.m.tickets[&b]);
    assert_eq!(xb.posts[0].moved, Some(r.m.id(a)));
    let m = xa.posts.last().unwrap();
    assert_eq!((m.text.clone(), m.from.clone()), (Text::New("the release".into()), Some(format!("{}.p1", r.m.id(b)))));
    assert!(r.go(Input::Move { t: b, n: 1, to: Some(a), by: "owner".into() }).reply.is_err(), "moved already");
    assert!(r.go(Input::Move { t: a, n: 1, to: Some(a), by: "owner".into() }).reply.is_err(), "to itself");
    // to a thread of its own (paper-research m1166): resting, to the agent the post was addressed to (the first it
    // names), shown as the owner's; the fixer's reply to it goes with it
    assert_eq!(r.m.tickets[&a].posts[1].by, "fixer");
    let out = r.go(Input::Move { t: a, n: 1, to: None, by: "owner".into() });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let c = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    let x = &r.m.tickets[&c];
    assert_eq!((x.kind.as_str(), x.form["to"].as_str()), ("thread", Some("fixer")));
    assert_eq!(r.status(c), ViewStatus::Resting);
    assert_eq!(r.m.view(c).unwrap().opener, Some("owner".into()));
    assert_eq!(x.posts.iter().map(|p| (p.by.clone(), p.from.clone())).collect::<Vec<_>>(), [("owner".into(), Some(format!("{}.p1", r.m.id(a)))), ("fixer".into(), Some(format!("{}.p2", r.m.id(a))))]);
    // a post on it naming nobody, with the fixer and the lead on it: the watcher says whom it is for (§13)
    r.post_on(c, "owner", "more", &[]);
    let out = r.route(json!({"head": "To", "agents": ["fixer"]}));
    assert!(out.effects.contains(&Effect::Deliver { to: "fixer".into() }), "{:?}", out.effects);
    r.replays();
}

#[test]
fn a_run_of_one_offs_becomes_a_thread() {
    // no surface sends Keep since §16: an old log's, replayed under today's kinds
    let mut r = Run::before_16();
    let t1 = r.one_off("fixer", "what is 5+5", 10);
    let other = r.one_off("lead", "hello", 20);
    let t2 = r.one_off("fixer", "and times 3?", 30);
    let t3 = r.one_off("fixer", "thanks", 40);
    // not in order, not one-offs, not to one agent: refused
    assert!(r.go(Input::Keep { ts: vec![t2, t1], by: "owner".into() }).reply.is_err());
    assert!(r.go(Input::Keep { ts: vec![t1, other], by: "owner".into() }).reply.is_err());
    // t1 and t3 are not a run: t2 came between them, to the fixer
    assert!(r.go(Input::Keep { ts: vec![t1, t3], by: "owner".into() }).reply.is_err());
    let out = r.go(Input::Keep { ts: vec![t1, t2], by: "owner".into() });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    r.nobody_minds(t1);
    assert_eq!(r.status(t2), ViewStatus::Done { result: json!({"head": "Moved", "to": r.m.id(t1)}) });
    let x = &r.m.tickets[&t1];
    let shape: Vec<(String, bool)> = x.posts.iter().map(|p| (p.by.clone(), matches!(p.text, Text::Loc { .. }))).collect();
    assert_eq!(shape, [("owner".into(), false), ("fixer".into(), true), ("owner".into(), false), ("fixer".into(), true)]);
    // a record, not news
    assert!(r.m.due("fixer").is_empty() && r.m.due("owner").is_empty());
    // and the thread goes on as one
    let out = r.post_on(t1, "owner", "more", &[]);
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert_eq!(r.m.tickets[&t1].posts.last().unwrap().told, vec!["fixer".to_string()]);
    r.replays_today();
}

#[test]
fn a_program_moves_its_own_post() {
    let k = "kind relay\n  form t: Ticket, w: Agent\n  result ()\n  pattern\n    _ <- post \"for you\" as owner naming w;\n    _ <- move 1 to t;\n    return ()\n";
    let (d, errors) = defs_with(&[("relay.kind", k)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    let a = r.say("lead", &["fixer"], "the release");
    let out = r.go(Input::Open { kind: "relay".into(), under: None, opener: "owner".into(), form: json!({"t": r.m.id(a), "w": "fixer"}), text: None, names: vec!["lead".into()], told: vec![] });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let b = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    assert_eq!(r.m.tickets[&b].posts[0].moved, Some(r.m.id(a)));
    let p = r.m.tickets[&a].posts.last().unwrap();
    assert_eq!((p.text.clone(), p.names.clone()), (Text::New("for you".into()), vec!["fixer".to_string()]));
    // landed there, with the lead on that ticket too: held till the watcher says (§13); then the fixer is given it
    // once, on the ticket it moved to
    let due: Vec<String> = r.m.due("fixer").iter().map(|d| d.id.clone()).collect();
    assert_eq!(due, [format!("{}.p1", r.m.id(a))], "the thread's first post, not the held one");
    r.route(json!({"head": "To", "agents": ["fixer"]}));
    let due: Vec<String> = r.m.due("fixer").iter().map(|d| d.id.clone()).collect();
    assert_eq!(due, [format!("{}.p2", r.m.id(a))]);
}

/// `post e informing a` (an agent's brain, 0.3, brain.kind): the post reaches A at its next delivery, marked as only
/// informing it, and wakes nobody; `naming` beside it still wakes. (The kind closes at once, so it names the lead.) It prints as it parses, replays the same, and an
/// `informing` that is no agent is refused.
#[test]
fn a_program_posts_informing_an_agent_and_wakes_nobody() {
    let k = "kind note_to\n  form w: Agent, l: Agent\n  result ()\n  pattern\n    _ <- post \"for your next turn\" informing w;\n    _ <- post \"for now\" naming l informing w;\n    return ()\n";
    let (d, errors) = defs_with(&[("note_to.kind", k)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    let out = r.go(Input::Open { kind: "note_to".into(), under: None, opener: "owner".into(), form: json!({"w": "fixer", "l": "lead"}), text: None, names: vec!["lead".into()], told: vec![] });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let t = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    let id = r.m.id(t);
    let posts: Vec<(String, Vec<String>, Vec<String>, Vec<String>)> = r.m.tickets[&t].posts.iter().filter(|p| p.head.is_none()).map(|p| (p.by.clone(), p.names.clone(), p.inform.clone(), p.wakes().cloned().collect())).collect();
    let (fixer, lead) = (vec!["fixer".to_string()], vec!["lead".to_string()]);
    assert_eq!(posts, [(id.clone(), vec![], fixer.clone(), vec![]), (id.clone(), lead.clone(), fixer.clone(), lead.clone())], "by the ticket; the fixer only informed");
    assert!(r.m.tickets[&t].posts.iter().filter(|p| p.head.is_none()).all(|p| p.into_turn == fixer), "its words ask into the fixer's turn, the turn it is about (proto-reader N-1446 p112)");
    let due = r.m.due("fixer");
    assert!(!due.is_empty() && due.iter().all(|d| d.inform), "{due:?}");
    assert!(r.m.due("lead").iter().any(|d| d.t == t && !d.inform));
    // into the turn of whom its words inform, by name: policy informing dev of the same post (`Notified`) does not
    // put it into dev's turn, nor does the lead it names (proto-reader N-1446 p177, consult-3 p181)
    let n = r.m.tickets[&t].posts.iter().filter(|p| p.head.is_none()).last().unwrap().n;
    let out = r.go(Input::Notified { t, event: "post".into(), to: vec![], inform: vec!["dev".into()], post: Some(n), provenance: json!({}) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(r.m.tickets[&t].posts.iter().find(|p| p.n == n).unwrap().inform.contains(&"dev".to_string()));
    let into = |a: &str| r.m.waiting(a).into_iter().find(|i| i["id"] == json!(format!("{id}.p{n}"))).map(|i| i["into_turn"].clone());
    assert_eq!((into("fixer"), into("dev"), into("lead")), (Some(json!(true)), Some(json!(false)), Some(json!(false))));
    // it prints as it parses
    let once = print::prog(&r.m.defs.kinds["note_to"].pattern, 0);
    assert!(once.contains("post \"for now\" naming l informing w"), "{once}");
    assert_eq!(print::prog(&syntax::parse_prog(&once, 1).unwrap(), 0), once);
    r.replays();
    // an agent, or a list of them
    let bad = "kind note_bad\n  form w: Agent\n  result ()\n  pattern\n    _ <- post \"x\" informing 3;\n    return ()\n";
    let (_, errors) = defs_with(&[("note_bad.kind", bad)]);
    assert!(errors.iter().any(|e| e.contains("informing") || e.contains("Agent")), "{errors:?}");
}

/// N-36 point 2: no ticket stops a turn. An `esc` key in the owner's message is read past, so neither the verdict's
/// give, a post naming the agent, nor a post the watcher routes stops anyone; `coord esc` (Input::Esc) is the one
/// thing that does, and it changes no ticket.
#[test]
fn no_ticket_stops_a_turn_only_coord_esc_does() {
    let stops = |out: &Out| out.effects.iter().any(|e| matches!(e, Effect::Interrupt { .. }));
    let mut r = Run::new();
    let out = r.go(Input::Event { name: "owner_message".into(), value: json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": [], "text": "the median is wrong", "addressed": {"head": "Unnamed"}, "esc": ["lead", "fixer"]}) });
    assert!(out.reply.is_ok() && !stops(&out), "{out:?}");
    let t = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    let out = r.intake(t, json!({"head": "Thread"}));
    assert!(!stops(&out), "the verdict's give: {:?}", out.effects);
    assert!(r.woken("lead", t), "the words still reach it, at its turn's end");
    r.handed("lead", 10);
    r.go(Input::Post { t, by: "lead".into(), text: Text::New("@fixer take the median".into()), names: vec!["fixer".into()], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
    r.handed("fixer", 20);
    r.handed("lead", 30);
    r.nobody_minds(t);
    // the owner's post on that thread, routed to both later (`tell`): nobody stopped
    let out = r.owner_says(t, &["lead"], "both of you, stop");
    assert!(!stops(&out), "{:?}", out.effects);
    let out = r.route(json!({"head": "To", "agents": ["lead", "fixer"]}));
    assert!(!stops(&out), "a routed post told later: {:?}", out.effects);
    assert!(r.woken("lead", t) && r.woken("fixer", t));
    // named in the message itself: no stop either
    let out = r.go(Input::Event { name: "owner_message".into(), value: json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": ["fixer"], "text": "both of you", "addressed": {"head": "Named"}, "esc": ["lead", "fixer"]}) });
    assert!(!stops(&out), "{:?}", out.effects);
    // coord esc: one Interrupt, one line on the record, no ticket changed
    let views: Vec<_> = r.m.tickets.keys().map(|t| r.m.view(*t)).collect();
    let out = r.go(Input::Esc { by: "owner".into(), agent: "fixer".into(), also: vec![], all: false });
    assert_eq!(out.effects, vec![Effect::Interrupt { agent: "fixer".into() }]);
    assert!(matches!(out.lines.as_slice(), [Line::Esc { by, agent, .. }] if by == "owner" && agent == "fixer"), "{:?}", out.lines);
    assert_eq!(r.m.tickets.keys().map(|t| r.m.view(*t)).collect::<Vec<_>>(), views);
    // one esc's line is written as before the project-wide act: no `also`, no `all`
    let text = serde_json::to_value(&out.lines[0]).unwrap();
    assert!(text.get("also").is_none() && text.get("all").is_none(), "{text}");
    r.replays();
}

/// Line 200 (the owner, 10 Nov: "i reallyt need them to stop"; N-1324): the project-wide esc is one line on the record
/// for the press, naming every agent that was in a turn, and one Interrupt for each; no ticket changes. The line keeps
/// the `esc` kind, so an engine from before `also` and `all` reads it as its first agent's esc: its fields are only
/// added, never required.
#[test]
fn esc_all_is_one_line_and_an_interrupt_for_each() {
    let mut r = Run::new();
    r.go(Input::Event { name: "owner_message".into(), value: json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": [], "text": "the median is wrong", "addressed": {"head": "Unnamed"}, "esc": []}) }).reply.unwrap();
    let views: Vec<_> = r.m.tickets.keys().map(|t| r.m.view(*t)).collect();
    let out = r.go(Input::Esc { by: "owner".into(), agent: "fixer".into(), also: vec!["lead".into(), "w3".into()], all: true });
    assert_eq!(out.effects, ["fixer", "lead", "w3"].map(|a| Effect::Interrupt { agent: a.into() }).to_vec());
    assert!(matches!(out.lines.as_slice(), [Line::Esc { by, agent, also, all: true, .. }] if by == "owner" && agent == "fixer" && also == &["lead", "w3"]), "{:?}", out.lines);
    assert_eq!(out.reply.as_deref(), Ok("The turns of fixer, lead, w3 are being stopped by owner."));
    assert_eq!(r.m.tickets.keys().map(|t| r.m.view(*t)).collect::<Vec<_>>(), views);
    // the line as an engine from before the two fields has it (792b4207's `Line::Esc`): it reads, the first agent's
    #[derive(serde::Deserialize)]
    #[serde(tag = "ev", rename_all = "snake_case")]
    enum Before {
        Esc { at: At, by: String, agent: String },
    }
    let text = serde_json::to_string(&out.lines[0]).unwrap();
    assert!(text.contains(r#""also":["lead","w3"]"#) && text.contains(r#""all":true"#), "{text}");
    let Before::Esc { at, by, agent } = serde_json::from_str::<Before>(&text).unwrap();
    assert!(!at.is_empty() && by == "owner" && agent == "fixer");
    r.replays();
}

impl Run {
    /// Each agent on thread T is handed what is due to it there and answers, turns from N on: T is quiet.
    fn settle(&mut self, n: u64) {
        for (i, a) in ["fixer", "lead"].into_iter().enumerate() {
            if !self.m.due(a).is_empty() {
                self.handed(a, n + 2 * i as u64);
            }
        }
        self.quiet();
    }
    /// T is out with the minder: the role is filled by "minder", which is handed the shape step and answers HEAD.
    fn minder(&mut self, t: u64, head: Value) -> Out {
        self.nothing_asked();
        self.quiet();
        let Status::Asking { owed, role } = &self.m.tickets[&t].status else { panic!("{:?}", self.m.tickets[&t].status) };
        assert_eq!((role.as_str(), owed.call.as_str()), ("minder", "shape"));
        let step = owed.step;
        self.decide(t, step, "minder");
        let d = self.take("minder");
        assert!(matches!(&d.what, What::Step { call, .. } if call == "shape"), "{:?}", d.what);
        let out = self.go(Input::Answer { t, by: "minder".into(), head, text: Text::New("my answer".into()) });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        out
    }
    fn asking_minder(&self, t: u64) -> bool {
        matches!(&self.m.tickets[&t].status, Status::Asking { role, .. } if role == "minder")
    }
}

#[test]
fn the_minder_shapes_a_resting_thread_and_the_owner_undoes_it() {
    let mut r = Run::new();
    let t = r.say("fixer", &["lead"], "I want to design this: add median(xs)");
    // not asked on arrival
    assert_eq!(r.status(t), ViewStatus::Resting);
    let v = r.m.view(t).unwrap();
    assert_eq!(v.hole.map(|h| h.keys().cloned().collect::<Vec<_>>()), Some(vec!["addressed".into(), "follows".into(), "label".into(), "names".into(), "opener".into(), "text".into(), "to".into(), "typed".into(), "v".into()]));
    // the fixer's reply moves its mark, but the lead still holds the post: not yet
    r.handed("fixer", 10);
    assert_eq!(r.status(t), ViewStatus::Resting);
    // the lead's turn ends without a reply: quiet, and the hole is put to the minder, still showing its scope
    let d = r.m.due("lead");
    r.go(Input::Given { t, to: "lead".into(), delivery: d[0].id.clone(), upto: d[0].upto, skipped: Default::default(), place: Some(Run::loc(12)) });
    r.go(Input::TurnEnded { agent: "lead".into(), delivery: Some(d[0].id.clone()), text: Loc { file: "transcript.jsonl".into(), offset: 1300, len: 0 }, later: vec![], stood: None, go: None });
    assert_eq!(r.m.tickets[&t].posts.len(), 2, "no reply");
    assert!(r.asking_minder(t), "{:?}", r.m.tickets[&t].status);
    assert!(r.m.view(t).unwrap().hole.is_some());
    // the facts beside the posts: fixer replied after the owner, who has not answered it (paper-research m1226)
    assert_eq!(r.m.view(t).unwrap().owner, Some(ViewOwner { spoke_last: false, unanswered: vec!["fixer".into()] }));
    let posts = r.m.tickets[&t].posts.len();
    // Rest: it rests again, and the minder posts nothing
    let out = r.minder(t, json!({"head": "Rest"}));
    assert_eq!(r.status(t), ViewStatus::Resting);
    assert_eq!(r.m.tickets[&t].posts.len(), posts);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Shaped { n: 1, refused: None, .. })), "{:?}", out.lines);
    // what an answer carries is never on the line (N-101 post 31): old records' shaped lines replay as they were
    assert!(out.lines.iter().all(|l| !matches!(l, Line::Shaped { answer, .. } if answer.get("@by").is_some() || answer.get("@text").is_some())), "{:?}", out.lines);
    // never twice for the same state: the minder's own turn moves nothing
    r.go(Input::TurnEnded { agent: "minder".into(), delivery: None, text: Run::loc(30), later: vec![], stood: None, go: None });
    assert_eq!(r.status(t), ViewStatus::Resting);
    // the owner's post: asked again. A program that does not check is refused, kept with its reason, and it rests
    r.post_on(t, "owner", "go on", &[]).reply.unwrap();
    assert!(!r.asking_minder(t), "held: nothing is settled while the watcher says whom it is for");
    r.route(json!({"head": "To", "agents": ["fixer", "lead"]}));
    assert!(!r.asking_minder(t), "the post is the agents' first");
    assert_eq!(r.m.view(t).unwrap().owner, Some(ViewOwner { spoke_last: true, unanswered: vec![] }));
    r.settle(40);
    assert!(r.asking_minder(t));
    r.minder(t, json!({"head": "Program", "text": "return 3"}));
    assert_eq!(r.status(t), ViewStatus::Resting);
    let s = r.m.view(t).unwrap().shaped;
    assert_eq!(s.len(), 2);
    assert!(s[1].refused.as_deref().is_some_and(|w| w.contains("does not check")), "{:?}", s[1]);
    assert!(r.go(Input::Undo { t, by: "owner".into() }).reply.is_err(), "a Rest and a refusal are not undone");
    // a program that checks runs at the hole, in its scope; the hole it ends at is asked as soon as its step is answered
    r.go(Input::Post { t, by: "lead".into(), text: Text::New("a design: sort, take the middle".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
    r.settle(50);
    r.minder(t, json!({"head": "Program", "text": "v <- step to intake(text, to);\n..."}));
    let d = r.take("fixer");
    assert!(matches!(&d.what, What::Step { call, .. } if call == "intake"), "{:?}", d.what);
    r.go(Input::Answer { t, by: "fixer".into(), head: json!({"head": "Thread"}), text: Text::New("it is".into()) }).reply.unwrap();
    assert!(!r.asking_minder(t), "the fixer's turn has not ended");
    r.go(Input::TurnEnded { agent: "fixer".into(), delivery: Some(d.id.clone()), text: Loc { file: "transcript.jsonl".into(), offset: 5500, len: 0 }, later: vec![], stood: None, go: None });
    assert!(r.asking_minder(t), "{:?}", r.m.tickets[&t].status);
    // Close with what does not fit the kind's result: refused; with what does: closed
    r.minder(t, json!({"head": "Close", "result": {"head": "Nope"}}));
    assert_eq!(r.status(t), ViewStatus::Resting);
    r.go(Input::Post { t, by: "owner".into(), text: Text::New("close it".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
    r.route(json!({"head": "To", "agents": ["fixer"]}));
    r.settle(60);
    r.minder(t, json!({"head": "Close", "result": "Kept"}));
    assert_eq!(r.status(t), ViewStatus::Done { result: json!({"head": "Kept"}) });
    // the owner undoes the last answer: it rests at the hole again, the answer marked undone, and not asked for it
    let out = r.go(Input::Undo { t, by: "owner".into() });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert_eq!(r.status(t), ViewStatus::Resting);
    let s = r.m.view(t).unwrap().shaped;
    assert_eq!(s.iter().map(|s| (s.undone, s.refused.is_some())).collect::<Vec<_>>(), [(false, false), (false, true), (false, false), (false, true), (true, false)]);
    // the next undo takes the program back
    assert!(r.go(Input::Undo { t, by: "owner".into() }).reply.is_ok());
    assert_eq!(r.status(t), ViewStatus::Resting);
    r.replays();
}

#[test]
fn an_edit_fills_a_hole_out_with_the_minder() {
    let mut r = Run::new();
    let t = r.say("fixer", &["lead"], "the parser");
    r.settle(10);
    let Status::Asking { owed, .. } = &r.m.tickets[&t].status else { panic!() };
    let step = owed.step;
    r.decide(t, step, "minder");
    r.take("minder");
    let e = |r: &mut Run, p: &str| r.go(Input::Edit { t, by: "owner".into(), program: p.into(), why: "w".into() }).reply;
    assert!(e(&mut r, "return 3").is_err());
    assert!(r.m.tickets[&t].shaping.is_some(), "a refused edit leaves it with the minder");
    assert!(e(&mut r, "return Kept").is_ok());
    assert_eq!(r.status(t), ViewStatus::Done { result: json!({"head": "Kept"}) });
    // the minder is told its step is withdrawn; its late answer is refused
    assert!(r.go(Input::Answer { t, by: "minder".into(), head: json!({"head": "Rest"}), text: Text::New(String::new()) }).reply.is_err());
    r.replays();
}

#[test]
fn a_minders_program_carries_a_call_inline() {
    let mut r = Run::new();
    let t = r.say("fixer", &["lead"], "add median(xs)");
    r.settle(10);
    // a place not in scope is refused, with what is
    r.minder(t, json!({"head": "Program", "text": "v <- step to \"Review {{ commit }}\" -> Fine | Fix(why: Text);\n..."}));
    let s = r.m.view(t).unwrap().shaped;
    assert!(s[0].refused.as_deref().is_some_and(|w| w.contains("`{{ commit }}`") && w.contains("in scope: addressed")), "{:?}", s[0]);
    r.go(Input::Post { t, by: "owner".into(), text: Text::New("review it".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
    r.route(json!({"head": "To", "agents": ["fixer"]}));
    r.settle(20);
    let p = "v <- step to \"Review what {{ to }} was asked: \\\"{{ text }}\\\"\" -> Fine | Fix(why: Text);\nmatch v {\n  Fine => return Kept\n  Fix(why) => ...\n}";
    r.minder(t, json!({"head": "Program", "text": p}));
    let d = r.take("fixer");
    let What::Step { call, template, head, .. } = &d.what else { panic!("{:?}", d.what) };
    assert_eq!((call.as_str(), template.as_deref()), ("inline", Some("Review what fixer was asked: \"add median(xs)\"")));
    assert_eq!(head.to_string(), "Fine | Fix(why: Text)");
    r.go(Input::Answer { t, by: "fixer".into(), head: json!({"head": "Fine"}), text: Text::New("fine".into()) }).reply.unwrap();
    assert_eq!(r.status(t), ViewStatus::Done { result: json!({"head": "Kept"}) });
    r.replays();
    // it prints as it parses
    let once = print::prog(&syntax::parse_prog(p, 1).unwrap(), 0);
    assert_eq!(print::prog(&syntax::parse_prog(&once, 1).unwrap(), 0), once);
    assert!(once.contains("\\\"{{ text }}\\\""), "{once}");
}

impl Run {
    /// A claim accepted, its fix's seek given to dev: dev holds a step of the fix (201).
    fn dev_holds_the_seek(&mut self) {
        self.m.defs.edit().block_ask = true;
        let c = self.open_claim();
        self.decide(c, 1, "orch");
        self.answer(c, "orch", json!({"head": "Accept"}));
        self.decide(201, 1, "dev");
        self.take("dev");
    }
    fn ask(&mut self, by: &str, text: &str) -> Out {
        self.go(Input::Ask { by: by.into(), text: text.into(), context: None })
    }
    /// The question the latest ask opened, asking who fills its lead.
    fn question(&self) -> u64 {
        let q = *self.m.tickets.range(..HIDDEN).next_back().unwrap().0;
        assert_eq!(self.m.tickets[&q].kind, "question");
        q
    }
}

#[test]
fn an_ask_waits_in_its_turn_and_is_answered_there() {
    let mut r = Run::new();
    r.m.defs.edit().block_ask = false;
    assert!(r.ask("dev", "which branch?").reply.is_err(), "block_ask off");
    r.dev_holds_the_seek();
    assert!(r.ask("orch", "and me?").reply.is_err(), "orch holds no step");
    let out = r.ask("dev", "which branch?");
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let a = r.question() - 1;
    let x = &r.m.tickets[&a];
    assert_eq!((x.kind.as_str(), x.under, x.parent), ("ask", Some(201), None));
    assert_eq!(r.m.view(a).unwrap().opener, Some("dev".into()));
    assert!(r.ask("dev", "and another?").reply.is_err(), "one at a time");
    // while it waits, nothing else is handed to dev
    r.go(Input::Post { t: 201, by: "orch".into(), text: Text::New("note this".into()), names: vec!["dev".into()], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
    assert!(r.m.due("dev").is_empty());
    // the lead answers; the answer is dev's command's result, and the post is due after
    let q = r.question();
    r.decide(q, 1, "orch");
    r.take("orch");
    let out = r.go(Input::Answer { t: q, by: "orch".into(), head: json!({"head": "Answer", "text": "main"}), text: Text::New("main".into()) });
    assert!(out.effects.contains(&Effect::Answered { agent: "dev".into(), text: "main".into() }), "{:?}", out.effects);
    assert!(matches!(r.status(a), ViewStatus::Done { .. }), "{:?}", r.status(a));
    assert_eq!(r.m.due("dev").len(), 1);
    // the fix goes on as before
    assert!(matches!(&r.m.tickets[&201].status, Status::Awaits { agent, .. } if agent == "dev"));
    r.replays();
}

#[test]
fn an_ask_its_asker_would_answer_is_passed_up() {
    let mut r = Run::new();
    r.dev_holds_the_seek();
    r.ask("dev", "which branch?").reply.unwrap();
    let q = r.question();
    // policy says dev leads: dev waits on this, so the owner gets it
    let out = r.go(Input::Decided { t: q, step: 1, question: "fills".into(), answer: json!("dev"), provenance: json!({}) });
    assert!(out.lines.iter().any(|l| matches!(l, Line::PassedUp { from, to, why, .. } if from == "dev" && to == "owner" && why.contains("the one asking"))), "{:?}", out.lines);
    // the kind's own way up (paper-research N-101 p1119, 1): the step answered `Up` with that why, so the owner is
    // given the kind's step after `Up` (agent-base's `reply`; the library's runs `consider` before it)
    assert!(matches!(&r.m.tickets[&q].status, Status::Awaits { agent, owed, .. } if agent == "owner" && owed.call == "reply" && owed.args["why"].as_str().is_some_and(|w| w.contains("the one asking"))), "{:?}", r.m.tickets[&q].status);
    r.replays();
}

#[test]
fn an_ask_nobody_takes_goes_to_the_owner() {
    let mut r = Run::new();
    r.dev_holds_the_seek();
    r.ask("dev", "which branch?").reply.unwrap();
    let q = r.question();
    // nobody leads: the question goes to the owner, not back to dev as a NoWorker
    let out = r.go(Input::Decided { t: q, step: 1, question: "fills".into(), answer: json!(""), provenance: json!({}) });
    assert!(out.lines.iter().any(|l| matches!(l, Line::PassedUp { to, why, .. } if to == "owner" && why.contains("nobody fills"))), "{:?}", out.lines);
    assert!(matches!(&r.m.tickets[&q].status, Status::Awaits { agent, owed, .. } if agent == "owner" && owed.call == "reply" && owed.args["why"] == json!("nobody fills lead")), "{:?}", r.m.tickets[&q].status);
    assert!(r.m.due("dev").is_empty(), "{:?}", r.m.due("dev"));
    // the owner's answer reaches dev as the answer to its question
    let out = r.go(Input::Answer { t: q, by: "owner".into(), head: json!({"head": "Answer", "text": "main"}), text: Text::New("main".into()) });
    assert!(out.effects.contains(&Effect::Answered { agent: "dev".into(), text: "main".into() }), "{:?}", out.effects);
    r.replays();
}

/// A new project, its first agent asking (paper-research N-1978 p2): the lead's step is filled but cannot be given
/// (here its model is refused; a lead that cannot start alike), which raises NoWorker. The question passes it up to the
/// owner saying no lead runs, and stops nothing.
#[test]
fn an_ask_whose_lead_cannot_take_it_goes_to_the_owner_saying_no_lead_runs() {
    let mut r = Run::new();
    r.dev_holds_the_seek();
    r.ask("dev", "which branch?").reply.unwrap();
    let q = r.question();
    r.decide(q, 1, "orch");
    r.go(Input::ModelRefused { t: q, step: 1, agent: "orch".into(), why: "orch runs no model the call meets".into() });
    assert!(matches!(&r.m.tickets[&q].status, Status::Awaits { agent, owed, .. } if agent == "owner" && owed.call == "reply" && owed.args["why"] == json!("no lead is running in this project")), "{:?}", r.m.tickets[&q].status);
    assert!(!r.m.tickets.values().any(|x| matches!(x.status, Status::Stopped { .. })), "nothing stopped");
    let out = r.go(Input::Answer { t: q, by: "owner".into(), head: json!({"head": "Answer", "text": "main"}), text: Text::New("main".into()) });
    assert!(out.effects.contains(&Effect::Answered { agent: "dev".into(), text: "main".into() }), "{:?}", out.effects);
    r.replays();
}

/// A claim with no lead to triage it is the owner's to decide (N-1978 p2); one with no fixer ends NoFix, saying so.
#[test]
fn a_claim_with_no_lead_goes_to_the_owner_and_one_with_no_fixer_ends_saying_so() {
    let mut r = Run::new();
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    r.go(Input::ModelRefused { t: c, step: 1, agent: "orch".into(), why: "orch cannot start".into() });
    let d = r.m.tickets.iter().find(|(_, x)| x.kind == "decision").map(|(t, _)| *t).expect("a decision opened");
    assert!(matches!(&r.m.tickets[&d].status, Status::Awaits { agent, owed, .. } if agent == "owner" && owed.args["question"].as_str().is_some_and(|q| q.starts_with("No lead is running in this project"))), "{:?}", r.m.tickets[&d].status);
    assert!(!r.m.tickets.values().any(|x| matches!(x.status, Status::Stopped { .. })), "nothing stopped");
    r.go(Input::Answer { t: d, by: "owner".into(), head: json!({"head": "Yes", "note": "fix it"}), text: Text::New(String::new()) });
    let f = r.m.tickets.iter().find(|(_, x)| x.kind == "fix").map(|(t, _)| *t).expect("the fix opened");
    r.decide(f, 1, "dev");
    r.go(Input::ModelRefused { t: f, step: 1, agent: "dev".into(), why: "dev cannot start".into() });
    assert!(matches!(&r.m.tickets[&c].status, Status::Failed(x) if x.exc == "NoFix" && x.value == json!("no fixer is running in this project")), "{:?}", r.m.tickets[&c].status);
    r.replays();
}

#[test]
fn an_ask_past_its_wait_is_answered_as_words() {
    let mut r = Run::new();
    r.dev_holds_the_seek();
    r.ask("dev", "which branch?").reply.unwrap();
    let q = r.question();
    assert!(r.go(Input::AskLater { by: "dev".into() }).reply.is_ok());
    assert!(r.go(Input::AskLater { by: "dev".into() }).reply.is_err());
    r.decide(q, 1, "orch");
    r.take("orch");
    let out = r.go(Input::Answer { t: q, by: "orch".into(), head: json!({"head": "Dismiss", "why": "ask the owner"}), text: Text::New(String::new()) });
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::Answered { .. })));
    let due = r.m.due("dev");
    assert!(due.iter().any(|d| matches!(&d.what, What::Words { text: Text::New(w), .. } if w == "The answer to your question \"which branch?\": Dismissed: ask the owner")), "{due:?}");
    r.replays();
}

#[test]
fn a_late_ask_keeps_its_step_open_until_its_answer() {
    let mut r = Run::new();
    r.dev_holds_the_seek();
    r.ask("dev", "which branch?").reply.unwrap();
    let q = r.question();
    r.go(Input::AskLater { by: "dev".into() }).reply.unwrap();
    // dev's turn ends while its ask is late: the seek is not answered by it, nor silent, nor reminded
    r.go(Input::TurnEnded { agent: "dev".into(), delivery: Some("F-201.s1".into()), text: Run::loc(30), later: vec![], stood: None, go: None });
    assert!(matches!(&r.m.tickets[&201].status, Status::Awaits { agent, forming: None, reminded: None, .. } if agent == "dev"), "{:?}", r.m.tickets[&201].status);
    assert!(r.go(Input::Expired { t: 201, step: 1, provenance: json!({}) }).reply.is_err());
    // and nothing more of the fix reaches dev meanwhile
    r.go(Input::Post { t: 201, by: "orch".into(), text: Text::New("also check the docs".into()), names: vec!["dev".into()], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
    assert!(r.m.due("dev").is_empty(), "{:?}", r.m.due("dev"));
    // the answer comes as words; the turn they start answers the step
    r.decide(q, 1, "orch");
    r.take("orch");
    r.go(Input::Answer { t: q, by: "orch".into(), head: json!({"head": "Answer", "text": "main"}), text: Text::New(String::new()) }).reply.unwrap();
    // the answer comes with the step's line again (and the post held back)
    let due = r.m.due("dev");
    let ids: Vec<&str> = due.iter().map(|d| d.id.as_str()).collect();
    assert!(ids.contains(&"F-201.s1.again"), "{ids:?}");
    let w = due.iter().find(|d| matches!(d.what, What::Words { .. })).unwrap().clone();
    for d in &due {
        r.go(Input::Given { t: d.t, to: "dev".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Run::loc(31)) });
    }
    let out = r.go(Input::TurnEnded { agent: "dev".into(), delivery: Some(w.id.clone()), text: Run::loc(32), later: vec![], stood: None, go: None });
    assert!(out.effects.iter().any(|e| matches!(e, Effect::Form { t: 201, .. })), "{:?}", out.effects);
    r.replays();
}

#[test]
fn a_cycle_goes_to_the_lead_first() {
    let k = "kind work\n  form w: Agent\n  result ()\n  pattern\n    v <- step w intake(\"do it\", w);\n    return ()\n  when ask(q) do\n    a <- step role fixer answer(q.text, q.context);\n    reply q a\n";
    let (d, errors) = defs_with(&[("work.kind", k)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    r.m.defs.edit().block_ask = true;
    r.go(Input::Open { kind: "work".into(), under: None, opener: "owner".into(), form: json!({"w": "dev"}), text: None, names: vec![], told: vec![] }).reply.unwrap();
    r.take("dev");
    r.ask("dev", "how?").reply.unwrap();
    let a = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    let out = r.go(Input::Decided { t: a, step: 1, question: "fills".into(), answer: json!("dev"), provenance: json!({}) });
    assert!(out.lines.iter().any(|l| matches!(l, Line::PassedUp { to, .. } if to == "lead")), "{:?}", out.lines);
    assert!(out.effects.contains(&Effect::Ask { t: a, step: 1, question: Question::Fills { role: "lead".into(), call: "answer".into() } }));
    r.decide(a, 1, "orch");
    r.take("orch");
    let out = r.go(Input::Answer { t: a, by: "orch".into(), head: json!({"head": "Answer", "text": "carefully"}), text: Text::New(String::new()) });
    assert!(out.effects.contains(&Effect::Answered { agent: "dev".into(), text: "carefully".into() }), "{:?}", out.effects);
    let (d2, _) = defs_with(&[("work.kind", k)]);
    let text: Vec<Line> = r.log.iter().map(|l| serde_json::from_str(&serde_json::to_string(l).unwrap()).unwrap()).collect();
    let mut m2 = Machine::load(d2);
    assert_eq!(m2.replay(&text), Vec::<String>::new());
    assert_eq!(m2.view(a), r.m.view(a));
}

#[test]
fn the_owners_words_on_a_ticket_that_owes_the_owner_a_step_answer_it() {
    let mut r = Run::new();
    let t = r.say("fixer", &["lead"], "I want to design this: add median(xs)");
    r.settle(10);
    r.minder(t, json!({"head": "Program", "text": "ok <- open decision(\"Go ahead with this design?\", none);\nmatch ok {\n  Yes(n) => return Kept\n  No(why) => return OneOff\n}"}));
    let d = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    assert_eq!(r.m.tickets[&d].kind, "decision");
    let posts = r.m.tickets[&t].posts.len();
    // a post of the owner's on the thread: formed against the decision's step first; not an answer, it is a post
    let out = r.go(Input::Post { t, by: "owner".into(), text: Text::New("what about empty lists?".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(out.effects.iter().any(|e| matches!(e, Effect::Form { t: u, agent, words: Some(w), .. } if *u == d && agent == "owner" && w == "what about empty lists?")), "{:?}", out.effects);
    assert_eq!(r.m.tickets[&t].posts.len(), posts, "not posted while it is formed");
    r.go(Input::Formed { t: d, step: 1, head: None, why: None, not_taken: None, call: None }).reply.unwrap();
    let p = r.m.tickets[&t].posts.last().unwrap();
    assert_eq!((r.m.tickets[&t].posts.len(), p.by.as_str(), p.text.clone()), (posts + 1, "owner", Text::New("what about empty lists?".into())));
    assert!(matches!(&r.m.tickets[&d].status, Status::Awaits { agent, .. } if agent == "owner"), "still owed");
    // the owner's plain message to the lead, placed on the thread by the watcher: it is the yes
    let m = r.say("lead", &[], "looks good, go ahead");
    let out = r.intake(m, json!({"head": "Place", "ticket": r.m.id(t)}));
    assert!(out.effects.iter().any(|e| matches!(e, Effect::Form { t: u, words: Some(w), .. } if *u == d && w == "looks good, go ahead")), "{:?}", out.effects);
    r.go(Input::Formed { t: d, step: 1, head: Some(json!({"head": "Yes"})), why: None, not_taken: None, call: None }).reply.unwrap();
    assert!(matches!(r.status(d), ViewStatus::Done { .. }), "{:?}", r.status(d));
    assert_eq!(r.status(t), ViewStatus::Done { result: json!({"head": "Kept"}) });
    assert_eq!(r.m.tickets[&t].posts.len(), posts + 1, "the yes is the decision's answer, not a post on the thread");
    r.replays();
}

/// Slice 2's run, through the machine alone (paper-research m1162): by plain messages to the lead, a design becomes a
/// thread; the minder installs the owner's yes, the fixer's work and the check; the owner's "go ahead" is the yes; the
/// fixer asks one question and gets the answer in its turn; the check is Green and the thread closes. "5+5" stays a
/// one-off.
#[test]
fn the_median_run() {
    let mut r = Run::new();
    r.m.defs.edit().block_ask = true;
    // the owner's message, which the watcher says is a thread: to the lead, resting
    let t = r.say("lead", &[], "I want to design this: add median(xs) to stats");
    r.intake(t, json!({"head": "Thread"}));
    assert_eq!(r.status(t), ViewStatus::Resting);
    // the lead designs (its turn's reply on the thread); quiet then, the minder is asked
    r.handed("lead", 10);
    assert_eq!(r.m.tickets[&t].posts.iter().map(|p| p.by.as_str()).collect::<Vec<_>>(), ["owner", "lead"]);
    let program = "ok <- open decision(\"Go ahead with the lead's design?\", none);
match ok {
  No(why) => return Kept
  Yes(n) =>
    r <- step role fixer \"Implement what the lead designed on this thread for: {{ text }}\" -> Commit(commit: Ref) | Cannot(why: Text);
    match r {
      Cannot(why) => ...
      Commit(c) =>
        g <- open check(c, none);
        match g {
          Green => return Kept
          Red(log) => ...
        }
    }
}";
    r.minder(t, json!({"head": "Program", "text": program}));
    let d = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    assert!(matches!(&r.m.tickets[&d].status, Status::Awaits { agent, .. } if agent == "owner"));
    // the owner's plain "go ahead" to the lead, placed on the thread: the decision's yes
    let m = r.say("lead", &[], "looks good, go ahead");
    r.intake(m, json!({"head": "Place", "ticket": r.m.id(t)}));
    r.go(Input::Formed { t: d, step: 1, head: Some(json!({"head": "Yes"})), why: None, not_taken: None, call: None }).reply.unwrap();
    // the fixer's step, its words the minder's
    let Status::Asking { owed, role } = &r.m.tickets[&t].status else { panic!("{:?}", r.m.tickets[&t].status) };
    assert_eq!(role, "fixer");
    let step = owed.step;
    r.decide(t, step, "fixer");
    let s = r.take("fixer");
    assert!(matches!(&s.what, What::Step { template: Some(w), .. } if w == "Implement what the lead designed on this thread for: I want to design this: add median(xs) to stats"), "{:?}", s.what);
    // the fixer asks, and waits in its turn; the lead answers; the answer is the fixer's
    r.ask("fixer", "Should median([]) raise, or give None?").reply.unwrap();
    let q = r.question();
    r.decide(q, 1, "lead");
    r.take("lead");
    let out = r.go(Input::Answer { t: q, by: "lead".into(), head: json!({"head": "Answer", "text": "raise ValueError"}), text: Text::New(String::new()) });
    assert!(out.effects.contains(&Effect::Answered { agent: "fixer".into(), text: "raise ValueError".into() }), "{:?}", out.effects);
    // the fixer's commit; the runner's check is Green; the thread is kept
    r.go(Input::Answer { t, by: "fixer".into(), head: json!({"head": "Commit", "commit": "abc123"}), text: Text::New("median added".into()) }).reply.unwrap();
    let k = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    assert_eq!(r.m.tickets[&k].kind, "check");
    r.go(Input::Answer { t: k, by: "runner".into(), head: json!("Green"), text: Text::New(String::new()) }).reply.unwrap();
    assert_eq!(r.status(t), ViewStatus::Done { result: json!({"head": "Kept"}) });
    // and "5+5" is a thread too (design §16, 3: no one-off), resting once lead is given it
    let o = r.say("lead", &[], "what is 5+5");
    r.intake(o, json!({"head": "Thread"}));
    assert_eq!(r.status(o), ViewStatus::Resting);
    r.replays();
}

#[test]
fn a_program_names_a_post_of_its_ticket_by_number() {
    let mut r = Run::new();
    let t = r.say("lead", &[], "I want to design this: add median(xs)");
    r.intake(t, json!({"head": "Thread"}));
    r.handed("lead", 10);
    let design = r.m.tickets[&t].posts[1].text.clone();
    assert!(matches!(design, Text::Loc { .. }));
    r.minder(t, json!({"head": "Program", "text": "r <- step role fixer repair(text, text, \"src/stats.py\", post 2, none);\n..."}));
    let Status::Asking { owed, .. } = &r.m.tickets[&t].status else { panic!() };
    // what is kept is the reference, not the words
    assert_eq!(owed.args["note"], json!({"post": {"t": t, "n": 2}}));
    let step = owed.step;
    r.decide(t, step, "fixer");
    // what the fixer is given is the lead's words, where they are
    let d = r.take("fixer");
    let What::Step { args, .. } = &d.what else { panic!() };
    assert_eq!(args["note"], design.value());
    // in a call's inline text: the owner's words as they are, an agent's as a pointer to them
    r.go(Input::Answer { t, by: "fixer".into(), head: json!({"head": "Cannot", "why": "no"}), text: Text::New("no".into()) }).reply.unwrap();
    r.go(Input::TurnEnded { agent: "fixer".into(), delivery: Some(d.id.clone()), text: Loc { file: "transcript.jsonl".into(), offset: 7000, len: 0 }, later: vec![], stood: None, go: None });
    r.minder(t, json!({"head": "Program", "text": "a <- return post 1;\nb <- return post 2;\nv <- step to \"Asked: {{ a }}. Designed: {{ b }}\" -> Fine | Fix(why: Text);\n..."}));
    let d2 = r.m.due("lead").into_iter().find(|d| matches!(d.what, What::Step { .. })).unwrap();
    let What::Step { template, refs, .. } = &d2.what else { panic!() };
    assert_eq!(template.as_deref(), Some("Asked: I want to design this: add median(xs). Designed: {{@0}}"));
    assert_eq!(refs, &vec![design.clone()], "the lead's words, where they are, for render.rs to fill");
    // a post that is not there says so
    let gone = Text::Post { post: PostRef { t, n: 99 } }.value();
    assert_eq!(r.m.readable(&gone), json!(format!("(post 99 of {}, which is not there)", r.m.id(t))));
    // it prints as it parses; `post` alone is still a name
    let p = syntax::parse_prog("x <- return post 2;\npost x on y", 1).unwrap();
    assert_eq!(print::prog(&p, 0), "x <- return post 2;\npost x on y");
    r.replays();
}

#[test]
fn a_thread_is_not_quiet_while_a_question_raised_from_it_waits() {
    // paper-research m1225 b: on N-4 the minder ran while lead's Q-6 was open
    let mut r = Run::new();
    let t = r.say("dev", &["dev"], "bug: mean([]) crashes");
    let d = r.m.due("dev");
    assert!(d.iter().any(|d| d.t == t), "{d:?}");
    for x in &d {
        r.go(Input::Given { t: x.t, to: "dev".into(), delivery: x.id.clone(), upto: x.upto, skipped: Default::default(), place: Some(Run::loc(10)) });
    }
    // in that turn dev asks the lead, and replies on the thread
    let q = r.go(Input::Open { kind: "question".into(), under: None, opener: "dev".into(), form: json!({"ask": "Change it now?"}), text: None, names: vec![], told: vec![] });
    assert!(q.reply.is_ok(), "{:?}", q.reply);
    let q = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    assert_eq!(r.m.tickets[&q].about, vec![t]);
    r.go(Input::TurnEnded { agent: "dev".into(), delivery: Some(d[0].id.clone()), text: Run::loc(11), later: vec![], stood: None, go: None });
    assert!(r.m.tickets[&t].posts.len() >= 2, "dev's reply");
    assert!(!r.asking_minder(t), "the question is open: {:?}", r.m.tickets[&t].status);
    // the lead answers: closed, but its answer is not yet with dev
    r.decide(q, 1, "orch");
    r.answer(q, "orch", json!({"head": "Answer", "text": "yes"}));
    assert!(!r.m.tickets[&q].is_open());
    assert!(!r.asking_minder(t), "the answer is not handed yet");
    // dev is handed the answer and its turn ends: quiet now
    r.handed("dev", 20);
    assert!(r.asking_minder(t), "{:?}", r.m.tickets[&t].status);
    let lines: Vec<Line> = r.log.iter().map(|l| serde_json::from_str(&serde_json::to_string(l).unwrap()).unwrap()).collect();
    assert_eq!(Machine::load(defs()).replay(&lines), Vec::<String>::new());
}

#[test]
fn agents_on_a_thread_answer_the_owner_once_and_fall_quiet() {
    // paper-research m1237, after N-6 and N-7: an agent's post informs the others on the thread, it wakes only whom
    // it names with @
    let mut r = Run::new();
    let t = r.say("lead", &["fixer"], "Run ./check.sh and tell me whether it is green.");
    let (dl, df) = (r.m.due("lead"), r.m.due("fixer"));
    assert!(dl.iter().chain(&df).all(|d| d.t == t && !d.inform), "{dl:?} {df:?}");
    // fixer's turn: its answer is its turn's last message (the rule Reply), naming the owner
    for d in &df {
        r.go(Input::Given { t: d.t, to: "fixer".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Run::loc(10)) });
    }
    let out = r.go(Input::TurnEnded { agent: "fixer".into(), delivery: Some(df[0].id.clone()), text: Run::loc(11), later: vec![], stood: None, go: None });
    let p = r.m.tickets[&t].posts.last().unwrap().clone();
    assert!(p.auto && p.names == ["owner"], "{p:?}");
    // and policy is asked about it as about any post (m1240); coordd answers as notify.eflint does: the owner, named
    // already, and lead informed
    assert!(out.effects.contains(&Effect::Notify { t, event: "post".into(), actor: "fixer".into(), post: Some(p.n) }), "{:?}", out.effects);
    let out = r.go(Input::Notified { t, event: "post".into(), to: vec!["owner".into()], inform: vec!["lead".into()], post: Some(p.n), provenance: json!({}) });
    assert!(out.reply.is_ok() && !out.effects.iter().any(|e| matches!(e, Effect::Deliver { .. })), "{out:?}");
    assert_eq!(r.m.tickets[&t].posts.last().unwrap().inform, ["lead"]);
    // lead's turn: it posts on the thread with no @; fixer is informed, not woken
    for d in &dl {
        r.go(Input::Given { t: d.t, to: "lead".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Run::loc(20)) });
    }
    let out = r.post_on(t, "lead", "Green here too.", &[]);
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(!out.effects.contains(&Effect::Deliver { to: "fixer".into() }), "{:?}", out.effects);
    assert_eq!(r.m.tickets[&t].posts.last().unwrap().inform, ["fixer"]);
    r.go(Input::TurnEnded { agent: "lead".into(), delivery: Some(dl[0].id.clone()), text: Run::loc(21), later: vec![], stood: None, go: None });
    // what fixer has of it starts no turn, so nothing answers it: quiet, and the minder is asked
    let df = r.m.due("fixer");
    assert!(df.len() == 1 && df[0].inform, "{df:?}");
    assert!(r.asking_minder(t), "{:?}", r.m.tickets[&t].status);
    assert_eq!(r.m.view(t).unwrap().owner, Some(ViewOwner { spoke_last: false, unanswered: vec!["fixer".into(), "lead".into()] }));
    r.minder(t, json!({"head": "Rest"}));
    // naming with @ still wakes
    let out = r.post_on(t, "lead", "@fixer please rerun it", &["fixer"]);
    assert!(out.effects.contains(&Effect::Deliver { to: "fixer".into() }), "{:?}", out.effects);
    assert!(r.m.due("fixer").iter().any(|d| !d.inform));
    let lines: Vec<Line> = r.log.iter().map(|l| serde_json::from_str(&serde_json::to_string(l).unwrap()).unwrap()).collect();
    assert_eq!(Machine::load(defs()).replay(&lines), Vec::<String>::new());
}

#[test]
fn the_watcher_unsure_holds_nothing() {
    // the owner, N-354/N-355 ("o cannot go rule o nthat he. psot on both for all i care"; paper-research N-101.p1285):
    // words that could go on two open tickets are a thread of their own, given at once to whom he wrote them; the
    // tickets stay in intake's answer
    let mut r = Run::new();
    let a = r.say("lead", &["fixer"], "a design for median");
    let b = r.say("lead", &["fixer"], "fix mean([])");
    let t = r.say("lead", &[], "looks good, go ahead");
    let posts = (r.m.tickets[&a].posts.len(), r.m.tickets[&b].posts.len());
    r.intake(t, json!({"head": "Unsure", "tickets": [r.m.id(a), r.m.id(b)]}));
    assert_eq!(r.status(t), ViewStatus::Resting);
    assert_eq!(r.m.tickets[&t].posts.last().unwrap().names, ["lead"]);
    assert!(r.m.due("lead").iter().any(|d| d.t == t && !d.inform), "{:?}", r.m.due("lead"));
    assert_eq!((r.m.tickets[&a].posts.len(), r.m.tickets[&b].posts.len()), posts);
    let lines: Vec<Line> = r.log.iter().map(|l| serde_json::from_str(&serde_json::to_string(l).unwrap()).unwrap()).collect();
    assert_eq!(Machine::load(defs()).replay(&lines), Vec::<String>::new());

    // a thread opened under the kind with the owner's pick (before this): its held step is withdrawn for `return
    // Thread` (coordd's `release_picks` at load), and the words go to lead as a thread of their own
    let mut r = Run { m: Machine::load(defs_with_pick()), log: Vec::new(), clock: 0 };
    let a = r.say("lead", &["fixer"], "a design for median");
    let b = r.say("lead", &["fixer"], "fix mean([])");
    let t = r.say("lead", &[], "looks good, go ahead");
    r.intake(t, json!({"head": "Unsure", "tickets": [r.m.id(a), r.m.id(b)]}));
    assert!(matches!(r.status(t), ViewStatus::Awaits { ref agent, ref call, .. } if agent == "owner" && call == "pick"), "{:?}", r.status(t));
    assert!(r.m.due("lead").iter().all(|d| d.t != t));
    *r.m.defs.edit() = defs();
    let out = r.go(Input::Withdraw { t, by: "coordd".into(), program: "return Thread".into(), why: "held for the owner's pick".into() });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert_eq!(r.status(t), ViewStatus::Resting);
    assert_eq!(r.m.tickets[&t].posts.last().unwrap().names, ["lead"]);
    assert!(r.m.due("lead").iter().any(|d| d.t == t && !d.inform), "{:?}", r.m.due("lead"));
    let lines: Vec<Line> = r.log.iter().map(|l| serde_json::from_str(&serde_json::to_string(l).unwrap()).unwrap()).collect();
    assert_eq!(Machine::load(defs()).replay(&lines), Vec::<String>::new());
}

#[test]
fn the_tickets_an_agent_is_on_count_resting_threads() {
    // docket-edge m1259: lead had replied on two resting threads, so 'go ahead' placed on one was not its only ticket
    let mut r = Run::new();
    let a = r.say("lead", &["fixer"], "a design for median");
    let b = r.say("lead", &[], "fix mean([])");
    r.intake(b, json!({"head": "Thread"}));
    assert_eq!(r.m.open_on("lead"), vec![a, b]);
    assert_eq!(r.m.open_on("fixer"), vec![a]);
    // a thread kept is no longer open
    r.go(Input::Edit { t: a, by: "owner".into(), program: "return Kept".into(), why: "done".into() }).reply.unwrap();
    assert_eq!(r.m.open_on("lead"), vec![b]);
    assert!(r.m.open_on("fixer").is_empty());
}

// The owner's post is an event (design-two-systems §13): base.kind's `when owner_post` handler says whom it is for.

impl Run {
    /// The owner's own post on T, typed to TO (none: to nobody), as coordd sends it.
    fn owner_says(&mut self, t: u64, to: &[&str], text: &str) -> Out {
        let out = self.go(Input::Post { t, by: "owner".into(), text: Text::New(text.into()), names: to.iter().map(|a| a.to_string()).collect(), told: vec![], inform: vec![], answers: None, kind: None });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        out
    }
    /// A thread typed to the lead, with the fixer brought in by the lead's @: both have read it all.
    fn two_on_a_thread(&mut self) -> u64 {
        let t = self.say("lead", &[], "the median is wrong");
        self.intake(t, json!({"head": "Thread"}));
        self.handed("lead", 10);
        self.go(Input::Post { t, by: "lead".into(), text: Text::New("@fixer take the median".into()), names: vec!["fixer".into()], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
        self.handed("fixer", 20);
        self.handed("lead", 30);
        assert!(self.m.due("lead").is_empty() && self.m.due("fixer").is_empty());
        self.nobody_minds(t);
        t
    }
    /// Whether AGENT has a delivery of T due that wakes it (not only informs).
    fn woken(&self, agent: &str, t: u64) -> bool {
        self.m.due(agent).iter().any(|d| d.t == t && !d.inform)
    }
    fn informed(&self, agent: &str, t: u64) -> bool {
        self.m.due(agent).iter().any(|d| d.t == t && d.inform)
    }
}

#[test]
fn the_owners_words_to_the_one_agent_on_a_thread_are_told_at_once() {
    let mut r = Run::new();
    let t = r.say("lead", &[], "go ahead");
    let out = r.intake(t, json!({"head": "Thread"}));
    // the handler ran and ended in the same input: no call, nothing held, the lead woken (paper-research m1302, 4)
    assert!(out.lines.iter().any(|l| matches!(l, Line::Opened { kind, parent, .. } if kind == OWNER_POST && *parent == t)), "{:?}", out.lines);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Told { t: u, n: 1, to, inform, .. } if *u == t && to == &["lead".to_string()] && inform.is_empty())), "{:?}", out.lines);
    assert!(!out.effects.contains(&Effect::Deliver { to: "watcher".into() }), "no route call: {:?}", out.effects);
    assert!(out.effects.contains(&Effect::Deliver { to: "lead".into() }));
    assert!(r.handler().is_none());
    let p = &r.m.tickets[&t].posts[0];
    assert_eq!((p.held.clone(), p.routed, p.told.clone()), (None, true, vec!["lead".to_string()]));
    assert!(r.woken("lead", t));
    // plumbing: the lead is on this thread alone, so the words still go bare (m1302, 2); the handler's ticket takes no
    // number the owner sees, and is in no list
    assert_eq!(r.m.open_on("lead"), vec![t]);
    let h = *r.m.tickets.range(HIDDEN..).next().unwrap().0;
    assert!(r.m.tickets[&h].handling.is_some() && !r.m.open().contains(&h));
    assert!(r.m.view(t).unwrap().handlers.is_empty(), "closed");
    let u = r.say("fixer", &[], "another");
    assert_eq!(u, t + 1);
    r.replays();
}

#[test]
fn words_that_name_an_agent_go_to_it_with_no_call() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    let out = r.owner_says(t, &["fixer"], "@fixer use the sorted copy");
    assert!(!out.effects.contains(&Effect::Deliver { to: "watcher".into() }), "{:?}", out.effects);
    assert!(r.handler().is_none());
    assert!(r.woken("fixer", t));
    assert!(r.informed("lead", t) && !r.woken("lead", t), "{:?}", r.m.due("lead"));
    r.replays();
}

/// His own words (`Machine::his_words`; proto-lead N-1446 p83, p105; consult-3 p110) on N-1445's open: paper-research
/// opens a Named thread to paper-research-brain. Under today's thread.kind (ls-opener) its words are posted once, as
/// its #1, and his #2 is his. Under thread.kind before it, as N-1445 recorded it (research's record lines
/// 103218-103219, 20 Nov 19:07:16Z), they are posted again as #2 by owner: #1 is the agent's, #2 is not his though it
/// says owner, and his #3 is his. On a thread he opened, his first post is his.
#[test]
fn his_words_leave_out_an_agents_opener_posted_as_his() {
    let words = "paper-research · **Brain fundamentals: this talk now has its own thread and its own agent** (the owner's yes, N-1104 p627). It runs from his p593 to p627 on N-1104: the one structure with lenses and layers, the holder, the brain step, the commitment ledger, the error signals and fork/join. @paper-research-brain is a fork of me that carries all of it and talks only this, here. I keep the builds on N-1104. When it settles, the join comes back to N-1104 as a summary.";
    let open = |r: &mut Run| {
        let form = json!({"addressed": {"head": "Named"}, "label": "Brain fundamentals", "names": ["paper-research-brain"], "text": words, "to": "paper-research-brain", "typed": ["paper-research-brain"]});
        let out = r.go(Input::Open { kind: "thread".into(), under: None, opener: "paper-research".into(), form, text: Some(Text::New(words.into())), names: vec!["paper-research-brain".into()], told: vec![] });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        *r.m.tickets.range(..HIDDEN).next_back().unwrap().0
    };
    let by = |r: &Run, t: u64| -> Vec<(u32, String, bool)> { r.m.tickets[&t].posts.iter().map(|p| (p.n, p.by.clone(), r.m.his_words(t, p.n))).collect() };
    let typed = "okay nice so can we run through some real world examples";
    // today's thread.kind (ls-opener): the opener's words once, as its own #1; his next words are his
    let mut r = Run::new();
    let t = open(&mut r);
    let (n, _) = r.owner_posts(t, &[], typed);
    assert_eq!(n, 2);
    assert_eq!(by(&r, t), vec![(1, "paper-research".to_string(), false), (2, "owner".to_string(), true)]);
    // a thread opened under thread.kind before it, as N-1445 was (103219: `posted` #2 by owner): the opener's words
    // again as #2 by the owner, which are not his
    let mut r = Run::before_opener();
    let t = open(&mut r);
    assert!(r.log.iter().any(|l| matches!(l, Line::Posted { t: u, n: 2, by, .. } if *u == t && by == "owner")), "{:?}", r.log);
    let (n, _) = r.owner_posts(t, &[], typed);
    assert_eq!(n, 3);
    assert_eq!(by(&r, t), vec![(1, "paper-research".to_string(), false), (2, "owner".to_string(), false), (3, "owner".to_string(), true)]);
    assert!(!r.m.his_words(t, 4) && !r.m.his_words(t + 1, 1), "no such post, no such ticket");
    // a thread of his: his first post is his
    let u = r.say("lead", &[], "the median is wrong");
    r.intake(u, json!({"head": "Thread"}));
    assert_eq!(r.m.tickets[&u].posts[0].by, "owner");
    assert!(r.m.his_words(u, 1));
}

/// The thread's holder (the owner's fundamentals, N-1104 p593; route.call): his p586 and p587 on N-1104 (20 Nov,
/// research's record lines 101194-101265), 22 s apart, typed to nobody, on a thread with more than one agent. Today's
/// handler, unchanged: p586's form is To lead; p587's form, seeing where p586 went, is To lead, joins p586: told to
/// lead as any To, with no clarify step owed by him (101217 was one), the fixer informed. The answer line keeps
/// `joins`, so the record holds the one decision about both; the log replays. An answer with no `joins`, as every
/// answer before it, reads as it did.
#[test]
fn the_holders_form_joins_his_second_post_to_the_first_with_no_question_to_him() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    let (a, _) = r.owner_posts(t, &[], "eh yeah but wait lets analyze where we need special actions vs not");
    r.route_of(t, a, json!({"head": "To", "agents": ["lead"]}));
    let (b, _) = r.owner_posts(t, &[], "like ofc for reset apply can be special action open a claude ocde put in the ocmmands etc");
    let out = r.go(Input::Answer { t: r.handler_of(t, b), by: "watcher".into(), head: json!({"head": "To", "agents": ["lead"], "joins": a}), text: Text::New(String::new()) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Told { n, to, inform, .. } if *n == b && to == &["lead".to_string()] && inform == &["fixer".to_string()])), "{:?}", out.lines);
    assert!(r.log.iter().any(|l| matches!(l, Line::Answer { head, .. } if head["joins"] == json!(a))), "the record keeps joins");
    assert!(!r.log.iter().any(|l| matches!(l, Line::Step { call, .. } if call == "clarify")), "no question to him");
    assert_eq!((r.post(t, a).told.clone(), r.post(t, b).told.clone()), (vec!["lead".to_string()], vec!["lead".to_string()]));
    assert!(r.woken("lead", t) && !r.woken("fixer", t) && r.informed("fixer", t));
    assert!(r.handler().is_none());
    r.replays();
}

#[test]
fn the_watcher_routes_words_to_one_of_two_and_the_other_is_informed() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    let out = r.owner_says(t, &["lead"], "go ahead");
    let n = r.m.tickets[&t].last();
    // held: no agent is given it in any form while the watcher says whom it is for
    assert!(out.effects.contains(&Effect::Deliver { to: "watcher".into() }), "{:?}", out.effects);
    let h = r.handler().unwrap();
    assert_eq!(r.m.tickets[&t].posts.last().unwrap().held, Some(r.m.id(h)));
    assert!(r.m.due("lead").is_empty() && r.m.due("fixer").is_empty());
    let Status::Awaits { owed, .. } = &r.m.tickets[&h].status else { panic!() };
    assert_eq!(r.m.readable(&owed.args)["to"], json!(["lead"]));
    assert_eq!(r.m.readable(&owed.args)["others"], json!(["fixer"]));
    assert!(!r.m.open_on("watcher").contains(&h), "plumbing: in no list");
    // the minder is not asked while it is held, and the owner facts do not count it
    assert!(!r.asking_minder(t));
    assert!(r.m.view(t).unwrap().owner.is_some_and(|o| !o.spoke_last));
    // the fixer posts meanwhile: its read mark stays below the held post, and the lead is not given past it
    r.go(Input::Post { t, by: "fixer".into(), text: Text::New("@lead which median?".into()), names: vec!["lead".into()], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
    assert!(r.m.due("lead").is_empty(), "{:?}", r.m.due("lead"));
    let out = r.route(json!({"head": "To", "agents": ["fixer"]}));
    assert!(out.lines.iter().any(|l| matches!(l, Line::Told { n: m, to, inform, .. } if *m == n && to == &["fixer".to_string()] && inform == &["lead".to_string()])), "{:?}", out.lines);
    let p = r.m.tickets[&t].posts.iter().find(|p| p.n == n).unwrap();
    assert!(p.routed && p.told != p.names, "shown 'to fixer'");
    assert!(r.handler().is_none());
    // the fixer is woken with the owner's words; the lead is woken by the fixer's post, which comes with them
    assert!(r.woken("fixer", t));
    let d = r.m.due("lead");
    let What::Posts { unread } = &d[0].what else { panic!("{d:?}") };
    assert_eq!(unread.iter().map(|p| p.by.as_str()).collect::<Vec<_>>(), ["owner", "fixer"]);
    r.replays();
}

/// A binding on the brain's `owner_to` beside agent-base's kinds (0.3; proto-mind N-1446 p36): each event coordd sends
/// back opens a `noted` for its agent. The one setup of the `owner_to` tests, here and in tests/gather.rs (proto-reader
/// N-1446 p766), with `raised`, `Run::bound` and `Run::back`.
const BOUND: &[(&str, &str)] = &[("noted.kind", "kind noted\n  form agent: Agent\n  result ()\n  pattern\n    return ()\n"), ("brain.kind", "on owner_to(e) open noted(e.agent)\n")];

/// The `owner_to` events OUT raises, their values in order.
fn raised(out: &Out) -> Vec<Value> {
    out.effects.iter().filter_map(|e| match e { Effect::Event { name, value } if name == syntax::OWNER_TO => Some(value.clone()), _ => None }).collect()
}

impl Run {
    /// A run under agent-base's kinds and EXTRA (`defs_with`).
    fn with(extra: &[(&str, &str)]) -> Run {
        let (d, errors) = defs_with(extra);
        assert!(errors.is_empty(), "{errors:?}");
        Run { m: Machine::load(d), log: Vec::new(), clock: 0 }
    }
    /// A run with `BOUND`'s binding on `owner_to`.
    fn bound() -> Run {
        Run::with(BOUND)
    }
    /// coordd sends each of VS, values `raised`, back: the binding opens a `noted` for each; then the record replays
    /// under `BOUND`. Whom a `noted` is open for, every one so far, in order.
    fn back(&mut self, vs: Vec<Value>) -> Vec<Value> {
        for v in vs {
            let out = self.go(Input::Event { name: syntax::OWNER_TO.into(), value: v });
            assert!(out.reply.is_ok(), "{:?}", out.reply);
        }
        self.replays_with(BOUND);
        self.m.tickets.values().filter(|x| x.kind == "noted").map(|x| x.form["agent"].clone()).collect()
    }
}

/// The brain's `owner_to` (0.3; proto-mind N-1446 p36): while a binding is on it, the owner's held post raises it at the
/// hand (`tell`), once, one for each agent it wakes, `{agent, posts, tied_at}`, none for whom it only informs and none
/// while it is held; his post goes as it goes with no binding, and with none nothing is raised. coordd sends it back as
/// a recorded event, which the binding takes; a replay raises nothing and gives the same tickets.
#[test]
fn the_owners_held_post_raises_owner_to_at_the_hand_for_each_agent_it_wakes() {
    // a thread's first words, told to the lead at the watcher's intake
    let mut first = Run::bound();
    let t = first.say("lead", &[], "go ahead");
    let out = first.intake(t, json!({"head": "Thread"}));
    let p1 = format!("{}.p1", first.m.id(t));
    assert_eq!(raised(&out), vec![json!({"agent": "lead", "posts": [p1], "tied_at": p1})], "{:?}", out.effects);
    // his post to two on a thread, held while the watcher says whom it is for, beside the same with no binding
    let (mut r, mut plain) = (Run::bound(), Run::with(&BOUND[..1]));
    assert!(r.m.bound(syntax::OWNER_TO) && !plain.m.bound(syntax::OWNER_TO));
    let t = r.two_on_a_thread();
    assert_eq!(plain.two_on_a_thread(), t);
    let out = r.owner_says(t, &["lead"], "go ahead");
    plain.owner_says(t, &["lead"], "go ahead");
    assert!(raised(&out).is_empty(), "held: nothing raised yet: {:?}", out.effects);
    let n = r.m.tickets[&t].last();
    let out = r.route(json!({"head": "To", "agents": ["fixer"]}));
    let quiet = plain.route(json!({"head": "To", "agents": ["fixer"]}));
    let id = format!("{}.p{n}", r.m.id(t));
    let to = json!({"agent": "fixer", "posts": [id], "tied_at": id});
    assert_eq!(raised(&out), vec![to.clone()], "the fixer it wakes; not the lead it informs: {:?}", out.effects);
    assert!(raised(&quiet).is_empty(), "{:?}", quiet.effects);
    // the binding changes nothing of his post: the same lines, the same deliveries due
    assert_eq!(out.lines, quiet.lines);
    for a in ["lead", "fixer"] {
        assert_eq!(r.m.due(a), plain.m.due(a), "{a}");
    }
    // coordd sends it back: a recorded event line, fired, and the binding's ticket opened for the fixer
    let back = r.go(Input::Event { name: syntax::OWNER_TO.into(), value: to.clone() });
    assert!(back.reply.is_ok(), "{:?}", back.reply);
    assert!(back.lines.iter().any(|l| matches!(l, Line::Event { name, value, fired, .. } if name == syntax::OWNER_TO && *value == to && fired.len() == 1)), "{:?}", back.lines);
    assert_eq!(r.m.tickets.values().filter(|x| x.kind == "noted").map(|x| x.form["agent"].clone()).collect::<Vec<_>>(), [json!("fixer")]);
    // a start replays the record and raises nothing: the event is its line, read as it was
    let mut m2 = Machine::load(defs_with(BOUND).0);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert!(!m2.resume_effects().iter().any(|e| matches!(e, Effect::Event { .. })));
    r.replays_with(BOUND);
}

/// Only his words raise `owner_to` (proto-lead N-1446 p587; proto-reader and consult-3 p648): an agent's Named open
/// under thread.kind before ls-opener posted its words again as #2 by owner (N-1445 #2, research's record 103219), told
/// to the agent it is for. They are not his (`Machine::his_words`), so `posts_reached` raises nothing for them and no
/// pass of that agent's brain starts on them; the agent has them as post 1, by its ordinary delivery; his #3 raises it
/// at the hand, as any post of his does. Today's thread.kind posts no #2 (ls-opener), so the run is under the old one
/// (`thread_before_opener`) beside the binding, and its record, an old one, replays under today's kinds to the same
/// tickets (proto-lead p696).
#[test]
fn an_agents_opener_posted_as_his_raises_no_owner_to() {
    let old = thread_before_opener();
    let mut r = Run { m: Machine::load(defs_before(&[("thread.kind", old.as_str()), BOUND[0], BOUND[1]])), log: Vec::new(), clock: 0 };
    let words = "lead · **The send box: this talk now has its own thread.** @fixer carries it, here.";
    let form = json!({"addressed": {"head": "Named"}, "label": "The send box", "names": ["fixer"], "text": words, "to": "fixer", "typed": ["fixer"]});
    let out = r.go(Input::Open { kind: "thread".into(), under: None, opener: "lead".into(), form, text: Some(Text::New(words.into())), names: vec!["fixer".into()], told: vec![] });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let t = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    let p2 = r.post(t, 2);
    assert_eq!((p2.by.as_str(), p2.visible(), p2.wakes().any(|a| a == "fixer"), r.m.his_words(t, 2)), ("owner", true, true, false), "#2: lead's opener, posted as his, to fixer");
    assert!(raised(&out).is_empty(), "#2 is not his: {:?}", out.effects);
    // fixer has the opener's words, post 1, by its ordinary delivery
    let shown: Vec<Vec<u32>> = r.m.due("fixer").iter().filter(|d| d.t == t).map(|d| match &d.what {
        What::Posts { unread } | What::Step { unread, .. } | What::Closed { unread, .. } => unread.iter().map(|p| p.n).collect(),
        _ => Vec::new(),
    }).collect();
    assert!(shown.iter().any(|ns| ns.contains(&1)), "fixer is given post 1: {shown:?}");
    // his #3, held while the watcher says whom it is for, raises it at the hand
    let (n, out) = r.owner_posts(t, &["fixer"], "is the box done?");
    assert!(raised(&out).is_empty(), "held: nothing raised yet: {:?}", out.effects);
    let out = r.route(json!({"head": "To", "agents": ["fixer"]}));
    let id = format!("{}.p{n}", r.m.id(t));
    assert_eq!(raised(&out), vec![json!({"agent": "fixer", "posts": [id], "tied_at": id})], "{:?}", out.effects);
    r.replays_with(BOUND);
}

/// Slow mode and the brain (proto-lead N-1446 p155): `owner_to` is raised when his post reaches the agents it wakes.
/// Not halted (the thread not slowed, or a slowed one where he named them), at the tell. Halted (a slowed thread
/// where the watcher's `route` chose), not at the tell, so no pass and no call while his card holds it: when his card
/// lets it go, for whom it wakes as he left them, the agent he redirected it to if he did, and nobody if he left it to
/// nobody. The binding takes each, and each case replays to the same views.
#[test]
fn owner_to_is_raised_when_his_post_reaches_whom_it_wakes_on_a_slowed_thread_when_his_card_lets_it_go() {
    // a thread of two, slowed or not, and his post on it typed to the lead: its id
    let said = |slowed: bool, text: &str| -> (Run, u64, String, Out) {
        let mut r = Run::bound();
        let t = r.two_on_a_thread();
        if slowed {
            r.go(Input::Slow { t, on: true, by: "owner".into() }).reply.unwrap();
        }
        let out = r.owner_says(t, &["lead"], text);
        let id = format!("{}.p{}", r.m.id(t), r.m.tickets[&t].last());
        (r, t, id, out)
    };
    let to = |a: &str, id: &str| json!({"agent": a, "posts": [id], "tied_at": id});
    let card = |r: &Run, t: u64| r.m.tickets[&t].halts.iter().find(|h| h.what.name() == "deliver").map(|h| h.n).expect("his card");
    // not slowed, the watcher chose: at the tell
    let (mut r, _, id, _) = said(false, "go ahead");
    let out = r.route(json!({"head": "To", "agents": ["fixer"]}));
    assert_eq!(raised(&out), vec![to("fixer", &id)], "{:?}", out.effects);
    assert_eq!(r.back(raised(&out)), [json!("fixer")]);
    // slowed, named by him: a rule tells, no card, at the tell
    let (mut r, t, id, out) = said(true, "@lead go ahead");
    assert!(r.m.tickets[&t].halts.is_empty() && r.woken("lead", t), "{:?}", r.m.tickets[&t].halts);
    assert_eq!(raised(&out), vec![to("lead", &id)], "{:?}", out.effects);
    assert_eq!(r.back(raised(&out)), [json!("lead")]);
    // slowed, the watcher chose: his card holds it, nothing raised, no pass; his card lets it go: raised for the fixer
    let (mut r, t, id, _) = said(true, "go ahead");
    let out = r.route(json!({"head": "To", "agents": ["fixer"]}));
    assert!(raised(&out).is_empty() && !r.woken("fixer", t), "held at his card: {:?}", out.effects);
    let out = r.approve(t, card(&r, t));
    assert!(r.woken("fixer", t));
    assert_eq!(raised(&out), vec![to("fixer", &id)], "{:?}", out.effects);
    assert_eq!(r.back(raised(&out)), [json!("fixer")]);
    // his card redirects it to w3: raised for w3
    let (mut r, t, id, _) = said(true, "go ahead");
    r.route(json!({"head": "To", "agents": ["fixer"]}));
    let n = card(&r, t);
    r.go(Input::HaltEdit { t, n, by: "owner".into(), value: json!({"wakes": ["w3"]}) }).reply.unwrap();
    let out = r.approve(t, n);
    assert_eq!(raised(&out), vec![to("w3", &id)], "{:?}", out.effects);
    assert_eq!(r.back(raised(&out)), [json!("w3")]);
    // his card leaves it to nobody: nothing raised
    let (mut r, t, _, _) = said(true, "go ahead");
    r.route(json!({"head": "To", "agents": ["fixer"]}));
    let n = card(&r, t);
    r.go(Input::HaltEdit { t, n, by: "owner".into(), value: json!({"wakes": []}) }).reply.unwrap();
    let out = r.approve(t, n);
    assert!(raised(&out).is_empty(), "{:?}", out.effects);
    assert_eq!(r.back(Vec::new()), Vec::<Value>::new());
}

#[test]
fn an_owners_post_routed_to_one_of_two_informs_the_other() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.owner_says(t, &["lead"], "go ahead");
    r.route(json!({"head": "To", "agents": ["lead"]}));
    assert!(r.woken("lead", t));
    assert!(r.informed("fixer", t) && !r.woken("fixer", t), "{:?}", r.m.due("fixer"));
    r.replays();
}

#[test]
fn unclear_words_wait_for_the_owner_who_sends_them() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.owner_says(t, &["lead"], "do the other one");
    r.route(json!({"head": "Unclear", "why": "which one is the other one?"}));
    let h = r.handler().unwrap();
    // the clarify chip: on the thread's view, the handler's step owed by the owner, reached through Post.held
    let v = r.m.view(t).unwrap();
    assert_eq!(v.handlers.len(), 1);
    assert!(matches!(&v.handlers[0].status, ViewStatus::Awaits { agent, call, args, .. } if agent == "owner" && call == "clarify" && args["why"] == "which one is the other one?" && args["agents"] == json!(["lead", "fixer"])), "{:?}", v.handlers[0].status);
    assert_eq!(v.posts.last().unwrap().held, Some(v.handlers[0].id.clone()));
    // the handler's own run tree: the step it waits on now, and what comes after it still to come (N-93)
    let run = &v.handlers[0].run;
    assert!(!run.is_empty() && run.iter().any(|n| n.state == "now"), "{run:#?}");
    // the thread rests at its hole, yet reads as waiting on the owner, under its own id (paper-research m1313, 2)
    assert_eq!(v.status, ViewStatus::Resting);
    assert_eq!(v.waits_on, vec![WaitsOn { ticket: r.m.id(t), agent: "owner".into(), call: "clarify".into(), post: None, layer: None }]);
    assert!(r.m.due("lead").is_empty() && r.m.due("fixer").is_empty());
    // his press of "fixer" at the chip answers it (typed words never do, N-807.p78): no post, no new handler
    let owners = r.m.tickets[&t].posts.iter().filter(|p| p.by == "owner").count();
    r.go(Input::Answer { t: h, by: "owner".into(), head: json!({"head": "Send", "to": ["fixer"]}), text: Text::New(String::new()) }).reply.unwrap();
    assert!(r.handler().is_none());
    assert_eq!(r.m.tickets[&t].posts.iter().filter(|p| p.by == "owner").count(), owners, "the press answered, nothing posted");
    assert!(r.woken("fixer", t));
    assert!(r.informed("lead", t) && !r.woken("lead", t));
    assert!(r.m.view(t).unwrap().handlers.is_empty() && r.m.view(t).unwrap().waits_on.is_empty());
    r.replays();
}

#[test]
fn unclear_words_the_owner_drops_reach_nobody() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.owner_says(t, &["lead"], "hmm");
    let n = r.m.tickets[&t].last();
    r.route(json!({"head": "Unclear", "why": "it asks nothing"}));
    let h = r.handler().unwrap();
    let out = r.go(Input::Answer { t: h, by: "owner".into(), head: json!({"head": "Drop"}), text: Text::New(String::new()) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Dropped { n: m, .. } if *m == n)), "{:?}", out.lines);
    let p = r.m.tickets[&t].posts.iter().find(|p| p.n == n).unwrap();
    assert!(p.dropped && p.held.is_none());
    assert!(r.m.due("lead").is_empty() && r.m.due("fixer").is_empty());
    // what comes after it reaches the agents as ever, without it
    r.go(Input::Post { t, by: "fixer".into(), text: Text::New("@lead done".into()), names: vec!["lead".into()], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
    let d = r.m.due("lead");
    let What::Posts { unread } = &d[0].what else { panic!("{d:?}") };
    assert_eq!(unread.iter().map(|p| p.by.as_str()).collect::<Vec<_>>(), ["fixer"]);
    r.replays();
}

#[test]
fn a_handler_that_neither_tells_nor_drops_leaves_the_post_to_policy() {
    let k = "kind lax\n  form w: Agent\n  result ()\n  pattern\n    ...\n  when owner_post(p) do\n    return ()\n";
    let (d, errors) = defs_with(&[("lax.kind", k)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    r.go(Input::Open { kind: "lax".into(), under: None, opener: "lead".into(), form: json!({"w": "lead"}), text: None, names: vec!["lead".into()], told: vec![] }).reply.unwrap();
    let t = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    let out = r.go(Input::Post { t, by: "owner".into(), text: Text::New("note this".into()), names: vec!["fixer".into()], told: vec!["lead".into()], inform: vec![], answers: None, kind: None });
    assert!(out.lines.iter().any(|l| matches!(l, Line::Released { t: u, n: 1, .. } if *u == t)), "{:?}", out.lines);
    assert!(r.woken("fixer", t) && r.woken("lead", t));
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::Interrupt { .. })), "a held post released stops nobody (N-36): {:?}", out.effects);
    assert!(!r.m.tickets[&t].posts[0].routed);
    r.replays_with(&[("lax.kind", k)]);
}

#[test]
fn the_empty_list_pattern_checks_and_matches() {
    let k = "kind count\n  form xs: [Agent]\n  result Zero | Some\n  pattern\n    match xs {\n      [] => return Zero\n      _ => return Some\n    }\n";
    let (d, errors) = defs_with(&[("count.kind", k)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    for (xs, head) in [(json!([]), "Zero"), (json!(["lead"]), "Some")] {
        r.go(Input::Open { kind: "count".into(), under: None, opener: "lead".into(), form: json!({"xs": xs}), text: None, names: vec!["lead".into()], told: vec![] }).reply.unwrap();
        let t = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
        assert_eq!(r.status(t), ViewStatus::Done { result: json!({"head": head}) });
    }
    // `[]` alone does not cover a list
    let bad = "kind half\n  form xs: [Agent]\n  result ()\n  pattern\n    match xs {\n      [] => return ()\n    }\n";
    let (_, errors) = defs_with(&[("half.kind", bad)]);
    assert!(errors.iter().any(|e| e.contains("does not cover")), "{errors:?}");
    // it prints as it parses, `tell` and `drop` too
    let src = "match xs {\n  [] => tell xs p\n  _ => drop p\n}";
    let once = print::prog(&syntax::parse_prog(src, 1).unwrap(), 0);
    assert!(once.contains("[] =>") && once.contains("tell xs p") && once.contains("drop p"), "{once}");
    assert_eq!(print::prog(&syntax::parse_prog(&once, 1).unwrap(), 0), once);
}

#[test]
fn a_list_answer_may_come_as_a_comma_string() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.owner_says(t, &["lead"], "both of you, look");
    r.route(json!({"head": "Unclear", "why": "whom?"}));
    let h = r.handler().unwrap();
    // as `coord answer O-1 Send --to fixer,lead` sends it (docket-edge m1332)
    let out = r.go(Input::Answer { t: h, by: "owner".into(), head: json!({"head": "Send", "to": "fixer, lead"}), text: Text::New(String::new()) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(r.woken("fixer", t) && r.woken("lead", t));
    assert_eq!(Type::List(Box::new(Type::Int)).coerce(&json!("1,x")).map_err(|e| e.contains("x")), Err(true));
    r.replays();
}

impl Run {
    /// The open `owner_post` handler holding post N of T.
    fn handler_of(&self, t: u64, n: u32) -> u64 {
        self.m.tickets.values().find(|x| x.is_open() && x.post_of.as_ref().is_some_and(|p| p.t == t && p.n == n)).map(|x| x.t).expect("a handler holds it")
    }
    /// The watcher's answer HEAD to the `route` step of the handler holding post N of T.
    fn route_of(&mut self, t: u64, n: u32, head: Value) {
        let h = self.handler_of(t, n);
        self.go(Input::Answer { t: h, by: "watcher".into(), head, text: Text::New(String::new()) }).reply.unwrap();
    }
    /// The owner's post on T, as coordd sends it, with its own names; its number.
    fn owner_posts(&mut self, t: u64, names: &[&str], text: &str) -> (u32, Out) {
        let out = self.owner_says(t, names, text);
        (self.m.tickets[&t].last(), out)
    }
    fn post(&self, t: u64, n: u32) -> &crate::line::Post {
        self.m.tickets[&t].posts.iter().find(|p| p.n == n).unwrap()
    }
    /// The `told` lines for post N of T in the log.
    fn told_lines(&self, t: u64, n: u32) -> usize {
        self.log.iter().filter(|l| matches!(l, Line::Told { t: u, n: m, .. } if *u == t && *m == n)).count()
    }
}

/// N-1104's case (paper-research N-807.p78, consult-lead p79): his post A to nobody by name, `route` Unclear, then his
/// post B with other words and a name of its own. A is handed to every agent on the thread, with no `dropped` line; B
/// is a post of its own, routed as any; both are told. Nothing is formed against the chip. (His A there said `@all`,
/// its only address then; since the `@all` rule an `@all` goes to `claims`, not `route`, so A here has none.)
#[test]
fn the_owners_next_words_hand_the_post_he_left_unanswered_and_are_a_post_of_their_own() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    let (a, _) = r.owner_posts(t, &[], "when does the runner come?");
    r.route_of(t, a, json!({"head": "Unclear", "why": "the last post is by fixer"}));
    let h = r.handler_of(t, a);
    let (b, out) = r.owner_posts(t, &["lead"], "iu cannot click q so and so and go there");
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::Form { .. })), "nothing formed: {:?}", out.effects);
    assert!(!r.log.iter().any(|l| matches!(l, Line::Dropped { .. })), "no dropped line");
    assert!(!r.m.tickets[&h].is_open(), "A's handler ended");
    let pa = r.post(t, a);
    assert_eq!((pa.dropped, pa.held.clone(), pa.told.clone(), pa.inform.clone()), (false, None, vec!["lead".to_string(), "fixer".to_string()], vec![]));
    assert!(r.woken("lead", t) && r.woken("fixer", t), "A wakes both, as a post naming them all");
    let pb = r.post(t, b);
    assert_eq!((pb.text.clone(), pb.names.clone()), (Text::New("iu cannot click q so and so and go there".into()), vec!["lead".to_string()]));
    // B goes to `route` like any post of his, and is told by it
    r.route_of(t, b, json!({"head": "To", "agents": ["lead"]}));
    assert_eq!(r.post(t, b).told, vec!["lead".to_string()]);
    assert_eq!((r.told_lines(t, a), r.told_lines(t, b)), (1, 1));
    r.replays();
}

/// Three posts of his held, each under its own question, and one new message: all three handed in his order, none
/// dropped; the message is a post of its own (paper-research N-807.p78, 3 and 4).
#[test]
fn one_message_hands_every_held_post_and_drops_none() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    let ns: Vec<u32> = ["strong model only .", "no bad its 6.2", "or more"].iter().map(|w| r.owner_posts(t, &[], w).0).collect();
    for n in &ns {
        r.route_of(t, *n, json!({"head": "Unclear", "why": "who posted last"}));
    }
    assert_eq!(r.m.unanswered_holds().len(), 3);
    let (m, _) = r.owner_posts(t, &[], "this is my new only group chat for now");
    assert!(!r.log.iter().any(|l| matches!(l, Line::Dropped { .. })), "none dropped");
    let told: Vec<u32> = r.log.iter().filter_map(|l| match l { Line::Told { t: u, n, .. } if *u == t && *n >= ns[0] => Some(*n), _ => None }).collect();
    assert_eq!(told, ns, "each once, in his order");
    assert!(ns.iter().all(|n| r.post(t, *n).held.is_none() && !r.post(t, *n).dropped));
    assert!(r.post(t, m).held.is_some(), "the new message is held for its own route");
    r.replays();
}

/// His words that intake places on the thread hand the post he left held there, as his words posted on it do
/// (paper-research N-412.p1356, 2; consult-3 p1355, wrong 2): A is told to every agent on the thread, and B is a post of
/// its own.
#[test]
fn his_words_placed_on_the_thread_by_intake_hand_the_held_post_too() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    let (a, _) = r.owner_posts(t, &[], "when does the runner come?");
    r.route_of(t, a, json!({"head": "Unclear", "why": "the last post is by fixer"}));
    let h = r.handler_of(t, a);
    let m = r.say("lead", &[], "and the tower, is it free");
    let out = r.intake(m, json!({"head": "Place", "ticket": r.m.id(t)}));
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::Form { .. })), "nothing formed: {:?}", out.effects);
    assert!(!r.m.tickets[&h].is_open(), "A's handler ended");
    let pa = r.post(t, a);
    assert_eq!((pa.dropped, pa.told.clone()), (false, vec!["lead".to_string(), "fixer".to_string()]));
    let b = r.m.tickets[&t].posts.iter().rfind(|p| p.by == "owner").unwrap();
    assert_eq!((b.n > a, b.text.clone()), (true, Text::New("and the tower, is it free".into())));
    assert_eq!(r.told_lines(t, a), 1);
    r.replays();
}

/// A record from before N-807.p78 (research's N-1104 at 14:15:16Z-14:15:19Z, consult-3 p1355, wrong 1): his words were
/// formed against the question about held post A, and two posts of his came while that forming call was out. They are
/// posts of their own and hand nothing, as the old engine ran them; the post numbers after them are as recorded, and the
/// whole replays clean.
#[test]
fn posts_of_his_while_a_forming_call_on_a_held_post_is_out_hand_nothing() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    let (a, _) = r.owner_posts(t, &[], "when does the runner come?");
    r.route_of(t, a, json!({"head": "Unclear", "why": "the last post is by fixer"}));
    let h = r.handler_of(t, a);
    let Status::Awaits { owed, .. } = &r.m.tickets[&h].status else { panic!() };
    let step = owed.step;
    // the old engine's lines: his words, and the forming call on A's question they began (75373, 75374)
    r.clock += 1;
    let at = format!("2026-11-10T12:00:{:02}Z", r.clock);
    let mut lines = r.log.clone();
    lines.push(Line::Post { at: at.clone(), t, by: "owner".into(), text: Text::New("for whoever has the runner".into()), names: vec![], told: vec![], inform: vec![], esc: vec![], answers: None, passes: None, hold: None, kind: None, answering: Vec::new() });
    lines.push(Line::Forming { at, t: h, step });
    let mut m2 = Machine::load(defs());
    assert_eq!(m2.replay(&lines), Vec::<String>::new());
    let mut r = Run { m: m2, log: lines, clock: r.clock };
    assert!(r.m.tickets[&h].owner_words.is_some(), "his words are being formed against A's question");
    assert_eq!(r.m.tickets[&t].last(), a, "his formed words are no post");
    // two posts of his while it is out (75375, 75380): each a post of its own; A stays held, told to nobody
    let (p1, _) = r.owner_posts(t, &[], "and the tower?");
    let (p2, _) = r.owner_posts(t, &[], "both today");
    assert_eq!((p1, p2), (a + 1, a + 2));
    assert!(r.m.tickets[&h].is_open(), "A's handler is not cancelled");
    assert!(r.post(t, a).told.is_empty() && r.told_lines(t, a) == 0, "A is handed to nobody");
    // the forming comes back (75389): the handler answers it
    r.go(Input::Formed { t: h, step, head: Some(json!({"head": "Again", "text": "for whoever has the runner"})), why: None, not_taken: None, call: None }).reply.unwrap();
    r.replays();
}


/// consult-3, N-807.p80, 1: A held; B and C eight seconds apart, B's route not back when C comes. A is handed once;
/// B and C are each posted once in his order, each with its own names; nothing dropped.
#[test]
fn two_messages_in_a_row_are_each_a_post_and_hand_the_held_one_once() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    let (a, _) = r.owner_posts(t, &[], "a note on running jobs across machines");
    r.route_of(t, a, json!({"head": "Unclear", "why": "no post for this to answer"}));
    let (b, _) = r.owner_posts(t, &["lead"], "yes go make this");
    r.clock += 8;
    let (c, _) = r.owner_posts(t, &["fixer"], "as a page in my ui");
    assert_eq!((b + 1, r.told_lines(t, a)), (c, 1));
    assert_eq!((r.post(t, b).names.clone(), r.post(t, c).names.clone()), (vec!["lead".to_string()], vec!["fixer".to_string()]));
    assert!(!r.log.iter().any(|l| matches!(l, Line::Dropped { .. } | Line::Forming { .. })));
    // B's route answers late: B is told; C waits on its own
    r.route_of(t, b, json!({"head": "To", "agents": ["lead"]}));
    r.route_of(t, c, json!({"head": "To", "agents": ["fixer"]}));
    assert_eq!((r.told_lines(t, a), r.told_lines(t, b), r.told_lines(t, c)), (1, 1, 1));
    r.replays();
}

/// consult-3, N-807.p80, 2 (paper-research p81, 2): words with "drop" in them about something else are a post, and
/// the held post is handed, not withdrawn; his press of "drop it" still withdraws it, and the post stays in his
/// thread, marked.
#[test]
fn drop_comes_from_his_press_alone() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    let (a, _) = r.owner_posts(t, &[], "the accounts page first");
    r.route_of(t, a, json!({"head": "Unclear", "why": "whom?"}));
    let (b, _) = r.owner_posts(t, &[], "drop the wsl thing");
    assert!(!r.post(t, a).dropped && r.post(t, a).held.is_none(), "handed, not withdrawn");
    assert_eq!(r.post(t, b).text, Text::New("drop the wsl thing".into()));
    // the press
    r.route_of(t, b, json!({"head": "Unclear", "why": "which thing?"}));
    let h = r.handler_of(t, b);
    r.go(Input::Answer { t: h, by: "owner".into(), head: json!({"head": "Drop"}), text: Text::New(String::new()) }).reply.unwrap();
    assert!(r.post(t, b).dropped, "the press withdraws it");
    assert!(r.m.view(t).unwrap().posts.iter().any(|p| p.n == b), "still in his thread, marked");
    r.replays();
}

/// At the install (paper-research N-807.p78, 5; p81, 3): each post held then under a question to him is handed by a
/// record line of its own, once; the line seen again hands nothing, and a replay hands it once.
#[test]
fn a_post_held_at_the_install_is_handed_once_by_its_line() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    let (a, _) = r.owner_posts(t, &[], "when does the runner come?");
    r.route_of(t, a, json!({"head": "Unclear", "why": "the call that says whom it is for failed"}));
    assert_eq!(r.m.unanswered_holds(), vec![(t, a, r.handler_of(t, a))]);
    assert!(!r.m.held_handed);
    let out = r.go(Input::Event { name: HELD_HANDED.into(), value: json!({"t": t, "n": a}) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(r.m.held_handed && r.m.unanswered_holds().is_empty());
    assert_eq!(r.post(t, a).told, vec!["lead".to_string(), "fixer".to_string()]);
    let again = r.go(Input::Event { name: HELD_HANDED.into(), value: json!({"t": t, "n": a}) });
    assert!(again.reply.is_ok() && !again.lines.iter().any(|l| matches!(l, Line::Told { .. })), "{:?}", again.lines);
    assert_eq!(r.told_lines(t, a), 1);
    r.replays();
}

// What an agent's post asks the owner (design-two-systems §16).

/// The marks MARKS in TEXT read back as the words they stand on.
fn marked<'a>(text: &'a str, marks: &[asks::Mark]) -> Vec<(&'a str, Vec<&'a str>)> {
    marks.iter().map(|m| (&text[m.at[0]..m.at[1]], m.options.iter().map(|o| &text[o[0]..o[1]]).collect())).collect()
}

#[test]
fn the_passages_of_two_real_replies_are_found_where_they_stand() {
    // lead's reply of 12 Nov and the call's real answer (plan/mockups msgA, outA): 3 asks, 8 options, all found;
    // the byte offsets are render.py's character offsets moved past the one "–" (3 bytes) before them
    let text = include_str!("../testdata/msgA.txt").trim_end_matches('\n');
    let out: Value = serde_json::from_str(include_str!("../testdata/outA.json")).unwrap();
    let (marks, lost) = asks::spans(text, &out["items"]);
    assert!(lost.is_empty(), "{lost:?}");
    let shift = |s: [usize; 2]| [s[0] + 2, s[1] + 2];
    assert_eq!(marks[0].at, shift([892, 998]));
    assert_eq!(marks[0].options, [[958, 964], [966, 976], [978, 987], [989, 996]].map(shift));
    assert_eq!(marks[1].options, [[1015, 1027], [1032, 1061]].map(shift));
    assert_eq!(marks[2].options, [[1084, 1093], [1132, 1141]].map(shift));
    let read = marked(text, &marks);
    assert_eq!(read[1], ("Should it be desktop-only, or work on mobile with touch too?", vec!["desktop-only", "work on mobile with touch too"]));
    // the owner's example reply (msgB, outB2): 5 asks, 20 options; the three ideas stand outside question 1 and are
    // found where they are, after it
    let text = include_str!("../testdata/msgB.txt").trim_end_matches('\n');
    let out: Value = serde_json::from_str(include_str!("../testdata/outB2.json")).unwrap();
    let (marks, lost) = asks::spans(text, &out["items"]);
    assert!(lost.is_empty(), "{lost:?}");
    assert_eq!((marks.len(), marks.iter().map(|m| m.options.len()).sum::<usize>()), (5, 20));
    let read = marked(text, &marks);
    assert_eq!(read[0].1.last(), Some(&"Lantern maze"));
    assert!(marks[0].options.last().unwrap()[0] > marks[0].at[1], "an idea is outside its ask");
    for (m, (ask, opts)) in marks.iter().zip(&read) {
        assert!(text[m.at[0]..m.at[1]] == **ask && !opts.is_empty());
        assert!(m.options.windows(2).all(|w| w[0][1] <= w[1][0]), "in order of place, not overlapping");
    }
}

#[test]
fn options_never_overlap_and_a_repeated_one_is_taken_inside_its_ask() {
    // luna's grain differs from run to run (paper-research m1396): "mouse or touch", then "mouse" and "touch"
    let text = "Pick one: mouse or touch? Or ask: mouse or keys?";
    let items = json!([
        {"ask": "Pick one: mouse or touch?", "options": ["mouse or touch", "touch", "mouse"]},
        {"ask": "Or ask: mouse or keys?", "options": ["keys", "mouse", "nowhere"]},
        {"ask": "not in the text", "options": ["mouse"]},
    ]);
    let (marks, lost) = asks::spans(text, &items);
    let read = marked(text, &marks);
    assert_eq!(read, [("Pick one: mouse or touch?", vec!["mouse or touch"]), ("Or ask: mouse or keys?", vec!["mouse", "keys"])]);
    assert_eq!(lost, ["touch", "mouse", "nowhere", "not in the text"]);
}

impl Run {
    /// An agent's reply on T: AGENT is handed what is due to it, its turn's text the reply (rule Reply).
    fn replies(&mut self, agent: &str, n: u64) -> u32 {
        self.handed_asking(agent, n);
        self.m.tickets.values().flat_map(|x| x.posts.iter()).filter(|p| p.by == agent).map(|p| p.n).max().unwrap()
    }
    /// AGENTS are each handed the owner's post before any of them replies, so that no reply answers another's (a
    /// reply that wakes an asker closes its asks); their replies' numbers, conversions left to the test.
    fn reply_at_once<const K: usize>(&mut self, agents: [&str; K], n: u64) -> [u32; K] {
        let due: Vec<Vec<Delivery>> = agents.iter().map(|a| self.m.due(a)).collect();
        for (a, due) in agents.iter().zip(&due) {
            for d in due {
                self.go(Input::Given { t: d.t, to: (*a).into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Run::loc(n)) });
            }
        }
        for (i, (a, due)) in agents.iter().zip(&due).enumerate() {
            self.go(Input::TurnEnded { agent: (*a).into(), delivery: due.first().map(|d| d.id.clone()), text: Run::loc(n + 1 + 2 * i as u64), later: vec![], stood: None, go: None });
        }
        agents.map(|a| self.m.tickets.values().flat_map(|x| x.posts.iter()).filter(|p| p.by == a).map(|p| p.n).max().unwrap())
    }
    fn mark(at: [usize; 2], options: &[[usize; 2]]) -> Value {
        json!({"at": at, "options": options})
    }
}

#[test]
fn an_agents_reply_is_converted_before_the_minder_and_its_asks_wait_on_the_owner() {
    let mut r = Run::before_asks();
    let t = r.say("lead", &[], "hi can you please help me design a simple webgame?");
    r.intake(t, json!({"head": "Thread"}));
    let n = r.replies("lead", 10);
    // the reply is shown at once, not held; the thread is not quiet while it is converted: no minder yet
    let p = r.m.tickets[&t].posts.iter().find(|p| p.n == n).unwrap().clone();
    assert!(p.visible() && p.asks.is_empty());
    assert_eq!(r.status(t), ViewStatus::Resting);
    let v = r.m.view(t).unwrap();
    assert_eq!(v.waits_on, vec![WaitsOn { ticket: r.m.id(t), agent: "watcher".into(), call: "asks".into(), post: None, layer: None }]);
    assert!(r.m.open().iter().all(|u| r.m.tickets[u].handling.is_none()), "the conversion is in no list");
    // the watcher's marks: put on the post, the text as it was
    let out = r.asks(n, json!({"head": "Asks", "marks": [Run::mark([10, 30], &[[12, 16], [20, 25]]), Run::mark([40, 50], &[])]}));
    assert!(out.lines.iter().any(|l| matches!(l, Line::Marked { t: u, n: m, marks, .. } if *u == t && *m == n && marks.len() == 2)), "{:?}", out.lines);
    let p = r.m.tickets[&t].posts.iter().find(|p| p.n == n).unwrap();
    assert_eq!((p.asks[0].at, p.asks[0].options.clone(), p.text.clone()), ([10, 30], vec![[12, 16], [20, 25]], Text::Loc { loc: Run::loc(11) }));
    // now the minder looks; until it has, the thread is not waiting on the owner
    assert!(r.asking_minder(t), "{:?}", r.m.tickets[&t].status);
    assert!(r.m.view(t).unwrap().waits_on.iter().all(|w| w.call != "asks"));
    r.minder(t, json!({"head": "Rest"}));
    let v = r.m.view(t).unwrap();
    assert_eq!(v.waits_on, vec![WaitsOn { ticket: r.m.id(t), agent: "owner".into(), call: "asks".into(), post: Some(n), layer: None }]);
    assert!(v.posts.iter().find(|p| p.n == n).unwrap().asks_open);
    // a line to show, not a step he owes: his words are a plain post, typed to the asker, answering post N
    let out = r.go(Input::Post { t, by: "owner".into(), text: Text::New("arcade, and desktop only".into()), names: vec!["lead".into()], told: vec![], inform: vec![], answers: Some(n), kind: None });
    assert!(out.reply.as_ref().is_ok_and(|s| s.starts_with("Posted")), "{:?}", out.reply);
    let v = r.m.view(t).unwrap();
    let a = v.posts.last().unwrap();
    assert_eq!((a.by.as_str(), a.answers, a.names.clone()), ("owner", Some(n), vec!["lead".to_string()]));
    assert!(!v.posts.iter().find(|p| p.n == n).unwrap().asks_open, "it wakes the asker: closed");
    assert!(v.waits_on.iter().all(|w| w.call != "asks"));
    assert!(r.m.due("lead").iter().any(|d| d.upto == a.n), "lead is given the answer");
    r.replays();
}

#[test]
fn a_reply_that_asks_nothing_brings_the_minder_as_before() {
    let mut r = Run::before_asks();
    let t = r.say("lead", &[], "the median is wrong");
    r.intake(t, json!({"head": "Thread"}));
    let n = r.replies("lead", 10);
    assert!(!r.asking_minder(t));
    let out = r.asks(n, json!({"head": "Nothing"}));
    assert!(!out.lines.iter().any(|l| matches!(l, Line::Marked { .. })));
    assert!(r.asking_minder(t), "{:?}", r.m.tickets[&t].status);
    assert!(r.m.tickets[&t].posts.iter().all(|p| p.asks.is_empty() && !p.asks_open));
    r.replays();
}

#[test]
fn an_ask_closes_when_its_agent_posts_again_is_given_a_step_or_the_owner_says_nothing_to_answer() {
    let mut r = Run::before_asks();
    let t = r.say("lead", &["fixer"], "the release: what is left?");
    // fixer asks; lead asks after it: one line each, per asker
    let [f, l] = r.reply_at_once(["fixer", "lead"], 10);
    r.asks(f, json!({"head": "Asks", "marks": [Run::mark([0, 5], &[])]}));
    r.asks(l, json!({"head": "Asks", "marks": [Run::mark([0, 5], &[])]}));
    r.minder(t, json!({"head": "Rest"}));
    let lines = |r: &Run| r.m.view(t).unwrap().waits_on.iter().filter(|w| w.call == "asks").map(|w| w.post.unwrap()).collect::<Vec<_>>();
    assert_eq!(lines(&r), [f, l]);
    // the owner: nothing to answer on fixer's
    assert!(r.go(Input::NothingToAnswer { t, n: f, by: "fixer".into() }).reply.is_err(), "the owner's act");
    let out = r.go(Input::NothingToAnswer { t, n: f, by: "owner".into() });
    assert!(out.reply.is_ok() && matches!(out.lines.as_slice(), [Line::NothingToAnswer { .. }]), "{out:?}");
    assert_eq!(lines(&r), [l]);
    // lead posts again (coord post): its earlier ask closes; the new post is converted, asks nothing
    let out = r.post_on(t, "lead", "never mind, I found it", &[]);
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(!r.m.asks_open(&r.m.tickets[&t], l));
    r.nothing_asked();
    r.minder(t, json!({"head": "Rest"}));
    assert_eq!(lines(&r), Vec::<u32>::new());
    // a step given to the asker after its ask closes it: the minder answers lead from the thread
    let l2 = {
        r.post_on(t, "owner", "@lead which branch?", &["lead"]);
        r.replies("lead", 30)
    };
    r.asks(l2, json!({"head": "Asks", "marks": [Run::mark([0, 5], &[])]}));
    assert!(r.m.asks_open(&r.m.tickets[&t], l2));
    r.minder(t, json!({"head": "Program", "text": "r <- step lead \"Go ahead.\" -> Done | Cannot(why: Text);\n..."}));
    assert!(r.m.asks_open(&r.m.tickets[&t], l2), "owed, not yet given");
    let d = r.m.due("lead");
    for x in &d {
        r.go(Input::Given { t: x.t, to: "lead".into(), delivery: x.id.clone(), upto: x.upto, skipped: Default::default(), place: Some(Run::loc(40)) });
    }
    assert!(!r.m.asks_open(&r.m.tickets[&t], l2), "given: closed");
    r.replays();
    // the owner's cancel closes an ask open when he cancels, with the ticket (N-27 post 35, 2); a post told to him
    // after it asks again
    let mut r = Run::before_asks();
    let t = r.say("lead", &["lead"], "the release: what is left?");
    let l3 = r.replies("lead", 10);
    r.asks(l3, json!({"head": "Asks", "marks": [Run::mark([0, 5], &[])]}));
    assert!(r.m.asks_open(&r.m.tickets[&t], l3));
    r.cancel(t, "owner", "done with it").reply.unwrap();
    let v = r.m.view(t).unwrap();
    assert!(!r.m.asks_open(&r.m.tickets[&t], l3) && !v.posts.iter().any(|p| p.asks_open) && v.waits_on.iter().all(|w| w.agent != "owner"));
    assert_eq!(r.m.asks_closed(&r.m.tickets[&t], l3), Some(crate::line::AsksClosed::Cancelled));
    let l4 = {
        r.post_on(t, "lead", "one thing still: ship Friday?", &["owner"]).reply.unwrap();
        r.m.tickets[&t].last()
    };
    r.asks(l4, json!({"head": "Asks", "marks": [Run::mark([0, 5], &[])]}));
    assert!(r.m.asks_open(&r.m.tickets[&t], l4), "asked after his cancel: open");
    assert!(r.m.view(t).unwrap().waits_on.iter().any(|w| w.agent == "owner" && w.post == Some(l4)));
    r.replays();
    // a close that is not his (here an agent's cancel) leaves it open until one of the four things (3h): here, he says
    // nothing to answer
    let mut r = Run::before_asks();
    let t = r.say("lead", &["lead"], "the release: what is left?");
    let l3 = r.replies("lead", 10);
    r.asks(l3, json!({"head": "Asks", "marks": [Run::mark([0, 5], &[])]}));
    r.cancel(t, "lead", "moved to the tracker").reply.unwrap();
    assert!(!r.m.tickets[&t].is_open());
    assert!(r.m.asks_open(&r.m.tickets[&t], l3) && r.m.view(t).unwrap().waits_on.iter().any(|w| w.agent == "owner" && w.post == Some(l3)));
    r.go(Input::NothingToAnswer { t, n: l3, by: "owner".into() }).reply.unwrap();
    assert!(!r.m.asks_open(&r.m.tickets[&t], l3) && !r.m.view(t).unwrap().posts.iter().any(|p| p.asks_open));
    r.replays();
}

/// Undo of the minder's answer gives back the asks its step closed, as they stood before it, and the thread waits on
/// the owner for them again (paper-research m1463, 1).
/// A question inside a post that only informs the owner does not wait on him (paper-research N-800.p12, docket-edge6
/// on N-807.p17: N-851.p2, paper-research to paper-research2, was in his "Waits on you"); told to him, it does.
#[test]
fn an_ask_in_a_post_that_only_informs_the_owner_does_not_wait_on_him() {
    let waits = |inform: bool| {
        let mut r = Run::before_asks();
        let t = r.say("lead", &["fixer"], "the release: what is left?");
        r.settle(10);
        // quiet at its start: the minder looks then and lets it rest, before the fixer's post
        if r.asking_minder(t) {
            r.minder(t, json!({"head": "Rest"}));
        }
        let (told, inform) = if inform { (vec![], vec!["owner".to_string()]) } else { (vec!["owner".to_string()], vec![]) };
        let out = r.go(Input::Post { t, by: "fixer".into(), text: Text::New("may I cut it now?".into()), names: vec![], told, inform, answers: None, kind: None });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        let n = r.m.tickets[&t].posts.last().unwrap().n;
        r.asks(n, json!({"head": "Asks", "marks": [Run::mark([0, 5], &[])]}));
        // the thread is quiet once each is handed what moved it; they read the fixer's post and say nothing, so its
        // asks stay open (a reply waking the fixer would answer them: `closers`): then the minder
        for a in ["lead", "fixer"] {
            let due = r.m.due(a);
            if due.is_empty() {
                continue;
            }
            for d in &due {
                r.go(Input::Given { t: d.t, to: a.into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Run::loc(30)) }).reply.unwrap();
            }
            r.go(Input::TurnEnded { agent: a.into(), delivery: due.first().map(|d| d.id.clone()), text: Loc { len: 0, ..Run::loc(31) }, later: vec![], stood: None, go: None }).reply.unwrap();
        }
        r.minder(t, json!({"head": "Rest"}));
        let w: Vec<u32> = r.m.view(t).unwrap().waits_on.iter().filter(|w| w.call == "asks" && w.agent == "owner").map(|w| w.post.unwrap()).collect();
        let x = &r.m.tickets[&t];
        let why = format!("{:?}; asked at {:?}; posts {:?}", x.status, x.asked_at, x.posts.iter().map(|p| (p.n, p.by.clone(), p.asks_open)).collect::<Vec<_>>());
        r.replays();
        (w, n, why)
    };
    let (w, _, why) = waits(true);
    assert_eq!(w, Vec::<u32>::new(), "only informed: not his; {why}");
    let (w, n, why) = waits(false);
    assert_eq!(w, vec![n], "told: it waits on him; {why}");
}

#[test]
fn undo_of_the_minders_answer_gives_the_asks_back() {
    use crate::line::AsksClosed;
    let (mut r, t, l, step) = Run::lead_holds_the_minders_step();
    let closed = |r: &Run| r.m.view(t).unwrap().posts.iter().find(|p| p.n == l).map(|p| (p.asks_open, p.asks_closed.clone())).unwrap();
    let n = r.m.tickets[&t].shaped.last().unwrap().n;
    assert_eq!(closed(&r), (false, Some(AsksClosed::Told { step, minder: Some(n) })));
    let out = r.go(Input::Undo { t, by: "owner".into() });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let v = r.m.view(t).unwrap();
    assert!(v.shaped.iter().find(|s| s.n == n).unwrap().undone, "the card can say so");
    assert_eq!(closed(&r), (true, None), "the ask is open again");
    assert_eq!(v.waits_on, vec![WaitsOn { ticket: r.m.id(t), agent: "owner".into(), call: "asks".into(), post: Some(l), layer: None }]);
    assert!(r.m.due("lead").iter().any(|d| d.id == format!("{}.s{step}.withdrawn", r.m.id(t))), "lead is told the step is withdrawn");
    r.replays();
}

impl Run {
    /// Lead's post on a thread asks; the minder's program gives lead a step about it, and lead takes it. The thread,
    /// lead's post and the step.
    fn lead_holds_the_minders_step() -> (Run, u64, u32, u32) {
        let mut r = Run::before_asks();
        r.m.defs.edit().block_ask = true;
        let t = r.say("lead", &["lead"], "add range(xs)");
        let l = r.replies("lead", 10);
        r.asks(l, json!({"head": "Asks", "marks": [Run::mark([0, 5], &[])]}));
        r.minder(t, json!({"head": "Program", "text": "r <- step lead \"Go ahead.\" -> Done | Cannot(why: Text);\n..."}));
        let d = r.take("lead");
        let What::Step { step, .. } = d.what else { panic!("{:?}", d.what) };
        (r, t, l, step)
    }
    /// The open ticket of AGENT's ask, and the question under it.
    fn ask_of(&self, agent: &str) -> (u64, u64) {
        let u = self.m.tickets.values().find(|x| x.ask.as_ref().is_some_and(|a| a.by == agent)).unwrap().t;
        (u, self.m.tickets[&u].children[0])
    }
}

/// The holder of a step withdrawn while it waits on an ask about it (paper-research m1469, 2): one path for Cancel,
/// Withdraw and Undo. Its ask is answered with the withdrawal in its own words; the ask and its question close as
/// Dismissed and leave the owner's list; the withdrawal is not handed again.
#[test]
fn a_withdrawn_step_answers_the_ask_its_holder_waits_on() {
    let dismissed = |r: &Run, u: u64| matches!(&r.m.tickets[&u].status, Status::Done(v) if v["head"] == "Dismissed");
    // Undo, the ask waiting inside ASK_WAIT
    let (mut r, t, _, step) = Run::lead_holds_the_minders_step();
    r.ask("lead", "Owner: shall I go ahead?").reply.unwrap();
    let (u, q) = r.ask_of("lead");
    assert!(r.m.open().contains(&q));
    let out = r.go(Input::Undo { t, by: "owner".into() });
    let answered = out.effects.iter().find_map(|e| match e {
        Effect::AnsweredWith { agent, d } if agent == "lead" => Some(d.clone()),
        _ => None,
    });
    let d = answered.expect("the ask is answered with the withdrawal");
    assert_eq!((d.id.clone(), d.t), (format!("{}.s{step}.withdrawn", r.m.id(t)), t));
    assert!(matches!(&d.what, What::Withdrawn { step: s, by: Some(b), why: Some(w), .. } if *s == step && b == "owner" && w == "undone"), "{:?}", d.what);
    assert!(dismissed(&r, u) && dismissed(&r, q), "{:?} {:?}", r.m.tickets[&u].status, r.m.tickets[&q].status);
    assert!(!r.m.open().contains(&q), "the question leaves the list");
    assert!(r.m.tickets[&u].ask.as_ref().is_some_and(|a| a.replied && !a.waiting));
    assert!(r.m.due("lead").is_empty(), "the withdrawal was its answer; nothing else: {:?}", r.m.due("lead"));
    r.replays();
    // Undo, the ask gone later: nothing to answer in a turn; the withdrawal is no longer held behind it
    let (mut r, t, _, step) = Run::lead_holds_the_minders_step();
    r.ask("lead", "Owner: shall I go ahead?").reply.unwrap();
    let (u, q) = r.ask_of("lead");
    r.go(Input::AskLater { by: "lead".into() }).reply.unwrap();
    assert!(r.m.due("lead").is_empty(), "{:?}", r.m.due("lead"));
    let out = r.go(Input::Undo { t, by: "owner".into() });
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::AnsweredWith { .. } | Effect::Answered { .. })), "{:?}", out.effects);
    assert!(dismissed(&r, u) && dismissed(&r, q));
    assert!(r.m.due("lead").iter().any(|d| d.id == format!("{}.s{step}.withdrawn", r.m.id(t))), "{:?}", r.m.due("lead"));
    r.replays();
    // Cancel of the thread, the ask waiting
    let (mut r, t, _, step) = Run::lead_holds_the_minders_step();
    r.ask("lead", "Owner: shall I go ahead?").reply.unwrap();
    let (u, q) = r.ask_of("lead");
    let out = r.cancel(t, "owner", "not needed");
    assert!(out.effects.iter().any(|e| matches!(e, Effect::AnsweredWith { agent, d: Delivery { what: What::Withdrawn { step: s, why: Some(w), .. }, .. } } if agent == "lead" && *s == step && w == "not needed")), "{:?}", out.effects);
    assert!(dismissed(&r, u) && dismissed(&r, q));
    r.replays();
}

/// The shaped view says how each closed ask was closed (paper-research m1463, 3), from the lines the log has.
#[test]
fn a_closed_ask_says_how_it_closed() {
    use crate::line::AsksClosed;
    let mut r = Run::before_asks();
    let t = r.say("lead", &["fixer"], "the release: what is left?");
    let [f, l] = r.reply_at_once(["fixer", "lead"], 10);
    r.asks(f, json!({"head": "Asks", "marks": [Run::mark([0, 5], &[])]}));
    r.asks(l, json!({"head": "Asks", "marks": [Run::mark([0, 5], &[])]}));
    r.minder(t, json!({"head": "Rest"}));
    let how = |r: &Run, n: u32| r.m.view(t).unwrap().posts.iter().find(|p| p.n == n).unwrap().asks_closed.clone();
    assert_eq!((how(&r, f), how(&r, l)), (None, None), "open");
    r.go(Input::NothingToAnswer { t, n: f, by: "owner".into() }).reply.unwrap();
    assert_eq!(how(&r, f), Some(AsksClosed::NothingToAnswer));
    r.post_on(t, "lead", "never mind, I found it", &[]).reply.unwrap();
    let p = r.m.tickets[&t].posts.last().unwrap().n;
    assert_eq!(how(&r, l), Some(AsksClosed::Posted { post: p }));
    r.nothing_asked();
    r.minder(t, json!({"head": "Rest"}));
    // the owner's answers, from the card
    r.post_on(t, "owner", "@lead which branch?", &["lead"]);
    let l2 = r.replies("lead", 30);
    r.asks(l2, json!({"head": "Asks", "marks": [Run::mark([0, 5], &[])]}));
    r.minder(t, json!({"head": "Rest"}));
    r.go(Input::Post { t, by: "owner".into(), text: Text::New("main".into()), names: vec!["lead".into()], told: vec![], inform: vec![], answers: Some(l2), kind: None }).reply.unwrap();
    let a = r.m.tickets[&t].posts.last().unwrap().n;
    assert_eq!(how(&r, l2), Some(AsksClosed::Answered { post: a, by: "owner".into() }));
    // what came first is what it says: nothing to answer after the answers changes nothing
    r.go(Input::NothingToAnswer { t, n: l2, by: "owner".into() }).reply.unwrap();
    assert_eq!(how(&r, l2), Some(AsksClosed::Answered { post: a, by: "owner".into() }));
    r.replays();
}

/// paper-research N-447 p16, 1 (the shape p18): on each post whose asks are open to the owner, which of his three
/// targets would close them. A closed thread: all three. An open one: his reply to the asker and a subthread do; his
/// post at the foot does when the asker is the only other agent there, else the routing decides. A thread tied at the
/// post whose first post wakes the asker closes them (`Subthread`); one that wakes somebody else does not.
#[test]
fn the_view_says_which_of_his_posts_would_close_an_open_ask() {
    use crate::line::{AsksClosed, Closes, WouldClose};
    let wc = |r: &Run, t: u64, n: u32| r.m.view(t).unwrap().posts.iter().find(|p| p.n == n).unwrap().would_close.clone();
    let all = |foot| Some(WouldClose { foot, reply: Closes::Yes, subthread: Closes::Yes });
    // a closed agents' thread
    let mut r = Run::before_asks();
    let t = r.note("lead", "owner", "the release: A or B?");
    assert_eq!(wc(&r, t, 1), None, "no asks yet");
    r.asks(1, json!({"head": "Asks", "marks": [Run::mark([13, 19], &[])]}));
    assert_eq!(wc(&r, t, 1), all(Closes::Yes));
    // a subthread tied at post 1 typed to somebody else closes nothing; one typed to the asker closes it
    let other = r.say("fixer", &[], "the docs, meanwhile");
    r.go(Input::Tie { t: other, to: Some(t), by: "owner".into() }).reply.unwrap();
    assert_eq!(wc(&r, t, 1), all(Closes::Yes), "tied, but it wakes the fixer");
    let sub = r.say("lead", &[], "B, and here is why");
    r.go(Input::Tie { t: sub, to: Some(t), by: "owner".into() }).reply.unwrap();
    let closed = r.m.view(t).unwrap().posts[0].asks_closed.clone();
    assert_eq!((wc(&r, t, 1), closed), (None, Some(AsksClosed::Subthread { ticket: r.m.id(sub) })));
    r.replays_today();
    // an open (resting) thread with two agents: the foot is the routing's
    let mut r = Run::before_asks();
    let t = r.say("lead", &["fixer"], "the release: what is left?");
    let [f, l] = r.reply_at_once(["fixer", "lead"], 10);
    r.asks(f, json!({"head": "Asks", "marks": [Run::mark([0, 5], &[])]}));
    r.asks(l, json!({"head": "Asks", "marks": [Run::mark([0, 5], &[])]}));
    r.minder(t, json!({"head": "Rest"}));
    assert_eq!((wc(&r, t, f), wc(&r, t, l)), (all(Closes::Routed), all(Closes::Routed)));
    // with one agent on it, the foot goes to that agent
    let mut r = Run::before_asks();
    let t = r.say("lead", &[], "the release: what is left?");
    r.intake(t, json!({"head": "Thread"}));
    let l = r.replies("lead", 10);
    r.asks(l, json!({"head": "Asks", "marks": [Run::mark([0, 5], &[])]}));
    r.minder(t, json!({"head": "Rest"}));
    assert_eq!(wc(&r, t, l), all(Closes::Yes));
    r.replays_today();
}

/// paper-research N-447 p16, 2 (the form p30); 0.2 B8 (N-1396): the author takes back its post. While nobody has it, it
/// goes: it keeps its number, is drawn no more and is due to nobody. Once an agent has it (given up to it, or read for
/// its handler) it is struck (`a_post_taken_back_after_it_was_given_is_struck_and_each_agent_told_once`). Anyone else,
/// a thread's first post and a post taken back are refused. The owner's post held by a handler that has handed nothing
/// ends the handler and goes; one whose route the watcher has been handed ends the handler all the same, the watcher's
/// step withdrawn, and is struck, the watcher told nothing more (coordd answers it itself).
#[test]
fn a_post_is_taken_back_gone_until_somebody_has_it_then_struck() {
    let shown = |r: &Run, t: u64| r.m.view(t).unwrap().posts.iter().map(|p| p.n).collect::<Vec<_>>();
    let struck = |r: &Run, t: u64| r.m.view(t).unwrap().struck.iter().map(|s| (s.post.n, s.to.clone())).collect::<Vec<_>>();
    let back = |r: &mut Run, t: u64, n: u32, by: &str| r.go(Input::TakeBack { t, n, by: by.into() }).reply;
    let mut r = Run::before_asks();
    let t = r.say("lead", &["fixer"], "the release: what is left?");
    let [f, l] = r.reply_at_once(["fixer", "lead"], 10);
    r.minder(t, json!({"head": "Rest"}));
    assert!(back(&mut r, t, 1, "owner").unwrap_err().contains("cancel the thread"));
    let _ = l;
    // the fixer's reply, which the minder was given (the lead's reply came at once, so the lead was not): struck, every
    // agent given it told
    let had: Vec<String> = r.m.tickets[&t].given.iter().filter(|g| g.upto >= f && g.to != "fixer" && g.to != WATCHER).fold(Vec::new(), |mut v, g| {
        if !v.contains(&g.to) {
            v.push(g.to.clone());
        }
        v
    });
    assert_eq!(had, vec![String::from("minder")]);
    let out = r.go(Input::TakeBack { t, n: f, by: "fixer".into() });
    assert!(out.reply.is_ok() && matches!(out.lines.as_slice(), [Line::Event { name, .. }] if name == TAKEN_BACK), "{out:?}");
    assert!(!shown(&r, t).contains(&f) && struck(&r, t) == vec![(f, had.clone())], "{:?}", struck(&r, t));
    for a in &had {
        assert!(r.m.due(a).iter().any(|d| d.id == format!("{}.p{f}.taken_back", r.m.id(t))), "{a} is told");
    }
    assert!(back(&mut r, t, f, "fixer").unwrap_err().contains("taken back already"));
    r.replays();
    // the fixer's post, informing the lead, not handed yet
    r.post_on(t, "fixer", "one more: the docs", &["lead"]).reply.unwrap();
    let p = r.m.tickets[&t].posts.last().unwrap().n;
    assert!(r.m.due("lead").iter().any(|d| d.t == t && d.upto >= p));
    assert!(back(&mut r, t, p, "lead").unwrap_err().contains("only its author"));
    let out = r.go(Input::TakeBack { t, n: p, by: "fixer".into() });
    assert!(out.reply.is_ok() && matches!(out.lines.as_slice(), [Line::TakenBack { .. }]), "{out:?}");
    assert!(!shown(&r, t).contains(&p) && r.m.tickets[&t].posts.iter().any(|q| q.n == p && q.taken_back), "kept, not drawn");
    assert!(r.m.due("lead").iter().all(|d| d.t != t || d.upto < p || r.m.tickets[&t].posts.iter().any(|q| q.n > p && q.n <= d.upto && q.visible())), "due to nobody");
    assert!(back(&mut r, t, p, "fixer").unwrap_err().contains("taken back already"));
    r.replays();
    // the owner's post to nobody, two agents on the thread: base's handler asks the watcher whom it is for
    r.post_on(t, "owner", "and the changelog", &[]).reply.unwrap();
    let o = r.m.tickets[&t].posts.last().unwrap().n;
    let h = r.handler().expect("held for its handler");
    back(&mut r, t, o, "owner").unwrap();
    assert!(!r.m.tickets[&h].is_open() && r.handler().is_none(), "the handler ends with it");
    assert!(!shown(&r, t).contains(&o));
    r.replays();
    r.post_on(t, "owner", "and the tests", &[]).reply.unwrap();
    let o = r.m.tickets[&t].posts.last().unwrap().n;
    let h = r.handler().expect("held");
    let d = r.give("watcher", h);
    let _ = d;
    // read for its handler: the handler ends, its step withdrawn from the watcher, and the post is struck
    let out = r.go(Input::TakeBack { t, n: o, by: "owner".into() });
    assert!(out.reply.is_ok() && matches!(out.lines.as_slice(), [Line::Event { name, .. }, Line::Closed { .. }] if name == TAKEN_BACK), "the event, then the handler's end: {out:?}");
    assert!(!r.m.tickets[&h].is_open() && r.handler().is_none(), "the handler ends with it");
    assert!(r.m.tickets[&h].withdrawn.iter().any(|w| w.0 == "watcher"), "its step withdrawn from the watcher");
    assert!(!shown(&r, t).contains(&o) && struck(&r, t).iter().any(|(n, to)| *n == o && to.iter().all(|a| a != WATCHER)), "{:?}", struck(&r, t));
    r.replays();
}

/// 0.2 B8 (N-1396; consult-3, N-818 p74): his post taken back after it was given is struck, not gone. The line is the
/// `event` kind (TAKEN_BACK), which an engine from before reads as an event nothing takes. The post is owed by nobody:
/// the agent's reply after it does not name him for it, and his list does not count it unanswered. The agent is told
/// once (`pN.taken_back`, not only informing it), a hand-over of that notice not read tells it again, and given, it is
/// due no more. A replay gives the same.
#[test]
fn a_post_taken_back_after_it_was_given_is_struck_and_each_agent_told_once() {
    let mut r = Run::new();
    let t = r.note("owner", "lead", "is the build green?");
    let d = r.give("lead", t);
    r.turn("lead", Some(&d), 5).reply.unwrap();
    r.post_on(t, "owner", "and ship the stills", &["lead"]).reply.unwrap();
    let p = r.m.tickets[&t].posts.last().unwrap().n;
    let d = r.give("lead", t);
    let id = r.m.id(t);
    // the agent has it: struck
    let out = r.go(Input::TakeBack { t, n: p, by: "owner".into() });
    let said = out.reply.clone().unwrap();
    assert!(said.contains("struck") && said.contains("lead told"), "{said}");
    match out.lines.as_slice() {
        [Line::Event { name, value, fired, counted, on: None, .. }] => {
            assert_eq!(name, TAKEN_BACK);
            assert_eq!(*value, json!({"t": t, "n": p, "by": "owner", "to": ["lead"]}));
            assert!(fired.is_empty() && counted.is_empty(), "nothing fires on it");
        }
        l => panic!("{l:?}"),
    }
    let v = r.m.view(t).unwrap();
    assert!(!v.posts.iter().any(|q| q.n == p), "not among the posts a window from before draws");
    assert_eq!(v.struck.iter().map(|s| (s.post.n, s.post.by.clone(), s.to.clone())).collect::<Vec<_>>(), vec![(p, "owner".to_string(), vec!["lead".to_string()])]);
    assert!(!v.unanswered_owner.contains(&p), "owed by nobody");
    let notice = format!("{id}.p{p}.taken_back");
    let due = r.m.due("lead");
    assert_eq!(due.len(), 1, "{due:?}");
    assert!(due[0].id == notice && !due[0].inform && matches!(&due[0].what, What::TakenBack { n, by } if *n == p && by == "owner"), "{due:?}");
    // the turn it had begun on the post ends: its reply names him for nothing of it
    r.turn("lead", Some(&d), 7).reply.unwrap();
    let last = r.m.tickets[&t].posts.last().unwrap();
    assert!(last.by == "lead" && !last.names.iter().any(|a| a == "owner"), "{last:?}");
    // the notice, handed and not read: due again; handed and read: due no more
    let give = |r: &mut Run| r.go(Input::Given { t, to: "lead".into(), delivery: notice.clone(), upto: r.m.tickets[&t].mu.get("lead").copied().unwrap_or(0), skipped: Default::default(), place: None }).reply.unwrap();
    give(&mut r);
    assert!(r.m.due("lead").iter().all(|d| d.id != notice));
    r.go(Input::Unread { agent: "lead".into() }).reply.unwrap();
    assert!(r.m.due("lead").iter().any(|d| d.id == notice), "not read: due again");
    give(&mut r);
    assert!(r.m.due("lead").iter().all(|d| d.id != notice));
    assert_eq!(r.m.view(t).unwrap().struck[0].told, vec!["lead".to_string()]);
    // a take-back sent as an event from outside is refused: only a take-back writes it
    assert!(r.go(Input::Event { name: TAKEN_BACK.into(), value: json!({"t": t, "n": p, "by": "owner", "to": ["lead"]}) }).reply.is_err());
    r.replays();
}

/// A take-back's notice (`N-1.p5.taken_back`) beside another thread's post in one turn: its post struck, it only
/// informs (`informs_only`, the post read by `post_id`; consult-3 N-1396 p310, 2), so the turn is not "several" and
/// its last message is the other thread's reply, as it was before the fold.
#[test]
fn a_take_backs_notice_beside_another_threads_post_leaves_the_reply_on_that_thread() {
    let mut r = Run::new();
    let b = r.note("owner", "lead", "is the build green?");
    let d = r.give("lead", b);
    r.turn("lead", Some(&d), 1).reply.unwrap();
    r.post_on(b, "owner", "and ship the stills", &["lead"]).reply.unwrap();
    let p = r.m.tickets[&b].posts.last().unwrap().n;
    let d = r.give("lead", b);
    r.go(Input::TakeBack { t: b, n: p, by: "owner".into() }).reply.unwrap();
    r.turn("lead", Some(&d), 2).reply.unwrap();
    let on_b = r.m.tickets[&b].posts.len();
    // the next turn: w3's question on a thread of its own, and the notice
    let a = r.note("w3", "lead", "which median?");
    let q = r.give("lead", a);
    let notice = format!("{}.p{p}.taken_back", r.m.id(b));
    let mu = r.m.tickets[&b].mu.get("lead").copied().unwrap_or(0);
    r.go(Input::Given { t: b, to: "lead".into(), delivery: notice.clone(), upto: mu, skipped: Default::default(), place: None }).reply.unwrap();
    let given: std::collections::BTreeSet<String> = [q.clone(), notice.clone()].into();
    assert_eq!(r.m.informs_only("lead", &given), [notice].into(), "the notice only informs");
    assert!(r.m.reply_threads("lead", &[], false).is_none(), "one thread told: the rule Reply's case");
    r.turn("lead", Some(&q), 3).reply.unwrap();
    let x = &r.m.tickets[&a];
    assert_eq!((x.posts.len(), x.posts.last().map(|y| y.by.as_str())), (2, Some("lead")), "the reply is on w3's thread");
    assert_eq!(r.m.tickets[&b].posts.len(), on_b, "nothing on the thread whose post was taken back");
    r.replays();
}

/// The rule Reply's parts count a thread answered by hand only by what the agent did there once handed it (proto-mind
/// N-1446 p2631, docket-core p2632): its own post made before the giving, or in the same second, numbered past what
/// it was given, answers nothing, so the turn's words without an address may still go there; a post made in a later
/// second, or one answering a post it was given, does.
#[test]
fn the_rule_replys_parts_count_a_thread_answered_only_by_a_post_made_once_it_was_handed() {
    let mut r = Run::new();
    let hand = |r: &mut Run, t: u64, at: &str| {
        let d = r.m.due("lead").into_iter().find(|d| d.t == t).unwrap_or_else(|| panic!("lead is owed nothing on {t}"));
        r.at(Input::Given { t, to: "lead".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }, at).reply.unwrap();
        d.id
    };
    let own = |r: &mut Run, t: u64, at: &str, answers: Option<u32>| {
        r.at(Input::Post { t, by: "lead".into(), text: Text::New("on it".into()), names: vec![], told: vec![], inform: vec![], answers, kind: None }, at).reply.unwrap();
    };
    let answered = |r: &Run| -> Vec<(u64, bool)> { r.m.reply_threads("lead", &[], false).expect("several threads told").iter().map(|x| (x.t, x.answered)).collect() };
    // its post on A, about something else, after w2's question and before A is handed; A's question handed as an item
    // of its own, up to it, as the delivery program hands one (N-1446.p2623 with `upto` 2623, beside p2624): no answer
    let a = r.note("w2", "lead", "your half of the list?");
    r.post_on(a, "lead", "fix 6's head is up", &[]).reply.unwrap();
    let b = r.note("w4", "lead", "is 43 in the table?");
    let da = format!("{}.p1", r.m.id(a));
    r.go(Input::Given { t: a, to: "lead".into(), delivery: da.clone(), upto: 1, skipped: Default::default(), place: None }).reply.unwrap();
    r.give("lead", b);
    assert_eq!(answered(&r), vec![(a, false), (b, false)], "a post made before the giving answers nothing");
    assert_eq!(r.turn("lead", Some(&da), 1).reply, Err("No step was answered.".into()), "several threads: the machine posts nothing");
    // the next turn: in the giving's second, a post of its own on A answers nothing, one answering B's question does;
    // on C, a post in the next second does
    r.post_on(a, "w2", "and the mean?", &["lead"]).reply.unwrap();
    r.post_on(b, "w4", "and 44?", &["lead"]).reply.unwrap();
    let qb = r.m.tickets[&b].posts.last().unwrap().n;
    let c = r.note("w5", "lead", "which median?");
    let da = hand(&mut r, a, "2026-11-10T12:00:30Z");
    hand(&mut r, b, "2026-11-10T12:00:30Z");
    hand(&mut r, c, "2026-11-10T12:00:30Z");
    own(&mut r, a, "2026-11-10T12:00:30Z", None);
    own(&mut r, b, "2026-11-10T12:00:30Z", Some(qb));
    own(&mut r, c, "2026-11-10T12:00:31Z", None);
    assert_eq!(answered(&r), vec![(a, false), (b, true), (c, true)]);
    r.clock = 40;
    assert_eq!(r.turn("lead", Some(&da), 2).reply, Err("No step was answered.".into()));
    r.replays();
}

/// Line 327 with 0.2 B8 (consult-3, N-818 post 81): under his Hold, his take-back of a post an agent had is told to it
/// at once, not kept until Release, or the agent goes on acting on what he took back. A replay gives the same.
#[test]
fn under_his_hold_his_take_back_is_told_at_once() {
    let mut r = Run::new();
    let t = r.note("owner", "lead", "is the build green?");
    let d = r.give("lead", t);
    r.turn("lead", Some(&d), 5).reply.unwrap();
    r.post_on(t, "owner", "and ship the stills", &["lead"]).reply.unwrap();
    let p = r.m.tickets[&t].posts.last().unwrap().n;
    let _ = r.give("lead", t);
    assert_eq!(r.hold_mail(true), "Agents' mail is held: until Release, only the owner's posts pass.");
    assert!(r.go(Input::TakeBack { t, n: p, by: "owner".into() }).reply.unwrap().contains("lead told"));
    let notice = format!("{}.p{p}.taken_back", r.m.id(t));
    let told = r.m.due("lead").into_iter().find(|d| d.id == notice).expect("the notice is due");
    assert_eq!(r.m.mail_held(&told), None, "his take-back passes his hold");
    r.replays();
}

/// 0.2 B8 (N-1396; consult-3, N-818 p74): a post taken back while a hand-over carrying it was on its way is gone in the
/// record; when that hand-over is taken (`Input::StruckFor`), the agent had it: struck, and the agent told. Only a post
/// taken back is struck so, and a replay gives the same.
#[test]
fn a_post_taken_back_on_its_way_is_struck_for_the_agent_that_took_it() {
    let mut r = Run::new();
    let t = r.note("owner", "lead", "is the build green?");
    let d = r.give("lead", t);
    r.turn("lead", Some(&d), 5).reply.unwrap();
    r.post_on(t, "owner", "and ship the stills", &["lead"]).reply.unwrap();
    let p = r.m.tickets[&t].posts.last().unwrap().n;
    assert!(r.go(Input::StruckFor { t, n: p, to: "lead".into() }).reply.unwrap_err().contains("not taken back"));
    let out = r.go(Input::TakeBack { t, n: p, by: "owner".into() });
    assert!(out.reply.is_ok() && matches!(out.lines.as_slice(), [Line::TakenBack { .. }]), "gone: nobody has it in the record: {out:?}");
    assert!(r.m.view(t).unwrap().struck.is_empty());
    let out = r.go(Input::StruckFor { t, n: p, to: "lead".into() });
    assert!(out.reply.is_ok() && matches!(out.lines.as_slice(), [Line::Event { name, value, .. }] if name == TAKEN_BACK && value["to"] == json!(["lead"])), "{out:?}");
    assert_eq!(r.m.view(t).unwrap().struck.iter().map(|s| (s.post.n, s.to.clone())).collect::<Vec<_>>(), vec![(p, vec!["lead".to_string()])]);
    assert!(r.m.due("lead").iter().any(|d| matches!(&d.what, What::TakenBack { n, .. } if *n == p)));
    r.replays();
}

/// paper-research N-461, N-447 p4/p5: the asker withdraws what its post asks the owner, with why. The asks close as
/// Withdrawn, nobody is told, the ticket no longer waits on him; the owner is refused (his act is "nothing to
/// answer"), as are a why left out, his own post and a post that asks nothing open. On a closed agents' thread too.
#[test]
fn the_asker_withdraws_its_asks_with_why_and_nobody_is_told() {
    use crate::line::AsksClosed;
    let waits = |r: &Run, t: u64| r.m.view(t).unwrap().waits_on.iter().filter(|w| w.call == "asks" && w.agent == "owner").map(|w| w.post.unwrap()).collect::<Vec<_>>();
    let closed = |r: &Run, t: u64, n: u32| r.m.view(t).unwrap().posts.iter().find(|p| p.n == n).unwrap().asks_closed.clone();
    // a closed agents' thread: 3h's ask that waits on him
    let mut r = Run::before_asks();
    let t = r.note("lead", "owner", "the release: A or B?");
    r.asks(1, json!({"head": "Asks", "marks": [Run::mark([13, 19], &[])]}));
    assert_eq!(waits(&r, t), vec![1]);
    let refused = |r: &mut Run, by: &str, n: u32, why: &str| r.go(Input::WithdrawAsks { t, n, by: by.into(), why: why.into() }).reply.unwrap_err();
    assert!(refused(&mut r, "owner", 1, "no").contains("nothing to answer"), "the owner's act is his own");
    assert!(refused(&mut r, "lead", 1, "  ").contains("why"));
    assert!(refused(&mut r, "lead", 1, "two\nlines").contains("one line"), "a why is one line (N-447 p19, 3)");
    assert!(refused(&mut r, "lead", 7, "x").contains("no post 7"));
    assert_eq!(r.m.others_open_asks(&r.m.tickets[&t], "fixer"), vec![("lead".to_string(), 1)], "what an end by another refuses on");
    assert!(r.m.others_open_asks(&r.m.tickets[&t], "lead").is_empty(), "the ender's own are not");
    let out = r.go(Input::WithdrawAsks { t, n: 1, by: "lead".into(), why: " overtaken by B ".into() });
    assert!(out.reply.is_ok() && matches!(out.lines.as_slice(), [Line::AsksWithdrawn { .. }]), "{out:?}");
    assert!(out.effects.iter().all(|e| matches!(e, Effect::Changed { .. })), "nobody is told: {:?}", out.effects);
    assert_eq!((waits(&r, t), closed(&r, t, 1)), (vec![], Some(AsksClosed::Withdrawn { by: "lead".into(), why: "overtaken by B".into() })));
    assert!(refused(&mut r, "lead", 1, "again").contains("asks nothing open"));
    r.replays_today();
    // a resting thread: one asker withdraws, the other's ask still waits; the owner's post is refused
    let mut r = Run::before_asks();
    let t = r.say("lead", &["fixer"], "the release: what is left?");
    let [f, l] = r.reply_at_once(["fixer", "lead"], 10);
    r.asks(f, json!({"head": "Asks", "marks": [Run::mark([0, 5], &[])]}));
    r.asks(l, json!({"head": "Asks", "marks": [Run::mark([0, 5], &[])]}));
    r.minder(t, json!({"head": "Rest"}));
    let before = waits(&r, t);
    assert!(before.contains(&f) && before.contains(&l), "{before:?}");
    r.go(Input::WithdrawAsks { t, n: f, by: "fixer".into(), why: "found it in the log".into() }).reply.unwrap();
    assert_eq!(waits(&r, t), vec![l]);
    assert_eq!(closed(&r, t, f), Some(AsksClosed::Withdrawn { by: "fixer".into(), why: "found it in the log".into() }));
    r.post_on(t, "owner", "noted", &[]).reply.unwrap();
    let o = r.m.tickets[&t].posts.last().unwrap().n;
    assert!(r.go(Input::WithdrawAsks { t, n: o, by: "lead".into(), why: "x".into() }).reply.unwrap_err().contains("the owner's"));
    r.replays_today();
}

/// The watcher's `asks` gone (proto-lead N-1479 p247): a post typed `ask` to the owner asks him from its push, as a
/// marked post asks from its marks (`Post.asks_open`), and closes only on his or its asker's own act on it
/// (paper-research N-1446 p1102; proto-window N-1479 p275). On a closed agents' thread: the lead's untyped post to him
/// asks him nothing of its own, nor its typed ask to fixer; its typed ask to him waits on him, and the view says it is
/// open; fixer's "@lead …" and the lead's own next post leave it waiting, and his "nothing to answer" closes it. A
/// second waits until the lead withdraws it (`coord ask --drop`: his list before the drop and after), a third until his
/// answer from the card. A fourth, moved by him, waits where it lands.
#[test]
fn a_post_typed_ask_to_the_owner_waits_on_him_until_he_or_its_asker_closes_it() {
    use crate::line::AsksClosed;
    let (waits, shown) = (waits_on_him, shown_open);
    let mut r = Run::new();
    let t = r.note("lead", "owner", "the release is cut");
    r.nothing_asked();
    assert_eq!(waits(&r, t), Vec::<u32>::new(), "an untyped post to him asks nothing of its own");
    let ask = |r: &mut Run, t: u64, to: &str, text: &str| typed_ask(r, t, "lead", to, text);
    ask(&mut r, t, "fixer", "run the bench on B");
    assert_eq!(waits(&r, t), Vec::<u32>::new(), "an ask to fixer asks him nothing");
    let first = ask(&mut r, t, "owner", "A or B?");
    assert_eq!((waits(&r, t), shown(&r, t, first)), (vec![first], true), "it waits on him, and the view says it is open");
    // another's post waking the asker, and the asker's own next post: neither is his answer
    r.post_on(t, "fixer", "@lead the bench says B", &["lead"]).reply.unwrap();
    r.nothing_asked();
    r.post_on(t, "lead", "@fixer once more with the new seed", &["fixer"]).reply.unwrap();
    r.nothing_asked();
    assert_eq!((waits(&r, t), shown(&r, t, first)), (vec![first], true), "fixer's post and the lead's own leave it waiting");
    // his "nothing to answer"
    assert_eq!(closed_how(&r, t, first), None, "open: nothing closed it");
    r.go(Input::NothingToAnswer { t, n: first, by: "owner".into() }).reply.unwrap();
    assert_eq!((waits(&r, t), shown(&r, t, first)), (vec![], false), "his nothing to answer closes it");
    assert_eq!(closed_how(&r, t, first), Some(AsksClosed::NothingToAnswer), "and the view says so");
    // the asker's drop
    let second = ask(&mut r, t, "owner", "ship B tonight?");
    assert_eq!(waits(&r, t), vec![second], "his list before the drop");
    r.go(Input::WithdrawAsks { t, n: second, by: "lead".into(), why: "B ships tomorrow with the rest".into() }).reply.unwrap();
    assert_eq!((waits(&r, t), shown(&r, t, second)), (vec![], false), "his list after the drop");
    assert_eq!(closed_how(&r, t, second), Some(AsksClosed::Withdrawn { by: "lead".into(), why: "B ships tomorrow with the rest".into() }));
    // his answer from the card, typed to the lead (base.kind: his answer goes to whom it was typed to, no call)
    let third = ask(&mut r, t, "owner", "the notes: short or long?");
    assert_eq!(waits(&r, t), vec![third]);
    r.go(Input::Post { t, by: "owner".into(), text: Text::New("short".into()), names: vec!["lead".into()], told: vec![], inform: vec![], answers: Some(third), kind: None }).reply.unwrap();
    assert_eq!((waits(&r, t), shown(&r, t, third)), (vec![], false), "his answer closes it");
    assert_eq!(closed_how(&r, t, third), Some(AsksClosed::Answered { post: r.m.tickets[&t].last(), by: "owner".into() }));
    // moved by him: the ask goes with the post, open where it lands
    let fourth = ask(&mut r, t, "owner", "the tag: v2 or v2.0?");
    let u = r.note("lead", "owner", "the tags");
    r.nothing_asked();
    r.go(Input::Move { t, n: fourth, to: Some(u), by: "owner".into() }).reply.unwrap();
    let landed = r.m.tickets[&u].last();
    assert_eq!((waits(&r, t), waits(&r, u), shown(&r, u, landed)), (vec![], vec![landed], true), "it waits where it landed");
    r.replays();
}

/// BY's post on T typed an ask to TO, named as coordd names whom a typed post asks (`kind_taken`), the watcher's
/// `asks` of a thread opened before answered Nothing: its number.
fn typed_ask(r: &mut Run, t: u64, by: &str, to: &str, text: &str) -> u32 {
    r.go(Input::Post { t, by: by.into(), text: Text::New(text.into()), names: vec![to.into()], told: vec![], inform: vec![], answers: None, kind: Some(PostKind::Ask { to: vec![to.into()] }) }).reply.unwrap();
    r.nothing_asked();
    r.m.tickets[&t].last()
}

/// The posts of T whose asks wait on the owner, as its view's `waits_on` lists them (the latest of each agent).
fn waits_on_him(r: &Run, t: u64) -> Vec<u32> {
    r.m.view(t).unwrap().waits_on.iter().filter(|w| w.call == "asks" && w.agent == "owner").map(|w| w.post.unwrap()).collect()
}

/// Whether T's view shows the asks of its post N open.
fn shown_open(r: &Run, t: u64, n: u32) -> bool {
    r.m.view(t).unwrap().posts.iter().find(|p| p.n == n).unwrap().asks_open
}

/// How T's view says the asks of its post N were closed.
fn closed_how(r: &Run, t: u64, n: u32) -> Option<crate::line::AsksClosed> {
    r.m.view(t).unwrap().posts.iter().find(|p| p.n == n).unwrap().asks_closed.clone()
}

/// proto-window N-1479 p294, 1: a step of the thread given the asker after its typed ask to the owner is no answer of
/// his, and the ask stays open (a marked post's closes there: `AsksClosed::Told`).
#[test]
fn a_step_given_its_asker_leaves_a_typed_ask_to_the_owner_open() {
    let mut r = Run::new();
    r.m.defs.edit().block_ask = true;
    let t = r.say("lead", &["lead"], "add range(xs)");
    r.replies("lead", 10);
    let n = typed_ask(&mut r, t, "lead", "owner", "range(xs) or range(lo, hi)?");
    r.minder(t, json!({"head": "Program", "text": "r <- step lead \"Go ahead.\" -> Done | Cannot(why: Text);\n..."}));
    let d = r.take("lead");
    assert!(matches!(d.what, What::Step { .. }) && d.upto >= n, "the step, and the posts up to the ask: {d:?}");
    assert!(shown_open(&r, t, n) && r.m.asks_open(&r.m.tickets[&t], n), "the step given it leaves it open");
    r.replays();
}

/// paper-research N-1479 p318 (proto-window p319): his cancel of a thread closes the typed ask open on it, and the
/// view says so, as for a marked post (`AsksClosed::Cancelled`).
#[test]
fn his_cancel_closes_a_typed_ask_and_the_view_says_so() {
    let mut r = Run::new();
    let t = r.say("lead", &["lead"], "the release: what is left?");
    r.replies("lead", 10);
    let n = typed_ask(&mut r, t, "lead", "owner", "ship Friday?");
    assert!(r.m.asks_open(&r.m.tickets[&t], n));
    r.cancel(t, "owner", "done with it").reply.unwrap();
    assert!(!r.m.asks_open(&r.m.tickets[&t], n));
    assert_eq!((closed_how(&r, t, n), r.m.asks_closed(&r.m.tickets[&t], n)), (Some(crate::line::AsksClosed::Cancelled), Some(crate::line::AsksClosed::Cancelled)));
    r.replays();
}

/// paper-research N-1479 p300 (proto-window p294, 2; consult-3 p301): a thread following a closed thread closes a typed
/// ask to the owner there only when it is his answer: his words opened it, and it reaches the asker (typed to it, or
/// its first post waking it). An agent's following thread and his thread typed to fixer leave the lead's ask waiting;
/// his thread typed to the lead closes it. His words at the foot typed to nobody are the routing's (`would_close`).
#[test]
fn a_typed_ask_closes_on_a_following_thread_only_when_his_reaches_its_asker() {
    use crate::line::{Closes, WouldClose};
    let mut r = Run::new();
    let t = r.note("lead", "owner", "the release is cut");
    let n = typed_ask(&mut r, t, "lead", "owner", "A or B?");
    let id = r.m.id(t);
    assert_eq!((waits_on_him(&r, t), shown_open(&r, t, n)), (vec![n], true));
    let wc = r.m.view(t).unwrap().posts.iter().find(|p| p.n == n).unwrap().would_close.clone();
    assert_eq!(wc, Some(WouldClose { foot: Closes::Routed, reply: Closes::Yes, subthread: Closes::Yes }), "his words at the foot, typed to nobody: the routing's");
    // fixer's thread following it, its first post to the lead
    let form = json!({"to": "fixer", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "fixer"}, "follows": id});
    r.go(Input::Open { kind: "thread".into(), under: None, opener: "fixer".into(), form, text: Some(Text::New("@lead the bench, meanwhile".into())), names: vec!["lead".into()], told: vec![] }).reply.unwrap();
    r.nothing_asked();
    assert_eq!((waits_on_him(&r, t), shown_open(&r, t, n)), (vec![n], true), "an agent's following thread is no answer of his");
    // his words for it, typed to fixer, then to the lead
    let follow = |r: &mut Run, to: &str, text: &str| {
        r.go(Input::Event { name: "owner_message".into(), value: json!({"from": "owner", "to": to, "typed": [to], "names": [], "text": text, "addressed": {"head": "Named"}, "follows": id}) }).reply.unwrap();
        r.nothing_asked();
    };
    follow(&mut r, "fixer", "rerun the bench first");
    assert_eq!((waits_on_him(&r, t), shown_open(&r, t, n)), (vec![n], true), "his thread typed to fixer leaves it waiting");
    assert_eq!(closed_how(&r, t, n), None);
    follow(&mut r, "lead", "B");
    assert_eq!((waits_on_him(&r, t), shown_open(&r, t, n)), (vec![], false), "his thread typed to the lead closes it");
    let f = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    assert_eq!(closed_how(&r, t, n), Some(crate::line::AsksClosed::Followed { ticket: r.m.id(f) }), "and the view says which");
    r.replays();
}

/// paper-research N-1479 p305 (proto-reader p295): his post aimed at one of the lead's typed asks (`answers`) closes
/// only that one, and the lead's other stays open; one that also answers another (`also`) closes that too; his plain
/// post to the lead closes every earlier ask of the lead's on the thread.
#[test]
fn his_answer_aimed_at_one_typed_ask_leaves_the_askers_others_open_and_his_plain_post_closes_them() {
    let mut r = Run::new();
    let t = r.note("lead", "owner", "the release is cut");
    let open = |r: &Run, ns: &[u32]| ns.iter().map(|&n| shown_open(r, t, n)).collect::<Vec<_>>();
    let his = |r: &mut Run, text: &str, answers: Option<u32>| -> u32 {
        r.go(Input::Post { t, by: "owner".into(), text: Text::New(text.into()), names: vec!["lead".into()], told: vec![], inform: vec![], answers, kind: None }).reply.unwrap();
        r.m.tickets[&t].last()
    };
    let a = typed_ask(&mut r, t, "lead", "owner", "the notes: short or long?");
    let b = typed_ask(&mut r, t, "lead", "owner", "the tag: v2 or v2.0?");
    assert_eq!((waits_on_him(&r, t), open(&r, &[a, b])), (vec![b], vec![true, true]));
    his(&mut r, "short", Some(a));
    assert_eq!((waits_on_him(&r, t), open(&r, &[a, b])), (vec![b], vec![false, true]), "his answer to the notes leaves the tag waiting");
    his(&mut r, "thanks, go ahead", None);
    assert_eq!((waits_on_him(&r, t), open(&r, &[a, b])), (vec![], vec![false, false]), "his plain post answers what is left");
    // aimed at one, and also at another
    let c = typed_ask(&mut r, t, "lead", "owner", "the date: Monday?");
    let d = typed_ask(&mut r, t, "lead", "owner", "the list: dev or all?");
    let e = typed_ask(&mut r, t, "lead", "owner", "the blog post too?");
    let both = his(&mut r, "Monday, to all", Some(c));
    assert_eq!(open(&r, &[c, d, e]), vec![false, true, true]);
    r.go(Input::AnswersAlso { t, n: both, also: vec![format!("{}.p{d}", r.m.id(t))] }).reply.unwrap();
    assert_eq!((waits_on_him(&r, t), open(&r, &[c, d, e])), (vec![e], vec![false, false, true]), "it also answers the list; the blog post waits");
    r.replays();
}

/// proto-reader N-1479 p295: a typed ask to the owner that its asker takes back before anyone is handed it asks
/// nothing: the views do not draw it, and it waits on nobody (a take-back closes it, p1102).
#[test]
fn a_typed_ask_taken_back_waits_on_nobody() {
    let mut r = Run::new();
    let t = r.note("lead", "owner", "the release is cut");
    // taken back before anyone is handed it (on a thread opened before the watcher's `asks` went, the run reading it is
    // cancelled with it)
    r.go(Input::Post { t, by: "lead".into(), text: Text::New("A or B?".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: None, kind: Some(PostKind::Ask { to: vec!["owner".into()] }) }).reply.unwrap();
    let n = r.m.tickets[&t].last();
    assert_eq!(waits_on_him(&r, t), vec![n]);
    r.go(Input::TakeBack { t, n, by: "lead".into() }).reply.unwrap();
    r.nothing_asked();
    assert!(r.m.tickets[&t].posts.iter().any(|p| p.n == n && p.taken_back));
    assert_eq!((waits_on_him(&r, t), r.m.asks_open(&r.m.tickets[&t], n)), (vec![], false), "taken back, it waits on nobody");
    r.replays();
}

/// proto-window N-1479 p294, 3 (paper-research p300, 3): an agent's end is refused while another agent's ask typed to
/// the owner is open, as for a marked post (N-447 p2), and the asker's own end withdraws it with the end's why.
#[test]
fn an_end_is_refused_while_anothers_typed_ask_to_the_owner_is_open_and_the_askers_own_withdraws_it() {
    let mut r = Run::new();
    let t = r.note("lead", "owner", "the release is cut");
    let n = typed_ask(&mut r, t, "lead", "owner", "A or B?");
    let id = r.m.id(t);
    let out = r.go(Input::End { t, by: "fixer".into(), how: "kept".into(), why: "over".into() });
    assert_eq!(out.reply, Err(format!("{id} waits on the owner: what post {n} (lead) asks him is open. The owner ends it, or the asker withdraws the asks first.")));
    assert!(out.lines.is_empty() && r.m.tickets[&t].ended.is_none());
    r.go(Input::End { t, by: "lead".into(), how: "dropped".into(), why: "B is out".into() }).reply.unwrap();
    assert_eq!((waits_on_him(&r, t), r.m.asks_open(&r.m.tickets[&t], n)), (vec![], false));
    assert_eq!(r.m.tickets[&t].asks_withdrawn.get(&n).map(|(_, by, why)| (by.as_str(), why.as_str())), Some(("lead", "B is out")), "withdrawn with the end's why");
    r.replays();
}

/// paper-research N-447 p6, p7: one answer to "does it owe the owner": `Machine::owes_owner`, the view's `owed` and
/// the outline's `owed` agree; a closed thread whose ask to him is open owes him and is listed open, "waits on you"
/// (N-16 on research's record was closed and owed, and the outline said "done"); his "nothing to answer" ends it.
#[test]
fn what_owes_the_owner_is_one_answer_closed_threads_too() {
    let all = |r: &Run, t: u64| {
        let x = &r.m.tickets[&t];
        let o = crate::view::ViewOutline::of(&r.m, x, false);
        (r.m.owes_owner(x), r.m.view(t).unwrap().owed, o.owed, o.open, o.state)
    };
    // a thread answered and closed as a one-off; then an ask of its agent to him on it
    let mut r = Run::before_asks();
    let t = r.say("lead", &[], "the release: A or B?");
    r.intake(t, json!({"head": "Thread"}));
    let n = r.replies("lead", 10);
    r.minder(t, json!({"head": "Rest"}));
    r.go(Input::Edit { t, by: "owner".into(), program: "return OneOff".into(), why: "answered".into() }).reply.unwrap();
    assert!(!r.m.tickets[&t].is_open(), "{:?}", r.m.tickets[&t].status);
    assert_eq!(all(&r, t).0, false);
    r.post_on(t, "lead", "and one more: ship today?", &["owner"]).reply.unwrap();
    let q = r.m.tickets[&t].posts.last().unwrap().n;
    assert!(q > n);
    r.asks(q, json!({"head": "Asks", "marks": [Run::mark([14, 25], &[])]}));
    assert_eq!(all(&r, t), (true, true, true, true, "waits on you".to_string()), "closed, and it owes him");
    r.go(Input::NothingToAnswer { t, n: q, by: "owner".into() }).reply.unwrap();
    let (a, b, c, open, _) = all(&r, t);
    assert_eq!((a, b, c, open), (false, false, false, false));
    // an agents' thread, closed from the start: the same
    let mut r = Run::before_asks();
    let t = r.note("lead", "owner", "the release: A or B?");
    assert_eq!(all(&r, t).0, false);
    r.asks(1, json!({"head": "Asks", "marks": [Run::mark([13, 19], &[])]}));
    assert_eq!(all(&r, t), (true, true, true, true, "waits on you".to_string()));
    r.replays_today();
}

/// docket-core3's note on 8e15085 (N-447 p104): the view's `owed` is read off its `waits_on`, the outline's off
/// `outline_waits`; they give the same answer as `owes_owner` for a halt down the tree (the parent's and the halted
/// child's) and for the owner's step in a handler under a thread that rests, before and after each is answered.
#[test]
fn owes_owner_the_view_and_the_outline_agree_on_a_halt_below_and_a_handlers_step() {
    let three = |r: &Run, t: u64| {
        let x = &r.m.tickets[&t];
        (r.m.owes_owner(x), r.m.view(t).unwrap().owed, crate::view::ViewOutline::of(&r.m, x, false).owed)
    };
    // a halt down the tree: the slow job's child halted at its open, then at its step's give
    let mut r = Run::slow_run();
    let (j, w) = r.slow_job();
    assert_eq!((three(&r, j), three(&r, w)), ((true, true, true), (true, true, true)), "halted at the open");
    r.approve(w, 1);
    assert_eq!((three(&r, j), three(&r, w)), ((true, true, true), (true, true, true)), "halted at the give");
    r.approve(w, 2);
    r.take("dev");
    assert_eq!((three(&r, j), three(&r, w)), ((false, false, false), (false, false, false)), "dev holds the step");
    // the owner's step in a handler under a thread at its hole: clarify, then answered
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.owner_says(t, &["lead"], "do the other one");
    r.route(json!({"head": "Unclear", "why": "which one is the other one?"}));
    let h = r.handler().unwrap();
    assert_eq!(r.m.view(t).unwrap().status, ViewStatus::Resting);
    assert_eq!((three(&r, t), three(&r, h)), ((true, true, true), (true, true, true)), "clarify waits on him");
    // his press answers it (typed words never do, N-807.p78)
    r.go(Input::Answer { t: h, by: "owner".into(), head: json!({"head": "Send", "to": ["fixer"]}), text: Text::New(String::new()) }).reply.unwrap();
    assert_eq!(three(&r, t), (false, false, false), "answered");
    r.replays();
}

#[test]
fn a_star_marks_a_ticket_or_a_post_by_whoever_set_it_and_does_nothing_else() {
    let mut r = Run::new();
    let t = r.say("lead", &["fixer"], "the parser");
    let before = r.m.view(t).unwrap();
    assert!(!before.starred && before.stars.is_empty());
    let out = r.go(Input::Star { t, on: true, by: "owner".into(), post: None });
    assert!(out.reply.is_ok() && out.effects == vec![Effect::Changed { t }], "{out:?}");
    let v = r.m.view(t).unwrap();
    assert!(v.starred, "the owner's star on the thread orders his list");
    assert_eq!(TicketView { starred: false, stars: Vec::new(), ..v.clone() }, before, "and changes nothing else");
    assert!(r.go(Input::Star { t, on: true, by: "owner".into(), post: None }).lines.is_empty(), "starred already: no line");
    // an agent marks a post (the owner N-234.p3: "the agents need to be able to mark too"); its star is its own
    r.go(Input::Star { t, on: true, by: "lead".into(), post: Some(1) }).reply.unwrap();
    assert!(r.go(Input::Star { t, on: true, by: "lead".into(), post: Some(9) }).reply.is_err(), "no post 9");
    let v = r.m.view(t).unwrap();
    assert_eq!(v.stars.iter().map(|s| (s.post, s.by.as_str())).collect::<Vec<_>>(), vec![(None, "owner"), (Some(1), "lead")]);
    r.go(Input::Star { t, on: false, by: "owner".into(), post: None }).reply.unwrap();
    let v = r.m.view(t).unwrap();
    assert!(!v.starred && v.stars.len() == 1, "the owner's off, the agent's kept");
    // his click that opens the starred post (paper-research N-101.p1295): his unstar of it ends every star on it
    r.go(Input::Star { t, on: true, by: "fixer".into(), post: Some(1) }).reply.unwrap();
    let out = r.go(Input::Star { t, on: false, by: "owner".into(), post: Some(1) });
    assert_eq!(out.reply.as_deref(), Ok(format!("{}.p1 opened by you; its stars end.", r.m.id(t)).as_str()));
    assert!(matches!(out.lines.as_slice(), [Line::Star { on: false, .. }]), "a line: {:?}", out.lines);
    assert!(r.m.view(t).unwrap().stars.is_empty());
    r.replays();
}

#[test]
fn an_answer_to_a_step_and_the_owners_words_are_not_converted() {
    let mut r = Run::new();
    let t = r.say("lead", &["fixer"], "the parser");
    r.post_on(t, "owner", "and the lexer", &[]);
    r.route(json!({"head": "To", "agents": ["lead", "fixer"]}));
    assert!(r.converting().is_empty());
    // a step's answer posts nothing that is converted
    r.settle(10);
    r.minder(t, json!({"head": "Program", "text": "r <- step fixer \"Fix it.\" -> Done | Cannot(why: Text);\n..."}));
    let d = r.m.due("fixer");
    r.go(Input::Given { t, to: "fixer".into(), delivery: d[0].id.clone(), upto: d[0].upto, skipped: Default::default(), place: Some(Run::loc(20)) });
    let out = r.go(Input::Answer { t, by: "fixer".into(), head: json!({"head": "Done"}), text: Text::New("done".into()) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(r.converting().is_empty(), "{:?}", r.converting());
    r.replays();
}

/// The tree as lines: each node indented under its parent, `what state [source] text [who] = value -> tickets`.
fn tree(ns: &[crate::view::RunNode], ind: usize, out: &mut Vec<String>) {
    for n in ns {
        let mut s = format!("{}{} {} [{}{}] {}", " ".repeat(ind), n.what, n.state, n.source, n.by.as_ref().map(|b| format!(" by {b}")).unwrap_or_default(), n.text);
        if let Some(w) = &n.who {
            s.push_str(&format!(" ({w})"));
        }
        if let Some(v) = &n.value {
            s.push_str(&format!(" = {v}"));
        }
        if !n.tickets.is_empty() {
            s.push_str(&format!(" -> {}", n.tickets.join(", ")));
        }
        out.push(s);
        tree(&n.nodes, ind + 2, out);
    }
}

fn tree_of(r: &Run, t: u64) -> String {
    let mut out = Vec::new();
    tree(&r.m.view(t).unwrap().run, 0, &mut out);
    out.join("\n")
}

#[test]
fn the_tree_of_a_thread_says_what_was_done_where_it_stands_and_what_the_minder_wrote() {
    let mut r = Run::new();
    let t = r.say("lead", &[], "add median(xs) to calc.py");
    let s = tree_of(&r, t);
    assert!(s.starts_with("match now [0] match addressed\n  arm not_taken [0] Sent(by)\n    return not_taken [0] return Between\n  arm now [0] _\n    gather done [0] text <- gather text = \"add median(xs) to calc.py\"\n    match now [0] v <- match addressed\n      arm not_taken [0] Named\n        return not_taken [0] return Thread\n      arm now [0] _\n        step now [0] step watcher intake(text, to) (watcher)\n    match to_come [0] match v\n      arm to_come [0] Place(t)"), "{s}");
    r.intake(t, json!({"head": "Thread"}));
    r.settle(10);
    let s = tree_of(&r, t);
    assert!(s.contains("        step done [0] step watcher intake(text, to) (watcher) = {\"head\":\"Thread\"}\n    match now [0] match v\n      arm not_taken [0] Place(t)\n        match not_taken [0] match opener\n          arm not_taken [0] none\n            post not_taken [0] post text on t naming typed\n          arm not_taken [0] o\n            post not_taken [0] post text on t as o naming typed\n        return not_taken [0] return Moved(t)\n"), "{s}");
    assert!(s.contains("      arm now [0] Thread\n        match done [0] match opener\n          arm not_taken [0] none\n            post not_taken [0] post text naming to, names\n          arm done [0] o\n            post done [0] post text as o naming to, names (lead)\n        hole now [0] ...\n      arm not_taken [0] Unsure(ts)\n        match not_taken [0] match opener\n          arm not_taken [0] none\n            post not_taken [0] post text naming to, names\n          arm not_taken [0] o\n            post not_taken [0] post text as o naming to, names"), "{s}");
    r.minder(t, json!({"head": "Program", "text": "r <- step lead \"Go ahead.\" -> Done | Cannot(why: Text);\nmatch r {\n  Done => ...\n  Cannot(why) => return Kept\n}"}));
    let s = tree_of(&r, t);
    let v = r.m.view(t).unwrap();
    let hole = &v.run[0].nodes[1].nodes[2].nodes[1].nodes[1];
    assert_eq!((hole.what.as_str(), hole.nodes[0].at.as_ref()), ("hole", Some(&v.shaped[0].at)), "reached when the minder answered");
    assert!(hole.at.as_ref().is_some_and(|a| *a < v.shaped[0].at) && v.run[0].nodes[1].nodes[2].nodes[2].at.is_none(), "the hole earlier; an arm not taken never");
    assert!(s.contains("        hole now [0] ...\n          step now [1] r <- step lead \"Go ahead.\" -> Done | Cannot(why: Text) (lead)\n          match to_come [1] match r\n            arm to_come [1] Done\n              hole to_come [1] ...\n"), "{s}");
    r.answer(t, "lead", json!({"head": "Done"}));
    r.settle(10);
    r.minder(t, json!({"head": "Program", "text": "x <- par(step lead \"A\" -> Done, step fixer \"B\" -> Done);\n..."}));
    let kids: Vec<u64> = r.m.tickets[&t].children.clone();
    let (a, b) = (r.m.id(kids[kids.len() - 2]), r.m.id(kids[kids.len() - 1]));
    r.answer(*kids.last().unwrap(), "fixer", json!({"head": "Done"}));
    let s = tree_of(&r, t);
    assert!(s.contains(&format!("            arm now [1] Done\n              hole now [1] ...\n                par now [2] x <- par -> {a}, {b}\n                  branch now [2] branch 1\n                    step now [2] step lead \"A\" -> Done (lead)\n                  branch done [2] branch 2\n                    step done [2] step fixer \"B\" -> Done (fixer) = {{\"head\":\"Done\"}}\n                hole to_come [2] ...\n            arm not_taken [1] Cannot(why)\n")), "{s}");
    r.answer(kids[kids.len() - 2], "lead", json!({"head": "Done"}));
    r.settle(10);
    r.minder(t, json!({"head": "Close", "result": {"head": "Kept"}}));
    let s = tree_of(&r, t);
    assert!(!s.contains("     now ") && !s.contains("    to_come"), "closed: {s}");
    assert!(s.contains("                hole done [2] ... = {\"head\":\"Kept\"}\n"), "{s}");
    let v = r.m.view(t).unwrap();
    assert_eq!((v.run[0].nodes[1].nodes[2].nodes[1].nodes[1].nodes[1].nodes[0].nodes[0].nodes[0].source, v.shaped[1].n), (2, 2), "the minder's second answer");
    // undone: it rests at the hole again
    r.go(Input::Undo { t, by: "owner".into() }).reply.unwrap();
    let s = tree_of(&r, t);
    assert!(s.contains("                hole now [2] ...\n            arm not_taken [1] Cannot(why)"), "{s}");
    r.replays();
}

/// `P; Q` is `_ <- P; Q` (paper-research m1409); a `;` between arms still ends an arm; a pattern after another on
/// the next line with no `;` is refused with what to write.
#[test]
fn a_pattern_then_a_semicolon_is_a_sequence_and_a_missing_one_is_named() {
    use crate::syntax::{Bound, Prog, parse_prog};
    let p = parse_prog("step lead \"Go ahead.\" -> Done | Cannot(why: Text);\n...", 1).unwrap();
    assert!(matches!(&p, Prog::Bind { x: Bound::Var(x), first, rest } if x == "_" && matches!(**first, Prog::Step { .. }) && matches!(**rest, Prog::Hole { .. })), "{p:?}");
    assert_eq!(crate::print::prog(&p, 0), "_ <- step lead \"Go ahead.\" -> Done | Cannot(why: Text);\n...");
    assert_eq!(parse_prog(&crate::print::prog(&p, 0), 1).unwrap(), p, "printed as a `_ <-`, which reads back the same");
    // between arms, of a match and of a handle, a `;` ends the arm
    let m = parse_prog("match v { A => return 1; B => post \"x\"; return 2; C(c) => return 3 }", 1).unwrap();
    let Prog::Match { arms, .. } = &m else { panic!("{m:?}") };
    assert_eq!(arms.len(), 3, "{m:?}");
    assert!(matches!(&arms[1].1, Prog::Bind { rest, .. } if matches!(**rest, Prog::Return { .. })), "B's arm is post then return");
    let h = parse_prog("try { post \"a\"; return 1 } handle { Silent => return 2; NoWorker(w) => ... }", 1).unwrap();
    assert!(matches!(&h, Prog::Try { arms, .. } if arms.len() == 2), "{h:?}");
    assert!(parse_prog("par(post \"a\"; return 1, return 2)", 1).is_ok());
    // no `;`: the reason says where it goes
    assert_eq!(parse_prog("step lead \"Go ahead.\" -> Done | Cannot(why: Text)\n...", 1).unwrap_err(), "line 2: `...` follows the pattern on line 1 with no `;` between them: end line 1 with `;`");
    assert_eq!(parse_prog("match r {\n  Done => post \"a\"\n    return 1\n}", 1).unwrap_err(), "line 3: `return` follows the pattern on line 2 with no `;` between them: end line 2 with `;`");
    // the minder's answer as luna wrote it: taken
    let mut r = Run::new();
    let t = r.say("lead", &[], "fix the parser");
    r.intake(t, json!({"head": "Thread"}));
    r.settle(10);
    let out = r.minder(t, json!({"head": "Program", "text": "step lead \"Go ahead.\" -> Done | Cannot(why: Text);\n..."}));
    assert!(matches!(r.status(t), ViewStatus::Awaits { ref agent, .. } if agent == "lead"), "{:?} {:?}", r.status(t), out.reply);
    r.replays();
}

/// `[x]`, a list of exactly one (paper-research m1418), and the base kind's arm that uses it: the owner's words typed
/// to nobody on a ticket with one agent go to that agent with no route call; with two agents, route as before.
#[test]
fn words_typed_to_nobody_on_a_ticket_with_one_agent_need_no_call() {
    use crate::syntax::{Pat, Prog, parse_prog};
    let check = |text: &str| {
        let mut p = parse_prog(text, 1).unwrap();
        let scope = BTreeMap::from([("xs".to_string(), Type::List(Box::new(Type::Agent)))]);
        crate::check::check_in(&mut p, &Type::Int, &scope, &[], &BTreeMap::new(), &BTreeMap::new(), true)
    };
    assert!(check("match xs { [] => return 0  [a] => return 1  _ => return 2 }").is_ok());
    assert!(check("match xs { [] => return 0  [_] => return 1 }").is_err(), "two or more is not matched");
    assert!(check("match xs { [] => return 0  _ => return 2  [a] => return 1 }").is_err(), "[a] after _ is dead");
    assert!(check("match xs { [a] => return 1  [b] => return 2  _ => return 0 }").is_err(), "the second [b] is dead");
    assert!(parse_prog("match xs { [a, b] => return 1 }", 1).unwrap_err().contains("a list pattern is `[]` or `[x]`"));
    let one = parse_prog("match xs { [a] => return 1 }", 1).unwrap();
    let Prog::Match { arms, .. } = &one else { panic!() };
    assert!(matches!(&arms[0].0, Pat::One(_)) && crate::print::pat(&arms[0].0) == "[a]");
    // lead alone on the thread: the owner's words typed to nobody go to lead, no call
    let mut r = Run::new();
    let t = r.say("lead", &["lead"], "the parser");
    r.settle(10);
    r.nothing_asked();
    r.post_on(t, "owner", "and the lexer", &[]).reply.unwrap();
    assert!(r.handler().is_none(), "no route step: the handler ended");
    assert!(r.m.due(WATCHER).is_empty());
    assert!(r.m.due("lead").iter().any(|d| d.t == t), "lead is given the words");
    // lead and fixer on it: route as before
    r.settle(12);
    r.nothing_asked();
    r.post_on(t, "lead", "@fixer have a look", &["fixer"]).reply.unwrap();
    r.settle(14);
    r.nothing_asked();
    r.post_on(t, "owner", "and the tests", &[]).reply.unwrap();
    r.route(json!({"head": "To", "agents": ["fixer"]}));
    r.replays();
}

/// `none`, an optional with no value (paper-research m1442), and the base kind's arm that uses it (m1437): the
/// owner's answers to what a post asks go to whom they were typed to, with no call; his plain words typed to the same
/// agent on a ticket with another agent on it are routed as before.
#[test]
fn the_owners_answers_go_to_whom_they_were_typed_to_with_no_call() {
    use crate::syntax::{Pat, Prog, parse_prog};
    let check = |text: &str| {
        let mut p = parse_prog(text, 1).unwrap();
        let scope = BTreeMap::from([("a".to_string(), Type::Opt(Box::new(Type::Int)))]);
        crate::check::check_in(&mut p, &Type::Int, &scope, &[], &BTreeMap::new(), &BTreeMap::new(), true)
    };
    assert!(check("match a { none => return 0  n => return n }").is_ok(), "the bound value is the Int");
    assert!(check("match a { none => return 0  _ => return 1 }").is_ok());
    assert!(check("match a { n => return 1 }").is_ok());
    assert!(check("match a { n => return n }").is_err(), "with no `none` before it, n may be absent: an Int? is no Int");
    assert!(check("match a { none => return 0 }").unwrap_err().contains("does not cover"), "none alone");
    assert!(check("match a { _ => return 1  none => return 0 }").unwrap_err().contains("can never be taken"), "none after _ is dead");
    assert!(check("match a { none => return 0  _ => return 1  n => return 2 }").unwrap_err().contains("can never be taken"));
    assert!(check("match a { none => return 0  none => return 1  _ => return 2 }").unwrap_err().contains("can never be taken"));
    let mut p = parse_prog("match 3 { none => return 0  _ => return 1 }", 1).unwrap();
    let e = crate::check::check_in(&mut p, &Type::Int, &BTreeMap::new(), &[], &BTreeMap::new(), &BTreeMap::new(), true).unwrap_err();
    assert!(e.contains("never absent"), "{e}");
    let one = parse_prog("match a { none => return 1  _ => return 2 }", 1).unwrap();
    let Prog::Match { arms, .. } = &one else { panic!() };
    assert!(matches!(&arms[0].0, Pat::None) && crate::print::pat(&arms[0].0) == "none");
    // lead and fixer on the thread; lead's post asks the owner
    let mut r = Run::before_asks();
    let t = r.say("lead", &["lead"], "the parser");
    r.settle(10);
    r.nothing_asked();
    r.post_on(t, "lead", "@fixer have a look", &["fixer"]).reply.unwrap();
    r.nothing_asked();
    r.post_on(t, "lead", "Which grammar, PEG or LALR?", &[]).reply.unwrap();
    let n = r.m.tickets[&t].posts.last().unwrap().n;
    r.asks(n, json!({"head": "Asks", "marks": [Run::mark([0, 27], &[[15, 18], [22, 26]])]}));
    assert!(r.m.view(t).unwrap().posts.iter().find(|p| p.n == n).unwrap().asks_open);
    // his answers, typed to lead from the card: to lead, no route step
    let out = r.go(Input::Post { t, by: "owner".into(), text: Text::New("PEG".into()), names: vec!["lead".into()], told: vec!["lead".into(), "fixer".into()], inform: vec![], answers: Some(n), kind: None });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(r.handler().is_none(), "no route step: the handler ended");
    assert!(r.m.due(WATCHER).is_empty());
    let a = r.m.tickets[&t].posts.last().unwrap().n;
    assert!(r.m.due("lead").iter().any(|d| d.t == t && d.upto == a), "lead is given the answers");
    // a plain post typed to lead on the same thread: routed as now
    r.settle(14);
    r.nothing_asked();
    r.post_on(t, "owner", "and the tests", &["lead"]).reply.unwrap();
    assert!(r.handler().is_some());
    r.route(json!({"head": "To", "agents": ["lead"]}));
    r.replays();
}

/// A handler's run records its program in its Opened line, and replay runs that (paper-research m1425): a log made
/// when the base's owner_post handler asked route about words typed to nobody on a one-agent ticket replays under
/// today's base, which asks nothing for them, as it happened.
#[test]
fn a_handlers_run_replays_under_the_program_it_ran() {
    let base = include_str!("../../../../agent-base/kinds/base.kind");
    let new_arm = "              [] => match (p.chosen, p.naming, p.others) {\n                ([], _, []) => return To(p.naming)\n                ([], [], [a]) => return To([a])\n                _ =>\n                  match p.left.posts {\n                    [] => return Split(p.chosen)\n                    _ => step watcher route(p.text, p.naming, p.others)\n                  }\n              }";
    assert!(base.contains(new_arm));
    let old = base.replace(new_arm, "              [] => match p.others {\n                [] => return To(p.naming)\n                _ => step watcher route(p.text, p.naming, p.others)\n              }");
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = CALLS.iter().flat_map(|(f, t)| syntax::parse_calls(f, t, &mut named).unwrap()).map(|c| (c.name.clone(), c)).collect();
    let files: Vec<(String, String)> = KINDS.iter().map(|(f, t)| (f.to_string(), if *f == "base.kind" { old.clone() } else { t.to_string() })).collect();
    let (d, errors) = Defs::build(&files, calls, named, 200);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    let t = r.say("lead", &["lead"], "the parser");
    r.settle(10);
    r.nothing_asked();
    r.post_on(t, "owner", "and the lexer", &[]).reply.unwrap();
    r.route(json!({"head": "To", "agents": ["lead"]}));
    let h = r.m.tickets.range(HIDDEN..).find(|(_, x)| x.handling.as_deref() == Some("owner_post")).map(|(h, _)| *h).unwrap();
    let recorded = r.log.iter().find_map(|l| match l {
        Line::Opened { t, program, .. } if *t == h => Some(program.clone()),
        _ => None,
    });
    assert!(recorded.as_deref().is_some_and(|p| p.contains("step watcher route(p.text, p.naming, p.others)") && !p.contains("[a]")), "{recorded:?}");
    r.replays();
}

/// The hole's handler runs in its ticket itself; where its run begins the log records its program (`handling`), and
/// replay runs that (paper-research m1430): a log made under a base whose hole handler asked another role replays
/// under today's base as it happened. A log without the line runs today's handler (the old logs: `replays_today`).
#[test]
fn a_hole_handlers_run_replays_under_the_program_it_ran() {
    let base = include_str!("../../../../agent-base/kinds/base.kind");
    assert!(base.contains("k <- step role minder shape(this);"));
    let old = base.replace("k <- step role minder shape(this);", "k <- step role shaper shape(this);");
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = CALLS.iter().flat_map(|(f, t)| syntax::parse_calls(f, t, &mut named).unwrap()).map(|c| (c.name.clone(), c)).collect();
    let files: Vec<(String, String)> = KINDS.iter().map(|(f, t)| (f.to_string(), if *f == "base.kind" { old.clone() } else { t.to_string() })).collect();
    let (d, errors) = Defs::build(&files, calls, named, 200);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    let t = r.say("lead", &["lead"], "the parser");
    r.settle(10);
    r.nothing_asked();
    let program = r.log.iter().find_map(|l| match l {
        Line::Handling { t: u, event, program, .. } if *u == t && event == "hole" => Some(program.clone()),
        _ => None,
    });
    assert!(program.as_deref().is_some_and(|p| p.contains("step role shaper shape(this)")), "{program:?}");
    assert!(matches!(&r.m.tickets[&t].status, Status::Asking { role, .. } if role == "shaper"), "{:?}", r.m.tickets[&t].status);
    r.replays();
}

/// On replay a handler runs only where the log has its run (paper-research m1432): a log made under a base with no
/// owner_post handler, an owner's post on it and no handler lines, replays clean under today's base, which has one;
/// the same post made live under today's base raises the handler.
#[test]
fn replay_runs_a_handler_only_where_the_log_has_its_run() {
    let base = include_str!("../../../../agent-base/kinds/base.kind");
    let from = base.find("  when owner_post(").unwrap();
    let to = base[from + 1..].find("\n  when ").map_or(base.len(), |i| from + 1 + i + 1);
    let old = format!("{}{}", &base[..from], &base[to..]);
    assert!(!old.contains("owner_post("));
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = CALLS.iter().flat_map(|(f, t)| syntax::parse_calls(f, t, &mut named).unwrap()).map(|c| (c.name.clone(), c)).collect();
    let files: Vec<(String, String)> = KINDS.iter().map(|(f, t)| (f.to_string(), if *f == "base.kind" { old.clone() } else { t.to_string() })).collect();
    let (d, errors) = Defs::build(&files, calls, named, 200);
    assert!(errors.is_empty(), "{errors:?}");
    let post = |r: &mut Run| {
        let t = r.say("lead", &["lead"], "the parser");
        r.settle(10);
        r.nothing_asked();
        r.post_on(t, "owner", "and the lexer", &[]).reply.unwrap();
        r.log.iter().any(|l| matches!(l, Line::Opened { kind, .. } if kind == "owner_post"))
    };
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    assert!(!post(&mut r), "no handler then");
    r.replays();
    let mut r = Run::new();
    assert!(post(&mut r), "live, today's base raises it");
    r.replays();
}

/// `tell AGENTS p joining J` (base.kind's To arm, with route's `joins`: the thread's holder; hs-ls N-1446 p38): J, an
/// earlier post of the owner's on the ticket, is kept on the post and on its `Told` line, for the line the agent reads
/// and his window's mark; a J that is not an earlier post of his there (an agent's post, one after it) is left out, and
/// the post goes all the same, as it does with none. A log made under base.kind from before the clause replays under
/// today's (the recorded program runs) and keeps none. The clause prints as it parses.
#[test]
fn a_post_told_joining_his_earlier_post_keeps_it_on_the_post() {
    let told = |r: &mut Run, j: Value| {
        let t = r.two_on_a_thread();
        // his #3, typed to nobody on a thread of two: the watcher's route says the lead, going on from J
        let (n, _) = r.owner_posts(t, &[], "and the mean too");
        r.route_of(t, n, json!({"head": "To", "agents": ["lead"], "joins": j}));
        let line = r.log.iter().find_map(|l| match l {
            Line::Told { t: u, n: m, to, joins, .. } if *u == t && *m == n => Some((to.clone(), *joins)),
            _ => None,
        });
        assert!(r.woken("lead", t) && r.informed("fixer", t), "told either way");
        r.replays();
        (r.post(t, n).joins, line)
    };
    let lead = || vec!["lead".to_string()];
    // #1 is his, the thread's first words: kept, on the post and on the line
    assert_eq!(told(&mut Run::new(), json!(1)), (Some(1), Some((lead(), Some(1)))));
    // #2 is the lead's, #9 is not before it, and none is none: left out
    for j in [json!(2), json!(9), Value::Null] {
        assert_eq!(told(&mut Run::new(), j.clone()), (None, Some((lead(), None))), "{j}");
    }
    // under base.kind from before the clause, as recorded then: the same answer keeps none, and the log replays
    let base = include_str!("../../../../agent-base/kinds/base.kind");
    let arm = "      To(as, j) => tell as p joining j\n";
    assert_eq!(base.matches(arm).count(), 1, "base.kind's To arm");
    let old = base.replace(arm, "      To(as) => tell as p\n");
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = CALLS.iter().flat_map(|(f, t)| syntax::parse_calls(f, t, &mut named).unwrap()).map(|c| (c.name.clone(), c)).collect();
    let files: Vec<(String, String)> = KINDS.iter().map(|(f, t)| (f.to_string(), if *f == "base.kind" { old.clone() } else { t.to_string() })).collect();
    let (d, errors) = Defs::build(&files, calls, named, 200);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    assert_eq!(told(&mut r, json!(1)), (None, Some((lead(), None))));
    let program = r.log.iter().rev().find_map(|l| match l {
        Line::Opened { kind, program, .. } if kind == "owner_post" => Some(program.clone()),
        _ => None,
    });
    assert!(program.as_deref().is_some_and(|p| p.contains("tell as p") && !p.contains("joining")), "{program:?}");
    // it prints as it parses
    let src = "tell xs p joining j";
    let once = print::prog(&syntax::parse_prog(src, 1).unwrap(), 0);
    assert_eq!(once.trim(), src);
    assert_eq!(print::prog(&syntax::parse_prog(&once, 1).unwrap(), 0), once);
}

/// An agent's open of a thread naming another (`coord open thread --to X --addressed Named …`: the subthread
/// paper-research opened for paper-research-brain, N-1445, whose #2 showed its words as the owner's): its words are
/// its own. The machine gives the form's `opener` (machine.rs `OPENER`) and thread.kind posts as it (hs-ls N-1446
/// p73). Given as the open's own text too, they are its one post, #1 by it, naming X, who is handed it: the open's line
/// records no text, so a coordd from before replays one post too (consult-3 p181). Given in the form alone, they are
/// posted by it as #1, naming X. An `opener` the agent wrote itself is given over. Where its post would not be the
/// open's (an @NAME the form has only in `typed`; an Unnamed open, which waits on the watcher's intake), the open keeps
/// its text as its own #1, naming whom it names (consult-3 p254). Nothing is posted as the owner, so his owner_post
/// handler never runs; his own message naming someone still posts as his, its `Fired` form recording
/// the owner. A thread opened under thread.kind from before, as N-1445 recorded it, posts as the owner and replays as
/// it was (the recorded program runs).
#[test]
fn an_agents_open_of_a_thread_posts_its_words_as_its_own() {
    let open_as = |r: &mut Run, form: Value, text: Option<&str>, names: &[&str]| {
        let names = names.iter().map(|a| a.to_string()).collect();
        let out = r.go(Input::Open { kind: "thread".into(), under: None, opener: "lead".into(), form, text: text.map(|w| Text::New(w.into())), names, told: vec![] });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        (*r.m.tickets.range(..HIDDEN).next_back().unwrap().0, out)
    };
    let open = |r: &mut Run, text: Option<&str>, opener: Value| {
        let form = json!({"to": "fixer", "typed": ["fixer"], "names": ["fixer"], "text": "the brain, on its own thread", "addressed": {"head": "Named"}, "label": "Brain", "opener": opener});
        open_as(r, form, text, &[])
    };
    let posts = |r: &Run, t: u64| r.m.tickets[&t].posts.iter().map(|p| (p.n, p.by.clone(), p.names.clone())).collect::<Vec<_>>();
    let his_handler_ran = |r: &Run| r.log.iter().any(|l| matches!(l, Line::Opened { kind, .. } if kind == "owner_post"));
    let opened = |r: &Run| r.log.iter().find_map(|l| match l {
        Line::Open { form, text, .. } => Some((form["opener"].clone(), text.is_some())),
        _ => None,
    });
    let lead_naming_fixer = vec![(1, "lead".to_string(), vec!["fixer".to_string()])];
    // the words as the open's own text too, as N-1445's: its one post, by it, naming fixer, who is handed it; the
    // line records the opener and no text of the open's
    let mut r = Run::new();
    let (t, out) = open(&mut r, Some("the brain, on its own thread"), Value::Null);
    assert_eq!(posts(&r, t), lead_naming_fixer);
    assert_eq!(opened(&r), Some((json!("lead"), false)));
    assert!(out.effects.contains(&Effect::Deliver { to: "fixer".into() }), "{out:?}");
    assert!(!his_handler_ran(&r) && matches!(r.m.tickets[&t].status, Status::Rests { .. }));
    assert!(r.m.due("lead").iter().all(|d| d.t != t), "its own words are not handed to it");
    r.replays();
    // the words in the form alone, and an `opener` it wrote itself: posted by it, naming fixer
    let mut r = Run::new();
    let (t, out) = open(&mut r, None, json!("owner"));
    assert_eq!(posts(&r, t), lead_naming_fixer);
    assert_eq!(opened(&r), Some((json!("lead"), false)));
    assert!(out.effects.contains(&Effect::Deliver { to: "fixer".into() }), "{out:?}");
    assert!(!his_handler_ran(&r));
    r.replays();
    // other words as the open's own text: both stay, each by it
    let mut r = Run::new();
    let (t, _) = open(&mut r, Some("and one more thing"), Value::Null);
    assert_eq!(posts(&r, t).into_iter().map(|(n, by, _)| (n, by)).collect::<Vec<_>>(), [(1, "lead".to_string()), (2, "lead".to_string())]);
    r.replays();
    // the words twice, naming w2, whom the form has only in `typed`: the program's post names fixer alone, so the open
    // keeps its text, #1 naming w2, who is handed it; then the program's #2, naming fixer
    let mut r = Run::new();
    let form = json!({"to": "fixer", "typed": ["fixer", "w2"], "names": ["fixer"], "text": "@w2 look", "addressed": {"head": "Named"}});
    let (t, out) = open_as(&mut r, form, Some("@w2 look"), &["w2"]);
    assert_eq!(opened(&r), Some((json!("lead"), true)));
    assert_eq!(posts(&r, t), vec![(1, "lead".to_string(), vec!["w2".to_string()]), (2, "lead".to_string(), vec!["fixer".to_string()])]);
    assert!(out.effects.contains(&Effect::Deliver { to: "w2".into() }), "{out:?}");
    r.replays();
    // the words twice, Unnamed: the watcher's intake decides where they go, so the open keeps its text, #1 at once
    let mut r = Run::new();
    let form = json!({"to": "fixer", "typed": ["fixer"], "names": ["fixer"], "text": "where does this go", "addressed": {"head": "Unnamed"}});
    let (t, _) = open_as(&mut r, form, Some("where does this go"), &["fixer"]);
    assert_eq!(opened(&r), Some((json!("lead"), true)));
    assert_eq!(posts(&r, t), lead_naming_fixer);
    assert!(matches!(&r.m.tickets[&t].status, Status::Awaits { agent, owed, .. } if agent == "watcher" && owed.call == "intake"), "{:?}", r.m.tickets[&t].status);
    r.replays();
    // his own message naming someone: his, its binding's form recording the owner
    let mut r = Run::new();
    let t = r.say("fixer", &["fixer"], "@fixer the brain");
    assert_eq!(r.post(t, 1).by, "owner");
    assert!(his_handler_ran(&r));
    let fired = r.log.iter().find_map(|l| match l {
        Line::Event { name, fired, .. } if name == "owner_message" => fired.first().map(|f| f.form["opener"].clone()),
        _ => None,
    });
    assert_eq!(fired, Some(json!("owner")));
    r.replays();
    // under thread.kind from before, as N-1445 recorded it: #2 by the owner, replayed as it was
    let mut r = Run::before_opener();
    let (t, _) = open(&mut r, Some("the brain, on its own thread"), Value::Null);
    assert_eq!(posts(&r, t).into_iter().map(|(n, by, _)| (n, by)).collect::<Vec<_>>(), [(1, "lead".to_string()), (2, "owner".to_string())]);
    r.replays();
}

/// What `a_record_from_after_replays_in_build_36` (build 36's own machine, eb8e03fc, in a worktree of its own: hs-ls
/// N-1446, consult-3 p181) reads: with WAY_BACK_OUT set, this head's record of his message naming an agent, an agent's
/// Named open given its words twice, and one given them in the form alone, written there as JSON lines, with each
/// ticket's posts as this head made them (`.posts`, one `[t, n, by, names]` per line).
#[test]
#[ignore]
fn a_record_for_build_36() {
    let Ok(out) = std::env::var("WAY_BACK_OUT") else { return };
    let mut r = Run::new();
    r.say("fixer", &["fixer"], "@fixer the brain");
    for text in [Some("the brain, on its own thread"), None] {
        let form = json!({"to": "fixer", "typed": ["fixer"], "names": ["fixer"], "text": "the brain, on its own thread", "addressed": {"head": "Named"}, "label": "Brain"});
        let o = r.go(Input::Open { kind: "thread".into(), under: None, opener: "lead".into(), form, text: text.map(|w| Text::New(w.into())), names: vec![], told: vec![] });
        assert!(o.reply.is_ok(), "{:?}", o.reply);
    }
    r.replays();
    let lines: Vec<String> = r.log.iter().map(|l| serde_json::to_string(l).unwrap()).collect();
    std::fs::write(&out, lines.join("\n") + "\n").unwrap();
    let posts: Vec<String> = r.m.tickets.range(..HIDDEN).flat_map(|(t, x)| x.posts.iter().map(move |p| json!([t, p.n, p.by, p.names]).to_string())).collect();
    std::fs::write(format!("{out}.posts"), posts.join("\n") + "\n").unwrap();
    println!("{} lines, {} posts", lines.len(), posts.len());
}

/// A thread a program opens (E2's aside opens one to its fork: proto-lead N-1446 p168) is never posted as his: its
/// words are the agent's whose open is above it, else the ticket's whose program opened it, as that program's own post
/// is; under a ticket he opened himself too, so his handler never runs. Its #1 names whom it is to, who is handed it,
/// and the log replays.
#[test]
fn a_thread_a_program_opens_is_never_posted_as_his() {
    const OPENS: &str = "kind opens\n  letter Y\n  form to: Agent, text: Text\n  result Text\n  pattern\n    _ <- open thread(to, [to], [to], text, Named, none, text);\n    return \"opened\"\n";
    let first = |opener: &str| {
        let (d, errors) = defs_with(&[("opens.kind", OPENS)]);
        assert!(errors.is_empty(), "{errors:?}");
        let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
        let form = json!({"to": "fixer", "text": "the brain, on its own thread"});
        let out = r.go(Input::Open { kind: "opens".into(), under: None, opener: opener.into(), form, text: None, names: vec![], told: vec![] });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        let o = r.m.tickets.values().find(|x| x.kind == "opens").map(|x| x.t).expect("the opener's ticket");
        let s = r.m.tickets.values().find(|x| x.kind == "thread" && x.parent == Some(o)).map(|x| x.t).expect("the thread its program opened");
        assert!(out.effects.contains(&Effect::Deliver { to: "fixer".into() }), "{out:?}");
        assert!(!r.log.iter().any(|l| matches!(l, Line::Opened { kind, .. } if kind == "owner_post")), "his handler never runs");
        let posts: Vec<(u32, String, Vec<String>)> = r.m.tickets[&s].posts.iter().map(|p| (p.n, p.by.clone(), p.names.clone())).collect();
        r.replays_with(&[("opens.kind", OPENS)]);
        (posts, r.m.id(o))
    };
    let fixer = || vec!["fixer".to_string()];
    // under an agent's open: the agent's
    let (posts, _) = first("lead");
    assert_eq!(posts, vec![(1, "lead".to_string(), fixer())]);
    // under his own open: the ticket whose program opened it, never his
    let (posts, o) = first("owner");
    assert_eq!(posts, vec![(1, o, fixer())]);
}

/// Nothing (paper-research N-17): coordd passes a turn that ended with the single word Nothing as an empty text, and
/// only when the turn answers no step. The rule Reply posts nothing for it and wakes nobody; the turn end is kept, so
/// what it was given counts as read. A turn that answers a step is told apart first.
#[test]
fn a_turn_that_says_nothing_posts_nothing_and_a_step_is_told_apart() {
    let mut r = Run::new();
    let n = r.note("owner", "orch", "FYI: the build is green.");
    let d = r.give("orch", n);
    assert!(!r.m.turn_answers_step("orch", Some(&d), &[], None), "a note is read by the rule Reply, not a step's answer");
    let out = r.go(Input::TurnEnded { agent: "orch".into(), delivery: Some(d.clone()), text: Loc { file: "t.jsonl".into(), offset: 9, len: 0 }, later: vec![], stood: None, go: None });
    assert!(matches!(out.lines.as_slice(), [Line::TurnEnded { .. }]), "{:?}", out.lines);
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::Deliver { .. } | Effect::Notify { .. })), "{:?}", out.effects);
    assert_eq!(r.m.tickets[&n].posts.len(), 1, "nothing posted");
    assert!(r.m.given_since("orch").is_empty(), "the note counts as read");
    let q = &r.m.view(n).unwrap().quiet;
    assert_eq!((q.len(), q[0].agent.as_str(), q[0].upto), (1, "orch", 1), "read, no reply: {q:?}");
    // a step owed: the turn answers it, so coordd leaves its text as it is
    let mut r = Run::new();
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    let s = r.take("orch");
    assert!(r.m.turn_answers_step("orch", Some(&s.id), &[], None));
}

/// A turn end answers only what was handed before its last message was written (paper-research N-17 post 10): what
/// coordd says was handed after it (LATER) is counted for the next turn, replied there, and a step handed so is not
/// answered by the old message.
#[test]
fn a_turn_end_does_not_answer_what_was_handed_after_its_message() {
    let mut r = Run::new();
    let a = r.note("owner", "orch", "is the build green?");
    let da = r.give("orch", a);
    // the message is written; the session looks idle and the next note is handed before the turn's end is read
    let b = r.note("dev", "orch", "can you look at C-1?");
    let db = r.give("orch", b);
    assert!(r.m.reply_names("orch", &[]).is_none(), "two tickets: no reply without LATER");
    assert_eq!(r.m.reply_names("orch", std::slice::from_ref(&db)), Some((a, vec!["owner".to_string()])));
    let out = r.go(Input::TurnEnded { agent: "orch".into(), delivery: Some(da.clone()), text: Run::loc(5), later: vec![db.clone()], stood: None, go: None });
    assert!(matches!(out.lines.as_slice(), [Line::TurnEnded { later, .. }, Line::Replied { t, .. }, ..] if *later == vec![db.clone()] && *t == a), "{:?}", out.lines);
    assert_eq!(r.m.given_since("orch"), vec![db.clone()], "counted for the next turn");
    assert_eq!(r.m.tickets[&b].posts.len(), 1);
    // the next turn reads it and its end replies there, to dev
    let out = r.turn("orch", Some(&db), 6);
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert_eq!(r.m.tickets[&b].posts.last().unwrap().names, vec!["dev".to_string()]);
    assert!(r.m.given_since("orch").is_empty());
    r.replays();
    // a step handed after the message is not answered by it
    let mut r = Run::new();
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    let s = r.take("orch");
    assert!(!r.m.turn_answers_step("orch", None, std::slice::from_ref(&s.id), None));
    let out = r.go(Input::TurnEnded { agent: "orch".into(), delivery: None, text: Run::loc(7), later: vec![s.id.clone()], stood: None, go: None });
    assert!(!out.lines.iter().any(|l| matches!(l, Line::Forming { .. })), "{:?}", out.lines);
    assert!(r.m.turn_answers_step("orch", Some(&s.id), &[], None), "still owed, by the next turn");
    r.replays();
}

/// docket-edge's run 2a (N-8 post 22, N-27 point 3c): cone was handed a post told to it on one ticket and one it was
/// only informed of on another, and its answer went nowhere as "several". What only informs does not make a turn
/// several: the rule Reply answers on the one ticket, and the line says which deliveries only informed it.
#[test]
fn an_inform_beside_a_told_post_does_not_make_a_turn_several() {
    let mut r = Run::new();
    let a = r.note("pipe", "lead", "FYI: dev is green.");
    let out = r.go(Input::Post { t: a, by: "lead".into(), text: Text::New("Noted.".into()), names: vec!["pipe".into()], told: vec![], inform: vec!["cone".into()], answers: None, kind: None });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let b = r.note("pipe", "cone", "median(xs): ties?");
    let due = r.m.due("cone");
    assert_eq!(due.len(), 2, "{due:?}");
    let ids: Vec<String> = due.iter().map(|d| d.id.clone()).collect();
    assert_eq!(r.m.reply_ticket("cone", &ids), Some(b), "the footer: one thing, on b");
    let (da, db) = (r.give("cone", a), r.give("cone", b));
    assert_eq!(r.m.reply_names("cone", &[]), Some((b, vec!["pipe".to_string()])));
    let out = r.turn("cone", Some(&db), 9);
    assert!(matches!(out.lines.as_slice(), [Line::TurnEnded { informs, .. }, Line::Replied { t, names, .. }] if *informs == vec![da.clone()] && *t == b && *names == vec!["pipe".to_string()]), "{:?}", out.lines);
    assert!(r.m.given_since("cone").is_empty(), "both read");
    // only informs: nothing to answer, nobody named
    let c = r.note("pipe", "lead", "one more FYI");
    r.go(Input::Post { t: c, by: "lead".into(), text: Text::New("ok".into()), names: vec!["pipe".into()], told: vec![], inform: vec!["cone".into()], answers: None, kind: None });
    r.give("cone", c);
    assert_eq!(r.m.reply_names("cone", &[]), Some((c, vec![])));
    r.replays();
}

/// An agents' thread (design part 9b, N-27 point 3d) waits on each agent told a post that has not ended a turn handed
/// it: while it is due, while its turn runs; not after its turn ends with a reply or Nothing. Derived, no line.
#[test]
fn an_agents_thread_waits_on_whom_it_told_until_their_turn_ends() {
    let mut r = Run::new();
    let out = r.go(Input::Open { kind: "thread".into(), under: None, opener: "pipe".into(), form: json!({"to": "cone", "typed": ["cone"], "names": ["cone"], "text": "median(xs): ties?", "addressed": {"head": "Sent", "by": "pipe"}, "follows": null}), text: Some(Text::New("median(xs): ties?".into())), names: vec!["cone".into()], told: vec![] });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let t = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    assert!(r.m.tickets[&t].between() && r.m.due("pipe").iter().all(|d| d.t != t), "ended as it opened, no closing notice to pipe");
    let waits = |r: &Run| r.m.view(t).unwrap().waits_on.iter().map(|w| (w.agent.clone(), w.call.clone(), w.post)).collect::<Vec<_>>();
    assert_eq!(waits(&r), vec![("cone".to_string(), "post".to_string(), Some(1))], "due");
    let d = r.give("cone", t);
    assert_eq!(waits(&r).len(), 1, "handed, its turn running");
    r.turn("cone", Some(&d), 5);
    assert_eq!(waits(&r), vec![("pipe".to_string(), "post".to_string(), Some(2))], "cone's reply names pipe");
    let d = r.give("pipe", t);
    r.go(Input::TurnEnded { agent: "pipe".into(), delivery: Some(d), text: Loc { len: 0, ..Run::loc(6) }, later: vec![], stood: None, go: None });
    assert!(waits(&r).is_empty(), "quiet after Nothing");
    r.replays();
}

/// 3h (paper-research N-27): an agent's message to the owner is a thread closed from the start; what it asks him stays
/// open there, and waits on him, until it is answered, he says nothing to answer, its asker posts again, or a thread
/// following it is opened. A post on a closed ticket not told to him asks him nothing.
#[test]
fn a_closed_tickets_asks_told_to_the_owner_stay_open_until_one_of_four_things() {
    use crate::line::AsksClosed;
    let open_asks = |r: &Run, t: u64| r.m.view(t).unwrap().posts.iter().filter(|p| p.asks_open).map(|p| p.n).collect::<Vec<_>>();
    let waits = |r: &Run, t: u64| r.m.view(t).unwrap().waits_on.iter().filter(|w| w.call == "asks" && w.agent == "owner").map(|w| w.post.unwrap()).collect::<Vec<_>>();
    let closed = |r: &Run, t: u64, n: u32| r.m.view(t).unwrap().posts.iter().find(|p| p.n == n).unwrap().asks_closed.clone();
    let asked = |r: &mut Run| {
        let t = r.note("lead", "owner", "the release: A or B?");
        assert!(r.m.tickets[&t].between() && !r.m.tickets[&t].is_open());
        r.asks(1, json!({"head": "Asks", "marks": [Run::mark([13, 19], &[])]}));
        assert_eq!((open_asks(r, t), waits(r, t)), (vec![1], vec![1]), "closed, and open: it waits on him");
        t
    };
    // 1. he says nothing to answer
    let mut r = Run::before_asks();
    let t = asked(&mut r);
    r.go(Input::NothingToAnswer { t, n: 1, by: "owner".into() }).reply.unwrap();
    assert_eq!((open_asks(&r, t), waits(&r, t), closed(&r, t, 1)), (vec![], vec![], Some(AsksClosed::NothingToAnswer)));
    r.replays_today();
    // 2. its asker posts again
    let mut r = Run::before_asks();
    let t = asked(&mut r);
    r.post_on(t, "lead", "B, I checked", &["owner"]).reply.unwrap();
    r.nothing_asked();
    assert_eq!((open_asks(&r, t), closed(&r, t, 1)), (vec![], Some(AsksClosed::Posted { post: 2 })));
    r.replays_today();
    // 3. a post that wakes the asker answers it
    let mut r = Run::before_asks();
    let t = asked(&mut r);
    r.go(Input::Post { t, by: "owner".into(), text: Text::New("B".into()), names: vec!["lead".into()], told: vec![], inform: vec![], answers: Some(1), kind: None }).reply.unwrap();
    assert_eq!((open_asks(&r, t), closed(&r, t, 1)), (vec![], Some(AsksClosed::Answered { post: 2, by: "owner".into() })));
    r.replays_today();
    // 4. his words for it open a thread that follows it
    let mut r = Run::before_asks();
    let t = asked(&mut r);
    let id = r.m.id(t);
    r.go(Input::Event { name: "owner_message".into(), value: json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": [], "text": "B", "addressed": {"head": "Named"}, "follows": id}) }).reply.unwrap();
    let f = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    assert_ne!(f, t);
    assert_eq!((open_asks(&r, t), waits(&r, t), closed(&r, t, 1)), (vec![], vec![], Some(AsksClosed::Followed { ticket: r.m.id(f) })));
    r.replays_today();
    // a closed ticket's post not told to him asks him nothing, marked or not
    let mut r = Run::before_asks();
    let t = r.note("lead", "cone", "the release: A or B?");
    assert!(r.converting().is_empty(), "the owner is not on it: no asks call");
    r.post_on(t, "cone", "and you, owner? A or B?", &[]).reply.unwrap();
    assert!(open_asks(&r, t).is_empty());
}

/// paper-research N-27 post 43, research's lines 847 to 854: edge's turn for one delivery ends after the next was
/// handed (later); its reply gets a number above what the next turn was handed. That reply answered what its own turn
/// held, not the posts handed after its message: the next turn's message is posted, naming their authors. A post by
/// hand made after a delivery still answers it: the turn's end posts nothing.
#[test]
fn a_reply_numbered_after_a_later_delivery_does_not_answer_it() {
    let post = |r: &mut Run, t: u64, by: &str, to: &str, text: &str| {
        let out = r.go(Input::Post { t, by: by.into(), text: Text::New(text.into()), names: vec![to.into()], told: vec![], inform: vec![], answers: None, kind: None });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
    };
    let mut r = Run::new();
    let t = r.note("research", "edge", "the stills, please");
    post(&mut r, t, "core", "edge", "3d is in");
    let d1 = r.give("edge", t);
    // edge's message is written; meanwhile research and core post, and edge is handed them before its turn's end
    post(&mut r, t, "research", "edge", "accepted, with one change");
    post(&mut r, t, "core", "edge", "the variant is Cancelled");
    let d2 = r.give("edge", t);
    assert_ne!(d1, d2);
    let out = r.go(Input::TurnEnded { agent: "edge".into(), delivery: Some(d1.clone()), text: Run::loc(1), later: vec![d2.clone()], stood: None, go: None });
    assert!(matches!(out.lines.as_slice(), [Line::TurnEnded { .. }, Line::Replied { n: 5, names, .. }, ..] if *names == vec!["research".to_string(), "core".to_string()]), "{:?}", out.lines);
    // the next turn, for what was handed after the message: posted, naming both
    assert_eq!(r.m.reply_names("edge", &[]), Some((t, vec!["research".to_string(), "core".to_string()])));
    let out = r.turn("edge", Some(&d2), 2);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Replied { n: 6, names, .. } if *names == vec!["research".to_string(), "core".to_string()])), "{:?}", out.lines);
    assert!(r.m.given_since("edge").is_empty());
    r.replays();
    // a post by hand after the delivery answers it: the turn's end posts nothing more
    let mut r = Run::new();
    let t = r.note("research", "edge", "the stills, please");
    let d = r.give("edge", t);
    post(&mut r, t, "edge", "research", "here they are");
    assert!(r.m.reply_names("edge", &[]).is_none());
    let out = r.turn("edge", Some(&d), 3);
    assert!(!out.lines.iter().any(|l| matches!(l, Line::Replied { .. })), "{:?}", out.lines);
    r.replays();
}

/// paper-research N-101.p1285: with notify.eflint telling the owner of nothing he is not named in, a reply to him
/// must still name him. On a thread an agent opened (N-101 is research's), his post to research is answered by its
/// turn's end, and by a nameless post by hand, both naming him; a post to another agent there names only that agent.
#[test]
fn a_reply_to_the_owner_on_an_agents_thread_names_him() {
    let mut r = Run::new();
    let t = r.note("research", "core", "the plan");
    r.owner_says(t, &["research"], "how long?");
    if !r.woken("research", t) {
        r.route(json!({"head": "To", "agents": ["research"]}));
    }
    let d = r.give("research", t);
    let out = r.turn("research", Some(&d), 1);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Replied { names, .. } if *names == vec!["owner".to_string()])), "{:?}", out.lines);
    r.replays();
    // by hand, naming nobody: the same
    let mut r = Run::new();
    let t = r.note("research", "core", "the plan");
    r.owner_says(t, &["research"], "how long?");
    if !r.woken("research", t) {
        r.route(json!({"head": "To", "agents": ["research"]}));
    }
    r.give("research", t);
    r.go(Input::Post { t, by: "research".into(), text: Text::New("an hour".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
    let last = r.m.tickets[&t].posts.last().unwrap();
    assert_eq!((last.by.as_str(), last.names.clone()), ("research", vec!["owner".to_string()]));
    // then a post naming core names only core
    r.go(Input::Post { t, by: "research".into(), text: Text::New("@core merge it".into()), names: vec!["core".into()], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
    assert_eq!(r.m.tickets[&t].posts.last().unwrap().names, vec!["core".to_string()]);
    r.replays();
}

/// The reply tool (paper-research N-101.p1285): an agent given several of the owner's messages in one turn answers
/// each by its number. The post records `answers` and reaches him; the turn's end posts nothing for what was given
/// several at once; the message not answered stays in `unanswered_owner`, and leaves it once answered.
#[test]
fn the_owners_messages_answered_one_by_one_and_the_one_left_shows_unanswered() {
    let mut r = Run::new();
    let a = r.note("research", "core", "the plan");
    let b = r.note("research", "core", "the release");
    let to_research = |r: &mut Run, t: u64, text: &str| {
        r.owner_says(t, &["research"], text);
        if !r.woken("research", t) {
            r.route(json!({"head": "To", "agents": ["research"]}));
        }
        r.m.tickets[&t].posts.last().unwrap().n
    };
    let na = to_research(&mut r, a, "how long?");
    let nb = to_research(&mut r, b, "and the release?");
    assert_eq!(r.m.view(a).unwrap().unanswered_owner, vec![na]);
    let da = r.give("research", a);
    r.give("research", b);
    // the tool answers a's message only
    let out = r.go(Input::Post { t: a, by: "research".into(), text: Text::New("an hour".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: Some(na), kind: None });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert_eq!(r.m.tickets[&a].posts.last().unwrap().answers, Some(na), "an agent's post keeps what it answers");
    let out = r.turn("research", Some(&da), 1);
    assert!(!out.lines.iter().any(|l| matches!(l, Line::Replied { .. })), "{:?}", out.lines);
    assert!(r.m.view(a).unwrap().unanswered_owner.is_empty());
    assert_eq!(r.m.view(b).unwrap().unanswered_owner, vec![nb], "what the tool did not answer stays unanswered");
    r.go(Input::Post { t: b, by: "research".into(), text: Text::New("tomorrow".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: Some(nb), kind: None }).reply.unwrap();
    assert!(r.m.view(b).unwrap().unanswered_owner.is_empty());
    r.replays();
}

/// The owner's posts put into a running turn (paper-research N-101 p1214, p1263): the turn began with one message of
/// his; two more were put in while it ran. Its last message is the reply to the first, posted there, not "several".
/// The one answered with the reply tool during the turn is done; the one left unanswered is the next turn's, whose
/// last message is posted on its thread. A turn that holds only what was put in replies to that. One put in on the
/// thread the turn began on and answered with the reply tool leaves the turn's last message the reply to what it began
/// with; the turn's own post answered with the tool is not answered again by its end (paper-research N-1397 p49), nor
/// when the tool answered it as a further id of its answer to the post put in; a turn ending on Nothing posts nothing
/// (consult-3, N-1397 p58). A record before the era [`ERA_PUT_IN`]: from it, the one left unanswered is read at the
/// turn's end (`a_post_put_into_a_turn_and_left_there_is_read_at_its_end_from_the_era_put_in`).
#[test]
fn the_owners_posts_put_into_a_running_turn_each_get_an_answer() {
    let mut r = Run::new();
    let a = r.note("research", "core", "the plan");
    let b = r.note("research", "core", "the release");
    let c = r.note("research", "core", "the record");
    let to_research = |r: &mut Run, t: u64, text: &str| {
        r.owner_says(t, &["research"], text);
        if !r.woken("research", t) {
            r.route(json!({"head": "To", "agents": ["research"]}));
        }
        r.m.tickets[&t].posts.last().unwrap().n
    };
    let into = |r: &mut Run, t: u64| {
        let d = r.m.due("research").into_iter().find(|d| d.t == t).unwrap();
        r.go(Input::GivenInto { t, to: "research".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).reply.unwrap();
        d.id
    };
    let na = to_research(&mut r, a, "how long?");
    let da = r.give("research", a);
    let nb = to_research(&mut r, b, "and the release?");
    let nc = to_research(&mut r, c, "and the record?");
    into(&mut r, b);
    into(&mut r, c);
    // b answered with the reply tool inside the turn
    r.go(Input::Post { t: b, by: "research".into(), text: Text::New("tomorrow".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: Some(nb), kind: None }).reply.unwrap();
    let out = r.turn("research", Some(&da), 1);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Replied { t, .. } if *t == a)), "the turn's end replies to what it began with: {:?}", out.lines);
    assert!(r.m.view(a).unwrap().unanswered_owner.is_empty() && r.m.view(b).unwrap().unanswered_owner.is_empty());
    assert_eq!(r.m.view(c).unwrap().unanswered_owner, vec![nc]);
    // c is the next turn's: its last message is posted on c
    let dc = r.m.given_since("research");
    assert_eq!(dc.len(), 1, "{dc:?}");
    let out = r.turn("research", Some(&dc[0]), 2);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Replied { t, .. } if *t == c)), "{:?}", out.lines);
    assert!(r.m.view(c).unwrap().unanswered_owner.is_empty());
    assert!(r.m.given_since("research").is_empty());
    let _ = na;
    // a turn holding only what was put in (its queued message began it) replies to that
    let nd = to_research(&mut r, a, "one more?");
    let dd = into(&mut r, a);
    let out = r.turn("research", Some(&dd), 3);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Replied { t, .. } if *t == a)), "{:?}", out.lines);
    assert!(r.m.view(a).unwrap().unanswered_owner.is_empty(), "{nd}");
    // a post put into the turn on the thread it began on, answered with the reply tool: the turn's last message is
    // still the reply to what it began with, posted there once (paper-research N-1397 p49: hs-ls's head, lost behind
    // its #182 to N-1396 p180, its turn begun on p179)
    let ne = to_research(&mut r, a, "the plan's first part?");
    let de = r.give("research", a);
    let nf = to_research(&mut r, a, "and its second?");
    into(&mut r, a);
    r.go(Input::Post { t: a, by: "research".into(), text: Text::New("second: tomorrow".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: Some(nf), kind: None }).reply.unwrap();
    let out = r.turn("research", Some(&de), 4);
    let replied: Vec<(u64, Vec<String>)> = out.lines.iter().filter_map(|l| match l { Line::Replied { t, names, .. } => Some((*t, names.clone())), _ => None }).collect();
    assert_eq!(replied, vec![(a, vec!["owner".to_string()])], "the turn's last message answers what it began with, once, naming its author: {:?}", out.lines);
    assert!(r.m.view(a).unwrap().unanswered_owner.is_empty(), "{ne}");
    // its pair: the turn's own post answered with the reply tool, then more text at its end: nothing posted twice
    let ng = to_research(&mut r, a, "and the third?");
    let dg = r.give("research", a);
    r.go(Input::Post { t: a, by: "research".into(), text: Text::New("third: Friday".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: Some(ng), kind: None }).reply.unwrap();
    let out = r.turn("research", Some(&dg), 5);
    assert!(!out.lines.iter().any(|l| matches!(l, Line::Replied { .. })), "answered by the tool, not again by its end: {:?}", out.lines);
    // the reply tool to "the put-in post, the turn's own" in one post (its `also` on this thread), then text at its end:
    // nothing posted under the turn's own (consult-3, N-1397 p58)
    let nh = to_research(&mut r, a, "and the fourth?");
    let dh = r.give("research", a);
    let ni = to_research(&mut r, a, "and the fifth?");
    into(&mut r, a);
    r.go(Input::Post { t: a, by: "research".into(), text: Text::New("fourth and fifth: Monday".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: Some(ni), kind: None }).reply.unwrap();
    let both = r.m.tickets[&a].posts.last().unwrap().n;
    r.go(Input::AnswersAlso { t: a, n: both, also: vec![format!("{}.p{nh}", r.m.id(a))] }).reply.unwrap();
    let out = r.turn("research", Some(&dh), 6);
    assert!(!out.lines.iter().any(|l| matches!(l, Line::Replied { .. })), "the tool's further id answered the turn's own: {:?}", out.lines);
    // the reply tool to the put-in post only, then Nothing at its end: nothing posted
    let _nj = to_research(&mut r, a, "and the sixth?");
    let dj = r.give("research", a);
    let nk = to_research(&mut r, a, "and the seventh?");
    into(&mut r, a);
    r.go(Input::Post { t: a, by: "research".into(), text: Text::New("seventh: Tuesday".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: Some(nk), kind: None }).reply.unwrap();
    let posts = r.m.tickets[&a].posts.len();
    let out = r.go(Input::TurnEnded { agent: "research".into(), delivery: Some(dj), text: Loc { len: 0, ..Run::loc(7) }, later: vec![], stood: None, go: None });
    assert!(!out.lines.iter().any(|l| matches!(l, Line::Replied { .. })), "Nothing posts nothing: {:?}", out.lines);
    assert_eq!(r.m.tickets[&a].posts.len(), posts);
    r.replays();
}

/// The owner's "when done" (paper-research N-101 p1390; docket-try3's N-4.p5): his post sent to wait while the agent
/// is in a turn is not read by the agent's own post in that turn (its answer to what was put in), so it is given when
/// the turn is over: ended by itself, or cut by an esc with the thread closed meanwhile. Both replay, as
/// a coordd start reads them.
#[test]
fn a_post_sent_to_wait_is_given_when_the_turn_is_over() {
    for cut in [false, true] {
        let mut r = Run::new();
        let t = r.say("lead", &[], "please wait first, then tell me the day");
        r.intake(t, json!({"head": "Thread"}));
        let d1 = r.give("lead", t);
        r.owner_says(t, &["lead"], "and which century?");
        let d = r.m.due("lead").into_iter().find(|d| d.t == t).unwrap();
        r.go(Input::GivenInto { t, to: "lead".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).reply.unwrap();
        r.owner_says(t, &["lead"], "when you are done, list what changed");
        let n = r.m.tickets[&t].last();
        r.go(Input::Waits { t, n: Some(n) }).reply.unwrap();
        r.go(Input::Post { t, by: "lead".into(), text: Text::New("the 21st".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: Some(n - 1), kind: None }).reply.unwrap();
        assert!(r.m.tickets[&t].mu["lead"] < n, "{cut}: its own post did not read the waiting one");
        if cut {
            r.go(Input::TurnCut { agent: "lead".into() }).reply.unwrap();
            if r.asking_minder(t) {
                r.minder(t, json!({"head": "Close", "result": {"head": "Kept"}}));
            } else {
                r.cancel(t, "owner", "done").reply.unwrap();
            }
            assert!(!r.m.tickets[&t].is_open(), "{:?}", r.m.tickets[&t].status);
        } else {
            r.turn("lead", Some(&d1), 5);
        }
        let due = r.m.due("lead");
        assert!(due.iter().any(|d| d.t == t && !d.inform && matches!(&d.what, What::Posts { unread } | What::Closed { unread, .. } if unread.iter().any(|p| p.n == n))), "{cut}: {due:?}");
        r.give("lead", t);
        assert!(!r.m.due("lead").iter().any(|d| d.t == t), "{cut}: given once");
        r.replays();
    }
}

/// An agent's post held from another agent's turn (paper-research N-560 p7, p10: N-476.p20 to docket-edge7): A names
/// B while B is in a turn, so it waits for that turn's end; B posts on the same thread in that turn; B's own post does
/// not read A's, never handed to it, so when the turn is over B is given A's post, once. Replays as coordd reads it.
#[test]
fn a_post_held_from_a_turn_is_given_after_the_agents_own_post() {
    let mut r = Run::new();
    let t = r.say("lead", &[], "build the server, then tell me");
    r.intake(t, json!({"head": "Thread"}));
    let d1 = r.give("lead", t);
    // A (dev) names lead while lead's turn runs: an agent's post is not put into a running turn
    r.go(Input::Post { t, by: "dev".into(), text: Text::New("yes, build 1-4 as you wrote them".into()), names: vec!["lead".into()], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
    let n = r.m.tickets[&t].last();
    // lead, still in that turn, posts on the thread (its test result)
    r.go(Input::Post { t, by: "lead".into(), text: Text::New("the server's tests pass".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: Some(n - 1), kind: None }).reply.unwrap();
    assert!(r.m.tickets[&t].mu["lead"] < n, "its own post did not read the post it was never handed");
    r.turn("lead", Some(&d1), 5);
    let due = r.m.due("lead");
    assert!(due.iter().any(|d| d.t == t && !d.inform && matches!(&d.what, What::Posts { unread } if unread.iter().any(|p| p.n == n))), "{due:?}");
    r.give("lead", t);
    assert!(!r.m.due("lead").iter().any(|d| d.t == t), "given once");
    // what it was handed, its own post reads as before
    r.go(Input::Post { t, by: "lead".into(), text: Text::New("building".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
    assert_eq!(r.m.tickets[&t].mu["lead"], r.m.tickets[&t].last());
    r.replays();
}

/// A message sent to wait (`msg`/`say` with `wait`): its intake ticket is marked before the watcher places it, and the
/// owner's post intake makes of it waits.
#[test]
fn a_message_sent_to_wait_waits_once_placed() {
    let mut r = Run::new();
    let t = r.say("lead", &[], "when you are done, list what changed");
    r.go(Input::Waits { t, n: None }).reply.unwrap();
    r.intake(t, json!({"head": "Thread"}));
    let n = r.m.tickets[&t].posts.iter().find(|p| p.by == "owner").map(|p| p.n).unwrap();
    assert!(r.m.tickets[&t].waits.contains(&n), "{:?}", r.m.tickets[&t].waits);
    assert!(r.m.due("lead").iter().any(|d| d.t == t));
    assert!(r.go(Input::Waits { t, n: Some(99) }).reply.is_err(), "no such post");
    r.replays();
}

/// The rollback's leftover (paper-research N-101 p1334): a delivery recorded as given up to 0 (seen in the transcript
/// after a restart, nothing handed to say how far) stays due, and the session refuses it again as held already. The
/// next Given with its real upto is taken once: due no more, no turn counted for it; a third is refused.
#[test]
fn a_delivery_recorded_short_is_given_up_to_its_post_once() {
    let mut r = Run::new();
    let a = r.note("research", "core", "the plan");
    r.owner_says(a, &["research"], "how long?");
    if !r.woken("research", a) {
        r.route(json!({"head": "To", "agents": ["research"]}));
    }
    let d = r.m.due("research").into_iter().find(|d| d.t == a).unwrap();
    r.go(Input::Given { t: a, to: "research".into(), delivery: d.id.clone(), upto: 0, skipped: Default::default(), place: Some(Run::loc(5)) }).reply.unwrap();
    r.go(Input::TurnCut { agent: "research".into() });
    assert!(r.m.due("research").iter().any(|x| x.id == d.id), "recorded short, it is still due");
    let out = r.go(Input::Given { t: a, to: "research".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(!r.m.due("research").iter().any(|x| x.t == a), "{:?}", r.m.due("research"));
    assert!(r.m.given_since("research").is_empty(), "no turn is counted for it");
    assert!(r.go(Input::Given { t: a, to: "research".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).reply.is_err());
    r.replays();
}

/// The reply tool's `to` list (paper-research N-101 p1361): one answer to the owner's messages on two threads, posted
/// on the first; the second is recorded as answered from it, leaves `unanswered_owner`, and the turn's end does not
/// post on it again. An id that is no post is refused.
#[test]
fn one_answer_to_two_messages_records_both() {
    let mut r = Run::new();
    let a = r.note("research", "core", "the plan");
    let b = r.note("research", "core", "the release");
    let to_research = |r: &mut Run, t: u64, text: &str| {
        r.owner_says(t, &["research"], text);
        if !r.woken("research", t) {
            r.route(json!({"head": "To", "agents": ["research"]}));
        }
        r.m.tickets[&t].posts.last().unwrap().n
    };
    let na = to_research(&mut r, a, "how long?");
    let nb = to_research(&mut r, b, "and the release, same?");
    let da = r.give("research", a);
    r.give("research", b);
    let out = r.go(Input::Post { t: a, by: "research".into(), text: Text::New("an hour, both".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: Some(na), kind: None });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let n = r.m.tickets[&a].posts.last().unwrap().n;
    let idb = format!("{}.p{nb}", r.m.id(b));
    assert!(r.go(Input::AnswersAlso { t: a, n, also: vec![format!("{}.p99", r.m.id(b))] }).reply.is_err());
    let out = r.go(Input::AnswersAlso { t: a, n, also: vec![idb.clone()] });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert_eq!(r.m.tickets[&a].posts.last().unwrap().also, vec![idb]);
    assert!(r.m.view(a).unwrap().unanswered_owner.is_empty());
    assert!(r.m.view(b).unwrap().unanswered_owner.is_empty(), "answered from a");
    let out = r.turn("research", Some(&da), 1);
    assert!(!out.lines.iter().any(|l| matches!(l, Line::Replied { .. })), "{:?}", out.lines);
    r.replays();
}

// ---------------------------------------------------------------- slow mode (N-80; plan/layers.md, Part I §5, 3)

/// A kind shaped like the design's `part` (implement, then hand in; on red implement again, up to 3), and a thread
/// above it that asks the owner first, so it can be switched to slow before it opens anything.
const SLOW_KINDS: [(&str, &str); 2] = [
    (
        "work.kind",
        "kind work\n  letter W\n  form task: Text\n  result Text\n  raises NoPart, Exhausted, Stalled\n  pattern\n    iterate errs = \"\" up to 3 {\n      r <- step dev \"Implement {{ task }}. Last errors: {{ errs }}\" -> Commit(c: Text) | Cannot(why: Text);\n      match r {\n        Cannot(why) => raise NoPart(why),\n        Commit(c) => (v <- step role runner \"Hand in {{ c }}\" -> Green(top: Text) | Red(errors: Text);\n          match v { Green(top) => return Done(top), Red(errors) => return Again(errors) })\n      }\n    }\n",
    ),
    ("job.kind", "kind job\n  letter J\n  form task: Text\n  result Text\n  raises NoPart, Exhausted, Stalled\n  pattern\n    g <- step owner \"Go?\" -> Go(task: Text);\n    x <- open work(g.task);\n    return x\n"),
];

impl Run {
    fn slow_run() -> Run {
        let (d, errors) = defs_with(&SLOW_KINDS);
        assert!(errors.is_empty(), "{errors:?}");
        Run { m: Machine::load(d), log: Vec::new(), clock: 0 }
    }
    fn halts(&self, t: u64) -> Vec<ViewHalt> {
        self.m.view(t).unwrap().halts
    }
    fn approve(&mut self, t: u64, n: u32) -> Out {
        let out = self.go(Input::Approve { t, n, by: "owner".into() });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        out
    }
    fn halted_lines(out: &Out) -> Vec<(u64, String)> {
        out.lines.iter().filter_map(|l| if let Line::Halted { t, what, .. } = l { Some((*t, what.clone())) } else { None }).collect()
    }
    /// A job, switched to slow, its owner's go given: the work below it opened, halted (J-200, W-201).
    fn slow_job(&mut self) -> (u64, u64) {
        self.go(Input::Open { kind: "job".into(), under: None, opener: "owner".into(), form: json!({"task": "t1"}), text: None, names: vec![], told: vec![] }).reply.unwrap();
        let out = self.go(Input::Slow { t: 200, on: true, by: "owner".into() });
        assert!(out.reply.is_ok() && out.lines.len() == 1, "{out:?}");
        let out = self.go(Input::Answer { t: 200, by: "owner".into(), head: json!({"head": "Go", "task": "parse the header"}), text: Text::New(String::new()) });
        assert_eq!(Run::halted_lines(&out), vec![(201, "open".to_string())], "the owner's answer is not halted; the open it leads to is");
        (200, 201)
    }
    /// Through the open and the first step's give: dev holds step 1 of the work.
    fn slow_to_dev(&mut self) -> u64 {
        let (_, w) = self.slow_job();
        let out = self.approve(w, 1);
        assert_eq!(Run::halted_lines(&out), vec![(w, "give".to_string())]);
        assert!(self.m.due("dev").is_empty(), "a halted step is not given");
        self.approve(w, 2);
        self.take("dev");
        w
    }
}

/// The owner's "no" in slow mode (N-1788): a halted give refused cancels its ticket, nothing given and nothing left
/// halted; a halt refused twice, or by an agent, is refused; the line replays.
#[test]
fn slow_a_halt_refused_cancels_what_it_held_and_leaves_nothing_halted() {
    let mut r = Run::slow_run();
    let (_, w) = r.slow_job();
    r.approve(w, 1);
    assert_eq!(r.halts(w).len(), 1, "the first step's give halts");
    assert!(r.go(Input::HaltReject { t: w, n: 2, by: "dev".into(), reason: None }).reply.is_err(), "only the owner refuses");
    let out = r.go(Input::HaltReject { t: w, n: 2, by: "owner".into(), reason: Some("not this task".into()) });
    assert_eq!(out.reply.as_deref(), Ok(format!("{} is cancelled: halt 2 refused: not this task.", r.m.id(w)).as_str()));
    assert!(out.lines.iter().any(|l| matches!(l, Line::HaltReject { n: 2, reason: Some(why), .. } if why == "not this task")));
    let v = r.m.view(w).unwrap();
    assert!(v.halts.is_empty(), "nothing is left halted");
    assert!(matches!(&v.status, ViewStatus::Failed { raised } if raised.exc == "Cancelled" && raised.value["why"] == "halt 2 refused: not this task"), "{:?}", v.status);
    assert!(r.m.due("dev").is_empty(), "the refused step was never given");
    assert!(r.go(Input::HaltReject { t: w, n: 2, by: "owner".into(), reason: None }).reply.is_err(), "a halt is refused once");
    r.replays_with(&SLOW_KINDS);
}

#[test]
fn slow_a_ticket_opened_halts_and_its_form_is_edited() {
    let mut r = Run::slow_run();
    let (j, w) = r.slow_job();
    let v = r.m.view(w).unwrap();
    assert!(v.slow && v.slow_by.as_deref() == Some("J-200"), "the child halts because its parent is slow");
    assert_eq!(r.m.view(j).unwrap().halted, vec!["W-201".to_string()]);
    let [ViewHalt::Open { n: 1, kind, form, .. }] = &v.halts[..] else { panic!("{:?}", v.halts) };
    assert_eq!((kind.as_str(), form["task"].as_str()), ("work", Some("parse the header")));
    assert!(v.continuation.as_deref().is_some_and(|c| c.contains("kind work")), "what comes after: its program");
    assert!(matches!(v.status, ViewStatus::Waits { .. }) && v.spent == 0, "its program has not started");
    // a form that does not type is refused; one that does is shown, and still waits
    assert!(r.go(Input::HaltEdit { t: w, n: 1, by: "owner".into(), value: json!({"form": {"task": 3}}) }).reply.is_err());
    r.go(Input::HaltEdit { t: w, n: 1, by: "owner".into(), value: json!({"form": {"task": "parse the footer"}}) }).reply.unwrap();
    assert!(matches!(&r.halts(w)[..], [ViewHalt::Open { form, .. }] if form["task"] == "parse the footer"));
    assert!(r.go(Input::Approve { t: w, n: 1, by: "dev".into() }).reply.is_err(), "only the owner approves");
    r.approve(w, 1);
    let ViewHalt::Give { worker, prompt, edited, .. } = &r.halts(w)[0] else { panic!() };
    assert_eq!((worker.as_str(), prompt.as_str(), *edited), ("dev", "", false));
    let ViewStatus::Awaits { step: 1, .. } = r.status(w) else { panic!() };
    r.approve(w, 2);
    let What::Step { template, prompt: None, .. } = r.take("dev").what else { panic!() };
    assert!(template.unwrap().contains("parse the footer"), "it ran on the edited form");
}

#[test]
fn slow_a_step_given_halts_and_its_prompt_and_worker_are_edited() {
    let mut r = Run::slow_run();
    let (_, w) = r.slow_job();
    r.approve(w, 1);
    // the prompt: the owner's text is what is sent
    r.go(Input::HaltEdit { t: w, n: 2, by: "owner".into(), value: json!({"prompt": "Only the header, please."}) }).reply.unwrap();
    assert!(matches!(&r.halts(w)[..], [ViewHalt::Give { edited: true, prompt, .. }] if prompt == "Only the header, please."));
    // the worker
    r.go(Input::HaltEdit { t: w, n: 2, by: "owner".into(), value: json!({"worker": "ann"}) }).reply.unwrap();
    assert!(r.go(Input::HaltEdit { t: w, n: 2, by: "owner".into(), value: json!({"nothing": 1}) }).reply.is_err());
    assert!(r.m.due("dev").is_empty() && r.m.due("ann").is_empty(), "an edit does not let it through");
    let out = r.approve(w, 2);
    assert!(out.effects.contains(&Effect::Deliver { to: "ann".into() }));
    let What::Step { prompt, .. } = r.take("ann").what else { panic!() };
    assert_eq!(prompt.as_deref(), Some("Only the header, please."));
    assert!(r.m.due("dev").is_empty());
}

#[test]
fn slow_an_answer_taken_halts_its_head_is_edited_and_a_new_answer_replaces_it() {
    let mut r = Run::slow_run();
    let w = r.slow_to_dev();
    r.go(Input::TurnEnded { agent: "dev".into(), delivery: None, text: Run::loc(2), later: vec![], stood: None, go: None });
    let out = r.go(Input::Formed { t: w, step: 1, head: Some(json!({"head": "Commit", "c": "abc"})), why: None, not_taken: None, call: None });
    assert_eq!(Run::halted_lines(&out), vec![(w, "take".to_string())]);
    let [ViewHalt::Take { n: 3, worker, words, head, .. }] = &r.halts(w)[..] else { panic!("{:?}", r.halts(w)) };
    assert_eq!((worker.as_str(), head["c"].as_str(), words), ("dev", Some("abc"), &Text::Loc { loc: Run::loc(2) }));
    assert!(matches!(r.status(w), ViewStatus::Awaits { step: 1, forming: false, .. }), "not taken yet");
    // the owner writes on the thread, the agent answers again: that answer halts in its place
    let out = r.go(Input::Answer { t: w, by: "dev".into(), head: json!({"head": "Commit", "c": "abd"}), text: Text::New("again".into()) });
    assert!(out.reply.as_ref().unwrap().contains("waits for the owner"));
    let [ViewHalt::Take { n: 4, head, .. }] = &r.halts(w)[..] else { panic!("{:?}", r.halts(w)) };
    assert_eq!(head["c"], "abd");
    // the head must type
    assert!(r.go(Input::HaltEdit { t: w, n: 4, by: "owner".into(), value: json!({"head": {"head": "Merged"}}) }).reply.is_err());
    r.go(Input::HaltEdit { t: w, n: 4, by: "owner".into(), value: json!({"head": {"head": "Commit", "c": "abe"}}) }).reply.unwrap();
    let out = r.approve(w, 4);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Posted { step: Some(1), .. })), "taken: the answer's post");
    let p = r.m.view(w).unwrap().posts.into_iter().find(|p| p.step == Some(1)).unwrap();
    assert_eq!(p.head.unwrap()["c"], "abe", "the edited head; the agent's words are its own");
    assert!(Run::halted_lines(&out).is_empty(), "the runner's role is asked, not yet answered");
    assert!(out.effects.iter().any(|e| matches!(e, Effect::Ask { question: Question::Fills { role, .. }, .. } if role == "runner")));
}

#[test]
fn slow_a_policy_answer_halts_and_is_edited() {
    let mut r = Run::slow_run();
    let w = r.slow_to_dev();
    r.go(Input::Answer { t: w, by: "dev".into(), head: json!({"head": "Commit", "c": "abc"}), text: Text::New(String::new()) });
    r.approve(w, 3);
    let out = r.go(Input::Decided { t: w, step: 2, question: "fills".into(), answer: json!("runner"), provenance: json!({"rule": "fills(runner, runner)"}) });
    assert_eq!(Run::halted_lines(&out), vec![(w, "decide".to_string())]);
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::Deliver { .. })));
    let [ViewHalt::Decide { question, answer, provenance, role, .. }] = &r.halts(w)[..] else { panic!("{:?}", r.halts(w)) };
    assert_eq!((question.as_str(), answer, provenance["rule"].as_str(), role.as_deref()), ("fills", &json!("runner"), Some("fills(runner, runner)"), Some("runner")));
    assert!(r.m.resume_effects().iter().all(|e| !matches!(e, Effect::Ask { .. })), "a halted answer is not asked again");
    assert!(r.go(Input::HaltEdit { t: w, n: 4, by: "owner".into(), value: json!({"answer": 3}) }).reply.is_err());
    r.go(Input::HaltEdit { t: w, n: 4, by: "owner".into(), value: json!({"answer": "runner2"}) }).reply.unwrap();
    let out = r.approve(w, 4);
    assert_eq!(Run::halted_lines(&out), vec![(w, "give".to_string())], "the answer used: the step to the edited worker, which halts in turn");
    let ViewHalt::Give { worker, .. } = &r.halts(w)[0] else { panic!() };
    assert_eq!(worker, "runner2");
}

#[test]
fn slow_a_post_delivered_halts_and_whom_it_wakes_is_edited_but_not_the_owners() {
    let mut r = Run::slow_run();
    let (j, w) = r.slow_job();
    // the owner's words naming whom they are for: a rule of his post's handler tells them, and that makes no card
    // (the owner N-94, 1)
    let out = r.go(Input::Post { t: w, by: "owner".into(), text: Text::New("start small".into()), names: vec!["lead".into()], told: vec![], inform: vec![], answers: None, kind: None });
    assert!(out.lines.iter().any(|l| matches!(l, Line::Told { .. })), "{:?}", out.lines);
    assert_eq!(Run::halted_lines(&out), vec![]);
    assert!(matches!(&r.m.view(w).unwrap().posts[0].text, Text::New(s) if s == "start small"));
    assert_eq!(r.m.due("lead").len(), 1);
    r.take("lead");
    let out = r.go(Input::Post { t: w, by: "lead".into(), text: Text::New("ann, look".into()), names: vec!["ann".into()], told: vec![], inform: vec![], answers: None, kind: None });
    assert_eq!(Run::halted_lines(&out), vec![(w, "deliver".to_string())]);
    assert!(r.m.due("ann").is_empty(), "halted: given to nobody");
    let ViewHalt::Deliver { post, wakes, by, .. } = r.halts(w).into_iter().find(|h| matches!(h, ViewHalt::Deliver { .. })).unwrap() else { panic!() };
    assert_eq!((post, wakes, by.as_str()), (2, vec!["ann".to_string()], "lead"));
    let n = r.m.tickets[&w].halts.iter().find(|h| h.what.name() == "deliver").unwrap().n;
    r.go(Input::HaltEdit { t: w, n, by: "owner".into(), value: json!({"wakes": ["bob"]}) }).reply.unwrap();
    r.approve(w, n);
    assert!(r.m.due("ann").is_empty());
    assert_eq!(r.m.due("bob").len(), 1, "edited: bob is woken instead");
    let _ = j;
}

#[test]
fn slow_time_halted_is_not_waiting_and_expired_halts_too() {
    let mut r = Run::slow_run();
    let (_, w) = r.slow_job();
    r.approve(w, 1);
    assert!(r.m.awaiting().is_empty(), "a step halted before it is given is not asked `expired`");
    r.clock = 40;
    r.approve(w, 2);
    let aw = r.m.awaiting();
    assert_eq!(aw.len(), 1);
    assert_eq!(aw[0].3, "2026-11-10T12:00:41Z", "its wait starts at the approval");
    let out = r.go(Input::Expired { t: w, step: 1, provenance: json!({"rule": "expired(s) :- waited(s, 1h)"}) });
    assert_eq!(Run::halted_lines(&out), vec![(w, "decide".to_string())]);
    assert!(r.m.awaiting().is_empty(), "nor while that answer halts");
    r.go(Input::HaltEdit { t: w, n: 3, by: "owner".into(), value: json!({"answer": false}) }).reply.unwrap();
    r.approve(w, 3);
    assert!(matches!(r.status(w), ViewStatus::Awaits { step: 1, .. }), "edited to not expired: the step stays");
    assert_eq!(r.m.awaiting()[0].3, "2026-11-10T12:00:44Z", "and waits again from then");
}

#[test]
fn slow_off_halts_nothing_more_and_a_replay_gives_the_same_state() {
    let mut r = Run::slow_run();
    let w = r.slow_to_dev();
    r.go(Input::Answer { t: w, by: "dev".into(), head: json!({"head": "Commit", "c": "abc"}), text: Text::New(String::new()) });
    r.go(Input::HaltEdit { t: w, n: 3, by: "owner".into(), value: json!({"head": {"head": "Commit", "c": "abz"}}) }).reply.unwrap();
    r.approve(w, 3);
    r.go(Input::Decided { t: w, step: 2, question: "fills".into(), answer: json!("runner"), provenance: json!({}) });
    // switched off: the halt waiting goes through (paper-research N-89.p2, 3); nothing new halts
    assert_eq!(r.halts(w).len(), 1);
    r.go(Input::Slow { t: 200, on: false, by: "owner".into() }).reply.unwrap();
    assert!(r.halts(w).is_empty() && r.m.due("runner").len() == 1, "off: the step is given at once");
    let mut again = Machine::load(crate::machine::Defs::clone(&r.m.defs));
    let problems = again.replay(&r.log);
    assert!(problems.is_empty(), "{problems:?}");
    for t in [200, w] {
        assert_eq!(again.view(t), r.m.view(t), "{t}");
    }
    assert_eq!(again.tickets[&w].halts, r.m.tickets[&w].halts);
    assert_eq!(again.tickets[&w].halted, 4);
}

/// paper-research N-89.p2, 2: what starts an agent's turn on a slow thread halts first; the closing notice to the
/// agent who opened the ticket halts as a `notice`, showing it and whom it wakes, and goes once approved. Insight (3):
/// closed with the notice halted, it owes him and is open in the outline; it ends when he lets the notice go
/// (`Ticket::halt_gone`, `view::stopped_owing`), not at its close, and the replay keeps that moment.
#[test]
fn a_closing_notice_on_a_slow_thread_halts_until_approved() {
    let (d, errors) = defs_with(&[("quick.kind", "kind quick\n  letter K\n  form task: Text\n  result Text\n  raises Stalled\n  pattern\n    r <- step dev \"Do {{ task }}\" -> Did(what: Text);\n    return r.what\n")]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d.clone()), log: Vec::new(), clock: 0 };
    r.go(Input::Open { kind: "quick".into(), under: None, opener: "lead".into(), form: json!({"task": "t"}), text: None, names: vec![], told: vec![] }).reply.unwrap();
    r.go(Input::Slow { t: 200, on: true, by: "owner".into() }).reply.unwrap();
    // the step was given before the switch; its answer halts as a take
    r.take("dev");
    let out = r.go(Input::Answer { t: 200, by: "dev".into(), head: json!({"head": "Did", "what": "done"}), text: Text::New(String::new()) });
    assert_eq!(Run::halted_lines(&out), vec![(200, "take".to_string())]);
    let out = r.approve(200, 1);
    assert!(matches!(&r.m.tickets[&200].status, Status::Done(_)));
    assert_eq!(Run::halted_lines(&out), vec![(200, "notice".to_string())], "{out:?}");
    assert!(r.m.due("lead").is_empty(), "the closing notice waits for the owner");
    let h = r.halts(200);
    assert!(matches!(&h[..], [ViewHalt::Notice { n: 2, to, notice, delivery, .. }] if to == "lead" && notice == "closed" && delivery.starts_with("K-200.c")), "{h:?}");
    let outline = |m: &Machine| crate::view::ViewOutline::of(m, &m.tickets[&200], false);
    assert_eq!((outline(&r.m).open, outline(&r.m).ended_at), (true, None), "closed, the notice halted owes him");
    let closed = r.m.tickets[&200].closed_at.clone();
    r.approve(200, 2);
    let gone = r.m.tickets[&200].halt_gone.clone();
    assert!(closed.is_some() && gone > closed, "{gone:?} after {closed:?}");
    assert_eq!((outline(&r.m).open, outline(&r.m).ended_at), (false, gone.clone()), "it ends when he lets the notice go");
    let due = r.m.due("lead");
    assert!(matches!(&due[..], [d] if matches!(d.what, What::Closed { .. })), "{due:?}");
    // given and handed back unread: not halted again
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert!(m2.tickets[&200].halts.is_empty());
    assert_eq!(outline(&m2).ended_at, gone, "the replay ends it at the same moment");
}

/// paper-research N-89.p2, 3: turning slow off lets what is waiting through, in order.
#[test]
fn turning_slow_off_lets_what_waits_through_in_order() {
    let mut r = Run::slow_run();
    let (j, w) = r.slow_job();
    // a halted ticket waits on the owner (N-88)
    let waits = r.m.view(j).unwrap().waits_on;
    assert!(waits.iter().any(|x| x.ticket == r.m.id(w) && x.agent == "owner" && x.call == "halt"), "{waits:?}");
    let out = r.go(Input::Post { t: j, by: "dev".into(), text: Text::New("a note".into()), names: vec!["lead".into()], told: vec![], inform: vec![], answers: None, kind: None });
    assert_eq!(Run::halted_lines(&out), vec![(j, "deliver".to_string())]);
    assert!(r.m.due("lead").is_empty());
    let out = r.go(Input::Slow { t: j, on: false, by: "owner".into() });
    assert!(out.reply.as_ref().unwrap().contains("the 2 halted under it went through"), "{:?}", out.reply);
    assert!(Run::halted_lines(&out).is_empty(), "nothing more halts: {out:?}");
    assert!(r.m.tickets[&j].halts.is_empty() && r.m.tickets[&w].halts.is_empty());
    // the open (halted first) ran, its step given at once; then the post
    assert!(r.m.due("dev").iter().any(|d| d.t == w && matches!(d.what, What::Step { .. })), "{:?}", r.m.due("dev"));
    assert!(r.m.due("lead").iter().any(|d| d.t == j), "{:?}", r.m.due("lead"));
    let (d, _) = defs_with(&SLOW_KINDS);
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
}

/// The owner's "slow mode per project" (N-412 ask 5): slow set over the scope "project", and every ticket halts as if
/// it were slow, a ticket opened after it too; lifted, what halted goes through in the order it halted, but for what a
/// slow ticket still holds. Whom the policy lets set it coordd asks (coordd ticket tests).
#[test]
fn slow_set_over_the_project_halts_every_ticket_and_lifted_lets_them_through_in_order() {
    let mut r = Run::slow_run();
    let set = |on: bool| Input::Slowed { scope: "project".into(), on, by: "owner".into() };
    // J-200 before it is set; nothing of it halts yet
    r.go(Input::Open { kind: "job".into(), under: None, opener: "owner".into(), form: json!({"task": "t1"}), text: None, names: vec![], told: vec![] }).reply.unwrap();
    assert!(r.go(Input::Slowed { scope: "agent".into(), on: true, by: "owner".into() }).reply.is_err(), "no such scope");
    let out = r.go(set(true));
    assert!(out.reply.as_ref().is_ok_and(|s| s.starts_with("Slow is set over the project")) && out.lines.len() == 1, "{out:?}");
    assert!(out.effects.contains(&Effect::Changed { t: 200 }), "the open ticket's view changes");
    let out = r.go(set(true));
    assert!(out.reply.is_ok() && out.lines.is_empty(), "set already: nothing written");
    let v = r.m.view(200).unwrap();
    assert_eq!((v.slow, v.slow_by.as_deref(), v.slow_scope.as_deref()), (true, None, Some("project")), "slow_by stays a ticket's");
    // the owner's answer does not halt; the work it opens does
    let out = r.go(Input::Answer { t: 200, by: "owner".into(), head: json!({"head": "Go", "task": "parse the header"}), text: Text::New(String::new()) });
    assert_eq!(Run::halted_lines(&out), vec![(201, "open".to_string())]);
    // a ticket the owner opens after it is set, at the top: not halted at its opening; its step to dev halts
    let out = r.go(Input::Open { kind: "work".into(), under: None, opener: "owner".into(), form: json!({"task": "t2"}), text: None, names: vec![], told: vec![] });
    assert_eq!(Run::halted_lines(&out), vec![(202, "give".to_string())], "its program ran: only the give halts");
    assert!(r.m.due("dev").is_empty());
    // an agent's post on a ticket that is not switched itself halts
    let out = r.go(Input::Post { t: 200, by: "dev".into(), text: Text::New("a note".into()), names: vec!["lead".into()], told: vec![], inform: vec![], answers: None, kind: None });
    assert_eq!(Run::halted_lines(&out), vec![(200, "deliver".to_string())]);
    // a ticket's own switch: its halts stay when the project's is lifted; switching one off says the project holds it
    r.go(Input::Open { kind: "work".into(), under: None, opener: "owner".into(), form: json!({"task": "t3"}), text: None, names: vec![], told: vec![] }).reply.unwrap();
    assert_eq!(r.halts(203).len(), 1);
    let out = r.go(Input::Slow { t: 202, on: false, by: "owner".into() });
    assert!(out.reply.as_ref().is_ok_and(|s| s.contains("halts while slow is set over the project")), "{:?}", out.reply);
    r.go(Input::Slow { t: 203, on: true, by: "owner".into() }).reply.unwrap();
    let out = r.go(set(false));
    assert!(out.reply.as_ref().is_ok_and(|s| s.contains("the 3 halted went through, in order")), "{:?}", out.reply);
    assert!(Run::halted_lines(&out).is_empty(), "nothing more halts: {out:?}");
    assert!([200, 201, 202].iter().all(|t| r.m.tickets[t].halts.is_empty()));
    assert_eq!(r.halts(203).len(), 1, "W-203's own switch still holds its step");
    let v = r.m.view(202).unwrap();
    assert!(!v.slow && v.slow_by.is_none() && v.slow_scope.is_none());
    assert_eq!(r.m.view(203).unwrap().slow_by.as_deref(), Some("W-203"));
    // in the order they halted: W-201's open (its step given at once), W-202's step, the post to lead
    let mut dev: Vec<u64> = r.m.due("dev").iter().filter(|d| matches!(d.what, What::Step { .. })).map(|d| d.t).collect();
    dev.sort();
    assert_eq!(dev, vec![201, 202]);
    assert!(r.m.due("lead").iter().any(|d| d.t == 200), "{:?}", r.m.due("lead"));
    // it is in the record: a replay comes to the same, and one cut just after it was set is slow
    let (d, _) = defs_with(&SLOW_KINDS);
    let mut m2 = Machine::load(d.clone());
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert!(m2.slowed.is_empty());
    for t in 200..=203 {
        assert_eq!(m2.view(t), r.m.view(t), "{t}");
    }
    let on = r.log.iter().position(|l| matches!(l, Line::Slowed { on: true, .. })).unwrap();
    let mut m3 = Machine::load(d);
    assert_eq!(m3.replay(&r.log[..=on]), Vec::<String>::new());
    assert!(m3.slowed.contains("project") && m3.view(200).unwrap().slow_scope.as_deref() == Some("project"));
    let line = serde_json::to_value(&r.log[on]).unwrap();
    assert_eq!((line["ev"].as_str(), line["scope"].as_str(), line["on"].as_bool()), (Some("slowed"), Some("project"), Some(true)));
}

// ---------------------------------------------------------------- the language (plan/layers.md I §3)

const LIST_KINDS: [(&str, &str); 3] = [
    (
        "fan.kind",
        "kind fan\n  letter F\n  form tasks: [Text]\n  result [Text]\n  raises Exhausted, Stalled\n  pattern\n    par x in tasks up to 3 {\n      r <- step dev \"Do {{ x }}\" -> Did(what: Text);\n      return r.what\n    }\n",
    ),
    (
        "first.kind",
        "kind first\n  letter G\n  form tasks: [Text]\n  result [Text]\n  raises Exhausted, Stalled\n  pattern\n    any 2 of x in tasks up to 3 {\n      r <- step dev \"Do {{ x }}\" -> Did(what: Text);\n      return r.what\n    }\n",
    ),
    (
        "pick.kind",
        "kind pick\n  letter Q\n  form task: Text\n  result Text\n  raises Stalled\n  pattern\n    p <- step owner \"Who does {{ task }}?\" -> Pick(who: Agent);\n    r <- step p.who \"Do {{ task }}\" -> Did(what: Text);\n    return r.what\n",
    ),
];

fn list_run() -> Run {
    let (d, errors) = defs_with(&LIST_KINDS);
    assert!(errors.is_empty(), "{errors:?}");
    Run { m: Machine::load(d), log: Vec::new(), clock: 0 }
}

impl Run {
    fn open_form(&mut self, kind: &str, form: Value) -> Out {
        self.go(Input::Open { kind: kind.into(), under: None, opener: "owner".into(), form, text: None, names: vec![], told: vec![] })
    }
    fn list_replays(&self) {
        let (d, _) = defs_with(&LIST_KINDS);
        let mut m2 = Machine::load(d);
        assert_eq!(m2.replay(&self.log), Vec::<String>::new());
    }
}

#[test]
fn par_over_a_list_runs_a_branch_per_element_and_gives_the_list() {
    let mut r = list_run();
    r.open_form("fan", json!({"tasks": ["a", "b"]})).reply.unwrap();
    let kids = r.m.tickets[&200].children.clone();
    assert_eq!(kids.len(), 2);
    let due = r.m.due("dev");
    assert_eq!(due.len(), 2, "one step per element: {due:?}");
    for (i, c) in kids.iter().enumerate() {
        let d = r.m.due("dev").into_iter().find(|d| d.t == *c).unwrap();
        r.go(Input::Given { t: d.t, to: "dev".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Run::loc(1)) });
        r.answer(*c, "dev", json!({"head": "Did", "what": format!("did {}", ["a", "b"][i])}));
    }
    assert!(matches!(&r.m.tickets[&200].status, Status::Done(v) if *v == json!(["did a", "did b"])), "{:?}", r.m.tickets[&200].status);
    r.list_replays();
}

#[test]
fn par_over_a_list_longer_than_its_most_opens_nothing_and_raises() {
    let mut r = list_run();
    r.open_form("fan", json!({"tasks": ["a", "b", "c", "d"]})).reply.unwrap();
    assert!(r.m.tickets[&200].children.is_empty(), "nothing opened");
    assert!(matches!(&r.m.tickets[&200].status, Status::Failed(x) if x.exc == "Exhausted" && x.value["branches"] == 4 && x.value["most"] == 3), "{:?}", r.m.tickets[&200].status);
    // the empty list: the empty result at once
    r.open_form("fan", json!({"tasks": []})).reply.unwrap();
    assert!(matches!(&r.m.tickets[&201].status, Status::Done(v) if *v == json!([])), "{:?}", r.m.tickets[&201].status);
    r.list_replays();
}

#[test]
fn any_over_a_list_takes_the_first_n_and_raises_on_too_few() {
    let mut r = list_run();
    r.open_form("first", json!({"tasks": ["a", "b", "c"]})).reply.unwrap();
    let kids = r.m.tickets[&200].children.clone();
    assert_eq!(kids.len(), 3);
    for c in [kids[2], kids[0]] {
        let d = r.m.due("dev").into_iter().find(|d| d.t == c).unwrap();
        r.go(Input::Given { t: d.t, to: "dev".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Run::loc(1)) });
        r.answer(c, "dev", json!({"head": "Did", "what": format!("did {c}")}));
    }
    assert!(matches!(&r.m.tickets[&200].status, Status::Done(v) if *v == json!([format!("did {}", kids[2]), format!("did {}", kids[0])])), "{:?}", r.m.tickets[&200].status);
    assert!(!r.m.tickets[&kids[1]].is_open(), "the branch not needed is cancelled");
    let n = *r.m.tickets.keys().filter(|t| **t < HIDDEN).max().unwrap() + 1;
    r.open_form("first", json!({"tasks": ["a"]})).reply.unwrap();
    assert!(matches!(&r.m.tickets[&n].status, Status::Failed(x) if x.exc == "Exhausted"), "any 2 of one: {:?}", r.m.tickets[&n].status);
    r.list_replays();
}

#[test]
fn a_worker_may_be_any_agent_expression() {
    let mut r = list_run();
    r.open_form("pick", json!({"task": "the header"})).reply.unwrap();
    r.answer(200, "owner", json!({"head": "Pick", "who": "ann"}));
    let d = r.take("ann");
    assert_eq!(d.t, 200);
    r.answer(200, "ann", json!({"head": "Did", "what": "parsed"}));
    assert!(matches!(&r.m.tickets[&200].status, Status::Done(v) if *v == json!("parsed")));
    // printed back as written
    assert!(crate::print::prog(&r.m.defs.kinds["pick"].pattern, 0).contains("step p.who \"Do {{ task }}\""), "{}", crate::print::prog(&r.m.defs.kinds["pick"].pattern, 0));
    r.list_replays();
}

#[test]
fn the_checker_refuses_a_worker_that_is_not_an_agent_and_branches_not_from_a_list() {
    let bad = [
        ("w.kind", "kind w\n  form task: Text\n  result Text\n  raises Stalled\n  pattern\n    p <- step owner \"?\" -> Pick(n: Text);\n    r <- step p.n \"Do\" -> Did(what: Text);\n    return r.what\n"),
        ("l.kind", "kind l\n  form task: Text\n  result [Text]\n  raises Exhausted, Stalled\n  pattern\n    par x in task up to 2 { r <- step dev \"Do\" -> Did(what: Text); return r.what }\n"),
        ("m.kind", "kind m\n  form tasks: [Text]\n  result [Text]\n  raises Stalled\n  pattern\n    par x in tasks up to 2 { r <- step dev \"Do\" -> Did(what: Text); return r.what }\n"),
    ];
    let (_, errors) = defs_with(&bad);
    assert_eq!(errors.len(), 3, "{errors:#?}");
    let of = |k: &str| errors.iter().find(|e| e.contains(&format!("kind {k} refused"))).unwrap_or_else(|| panic!("{k}: {errors:#?}"));
    assert!(of("w").contains("not an agent"), "{}", of("w"));
    assert!(of("l").contains("made from a list"), "{}", of("l"));
    assert!(of("m").contains("Exhausted"), "{}", of("m"));
}

const FOR_KINDS: [(&str, &str); 1] = [(
    "seq.kind",
    "kind seq\n  letter Z\n  form tasks: [Text]\n  result [Text]\n  raises Exhausted, Broke, Stalled\n  pattern\n    for x in tasks up to 3 {\n      r <- step dev \"Do {{ x }}\" -> Did(what: Text) | Fail(why: Text);\n      match r { Did(what) => return what, Fail(why) => raise Broke(why) }\n    }\n",
)];

fn for_run() -> Run {
    let (d, errors) = defs_with(&FOR_KINDS);
    assert!(errors.is_empty(), "{errors:?}");
    Run { m: Machine::load(d), log: Vec::new(), clock: 0 }
}

impl Run {
    fn for_replays(&self) {
        let (d, _) = defs_with(&FOR_KINDS);
        let mut m2 = Machine::load(d);
        assert_eq!(m2.replay(&self.log), Vec::<String>::new());
    }
}

#[test]
fn for_runs_one_round_per_element_in_order_and_gives_the_list() {
    let mut r = for_run();
    r.open_form("seq", json!({"tasks": ["a", "b", "c"]})).reply.unwrap();
    for x in ["a", "b", "c"] {
        let d = r.take("dev");
        assert_eq!(d.t, 200);
        assert!(r.m.due("dev").is_empty(), "one round at a time");
        assert!(format!("{:?}", d.what).contains(&format!("Do {x}")), "{x}: {:?}", d.what);
        r.answer(200, "dev", json!({"head": "Did", "what": format!("did {x}")}));
    }
    assert!(matches!(&r.m.tickets[&200].status, Status::Done(v) if *v == json!(["did a", "did b", "did c"])), "{:?}", r.m.tickets[&200].status);
    assert!(crate::print::prog(&r.m.defs.kinds["seq"].pattern, 0).contains("for x in tasks up to 3 {"));
    r.for_replays();
}

#[test]
fn a_round_that_raises_ends_the_for_and_too_long_raises_before_anything_runs() {
    let mut r = for_run();
    r.open_form("seq", json!({"tasks": ["a", "b", "c"]})).reply.unwrap();
    r.take("dev");
    r.answer(200, "dev", json!({"head": "Fail", "why": "no"}));
    assert!(matches!(&r.m.tickets[&200].status, Status::Failed(x) if x.exc == "Broke"), "{:?}", r.m.tickets[&200].status);
    assert!(r.m.due("dev").is_empty(), "no round after the raise");
    r.open_form("seq", json!({"tasks": ["a", "b", "c", "d"]})).reply.unwrap();
    assert!(r.m.due("dev").is_empty(), "nothing ran");
    assert!(matches!(&r.m.tickets[&201].status, Status::Failed(x) if x.exc == "Exhausted" && x.value["rounds"] == 4 && x.value["most"] == 3), "{:?}", r.m.tickets[&201].status);
    r.open_form("seq", json!({"tasks": []})).reply.unwrap();
    assert!(matches!(&r.m.tickets[&202].status, Status::Done(v) if *v == json!([])), "{:?}", r.m.tickets[&202].status);
    r.for_replays();
}

#[test]
fn the_checker_wants_a_list_and_exhausted_for_a_for() {
    let bad = [
        ("l.kind", "kind l\n  form task: Text\n  result [Text]\n  raises Exhausted\n  pattern\n    for x in task up to 2 { return x }\n"),
        ("m.kind", "kind m\n  form tasks: [Text]\n  result [Text]\n  pattern\n    for x in tasks up to 2 { return x }\n"),
    ];
    let (_, errors) = defs_with(&bad);
    assert_eq!(errors.len(), 2, "{errors:#?}");
    let of = |k: &str| errors.iter().find(|e| e.contains(&format!("kind {k} refused"))).unwrap_or_else(|| panic!("{k}: {errors:#?}"));
    assert!(of("l").contains("over a list"), "{}", of("l"));
    assert!(of("m").contains("Exhausted"), "{}", of("m"));
    let (_, ok) = defs_with(&[("o.kind", "kind o\n  form tasks: [Text]\n  result [Text]\n  raises Exhausted\n  pattern\n    for x in tasks up to 2 { return x }\n")]);
    assert!(ok.is_empty(), "{ok:?}");
}

const LAYER_KINDS: [(&str, &str); 2] = [
    (
        "lay.kind",
        "kind lay\n  letter Y\n  form task: Text, base: Ref\n  result Text\n  raises Stalled\n  pattern\n    u <- layer on base {\n      r <- step fresh \"Do {{ task }}\" -> Commit(c: Ref);\n      return r.c\n    };\n    v <- step fresh \"Look at {{ task }}\" -> Seen;\n    return \"laid\"\n",
    ),
    (
        "both.kind",
        "kind both\n  letter B\n  form tasks: [Text]\n  result Text\n  raises Exhausted, Stalled\n  pattern\n    u <- layer {\n      par x in tasks up to 2 {\n        r <- step fresh \"Do {{ x }}\" -> Commit(c: Ref);\n        return r.c\n      }\n    };\n    return \"done\"\n",
    ),
];

fn layer_run() -> Run {
    let (d, errors) = defs_with(&LAYER_KINDS);
    assert!(errors.is_empty(), "{errors:?}");
    Run { m: Machine::load(d), log: Vec::new(), clock: 0 }
}

#[test]
fn a_fresh_agent_in_a_layer_starts_in_it_and_the_layer_ends_with_its_scope() {
    let mut r = layer_run();
    let out = r.open_form("lay", json!({"task": "median", "base": "abc123"}));
    assert_eq!(r.m.active_layers(), vec![("Y-200.l1".to_string(), 200, Some("abc123".to_string()))]);
    assert!(out.effects.contains(&Effect::Start { agent: "f200-1".into(), t: 200, call: "inline".into(), layer: Some("Y-200.l1".into()) }), "{:?}", out.effects);
    let d = r.take("f200-1");
    r.answer(d.t, "f200-1", json!({"head": "Commit", "c": "def456"}));
    // out of the layer: none active, and the next fresh agent starts where it would without one
    assert!(r.m.active_layers().is_empty());
    assert!(r.m.due("f200-2").len() == 1);
    assert_eq!(r.m.layer_of(200), None);
    let text = crate::print::prog(&r.m.defs.kinds["lay"].pattern, 0);
    assert!(text.contains("u <- layer on base {"), "{text}");
    let (d, _) = defs_with(&LAYER_KINDS);
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
}

#[test]
fn the_branches_of_a_par_in_a_layer_share_it_and_a_cancel_ends_it() {
    let mut r = layer_run();
    let out = r.open_form("both", json!({"tasks": ["a", "b"]}));
    assert_eq!(r.m.active_layers(), vec![("B-200.l1".to_string(), 200, None)], "without on: the root's head, coordd's to find");
    let kids = r.m.tickets[&200].children.clone();
    for c in &kids {
        assert_eq!(r.m.layer_of(*c), Some("B-200.l1".into()), "a branch shares its parent's layer");
        assert!(out.effects.iter().any(|e| matches!(e, Effect::Start { t, layer: Some(l), .. } if t == c && l == "B-200.l1")), "{:?}", out.effects);
    }
    // cancelled: its frames go, and the layer is no longer active
    r.cancel(200, "owner", "enough").reply.unwrap();
    assert!(r.m.active_layers().is_empty());
}

#[test]
fn a_layer_that_cannot_be_made_raises_no_layer_at_its_scope() {
    let mut r = layer_run();
    r.open_form("lay", json!({"task": "median", "base": "abc123"}));
    r.take("f200-1");
    let out = r.go(Input::LayerFailed { layer: "Y-200.l1".into(), why: "no root branch is set".into() });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(matches!(&r.m.tickets[&200].status, Status::Failed(x) if x.exc == "NoLayer" && x.value["why"] == "no root branch is set"), "{:?}", r.m.tickets[&200].status);
    assert!(out.effects.contains(&Effect::End { agent: "f200-1".into() }), "the step's agent ends: {:?}", out.effects);
    assert!(r.m.active_layers().is_empty());
    // an unknown layer is refused, and nothing is logged
    let n = r.log.len();
    assert!(r.go(Input::LayerFailed { layer: "Y-200.l1".into(), why: "x".into() }).reply.is_err());
    assert_eq!(r.log.len(), n);
    // caught around the layer; the branches inside are cancelled
    let (d, errors) = defs_with(&[("c.kind", "kind c\n  letter C\n  form tasks: [Text]\n  result Text\n  raises Exhausted, Stalled\n  pattern\n    w <- try {\n      u <- layer on \"dev\" {\n        par x in tasks up to 2 { r <- step fresh \"Do {{ x }}\" -> Did; return \"d\" }\n      };\n      return \"in\"\n    } handle { NoLayer(why) => return why };\n    return w\n")]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r2 = Run { m: Machine::load(d.clone()), log: Vec::new(), clock: 0 };
    r2.open_form("c", json!({"tasks": ["a", "b"]}));
    let kids = r2.m.tickets[&200].children.clone();
    r2.go(Input::LayerFailed { layer: "C-200.l1".into(), why: "the kernel refused the mount".into() }).reply.unwrap();
    // what an arm binds is the raise's value, as for NoWorker
    assert!(matches!(&r2.m.tickets[&200].status, Status::Done(v) if v["why"] == "the kernel refused the mount" && v["layer"] == "C-200.l1"), "{:?}", r2.m.tickets[&200].status);
    assert!(kids.iter().all(|c| !r2.m.tickets[c].is_open()));
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r2.log), Vec::<String>::new());
    let (d, _) = defs_with(&LAYER_KINDS);
    let mut m3 = Machine::load(d);
    assert_eq!(m3.replay(&r.log), Vec::<String>::new());
}

#[test]
fn the_checker_wants_a_ref_for_a_layers_base() {
    let (_, errors) = defs_with(&[("n.kind", "kind n\n  form task: Int\n  result Text\n  pattern\n    u <- layer on task { return \"x\" };\n    return \"y\"\n")]);
    assert!(errors.len() == 1 && errors[0].contains("a layer is on a commit or a branch"), "{errors:?}");
}

/// A layer's value is its body's (paper-research's ruling after N-92): an `iterate` stands in one, and the kind's result
/// comes out of it, as Tovrin's `piece` has it.
#[test]
fn a_layers_value_is_its_bodys() {
    let k = "kind piece\n  letter P\n  form task: Text\n  result Ref\n  raises Exhausted, Stalled\n  pattern\n    layer on \"line\" {\n      let agent a in {\n        iterate e = none up to 2 {\n          r <- step a \"Do {{ task }}\" -> Commit(c: Ref) | Again(why: Text);\n          match r {\n            Commit(c) => return Done(c)\n            Again(why) => return Again(why)\n          }\n        }\n      }\n    }\n";
    let (d, errors) = defs_with(&[("p.kind", k)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d.clone()), log: Vec::new(), clock: 0 };
    r.open_form("piece", json!({"task": "median"}));
    assert_eq!(r.m.active_layers().len(), 1);
    let dl = r.take("f200-1");
    r.answer(dl.t, "f200-1", json!({"head": "Commit", "c": "abc123"}));
    assert!(matches!(&r.m.tickets[&200].status, Status::Done(v) if v == "abc123"), "{:?}", r.m.tickets[&200].status);
    assert!(r.m.active_layers().is_empty());
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    // and a layer's value must fit where it stands
    let (_, errors) = defs_with(&[("q.kind", "kind q\n  form task: Text\n  result Int\n  pattern\n    layer { return task }\n")]);
    assert!(errors.len() == 1, "{errors:?}");
}

const EVENT_KINDS: [(&str, &str); 1] = [(
    "build.kind",
    "event outcome { ticket: Text, state: Text, log: Text }\n\nkind build\n  letter B\n  form cut: Text\n  result Text\n  raises Stalled\n  pattern\n    v <- step runner \"Cases on {{ cut }}\" -> Green | Red(log: Text);\n    match v { Green => return \"green\", Red(log) => return log }\n  when outcome(o) where o.state == \"red\" do post o.log\n\nkind stray\n  letter S\n  form log: Text\n  result Text\n  pattern\n    return log\n\non outcome(o) where o.state == \"loose\" open stray(o.log)\n",
)];

fn event_run() -> Run {
    let (d, errors) = defs_with(&EVENT_KINDS);
    assert!(errors.is_empty(), "{errors:?}");
    Run { m: Machine::load(d), log: Vec::new(), clock: 0 }
}

#[test]
fn a_project_event_about_a_ticket_runs_its_kinds_handler_and_else_goes_to_the_bindings() {
    let mut r = event_run();
    r.go(Input::Open { kind: "build".into(), under: None, opener: "owner".into(), form: json!({"cut": "abc"}), text: None, names: vec![], told: vec![] }).reply.unwrap();
    r.take("runner");
    // a station's red: the handler posts its log on the build, by the runner, at once; the step is still owed
    let out = r.go(Input::Event { name: "outcome".into(), value: json!({"ticket": "B-200", "state": "red", "log": "case 3 fails"}) });
    assert_eq!(out.reply, Ok("Handled on B-200.".into()));
    assert!(matches!(&out.lines[0], Line::Event { on: Some(200), fired, .. } if fired.is_empty()), "{:?}", out.lines);
    let posts = &r.m.tickets[&200].posts;
    let last = posts.last().unwrap();
    assert_eq!((last.by.as_str(), last.text.clone()), ("runner", Text::New("case 3 fails".into())));
    assert!(matches!(r.m.tickets[&200].status, Status::Awaits { .. }), "the step is the runner's to answer");
    // a green one: taken by the build, its `where` does not hold, nothing posted, no binding asked
    let n = r.m.tickets[&200].posts.len();
    let out = r.go(Input::Event { name: "outcome".into(), value: json!({"ticket": "B-200", "state": "loose", "log": "x"}) });
    assert!(out.reply.as_ref().unwrap().starts_with("Taken by B-200"), "{:?}", out.reply);
    assert_eq!(r.m.tickets[&200].posts.len(), n);
    // about no ticket with a step open: the bindings'
    let out = r.go(Input::Event { name: "outcome".into(), value: json!({"ticket": "B-999", "state": "loose", "log": "stray log"}) });
    assert!(out.reply.as_ref().unwrap().starts_with("Opened S-"), "{:?}", out.reply);
    // the build closed: an event about it goes to the bindings too
    r.answer(200, "runner", json!({"head": "Green"}));
    let out = r.go(Input::Event { name: "outcome".into(), value: json!({"ticket": "B-200", "state": "red", "log": "late"}) });
    // (N-101 post 27, G: no binding fires on it, so it is only recorded, on the build)
    assert_eq!(out.reply, Ok("Recorded.".into()));
    assert!(matches!(&out.lines[..], [Line::Event { on: Some(200), fired, .. }] if fired.is_empty()), "{:?}", out.lines);
    // a value that does not fit the declaration is refused
    let out = r.go(Input::Event { name: "outcome".into(), value: json!({"ticket": "B-200"}) });
    assert!(out.reply.as_ref().is_err_and(|e| e.contains("does not fit its declaration")), "{:?}", out.reply);
    let (d, _) = defs_with(&EVENT_KINDS);
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert_eq!(m2.view(200), r.m.view(200));
}

#[test]
fn the_checker_refuses_a_step_or_an_open_in_a_project_events_handler_and_an_undeclared_event() {
    let bad = [
        ("a.kind", "event outcome { ticket: Text }\n\nkind a\n  form cut: Text\n  result Text\n  raises Stalled\n  pattern\n    return cut\n  when outcome(o) do (r <- step dev \"Look at {{ cut }}\" -> Seen; post \"seen\")\n"),
        ("b.kind", "kind b\n  form cut: Text\n  result Text\n  pattern\n    return cut\n  when outcome(o) do (_ <- open b(o.ticket); post \"x\")\n"),
        ("c.kind", "kind c\n  form cut: Text\n  result Text\n  pattern\n    return cut\n  when finished(o) do post \"x\"\n"),
    ];
    let (_, errors) = defs_with(&bad);
    let of = |k: &str| errors.iter().find(|e| e.contains(&format!("kind {k} refused"))).unwrap_or_else(|| panic!("{k}: {errors:#?}"));
    assert!(of("a").contains("makes no step"), "{}", of("a"));
    assert!(of("b").contains("opens no ticket"), "{}", of("b"));
    assert!(of("c").contains("there is no event finished"), "{}", of("c"));
}

const WHEN_KINDS: [(&str, &str); 1] = [(
    "cut.kind",
    "kind bit\n  letter Z\n  form task: Text\n  result Text\n  pattern\n    return task\n\nkind batch\n  letter Y\n  form cut: Text\n  result Text\n  raises Stalled\n  pattern\n    v <- step runner \"Cases on {{ cut }}\" -> Green;\n    return cut\n\nwhen cut-build is build open batch(\"line\")\n",
)];

#[test]
fn a_binding_fires_when_the_policys_decision_answers_its_option() {
    let (d, errors) = defs_with(&WHEN_KINDS);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(d.bindings.iter().any(|b| matches!(&b.every, Every::When { decision, option } if decision == "cut-build" && option == "build")));
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    for task in ["a", "b"] {
        r.go(Input::Open { kind: "bit".into(), under: None, opener: "dev".into(), form: json!({"task": task}), text: None, names: vec!["lead".into()], told: vec![] }).reply.unwrap();
    }
    // what the policy is shown: two bits closed, no batch open, never fired
    let asks = r.m.when_asks("2026-11-10T12:05:00Z");
    assert_eq!(asks.len(), 1);
    let (text, decision, option, f) = &asks[0];
    assert_eq!(option, "build");
    assert_eq!((text.as_str(), decision.as_str()), ("when cut-build is build open batch(\"line\")", "cut-build"));
    assert_eq!(f.get("open").map(String::as_str), Some("0"));
    assert_eq!(f.get("closed-bit").map(String::as_str), Some("2"));
    assert_eq!(f.get("oldest-bit").map(String::as_str), Some("4"));
    assert!(!f.contains_key("since"));
    // another answer opens nothing; its own opens the batch
    let out = r.go(Input::Event { name: "when".into(), value: json!({"binding": text, "answer": "wait"}) });
    assert!(out.reply.as_ref().is_err_and(|e| e.contains("the policy answered \"wait\"")), "{:?}", out.reply);
    // the clocks list it with its decision and no period (0.2 line 26); another answer is no firing
    let c = r.m.clock_views();
    assert_eq!(c.len(), 1, "{c:?}");
    assert_eq!((c[0].binding.as_str(), c[0].kind.as_str(), c[0].every, c[0].when.clone(), c[0].last.clone()), (text.as_str(), "batch", None, Some(("cut-build".to_string(), "build".to_string())), None));
    let out = r.go(Input::Event { name: "when".into(), value: json!({"binding": text, "answer": "build", "decision": {"chosen": "build"}}) });
    assert_eq!(out.reply, Ok("Opened Y-202.".into()));
    assert_eq!(r.m.tickets[&202].form, json!({"cut": "line"}));
    // the batch names the binding that opened it, and the clock when it fired
    assert_eq!(r.m.view(202).unwrap().binding.as_deref(), Some(text.as_str()));
    assert!(r.m.clock_views()[0].last.is_some());
    let f = &r.m.when_asks("2026-11-10T12:10:00Z")[0].3;
    assert_eq!(f.get("open").map(String::as_str), Some("1"));
    assert!(f.contains_key("since") && !f.contains_key("closed-bit"), "{f:?}");
    let (d, _) = defs_with(&WHEN_KINDS);
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert_eq!(m2.when_asks("2026-11-10T12:10:00Z"), r.m.when_asks("2026-11-10T12:10:00Z"));
    assert_eq!((m2.clock_views(), m2.view(202).unwrap().binding), (r.m.clock_views(), r.m.view(202).unwrap().binding));
}

#[test]
fn a_step_in_a_loop_keeps_each_round_and_its_answer_type() {
    let kinds = [(
        "loop.kind",
        "kind loop\n  letter L\n  form task: Text\n  result Ref\n  raises Exhausted, Stalled\n  pattern\n    iterate errs = \"none\" up to 3 {\n      r <- step dev \"Do {{ task }}; last errors {{ errs }}\" -> Commit(c: Ref);\n      v <- step runner \"Check the last commit\" -> Green | Red(log: Text);\n      match v { Green => return Done(r.c), Red(log) => return Again(log) }\n    }\n",
    )];
    let (d, errors) = defs_with(&kinds);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    r.go(Input::Open { kind: "loop".into(), under: None, opener: "owner".into(), form: json!({"task": "t"}), text: None, names: vec![], told: vec![] }).reply.unwrap();
    for (c, verdict) in [("aaa", json!({"head": "Red", "log": "case 3"})), ("bbb", json!({"head": "Green"}))] {
        r.take("dev");
        r.answer(200, "dev", json!({"head": "Commit", "c": c}));
        r.take("runner");
        r.answer(200, "runner", verdict);
    }
    fn find<'a>(ns: &'a [RunNode], text: &str) -> Option<&'a RunNode> {
        ns.iter().find_map(|n| if n.text.contains(text) && n.what == "step" { Some(n) } else { find(&n.nodes, text) })
    }
    let v = r.m.view(200).unwrap();
    let dev = find(&v.run, "step dev").expect("the dev step");
    assert_eq!(dev.rounds.iter().map(|x| (x.step, x.to.clone(), x.head.clone())).collect::<Vec<_>>(), vec![(1, Some("dev".to_string()), Some(json!({"head": "Commit", "c": "aaa"}))), (3, Some("dev".to_string()), Some(json!({"head": "Commit", "c": "bbb"})))]);
    assert!(dev.rounds.iter().all(|x| x.place.is_some() && x.given.is_some() && x.answered.is_some() && x.post.is_some()));
    assert_eq!(dev.ty, Some(Type::Variant(vec![("Commit".into(), vec![("c".into(), Type::Ref)])])));
    let runner = find(&v.run, "step runner").unwrap();
    assert_eq!(runner.rounds.iter().map(|x| x.step).collect::<Vec<_>>(), vec![2, 4]);
    let (d, _) = defs_with(&kinds);
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert_eq!(m2.view(200), r.m.view(200));
}

/// What halts on a slow thread (the owner N-94): a card only for what starts an agent's turn or changes the program.
fn slow_thread() -> (Run, u64) {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    let out = r.go(Input::Slow { t, on: true, by: "owner".into() });
    assert!(out.reply.is_ok(), "{out:?}");
    (r, t)
}

impl Run {
    /// Every halt of T approved, oldest first.
    fn approve_all(&mut self, t: u64) {
        while let Some(n) = self.m.tickets[&t].halts.first().map(|h| h.n) {
            self.approve(t, n);
        }
    }
}

fn halts_of(r: &Run, t: u64) -> Vec<String> {
    r.m.tickets[&t].halts.iter().map(|h| h.what.name().to_string()).collect()
}

/// 1. The owner's words: no card when a rule says whom they go to; a deliver card when the watcher's `route` chose,
/// carrying that it did; none when his own `clarify` did.
#[test]
fn slow_the_owners_words_halt_only_where_the_watcher_chose_whom_they_wake() {
    let (mut r, t) = slow_thread();
    // typed to nobody on a thread of two: the watcher is asked; its step neither gives nor takes a card
    let out = r.owner_says(t, &[], "who takes the mean?");
    assert_eq!(Run::halted_lines(&out), vec![]);
    let out = r.route(json!({"head": "To", "agents": ["fixer"]}));
    assert_eq!(Run::halted_lines(&out), vec![(t, "deliver".to_string())], "{:?}", out.lines);
    let Some(ViewHalt::Deliver { wakes, chose, .. }) = r.halts(t).into_iter().find(|h| matches!(h, ViewHalt::Deliver { .. })) else { panic!() };
    assert_eq!((wakes, chose.as_deref()), (vec!["fixer".to_string()], Some("watcher")));
    assert!(!r.woken("fixer", t), "halted: not delivered yet");
    let n = r.m.tickets[&t].halts[0].n;
    r.approve(t, n);
    assert!(r.woken("fixer", t));
    // named: a rule tells, no card
    let out = r.owner_says(t, &["lead"], "and you, @lead");
    assert_eq!(Run::halted_lines(&out), vec![]);
    assert!(r.woken("lead", t));
    // the watcher cannot tell, he says whom it is for himself: no card
    r.owner_says(t, &[], "the other one");
    r.route(json!({"head": "Unclear", "why": "which one?"}));
    let h = r.handler().unwrap();
    let out = r.go(Input::Answer { t: h, by: "owner".into(), head: json!({"head": "Send", "to": ["fixer"]}), text: Text::New(String::new()) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert_eq!(Run::halted_lines(&out), vec![]);
    assert!(halts_of(&r, t).is_empty(), "{:?}", halts_of(&r, t));
}

/// 2. An agent's post that wakes no agent is shown at once and makes no card; one that wakes an agent halts.
#[test]
fn slow_an_agents_post_that_wakes_nobody_makes_no_card() {
    let mut r = Run::slow_run();
    let w = r.slow_to_dev();
    let n0 = r.m.tickets[&w].halts.len();
    // dev alone on the work: its post names and wakes nobody
    let out = r.go(Input::Post { t: w, by: "dev".into(), text: Text::New("noted".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert_eq!(Run::halted_lines(&out), vec![], "{:#?}", out.lines);
    let p = r.m.tickets[&w].posts.last().unwrap();
    assert!(p.visible() && p.by == "dev" && p.wakes().next().is_none(), "shown: {p:?}");
    assert_eq!(r.m.tickets[&w].halts.len(), n0);
    // naming an agent: it wakes one, and halts as built
    let out = r.go(Input::Post { t: w, by: "dev".into(), text: Text::New("@ann look".into()), names: vec!["ann".into()], told: vec![], inform: vec![], answers: None, kind: None });
    assert_eq!(Run::halted_lines(&out), vec![(w, "deliver".to_string())]);
    assert!(r.m.due("ann").is_empty());
}

/// A slow thread with the lead alone on it, its first words handed and its turn ended quiet: at its hole, asked.
fn slow_at_hole() -> (Run, u64) {
    let mut r = Run::new();
    let t = r.say("lead", &[], "the median is wrong");
    r.intake(t, json!({"head": "Thread"}));
    r.go(Input::Slow { t, on: true, by: "owner".into() }).reply.unwrap();
    let d = r.m.due("lead");
    r.go(Input::Given { t, to: "lead".into(), delivery: d[0].id.clone(), upto: d[0].upto, skipped: Default::default(), place: Some(Run::loc(12)) });
    // its post wakes nobody (it is alone on the thread): no card
    let out = r.go(Input::Post { t, by: "lead".into(), text: Text::New("looking into it".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None });
    assert!(out.reply.is_ok() && Run::halted_lines(&out).is_empty(), "{out:?}");
    r.go(Input::TurnEnded { agent: "lead".into(), delivery: Some(d[0].id.clone()), text: Loc { file: "transcript.jsonl".into(), offset: 1300, len: 0 }, later: vec![], stood: None, go: None });
    // the watcher finds nothing the post asks: its `asks` step neither gives nor takes a card
    let before = halts_of(&r, t);
    r.nothing_asked();
    assert_eq!(halts_of(&r, t), before);
    assert!(r.asking_minder(t), "{:?}", r.m.tickets[&t].status);
    assert!(halts_of(&r, t).is_empty(), "{:?}", halts_of(&r, t));
    (r, t)
}

/// 3. coordd's own minder: its decision, give and take make no card, and Rest halts nothing.
#[test]
fn slow_coordds_own_minder_resting_halts_nothing() {
    let (mut r, t) = slow_at_hole();
    // `minder` runs decide, take and answer: each would fail on a halt
    let out = r.minder(t, json!({"head": "Rest"}));
    assert_eq!(Run::halted_lines(&out), vec![]);
    assert!(halts_of(&r, t).is_empty(), "{:?}", halts_of(&r, t));
    assert_eq!(r.status(t), ViewStatus::Resting);
}

/// 3. Its Program halts at the take: the program it would put at the hole is what he approves.
#[test]
fn slow_coordds_own_minder_program_halts_at_its_take() {
    let (mut r, t) = slow_at_hole();
    let out = r.minder(t, json!({"head": "Program", "text": "return 3"}));
    assert_eq!(Run::halted_lines(&out), vec![(t, "take".to_string())]);
    let Some(ViewHalt::Take { head, .. }) = r.halts(t).into_iter().next() else { panic!("{:?}", r.halts(t)) };
    assert_eq!(head["head"], "Program");
    r.approve_all(t);
    assert_eq!(r.status(t), ViewStatus::Resting, "the program ran: `return 3` at the hole");
    assert!(!r.m.view(t).unwrap().shaped.is_empty());
}

/// 3. Its Close halts at the take too.
#[test]
fn slow_coordds_own_minder_close_halts_at_its_take() {
    let (mut r, t) = slow_at_hole();
    let out = r.minder(t, json!({"head": "Close", "result": "Kept"}));
    assert_eq!(Run::halted_lines(&out), vec![(t, "take".to_string())]);
    assert!(r.m.tickets[&t].is_open(), "not closed before he approves");
    r.approve_all(t);
    assert!(!r.m.tickets[&t].is_open());
}

/// 4. An agent filling the minder role: its step is an agent's, and halts like any other.
#[test]
fn slow_an_agent_in_the_minder_role_halts_like_any_agent() {
    let (mut r, t) = slow_at_hole();
    r.nothing_asked();
    r.quiet();
    let Status::Asking { owed, .. } = &r.m.tickets[&t].status else { panic!("{:?}", r.m.tickets[&t].status) };
    let step = owed.step;
    let out = r.go(Input::Decided { t, step, question: "fills".into(), answer: json!("orch"), provenance: json!({}) });
    assert_eq!(Run::halted_lines(&out), vec![(t, "decide".to_string())]);
    let n = r.m.tickets[&t].halts[0].n;
    let out = r.approve(t, n);
    assert_eq!(Run::halted_lines(&out), vec![(t, "give".to_string())], "{:#?}", out.lines);
    let n = r.m.tickets[&t].halts[0].n;
    r.approve(t, n);
    r.take("orch");
    let out = r.go(Input::Answer { t, by: "orch".into(), head: json!({"head": "Rest"}), text: Text::New("rest".into()) });
    assert_eq!(Run::halted_lines(&out), vec![(t, "take".to_string())], "even its Rest: an agent's answer");
}

/// N-101, the minder as a program writes it: a `during` over the implementer's step, woken by its own check (`act`
/// with `is: "check"`); a look that rests, a look that speaks (`stop`, then the word), one look at a time (an act
/// during a look is kept for the next), and the block's end cancelling a look that still runs.
const DURING_KINDS: [(&str, &str); 1] = [(
    "piece.kind",
    "kind piece\n  letter P\n  form task: Text\n  result Text\n  pattern\n    let agent a in {\n      let agent m in {\n        r <- during {\n          step a \"Do {{ task }}\" -> Done(c: Text)\n        } when act(u) where u.by == a and u.is == \"check\" do {\n          k <- step m \"Look at {{ task }}: {{ u.text }}\" -> Rest | Say(text: Text);\n          match k {\n            Rest => return none\n            Say(text) => stop a; post text naming [a]\n          }\n        };\n        match r { Done(c) => return c }\n      }\n    }\n",
)];

fn act(r: &mut Run, by: &str, is: &str, text: &str) -> Out {
    r.go(Input::Event { name: "act".into(), value: json!({"ticket": "P-200", "by": by, "is": is, "text": text}) })
}

#[test]
fn a_during_looks_at_what_the_agent_does_one_look_at_a_time_and_can_stop_its_turn() {
    let (d, errors) = defs_with(&DURING_KINDS);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d.clone()), log: Vec::new(), clock: 0 };
    r.open_form("piece", json!({"task": "median"}));
    r.take("f200-1");
    // an edit: the `where` does not hold
    let out = act(&mut r, "f200-1", "edit", "src/lib.rs");
    assert!(out.reply.as_ref().unwrap().contains("does not hold"), "{:?}", out.reply);
    assert!(r.m.due("f200-2").is_empty());
    // its own check: a look, the minder's step, in a hidden ticket under the piece
    let out = act(&mut r, "f200-1", "check", "case 3 red");
    assert_eq!(out.reply, Ok("Looked at on P-200.".into()));
    let look = r.take("f200-2");
    assert!(look.t >= HIDDEN && r.m.tickets[&look.t].under == Some(200));
    // another check while it looks: kept for the next look
    let out = act(&mut r, "f200-1", "check", "case 4 red");
    assert!(out.reply.as_ref().unwrap().starts_with("Kept by P-200"), "{:?}", out.reply);
    // the look speaks: the implementer's turn is stopped, then the word, naming it; its step stays owed
    let out = r.go(Input::Answer { t: look.t, by: "f200-2".into(), head: json!({"head": "Say", "text": "the median of an empty list"}), text: Text::New("as said".into()) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(out.effects.contains(&Effect::Interrupt { agent: "f200-1".into() }), "{:?}", out.effects);
    assert!(matches!(&r.m.tickets[&200].status, Status::Awaits { agent, .. } if agent == "f200-1"), "{:?}", r.m.tickets[&200].status);
    // the kept check: the next look, at once
    let next = r.take("f200-2");
    assert_ne!(next.t, look.t);
    r.answer(next.t, "f200-2", json!({"head": "Rest"}));
    assert!(!r.m.tickets[&next.t].is_open());
    // a third look, still running when the implementer answers: cancelled with the block; the piece is done
    act(&mut r, "f200-1", "check", "all green").reply.unwrap();
    let third = r.take("f200-2");
    r.answer(200, "f200-1", json!({"head": "Done", "c": "abc123"}));
    assert!(matches!(&r.m.tickets[&third.t].status, Status::Failed(x) if x.exc == "Cancelled"), "{:?}", r.m.tickets[&third.t].status);
    assert!(matches!(&r.m.tickets[&200].status, Status::Done(v) if v == "abc123"), "{:?}", r.m.tickets[&200].status);
    // after the block, an act about the piece is nobody's: only recorded (Line::Act, N-101 post 27 B)
    assert_eq!(act(&mut r, "f200-1", "check", "late").reply, Ok("Recorded.".into()));
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert_eq!(m2.view(200), r.m.view(200));
}

/// `or` in a `where` (paper-research N-101 p392): `and` binds tighter, parentheses group; the minder looks at checks
/// and asks, not edits; printed back as written, so the recorded program reads the same.
#[test]
fn a_where_takes_or_with_and_binding_tighter() {
    let piece = DURING_KINDS[0].1.replace("where u.by == a and u.is == \"check\"", "where u.by == a and (u.is == \"check\" or u.is == \"ask\")");
    assert!(piece.contains(" or "), "{piece}");
    let (d, errors) = defs_with(&[("piece.kind", piece.as_str())]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d.clone()), log: Vec::new(), clock: 0 };
    r.open_form("piece", json!({"task": "median"}));
    r.take("f200-1");
    assert!(act(&mut r, "f200-1", "edit", "src/lib.rs").reply.as_ref().unwrap().contains("does not hold"));
    let out = act(&mut r, "f200-2", "check", "not the implementer");
    assert!(!out.reply.as_ref().unwrap().starts_with("Looked at"), "{:?}", out.reply);
    assert_eq!(act(&mut r, "f200-1", "ask", "drop or report?").reply, Ok("Looked at on P-200.".into()));
    let text = r.log.iter().find_map(|l| match l { Line::Open { program, .. } => Some(program.clone()), _ => None }).unwrap();
    assert!(text.contains("where u.by == a and (u.is == \"check\" or u.is == \"ask\") do"), "{text}");
    // precedence: `a or b and c` is `a or (b and c)`; `(a or b) and c` keeps its parentheses
    let named = BTreeMap::new();
    let k = |w: &str| format!("kind w\n  form task: Text\n  result Text\n  pattern\n    let agent a in {{\n      r <- during {{ step a \"Do\" -> Done }} when act(u) where {w} do {{ post u.text }};\n      return task\n    }}\n");
    for (w, printed) in [
        ("u.is == \"x\" or u.is == \"y\" and u.by == a", "u.is == \"x\" or u.is == \"y\" and u.by == a"),
        ("(u.is == \"x\" or u.is == \"y\") and u.by == a", "(u.is == \"x\" or u.is == \"y\") and u.by == a"),
    ] {
        let kind = syntax::parse_kind("w", &k(w), 1, &named).unwrap();
        let shown = print::prog(&kind.pattern, 0);
        assert!(shown.contains(&format!("where {printed} do")), "{shown}");
    }
    let or_and = syntax::parse_kind("w", &k("u.is == \"x\" or u.is == \"y\" and u.by == a"), 1, &named).unwrap();
    assert!(format!("{:?}", or_and.pattern).contains("Or(Cmp"), "{:?}", or_and.pattern);
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
}

#[test]
fn the_checker_types_a_durings_handler_in_the_blocks_scope() {
    let bad = [
        // X is the event's value: a record, not an agent
        ("a.kind", "kind a\n  form task: Text\n  result Text\n  pattern\n    let agent w in {\n      r <- during { step w \"Do {{ task }}\" -> Done } when act(u) where u.by == w do { stop u };\n      return task\n    }\n"),
        // a project's event's handler makes no step
        ("b.kind", "event seen { ticket: Text }\n\nkind b\n  form task: Text\n  result Text\n  pattern\n    let agent w in {\n      r <- during { step w \"Do {{ task }}\" -> Done } when seen(s) do { k <- step w \"Again\" -> Done; return none };\n      return task\n    }\n"),
        // no such event
        ("c.kind", "kind c\n  form task: Text\n  result Text\n  pattern\n    r <- during { return task } when finished(f) do { return none };\n    return task\n"),
        // `and` joins Bools
        ("e.kind", "kind e\n  form task: Text\n  result Text\n  pattern\n    let agent w in {\n      r <- during { step w \"Do {{ task }}\" -> Done } when act(u) where u.by == w and u.text do { stop w };\n      return task\n    }\n"),
    ];
    let (_, errors) = defs_with(&bad);
    let of = |k: &str| errors.iter().find(|e| e.contains(&format!("kind {k} refused"))).unwrap_or_else(|| panic!("{k}: {errors:#?}"));
    assert!(of("a").contains("where Agent is wanted"), "{}", of("a"));
    assert!(of("b").contains("makes no step"), "{}", of("b"));
    assert!(of("c").contains("there is no event finished"), "{}", of("c"));
    assert!(of("e").contains("`and` joins Bools"), "{}", of("e"));
    // and it prints as it is written, and parses back
    let (d, errors) = defs_with(&DURING_KINDS);
    assert!(errors.is_empty(), "{errors:?}");
    let p = &d.kinds["piece"].pattern;
    let text = crate::print::prog(p, 0);
    assert!(text.contains("} when act(u) where u.by == a and u.is == \"check\" do {") && text.contains("stop a"), "{text}");
    assert_eq!(crate::print::prog(&syntax::parse_prog(&text, 1).unwrap(), 0), text);
}

/// N-101, widening 1: an agent is its conversation. The piece's agent, ended with the piece, is named in its result;
/// a later step to it starts it again (Effect::Start, which coordd turns into `roster restart` on its kept session) in
/// that step's layer, and it ends when that step is answered.
const AGAIN_KINDS: [(&str, &str); 1] = [(
    "again.kind",
    "kind piece\n  letter P\n  form task: Text\n  result Agent\n  pattern\n    let agent a in {\n      r <- step a \"Do {{ task }}\" -> Done;\n      return a\n    }\n\nkind redo\n  letter R\n  form task: Text\n  result Text\n  pattern\n    p <- open piece(task);\n    layer on \"cut\" {\n      f <- step p \"Fix {{ task }} on the cut\" -> Fixed(c: Text);\n      match f { Fixed(c) => return c }\n    }\n",
)];

#[test]
fn a_step_to_an_agent_whose_block_ended_starts_it_again_in_the_steps_layer() {
    let (d, errors) = defs_with(&AGAIN_KINDS);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d.clone()), log: Vec::new(), clock: 0 };
    r.open_form("redo", json!({"task": "median"}));
    assert!(!r.m.ended("f201-1"));
    let dl = r.take("f201-1");
    let out = r.go(Input::Answer { t: dl.t, by: "f201-1".into(), head: json!({"head": "Done"}), text: Text::New("as said".into()) });
    assert!(out.effects.contains(&Effect::End { agent: "f201-1".into() }), "{:?}", out.effects);
    // the piece closed with its agent; the step to it starts it again, in the layer on the cut
    assert!(out.effects.contains(&Effect::Start { agent: "f201-1".into(), t: 200, call: syntax::INLINE.into(), layer: Some("R-200.l1".into()) }), "{:?}", out.effects);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Agent { agent, what, t: 200, .. } if agent == "f201-1" && what == "restarted")), "{:?}", out.lines);
    assert!(!r.m.ended("f201-1"));
    let dl = r.take("f201-1");
    assert_eq!(dl.t, 200);
    let out = r.go(Input::Answer { t: 200, by: "f201-1".into(), head: json!({"head": "Fixed", "c": "abc123"}), text: Text::New("as said".into()) });
    assert!(out.effects.contains(&Effect::End { agent: "f201-1".into() }), "{:?}", out.effects);
    assert!(matches!(&r.m.tickets[&200].status, Status::Done(v) if v == "abc123"), "{:?}", r.m.tickets[&200].status);
    assert!(r.m.ended("f201-1"));
    // a name the program never made is not restarted: a standing agent's step as before
    assert!(!r.m.ended("lead") && !r.m.ended("f999-1") && !r.m.ended("f200-7"));
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
}

/// N-101, widening 3: `decide`. The policy is asked first; a rule's answer names a constructor (its fields from the
/// decision's arguments of the same names); with none the owner answers, as a step owed to him; a decision nobody
/// declared is refused at load.
#[test]
fn decide_asks_the_policy_and_with_no_rule_the_owner() {
    let mut named = BTreeMap::new();
    let extra = ("tovrin.call", "decision fix-or-out(piece: Text, why: Text) -> Fix | Out\ndecision whose-red(cut: Text, log: Text) -> Piece(cut: Text) | Unclear\n");
    let calls: BTreeMap<String, CallSig> = CALLS.iter().chain([&extra]).flat_map(|(f, t)| syntax::parse_calls(f, t, &mut named).unwrap()).map(|c| (c.name.clone(), c)).collect();
    assert!(calls.contains_key("decide fix-or-out"));
    let kinds = "kind mend\n  letter M\n  form task: Text\n  result Text\n  pattern\n    d <- decide fix-or-out(task, \"case 3 red\");\n    match d {\n      Fix => return \"fix\"\n      Out => return \"out\"\n    }\n\nkind culprit\n  letter C\n  form cut: Text\n  result Text\n  pattern\n    w <- decide whose-red(cut, \"log\");\n    match w {\n      Piece(c) => return c\n      Unclear => return \"unclear\"\n    }\n\nkind stray\n  form task: Text\n  result Text\n  pattern\n    d <- decide nobody-declared(task);\n    return task\n";
    let mut files: Vec<(String, String)> = KINDS.iter().map(|(f, t)| (f.to_string(), t.to_string())).collect();
    files.push(("t.kind".into(), kinds.into()));
    let (d, errors) = Defs::build(&files, calls, named, 200);
    assert!(errors.len() == 1 && errors[0].contains("there is no decision nobody-declared"), "{errors:?} {:?}", d.kinds.get("stray").map(|k| crate::print::prog(&k.pattern, 0)));
    let mut r = Run { m: Machine::load(d.clone()), log: Vec::new(), clock: 0 };
    // a rule answers Out
    let out = r.open_form("mend", json!({"task": "P-201"}));
    assert!(out.effects.contains(&Effect::Ask { t: 200, step: 1, question: Question::Decide { name: "fix-or-out".into(), args: json!({"piece": "P-201", "why": "case 3 red"}) } }), "{:?} {:?} {:?}", out.effects, out.reply, r.m.tickets.get(&200).map(|x| &x.status));
    r.go(Input::Decided { t: 200, step: 1, question: "decide".into(), answer: json!("Out"), provenance: json!({"decision": "fix-or-out"}) }).reply.unwrap();
    assert!(matches!(&r.m.tickets[&200].status, Status::Done(v) if v == "out"), "{:?}", r.m.tickets[&200].status);
    // no rule decides: the owner's, a step owed to him with the decision's arguments; he answers Fix
    r.open_form("mend", json!({"task": "P-202"}));
    r.go(Input::Decided { t: 201, step: 1, question: "decide".into(), answer: Value::Null, provenance: json!({"fault": "no decision fix-or-out"}) }).reply.unwrap();
    assert!(matches!(&r.m.tickets[&201].status, Status::Awaits { agent, owed, .. } if agent == "owner" && owed.call == "decide fix-or-out" && owed.args["piece"] == "P-202"), "{:?}", r.m.tickets[&201].status);
    r.answer(201, "owner", json!({"head": "Fix"}));
    assert!(matches!(&r.m.tickets[&201].status, Status::Done(v) if v == "fix"), "{:?}", r.m.tickets[&201].status);
    // a constructor with fields: from the arguments of those names
    r.open_form("culprit", json!({"cut": "abc123"}));
    r.go(Input::Decided { t: 202, step: 1, question: "decide".into(), answer: json!("Piece"), provenance: Value::Null }).reply.unwrap();
    assert!(matches!(&r.m.tickets[&202].status, Status::Done(v) if v == "abc123"), "{:?}", r.m.tickets[&202].status);
    // an answer that names none of the answers: the owner's
    r.open_form("mend", json!({"task": "P-203"}));
    r.go(Input::Decided { t: 203, step: 1, question: "decide".into(), answer: json!("Maybe"), provenance: Value::Null }).reply.unwrap();
    assert!(matches!(&r.m.tickets[&203].status, Status::Awaits { agent, .. } if agent == "owner"), "{:?}", r.m.tickets[&203].status);
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
}

/// N-101, widening 2 and the record: a declared record built and matched by its name (`Handed(c, top, a)`), and
/// `result of` a ticket: its result, typed by `as k` or by the constructor the pattern names; NoResult when it is
/// open or failed (untracked, as NoLayer).
#[test]
fn result_of_a_ticket_and_a_record_built_and_matched_by_name() {
    let mut named = BTreeMap::new();
    let extra = ("t.call", "type Handed {\n  commit: Ref\n  top: Ref\n  by: Agent\n}\n");
    let calls: BTreeMap<String, CallSig> = CALLS.iter().chain([&extra]).flat_map(|(f, t)| syntax::parse_calls(f, t, &mut named).unwrap()).map(|c| (c.name.clone(), c)).collect();
    let kinds = "kind piece\n  letter P\n  form task: Text\n  result Handed\n  pattern\n    let agent a in {\n      r <- step a \"Do {{ task }}\" -> Commit(c: Ref);\n      match r { Commit(c) => return Handed(c, \"top1\", a) }\n    }\n\nkind mend\n  letter M\n  form piece: Ticket\n  result Agent\n  pattern\n    match result of piece { Handed(_, top, by) => return by }\n\nkind typed\n  letter T\n  form piece: Ticket\n  result Ref\n  pattern\n    h <- return result of piece as piece;\n    return h.commit\n\nkind early\n  letter E\n  form task: Text\n  result Text\n  pattern\n    p <- open piece(task);\n    return \"x\"\n";
    let mut files: Vec<(String, String)> = KINDS.iter().map(|(f, t)| (f.to_string(), t.to_string())).collect();
    files.push(("t.kind".into(), kinds.into()));
    let (d, errors) = Defs::build(&files, calls.clone(), named.clone(), 200);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d.clone()), log: Vec::new(), clock: 0 };
    r.open_form("piece", json!({"task": "median"}));
    r.take("f200-1");
    r.answer(200, "f200-1", json!({"head": "Commit", "c": "abc123"}));
    assert!(matches!(&r.m.tickets[&200].status, Status::Done(v) if v["commit"] == "abc123" && v["top"] == "top1" && v["by"] == "f200-1"), "{:?}", r.m.tickets[&200].status);
    let out = named_open(&mut r, "mend", json!({"piece": "P-200"}));
    assert!(out.reply.is_ok(), "{:?} {:?}", out.reply, r.m.tickets.keys().collect::<Vec<_>>());
    assert!(matches!(&r.m.tickets[&201].status, Status::Done(v) if v == "f200-1"), "{:?}", r.m.tickets[&201].status);
    named_open(&mut r, "typed", json!({"piece": "P-200"}));
    assert!(matches!(&r.m.tickets[&202].status, Status::Done(v) if v == "abc123"), "{:?}", r.m.tickets[&202].status);
    // a record as an agent gives it (no head) matches by its name too
    let mut env = Default::default();
    use crate::syntax::Pat;
    let pat = Pat::Con { name: "Handed".into(), args: vec![Pat::Wild, Pat::Wild, Pat::Var("by".into())], fields: vec!["commit".into(), "top".into(), "by".into()] };
    assert!(crate::machine::tests_matches(&pat, &json!({"commit": "c", "top": "t", "by": "dev"}), &mut env) && env.get("by") == Some(&json!("dev")));
    // open: NoResult
    r.open_form("piece", json!({"task": "mode"}));
    named_open(&mut r, "mend", json!({"piece": "P-203"}));
    assert!(matches!(&r.m.tickets[&204].status, Status::Failed(x) if x.exc == "NoResult" && x.value["why"] == "P-203 is open"), "{:?}", r.m.tickets[&204].status);
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    // checked: an unknown constructor on an untyped result (one no type of agent-base declares: `Nothing` is Found's
    // since `look`, now `seek`), and a wrong record name
    let bad = "kind b1\n  form piece: Ticket\n  result Text\n  pattern\n    match result of piece { Zilch(x) => return x }\n\nkind b2\n  form task: Text\n  result Handed\n  pattern\n    return Given(\"c\", \"t\", \"a\")\n";
    let mut files: Vec<(String, String)> = KINDS.iter().map(|(f, t)| (f.to_string(), t.to_string())).collect();
    files.push(("b.kind".into(), bad.into()));
    let (_, errors) = Defs::build(&files, calls, named, 200);
    assert!(errors.iter().any(|e| e.contains("kind b1 refused") && e.contains("no declared type has Zilch")), "{errors:?}");
    assert!(errors.iter().any(|e| e.contains("kind b2 refused") && e.contains("Given is not a constructor")), "{errors:?}");
}

fn named_open(r: &mut Run, kind: &str, form: Value) -> Out {
    r.go(Input::Open { kind: kind.into(), under: None, opener: "owner".into(), form, text: None, names: vec!["lead".into()], told: vec![] })
}

// Part 4 of the message runtime (N-818 post 32, 4a): where each post of the owner's is, for each agent it went to.

impl Run {
    /// Where the owner's post N on T is for AGENT (`TicketView::whereabouts`), if it went to it.
    fn where_is(&self, t: u64, n: u32, agent: &str) -> Option<Where> {
        self.m.view(t).unwrap().whereabouts.into_iter().find(|w| w.n == n)?.agents.into_iter().find(|w| w.agent == agent)
    }
    /// The owner's post on T to AGENT, told it as his handler tells; its number.
    fn owner_to(&mut self, t: u64, agent: &str, text: &str) -> u32 {
        self.owner_says(t, &[agent], text);
        if !self.woken(agent, t) {
            self.route(json!({"head": "To", "agents": [agent]}));
        }
        self.m.tickets[&t].last()
    }
    /// The time of post N on T.
    fn post_at(&self, t: u64, n: u32) -> At {
        self.m.tickets[&t].posts.iter().find(|p| p.n == n).unwrap().at.clone()
    }
}

/// Waiting, with how many of his posts to the same agent are ahead of it on any ticket; handed; read; answered by a
/// reply naming him (the rule), by `answers`, and from another ticket (`AnswersAlso`), the first answer staying the
/// answer. An agent handed his post only as an inform (a later post informing it) has no row for it. Each is read off
/// the lines, so a replay shows the same.
#[test]
fn each_post_of_the_owners_shows_where_it_is_for_each_agent() {
    let mut r = Run::new();
    let a = r.note("owner", "research", "how long?");
    let b = r.note("owner", "research", "and the release?");
    let n = r.owner_to(b, "research", "and the docs?");
    // nothing handed yet: each waits, behind his earlier ones to research on any ticket
    let w = r.where_is(a, 1, "research").unwrap();
    assert_eq!(w, Where { agent: "research".into(), waiting: Some(Waiting { ahead: 0 }), handed: None, delivered: None, read: None, step: None, answered: None });
    assert_eq!(r.where_is(b, 1, "research").unwrap().waiting, Some(Waiting { ahead: 1 }));
    assert_eq!(r.where_is(b, n, "research").unwrap().waiting, Some(Waiting { ahead: 2 }));
    assert!(r.where_is(a, 1, "owner").is_none(), "not his own");
    // a is handed: no longer waiting, and nothing of b's has it ahead
    let da = r.give("research", a);
    let handed = r.m.tickets[&a].given.last().unwrap().at.clone();
    let w = r.where_is(a, 1, "research").unwrap();
    assert_eq!((w.waiting, w.handed.as_ref(), w.read), (None, Some(&handed), None));
    assert_eq!(r.where_is(b, 1, "research").unwrap().waiting, Some(Waiting { ahead: 0 }));
    assert_eq!(r.where_is(b, n, "research").unwrap().waiting, Some(Waiting { ahead: 1 }));
    // its turn ends with a reply naming him: read then, and answered by the rule
    let out = r.turn("research", Some(&da), 1);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Replied { .. })), "{:?}", out.lines);
    r.nothing_asked();
    let ended = r.m.tickets[&a].given.last().unwrap().read.clone().expect("read at the turn's end");
    let reply = r.m.tickets[&a].posts.iter().rev().find(|p| p.by == "research").unwrap().clone();
    let w = r.where_is(a, 1, "research").unwrap();
    assert_eq!((w.handed, w.read.as_ref()), (Some(handed), Some(&ended)));
    assert_eq!(w.answered, Some(WhereAnswer { post: format!("{}.p{}", r.m.id(a), reply.n), at: reply.at.clone() }));
    // b's two in one delivery: the first answered by `answers` there, the second from a
    let db = r.give("research", b);
    r.go(Input::Post { t: b, by: "research".into(), text: Text::New("Friday".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: Some(1), kind: None }).reply.unwrap();
    let on_b = r.m.tickets[&b].last();
    r.go(Input::Post { t: a, by: "research".into(), text: Text::New("the docs with it".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
    let on_a = r.m.tickets[&a].last();
    r.go(Input::AnswersAlso { t: a, n: on_a, also: vec![format!("{}.p{n}", r.m.id(b))] }).reply.unwrap();
    r.turn("research", Some(&db), 2);
    r.nothing_asked();
    let w1 = r.where_is(b, 1, "research").unwrap();
    assert_eq!(w1.answered, Some(WhereAnswer { post: format!("{}.p{on_b}", r.m.id(b)), at: r.post_at(b, on_b) }));
    let w2 = r.where_is(b, n, "research").unwrap();
    assert_eq!(w2.answered, Some(WhereAnswer { post: format!("{}.p{on_a}", r.m.id(a)), at: r.post_at(a, on_a) }));
    assert!(w1.read.is_some() && w1.read == w2.read && w1.handed.is_some() && w1.handed == w2.handed, "{w1:?} {w2:?}");
    // a's later post answers nothing more there: the first answer stays
    assert_eq!(r.where_is(a, 1, "research").unwrap().answered.unwrap().post, format!("{}.p{}", r.m.id(a), reply.n));
    // his post told to lead, handed to research only by lead's post after it informing research: a given row of
    // research's reaches it, yet his post is nowhere for research, which never had it to answer (coordd's footer for a
    // post going on from it reads this: consult-3 N-1446 p187)
    let m = r.owner_to(a, "lead", "and the stills?");
    r.go(Input::Post { t: a, by: "lead".into(), text: Text::New("on them".into()), names: vec![], told: vec![], inform: vec!["research".into()], answers: None, kind: None }).reply.unwrap();
    assert!(r.informed("research", a) && !r.woken("research", a), "{:?}", r.m.due("research"));
    r.give("research", a);
    assert!(r.m.tickets[&a].given.iter().any(|g| g.to == "research" && g.upto >= m), "{:?}", r.m.tickets[&a].given);
    assert!(r.where_is(a, m, "research").is_none());
    assert!(r.where_is(a, m, "lead").is_some_and(|w| w.handed.is_none()));
    r.replays();
}

/// The owner writing in a ticket's box (0.3; proto-lead N-1446 p26): a record line of the machine's own each way,
/// `writing {t, on}`, an `event` that an engine from before reads as one nothing takes; the machine keeps since when
/// (`writing_since`), which E1b's hold reads. A second `on` keeps the first one's time and writes nothing, nor does an
/// `off` with none; a ticket that is not there, or a value neither on nor off, is refused. Replayed, the same.
#[test]
fn his_writing_in_a_tickets_box_is_a_record_line_with_since_when() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    let id = r.m.id(t);
    let w = |on: bool| Input::Event { name: syntax::WRITING.into(), value: json!({"t": t, "on": on}) };
    assert_eq!(r.m.writing_since(t), None);
    let out = r.go_at(w(true), "2026-11-20T21:00:00Z");
    assert_eq!(out.reply, Ok(format!("The owner is writing on {id}.")));
    assert!(matches!(&out.lines[..], [Line::Event { name, fired, counted, on: None, .. }] if name == syntax::WRITING && fired.is_empty() && counted.is_empty()), "{:?}", out.lines);
    assert_eq!(r.m.writing_since(t).map(String::as_str), Some("2026-11-20T21:00:00Z"));
    // his box still on a moment later: nothing written, and the time is the first one's
    let out = r.go_at(w(true), "2026-11-20T21:00:05Z");
    assert!(out.lines.is_empty() && out.reply == Ok(format!("The owner is writing on {id} already, since 2026-11-20T21:00:00Z.")), "{out:?}");
    // the record as a start reads it: the same
    let mut m2 = Machine::load(defs());
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert_eq!(m2.writing, r.m.writing);
    // refused: a ticket that is not there, a value neither on nor off
    for value in [json!({"t": 9999, "on": true}), json!({"t": t})] {
        let out = r.go_at(Input::Event { name: syntax::WRITING.into(), value }, "2026-11-20T21:00:06Z");
        assert!(out.lines.is_empty() && out.reply == Err("Writing is switched on or off in a ticket's box.".into()), "{out:?}");
    }
    let out = r.go_at(w(false), "2026-11-20T21:00:09Z");
    assert!(out.reply == Ok(format!("The owner stopped writing on {id}.")) && out.lines.len() == 1, "{out:?}");
    assert_eq!(r.m.writing_since(t), None);
    let out = r.go_at(w(false), "2026-11-20T21:00:10Z");
    assert!(out.lines.is_empty() && out.reply == Ok(format!("The owner is not writing on {id}.")), "{out:?}");
    // its lines as the record keeps them, read back and replayed
    let text: Vec<String> = r.log.iter().map(|l| serde_json::to_string(l).unwrap()).collect();
    let back: Vec<Line> = text.iter().map(|s| serde_json::from_str(s).unwrap()).collect();
    assert_eq!(back, r.log);
    let mut m3 = Machine::load(defs());
    assert_eq!(m3.replay(&back), Vec::<String>::new());
    assert!(m3.writing.is_empty());
    r.replays();
}

// Line 327 (N-1324): the owner's hold on agents' mail, the fact the delivery program reads (N-1324 posts 27, 28).

impl Run {
    /// The owner's hold on agents' mail switched ON or off, as coordd writes it; its reply.
    fn hold_mail(&mut self, on: bool) -> String {
        self.go(Input::Event { name: MAIL_HOLD.into(), value: json!({"by": "owner", "on": on}) }).reply.unwrap()
    }
    /// As `go`, at AT: lines in the same second.
    fn go_at(&mut self, i: Input, at: &str) -> Out {
        let out = self.m.apply(i, at.into());
        self.log.extend(out.lines.iter().cloned());
        out
    }
}

/// The hold is a record line of the machine's own (`mail_hold`). While it is on, every post but his is kept, and his
/// pass whenever written (`mail_held_post`; paper-research N-1396 p235, a); a delivery is kept unless it carries one of
/// his (`mail_held`), so a step is kept as an agent's post is, on a ticket opened before the hold or since. A second
/// Hold and a Release with none change nothing; a value neither on nor off is refused; off, nothing is kept. Replayed,
/// on and off alike.
#[test]
fn the_owners_hold_keeps_every_post_but_his() {
    let mut r = Run::new();
    let c = r.open_claim();
    r.decide(c, 1, "research");
    let t = r.two_on_a_thread();
    let his = r.owner_to(t, "fixer", "is the median fixed?");
    r.post_on(t, "lead", "it was the mean", &["fixer"]).reply.unwrap();
    let lead = r.m.tickets[&t].last();
    assert_eq!(r.m.mail_held_post(t, his), None, "no hold");
    assert_eq!(r.hold_mail(true), "Agents' mail is held: until Release, only the owner's posts pass.");
    let at = r.m.mail_hold.as_ref().unwrap().at.clone();
    // from before the hold: his passes, the lead's is kept, and so is the step
    assert_eq!((r.m.mail_held_post(t, his), r.m.mail_held_post(t, lead)), (None, Some(&at)));
    let d = r.m.due("fixer").into_iter().find(|d| d.t == t).unwrap();
    assert_eq!(r.m.mail_held(&d), None, "it carries his post from before the hold: {d:?}");
    let s = r.m.due("research").into_iter().find(|d| d.t == c).unwrap();
    assert!(matches!(s.what, What::Step { .. }), "{s:?}");
    assert_eq!(r.m.mail_held(&s), Some(&at));
    // written since: an agent's post is kept; his passes, and the delivery that carries it
    r.post_on(t, "lead", "and the docs", &["fixer"]).reply.unwrap();
    assert_eq!(r.m.mail_held_post(t, r.m.tickets[&t].last()), Some(&at));
    // his post through intake, routed to fixer (`owner_to` routes only when nothing woke fixer there, and the posts
    // before it still do)
    let (after, _) = r.owner_posts(t, &["fixer"], "fixer, the median first");
    r.route_of(t, after, json!({"head": "To", "agents": ["fixer"]}));
    assert_eq!(r.m.mail_held_post(t, after), None);
    let d = r.m.due("fixer").into_iter().find(|d| d.t == t).unwrap();
    assert!(matches!(&d.what, What::Posts { unread } if unread.last().is_some_and(|p| p.n == after)), "{d:?}");
    assert_eq!(r.m.mail_held(&d), None, "it carries his posts");
    // a ticket opened since: his pass, the rest are kept
    let u = r.note("lead", "fixer", "a new thread");
    let v = r.note("owner", "fixer", "and one of his");
    assert_eq!((r.m.mail_held_post(u, r.m.tickets[&u].last()), r.m.mail_held_post(v, r.m.tickets[&v].last())), (Some(&at), None));
    // a second Hold changes nothing; a value neither on nor off is refused
    assert_eq!(r.hold_mail(true), format!("Agents' mail is held already, since {at}."));
    assert_eq!(r.m.mail_hold.as_ref().map(|h| &h.at), Some(&at));
    let refused = r.go(Input::Event { name: MAIL_HOLD.into(), value: json!({"by": "owner"}) });
    assert_eq!(refused.reply, Err("A hold on agents' mail is switched on or off.".into()));
    r.replays();
    assert_eq!(r.hold_mail(false), format!("Agents' mail is let go (held since {at})."));
    assert_eq!((r.m.mail_hold.as_ref(), r.m.mail_held_post(t, lead), r.m.mail_held(&s)), (None, None, None));
    assert_eq!(r.hold_mail(false), "Agents' mail is not held.");
    r.replays();
}

/// What the hold keeps is read from who wrote it, not from when (paper-research N-1396 p235, a): his posts in the same
/// second as the hold, one before its line and one after it, both pass; the lead's after it is kept. The record's lines
/// replayed, as at a start, give the hold before any round, and the same answers (consult-3's case 2, N-1324 post 16).
#[test]
fn the_hold_passes_his_posts_whenever_written_and_comes_back_by_replay() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    let same = "2026-11-20T07:20:00Z";
    let post = |by: &str, text: &str| Input::Post { t, by: by.into(), text: Text::New(text.into()), names: vec!["fixer".into()], told: vec![], inform: vec![], answers: None, kind: None };
    r.go_at(post("owner", "before it"), same).reply.unwrap();
    let a = r.m.tickets[&t].last();
    r.go_at(Input::Event { name: MAIL_HOLD.into(), value: json!({"by": "owner", "on": true}) }, same).reply.unwrap();
    r.go_at(post("owner", "after it"), same).reply.unwrap();
    let b = r.m.tickets[&t].last();
    r.go_at(post("lead", "and mine"), same).reply.unwrap();
    let lead = r.m.tickets[&t].last();
    assert!([b, lead].iter().all(|n| r.post_at(t, *n) == r.post_at(t, a)), "all in the hold's second");
    let held = |m: &Machine| [a, b, lead].map(|n| m.mail_held_post(t, n).is_some());
    assert_eq!(held(&r.m), [false, false, true]);
    // a start: the record's lines read back and replayed, before any round
    let lines: Vec<Line> = r.log.iter().map(|l| serde_json::from_str(&serde_json::to_string(l).unwrap()).unwrap()).collect();
    let mut m2 = Machine::load(defs());
    assert_eq!(m2.replay(&lines), Vec::<String>::new());
    assert_eq!(m2.mail_hold, r.m.mail_hold);
    assert_eq!(held(&m2), [false, false, true]);
}

/// The release his window runs is a record line of the machine's own (`window_release`, I-1457): the last one read
/// is kept, with the earlier releases it contains and coordd's own release beside it (p610); a line naming none, or an
/// engine naming none, is refused, and a replay keeps the last. A build is in his window while either half is it or
/// contains it: after a `back` to an older release, a later build is not; an engine put in alone is.
#[test]
fn the_window_release_line_keeps_the_last_release_and_what_it_contains() {
    let mut r = Run::new();
    assert_eq!(r.m.window_release, None);
    for v in [json!({"release": ""}), json!({"release": "20261120-220601-5815e775", "engine": {"contains": []}})] {
        let refused = r.go(Input::Event { name: WINDOW_RELEASE.into(), value: v }).reply;
        assert_eq!(refused, Err("A window's release line names the release, and its engine's when it has one.".into()));
    }
    let (older, newer, engine) = ("20261120-220601-5815e775", "20261120-232357-a633aa96", "20261121-003000-b1e55ed0");
    let said = r.go(Input::Event { name: WINDOW_RELEASE.into(), value: json!({"release": newer, "contains": [older, "20261119-101752-e0eb4d1"]}) });
    assert_eq!(said.reply, Ok(format!("His window runs {newer}.")));
    let w = r.m.window_release.clone().unwrap();
    assert_eq!((w.has(newer), w.has(older), w.has("20261121-010000-0000000"), w.engine), (true, true, false, None));
    let replayed = |r: &Run| {
        let lines: Vec<Line> = r.log.iter().map(|l| serde_json::from_str(&serde_json::to_string(l).unwrap()).unwrap()).collect();
        let mut m2 = Machine::load(defs());
        assert_eq!(m2.replay(&lines), Vec::<String>::new());
        m2.window_release
    };
    assert_eq!(replayed(&r), r.m.window_release);
    // a `back`: the older one runs, and its own list decides
    r.go(Input::Event { name: WINDOW_RELEASE.into(), value: json!({"release": older, "contains": ["20261119-101752-e0eb4d1"]}) }).reply.unwrap();
    let w = r.m.window_release.clone().unwrap();
    assert_eq!((w.window.release.as_str(), w.has(newer), w.has(older)), (older, false, true));
    assert_eq!(replayed(&r), r.m.window_release);
    // an engine put in alone (`live --no-exe`): what it is and contains is in his window, the window's half unchanged;
    // the line is written back as it was read, each list sorted
    let v = json!({"release": older, "contains": ["20261119-101752-e0eb4d1"], "engine": {"release": engine, "contains": [older, newer]}});
    let said = r.go(Input::Event { name: WINDOW_RELEASE.into(), value: v.clone() });
    assert_eq!(said.reply, Ok(format!("His window runs {older}; coordd runs {engine}.")));
    let w = r.m.window_release.clone().unwrap();
    assert_eq!((w.has(engine), w.has(newer), w.has("20261121-010000-0000000"), w.value()), (true, true, false, v));
    assert_eq!(replayed(&r), r.m.window_release);
}

/// A step the agent owes on a ticket opened under the thread after his post: on which ticket, its number, its call,
/// since when. His post after that ticket opened does not show it.
#[test]
fn a_post_of_his_shows_the_step_its_agent_owes_on_what_it_opened_since() {
    let mut r = Run::new();
    let t = r.note("owner", "lead", "the agda load fails, look");
    let d = r.give("lead", t);
    r.turn("lead", Some(&d), 1);
    r.nothing_asked();
    assert_eq!(r.where_is(t, 1, "lead").unwrap().step, None);
    r.go(Input::Open {
        kind: "claim".into(),
        under: Some(t),
        opener: "lead".into(),
        form: json!({"what": "load fails", "evidence": "the agda MCP load errors", "where": "path:agda/x.agda:201", "branch": "main"}),
        text: None,
        names: vec![],
        told: vec![],
    })
    .reply
    .unwrap();
    let c = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    assert_eq!(r.m.tickets[&c].under, Some(t));
    r.decide(c, 1, "lead");
    let Status::Awaits { since, owed, .. } = &r.m.tickets[&c].status else { panic!("{:?}", r.m.tickets[&c].status) };
    let want = WhereStep { on: r.m.id(c), step: owed.step, call: "assess".into(), since: since.clone() };
    assert_eq!(r.where_is(t, 1, "lead").unwrap().step, Some(want));
    let n = r.owner_to(t, "lead", "and the other file?");
    assert_eq!(r.where_is(t, n, "lead").unwrap().step, None, "the claim was opened before it");
    r.replays();
}

/// An inline step on the thread itself is shown by the first line of its words, for every post of his there.
#[test]
fn an_inline_step_shows_by_its_words() {
    let mut r = Run::new();
    let t = r.say("fixer", &["lead"], "add median(xs)");
    r.settle(10);
    // as a_minders_program_carries_a_call_inline: a program refused first, then his post, then the one that holds
    r.minder(t, json!({"head": "Program", "text": "v <- step to \"Review {{ commit }}\" -> Fine | Fix(why: Text);\n..."}));
    r.go(Input::Post { t, by: "owner".into(), text: Text::New("review it".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
    let n = r.m.tickets[&t].last();
    r.route(json!({"head": "To", "agents": ["fixer"]}));
    r.settle(20);
    r.minder(t, json!({"head": "Program", "text": "v <- step to \"Review what {{ to }} was asked: \\\"{{ text }}\\\"\" -> Fine | Fix(why: Text);\nmatch v {\n  Fine => return Kept\n  Fix(why) => ...\n}"}));
    let w = r.where_is(t, n, "fixer").unwrap();
    let s = w.step.expect("the fixer owes the step");
    assert_eq!((s.on, s.call.as_str()), (r.m.id(t), "Review what fixer was asked: \"add median(xs)\""));
    assert_eq!(r.where_is(t, 1, "fixer").unwrap().step.map(|s| s.call), Some(s.call.clone()), "on the thread itself, for his first post too");
    r.replays();
}

/// What `ahead` counts is made once while no ticket changes, and made again after one does.
#[test]
fn the_unhanded_list_is_kept_until_a_ticket_changes() {
    let mut r = Run::new();
    let a = r.note("owner", "research", "how long?");
    let one = r.m.unhanded();
    assert!(std::sync::Arc::ptr_eq(&one, &r.m.unhanded()), "kept");
    assert_eq!(one.get("research").map(Vec::len), Some(1));
    r.give("research", a);
    let two = r.m.unhanded();
    assert!(!std::sync::Arc::ptr_eq(&one, &two));
    assert_eq!(two.get("research"), None);
}

/// A reply naming him answers only the posts its writer had been handed when it wrote it (`Ticket::answers_upto`, the
/// machine's own rule in `waits_between`; consult-3, N-412 post 1916): his second post, put into the running turn or
/// sent to wait until it ends, is not answered by that turn's reply to his first. The next turn's reply answers it, and
/// the first's answer stays. A record before the era [`ERA_PUT_IN`]: from it, one put in is read at that turn's end.
#[test]
fn a_reply_answers_only_the_posts_its_writer_had_been_handed() {
    for wait in [false, true] {
        let mut r = Run::new();
        let t = r.note("owner", "research", "how long?");
        let d1 = r.give("research", t);
        let n = r.owner_to(t, "research", "and the release?");
        if wait {
            r.go(Input::Waits { t, n: Some(n) }).reply.unwrap();
        } else {
            let d = r.m.due("research").into_iter().find(|d| d.t == t).unwrap();
            r.go(Input::GivenInto { t, to: "research".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).reply.unwrap();
        }
        let out = r.turn("research", Some(&d1), 1);
        assert!(out.lines.iter().any(|l| matches!(l, Line::Replied { t: u, .. } if *u == t)), "{wait}: {:?}", out.lines);
        r.nothing_asked();
        let first = r.m.tickets[&t].posts.iter().rev().find(|p| p.by == "research").unwrap().clone();
        assert_eq!(r.m.tickets[&t].answers_upto(&first), 1, "{wait}: the turn's reply was written on his first post alone");
        let id = r.m.id(t);
        let by_first = Some(WhereAnswer { post: format!("{id}.p{}", first.n), at: first.at.clone() });
        assert_eq!(r.where_is(t, 1, "research").unwrap().answered, by_first, "{wait}");
        let w = r.where_is(t, n, "research").unwrap();
        assert_eq!((w.read.as_ref(), w.answered.as_ref()), (None, None), "{wait}: neither read nor answered: {w:?}");
        if wait {
            assert_eq!(w.waiting, Some(Waiting { ahead: 0 }), "{w:?}");
        }
        // the next turn holds it, and its reply answers it
        let d2 = if wait {
            r.give("research", t)
        } else {
            let d = r.m.given_since("research");
            assert_eq!(d.len(), 1, "{d:?}");
            d[0].clone()
        };
        r.turn("research", Some(&d2), 2);
        r.nothing_asked();
        let second = r.m.tickets[&t].posts.iter().rev().find(|p| p.by == "research").unwrap().clone();
        assert!(second.n > first.n, "{wait}");
        let w = r.where_is(t, n, "research").unwrap();
        assert!(w.read.is_some(), "{wait}: {w:?}");
        assert_eq!(w.answered, Some(WhereAnswer { post: format!("{id}.p{}", second.n), at: second.at.clone() }), "{wait}");
        assert_eq!(r.where_is(t, 1, "research").unwrap().answered, by_first, "{wait}: the first's answer stays");
        r.replays();
    }
}

/// A post put into a running turn and left there unanswered (paper-research N-1397.p81, p91), by the record's era.
/// Before [`ERA_PUT_IN`] it stays given, the next turn's, as the record's replies were drawn (N-101 p1214); the era's
/// line reads an agent's, and makes his due again (N-412.p2197, 4), so nothing stays given and no `free` waits on
/// either. From the era on, his put in is read at its turn's end once that turn's last message is posted on its thread
/// after it (p2197, 2); one still queued at that end (`later`) is the next turn's, as before. The record replays so.
#[test]
fn a_post_put_into_a_turn_and_left_there_is_read_at_its_end_from_the_era_put_in() {
    let mut r = Run::new();
    let put_in = |r: &mut Run, t: u64| {
        let d = r.m.due("research").into_iter().find(|d| d.t == t).unwrap();
        r.go(Input::GivenInto { t, to: "research".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).reply.unwrap();
        d.id
    };
    // before the era: the next turn's
    let t = r.note("owner", "research", "how long?");
    let d1 = r.give("research", t);
    // his on a thread of its own, which the turn's reply on t does not answer
    let h = r.note("owner", "research", "and the release?");
    let d2 = put_in(&mut r, h);
    let s = r.note("lead", "research", "the stills?");
    let d6 = put_in(&mut r, s);
    r.turn("research", Some(&d1), 1);
    assert_eq!(r.m.given_since("research"), vec![d2.clone(), d6.clone()], "carried, as before the era");
    assert!(r.m.holds_put_in());
    // the era's line reads the lead's, and makes his due again
    let out = r.go(Input::Event { name: ERA.into(), value: json!({"era": ERA_PUT_IN}) });
    assert_eq!(r.m.era, ERA_PUT_IN);
    assert!(r.m.given_since("research").is_empty() && !r.m.holds_put_in());
    assert!(r.m.tickets[&s].given.iter().any(|g| g.delivery == d6 && g.read.is_some()), "the lead's read at the era's line");
    assert!(r.m.tickets[&h].given.iter().any(|g| g.delivery == d2 && g.read.is_none() && g.left), "his not read: {:?}", r.m.tickets[&h].given);
    assert!(out.effects.contains(&Effect::Deliver { to: "research".into() }), "{out:?}");
    assert!(r.m.due("research").iter().any(|d| d.t == h && d.id == d2) && r.m.handed_before("research", &d2), "his due again, said so");
    assert!(r.where_is(h, 1, "research").unwrap().waiting.is_some());
    // from the era: one that entered the turn is read at its end, unanswered; one still queued is the next turn's
    let u = r.note("owner", "research", "which median?");
    let d3 = r.give("research", u);
    let n4 = r.owner_to(u, "research", "and its spread?");
    put_in(&mut r, u);
    let v = r.note("owner", "research", "and the build?");
    let d5 = put_in(&mut r, v);
    let out = r.go(Input::TurnEnded { agent: "research".into(), delivery: Some(d3), text: Run::loc(2), later: vec![d5.clone()], stood: None, go: None });
    assert!(out.lines.iter().any(|l| matches!(l, Line::Replied { t, .. } if *t == u)), "the turn's end replies to what it began with: {:?}", out.lines);
    let w = r.where_is(u, n4, "research").unwrap();
    // read: the turn's last message is on its thread after it; the view's answer is the reply's own rule, by what it
    // was handed when written (`answers_upto`)
    assert!(w.read.is_some() && w.answered.is_none(), "read at the turn's end: {w:?}");
    assert_eq!(r.m.given_since("research"), vec![d5], "still queued at the end: the next turn's");
    r.replays();
}

/// His post put into a turn that ends without answering it (paper-research N-412.p2194; p2197, 1): not read, out of
/// what the agent was given, due again once and said so. Handed again, its own turn ending with Nothing, it is owed:
/// not handed a third time, not read, given no more, and its giving shown owed (`ViewGiven::owed`) through later turns
/// about other things. Answered with the reply tool, it is owed no more, and the next turn's end reads it.
#[test]
fn his_post_left_unanswered_in_two_turns_is_owed_until_answered() {
    let mut r = Run::new();
    r.go(Input::Event { name: ERA.into(), value: json!({"era": ERA_PUT_IN}) }).reply.unwrap();
    let a = r.note("lead", "research", "the stills?");
    let d1 = r.give("research", a);
    let h = r.note("owner", "research", "and the release notes?");
    let d = r.m.due("research").into_iter().find(|d| d.t == h).unwrap();
    r.go(Input::GivenInto { t: h, to: "research".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).reply.unwrap();
    let out = r.turn("research", Some(&d1), 1);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Replied { t, .. } if *t == a)), "its last message is on the lead's thread: {:?}", out.lines);
    assert!(out.effects.contains(&Effect::Deliver { to: "research".into() }), "{out:?}");
    assert!(r.m.given_since("research").is_empty());
    assert!(r.m.due("research").iter().any(|e| e.t == h && e.id == d.id) && r.m.handed_before("research", &d.id), "due again, said so");
    assert!(r.where_is(h, 1, "research").unwrap().read.is_none());
    // handed again; its own turn ends with Nothing: owed
    assert_eq!(r.give("research", h), d.id);
    let _ = r.go(Input::TurnEnded { agent: "research".into(), delivery: Some(d.id.clone()), text: Loc { len: 0, ..Run::loc(2) }, later: vec![], stood: None, go: None });
    let id = format!("{}.p1", r.m.id(h));
    assert!(r.m.given_since("research").is_empty() && !r.m.due("research").iter().any(|e| e.t == h), "not handed a third time");
    assert_eq!(r.m.owed_posts("research"), vec![id.clone()]);
    assert!(r.where_is(h, 1, "research").unwrap().read.is_none(), "not read");
    assert_eq!(r.m.view(h).unwrap().given.iter().map(|g| g.owed).collect::<Vec<_>>(), [false, true]);
    // a turn about something else: owed still
    r.go(Input::Post { t: a, by: "lead".into(), text: Text::New("and the spread?".into()), names: vec!["research".into()], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
    let d3 = r.give("research", a);
    r.turn("research", Some(&d3), 3);
    assert_eq!(r.m.owed_posts("research"), vec![id.clone()]);
    // answered with the reply tool: owed no more; read at the next turn's end
    r.go(Input::Post { t: h, by: "research".into(), text: Text::New("in N-9".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: Some(1), kind: None }).reply.unwrap();
    assert!(r.m.owed_posts("research").is_empty());
    r.go(Input::Post { t: a, by: "lead".into(), text: Text::New("thanks".into()), names: vec!["research".into()], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
    let d4 = r.give("research", a);
    r.turn("research", Some(&d4), 4);
    assert!(r.where_is(h, 1, "research").unwrap().read.is_some());
    assert!(r.m.view(h).unwrap().given.iter().all(|g| !g.owed));
    r.replays();
}

/// His post handed to three agents, two of which leave it unanswered in two turns each, so both owe it and the foot of
/// each one's deliveries names it (`owed_posts`), until the third, handed it too, answers it on its thread: then it is
/// owed by neither, no foot names it and no giving of it says `not answered`. A post there by one it was not handed
/// to answers nothing (paper-research N-1446 p2348: N-1104's p843 and p938, answered by the thread's voices, named at
/// the foot of every delivery of an agent that may not post on N-1104).
#[test]
fn his_post_owed_by_two_is_owed_by_neither_once_a_third_it_was_handed_answers_it_on_its_thread() {
    let mut r = Run::new();
    r.go(Input::Event { name: ERA.into(), value: json!({"era": ERA_PUT_IN}) }).reply.unwrap();
    let out = r.go(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("and the release notes?".into())), names: vec!["research".into(), "dev".into(), "voice".into()], told: vec![] });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let h = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    // each of the two: his post put into a turn about the lead's post, left; handed again alone, left again
    let owe = |r: &mut Run, x: &str, n: u64| {
        let a = r.note("lead", x, "the stills?");
        let d1 = r.give(x, a);
        let d = r.m.due(x).into_iter().find(|d| d.t == h).unwrap();
        r.go(Input::GivenInto { t: h, to: x.into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).reply.unwrap();
        r.turn(x, Some(&d1), n);
        assert_eq!(r.give(x, h), d.id, "{x}: handed again");
        let _ = r.go(Input::TurnEnded { agent: x.into(), delivery: Some(d.id), text: Loc { len: 0, ..Run::loc(n + 1) }, later: vec![], stood: None, go: None });
    };
    owe(&mut r, "research", 1);
    owe(&mut r, "dev", 3);
    let id = format!("{}.p1", r.m.id(h));
    assert_eq!((r.m.owed_posts("research"), r.m.owed_posts("dev")), (vec![id.clone()], vec![id.clone()]), "owed by both");
    assert_eq!(r.m.view(h).unwrap().given.iter().filter(|g| g.owed).count(), 2, "{:?}", r.m.view(h).unwrap().given);
    // the lead, not handed it, posts there: that answers nothing
    r.go(Input::Post { t: h, by: "lead".into(), text: Text::New("on it".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
    assert_eq!((r.m.owed_posts("research"), r.m.owed_posts("dev")), (vec![id.clone()], vec![id.clone()]), "the lead was not handed it");
    // the third, handed it, answers it there: owed by neither
    r.give("voice", h);
    r.go(Input::Post { t: h, by: "voice".into(), text: Text::New("the notes are in N-9".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: Some(1), kind: None }).reply.unwrap();
    assert!(r.m.owed_posts("research").is_empty() && r.m.owed_posts("dev").is_empty(), "no foot names it");
    assert_eq!((r.m.owed_answered("research"), r.m.owed_answered("dev")), (vec![id.clone()], vec![id]), "answered by the third");
    assert!(r.m.view(h).unwrap().given.iter().all(|g| !g.owed), "no giving says not answered: {:?}", r.m.view(h).unwrap().given);
    r.replays();
}

/// A turn cut short that held his post (paper-research N-412.p2202, 2): a lost message no more. From the era
/// [`ERA_PUT_IN`], the cut leaves it as a turn's end that did not answer it leaves it: made due again, said to be
/// handed again; cut again, owed, not handed a third time and not read. The lead's post given with it is forgotten as
/// before, and his post answered in the cut turn is read. Before the era, and for an agent of another project, the cut
/// forgets his post as it always did.
#[test]
fn his_post_in_a_cut_turn_comes_again_once_then_is_owed() {
    let mut r = Run::new();
    // before the era: forgotten, as recorded
    let old = r.note("owner", "research", "the logs?");
    r.give("research", old);
    r.go(Input::TurnCut { agent: "research".into() }).reply.unwrap();
    assert!(!r.m.due("research").iter().any(|d| d.t == old), "before the era a cut forgets it");
    r.go(Input::Event { name: ERA.into(), value: json!({"era": ERA_PUT_IN}) }).reply.unwrap();
    let a = r.note("lead", "research", "the stills?");
    r.give("research", a);
    let h = r.note("owner", "research", "and the release notes?");
    let d = r.give("research", h);
    let done = r.note("owner", "research", "which branch?");
    r.give("research", done);
    r.go(Input::Post { t: done, by: "research".into(), text: Text::New("main".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: Some(1), kind: None }).reply.unwrap();
    let out = r.go(Input::TurnCut { agent: "research".into() });
    assert!(out.reply.as_ref().is_ok_and(|s| s.contains("1 with the owner's post not answered")), "{:?}", out.reply);
    assert!(out.effects.contains(&Effect::Deliver { to: "research".into() }), "{out:?}");
    assert!(r.m.given_since("research").is_empty());
    assert!(r.m.due("research").iter().any(|e| e.t == h && e.id == d) && r.m.handed_before("research", &d), "his post due again, said so");
    assert!(!r.m.due("research").iter().any(|e| e.t == a), "the lead's forgotten, as before");
    assert!(r.where_is(done, 1, "research").unwrap().read.is_some(), "his post answered in the cut turn: read");
    assert!(r.where_is(h, 1, "research").unwrap().read.is_none());
    // handed again, and that turn cut too: owed
    assert_eq!(r.give("research", h), d);
    r.go(Input::TurnCut { agent: "research".into() }).reply.unwrap();
    assert!(!r.m.due("research").iter().any(|e| e.t == h), "not handed a third time");
    assert_eq!(r.m.owed_posts("research"), vec![format!("{}.p1", r.m.id(h))]);
    assert!(r.where_is(h, 1, "research").unwrap().read.is_none(), "not read");
    // an agent of another project: its giving is the relay's carrying, done at the cut
    let far = r.note("owner", "proj/dee", "the proxy?");
    r.give("proj/dee", far);
    assert!(!r.m.cut_holds_his("proj/dee"));
    r.go(Input::TurnCut { agent: "proj/dee".into() }).reply.unwrap();
    assert!(!r.m.due("proj/dee").iter().any(|e| e.t == far));
    r.replays();
}

/// "Handed again" goes by post number, as "owed" does (consult-3 N-412.p2203, 2; p2210, 2). His post h.p1, put into a
/// turn that ends without answering it, is due again. A second post of his on h before the re-hand gives the delivery
/// a new id: the hand-over still says it is the second time, and both are his posts the turn must answer
/// (`his_to_answer`). That second post is at its first leaving when the re-hand's turn ends with Nothing: the delivery
/// comes again, p1 with it, so each post of his comes again once. Left once more, both are owed.
#[test]
fn his_post_handed_again_under_a_new_id_says_so_and_a_post_of_his_riding_in_it_comes_again_once() {
    let mut r = Run::new();
    r.go(Input::Event { name: ERA.into(), value: json!({"era": ERA_PUT_IN}) }).reply.unwrap();
    let a = r.note("lead", "research", "the stills?");
    let d1 = r.give("research", a);
    let h = r.note("owner", "research", "and the release notes?");
    let d = r.m.due("research").into_iter().find(|d| d.t == h).unwrap();
    r.go(Input::GivenInto { t: h, to: "research".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).reply.unwrap();
    r.turn("research", Some(&d1), 1);
    assert!(r.m.handed_before("research", &d.id), "due again, said so");
    // his second post on h before the re-hand: a new id, and his first post in it still handed again
    let n2 = r.owner_to(h, "research", "and the date?");
    let (p1, p2) = (format!("{}.p1", r.m.id(h)), format!("{}.p{n2}", r.m.id(h)));
    let e = r.m.due("research").into_iter().find(|e| e.t == h).unwrap();
    assert_ne!(e.id, d.id);
    assert!(r.m.handed_before("research", &e.id), "said to be handed again under its new id");
    assert_eq!(r.m.his_to_answer("research", &e, false), vec![p1.clone(), p2.clone()]);
    let nothing = |r: &mut Run, d: &str, n: u64| {
        r.go(Input::TurnEnded { agent: "research".into(), delivery: Some(d.to_string()), text: Loc { len: 0, ..Run::loc(n) }, later: vec![], stood: None, go: None });
    };
    // handed, its turn ending with Nothing: p2 at its first leaving brings it again, p1 with it
    assert_eq!(r.give("research", h), e.id);
    nothing(&mut r, &e.id, 2);
    assert!(r.m.owed_posts("research").is_empty(), "p2 was never handed again: not owed");
    assert!(r.m.due("research").iter().any(|x| x.t == h && x.id == e.id) && r.m.handed_before("research", &e.id), "due again, said so");
    // left once more: both owed, and not handed again
    assert_eq!(r.give("research", h), e.id);
    nothing(&mut r, &e.id, 3);
    assert_eq!(r.m.owed_posts("research"), vec![p1, p2]);
    assert!(!r.m.due("research").iter().any(|x| x.t == h));
    r.replays();
}

/// What answers his post comes after it in the record, compared by post number, never by time (consult-3 N-412.p2198,
/// 2), each case in one second: his post put in, then the turn's last message on its thread: read. The turn's last
/// message on its thread, then his post: no answer to it; it waits. The agent's own post on his thread, then his put
/// in, the turn ending elsewhere: no answer; due again. His put in, then the agent's post there: read.
#[test]
fn what_answers_his_post_comes_after_it_in_the_record_in_one_second() {
    // each line in the second of the one before it
    let same = |r: &mut Run| r.clock -= 1;
    let his = |r: &mut Run, t: u64, text: &str| -> u32 {
        same(r);
        r.owner_says(t, &["research"], text);
        if !r.woken("research", t) {
            same(r);
            r.route(json!({"head": "To", "agents": ["research"]}));
        }
        r.m.tickets[&t].last()
    };
    let put_in = |r: &mut Run, t: u64| -> String {
        let d = r.m.due("research").into_iter().find(|d| d.t == t).unwrap();
        same(r);
        r.go(Input::GivenInto { t, to: "research".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).reply.unwrap();
        d.id
    };
    let own = |r: &mut Run, t: u64| {
        same(r);
        r.go(Input::Post { t, by: "research".into(), text: Text::New("on it".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
        r.m.tickets[&t].last()
    };
    let mut r = Run::new();
    r.go(Input::Event { name: ERA.into(), value: json!({"era": ERA_PUT_IN}) }).reply.unwrap();
    // his post put in, then the turn's last message on its thread
    let a = r.note("lead", "research", "the stills?");
    let d1 = r.give("research", a);
    let n = his(&mut r, a, "@research and the notes?");
    let d = put_in(&mut r, a);
    same(&mut r);
    let out = r.turn("research", Some(&d1), 1);
    let m = r.m.tickets[&a].last();
    assert!(out.lines.iter().any(|l| matches!(l, Line::Replied { t, n: k, .. } if *t == a && *k == m)) && m > n, "{:?}", out.lines);
    assert_eq!(r.post_at(a, n), r.post_at(a, m), "one second");
    assert!(r.m.tickets[&a].given.iter().any(|g| g.delivery == d && g.read.is_some() && !g.left), "read");
    assert!(!r.m.due("research").iter().any(|e| e.t == a));
    // the turn's last message on its thread, then his post
    let b = r.note("lead", "research", "the spread?");
    let d2 = r.give("research", b);
    r.turn("research", Some(&d2), 2);
    let m = r.m.tickets[&b].last();
    let n = his(&mut r, b, "@research and the median?");
    assert!(n > m && r.post_at(b, n) == r.post_at(b, m), "one second, his after");
    let w = r.where_is(b, n, "research").unwrap();
    assert!(w.answered.is_none() && w.waiting.is_some(), "the message before it answers it nothing: {w:?}");
    // the agent's own post on his thread, then his put in; the turn's last message on another thread
    let c = r.note("lead", "research", "the build?");
    let e = r.note("lead", "research", "the release?");
    let d3 = r.give("research", c);
    r.give("research", e);
    let k = own(&mut r, e);
    let n = his(&mut r, e, "@research and its date?");
    let d = put_in(&mut r, e);
    assert!(n > k && r.post_at(e, n) == r.post_at(e, k), "one second, his after");
    same(&mut r);
    r.turn("research", Some(&d3), 3);
    assert!(r.m.tickets[&e].given.iter().any(|g| g.delivery == d && g.read.is_none() && g.left), "not answered: {:?}", r.m.tickets[&e].given);
    assert!(r.m.due("research").iter().any(|x| x.t == e), "due again");
    // his put in, then the agent's post there; the turn's last message on another thread
    let f = r.note("lead", "research", "the stills, again?");
    let g = r.note("lead", "research", "the logs?");
    let d4 = r.give("research", f);
    r.give("research", g);
    let n = his(&mut r, g, "@research and the errors?");
    let d = put_in(&mut r, g);
    let k = own(&mut r, g);
    assert!(k > n && r.post_at(g, n) == r.post_at(g, k), "one second, its after");
    same(&mut r);
    r.turn("research", Some(&d4), 4);
    assert!(r.m.tickets[&g].given.iter().any(|x| x.delivery == d && x.read.is_some() && !x.left), "answered by the post after it: {:?}", r.m.tickets[&g].given);
    assert!(!r.m.due("research").iter().any(|x| x.t == g));
    r.replays();
}

mod faults;
mod archive;
mod iterate;
mod fork;
mod answers;
mod parts;
mod session;
mod ask_minded;
mod calls_recorded;
mod guards;
mod runner;
mod violated;
mod unformed;
mod changed_calls;
mod runner_refused;
mod model_classes;
mod live_edits;
mod assign;
mod docs;
mod labels;
mod moment;
mod outcome_fields;
mod agents_given;
mod names_given;
mod kind_docs;
mod stop;
mod lapse;
mod words;
mod delivery;
mod gather;
mod tonight;
mod ran;
mod insight;
mod said;
mod ledger;
mod graph_posts;
mod signed;
mod kept;
mod window_release;

/// The owner's once-edit of a call's template (`Input::Template`; paper-research N-101 post 145, 8): `step` goes with
/// the next step of the call on the ticket only, `tree` with every later one there and under it; a step owed and not
/// yet given takes it at once; the log replays to the same.
const EDIT_KINDS: [(&str, &str); 2] = [
    ("edits.kind", "kind edits\n  letter Z\n  form q: Text\n  result Text\n  raises Exhausted, Stalled\n  pattern\n    a <- step dev answer(q, none);\n    b <- step dev answer(q, none);\n    x <- open sub(q);\n    return x\n"),
    ("sub.kind", "kind sub\n  letter Y\n  form q: Text\n  result Text\n  raises Exhausted, Stalled\n  pattern\n    a <- step dev answer(q, none);\n    return q\n"),
];

#[test]
fn a_template_edited_once_goes_with_the_next_step_or_with_the_tree() {
    let (d, errors) = defs_with(&EDIT_KINDS);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    r.go(Input::Open { kind: "edits".into(), under: None, opener: "owner".into(), form: json!({"q": "why"}), text: None, names: vec![], told: vec![] }).reply.unwrap();
    let edit = |scope: &str, text: &str, by: &str, call: &str| Input::Template { t: 200, call: call.into(), scope: scope.into(), text: text.into(), by: by.into(), why: "clearer".into() };
    // refused: not the owner, no such call, no such scope, nothing written
    for bad in [edit("step", "A {{ ask }}", "dev", "answer"), edit("step", "A", "owner", "nosuch"), edit("once", "A", "owner", "answer"), edit("step", " ", "owner", "answer")] {
        let out = r.go(bad);
        assert!(out.reply.is_err() && out.lines.is_empty(), "{out:?}");
    }
    // step 1 is owed and not given: it takes the step edit now
    let out = r.go(edit("step", "A {{ ask }}", "owner", "answer"));
    assert_eq!(out.reply.as_deref(), Ok("Your prompt for answer goes with step 1 of Z-200."));
    assert!(matches!(&out.lines[..], [Line::Template { t: 200, scope, .. }] if scope == "step"));
    let What::Step { edit: e, .. } = r.take("dev").what else { panic!() };
    assert_eq!(e.as_deref(), Some("A {{ ask }}"));
    // the view shows the edit the owed step carries (N-208)
    let v = |r: &Run, t: u64| r.m.view(t).unwrap().templates.iter().map(|x| (x.call.clone(), x.on.clone(), x.tree, x.step, x.text.clone())).collect::<Vec<_>>();
    assert_eq!(v(&r, 200), vec![("answer".into(), "Z-200".into(), false, Some(1), "A {{ ask }}".into())]);
    // a step edit made while none is owed waits for the next one; used once
    r.go(Input::Answer { t: 200, by: "dev".into(), head: json!({"head": "Answer", "text": "x"}), text: Text::New("x".into()) }).reply.unwrap();
    let What::Step { step: 2, edit: e, .. } = r.take("dev").what else { panic!() };
    assert_eq!(e, None, "the step edit went with step 1 only");
    assert_eq!(v(&r, 200), vec![], "used: no longer shown");
    // a step edit while the step owed is given already: it waits for the next, and the view shows it waiting
    assert_eq!(r.go(edit("step", "S {{ ask }}", "owner", "answer")).reply.as_deref(), Ok("Your prompt for answer goes with the next step of answer on Z-200."));
    assert_eq!(v(&r, 200), vec![("answer".into(), "Z-200".into(), false, None, "S {{ ask }}".into())]);
    // tree: step 2 was given already and keeps its prompt; the child's step takes it
    let out = r.go(edit("tree", "B {{ ask }}", "owner", "answer"));
    assert_eq!(out.reply.as_deref(), Ok("Your prompt for answer goes with every later step of answer on Z-200 and the tickets under it."));
    r.go(Input::Answer { t: 200, by: "dev".into(), head: json!({"head": "Answer", "text": "y"}), text: Text::New("y".into()) }).reply.unwrap();
    let d = r.take("dev");
    assert_eq!(d.t, 201);
    let What::Step { edit: e, .. } = d.what else { panic!() };
    assert_eq!(e.as_deref(), Some("B {{ ask }}"), "the tree edit reaches the ticket under it");
    // the child shows the tree edit its owed step carries, then the tree edit it comes from; the parent its unused one too
    let w = v(&r, 201);
    assert!(matches!(&w[..], [(_, on, true, Some(_), t), (_, on2, true, None, t2)] if on == "Z-200" && on2 == "Z-200" && t == "B {{ ask }}" && t2 == "B {{ ask }}"), "{w:?}");
    assert_eq!(v(&r, 200), vec![("answer".into(), "Z-200".into(), false, None, "S {{ ask }}".into()), ("answer".into(), "Z-200".into(), true, None, "B {{ ask }}".into())]);
    // the log replays to the same: the child's owed step carries the edit
    let mut m = Machine::load(defs_with(&EDIT_KINDS).0);
    assert!(m.replay(&r.log).is_empty());
    let Status::Awaits { owed, .. } = &m.tickets[&201].status else { panic!() };
    assert_eq!(owed.edit.as_deref(), Some("B {{ ask }}"));
    assert!(m.templates.iter().filter(|e| e.text.starts_with('A')).all(|e| e.used), "the step edit is used in the replay too");
    assert_eq!(m.view(200).unwrap().templates, r.m.view(200).unwrap().templates, "and the views show the same");
    // closed tickets take none; a reload is a record
    r.go(Input::Answer { t: 201, by: "dev".into(), head: json!({"head": "Answer", "text": "z"}), text: Text::New("z".into()) }).reply.unwrap();
    assert!(!r.m.tickets[&200].is_open());
    assert!(r.go(edit("tree", "C", "owner", "answer")).reply.is_err());
    let out = r.go(Input::Reload { by: "owner".into(), why: "w".into(), commit: Some("abc".into()), kinds: vec!["sub".into()], calls: vec![] });
    assert_eq!(out.reply.as_deref(), Ok("The kinds and calls are read again: kinds sub changed or new."));
    assert!(matches!(&out.lines[..], [Line::Reload { .. }]));
}

/// WAITING-K (paper-research N-101 post 166, C-225's gap 2): a ticket closed done waits until a ticket of the binding's
/// kind that carried it (named it in what came back to a step) closes done; when that one fails, what it carried waits
/// again at once, with its age from when it closed, though nothing closed since the binding last fired.
const CARRY_KINDS: [(&str, &str); 1] = [(
    "carry.kind",
    "kind bit\n  letter Z\n  form task: Text\n  result Text\n  pattern\n    return task\n\nkind batch\n  letter Y\n  form cut: Text\n  result Text\n  raises Bad, Stalled\n  pattern\n    c <- step runner \"Cut\" -> Cut(carries: [Ticket]);\n    v <- step runner \"Cases\" -> Green | Red(log: Text);\n    match v {\n      Green => return cut\n      Red(log) => raise Bad(log)\n    }\n\nwhen cut-build is build open batch(\"line\")\n",
)];

#[test]
fn what_a_failed_build_carried_waits_again_at_once() {
    let (d, errors) = defs_with(&CARRY_KINDS);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    for task in ["a", "b"] {
        r.go(Input::Open { kind: "bit".into(), under: None, opener: "dev".into(), form: json!({"task": task}), text: None, names: vec!["lead".into()], told: vec![] }).reply.unwrap();
    }
    let f = |r: &Run, now: &str| r.m.when_asks(now)[0].3.clone();
    let get = |f: &BTreeMap<String, String>, k: &str| f.get(k).cloned();
    let at = f(&r, "2026-11-10T12:05:00Z");
    assert_eq!((get(&at, "waiting-bit"), get(&at, "oldest-waiting-bit")), (Some("2".into()), Some("4".into())));
    assert_eq!(get(&at, "waiting-batch"), None, "the binding's own kind is not counted");
    let text = r.m.when_asks("2026-11-10T12:05:00Z")[0].0.clone();
    let fire = |r: &mut Run| r.go(Input::Event { name: "when".into(), value: json!({"binding": text, "answer": "build", "decision": {"chosen": "build"}}) }).reply.unwrap();
    // a build cut, carrying both: while it is open they still wait (rule 1 holds the next one back)
    fire(&mut r);
    r.answer(202, "runner", json!({"head": "Cut", "carries": ["Z-200", "Z-201"]}));
    let at = f(&r, "2026-11-10T12:30:00Z");
    assert_eq!((get(&at, "open"), get(&at, "waiting-bit")), (Some("1".into()), Some("2".into())));
    // it fails: nothing closed since it fired, yet both wait again at once, aged from when they closed
    r.answer(202, "runner", json!({"head": "Red", "log": "case-1"}));
    assert!(matches!(r.m.tickets[&202].status, Status::Failed(_)));
    let at = f(&r, "2026-11-10T14:05:00Z");
    assert_eq!(get(&at, "open"), Some("0".into()));
    assert_eq!(get(&at, "closed-bit"), None, "the old factor counts only what closed since the cut");
    assert_eq!((get(&at, "waiting-bit"), get(&at, "oldest-waiting-bit")), (Some("2".into()), Some("124".into())));
    // a second build carries one and closes green: the other still waits, the carried one does not
    fire(&mut r);
    r.answer(203, "runner", json!({"head": "Cut", "carries": ["Z-200"]}));
    r.answer(203, "runner", json!("Green"));
    assert!(matches!(r.m.tickets[&203].status, Status::Done(_)));
    let at = f(&r, "2026-11-10T14:10:00Z");
    assert_eq!((get(&at, "waiting-bit"), get(&at, "oldest-waiting-bit")), (Some("1".into()), Some("129".into())));
    let (d, _) = defs_with(&CARRY_KINDS);
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert_eq!(m2.when_asks("2026-11-10T14:10:00Z"), r.m.when_asks("2026-11-10T14:10:00Z"));
}

/// WAITING-K counts a piece only while what opened it still wants it (docket-core3's item 11, paper-research N-101
/// p432): accepted though no build named it, taken out (its opener raised), or its opener cancelled, it never counts
/// again, so with nothing left waiting there is no factor for a rule to cut an empty build on.
const HELD_KINDS: [(&str, &str); 1] = [(
    "held.kind",
    "kind bit\n  letter Z\n  form task: Text\n  result Text\n  pattern\n    return task\n\nkind holder\n  letter H\n  form task: Text\n  result Text\n  raises PlanDoesNotFit\n  pattern\n    h <- open bit(task);\n    s <- step runner \"Accepted\" -> Accepted | Out(why: Text);\n    match s {\n      Accepted => return h\n      Out(why) => raise PlanDoesNotFit(why)\n    }\n\nkind batch\n  letter Y\n  form cut: Text\n  result Text\n  pattern\n    c <- step runner \"Cut\" -> Cut(carries: [Ticket]);\n    return cut\n\nwhen cut-build is build open batch(\"line\")\n",
)];

#[test]
fn a_piece_taken_out_stops_waiting_and_nothing_waits_for_an_empty_build() {
    let (d, errors) = defs_with(&HELD_KINDS);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    let mut holders = Vec::new();
    for task in ["accepted", "out", "cancelled", "waits"] {
        r.go(Input::Open { kind: "holder".into(), under: None, opener: "dev".into(), form: json!({"task": task}), text: None, names: vec!["lead".into()], told: vec![] }).reply.unwrap();
        holders.push(*r.m.tickets.range(..HIDDEN).filter(|(_, x)| x.kind == "holder").next_back().unwrap().0);
    }
    let f = |r: &Run| r.m.when_asks("2026-11-10T12:30:00Z")[0].3.clone();
    let waiting = |r: &Run| f(r).get("waiting-bit").cloned();
    // each bit closed done at once, and each holder is open on its step: all four wait
    assert_eq!(waiting(&r), Some("4".into()));
    // accepted with no build naming it (the cut was asked before it was there): it stops waiting
    r.answer(holders[0], "runner", json!("Accepted"));
    assert!(matches!(r.m.tickets[&holders[0]].status, Status::Done(_)));
    assert_eq!(waiting(&r), Some("3".into()));
    // taken out: its holder raises, back to the planner; it does not wait again
    r.answer(holders[1], "runner", json!({"head": "Out", "why": "red on the cut"}));
    assert!(matches!(r.m.tickets[&holders[1]].status, Status::Failed(_)));
    assert_eq!(waiting(&r), Some("2".into()));
    // its change cancelled
    r.go(Input::Cancel { t: holders[2], by: "owner".into(), why: "dropped".into() }).reply.unwrap();
    assert_eq!(waiting(&r), Some("1".into()));
    // the last one accepted: nothing waits, and neither waiting-bit nor its age is a factor
    r.answer(holders[3], "runner", json!("Accepted"));
    let at = f(&r);
    assert_eq!((at.get("waiting-bit"), at.get("oldest-waiting-bit")), (None, None), "{at:?}");
    let (d, _) = defs_with(&HELD_KINDS);
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert_eq!(m2.when_asks("2026-11-10T12:30:00Z"), r.m.when_asks("2026-11-10T12:30:00Z"));
}

/// A hand answer (`coord answer`) the command cannot read is refused with what the step takes and is not one of its
/// three; a list of records given as JSON is read whole, commas inside its values kept; only an answer read that does
/// not fit counts (docket-core3 N-101 p601, paper-research p602, 2 and 3).
#[test]
fn a_hand_answer_that_cannot_be_read_is_not_counted() {
    let mut named = BTreeMap::new();
    syntax::parse_types("system", syntax::SYSTEM_TYPES, &mut named).unwrap();
    let call = ("d.call".to_string(), "type Part {\n  name: Text\n  stands_on: [Text]\n}\n\ncall design(w: Text) -> Design(asked: Text, parts: [Part]) | Ask(question: Text)\n  task t\n".to_string());
    let calls: BTreeMap<String, CallSig> = syntax::parse_calls(&call.0, &call.1, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
    let kind = "kind des\n  form w: Text\n  result Text\n  pattern\n    d <- step x design(w);\n    return w\n";
    let (d, errors) = Defs::build(&[("des.kind".to_string(), kind.to_string())], calls, named, 200);
    assert!(errors.is_empty(), "{errors:?}");
    let mut m = Machine::load(d);
    m.apply(Input::Open { kind: "des".into(), under: None, opener: "a".into(), form: json!({"w": "x"}), text: None, names: vec![], told: vec![] }, "t".into()).reply.unwrap();
    let refusals = |m: &Machine| match &m.tickets[&200].status {
        Status::Awaits { refusals, .. } => *refusals,
        s => panic!("{s:?}"),
    };
    let answer = |m: &mut Machine, parts: &str| m.apply(Input::Answer { t: 200, by: "x".into(), head: json!({"head": "Design", "asked": "the words", "parts": parts}), text: Text::New(String::new()) }, "t".into());
    // the design's text where its parts go, and a JSON list cut at its commas as before: neither can be read
    for parts in ["Part about\nStands on: none.", "[{\"name\": \"about\""] {
        let out = answer(&mut m, parts);
        let e = out.reply.unwrap_err();
        assert!(e.contains("cannot be read as an answer to step 1") && e.contains("Nothing was counted") && e.contains("is not a record") && e.contains("given as JSON"), "{e}");
        assert!(out.lines.is_empty(), "not logged: {:?}", out.lines);
        assert_eq!(refusals(&m), 0);
    }
    // read, and not fitting (an outcome the step does not have): counted
    let out = m.apply(Input::Answer { t: 200, by: "x".into(), head: json!({"head": "Maybe"}), text: Text::New(String::new()) }, "t".into());
    assert!(out.reply.unwrap_err().contains("2 more and the step fails"));
    assert_eq!(refusals(&m), 1);
    // the list as JSON, a comma inside a value: taken
    answer(&mut m, "[{\"name\": \"about, and more\", \"stands_on\": []}]").reply.unwrap();
    let ViewStatus::Done { .. } = m.view(200).unwrap().status else { panic!("{:?}", m.view(200).unwrap().status) };
    let h = m.tickets[&200].posts.iter().rev().find_map(|p| p.head.clone()).unwrap();
    assert_eq!(h["parts"], json!([{"name": "about, and more", "stands_on": []}]));
}

/// A step halted before it is given holds the posts it would carry (paper-research N-101 p1015; docket-try2's run 2,
/// N-1, 18:12:10Z): the owner's post waits with the step and is given once, in it, not to the step's agent alone first.
#[test]
fn a_halted_step_holds_the_posts_it_carries() {
    let mut r = Run::slow_run();
    let (_, w) = r.slow_job();
    let out = r.approve(w, 1);
    assert_eq!(Run::halted_lines(&out), vec![(w, "give".to_string())]);
    r.go(Input::Post { t: w, by: "owner".into(), text: Text::New("use the header's first line".into()), names: vec!["dev".into()], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
    assert!(r.m.due("dev").is_empty(), "the post waits with the halted step: {:?}", r.m.due("dev"));
    r.approve_all(w);
    let d = r.take("dev");
    let What::Step { unread, .. } = &d.what else { panic!("{d:?}") };
    assert!(unread.iter().any(|p| p.text == Text::New("use the header's first line".into())), "{unread:?}");
    assert_eq!(Machine::load(crate::machine::Defs::clone(&r.m.defs)).replay(&r.log), Vec::<String>::new());
}

#[test]
fn an_edit_of_a_ticket_waiting_on_its_parts_holds_what_they_raise() {
    // paper-research N-101 p1631: `coord edit T` while T waits on tickets below it
    let br = |w: &str| format!("v <- step {w} triage(\"{w}\", \"e\", \"r\");\n          match v {{ Accept(n) => return \"{w}\", Dismiss(y) => raise Refused(y) }}");
    let k = format!(
        "kind poll\n  result [Text]\n  pattern\n    who <- return \"o\";\n    try {{\n      r <- any 2 of (\n          {},\n          {}\n        );\n      return r\n    }} handle {{\n      Refused(y) => return [\"old\", y]\n    }}\n",
        br("a"),
        br("b")
    );
    let d = || {
        let (d, errors) = defs_with(&[("poll.kind", k.as_str())]);
        assert!(errors.is_empty(), "{errors:?}");
        d
    };
    let open = |r: &mut Run| r.go(Input::Open { kind: "poll".into(), under: None, opener: "o".into(), form: json!({}), text: None, names: vec![], told: vec![] }).reply.unwrap();
    let ans = |r: &mut Run, t: u64, by: &str, head: Value| r.go(Input::Answer { t, by: by.into(), head, text: Text::New(String::new()) }).reply.unwrap();
    let e = |r: &mut Run, p: &str| r.go(Input::Edit { t: 200, by: "owner".into(), program: p.into(), why: "today".into() }).reply;
    let mut r = Run { m: Machine::load(d()), log: Vec::new(), clock: 0 };
    open(&mut r);
    assert_eq!(r.status(200), ViewStatus::Waits { children: vec!["P-201".into(), "P-202".into()] });
    let shown = r.m.view(200).unwrap().continuation.unwrap();
    assert!(shown.starts_with("// waits for 2 ticket(s) below\n// in scope:\n//   who = \"o\"\nanswer <- (in flight);\ntry {\n  r <- return answer;\n"), "{shown}");
    // the printed program, after its in-flight line, is taken as it is
    let rest = shown.split_once("answer <- (in flight);\n").unwrap().1.to_string();
    let ok = e(&mut r, &rest).unwrap();
    assert_eq!(ok, "P-200 edited: it still waits on P-201, P-202, and runs the new program on what they give (at most 0 more step(s)).");
    // refused, literally, and unlogged: what does not give the kind's result; `answer` named where it is not yet
    let n = r.log.len();
    let no = e(&mut r, "return \"x\"").unwrap_err();
    assert!(no.starts_with("The program cannot follow what P-200 waits for (P-201, P-202): ") && no.contains("It must give [Text]; in scope: answer: [Text], who: Text."), "{no}");
    let no = e(&mut r, "x <- return \"pre\";\nreturn answer").unwrap_err();
    assert!(no.starts_with("The program names `answer` where it is not there yet"), "{no}");
    let no = e(&mut r, "try {\n  r <- return answer;\n  return r\n} handle {\n  Refused(y) => return answer\n}").unwrap_err();
    assert!(no.starts_with("The program names `answer` where it is not there yet"), "{no}");
    assert_eq!(r.log.len(), n);
    // the parts are untouched: their steps are still owed, to the same agents
    assert!(!r.m.due("a").is_empty() && !r.m.due("b").is_empty());
    // a new handler, around the parts still running: a part's raise reaches it
    e(&mut r, "try {\n  r <- return answer;\n  return [\"new\"]\n} handle {\n  Refused(y) => return [who, y, \"edited\"]\n}").unwrap();
    assert!(matches!(r.status(200), ViewStatus::Waits { .. }));
    ans(&mut r, 202, "b", json!({"head": "Dismiss", "why": "no"}));
    assert_eq!(r.status(200), ViewStatus::Done { result: json!(["o", "no", "edited"]) });
    assert_eq!(Machine::load(d()).replay(&r.log), Vec::<String>::new());
    // the value, once it comes, goes to what follows; `answer` is bound where the program goes on past it
    let mut r = Run { m: Machine::load(d()), log: Vec::new(), clock: 0 };
    open(&mut r);
    e(&mut r, "match answer {\n  _ => return answer\n}").unwrap();
    ans(&mut r, 201, "a", json!("Accept"));
    ans(&mut r, 202, "b", json!("Accept"));
    assert_eq!(r.status(200), ViewStatus::Done { result: json!(["a", "b"]) });
    assert_eq!(Machine::load(d()).replay(&r.log), Vec::<String>::new());
}

#[test]
fn an_edit_is_refused_while_the_parts_run_inside_what_it_would_end() {
    let k = "kind poll\n  result Text\n  pattern\n    let agent p in {\n      r <- par(\n          step p triage(\"w\", \"e\", \"r\"),\n          step p triage(\"x\", \"e\", \"r\")\n        );\n      return \"both\"\n    }\n";
    let (d, errors) = defs_with(&[("poll.kind", k)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    r.go(Input::Open { kind: "poll".into(), under: None, opener: "o".into(), form: json!({}), text: None, names: vec![], told: vec![] }).reply.unwrap();
    assert!(matches!(r.status(200), ViewStatus::Waits { .. }));
    let no = r.go(Input::Edit { t: 200, by: "owner".into(), program: "return \"x\"".into(), why: "w".into() }).reply.unwrap_err();
    assert_eq!(no, "P-200 waits on P-201, P-202 inside the `let agent` of f200-1, which an edit would end under them: it is edited once they close, or the ticket is repaired.");
}

#[test]
fn a_tie_sets_a_threads_place_in_the_outline_at_its_parents_post_and_a_title_its_label() {
    let mut r = Run::new();
    let a = r.say("lead", &["fixer"], "the parser");
    let b = r.say("lead", &["fixer"], "the lexer");
    let c = r.say("lead", &["fixer"], "the printer");
    let (ia, ib, ic) = (r.m.id(a), r.m.id(b), r.m.id(c));
    assert_eq!(r.m.outline_parent(b), None, "said by the owner: under nothing");
    let before = r.m.view(b).unwrap();
    // the tie stamps A's last post: B's line stands there
    let n = r.m.tickets[&a].posts.last().unwrap().n;
    let out = r.go(Input::Tie { t: b, to: Some(a), by: "owner".into() });
    assert_eq!(out.reply, Ok(format!("{ib} is tied under {ia}, at its post {n}.")));
    assert!(out.effects.contains(&Effect::Changed { t: a }) && out.effects.contains(&Effect::Changed { t: b }), "{out:?}");
    assert_eq!(r.m.outline_parent(b), Some((a, n)));
    assert_eq!(r.m.view(b).unwrap(), before, "the tie is the outline's only: the ticket's view is as it was");
    assert!(r.go(Input::Tie { t: b, to: Some(a), by: "owner".into() }).lines.is_empty(), "tied already: no line");
    // C tied under A after a later post of A: it stands after B
    r.post_on(a, "owner", "and the tests", &[]);
    r.go(Input::Tie { t: c, to: Some(a), by: "owner".into() }).reply.unwrap();
    let o = ViewOutline::of(&r.m, &r.m.tickets[&a], true);
    assert_eq!(o.children.iter().map(|k| k.id.as_str()).collect::<Vec<_>>(), vec![ib.as_str(), ic.as_str()]);
    assert_eq!(o.children[0].parent.as_deref(), Some(ia.as_str()));
    // a loop is refused by name
    let out = r.go(Input::Tie { t: a, to: Some(c), by: "owner".into() });
    assert_eq!(out.reply, Err(format!("{ic} is already under {ia} ({ic} → {ia}): tying {ia} under {ic} would make a loop.")));
    assert!(out.lines.is_empty());
    assert!(r.go(Input::Tie { t: a, to: Some(a), by: "owner".into() }).reply.is_err(), "nor under itself");
    // who may: the owner, its opener, a lead
    let out = r.go(Input::Tie { t: c, to: Some(b), by: "fixer".into() });
    assert!(out.reply.as_ref().is_err_and(|e| e.starts_with("fixer may not tie")), "{out:?}");
    r.go(Input::Role { agent: "boss".into(), role: "lead".into(), also: vec![] });
    // tying to another parent later is a new line; the history before it stands
    let out = r.go(Input::Tie { t: c, to: Some(b), by: "boss".into() });
    assert!(out.reply.is_ok() && out.effects.contains(&Effect::Changed { t: a }), "the old parent changes too: {out:?}");
    assert_eq!(r.m.outline_parent(c).map(|p| p.0), Some(b));
    assert_eq!(ViewOutline::of(&r.m, &r.m.tickets[&a], true).children.len(), 1);
    // untied
    r.go(Input::Tie { t: b, to: None, by: "owner".into() }).reply.unwrap();
    assert_eq!(r.m.outline_parent(b), None);
    assert!(r.go(Input::Tie { t: b, to: None, by: "owner".into() }).lines.is_empty());
    // a title is the label; without one the label is cut from the first line
    let cut = ViewOutline::of(&r.m, &r.m.tickets[&a], false).label;
    assert!(cut.contains("parser"), "{cut}");
    assert!(r.go(Input::Title { t: a, label: "  ".into(), by: "owner".into() }).reply.is_err(), "an empty label is refused");
    assert!(r.go(Input::Title { t: a, label: "Parse".into(), by: "fixer".into() }).reply.is_err());
    r.go(Input::Title { t: a, label: " The parse step ".into(), by: "owner".into() }).reply.unwrap();
    assert_eq!(ViewOutline::of(&r.m, &r.m.tickets[&a], false).label, "The parse step");
    // the state word: one it waits on only, and that one has it, reads at work (docket-edge N-397.p2)
    for t in [a, b, c] {
        let o = ViewOutline::of(&r.m, &r.m.tickets[&t], false);
        assert_eq!(o.owed, o.state == "waits on you", "{o:?}");
        if o.on.len() == 1 && o.who.as_ref() == o.on.first() && !o.owed {
            assert_eq!(o.state, "at work", "{o:?}");
        }
    }
    r.replays();
}

#[test]
fn a_cut_label_passes_over_the_quoted_lines_and_a_given_one_is_as_given() {
    use crate::label::{first_line, given_line};
    assert_eq!(first_line("> 5. \"Engine\". I mean the back-ends\n\nfirst lower the load", 60), "first lower the load");
    assert_eq!(first_line(">\n> > the asked line\n> more", 60), "the asked line", "all quotes: the first quote's words");
    assert_eq!(first_line("## The plan\n> quoted", 60), "The plan");
    assert_eq!(given_line("> as given", 60), "> as given");
    let mut r = Run::new();
    let t = r.say("lead", &["fixer"], "> what was asked\n\nmy answer to it");
    assert_eq!(ViewOutline::of(&r.m, &r.m.tickets[&t], false).label, "my answer to it");
    r.go(Input::Title { t, label: "> kept as titled".into(), by: "owner".into() }).reply.unwrap();
    assert_eq!(ViewOutline::of(&r.m, &r.m.tickets[&t], false).label, "> kept as titled");
}

/// A thread ends (paper-research N-447, piece 1): resting at its hole, its hole is filled with Done(why), Dropped(why)
/// or Kept, and it leaves the outline; an agents' thread keeps Between and is marked ended; one at work, an item, one
/// ended already, and a how that is none of the three are refused. A thread opened before Done and Dropped were
/// results (its own text older) ends with them too. The log replays.
#[test]
fn a_thread_ends_done_dropped_or_kept_and_leaves_the_outline() {
    // an open thread whose own text is from before Done and Dropped: opened under that text, today's loaded after
    let old_thread = KINDS.iter().find(|(f, _)| *f == "thread.kind").unwrap().1.replace("    Done(why: Text)       /// the work it carried is done; why says how (`coord end T done WHY`)\n    | Dropped(why: Text)  /// the work it carried was given up; why says why (`coord end T dropped WHY`)\n    | Kept ", "    Kept ");
    assert!(!old_thread.contains("| Dropped(why: Text)"), "the old text has no Dropped");
    let (old, errors) = defs_with(&[("thread.kind", &old_thread)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(old), log: Vec::new(), clock: 0 };
    let o = r.say("lead", &["fixer"], "the old one");
    *r.m.defs.edit() = defs();
    let end = |r: &mut Run, t: u64, how: &str, why: &str| r.go(Input::End { t, by: "lead".into(), how: how.into(), why: why.into() });
    let out = end(&mut r, o, "dropped", "superseded by the \"new\" one");
    assert!(out.reply.is_ok(), "{out:?}");
    assert!(matches!(&r.m.tickets[&o].status, Status::Done(v) if v["head"] == "Dropped" && v["why"] == "superseded by the \"new\" one"), "{:?}", r.m.tickets[&o].status);

    // resting at its hole: Done(why)
    let a = r.say("lead", &["fixer"], "the parser");
    let b = r.say("lead", &["fixer"], "the lexer");
    let (ia, ib) = (r.m.id(a), r.m.id(b));
    r.go(Input::Tie { t: b, to: Some(a), by: "owner".into() }).reply.unwrap();
    assert!(matches!(r.m.tickets[&b].status, Status::Rests { .. }));
    assert!(ViewOutline::of(&r.m, &r.m.tickets[&b], false).open);
    let out = end(&mut r, b, "finished", "w");
    assert_eq!(out.reply, Err(String::from("A thread ends done, dropped or kept, not finished.")));
    assert!(out.lines.is_empty());
    assert_eq!(end(&mut r, b, "done", "  ").reply, Err(format!("Ending {ib} needs one line saying why.")));
    let out = end(&mut r, b, "done", "merged\nand more");
    assert_eq!(out.reply, Err(format!("Ending {ib} takes one line saying why, not several.")));
    assert!(out.lines.is_empty(), "a refused end writes nothing");
    let out = end(&mut r, b, "done", "the lexer is merged");
    assert_eq!(out.reply, Ok(format!("{ib} has ended (done): it leaves the outline.")));
    assert!(out.effects.contains(&Effect::Changed { t: a }), "its parent's line changes: {out:?}");
    assert!(matches!(&r.m.tickets[&b].status, Status::Done(v) if v["head"] == "Done" && v["why"] == "the lexer is merged"));
    let v = r.m.view(b).unwrap();
    assert_eq!(v.ended.as_ref().map(|e| (e.how.as_str(), e.why.as_str(), e.by.as_str())), Some(("done", "the lexer is merged", "lead")));
    let line = ViewOutline::of(&r.m, &r.m.tickets[&b], false);
    assert_eq!((line.open, line.state.as_str(), line.why.as_deref()), (false, "done", Some("the lexer is merged")));
    // the parent's line for it stays, marked ended
    let kids = ViewOutline::of(&r.m, &r.m.tickets[&a], true).children;
    assert_eq!(kids.iter().map(|k| (k.id.as_str(), k.state.as_str())).collect::<Vec<_>>(), vec![(ib.as_str(), "done")]);
    assert_eq!(end(&mut r, b, "kept", "again").reply, Err(format!("{ib} has ended already (done, by lead: the lexer is merged).")));
    // kept: the result as before, the why on the end
    end(&mut r, a, "kept", "talk over").reply.unwrap();
    assert!(matches!(&r.m.tickets[&a].status, Status::Done(v) if v["head"] == "Kept"));
    assert_eq!(ViewOutline::of(&r.m, &r.m.tickets[&a], false).why.as_deref(), Some("talk over"));
    let _ = ia;

    // an agents' thread: Between, open in the outline till ended; then ended, its result as it was
    let out = r.go(Input::Open { kind: "thread".into(), under: None, opener: "dev".into(), form: json!({"to": "orch", "typed": ["orch"], "names": ["orch"], "text": "", "addressed": {"head": "Sent", "by": "dev"}, "follows": null}), text: Some(Text::New("the build is slow".into())), names: vec!["orch".into()], told: vec![] });
    assert!(out.reply.is_ok(), "{out:?}");
    let s = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    assert!(r.m.tickets[&s].between());
    assert!(ViewOutline::of(&r.m, &r.m.tickets[&s], false).open);
    let out = r.go(Input::End { t: s, by: "dev".into(), how: "kept".into(), why: "answered".into() });
    assert!(out.reply.is_ok(), "{out:?}");
    assert!(r.m.tickets[&s].between(), "its result stays Between");
    let line = ViewOutline::of(&r.m, &r.m.tickets[&s], false);
    assert_eq!((line.open, line.state.as_str()), (false, "kept"));
    // a later post on it is posted, and it stays ended
    r.post_on(s, "orch", "faster now", &[]).reply.unwrap();
    assert!(!ViewOutline::of(&r.m, &r.m.tickets[&s], false).open);

    // at work (the watcher's intake running): refused, nothing changes
    let w = r.say("lead", &[], "is this a thread?");
    let iw = r.m.id(w);
    let out = end(&mut r, w, "done", "w");
    assert_eq!(out.reply, Err(format!("{iw} is at work, not resting at its hole: it can be ended when it rests, and `coord cancel {iw}` stops it now.")));
    assert!(out.lines.is_empty() && r.m.tickets[&w].is_open());
    // a ticket of another kind
    r.go(Input::Open { kind: "claim".into(), under: None, opener: "DEBUG".into(), form: json!({"what": "load fails", "evidence": "e", "where": "path:x:1", "branch": "main"}), text: None, names: vec![], told: vec![] }).reply.unwrap();
    let c = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    assert!(end(&mut r, c, "done", "w").reply.unwrap_err().contains("not a thread"));
    r.replays();
}

/// An end and the asks to the owner (paper-research N-447 p2, 3; p5): another agent's ask still open refuses the
/// end, by name, and writes nothing; the ender's own asks are withdrawn with the end's why; the owner's end closes
/// them as his "nothing to answer". The log replays.
#[test]
fn an_end_withdraws_the_enders_asks_refuses_on_anothers_and_the_owners_closes_them() {
    use crate::line::AsksClosed;
    let closed = |r: &Run, t: u64, n: u32| r.m.view(t).unwrap().posts.iter().find(|p| p.n == n).unwrap().asks_closed.clone();
    let mut r = Run::before_asks();
    let t = r.note("lead", "owner", "the release: A or B?");
    r.asks(1, json!({"head": "Asks", "marks": [Run::mark([13, 19], &[])]}));
    let id = r.m.id(t);
    let out = r.go(Input::End { t, by: "fixer".into(), how: "kept".into(), why: "over".into() });
    assert_eq!(out.reply, Err(format!("{id} waits on the owner: what post 1 (lead) asks him is open. The owner ends it, or the asker withdraws the asks first.")));
    assert!(out.lines.is_empty() && r.m.tickets[&t].ended.is_none());
    let out = r.go(Input::End { t, by: "lead".into(), how: "dropped".into(), why: "B is out".into() });
    assert!(out.reply.is_ok(), "{out:?}");
    assert_eq!(closed(&r, t, 1), Some(AsksClosed::Withdrawn { by: "lead".into(), why: "B is out".into() }));
    // the owner's end
    let u = r.note("lead", "owner", "the next release: C or D?");
    r.asks(1, json!({"head": "Asks", "marks": [Run::mark([18, 24], &[])]}));
    r.go(Input::End { t: u, by: "owner".into(), how: "kept".into(), why: "C".into() }).reply.unwrap();
    assert_eq!(closed(&r, u, 1), Some(AsksClosed::NothingToAnswer));
    r.replays_today();
}

#[test]
fn a_views_mentions_are_read_back_inside_a_flattened_line() {
    // the bridge's lines are `{"seq": N, "ticket": {...}}`, the body flattened (coord-types' Event): read back there,
    // the mentions' post numbers come as string keys (paper-research N-539)
    #[derive(serde::Serialize, serde::Deserialize, Debug)]
    #[serde(rename_all = "lowercase")]
    enum Body {
        Ticket(crate::view::TicketView),
    }
    #[derive(serde::Serialize, serde::Deserialize, Debug)]
    struct Event {
        seq: u64,
        #[serde(flatten)]
        body: Body,
    }
    let mut r = Run::new();
    let out = r.go(Input::Open { kind: "thread".into(), under: None, opener: "dev".into(), form: json!({"to": "dev", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "dev"}, "follows": null}), text: Some(Text::New("@orch see N-1".into())), names: vec!["orch".into()], told: vec![] });
    let t = parse_ticket(out.reply.unwrap().trim_start_matches("Opened ").trim_end_matches('.')).unwrap();
    let mut v = r.m.view(t).unwrap();
    v.mentions.insert(1, vec!["N-1".into()]);
    let line = serde_json::to_string(&Event { seq: 7, body: Body::Ticket(v.clone()) }).unwrap();
    let back: Event = serde_json::from_str(&line).unwrap();
    let Body::Ticket(w) = back.body;
    assert_eq!((back.seq, w), (7, v));
}

/// A subthread opened at a post (paper-research N-447, piece 2): the owner's words with `tied_at` on the event, or an
/// agent's message with `tied_at` in its form, open a thread that stands under that ticket at that post from its
/// open; the label is the given one, else cut from the first line. A place that is not there leaves it untied. The log
/// replays.
#[test]
fn a_subthread_opened_at_a_post_stands_there_from_its_open() {
    let mut r = Run::new();
    let a = r.say("lead", &["fixer"], "the parser");
    r.post_on(a, "fixer", "the lexer first?", &[]).reply.unwrap();
    r.post_on(a, "lead", "yes", &[]).reply.unwrap();
    let ia = r.m.id(a);
    assert_eq!(r.m.place_of(&format!("{ia}.p2")), Some((a, 2)));
    assert_eq!(r.m.place_of(&ia), Some((a, 3)), "a ticket: at its last post");
    assert_eq!(r.m.place_of(&format!("{ia}.p9")), None, "no such post");
    assert_eq!(r.m.place_of(&format!("{ia}.p+2")), None, "a number not all digits names no post (`post_id`)");
    assert_eq!(r.m.place_of("N-9999"), None);
    // its ticket by `ticket_named`: the id as written, and never its number written another way
    assert_eq!(r.m.ticket_named(&ia), Some(a));
    for other in [a.to_string(), ia.to_lowercase(), format!(" {ia}"), format!("consult-{a}"), "N-9999".to_string()] {
        assert_eq!((r.m.ticket_named(&other), r.m.place_of(&other)), (None, None), "{other}");
    }
    // the owner's, in a subthread at post 2, labelled
    let out = r.go(Input::Event { name: "owner_message".into(), value: json!({"from": "owner", "to": "fixer", "typed": ["fixer"], "names": [], "text": "the lexer: keep comments\nmore", "addressed": {"head": "Named"}, "tied_at": format!("{ia}.p2"), "label": "keep comments"}) });
    assert!(out.reply.is_ok(), "{out:?}");
    assert!(out.effects.contains(&Effect::Changed { t: a }), "the parent's line changes: {out:?}");
    let b = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    assert_eq!(r.m.outline_parent(b), Some((a, 2)));
    assert!(matches!(r.m.tickets[&b].status, Status::Rests { .. }), "no intake: it rests at its hole");
    let ob = ViewOutline::of(&r.m, &r.m.tickets[&b], false);
    assert_eq!((ob.label.as_str(), ob.at_post), ("keep comments", Some(2)));
    let kids = ViewOutline::of(&r.m, &r.m.tickets[&a], true).children;
    assert_eq!(kids.iter().map(|k| k.id.clone()).collect::<Vec<_>>(), vec![r.m.id(b)]);
    // an agent's, at the ticket (its last post), no label: cut from the first line
    let out = r.go(Input::Open { kind: "thread".into(), under: None, opener: "fixer".into(), form: json!({"to": "lead", "typed": ["lead"], "names": ["lead"], "text": "the printer next", "addressed": {"head": "Sent", "by": "fixer"}, "follows": null, "tied_at": format!("{ia}.p3")}), text: Some(Text::New("the printer next".into())), names: vec!["lead".into()], told: vec![] });
    assert!(out.reply.is_ok(), "{out:?}");
    let c = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    assert_eq!(r.m.outline_parent(c), Some((a, 3)));
    assert!(ViewOutline::of(&r.m, &r.m.tickets[&c], false).label.contains("printer"));
    assert_eq!(ViewOutline::of(&r.m, &r.m.tickets[&a], true).children.len(), 2);
    // a place not there: untied
    r.go(Input::Event { name: "owner_message".into(), value: json!({"from": "owner", "to": "fixer", "typed": ["fixer"], "names": [], "text": "elsewhere", "addressed": {"head": "Named"}, "tied_at": "N-9999.p1"}) }).reply.unwrap();
    let d = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    assert_eq!(r.m.outline_parent(d), None);
    r.replays();
}

/// A kind says how its tickets stand in the thread they stand at (proto-lead N-1446 p17): `drawn line` (thread.kind,
/// item.kind) or `drawn mark`; a kind that says nothing keeps the rule from before (thread and item are lines, nothing
/// else is drawn). A mark opened by a binding whose event carries `tied_at` stands at that post from its open: a child
/// of the thread's line at that post, its view saying `drawn: mark`, never a row of the outline of its own. The log
/// replays.
#[test]
fn a_kind_says_how_its_tickets_are_drawn_and_a_mark_stands_at_its_post() {
    use crate::syntax::Drawn;
    let named = BTreeMap::new();
    let kind = |t: &str| syntax::parse_kind("b.kind", t, 1, &named);
    assert_eq!(kind("kind b\n  drawn mark   // at its post\n  pattern\n    return ()\n").unwrap().drawn, Some(Drawn::Mark));
    assert_eq!(kind("kind b\n  drawn line\n  pattern\n    return ()\n").unwrap().drawn, Some(Drawn::Line));
    // a note: a mark drawn once it has spoken (consult-3 N-1446 p890, proto-reader p901); it stands as a mark does
    assert_eq!(kind("kind b\n  drawn note\n  pattern\n    return ()\n").unwrap().drawn, Some(Drawn::Note));
    assert!(Drawn::Note.is_mark() && Drawn::Mark.is_mark() && !Drawn::Line.is_mark());
    assert_eq!(kind("kind b\n  pattern\n    return ()\n").unwrap().drawn, None);
    for bad in ["kind b\n  drawn\n  pattern\n    return ()\n", "kind b\n  drawn dot\n  pattern\n    return ()\n", "kind b\n  drawn line mark\n  pattern\n    return ()\n"] {
        assert!(kind(bad).unwrap_err().starts_with("b.kind:2: `drawn line`"), "{bad}");
    }
    let brain = [("brain.kind", "event marked { agent: Text, tied_at: Text }\n\n/// the brain's attention pass\nkind brain\n  form agent: Text\n  result Quiet | Said(text: Text)\n  drawn mark\n  pattern\n    return Quiet\n\nkind aside\n  form agent: Text\n  pattern\n    return ()\n\non marked(e) open brain(e.agent)\non marked(e) open aside(e.agent)\n")];
    let (d, errors) = defs_with(&brain);
    assert!(errors.is_empty(), "{errors:?}");
    // agent-base's own kinds say it now (its brain.kind, a note: tests/said.rs)
    assert_eq!((d.kinds["thread"].drawn, d.kinds["item"].drawn, d.kinds["question"].drawn), (Some(Drawn::Line), Some(Drawn::Line), None));
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    let a = r.say("lead", &["fixer"], "the parser");
    r.post_on(a, "fixer", "the lexer first?", &[]).reply.unwrap();
    r.post_on(a, "lead", "yes", &[]).reply.unwrap();
    let ia = r.m.id(a);
    let out = r.go(Input::Event { name: "marked".into(), value: json!({"agent": "fixer", "tied_at": format!("{ia}.p2")}) });
    assert!(out.reply.is_ok(), "{out:?}");
    let ts: Vec<u64> = r.m.tickets.range(..HIDDEN).map(|(t, _)| *t).filter(|t| *t != a && r.m.tickets[t].kind != "thread").collect();
    let (b, aside) = (*ts.iter().find(|t| r.m.tickets[t].kind == "brain").unwrap(), *ts.iter().find(|t| r.m.tickets[t].kind == "aside").unwrap());
    // the mark stands at his post 2; the kind that says nothing is not tied there
    assert_eq!((r.m.outline_parent(b), r.m.outline_parent(aside)), (Some((a, 2)), None));
    assert!(out.effects.contains(&Effect::Changed { t: a }), "the thread's line changes: {out:?}");
    let (bm, am, tm) = (&r.m.tickets[&b], &r.m.tickets[&aside], &r.m.tickets[&a]);
    assert_eq!((crate::view::drawn(&r.m, bm), crate::view::drawn(&r.m, am), crate::view::drawn(&r.m, tm)), (Some(Drawn::Mark), None, Some(Drawn::Line)));
    assert_eq!((r.m.view(b).unwrap().drawn, r.m.view(aside).unwrap().drawn, r.m.view(a).unwrap().drawn), (Some(Drawn::Mark), None, Some(Drawn::Line)));
    // a child of the thread's line, at its post; never a line of the outline itself
    let kids = ViewOutline::of(&r.m, tm, true).children;
    assert_eq!(kids.iter().map(|k| (k.id.clone(), k.at_post)).collect::<Vec<_>>(), vec![(r.m.id(b), Some(2))]);
    // what the mark came to, as a handler's run says it (`view::said`); a line says none
    assert_eq!((kids[0].said.as_deref(), ViewOutline::of(&r.m, tm, true).said), (Some("Quiet"), None));
    assert!(!crate::view::outlined(&r.m, bm) && crate::view::outlined(&r.m, tm) && crate::view::stands(&r.m, bm) && !crate::view::stands(&r.m, am));
    // a view from before the clause reads (no `drawn`), and a mark's is said
    let v = serde_json::to_value(r.m.view(b).unwrap()).unwrap();
    assert_eq!(v["drawn"], json!("mark"));
    let mut old = serde_json::to_value(r.m.view(a).unwrap()).unwrap();
    old.as_object_mut().unwrap().remove("drawn");
    assert_eq!(serde_json::from_value::<crate::view::TicketView>(old).unwrap().drawn, None);
    r.replays_with(&brain);
}

/// 0.2 A1 (N-1397 posts 5 and 6): coordd's `delivered` event marks delivered the giving at its place, once, and that
/// shows on the given row and on his post's `Where`. One naming another place (another giving of the same id) marks
/// nothing. One that began a turn gives it back to that turn after a cut forgot it (the esc shape). A replay makes the
/// same, from the lines alone.
#[test]
fn a_delivered_event_marks_the_giving_at_its_place_once_and_gives_a_prompt_back_to_its_turn_after_a_cut() {
    let mut r = Run::new();
    let t = r.note("owner", "research", "how long?");
    let d = r.m.due("research").into_iter().find(|d| d.t == t).expect("due to research");
    r.go(Input::Given { t, to: "research".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Run::loc(40)) }).reply.unwrap();
    let delivered = |r: &Run| r.m.tickets[&t].given.iter().find(|g| g.to == "research" && g.delivery == d.id).unwrap().delivered.clone();
    let event = |place: u64| Input::Event { name: DELIVERED.into(), value: json!({"t": t, "to": "research", "delivery": d.id, "at": "2026-11-20T09:00:05Z", "place": Run::loc(place), "began": true}) };
    assert_eq!(delivered(&r), None, "given, not delivered");
    // another place: another giving's, so nothing here
    let out = r.go(event(41));
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert_eq!(delivered(&r), None);
    // the turn it was given in is cut (an esc), and the session puts it in as the next turn's prompt
    r.go(Input::TurnCut { agent: "research".into() }).reply.unwrap();
    assert!(r.m.given_since("research").is_empty());
    let out = r.go(event(40));
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(matches!(&out.lines[..], [Line::Event { name, on: None, fired, counted, .. }] if name == DELIVERED && fired.is_empty() && counted.is_empty()), "{:?}", out.lines);
    assert!(out.effects.contains(&Effect::Changed { t }), "the line under his post changes: {:?}", out.effects);
    assert_eq!(delivered(&r).as_deref(), Some("2026-11-20T09:00:05Z"));
    assert_eq!(r.m.given_since("research"), vec![d.id.clone()], "the turn it began holds it");
    // seen again: once
    let again = r.go(event(40));
    assert!(again.reply.is_ok() && !again.effects.contains(&Effect::Changed { t }), "{again:?}");
    // the view
    let v = r.m.view(t).unwrap();
    assert_eq!(v.given.iter().filter(|g| g.to == "research").map(|g| g.delivered.clone()).collect::<Vec<_>>(), vec![Some("2026-11-20T09:00:05Z".to_string())]);
    assert_eq!(r.where_is(t, 1, "research").and_then(|w| w.delivered).as_deref(), Some("2026-11-20T09:00:05Z"));
    // its turn ends: read, and nothing more is held for it
    r.turn("research", Some(d.id.as_str()), 50);
    assert!(r.m.given_since("research").is_empty());
    r.replays();
}

/// 0.2 A4 (N-1397 post 5, 3): coordd's `due_again` event makes due again only the deliveries it names, of what the
/// agent was given since its last turn ended; the rest stays given (one still in its session's queue waits). One whose
/// process ended before it entered (`exited`) is due again each time and is never "given twice, not read"; one a live
/// session dropped is, the second time, as `unread` says. A replay makes the same, from the lines alone.
#[test]
fn a_due_again_event_makes_due_only_what_it_names_and_a_process_end_is_never_a_second_miss() {
    // (his posts given twice and read neither time wait for the next turn's end or process's end: below)
    let mut r = Run::new();
    let (a, b) = (r.note("owner", "research", "how long?"), r.note("owner", "research", "and the width?"));
    let give = |r: &mut Run, t: u64, place: u64| -> String {
        let d = r.m.due("research").into_iter().find(|d| d.t == t).expect("due to research");
        r.go(Input::Given { t, to: "research".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Run::loc(place)) }).reply.unwrap();
        d.id
    };
    let (da, db) = (give(&mut r, a, 40), give(&mut r, b, 60));
    let due_again = |ds: &[&str], exited: bool| Input::Event { name: DUE_AGAIN.into(), value: json!({"agent": "research", "deliveries": ds, "exited": exited}) };
    let twice = |r: &Run| r.m.tickets[&a].posts.iter().any(|p| p.by == "coordd");
    // the posts of A due to research (after its second miss the delivery is named past coordd's post, shown to nobody)
    let due = |r: &Run| r.m.due("research").iter().any(|d| d.t == a);
    // A's process ended before A entered; B entered and stays given, for its turn
    let out = r.go(due_again(&[&da], true));
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(matches!(&out.lines[..], [Line::Event { name, on: None, .. }] if name == DUE_AGAIN), "{:?}", out.lines);
    assert!(out.effects.contains(&Effect::Deliver { to: "research".into() }), "{:?}", out.effects);
    assert_eq!(r.m.given_since("research"), vec![db.clone()], "B stays given");
    assert!(due(&r), "A is due again");
    // given again, and its new process ends before it entered: due again, never "twice"
    assert_eq!(give(&mut r, a, 80), da);
    r.go(due_again(&[&da], true)).reply.unwrap();
    assert!(due(&r) && !twice(&r), "due again, not said twice");
    // given a third time and dropped by a live session: due again once, the process ends not counted
    give(&mut r, a, 100);
    r.go(due_again(&[&da], false)).reply.unwrap();
    assert!(due(&r) && !twice(&r));
    // dropped again: said on its ticket (shown to nobody), and his post is held until research's next turn's end or
    // process's end, never left behind (consult-3 N-818.p76, 2)
    give(&mut r, a, 120);
    r.go(due_again(&[&da], false)).reply.unwrap();
    assert!(twice(&r) && !due(&r) && r.m.missed("research"), "given twice, not read: held");
    let v = r.m.view(a).unwrap();
    assert!(v.given.iter().filter(|g| g.to == "research").all(|g| g.unread), "the line under it: not read");
    // one no longer given since the turn's end: nothing
    let out = r.go(due_again(&[&da], false));
    assert!(out.reply.is_ok() && !out.effects.contains(&Effect::Deliver { to: "research".into() }), "{out:?}");
    assert_eq!(r.m.given_since("research"), vec![db.clone()]);
    // research's turn on B ends: A is due again
    let out = r.turn("research", Some(db.as_str()), 140);
    assert!(out.reply.is_ok() && out.effects.contains(&Effect::Deliver { to: "research".into() }), "{out:?}");
    assert!(due(&r) && !r.m.missed("research"));
    // missed twice more: held again, and its process's end hands it again
    let da2 = give(&mut r, a, 160);
    r.go(due_again(&[&da2], false)).reply.unwrap();
    assert!(due(&r) && !r.m.missed("research"), "once: due again");
    assert_eq!(give(&mut r, a, 170), da2);
    r.go(due_again(&[&da2], false)).reply.unwrap();
    assert!(!due(&r) && r.m.missed("research"), "twice: held");
    let out = r.go(due_again(&[], true));
    assert!(out.reply.is_ok() && out.effects.contains(&Effect::Deliver { to: "research".into() }), "{out:?}");
    assert!(due(&r) && !r.m.missed("research"));
    r.replays();
}

/// A ticket the replay refuses keeps its number (paper-research N-1446 p327). Build 36 refused 0.3's asides, whose
/// recorded program it could not read, and then gave its next ticket the aside's number: coming forward, that open was
/// "ticket 201 exists already" and 36's lines landed on the aside (docket-core2, N-1446). Here ticket 201's recorded
/// form no longer fits its program, so its open is refused as 36 refused the aside's: a refusal that holds in every
/// release, where a word one does not read is read by a later one (`drawn` from 27cbe87b, `gather` from E1b;
/// proto-reader N-1446 p513, p515).
/// The next open takes 202, and the machine that wrote the record replays it whole with that open after it. So do
/// the numbers of the hidden tickets, the message runtime's, and the cards on a ticket, each counted on its own: lines
/// a newer build made and this one does not (0.3's brain tickets, a delivery ticket, the burst card on a slowed
/// thread) keep theirs, moved at the input they stand at (proto-lead p346.3; proto-reader p347, consult-3 p349).
#[test]
fn a_ticket_the_replay_refuses_keeps_its_number() {
    // a step to `to`, where 200 and 201 rest
    const RESTS: &str = "kind rests\n  letter N\n  form to: Agent\n  result Fine\n  pattern\n    step to \"Rest.\" -> Fine\n";
    let (d, errors) = defs_with(&[("rests.kind", RESTS)]);
    assert!(errors.is_empty(), "{errors:?}");
    let open = |names: Vec<String>| Input::Open { kind: "rests".into(), under: None, opener: "owner".into(), form: json!({"to": "w1"}), text: None, names, told: vec![] };
    let mut m = Machine::load(d.clone());
    let mut log = Vec::new();
    for (names, at) in [(vec!["w1".to_string()], "2026-11-20T22:00:01Z"), (vec![], "2026-11-20T22:00:02Z")] {
        let out = m.apply(open(names), at.into());
        assert!(out.reply.is_ok(), "{out:?}");
        log.extend(out.lines);
    }
    assert_eq!(m.tickets.range(..HIDDEN).map(|(t, _)| *t).collect::<Vec<_>>(), vec![200, 201]);
    // the record as a newer build wrote it: 201's form is one this machine's `rests` does not take
    let mut back = log.clone();
    for l in back.iter_mut() {
        if let Line::Open { t: 201, form, .. } = l {
            *form = json!({});
        }
    }
    assert_ne!(back, log);
    // with it, what a newer build made at that input: a hidden ticket under it, a delivery ticket; and at 200's open,
    // a card on 200 this machine does not make there
    let at = back.iter().position(|l| matches!(l, Line::Open { t: 201, .. })).unwrap();
    let now = || "2026-11-20T22:00:02Z".to_string();
    back.insert(at + 1, Line::Opened { at: now(), t: HIDDEN + 5, kind: "owner_post".into(), parent: 201, program: String::new(), calls: BTreeMap::new() });
    back.insert(at + 2, Line::Event { at: now(), name: syntax::DELIVERY_OPENED.into(), value: json!({"t": DESK + 3, "agent": "w1", "program": ""}), fired: Vec::new(), counted: Vec::new(), on: None });
    back.insert(at, Line::Halted { at: now(), t: 200, n: 2, what: "deliver".into() });
    let mut m2 = Machine::load(d.clone());
    assert_eq!((m2.next_hidden(), m2.next_desk()), (HIDDEN, DESK));
    let problems = m2.replay(&back);
    assert!(problems.len() == 2 && problems[0].contains("where the log has") && problems[1].starts_with(&format!("line {}: the input is now refused: The form of a rests does not fit", at + 2)), "{problems:?}");
    assert_eq!(m2.tickets.range(..HIDDEN).map(|(t, _)| *t).collect::<Vec<_>>(), vec![200]);
    assert_eq!((m2.next_hidden(), m2.next_desk(), m2.tickets[&200].halted), (HIDDEN + 6, DESK + 4, 2));
    // its next open takes 202, never 201
    let out = m2.apply(open(vec!["w1".into()]), "2026-11-20T22:10:00Z".into());
    assert!(out.reply.is_ok(), "{out:?}");
    assert!(out.lines.iter().any(|l| matches!(l, Line::Open { t: 202, .. })), "{:?}", out.lines);
    // coming forward: the whole record with that open after it replays, with no "exists already"
    let mut m3 = Machine::load(d);
    let forward: Vec<Line> = log.iter().cloned().chain(out.lines).collect();
    assert_eq!(m3.replay(&forward), Vec::<String>::new());
    assert_eq!(m3.tickets.range(..HIDDEN).map(|(t, _)| *t).collect::<Vec<_>>(), vec![200, 201, 202]);
}

/// The way back (proto-lead N-1446 p398, clause 1): a record from a later release opens 201 under 200, of a kind this
/// release does not have and whose program does not parse here (a word no release reads, as 0.3's `gather` is to 38:
/// not `gather` itself, which parses from E1b on; proto-reader N-1446 p515). The open is refused for
/// that, and the input on 201 after it is refused before it runs; 202, a kind this release has, opened at 201's open,
/// stays not loaded too. What those inputs did on 200, which this release loads, is taken as recorded: its two posts,
/// with their numbers and author (the record has no words of a post a program made), the first told to w1 as the
/// record tells it, the second to w2, whom its line says it names (proto-mind N-1446 p802), and 200 is not broken.
/// Its next open takes 203. A line of another kind on 200 from such an input (here 200 closed) is still a problem
/// line, and 200 is broken by it. With 200 slowed, the record halts that second post for his approval, and it is taken
/// so, its card numbered as recorded: due to nobody until he approves it, then to w2 (proto-reader N-1446 p947). With
/// a post that only informs w3 given before it in that input (shown, no card), the card holds only the post whose line
/// is just before it, and w3 is given the other at once (proto-reader N-1446 p1025); a card whose number this release's
/// count has passed is a problem line, and 200 is broken, never a card of another number (consult-3 N-1446 p1026).
#[test]
fn a_record_from_a_later_release_keeps_what_it_did_on_the_tickets_loaded_here() {
    const RESTS: &str = "kind rests\n  letter N\n  form to: Agent\n  result Fine\n  pattern\n    step to \"Rest.\" -> Fine\n";
    const LATER: &str = "kind later\n  letter L\n  form to: Agent\n  result Fine\n  pattern\n    p <- @@ a later release's word @@;\n    step to \"Go.\" -> Fine\n";
    let (d, errors) = defs_with(&[("rests.kind", RESTS)]);
    assert!(errors.is_empty(), "{errors:?}");
    let open = || Input::Open { kind: "rests".into(), under: None, opener: "owner".into(), form: json!({"to": "w1"}), text: None, names: vec!["w1".into()], told: vec![] };
    let mut m = Machine::load(d.clone());
    let out = m.apply(open(), "2026-11-20T22:00:01Z".into());
    assert!(out.reply.is_ok(), "{out:?}");
    let mut rec = out.lines.clone();
    let now = |s: u32| format!("2026-11-20T22:00:{s:02}Z");
    // the later release's open of 201 under 200, and what it recorded at it: 202 opened under 201, a post of 201's
    // on 200 and its telling
    let mut later = rec[0].clone();
    let Line::Open { at, t, kind, under, program, .. } = &mut later else { panic!("{rec:?}") };
    (*at, *t, *kind, *under, *program) = (now(2), 201, "later".into(), Some(200), LATER.into());
    rec.push(later);
    rec.push(Line::Opened { at: now(2), t: 202, kind: "rests".into(), parent: 201, program: RESTS.into(), calls: BTreeMap::new() });
    rec.push(Line::Posted { at: now(2), t: 200, n: 1, by: "L-201".into(), step: None, names: vec![], inform: vec![], into_turn: vec![] });
    rec.push(Line::Told { at: now(2), t: 200, n: 1, to: vec!["w1".into()], inform: vec![], joins: None });
    // an answer on 201, with another post of its on 200 that names w2, read as a release reads its record
    rec.push(Line::Answer { at: now(3), t: 201, by: "w1".into(), head: json!({"head": "Fine"}), text: Text::New("done".into()) });
    rec.push(serde_json::from_value(json!({"ev": "posted", "at": now(3), "t": 200, "n": 2, "by": "L-201", "names": ["w2"]})).unwrap());
    let mut m2 = Machine::load(d.clone());
    let problems = m2.replay(&rec);
    let open_at = rec.len() - 6 + 1;
    assert_eq!(problems, vec![format!("line {open_at}: the input is now refused: later is a kind this release does not load."), format!("line {}: the input is now refused: There is no ticket 201.", open_at + 4)]);
    assert_eq!(m2.not_loaded, std::collections::BTreeSet::from([201, 202]));
    assert_eq!(m2.tickets.range(..HIDDEN).map(|(t, _)| *t).collect::<Vec<_>>(), vec![200]);
    let x = &m2.tickets[&200];
    assert_eq!(x.broken, None);
    let posts: Vec<(u32, &str, Vec<String>)> = x.posts.iter().map(|p| (p.n, p.by.as_str(), p.told.clone())).collect();
    assert_eq!(posts, vec![(1, "L-201", vec!["w1".to_string()]), (2, "L-201", vec![])]);
    assert_eq!(x.posts[1].names, vec!["w2".to_string()], "it names whom its line names");
    assert!(m2.due("w2").iter().any(|d| d.t == 200 && d.id.ends_with(".p2")), "it is due to w2: {:?}", m2.due("w2"));
    let out = m2.apply(open(), "2026-11-20T22:10:00Z".into());
    assert!(out.lines.iter().any(|l| matches!(l, Line::Open { t: 203, .. })), "{:?}", out.lines);
    // a line on 200 that is neither a post, a telling nor a card, from the input on 201: a problem line, 200 broken
    let mut rec2 = rec.clone();
    rec2.push(Line::Closed { at: now(3), t: 200, result: Some(json!({"head": "Fine"})), raised: None });
    let mut m3 = Machine::load(d.clone());
    let problems = m3.replay(&rec2);
    assert!(problems.len() == 3 && problems[2].starts_with(&format!("line {}: the record has {{\"ev\":\"closed\"", rec2.len())), "{problems:?}");
    assert!(m3.tickets[&200].broken.as_ref().is_some_and(|b| *b == problems[2]), "{:?}", m3.tickets[&200].broken);
    // 200 slowed before 201 opens: the record halts post 2 (`halt_post`), its card 1 on 200
    let mut ms = Machine::load(d.clone());
    let mut slowed = ms.apply(open(), "2026-11-20T22:00:01Z".into()).lines;
    let out = ms.apply(Input::Slow { t: 200, on: true, by: "owner".into() }, "2026-11-20T22:00:01Z".into());
    assert!(out.reply.is_ok(), "{out:?}");
    slowed.extend(out.lines);
    let later_at = rec.iter().position(|l| matches!(l, Line::Open { t: 201, .. })).unwrap();
    slowed.extend(rec[later_at..].iter().cloned());
    let p2 = slowed.iter().position(|l| matches!(l, Line::Posted { t: 200, n: 2, .. })).unwrap();
    slowed.insert(p2 + 1, Line::Halted { at: now(3), t: 200, n: 1, what: "deliver".into() });
    let mut m4 = Machine::load(d.clone());
    let problems = m4.replay(&slowed);
    assert_eq!(problems.len(), 2, "{problems:?}");
    let x = &m4.tickets[&200];
    assert!(x.posts[1].halted && x.halts.iter().any(|h| h.n == 1 && matches!(h.what, Halting::Deliver { post: 2, .. })), "halted as recorded: {:?} {:?}", x.posts[1], x.halts);
    assert!(!m4.due("w2").iter().any(|d| d.t == 200), "due to nobody before he approves: {:?}", m4.due("w2"));
    let out = m4.apply(Input::Approve { t: 200, n: 1, by: "owner".into() }, "2026-11-20T22:10:00Z".into());
    assert!(out.reply.is_ok(), "{out:?}");
    assert!(m4.due("w2").iter().any(|d| d.t == 200 && d.id.ends_with(".p2")), "then to w2: {:?}", m4.due("w2"));
    // in that input, post 2 informs w3 alone (`halt_post` makes no card for a post that wakes no agent), then post 3
    // names w2, then its card
    let mut two = slowed.clone();
    let Line::Posted { n, .. } = &mut two[p2] else { panic!("{:?}", two[p2]) };
    *n = 3;
    two.insert(p2, Line::Posted { at: now(3), t: 200, n: 2, by: "L-201".into(), step: None, names: vec![], inform: vec!["w3".into()], into_turn: vec![] });
    let mut m5 = Machine::load(d.clone());
    let problems = m5.replay(&two);
    assert_eq!(problems.len(), 2, "{problems:?}");
    let x = &m5.tickets[&200];
    let cards: Vec<(u32, Halting)> = x.halts.iter().map(|h| (h.n, h.what.clone())).collect();
    assert_eq!(cards, vec![(1, Halting::Deliver { post: 3, with: vec![], edited: false, chose: None })], "{:?}", x.posts);
    assert!(!x.posts[1].halted && x.posts[2].halted, "only post 3 halted: {:?}", x.posts);
    assert!(m5.due("w3").iter().any(|d| d.t == 200 && d.id.ends_with(".p2")), "post 2 given to w3 at once: {:?}", m5.due("w3"));
    assert!(!m5.due("w2").iter().any(|d| d.t == 200), "post 3 due to nobody before he approves: {:?}", m5.due("w2"));
    // the record's input at 201's open made card 1 on 200 as well (here a notice's): its card 1 again is a problem,
    // and post 2 is halted all the same, on card 2, the one he can press here (consult-3 N-1446 p1047, proto-lead
    // p1049); the notice's card is a number only, as any `Halted` but a delivery's is on the way back
    let mut past = slowed.clone();
    let told1 = past.iter().position(|l| matches!(l, Line::Told { t: 200, n: 1, .. })).unwrap();
    past.insert(told1 + 1, Line::Halted { at: now(2), t: 200, n: 1, what: "notice".into() });
    let h = past.iter().position(|l| matches!(l, Line::Halted { what, .. } if what == "deliver")).unwrap();
    let mut m6 = Machine::load(d);
    let problems = m6.replay(&past);
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert_eq!(problems[2], format!("line {}: the record halts card 1 on 200 from an input on a ticket this release does not load; it is card 2 here", h + 1));
    let x = &m6.tickets[&200];
    assert_eq!(x.broken.as_ref(), Some(&problems[2]));
    let cards: Vec<(u32, Halting)> = x.halts.iter().map(|h| (h.n, h.what.clone())).collect();
    assert_eq!(cards, vec![(2, Halting::Deliver { post: 2, with: vec![], edited: false, chose: None })], "{:?}", x.posts);
    assert!(x.posts[1].halted, "post 2 halted: {:?}", x.posts[1]);
    assert!(!m6.due("w2").iter().any(|d| d.t == 200), "due to nobody before he approves: {:?}", m6.due("w2"));
    let out = m6.apply(Input::Approve { t: 200, n: 2, by: "owner".into() }, "2026-11-20T22:10:00Z".into());
    assert!(out.reply.is_ok(), "{out:?}");
    assert!(m6.due("w2").iter().any(|d| d.t == 200 && d.id.ends_with(".p2")), "then to w2: {:?}", m6.due("w2"));
}

/// The order the way back reads a delivery's card by (`as_recorded`'s `Halted` arm; proto-lead N-1446 p1044,
/// proto-reader p1039): on a slowed ticket, a program's post that wakes an agent writes its `halted` right after its
/// own `posted` line (`halt_post`), and one that only informs writes none, so that input's record is a `posted` that
/// wakes nobody, then the one that wakes, then its card. His burst's tell writes its card right after its `told` lines
/// (`tell_held`; gather.rs's burst test pins that). A writer that changes either fails here, and `as_recorded` is
/// extended with it. (The answer that runs the posts halts at its take first; his card lets it go.)
#[test]
fn a_halted_delivery_comes_right_after_the_lines_of_what_it_holds() {
    let k = "kind go_then_post\n  form w: Agent, l: Agent\n  result ()\n  pattern\n    g <- step l \"Go?\" -> Go;\n    _ <- post \"for your next turn\" informing w;\n    _ <- post \"for now\" naming l informing w;\n    return ()\n";
    let (d, errors) = defs_with(&[("go_then_post.kind", k)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    let out = r.go(Input::Open { kind: "go_then_post".into(), under: None, opener: "owner".into(), form: json!({"w": "fixer", "l": "lead"}), text: None, names: vec![], told: vec![] });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let t = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    r.go(Input::Slow { t, on: true, by: "owner".into() }).reply.unwrap();
    r.take("lead");
    let out = r.go(Input::Answer { t, by: "lead".into(), head: json!({"head": "Go"}), text: Text::New(String::new()) });
    assert_eq!(Run::halted_lines(&out), vec![(t, "take".to_string())], "the answer halts at its take first");
    // his card 1 lets the answer go: the program posts
    let out = r.approve(t, 1);
    // the program's two posts (the answer's own `posted` line, with its step, comes first)
    let posted: Vec<(usize, u32)> = out.lines.iter().enumerate().filter_map(|(i, l)| match l { Line::Posted { t: u, n, step: None, .. } if *u == t => Some((i, *n)), _ => None }).collect();
    let cards: Vec<usize> = out.lines.iter().enumerate().filter_map(|(i, l)| matches!(l, Line::Halted { what, .. } if what == "deliver").then_some(i)).collect();
    assert!(posted.len() == 2 && cards == vec![posted[1].0 + 1], "one card, right after the post that wakes: {:#?}", out.lines);
    let halts: Vec<Halting> = r.m.tickets[&t].halts.iter().map(|h| h.what.clone()).filter(|h| h.name() == "deliver").collect();
    assert_eq!(halts, vec![Halting::Deliver { post: posted[1].1, with: vec![], edited: false, chose: None }]);
}

/// A ticket a binding fired keeps its number where this machine refuses the event (consult-3 N-1446 p430): its number
/// is on the event alone (`fired`, no `Open` or `Opened` line), as 0.3's `on owner_to(e) open …` gives one. The replay
/// opens it from the program the event records, with or without the binding; here its form no longer fits that
/// program, so the event is refused, and the next open takes 201, never 200. An imported item's number is on its `Open`
/// (an `Import` writes one, with the number it was given): refused here (its recorded form no longer fits its program),
/// it keeps 300, and the next open takes 301.
#[test]
fn a_ticket_a_binding_fired_or_an_import_keeps_its_number_where_this_machine_refuses_it() {
    const RESTS: &str = "kind rests\n  letter N\n  form to: Agent\n  result Fine\n  pattern\n    step to \"Rest.\" -> Fine\n";
    let defs = |binds: &[(&str, &str)]| {
        let mut files = vec![("rests.kind", RESTS)];
        files.extend_from_slice(binds);
        let (mut d, errors) = defs_with(&files);
        assert!(errors.is_empty(), "{errors:?}");
        d.bindings.retain(|b| b.kind != "filing");
        d
    };
    let mut m = Machine::load(defs(&[("binds.kind", "on commit(c) open question(c.range)\n")]));
    let out = m.apply(Input::Event { name: "commit".into(), value: json!({"range": "a..b"}) }, "2026-11-21T00:00:01Z".into());
    assert_eq!(out.reply, Ok("Opened Q-200.".into()));
    assert!(out.lines.iter().any(|l| matches!(l, Line::Event { fired, .. } if fired.iter().map(|f| f.t).eq([200]))) && !out.lines.iter().any(|l| matches!(l, Line::Open { .. } | Line::Opened { .. })), "{:?}", out.lines);
    let mut fired = out.lines;
    for l in fired.iter_mut() {
        if let Line::Event { fired, .. } = l {
            fired[0].form = json!({});
        }
    }
    let out = m.apply(Input::Import { t: 300, kind: "rests".into(), opener: "owner".into(), form: json!({"to": "w1"}), text: None, closed: None }, "2026-11-21T00:00:02Z".into());
    assert!(out.reply.is_ok() && out.lines.iter().any(|l| matches!(l, Line::Open { t: 300, .. })), "{out:?}");
    let mut imported = out.lines;
    for l in imported.iter_mut() {
        if let Line::Open { t: 300, form, .. } = l {
            *form = json!({});
        }
    }
    let open = || Input::Open { kind: "rests".into(), under: None, opener: "owner".into(), form: json!({"to": "w1"}), text: None, names: vec!["w1".into()], told: vec![] };
    // the binding gone and the event refused: its ticket's number stays taken
    let mut m2 = Machine::load(defs(&[]));
    let problems = m2.replay(&fired);
    assert!(problems.len() == 1 && problems[0].starts_with("line 1: the input is now refused: The form of a question does not fit"), "{problems:?}");
    assert!(m2.tickets.is_empty());
    assert_eq!(m2.next_number(), 201);
    let out = m2.apply(open(), "2026-11-21T00:10:00Z".into());
    assert!(out.lines.iter().any(|l| matches!(l, Line::Open { t: 201, .. })), "{:?}", out.lines);
    // and the import refused: 300 stays taken
    let mut m3 = Machine::load(defs(&[]));
    let rec: Vec<Line> = fired.iter().chain(&imported).cloned().collect();
    let problems = m3.replay(&rec);
    assert!(problems.len() == 2 && problems[1].starts_with(&format!("line {}: the input is now refused: The form of a rests does not fit", fired.len() + 1)), "{problems:?}");
    assert_eq!(m3.next_number(), 301);
    let out = m3.apply(open(), "2026-11-21T00:10:00Z".into());
    assert!(out.lines.iter().any(|l| matches!(l, Line::Open { t: 301, .. })), "{:?}", out.lines);
}

/// The way back, clause 1's handler and the after-replay step (proto-lead N-1446 p431): a record from a later release
/// whose handler of his posts holds a program this release cannot parse (a word no release reads, as 0.3's `gather` is
/// to 38). His first post's handler is
/// not loaded: its open is that input's one allowed line, the route answer on it is refused as an input on a ticket not
/// loaded, its `Told` is taken as recorded, and his thread is not broken. His second post's handler had not told it
/// when the record ends: coordd sends it on (UNTOLD), and it goes to this release's own `owner_post`, its route due
/// to the watcher. His third, written while that handler ran, opened no handler of its own (0.3's `gather` took it into
/// that run): it is left untold too, and sent on so. Replayed again with those lines, nothing is left to send.
#[test]
fn his_post_whose_handler_this_release_cannot_parse_is_told_as_recorded_or_sent_on_after_the_replay() {
    let mut r = Run::new();
    let a = r.say("lead", &["fixer"], "the release");
    let from = r.log.len();
    let post = |r: &mut Run, text: &str| {
        let out = r.go(Input::Post { t: a, by: "owner".into(), text: Text::New(text.into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        r.m.tickets[&a].last()
    };
    let n1 = post(&mut r, "first: look at the lock");
    r.route(json!({"head": "To", "agents": ["lead"]}));
    let n2 = post(&mut r, "second: the brain is next");
    let n3 = post(&mut r, "and the window after it");
    // the later release's record: the handlers of his first two posts hold a program this release cannot parse, and
    // his third has no line of its own (the second's `gather` took it)
    let mut rec = r.log.clone();
    let third = rec.iter().rposition(|l| matches!(l, Line::Post { .. })).unwrap();
    rec.truncate(third + 1);
    let mut handlers = Vec::new();
    for l in rec[from..].iter_mut() {
        if let Line::Opened { t, kind, program, .. } = l {
            if kind == "owner_post" && *t >= HIDDEN {
                *program = format!("p <- @@ a later release's word @@;\n{program}");
                handlers.push(*t);
            }
        }
    }
    let at = |p: &dyn Fn(&Line) -> bool| rec.iter().position(|l| p(l)).unwrap() + 1;
    let mut m2 = Machine::load(defs());
    let problems = m2.replay(&rec);
    let ids: Vec<String> = handlers.iter().map(|h| m2.id(*h)).collect();
    assert_eq!(handlers.len(), 2, "{rec:?}");
    let routed = at(&|l: &Line| matches!(l, Line::Answer { t, .. } if *t == handlers[0]));
    assert!(problems.len() == 3, "{problems:#?}");
    assert!(problems[0].ends_with(&format!("the handler the record opens at it ({}) is not loaded: this release cannot parse its recorded program.", ids[0])), "{problems:#?}");
    assert_eq!(problems[1], format!("line {routed}: the input is now refused: There is no ticket {}.", handlers[0]));
    assert!(problems[2].ends_with(&format!("({}) is not loaded: this release cannot parse its recorded program.", ids[1])), "{problems:#?}");
    let x = &m2.tickets[&a];
    assert_eq!(x.broken, None);
    let p1 = x.posts.iter().find(|p| p.n == n1).unwrap();
    assert_eq!((p1.told.clone(), p1.routed), (vec!["lead".to_string()], true));
    assert!(handlers.iter().all(|c| !m2.tickets.contains_key(c) && m2.not_loaded.contains(c)));
    assert_eq!(m2.untold_posts(), vec![(a, n2), (a, n3)]);
    // coordd's first live moment: each goes to this release's own handler, whose route is the watcher's
    let mut again = rec.clone();
    let mut hs = Vec::new();
    for n in [n2, n3] {
        let out = m2.apply(Input::Event { name: UNTOLD.into(), value: json!({"t": a, "n": n}) }, "2026-11-10T12:10:00Z".into());
        assert!(out.reply.is_ok(), "{out:?}");
        let h = m2.tickets.values().find(|u| u.post_of.as_ref().is_some_and(|p| p.t == a && p.n == n)).map(|u| u.t).expect("a handler holds it");
        assert!(handlers.iter().all(|c| *c < h), "a number of its own, past the record's: {h} {handlers:?}");
        assert!(matches!(&m2.tickets[&h].status, Status::Awaits { agent, owed, .. } if agent == "watcher" && owed.call == "route"), "{:?}", m2.tickets[&h].status);
        again.extend(out.lines);
        hs.push(h);
    }
    assert!(m2.untold_posts().is_empty());
    assert_eq!(m2.due("watcher").len(), 2, "{:?}", m2.due("watcher"));
    // the record with those lines replays as before, and leaves nothing to send
    let mut m3 = Machine::load(defs());
    assert_eq!(m3.replay(&again), problems);
    assert!(m3.untold_posts().is_empty());
    for h in hs {
        assert!(matches!(&m3.tickets[&h].status, Status::Awaits { agent, .. } if agent == "watcher"), "{:?}", m3.tickets[&h].status);
    }
}
