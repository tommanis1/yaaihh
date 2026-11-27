//! The brain's triggers (0.3; N-1444 p16, proto-lead N-1446 p9): the `turn_end` coordd sends at each turn end of an
//! agent while a binding is on it, carrying the policy's `check-in-every` for that agent, and counted by the agent's own
//! `every e.every of turn_end(e)` binding since it last fired. The `owner_to` the machine raises at the hand of the
//! owner's held post, and the `compacting` of an agent's compaction hook (proto-mind N-1446), go the same way.

use super::{Core, Task};
use crate::core::Who;
use docket_machine::syntax::{COMPACTING, OWNER_TO, TURN_END};
use docket_machine::{ASKED, Effect, Input, Loc, Text};
use serde_json::{Value, json};

struct Brain {
    core: Core,
    coord: std::path::PathBuf,
    project: std::path::PathBuf,
    rx: tokio::sync::mpsc::UnboundedReceiver<Task>,
}

impl Brain {
    /// A coord folder with a project and its agents w2 and w3, nothing bound on turn_end.
    fn new(tag: &str) -> Brain {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-brain-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        let project = coord.join("files");
        std::fs::create_dir_all(project.join("kinds/agents")).unwrap();
        std::fs::create_dir_all(project.join("policy/agents")).unwrap();
        std::fs::write(project.join("project.toml"), "[project]\nname = \"w\"\n").unwrap();
        std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
        let reg: String = ["w2", "w3"].iter().enumerate().map(|(i, n)| format!("{{\"ev\":\"add\",\"id\":\"AGT-00{i}\",\"name\":\"{n}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}}\n")).collect();
        std::fs::write(coord.join("registry.jsonl"), reg).unwrap();
        std::fs::write(coord.join("w2.jsonl"), "x".repeat(400)).unwrap();
        let core = Brain::load(&coord, &base);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut b = Brain { core, coord, project, rx };
        b.core.tickets.tasks = Some(tx);
        b
    }

    fn load(coord: &std::path::Path, base: &str) -> Core {
        let mut core = Core::load(&coord.display().to_string(), base);
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        core.load_tickets();
        assert!(core.tickets.errors.is_empty(), "{:?}", core.tickets.errors);
        core
    }

    /// AGENT's turn ended with its last message the K-th place of its transcript; the events it sent, applied.
    fn turn(&mut self, agent: &str, k: u64) {
        let loc = Loc { file: self.coord.join("w2.jsonl").display().to_string(), offset: k * 10, len: 5 };
        self.core.ticket_turn_ended(agent, loc);
        self.sent();
    }

    /// The events coordd sent the machine, applied as its task runner applies them.
    fn sent(&mut self) {
        while let Ok(t) = self.rx.try_recv() {
            if let Task::Event { name, value } = t {
                // an agent no binding counts writes nothing: the machine refuses it
                let _ = self.core.ticket_input(Input::Event { name, value });
            }
        }
    }

    /// The record's event lines named NAME.
    fn events(&self, name: &str) -> Vec<Value> {
        let log = std::fs::read_to_string(self.coord.join("tickets/log.jsonl")).unwrap_or_default();
        log.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|l| l["ev"] == "event" && l["name"] == name).collect()
    }

    /// The record's turn_end lines.
    fn ends(&self) -> Vec<Value> {
        self.events(TURN_END)
    }

    /// The tickets BINDING opened.
    fn opened(&self, binding: &str) -> usize {
        self.core.tickets.m.tickets.values().filter(|x| x.binding.as_deref() == Some(binding)).count()
    }

    fn reload(&mut self) {
        let r = self.core.ticket_reload(&Who::Owner, None, "the brain's binding".into());
        assert!(r.ok, "{}", r.out);
    }
}

impl Drop for Brain {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.coord);
    }
}

const NOTED: &str = "kind noted\n  form agent: Agent\n  result ()\n  pattern\n    return ()\n";
const BINDING: &str = "every e.every of turn_end(e) where e.agent == \"w2\" open noted(e.agent)";

/// With no binding on turn_end a turn's end writes nothing. A reload brings w2's own binding: from the next turn end
/// each of w2's turns is one line carrying the policy's N, counted, a turn given nothing too, and the same end seen
/// twice once; it fires at N since it last fired; w3's turns write nothing. A start replays it as it was and writes
/// nothing; a reload that drops the binding stops the lines.
#[test]
fn an_agents_turns_are_counted_by_its_own_binding_at_the_policys_n_from_the_reload_that_brings_it() {
    let mut b = Brain::new("count");
    b.turn("w2", 1);
    assert!(b.ends().is_empty(), "nothing bound: no line");
    std::fs::write(b.project.join("kinds/agents/w2.kind"), format!("{NOTED}\n{BINDING}\n")).unwrap();
    std::fs::write(b.project.join("policy/agents/w2.eflint"), "+check-in-every(\"w2\", 2).\n").unwrap();
    b.reload();
    b.turn("w2", 2);
    let ends = b.ends();
    assert_eq!(ends.len(), 1, "{ends:?}");
    assert_eq!((ends[0]["value"]["agent"].as_str(), ends[0]["value"]["every"].as_i64(), ends[0]["counted"].clone()), (Some("w2"), Some(2), serde_json::json!([BINDING])));
    assert!(ends[0]["value"]["text"].is_object(), "its last message by place: {}", ends[0]);
    assert_eq!(b.opened(BINDING), 0, "1 of 2");
    // the same end, taken again from the transcript: counted once
    b.turn("w2", 2);
    assert_eq!(b.ends().len(), 1);
    // another agent's turn: its `where` does not hold, so nothing is written
    b.turn("w3", 3);
    assert_eq!(b.ends().len(), 1);
    b.turn("w2", 4);
    assert_eq!((b.ends().len(), b.opened(BINDING)), (2, 1), "2 of 2: it fires");
    assert_eq!(b.core.tickets.m.counts[BINDING], 0);
    b.turn("w2", 5);
    assert_eq!((b.ends().len(), b.opened(BINDING), b.core.tickets.m.counts[BINDING]), (3, 1, 1));
    // a start replays the count and the tickets from the record, and writes nothing
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let again = Brain::load(&b.coord, &base);
    assert_eq!(again.tickets.m.counts, b.core.tickets.m.counts);
    assert_eq!(again.tickets.m.tickets.keys().collect::<Vec<_>>(), b.core.tickets.m.tickets.keys().collect::<Vec<_>>());
    assert_eq!(b.ends().len(), 3);
    // the binding taken out by a reload: the next turn end writes nothing
    std::fs::write(b.project.join("kinds/agents/w2.kind"), format!("{NOTED}\n")).unwrap();
    b.reload();
    b.turn("w2", 6);
    assert_eq!(b.ends().len(), 3);
}

/// The machine's `owner_to` (raised at the hand of the owner's held post, docket-machine's
/// `the_owners_held_post_raises_owner_to_at_the_hand_for_each_agent_it_wakes`) and the compaction hook's `compacting`
/// go through coordd as its own events do: with no binding on them nothing is written; once a reload brings one, each
/// is a recorded line that opens the binding's ticket. A compaction raises `compacting` once, however often its hook
/// comes before it is over; the next compaction raises it again.
#[test]
fn owner_to_and_compacting_are_recorded_lines_while_a_binding_is_on_them() {
    let mut b = Brain::new("owner-to");
    let to = json!({"agent": "w2", "posts": ["N-1.p3"], "tied_at": "N-1.p3"});
    let raise = |b: &mut Brain| {
        b.core.ticket_effects(vec![Effect::Event { name: OWNER_TO.into(), value: to.clone() }]);
        b.core.hook("w2", "compact", &json!({}));
        b.sent();
    };
    let over = |b: &mut Brain| b.core.agents.get_mut("w2").unwrap().live.compacting = false;
    raise(&mut b);
    assert!(b.events(OWNER_TO).is_empty() && b.events(COMPACTING).is_empty(), "nothing bound: no line");
    assert!(b.core.agents["w2"].live.compacting, "the compaction is shown all the same");
    over(&mut b);
    let brain = "on owner_to(e) open noted(e.agent)\non compacting(e) open noted(e.agent)\n";
    std::fs::write(b.project.join("kinds/agents/w2.kind"), format!("{NOTED}\n{brain}")).unwrap();
    b.reload();
    raise(&mut b);
    let (owner_to, compacting) = (b.events(OWNER_TO), b.events(COMPACTING));
    assert_eq!(owner_to.len(), 1, "{owner_to:?}");
    assert_eq!((owner_to[0]["value"].clone(), owner_to[0]["fired"].as_array().map(Vec::len)), (to.clone(), Some(1)));
    assert_eq!(compacting.len(), 1, "{compacting:?}");
    assert_eq!((compacting[0]["value"].clone(), compacting[0]["fired"].as_array().map(Vec::len)), (json!({"agent": "w2"}), Some(1)));
    assert_eq!(b.opened("on owner_to(e) open noted(e.agent)") + b.opened("on compacting(e) open noted(e.agent)"), 2);
    // its hook again before the compaction is over: no second line
    b.core.hook("w2", "compact", &json!({}));
    b.sent();
    assert_eq!(b.events(COMPACTING).len(), 1);
    over(&mut b);
    b.core.hook("w2", "compact", &json!({}));
    b.sent();
    assert_eq!(b.events(COMPACTING).len(), 2, "the next compaction");
}

/// A goal for each ask of his, opened by the binding on `asked` (N-1511 p47).
const GOAL: &str = "kind goal\n  form post: Ref, title: Text\n  result ()\n  pattern\n    return ()\n\non asked(e) where e.by == \"owner\" open goal(e.post, e.title)\n";
const GOAL_ON: &str = "on asked(e) where e.by == \"owner\" open goal(e.post, e.title)";
/// A head handed in for a post (`coord head --for`), done at once: at the first for his post `asked` is raised.
const HEAD: &str = "kind head\n  form commit: Ref, for: Text\n  result ()\n  pattern\n    return ()\n";

/// His ask taken (N-1511 p47; paper-research N-1446 p2204, p2354): coordd sends `asked` naming his post at the first
/// head handed in for it, not at the post. With no binding on it nothing is written, a head or none; once a reload
/// brings one, his next ask writes nothing until w2 hands in a head for it, which is one `asked` line with the
/// ledger's value that opens its goal. A second head for it, and the next head for his ask from before the binding
/// (its first came with none), raise nothing; w2's posts raise nothing, and a start replays the goal from the lines.
#[test]
fn his_ask_is_one_asked_line_at_its_first_head_while_a_binding_is_on_it() {
    let mut b = Brain::new("asked");
    let his = |b: &mut Brain, text: &str| -> u64 {
        let form = json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null});
        let open = Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form, text: Some(Text::New(text.into())), names: vec!["w2".into()], told: vec![] };
        b.core.ticket_input(open).unwrap();
        let t = *b.core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        b.sent();
        t
    };
    let w2 = |b: &mut Brain, t: u64| {
        b.core.ticket_input(Input::Post { t, by: "w2".into(), text: Text::New("on it".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        b.sent();
    };
    let head = |b: &mut Brain, by: &str, post: &str| {
        b.core.ticket_input(Input::Open { kind: "head".into(), under: None, opener: by.into(), form: json!({"commit": "1a2b3c4d", "for": post}), text: None, names: vec!["w3".into()], told: vec![] }).unwrap();
        b.sent();
    };
    std::fs::write(b.project.join("kinds/head.kind"), HEAD).unwrap();
    b.reload();
    let before = his(&mut b, "is the build green?");
    let pb = format!("{}.p1", b.core.tickets.m.id(before));
    head(&mut b, "w2", &pb);
    assert!(b.events(ASKED).is_empty(), "nothing bound: no line");
    std::fs::write(b.project.join("kinds/goal.kind"), GOAL).unwrap();
    b.reload();
    let t = his(&mut b, "and the stills?");
    let p = format!("{}.p1", b.core.tickets.m.id(t));
    assert!(b.events(ASKED).is_empty(), "at his post: no line");
    head(&mut b, "w2", &p);
    let asked = b.events(ASKED);
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!((asked[0]["value"].clone(), asked[0]["fired"].as_array().map(Vec::len)), (json!({"post": p, "by": "owner", "to": ["w2"], "title": "and the stills?"}), Some(1)));
    assert_eq!(b.opened(GOAL_ON), 1);
    head(&mut b, "w3", &p);
    head(&mut b, "w3", &pb);
    w2(&mut b, t);
    w2(&mut b, before);
    assert_eq!(b.events(ASKED).len(), 1);
    let again = Brain::load(&b.coord, &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
    assert_eq!(again.tickets.m.tickets.values().filter(|x| x.binding.as_deref() == Some(GOAL_ON)).count(), 1);
    assert_eq!(b.events(ASKED).len(), 1, "a start writes nothing");
}

/// His ask's goal whose hold reads its duties, a head handed in for his ask and its read (N-1511 p47; paper-research
/// N-1446 p2139, 2), and the policy that owes the read: taken by the agent his ask asks, met by a reader's Right.
const DUTY_KINDS: &str = "kind goal
  form post: Ref, title: Text
  result Done(by: [Ref]) | Broken(duty: Text?, line: Ref?) | Dropped(why: Text)
  pattern
    hold w3 close_goal(title) on duties(e) where e.goal == this and e.owed == [] => Done(e.by) on duties(e) where e.goal == this and e.broken != none => Broken(e.broken.duty, e.broken.line)

kind head
  form commit: Ref, for: Text
  result Text
  pattern
    r <- step w3 read(commit);
    match r { Right(why) => return why, Hold(point) => return point }

on asked(e) where e.by == \"owner\" open goal(e.post, e.title)
";
const DUTY_CALLS: &str = "call close_goal(title: Text) -> Done(by: [Ref]) | Broken(duty: Text?, line: Ref?) | Dropped(why: Text)\n  template\n    Close {{ title }}.\n\ncall read(commit: Ref) -> Right(why: Text) | Hold(point: Text)\n  template\n    Read {{ commit }}.\n";
const DUTY_POLICY: &str = "Fact agent Identified by String
Fact role-name Identified by String
Fact role Identified by agent * role-name
Fact goal Identified by String
Act take
  Actor agent
  Related to goal
  Creates owes-read(agent, agent', goal) When role(agent', \"owner\")
Act right
  Actor agent
  Recipient agent'
  Related to goal
  Holds when agent != agent'
  Terminates owes-read(agent', agent'', goal) When owes-read(agent', agent'', goal) && agent != agent'
Duty owes-read
  Holder agent
  Claimant agent'
  Related to goal
.
";

/// `DUTY_POLICY` with answers' duties asked too (`violated::declared`: `answering`, and a duty with `Violated when`).
const DUTY_ANSWERING: &str = "Fact agent Identified by String
Fact role-name Identified by String
Fact role Identified by agent * role-name
Fact goal Identified by String
Fact turn-ended Identified by agent
Fact call-name Identified by String
Fact head-name Identified by String
Fact answering Identified by agent * call-name * head-name
Act take
  Actor agent
  Related to goal
  Creates owes-read(agent, agent', goal) When role(agent', \"owner\")
Act right
  Actor agent
  Recipient agent'
  Related to goal
  Holds when agent != agent'
  Terminates owes-read(agent', agent'', goal) When owes-read(agent', agent'', goal) && agent != agent'
Duty owes-read
  Holder agent
  Claimant agent'
  Related to goal
  Violated when turn-ended(agent)
.
";

/// The tasks coordd made, run as its task runner runs them (the events and the goals' duties; no other here), each
/// answer put in, until none is left.
fn settle(b: &mut Brain, engine: &crate::policy::Engine) {
    while let Ok(task) = b.rx.try_recv() {
        let inputs = match task {
            Task::Event { name, value } => vec![Input::Event { name, value }],
            t @ Task::Duties { .. } => super::run_task(engine, t),
            _ => Vec::new(),
        };
        for i in inputs {
            let _ = b.core.ticket_input(i);
        }
    }
}

/// His ask's goal asks its duties of the project's policy/duties.eflint (N-1511 p47; paper-research N-1446 p2139, 2,
/// p2204): his post alone opens none; the head w2 hands in for it does (p2354), and at its hold the read is owed by
/// w2, whom his ask asks; at w3's Right on that head, met, and the hold is Done with that answer's line. Each a
/// `decided` line of eflint-rs's answer, two in all, none for w2's post, saying who holds each duty owed and which line
/// met each met (p2421), the latest on the goal's view with its line's time (p2328, 2). A start replays the same and
/// asks nothing.
#[test]
fn his_asks_goal_reads_its_duties_from_the_policy_at_the_lines_that_touch_it() {
    let mut b = Brain::new("duties");
    std::fs::create_dir_all(b.project.join("calls")).unwrap();
    std::fs::write(b.project.join("calls/goal.call"), DUTY_CALLS).unwrap();
    std::fs::write(b.project.join("kinds/goal.kind"), DUTY_KINDS).unwrap();
    std::fs::write(b.project.join("policy/duties.eflint"), DUTY_POLICY).unwrap();
    b.reload();
    let engine = crate::policy::Engine::start(b.core.places());
    let decided = |b: &Brain| -> Vec<Value> {
        let log = std::fs::read_to_string(b.coord.join("tickets/log.jsonl")).unwrap_or_default();
        log.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|l| l["ev"] == "decided" && l["question"] == "duties").collect()
    };
    let form = json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null});
    b.core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form, text: Some(Text::New("the page".into())), names: vec!["w2".into()], told: vec![] }).unwrap();
    let thread = *b.core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
    settle(&mut b, &engine);
    let p = format!("{}.p1", b.core.tickets.m.id(thread));
    assert!(b.core.tickets.m.tickets.values().all(|x| x.kind != "goal"), "his post alone opens no goal");
    b.core.ticket_input(Input::Open { kind: "head".into(), under: None, opener: "w2".into(), form: json!({"commit": "1a2b3c4d", "for": p}), text: None, names: vec![], told: vec![] }).unwrap();
    let h = *b.core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
    settle(&mut b, &engine);
    let g = b.core.tickets.m.tickets.values().find(|x| x.kind == "goal").map(|x| x.t).expect("the head handed in for his ask opened its goal");
    let gid = b.core.tickets.m.id(g);
    let d = decided(&b);
    assert_eq!(d.len(), 1, "{d:?}");
    let first = json!({"goal": gid, "owed": ["owes-read"], "holders": [{"duty": "owes-read", "holder": "w2"}], "by": [], "met": [], "broken": null});
    assert_eq!(d[0]["answer"], first, "{}", d[0]["provenance"]);
    assert!(b.core.tickets.m.awaits_duties(g).is_some());
    b.core.ticket_input(Input::Post { t: thread, by: "w2".into(), text: Text::New("on it".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    settle(&mut b, &engine);
    assert_eq!(decided(&b).len(), 1, "w2's post touches no goal");
    b.core.ticket_input(Input::Answer { t: h, by: "w3".into(), head: json!({"head": "Right", "why": "fine"}), text: Text::New("right".into()) }).unwrap();
    settle(&mut b, &engine);
    let d = decided(&b);
    assert_eq!((d.len(), &d[1]["answer"]["owed"]), (2, &json!([])), "{d:?}");
    let by = d[1]["answer"]["by"].clone();
    assert!(by.as_array().is_some_and(|b| b.len() == 1 && b[0].as_str().is_some_and(|l| l.ends_with(" answer by w3"))), "{by}");
    assert_eq!((&d[1]["answer"]["holders"], &d[1]["answer"]["met"]), (&json!([]), &json!([{"duty": "owes-read", "holder": "w2", "line": by[0]}])), "the read met by w3's Right");
    let seen = b.core.tickets.m.view(g).and_then(|v| v.duties).expect("its latest duties on its view, kept once it closed");
    assert_eq!((&seen["at"], &seen["met"], &seen["owed"]), (&d[1]["at"], &d[1]["answer"]["met"], &json!([])), "{seen}");
    assert!(matches!(&b.core.tickets.m.tickets[&g].status, docket_machine::Status::Done(v) if v["head"] == "Done" && v["by"] == by), "{:?}", b.core.tickets.m.tickets[&g].status);
    assert_eq!(b.core.tickets.m.duties_asked, 2);
    assert!(b.core.tickets.asks.is_empty(), "no ask runs on");
    let again = Brain::load(&b.coord, &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
    assert_eq!((again.tickets.m.duties_asked, again.tickets.m.view(g)), (2, b.core.tickets.m.view(g)));
    assert_eq!(decided(&b).len(), 2, "a start asks nothing");
}

/// A goal's `duties` ask and a `violated` ask of the same step each have their own stop (paper-research N-1446 p2251):
/// w3 drops the goal by hand while its duties ask runs, and the policy asks answers' duties too, so the drop is put to
/// it. Both asks are started on step 1; the duties ask's stop is set there, its goal no longer awaiting them, and the
/// violated one's stands until its answer, which takes the drop. Its run, given up, is answered null and changes
/// nothing; then no ask is left.
#[test]
fn a_goals_duties_ask_is_stopped_at_its_own_end_while_a_violated_ask_of_its_step_runs() {
    let mut b = Brain::new("two-asks");
    std::fs::create_dir_all(b.project.join("calls")).unwrap();
    std::fs::write(b.project.join("calls/goal.call"), DUTY_CALLS).unwrap();
    std::fs::write(b.project.join("kinds/goal.kind"), DUTY_KINDS).unwrap();
    std::fs::write(b.project.join("policy/duties.eflint"), DUTY_ANSWERING).unwrap();
    b.reload();
    assert!(b.core.tickets.m.defs.duties, "answers' duties are asked");
    let engine = crate::policy::Engine::start(b.core.places());
    let form = json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null});
    b.core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form, text: Some(Text::New("the page".into())), names: vec!["w2".into()], told: vec![] }).unwrap();
    let thread = *b.core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
    let p = format!("{}.p1", b.core.tickets.m.id(thread));
    b.core.ticket_input(Input::Open { kind: "head".into(), under: None, opener: "w2".into(), form: json!({"commit": "1a2b3c4d", "for": p}), text: None, names: vec![], told: vec![] }).unwrap();
    // the events only: the goal the head opens enters its hold, and its duties ask is started, not run
    let mut asks = Vec::new();
    while let Ok(task) = b.rx.try_recv() {
        match task {
            Task::Event { name, value } => {
                let _ = b.core.ticket_input(Input::Event { name, value });
            }
            t @ (Task::Duties { .. } | Task::Violated { .. }) => asks.push(t),
            _ => {}
        }
    }
    let g = b.core.tickets.m.tickets.values().find(|x| x.kind == "goal").map(|x| x.t).expect("his ask opened its goal");
    let [Task::Duties { step: 1, stop: duties_stop, .. }] = &asks[..] else { panic!("{asks:?}") };
    let duties_stop = duties_stop.clone();
    assert_eq!(b.core.tickets.asks.keys().cloned().collect::<Vec<_>>(), [(g, 1, "duties")]);
    // w3 drops it by hand: the drop is put to the policy; the duties ask ends, the violated one stands
    b.core.ticket_input(Input::Answer { t: g, by: "w3".into(), head: json!("Dropped"), text: Text::New("he took it back".into()) }).unwrap();
    while let Ok(task) = b.rx.try_recv() {
        if matches!(task, Task::Violated { .. }) {
            asks.push(task);
        }
    }
    let [_, Task::Violated { step: 1, stop: violated_stop, .. }] = &asks[..] else { panic!("{asks:?}") };
    assert!(b.core.tickets.m.asks_violated(g, 1));
    assert!(duties_stop.load(std::sync::atomic::Ordering::Relaxed), "the duties ask is stopped: its goal no longer awaits them");
    assert!(!violated_stop.load(std::sync::atomic::Ordering::Relaxed), "the violated ask runs on");
    assert_eq!(b.core.tickets.asks.keys().cloned().collect::<Vec<_>>(), [(g, 1, "violated")]);
    // the duties run gives up: null, which changes nothing; the violated answer takes the drop
    let mut asks = asks.into_iter();
    for i in super::run_task(&engine, asks.next().unwrap()) {
        let _ = b.core.ticket_input(i);
    }
    assert!(b.core.tickets.m.asks_violated(g, 1));
    for i in super::run_task(&engine, asks.next().unwrap()) {
        b.core.ticket_input(i).unwrap();
    }
    assert!(matches!(&b.core.tickets.m.tickets[&g].status, docket_machine::Status::Done(v) if v["head"] == "Dropped"), "{:?}", b.core.tickets.m.tickets[&g].status);
    assert!(b.core.tickets.asks.is_empty(), "no ask runs on");
}

/// Leg A of head 3a, as 75 raises `asked` (paper-research N-1446 p2209, p2222, p2354): a coord's record (REPLAY_COORD,
/// a copy; REPLAY_BASE; REPLAY_PROJECT, research's with a binding on `asked` in its kinds/goals.kind) replayed as it
/// is, nothing written. At each input that hands in the first head for a post of his (`graph::Builder::first_handed`,
/// where coordd raises `asked`), the machine has its `asked` value and the binding would fire once there, an item
/// whose `ask` is that post. No post comes twice, each at an input that opens a ticket `--for` a post, and the replay
/// has no problem line. Printed: his asks in Open → done (`Graph::asks_now`), the goals by day, the last ten.
#[test]
#[ignore]
fn his_asks_on_a_record_would_each_open_one_goal_at_their_first_head() {
    let (Ok(coord), Ok(base)) = (std::env::var("REPLAY_COORD"), std::env::var("REPLAY_BASE")) else { return };
    let places = crate::policy::Places { base, project: std::env::var("REPLAY_PROJECT").ok(), coord: coord.clone(), ..Default::default() };
    let (defs, errors, ..) = super::Tickets::read_defs(&places, 1);
    let binding = defs.bindings.iter().find(|b| b.ev == ASKED).map(|b| b.text.clone());
    println!("leg A: {coord}; the binding on asked: {binding:?}; {} definitions refused", errors.len());
    let binding = binding.expect("research's kinds/goals.kind binds `asked`");
    let mut unread = Vec::new();
    let (_, lines) = super::Tickets::read_log(&format!("{coord}/tickets/log.jsonl"), &mut unread);
    let mut m = docket_machine::Machine::load(defs);
    let mut b = docket_machine::graph::Builder::default();
    let mut seen = std::collections::BTreeSet::new();
    let (mut twice, mut none, mut opens, mut wrong, mut unhanded) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let started = std::time::Instant::now();
    let problems = m.replay_with(&lines, &mut |m, i, group| {
        b.after(m, i, group);
        for (t, n) in b.first_handed.drain(..) {
            let post = docket_machine::graph::post_id(m, t, n);
            if !seen.insert(post.clone()) {
                twice.push(post.clone());
            }
            let shown: Vec<Value> = group.iter().filter_map(|l| serde_json::to_value(l).ok()).collect();
            if !shown.iter().any(|l| l["ev"] == "open" && l["form"]["for"].is_string()) {
                unhanded.push(post.clone());
            }
            let Some(v) = m.asked_value(t, n) else {
                none.push(post);
                continue;
            };
            let at = shown.first().and_then(|l| l["at"].as_str().map(str::to_string)).unwrap_or_default();
            let (fired, _, why) = m.fire_bindings(&at, ASKED, &v);
            match &fired[..] {
                [f] if f.kind == "item" && f.binding == binding && f.form["ask"] == json!(post) => opens.push((at, post)),
                _ => wrong.push(format!("{post} at {at}: fired {:?}, {why:?}", fired.iter().map(|f| (&f.kind, &f.form)).collect::<Vec<_>>())),
            }
        }
    });
    let asks: Vec<String> = b.graph.asks_now(None).into_iter().map(|a| a.times.post).collect();
    let opened: std::collections::BTreeSet<&String> = opens.iter().map(|(_, p)| p).collect();
    let listed: std::collections::BTreeSet<&String> = asks.iter().collect();
    println!("leg A: {} lines replayed in {:.0} s, {} problem lines, {} unread", lines.len(), started.elapsed().as_secs_f64(), problems.len(), unread.len());
    println!("leg A: first heads {}; each with its value, one item opened: {}; his, no value: {}; fired otherwise: {}; twice: {}; at no hand-in: {}", seen.len(), opens.len(), none.len(), wrong.len(), twice.len(), unhanded.len());
    println!("leg A: Open → done's asks {}, of which a goal opened {}; opened and not listed {}", asks.len(), listed.intersection(&opened).count(), opened.difference(&listed).count());
    let mut by_day: std::collections::BTreeMap<&str, usize> = Default::default();
    for (at, _) in &opens {
        *by_day.entry(at.get(..10).unwrap_or(at)).or_default() += 1;
    }
    println!("leg A: goals by day {by_day:?}");
    for (at, p) in opens.iter().rev().take(10) {
        println!("  opens: {p} at {at}");
    }
    for p in opened.difference(&listed).take(10) {
        println!("  opened, not in Open → done: {p}");
    }
    for w in none.iter().chain(&wrong).chain(&twice).chain(&unhanded).take(20) {
        println!("  not one open: {w}");
    }
    assert_eq!((problems.len(), twice.len(), none.len(), wrong.len(), unhanded.len()), (0, 0, 0, 0, 0));
}

/// Leg B of head 3a (paper-research N-1446 p2253, 4; p2255, 4): p47's walk of his N-1104.p863, live on a copy of
/// research's record cut once that post asks (legb.py's LEG_COORD, in the leg's own folder, its project research's
/// with 3a's kinds/goals.kind and policy/duties.eflint). Each line of LEG_WALK is one input put in at its own time (the
/// test's clock), the record's line (its number there) or an added one (and from where): the post's `asked`, the job
/// lines on 10f43a97's tree, the head handed in for the post with its `green` and `reader` decided, paper-research's
/// Right, his window on it. coordd's tasks are run as its task runner runs them, the events, and on that policy who
/// fills a role, the holds, `violated` and each goal's `duties`; a `decide` and a decision among agents are the walk's.
/// The agents the walk names (paper-research, docket-edge8) ran in that hour: marked running, as assign_tests do.
/// After each line, the goal's owed or how it closed, and the head's state. Written in LEG_COORD only.
#[test]
#[ignore]
fn his_n1104_p863_walks_to_its_goals_close_on_a_copy_of_researchs_record() {
    let (Ok(coord), Ok(base), Ok(walk)) = (std::env::var("LEG_COORD"), std::env::var("LEG_BASE"), std::env::var("LEG_WALK")) else { return };
    let started = std::time::Instant::now();
    let mut core = Core::load(&coord, &base);
    core.project = crate::project::Project::load(&coord).unwrap();
    core.load_tickets();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    core.tickets.tasks = Some(tx);
    let engine = crate::policy::Engine::start(core.places());
    let running: Vec<&str> = ["paper-research", "docket-edge8"].into_iter().filter(|a| core.agents.get_mut(*a).map(|g| g.live.pid = Some(1)).is_some()).collect();
    println!("leg B: running in the leg (they posted in the walk's hour): {running:?}");
    let p863 = json!("N-1104.p863");
    println!("leg B: {coord} loaded in {:.0} s, {} problem lines; N-1104.p863 asks {:?}, its value {:?}", started.elapsed().as_secs_f64(), core.tickets.errors.len(), core.tickets.m.asked(1104, 863), core.tickets.m.asked_value(1104, 863));
    let goal = |core: &Core| core.tickets.m.tickets.values().filter(|x| x.kind == "item" && x.form["ask"] == p863).map(|x| x.t).collect::<Vec<u64>>();
    let mut head: Option<u64> = None;
    for (k, l) in std::fs::read_to_string(&walk).unwrap().lines().enumerate() {
        let e: Value = serde_json::from_str(l).unwrap();
        let at = e["at"].as_str().unwrap().to_string();
        crate::core::TEST_NOW.with(|n| *n.borrow_mut() = Some(at.clone()));
        let line = &e["line"];
        let input = match e["input"].as_str().unwrap() {
            "asked" => Input::Event { name: ASKED.into(), value: json!({"post": p863}) },
            "act" => {
                let more = line.as_object().unwrap().iter().filter(|(f, _)| !["ev", "at", "agent", "is", "text", "t"].contains(&f.as_str())).map(|(f, v)| (f.clone(), v.clone())).collect();
                Input::Act { agent: line["agent"].as_str().unwrap().into(), is: line["is"].as_str().unwrap().into(), text: line["text"].as_str().unwrap_or_default().into(), more, t: line["t"].as_u64() }
            }
            "event" => Input::Event { name: line["name"].as_str().unwrap().into(), value: line["value"].clone() },
            "open" => Input::Open { kind: line["kind"].as_str().unwrap().into(), under: line["under"].as_u64(), opener: line["opener"].as_str().unwrap().into(), form: line["form"].clone(), text: None, names: vec![], told: vec![] },
            "decided" => {
                let h = head.expect("the head is open");
                let docket_machine::Status::Asking { owed, .. } = &core.tickets.m.tickets[&h].status else { panic!("{:?}", core.tickets.m.tickets[&h].status) };
                Input::Decided { t: h, step: owed.step, question: "decide".into(), answer: line["answer"].clone(), provenance: json!({"added": e["from"]}) }
            }
            "answer" => Input::Answer { t: head.expect("the head is open"), by: line["by"].as_str().unwrap().into(), head: line["head"].clone(), text: Text::New(line["text"].as_str().unwrap_or_default().into()) },
            other => panic!("no input {other}"),
        };
        let opening = matches!(&input, Input::Open { .. });
        let said = core.ticket_input(input);
        if opening {
            head = said.as_ref().ok().and_then(|s| docket_machine::parse_ticket(s.trim_start_matches("Opened ").trim_end_matches('.')));
        }
        let mut read = Vec::new();
        while let Ok(task) = rx.try_recv() {
            let inputs = match task {
                Task::Event { name, value } => vec![Input::Event { name, value }],
                t @ (Task::Fills { .. } | Task::Holds { .. } | Task::Violated { .. } | Task::Duties { .. }) => super::run_task(&engine, t),
                _ => Vec::new(),
            };
            for i in inputs {
                if let Input::Decided { question, answer, .. } = &i {
                    if question == "duties" {
                        read.push(answer.clone());
                    }
                }
                let _ = core.ticket_input(i);
                // what that input made: the events, and a duties ask again
                while let Ok(task) = rx.try_recv() {
                    let more = match task {
                        Task::Event { name, value } => vec![Input::Event { name, value }],
                        t @ (Task::Fills { .. } | Task::Holds { .. } | Task::Violated { .. } | Task::Duties { .. }) => super::run_task(&engine, t),
                        _ => Vec::new(),
                    };
                    for i in more {
                        if let Input::Decided { question, answer, .. } = &i {
                            if question == "duties" {
                                read.push(answer.clone());
                            }
                        }
                        let _ = core.ticket_input(i);
                    }
                }
            }
        }
        let gs = goal(&core);
        let state = gs.iter().map(|g| match &core.tickets.m.tickets[g].status {
            docket_machine::Status::Done(v) => format!("{} closed {v}", core.tickets.m.id(*g)),
            _ if core.tickets.m.awaits_duties(*g).is_some() => format!("{} awaits its duties", core.tickets.m.id(*g)),
            s => format!("{} {}", core.tickets.m.id(*g), format!("{s:?}").chars().take(80).collect::<String>()),
        }).collect::<Vec<_>>();
        println!("leg B {k}: {at} {} ({}): {}", e["input"].as_str().unwrap_or_default(), e["from"].as_str().unwrap_or_default(), said.map_or_else(|e| format!("refused: {e}"), |s| s.chars().take(120).collect()));
        for r in &read {
            println!("  duties read: owed {}, by {}, broken {}", r["owed"], r["by"], r["broken"]);
        }
        println!("  goal: {state:?}");
        if let Some(h) = head {
            println!("  head: {} {}", core.tickets.m.id(h), format!("{:?}", core.tickets.m.tickets[&h].status).chars().take(100).collect::<String>());
        }
    }
    crate::core::TEST_NOW.with(|n| *n.borrow_mut() = None);
    let gs = goal(&core);
    println!("leg B: one goal for N-1104.p863: {gs:?}; duties asked {} times in all; {:.0} s", core.tickets.m.duties_asked, started.elapsed().as_secs_f64());
    assert_eq!(gs.len(), 1, "one goal for the post");
}
