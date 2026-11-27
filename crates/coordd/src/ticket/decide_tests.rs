//! A decision that picks from a list it is given (N-101 p158-p162): whose-red and fix-or-out through coordd, each
//! answered by the project's policy on the facts coordd gives (`Machine::decide_facts`), its answer's fields filled by
//! the machine (`decided_fields`). The cases are those of plan/tovrin-program/policy-rules.md, sections 2 (cut down to
//! rule 1 and Unclear, the owner N-186) and 3, on this engine's ticket numbers.

use super::{run_task, Core, Task};
use crate::core::Who;
use docket_machine::{Input, Loc, Status};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// whose-red's rule 1 and its Unclear; fix-or-out's rules 1, 2 and 4, as policy files say them.
const WHOSE_RED: &str = r#"Decision whose-red
  Options Piece | Unclear
// rule 1: the build carries one piece (`not` left out): it is that piece
Only for whose-red When call.carries-count == 1:
  option == "Piece"
// otherwise Unclear: the program's fresh reader decides
Only for whose-red When call.carries-count != 1:
  option == "Unclear"
"#;

const FIX_OR_OUT: &str = r#"Decision fix-or-out
  Options Fix | Out
// rule 1: Fix, by default
Prefer for fix-or-out:
  option == "Fix" > option == "Out"
// rule 2: Out, when this piece has already been fixed twice in this build
Only for fix-or-out When call.earlier-Fix >= 2:
  option == "Out"
// rule 4: Out, after 180 minutes of mending, with other pieces in the build waiting on it
Only for fix-or-out When call.first-minutes > 180 and call.carries-count >= 2:
  option == "Out"
"#;

const CALLS: &str = "\
type Handed {
  commit: Ref
  top: Ref
  by: Agent
}

call implement(what: Text) -> Commit(c: Ref)
  template
    Implement {{ what }}.

call built_green(commit: Ref) -> Green(top: Ref) | Red(line: Ref, log: Text, carries: [Ticket])
  template
    Build {{ commit }} on the line.

decision whose-red(cut: Ref, log: Text, carries: [Ticket], not: [Ticket]) -> Piece(piece: Ticket, commit: Ref, why: Text) | Unclear
decision fix-or-out(piece: Ticket, why: Text, carries: [Ticket]) -> Fix | Out
";

const KINDS: &str = "\
kind piece
  letter P
  form what: Text
  result Handed
  pattern
    r <- step dev implement(what);
    match r { Commit(c) => return Handed(c, \"top1\", \"dev\") }

kind waiting
  letter W
  form commit: Ref
  result Ref
  pattern
    g <- step runner built_green(commit);
    match g {
      Green(top) => return top
      Red(line, log, carries) =>
        k <- open culprit(line, log, carries, []);
        match k {
          Piece(p, c, why) => return c
          Unclear => return line
        }
    }

kind culprit
  letter C
  form cut: Ref, log: Text, carries: [Ticket], not: [Ticket]
  result Piece(piece: Ticket, commit: Ref, why: Text) | Unclear
  pattern
    w <- decide whose-red(cut, log, carries, not);
    return w

kind second
  letter S
  form cut: Ref, log: Text, carries: [Ticket], first: Ticket, then: Ticket
  result Piece(piece: Ticket, commit: Ref, why: Text) | Unclear
  pattern
    w <- decide whose-red(cut, log, carries, [first] ++ [then]);
    return w

kind build
  letter B
  form piece: Ticket, carries: [Ticket]
  result Text
  pattern
    a <- open mend(piece, carries);
    b <- open mend(piece, carries);
    c <- open mend(piece, carries);
    return c

kind mend
  letter M
  form piece: Ticket, carries: [Ticket]
  result Text
  pattern
    d <- decide fix-or-out(piece, \"case red\", carries);
    match d {
      Fix => return \"fix\"
      Out => return \"out\"
    }
";

struct Coord {
    core: Core,
    rx: tokio::sync::mpsc::UnboundedReceiver<Task>,
    engine: crate::policy::Engine,
    dir: std::path::PathBuf,
}

impl Coord {
    fn new(tag: &str) -> Coord {
        Coord::with(tag, &[])
    }
    /// [`Coord::new`] with MORE of the project's files beside its own: each one's path under the project's folder and
    /// its text.
    fn with(tag: &str, more: &[(&str, &str)]) -> Coord {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let dir = std::env::temp_dir().join(format!("docket-decide-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let project = dir.join("files");
        for d in ["kinds", "calls", "policy"] {
            std::fs::create_dir_all(project.join(d)).unwrap();
        }
        let add = |n: &str, i: u32, k: &str| format!("{{\"ev\":\"add\",\"id\":\"AGT-00{i}\",\"name\":\"{n}\",\"parent\":\"owner\",\"kind\":\"{k}\",\"model\":\"m\",\"at\":\"x\"}}\n");
        std::fs::write(dir.join("registry.jsonl"), format!("{}{}", add("dev", 1, "claude"), add("runner", 2, "program"))).unwrap();
        std::fs::write(project.join("calls/v.call"), CALLS).unwrap();
        std::fs::write(project.join("kinds/v.kind"), KINDS).unwrap();
        std::fs::write(project.join("policy/whose-red.eflint"), WHOSE_RED).unwrap();
        std::fs::write(project.join("policy/fix-or-out.eflint"), FIX_OR_OUT).unwrap();
        for (f, text) in more {
            std::fs::write(project.join(f), text).unwrap();
        }
        std::fs::write(project.join("project.toml"), "[project]\nname = \"v\"\n").unwrap();
        std::fs::write(dir.join("project"), project.join("project.toml").display().to_string()).unwrap();
        let mut core = Core::load(&dir.display().to_string(), &base);
        core.project = crate::project::Project::load(&dir.display().to_string()).unwrap();
        core.load_tickets();
        assert!(core.tickets.m.defs.kinds.contains_key("culprit"), "{:?}", core.tickets.m.defs.kinds.keys().collect::<Vec<_>>());
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        core.tickets.tasks = Some(tx);
        // the policy is solved with the proxy's models: a stand-in that serves none (as tests_library.rs), never the real one
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
        Coord { core, rx, engine, dir }
    }
    fn settle(&mut self) {
        while let Ok(task) = self.rx.try_recv() {
            for i in run_task(&self.engine, task) {
                let _ = self.core.ticket_input(i);
            }
        }
    }
    /// KIND opened by the owner with FORM; its number.
    fn open(&mut self, kind: &str, form: Value) -> u64 {
        let before = self.core.tickets.m.tickets.keys().filter(|t| **t < docket_machine::HIDDEN).max().copied().unwrap_or(0);
        self.core.ticket_input(Input::Open { kind: kind.into(), under: None, opener: "owner".into(), form, text: None, names: vec![], told: vec![] }).unwrap();
        // the ticket opened, not one it opened in turn
        let t = *self.core.tickets.m.tickets.keys().find(|t| **t > before && **t < docket_machine::HIDDEN).unwrap();
        self.settle();
        t
    }
    /// AGENT's step on T, given and answered HEAD.
    fn answer(&mut self, agent: &str, t: u64, head: Value) {
        for d in self.core.tickets.m.due(agent) {
            self.core.ticket_input(Input::Given { t: d.t, to: agent.into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Loc { file: "t.jsonl".into(), offset: 1, len: 1 }) }).unwrap();
        }
        let id = self.core.tickets.m.id(t);
        let r = self.core.ticket_answer_request(&Who::Agent(agent.into()), &id, head, Some("done".into()));
        assert!(r.ok, "{}", r.out);
        self.settle();
    }
    /// A piece closed done, handed in as COMMIT: its id.
    fn piece(&mut self, commit: &str) -> String {
        let p = self.open("piece", json!({"what": "Packet.parse"}));
        self.answer("dev", p, json!({"head": "Commit", "c": commit}));
        assert!(matches!(&self.core.tickets.m.tickets[&p].status, Status::Done(v) if v["commit"] == commit), "{:?}", self.core.tickets.m.tickets[&p].status);
        self.core.tickets.m.id(p)
    }
    fn status(&self, t: u64) -> Status {
        self.core.tickets.m.tickets[&t].status.clone()
    }
    /// The decided lines of question `decide` in the record.
    fn decided(&self) -> Vec<Value> {
        std::fs::read_to_string(self.dir.join("tickets/log.jsonl")).unwrap().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["ev"] == "decided" && v["question"] == "decide").collect()
    }
}

impl Drop for Coord {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// whose-red, rule 1: a build that carries one piece names it, with the commit it handed in (its Handed) and the
/// rule's comment as its why; a build that carries three is Unclear (the fresh reader's); one whose only piece is named in `not` (its
/// agent answered NotMine) is Unclear too.
#[test]
fn whose_red_names_the_one_piece_a_build_carries_and_else_is_unclear() {
    let mut c = Coord::new("whose");
    let e12 = c.piece("a7e2");
    let t = c.open("culprit", json!({"cut": "c41f", "log": "src/Packet.agda:44: type mismatch", "carries": [e12], "not": []}));
    let Status::Done(v) = c.status(t) else { panic!("{:?} {:?}", c.status(t), c.decided()) };
    assert_eq!((&v["head"], &v["piece"], &v["commit"]), (&json!("Piece"), &json!(e12), &json!("a7e2")), "{v}");
    // the why is the rule's own comment; the rule's text stays in the record (paper-research N-101 p1091)
    assert_eq!(v["why"], json!("rule 1: the build carries one piece (`not` left out): it is that piece"), "{v}");
    let d = c.decided();
    assert_eq!(d.last().unwrap()["provenance"]["call"]["carries-count"], json!("1"), "{d:?}");
    assert!(d.last().unwrap()["provenance"].to_string().contains("call.carries-count == 1"), "{d:?}");
    // three pieces: Unclear
    let (e14, e15) = (c.piece("b301"), c.piece("c977"));
    let t = c.open("culprit", json!({"cut": "c41f", "log": "short-buffer.p4: expected dropped, got forwarded", "carries": [e12, e14, e15], "not": []}));
    assert!(matches!(c.status(t), Status::Done(v) if v == json!({"head": "Unclear"})), "{:?}", c.status(t));
    // the one piece is the one named before: Unclear, so the reader decides
    let t = c.open("culprit", json!({"cut": "c41f", "log": "src/Packet.agda:44: type mismatch", "carries": [e12], "not": [e12]}));
    assert!(matches!(c.status(t), Status::Done(v) if v == json!({"head": "Unclear"})), "{:?}", c.status(t));
    assert_eq!(c.decided().last().unwrap()["provenance"]["call"]["carries-count"], json!("0"));
    // `not` built up across rounds (`++`): two named before, of three carried, leave the third
    let t = c.open("second", json!({"cut": "c41f", "log": "short-buffer.p4", "carries": [e12, e14, e15], "first": e12, "then": e14}));
    assert!(matches!(c.status(t), Status::Done(v) if v["head"] == "Piece" && v["piece"] == json!(e15) && v["commit"] == "c977"), "{:?}", c.status(t));
    assert_eq!(c.decided().last().unwrap()["provenance"]["call"]["not"], json!(serde_json::to_string(&json!([e12, e14])).unwrap()));
    // of two carried, both named before: Unclear
    let t = c.open("second", json!({"cut": "c41f", "log": "short-buffer.p4", "carries": [e12, e14], "first": e12, "then": e14}));
    assert!(matches!(c.status(t), Status::Done(v) if v == json!({"head": "Unclear"})), "{:?}", c.status(t));
}

/// The case the picked ticket's open steps exist for: a piece still waiting in `built_green` (runner-contract.md h)
/// whose red names it alone: its commit is the one it gave `built_green`, as it has no result yet.
#[test]
fn a_piece_waiting_in_built_green_is_named_with_the_commit_it_gave() {
    let mut c = Coord::new("waiting");
    let w = c.open("waiting", json!({"commit": "d5e1"}));
    let id = c.core.tickets.m.id(w);
    c.answer("runner", w, json!({"head": "Red", "line": "f00d", "log": "case 3 red", "carries": [id]}));
    assert!(matches!(c.status(w), Status::Done(v) if v == "d5e1"), "{:?}", c.status(w));
}

/// fix-or-out, section 3's example: Fix, Fix again, then Out once the piece has been fixed twice in this build; rule
/// 4 on the minutes since the build's first red and the other pieces the build carries.
#[test]
fn fix_or_out_fixes_twice_then_takes_the_piece_out() {
    let mut c = Coord::new("fix");
    let e12 = c.piece("a7e2");
    let b = c.open("build", json!({"piece": e12, "carries": [e12]}));
    assert!(matches!(c.status(b), Status::Done(v) if v == "out"), "{:?}", c.status(b));
    let d = c.decided();
    let answers: Vec<&Value> = d.iter().map(|v| &v["answer"]).collect();
    assert_eq!(answers, vec![&json!("Fix"), &json!("Fix"), &json!("Out")], "{d:?}");
    let earlier: Vec<&Value> = d.iter().map(|v| &v["provenance"]["call"]["earlier-Fix"]).collect();
    assert_eq!(earlier, vec![&json!("0"), &json!("1"), &json!("2")]);
    // another build of the same piece starts again from none fixed
    let b2 = c.open("build", json!({"piece": e12, "carries": [e12]}));
    assert!(matches!(c.status(b2), Status::Done(v) if v == "out"));
    assert_eq!(c.decided()[3]["provenance"]["call"]["earlier-Fix"], json!("0"));
    // rule 4, on the facts as coordd would give them: the minutes, and the pieces the build carries
    let ask = |m: &str, w: &str| {
        let f: BTreeMap<String, String> = [("piece", e12.as_str()), ("why", "red"), ("earlier-Fix", "0"), ("earlier-Out", "0"), ("first-minutes", m), ("carries-count", w)].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        c.engine.decide("fix-or-out", f).unwrap().chosen
    };
    assert_eq!(ask("200", "2").as_deref(), Some("Out"));
    assert_eq!(ask("200", "1").as_deref(), Some("Fix"));
    assert_eq!(ask("90", "2").as_deref(), Some("Fix"));
}

/// Draft 2's `green` (proto-mind N-1446 p1502, research/policy/dev.eflint), its facts and rules word for word, and one
/// way more to `red-on`, a job that names a failing test, so the `why` of a commit that is not green names its failing
/// tests (p1502 (9)).
const GREEN: &str = r#"Fact job Identified by String * String
Fact job-package Identified by String * String
Fact job-rc Identified by String * Int
Fact job-passed Identified by String * Int
Fact job-failed Identified by String * Int
Fact job-failing Identified by String * String
Fact job-whole Identified by String
Fact tree-of Identified by String * String
Fact changes Identified by String * String
Fact names-item Identified by String * String
Fact package Identified by String
Fact uses Identified by String * String
Fact shown Identified by String * String
  Derived from shown(tree, package) When job(id, tree) and job-package(id, package) and job-rc(id, 0) and job-passed(id, n) and n > 0 and job-failed(id, 0)
Fact covered Identified by String * String
  Derived from covered(commit, package) When tree-of(commit, tree) and shown(tree, package)
  Derived from covered(commit, package) When tree-of(commit, tree) and uses(user, package) and shown(tree, user)
Fact unshown Identified by String * String
  Derived from unshown(commit, package) When changes(commit, package) and not covered(commit, package)
Fact red-on Identified by String
  Derived from red-on(commit) When tree-of(commit, tree) and job(id, tree) and job-failed(id, n) and n > 0
  Derived from red-on(commit) When tree-of(commit, tree) and job(id, tree) and job-failing(id, test)
Fact tested Identified by String
  Derived from tested(commit) When tree-of(commit, tree) and shown(tree, package)

Decision green
  Options Green | Red

// No commit it adds names what it is handed in for in an `Item:` trailer, so it is not this ask's.
Only for green When not names-item(call.commit, call.for):
  option == "Red"
// A package it changes has no green job on its own tree, nor a package that uses it, so it is not green.
Only for green When unshown(call.commit, _):
  option == "Red"
// A job on its own tree failed a test, so it is not green.
Only for green When red-on(call.commit):
  option == "Red"
// No job ran green on its own tree, so it is not green.
Only for green When not tested(call.commit):
  option == "Red"
// Every package it changes has a green job on its own tree, and none failed there, so it is green.
Prefer for green:
  option == "Green" > option == "Red"
"#;

const GREEN_CALL: &str = "decision green(commit: Ref, for: Text) -> Green(why: Text) | Red(why: Text)\n";

const GREEN_KIND: &str = "\
kind greenhead
  letter E
  form commit: Ref, for: Text
  result Green(why: Text) | Red(why: Text)
  pattern
    g <- decide green(commit, for);
    return g
";

/// The facts a decision on a commit is handed (proto-mind N-1446 p1502 (1); paper-research p1495, p1519, p1559), from
/// the kept graph as the side's round of git reads it: the head's tree, the package its change is in, its `Item:`, the
/// workspace's packages and their uses from its manifests at `main` (`crates/*` members, a `workspace = true`
/// dependency, a dev-dependency, a package in no workspace), and the runner's job on its tree as its lines say it,
/// whole. Draft 2's `green` on them: Green for the item the head names; Red for another, for a commit with no job on
/// its tree, and for a commit whose job failed a test; a commit the graph has not read is the engine's, as before.
/// Each answer's `why`, which draft 2 hands `again` and `read`, is its rule's comment and line with the facts that
/// made it hold (p1502 (9); paper-research p1617): Green's the trailer and the job that ran green, Red's the
/// trailer's absence, and the failing job with its failing test.
#[test]
fn a_decision_on_a_commit_is_handed_its_tree_changes_items_packages_and_jobs() {
    use docket_machine::graph_git::Packages;
    let mut c = Coord::with("green", &[("calls/green.call", GREEN_CALL), ("kinds/green.kind", GREEN_KIND), ("policy/green.eflint", GREEN)]);
    assert!(c.core.tickets.m.defs.kinds.contains_key("greenhead"));
    let repo = c.dir.join("repo");
    let git = |args: &[&str]| -> String {
        let o = std::process::Command::new("git").args(["-c", "core.hooksPath=/dev/null", "-c", "user.name=t", "-c", "user.email=t@t", "-c", "init.defaultBranch=main"]).args(args).current_dir(&repo).env("GIT_COMMITTER_DATE", "2026-11-22 10:00:00 +0000").env("GIT_AUTHOR_DATE", "2026-11-22 10:00:00 +0000").output().unwrap();
        assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };
    let files = [
        ("Cargo.toml", "[workspace]\nmembers = [\"crates/*\", \"tools/t\"]\n\n[workspace.dependencies]\nb = { path = \"crates/b\" }\n"),
        ("crates/a/Cargo.toml", "[package]\nname = \"a\"\n\n[dependencies]\nb.workspace = true\n"),
        ("crates/a/src/lib.rs", "pub fn a() {}\n"),
        ("crates/b/Cargo.toml", "[package]\nname = \"b\"\n"),
        ("crates/b/src/lib.rs", "pub fn b() {}\n"),
        ("tools/t/Cargo.toml", "[package]\nname = \"t\"\n\n[dev-dependencies]\na = { path = \"../../crates/a\" }\n"),
        ("lone/Cargo.toml", "[package]\nname = \"lone\"\n"),
    ];
    for (f, text) in files {
        std::fs::create_dir_all(repo.join(f).parent().unwrap()).unwrap();
        std::fs::write(repo.join(f), text).unwrap();
    }
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "base\n\nItem: I-1\nAgent: dev"]);
    git(&["checkout", "-q", "-b", "head"]);
    std::fs::write(repo.join("crates/b/src/lib.rs"), "pub fn b() {}\npub fn c() {}\n").unwrap();
    git(&["commit", "-q", "-am", "b: c\n\nItem: I-7\nAgent: dev"]);
    let (base, head, tree) = (git(&["rev-parse", "--short=8", "main"]), git(&["rev-parse", "--short=8", "head"]), git(&["rev-parse", "head^{tree}"]));
    let (h8, tr) = (head.as_str(), tree.as_str());
    // a commit on it whose job fails a test
    git(&["checkout", "-q", "-b", "head2"]);
    std::fs::write(repo.join("crates/a/src/lib.rs"), "pub fn a() {}\npub fn d() {}\n").unwrap();
    git(&["commit", "-q", "-am", "a: d\n\nItem: I-7\nAgent: dev"]);
    let (h9, t9) = (git(&["rev-parse", "--short=8", "head2"]), git(&["rev-parse", "head2^{tree}"]));
    // dev's edit in it, and its job on the head's tree, whole, as build-runner's lines say it
    let at = |f: &str| repo.join(f).display().to_string();
    let act = |is: &str, more: Value| Input::Act { agent: "dev".into(), is: is.into(), text: is.into(), more: more.as_object().unwrap().clone(), t: None };
    c.core.ticket_input(act("edit", json!({"tool": "Edit", "files": [at("crates/b/src/lib.rs")]}))).unwrap();
    let job = |phase: &str, end: Value| {
        let mut j = json!({"phase": phase, "id": "7201-1795341600", "wt": repo.display().to_string(), "branch": "head", "commit": &head, "verb": "test", "packages": "[\"a\"]", "since": "2026-11-22T11:00:00+0100", "tree": &tree, "tests": "[]", "whole": "1"});
        j.as_object_mut().unwrap().extend(end.as_object().unwrap().clone());
        act("job", j)
    };
    c.core.ticket_input(job("start", json!({}))).unwrap();
    c.core.ticket_input(job("end", json!({"rc": "0", "passed": "3", "failed": "0"}))).unwrap();
    let j9 = "7202-1795341700";
    let on9 = || json!({"id": j9, "branch": "head2", "commit": &h9, "tree": &t9}).as_object().unwrap().clone();
    c.core.ticket_input(job("start", Value::Object(on9()))).unwrap();
    let mut end9 = on9();
    end9.extend(json!({"rc": "101", "passed": "2", "failed": "1", "failing": "[\"a::tests::c\"]"}).as_object().unwrap().clone());
    c.core.ticket_input(job("end", Value::Object(end9))).unwrap();
    let home = c.dir.join("home").display().to_string();
    let round = c.core.graph_git_reading().read(&[], &home);
    c.core.graph_git_put(round, None);
    let dirs = [("crates/a", "a"), ("crates/b", "b"), ("lone", "lone"), ("tools/t", "t")].map(|(d, n)| (d.to_string(), n.to_string())).into();
    let members = ["a", "b", "t"].map(String::from).into();
    let uses = [("a", "b"), ("t", "a")].map(|(u, p)| (u.to_string(), p.to_string())).into();
    assert_eq!(c.core.tickets.graph_git.packages, [Packages { repo: "repo".into(), dirs, members, uses }], "read from its manifests at main");

    let fact = |f: &str, a: &[&str]| (f.to_string(), a.iter().map(|x| x.to_string()).collect::<Vec<_>>());
    let facts_of = |c: &Coord, commit: &str| c.core.decision_facts("green", &|p| [("commit", commit), ("for", "I-7")].iter().find(|(k, _)| *k == p).map(|(_, v)| v.to_string()));
    let (given, _) = facts_of(&c, h8).unwrap();
    let id = "7201-1795341600";
    let mut want = vec![
        fact("tree-of", &[h8, tr]),
        fact("changes", &[h8, "b"]),
        fact("names-item", &[h8, "I-7"]),
        fact("package", &["a"]),
        fact("package", &["b"]),
        fact("package", &["t"]),
        fact("uses", &["a", "b"]),
        fact("uses", &["t", "a"]),
        fact("job", &[id, tr]),
        fact("job-package", &[id, "a"]),
        fact("job-verb", &[id, "test"]),
        fact("job-whole", &[id]),
        fact("job-rc", &[id, "0"]),
        fact("job-passed", &[id, "3"]),
        fact("job-failed", &[id, "0"]),
    ];
    want.sort();
    assert_eq!(given, want, "the head adds its one commit over main's tip; a, which uses b, ran green on its tree");
    assert!(facts_of(&c, "c41f").is_none(), "too short to say a commit: the engine's");

    let head_of = |c: &mut Coord, commit: &str, item: &str| {
        let t = c.open("greenhead", json!({"commit": commit, "for": item}));
        let Status::Done(v) = c.status(t) else { panic!("{:?} {:?}", c.status(t), c.decided()) };
        (v["head"].as_str().unwrap_or_default().to_string(), v["why"].as_str().unwrap_or_default().to_string())
    };
    let green = |why: &str| ("Green".to_string(), why.to_string());
    let red = |why: &str| ("Red".to_string(), why.to_string());
    // each rule's place, its file's name and line
    let place = |rule: &str| format!("green.eflint:{}", GREEN.lines().position(|l| l == rule).unwrap() + 1);
    let ran = format!("tree-of(\"{h8}\", \"{tr}\"), job(\"{id}\", \"{tr}\"), job-package(\"{id}\", \"a\"), job-rc(\"{id}\", 0), job-passed(\"{id}\", 3), job-failed(\"{id}\", 0)");
    let item_rule = place("Only for green When not names-item(call.commit, call.for):");
    let green_why = format!(
        "Every package it changes has a green job on its own tree, and none failed there, so it is green ({}); no rule left it out, as names-item(\"{h8}\", \"I-7\") ({item_rule}) and tested(\"{h8}\"), from {ran} ({}).",
        place("Prefer for green:"),
        place("Only for green When not tested(call.commit):")
    );
    assert_eq!(head_of(&mut c, &head, "I-7"), green(&green_why), "the trailer, and the job that ran green on its tree");
    let d = c.decided();
    assert_eq!((&d.last().unwrap()["provenance"]["call"]["commit"], &d.last().unwrap()["provenance"]["decision"]), (&json!(h8), &json!("green")), "{d:?}");
    assert_eq!(head_of(&mut c, &head, "I-8"), red(&format!("No commit it adds names what it is handed in for in an `Item:` trailer, so it is not this ask's ({item_rule}): not names-item(\"{h8}\", \"I-8\").")));
    let (h, why) = head_of(&mut c, &base, "I-1");
    assert!(h == "Red" && why.contains("No job ran green on its own tree") && why.contains("has no green job on its own tree"), "{h} {why}");
    assert!(why.contains(&format!("({}): not tested(\"{base}\")", place("Only for green When not tested(call.commit):"))), "{why}");
    // the job that failed, and its failing test, as `again` is handed them
    let (h, why) = head_of(&mut c, &h9, "I-7");
    let failed = format!("tree-of(\"{h9}\", \"{t9}\"), job(\"{j9}\", \"{t9}\"), job-failed(\"{j9}\", 1), job-failing(\"{j9}\", \"a::tests::c\")");
    let red_on = place("Only for green When red-on(call.commit):");
    assert!(h == "Red" && why.contains(&format!("A job on its own tree failed a test, so it is not green ({red_on}): red-on(\"{h9}\"), from {failed}")), "{h} {why}");
    let d = c.decided();
    let grounds = d.last().unwrap()["provenance"]["grounds"].as_array().cloned().unwrap_or_default();
    assert!(grounds.iter().any(|g| g["at"] == json!(red_on) && g["held"] == json!(true) && g["from"].as_array().is_some_and(|f| f.contains(&json!(format!("job-failing(\"{j9}\", \"a::tests::c\")"))))), "the record keeps them: {grounds:?}");
    assert_eq!(head_of(&mut c, "c41f", "I-7").0, "Red", "no facts: the engine's answer, no item named");
}

/// The dev program's `cut-due` (proto-mind's draft 2, policy/dev.eflint, as paper-research narrowed it, N-1446 p1505),
/// cut down to where the heads stand: a cut goes when a head that hands in for one of his posts stands at the cut's
/// hold; an engine cut waits while another engine head is in its merge; one cut at a time. CHANGES, (1)'s, is stated
/// here as the policy's own: aaaa1111, cccc3333 and eeee5555 change the engine, dddd4444 the window alone. Beside it a
/// `when` rule on the fields alone, `cut-build`.
const CUT_DUE: &str = r#"Fact stands Identified by String * String
Fact stands-with Identified by String * String * String
Fact hands-in Identified by String * String * String
Fact changes Identified by String * String
+changes("aaaa1111", "coordd").
+changes("cccc3333", "coordd").
+changes("dddd4444", "yaaihh").
+changes("eeee5555", "docket-machine").
Fact his-stands Identified by String
  Derived from his-stands(head) When stands(head, "cut_of") and hands-in(head, post, "owner")
Fact head-at Identified by String * String
  Derived from head-at(head, commit) When stands-with(head, "commit", commit)
Fact engine-head Identified by String
  Derived from engine-head(head) When head-at(head, commit) and changes(commit, p) and p != "yaaihh"
Fact engine-stands Identified by String
  Derived from engine-stands(head) When stands(head, "cut_of") and engine-head(head)
Fact engine-merging Identified by String
  Derived from engine-merging(head) When stands(head, "merge") and engine-head(head)
Fact due-option Identified by String
+due-option("go").
+due-option("wait").
Decision cut-due
  Options due-option
// A head that answers an ask of his stands on the train, so a cut goes.
Prefer for cut-due When his-stands(_):
  option == "go" > option == "wait"
// No head of his stands, so no cut goes.
Prefer for cut-due:
  option == "wait" > option == "go"
// A standing head changes the engine, and another engine head is in its merge, so the cut waits for it.
Only for cut-due When engine-stands(_) and engine-merging(_):
  option == "wait"
// A cut is open, and one runs at a time.
Only for cut-due When call.open >= 1:
  option == "wait"
Fact build-option Identified by String
+build-option("build").
+build-option("wait").
Decision cut-build
  Options build-option
Prefer for cut-build:
  option == "wait" > option == "build"
Only for cut-build When call.open >= 1:
  option == "build"
"#;

/// dev.call's `merge` and `cut_of`, cut down.
const STAND_CALL: &str = "\
call merge(commit: Ref) -> Merged(merge: Ref)
  template
    Merge {{ commit }} onto the train's tip.

call cut_of(commit: Ref, merge: Ref) -> In(release: Text)
  template
    {{ commit }} stands on the train as {{ merge }}.
";

/// dev.kind's `head`, cut down: dev merges its commit, then holds it at the cut. The cut, which stands open at a hold
/// of its own; the two `when` bindings that open it.
const STAND_KIND: &str = "\
kind head
  letter H
  form commit: Ref, for: Text
  result Text
  pattern
    m <- step dev merge(commit);
    match m {
      Merged(onto) =>
        k <- hold dev cut_of(commit, onto);
        match k { In(release) => return release }
    }

kind cut
  letter U
  result ()
  pattern
    _ <- hold dev cut_of(\"ffff6666\", \"ffff6666\");
    return ()

when cut-due is go  open cut()
when cut-build is build  open cut()
";

/// (10) (proto-mind N-1446 p1502, p1615; paper-research p1505, p1616): a `when` binding's decision is handed where the
/// open tickets stand (`Machine::standing`) by `decision_facts`, beside (1)'s, and solved on them by `decide_in`, as the
/// clock asks it (`when_asks`, then those two). No head: wait. An agent's engine head at `cut_of` alone: wait. His
/// engine head in its merge, beside it: wait; at `cut_of`: go. His window head in its merge: go. Another engine head in
/// its merge: wait; once it stands: go. A cut open: wait. The fields-only `cut-build` answers by its fields as before,
/// though handed the same facts; a decision no `when` binding asks is handed none of them.
#[test]
fn a_when_decision_is_handed_where_the_open_tickets_stand() {
    use docket_machine::Text;
    let mut c = Coord::with("stand", &[("calls/stand.call", STAND_CALL), ("kinds/stand.kind", STAND_KIND), ("policy/dev.eflint", CUT_DUE)]);
    assert!(c.core.tickets.m.defs.kinds.contains_key("head") && c.core.tickets.m.defs.kinds.contains_key("cut"), "{:?}", c.core.tickets.errors);
    // his thread to dev, and dev's post on it
    c.core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "dev", "typed": ["dev"], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("the head for the stills?".into())), names: vec!["dev".into()], told: vec![] }).unwrap();
    let t = *c.core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
    let id = c.core.tickets.m.id(t);
    c.core.ticket_input(Input::Post { t, by: "dev".into(), text: Text::New("and one for the log".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let (his, devs) = (format!("{id}.p1"), format!("{id}.p{}", c.core.tickets.m.tickets[&t].last()));
    c.settle();
    let ask = |c: &Coord, d: &str| {
        let (_, d, _, f) = c.core.tickets.m.when_asks(&crate::core::now()).into_iter().find(|a| a.1 == d).unwrap();
        let situation = c.core.decision_facts(&d, &|p| f.get(p).cloned());
        let r = super::decide_in(&c.engine, &d, f, situation).unwrap();
        assert!(r.fault.is_none() && r.optimal.len() <= 1, "{r:?}");
        r.chosen.unwrap_or_default()
    };
    let head = |c: &mut Coord, commit: &str, for_: &str| {
        let h = c.open("head", json!({"commit": commit, "for": for_}));
        (h, c.core.tickets.m.id(h))
    };
    let merged = |c: &mut Coord, h: u64, onto: &str| c.answer("dev", h, json!({"head": "Merged", "merge": onto}));
    assert_eq!(ask(&c, "cut-due"), "wait", "no head stands");
    let (b, _) = head(&mut c, "cccc3333", &devs);
    merged(&mut c, b, "cccc0000");
    assert_eq!(ask(&c, "cut-due"), "wait", "an agent's head alone");
    let (a, ha) = head(&mut c, "aaaa1111", &his);
    assert_eq!(ask(&c, "cut-due"), "wait", "his head in its merge");
    merged(&mut c, a, "aaaa0000");
    let (given, _) = c.core.decision_facts("cut-due", &|_| None).expect("where the tickets stand");
    let fact = |f: &str, args: &[&str]| (f.to_string(), args.iter().map(|x| x.to_string()).collect::<Vec<_>>());
    for f in [fact("stands", &[&ha, "cut_of"]), fact("stands-with", &[&ha, "commit", "aaaa1111"]), fact("stands-with", &[&ha, "merge", "aaaa0000"]), fact("hands-in", &[&ha, &his, "owner"])] {
        assert!(given.contains(&f), "{f:?} in {given:?}");
    }
    assert_eq!(ask(&c, "cut-due"), "go", "his engine head stands, and the agent's beside it");
    head(&mut c, "dddd4444", &his);
    assert_eq!(ask(&c, "cut-due"), "go", "the head in its merge is window-only");
    let (e, _) = head(&mut c, "eeee5555", &devs);
    assert_eq!(ask(&c, "cut-due"), "wait", "an engine head in its merge");
    merged(&mut c, e, "eeee0000");
    assert_eq!(ask(&c, "cut-due"), "go", "it stands too: one cut");
    assert_eq!(ask(&c, "cut-build"), "wait", "the fields alone: no cut open");
    c.open("cut", json!({}));
    assert_eq!((ask(&c, "cut-due"), ask(&c, "cut-build")), ("wait".to_string(), "build".to_string()), "a cut is open");
    assert!(c.core.decision_facts("fix-or-out", &|_| None).is_none(), "no `when` binding asks it, and it names no commit");
}

/// (proto-mind N-1446 p1765, the dev program's stage 2) A `when` decision is handed, beside where the tickets stand,
/// (1)'s facts for each commit they stand with (a `Ref` argument of the step or hold they wait at), from the kept
/// graph as the side's round of git reads it: what draft 2's `cut-due` reads to tell an engine head from a window one,
/// with no `changes` of the policy's own. His engine head stands while another engine head is in its merge: wait; once
/// it stands too: go; a window head in its merge then keeps no cut waiting.
#[test]
fn a_when_decision_is_handed_what_the_commits_its_tickets_stand_with_change() {
    use docket_machine::Text;
    let policy: String = CUT_DUE.lines().filter(|l| !l.starts_with("+changes(")).map(|l| format!("{l}\n")).collect();
    let mut c = Coord::with("standc", &[("calls/stand.call", STAND_CALL), ("kinds/stand.kind", STAND_KIND), ("policy/dev.eflint", policy.as_str())]);
    assert!(c.core.tickets.m.defs.kinds.contains_key("head"), "{:?}", c.core.tickets.errors);
    // dev's repository: main, and three heads on it, each on its own branch: e1 and e2 change the engine's crate b,
    // w1 the window's crate yaaihh
    let repo = c.dir.join("repo");
    let git = |args: &[&str]| -> String {
        let o = std::process::Command::new("git").args(["-c", "core.hooksPath=/dev/null", "-c", "user.name=t", "-c", "user.email=t@t", "-c", "init.defaultBranch=main"]).args(args).current_dir(&repo).env("GIT_COMMITTER_DATE", "2026-11-22 10:00:00 +0000").env("GIT_AUTHOR_DATE", "2026-11-22 10:00:00 +0000").output().unwrap();
        assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };
    let files = [
        ("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n"),
        ("crates/b/Cargo.toml", "[package]\nname = \"b\"\n"),
        ("crates/b/src/lib.rs", "pub fn b() {}\n"),
        ("crates/yaaihh/Cargo.toml", "[package]\nname = \"yaaihh\"\n"),
        ("crates/yaaihh/src/lib.rs", "pub fn w() {}\n"),
    ];
    for (f, text) in files {
        std::fs::create_dir_all(repo.join(f).parent().unwrap()).unwrap();
        std::fs::write(repo.join(f), text).unwrap();
    }
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "base\n\nItem: I-1\nAgent: dev"]);
    let head_commit = |branch: &str, file: &str| {
        git(&["checkout", "-q", "-b", branch, "main"]);
        std::fs::write(repo.join(file), format!("pub fn {branch}() {{}}\n")).unwrap();
        git(&["commit", "-q", "-am", &format!("{branch}\n\nItem: I-7\nAgent: dev")]);
        git(&["rev-parse", "--short=8", branch])
    };
    let (e1, e2, w1) = (head_commit("e1", "crates/b/src/lib.rs"), head_commit("e2", "crates/b/src/lib.rs"), head_commit("w1", "crates/yaaihh/src/lib.rs"));
    let act = Input::Act { agent: "dev".into(), is: "edit".into(), text: "edit".into(), more: json!({"tool": "Edit", "files": [repo.join("crates/b/src/lib.rs").display().to_string()]}).as_object().unwrap().clone(), t: None };
    c.core.ticket_input(act).unwrap();
    let home = c.dir.join("home").display().to_string();
    let round = c.core.graph_git_reading().read(&[], &home);
    c.core.graph_git_put(round, None);
    // his thread to dev, and dev's post on it
    c.core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "dev", "typed": ["dev"], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("the heads?".into())), names: vec!["dev".into()], told: vec![] }).unwrap();
    let t = *c.core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
    let id = c.core.tickets.m.id(t);
    c.core.ticket_input(Input::Post { t, by: "dev".into(), text: Text::New("and mine".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let (his, devs) = (format!("{id}.p1"), format!("{id}.p{}", c.core.tickets.m.tickets[&t].last()));
    c.settle();
    let ask = |c: &Coord| {
        let (_, d, _, f) = c.core.tickets.m.when_asks(&crate::core::now()).into_iter().find(|a| a.1 == "cut-due").unwrap();
        let situation = c.core.decision_facts(&d, &|p| f.get(p).cloned());
        let r = super::decide_in(&c.engine, &d, f, situation).unwrap();
        assert!(r.fault.is_none() && r.optimal.len() <= 1, "{r:?}");
        r.chosen.unwrap_or_default()
    };
    let head = |c: &mut Coord, commit: &str, for_: &str| c.open("head", json!({"commit": commit, "for": for_}));
    let merged = |c: &mut Coord, h: u64, onto: &str| c.answer("dev", h, json!({"head": "Merged", "merge": onto}));
    let a = head(&mut c, &e1, &his);
    merged(&mut c, a, "aaaa0000");
    let b = head(&mut c, &e2, &devs);
    let (given, _) = c.core.decision_facts("cut-due", &|_| None).expect("where the tickets stand");
    let fact = |f: &str, args: &[&str]| (f.to_string(), args.iter().map(|x| x.to_string()).collect::<Vec<_>>());
    for f in [fact("changes", &[&e1, "b"]), fact("changes", &[&e2, "b"]), fact("stands-with", &[&c.core.tickets.m.id(a), "commit", &e1])] {
        assert!(given.contains(&f), "{f:?} in {given:?}");
    }
    assert!(!given.iter().any(|(f, a)| f == "changes" && a[0] == w1), "w1 stands nowhere yet: {given:?}");
    assert_eq!(ask(&c), "wait", "his engine head stands, and another engine head is in its merge");
    merged(&mut c, b, "bbbb0000");
    assert_eq!(ask(&c), "go", "both stand: one cut");
    head(&mut c, &w1, &devs);
    let (given, _) = c.core.decision_facts("cut-due", &|_| None).unwrap();
    assert!(given.contains(&fact("changes", &[&w1, "yaaihh"])), "{given:?}");
    assert_eq!(ask(&c), "go", "a window head in its merge keeps no cut waiting");
}

/// The engine page's move 6, its two pieces through coordd (ticket/effects.rs; paper-research N-1446 p3020, 8):
/// agent-base's `working` is made by the record's `turn_began` line (coordd's begin_turn writes it; proto-mind's copy
/// piece, p3020, 1) and ended by its `turn_cut`, and a program reads it with the `decide` it has, through a decision
/// whose rule reads it; the norms are given it as well.
#[test]
fn a_program_reads_with_decide_the_fact_the_records_own_lines_made_and_ended() {
    const WORKING_NOW: &str = "\
Decision working-now
  Options Yes | No
Prefer for working-now:
  option == \"No\" > option == \"Yes\"
// a turn running: the store's fact, made by the line its turn began with
Only for working-now When working(call.who):
  option == \"Yes\"
";
    const ASKS: &str = "\
kind asks
  letter A
  form who: Agent
  result Text
  pattern
    d <- decide working-now(who);
    match d {
      Yes => return \"working\"
      No => return \"not\"
    }
";
    let call = "decision working-now(who: Agent) -> Yes | No\n";
    let mut c = Coord::with("working", &[("policy/working-now.eflint", WORKING_NOW), ("calls/w.call", call), ("kinds/w.kind", ASKS)]);
    let working = |c: &Coord| c.core.norm_facts(&[]).contains(&("working".to_string(), vec!["dev".to_string()]));
    let t = c.open("asks", json!({"who": "dev"}));
    assert!(matches!(c.status(t), Status::Done(v) if v == "not"), "{:?} {:?}", c.status(t), c.decided());
    assert!(!working(&c));
    // dev is handed its step, and its turn begins on it: the `turn_began` line makes working(dev), the `given` line
    // does not
    c.open("piece", json!({"what": "Packet.parse"}));
    let mut first = None;
    for d in c.core.tickets.m.due("dev") {
        c.core.ticket_input(Input::Given { t: d.t, to: "dev".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Loc { file: "t.jsonl".into(), offset: 1, len: 1 }) }).unwrap();
        first.get_or_insert(d.id);
    }
    assert!(!working(&c), "given, its turn not yet begun");
    c.core.ticket_input(Input::Event { name: docket_machine::TURN_BEGAN.into(), value: json!({"agent": "dev", "delivery": first}) }).unwrap();
    assert!(working(&c));
    let t = c.open("asks", json!({"who": "dev"}));
    assert!(matches!(c.status(t), Status::Done(v) if v == "working"), "{:?} {:?}", c.status(t), c.decided());
    // its turn cut short: the `turn_cut` line ends it
    c.core.ticket_input(Input::TurnCut { agent: "dev".into() }).unwrap();
    assert!(!working(&c));
    let t = c.open("asks", json!({"who": "dev"}));
    assert!(matches!(c.status(t), Status::Done(v) if v == "not"), "{:?} {:?}", c.status(t), c.decided());
}

/// A role's step is handed, for each commit it is given (a `Ref` argument of its call), what a decision on that commit
/// is handed ((1), `Core::commits_facts`; paper-research N-1446 p2954), so `fills` rules on what the commit changes:
/// research's two cutters as its policy is to say them. A head whose commit changes the window's crate alone goes to
/// the window cutter; one that changes the engine's crate, and one on a commit the graph has not read, to the other.
#[test]
fn a_roles_step_is_handed_what_the_commit_it_is_given_changes() {
    const CALL: &str = "\
call merge(commit: Ref, for: Text) -> Merged(merge: Ref)
  template
    Merge {{ commit }} for {{ for }} onto the train's tip.
";
    const KIND: &str = "\
kind head
  letter H
  form commit: Ref, for: Text
  result ()
  pattern
    m <- step role cutter merge(commit, for);
    match m {
      Merged(onto) => return ()
    }
";
    const POLICY: &str = r#"Fact changes Identified by String * String
// A commit that changes a package besides the window's.
Fact engine-change Identified by String
  Derived from engine-change(commit) When changes(commit, package) and package != "yaaihh"
// A commit that changes the window's crate and nothing else.
Fact window-head Identified by String
  Derived from window-head(commit) When changes(commit, "yaaihh") and not engine-change(commit)
+role("dev", "cutter").
+role("runner", "cutter").
+role("runner", "window-cutter").
// The window cutter merges a window head.
Only for fills When call.role == "cutter" and window-head(call.commit):
  role(option, "window-cutter")
// The other cutter merges every other head.
Only for fills When call.role == "cutter" and not window-head(call.commit):
  not role(option, "window-cutter")
"#;
    let mut c = Coord::with("fillsc", &[("calls/cut.call", CALL), ("kinds/cut.kind", KIND), ("policy/cutters.eflint", POLICY)]);
    assert!(c.core.tickets.m.defs.kinds.contains_key("head"), "{:?}", c.core.tickets.errors);
    for a in ["dev", "runner"] {
        c.core.agents.get_mut(a).unwrap().live.pid = Some(1);
    }
    // dev's repository: main, and two heads on it: e1 changes the engine's crate b, w1 the window's crate yaaihh
    let repo = c.dir.join("repo");
    let git = |args: &[&str]| -> String {
        let o = std::process::Command::new("git").args(["-c", "core.hooksPath=/dev/null", "-c", "user.name=t", "-c", "user.email=t@t", "-c", "init.defaultBranch=main"]).args(args).current_dir(&repo).env("GIT_COMMITTER_DATE", "2026-11-24 20:00:00 +0000").env("GIT_AUTHOR_DATE", "2026-11-24 20:00:00 +0000").output().unwrap();
        assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };
    let files = [
        ("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n"),
        ("crates/b/Cargo.toml", "[package]\nname = \"b\"\n"),
        ("crates/b/src/lib.rs", "pub fn b() {}\n"),
        ("crates/yaaihh/Cargo.toml", "[package]\nname = \"yaaihh\"\n"),
        ("crates/yaaihh/src/lib.rs", "pub fn w() {}\n"),
    ];
    for (f, text) in files {
        std::fs::create_dir_all(repo.join(f).parent().unwrap()).unwrap();
        std::fs::write(repo.join(f), text).unwrap();
    }
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "base\n\nItem: I-1\nAgent: dev"]);
    let head_commit = |branch: &str, file: &str| {
        git(&["checkout", "-q", "-b", branch, "main"]);
        std::fs::write(repo.join(file), format!("pub fn {branch}() {{}}\n")).unwrap();
        git(&["commit", "-q", "-am", &format!("{branch}\n\nItem: I-7\nAgent: dev")]);
        git(&["rev-parse", "--short=8", branch])
    };
    let (e1, w1) = (head_commit("e1", "crates/b/src/lib.rs"), head_commit("w1", "crates/yaaihh/src/lib.rs"));
    let act = Input::Act { agent: "dev".into(), is: "edit".into(), text: "edit".into(), more: json!({"tool": "Edit", "files": [repo.join("crates/b/src/lib.rs").display().to_string()]}).as_object().unwrap().clone(), t: None };
    c.core.ticket_input(act).unwrap();
    let home = c.dir.join("home").display().to_string();
    let round = c.core.graph_git_reading().read(&[], &home);
    c.core.graph_git_put(round, None);
    // who `fills` gave each head's merge, by the record's decided line
    let filled = |c: &Coord, t: u64| -> Value {
        let log = std::fs::read_to_string(c.dir.join("tickets/log.jsonl")).unwrap();
        let line = log.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).find(|v| v["ev"] == "decided" && v["question"] == "fills" && v["t"] == json!(t));
        line.map(|v| v["answer"].clone()).unwrap_or(Value::Null)
    };
    let w = c.open("head", json!({"commit": w1, "for": "I-7"}));
    assert_eq!(filled(&c, w), json!("runner"), "the window head is the window cutter's");
    let e = c.open("head", json!({"commit": e1, "for": "I-7"}));
    assert_eq!(filled(&c, e), json!("dev"), "the engine head is the other cutter's");
    let u = c.open("head", json!({"commit": "abcd1234", "for": "I-7"}));
    assert_eq!(filled(&c, u), json!("dev"), "a commit the graph has not read changes nothing it knows of");
}

/// The runner's jobs open now, as the record's own job lines make and end them in agent-base's policy text
/// (open-jobs.eflint, `Event act` with each effect's `When` and a Terminates' `Foreach`; docket-edge5 N-1446 p3123,
/// p3127, H-1651; its `packages` a list, made the fact's JSON text, docket-edge6 p3126), read by a
/// project's pick of the next job as research's policy/jobs.eflint writes it (the scheduling of his N-1104 p1156):
/// the shortest waiting job by what the last of its verb and packages took; while a replay runs, the waiting replay
/// waits. An end replaces the last time of its kind, and ends the job's facts.
#[test]
fn a_pick_of_the_next_job_reads_the_jobs_the_records_lines_opened() {
    const JOBS: &str = "\
Fact waiting-job Identified by String
  Derived from waiting-job(j) When job-open(j, \"start\")
Fact running-replay Identified by String
  Derived from running-replay(j) When job-open(j, \"runs\") and job-kind(j, \"replay\", _)
Fact job-took Identified by String * Int
  Derived from job-took(j, s) When job-kind(j, v, p) and job-last(v, p, s)
Decision next-job
  Options waiting-job
// One replay at a time.
Only for next-job When running-replay(_):
  not job-kind(option, \"replay\", _)
// The shortest.
Prefer for next-job:
  minimise job-took(option)
";
    let mut c = Coord::with("jobsopen", &[("policy/jobs.eflint", JOBS)]);
    let line = |c: &mut Coord, phase: &str, id: &str, verb: &str, pkg: &str, secs: &str| {
        let mut more = json!({"phase": phase, "id": id, "verb": verb, "packages": [pkg]});
        if !secs.is_empty() {
            more["secs"] = json!(secs);
        }
        let act = Input::Act { agent: "dev".into(), is: "job".into(), text: format!("job {phase}"), more: more.as_object().unwrap().clone(), t: None };
        c.core.ticket_input(act).unwrap();
    };
    let pick = |c: &Coord| {
        let situation = c.core.decision_facts("next-job", &|_| None);
        super::decide_in(&c.engine, "next-job", BTreeMap::from([("runner".to_string(), "runner".to_string())]), situation).unwrap().chosen
    };
    let has = |c: &Coord, f: &str, args: &[&str]| c.core.norm_facts(&[]).contains(&(f.to_string(), args.iter().map(|x| x.to_string()).collect()));
    // the last replay of coordd took 30 s, the last test of docket-machine 600 s
    for (id, verb, pkg, secs) in [("1-1", "replay", "coordd", "30"), ("2-2", "test", "docket-machine", "600")] {
        line(&mut c, "start", id, verb, pkg, "");
        line(&mut c, "runs", id, verb, pkg, "");
        line(&mut c, "end", id, verb, pkg, secs);
    }
    assert!(!has(&c, "job-open", &["1-1", "runs"]) && !has(&c, "job-kind", &["1-1", "replay", "[\"coordd\"]"]), "an ended job is open no more");
    line(&mut c, "start", "3-3", "replay", "coordd", "");
    line(&mut c, "start", "4-4", "test", "docket-machine", "");
    assert!(has(&c, "job-open", &["3-3", "start"]) && has(&c, "job-by", &["3-3", "dev"]));
    assert_eq!(pick(&c).as_deref(), Some("3-3"), "the shortest");
    line(&mut c, "start", "5-5", "replay", "coordd", "");
    line(&mut c, "runs", "5-5", "replay", "coordd", "");
    assert!(has(&c, "job-open", &["5-5", "runs"]) && !has(&c, "job-open", &["5-5", "start"]), "running, waiting no more");
    assert_eq!(pick(&c).as_deref(), Some("4-4"), "a replay runs, so the waiting replay waits");
    line(&mut c, "end", "5-5", "replay", "coordd", "900");
    assert!(has(&c, "job-last", &["replay", "[\"coordd\"]", "900"]) && !has(&c, "job-last", &["replay", "[\"coordd\"]", "30"]), "the last time replaced");
    assert_eq!(pick(&c).as_deref(), Some("4-4"), "the replay now takes 900 s, the test 600 s");
    // a line of another kind, with the same fields, does nothing
    let other = Input::Act { agent: "dev".into(), is: "release".into(), text: "release".into(), more: json!({"phase": "start", "id": "6-6"}).as_object().unwrap().clone(), t: None };
    c.core.ticket_input(other).unwrap();
    assert!(!has(&c, "job-open", &["6-6", "start"]), "only a job's line opens a job");
}
