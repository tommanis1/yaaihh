//! The 0.3 acceptance test: tonight's case, 20 Nov, replayed from research's own record (proto-reader, N-1446 p11).
//!
//! `tonight.jsonl` beside this file holds the record's lines, copied byte for byte (testdata/tonight.py; its first line
//! names the line ranges and their sha256). Each act runs twice:
//! - **then**: today's kinds, answered with the answers the record holds. It must go as the record went, which shows
//!   the test drives the machine as coordd did tonight;
//! - **0.3**: what the prototype must do on the same input, `#[ignore]`d until the head that does it. The answers there
//!   are made up, as every test's are; the checks are on what the program does with the decision, not on the model's
//!   judgment. What the decision is given (the thread's report, E1's THREAD and YOURS) coordd builds, so its checks
//!   are coordd's; each 0.3 test here names them.
//!
//! Act 1, N-1104 p586 and p587 (his two posts 22 s apart, 18:22:55Z and 18:23:17Z). Then: p586 was routed to
//! paper-research; p587 was routed Unclear, and he was asked whom his own post was for, which he pressed at 18:24:13Z.
//! N-1104 is to paper-research, but `route` is given only whom he typed it to (nobody), the agents on the thread and,
//! in coordd's RECENT, the last three posts with nothing on where p586 went (base.kind:49-52, machine.rs:2834-2837).
//!
//! Act 1b, N-1445 p3 (paper-research's case, N-1446 p6). N-1445 was opened to paper-research-brain; his first words
//! there went to paper-research, the brain only informed (record line 103418). `route` was given `to: []` again.

use super::Run;
use crate::*;
use serde_json::{Value, json};

const TONIGHT: &str = include_str!("tonight.jsonl");

/// The record's lines in tonight.jsonl, after its head line.
fn record() -> Vec<Value> {
    TONIGHT.lines().skip(1).map(|l| serde_json::from_str(l).unwrap()).collect()
}

/// The one recorded line F picks.
fn the(f: impl Fn(&Value) -> bool) -> Value {
    let found: Vec<Value> = record().into_iter().filter(|d| f(d)).collect();
    assert_eq!(found.len(), 1, "{found:#?}");
    found.into_iter().next().unwrap()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().map(|xs| xs.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

fn at_of(d: &Value) -> String {
    d["at"].as_str().unwrap().to_string()
}

/// A time on 20 Nov, S seconds after 13:00Z (after the record's era line, before tonight): for the posts that stand in
/// for N-1104's p2-p585.
fn stand_in_at(s: u32) -> String {
    format!("2026-11-20T{:02}:{:02}:{:02}Z", 13 + s / 3600, s / 60 % 60, s % 60)
}

/// His recorded post on ticket T (the record's number), the I-th of his there in tonight.jsonl.
fn his_recorded(t: u64, i: usize) -> Value {
    record().into_iter().filter(|d| d["ev"] == "post" && d["t"] == json!(t) && d["by"] == "owner").nth(i).unwrap()
}

/// The recorded `told` line of post N of ticket T (the record's number): to, inform.
fn recorded_told(t: u64, n: u32) -> (Vec<String>, Vec<String>) {
    let d = the(|d| d["ev"] == "told" && d["t"] == json!(t) && d["n"] == json!(n));
    (strings(&d["to"]), strings(&d["inform"]))
}

/// The recorded `step_args` line of the `route` step deciding post N of ticket T (the record's number): the one with
/// route's `others` (the `clarify` step after it names the same post).
fn recorded_route(t: u64, n: u32) -> Value {
    the(|d| d["ev"] == "step_args" && d["args"]["text"]["post"] == json!({"n": n, "t": t}) && d["args"].get("others").is_some())
}

/// The recorded arguments of the `route` step deciding post N of ticket T (the record's number).
fn recorded_route_args(t: u64, n: u32) -> Value {
    recorded_route(t, n)["args"].clone()
}

/// The recorded answer of the step run R owed (R the record's handler number) by WHO.
fn recorded_answer(run: u64, who: &str) -> Value {
    the(|d| d["ev"] == "answer" && d["t"] == json!(run) && d["by"] == who)["head"].clone()
}

/// The record's number of the handler that decided post N of ticket T.
fn recorded_run(t: u64, n: u32) -> u64 {
    recorded_route(t, n)["t"].as_u64().unwrap()
}

impl Run {
    /// Every step owed for an agent's post (`asks`) answered Nothing at AT, and the minder of T, if asked, filled by
    /// nobody: T rests as it was. The stand-in posts' upkeep; tonight's acts have none of it.
    fn quiet_at(&mut self, t: u64, at: &str) {
        for (h, _) in self.converting() {
            let out = self.at(Input::Answer { t: h, by: "watcher".into(), head: json!({"head": "Nothing"}), text: Text::New(String::new()) }, at);
            assert!(out.reply.is_ok(), "{:?}", out.reply);
        }
        if let Status::Asking { owed, .. } = &self.m.tickets[&t].status {
            let step = owed.step;
            self.at(Input::Decided { t, step, question: "fills".into(), answer: Value::Null, provenance: json!({}) }, at).reply.unwrap();
        }
    }

    /// N-1104 as it stood before p586: opened by his words to paper-research as recorded (act 0), in the record's era
    /// (act 0e), the ten other agents brought on by paper-research's p2, and its posts up to p585 by paper-research.
    /// p2-p585 are not in tonight.jsonl; they stand in so that his posts have their recorded numbers. Its number here.
    fn n1104() -> (Run, u64) {
        Run::n1104_in(Run::new())
    }

    /// [`Run::n1104`] in R, a run with the acts before it (insight (3)'s i-kept).
    fn n1104_in(mut r: Run) -> (Run, u64) {
        // the message that opened N-1104 (insight (3)'s acts hold two more of his: N-294's, N-1432's)
        let m = the(|d| d["ev"] == "event" && d["name"] == "owner_message" && d["fired"].as_array().is_some_and(|f| f.iter().any(|f| f["t"] == json!(1104))));
        let out = r.at(Input::Event { name: "owner_message".into(), value: m["value"].clone() }, &at_of(&m));
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        let t = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
        assert_eq!(r.told(t, 1), recorded_told(1104, 1), "his first words went as recorded");
        let e = the(|d| d["ev"] == "event" && d["name"] == ERA);
        r.at(Input::Event { name: ERA.into(), value: e["value"].clone() }, &at_of(&e)).reply.unwrap();
        let others = strings(&recorded_route_args(1104, 586)["others"]);
        assert_eq!(others[0], "paper-research");
        let ten = others[1..].to_vec();
        let at = stand_in_at(2);
        let text = format!("{} (stands in for N-1104 p2-p585, not in tonight.jsonl)", ten.iter().map(|a| format!("@{a}")).collect::<Vec<_>>().join(" "));
        r.at(Input::Post { t, by: "paper-research".into(), text: Text::New(text), names: ten, told: vec![], inform: vec![], answers: None, kind: None }, &at).reply.unwrap();
        r.quiet_at(t, &at);
        for n in 3..=585 {
            let at = stand_in_at(n);
            let inform: Vec<String> = r.m.participants(t).into_iter().filter(|a| a != "owner" && a != "paper-research").collect();
            r.at(Input::Post { t, by: "paper-research".into(), text: Text::New(format!("p{n}")), names: vec![], told: vec![], inform, answers: None, kind: None }, &at).reply.unwrap();
            r.quiet_at(t, &at);
        }
        assert_eq!(r.m.tickets[&t].last(), 585);
        assert_eq!(r.m.participants(t).into_iter().filter(|a| a != "owner").collect::<Vec<_>>(), others, "the agents on N-1104 as recorded");
        assert!(r.handler().is_none());
        (r, t)
    }

    /// His I-th recorded post on ticket REC (the record's number), posted on T here at its recorded time: its number.
    fn his(&mut self, t: u64, rec: u64, i: usize) -> u32 {
        let d = his_recorded(rec, i);
        let out = self.at(Input::Post { t, by: "owner".into(), text: Text::New(d["text"].as_str().unwrap().into()), names: strings(&d["names"]), told: vec![], inform: vec![], answers: None, kind: None }, &at_of(&d));
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        self.m.tickets[&t].last()
    }

    /// The `told` line of post N of T here: to, inform.
    fn told(&self, t: u64, n: u32) -> (Vec<String>, Vec<String>) {
        let found: Vec<(Vec<String>, Vec<String>)> = self.log.iter().filter_map(|l| match l {
            Line::Told { t: u, n: m, to, inform, .. } if *u == t && *m == n => Some((to.clone(), inform.clone())),
            _ => None,
        }).collect();
        assert_eq!(found.len(), 1, "post {n} of {}: {found:?}", self.m.id(t));
        found.into_iter().next().unwrap()
    }

    /// The arguments of the last step written for a decision on post N of T: whatever decides it (`route` today, the
    /// thread's holder in 0.3), the newest `step_args` line that refers to that post.
    fn deciding(&self, t: u64, n: u32) -> Value {
        self.log.iter().rev().find_map(|l| match l {
            Line::StepArgs { args, .. } if args.to_string().contains(&json!({"post": {"n": n, "t": t}}).to_string()) || args.to_string().contains(&json!({"post": {"t": t, "n": n}}).to_string()) => Some(args.clone()),
            _ => None,
        }).unwrap_or_else(|| panic!("no step was written for a decision on post {n} of {}", self.m.id(t)))
    }

    /// What became of post N of T, in words, for a failed check: held by whom, told to whom, and the state of the run
    /// that decides it.
    fn fate(&self, t: u64, n: u32) -> String {
        let p = self.m.tickets[&t].posts.iter().find(|p| p.n == n).unwrap();
        let runs: Vec<String> = self.m.tickets.values().filter(|x| x.post_of.as_ref().is_some_and(|q| q.t == t && q.n == n)).map(|x| format!("{} {:?}", self.m.id(x.t), x.status)).collect();
        format!("post {n} of {}: held {:?}, routed {}, told {:?}; its runs: {runs:?}", self.m.id(t), p.held, p.routed, p.told)
    }

    /// The step the run deciding post N of T owes now: who owes it and its call.
    fn owed_for(&self, t: u64, n: u32) -> Option<(String, String)> {
        let h = self.m.tickets.values().find(|x| x.is_open() && x.post_of.as_ref().is_some_and(|p| p.t == t && p.n == n))?;
        match &h.status {
            Status::Awaits { agent, owed, .. } => Some((agent.clone(), owed.call.clone())),
            _ => None,
        }
    }

    /// The run deciding post N of T answered by WHO with the record's answer of run REC at AT.
    fn answer_as_recorded(&mut self, t: u64, n: u32, rec: u64, who: &str, at: &str) {
        let h = self.handler_of(t, n);
        let out = self.at(Input::Answer { t: h, by: who.into(), head: recorded_answer(rec, who), text: Text::New(String::new()) }, at);
        assert!(out.reply.is_ok(), "{:?}", out.reply);
    }

    /// The record's `given` lines of post REC_N of ticket REC (the record's number) to TO, played as post N of T here at
    /// their times: the delivery, and the line after it with the place the transcript showed it at (`where`, the same
    /// delivery seen again); `into` as recorded.
    fn given_as_recorded(&mut self, t: u64, rec: u64, rec_n: u32, n: u32, to: &str) {
        let given: Vec<Value> = record().into_iter().filter(|d| d["ev"] == "given" && d["t"] == json!(rec) && d["to"] == to && d["delivery"].as_str().is_some_and(|w| w.ends_with(&format!(".p{rec_n}")))).collect();
        assert!(!given.is_empty(), "{to} was given p{rec_n} of {rec} in the record");
        let delivery = format!("{}.p{n}", self.m.id(t));
        for d in given {
            let (to, delivery, place) = (to.to_string(), delivery.clone(), d.get("where").map(|w| serde_json::from_value(w.clone()).unwrap()));
            let input = if d["into"] == true { Input::GivenInto { t, to, delivery, upto: n, place, skipped: Default::default() } } else { Input::Given { t, to, delivery, upto: n, place, skipped: Default::default() } };
            let out = self.at(input, &at_of(&d));
            assert!(out.reply.is_ok(), "{:?}", out.reply);
        }
    }

    /// Act i-kept, his N-294 (16 Nov) as recorded: his message, the watcher's Thread, his words given to paper-research,
    /// its turn ending with its reply p2 naming him (tonight.jsonl holds the `replied` line, not its words), the run on p2 (the record's …965, kept since
    /// reader-ins3b) given its watcher and answered Nothing as written, the minder filled and handed the step, and its
    /// Close(Kept); the runs on his post (…964, run at once as his words went) and on p2 opened and closed when the
    /// record says. Its number here.
    fn n294(&mut self) -> u64 {
        let m = the(|d| d["ev"] == "event" && d["name"] == "owner_message" && d["fired"].as_array().is_some_and(|f| f.iter().any(|f| f["t"] == json!(294))));
        let out = self.at(Input::Event { name: "owner_message".into(), value: m["value"].clone() }, &at_of(&m));
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        let t = *self.m.tickets.range(..HIDDEN).next_back().unwrap().0;
        let a = the(|d| d["ev"] == "answer" && d["t"] == json!(294) && d["by"] == "watcher");
        self.at(Input::Answer { t, by: "watcher".into(), head: a["head"].clone(), text: Text::New(String::new()) }, &at_of(&a)).reply.unwrap();
        assert_eq!(self.told(t, 1), recorded_told(294, 1), "his words went as recorded");
        self.given_as_recorded(t, 294, 1, 1, "paper-research");
        // its turn on his words ends with its reply, p2 (rule Reply): tonight.jsonl holds the `replied` line, not its words
        let p = the(|d| d["ev"] == "replied" && d["t"] == json!(294));
        let out = self.at(Input::TurnEnded { agent: "paper-research".into(), delivery: Some(format!("{}.p1", self.m.id(t))), text: Run::loc(2), later: vec![], stood: None, go: None }, &at_of(&p));
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        let reply = self.m.tickets[&t].posts.last().unwrap();
        assert_eq!((reply.n, reply.by.as_str(), reply.names.clone()), (2, "paper-research", strings(&p["names"])), "its reply as recorded");
        let run = |kind: &str| the(|d| d["ev"] == "opened" && d["parent"] == json!(294) && d["kind"] == kind);
        let rec = run("agent_post")["t"].clone();
        let (h, _) = *self.converting().iter().find(|(_, n)| *n == 2).expect("p2 is being converted");
        let g = the(|d| d["ev"] == "given" && d["t"] == rec);
        let due = self.m.due("watcher").into_iter().find(|d| d.t == h).expect("the watcher's asks step on p2 is due");
        self.at(Input::Given { t: h, to: "watcher".into(), delivery: due.id, upto: due.upto, place: None, skipped: Default::default() }, &at_of(&g)).reply.unwrap();
        let a = the(|d| d["ev"] == "answer" && d["t"] == rec);
        self.at(Input::Answer { t: h, by: "watcher".into(), head: a["head"].clone(), text: Text::New(String::new()) }, &at_of(&a)).reply.unwrap();
        for kind in ["owner_post", "agent_post"] {
            let mine: Vec<&Ticket> = self.m.tickets.values().filter(|x| x.under == Some(t) && x.handling.as_deref() == Some(kind)).collect();
            let (o, [x]) = (run(kind), mine.as_slice()) else { panic!("one {kind} run under N-294: {}", mine.len()) };
            let c = the(|d| d["ev"] == "closed" && d["t"] == o["t"]);
            assert_eq!((Some(x.opened_at.as_str()), x.closed_at.as_deref()), (o["at"].as_str(), c["at"].as_str()), "the {kind} run as recorded");
        }
        let Status::Asking { owed, .. } = &self.m.tickets[&t].status else { panic!("{:?}", self.m.tickets[&t].status) };
        let step = owed.step;
        let f = the(|d| d["ev"] == "decided" && d["t"] == json!(294) && d["question"] == "fills");
        self.at(Input::Decided { t, step, question: "fills".into(), answer: f["answer"].clone(), provenance: json!({}) }, &at_of(&f)).reply.unwrap();
        let g = the(|d| d["ev"] == "given" && d["t"] == json!(294) && d["to"] == "minder");
        let due = self.m.due("minder").into_iter().find(|d| d.t == t).expect("the minder's step is due");
        self.at(Input::Given { t, to: "minder".into(), delivery: due.id, upto: due.upto, place: None, skipped: Default::default() }, &at_of(&g)).reply.unwrap();
        let k = the(|d| d["ev"] == "answer" && d["t"] == json!(294) && d["by"] == "minder");
        self.at(Input::Answer { t, by: "minder".into(), head: k["head"].clone(), text: Text::New(String::new()) }, &at_of(&k)).reply.unwrap();
        let c = the(|d| d["ev"] == "closed" && d["t"] == json!(294));
        assert_eq!(self.m.tickets[&t].closed_at.as_deref(), c["at"].as_str(), "closed when recorded");
        assert!(matches!(&self.m.tickets[&t].status, Status::Done(v) if *v == c["result"]), "closed as recorded: {:?}", self.m.tickets[&t].status);
        t
    }

    /// Act i-moved, his N-1432 (20 Nov) as recorded: his message, and the watcher's Place on N-1104, here T. Its number
    /// here.
    fn n1432(&mut self, t: u64) -> u64 {
        let m = the(|d| d["ev"] == "event" && d["name"] == "owner_message" && d["fired"].as_array().is_some_and(|f| f.iter().any(|f| f["t"] == json!(1432))));
        let out = self.at(Input::Event { name: "owner_message".into(), value: m["value"].clone() }, &at_of(&m));
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        let x = *self.m.tickets.range(..HIDDEN).next_back().unwrap().0;
        let a = the(|d| d["ev"] == "answer" && d["t"] == json!(1432) && d["by"] == "watcher");
        assert_eq!(a["head"]["ticket"], "N-1104");
        let mut head = a["head"].clone();
        head["ticket"] = json!(self.m.id(t));
        self.at(Input::Answer { t: x, by: "watcher".into(), head, text: Text::New(String::new()) }, &at_of(&a)).reply.unwrap();
        assert!(matches!(&self.m.tickets[&x].status, Status::Done(v) if v["head"] == "Moved"), "{:?}", self.m.tickets[&x].status);
        x
    }

    /// Act i-agents, N-1443 (20 Nov) opened as recorded by paper-research to consult-3, an agents' thread: its number
    /// here.
    fn n1443(&mut self) -> u64 {
        let d = the(|d| d["ev"] == "open" && d["t"] == 1443);
        assert!(d.get("under").is_none_or(Value::is_null));
        let out = self.at(Input::Open { kind: "thread".into(), under: None, opener: d["opener"].as_str().unwrap().into(), form: d["form"].clone(), text: d["text"].as_str().map(|s| Text::New(s.into())), names: strings(&d["names"]), told: strings(&d["told"]) }, &at_of(&d));
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        let x = *self.m.tickets.range(..HIDDEN).next_back().unwrap().0;
        assert!(self.m.tickets[&x].between(), "closed Between as it opened, as recorded");
        x
    }

    /// N-1445 opened under T as recorded, by paper-research, to paper-research-brain: its number here.
    fn n1445(&mut self, t: u64) -> u64 {
        let d = the(|d| d["ev"] == "open" && d["t"] == 1445);
        let out = self.at(Input::Open { kind: "thread".into(), under: Some(t), opener: d["opener"].as_str().unwrap().into(), form: d["form"].clone(), text: d["text"].as_str().map(|s| Text::New(s.into())), names: strings(&d["names"]), told: vec![] }, &at_of(&d));
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        *self.m.tickets.range(..HIDDEN).next_back().unwrap().0
    }
}

/// Act 1, then: p586 to paper-research by `route`, p587 Unclear and his press at 18:24:13Z, each told as recorded.
#[test]
fn tonight_then_his_p586_and_p587_go_as_recorded() {
    let (mut r, t) = Run::n1104();
    // p586, 18:22:55Z: typed to nobody, ten others on the thread, so the watcher's `route`, given what it was given
    assert_eq!(r.his(t, 1104, 0), 586);
    let mut args = recorded_route_args(1104, 586);
    args["text"]["post"]["t"] = json!(t);
    assert_eq!(r.deciding(t, 586), args);
    assert_eq!(r.owed_for(t, 586), Some(("watcher".into(), "route".into())));
    r.answer_as_recorded(t, 586, recorded_run(1104, 586), "watcher", "2026-11-20T18:23:00Z");
    assert_eq!(r.told(t, 586), recorded_told(1104, 586));
    // p587, 18:23:17Z: the same call, given the same, answered Unclear; then he is asked whom his own post is for
    assert_eq!(r.his(t, 1104, 1), 587);
    let mut args = recorded_route_args(1104, 587);
    args["text"]["post"]["t"] = json!(t);
    assert_eq!(r.deciding(t, 587), args);
    let run = recorded_run(1104, 587);
    r.answer_as_recorded(t, 587, run, "watcher", "2026-11-20T18:23:20Z");
    assert_eq!(r.owed_for(t, 587), Some(("owner".into(), "clarify".into())), "the press he was asked for");
    r.answer_as_recorded(t, 587, run, "owner", "2026-11-20T18:24:13Z");
    assert_eq!(r.told(t, 587), recorded_told(1104, 587));
    r.replays();
}

/// Act 1b, then: N-1445 opened to paper-research-brain; his first words there (the record's p3), typed to nobody,
/// routed to paper-research as recorded (line 103418), the brain only informed. The record posted the opening's words
/// again as his (#2, line 103219); since hs-ls's 435d5075 they stand once, as paper-research's, so his first words
/// are #2 here.
#[test]
fn tonight_then_his_first_words_on_n1445_go_to_paper_research() {
    let (mut r, t) = Run::n1104();
    let b = r.n1445(t);
    assert_eq!(recorded_told(1445, 2), (vec!["paper-research-brain".to_string()], vec!["paper-research".to_string()]), "the record's #2: the opening's words, posted as his");
    let posts: Vec<(u32, String, Vec<String>)> = r.m.tickets[&b].posts.iter().map(|p| (p.n, p.by.clone(), p.names.clone())).collect();
    assert_eq!(posts, vec![(1, "paper-research".to_string(), vec!["paper-research-brain".to_string()])], "the opening's words, once, as paper-research's");
    assert_eq!(r.his(b, 1445, 0), 2);
    let mut args = recorded_route_args(1445, 3);
    args["text"]["post"] = json!({"n": 2, "t": b});
    assert_eq!(r.deciding(b, 2), args);
    r.answer_as_recorded(b, 2, recorded_run(1445, 3), "watcher", "2026-11-20T19:09:24Z");
    assert_eq!(r.told(b, 2), recorded_told(1445, 3));
    assert_eq!(recorded_told(1445, 3), (vec!["paper-research".to_string()], vec!["paper-research-brain".to_string()]));
    r.replays();
}

/// Act 1, 0.3 (A1.2): the holder's form for p587, To paper-research joining p586 (route.call's `To(agents, joins: Int?)`,
/// E1 21c4a12c), told to whom his press told it then, 53 s sooner, with p586 kept on it (`tell … joining`, hs-ls
/// cede9c99), and nothing asks him. The form is made up. What the holder is given to decide it ("#586 … went to
/// paper-research (given 18:23:01Z)") is coordd's report, checked in coordd's tests on the record's words
/// (`clerk::the_holder_of_n1104_sees_p586_where_it_went_when_p587_comes`); whether the holder decides so is the sol
/// eval's, with his yes (proto-lead N-1446 p18), not a test's.
#[test]
fn tonight_03_the_holder_joins_p587_to_p586_and_he_owes_nothing() {
    let (mut r, t) = Run::n1104();
    r.his(t, 1104, 0);
    r.answer_as_recorded(t, 586, recorded_run(1104, 586), "watcher", "2026-11-20T18:23:00Z");
    assert_eq!(r.his(t, 1104, 1), 587);
    let h = r.handler_of(t, 587);
    // 18:23:20Z, when route answered p587 then
    let out = r.at(Input::Answer { t: h, by: "watcher".into(), head: json!({"head": "To", "agents": ["paper-research"], "joins": 586}), text: Text::New(String::new()) }, "2026-11-20T18:23:20Z");
    assert!(out.reply.is_ok(), "the holder's form is refused: {:?}", out.reply);
    assert!(r.owed_for(t, 587).is_none(), "he is asked: {}", r.fate(t, 587));
    assert!(!r.log.iter().any(|l| matches!(l, Line::Step { call, .. } if call == "clarify")), "a question to him was written");
    assert_eq!(r.told(t, 587), recorded_told(1104, 587), "whom his press told it then");
    let told: Vec<Value> = r.log.iter().filter(|l| matches!(l, Line::Told { t: u, n: 587, .. } if *u == t)).map(|l| serde_json::to_value(l).unwrap()).collect();
    assert_eq!(told[0]["joins"], json!(586), "the Told line: {}", told[0]);
    let post = serde_json::to_value(r.post(t, 587)).unwrap();
    assert_eq!(post["joins"], json!(586), "the post: {post}");
    r.replays();
}

/// Act 1b, 0.3: N-1445, opened by paper-research for paper-research-brain, holds no post of his before his own first
/// words. Then thread.kind:66 posted the opener's words again as his (record line 103219, `posted` n 2 by owner), so
/// the holder's YOURS and the brain's HIS would read paper-research's words as his (proto-reader N-1446 p71, proto-mind
/// p79); the fix is hs-ls's (proto-lead p74.1). What the holder is given for p3 (THREAD "opened for paper-research-
/// brain", rule 5) is coordd's report (`clerk::the_holder_of_n1445_knows_whom_the_thread_is_for`); the decision is the
/// sol eval's.
#[test]
fn tonight_03_n1445_holds_no_post_of_his_before_his_first_words() {
    let (mut r, t) = Run::n1104();
    let b = r.n1445(t);
    let by = |r: &Run, who: &str| -> Vec<u32> { r.m.tickets[&b].posts.iter().filter(|p| p.by == who).map(|p| p.n).collect() };
    assert!(by(&r, "owner").is_empty(), "posts by owner on N-1445 before he wrote there: {:?}", by(&r, "owner"));
    assert!(!by(&r, "paper-research").is_empty(), "the opener's words are on it as paper-research's");
    let n = r.his(b, 1445, 0);
    assert_eq!(by(&r, "owner"), vec![n], "his first words are his only post there");
    r.replays();
}

// A1.7, a failed decision (three answers out of shape): his post goes To the thread's `to`, else where his last post
// went, and Unclear only when neither exists (proto-lead N-1446 p57.2). It is coordd's own failure answer
// (clerk::route_failed, reached from route_job; proto-engine p81), so the machine is given a To and its
// Unfit path is not the route's: the tests are coordd's, on the record's two cases (N-1104 p587 -> paper-research,
// N-1445 p3 -> paper-research-brain) and a thread with neither.

/// S6, insight (3) open → done (proto-lead N-1446 p388; what counts, p473, 2 and p499, 1) over acts i-kept, 0, 0e,
/// i-moved, i-agents and 1b, in the record's order: its numbers are the record's own count in tonight.jsonl at its
/// last line. A line of the outline is opened by an `open` line of a thread or an item (an agent's when its opener is
/// not the owner), or by his message (`owner_message`) that `fired` one (his); one whose `closed` line says `Moved` is
/// not counted; it ended at its last `closed` line that is not Between, or its `ended` line; open for days is open a
/// day or more at the last line. Tonight: his N-294 ended Kept after 72 s and N-1104 open for days (opened 2, ended 1,
/// 50%, median 72 s, open 1); the agents' N-1443 and N-1445, both open; N-1432 moved, not counted.
#[test]
fn tonight_insight_open_to_done_is_the_records_own_count() {
    let mut r = Run::before_asks();
    let kept = r.n294();
    let (mut r, t) = Run::n1104_in(r);
    let moved = r.n1432(t);
    let sent = r.n1443();
    let b = r.n1445(t);
    let lines = record();
    let last = lines.iter().filter_map(|d| d["at"].as_str()).max().unwrap().to_string();
    let now = crate::machine::secs(&last).unwrap();
    let secs = |at: &str| crate::machine::secs(at).unwrap();
    // the record's count: each line of the outline opened, (its number, when, whether an agent opened it)
    let lined = |d: &Value| d["kind"] == "thread" || d["kind"] == "item";
    let mut opened: Vec<(u64, String, bool)> = lines.iter().filter(|d| d["ev"] == "open" && lined(d)).map(|d| (d["t"].as_u64().unwrap(), at_of(d), d["opener"].as_str().is_some_and(|o| o != "owner"))).collect();
    for d in lines.iter().filter(|d| d["fired"].as_array().is_some_and(|f| f.iter().any(lined))) {
        assert_eq!(d["name"], "owner_message", "tonight a line of the outline is fired by his message only");
        opened.extend(d["fired"].as_array().unwrap().iter().filter(|f| lined(f)).map(|f| (f["t"].as_u64().unwrap(), at_of(d), false)));
    }
    let closed = |t: u64, how: &dyn Fn(&Value) -> bool| lines.iter().filter(|d| d["ev"] == "closed" && d["t"] == json!(t) && how(&d["result"]["head"])).map(at_of).last();
    opened.retain(|(t, _, _)| closed(*t, &|h| h == "Moved").is_none());
    let end = |t: u64| closed(t, &|h| h != "Between").or_else(|| lines.iter().filter(|d| d["ev"] == "ended" && d["t"] == json!(t)).map(at_of).last());
    // the newest first; the ended with their seconds; those open for days
    let count = |agents: bool| {
        let mut mine: Vec<&(u64, String, bool)> = opened.iter().filter(|o| o.2 == agents).collect();
        mine.sort_by(|a, b| b.1.cmp(&a.1));
        let ended: Vec<(u64, i64)> = mine.iter().filter_map(|(t, at, _)| end(*t).map(|e| (*t, secs(&e) - secs(at)))).collect();
        let days: Vec<u64> = mine.iter().filter(|(t, at, _)| end(*t).is_none() && now - secs(at) >= crate::insight::DAY).map(|(t, _, _)| *t).collect();
        (mine.iter().map(|(t, _, _)| *t).collect::<Vec<_>>(), ended, days)
    };
    let (his, agents) = (count(false), count(true));
    assert_eq!(his, (vec![1104, 294], vec![(294, 72)], vec![1104]), "his, the record's count");
    assert_eq!(agents, (vec![1445, 1443], vec![], vec![]), "the agents', the record's count");
    // the insight, over the machine as these acts leave it
    let here: std::collections::BTreeMap<u64, String> = [(294, kept), (1104, t), (1432, moved), (1443, sent), (1445, b)].into_iter().map(|(n, x)| (n, r.m.id(x))).collect();
    let rows = crate::insight::threads(&r.m);
    assert!(rows.iter().all(|x| x.id != here[&1432]), "N-1432 moved: not counted");
    let n = crate::insight::open_to_done(&rows, now);
    let ids = |xs: &[(&crate::insight::Thread, Option<i64>)]| xs.iter().map(|(x, _)| x.id.clone()).collect::<Vec<_>>();
    let named = |ns: &[u64]| ns.iter().map(|n| here[n].clone()).collect::<Vec<_>>();
    assert_eq!(ids(&n.his.opened), named(&his.0), "his opened, the newest first");
    assert_eq!(n.his.ended.iter().map(|(x, s)| (x.id.clone(), *s)).collect::<Vec<_>>(), his.1.iter().map(|(t, s)| (here[t].clone(), Some(*s))).collect::<Vec<_>>(), "his ended, with the seconds it took");
    assert_eq!((n.his.rate(), n.his.median), (Some(0.5), Some(72)));
    assert_eq!(n.his.ended[0].0.ended, end(294), "N-294 ended when the record closed it");
    assert_eq!(ids(&n.his.open), named(&[1104]));
    assert_eq!(n.his.days.iter().map(|(x, _)| x.id.clone()).collect::<Vec<_>>(), named(&his.2), "N-1104 open for days");
    assert_eq!(n.his.days[0].0.opened, opened.iter().find(|(o, _, _)| *o == 1104).unwrap().1, "opened when the record says");
    assert_eq!((ids(&n.agents.opened), n.agents.ended.len(), n.agents.days.len()), (named(&agents.0), agents.1.len(), agents.2.len()), "the agents' row");
    assert_eq!(ids(&n.agents.open), named(&[1443, 1445]), "both open, the longest open first");
    assert!(n.agents.opened.iter().all(|(x, _)| x.by.as_deref() == Some("paper-research")), "paper-research opened both");
}

/// S6, insight (1) work and talk (proto-lead N-1446 p388) over tonight.jsonl: its numbers are the record's own count.
/// A turn is a `turn_ended` line; it worked when its `tools` name one not of coord's (its MCP tools, `mcp__coord__…`
/// and `coord__…`, and a shell's call running coord alone, `Bash(coord)`), talked when they name only coord's or its
/// `model` is named, and what it did is not known when it names neither.
/// Tonight: paper-research's two turns (N-1104.p586, p587), written before turns carried their tools: both not known.
#[test]
fn tonight_insight_work_talk_is_the_records_own_count() {
    // the record's count, from its lines as JSON
    let ended: Vec<(u64, Value)> = TONIGHT.lines().enumerate().filter_map(|(i, l)| serde_json::from_str::<Value>(l).ok().map(|v| (i as u64 + 1, v))).filter(|(_, v)| v["ev"] == "turn_ended").collect();
    let coords = |t: &str| t.starts_with("mcp__coord__") || t.starts_with("coord__") || t.ends_with("(coord)");
    // work, talk, not known
    let mut count = [0usize; 3];
    for (_, v) in &ended {
        let tools: Vec<&String> = v["tools"].as_object().map(|o| o.keys().collect()).unwrap_or_default();
        let i = if tools.iter().any(|t| !coords(t)) { 0 } else if !tools.is_empty() || v["model"].is_string() { 1 } else { 2 };
        count[i] += 1;
    }
    assert_eq!((ended.len(), count), (2, [0, 0, 2]), "the record's count");
    // the insight, over the same lines as coordd reads them (`insight_turns`)
    let turns: Vec<crate::insight::Turn> = TONIGHT.lines().enumerate().filter_map(|(i, l)| crate::insight::turn(&serde_json::from_str::<Line>(l).ok()?, i as u64 + 1)).collect();
    let n = crate::insight::work_talk(&turns);
    assert_eq!([n.all.work.len(), n.all.talk.len(), n.all.not_known.len()], count);
    assert_eq!(n.agents.iter().map(|t| (t.label.as_str(), t.len())).collect::<Vec<_>>(), vec![("paper-research", 2)]);
    assert_eq!(n.all.not_known.iter().map(|x| (x.line, x.delivery.as_deref())).collect::<Vec<_>>(), ended.iter().rev().map(|(i, v)| (*i, v["delivery"].as_str())).collect::<Vec<_>>(), "each at its line, the newest first");
}

/// S6, insight (2) agents per topic (proto-lead N-1446 p653) over acts i-kept, 0, 0e, 1, i-moved, i-agents and 1b, each
/// line's givings played as recorded: its numbers are the record's own count in tonight.jsonl. A line's agents are those
/// a `given` line names on it or on a run whose `opened` line's `parent` leads to it, but coordd's own watcher and
/// minder, and those a `started` line names on it (an item's); a thread's topic is it with every line whose `open`
/// line's `under` leads to it, its agents the names of those lines (proto-lead p671); the lines counted are
/// insight (3)'s (N-1432 moved). Tonight: N-1445 paper-research and paper-research-brain; N-1104 the same, N-1445
/// standing under it; N-294 paper-research, ended after 72 s; N-1443 nobody (consult-3 was never given it in these
/// lines); no item. The record's p2 of N-1445, the opening's words again as his (no post since hs-ls's 435d5075), is
/// played as the opening for the brain, and was given to paper-research too, which its p3 counts anyway; p4 as given to
/// the brain (paper-research's answer to p3) is not played.
#[test]
fn tonight_insight_agents_per_topic_is_the_records_own_count() {
    use std::collections::{BTreeMap, BTreeSet};
    let mut r = Run::before_asks();
    let kept = r.n294();
    let (mut r, t) = Run::n1104_in(r);
    // act 1: p586 to paper-research, p587 at his press, each given as recorded
    assert_eq!(r.his(t, 1104, 0), 586);
    r.answer_as_recorded(t, 586, recorded_run(1104, 586), "watcher", "2026-11-20T18:23:00Z");
    r.given_as_recorded(t, 1104, 586, 586, "paper-research");
    assert_eq!(r.his(t, 1104, 1), 587);
    let run = recorded_run(1104, 587);
    r.answer_as_recorded(t, 587, run, "watcher", "2026-11-20T18:23:20Z");
    r.answer_as_recorded(t, 587, run, "owner", "2026-11-20T18:24:13Z");
    r.given_as_recorded(t, 1104, 587, 587, "paper-research");
    // N-1432's words placed on N-1104 after act 1 here, so that his p586 and p587 keep their numbers (the stand-ins
    // take p2-p585, which held them in the record)
    let moved = r.n1432(t);
    let sent = r.n1443();
    // act 1b: the opening's words to the brain, his first words to paper-research
    let b = r.n1445(t);
    r.given_as_recorded(b, 1445, 2, 1, "paper-research-brain");
    assert_eq!(r.his(b, 1445, 0), 2);
    r.answer_as_recorded(b, 2, recorded_run(1445, 3), "watcher", "2026-11-20T19:09:24Z");
    r.given_as_recorded(b, 1445, 3, 2, "paper-research");

    // the record's count
    let lines = record();
    let now = crate::machine::secs(&lines.iter().filter_map(|d| d["at"].as_str()).max().unwrap().to_string()).unwrap();
    let secs = |at: &str| crate::machine::secs(at).unwrap();
    let to = |d: &Value| d["to"].as_str().unwrap().to_string();
    assert!(lines.iter().filter(|d| d["ev"] == "given").all(|d| ["watcher", "minder", "paper-research", "paper-research-brain"].contains(&to(d).as_str())), "tonight's given lines name these four only");
    let up = |mut t: u64| {
        while let Some(p) = lines.iter().find(|d| d["ev"] == "opened" && d["t"] == json!(t)).and_then(|d| d["parent"].as_u64()) {
            t = p;
        }
        t
    };
    let mut agents: BTreeMap<u64, BTreeSet<String>> = BTreeMap::new();
    for d in lines.iter().filter(|d| d["ev"] == "given" && !["watcher", "minder"].contains(&to(d).as_str())) {
        agents.entry(up(d["t"].as_u64().unwrap())).or_default().insert(to(d));
    }
    for d in lines.iter().filter(|d| d["ev"] == "started") {
        agents.entry(d["t"].as_u64().unwrap()).or_default().insert(d["by"].as_str().unwrap().to_string());
    }
    let under = |t: u64| lines.iter().find(|d| d["ev"] == "open" && d["t"] == json!(t)).and_then(|d| d["under"].as_u64());
    let leads_to = |mut x: u64, t: u64| loop {
        if x == t {
            break true;
        }
        match under(x) {
            Some(y) => x = y,
            None => break false,
        }
    };
    let topic = |t: u64| agents.iter().filter(|(l, _)| leads_to(**l, t)).flat_map(|(_, a)| a.iter().cloned()).collect::<BTreeSet<String>>();
    // insight (3)'s lines (tonight_insight_open_to_done_is_the_records_own_count), the newest first, the most agents
    // first
    let mut want: Vec<(u64, BTreeSet<String>)> = [1445, 1443, 1104, 294].into_iter().map(|t| (t, topic(t))).collect();
    want.sort_by_key(|(_, a)| std::cmp::Reverse(a.len()));
    let set = |a: &[&str]| a.iter().map(|x| x.to_string()).collect::<BTreeSet<String>>();
    assert_eq!(want, vec![(1445, set(&["paper-research", "paper-research-brain"])), (1104, set(&["paper-research", "paper-research-brain"])), (294, set(&["paper-research"])), (1443, set(&[]))], "the record's count");
    let opened_at = |t: u64| {
        let o = lines.iter().find(|d| d["ev"] == "open" && d["t"] == json!(t)).or_else(|| lines.iter().find(|d| d["name"] == "owner_message" && d["fired"].as_array().is_some_and(|f| f.iter().any(|f| f["t"] == json!(t)))));
        secs(&at_of(o.unwrap()))
    };
    let closed = |t: u64| lines.iter().filter(|d| d["ev"] == "closed" && d["t"] == json!(t) && d["result"]["head"] != "Between").map(at_of).last();
    let took = |t: u64| closed(t).map_or(now, |c| secs(&c)) - opened_at(t);
    assert_eq!(took(294), 72);

    // the insight, over the machine as these acts leave it
    let here: BTreeMap<String, u64> = [(294, kept), (1104, t), (1432, moved), (1443, sent), (1445, b)].into_iter().map(|(n, x)| (r.m.id(x), n)).collect();
    let rows = crate::insight::threads(&r.m);
    let n = crate::insight::per_topic(&rows, now);
    let got: Vec<(u64, BTreeSet<String>, Option<i64>)> = n.threads.list.iter().map(|x| (here[&x.line.id], x.agents.iter().map(|a| a.to_string()).collect(), x.secs)).collect();
    assert_eq!(got, want.iter().map(|(t, a)| (*t, a.clone(), Some(took(*t)))).collect::<Vec<_>>(), "each topic's agents and its time, in the record's order");
    assert_eq!(n.threads.list.iter().find(|x| here[&x.line.id] == 1104).map(|x| x.lines.iter().map(|l| here[&l.id]).collect::<Vec<_>>()), Some(vec![1104, 1445]), "N-1445 under N-1104");
    assert_eq!((n.threads.agents, n.threads.took), (Some(1), Some(72)), "the smaller middle of 0, 1, 2, 2; N-294 the one ended");
    assert!(n.items.list.is_empty(), "no item tonight");
    r.replays();
}
