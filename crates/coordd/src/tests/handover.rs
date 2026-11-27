//! One message for a hand-over of several deliveries (`render::handover`, `handover.tmpl`, 0.2 line 211), as coordd
//! makes it from a record and the session's transcript gives it back: consult-3's points (N-807.p112).

use super::{rendered_all, Shared};
use crate::core::{Core, Job, Live};
use docket_machine::{Input, Text};
use serde_json::{json, Value};

/// A coord of lead, w2 and w3 (Claude Code agents), w2's session up and STATE (`idle`, `working`), its jobs to the
/// receiver.
fn coord(tag: &str, state: &str) -> (std::path::PathBuf, Core, tokio::sync::mpsc::UnboundedReceiver<Job>) {
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let coord = std::env::temp_dir().join(format!("docket-handover-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&coord);
    std::fs::create_dir_all(&coord).unwrap();
    let reg: String = [("AGT-001", "lead", "owner"), ("AGT-002", "w2", "lead"), ("AGT-003", "w3", "owner")].iter().map(|(id, n, p)| format!("{}\n", json!({"ev": "add", "id": id, "name": n, "parent": p, "kind": "claude", "model": "m", "at": "x"}))).collect();
    std::fs::write(coord.join("registry.jsonl"), reg).unwrap();
    let mut core = Core::load(&coord.display().to_string(), &base);
    core.load_tickets();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    core.jobs = Some(tx);
    let w2 = core.agents.get_mut("w2").unwrap();
    w2.live = Live { pid: Some(std::process::id() as i32), state: state.into(), turn: 1, ..Default::default() };
    (coord, core, rx)
}

/// A thread BY opens with TEXT, naming w2: its number (below the hidden ones, the delivery program's among them).
fn open(core: &mut Core, by: &str, text: &str) -> u64 {
    core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: by.into(), form: json!({"to": by, "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": by}, "follows": null}), text: Some(Text::New(text.into())), names: vec!["w2".into()], told: vec![] }).unwrap();
    *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0
}

type Handed = (Vec<String>, Vec<(docket_machine::Delivery, docket_machine::TicketView)>, Option<String>, Vec<String>, bool, Vec<String>);

/// The channel job the last hand-over sent.
fn channel_job(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Job>) -> Handed {
    loop {
        match rx.try_recv().expect("a channel job") {
            Job::Channel { ids, tickets, replies_on, again, into, rest, .. } => return (ids, tickets, replies_on, again, into, rest),
            _ => continue,
        }
    }
}

/// A hand-over round for w2: the delivery events raised, then what its messages hold handed.
fn hand(core: &mut Core) {
    core.raise_delivery();
    core.hand_over("w2");
}

/// Three threads handed to an idle agent at once: one message, one head, each thread under `#ID · its head`, one foot
/// naming each id for the reply tool; the tag's id holds the three, and the transcript's entry with that tag makes
/// each given once, at that place (consult-3 N-807.p112, 2).
#[tokio::test]
async fn three_threads_handed_at_once_are_one_message_each_given_where_it_is() {
    let (dir, mut core, mut rx) = coord("three", "idle");
    let ts = [open(&mut core, "lead", "the fixture is in place"), open(&mut core, "w3", "the second run is green"), open(&mut core, "lead", "stop the build, the runner is full")];
    let names: Vec<String> = ts.iter().map(|t| core.tickets.m.id(*t)).collect();
    hand(&mut core);
    let (ids, tickets, replies_on, again, into, rest) = channel_job(&mut rx);
    assert_eq!((ids.len(), replies_on.as_deref(), into, rest.len()), (3, None, false, 0), "{ids:?}");
    let shared = Shared::new(core);
    let out = rendered_all(&shared, "w2", &tickets, replies_on.as_deref(), &again, into, false, &rest, &[]).await;
    assert_eq!(out.len(), 1, "{out:?}");
    let (text, meta) = &out[0];
    assert_eq!(meta, &json!({"from": "coord", "kind": "handover", "id": ids.join(" "), "keys": ids}));
    // as it goes on the session's socket (`run_job`, render::event): its meta all strings, its keys beside it, so a
    // channel.py from before build 27 passes on a meta Claude Code takes (N-1396 p71)
    let ev = crate::render::event(text.clone(), meta.clone());
    assert_eq!((&ev["meta"], &ev["keys"]), (&json!({"from": "coord", "kind": "handover", "id": ids.join(" ")}), &json!(ids)));
    assert!(text.starts_with("3 threads, one after another.\n\n#"), "{text}");
    for (n, head) in names.iter().zip(["the fixture is in place", "the second run is green", "stop the build, the runner is full"]) {
        assert!(text.contains(&format!("\n\n#{n} · {head}\n")), "{n}: {text}");
    }
    let answers: Vec<String> = tickets.iter().map(|(d, _)| d.id.clone()).collect();
    assert!(text.contains(&format!("\n\nAnswer each thread with the reply tool, to its id: {}. Or start a line", answers.join(", "))), "{text}");
    assert_eq!((text.matches("reply tool, to its id").count(), text.matches("You were given several things at once").count()), (1, 0), "{text}");
    // the session's entry as Claude Code writes the tag: every id read from it, each given once, at that place
    let tag = format!("<channel source=\"coord\" from=\"coord\" kind=\"handover\" id=\"{}\">\n{text}\n</channel>", meta["id"].as_str().unwrap());
    assert_eq!(crate::follow::channel_ids(&tag), ids);
    let mut core = shared.lock().await;
    core.handed("w2", &ids, true, "channel");
    let file = dir.join("w2.jsonl").display().to_string();
    let at: Vec<(String, String, u64, u64)> = crate::follow::channel_ids(&tag).into_iter().map(|id| (id, file.clone(), 40, 900)).collect();
    core.transcript_read("w2", at, vec![], vec![], &[], None, &[], false);
    for (d, _) in &tickets {
        let given: Vec<Option<u64>> = core.tickets.m.tickets[&d.t].given.iter().filter(|g| g.to == "w2" && g.delivery == d.id).map(|g| g.place.as_ref().map(|p| p.offset)).collect();
        assert_eq!(given, [Some(40)], "{}", d.id);
    }
    assert!(core.ticket_due("w2").is_empty(), "{:?}", core.ticket_due("w2"));
    drop(core);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The hand-over's `ok` lost after the session wrote it (consult-3 N-807.p112, 1): the next hand-over gives the batch's
/// own deliveries alone, the same message with the same keys (which channel.py then drops whole, its keys all passed:
/// tests/channel-batch-test.sh), and the thread that came meanwhile goes in the hand-over after it. Each reaches the
/// model once.
#[tokio::test]
async fn a_batch_not_answered_is_handed_again_alone_and_what_came_since_after_it() {
    let (dir, mut core, mut rx) = coord("retry", "idle");
    open(&mut core, "lead", "the fixture is in place");
    open(&mut core, "w3", "the second run is green");
    hand(&mut core);
    let (first, tickets, replies_on, again, into, _) = channel_job(&mut rx);
    assert_eq!(first.len(), 2);
    core.handed("w2", &first, false, "channel");
    let c = open(&mut core, "lead", "stop the build, the runner is full");
    let late = core.tickets.m.id(c);
    hand(&mut core);
    let (retry, tickets2, replies_on2, again2, into2, _) = channel_job(&mut rx);
    assert_eq!(retry, first, "the batch's own deliveries first, and only they");
    // the session waits for its retry: the lead's new post waits for its turn's end (delivery.kind's `arrived`: only
    // his posts and a few of the program's go into a turn, I-1480 c)
    assert_eq!(core.tickets.m.kept("w2").get(&format!("{late}.p1")).map(String::as_str), Some("comes when its turn ends"));
    let shared = Shared::new(core);
    let one = rendered_all(&shared, "w2", &tickets, replies_on.as_deref(), &again, into, false, &[], &[]).await;
    let two = rendered_all(&shared, "w2", &tickets2, replies_on2.as_deref(), &again2, into2, false, &[], &[]).await;
    assert_eq!((one.len(), two.len()), (1, 1));
    assert_eq!(one[0].1["keys"], two[0].1["keys"], "the same keys, so channel.py drops the second");
    assert!(!two[0].0.contains(&late), "{}", two[0].0);
    let mut core = shared.lock().await;
    core.handed("w2", &retry, true, "channel");
    // that turn over, the thread that came meanwhile goes alone, as one delivery goes
    core.ticket_input(Input::TurnCut { agent: "w2".into() }).unwrap();
    hand(&mut core);
    let (third, ..) = channel_job(&mut rx);
    assert_eq!(third.len(), 1);
    assert!(third[0].starts_with(&format!("{late}.")), "{third:?}");
    drop(core);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The owner's posts on two threads put into a running turn (consult-3 N-807.p112, 3): one `owner` message whose head
/// counts his posts; the lead's post naming its worker waits for the turn's end (delivery.kind's `arrived`, I-1480 c:
/// the text before put it in, apart, consult-3 N-1397.p13, 1).
#[tokio::test]
async fn the_owners_posts_into_a_turn_are_one_message_and_a_leads_post_waits() {
    let (dir, mut core, mut rx) = coord("into", "working");
    crate::core::delivery_tests::his_into(&dir, &mut core);
    let a = open(&mut core, "owner", "give me a number");
    let b = open(&mut core, "owner", "and post it here");
    let l = open(&mut core, "lead", "stop the build, the runner is full");
    let (ia, ib, il) = (core.tickets.m.id(a), core.tickets.m.id(b), core.tickets.m.id(l));
    hand(&mut core);
    let (ids, tickets, replies_on, again, into, rest) = channel_job(&mut rx);
    assert!(into, "{ids:?}");
    assert_eq!(ids, [format!("{ia}.p1"), format!("{ib}.p1")]);
    assert_eq!(core.tickets.m.kept("w2").get(&format!("{il}.p1")).map(String::as_str), Some("comes when its turn ends"));
    let shared = Shared::new(core);
    let out = rendered_all(&shared, "w2", &tickets, replies_on.as_deref(), &again, into, false, &rest, &[]).await;
    let kinds: Vec<&str> = out.iter().map(|(_, m)| m["kind"].as_str().unwrap_or_default()).collect();
    let his: Vec<&(String, Value)> = out.iter().filter(|(_, m)| m["kind"] == "owner").collect();
    assert_eq!(his.len(), 1, "{kinds:?}");
    let (text, meta) = his[0];
    assert!(text.starts_with("The owner wrote 2 messages while you were working."), "{text}");
    assert!(text.contains(&format!("\n\n#{ia} · give me a number\n")) && text.contains(&format!("\n\n#{ib} · and post it here\n")), "{text}");
    assert!(text.ends_with(&format!("Answer each with the reply tool, to its id: {ia}.p1, {ib}.p1.")), "{text}");
    assert_eq!(meta["id"], json!(format!("{ia}.p1 {ib}.p1")));
    // his posts put in, which the turn must answer: named for channel.py beside the meta by delivery, so the reply
    // tool's Nothing is no answer to them and names them alone (paper-research N-412.p2202, 1; consult-3 p2210); the
    // lead's is not
    assert_eq!(meta["his"], json!({format!("{ia}.p1"): [format!("{ia}.p1")], format!("{ib}.p1"): [format!("{ib}.p1")]}));
    let ev = crate::render::event(text.clone(), meta.clone());
    assert!(ev["meta"].get("his").is_none() && ev["his"] == meta["his"], "{ev}");
    assert!(!text.contains("stop the build"), "the lead's post is not in his message: {text}");
    assert_eq!(out.len(), 1, "{kinds:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// For a Codex agent the same hand-over is one `send` with one foot, each part under the head its reader finds the
/// delivery by (`follow::codex_mail`).
#[tokio::test]
async fn a_codex_thread_is_given_the_batch_as_one_text_with_each_head() {
    let (dir, mut core, mut rx) = coord("codex", "idle");
    open(&mut core, "lead", "the fixture is in place");
    open(&mut core, "w3", "the second run is green");
    hand(&mut core);
    let (ids, tickets, replies_on, again, into, rest) = channel_job(&mut rx);
    let shared = Shared::new(core);
    let out = rendered_all(&shared, "w2", &tickets, replies_on.as_deref(), &again, into, true, &rest, &[]).await;
    assert_eq!(out.len(), 1);
    assert_eq!(crate::follow::codex_mail(&out[0].0).0, ids);
    assert_eq!(out[0].0.matches("Answer each thread").count(), 1, "{}", out[0].0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The runner's file of w2's job `test yaaihh · edge5-typing ec7f09cb`, begun at 06:20, its caller gone; read into
/// w2's view as the session is now.
fn read_job(core: &mut Core) {
    let job = coord_types::Job { id: "41233-1763619362".into(), verb: "test".into(), packages: vec!["yaaihh".into()], branch: Some("edge5-typing".into()), commit: Some("ec7f09cb".into()), since: "2026-11-20T06:20:00Z".into(), step: "testing".into(), step_since: "2026-11-20T06:21:00Z".into(), ..Default::default() };
    let coord = core.coord.clone();
    core.set_jobs(vec![crate::jobs::Read { agent: "w2".into(), coord, alive: false, written: "2026-11-20T06:29:00Z".into(), job }], None);
}

/// The notice of a job stopped with the session (N-1334; hs-ls p6): in a hand-over of several deliveries, its own
/// line right after the head, before any entry and in no foot; in a hand-over of one, before that delivery's text,
/// a blank line between. It rides only in a hand-over that was due, and goes after the agent's next turn's end.
#[tokio::test]
async fn a_job_stopped_with_the_session_is_told_under_the_head_or_before_the_one_delivery() {
    let (dir, mut core, mut rx) = coord("job-notice", "idle");
    // its session restarted at 06:30, after the job began and its last turn ended
    let w2 = core.agents.get_mut("w2").unwrap();
    (w2.live.started, w2.live.turn_ended) = ("2026-11-20T06:30:00Z".into(), Some("2026-11-20T06:10:00Z".into()));
    read_job(&mut core);
    let notice = format!("Your job `test yaaihh · edge5-typing ec7f09cb` was stopped at {} with your session; it did not finish.", crate::jobs::clock("2026-11-20T06:30:00Z"));
    let first = open(&mut core, "lead", "the fixture is in place");
    open(&mut core, "w3", "the second run is green");
    let first = core.tickets.m.id(first);
    hand(&mut core);
    let (ids, tickets, replies_on, again, into, rest) = channel_job(&mut rx);
    let shared = Shared::new(core);
    let out = rendered_all(&shared, "w2", &tickets, replies_on.as_deref(), &again, into, false, &rest, &[]).await;
    assert_eq!(out.len(), 1, "{out:?}");
    let text = &out[0].0;
    assert!(text.starts_with(&format!("2 threads, one after another.\n{notice}\n\n#{first} · the fixture is in place\n")), "{text}");
    assert_eq!(text.matches("was stopped at").count(), 1, "{text}");
    // one delivery: the notice, a blank line, then its text
    let mut core = shared.lock().await;
    core.handed("w2", &ids, true, "channel");
    core.ticket_input(Input::TurnCut { agent: "w2".into() }).unwrap();
    open(&mut core, "lead", "stop the build, the runner is full");
    hand(&mut core);
    let (_, one, replies_on, again, into, rest) = channel_job(&mut rx);
    drop(core);
    let out = rendered_all(&shared, "w2", &one, replies_on.as_deref(), &again, into, false, &rest, &[]).await;
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0].0.starts_with(&format!("{notice}\n\n")) && out[0].0.contains("stop the build, the runner is full"), "{}", out[0].0);
    // its next turn's end: told no more
    let mut core = shared.lock().await;
    assert!(core.end_turn("w2").is_some());
    read_job(&mut core);
    drop(core);
    let out = rendered_all(&shared, "w2", &one, replies_on.as_deref(), &again, into, false, &rest, &[]).await;
    assert!(!out[0].0.contains("was stopped at"), "{}", out[0].0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The hand-over runs on a change, not every round (paper-research N-1446 p3381): with nothing new, a round hands
/// nothing and leaves `hand_key` as it was, so the follow round skips the next; a thread opened to w2 moves the key and
/// the round then hands it; what was handed being on its way moves it again; a start wait running out moves it with
/// no line written.
#[tokio::test]
async fn the_hand_over_key_moves_only_when_something_it_reads_does() {
    let (dir, mut core, mut rx) = coord("key", "idle");
    // what a new coord raises once (its first `free`s) settled first
    for _ in 0..3 {
        for n in core.to_hand_over() {
            core.hand_over(&n);
        }
    }
    assert!(rx.try_recv().is_err(), "a new coord hands nothing");
    let k = core.hand_key();
    assert!(core.to_hand_over().is_empty(), "nothing new: nothing handed");
    assert_eq!(core.hand_key(), k, "a round that hands nothing leaves the key");
    assert_eq!(core.hand_key(), k, "and looking again does too");
    open(&mut core, "lead", "the fixture is in place");
    let k2 = core.hand_key();
    assert_ne!(k2, k, "a thread opened to w2 moves the key");
    assert_eq!(core.to_hand_over(), ["w2"]);
    core.hand_over("w2");
    let _ = channel_job(&mut rx);
    assert_ne!(core.hand_key(), k2, "what is on its way to w2 moves it");
    // w3's new process, its start hook not yet come: 60 s, then past START_WAIT, with nothing written meanwhile
    let now = std::time::Instant::now();
    core.agents.get_mut("w3").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), turn: 1, awaiting_start: now.checked_sub(std::time::Duration::from_secs(60)), ..Default::default() };
    let k3 = core.hand_key();
    assert_eq!(core.hand_key(), k3);
    core.agents.get_mut("w3").unwrap().live.awaiting_start = now.checked_sub(std::time::Duration::from_secs(121));
    assert_ne!(core.hand_key(), k3, "a start wait run out moves it");
    let _ = std::fs::remove_dir_all(&dir);
}
