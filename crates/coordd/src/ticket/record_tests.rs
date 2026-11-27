//! `Request::Record` (the 0.3 window's Record layer, proto-window N-1446 p10, p84; consult-3 p34): a ticket's lines of
//! the record as written, its handlers' runs among them, from a time or after a line, capped. The read off the lock is
//! main.rs's `record_tests`. And insight (1)'s turns (S6): what a turn did, written on its `turn_ended` line, read back
//! from the record's file (`insight_turns`).

use super::edit_tests::core;
use super::{RecordScope, record_lines};
use docket_machine::{HIDDEN, Input, Text};
use serde_json::{Value, json};

fn read(r: &coord_types::Reply) -> Vec<coord_types::RecordLine> {
    assert!(r.ok, "{}", r.out);
    r.out.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

fn numbers(r: &coord_types::Reply) -> Vec<u64> {
    read(r).iter().map(|l| l.line).collect()
}

/// Tonight's p586/p587 in small: thread 5's own lines, the delivered event about it, its handler's run, and the turns
/// its posts went into, as written and numbered, and a post of it kept waiting or reported (about the post, not a
/// turn's: proto-reader N-1446 p223); not thread 6's, not another run's line that names thread 5's post inside it, not
/// an agent's act, not a turn where its post only waited.
#[test]
fn a_tickets_record_is_its_lines_and_its_runs_as_written_from_a_time_or_a_line() {
    let dir = std::env::temp_dir().join(format!("docket-record-lines-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let run = HIDDEN + 3;
    let lines = [
        json!({"ev": "opened", "at": "2026-11-20T18:00:00Z", "t": 5, "kind": "thread"}),
        json!({"ev": "opened", "at": "2026-11-20T18:00:01Z", "t": 6, "kind": "thread"}),
        json!({"ev": "post", "at": "2026-11-20T18:22:55Z", "t": 5, "by": "owner", "text": "eh yeah but wait lets analyze"}),
        json!({"ev": "event", "at": "2026-11-20T18:23:02Z", "name": "delivered", "value": {"t": 5, "delivery": "N-5.p2"}}),
        json!({"ev": "post", "at": "2026-11-20T18:23:17Z", "t": 5, "by": "owner", "text": "like ofc for reset apply can be special action"}),
        json!({"ev": "opened", "at": "2026-11-20T18:23:17Z", "t": run, "kind": "owner_post", "parent": 5}),
        json!({"ev": "step_args", "at": "2026-11-20T18:23:17Z", "t": run, "step": 1, "args": {"text": {"post": {"n": 3, "t": 5}}}}),
        json!({"ev": "step_args", "at": "2026-11-20T18:23:18Z", "t": HIDDEN + 9, "step": 1, "args": {"text": {"post": {"n": 3, "t": 5}}}}),
        json!({"ev": "act", "at": "2026-11-20T18:23:18Z", "agent": "accounts-lead", "is": "edit"}),
        json!({"ev": "answer", "at": "2026-11-20T18:23:20Z", "t": run, "by": "watcher", "head": {"head": "Unclear"}}),
        // the turns his posts went into, which name them by id (proto-reader N-1446 p128)
        json!({"ev": "event", "at": "2026-11-20T18:23:21Z", "name": "delivery_acted", "value": {"act": "deliver", "agent": "paper-research", "items": ["N-5.p2"], "rest": ["N-6.p2"]}, "fired": []}),
        json!({"ev": "turn_ended", "at": "2026-11-20T18:23:32Z", "agent": "paper-research", "delivery": "N-5.p2", "stood": ""}),
        json!({"ev": "event", "at": "2026-11-20T18:23:33Z", "name": "owner_to", "value": {"agent": "paper-research", "posts": ["N-5.p3"]}, "fired": []}),
        json!({"ev": "event", "at": "2026-11-20T18:23:34Z", "name": "owner_message", "value": {"text": "a new one"}, "fired": [{"t": 5, "kind": "thread"}]}),
        // what waited is not about it, nor another thread's post whose id starts the same
        json!({"ev": "event", "at": "2026-11-20T18:23:35Z", "name": "delivery_acted", "value": {"act": "deliver", "agent": "paper-research", "items": ["N-6.p2"], "rest": ["N-5.p3"]}, "fired": []}),
        json!({"ev": "turn_ended", "at": "2026-11-20T18:23:36Z", "agent": "paper-research", "delivery": "N-55.p1", "stood": ""}),
        // a post of it left waiting, or reported, beside another thread's: about the post
        json!({"ev": "event", "at": "2026-11-20T18:23:37Z", "name": "delivery_acted", "value": {"act": "keep", "agent": "docket-core", "items": ["N-6.p2", "N-5.p3"], "why": "no queued post asks it to take another step"}, "fired": []}),
        json!({"ev": "event", "at": "2026-11-20T18:23:38Z", "name": "delivery_acted", "value": {"act": "report", "agent": "a2h-speed", "items": ["N-5.p3"], "why": "not running"}, "fired": []}),
    ];
    let written: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    std::fs::write(dir.join("log.jsonl"), written.join("\n") + "\n").unwrap();
    let scope = |since: Option<&str>| RecordScope { file: dir.join("log.jsonl"), id: "N-5".into(), ts: [5, run].into(), since: since.map(str::to_string) };
    let all = record_lines(&scope(None), None, 0);
    assert_eq!(numbers(&all), [1, 3, 4, 5, 6, 7, 10, 11, 12, 13, 14, 17, 18], "its lines, the events about it, its run's, the turns its posts went into, its post kept or reported; nothing else");
    for l in read(&all) {
        assert_eq!(l.text, written[l.line as usize - 1], "line {} as written", l.line);
    }
    // from post 3's time (p587's 18:23:17): the post, its run opened, its route step, its answer
    let from = scope(Some("2026-11-20T18:23:17Z"));
    assert_eq!(numbers(&record_lines(&from, None, 0)), [5, 6, 7, 10, 11, 12, 13, 14, 17, 18]);
    assert_eq!(numbers(&record_lines(&from, Some(6), 0)), [7, 10, 11, 12, 13, 14, 17, 18], "the next page, after line 6");
    assert_eq!(numbers(&record_lines(&from, None, 2)), [5, 6], "capped");
    // no file: said, not an empty record
    let gone = RecordScope { file: dir.join("none.jsonl"), id: "N-5".into(), ts: [5].into(), since: None };
    assert!(!record_lines(&gone, None, 0).ok);
    let _ = std::fs::remove_dir_all(&dir);
}

/// consult-3's turn on the record (N-1446 p174): given N-1446.p109 at 106453 beside other threads, it ended at 107816
/// naming N-1334.p14 and informing two others, none of them N-1446. Here in small: thread 5's post given to c3, another
/// agent's turn ending, c3's turn ending on N-9's post; then c3's next turn, which was not given it, and a turn that
/// informs N-5. The given turn is kept from a page or a time that starts after the `given`.
#[test]
fn a_turn_given_the_thread_is_in_its_record_though_its_end_names_another() {
    let dir = std::env::temp_dir().join(format!("docket-record-given-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let lines = [
        json!({"ev": "post", "at": "2026-11-20T19:57:50Z", "t": 5, "by": "proto-window", "text": "Head S4b for a read"}),
        json!({"ev": "given", "at": "2026-11-20T19:57:54Z", "t": 5, "to": "consult-3", "delivery": "N-5.p1", "upto": 1}),
        json!({"ev": "given", "at": "2026-11-20T19:57:55Z", "t": 9, "to": "consult-3", "delivery": "N-9.p14", "upto": 14}),
        json!({"ev": "turn_ended", "at": "2026-11-20T20:00:00Z", "agent": "proto-reader", "delivery": "N-9.p13", "stood": ""}),
        json!({"ev": "turn_ended", "at": "2026-11-20T20:13:52Z", "agent": "consult-3", "delivery": "N-9.p14", "informs": ["N-7.p12", "N-8.p2200"], "stood": ""}),
        json!({"ev": "turn_ended", "at": "2026-11-20T20:20:00Z", "agent": "consult-3", "delivery": "N-9.p15", "stood": ""}),
        json!({"ev": "turn_ended", "at": "2026-11-20T20:21:00Z", "agent": "proto-lead", "delivery": "N-9.p16", "informs": ["N-5.p1"], "stood": ""}),
    ];
    let written: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    std::fs::write(dir.join("log.jsonl"), written.join("\n") + "\n").unwrap();
    let scope = |since: Option<&str>| RecordScope { file: dir.join("log.jsonl"), id: "N-5".into(), ts: [5].into(), since: since.map(str::to_string) };
    assert_eq!(numbers(&record_lines(&scope(None), None, 0)), [1, 2, 5, 7], "its post, its given, the turn given it, the turn that informs it");
    assert_eq!(numbers(&record_lines(&scope(None), Some(2), 0)), [5, 7], "a page after the given keeps the turn");
    assert_eq!(numbers(&record_lines(&scope(Some("2026-11-20T20:00:00Z")), None, 0)), [5, 7], "and a time after it");
    let _ = std::fs::remove_dir_all(&dir);
}

/// What a thread's record takes under the lock: the thread and its handlers' runs, not another thread, not a
/// subthread (a node of its own); its post's time, and a post it does not have refused.
#[test]
fn a_threads_record_holds_its_runs_and_not_its_subthread() {
    let (mut core, coord) = core("record");
    let open = |core: &mut super::Core, under: Option<u64>, text: &str| {
        let form = json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null});
        let r = core.ticket_input(Input::Open { kind: "thread".into(), under, opener: "owner".into(), form, text: Some(Text::New(text.into())), names: vec!["w2".into()], told: vec![] }).unwrap();
        docket_machine::parse_ticket(r.trim_start_matches("Opened ").trim_end_matches('.')).unwrap()
    };
    let a = open(&mut core, None, "the parser");
    let b = open(&mut core, None, "the lexer");
    core.ticket_input(Input::Post { t: a, by: "owner".into(), text: Text::New("and the printer".into()), names: vec!["w2".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let c = open(&mut core, Some(a), "the printer, apart");
    let id = core.tickets.m.id(a);
    let scope = core.ticket_record(&id, None).unwrap_or_else(|r| panic!("{}", r.out));
    let runs: Vec<u64> = core.tickets.m.tickets.range(HIDDEN..).filter(|(_, x)| x.under == Some(a)).map(|(t, _)| *t).collect();
    assert!(!runs.is_empty(), "his post on it ran a handler");
    assert!(scope.ts.contains(&a) && runs.iter().all(|r| scope.ts.contains(r)), "the thread and its runs");
    assert!(!scope.ts.contains(&b) && !scope.ts.contains(&c), "not another thread, not its subthread");
    assert!(scope.ts.iter().all(|t| *t == a || *t >= HIDDEN));
    // read: his post's line is in it, as written; no line is the other threads'
    let got = read(&record_lines(&scope, None, 0));
    let tickets: Vec<Value> = got.iter().map(|l| serde_json::from_str(&l.text).unwrap()).collect();
    assert!(tickets.iter().any(|v| v["ev"] == "post" && v["text"] == "and the printer"), "{got:?}");
    assert!(tickets.iter().all(|v| v["t"] != json!(b) && v["t"] != json!(c)));
    // from its second post: that post's time; a post it does not have is refused
    let second = core.tickets.m.tickets[&a].posts.iter().find(|p| p.n == 2).map(|p| serde_json::to_value(&p.at).unwrap());
    assert_eq!(core.ticket_record(&id, Some(2)).ok().and_then(|s| s.since).map(Value::from), second);
    assert!(core.ticket_record(&id, Some(99)).is_err());
    let _ = std::fs::remove_dir_all(&coord);
}

/// Insight (1) in coordd (S6): what the follower said w2's turn did, registered by where its final output ends (as
/// main's follower loop does), goes on that turn's `turn_ended` line when its end is taken, and on no other: an entry
/// ending earlier in the file, or in another session's file, is a turn whose end was never kept, dropped; the next
/// turn, told nothing, carries nothing. `insight_turns` reads the record's file back: each turn with its line's number,
/// the line itself the record's `turn_ended`.
#[test]
fn a_turns_tools_and_model_go_on_its_own_line_and_are_read_back_from_the_record() {
    use docket_machine::{Loc, Worked};
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let coord = std::env::temp_dir().join(format!("docket-insight-worked-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&coord);
    std::fs::create_dir_all(&coord).unwrap();
    std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
    let mut core = super::Core::load(&coord.display().to_string(), &base);
    core.load_tickets();
    // his note to w2, as `say` opens one
    let form = json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null});
    core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form, text: Some(Text::New("the parser".into())), names: vec!["w2".into()], told: vec![] }).unwrap();
    let t = *core.tickets.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    let transcript = coord.join("w2.jsonl");
    let path = transcript.display().to_string();
    let said = |text: &str| format!("{}\n", json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": text}], "stop_reason": "end_turn"}}));
    // w2 is given what is due to it on the thread and its turn ends with TEXT: what it was given, where its text is
    let turn = |core: &mut super::Core, text: &str| {
        let d = core.ticket_due("w2").into_iter().find(|d| d.t == t).expect("a post is due");
        core.ticket_input(Input::Given { t, to: "w2".into(), delivery: d.id.clone(), upto: d.upto, place: None, skipped: Default::default() }).unwrap();
        let at = std::fs::metadata(&transcript).map_or(0, |m| m.len());
        let line = said(text);
        std::io::Write::write_all(&mut std::fs::OpenOptions::new().create(true).append(true).open(&transcript).unwrap(), line.as_bytes()).unwrap();
        (d.id, Loc { file: path.clone(), offset: at, len: line.len() as u64 })
    };
    let worked = Worked { tools: [("Bash".to_string(), 2), ("mcp__coord__reply".to_string(), 1)].into(), model: Some("claude-opus-5-5".into()) };
    let (first, loc) = turn(&mut core, "because");
    let end = loc.offset + loc.len;
    let other = Worked { tools: [("Read".to_string(), 9)].into(), model: Some("m".into()) };
    core.tickets.worked.entry("w2".into()).or_default().extend([(path.clone(), 3, other.clone()), ("old.jsonl".into(), end, other), (path.clone(), end, worked.clone())]);
    core.ticket_turn_ended("w2", loc);
    assert_eq!(core.tickets.worked.get("w2").map(Vec::len), Some(0), "its own taken, the others dropped");
    assert_eq!(core.tickets.m.turn_worked, None, "cleared after its line");
    core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("and the lexer".into()), names: vec![], told: vec!["w2".into()], inform: vec![], answers: None, kind: None }).unwrap();
    let (second, loc) = turn(&mut core, "and so");
    core.ticket_turn_ended("w2", loc);
    let r = super::insight_turns(&core.record_file());
    assert!(r.ok, "{}", r.out);
    let turns: Vec<docket_machine::insight::Turn> = r.out.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let got: Vec<(Option<&str>, &std::collections::BTreeMap<String, u32>, Option<&str>)> = turns.iter().map(|x| (x.delivery.as_deref(), &x.tools, x.model.as_deref())).collect();
    assert_eq!(got, vec![(Some(first.as_str()), &worked.tools, Some("claude-opus-5-5")), (Some(second.as_str()), &Default::default(), None)]);
    let record: Vec<String> = std::fs::read_to_string(core.record_file()).unwrap().lines().map(String::from).collect();
    for x in &turns {
        let line: Value = serde_json::from_str(&record[x.line as usize - 1]).unwrap();
        assert_eq!((line["ev"].as_str(), line["delivery"].as_str()), (Some("turn_ended"), x.delivery.as_deref()), "line {}", x.line);
    }
    assert_eq!(turns.iter().map(|x| x.did()).collect::<Vec<_>>(), [docket_machine::insight::Did::Work, docket_machine::insight::Did::NotKnown]);
    let _ = std::fs::remove_dir_all(&coord);
}
