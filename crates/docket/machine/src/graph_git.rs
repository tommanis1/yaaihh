//! The graph's sources beside the record (paper-research N-1446 p1364, p1372, p1376 1; the owner, N-1104 p764:
//! "submititng work to the runner, should track the relvant ocmmits. in the ocmmit agents musting mention what wokrs it
//! relvant too"): git's commits and the runner's jobs. Each node and edge made here says its source, its commit or its
//! job ([`Source::Commit`], [`Source::Job`]); what a release carries, its release ([`Source::Release`]). docket-machine
//! runs no git and reads no file: its caller gives the log (`git log --all --name-only --format=` [`Commit::FORMAT`],
//! read by [`Commit::parse_log`]) and the jobs ([`Job::parse`]).
//!
//! - commit `commit:HASH8`, its subject its `what`, at its commit time: each of its `Agent:` → it, `wrote`; it → each
//!   of its `Session:` (`session:ID`), `came_from`; it → its first parent, `came_from`; each other parent → it,
//!   `merged_into`.
//! - job `job:FILE`, at its start, its verb, packages and totals its `what`, `ok` its rc 0 once it has ended: its agent
//!   → it, `ran`, at its start; it → the commit it was filed under, `ran`, at its start, and `ended`, at its end. From
//!   the job's file, or from its lines in the record (`act` `job`, build-runner's, docket-core N-1446 p1397), which
//!   name the file as `id`: one job either way, and the record's when it has the job. It → the worktree it sent (its
//!   `wt`), `in`, at its start.
//! - worktree `worktree:TOP` ([`worktree_id`]): a job's, as above; a commit → the worktree it was made in, `in`, is
//!   docket-edge8's, from each worktree's HEAD reflog (paper-research N-1446 p1530).
//! - branch `branch:REPO/NAME`: each commit → each branch git's reflog says was made on it or came to it, `on`, the
//!   first time (`git log -g --all --format=` [`Moved::FORMAT`], read by [`Moved::parse_log`]).
//! - worktree ([`worktree_id`], by its top as git gives it): each commit → the worktree whose own HEAD reflog first
//!   says git made it there, `in`, at that time ([`MadeIn::parse_reflog`], paper-research N-1446 p1530, p1554). A
//!   commit no reflog left says was made (its worktree pruned, or its entry expired) is in none: its worktree is not
//!   known, never guessed from its branch.
//! - a build in the graph carries each commit its head (the hash its name ends in) reaches that the head of the
//!   release it follows does not (`came_from`, from its `window_release` line's `contains`): each → the build,
//!   `built_into`, but the head, whose edge is the record's. A build that follows none in the graph carries what
//!   nothing says, so none.
//!
//! - the facts a decision on a commit is handed ([`facts`]; proto-mind N-1446 p1502 (1)): from the commits read (their
//!   trees and files), the jobs' lines and files as they say them ([`Graph::jobs`]), the release acts
//!   ([`Graph::releases`]) and the repositories' cargo packages ([`Packages`]).
//!
//! Not here: a commit's `Item:` → its item or his post, and its subject → the posts it names (docket-edge8's).
//! Not given by these sources, so not made up: a job's tree, when it sent work that is no commit (`Job::tree`, once
//! the runner records it, p1372); a release's time (its name has it in a zone it does not name).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ops::Bound;

use serde::{Deserialize, Serialize};

use crate::graph::{EdgeKind, Graph, NodeKind, Source, agent_id};
use crate::line::At;

/// A commit with its Agent, Session and Item trailers: the window's, from the project's `commits` view, and the
/// graph's, from git's log ([`Commit::parse_log`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Commit {
    pub hash: String,
    /// When it was committed: the view's as it gives it (RFC 3339), the log's as the record writes a time.
    pub at: String,
    pub subject: String,
    /// Its `Agent:` trailers, comma-separated.
    pub agent: String,
    /// Its `Session:` trailers, comma-separated.
    #[serde(default)]
    pub session: String,
    /// Its `Item:` trailers, each split at commas.
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(default)]
    pub refs: String,
    #[serde(default)]
    pub repo: String,
    /// landed, in the runner or not yet.
    #[serde(default)]
    pub state: String,
    /// Its parents' hashes, the first first: the log's (the view gives none).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parents: Vec<String>,
    /// Its tree's hash, the log's (`%T`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tree: String,
    /// The files it changes over its first parent, the log's under `--name-only`; a merge's none, as git gives none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
}

impl Commit {
    /// The `git log` format [`Commit::parse_log`] reads, given with `--name-only`: a record separator before each, then
    /// hash, commit time (seconds), parents, refs, tree, the whole message, a unit separator between and after them,
    /// and the files `--name-only` lists after that (proto-mind N-1446 p1502 (1)).
    pub const FORMAT: &'static str = "%x1e%H%x1f%ct%x1f%P%x1f%D%x1f%T%x1f%B%x1f";

    /// The commits of LOG, git's log in [`Commit::FORMAT`], of the repository named REPO; one that does not read is
    /// left out.
    pub fn parse_log(log: &str, repo: &str) -> Vec<Commit> {
        log.split('\x1e')
            .filter_map(|r| {
                let r = r.trim_start_matches('\n');
                let mut f = r.splitn(7, '\x1f');
                let (hash, ct, parents, refs, tree, message) = (f.next()?, f.next()?, f.next()?, f.next()?, f.next()?, f.next()?);
                let files = f.next().unwrap_or_default();
                if hash.len() < 8 {
                    return None;
                }
                let at = crate::machine::at_secs(ct.trim().parse().ok()?);
                let ts = trailers(message);
                let all = |key: &str| -> Vec<&str> { ts.iter().filter(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str()).collect() };
                let subject = message.lines().map(str::trim).take_while(|l| !l.is_empty()).collect::<Vec<_>>().join(" ");
                Some(Commit {
                    hash: hash.to_string(),
                    at,
                    subject,
                    agent: all("Agent").join(","),
                    session: all("Session").join(","),
                    items: all("Item").iter().flat_map(|v| v.split(',')).map(str::trim).filter(|i| !i.is_empty()).map(String::from).collect(),
                    refs: refs.to_string(),
                    repo: repo.to_string(),
                    state: String::new(),
                    parents: parents.split_whitespace().map(String::from).collect(),
                    tree: tree.trim().to_string(),
                    files: files.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect(),
                })
            })
            .collect()
    }
}

/// The trailers of MESSAGE, by paper-research's rule (N-1446 p1372): git's own reading of a trailer (`Token: value`,
/// the token letters, digits and `-`, space allowed before the colon; a line that starts with a space or a tab goes
/// on with the one above), applied to the run of paragraphs at the message's end that hold only such lines, the
/// subject's never. So an `Item:` a blank line splits off the `Co-Authored-By:` paragraph is read, where git's
/// `%(trailers)` takes the last paragraph alone (77 of the 226 commits from 4ee5c14e to a0c60fba, proto-window p1371).
pub fn trailers(message: &str) -> Vec<(String, String)> {
    let mut paras: Vec<Vec<&str>> = vec![Vec::new()];
    for l in message.lines().map(str::trim_end) {
        if l.is_empty() {
            if !paras.last().is_some_and(Vec::is_empty) {
                paras.push(Vec::new());
            }
        } else {
            paras.last_mut().unwrap().push(l);
        }
    }
    paras.retain(|p| !p.is_empty());
    let mut run: Vec<Vec<(String, String)>> = Vec::new();
    for p in paras.iter().skip(1).rev() {
        match paragraph_trailers(p) {
            Some(ts) => run.push(ts),
            None => break,
        }
    }
    run.into_iter().rev().flatten().collect()
}

/// The trailers of paragraph P, if each of its lines is one or goes on with one.
fn paragraph_trailers(p: &[&str]) -> Option<Vec<(String, String)>> {
    let mut out: Vec<(String, String)> = Vec::new();
    for l in p {
        if l.starts_with([' ', '\t']) {
            let (_, v) = out.last_mut()?;
            v.push(' ');
            v.push_str(l.trim());
            continue;
        }
        let (k, v) = l.split_once(':')?;
        let k = k.trim_end_matches([' ', '\t']);
        if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return None;
        }
        out.push((k.to_string(), v.trim().to_string()));
    }
    Some(out)
}

/// A runner job, with the fields of build-runner's `Job.d`: its job file today (`jobs/PID-SINCE.json`, kept a day
/// after its caller has gone), the record's job lines once the runner writes them (paper-research N-1446 p1372).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Job {
    /// Its id: its job file's name, without `.json` (`548417-1795251629`).
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub agent: String,
    /// The coord it was sent from, its folder: the graph of a record takes that coord's jobs.
    #[serde(default)]
    pub coord: String,
    /// The worktree it sent, and its branch.
    #[serde(default)]
    pub wt: String,
    #[serde(default)]
    pub branch: String,
    /// The commit it was filed under, as the runner gives it (its first 8 characters): the worktree's HEAD.
    #[serde(default)]
    pub commit: String,
    #[serde(default)]
    pub verb: String,
    #[serde(default)]
    pub packages: Vec<String>,
    /// When it started, and when its last step began (build-runner writes `2026-11-21T10:00:29+0100`).
    #[serde(default)]
    pub since: String,
    #[serde(default)]
    pub at: String,
    /// Its last step: `ended` once it has ended.
    #[serde(default)]
    pub step: String,
    #[serde(default)]
    pub rc: Option<i64>,
    #[serde(default)]
    pub passed: Option<u64>,
    #[serde(default)]
    pub failed: Option<u64>,
    /// The tree it sent and its commit's `Item:`s, once the runner records them (p1372); empty in today's files.
    #[serde(default)]
    pub tree: String,
    #[serde(default)]
    pub items: Vec<String>,
    /// A test job's (docket-core's core-jobstate 9145bc00, N-1446 p1515): whether it named no test, target or skip, so
    /// its packages' tests ran whole (`whole`: "1" on its lines, true in its file), and the tests it named (`tests`);
    /// once it has ended, the runner's words when it refused it (`refused`: rc 3, nothing ran, no counts) and the
    /// tests that failed (`failing`, the first 200; `failed` counts them all). One reader of lines and files (p1519).
    #[serde(default)]
    pub whole: bool,
    #[serde(default)]
    pub tests: Vec<String>,
    #[serde(default)]
    pub refused: String,
    #[serde(default)]
    pub failing: Vec<String>,
    /// A replay job's (`build-runner replay`, docket-core's core-replayjob 5bdbb76b, N-1446 p1523): the record it
    /// replayed (`record`, the live record's name, or for the way back the coord the gated tip's engine wrote), which
    /// way (`way`, `forward` or `back`), and once it has ended the counts its test printed (`problem_lines`, `now_makes`,
    /// `now_refused`), none when it printed none (the dev program's stage 2, proto-mind N-1446 p1765).
    #[serde(default)]
    pub record: String,
    #[serde(default)]
    pub way: String,
    #[serde(default)]
    pub problem_lines: Option<u64>,
    #[serde(default)]
    pub now_makes: Option<u64>,
    #[serde(default)]
    pub now_refused: Option<u64>,
}

impl Job {
    /// The job TEXT holds (a job file's JSON), its id ID; none when it does not read.
    pub fn parse(id: &str, text: &str) -> Option<Job> {
        let mut j: Job = serde_json::from_str(text).ok()?;
        j.id = id.to_string();
        Some(j)
    }

    /// The job a runner's record line says (`act` `job`: build-runner's `act`, docket-core N-1446 p1397): its file's
    /// fields beside `phase`, each value as coordc sends it, a string (`"0"`) or a list; AGENT the line's, AT its time,
    /// the job's end when PHASE is `end`, a job out of the runner's queue, running, when it is `runs` (build-runner's
    /// RUNS_STEPS), and else (`start`) a job sent to the runner. None without an `id`.
    pub fn of_act(agent: &str, at: &str, more: &serde_json::Map<String, serde_json::Value>) -> Option<Job> {
        let text = |k: &str| more.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let list = |k: &str| -> Vec<String> {
            match more.get(k) {
                Some(serde_json::Value::Array(xs)) => xs.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
                Some(serde_json::Value::String(x)) => serde_json::from_str(x).unwrap_or_default(),
                _ => Vec::new(),
            }
        };
        let number = |k: &str| more.get(k).and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()));
        let id = text("id");
        if id.is_empty() {
            return None;
        }
        let step = match text("phase").as_str() {
            "end" => "ended",
            "runs" => "runs",
            _ => "start",
        };
        Some(Job {
            id,
            agent: agent.to_string(),
            wt: text("wt"),
            branch: text("branch"),
            commit: text("commit"),
            verb: text("verb"),
            packages: list("packages"),
            since: text("since"),
            at: at.to_string(),
            step: step.into(),
            rc: number("rc"),
            passed: number("passed").and_then(|n| u64::try_from(n).ok()),
            failed: number("failed").and_then(|n| u64::try_from(n).ok()),
            tree: text("tree"),
            items: list("items"),
            whole: text("whole") == "1",
            tests: list("tests"),
            refused: text("refused"),
            failing: list("failing"),
            record: text("record"),
            way: text("way"),
            problem_lines: number("problem_lines").and_then(|n| u64::try_from(n).ok()),
            now_makes: number("now_makes").and_then(|n| u64::try_from(n).ok()),
            now_refused: number("now_refused").and_then(|n| u64::try_from(n).ok()),
            ..Default::default()
        })
    }

    pub(crate) fn ended(&self) -> bool {
        self.step == "ended"
    }

    /// How far it has come: sent 0 (`start`, and a job file's own steps), running 1 (`runs`), ended 2. A line of an
    /// earlier step that comes after a later one does not take it back.
    fn reached(&self) -> u8 {
        match self.step.as_str() {
            "ended" => 2,
            "runs" => 1,
            _ => 0,
        }
    }

    /// Its verb and packages, and once it has ended its rc and totals: `test yaaihh: rc 0, 42 passed, 0 failed`.
    fn what(&self) -> String {
        let mut s = format!("{} {}", self.verb, self.packages.join(" ")).trim_end().to_string();
        if self.ended() {
            s += &format!(": rc {}", self.rc.map_or("?".into(), |r| r.to_string()));
            if let (Some(p), Some(f)) = (self.passed, self.failed) {
                s += &format!(", {p} passed, {f} failed");
            }
        }
        s
    }
}

/// A time as RFC 3339 or build-runner's `%z` writes it (`+0100`, `+01:00`, `Z`), as the record writes one (UTC, `Z`).
pub fn utc(t: &str) -> Option<At> {
    let base = crate::machine::secs(t)?;
    let rest = t.get(19..)?;
    let rest = rest.strip_prefix('.').map_or(rest, |r| r.trim_start_matches(|c: char| c.is_ascii_digit()));
    let off = match rest {
        "" | "Z" => 0,
        r => {
            let sign = match r.as_bytes()[0] {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let d: String = r[1..].chars().filter(char::is_ascii_digit).collect();
            if d.len() != 4 || r[1..].chars().any(|c| !c.is_ascii_digit() && c != ':') {
                return None;
            }
            sign * (d[..2].parse::<i64>().ok()? * 3600 + d[2..].parse::<i64>().ok()? * 60)
        }
    };
    Some(crate::machine::at_secs(base - off))
}

/// A commit's node id: `commit:` and its first 8 characters.
pub fn commit_id(hash: &str) -> String {
    format!("commit:{}", &hash[..8.min(hash.len())])
}

/// A worktree's node id: `worktree:` and its top as the runner's line or git gives it, so that a job's worktree and
/// a commit's are one node (paper-research N-1446 p1530).
pub fn worktree_id(top: &str) -> String {
    format!("worktree:{top}")
}

fn short(hash: &str) -> String {
    hash[..8.min(hash.len())].to_string()
}

/// Adds COMMITS (git's, of every repository the record's work is in) and JOBS (those sent from the record's coord)
/// to G, and then what each build in G carries.
pub fn add(g: &mut Graph, commits: &[Commit], jobs: &[Job]) {
    commit_edges(g, commits);
    // a job the record has (its `act` lines) is the record's; its file says nothing more
    for j in jobs {
        if !g.get(&format!("job:{}", j.id)).is_some_and(|n| n.line().is_some()) {
            job(g, j, Source::Job(j.id.clone()), true);
        }
    }
    carried(g, commits);
}

/// Adds NEW, commits not in G yet, to G, and then what each build in G carries by ALL, every commit read with them:
/// coordd's kept graph, as git's log is read again once its refs move (paper-research N-1446 p1406, head 1b). What a
/// build carries already is not added again (`carried`).
pub fn add_commits(g: &mut Graph, new: &[Commit], all: &[Commit]) {
    commit_edges(g, new);
    carried(g, all);
}

/// COMMITS' nodes and edges in G: each commit's own node first, so one met first as another's parent or a job's, or in
/// the record, has its time and subject.
fn commit_edges(g: &mut Graph, commits: &[Commit]) {
    for c in commits {
        let id = g.node_from(commit_id(&c.hash), NodeKind::Commit, Source::Commit(short(&c.hash)), None);
        let n = g.get_mut(&id).unwrap();
        n.when = Some(c.at.clone()).filter(|a| !a.is_empty());
        n.what = c.subject.clone();
    }
    for c in commits {
        let (cid, src, when) = (commit_id(&c.hash), Source::Commit(short(&c.hash)), Some(c.at.clone()).filter(|a| !a.is_empty()));
        for a in c.agent.split(',').map(str::trim).filter(|a| !a.is_empty()) {
            let a = g.node_from(agent_id(a), NodeKind::Agent, src.clone(), when.clone());
            g.edge_from(&a, &cid, EdgeKind::Wrote, src.clone(), when.clone());
        }
        for s in c.session.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let s = g.node_from(format!("session:{s}"), NodeKind::Session, src.clone(), when.clone());
            g.edge_from(&cid, &s, EdgeKind::CameFrom, src.clone(), when.clone());
        }
        for (k, p) in c.parents.iter().enumerate() {
            let p = g.node_from(commit_id(p), NodeKind::Commit, src.clone(), None);
            if k == 0 {
                g.edge_from(&cid, &p, EdgeKind::CameFrom, src.clone(), when.clone());
            } else {
                g.edge_from(&p, &cid, EdgeKind::MergedInto, src.clone(), when.clone());
            }
        }
    }
}

/// A runner job's record line (`act` `job`) at LINE, at AT, by AGENT, its fields MORE: its start's edges, or its end's
/// (and its start's, when the record had no start line of it).
pub(crate) fn job_line(g: &mut Graph, line: usize, at: &str, agent: &str, more: &serde_json::Map<String, serde_json::Value>) {
    let Some(j) = Job::of_act(agent, at, more) else { return };
    // its `runs` adds no edge its start made (it says only that the job left the queue)
    let start = j.step == "start" || g.get(&format!("job:{}", j.id)).is_none();
    job(g, &j, Source::Line(line), start);
}

/// Job J's node and edges from SRC: at its start (when START), its agent → it and it → its commit, `ran`, and it →
/// each item or post it was for (its `items`, as the runner's line says them), `named`, so a job reaches an ask through
/// what it was for as well as through its commit (paper-research N-1446 p1422, 4), and → the worktree it sent (its
/// `wt`), `in` (p1530); once it has ended, it → its commit, `ended`, and its node's `ok`.
fn job(g: &mut Graph, j: &Job, src: Source, start: bool) {
    // the job as its lines or file say it, for the facts ([`facts`]); a start that comes after its end does not undo it
    if g.jobs.get(&j.id).is_none_or(|old| old.reached() <= j.reached()) {
        g.jobs.insert(j.id.clone(), j.clone());
    }
    let begun = utc(&j.since).or_else(|| if j.ended() { None } else { utc(&j.at) });
    let jid = g.node_from(format!("job:{}", j.id), NodeKind::Job, src.clone(), begun.clone());
    let n = g.get_mut(&jid).unwrap();
    n.what = j.what();
    n.ok = if j.ended() { j.rc.map(|r| r == 0) } else { None };
    let c = (!j.commit.is_empty()).then(|| g.node_from(commit_id(&j.commit), NodeKind::Commit, src.clone(), None));
    if start {
        if !j.agent.is_empty() {
            let a = g.node_from(agent_id(&j.agent), NodeKind::Agent, src.clone(), begun.clone());
            g.edge_from(&a, &jid, EdgeKind::Ran, src.clone(), begun.clone());
        }
        if let Some(c) = &c {
            g.edge_from(&jid, c, EdgeKind::Ran, src.clone(), begun.clone());
        }
        for (tk, n) in j.items.iter().filter_map(|i| crate::post_or_ticket(i.trim())) {
            let to = match n {
                Some(n) => g.node_from(format!("{tk}.p{n}"), NodeKind::Post, src.clone(), None),
                None => g.node_from(tk, NodeKind::Ticket, src.clone(), None),
            };
            g.edge_from(&jid, &to, EdgeKind::Named, src.clone(), begun.clone());
        }
        if !j.wt.is_empty() {
            let w = g.node_from(worktree_id(&j.wt), NodeKind::Worktree, src.clone(), begun.clone());
            g.edge_from(&jid, &w, EdgeKind::In, src.clone(), begun.clone());
        }
    }
    if let (true, Some(c)) = (j.ended(), &c) {
        g.edge_from(&jid, c, EdgeKind::Ended, src, utc(&j.at));
    }
}

/// A move of a branch, from git's reflog of it: BRANCH of REPO came to HASH at AT, by HOW (`commit: …`, `merge X:
/// Fast-forward`, `branch: Created from …`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Moved {
    pub repo: String,
    pub branch: String,
    pub hash: String,
    pub at: At,
    pub how: String,
}

impl Moved {
    /// The one `git log -g --all --date=unix` format: each ref's moves (its selector, with its time), newest first.
    pub const FORMAT: &'static str = "%gD%x1f%H%x1f%gs%x1e";

    /// The branches' moves LOG (`git log -g --all --date=unix --format=` [`Moved::FORMAT`]) says, in REPO; a ref that
    /// is no branch (a worktree's HEAD, a remote's) is left out: a worktree's HEAD reflog is read from its own file,
    /// for where a commit was made ([`MadeIn::parse_reflog`]).
    pub fn parse_log(log: &str, repo: &str) -> Vec<Moved> {
        log.split('\x1e')
            .filter_map(|r| {
                let mut f = r.trim_start_matches('\n').splitn(3, '\x1f');
                let (sel, hash, how) = (f.next()?, f.next()?, f.next().unwrap_or_default());
                let (name, t) = sel.rsplit_once("@{")?;
                let branch = name.strip_prefix("refs/heads/")?;
                let secs: i64 = t.strip_suffix('}')?.parse().ok()?;
                Some(Moved { repo: repo.to_string(), branch: branch.to_string(), hash: hash.to_string(), at: crate::machine::at_secs(secs), how: how.trim().to_string() })
            })
            .collect()
    }
}

/// Adds what MOVES say: each commit → each branch made on it or come to it, `on`, at the first time (the branch it was
/// made on comes first; one a fast-forward or a reset brought it to, later).
pub fn add_moves(g: &mut Graph, moves: &[Moved]) {
    let mut first: BTreeMap<(String, String), &Moved> = BTreeMap::new();
    for m in moves {
        let k = (commit_id(&m.hash), format!("branch:{}/{}", m.repo, m.branch));
        if first.get(&k).is_none_or(|x| m.at < x.at) {
            first.insert(k, m);
        }
    }
    for ((c, b), m) in first {
        let src = Source::Commit(short(&m.hash));
        let c = g.node_from(c, NodeKind::Commit, src.clone(), None);
        let b = g.node_from(b, NodeKind::Branch, src.clone(), Some(m.at.clone()));
        g.edge_from(&c, &b, EdgeKind::On, src, Some(m.at.clone()));
    }
}

/// A release act (`act` `release`, release.sh's `act_line`; docket-core's core-relacts 7d675f94, N-1446 p1539): when,
/// its phase (`cut`, `live`, `switch`, `coord_back`, ...), the checkout whose release.sh wrote it (production's for an
/// install, a train tree's for a cut, a scratch tree's for a test), the coord a switch or a return is of, and the
/// commit that coord's coordd runs after it (`coordd_commit`). Saved with a start's replay (coordd's replay_state.rs,
/// I-1509): the graph's own serde leaves it out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseAct {
    pub at: At,
    pub phase: String,
    pub checkout: String,
    pub coord: String,
    pub coordd_commit: String,
}

/// A release act's record line at AT, its fields MORE, into G's [`Graph::releases`]: taken by its `is` wherever it
/// lands (proto-mind N-1446 p1541: under a program it lands on the newest ticket its agent owes a step on).
pub(crate) fn release_line(g: &mut Graph, at: &str, more: &serde_json::Map<String, serde_json::Value>) {
    let text = |k: &str| more.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    g.releases.push(ReleaseAct { at: at.to_string(), phase: text("phase"), checkout: text("checkout"), coord: text("coord"), coordd_commit: text("coordd_commit") });
}

/// The commit coord COORD's coordd runs by G's release acts: the last `switch` or `coord_back` of it whose checkout is
/// CHECKOUT, production's (proto-mind N-1446 p1541); `current_commit`, the links, runs nothing until the switch, and
/// his window's commit does not say it (draft 2's dev.eflint `way`).
pub fn live_commit<'a>(g: &'a Graph, coord: &str, checkout: &str) -> Option<&'a str> {
    let checkout = checkout.trim_end_matches('/');
    g.releases
        .iter()
        .rev()
        .find(|r| matches!(r.phase.as_str(), "switch" | "coord_back") && r.coord == coord && r.checkout.trim_end_matches('/') == checkout && !r.coordd_commit.is_empty())
        .map(|r| r.coordd_commit.as_str())
}

/// A repository's cargo packages, from its manifests as git's side read them (coordd `git_read`; proto-mind N-1446
/// p1502 (1)): each package's folder and name (`dirs`: `crates/coordd` → coordd), its workspaces' members by name
/// (`members`), and each (user, used) where a package's manifest names another's folder as a `path` dependency, of its
/// dependencies, dev- or build-dependencies (`uses`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Packages {
    pub repo: String,
    pub dirs: BTreeMap<String, String>,
    pub members: BTreeSet<String>,
    pub uses: BTreeSet<(String, String)>,
}

impl Packages {
    /// The package file PATH of the repository is in: the one whose folder holds it, the nearest; none for a file in no
    /// package's folder (agent-base's, a plan's).
    pub fn of(&self, path: &str) -> Option<&str> {
        self.dirs.iter().filter(|(d, _)| d.is_empty() || path.strip_prefix(d.as_str()).is_some_and(|r| r.starts_with('/'))).max_by_key(|(d, _)| d.len()).map(|(_, n)| n.as_str())
    }
}

/// The facts a decision on commit COMMIT is handed, the commit as its call gives it (a hash or its first characters),
/// from the kept graph G, the commits read COMMITS and their repositories' PACKAGES (proto-mind N-1446 p1502 (1), the
/// facts draft 2's policy/dev.eflint declares; paper-research p1495, p1519; proto-mind p1541); none when no commit read
/// is it. Each names the commit as the call does, so a rule's `call.commit` meets it.
/// - `tree-of(C, T)`: its tree.
/// - `changes(C, P)`, each package a file of the commits it adds is in, and `names-item(C, I)`, each of their `Item:`s.
///   What it adds is what it reaches and the train's tips before it do not: the commits its repository's `main` and
///   each `train…` branch stood at, by the reflogs (`on`), it itself aside. So a head adds its own commits over the tip
///   it was made on or merged in, and a train's merge, or a head the train came to, those over the tip before it.
/// - `package(P)`, each member of its repository's workspaces, and `uses(U, P)`.
/// - each job of G on its tree (`job(ID, T)`): `job-package`, `job-verb`, `job-test` (each test it named), `job-whole`,
///   and once it has ended `job-rc`, `job-passed`, `job-failed`, `job-refused(ID, WHY)` and `job-failing(ID, TEST)`;
///   a replay's `job-record(ID, R)` and `job-way(ID, W)`, and once it has ended the counts its test printed,
///   `job-problem-lines`, `job-now-makes` and `job-now-refused` (proto-mind N-1446 p1765: what the cut's `gates` reads).
/// - `live-changes(C, P)`, with LIVE, the commit the coord's coordd runs ([`live_commit`]): each package a file of the
///   commits one of the two reaches and the other does not is in, what `git diff` from LIVE to C touches and more (a
///   file changed and changed back).
pub fn facts(g: &Graph, commits: &[Commit], packages: &[Packages], commit: &str, live: Option<&str>) -> Vec<(String, Vec<String>)> {
    let mut out: BTreeSet<(&'static str, Vec<String>)> = BTreeSet::new();
    let by: BTreeMap<&str, &Commit> = commits.iter().map(|c| (c.hash.as_str(), c)).collect();
    let find = |h: &str| -> Option<&str> {
        let h = h.trim();
        (h.len() >= 7).then_some(())?;
        by.range::<str, _>((Bound::Included(h), Bound::Unbounded)).next().filter(|(k, _)| k.starts_with(h)).map(|(k, _)| *k)
    };
    let Some(me) = find(commit) else { return Vec::new() };
    let (c, id) = (by[me], commit.trim().to_string());
    let pk = packages.iter().find(|p| p.repo == c.repo);
    let changed = |xs: &mut dyn Iterator<Item = &Commit>| -> BTreeSet<String> { xs.flat_map(|x| &x.files).filter_map(|f| pk?.of(f)).map(str::to_string).collect() };
    if !c.tree.is_empty() {
        out.insert(("tree-of", vec![id.clone(), c.tree.clone()]));
    }
    // the train's tips: where its repository's main and each train branch stood
    let by8: BTreeMap<&str, &str> = commits.iter().filter(|x| x.repo == c.repo).map(|x| (&x.hash[..8.min(x.hash.len())], x.hash.as_str())).collect();
    let prefix = format!("branch:{}/", c.repo);
    let tips: BTreeSet<&str> = g
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::On && e.to.strip_prefix(prefix.as_str()).is_some_and(|b| b == "main" || b.starts_with("train")))
        .filter_map(|e| by8.get(e.from.trim_start_matches("commit:")).copied())
        .collect();
    let mine = reach(&by, me);
    let mut before: Vec<&str> = mine.iter().copied().filter(|h| *h != me && tips.contains(h)).collect();
    before.sort_by(|a, b| by[b].at.cmp(&by[a].at));
    let mut old: BTreeSet<&str> = BTreeSet::new();
    for t in before {
        if !old.contains(t) {
            old.extend(reach(&by, t));
        }
    }
    let adds: Vec<&Commit> = mine.iter().filter(|h| !old.contains(*h)).map(|h| by[h]).collect();
    for p in changed(&mut adds.iter().copied()) {
        out.insert(("changes", vec![id.clone(), p]));
    }
    for i in adds.iter().flat_map(|x| &x.items) {
        out.insert(("names-item", vec![id.clone(), i.clone()]));
    }
    for p in pk.into_iter().flat_map(|p| &p.members) {
        out.insert(("package", vec![p.clone()]));
    }
    for (u, p) in pk.into_iter().flat_map(|p| &p.uses) {
        out.insert(("uses", vec![u.clone(), p.clone()]));
    }
    for j in g.jobs.values().filter(|j| !c.tree.is_empty() && j.tree == c.tree) {
        let ji = j.id.clone();
        out.insert(("job", vec![ji.clone(), c.tree.clone()]));
        for p in &j.packages {
            out.insert(("job-package", vec![ji.clone(), p.clone()]));
        }
        if !j.verb.is_empty() {
            out.insert(("job-verb", vec![ji.clone(), j.verb.clone()]));
        }
        for t in &j.tests {
            out.insert(("job-test", vec![ji.clone(), t.clone()]));
        }
        if j.whole {
            out.insert(("job-whole", vec![ji.clone()]));
        }
        for (f, v) in [("job-record", &j.record), ("job-way", &j.way)].into_iter().filter(|(_, v)| !v.is_empty()) {
            out.insert((f, vec![ji.clone(), v.clone()]));
        }
        if !j.ended() {
            continue;
        }
        let n = |x: Option<u64>| x.map(|n| n.to_string());
        let counts = [("job-problem-lines", n(j.problem_lines)), ("job-now-makes", n(j.now_makes)), ("job-now-refused", n(j.now_refused))];
        for (f, v) in [("job-rc", j.rc.map(|r| r.to_string())), ("job-passed", n(j.passed)), ("job-failed", n(j.failed))].into_iter().chain(counts) {
            if let Some(v) = v {
                out.insert((f, vec![ji.clone(), v]));
            }
        }
        if !j.refused.is_empty() {
            out.insert(("job-refused", vec![ji.clone(), j.refused.clone()]));
        }
        for t in &j.failing {
            out.insert(("job-failing", vec![ji.clone(), t.clone()]));
        }
    }
    if let Some(l) = live.and_then(|l| find(l)) {
        let theirs = reach(&by, l);
        for p in changed(&mut mine.symmetric_difference(&theirs).map(|h| by[h])) {
            out.insert(("live-changes", vec![id.clone(), p]));
        }
    }
    out.into_iter().map(|(f, a)| (f.to_string(), a)).collect()
}

/// A commit git made in a worktree, from that worktree's own HEAD reflog: HASH, of REPO, made at AT in the worktree
/// whose top is TOP, by HOW (the entry's kind: `commit`, `commit (amend)`, `merge side`, `cherry-pick`, `rebase
/// (pick)`, …).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MadeIn {
    pub repo: String,
    pub top: String,
    pub hash: String,
    pub at: At,
    pub how: String,
}

impl MadeIn {
    /// The commits LOG, the text of the HEAD reflog of REPO's worktree whose top is TOP (its `logs/HEAD` in the
    /// repository's common git folder, `worktrees/NAME/logs/HEAD` for one not its main), says git made there: an entry
    /// whose kind makes its commit ([`makes_commit`]). Of each entry only its new commit, its time and its kind are
    /// read; who it was by, never.
    pub fn parse_reflog(log: &str, repo: &str, top: &str) -> Vec<MadeIn> {
        log.lines()
            .filter_map(|l| {
                let (entry, message) = l.split_once('\t').unwrap_or((l, ""));
                let hash = entry.split(' ').nth(1).filter(|h| h.len() >= 40 && h.chars().all(|c| c.is_ascii_hexdigit()) && h.chars().any(|c| c != '0'))?;
                let secs: i64 = entry.rsplit(' ').nth(1)?.parse().ok()?;
                let (how, rest) = message.split_once(':').unwrap_or((message, ""));
                makes_commit(how, rest.trim()).then(|| MadeIn { repo: repo.to_string(), top: top.to_string(), hash: hash.to_string(), at: crate::machine::at_secs(secs), how: how.to_string() })
            })
            .collect()
    }
}

/// Whether a HEAD reflog entry of kind HOW, its words after the kind REST, is git making its commit there: a commit (an
/// amend, a merge's or a cherry-pick's own commit after a stop), a cherry-pick, a revert, `am`, a merge that made one,
/// a rebase's picks; not a checkout, a reset, a fast-forward (a merge's, a pull's, a cherry-pick's), a rebase's start
/// or finish.
fn makes_commit(how: &str, rest: &str) -> bool {
    let picks = ["(pick)", "(reword)", "(edit)", "(squash)", "(fixup)", "(continue)"];
    match how.split(' ').next().unwrap_or_default() {
        "commit" | "revert" | "am" => true,
        "cherry-pick" => rest != "fast-forward",
        "rebase" | "pull" if picks.iter().any(|p| how.ends_with(p)) => true,
        "merge" | "pull" => rest.starts_with("Merge made by"),
        _ => false,
    }
}

/// Adds what MADE says: each commit → the worktree whose HEAD reflog first has git make it, `in`, at that time (one in
/// the same second in two, by the worktree's top); a commit none of them has is in none.
pub fn add_made_in(g: &mut Graph, made: &[MadeIn]) {
    let mut first: BTreeMap<String, &MadeIn> = BTreeMap::new();
    for m in made {
        let c = commit_id(&m.hash);
        if first.get(&c).is_none_or(|x| (&m.at, &m.top) < (&x.at, &x.top)) {
            first.insert(c, m);
        }
    }
    for (c, m) in first {
        let src = Source::Commit(short(&m.hash));
        let c = g.node_from(c, NodeKind::Commit, src.clone(), None);
        let w = g.node_from(worktree_id(&m.top), NodeKind::Worktree, src.clone(), Some(m.at.clone()));
        g.edge_from(&c, &w, EdgeKind::In, src, Some(m.at.clone()));
    }
}

/// What each build in G carries, from COMMITS' parents: the edges `add` says.
fn carried(g: &mut Graph, commits: &[Commit]) {
    let by: BTreeMap<&str, &Commit> = commits.iter().map(|c| (c.hash.as_str(), c)).collect();
    let follows: Vec<(String, String)> = g
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::CameFrom && [&e.from, &e.to].iter().all(|x| g.get(x).is_some_and(|n| n.kind == NodeKind::Build)))
        .map(|e| (e.from.clone(), e.to.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    for (b, before) in follows {
        let name = b.trim_start_matches("build:");
        let (Some(h), Some(p)) = (head(&by, name), head(&by, before.trim_start_matches("build:"))) else { continue };
        let old = reach(&by, p);
        let recorded: BTreeSet<String> = g.edges.iter().filter(|e| e.kind == EdgeKind::BuiltInto && e.to == b).map(|e| e.from.clone()).collect();
        for c in reach(&by, h).into_iter().filter(|c| !old.contains(c)) {
            let cid = commit_id(c);
            if !recorded.contains(&cid) {
                g.edge_from(&cid, &b, EdgeKind::BuiltInto, Source::Release(name.to_string()), None);
            }
        }
    }
}

/// The commit of BY (by full hash) a release NAME ends in: by 7 or 8 characters, as release names have it.
fn head<'a>(by: &BTreeMap<&'a str, &'a Commit>, name: &str) -> Option<&'a str> {
    let h = name.rsplit('-').next().filter(|h| h.len() >= 7 && h.chars().all(|c| c.is_ascii_hexdigit()))?;
    by.range::<str, _>((Bound::Included(h), Bound::Unbounded)).next().filter(|(k, _)| k.starts_with(h)).map(|(k, _)| *k)
}

/// FROM and every commit of BY it reaches by parents.
fn reach<'a>(by: &BTreeMap<&'a str, &'a Commit>, from: &'a str) -> BTreeSet<&'a str> {
    let mut seen = BTreeSet::from([from]);
    let mut front = VecDeque::from([from]);
    while let Some(h) = front.pop_front() {
        for p in by.get(h).into_iter().flat_map(|c| &c.parents) {
            if let Some((p, _)) = by.get_key_value(p.as_str()) {
                if seen.insert(*p) {
                    front.push_back(*p);
                }
            }
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Node;

    fn entry(hash: &str, ct: i64, parents: &[&str], message: &str) -> String {
        entry_of(hash, ct, parents, "", message, &[])
    }

    /// A commit as `git log --name-only --format=` [`Commit::FORMAT`] gives it, its tree TREE and its files FILES.
    fn entry_of(hash: &str, ct: i64, parents: &[&str], tree: &str, message: &str, files: &[&str]) -> String {
        format!("\x1e{hash}\x1f{ct}\x1f{}\x1f\x1f{tree}\x1f{message}\x1f\n\n{}\n", parents.join(" "), files.join("\n"))
    }

    fn full(h: &str) -> String {
        format!("{h}{}", "0".repeat(40 - h.len()))
    }

    #[test]
    fn trailers_are_the_run_of_key_value_paragraphs_at_the_end() {
        let split = "Merge acc2\n\nlens.rs keeps its own span.\n\nItem: I-1469\n\nCo-Authored-By: Claude <x@y>\nAgent: proto-lead\nSession: 27dc\n";
        let t = trailers(split);
        let keys: Vec<&str> = t.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["Item", "Co-Authored-By", "Agent", "Session"], "the Item a blank line splits off is read");
        // a paragraph with one line that is no trailer ends the run; the subject is never one; a line that starts
        // with a space goes on with the one above
        assert!(trailers("Item: I-1\n\nNot a trailer here\nAgent: a\n").is_empty());
        assert!(trailers("Item: I-1").is_empty());
        assert_eq!(trailers("s\n\nItem: I-1,\n  N-1104.p678\nFixes bug: no\n"), Vec::<(String, String)>::new());
        assert_eq!(trailers("s\n\nItem: I-1,\n  N-1104.p678\n"), vec![("Item".into(), "I-1, N-1104.p678".into())]);
    }

    #[test]
    fn the_log_reads_parents_times_and_trailers_and_a_job_its_times_in_utc() {
        let log = entry(&full("de447f86"), 1795250493, &[&full("7b3e6724")], "Accounts tab: one d h m\n(owner N-1104 p678)\n\nbody\n\nItem: I-1469, N-1104.p678\nCo-Authored-By: C\nAgent: accounts-2\nSession: ec4b\n");
        let c = Commit::parse_log(&log, "soft-swarm");
        assert_eq!(c.len(), 1);
        let c = &c[0];
        assert_eq!((c.at.as_str(), c.subject.as_str(), c.agent.as_str(), c.session.as_str()), ("2026-11-21T08:41:33Z", "Accounts tab: one d h m (owner N-1104 p678)", "accounts-2", "ec4b"));
        assert_eq!((c.items.clone(), c.parents.clone(), c.repo.as_str()), (vec!["I-1469".to_string(), "N-1104.p678".into()], vec![full("7b3e6724")], "soft-swarm"));
        let j = Job::parse("548417-1795251629", r#"{"agent": "accounts-2", "commit": "de447f86", "verb": "test", "packages": ["yaaihh"], "since": "2026-11-21T10:00:29+0100", "at": "2026-11-21T10:01:19+0100", "step": "ended", "rc": 0, "passed": 42, "failed": 0, "pid": 548417}"#).unwrap();
        assert_eq!((utc(&j.since).unwrap(), utc(&j.at).unwrap(), j.what()), ("2026-11-21T09:00:29Z".to_string(), "2026-11-21T09:01:19Z".to_string(), "test yaaihh: rc 0, 42 passed, 0 failed".to_string()));
        assert_eq!((utc("2026-11-21T10:00:29+01:00"), utc("2026-11-21T08:19:19Z"), utc("2026-11-21T08:19:19.25Z"), utc("2026-11-21T08:19:19 x")), (Some("2026-11-21T09:00:29Z".into()), Some("2026-11-21T08:19:19Z".into()), Some("2026-11-21T08:19:19Z".into()), None));
    }

    /// p678's course, made up in its shape (N-1104 p678 → accounts-2's de447f86 → proto-lead's merge 4ee5c14e →
    /// build 59 in his window): the record's edges as its lines would give them, then git's and the runner's.
    #[test]
    fn an_asks_course_runs_from_his_post_through_its_commit_merge_and_green_job_to_his_window() {
        let mut g = Graph::default();
        let line = |g: &mut Graph, from: (&str, NodeKind), to: (&str, NodeKind), k: EdgeKind, n: usize, at: &str| {
            let a = g.node_from(from.0, from.1, Source::Line(n), Some(at.into()));
            let b = g.node_from(to.0, to.1, Source::Line(n), Some(at.into()));
            g.edge_from(&a, &b, k, Source::Line(n), Some(at.into()));
        };
        let (owner, post, rel, before) = ("agent:owner", "N-1104.p678", "build:20261121-101928-4ee5c14e", "build:20261121-095202-b49e1ed3");
        line(&mut g, (owner, NodeKind::Agent), (post, NodeKind::Post), EdgeKind::Wrote, 1, "2026-11-21T08:19:19Z");
        // each post of his here asks an agent, as the ASKED ledger says, so each is an ask (paper-research N-1446 p1561)
        line(&mut g, (post, NodeKind::Post), ("agent:paper-research", NodeKind::Agent), EdgeKind::Asks, 1, "2026-11-21T08:19:19Z");
        line(&mut g, ("N-1104.p677", NodeKind::Post), ("agent:paper-research", NodeKind::Agent), EdgeKind::Handed, 2, "2026-11-21T08:19:00Z");
        line(&mut g, ("N-1104.p679", NodeKind::Post), ("agent:paper-research", NodeKind::Agent), EdgeKind::Handed, 3, "2026-11-21T08:20:05Z");
        line(&mut g, ("N-1104.p680", NodeKind::Post), ("agent:accounts-2", NodeKind::Agent), EdgeKind::Handed, 4, "2026-11-21T08:22:00Z");
        line(&mut g, ("commit:4ee5c14e", NodeKind::Commit), (rel, NodeKind::Build), EdgeKind::BuiltInto, 9, "2026-11-21T09:39:20Z");
        line(&mut g, (rel, NodeKind::Build), (before, NodeKind::Build), EdgeKind::CameFrom, 9, "2026-11-21T09:39:20Z");
        line(&mut g, (rel, NodeKind::Build), (owner, NodeKind::Agent), EdgeKind::Installed, 9, "2026-11-21T09:39:20Z");
        // taken: paper-research's answer to a later post of the ticket; not his own answer, nor an answer to an earlier post
        line(&mut g, ("agent:paper-research", NodeKind::Agent), ("N-1104.p681", NodeKind::Post), EdgeKind::Wrote, 5, "2026-11-21T08:29:36Z");
        line(&mut g, ("N-1104.p681", NodeKind::Post), ("N-1104.p680", NodeKind::Post), EdgeKind::Answered, 5, "2026-11-21T08:29:36Z");
        line(&mut g, (owner, NodeKind::Agent), ("N-1104.p682", NodeKind::Post), EdgeKind::Wrote, 6, "2026-11-21T08:30:00Z");
        line(&mut g, ("N-1104.p682", NodeKind::Post), ("agent:paper-research", NodeKind::Agent), EdgeKind::Asks, 6, "2026-11-21T08:30:00Z");
        line(&mut g, ("N-1104.p682", NodeKind::Post), ("N-1104.p681", NodeKind::Post), EdgeKind::Answered, 6, "2026-11-21T08:30:00Z");
        line(&mut g, ("agent:docket-core", NodeKind::Agent), ("N-1104.p676", NodeKind::Post), EdgeKind::Wrote, 7, "2026-11-21T08:21:00Z");
        line(&mut g, ("N-1104.p676", NodeKind::Post), ("N-1104.p677", NodeKind::Post), EdgeKind::Answered, 7, "2026-11-21T08:21:00Z");
        // two more of his: p690, reached and taken by an item whose note it is; p695, reached and never taken
        line(&mut g, (owner, NodeKind::Agent), ("N-1104.p690", NodeKind::Post), EdgeKind::Wrote, 10, "2026-11-21T08:50:00Z");
        line(&mut g, ("N-1104.p690", NodeKind::Post), ("agent:accounts-2", NodeKind::Agent), EdgeKind::Asks, 10, "2026-11-21T08:50:00Z");
        line(&mut g, ("N-1104.p690", NodeKind::Post), ("agent:accounts-2", NodeKind::Agent), EdgeKind::Handed, 11, "2026-11-21T08:55:00Z");
        line(&mut g, ("I-1501", NodeKind::Ticket), ("N-1104.p690", NodeKind::Post), EdgeKind::CameFrom, 12, "2026-11-21T08:56:00Z");
        line(&mut g, ("agent:accounts-2", NodeKind::Agent), ("I-1501", NodeKind::Ticket), EdgeKind::Started, 13, "2026-11-21T09:00:00Z");
        line(&mut g, (owner, NodeKind::Agent), ("N-1104.p695", NodeKind::Post), EdgeKind::Wrote, 14, "2026-11-21T09:05:00Z");
        line(&mut g, ("N-1104.p695", NodeKind::Post), ("agent:paper-research", NodeKind::Agent), EdgeKind::Asks, 14, "2026-11-21T09:05:00Z");
        line(&mut g, ("N-1104.p695", NodeKind::Post), ("agent:paper-research", NodeKind::Agent), EdgeKind::Handed, 15, "2026-11-21T09:06:00Z");
        // docket-edge8's: the commit's subject names his post
        g.edge_from("commit:de447f86", post, EdgeKind::Named, Source::Commit("de447f86".into()), Some("2026-11-21T08:41:33Z".into()));

        let (old, work, other, merge) = (full("b49e1ed3"), full("de447f86"), full("f0591f86"), full("4ee5c14e"));
        let log = [
            entry(&merge, 1795250754, &[&other, &work], "merge acc2\n\nItem: I-1469\n\nCo-Authored-By: C\nAgent: proto-lead\nSession: 27dc\n"),
            entry(&other, 1795250600, &[&old], "train work\n\nAgent: docket-core\n"),
            entry(&work, 1795250493, &[&old], "Accounts tab (owner N-1104 p678)\n\nItem: I-1469\nAgent: accounts-2\nSession: ec4b\n"),
            entry(&old, 1795240000, &[], "release 58's head\n"),
        ]
        .concat();
        let commits = Commit::parse_log(&log, "soft-swarm");
        assert_eq!(commits.iter().map(|c| c.items.clone()).collect::<Vec<_>>(), vec![vec!["I-1469".to_string()], vec![], vec!["I-1469".into()], vec![]]);
        let job = |id: &str, since: &str, at: &str, rc: Option<i64>, step: &str| Job { id: id.into(), agent: "accounts-2".into(), commit: "de447f86".into(), verb: "test".into(), packages: vec!["yaaihh".into()], since: since.into(), at: at.into(), step: step.into(), rc, ..Default::default() };
        // and three for what they were for (their `items`, p1422 4), on a commit that names no post of his: green for
        // the item whose note p690 is; red, then green, for p695
        let for_ = |mut j: Job, commit: &str, items: &[&str]| {
            j.commit = commit.into();
            j.items = items.iter().map(|i| i.to_string()).collect();
            j
        };
        let jobs = [
            job("1-1", "2026-11-21T09:55:00+0100", "2026-11-21T09:56:00+0100", Some(101), "ended"),
            job("2-2", "2026-11-21T09:58:10+0100", "2026-11-21T09:59:00+0100", Some(0), "ended"),
            job("3-3", "2026-11-21T09:57:00+0100", "2026-11-21T09:57:30+0100", None, "build"),
            for_(job("4-4", "2026-11-21T10:01:00+0100", "2026-11-21T10:02:00+0100", Some(0), "ended"), "f0591f86", &["I-1501"]),
            for_(job("5-5", "2026-11-21T10:06:00+0100", "2026-11-21T10:07:00+0100", Some(101), "ended"), "b49e1ed3", &["N-1104.p695"]),
            for_(job("6-6", "2026-11-21T10:07:30+0100", "2026-11-21T10:08:00+0100", Some(0), "ended"), "b49e1ed3", &["N-1104.p695"]),
        ];
        add(&mut g, &commits, &jobs);

        let node = |id: &str| g.get(id).cloned().unwrap();
        let Node { source, when, what, .. } = node("commit:4ee5c14e");
        assert_eq!((source, when.as_deref(), what.as_str()), (Source::Line(9), Some("2026-11-21T08:45:54Z"), "merge acc2"), "a commit the record met first keeps that line, and takes git's time and subject");
        assert_eq!((node("job:2-2").ok, node("job:1-1").ok, node("job:3-3").ok), (Some(true), Some(false), None));
        let edges = |k: EdgeKind| -> Vec<(String, String, Source)> { g.edges.iter().filter(|e| e.kind == k).map(|e| (e.from.clone(), e.to.clone(), e.source.clone())).collect() };
        assert_eq!(edges(EdgeKind::MergedInto), vec![("commit:de447f86".into(), "commit:4ee5c14e".into(), Source::Commit("4ee5c14e".into()))]);
        let came: Vec<(String, String)> = edges(EdgeKind::CameFrom).into_iter().filter(|(_, _, s)| *s == Source::Commit("4ee5c14e".into())).map(|(a, b, _)| (a, b)).collect();
        assert_eq!(came, vec![("commit:4ee5c14e".into(), "session:27dc".into()), ("commit:4ee5c14e".into(), "commit:f0591f86".into())]);
        let rel_src = Source::Release("20261121-101928-4ee5c14e".into());
        let mut carried: Vec<String> = edges(EdgeKind::BuiltInto).into_iter().filter(|(_, _, s)| *s == rel_src).map(|(c, _, _)| c).collect();
        carried.sort();
        assert_eq!(carried, vec!["commit:de447f86".to_string(), "commit:f0591f86".into()], "the release carries what its head reaches and the one it follows does not; its head's edge is the record's");

        let t = g.ask_times();
        assert_eq!(t.iter().map(|a| a.post.as_str()).collect::<Vec<_>>(), vec![post, "N-1104.p682", "N-1104.p690", "N-1104.p695"]);
        let r = |at: &str, id: &str, via: Option<&str>| Some(crate::graph::Reached { at: at.into(), id: id.into(), via: via.map(String::from) });
        assert_eq!(t[0].asked.as_deref(), Some("2026-11-21T08:19:19Z"));
        assert_eq!(t[0].reached, r("2026-11-21T08:20:05Z", "agent:paper-research", Some("N-1104.p679")), "a session handed a later post was handed it; one handed only an earlier was not");
        assert_eq!(t[0].taken, r("2026-11-21T08:29:36Z", "agent:paper-research", Some("N-1104.p681")), "an agent's answer to a later post took it; his own answer and an answer to an earlier post did not");
        assert_eq!(t[0].first_commit, r("2026-11-21T08:41:33Z", "commit:de447f86", None));
        assert_eq!(t[0].merged, r("2026-11-21T08:45:54Z", "commit:4ee5c14e", None));
        assert_eq!(t[0].first_green, r("2026-11-21T08:59:00Z", "job:2-2", None), "the red job and the one still building are not green");
        assert_eq!((t[1].reached.clone(), t[1].taken.clone()), (r("2026-11-21T08:55:00Z", "agent:accounts-2", Some("N-1104.p690")), None), "his p682: answered by nobody past it");
        assert_eq!((t[2].reached.clone(), t[2].taken.clone()), (r("2026-11-21T08:55:00Z", "agent:accounts-2", Some("N-1104.p690")), r("2026-11-21T09:00:00Z", "agent:accounts-2", Some("I-1501"))));
        assert_eq!((t[3].reached.clone(), t[3].taken.clone(), t[3].first_commit.clone()), (r("2026-11-21T09:06:00Z", "agent:paper-research", Some("N-1104.p695")), None, None), "reached and never taken: the one he needs to see");
        assert_eq!(t[0].in_window, r("2026-11-21T09:39:20Z", rel, None));
        // what the jobs were for: p690's item's green job, p695's green one and not its red; p678's stays its commit's
        let green: Vec<_> = t.iter().map(|a| a.first_green.clone()).collect();
        assert_eq!(green, [r("2026-11-21T08:59:00Z", "job:2-2", None), None, r("2026-11-21T09:02:00Z", "job:4-4", None), r("2026-11-21T09:08:00Z", "job:6-6", None)]);
        assert_eq!((t[2].first_commit.clone(), t[3].first_commit.clone()), (None, None), "a job's commit is not the ask's");
    }

    /// A branch's moves from git's reflog, the first branch of a commit the one it was made on; and a job's record lines
    /// and its file one job, the record's; a job → each item or post it was for, `named`, at its start (paper-research
    /// N-1446 p1422, 4), and nothing for one that names none or names no place.
    #[test]
    fn a_branch_comes_from_the_reflog_and_a_jobs_lines_and_file_are_one_job() {
        let (a, b) = (full("21fed2f4"), full("0f40917b"));
        let log = format!(
            "refs/heads/main@{{1795338146}}\x1f{b}\x1fmerge 0f40917b: Fast-forward\x1e\nworktrees/x/HEAD@{{1795337100}}\x1f{a}\x1fcommit: y\x1e\nrefs/heads/main@{{1795337300}}\x1f{a}\x1fmerge 21fed2f4: Fast-forward\x1e\nrefs/heads/edge5-graph@{{1795337036}}\x1f{a}\x1fcommit: docket-machine graph: reached and taken\x1e\n"
        );
        let moves = Moved::parse_log(&log, "soft-swarm");
        assert_eq!(moves.iter().map(|m| (m.branch.as_str(), &m.hash[..8], m.at.as_str(), m.how.as_str())).collect::<Vec<_>>(), vec![("main", "0f40917b", "2026-11-22T09:02:26Z", "merge 0f40917b: Fast-forward"), ("main", "21fed2f4", "2026-11-22T08:48:20Z", "merge 21fed2f4: Fast-forward"), ("edge5-graph", "21fed2f4", "2026-11-22T08:43:56Z", "commit: docket-machine graph: reached and taken")], "a worktree's HEAD is no branch");
        let mut g = Graph::default();
        add_moves(&mut g, &moves);
        let on: Vec<(String, String, Option<String>)> = g.edges.iter().map(|e| (e.from.clone(), e.to.clone(), e.when.clone())).collect();
        assert_eq!(
            on,
            vec![
                ("commit:0f40917b".to_string(), "branch:soft-swarm/main".to_string(), Some("2026-11-22T09:02:26Z".to_string())),
                ("commit:21fed2f4".into(), "branch:soft-swarm/edge5-graph".into(), Some("2026-11-22T08:43:56Z".into())),
                ("commit:21fed2f4".into(), "branch:soft-swarm/main".into(), Some("2026-11-22T08:48:20Z".into())),
            ]
        );

        // job 9-1 is in the record (its start and end lines) and in a file; job 8-1 in a file alone
        let m = crate::machine::Machine::load(Default::default());
        let l = |v: serde_json::Value| -> crate::line::Line { serde_json::from_value(v).unwrap() };
        let mut bld = crate::graph::Builder::default();
        let line = |phase: &str, at: &str, end: &str| l(serde_json::from_str(&format!(r#"{{"ev": "act", "at": "{at}", "agent": "docket-core", "is": "job", "text": "job", "phase": "{phase}", "id": "9-1", "since": "2026-11-22T09:30:00+0100", "verb": "test", "packages": ["coordd"], "commit": "a2c839a6", "items": ["I-1469", "N-1104.p678"]{end}}}"#)).unwrap());
        bld.after(&m, 0, &[line("start", "2026-11-22T08:30:00Z", "")]);
        bld.after(&m, 1, &[line("end", "2026-11-22T08:31:00Z", r#", "rc": "101", "passed": "35", "failed": "1""#)]);
        let mut g = bld.graph;
        let file = |id: &str, commit: &str| Job { id: id.into(), agent: "docket-core".into(), commit: commit.into(), verb: "test".into(), since: "2026-11-22T09:30:00+0100".into(), at: "2026-11-22T09:31:00+0100".into(), step: "ended".into(), rc: Some(0), ..Default::default() };
        let mut eight = file("8-1", "33f4fb22");
        eight.items = vec!["N-1104".into(), " I-1469 ".into(), "N-1104.s3".into()];
        add(&mut g, &[], &[file("9-1", "a2c839a6"), eight]);
        let jobs: Vec<(String, String, EdgeKind, Source)> = g.edges.iter().map(|e| (e.from.clone(), e.to.clone(), e.kind, e.source.clone())).collect();
        let (j9, j8) = (Source::Line(1), Source::Job("8-1".into()));
        assert_eq!(
            jobs,
            vec![
                ("agent:docket-core".to_string(), "job:9-1".to_string(), EdgeKind::Ran, j9.clone()),
                ("job:9-1".into(), "commit:a2c839a6".into(), EdgeKind::Ran, j9.clone()),
                ("job:9-1".into(), "I-1469".into(), EdgeKind::Named, j9.clone()),
                ("job:9-1".into(), "N-1104.p678".into(), EdgeKind::Named, j9),
                ("job:9-1".into(), "commit:a2c839a6".into(), EdgeKind::Ended, Source::Line(2)),
                ("agent:docket-core".into(), "job:8-1".into(), EdgeKind::Ran, j8.clone()),
                ("job:8-1".into(), "commit:33f4fb22".into(), EdgeKind::Ran, j8.clone()),
                ("job:8-1".into(), "N-1104".into(), EdgeKind::Named, j8.clone()),
                ("job:8-1".into(), "I-1469".into(), EdgeKind::Named, j8.clone()),
                ("job:8-1".into(), "commit:33f4fb22".into(), EdgeKind::Ended, j8),
            ],
            "the record's job is not read again from its file"
        );
        assert_eq!((g.get("job:9-1").unwrap().ok, g.get("job:9-1").unwrap().what.as_str()), (Some(false), "test coordd: rc 101, 35 passed, 1 failed"));
        let kinds: Vec<_> = ["I-1469", "N-1104.p678", "N-1104"].iter().map(|id| g.get(id).map(|n| n.kind)).collect();
        assert_eq!(kinds, [Some(NodeKind::Ticket), Some(NodeKind::Post), Some(NodeKind::Ticket)]);
        let named = |j: &str| g.edges.iter().find(|e| e.from == j && e.kind == EdgeKind::Named).and_then(|e| e.when.clone());
        assert_eq!((named("job:9-1"), named("job:8-1")), (Some("2026-11-22T08:30:00Z".into()), Some("2026-11-22T08:30:00Z".into())), "at its start");
    }

    /// A commit's tree and files from the log under `--name-only`, a merge's none; and the facts a decision on a commit
    /// is handed (proto-mind N-1446 p1502 (1), the facts draft 2's dev.eflint declares): what a head adds over the
    /// train's tip it was made on, the packages its files are in and its items; the workspace's packages and their uses;
    /// the jobs on its tree as their lines say them (a refusal, a failing test, the tests it named or whole: docket-core's
    /// p1515 fields, p1519's one reader), a start after its end undoing nothing (proto-window p1565); and what it changes
    /// over the commit production's coordd runs by the release acts, taken by `is` and production's checkout, a scratch
    /// tree's switch not (p1541).
    #[test]
    fn a_commits_facts_come_from_the_log_its_trees_jobs_the_packages_and_the_release_acts() {
        let (m1, h1, h2, t2) = (full("5d852eb8"), full("a65bb8e4"), full("e7ab6de8"), full("7c0ffee1"));
        let log = [
            entry_of(&t2, 1795360000, &[&m1, &h2], "77t2", "merge e7ab6de8 into train 68\n\nItem: N-1446\n", &[]),
            entry_of(&h2, 1795359000, &[&h1], "77h2", "agent-base: words\n\nItem: N-1446\n", &["agent-base/bin/coordc"]),
            entry_of(&h1, 1795358000, &[&m1], "77h1", "docket-machine graph: facts\n\nItem: I-1478\n", &["yaaihh/crates/docket-machine/src/graph_git.rs", "yaaihh/crates/docket-machine/src/graph.rs"]),
            entry_of(&m1, 1795357000, &[], "77m1", "train 67\n", &["yaaihh/crates/coordd/src/main.rs", "README.md"]),
        ]
        .concat();
        let commits = Commit::parse_log(&log, "soft-swarm");
        let tf = |h: &str| commits.iter().find(|c| c.hash == h).map(|c| (c.tree.clone(), c.files.clone(), c.parents.len())).unwrap();
        assert_eq!(tf(&h1), ("77h1".to_string(), vec!["yaaihh/crates/docket-machine/src/graph_git.rs".to_string(), "yaaihh/crates/docket-machine/src/graph.rs".into()], 1));
        assert_eq!((tf(&t2), commits[0].items.clone()), (("77t2".to_string(), vec![], 2), vec!["N-1446".to_string()]), "a merge lists no files");

        let pk = Packages {
            repo: "soft-swarm".into(),
            dirs: [("yaaihh/crates/coordd", "coordd"), ("yaaihh/crates/docket-machine", "docket-machine"), ("yaaihh/crates/docket-machine/tries/speed", "speed")].map(|(d, n)| (d.to_string(), n.to_string())).into(),
            members: ["coordd", "docket-machine"].map(String::from).into(),
            uses: [("coordd".to_string(), "docket-machine".to_string())].into(),
        };
        let of = ["yaaihh/crates/docket-machine/src/graph.rs", "yaaihh/crates/docket-machine/tries/speed/src/main.rs", "yaaihh/crates/docket-machine-old/x.rs", "agent-base/bin/coordc"].map(|f| pk.of(f));
        assert_eq!(of, [Some("docket-machine"), Some("speed"), None, None], "the nearest package folder holding the file");

        // the record: jobs' lines as build-runner writes them through coordc (each value a string), release acts
        let m = crate::machine::Machine::load(Default::default());
        let l = |s: String| -> crate::line::Line { serde_json::from_str(&s).unwrap() };
        let job = |phase: &str, at: &str, id: &str, pkg: &str, tree: &str, more: &str| {
            l(format!(r#"{{"ev": "act", "at": "{at}", "agent": "docket-edge5", "is": "job", "text": "job", "phase": "{phase}", "id": "{id}", "since": "2026-11-22T16:00:00+0100", "verb": "test", "packages": "[\"{pkg}\"]", "commit": "e7ab6de8", "tree": "{tree}"{more}}}"#))
        };
        let release = |at: &str, phase: &str, checkout: &str, coord: &str, commit: &str| {
            l(format!(r#"{{"ev": "act", "at": "{at}", "agent": "docket-core", "is": "release", "text": "release {phase}", "phase": "{phase}", "checkout": "{checkout}", "coord": "{coord}", "coordd": "20261122-1500", "coordd_commit": "{commit}"}}"#))
        };
        let named = r#", "tests": "[\"graph\", \"tests::ran::\"]", "whole": """#;
        let prod = "/home/t/Documents/papers/soft-swarm";
        let lines = vec![
            release("2026-11-22T14:00:00Z", "switch", prod, "research", &m1),
            job("start", "2026-11-22T15:00:00Z", "100-1", "docket-machine", "77h2", named),
            job("end", "2026-11-22T15:01:00Z", "100-1", "docket-machine", "77h2", &format!(r#"{named}, "rc": "101", "passed": "35", "failed": "1", "failing": "[\"graph::tests::now_gives\"]""#)),
            job("end", "2026-11-22T15:02:00Z", "101-2", "coordd", "77h2", r#", "tests": "[]", "whole": "1", "rc": "3", "refused": "a whole run of coordd: refused (p206)""#),
            job("start", "2026-11-22T15:03:00Z", "100-1", "docket-machine", "77h2", named),
            job("end", "2026-11-22T15:04:00Z", "102-3", "docket-machine", "77h1", r#", "rc": "0", "passed": "36", "failed": "0""#),
            release("2026-11-22T15:05:00Z", "switch", "/home/t/scratch/train-68", "research", &h1),
            release("2026-11-22T15:06:00Z", "cut", prod, "", &t2),
            release("2026-11-22T15:07:00Z", "coord_back", prod, "box", &h2),
        ];
        let mut bld = crate::graph::Builder::default();
        bld.after(&m, 0, &lines);
        let mut g = bld.graph;
        assert_eq!(g.releases.len(), 4, "each release act, by its `is`");
        let moved = |b: &str, h: &str, at: &str| Moved { repo: "soft-swarm".into(), branch: b.into(), hash: h.to_string(), at: at.into(), how: String::new() };
        add_moves(&mut g, &[moved("main", &m1, "2026-11-22T13:56:40Z"), moved("edge5-facts", &h1, "2026-11-22T14:03:20Z"), moved("edge5-facts", &h2, "2026-11-22T14:20:00Z"), moved("train-68", &t2, "2026-11-22T14:36:40Z")]);

        let live = live_commit(&g, "research", &format!("{prod}/"));
        assert_eq!(live, Some(m1.as_str()), "production's switch of research's coordd: a scratch tree's, a cut and another coord's return are not");
        let fact = |f: &str, a: &[&str]| (f.to_string(), a.iter().map(|x| x.to_string()).collect::<Vec<_>>());
        let mut want = vec![
            fact("tree-of", &["e7ab6de8", "77h2"]),
            fact("changes", &["e7ab6de8", "docket-machine"]),
            fact("names-item", &["e7ab6de8", "I-1478"]),
            fact("names-item", &["e7ab6de8", "N-1446"]),
            fact("package", &["coordd"]),
            fact("package", &["docket-machine"]),
            fact("uses", &["coordd", "docket-machine"]),
            fact("job", &["100-1", "77h2"]),
            fact("job-package", &["100-1", "docket-machine"]),
            fact("job-verb", &["100-1", "test"]),
            fact("job-test", &["100-1", "graph"]),
            fact("job-test", &["100-1", "tests::ran::"]),
            fact("job-rc", &["100-1", "101"]),
            fact("job-passed", &["100-1", "35"]),
            fact("job-failed", &["100-1", "1"]),
            fact("job-failing", &["100-1", "graph::tests::now_gives"]),
            fact("job", &["101-2", "77h2"]),
            fact("job-package", &["101-2", "coordd"]),
            fact("job-verb", &["101-2", "test"]),
            fact("job-whole", &["101-2"]),
            fact("job-rc", &["101-2", "3"]),
            fact("job-refused", &["101-2", "a whole run of coordd: refused (p206)"]),
            fact("live-changes", &["e7ab6de8", "docket-machine"]),
        ];
        want.sort();
        assert_eq!(facts(&g, &commits, std::slice::from_ref(&pk), "e7ab6de8", live), want, "the head: its own two commits over main's tip, its tree's jobs");

        // the train's merge adds the head's commits over main's tip; its tree has no job yet
        let mut want = vec![
            fact("tree-of", &["7c0ffee1", "77t2"]),
            fact("changes", &["7c0ffee1", "docket-machine"]),
            fact("names-item", &["7c0ffee1", "I-1478"]),
            fact("names-item", &["7c0ffee1", "N-1446"]),
            fact("package", &["coordd"]),
            fact("package", &["docket-machine"]),
            fact("uses", &["coordd", "docket-machine"]),
            fact("live-changes", &["7c0ffee1", "docket-machine"]),
        ];
        want.sort();
        assert_eq!(facts(&g, &commits, std::slice::from_ref(&pk), "7c0ffee1", live), want);
        let main: Vec<_> = facts(&g, &commits, std::slice::from_ref(&pk), &m1, live).into_iter().filter(|(f, _)| f.contains("changes")).collect();
        assert_eq!(main, vec![fact("changes", &[m1.as_str(), "coordd"])], "main's tip adds itself; it changes nothing over itself");
        assert_eq!(facts(&g, &commits, std::slice::from_ref(&pk), "a65bb8e4", None).iter().filter(|(f, _)| f.starts_with("job")).count(), 6, "job 102-3, on its tree, ended: job, package, verb, rc, passed, failed");
        assert!(facts(&g, &commits, &[pk.clone()], "deadbeef", live).is_empty() && facts(&g, &commits, &[pk], "e7ab", live).is_empty(), "no commit read, or too short to say one");
    }

    /// A replay job's facts on the tree it gates (proto-mind N-1446 p1765, the cut's `gates`): the record it replayed and
    /// which way, as build-runner's `replay` lines say them (5bdbb76b), and once it has ended the counts its test
    /// printed; a replay whose test printed none says none, so no rule can read it as clean.
    #[test]
    fn a_replay_jobs_record_way_and_counts_are_facts_of_the_tree_it_ran_for() {
        let h = full("b16b00b5");
        let commits = Commit::parse_log(&entry_of(&h, 1795360000, &[], "88t1", "train 69's tip\n", &[]), "soft-swarm");
        let m = crate::machine::Machine::load(Default::default());
        let l = |s: String| -> crate::line::Line { serde_json::from_str(&s).unwrap() };
        let replay = |phase: &str, id: &str, record: &str, way: &str, more: &str| {
            l(format!(r#"{{"ev": "act", "at": "2026-11-22T23:00:00Z", "agent": "docket-core", "is": "job", "text": "job", "phase": "{phase}", "id": "{id}", "since": "2026-11-23T00:00:00+0100", "verb": "replay", "packages": "[]", "commit": "b16b00b5", "tree": "88t1", "record": "{record}", "way": "{way}", "back": "", "tests": "[\"ticket::tests::a_coords_record_replays\"]"{more}}}"#))
        };
        let lines = vec![
            replay("start", "200-1", "research", "forward", ""),
            replay("end", "200-1", "research", "forward", r#", "rc": "0", "passed": "1", "failed": "0", "problem_lines": "0", "now_makes": "0", "now_refused": "0""#),
            replay("end", "201-2", "written-a", "back", r#", "rc": "0", "passed": "1", "failed": "0", "problem_lines": "2", "now_makes": "0", "now_refused": "1""#),
            replay("end", "202-3", "cav-paper", "forward", r#", "rc": "0", "passed": "1", "failed": "0""#),
        ];
        let mut bld = crate::graph::Builder::default();
        bld.after(&m, 0, &lines);
        let fact = |f: &str, a: &[&str]| (f.to_string(), a.iter().map(|x| x.to_string()).collect::<Vec<_>>());
        let replayed: Vec<_> = facts(&bld.graph, &commits, &[], "b16b00b5", None).into_iter().filter(|(f, _)| ["job-record", "job-way", "job-problem-lines", "job-now-makes", "job-now-refused"].contains(&f.as_str())).collect();
        let mut want = vec![
            fact("job-record", &["200-1", "research"]),
            fact("job-way", &["200-1", "forward"]),
            fact("job-problem-lines", &["200-1", "0"]),
            fact("job-now-makes", &["200-1", "0"]),
            fact("job-now-refused", &["200-1", "0"]),
            fact("job-record", &["201-2", "written-a"]),
            fact("job-way", &["201-2", "back"]),
            fact("job-problem-lines", &["201-2", "2"]),
            fact("job-now-makes", &["201-2", "0"]),
            fact("job-now-refused", &["201-2", "1"]),
            fact("job-record", &["202-3", "cav-paper"]),
            fact("job-way", &["202-3", "forward"]),
        ];
        want.sort();
        assert_eq!(replayed, want, "each replay's record and way; the counts of those whose test printed them");
        // a job file says the counts as numbers, its lines as strings: one job either way
        let file = Job::parse("201-2", r#"{"verb": "replay", "tree": "88t1", "record": "written-a", "way": "back", "step": "ended", "rc": 0, "problem_lines": 2, "now_makes": 0, "now_refused": 1}"#).unwrap();
        assert_eq!((file.record.as_str(), file.way.as_str(), file.problem_lines, file.now_makes, file.now_refused), ("written-a", "back", Some(2), Some(0), Some(1)));
    }

    /// A job's `runs` line (build-runner's, as the job leaves its queue; docket-edge5 N-1446 p3123) is a step of the job:
    /// it adds no second `ran` edge, a running job is no ended one, and a start line that comes after it takes it back
    /// no more than one after the end does.
    #[test]
    fn a_jobs_runs_line_is_a_step_of_it() {
        let m = crate::machine::Machine::load(Default::default());
        let l = |s: String| -> crate::line::Line { serde_json::from_str(&s).unwrap() };
        let job = |phase: &str, at: &str, more: &str| {
            l(format!(r#"{{"ev": "act", "at": "{at}", "agent": "docket-edge5", "is": "job", "text": "job", "phase": "{phase}", "id": "3-3", "since": "2026-11-24T21:20:00+0100", "verb": "test", "packages": "[\"docket-machine\"]", "commit": "e7ab6de8", "tree": "77h2"{more}}}"#))
        };
        let mut bld = crate::graph::Builder::default();
        bld.after(&m, 0, &[job("start", "2026-11-24T20:20:00Z", ""), job("runs", "2026-11-24T20:21:00Z", ""), job("start", "2026-11-24T20:21:01Z", "")]);
        let ran = |g: &Graph| g.edges.iter().filter(|e| e.kind == EdgeKind::Ran && e.to == "job:3-3").count();
        assert_eq!((bld.graph.jobs["3-3"].step.as_str(), bld.graph.get("job:3-3").unwrap().ok), ("runs", None), "running, and a start after it takes it back to no earlier step");
        bld.after(&m, 3, &[job("end", "2026-11-24T20:30:00Z", r#", "rc": "0", "passed": "4", "failed": "0""#)]);
        assert_eq!((bld.graph.jobs["3-3"].step.as_str(), bld.graph.get("job:3-3").unwrap().ok), ("ended", Some(true)));
        assert_eq!(ran(&bld.graph), 2, "its agent's `ran` edge from each start line, none from `runs` or the end");
    }

    /// Where a commit was made, from each worktree's own HEAD reflog (paper-research N-1446 p1530, p1554): the worktree
    /// whose reflog first has git make it, by its entry's kind; a checkout, a reset or a fast-forward makes none there,
    /// and who made it is never read.
    #[test]
    fn a_commit_is_in_the_worktree_whose_head_reflog_first_made_it() {
        let (a, b, c, d) = (full("21fed2f4"), full("0f40917b"), full("5a3ad6d7"), full("e1bc9f7a"));
        let z = "0".repeat(40);
        let e = |old: &str, new: &str, secs: i64, msg: &str| format!("{old} {new} Some One <one@x> {secs} +0100\t{msg}\n");
        let main = [
            e(&z, &a, 1795337000, "commit (initial): first"),
            e(&a, &b, 1795337100, "merge side: Fast-forward"),
            e(&b, &a, 1795337200, "reset: moving to HEAD~1"),
            e(&a, &c, 1795337300, "merge side: Merge made by the 'ort' strategy."),
            e(&c, &d, 1795337400, "cherry-pick: fast-forward"),
        ]
        .concat();
        // a worktree's first entry, its `worktree add`, has no words
        let wt = [
            format!("{z} {a} Some One <one@x> 1795337050 +0100\n"),
            e(&a, &b, 1795337060, "commit: on the side"),
            e(&b, &d, 1795337070, "commit (amend): on the side"),
            e(&d, &d, 1795337080, "checkout: moving from side to main"),
        ]
        .concat();
        let mut made = MadeIn::parse_reflog(&main, "soft", "/w/main");
        made.extend(MadeIn::parse_reflog(&wt, "soft", "/w/wt"));
        let rows = |made: &[MadeIn]| made.iter().map(|m| (m.hash[..8].to_string(), m.top.clone(), m.how.clone(), m.at.clone())).collect::<Vec<_>>();
        let row = |h: &str, top: &str, how: &str, secs: i64| (h.to_string(), top.to_string(), how.to_string(), crate::machine::at_secs(secs));
        assert_eq!(
            rows(&made),
            [row("21fed2f4", "/w/main", "commit (initial)", 1795337000), row("5a3ad6d7", "/w/main", "merge side", 1795337300), row("0f40917b", "/w/wt", "commit", 1795337060), row("e1bc9f7a", "/w/wt", "commit (amend)", 1795337070)]
        );
        assert!(made.iter().all(|m| !format!("{m:?}").contains("one@x") && !format!("{m:?}").contains("Some One")), "who made it is not read");
        // 0f40917b again, in a third worktree's rebase that took its pick as it was: still where it was made first
        let other = MadeIn::parse_reflog(&e(&a, &b, 1795338000, "rebase (pick): on the side"), "soft", "/w/other");
        assert_eq!(rows(&other), [row("0f40917b", "/w/other", "rebase (pick)", 1795338000)]);
        let mut g = Graph::default();
        add_made_in(&mut g, &[other, made].concat());
        let ins: Vec<(String, String, EdgeKind, Source, Option<String>)> = g.edges.iter().map(|e| (e.from.clone(), e.to.clone(), e.kind, e.source.clone(), e.when.clone())).collect();
        let edge = |h: &str, top: &str, secs: i64| (commit_id(h), worktree_id(top), EdgeKind::In, Source::Commit(h.into()), Some(crate::machine::at_secs(secs)));
        assert_eq!(ins, [edge("0f40917b", "/w/wt", 1795337060), edge("21fed2f4", "/w/main", 1795337000), edge("5a3ad6d7", "/w/main", 1795337300), edge("e1bc9f7a", "/w/wt", 1795337070)]);
        assert_eq!(["/w/main", "/w/wt", "/w/other"].map(|t| g.get(&worktree_id(t)).map(|n| n.kind)), [Some(NodeKind::Worktree), Some(NodeKind::Worktree), None], "a worktree that made nothing first has no node from here");
        // the kinds that make a commit, and those that do not
        for (how, rest, makes) in [
            ("commit (merge)", "x", true),
            ("commit (cherry-pick)", "x", true),
            ("cherry-pick", "x", true),
            ("revert", "Revert \"x\"", true),
            ("am", "x", true),
            ("rebase -i (pick)", "x", true),
            ("rebase (squash)", "x", true),
            ("rebase (continue)", "x", true),
            ("pull --rebase (pick)", "x", true),
            ("pull", "Merge made by the 'ort' strategy.", true),
            ("merge a b", "Merge made by the 'octopus' strategy.", true),
            ("rebase (start)", "checkout x", false),
            ("rebase (finish)", "returning to refs/heads/x", false),
            ("pull", "Fast-forward", false),
            ("merge x", "updating HEAD", false),
            ("branch", "Created from HEAD", false),
            ("", "", false),
        ] {
            assert_eq!(makes_commit(how, rest), makes, "{how}: {rest}");
        }
    }
}
