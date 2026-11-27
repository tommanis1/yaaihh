//! The record's one graph in coordd (proto-window N-1446 p1383, head 1): made along the start's replay and kept with
//! each input recorded after it (`Tickets::graph`), so the graph coordd serves is the one `graph::of` makes of the
//! record file, before a restart and after it; `Request::Graph` gives its slice, each node labelled as
//! `Request::Label` labels its id.

use super::delivery::tests::{coord, load, open, t_of};
use docket_machine::graph::{EdgeKind, Graph, Source};
use docket_machine::graph_slice::{Ask, Layer, Row};
use docket_machine::{Input, Line, Machine, Text};
use std::collections::BTreeSet;
use std::path::Path;

/// The graph of the record file at DIR, its lines replayed afresh under the core's kinds, as coordd's start does.
fn of_record(core: &crate::core::Core, dir: &Path) -> Graph {
    let lines: Vec<Line> = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    docket_machine::graph::of(&mut Machine::load(docket_machine::Defs::clone(&core.tickets.m.defs)), &lines).0
}

/// A and B the same graph, each node and edge from the same line at the same time, in whatever order they were added
/// (the start adds every `named` edge after the replay, a landing post its own after its group).
fn same(a: &Graph, b: &Graph) {
    let nodes = |g: &Graph| {
        let mut v = g.nodes.clone();
        v.sort_by(|x, y| x.id.cmp(&y.id));
        v
    };
    let edges = |g: &Graph| {
        let mut v = g.edges.clone();
        v.sort_by(|x, y| (&x.source, &x.from, &x.to, x.kind).cmp(&(&y.source, &y.from, &y.to, y.kind)));
        v
    };
    assert_eq!(nodes(a), nodes(b));
    assert_eq!(edges(a), edges(b));
}

/// The lead and w2 started, a lead's thread to w2, his post on it naming w2, which asks w2 as the ASKED ledger reads it
/// (paper-research N-1446 p1561, p1594), w2's answer: the graph coordd kept as they went in is the record
/// file's, node for node and edge for edge, each at its line and time, with the `named` edge of his `@w2` at his post's;
/// a restart makes it again; the slice around his post gives his edges in his layer, the agents' in theirs, and the
/// labels the record's words.
#[tokio::test]
async fn the_graph_coordd_keeps_is_the_one_its_record_makes_and_a_restart_makes_it_again() {
    let (dir, base, mut core, _rx) = coord("graph", "idle");
    // each started, as every agent of a coord has (roster's `began`): `now` has a row for an agent the graph shows a
    // turn or a start of (docket-edge5 N-1446 p1481, p1510)
    for a in ["lead", "w2"] {
        core.ticket_input(Input::Act { agent: a.into(), is: "began".into(), text: "began".into(), more: serde_json::Map::new(), t: None }).unwrap();
    }
    let a = open(&mut core, "lead", &["w2"], "the fixture is in place");
    let t = t_of(&a);
    core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("@w2 keep the d h m breakdown".into()), names: vec!["w2".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let his = format!("{}.p{}", core.tickets.m.id(t), core.tickets.m.tickets[&t].last());
    assert_eq!(core.tickets.m.asked(t, core.tickets.m.tickets[&t].last()), [("w2".to_string(), None)], "his post asks w2, as the ledger reads it");
    core.ticket_input(Input::Post { t, by: "w2".into(), text: Text::New("done: 4 d 18 h".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let w2s = format!("{}.p{}", core.tickets.m.id(t), core.tickets.m.tickets[&t].last());
    let made = of_record(&core, &dir);
    assert!(made.edges.len() >= 6, "{:?}", made.edges);
    let kept = core.tickets.graph.graph.clone();
    let named: Vec<_> = kept.edges.iter().filter(|e| e.kind == EdgeKind::Named).map(|e| (e.from.as_str(), e.to.as_str(), e.line(), e.when.clone())).collect();
    let post = kept.get(&his).unwrap();
    assert_eq!(named, [(his.as_str(), "agent:w2", post.line(), post.when.clone())], "his @w2, at his post's line and time");
    let mut record = kept.clone();
    record.edges.retain(|e| e.kind != EdgeKind::Named);
    same(&record, &made);
    // a restart: the record replayed, the Mentions read, their `named` edges added
    let (again, _rx2) = load(&dir, &base, "idle");
    same(&again.tickets.graph.graph, &kept);
    // around his post: his edges are his, what woke w2 a delivery; its labels as the record has them
    let answer = core.graph_slice(&Ask { around: Some(his.clone()), hops: 1, ..Ask::default() });
    let _ = std::fs::remove_dir_all(&dir);
    let layers: BTreeSet<Layer> = answer.slice.edges.iter().map(|e| e.layer).collect();
    assert!(layers.contains(&Layer::His) && layers.contains(&Layer::Deliveries), "{:?}", answer.slice.edges);
    assert!(answer.slice.edges.iter().all(|e| e.edge.when.is_some()), "each edge has its time");
    assert_eq!(answer.labels.len(), answer.slice.nodes.len());
    let label = |id: &str| answer.labels.iter().find(|l| l.id == id).map(|l| (l.is.clone(), l.label.clone()));
    assert!(label(&his).is_some_and(|(is, l)| is == "post" && l.contains("keep the d h m")), "{:?}", label(&his));
    assert!(label("agent:w2").is_some_and(|(is, _)| is == "agent"), "{:?}", answer.labels);
    assert_eq!(answer.slice.nodes.iter().find(|n| n.node.id == his).and_then(|n| n.by.as_deref()), Some("owner"));
    // with what is worked on now (head 2's right edge): his post among his asks, w2 among the agents; his post is in the
    // slice, so labelled once; each agent's last post labelled with its words, those out of the slice too (p1469, 6);
    // and the answer reads back as the window reads it
    let with = core.graph_slice(&Ask { around: Some(his.clone()), hops: 1, now: true, ..Ask::default() });
    let now = with.now.as_ref().expect("asked for");
    assert!(now.asks.iter().any(|a| a.times.post == his), "{:?}", now.asks);
    assert!(now.agents.iter().any(|a| a.agent == "w2"), "{:?}", now.agents);
    let last = |who: &str| now.agents.iter().find(|a| a.agent == who).and_then(|a| a.last_post.as_ref()).map(|p| p.id.clone()).unwrap_or_default();
    assert_eq!(last("w2"), w2s, "w2's answer is its last post");
    let outside: BTreeSet<String> = now.agents.iter().filter_map(|a| a.last_post.as_ref()).map(|p| p.id.clone()).filter(|p| !with.slice.nodes.iter().any(|n| n.node.id == *p)).collect();
    assert_eq!(with.labels.len(), with.slice.nodes.len() + outside.len(), "his post labelled once; each last post out of the slice once: {outside:?}");
    let words = |id: &str| with.labels.iter().find(|l| l.id == id).map(|l| l.label.clone()).unwrap_or_default();
    assert!(words(&w2s).contains("4 d 18 h"), "{:?}", with.labels);
    assert!(words(&last("lead")).contains("the fixture is in place"), "the lead's last post: {:?}", with.labels);
    // and around his post its course (p1429, 1), as the graph's ask times have it; around w2's post none: not his ask
    let course = with.course.as_ref().expect("his post's course");
    assert_eq!(Some(course), core.tickets.graph.graph.ask_times().iter().find(|t| t.post == his));
    assert_eq!(course.asked, core.tickets.graph.graph.get(&his).and_then(|n| n.when.clone()), "asked when he wrote it");
    assert!(answer.course.is_some(), "his course, whether or not now was asked");
    assert!(core.graph_slice(&Ask { around: Some(w2s.clone()), hops: 1, ..Ask::default() }).course.is_none(), "w2's post has no course");
    let back: coord_types::GraphAnswer = serde_json::from_str(&serde_json::to_string(&with).unwrap()).unwrap();
    assert_eq!(back, with);
    // its posts' words read off the lock (p2432): under it each post reads as who wrote it alone; its words put in
    // after, the answer is the one made whole, and each label what labelling its id alone gives
    let around = Ask { around: Some(his.clone()), hops: 1, ..Ask::default() };
    let (mut held, words) = core.graph_slice_held(&around);
    let posts: Vec<(String, String)> = held.labels.iter().filter(|l| l.is == "post").map(|l| (l.id.clone(), l.label.clone())).collect();
    assert!(posts.contains(&(his.clone(), "owner".into())) && posts.iter().all(|(_, l)| ["owner", "lead", "w2"].contains(&l.as_str())), "{posts:?}");
    words.fill(&mut held.labels);
    assert_eq!(held, answer);
    let agents: Vec<String> = core.agents.keys().cloned().collect();
    let alone: Vec<coord_types::Label> = held.labels.iter().map(|l| coord_types::Label { id: l.id.clone(), ..crate::label::of(&core.tickets.m, &agents, l.id.strip_prefix("agent:").unwrap_or(&l.id)) }).collect();
    assert_eq!(held.labels, alone);
    assert!(answer.now.is_none(), "not asked for, not given");
}

/// Head 1b (paper-research N-1446 p1406): a runner's job from its record lines (`act` "job", docket-core p1404) is
/// the kept graph's, as the record file's graph has it, with its item `named` at its start (p1422, 4); git's commits
/// of the project's repository come in off the lock (`git_read`, `graph_git_add`) once its refs move, each once, with
/// each one's `on` its branch from the reflog (p1476), and
/// the job's commit is then git's, with its subject and time; the slice around the commit in the work layer has the
/// job on it at its start and green at its end; the runner's files from before the record had its jobs (p1422, 2)
/// put in this coord's job, once, and not another coord's.
#[tokio::test]
async fn the_runners_jobs_from_the_record_and_gits_commits_come_into_the_kept_graph() {
    let (dir, _base, mut core, _rx) = coord("graph-git", "idle");
    let repo = dir.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str]| {
        let o = std::process::Command::new("git").args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "init.defaultBranch=main"]).args(args).current_dir(&repo).env("GIT_COMMITTER_DATE", "2026-11-21 08:41:33 +0000").env("GIT_AUTHOR_DATE", "2026-11-21 08:41:33 +0000").output().unwrap();
        assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };
    git(&["init", "-q"]);
    git(&["commit", "-q", "--allow-empty", "-m", "keep the d h m breakdown\n\nItem: I-7\nAgent: w2"]);
    let head = git(&["rev-parse", "HEAD"]);
    let cid = format!("commit:{}", &head[..8]);
    for d in ["files/kinds", "files/calls"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    std::fs::write(dir.join("files/project.toml"), "[project]\nname = \"g\"\nrepo = \"../repo\"\nrepos = { soft = \"../repo\" }\n").unwrap();
    std::fs::write(dir.join("project"), dir.join("files/project.toml").display().to_string()).unwrap();
    core.project = crate::project::Project::load(&dir.display().to_string()).unwrap();
    // w2's job on it, its start and its end as the runner writes them
    let act = |phase: &str, more: serde_json::Value| {
        let mut m = serde_json::json!({"id": "7001-1795251629", "phase": phase, "branch": "main", "commit": &head[..8], "changed": "0", "verb": "test", "packages": ["coordd"], "since": "2026-11-21T09:50:00+0100", "items": ["I-7"], "tree": "c210cae5"});
        m.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
        Input::Act { agent: "w2".into(), is: "job".into(), text: format!("job {phase}: test -p coordd"), more: m.as_object().unwrap().clone(), t: None }
    };
    core.ticket_input(act("start", serde_json::json!({}))).unwrap();
    core.ticket_input(act("end", serde_json::json!({"rc": "0", "passed": "12", "failed": "0", "secs": "40"}))).unwrap();
    same(&core.tickets.graph.graph, &of_record(&core, &dir));
    let job = core.tickets.graph.graph.get("job:7001-1795251629").cloned().unwrap();
    assert_eq!((job.when.as_deref(), job.what.as_str(), job.ok), (Some("2026-11-21T08:50:00Z"), "test coordd: rc 0, 12 passed, 0 failed", Some(true)));
    let named: Vec<_> = core.tickets.graph.graph.edges.iter().filter(|e| e.from == "job:7001-1795251629" && e.kind == EdgeKind::Named).map(|e| (e.to.as_str(), e.line(), e.when.as_deref())).collect();
    assert_eq!(named, [("I-7", job.line(), Some("2026-11-21T08:50:00Z"))], "what it was for, at its start");
    // git's read: the commit is git's now, its time and subject, w2's `wrote` from its trailer
    let read = crate::ticket::git_read(&core.project.as_ref().unwrap().repos(), &core.tickets.graph_git_refs());
    assert_eq!(read.iter().map(|r| (r.commits.len(), r.moves.len())).collect::<Vec<_>>(), [(1, 1)], "its commit, and main's one move");
    core.graph_git_add(read);
    let g = &core.tickets.graph.graph;
    assert!(g.edges.iter().any(|e| e.from == cid && e.to == "branch:soft/main" && e.kind == EdgeKind::On), "{:?}", g.edges);
    let c = g.get(&cid).unwrap();
    assert_eq!((c.when.as_deref(), c.what.as_str()), (Some("2026-11-21T08:41:33Z"), "keep the d h m breakdown"));
    assert!(g.edges.iter().any(|e| e.from == "agent:w2" && e.to == cid && e.kind == EdgeKind::Wrote), "{:?}", g.edges);
    // refs that did not move are not read again; a new commit is, and only it comes in
    assert!(crate::ticket::git_read(&core.project.as_ref().unwrap().repos(), &core.tickets.graph_git_refs()).is_empty());
    let edges = core.tickets.graph.graph.edges.len();
    git(&["commit", "-q", "--allow-empty", "-m", "the next one"]);
    let read = crate::ticket::git_read(&core.project.as_ref().unwrap().repos(), &core.tickets.graph_git_refs());
    assert_eq!(read.iter().map(|r| (r.commits.len(), r.moves.len())).collect::<Vec<_>>(), [(2, 2)], "the log is read whole, and main's reflog, the branch that moved");
    core.graph_git_add(read);
    let g = &core.tickets.graph.graph;
    assert_eq!(g.nodes.iter().filter(|n| n.kind == docket_machine::graph::NodeKind::Commit).count(), 2, "each commit once");
    let new: Vec<_> = g.edges[edges..].iter().map(|e| (e.kind, e.to.as_str())).collect();
    let made_in = docket_machine::graph_git::worktree_id(&std::fs::canonicalize(&repo).unwrap().display().to_string());
    assert_eq!(new, [(EdgeKind::CameFrom, cid.as_str()), (EdgeKind::On, "branch:soft/main"), (EdgeKind::In, made_in.as_str())], "the new commit's `came_from` its parent, its `on` main and its `in` the repository's worktree alone");
    // around the commit, in the work layer: the job on it at its start line, green at its end line
    let answer = core.graph_slice(&Ask { around: Some(cid.clone()), hops: 1, layers: BTreeSet::from([Layer::Work]), ..Ask::default() });
    let mut on: Vec<_> = answer.slice.edges.iter().filter(|e| e.edge.from == "job:7001-1795251629" && e.edge.to == cid).map(|e| (e.edge.kind, e.layer, e.edge.line())).collect();
    on.sort_by_key(|o| o.2);
    assert_eq!(on.iter().map(|o| (o.0, o.1)).collect::<Vec<_>>(), [(EdgeKind::Ran, Layer::Work), (EdgeKind::Ended, Layer::Work)], "{on:?}");
    assert!(on[0].2 == job.line() && on[1].2 > job.line(), "ran at its start line, ended at its end's: {on:?}");
    // the runner's files from before the record had its jobs: this coord's comes in from its file, once; another
    // coord's and a file that does not read do not
    let before = dir.join("jobs-before-record");
    std::fs::create_dir_all(&before).unwrap();
    let file = |coord: &str| serde_json::json!({"agent": "w2", "coord": coord, "pid": 6001, "branch": "main", "commit": &head[..8], "verb": "test", "packages": ["yaaihh"], "since": "2026-11-21T09:10:00+0100", "at": "2026-11-21T09:12:00+0100", "step": "ended", "rc": 0, "passed": 75, "failed": 0, "front": true}).to_string();
    std::fs::write(before.join("6001-1795250000.json"), file(&core.coord)).unwrap();
    std::fs::write(before.join("6002-1795250001.json"), file("/elsewhere")).unwrap();
    std::fs::write(before.join("6003-1795250002.json"), "{\"agent\": ").unwrap();
    let jobs = crate::jobs::of_coord(&before, &core.coord);
    assert_eq!(jobs.iter().map(|j| j.id.as_str()).collect::<Vec<_>>(), ["6001-1795250000"]);
    assert!(!core.tickets.graph_jobs_read());
    core.tickets.graph_jobs_add(&jobs);
    core.tickets.graph_jobs_add(&jobs);
    assert!(core.tickets.graph_jobs_read());
    let _ = std::fs::remove_dir_all(&dir);
    let g = &core.tickets.graph.graph;
    let j = g.get("job:6001-1795250000").unwrap();
    assert_eq!((j.when.as_deref(), j.ok, j.what.as_str()), (Some("2026-11-21T08:10:00Z"), Some(true), "test yaaihh: rc 0, 75 passed, 0 failed"));
    let id = "job:6001-1795250000";
    let its: Vec<_> = g.edges.iter().filter(|e| e.from == id || e.to == id).map(|e| (e.from.as_str(), e.to.as_str(), e.kind, e.source.clone(), e.when.as_deref())).collect();
    let src = Source::Job("6001-1795250000".into());
    assert_eq!(
        its,
        [
            ("agent:w2", id, EdgeKind::Ran, src.clone(), Some("2026-11-21T08:10:00Z")),
            (id, cid.as_str(), EdgeKind::Ran, src.clone(), Some("2026-11-21T08:10:00Z")),
            (id, cid.as_str(), EdgeKind::Ended, src, Some("2026-11-21T08:12:00Z")),
        ],
        "its file's, once"
    );
    assert!(g.get("job:6002-1795250001").is_none(), "another coord's job is not this record's");
}

/// The git round writes a `merged` line when a train branch of the project's repository moves (N-1511 p43;
/// paper-research N-1446 p2139, 1, p2152): none at the first read, which takes the trains as they stand; one for a
/// merge onto `train-1`, with the commits it brought and their parents, so the machine's `reaches` walks them; one for
/// `train-2`, new since, with its tip alone; none for a branch that is no train, or a round with nothing moved. A
/// coordd started again reads the tips from the record's lines, and a move while it was down is its next line.
#[tokio::test]
async fn a_trains_move_is_a_merged_line_with_the_commits_it_brought() {
    let (dir, base, mut core, _rx) = coord("graph-merged", "idle");
    let repo = dir.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str]| {
        let o = std::process::Command::new("git").args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "init.defaultBranch=main"]).args(args).current_dir(&repo).env("GIT_COMMITTER_DATE", "2026-11-23 02:17:25 +0000").env("GIT_AUTHOR_DATE", "2026-11-23 02:17:25 +0000").output().unwrap();
        assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };
    git(&["init", "-q"]);
    git(&["commit", "-q", "--allow-empty", "-m", "the base"]);
    let a = git(&["rev-parse", "HEAD"]);
    git(&["branch", "train-1"]);
    for d in ["files/kinds", "files/calls"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    std::fs::write(dir.join("files/project.toml"), "[project]\nname = \"g\"\nrepo = \"../repo\"\nrepos = { soft = \"../repo\" }\n").unwrap();
    std::fs::write(dir.join("project"), dir.join("files/project.toml").display().to_string()).unwrap();
    core.project = crate::project::Project::load(&dir.display().to_string()).unwrap();
    let round = |core: &mut crate::core::Core| {
        let read = crate::ticket::git_read(&core.project.as_ref().unwrap().repos(), &core.tickets.graph_git_refs());
        core.graph_git_add(read);
    };
    let merged = || -> Vec<serde_json::Value> {
        let log = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap_or_default();
        log.lines().map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap()).filter(|l| l["name"] == docket_machine::MERGED).map(|l| l["value"].clone()).collect()
    };
    round(&mut core);
    assert!(merged().is_empty(), "the first read takes the trains as they stand");
    // a head merged onto train-1, and a branch that is no train moved
    git(&["checkout", "-q", "-b", "head-1"]);
    git(&["commit", "-q", "--allow-empty", "-m", "the head"]);
    let b = git(&["rev-parse", "HEAD"]);
    git(&["checkout", "-q", "train-1"]);
    git(&["merge", "-q", "--no-ff", "-m", "merge head-1", "head-1"]);
    let m = git(&["rev-parse", "HEAD"]);
    let tree = |c: &str| git(&["rev-parse", &format!("{c}^{{tree}}")]);
    round(&mut core);
    let lines = merged();
    assert_eq!(lines.len(), 1, "{lines:?}");
    let v = &lines[0];
    assert_eq!((v["repo"].as_str(), v["branch"].as_str(), v["tip"].as_str(), v["from"].as_str(), v["tree"].as_str()), (Some("soft"), Some("train-1"), Some(m.as_str()), Some(a.as_str()), Some(tree(&m).as_str())));
    assert_eq!(v["parents"], serde_json::json!([a, b]));
    assert_eq!(v["commits"], serde_json::json!([{"commit": m, "parents": [a, b], "tree": tree(&m)}, {"commit": b, "parents": [a], "tree": tree(&b)}]));
    assert_eq!(core.tickets.m.seen.tips["soft/train-1"], m);
    assert_eq!(core.tickets.m.seen.fact("reaches", &[serde_json::json!(m), serde_json::json!(&b[..8])]), serde_json::json!(true));
    // a train new since: its tip alone; then nothing moved, nothing written
    git(&["branch", "train-2"]);
    round(&mut core);
    let lines = merged();
    assert_eq!((lines.len(), lines[1]["branch"].as_str(), lines[1]["from"].clone(), lines[1]["commits"].as_array().map(Vec::len)), (2, Some("train-2"), serde_json::Value::Null, Some(1)));
    round(&mut core);
    assert_eq!(merged().len(), 2);
    // started again: the tips come from the record, and a move while it was down is the next line, from where it stood
    git(&["checkout", "-q", "train-1"]);
    git(&["commit", "-q", "--allow-empty", "-m", "0.3.1"]);
    let v1 = git(&["rev-parse", "HEAD"]);
    let (mut again, _rx2) = load(&dir, &base, "idle");
    again.project = core.project.clone();
    assert_eq!(again.tickets.m.seen.tips, core.tickets.m.seen.tips);
    round(&mut again);
    let lines = merged();
    assert_eq!((lines.len(), lines[2]["tip"].as_str(), lines[2]["from"].as_str(), lines[2]["commits"].as_array().map(Vec::len)), (3, Some(v1.as_str()), Some(m.as_str()), Some(1)));
    let _ = std::fs::remove_dir_all(&dir);
}

/// His ask reaches the commit that names it, in the kept graph (paper-research N-1446 p1433), as N-1104.p678 reaches
/// de447f86 "(owner N-1104 p678)": git's commits come in (`Core::graph_git_add`, as the side's clock puts them in) with
/// what each one an agent here wrote names (`mentions::commits_into`), so his post's `ask_times` has that commit
/// first; the same words in a commit by no agent here name nothing.
#[tokio::test]
async fn his_ask_reaches_the_commit_that_names_it_in_the_kept_graph() {
    let (dir, _base, mut core, _rx) = coord("graph-named", "idle");
    let a = open(&mut core, "lead", &["w2"], "the fixture is in place");
    let t = t_of(&a);
    core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("@w2 keep the d h m breakdown".into()), names: vec!["w2".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let (id, n) = (core.tickets.m.id(t), core.tickets.m.tickets[&t].last());
    let his = format!("{id}.p{n}");
    let repo = dir.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str]| {
        let o = std::process::Command::new("git").args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "init.defaultBranch=main"]).args(args).current_dir(&repo).env("GIT_COMMITTER_DATE", "2026-11-21 08:41:33 +0000").env("GIT_AUTHOR_DATE", "2026-11-21 08:41:33 +0000").output().unwrap();
        assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };
    git(&["init", "-q"]);
    git(&["commit", "-q", "--allow-empty", "-m", &format!("keep the d h m breakdown (owner {id} p{n})\n\nAgent: w2")]);
    let head = git(&["rev-parse", "HEAD"]);
    git(&["commit", "-q", "--allow-empty", "-m", &format!("the same words (owner {id} p{n})\n\nAgent: w9")]);
    for d in ["files/kinds", "files/calls"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    std::fs::write(dir.join("files/project.toml"), "[project]\nname = \"g\"\nrepo = \"../repo\"\nrepos = { soft = \"../repo\" }\n").unwrap();
    std::fs::write(dir.join("project"), dir.join("files/project.toml").display().to_string()).unwrap();
    core.project = crate::project::Project::load(&dir.display().to_string()).unwrap();
    let read = crate::ticket::git_read(&core.project.as_ref().unwrap().repos(), &core.tickets.graph_git_refs());
    assert_eq!(read.iter().map(|r| r.commits.len()).sum::<usize>(), 2);
    core.graph_git_add(read);
    let _ = std::fs::remove_dir_all(&dir);
    let g = &core.tickets.graph.graph;
    let cid = format!("commit:{}", &head[..8]);
    let named: Vec<_> = g.edges.iter().filter(|e| e.kind == EdgeKind::Named && e.from.starts_with("commit:")).map(|e| (e.from.as_str(), e.to.as_str(), e.source.clone(), e.when.as_deref())).collect();
    assert_eq!(named, [(cid.as_str(), his.as_str(), Source::Commit(head[..8].to_string()), Some("2026-11-21T08:41:33Z"))], "w2's commit names his post, w9's nothing");
    let times = g.ask_times().into_iter().find(|x| x.post == his).expect("his ask");
    assert_eq!(times.first_commit.map(|r| (r.id, r.at)), Some((cid.clone(), "2026-11-21T08:41:33Z".to_string())), "his ask reaches w2's commit");
}

/// The kept graph reads git from the repositories the folders where agents work sit in, as the record names them
/// (paper-research N-1446 p1455): an edit's files' folders, a job's worktree on its line and on the runner's file from
/// before the record had it. A worktree's repository is read once, by its main folder's name; a repository inside
/// another is one of its own; a folder in none gives none, a `.git` folder git does not take for a repository
/// passed as git passes it; a place never read is not asked, whatever repository is there; a folder gone, or whose
/// worktree link git no longer reads, is named, never put under another; git runs nothing
/// a repository's config names; each folder is placed once, and a repository read again only when its refs move.
/// Of those read (p1476), a repository's commits come in once one of them is by an agent here or a job from here ran
/// on one, so one with an edit in it and commits by no one here stays out until an agent here commits there; with
/// them its branches' moves from the reflogs, so a commit only a reflog reaches is in, `on` its branch; and the slice
/// labels each commit by what was read, with no git asked; a round out across a reload puts nothing, the next reads
/// the new graph's whole.
#[tokio::test]
async fn the_kept_graph_reads_git_where_the_record_says_agents_work() {
    use crate::ticket::work_repos::Placed;
    let (dir, _base, mut core, _rx) = coord("graph-repos", "idle");
    let at = |p: &str| dir.join(p).display().to_string();
    let real = |p: &str| std::fs::canonicalize(&dir).unwrap().join(p).display().to_string();
    let git = |repo: &str, args: &[&str]| {
        let o = std::process::Command::new("git").args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "init.defaultBranch=main"]).args(args).current_dir(dir.join(repo)).env("GIT_COMMITTER_DATE", "2026-11-22 10:00:00 +0000").env("GIT_AUTHOR_DATE", "2026-11-22 10:00:00 +0000").output().unwrap();
        assert!(o.status.success(), "git {args:?} in {repo}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };
    // each repository with one commit, its subject the repository's folder; main's inside one of its own and one
    // under a name never read; another never read for being in the home's `.ssh`; main's and inner's by agents here,
    // elsewhere's by no one here; other's and late's by no one, a job from here ran on each
    let by = |r: &str| match r {
        "main" | "main/claude-home" | "home/.ssh/keys" => "\n\nAgent: w2",
        "main/inner" => "\n\nAgent: someone, w3",
        "elsewhere" => "\n\nAgent: someone",
        _ => "",
    };
    for r in ["main", "main/inner", "main/claude-home", "other", "late", "home/.ssh/keys", "signed", "elsewhere"] {
        std::fs::create_dir_all(dir.join(r)).unwrap();
        git(r, &["init", "-q"]);
        if r != "signed" {
            git(r, &["commit", "-q", "--allow-empty", "-m", &format!("{r}{}", by(r))]);
        }
    }
    // a commit on main that only main's reflog reaches now
    git("main", &["commit", "-q", "--allow-empty", "-m", "dropped\n\nAgent: w2"]);
    let dropped = format!("commit:{}", &git("main", &["rev-parse", "HEAD"])[..8]);
    git("main", &["reset", "-q", "--hard", "HEAD~1"]);
    // main's worktree with a commit of its own, and one whose repository's record of it was taken away
    git("main", &["worktree", "add", "-q", "-b", "side", &at("wt")]);
    git("wt", &["commit", "-q", "--allow-empty", "-m", "wt"]);
    git("main", &["worktree", "add", "-q", "-b", "taken", &at("wt2")]);
    std::fs::remove_dir_all(dir.join("main/.git/worktrees/wt2")).unwrap();
    // a repository whose config would have a read run a program: its commit signed, its log showing signatures
    let gpg = dir.join("gpg.sh");
    crate::accounts::testing::program(&gpg, &format!("#!/bin/sh\ntouch '{}'\n", dir.join("ran").display()));
    git("signed", &["config", "log.showSignature", "true"]);
    git("signed", &["config", "gpg.program", &gpg.display().to_string()]);
    let tree = git("signed", &["mktree"]);
    std::fs::write(dir.join("signed.commit"), format!("tree {tree}\nauthor t <t@t> 1795341600 +0000\ncommitter t <t@t> 1795341600 +0000\ngpgsig -----BEGIN PGP SIGNATURE-----\n \n iQEzBAABCAAdFiEE\n -----END PGP SIGNATURE-----\n\nsigned\n\nAgent: w2\n")).unwrap();
    let signed = git("signed", &["hash-object", "-t", "commit", "-w", &at("signed.commit")]);
    git("signed", &["update-ref", "refs/heads/main", &signed]);
    // folders in none, one of them with a `.git` folder git does not take for a repository, and one such inside main
    for d in ["plain", "stray/.git", "main/vendor/.git"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    // w2's edits, and its job on `other` from its worktree
    let edit = |files: &[&str]| Input::Act { agent: "w2".into(), is: "edit".into(), text: "Edit".into(), more: serde_json::json!({"tool": "Edit", "files": files.iter().map(|f| at(f)).collect::<Vec<_>>()}).as_object().unwrap().clone(), t: None };
    core.ticket_input(edit(&["main/a.rs", "wt/b.rs", "main/inner/c.rs", "plain/d.rs", "gone/deep/e.rs", "main/claude-home/f.rs", "home/.ssh/keys/g", "wt2/h.rs", "signed/i.rs", "stray/j.rs", "main/vendor/k.rs", "elsewhere/l.rs"])).unwrap();
    let other = git("other", &["rev-parse", "--short=8", "HEAD"]);
    let job = serde_json::json!({"id": "7101-1795340000", "phase": "start", "wt": at("other"), "branch": "main", "commit": &other, "verb": "test", "packages": ["coordd"], "since": "2026-11-22T11:00:00+0100"});
    core.ticket_input(Input::Act { agent: "w2".into(), is: "job".into(), text: "job start: test -p coordd".into(), more: job.as_object().unwrap().clone(), t: None }).unwrap();
    // the runner's file from before the record had its jobs: one on `late`
    let late = git("late", &["rev-parse", "--short=8", "HEAD"]);
    let before = vec![docket_machine::graph_git::Job { id: "7102-1795339000".into(), agent: "w2".into(), coord: core.coord.clone(), wt: at("late"), branch: "main".into(), commit: late, verb: "test".into(), since: "2026-11-22T10:50:00+0100".into(), at: "2026-11-22T10:51:00+0100".into(), step: "ended".into(), rc: Some(0), ..Default::default() }];
    // the side clock's round of git, the first time
    let home = at("home");
    let round = core.graph_git_reading().read(&before, &home);
    core.graph_git_put(round, Some(before.clone()));
    assert!(!dir.join("ran").exists(), "git ran the program the signed repository's config names");
    assert_eq!(core.graph_repos(), [("elsewhere", "elsewhere"), ("late", "late"), ("main", "main"), ("inner", "main/inner"), ("other", "other"), ("signed", "signed")].map(|(n, d)| (n.to_string(), real(d))));
    let (counted, left) = core.tickets.graph_git.counted_and_left();
    assert_eq!((counted, left), (["late", "main", "main/inner", "other", "signed"].map(real).iter().collect(), vec![&real("elsewhere")]), "elsewhere has no commit by an agent here");
    let subjects = |core: &crate::core::Core| {
        let mut s: Vec<String> = core.tickets.graph.graph.nodes.iter().filter(|n| n.kind == docket_machine::graph::NodeKind::Commit && n.id != dropped).map(|n| n.what.clone()).collect();
        s.sort();
        s
    };
    assert_eq!(subjects(&core), ["late", "main", "main/inner", "other", "signed", "wt"], "the never-read places' commits are not read, nor elsewhere's");
    let g = &core.tickets.graph.graph;
    let on = |g: &Graph, c: &str| -> Vec<String> { g.edges.iter().filter(|e| e.from == c && e.kind == EdgeKind::On).map(|e| e.to.clone()).collect() };
    assert_eq!((g.get(&dropped).map(|n| n.what.as_str()), on(g, &dropped)), (Some(""), vec!["branch:main/main".to_string()]), "the commit only the reflog reaches, `on` main");
    assert_eq!(core.tickets.graph_git.gone(), [at("gone"), real("wt2")], "the `.git` folders git passes are no repository gone");
    let answer = core.graph_slice(&Ask::default());
    assert_eq!(answer.gone, [at("gone"), real("wt2")], "named in the answer");
    // the next read places nothing again and reads nothing whose refs did not move; a new folder is placed, a
    // repository with a new commit read again, alone; elsewhere, an agent here's commit in it now, comes in whole
    let round = core.graph_git_reading().read(&[], &home);
    assert!(round.placed.is_empty() && round.read.is_empty(), "{:?} {:?}", round.placed, round.read.iter().map(|r| &r.dir).collect::<Vec<_>>());
    std::fs::create_dir_all(dir.join("main/src")).unwrap();
    core.ticket_input(edit(&["main/src/x.rs"])).unwrap();
    git("main/inner", &["commit", "-q", "--allow-empty", "-m", "inner again"]);
    git("elsewhere", &["commit", "-q", "--allow-empty", "-m", "elsewhere again\n\nAgent: w2"]);
    let round = core.graph_git_reading().read(&[], &home);
    assert_eq!(round.placed, [(at("main/src"), Placed::In { top: real("main"), common: real("main/.git") })]);
    assert_eq!(round.read.iter().map(|r| (r.dir.clone(), r.commits.len(), r.moves.len())).collect::<Vec<_>>(), [(real("elsewhere"), 2, 2), (real("main/inner"), 2, 2)]);
    core.graph_git_put(round, None);
    assert!(core.tickets.graph_git.counted_and_left().1.is_empty());
    assert_eq!(subjects(&core), ["elsewhere", "elsewhere again", "inner again", "late", "main", "main/inner", "other", "signed", "wt"]);
    let g = &core.tickets.graph.graph;
    let first = g.nodes.iter().find(|n| n.what == "elsewhere").map(|n| n.id.clone()).unwrap();
    assert_eq!(on(g, &first), ["branch:elsewhere/main"], "its move from the first read, set aside with its commit, once");
    // a round out across a reload of the record, late read again in it, puts nothing in the new graph; the next reads
    // it whole
    let mut all = subjects(&core);
    let ons = |core: &crate::core::Core| core.tickets.graph.graph.edges.iter().filter(|e| e.kind == EdgeKind::On).count();
    let on_before = ons(&core);
    git("late", &["commit", "-q", "--allow-empty", "-m", "late again"]);
    let out = core.graph_git_reading().read(&[], &home);
    assert_eq!(out.read.iter().map(|r| (r.dir.clone(), r.commits.len())).collect::<Vec<_>>(), [(real("late"), 2)]);
    core.load_tickets();
    core.graph_git_put(out, Some(before.clone()));
    assert_eq!((subjects(&core), ons(&core)), (vec![String::new()], 0), "the round from before the reload: only the job line's commit, no subject");
    let round = core.graph_git_reading().read(&before, &home);
    core.graph_git_put(round, Some(before));
    all.push("late again".into());
    all.sort();
    assert_eq!((subjects(&core), ons(&core)), (all, on_before + 1), "read whole again, late again `on` its main");
    let g = &core.tickets.graph.graph;
    // the folders gone: the slice labels each commit by what was read, no git asked
    let _ = std::fs::remove_dir_all(&dir);
    let main = g.nodes.iter().find(|n| n.what == "main").map(|n| n.id.clone()).unwrap();
    let answer = core.graph_slice(&Ask { around: Some(main.clone()), hops: 1, ..Ask::default() });
    let label = answer.labels.iter().find(|l| l.id == main).map(|l| (l.is.as_str(), l.label.as_str(), l.on.as_deref()));
    assert_eq!(label, Some(("commit", "main", Some("main"))));
}

/// Where each commit was made (paper-research N-1446 p1530, p1554): `in` the worktree whose own HEAD reflog first has
/// git make it, by the top `rev-parse --show-toplevel` gives in it (`Placed::In`'s, so a job's worktree and a commit's
/// are one node); a fast-forward or a checkout makes none there; a commit amended away is still where it was made; a
/// worktree removed leaves what it made in none ("worktree not known"), though its branch still has it, `on` as before;
/// the next read puts in only the new commit's.
#[tokio::test]
async fn a_commit_is_in_the_worktree_whose_head_reflog_first_made_it() {
    use docket_machine::graph_git::{commit_id, worktree_id};
    use std::collections::BTreeMap;
    let (dir, _base, mut core, _rx) = coord("graph-worktrees", "idle");
    let at = |p: &str| dir.join(p).display().to_string();
    let real = |p: &str| std::fs::canonicalize(&dir).unwrap().join(p).display().to_string();
    let git = |wt: &str, clock: &str, args: &[&str]| {
        let when = format!("2026-11-22 {clock} +0000");
        let o = std::process::Command::new("git").args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "init.defaultBranch=main"]).args(args).current_dir(dir.join(wt)).env("GIT_COMMITTER_DATE", &when).env("GIT_AUTHOR_DATE", &when).output().unwrap();
        assert!(o.status.success(), "git {args:?} in {wt}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };
    let head = |wt: &str| commit_id(&git(wt, "10:00:00", &["rev-parse", "HEAD"]));
    std::fs::create_dir_all(dir.join("repo")).unwrap();
    git("repo", "10:00:00", &["init", "-q"]);
    git("repo", "10:00:00", &["commit", "-q", "--allow-empty", "-m", "first"]);
    let first = head("repo");
    git("repo", "10:01:00", &["worktree", "add", "-q", "-b", "side", &at("wt")]);
    git("wt", "10:02:00", &["commit", "-q", "--allow-empty", "-m", "side one"]);
    let one = head("wt");
    git("repo", "10:03:00", &["merge", "-q", "--ff-only", "side"]);
    git("wt", "10:04:00", &["commit", "-q", "--allow-empty", "-m", "side two"]);
    let two = head("wt");
    git("wt", "10:05:00", &["commit", "-q", "--allow-empty", "--amend", "-m", "side two, amended"]);
    let amended = head("wt");
    git("repo", "10:06:00", &["merge", "-q", "--no-ff", "-m", "merge side", "side"]);
    let merge = head("repo");
    git("wt", "10:07:00", &["commit", "-q", "--allow-empty", "-m", "side three"]);
    let three = head("wt");
    git("repo", "10:08:00", &["cherry-pick", "--allow-empty", "side"]);
    let picked = head("repo");
    git("repo", "10:09:00", &["worktree", "add", "-q", "-b", "gone", &at("wt3")]);
    git("wt3", "10:10:00", &["commit", "-q", "--allow-empty", "-m", "made where it is gone"]);
    let gone = head("wt3");
    git("repo", "10:11:00", &["worktree", "remove", "--force", &at("wt3")]);
    git("repo", "10:12:00", &["checkout", "-q", "gone"]);
    git("repo", "10:13:00", &["checkout", "-q", "main"]);
    for d in ["files/kinds", "files/calls"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    std::fs::write(dir.join("files/project.toml"), "[project]\nname = \"g\"\nrepo = \"../repo\"\nrepos = { soft = \"../repo\" }\n").unwrap();
    std::fs::write(dir.join("project"), dir.join("files/project.toml").display().to_string()).unwrap();
    core.project = crate::project::Project::load(&dir.display().to_string()).unwrap();
    let ins = |core: &crate::core::Core| -> BTreeMap<String, (String, Option<String>)> {
        let g = &core.tickets.graph.graph;
        g.edges.iter().filter(|e| e.kind == EdgeKind::In && g.get(&e.from).is_some_and(|n| n.kind == docket_machine::graph::NodeKind::Commit)).map(|e| (e.from.clone(), (e.to.clone(), e.when.clone()))).collect()
    };
    let read = crate::ticket::git_read(&core.project.as_ref().unwrap().repos(), &core.tickets.graph_git_refs());
    assert_eq!(read.iter().map(|r| r.made.len()).collect::<Vec<_>>(), [7], "repo's first, merge and pick; wt's one, two, amended and three");
    core.graph_git_add(read);
    let (repo, wt) = (worktree_id(&real("repo")), worktree_id(&real("wt")));
    let when = |clock: &str| Some(format!("2026-11-22T{clock}Z"));
    let want = BTreeMap::from([
        (first.clone(), (repo.clone(), when("10:00:00"))),
        (one.clone(), (wt.clone(), when("10:02:00"))),
        (two.clone(), (wt.clone(), when("10:04:00"))),
        (amended.clone(), (wt.clone(), when("10:05:00"))),
        (merge.clone(), (repo.clone(), when("10:06:00"))),
        (three.clone(), (wt.clone(), when("10:07:00"))),
        (picked.clone(), (repo.clone(), when("10:08:00"))),
    ]);
    assert_eq!(ins(&core), want, "side one fast-forwarded into repo is wt's; what wt3 made is in none");
    let g = &core.tickets.graph.graph;
    assert_eq!([&repo, &wt].map(|w| g.get(w).map(|n| n.kind)), [Some(docket_machine::graph::NodeKind::Worktree); 2]);
    let on = |c: &str| -> Vec<String> { g.edges.iter().filter(|e| e.from == c && e.kind == EdgeKind::On).map(|e| e.to.clone()).collect() };
    assert_eq!((g.get(&gone).map(|n| n.what.as_str()), on(&gone)), (Some("made where it is gone"), vec!["branch:soft/gone".to_string()]), "its branch has it, as before");
    assert_eq!(on(&one), ["branch:soft/main", "branch:soft/side"], "the `on` edges as they were");
    // a new commit in wt: its own `in` alone
    let edges = core.tickets.graph.graph.edges.len();
    git("wt", "10:14:00", &["commit", "-q", "--allow-empty", "-m", "side four"]);
    let four = head("wt");
    let read = crate::ticket::git_read(&core.project.as_ref().unwrap().repos(), &core.tickets.graph_git_refs());
    assert_eq!(read.iter().map(|r| r.made.len()).collect::<Vec<_>>(), [8], "each reflog read whole again");
    core.graph_git_add(read);
    let g = &core.tickets.graph.graph;
    let new: Vec<_> = g.edges[edges..].iter().filter(|e| e.kind == EdgeKind::In).map(|e| (e.from.clone(), e.to.clone())).collect();
    assert_eq!(new, [(four, wt)]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// What the graph costs coordd's start, on a copy (REPLAY_COORD, REPLAY_BASE, REPLAY_PROJECT as
/// `a_coords_record_replays` reads them): the start's replay of the record and its Mentions read, as `load_joint` and
/// `load_tickets` do them, with GRAPH_COST=with the Builder along the replay and `named_into` after the read, with
/// GRAPH_COST=plain neither, as the start was before; one of them a process, so that the process's peak memory
/// (VmHWM) is that one's. Prints the replay's time, the read's, `named_into`'s, the peak and what is held at the end
/// (VmRSS), and the graph's size; with the Builder, its own part of the replay, timed alone. Run here, behind the replay lock, each way in turn, on a binary built with the
/// profile the start's is (paper-research N-1446 p1385: "the start replay's time with and without the Builder, and its
/// memory too").
#[test]
#[ignore]
fn a_coords_record_graph_costs() {
    let (Ok(coord), Ok(base)) = (std::env::var("REPLAY_COORD"), std::env::var("REPLAY_BASE")) else { return };
    let with = match std::env::var("GRAPH_COST").as_deref() {
        Ok("with") => true,
        Ok("plain") => false,
        other => panic!("GRAPH_COST is with or plain, not {other:?}"),
    };
    let places = crate::policy::Places { base, project: std::env::var("REPLAY_PROJECT").ok(), coord: coord.clone(), ..Default::default() };
    let (defs, mut errors, _, _) = crate::ticket::Tickets::read_defs(&places, 1);
    let (_, lines) = crate::ticket::Tickets::read_log(&format!("{coord}/tickets/log.jsonl"), &mut errors);
    let kb = |key: &str| std::fs::read_to_string("/proc/self/status").ok().and_then(|s| s.lines().find(|l| l.starts_with(key)).and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())).unwrap_or(0);
    let before = kb("VmRSS:");
    let started = std::time::Instant::now();
    let mut m = Machine::load(defs);
    let mut b = docket_machine::graph::Builder::default();
    // the Builder's own part of the replay, timed alone: a whole replay's time varies more than the Builder takes
    let mut building = std::time::Duration::ZERO;
    let problems = if with {
        m.replay_with(&lines, &mut |m, i, group| {
            let at = std::time::Instant::now();
            b.after(m, i, group);
            building += at.elapsed();
        })
    } else {
        m.replay(&lines)
    };
    let replay = started.elapsed().as_secs_f64();
    let building = building.as_secs_f64();
    let started = std::time::Instant::now();
    let mut idx = crate::ticket::mentions::Mentions::default();
    idx.read_all(&m, &crate::label::read);
    let read = started.elapsed().as_secs_f64();
    let started = std::time::Instant::now();
    if with {
        // which names are agents: the graph's, as the printout has them (a copy has no registry)
        let agents: BTreeSet<String> = b.graph.nodes.iter().filter_map(|n| n.id.strip_prefix("agent:").map(str::to_string)).collect();
        idx.named_into(&m, &|a| agents.contains(a), &mut b.graph);
    }
    let named = started.elapsed().as_secs_f64();
    let (nodes, edges) = (b.graph.nodes.len(), b.graph.edges.len());
    let untimed = b.graph.edges.iter().filter(|e| e.when.is_none()).count();
    println!(
        "graph cost {}: {} lines, {} load errors, {} replay problems, {} graph problems; replay {replay:.2} s (the Builder {building:.2} s of it), mentions read {read:.2} s, named_into {named:.2} s; VmRSS before {} MB, at the end {} MB, peak (VmHWM) {} MB; graph {nodes} nodes, {edges} edges ({untimed} with no time)",
        if with { "with" } else { "plain" },
        lines.len(),
        errors.len(),
        problems.len(),
        b.problems.len(),
        before / 1024,
        kb("VmRSS:") / 1024,
        kb("VmHWM:") / 1024,
    );
}

/// Every ticket's label and title on a copy of a coord's record (REPLAY_COORD, REPLAY_BASE, REPLAY_PROJECT as
/// `a_coords_record_graph_costs` reads them), one line each to LABELS_OUT: `ID<TAB>label<TAB>title`, as coordd's
/// `label` request and `TicketView.title` give them. Run behind the replay lock with a binary of each side of a label
/// change, the two files compared, for the count of labels it changes (paper-research N-1446 p3189).
#[test]
#[ignore]
fn a_coords_record_labels() {
    let (Ok(coord), Ok(base), Ok(out)) = (std::env::var("REPLAY_COORD"), std::env::var("REPLAY_BASE"), std::env::var("LABELS_OUT")) else { return };
    let places = crate::policy::Places { base, project: std::env::var("REPLAY_PROJECT").ok(), coord: coord.clone(), ..Default::default() };
    let (defs, mut errors, _, _) = crate::ticket::Tickets::read_defs(&places, 1);
    let (_, lines) = crate::ticket::Tickets::read_log(&format!("{coord}/tickets/log.jsonl"), &mut errors);
    let mut m = Machine::load(defs);
    let problems = m.replay(&lines);
    let mut text = String::new();
    for (t, x) in m.tickets.iter() {
        let flat = |s: String| s.replace(['\t', '\n'], " ");
        text += &format!("{}\t{}\t{}\n", m.id(*t), flat(docket_machine::label::ticket(&m, *t, &crate::label::read)), flat(docket_machine::label::title(&m, x)));
    }
    std::fs::write(&out, text).unwrap();
    println!("labels: {} lines, {} load errors, {} replay problems, {} tickets to {out}", lines.len(), errors.len(), problems.len(), m.tickets.len());
}

/// Coordd's answers to the window's graph asks on a copy of a coord's record (REPLAY_COORD: its `tickets/log.jsonl`,
/// `registry.jsonl` and `project`; REPLAY_BASE), as `Request::Graph` answers them (main.rs): the start's kept graph
/// (`Core::load`, the project, `load_tickets`), then a round of git's side as the side's clock reads it (main.rs
/// `read_sides`: the folders the record names placed, with the worktrees of the runner's jobs from before the record of
/// GRAPH_JOBS_COORD, the coord the copy is of, whose path the jobs name; each repository read; HOME the owner's) and
/// one of the agents' side ([`read_side`], as `read_sides` reads it: an agent's folder has its sessions from it), then
/// each ask of GRAPH_ASKS (a JSON list of `{"name", "ask"}`; an ask around `newest` is around his newest post) sliced
/// and labelled (`Core::graph_slice`, its commits by the kept graph's read). Each is written to
/// GRAPH_ANSWERS_OUT/NAME.json as `{"ask", "answer"}`, for the window's `lanes_stills` (paper-research N-1446 p1376:
/// "stills of his record") and `folder_stills` (p1530), and its course printed, a folder's groups by their counts;
/// before the asks, the kept graph's duplicate edges by kind and this process's peak (p1838; GRAPH_ASKS may be `[]`). Run
/// here on a fresh copy, behind the replay lock, with COORD_ACCOUNTS and COORD_CONVERSATIONS at empty folders of its
/// own.
#[tokio::test]
#[ignore]
async fn a_coords_record_graph_answers() {
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let (Some(coord), Some(base), Some(asks), Some(out)) = (var("REPLAY_COORD"), var("REPLAY_BASE"), var("GRAPH_ASKS"), var("GRAPH_ANSWERS_OUT")) else { return };
    let started = std::time::Instant::now();
    // his record's now is its last line's time: the lines the load writes after it (`load_tickets` cuts the turn of
    // each agent whose process is gone, as a start does) are this run's, not his record's, so each slice ends there
    // and its `now` is then (paper-research N-1446 p1469, 6: on the copy at 12:47Z every agent in a turn read "idle
    // since 13:00:26", the load's time)
    let last_at = {
        use std::io::{Read, Seek, SeekFrom};
        let mut f = std::fs::File::open(Path::new(&coord).join("tickets/log.jsonl")).unwrap();
        let len = f.metadata().unwrap().len();
        f.seek(SeekFrom::Start(len.saturating_sub(1 << 16))).unwrap();
        let mut tail = Vec::new();
        f.read_to_end(&mut tail).unwrap();
        String::from_utf8_lossy(&tail).lines().rev().find_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()?["at"].as_str().map(String::from))
    };
    println!("his record's last line at {last_at:?}");
    let mut core = crate::core::Core::load(&coord, &base);
    core.project = crate::project::Project::load(&coord).unwrap();
    core.load_tickets();
    // coordd's memory at three points, and by part at the one MEM_STOP names: `start`, `side`, else after the reads
    // (paper-research N-1446 p1900)
    let stop = var("MEM_STOP").unwrap_or_default();
    memory("at the start");
    if stop == "start" {
        return memory_by_part(core, "at the start");
    }
    // git's side, one round as the side's clock reads it (main.rs `read_sides`): it places every folder the record names
    // and reads every repository they sit in
    let before = var("GRAPH_JOBS_COORD").map(|of| crate::jobs::of_coord(&crate::jobs::before_record_dir(), &of));
    println!("jobs from before the record of {:?}: {}", var("GRAPH_JOBS_COORD"), before.as_ref().map_or(0, |b| b.len()));
    let round = core.graph_git_reading().read(before.as_deref().unwrap_or_default(), &std::env::var("HOME").unwrap_or_default());
    core.graph_git_put(round, before);
    // the agents' side, one round as the side's clock reads it (`read_sides`), of GRAPH_SIDE's sessions (docket-edge5's
    // unless it names another), or with GRAPH_SIDE_ALL of every session a `given` names, as
    // `a_coords_record_graph_costs` picks them: a copy has no registry, and `Core::side_round` leaves out an agent not
    // made in its record (the run of 19:34 22 Nov gave docket-edge5's folder no session)
    let (who, all) = (var("GRAPH_SIDE").unwrap_or_else(|| "docket-edge5".into()), var("GRAPH_SIDE_ALL").is_some());
    let picked: BTreeSet<String> = core.tickets.side.agents().filter(|a| all || *a == who).map(String::from).collect();
    let round = core.tickets.side.round(&picked).read();
    println!("the side of {picked:?}: {} sessions not opened", round.unopened().len());
    for one in round.each() {
        core.side_put(one);
    }
    let repos = core.graph_repos();
    println!("gone: {}", core.tickets.graph_git.gone().len());
    memory("after the sessions' side");
    let g = &core.tickets.graph.graph;
    println!("graph of {coord}: {} repositories, {} nodes, {} edges, in {} s", repos.len(), g.nodes.len(), g.edges.len(), started.elapsed().as_secs());
    // the kept graph's duplicate edges (paper-research N-1446 p1838), by kind: an edge whose ends, kind and source an
    // earlier edge has, and one whose ends and kind an earlier edge has from another source; the sessions with the
    // most of the first; and this process's peak and what it holds, coordd's start and its sides' first round
    {
        use std::collections::{BTreeMap, HashSet};
        let (mut exact, mut ends): (HashSet<(&str, &str, EdgeKind, &Source)>, HashSet<(&str, &str, EdgeKind)>) = Default::default();
        let (mut same, mut elsewhere, mut by_session): (BTreeMap<EdgeKind, usize>, BTreeMap<EdgeKind, usize>, BTreeMap<&str, usize>) = Default::default();
        // the exact duplicates no session's line made, each said (the first 20), with what made it
        let mut no_session: Vec<String> = Vec::new();
        for e in &g.edges {
            if !exact.insert((e.from.as_str(), e.to.as_str(), e.kind, &e.source)) {
                *same.entry(e.kind).or_default() += 1;
                if let Source::Session(s, _) = &e.source {
                    *by_session.entry(s.as_str()).or_default() += 1;
                } else if no_session.len() < 20 {
                    no_session.push(format!("{} -{:?}-> {} from {:?}", e.from, e.kind, e.to, e.source));
                }
            } else if !ends.insert((e.from.as_str(), e.to.as_str(), e.kind)) {
                *elsewhere.entry(e.kind).or_default() += 1;
            }
        }
        let say = |m: &BTreeMap<EdgeKind, usize>| m.iter().map(|(k, n)| format!("{k:?} {n}")).collect::<Vec<_>>().join(", ");
        let mut most: Vec<(&str, usize)> = by_session.into_iter().collect();
        most.sort_by(|a, b| b.1.cmp(&a.1));
        let most: Vec<String> = most.iter().take(5).map(|(s, n)| format!("{s} {n}")).collect();
        let kb = |key: &str| std::fs::read_to_string("/proc/self/status").ok().and_then(|s| s.lines().find(|l| l.starts_with(key)).and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())).unwrap_or(0);
        println!(
            "duplicate edges of {}: {} with an earlier edge's ends, kind and source ({}); {} more with an earlier edge's ends and kind from another source ({}); the sessions with the most: {}; VmHWM {} MB, VmRSS {} MB",
            g.edges.len(),
            same.values().sum::<usize>(),
            say(&same),
            elsewhere.values().sum::<usize>(),
            say(&elsewhere),
            most.join(", "),
            kb("VmHWM:") / 1024,
            kb("VmRSS:") / 1024,
        );
        println!("duplicate edges no session's line made: {}", if no_session.is_empty() { "none".to_string() } else { no_session.join(" · ") });
    }
    if stop == "side" {
        return memory_by_part(core, "after the sessions' side");
    }
    let m = &core.tickets.m;
    // his newest post as the window's `his_newest` reads the snapshot: on no handler's run ticket (`handling`, as the
    // machine's own lists leave them out), which the snapshot lists only under its ticket's handlers; an owner_post
    // handler's post is his words posted again
    let newest = m.tickets.iter().filter(|(_, k)| k.handling.is_none()).flat_map(|(t, k)| k.posts.iter().filter(|p| p.by == "owner").map(move |p| (p.at.clone(), *t, p.n))).max().map(|(_, t, n)| docket_machine::graph::post_id(m, t, n));
    println!("his newest post: {newest:?}");
    let asks: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(&asks).unwrap()).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    for a in asks {
        let name = a["name"].as_str().unwrap().to_string();
        let mut ask: Ask = serde_json::from_value(a["ask"].clone()).unwrap();
        if ask.around.as_deref() == Some("newest") {
            ask.around = newest.clone();
        }
        if ask.to.is_none() {
            ask.to = last_at.clone();
        }
        // "first": the first session of its agent's folder (the file view's stills of a session's requests, p1642, 3)
        if ask.session.as_deref() == Some("first") {
            let folder = core.graph_slice(&Ask { session: None, ..ask.clone() }).folder.unwrap_or_default();
            ask.session = folder.iter().find(|r| r.text.starts_with("sessions")).and_then(|r| r.rows.first()).map(|r| r.of.clone());
            println!("{name}: the first session of {:?}: {:?}", ask.folder, ask.session);
        }
        let asked = std::time::Instant::now();
        let mut answer = core.graph_slice(&ask);
        let took = asked.elapsed().as_millis();
        if let (true, Some(at)) = (ask.now, &last_at) {
            answer.now = Some(g.now(at));
        }
        let s = &answer.slice;
        let now = answer.now.as_ref().map(|n| format!("now at {}: {} agents, {} asks, not taken {:?}", n.at, n.agents.len(), n.asks.len(), n.not_taken)).unwrap_or_default();
        println!("{name}: in {took} ms, around {:?}, {} hops, layers {:?}: {} nodes, {} edges, {} left out, unknown {}; {now}", ask.around, ask.hops, ask.layers, s.nodes.len(), s.edges.len(), s.left, s.unknown);
        // his post's course, each point's time and what reached it (p1429, 1)
        if let Some(c) = &answer.course {
            let points: Vec<String> = c.points().iter().map(|(p, at, r)| format!("{p:?} {} {}", at.map_or("-", |a| a.as_str()), r.map_or("", |r| r.id.as_str()))).collect();
            println!("{name}: course of {}: {}", c.post, points.join(" · "));
        }
        match (&answer.folder, &ask.session) {
            (Some(f), Some(session)) => println!("{name}: requests of {session}: {}", f.len()),
            (Some(f), None) => println!("{name}: folder of {:?}: {}", ask.folder, f.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join(" · ")),
            _ => {}
        }
        std::fs::write(Path::new(&out).join(format!("{name}.json")), serde_json::to_string(&serde_json::json!({"ask": ask, "answer": answer})).unwrap()).unwrap();
    }
    memory("after the reads");
    memory_by_part(core, "after the reads");
}

/// This process's resident and peak memory (/proc/self/status), in MB, for `memory`.
fn status_mb(key: &str) -> u64 {
    std::fs::read_to_string("/proc/self/status").ok().and_then(|s| s.lines().find(|l| l.starts_with(key)).and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())).unwrap_or(0) / 1024
}

/// glibc's count of the heap in use (`mallinfo2`: every arena's, with the blocks mapped alone), in bytes.
fn heap_in_use() -> i64 {
    let m = unsafe { libc::mallinfo2() };
    (m.uordblks + m.hblkhd) as i64
}

/// This process's memory at WHEN (paper-research N-1446 p1900): resident and peak, glibc's heap in use, and the
/// allocator's part of coordd's counts line (`counts::allocator`: its arenas, what they hold from the system, use and
/// keep free, and what lies in blocks mapped alone).
fn memory(when: &str) {
    let arenas = crate::counts::malloc_info_now().map(|x| crate::counts::allocator(&x)).unwrap_or_default();
    println!("memory {when}: resident {} MB, peak {} MB; heap in use {} MB{arenas}", status_mb("VmRSS:"), status_mb("VmHWM:"), heap_in_use() >> 20);
}

/// What each part of coordd's state alone holds of the heap at WHEN (paper-research N-1446 p1900): glibc's heap in use
/// before and after each part is emptied in turn, so each part's figure is what it alone holds and nothing is counted
/// twice. The machine's tickets by their parts, then the rest of the machine (its defs and caches of views); the kept
/// graph's edges, its nodes, and the rest of it; the agents' side (`KeptSide`); the texts' places; git's side; the
/// rest of the tickets' state; the core's mails, the requests its sessions sent (`Sent`) and its events; and the rest
/// of the core. Then what is left in use, and the resident size before and after `malloc_trim` hands back what the
/// arenas keep free.
fn memory_by_part(mut core: crate::core::Core, when: &str) {
    fn each(core: &mut crate::core::Core, f: impl FnMut(&mut docket_machine::Ticket)) {
        core.tickets.m.tickets.edit().values_mut().for_each(f);
    }
    let (tickets, nodes, edges) = (core.tickets.m.tickets.len(), core.tickets.graph.graph.nodes.len(), core.tickets.graph.graph.edges.len());
    // by kind, as JSON bytes (a count of what each holds, not the heap's): the tickets, and their forms, contexts,
    // traces' programs, traces' visits and handlers; the eight kinds holding most
    fn len(v: &impl serde::Serialize) -> usize {
        serde_json::to_vec(v).map_or(0, |b| b.len())
    }
    let mut by_kind: std::collections::BTreeMap<&str, [usize; 6]> = Default::default();
    for t in core.tickets.m.tickets.values() {
        let k = by_kind.entry(t.kind.as_str()).or_default();
        let parts: Vec<_> = t.trace.parts.iter().map(|p| &p.prog).collect();
        let visits: Vec<_> = t.trace.visits.iter().collect();
        for (i, n) in [1, len(&t.form), len(&t.ctx), len(&parts), len(&visits), len(&t.handlers)].into_iter().enumerate() {
            k[i] += n;
        }
    }
    let mut kinds: Vec<(&str, [usize; 6])> = by_kind.into_iter().collect();
    kinds.sort_by_key(|(_, b)| std::cmp::Reverse(b[1..].iter().sum::<usize>()));
    let mb = |b: usize| b >> 20;
    let all: [usize; 6] = kinds.iter().fold([0; 6], |mut s, (_, b)| { s.iter_mut().zip(b).for_each(|(s, b)| *s += b); s });
    let say = |(k, b): &(&str, [usize; 6])| format!("{k} {} tickets, forms {} MB, contexts {} MB, traces' programs {} MB, visits {} MB, handlers {} MB", b[0], mb(b[1]), mb(b[2]), mb(b[3]), mb(b[4]), mb(b[5]));
    println!("tickets by kind {when}, as JSON: all {}; {} kinds, the most: {}", say(&("", all)), kinds.len(), kinds.iter().take(8).map(say).collect::<Vec<_>>().join(" · "));
    let total = heap_in_use();
    let (mut was, mut parts) = (total, Vec::<(&str, i64)>::new());
    let mut emptied = |what: &'static str| {
        let now = heap_in_use();
        parts.push((what, was - now));
        was = now;
    };
    each(&mut core, |t| (t.program, t.calls) = Default::default());
    emptied("the tickets' programs and calls, as each was opened with them");
    each(&mut core, |t| t.k = Vec::new());
    emptied("their frames");
    each(&mut core, |t| t.posts = Vec::new());
    emptied("their posts");
    each(&mut core, |t| t.given = Vec::new());
    emptied("their givens");
    each(&mut core, |t| t.form = Default::default());
    emptied("their forms");
    each(&mut core, |t| t.ctx = Default::default());
    emptied("their contexts");
    each(&mut core, |t| t.trace.parts = Vec::new());
    emptied("their traces' programs (`Trace.parts`, the kind's program first)");
    each(&mut core, |t| t.trace.visits = Default::default());
    emptied("their traces' visits");
    each(&mut core, |t| t.handlers = Vec::new());
    emptied("their kinds' `when` handlers, as each was opened with them");
    each(&mut core, |t| t.status = docket_machine::Status::Done(serde_json::Value::Null));
    emptied("their statuses (what each owes, waits on or gave back)");
    each(&mut core, |t| (t.last_node, t.layer_nodes) = Default::default());
    emptied("their last nodes, with their envs");
    each(&mut core, |t| (t.waiting, t.shaping) = Default::default());
    emptied("what each waits on and shapes, with their envs");
    each(&mut core, |t| (t.gives, t.answers, t.shaped) = Default::default());
    emptied("their gifts, answers and shaped holes");
    core.tickets.m.tickets.edit().clear();
    emptied("the rest of the tickets");
    core.tickets.m = Machine::load(Default::default());
    emptied("the rest of the machine (its defs: kinds, programs, calls; its caches of views)");
    core.tickets.graph.graph.edges = Vec::new();
    emptied("the kept graph's edges");
    core.tickets.graph.graph.nodes = Vec::new();
    emptied("its nodes");
    core.tickets.graph = Default::default();
    emptied("the rest of the graph (its index of nodes by id, jobs, release acts, the builder's own)");
    core.tickets.side = Default::default();
    emptied("the agents' side (KeptSide: its sessions' readers and what each put)");
    *core.tickets.places.write().unwrap() = Default::default();
    emptied("the texts' places");
    core.tickets.graph_git = Default::default();
    emptied("git's side");
    core.tickets = Default::default();
    emptied("the rest of the tickets' state");
    (core.mails, core.ix) = Default::default();
    emptied("the core's mails and their indexes");
    core.sent = Default::default();
    emptied("the requests the sessions sent (`Sent`)");
    (core.ring, core.latest, core.sent_posts) = Default::default();
    emptied("the events kept for watchers");
    drop(core);
    emptied("the rest of the core");
    let (left, resident) = (heap_in_use(), status_mb("VmRSS:"));
    unsafe { libc::malloc_trim(0) };
    let mb = |b: i64| b >> 20;
    let mut most = parts.clone();
    most.sort_by(|a, b| b.1.cmp(&a.1));
    let say: Vec<String> = parts.iter().map(|(w, b)| format!("{w} {} MB ({:.1}%)", mb(*b), 100.0 * *b as f64 / total.max(1) as f64)).collect();
    println!("memory by part {when} ({tickets} tickets, {nodes} nodes, {edges} edges): heap in use {} MB; {}; left in use after them {} MB; the most: {} {} MB; resident {resident} MB, {} MB after malloc_trim handed back what the arenas kept free", mb(total), say.join("; "), mb(left), most[0].0, mb(most[0].1), status_mb("VmRSS:"));
}

/// The repositories the kept graph reads on a copy of a coord's record (paper-research N-1446 p1455; ignored, run
/// behind the replay lock): REPLAY_COORD, REPLAY_BASE and REPLAY_PROJECT as `a_coords_record_graph_costs` reads them,
/// loaded as coordd's start loads it (`Tickets::load`), then git read as the side's clock reads it the first time,
/// with the runner's job files in GRAPH_JOBS sent from JOBS_COORD, HOME the owner's; the agents here the graph's (a
/// copy has no registry). Printed: the folders the record names and where they sit, each repository read with its
/// commits and moves, whether it counts (p1476) and by how many commits of agents here and of jobs, the folders gone,
/// the kept graph's commits, and beside them what a read of GRAPH_REPOS (`NAME=DIR,…`, as docket-edge5's run read
/// them, with the reflogs) gives the same graph: its commits, those of them the kept graph lacks, and the kept graph's
/// others by repository. Paths and counts only.
#[test]
#[ignore]
fn a_coords_record_repos() {
    use crate::ticket::work_repos::Placed;
    use docket_machine::graph::NodeKind;
    use std::collections::{BTreeMap, HashMap};
    let (Ok(coord), Ok(base)) = (std::env::var("REPLAY_COORD"), std::env::var("REPLAY_BASE")) else { return };
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    let started = std::time::Instant::now();
    let mut t = crate::ticket::Tickets::load(&coord, &base, std::env::var("REPLAY_PROJECT").ok().as_deref(), 1);
    let loaded = started.elapsed().as_secs();
    let commits = |g: &Graph| -> BTreeSet<String> { g.nodes.iter().filter(|n| n.kind == NodeKind::Commit).map(|n| n.id.clone()).collect() };
    let record = commits(&t.graph.graph);
    let jobs = crate::jobs::of_coord(Path::new(&var("GRAPH_JOBS")), &var("JOBS_COORD"));
    let mut of_jobs = Graph::default();
    docket_machine::graph_git::add(&mut of_jobs, &[], &jobs);
    let project = crate::project::Project::load(&coord).ok().flatten().map(|p| p.repos()).unwrap_or_default();
    let started = std::time::Instant::now();
    let crate::ticket::work_repos::GitRead { placed, read, .. } = t.graph_git.reading(project.clone()).read(&jobs, &var("HOME"));
    let reading = started.elapsed().as_secs_f64();
    // as the side's clock puts them in (`Core::graph_git_put`): the places, the jobs' commits, git's read, the jobs
    let agents: BTreeSet<String> = t.graph.graph.nodes.iter().filter_map(|n| n.id.strip_prefix("agent:").map(String::from)).collect();
    t.graph_git.keep_placed(placed.clone());
    t.graph_git.take_jobs(&jobs);
    use docket_machine::graph_git::commit_id;
    let ids = |read: &[crate::ticket::work_repos::RepoRead]| -> BTreeMap<String, BTreeSet<String>> { read.iter().map(|r| (r.dir.clone(), r.commits.iter().map(|c| &c.hash).chain(r.moves.iter().map(|m| &m.hash)).map(|h| commit_id(h)).collect())).collect() };
    let found = ids(&read);
    let here: BTreeSet<String> = t.graph_git.names(&read).into_iter().filter(|a| agents.contains(a)).collect();
    let by_agent = |c: &docket_machine::graph_git::Commit| c.agent.split(',').any(|a| here.contains(a.trim()));
    let how: BTreeMap<String, (usize, usize, usize, usize)> = read.iter().map(|r| (r.dir.clone(), (r.commits.len(), r.moves.len(), r.commits.iter().filter(|c| by_agent(c)).count(), r.commits.iter().filter(|c| !by_agent(c) && t.graph_git.counts(c, &here)).count()))).collect();
    let reflog_only: usize = read.iter().map(|r| r.moves.iter().map(|m| commit_id(&m.hash)).collect::<BTreeSet<_>>().difference(&r.commits.iter().map(|c| commit_id(&c.hash)).collect()).count()).sum();
    let dirs: BTreeSet<String> = project.iter().map(|(_, d)| d.clone()).collect();
    let started = std::time::Instant::now();
    t.graph_git_add(read, &dirs, &here);
    t.graph_jobs_add(&jobs);
    let putting = started.elapsed().as_secs_f64();
    let repos = t.graph_git.repos(&project);
    let kept = commits(&t.graph.graph);
    let (counted, left) = t.graph_git.counted_and_left();
    let (counted, left): (BTreeSet<String>, BTreeSet<String>) = (counted.into_iter().cloned().collect(), left.into_iter().cloned().collect());
    let ons = t.graph.graph.edges.iter().filter(|e| e.kind == docket_machine::graph::EdgeKind::On).count();
    println!("repos of {coord}: loaded in {loaded} s; {} jobs from before the record of {}; the project's repositories: {}; {} agents in the graph, {} of them named by commits read; first read (placing, logs and reflogs) {reading:.2} s, put in {putting:.2} s; {reflog_only} commits read only a reflog reaches; {ons} `on` edges", jobs.len(), var("JOBS_COORD"), project.len(), agents.len(), here.len());
    let (mut ins, mut nowhere, mut gone) = (BTreeMap::<String, usize>::new(), 0, 0);
    for (_, p) in &placed {
        match p {
            Placed::In { common, .. } => *ins.entry(common.clone()).or_default() += 1,
            Placed::Nowhere => nowhere += 1,
            Placed::Gone(_) => gone += 1,
        }
    }
    println!("folders placed: {} ({} in a repository, {nowhere} in none or never read, {gone} gone)", placed.len(), ins.values().sum::<usize>());
    println!("repositories: {} read, {} counted, {} left out", how.len(), counted.len(), left.len());
    for (name, dir) in &repos {
        let common = ins.keys().find(|c| c.strip_suffix("/.git").unwrap_or(c) == dir.as_str());
        let (c, m, a, j) = how.get(dir).copied().unwrap_or_default();
        let is = if dirs.contains(dir) { "counted, the project's" } else if counted.contains(dir) { "counted" } else if left.contains(dir) { "LEFT OUT" } else { "not read" };
        println!("  repository {name} = {dir}: {} folders; {c} commits, {m} moves read; {a} by agents here, {j} more a job's; {is}", common.map_or(0, |c| ins[c]));
    }
    let g = t.graph_git.gone();
    println!("gone: {}", g.len());
    for p in g.iter().take(60) {
        println!("  gone {p}");
    }
    println!("kept graph: {} commits ({} from the record's lines, {} from the jobs before it, {} from git's counted repositories)", kept.len(), record.len(), commits(&of_jobs).len(), found.iter().filter(|(d, _)| counted.contains(*d)).flat_map(|(_, c)| c).collect::<BTreeSet<_>>().len());
    let given: Vec<(String, String)> = var("GRAPH_REPOS").split(',').filter_map(|r| r.split_once('=')).map(|(n, d)| (n.to_string(), d.to_string())).collect();
    if given.is_empty() {
        return;
    }
    let theirs = ids(&crate::ticket::git_read(&given, &HashMap::new()));
    let run: BTreeSet<String> = record.iter().chain(commits(&of_jobs).iter()).chain(theirs.values().flatten()).cloned().collect();
    println!("GRAPH_REPOS {}: {} commits read with the reflogs; that run's graph: {} commits; of them not in the kept graph: {}", given.iter().map(|(n, d)| format!("{n}={d}")).collect::<Vec<_>>().join(","), theirs.values().flatten().collect::<BTreeSet<_>>().len(), run.len(), run.difference(&kept).count());
    let more: BTreeSet<&String> = kept.difference(&run).collect();
    println!("the kept graph's others: {}", more.len());
    for (dir, c) in found.iter().filter(|(d, _)| counted.contains(*d)) {
        println!("  of {dir}: {}", c.iter().filter(|c| more.contains(c)).count());
    }
}

/// One round of the agents' side as coordd's clock reads it (`read_sides`), here on the test's thread: each session put
/// on its own (`SideRound::each`, `put_side`), in parts of three edges, each with the index caught up
/// (`Core::side_put_part`; coordd's are `follow::SIDE_PART`), so each leg of the side has the parts make what one put
/// makes.
fn read_side(core: &mut crate::core::Core) {
    for mut one in core.side_round().read().each() {
        while !core.side_put_part(&mut one, 3) {}
    }
}

/// CORE's kept graph, its `named` edges aside, is the record file's at DIR with `follow::side_into` over its lines and
/// the session files they name (w2's), at STEP of the test.
fn same_side(core: &crate::core::Core, dir: &Path, step: &str) {
    let lines: Vec<Line> = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let mut made = of_record(core, dir);
    let unopened = crate::follow::side_into(&mut made, &lines, &|a| a == "w2", &|t, n| docket_machine::graph::post_id(&core.tickets.m, t, n));
    assert!(unopened.is_empty(), "{step}: {unopened:?}");
    let mut kept = core.tickets.graph.graph.clone();
    kept.edges.retain(|e| e.kind != EdgeKind::Named);
    eprintln!("the side at step {step}");
    same(&kept, &made);
}

/// The agents' side in the kept graph (paper-research N-1446 p1458): each session a `given` names is read on from where
/// it left off, off the lock, in rounds (`Core::side_round`, `follow::SideRound::read`, `Core::side_put`), and after
/// each live input and its round the kept graph is the record file's with `follow::side_into` over the same lines and
/// files. On the way: a line still being written waits for the next round; a file read before it was loaded keeps its
/// read's line, the earlier; a request's tokens follow its response's latest line; a shape written after its request
/// is carried; a `given` recorded after its line was read begins its step where one read would, and the requests from
/// there are for its post, or with no request after it yet, the next round's; each request is in the first of w2's
/// turns that ended at or after it, to the second (one in the second its first turn ended is in it, one after the last
/// in none yet), and ran with the text its memory file stood as before it; a load's next version and a system block's
/// carry the words that changed; a round out across a reload puts nothing; a restart reads it whole again, and the
/// window's ask around his post then reaches the request it went in with and the system prompt block that request
/// carried, a text said by its size alone (still 3, paper-research N-1446 p1677, p1684).
#[tokio::test]
async fn the_kept_graphs_side_is_side_into_over_the_same_files_after_each_live_input() {
    use crate::follow::tests::typed;
    use docket_machine::graph::NodeKind;
    use serde_json::json;
    let (dir, base, mut core, _rx) = coord("graph-side", "idle");
    let a = open(&mut core, "lead", &["w2"], "the fixture is in place");
    let t = t_of(&a);
    let f = dir.join("w2-session.jsonl");
    let file = f.display().to_string();
    // a piece of the session as Claude Code writes it: where it starts
    let write = |text: &str| {
        use std::io::Write;
        let at = std::fs::metadata(&f).map(|m| m.len()).unwrap_or(0);
        std::fs::OpenOptions::new().create(true).append(true).open(&f).unwrap().write_all(text.as_bytes()).unwrap();
        at
    };
    let shape = |h: &str, second: &str| typed("api-request-shape", json!({"shapeHash": h, "shape": {"system": [{"type": "text", "text": "You are Claude Code."}, {"type": "text", "text": second}], "tools": [{"name": "Read"}]}})) + "\n";
    let request = |id: &str, at: &str, shape: &str| typed("api-request", json!({"id": id, "timestamp": at, "params": {"model": "claude-opus-5-5", "output_config": {"effort": "xhigh"}}, "shapeHash": shape, "tail": []})) + "\n";
    let response = |r: &str, output: u64, at: &str| json!({"type": "assistant", "requestRef": r, "message": {"role": "assistant", "content": [{"type": "text", "text": "ok"}], "usage": {"input_tokens": 3, "output_tokens": output, "cache_read_input_tokens": 900, "cache_creation_input_tokens": 60}}, "timestamp": at}).to_string() + "\n";
    let user = |text: &str, at: &str| json!({"type": "user", "message": {"role": "user", "content": text}, "timestamp": at}).to_string() + "\n";
    let loads = |text: &str, at: &str| json!({"type": "attachment", "attachment": {"type": "instructions", "files": [{"path": "/m/MEMORY.md", "type": "AutoMem", "content": text}]}, "timestamp": at}).to_string() + "\n";
    let reads = |at: &str| json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/m/MEMORY.md"}}]}, "timestamp": at}).to_string() + "\n";
    let given = |core: &mut crate::core::Core, offset: u64| {
        let d = core.tickets.due("w2").remove(0);
        core.ticket_input(Input::Given { t: d.t, to: "w2".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(docket_machine::Loc { file: file.clone(), offset, len: 10 }) }).unwrap();
    };
    // 1. the session's first lines: a read of the memory file, the lead's post going in, a request with it
    write(&shape("s1", "standing, first"));
    write(&reads("2026-11-22T09:00:01Z"));
    let first = write(&user("the fixture is in place", "2026-11-22T09:00:02Z"));
    write(&request("r1", "2026-11-22T09:00:03Z", "s1"));
    write(&response("r1", 5, "2026-11-22T09:00:04Z"));
    given(&mut core, first);
    read_side(&mut core);
    same_side(&core, &dir, "1, the first round");
    // 2. the response's next line; the memory file loaded, and loaded again in a line still being written
    write(&response("r1", 7, "2026-11-22T09:00:05Z"));
    write(&loads("- one", "2026-11-22T09:01:00Z"));
    let whole = loads("- one\n- two", "2026-11-22T09:02:00Z");
    let (half, rest) = whole.split_at(whole.len() / 2);
    write(half);
    read_side(&mut core);
    same_side(&core, &dir, "2, half a line");
    // 3. his post to w2 goes in with a request whose shape comes after it, read before w2's `given` is recorded
    write(rest);
    core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("@w2 and the second".into()), names: vec!["w2".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let his = format!("{}.p{}", core.tickets.m.id(t), core.tickets.m.tickets[&t].last());
    let second = write(&user("@w2 and the second", "2026-11-22T09:02:01Z"));
    write(&request("r2", "2026-11-22T09:02:02Z", "s2"));
    write(&response("r2", 9, "2026-11-22T09:02:03Z"));
    read_side(&mut core);
    same_side(&core, &dir, "3, a request before its shape and its given");
    // 4. the shape, and the given
    write(&shape("s2", "standing, second"));
    given(&mut core, second);
    read_side(&mut core);
    same_side(&core, &dir, "4, a given after its line was read");
    let kept = &core.tickets.graph.graph;
    let mut fors: Vec<(&str, &str)> = kept.edges.iter().filter(|e| e.kind == EdgeKind::For).map(|e| (e.from.as_str(), e.to.as_str())).collect();
    fors.sort();
    assert_eq!(fors, [("request:r1", a.as_str()), ("request:r2", his.as_str())], "r2 is for his post, not the lead's");
    let stem = f.file_stem().unwrap().to_str().unwrap().to_string();
    assert_eq!(kept.get("file:/m/MEMORY.md").map(|n| n.source.clone()), Some(Source::Session(stem, 2)), "the read's line, before the load's");
    assert_eq!(kept.get("request:r1").and_then(|n| n.tokens).map(|t| t.output), Some(7));
    assert!(kept.edges.iter().any(|e| e.from == "request:r2" && e.to == crate::follow::text_id("standing, second") && e.kind == EdgeKind::Carried), "r2 carried its shape's block");
    assert!(kept.get(&crate::follow::text_id("- one\n- two")).is_some(), "the line once it was whole");
    // 5. his third post goes in and is read before its `given` is recorded, with no request after it yet: it waits
    //    there for the next round's
    core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("@w2 and the third".into()), names: vec!["w2".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let his_third = format!("{}.p{}", core.tickets.m.id(t), core.tickets.m.tickets[&t].last());
    let third = write(&user("@w2 and the third", "2026-11-22T09:03:01Z"));
    read_side(&mut core);
    given(&mut core, third);
    read_side(&mut core);
    same_side(&core, &dir, "5, a given after its line was read, with no request after it yet");
    write(&request("r3", "2026-11-22T09:03:02Z", "s2"));
    read_side(&mut core);
    same_side(&core, &dir, "5, the request after it");
    assert!(core.tickets.graph.graph.edges.iter().any(|e| e.kind == EdgeKind::For && e.from == "request:r3" && e.to == his_third), "r3 is for his third post");
    // 6. w2's turn ends twice, at the record's own times: the requests read before are in the first; one written in the
    //    second the first ended is in it, one after the last in none yet
    //    (each names w2's last delivery, as a turn that read one does: the machine keeps it, whatever it answers)
    let lines: Vec<Line> = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let delivery = lines.iter().rev().find_map(|l| match l {
        Line::Given { to, delivery, .. } if to == "w2" => Some(delivery.clone()),
        _ => None,
    });
    let turn_end = |core: &mut crate::core::Core| {
        let _ = core.ticket_input(Input::TurnEnded { agent: "w2".into(), delivery: delivery.clone(), text: docket_machine::Loc { file: file.clone(), offset: 0, len: 0 }, later: vec![], stood: None, go: None });
    };
    turn_end(&mut core);
    turn_end(&mut core);
    let mut turns: Vec<(usize, String, String)> = core.tickets.graph.graph.nodes.iter().filter(|n| n.kind == NodeKind::Turn && n.id.starts_with("turn:w2@")).map(|n| (n.line().unwrap(), n.id.clone(), n.when.clone().unwrap())).collect();
    turns.sort();
    assert_eq!(turns.len(), 2, "{turns:?}");
    let (first_end, ended_at) = (turns[0].1.clone(), turns[0].2.clone());
    write(&request("r4", &format!("{}.900Z", &ended_at[..19]), "s2"));
    write(&request("r5", "2999-01-01T00:00:00.000Z", "s2"));
    read_side(&mut core);
    same_side(&core, &dir, "6, the turns' ends");
    let kept = &core.tickets.graph.graph;
    let edges = |k: EdgeKind| {
        let mut v: Vec<(String, String, String)> = kept.edges.iter().filter(|e| e.kind == k).map(|e| (e.from.clone(), e.to.clone(), e.what.clone())).collect();
        v.sort();
        v
    };
    let w = |a: &str, b: &str, what: &str| (a.to_string(), b.to_string(), what.to_string());
    let rs = |to: &str, ids: &[&str]| ids.iter().map(|r| w(&format!("request:{r}"), to, "")).collect::<Vec<_>>();
    assert_eq!(edges(EdgeKind::SentIn), rs(&first_end, &["r1", "r2", "r3", "r4"]), "r1 to r3 in the first, r4 in its second, r5 in none");
    let two = crate::follow::text_id("- one\n- two");
    assert_eq!(edges(EdgeKind::RanWith), rs(&two, &["r2", "r3", "r4", "r5"]), "r1 came before the first load");
    let id = crate::follow::text_id;
    let mut became = vec![w(&id("- one"), &two, "2 words added: - one {+- two+}"), w(&id("standing, first"), &id("standing, second"), "1 word removed, 1 added: standing, [-first-] {+second+}")];
    became.sort();
    assert_eq!(edges(EdgeKind::Became), became, "the memory file's next version, and r2's system block");
    // 7. w2's folder, as `Request::Graph` answers an ask that names it (the file view, paper-research N-1446 p1441):
    //    alone, the kept graph's, its memory file's two versions and r2's system block's with the words that changed
    let answer = core.graph_slice(&Ask { folder: Some("w2".into()), ..Ask::default() });
    let kept = &core.tickets.graph.graph;
    assert_eq!((answer.folder.as_ref(), answer.slice.edges.len(), answer.labels.len()), (Some(&kept.folder("w2")), 0, 0), "the folder alone");
    // its system prompt, as an ask that names it is answered (label.rs, `Ask::prompt`; fix 4): alone too, the kept
    // graph's, and with a version that version's requests
    let prompt = core.graph_slice(&Ask { prompt: Some("w2".into()), ..Ask::default() });
    assert_eq!((prompt.folder.as_ref(), prompt.slice.edges.len()), (Some(&kept.prompt("w2", None)), 0), "its system prompt alone");
    let heads: Vec<String> = prompt.folder.unwrap_or_default().iter().map(|r| r.text.chars().take_while(|c| *c != ':').collect()).collect();
    assert_eq!(heads, ["started with", "Claude Code's own"]);
    let first = crate::follow::text_id("standing, first");
    let version = core.graph_slice(&Ask { prompt: Some("w2".into()), version: Some(first.clone()), ..Ask::default() });
    assert_eq!(version.folder, Some(kept.prompt("w2", Some(&first))), "a version's requests");
    assert!(!kept.prompt("w2", Some(&first)).is_empty(), "r1 carried it");
    let folder = answer.folder.unwrap_or_default();
    let heads: Vec<&str> = folder.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(heads, ["kind and brief: no start line of it yet, so its sessions' briefs", "loaded texts: 1 file, 2 versions", "read: 1 file, 1 read", "sessions: 1, with 5 requests", "commits and jobs: 0 commits, 0 jobs"]);
    fn words(rows: &[Row], out: &mut Vec<(String, String)>) {
        for r in rows {
            if !r.words.is_empty() {
                out.push((r.text.clone(), r.words.clone()));
            }
            words(&r.rows, out);
        }
    }
    let mut said = Vec::new();
    words(&folder, &mut said);
    let w = |a: &str, b: &str| (a.to_string(), b.to_string());
    assert_eq!(
        said,
        [
            w("version 2: 4 requests ran with it; 2 words added", "2 words added: - one {+- two+}"),
            w("version 2: carried by 4 requests; 1 word removed, 1 added", "1 word removed, 1 added: standing, [-first-] {+second+}"),
        ]
    );
    // a round out across a reload of the record puts nothing in the new graph; the next reads it whole
    let out = core.side_round();
    core.load_tickets();
    core.side_put(out.read());
    assert!(core.tickets.graph.graph.nodes.iter().all(|n| n.kind != NodeKind::Session), "the round from before the reload");
    read_side(&mut core);
    same_side(&core, &dir, "after a reload");
    // a restart reads it whole again
    drop(core);
    let (mut again, _rx2) = load(&dir, &base, "idle");
    read_side(&mut again);
    same_side(&again, &dir, "after a restart");
    // 8. the window's ask around his post on the restarted coordd, as the answers' writer asks it of his record (still
    //    3, p1677, p1684): two hops reach the request his post went in with and the system prompt block it carried,
    //    and the block is said by its size, with none of its words
    let answer = again.graph_slice(&Ask { around: Some(his.clone()), hops: 2, ..Ask::default() });
    let reached: BTreeSet<(&str, EdgeKind, &str)> = answer.slice.edges.iter().map(|e| (e.edge.from.as_str(), e.edge.kind, e.edge.to.as_str())).collect();
    let system = crate::follow::text_id("standing, second");
    assert!(reached.contains(&("request:r2", EdgeKind::For, his.as_str())), "8, the request his post went in with: {reached:?}");
    assert!(reached.contains(&("request:r2", EdgeKind::Carried, system.as_str())), "8, the system prompt block it carried: {reached:?}");
    let block = answer.slice.nodes.iter().find(|n| n.node.id == system).map(|n| (n.node.kind, n.node.what.as_str()));
    assert_eq!(block, Some((NodeKind::Text, "a system prompt block, 16 bytes")), "8, the block by its size, none of its words");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A system prompt block's text on his click (paper-research N-1446 p1642, 2): `Request::Raw` of its text node finds,
/// under the lock, the request the graph says carried it and where that request's shape's line starts in its session's
/// file (`Core::ticket_block`), and reads that line alone off it (`ticket::block_words`): for the owner only and a
/// carried text only, served only while the line there is that shape's and one of its blocks is the node; and the
/// session's requests alone (`Ask::session`, p1642, 3). A file the request ran with (his memory under `.claude`,
/// paper-research N-1446 p1844) is answered from the session or refused, and never read at its path. Made up: w2 given
/// the lead's post, its session a shape, the post going in, a load of a memory file and a request.
#[tokio::test]
async fn a_blocks_text_is_served_to_the_owner_from_the_shape_line_of_the_request_that_carried_it() {
    use crate::core::Who;
    use crate::follow::{tests::typed, text_id};
    use serde_json::json;
    let (dir, _base, mut core, _rx) = coord("graph-block", "idle");
    open(&mut core, "lead", &["w2"], "the fixture is in place");
    let f = dir.join("w2-session.jsonl");
    // the memory file as Claude Code loads it, under `.claude`: on disk with other words than the session holds, so an
    // answer read at its path would show them
    let mem = dir.join(".claude/projects/x/memory/MEMORY.md");
    std::fs::create_dir_all(mem.parent().unwrap()).unwrap();
    std::fs::write(&mem, "on disk, never served").unwrap();
    let shape = |h: &str, second: &str| typed("api-request-shape", json!({"shapeHash": h, "shape": {"system": [{"type": "text", "text": "You are Claude Code."}, {"type": "text", "text": second}], "tools": []}})) + "\n";
    let rest = [
        json!({"type": "user", "message": {"role": "user", "content": "the fixture is in place"}, "timestamp": "2026-11-22T09:00:02Z"}).to_string() + "\n",
        json!({"type": "attachment", "attachment": {"type": "instructions", "files": [{"path": mem.display().to_string(), "type": "AutoMem", "content": "- one"}]}, "timestamp": "2026-11-22T09:00:02Z"}).to_string() + "\n",
        typed("api-request", json!({"id": "r1", "timestamp": "2026-11-22T09:00:03Z", "params": {"model": "claude-opus-5-5"}, "shapeHash": "s1", "tail": []})) + "\n",
    ]
    .concat();
    let first = shape("s1", "standing, first");
    std::fs::write(&f, first.clone() + &rest).unwrap();
    let d = core.tickets.due("w2").remove(0);
    core.ticket_input(Input::Given { t: d.t, to: "w2".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(docket_machine::Loc { file: f.display().to_string(), offset: first.len() as u64, len: 10 }) }).unwrap();
    read_side(&mut core);
    // the session's requests alone, as an ask with its session is answered (label.rs, `Ask::session`; p1642, 3)
    let stem = f.file_stem().unwrap().to_str().unwrap().to_string();
    let asked = |session: Option<String>| core.graph_slice(&Ask { folder: Some("w2".into()), session, ..Default::default() }).folder.unwrap_or_default();
    assert_eq!(asked(Some(format!("session:{stem}"))).iter().map(|r| r.of.as_str()).collect::<Vec<_>>(), ["request:r1"], "w2's session's requests alone");
    assert_eq!(asked(None).len(), 5, "w2's folder, its five groups");
    let block = text_id("standing, first");
    let served = |core: &crate::core::Core, w: &Who, id: &str| core.ticket_block(w, id).and_then(|s| crate::ticket::block_words(&s));
    assert_eq!(served(&core, &Who::Owner, &block), Ok("standing, first".to_string()));
    assert_eq!(served(&core, &Who::Owner, &text_id("You are Claude Code.")), Ok("You are Claude Code.".to_string()), "each block of the shape");
    assert_eq!(served(&core, &Who::Agent("w2".into()), &block), Err("a session's words are the owner's, read in his window".to_string()));
    assert_eq!(served(&core, &Who::Owner, "request:r1"), Err("request:r1 is no text of the record's graph".to_string()));
    let (unknown, loaded) = (text_id("never written"), text_id("- one"));
    assert_eq!(served(&core, &Who::Owner, &unknown), Err(format!("{unknown} is no text of the record's graph")));
    assert_eq!(served(&core, &Who::Owner, &loaded), Err(format!("no request carried {loaded} and no start gave it: only a system prompt's text is served")), "a load's text is not a block's");
    // that load: a text the request ran with, a version of the file under `.claude` (the cloud's "ran with" card, p1844)
    let g = &core.tickets.graph.graph;
    assert!(g.edges.iter().any(|e| e.kind == EdgeKind::RanWith && e.from == "request:r1" && e.to == loaded), "the request ran with it");
    assert!(g.edges.iter().any(|e| e.kind == EdgeKind::VersionOf && e.from == loaded && e.to == format!("file:{}", mem.display())), "a version of the file under .claude");
    // coordd's own path for the request
    let shared = crate::Shared::new(core);
    assert_eq!(crate::raw_off_lock(&shared, &Who::Owner, &block).await, crate::core::ok("standing, first".into()));
    // the file it ran with, by coordd's own path: refused from the graph, its file never read
    let refused = crate::raw_off_lock(&shared, &Who::Owner, &loaded).await;
    assert_eq!(refused, crate::core::err(format!("coordd: no request carried {loaded} and no start gave it: only a system prompt's text is served")));
    assert!(!format!("{refused:?}").contains("on disk"), "not read at its path: {refused:?}");
    // the file rewritten under it: at the shape's place another shape's line, then the shape with that block changed
    std::fs::write(&f, shape("s9", "standing, first") + &rest).unwrap();
    assert_eq!(crate::raw_off_lock(&shared, &Who::Owner, &block).await, crate::core::err("coordd: the line at byte 0 of its session's file is another shape's".into()));
    std::fs::write(&f, shape("s1", "standing, fixed") + &rest).unwrap();
    assert_eq!(served(&*shared.lock().await, &Who::Owner, &block), Err(format!("no block of that shape is {block}")));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A text a start gave Claude Code (fix 4, docket-edge6 N-1446 p2529; his N-1104 p996: "be able to navigate to its
/// system prompt"): Claude Code joins it onto its own last block after a blank line, so no request carries it as a
/// block. `Request::Raw` of its node (`started_with`, from the start's `system_sha`) finds a request sent under that
/// start (`Graph::prompt`) and serves its shape's last block's part after a blank line that hashes to it, to the owner
/// only; refused while no request was sent under it, and where no request's last block ends with it. The standing text
/// is many agents' (accounts-lead N-1446 p2587): the first agent whose requests under it the graph holds, past one that
/// sent none; and each shape of those requests once, the most used first (a subagent's runs with its own, p2587, 2).
/// Made up: w3 started with that text and sending nothing, then w2 with its version 1, given the lead's post, its
/// session a subagent's shape and a shape whose last block ends with that text, and two subagent requests and one of
/// its own a minute after the start.
#[tokio::test]
async fn a_text_a_start_gave_is_served_from_the_last_block_of_a_request_sent_under_it() {
    use crate::core::Who;
    use crate::follow::{tests::typed, text_id};
    use serde_json::json;
    use sha2::{Digest, Sha256};
    let (dir, _base, mut core, _rx) = coord("graph-started-text", "idle");
    let began = |core: &mut crate::core::Core, agent: &str, n: &str, text: &str| {
        let sha: String = Sha256::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
        let more = json!({"phase": "system", "system": n, "replace": "", "system_sha": sha}).as_object().cloned().unwrap();
        core.ticket_input(Input::Act { agent: agent.into(), is: "began".into(), text: "began".into(), more, t: None }).unwrap();
    };
    began(&mut core, "w3", "1", "Be brief.\n\nAnd kind.");
    began(&mut core, "w2", "1", "Be brief.\n\nAnd kind.");
    open(&mut core, "lead", &["w2"], "the fixture is in place");
    let his = text_id("Be brief.\n\nAnd kind.");
    let served = |core: &crate::core::Core, w: &Who, id: &str| core.ticket_block(w, id).and_then(|s| crate::ticket::block_words(&s));
    assert_eq!(served(&core, &Who::Owner, &his), Err(format!("no request sent under a start that gave {his} is in the record's graph yet")));
    // its session: a subagent's shape and its own, his post going in, two subagent requests and its own a minute after
    let f = dir.join("w2-session.jsonl");
    let later = (chrono::Utc::now() + chrono::Duration::seconds(60)).format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let system = json!([{"type": "text", "text": "You are Claude Code."}, {"type": "text", "text": "Its own words.\n\nBe brief.\n\nAnd kind."}]);
    let subagent = json!([{"type": "text", "text": "A subagent's own words."}]);
    let shape = [("s0", subagent), ("s1", system)].map(|(h, s)| typed("api-request-shape", json!({"shapeHash": h, "shape": {"system": s, "tools": []}})) + "\n").concat();
    let request = |id: &str, shape: &str| typed("api-request", json!({"id": id, "timestamp": later, "params": {"model": "claude-opus-5-5"}, "shapeHash": shape, "tail": []})) + "\n";
    let rest = [
        json!({"type": "user", "message": {"role": "user", "content": "the fixture is in place"}, "timestamp": later}).to_string() + "\n",
        request("r0", "s0"),
        request("r1", "s1"),
        request("r2", "s0"),
    ]
    .concat();
    std::fs::write(&f, shape.clone() + &rest).unwrap();
    let d = core.tickets.due("w2").remove(0);
    core.ticket_input(Input::Given { t: d.t, to: "w2".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(docket_machine::Loc { file: f.display().to_string(), offset: shape.len() as u64, len: 10 }) }).unwrap();
    read_side(&mut core);
    let g = &core.tickets.graph.graph;
    assert_eq!(g.edges.iter().filter(|e| e.kind == EdgeKind::StartedWith && e.to == his).count(), 2, "w3's start and w2's gave it");
    assert!(!g.edges.iter().any(|e| e.kind == EdgeKind::Carried && e.to == his), "no request carried it as a block");
    assert_eq!(served(&core, &Who::Owner, &his), Ok("Be brief.\n\nAnd kind.".to_string()), "w2's last block's tail, its own blank line kept, past w3 and the subagent's shape");
    assert_eq!(served(&core, &Who::Agent("w2".into()), &his), Err("a session's words are the owner's, read in his window".to_string()));
    // the block a request carried is served as before
    assert_eq!(served(&core, &Who::Owner, &text_id("You are Claude Code.")), Ok("You are Claude Code.".to_string()));
    // a second start with another text: the request, sent after it to the second, is its now, and does not end with it
    began(&mut core, "w2", "2", "Be thorough.");
    let second = text_id("Be thorough.");
    assert_eq!(served(&core, &Who::Owner, &second), Err(format!("no block of that shape ends with {second}")));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A round of two sessions goes into the kept graph under two holds of the core's lock, one a session, as coordd's
/// clock puts it (`put_side`; paper-research N-1446 p1474): what waits for the lock behind the put takes it between the
/// two, with one session in. Made up: w2 given the lead's post in one session and his in another.
#[tokio::test]
async fn a_side_round_puts_each_session_under_its_own_hold() {
    use docket_machine::graph::NodeKind;
    let (dir, _base, mut core, _rx) = coord("graph-side-holds", "idle");
    let a = open(&mut core, "lead", &["w2"], "the fixture is in place");
    let t = t_of(&a);
    let give = |core: &mut crate::core::Core, i: usize| {
        let f = dir.join(format!("w2-session-{i}.jsonl"));
        std::fs::write(&f, crate::follow::tests::typed("api-request", serde_json::json!({"id": format!("r{i}"), "timestamp": "2026-11-22T09:00:03Z", "params": {}, "shapeHash": "", "tail": []})) + "\n").unwrap();
        let d = core.tickets.due("w2").remove(0);
        core.ticket_input(Input::Given { t: d.t, to: "w2".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(docket_machine::Loc { file: f.display().to_string(), offset: 0, len: 10 }) }).unwrap();
    };
    give(&mut core, 0);
    core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("@w2 and the second".into()), names: vec!["w2".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    give(&mut core, 1);
    let round = core.side_round().read();
    assert!(round.unopened().is_empty());
    let sessions = |c: &crate::core::Core| c.tickets.graph.graph.nodes.iter().filter(|n| n.kind == NodeKind::Session).count();
    let shared = crate::Shared::new(core);
    // the lock held here while the put and then a waiter queue for it: tokio's lock is first come, first served
    let held = shared.lock().await;
    let put = tokio::spawn({
        let shared = shared.clone();
        async move { crate::put_side(&shared, round, crate::follow::SIDE_PART).await }
    });
    let waiter = tokio::spawn({
        let shared = shared.clone();
        async move { sessions(&*shared.lock().await) }
    });
    for _ in 0..4 {
        tokio::task::yield_now().await;
    }
    drop(held);
    let between = waiter.await.unwrap();
    put.await.unwrap();
    let after = sessions(&*shared.lock().await);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!((between, after), (1, 2), "what waited behind the put took the lock between its two sessions");
}

/// A session's read goes into the kept graph in parts, each under a hold of the core's lock of its own, as coordd's
/// clock puts a round (`put_side`, with `follow::SIDE_PART`; paper-research N-1446 p1922 (a)): what waits for the lock
/// behind the put takes it between two parts, with some of the session's requests in and not all. Made up: w2 given
/// the lead's post, in a session of six requests put in parts of two edges.
#[tokio::test]
async fn a_sessions_read_goes_in_in_parts_each_under_its_own_hold() {
    use docket_machine::graph::NodeKind;
    let (dir, _base, mut core, _rx) = coord("graph-side-parts", "idle");
    open(&mut core, "lead", &["w2"], "the fixture is in place");
    let f = dir.join("w2-session.jsonl");
    let session: String = (0..6).map(|i| crate::follow::tests::typed("api-request", serde_json::json!({"id": format!("r{i}"), "timestamp": "2026-11-22T09:00:03Z", "params": {}, "shapeHash": "", "tail": []})) + "\n").collect();
    std::fs::write(&f, session).unwrap();
    let d = core.tickets.due("w2").remove(0);
    core.ticket_input(Input::Given { t: d.t, to: "w2".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(docket_machine::Loc { file: f.display().to_string(), offset: 0, len: 10 }) }).unwrap();
    let round = core.side_round().read();
    assert!(round.unopened().is_empty());
    let requests = |c: &crate::core::Core| c.tickets.graph.graph.nodes.iter().filter(|n| n.kind == NodeKind::Request).count();
    let shared = crate::Shared::new(core);
    // the lock held here while the put and then a waiter queue for it: tokio's lock is first come, first served
    let held = shared.lock().await;
    let put = tokio::spawn({
        let shared = shared.clone();
        async move { crate::put_side(&shared, round, 2).await }
    });
    let waiter = tokio::spawn({
        let shared = shared.clone();
        async move { requests(&*shared.lock().await) }
    });
    for _ in 0..4 {
        tokio::task::yield_now().await;
    }
    drop(held);
    let between = waiter.await.unwrap();
    put.await.unwrap();
    let after = requests(&*shared.lock().await);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(between > 0 && between < after && after == 6, "what waited behind the put took the lock between two of its parts: {between} of {after} requests in");
}
