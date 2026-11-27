//! Edits made through Bash, found (coordd found.rs; N-101 p117, p118): through coordd, each on the record it writes.
//! A layer here is its entry in `layers.json` and a plain folder as its upper folder: what found.rs reads.

use super::{run_task, Core, Task};
use crate::core::Who;
use docket_machine::{Input, Loc, Status};
use serde_json::{Value, json};

const DUTIES: &str = r#"Fact agent Identified by String
Fact work Identified by String
Fact made Identified by agent * work
Fact turn-ended Identified by agent
Fact role-name Identified by String
Fact role Identified by agent * role-name
Fact call-name Identified by String
Fact head-name Identified by String
Fact answering Identified by agent * call-name * head-name
Fact hands-in Identified by call-name * head-name
Fact excused Identified by agent
  Holds when (Exists answering, hands-in : answering.agent == agent
           && hands-in.call-name == answering.call-name && hands-in.head-name == answering.head-name)
Act edit
  Actor agent
  Related to work
  Holds when made(agent, work)
  Creates owes-typecheck(agent, agent', work) When role(agent', "runner")
Duty owes-typecheck
  Holder agent
  Claimant agent'
  Related to work
  Violated when turn-ended(agent) && Not(excused(agent))
Act submit
  Actor agent
  Recipient agent'
  Related to work
  Holds when owes-typecheck(agent, agent', work)
  Terminates owes-typecheck(agent, agent', work)
  Creates owes-check(agent', agent, work)
Duty owes-check
  Holder agent
  Claimant agent'
  Related to work
.
+hands-in("implement", "Commit").
"#;

const CALLS: &str = "call mend_red(what: Text) -> Commit(c: Text)\n  template\n    Mend {{ what }}.\n\ncall implement(what: Text) -> Commit(c: Text)\n  template\n    Implement {{ what }}.\n";

fn kind(name: &str, letter: &str, call: &str) -> String {
    format!("kind {name}\n  letter {letter}\n  form what: Text\n  result Text\n  pattern\n    try {{\n      r <- step dev {call}(what);\n      match r {{ Commit(c) => return c }}\n    }} handle {{ Violated(v) => return \"violated\" }}\n")
}

struct Coord {
    core: Core,
    rx: tokio::sync::mpsc::UnboundedReceiver<Task>,
    engine: crate::policy::Engine,
    dir: std::path::PathBuf,
    repo: String,
    up: std::path::PathBuf,
}

impl Coord {
    fn new(tag: &str) -> Coord {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let dir = std::env::temp_dir().join(format!("docket-found-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let project = dir.join("files");
        for d in ["kinds", "calls", "policy"] {
            std::fs::create_dir_all(project.join(d)).unwrap();
        }
        let add = |n: &str, i: u32| format!("{{\"ev\":\"add\",\"id\":\"AGT-00{i}\",\"name\":\"{n}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}}\n");
        std::fs::write(dir.join("registry.jsonl"), format!("{}{}", add("dev", 1), add("minder", 2))).unwrap();
        std::fs::write(project.join("calls/v.call"), CALLS).unwrap();
        std::fs::write(project.join("kinds/v.kind"), format!("{}\n{}", kind("mend", "M", "mend_red"), kind("work", "W", "implement"))).unwrap();
        std::fs::write(project.join("policy/duties.eflint"), DUTIES).unwrap();
        std::fs::write(project.join("project.toml"), "[project]\nname = \"v\"\n").unwrap();
        std::fs::write(project.join(".gitignore"), "*.pyc\n.conversations/\n").unwrap();
        let ok = std::process::Command::new("git").args(["init", "-q"]).current_dir(&project).status().unwrap();
        assert!(ok.success());
        std::fs::write(dir.join("project"), project.join("project.toml").display().to_string()).unwrap();
        let mut core = Core::load(&dir.display().to_string(), &base);
        core.project = crate::project::Project::load(&dir.display().to_string()).unwrap();
        core.load_tickets();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        core.tickets.tasks = Some(tx);
        let engine = crate::policy::Engine::start(core.places());
        let up = dir.join("layers/L.l1/up");
        std::fs::create_dir_all(&up).unwrap();
        std::fs::create_dir_all(dir.join("layers/L.l1/out/target")).unwrap();
        Coord { core, rx, engine, repo: project.display().to_string(), dir, up }
    }
    fn settle(&mut self) {
        while let Ok(task) = self.rx.try_recv() {
            for i in run_task(&self.engine, task) {
                let _ = self.core.ticket_input(i);
            }
        }
    }
    /// A KIND opened by the owner and its step given to dev; its number.
    fn open(&mut self, kind: &str) -> u64 {
        self.core.ticket_input(Input::Open { kind: kind.into(), under: None, opener: "owner".into(), form: json!({"what": "Packet.parse"}), text: None, names: vec![], told: vec![] }).unwrap();
        self.settle();
        let t = *self.core.tickets.m.tickets.keys().filter(|t| **t < docket_machine::HIDDEN).max().unwrap();
        for d in self.core.tickets.m.due("dev") {
            self.core.ticket_input(Input::Given { t: d.t, to: "dev".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Loc { file: "t.jsonl".into(), offset: 1, len: 1 }) }).unwrap();
        }
        t
    }
    /// Layer L.l1 of ticket T, open, with AGENTS working in it: the view's upper folder is `up`, over the project.
    fn layer(&self, t: u64, agents: &[&str]) {
        let id = self.core.tickets.m.id(t);
        let l = json!({"L.l1": {"id": "L.l1", "ticket": id, "repo": self.repo, "base": "0", "branch": "layer/L.l1", "state": "open", "up": self.up.display().to_string(), "outputs": ["target"], "at": "x"}});
        std::fs::write(self.dir.join("layers/layers.json"), l.to_string()).unwrap();
        let _ = std::fs::remove_file(self.dir.join("layers/L.l1/found.json"));
        let reg = crate::layer::load(&self.dir.display().to_string());
        crate::found::opened(&self.dir.display().to_string(), &reg["L.l1"]);
        for a in agents {
            std::fs::create_dir_all(self.dir.join(format!("agents/{a}"))).unwrap();
            std::fs::write(self.dir.join(format!("agents/{a}/layer")), "L.l1\n").unwrap();
        }
    }
    /// A write through Bash in the view: a file of the layer's upper folder.
    fn bash(&self, rel: &str, text: &str) {
        let p = self.up.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(p).unwrap();
        std::io::Write::write_all(&mut f, text.as_bytes()).unwrap();
    }
    fn acts(&self) -> Vec<Value> {
        self.lines().into_iter().filter(|v| v["ev"] == "act").collect()
    }
    fn lines(&self) -> Vec<Value> {
        std::fs::read_to_string(self.dir.join("tickets/log.jsonl")).unwrap().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).collect()
    }
    fn answer(&mut self, t: u64) -> Status {
        let id = self.core.tickets.m.id(t);
        let _ = self.core.ticket_answer_request(&Who::Agent("dev".into()), &id, json!({"head": "Commit", "c": "9d59941"}), Some("done".into()));
        self.settle();
        self.core.tickets.m.tickets[&t].status.clone()
    }
}

impl Drop for Coord {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A `sed` edit and the turn's end: one act edit, found at the turn's end and before it, on the ticket owed; the
/// duty it makes is broken at the answer (no submit). Another look finds nothing new.
#[test]
fn an_edit_through_bash_is_found_at_the_turns_end() {
    let mut c = Coord::new("turn");
    let m = c.open("mend");
    c.layer(m, &["dev"]);
    c.bash("src/a.rs", "fn a() {}\n");
    // after the step was given (at offset 1), so the turn's end is taken
    c.core.ticket_turn_ended("dev", Loc { file: "t.jsonl".into(), offset: 2, len: 0 });
    c.settle();
    let acts = c.acts();
    assert_eq!(acts.len(), 1, "{acts:?}");
    let a = &acts[0];
    assert_eq!((a["agent"].clone(), a["is"].clone(), a["t"].clone(), a["found"].clone()), (json!("dev"), json!("edit"), json!(m), json!("turn_end")), "{a}");
    assert_eq!(a["files"], json!([format!("{}/src/a.rs", c.repo)]));
    assert_eq!(a["turns"], json!(["dev"]));
    // before the turn's end in the record
    let lines = c.lines();
    let i = lines.iter().position(|v| v["ev"] == "act").unwrap();
    let j = lines.iter().position(|v| v["ev"] == "turn_ended").expect("the turn's end is taken");
    assert!(i < j, "{lines:?}");
    // nothing changed since: nothing found
    c.core.look_found("dev", "turn_end");
    assert_eq!(c.acts().len(), 1);
    // the duty: broken at the answer, as for the hook's edit
    assert!(matches!(c.answer(m), Status::Done(v) if v == "violated"), "{:?}", c.core.tickets.m.tickets[&m].status);
}

/// A `sed` edit, then `coord check`, then the answer: the edit is found before the submit, which covers it.
#[test]
fn an_edit_before_a_submit_is_covered_by_it() {
    let mut c = Coord::new("submit");
    let m = c.open("mend");
    c.layer(m, &["dev"]);
    c.bash("src/a.rs", "fn a() {}\n");
    // check()'s order: the look, then the submit
    c.core.look_found("dev", "submit");
    // the refs name M's layer: a check of its work (eflint-rs 27c65c0)
    let refs = [format!("v:layer/{}.l1", c.core.tickets.m.id(m))];
    c.core.ticket_event("act", &json!({"by": "dev", "is": "submit", "text": "quick", "check": "dev.c1", "refs": refs}));
    c.settle();
    let acts = c.acts();
    assert_eq!(acts.iter().map(|a| (a["is"].clone(), a["found"].clone())).collect::<Vec<_>>(), vec![(json!("edit"), json!("submit")), (json!("submit"), Value::Null)], "{acts:?}");
    assert!(matches!(c.answer(m), Status::Done(v) if v == "9d59941"), "{:?}", c.core.tickets.m.tickets[&m].status);
    // and the answer's look found nothing more
    assert_eq!(c.acts().len(), 2);
}

/// An edit the hook saw is not found again; a later write through Bash to the same file is.
#[test]
fn a_hook_covered_edit_is_not_found_again() {
    let mut c = Coord::new("hook");
    let m = c.open("mend");
    c.layer(m, &["dev"]);
    c.bash("src/b.rs", "fn b() {}\n");
    let file = format!("{}/src/b.rs", c.repo);
    c.core.act_of_hook("dev", &json!({"tool_name": "Write", "tool_input": {"file_path": file}}));
    c.settle();
    c.core.look_found("dev", "turn_end");
    let acts = c.acts();
    assert_eq!(acts.len(), 1, "only the hook's: {acts:?}");
    assert_eq!(acts[0]["tool"], "Write");
    c.bash("src/b.rs", "// and more\n");
    c.core.look_found("dev", "turn_end");
    let acts = c.acts();
    assert_eq!(acts.len(), 2, "{acts:?}");
    assert_eq!((acts[1]["files"].clone(), acts[1]["found"].clone()), (json!([file]), json!("turn_end")));
}

/// A build output is bound from outside the layer, and what git ignores is no edit: neither is found.
#[test]
fn a_build_output_and_an_ignored_file_are_not_found() {
    let mut c = Coord::new("outputs");
    let m = c.open("mend");
    c.layer(m, &["dev"]);
    std::fs::write(c.dir.join("layers/L.l1/out/target/app"), "binary").unwrap();
    c.bash("src/__pycache__/a.pyc", "x");
    c.core.look_found("dev", "turn_end");
    assert_eq!(c.acts(), Vec::<Value>::new());
    // what is not ignored still is
    c.bash("src/a.py", "x = 1\n");
    c.core.look_found("dev", "turn_end");
    assert_eq!(c.acts()[0]["files"], json!([format!("{}/src/a.py", c.repo)]));
}

/// The minder's case (C-48): an agent owing no step writes a file through Bash in the layer; at its turn's end the
/// act falls on the layer's ticket, and names every agent that had a turn there since the last look.
#[test]
fn a_minders_edit_falls_on_the_layers_ticket() {
    let mut c = Coord::new("minder");
    let w = c.open("work");
    c.layer(w, &["dev", "minder"]);
    assert_eq!(c.core.tickets.m.owing("minder"), None);
    c.core.begin_turn("dev");
    c.core.begin_turn("minder");
    c.bash("notes/minder.md", "seen\n");
    c.core.ticket_turn_ended("minder", Loc { file: "t.jsonl".into(), offset: 0, len: 0 });
    c.settle();
    let acts = c.acts();
    assert_eq!(acts.len(), 1, "{acts:?}");
    assert_eq!((acts[0]["agent"].clone(), acts[0]["t"].clone()), (json!("minder"), json!(w)));
    assert!(acts[0].get("on").is_none(), "no field but the act's own: {}", acts[0]);
    assert_eq!(acts[0]["turns"], json!(["dev", "minder"]));
    // dev is still in its turn: at the next look it counts again; the minder, whose turn ended, does not
    c.bash("src/x.rs", "x\n");
    c.core.look_found("dev", "answer");
    assert_eq!(c.acts()[1]["turns"], json!(["dev"]));
    // the record replays to the same tickets
    let again = super::Tickets::load(&c.dir.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), c.core.project.as_ref().map(|p| p.dir.as_str()), 1);
    assert!(again.errors.is_empty(), "{:#?}", again.errors);
    assert_eq!(again.m.view(w), c.core.tickets.m.view(w));
}
