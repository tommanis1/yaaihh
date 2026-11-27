//! coordd's moves (N-101 p406, C), the note line (p410) and the commit check: what is decided, without sessions.

use super::{LINE, Move, note};
use crate::core::Core;
use docket_machine::{Input, Loc, Text};
use serde_json::json;
use std::process::Command;

const CALLS: &str = "\
call build(part: Text) -> Built(top: Ref)
  template
    Build {{ part }}.

call plan(part: Text) -> Planned(why: Text)
  template
    Plan {{ part }}.

call watch(part: Text) where model.class == large -> Planned(why: Text)
  template
    Watch {{ part }}.

call land(part: Text) -> OnLine(top: Ref) | Red(why: Text)
  template
    Land {{ part }}.
";

const KINDS: &str = "\
kind laid
  letter L
  form words: Text
  result Text
  pattern
    layer on \"line\" {
      b <- step bob build(words);
      return words
    }

kind handed
  letter H
  form words: Text
  result Text
  pattern
    layer on \"line\" {
      h <- step runner land(words);
      return words
    }

kind minded
  letter M
  form words: Text
  result Text
  pattern
    layer on \"line\" {
      let agent a in {
        p <- step a watch(words);
        q <- step a watch(words);
        return words
      }
    }

kind plain
  letter P
  form words: Text
  result Text
  pattern
    p <- step bob plan(words);
    return words
";

struct Coord {
    core: Core,
    dir: std::path::PathBuf,
    repo: String,
}

impl Drop for Coord {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn git(repo: &str, args: &[&str]) -> String {
    let o = Command::new("git").args(["-C", repo, "-c", "user.name=t", "-c", "user.email=t@x"]).args(args).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

impl Coord {
    fn new(tag: &str) -> Coord {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let dir = std::env::temp_dir().join(format!("docket-moves-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let project = dir.join("files");
        for d in ["kinds", "calls", "policy"] {
            std::fs::create_dir_all(project.join(d)).unwrap();
        }
        let repo = dir.join("repo").display().to_string();
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(format!("{repo}/a.txt"), "a\n").unwrap();
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-qm", "base"]);
        let add = |n: &str, i: u32| format!("{{\"ev\":\"add\",\"id\":\"AGT-00{i}\",\"name\":\"{n}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"opus\",\"at\":\"x\"}}\n");
        std::fs::write(dir.join("registry.jsonl"), format!("{}{}", add("bob", 1), add("ann", 2))).unwrap();
        std::fs::write(project.join("calls/m.call"), CALLS).unwrap();
        std::fs::write(project.join("kinds/m.kind"), KINDS).unwrap();
        std::fs::write(project.join("project.toml"), format!("[project]\nname = \"m\"\nrepo = \"{repo}\"\nroot = \"main\"\n")).unwrap();
        std::fs::write(dir.join("project"), project.join("project.toml").display().to_string()).unwrap();
        let mut core = Core::load(&dir.display().to_string(), &base);
        core.project = crate::project::Project::load(&dir.display().to_string()).unwrap();
        core.load_tickets();
        for n in ["bob", "ann"] {
            std::fs::create_dir_all(dir.join(format!("agents/{n}"))).unwrap();
            std::fs::write(dir.join(format!("agents/{n}/session.json")), "{}").unwrap();
            let a = core.agents.get_mut(n).unwrap();
            a.live.pid = Some(1);
            a.live.state = "idle".into();
        }
        Coord { core, dir, repo }
    }
    fn open(&mut self, kind: &str) -> u64 {
        let before: Vec<u64> = self.core.tickets.m.tickets.keys().copied().collect();
        self.core.ticket_input(Input::Open { kind: kind.into(), under: None, opener: "owner".into(), form: json!({"words": "a cache"}), text: None, names: vec![], told: vec![] }).unwrap();
        *self.core.tickets.m.tickets.keys().find(|t| **t < docket_machine::HIDDEN && !before.contains(t)).unwrap()
    }
    fn give(&mut self, agent: &str, t: u64) {
        let d = self.core.ticket_due(agent).into_iter().find(|d| d.t == t).unwrap_or_else(|| panic!("nothing due to {agent} on {t}"));
        self.core.ticket_input(Input::Given { t: d.t, to: agent.into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Loc { file: "t.jsonl".into(), offset: 1, len: 1 }) }).unwrap();
    }
    fn stand(&self, agent: &str, at: Option<&str>) {
        let f = self.dir.join(format!("agents/{agent}/layer"));
        match at {
            Some(l) => std::fs::write(f, format!("{l}\n")).unwrap(),
            None => {
                let _ = std::fs::remove_file(f);
            }
        }
    }
}

#[test]
fn an_agent_is_moved_to_its_steps_layer_once_it_has_no_open_work_where_it_stands() {
    let mut c = Coord::new("due");
    let t = c.open("laid");
    let l = c.core.tickets.m.layer_of(t).unwrap();
    let id = c.core.tickets.m.id(t);
    // in the project's own folder, owing nothing there: to the step's layer
    assert_eq!(c.core.move_due("bob"), Some(Move { agent: "bob".into(), to: l.clone(), t: Some(t), why: format!("given a step of {id} in it"), resume: false }));
    // standing in that layer: nothing to do
    c.stand("bob", Some(&l));
    assert_eq!(c.core.move_due("bob"), None);
    // in another layer, owing a step there too (a plain ticket's step is no layer's: open work in the folder only)
    c.stand("bob", Some("Q-9.l1"));
    assert_eq!(c.core.move_due("bob").map(|m| m.to), Some(l.clone()), "Q-9.l1 is no active layer, and no step is in it");
    // in the project's own folder with a step there (no layer): it stays, and is handed the layer's step with the note
    c.stand("bob", None);
    let p = c.open("plain");
    assert!(c.core.tickets.m.layer_of(p).is_none());
    assert_eq!(c.core.move_due("bob"), None, "open work where it stands");
    // in a turn: never moved
    c.core.ticket_input(Input::Cancel { t: p, by: "owner".into(), why: "x".into() }).unwrap();
    let a = c.core.agents.get_mut("bob").unwrap();
    a.live.state = "working".into();
    a.live.turn = 3;
    assert_eq!(c.core.move_due("bob"), None);
    let a = c.core.agents.get_mut("bob").unwrap();
    a.ended_turn = 3;
    assert!(c.core.move_due("bob").is_some(), "its turn has ended");
    // a move running: asked once
    let ms = c.core.to_move();
    assert_eq!(ms.iter().map(|m| m.agent.as_str()).collect::<Vec<_>>(), vec!["bob"]);
    assert!(c.core.to_move().is_empty());
    c.core.moving.clear();
    // not running, or with no session kept: not moved
    c.core.agents.get_mut("bob").unwrap().live.pid = None;
    assert_eq!(c.core.move_due("bob"), None);
    c.core.agents.get_mut("bob").unwrap().live.pid = Some(1);
    std::fs::remove_file(c.dir.join("agents/bob/session.json")).unwrap();
    assert_eq!(c.core.move_due("bob"), None);
}

#[test]
fn an_agent_in_a_layer_that_ended_goes_to_the_lines_view_and_one_that_ran_in_a_view_is_resumed() {
    let mut c = Coord::new("ended");
    // ann stands in a layer no ticket has open: out to the line's view
    c.stand("ann", Some("L-7.l1"));
    assert_eq!(c.core.move_due("ann"), Some(Move { agent: "ann".into(), to: LINE.into(), t: None, why: "layer L-7.l1 ended".into(), resume: false }));
    // in the line's view: stays
    c.stand("ann", Some("line@0123456789ab"));
    assert_eq!(c.core.move_due("ann"), None);
    // after a restart: into its layer while it is active, else the line's view
    let t = c.open("laid");
    let l = c.core.tickets.m.layer_of(t).unwrap();
    c.stand("ann", Some(&l));
    c.stand("bob", Some("L-7.l1"));
    for n in ["ann", "bob"] {
        c.core.agents.get_mut(n).unwrap().live.pid = None;
    }
    c.core.resuming = vec!["ann".into(), "bob".into()];
    let ms = c.core.to_move();
    assert_eq!(ms.len(), 2, "{ms:?}");
    assert_eq!((ms[0].to.as_str(), ms[0].resume), (l.as_str(), true));
    assert_eq!((ms[1].to.as_str(), ms[1].why.as_str()), (LINE, "coordd restarted; layer L-7.l1 is gone"));
    assert!(c.core.resuming.is_empty());
    // a move recorded: the agent's view shows its last one (docket-edge N-238), and the record replays to it
    assert_eq!(c.core.view("ann").moved, None);
    for (from, to, left) in [(None, Some("L-7.l1"), vec![]), (Some("L-7.l1"), Some(LINE), vec!["a.txt".to_string()])] {
        let input = docket_machine::Input::Moved { t: None, agent: "ann".into(), from: from.map(str::to_string), to: to.map(str::to_string), why: "layer L-7.l1 ended".into(), killed: vec![], left };
        c.core.ticket_input(input).unwrap();
    }
    let m = c.core.view("ann").moved.unwrap();
    assert_eq!((m.from.as_deref(), m.to.as_deref(), m.why.as_str(), m.left.clone()), (Some("L-7.l1"), Some(LINE), "layer L-7.l1 ended", vec!["a.txt".to_string()]));
    assert!(!m.at.is_empty());
    c.core.load_tickets();
    assert_eq!(c.core.view("ann").moved, Some(m));
    assert_eq!(c.core.view("bob").moved, None);
}

#[test]
fn the_note_line_says_where_the_step_is_and_that_coord_go_moves_the_agent_there() {
    // paper-research N-101 p410's words, with p414's last sentence (note.tmpl of the system's renderings)
    let places = crate::policy::Places { base: format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), ..Default::default() };
    let note = |l: &str, stand: Option<&str>, moved: bool| note(&places, l, stand, moved).unwrap();
    assert_eq!(note("P-8", Some("P-7"), true), "This step is in layer P-8; you stand in P-7. Your files here are P-7's. `coord go P-8` moves you there.");
    assert_eq!(note("P-8", None, true), "This step is in layer P-8; you stand in the project's own folder. Your files here are the project folder's. `coord go P-8` moves you there.");
    // a Codex agent is not moved: not said
    assert_eq!(note("P-8", Some("P-7"), false), "This step is in layer P-8; you stand in P-7. Your files here are P-7's.");
    assert_eq!(note("P-8", Some("line@a1b2"), true), "This step is in layer P-8; you stand in the line's read-only view (line@a1b2). Your files here are the line's, read-only. `coord go P-8` moves you there.");
}

fn turn_end(c: &mut Coord, stood: &str, go: Option<&str>) -> Result<String, String> {
    c.core.ticket_input(Input::TurnEnded { agent: "bob".into(), delivery: None, text: Loc { file: "t.jsonl".into(), offset: 5, len: 9 }, later: vec![], stood: Some(stood.into()), go: go.map(str::to_string) })
}

fn forming(c: &Coord, t: u64) -> bool {
    matches!(&c.core.tickets.m.tickets[&t].status, docket_machine::Status::Awaits { forming: Some(_), .. })
}

#[test]
fn a_turns_end_answers_only_a_step_where_the_agent_stood_and_a_turn_that_asks_to_go_answers_none() {
    let mut c = Coord::new("turn");
    let l = c.open("laid");
    let p = c.open("plain");
    let layer = c.core.tickets.m.layer_of(l).unwrap();
    c.give("bob", l);
    c.give("bob", p);
    // in the project's folder: L's step (the older) is not its turn's to answer; P's (in no layer) is
    turn_end(&mut c, "", None).unwrap();
    assert!(!forming(&c, l) && forming(&c, p));
    // an answer in the project's folder: no warning kept for the next delivery (p466, 2)
    assert!(!c.dir.join("agents/bob/answered").exists());
    // in the layer: its step is
    let mut c = Coord::new("turn2");
    let l = c.open("laid");
    let p = c.open("plain");
    c.give("bob", l);
    c.give("bob", p);
    // (through coordd's turn's end, which reads where it stood)
    c.stand("bob", Some(&layer));
    c.core.ticket_turn_ended("bob", Loc { file: "t.jsonl".into(), offset: 5, len: 9 });
    assert!(forming(&c, l) && !forming(&c, p));
    // an answer given by ending the turn in a layer: the layer is kept, so the next delivery's first line warns of
    // what it left uncommitted there (none here: the test's layer has no view, so nothing is said, and the file goes)
    assert_eq!(std::fs::read_to_string(c.dir.join("agents/bob/answered")).unwrap().trim(), layer);
    let coord = c.dir.display().to_string();
    let (said, file) = crate::moves::turn_warning(&c.core.places(), "bob").unwrap();
    assert!(said.is_empty() && file.ends_with("agents/bob/answered"), "{said}");
    // a move out of that layer read stray.txt there: the warning says it, though the layer's view is gone by then (an
    // answer that ends its ticket ends the layer: moves-try run 5); a move out of another layer changes nothing
    crate::moves::keep_left(&coord, "bob", "Z-9.l1", &["other.txt".to_string()]);
    crate::moves::keep_left(&coord, "bob", &layer, &["stray.txt".to_string()]);
    let (said, _) = crate::moves::turn_warning(&c.core.places(), "bob").unwrap();
    assert_eq!(said, format!("Warning: your last turn's end answered your step in layer {layer} and left changes uncommitted there: stray.txt. They stay in the layer's view, where nobody else reads them; commit them (or remove them) if they were part of that work."));
    // rendered again (a hand-over the session did not take is sent again): said again, until a turn given it ends
    assert_eq!(crate::moves::turn_warning(&c.core.places(), "bob").map(|(s, _)| s), Some(said));
    // (its next turn, in the project's folder, so that turn's own answer keeps no warning of its own)
    c.stand("bob", None);
    let p = c.open("plain");
    c.give("bob", p);
    c.core.ticket_turn_ended("bob", Loc { file: "t.jsonl".into(), offset: 50, len: 9 });
    assert!(crate::moves::turn_warning(&c.core.places(), "bob").is_none(), "said: the turn given it ended");
    // a turn that asked to go answers none: its last message is a post on the ticket it would have answered
    let mut c = Coord::new("turn3");
    let l = c.open("laid");
    let p = c.open("plain");
    c.give("bob", l);
    c.give("bob", p);
    let said = turn_end(&mut c, "", Some(&layer)).unwrap();
    assert!(!forming(&c, l) && !forming(&c, p), "{said}");
    let last = c.core.tickets.m.tickets[&p].posts.last().unwrap();
    assert!(last.by == "bob" && last.step.is_none(), "{said}");
    assert!(said.contains(&format!("goes to layer {layer}")), "{said}");
    // the record replays to the same
    let before: Vec<_> = [l, p].iter().map(|t| c.core.tickets.m.view(*t)).collect();
    c.core.load_tickets();
    assert_eq!([l, p].iter().map(|t| c.core.tickets.m.view(*t)).collect::<Vec<_>>(), before);
}

#[test]
fn coord_go_moves_the_agent_at_its_turns_end_to_a_layer_where_it_holds_a_step() {
    let mut c = Coord::new("go");
    let l = c.open("laid");
    let p = c.open("plain");
    let layer = c.core.tickets.m.layer_of(l).unwrap();
    let (lid, pid) = (c.core.tickets.m.id(l), c.core.tickets.m.id(p));
    c.give("bob", l);
    c.give("bob", p);
    let bob = crate::core::Who::Agent("bob".into());
    // the list
    let r = c.core.go(&bob, None);
    assert!(r.ok, "{}", r.out);
    assert!(r.out.starts_with("You stand in the project's own folder.") && r.out.contains(&format!("{lid} build: layer {layer} (coord go {layer})")) && r.out.contains(&format!("{pid} plan: no layer")), "{}", r.out);
    // no step there, or no such layer: refused, with the list
    let r = c.core.go(&crate::core::Who::Agent("ann".into()), Some(layer.clone()));
    assert!(!r.ok && r.out.contains("you hold no step in layer") && r.out.contains("You stand in"), "{}", r.out);
    let r = c.core.go(&bob, Some("Z-9.l1".into()));
    assert!(!r.ok && r.out.contains("no open layer"), "{}", r.out);
    // in a turn: kept for its end, said literally (the answer to post 363); by the ticket's id as well. Kept so even
    // when coordd reads the agent as idle (a turn a channel message began: moves-try run 5, where the move cut it)
    let a = c.core.agents.get_mut("bob").unwrap();
    a.live.state = "idle".into();
    a.live.turn = 4;
    let r = c.core.go(&bob, Some(lid.clone()));
    assert!(r.ok, "{}", r.out);
    assert_eq!(r.out, format!("Going to layer {layer} ({lid}, your step build). The move is made when this turn ends: end your turn now. Nothing more you do in this turn is in {layer}. Your work in the project's own folder stays there as you leave it, uncommitted edits included."));
    assert_eq!(super::going(&c.dir.display().to_string(), "bob", "go"), Some((layer.clone(), l)));
    assert_eq!(c.core.move_due("bob"), None, "in its turn");
    // its turn ends: the turn answers nothing (P's step stays unformed), and the move is due though it has open work
    // where it stands
    c.core.agents.get_mut("bob").unwrap().ended_turn = 4;
    assert_eq!(super::turn_went(&c.dir.display().to_string(), "bob"), Some(layer.clone()));
    turn_end(&mut c, "", Some(&layer)).unwrap();
    assert!(!forming(&c, p));
    assert_eq!(c.core.move_due("bob"), Some(Move { agent: "bob".into(), to: layer.clone(), t: Some(l), why: "coord go".into(), resume: false }));
    // what it is told once there
    let said = c.core.arrived("bob", &layer, "/p");
    assert_eq!(said, Some(format!("You are now in layer {layer} ({lid}), at /p. Your step here: build on {lid}, \"a cache\". Your other steps: no layer ({pid}, plan).")));
    // standing there: `coord go` to it again is refused
    c.stand("bob", Some(&layer));
    let r = c.core.go(&bob, Some(layer.clone()));
    assert!(!r.ok && r.out.contains("already"), "{}", r.out);
}

#[test]
fn an_answers_commit_not_on_its_steps_layer_branch_is_refused_and_the_third_fails_the_step() {
    let mut c = Coord::new("commit");
    let t = c.open("laid");
    let l = c.core.tickets.m.layer_of(t).unwrap();
    c.give("bob", t);
    let repo = c.repo.clone();
    let base = git(&repo, &["rev-parse", "HEAD"]);
    // the layer's branch, one commit on from the base; another commit beside it, on main
    git(&repo, &["checkout", "-q", "-b", &format!("layer/{l}")]);
    std::fs::write(format!("{repo}/a.txt"), "in the layer\n").unwrap();
    git(&repo, &["commit", "-qam", "in"]);
    let on = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["checkout", "-q", "main"]);
    std::fs::write(format!("{repo}/a.txt"), "outside\n").unwrap();
    git(&repo, &["commit", "-qam", "out"]);
    let off = git(&repo, &["rev-parse", "HEAD"]);
    let answer = |c: &mut Coord, sha: &str| c.core.ticket_input(Input::Answer { t, by: "bob".into(), head: json!({"head": "Built", "top": sha}), text: Text::New(String::new()) });
    // off the branch: refused, recorded as unfit, two more and it fails
    let e = answer(&mut c, &off).unwrap_err();
    assert!(e.contains(&format!("is not on layer/{l}")) && e.contains("2 more"), "{e}");
    let log = std::fs::read_to_string(c.dir.join("tickets/log.jsonl")).unwrap();
    assert!(log.lines().any(|x| x.contains("\"ev\":\"unfit\"") && x.contains(&off)), "{log}");
    // not a commit at all: refused the same way
    let e = answer(&mut c, "0000000").unwrap_err();
    assert!(e.contains("is no commit of"), "{e}");
    // the base and a commit on the branch fit
    answer(&mut c, &on).unwrap();
    assert!(!c.core.tickets.m.tickets[&t].is_open() || !matches!(c.core.tickets.m.tickets[&t].status, docket_machine::Status::Awaits { .. }));
    let _ = base;
    // a third refusal fails the step as Malformed
    let mut c = Coord::new("commit3");
    let t = c.open("laid");
    let l = c.core.tickets.m.layer_of(t).unwrap();
    c.give("bob", t);
    let repo = c.repo.clone();
    git(&repo, &["branch", &format!("layer/{l}")]);
    std::fs::write(format!("{repo}/a.txt"), "outside\n").unwrap();
    git(&repo, &["commit", "-qam", "out"]);
    let off = git(&repo, &["rev-parse", "HEAD"]);
    for _ in 0..2 {
        answer2(&mut c, t, &off).unwrap_err();
    }
    let e = answer2(&mut c, t, &off).unwrap_err();
    assert!(e.contains("the step failed as Malformed"), "{e}");
}

/// A step the program holds in a layer (the runner's hand-in) answers the line's new head, which is not on the layer's
/// branch: it is taken, not refused, and not completed with the layer's heads (docket-core3, N-101: run 1 on bf7c551).
#[test]
fn the_runners_answer_in_a_layer_is_the_lines_and_is_taken() {
    let mut c = Coord::new("runner");
    let t = c.open("handed");
    let l = c.core.tickets.m.layer_of(t).unwrap();
    assert!(matches!(&c.core.tickets.m.tickets[&t].status, docket_machine::Status::Awaits { agent, .. } if agent == "runner"), "{:?}", c.core.tickets.m.tickets[&t].status);
    let repo = c.repo.clone();
    // the layer's branch moved on from the base; the line's new head is another commit, on main
    git(&repo, &["checkout", "-q", "-b", &format!("layer/{l}")]);
    std::fs::write(format!("{repo}/a.txt"), "in the layer\n").unwrap();
    git(&repo, &["commit", "-qam", "in"]);
    git(&repo, &["checkout", "-q", "main"]);
    std::fs::write(format!("{repo}/a.txt"), "landed\n").unwrap();
    git(&repo, &["commit", "-qam", "runner hand-in"]);
    let top = git(&repo, &["rev-parse", "HEAD"]);
    // a bob of the same step would be refused: the check stands for agents
    assert!(super::on_branch(&repo, &top, &format!("layer/{l}")).is_err());
    c.core.ticket_input(Input::Answer { t, by: "runner".into(), head: json!({"head": "OnLine", "top": top}), text: Text::New(String::new()) }).unwrap();
    let log = std::fs::read_to_string(c.dir.join("tickets/log.jsonl")).unwrap();
    assert!(!log.lines().any(|x| x.contains("\"ev\":\"unfit\"")), "{log}");
    assert!(log.lines().any(|x| x.contains("\"ev\":\"answer\"") && x.contains(&top) && !x.contains("\"a\":")), "the line's head as answered, nothing of the layer added: {log}");
    assert!(!matches!(c.core.tickets.m.tickets[&t].status, docket_machine::Status::Awaits { .. }), "{:?}", c.core.tickets.m.tickets[&t].status);
}

fn answer2(c: &mut Coord, t: u64, sha: &str) -> Result<String, String> {
    c.core.ticket_input(Input::Answer { t, by: "bob".into(), head: json!({"head": "Built", "top": sha}), text: Text::New(String::new()) })
}

#[test]
fn an_answer_in_a_layer_takes_every_repository_whose_layer_branch_moved_with_its_head() {
    // W1 (paper-research N-101 p466): a project of two repositories, `a` (its own) and `b`
    let mut c = Coord::new("w1");
    let a = c.repo.clone();
    let b = c.dir.join("b").display().to_string();
    std::fs::create_dir_all(&b).unwrap();
    git(&b, &["init", "-q", "-b", "main"]);
    std::fs::write(format!("{b}/b.txt"), "b\n").unwrap();
    git(&b, &["add", "-A"]);
    git(&b, &["commit", "-qm", "base b"]);
    let toml = c.dir.join("files/project.toml");
    std::fs::write(&toml, format!("[project]\nname = \"m\"\nrepo = \"a\"\nroot = \"main\"\nrepos = {{ a = \"{a}\", b = \"{b}\" }}\n")).unwrap();
    c.core.project = crate::project::Project::load(&c.dir.display().to_string()).unwrap();
    let answer = |c: &mut Coord, t: u64, top: serde_json::Value| c.core.ticket_input(Input::Answer { t, by: "bob".into(), head: json!({"head": "Built", "top": top}), text: Text::New(String::new()) });
    let recorded = |c: &Coord, t: u64| -> serde_json::Value {
        let log = std::fs::read_to_string(c.dir.join("tickets/log.jsonl")).unwrap();
        let l = log.lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()).filter(|v| v["ev"] == "answer" && v["t"] == json!(t)).last().unwrap();
        l["head"]["top"].clone()
    };
    // the layer as layer.rs registers it: its base and branch in each repository
    let register = |c: &Coord, l: &str| {
        let (ba, bb) = (git(&a, &["rev-parse", "main"]), git(&b, &["rev-parse", "main"]));
        let branch = format!("layer/{l}");
        git(&a, &["branch", &branch, &ba]);
        git(&b, &["branch", &branch, &bb]);
        let reg = json!({ l: {"id": l, "ticket": "L-1", "repo": a, "base": ba, "branch": branch, "state": "open", "up": "", "outputs": [], "at": "x",
            "also": [{"name": "b", "repo": b, "base": bb, "branch": branch}]} });
        std::fs::create_dir_all(c.dir.join("layers")).unwrap();
        std::fs::write(c.dir.join("layers/layers.json"), reg.to_string()).unwrap();
        branch
    };
    let commit_on = |repo: &str, branch: &str, file: &str| {
        git(repo, &["checkout", "-q", branch]);
        std::fs::write(format!("{repo}/{file}"), format!("{branch}\n")).unwrap();
        git(repo, &["add", "-A"]);
        git(repo, &["commit", "-qm", file]);
        let sha = git(repo, &["rev-parse", "HEAD"]);
        git(repo, &["checkout", "-q", "main"]);
        sha
    };
    // both branches moved: the answer's bare commit of `a` takes `b`'s head with it
    let t = c.open("laid");
    let l = c.core.tickets.m.layer_of(t).unwrap();
    c.give("bob", t);
    let branch = register(&c, &l);
    let sa = commit_on(&a, &branch, "x.txt");
    let sb = commit_on(&b, &branch, "y.txt");
    answer(&mut c, t, json!(sa)).unwrap();
    assert_eq!(recorded(&c, t), json!({"a": sa, "b": sb}));
    // only `a`'s moved: nothing goes in beside it; the bare commit, found in `a` alone, is keyed as `a`'s (runner.rs
    // `bare_refs`, paper-research N-101 p953)
    let t = c.open("laid");
    let l = c.core.tickets.m.layer_of(t).unwrap();
    c.give("bob", t);
    let branch = register(&c, &l);
    let sa = commit_on(&a, &branch, "z.txt");
    answer(&mut c, t, json!(sa)).unwrap();
    assert_eq!(recorded(&c, t), json!({"a": sa}));
    // `b`'s moved and the answer names `b` itself: what it names is kept, `a`'s head goes in beside it
    let t = c.open("laid");
    let l = c.core.tickets.m.layer_of(t).unwrap();
    c.give("bob", t);
    let branch = register(&c, &l);
    let sa = commit_on(&a, &branch, "p.txt");
    let sb1 = commit_on(&b, &branch, "q.txt");
    let _sb2 = commit_on(&b, &branch, "r.txt");
    answer(&mut c, t, json!({"b": sb1})).unwrap();
    assert_eq!(recorded(&c, t), json!({"a": sa, "b": sb1}));
}

/// A restart on its conversation (every move): the new process's first turn ends as a turn of its own, though its
/// number is its last process's last turn's (docket-try3's boxed Codex try: B-2's answer never formed, N-243).
#[test]
fn a_moved_agents_first_turn_in_its_new_process_ends_as_its_own() {
    let mut c = Coord::new("newproc");
    c.core.agents.get_mut("bob").unwrap().live.pid = Some(std::process::id() as i32);
    c.core.begin_turn("bob");
    assert!(c.core.end_turn("bob").is_some());
    assert!(c.core.end_turn("bob").is_none(), "one turn end taken once");
    // moved: its new process, its turns from 0 again
    c.core.new_process("bob", std::process::id() as i32, false);
    c.core.begin_turn("bob");
    assert_eq!(c.core.agents["bob"].live.turn, 1);
    assert!(c.core.end_turn("bob").is_some(), "the new process's turn 1 is not its last process's");
}

/// H-86 on docket-try2 (paper-research N-101 p629): a step given, then left by `coord go` (the go-turn answers
/// nothing), comes back to its agent moved into its layer again as the step itself (`sN.back`), not as a notice that
/// only names it. Given once; that delivery is what its turn's end answers, and what forming reads the step from.
#[test]
fn a_step_left_by_coord_go_is_handed_again_once_its_agent_is_back_in_its_layer() {
    let mut c = Coord::new("back");
    let t = c.open("laid");
    let l = c.core.tickets.m.layer_of(t).unwrap();
    let lid = c.core.tickets.m.id(t);
    c.stand("bob", Some(&l));
    c.give("bob", t);
    let back = format!("{lid}.s1.back");
    let due = |c: &Coord| c.core.ticket_due("bob").into_iter().map(|d| d.id).collect::<Vec<_>>();
    let moved = |c: &mut Coord, to: &str, why: &str| c.core.ticket_input(Input::Moved { t: Some(t), agent: "bob".into(), from: None, to: Some(to.into()), why: why.into(), killed: vec![], left: vec![] }).unwrap();
    // away (coord go to another layer): nothing of it is due
    moved(&mut c, "Z-9.l1", "coord go");
    assert!(!due(&c).contains(&back), "{:?}", due(&c));
    // back in its layer: the step again, as a delivery of its own
    moved(&mut c, &l, &format!("given a step of {lid} in it"));
    assert!(due(&c).contains(&back), "{:?}", due(&c));
    let d = c.core.ticket_due("bob").into_iter().find(|d| d.id == back).unwrap();
    assert!(matches!(d.what, docket_machine::What::Step { again: false, .. }), "the step itself, not a reminder");
    let place = Loc { file: "t.jsonl".into(), offset: 40, len: 7 };
    c.core.ticket_input(Input::Given { t, to: "bob".into(), delivery: back.clone(), upto: d.upto, skipped: Default::default(), place: Some(place.clone()) }).unwrap();
    assert!(!due(&c).contains(&back), "given once");
    // what forming reads the step from: that delivery, not the first one
    assert_eq!(crate::ticket::shown_place(&c.core.tickets.m, t, 1, "bob"), Some(place));
    // its turn's end answers the step
    c.core.ticket_input(Input::TurnEnded { agent: "bob".into(), delivery: Some(back.clone()), text: Loc { file: "t.jsonl".into(), offset: 50, len: 9 }, later: vec![], stood: Some(l.clone()), go: None }).unwrap();
    assert!(forming(&c, t));
    // the record replays to the same: not due again
    c.core.load_tickets();
    assert!(!due(&c).contains(&back), "{:?}", due(&c));
    assert!(forming(&c, t));
}

#[test]
fn an_agent_in_an_older_lines_view_moves_to_the_lines_view_as_it_is_now() {
    let mut c = Coord::new("line-moved");
    let spec = c.core.line_spec().unwrap();
    let first = crate::layer::line_now(&spec).unwrap();
    assert!(crate::layer::is_line(&first), "{first}");
    // ann stands in the line's view of the line as it is: stays
    c.stand("ann", Some(&first));
    c.core.line_now = Some(first.clone());
    assert_eq!(c.core.move_due("ann"), None);
    // a part lands: the line moves, and her view is of the older line
    std::fs::write(format!("{}/b.txt", c.repo), "b\n").unwrap();
    git(&c.repo, &["add", "-A"]);
    git(&c.repo, &["commit", "-qm", "a part landed"]);
    let now = crate::layer::line_now(&spec).unwrap();
    assert_ne!(now, first);
    // the line not read yet (the follow loop's first read, or a read that failed): nobody is moved for it
    c.core.line_now = None;
    assert_eq!(c.core.move_due("ann"), None);
    c.core.line_now = Some(now.clone());
    assert_eq!(c.core.move_due("ann"), Some(Move { agent: "ann".into(), to: LINE.into(), t: None, why: "the line moved".into(), resume: false }));
    // in a turn: not moved; once its turn has ended, moved
    let a = c.core.agents.get_mut("ann").unwrap();
    a.live.state = "working".into();
    a.live.turn = 4;
    assert_eq!(c.core.move_due("ann"), None, "never in a turn");
    c.core.agents.get_mut("ann").unwrap().ended_turn = 4;
    assert_eq!(c.core.move_due("ann").map(|m| m.why), Some("the line moved".to_string()));
    // owing a step in no layer (bob's on a plain ticket) while in the older line's view: moved all the same, so the
    // step is handed in the line's view as it is now (a move runs before anything is handed: main.rs `follow`)
    let p = c.open("plain");
    assert!(c.core.tickets.m.layer_of(p).is_none() && !c.core.tickets.m.owes("bob").is_empty());
    c.stand("bob", Some(&first));
    assert_eq!(c.core.move_due("bob").map(|m| (m.to, m.why)), Some((LINE.to_string(), "the line moved".to_string())));
    c.core.ticket_input(Input::Cancel { t: p, by: "owner".into(), why: "x".into() }).unwrap();
    // owing a step in an active layer: to that layer, not to the line's view
    let t = c.open("laid");
    let l = c.core.tickets.m.layer_of(t).unwrap();
    c.stand("bob", Some(&first));
    assert_eq!(c.core.move_due("bob").map(|m| m.to), Some(l));
    // in the line's view as it is now: stays
    c.stand("ann", Some(&now));
    assert_eq!(c.core.move_due("ann"), None);
}

/// N-101 p1778: a piece's minder rests in its block between its steps; when its session went down (a reboot) before
/// its next step, coordd starts it again for that step in the step's layer, once, and `coord start` puts it there too.
#[test]
fn a_step_to_a_minder_resting_when_coordd_went_down_starts_it_again_in_the_steps_layer() {
    let mut c = Coord::new("minder");
    let t = c.open("minded");
    let l = c.core.tickets.m.layer_of(t).unwrap();
    let f = format!("f{t}-1");
    // the machine's own start of the new agent is marked: not started a second time while it comes up
    assert!(c.core.started_owing.contains_key(&f), "{:?}", c.core.started_owing);
    assert!(c.core.add(&crate::core::Who::Owner, &f, "claude", "opus").ok);
    std::fs::create_dir_all(c.dir.join(format!("agents/{f}"))).unwrap();
    std::fs::write(c.dir.join(format!("agents/{f}/session.json")), "{}").unwrap();
    c.stand(&f, Some(&l));
    assert!(c.core.to_start_owing().is_empty(), "started a moment ago");
    // it ran and answered step 1; its session then went down with the machine, resting in its block
    let a = c.core.agents.get_mut(&f).unwrap();
    a.live.pid = Some(1);
    a.live.state = "idle".into();
    assert_eq!(c.core.tickets.m.owes(&f).len(), 1, "{:?}", c.core.tickets.m.view(t).map(|v| (v.status, v.posts.len())));
    assert!(!c.core.ticket_due(&f).is_empty(), "{:?}", c.core.tickets.m.view(t).map(|v| v.status));
    c.give(&f, t);
    // its model is not what this test is about: the call's `where` is not checked against it (`refuse_unmet`)
    c.core.agents.get_mut(&f).unwrap().kind = "program".into();
    c.core.ticket_input(Input::Answer { t, by: f.clone(), head: json!({"head": "Planned", "why": "first"}), text: Text::New("first".into()) }).unwrap();
    c.core.agents.get_mut(&f).unwrap().kind = "claude".into();
    assert!(c.core.to_start_owing().is_empty(), "running");
    // the machine's start mark stands its time, running or not: its age is what lets a later start through
    c.core.started_owing.clear();
    let a = c.core.agents.get_mut(&f).unwrap();
    a.live.pid = None;
    a.live.state = "stopped".into();
    assert!(!c.core.tickets.m.ended(&f));
    assert_eq!(c.core.tickets.m.owes(&f).len(), 1, "step 2 is owed to the minder: {:?}", c.core.tickets.m.view(t).map(|v| (v.status, v.posts.iter().map(|p| p.by.clone()).collect::<Vec<_>>())));
    // the step it owes, in its layer: started again for it, once
    assert_eq!(c.core.to_start_owing(), vec![(f.clone(), t, "watch".to_string(), Some(l.clone()))]);
    assert!(c.core.to_start_owing().is_empty(), "once");
    // it came up and went down again while starting (a start ends the session running): not started again for the
    // same step, which would end the one coming up (Tovrin-dev 11:12Z, f1729-1 four times in 15 s)
    c.core.agents.get_mut(&f).unwrap().live.pid = Some(3);
    assert!(c.core.to_start_owing().is_empty(), "running");
    c.core.agents.get_mut(&f).unwrap().live.pid = None;
    assert!(c.core.to_start_owing().is_empty(), "the same step: its start stands");
    // a mark for another step does not hold this one back
    c.core.started_owing.insert(f.clone(), (std::time::Instant::now(), Some((t, 1))));
    assert_eq!(c.core.to_start_owing().len(), 1, "a new step");
    // the owner's start puts it in the step's layer, not the line's view
    assert_eq!(c.core.start_layer(&f), Some((t, l.clone())));
    // standing in the line's view, running: moved to the step's layer
    c.core.started_owing.clear();
    let a = c.core.agents.get_mut(&f).unwrap();
    a.live.pid = Some(2);
    a.live.state = "idle".into();
    c.stand(&f, Some("line@0123456789ab"));
    assert_eq!(c.core.move_due(&f).map(|m| m.to), Some(l.clone()));
    // the move marks it: while the moved session comes up it is not started for its step as well
    assert_eq!(c.core.to_move().len(), 1);
    c.core.agents.get_mut(&f).unwrap().live.pid = None;
    assert!(c.core.to_start_owing().is_empty(), "moving");
    c.core.moving.clear();
    assert!(c.core.to_start_owing().is_empty(), "moved a moment ago: the move's start stands");
    c.core.started_owing.clear();
    c.core.agents.get_mut(&f).unwrap().live.pid = Some(2);
    c.stand(&f, Some(&l));
    assert_eq!(c.core.move_due(&f), None);
    // ended by the owner, or with no session kept: not started
    let a = c.core.agents.get_mut(&f).unwrap();
    a.live.pid = None;
    a.ended = Some("x".into());
    assert!(c.core.to_start_owing().is_empty());
    c.core.agents.get_mut(&f).unwrap().ended = None;
    std::fs::remove_file(c.dir.join(format!("agents/{f}/session.json"))).unwrap();
    assert!(c.core.to_start_owing().is_empty());
}

/// docket-core8 (paper-research N-412 p1608): an agent's view keeps its ticket and items while the machine stands
/// (`Core::on_and_items`); a ticket written between two views gives the new view, as a view made afresh does.
#[test]
fn a_views_kept_ticket_is_read_again_after_a_ticket_write() {
    let mut c = Coord::new("viewkept");
    assert_eq!(c.core.view("bob").on, None);
    let p = c.open("plain");
    let pid = c.core.tickets.m.id(p);
    assert_eq!(c.core.view("bob").on.as_deref(), Some(pid.as_str()));
    assert_eq!(c.core.view("bob").on.as_deref(), Some(pid.as_str()), "kept, the machine standing");
    // a second step for bob: the higher ticket ranks first, read again since the open wrote the machine
    let q = c.open("plain");
    let qid = c.core.tickets.m.id(q);
    assert_eq!(c.core.view("bob").on.as_deref(), Some(qid.as_str()));
    // cancelled: the open one again
    c.core.ticket_input(Input::Cancel { t: q, by: "owner".into(), why: "x".into() }).unwrap();
    assert_eq!(c.core.view("bob").on.as_deref(), Some(pid.as_str()));
    // each view as one made with nothing kept
    let fresh = |c: &Coord, n: &str| docket_machine::label::agent_on(&c.core.tickets.m, n, &crate::label::read).map(|t| c.core.tickets.m.id(t));
    for n in ["bob", "ann"] {
        assert_eq!(c.core.view(n).on, fresh(&c, n), "{n}");
        assert_eq!(c.core.view(n).items, c.core.items_of(n), "{n}");
    }
}
