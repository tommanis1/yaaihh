//! The text-change kind (agent-base kinds/text-change.kind, paper-research N-865) through coordd, on a scratch project
//! whose text is a git repository of its own: a change that checks is live and committed, its commit on the reload's
//! line; a change refused changes nothing and makes no commit; what is not the project's text is refused at the
//! check and again at the load; only agent-base's own kind reaches the load; the project's policy decides who may
//! open one and whether it is read first, and an agent's change never reaches the owner; the way back is the same act
//! with the commit; the window's views come from the project file a change loads; the flavour's look changes by the
//! same act; a load stopped midway ends at the next start; a record with a change open is left out on a base without
//! the kind (train 68's way back) and waits again forward; the agent that made a change looks at a still of a stand-in
//! window that took it, and Wrong takes it back.

use super::{Core, Task, run_task};
use crate::change::{CHECK, COORD, LOAD, STILL, check, draw};
use crate::core::Who;
use docket_machine::{Input, Line, Status};
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const CALL: &str = "call ask(q: Text) -> Said(text: Text)\n  task t\n  template\n    Q: {{ q }}\n";
const KIND: &str = "kind e\n  letter E\n  form q: Text\n  result Text\n  pattern\n    a <- step dev ask(q);\n    return a.text\n";
/// The small kind an agent adds and then uses.
const SUM: &str = "/// a question and its answer, in one step\nkind sum\n  form q: Text\n  result Text\n  pattern\n    a <- step dev ask(q);\n    return a.text\n";

struct Coord {
    core: Core,
    rx: tokio::sync::mpsc::UnboundedReceiver<Task>,
    engine: crate::policy::Engine,
    dir: PathBuf,
    /// the project's folder
    text: PathBuf,
    /// the repository it is in: the project's folder, or the flavour's
    repo: PathBuf,
    lead: String,
    /// what the agent that made a change answers to the look at its still (`look_at`); none: left owed
    looks: Option<Value>,
}

/// A stand-in window as the still starts one (change.rs `text_still`): what it was given, in the coord's folder, then
/// "watching" when it waits for the look, and its picture where `--snap` says.
const STAND_IN: &str = "#!/bin/sh\nenv | sort > \"$COORD/stand-in.env\"\nprintf '%s\\n' \"$@\" > \"$COORD/stand-in.args\"\n[ \"$YAAIHH_STILL_AFTER\" = look ] && echo 'yaaihh: watching'\nprintf png > \"$2\"\n";

fn git(dir: &Path, args: &[&str]) -> String {
    let o = std::process::Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@t"]).args(args).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

fn agent_base() -> String {
    format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"))
}

/// How a scratch coord is made.
#[derive(Default)]
struct Make<'a> {
    /// the project's policy files
    policy: &'a [(&'a str, &'a str)],
    /// more of the project's kind files, from the start
    kinds: &'a [(&'a str, &'a str)],
    /// the project is FLAVOUR/projects/e, the flavour the repository (and its look.toml beside `projects`)
    flavour: bool,
    /// a project's text to copy (kinds, calls, policy, renderings) in place of kind e and call ask
    from: Option<&'a Path>,
    /// the agent in the role lead (default `lead`)
    lead: Option<&'a str>,
    /// more agents in the registry, with no role, running from the start
    others: &'a [&'a str],
}

impl Coord {
    /// A coord whose project's text is its own repository, with kind e, call ask and POLICY's files; dev, and lead
    /// in the role lead, running.
    fn new(tag: &str, policy: &[(&str, &str)]) -> Coord {
        Coord::make(tag, Make { policy, ..Default::default() })
    }

    fn make(tag: &str, m: Make) -> Coord {
        let base = agent_base();
        let dir = std::env::temp_dir().join(format!("docket-text-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (repo, text) = if m.flavour { (dir.join("flavour"), dir.join("flavour/projects/e")) } else { (dir.join("text"), dir.join("text")) };
        for d in ["kinds", "calls", "policy"] {
            std::fs::create_dir_all(text.join(d)).unwrap();
        }
        match m.from {
            Some(from) => {
                for d in ["kinds", "calls", "policy", "renderings"] {
                    if from.join(d).is_dir() {
                        let o = std::process::Command::new("cp").arg("-rL").arg(from.join(d)).arg(&text).output().unwrap();
                        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
                    }
                }
                // its policy and its norms as it has them; nothing else of its file (no servers, no logs)
                std::fs::write(text.join("project.toml"), "[project]\nname = \"e\"\nnorms = \"clingo\"\n").unwrap();
            }
            None => {
                std::fs::write(text.join("calls/a.call"), CALL).unwrap();
                std::fs::write(text.join("kinds/e.kind"), KIND).unwrap();
                std::fs::write(text.join("project.toml"), "[project]\nname = \"e\"\n").unwrap();
            }
        }
        for (n, t) in m.policy {
            std::fs::write(text.join("policy").join(n), t).unwrap();
        }
        for (n, t) in m.kinds {
            std::fs::write(text.join("kinds").join(n), t).unwrap();
        }
        git(&repo, &["init", "-q"]);
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-q", "-m", "start"]);
        let lead = m.lead.unwrap_or("lead").to_string();
        let add = |n: &str, i: u32| format!("{{\"ev\":\"add\",\"id\":\"AGT-00{i}\",\"name\":\"{n}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}}\n");
        let others: String = m.others.iter().zip(3..).map(|(n, i)| add(n, i)).collect();
        std::fs::write(dir.join("registry.jsonl"), format!("{}{}{others}{{\"ev\":\"role\",\"name\":\"{lead}\",\"role\":\"lead\"}}\n", add("dev", 1), add(&lead, 2))).unwrap();
        std::fs::write(dir.join("project"), text.join("project.toml").display().to_string()).unwrap();
        let mut core = Core::load(&dir.display().to_string(), &base);
        core.project = crate::project::Project::load(&dir.display().to_string()).unwrap();
        core.load_tickets();
        assert!(core.tickets.errors.is_empty(), "{:?}", core.tickets.errors);
        assert!(core.tickets.m.defs.kinds.contains_key("text-change"), "agent-base's kind is there");
        core.agents.get_mut(&lead).unwrap().live.pid = Some(1);
        for n in m.others {
            core.agents.get_mut(*n).unwrap().live.pid = Some(1);
        }
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        core.tickets.tasks = Some(tx);
        // the policy is solved with the proxy's models: a stand-in that serves none (as decide_tests.rs)
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            for mut s in l.incoming().flatten() {
                let mut buf = [0u8; 65536];
                let _ = s.read(&mut buf);
                let body = r#"{"data": []}"#;
                let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
            }
        });
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = Some(url));
        let engine = crate::policy::Engine::start(core.places());
        // the stand-in window this test's stills start
        let stand_in = dir.join("stand-in.sh");
        crate::accounts::testing::program(&stand_in, STAND_IN);
        crate::change::STAND_IN_AT.with(|s| *s.borrow_mut() = Some(stand_in));
        Coord { core, rx, engine, dir, text, repo, lead, looks: Some(json!({"head": "LooksRight"})) }
    }

    /// coordd started again on the same folder: the record replayed, the journal ended (`text_recover_files`,
    /// `text_recover_line`).
    fn restart(&mut self) {
        let mut core = Core::load(&self.dir.display().to_string(), &self.core.base.clone());
        core.project = crate::project::Project::load(&self.dir.display().to_string()).unwrap();
        core.load_tickets();
        assert!(core.tickets.errors.is_empty(), "{:?}", core.tickets.errors);
        core.agents.get_mut(&self.lead).unwrap().live.pid = Some(1);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        core.tickets.tasks = Some(tx);
        self.core = core;
        self.rx = rx;
    }

    /// The number of the last event coordd sent the windows (`Core::since` takes what came after it).
    fn seq(&self) -> u64 {
        self.core.snapshot_with(Vec::new()).seq
    }

    /// The policy's answers applied, until none is waiting.
    fn tasks(&mut self) -> bool {
        let mut any = false;
        while let Ok(task) = self.rx.try_recv() {
            any = true;
            for i in run_task(&self.engine, task) {
                let _ = self.core.ticket_input(i);
            }
        }
        any
    }

    /// The steps due to `coord`, answered as coordd's job answers them (main.rs `Job::Coord`); how many.
    fn coord(&mut self) -> usize {
        let mut n = 0;
        for d in self.core.ticket_due(COORD) {
            let Some(job) = self.core.text_job(&d) else { continue };
            n += 1;
            self.run(&job);
        }
        n
    }

    fn run(&mut self, job: &crate::change::Job) {
        if job.call == CHECK {
            let c = check(job);
            self.core.text_checked(job, c);
        } else if job.call == STILL {
            let head = match self.core.text_still(job) {
                Err(head) => head,
                Ok(stand) => {
                    let core = &mut self.core;
                    draw(&stand, || core.text_still_look())
                }
            };
            self.core.text_answer(job, head);
        } else {
            self.core.text_load(job);
        }
    }

    /// The looks at stills owed to the agents that made the changes (`look_at`), answered with `looks`; whether any was.
    fn looks(&mut self) -> bool {
        let Some(head) = self.looks.clone() else { return false };
        let owed: Vec<(u64, String)> = self.core.tickets.m.tickets.values().filter_map(|x| match &x.status {
            Status::Awaits { agent, owed, .. } if owed.call == "look_at" => Some((x.t, agent.clone())),
            _ => None,
        }).collect();
        for (t, by) in &owed {
            self.core.ticket_input(Input::Answer { t: *t, by: by.clone(), head: head.clone(), text: docket_machine::Text::New(String::new()) }).unwrap();
        }
        !owed.is_empty()
    }

    fn settle(&mut self) {
        loop {
            let (t, l) = (self.tasks(), self.looks());
            if self.coord() == 0 && !t && !l {
                break;
            }
        }
    }

    /// The look at ticket T's still owed to its agent: the still's path and page, as the step has them.
    fn owed_look(&self, t: u64) -> Option<(String, String, String)> {
        match self.status(t) {
            Status::Awaits { agent, owed, .. } if owed.call == "look_at" => Some((agent, owed.args["still"].as_str()?.to_string(), owed.args["page"].as_str()?.to_string())),
            _ => None,
        }
    }

    /// A text-change opened by BY with FILES (path, text) and WHY, run until nothing more is due to coord or the policy.
    fn change(&mut self, by: &str, files: &[(&str, Option<&str>)], why: &str) -> u64 {
        let files: Vec<Value> = files.iter().map(|(p, t)| json!({"path": p, "text": t})).collect();
        self.open(by, json!({"files": files, "back": null, "why": why}))
    }

    fn open(&mut self, by: &str, form: Value) -> u64 {
        let t = self.open_only("text-change", by, form);
        self.settle();
        t
    }

    fn open_only(&mut self, kind: &str, by: &str, form: Value) -> u64 {
        let before = self.core.tickets.m.next_number();
        self.core.ticket_input(Input::Open { kind: kind.into(), under: None, opener: by.into(), form, text: None, names: vec![], told: vec![] }).unwrap();
        *self.core.tickets.m.tickets.keys().find(|t| **t >= before && **t < docket_machine::HIDDEN).unwrap()
    }

    fn status(&self, t: u64) -> Status {
        self.core.tickets.m.tickets[&t].status.clone()
    }

    fn head(&self) -> String {
        git(&self.repo, &["rev-parse", "HEAD"])
    }

    fn reloads(&self) -> Vec<Line> {
        let log = std::fs::read_to_string(self.dir.join("tickets/log.jsonl")).unwrap_or_default();
        log.lines().map(|l| serde_json::from_str::<Line>(l).unwrap()).filter(|l| matches!(l, Line::Reload { .. })).collect()
    }

    /// The check's answer on ticket T (its first post by coord).
    fn checked(&self, t: u64) -> Value {
        self.core.tickets.m.tickets[&t].posts.iter().find(|p| p.by == COORD).and_then(|p| p.head.clone()).unwrap_or(Value::Null)
    }

    /// The tickets, hidden ones too, that owe the owner a step or ask him.
    fn owner_steps(&self) -> Vec<String> {
        self.core.tickets.m.tickets.values().filter(|x| matches!(&x.status, Status::Awaits { agent, .. } if agent == "owner")).map(|x| self.core.tickets.m.id(x.t)).collect()
    }

    /// The lead's answer HEAD to the read ticket T waits on.
    fn read(&mut self, t: u64, head: Value) {
        let lead = self.lead.clone();
        self.read_as(&lead, t, head);
    }

    /// AGENT's answer HEAD to the read ticket T waits on.
    fn read_as(&mut self, agent: &str, t: u64, head: Value) {
        let id = self.core.tickets.m.id(t);
        let lead = agent.to_string();
        for d in self.core.tickets.m.due(&lead) {
            self.core.ticket_input(Input::Given { t: d.t, to: lead.clone(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(docket_machine::Loc { file: "t.jsonl".into(), offset: 1, len: 1 }) }).unwrap();
        }
        let r = self.core.ticket_answer_request(&Who::Agent(lead), &id, head, Some("read".into()));
        assert!(r.ok, "{}", r.out);
        self.settle();
    }

    /// The agent the read ticket T waits on, when it waits on a read.
    fn reader(&self, t: u64) -> Option<String> {
        match self.status(t) {
            Status::Awaits { agent, owed, .. } if owed.call == "read_text" => Some(agent),
            _ => None,
        }
    }

    fn awaits_read(&self, t: u64) -> bool {
        self.reader(t).as_deref() == Some(self.lead.as_str())
    }
}

impl Drop for Coord {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The result a closed change gave.
fn result(s: &Status) -> Value {
    match s {
        Status::Done(v) => v.clone(),
        other => panic!("not done: {other:?}"),
    }
}

fn why(s: &Status) -> String {
    let r = result(s);
    assert_eq!(r["head"], "Refused", "{r}");
    r["why"].as_str().unwrap().to_string()
}

#[test]
fn a_change_that_checks_is_live_and_committed_and_its_kind_is_used() {
    let mut c = Coord::new("live", &[]);
    let start = c.head();
    let t = c.change("dev", &[("kinds/sum.kind", Some(SUM))], "a kind for one question");
    let r = result(&c.status(t));
    assert_eq!(r["head"], "Live", "{r}");
    let commit = r["commit"].as_str().unwrap().to_string();
    // committed in the project's text repository, by coordd, the ticket and its opener in the message; nothing left
    assert_eq!(c.head(), commit);
    assert_eq!(git(&c.text, &["rev-parse", "HEAD^"]), start);
    assert_eq!(git(&c.text, &["show", "-s", "--format=%an"]), "coord");
    let msg = git(&c.text, &["show", "-s", "--format=%B"]);
    assert!(msg.starts_with("a kind for one question") && msg.contains(&format!("Ticket: {}", c.core.tickets.m.id(t))) && msg.contains("By: dev"), "{msg}");
    assert_eq!(git(&c.text, &["status", "--porcelain"]), "");
    assert_eq!(std::fs::read_to_string(c.text.join("kinds/sum.kind")).unwrap(), SUM);
    assert!(!c.dir.join("text-change.journal").exists(), "the journal ends with the load");
    // what the ticket shows (for the owner's example)
    for p in &c.core.tickets.m.tickets[&t].posts {
        println!("post {} by {}: {}", p.n, p.by, p.head.as_ref().map(|h| h.to_string()).unwrap_or_default().chars().take(300).collect::<String>());
    }
    // the check's answer: who, and what it touches
    let k = c.checked(t);
    assert_eq!((k["by"].clone(), k["kinds"].clone(), k["policy"].clone(), k["shadows"].clone(), k["bindings"].clone(), k["handlers"].clone()), (json!("dev"), json!(["kinds/sum.kind"]), json!([]), json!([]), json!([]), json!([])), "{k}");
    // live: on the reload's line with its commit and who, and the kind opens and runs at once
    let reloads = c.reloads();
    assert!(matches!(&reloads[..], [Line::Reload { by, commit: Some(h), kinds, .. }] if by == "dev" && *h == commit && kinds == &vec!["sum".to_string()]), "{reloads:?}");
    assert!(c.core.tickets.m.defs.kinds.contains_key("sum"));
    let s = c.open_only("sum", "dev", json!({"q": "two and two"}));
    assert!(matches!(c.status(s), Status::Awaits { ref agent, .. } if agent == "dev"), "{:?}", c.status(s));
    c.core.ticket_input(Input::Answer { t: s, by: "dev".into(), head: json!({"head": "Said", "text": "four"}), text: docket_machine::Text::New("four".into()) }).unwrap();
    assert_eq!(c.status(s), Status::Done(json!("four")));
    assert!(c.owner_steps().is_empty());
    // a restart reads it as the files have it, and the record replays
    let mut again = Core::load(&c.dir.display().to_string(), &agent_base());
    again.project = crate::project::Project::load(&c.dir.display().to_string()).unwrap();
    again.load_tickets();
    assert!(again.tickets.errors.is_empty(), "{:?}", again.tickets.errors);
    assert_eq!(again.tickets.m.view(s), c.core.tickets.m.view(s));
}

#[test]
fn a_change_with_a_refusal_changes_nothing_and_makes_no_commit() {
    let mut c = Coord::new("refused", &[]);
    let start = c.head();
    // a kind whose step names a call there is none of: refused at the check, at its file's line
    let bad = "kind bad\n  form q: Text\n  result Text\n  pattern\n    a <- step dev nosuch(q);\n    return a.text\n";
    let t = c.change("dev", &[("kinds/bad.kind", Some(bad)), ("kinds/sum.kind", Some(SUM))], "two kinds");
    let w = why(&c.status(t));
    assert!(w.contains("nosuch"), "{w}");
    let r = result(&c.status(t));
    println!("refused: {r}");
    let real = std::fs::canonicalize(&c.text).unwrap().join("kinds/bad.kind").display().to_string();
    assert!(r["at"].as_array().unwrap().iter().any(|a| a.as_str().unwrap().starts_with(&format!("{real}:"))), "at the line of the real file, not the copy: {r}");
    assert!(!c.text.join("kinds/bad.kind").exists() && !c.text.join("kinds/sum.kind").exists(), "neither file written");
    assert_eq!(c.head(), start, "no commit");
    assert!(c.reloads().is_empty() && !c.core.tickets.m.defs.kinds.contains_key("sum"));
    // a kind removed while a ticket of it is open: refused at the check, the ticket named in plain words
    let e = c.open_only("e", "dev", json!({"q": "q"}));
    let t = c.change("dev", &[("kinds/e.kind", None)], "no more e");
    let id = c.core.tickets.m.id(e);
    assert!(why(&c.status(t)).contains(&format!("the kind e is gone, and the record has tickets that stand on it ({id})")), "{:?}", c.status(t));
    // a form changed while one is open: refused, and why in the open tickets' terms
    let t = c.change("dev", &[("kinds/e.kind", Some(&KIND.replace("form q: Text", "form q: Text, r: Text")))], "a second field");
    assert!(why(&c.status(t)).contains(&format!("the form of e changed, and tickets of it are open ({id}): they run to the form they were opened with")));
    assert_eq!(std::fs::read_to_string(c.text.join("kinds/e.kind")).unwrap(), KIND);
    assert_eq!(c.head(), start);
    // the files changed between the check and the load: the load refuses, and writes nothing
    let t = c.open_only("text-change", "dev", json!({"files": [{"path": "calls/a.call", "text": CALL.replace("Q:", "Question:")}], "back": null, "why": "w"}));
    assert_eq!(c.coord(), 1, "the check");
    std::fs::write(c.text.join("calls/a.call"), CALL.replace("Q:", "Asked:")).unwrap();
    c.settle();
    assert!(why(&c.status(t)).contains("not what the check read"));
    assert_eq!(std::fs::read_to_string(c.text.join("calls/a.call")).unwrap(), CALL.replace("Q:", "Asked:"), "the other edit stands");
    assert_eq!(c.head(), start);
    // another git holds the index: the load refuses at once, and waits for nothing
    std::fs::write(c.text.join("calls/a.call"), CALL).unwrap();
    let lock = c.text.join(".git/index.lock");
    std::fs::write(&lock, "").unwrap();
    let t = c.change("dev", &[("kinds/sum.kind", Some(SUM))], "w");
    assert!(why(&c.status(t)).contains("index is held by another git"));
    assert!(!c.text.join("kinds/sum.kind").exists());
    std::fs::remove_file(&lock).unwrap();
    // what someone else staged stays staged, out of the change's commit
    std::fs::write(c.text.join("kinds/e.kind"), format!("{KIND}// a note\n")).unwrap();
    git(&c.text, &["add", "kinds/e.kind"]);
    let t = c.change("dev", &[("kinds/sum.kind", Some(SUM))], "w");
    assert_eq!(result(&c.status(t))["head"], "Live");
    assert_eq!(git(&c.text, &["show", "--name-only", "--format="]), "kinds/sum.kind", "only its own path");
    assert_eq!(git(&c.text, &["diff", "--cached", "--name-only"]), "kinds/e.kind", "the other's staging stands");
}

#[test]
fn what_is_not_the_projects_text_is_refused() {
    let mut c = Coord::new("paths", &[]);
    let start = c.head();
    let base = std::fs::canonicalize(agent_base()).unwrap();
    let outside = c.dir.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("x.kind"), "kind x\n").unwrap();
    // a link in the project's folder to a folder outside it, and a kind file that is a link
    std::os::unix::fs::symlink(&outside, c.text.join("linked")).unwrap();
    std::os::unix::fs::symlink(outside.join("x.kind"), c.text.join("kinds/link.kind")).unwrap();
    git(&c.text, &["add", "-A"]);
    git(&c.text, &["commit", "-q", "-m", "links"]);
    let start2 = c.head();
    assert_ne!(start, start2);
    let cases: Vec<(String, &str)> = vec![
        ("../outside/x.kind".into(), "`..`"),
        ("kinds/../../outside/x.kind".into(), "`..`"),
        // in the coord's folder, which holds this scratch project: the record's place; and outside both
        (outside.join("x.kind").display().to_string(), "in the coord's folder"),
        (base.parent().unwrap().join("x.kind").display().to_string(), "not in the project's text"),
        (base.join("kinds/x.kind").display().to_string(), "agent-base's"),
        (base.join("kinds/text-change.kind").display().to_string(), "agent-base's"),
        (c.dir.join("tickets/log.jsonl").display().to_string(), "the record's"),
        ("linked/x.kind".into(), "not the project's text"),
        ("kinds/link.kind".into(), "is a link"),
        (".git/config".into(), "a name of letters"),
        ("src/main.rs".into(), "not the project's text"),
        ("kinds/x.txt".into(), "not the project's text"),
        ("look.toml".into(), "not the project's text"),
        // the standing text beside the project file alone
        ("kinds/standing.md".into(), "not the project's text"),
        ("notes.md".into(), "not the project's text"),
        // the files a session runs with, which no agent's edit changes either (docket-core3, N-865 post 10; N-926)
        ("claude-settings.json".into(), "not the project's text"),
        ("kinds/claude-settings.json".into(), "not the project's text"),
        (".claude/settings.json".into(), "a name of letters"),
        (c.dir.join("mcp/symbols").display().to_string(), "in the coord's folder"),
    ];
    for (path, w) in &cases {
        let t = c.change("dev", &[(path.as_str(), Some("kind x\n"))], "w");
        assert!(why(&c.status(t)).contains(w), "{path}: {:?}", c.status(t));
    }
    // the project file: only its views with no source
    let t = c.change("dev", &[("project.toml", Some("[project]\nname = \"other\"\n"))], "w");
    assert!(why(&c.status(t)).contains("only its [[view]] entries"));
    let t = c.change("dev", &[("project.toml", Some("[project]\nname = \"e\"\n\n[[view]]\nid = \"x\"\nkind = \"x\"\nsource = \"./x\"\n"))], "w");
    assert!(why(&c.status(t)).contains("a view with a command rides a train"));
    // a project whose text is in the system's own repository (the one agent-base is in): refused, whatever the path;
    // a repository of its own here, as a job's tree on the build machine comes with no .git
    let system = c.dir.join("system");
    for d in ["agent-base/kinds", "yaaihh"] {
        std::fs::create_dir_all(system.join(d)).unwrap();
    }
    std::fs::write(system.join("agent-base/kinds/k.kind"), "kind k\n").unwrap();
    git(&system, &["init", "-q"]);
    git(&system, &["add", "-A"]);
    git(&system, &["commit", "-q", "-m", "the system"]);
    let mut at = c.core.text_at().unwrap();
    at.base = system.join("agent-base").display().to_string();
    at.project = system.join("yaaihh").display().to_string();
    let job = crate::change::Job { t: 0, step: 1, ticket: "T-0".into(), call: CHECK.into(), files: vec![crate::change::File { path: "x.kind".into(), text: Some("kind x\n".into()) }], back: None, commit: None, read: String::new(), why: "w".into(), by: "dev".into(), at: Some(at), first: 1, not_ours: None };
    match check(&job) {
        crate::change::Checked::Refused(v) => assert!(v["why"].as_str().unwrap().contains("the system's own repository"), "{v}"),
        _ => panic!("the system's repository is never changed this way"),
    }
    assert_eq!(c.head(), start2, "no commit");
    assert!(c.reloads().is_empty());
    assert_eq!(std::fs::read_to_string(outside.join("x.kind")).unwrap(), "kind x\n");
}

/// agent-base's text-change, its decision taken out: a project's kind of that name.
fn no_decide() -> String {
    let k = std::fs::read_to_string(format!("{}/kinds/text-change.kind", agent_base())).unwrap();
    let from = k.find("  pattern\n").unwrap();
    format!("{}  pattern\n    l <- step coord load_text(files, \"0\", why);\n    match l {{\n      Live(c) => return Live(c, none)\n      Refused(w, at) => return Refused(w, at)\n    }}\n", &k[..from])
}

/// A kind of the project's that steps `coord load_text` itself.
const SNEAK: &str = "kind sneak\n  form files: [TextFile], why: Text\n  result Text\n  pattern\n    l <- step coord load_text(files, \"0\", why);\n    match l {\n      Live(c) => return \"live\"\n      Refused(w, at) => return w\n    }\n";

#[test]
fn only_agent_bases_text_change_reaches_the_load() {
    // a kind of the project's that steps coord itself: coord answers it with a refusal, and writes nothing
    let mut c = Coord::make("own", Make { kinds: &[("sneak.kind", SNEAK)], ..Default::default() });
    let start = c.head();
    let s = c.open_only("sneak", "dev", json!({"files": [{"path": "kinds/sum.kind", "text": SUM}], "why": "w"}));
    c.settle();
    let id = c.core.tickets.m.id(s);
    assert_eq!(c.status(s), Status::Done(json!(format!("coord answers only agent-base's text-change tickets, and {id} is a sneak"))));
    assert!(!c.text.join("kinds/sum.kind").exists());
    assert_eq!(c.head(), start);
    // a change that would add a kind or call of a name agent-base's text-change stands on: refused at the check
    let t = c.change("dev", &[("kinds/mine.kind", Some(&no_decide()))], "w");
    assert!(why(&c.status(t)).contains("kind text-change is agent-base's text-change's own"), "{:?}", c.status(t));
    let load = "call load_text(files: [TextFile], read: Text, why: Text) -> Live(commit: Ref) | Refused(why: Text, at: [Text])\n  task t\n";
    let t = c.change("dev", &[("calls/mine.call", Some(load))], "w");
    let w = why(&c.status(t));
    let real = std::fs::canonicalize(&c.text).unwrap().join("calls/mine.call:1").display().to_string();
    assert!(w.contains("call load_text is agent-base's text-change's own") && result(&c.status(t))["at"] == json!([real]), "{:?}", c.status(t));
    assert!(!c.text.join("kinds/mine.kind").exists() && !c.text.join("calls/mine.call").exists());
    // a ticket whose program was edited since it opened (`coord edit`): refused at its next step
    let t = c.open_only("text-change", "dev", json!({"files": [{"path": "kinds/sum.kind", "text": SUM}], "back": null, "why": "w"}));
    c.core.tickets.m.tickets.edit().get_mut(&t).unwrap().program.push_str("\n// edited\n");
    c.settle();
    assert!(why(&c.status(t)).contains("program is not agent-base's text-change as its file has it"), "{:?}", c.status(t));
    // a load whose files are not those its check saw: refused, nothing written
    let t = c.open_only("text-change", "dev", json!({"files": [{"path": "kinds/sum.kind", "text": SUM}], "back": null, "why": "w"}));
    assert_eq!(c.coord(), 1, "the check");
    c.tasks();
    let d = c.core.ticket_due(COORD).pop().unwrap();
    let mut job = c.core.text_job(&d).unwrap();
    assert_eq!(job.call, LOAD);
    job.files[0].text = Some(SNEAK.into());
    c.run(&job);
    assert!(why(&c.status(t)).contains("not what the check read"));
    assert!(!c.text.join("kinds/sum.kind").exists());
    assert_eq!(c.head(), start);
    assert!(c.reloads().is_empty());
    drop(c);

    // a project's own kind named text-change, with no decision: coord answers none of its tickets
    let mut c = Coord::make("shadowed", Make { kinds: &[("text-change.kind", &no_decide())], ..Default::default() });
    let start = c.head();
    let t = c.change("dev", &[("kinds/sum.kind", Some(SUM))], "w");
    assert!(why(&c.status(t)).contains("is read from") && why(&c.status(t)).contains("not agent-base"), "{:?}", c.status(t));
    assert!(!c.text.join("kinds/sum.kind").exists());
    assert_eq!(c.head(), start);
}

/// A project's own text-change.eflint, in place of agent-base's: its kinds are read by the lead first, its policy is
/// refused; and its norms say who may open one.
const STRICT: &str = r#"Fact text-read-option Identified by String
+text-read-option("Go").
+text-read-option("Read").
+text-read-option("No").

Decision text-read
  Options text-read-option

// Kinds and calls of this project are read first, the owner's too.
Only for text-read When call.policy-count == 0:
  option == "Read"

// This project's policy changes on the trains.
Only for text-read When call.policy-count > 0:
  option == "No"

"#;

const OPENS: &str = r#"Act open
  Actor agent
  Holds when not ticket-kind("new", "text-change")
  Holds when ticket-kind("new", "text-change") and role(actor, "lead")
"#;

#[test]
fn the_projects_policy_decides_who_may_and_what_is_read_first() {
    // agent-base's rule: kinds go at once (above); a policy file is read by the lead first
    let mut c = Coord::new("policy", &[]);
    let rule = "Fact note Identified by String\n";
    let t = c.change("dev", &[("policy/note.eflint", Some(rule))], "a fact");
    assert!(c.awaits_read(t), "{:?}", c.status(t));
    assert!(!c.text.join("policy/note.eflint").exists(), "nothing written while it is read");
    c.read(t, json!({"head": "Go"}));
    assert_eq!(result(&c.status(t))["head"], "Live");
    assert!(c.text.join("policy/note.eflint").exists());
    // the lead's No: nothing changes
    let start = c.head();
    let t = c.change("dev", &[("policy/other.eflint", Some("Fact other Identified by String\n"))], "another");
    c.read(t, json!({"head": "No", "why": "not now"}));
    assert_eq!(result(&c.status(t)), json!({"head": "Refused", "why": "not now", "at": []}));
    assert_eq!(c.head(), start);
    // nobody running to read it: refused, and never the owner's step
    c.core.agents.get_mut("lead").unwrap().live.pid = None;
    let t = c.change("dev", &[("policy/other.eflint", Some("Fact other Identified by String\n"))], "another");
    assert!(why(&c.status(t)).contains("nobody who may read it is running now"), "{:?}", c.status(t));
    assert!(c.owner_steps().is_empty(), "{:?}", c.owner_steps());
    assert_eq!(c.head(), start);
    c.core.agents.get_mut("lead").unwrap().live.pid = Some(1);
    // the owner's own change (his window's Save): at once, a policy file too
    let t = c.change("owner", &[("policy/mine.eflint", Some("Fact mine Identified by String\n"))], "his");
    assert_eq!(result(&c.status(t))["head"], "Live", "{:?}", c.status(t));
    drop(c);

    // a project's own rule in place of agent-base's: its kinds are read first, its policy refused with the rule's why
    let mut c = Coord::new("strict", &[("text-change.eflint", STRICT), ("opens.eflint", OPENS)]);
    let t = c.change("dev", &[("kinds/sum.kind", Some(SUM))], "a kind");
    assert!(c.awaits_read(t), "{:?}", c.status(t));
    let t = c.change("dev", &[("policy/note.eflint", Some(rule))], "a fact");
    assert_eq!(result(&c.status(t)), json!({"head": "Refused", "why": "This project's policy changes on the trains.", "at": []}));
    // who may open one: the project's `Act open`, never the engine's
    let open = coord_types::Request::Open { kind: "text-change".into(), under: None, form: json!({}), text: None };
    let may = |c: &Coord, who: &str| crate::ticket::may(c.core.ticket_may(&Who::Agent(who.into()), &open).unwrap());
    assert!(may(&c, "dev").is_err_and(|e| e.contains("You may not open a text-change")), "dev may not");
    assert!(may(&c, "lead").is_ok(), "the lead may");
}

/// A kind of the project's whose program opens a text-change.
const RELAY: &str = "kind relay\n  form files: [TextFile], why: Text\n  result Text\n  pattern\n    r <- open text-change(files, none, why);\n    match r {\n      Live(c, s) => return \"live\"\n      Unseen(c, w) => return w\n      Back(what, s, c) => return what\n      Stuck(what, s, w) => return w\n      Refused(w, at) => return w\n    }\n";

#[test]
fn a_change_a_program_opens_is_its_openers_never_the_owners() {
    let mut c = Coord::make("relay", Make { kinds: &[("relay.kind", RELAY)], ..Default::default() });
    // dev's relay opens the change: dev's, so a policy file is read by the lead first, not the owner's Go
    let r = c.open_only("relay", "dev", json!({"files": [{"path": "policy/note.eflint", "text": "Fact note Identified by String\n"}], "why": "a fact"}));
    c.settle();
    let t = *c.core.tickets.m.tickets.values().filter(|x| x.kind == "text-change").map(|x| &x.t).next().expect("the relay opened one");
    assert_eq!(c.checked(t)["by"], "dev", "the nearest opener above it");
    assert!(c.awaits_read(t), "{:?}", c.status(t));
    c.read(t, json!({"head": "No", "why": "not this"}));
    assert_eq!(c.status(r), Status::Done(json!("not this")));
    assert!(!c.text.join("policy/note.eflint").exists());
}

#[test]
fn what_opens_tickets_or_takes_the_owners_words_is_read_first() {
    let mut c = Coord::new("counts", &[]);
    // a binding: tickets opened with nobody asking
    let bound = format!("{SUM}\non owner_message(m) open sum(m.text)\n");
    let t = c.change("dev", &[("kinds/sum.kind", Some(&bound))], "sum for each message");
    assert!(c.awaits_read(t), "{:?}", c.status(t));
    assert_eq!(c.checked(t)["bindings"], json!(["on owner_message(m) open sum(m.text)"]));
    // a kind's own handler of the owner's words
    let hears = "kind hears\n  form q: Text\n  result ()\n  pattern\n    a <- step dev ask(q);\n    return ()\n  when owner_post(p) do\n    return ()\n";
    let t = c.change("dev", &[("kinds/hears.kind", Some(hears))], "a kind that takes the owner's posts");
    assert!(c.awaits_read(t), "{:?}", c.status(t));
    assert_eq!(c.checked(t)["handlers"], json!(["hears"]));
    // a kind in the place of agent-base's
    let note = std::fs::read_to_string(format!("{}/kinds/note.kind", agent_base())).unwrap();
    let t = c.change("dev", &[("kinds/note.kind", Some(&note))], "our own note");
    assert!(c.awaits_read(t), "{:?}", c.status(t));
    assert_eq!(c.checked(t)["shadows"], json!(["note"]));
    // a rendering alone, or a kind with none of these: at once
    let t = c.change("dev", &[("renderings/ask.tmpl", Some("Q: {{ q }}\n"))], "a rendering");
    assert_eq!(result(&c.status(t))["head"], "Live", "{:?}", c.status(t));
    assert_eq!(c.checked(t)["renderings"], json!(["renderings/ask.tmpl"]));
    // the standing text, a prompt text as a rendering is (paper-research N-1446 p2884): at once, written and committed
    let t = c.change("dev", &[("standing.md", Some("Say the answer first.\n"))], "the standing text");
    assert_eq!(result(&c.status(t))["head"], "Live", "{:?}", c.status(t));
    assert_eq!(c.checked(t)["renderings"], json!(["standing.md"]));
    assert_eq!(std::fs::read_to_string(c.text.join("standing.md")).unwrap(), "Say the answer first.\n");
    assert_eq!(git(&c.repo, &["log", "-1", "--format=%H", "--", "standing.md"]).trim(), c.head());
    // the owner's own binding: his, at once
    let t = c.change("owner", &[("kinds/sum.kind", Some(&bound))], "his");
    assert_eq!(result(&c.status(t))["head"], "Live", "{:?}", c.status(t));
    assert!(c.owner_steps().is_empty(), "{:?}", c.owner_steps());
}

#[test]
fn the_way_back_is_the_same_act_with_the_commit() {
    let mut c = Coord::new("back", &[]);
    let start = c.head();
    let t = c.change("dev", &[("kinds/sum.kind", Some(SUM)), ("calls/a.call", Some(&CALL.replace("Q:", "Question:")))], "sum, and a clearer ask");
    let commit = result(&c.status(t))["commit"].as_str().unwrap().to_string();
    // back: the files as they were before that commit, live and committed again
    let b = c.open("dev", json!({"files": [], "back": commit, "why": "undo"}));
    let r = result(&c.status(b));
    assert_eq!(r["head"], "Live", "{r}");
    assert_eq!(git(&c.text, &["rev-parse", "HEAD^"]), commit);
    assert_eq!(git(&c.text, &["diff", &start, "HEAD", "--stat"]), "", "the text is as it was");
    assert!(!c.core.tickets.m.defs.kinds.contains_key("sum"), "the kind no ticket has goes");
    assert_eq!(c.reloads().len(), 2);
    // a commit whose files changed since: refused, naming the file
    let t = c.change("dev", &[("kinds/sum.kind", Some(SUM))], "sum again");
    let again = result(&c.status(t))["commit"].as_str().unwrap().to_string();
    let t = c.change("dev", &[("kinds/sum.kind", Some(&SUM.replace("one step", "a single step")))], "a better doc");
    assert_eq!(result(&c.status(t))["head"], "Live");
    let head = c.head();
    let b = c.open("dev", json!({"files": [], "back": again, "why": "undo"}));
    assert!(why(&c.status(b)).contains("changed since"));
    assert_eq!(c.head(), head);
    // once a ticket has the kind, the way back that would take it away is refused: the record replays with it
    c.open_only("sum", "dev", json!({"q": "q"}));
    let b = c.open("dev", json!({"files": [], "back": head, "why": "undo the doc"}));
    assert_eq!(result(&c.status(b))["head"], "Live", "a doc line back is fine: {:?}", c.status(b));
    let b = c.open("dev", json!({"files": [], "back": again, "why": "undo sum"}));
    assert!(why(&c.status(b)).contains("stand on it"), "{:?}", c.status(b));
    assert!(c.core.tickets.m.defs.kinds.contains_key("sum"));
}

#[test]
fn a_view_with_no_source_is_loaded_into_the_projects_file() {
    let mut c = Coord::new("views", &[]);
    // the owner's view with a command, by his hand
    let sourced = "[project]\nname = \"e\"\n\n[[view]]\nid = \"runs\"\nkind = \"stages\"\nsource = \"./runs\"\n";
    std::fs::write(c.text.join("project.toml"), sourced).unwrap();
    git(&c.text, &["commit", "-q", "-am", "runs"]);
    let start = c.head();
    let board = "\n[[view]]\nid = \"items\"\nkind = \"board\"\ntitle = \"Items\"\n";
    let seq = c.seq();
    let t = c.change("dev", &[("project.toml", Some(&format!("{sourced}{board}")))], "a board of the items");
    assert_eq!(result(&c.status(t))["head"], "Live", "{:?}", c.status(t));
    assert_eq!(c.checked(t)["views"], json!(["items"]));
    let snap = c.core.snapshot();
    let coord_types::Body::Snapshot(s) = &snap.body else { panic!("a snapshot") };
    let views = s.project.as_ref().unwrap()["view"].clone();
    assert_eq!(views, json!([{"id": "runs", "kind": "stages", "source": "./runs"}, {"id": "items", "kind": "board", "title": "Items"}]), "the window's views: its snapshot's project file");
    // a window already connected has the view at once: a snapshot is sent after the load (N-1446 p1785 4.3)
    let sent: Vec<Value> = c.core.since(seq).unwrap().into_iter().filter_map(|e| match e.body { coord_types::Body::Snapshot(s) => s.project.map(|p| p["view"].clone()), _ => None }).collect();
    assert_eq!(sent.last(), Some(&views), "the snapshot sent after the load carries the new view: {sent:?}");
    // its still is the view's own page (p1787 2)
    let stand_in = |c: &Coord| (std::fs::read_to_string(c.dir.join("stand-in.args")).unwrap(), std::fs::read_to_string(c.dir.join("stand-in.env")).unwrap());
    let (args, env) = stand_in(&c);
    assert!(args.contains("\n--scene\noverview\n") && env.lines().any(|l| l == "YAAIHH_VIEW=items"), "{args} {env}");
    // the command of a view, or the file's other lines: refused, the file as it is
    let now = std::fs::read_to_string(c.text.join("project.toml")).unwrap();
    let head = c.head();
    assert_ne!(head, start);
    for (to, w) in [
        (now.replace("./runs", "./other"), "a view with a command rides a train"),
        (format!("{now}\n[[view]]\nid = \"x\"\nkind = \"stages\"\nsource = \"./x\"\n"), "a view with a command rides a train"),
        (now.replace("name = \"e\"\n", "name = \"e\"\nnorms = \"clingo\"\n"), "only its [[view]] entries"),
    ] {
        let t = c.change("dev", &[("project.toml", Some(&to))], "w");
        assert!(why(&c.status(t)).contains(w), "{to}: {:?}", c.status(t));
        assert_eq!(std::fs::read_to_string(c.text.join("project.toml")).unwrap(), now);
        assert_eq!(c.head(), head);
    }
    // a change that only takes a view away has no page of its own: the project's first view
    let t = c.change("dev", &[("project.toml", Some(sourced))], "no board");
    assert_eq!(result(&c.status(t))["head"], "Live", "{:?}", c.status(t));
    let (args, env) = stand_in(&c);
    assert!(args.contains("\n--scene\noverview\n") && !env.contains("YAAIHH_VIEW="), "{args} {env}");
}

/// paper-research N-1446 p1789, 2: a template (`kind = "lens"`) changes by the act as any view with no source does,
/// and one the window's check refuses (an agent's folder or session, more than 3 hops) is refused before anything is
/// written, at the line of the real file that names it.
#[test]
fn a_template_is_refused_by_the_windows_check_before_anything_is_written() {
    let mut c = Coord::new("templates", &[]);
    let base = std::fs::read_to_string(c.text.join("project.toml")).unwrap();
    let made = "\n[[view]]\nid = \"edge6-made\"\nkind = \"lens\"\ntitle = \"What docket-edge6 made this week\"\nnodes = [\"commit\", \"job\", \"build\"]\nwithin = \"7d\"\n\n[view.ask]\naround = \"agent:docket-edge6\"\nhops = 2\nlayers = [\"work\"]\n";
    let line = base.lines().count() + 3;
    let real = std::fs::canonicalize(&c.text).unwrap().join("project.toml").display().to_string();
    let head = c.head();
    for (to, says) in [("hops = 4", "`ask.hops`: a whole number from 0 to 3"), ("hops = 2\nfolder = \"docket-edge6\"", "`ask.folder`"), ("hops = 2\nsession = \"session:5c6f1a2b\"", "`ask.session`")] {
        let t = c.change("dev", &[("project.toml", Some(&format!("{base}{}", made.replace("hops = 2", to))))], "w");
        let w = why(&c.status(t));
        assert!(w.contains(&format!("{real}:{line}: the view edge6-made is not a template the window takes")) && w.contains(says), "{to}: {w}");
        assert_eq!(result(&c.status(t))["at"], json!([format!("{real}:{line}")]), "{to}: at its line");
        assert_eq!(std::fs::read_to_string(c.text.join("project.toml")).unwrap(), base, "{to}: nothing written");
        assert_eq!(c.head(), head, "{to}: no commit");
    }
    let t = c.change("dev", &[("project.toml", Some(&format!("{base}{made}")))], "what docket-edge6 made");
    assert_eq!(result(&c.status(t))["head"], "Live", "{:?}", c.status(t));
    assert_eq!(c.checked(t)["views"], json!(["edge6-made"]));
    let snap = c.core.snapshot();
    let coord_types::Body::Snapshot(s) = &snap.body else { panic!("a snapshot") };
    let view = s.project.as_ref().unwrap()["view"][0].clone();
    assert_eq!(view["ask"], json!({"around": "agent:docket-edge6", "hops": 2, "layers": ["work"]}), "the window's template: its snapshot's project file");
    assert!(coord_types::template::check(&view).is_ok(), "{view}");
}

#[test]
fn the_flavours_look_changes_by_the_same_act() {
    let mut c = Coord::make("look", Make { flavour: true, ..Default::default() });
    let look = c.repo.join("look.toml");
    let start = c.head();
    // in the window's words for a look (coord_types::look, here-2's head A): all seven colours, as a look that gives
    // one gives (consult-5, N-412 post 2013), the doc's dark palette with a calmer accent
    let calm = "[colour]\npage = \"#101114\"\nread = \"#e8e8ea\"\nsupport = \"#a0a1a6\"\nquiet = \"#6c6d72\"\nlayer = \"#1a1b1f\"\nborder = \"#2a2b30\"\nattention = \"#336699\"\n";
    let calm_v = json!({"colour": {"page": "#101114", "read": "#e8e8ea", "support": "#a0a1a6", "quiet": "#6c6d72", "layer": "#1a1b1f", "border": "#2a2b30", "attention": "#336699"}});
    let seq = c.seq();
    let t = c.change("dev", &[("look.toml", Some(calm))], "a calmer accent");
    let r = result(&c.status(t));
    assert_eq!(r["head"], "Live", "{r}");
    assert_eq!(c.checked(t)["head"], "Look");
    assert_eq!(c.head(), r["commit"].as_str().unwrap());
    assert_eq!(git(&c.repo, &["show", "--name-only", "--format="]), "look.toml");
    assert_eq!(c.core.look, Some(calm_v.clone()), "in memory, for the window");
    assert!(matches!(&c.reloads()[..], [Line::Reload { commit: Some(h), .. }] if h == r["commit"].as_str().unwrap()));
    // sent at once to the windows that read it (coordc's `for_app` keeps it from an older one), and in each snapshot
    let looks = |c: &Coord, seq: u64| -> Vec<Value> { c.core.since(seq).unwrap().into_iter().filter_map(|e| if let coord_types::Body::Look(l) = e.body { Some(l) } else { None }).collect() };
    // and once more when the still's stand-in window says its link is up, so it draws the look as it came on that link
    assert_eq!(looks(&c, seq), vec![calm_v.clone(), calm_v.clone()]);
    assert!(matches!(c.core.snapshot_with(Vec::new()).body, coord_types::Body::Snapshot(s) if s.look == Some(calm_v.clone())));
    // the project's text beside it, in the same repository, still changes by itself, and sends no look
    let seq = c.seq();
    let t = c.change("dev", &[("kinds/sum.kind", Some(SUM))], "sum");
    assert_eq!(result(&c.status(t))["head"], "Live");
    assert!(looks(&c, seq).is_empty());
    let head = c.head();
    // refused: too long, not TOML, not a look the window takes (in the window's own words), with the project's text
    let long = format!("[colour]\nnote = \"{}\"\n", "x".repeat(9000));
    let seq = c.seq();
    for (files, w) in [
        (vec![("look.toml", Some(long.as_str()))], "at most 8 KB"),
        (vec![("look.toml", Some("[colour\n"))], "does not read"),
        (vec![("look.toml", Some("[colour]\nread = \"#e8e8ea\"\n"))], "is not a look the window takes: "),
        (vec![("look.toml", Some("[colors]\naccent = \"#336699\"\n"))], "`colors`: no such part of the look"),
        (vec![("look.toml", Some(calm)), ("kinds/x.kind", Some(SUM))], "two changes"),
    ] {
        let t = c.change("dev", &files, "w");
        assert!(why(&c.status(t)).contains(w), "{w}: {:?}", c.status(t));
    }
    assert_eq!(std::fs::read_to_string(&look).unwrap(), calm, "the window keeps what it had");
    assert_eq!(c.head(), head);
    assert!(looks(&c, seq).is_empty(), "nothing sent for a refused look");
    // a second look, then the way back to the first, byte for byte, and sent
    let bold = "# bolder\n[colour]\npage = \"#000000\"\nread = \"#ffffff\"\nsupport = \"#a0a1a6\"\nquiet = \"#6c6d72\"\nlayer = \"#1a1b1f\"\nborder = \"#2a2b30\"\nattention = \"#e5484d\"\n";
    let t = c.change("dev", &[("look.toml", Some(bold))], "bolder");
    let r2 = result(&c.status(t));
    assert_eq!(r2["head"], "Live", "{r2}");
    let seq = c.seq();
    let b = c.open("dev", json!({"files": [], "back": r2["commit"], "why": "the calmer one again"}));
    assert_eq!(result(&c.status(b))["head"], "Live", "{:?}", c.status(b));
    assert_eq!(std::fs::read(&look).unwrap(), calm.as_bytes());
    assert_eq!(looks(&c, seq), vec![calm_v.clone(), calm_v.clone()]);
    // and back to none: the look as it was before the first (none), sent as null
    let seq = c.seq();
    let b = c.open("dev", json!({"files": [], "back": r["commit"], "why": "undo"}));
    assert_eq!(result(&c.status(b))["head"], "Live", "{:?}", c.status(b));
    assert!(!look.exists() && c.core.look.is_none());
    assert_eq!(looks(&c, seq), vec![Value::Null, Value::Null]);
    assert!(matches!(c.core.snapshot_with(Vec::new()).body, coord_types::Body::Snapshot(s) if s.look.is_none()));
    assert_ne!(c.head(), start);
    // a link in the look's place, into the folder of what sessions run (agent-base's claude/): refused, nothing written
    // there (docket-core3, N-412 p1119)
    let claude = c.dir.join("agent-base/claude");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(claude.join("hook"), "#!/bin/sh\n").unwrap();
    std::os::unix::fs::symlink(claude.join("hook"), &look).unwrap();
    let t = c.change("dev", &[("look.toml", Some("[a]\n"))], "w");
    assert!(why(&c.status(t)).contains("is a link"), "{:?}", c.status(t));
    assert_eq!(std::fs::read_to_string(claude.join("hook")).unwrap(), "#!/bin/sh\n");
    assert_eq!(std::fs::read_dir(&claude).unwrap().count(), 1);
    std::fs::remove_file(&look).unwrap();
    drop(c);

    // while the project's policy rules on the look's decision, which is the system's: no look changes
    let mine = "Fact look-read-option Identified by String\n+look-read-option(\"Go\").\n+look-read-option(\"Read\").\n+look-read-option(\"No\").\n\nDecision look-read\n  Options look-read-option\n\nOnly for look-read:\n  option == \"Go\"\n";
    let mut c = Coord::make("look-ours", Make { flavour: true, ..Default::default() });
    // a change that would put it there is refused at its own check
    for path in ["policy/look.eflint", "policy/mine.eflint"] {
        let t = c.change("dev", &[(path, Some(mine))], "w");
        assert!(why(&c.status(t)).contains("`look-read`, the look's decision, is the system's, and a project's policy cannot rule on it"), "{path}: {:?}", c.status(t));
    }
    drop(c);
    // put there by hand: the look's change is refused while it stands
    let mut c = Coord::make("look-hand", Make { flavour: true, policy: &[("look.eflint", mine)], ..Default::default() });
    let t = c.change("dev", &[("look.toml", Some("[a]\n"))], "w");
    assert!(why(&c.status(t)).contains("the look's rule (`look-read`) is the system's"), "{:?}", c.status(t));
    // named by its place in the project for him, its path for the window to open
    assert!(why(&c.status(t)).contains("(policy/look.eflint)"), "{:?}", c.status(t));
    assert!(result(&c.status(t))["at"].as_array().is_some_and(|a| a.len() == 1 && a[0].as_str().is_some_and(|p| p.ends_with("/policy/look.eflint"))), "{:?}", c.status(t));
    assert!(!c.repo.join("look.toml").exists());
}

/// A kind of the project's that steps `coord still_text` itself.
const PEEK: &str = "kind peek\n  form files: [TextFile], commit: Ref\n  result Text\n  pattern\n    s <- step coord still_text(files, commit);\n    match s {\n      Still(p, b, g) => return p\n      Own => return \"own\"\n      Unseen(w) => return w\n    }\n";

/// The step after the load (consult-lead N-412 p1203; the owner's N-1104 p81, "agent changes can go live they then
/// look at a real ss to see if it works like a human would"): the agent that made the change looks at a still drawn by
/// a stand-in window that took the change the way his window does, and Wrong takes it back by its own commit. The
/// stand-in starts with only its own environment, connected as his window is (`coordc bridge` on this coord), on the
/// page the change touches, and for the look waits for it on its link, where coordd sends it again once the link is
/// up; the picture's path is coordd's alone (the coord's `stills`, by ticket and step) and stands on the ticket.
#[test]
fn the_agent_looks_at_a_still_of_its_change_and_wrong_takes_it_back() {
    let mut c = Coord::make("still", Make { flavour: true, kinds: &[("peek.kind", PEEK)], ..Default::default() });
    c.looks = None;
    let look = c.repo.join("look.toml");
    let calm = "[colour]\npage = \"#101114\"\nread = \"#e8e8ea\"\nsupport = \"#a0a1a6\"\nquiet = \"#6c6d72\"\nlayer = \"#1a1b1f\"\nborder = \"#2a2b30\"\nattention = \"#336699\"\n";
    let calm_v = json!({"colour": {"page": "#101114", "read": "#e8e8ea", "support": "#a0a1a6", "quiet": "#6c6d72", "layer": "#1a1b1f", "border": "#2a2b30", "attention": "#336699"}});
    let looks = |c: &Coord, seq: u64| -> Vec<Value> { c.core.since(seq).unwrap().into_iter().filter_map(|e| if let coord_types::Body::Look(l) = e.body { Some(l) } else { None }).collect() };
    let seq = c.seq();
    let t = c.change("dev", &[("look.toml", Some(calm))], "a calmer accent");
    let changed = c.head();
    assert_eq!(std::fs::read_to_string(&look).unwrap(), calm, "live before the look");
    // the look owed to dev, at the picture coordd chose: the coord's stills, named by the ticket and the step
    let (agent, still, page) = c.owed_look(t).expect("the look at the still is owed");
    let id = c.core.tickets.m.id(t);
    assert_eq!((agent.as_str(), page.as_str()), ("dev", "dev's agent page"));
    assert!(still.starts_with(&format!("{}/stills/{id}-step", c.dir.display())) && still.ends_with(".png"), "{still}");
    assert_eq!(std::fs::read(&still).unwrap(), b"png");
    // the stand-in: the agent page drawn there, connected as his window is, waiting for the look on its link; of
    // coordd's environment nothing but these and what draws on the machine
    let args = std::fs::read_to_string(c.dir.join("stand-in.args")).unwrap();
    assert_eq!(args.lines().collect::<Vec<_>>(), ["--snap", still.as_str(), "--scene", "agent", "--size", "1400x900", "--at", "1.0"]);
    let env = std::fs::read_to_string(c.dir.join("stand-in.env")).unwrap();
    let var = |k: &str| env.lines().find_map(|l| l.strip_prefix(&format!("{k}=")).map(String::from));
    assert_eq!(
        (var("YAAIHH_LOCAL_PIPE"), var("COORD"), var("YAAIHH_STILL_AFTER"), var("YAAIHH_AGENT"), var("HOME"), var("YAAIHH_CONFIG")),
        (Some("1".into()), Some(c.dir.display().to_string()), Some("look".into()), Some("dev".into()), Some(c.dir.join("stand-in").display().to_string()), Some(c.dir.join("stand-in/projects.toml").display().to_string()))
    );
    let given = ["PATH", "HOME", "YAAIHH_CONFIG", "YAAIHH_LOCAL_PIPE", "COORD", "COORDD_SOCK", "AGENT_BASE", "YAAIHH_AGENT", "YAAIHH_STILL_AFTER", "LD_LIBRARY_PATH", "XDG_RUNTIME_DIR", "WAYLAND_DISPLAY", "DISPLAY"];
    // the shell's own: PWD, SHLVL, OLDPWD, _
    let other: Vec<&str> = env.lines().filter_map(|l| l.split_once('=').map(|(k, _)| k)).filter(|k| !given.contains(k) && !["WGPU_", "MESA_", "VK_", "LIBGL_", "GALLIUM_"].iter().any(|p| k.starts_with(p)) && !["PWD", "SHLVL", "OLDPWD", "_"].contains(k)).collect();
    assert!(other.is_empty(), "nothing else of coordd's environment: {other:?}");
    assert!(std::fs::read_to_string(c.dir.join("stand-in/projects.toml")).unwrap().contains("boxed = false"));
    // the look sent at the load, and again once the stand-in's link was up
    assert_eq!(looks(&c, seq), vec![calm_v.clone(), calm_v.clone()]);
    // Wrong: taken back by its own commit, the look as it was before it (none), sent so; what was wrong on the ticket
    let what = "the accent is too dark to see on the page";
    c.core.ticket_input(Input::Answer { t, by: "dev".into(), head: json!({"head": "Wrong", "what": what}), text: docket_machine::Text::New(String::new()) }).unwrap();
    let seq = c.seq();
    c.settle();
    let r = result(&c.status(t));
    assert_eq!((r["head"].as_str(), r["what"].as_str(), r["still"].as_str(), r["commit"].as_str()), (Some("Back"), Some(what), Some(still.as_str()), Some(c.head().as_str())), "{r}");
    assert_eq!(git(&c.repo, &["rev-parse", "HEAD^"]), changed);
    assert!(git(&c.repo, &["show", "-s", "--format=%B"]).starts_with(what));
    assert!(!look.exists() && c.core.look.is_none(), "the look as it was before the change: none");
    assert_eq!(looks(&c, seq), vec![Value::Null], "the way back's look sent; no still of the way back");
    // LooksRight: live, its still beside its commit
    c.looks = Some(json!({"head": "LooksRight"}));
    let t = c.change("dev", &[("look.toml", Some(calm))], "a calmer accent again");
    let r = result(&c.status(t));
    assert_eq!((r["head"].as_str(), r["commit"].as_str()), (Some("Live"), Some(c.head().as_str())), "{r}");
    assert!(r["still"].as_str().is_some_and(|s| s.starts_with(&format!("{}/stills/", c.dir.display()))), "{r}");
    // the owner's own change: his window draws it, no stand-in is started
    std::fs::remove_file(c.dir.join("stand-in.args")).unwrap();
    let bold = "[colour]\npage = \"#000000\"\nread = \"#ffffff\"\nsupport = \"#a0a1a6\"\nquiet = \"#6c6d72\"\nlayer = \"#1a1b1f\"\nborder = \"#2a2b30\"\nattention = \"#e5484d\"\n";
    let o = c.change("owner", &[("look.toml", Some(bold))], "his");
    let r = result(&c.status(o));
    assert_eq!((r["head"].as_str(), r["still"].clone()), (Some("Live"), Value::Null), "{r}");
    assert!(!c.dir.join("stand-in.args").exists());
    // Wrong when the file changed since: the way back is refused, the change stays (Stuck), and says why
    c.looks = None;
    let t = c.change("dev", &[("look.toml", Some(calm))], "calm once more");
    assert!(c.owed_look(t).is_some());
    c.change("owner", &[("look.toml", Some(bold))], "his again");
    c.core.ticket_input(Input::Answer { t, by: "dev".into(), head: json!({"head": "Wrong", "what": "too calm"}), text: docket_machine::Text::New(String::new()) }).unwrap();
    c.settle();
    let r = result(&c.status(t));
    assert_eq!((r["head"].as_str(), r["what"].as_str()), (Some("Stuck"), Some("too calm")), "{r}");
    assert!(r["why"].as_str().unwrap().contains("changed since"), "{r}");
    assert_eq!(std::fs::read_to_string(&look).unwrap(), bold, "his change stays");
    // a project's kind that steps `coord still_text` itself: refused, no stand-in started
    std::fs::remove_file(c.dir.join("stand-in.args")).unwrap();
    let head = c.head();
    let p = c.open_only("peek", "dev", json!({"files": [{"path": look.display().to_string(), "text": calm}], "commit": head}));
    c.settle();
    assert!(matches!(c.status(p), Status::Done(Value::String(ref w)) if w.contains("coord answers only agent-base's")), "{:?}", c.status(p));
    assert!(!c.dir.join("stand-in.args").exists());
    drop(c);

    // a kind: the Kinds page with the kind open, and no look sent
    let mut c = Coord::new("still-kind", &[]);
    c.looks = None;
    let seq = c.seq();
    let t = c.change("dev", &[("kinds/sum.kind", Some(SUM))], "a kind for one question");
    let (_, _, page) = c.owed_look(t).expect("owed");
    assert_eq!(page, "the Kinds page, sum open");
    let args = std::fs::read_to_string(c.dir.join("stand-in.args")).unwrap();
    assert!(args.contains("\n--scene\nkinds\n"), "{args}");
    let env = std::fs::read_to_string(c.dir.join("stand-in.env")).unwrap();
    assert!(env.lines().any(|l| l == "YAAIHH_DOCKET=kind:sum") && !env.contains("YAAIHH_STILL_AFTER"), "{env}");
    assert!(looks(&c, seq).is_empty());
    c.core.ticket_input(Input::Answer { t, by: "dev".into(), head: json!({"head": "LooksRight"}), text: docket_machine::Text::New(String::new()) }).unwrap();
    c.settle();
    assert_eq!(result(&c.status(t))["head"], "Live");
    // no stand-in built beside coordd: live, and Unseen says why
    crate::change::STAND_IN_AT.with(|s| *s.borrow_mut() = None);
    let t = c.change("dev", &[("kinds/sum.kind", Some(&SUM.replace("one step", "a single step")))], "a better doc");
    let r = result(&c.status(t));
    assert_eq!((r["head"].as_str(), r["commit"].as_str()), (Some("Unseen"), Some(c.head().as_str())), "{r}");
    assert!(r["why"].as_str().unwrap().contains("no stand-in window (window-stand-in) is built beside coordd"), "{r}");
    // a stand-in that draws nothing: Unseen with how it ended and its last words
    let failing = c.dir.join("failing.sh");
    crate::accounts::testing::program(&failing, "#!/bin/sh\necho 'no adapter on this machine' >&2\nexit 3\n");
    crate::change::STAND_IN_AT.with(|s| *s.borrow_mut() = Some(failing));
    let t = c.change("dev", &[("kinds/sum.kind", Some(SUM))], "the doc as it was");
    let r = result(&c.status(t));
    assert_eq!(r["head"], "Unseen", "{r}");
    let w = r["why"].as_str().unwrap();
    assert!(w.contains("exit status: 3") && w.contains("no adapter on this machine"), "{w}");
}

#[test]
fn a_load_stopped_midway_ends_at_the_next_start() {
    for stop in ["half", "written", "committed"] {
        let mut c = Coord::new(&format!("stop-{stop}"), &[]);
        let start = c.head();
        let clearer = CALL.replace("Q:", "Question:");
        crate::change::STOP_AT.with(|s| s.set(Some(stop)));
        let t = c.change("dev", &[("kinds/sum.kind", Some(SUM)), ("calls/a.call", Some(&clearer))], "sum, and a clearer ask");
        crate::change::STOP_AT.with(|s| s.set(None));
        assert!(c.core.tickets.m.tickets[&t].is_open(), "{stop}: coordd stopped before it answered");
        let journal = c.dir.join("text-change.journal");
        assert_eq!(std::fs::metadata(&journal).unwrap().permissions().mode() & 0o777, 0o600, "{stop}");
        let call = || std::fs::read_to_string(c.text.join("calls/a.call")).unwrap();
        match stop {
            "half" => assert!(c.text.join("kinds/sum.kind").exists() && call() == CALL, "one of the two written"),
            "written" => assert!(c.text.join("kinds/sum.kind").exists() && call() == clearer && c.head() == start),
            _ => assert!(git(&c.repo, &["rev-parse", "HEAD^"]) == start && c.reloads().is_empty(), "committed, with no line"),
        }
        c.restart();
        assert!(!journal.exists(), "{stop}: the journal ends at the start");
        if stop == "committed" {
            // forward: the commit's line, written at the start
            let head = c.head();
            assert!(matches!(&c.reloads()[..], [Line::Reload { by, commit: Some(h), .. }] if by == "dev" && *h == head), "{:?}", c.reloads());
        } else {
            // back: the files as they were, nothing staged, no commit
            assert!(!c.text.join("kinds/sum.kind").exists() && std::fs::read_to_string(c.text.join("calls/a.call")).unwrap() == CALL, "{stop}");
            assert_eq!(git(&c.repo, &["status", "--porcelain"]), "");
            assert_eq!(c.head(), start);
            assert!(!c.core.tickets.m.defs.kinds.contains_key("sum"));
        }
        // the load given before the stop runs again
        let owed = c.core.text_owed();
        assert_eq!(owed.iter().map(|j| j.call.as_str()).collect::<Vec<_>>(), vec![LOAD], "{stop}");
        for j in &owed {
            c.run(j);
        }
        c.settle();
        let r = result(&c.status(t));
        assert_eq!(r["head"], "Live", "{stop}: {r}");
        assert_eq!(c.head(), r["commit"].as_str().unwrap(), "{stop}");
        assert_eq!(git(&c.repo, &["rev-parse", "HEAD^"]), start, "{stop}: one commit");
        assert_eq!(c.reloads().len(), 1, "{stop}: one line");
        assert!(c.core.tickets.m.defs.kinds.contains_key("sum") && std::fs::read_to_string(c.text.join("calls/a.call")).unwrap() == clearer);
    }
}

#[test]
fn on_a_base_without_the_kind_a_change_is_left_out_and_forward_it_waits() {
    // a change waiting for its read, as a train's install finds it
    let mut c = Coord::new("old-base", &[]);
    let t = c.change("dev", &[("policy/note.eflint", Some("Fact note Identified by String\n"))], "a fact");
    assert!(c.awaits_read(t));
    let u = c.open_only("text-change", "dev", json!({"files": [{"path": "kinds/sum.kind", "text": SUM}], "back": null, "why": "w"}));
    // the base before the kind: agent-base without its files
    let old = c.dir.join("old-base");
    let o = std::process::Command::new("cp").arg("-a").arg(agent_base()).arg(&old).output().unwrap();
    assert!(o.status.success());
    for f in ["kinds/text-change.kind", "calls/text-change.call", "policy/text-change.eflint", "policy/look.eflint"] {
        std::fs::remove_file(old.join(f)).unwrap();
    }
    let mut again = Core::load(&c.dir.display().to_string(), &old.display().to_string());
    again.project = crate::project::Project::load(&c.dir.display().to_string()).unwrap();
    again.load_tickets();
    // the way back (proto-lead N-1446 p398, clause 1): a kind this release does not load is not loaded, its tickets'
    // opens and the inputs on them refused for that alone, each a problem line; nothing else
    let ours = |e: &String| e.contains("text-change is a kind this release does not load") || [t, u].iter().any(|n| e.ends_with(&format!("There is no ticket {n}.")));
    assert!(!again.tickets.errors.is_empty() && again.tickets.errors.iter().all(ours), "{:?}", again.tickets.errors);
    assert!(!again.tickets.m.defs.kinds.contains_key("text-change"));
    assert!(!again.tickets.m.tickets.contains_key(&t) && !again.tickets.m.tickets.contains_key(&u));
    // forward again on the base with the kind: both as they were, waiting for the read and for coord
    let mut forward = Core::load(&c.dir.display().to_string(), &agent_base());
    forward.project = crate::project::Project::load(&c.dir.display().to_string()).unwrap();
    forward.load_tickets();
    assert!(forward.tickets.errors.is_empty(), "{:?}", forward.tickets.errors);
    assert_eq!(forward.tickets.m.view(t), c.core.tickets.m.view(t), "kept, waiting for its read");
    assert_eq!(forward.tickets.m.view(u), c.core.tickets.m.view(u), "kept, waiting for coord");
    assert!(matches!(&forward.tickets.m.tickets[&u].status, Status::Awaits { agent, .. } if agent == COORD));
}

/// Research's own policy, copied to a scratch repository: an agent's change reaches no step of the owner's
/// (paper-research N-865 post 4, 1). Its policy folder as it was on 22 Nov 2026 (tests/research-text/policy, from
/// flavor/projects/research; research has no `text-read` rule of its own, and its `Act open` lets every agent open
/// a text-change), or TEXT_CHANGE_RESEARCH: its project folder as it is now, read only, its kinds and calls too.
/// The ticket's posts as one line each: who, the head, and its words without the files' texts and the read word.
fn thread(c: &Coord, t: u64) -> Vec<String> {
    let short = |v: &Value| -> String {
        let mut v = v.clone();
        if let Some(o) = v.as_object_mut() {
            o.remove("read");
            if let Some(fs) = o.get_mut("files").and_then(Value::as_array_mut) {
                for f in fs.iter_mut() {
                    *f = f["path"].clone();
                }
            }
        }
        v.to_string()
    };
    c.core.tickets.m.tickets[&t].posts.iter().map(|p| format!("{} {}: {}", c.core.tickets.m.id(t), p.by, p.head.as_ref().map(short).unwrap_or_default())).collect()
}

/// The example for the owner (paper-research, N-865 p4, 4): on research's policy, an agent adds a small kind, read once
/// by the lead first (the one other agent running; N-1446 p2884), and uses it; a change that does not read is refused
/// as he would see it, with nothing written; the Undo of a later change goes back byte for byte, and the Undo of the
/// kind once a ticket stands on it is refused. Each step printed as `EXAMPLE`.
#[test]
fn the_example_for_him_on_researchs_policy() {
    let from = std::env::var("TEXT_CHANGE_RESEARCH").map(PathBuf::from).unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tests/research-text"));
    let mut c = Coord::make("example", Make { from: Some(&from), lead: Some("paper-research"), ..Default::default() });
    let say = |l: String| println!("EXAMPLE {l}");
    let call = "// The question a summary answers.\ncall summary_of(thread: Text) -> Summary(text: Text)\n  task summary_of\n  template\n    Say in three lines what thread {{ thread }} decided.\n";
    let kind = "/// a thread's decisions in three lines\nkind summary\n  form thread: Text\n  result Text\n  pattern\n    s <- step dev summary_of(thread);\n    return s.text\n";
    let start = c.head();
    // 1. dev adds the kind and its call, and uses it
    let t = c.change("dev", &[("calls/summary.call", Some(call)), ("kinds/summary.kind", Some(kind))], "a summary of a thread's decisions");
    assert!(c.awaits_read(t), "{:?}", c.status(t));
    c.read(t, json!({"head": "Go"}));
    say(format!("dev opens text-change {}: files calls/summary.call, kinds/summary.kind; why \"a summary of a thread's decisions\"", c.core.tickets.m.id(t)));
    thread(&c, t).into_iter().for_each(&say);
    let r = result(&c.status(t));
    assert_eq!(r["head"], "Live", "{r}");
    let added = r["commit"].as_str().unwrap().to_string();
    say(format!("result {r}"));
    say(format!("git: {}", git(&c.repo, &["log", "-1", "--format=%h %an: %s%n%b"]).replace('\n', " | ")));
    let s = c.open_only("summary", "dev", json!({"thread": "N-12"}));
    say(format!("dev opens summary {} (thread N-12): {:?}", c.core.tickets.m.id(s), c.status(s)));
    c.core.ticket_input(Input::Answer { t: s, by: "dev".into(), head: json!({"head": "Summary", "text": "Ship on Friday. Tests first. The owner reads the plan."}), text: docket_machine::Text::New(String::new()) }).unwrap();
    say(format!("dev answers Summary: {} is {:?}", c.core.tickets.m.id(s), c.status(s)));
    assert!(matches!(c.status(s), Status::Done(_)));
    assert!(c.owner_steps().is_empty(), "{:?}", c.owner_steps());
    // 2. refused: a kind whose step names a call there is none of; nothing written, no commit
    let head = c.head();
    let bad = "kind tally\n  form q: Text\n  result Text\n  pattern\n    a <- step dev count_of(q);\n    return a.text\n";
    let t = c.change("dev", &[("kinds/tally.kind", Some(bad))], "a tally");
    say(format!("dev opens text-change {}: files kinds/tally.kind; why \"a tally\"", c.core.tickets.m.id(t)));
    thread(&c, t).into_iter().for_each(&say);
    let r = result(&c.status(t));
    say(format!("result {r}"));
    assert_eq!(r["head"], "Refused", "{r}");
    assert!(r["at"].as_array().is_some_and(|a| a.iter().any(|p| p.as_str().is_some_and(|p| p.ends_with("kinds/tally.kind:5")))), "{r}");
    assert!(!c.text.join("kinds/tally.kind").exists() && c.head() == head);
    say(format!("kinds/tally.kind written: {}; commit the same: {}", c.text.join("kinds/tally.kind").exists(), c.head() == head));
    // 3. the Undo: a later change to the call's words, taken back byte for byte
    let clearer = call.replace("Say in three lines", "In three short lines, say");
    let t = c.change("dev", &[("calls/summary.call", Some(&clearer))], "clearer words for the summary");
    c.read(t, json!({"head": "Go"}));
    let worded = result(&c.status(t))["commit"].as_str().unwrap().to_string();
    let b = c.open("dev", json!({"files": [], "back": worded, "why": "the first words read better"}));
    c.read(b, json!({"head": "Go"}));
    say(format!("dev opens text-change {}: back {}; why \"the first words read better\"", c.core.tickets.m.id(b), &worded[..8]));
    thread(&c, b).into_iter().for_each(&say);
    let r = result(&c.status(b));
    say(format!("result {r}"));
    assert_eq!(r["head"], "Live", "{r}");
    assert_eq!(std::fs::read_to_string(c.text.join("calls/summary.call")).unwrap(), call, "byte for byte");
    // the Undo of the kind itself, once summary S-… stands on it in the record: refused, the kind stays
    let b = c.open("dev", json!({"files": [], "back": added, "why": "no summaries after all"}));
    say(format!("dev opens text-change {}: back {}; why \"no summaries after all\"", c.core.tickets.m.id(b), &added[..8]));
    let r = result(&c.status(b));
    say(format!("result {r}"));
    assert_eq!(r["head"], "Refused", "{r}");
    assert!(c.core.tickets.m.defs.kinds.contains_key("summary"));
    say(format!("commits since the start: {}", git(&c.repo, &["log", "--format=%h %s", &format!("{start}..HEAD")]).replace('\n', " | ")));
}

#[test]
fn on_researchs_policy_an_agents_change_never_reaches_the_owner() {
    let from = std::env::var("TEXT_CHANGE_RESEARCH").map(PathBuf::from).unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tests/research-text"));
    // research says `own` owns the kind (its policy/text-change.eflint's OWNS, stated so)
    let owners = [("owners.eflint", "+owns(\"own\", \"kinds/tc-sum.kind\").\n")];
    let mut c = Coord::make("research", Make { from: Some(&from), lead: Some("paper-research"), others: &["ed", "fa", "own"], policy: &owners, ..Default::default() });
    let stop = |c: &mut Coord, a: &str| c.core.agents.get_mut(a).unwrap().live.pid = None;
    let call = "call tc_ask(q: Text) -> Said(text: Text)\n  task t\n  template\n    Q: {{ q }}\n";
    let kind = "kind tc-sum\n  form q: Text\n  result Text\n  pattern\n    a <- step dev tc_ask(q);\n    return a.text\n";
    // an agent's kind and its call: read once first (p2884), by the agent research says owns what it touches (p2888)
    let t = c.change("dev", &[("calls/tc-ask.call", Some(call)), ("kinds/tc-sum.kind", Some(kind))], "a small kind");
    assert_eq!(c.reader(t).as_deref(), Some("own"), "{:?}", c.status(t));
    c.read_as("own", t, json!({"head": "Go"}));
    assert_eq!(result(&c.status(t))["head"], "Live", "{:?}", c.status(t));
    // what nobody owns: any running agent but the lead, the first by name of those alike, never the owner
    let t = c.change("dev", &[("policy/tc-note.eflint", Some("Fact tc-note Identified by String\n"))], "a fact");
    assert_eq!(c.reader(t).as_deref(), Some("ed"), "{:?}", c.status(t));
    c.read_as("ed", t, json!({"head": "Go"}));
    assert_eq!(result(&c.status(t))["head"], "Live", "{:?}", c.status(t));
    // never the agent that made it
    let t = c.change("ed", &[("policy/tc-ed.eflint", Some("Fact tc-ed Identified by String\n"))], "ed's fact");
    assert_eq!(c.reader(t).as_deref(), Some("fa"), "{:?}", c.status(t));
    c.read_as("fa", t, json!({"head": "Go"}));
    assert_eq!(result(&c.status(t))["head"], "Live", "{:?}", c.status(t));
    // the lead only when no other agent runs
    for a in ["ed", "fa", "own"] {
        stop(&mut c, a);
    }
    let t = c.change("dev", &[("policy/tc-lead.eflint", Some("Fact tc-lead Identified by String\n"))], "a third fact");
    assert!(c.awaits_read(t), "{:?}", c.status(t));
    c.read(t, json!({"head": "Go"}));
    assert_eq!(result(&c.status(t))["head"], "Live", "{:?}", c.status(t));
    // nobody but the agent that made it running: refused, never the owner
    stop(&mut c, "paper-research");
    let t = c.change("dev", &[("policy/tc-other.eflint", Some("Fact tc-other Identified by String\n"))], "another");
    assert!(why(&c.status(t)).contains("nobody who may read it is running now"), "{:?}", c.status(t));
    assert!(c.owner_steps().is_empty(), "{:?}", c.owner_steps());
    println!("research's policy: 5 changes, read by own, ed, fa and the lead, one refused, no owner step");
}
