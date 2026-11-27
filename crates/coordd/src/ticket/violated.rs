//! Whether an answer breaks a duty (N-101 post 57, point 3): coordd's side of the machine's `violated` question.
//!
//! The machine asks, before it takes an agent's answer to a step, which duties the answer breaks (docket-machine
//! machine/violated.rs). coordd answers off the lock from the record alone: the lines of the ticket's tree up to the
//! answer, and the `answering(agent, call, head)` fact for the answer being given, which no line carries (the answer
//! is not taken yet). The engine is eflint-rs (the owner on Q-156: "efling for duties yes fine"), in guided mode, over
//! the project's `policy/duties.eflint` (with its `#include`s); the clingo norms leave that file to it. The answer is
//! the duties broken, each `{duty, holder, claimant, work}`; [] takes the answer.
//!
//! A goal's duties are read the same way, over the goal's own lines the machine sends (`goal_duties`; N-1511 p47,
//! paper-research N-1446 p2139, 2): what it still owes, the lines that met a duty, and the first owed one broken.

use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;

/// The file of a project's policy folder that declares its duties.
pub const FILE: &str = "duties.eflint";

/// The answer being given, as the policy's fact `answering(agent, call, head)` has it.
#[derive(Clone, Debug, PartialEq)]
pub struct Answering {
    pub agent: String,
    pub call: String,
    pub head: String,
}

/// Whether P is a policy folder's duties file.
pub fn is_duties_file(p: &Path) -> bool {
    p.file_name().is_some_and(|n| n == FILE)
}

/// The duties file of PROJECT's policy, if it declares duties an answer can break (`answering` and a duty with
/// `Violated when`); a file that does not load is an error.
pub fn declared(project: Option<&str>) -> Result<Option<String>, String> {
    let Some(p) = project else { return Ok(None) };
    let path = format!("{p}/policy/{FILE}");
    if !Path::new(&path).is_file() {
        return Ok(None);
    }
    let policy = eflint::replay::Replay::load(Path::new(&path)).map_err(|e| format!("{path} does not load: {e}"))?;
    Ok(eflint::duties::asks_at_answer(&policy).then_some(path))
}

/// The record LOG up to T's answer by AGENT (the lines after it, its own, and the `asked` question): the text the
/// engine replays. With no such answer (asked again after a restart), up to the question's `asked` line.
pub fn record_to_answer(log: &str, t: u64, step: u32, agent: &str) -> String {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    let parsed: Vec<Option<Value>> = lines.iter().map(|l| serde_json::from_str(l).ok()).collect();
    let is = |v: &Option<Value>, ev: &str| v.as_ref().is_some_and(|v| v["ev"] == ev && v["t"].as_u64() == Some(t));
    let answer = parsed.iter().rposition(|v| is(v, "answer") && v.as_ref().is_some_and(|v| v["by"] == agent));
    let asked = parsed.iter().rposition(|v| is(v, "asked") && v.as_ref().is_some_and(|v| v["question"] == "violated" && v["step"].as_u64() == Some(u64::from(step))));
    let cut = match (answer, asked) {
        (Some(a), Some(q)) if a < q => a,
        (_, Some(q)) => q,
        (Some(a), None) => a,
        (None, None) => lines.len(),
    };
    lines[..cut].iter().map(|l| format!("{l}\n")).collect()
}

/// The duties AGENT's answer breaks, read by POLICY from RECORD on the tickets TREE, an edit counting only in the
/// project's REPOS (none: every edit; paper-research N-159 post 19), and why: (duties, provenance).
pub fn violated(policy: &str, record: &str, tree: &BTreeSet<u64>, repos: &[String], answering: &Answering) -> Result<(Vec<Value>, Value), String> {
    violated_until(policy, record, tree, repos, answering, &|| false).map(|o| o.expect("an ask nothing stops ends"))
}

/// [`violated`], given up (None) when STOP says so, before the replay or between its events.
pub fn violated_until(policy: &str, record: &str, tree: &BTreeSet<u64>, repos: &[String], answering: &Answering, stop: &dyn Fn() -> bool) -> Result<Option<(Vec<Value>, Value)>, String> {
    let p = eflint::replay::Replay::load(Path::new(policy))?;
    let opts = eflint::record::Options { runner: "runner".into(), lead: None, tickets: tree.iter().map(|t| *t as i64).collect(), repos: repos.to_vec(), ..Default::default() };
    let asked = eflint::duties::Asked { agent: &answering.agent, call: &answering.call, head: &answering.head };
    let Some(o) = eflint::duties::at_answer_until(p, record, &opts, &asked, stop)? else { return Ok(None) };
    let broken: Vec<Value> = o.violated().map(|d| json!({"duty": d.duty, "holder": d.holder, "claimant": d.claimant, "work": d.work})).collect();
    let held: Vec<Value> = o.duties.iter().map(|d| json!({"duty": d.duty, "instance": d.instance, "violated": d.violated})).collect();
    let mut why = json!({"asked": "violated", "policy": policy, "answering": [answering.agent, answering.call, answering.head], "events": o.events, "held": held, "supplied": o.supplied});
    // a fact no line records went to the policy: whoever reads the record later sees it (docket-core, N-149 post 9)
    if o.supplied.iter().any(|s| s.starts_with("turn-ended(")) {
        why["note"] = json!(format!("turn_ended({}) supplied: answered mid-turn", answering.agent));
    }
    if !o.declared {
        why["note"] = json!("the policy declares no `answering`: nothing asked");
    }
    if !o.not_performed.is_empty() {
        why["not_performed"] = json!(o.not_performed);
    }
    Ok(Some((broken, why)))
}

/// Goal GOAL's duties, read by POLICY over LINES, its lines as the machine keeps them (docket-machine's `duties`
/// question): `{goal, owed, holders, by, met, broken}`, HOLDERS each owed duty's holder in OWED's order, MET each duty
/// met with its holder and the line that met it (eflint-rs `Goal::met`; docket-edge6 N-1446 p2421), each line of BY,
/// MET and BROKEN named as [`line_ref`] names it, and why. None when STOP says so, before the replay or between its
/// events.
pub fn goal_duties(policy: &str, goal: &str, lines: &[Value], stop: &dyn Fn() -> bool) -> Result<Option<(Value, Value)>, String> {
    let p = eflint::replay::Replay::load(Path::new(policy))?;
    let opts = eflint::record::Options { runner: "runner".into(), goal: Some(goal.to_string()), ..Default::default() };
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    let Some(g) = eflint::duties::of_goal_until(p, &text, &opts, stop)? else { return Ok(None) };
    let at = |n: i64| usize::try_from(n - 1).ok().and_then(|i| lines.get(i)).map_or_else(|| format!("line {n}"), line_ref);
    let owed: Vec<&str> = g.owed.iter().map(|d| d.duty.as_str()).collect();
    let by: Vec<String> = g.by.iter().map(|n| at(*n)).collect();
    let broken = g.broken.as_ref().map(|(d, n)| json!({"duty": d.duty, "line": at(*n)}));
    let holders: Vec<Value> = g.owed.iter().map(|d| json!({"duty": d.duty, "holder": d.holder})).collect();
    let met: Vec<Value> = g.met.iter().map(|(d, n)| json!({"duty": d.duty, "holder": d.holder, "line": at(*n)})).collect();
    let held: Vec<Value> = g.owed.iter().map(|d| json!({"duty": d.duty, "instance": d.instance, "violated": d.violated})).collect();
    let mut why = json!({"asked": "duties", "policy": policy, "goal": goal, "lines": lines.len(), "events": g.events, "held": held});
    if g.undeclared > 0 {
        why["undeclared"] = json!(g.undeclared);
    }
    if !g.not_performed.is_empty() {
        why["not_performed"] = json!(g.not_performed);
    }
    Ok(Some((json!({"goal": goal, "owed": owed, "holders": holders, "by": by, "met": met, "broken": broken}), why)))
}

/// Line L of a goal's as the `duties` answer names it: its time and what it is (`… job end 41-17`, `… answer by w3`).
pub fn line_ref(l: &Value) -> String {
    let s = |k: &str| l.get(k).and_then(Value::as_str).unwrap_or_default();
    let what = match s("ev") {
        "act" => format!("{} {} {}", s("is"), s("phase"), s("id")),
        "event" => s("name").to_string(),
        "answer" => format!("answer by {}", s("by")),
        ev => ev.to_string(),
    };
    format!("{} {}", s("at"), what.trim())
}

#[cfg(test)]
mod tests {
    use super::super::{run_task, Core, Task};
    use docket_machine::{Input, Loc, Status, Text};
    use serde_json::{Value, json};

    /// The part of Tovrin's duties the question needs (post 57; paper-research2's eflint-rs tests/duties.rs): an
    /// edit owes a typecheck, a submit hands it on, and an answer the program hands in excuses it.
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
    }

    impl Coord {
        fn new(duties: bool) -> Coord {
            let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
            let dir = std::env::temp_dir().join(format!("docket-violated-{duties}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let project = dir.join("files");
            for d in ["kinds", "calls", "policy"] {
                std::fs::create_dir_all(project.join(d)).unwrap();
            }
            std::fs::write(dir.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"dev\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
            std::fs::write(project.join("calls/v.call"), CALLS).unwrap();
            std::fs::write(project.join("kinds/v.kind"), format!("{}\n{}", kind("mend", "M", "mend_red"), kind("work", "W", "implement"))).unwrap();
            if duties {
                std::fs::write(project.join("policy/duties.eflint"), DUTIES).unwrap();
            }
            // the project's code: one repository, the project's folder
            assert!(std::process::Command::new("git").args(["init", "-q"]).current_dir(&project).status().unwrap().success());
            std::fs::write(project.join("project.toml"), "[project]\nname = \"v\"\nrepo = \".\"\n").unwrap();
            std::fs::write(dir.join("project"), project.join("project.toml").display().to_string()).unwrap();
            let mut core = Core::load(&dir.display().to_string(), &base);
            core.project = crate::project::Project::load(&dir.display().to_string()).unwrap();
            core.load_tickets();
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            core.tickets.tasks = Some(tx);
            let engine = crate::policy::Engine::start(core.places());
            Coord { core, rx, engine, dir }
        }
        /// The tasks the inputs made, run, and their answers put in, until none is left.
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
        fn act(&mut self, is: &str, more: Value) {
            let more = more.as_object().cloned().unwrap_or_default();
            self.core.ticket_input(Input::Act { agent: "dev".into(), is: is.into(), text: format!("{is} Packet.parse"), more, t: None }).unwrap();
            self.settle();
        }
        fn answer(&mut self, t: u64) -> Status {
            self.core.ticket_input(Input::Answer { t, by: "dev".into(), head: json!({"head": "Commit", "c": "9d59941"}), text: Text::New("done".into()) }).unwrap();
            self.settle();
            self.core.tickets.m.tickets[&t].status.clone()
        }
        fn decided(&self) -> Vec<Value> {
            std::fs::read_to_string(self.dir.join("tickets/log.jsonl")).unwrap().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["ev"] == "decided" && v["question"] == "violated").collect()
        }
    }

    impl Drop for Coord {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn edit() -> Value {
        json!({"tool": "replace", "symbol": "Packet.parse", "files": ["src/Packet.agda"]})
    }

    /// Post 57's example through coordd, each on the record coordd writes: mend_red's Commit after an edit and no
    /// submit raises Violated; with a submit, and implement's Commit (handed in), it does not.
    #[test]
    fn post_57_through_coordd() {
        let mut c = Coord::new(true);
        assert!(c.core.tickets.errors.is_empty(), "{:#?}", c.core.tickets.errors);
        assert!(c.core.tickets.m.defs.duties);
        // the clingo norms leave duties.eflint (Duty, which they refuse) to eflint-rs
        crate::policy::norms::load(&c.core.places()).expect("the norms load beside duties.eflint");
        // mend_red, an edit, no submit: Violated, handled by the try
        let m = c.open("mend");
        c.act("edit", edit());
        assert!(matches!(c.answer(m), Status::Done(v) if v == "violated"), "{:?}", c.core.tickets.m.tickets[&m].status);
        let d = c.decided();
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0]["answer"][0]["duty"], "owes-typecheck");
        assert_eq!(d[0]["answer"][0]["holder"], "dev");
        assert_eq!(d[0]["answer"][0]["work"], json!(c.core.tickets.m.id(m)));
        assert_eq!(d[0]["provenance"]["note"], "turn_ended(dev) supplied: answered mid-turn", "{}", d[0]["provenance"]);
        // mend_red with a submit after the edit: taken
        let m2 = c.open("mend");
        c.act("edit", edit());
        // the refs name M2's layer: a check of its work (eflint-rs 27c65c0)
        c.act("submit", json!({"check": "dev.c1", "refs": [format!("tovrin:layer/{}.l1", c.core.tickets.m.id(m2))], "mode": "quick"}));
        assert!(matches!(c.answer(m2), Status::Done(v) if v == "9d59941"), "{:?} {:?}", c.core.tickets.m.tickets[&m2].status, c.decided());
        assert_eq!(c.decided()[1]["answer"], json!([]));
        // implement's Commit is handed in by the program: excused, taken
        let w = c.open("work");
        c.act("edit", edit());
        assert!(matches!(c.answer(w), Status::Done(v) if v == "9d59941"), "{:?} {:?}", c.core.tickets.m.tickets[&w].status, c.decided());
        assert_eq!(c.decided()[2]["answer"], json!([]));
        // an edit outside every repository (a design note in the vault) owes no typecheck: taken
        let m3 = c.open("mend");
        c.act("edit", json!({"tool": "Write", "files": ["/home/t/vault/designs/packet.md"]}));
        assert!(matches!(c.answer(m3), Status::Done(v) if v == "9d59941"), "{:?} {:?}", c.core.tickets.m.tickets[&m3].status, c.decided());
        assert_eq!(c.decided()[3]["answer"], json!([]));
        // inside the repository, by its absolute path, with no submit: Violated
        let m4 = c.open("mend");
        let inside = c.dir.join("files/src/Packet.agda").display().to_string();
        c.act("edit", json!({"tool": "Write", "files": [inside]}));
        assert!(matches!(c.answer(m4), Status::Done(v) if v == "violated"), "{:?} {:?}", c.core.tickets.m.tickets[&m4].status, c.decided());
        // a check of the line (`a:landed`), not of the work's layer, meets no duty: Violated (tovrin-writer, N-101 p1071)
        let m5 = c.open("mend");
        c.act("edit", edit());
        c.act("submit", json!({"check": "dev.c2", "refs": ["tovrin:landed"], "mode": "quick"}));
        assert!(matches!(c.answer(m5), Status::Done(v) if v == "violated"), "{:?} {:?}", c.core.tickets.m.tickets[&m5].status, c.decided());
        // the record replays to the same tickets
        let again = super::super::Tickets::load(&c.dir.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), c.core.project.as_ref().map(|p| p.dir.as_str()), 1);
        assert!(again.errors.is_empty(), "{:#?}", again.errors);
        for t in [m, m2, w, m3, m4, m5] {
            assert_eq!(again.m.view(t), c.core.tickets.m.view(t), "{t}");
        }
    }

    /// Without a duties file nothing is asked, and the kinds' Violated arms are refused.
    #[test]
    fn without_duties_nothing_is_asked() {
        let c = Coord::new(false);
        assert!(!c.core.tickets.m.defs.duties);
        assert!(c.core.tickets.errors.iter().any(|e| e.contains("Violated")), "{:#?}", c.core.tickets.errors);
    }

    #[test]
    fn the_record_is_cut_at_the_answer() {
        let dir = std::env::temp_dir().join(format!("docket-violated-cut-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("log.jsonl");
        std::fs::write(&log, "{\"ev\":\"open\",\"t\":5}\n{\"ev\":\"answer\",\"t\":5,\"by\":\"a\"}\n{\"ev\":\"asked\",\"t\":5,\"step\":1,\"question\":\"violated\"}\n{\"ev\":\"given\",\"t\":6}\n").unwrap();
        let cut = super::record_to_answer(log.to_str().unwrap(), 5, 1, "a");
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(cut, "{\"ev\":\"open\",\"t\":5}\n");
    }
}

/// How many `violated` asks are running now, on their blocking threads.
static RUNNING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Whether the memory freed is given back after each ask ends (true) or once the last ask running ends (false).
pub(crate) const EVERY_ASK: bool = true;

/// One `violated` ask running, from its start until it is dropped at its end. When it ends, the memory freed by the
/// ask is given back to the system. Otherwise glibc's thread arenas keep the peak of the asks that ran at once:
/// Tovrin-dev's coordd stood at 2.3 GB, and on a copy of its record 43 arenas kept 1.7 GB after 8 asks at once,
/// against 525 MB with the memory given back (docket-core4, N-101 p1825). EVERY gives it back after each ask; otherwise
/// it is given back once no ask runs.
pub(crate) struct Running {
    every: bool,
}

impl Running {
    pub(crate) fn start(every: bool) -> Running {
        RUNNING.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Running { every }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let last = RUNNING.fetch_sub(1, std::sync::atomic::Ordering::SeqCst) == 1;
        if self.every || last {
            give_back();
        }
    }
}

/// The memory this process has freed, given back to the system: glibc's `malloc_trim`, which walks every arena.
/// Said once in coordd's log, at its first use.
fn give_back() {
    #[cfg(target_env = "gnu")]
    {
        static SAID: std::sync::Once = std::sync::Once::new();
        SAID.call_once(|| eprintln!("coordd: tickets: the memory a `violated` ask frees is given back to the system when the ask ends (malloc_trim)"));
        unsafe { libc::malloc_trim(0) };
    }
}
