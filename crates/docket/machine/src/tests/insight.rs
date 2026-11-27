//! Insights (S6, proto-lead N-1446 p388). Insight (3), open → done: the lines of the outline with when each opened and
//! ended (`insight::threads`), and the numbers drawn from them (`insight::open_to_done`). Insight (1), work and talk: a
//! turn's end line carrying what it did (`Machine::turn_worked`), the turns read from the lines (`insight::turn`), and
//! the counts drawn from them (`insight::work_talk`). Insight (2), agents per topic: each line's agents and the line it
//! stands under (`Thread::agents`, `Thread::parent`), an item's starters (`Thread::started`), and the topics drawn from
//! them (`insight::per_topic`). Made-up threads and turns at made-up times; tonight's own counts are tonight.rs's.

use super::Run;
use crate::insight::{DAY, Did, OpenToDone, Turn, open_to_done, per_topic, threads, turn, work_talk};
use crate::*;
use serde_json::json;
use std::collections::BTreeMap;

impl Run {
    /// A thread opened by his words to lead at AT, as `say` opens one: its number.
    fn said_at(&mut self, text: &str, at: &str) -> u64 {
        let out = self.at(Input::Event { name: "owner_message".into(), value: json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": ["fixer"], "text": text, "addressed": {"head": "Named"}}) }, at);
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        *self.m.tickets.range(..HIDDEN).next_back().unwrap().0
    }

    /// Thread T ended HOW by lead at AT (`coord end`).
    fn end_at(&mut self, t: u64, how: &str, at: &str) {
        let out = self.at(Input::End { t, by: "lead".into(), how: how.into(), why: format!("{how} at {at}") }, at);
        assert!(out.reply.is_ok(), "{:?}", out.reply);
    }
}

/// Six lines of the outline: four of his threads, two of them ended done and dropped after 3 h and 1 h, two still
/// open, one of them for more than a day; an agents' thread ended kept after 30 min (it returned Between when it
/// opened; its end is the `coord end`); and his message moved onto the printer, not counted. His: opened 4, ended 2
/// (50%), the median 1 h (the shorter middle of an even count), open 2, open for days 1, the longest first. The
/// agents': opened 1, ended 1. At no time does a thread open count as ended. The log replays.
#[test]
fn open_to_done_counts_opened_ended_the_median_and_the_threads_open_for_days() {
    let mut r = Run::new();
    let a = r.said_at("the parser", "2026-11-18T09:00:00Z");
    let b = r.said_at("the lexer", "2026-11-19T10:00:00Z");
    let c = r.said_at("the printer", "2026-11-20T08:00:00Z");
    let d = r.said_at("the docs", "2026-11-20T09:00:00Z");
    let out = r.at(Input::Open { kind: "thread".into(), under: None, opener: "dev".into(), form: json!({"to": "orch", "typed": ["orch"], "names": ["orch"], "text": "", "addressed": {"head": "Sent", "by": "dev"}, "follows": null}), text: Some(Text::New("the build is slow".into())), names: vec!["orch".into()], told: vec![] }, "2026-11-20T10:00:00Z");
    assert!(out.reply.is_ok(), "{out:?}");
    let e = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    assert!(r.m.tickets[&e].between() && r.m.tickets[&e].closed_at.is_some(), "an agents' thread returns Between as it opens");
    // his words for the printer, a thread of their own until the watcher places them there
    let out = r.at(Input::Event { name: "owner_message".into(), value: json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": [], "text": "and the tests", "addressed": {"head": "Unnamed"}}) }, "2026-11-20T10:05:00Z");
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let moved = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    assert_eq!(threads(&r.m).len(), 6, "his words' thread counted while it is one");
    r.at(Input::Answer { t: moved, by: "watcher".into(), head: json!({"head": "Place", "ticket": r.m.id(c)}), text: Text::New(String::new()) }, "2026-11-20T10:05:03Z").reply.unwrap();
    assert!(matches!(&r.m.tickets[&moved].status, Status::Done(v) if v["head"] == "Moved"), "{:?}", r.m.tickets[&moved].status);
    let still = threads(&r.m);
    assert!(still.iter().all(|x| x.id != r.m.id(moved)), "moved: not counted, his words went onto the printer");
    assert_eq!(still.iter().filter(|x| x.ended.is_some()).count(), 0, "an agents' thread is open till it is ended: {still:#?}");
    assert_eq!(still.iter().map(|x| (x.id.clone(), x.by.as_deref())).collect::<Vec<_>>(), vec![(r.m.id(a), None), (r.m.id(b), None), (r.m.id(c), None), (r.m.id(d), None), (r.m.id(e), Some("dev"))], "his message's threads his; dev's its");
    r.end_at(a, "done", "2026-11-18T12:00:00Z");
    r.end_at(b, "dropped", "2026-11-19T11:00:00Z");
    r.end_at(e, "kept", "2026-11-20T10:30:00Z");

    let rows = threads(&r.m);
    let id = |t: u64| r.m.id(t);
    assert_eq!(rows.iter().map(|x| (x.id.clone(), x.state.as_str(), x.ended.as_deref())).filter(|x| x.2.is_some()).collect::<Vec<_>>(), vec![
        (id(a), "done", Some("2026-11-18T12:00:00Z")),
        (id(b), "dropped", Some("2026-11-19T11:00:00Z")),
        (id(e), "kept", Some("2026-11-20T10:30:00Z")),
    ]);
    assert_eq!(rows.iter().find(|x| x.id == id(c)).map(|x| (x.label.as_str(), x.kind.as_str(), x.opened.as_str(), x.last)), Some(("the printer", "thread", "2026-11-20T08:00:00Z", Some(2))), "his placed words its p2");

    // 21 Nov 08:30Z: the printer open 24 h 30 min, the docs 23 h 30 min
    let now = crate::machine::secs("2026-11-21T08:30:00Z").unwrap();
    let OpenToDone { his: n, agents } = open_to_done(&rows, now);
    assert_eq!((n.opened.len(), n.ended.len(), n.open.len()), (4, 2, 2));
    assert_eq!(n.rate(), Some(0.5));
    assert_eq!(n.median, Some(3600), "the shorter middle of 1 h and 3 h");
    assert_eq!(n.days.iter().map(|(x, s)| (x.id.clone(), *s)).collect::<Vec<_>>(), vec![(id(c), DAY + 1800)]);
    let listed = |xs: &[(&crate::insight::Thread, Option<i64>)]| xs.iter().map(|(x, s)| (x.id.clone(), *s)).collect::<Vec<_>>();
    assert_eq!(listed(&n.open), vec![(id(c), Some(DAY + 1800)), (id(d), Some(DAY - 1800))], "the longest open first");
    assert_eq!(listed(&n.ended), vec![(id(b), Some(3600)), (id(a), Some(3 * 3600))], "the latest end first");
    assert_eq!(n.opened.iter().map(|(x, _)| x.id.clone()).collect::<Vec<_>>(), vec![id(d), id(c), id(b), id(a)], "the newest first");
    assert_eq!((listed(&agents.opened), listed(&agents.ended), agents.open.len(), agents.rate()), (vec![(id(e), Some(1800))], vec![(id(e), Some(1800))], 0, Some(1.0)), "the agents' row: dev's, ended after 30 min");
    // with the docs done after 2 h: 1 h, 2 h, 3 h
    r.end_at(d, "done", "2026-11-20T11:00:00Z");
    let rows = threads(&r.m);
    assert_eq!(open_to_done(&rows, now).his.median, Some(7200));
    assert_eq!(open_to_done(&[], now).his.rate(), None, "nothing opened: no rate");
    r.replays();
}

/// A thread a clock's binding fired is not his (proto-lead N-1446 p581): it is counted in the row under his tiles, `by`
/// naming the binding as written, and not in his completion. His message's thread stays his. The log replays.
#[test]
fn a_thread_a_clock_fired_is_counted_apart_from_his() {
    const CLOCK: &str = "every 1h open thread(\"lead\", [\"lead\"], [\"lead\"], \"the hourly check\", Named, none)";
    let (d, errors) = super::defs_with(&[("clock.kind", CLOCK)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    let his = r.said_at("the parser", "2026-11-20T09:00:00Z");
    let out = r.at(Input::Event { name: "every".into(), value: json!(CLOCK) }, "2026-11-20T10:00:00Z");
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let fired = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    let rows = threads(&r.m);
    assert_eq!(rows.iter().map(|x| (x.id.clone(), x.by.as_deref())).collect::<Vec<_>>(), vec![(r.m.id(his), None), (r.m.id(fired), Some(CLOCK))]);
    let n = open_to_done(&rows, crate::machine::secs("2026-11-20T11:00:00Z").unwrap());
    let ids = |c: &crate::insight::Counts| c.opened.iter().map(|(x, _)| x.id.clone()).collect::<Vec<_>>();
    assert_eq!((ids(&n.his), ids(&n.agents)), (vec![r.m.id(his)], vec![r.m.id(fired)]));
    r.replays_with(&[("clock.kind", CLOCK)]);
}

/// A thread that still owes him when it closes ends when it stops owing him (proto-lead N-1446 p473, 3; consult-3
/// p451, 1), so Ended agrees with Open: his thread answered and closed, ended at its close; then lead's ask to him on
/// it, and it is open again, "waits on you", as the outline shows it; his "nothing to answer" ends it then. A note his
/// agent sent is the agents', one he sent his.
#[test]
fn a_thread_that_owes_him_ends_when_it_stops_owing_him() {
    let mut r = Run::before_asks();
    let t = r.say("lead", &[], "the release: A or B?");
    r.intake(t, json!({"head": "Thread"}));
    r.replies("lead", 10);
    r.minder(t, json!({"head": "Rest"}));
    r.go(Input::Edit { t, by: "owner".into(), program: "return OneOff".into(), why: "answered".into() }).reply.unwrap();
    let closed = r.m.tickets[&t].closed_at.clone().unwrap();
    let row = |r: &Run, t: u64| threads(&r.m).into_iter().find(|x| x.id == r.m.id(t)).unwrap();
    assert_eq!((row(&r, t).ended, row(&r, t).by), (Some(closed.clone()), None), "his, closed owing nothing: ended at its close");
    r.post_on(t, "lead", "and one more: ship today?", &["owner"]).reply.unwrap();
    let q = r.m.tickets[&t].posts.last().unwrap().n;
    r.asks(q, json!({"head": "Asks", "marks": [Run::mark([14, 25], &[])]}));
    assert_eq!((row(&r, t).ended, row(&r, t).state), (None, "waits on you".to_string()), "it owes him: open");
    let out = r.go(Input::NothingToAnswer { t, n: q, by: "owner".into() });
    let Some(Line::NothingToAnswer { at, .. }) = out.lines.first() else { panic!("{out:?}") };
    assert!(*at > closed, "{at} after {closed}");
    assert_eq!(row(&r, t).ended.as_ref(), Some(at), "ended when it stopped owing him, not at its close");
    assert_eq!(crate::view::ViewOutline::of(&r.m, &r.m.tickets[&t], false).ended_at.as_ref(), Some(at), "the outline's own end");
    let sent = r.note("lead", "fixer", "the build is slow");
    let his = r.note("owner", "lead", "the parser");
    assert_eq!((row(&r, sent).by, row(&r, his).by), (Some("lead".to_string()), None));
    r.replays_today();
}

/// A run under his line that gives him a step lets the line go when that step is answered, not at the run's close
/// (proto-reader N-1446 p592, his N-9: 10:07, not 10:40): the line closes at 10:00 while the run on lead's post there
/// (its kind's `when agent_post`) asks him; he answers at 10:07, and the run then waits on dev until 10:40. Open while his step waits, "waits on
/// you"; ended at 10:07 once he answered, before dev answers and after; the replay ends it at the same moment.
#[test]
fn a_run_under_his_line_lets_it_go_when_his_step_there_is_answered() {
    const LOOK: &str = "kind look\n  letter L\n  drawn line\n  form what: Text\n  result Text\n  pattern\n    v <- step lead \"Look at {{ what }}\" -> Seen;\n    return what\n  when agent_post(p) do (a <- step owner \"Is {{ what }} right?\" -> Seen; b <- step dev \"Then check {{ what }}\" -> Seen; return ())\n";
    let (d, errors) = super::defs_with(&[("look.kind", LOOK)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    let at = |hm: &str| format!("2026-11-20T{hm}:00Z");
    let out = r.at(Input::Open { kind: "look".into(), under: None, opener: "owner".into(), form: json!({"what": "the parser"}), text: None, names: vec![], told: vec![] }, &at("09:00"));
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let t = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    let give = |r: &mut Run, agent: &str, hm: &str| {
        let d = r.m.due(agent).pop().expect("a step due");
        r.at(Input::Given { t: d.t, to: agent.into(), delivery: d.id, upto: d.upto, place: None, skipped: Default::default() }, &at(hm)).reply.unwrap();
        d.t
    };
    let answer = |r: &mut Run, t: u64, by: &str, hm: &str| {
        let out = r.at(Input::Answer { t, by: by.into(), head: json!({"head": "Seen"}), text: Text::New(String::new()) }, &at(hm));
        assert!(out.reply.is_ok(), "{:?}", out.reply);
    };
    let row = |m: &Machine| threads(m).into_iter().find(|x| x.id == m.id(t)).unwrap();
    give(&mut r, "lead", "09:01");
    let out = r.at(Input::Post { t, by: "lead".into(), text: Text::New("one thing to check".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }, &at("09:50"));
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let run = r.m.tickets.values().find(|y| y.under == Some(t) && y.handling.is_some()).expect("a run under it").t;
    answer(&mut r, t, "lead", "10:00");
    assert_eq!(r.m.tickets[&t].closed_at, Some(at("10:00")));
    assert_eq!((row(&r.m).ended, row(&r.m).state), (None, "waits on you".to_string()), "closed, his step in the run owed: open");
    answer(&mut r, run, "owner", "10:07");
    assert_eq!((row(&r.m).ended, row(&r.m).state), (Some(at("10:07")), "done".to_string()), "his step answered: it goes, dev's step owed");
    give(&mut r, "dev", "10:08");
    answer(&mut r, run, "dev", "10:40");
    assert_eq!((r.m.tickets[&run].closed_at.clone(), row(&r.m).ended), (Some(at("10:40")), Some(at("10:07"))), "the run's close is not his line's end");
    let lines: Vec<Line> = r.log.iter().map(|l| serde_json::from_str(&serde_json::to_string(l).unwrap()).unwrap()).collect();
    let mut m2 = Machine::load(super::defs_with(&[("look.kind", LOOK)]).0);
    assert_eq!(m2.replay(&lines), Vec::<String>::new());
    assert_eq!(row(&m2).ended, Some(at("10:07")), "the replay ends it at the same moment");
}

/// Insight (2), agents per topic (proto-lead N-1446 p653, p671): a line's agents are those given anything on it or on
/// what counts for it (a run under it), coordd's own watcher not among them; an item's starters are those who said they
/// work on it (`item start`); a line under another names it. A thread's topic is it with every line under it, an
/// item's itself; a topic's agents are the names of its lines, the starters among them. Made up: his look at the parser, lead given its step there, and a run on lead's post there that asks him,
/// then gives dev a step (the line ends 10:07, as he answers); his message to lead on the lexer, the watcher given its
/// intake first; an item under the look that dev and fixer started; lead's note to fixer. The look's topic: dev, fixer
/// and lead (the item under it started by dev and fixer), took 1 h 7 min; the note's and the lexer's one each; the
/// item's two. The log replays to the same rows.
#[test]
fn agents_per_topic_counts_whoever_was_given_anything_there_and_who_started_an_item() {
    const LOOK: &str = "kind look\n  letter L\n  drawn line\n  form what: Text\n  result Text\n  pattern\n    v <- step lead \"Look at {{ what }}\" -> Seen;\n    return what\n  when agent_post(p) do (a <- step owner \"Is {{ what }} right?\" -> Seen; b <- step dev \"Then check {{ what }}\" -> Seen; return ())\n";
    let (d, errors) = super::defs_with(&[("look.kind", LOOK)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    let at = |hm: &str| format!("2026-11-20T{hm}:00Z");
    let last = |r: &Run| *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    let give = |r: &mut Run, agent: &str, t: u64, hm: &str| {
        let d = r.m.due(agent).into_iter().find(|d| d.t == t).unwrap_or_else(|| panic!("{agent} is owed something on {t}"));
        r.at(Input::Given { t, to: agent.into(), delivery: d.id, upto: d.upto, place: None, skipped: Default::default() }, &at(hm)).reply.unwrap();
    };
    let answer = |r: &mut Run, t: u64, by: &str, hm: &str| {
        let out = r.at(Input::Answer { t, by: by.into(), head: json!({"head": "Seen"}), text: Text::New(String::new()) }, &at(hm));
        assert!(out.reply.is_ok(), "{:?}", out.reply);
    };
    r.at(Input::Open { kind: "look".into(), under: None, opener: "owner".into(), form: json!({"what": "the parser"}), text: None, names: vec![], told: vec![] }, &at("09:00")).reply.unwrap();
    let look = last(&r);
    give(&mut r, "lead", look, "09:01");
    r.at(Input::Post { t: look, by: "lead".into(), text: Text::New("one thing to check".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }, &at("09:50")).reply.unwrap();
    let run = r.m.tickets.values().find(|y| y.under == Some(look) && y.handling.is_some()).expect("a run under it").t;
    answer(&mut r, look, "lead", "10:00");
    answer(&mut r, run, "owner", "10:07");
    give(&mut r, "dev", run, "10:08");
    answer(&mut r, run, "dev", "10:40");
    // his words naming nobody: the watcher's intake first, given as coordd writes it (the record's `N-294.s1`)
    r.at(Input::Event { name: "owner_message".into(), value: json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": [], "text": "the lexer", "addressed": {"head": "Unnamed"}}) }, &at("11:00")).reply.unwrap();
    let lexer = last(&r);
    r.at(Input::Given { t: lexer, to: "watcher".into(), delivery: format!("{}.s1", r.m.id(lexer)), upto: 0, place: None, skipped: Default::default() }, &at("11:00")).reply.unwrap();
    r.at(Input::Answer { t: lexer, by: "watcher".into(), head: json!({"head": "Thread"}), text: Text::New(String::new()) }, &at("11:01")).reply.unwrap();
    give(&mut r, "lead", lexer, "11:02");
    r.at(Input::Open { kind: "item".into(), under: Some(look), opener: "lead".into(), form: json!({"title": "the parser's tests"}), text: None, names: vec![], told: vec![] }, &at("11:30")).reply.unwrap();
    let item = last(&r);
    for (by, hm) in [("dev", "11:31"), ("fixer", "11:32")] {
        r.at(Input::Started { t: item, by: by.into() }, &at(hm)).reply.unwrap();
    }
    r.at(Input::Open { kind: "thread".into(), under: None, opener: "lead".into(), form: json!({"to": "lead", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "lead"}, "follows": null}), text: Some(Text::New("the build is slow".into())), names: vec!["fixer".into()], told: vec![] }, &at("11:40")).reply.unwrap();
    let note = last(&r);
    give(&mut r, "fixer", note, "11:41");

    let rows = threads(&r.m);
    let id = |t: u64| r.m.id(t);
    let names = |v: &[&str]| v.iter().map(|a| a.to_string()).collect::<Vec<_>>();
    let got = |rows: &[crate::insight::Thread]| rows.iter().map(|x| (x.id.clone(), x.kind.clone(), x.parent.clone(), x.agents.clone(), x.started.clone())).collect::<Vec<_>>();
    assert_eq!(got(&rows), vec![
        (id(look), "look".to_string(), None, names(&["dev", "lead"]), vec![]),
        (id(lexer), "thread".to_string(), None, names(&["lead"]), vec![]),
        (id(item), "item".to_string(), Some(id(look)), vec![], names(&["dev", "fixer"])),
        (id(note), "thread".to_string(), None, names(&["fixer"]), vec![]),
    ], "the run's dev counted on the look; the watcher on none");

    let now = crate::machine::secs(&at("12:00")).unwrap();
    let n = per_topic(&rows, now);
    let topics = |t: &crate::insight::Topics| t.list.iter().map(|x| (x.line.id.clone(), x.agents.iter().map(|a| a.to_string()).collect::<Vec<_>>(), x.lines.iter().map(|l| l.id.clone()).collect::<Vec<_>>(), x.secs)).collect::<Vec<_>>();
    assert_eq!(topics(&n.threads), vec![
        (id(look), names(&["dev", "fixer", "lead"]), vec![id(look), id(item)], Some(67 * 60)),
        (id(note), names(&["fixer"]), vec![id(note)], Some(20 * 60)),
        (id(lexer), names(&["lead"]), vec![id(lexer)], Some(3600)),
    ], "the most agents first, then the newest");
    assert_eq!((n.threads.agents, n.threads.took), (Some(1), Some(67 * 60)), "the median of 3, 1, 1; only the look ended");
    assert_eq!(topics(&n.items), vec![(id(item), names(&["dev", "fixer"]), vec![id(item)], Some(30 * 60))]);
    assert_eq!((n.items.agents, n.items.took), (Some(2), None), "the item open: no time to done");

    let lines: Vec<Line> = r.log.iter().map(|l| serde_json::from_str(&serde_json::to_string(l).unwrap()).unwrap()).collect();
    let mut m2 = Machine::load(super::defs_with(&[("look.kind", LOOK)]).0);
    assert_eq!(m2.replay(&lines), Vec::<String>::new());
    assert_eq!(threads(&m2), rows, "the replay's rows the same");
}

/// consult-3's grep test (N-1446 p368, p377), in each insight's head: the insight's reader, end to end, names no
/// coord's `route`, `envdir`, `.proxy-key` or `claude-home`, none of the proxy's files, nothing of the gateway (8317, its
/// management side), no login file, and no path under the accounts root; and it does no IO of its own (no file, process,
/// socket or environment read), but for insight (1)'s one: the record's own file, tickets/log.jsonl, opened once. The
/// reader is this crate's insight.rs (the queries and the numbers), coordd's `Core::insight` (the answer under the
/// lock), `Core::record_file`, `insight_off_lock` and `insight_turns` (insight (1)'s off it) and the window's insight.rs
/// (the request and the drawing), each up to its tests (the window's stills write their pictures).
#[test]
fn the_insights_reader_names_no_login_no_gateway_and_nothing_under_the_accounts_root() {
    let coordd = include_str!("../../../../coordd/src/ticket/mod.rs");
    let main = include_str!("../../../../coordd/src/main.rs");
    // a function of TEXT from its head HEAD to the end of its body (END, the first after it at its indent)
    let body = |text: &'static str, head: &str, end: &str| -> &'static str {
        let from = text.find(head).unwrap_or_else(|| panic!("{head}"));
        &text[from..from + text[from..].find(end).expect("its end")]
    };
    let code = |text: &'static str| text.split("\n#[cfg(test)]").next().unwrap_or_default();
    let reader = [
        ("docket-machine insight.rs", code(include_str!("../insight.rs"))),
        ("coordd Core::insight", body(coordd, "    pub fn insight(&self", "\n    }\n")),
        ("coordd Core::record_file", body(coordd, "    pub fn record_file(&self", "\n    }\n")),
        ("coordd insight_off_lock", body(main, "async fn insight_off_lock(", "\n}\n")),
        ("coordd insight_turns", body(coordd, "pub fn insight_turns(", "\n}\n")),
        ("yaaihh insight.rs", code(include_str!("../../../../yaaihh/src/insight.rs"))),
    ];
    let forbidden = [
        // a coord's own files and the proxy's (p368)
        "route", "envdir", "proxy-key", "claude-home", "config.yaml", "oauth", "proxy_files", "PROXY_FILES",
        // the gateway
        "8317", "management", "127.0.0.1", "localhost",
        // login files
        "credentials", "auth.json", ".claude.json", "CLAUDE_CONFIG_DIR",
        // the accounts root and what is under it (p377)
        "COORD_ACCOUNTS", "agent-coord/accounts", "accounts::", "account.json", "state.json", "/bound/", "bound_to", "Store::",
        // IO of its own
        "std::fs", "fs::read", "fs::write", "read_dir", "File::", "Command", "TcpStream", "UdpSocket", "reqwest", "http", "env::var",
    ];
    // insight (1)'s one file: the record's, named under the lock, opened once off it
    let opens = ["std::fs::File::open(file)", "\"tickets/log.jsonl\""];
    for (name, text) in reader {
        assert!(text.len() > 60, "{name} read");
        let rest = opens.iter().fold(text.to_string(), |t, o| t.replacen(o, "", 1));
        let found: Vec<&str> = forbidden.iter().copied().filter(|w| rest.contains(w)).collect();
        assert!(found.is_empty(), "{name} names {found:?}");
    }
    assert!(reader[2].1.contains(opens[1]) && reader[4].1.contains(opens[0]), "the record's file, opened by the scan");
}

impl Run {
    /// AGENT's turn ends, given what is due to it on T, with WORKED told by coordd for WHO's turn: its line.
    fn worked_turn(&mut self, agent: &str, t: u64, who: &str, worked: &Worked, n: u64) -> Line {
        let d = self.give(agent, t);
        self.m.turn_worked = Some((who.into(), worked.clone()));
        let out = self.turn(agent, Some(&d), n);
        self.m.turn_worked = None;
        out.lines.into_iter().find(|l| matches!(l, Line::TurnEnded { .. })).expect("its turn's end kept")
    }
}

/// A turn's end line carries what coordd's follower said the turn did (insight (1)): its own tools by name and the
/// model that answered, told for that agent's turn and written on its line alone; a line without them is written as
/// before (no key), so an older engine reads both; `insight::turn` reads it back with its line's number. The log
/// replays.
#[test]
fn a_turns_end_line_carries_the_tools_and_model_coordd_told_for_it() {
    let mut r = Run::new();
    let a = r.note("owner", "lead", "the parser");
    let worked = Worked { tools: BTreeMap::from([("Bash".into(), 3), ("mcp__coord__reply".into(), 1)]), model: Some("claude-opus-5-5".into()) };
    let line = r.worked_turn("lead", a, "lead", &worked, 2);
    let v = serde_json::to_value(&line).unwrap();
    assert_eq!((&v["tools"], &v["model"]), (&json!({"Bash": 3, "mcp__coord__reply": 1}), &json!("claude-opus-5-5")), "{v}");
    let x = turn(&line, 7).unwrap();
    assert_eq!((x.agent.as_str(), x.tools.clone(), x.model.as_deref(), x.line, x.did()), ("lead", worked.tools.clone(), Some("claude-opus-5-5"), 7, Did::Work));
    // told for another agent's turn: not this one's
    let out = r.go(Input::Post { t: a, by: "owner".into(), text: Text::New("and the lexer".into()), names: vec![], told: vec!["lead".into()], inform: vec![], answers: None, kind: None });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let other = r.worked_turn("lead", a, "fixer", &worked, 3);
    let v = serde_json::to_value(&other).unwrap();
    assert!(v.get("tools").is_none() && v.get("model").is_none(), "nothing told for lead's turn, nothing written: {v}");
    assert_eq!(turn(&other, 8).map(|x| x.did()), Some(Did::NotKnown));
    assert_eq!(serde_json::from_value::<Line>(v).unwrap(), other, "read back as written");
    r.replays();
}

/// A made-up turn: AGENT's, ended AT, its TOOLS and MODEL, at LINE.
fn made(agent: &str, at: &str, tools: &[(&str, u32)], model: Option<&str>, line: u64) -> Turn {
    Turn { agent: agent.into(), at: at.into(), delivery: Some(format!("N-1.p{line}")), tools: tools.iter().map(|(t, n)| (t.to_string(), *n)).collect(), model: model.map(String::from), line }
}

/// Insight (1)'s counts over five turns: a line from before (not known); fixer's turn that ran, read and replied
/// (work); lead's that only replied (talk); fixer's that called nothing (talk: its model named); a Codex turn of fixer's
/// that replied and edited (work: Codex names coord's tools `coord__…`). Work 2, talk 2, not known 1, half the turns
/// known; by agent the most turns first, by model then by name, by hour the latest first; each list the newest first.
#[test]
fn work_talk_counts_each_turn_by_what_it_did_by_agent_model_and_hour() {
    let turns = [
        made("lead", "2026-11-20T08:10:00Z", &[], None, 1),
        made("fixer", "2026-11-20T09:05:00Z", &[("Bash", 4), ("Read", 2), ("mcp__coord__reply", 1)], Some("claude-opus-5-5"), 2),
        made("lead", "2026-11-20T09:40:00Z", &[("mcp__coord__reply", 1)], Some("claude-sonnet-5-5"), 3),
        made("fixer", "2026-11-20T10:15:00Z", &[], Some("claude-opus-5-5"), 4),
        made("fixer", "2026-11-20T10:50:00Z", &[("coord__reply", 1), ("Edit", 3)], Some("gpt-6.2-sol"), 5),
    ];
    assert_eq!(made("w", "2026-11-20T11:00:00Z", &[("coord__reply", 2)], None, 6).did(), Did::Talk, "Codex's coord tools only talk");
    // coord in a shell (proto-reader N-1446 p511): a `coord post` run alone is coord's own, as its reply tool is; a
    // shell call that runs anything else is work
    assert_eq!(made("w", "2026-11-20T11:00:00Z", &[("Bash(coord)", 1), ("mcp__coord__reply", 1)], Some("claude-opus-5-5"), 6).did(), Did::Talk, "a Bash coord post and a reply: talk");
    assert_eq!(made("w", "2026-11-20T11:00:00Z", &[("Bash", 1), ("mcp__coord__reply", 1)], Some("claude-opus-5-5"), 6).did(), Did::Work);
    let n = work_talk(&turns);
    let lines = |xs: &[&Turn]| xs.iter().map(|x| x.line).collect::<Vec<_>>();
    assert_eq!((lines(&n.all.work), lines(&n.all.talk), lines(&n.all.not_known)), (vec![5, 2], vec![4, 3], vec![1]));
    assert_eq!(n.all.share(), Some(0.5));
    let rows = |ts: &[crate::insight::Tally]| ts.iter().map(|t| (t.label.clone(), lines(&t.work), lines(&t.talk), lines(&t.not_known))).collect::<Vec<_>>();
    assert_eq!(rows(&n.agents), vec![("fixer".to_string(), vec![5, 2], vec![4], vec![]), ("lead".to_string(), vec![], vec![3], vec![1])]);
    assert_eq!(rows(&n.models), vec![
        ("claude-opus-5-5".to_string(), vec![2], vec![4], vec![]),
        (String::new(), vec![], vec![], vec![1]),
        ("claude-sonnet-5-5".to_string(), vec![], vec![3], vec![]),
        ("gpt-6.2-sol".to_string(), vec![5], vec![], vec![]),
    ]);
    assert_eq!(rows(&n.hours), vec![("2026-11-20T10".to_string(), vec![5], vec![4], vec![]), ("2026-11-20T09".to_string(), vec![2], vec![3], vec![]), ("2026-11-20T08".to_string(), vec![], vec![], vec![1])]);
    assert_eq!((n.agents[1].share(), n.models[1].share()), (Some(0.0), None), "lead only talked; no model named, none known");
    assert_eq!(work_talk(&[]).all.len(), 0);
}
