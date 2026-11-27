//! A commit given as a bare hash in a project of several repositories (paper-research N-101 p953, 4; runner.rs
//! `bare_refs`): found in exactly one repository, the Ref is that one's; in none or in several, the answer is not
//! read and the reminder says which. The repositories: a, and b cloned from it, each with a commit of its own after.

use crate::core::Core;
use docket_machine::{Input, Loc, Status, Text};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

struct Two {
    core: Core,
    dir: PathBuf,
    /// The commits: in both (the one b was cloned at), only in a, only in b.
    both: String,
    only_a: String,
}

impl Drop for Two {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let o = std::process::Command::new("git").args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "init.defaultBranch=main"]).args(args).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

fn commit(dir: &Path, file: &str) -> String {
    std::fs::write(dir.join(file), file).unwrap();
    git(dir, &["add", file]);
    git(dir, &["commit", "-q", "-m", file]);
    git(dir, &["rev-parse", "HEAD"])
}

fn two(name: &str) -> Two {
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let dir = std::env::temp_dir().join(format!("docket-bare-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (a, b) = (dir.join("a"), dir.join("b"));
    std::fs::create_dir_all(&a).unwrap();
    git(&a, &["init", "-q"]);
    let both = commit(&a, "one");
    git(&dir, &["clone", "-q", "a", "b"]);
    let only_a = commit(&a, "two");
    commit(&b, "three");
    let project = dir.join("files");
    for d in ["kinds", "calls"] {
        std::fs::create_dir_all(project.join(d)).unwrap();
    }
    std::fs::write(dir.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"dev\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
    std::fs::write(project.join("calls/t.call"), "call trace(w: Text) -> Part(commit: Ref) | Unclear(why: Text)\n  template\n    Trace {{ w }}.\n").unwrap();
    std::fs::write(project.join("kinds/t.kind"), "kind traced\n  letter R\n  form w: Text\n  result Ref?\n  pattern\n    r <- step dev trace(w);\n    match r {\n      Part(c) => return c\n      Unclear(w) => return none\n    }\n").unwrap();
    std::fs::write(project.join("project.toml"), "[project]\nname = \"two\"\nrepo = \"../a\"\nrepos = { a = \"../a\", b = \"../b\" }\n").unwrap();
    std::fs::write(dir.join("project"), project.join("project.toml").display().to_string()).unwrap();
    let mut core = Core::load(&dir.display().to_string(), &base);
    core.project = crate::project::Project::load(&dir.display().to_string()).unwrap();
    core.load_tickets();
    assert!(core.tickets.errors.is_empty(), "{:#?}", core.tickets.errors);
    Two { core, dir, both, only_a }
}

impl Two {
    /// The ticket opened, its step given to dev, and dev's answer Part with COMMIT: the reply, and the ticket.
    fn answer(&mut self, commit: &str) -> (Result<String, String>, u64) {
        self.core.ticket_input(Input::Open { kind: "traced".into(), under: None, opener: "owner".into(), form: json!({"w": "case-5"}), text: None, names: vec![], told: vec![] }).unwrap();
        let t = *self.core.tickets.m.tickets.keys().filter(|t| **t < docket_machine::HIDDEN).max().unwrap();
        for d in self.core.tickets.m.due("dev") {
            self.core.ticket_input(Input::Given { t: d.t, to: "dev".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Loc { file: "t.jsonl".into(), offset: 1, len: 1 }) }).unwrap();
        }
        let r = self.core.ticket_input(Input::Answer { t, by: "dev".into(), head: json!({"head": "Part", "commit": commit}), text: Text::New(format!("Part {commit}")) });
        (r, t)
    }
    fn refused(&self, t: u64) -> Option<String> {
        match &self.core.tickets.m.tickets[&t].status {
            Status::Awaits { refused: Some((why, _)), refusals: 1, .. } => Some(why.clone()),
            _ => None,
        }
    }
}

#[test]
fn a_bare_hash_found_in_one_repository_is_that_ones() {
    let mut c = two("one");
    let short = c.only_a[..7].to_string();
    let (r, t) = c.answer(&short);
    assert!(r.is_ok(), "{r:?}");
    let full = c.only_a.clone();
    assert!(matches!(&c.core.tickets.m.tickets[&t].status, Status::Done(v) if *v == json!({"a": full})), "{:?}", c.core.tickets.m.tickets[&t].status);
    // the record keeps the keyed commit
    let log = std::fs::read_to_string(c.dir.join("tickets/log.jsonl")).unwrap();
    assert!(log.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).any(|v| v["ev"] == "answer" && v["head"]["commit"] == json!({"a": c.only_a})), "{log}");
}

#[test]
fn a_bare_hash_in_no_repository_is_not_read() {
    let mut c = two("none");
    let (r, t) = c.answer("deadbee");
    assert!(r.as_ref().is_err_and(|e| e.contains("2 more")), "{r:?}");
    assert_eq!(c.refused(t).as_deref(), Some("commit is the commit deadbee, which is in none of the project's repositories (a, b); give it with its repository, as `a deadbee`"));
}

#[test]
fn a_bare_hash_in_two_repositories_is_not_read() {
    let mut c = two("several");
    let short = c.both[..7].to_string();
    let (r, t) = c.answer(&short);
    assert!(r.is_err(), "{r:?}");
    assert_eq!(c.refused(t), Some(format!("commit is the commit {short}, which is in a and b; say which repository it is of, as `a {short}`")));
}
