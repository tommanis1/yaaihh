//! The message runtime's coordd half (0.2 head 2, delivery.rs): the delivery events raised from what coordd knows of
//! an agent, the program's `deliver` handed as one message with the rest named at its foot and its digest, and a run's
//! `next` on the text before answered with no call (consult-3, N-1397.p11; paper-research N-1397.p9; I-1480). What a
//! record from before runs, a lead's post into its worker's turn among it, runs on dm's copy of that text ([`before`]).

use crate::core::{Core, Job, Live, Who};
use docket_machine::{Delivery, Input, Loc, PostKind, Text, TicketView, WATCHER};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedReceiver;

pub(crate) type Handed = (Vec<String>, Vec<(Delivery, TicketView)>, Option<String>, Vec<String>, bool, Vec<String>);

/// A coord of lead, w2 and w3 (Claude Code agents), w2's session up and STATE: the coord, agent-base, the core, its
/// jobs.
pub(crate) fn coord(tag: &str, state: &str) -> (PathBuf, String, Core, UnboundedReceiver<Job>) {
    coord_named(tag, state, ["lead", "w2", "w3"])
}

/// [`coord`] with its agents named NAMES: the lead, the agent below it whose session is up, the other.
fn coord_named(tag: &str, state: &str, names: [&str; 3]) -> (PathBuf, String, Core, UnboundedReceiver<Job>) {
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let coord = std::env::temp_dir().join(format!("docket-desk-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&coord);
    std::fs::create_dir_all(&coord).unwrap();
    let reg: String = [("AGT-001", names[0], "owner"), ("AGT-002", names[1], names[0]), ("AGT-003", names[2], "owner")].iter().map(|(id, n, p)| format!("{}\n", json!({"ev": "add", "id": id, "name": n, "parent": p, "kind": "claude", "model": "m", "at": "x"}))).collect();
    std::fs::write(coord.join("registry.jsonl"), reg).unwrap();
    let (core, rx) = load(&coord, &base, state);
    (coord, base, core, rx)
}

/// A coordd started on COORD's record, w2's session (AGT-002's) up and STATE in its turn 1.
pub(crate) fn load(coord: &Path, base: &str, state: &str) -> (Core, UnboundedReceiver<Job>) {
    load_with(coord, base, state, false)
}

/// [`load`], RUNNING when w2's session ran on while coordd was down (`recover` found its process), so the start does
/// not cut its turn; else it does, as for a process that ended meanwhile (`load_tickets`).
pub(crate) fn load_with(coord: &Path, base: &str, state: &str, running: bool) -> (Core, UnboundedReceiver<Job>) {
    let mut core = Core::load(&coord.display().to_string(), base);
    let up = |core: &mut Core| core.agents.values_mut().find(|a| a.id == "AGT-002").unwrap().live = Live { pid: Some(std::process::id() as i32), state: state.into(), turn: 1, ..Default::default() };
    if running {
        up(&mut core);
    }
    core.load_tickets();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    core.jobs = Some(tx);
    up(&mut core);
    (core, rx)
}

/// A thread BY opens with TEXT, naming NAMES: its post's id. Its form is an agent's message's (`Sent`), ended at once as
/// Between (thread.kind); [`open_as`] takes another.
pub(crate) fn open(core: &mut Core, by: &str, names: &[&str], text: &str) -> String {
    open_as(core, by, names, text, json!({"to": by, "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": by}, "follows": null}))
}

/// [`open`] with the thread's FORM (proto-reader N-1446 p782).
fn open_as(core: &mut Core, by: &str, names: &[&str], text: &str, form: Value) -> String {
    core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: by.into(), form, text: Some(Text::New(text.into())), names: names.iter().map(|n| n.to_string()).collect(), told: vec![] }).unwrap();
    let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
    format!("{}.p1", core.tickets.m.id(t))
}

pub(crate) fn t_of(id: &str) -> u64 {
    docket_machine::parse_delivery(id).unwrap().0
}

/// What w2's hand-over sends now, if anything.
pub(crate) fn channel(core: &mut Core, rx: &mut UnboundedReceiver<Job>) -> Option<Handed> {
    channel_as_sent(core, rx).map(|(handed, _)| handed)
}

/// [`channel`] with the digest its job carries, which `run_job` gives `rendered_all` (consult-3 N-1481 p126).
fn channel_as_sent(core: &mut Core, rx: &mut UnboundedReceiver<Job>) -> Option<(Handed, Vec<String>)> {
    core.hand_over("w2");
    while let Ok(j) = rx.try_recv() {
        if let Job::Channel { ids, tickets, replies_on, again, into, rest, digest, .. } = j {
            return Some(((ids, tickets, replies_on, again, into, rest), digest));
        }
    }
    None
}

/// Why ID is kept from w2, if it is.
pub(crate) fn kept(core: &Core, id: &str) -> Option<String> {
    core.tickets.m.kept("w2").get(id).cloned()
}

pub(crate) fn working(core: &mut Core) {
    core.agents.get_mut("w2").unwrap().live.state = "working".into();
}

/// w2's turn over: its end taken.
pub(crate) fn idle(core: &mut Core) {
    let a = core.agents.get_mut("w2").unwrap();
    a.ended_turn = a.live.turn;
    a.live.state = "idle".into();
}

/// The `given` lines of the record of COORD.
fn given_lines(coord: &Path) -> Vec<Value> {
    std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap_or_default().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["ev"] == "given").collect()
}

/// Where w2 was given ID, each time: the offset in its transcript, None for no place.
fn given_at(core: &Core, id: &str) -> Vec<Option<u64>> {
    core.tickets.m.tickets[&t_of(id)].given.iter().filter(|g| g.to == "w2" && g.delivery == id).map(|g| g.place.as_ref().map(|p| p.offset)).collect()
}

/// A post of KIND by BY on thread T naming TO, "@TO TEXT" (`coord post --fyi`, `--ask`, `--hold`): an fyi is pulled,
/// given with TO's next message by its first line (I-1480 rule 2), unless it answers an ask of TO's to BY; an ask
/// pushes; a hold names whoever works on its item (dm `Input::Post`). Its id.
fn typed(core: &mut Core, t: u64, by: &str, to: &[&str], kind: PostKind, text: &str) -> String {
    let text = to.iter().map(|a| format!("@{a} ")).collect::<String>() + text;
    core.ticket_input(Input::Post { t, by: by.into(), text: Text::New(text), names: to.iter().map(|a| a.to_string()).collect(), told: vec![], inform: vec![], answers: None, kind: Some(kind) }).unwrap();
    format!("{}.p{}", core.tickets.m.id(t), core.tickets.m.tickets[&t].last())
}

/// `free` for an idle agent with three agents' posts waiting, each naming it (an `@` its writer typed: it goes to it
/// now): one message of all three, oldest first, with no call and nothing named at its foot (I-1480 c). While it is on
/// its way, and once given until its turn ends, nothing more is raised.
#[tokio::test]
async fn free_hands_what_goes_to_it_now_as_one_message_with_no_call() {
    let (dir, _, mut core, mut rx) = coord("give", "idle");
    let a = open(&mut core, "lead", &["w2"], "the fixture is in place");
    let b = open(&mut core, "w3", &["w2"], "is the second run green?");
    let c = open(&mut core, "w3", &["w2"], "taken");
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert!(!core.tickets.m.deciding("w2") && core.ticket_due(WATCHER).is_empty(), "no next");
    let (ids, tickets, replies_on, again, into, rest) = channel(&mut core, &mut rx).expect("a hand-over");
    assert_eq!((ids.clone(), into, rest.clone()), (vec![a, b, c], false, Vec::<String>::new()));
    assert!(core.to_hand_over().is_empty(), "on its way");
    core.handed("w2", &ids, true, "channel");
    assert!(core.to_hand_over().is_empty(), "given, its turn not begun: nothing more");
    let shared = crate::Shared::new(core);
    let out = crate::rendered_all(&shared, "w2", &tickets, replies_on.as_deref(), &again, into, false, &rest, &[]).await;
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(!out[0].0.contains("Waiting for you still") && !out[0].0.contains("New on the threads and items you are on"), "{}", out[0].0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// What is pulled wakes nothing and goes with the next message by first lines (I-1480; the brief's test 4, paper-research
/// N-1481 p25, p26): twenty fyis by w3 on w2's own thread, w2 idle, raise no `free`. w3's ask does, and its message's
/// foot names that thread once, with its newest three first lines and "and 17 more" (`rest.tmpl`, hs-ls p28). All
/// twenty are given with it, one delivery up to the last, so the next message's digest lists only what came since.
#[tokio::test]
async fn fyis_on_an_idle_agents_own_thread_wake_nothing_and_go_with_its_next_message_as_one_line() {
    let (dir, _, mut core, mut rx) = coord("digest", "idle");
    // w2's thread to its lead (one naming nobody is refused): w3's fyis there answer no ask of w2's to w3, and pull
    let own = t_of(&open(&mut core, "w2", &["lead"], "the replay plan"));
    // fyi K by w3 on w2's thread: its line in the digest
    let step = |core: &mut Core, k: u32| format!("{}, w3: @w2 step {k} replayed", typed(core, own, "w3", &["w2"], PostKind::Fyi, &format!("step {k} replayed")));
    let lines: Vec<String> = (1..=20).map(|k| step(&mut core, k)).collect();
    let pulled = core.tickets.m.waiting("w2");
    assert_eq!(pulled.len(), 20, "{pulled:?}");
    assert!(pulled.iter().all(|i| i["push"] != json!(true)), "{pulled:?}");
    core.raise_delivery();
    assert!(core.to_hand_over().is_empty() && core.tickets.m.delivery_queue("w2").is_empty(), "a pulled post wakes nothing");
    assert_eq!(frees(&dir), 0, "no `free` is raised for it, to hand nothing");
    let id = core.tickets.m.id(own);
    let label = docket_machine::label::title(&core.tickets.m, &core.tickets.m.tickets[&own]);
    // w3's ask wakes w2: the message carries it, its digest the twenty
    let ask = open(&mut core, "w3", &["w2"], "is the second run green?");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let digest = core.hand_now("w2", false).digest;
    assert_eq!(digest.len(), 20, "{digest:?}");
    let drawn = json!([{"id": id, "label": label, "changes": "20 changes", "newest": format!("{}; {}; {}", lines[19], lines[18], lines[17]), "more": 17}]);
    assert_eq!(Value::from(core.digest_named("w2", &digest)), drawn);
    let ((ids, tickets, replies_on, again, into, rest), sent) = channel_as_sent(&mut core, &mut rx).expect("handed");
    assert_eq!(sent, digest, "the job carries the digest hand_now gave");
    let last = format!("{id}.p{}", core.tickets.m.tickets[&own].last());
    assert_eq!(ids, [ask.clone(), last.clone()], "the ask, and the digest's thread up to its last post");
    core.handed("w2", &ids, true, "channel");
    assert!(core.tickets.m.waiting("w2").is_empty(), "all twenty given with the ask");
    assert_eq!(given_at(&core, &last).len(), 1);
    let shared = crate::Shared::new(core);
    let out = crate::rendered_all(&shared, "w2", &tickets, replies_on.as_deref(), &again, into, false, &rest, &sent).await;
    assert_eq!(out.len(), 1, "{out:?}");
    let foot = format!("\n\nNew on the threads and items you are on, your own first, the newest first in each:\n{id} ({label}), 20 changes: {}; {}; {}; and 17 more\nOpen the ones you need with coord show ID.", lines[19], lines[18], lines[17]);
    assert!(out[0].0.ends_with(&foot), "{}", out[0].0);
    // its turn ends; two fyis more and an ask: the digest is those two alone
    let mut core = Arc::try_unwrap(shared.0).ok().expect("one owner").into_inner();
    core.begin_turn("w2");
    end_turn(&mut core, &dir, "it is green");
    let later = [step(&mut core, 21), step(&mut core, 22)];
    assert!(core.to_hand_over().is_empty());
    open(&mut core, "w3", &["w2"], "and the third run?");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let digest = core.hand_now("w2", false).digest;
    let drawn = json!([{"id": id, "label": label, "changes": "2 changes", "newest": format!("{}; {}", later[1], later[0]), "more": null}]);
    assert_eq!(Value::from(core.digest_named("w2", &digest)), drawn);
    let _ = std::fs::remove_dir_all(&dir);
}

/// An ask to an idle agent wakes it once, with the ask (I-1480; the brief's test 2, paper-research N-1481 p1): w3's
/// typed ask on its thread to the lead (`coord post --ask w2`) names w2, pushes, and raises one `free`; the message is
/// the ask alone, given once. The rounds after it, and its turn's end, raise nothing more.
#[test]
fn an_ask_to_an_idle_agent_wakes_it_once_with_the_ask() {
    let (dir, _, mut core, mut rx) = coord("ask-once", "idle");
    let t = t_of(&open(&mut core, "w3", &["lead"], "the replay plan"));
    let ask = typed(&mut core, t, "w3", &["w2"], PostKind::Ask { to: vec!["w2".into()] }, "is the second run green?");
    let waiting: Vec<(Value, Value)> = core.tickets.m.waiting("w2").iter().map(|i| (i["id"].clone(), i["push"].clone())).collect();
    assert_eq!(waiting, [(json!(ask), json!(true))]);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, _, into, rest) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!((ids.clone(), into, rest), (vec![ask.clone()], false, Vec::<String>::new()));
    core.handed("w2", &ids, true, "channel");
    assert!(core.to_hand_over().is_empty() && core.to_hand_over().is_empty(), "given: nothing more");
    core.begin_turn("w2");
    end_turn(&mut core, &dir, "@w3 it is green");
    assert!(core.to_hand_over().is_empty(), "its turn's end: nothing more");
    assert_eq!((frees(&dir), given_at(&core, &ask).len()), (1, 1), "one free, the ask given once");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A hold on a working agent's item reaches it at its turn's end, not in the turn (I-1480; the brief's test 3): w2 is
/// in the turn its `coord item start` raised the seek in, and w3's hold on that item (`coord post --hold`), on w3's
/// thread to the lead, names w2 as the one who works on it and pushes: it is kept, "comes when its turn ends", and
/// nothing goes into the turn. The seek answered and the turn over, the hold goes, alone.
#[test]
fn a_hold_on_a_working_agents_item_reaches_it_at_its_turns_end_not_in_the_turn() {
    let (dir, _, mut core, mut rx) = coord("hold-end", "working");
    let (id, _) = item(&mut core, "faster replay");
    assert!(core.item_start(&Who::Agent("w2".into()), &id).ok);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, tickets, _, _, into, _) = channel(&mut core, &mut rx).expect("the seek");
    assert!(into, "the seek goes into its turn");
    core.handed("w2", &ids, true, "channel");
    let t = t_of(&open(&mut core, "w3", &["lead"], "the merge"));
    let hold = typed(&mut core, t, "w3", &[], PostKind::Hold { on: id.clone() }, "hold the merge: the lock fails");
    assert_eq!(core.tickets.m.tickets[&t].posts.last().unwrap().names, ["w2"], "it names whoever works on the item");
    assert!(core.to_hand_over().is_empty(), "nothing goes into the turn");
    assert_eq!(kept(&core, &hold).as_deref(), Some("comes when its turn ends"));
    core.ticket_input(Input::Answer { t: tickets[0].0.t, by: "w2".into(), head: json!({"head": "Nothing", "searched": "replay in docket-machine"}), text: Text::New("as said".into()) }).unwrap();
    assert!(core.to_hand_over().is_empty(), "still in the turn");
    end_turn(&mut core, &dir, "The seek is answered.");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, _, into, rest) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!((ids, into, rest), (vec![hold], false, Vec::<String>::new()));
    let _ = std::fs::remove_dir_all(&dir);
}

/// What w2 is handed of the owner's two posts on the text OLD names ([`before`] for the text before I-1480 c, else
/// I-1480 c's, [`his_into`]): his first, to it idle with w3's post waiting, then his second, into the turn the first
/// began. For each: the ids, whether into the turn, the ids named at its foot, the digest its job carries, the messages
/// rendered from the job (text, meta), and what is kept from w2.
async fn his_given(old: bool) -> Vec<(Vec<String>, bool, Vec<String>, Vec<String>, Vec<(String, Value)>, std::collections::BTreeMap<String, String>)> {
    let (dir, _, mut core, mut rx) = coord("his-as-before", "idle");
    if old {
        before(&dir, &mut core);
    } else {
        his_into(&dir, &mut core);
    }
    open(&mut core, "w3", &["w2"], "from w3");
    let mut out = Vec::new();
    for text in ["from the owner", "stop and tell me"] {
        let o = open(&mut core, "owner", &["w2"], text);
        assert_eq!(core.to_hand_over(), ["w2"]);
        let ((ids, tickets, replies_on, again, into, rest), digest) = channel_as_sent(&mut core, &mut rx).expect("handed");
        assert_eq!(ids, [o]);
        let shared = crate::Shared::new(core);
        let rendered = crate::rendered_all(&shared, "w2", &tickets, replies_on.as_deref(), &again, into, false, &rest, &digest).await;
        core = Arc::try_unwrap(shared.0).ok().expect("one owner").into_inner();
        core.handed("w2", &ids, true, "channel");
        if !into {
            core.begin_turn("w2");
        }
        out.push((ids, into, rest, digest, rendered, core.tickets.m.kept("w2")));
    }
    let _ = std::fs::remove_dir_all(&dir);
    out
}

/// His posts are given exactly as before (I-1480; the brief's test 5): on I-1480 c's text and on the text before,
/// his post to idle w2 goes at once and alone, w3's waiting named at its foot, and his next goes into the turn it
/// began; the same ids, no digest on either job, the same messages byte for byte, the same meta, the same kept.
#[tokio::test]
async fn his_posts_are_given_byte_for_byte_as_on_the_text_before() {
    let now = his_given(false).await;
    let shape: Vec<(bool, usize, usize, usize)> = now.iter().map(|(_, into, rest, digest, rendered, _)| (*into, rest.len(), digest.len(), rendered.len())).collect();
    assert_eq!(shape, [(false, 1, 0, 1), (true, 0, 0, 1)], "{now:?}");
    assert_eq!(now, his_given(true).await);
}

/// The owner's post to an idle agent with two agents' posts waiting (paper-research N-101 p1127): it goes at once
/// with no call, alone, the two named at its foot; its turn's reply lands on his thread; the two, decided at that
/// `free` and not come in the turn, raise nothing in it, wait for its end, and then go together. The same when its
/// turn begins and ends between two rounds, which no round sees working (one `follow` pass; proto-engine N-1481
/// p162, proto-lead p164): its end owes the `free` that hands the two.
#[test]
fn his_post_goes_at_once_alone_with_no_call_the_rest_named() {
    for seen_working in [true, false] {
        his_post_alone(seen_working);
    }
}

fn his_post_alone(seen_working: bool) {
    let (dir, _, mut core, mut rx) = coord(if seen_working { "his" } else { "his-pass" }, "idle");
    let l = open(&mut core, "w3", &["w2"], "from w3 first");
    let w = open(&mut core, "w3", &["w2"], "from w3");
    let o = open(&mut core, "owner", &["w2"], "from the owner");
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert!(!core.tickets.m.deciding("w2") && core.ticket_due(WATCHER).is_empty(), "no next asked");
    let (ids, _, _, _, into, rest) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!((ids.clone(), into, rest), (vec![o.clone()], false, vec![l.clone(), w.clone()]));
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    if seen_working {
        core.raise_delivery();
        assert!(core.tickets.m.delivery_queue("w2").is_empty() && !core.tickets.m.deciding("w2"), "no arrived for what waited at the free");
    }
    assert_eq!((kept(&core, &l), kept(&core, &w)), (None, None));
    assert_eq!(core.tickets.m.waiting("w2").len(), 2, "both wait still");
    end_turn(&mut core, &dir, "Done.");
    let x = &core.tickets.m.tickets[&t_of(&o)];
    assert_eq!((x.posts.len(), x.posts[1].by.as_str()), (2, "w2"), "the turn's reply lands on his thread");
    assert_eq!(core.to_hand_over(), ["w2"], "its turn's end owes a free (a round saw it working: {seen_working})");
    let (ids, _, _, _, into, rest) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!((ids, into, rest.len()), (vec![l, w], false, 0));
    let _ = std::fs::remove_dir_all(&dir);
}

/// gemini-advisor N-1446 p2365, 1 (line 177568, 06:46:37Z): fresh-lead-3, idle, was handed his 93-byte N-1104.p982 to
/// it with the 23 posts before it on his chat, 17.8 KB, whole, though they only informed it (paper-research's to him,
/// told it as one of the chat). His post goes at once and alone, as ever; what only informs the agent goes with it by
/// first lines, as its digest, as with any message (delivery.kind's `his` and `own` arms; paper-research p2367, 2).
/// Here w3's five posts to him on his thread, each informing w2 and of two lines, then his to w2: the job carries his
/// alone and the five as its digest, its message holds none of their second lines, and all of it is given.
#[tokio::test]
async fn his_post_to_an_idle_agent_brings_the_posts_that_only_inform_it_by_first_lines_not_whole() {
    let (dir, _, mut core, mut rx) = coord("his-digest", "idle");
    let his = t_of(&open(&mut core, "owner", &["w3"], "the morning report"));
    let id = core.tickets.m.id(his);
    let mut notes: Vec<String> = Vec::new();
    for k in 1..=5 {
        let text = format!("@owner build 73, note {k}\nthe whole of note {k}, which w2 need not read");
        core.ticket_input(Input::Post { t: his, by: "w3".into(), text: Text::New(text), names: vec!["owner".into()], told: vec![], inform: vec!["w2".into()], answers: None, kind: None }).unwrap();
        let n = core.tickets.m.tickets[&his].last();
        core.ticket_input(Input::Notified { t: his, event: "post".into(), to: vec![], inform: vec!["w2".into()], post: Some(n), provenance: json!({}) }).unwrap();
        notes.push(format!("{id}.p{n}"));
    }
    let waiting = core.tickets.m.waiting("w2");
    assert_eq!(waiting.iter().map(|i| (i["id"].clone(), i["push"].clone())).collect::<Vec<_>>(), notes.iter().map(|n| (json!(n), json!(false))).collect::<Vec<_>>(), "five that only inform it: pulled");
    core.raise_delivery();
    assert!(core.to_hand_over().is_empty(), "they wake nothing");
    core.ticket_input(Input::Post { t: his, by: "owner".into(), text: Text::New("@w2 yes, take it".into()), names: vec!["w2".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let o = format!("{id}.p{}", core.tickets.m.tickets[&his].last());
    assert_eq!(core.to_hand_over(), ["w2"]);
    let ((ids, tickets, replies_on, again, into, rest), digest) = channel_as_sent(&mut core, &mut rx).expect("handed");
    assert_eq!((ids.clone(), into, rest.clone(), digest.clone()), (vec![o.clone()], false, Vec::<String>::new(), notes.clone()), "his alone, the five by first lines");
    let shared = crate::Shared::new(core);
    let out = crate::rendered_all(&shared, "w2", &tickets, replies_on.as_deref(), &again, into, false, &rest, &digest).await;
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0].0.contains("@w2 yes, take it") && out[0].0.contains("build 73, note 5") && !out[0].0.contains("which w2 need not read"), "{}", out[0].0);
    let mut core = Arc::try_unwrap(shared.0).ok().expect("one owner").into_inner();
    core.handed("w2", &ids, true, "channel");
    assert!(core.tickets.m.waiting("w2").is_empty(), "the five given with his");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The owner's post into a running turn (paper-research N-101 p1214, p1216): while w2 works his post is delivered
/// into the turn and w3's waits; the footer asks for the reply tool to his post; taken, it is given `into`. A
/// post he sent to wait ("when done") is kept, and at the turn's end it goes first and alone, the lead's named.
#[test]
fn his_post_goes_into_the_running_turn_unless_he_sent_it_to_wait() {
    let (dir, _, mut core, mut rx) = coord("into", "working");
    his_into(&dir, &mut core);
    let l = open(&mut core, "w3", &["w2"], "from w3");
    let o = open(&mut core, "owner", &["w2"], "stop and tell me");
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert_eq!(kept(&core, &l).as_deref(), Some("comes when its turn ends"));
    let (ids, tickets, replies_on, _, into, _) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!((ids.clone(), replies_on, into), (vec![o.clone()], None, true));
    let (d, view) = tickets[0].clone();
    let (content, meta) = crate::render::delivery_as(&core.places(), &d, &view, false, true, false, false, None, true, crate::render::Hints::default()).unwrap();
    assert!(content.contains(&format!("Answer it now with the reply tool, to {o}, then go on")), "{content}");
    assert_eq!(meta["id"], json!(d.id));
    core.handed("w2", &ids, true, "channel");
    assert_eq!(given_lines(&dir).last().map(|g| (g["delivery"].clone(), g["into"].clone())), Some((json!(o), json!(true))));
    // "when done": kept for the turn's end
    let before = core.ticket_owner_posts();
    let wait = open(&mut core, "owner", &["w2"], "after you are done");
    core.ticket_wait_posts(&before);
    assert!(core.to_hand_over().is_empty(), "his post sent to wait is not put in");
    assert_eq!(kept(&core, &wait).as_deref(), Some("comes when its turn ends"));
    // w2 answers his post with the reply tool, as the footer asks; the turn is then cut, and his post, answered, is
    // read (a cut leaves his post unanswered due again: paper-research N-412.p2202, 2)
    core.ticket_input(Input::Post { t: t_of(&o), by: "w2".into(), text: Text::New("stopped at the second step".into()), names: vec![], told: vec![], inform: vec![], answers: Some(1), kind: None }).unwrap();
    core.ticket_input(Input::TurnCut { agent: "w2".into() }).unwrap();
    idle(&mut core);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, _, into, rest) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!((ids, into, rest), (vec![wait], false, vec![l]));
    let _ = std::fs::remove_dir_all(&dir);
}

/// His post to a working agent waits for its turn's end, as every post that goes to it now does (the owner, N-1104
/// p716: "only the turn end"; his yes p923 to proto-mind's N-1511 p28, which ends N-342's "answer me first"): kept,
/// "comes when its turn ends", nothing put into the turn; at its end his goes first and alone, w3's named at its foot.
/// On the text before, [`his_post_goes_into_the_running_turn_unless_he_sent_it_to_wait`]. Then the turn that answers
/// him runs, w3's still waiting, and at its end `free` says `worked`, so one `fold` orders w3's (fresh-lead-3 N-1540
/// p5, point 2: what was named at his posts' foot came while it worked; the owner, N-1104 p695). Before, the `free`
/// that gave his post forgot what came, and w3's went at that end with no call.
#[test]
fn his_post_to_a_working_agent_waits_for_its_turns_end_goes_first_alone_and_fold_orders_the_rest_after() {
    let (dir, _, mut core, mut rx) = coord("his-waits", "working");
    core.fold_by = None;
    let l = open(&mut core, "w3", &["w2"], "from w3");
    let o = open(&mut core, "owner", &["w2"], "stop and tell me");
    assert!(core.to_hand_over().is_empty(), "nothing goes into its turn");
    assert_eq!((kept(&core, &l).as_deref(), kept(&core, &o).as_deref()), (Some("comes when its turn ends"), Some("comes when its turn ends")));
    idle(&mut core);
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert!(core.ticket_due(WATCHER).is_empty(), "his post waits behind no call");
    let (ids, _, _, _, into, rest) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!((ids.clone(), into, rest), (vec![o], false, vec![l.clone()]), "his first and alone, w3's named at its foot");
    // the turn that answers him: w3's waits through it, and goes by one fold at its end
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    working(&mut core);
    core.raise_delivery();
    assert!(core.to_hand_over().is_empty(), "w3's does not go into the turn that answers him");
    end_turn(&mut core, &dir, "stopped; here is where it stands");
    core.raise_delivery();
    assert_eq!(free_values(&dir).last().map(|v| v["worked"].clone()), Some(json!(true)), "{:?}", free_values(&dir));
    let folds: Vec<Delivery> = core.ticket_due(WATCHER).into_iter().filter(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "fold")).collect();
    assert_eq!(folds.len(), 1, "{folds:?}");
    assert_eq!(core.fold_job(&folds[0]).and_then(|j| j.fold).map(|n| n.ids), Some(vec![l]));
    let _ = std::fs::remove_dir_all(&dir);
}

/// His post to a working agent, passed by that agent's own post on his thread before a round hands it (consult-3
/// N-1479 p151): the post was written without it, so it is handed after it, into the turn after a post in it (b1), at
/// the `free` after the turn's last message, which the rule Reply posts there (b2, proto-engine p153); passed with a
/// round's hand-over of it on its way, it goes once: as that hand-over, taken (proto-reader p204), or, not taken, as
/// the retry of it, which the delivery program still holds (consult-3 p212); either `given` gives what the era's line
/// told late. The first such post begins the era ERA_HIS_HANDED on the line after it, which names his post and tells it
/// to w2 late, alone; from it w2's post leaves his next one due, and it goes as any post, each time once. Before the era such a post read it, and it was never handed (research's N-1104.p102 on 19 Nov).
#[test]
fn his_post_its_agent_passes_before_a_round_hands_it_is_handed_still() {
    for way in [Passed::InTurn, Passed::AtEnd, Passed::NotTaken, Passed::OnItsWay] {
        his_post_passed(way);
    }
}

/// [`his_post_passed`]'s hand-over on its way, its taking lost to a start (consult-3 N-1479 p353): nothing handed is
/// known then, so the follower's first read of it in w2's transcript gives it with nothing to say what it showed
/// (`skipped` None, `Ticket::skipped_now`), and his post the era's line told late goes once more, as `.told`. It is
/// never lost (paper-research p352, p355).
#[test]
fn his_post_told_late_on_its_way_goes_once_more_after_a_start() {
    his_post_passed(Passed::Restarted);
}

/// Where w2's post passes his (`his_post_passed`): in its turn, at its end (the rule Reply), or in its turn with a
/// round's hand-over of his post on its way, which its session then takes or does not, or which a start finds in its
/// transcript, its taking lost.
#[derive(Clone, Copy, PartialEq)]
enum Passed {
    InTurn,
    AtEnd,
    NotTaken,
    OnItsWay,
    Restarted,
}

/// [`his_post_its_agent_passes_before_a_round_hands_it_is_handed_still`]: w2's turn begun on w3's thread A, then twice
/// his post on A to w2 and w2's post on A, passing it as WAY says.
fn his_post_passed(way: Passed) {
    let reply = way == Passed::AtEnd;
    let (dir, _, mut core, mut rx) = coord(
        match way {
            Passed::InTurn => "his-passed",
            Passed::AtEnd => "his-passed-end",
            Passed::NotTaken => "his-passed-not-taken",
            Passed::OnItsWay => "his-passed-on-way",
            Passed::Restarted => "his-passed-restarted",
        },
        "idle",
    );
    his_into(&dir, &mut core);
    let w = open(&mut core, "w3", &["w2"], "is build 35 in?");
    let a = t_of(&w);
    let id = core.tickets.m.id(a);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("w3's post");
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    for (his, said, era) in [("and the release notes?", "It is in.", false), ("and the changelog?", "Both are in.", true)] {
        assert_eq!(core.tickets.m.era >= docket_machine::ERA_HIS_HANDED, era, "in the era before his post: {his}");
        core.ticket_input(Input::Post { t: a, by: "owner".into(), text: Text::New(format!("@w2 {his}")), names: vec!["w2".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let n = core.tickets.m.tickets[&a].last();
        // a round hands it before w2's post, which comes before the session says it was taken
        let on_way = matches!(way, Passed::NotTaken | Passed::OnItsWay | Passed::Restarted).then(|| {
            assert_eq!(core.to_hand_over(), ["w2"]);
            channel(&mut core, &mut rx).expect("his post, on its way")
        });
        if reply {
            end_turn(&mut core, &dir, said);
        } else {
            core.ticket_input(Input::Post { t: a, by: "w2".into(), text: Text::New(format!("@w3 {said}")), names: vec!["w3".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        }
        let x = &core.tickets.m.tickets[&a];
        assert_eq!((x.last(), x.posts[n as usize - 1].by.as_str(), x.posts[n as usize].by.as_str()), (n + 1, "owner", "w2"), "w2's post passed his");
        // not taken: the retry, after its pause, hands what of it is due still
        let on_way = match on_way {
            Some((ids, ..)) if way == Passed::NotTaken => {
                core.handed("w2", &ids, false, "channel");
                core.retry_at.remove("w2");
                None
            }
            taken => taken,
        };
        let (ids, tickets, _, _, into, _) = on_way.unwrap_or_else(|| {
            assert_eq!(core.to_hand_over(), ["w2"], "his post {n} is due still");
            channel(&mut core, &mut rx).expect("his post")
        });
        let shown = tickets[0].0.shows();
        let told = ids.first() == Some(&format!("{id}.p{n}.told"));
        let late = !era && matches!(way, Passed::InTurn | Passed::AtEnd);
        assert_eq!((ids.len(), shown, into, told), (1, vec![n], !reply, late), "his alone, into the turn unless it ended, told late at the era's line only, unless on its way");
        if way == Passed::Restarted {
            // a start before its session said it was taken: nothing handed is known (`Tickets::load`), and the
            // follower's first read finds it in w2's transcript (`ticket_seen`)
            core.tickets.handed.clear();
            core.tickets.into.clear();
            core.tickets.backs.clear();
            core.inflight.clear();
            core.ticket_seen("w2", &ids[0], Loc { file: dir.join("w2.jsonl").display().to_string(), offset: u64::from(n), len: 1 });
            if !era {
                assert_eq!(core.to_hand_over(), ["w2"], "his post {n}, told late, once more after the start");
                let (again, ..) = channel(&mut core, &mut rx).expect("his post, told late");
                assert_eq!(again, [format!("{id}.p{n}.told")]);
                core.handed("w2", &again, true, "channel");
            }
        } else {
            core.handed("w2", &ids, true, "channel");
        }
        assert!(core.to_hand_over().is_empty(), "his post {n} is due no more");
        if reply {
            core.begin_turn("w2");
        }
    }
    let lines = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap();
    let begun: Vec<Value> = lines.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["ev"] == "event" && v["name"] == docket_machine::ERA && v["value"]["era"] == docket_machine::ERA_HIS_HANDED).map(|v| v["value"]["passed"].clone()).collect();
    assert_eq!(begun, [json!([[a, 2, "w2"]])], "one line begins the era, naming his first post");
    let _ = std::fs::remove_dir_all(&dir);
}

/// One post that both passes his post before it was handed and goes otherwise by the close (proto-reader N-1479 p342,
/// proto-lead p344, p349): w3's ask on its thread, handed to w2 in its turn, then his post to w2 there, then w2's post that
/// only passes the ask on to the lead ("@lead check this"), which is no answer to w3 by the ledger's rule. The machine
/// begins the era ERA_CLOSES on the line after it (`Machine::apply`), and coordd the era ERA_HIS_HANDED on the line
/// after that, naming his post (`ticket_input`), which goes to w2 told late, once. A start replays the record so.
#[test]
fn one_post_that_passes_his_and_goes_otherwise_by_the_close_begins_each_era() {
    let (dir, base, mut core, mut rx) = coord("his-passed-close", "idle");
    his_into(&dir, &mut core);
    let a = t_of(&open(&mut core, "w3", &["lead"], "the replay plan"));
    let id = core.tickets.m.id(a);
    let ask = typed(&mut core, a, "w3", &["w2"], PostKind::Ask { to: vec!["w2".into()] }, "run the replay");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("w3's ask");
    assert_eq!(ids, [ask]);
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    core.ticket_input(Input::Post { t: a, by: "owner".into(), text: Text::New("@w2 and cav-paper's?".into()), names: vec!["w2".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let his = core.tickets.m.tickets[&a].last();
    let log = dir.join("tickets/log.jsonl");
    let at = std::fs::read_to_string(&log).unwrap().lines().count();
    assert!(core.tickets.m.era < docket_machine::ERA_CLOSES, "in the era before the close, {}", core.tickets.m.era);
    core.ticket_input(Input::Post { t: a, by: "w2".into(), text: Text::New("@lead check this".into()), names: vec!["lead".into()], told: vec![], inform: vec!["w3".into()], answers: None, kind: None }).unwrap();
    let n = core.tickets.m.tickets[&a].last();
    let changes: Vec<(String, Option<&str>, Option<&str>)> = core.tickets.m.close_changes(a, n).into_iter().map(|k| (k.agent, k.before, k.by)).collect();
    assert_eq!((n, changes), (his + 1, vec![("w3".to_string(), Some("reply"), None)]), "it passed his post, and is no answer to w3's ask");
    let after: Vec<Value> = std::fs::read_to_string(&log).unwrap().lines().skip(at).filter_map(|l| serde_json::from_str::<Value>(l).ok()).collect();
    let eras: Vec<Value> = after.iter().filter(|v| v["ev"] == "event" && v["name"] == docket_machine::ERA).map(|v| v["value"].clone()).collect();
    assert_eq!(after.first().map(|v| (v["ev"].clone(), v["by"].clone())), Some((json!("post"), json!("w2"))));
    assert_eq!(eras, [json!({"era": docket_machine::ERA_CLOSES}), json!({"era": docket_machine::ERA_HIS_HANDED, "passed": [[a, his, "w2"]]})], "era 4, then era 5 with its passed, after the post");
    assert_eq!(core.tickets.m.era, docket_machine::ERA_HIS_HANDED);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, tickets, ..) = channel(&mut core, &mut rx).expect("his post, told late");
    assert_eq!((ids.clone(), tickets[0].0.shows()), (vec![format!("{id}.p{his}.told")], vec![his]));
    core.handed("w2", &ids, true, "channel");
    assert!(core.to_hand_over().is_empty(), "his post went once");
    drop(core);
    let (again, _jobs) = load(&dir, &base, "idle");
    let problems: Vec<&String> = again.tickets.errors.iter().filter(|e| e.contains("log.jsonl")).collect();
    assert_eq!((again.tickets.m.era, problems), (docket_machine::ERA_QUIET, Vec::<&String>::new()), "a start replays the record so, and begins the quiet era on it");
    let _ = std::fs::remove_dir_all(&dir);
}

/// gemini-advisor N-1446 p2365, 4 (research's record lines 176700-176702, 04:53:42Z): docket-core's p2293, a result
/// naming paper-research and answering its p2284, woke accounts-lead, docket-edge5 and docket-edge6, each idle and each
/// with an ask of docket-core's on N-1446 before it; two of them ended in "Nothing". From the era ERA_QUIET
/// (paper-research p2371, (c); proto-mind p2444) a result answers an ask only as any post does. Here, in a record of
/// that era, the lead's ask to w3 and w2's, both handed to w3, then w3's result to the lead answering the lead's: it
/// wakes no `free` for w2, idle, and waits in w2's pull. w3's next result, to w2 (the thread closed with the lead's ask
/// answered), wakes w2, and the first goes with it by its first line. With WAY_BACK_OUT, the coord is kept for the way
/// back (`result-close`).
#[test]
fn a_result_to_another_wakes_no_idle_agent_that_asked_its_writer_before() {
    let (dir, _, mut core, mut rx) = coord("result-close", "idle");
    core.ticket_input(Input::Event { name: docket_machine::ERA.into(), value: json!({"era": docket_machine::ERA_QUIET}) }).unwrap();
    let t = t_of(&open(&mut core, "lead", &["w3"], "run 73's gate"));
    let id = core.tickets.m.id(t);
    let lock = typed(&mut core, t, "w2", &["w3"], PostKind::Ask { to: vec!["w3".into()] }, "is the replay lock free?");
    let upto = core.tickets.m.tickets[&t].last();
    core.ticket_input(Input::Given { t, to: "w3".into(), delivery: format!("{id}.p{upto}"), upto, skipped: Default::default(), place: None }).unwrap();
    core.ticket_input(Input::Post { t, by: "w3".into(), text: Text::New("@lead 73's gate is green".into()), names: vec!["lead".into()], told: vec![], inform: vec![], answers: Some(1), kind: Some(PostKind::Result) }).unwrap();
    let n = core.tickets.m.tickets[&t].last();
    let green = format!("{id}.p{n}");
    // the policy tells w2, on the thread, of it (notify.eflint), as research's did the three
    let told = |core: &mut Core, n: u32| core.ticket_input(Input::Notified { t, event: "post".into(), to: vec![], inform: vec!["w2".into()], post: Some(n), provenance: json!({}) }).unwrap();
    told(&mut core, n);
    let waiting: Vec<(Value, Value)> = core.tickets.m.waiting("w2").iter().map(|i| (i["id"].clone(), i["push"].clone())).collect();
    assert_eq!(waiting, [(json!(green), json!(false))], "it answers the lead's, not {lock}: pulled");
    core.raise_delivery();
    assert!(core.to_hand_over().is_empty() && core.tickets.m.delivery_queue("w2").is_empty(), "nothing wakes w2");
    assert_eq!(frees(&dir), 0, "no `free` for w2");
    let free = typed(&mut core, t, "w3", &["w2"], PostKind::Result, "the lock is free");
    told(&mut core, n + 1);
    assert_eq!(core.to_hand_over(), ["w2"], "{free} answers {lock}");
    let (ids, ..) = channel(&mut core, &mut rx).expect("handed");
    core.handed("w2", &ids, true, "channel");
    assert!(core.tickets.m.waiting("w2").is_empty(), "{green} given with it: {ids:?}");
    assert_eq!(core.tickets.m.era, docket_machine::ERA_QUIET);
    crate::core::brain::tests::kept_for_the_way_back(&dir, "result-close");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The Docket's act chips (paper-research N-1396 p223, p247): an act of w2's delivery reaches the window in w2's view,
/// sent again when it acts: a keep with its items and why, the newest first.
#[test]
fn an_act_sends_the_agents_view_again_with_its_last_acts() {
    let (dir, _, mut core, _rx) = coord("acted", "working");
    let l = open(&mut core, "w3", &["w2"], "from w3");
    assert!(core.to_hand_over().is_empty(), "w3's post waits for its turn to end");
    assert_eq!(kept(&core, &l).as_deref(), Some("comes when its turn ends"));
    let v = core.view("w2");
    let keep = v.acted.first().expect("w2's last act");
    assert_eq!((keep.act.as_str(), keep.items.clone(), keep.why.as_deref()), ("keep", vec![l.clone()], Some("comes when its turn ends")));
    assert!(core.view("w3").acted.is_empty(), "w3's delivery did nothing");
    // the view went out with it: the newest of w2's in the ring carries the same acts
    let sent = core.ring.iter().rev().filter_map(|(_, e)| e.as_ref()).find_map(|e| match &e.body {
        coord_types::Body::Agent(a) if a.name == "w2" => Some(a.acted.clone()),
        _ => None,
    });
    assert_eq!(sent, Some(v.acted.clone()), "w2's view sent with its acts");
    // read back as the window reads a line: an agent's event, and a snapshot holding it
    for body in [coord_types::Body::Agent(v.clone()), coord_types::Body::Snapshot(coord_types::Snapshot { agents: vec![v.clone()], ..Default::default() })] {
        let line = serde_json::to_string(&coord_types::Event { seq: 7, body }).unwrap();
        let back: coord_types::Event = serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line}"));
        let view = match back.body { coord_types::Body::Agent(a) => a, coord_types::Body::Snapshot(s) => s.agents[0].clone(), _ => panic!("{line}") };
        assert_eq!(view.acted, v.acted, "{line}");
    }
    // a view with no acts writes none: a window from before reads it as it did
    let line = serde_json::to_string(&coord_types::Event { seq: 8, body: coord_types::Body::Agent(core.view("w3")) }).unwrap();
    assert!(!line.contains("\"acted\""), "{line}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Into a running turn (I-1480 c, delivery.kind's `arrived`): a post naming the agent from one above it in the team's
/// tree waits for the turn's end, as one from an agent that holds the role `lead` but is not above it does; only his
/// post, a take-back, a step it raised itself and a note about its turn go in. (The text before put the first in,
/// consult-3 N-1397.p13, 1: a record from before runs so, [`before`].) At the turn's end both go, in one message.
#[test]
fn a_post_from_one_above_it_waits_for_its_turns_end_as_a_role_named_leads_does() {
    let (dir, _, mut core, mut rx) = coord("above", "working");
    core.ticket_input(Input::Role { agent: "w3".into(), role: "lead".into(), also: vec![] }).unwrap();
    assert_eq!(core.tickets.m.roles_of("w3"), ["lead"]);
    let w = open(&mut core, "w3", &["w2"], "from w3, a lead by its role");
    let l = open(&mut core, "lead", &["w2"], "from its lead");
    assert!(core.to_hand_over().is_empty(), "nothing goes into the turn");
    assert_eq!((kept(&core, &w).as_deref(), kept(&core, &l).as_deref()), (Some("comes when its turn ends"), Some("comes when its turn ends")));
    idle(&mut core);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, _, into, _) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!((ids, into), (vec![w, l], false));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A post w2 had, taken back (0.2 B8, train-two's take-back in the program, delivery.kind): the notice goes into a
/// running turn whoever took it back, while that one's posts wait for the turn's end; the owner's take-back at `free`
/// is his word, first and alone with no call, as his posts are, the rest named at its foot. (A thread's first post is
/// its words, never taken back: the posts taken back are second posts, to w2 on threads to the lead.)
#[test]
fn a_take_back_goes_into_its_turn_and_his_goes_first_and_alone() {
    let (dir, _, mut core, mut rx) = coord("back", "idle");
    his_into(&dir, &mut core);
    let second = |core: &mut Core, by: &str, text: &str| -> (String, u32) {
        let t = t_of(&open(core, by, &["lead"], "for the lead"));
        // "@w2" in the words: his, naming nobody on a thread the lead is on, would go to the watcher's route (base.kind)
        core.ticket_input(Input::Post { t, by: by.into(), text: Text::New(format!("@w2 {text}")), names: vec!["w2".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let n = core.tickets.m.tickets[&t].posts.last().unwrap().n;
        (format!("{}.p{n}", core.tickets.m.id(t)), n)
    };
    // w3's post, given at a free (`next` by default: everything waiting)
    let (w, wn) = second(&mut core, "w3", "use the old fixture");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!(ids, [w.clone()]);
    core.handed("w2", &ids, true, "channel");
    working(&mut core);
    let later = open(&mut core, "w3", &["w2"], "and the new one");
    let id = core.tickets.m.id(t_of(&w));
    let r = core.ticket_take_back(&Who::Agent("w3".into()), &id, wn);
    assert!(r.ok, "{}", r.out);
    let back = format!("{w}.taken_back");
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert_eq!(kept(&core, &later).as_deref(), Some("comes when its turn ends"), "w3's post waits");
    let (ids, _, _, _, into, _) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!((ids.clone(), into), (vec![back.clone()], true), "its take-back goes in");
    core.handed("w2", &ids, true, "channel");
    // his post, put into the turn and given; taken back once the turn ended, with w3's waiting
    let (o, on) = second(&mut core, "owner", "stop and tell me");
    assert_eq!(core.to_hand_over(), ["w2"], "due {:?}, waiting {:?}, kept {:?}", core.tickets.due_whole("w2").iter().map(|d| &d.id).collect::<Vec<_>>(), core.tickets.m.waiting_of("w2", core.tickets.due_whole("w2")), core.tickets.m.kept("w2"));
    let (ids, ..) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!(ids, [o.clone()]);
    core.handed("w2", &ids, true, "channel");
    core.ticket_input(Input::TurnCut { agent: "w2".into() }).unwrap();
    idle(&mut core);
    let id = core.tickets.m.id(t_of(&o));
    let r = core.ticket_take_back(&Who::Owner, &id, on);
    assert!(r.ok, "{}", r.out);
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert!(!core.tickets.m.deciding("w2"), "no call");
    let (ids, _, _, _, into, rest) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!((ids, into, rest), (vec![format!("{o}.taken_back")], false, vec![later]));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The owner's Hold with its arms in delivery.kind (line 327; consult-3 N-1324 p32, paper-research N-1396 p180),
/// through coordd's round:
/// - his post written before the hold that had not gone yet goes, alone, as it would without it (paper-research
///   N-1396 p235, a);
/// - with Hold on, w3's post to idle w2 is kept with the hold's words, and no `next` is asked;
/// - a coordd started meanwhile keeps it so;
/// - his post written after the hold goes at once, alone, w3's not named at its foot;
/// - at the turn's end the hold still keeps w3's;
/// - his Release lets it go, and the round hands it (here by `next`'s fallback, everything), with nothing else raised
///   by hand.
#[test]
fn hold_keeps_an_agents_post_his_goes_and_release_hands_what_it_kept() {
    let (dir, base, mut core, mut rx) = coord("hold", "idle");
    // his post from before the hold, not gone yet: no round has run since it was written
    let p = open(&mut core, "owner", &["w2"], "first, the logs");
    let r = core.ticket_mail_hold(&Who::Owner, true);
    assert!(r.ok, "{}", r.out);
    let at = core.tickets.m.mail_hold.as_ref().unwrap().at.clone();
    let why = format!("the owner holds agents' mail since {} UTC", &at[11..16]);
    let w = open(&mut core, "w3", &["w2"], "is the second run green?");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, _, into, rest) = channel(&mut core, &mut rx).expect("his post from before the hold goes");
    assert_eq!((ids.clone(), into, rest), (vec![p.clone()], false, Vec::<String>::new()), "alone, w3's not named");
    core.handed("w2", &ids, true, "channel");
    working(&mut core);
    core.raise_delivery();
    // answered with the reply tool, then its turn cut: read (unanswered, the cut would leave it due again, p2202, 2)
    core.ticket_input(Input::Post { t: t_of(&p), by: "w2".into(), text: Text::New("reading them".into()), names: vec![], told: vec![], inform: vec![], answers: Some(1), kind: None }).unwrap();
    core.ticket_input(Input::TurnCut { agent: "w2".into() }).unwrap();
    idle(&mut core);
    assert!(core.to_hand_over().is_empty(), "held: nothing more to hand");
    assert_eq!(kept(&core, &w), Some(why.clone()));
    // (the watcher has w2's answer to read for asks, thread.kind's agent_post; no `next`)
    assert!(!core.tickets.m.deciding("w2") && core.ticket_due(WATCHER).iter().all(|d| core.tickets.m.run_of(d.t).is_none()), "no next for what the hold keeps");
    // a start meanwhile: the hold and its keep come back by replay, and nothing is handed
    drop(core);
    let (mut core, mut rx) = load(&dir, &base, "idle");
    assert!(core.to_hand_over().is_empty());
    assert_eq!(kept(&core, &w), Some(why.clone()));
    // his post, written after the hold: at once, alone, nothing held at its foot
    let o = open(&mut core, "owner", &["w2"], "stop and tell me");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, _, into, rest) = channel(&mut core, &mut rx).expect("his post goes");
    assert_eq!((ids.clone(), into, rest), (vec![o.clone()], false, Vec::<String>::new()));
    core.handed("w2", &ids, true, "channel");
    working(&mut core);
    core.raise_delivery();
    core.ticket_input(Input::Post { t: t_of(&o), by: "w2".into(), text: Text::New("stopped".into()), names: vec![], told: vec![], inform: vec![], answers: Some(1), kind: None }).unwrap();
    core.ticket_input(Input::TurnCut { agent: "w2".into() }).unwrap();
    idle(&mut core);
    assert!(core.to_hand_over().is_empty(), "the turn over, the hold still keeps w3's");
    assert_eq!(kept(&core, &w), Some(why));
    // Release: let go, and handed by the round
    let r = core.ticket_mail_hold(&Who::Owner, false);
    assert!(r.ok && r.out.ends_with("The one thing it kept is decided again."), "{}", r.out);
    assert_eq!(kept(&core, &w), None);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("what the hold kept, handed");
    assert_eq!(ids, [w]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// His Release owes a `free` only to an agent the hold still keeps a post from (consult-3 N-1396 p220, 1):
/// - w3, idle with an fyi for it waiting since before the Hold (pulled: it raises no `free`, I-1480), which the hold
///   never touched, gets no `free` at the Release, and its fyi waits on;
/// - w2 was kept the lead's post under the Hold, his post after it on its thread was handed and given past it, so it
///   waits below w2's read mark by its keep alone: the Release keeps it "from … to …", and w2 gets exactly one `free`,
///   which hands it.
#[test]
fn release_owes_a_free_only_to_an_agent_the_hold_still_keeps_a_post_from() {
    let (dir, _, mut core, mut rx) = coord("release", "idle");
    core.agents.get_mut("w3").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), turn: 1, ..Default::default() };
    // the record's delivery events NAME for AGENT, counted
    let events = |name: &str, agent: &str| -> usize {
        let log = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap_or_default();
        log.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["ev"] == "event" && v["name"] == name && v["value"]["agent"] == agent).count()
    };
    // before the Hold: an fyi for w3, on its thread to the lead
    let t3 = t_of(&open(&mut core, "w3", &["lead"], "the plan for the fixture"));
    let r3 = typed(&mut core, t3, "w2", &["w3"], PostKind::Fyi, "the fixture is in place");
    assert!(core.to_hand_over().is_empty());
    let waits = |core: &Core| core.tickets.m.waiting("w3").iter().any(|i| i["id"] == json!(r3));
    assert!(waits(&core) && events("free", "w3") == 0, "pulled: no free");
    // Hold on: the lead's post to w2 is kept; his after it on its thread goes alone and is given past it
    assert!(core.ticket_mail_hold(&Who::Owner, true).ok);
    let w = open(&mut core, "lead", &["w2"], "is the second run green?");
    assert!(core.to_hand_over().is_empty());
    let t = t_of(&w);
    core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("@w2 stop and tell me".into()), names: vec!["w2".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let n = core.tickets.m.tickets[&t].posts.last().unwrap().n;
    let o = format!("{}.p{n}", core.tickets.m.id(t));
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, _, _, rest) = channel(&mut core, &mut rx).expect("his post goes");
    assert_eq!((ids.clone(), rest), (vec![o], Vec::<String>::new()), "alone, the held post not named");
    core.handed("w2", &ids, true, "channel");
    working(&mut core);
    core.raise_delivery();
    // answered with the reply tool, then its turn cut: read (unanswered, the cut would leave it due again, p2202, 2)
    core.ticket_input(Input::Post { t, by: "w2".into(), text: Text::New("stopped".into()), names: vec![], told: vec![], inform: vec![], answers: Some(n), kind: None }).unwrap();
    core.ticket_input(Input::TurnCut { agent: "w2".into() }).unwrap();
    idle(&mut core);
    assert!(core.to_hand_over().is_empty(), "the turn over, the hold still keeps the lead's post");
    let why = kept(&core, &w).expect("kept below w2's read mark");
    assert!(why.starts_with("the owner holds agents' mail since "), "{why}");
    let before = (events("free", "w2"), events("free", "w3"));
    // Release: one `free` for w2, which hands the post; none for w3
    let r = core.ticket_mail_hold(&Who::Owner, false);
    assert!(r.ok && r.out.ends_with("The one thing it kept is decided again."), "{}", r.out);
    let why = kept(&core, &w).expect("still kept, below the read mark");
    assert!(why.starts_with("the owner held agents' mail from "), "{why}");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("what the hold kept, handed");
    assert_eq!(ids, [w]);
    assert_eq!((events("free", "w2"), events("free", "w3")), (before.0 + 1, before.1));
    assert!(waits(&core), "w3's fyi waits on");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A message put into a turn that ends before it is rendered (paper-research N-807.p37) goes as out of a turn: no
/// "while you were working", and given as such.
#[tokio::test]
async fn a_message_into_a_turn_that_ended_before_it_is_rendered_goes_as_out_of_one() {
    let (dir, _, mut core, mut rx) = coord("ended", "working");
    his_into(&dir, &mut core);
    open(&mut core, "owner", &["w2"], "give me a number");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, tickets, replies_on, again, into, rest) = channel(&mut core, &mut rx).expect("handed");
    assert!(into);
    idle(&mut core);
    let shared = crate::Shared::new(core);
    let out = crate::rendered_all(&shared, "w2", &tickets, replies_on.as_deref(), &again, into, false, &rest, &[]).await;
    assert_eq!(out.len(), 1);
    assert!(!out[0].0.contains("while you were working"), "{}", out[0].0);
    let mut core = shared.lock().await;
    core.handed("w2", &ids, true, "channel");
    assert_ne!(given_lines(&dir).last().map(|g| g["into"].clone()), Some(json!(true)));
    drop(core);
    let _ = std::fs::remove_dir_all(&dir);
}

/// His posts while the agent compacts (runtime-design §8, 19 Nov 12:26–12:28): each delivered as it comes, none handed
/// while it compacts; when it is over, the one message of all of them.
#[tokio::test]
async fn his_posts_during_a_compaction_are_one_message_when_it_is_over() {
    let (dir, _, mut core, mut rx) = coord("compact", "working");
    his_into(&dir, &mut core);
    core.agents.get_mut("w2").unwrap().live.compacting = true;
    let mut his = Vec::new();
    for i in 1..=3 {
        his.push(open(&mut core, "owner", &["w2"], &format!("and number {i}")));
        assert!(core.to_hand_over().is_empty(), "not while it compacts");
    }
    assert_eq!(core.tickets.m.hands("w2").len(), 3);
    core.agents.get_mut("w2").unwrap().live.compacting = false;
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, tickets, replies_on, again, into, rest) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!((ids, into), (his, true));
    let shared = crate::Shared::new(core);
    let out = crate::rendered_all(&shared, "w2", &tickets, replies_on.as_deref(), &again, into, false, &rest, &[]).await;
    assert_eq!(out.len(), 1);
    assert!(out[0].0.starts_with("The owner wrote 3 messages while you were working."), "{}", out[0].0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// [`his_posts_during_a_compaction_are_one_message_when_it_is_over`] on the text now (paper-research N-1446 p2111
/// point 1): each of his posts waits for the turn's end, none handed while it compacts nor when the compaction is over
/// and its turn goes on; at the turn's end, with nothing else to end their wait, the one message of all of them, once.
#[tokio::test]
async fn his_posts_during_a_compaction_wait_for_the_turns_end_and_go_as_one_given_once() {
    let (dir, _, mut core, mut rx) = coord("compact-now", "working");
    core.agents.get_mut("w2").unwrap().live.compacting = true;
    let mut his = Vec::new();
    for i in 1..=3 {
        his.push(open(&mut core, "owner", &["w2"], &format!("and number {i}")));
        assert!(core.to_hand_over().is_empty(), "not while it compacts");
    }
    assert!(his.iter().all(|o| kept(&core, o).as_deref() == Some("comes when its turn ends")), "{:?}", core.tickets.m.kept("w2"));
    core.agents.get_mut("w2").unwrap().live.compacting = false;
    assert!(core.to_hand_over().is_empty(), "the compaction over, its turn goes on: they wait for its end");
    idle(&mut core);
    assert_eq!(core.to_hand_over(), ["w2"], "at the turn's end");
    let (ids, tickets, replies_on, again, into, rest) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!((ids.clone(), into, rest.clone()), (his.clone(), false, Vec::<String>::new()));
    core.handed("w2", &ids, true, "channel");
    assert!(core.to_hand_over().is_empty() && channel(&mut core, &mut rx).is_none(), "given once");
    let shared = crate::Shared::new(core);
    let out = crate::rendered_all(&shared, "w2", &tickets, replies_on.as_deref(), &again, into, false, &rest, &[]).await;
    assert_eq!(out.len(), 1, "one message: {out:?}");
    assert!((1..=3).all(|i| out[0].0.contains(&format!("and number {i}"))), "{}", out[0].0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// While the owner writes to the agent (paper-research N-101 p1099, p1127) what waits is kept with why; when he stops,
/// it goes. Said by an app that has ended, his writing holds nothing back. Only he says it.
#[test]
fn while_he_writes_to_it_the_rest_is_kept_and_his_writing_ends_with_his_app() {
    let (dir, _, mut core, mut rx) = coord("writing", "idle");
    let l = open(&mut core, "lead", &["w2"], "from lead");
    assert!(core.owner_writing(&Who::Owner, "w2", true, std::process::id() as i32).ok);
    assert!(core.to_hand_over().is_empty());
    assert_eq!(kept(&core, &l).as_deref(), Some("the owner is writing to it"));
    assert!(core.to_hand_over().is_empty(), "kept still while he writes");
    assert!(!core.owner_writing(&Who::Agent("lead".into()), "w2", false, 0).ok, "the owner's only");
    assert!(core.owner_writing(&Who::Owner, "w2", false, 0).ok);
    assert_eq!(core.to_hand_over(), ["w2"], "his writing over: what waited goes");
    let (ids, ..) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!(ids, [l.clone()]);
    core.handed("w2", &ids, true, "channel");
    core.ticket_input(Input::TurnCut { agent: "w2".into() }).unwrap();
    // said by an app that has ended: nothing held
    let l2 = open(&mut core, "lead", &["w2"], "and the docs");
    let mut child = std::process::Command::new("true").spawn().unwrap();
    let gone = child.id() as i32;
    child.wait().unwrap();
    assert!(core.owner_writing(&Who::Owner, "w2", true, gone).ok);
    assert_eq!(core.to_hand_over(), ["w2"], "its app closed: nothing held");
    let (ids, ..) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!(ids, [l2]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// While he writes to the agent his own posts wait too, as his posts wait for the turn's end (gemini-advisor N-1511
/// p32, fresh-lead-3 p33; the owner, N-1104 p716): his first is kept, "the owner is writing to it", and goes with his
/// second as one message when he stops, the lead's post named at its foot. His window ends his writing with a draft
/// he leaves behind (yaaihh lib.rs `writing_changed`; paper-research N-1446 p2189), so the wait has an end.
#[test]
fn while_he_writes_his_own_post_waits_too_and_goes_with_his_next_as_one() {
    let (dir, _, mut core, mut rx) = coord("writing-his", "idle");
    let l = open(&mut core, "lead", &["w2"], "from lead");
    assert!(core.owner_writing(&Who::Owner, "w2", true, std::process::id() as i32).ok);
    let a = open(&mut core, "owner", &["w2"], "the median first");
    assert!(core.to_hand_over().is_empty(), "his first waits while he writes");
    assert_eq!((kept(&core, &a).as_deref(), kept(&core, &l).as_deref()), (Some("the owner is writing to it"), Some("the owner is writing to it")));
    let b = open(&mut core, "owner", &["w2"], "then the mean");
    assert!(core.to_hand_over().is_empty(), "his second too");
    assert!(core.owner_writing(&Who::Owner, "w2", false, 0).ok);
    assert_eq!(core.to_hand_over(), ["w2"], "his writing over: his two go");
    let (ids, _, _, _, _, rest) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!((ids, rest), (vec![a, b], vec![l]), "his two as one message, the lead's named at its foot");
    let _ = std::fs::remove_dir_all(&dir);
}

/// His writing in a ticket's box (0.3; proto-lead N-1446 p26), `Request::Writing` with a thread: only he says it, on a
/// ticket that is there; one `writing` line each way, the machine keeping since when, and a second `on` writes nothing.
/// It holds nothing back from the agents on the ticket (his writing to an agent is apart). Said by an app that has
/// ended, the next round writes it off; so does the first round after a start, for what the record left on.
#[test]
fn his_writing_in_a_tickets_box_is_recorded_and_ends_with_his_app_or_a_start() {
    let (dir, base, mut core, mut rx) = coord("writing-on", "idle");
    let l = open(&mut core, "lead", &["w2"], "from lead");
    let t = t_of(&l);
    let id = core.tickets.m.id(t);
    let lines = || std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap_or_default().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|l| l["ev"] == "event" && l["name"] == docket_machine::syntax::WRITING).map(|l| l["value"].clone()).collect::<Vec<_>>();
    let me = std::process::id() as i32;
    assert!(!core.owner_writing_on(&Who::Agent("lead".into()), &id, true, me).ok, "the owner's only");
    assert!(!core.owner_writing_on(&Who::Owner, "N-9999", true, me).ok, "no such ticket");
    assert!(core.owner_writing_on(&Who::Owner, &id, true, me).ok);
    assert!(core.owner_writing_on(&Who::Owner, &id, true, me).ok);
    assert_eq!(lines(), [json!({"t": t, "on": true})], "a second on writes nothing");
    assert!(core.tickets.m.writing_since(t).is_some());
    // what waits for w2 on it goes all the same
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!(ids, [l.clone()]);
    // a round while his app lives writes nothing; he stops: one line
    core.raise_delivery();
    assert_eq!(lines().len(), 1);
    assert!(core.owner_writing_on(&Who::Owner, &id, false, 0).ok);
    assert_eq!(lines(), [json!({"t": t, "on": true}), json!({"t": t, "on": false})]);
    assert_eq!(core.tickets.m.writing_since(t), None);
    // said by an app that has ended: the next round writes it off
    let mut child = std::process::Command::new("true").spawn().unwrap();
    let gone = child.id() as i32;
    child.wait().unwrap();
    assert!(core.owner_writing_on(&Who::Owner, &id, true, gone).ok);
    core.raise_delivery();
    assert_eq!((lines().len(), core.tickets.m.writing_since(t)), (4, None));
    // left on at a stop: a start reads it on from the record, and its first round writes it off
    assert!(core.owner_writing_on(&Who::Owner, &id, true, me).ok);
    drop(core);
    let (mut again, _jobs) = load(&dir, &base, "idle");
    assert!(again.tickets.m.writing_since(t).is_some(), "as the record left it");
    again.raise_delivery();
    assert_eq!(again.tickets.m.writing_since(t), None);
    assert_eq!((lines().len(), lines().last().cloned()), (6, Some(json!({"t": t, "on": false}))));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The owner's post that only informs the agent (told to another, the agent in its inform) is no post of his for
/// `free`: it holds nothing back, and goes with what `next` gives (paper-research N-101 p1842).
#[test]
fn his_post_that_only_informs_it_holds_nothing_back() {
    let (dir, _, mut core, mut rx) = coord("inform", "idle");
    let his = t_of(&open(&mut core, "owner", &["lead"], "to the lead"));
    core.ticket_input(Input::Post { t: his, by: "owner".into(), text: Text::New("when does it reach prod".into()), names: vec!["lead".into()], told: vec!["lead".into()], inform: vec!["w2".into()], answers: None, kind: None }).unwrap();
    let n = core.tickets.m.tickets[&his].posts.last().unwrap().n;
    core.ticket_input(Input::Notified { t: his, event: "post".into(), to: vec![], inform: vec!["w2".into()], post: Some(n), provenance: json!({}) }).unwrap();
    let l = open(&mut core, "lead", &["w2"], "for you, w2");
    assert_eq!(core.to_hand_over(), ["w2"], "w2 is handed to");
    let (ids, tickets, ..) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!(ids, [format!("{}.p{n}", core.tickets.m.id(his)), l]);
    assert_eq!(tickets.iter().map(|(d, _)| d.inform).collect::<Vec<_>>(), [true, false], "his only informs it");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A post put into a turn just before its last message (paper-research N-1397.p81: docket-core on build 34, nothing
/// raised from 16:18:29Z): its words entered the model's input in that turn (`delivered`), and the turn ends replying
/// on the post it began with. From the era ERA_PUT_IN the one put in is read at that end (docket-core2's 9af55a0a,
/// kept without its guard: consult-3 N-1397.p102): nothing stays given, w2, idle, is free, and a post to it later
/// raises `free` and is handed. On the text before ([`before`]), whose `arrived` put a lead's post into its worker's
/// turn.
#[test]
fn a_post_put_into_a_turn_just_before_its_end_holds_no_free_back() {
    let (dir, _, mut core, mut rx) = coord("into-end", "idle");
    before(&dir, &mut core);
    let a = open(&mut core, "w3", &["w2"], "is the second run green?");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("w3's post");
    assert_eq!(ids, [a.clone()]);
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    core.raise_delivery();
    // its lead's post goes into the turn, and its words into the model's input before the turn's last message
    let l = open(&mut core, "lead", &["w2"], "and the stills, please");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, _, into, _) = channel(&mut core, &mut rx).expect("the lead's post, into the turn");
    assert_eq!((ids.clone(), into), (vec![l.clone()], true));
    core.handed("w2", &ids, true, "channel");
    let file = dir.join("w2.jsonl").display().to_string();
    let _ = core.transcript_read("w2", vec![(l.clone(), file.clone(), 40, 700)], vec![], vec![], &[], None, &[], false);
    core.ticket_delivered("w2", &l, Loc { file: file.clone(), offset: 40, len: 700 }, "2026-11-20T16:18:20Z", false);
    // its entry's bytes, 40 to 740, stand in the transcript before the turn's last message, as in p81's record (p292
    // entered at line 100254, the turn ended at 100255); one written at 0 would have it handed after that message
    std::fs::write(&file, format!("{}\n", " ".repeat(739))).unwrap();
    end_turn(&mut core, &dir, "Yes, green.");
    let x = &core.tickets.m.tickets[&t_of(&a)];
    assert_eq!((x.posts.len(), x.posts.last().map(|p| p.by.as_str())), (2, Some("w2")), "the reply is on w3's thread");
    assert!(core.ticket_given_since("w2").is_empty(), "the lead's post is read at the turn's end: {:?}", core.ticket_given_since("w2"));
    // a post later: `free`, and it is handed
    let b = open(&mut core, "w3", &["w2"], "and the third?");
    assert_eq!(core.to_hand_over(), ["w2"], "free: nothing it holds is still to begin a turn");
    let (ids, ..) = channel(&mut core, &mut rx).expect("w3's second post");
    assert_eq!(ids, [b]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A message handed, and coordd stopped before the session's answer (consult-3, N-1397.p11, 1): the record still has
/// it to hand. After the start the follower's first read, which comes before the round's hand-over, finds it in the
/// transcript: given at its place, once, and not handed again.
#[test]
fn a_message_handed_before_a_stop_is_given_at_its_place_after_the_start_not_handed_again() {
    let (dir, base, mut core, mut rx) = coord("restart", "idle");
    let a = open(&mut core, "lead", &["w2"], "the fixture is in place");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!(ids, [a.clone()]);
    drop(core);
    let (mut core, mut rx) = load(&dir, &base, "idle");
    assert_eq!(core.hand_now("w2", false).items.len(), 1, "the record's message, still to go");
    let file = dir.join("w2.jsonl").display().to_string();
    let _ = core.transcript_read("w2", vec![(a.clone(), file, 40, 900)], vec![], vec![], &[], None, &[], false);
    assert_eq!(given_at(&core, &a), [Some(40)]);
    assert!(core.to_hand_over().is_empty(), "not handed again");
    assert!(channel(&mut core, &mut rx).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// His post while the agent starts (paper-research N-1397.p9): one message, made once, handed once after its start
/// hook, given once at its place.
#[test]
fn his_post_while_it_starts_is_handed_once_after_the_start_and_given_once() {
    let (dir, _, mut core, mut rx) = coord("starting", "idle");
    his_into(&dir, &mut core);
    core.agents.get_mut("w2").unwrap().live.awaiting_start = Some(std::time::Instant::now());
    let o = open(&mut core, "owner", &["w2"], "are you up?");
    for _ in 0..3 {
        assert!(core.to_hand_over().is_empty(), "nothing before its start hook");
    }
    assert_eq!(core.tickets.m.hands("w2").len(), 1, "one message, made once");
    core.agents.get_mut("w2").unwrap().live.awaiting_start = None;
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!(ids, [o.clone()]);
    core.handed("w2", &ids, true, "channel");
    let file = dir.join("w2.jsonl").display().to_string();
    let _ = core.transcript_read("w2", vec![(o.clone(), file, 40, 900)], vec![], vec![], &[], None, &[], false);
    assert!(core.to_hand_over().is_empty() && channel(&mut core, &mut rx).is_none(), "handed once");
    assert_eq!(given_at(&core, &o), [Some(40)], "given once, at its place");
    let _ = std::fs::remove_dir_all(&dir);
}

/// [`his_post_while_it_starts_is_handed_once_after_the_start_and_given_once`] on the text now, where his post waits
/// for the turn's end (paper-research N-1446 p2111 point 1): while it starts his post is kept, nothing made; its start
/// over, it goes with nothing else to end its wait, made once, handed once, given once at its place.
#[test]
fn his_post_while_it_starts_waits_for_the_start_and_goes_given_once() {
    let (dir, _, mut core, mut rx) = coord("starting-now", "idle");
    core.agents.get_mut("w2").unwrap().live.awaiting_start = Some(std::time::Instant::now());
    let o = open(&mut core, "owner", &["w2"], "are you up?");
    for _ in 0..3 {
        assert!(core.to_hand_over().is_empty(), "nothing before its start hook");
    }
    assert_eq!(kept(&core, &o).as_deref(), Some("comes when its turn ends"));
    assert!(core.tickets.m.hands("w2").is_empty(), "nothing made while it starts");
    core.agents.get_mut("w2").unwrap().live.awaiting_start = None;
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert_eq!(core.tickets.m.hands("w2").len(), 1, "one message, made once");
    let (ids, _, _, _, into, rest) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!((ids.clone(), into, rest), (vec![o.clone()], false, Vec::<String>::new()));
    core.handed("w2", &ids, true, "channel");
    let file = dir.join("w2.jsonl").display().to_string();
    let _ = core.transcript_read("w2", vec![(o.clone(), file, 40, 900)], vec![], vec![], &[], None, &[], false);
    assert!(core.to_hand_over().is_empty() && channel(&mut core, &mut rx).is_none(), "handed once");
    assert_eq!(given_at(&core, &o), [Some(40)], "given once, at its place");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The install of I-1480 c (proto-engine N-1481 p94, 3): w2's `free` on the text before ([`before`]) awaits the
/// watcher's `next` when coordd stops, its call out. The start, on agent-base's text, answers it at once, with no call,
/// as a failed call's was: what waits, oldest first, with its first step, a later step named at the foot. Once that
/// turn has ended, w2's next run opens its delivery ticket on the text now (`delivery_ticket`), whose `free` delivers
/// the step left with no `next`.
#[test]
fn a_run_open_at_the_install_is_answered_with_no_call_and_the_next_run_is_on_the_text_now() {
    let (dir, base, mut core, _rx) = coord("install", "idle");
    before(&dir, &mut core);
    let q1 = question(&mut core, "which median?");
    let a = open(&mut core, "w3", &["w2"], "is the second run green?");
    let q2 = question(&mut core, "which mean?");
    // its `free`, raised as coordd raises it, and coordd stopped before the call's answer
    let free = json!({"agent": "w2", "doing": "", "writing": false, "above": ["lead"]});
    core.ticket_input(Input::Event { name: docket_machine::syntax::FREE.into(), value: free }).unwrap();
    assert!(core.tickets.m.deciding("w2"));
    let old = desk_id(&core, "w2");
    drop(core);
    let (mut core, mut rx) = load(&dir, &base, "idle");
    assert!(core.tickets.m.deciding("w2"), "its run awaits next across the start");
    assert_eq!(core.to_hand_over(), ["w2"], "answered in the start's first round");
    assert!(!core.tickets.m.deciding("w2"));
    let (s1, s2) = (format!("{}.s1", core.tickets.m.id(q1)), format!("{}.s1", core.tickets.m.id(q2)));
    let (ids, _, _, _, into, rest) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!((ids.clone(), into, rest), (vec![s1, a], false, vec![s2.clone()]), "everything waiting, the first step only");
    core.hand_clerk();
    assert!(std::iter::from_fn(|| rx.try_recv().ok()).all(|j| !matches!(j, Job::Deliver { .. })), "no call");
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    end_turn(&mut core, &dir, "The median of the middle two; green.");
    // the next run: a delivery ticket on the text now, and no `next`
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert!(!core.tickets.m.deciding("w2"));
    let now = desk_id(&core, "w2");
    assert_ne!(now, old, "its next run opened its next delivery ticket");
    let program = &core.tickets.m.tickets[&docket_machine::parse_ticket(&now).unwrap()].program;
    assert_eq!(Some(program), core.tickets.m.defs.kinds.get("delivery").map(|k| &k.text), "on the text loaded now, agent-base's");
    assert!(!program.contains("step watcher next"), "{program}");
    let (ids, ..) = channel(&mut core, &mut rx).expect("the step left");
    assert_eq!(ids, [s2]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// An item the lead opened, its close held by the lead (item.kind): its id and ticket.
pub(crate) fn item(core: &mut Core, title: &str) -> (String, u64) {
    let form = json!({"title": title, "note": null});
    let r = core.ticket_input(Input::Open { kind: "item".into(), under: None, opener: "lead".into(), form, text: None, names: vec![], told: vec![] }).unwrap();
    let id = r.trim_start_matches("Opened ").trim_end_matches('.').to_string();
    let t = docket_machine::parse_ticket(&id).unwrap();
    core.ticket_input(Input::Decided { t, step: 1, question: "fills".into(), answer: json!("lead"), provenance: json!({}) }).unwrap();
    (id, t)
}

/// The hidden runs item I's acts raised (item.kind's `when act`).
fn act_runs(core: &Core, i: u64) -> usize {
    core.tickets.m.tickets.range(docket_machine::HIDDEN..).filter(|(_, x)| x.under == Some(i) && x.handling.as_deref() == Some("act")).count()
}

/// `coord item start` in a running turn (consult-3 N-1396 p189): the seek it raises is w2's own step, and goes into
/// that turn (delivery.kind's `arrived`), while w3's post waits for the turn's end; w2's answer stands on the item.
#[test]
fn the_seek_a_start_raises_goes_into_its_turn_and_another_agents_post_waits() {
    let (dir, _, mut core, mut rx) = coord("seek-into", "working");
    let (id, i) = item(&mut core, "faster replay");
    let w = open(&mut core, "w3", &["w2"], "is the second run green?");
    let r = core.item_start(&Who::Agent("w2".into()), &id);
    assert!(r.ok, "{}", r.out);
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert_eq!(kept(&core, &w).as_deref(), Some("comes when its turn ends"));
    let (ids, tickets, _, _, into, _) = channel(&mut core, &mut rx).expect("handed");
    let d = tickets[0].0.clone();
    let docket_machine::What::Step { call, args, .. } = &d.what else { panic!("{d:?}") };
    assert_eq!((ids.len(), call.as_str(), &args["what"], into), (1, "seek", &json!("faster replay"), true));
    assert_eq!(core.tickets.m.tickets[&d.t].under, Some(i));
    core.handed("w2", &ids, true, "channel");
    core.ticket_input(Input::Answer { t: d.t, by: "w2".into(), head: json!({"head": "Nothing", "searched": "replay in docket-machine"}), text: Text::New("as said".into()) }).unwrap();
    let p = core.tickets.m.tickets[&i].posts.last().cloned().unwrap();
    assert_eq!((p.by.as_str(), p.text), ("w2", Text::New("Nothing does part of it yet. Searched: replay in docket-machine".into())));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The state changes between the read and the act (consult-3 N-1396 p189, a): w2's turn ends between its `coord item
/// start` and the round, so the seek meets `free`, not `arrived`. It goes at once, the rest named at its foot, and
/// `next` is not asked: a Rest there would let the work come before the look it is to follow.
#[test]
fn a_seek_whose_turn_ended_before_the_round_goes_at_once_with_no_next() {
    let (dir, _, mut core, mut rx) = coord("seek-free", "working");
    let (id, _) = item(&mut core, "faster replay");
    let w = open(&mut core, "w3", &["w2"], "is the second run green?");
    assert!(core.item_start(&Who::Agent("w2".into()), &id).ok);
    idle(&mut core);
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert!(!core.tickets.m.deciding("w2") && core.ticket_due(WATCHER).is_empty(), "no next asked");
    let (ids, tickets, _, _, into, rest) = channel(&mut core, &mut rx).expect("handed");
    let docket_machine::What::Step { call, .. } = &tickets[0].0.what else { panic!("{:?}", tickets[0].0) };
    assert_eq!((ids.len(), call.as_str(), into, rest), (1, "seek", false, vec![w.clone()]));
    assert_eq!(kept(&core, &w), None, "named at the foot, not kept");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The first read after a start (consult-3 N-1396 p189, b): w2 starts the item, its seek is handed, taken into its
/// session (its transcript shows it) and not yet answered, and coordd starts again on its record while that session
/// runs on. The step is w2's still, is not raised or handed again, and no second run is made, a second start by w2 included; w2's answer after
/// the start stands on the item. (A seek its session never took is due again at the start, as any giving not read is:
/// `ticket_lost_with_process`.)
#[test]
fn a_seek_handed_before_a_restart_is_owed_still_and_neither_raised_nor_run_again() {
    let (dir, base, mut core, mut rx) = coord("seek-restart", "working");
    let (id, i) = item(&mut core, "faster replay");
    assert!(core.item_start(&Who::Agent("w2".into()), &id).ok);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, tickets, ..) = channel(&mut core, &mut rx).expect("handed");
    core.handed("w2", &ids, true, "channel");
    // its session took it: the follower finds its place, then its entry into the model's input
    let file = dir.join("w2.jsonl").display().to_string();
    let _ = core.transcript_read("w2", vec![(ids[0].clone(), file.clone(), 40, 900)], vec![], vec![], &[], None, &[], false);
    core.ticket_delivered("w2", &ids[0], Loc { file, offset: 40, len: 900 }, "2026-11-20T15:00:00Z", false);
    let c = tickets[0].0.t;
    assert_eq!(act_runs(&core, i), 1);
    drop(core);
    let (mut core, mut rx) = load_with(&dir, &base, "working", true);
    assert_eq!(act_runs(&core, i), 1, "no second run");
    assert!(matches!(&core.tickets.m.tickets[&c].status, docket_machine::Status::Awaits { agent, .. } if agent == "w2"), "{:?}", core.tickets.m.tickets[&c].status);
    assert!(core.to_hand_over().is_empty() && channel(&mut core, &mut rx).is_none(), "not handed again");
    assert!(core.tickets.m.hands("w2").is_empty() && core.tickets.m.delivery_queue("w2").is_empty(), "not raised again");
    assert!(core.item_start(&Who::Agent("w2".into()), &id).ok);
    assert_eq!(act_runs(&core, i), 1, "a second start asks nothing");
    core.ticket_input(Input::Answer { t: c, by: "w2".into(), head: json!({"head": "Extends", "pieces": "replay at machine.rs:8379"}), text: Text::New("as said".into()) }).unwrap();
    assert!(!core.tickets.m.tickets[&c].is_open());
    assert_eq!(core.tickets.m.tickets[&i].posts.last().map(|p| p.by.clone()).as_deref(), Some("w2"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The same seek, taken into w2's running turn, whose process ended while coordd was down (consult-3 N-1396 p300, 3):
/// the start cuts that turn (`load_tickets`), so the seek comes again as at a turn's end without its answer, alone and
/// with no `next`; no second run is made.
#[test]
fn a_seek_in_a_turn_whose_process_ended_while_coordd_was_down_comes_again_at_the_start() {
    let (dir, base, mut core, mut rx) = coord("seek-cut", "working");
    let (id, i) = item(&mut core, "faster replay");
    assert!(core.item_start(&Who::Agent("w2".into()), &id).ok);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, _, into, _) = channel(&mut core, &mut rx).expect("handed");
    assert!(into, "into its running turn");
    core.handed("w2", &ids, true, "channel");
    let file = dir.join("w2.jsonl").display().to_string();
    let _ = core.transcript_read("w2", vec![(ids[0].clone(), file.clone(), 40, 900)], vec![], vec![], &[], None, &[], false);
    core.ticket_delivered("w2", &ids[0], Loc { file, offset: 40, len: 900 }, "2026-11-20T15:00:00Z", false);
    drop(core);
    let (mut core, mut rx) = load(&dir, &base, "idle");
    assert_eq!(act_runs(&core, i), 1, "no second run");
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert!(!core.tickets.m.deciding("w2"), "no next asked");
    let (again, tickets, _, _, into, rest) = channel(&mut core, &mut rx).expect("the seek again");
    assert_eq!((again, into, rest.len()), (vec![format!("{}.again", ids[0])], false, 0));
    assert!(matches!(&tickets[0].0.what, docket_machine::What::Step { again: true, left: true, why: None, .. }), "{:?}", tickets[0].0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A question the lead asks, whose `answer` is w2's to give (question.kind's role lead, filled by w2): its ticket.
fn question(core: &mut Core, ask: &str) -> u64 {
    let r = core.ticket_input(Input::Open { kind: "question".into(), under: None, opener: "lead".into(), form: json!({"ask": ask}), text: None, names: vec![], told: vec![] }).unwrap();
    let t = docket_machine::parse_ticket(r.trim_start_matches("Opened ").trim_end_matches('.')).unwrap();
    core.ticket_input(Input::Decided { t, step: 1, question: "fills".into(), answer: json!("w2"), provenance: json!({}) }).unwrap();
    t
}

/// w2's turn ends with TEXT as its last message, written to its transcript in DIR, and is over.
pub(crate) fn end_turn(core: &mut Core, dir: &Path, text: &str) {
    let transcript = dir.join("w2.jsonl");
    let mut all = std::fs::read_to_string(&transcript).unwrap_or_default();
    let offset = all.len() as u64;
    let said = format!("{}\n", json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": text}], "stop_reason": "end_turn"}}));
    all.push_str(&said);
    std::fs::write(&transcript, &all).unwrap();
    core.ticket_turn_ended("w2", Loc { file: transcript.display().to_string(), offset, len: said.len() as u64 });
    idle(core);
}

/// The step T owes now, if it awaits one: its number, whether it was taken, whether an answer is being formed.
fn owed(core: &Core, t: u64) -> Option<(u32, bool, bool)> {
    match &core.tickets.m.tickets[&t].status {
        docket_machine::Status::Awaits { owed, taken, forming, .. } => Some((owed.step, *taken, forming.is_some())),
        _ => None,
    }
}

/// A start in a turn that holds a step (consult-3 N-1396 p280): w2's turn began with a question's `answer` when it
/// runs `coord item start`. The seek goes into that turn (delivery.kind's `arrived`) and w2 answers it there, with
/// `coord answer`: the answer stands on the item. While the seek was open `owing` gave the question, so w2's acts
/// stayed with it (p264, 2); the turn's end answers the question, nothing comes again, and a later post's turn is
/// replied on that post.
#[test]
fn a_seek_raised_in_a_turn_that_holds_a_step_goes_in_and_is_answered_there() {
    let (dir, _, mut core, mut rx) = coord("seek-step", "idle");
    let (id, i) = item(&mut core, "faster replay");
    let q = question(&mut core, "which median?");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("the question's step");
    assert_eq!(ids, [format!("{}.s1", core.tickets.m.id(q))]);
    core.handed("w2", &ids, true, "channel");
    working(&mut core);
    assert!(core.item_start(&Who::Agent("w2".into()), &id).ok);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, tickets, _, _, into, _) = channel(&mut core, &mut rx).expect("the seek, into its turn");
    let docket_machine::What::Step { call, .. } = &tickets[0].0.what else { panic!("{:?}", tickets[0].0) };
    assert_eq!((ids.len(), call.as_str(), into), (1, "seek", true));
    let (c, seek_ticket) = (tickets[0].0.t, tickets[0].0.ticket.clone());
    core.handed("w2", &ids, true, "channel");
    assert_eq!(core.tickets.m.owing("w2"), Some(core.tickets.m.id(q)), "its acts stay with the question");
    // `coord answer` in the turn: the seek is done, its answer on the item
    let r = core.ticket_answer_request(&Who::Agent("w2".into()), &seek_ticket, json!({"head": "Nothing", "searched": "replay in docket-machine"}), None);
    assert!(r.ok, "{}", r.out);
    assert!(!core.tickets.m.tickets[&c].is_open());
    let p = core.tickets.m.tickets[&i].posts.last().cloned().unwrap();
    assert_eq!((p.by.as_str(), p.text), ("w2", Text::New("Nothing does part of it yet. Searched: replay in docket-machine".into())));
    // the turn's end answers the question; nothing comes again
    end_turn(&mut core, &dir, "The median of the middle two.");
    assert_eq!(owed(&core, q), Some((1, true, true)), "the question's answer is formed from that turn's last message");
    core.ticket_input(Input::Formed { t: q, step: 1, head: Some(json!({"head": "Answer", "text": "the middle two"})), why: None, not_taken: None, call: None }).unwrap();
    assert!(!core.tickets.m.tickets[&q].is_open());
    assert!(core.to_hand_over().is_empty() && channel(&mut core, &mut rx).is_none(), "nothing comes again");
    // a later post: nothing is left owed to take its turn's last message, which is its reply
    let w = open(&mut core, "w3", &["w2"], "is the second run green?");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("w3's post");
    assert_eq!(ids, [w.clone()]);
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    end_turn(&mut core, &dir, "Yes, green.");
    let x = &core.tickets.m.tickets[&t_of(&w)];
    assert_eq!((x.posts.len(), x.posts.last().map(|p| p.by.as_str())), (2, Some("w2")), "the reply is posted on w3's thread");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A start in a turn begun with a post its last message replies to (consult-3 N-1396 p280): w2's turn began with its
/// lead's post. The seek goes in, and the turn ends with no `coord answer`: the reply lands on the lead's thread, and
/// the seek, owed and taken, is no turn's oldest taken step (`answered_by_turn`). It comes again (`.again`, its why
/// `RemindWhy::Left`), after the owner's post that came meanwhile, whose turn's end is his reply; then alone, with no
/// `next` (delivery.kind's `free`), and its own turn answers it.
#[test]
fn a_seek_left_in_a_turn_begun_with_a_post_comes_again_alone_and_its_own_turn_answers_it() {
    let (dir, _, mut core, mut rx) = coord("seek-post", "idle");
    let (id, i) = item(&mut core, "faster replay");
    let l = open(&mut core, "lead", &["w2"], "take the faster replay, please");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("the lead's post");
    assert_eq!(ids, [l.clone()]);
    core.handed("w2", &ids, true, "channel");
    working(&mut core);
    assert!(core.item_start(&Who::Agent("w2".into()), &id).ok);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, tickets, _, _, into, _) = channel(&mut core, &mut rx).expect("the seek, into its turn");
    assert_eq!((ids.len(), into), (1, true));
    let (seek, c) = (ids[0].clone(), tickets[0].0.t);
    core.handed("w2", &ids, true, "channel");
    end_turn(&mut core, &dir, "Taken: I start it now.");
    let x = &core.tickets.m.tickets[&t_of(&l)];
    assert_eq!((x.posts.len(), x.posts.last().map(|p| p.by.as_str())), (2, Some("w2")), "the reply is posted on the lead's thread");
    let (step, taken, forming) = owed(&core, c).expect("the seek is owed still");
    assert!(taken && !forming, "taken in the turn, not answered by its end");
    // the owner writes before the round: his post goes first, alone, the seek's reminder named at its foot
    let o = open(&mut core, "owner", &["w2"], "how far is the replay?");
    let again = format!("{seek}.again");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, _, into, rest) = channel(&mut core, &mut rx).expect("his post");
    assert_eq!((ids.clone(), into, rest), (vec![o.clone()], false, vec![again.clone()]));
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    // a round sees his turn run: the reminder, decided at that free, does not go into it
    core.raise_delivery();
    assert!(core.tickets.m.delivery_queue("w2").is_empty() && !core.tickets.m.deciding("w2"), "no arrived for the reminder");
    end_turn(&mut core, &dir, "Half: the second run is next.");
    let x = &core.tickets.m.tickets[&t_of(&o)];
    assert_eq!((x.posts.len(), x.posts.last().map(|p| p.by.as_str())), (2, Some("w2")), "his turn's end is his reply");
    assert_eq!(owed(&core, c), Some((step, true, false)), "and not the seek's answer");
    // the reminder alone; `next` is not asked (a Rest would keep it); its own turn's end answers the seek
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert!(!core.tickets.m.deciding("w2"), "no next asked");
    let (ids, tickets, _, _, into, rest) = channel(&mut core, &mut rx).expect("the seek again");
    assert_eq!((ids.clone(), into, rest.len()), (vec![again.clone()], false, 0));
    let docket_machine::What::Step { again: true, left, why, .. } = &tickets[0].0.what else { panic!("{:?}", tickets[0].0) };
    assert_eq!((*left, why.as_deref()), (true, None), "left in its turn, not a reading's why");
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    end_turn(&mut core, &dir, "Nothing does part of it: I searched replay in docket-machine.");
    assert_eq!(owed(&core, c), Some((step, true, true)), "its own turn's end answers the seek");
    core.ticket_input(Input::Formed { t: c, step, head: Some(json!({"head": "Nothing", "searched": "replay in docket-machine"})), why: None, not_taken: None, call: None }).unwrap();
    let p = core.tickets.m.tickets[&i].posts.last().cloned().unwrap();
    assert_eq!((p.by.as_str(), p.text), ("w2", Text::New("Nothing does part of it yet. Searched: replay in docket-machine".into())));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A step `free` gives just as a turn begins (consult-3 N-1396 p300, 2): w2 is idle and a question's step waits;
/// `free` delivers it, and before it is handed a turn begins with nothing of coord's (a background job's notice, or
/// his terminal). The step goes into that turn (`into`); the turn ends with no `coord answer`, and its end does not
/// answer the step as the oldest taken step owed: it comes again (`RemindWhy::Left`), and its own turn answers it.
#[test]
fn a_step_free_gives_as_a_turn_begins_goes_into_it_and_left_there_comes_again() {
    let (dir, _, mut core, mut rx) = coord("free-into", "idle");
    let q = question(&mut core, "which median?");
    let s = format!("{}.s1", core.tickets.m.id(q));
    assert_eq!(core.to_hand_over(), ["w2"]);
    // before it is handed, a turn begins with nothing of coord's, and a round sees it run
    core.begin_turn("w2");
    core.raise_delivery();
    let (ids, _, _, _, into, _) = channel(&mut core, &mut rx).expect("the step, into the turn");
    assert_eq!((ids.clone(), into), (vec![s.clone()], true));
    core.handed("w2", &ids, true, "channel");
    end_turn(&mut core, &dir, "The logs are read.");
    assert_eq!(owed(&core, q), Some((1, true, false)), "taken in the turn, not answered by its end");
    // it comes again, out of a turn; its own turn's end answers it
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, tickets, _, _, into, _) = channel(&mut core, &mut rx).expect("the step again");
    assert_eq!((ids.clone(), into), (vec![format!("{s}.again")], false));
    assert!(matches!(&tickets[0].0.what, docket_machine::What::Step { again: true, left: true, why: None, .. }), "{:?}", tickets[0].0);
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    end_turn(&mut core, &dir, "The median of the middle two.");
    assert_eq!(owed(&core, q), Some((1, true, true)), "its own turn's end answers it");
    let _ = std::fs::remove_dir_all(&dir);
}

/// paper-research N-1397.p81's case (docket-core on build 34, raised nothing from 16:18Z for two hours, its lead's
/// orders waiting): w2's turn began with w3's post on thread A; its lead's post on A went into it; w2 answered that
/// with the reply tool's Nothing, which sends coordd nothing; and the turn ended with Nothing. The post put in entered
/// its input, so it is read at that end, as the tool promised (p91), and nothing stays given. On the text before
/// ([`before`]), whose `arrived` put a lead's post into its worker's turn: a start after it runs agent-base's text, as
/// the install does. The coord, its core, its jobs, A.
fn put_in_and_left(tag: &str) -> (PathBuf, Core, UnboundedReceiver<Job>, u64) {
    let (dir, core, rx, a, _) = put_in_and_left_with(tag, false);
    (dir, core, rx, a)
}

/// [`put_in_and_left`], and when HIS, his post on thread H put into the turn too, after the lead's: his is not read
/// at the turn's end, which did not answer it, but due again (`his_post_put_into_a_turn_…`). H's post's id too.
fn put_in_and_left_with(tag: &str, his: bool) -> (PathBuf, Core, UnboundedReceiver<Job>, u64, Option<String>) {
    let (dir, _, mut core, mut rx) = coord(tag, "idle");
    before(&dir, &mut core);
    let w = open(&mut core, "w3", &["w2"], "is build 34 in?");
    let a = t_of(&w);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("w3's post");
    assert_eq!(ids, [w]);
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    core.ticket_input(Input::Post { t: a, by: "lead".into(), text: Text::New("it is; cut build 35 now".into()), names: vec!["w2".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let l = format!("{}.p2", core.tickets.m.id(a));
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, _, into, _) = channel(&mut core, &mut rx).expect("the lead's post, into the turn");
    assert_eq!((ids.clone(), into), (vec![l], true));
    core.handed("w2", &ids, true, "channel");
    assert_eq!((core.tickets.m.era, era_lines(&dir)), (docket_machine::ERA_PUT_IN, 1), "the first post put in begins the era");
    let h = his.then(|| {
        let h = open(&mut core, "owner", &["w2"], "and the release notes?");
        assert_eq!(core.to_hand_over(), ["w2"]);
        let (ids, _, _, _, into, _) = channel(&mut core, &mut rx).expect("his post, into the turn");
        assert_eq!((ids.clone(), into), (vec![h.clone()], true));
        core.handed("w2", &ids, true, "channel");
        h
    });
    end_turn(&mut core, &dir, "Nothing");
    assert!(core.ticket_given_since("w2").is_empty(), "the post put in is read at the turn's end: {:?}", core.ticket_given_since("w2"));
    let x = &core.tickets.m.tickets[&a];
    assert!(x.given.iter().filter(|g| g.to == "w2").all(|g| g.read.is_some()), "both read: {:?}", x.given);
    assert_eq!(x.posts.len(), 2, "Nothing posts nothing");
    (dir, core, rx, a, h)
}

/// The `free` events raised for w2 so far, in the record of DIR.
fn frees(dir: &Path) -> usize {
    let lines = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap_or_default();
    lines.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["ev"] == "event" && v["name"] == "free" && v["value"]["agent"] == "w2").count()
}

/// Whether V is a line beginning the era ERA_PUT_IN (docket_machine::ERA; w2 and w3, named like tickets, begin the
/// era ERA_NAMES before it).
fn put_in_era(v: &Value) -> bool {
    v["ev"] == "event" && v["name"] == docket_machine::ERA && v["value"]["era"] == docket_machine::ERA_PUT_IN
}

/// The lines beginning the era ERA_PUT_IN in the record of DIR.
fn era_lines(dir: &Path) -> usize {
    let lines = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap_or_default();
    lines.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(put_in_era).count()
}

/// A start on a record from before the era ERA_PUT_IN (build 34's, where p81's case left docket-core silent): p81's
/// case, his post on H put into the turn beside the lead's, with the era's line taken out, so the replay keeps both
/// given, the next turn's, as build 34 did. The start begins the era (`load_tickets`), and its line reads the lead's
/// post and makes his due again (paper-research N-412.p2197, 4): w2, running on, is handed his alone, said to be
/// handed again, and its turn's reply is posted under it. A post for it later raises `free` and is handed. An agent
/// left silent so is freed by the install.
#[test]
fn a_start_on_a_record_from_before_the_era_reads_the_post_a_turn_carried() {
    let (dir, core, _rx, a, h) = put_in_and_left_with("put-start", true);
    let h = h.unwrap();
    drop(core);
    let log = dir.join("tickets/log.jsonl");
    let before: String = std::fs::read_to_string(&log).unwrap().lines().filter(|l| serde_json::from_str::<Value>(l).is_ok_and(|v| !put_in_era(&v))).map(|l| format!("{l}\n")).collect();
    std::fs::write(&log, before).unwrap();
    assert_eq!(era_lines(&dir), 0);
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let (mut core, mut rx) = load_with(&dir, &base, "idle", true);
    assert_eq!((core.tickets.m.era, era_lines(&dir)), (docket_machine::ERA_PUT_IN, 1), "begun at the start");
    assert!(core.ticket_given_since("w2").is_empty(), "the carried posts are given no more: {:?}", core.ticket_given_since("w2"));
    let l = format!("{}.p2", core.tickets.m.id(a));
    assert!(core.tickets.m.tickets[&a].given.iter().any(|g| g.to == "w2" && g.delivery == l && g.read.is_some()), "the lead's read at the era's line");
    let x = &core.tickets.m.tickets[&t_of(&h)];
    assert!(x.given.iter().all(|g| g.read.is_none()) && x.given.iter().any(|g| g.left), "his not read, due again: {:?}", x.given);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, again, into, _) = channel(&mut core, &mut rx).expect("his post again");
    assert_eq!((ids.clone(), again, into), (vec![h.clone()], vec![h.clone()], false), "his alone, said to be handed again");
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    end_turn(&mut core, &dir, "The notes are in N-9.");
    let x = &core.tickets.m.tickets[&t_of(&h)];
    assert_eq!((x.posts.len(), x.posts.last().map(|p| p.by.as_str())), (2, Some("w2")), "its turn's reply is under his post");
    assert!(x.given.last().is_some_and(|g| g.read.is_some()));
    let b = open(&mut core, "w3", &["w2"], "is the second run green?");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("B's post");
    assert_eq!(ids, [b]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// After p81's case (`put_in_and_left`) w2 is idle with nothing given: nothing waits, so no `free`; a post on thread B
/// later raises one and is handed (on build 34 none was: the post put in stayed given, and `free` waits for nothing
/// given). That turn holds B alone, and its last message is posted on B alone.
#[test]
fn a_post_put_into_a_turn_and_left_there_is_read_and_a_later_post_raises_free() {
    let (dir, mut core, mut rx, a) = put_in_and_left("put-left");
    let before = frees(&dir);
    assert!(core.to_hand_over().is_empty() && !core.tickets.m.deciding("w2"), "nothing waits");
    assert_eq!(frees(&dir), before);
    let b = open(&mut core, "w3", &["w2"], "is the second run green?");
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert_eq!(frees(&dir), before + 1, "the post on B raised free");
    let (ids, ..) = channel(&mut core, &mut rx).expect("B's post");
    assert_eq!(ids, [b.clone()]);
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    end_turn(&mut core, &dir, "Green, both runs.");
    let by_w2 = |core: &Core, t: u64| core.tickets.m.tickets[&t].posts.iter().filter(|p| p.by == "w2").count();
    assert_eq!((by_w2(&core, a), by_w2(&core, t_of(&b))), (0, 1), "its last message goes to B alone");
    assert!(core.ticket_given_since("w2").is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// After p81's case (`put_in_and_left`), the owner's words typed into w2's idle terminal (`Core::typed`) are given to
/// it in the turn they begin (on build 34 they were not: "typed into a turn already given something"), so that turn's
/// last message is posted on their thread.
#[test]
fn words_typed_to_an_agent_idle_after_a_post_put_in_and_left_are_given() {
    let (dir, mut core, _rx, a) = put_in_and_left("put-typed");
    let transcript = dir.join("w2.jsonl");
    let mut all = std::fs::read_to_string(&transcript).unwrap_or_default();
    let offset = all.len() as u64;
    let words = format!("{}\n", json!({"type": "user", "timestamp": "2026-11-20T16:40:05.000Z", "message": {"role": "user", "content": "is build 35 cut?"}}));
    all.push_str(&words);
    std::fs::write(&transcript, &all).unwrap();
    // his keys, from his app, just before the entry
    let keys = coord_types::pty::keys(&dir.display().to_string(), "w2");
    std::fs::create_dir_all(keys.parent().unwrap()).unwrap();
    let ms = crate::transcript::epoch_ms("2026-11-20T16:40:04.000Z").unwrap();
    std::fs::write(&keys, format!("{}\n", json!({"ms": ms, "pid": 1, "cgroup": "/user.slice/app.slice/yaaihh.scope"}))).unwrap();
    core.typed("w2", Loc { file: transcript.display().to_string(), offset, len: words.len() as u64 });
    let x = core.tickets.m.tickets.values().find(|x| x.kind == "thread" && x.t != a && x.posts.first().is_some_and(|p| p.by == "owner")).expect("his thread").clone();
    assert_eq!(x.posts[0].typed.as_deref(), Some("w2"));
    assert!(x.given.iter().any(|g| g.to == "w2"), "given in the turn they begin: {:?}", x.given);
    assert_eq!(core.ticket_given_since("w2"), [format!("{}.p1", core.tickets.m.id(x.t))]);
    core.begin_turn("w2");
    end_turn(&mut core, &dir, "Not yet: the check runs.");
    let y = &core.tickets.m.tickets[&x.t];
    assert_eq!((y.posts.len(), y.posts.last().map(|p| p.by.as_str())), (2, Some("w2")), "the reply is on his thread");
    let _ = std::fs::remove_dir_all(&dir);
}

/// w2's turn began with w3's post on thread A; his post on thread H went into it (the first post put in begins the era
/// ERA_PUT_IN); w2 answered it with the reply tool when ANSWER is some; and the turn ended with END. The coord, its
/// core, its jobs, A, his post's id.
fn his_put_in(tag: &str, answer: Option<&str>, end: &str) -> (PathBuf, Core, UnboundedReceiver<Job>, u64, String) {
    let (dir, _, mut core, mut rx) = coord(tag, "idle");
    his_into(&dir, &mut core);
    let w = open(&mut core, "w3", &["w2"], "is build 35 in?");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("w3's post");
    assert_eq!(ids, [w.clone()]);
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    let h = open(&mut core, "owner", &["w2"], "and the release notes?");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, _, into, _) = channel(&mut core, &mut rx).expect("his post, into the turn");
    assert_eq!((ids.clone(), into), (vec![h.clone()], true));
    core.handed("w2", &ids, true, "channel");
    if let Some(text) = answer {
        core.ticket_input(Input::Post { t: t_of(&h), by: "w2".into(), text: Text::New(text.into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: Some(1), kind: None }).unwrap();
    }
    end_turn(&mut core, &dir, end);
    (dir, core, rx, t_of(&w), h)
}

/// His post put into a turn that ends with Nothing (paper-research N-412.p2194): not read, and nothing stays given; it
/// is due again, raises `free` by itself with nothing else waiting (p2197, 3), and is handed first and alone, said to
/// be handed again. That turn's reply is posted under it, and it is read: it does not come a third time.
#[test]
fn his_post_put_into_a_turn_that_ends_with_nothing_comes_again_alone_and_its_turn_replies_under_it() {
    let (dir, mut core, mut rx, a, h) = his_put_in("his-left", None, "Nothing");
    assert!(core.ticket_given_since("w2").is_empty(), "nothing stays given: {:?}", core.ticket_given_since("w2"));
    assert!(core.tickets.m.tickets[&a].given.iter().all(|g| g.read.is_some()), "w3's post, the turn's own, read");
    let x = &core.tickets.m.tickets[&t_of(&h)];
    assert!(x.given.iter().all(|g| g.read.is_none()) && x.given.iter().any(|g| g.left), "his not read, due again: {:?}", x.given);
    assert_eq!(x.posts.len(), 1, "Nothing posts nothing");
    let before = frees(&dir);
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert_eq!(frees(&dir), before + 1, "his post, due again, raised free by itself");
    let (ids, _, _, again, into, rest) = channel(&mut core, &mut rx).expect("his post again");
    assert_eq!((ids.clone(), again, into, rest.len()), (vec![h.clone()], vec![h.clone()], false, 0), "alone, said to be handed again");
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    end_turn(&mut core, &dir, "The notes are in N-9.");
    let x = &core.tickets.m.tickets[&t_of(&h)];
    assert_eq!((x.posts.len(), x.posts.last().map(|p| p.by.as_str())), (2, Some("w2")), "its turn's reply is under his post");
    assert!(x.given.last().is_some_and(|g| g.read.is_some()), "read now: {:?}", x.given);
    assert!(core.to_hand_over().is_empty() && core.ticket_given_since("w2").is_empty(), "not a third time");
    assert!(core.tickets.m.owed_posts("w2").is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The pair (paper-research N-412.p2194): his post put into a turn and answered there with the reply tool is read at
/// the turn's end, and does not come again.
#[test]
fn his_post_put_into_a_turn_and_answered_there_with_the_reply_tool_is_read_and_does_not_come_again() {
    let (dir, mut core, _rx, _, h) = his_put_in("his-tool", Some("The notes are in N-9."), "Nothing");
    let x = &core.tickets.m.tickets[&t_of(&h)];
    assert!(x.given.iter().all(|g| g.read.is_some() && !g.left), "read at the turn's end: {:?}", x.given);
    assert_eq!(x.posts.len(), 2);
    let before = frees(&dir);
    assert!(core.to_hand_over().is_empty() && core.tickets.m.waiting("w2").is_empty(), "nothing comes again");
    assert_eq!(frees(&dir), before);
    let _ = std::fs::remove_dir_all(&dir);
}

/// What answers his post put into a turn (paper-research N-412.p2197, 2): the turn's last message, when it is posted on
/// his post's own thread after it, as the rule Reply posts it for a turn begun on that thread: read, and it does not
/// come again. Posted on another thread, the turn begun there, it answers him nothing, and his post comes again.
#[test]
fn his_post_put_into_a_turn_is_answered_by_its_last_message_on_his_thread_not_on_another() {
    // on his thread: the turn began on A, and his post on A went into it
    let (dir, _, mut core, mut rx) = coord("his-thread", "idle");
    his_into(&dir, &mut core);
    let w = open(&mut core, "w3", &["w2"], "is build 35 in?");
    let a = t_of(&w);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("w3's post");
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    core.ticket_input(Input::Post { t: a, by: "owner".into(), text: Text::New("@w2 and the release notes?".into()), names: vec!["w2".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let o = format!("{}.p2", core.tickets.m.id(a));
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, _, into, _) = channel(&mut core, &mut rx).expect("his post, into the turn");
    assert_eq!((ids.clone(), into), (vec![o.clone()], true));
    core.handed("w2", &ids, true, "channel");
    end_turn(&mut core, &dir, "Yes, in; the notes are in N-9.");
    let x = &core.tickets.m.tickets[&a];
    assert_eq!((x.posts.len(), x.posts.last().map(|p| (p.n, p.by.as_str()))), (3, Some((3, "w2"))), "its last message, after his post");
    assert!(x.given.iter().all(|g| g.read.is_some() && !g.left), "his read: {:?}", x.given);
    assert!(core.to_hand_over().is_empty() && core.tickets.m.waiting("w2").is_empty(), "nothing comes again");
    let _ = std::fs::remove_dir_all(&dir);
    // on another thread: the turn began on A, his post on H went into it, and its last message is on A
    let (dir, mut core, mut rx, a, h) = his_put_in("his-other", None, "Yes, in.");
    assert_eq!(core.tickets.m.tickets[&a].posts.last().map(|p| p.by.as_str()), Some("w2"), "its last message is on A");
    let x = &core.tickets.m.tickets[&t_of(&h)];
    assert!(x.given.iter().all(|g| g.read.is_none()) && x.given.iter().any(|g| g.left), "his not answered, not read: {:?}", x.given);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("his post again");
    assert_eq!(ids, [h]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// His post left unanswered in two turns (paper-research N-412.p2197, 1; consult-3 p2198, 1): put into one that ended
/// with Nothing, handed again alone, and its own turn ending with Nothing too. It is not handed a third time and not
/// read; it is out of what w2 was given, so another agent's post to w2 raises `free` and begins a turn, and the foot
/// of that turn's message names his post. The line under it says it is owed (`ViewGiven::owed`, the window's
/// `delivered · not answered`). Answered with the reply tool later, it is named no more, and read at the next turn's end.
#[tokio::test]
async fn his_post_left_unanswered_in_two_turns_is_owed_not_handed_again_and_named_at_the_foot() {
    let (dir, mut core, mut rx, _, h) = his_put_in("his-owed", None, "Nothing");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("his post again");
    assert_eq!(ids, [h.clone()]);
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    end_turn(&mut core, &dir, "Nothing");
    let x = &core.tickets.m.tickets[&t_of(&h)];
    assert_eq!(x.posts.len(), 1, "Nothing posts nothing");
    assert!(x.given.iter().all(|g| g.read.is_none()), "not read: {:?}", x.given);
    assert!(core.ticket_given_since("w2").is_empty(), "out of what it was given");
    assert_eq!(core.tickets.m.owed_posts("w2"), [h.clone()]);
    let rows = core.tickets.m.view(t_of(&h)).unwrap().given;
    assert_eq!(rows.iter().map(|g| g.owed).collect::<Vec<_>>(), [false, true], "the last giving is owed: {rows:?}");
    let before = frees(&dir);
    assert!(core.to_hand_over().is_empty() && core.tickets.m.waiting("w2").is_empty(), "not handed a third time");
    assert_eq!(frees(&dir), before);
    // another agent's post: `free`, a turn, and his post at its foot
    let b = open(&mut core, "w3", &["w2"], "is the second run green?");
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert_eq!(frees(&dir), before + 1, "w3's post raised free");
    let (ids, tickets, replies_on, again, into, rest) = channel(&mut core, &mut rx).expect("w3's post");
    assert_eq!(ids, [b.clone()]);
    core.handed("w2", &ids, true, "channel");
    let shared = crate::Shared::new(core);
    let out = crate::rendered_all(&shared, "w2", &tickets, replies_on.as_deref(), &again, into, false, &rest, &[]).await;
    assert!(out.last().unwrap().0.ends_with(&format!("\n\nNot answered yet, and not handed to you again: {h} from owner. Answer with the reply tool, to its id.")), "{}", out.last().unwrap().0);
    let mut core = Arc::try_unwrap(shared.0).ok().unwrap().into_inner();
    core.begin_turn("w2");
    // answered with the reply tool in that turn: named no more, and read at its end
    core.ticket_input(Input::Post { t: t_of(&h), by: "w2".into(), text: Text::New("The notes are in N-9.".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: Some(1), kind: None }).unwrap();
    assert!(core.tickets.m.owed_posts("w2").is_empty());
    end_turn(&mut core, &dir, "Green.");
    let x = &core.tickets.m.tickets[&t_of(&h)];
    assert!(x.given.last().is_some_and(|g| g.read.is_some()), "read: {:?}", x.given);
    assert!(core.tickets.m.view(t_of(&h)).unwrap().given.iter().all(|g| !g.owed));
    let c = open(&mut core, "w3", &["w2"], "and the third?");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, tickets, replies_on, again, into, rest) = channel(&mut core, &mut rx).expect("w3's third");
    assert_eq!(ids, [c]);
    let shared = crate::Shared::new(core);
    let out = crate::rendered_all(&shared, "w2", &tickets, replies_on.as_deref(), &again, into, false, &rest, &[]).await;
    assert!(!out.last().unwrap().0.contains("Not answered yet"), "{}", out.last().unwrap().0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A round with nothing new for any agent (paper-research N-1397.p38: build 31's round read every agent's due and built
/// what waits for it, each second, about 170 ms of research's lock): no agent is read, and the machine is not passed
/// over for what is due. w2 turning to work reads it alone, the machine as it was. A round after a change reads only
/// the agents whose own part of the machine it changed (N-1397.p58: about 5 s of the lock a minute when it read every
/// agent): a post for w2 reads w2, and one for w3 reads w3.
#[test]
fn a_round_with_nothing_new_reads_no_agent_and_no_due() {
    let (dir, _, mut core, _rx) = coord("still", "idle");
    // w2 idle with an fyi waiting, on its own thread to its lead: pulled, it raises no `free` (I-1480)
    let own = t_of(&open(&mut core, "w2", &["lead"], "the replay plan"));
    let a = typed(&mut core, own, "w3", &["w2"], PostKind::Fyi, "taken");
    assert!(core.to_hand_over().is_empty());
    let waits = |core: &Core, i: &str| core.tickets.m.waiting("w2").iter().any(|w| w["id"] == json!(i));
    assert!(waits(&core, &a));
    let reads = |core: &Core| ["lead", "w2", "w3"].map(|n| core.desk_seen.get(n).map_or(0, |s| s.reads));
    // the round after reads what that round changed; from then on nothing is new
    let settled = |core: &mut Core| {
        for _ in 0..2 {
            assert!(core.to_hand_over().is_empty());
        }
        (reads(core), core.tickets.due_passes())
    };
    let (r, p) = settled(&mut core);
    for _ in 0..3 {
        assert!(core.to_hand_over().is_empty());
    }
    assert_eq!((reads(&core), core.tickets.due_passes()), (r, p), "nothing new: no agent read, no pass over the machine");
    // something new: read, a pass, and no `free` (pulled too)
    let b = typed(&mut core, own, "w3", &["w2"], PostKind::Fyi, "is the second run green?");
    assert!(core.to_hand_over().is_empty());
    let now = reads(&core);
    assert!(now[1] > r[1] && core.tickets.due_passes() > p);
    assert_eq!(now[2], r[2], "a post for w2: its author is not read (the lead, on w2's thread, takes part)");
    assert!(waits(&core, &a) && waits(&core, &b));
    // w2 works: it alone is read, and what it reads is kept from the round before
    let (r, p) = settled(&mut core);
    working(&mut core);
    assert!(core.to_hand_over().is_empty());
    assert_eq!((reads(&core), core.tickets.due_passes()), ([r[0], r[1] + 1, r[2]], p));
    // a post for w3 alone, which is not running: w3 is read and its post reported; w2 at work is not read
    let (r, p) = settled(&mut core);
    let c = open(&mut core, "lead", &["w3"], "the fixture for w3");
    assert!(core.to_hand_over().is_empty());
    assert!(core.tickets.due_passes() > p);
    assert_eq!(reads(&core), [r[0], r[1], r[2] + 1]);
    assert!(core.tickets.m.reported("w3").contains_key(&c), "{:?}", core.tickets.m.reported("w3"));
    // the round after reads what the report changed, of w3 alone, and then nothing is new
    let (s, _) = settled(&mut core);
    assert_eq!(s, [r[0], r[1], r[2] + 2]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A message handed and on its way changes what is on its way (`Tickets::handed`), not the machine: the round after
/// reads w2 once, from what it is now (whether a message of its goes now reads what is on its way too), and a post for
/// w3 alone then reads w3, not w2 (on research's record the cost harness's change round read two such agents again).
#[test]
fn a_message_on_its_way_reads_its_agent_once_and_a_post_for_another_reads_that_one() {
    let (dir, _, mut core, mut rx) = coord("onway", "idle");
    let reads = |core: &Core| ["lead", "w2", "w3"].map(|n| core.desk_seen.get(n).map_or(0, |s| s.reads));
    let a = open(&mut core, "lead", &["w2"], "the fixture is in place");
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert_eq!(core.to_hand_over(), ["w2"], "nothing on its way yet");
    let (ids, ..) = channel(&mut core, &mut rx).expect("a hand-over");
    assert_eq!(ids, [a]);
    let r = reads(&core);
    assert!(core.to_hand_over().is_empty(), "on its way");
    assert_eq!(reads(&core), [r[0], r[1] + 1, r[2]], "w2 read once, for what is on its way");
    assert!(core.to_hand_over().is_empty());
    assert_eq!(reads(&core), [r[0], r[1] + 1, r[2]]);
    let c = open(&mut core, "lead", &["w3"], "the fixture for w3");
    assert!(core.to_hand_over().is_empty());
    assert_eq!(reads(&core), [r[0], r[1] + 1, r[2] + 1], "a post for w3 reads w3 alone");
    assert!(core.tickets.m.reported("w3").contains_key(&c));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A session whose coord server answers on no channel socket (paper-research N-1397.p38, 3: yaaihh's, for an hour, read
/// as idle, a `next` call at each `free`): stopped, with why; what waits is reported, no `free` is raised, and nothing
/// is handed to it. Once its folder changes (channel.py binds a socket there) it is idle again: `free`, which hands it.
#[test]
fn a_session_no_channel_socket_answers_is_stopped_until_one_is_made() {
    let (dir, _, mut core, _rx) = coord("unreached", "idle");
    let why = "no channel socket in agents/w2 answers (channel-s.sock: Connection refused (os error 111)): channel.py is not running";
    core.unreached("w2", why);
    let a = open(&mut core, "lead", &["w2"], "the fixture is in place");
    assert!(core.to_hand_over().is_empty(), "nothing handed");
    let stopped = format!("its session takes nothing: {why}");
    assert_eq!(core.agent_state("w2"), Some(crate::core::AgentState::Stopped { why: stopped.clone() }));
    assert!(!core.tickets.m.deciding("w2") && core.ticket_due(WATCHER).is_empty(), "no free, no next");
    assert_eq!(core.tickets.m.reported("w2").get(&a), Some(&stopped));
    assert!(core.to_hand_over().is_empty(), "still stopped while its folder stands");
    assert!(core.agents["w2"].live.unreached.is_some());
    // channel.py binds a socket in its folder: reached again, idle, and its `free` hands what was reported
    core.agents.get_mut("w2").unwrap().live.unreached.as_mut().unwrap().1 -= std::time::Duration::from_secs(2);
    let folder = dir.join("agents/w2");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("channel-s.sock"), "").unwrap();
    assert_eq!(core.to_hand_over(), ["w2"], "free once it is reached again");
    assert!(core.agents["w2"].live.unreached.is_none());
    assert_eq!(core.agent_state("w2"), Some(crate::core::AgentState::Idle));
    assert_eq!(core.hand_now("w2", false).items.iter().map(|d| d.id.clone()).collect::<Vec<_>>(), [a]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A session whose Claude Code said on its screen that it takes no channel message (its holder's `Request::Busy`
/// `channels_off`; paper-research N-1446 p2424: a scratch HOME signed in nowhere, accounts-lead's walk p2423): stopped,
/// with Claude Code's words, and handed nothing, since what it took would be dropped unseen; its view says why and what
/// he can do, and so does the answer to his message to it, never only "Taken". Its holder telling nothing of it (a new
/// screen, a holder from before) ends the stop: free, and what waited is handed.
#[test]
fn a_session_whose_claude_code_takes_no_channel_message_is_handed_nothing_and_says_why() {
    let (dir, _, mut core, _rx) = coord("chanoff", "idle");
    let why = "Channels are not currently available";
    core.channels_off("w2", Some(why));
    let a = open(&mut core, "lead", &["w2"], "the fixture is in place");
    assert!(core.to_hand_over().is_empty(), "nothing handed");
    let stopped = format!("its session takes no messages: Claude Code says “{why}”");
    assert_eq!(core.agent_state("w2"), Some(crate::core::AgentState::Stopped { why: stopped.clone() }));
    assert_eq!(core.tickets.m.reported("w2").get(&a), Some(&stopped));
    let words = format!("its session takes no messages (Claude Code says “{why}”); pick an account for this agent");
    assert_eq!(core.view("w2").deaf, Some(words.clone()));
    assert_eq!(core.view("w3").deaf, None, "another agent takes them");
    let r = core.ticket_say_under(&Who::Owner, "w2".into(), vec!["w2".into()], vec![], "and the printer".into(), None, None, None, true);
    assert_eq!(r.out, format!("Taken: to w2. Not delivered: {words}."));
    assert!(core.to_hand_over().is_empty(), "still nothing handed");
    // an organization's policy: another account
    core.channels_off("w2", Some("Inbound messages will be silently dropped"));
    assert_eq!(core.view("w2").deaf.as_deref(), Some("its session takes no messages (Claude Code says “Inbound messages will be silently dropped”); pick another account for this agent"));
    // its holder tells nothing of it: free, and what waited is handed
    core.channels_off("w2", None);
    assert_eq!(core.view("w2").deaf, None);
    assert_eq!(core.to_hand_over(), ["w2"], "free once its screen says nothing of it");
    assert!(core.hand_now("w2", false).items.iter().any(|d| d.id == a));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A turn that ended on Claude Code's sign-in error (accounts-lead N-1446 p2446, fix 2: a new agent on a coord with no
/// sign-in read "working (a turn is running)" for good, the hooks deciding, and Restart was refused): its holder's word
/// ends the turn the hooks hold, as a StopFailure would, so a restart is not held for its end; it is handed nothing
/// while the line stands, its state and view say why in the screen's words; the line gone (a /login typed), free again.
#[test]
fn a_turn_ended_on_claude_codes_sign_in_error_is_over_and_says_so() {
    let (dir, _, mut core, _rx) = coord("nosignin", "working");
    core.hook_busy("w2", true);
    let why = "Not logged in · Please run /login";
    core.no_sign_in("w2", Some(why));
    assert_eq!(core.agents["w2"].live.hook_busy, Some(false), "the turn the hooks held is over");
    assert_eq!(core.agents["w2"].failed.as_deref(), Some(why));
    let a = open(&mut core, "lead", &["w2"], "the fixture is in place");
    assert!(core.to_hand_over().is_empty(), "nothing handed while the line stands");
    assert_eq!(core.agent_state("w2"), Some(crate::core::AgentState::Stopped { why: format!("its turn failed: Claude Code says “{why}”") }));
    assert_eq!(core.view("w2").deaf, Some(format!("its turn failed and it takes no messages (Claude Code says “{why}”); pick an account for this agent")));
    core.no_sign_in("w2", None);
    assert_eq!(core.view("w2").deaf, None);
    core.set_state("w2", "idle");
    assert_eq!(core.to_hand_over(), ["w2"], "the line gone: free");
    assert!(core.hand_now("w2", false).items.iter().any(|d| d.id == a));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The coord's project, of no kinds or calls of its own: where a draft is saved (`files/kinds`, `files/calls`).
fn project(dir: &Path, core: &mut Core) -> PathBuf {
    let files = dir.join("files");
    std::fs::create_dir_all(files.join("kinds")).unwrap();
    std::fs::create_dir_all(files.join("calls")).unwrap();
    let mut p: crate::project::Project = toml::from_str("[project]\nname = \"x\"\n").unwrap();
    p.dir = files.display().to_string();
    core.project = Some(p);
    files
}

/// delivery.kind as it was before I-1480 c, dm's copy of that text (`free` asks the watcher's `next`, which coordd
/// answers with no call: delivery.rs `next_given`; a post from one above an agent goes into its turn): what a record
/// from before runs (proto-engine N-1481 p94, 4).
const NEXT_KIND: &str = include_str!("../../../docket/machine/src/tests/delivery_next.kind");

/// DIR's coord set back to the text before ([`NEXT_KIND`]), by [`text_back`].
fn before(dir: &Path, core: &mut Core) -> PathBuf {
    text_back(dir, core, NEXT_KIND, "delivery.kind as it was before I-1480 c")
}

/// delivery.kind as it was before his posts waited for the turn's end too (the owner, N-1104 p716, his yes p923 to
/// proto-mind's N-1511 p28), dm's copy of that text: his post to a working agent goes into its turn at its next step
/// (N-342), unless he sent it to wait. What a record from before runs: a delivery ticket opened on it runs it until its
/// agent's next run opens one on agent-base's text. The legs of what he sends into a turn (the reply tool's footer, his
/// posts passed, the eras, a compaction, a start, the brain's line) run on it.
pub(crate) const INTO_KIND: &str = include_str!("../../../docket/machine/src/tests/delivery_into.kind");

/// DIR's coord set back to the text before p716 for his posts ([`INTO_KIND`]), as [`before`] sets it back to the one
/// before I-1480 c.
pub(crate) fn his_into(dir: &Path, core: &mut Core) -> PathBuf {
    text_back(dir, core, INTO_KIND, "delivery.kind as it was before his posts waited for the turn's end")
}

/// DIR's coord given TEXT as its project's delivery.kind (a project's kind of the same name wins: CORE's project, or a
/// new one of no kinds or calls), reloaded for WHY: each agent's next run opens its delivery ticket on it. The project
/// is CORE's alone, so a start after it reads agent-base's text, as the install does. Its project's folder.
fn text_back(dir: &Path, core: &mut Core, text: &str, why: &str) -> PathBuf {
    let files = match &core.project {
        Some(p) => PathBuf::from(&p.dir),
        None => project(dir, core),
    };
    std::fs::create_dir_all(files.join("kinds")).unwrap();
    std::fs::write(files.join("kinds/delivery.kind"), text).unwrap();
    let r = core.ticket_reload(&Who::Owner, None, why.into());
    assert!(r.ok, "{}", r.out);
    files
}

/// The reach answer R's delivery tickets: id, case, agent, words; by id.
fn desks_reached(r: &coord_types::Reply) -> Vec<(String, String, String, String)> {
    assert!(r.ok, "{}", r.out);
    let v: Value = serde_json::from_str(&r.out).unwrap();
    let s = |x: &Value, k: &str| x[k].as_str().unwrap_or_default().to_string();
    let mut out: Vec<_> = v["tickets"].as_array().unwrap().iter().filter(|x| x["kind"] == "delivery").map(|x| (s(x, "id"), s(x, "reach"), s(x, "agent"), s(x, "said"))).collect();
    out.sort();
    out
}

/// AGENT's open delivery ticket's id.
fn desk_id(core: &Core, agent: &str) -> String {
    let x = core.tickets.m.tickets.values().find(|x| x.kind == "delivery" && x.is_open() && x.form["agent"] == agent).unwrap_or_else(|| panic!("{agent} has no delivery ticket"));
    core.tickets.m.id(x.t)
}

/// The reach of `choose`'s words, its template in fold.call since proto-mind's head moved it out of next.call (N-1396
/// p301, 4; N-1446 p2105), on the text before ([`before`]):
/// no delivery ticket, though each has `next` ahead in its `free`. coordd renders no `next` step from `choose` any
/// more (`clerk::runs` gives it none; `choose` is `fold`'s, N-1446 p2097): it answers a run's `next` with no call
/// (I-1480).
#[test]
fn a_reach_of_nexts_words_names_no_delivery_ticket_now_that_next_runs_no_call() {
    let (dir, base, mut core, mut rx) = coord("reach-next", "idle");
    let files = before(&dir, &mut core);
    // w3 in a turn: a post for it is kept, and its delivery ticket has no run open
    core.agents.get_mut("w3").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "working".into(), turn: 1, ..Default::default() };
    open(&mut core, "lead", &["w3"], "the plan is up");
    core.raise_delivery();
    // w2 idle with a post waiting: its free run's `next` answered at once, the post handed
    let a = open(&mut core, "lead", &["w2"], "the fixture is in place");
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert_eq!(channel(&mut core, &mut rx).map(|h| h.0), Some(vec![a]));
    let fold = std::fs::read_to_string(format!("{base}/calls/fold.call")).unwrap();
    let new = fold.replace(", of a software team. Its turn has just ended", ", of the team. Its turn has just ended");
    assert_ne!(new, fold);
    let r = core.reach_of(&files.join("calls/fold.call"), &new);
    let v: Value = serde_json::from_str(&r.out).unwrap_or_default();
    assert_eq!(v["calls"], json!(["choose"]), "{}", r.out);
    assert!(desks_reached(&r).is_empty(), "{}", r.out);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A kind edit of delivery.kind, a `keep`'s reason (N-1396 p301, 4): the reach names each open delivery ticket
/// `ahead`, since a delivery ticket is never carried: its agent's next run opens its next one on the new text
/// (delivery.rs). Saved, no delivery ticket has a line of its own; the next post kept from w2 is kept with the new
/// reason, the one kept before with its own, and a replay keeps the same.
#[test]
fn a_reach_of_a_keeps_reason_names_every_delivery_ticket_and_saved_the_next_keep_says_it() {
    let (dir, base, mut core, _rx) = coord("reach-keep", "working");
    let files = project(&dir, &mut core);
    let a = open(&mut core, "w3", &["w2"], "is the second run green?");
    core.raise_delivery();
    assert_eq!(kept(&core, &a).as_deref(), Some("comes when its turn ends"));
    let w2 = desk_id(&core, "w2");
    let kind = std::fs::read_to_string(format!("{base}/kinds/delivery.kind")).unwrap();
    let new = kind.replace("keep [i] \"comes when its turn ends\"", "keep [i] \"waits for its turn to end\"");
    assert_ne!(new, kind);
    let path = files.join("kinds/delivery.kind");
    let r = core.reach_of(&path, &new);
    let got = desks_reached(&r);
    assert!(got.iter().any(|g| g.0 == w2), "{got:#?}");
    for (_, case, agent, said) in &got {
        assert_eq!((case.as_str(), said.clone()), ("ahead", format!("{agent}'s next delivery run opens its next delivery ticket on the new text")), "{got:#?}");
    }
    let lines = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap().lines().count();
    std::fs::write(&path, &new).unwrap();
    let r = core.ticket_reload(&Who::Owner, None, "a clearer reason".into());
    assert!(r.ok, "{}", r.out);
    let after: Vec<Value> = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap().lines().skip(lines).filter_map(|l| serde_json::from_str(l).ok()).collect();
    assert!(!after.iter().any(|l| l["ev"] == "edited"), "{after:#?}");
    let b = open(&mut core, "w3", &["w2"], "and the third?");
    core.raise_delivery();
    assert_eq!(kept(&core, &b).as_deref(), Some("waits for its turn to end"));
    assert_eq!(kept(&core, &a).as_deref(), Some("comes when its turn ends"));
    assert_ne!(desk_id(&core, "w2"), w2, "its next run opened its next delivery ticket");
    assert_eq!(core.tickets.replayed().kept("w2"), core.tickets.m.kept("w2"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// w2 is handed his post P at `free`, alone, its session gives it (at a place of its own, after any earlier one) and
/// begins its turn with it, which the round sees, and that turn is cut by his esc (the stop lands: its transcript's
/// interrupt is the cut, `main::follow`). Idle after.
fn esc_cut(core: &mut Core, rx: &mut UnboundedReceiver<Job>, p: &str) {
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(core, rx).expect("his post");
    assert_eq!(ids, [p.to_string()]);
    core.handed("w2", &ids, true, "channel");
    let (file, offset) = (format!("{}/w2.jsonl", core.coord), 100_000 + 1000 * core.agents["w2"].live.turn as u64);
    let _ = core.transcript_read("w2", vec![(p.to_string(), file.clone(), offset, 900)], vec![], vec![], &[], None, &[], false);
    core.ticket_delivered("w2", p, Loc { file, offset, len: 900 }, "2026-11-20T21:00:00Z", true);
    core.begin_turn("w2");
    core.raise_delivery();
    let r = core.ticket_esc(&Who::Owner, vec!["w2".into()]);
    assert!(r.ok && r.out == "w2's turn is being stopped", "{}", r.out);
    core.interrupted("w2");
    core.ticket_input(Input::TurnCut { agent: "w2".into() }).unwrap();
    idle(core);
}

/// His post in a turn cut short is no lost message (paper-research N-412.p2202, 2): the turn begun on it is cut by its
/// process ending, then the turn on his next post by his esc. Each time the cut leaves it as a turn's end that did not
/// answer it: due again, handed again at `free`, first and alone, said to be handed again, w3's post named at its foot.
/// The first such cut begins the era ERA_PUT_IN. Handed at `free` the first time, his post is the turn's own, and the
/// reply tool's Nothing to it is read at the turn's end: `his_to_answer` names nothing; handed again, it names it.
#[test]
fn his_post_in_a_turn_cut_by_its_process_ending_then_by_his_esc_comes_again_alone_each_time() {
    let (dir, _, mut core, mut rx) = coord("his-cut", "idle");
    let pid = std::process::id() as i32;
    let o1 = open(&mut core, "owner", &["w2"], "measure the drag");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, tickets, ..) = channel(&mut core, &mut rx).expect("his post");
    assert_eq!(ids, [o1.clone()]);
    assert!(core.tickets.m.his_to_answer("w2", &tickets[0].0, false).is_empty(), "its turn begins with it: Nothing to it is read");
    core.handed("w2", &ids, true, "channel");
    let file = dir.join("w2.jsonl").display().to_string();
    let _ = core.transcript_read("w2", vec![(o1.clone(), file.clone(), 40, 900)], vec![], vec![], &[], None, &[], false);
    core.ticket_delivered("w2", &o1, Loc { file, offset: 40, len: 900 }, "2026-11-20T21:00:00Z", true);
    core.begin_turn("w2");
    core.raise_delivery();
    // 1. its process ends: his post entered its input, so it did not go with the process; the cut leaves it unanswered
    core.exited("w2", pid);
    assert_eq!(core.tickets.m.era, docket_machine::ERA_PUT_IN, "the cut holding his post begins the era");
    assert!(core.tickets.m.tickets[&t_of(&o1)].given.iter().any(|g| g.left && g.read.is_none()), "{:?}", core.tickets.m.tickets[&t_of(&o1)].given);
    core.agents.get_mut("w2").unwrap().live = Live { pid: Some(pid), state: "idle".into(), turn: 2, ..Default::default() };
    idle(&mut core);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, tickets, _, again, into, _) = channel(&mut core, &mut rx).expect("his post again");
    assert_eq!((ids.clone(), again, into), (vec![o1.clone()], vec![o1.clone()], false), "alone, said to be handed again");
    assert_eq!(core.tickets.m.his_to_answer("w2", &tickets[0].0, false), vec![o1.clone()], "handed again: Nothing is no answer to it");
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    core.raise_delivery();
    end_turn(&mut core, &dir, "The drag is 3 px.");
    let x = &core.tickets.m.tickets[&t_of(&o1)];
    assert_eq!((x.posts.len(), x.given.last().is_some_and(|g| g.read.is_some())), (2, true), "its turn's reply under it, read");
    // 2. his esc cuts the turn on his next post; w3's post comes meanwhile
    let o2 = open(&mut core, "owner", &["w2"], "and the pane width?");
    esc_cut(&mut core, &mut rx, &o2);
    let w = open(&mut core, "w3", &["w2"], "the fixture is in place");
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert!(!core.tickets.m.deciding("w2"), "his goes first, with no call");
    let (ids, _, _, again, into, rest) = channel(&mut core, &mut rx).expect("his post again");
    assert_eq!((ids, again, into, rest), (vec![o2.clone()], vec![o2], false, vec![w]), "alone, w3's named at its foot");
    let _ = std::fs::remove_dir_all(&dir);
}

/// His esc, then his next message (consult-3 N-412.p2203; his rule: esc first, then the message): his post p1's turn is
/// cut by his esc, and he posts p2 to the same agent before its `free`. p1, due again, and p2 go in one message at that
/// `free`, in his order: on one thread as one delivery holding both, on two as one hand-over. Not p1 alone in a turn
/// and p2 in the next.
#[test]
fn his_post_in_a_turn_his_esc_cut_goes_with_his_next_post_in_one_message() {
    let (dir, _, mut core, mut rx) = coord("his-esc", "idle");
    let posts = |d: &Delivery| match &d.what {
        docket_machine::What::Posts { unread } => unread.iter().map(|p| p.n).collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    // p2 on p1's thread
    let p1 = open(&mut core, "owner", &["w2"], "use main");
    esc_cut(&mut core, &mut rx, &p1);
    let t = t_of(&p1);
    core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("@w2 and rebase first".into()), names: vec!["w2".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let both = format!("{}.p2", core.tickets.m.id(t));
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, tickets, _, _, into, _) = channel(&mut core, &mut rx).expect("one message");
    assert_eq!((ids.clone(), posts(&tickets[0].0), into), (vec![both], vec![1, 2], false), "one delivery, p1 then p2");
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    core.raise_delivery();
    end_turn(&mut core, &dir, "On main, rebased.");
    assert!(core.to_hand_over().is_empty(), "answered: nothing comes again");
    // p2 on a thread of its own
    let p1 = open(&mut core, "owner", &["w2"], "measure the drag");
    esc_cut(&mut core, &mut rx, &p1);
    let p2 = open(&mut core, "owner", &["w2"], "no, the pane width first");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, again, into, rest) = channel(&mut core, &mut rx).expect("one message");
    assert_eq!((ids, again, into, rest.len()), (vec![p1.clone(), p2], vec![p1], false, 0), "one hand-over, in his order");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The same under his Hold (paper-research N-412.p2205; his words: "keep them interrupted so i can be the one to send
/// them messages"): p1, in the turn his esc cut, waits with what the hold keeps, with its words, and nothing is handed.
/// His esc, then w3's post to w2, then his next post p2 (consult-3 N-412.p2206): p2, on a thread of its own, goes at
/// once as his posts do under Hold, and p1 goes with it in one message, in his order, while w3's post stays held and
/// unnamed; p2 on p1's own thread makes one delivery holding both. With no post of his after it, his post waits for his
/// Release, which hands it first and alone, w3's named at its foot; its turn answering it, it does not come again, and
/// w3's comes next.
#[test]
fn under_his_hold_his_post_his_esc_cut_waits_for_his_next_post_or_his_release() {
    let (dir, _, mut core, mut rx) = coord("his-esc-hold", "idle");
    assert!(core.ticket_mail_hold(&Who::Owner, true).ok);
    let why = format!("the owner holds agents' mail since {} UTC", &core.tickets.m.mail_hold.as_ref().unwrap().at[11..16]);
    let p1 = open(&mut core, "owner", &["w2"], "use main");
    esc_cut(&mut core, &mut rx, &p1);
    assert!(core.to_hand_over().is_empty(), "p1 waits under his hold");
    assert_eq!(kept(&core, &p1), Some(why.clone()));
    let w = open(&mut core, "w3", &["w2"], "is the second run green?");
    assert!(core.to_hand_over().is_empty());
    let p2 = open(&mut core, "owner", &["w2"], "no, the pane width first");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, again, _, rest) = channel(&mut core, &mut rx).expect("p2, with p1");
    assert_eq!((ids.clone(), again, rest), (vec![p1.clone(), p2.clone()], vec![p1.clone()], Vec::<String>::new()), "one message in his order; w3's held, not named");
    assert_eq!(kept(&core, &w), Some(why.clone()));
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    core.raise_delivery();
    for p in [&p1, &p2] {
        core.ticket_input(Input::Post { t: t_of(p), by: "w2".into(), text: Text::New("on it".into()), names: vec![], told: vec![], inform: vec![], answers: Some(1), kind: None }).unwrap();
    }
    end_turn(&mut core, &dir, "Nothing");
    assert!(core.to_hand_over().is_empty() && core.tickets.m.owed_posts("w2").is_empty(), "both answered");
    // his p2 on p1's own thread: one delivery holding both, in his order; w3's still held, not named
    let q = open(&mut core, "owner", &["w2"], "use main");
    esc_cut(&mut core, &mut rx, &q);
    assert!(core.to_hand_over().is_empty(), "it waits under his hold");
    let t = t_of(&q);
    core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("@w2 and rebase first".into()), names: vec!["w2".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, tickets, _, _, _, rest) = channel(&mut core, &mut rx).expect("one delivery");
    let posts: Vec<u32> = match &tickets[0].0.what {
        docket_machine::What::Posts { unread } => unread.iter().map(|p| p.n).collect(),
        _ => Vec::new(),
    };
    assert_eq!((ids.clone(), posts, rest), (vec![format!("{}.p2", core.tickets.m.id(t))], vec![1, 2], Vec::<String>::new()), "one delivery, q then p2; w3's held");
    assert_eq!(kept(&core, &w), Some(why.clone()));
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    core.raise_delivery();
    core.ticket_input(Input::Post { t, by: "w2".into(), text: Text::New("on main, rebased".into()), names: vec![], told: vec![], inform: vec![], answers: Some(2), kind: None }).unwrap();
    end_turn(&mut core, &dir, "Nothing");
    assert!(core.to_hand_over().is_empty() && core.tickets.m.owed_posts("w2").is_empty(), "answered");
    // no post of his after it: his Release lets it go
    let p3 = open(&mut core, "owner", &["w2"], "and the docs");
    esc_cut(&mut core, &mut rx, &p3);
    assert!(core.to_hand_over().is_empty(), "p3 waits under his hold");
    let r = core.ticket_mail_hold(&Who::Owner, false);
    assert!(r.ok, "{}", r.out);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, again, _, rest) = channel(&mut core, &mut rx).expect("p3 at the Release");
    assert_eq!((ids.clone(), again, rest), (vec![p3.clone()], vec![p3.clone()], vec![w.clone()]), "his first and alone, w3's named");
    // its turn answers it: p3 came once, and w3's comes next, alone
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    core.raise_delivery();
    end_turn(&mut core, &dir, "The docs are in N-9.");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("w3's");
    assert_eq!(ids, [w], "w3's alone; p3 not again");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A coord as [`coord`]'s, w2 idle, under both his holds: his Hold on agents' mail, and the policy's
/// `+lapse-after("gather", 15).` holding his typed posts (E1b), read from the project's policy file as coordd reads it.
fn both_holds(tag: &str) -> (PathBuf, Core, UnboundedReceiver<Job>) {
    let (dir, _, mut core, rx) = coord(tag, "idle");
    let files = project(&dir, &mut core);
    std::fs::create_dir_all(files.join("policy")).unwrap();
    std::fs::write(files.join("policy/hold.eflint"), "+lapse-after(\"gather\", 15).\n").unwrap();
    *core.lapse_times.borrow_mut() = Default::default();
    assert!(core.ticket_mail_hold(&Who::Owner, true).ok);
    (dir, core, rx)
}

/// When `gather`'s one hold on T lets his burst go: its handler's `lapses_at`.
fn held_until(core: &Core, t: u64) -> chrono::DateTime<chrono::Utc> {
    let view = core.ticket_shown(t).unwrap();
    let held: Vec<&TicketView> = view.handlers.iter().filter(|h| h.lapses_at.is_some()).collect();
    assert_eq!(held.len(), 1, "gather holds his post: {:?}", view.handlers.iter().map(|h| (&h.id, &h.lapses_at)).collect::<Vec<_>>());
    chrono::DateTime::parse_from_rfc3339(held[0].lapses_at.as_deref().unwrap()).unwrap().to_utc()
}

/// The holder's `route` steps waiting now.
fn routes(core: &Core) -> Vec<Delivery> {
    core.ticket_due(WATCHER).into_iter().filter(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "route")).collect()
}

/// Both his holds at once (proto-lead N-1446 p714; paper-research N-412.p2205): under his Hold, with the policy's
/// `+lapse-after("gather", 15).` holding his typed posts (E1b), his esc cuts the turn on his p1 to w2, and p1 waits. His
/// next post, "@w2 and rebase first" on p1's thread, is held 15 s by `gather`: p1 does not go while it is held, nor
/// one second before the 15 s have passed. When they have, p2 leaves the hold and is told to w2, whom it names, and p1
/// goes with it: one delivery holding both, in his order.
#[test]
fn under_both_his_holds_his_post_his_esc_cut_goes_only_with_his_next_post_once_told() {
    let (dir, mut core, mut rx) = both_holds("both-holds");
    let p1 = open(&mut core, "owner", &["w2"], "use main");
    esc_cut(&mut core, &mut rx, &p1);
    assert!(core.to_hand_over().is_empty(), "p1 waits under his hold");
    let t = t_of(&p1);
    core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("@w2 and rebase first".into()), names: vec!["w2".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let p2 = core.tickets.m.tickets[&t].last();
    let goes = held_until(&core, t);
    assert!(core.to_hand_over().is_empty(), "p2 held: p1 does not go before it");
    assert!(channel(&mut core, &mut rx).is_none());
    // 14 s on, still held, and p1 waits; 15 s on, p2 leaves the hold and is told to w2, whom it names (base.kind's
    // `To(p.naming)`, no holder asked): p1 goes with it, in one delivery, in his order
    core.lapse_round(goes - chrono::Duration::seconds(1));
    assert!(core.to_hand_over().is_empty(), "14 s on: p2 held, p1 waits");
    assert!(channel(&mut core, &mut rx).is_none());
    core.lapse_round(goes);
    assert!(routes(&core).is_empty(), "his own @w2: no holder asked");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, tickets, _, _, _, rest) = channel(&mut core, &mut rx).expect("p2 told: p1 with it");
    let posts: Vec<u32> = match &tickets[0].0.what {
        docket_machine::What::Posts { unread } => unread.iter().map(|p| p.n).collect(),
        _ => Vec::new(),
    };
    assert_eq!((ids, posts, rest), (vec![format!("{}.p{p2}", core.tickets.m.id(t))], vec![1, p2], Vec::<String>::new()), "one delivery, p1 then p2");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The same with his next post typed to nobody, on his open thread to w2 that names w3 (proto-lead N-1446 p734, the gap
/// named at p729): out of the hold it goes to the holder (`route`), as base.kind sends a post naming nobody when more
/// than one agent could take it. p1 does not go when the hold has passed and the holder is still to read p2; it goes
/// once the holder tells p2 to w2, with it, one delivery holding both in his order. The holder's answer is made up.
#[test]
fn under_both_his_holds_his_post_his_esc_cut_waits_for_the_holder_to_tell_his_next_post() {
    let (dir, mut core, mut rx) = both_holds("both-holds-route");
    // his message to w2 naming w3, opened `Named`: a thread of his that stays open (thread.kind's `Thread`), where a
    // post naming nobody may go; `open`'s `Sent` ends it at once. Its words in the form and as the open's text are
    // posted once (hs-ls N-1446 p73)
    let form = json!({"to": "w2", "typed": ["w2"], "names": ["w3"], "text": "use main @w3", "addressed": {"head": "Named"}, "follows": null});
    let t = t_of(&open_as(&mut core, "owner", &["w2", "w3"], "use main @w3", form));
    let n1 = core.tickets.m.tickets[&t].last();
    assert!(core.tickets.m.tickets[&t].is_open());
    let p1 = format!("{}.p{n1}", core.tickets.m.id(t));
    esc_cut(&mut core, &mut rx, &p1);
    assert!(core.to_hand_over().is_empty(), "p1 waits under his hold");
    core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("and rebase first".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let p2 = core.tickets.m.tickets[&t].last();
    let goes = held_until(&core, t);
    assert!(core.to_hand_over().is_empty(), "p2 held: p1 does not go before it");
    // out of the hold, to the holder: p2 is told to nobody yet, and p1 waits still
    core.lapse_round(goes);
    let route = routes(&core);
    assert_eq!(route.len(), 1, "the holder is asked once the hold has passed");
    assert!(core.to_hand_over().is_empty(), "out of the hold, not told: p1 waits still");
    assert!(channel(&mut core, &mut rx).is_none());
    // the holder tells it to w2: p1 goes with it, in one delivery, in his order
    core.ticket_answer(WATCHER, route[0].t, json!({"head": "To", "agents": ["w2"]}), Text::New(String::new())).unwrap();
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, tickets, _, _, _, rest) = channel(&mut core, &mut rx).expect("p2 told: p1 with it");
    let posts: Vec<u32> = match &tickets[0].0.what {
        docket_machine::What::Posts { unread } => unread.iter().map(|p| p.n).collect(),
        _ => Vec::new(),
    };
    assert_eq!((ids, posts, rest), (vec![format!("{}.p{p2}", core.tickets.m.id(t))], vec![n1, p2], Vec::<String>::new()), "one delivery, p1 then p2");
    let _ = std::fs::remove_dir_all(&dir);
}

/// BY's post on T naming NAMES, informing INFORM as policy would (notify.eflint: everyone else on the thread), typed
/// KIND: its id.
fn post_informing(core: &mut Core, t: u64, by: &str, names: &[&str], inform: &[&str], kind: Option<PostKind>, text: &str) -> String {
    let s = |xs: &[&str]| xs.iter().map(|a| a.to_string()).collect::<Vec<String>>();
    core.ticket_input(Input::Post { t, by: by.into(), text: Text::New(text.into()), names: s(names), told: vec![], inform: s(inform), answers: None, kind }).unwrap();
    format!("{}.p{}", core.tickets.m.id(t), core.tickets.m.tickets[&t].last())
}

/// The values of w2's `free` events in DIR's record, in order.
fn free_values(dir: &Path) -> Vec<Value> {
    let lines = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap_or_default();
    lines.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["ev"] == "event" && v["name"] == "free" && v["value"]["agent"] == "w2").map(|v| v["value"].clone()).collect()
}

/// Today's second case (paper-research N-1446 p2095; proto-mind p2094), on N-1446's shape: w2 is given its work while
/// idle, at once and with no call. At work, the lock talk among the others goes on the train's thread, informing it
/// only as one on the thread, and two posts for it come there, as p2067 and p2070 came to docket-core. Its turn ends: `free` says
/// what goes came while it worked, where its work was given and how far its session ran on since. One `fold`, the
/// clerk's `choose` (`since` in its words), sends the two as one message, Fork as Give; the lock talk goes by its
/// first line in the digest, none of it whole. On agent-base's own program, delivery.kind and fold.call (proto-mind's
/// head, N-1446 p2105), in place of the cut copies the engine half was tried on.
#[tokio::test]
async fn what_came_while_it_worked_goes_by_one_fold_at_its_turns_end_and_the_lock_talk_by_first_lines() {
    let (dir, _, mut core, mut rx) = coord("fold", "idle");
    core.fold_by = None;
    // the train's thread, N-1446's like, where the lock talk goes on; w2 is informed there as one on it
    let t = t_of(&open(&mut core, "lead", &["w3"], "train 70: merges and the gate"));
    // idle: its work goes at once, with no call
    let work = open(&mut core, "lead", &["w2"], "the hotfix: find where the thread goes in whole");
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert!(core.ticket_due(WATCHER).is_empty(), "no fold for what came while it was idle");
    assert_eq!(free_values(&dir).last().map(|v| v["worked"].clone()), Some(json!(false)));
    let (ids, ..) = channel(&mut core, &mut rx).expect("its work");
    assert_eq!(ids, [work.clone()]);
    core.handed("w2", &ids, true, "channel");
    // given at 4,000 bytes into its session, which runs on to 404,000 before its last message
    let file = dir.join("w2.jsonl");
    std::fs::write(&file, " ".repeat(404_000)).unwrap();
    let path = file.display().to_string();
    let _ = core.transcript_read("w2", vec![(work.clone(), path.clone(), 3_000, 1_000)], vec![], vec![], &[], None, &[], false);
    core.ticket_delivered("w2", &work, Loc { file: path, offset: 3_000, len: 1_000 }, "2026-11-23T18:12:00Z", false);
    core.begin_turn("w2");
    working(&mut core);
    // at work: the lock talk between lead and w3, w2 informed as on the thread; then two posts for w2
    let lock = post_informing(&mut core, t, "w3", &["lead"], &["w2"], None, "@lead the lock is yours\nkept-a5 ran 20 minutes, rc 0, fuser empty");
    let p2067 = post_informing(&mut core, t, "lead", &["w2"], &["w3"], None, "@w2 right: found at machine.rs:1222");
    let p2070 = post_informing(&mut core, t, "lead", &[], &["w3"], Some(PostKind::Ask { to: vec!["w2".into()] }), "core-newdeliv is urgent");
    let member = |core: &Core, id: &str| core.tickets.m.waiting("w2").into_iter().find(|i| i["id"] == json!(id)).map(|i| i["member"].clone());
    assert_eq!((member(&core, &lock), member(&core, &p2067), member(&core, &p2070)), (Some(json!(true)), Some(json!(false)), Some(json!(false))));
    core.raise_delivery();
    assert_eq!((kept(&core, &p2067).as_deref(), kept(&core, &p2070).as_deref()), (Some("comes when its turn ends"), Some("comes when its turn ends")));
    assert_eq!(kept(&core, &lock), None, "pulled: it does not arrive");
    // its turn ends: free, worked, with its work's place and how far the session ran on since
    end_turn(&mut core, &dir, "found it");
    core.raise_delivery();
    let free = free_values(&dir).last().cloned().unwrap();
    let ran = std::fs::metadata(&file).unwrap().len() - 4_000;
    let since = format!("{} KB of its session (about {}k tokens)", ran / 1024, ran / 4 / 1000);
    assert_eq!((free["worked"].clone(), free["place"].clone(), free["since"].clone()), (json!(true), json!(work), json!(since)), "{free}");
    assert_eq!(free["forkable"], json!(false), "w2's session here names no transcript: no fork at its place");
    // one fold, the clerk's job: what came, a line each, oldest first; `since` in choose's words
    let folds: Vec<Delivery> = core.ticket_due(WATCHER).into_iter().filter(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "fold")).collect();
    assert_eq!(folds.len(), 1, "{folds:?}");
    let job = core.fold_job(&folds[0]).expect("a fold job");
    let n = job.fold.clone().unwrap();
    assert_eq!((job.to.as_str(), n.ids.clone(), n.since.clone()), ("w2", vec![p2067.clone(), p2070.clone()], since.clone()));
    assert_eq!(n.waiting.lines().count(), 2, "{}", n.waiting);
    let prompt = crate::clerk::fold_prompt(&job, &n).unwrap();
    assert!(prompt.contains("The agent is w2,") && prompt.contains(&p2070) && prompt.contains(&p2067), "{prompt}");
    assert!(prompt.contains(&format!("its session has grown by {since}")), "{prompt}");
    // its answer: Fork, taken as the call gave it; w2's session here names no transcript, so `free` said it cannot
    // fork at its place, and the program delivers it as Give (gemini-advisor N-1446 p2400, A4; part 4's own legs fork:
    // `a_fork_restarts_it_on_its_session_cut_after_its_works_message_and_hands_the_message_there`)
    let head = crate::clerk::fold_ok(&json!({"head": "Fork", "now": [p2070, p2067], "rest": []}), &n).expect("Fork is an answer");
    assert_eq!(head, json!({"head": "Fork", "now": [p2070, p2067], "rest": []}));
    core.fold_answered(&job, &folds[0], head, Some("m".into()), None, Some(std::time::Duration::from_millis(2340)));
    assert_eq!(acts_of_w2(&dir).last().map(|a| a.0.clone()), Some("deliver".to_string()), "the program's deliver, not a fork coordd turns into one");
    // the journal says who decided and how long the mail waited on it (p2115)
    assert_eq!(super::fold_said(Some("m"), None, Some(std::time::Duration::from_millis(2340))), "m decided in 2.3 s");
    assert_eq!(super::fold_said(None, Some("no model answered"), Some(std::time::Duration::from_secs(31))), "the call failed (no model answered) in 31.0 s, everything waiting goes");
    assert_eq!(super::fold_said(None, None, None), "the call decided");
    assert!(core.ticket_due(WATCHER).is_empty(), "one fold");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let ((ids, tickets, replies_on, again, into, rest), digest) = channel_as_sent(&mut core, &mut rx).expect("handed");
    assert_eq!((ids.clone(), rest.clone(), digest.clone()), (vec![p2070.clone()], vec![], vec![lock.clone()]), "the two as one delivery of the thread, up to the later, the lock talk in its digest");
    let shared = crate::Shared::new(core);
    let out = crate::rendered_all(&shared, "w2", &tickets, replies_on.as_deref(), &again, into, false, &rest, &digest).await;
    assert_eq!(out.len(), 1, "{out:?}");
    let text = &out[0].0;
    assert!(text.contains("found at machine.rs:1222") && text.contains("core-newdeliv is urgent"), "{text}");
    assert!(text.contains("the lock is yours") && !text.contains("kept-a5 ran 20 minutes"), "the lock talk by its first line, none of it whole: {text}");
    // the failed call's answer: Failed, with why, on agent-base's fold.call (since 76); on a text whose fold has no
    // Failed, as before I-1480 c, everything waiting, oldest first
    assert!(n.failed, "agent-base's fold answers Failed");
    assert_eq!(crate::clerk::fold_fell(&n, "no model answered"), json!({"head": "Failed", "why": "no model answered"}));
    let before = crate::clerk::Fold { failed: false, ..n.clone() };
    assert_eq!(crate::clerk::fold_fell(&before, "no model answered"), json!({"head": "Give", "now": [p2067, p2070], "rest": []}));
    assert_eq!(crate::clerk::fold_failed(&n), json!({"head": "Give", "now": [p2067, p2070], "rest": []}));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A message holds one step by `deliver`'s rule alone (gemini-advisor N-1446 p2400, A3; settled on N-1540): two steps
/// came for w2 while it worked, and fold gives both now. The clerk keeps the answer as given, and the message holds
/// the first, the second named first at its foot, as the act does for any program (extends docket-machine's
/// `a_message_holds_one_step_and_names_the_next_at_its_foot`, tests/delivery.rs:1562, to fold's answer). A failed
/// call's answer is everything waiting, oldest first, and goes the same way.
#[test]
fn fold_gives_two_steps_and_the_message_holds_the_first_the_second_first_at_its_foot() {
    let (dir, _, mut core, mut rx) = coord("fold-steps", "working");
    core.fold_by = None;
    let q1 = question(&mut core, "which median?");
    let q2 = question(&mut core, "which mean?");
    let (s1, s2) = (format!("{}.s1", core.tickets.m.id(q1)), format!("{}.s1", core.tickets.m.id(q2)));
    core.raise_delivery();
    assert_eq!((kept(&core, &s1).as_deref(), kept(&core, &s2).as_deref()), (Some("comes when its turn ends"), Some("comes when its turn ends")));
    end_turn(&mut core, &dir, "the build is green");
    core.raise_delivery();
    let folds: Vec<Delivery> = core.ticket_due(WATCHER).into_iter().filter(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "fold")).collect();
    assert_eq!(folds.len(), 1, "{folds:?}");
    let job = core.fold_job(&folds[0]).expect("a fold job");
    let n = job.fold.clone().unwrap();
    assert_eq!(n.ids, vec![s1.clone(), s2.clone()]);
    assert_eq!(crate::clerk::fold_failed(&n), json!({"head": "Give", "now": [s1, s2], "rest": []}), "a failed call's: everything, oldest first");
    let head = crate::clerk::fold_ok(&json!({"head": "Give", "now": [s1, s2], "rest": []}), &n).expect("Give is an answer");
    assert_eq!(head, json!({"head": "Give", "now": [s1, s2], "rest": []}), "the clerk keeps both");
    core.fold_answered(&job, &folds[0], head, Some("m".into()), None, None);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, _, into, rest) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!((ids, into, rest), (vec![s1], false, vec![s2]), "one step a message, the second first at its foot");
    let _ = std::fs::remove_dir_all(&dir);
}

/// fold's call fails (gemini-advisor N-1446 p2400; settled on N-1540): agent-base's fold answers Failed, so the clerk
/// says so, with why, not a made-up Give of everything. The run's record has Failed, and delivery.kind's arm hands
/// what goes now, as one message, with no second call: the step that came while w2 worked, and the post.
#[test]
fn a_fold_whose_call_fails_answers_failed_and_what_goes_now_goes_with_no_second_call() {
    let (dir, _, mut core, mut rx) = coord("fold-failed", "working");
    core.fold_by = None;
    let q = question(&mut core, "which median?");
    let s = format!("{}.s1", core.tickets.m.id(q));
    let p = open(&mut core, "w3", &["w2"], "the gate waits on it");
    core.raise_delivery();
    assert_eq!((kept(&core, &s).as_deref(), kept(&core, &p).as_deref()), (Some("comes when its turn ends"), Some("comes when its turn ends")));
    end_turn(&mut core, &dir, "the build is green");
    core.raise_delivery();
    let folds: Vec<Delivery> = core.ticket_due(WATCHER).into_iter().filter(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "fold")).collect();
    assert_eq!(folds.len(), 1, "{folds:?}");
    let job = core.fold_job(&folds[0]).expect("a fold job");
    let n = job.fold.clone().unwrap();
    assert!(n.failed, "agent-base's fold answers Failed");
    let head = crate::clerk::fold_fell(&n, "no model answered");
    core.fold_answered(&job, &folds[0], head, None, Some("no model answered".into()), None);
    let log = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap();
    assert!(log.contains(r#""head":"Failed","why":"no model answered""#), "the run's record says the call failed");
    assert!(core.ticket_due(WATCHER).is_empty(), "no second call");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, _, into, rest) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!((ids, into, rest), (vec![s, p], false, Vec::<String>::new()), "what goes now, as one message");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A message holds what the program gave in the program's order, not by its threads' numbers (fold.call: "most
/// important first"; fresh-lead-3 N-1540 p18: the brain's line after his posts). coordd's hand sorted a message's
/// deliveries by ticket, so fold's order never reached the agent. Two posts come while w2 works, the older thread's
/// first; fold is offered them so, and gives the newer first: the message holds them as fold gave them.
#[test]
fn a_message_holds_what_fold_gave_most_important_first_not_by_its_threads_numbers() {
    let (dir, _, mut core, mut rx) = coord("fold-order", "working");
    core.fold_by = None;
    let bench = open(&mut core, "w3", &["w2"], "the bench ran: look at the median");
    let gate = open(&mut core, "lead", &["w2"], "the gate waits on it");
    assert!(t_of(&bench) < t_of(&gate));
    core.raise_delivery();
    end_turn(&mut core, &dir, "the build is green");
    core.raise_delivery();
    let folds: Vec<Delivery> = core.ticket_due(WATCHER).into_iter().filter(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "fold")).collect();
    assert_eq!(folds.len(), 1, "{folds:?}");
    let job = core.fold_job(&folds[0]).expect("a fold job");
    let n = job.fold.clone().unwrap();
    assert_eq!(n.ids, [bench.clone(), gate.clone()], "offered in the threads' order");
    let head = crate::clerk::fold_ok(&json!({"head": "Give", "now": [gate.clone(), bench.clone()], "rest": []}), &n).expect("Give is an answer");
    core.fold_answered(&job, &folds[0], head, Some("m".into()), None, None);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, tickets, ..) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!(ids, [gate.clone(), bench.clone()], "as fold gave them");
    assert_eq!(tickets.iter().map(|(d, _)| d.id.clone()).collect::<Vec<_>>(), [gate, bench], "and the message reads so");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A `free` with no turn after it leaves what came while the agent worked as having come (docket-core N-1446 p2534,
/// note 2): w3's post comes while w2 works; at its turn's end the owner is writing to w2, so `free` keeps it. When he
/// stops, `free` says it came while w2 worked, and one `fold` decides it. On 75 the writing's `free` cleared that, and
/// the post went with no call. A post that came while w2 was idle still goes with none
/// (`while_he_writes_to_it_the_rest_is_kept_and_his_writing_ends_with_his_app`).
#[test]
fn what_his_writing_kept_at_a_working_agents_turns_end_is_decided_by_one_fold_when_he_stops() {
    let (dir, _, mut core, mut rx) = coord("fold-writing", "working");
    core.fold_by = None;
    let gate = open(&mut core, "w3", &["w2"], "the gate waits on it");
    core.raise_delivery();
    assert_eq!(kept(&core, &gate).as_deref(), Some("comes when its turn ends"));
    assert!(core.owner_writing(&Who::Owner, "w2", true, std::process::id() as i32).ok);
    end_turn(&mut core, &dir, "the build is green");
    core.raise_delivery();
    assert_eq!(kept(&core, &gate).as_deref(), Some("the owner is writing to it"));
    let folds = |core: &mut Core| -> Vec<Delivery> { core.ticket_due(WATCHER).into_iter().filter(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "fold")).collect() };
    assert!(folds(&mut core).is_empty(), "no call while he writes");
    assert!(core.owner_writing(&Who::Owner, "w2", false, 0).ok);
    core.raise_delivery();
    let due = folds(&mut core);
    assert_eq!(due.len(), 1, "it came while w2 worked: one fold, {due:?}");
    let job = core.fold_job(&due[0]).expect("a fold job");
    assert_eq!(job.fold.clone().unwrap().ids, [gate.clone()]);
    let head = crate::clerk::fold_ok(&json!({"head": "Give", "now": [gate.clone()], "rest": []}), job.fold.as_ref().unwrap()).unwrap();
    core.fold_answered(&job, &due[0], head, None, None, None);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("handed");
    assert_eq!(ids, [gate]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Fold's race (gemini-advisor N-1446 p2365, 5b; proto-mind p2370 (a)): w2's turn ends with a post for it waiting,
/// `free` runs fold, and before fold answers a turn of w2's begins (its own job's end begins one, as docket-edge6's did
/// at 00:28:47Z on 24 Nov, and N-1446.p2262 was typed into it). Fold's Give was made at its `free` (`Hand::idle`), so
/// nothing goes into the running turn: it waits, no `free` runs again, and at that turn's end it goes as one message,
/// out of any turn, with no second fold.
#[tokio::test]
async fn a_message_fold_gave_after_a_turn_began_goes_at_that_turns_end_not_into_it() {
    let (dir, _, mut core, mut rx) = coord("fold-race", "idle");
    core.fold_by = None;
    // idle: its work goes at once, with no call
    let work = open(&mut core, "lead", &["w2"], "the hotfix: find where the thread goes in whole");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("its work");
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    working(&mut core);
    // at work: a post for it, kept for its turn's end
    let p2 = post_informing(&mut core, t_of(&work), "lead", &["w2"], &[], None, "@w2 the gate waits on it");
    core.raise_delivery();
    assert_eq!(kept(&core, &p2).as_deref(), Some("comes when its turn ends"));
    // its turn ends: `free`, worked, so one fold
    end_turn(&mut core, &dir, "found it");
    core.raise_delivery();
    let folds: Vec<Delivery> = core.ticket_due(WATCHER).into_iter().filter(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "fold")).collect();
    assert_eq!(folds.len(), 1, "{folds:?}");
    let job = core.fold_job(&folds[0]).expect("a fold job");
    let n = job.fold.clone().unwrap();
    // fold's call out, a turn of w2's begins
    core.begin_turn("w2");
    working(&mut core);
    let head = crate::clerk::fold_ok(&json!({"head": "Give", "now": [p2.clone()], "rest": []}), &n).expect("Give is an answer");
    core.fold_answered(&job, &folds[0], head, Some("m".into()), None, Some(std::time::Duration::from_millis(3000)));
    assert!(core.tickets.m.hands("w2").iter().any(|h| h.idle && h.items.contains(&p2)), "made at its free: {:?}", core.tickets.m.hands("w2"));
    assert!(core.to_hand_over().is_empty(), "nothing offered while the turn begun meanwhile runs");
    assert!(channel(&mut core, &mut rx).is_none(), "nothing into the running turn");
    assert!(given_lines(&dir).iter().all(|g| g["delivery"] != json!(p2)), "not given");
    // that turn ends: no `free` again, no second fold, and the message goes out of any turn
    end_turn(&mut core, &dir, "the build is green");
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert!(core.ticket_due(WATCHER).is_empty(), "no second fold");
    let (ids, _, _, _, into, _) = channel(&mut core, &mut rx).expect("handed at its end");
    assert_eq!((ids, into), (vec![p2.clone()], false), "one message, out of any turn");
    let _ = std::fs::remove_dir_all(&dir);
}

/// w2's session for a fork's legs: its line N's place and the lines, written to DIR/w2.jsonl. Its work's message WORK
/// is queued (line 1), then taken into its chain two lines down (line 3), as Claude Code writes a channel message;
/// then its work.
fn fork_session(dir: &Path, work: &str) -> (String, Vec<String>) {
    let tag = format!("<channel source=\"coord\" from=\"lead\" id=\"{work}\" kind=\"thread\">\nthe hotfix\n</channel>");
    let lines: Vec<String> = [
        json!({"type": "user", "message": {"role": "user", "content": "before"}, "uuid": "u0", "sessionId": "s-w2"}),
        json!({"type": "queue-operation", "operation": "enqueue", "content": tag, "sessionId": "s-w2"}),
        json!({"type": "queue-operation", "operation": "dequeue", "sessionId": "s-w2"}),
        json!({"type": "user", "isMeta": true, "message": {"role": "user", "content": tag}, "uuid": "u1", "parentUuid": "u0", "sessionId": "s-w2"}),
        json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": "looking"}]}, "uuid": "u2", "parentUuid": "u1", "sessionId": "s-w2"}),
    ]
    .iter()
    .map(|l| format!("{l}\n"))
    .collect();
    let file = dir.join("w2.jsonl");
    std::fs::write(&file, lines.concat()).unwrap();
    (file.display().to_string(), lines)
}

/// The bytes of LINES before line N.
fn before_line(lines: &[String], n: usize) -> u64 {
    lines[..n].iter().map(|l| l.len() as u64).sum()
}

/// Up to the program's `fork` (agent-base's delivery.kind, fold's Fork): w2 is given its work while idle, its session
/// ([`fork_session`]) records where, an ask for it comes while it works, its turn ends with TEXT, and `fold` answers
/// Fork with the ask, BEFORE run just before that answer. The coord, the core, its jobs, the work, the ask, w2's
/// transcript and its lines.
fn forked_at_turn_end(tag: &str, text: &str, before: impl FnOnce(&mut Core, &Path)) -> (PathBuf, Core, UnboundedReceiver<Job>, String, String, String, Vec<String>) {
    let (dir, _, mut core, mut rx) = coord(tag, "idle");
    core.fold_by = None;
    let work = open(&mut core, "lead", &["w2"], "the hotfix: find where the thread goes in whole");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("its work");
    core.handed("w2", &ids, true, "channel");
    let (path, lines) = fork_session(&dir, &work);
    core.merge_session("w2", &json!({"session_id": "s-w2", "transcript_path": path}));
    // given where it was queued, as the follower finds it first
    let given = Loc { file: path.clone(), offset: before_line(&lines, 1), len: lines[1].len() as u64 };
    let _ = core.transcript_read("w2", vec![(work.clone(), path.clone(), given.offset, given.len)], vec![], vec![], &[], None, &[], false);
    core.ticket_delivered("w2", &work, given, "2026-11-23T18:12:00Z", false);
    core.begin_turn("w2");
    working(&mut core);
    let ask = post_informing(&mut core, t_of(&work), "lead", &[], &[], Some(PostKind::Ask { to: vec!["w2".into()] }), "the plan changed: the digest goes first");
    core.raise_delivery();
    assert_eq!(kept(&core, &ask).as_deref(), Some("comes when its turn ends"));
    end_turn(&mut core, &dir, text);
    core.raise_delivery();
    let folds: Vec<Delivery> = core.ticket_due(WATCHER).into_iter().filter(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "fold")).collect();
    assert_eq!(folds.len(), 1, "{folds:?}");
    let job = core.fold_job(&folds[0]).expect("a fold job");
    let n = job.fold.clone().unwrap();
    let head = crate::clerk::fold_ok(&json!({"head": "Fork", "now": [ask], "rest": []}), &n).expect("Fork is an answer");
    before(&mut core, &dir);
    core.fold_answered(&job, &folds[0], head, Some("m".into()), None, None);
    let lines_now = std::fs::read_to_string(&path).unwrap().split_inclusive('\n').map(str::to_string).collect();
    (dir, core, rx, work, ask, path, lines_now)
}

/// The acts in DIR's record for w2, as (act, at, items).
fn acts_of_w2(dir: &Path) -> Vec<(String, Option<String>, Vec<String>)> {
    let log = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap_or_default();
    log.lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v["ev"] == "event" && v["name"] == docket_machine::syntax::ACTED && v["value"]["agent"] == "w2")
        .map(|v| (v["value"]["act"].as_str().unwrap_or_default().to_string(), v["value"]["at"].as_str().map(str::to_string), serde_json::from_value(v["value"]["items"].clone()).unwrap_or_default()))
        .collect()
}

/// w2's hand-over now, if it sends a Channel job: its ids, and the words of the events it carries ahead of the message
/// (coord's own, as a rate limit's resume).
fn channel_words(core: &mut Core, rx: &mut UnboundedReceiver<Job>) -> Option<(Vec<String>, Vec<String>)> {
    core.hand_over("w2");
    while let Ok(j) = rx.try_recv() {
        if let Job::Channel { ids, line, .. } = j {
            let v: Value = serde_json::from_str(&line).unwrap();
            let words = v["events"].as_array().unwrap().iter().map(|e| e["content"].as_str().unwrap_or_default().to_string()).collect();
            return Some((ids, words));
        }
    }
    None
}

/// Part 4, `fork A at PLACE with ITEMS` (docket-core N-1446 p2096; paper-research p2092, p2097), on agent-base's own
/// program: fold answers Fork, and the program forks w2 at its work's place. Nothing is handed at once: w2 is to be
/// restarted under its own name on a copy of its session cut after the entry that took its work's message (not the
/// queue line it was given on, whose copy would hold no message of it), its state saying so. Its new session takes the
/// message, coord's words ahead of it: where it goes on from, and what its work ended with. Decided once: the message
/// handed again does not fork again.
#[test]
fn a_fork_restarts_it_on_its_session_cut_after_its_works_message_and_hands_the_message_there() {
    let (dir, mut core, mut rx, work, ask, path, lines) = forked_at_turn_end("forkcut", "found it at machine.rs:1222", |_, _| {});
    assert_eq!(free_values(&dir).last().map(|v| v["forkable"].clone()), Some(json!(true)), "a Claude Code agent, its work given in the session it runs");
    assert_eq!(acts_of_w2(&dir).last(), Some(&("fork".to_string(), Some(work.clone()), vec![ask.clone()])), "the program's act");
    assert!(channel(&mut core, &mut rx).is_none(), "nothing handed before the restart");
    let cut = crate::core::ForkCut { place: work.clone(), file: path.clone(), upto: before_line(&lines, 4) };
    assert_eq!(core.agents["w2"].live.fork_due.as_ref(), Some(&cut), "cut after line 3, the entry that took the message");
    assert!(lines[3].contains(&format!("id=\\\"{work}\\\"")) && lines[3].contains("\"type\":\"user\""), "{}", lines[3]);
    assert_eq!(core.agent_state("w2"), Some(crate::core::AgentState::Starting { why: format!("going on from a copy of its session cut at {work}") }));
    let pid = core.agents["w2"].live.pid;
    assert_eq!(core.to_restart(), vec![("w2".to_string(), pid)]);
    assert!(channel(&mut core, &mut rx).is_none(), "nor while it restarts");
    // roster's restart --cut: the copy, a new process on it, its start hook naming it
    let copy = dir.join("s-new.jsonl");
    std::fs::write(&copy, lines[..4].concat().replace("\"sessionId\":\"s-w2\"", "\"sessionId\":\"s-new\"")).unwrap();
    let a = core.agents.get_mut("w2").unwrap();
    a.live = Live { pid: Some(pid.unwrap() + 1), state: "idle".into(), turn: a.ended_turn, ..Default::default() };
    core.restarted("w2", pid, "");
    core.merge_session("w2", &json!({"session_id": "s-new", "transcript_path": copy.display().to_string()}));
    let (ids, words) = channel_words(&mut core, &mut rx).expect("handed to the copy");
    assert_eq!(ids, [ask.clone()]);
    // coord's words go as an event ahead of the message, which the sender renders from its tickets
    assert_eq!(words.len(), 1, "{words:?}");
    assert!(words[0].contains(&format!("cut where {work} was given to you")) && words[0].contains("your session w2 is kept") && words[0].contains("found it at machine.rs:1222"), "{}", words[0]);
    // its session's answer lost: handed again as decided, not forked again
    core.handed("w2", &ids, false, "channel");
    core.retry_at.clear();
    let (again, words) = channel_words(&mut core, &mut rx).expect("handed again");
    assert_eq!((again, words.len()), (vec![ask.clone()], 1));
    assert_eq!(core.agents["w2"].live.fork_due, None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A fork it cannot make (part 4): the message goes as a `deliver`'s, to the session it runs, with no words of coord's
/// (coordd's log says why), and is not forked again when handed again. Its restart on the cut copy failed; its session
/// is no longer the one its work was given in; it is a Codex agent, which is not forked yet.
#[test]
fn a_fork_it_cannot_make_is_handed_as_a_deliver() {
    // the restart failed: its session goes on as it was
    let (dir, mut core, mut rx, work, ask, ..) = forked_at_turn_end("forkfail", "found it", |_, _| {});
    let pid = core.agents["w2"].live.pid;
    assert!(channel(&mut core, &mut rx).is_none(), "nothing handed before the restart");
    assert!(core.agents["w2"].live.fork_due.is_some());
    assert_eq!(core.to_restart(), vec![("w2".to_string(), pid)]);
    core.restarted("w2", pid, "roster restart failed: a boxed agent is not restarted on a cut copy of its session yet");
    assert_eq!(core.agents["w2"].live.fork_due, None);
    let alone = Some((vec![ask.clone()], vec![]));
    assert_eq!(channel_words(&mut core, &mut rx).map(|(ids, w)| (ids, w.into_iter().filter(|w| w.contains("copy of your session")).collect::<Vec<_>>())), alone, "the message, no words of a fork");
    assert_eq!(acts_of_w2(&dir).last().map(|a| (a.0.clone(), a.1.clone())), Some(("fork".to_string(), Some(work))), "the record keeps the program's act");
    let _ = std::fs::remove_dir_all(&dir);
    // its session is another now
    let (dir, mut core, mut rx, _, ask, ..) = forked_at_turn_end("forkother", "found it", |core, dir| {
        let other = dir.join("other.jsonl");
        std::fs::write(&other, "").unwrap();
        core.merge_session("w2", &json!({"session_id": "other", "transcript_path": other.display().to_string()}));
    });
    let got = channel_words(&mut core, &mut rx).map(|(ids, w)| (ids, w.iter().any(|w| w.contains("copy of your session"))));
    assert_eq!(got, Some((vec![ask.clone()], false)));
    assert_eq!(core.agents["w2"].live.fork_due, None);
    assert!(core.to_restart().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
    // a Codex agent
    let (dir, mut core, mut rx, _, ask, ..) = forked_at_turn_end("forkcodex", "found it", |core, _| core.agents.get_mut("w2").unwrap().kind = "codex".into());
    core.hand_over("w2");
    assert_eq!(core.agents["w2"].live.fork_due, None);
    let mut codex = None;
    while let Ok(j) = rx.try_recv() {
        if let Job::Codex { ids, text, .. } = j {
            codex = Some((ids, text.contains("copy of your session")));
        }
    }
    assert_eq!(codex, Some((vec![ask.clone()], false)), "delivered as a deliver's");
    assert_eq!(core.forked.get("w2").map(|f| f.words.clone()), Some(None), "decided: not forked");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A session of two turns for the owner's edits of a system prompt: a prompt, its answer and its end (Claude Code's
/// `turn_duration`), then a second prompt and its answer, still running. The file and its lines.
fn two_turns(dir: &Path) -> (String, Vec<String>) {
    let lines: Vec<String> = [
        json!({"type": "user", "message": {"role": "user", "content": "first"}, "uuid": "u0", "sessionId": "s-w2", "timestamp": "2026-11-24T09:00:00Z"}),
        json!({"type": "assistant", "message": {"role": "assistant", "model": "m", "content": [{"type": "text", "text": "one"}]}, "uuid": "u1", "parentUuid": "u0", "sessionId": "s-w2", "timestamp": "2026-11-24T09:00:02Z"}),
        json!({"type": "system", "subtype": "turn_duration", "durationMs": 2000, "uuid": "u2", "parentUuid": "u1", "sessionId": "s-w2", "timestamp": "2026-11-24T09:00:02Z"}),
        json!({"type": "user", "message": {"role": "user", "content": "second"}, "uuid": "u3", "parentUuid": "u2", "sessionId": "s-w2", "timestamp": "2026-11-24T09:01:00Z"}),
        json!({"type": "assistant", "message": {"role": "assistant", "model": "m", "content": [{"type": "text", "text": "two"}]}, "uuid": "u4", "parentUuid": "u3", "sessionId": "s-w2", "timestamp": "2026-11-24T09:01:02Z"}),
    ]
    .iter()
    .map(|l| format!("{l}\n"))
    .collect();
    let file = dir.join("w2.jsonl");
    std::fs::write(&file, lines.concat()).unwrap();
    (file.display().to_string(), lines)
}

/// The Map's Rewind / Branch from here (`Request::ForkAgent`'s `at`): the copy keeps the session up to the end of the
/// last turn that ended by the time pressed; before any ended, none; a time inside the second turn still cuts after
/// the first, a session between turns.
#[test]
fn a_rewind_cuts_at_the_last_turn_that_ended_by_its_time() {
    let dir = std::env::temp_dir().join(format!("coordd-rewind-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (file, lines) = two_turns(&dir);
    let secs = |t: &str| chrono::DateTime::parse_from_rfc3339(t).unwrap().timestamp();
    let first = lines[..3].concat().len() as u64;
    assert_eq!(crate::follow::turn_end_by(&file, secs("2026-11-24T08:59:00Z")), None, "no turn had ended");
    assert_eq!(crate::follow::turn_end_by(&file, secs("2026-11-24T09:00:02Z")), Some(first), "at its end");
    assert_eq!(crate::follow::turn_end_by(&file, secs("2026-11-24T09:01:30Z")), Some(first), "inside the second turn");
    assert!(crate::follow::turn_end(&file, first), "a session between turns");
    let _ = std::fs::remove_dir_all(&dir);
}

/// `Request::SystemPrompt` as coordd answers it (main.rs `handle`): its refusals, the turn's end read in the transcript
/// (off the lock there, `core::system_cut`), then applied with the refusals checked again.
fn edit(core: &mut Core, who: &Who, name: &str, text: &str, replace: bool, after: Option<u64>) -> coord_types::Reply {
    match core.system_prompt_from(who, name, text, replace, after).and_then(|file| crate::core::system_cut(file, after)) {
        Ok(cut) => core.system_prompt(who, name, text, replace, cut),
        Err(r) => r,
    }
}

/// N-1788: what refuses a system prompt is the harness's capabilities, not its kind's name nor a boxed project, and
/// there is no bound on its length (roster gives it as a file): a 300 KB text is taken, written whole; a boxed
/// project's Claude Code agent takes one; each harness's view says what its measured table (`phase1-harness-adapters`)
/// lets it take, Codex, opencode and agy too; a program agent, which has no harness, is refused by its capabilities.
#[test]
fn a_system_prompt_is_refused_by_capabilities_only_and_has_no_length_bound() {
    let (dir, _, mut core, _rx) = coord("syscaps", "idle");
    let caps = |s: [bool; 6]| coord_types::HarnessCapabilities {
        system_prompt_file: s[0], system_prompt_replace: s[1], tool_disabling: s[2], safety_toggle: s[3], prompt_parts_off: s[4], turn_branching: s[5],
    };
    for (kind, want) in [
        ("claude", caps([true; 6])),
        ("codex", caps([true, true, true, false, false, false])),
        ("opencode", caps([true, true, true, false, false, false])),
        ("agy", caps([true, false, false, false, false, false])),
        ("program", coord_types::HarnessCapabilities::default()),
    ] {
        core.agents.get_mut("w3").unwrap().kind = kind.into();
        assert_eq!(core.view("w3").capabilities, want, "{kind}");
    }
    let r = edit(&mut core, &Who::Owner, "w3", "You review.", false, None);
    assert!(!r.ok && r.out.contains("system_prompt_file false"), "{}", r.out);
    core.agents.get_mut("w3").unwrap().kind = "codex".into();
    let long = "a".repeat(300_000);
    let r = edit(&mut core, &Who::Owner, "w2", &long, true, None);
    assert!(r.ok, "{}", r.out);
    assert_eq!(std::fs::read_to_string(dir.join("prompts/w2.system/1.md")).unwrap().len(), 300_001);
    // a failed apply is the agent's, in its view, until one applies
    core.system_applied("w2", 1, "roster restart failed: w2: no session_id");
    assert_eq!(core.view("w2").apply_error.as_deref(), Some("system prompt 1 not applied: roster restart failed: w2: no session_id"));
    core.system_applied("w2", 2, "");
    assert_eq!(core.view("w2").apply_error, None);
}

/// Switches go to the agent's harness.toml under its harness; refused ones write nothing; the view merges the levels.
#[test]
fn the_owners_harness_switches_are_written_where_roster_reads_them_for_each_harness() {
    let (dir, _, mut core, _rx) = coord("switches", "idle");
    let file = |n: &str| dir.join("agents").join(n).join("harness.toml");
    let read = |n: &str| std::fs::read_to_string(file(n)).unwrap().parse::<toml::Table>().unwrap();
    let strs = |v: &[&str]| Some(v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    // Claude Code takes all three
    let r = core.harness_switches(&Who::Owner, "w2", Some(false), strs(&["WebFetch", "WebSearch"]), strs(&["You are Claude Code"]));
    assert!(r.ok, "{}", r.out);
    let t = read("w2");
    assert_eq!(t["claude"]["safety_check"].as_bool(), Some(false));
    assert_eq!(t["claude"]["tools_off"].as_array().unwrap().len(), 2);
    assert_eq!(t["claude"]["parts_off"][0].as_str(), Some("You are Claude Code"));
    let r = core.harness_switches(&Who::Owner, "w2", None, strs(&[]), None);
    assert!(r.ok, "{}", r.out);
    let t = read("w2");
    assert_eq!(t["claude"]["tools_off"].as_array().unwrap().len(), 0, "replaced");
    assert_eq!(t["claude"]["safety_check"].as_bool(), Some(false), "kept");
    // Codex and opencode take tools_off under their own tables, the file's others kept; agy takes none of the three
    std::fs::create_dir_all(dir.join("agents/w3")).unwrap();
    std::fs::write(file("w3"), "[agy]\ntools_off = []\n").unwrap();
    for kind in ["codex", "opencode"] {
        core.agents.get_mut("w3").unwrap().kind = kind.into();
        let r = core.harness_switches(&Who::Owner, "w3", None, strs(&["web_search"]), None);
        assert!(r.ok, "{kind}: {}", r.out);
        assert_eq!(read("w3")[kind]["tools_off"][0].as_str(), Some("web_search"), "{kind}");
        for (check, parts, why) in [(Some(false), None, "safety_toggle false"), (None, strs(&["x"]), "prompt_parts_off false")] {
            let r = core.harness_switches(&Who::Owner, "w3", check, None, parts);
            assert!(!r.ok && r.out.contains(why), "{kind}: {}", r.out);
        }
    }
    assert!(read("w3").contains_key("agy"), "another table kept");
    core.agents.get_mut("w3").unwrap().kind = "agy".into();
    let before = std::fs::read_to_string(file("w3")).unwrap();
    let r = core.harness_switches(&Who::Owner, "w3", None, strs(&["x"]), None);
    assert!(!r.ok && r.out.contains("tool_disabling false"), "{}", r.out);
    // refused, nothing written
    for (who, name, why) in [(Who::Agent("lead".into()), "w2", "the owner's"), (Who::Owner, "zz", "no agent zz")] {
        let r = core.harness_switches(&who, name, None, strs(&["x"]), None);
        assert!(!r.ok && r.out.contains(why), "{}", r.out);
    }
    let r = core.harness_switches(&Who::Owner, "w2", None, None, None);
    assert!(!r.ok && r.out.contains("no switch given"), "{}", r.out);
    // the view says what its next start takes and from where: flavor, then project, then the agent's own, key by key
    let sw = core.view("w2").switches;
    assert_eq!((sw.safety_check, sw.tools_off.len(), sw.from["safety_check"].as_str()), (false, 0, "agent"));
    let project = dir.join("flavor/projects/p");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("project.toml"), "[project]\nname = \"p\"\n").unwrap();
    std::fs::write(dir.join("project"), project.join("project.toml").display().to_string()).unwrap();
    core.project = crate::project::Project::load(&dir.display().to_string()).unwrap();
    std::fs::write(dir.join("flavor/harness.toml"), "[claude]\ntools_off = [\"Bash\"]\nparts_off = [\"F\"]\n").unwrap();
    std::fs::write(project.join("harness.toml"), "[claude]\ntools_off = [\"WebSearch\"]\n").unwrap();
    let sw = core.view("lead").switches;
    assert_eq!((sw.tools_off.clone(), sw.from["tools_off"].as_str()), (vec!["WebSearch".to_string()], "project"), "the project's over the flavor's");
    assert_eq!((sw.parts_off.clone(), sw.from["parts_off"].as_str(), sw.safety_check), (vec!["F".to_string()], "flavor", true));
    assert!(!sw.from.contains_key("safety_check"), "its default");
    let sw = core.view("w2").switches;
    assert_eq!((sw.tools_off.len(), sw.from["tools_off"].as_str(), sw.parts_off.clone()), (0, "agent", vec!["You are Claude Code".to_string()]), "the agent's own wins");
    // an edit by hand shows at the next view (the file's time and length changed)
    std::fs::write(project.join("harness.toml"), "[claude]\ntools_off = [\"WebSearch\", \"WebFetch\"]\n").unwrap();
    assert_eq!(core.view("lead").switches.tools_off.len(), 2);
    assert_eq!(std::fs::read_to_string(file("w3")).unwrap(), before);
}

/// The owner's edit of an agent's system prompt as a branch in its history (`Request::SystemPrompt`; the owner N-1104
/// p996, accounts-lead N-1446 p2447, go paper-research p2456). Each version is its own file, numbered on, never
/// overwritten; w2 restarts with it once not in a turn, its state saying so, roster asked `--system N append|replace`;
/// with a turn he picked, on a copy of its session cut at that turn's end (`--cut`), a byte that ends no turn refused.
/// A restart that did not happen takes the edit away, its file kept; refused, nothing is written: among the refusals an
/// edit while his rename of it runs, and a cut of a session it no longer runs (the lock let go while the turn's end was
/// read; docket-core N-1446 p2471). A text past Linux's 128 KiB bound on one argument is taken, as roster gives it as a
/// file; so is a Codex, opencode or agy agent's, agy's only appended.
#[test]
fn the_owners_edit_of_a_system_prompt_restarts_it_with_that_version_as_a_branch() {
    let (dir, _, mut core, _rx) = coord("sysprompt", "idle");
    let (path, lines) = two_turns(&dir);
    core.merge_session("w2", &json!({"session_id": "s-w2", "transcript_path": path}));
    let prompts = dir.join("prompts/w2.system");
    let end1 = before_line(&lines, 3);
    // refused, nothing written
    core.agents.get_mut("w3").unwrap().kind = "codex".into();
    for (who, name, text, after, why) in [
        (Who::Agent("lead".into()), "w2", "You review.", None, "the owner's"),
        (Who::Owner, "zz", "You review.", None, "no agent zz"),
        (Who::Owner, "w3", "You review.", Some(end1), "turn_branching false"),
        (Who::Owner, "w2", "  \n", None, "no text"),
        (Who::Owner, "w2", "You review.", Some(before_line(&lines, 2)), "not the end of a turn"),
        (Who::Owner, "w2", "You review.", Some(end1 - 1), "not the end of a turn"),
        (Who::Owner, "w2", "You review.", Some(0), "not the end of a turn"),
        (Who::Owner, "w2", "You review.", Some(before_line(&lines, 5)), "not the end of a turn"),
        (Who::Owner, "w2", "You review.", Some(before_line(&lines, 5) + 10), "not the end of a turn"),
    ] {
        let r = edit(&mut core, &who, name, text, false, after);
        assert!(!r.ok && r.out.contains(why), "{name} {after:?}: {}", r.out);
    }
    // a text past Linux's 128 KiB bound on one argument is taken: roster gives it to Claude Code as a file
    let long = "a".repeat(200_000);
    assert!(matches!(core.system_prompt_from(&Who::Owner, "w2", &long, true, None), Ok(None)), "a 200 KB text is taken");
    // Codex, opencode and agy are given one too (roster's app_server, opencode_agent, agy_agent); agy only appended
    assert!(matches!(core.system_prompt_from(&Who::Owner, "w3", "You review.", true, None), Ok(None)), "a Codex agent's is taken");
    core.agents.get_mut("w3").unwrap().kind = "agy".into();
    let r = core.system_prompt_from(&Who::Owner, "w3", "You review.", true, None).unwrap_err();
    assert!(r.out.contains("system_prompt_replace false"), "{}", r.out);
    assert!(matches!(core.system_prompt_from(&Who::Owner, "w3", "You review.", false, None), Ok(None)), "agy's appended one is taken");
    core.agents.get_mut("w3").unwrap().kind = "program".into();
    let r = core.system_prompt_from(&Who::Owner, "w3", "You review.", false, None).unwrap_err();
    assert!(r.out.contains("system_prompt_file false"), "{}", r.out);
    core.agents.get_mut("w3").unwrap().kind = "codex".into();
    core.renaming.insert("w2".into());
    let r = edit(&mut core, &Who::Owner, "w2", "You review.", false, None);
    assert!(!r.ok && r.out.contains("being renamed"), "{}", r.out);
    core.renaming.clear();
    let elsewhere = crate::core::ForkCut { place: format!("byte {end1}"), file: format!("{}/other.jsonl", dir.display()), upto: end1 };
    let r = core.system_prompt(&Who::Owner, "w2", "You review.", false, Some(elsewhere));
    assert!(!r.ok && r.out.contains("runs another session"), "{}", r.out);
    assert!(!prompts.exists(), "nothing written");
    assert_eq!(core.agents["w2"].live.system_due, None);
    // appended, on its conversation as it stands
    let r = edit(&mut core, &Who::Owner, "w2", "You review.\n\n", false, None);
    assert!(r.ok, "{}", r.out);
    assert_eq!(std::fs::read_to_string(prompts.join("1.md")).unwrap(), "You review.\n");
    let due = crate::core::SystemDue { version: 1, replace: false, cut: None };
    assert_eq!(core.agents["w2"].live.system_due.as_ref(), Some(&due));
    assert_eq!(core.agent_state("w2"), Some(crate::core::AgentState::Starting { why: "restarting with system prompt 1".into() }));
    assert_eq!(crate::restart_args("w2", None, Some(&due)), ["restart", "w2", "--system", "1", "append"]);
    let again = edit(&mut core, &Who::Owner, "w2", "Another.", false, None);
    assert!(!again.ok && again.out.contains("about to restart already"), "{}", again.out);
    assert!(!prompts.join("2.md").exists());
    let pid = core.agents["w2"].live.pid;
    assert_eq!(core.to_restart(), vec![("w2".to_string(), pid)]);
    // roster restarted it: a new process, without the edit due
    let a = core.agents.get_mut("w2").unwrap();
    a.live = Live { pid: Some(pid.unwrap() + 1), state: "idle".into(), turn: a.ended_turn, ..Default::default() };
    core.restarted("w2", pid, "");
    assert_eq!(core.agents["w2"].live.system_due, None);
    // in place of Claude Code's own, from the end of its first turn
    let r = edit(&mut core, &Who::Owner, "w2", "You only review.", true, Some(end1));
    assert!(r.ok, "{}", r.out);
    assert_eq!(std::fs::read_to_string(prompts.join("2.md")).unwrap(), "You only review.\n");
    assert_eq!(std::fs::read_to_string(prompts.join("1.md")).unwrap(), "You review.\n", "the version before kept");
    let cut = crate::core::ForkCut { place: format!("byte {end1}"), file: path.clone(), upto: end1 };
    let due = crate::core::SystemDue { version: 2, replace: true, cut: Some(cut) };
    assert_eq!(core.agents["w2"].live.system_due.as_ref(), Some(&due));
    assert_eq!(crate::restart_args("w2", None, Some(&due)), ["restart", "w2", "--cut", path.as_str(), &end1.to_string(), "--system", "2", "replace"]);
    // the restart failed: it runs with the prompt it had, the version kept as written
    let pid = core.agents["w2"].live.pid;
    assert_eq!(core.to_restart(), vec![("w2".to_string(), pid)]);
    core.restarted("w2", pid, "roster restart failed: a boxed agent is not given a system prompt of its own yet");
    assert_eq!(core.agents["w2"].live.system_due, None);
    assert!(prompts.join("2.md").exists());
    assert!(core.to_restart().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A fork (part 4) whose message comes while the owner's edit of w2's system prompt waits: nothing is handed before
/// that restart, so no fork is decided; then the fork is made on the session w2 runs with his version, roster asked
/// for its cut alone (roster's `current` keeps his version over later restarts, roster-cut-test 4). One cut per restart.
#[test]
fn a_fork_while_his_system_prompt_edit_waits_is_made_after_that_restart() {
    let due = crate::core::SystemDue { version: 1, replace: false, cut: None };
    let (dir, mut core, mut rx, work, _, path, lines) = forked_at_turn_end("forksys", "found it", |core, _| core.agents.get_mut("w2").unwrap().live.system_due = Some(due.clone()));
    assert_eq!((core.agents["w2"].live.fork_due.as_ref(), core.forked.get("w2")), (None, None), "no fork decided yet");
    assert!(channel(&mut core, &mut rx).is_none(), "nothing handed before his edit's restart");
    let pid = core.agents["w2"].live.pid;
    assert_eq!(core.to_restart(), vec![("w2".to_string(), pid)]);
    assert_eq!(crate::restart_args("w2", core.agents["w2"].live.fork_due.as_ref(), core.agents["w2"].live.system_due.as_ref()), ["restart", "w2", "--system", "1", "append"]);
    // roster resumed its session with his version: a new process on the same session
    let a = core.agents.get_mut("w2").unwrap();
    a.live = Live { pid: Some(pid.unwrap() + 1), state: "idle".into(), turn: a.ended_turn, ..Default::default() };
    core.restarted("w2", pid, "");
    assert!(channel(&mut core, &mut rx).is_none(), "the fork restarts it first");
    let cut = crate::core::ForkCut { place: work.clone(), file: path.clone(), upto: before_line(&lines, 4) };
    assert_eq!(core.agents["w2"].live.fork_due.as_ref(), Some(&cut));
    assert_eq!(crate::restart_args("w2", Some(&cut), None), ["restart", "w2", "--cut", path.as_str(), &cut.upto.to_string()]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// What NAME's hand-over sends now, if it sends a Channel job: its ids. What else the jobs carry until then (a copy's
/// start, an end) is given back, in order.
fn channel_to(core: &mut Core, rx: &mut UnboundedReceiver<Job>, name: &str) -> (Option<Vec<String>>, Vec<Job>) {
    core.hand_over(name);
    let mut other = Vec::new();
    while let Ok(j) = rx.try_recv() {
        match j {
            Job::Channel { ids, .. } => return (Some(ids), other),
            j => other.push(j),
        }
    }
    (None, other)
}

/// The jobs sent so far: the copies started, as (agent, call, from, cut), and the agents ended.
fn starts_and_ends(jobs: impl IntoIterator<Item = Job>) -> (Vec<(String, String, Option<String>, Option<String>)>, Vec<String>) {
    let (mut starts, mut ends) = (Vec::new(), Vec::new());
    for j in jobs {
        match j {
            Job::Start { agent, call, from, cut, .. } => starts.push((agent, call, from, cut)),
            Job::End { agent, .. } => ends.push(agent),
            _ => {}
        }
    }
    (starts, ends)
}

/// COPY's turn on what it is handed: given in its own transcript in DIR, it answers with TEXT, and is idle again.
/// What it was handed.
fn copy_turn(core: &mut Core, rx: &mut UnboundedReceiver<Job>, dir: &Path, copy: &str, text: &str) -> Vec<String> {
    let (ids, _) = channel_to(core, rx, copy);
    let ids = ids.expect("handed to the copy");
    core.handed(copy, &ids, true, "channel");
    let file = dir.join(format!("{copy}.jsonl"));
    let mut all = std::fs::read_to_string(&file).unwrap_or_default();
    for id in &ids {
        let line = format!("{}\n", json!({"type": "user", "message": {"role": "user", "content": format!("<channel source=\"coord\" from=\"owner\" id=\"{id}\">…</channel>")}}));
        let given = Loc { file: file.display().to_string(), offset: all.len() as u64, len: line.len() as u64 };
        all.push_str(&line);
        std::fs::write(&file, &all).unwrap();
        core.ticket_delivered(copy, id, given, "2026-11-24T21:40:00Z", false);
    }
    core.begin_turn(copy);
    core.agents.get_mut(copy).unwrap().live.state = "working".into();
    core.raise_delivery();
    let said = format!("{}\n", json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": text}], "stop_reason": "end_turn"}}));
    let offset = all.len() as u64;
    all.push_str(&said);
    std::fs::write(&file, &all).unwrap();
    core.ticket_turn_ended(copy, Loc { file: file.display().to_string(), offset, len: said.len() as u64 });
    let a = core.agents.get_mut(copy).unwrap();
    a.ended_turn = a.live.turn;
    a.live.state = "idle".into();
    ids
}

/// The project policy file that says a copy answers the owner's posts while the agent works (fresh-lead-3 N-1104
/// p1154's line), as research's goes in by rule 6 (paper-research N-1446 p3020, 1).
const COPY_EFLINT: &str = "\
// `working` is agent-base's (policy/working.eflint); declared here too, so an engine before it still reads this file
Fact working Identified by agent

Act copy-answers
  Actor agent
  Recipient agent
  Holds when actor == \"owner\" and working(recipient)
";

/// A copy is made only of the one agent his post is written to (the owner's N-1794; N-1104 p1206: twelve copies, one
/// of every busy agent a post reached): `written_to_alone` holds for a post naming that agent and no other, not for one
/// naming several, nor one naming another (it reaches the agent as one on the thread).
#[test]
fn only_his_post_to_one_agent_alone_is_one_a_copy_answers() {
    let (dir, _, mut core, _rx) = coord("copy-alone", "working");
    let t = t_of(&open(&mut core, "lead", &["w2", "w3"], "the thread both work on"));
    let id = |core: &Core| format!("{}.p{}", core.tickets.m.id(t), core.tickets.m.tickets[&t].last());
    post_informing(&mut core, t, "owner", &["w2"], &[], None, "@w2 how far is it?");
    let alone = id(&core);
    assert!(core.written_to_alone(&alone, "w2") && !core.written_to_alone(&alone, "w3"), "to w2 alone");
    post_informing(&mut core, t, "owner", &["w2", "w3"], &[], None, "@w2 @w3 how far are you?");
    let both = id(&core);
    assert!(!core.written_to_alone(&both, "w2") && !core.written_to_alone(&both, "w3"), "to two: a copy of neither");
    post_informing(&mut core, t, "owner", &["w3"], &[], None, "@w3 and you?");
    let other = id(&core);
    assert!(!core.written_to_alone(&other, "w2") && core.written_to_alone(&other, "w3"), "w2 reached as one on the thread: no copy of it");
    assert!(!core.written_to_alone("N-999.p1", "w2"), "no such post");
    let _ = std::fs::remove_dir_all(&dir);
}

/// "A copy answers you, only while the agent is busy" (fresh-lead-3 N-1104 p1154, p1159; his yes p1160;
/// paper-research N-1446 p3020, 1), on agent-base's own delivery.kind, with a project's copy.eflint whose line says
/// it holds for the owner's posts while the agent works, p1154's line (agent-base declares no such act). w2 works on
/// its work, given at a place in its session, and its turn's beginning is a `turn_began` line naming that work, which
/// makes `working(w2)` (agent-base's working.eflint, the record's facts: ticket/effects.rs); his post comes. coordd
/// reads that the policy says a copy answers it and that w2's turn can be cut where it began (`arrived`'s
/// `copy_answers`, `forkable`; `began` is the line's), and the program starts a copy of w2,
/// cut at its work's place, in its folder, and hands it his post on its own, at once. The copy's answer is a post on
/// his thread that answers his post; his next post goes to the same copy. w2's turn ends while the copy still holds
/// his second post: everything waits (`answering`). The copy answers that too: w2 is given his two posts and the
/// copy's two answers, whole, as one message, and the copy ends. The record is in the era research's is in
/// (ERA_QUIET), where w2's own post at its turn's end leaves his posts due as any post. With WAY_BACK_OUT set the coord
/// is kept for the way back.
#[test]
fn his_post_to_a_working_agent_is_answered_at_once_by_a_copy_cut_where_its_turn_began_and_handed_over_whole_at_its_end() {
    let (dir, _, mut core, mut rx) = coord("copy", "idle");
    core.ticket_input(Input::Event { name: docket_machine::ERA.into(), value: json!({"era": docket_machine::ERA_QUIET}) }).unwrap();
    core.fold_by = None;
    let files = project(&dir, &mut core);
    std::fs::create_dir_all(files.join("policy")).unwrap();
    // research's file as it goes in by rule 6: `working` declared again (it overwrites agent-base's, the same), so an
    // engine before working.eflint still compiles the project's policy on a way back
    std::fs::write(files.join("policy/copy.eflint"), COPY_EFLINT).unwrap();
    assert!(!core.copy_answers("w2", "owner"), "the project's line: w2 is not working");
    // w2 is given its work while idle, its session recording where, and works
    let work = open(&mut core, "lead", &["w2"], "the hotfix: find where the thread goes in whole");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("its work");
    core.handed("w2", &ids, true, "channel");
    let (path, lines) = fork_session(&dir, &work);
    core.merge_session("w2", &json!({"session_id": "s-w2", "transcript_path": path, "cwd": dir.display().to_string()}));
    let given = Loc { file: path.clone(), offset: before_line(&lines, 1), len: lines[1].len() as u64 };
    let _ = core.transcript_read("w2", vec![(work.clone(), path.clone(), given.offset, given.len)], vec![], vec![], &[], None, &[], false);
    core.ticket_delivered("w2", &work, given, "2026-11-24T21:30:00Z", false);
    core.begin_turn("w2");
    working(&mut core);
    let began: Vec<Value> = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["ev"] == "event" && v["name"] == docket_machine::TURN_BEGAN).map(|v| v["value"].clone()).collect();
    assert_eq!(began.last(), Some(&json!({"agent": "w2", "delivery": work})), "the line its turn began with: {began:?}");
    assert_eq!(core.tickets.m.began("w2"), Some(work.clone()));
    assert!(core.copy_answers("w2", "owner") && !core.copy_answers("w2", "lead"), "the project's line: w2 works, and only his posts");
    // his post: a copy, cut where the turn began, is started and handed it, at once
    let t = t_of(&work);
    let his1 = post_informing(&mut core, t, "owner", &["w2"], &[], None, "@w2 how far is it?");
    let n1 = core.tickets.m.tickets[&t].last();
    core.raise_delivery();
    let arrived: Vec<Value> = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["ev"] == "event" && v["name"] == "arrived" && v["value"]["agent"] == "w2").map(|v| v["value"].clone()).collect();
    assert_eq!(arrived.last().map(|v| (v["forkable"].clone(), v["copy_answers"].clone())), Some((json!(true), json!(true))), "{arrived:?}");
    let acts: Vec<String> = acts_of_w2(&dir).into_iter().map(|a| a.0).collect();
    assert_eq!(acts[acts.len() - 3..], ["fork", "deliver", "keep"], "{acts:?}");
    let copy = core.tickets.m.copy_of("w2").expect("w2's copy");
    let (starts, _) = starts_and_ends(std::iter::from_fn(|| rx.try_recv().ok()));
    assert_eq!(starts, vec![(copy.clone(), "copy".to_string(), Some("w2".to_string()), Some(work.clone()))]);
    assert_eq!(kept(&core, &his1).as_deref(), Some("its copy answers it now; it comes when its turn ends"));
    // roster's fork --cut registers it; it is up, idle, and takes his post
    assert!(core.add(&Who::Owner, &copy, "claude", "m").ok);
    core.agents.get_mut(&copy).unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), turn: 1, ..Default::default() };
    core.agents.get_mut(&copy).unwrap().ended_turn = 1;
    assert!(core.copy_answering("w2"), "his post waits for the copy");
    core.raise_delivery();
    assert_eq!(copy_turn(&mut core, &mut rx, &dir, &copy, "about halfway: the thread goes in whole at machine.rs:1222"), vec![format!("{his1}.told")]);
    let c1 = core.tickets.m.tickets[&t].last();
    let x = &core.tickets.m.tickets[&t];
    let reply = x.posts.iter().find(|p| p.n == c1).unwrap();
    assert!(reply.by == copy && x.answers(reply, n1), "its answer, under his post: {:?}", (reply.by.as_str(), reply.n));
    assert!(!core.copy_answering("w2"));
    // his next post goes to the same copy
    let his2 = post_informing(&mut core, t, "owner", &["w2"], &[], None, "@w2 and the lock?");
    core.raise_delivery();
    assert_eq!(core.tickets.m.copy_of("w2"), Some(copy.clone()));
    let (starts, _) = starts_and_ends(std::iter::from_fn(|| rx.try_recv().ok()));
    assert_eq!(starts, vec![], "no second copy");
    // w2's turn ends while the copy holds his second post: everything waits for it
    end_turn(&mut core, &dir, "found it");
    core.raise_delivery();
    assert_eq!(free_values(&dir).last().map(|v| v["answering"].clone()), Some(json!(true)));
    assert_eq!(kept(&core, &his1).as_deref(), Some("its copy is still answering the owner"));
    assert!(channel(&mut core, &mut rx).is_none(), "nothing for w2 while its copy answers");
    // the copy answers it: w2 gets his posts and the copy's answers, whole, as one message, and the copy ends
    copy_turn(&mut core, &mut rx, &dir, &copy, "the lock is free since 21:20");
    let c2 = core.tickets.m.tickets[&t].last();
    core.raise_delivery();
    assert_eq!(free_values(&dir).last().map(|v| v["answering"].clone()), Some(json!(false)));
    core.hand_over("w2");
    let (mut handed, mut jobs) = (None, Vec::new());
    while let Ok(j) = rx.try_recv() {
        match j {
            Job::Channel { tickets, .. } if handed.is_none() => handed = Some(tickets.iter().map(|(d, _)| (d.id.clone(), d.shows())).collect::<Vec<(String, Vec<u32>)>>()),
            j => jobs.push(j),
        }
    }
    // his two posts as one delivery of his thread (coordd hands a thread's posts up to the last of them), then the
    // copy's two answers, each whole (told late); nothing twice
    let id = core.tickets.m.id(t);
    let n2 = docket_machine::post_id(&his2).map(|(_, n)| n).unwrap();
    let expected = vec![(his2.clone(), vec![n1, n2]), (format!("{id}.p{c1}.told"), vec![c1]), (format!("{id}.p{c2}.told"), vec![c2])];
    assert_eq!(handed, Some(expected), "w2's acts {:?}", acts_of_w2(&dir));
    let (_, ends) = starts_and_ends(jobs);
    assert_eq!(ends, vec![copy.clone()]);
    assert_eq!(core.tickets.m.copy_of("w2"), None);
    crate::core::brain::tests::kept_for_the_way_back(&dir, "copy");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A turn is named by the work it began with, not by a digest's post on a thread numbered lower (docket-edge5's audit,
/// N-1446 p3263: N-1104 p1160 named the turns of six agents it went to only as a digest line, their item N-1446 p3020;
/// L10, proto-mind p3271, paper-research p3273). w3's fyi to w2 on a thread numbered lower waits, pulled; the lead's
/// work for w2 goes, the fyi in its digest, and coordd gives both threads. The session takes the work's message, so
/// the turn's `turn_began` line names the work; the turn's end line names it too, and the next `free` has the work for
/// its `place`, how far the session ran on since then, and `forkable`. Before, the first of the turn's deliveries
/// sorted as text named it, the fyi's thread, given nowhere in the session: `since` "-" and `forkable` false.
#[test]
fn a_turn_is_named_by_the_work_it_began_with_and_its_free_has_that_place_not_a_digests_thread_numbered_lower() {
    let (dir, _, mut core, mut rx) = coord("began-digest", "idle");
    core.fold_by = None;
    let own = t_of(&open(&mut core, "w2", &["lead"], "the replay plan"));
    let fyi = typed(&mut core, own, "w3", &["w2"], PostKind::Fyi, "build 92 is in");
    assert!(core.to_hand_over().is_empty(), "pulled: it wakes nothing");
    let work = open(&mut core, "lead", &["w2"], "the hotfix: find where the thread goes in whole");
    assert!(fyi < work, "{fyi} sorts before {work}");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("its work, the fyi in its digest");
    assert_eq!(ids, [work.clone(), fyi.clone()], "the work, and the digest's thread up to its last post");
    core.handed("w2", &ids, true, "channel");
    assert_eq!(core.tickets.m.given_since("w2"), vec![fyi.clone(), work.clone()], "sorted as text: the digest's first");
    // its session takes the work's message; its turn begins with it
    let (path, lines) = fork_session(&dir, &work);
    core.merge_session("w2", &json!({"session_id": "s-w2", "transcript_path": path, "cwd": dir.display().to_string()}));
    let given = Loc { file: path.clone(), offset: before_line(&lines, 1), len: lines[1].len() as u64 };
    let _ = core.transcript_read("w2", vec![(work.clone(), path.clone(), given.offset, given.len)], vec![], vec![], &[], None, &[], false);
    core.ticket_delivered("w2", &work, given, "2026-11-25T02:40:00Z", false);
    core.begin_turn("w2");
    working(&mut core);
    assert_eq!(core.tickets.m.began("w2"), Some(work.clone()), "coordd's line: the delivery given first in its session");
    // an ask for it while it works, kept for its turn's end, so a `free` comes then
    let ask = post_informing(&mut core, t_of(&work), "lead", &[], &[], Some(PostKind::Ask { to: vec!["w2".into()] }), "and the lock?");
    core.raise_delivery();
    assert_eq!(kept(&core, &ask).as_deref(), Some("comes when its turn ends"));
    end_turn(&mut core, &dir, "found it");
    let ended: Vec<Value> = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["ev"] == "turn_ended" && v["agent"] == "w2").map(|v| v["delivery"].clone()).collect();
    assert_eq!(ended, vec![json!(work)], "the turn's end line names its work");
    core.raise_delivery();
    let free = free_values(&dir).last().cloned().unwrap();
    assert_eq!((free["worked"].clone(), free["place"].clone(), free["forkable"].clone()), (json!(true), json!(work), json!(true)), "{free}");
    assert_ne!(free["since"], json!("-"), "{free}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The copy when coordd begins the turn a read before the session shows its prompt (try-80; proto-mind's copy proof,
/// paper-research N-1446 p3297, p3298, p3314: every turn of an unboxed project on engine 80, where its hooks or screen
/// say the turn runs first): its `turn_began` line names none, as no giving has a place yet. Then the work's place is
/// read and its `delivered` line says it began the turn: coordd writes the turn's `turn_began` line again, naming the
/// work, so the machine's `began` is it and his post while it runs goes to a copy cut there, at once, as when the
/// first line named it. Before, `arrived` said `forkable: false` and his post waited for the turn's end.
#[test]
fn his_post_goes_to_a_copy_when_the_turn_began_before_its_works_place_was_read() {
    let (dir, _, mut core, mut rx) = coord("copy-early", "idle");
    core.ticket_input(Input::Event { name: docket_machine::ERA.into(), value: json!({"era": docket_machine::ERA_QUIET}) }).unwrap();
    core.fold_by = None;
    let files = project(&dir, &mut core);
    std::fs::create_dir_all(files.join("policy")).unwrap();
    std::fs::write(files.join("policy/copy.eflint"), COPY_EFLINT).unwrap();
    let work = open(&mut core, "lead", &["w2"], "the hotfix: find where the thread goes in whole");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("its work");
    core.handed("w2", &ids, true, "channel");
    let (path, lines) = fork_session(&dir, &work);
    core.merge_session("w2", &json!({"session_id": "s-w2", "transcript_path": path, "cwd": dir.display().to_string()}));
    // its turn begins before the session's read shows the work: the line names none
    core.begin_turn("w2");
    working(&mut core);
    let began_lines = |dir: &Path| -> Vec<Value> { std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["ev"] == "event" && v["name"] == docket_machine::TURN_BEGAN && v["value"]["agent"] == "w2").map(|v| v["value"].clone()).collect() };
    let n = began_lines(&dir).len();
    assert_eq!(began_lines(&dir).last(), Some(&json!({"agent": "w2", "delivery": null})));
    assert_eq!(core.tickets.m.began("w2"), None);
    // then its place, and the prompt that began the turn: the turn's line again, naming the work
    let given = Loc { file: path.clone(), offset: before_line(&lines, 1), len: lines[1].len() as u64 };
    let _ = core.transcript_read("w2", vec![(work.clone(), path.clone(), given.offset, given.len)], vec![], vec![], &[], None, &[], false);
    core.ticket_delivered("w2", &work, given.clone(), "2026-11-25T03:25:12Z", true);
    assert_eq!(began_lines(&dir)[n..], [json!({"agent": "w2", "delivery": work})]);
    assert_eq!(core.tickets.m.began("w2"), Some(work.clone()));
    // read again, it is delivered already: no third line
    core.ticket_delivered("w2", &work, given, "2026-11-25T03:25:12Z", true);
    assert_eq!(began_lines(&dir).len(), n + 1);
    // his post: a copy, cut at the work, is started and handed it, at once
    let his = post_informing(&mut core, t_of(&work), "owner", &["w2"], &[], None, "@w2 what is 7 times 6?");
    core.raise_delivery();
    let arrived: Vec<Value> = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["ev"] == "event" && v["name"] == "arrived" && v["value"]["agent"] == "w2").map(|v| v["value"].clone()).collect();
    assert_eq!(arrived.last().map(|v| (v["forkable"].clone(), v["copy_answers"].clone())), Some((json!(true), json!(true))), "{arrived:?}");
    let copy = core.tickets.m.copy_of("w2").expect("w2's copy");
    let (starts, _) = starts_and_ends(std::iter::from_fn(|| rx.try_recv().ok()));
    assert_eq!(starts, vec![(copy.clone(), "copy".to_string(), Some("w2".to_string()), Some(work.clone()))]);
    assert_eq!(kept(&core, &his).as_deref(), Some("its copy answers it now; it comes when its turn ends"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// In the usual order the work's place is read and delivered before coordd begins the turn: one `turn_began` line,
/// naming the work, and the `delivered` line before it brings none (the agent was not working yet).
#[test]
fn a_turn_whose_works_place_was_read_first_has_one_turn_began_line() {
    let (dir, _, mut core, mut rx) = coord("began-once", "idle");
    core.fold_by = None;
    let work = open(&mut core, "lead", &["w2"], "the hotfix: find where the thread goes in whole");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("its work");
    core.handed("w2", &ids, true, "channel");
    let (path, lines) = fork_session(&dir, &work);
    core.merge_session("w2", &json!({"session_id": "s-w2", "transcript_path": path, "cwd": dir.display().to_string()}));
    let given = Loc { file: path.clone(), offset: before_line(&lines, 1), len: lines[1].len() as u64 };
    let _ = core.transcript_read("w2", vec![(work.clone(), path.clone(), given.offset, given.len)], vec![], vec![], &[], None, &[], false);
    core.ticket_delivered("w2", &work, given, "2026-11-25T03:25:12Z", true);
    core.begin_turn("w2");
    let began: Vec<Value> = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["ev"] == "event" && v["name"] == docket_machine::TURN_BEGAN && v["value"]["agent"] == "w2").map(|v| v["value"].clone()).collect();
    assert_eq!(began, [json!({"agent": "w2", "delivery": work})]);
    assert_eq!(core.tickets.m.began("w2"), Some(work.clone()));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A copy coordd could not start (proto-mind's copy proof on engine 81, p3335: no entry to cut its session at, as its
/// channel tag carried no id; paper-research N-1446 p3336): his post while w2 works makes the copy, and its start
/// fails as `Job::Start` says so, with `start_failed` naming the copy's delivery ticket. The line is on the record,
/// w2 has no copy, and a start of coordd, replaying the record, has none either. Before, the line was not written,
/// and a start kept the copy that never ran.
#[test]
fn a_copy_that_could_not_start_is_on_the_record_and_a_start_keeps_no_copy() {
    let (dir, base, mut core, mut rx) = coord("copy-failed", "idle");
    core.ticket_input(Input::Event { name: docket_machine::ERA.into(), value: json!({"era": docket_machine::ERA_QUIET}) }).unwrap();
    core.fold_by = None;
    let files = project(&dir, &mut core);
    std::fs::create_dir_all(files.join("policy")).unwrap();
    std::fs::write(files.join("policy/copy.eflint"), COPY_EFLINT).unwrap();
    let work = open(&mut core, "lead", &["w2"], "the hotfix: find where the thread goes in whole");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("its work");
    core.handed("w2", &ids, true, "channel");
    let (path, lines) = fork_session(&dir, &work);
    core.merge_session("w2", &json!({"session_id": "s-w2", "transcript_path": path, "cwd": dir.display().to_string()}));
    let given = Loc { file: path.clone(), offset: before_line(&lines, 1), len: lines[1].len() as u64 };
    let _ = core.transcript_read("w2", vec![(work.clone(), path.clone(), given.offset, given.len)], vec![], vec![], &[], None, &[], false);
    core.ticket_delivered("w2", &work, given, "2026-11-25T03:25:12Z", true);
    core.begin_turn("w2");
    working(&mut core);
    assert_eq!(core.tickets.m.began("w2"), Some(work.clone()));
    // his post: the copy, and its start
    post_informing(&mut core, t_of(&work), "owner", &["w2"], &[], None, "@w2 what is 7 times 6?");
    core.raise_delivery();
    let copy = core.tickets.m.copy_of("w2").expect("w2's copy");
    let started: Vec<(String, u64)> = std::iter::from_fn(|| rx.try_recv().ok()).filter_map(|j| match j { Job::Start { agent, view, cut: Some(_), .. } => Some((agent, view.n)), _ => None }).collect();
    let [(agent, d)] = started.as_slice() else { panic!("{started:?}") };
    assert_eq!(agent, &copy);
    // it could not start: Job::Start's input
    let why = format!("no entry of {path} after {work}'s given line holds its message");
    core.ticket_input(Input::StartFailed { t: *d, agent: copy.clone(), why }).unwrap();
    let failed: Vec<(Value, Value)> = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["ev"] == "start_failed").map(|v| (v["t"].clone(), v["agent"].clone())).collect();
    assert_eq!(failed, [(json!(d), json!(copy))]);
    assert_eq!(core.tickets.m.copy_of("w2"), None);
    drop(core);
    let (again, _jobs) = load(&dir, &base, "idle");
    let problems: Vec<&String> = again.tickets.errors.iter().filter(|e| e.contains("log.jsonl")).collect();
    assert_eq!((again.tickets.m.copy_of("w2"), problems), (None, Vec::<&String>::new()), "a start replays the record so");
    crate::core::brain::tests::kept_for_the_way_back(&dir, "copy-failed");
    let _ = std::fs::remove_dir_all(&dir);
}

/// N-1788, the owner's urgent hand: w2 in its turn, his post to it waits (the program does not give it into the turn
/// by itself); his DeliverUrgent without interrupt hands it now, into the running turn, nothing stopped; with
/// interrupt his next post goes as the turn is stopped. Nothing of his waiting is an error, not a silent nothing; an
/// agent's is refused. His pace: Step kept in the registry over a restart of coordd, and refused to anyone else.
#[test]
fn the_owners_urgent_hand_goes_into_the_turn_or_stops_it_and_step_is_kept() {
    use coord_types::AgentMode;
    let (dir, base, mut core, mut rx) = coord("urgent", "working");
    let p = open(&mut core, "owner", &["w2"], "stop and look at the lock");
    core.raise_delivery();
    assert!(core.tickets.m.hands("w2").is_empty(), "the program keeps it for the turn's end");
    assert!(!core.deliver_urgent(&Who::Agent("lead".into()), "w2", false).ok, "the owner's");
    let r = core.deliver_urgent(&Who::Owner, "w2", false);
    assert!(r.ok && r.out.contains("into its running turn"), "{}", r.out);
    assert_eq!(core.tickets.m.hands("w2").iter().map(|h| (h.items.clone(), h.idle)).collect::<Vec<_>>(), vec![(vec![p.clone()], false)]);
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("his post, into the turn");
    assert_eq!(ids, [p.clone()]);
    let r = core.deliver_urgent(&Who::Owner, "w2", false);
    assert!(!r.ok && r.out.contains("Nothing of the owner's waits"), "{}", r.out);
    let q = open(&mut core, "owner", &["w2"], "no: stop now");
    let r = core.deliver_urgent(&Who::Owner, "w2", true);
    assert!(r.ok && r.out.contains("being stopped"), "{}", r.out);
    assert!(core.tickets.stopping.contains("w2"));
    assert!(core.tickets.m.hands("w2").iter().any(|h| h.items.contains(&q) && !h.idle));
    // his pace
    assert_eq!(core.view("w2").control, AgentMode::Run);
    assert!(!core.set_agent_mode(&Who::Agent("lead".into()), "w2", AgentMode::Step).ok);
    assert!(core.set_agent_mode(&Who::Owner, "w2", AgentMode::Step).ok);
    assert_eq!(core.view("w2").control, AgentMode::Step);
    let (again, _) = load(&dir, &base, "idle");
    assert_eq!((again.agents["w2"].control, again.agents["w3"].control), (AgentMode::Step, AgentMode::Run), "kept in the registry");
    let _ = std::fs::remove_dir_all(&dir);
}

/// An agy agent's `coord effort` takes agy's thinking levels, recorded as MODEL:EFFORT and sent with its next turn.
#[test]
fn an_agy_agents_effort_is_its_thinking_level_from_its_next_turn() {
    let (dir, _, mut core, _rx) = coord("agyeffort", "idle");
    let a = core.agents.get_mut("w2").unwrap();
    (a.kind, a.model) = ("agy".into(), "agy/gemini-3.8-flash".into());
    let r = core.effort(&Who::Owner, "w2", "minimal", None);
    assert!(!r.ok && r.out.contains("low, medium, high, xhigh, max"), "{}", r.out);
    let r = core.effort(&Who::Owner, "w2", "xhigh", None);
    assert!(r.ok && r.out.contains("effort default → xhigh"), "{}", r.out);
    assert_eq!(core.agents["w2"].model, "agy/gemini-3.8-flash:xhigh");
    let log = std::fs::read_to_string(dir.join("registry.jsonl")).unwrap();
    assert!(log.lines().any(|l| l.contains(r#""model":"agy/gemini-3.8-flash:xhigh""#)), "recorded");
}
