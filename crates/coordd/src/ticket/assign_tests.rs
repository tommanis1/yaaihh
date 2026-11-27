//! The orchestrator's `assign` through coordd (item 11): the word `agents` filled in with the project's agents when
//! the step is handed over, the record keeping the marker (G1); an answer naming an agent that is not registered
//! refused with that reason (G5).

use super::Core;
use crate::core::Who;
use docket_machine::{Input, Line, Loc, Status};
use serde_json::{Value, json};

const CALLS: &str = "\
type Placed {
  piece: Text
  agent: Agent
  why: Text
}

call assign(words: Text, what: Text, agents: Agents, pieces: [Text]) -> Assigned(agent: Agent, why: Text) | Placed(picks: [Placed])
  template
    Who takes {{ what }} of {{ words }}? The agents:
    {{ agents }}

call go(words: Text) -> Go
  template
    Go on {{ words }}?
";

const KINDS: &str = "\
kind change
  letter C
  form words: Text
  result Text
  pattern
    d <- step lead assign(words, \"the change\", agents, [\"the design\"]);
    return words

kind job
  letter J
  form words: Text
  result Text
  pattern
    g <- step owner go(words);
    c <- open change(words);
    return c

kind quick
  letter Q
  form words: Text
  result Text
  pattern
    v <- step fresh \"Look at {{ words }}\" -> Seen;
    return words
";

struct Coord {
    core: Core,
    dir: std::path::PathBuf,
}

impl Drop for Coord {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Coord {
    fn new(tag: &str) -> Coord {
        Coord::with(tag, &[])
    }
    /// With MORE registered as well: claude agents the owner added.
    fn with(tag: &str, more: &[&str]) -> Coord {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let dir = std::env::temp_dir().join(format!("docket-assign-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let project = dir.join("files");
        for d in ["kinds", "calls"] {
            std::fs::create_dir_all(project.join(d)).unwrap();
        }
        let add = |n: &str, i: u32, k: &str| format!("{{\"ev\":\"add\",\"id\":\"AGT-00{i}\",\"name\":\"{n}\",\"parent\":\"owner\",\"kind\":\"{k}\",\"model\":\"opus\",\"at\":\"x\"}}\n");
        let role = "{\"ev\":\"role\",\"name\":\"lead\",\"role\":\"lead\",\"at\":\"x\"}\n";
        let more: String = more.iter().enumerate().map(|(i, n)| add(n, 4 + i as u32, "claude")).collect();
        std::fs::write(dir.join("registry.jsonl"), format!("{}{}{}{more}{role}", add("lead", 1, "claude"), add("ann", 2, "claude"), add("runner", 3, "program"))).unwrap();
        std::fs::write(project.join("calls/a.call"), CALLS).unwrap();
        std::fs::write(project.join("kinds/a.kind"), KINDS).unwrap();
        std::fs::write(project.join("project.toml"), "[project]\nname = \"a\"\n").unwrap();
        std::fs::write(dir.join("project"), project.join("project.toml").display().to_string()).unwrap();
        let mut core = Core::load(&dir.display().to_string(), &base);
        core.project = crate::project::Project::load(&dir.display().to_string()).unwrap();
        core.load_tickets();
        assert!(core.tickets.m.defs.kinds.contains_key("change"), "{:?}", core.tickets.m.defs.kinds.keys().collect::<Vec<_>>());
        Coord { core, dir }
    }
    fn open(&mut self) -> u64 {
        self.core.ticket_input(Input::Open { kind: "change".into(), under: None, opener: "owner".into(), form: json!({"words": "a cache"}), text: None, names: vec![], told: vec![] }).unwrap();
        *self.core.tickets.m.tickets.keys().find(|t| **t < docket_machine::HIDDEN).unwrap()
    }
    fn answer(&mut self, t: u64, head: Value) -> coord_types::Reply {
        let id = self.core.tickets.m.id(t);
        self.core.ticket_answer_request(&Who::Agent("lead".into()), &id, head, Some("picked".into()))
    }
}

#[test]
fn assign_is_handed_the_projects_agents_and_the_record_keeps_the_marker() {
    let mut c = Coord::new("hand");
    let t = c.open();
    let due = c.core.tickets.m.due("lead");
    assert_eq!(due.len(), 1, "{due:?}");
    c.core.agents.get_mut("lead").unwrap().live.pid = Some(1);
    let handed = c.core.ticket_handing("lead", due);
    let docket_machine::What::Step { args, .. } = &handed[0].0.what else { panic!("{:?}", handed[0].0.what) };
    let agents = args["agents"].as_str().unwrap_or_else(|| panic!("{args}"));
    // one line each for the agents that work (not the runner, a program): name, role, model, in a turn or free
    assert!(agents.lines().any(|l| l == "lead: lead, opus, free"), "{agents}");
    assert!(agents.lines().any(|l| l == "ann: member, opus, not running"), "{agents}");
    assert!(!agents.contains("runner"), "{agents}");
    // the rendered prompt has them
    let text = crate::render::delivery(&c.core.places(), &handed[0].0, &handed[0].1, true, true, false, false, None).unwrap().0;
    assert!(text.contains("ann: member, opus, not running"), "{text}");
    // the machine and the record keep the marker
    let Status::Awaits { owed, .. } = &c.core.tickets.m.tickets[&t].status else { panic!() };
    assert_eq!(owed.args["agents"], json!({"agents": "project"}));
    let log = std::fs::read_to_string(c.dir.join("tickets/log.jsonl")).unwrap();
    assert!(log.contains(r#""agents":{"agents":"project"}"#) && !log.contains("ann: member"), "{log}");
}

#[test]
fn an_answer_naming_an_agent_that_is_not_registered_is_refused_with_why() {
    let mut c = Coord::new("unreg");
    let t = c.open();
    for d in c.core.tickets.m.due("lead") {
        c.core.ticket_input(Input::Given { t: d.t, to: "lead".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Loc { file: "t.jsonl".into(), offset: 1, len: 1 }) }).unwrap();
    }
    let pick = |agent: &str| json!({"head": "Assigned", "agent": agent, "why": "it fits"});
    let r = c.answer(t, pick("zed"));
    assert!(!r.ok && r.out.contains("zed is not a registered agent"), "{}", r.out);
    // in a list of records too
    let r = c.answer(t, json!({"head": "Placed", "picks": [{"piece": "a", "agent": "ann", "why": "-"}, {"piece": "b", "agent": "yul", "why": "-"}]}));
    assert!(!r.ok && r.out.contains("yul is not a registered agent"), "{}", r.out);
    let Status::Awaits { refusals, .. } = &c.core.tickets.m.tickets[&t].status else { panic!() };
    assert_eq!(*refusals, 2);
    let r = c.answer(t, pick("ann"));
    assert!(r.ok, "{}", r.out);
    assert!(matches!(&c.core.tickets.m.tickets[&t].status, Status::Done(_)), "{:?}", c.core.tickets.m.tickets[&t].status);
    // the refusals are in the record with their reason
    let log = std::fs::read_to_string(c.dir.join("tickets/log.jsonl")).unwrap();
    let unfit: Vec<Line> = log.lines().filter_map(|l| serde_json::from_str::<Line>(l).ok()).filter(|l| matches!(l, Line::Unfit { .. })).collect();
    assert_eq!(unfit.len(), 2, "{log}");
}

#[test]
fn only_an_answer_the_machine_would_take_is_refused_for_an_unregistered_agent() {
    let mut c = Coord::new("which");
    let t = c.open();
    let zed = json!({"head": "Assigned", "agent": "zed", "why": "it fits"});
    // by another than the agent it awaits: the machine's own refusal, not a refusal of the step
    let r = c.core.ticket_answer_request(&Who::Agent("ann".into()), &c.core.tickets.m.id(t), zed.clone(), Some("picked".into()));
    assert!(!r.ok && !r.out.contains("registered"), "{}", r.out);
    // formed for another step, or with nothing being formed: ignored as the machine ignores it
    for step in [2, 1] {
        let r = c.core.ticket_input(Input::Formed { t, step, head: Some(zed.clone()), why: None, not_taken: None, call: None });
        assert!(r.as_ref().is_err_and(|e| e.contains("Not forming")), "{r:?}");
    }
    let Status::Awaits { refusals, .. } = &c.core.tickets.m.tickets[&t].status else { panic!() };
    assert_eq!(*refusals, 0);
}

#[test]
fn the_agents_listed_are_those_that_can_still_be_given_work() {
    // paper-research N-101 p595, p599: this thread's first, then the standing ones, then those of another open
    // thread, marked; a step's fresh agent and an agent of a closed thread left out, counted in one closing line
    // f3-1: an agent an earlier record made for its step, in the registry (taken: the third ticket's fresh step here
    // is f3-2)
    let mut c = Coord::with("which-agents", &["old1", "mid1", "f3-1", "dev1"]);
    let made = |c: &mut Coord, t: u64, agent: &str| {
        c.core.ticket_input(Input::Made { t: Some(t), agent: agent.into(), by: "lead".into(), from: None }).unwrap();
    };
    let newest = |c: &Coord| c.core.tickets.m.tickets.keys().copied().filter(|t| *t < docket_machine::HIDDEN).max().unwrap();
    // a closed thread, with the agent made for it
    c.open();
    let closed = newest(&c);
    made(&mut c, closed, "old1");
    let id = c.core.tickets.m.id(closed);
    for d in c.core.tickets.m.due("lead").into_iter().filter(|d| d.t == closed) {
        c.core.ticket_input(Input::Given { t: d.t, to: "lead".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Loc { file: "t.jsonl".into(), offset: 1, len: 1 }) }).unwrap();
    }
    let r = c.core.ticket_answer_request(&Who::Agent("lead".into()), &id, json!({"head": "Assigned", "agent": "ann", "why": "-"}), Some("picked".into()));
    assert!(r.ok, "{}", r.out);
    assert!(!c.core.tickets.m.tickets[&closed].is_open());
    // another open thread, with its agent
    c.open();
    let other = newest(&c);
    assert!(other != closed);
    made(&mut c, other, "mid1");
    // a thread whose step is given to a fresh agent of its own
    c.core.ticket_input(Input::Open { kind: "quick".into(), under: None, opener: "owner".into(), form: json!({"words": "a look"}), text: None, names: vec![], told: vec![] }).unwrap();
    let quick = newest(&c);
    assert!(c.core.tickets.m.tickets[&quick].live.contains(&"f3-2".to_string()), "{:?}", c.core.tickets.m.tickets[&quick].live);
    // this thread, with its agent
    c.open();
    let here = newest(&c);
    made(&mut c, here, "dev1");
    c.core.agents.get_mut("lead").unwrap().live.pid = Some(1);
    let due: Vec<_> = c.core.tickets.m.due("lead").into_iter().filter(|d| d.t == here).collect();
    assert_eq!(due.len(), 1, "{due:?}");
    let handed = c.core.ticket_handing("lead", due);
    let docket_machine::What::Step { args, .. } = &handed[0].0.what else { panic!("{:?}", handed[0].0.what) };
    let agents = args["agents"].as_str().unwrap_or_else(|| panic!("{args}"));
    let want = format!(
        "dev1: member, opus, not running, made for this thread\n\
         ann: member, opus, not running\n\
         lead: lead, opus, free\n\
         mid1: member, opus, not running, made for {}\n\
         (2 agents are left out: made for one step or for a thread now closed, or ended by the owner; `coord ls` lists every agent.)",
        c.core.tickets.m.id(other)
    );
    assert_eq!(agents, want);
    // the record replays to the same list
    c.core.load_tickets();
    let due: Vec<_> = c.core.tickets.m.due("lead").into_iter().filter(|d| d.t == here).collect();
    let handed = c.core.ticket_handing("lead", due);
    let docket_machine::What::Step { args, .. } = &handed[0].0.what else { panic!() };
    assert_eq!(args["agents"].as_str(), Some(want.as_str()));
}

#[test]
fn a_fresh_step_never_gets_an_agent_an_earlier_record_made() {
    // docket-core3 N-101 p609, paper-research p610: a record set aside, the registry kept with an earlier record's f1-1
    // (claude-opus, another conversation); the first ticket's fresh step would be named f1-1 again
    let mut c = Coord::with("taken", &["f1-1"]);
    let log = |c: &Coord| std::fs::read_to_string(c.dir.join("tickets/log.jsonl")).unwrap_or_default();
    assert_eq!(log(&c).lines().filter(|l| l.contains("\"ev\":\"taken\"")).count(), 1, "{}", log(&c));
    c.core.ticket_input(Input::Open { kind: "quick".into(), under: None, opener: "owner".into(), form: json!({"words": "a look"}), text: None, names: vec![], told: vec![] }).unwrap();
    let t = *c.core.tickets.m.tickets.keys().find(|t| **t < docket_machine::HIDDEN).unwrap();
    assert_eq!(t, 1);
    let x = &c.core.tickets.m.tickets[&t];
    assert_eq!(x.live, vec!["f1-2".to_string()], "the next free name");
    let made: Vec<Line> = log(&c).lines().filter_map(|l| serde_json::from_str::<Line>(l).ok()).filter(|l| matches!(l, Line::Agent { what, .. } if what == "made")).collect();
    assert!(matches!(made.as_slice(), [Line::Agent { agent, .. }] if agent == "f1-2"), "only the agent made is said made: {made:?}");
    // the old one is no option, and is not listed among the agents
    assert!(c.core.tickets.m.program_made("f1-1"));
    // a restart replays the same names and writes no second taken line
    c.core.load_tickets();
    assert_eq!(c.core.tickets.m.tickets[&t].live, vec!["f1-2".to_string()]);
    assert_eq!(log(&c).lines().filter(|l| l.contains("\"ev\":\"taken\"")).count(), 1, "{}", log(&c));
}

#[test]
fn an_agent_the_owner_ended_is_given_no_work_and_never_started() {
    // paper-research N-101 p644: a standing agent the owner ends for good (`coord ended NAME`) is recorded, left out
    // of `fills` and of the `agents` list, never started again, and only the owner takes the mark back
    let mut c = Coord::with("ended", &["mv-1"]);
    assert!(!c.core.set_ended(&Who::Agent("lead".into()), "mv-1", false).ok, "an agent does not end one");
    let r = c.core.set_ended(&Who::Owner, "mv-1", false);
    assert!(r.ok, "{}", r.out);
    let reg = std::fs::read_to_string(c.dir.join("registry.jsonl")).unwrap();
    assert!(reg.lines().last().is_some_and(|l| l.contains("\"ev\":\"ended\"") && l.contains("mv-1")), "{reg}");
    // the registry read again keeps it
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let again = Core::load(&c.dir.display().to_string(), &base);
    assert!(again.agents["mv-1"].ended.is_some());
    assert!(again.state_lines().out.lines().any(|l| l.starts_with("mv-1\t") && l.contains("\tended by the owner at ")));
    // never started again
    let e = c.core.run(&crate::cgroup::Caller::Owner, "mv-1", 1).unwrap_err();
    assert!(e.contains("ended by the owner"), "{e}");
    // no option for fills, not in the agents list
    let t = c.open();
    assert!(!c.core.fills_facts(t).iter().any(|(_, args)| args.iter().any(|a| a == "mv-1")));
    c.core.agents.get_mut("lead").unwrap().live.pid = Some(1);
    let list = |c: &mut Coord| {
        let due: Vec<_> = c.core.tickets.m.due("lead").into_iter().filter(|d| d.t == t).collect();
        let handed = c.core.ticket_handing("lead", due);
        let docket_machine::What::Step { args, .. } = &handed[0].0.what else { panic!() };
        args["agents"].as_str().unwrap().to_string()
    };
    let agents = list(&mut c);
    assert!(!agents.contains("mv-1:") && agents.contains("(1 agent is left out"), "{agents}");
    // the owner takes it back
    assert!(!c.core.set_ended(&Who::Agent("lead".into()), "mv-1", true).ok);
    assert!(c.core.set_ended(&Who::Owner, "mv-1", true).ok);
    assert!(list(&mut c).contains("mv-1: member"));
    assert!(Core::load(&c.dir.display().to_string(), &base).agents["mv-1"].ended.is_none());
}

#[test]
fn a_give_halt_shows_the_agents_as_the_step_would_be_handed_them() {
    // paper-research N-101 p1256: on Tovrin-dev the halted `assign` read `{"agents":"project"}`; the halt shows the
    // prompt that would be sent, so the agents are filled in there as at the hand-over, and the record keeps the marker
    let mut c = Coord::new("halt");
    c.core.ticket_input(Input::Open { kind: "job".into(), under: None, opener: "owner".into(), form: json!({"words": "a cache"}), text: None, names: vec![], told: vec![] }).unwrap();
    let job = *c.core.tickets.m.tickets.keys().find(|t| **t < docket_machine::HIDDEN).unwrap();
    c.core.ticket_input(Input::Slow { t: job, on: true, by: "owner".into() }).unwrap();
    c.core.ticket_input(Input::Answer { t: job, by: "owner".into(), head: json!({"head": "Go"}), text: docket_machine::Text::New(String::new()) }).unwrap();
    let change = *c.core.tickets.m.tickets.keys().filter(|t| **t < docket_machine::HIDDEN && **t != job).max().unwrap();
    c.core.ticket_input(Input::Approve { t: change, n: 1, by: "owner".into() }).unwrap();
    let v = c.core.ticket_shown(change).unwrap();
    let Some(docket_machine::ViewHalt::Give { prompt, .. }) = v.halts.iter().find(|h| matches!(h, docket_machine::ViewHalt::Give { .. })) else { panic!("{:?}", v.halts) };
    assert!(prompt.contains("The agents:\nann: member, opus, not running\nlead: lead, opus, not running"), "{prompt}");
    assert!(!prompt.contains("\"agents\""), "{prompt}");
    let Some((_, args, ..)) = c.core.tickets.m.halted_give(change, 2) else { panic!() };
    assert!(docket_machine::types::is_agents(&args["agents"]), "the record keeps the marker: {args}");
}

/// A post on its way to an agent (written to its session, no `given` line yet) is taken back all the same (0.2 B8,
/// N-1396; consult-3, N-818 p74): gone in the record, while coordd keeps which hand-over carried it (its `handed`, which
/// the record does not hold). Taken, the agent had the post: struck, and the agent told once, the program's item a
/// `taken_back` (delivery.kind puts it into its turn if one runs). Not taken: gone, nobody told.
#[test]
fn a_post_on_its_way_to_an_agent_is_struck_if_it_takes_it_and_gone_if_not() {
    let mut c = Coord::new("takeback");
    c.core.ticket_input(Input::Open { kind: "change".into(), under: None, opener: "owner".into(), form: json!({"words": "a cache"}), text: Some(docket_machine::Text::New("a cache".into())), names: vec![], told: vec![] }).unwrap();
    let t = *c.core.tickets.m.tickets.keys().find(|t| **t < docket_machine::HIDDEN).unwrap();
    let post = |c: &mut Coord, words: &str| c.core.ticket_input(Input::Post { t, by: "ann".into(), text: docket_machine::Text::New(words.into()), names: vec!["lead".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    post(&mut c, "use the old one");
    let id = c.core.tickets.m.id(t);
    let ann = Who::Agent("ann".into());
    c.core.agents.get_mut("lead").unwrap().live.pid = Some(1);
    let due = c.core.tickets.m.due("lead");
    assert!(due.iter().any(|d| d.t == t && d.upto >= 2), "{due:?}");
    let d = due.iter().find(|d| d.t == t && d.upto >= 2).unwrap().id.clone();
    c.core.ticket_handing("lead", due);
    let r = c.core.ticket_take_back(&ann, &id, 2);
    assert!(r.ok, "{}", r.out);
    let struck = |c: &Coord| c.core.ticket_shown(t).unwrap().struck.iter().map(|s| (s.post.n, s.to.clone())).collect::<Vec<_>>();
    assert!(c.core.tickets.m.tickets[&t].posts.iter().any(|p| p.n == 2 && p.taken_back) && struck(&c).is_empty(), "gone while on its way");
    // lead's session took it: lead had the post
    c.core.ticket_handed("lead", std::slice::from_ref(&d), true);
    assert_eq!(struck(&c), vec![(2, vec!["lead".to_string()])]);
    let notice = format!("{id}.p2.taken_back");
    let due = c.core.tickets.m.due("lead");
    let n = due.iter().find(|d| d.id == notice).unwrap_or_else(|| panic!("{due:?}"));
    assert!(matches!(&n.what, docket_machine::What::TakenBack { n: 2, by } if by == "ann"), "{:?}", n.what);
    // the program's item: delivery.kind's `arrived` puts a `taken_back` into a running turn, whatever it holds
    let waiting = c.core.tickets.m.waiting_of("lead", c.core.tickets.due_whole("lead"));
    let item = waiting.iter().find(|i| i["id"] == json!(notice)).unwrap_or_else(|| panic!("{waiting:?}"));
    assert_eq!((item["what"].as_str(), item["from"].as_str(), item["owner"].as_bool()), (Some("taken_back"), Some("ann"), Some(false)), "{item}");
    let view = c.core.ticket_shown(t).unwrap();
    let (text, meta) = crate::render::delivery(&c.core.places(), n, &view, false, false, false, false, None).unwrap();
    assert_eq!((text.as_str(), meta["id"].as_str()), (format!("ann took back {id}.p2; do not act on it.").as_str(), Some(notice.as_str())));
    // the record: gone, then struck for lead, the `event` kind an engine from before takes as nothing
    let log = std::fs::read_to_string(c.dir.join("tickets/log.jsonl")).unwrap();
    let tail: Vec<Value> = log.lines().rev().take(2).map(|l| serde_json::from_str(l).unwrap()).collect();
    assert!(tail[0]["ev"] == "event" && tail[0]["name"] == "taken_back" && tail[0]["value"]["to"] == json!(["lead"]), "{}", tail[0]);
    // a second post, its hand-over on its way, taken back, and the hand-over not taken: gone, nobody told
    post(&mut c, "and the new one");
    let due = c.core.tickets.m.due("lead");
    let d = due.iter().find(|d| d.t == t && d.upto >= 3).unwrap().id.clone();
    c.core.ticket_handing("lead", due);
    assert!(c.core.ticket_take_back(&ann, &id, 3).ok);
    c.core.ticket_handed("lead", std::slice::from_ref(&d), false);
    assert!(c.core.tickets.m.tickets[&t].posts.iter().any(|p| p.n == 3 && p.taken_back));
    assert_eq!(struck(&c), vec![(2, vec!["lead".to_string()])], "3 is gone");
    assert!(c.core.tickets.backs.is_empty(), "nothing kept for it");
    assert!(!c.core.tickets.m.due("lead").iter().any(|d| matches!(&d.what, docket_machine::What::TakenBack { n: 3, .. })));
}
