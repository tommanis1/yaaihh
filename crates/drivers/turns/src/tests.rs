//! Passages, usage and answers read from made-up Claude Code and Codex transcripts and an agy-rpc rollout; words
//! carried to agents.

use super::*;
use serde_json::json;
use std::cell::RefCell;
use std::rc::Rc;

type Log = Rc<RefCell<Vec<String>>>;

/// The daemon's calls, each written to LOG; `nobody` has no terminal.
fn stand(log: &Log) -> Daemon {
    let (a, b, c) = (log.clone(), log.clone(), log.clone());
    Daemon {
        type_in: Box::new(move |p, t| match p.to_string().contains("nobody") {
            true => Err(format!("{p} has no terminal")),
            false => Ok(a.borrow_mut().push(format!("type {p} {t}"))),
        }),
        fork: Box::new(move |p, path| {
            b.borrow_mut().push(format!("fork {p} {path}"));
            Ok(party("w1-copy"))
        }),
        end: Box::new(move |p| Ok(c.borrow_mut().push(format!("end {p}")))),
    }
}

fn turns(log: &Log) -> Turns {
    Turns::new(stand(log), Config::default()).unwrap()
}

fn party(s: &str) -> Value {
    Value::text(s)
}

fn tmp(name: &str) -> String {
    let d = std::env::temp_dir().join(format!("turns-test-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join("t.jsonl").display().to_string()
}

fn append(path: &str, lines: &[serde_json::Value]) {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
    for l in lines {
        writeln!(f, "{l}").unwrap();
    }
}

fn claude(id: &str, text: &str, out: i64) -> serde_json::Value {
    json!({"type": "assistant", "timestamp": "2026-11-26T13:35:48.123Z", "message": {"id": id, "content": [{"type": "text", "text": text}], "usage": {"input_tokens": 3, "cache_read_input_tokens": 900, "cache_creation_input_tokens": 40, "output_tokens": out}}})
}

#[test]
fn a_claude_turn_gives_its_passages_once_each_message_counted_once_and_the_waiting_word_its_answer() {
    let path = tmp("claude");
    append(&path, &[claude("old", "before the follow", 1)]);
    let log = Log::default();
    let mut t = turns(&log);
    let w1 = party("w1");
    let as_instance = Value::Inst(Instance { ty: "std.hooks.Party".into(), args: vec![w1.clone()] });
    t.follow(&as_instance, "claude-code", &path).unwrap();
    let Reply::Later = t.carry(DELIVER, &[party("him"), w1.clone(), Value::text("look at the plan")], 7) else { panic!("waits for the turn") };
    assert_eq!(*log.borrow(), [format!("type {w1} look at the plan")]);
    let shown = t.ready().1;
    assert_eq!(shown, [event(SHOWN, vec![w1.clone(), key("look at the plan")])]);
    let sidechain = json!({"type": "assistant", "isSidechain": true, "message": {"id": "s", "content": [{"type": "text", "text": "a subagent's"}]}});
    append(&path, &[claude("m1", "the plan holds", 5), claude("m1", "", 9), sidechain, json!({"type": "user", "message": {"content": "x"}}), claude("m2", "done", 2)]);
    t.turn_ended(&w1).unwrap();
    let (answers, ev) = t.ready();
    let spoke: Vec<_> = ev.iter().filter(|e| e.ty == SPOKE).collect();
    assert_eq!(spoke.len(), 2, "two passages; the line before the follow, the empty one and the subagent's are none");
    let Value::Text(id) = &spoke[0].args[1] else { panic!() };
    assert_eq!(t.carry(TEXT_OF, &[Value::text(id)], 8), Reply::Answer { value: Value::text("the plan holds"), costs: vec![], events: vec![] });
    let usage: Vec<_> = ev.iter().filter(|e| e.ty == USAGE).map(|e| (e.args[2].to_string(), e.args[3].clone())).collect();
    assert_eq!(usage.len(), 8, "four kinds for each of two messages: {usage:?}");
    assert!(usage.contains(&(Value::text("output").to_string(), Value::Int(9))), "m1 at its last line's count");
    assert_eq!(ev.iter().find(|e| e.ty == USAGE).unwrap().args[4], Value::Int(transcript::minute("2026-11-26T13:35:48Z").unwrap()));
    assert_eq!(answers, [(7, Reply::Answer { value: Value::text("done"), costs: vec![], events: vec![] })], "the turn's last passage");
}

#[test]
fn a_codex_turn_counts_each_request_with_cached_reads_and_reasoning_apart() {
    let path = tmp("codex");
    std::fs::write(&path, "").unwrap();
    let mut t = turns(&Log::default());
    let c1 = party("c1");
    t.follow(&c1, "codex", &path).unwrap();
    let count = |i: i64| json!({"type": "event_msg", "timestamp": "2026-11-26T10:00:00Z", "payload": {"type": "token_count", "info": {"last_token_usage": {"input_tokens": 100 * i, "cached_input_tokens": 60, "output_tokens": 30, "reasoning_output_tokens": 10}}}});
    let said = json!({"type": "response_item", "payload": {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "done"}]}});
    append(&path, &[count(1), count(2), said]);
    t.turn_ended(&c1).unwrap();
    let ev = t.ready().1;
    let n = |kind: &str| ev.iter().filter(|e| e.ty == USAGE && e.args[2] == Value::text(kind)).map(|e| e.args[3].clone()).collect::<Vec<_>>();
    assert_eq!((n("input"), n("cached_read"), n("output"), n("reasoning")), (vec![Value::Int(40), Value::Int(140)], vec![Value::Int(60); 2], vec![Value::Int(20); 2], vec![Value::Int(10); 2]), "two requests in one minute, each counted");
    assert_eq!(ev.iter().filter(|e| e.ty == SPOKE).count(), 1);
}

/// A turn of the rollout agy-rpc wrote in its own test's ACP leg (agy-rpc-test.py, agent gem1, on a stand-in agy;
/// agy-rpc at ce78b4766): read as Codex's, its cache writes counted apart from the input that holds them.
#[test]
fn an_agy_turn_is_read_from_agy_rpcs_rollout_with_cache_writes_apart() {
    assert!(Turns::reads("agy"), "an agy party is started, not refused as deaf");
    let path = tmp("agy");
    std::fs::write(&path, "").unwrap();
    let mut t = turns(&Log::default());
    let g = party("gem1");
    t.follow(&g, "agy", &path).unwrap();
    std::fs::write(&path, include_str!("../testdata/agy-rollout.jsonl")).unwrap();
    t.turn_ended(&g).unwrap();
    let ev = t.ready().1;
    let n = |kind: &str| ev.iter().filter(|e| e.ty == USAGE && e.args[2] == Value::text(kind)).map(|e| e.args[3].clone()).collect::<Vec<_>>();
    let counts = [n("input"), n("cached_read"), n("cache_write"), n("output"), n("reasoning")];
    assert_eq!(counts, [90, 40, 10, 20, 5].map(|k| vec![Value::Int(k)]), "140 in, 40 of it read and 10 written to the cache; 25 out, 5 of it reasoning");
    let spoke: Vec<_> = ev.iter().filter(|e| e.ty == SPOKE).collect();
    let [s] = &spoke[..] else { panic!("one passage, the assistant's: {spoke:?}") };
    let Value::Text(id) = &s.args[1] else { panic!() };
    assert_eq!(t.carry(TEXT_OF, &[Value::text(id)], 1), Reply::Answer { value: Value::text("hello there"), costs: vec![], events: vec![] });
}

#[test]
fn text_of_reads_followed_transcripts_and_the_plain_files_named_only() {
    let mut t = turns(&Log::default());
    let Reply::Failed(why) = t.carry(TEXT_OF, &[Value::text("t:/etc/passwd:0:10")], 1) else { panic!("refused") };
    assert!(why.contains("no followed transcript"), "{why}");
    let plain = tmp("plain");
    std::fs::write(&plain, "first words\nhis words").unwrap();
    let mut t = Turns::new(stand(&Log::default()), Config { plain: vec![plain.clone()], ..Default::default() }).unwrap();
    assert_eq!(t.carry(TEXT_OF, &[Value::text(&transcript::id(&plain, 12, 9))], 2), Reply::Answer { value: Value::text("his words"), costs: vec![], events: vec![] }, "a plain file's span is its words");
    assert_eq!(transcript::span("t:/a:b/c.jsonl:12:34"), Some(("/a:b/c.jsonl", 12, 34)), "a path with a colon");
}

#[test]
fn a_fork_is_made_joined_and_deleted_and_a_word_to_no_session_fails() {
    let log = Log::default();
    let mut t = turns(&log);
    let w1 = party("w1");
    assert!(matches!(t.carry(DELIVER, &[party("him"), party("nobody"), Value::text("x")], 2), Reply::Later));
    assert!(matches!(&t.ready().0[..], [(2, Reply::Failed(why))] if why.contains("no terminal")), "no session to type into");
    assert!(matches!(t.carry(SPIN_OFF, &[w1.clone()], 3), Reply::Failed(_)), "no transcript to copy");
    let parent = tmp("parent");
    std::fs::write(&parent, "").unwrap();
    t.follow(&w1, "claude-code", &parent).unwrap();
    assert_eq!(t.carry(SPIN_OFF, &[w1.clone()], 3), Reply::Answer { value: party("w1-copy"), costs: vec![], events: vec![] });
    assert_eq!(log.borrow()[0], format!("fork {w1} {parent}"), "the copy started from the parent's transcript");
    let path = tmp("fork");
    std::fs::write(&path, "").unwrap();
    t.follow(&party("w1-copy"), "claude-code", &path).unwrap();
    assert!(matches!(t.carry(DELIVER, &[party("him"), party("w1-copy"), Value::text("go")], 4), Reply::Later));
    assert!(matches!(t.carry(JOIN_BACK, &[party("w1-copy"), w1.clone(), Value::text("it is done")], 5), Reply::Answer { .. }));
    assert!(matches!(t.carry(DELETE_FORK, &[party("w1-copy")], 6), Reply::Answer { .. }));
    let late: Vec<_> = t.ready().0.into_iter().filter_map(|(l, r)| matches!(r, Reply::Failed(_)).then_some(l)).collect();
    assert_eq!(late, [4], "the word waiting on the deleted fork fails");
    let log = log.borrow().clone();
    assert_eq!(log.len(), 4, "{log:?}");
    assert!(log[2].ends_with("it is done") && log[3].starts_with("end "), "{log:?}");
    let path = tmp("silent");
    std::fs::write(&path, "").unwrap();
    t.follow(&w1, "claude-code", &path).unwrap();
    assert!(matches!(t.carry(DELIVER, &[party("him"), w1.clone(), Value::text("go")], 7), Reply::Later));
    t.turn_ended(&w1).unwrap();
    let (answers, _) = t.ready();
    assert!(matches!(&answers[..], [(7, Reply::Failed(why))] if why.contains("no passage read")), "a turn with no passage is no empty answer: {answers:?}");
}

#[test]
fn words_to_an_agent_in_a_turn_are_typed_at_its_end_and_answered_at_the_end_of_the_turn_they_begin() {
    let path = tmp("busy");
    std::fs::write(&path, "").unwrap();
    let log = Log::default();
    let mut t = turns(&log);
    let w1 = party("w1");
    t.follow(&w1, "claude-code", &path).unwrap();
    t.turn_began(&w1).unwrap();
    assert!(matches!(t.carry(DELIVER, &[party("him"), w1.clone(), Value::text("first")], 3), Reply::Later));
    assert!(matches!(t.carry(DELIVER, &[party("him"), w1.clone(), Value::text("second")], 4), Reply::Later));
    assert!(log.borrow().is_empty(), "nothing typed into a turn");
    append(&path, &[claude("m1", "its own work", 1)]);
    t.turn_ended(&w1).unwrap();
    let (answers, ev) = t.ready();
    assert!(answers.is_empty(), "the turn it was in answers neither");
    assert_eq!((log.borrow().clone(), ev.iter().filter(|e| e.ty == SHOWN).count()), (vec![format!("type {w1} first")], 1), "the oldest typed at its end, the next held");
    t.turn_began(&w1).unwrap();
    append(&path, &[claude("m2", "first, done", 1)]);
    t.turn_ended(&w1).unwrap();
    assert_eq!(t.ready().0, [(3, Reply::Answer { value: Value::text("first, done"), costs: vec![], events: vec![] })]);
    assert_eq!(log.borrow().last().unwrap(), &format!("type {w1} second"));
    assert!(matches!(t.carry(NOTIFY, &[w1.clone(), Value::text("rejected: too long")], 5), Reply::Later), "held: it is in the turn word 4 began");
    t.dropped(4);
    t.turn_ended(&w1).unwrap();
    assert_eq!(t.ready().0, [(5, Reply::Answer { value: Value::Unit, costs: vec![], events: vec![] })], "the notice answered once typed; dropped line 4 answered no more");
    assert_eq!(log.borrow().last().unwrap(), &format!("type {w1} rejected: too long"));
    assert!(matches!(t.carry(NOTIFY, &[party("nobody"), Value::text("x")], 6), Reply::Later));
    assert!(matches!(&t.ready().0[..], [(6, Reply::Failed(_))]), "no session to type into");
}

#[test]
fn words_to_a_session_no_hook_has_named_yet_are_typed_and_answered_at_the_end_of_its_first_turn() {
    let log = Log::default();
    let mut t = turns(&log);
    let w1 = party("w1");
    assert!(matches!(t.carry(DELIVER, &[party("him"), w1.clone(), Value::text("first words")], 3), Reply::Later));
    assert!(matches!(t.carry(NOTIFY, &[w1.clone(), Value::text("a notice")], 4), Reply::Later));
    assert_eq!(*log.borrow(), [format!("type {w1} first words")], "typed at once; the notice held for the turn they begin");
    let path = tmp("first");
    std::fs::write(&path, "").unwrap();
    t.follow(&w1, "claude-code", &path).unwrap();
    t.turn_began(&w1).unwrap();
    append(&path, &[claude("m1", "hello", 1)]);
    t.turn_ended(&w1).unwrap();
    let answered = [(3, Reply::Answer { value: Value::text("hello"), costs: vec![], events: vec![] }), (4, Reply::Answer { value: Value::Unit, costs: vec![], events: vec![] })];
    assert_eq!(t.ready().0, answered, "its first turn's reply; the notice once typed at that turn's end");
    assert_eq!(log.borrow().last().unwrap(), &format!("type {w1} a notice"));
}

#[test]
fn a_push_and_a_codex_packet_wait_out_a_turn_and_an_interrupting_word_or_a_steer_goes_into_it_at_once() {
    let path = tmp("delivery");
    std::fs::write(&path, "").unwrap();
    let log = Log::default();
    let mut t = turns(&log);
    let w1 = party("w1");
    t.follow(&w1, "claude-code", &path).unwrap();
    assert!(matches!(t.carry(PUSH, &[w1.clone(), Value::text("[coord delivery] one")], 3), Reply::Later));
    assert_eq!(*log.borrow(), [format!("type {w1} [coord delivery] one")], "idle: typed at once");
    assert_eq!(t.ready().0, [(3, Reply::Answer { value: Value::Unit, costs: vec![], events: vec![] })], "answered once typed");
    t.turn_began(&w1).unwrap();
    assert!(matches!(t.carry(START_TURN, &[w1.clone(), Value::text("CONTEXT two")], 4), Reply::Later));
    assert!(matches!(t.carry(PUSH_NOW, &[w1.clone(), Value::text("stop: red")], 5), Reply::Later));
    assert!(matches!(t.carry(STEER, &[w1.clone(), Value::text("and this")], 6), Reply::Later));
    assert_eq!(log.borrow()[1..], [format!("type {w1} stop: red"), format!("type {w1} and this")], "in the turn: only the interrupting words typed");
    let unit = || Reply::Answer { value: Value::Unit, costs: vec![], events: vec![] };
    assert_eq!(t.ready().0, [(5, unit()), (6, unit())]);
    t.turn_ended(&w1).unwrap();
    assert_eq!(log.borrow().last().unwrap(), &format!("type {w1} CONTEXT two"), "the packet held to the turn's end");
    assert_eq!(t.ready().0, [(4, unit())]);
    assert!(matches!(t.carry(PUSH_NOW, &[party("nobody"), Value::text("x")], 7), Reply::Later));
    assert!(matches!(&t.ready().0[..], [(7, Reply::Failed(_))]), "no session to type into");
    assert!(t.effects().iter().any(|(e, _)| e == "std.bridges.codex.Codex-Steer"), "named as std/bridges/codex.docket declares it");
    // the owner's word to an agy session, one this driver reads no transcript of: typed at once
    let g1 = party("g1");
    assert!(t.effects().iter().any(|(e, _)| e == "std.bridges.agy.Agy-Kick"), "named as std/bridges/agy.docket declares it");
    let n = log.borrow().len();
    assert!(matches!(t.carry(AGY_KICK, &[g1.clone(), Value::text("[coord delivery, from the owner] go")], 8), Reply::Later));
    assert_eq!(log.borrow()[n..], [format!("type {g1} [coord delivery, from the owner] go")], "typed at once when no turn runs");
    assert_eq!(t.ready().0, [(8, unit())]);
}
