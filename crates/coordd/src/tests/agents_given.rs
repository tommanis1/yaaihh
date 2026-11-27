//! An answer naming an agent where the program gives it calls later (`Defs.agents_given`, paper-research N-101
//! p980) is not taken when the agent's model does not meet the class of one of those calls: the reminder says which
//! agent, which call by its doc line and which class, as the step's gives says them; an agent that meets them is
//! taken.

use crate::core::{Core, Who};
use docket_machine::{Input, Loc, Status};
use serde_json::json;

const CALLS: &str = "\
/// who designs it
call assign(words: Text) -> Assigned(who: Agent)
  template
    Who designs {{ words }}?

/// the design for the owner's words
call design(words: Text) where model >= medium -> Designed(how: Text)
  template
    Design {{ words }}.
";

const KINDS: &str = "\
kind pick
  letter A
  form words: Text
  result Designed(how: Text)
  pattern
    w <- step orch assign(words);
    match w { Assigned(who) => step who design(words) }
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

/// A coord with CALLS and KINDS: ann runs haiku (small), bob sonnet (medium), orch opus.
fn coord(tag: &str) -> Coord {
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let dir = std::env::temp_dir().join(format!("docket-given-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let project = dir.join("files");
    for d in ["kinds", "calls", "policy"] {
        std::fs::create_dir_all(project.join(d)).unwrap();
    }
    let add = |n: &str, i: u32, m: &str| format!("{{\"ev\":\"add\",\"id\":\"AGT-00{i}\",\"name\":\"{n}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"{m}\",\"at\":\"x\"}}\n");
    std::fs::write(dir.join("registry.jsonl"), format!("{}{}{}", add("ann", 1, "claude-haiku-5-5"), add("bob", 2, "claude-sonnet-5-5"), add("orch", 3, "claude-opus-5-5"))).unwrap();
    std::fs::write(project.join("calls/g.call"), CALLS).unwrap();
    std::fs::write(project.join("kinds/g.kind"), KINDS).unwrap();
    std::fs::write(project.join("project.toml"), "[project]\nname = \"g\"\n").unwrap();
    std::fs::write(dir.join("project"), project.join("project.toml").display().to_string()).unwrap();
    let mut core = Core::load(&dir.display().to_string(), &base);
    core.project = crate::project::Project::load(&dir.display().to_string()).unwrap();
    core.set_models(vec![("claude-haiku-5-5".into(), "claude".into()), ("claude-sonnet-5-5".into(), "claude".into()), ("claude-opus-5-5".into(), "claude".into())]);
    core.load_tickets();
    assert!(core.tickets.m.defs.kinds.contains_key("pick"), "{:?}", core.tickets.errors);
    Coord { core, dir }
}

#[test]
fn an_agent_named_where_it_is_given_a_call_it_cannot_run_is_not_taken() {
    let mut c = coord("unmet");
    assert_eq!(c.core.tickets.m.defs.agents_given[&("pick".to_string(), "assign".to_string())]["Assigned.who"], vec!["design".to_string()]);
    c.core.ticket_input(Input::Open { kind: "pick".into(), under: None, opener: "owner".into(), form: json!({"words": "the cache"}), text: None, names: vec![], told: vec![] }).unwrap();
    let t = *c.core.tickets.m.tickets.keys().find(|t| **t < docket_machine::HIDDEN).unwrap();
    for d in c.core.tickets.m.due("orch") {
        c.core.ticket_input(Input::Given { t: d.t, to: "orch".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Loc { file: "t.jsonl".into(), offset: 1, len: 1 }) }).unwrap();
    }
    let id = c.core.tickets.m.id(t);
    let r = c.core.ticket_answer_request(&Who::Agent("orch".into()), &id, json!({"head": "Assigned", "who": "ann"}), Some("ann".into()));
    assert!(!r.ok && r.out.contains("ann runs claude-haiku-5-5 (1, small), and the agent named at Assigned.who is given the design for the owner's words (medium)"), "{}", r.out);
    assert!(matches!(&c.core.tickets.m.tickets[&t].status, Status::Awaits { owed, .. } if owed.call == "assign"));
    // bob runs sonnet: taken, and design is his
    let r = c.core.ticket_answer_request(&Who::Agent("orch".into()), &id, json!({"head": "Assigned", "who": "bob"}), Some("bob".into()));
    assert!(r.ok, "{}", r.out);
    assert!(c.core.ticket_due("bob").iter().any(|d| d.t == t));
}
