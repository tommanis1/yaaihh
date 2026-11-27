//! Agents made from an agent's turn, by the project's policy (item 11, paper-research N-101 p271): Tovrin's rule as
//! the sample policy.

use super::{Asked, allowed, plan, register};
use crate::core::{Core, Who};
use docket_machine::{Input, Loc};
use serde_json::json;

/// Tovrin's rules for agents, the sample a project writes (policy/assign.eflint, which replaces agent-base's).
pub const SAMPLE: &str = r#"// Who makes agents (Tovrin; item 11). The orchestrator makes a fresh agent, or a
// copy of one made in this thread: never a copy of an agent that existed before it. Nobody else makes agents.
Act make-agent
  Actor agent
  Recipient agent
  Related to source, thread
  Holds when role(actor, "orchestrator") and source == "none"
  Holds when role(actor, "orchestrator") and made(source, _, _, thread)
"#;

const CALLS: &str = "\
call assign(words: Text) -> Assigned(agent: Agent, why: Text)
  template
    Who designs {{ words }}?

call build(part: Text) -> Built(top: Text)
  template
    Build {{ part }}.
";

const KINDS: &str = "\
kind change
  letter C
  form words: Text
  result Text
  raises Exhausted, NoOrder
  pattern
    w <- step orch assign(words);
    tops <- par x in [\"a\", \"b\"] up to 4 as x after [] {
      b <- step bob build(x);
      match b { Built(top) => return top }
    };
    return words

kind laid
  letter L
  form words: Text
  result Text
  pattern
    layer on \"line\" {
      w <- step orch assign(words);
      return words
    }
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
    fn new(tag: &str, policy: Option<&str>) -> Coord {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let dir = std::env::temp_dir().join(format!("docket-make-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let project = dir.join("files");
        for d in ["kinds", "calls", "policy"] {
            std::fs::create_dir_all(project.join(d)).unwrap();
        }
        let add = |n: &str, i: u32, parent: &str| format!("{{\"ev\":\"add\",\"id\":\"AGT-00{i}\",\"name\":\"{n}\",\"parent\":\"{parent}\",\"kind\":\"claude\",\"model\":\"opus\",\"at\":\"x\"}}\n");
        let role = "{\"ev\":\"role\",\"name\":\"orch\",\"role\":\"orchestrator\",\"at\":\"x\"}\n";
        std::fs::write(dir.join("registry.jsonl"), format!("{}{}{}{}{role}", add("orch", 1, "owner"), add("ann", 2, "owner"), add("bob", 3, "owner"), add("old", 4, "orch"))).unwrap();
        std::fs::write(project.join("calls/m.call"), CALLS).unwrap();
        std::fs::write(project.join("kinds/m.kind"), KINDS).unwrap();
        if let Some(p) = policy {
            std::fs::write(project.join("policy/assign.eflint"), p).unwrap();
        }
        std::fs::write(project.join("project.toml"), "[project]\nname = \"m\"\n").unwrap();
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
    /// BY asks to make NEW (a copy of FROM when FORK): planned, asked, and registered when the policy lets it.
    fn make(&mut self, by: &str, new: &str, from: Option<&str>, fork: bool) -> Result<(), String> {
        let a = Asked { new: new.into(), from: from.map(str::to_string), model: None, text: "Design the cache.".into(), fork };
        let p = plan(&self.core, &Who::Agent(by.into()), a)?;
        allowed(&p)?;
        register(&mut self.core, &p)
    }
    fn give(&mut self, agent: &str, t: u64) {
        let d = self.core.ticket_due(agent).into_iter().find(|d| d.t == t).unwrap_or_else(|| panic!("nothing due to {agent} on {t}"));
        self.core.ticket_input(Input::Given { t: d.t, to: agent.into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Loc { file: "t.jsonl".into(), offset: 1, len: 1 }) }).unwrap();
    }
}

#[test]
fn by_agent_bases_rule_an_agent_copies_itself_and_makes_no_other() {
    // a project with no assign.eflint of its own (cav-paper, research): agent-base's make-agent
    let mut c = Coord::new("base", None);
    let t = c.open();
    c.give("orch", t);
    c.make("orch", "", None, true).unwrap();
    assert_eq!(c.core.agents["orch-fork"].parent, "orch");
    assert!(c.core.tickets.m.made().contains(&("orch-fork".into(), "orch".into(), Some("orch".into()), Some(c.core.tickets.m.thread_of(t)))));
    let e = c.make("orch", "dev1", None, false).unwrap_err();
    assert!(e.contains("does not let orch make dev1"), "{e}");
    let e = c.make("orch", "old2", Some("old"), true).unwrap_err();
    assert!(e.contains("does not let orch make old2"), "{e}");
    assert!(!c.core.agents.contains_key("dev1") && !c.core.agents.contains_key("old2"));
    // with no step owed: the thread is none, and a copy of itself is still made
    c.make("bob", "", None, true).unwrap();
    assert!(c.core.agents.contains_key("bob-fork"));
    // with no make-agent anywhere (a policy folder without agent-base's file): nobody makes agents
    std::fs::write(c.dir.join("files/policy/assign.eflint"), "// none\n").unwrap();
    let e = c.make("orch", "", None, true).unwrap_err();
    assert!(e.contains("has no make-agent"), "{e}");
}

#[test]
fn the_orchestrator_makes_a_new_agent_or_a_copy_of_one_made_in_this_thread() {
    let mut c = Coord::new("make", Some(SAMPLE));
    let t = c.open();
    let thread = c.core.tickets.m.thread_of(t);
    // a fresh one, in the thread of the step it owes: registered below it, recorded as a `made` line
    c.make("orch", "dev1", None, false).unwrap();
    assert_eq!(c.core.agents["dev1"].parent, "orch");
    let reg = std::fs::read_to_string(c.dir.join("registry.jsonl")).unwrap();
    assert!(reg.lines().any(|l| l.contains("\"name\":\"dev1\"") && l.contains("\"parent\":\"orch\"")), "{reg}");
    // roster's own `coord add` as it starts the made agent, with the same kind and model: nothing written again
    let model = c.core.agents["dev1"].model.clone();
    assert!(c.core.add(&Who::Agent("orch".into()), "dev1", "claude", &model).ok);
    let again = std::fs::read_to_string(c.dir.join("registry.jsonl")).unwrap();
    assert_eq!(again.lines().filter(|l| l.contains("\"name\":\"dev1\"")).count(), 1, "{again}");
    assert!(c.core.tickets.m.made().contains(&("dev1".into(), "orch".into(), None, Some(thread.clone()))), "{:?}", c.core.tickets.m.made());
    let log = std::fs::read_to_string(c.dir.join("tickets/log.jsonl")).unwrap();
    assert!(log.lines().any(|l| l.contains("\"ev\":\"made\"") && l.contains("\"agent\":\"dev1\"") && l.contains("\"by\":\"orch\"")), "{log}");
    // made for this thread, as the word `agents` says
    assert!(c.core.tickets.m.made_for(t).contains(&"dev1".to_string()));
    // a copy of it: made in this thread, so allowed; its source on the registry
    c.make("orch", "dev1-b", Some("dev1"), true).unwrap();
    let reg = std::fs::read_to_string(c.dir.join("registry.jsonl")).unwrap();
    assert!(reg.lines().any(|l| l.contains("\"name\":\"dev1-b\"") && l.contains("\"from\":\"dev1\"")), "{reg}");
    assert!(c.core.tickets.m.made().contains(&("dev1-b".into(), "orch".into(), Some("dev1".into()), Some(thread))));
    // a copy of an agent that existed before the thread: refused by the policy
    let e = c.make("orch", "old2", Some("old"), true).unwrap_err();
    assert!(e.contains("does not let orch make old2"), "{e}");
    // a copy of an agent it may not read: refused before the policy is asked
    let e = c.make("orch", "ann2", Some("ann"), true).unwrap_err();
    assert!(e.contains("reads its session"), "{e}");
    // another agent makes none
    let e = c.make("bob", "dev2", None, false).unwrap_err();
    assert!(e.contains("does not let bob make dev2"), "{e}");
    // the record replays to the same agents made
    let made = c.core.tickets.m.made();
    c.core.load_tickets();
    assert_eq!(c.core.tickets.m.made(), made);
}

#[test]
fn a_made_agent_starts_in_the_layer_its_maker_runs_in() {
    let mut c = Coord::new("laid", Some(SAMPLE));
    c.core.ticket_input(Input::Open { kind: "laid".into(), under: None, opener: "owner".into(), form: json!({"words": "a cache"}), text: None, names: vec![], told: vec![] }).unwrap();
    let layers = c.core.tickets.m.active_layers();
    assert_eq!(layers.len(), 1, "{layers:?}");
    let asked = |fork: bool| Asked { new: "dev1".into(), from: fork.then(|| "orch".to_string()), model: None, text: "x".into(), fork };
    // not marked as in a layer: none
    assert_eq!(plan(&c.core, &Who::Agent("orch".into()), asked(false)).unwrap().layer, None);
    // in the active layer: the new one and a copy start in it (launch then boxes them where the project is boxed)
    std::fs::create_dir_all(c.dir.join("agents/orch")).unwrap();
    std::fs::write(c.dir.join("agents/orch/layer"), format!("{}\n", layers[0].0)).unwrap();
    assert_eq!(plan(&c.core, &Who::Agent("orch".into()), asked(false)).unwrap().layer, Some(layers[0].0.clone()));
    assert_eq!(plan(&c.core, &Who::Agent("orch".into()), asked(true)).unwrap().layer, Some(layers[0].0.clone()));
    // a layer that has ended: none
    std::fs::write(c.dir.join("agents/orch/layer"), "L-9.l1\n").unwrap();
    assert_eq!(plan(&c.core, &Who::Agent("orch".into()), asked(false)).unwrap().layer, None);
}
