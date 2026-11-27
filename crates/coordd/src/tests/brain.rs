//! An agent's brain, coordd's half (brain.rs; 0.3, proto-mind N-1446 p35): a pass's steps go on the agent's own brain
//! queue, each answered from one call on a report built from the record; the selector's thought is posted informing
//! the agent, never goes into a running turn, and goes whole with the agent's next message, after his posts (76;
//! fresh-lead-3 N-1540 p5, p18), at once to an idle agent. Every answer here is made up and read through the call's own
//! check (`clerk::brain_ok`): no model runs.

use super::super::delivery::tests::{channel, coord, end_turn, his_into, kept, open, t_of, working};
use crate::clerk::{brain_failed, brain_ok, Brain, ATTENTION, CONSOLIDATION, ELECTION, OUTLOOK, OWNER_DELTA, OWNER_WHOLE};
use crate::core::{Core, Job, Who};
use coord_types::BrainView;
use docket_machine::syntax::{COMPACTING, OWNER_TO, TURN_END};
use docket_machine::{Delivery, Input, Status, Text, What, BRAIN_SWITCH, WATCHER};
use std::path::Path;
use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedReceiver;

/// A pass of w2's brain on his posts BURST, opened as its binding opens it (brain.kind's header): under his thread,
/// so it stands at his post. Its ticket.
fn pass(core: &mut Core, burst: &[&str]) -> u64 {
    let form = json!({"agent": "w2", "pass": {"head": "Attend", "posts": burst}});
    let r = core.ticket_input(Input::Open { kind: "brain".into(), under: Some(t_of(burst[0])), opener: "coordd".into(), form, text: None, names: vec![], told: vec![] }).unwrap();
    docket_machine::parse_ticket(r.trim_start_matches("Opened ").split(['.', ';']).next().unwrap()).unwrap()
}

/// The coord at DIR given a project whose policy is POLICY (one file), as a project's policy folder holds it.
fn policy(core: &mut Core, dir: &std::path::Path, policy: &str) {
    let project = dir.join("files");
    for d in ["kinds", "calls", "policy"] {
        std::fs::create_dir_all(project.join(d)).unwrap();
    }
    std::fs::write(project.join("policy/brain.eflint"), policy).unwrap();
    std::fs::write(project.join("project.toml"), "[project]\nname = \"r\"\n").unwrap();
    std::fs::write(dir.join("project"), project.join("project.toml").display().to_string()).unwrap();
    core.project = crate::project::Project::load(&dir.display().to_string()).unwrap();
}

/// The step of pass T coordd hands now: on w2's brain queue, apart from the clerk's and from its `next`.
fn step(core: &mut Core, rx: &mut UnboundedReceiver<Job>, t: u64) -> Delivery {
    core.hand_clerk();
    let mut out = None;
    while let Ok(j) = rx.try_recv() {
        match j {
            Job::Deliver { queue, delivery } if delivery.t == t => {
                assert_eq!(queue, "brain w2");
                out = Some(delivery);
            }
            Job::Clerk { deliveries } => assert!(deliveries.iter().all(|(d, _)| d.t != t), "no step of the brain's goes to the clerk"),
            _ => {}
        }
    }
    out.expect("the brain's step, on its own queue")
}

/// The call step D is, its job as main.rs makes it.
fn job(core: &mut Core, d: &Delivery) -> (crate::clerk::Job, Brain) {
    let job = core.brain_job(d).expect("a brain job");
    let b = job.brain.clone().expect("the brain's call");
    (job, b)
}

/// Step D answered with GOT, made up, as its call's check reads it.
fn answer(core: &mut Core, d: &Delivery, got: Value) -> Brain {
    let (job, b) = job(core, d);
    let head = brain_ok(&got, &b).unwrap_or_else(|| panic!("in shape: {got}"));
    core.brain_answered(&job, d, head, None, None);
    b
}

fn call(d: &Delivery) -> String {
    let What::Step { call, .. } = &d.what else { panic!("{d:?}") };
    call.clone()
}

/// The owner model's reading of his post HIS: off his topic, a correction, and one proposal on it.
fn drifted(his: &str) -> Value {
    json!({"head": "Read", "topic": "the brain's mechanism", "on_topic": false,
           "signals": [{"kind": "correction", "post": his, "words": "the brain shoud prbluy be tleling oyu getb akc to the owner topic"}],
           "proposals": [{"act": "answer him on the brain, with no build news", "rests_on": his}]})
}

/// The selector's pick of the proposal on HIS, with THOUGHT.
fn picked(his: &str, thought: &str) -> Value {
    json!({"head": "Pick", "act": "answer him on the brain, with no build news", "rests_on": his, "against": "he may want the build's state too", "thought": thought,
           "scores": [{"act": "answer him on the brain, with no build news", "newest": 2, "rules": 2, "owed": 2, "sees": 1, "cost": 2, "rank": 1},
                      {"act": "nothing", "newest": 0, "rules": 2, "owed": 0, "sees": 0, "cost": 2, "rank": 2}]})
}

/// A pass on his post HIS that speaks, as the drift's: the owner model reads w2 off his topic and the selector picks
/// its proposal, with THOUGHT. The id of the line it posts.
fn said(core: &mut Core, rx: &mut UnboundedReceiver<Job>, his: &str, thought: &str) -> String {
    let t = pass(core, &[his]);
    let d = step(core, rx, t);
    answer(core, &d, drifted(his));
    let d = step(core, rx, t);
    answer(core, &d, picked(his, thought));
    let p = core.tickets.m.tickets[&t].posts.last().unwrap();
    assert_eq!(p.into_turn, ["w2"], "the line asks into w2's turn");
    format!("{}.p{}", core.tickets.m.id(t), p.n)
}

/// The `fold` steps due now (delivery.kind's `free` at a working agent's turn's end).
fn folds(core: &mut Core) -> Vec<Delivery> {
    core.ticket_due(WATCHER).into_iter().filter(|d| matches!(&d.what, What::Step { call, .. } if call == "fold")).collect()
}

/// Whether DIR's record holds an `arrived` for ID.
fn arrived(dir: &Path, id: &str) -> bool {
    let log = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap_or_default();
    log.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).any(|l| l["ev"] == "event" && l["name"] == docket_machine::syntax::ARRIVED && l["value"]["item"] == json!(id))
}

/// The pass's result, once it has closed.
fn result(core: &Core, t: u64) -> Value {
    match &core.tickets.m.tickets[&t].status {
        Status::Done(head) => head.clone(),
        s => panic!("not closed: {s:?}"),
    }
}

/// The drift (his N-1104 p614-p619): his post goes into w2's turn, the owner model reads w2 off his topic, the
/// selector picks its proposal. The thought is posted on the pass, by the pass, informing w2 and waking nobody. It
/// never goes into the turn his post began (76; fresh-lead-3 N-1540 p5: on the text before, `arrived` put it in): no
/// `arrived`, nothing handed. At that turn's end `free` gives it whole, alone, with no call, and with no footer: it asks
/// for nothing. Each call ran on w2's brain queue, never the clerk's.
#[test]
fn the_thought_waits_for_the_end_of_the_turn_his_post_began_and_goes_whole_asking_nothing() {
    let (dir, _) = the_drift_into_his_turn("brain-into", |core, _, his| pass(core, &[his]));
    let _ = std::fs::remove_dir_all(&dir);
}

/// research's binding (flavor projects/research/kinds/agents/paper-research.kind: brain.kind's header line), on w2.
const BOUND: &str = "on owner_to(m) where m.agent == \"w2\" open brain(m.agent, Attend(m.posts))";

/// The brain as research turns it on (his N-1104 p685: "turning on the brain would be fine"): the agent's own program
/// (`kinds/agents/NAME.kind`) holds brain.kind's binding line with its name, and the reload that brings it, read as a
/// start reads it, is taken. The machine raises `owner_to` at the hand of his post, with the burst (docket-machine's
/// `the_owners_held_post_raises_owner_to_at_the_hand_for_each_agent_it_wakes`); fed back as coordd's task runner feeds
/// it, it opens one pass whose form is that burst, standing at his post as a note of his thread's line (brain.kind's
/// `drawn note`: without a `drawn` word the pass stood nowhere, so its thought was about no turn and waited for w2's
/// next delivery), and the pass goes on as the drift above: its thought goes at the end of the turn his post began.
/// Another agent's `owner_to` opens nothing.
#[test]
fn research_binding_opens_the_pass_at_his_post_and_its_thought_goes_at_his_turns_end() {
    let (dir, _) = the_drift_into_his_turn("brain-bound", |core, dir, his| {
        bind(core, dir);
        let raised = |agent: &str| Input::Event { name: OWNER_TO.into(), value: json!({"agent": agent, "posts": [his], "tied_at": his}) };
        // w3's: the binding's `where` does not hold, and the machine refuses an event nothing takes
        let _ = core.ticket_input(raised("w3"));
        assert!(!core.tickets.m.tickets.values().any(|x| x.kind == "brain"), "w3's opens nothing");
        core.ticket_input(raised("w2")).unwrap();
        let passes: Vec<u64> = core.tickets.m.tickets.values().filter(|x| x.binding.as_deref() == Some(BOUND)).map(|x| x.t).collect();
        assert_eq!(passes.len(), 1, "one pass, w2's");
        let t = passes[0];
        assert_eq!(core.tickets.m.tickets[&t].form, json!({"agent": "w2", "pass": {"head": "Attend", "posts": [his]}}));
        assert_eq!(core.tickets.m.outline_parent(t), Some((t_of(his), 1)), "it stands at his post");
        assert_eq!(core.tickets.m.view(t).unwrap().drawn, Some(docket_machine::syntax::Drawn::Note), "a note there: drawn once it has spoken");
        t
    });
    let _ = std::fs::remove_dir_all(&dir);
}

/// The coord at DIR with research's policy and binding (`BOUND`) in w2's own program, brought by his reload.
fn bind(core: &mut Core, dir: &Path) {
    policy(core, dir, "+unanswered-within(43200).\n+earlier-within(604800).\n+earlier-lines(100).\n");
    std::fs::create_dir_all(dir.join("files/kinds/agents")).unwrap();
    std::fs::write(dir.join("files/kinds/agents/w2.kind"), format!("{BOUND}\n")).unwrap();
    let r = core.ticket_reload(&Who::Owner, None, "w2's brain on".into());
    assert!(r.ok, "{}", r.out);
}

/// His switch of w2's brain (proto-lead N-1446 p857.2), the binding in w2's own program: w2's card shows it on, never
/// pressed, and w3's has none. An agent's press, or one for an agent there is not, is refused. His Off is one
/// `brain_switch` line, and a second Off writes nothing. While it is off, `owner_to` for his post opens no pass, saying
/// why, and the binding is still in its file. His On is one line more; then the same post's pass opens, and the card
/// names it as the last. His Off again while that pass is at work (proto-lead N-1446 p987): a third line, and the same
/// post opens no new pass, while the one at work goes on as the drift above, to its thought in his turn. A start
/// replays the record with no problem line, his switch off since his second Off, the one pass Said. With WAY_BACK_OUT
/// set the coord is kept there for the way back's replay (an older release's `ticket::tests::a_coords_record_replays`).
/// The other ways a pass would open are the test below.
#[test]
fn his_switch_off_opens_no_pass_until_his_on_and_a_start_replays_it() {
    let (dir, base) = the_drift_into_his_turn("brain-switch", |core, dir, his| {
        bind(core, dir);
        let brain = |core: &Core, a: &str| core.view(a).brain;
        let switches = || std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap().lines().filter(|l| l.contains(&format!("\"{BRAIN_SWITCH}\""))).count();
        assert_eq!(brain(core, "w2"), Some(BrainView { on: true, ..Default::default() }), "on by the binding, never pressed");
        assert_eq!(brain(core, "w3"), None, "no binding opens w3's: its card has no switch");
        let r = core.ticket_brain(&Who::Agent("w2".into()), "w2", false);
        assert!(!r.ok && r.out.contains("only the owner"), "{}", r.out);
        assert!(!core.ticket_brain(&Who::Owner, "w9", false).ok, "no agent w9");
        // his Off, and Off again
        let r = core.ticket_brain(&Who::Owner, "w2", false);
        assert!(r.ok, "{}", r.out);
        let r = core.ticket_brain(&Who::Owner, "w2", false);
        assert!(r.ok && r.out.contains("already"), "{}", r.out);
        assert_eq!(switches(), 1, "a press that changes nothing writes nothing");
        let off = brain(core, "w2").unwrap();
        assert!(!off.on && off.since.is_some() && off.last_id.is_none(), "{off:?}");
        // while off: no pass
        let raised = Input::Event { name: OWNER_TO.into(), value: json!({"agent": "w2", "posts": [his], "tied_at": his}) };
        let e = core.ticket_input(raised.clone()).unwrap_err();
        assert!(e.contains("w2's brain is switched off"), "{e}");
        assert!(!core.tickets.m.tickets.values().any(|x| x.kind == "brain"), "no pass while off");
        assert!(std::fs::read_to_string(dir.join("files/kinds/agents/w2.kind")).unwrap().contains(BOUND), "the binding stays in its file");
        // his On: the pass opens
        assert!(core.ticket_brain(&Who::Owner, "w2", true).ok);
        assert_eq!(switches(), 2);
        core.ticket_input(raised.clone()).unwrap();
        let t = *core.tickets.m.tickets.iter().find(|(_, x)| x.kind == "brain").unwrap().0;
        let on = brain(core, "w2").unwrap();
        assert_eq!(on, BrainView { on: true, since: on.since.clone(), last_pass: Some(core.tickets.m.tickets[&t].opened_at.clone()), last_id: Some(core.tickets.m.id(t)), ready: Vec::new() });
        assert!(on.since.is_some(), "since his On");
        // his Off while that pass is at work (proto-lead N-1446 p987): it runs to its end (the drift goes on below,
        // to its thought in his turn), and no new one opens
        assert!(core.ticket_brain(&Who::Owner, "w2", false).ok);
        assert_eq!(switches(), 3);
        let e = core.ticket_input(raised).unwrap_err();
        assert!(e.contains("w2's brain is switched off"), "{e}");
        assert_eq!(core.tickets.m.tickets.values().filter(|x| x.kind == "brain").count(), 1, "none opens after his Off");
        t
    });
    // a start replays the lines: no problem line, off since his second Off, the one pass
    let again = crate::ticket::Tickets::load(&dir.display().to_string(), &base, Some(&dir.join("files").display().to_string()), 1);
    assert!(again.errors.is_empty(), "{:#?}", again.errors);
    assert!(!again.m.brain_is_on("w2") && again.m.brain_switched.get("w2").is_some_and(|(on, _)| !*on));
    assert_eq!(again.m.tickets.values().filter(|x| x.kind == "brain").count(), 1);
    assert!(again.m.tickets.values().any(|x| x.kind == "brain" && matches!(&x.status, Status::Done(h) if h["head"] == "Said")), "it ran to its thought");
    kept_for_the_way_back(&dir, "brain-switch");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A kind of the project's whose step opens a pass of AGENT's brain on POSTS, as base.kind's `open question(…)` opens
/// its question (consult-3 N-1446 p981).
const WAKE: &str = "\
kind wake
  letter W
  form agent: Agent, posts: [Text]
  result Text
  pattern
    b <- open brain(agent, Attend(posts));
    return \"woke\"
";

/// His Off stops a new pass of w2's brain whichever way it would open (consult-3 N-1446 p981, proto-lead p985), each
/// saying since when and making no pass. w3's `coord open brain` naming w2 is refused. A step's `open brain(w2, …)`
/// (`WAKE`) is stuck, as at a kind there is not, with the same words. A rekind of that ticket into w2's brain is
/// refused. w3's own pass opens all the while. After his On each of the three opens w2's pass. The binding's way is
/// the test above.
#[test]
fn his_switch_off_stops_a_pass_every_way_it_would_open() {
    let (dir, _, mut core, _rx) = coord("brain-ways", "idle");
    policy(&mut core, &dir, "");
    std::fs::write(dir.join("files/kinds/wake.kind"), WAKE).unwrap();
    let r = core.ticket_reload(&Who::Owner, None, "a step that opens a brain".into());
    assert!(r.ok, "{}", r.out);
    let his = open(&mut core, "owner", &["w2"], "the brain");
    let passes = |core: &Core, agent: &str| core.tickets.m.tickets.values().filter(|x| x.kind == "brain" && x.form["agent"] == agent).count();
    let brain = |agent: &str| json!({"agent": agent, "pass": {"head": "Attend", "posts": [his]}});
    let opened = |core: &mut Core, kind: &str, form: Value| core.ticket_input(Input::Open { kind: kind.into(), under: None, opener: "w3".into(), form, text: None, names: vec![], told: vec![] });
    let rekind = |core: &mut Core, t: u64| core.ticket_input(Input::Rekind { t, by: "owner".into(), kind: "brain".into(), form: brain("w2"), why: "his pass".into() });
    assert!(core.ticket_brain(&Who::Owner, "w2", false).ok);
    let off = format!("w2's brain is switched off, since {} UTC", &core.tickets.m.brain_switched["w2"].1[11..16]);
    // `coord open brain`: refused, saying since when
    let e = opened(&mut core, "brain", brain("w2")).unwrap_err();
    assert!(e.starts_with(&off), "{e}");
    // a step's open: its ticket is stuck there, with the same words
    opened(&mut core, "wake", json!({"agent": "w2", "posts": [his]})).unwrap();
    let k = *core.tickets.m.tickets.iter().find(|(_, x)| x.kind == "wake").unwrap().0;
    match &core.tickets.m.tickets[&k].status {
        Status::Failed(r) | Status::Stopped { raised: r, .. } => assert!(r.exc == "Stuck" && r.value["why"] == off.as_str(), "{r:?}"),
        s => panic!("not stuck: {s:?}"),
    }
    // a rekind into w2's brain: refused
    let e = rekind(&mut core, k).unwrap_err();
    assert!(e.starts_with(&off), "{e}");
    assert_eq!(passes(&core, "w2"), 0, "no pass of w2's while off");
    // another agent's pass opens
    opened(&mut core, "brain", brain("w3")).unwrap();
    assert_eq!(passes(&core, "w3"), 1);
    // his On: each opens one
    assert!(core.ticket_brain(&Who::Owner, "w2", true).ok);
    opened(&mut core, "brain", brain("w2")).unwrap();
    assert_eq!(passes(&core, "w2"), 1, "`coord open brain`");
    opened(&mut core, "wake", json!({"agent": "w2", "posts": [his]})).unwrap();
    assert_eq!(passes(&core, "w2"), 2, "a step's open");
    rekind(&mut core, k).unwrap();
    assert_eq!((passes(&core, "w2"), core.tickets.m.tickets[&k].kind.as_str()), (3, "brain"), "the rekind");
    let _ = std::fs::remove_dir_all(&dir);
}

/// With WAY_BACK_OUT set, the coord DIR copied to WAY_BACK_OUT/TAG for the way back's replay (an older release's
/// `ticket::tests::a_coords_record_replays`, its REPLAY_COORD and REPLAY_PROJECT): its files as they are, and its
/// `project` naming the copy's project.
pub(crate) fn kept_for_the_way_back(dir: &Path, tag: &str) {
    let Ok(out) = std::env::var("WAY_BACK_OUT") else { return };
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap().flatten() {
            let (p, ty) = (e.path(), e.file_type().unwrap());
            if ty.is_dir() {
                copy(&p, &to.join(e.file_name()));
            } else if ty.is_file() {
                std::fs::copy(&p, to.join(e.file_name())).unwrap();
            }
        }
    }
    let to = Path::new(&out).join(tag);
    let _ = std::fs::remove_dir_all(&to);
    copy(dir, &to);
    // a coord with a project of its own (`files`) names the copy's; one with none (delivery.rs's) has no `project`
    if to.join("files/project.toml").is_file() {
        std::fs::write(to.join("project"), to.join("files/project.toml").display().to_string()).unwrap();
    }
    println!("kept for the way back: {}", to.display());
}

/// The drift, with the pass OPENS gives on his post HIS (in the coord DIR), once his post is in w2's running turn. The
/// coord's folder and its base, which the caller removes.
fn the_drift_into_his_turn(tag: &str, opens: impl FnOnce(&mut Core, &Path, &str) -> u64) -> (std::path::PathBuf, String) {
    let (dir, base, mut core, mut rx) = coord(tag, "idle");
    let his = open(&mut core, "owner", &["w2"], "the brain shoud prbluy be tleling oyu getb akc to the owner topic");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("his post");
    assert_eq!(ids, [his.clone()]);
    core.handed("w2", &ids, true, "channel");
    working(&mut core);
    let t = opens(&mut core, &dir, &his);
    let d = step(&mut core, &mut rx, t);
    assert_eq!(call(&d), "attend");
    let b = answer(&mut core, &d, drifted(&his));
    assert_eq!((b.call.as_str(), b.burst.clone()), (ATTENTION, vec![his.clone()]));
    let d = step(&mut core, &mut rx, t);
    assert_eq!(call(&d), "elect");
    let (_, b) = job(&mut core, &d);
    assert_eq!(b.call, ELECTION);
    let candidates = b.env["candidates"].as_str().unwrap();
    assert_eq!(candidates, format!("1 · the owner model · rests on {his} · answer him on the brain, with no build news\n2 · always · rests on nothing · nothing: w2 goes on as it is"), "his post is the burst's, so the record adds none");
    let thought = format!("He is on the brain and I have been on builds. I answer his {his} first.");
    answer(&mut core, &d, picked(&his, &thought));
    let r = result(&core, t);
    assert_eq!((r["head"].clone(), r["on_topic"].clone(), r["rests_on"].clone(), r["thought"].clone()), (json!("Said"), json!(false), json!(his), json!(thought)));
    assert_eq!(r["signals"][0]["kind"], "correction");
    let x = &core.tickets.m.tickets[&t];
    let p = x.posts.last().unwrap();
    let id = core.tickets.m.id(t);
    assert_eq!((p.by.as_str(), p.text.clone(), p.inform.clone(), p.wakes().count()), (id.as_str(), Text::New(thought.clone()), vec!["w2".to_string()], 0));
    assert_eq!(p.into_turn, ["w2"], "its words ask into w2's turn, the turn it is about");
    let line = format!("{id}.p{}", p.n);
    // never into the running turn
    assert!(core.to_hand_over().is_empty() && channel(&mut core, &mut rx).is_none(), "nothing into the running turn");
    assert!(!arrived(&dir, &line) && kept(&core, &line).is_none(), "no `arrived` for it, no keep");
    // the turn's end: alone, whole, with no call (`told`)
    end_turn(&mut core, &dir, "On builds still.");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, tickets, _, _, into, rest) = channel(&mut core, &mut rx).expect("the thought, at the turn's end");
    assert_eq!((ids.clone(), into, rest.len()), (vec![line.clone()], false, 0));
    assert!(folds(&mut core).is_empty(), "no call");
    let (d, view) = tickets[0].clone();
    assert!(d.inform);
    let (content, _) = crate::render::delivery_as(&core.places(), &d, &view, false, true, false, false, None, true, crate::render::Hints::default()).unwrap();
    assert!(content.contains(&thought), "{content}");
    assert!(!content.contains("Answer it now") && !content.contains("reply tool"), "it asks for nothing: {content}");
    core.handed("w2", &ids, true, "channel");
    assert!(core.to_hand_over().is_empty(), "nothing more");
    (dir, base)
}

/// The same thought, once the turn his post began has ended: idle w2 is given it at once, alone and whole, with no
/// call (76; fresh-lead-3 N-1540 p18: the line steers the next turn; on the text before it waited for whatever woke w2
/// next, which might not come). The next post goes alone.
#[test]
fn a_thought_after_the_turn_ended_goes_at_once_alone_and_whole() {
    let (dir, _, mut core, mut rx) = coord("brain-after", "idle");
    let his = open(&mut core, "owner", &["w2"], "the brain shoud prbluy be tleling oyu getb akc to the owner topic");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("his post");
    core.handed("w2", &ids, true, "channel");
    working(&mut core);
    let t = pass(&mut core, &[&his]);
    let d = step(&mut core, &mut rx, t);
    answer(&mut core, &d, drifted(&his));
    end_turn(&mut core, &dir, "Build 36 is green.");
    let d = step(&mut core, &mut rx, t);
    let thought = format!("He is on the brain and I have been on builds. I answer his {his} first.");
    answer(&mut core, &d, picked(&his, &thought));
    let line = format!("{}.p{}", core.tickets.m.id(t), core.tickets.m.tickets[&t].posts.last().unwrap().n);
    assert_eq!(core.to_hand_over(), ["w2"], "its `free`, raised for it");
    let (ids, tickets, _, _, into, rest) = channel(&mut core, &mut rx).expect("the thought, at once");
    assert_eq!((ids.clone(), into, rest.len()), (vec![line.clone()], false, 0));
    assert!(folds(&mut core).is_empty(), "no call");
    let (content, _) = crate::render::delivery_as(&core.places(), &tickets[0].0, &tickets[0].1, false, true, false, false, None, true, crate::render::Hints::default()).unwrap();
    assert!(content.contains("I answer his"), "whole: {content}");
    core.handed("w2", &ids, true, "channel");
    end_turn(&mut core, &dir, "On it.");
    let w = open(&mut core, "w3", &["w2"], "is the second run green?");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("the next delivery");
    assert_eq!(ids, [w], "the thought once");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The thought given at the end of the turn his post began, in a turn that ends without a word on it: read there
/// (core5-carry, paper-research N-1397.p91), so a later post still raises `free` and is handed alone (consult-3 N-1444
/// p20.2). On the text before 76 it went into his post's turn; now it waits for that turn's end (fresh-lead-3 N-1540
/// p5).
#[test]
fn a_turn_that_ends_without_a_word_on_the_thought_leaves_free_to_come() {
    let (dir, _, mut core, mut rx) = coord("brain-left", "idle");
    let his = open(&mut core, "owner", &["w2"], "get back to my topic");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("his post");
    core.handed("w2", &ids, true, "channel");
    working(&mut core);
    let t = pass(&mut core, &[&his]);
    let d = step(&mut core, &mut rx, t);
    answer(&mut core, &d, drifted(&his));
    let d = step(&mut core, &mut rx, t);
    answer(&mut core, &d, picked(&his, &format!("I answer his {his} first.")));
    assert!(core.to_hand_over().is_empty(), "not into the running turn");
    end_turn(&mut core, &dir, "Back on the brain.");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, _, into, _) = channel(&mut core, &mut rx).expect("the thought, at the turn's end");
    assert!(!into);
    core.handed("w2", &ids, true, "channel");
    end_turn(&mut core, &dir, "On it.");
    let w = open(&mut core, "w3", &["w2"], "is the second run green?");
    assert_eq!(core.to_hand_over(), ["w2"], "free comes");
    let (ids, ..) = channel(&mut core, &mut rx).expect("w3's post");
    assert_eq!(ids, [w]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// His post and the brain's line at a working agent's turn's end (76; fresh-lead-3 N-1540 p18): w2 at work on his
/// post, a pass on it speaks; then his second post and w3's ask come. Nothing goes into the turn and the line arrives
/// nowhere. At the turn's end his second post goes first and the line after it, whole, w3's ask named at the foot, with
/// no call: his post never waits behind a model call, and the line, steering the next turn, is not cut to a first line
/// at the foot (`free`'s `told`; delivery.kind's `his ++ e.told`).
#[test]
fn his_post_goes_first_and_the_brains_line_whole_after_it_at_the_turns_end_with_no_call() {
    let (dir, _, mut core, mut rx) = coord("brain-his", "idle");
    let his = open(&mut core, "owner", &["w2"], "the brain shoud prbluy be tleling oyu getb akc to the owner topic");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("his post");
    core.handed("w2", &ids, true, "channel");
    working(&mut core);
    let line = said(&mut core, &mut rx, &his, &format!("He is on the brain and I have been on builds. I answer his {his} first."));
    let more = open(&mut core, "owner", &["w2"], "and the median, after");
    let ask = open(&mut core, "w3", &["w2"], "is the second run green?");
    assert!(core.to_hand_over().is_empty() && channel(&mut core, &mut rx).is_none(), "nothing into the running turn");
    assert!(!arrived(&dir, &line) && kept(&core, &line).is_none(), "the line arrives nowhere");
    assert_eq!(kept(&core, &more).as_deref(), Some("comes when its turn ends"));
    end_turn(&mut core, &dir, "On builds still.");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, _, into, rest) = channel(&mut core, &mut rx).expect("his post and the line");
    assert_eq!((ids, into, rest), (vec![more, line], false, vec![ask]), "his first, the line whole after, the ask at the foot");
    assert!(folds(&mut core).is_empty(), "no call");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The brain's line never goes through `fold` (76; fresh-lead-3 N-1540 p18; docket-core p20, p22): w2 at work on his
/// post, a pass on it speaks, and w3's post for w2 comes. At the turn's end one `fold` is offered w3's post alone. Its
/// Give hands the line first, whole, then w3's post; its Rest keeps w3's post with its words and hands the line, which
/// steers the next turn and does not wait on the Rest; its Failed hands w3's post, then the line, with no second call
/// (gemini-advisor N-1540 p23's text).
#[test]
fn the_brains_line_goes_whole_beside_folds_give_rest_and_failed_and_fold_never_orders_it() {
    for verdict in ["Give", "Rest", "Failed"] {
        let (dir, _, mut core, mut rx) = coord(&format!("brain-fold-{verdict}"), "idle");
        core.fold_by = None;
        let his = open(&mut core, "owner", &["w2"], "get back to my topic");
        assert_eq!(core.to_hand_over(), ["w2"]);
        let (ids, ..) = channel(&mut core, &mut rx).expect("his post");
        core.handed("w2", &ids, true, "channel");
        working(&mut core);
        let line = said(&mut core, &mut rx, &his, &format!("I answer his {his} first."));
        let p = open(&mut core, "w3", &["w2"], "the gate waits on it");
        core.raise_delivery();
        assert_eq!((kept(&core, &p).as_deref(), kept(&core, &line)), (Some("comes when its turn ends"), None), "{verdict}");
        end_turn(&mut core, &dir, "Back on the brain.");
        core.raise_delivery();
        let due = folds(&mut core);
        assert_eq!(due.len(), 1, "{verdict}: {due:?}");
        let job = core.fold_job(&due[0]).expect("a fold job");
        let n = job.fold.clone().unwrap();
        assert_eq!(n.ids, [p.clone()], "{verdict}: fold is offered w3's post alone");
        let head = match verdict {
            "Give" => crate::clerk::fold_ok(&json!({"head": "Give", "now": [p.clone()], "rest": []}), &n).unwrap(),
            "Rest" => crate::clerk::fold_ok(&json!({"head": "Rest", "why": "after the gate"}), &n).unwrap(),
            _ => crate::clerk::fold_fell(&n, "no model answered"),
        };
        let failed = (verdict == "Failed").then(|| String::from("no model answered"));
        core.fold_answered(&job, &due[0], head, None, failed, None);
        assert!(folds(&mut core).is_empty(), "{verdict}: no second call");
        assert_eq!(core.to_hand_over(), ["w2"], "{verdict}");
        let (ids, _, _, _, into, _) = channel(&mut core, &mut rx).expect("handed");
        let (want, kept_as) = match verdict {
            "Give" => (vec![line.clone(), p.clone()], None),
            "Rest" => (vec![line.clone()], Some("after the gate")),
            _ => (vec![p.clone(), line.clone()], None),
        };
        assert_eq!((ids, into, kept(&core, &p).as_deref()), (want, false, kept_as), "{verdict}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// An agent's post on a thread that stands at a post of the one its turn holds, which policy only informs it of,
/// waits for the turn's end, with no `arrived` and no keep, as in build 36 (proto-lead N-1446 p173); from 76 so does
/// a post that asks it (`Post.into_turn`, a program's `post … informing`), which arrived on the text before. The record's shape (proto-reader N-1446 p112): paper-research's turn holds N-1104.p586, it opens N-1445 under
/// N-1104 for paper-research-brain, who posts there ("talks only this, here. I keep the builds on N-1104"); and E2's,
/// a thread tied at his post (`tied_at`, `coord msg --under`; consult-3 p159).
#[test]
fn an_agents_post_on_a_thread_under_the_turns_waits_for_the_turns_end() {
    for tied in [false, true] {
        let (dir, _, mut core, mut rx) = coord(if tied { "brain-tied" } else { "brain-under" }, "idle");
        let his = open(&mut core, "owner", &["w2"], "the brain shoud prbluy be tleling oyu getb akc to the owner topic");
        assert_eq!(core.to_hand_over(), ["w2"]);
        let (ids, ..) = channel(&mut core, &mut rx).expect("his post");
        core.handed("w2", &ids, true, "channel");
        working(&mut core);
        let t = t_of(&his);
        let mut form = json!({"to": "w3", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "w2"}, "follows": null});
        if tied {
            form["tied_at"] = json!(his);
        }
        let under = (!tied).then_some(t);
        core.ticket_input(Input::Open { kind: "thread".into(), under, opener: "w2".into(), form, text: Some(Text::New("talks only this, here. I keep the builds on the first".into())), names: vec!["w3".into()], told: vec![] }).unwrap();
        let sub = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        assert_eq!(core.tickets.m.outline_parent(sub).map(|(p, _)| p), Some(t), "it stands at his thread (tied: {tied})");
        // w3 answers him there, as paper-research-brain did; policy informs the thread's others of it (notify.eflint)
        core.ticket_input(Input::Post { t: sub, by: "w3".into(), text: Text::New("The brain's mechanism, first cut.".into()), names: vec!["owner".into()], told: vec![], inform: vec!["w2".into()], answers: None, kind: None }).unwrap();
        let p = core.tickets.m.tickets[&sub].posts.last().unwrap();
        assert!(p.inform.contains(&"w2".to_string()) && p.into_turn.is_empty(), "{p:?}");
        let id = format!("{}.p{}", core.tickets.m.id(sub), p.n);
        core.raise_delivery();
        assert!(!core.to_hand_over().contains(&"w2".to_string()), "not put into w2's turn (tied: {tied})");
        let item = core.tickets.m.waiting("w2").into_iter().find(|i| i["id"] == json!(id)).expect("it waits");
        assert_eq!((item["inform"].clone(), item["about_turn"].clone(), item["into_turn"].clone()), (json!(true), json!(true), json!(false)));
        assert_eq!(kept(&core, &id), None, "no keep: it never arrived (tied: {tied})");
        let log: Vec<Value> = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        assert!(log.iter().any(|l| l["ev"] == "post" && l["t"] == json!(sub) && l["by"] == "w3"), "the record holds w3's post");
        assert!(!log.iter().any(|l| l["ev"] == "event" && l["name"] == docket_machine::syntax::ARRIVED && l["value"]["item"] == json!(id)), "no `arrived` for it (tied: {tied})");
        assert!(channel(&mut core, &mut rx).is_none(), "nothing handed to w2");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// On his topic with no signal and nothing proposed: Quiet, with no second call and nothing said. The selector's
/// Nothing: Quiet too, its scores kept. A call that failed: Failed, nothing said. The pick of the `unanswered by it`
/// candidate on his post, which w2 answered while the call ran: Quiet, its scores kept, though its act echoes his
/// quoted words inexactly; a pick of `his earlier words` resting on the same post would still be said.
#[test]
fn quiet_nothing_and_a_failed_call_say_nothing() {
    let (dir, _, mut core, mut rx) = coord("brain-quiet", "working");
    policy(&mut core, &dir, "+unanswered-within(43200).\n");
    his_into(&dir, &mut core);
    let his = open(&mut core, "owner", &["w2"], "yes the 15 s hold");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("his post, into its turn");
    core.handed("w2", &ids, true, "channel");
    // on his topic: one call
    let t = pass(&mut core, &[&his]);
    let d = step(&mut core, &mut rx, t);
    answer(&mut core, &d, json!({"head": "Read", "topic": "the hold", "on_topic": true, "signals": [], "proposals": []}));
    assert_eq!(result(&core, t), json!({"head": "Quiet", "topic": "the hold", "on_topic": true, "signals": [], "scores": []}));
    assert_eq!(core.tickets.m.tickets[&t].posts.iter().filter(|p| p.head.is_none()).count(), 0, "nothing said");
    core.hand_clerk();
    assert!(std::iter::from_fn(|| rx.try_recv().ok()).all(|j| !matches!(j, Job::Deliver { delivery, .. } if delivery.t == t)), "no second call");
    // Nothing ranked first
    let t = pass(&mut core, &[&his]);
    let d = step(&mut core, &mut rx, t);
    answer(&mut core, &d, drifted(&his));
    let d = step(&mut core, &mut rx, t);
    let scores = json!([{"act": "nothing", "newest": 2, "rules": 2, "owed": 1, "sees": 0, "cost": 2, "rank": 1}]);
    answer(&mut core, &d, json!({"head": "Nothing", "scores": scores}));
    let r = result(&core, t);
    assert_eq!((r["head"].clone(), r["scores"].clone()), (json!("Quiet"), scores));
    assert_eq!(core.tickets.m.tickets[&t].posts.iter().filter(|p| p.head.is_none()).count(), 0, "nothing said");
    // the call failed
    let t = pass(&mut core, &[&his]);
    let d = step(&mut core, &mut rx, t);
    let (job, _) = job(&mut core, &d);
    core.brain_answered(&job, &d, brain_failed("no model answered"), None, Some("no model answered".into()));
    assert_eq!(result(&core, t), json!({"head": "Failed", "why": "no model answered"}));
    assert_eq!(core.tickets.m.tickets[&t].posts.iter().filter(|p| p.head.is_none()).count(), 0, "nothing said");
    assert!(core.to_hand_over().is_empty(), "nothing for w2");
    // the selector's pick of its `unanswered by it` candidate on his post, which w2 answered while the call ran (the
    // read before the act; I-1455 (B)): when the pick comes back the ledger holds w2's answer, so nothing is said. The
    // owner model named that post in EARLIER too, so `his earlier words` rests on it as well: a pick of that one is not
    // stale (consult-3's N-1479 p136 point 4)
    let asked = open(&mut core, "owner", &["w2"], "what does the brain read?");
    let burst = open(&mut core, "owner", &["w2"], "and the 15 s hold?");
    let t = pass(&mut core, &[&burst]);
    let d = step(&mut core, &mut rx, t);
    let mut read = drifted(&burst);
    read["earlier"] = json!([asked]);
    answer(&mut core, &d, read);
    let d = step(&mut core, &mut rx, t);
    let (elect, b) = self::job(&mut core, &d);
    let act = format!("answer his {asked}, which w2 has not answered: \"what does the brain read?\"");
    let candidates = b.env["candidates"].as_str().unwrap().to_string();
    assert!(candidates.lines().any(|l| l.ends_with(&format!(" · unanswered by it · rests on {asked} · {act}"))), "{candidates}");
    let words = candidates.lines().find_map(|l| l.split_once(&format!(" · his earlier words · rests on {asked} · ")).map(|(_, a)| a.to_string())).unwrap_or_else(|| panic!("{candidates}"));
    handed_on(&mut core, "w2", &asked);
    core.ticket_input(Input::Post { t: t_of(&asked), by: "w2".into(), text: Text::New("The brain reads the record alone.".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    assert!(core.answered_since(&b, &json!({"act": act, "rests_on": asked})).is_some(), "its own candidate is stale");
    // the selector's echo of his quoted words with its quote marks and an ellipsis changed: coordd's own part of the act
    // is the same, so it is that candidate, stale too (proto-lead N-1479 p187)
    let echo = format!("answer his {asked}, which w2 has not answered: “what does the brain read…”");
    assert!(core.answered_since(&b, &json!({"act": echo, "rests_on": asked})).is_some(), "an inexact echo of it is stale too");
    assert_eq!(core.answered_since(&b, &json!({"act": words, "rests_on": asked})), None, "his earlier words on it are not");
    let scores = json!([{"act": act, "newest": 1, "rules": 2, "owed": 2, "sees": 1, "cost": 1, "rank": 1}]);
    let head = brain_ok(&json!({"head": "Pick", "act": act, "rests_on": asked, "against": "", "thought": "I answer what the brain reads first.", "scores": scores}), &b).expect("in shape");
    core.brain_answered(&elect, &d, head, None, None);
    let r = result(&core, t);
    assert_eq!((r["head"].clone(), r["scores"].clone()), (json!("Quiet"), scores));
    assert_eq!(core.tickets.m.tickets[&t].posts.iter().filter(|p| p.head.is_none()).count(), 0, "nothing said");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The report, from the record alone: BURST his posts the pass is on, whole; HIS his last three posts to w2 before
/// them; OPEN those w2 has not answered (the ledger, `Machine::asked`: w2's answer written once it was handed his post
/// answers it; its post that only passes his on to w3, typing only `@w3`, does not, consult-3's N-1479 p84 case), from
/// the policy's `unanswered-within` before the burst (12 h here: his post of 13 hours before is not one, proto-reader
/// N-1446 p98, proto-lead p138), which the selector also gets as candidates, `unanswered by it`; ITS w2's own posts,
/// first lines; DOING, w2's turn as the record has it: the one it was handed those two posts of his in, running. With
/// no `unanswered-within` in the policy, OPEN reads none, and says why.
#[test]
fn the_report_is_his_posts_its_open_asks_and_its_own_posts() {
    let (dir, _, mut core, mut rx) = coord("brain-report", "idle");
    policy(&mut core, &dir, "+unanswered-within(43200).\n");
    let stale = open(&mut core, "owner", &["w2"], "take from the criticism");
    let p = core.tickets.m.tickets.edit().get_mut(&t_of(&stale)).unwrap().posts.iter_mut().find(|p| p.n == 1).unwrap();
    p.at = crate::core::at_secs(crate::transcript::epoch_ms(&p.at).unwrap() / 1000 - 13 * 3600);
    let answered = open(&mut core, "owner", &["w2"], "is build 35 green?");
    handed_on(&mut core, "w2", &answered);
    core.ticket_input(Input::Post { t: t_of(&answered), by: "w2".into(), text: Text::New("Build 35 is green.\nThe log is attached.".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let passed = open(&mut core, "owner", &["w2"], "can the accounts tab show every duration as d h m?");
    handed_on(&mut core, "w2", &passed);
    core.ticket_input(Input::Post { t: t_of(&passed), by: "w2".into(), text: Text::New("@w3 please make every duration d h m".into()), names: vec!["w3".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let asked = open(&mut core, "owner", &["w2"], "what does the brain read?");
    let other = open(&mut core, "owner", &["w3"], "not for w2");
    let burst = open(&mut core, "owner", &["w2"], "the brain shoud prbluy be tleling oyu getb akc to the owner topic");
    let t = pass(&mut core, &[&burst]);
    let d = step(&mut core, &mut rx, t);
    let (_, b) = job(&mut core, &d);
    let line = |id: &str| format!("{id} · {} · ", core.ticket_head(t_of(id)));
    let env = &b.env;
    assert_eq!(env["agent"], "w2");
    assert!(env["burst"].as_str().unwrap().starts_with(&line(&burst)) && env["burst"].as_str().unwrap().ends_with(" · the brain shoud prbluy be tleling oyu getb akc to the owner topic"), "{}", env["burst"]);
    let his: Vec<&str> = env["his"].as_str().unwrap().lines().collect();
    assert_eq!(his.len(), 3, "{his:?}");
    assert!(his[0].starts_with(&line(&answered)) && his[1].starts_with(&line(&passed)) && his[2].starts_with(&line(&asked)), "{his:?}");
    assert!(!env["his"].as_str().unwrap().contains(&other), "his post to another is not its");
    let open_asks = env["open"].as_str().unwrap();
    let open_lines: Vec<&str> = open_asks.lines().collect();
    assert_eq!(open_lines.len(), 2, "{open_asks}");
    assert!(open_lines[0].starts_with(&line(&passed)) && open_lines[1].starts_with(&line(&asked)), "passed on is not answered: {open_asks}");
    assert!(!open_asks.contains(&answered) && !open_asks.contains(&stale), "{open_asks}");
    assert_eq!(env["within"], "from the 12 hours before these");
    let its = env["its"].as_str().unwrap();
    let its: Vec<&str> = its.lines().collect();
    assert!(its.len() == 2 && its[0].ends_with(" · Build 35 is green.") && its[1].ends_with(" · @w3 please make every duration d h m"), "its first lines: {its:?}");
    let doing = env["doing"].as_str().unwrap();
    let given = |a: &str, b: &str| doing.starts_with(&format!("the turn given {a} + {b} at "));
    assert!((given(&answered, &passed) || given(&passed, &answered)) && doing.ends_with(", running"), "{doing}");
    assert!(b.ids.contains(&burst) && b.ids.contains(&passed) && b.ids.contains(&asked) && !b.ids.contains(&other), "{:?}", b.ids);
    // the selector: his post in OPEN is a candidate, `unanswered by it`, with no model
    answer(&mut core, &d, json!({"head": "Read", "topic": "the brain", "on_topic": false, "signals": [], "proposals": []}));
    let d = step(&mut core, &mut rx, t);
    let (_, b) = job(&mut core, &d);
    assert_eq!(
        b.env["candidates"],
        format!("1 · unanswered by it · rests on {passed} · answer his {passed}, which w2 has not answered: \"can the accounts tab show every duration as d h m?\"\n2 · unanswered by it · rests on {asked} · answer his {asked}, which w2 has not answered: \"what does the brain read?\"\n3 · always · rests on nothing · nothing: w2 goes on as it is")
    );
    assert_eq!((b.env["on_topic"].clone(), b.env["topic"].clone()), (json!("no"), json!("the brain")));
    assert!(kept(&core, &asked).is_none());
    let _ = std::fs::remove_dir_all(&dir);
    // no `unanswered-within`: none read, and the report says why
    let (dir, _, mut core, mut rx) = coord("brain-report-none", "idle");
    open(&mut core, "owner", &["w2"], "what does the brain read?");
    let burst = open(&mut core, "owner", &["w2"], "and the 15 s hold?");
    let t = pass(&mut core, &[&burst]);
    let d = step(&mut core, &mut rx, t);
    let (_, b) = job(&mut core, &d);
    assert_eq!((b.env["open"].clone(), b.env["within"].clone()), (json!("(none)"), json!("none read (the policy gives no unanswered-within)")));
    assert_eq!((b.env["earlier"].clone(), b.env["earlier_within"].clone()), (json!("(none)"), json!("none read (the policy gives no earlier-within)")));
    let _ = std::fs::remove_dir_all(&dir);
}

/// AGENT handed what is due to it on the ticket of post ID, as coordd's channel reports a giving.
fn handed_on(core: &mut Core, agent: &str, id: &str) {
    let t = t_of(id);
    let d = core.tickets.m.due(agent).into_iter().find(|d| d.t == t).unwrap_or_else(|| panic!("{agent} is owed nothing on {id}"));
    core.ticket_input(Input::Given { t, to: agent.into(), delivery: d.id, upto: d.upto, skipped: Default::default(), place: None }).unwrap();
}

/// Post ID's time set S seconds before now, as if he had written it then.
fn written_ago(core: &mut Core, id: &str, s: i64) {
    let (t, n) = docket_machine::post_id(id).unwrap();
    let p = core.tickets.m.tickets.edit().get_mut(&t).unwrap().posts.iter_mut().find(|p| p.n == n).unwrap();
    p.at = crate::core::at_secs(crate::transcript::epoch_ms(&crate::core::now()).unwrap() / 1000 - s);
}

/// His post on ticket T, as `coord post` makes it, typed to NAMES, and routed to them as the watcher's `route` routes
/// it when base.kind's owner_post holds it until then (on a thread an agent opened for NAMES it goes straight). Its id.
fn his_post(core: &mut Core, t: u64, names: &[&str], text: &str) -> String {
    core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New(text.into()), names: names.iter().map(|n| n.to_string()).collect(), told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let n = core.tickets.m.tickets[&t].posts.iter().filter(|p| p.by == "owner").map(|p| p.n).max().unwrap();
    let id = format!("{}.p{n}", core.tickets.m.id(t));
    let shown = |core: &Core| core.tickets.m.tickets[&t].posts.iter().any(|p| p.n == n && p.visible());
    if !shown(core) {
        let o = *core.tickets.m.tickets.iter().rev().find(|(_, x)| x.kind == "owner_post" && matches!(x.status, Status::Awaits { .. })).expect("its handler, at its route").0;
        core.ticket_answer(docket_machine::WATCHER, o, json!({"head": "To", "agents": names}), Text::New(String::new())).unwrap();
    }
    assert!(shown(core), "{id} routed");
    id
}

/// His earlier words before an answer (candidate 2, proto-mind N-1446 p450; I-1455). With `+earlier-within(604800).` the
/// owner model reads EARLIER, an index of his posts from the 7 days before the burst, each by its first line: those on the
/// burst's thread, on the thread it hangs from and on the threads hanging from it, whomever they went to, and those to
/// w2 anywhere; not those HIS shows, nor his post to w3 elsewhere, nor his post of 8 days before. It names two of them
/// (an id of the burst's and one the report did not hold are dropped), so a pass that would have been Quiet asks the
/// selector, whose candidate is `his earlier words`; the pick's post gives w2 their words as the record holds them,
/// byte for byte, under the thought, which does not quote him. With `+earlier-lines(2).` the index lists the newest two.
#[test]
fn his_earlier_words_are_an_index_and_a_pick_gives_them_as_the_record_holds_them() {
    let (dir, _, mut core, mut rx) = coord("brain-earlier", "idle");
    policy(&mut core, &dir, "+earlier-within(604800).\n");
    let day = 86400;
    let old = open(&mut core, "owner", &["w2"], "keep the one send path");
    written_ago(&mut core, &old, 8 * day);
    let rule = open(&mut core, "owner", &["w2"], "one message box, not two. reuse the one we have\nnot a second copy beside it");
    written_ago(&mut core, &rule, 3 * day);
    let parent = open(&mut core, "owner", &["w3"], "the box sends a list as a list");
    written_ago(&mut core, &parent, 2 * day);
    let form = json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null});
    let under = |core: &mut Core, t: u64, names: &[&str], text: &str| -> String {
        core.ticket_input(Input::Open { kind: "thread".into(), under: Some(t), opener: "owner".into(), form: form.clone(), text: Some(Text::New(text.into())), names: names.iter().map(|n| n.to_string()).collect(), told: vec![] }).unwrap();
        format!("{}.p1", core.tickets.m.id(*core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0))
    };
    let first = under(&mut core, t_of(&parent), &["w2"], "the send box: what is left?");
    written_ago(&mut core, &first, day);
    let t = t_of(&first);
    let child = under(&mut core, t, &["w3"], "no second copy of the box");
    written_ago(&mut core, &child, 20 * 3600);
    let spell = his_post(&mut core, t, &["w3"], "and the spellcheck in it");
    written_ago(&mut core, &spell, 10 * 3600);
    let elsewhere = open(&mut core, "owner", &["w3"], "the runner's rows");
    written_ago(&mut core, &elsewhere, 3600 * 5);
    let mut recent = Vec::new();
    for (i, w) in ["a", "b", "c"].into_iter().enumerate() {
        let id = open(&mut core, "owner", &["w2"], w);
        written_ago(&mut core, &id, 3600 - 600 * i as i64);
        recent.push(id);
    }
    let burst = his_post(&mut core, t, &["w2"], "is the box done?");
    let pass_t = pass(&mut core, &[&burst]);
    let d = step(&mut core, &mut rx, pass_t);
    let (_, b) = job(&mut core, &d);
    let ids_of = |lines: &str| -> Vec<String> { lines.lines().map(|l| l.split(" · ").next().unwrap().to_string()).collect() };
    assert_eq!(ids_of(b.env["his"].as_str().unwrap()), recent, "HIS: his last three to w2");
    let earlier = b.env["earlier"].as_str().unwrap();
    assert_eq!(ids_of(earlier), [rule.clone(), parent.clone(), first.clone(), child.clone(), spell.clone()], "{earlier}");
    let at = |core: &Core, id: &str| -> String {
        let (t, n) = docket_machine::post_id(id).unwrap();
        super::day_time(&core.tickets.m.tickets[&t].posts.iter().find(|p| p.n == n).unwrap().at)
    };
    assert_eq!(earlier.lines().next().unwrap(), format!("{rule} · {} · {} · one message box, not two. reuse the one we have", core.ticket_head(t_of(&rule)), at(&core, &rule)), "each by its first line, with its day");
    assert_eq!(b.env["earlier_within"], "from the 7 days before these");
    assert!([&rule, &spell].iter().all(|id| b.ids.contains(id)), "{:?}", b.ids);
    // the owner model names two; on his topic with no signal and nothing proposed, the selector is still asked
    let got = json!({"head": "Read", "topic": "the send box", "on_topic": true, "signals": [], "proposals": [], "earlier": [&rule, &burst, "N-999.p1", &spell, &rule]});
    answer(&mut core, &d, got);
    let d = step(&mut core, &mut rx, pass_t);
    assert_eq!(call(&d), "elect", "his earlier words are something to weigh");
    let What::Step { args, .. } = &d.what else { panic!() };
    assert_eq!(crate::render::resolved(args)["earlier"], json!([&rule, &spell]), "the burst's and an unknown id dropped, each once");
    let (_, b) = job(&mut core, &d);
    assert_eq!(b.env["candidates"], format!("1 · his earlier words · rests on {spell} · give w2 his earlier words on this before it answers: {rule} \"one message box, not two. reuse the one we have\" · {spell} \"and the spellcheck in it\"\n2 · always · rests on nothing · nothing: w2 goes on as it is"));
    let thought = format!("He asked for one box before; I keep to the one we have ({rule}).");
    let act = "give w2 his earlier words on this before it answers";
    answer(&mut core, &d, json!({"head": "Pick", "act": act, "rests_on": &spell, "against": "he may only want the state", "thought": &thought,
        "scores": [{"act": act, "newest": 2, "rules": 2, "owed": 1, "sees": 2, "cost": 2, "rank": 1}, {"act": "nothing", "newest": 1, "rules": 2, "owed": 0, "sees": 0, "cost": 2, "rank": 2}]}));
    // the post: the thought, then his words as the record holds them
    let words = |id: &str| -> String {
        let (t, n) = docket_machine::post_id(id).unwrap();
        crate::label::read(&core.tickets.m.tickets[&t].posts.iter().find(|p| p.n == n).unwrap().text).unwrap()
    };
    assert_eq!(words(&rule), "one message box, not two. reuse the one we have\nnot a second copy beside it");
    let want = format!("{thought}\n\nHis earlier words on this, as he wrote them:\n{rule} ({} UTC): \"{}\"\n{spell} ({} UTC): \"{}\"", at(&core, &rule), words(&rule), at(&core, &spell), words(&spell));
    let p = core.tickets.m.tickets[&pass_t].posts.iter().filter(|p| p.head.is_none()).last().unwrap();
    assert_eq!((p.text.clone(), p.inform.clone(), p.into_turn.clone()), (Text::New(want), vec!["w2".to_string()], vec!["w2".to_string()]));
    let r = result(&core, pass_t);
    assert_eq!((r["head"].clone(), r["thought"].clone()), (json!("Said"), json!(thought)), "the result's thought is the model's alone");
    // the policy's `earlier-lines`: the index lists the newest two
    policy(&mut core, &dir, "+earlier-within(604800).\n+earlier-lines(2).\n");
    core.lapse_times = Default::default();
    let again = pass(&mut core, &[&burst]);
    let d = step(&mut core, &mut rx, again);
    assert_eq!(ids_of(job(&mut core, &d).1.env["earlier"].as_str().unwrap()), [child, spell]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// His words only (proto-lead N-1446 p552; proto-reader p105): w3 opens a thread Named for w2, as paper-research opened
/// N-1445 for paper-research-brain, and thread.kind before ls-opener posted the opener's words again as his #2
/// (research's record 103219; E1's `Run::before_opener`). They are not his words (`Machine::his_words`), so they are in
/// none of HIS, OPEN and EARLIER, the owner model cannot name them, and no pick gives them as his. A post of his with
/// those very words, routed to w2, stands for #2, as in E1's own coordd test (ticket/mod.rs
/// `the_holder_leaves_out_an_agents_opener_posted_as_his_…`): `Run::before_opener` is docket-machine's test module.
#[test]
fn an_agents_opener_posted_as_his_is_in_none_of_his_open_and_earlier() {
    let ids_of = |lines: &str| -> Vec<String> { lines.lines().map(|l| l.split(" · ").next().unwrap().to_string()).collect() };
    let words = "w3 · **The send box: this talk now has its own thread.** @w2 carries it, here.";
    let named = |core: &mut Core| -> (u64, String) {
        let form = json!({"to": "w2", "typed": ["w2"], "names": ["w2"], "text": words, "addressed": {"head": "Named"}, "label": "The send box"});
        core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "w3".into(), form, text: Some(Text::New(words.into())), names: vec!["w2".into()], told: vec![] }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        let again = his_post(core, t, &["w2"], words);
        assert_eq!((again.clone(), core.tickets.m.his_words(t, 2)), (format!("{}.p2", core.tickets.m.id(t)), false), "#2: w3's opener, posted as his");
        (t, again)
    };
    // HIS and OPEN: his #3 is the burst; #2, to w2 and unanswered by it, is in neither
    let (dir, _, mut core, mut rx) = coord("brain-opener", "idle");
    policy(&mut core, &dir, "+unanswered-within(43200).\n+earlier-within(604800).\n");
    let (t, again) = named(&mut core);
    let burst = his_post(&mut core, t, &["w2"], "what is left of the box?");
    let pass_t = pass(&mut core, &[&burst]);
    let d = step(&mut core, &mut rx, pass_t);
    let (_, b) = job(&mut core, &d);
    assert_eq!(ids_of(b.env["burst"].as_str().unwrap()), [burst.clone()]);
    assert_eq!((b.env["his"].clone(), b.env["open"].clone(), b.env["earlier"].clone()), (json!("(none)"), json!("(none)"), json!("(none)")), "{again} is not his");
    assert!(!b.ids.contains(&again), "{:?}", b.ids);
    assert_eq!((core.his_post(&again), core.his_post(&burst).is_some()), (None, true), "HIS's words come from his posts only");
    let _ = std::fs::remove_dir_all(&dir);
    // EARLIER: #2 13 hours old (before OPEN's 12) and three posts of his to w2 since (HIS's three), so only EARLIER could
    // show it; the owner model names it, the check drops it as an id the report did not hold, and the pass is Quiet
    let (dir, _, mut core, mut rx) = coord("brain-opener-earlier", "idle");
    policy(&mut core, &dir, "+unanswered-within(43200).\n+earlier-within(604800).\n");
    let (t, again) = named(&mut core);
    written_ago(&mut core, &again, 13 * 3600);
    let mut recent = Vec::new();
    for (i, w) in ["a", "b", "c"].into_iter().enumerate() {
        let id = open(&mut core, "owner", &["w2"], w);
        written_ago(&mut core, &id, 3600 - 600 * i as i64);
        recent.push(id);
    }
    let burst = his_post(&mut core, t, &["w2"], "and the spellcheck?");
    let pass_t = pass(&mut core, &[&burst]);
    let d = step(&mut core, &mut rx, pass_t);
    let (_, b) = job(&mut core, &d);
    assert_eq!(ids_of(b.env["his"].as_str().unwrap()), recent, "HIS: his last three to w2");
    assert_eq!(b.env["earlier"], "(none)", "{again} is not his");
    answer(&mut core, &d, json!({"head": "Read", "topic": "the send box", "on_topic": true, "signals": [], "proposals": [], "earlier": [&again]}));
    assert_eq!(result(&core, pass_t)["head"], "Quiet", "nothing named that the report held: no selector, no words of his");
    let _ = std::fs::remove_dir_all(&dir);
}

/// DOING, w2's turn as the record has it (proto-reader N-1446 p98): while one runs, what it was given and when, and
/// his post that went into it; once a turn has ended and nothing is given since, that last turn, what it was given
/// and when it ended.
#[test]
fn doing_is_the_turn_the_record_has() {
    let doing = |core: &mut Core, rx: &mut UnboundedReceiver<Job>, his: &str| -> String {
        let t = pass(core, &[his]);
        let d = step(core, rx, t);
        job(core, &d).1.env["doing"].as_str().unwrap().to_string()
    };
    // w3's post begins a turn, and his goes into it
    let (dir, _, mut core, mut rx) = coord("brain-doing", "idle");
    his_into(&dir, &mut core);
    let w = open(&mut core, "w3", &["w2"], "is the second run green?");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("w3's post");
    core.handed("w2", &ids, true, "channel");
    working(&mut core);
    let his = open(&mut core, "owner", &["w2"], "and the brain?");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, _, _, _, into, _) = channel(&mut core, &mut rx).expect("his post");
    assert_eq!((ids.clone(), into), (vec![his.clone()], true));
    core.handed("w2", &ids, true, "channel");
    let s = doing(&mut core, &mut rx, &his);
    assert!(s.starts_with(&format!("the turn given {w} at ")) && s.contains(&format!(", running; {his} went into it at ")), "{s}");
    let _ = std::fs::remove_dir_all(&dir);
    // the turn ended, his post not handed yet
    let (dir, _, mut core, mut rx) = coord("brain-done", "idle");
    let w = open(&mut core, "w3", &["w2"], "is the second run green?");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("w3's post");
    core.handed("w2", &ids, true, "channel");
    working(&mut core);
    end_turn(&mut core, &dir, "Run 2 is green.");
    let his = open(&mut core, "owner", &["w2"], "and the brain?");
    let s = doing(&mut core, &mut rx, &his);
    assert!(s.starts_with(&format!("no turn running; its last, given {w} at ")) && s.contains(", ended at "), "{s}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The check on the calls' answers (`brain_ok`): what rests on nothing the report held is dropped; a signal of no
/// known kind, or on a post outside the burst, or twice, is left out; at most three proposals; scores held to 0..2;
/// a form without what it must have is out of shape, and the pass says nothing; the base step's Next on nothing the
/// report held is Same, and its stop on nothing the report held is left out.
#[test]
fn the_check_drops_what_rests_on_nothing_the_report_held() {
    let b = Brain { agent: "w2".into(), call: ATTENTION.into(), env: json!({}), burst: vec!["N-9.p4".into()], open: Vec::new(), ids: vec!["N-9.p4".into(), "N-7.p2".into()] };
    // his p590 and p591 on N-1104 (18:24:50, 18:24:59): the record's one literal repeat, and a due diligence
    let tonight = Brain { burst: vec!["N-1104.p590".into(), "N-1104.p591".into()], ids: vec!["N-1104.p590".into(), "N-1104.p591".into()], ..b.clone() };
    let got = json!({"head": "Read", "topic": "his two messages, ordered and split", "on_topic": true,
        "signals": [{"kind": "repeat", "post": "N-1104.p590", "words": "see 2 messgaes. should have been roder / splti rpetly"},
                    {"kind": "diligence", "post": "N-1104.p591", "words": "read whats relvant from ealrier chats"}], "proposals": []});
    let mut want = got.clone();
    want["earlier"] = json!([]);
    assert_eq!(brain_ok(&got, &tonight), Some(want), "both kept, his words as he typed them");
    let got = json!({"head": "Read", "topic": " the brain ", "on_topic": false,
        "signals": [{"kind": "correction", "post": "N-9.p4", "words": "not what i meant"}, {"kind": "correction", "post": "N-9.p4", "words": "again"},
                    {"kind": "anger", "post": "N-9.p4", "words": "x"}, {"kind": "repeat", "post": "N-7.p2", "words": "as i said"}, {"kind": "shallow", "post": "N-9.p4", "words": ""}],
        "proposals": [{"act": "a", "rests_on": "N-9.p4"}, {"act": "b", "rests_on": "N-1.p1"}, {"act": "c", "rests_on": "N-7.p2"}, {"act": "d", "rests_on": "N-9.p4"}, {"act": "e", "rests_on": "N-9.p4"}],
        "earlier": ["N-9.p4", "N-1.p1", " N-7.p2 ", "N-7.p2", 3]});
    assert_eq!(brain_ok(&got, &b), Some(json!({"head": "Read", "topic": "the brain", "on_topic": false,
        "signals": [{"kind": "correction", "post": "N-9.p4", "words": "not what i meant"}],
        "proposals": [{"act": "a", "rests_on": "N-9.p4"}, {"act": "c", "rests_on": "N-7.p2"}, {"act": "d", "rests_on": "N-9.p4"}],
        "earlier": ["N-7.p2"]})), "EARLIER: the burst's id, one the report did not hold, a second time and a number are dropped");
    let many = Brain { ids: (1..=7).map(|n| format!("N-{n}.p1")).collect(), ..b.clone() };
    let got = json!({"head": "Read", "topic": "t", "on_topic": true, "earlier": (1..=7).map(|n| format!("N-{n}.p1")).collect::<Vec<_>>()});
    assert_eq!(brain_ok(&got, &many).unwrap()["earlier"], json!(["N-1.p1", "N-2.p1", "N-3.p1", "N-4.p1", "N-5.p1"]), "at most five");
    assert_eq!(brain_ok(&json!({"head": "Read", "topic": "", "on_topic": true}), &b), None, "no topic");
    assert_eq!(brain_ok(&json!({"head": "Read", "topic": "t", "on_topic": "yes"}), &b), None, "on_topic not a Bool");
    assert_eq!(brain_ok(&json!({"head": "Pick", "act": "a", "rests_on": "N-9.p4", "thought": "t"}), &b), None, "the owner model does not pick");
    let e = Brain { call: ELECTION.into(), ..b };
    let pick = |on: &str, thought: &str| json!({"head": "Pick", "act": "a", "rests_on": on, "thought": thought, "scores": [{"act": "a", "newest": 3, "rules": -1, "owed": 1, "sees": 2, "cost": 9, "rank": -2}, {"newest": 1}]});
    assert_eq!(brain_ok(&pick("N-7.p2", "I answer N-7.p2 first."), &e), Some(json!({"head": "Pick", "act": "a", "rests_on": "N-7.p2", "against": "", "thought": "I answer N-7.p2 first.", "scores": [{"act": "a", "newest": 2, "rules": 0, "owed": 1, "sees": 2, "cost": 2, "rank": 0}]})));
    assert_eq!(brain_ok(&pick("N-1.p1", "I answer N-1.p1 first."), &e), None, "a pick resting on an id the report did not hold");
    assert_eq!(brain_ok(&pick("N-7.p2", " "), &e), None, "a pick with no thought");
    assert_eq!(brain_ok(&json!({"head": "Nothing"}), &e), Some(json!({"head": "Nothing", "scores": []})));
    assert_eq!(brain_ok(&json!({"head": "Read", "topic": "t", "on_topic": true}), &e), None, "the selector does not read");
    // the base step (`outlook`): Next on an id of the report, its thought's first line cut at THOUGHT_CHARS and its notes
    // at NOTES_LINES; Next on an id the report did not hold, or with no thought, is Same with the same perspectives and
    // notes; Same as it is; without its notes, or a perspective, it is out of shape
    let o = Brain { call: OUTLOOK.into(), ..e };
    let notes = |k: usize| (1..=k).map(|n| format!("note {n}")).collect::<Vec<_>>().join("\n");
    let with = |head: &str, more: Value| {
        let mut v = json!({"head": head, "goal": "g", "critic": "c", "memory": "m", "attention": "a", "notes": notes(12)});
        more.as_object().unwrap().iter().for_each(|(k, x)| v[k] = x.clone());
        v
    };
    let same = json!({"head": "Same", "goal": "g", "critic": "c", "memory": "m", "attention": "a", "notes": notes(10)});
    let long = format!("I owe N-7.p2 its answer. {}\nsecond line", "and then some more ".repeat(12));
    let got = brain_ok(&with("Next", json!({"act": "answer N-7.p2", "rests_on": "N-7.p2", "thought": long})), &o).expect("in shape");
    assert_eq!((got["head"].clone(), got["act"].clone(), got["rests_on"].clone(), got["notes"].clone()), (json!("Next"), json!("answer N-7.p2"), json!("N-7.p2"), json!(notes(10))));
    let thought = got["thought"].as_str().unwrap();
    assert!(thought.starts_with("I owe N-7.p2 its answer.") && thought.chars().count() <= crate::clerk::THOUGHT_CHARS + 1 && !thought.contains("second"), "{thought}");
    assert_eq!(brain_ok(&with("Next", json!({"act": "a", "rests_on": "N-1.p1", "thought": "t"})), &o), Some(same.clone()), "on an id the report did not hold");
    assert_eq!(brain_ok(&with("Next", json!({"act": "a", "rests_on": "N-7.p2", "thought": " "})), &o), Some(same.clone()), "with no thought");
    // its stop, either way, as consolidation's lines: kept when it rests on the report, else left out (inhibition)
    let stop = "I posted N-7.p2 twice, unasked. I stop.";
    assert_eq!(brain_ok(&with("Same", json!({"stop": stop})), &o).map(|v| v["stop"].clone()), Some(json!(stop)));
    assert_eq!(brain_ok(&with("Same", json!({"stop": "I posted N-1.p1 twice, unasked. I stop."})), &o), Some(same.clone()), "on a post the report did not hold");
    assert_eq!(brain_ok(&with("Same", json!({"stop": null})), &o), Some(same.clone()), "none");
    assert_eq!(brain_ok(&with("Same", json!({"act": "a"})), &o), Some(same));
    let mut bare = with("Same", json!({}));
    bare.as_object_mut().unwrap().remove("notes");
    assert_eq!(brain_ok(&bare, &o), None, "no notes");
    bare = with("Next", json!({"act": "a", "rests_on": "N-7.p2", "thought": "t"}));
    bare.as_object_mut().unwrap().remove("critic");
    assert_eq!(brain_ok(&bare, &o), None, "no critic");
    assert_eq!(brain_ok(&json!({"head": "Nothing"}), &o), None, "the base step does not pick");
}

/// His earlier words on a coord's own record, against paper-research-brain's lessons (proto-mind N-1446 p450; the
/// frozen copy, docket-try-saved/paper-research/fundamentals/lessons.jsonl). Each lesson rests on posts of his that
/// say the same thing; each of them the record holds is a burst, and those of them before it are what he had said
/// before on it. Printed for each reach of `earlier-within` (OPEN at 12 hours throughout): how many of those the report
/// put in front of the owner model (HIS, OPEN or EARLIER), how many were within the reach at all, what (A)'s report
/// alone showed (HIS and OPEN), and how many lines the index had. No model runs and nothing he wrote is printed.
/// `EARLIER_COORD=DIR EARLIER_BASE=AGENT_BASE EARLIER_LESSONS=FILE coordd core::brain::tests::earlier_reach_on_a_coords_record
/// --ignored --nocapture`, DIR a copy of the coord's tickets/log.jsonl, registry.jsonl and project file, nothing else
/// of it: the load writes into it as a start does (its turn cuts), so it is never the coord itself. AGENT_BASE is the
/// built tree's agent-base where the binary runs: a binary built on the tower names the tower's tree in
/// CARGO_MANIFEST_DIR, and a load with no agent-base reads the project's calls alone (proto-mind N-1446: 12,905
/// problem lines, T-11's `intake` unknown).
#[test]
#[ignore]
fn earlier_reach_on_a_coords_record() {
    let (Ok(coord), Ok(base), Ok(lessons)) = (std::env::var("EARLIER_COORD"), std::env::var("EARLIER_BASE"), std::env::var("EARLIER_LESSONS")) else { return };
    assert!(std::path::Path::new(&format!("{base}/calls")).is_dir(), "EARLIER_BASE {base} has no calls folder");
    let mut core = Core::load(&coord, &base);
    // the project's own kinds and calls, as coordd's start reads them before the record (main.rs `Project::load`):
    // without them research's record replays with thousands of problem lines (docket-core2, N-1446 p593)
    core.project = crate::project::Project::load(&coord).expect("the coord's project file");
    core.load_tickets();
    println!("{coord} loaded with {base} and the project {:?}: {} problem lines", core.project.as_ref().map(|p| &p.dir), core.tickets.errors.len());
    let secs = |a: &super::At| crate::transcript::epoch_ms(&a.0).unwrap() / 1000;
    let day = 86400;
    let rows_of: [(&str, Option<u64>, usize); 7] = [("12 hours", Some(day / 2), 40), ("1 day", Some(day), 40), ("3 days", Some(3 * day), 40), ("7 days", Some(7 * day), 40), ("7 days", Some(7 * day), 100), ("7 days", Some(7 * day), 200), ("7 days", Some(7 * day), usize::MAX)];
    // per row: bursts, said before, within the reach, shown, index lines; and what (A) alone showed
    let mut rows = vec![[0usize; 5]; rows_of.len()];
    let (mut n, mut ids, mut held, mut alone, mut said, mut nobody) = (0, 0, 0, 0, 0, Vec::new());
    for line in std::fs::read_to_string(&lessons).unwrap().lines() {
        let l: Value = serde_json::from_str(line).unwrap();
        n += 1;
        let on: Vec<&str> = l["rests_on"].as_array().into_iter().flatten().filter_map(Value::as_str).filter(|i| i.starts_with("N-")).collect();
        ids += on.len();
        let mut posts: Vec<super::At> = on.iter().filter_map(|i| core.his_post(i)).collect();
        posts.sort();
        posts.dedup();
        held += posts.len();
        for i in 1..posts.len() {
            let b = &posts[i];
            let p = core.tickets.m.tickets[&b.1].posts.iter().find(|p| p.n == b.2).unwrap();
            let Some(agent) = p.wakes().next().cloned() else {
                nobody.push(format!("{} (routed {}, inform {:?}, moved {:?})", core.post_name(b), p.routed, p.inform, p.moved));
                continue;
            };
            let (burst, before) = (vec![core.post_name(b)], &posts[..i]);
            said += before.len();
            let a = core.reading_within(&agent, &burst, Some(day / 2), None, 0);
            alone += before.iter().filter(|x| a.his.contains(x) || a.open.contains(x)).count();
            for (k, (_, w, cap)) in rows_of.iter().enumerate() {
                let r = core.reading_within(&agent, &burst, Some(day / 2), *w, *cap);
                let row = &mut rows[k];
                row[0] += 1;
                row[1] += before.len();
                row[2] += before.iter().filter(|x| w.is_some_and(|w| secs(x) >= secs(b) - w as i64)).count();
                row[3] += before.iter().filter(|x| r.his.contains(x) || r.open.contains(x) || r.earlier.contains(x)).count();
                row[4] += r.earlier.len();
            }
        }
    }
    println!("{n} lessons; {ids} of the ids they rest on are posts (not the terminal's), {held} of them his in the record");
    println!("{} bursts: each of his posts a lesson rests on with one before it; {said} posts he had said before on it", rows[0][0]);
    println!("{} more of them woke nobody, so no agent's brain would read them: {}", nobody.len(), nobody.iter().take(6).cloned().collect::<Vec<_>>().join("; "));
    println!("(A)'s report alone (HIS, OPEN at 12 hours) showed {alone} of the {said}");
    println!("{:<10} {:>10} {:>14} {:>22} {:>20}", "reach", "index cap", "within reach", "shown to the model", "index lines (mean)");
    for ((name, _, cap), r) in rows_of.iter().zip(&rows) {
        let cap = if *cap == usize::MAX { "none".to_string() } else { cap.to_string() };
        println!("{:<10} {:>10} {:>14} {:>22} {:>20.1}", name, cap, r[2], r[3], r[4] as f64 / r[0].max(1) as f64);
    }
}

/// The project's owner model's two numbers as research gives them (paper-research N-1446 p1108): a whole build reads
/// the week, and is made again after three misses.
const OWNER_POLICY: &str = "+owner-model-within(604800).\n+owner-model-rebuild-after(3).\n";

/// The project's owner model, opened as its lead opens it (`coord open owner_model`): its ticket, its stop held by the
/// lead (`hold role lead`), whom the policy's `fills` names, as coordd's task runner answers it (`run_task`). Hidden
/// (paper-research N-1446 p1121): it stands in no outline (its kind says no `drawn`), and the window's Threads leaves
/// it out (yaaihh threads.rs `listed`, its test `the_owner_model_and_its_updates_are_in_no_list_of_his`).
fn owner_model_node(core: &mut Core) -> u64 {
    core.ticket_input(Input::Open { kind: coord_types::OWNER_MODEL.into(), under: None, opener: "lead".into(), form: json!({}), text: None, names: vec![], told: vec![] }).unwrap();
    let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).rev().find(|(_, x)| x.kind == coord_types::OWNER_MODEL).unwrap().0;
    assert_eq!(docket_machine::view::drawn(&core.tickets.m, &core.tickets.m.tickets[&t]), None, "hidden: a line of no outline");
    core.ticket_input(Input::Decided { t, step: 1, question: "fills".into(), answer: json!("lead"), provenance: json!({"policy": "the lead holds the role"}) }).unwrap();
    assert!(matches!(&core.tickets.m.tickets[&t].status, Status::Awaits { agent, .. } if agent == "lead"), "its lead holds its stop: {:?}", core.tickets.m.tickets[&t].status);
    t
}

/// The updates of the owner model NODE, oldest first: its `owner_update` tickets.
fn model_runs(core: &Core, node: u64) -> Vec<u64> {
    let runs: Vec<u64> = core.tickets.m.tickets.range(..docket_machine::HIDDEN).filter(|(_, x)| x.under == Some(node) && x.kind == coord_types::OWNER_UPDATE).map(|(t, _)| *t).collect();
    assert!(runs.iter().all(|t| docket_machine::view::drawn(&core.tickets.m, &core.tickets.m.tickets[t]).is_none()), "an update is hidden too");
    runs
}

/// The step of the owner model's update RUN coordd hands now: on its own queue, `brain owner`, never the clerk's.
fn model_step(core: &mut Core, rx: &mut UnboundedReceiver<Job>, run: u64) -> Delivery {
    core.hand_clerk();
    let mut out = None;
    while let Ok(j) = rx.try_recv() {
        match j {
            Job::Deliver { queue, delivery } if delivery.t == run => {
                assert_eq!(queue, "brain owner");
                out = Some(delivery);
            }
            Job::Clerk { deliveries } => assert!(deliveries.iter().all(|(d, _)| d.t != run), "no step of the owner model's goes to the clerk"),
            _ => {}
        }
    }
    out.expect("the owner model's step, on its own queue")
}

/// His post ID as the owner model reads it (`owner_posts`): under its day, time, id and thread, his words whole.
fn model_block(core: &Core, id: &str) -> String {
    let (t, n) = docket_machine::post_id(id).unwrap();
    let p = core.tickets.m.tickets[&t].posts.iter().find(|p| p.n == n).unwrap();
    format!("## {}, {id} ({})\n{}\n", super::week_time(&p.at), core.ticket_head(t), crate::label::read(&p.text).unwrap())
}

/// Post ID's time as the owner model's FROM and TO say it.
fn model_time(core: &Core, id: &str) -> String {
    let (t, n) = docket_machine::post_id(id).unwrap();
    super::week_time(&core.tickets.m.tickets[&t].posts.iter().find(|p| p.n == n).unwrap().at)
}

/// The call ENV renders, as clerk.rs's `brain` renders it for the model.
fn model_prompt(core: &Core, call: &str, env: &Value) -> String {
    let places = core.places();
    crate::policy::call::Calls::load(&places.call_dirs(), &places.overlay).unwrap().render_step(call, env).unwrap()
}

/// The owner model (the brain's part 1, paper-research N-1446 p1108). With no node no update is opened. Once the lead
/// has opened it, coordd opens one update in its round for his posts to w2 and to w3 (an agent's post is not his): it
/// builds the model whole, on its own queue, with N-1445 p29's question over his posts in his words. While that update
/// is open his next post opens nothing; once it has answered Model, that post goes in the next update (written in the
/// same second as the newest the open one was sent, on an older thread: a model covers the posts it was sent, never a
/// time), which brings the model up to date (`delta`) from the last one, marked as a model's. Each model is then what
/// `owner_model` gives, up to the newest post its update was sent, and with nothing new nothing is opened. Attention
/// and the selector read it under the words "the model's reading of him, not his words". The record replays it with no
/// problem line, and a start reads it back first: nothing is opened for what was read, and his next post opens a delta.
#[test]
fn the_owner_model_is_built_whole_then_brought_up_to_date_from_his_posts() {
    let (dir, base, mut core, mut rx) = coord("owner-model", "idle");
    policy(&mut core, &dir, OWNER_POLICY);
    core.lapse_times = Default::default();
    let a = open(&mut core, "owner", &["w2"], "one message box, not two. reuse the one we have");
    let b = open(&mut core, "owner", &["w3"], "and the spellcheck in it");
    let theirs = open(&mut core, "w2", &["w3"], "w2 · the box is built");
    core.to_hand_over();
    assert!(core.tickets.m.tickets.values().all(|x| x.kind != coord_types::OWNER_UPDATE), "no owner model, no update");
    let node = owner_model_node(&mut core);
    let id = core.tickets.m.id(node);
    assert_eq!(core.owner_model(), None, "no model yet");
    core.to_hand_over();
    let runs = model_runs(&core, node);
    assert_eq!(runs.len(), 1, "one update for his posts to w2 and to w3");
    assert_eq!(core.tickets.m.tickets[&runs[0]].form, json!({"posts": [a, b], "built": false, "misses": 0, "rebuild_after": 3, "within": 604800}));
    // whole: his posts of the week, his words only
    let d = model_step(&mut core, &mut rx, runs[0]);
    assert_eq!(call(&d), "whole");
    let (j, m) = job(&mut core, &d);
    assert_eq!((m.agent.as_str(), m.call.as_str()), ("owner", OWNER_WHOLE));
    assert_eq!(crate::clerk::brain_whose(&m), "the owner model", "each journal line names it so");
    assert_eq!(m.env, json!({"from": model_time(&core, &a), "to": model_time(&core, &b), "posts": format!("{}\n{}", model_block(&core, &a), model_block(&core, &b))}));
    let prompt = model_prompt(&core, OWNER_WHOLE, &m.env);
    // the journal, beside its answer: what it sent, his posts and the bytes of its words as rendered (proto-lead p1118)
    assert_eq!(core.owner_sent(&j), Some(format!("sent 2 posts, {} bytes", prompt.len())));
    assert!(prompt.contains(&model_block(&core, &a)) && prompt.contains("what do you think the owner wants? In general, not task by task"), "{prompt}");
    assert!(!prompt.contains("the box is built"), "{prompt}");
    // his post while it is open, on a's thread, in the same second as b, the newest it was sent (the record's time is
    // to the second): nothing opened until it has answered, then it goes, though by (time, ticket, post) it is before b
    let c = his_post(&mut core, t_of(&a), &["w2"], "keep it one box, and fix the spelling as he types");
    let same = core.tickets.m.tickets[&t_of(&b)].posts[0].at.clone();
    let (ct, cn) = docket_machine::post_id(&c).unwrap();
    core.tickets.m.tickets.edit().get_mut(&ct).unwrap().posts.iter_mut().find(|p| p.n == cn).unwrap().at = same;
    core.to_hand_over();
    assert_eq!(model_runs(&core, node).len(), 1, "none while an update is open");
    let whole = "He wants one of each thing, extended, never a second copy beside it.";
    answer(&mut core, &d, json!({"head": "Model", "text": whole}));
    assert_eq!((result(&core, runs[0])["head"].clone(), result(&core, runs[0])["text"].clone()), (json!("Model"), json!(whole)));
    assert_eq!(core.owner_model(), Some((id.clone(), whole.to_string(), b.clone())), "up to the newest post it was sent");
    // what a pass read is what it renders (proto-lead N-1446 p1153, docket-core2 p1155): a reading of his burst taken
    // now, while this model stands and w2's turn is as it is
    let read_then = core.reading("w2", std::slice::from_ref(&a));
    let doing_then = core.turn_on("w2");
    // delta: the post he wrote while it ran, on the last model
    core.to_hand_over();
    let runs = model_runs(&core, node);
    assert_eq!(runs.len(), 2, "the post he wrote while it was open goes next");
    assert_eq!(core.tickets.m.tickets[&runs[1]].form, json!({"posts": [c], "built": true, "misses": 0, "rebuild_after": 3, "within": 604800}));
    let d = model_step(&mut core, &mut rx, runs[1]);
    assert_eq!(call(&d), "delta");
    let (j, m) = job(&mut core, &d);
    assert_eq!(m.call, OWNER_DELTA);
    assert_eq!(m.env, json!({"upto": b, "model": whole, "from": model_time(&core, &c), "to": model_time(&core, &c), "posts": model_block(&core, &c)}));
    let prompt = model_prompt(&core, OWNER_DELTA, &m.env);
    assert_eq!(core.owner_sent(&j), Some(format!("sent 1 post, {} bytes", prompt.len())));
    assert!(prompt.contains(whole) && prompt.contains(&model_block(&core, &c)) && !prompt.contains(&model_block(&core, &a)), "{prompt}");
    let delta = "He wants one of each thing, extended; and his words spelled right as he types.";
    answer(&mut core, &d, json!({"head": "Model", "text": delta}));
    assert_eq!(core.owner_model(), Some((id.clone(), delta.to_string(), c.clone())));
    // a new model since, and w2 given his post: the reading taken before renders the model and the turn it read
    handed_on(&mut core, "w2", &a);
    assert_ne!(core.turn_on("w2"), doing_then, "w2's turn has moved on");
    let then = format!("the model's reading of him, not his words (owner model {id}, from his posts up to {b}):\n{whole}");
    let env = core.attention_env("w2", &read_then);
    assert_eq!((env["model"].as_str(), env["doing"].as_str()), (Some(then.as_str()), Some(doing_then.as_str())));
    assert_eq!(core.election_env("w2", &read_then, &json!({})).0["model"], then);
    core.to_hand_over();
    assert_eq!(model_runs(&core, node).len(), 2, "nothing new, nothing opened");
    // attention and the selector read it, marked as the model's
    let label = format!("the model's reading of him, not his words (owner model {id}, from his posts up to {c}):\n{delta}");
    let t = pass(&mut core, &[&c]);
    let d = step(&mut core, &mut rx, t);
    let (_, p) = job(&mut core, &d);
    assert_eq!(p.env["model"], label);
    assert_eq!(crate::clerk::brain_whose(&p), "the brain of w2");
    assert!(model_prompt(&core, ATTENTION, &p.env).contains(&format!("What he wants in general, as a model read it from all his posts: {label}")));
    answer(&mut core, &d, drifted(&c));
    let d = step(&mut core, &mut rx, t);
    assert_eq!(job(&mut core, &d).1.env["model"], label);
    let _ = theirs;
    // the record replays it; a start reads it back first
    let again = crate::ticket::Tickets::load(&dir.display().to_string(), &base, Some(&dir.join("files").display().to_string()), 1);
    assert!(again.errors.is_empty(), "{:#?}", again.errors);
    let made: Vec<(Value, Value)> = again.m.tickets.range(..docket_machine::HIDDEN).filter(|(_, x)| x.under == Some(node) && x.kind == coord_types::OWNER_UPDATE).map(|(_, x)| match &x.status { Status::Done(v) => (v["head"].clone(), v["text"].clone()), s => panic!("{s:?}") }).collect();
    assert_eq!(made, [(json!("Model"), json!(whole)), (json!("Model"), json!(delta))]);
    drop(core);
    let (mut core, mut rx) = super::super::delivery::tests::load(&dir, &base, "idle");
    policy(&mut core, &dir, OWNER_POLICY);
    assert_eq!(core.owner_model(), Some((id.clone(), delta.to_string(), c.clone())), "the first read after a start");
    core.to_hand_over();
    assert_eq!(model_runs(&core, node).len(), 2, "nothing opened for what was read");
    let e = his_post(&mut core, t_of(&b), &["w3"], "and the spellcheck marks what it is not sure of");
    core.to_hand_over();
    let runs = model_runs(&core, node);
    assert_eq!(core.tickets.m.tickets[&runs[2]].form["posts"], json!([e]));
    assert_eq!(call(&model_step(&mut core, &mut rx, runs[2])), "delta");
    kept_for_the_way_back(&dir, "owner-model");
    let _ = std::fs::remove_dir_all(&dir);
}

/// An update whose call failed (Failed, as `brain_failed` answers it) makes no model, and its posts are not sent again
/// by themselves (no loop of calls): only with a post of his it was not sent, both then, built whole. Without both of
/// the policy's numbers no update is opened, and a post of his from before `owner-model-within` is not sent; while he
/// is writing, in a thread's box or to an agent, none is, and once he stops what he wrote goes in the same update. Once
/// the lead has stopped it there is no model and none is opened.
#[test]
fn a_failed_update_waits_for_his_next_post_and_none_opens_without_the_policy_or_while_he_writes() {
    let (dir, _, mut core, mut rx) = coord("owner-failed", "idle");
    policy(&mut core, &dir, "+owner-model-within(604800).\n");
    core.lapse_times = Default::default();
    let old = open(&mut core, "owner", &["w2"], "first of all the brain should do what do i think the owner wants");
    written_ago(&mut core, &old, 9 * 86400);
    let a = open(&mut core, "owner", &["w2"], "one message box, not two");
    let node = owner_model_node(&mut core);
    core.to_hand_over();
    assert!(model_runs(&core, node).is_empty(), "no owner-model-rebuild-after: nothing opened");
    policy(&mut core, &dir, OWNER_POLICY);
    core.lapse_times = Default::default();
    core.to_hand_over();
    let runs = model_runs(&core, node);
    assert_eq!(runs.len(), 1);
    assert_eq!(core.tickets.m.tickets[&runs[0]].form["posts"], json!([a]), "his post of nine days before is not sent");
    let d = model_step(&mut core, &mut rx, runs[0]);
    let (j, _) = job(&mut core, &d);
    core.brain_answered(&j, &d, brain_failed("no model answered"), None, Some("no model answered".into()));
    assert_eq!((result(&core, runs[0])["head"].clone(), result(&core, runs[0])["why"].clone()), (json!("Failed"), json!("no model answered")));
    assert_eq!(core.owner_model(), None, "no model");
    core.to_hand_over();
    let _ = open(&mut core, "w2", &["w3"], "w2 · the box is built");
    core.to_hand_over();
    assert_eq!(model_runs(&core, node).len(), 1, "not again on the same posts");
    // he writes in a thread's box, then to w2: nothing; once he stops, his post goes, with the one the failed run had
    let pid = std::process::id() as i32;
    let b = open(&mut core, "owner", &["w3"], "and the spellcheck in it");
    let box_of = core.tickets.m.id(t_of(&b));
    assert!(core.owner_writing_on(&Who::Owner, &box_of, true, pid).ok);
    core.to_hand_over();
    assert_eq!(model_runs(&core, node).len(), 1, "he is writing in a box");
    assert!(core.owner_writing_on(&Who::Owner, &box_of, false, pid).ok);
    assert!(core.owner_writing(&Who::Owner, "w2", true, pid).ok);
    core.to_hand_over();
    assert_eq!(model_runs(&core, node).len(), 1, "he is writing to w2");
    assert!(core.owner_writing(&Who::Owner, "w2", false, pid).ok);
    core.to_hand_over();
    let runs = model_runs(&core, node);
    assert_eq!(runs.len(), 2, "once he stops");
    assert_eq!((core.tickets.m.tickets[&runs[1]].form["posts"].clone(), core.tickets.m.tickets[&runs[1]].form["built"].clone()), (json!([a, b]), json!(false)));
    let d = model_step(&mut core, &mut rx, runs[1]);
    assert_eq!(call(&d), "whole");
    answer(&mut core, &d, json!({"head": "Model", "text": "He wants one box."}));
    assert!(core.owner_model().is_some());
    // the lead stops it: no model, nothing opened
    let Status::Awaits { agent, .. } = core.tickets.m.tickets[&node].status.clone() else { panic!() };
    core.ticket_answer(&agent, node, json!({"head": "Stopped", "why": "off for now"}), Text::New(String::new())).unwrap();
    assert!(!core.tickets.m.tickets[&node].is_open());
    assert_eq!(core.owner_model(), None, "stopped: no model");
    let k = his_post(&mut core, t_of(&a), &["w2"], "keep it one box");
    core.to_hand_over();
    assert_eq!(model_runs(&core, node).len(), 2, "stopped: nothing opened");
    // and a pass on his post is told none stands, not that none was ever made (consult-3 N-1446 p1176)
    let t = pass(&mut core, &[&k]);
    let d = step(&mut core, &mut rx, t);
    assert_eq!(job(&mut core, &d).1.env["model"], super::NO_MODEL);
    answer(&mut core, &d, drifted(&k));
    let d = step(&mut core, &mut rx, t);
    assert_eq!(job(&mut core, &d).1.env["model"], super::NO_MODEL, "the selector too");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A way back onto build 64 with an update open (proto-reader N-1446 p1184, 1; proto-mind's wb64-step-run5.log): 64
/// has no `delta`, so its watcher takes the step for an intake (a call of `sort` with no text) and answers Thread, which
/// the step refuses; 64 hands it no more, at that start or the next. A refused step is not handed again here either, and
/// an open update would hold every later one: back on this build, the round answers it Failed, saying why, and his next
/// post opens the update that reads it with the one the refused update was sent.
#[test]
fn an_update_an_older_build_answered_out_of_shape_fails_and_his_next_post_opens_the_next() {
    let (dir, _, mut core, _rx) = coord("owner-refused", "idle");
    policy(&mut core, &dir, OWNER_POLICY);
    core.lapse_times = Default::default();
    let a = open(&mut core, "owner", &["w2"], "one message box, not two");
    let node = owner_model_node(&mut core);
    core.to_hand_over();
    let runs = model_runs(&core, node);
    assert_eq!(runs.len(), 1);
    // build 64's watcher, as wb64-step-run5.log shows it: its intake's answer to a step it has no call for
    let e = core.ticket_answer(docket_machine::WATCHER, runs[0], json!({"head": "Thread"}), Text::New(String::new())).unwrap_err();
    assert!(e.contains("Thread is not one of Model, Failed"), "{e}");
    core.to_hand_over();
    let failed = result(&core, runs[0]);
    assert_eq!(failed["head"], "Failed", "{failed}");
    assert!(failed["why"].as_str().unwrap().contains("Thread is not one of Model, Failed"), "{failed}");
    assert_eq!(core.owner_model(), None, "no model");
    let b = his_post(&mut core, t_of(&a), &["w2"], "and the spellcheck in it");
    core.to_hand_over();
    let runs = model_runs(&core, node);
    assert_eq!(runs.len(), 2, "his next post opens the next");
    assert_eq!(core.tickets.m.tickets[&runs[1]].form["posts"], json!([a, b]));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The owner model each change told his window (`Body::OwnerModel`) since event FROM.
fn owner_model_told(core: &Core, from: u64) -> Vec<Option<coord_types::OwnerModelView>> {
    core.since(from).unwrap_or_default().into_iter().filter_map(|e| match e.body {
        coord_types::Body::OwnerModel(o) => Some(o),
        _ => None,
    }).collect()
}

/// What his window draws of the owner model is coordd's reading of it, at each change (proto-reader N-1446 p1212, B;
/// proto-lead p1227): no node; a node with its first update at work; the model it made, by its update, the newest post
/// it read, when it closed and its first line that says something (proto-window p1227: the heading passed over);
/// nothing told while nothing changed; a later update that failed beside the model that stands; the snapshot carrying
/// the same; and none once the lead stops it.
#[test]
fn the_owner_model_his_window_draws_is_coordds_reading_at_each_change() {
    use coord_types::{ModelMade, OwnerModelView, UpdateEnded};
    let (dir, _, mut core, mut rx) = coord("owner-view", "idle");
    policy(&mut core, &dir, OWNER_POLICY);
    core.lapse_times = Default::default();
    let s = core.seq;
    core.to_hand_over();
    assert_eq!((core.owner_model_view(), owner_model_told(&core, s)), (None, vec![None]), "the first round tells what stands: no node");
    let a = open(&mut core, "owner", &["w2"], "one message box, not two");
    let node = owner_model_node(&mut core);
    let s = core.seq;
    core.to_hand_over();
    let runs = model_runs(&core, node);
    let at_work = OwnerModelView { node: core.tickets.m.id(node), running: true, model: None, failed: None };
    assert_eq!(owner_model_told(&core, s), vec![Some(at_work)], "its first update at work");
    let d = model_step(&mut core, &mut rx, runs[0]);
    answer(&mut core, &d, json!({"head": "Model", "text": "# Owner model\n\n**He wants one box**, not two.\nAnd his words spelled right."}));
    let s = core.seq;
    core.to_hand_over();
    let closed = |core: &Core, u: u64| core.tickets.m.tickets[&u].closed_at.clone().unwrap();
    let made = ModelMade { update: core.tickets.m.id(runs[0]), upto: a.clone(), at: closed(&core, runs[0]), first_line: "He wants one box, not two.".into() };
    let stands = OwnerModelView { node: core.tickets.m.id(node), running: false, model: Some(made.clone()), failed: None };
    assert_eq!(owner_model_told(&core, s), vec![Some(stands.clone())]);
    let s = core.seq;
    core.to_hand_over();
    assert!(owner_model_told(&core, s).is_empty(), "nothing changed, nothing told");
    let _ = his_post(&mut core, t_of(&a), &["w2"], "and the spellcheck in it");
    core.to_hand_over();
    let runs = model_runs(&core, node);
    let d = model_step(&mut core, &mut rx, runs[1]);
    let (j, _) = job(&mut core, &d);
    core.brain_answered(&j, &d, brain_failed("no model answered"), None, Some("no model answered".into()));
    let s = core.seq;
    core.to_hand_over();
    let failed = OwnerModelView { failed: Some(UpdateEnded { update: core.tickets.m.id(runs[1]), at: closed(&core, runs[1]) }), ..stands };
    assert_eq!(owner_model_told(&core, s), vec![Some(failed.clone())], "the model that stands, and the update since that made none");
    let coord_types::Body::Snapshot(snap) = core.snapshot().body else { panic!() };
    assert_eq!(snap.owner_model, Some(failed), "the snapshot carries it whole");
    let Status::Awaits { agent, .. } = core.tickets.m.tickets[&node].status.clone() else { panic!() };
    core.ticket_answer(&agent, node, json!({"head": "Stopped", "why": "off for now"}), Text::New(String::new())).unwrap();
    let s = core.seq;
    core.to_hand_over();
    assert_eq!(owner_model_told(&core, s), vec![None], "stopped: no node");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A words kind as research's (its flavor's words.kind, cut to its owner's `look`): what he typed stays in its form, and
/// his answer is a post of his with no words, as the window sends it (yaaihh words.rs: `text: None`).
const WORDS_CALL: &str = "call look(text: Text) -> Go | Use(text: Text) | AsTyped\n  task look\n  default Go\n";
const WORDS_KIND: &str = "kind words\n  form text: Text\n  result Send(text: Text) | Back(why: Text)\n  pattern\n    o <- step owner look(text);\n    match o {\n      Go => return Send(\"and the spelling\")\n      Use(mine) => return Send(mine)\n      AsTyped => return Send(text)\n    }\n";

/// Each of his posts once, as his window shows it (paper-research N-1446 p1124, 1). He typed "one box, nto two" in his
/// box: a words ticket of his holds it as typed, his answer to its `look` (Use) is a post of his there (with its words,
/// as `coord answer` can give them; the window gives none), and the cleaned words are his post to w2 on a thread,
/// marked as cleaned from that ticket. The update that builds the model is sent that post alone and reads it once,
/// cleaned: neither what he typed nor anything of the words ticket. Since then, his Granted to w2's request and a Go on
/// a second words ticket, as the window sends them, are posts of his with no words, and they open nothing.
#[test]
fn a_cleaned_post_of_his_is_read_once_as_his_window_shows_it() {
    let (dir, _base, mut core, mut rx) = coord("owner-model-cleaned", "idle");
    policy(&mut core, &dir, OWNER_POLICY);
    std::fs::write(dir.join("files/kinds/words.kind"), WORDS_KIND).unwrap();
    std::fs::write(dir.join("files/calls/words.call"), WORDS_CALL).unwrap();
    core.load_tickets();
    assert!(core.tickets.errors.is_empty(), "{:?}", core.tickets.errors);
    core.lapse_times = Default::default();
    let node = owner_model_node(&mut core);
    let words = |core: &mut Core, typed: &str, head: Value| -> u64 {
        let r = core.ticket_input(Input::Open { kind: "words".into(), under: None, opener: "owner".into(), form: json!({"text": typed}), text: None, names: vec![], told: vec![] }).unwrap();
        let w = docket_machine::parse_ticket(r.trim_start_matches("Opened ").split(['.', ';']).next().unwrap()).unwrap();
        core.ticket_answer("owner", w, head, Text::New(String::new())).unwrap();
        assert!(core.tickets.m.cleaned_words(w).is_ok(), "{:?}", core.tickets.m.tickets[&w].status);
        assert!(core.tickets.m.tickets[&w].posts.iter().any(|p| p.by == "owner" && core.tickets.m.tickets[&w].his_shown().contains(&p.n)), "his answer is a post of his");
        w
    };
    let w = words(&mut core, "one box, nto two", json!({"head": "Use", "text": "one box, not two"}));
    let on_w = core.tickets.m.tickets.edit().get_mut(&w).unwrap().posts.iter_mut().find(|p| p.by == "owner").unwrap();
    on_w.text = Text::New("one box, not two".into());
    let a = open(&mut core, "owner", &["w2"], "one box, not two");
    let (t, n) = docket_machine::post_id(&a).unwrap();
    core.ticket_input(Input::Cleaned { t, n, words: w }).unwrap();
    assert_eq!(core.tickets.m.tickets[&t].posts.iter().find(|p| p.n == n).unwrap().cleaned_from, Some(core.tickets.m.id(w)));
    core.to_hand_over();
    let runs = model_runs(&core, node);
    assert_eq!(runs.len(), 1);
    assert_eq!(core.tickets.m.tickets[&runs[0]].form["posts"], json!([a]), "the cleaned post alone");
    let d = model_step(&mut core, &mut rx, runs[0]);
    let (_, m) = job(&mut core, &d);
    assert_eq!(m.env["posts"], json!(model_block(&core, &a)), "once, cleaned");
    let prompt = model_prompt(&core, OWNER_WHOLE, &m.env);
    assert_eq!(prompt.matches(&format!("{a} (")).count(), 1, "{prompt}");
    assert!(!prompt.contains("nto two") && !prompt.contains(&core.tickets.m.id(w)), "nothing of the words ticket: {prompt}");
    answer(&mut core, &d, json!({"head": "Model", "text": "He wants one box."}));
    let r = core.ticket_input(Input::Open { kind: "request".into(), under: None, opener: "w2".into(), form: json!({"who": "w2", "approver": "owner", "why": "three more turns", "n": 3}), text: None, names: vec![], told: vec![] }).unwrap();
    let r = docket_machine::parse_ticket(r.trim_start_matches("Opened ").split(['.', ';']).next().unwrap()).unwrap();
    core.ticket_answer("owner", r, json!({"head": "Granted", "n": 3}), Text::New(String::new())).unwrap();
    assert!(core.tickets.m.tickets[&r].posts.iter().any(|p| p.by == "owner" && core.tickets.m.tickets[&r].his_shown().contains(&p.n)), "his Granted is a post of his");
    core.to_hand_over();
    assert_eq!(model_runs(&core, node).len(), 1, "his answer with no words opens nothing");
    let _ = words(&mut core, "and teh spelling", json!({"head": "Go"}));
    core.to_hand_over();
    assert_eq!(model_runs(&core, node).len(), 1, "nor his Go on his words");
    let _ = std::fs::remove_dir_all(&dir);
}

/// research's binding of the base step (flavor projects/research/kinds/agents/paper-research.kind), on w2.
const AHEAD: &str = "every e.every of turn_end(e) where e.agent == \"w2\" open brain(e.agent, Ahead(e.every))";

/// The base step (brain.kind's Ahead; the owner, N-1104 p606, p610; N-1511 p56, p59) as research binds it, on w2 at
/// the policy's N 2. The lead's ask was given to w2 in a turn that ended with Nothing, so w2 owes it; w2's own post went
/// two minutes before. At the second turn end one pass opens, and its step `ahead` goes on w2's brain queue. Its report
/// is the record: w2's role under the lead, its post, the ask it owes by the ledger, nothing waiting, its turn, and no
/// last look; the call's template takes it whole. Next, resting on the ask, posts the thought naming w2: idle, w2 is
/// handed it at once, with no call (`free`'s `worked` false). The next pass, while w2 works, reads that look's act and
/// notes and not w2's post from before it, nor that thought as an ask w2 owes, though the ledger holds it unanswered
/// (paper-research N-1446 p2781); its Next rests on an id its report did not hold, so it is Same and says nothing. The
/// third's Next, while w2 still works, goes into no turn: `arrived` keeps it for the turn's end, where `free` is raised
/// for it, worked, and delivery's `fold` gives it alone. A failed call says nothing. With WAY_BACK_OUT set the coord is
/// kept for the way back.
#[test]
fn the_base_step_reads_the_record_since_its_last_look_and_its_next_asks_the_agent_at_its_next_quiet_point() {
    let (dir, _, mut core, mut rx) = coord("brain-ahead", "idle");
    policy(&mut core, &dir, "+unanswered-within(43200).\n+check-in-every(\"w2\", 2).\n");
    std::fs::create_dir_all(dir.join("files/kinds/agents")).unwrap();
    std::fs::write(dir.join("files/kinds/agents/w2.kind"), format!("{AHEAD}\n")).unwrap();
    let r = core.ticket_reload(&Who::Owner, None, "w2's base step on".into());
    assert!(r.ok, "{}", r.out);
    assert_eq!(core.view("w2").brain.map(|b| b.on), Some(true), "its card shows the switch");
    let stills = open(&mut core, "lead", &["w2"], "@w2 send the stills");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("the lead's ask");
    core.handed("w2", &ids, true, "channel");
    end_turn(&mut core, &dir, "Nothing");
    let green = open(&mut core, "w2", &["lead"], "@lead 73's gate is green; the stills come after the replay");
    written_ago(&mut core, &green, 120);
    let ends = |core: &mut Core| {
        for _ in 0..2 {
            core.ticket_input(Input::Event { name: TURN_END.into(), value: json!({"agent": "w2", "answered": false, "every": 2}) }).unwrap();
        }
        *core.tickets.m.tickets.iter().rev().find(|(_, x)| x.kind == "brain").expect("a pass").0
    };
    let frees = |dir: &Path| -> Vec<Value> {
        let log = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap();
        log.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["ev"] == "event" && v["name"] == "free" && v["value"]["agent"] == "w2").map(|v| v["value"].clone()).collect()
    };
    let next = |on: &str, thought: &str| json!({"head": "Next", "goal": "the stills the lead asked for", "critic": "the lead waits", "memory": "it said the stills come after the replay",
        "attention": "the lead waits on the stills", "act": "send the stills", "rests_on": on, "thought": thought, "notes": format!("told it: {thought}")});

    // the first look: its report, from the record
    let first = ends(&mut core);
    assert_eq!(core.tickets.m.tickets[&first].form, json!({"agent": "w2", "pass": {"head": "Ahead", "turns": 2}}));
    let d = step(&mut core, &mut rx, first);
    assert_eq!(call(&d), "ahead");
    let (_, b) = job(&mut core, &d);
    assert_eq!(b.call, OUTLOOK);
    let env = |k: &str| b.env[k].as_str().unwrap_or_default().to_string();
    assert_eq!(env("role"), "a member, under lead");
    assert!(env("turns").starts_with("its last 2 turns; this is your first look"), "{}", env("turns"));
    assert!(env("its").contains(&green) && env("its").contains("the stills come after the replay"), "{}", env("its"));
    assert!(env("owed").starts_with(&format!("{stills} · lead · named · ")) && env("owed").ends_with("send the stills"), "{}", env("owed"));
    assert_eq!((env("within"), env("waiting"), env("last")), ("from the 12 hours before now".to_string(), "(none)".to_string(), "none: this is your first look".to_string()));
    assert!(env("doing").starts_with("no turn running"), "{}", env("doing"));
    assert!(b.ids.contains(&stills) && b.ids.contains(&green), "{:?}", b.ids);
    let prompt = model_prompt(&core, OUTLOOK, &b.env);
    assert!(prompt.contains("What it was asked and has not answered, from the 12 hours before now, oldest first") && prompt.contains(&env("owed")) && prompt.contains("Your last look, and your notes then:"), "{prompt}");
    let said = format!("I owe the lead the stills on {stills}. I send them now.");
    answer(&mut core, &d, next(&stills, &said));
    assert!(matches!(&core.tickets.m.tickets[&first].status, Status::Done(v) if v["head"] == "Next"));
    let post = core.tickets.m.tickets[&first].posts.last().cloned().expect("its thought");
    assert_eq!(post.names, ["w2"]);
    let thought = format!("{}.p{}", core.tickets.m.id(first), post.n);
    assert_eq!(core.to_hand_over(), ["w2"], "idle: it goes at once");
    assert_eq!(frees(&dir).last().map(|f| f["worked"].clone()), Some(json!(false)), "with no call");
    let (ids, ..) = channel(&mut core, &mut rx).expect("the thought");
    assert_eq!(ids, [thought.clone()]);
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    working(&mut core);

    // the next look, while w2 works: the last look's act and notes, w2's post from before it no more; its Next rests on
    // nothing the report held, so it is Same
    let second = ends(&mut core);
    let d = step(&mut core, &mut rx, second);
    let (_, b) = job(&mut core, &d);
    let env = |k: &str| b.env[k].as_str().unwrap_or_default().to_string();
    assert!(env("turns").starts_with("its last 2 turns, since your last look at"), "{}", env("turns"));
    assert!(env("last").contains(&format!("you had it act: send the stills (resting on {stills}), and told it: \"{said}\"")) && env("last").ends_with(&format!("Your notes then:\ntold it: {said}")), "{}", env("last"));
    assert!(!env("its").contains(&green), "{}", env("its"));
    // its first look's thought, given and not answered, is no ask it owes: OWED holds the lead's ask alone
    let unanswered = |core: &Core| core.tickets.m.ledger("w2").into_iter().filter(|c| c.answer.is_none()).map(|c| c.post).collect::<Vec<_>>();
    assert_eq!(unanswered(&core), [stills.clone(), thought.clone()], "the ledger holds both");
    assert!(env("owed").starts_with(&format!("{stills} · lead · named · ")) && !env("owed").contains('\n') && !b.ids.contains(&thought), "{}", env("owed"));
    answer(&mut core, &d, next("N-99.p1", "I send the stills now."));
    assert!(matches!(&core.tickets.m.tickets[&second].status, Status::Done(v) if v["head"] == "Same" && v["notes"] == "told it: I send the stills now."), "{:?}", core.tickets.m.tickets[&second].status);
    assert!(core.tickets.m.tickets[&second].posts.is_empty(), "Same says nothing");

    // the third's Next while w2 still works: into no turn; kept for its end, where `free` is raised for it
    let third = ends(&mut core);
    let d = step(&mut core, &mut rx, third);
    answer(&mut core, &d, next(&stills, "The lead still waits on the stills. I send them before anything else."));
    let again = format!("{}.p{}", core.tickets.m.id(third), core.tickets.m.tickets[&third].last());
    let n = frees(&dir).len();
    assert!(!core.to_hand_over().contains(&"w2".to_string()) && channel(&mut core, &mut rx).is_none(), "nothing goes into the running turn");
    assert_eq!(kept(&core, &again).as_deref(), Some("comes when its turn ends"), "delivery.kind's `arrived` keeps a push");
    end_turn(&mut core, &dir, "Nothing");
    let handing = core.to_hand_over();
    let pushed = frees(&dir);
    assert_eq!(pushed.len(), n + 1, "{pushed:?}");
    assert_eq!(pushed[n]["worked"], json!(true), "it came while w2 worked, so `fold` decides it at the turn's end");
    let decided = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["name"] == "delivery_decided").last().map(|v| v["value"]["head"].clone());
    assert_eq!(decided, Some(json!({"head": "Give", "now": [again], "rest": []})), "fold gives it");
    assert_eq!(handing, ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("the thought, at the turn's end");
    assert_eq!(ids, [again.clone()]);
    core.handed("w2", &ids, true, "channel");

    // a failed call says nothing
    let fourth = ends(&mut core);
    let d = step(&mut core, &mut rx, fourth);
    let (job4, b) = job(&mut core, &d);
    assert_eq!(unanswered(&core), [stills.clone(), thought.clone(), again.clone()]);
    let owed = b.env["owed"].as_str().unwrap_or_default();
    assert!(owed.starts_with(&format!("{stills} · lead · named · ")) && !owed.contains('\n') && !b.ids.contains(&thought) && !b.ids.contains(&again), "{owed}");
    core.brain_answered(&job4, &d, brain_failed("no model answered"), None, Some("no model answered".into()));
    assert!(matches!(&core.tickets.m.tickets[&fourth].status, Status::Done(v) if v["head"] == "Failed") && core.tickets.m.tickets[&fourth].posts.is_empty());
    kept_for_the_way_back(&dir, "brain-ahead");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Consolidation as research would bind it (brain.kind's header line), on w2.
const COMPACTS: &str = "on compacting(e) where e.agent == \"w2\" open brain(e.agent, Compacting)";

/// Consolidation (brain.kind's Compacting; paper-research N-1104 p622, 5, his p624: "yes yes"), bound on w2. The lead's
/// ask was given to w2 in a turn that ended with Nothing, so w2 owes it; w2's own post said the stills come after the
/// replay. w2 at work, its compaction begins: its hook shows it, and the `compacting` line coordd sends opens one pass,
/// whose step `consolidate` goes on w2's brain queue and reads the base step's report, TURNS since when alone (no look
/// yet). Kept keeps the lines that name a post the report held, one resting on the ask and one on w2's post; a line
/// naming a post the report did not hold and one naming none are dropped. The lines are posted on the pass informing
/// w2, waking nobody, and go into neither the compaction nor the turn: at the turn's end they are given alone and whole,
/// with no call, asking for nothing. Kept is no look: the next compaction's report still has none. An answer whose
/// every line is dropped is out of shape, and a failed call says nothing. The record replays both passes as they
/// closed. With WAY_BACK_OUT set the coord is kept for the way back.
#[test]
fn at_its_compaction_the_brain_writes_what_the_agent_keeps_and_it_goes_whole_at_the_turns_end() {
    let (dir, base, mut core, mut rx) = coord("brain-keep", "idle");
    policy(&mut core, &dir, "+unanswered-within(43200).\n");
    std::fs::create_dir_all(dir.join("files/kinds/agents")).unwrap();
    std::fs::write(dir.join("files/kinds/agents/w2.kind"), format!("{COMPACTS}\n")).unwrap();
    let r = core.ticket_reload(&Who::Owner, None, "w2's consolidation on".into());
    assert!(r.ok, "{}", r.out);
    let stills = open(&mut core, "lead", &["w2"], "@w2 send the stills");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("the lead's ask");
    core.handed("w2", &ids, true, "channel");
    end_turn(&mut core, &dir, "Nothing");
    let green = open(&mut core, "w2", &["lead"], "@lead 73's gate is green; the stills come after the replay");
    let compacts = |core: &mut Core| -> u64 {
        core.begin_turn("w2");
        working(core);
        let _ = core.hook("w2", "compact", &json!({}));
        assert!(core.agents["w2"].live.compacting, "its hook shows the compaction");
        core.ticket_input(Input::Event { name: COMPACTING.into(), value: json!({"agent": "w2"}) }).unwrap();
        *core.tickets.m.tickets.iter().rev().find(|(_, x)| x.kind == "brain").expect("a pass").0
    };

    // the first compaction: the base step's report, and what w2 keeps
    let first = compacts(&mut core);
    assert_eq!(core.tickets.m.tickets[&first].form, json!({"agent": "w2", "pass": {"head": "Compacting"}}));
    assert_eq!(core.tickets.m.tickets[&first].binding.as_deref(), Some(COMPACTS));
    let d = step(&mut core, &mut rx, first);
    assert_eq!(call(&d), "consolidate");
    let (_, b) = job(&mut core, &d);
    assert_eq!(b.call, CONSOLIDATION);
    let env = |k: &str| b.env[k].as_str().unwrap_or_default().to_string();
    assert_eq!(env("turns"), "its newest 12, as this is your first look");
    assert!(env("its").contains(&green) && env("its").contains("the stills come after the replay"), "{}", env("its"));
    assert!(env("owed").starts_with(&format!("{stills} · lead · named · ")) && env("owed").ends_with("send the stills"), "{}", env("owed"));
    assert_eq!(env("last"), "none: this is your first look");
    let prompt = model_prompt(&core, CONSOLIDATION, &b.env);
    assert!(prompt.contains("You are its memory at sleep.") && prompt.contains(&format!("Its own posts, {}, oldest first", env("turns"))) && prompt.contains(&env("owed")), "{prompt}");
    let lines = [format!("I am on the stills for the lead, {stills}."), format!("I said the stills come after the replay, {green}."), "I told w3 the replay is green, N-99.p1.".to_string(), "I keep my answers short.".to_string()];
    answer(&mut core, &d, json!({"head": "Kept", "text": lines.join("\n")}));
    let kept_text = lines[..2].join("\n");
    assert_eq!(result(&core, first), json!({"head": "Kept", "text": kept_text}));
    let p = core.tickets.m.tickets[&first].posts.last().cloned().expect("what it keeps");
    let id = core.tickets.m.id(first);
    assert_eq!((p.by.as_str(), p.text.clone(), p.inform.clone(), p.wakes().count()), (id.as_str(), Text::New(kept_text.clone()), vec!["w2".to_string()], 0));
    assert_eq!(p.into_turn, ["w2"]);
    let line = format!("{id}.p{}", p.n);
    assert!(core.to_hand_over().is_empty() && channel(&mut core, &mut rx).is_none(), "nothing into the compaction");
    assert!(!arrived(&dir, &line) && kept(&core, &line).is_none(), "no `arrived` for it, no keep");
    core.agents.get_mut("w2").unwrap().live.compacting = false;
    assert!(core.to_hand_over().is_empty(), "nor into the turn after it");
    end_turn(&mut core, &dir, "The replay is green.");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, tickets, _, _, into, rest) = channel(&mut core, &mut rx).expect("what it keeps, at the turn's end");
    assert_eq!((ids.clone(), into, rest.len()), (vec![line.clone()], false, 0));
    assert!(folds(&mut core).is_empty(), "no call");
    let (content, _) = crate::render::delivery_as(&core.places(), &tickets[0].0, &tickets[0].1, false, true, false, false, None, true, crate::render::Hints::default()).unwrap();
    assert!(content.contains(&kept_text), "whole: {content}");
    assert!(!content.contains("Answer it now") && !content.contains("reply tool"), "it asks for nothing: {content}");
    core.handed("w2", &ids, true, "channel");

    // the next compaction: still no look; an answer resting on nothing the report held, then a failed call
    let second = compacts(&mut core);
    let d = step(&mut core, &mut rx, second);
    let (job2, b) = job(&mut core, &d);
    assert_eq!((b.env["turns"].as_str(), b.env["last"].as_str()), (Some("its newest 12, as this is your first look"), Some("none: this is your first look")), "Kept writes no notes");
    assert!(brain_ok(&json!({"head": "Kept", "text": "I keep my answers short.\nI told w3 the replay is green, N-99.p1."}), &b).is_none(), "no line rests on the report");
    core.brain_answered(&job2, &d, brain_failed("out of shape"), None, Some("out of shape".into()));
    assert!(matches!(&core.tickets.m.tickets[&second].status, Status::Done(v) if v["head"] == "Failed") && core.tickets.m.tickets[&second].posts.is_empty());

    // the record replays both
    let again = crate::ticket::Tickets::load(&dir.display().to_string(), &base, Some(&dir.join("files").display().to_string()), 1);
    assert!(again.errors.is_empty(), "{:#?}", again.errors);
    let closed = |t: u64| match &again.m.tickets[&t].status {
        Status::Done(v) => v.clone(),
        s => panic!("{s:?}"),
    };
    assert_eq!((closed(first), closed(second)["head"].clone()), (json!({"head": "Kept", "text": kept_text}), json!("Failed")));
    kept_for_the_way_back(&dir, "brain-keep");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Inhibition (brain.kind's Ahead with STOP; paper-research N-1104 p622, 3: "as important as `next` ... A brain mostly
/// says no"; N-1446 p2925), on w2 as research binds the base step, at the policy's N 2. w2 owes the lead's ask and has
/// posted the replay's progress, unasked. The first look's Next carries a STOP resting on that post: one post naming
/// w2, the thought and then the stop, given at once to idle w2. w2 posts the progress again in its turn. The next look,
/// while w2 works, reads both lines in LAST; its Same with a STOP on the new post ends the pass Stop, which posts the
/// stop alone informing w2, waking nobody, going into neither the running turn nor w2's ledger: at the turn's end it
/// is given alone and whole, with no call, asking for nothing. The third look reads that Stop in LAST. A STOP that
/// names no post, or one the report did not hold, is dropped and the rest stands: Same says nothing, and the fourth's
/// Next posts its thought alone, as before the field. The record replays the four as they closed. With WAY_BACK_OUT set
/// the coord is kept for the way back.
#[test]
fn the_brains_stop_follows_its_thought_or_goes_alone_asking_nothing_and_one_resting_on_nothing_is_dropped() {
    let (dir, base, mut core, mut rx) = coord("brain-stop", "idle");
    policy(&mut core, &dir, "+unanswered-within(43200).\n+check-in-every(\"w2\", 2).\n");
    std::fs::create_dir_all(dir.join("files/kinds/agents")).unwrap();
    std::fs::write(dir.join("files/kinds/agents/w2.kind"), format!("{AHEAD}\n")).unwrap();
    let r = core.ticket_reload(&Who::Owner, None, "w2's base step on".into());
    assert!(r.ok, "{}", r.out);
    let stills = open(&mut core, "lead", &["w2"], "@w2 send the stills");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, ..) = channel(&mut core, &mut rx).expect("the lead's ask");
    core.handed("w2", &ids, true, "channel");
    end_turn(&mut core, &dir, "Nothing");
    let progress = open(&mut core, "w2", &["lead"], "@lead the replay is at 40%; next update at 60%");
    let ends = |core: &mut Core| {
        for _ in 0..2 {
            core.ticket_input(Input::Event { name: TURN_END.into(), value: json!({"agent": "w2", "answered": false, "every": 2}) }).unwrap();
        }
        *core.tickets.m.tickets.iter().rev().find(|(_, x)| x.kind == "brain").expect("a pass").0
    };
    let look = |head: &str, notes: &str, stop: Value| {
        json!({"head": head, "goal": "the stills the lead asked for", "critic": "the lead waits", "memory": "it said the stills come after the replay",
               "attention": "the lead waits on the stills", "notes": notes, "stop": stop})
    };
    let next = |thought: &str, notes: &str, stop: Value| {
        let mut v = look("Next", notes, stop);
        v["act"] = json!("send the stills");
        v["rests_on"] = json!(stills.clone());
        v["thought"] = json!(thought);
        v
    };
    let posted = |core: &Core, t: u64| core.tickets.m.tickets[&t].posts.last().cloned().expect("a post");

    // the first look's Next with a stop: one post naming w2, the thought, then the stop
    let first = ends(&mut core);
    let d = step(&mut core, &mut rx, first);
    let thought = format!("I owe the lead the stills on {stills}. I send them now.");
    let stop = format!("I posted the replay's progress on {progress}, which no one asked for. I stop.");
    answer(&mut core, &d, next(&thought, "told it to stop the progress posts", json!(stop)));
    assert_eq!((result(&core, first)["head"].clone(), result(&core, first)["stop"].clone()), (json!("Next"), json!(stop)));
    let p = posted(&core, first);
    assert_eq!((p.names.clone(), p.text.clone()), (vec!["w2".to_string()], Text::New(format!("{thought}\n{stop}"))));
    assert_eq!(core.to_hand_over(), ["w2"], "idle: it goes at once");
    let (ids, ..) = channel(&mut core, &mut rx).expect("the thought and the stop");
    assert_eq!(ids, [format!("{}.p{}", core.tickets.m.id(first), p.n)]);
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    working(&mut core);
    let more = open(&mut core, "w2", &["lead"], "@lead the replay is at 60%");

    // the next look, while w2 works: Same with a stop is Stop, posted alone informing w2, given at the turn's end
    let second = ends(&mut core);
    let d = step(&mut core, &mut rx, second);
    let (_, b) = job(&mut core, &d);
    let last = b.env["last"].as_str().unwrap_or_default().to_string();
    assert!(last.contains(&format!("and told it: \"{thought}\"; and to stop: \"{stop}\".")), "{last}");
    assert!(b.ids.contains(&more), "{:?}", b.ids);
    let again = format!("I posted the replay's progress again on {more}. I stop until it is done.");
    answer(&mut core, &d, look("Same", "told it to stop again", json!(again)));
    assert_eq!(result(&core, second), look("Stop", "told it to stop again", json!(again)));
    let p = posted(&core, second);
    let id = core.tickets.m.id(second);
    assert_eq!((p.by.as_str(), p.text.clone(), p.inform.clone(), p.names.len(), p.wakes().count()), (id.as_str(), Text::New(again.clone()), vec!["w2".to_string()], 0, 0));
    assert_eq!(p.into_turn, ["w2"]);
    let line = format!("{id}.p{}", p.n);
    assert!(core.to_hand_over().is_empty() && channel(&mut core, &mut rx).is_none(), "nothing into the running turn");
    assert!(!core.tickets.m.ledger("w2").iter().any(|c| c.post == line), "no ask in its ledger");
    end_turn(&mut core, &dir, "Nothing");
    assert_eq!(core.to_hand_over(), ["w2"]);
    let (ids, tickets, _, _, into, rest) = channel(&mut core, &mut rx).expect("the stop, at the turn's end");
    assert_eq!((ids.clone(), into, rest.len()), (vec![line.clone()], false, 0));
    assert!(folds(&mut core).is_empty(), "no call");
    let (content, _) = crate::render::delivery_as(&core.places(), &tickets[0].0, &tickets[0].1, false, true, false, false, None, true, crate::render::Hints::default()).unwrap();
    assert!(content.contains(&again), "whole: {content}");
    assert!(!content.contains("Answer it now") && !content.contains("reply tool"), "it asks for nothing: {content}");
    core.handed("w2", &ids, true, "channel");
    core.begin_turn("w2");
    working(&mut core);

    // the third look reads the Stop; a stop resting on nothing the report held is dropped, and Same says nothing
    let third = ends(&mut core);
    let d = step(&mut core, &mut rx, third);
    let (_, b) = job(&mut core, &d);
    let last = b.env["last"].as_str().unwrap_or_default().to_string();
    assert!(last.contains(&format!("you had it go on as it was, and told it only to stop: \"{again}\".")) && last.ends_with("Your notes then:\ntold it to stop again"), "{last}");
    for s in ["I stop the progress posts.".to_string(), "I posted on N-99.p1 unasked. I stop.".to_string(), format!("I posted on {stills} and N-99.p1 unasked. I stop.")] {
        assert_eq!(brain_ok(&look("Same", "", json!(s)), &b).map(|v| v["stop"].clone()), Some(Value::Null), "{s}");
    }
    assert_eq!(brain_ok(&look("Same", "", json!(format!("I posted on {stills}, twice. I stop."))), &b).map(|v| v["stop"].clone()), Some(json!(format!("I posted on {stills}, twice. I stop."))));
    answer(&mut core, &d, look("Same", "nothing new", json!("I stop, N-99.p1.")));
    let mut same = look("Same", "nothing new", Value::Null);
    same.as_object_mut().unwrap().remove("stop");
    assert_eq!(result(&core, third), same, "Same as it was before the field");
    assert!(core.tickets.m.tickets[&third].posts.is_empty(), "Same says nothing");

    // the fourth's Next with such a stop: its thought alone
    let fourth = ends(&mut core);
    let d = step(&mut core, &mut rx, fourth);
    let alone = format!("The lead still waits on {stills}. I send the stills before anything else.");
    answer(&mut core, &d, next(&alone, "told it the stills again", json!("I stop.")));
    assert_eq!(result(&core, fourth)["stop"], Value::Null);
    assert_eq!(posted(&core, fourth).text, Text::New(alone));

    // the record replays the four
    let replayed = crate::ticket::Tickets::load(&dir.display().to_string(), &base, Some(&dir.join("files").display().to_string()), 1);
    assert!(replayed.errors.is_empty(), "{:#?}", replayed.errors);
    for t in [first, second, third, fourth] {
        assert_eq!(replayed.m.tickets[&t].status, core.tickets.m.tickets[&t].status, "pass {t}");
    }
    kept_for_the_way_back(&dir, "brain-stop");
    let _ = std::fs::remove_dir_all(&dir);
}
