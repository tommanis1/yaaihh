//! Docket in coordd: the adapter around the pure machine (`docket_machine`, owner m613 change 4). It reads the kind
//! and call files, owns the log `$COORD/tickets/log.jsonl` and replays it at start, applies inputs under the core's
//! lock, appends the lines through the writer thread, and carries out the machine's effects: questions to policy and
//! the forming of heads off the lock (a task of its own), deliveries through the hand-over of `core.rs`, views as
//! events.

use crate::core::{append, err, now, ok, Core, Who};
use crate::lapse::BOX;
use coord_types::{Body, Reply};
use docket_machine::{CallSig, Defs, Delivery, Effect, Input, Line, Loc, Machine, PostKind, Question, Text, TicketView, Type};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use tokio::sync::Mutex;

mod violated;
pub mod mentions;
pub(crate) mod text_places;
pub(crate) mod work_repos;
pub(crate) mod replay_state;
pub(crate) mod effects;
mod comments;
/// Where the step STEP of T that AGENT's answer answers was shown to it: the last time it was handed again in its
/// layer after a move there (`sN.back`, paper-research N-101 p629), else its first delivery (not a reminder or a
/// refusal); its place in the agent's session.
pub(crate) fn shown_place(m: &docket_machine::Machine, t: u64, step: u32, agent: &str) -> Option<Loc> {
    let d = format!("{}.s{step}", m.id(t));
    let given = &m.tickets.get(&t)?.given;
    let back = format!("{d}.back");
    given.iter().rev().find(|g| g.delivery == back && g.to == agent && g.place.is_some()).or_else(|| given.iter().find(|g| g.delivery == d && g.to == agent && g.place.is_some())).and_then(|g| g.place.clone())
}

/// The outline's rows (`coord outline`): every open thread and item, and each closed one (ended, done, dropped) whose
/// parent in the outline is open, so that the parent's line for it stays (paper-research N-447). Without children.
pub fn outline_rows(m: &docket_machine::Machine) -> Vec<docket_machine::ViewOutline> {
    let rows: Vec<docket_machine::ViewOutline> = m.tickets.values().filter(|x| x.t < docket_machine::HIDDEN && docket_machine::view::outlined(m, x)).map(|x| docket_machine::view::ViewOutline::of(m, x, false)).collect();
    let open: BTreeSet<String> = rows.iter().filter(|o| o.open).map(|o| o.id.clone()).collect();
    rows.into_iter().filter(|o| o.open || o.parent.as_ref().is_some_and(|p| open.contains(p))).collect()
}

/// Why a point FROM of a message split by point is refused before anything is sent (N-696), or None: only the owner's
/// messages are split, and a point is one of two or more, from 1, of a message with a key.
pub fn split_refused(who: &Who, f: &docket_machine::SplitFrom) -> Option<String> {
    if !matches!(who, Who::Owner) {
        return Some("only the owner's messages are split by point".into());
    }
    if f.message.trim().is_empty() || f.of < 2 || f.point < 1 || f.point > f.of {
        return Some(format!("point {} of {} of message {:?} is no point of a split; nothing was sent", f.point, f.of, f.message));
    }
    None
}

#[cfg(test)]
mod found_tests;
#[cfg(test)]
#[path = "../tests/cost.rs"]
mod cost_tests;
#[cfg(test)]
mod decide_tests;
#[cfg(test)]
mod claims_tests;
#[cfg(test)]
mod assign_tests;
mod quiet;
#[cfg(test)]
mod quiet_tests;
#[cfg(test)]
mod effects_tests;
#[cfg(test)]
mod joint_off_tests;
#[cfg(test)]
mod edit_tests;
#[cfg(test)]
mod record_tests;
#[cfg(test)]
#[path = "../tests/live_edits.rs"]
mod live_edits_tests;
#[cfg(test)]
#[path = "../tests/text_change.rs"]
mod text_change_tests;
#[cfg(test)]
mod waiting_tests;
#[cfg(test)]
#[path = "../tests/thread_end.rs"]
mod thread_end_tests;
#[cfg(test)]
#[path = "../tests/schedule.rs"]
mod schedule_tests;
#[cfg(test)]
#[path = "../tests/signed_off.rs"]
mod signed_off_tests;
#[cfg(test)]
#[path = "../tests/archive.rs"]
mod archive_tests;
#[cfg(test)]
#[path = "../tests/posted_on.rs"]
mod posted_on_tests;
#[cfg(test)]
#[path = "../tests/subthread.rs"]
mod subthread_tests;
#[cfg(test)]
#[path = "../tests/said_cleaned.rs"]
mod said_cleaned_tests;
#[cfg(test)]
#[path = "../tests/due_kept.rs"]
mod due_kept_tests;
#[cfg(test)]
#[path = "../tests/brain_triggers.rs"]
mod brain_triggers_tests;
#[cfg(test)]
#[path = "../tests/acts_compare.rs"]
mod acts_compare_tests;
#[cfg(test)]
#[path = "../tests/handler_runs.rs"]
mod handler_runs_tests;
pub use violated::is_duties_file;

pub use coord_types::ESC_APART;

/// `Tickets::machine_due`'s answer and the machine it was read from.
#[derive(Debug)]
struct DueKept {
    key: (u64, usize),
    all: BTreeMap<String, Vec<Delivery>>,
    /// The passes over the machine made so far, this one included (what the tests count).
    passes: u64,
}

/// A hand-over on its way, as it was handed (`Tickets::handed`).
#[derive(Clone, Debug, Default)]
pub(crate) struct Handing {
    /// What it leaves out of his posts once asked or told late, `Machine::left_out` as it was handed: None when none up
    /// to its mark could be.
    pub left_out: Option<BTreeSet<u32>>,
    /// The posts it shows (`Delivery::shows`).
    pub shows: Vec<u32>,
}

impl Handing {
    /// D, as it is handed to AGENT now.
    pub(crate) fn of(m: &Machine, agent: &str, d: &Delivery) -> Handing {
        Handing { left_out: m.left_out(agent, d), shows: d.shows() }
    }

    /// The `skipped` of its `given` line, the hand-over of T up to UPTO given to AGENT now: what it left out, with his
    /// posts told to AGENT late since that it does not show (`Machine::left_out_given`; proto-reader N-1479 p264).
    pub(crate) fn skipped(&self, m: &Machine, agent: &str, t: u64, upto: u32) -> Option<BTreeSet<u32>> {
        m.left_out_given(agent, t, upto, &self.shows, self.left_out.clone())
    }
}

/// Git's side of the kept graph (paper-research N-1446 p1406, head 1b): each repository's refs as they were when its
/// log was last read, every commit read, each once, for what each build carries, how many builds that was last
/// worked out for, and whether the runner's jobs from before the record had them are in (`graph_jobs_add`). Its
/// repositories are those the folders where agents work sit in ([`work_repos`], p1455): NAMED, the folders the record
/// names, and PLACED, where each of them and each of the project's repositories sits for git, once. Of those read,
/// COUNTED are the ones whose commits are in (p1476: the project's, or one with a commit an agent here wrote or a job
/// from here ran on, by JOB_COMMITS, their first 8 characters); LEFT, each other one's last read, set aside until it
/// counts. MOVED: each (commit, branch) whose `on` edge from the reflogs is in; IN_WORKTREE: each commit whose `in` edge
/// to the worktree it was made in is (p1554); BY_ID: each commit's place in COMMITS. EPOCH: which one it is, so a round
/// of git's side taken from it before a reload puts nothing in the next. PACKAGES: the cargo packages of each repository
/// whose commits are in, as its last read gave them (one per repository), for the facts a decision on a commit is handed
/// ([`Core::decision_facts`]; proto-mind N-1446 p1502 (1)). TRAINS: each counted repository's `train-` branches and
/// their tips as last read, by folder, for the `merged` lines ([`Core::trains_moved`]).
#[derive(Debug, Default)]
pub struct GraphGit {
    epoch: work_repos::Epoch,
    named: BTreeSet<String>,
    placed: BTreeMap<String, work_repos::Placed>,
    refs: HashMap<String, String>,
    commits: Vec<docket_machine::graph_git::Commit>,
    hashes: std::collections::HashSet<String>,
    builds: usize,
    jobs_read: bool,
    job_commits: BTreeSet<String>,
    counted: BTreeSet<String>,
    left: BTreeMap<String, work_repos::RepoRead>,
    moved: std::collections::HashSet<(String, String)>,
    in_worktree: std::collections::HashSet<String>,
    by_id: HashMap<String, usize>,
    packages: Vec<docket_machine::graph_git::Packages>,
    trains: BTreeMap<String, BTreeMap<String, String>>,
}

/// The branches whose moves the git round writes as `merged` lines ([`Core::trains_moved`]): the trains, `train-N`.
const TRAIN: &str = "train-";

/// The most commits one `merged` line lists ([`GraphGit::brought`]); it counts the rest.
const MERGED_MOST: usize = 500;

impl GraphGit {
    /// The value of a `merged` line (`docket_machine::MERGED`) for BRANCH of repository NAME, now at TIP, FROM where
    /// it stood: the commits read that the move brought, the tip's history less FROM's, the newest first (the tip
    /// first), as many as [`MERGED_MOST`], each with its parents and tree; a branch new since the last read (no FROM)
    /// brings its tip alone. None while the tip is not among the commits read.
    fn brought(&self, name: &str, branch: &str, tip: &str, from: Option<&str>) -> Option<Value> {
        use docket_machine::graph_git::{Commit, commit_id};
        let get = |h: &str| self.by_id.get(&commit_id(h)).map(|i| &self.commits[*i]);
        let top = get(tip)?;
        let walk = |start: &str, stop: &std::collections::HashSet<String>| {
            let (mut out, mut seen, mut next) = (Vec::<&Commit>::new(), std::collections::HashSet::new(), vec![start.to_string()]);
            while let Some(h) = next.pop() {
                let id = commit_id(&h);
                if stop.contains(&id) || !seen.insert(id) {
                    continue;
                }
                if let Some(c) = get(&h) {
                    next.extend(c.parents.iter().cloned());
                    out.push(c);
                }
            }
            out
        };
        let mut brought = match from {
            Some(f) => {
                let old = walk(f, &Default::default()).into_iter().map(|c| commit_id(&c.hash)).collect();
                walk(tip, &old)
            }
            None => vec![top],
        };
        brought.sort_by(|a, b| (a.hash != top.hash, std::cmp::Reverse(&a.at)).cmp(&(b.hash != top.hash, std::cmp::Reverse(&b.at))));
        let unlisted = brought.len().saturating_sub(MERGED_MOST);
        let commits: Vec<Value> = brought.iter().take(MERGED_MOST).map(|c| json!({"commit": c.hash, "parents": c.parents, "tree": c.tree})).collect();
        let mut v = json!({"repo": name, "branch": branch, "tip": top.hash, "tree": top.tree, "parents": top.parents, "from": from, "commits": commits});
        if unlisted > 0 {
            v["unlisted"] = json!(unlisted);
        }
        Some(v)
    }
}

/// Git's side of each of REPOS (name, folder) whose refs moved since REFS (by folder, as [`GraphGit`] last read them),
/// read off the lock on the side's clock (`read_sides`, paper-research N-1446 p1476): its refs now, its commits (`log
/// --all --name-only`, [`docket_machine::graph_git::Commit::FORMAT`], each with its tree and files), its cargo packages
/// at its `main` ([`work_repos::manifests`]) and its branches' moves from the reflogs (`log -g`,
/// [`docket_machine::graph_git::Moved::FORMAT`], as `a_coords_record_now` reads them): every branch's the first time,
/// after it those of the branches that moved, every branch's again when that read fails (a branch gone meanwhile); and
/// what each of its worktrees made, from their HEAD reflogs in its common git folder, whole ([`work_repos::head_reflogs`],
/// p1554), so a commit on a detached HEAD in a worktree comes in at the next read its refs move for. A repository whose
/// refs did not move, or that git cannot read, is not in it. Its refs are `for-each-ref`'s and its HEAD. git only reads
/// ([`work_repos::git`]).
pub fn git_read(repos: &[(String, String)], refs: &HashMap<String, String>) -> Vec<work_repos::RepoRead> {
    use docket_machine::graph_git::{Commit, Moved};
    let git = |dir: &str, args: &[&str]| work_repos::git(std::path::Path::new(dir)).args(args).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned());
    let (commits, moves) = (format!("--format={}", Commit::FORMAT), format!("--format={}", Moved::FORMAT));
    repos
        .iter()
        .filter_map(|(name, dir)| {
            let now = git(dir, &["for-each-ref", "--format=%(objectname) %(refname)"])? + &git(dir, &["rev-parse", "HEAD"]).unwrap_or_default();
            let before = refs.get(dir);
            if before == Some(&now) {
                return None;
            }
            let log = git(dir, &["log", "--all", "--name-only", &commits])?;
            let reflog = |which: &[&str]| git(dir, &[&["log", "-g", "--date=unix", moves.as_str()][..], which, &["--"]].concat());
            let moved: Option<Vec<&str>> = before.map(|b| {
                let had: std::collections::HashSet<&str> = b.lines().collect();
                now.lines().filter(|l| !had.contains(l)).filter_map(|l| l.split_once(' ').map(|(_, r)| r)).filter(|r| r.starts_with("refs/heads/")).collect()
            });
            let reflog = match moved {
                Some(m) if m.is_empty() => String::new(),
                Some(m) => reflog(&m).or_else(|| reflog(&["--all"])).unwrap_or_default(),
                None => reflog(&["--all"]).unwrap_or_default(),
            };
            let packages = work_repos::manifests(dir).map(|m| work_repos::packages_of(name, &m));
            let common = git(dir, &["rev-parse", "--path-format=absolute", "--git-common-dir"]).unwrap_or_default();
            let made = Some(common.trim()).filter(|c| !c.is_empty()).map(|c| work_repos::head_reflogs(std::path::Path::new(c), name)).unwrap_or_default();
            Some(work_repos::RepoRead { dir: dir.clone(), refs: now.clone(), commits: Commit::parse_log(&log, name), moves: Moved::parse_log(&reflog, name), made, packages })
        })
        .collect()
}

/// The tickets' part of the core.
#[derive(Debug)]
pub struct Tickets {
    pub m: Machine,
    /// The record's one graph (docket-machine `graph`): made along the start's replay, and kept with each input recorded
    /// after it, as its lines go in (`ticket_input`), so it is the record's and no further (proto-window N-1446 p1383,
    /// head 1; `Request::Graph`); with the `named` edges of the Mentions index (`load_tickets`, and each landing post's
    /// as it is read). A post taken back keeps its `named` edges here until the next start, whose `named_into` leaves
    /// them out as `Mentions::held` does. GRAPH_LINES: how many of the record's lines it has.
    pub graph: docket_machine::graph::Builder,
    graph_lines: usize,
    /// The record's line the newest replay state saved for this program stands at (replay_state.rs, I-1509): the start's,
    /// then each `coordd save-replay` this coordd ran ([`crate::save_replays`]); none while there is none.
    pub(crate) saved_at: Option<usize>,
    /// Git's side of the graph: the commits of the project's repositories and of those the folders where agents work
    /// sit in, read off the lock when their refs move ([`git_read`], [`Tickets::graph_git_add`]; paper-research N-1446
    /// p1406, head 1b; p1455); the folders taken from the lines as the graph takes them.
    pub graph_git: GraphGit,
    /// The agents' side of the graph (`follow::KeptSide`, paper-research N-1446 p1458): the sessions the record's
    /// `given` lines name, taken along the start's replay and with each input recorded, as the graph takes its lines,
    /// and read on off the lock each 5 s (`read_sides`, [`Core::side_round`], [`Core::side_put`]).
    pub(crate) side: crate::follow::KeptSide,
    /// The places the record gives a text at (`text_places`), the only ones `Request::Text` reads (paper-research
    /// N-1446 p1419, 2): made along the start's replay and kept with each input recorded, as the graph is. `Shared`
    /// holds it too, so a text is checked off the core's lock; a reload keeps the one it holds (`load_tickets`).
    pub(crate) places: Arc<std::sync::RwLock<text_places::TextPlaces>>,
    /// The machine's `due_all` as last read, and of whom: given again while the machine is as it was (`machine_due`).
    due_kept: std::sync::Mutex<Option<DueKept>>,
    log: String,
    /// Kinds and call files refused, and what replay found diverging: shown, never hidden.
    pub errors: Vec<String>,
    /// The project's duties file, when its policy declares duties an answer can break (violated.rs).
    duties: Option<String>,
    /// The calls as read (`read_defs`): what a reload compares to say which calls changed, what the Kinds answer gives
    /// of the calls a kind's steps name, and the tasks the watcher's steps run.
    pub call_files: CallFiles,
    /// Ticket deliveries handed to a session and not yet reported taken: (agent, delivery id) -> (ticket, upto, what
    /// it showed and left out as it was handed, for its `given` line's `skipped`).
    pub(crate) handed: HashMap<(String, String), (u64, u32, Handing)>,
    /// Of those, the ones put into a turn running (what the delivery program delivers to a working agent): taken,
    /// they are `GivenInto`.
    pub(crate) into: BTreeSet<(String, String)>,
    /// Of those, the ones that carried a post taken back while they were on their way: (agent, delivery id) -> (ticket,
    /// post). Taken, the agent had the post, and is told (`Input::StruckFor`; consult-3, N-818 p74); not taken, nothing.
    pub(crate) backs: HashMap<(String, String), Vec<(u64, u32)>>,
    /// Where the last final output taken for an agent ends: (file, offset past it). A turn's end is taken once, by
    /// its hooks or screen with the text said so far, or by the transcript's stop, whichever is first (N-807.p63).
    pub(crate) taken_at: HashMap<String, (String, u64)>,
    /// What each ended turn of an agent did, as its transcript's follower read it (`follow::Follower::take_worked`),
    /// by where its final output ends: (file, offset past it, [`docket_machine::Worked`]). Taken at that turn's end
    /// ([`Core::ticket_turn_ended_queued`]), which drops any older; written on its `turn_ended` line (insight (1)).
    pub(crate) worked: HashMap<String, Vec<(String, u64, docket_machine::Worked)>>,
    /// An agent's background work still running when a turn of it stopped, by task ("KIND TASK-ID", as
    /// `Background::still_running` names it): the thread that turn replied on and whom its reply named, none for a
    /// turn given nothing. The turn its notice begins, given nothing, posts its final output there (consult-3,
    /// N-807.p63: the real answer after a half-way end was posted nowhere; p1205: per task, so a turn answering
    /// another thread in the wait neither posts there nor moves it).
    pub(crate) carry: HashMap<String, Vec<(String, Option<(u64, Vec<String>)>)>>,
    /// Where questions to policy and heads to form go (`start`).
    tasks: Option<tokio::sync::mpsc::UnboundedSender<Task>>,
    /// Requests closed before or whose grant has been applied: a grant is applied once, when its request closes.
    granted: BTreeSet<u64>,
    /// Agents whose running turn an esc is stopping (Effect::Interrupt): nothing from the tickets is handed to them
    /// until the stop has landed (`Core::interrupted`), so the Escape never hits the turn the words start.
    pub stopping: BTreeSet<String>,
    /// Answers to waiting asks (Effect::Answered), by asker: what its waiting `coord ask` returns (docket-edge's 8
    /// takes it from here).
    pub answered: BTreeMap<String, String>,
    /// Bumped each time an answer is put in `answered`: a waiter subscribes, looks in `answered`, then awaits
    /// `changed()`, so no answer is missed and every waiting asker wakes (docket-edge m1203).
    pub answered_tx: tokio::sync::watch::Sender<u64>,
    /// The layers coordd has asked to be made and not yet to end (`Core::layers_reconcile`); None until the first
    /// reconcile reads the registry.
    layers: Option<BTreeSet<String>>,
    /// The removals coordd has asked for since it started (`Core::layers_tidy`): a layer's id, and whether the top
    /// ticket of its tree had closed.
    tidied: BTreeSet<(String, bool)>,
    /// What `waited` reads besides a step's `since` (quiet.rs): the jobs running per ticket, the agents' turn ends.
    pub quiet: quiet::Quiet,
    /// The first post an input recorded since this was taken: its ticket's id and its number (N-1788, `Reply::post`),
    /// read off the record's own `post` or `open` line, so a request's reply carries it and nobody parses the words.
    pub(crate) posted: Option<(String, u32)>,
    /// The facts the record's acts made and have not ended, under the policy text in force (effects.rs; the engine
    /// page's move 6, paper-research N-1446 p3020, 8), given to every decision and act with `Core::norm_facts`.
    pub effects: effects::Store,
    /// The `violated` and `duties` asks running off the lock, by (ticket, step, question): each one's stop, set when
    /// the ticket no longer asks it (closed while it was asked), which gives the ask up (`Core::end_asks`;
    /// docket-core4, N-101 p1664). A goal's two asks stand apart, so one's end never stops the other's (paper-research
    /// N-1446 p2251).
    pub(crate) asks: BTreeMap<(u64, u32, &'static str), Arc<std::sync::atomic::AtomicBool>>,
    /// What every post's words name, and what names each ticket and agent (threads piece 3, N-412 p166): built at start,
    /// kept as posts land, never recorded.
    pub(crate) mentions: mentions::Mentions,
    /// What each agent's pull reads: its open items and checks (pull.rs).
    pub(crate) holds: crate::pull::Holds,
    /// The joint's thread (N-568 part 5; `run-acts both`): each input's lines go to it after they are recorded, with
    /// the line number of the input in the record.
    /// The switch's file is watched at each send: `off` there drops the sender, and the thread ends (paper-research N-788).
    pub(crate) joint: Option<(std::sync::mpsc::Sender<crate::joint::Group>, usize, crate::joint::Watch)>,
}

/// The facts a decision is handed beside its call fields, with the policy's places to solve it in
/// ([`Core::decision_facts`], [`decide_in`]).
pub(crate) type Situation = (Vec<(String, Vec<String>)>, crate::policy::Places);

/// Work done off the lock.
#[derive(Debug)]
enum Task {
    /// Who fills ROLE for the step: the policy's decision `fills`, in the situation FACTS (the team's and the ticket's,
    /// its lists' among(X, I), and (1)'s facts for each commit it is given, `Core::commits_facts`). ARGS: the step's
    /// arguments that are text, numbers or truths, each as text by its name, beside its call's three fields
    /// (`step_fields`).
    Fills { t: u64, step: u32, ticket: String, role: String, call: String, args: BTreeMap<String, String>, facts: Vec<(String, Vec<String>)>, places: crate::policy::Places },
    /// ASKED: for the owner's words (§12), the step they answer as the owner was shown it: its call, its arguments,
    /// and where the calls are, rendered off the lock (paper-research m1339, b1).
    /// SHOWN: for an agent's answer, the step as the agent was shown it: where its first delivery was written in the
    /// agent's session, else its call and arguments to render it again (N-246.p1).
    /// COORD: the coord's folder, where the forming calls are kept (`tickets/forming/`).
    Form { t: u64, step: u32, agent: String, head: Type, text: Loc, words: Option<String>, asked: Option<(String, Value, crate::policy::Places)>, shown: Option<(Option<Loc>, String, Value, crate::policy::Places)>, coord: String },
    /// Whom policy's `notify` tells of EVENT on TICKET, ACTOR having caused it, in the situation FACTS.
    Notify { t: u64, event: String, actor: String, post: Option<u32>, ticket: String, facts: Vec<(String, Vec<String>)>, places: crate::policy::Places },
    /// Event NAME with VALUE, which an `on` binding is on: fed back as an input, under the lock.
    Event { name: String, value: Value },
    /// Whether ticket T's GOALS hold (Owed): each the policy's act of that name, for the ticket's opener, in the
    /// situation FACTS (with what closed under it).
    Holds { t: u64, step: u32, ticket: String, actor: String, goals: Vec<String>, facts: Vec<(String, Vec<String>)>, places: crate::policy::Places },
    /// The policy's decision NAME on ARGS for ticket T's `decide` (N-101): its rules' answer, or none for the owner.
    /// FACTS: what the record shows it beside its arguments (`Machine::decide_facts`); WHY: an answer of its has a
    /// field `why` no argument fills, which gets the text of the rule that chose it (N-101 p158-p162).
    /// SITUATION: the facts it is handed beside its arguments, with the policy's places ([`Core::decision_facts`]: for
    /// a decision on a commit the kept graph has read, that commit's), decided through [`decide_in`].
    Decide { t: u64, step: u32, ticket: String, name: String, args: Value, facts: serde_json::Map<String, Value>, why: bool, situation: Option<Situation> },
    /// The policy's decision NAME among agents (`decision NAME(...) -> Agent`, N-1134's `claims`) for ticket T's
    /// `decide`: the one agent its rules leave, or none for the owner. FIELDS: its arguments as text and the ticket;
    /// FACTS: the team's and the ticket's, with the agents it is given (`Core::among_facts`).
    Among { t: u64, step: u32, name: String, fields: BTreeMap<String, String>, facts: Vec<(String, Vec<String>)>, places: crate::policy::Places },
    /// Which duties ANSWERING breaks (N-101, violated.rs), read from LOG's lines on the tickets TREE; given up when STOP
    /// is set (the ticket no longer asks it).
    Violated { t: u64, step: u32, answering: violated::Answering, policy: Option<String>, log: String, tree: BTreeSet<u64>, repos: Vec<String>, stop: Arc<std::sync::atomic::AtomicBool> },
    /// Goal GOAL's duties (docket-machine machine/duties.rs), read by POLICY, the project's duties file, over LINES,
    /// the goal's lines; given up when STOP is set (the goal no longer asks it).
    Duties { t: u64, step: u32, goal: String, lines: Vec<Value>, policy: Option<String>, stop: Arc<std::sync::atomic::AtomicBool> },
}

impl Default for Tickets {
    fn default() -> Tickets {
        Tickets { m: Machine::load(Defs::default()), graph: Default::default(), graph_lines: 0, saved_at: None, graph_git: Default::default(), side: Default::default(), places: Default::default(), due_kept: Default::default(), log: String::new(), errors: Vec::new(), duties: None, call_files: CallFiles::default(), handed: HashMap::new(), into: BTreeSet::new(), backs: HashMap::new(), taken_at: HashMap::new(), worked: HashMap::new(), carry: HashMap::new(), tasks: None, granted: BTreeSet::new(), stopping: BTreeSet::new(), answered: BTreeMap::new(), answered_tx: tokio::sync::watch::channel(0).0, layers: None, tidied: BTreeSet::new(), posted: None, quiet: Default::default(), effects: Default::default(), asks: BTreeMap::new(), mentions: Default::default(), holds: Default::default(), joint: None }
    }
}

/// The calls as coordd read them (`Tickets::read_defs`): each as the one parser gave it, by name, and each task the
/// watcher's steps run with the call coordd runs for it (`watcher_tasks`).
#[derive(Debug, Default)]
pub struct CallFiles {
    pub defs: BTreeMap<String, docket_machine::syntax::CallDef>,
    pub tasks: BTreeMap<String, String>,
}

impl CallFiles {
    /// Call D's text as read (its signature, attributes and template): what a reload compares.
    fn text(d: &docket_machine::syntax::CallDef) -> String {
        format!("{:?} {:?} {:?}", d.sig, d.attrs, d.template)
    }
}

/// Each task the watcher's steps run in DEFS' kinds, with the call coordd runs for it: clerk.rs's (`clerk::runs`,
/// `choose` for `fold`'s `deliver`, none for `next`'s), else the step's own. What `model-for` is asked for each, off the lock
/// (`Core::task_models`, paper-research N-1396 p235).
fn watcher_tasks(defs: &Defs, calls: &BTreeMap<String, docket_machine::syntax::CallDef>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for k in defs.kinds.values() {
        for p in std::iter::once(&k.pattern).chain(k.handlers.iter().map(|h| &h.body)) {
            for (w, call, _) in docket_machine::check::steps(p) {
                let task = calls.get(call).and_then(|d| d.attr("task"));
                if let (docket_machine::syntax::Worker::Named(n), Some(t)) = (w, task) {
                    if n == docket_machine::WATCHER {
                        out.entry(t.to_string()).or_insert_with(|| crate::clerk::runs(t, call).unwrap_or(call).to_string());
                    }
                }
            }
        }
    }
    out
}

/// The calls the steps of PROGS (a kind's pattern and handlers) name, in order, each as coordd read it: its file and
/// line, signature, attributes, template and doc, its task, the call coordd runs for it when it is a watcher's task
/// (`choose` for `next`), and that task's model as the policy gives it (MODELS, solved off the lock). What a step
/// pressed on the Docket shows (paper-research N-1396 p174, p223, p235). It only reads.
/// EDITED, the call's last live edit (`Machine::calls_edited`), on each entry and on the call it runs.
fn kind_calls<'a>(files: &CallFiles, models: &BTreeMap<String, crate::policy::TaskModel>, edited: &BTreeMap<String, docket_machine::LiveEdit>, progs: impl Iterator<Item = &'a docket_machine::syntax::Prog>) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    progs
        .flat_map(|p| docket_machine::check::steps(p))
        .filter_map(|(_, call, _)| seen.insert(call).then(|| files.defs.get(call)).flatten())
        .map(|d| {
            let task = d.attr("task");
            let runs = task.and_then(|t| files.tasks.get(t)).filter(|r| **r != d.sig.name);
            let v = json!({
                "name": d.sig.name, "file": d.file, "line": d.line, "sig": d.written,
                "attrs": d.attrs, "template": d.template, "doc": d.doc.text, "task": task,
                "runs": runs.map(|r| with_edit(json!({"name": r, "template": files.defs.get(r.as_str()).and_then(|x| x.template.as_ref())}), edited.get(r))),
                "model": task.and_then(|t| models.get(t)),
            });
            with_edit(v, edited.get(&d.sig.name))
        })
        .collect()
}

/// V, an entry of the Kinds answer, with `edited`: EDIT, the last reload that named it (`docket_machine::LiveEdit`),
/// where one did; else none (N-1396 p305, 4).
fn with_edit(mut v: Value, edit: Option<&docket_machine::LiveEdit>) -> Value {
    if let Some(e) = edit {
        v["edited"] = json!(e);
    }
    v
}

/// The files of DIR ending in `.EXT`, with OVERLAY's texts in place of theirs and its paths in DIR added: (path,
/// text), by name (`policy::paths_in`); one that does not read is left out.
fn files(dir: &str, ext: &str, overlay: &crate::policy::Overlay) -> Vec<(String, String)> {
    crate::policy::paths_in(dir, ext, overlay).into_iter().filter_map(|p| Some((p.clone(), crate::policy::text_at(&p, overlay).ok()?))).collect()
}

/// The folders the kinds and calls are read from, in order: agent-base, the project's library, the project
/// (`project::folders`).
fn folders(coord: &str, base: &str, project: Option<&str>) -> Vec<String> {
    let dirs: Vec<String> = crate::project::folders(coord, base, project);
    // the replay test (`a_coords_record_replays`) with a program and library of its own, the coord's record read in
    // place: REPLAY_LIBRARY in place of the one the coord's project file names (paper-research N-101 p1362)
    #[cfg(test)]
    let dirs = match (std::env::var("REPLAY_LIBRARY"), project) {
        (Ok(l), Some(p)) => vec![base.to_string(), l, p.to_string()],
        _ => dirs,
    };
    dirs
}

/// The folders the kinds and the calls are read from (`Tickets::read_defs`) in DIRS ([`folders`]): each one's `kinds/`
/// with its agents' own programs (`kinds/agents/`), and its `calls/`.
fn defs_dirs(dirs: &[String]) -> (Vec<String>, Vec<String>) {
    // with an agent's own programs, `kinds/agents/NAME.kind` (0.2 line 26: whose clock it is), where a folder keeps
    // its kinds in `kinds/` (a flat folder is read once)
    let kinds = dirs
        .iter()
        .flat_map(|d| {
            let k = crate::project::place(d, "kinds");
            let own = (k != *d).then(|| format!("{k}/agents"));
            std::iter::once(k).chain(own)
        })
        .collect();
    (kinds, dirs.iter().map(|d| crate::project::place(d, "calls")).collect())
}

/// The kinds and calls as `Tickets::read_defs` reads them: the defs, why a file, call or kind was left out, the duties
/// file, the calls as read.
pub type DefsRead = (Defs, Vec<String>, Option<String>, CallFiles);

/// A reload's refusals ([`Core::reload_checked`]) as the owner's Reload and Reach answer them.
fn not_read_again(refused: &[String]) -> String {
    format!("nothing is read again, the kinds and calls stay as they were: {}", refused.join("; "))
}

/// What a reload takes, checked (`Core::reload_checked`): the definitions, the duties file and the calls as read, and
/// the kinds and calls whose text it changes.
pub(crate) struct Reread {
    defs: Defs,
    duties: Option<String>,
    call_files: CallFiles,
    kinds: Vec<String>,
    calls: Vec<String>,
}

impl Tickets {
    /// Load the kinds and calls of BASE and PROJECT (a project's kind or call of the same name wins), then replay the
    /// log in COORD. FIRST: the first ticket number.
    /// The machine as the log has it: the defs it runs, the log replayed (after a line that did not go in).
    pub fn replayed(&self) -> Machine {
        let mut m = Machine::load(docket_machine::Defs::clone(&self.m.defs));
        let lines: Vec<Line> = std::fs::read_to_string(&self.log).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        for p in m.replay(&lines) {
            eprintln!("coordd: tickets: {}: {p}", self.log);
        }
        // past the machine it replaces, so nothing kept from that one is given for this one (`machine_due`)
        m.start_above(&self.m);
        m
    }

    /// The kinds and calls as coordd reads them at PLACES, from agent-base, the project's library and the project
    /// (`defs_dirs`), with PLACES' overlay's texts in place of the files at its paths (the owner's draft at a Reach;
    /// empty for the real thing), the tickets numbered from FIRST: the machine's defs, why a file, call or kind was left
    /// out, the duties file, and the calls as read. At start, at a reload (`Core::places`), and at a Reach (those with
    /// the draft). It needs nothing of the core: a Reach reads it off the lock.
    pub fn read_defs(places: &crate::policy::Places, first: u64) -> DefsRead {
        Tickets::read_defs_in(places, &folders(&places.coord, &places.base, places.project.as_deref()), first)
    }

    /// [`Tickets::read_defs`] from the folders DIRS (agent-base, the library, the project, in that order), PLACES'
    /// project being the last: the text-change kind's check reads a copy of the project's text this way, whose library
    /// the project file does not name (change.rs, N-865).
    pub fn read_defs_in(places: &crate::policy::Places, dirs: &[String], first: u64) -> DefsRead {
        let (project, draft) = (places.project.as_deref(), &places.overlay);
        let mut errors = Vec::new();
        let mut call_defs = BTreeMap::new();
        let mut call_docs = BTreeMap::new();
        let (kind_dirs, call_dirs) = defs_dirs(dirs);
        // the calls as policy::call read them (the one parser, docket_machine::syntax, on top of the nets' system types),
        // the same reading for the machine as for the renderer
        let (calls, named) = match crate::policy::call::Calls::load(&call_dirs, draft) {
            Ok(c) => {
                // a bad file costs only its calls; why shows here, where kind refusals show
                errors.extend(c.errors);
                // each call as read
                call_defs = c.defs.clone();
                // each call's label (docket_machine::label): its doc's first line
                call_docs = c.defs.iter().filter_map(|(n, d)| d.doc.text.lines().map(str::trim).find(|l| !l.is_empty()).map(|l| (n.clone(), l.to_string()))).collect();
                (c.defs.into_iter().map(|(n, c)| (n, c.sig)).collect::<BTreeMap<String, CallSig>>(), c.named)
            }
            Err(e) => {
                errors.push(format!("the calls do not load: {e}"));
                (BTreeMap::new(), BTreeMap::new())
            }
        };
        let kinds: Vec<(String, String)> = kind_dirs.iter().flat_map(|k| files(k, "kind", draft)).collect();
        // whether a step to an agent may raise Violated (N-101): the project's policy declares duties
        let duties = violated::declared(project).unwrap_or_else(|e| {
            errors.push(e);
            None
        });
        // the model classes (models.rs): a call's `where` names what the policy knows, or the call is left out
        let scopes = places.sources().unwrap_or_else(|e| {
            errors.push(format!("the policy, for the calls' models: {e}"));
            Vec::new()
        });
        // the policy compiled whole, its decisions and its norms, so what they would refuse at the first solve is said at
        // load with its file and line: an instance with another number of values than its fact declares
        // (`+effort-for("mind").`, tovrin-writer N-101 p677), a fact type nobody declares, a rule over a decision none has
        if !scopes.is_empty() {
            if let Err(e) = crate::policy::compile::compile(&scopes).and_then(|_| crate::policy::norms::load(&places).map(|_| ())).and_then(|_| match crate::policy::interp::Mode::of(&places) {
                // with eflint-rs answering, what it cannot load is said here too (in `both` the journal says it)
                crate::policy::interp::Mode::Eflint => crate::policy::interp::load(&places).map(|_| ()),
                _ => Ok(()),
            }) {
                errors.push(format!("the policy: {e}"));
            }
        }
        let order = crate::models::Order::of(&scopes, &[]);
        let mut calls = calls;
        calls.retain(|name, sig| match crate::models::cond_of(sig).map(|c| c.and_then(|c| order.check(&c))) {
            Some(Err(e)) => {
                errors.push(format!("call {name}: `where {}`: {e}; the call is left out", sig.cond.as_deref().unwrap_or("")));
                false
            }
            _ => true,
        });
        let (mut defs, errs) = Defs::build_with(&kinds, calls, named, first, duties.is_some());
        errors.extend(errs);
        defs.call_docs = call_docs;
        // a step that would make an agent with no model said (no `where` on its call, no model-for rule of the project
        // for agents) refuses its kind, naming the call: no model is taken by default
        let rule = crate::models::project_rule(&scopes);
        if rule.is_none() {
            let unsaid: Vec<(String, String)> = defs
                .kinds
                .values()
                .filter_map(|k| {
                    let mut made = docket_machine::check::makes_agents(&k.pattern);
                    k.handlers.iter().for_each(|h| made.extend(docket_machine::check::makes_agents(&h.body)));
                    let (call, line) = made.into_iter().find(|(c, _)| defs.calls.get(c).is_some_and(|s| s.cond.is_none()))?;
                    Some((k.name.clone(), format!("{}:{line}: kind {}: its step makes an agent for call {call}, and nothing says its model: give call {call} a `where` (`where model >= medium`), or the project's policy a model-for rule for agents; the kind is refused", k.file, k.name)))
                })
                .collect();
            for (k, why) in unsaid {
                defs.kinds.remove(&k);
                errors.push(why);
            }
        }
        let tasks = watcher_tasks(&defs, &call_defs);
        (defs, errors, duties, CallFiles { defs: call_defs, tasks })
    }

    /// READ ([`git_read`]) into the kept graph (paper-research N-1446 p1406, head 1b; p1476): each repository's refs as
    /// read, and its read set aside until the repository counts: one of PROJECT (the project's `repos`, by folder), or
    /// one with a commit whose `Agent:` names one of HERE (the agents here among [`GraphGit::names`]) or that a job
    /// from here ran on. One edit in a folder brings in no other history. Once it counts it counts from then on: its
    /// commits not in the graph yet go in, with what each build carries by every commit in (`graph_git::add_commits`,
    /// worked out again when there are new commits or the record has put in a build since), and its branches' moves
    /// not in yet (`graph_git::add_moves`), and where each commit not placed yet was made, by its worktrees' HEAD
    /// reflogs (`graph_git::add_made_in`, p1554). Gives the commits it put in, for what they name
    /// ([`Core::graph_git_add`]).
    pub fn graph_git_add(&mut self, read: Vec<work_repos::RepoRead>, project: &BTreeSet<String>, here: &BTreeSet<String>) -> Vec<docket_machine::graph_git::Commit> {
        use docket_machine::graph_git::{commit_id, Moved};
        let Tickets { graph_git: g, graph, .. } = self;
        for r in read {
            g.refs.insert(r.dir.clone(), r.refs.clone());
            g.set_aside(r);
        }
        let count: Vec<String> = g.left.iter().filter(|(d, r)| g.counted.contains(*d) || project.contains(*d) || r.commits.iter().any(|c| g.counts(c, here))).map(|(d, _)| d.clone()).collect();
        let (mut new, mut moves, mut made) = (Vec::new(), Vec::new(), Vec::new());
        for d in count {
            let Some(r) = g.left.remove(&d) else { continue };
            g.counted.insert(d);
            if let Some(p) = r.packages {
                g.packages.retain(|x| x.repo != p.repo);
                g.packages.push(p);
            }
            new.extend(r.commits.into_iter().filter(|c| g.hashes.insert(c.hash.clone())));
            moves.extend(r.moves);
            made.extend(r.made);
        }
        let key = |m: &Moved| (commit_id(&m.hash), format!("branch:{}/{}", m.repo, m.branch));
        moves.retain(|m| !g.moved.contains(&key(m)));
        let builds = graph.graph.nodes.iter().filter(|n| n.kind == docket_machine::graph::NodeKind::Build).count();
        if !new.is_empty() || builds != g.builds {
            for c in &new {
                g.by_id.insert(commit_id(&c.hash), g.commits.len());
                g.commits.push(c.clone());
            }
            g.builds = builds;
            docket_machine::graph_git::add_commits(&mut graph.graph, &new, &g.commits);
        }
        docket_machine::graph_git::add_moves(&mut graph.graph, &moves);
        g.moved.extend(moves.iter().map(key));
        // each reflog is read whole, so a commit placed already has its first entry in what was read then
        made.retain(|m| !g.in_worktree.contains(&commit_id(&m.hash)));
        docket_machine::graph_git::add_made_in(&mut graph.graph, &made);
        g.in_worktree.extend(made.iter().map(|m| commit_id(&m.hash)));
        new
    }

    /// JOBS into the kept graph, as `graph_git::add` puts in a job file's: the runner's jobs from before the record had
    /// them (`jobs::of_coord` of `jobs::before_record_dir`, paper-research N-1446 p1422, 2), read once, at the first
    /// round of git's side (`Core::graph_git_put`), after git's commits, so that a job's commit is git's; once in, not
    /// again.
    pub fn graph_jobs_add(&mut self, jobs: &[docket_machine::graph_git::Job]) {
        if std::mem::replace(&mut self.graph_git.jobs_read, true) {
            return;
        }
        self.graph_git.take_jobs(jobs);
        docket_machine::graph_git::add(&mut self.graph.graph, &[], jobs);
    }

    /// Whether [`Tickets::graph_jobs_add`] has put them in.
    pub fn graph_jobs_read(&self) -> bool {
        self.graph_git.jobs_read
    }

    /// Each repository's refs as [`GraphGit`] last read them, for [`git_read`] off the lock (a test's; the side's clock
    /// takes them with the rest of its reading, [`Core::graph_git_reading`]).
    #[cfg(test)]
    pub fn graph_git_refs(&self) -> HashMap<String, String> {
        self.graph_git.refs.clone()
    }

    /// The tickets of COORD's record replayed whole, no state read or saved (replay_state.rs): the tests' and the
    /// harnesses' load of a record or a copy of one.
    pub fn load(coord: &str, base: &str, project: Option<&str>, first: u64) -> Tickets {
        Tickets::load_joint(coord, base, project, first, false, false)
    }

    /// How many of the record's lines that read the machine has taken: its replay's and each input's since.
    pub fn record_lines(&self) -> usize {
        self.graph_lines
    }

    /// [`Tickets::load`], and with JOINT the joint's thread started on the record as replayed (N-568 part 5); with
    /// STATES the replay goes on from the state saved for this program and these defs, and a whole replay saves one
    /// (coordd's start, `load_tickets`; replay_state.rs, I-1509).
    pub fn load_joint(coord: &str, base: &str, project: Option<&str>, first: u64, joint: bool, states: bool) -> Tickets {
        let (defs, mut errors, duties, call_files) = Tickets::read_defs(&crate::policy::Places { base: base.to_string(), project: project.map(str::to_string), coord: coord.to_string(), ..Default::default() }, first);
        let log = format!("{coord}/tickets/log.jsonl");
        // whole, or up to the cut a replay of a live record is given (`replay_state::read_record`)
        let text = Tickets::record_text(&log, &mut errors);
        // the one replay, from the state saved for this program and these defs and the lines after it, or the whole
        // record (replay_state.rs, I-1509): the machine; the graph made along it (`Builder::after`, as `graph::of`); the
        // sessions its `given` lines name, for the agents' side (p1458); the places its lines give a text at (p1419,
        // 2); the jobs still open and the turn ends (a restart mid-job keeps the job's step held); the folders where
        // agents work, for the repositories the graph reads git from (p1455); the lines that did not read, the
        // replay's problems, then where the graph and the record disagree (paper-research N-1446 p1378); and the
        // joint's history, what the machine showed after each input, taken along it (p1644)
        let started = replay_state::start(coord, &log, &text, defs, &folders(coord, base, project), states);
        eprintln!("{}", started.said);
        let replay_state::Started { ended, saved_at, .. } = started;
        let replay_state::Ended { m, graph, side, places, mut quiet, git: graph_git, errors: found, joint: history, lines: graph_lines } = ended;
        errors.extend(found);
        quiet.read_down(&format!("{coord}/tickets/alive.json"));
        for e in &errors {
            eprintln!("coordd: tickets: {e}");
        }
        let _ = std::fs::create_dir_all(format!("{coord}/tickets"));
        // the grants of requests closed before are in the registry already
        let granted = m.tickets.values().filter(|x| x.kind == "request" && !x.is_open()).map(|x| x.t).collect();
        // the facts the record's acts made, under the policy text in force (effects.rs; the engine page's move 6)
        let (effects, why) = effects::Store::start(&crate::policy::Places { base: base.to_string(), project: project.map(str::to_string), coord: coord.to_string(), ..Default::default() }, &text);
        if let Some(why) = why {
            eprintln!("coordd: tickets: {why}");
            errors.push(why);
        }
        // the joint's thread is fed the history the one replay took (I-1509, p1644): no second replay of the record
        let joint = if joint {
            let threads = crate::joint::threads_on(coord);
            let only = crate::joint::only(&m, threads);
            crate::joint::start(base, project, text, history, only, threads).map(|tx| (tx, graph_lines, crate::joint::Watch::new(coord)))
        } else {
            None
        };
        Tickets { m, graph, graph_lines, saved_at, graph_git, side, places: Arc::new(std::sync::RwLock::new(places)), due_kept: Default::default(), log, errors, duties, call_files, handed: HashMap::new(), into: BTreeSet::new(), backs: HashMap::new(), taken_at: HashMap::new(), worked: HashMap::new(), carry: HashMap::new(), tasks: None, granted, stopping: BTreeSet::new(), answered: BTreeMap::new(), answered_tx: tokio::sync::watch::channel(0).0, layers: None, tidied: BTreeSet::new(), posted: None, quiet, effects, asks: BTreeMap::new(), mentions: Default::default(), holds: Default::default(), joint }
    }

    /// The ticket log LOG's text as a load reads it ([`replay_state::read_record`]). What a cut read is said once. When it
    /// reads as nothing, why is one of ERRORS, so a replay counts it as a problem line.
    fn record_text(log: &str, errors: &mut Vec<String>) -> String {
        match replay_state::read_record(log) {
            Ok((text, said)) => {
                if let Some(said) = said {
                    eprintln!("{said}");
                }
                text
            }
            Err(why) => {
                errors.push(why);
                String::new()
            }
        }
    }

    /// The lines of the ticket log LOG, and its text; a line that does not read is one of ERRORS.
    pub(crate) fn read_log(log: &str, errors: &mut Vec<String>) -> (String, Vec<Line>) {
        let text = Tickets::record_text(log, errors);
        let (lines, _, unread) = replay_state::split(log, replay_state::read_lines(&text, 0, 0));
        errors.extend(unread);
        (text, lines)
    }

    /// What is due to AGENT that is not on its way already.
    pub fn due(&self, agent: &str) -> Vec<Delivery> {
        self.machine_due(&[agent.to_string()]).remove(agent).unwrap_or_default().into_iter().filter(|d| !self.handed.contains_key(&(agent.to_string(), d.id.clone()))).collect()
    }

    /// Everything due to AGENT, on its way or not, as the machine has it (read with the others asked, `machine_due`):
    /// what the delivery program reads as waiting, and what its messages are cut from (delivery.rs).
    pub fn due_whole(&self, agent: &str) -> Vec<Delivery> {
        self.machine_due(&[agent.to_string()]).remove(agent).unwrap_or_default()
    }

    /// `due` of each of AGENTS, the machine read once for all of them (`Machine::due_all`).
    pub fn due_all(&self, agents: &[String]) -> BTreeMap<String, Vec<Delivery>> {
        let mut all = self.machine_due(agents);
        for (a, due) in all.iter_mut() {
            due.retain(|d| !self.handed.contains_key(&(a.clone(), d.id.clone())));
        }
        all
    }

    /// The machine's `due_all` of AGENTS, read once for every name asked so far and kept until the machine changes
    /// (its `changes`, and its tickets counted): the follow round asks it of every agent, the clerk, the
    /// watcher and the far names each second, and a pass over every ticket's posts each time held the lock most of
    /// that round when nothing had happened (a2h-speed, paper-research N-560 p37). The names kept are asked again
    /// with the next read, so a round's askers share one pass. What has been handed is taken out by the callers.
    /// What `machine_due` keeps, forgotten: the next read is a pass over the machine (the cost tests' rounds before).
    #[cfg(test)]
    pub fn forget_due(&self) {
        *self.due_kept.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// The passes over the machine `machine_due` has made since it last forgot.
    #[cfg(test)]
    pub fn due_passes(&self) -> u64 {
        self.due_kept.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map_or(0, |k| k.passes)
    }

    fn machine_due(&self, agents: &[String]) -> BTreeMap<String, Vec<Delivery>> {
        self.with_due(agents, |all| agents.iter().map(|a| (a.clone(), all.get(a).cloned().unwrap_or_default())).collect())
    }

    /// F of the machine's `due_all` of AGENTS (each one's whole due, on its way or not) as `machine_due` keeps it, read
    /// afresh first when it is not: what the delivery round reads of each agent's due without copying it
    /// (`Core::desk_mine`).
    pub(crate) fn with_due<T>(&self, agents: &[String], f: impl FnOnce(&BTreeMap<String, Vec<Delivery>>) -> T) -> T {
        // its count of changes counts every write to the tickets and their posts (`Counted`, tests/due_kept.rs): no
        // pass over them to count them again, which each read made, dozens a round (N-1397.p38)
        let key = (self.m.changes(), self.m.tickets.len());
        let mut kept = self.due_kept.lock().unwrap_or_else(|e| e.into_inner());
        let fresh = kept.as_ref().is_some_and(|k| k.key == key && agents.iter().all(|a| k.all.contains_key(a)));
        if !fresh {
            let passes = kept.as_ref().map_or(0, |k| k.passes) + 1;
            let mut names: BTreeSet<String> = kept.take().map(|k| k.all.into_keys().collect()).unwrap_or_default();
            names.extend(agents.iter().cloned());
            let names: Vec<String> = names.into_iter().collect();
            *kept = Some(DueKept { key, all: self.m.due_all(&names), passes });
        }
        f(&kept.as_ref().unwrap().all)
    }
}

/// Start the task that asks policy and forms heads off the lock, and load the tickets.
pub fn start(shared: Arc<Mutex<Core>>) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Task>();
    tokio::spawn(async move {
        let (engine, resume) = {
            let mut core = shared.lock().await;
            core.tickets.tasks = Some(tx);
            let resume = core.tickets.m.resume_effects();
            (crate::policy::Engine::start(core.places()), resume)
        };
        shared.lock().await.ticket_effects(resume);
        // the time since a coordd last ran on this record is not silence (quiet.rs)
        if let Err(e) = shared.lock().await.came_up(chrono::Utc::now()) {
            eprintln!("coordd: tickets: {e}");
        }
        expiry(shared.clone());
        clocks(shared.clone(), engine.clone());
        while let Some(task) = rx.recv().await {
            let (shared, engine) = (shared.clone(), engine.clone());
            tokio::spawn(async move {
                let inputs = tokio::task::spawn_blocking(move || run_task(&engine, task)).await;
                match inputs {
                    Ok(inputs) => {
                        // timed as one kind, its lock wait apart (`counts`): what policy and forming answered, applied
                        crate::counts::timed("ticket task results applied", async {
                            let mut core = crate::counts::lock(&shared).await;
                            for input in inputs {
                                if let Err(e) = core.ticket_input(input) {
                                    eprintln!("coordd: tickets: {e}");
                                }
                            }
                        })
                        .await;
                    }
                    Err(e) => eprintln!("coordd: tickets: a task failed: {e}"),
                }
            });
        }
    });
}

/// How often policy is asked whether owed steps have waited too long (`expired`): every minute, so that how long is
/// policy's alone (`silent-after`); a solve is made only while some step is owed.
const EXPIRY_EVERY: std::time::Duration = std::time::Duration::from_secs(60);

/// Ask `expired` of every owed step on a clock: the facts taken under the lock, the norms solved off it, a withdrawn
/// step fed back as `Input::Expired`.
fn expiry(shared: Arc<Mutex<Core>>) {
    tokio::spawn(crate::counts::scoped(async move {
        loop {
            tokio::time::sleep(EXPIRY_EVERY).await;
            let _round = crate::counts::Round::new("clock expired (each minute)");
            // a process killed at its box's memory limit since the last round: told to its agent
            let counts = tokio::task::spawn_blocking(Core::oom_counts).await.unwrap_or_default();
            {
                let mut core = crate::counts::lock(&shared).await;
                core.oom_round(counts);
                // the sidebar's parts, as the owner's policy states them now
                core.parts_round();
            }
            let (places, facts, owed) = {
                let core = crate::counts::lock(&shared).await;
                if let Err(e) = core.tickets.quiet.alive(&core.alive_file(), chrono::Utc::now()) {
                    eprintln!("coordd: tickets: {e}");
                }
                match expiry_question(&core, chrono::Utc::now()) {
                    Some(q) => q,
                    None => continue,
                }
            };
            let expired = tokio::task::spawn_blocking(move || expired(&places, facts, &owed)).await;
            match expired {
                Ok(Ok(found)) => {
                    let mut core = crate::counts::lock(&shared).await;
                    for (t, step, why) in found {
                        let _ = core.ticket_input(Input::Expired { t, step, provenance: json!({"why": why}) });
                    }
                }
                Ok(Err(e)) => eprintln!("coordd: tickets: expired: {e}"),
                Err(e) => eprintln!("coordd: tickets: expired: {e}"),
            }
        }
    }));
}

/// Fire the clock bindings (`every 1h open ...`) that are due, every minute: each as the event `every` naming the
/// binding. When one last fired comes from the log, so a restart does not fire it again before its time.
///
/// On the same clock, the `when D is o` bindings (plan/layers.md I §3): the policy's decision D asked of each, the
/// record's factors as its call (`Machine::when_asks`), solved off the lock; an answer of o fires the binding as the
/// event `when`, the decision in its value (so in the log). A policy with no decision D fires nothing. A decision the
/// rules leave open (more than one option optimal) fires nothing either, never the first by name: it goes to the
/// journal as open, once each time what is open changes (paper-research N-89.p2, 4).
fn clocks(shared: Arc<Mutex<Core>>, engine: crate::policy::Engine) {
    tokio::spawn(crate::counts::scoped(async move {
        let mut open_last: BTreeMap<String, String> = BTreeMap::new();
        loop {
            tokio::time::sleep(EXPIRY_EVERY).await;
            let _round = crate::counts::Round::new("clock when and every (each minute)");
            let asks = {
                let mut core = crate::counts::lock(&shared).await;
                for b in core.tickets.m.clocks_due(&now()) {
                    if let Err(e) = core.ticket_input(Input::Event { name: "every".into(), value: json!(b) }) {
                        eprintln!("coordd: tickets: {b}: {e}");
                    }
                }
                // each with the facts its decision is handed (`Core::decision_facts`), as a pattern's `decide` is
                core.tickets.m.when_asks(&now()).into_iter().map(|(b, d, o, f)| {
                    let situation = core.decision_facts(&d, &|p| f.get(p).cloned());
                    (b, d, o, f, situation)
                }).collect::<Vec<_>>()
            };
            if asks.is_empty() {
                continue;
            }
            let engine = engine.clone();
            let decided = tokio::task::spawn_blocking(move || asks.into_iter().filter_map(|(b, d, o, f, situation)| decide_in(&engine, &d, f, situation).ok().map(|r| (b, o, r))).collect::<Vec<_>>()).await;
            let Ok(decided) = decided else { continue };
            let (decided, open) = when_split(decided);
            for (b, _, r) in &open {
                let what = format!("{} open between {} on {:?}", r.decision, r.optimal.join(", "), r.call);
                if open_last.get(b) != Some(&what) {
                    eprintln!("coordd: tickets: `{b}`: the decision is open ({what}); nothing fires");
                    open_last.insert(b.clone(), what);
                }
            }
            open_last.retain(|b, _| open.iter().any(|(o, _, _)| o == b));
            let mut core = crate::counts::lock(&shared).await;
            for (b, o, r) in decided {
                if let Err(e) = core.ticket_input(Input::Event { name: "when".into(), value: json!({"binding": b, "answer": o, "decision": r}) }) {
                    eprintln!("coordd: tickets: {b}: {e}");
                }
            }
        }
    }));
}

/// The `when D is o` bindings asked, each (binding, o, the decision), split into those that fire (D decided, and o) and
/// those D is open for (more than one option optimal: nothing fires, never the first by name).
type WhenAsked = (String, String, coord_types::Decision);
fn when_split(decided: Vec<WhenAsked>) -> (Vec<WhenAsked>, Vec<WhenAsked>) {
    let (open, decided): (Vec<_>, Vec<_>) = decided.into_iter().partition(|(_, _, r)| r.optimal.len() > 1);
    (decided.into_iter().filter(|(_, o, r)| r.chosen.as_deref() == Some(o.as_str())).collect(), open)
}

impl Core {
    /// The minutes the step of T owed by AGENT since SINCE has been silent at NOW, policy's `waited` (quiet.rs): zero
    /// while AGENT is in a turn or a runner job runs for T (N-159 posts 56, 59).
    pub fn silent_minutes(&self, t: u64, agent: &str, since: &str, now: chrono::DateTime<chrono::Utc>) -> i64 {
        let in_turn = self.agents.get(agent).is_some_and(|a| a.live.state == "working" && a.ended_turn != a.live.turn);
        self.tickets.quiet.waited(t, agent, since, in_turn, now)
    }

    /// The record's alive mark and times down (quiet.rs).
    pub fn alive_file(&self) -> String {
        format!("{}/tickets/alive.json", self.coord)
    }

    /// A coordd starts on this record at NOW: the time since one last ran is a time down (quiet.rs).
    pub fn came_up(&mut self, now: chrono::DateTime<chrono::Utc>) -> Result<(), String> {
        let log = format!("{}/tickets/log.jsonl", self.coord);
        let last = std::fs::read_to_string(&log).unwrap_or_default().lines().rev().find_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()?.get("at")?.as_str().map(str::to_owned));
        let file = self.alive_file();
        self.tickets.quiet.came_up(&file, last.as_deref(), now)
    }
}

/// One minute's `expired` question, taken under the lock: none when no step is owed, so a minute with nothing owed
/// costs no solve (paper-research N-101 p1781).
fn expiry_question(core: &Core, now: chrono::DateTime<chrono::Utc>) -> Option<(crate::policy::Places, Vec<(String, Vec<String>)>, Vec<(u64, u32, String, String, i64)>)> {
    let owed = core.tickets.m.awaiting();
    if owed.is_empty() {
        return None;
    }
    let owed: Vec<(u64, u32, String, String, i64)> = owed.into_iter().map(|(t, s, a, since)| (t, s, a.clone(), core.tickets.m.id(t), core.silent_minutes(t, &a, &since, now))).collect();
    let mut facts = core.norm_facts(&[]);
    // a step held on a ticket the owner archived is not silent (tickets.eflint `expired`)
    facts.extend(owed.iter().filter(|o| core.tickets.m.archived_by(o.0).is_some()).map(|o| ("archived".to_string(), vec![o.3.clone()])));
    Some((core.places(), facts, owed))
}

/// The owed steps OWED (ticket, step, agent, id, minutes waited) for which the policy of PLACES enables `expired`, in
/// the team's situation FACTS: none when the policy has no such act (nothing expires then).
fn expired(places: &crate::policy::Places, mut facts: Vec<(String, Vec<String>)>, owed: &[(u64, u32, String, String, i64)]) -> Result<Vec<(u64, u32, String)>, String> {
    let norms = crate::policy::interp::acts(places)?;
    let Some(act) = norms.acts.get("expired") else { return Ok(Vec::new()) };
    let facts = &mut facts;
    for (_, _, agent, id, minutes) in owed {
        if !facts.iter().any(|(f, v)| f == "agent" && v[0] == *agent) {
            facts.push(("agent".into(), vec![agent.clone()]));
        }
        facts.push(("ticket".into(), vec![id.clone()]));
        facts.push(("awaits".into(), vec![id.clone(), agent.clone()]));
        facts.push(("waited".into(), vec![id.clone(), minutes.to_string()]));
    }
    let shown = norms.solve_for(&["expired"], facts)?;
    let mut out = Vec::new();
    for (t, step, agent, id, _) in owed {
        let args: Vec<&str> = act.fields.iter().map(|(_, ty)| if ty == "ticket" { id.as_str() } else { agent.as_str() }).collect();
        if let Some(v) = norms.verdict(&shown, "expired", &args)? {
            if v.enabled {
                out.push((*t, *step, v.why));
            }
        }
    }
    Ok(out)
}

/// A `may` question taken under the lock (`Core::ticket_may`) and solved off it (`may`): act ACT (`open` or `post`)
/// by WHO, about ticket TICKET or kind KIND, in the situation FACTS; BOUND when WHO is bound by the norms (not the
/// owner). With it, `notify` for the post it makes (for an open, of the ticket to be, asked as `new`).
pub struct MayAsk {
    places: crate::policy::Places,
    facts: Vec<(String, Vec<String>)>,
    act: &'static str,
    who: String,
    ticket: Option<String>,
    kind: String,
    bound: bool,
}

/// The ticket a `--re` names, and the post when it names one: `C-12`, `C-12.p7`.
fn reply_of(m: &Machine, r: &str) -> Option<(u64, Option<u32>)> {
    let (id, rest) = r.split_once('.').map_or((r, None), |(a, b)| (a, Some(b)));
    // an old mail's id (m836, q12): no dash, lower case
    if !id.contains('-') && id.starts_with(|c: char| c.is_ascii_lowercase()) {
        return None;
    }
    let t = docket_machine::parse_ticket(id).filter(|t| m.tickets.contains_key(t))?;
    match rest {
        None => Some((t, None)),
        Some(_) => {
            let (_, n) = docket_machine::post_id(r).filter(|(_, n)| *n >= 1 && *n <= m.tickets[&t].last())?;
            Some((t, Some(n)))
        }
    }
}

/// What `may` found: why the policy went unchecked, if it did; whom `notify` adds to the post, and whom `inform`
/// tells of it without starting a turn (paper-research m1237).
#[derive(Debug, Default, PartialEq)]
pub struct Said {
    pub unchecked: Option<String>,
    pub told: Vec<String>,
    pub inform: Vec<String>,
}

/// Whom `notify` tells of EVENT on TICKET, ACTOR having caused it: every agent of FACTS (and the owner) for whom the
/// act holds, but the actor. A policy without the act tells nobody.
fn notify_set(norms: &crate::policy::interp::Acts, shown: &crate::policy::interp::Shown, name: &str, actor: &str, ticket: &str, event: &str, facts: &[(String, Vec<String>)]) -> Result<Vec<String>, String> {
    let Some(act) = norms.acts.get(name) else { return Ok(Vec::new()) };
    let candidates: BTreeSet<&str> = facts.iter().filter(|(f, _)| f == "agent").map(|(_, v)| v[0].as_str()).chain(["owner"]).filter(|c| *c != actor).collect();
    let mut out = Vec::new();
    for c in candidates {
        let args: Vec<&str> = act.fields.iter().map(|(name, ty)| match (name.as_str(), ty.as_str()) {
            ("actor", _) => actor,
            ("recipient", _) => c,
            (_, "event") => event,
            _ => ticket,
        }).collect();
        if norms.verdict(shown, name, &args)?.is_some_and(|v| v.enabled) {
            out.push(c.to_string());
        }
    }
    Ok(out)
}

/// The facts `notify` is asked with beside the situation: the event, and the owner as an agent it may tell.
fn with_notify_facts(mut facts: Vec<(String, Vec<String>)>, event: &str) -> Vec<(String, Vec<String>)> {
    facts.push(("event".into(), vec![event.into()]));
    if !facts.iter().any(|(f, v)| f == "agent" && v[0] == "owner") {
        facts.push(("agent".into(), vec!["owner".into()]));
    }
    facts
}

/// Whether the norms let the act of ASK: Ok, or why not (blocking: the norms are solved here). A policy without the act
/// lets it through.
/// A policy that cannot be read or solved lets it through too, as `Core::permitted` does for mail: `unchecked`, which
/// the act writes in the log as an `unchecked` line before its own (and nobody is added by `notify` then).
pub fn may(ask: MayAsk) -> Result<Said, String> {
    let through = |e: String| {
        eprintln!("coordd: tickets: may {}: {e}; let through", ask.act);
        Ok(Said { unchecked: Some(e), ..Said::default() })
    };
    let norms = match crate::policy::interp::acts(&ask.places) {
        Ok(n) => n,
        Err(e) => return through(e),
    };
    if !ask.bound && !norms.acts.contains_key("notify") {
        return Ok(Said::default());
    }
    // the ticket the post is on: for an open, the ticket to be
    let on = match (ask.act, &ask.ticket) {
        ("post" | "edit", Some(t)) => t.clone(),
        _ => "new".into(),
    };
    let mut facts = with_notify_facts(ask.facts.clone(), "post");
    if on == "new" {
        facts.push(("ticket".into(), vec!["new".into()]));
        facts.push(("ticket-kind".into(), vec!["new".into(), ask.kind.clone()]));
        facts.push(("opener".into(), vec!["new".into(), ask.who.clone()]));
    }
    let shown = match norms.solve_for(&[ask.act, "notify", "inform"], &facts) {
        Ok(s) => s,
        Err(e) => return through(e),
    };
    if let (true, Some(act)) = (ask.bound, norms.acts.get(ask.act)) {
        let ticket = ask.ticket.clone().unwrap_or_else(|| "none".into());
        let args: Vec<&str> = act.fields.iter().map(|(_, ty)| match ty.as_str() { "agent" => ask.who.as_str(), "ticket" => ticket.as_str(), _ => ask.kind.as_str() }).collect();
        match norms.verdict(&shown, ask.act, &args) {
            Ok(Some(v)) if !v.enabled => {
                return Err(match &ask.ticket {
                    Some(t) => format!("You may not {} on {t}: {}", ask.act, v.why),
                    None => format!("You may not open a {}: {}", ask.kind, v.why),
                });
            }
            Ok(_) => {}
            Err(e) => return through(e),
        }
    }
    if ask.act == "edit" {
        return Ok(Said::default());
    }
    let told = match notify_set(&norms, &shown, "notify", &ask.who, &on, "post", &facts) {
        Ok(told) => told,
        Err(e) => return through(e),
    };
    // a note opened tells by notify only: it has no thread to be read in later
    let inform = match ask.act {
        "post" => match notify_set(&norms, &shown, "inform", &ask.who, &on, "post", &facts) {
            Ok(i) => i.into_iter().filter(|a| !told.contains(a)).collect(),
            Err(e) => return through(e),
        },
        _ => Vec::new(),
    };
    Ok(Said { unchecked: None, told, inform })
}

/// Carry out TASK (blocking: policy solves, a model call) and give the input that answers it, after the decided lines
/// it made (a forming's `model-for`).
fn run_task(engine: &crate::policy::Engine, task: Task) -> Vec<Input> {
    let input = match task {
        Task::Fills { t, step, ticket, role, call, args, facts, places } => {
            // the minder's role with nobody in it is coordd's own stateless call (shape.rs, as the watcher's intake is;
            // design-two-systems §8: on a small project the minder is a fresh call); an agent given the role wins
            let minder = role == crate::shape::MINDER;
            // the step's arguments, then the three every `fills` has, which win over an argument of the same name
            let mut fields = args;
            fields.extend([("role".to_string(), role), ("call".to_string(), call), ("ticket".to_string(), ticket)]);
            let (answer, provenance) = match fills(&places, facts, fields) {
                Ok((None, outcome)) if minder => (Value::String(crate::shape::MINDER.into()), json!({"policy": outcome, "default": "coordd's own minder call"})),
                Ok((chosen, outcome)) => (chosen.map_or(Value::Null, Value::String), outcome),
                Err(e) => (Value::Null, json!({"fault": e})),
            };
            Input::Decided { t, step, question: "fills".into(), answer, provenance }
        }
        Task::Notify { t, event, actor, post, ticket, facts, places } => {
            let facts = with_notify_facts(facts, &event);
            let found = crate::policy::interp::acts(&places).and_then(|n| {
                let shown = n.solve_for(&["notify", "inform"], &facts)?;
                let to = notify_set(&n, &shown, "notify", &actor, &ticket, &event, &facts)?;
                let inform: Vec<String> = notify_set(&n, &shown, "inform", &actor, &ticket, &event, &facts)?.into_iter().filter(|a| !to.contains(a)).collect();
                Ok((to, inform))
            });
            let (to, inform, provenance) = match found {
                Ok((to, inform)) => (to, inform, json!({"asked": "notify", "actor": actor})),
                Err(e) => (Vec::new(), Vec::new(), json!({"fault": e})),
            };
            Input::Notified { t, event, to, inform, post, provenance }
        }
        Task::Event { name, value } => Input::Event { name, value },
        // as `decide` below: one optimal agent names the answer; none, a tie or a fault leaves it to the owner (null)
        Task::Among { t, step, name, fields, facts, places } => {
            let (answer, provenance) = match solved(&places, &name, facts, fields, false) {
                Ok(o) if o.fault.is_none() && o.optimal.len() <= 1 => (o.chosen.clone().map_or(Value::Null, Value::String), serde_json::to_value(&o).unwrap_or(Value::Null)),
                Ok(o) => (Value::Null, serde_json::to_value(&o).unwrap_or(Value::Null)),
                Err(e) => (Value::Null, json!({"fault": e})),
            };
            Input::Decided { t, step, question: "decide".into(), answer, provenance }
        }
        // the decision's rules (the policy's `Decision NAME`, its call the arguments as text and the ticket): their one
        // optimal option names the answer; a decision the policy has not, an open one (a tie) or a fault leaves it to
        // the owner (null). An LLM policy would answer here, between the rules and the owner; none is built.
        Task::Decide { t, step, ticket, name, args, facts, why, situation } => {
            let text = |v: &Value| v.as_str().map_or_else(|| v.to_string(), str::to_string);
            let mut fields: BTreeMap<String, String> = args.as_object().map(|o| o.iter().map(|(k, v)| (k.clone(), text(v))).collect()).unwrap_or_default();
            fields.extend(facts.iter().map(|(k, v)| (k.clone(), text(v))));
            fields.insert("ticket".into(), ticket);
            let (answer, provenance) = match decide_in(engine, &name, fields, situation) {
                Ok(d) if d.fault.is_none() && d.optimal.len() <= 1 => {
                    let answer = match d.chosen.clone() {
                        Some(c) if why => json!({"head": c, "why": rule_text(&d)}),
                        Some(c) => Value::String(c),
                        None => Value::Null,
                    };
                    (answer, serde_json::to_value(&d).unwrap_or(Value::Null))
                }
                Ok(d) => (Value::Null, serde_json::to_value(&d).unwrap_or(Value::Null)),
                Err(e) => (Value::Null, json!({"fault": e})),
            };
            Input::Decided { t, step, question: "decide".into(), answer, provenance }
        }
        // a record that cannot be read for duties lets the answer through, as for `holds`, and says so
        // an ask whose ticket no longer asks it is given up, before it starts or between its events, and says so: a
        // null answer, which the machine records as the ask ended unanswered (docket-core4, N-101 p1664)
        Task::Violated { t, step, answering, policy, log, tree, repos, stop } => {
            let _running = violated::Running::start(violated::EVERY_ASK);
            let ended = || stop.load(std::sync::atomic::Ordering::Relaxed);
            let given_up = |when: &str| json!({"asked": "violated", "answering": [answering.agent, answering.call, answering.head], "ended": format!("the ticket no longer asks it: given up {when}")});
            let (answer, provenance) = if ended() {
                (Value::Null, given_up("before it started"))
            } else {
                let found = policy.ok_or_else(|| String::from("the project's policy declares no duties now")).and_then(|p| violated::violated_until(&p, &violated::record_to_answer(&log, t, step, &answering.agent), &tree, &repos, &answering, &ended));
                match found {
                    Ok(None) => (Value::Null, given_up("while it ran")),
                    Ok(Some(_)) if ended() => (Value::Null, given_up("when it had run")),
                    Ok(Some((duties, why))) => (Value::Array(duties), why),
                    Err(e) => (json!([]), json!({"unchecked": e})),
                }
            };
            Input::Decided { t, step, question: "violated".into(), answer, provenance }
        }
        // a goal's duties read by the policy over its lines (`violated::goal_duties`): none (null) when the goal no
        // longer asks it or the policy cannot read them, and the provenance says which; asked again at its next line
        Task::Duties { t, step, goal, lines, policy, stop } => {
            let _running = violated::Running::start(violated::EVERY_ASK);
            let ended = || stop.load(std::sync::atomic::Ordering::Relaxed);
            let read = policy.as_deref().ok_or_else(|| format!("the project's policy has no {}", violated::FILE)).and_then(|p| violated::goal_duties(p, &goal, &lines, &ended));
            let (answer, provenance) = match read {
                Ok(Some((answer, why))) if !ended() => (answer, why),
                Ok(_) => (Value::Null, json!({"asked": "duties", "goal": goal, "ended": "the goal no longer asks it"})),
                Err(e) => (Value::Null, json!({"asked": "duties", "goal": goal, "unchecked": e})),
            };
            Input::Decided { t, step, question: "duties".into(), answer, provenance }
        }
        Task::Holds { t, step, ticket, actor, goals, facts, places } => {
            let (unmet, provenance) = match holds(&places, &facts, &actor, &ticket, &goals) {
                Ok((unmet, why)) => (unmet, json!({"asked": "holds", "why": why})),
                // a policy that cannot be read or solved lets it through, as for `may`, and says so
                Err(e) => (Vec::new(), json!({"unchecked": e})),
            };
            Input::Decided { t, step, question: "holds".into(), answer: json!(unmet), provenance }
        }
        Task::Form { t, step, agent, head, text, words, asked, shown, coord } => {
            // the policy's order, for each kept call's model as a reader is shown it (its number and class)
            let order = asked.as_ref().map(|a| &a.2).or(shown.as_ref().map(|s| &s.3)).and_then(|p| crate::models::Order::read(p, &[]).ok());
            let render = |call: &str, args: &Value, places: &crate::policy::Places| crate::policy::call::Calls::load(&places.call_dirs(), &places.overlay).ok().and_then(|c| c.render_step(call, &crate::render::resolved(args)));
            // the question the owner's words answer, as the owner was shown it: the call's text with its arguments
            // what the step gives with its docs, as the agent's step showed it (`doc::gives`), for the marks reading
            let gives = shown.as_ref().map(|s| (&s.1, &s.3)).or(asked.as_ref().map(|a| (&a.0, &a.2))).map(|(call, places)| crate::render::gives(places, call, &head, None));
            // the lists of the answer whose entries name other elements of it (`Defs.names`: `stands_on` names
            // `parts[].name`), for an agent's answer
            let names = shown.as_ref().map(|s| crate::render::names(&s.3, &s.1)).unwrap_or_default();
            let context = asked.map(|(call, args, places)| render(&call, &args, &places).unwrap_or_else(|| format!("({call}: {args})")));
            // the step an agent's answer answers, as the agent was shown it: the delivery as written in its session,
            // whole (none is cut: a long design goes as it is), else rendered again; none when neither reads
            let shown = shown.and_then(|(at, call, args, places)| at.and_then(|l| crate::follow::text_at(&l.file, l.offset, l.len)).filter(|s| !s.trim().is_empty()).or_else(|| render(&call, &args, &places)));
            // the owner's words placed on a ticket that owes the owner a step come as they are; an agent's turn by place
            let words = words.unwrap_or_else(|| crate::follow::text_at(&text.file, text.offset, text.len).unwrap_or_default());
            let loc = serde_json::to_value(&text).unwrap_or(Value::Null);
            // a forming call that faults is coordd's own failure: its journal says so (once per fault), and it forms
            // again a minute later, for as long as it faults; it is never taken as an answer with no form (N-159.p16)
            let mut last: Option<String> = None;
            let r = loop {
                if words.trim().is_empty() {
                    break crate::form::Reading { not_taken: Some(String::from("The answer is empty.")), ..Default::default() };
                }
                match crate::form::form(engine, &agent, &head, &words, &loc, context.as_deref(), shown.as_deref(), gives.as_deref(), &names) {
                    Ok(r) => break r,
                    Err(e) => {
                        if last.as_ref() != Some(&e) {
                            eprintln!("coordd: tickets: forming {agent}'s answer to step {step} of ticket {t} faulted: {e}; formed again each minute until it does not");
                            last = Some(e);
                        }
                        std::thread::sleep(std::time::Duration::from_secs(60));
                    }
                }
            };
            let mut r = r;
            for k in r.calls.iter_mut() {
                k.class = k.model.as_deref().zip(order.as_ref()).map(|(m, o)| o.show(m));
            }
            let call = keep_forming(&coord, t, step, &agent, &r.calls);
            let (why, not_taken) = if r.head.is_none() { (r.why, r.not_taken) } else { (None, None) };
            return r.decision.map(|d| model_for(t, step, &d)).into_iter().chain([Input::Formed { t, step, head: r.head, why, not_taken, call }]).collect();
        }
    };
    vec![input]
}

/// The forming calls CALLS made on AGENT's answer to STEP of T, kept whole under COORD (`tickets/forming/T.sSTEP.N.json`,
/// N the next free): where, relative to COORD, for the `formed` line (paper-research N-101 p554, 1). None when no call
/// was made, or the file could not be written (the journal says why).
fn keep_forming(coord: &str, t: u64, step: u32, agent: &str, calls: &[crate::form::Kept]) -> Option<String> {
    if calls.is_empty() {
        return None;
    }
    let dir = std::path::Path::new(coord).join("tickets/forming");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("coordd: tickets: the forming calls on step {step} of ticket {t} are not kept: {}: {e}", dir.display());
        return None;
    }
    let body = serde_json::to_string_pretty(&json!({"t": t, "step": step, "agent": agent, "calls": calls})).unwrap_or_default();
    for n in 1.. {
        let name = format!("tickets/forming/{t}.s{step}.{n}.json");
        match std::fs::OpenOptions::new().write(true).create_new(true).open(std::path::Path::new(coord).join(&name)) {
            Ok(mut f) => {
                use std::io::Write;
                return match f.write_all(body.as_bytes()) {
                    Ok(()) => Some(name),
                    Err(e) => {
                        eprintln!("coordd: tickets: the forming calls on step {step} of ticket {t} are not kept: {name}: {e}");
                        None
                    }
                };
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                eprintln!("coordd: tickets: the forming calls on step {step} of ticket {t} are not kept: {name}: {e}");
                return None;
            }
        }
    }
    None
}

/// The strings V gives where type TY has an Agent, into OUT: through variants (by `head`), records, lists, tuples and
/// options.
/// Each agent answer V (of type TY) names, with its place in the answer (`docket_machine::given::place`: the
/// alternative, `.field`, `[]` for a list's element), as `agents_named` finds them; AT is the place so far.
fn agents_at(ty: &docket_machine::Type, v: &Value, at: &mut Vec<String>, out: &mut Vec<(String, String)>) {
    let under = |seg: &str, t: &docket_machine::Type, x: &Value, at: &mut Vec<String>, out: &mut Vec<(String, String)>| {
        at.push(seg.to_string());
        agents_at(t, x, at, out);
        at.pop();
    };
    match (ty, v) {
        (Type::Agent, Value::String(s)) => out.push((docket_machine::given::place(at), s.trim().to_string())),
        (Type::Opt(t), v) if !v.is_null() => agents_at(t, v, at, out),
        (Type::List(t), Value::Array(xs)) => xs.iter().for_each(|x| under("[]", t, x, at, out)),
        (Type::Record(fs), Value::Object(o)) => fs.iter().filter_map(|(f, t)| Some((f, t, o.get(f)?))).for_each(|(f, t, x)| under(f, t, x, at, out)),
        (Type::Variant(_), Value::Object(o)) => {
            if let Some((h, fs)) = o.get("head").and_then(Value::as_str).and_then(|h| Some((h, ty.constructor(h)?))) {
                at.push(h.to_string());
                fs.iter().filter_map(|(f, t)| Some((f, t, o.get(f)?))).for_each(|(f, t, x)| under(f, t, x, at, out));
                at.pop();
            }
        }
        _ => {}
    }
}

fn agents_named(ty: &docket_machine::Type, v: &Value, out: &mut Vec<String>) {
    match (ty, v) {
        (Type::Agent, Value::String(s)) => out.push(s.trim().to_string()),
        (Type::Opt(t), v) if !v.is_null() => agents_named(t, v, out),
        (Type::List(t), Value::Array(xs)) => xs.iter().for_each(|x| agents_named(t, x, out)),
        (Type::Tuple(ts), Value::Array(xs)) => ts.iter().zip(xs).for_each(|(t, x)| agents_named(t, x, out)),
        (Type::Record(fs), Value::Object(o)) => fs.iter().filter_map(|(f, t)| Some((t, o.get(f)?))).for_each(|(t, x)| agents_named(t, x, out)),
        (Type::Variant(_), Value::Object(o)) => {
            if let Some(fs) = o.get("head").and_then(Value::as_str).and_then(|h| ty.constructor(h)) {
                fs.iter().filter_map(|(f, t)| Some((t, o.get(f)?))).for_each(|(t, x)| agents_named(t, x, out));
            }
        }
        _ => {}
    }
}

/// The text of the rule that chose decision D's option (N-101 p158): the preference that decided it over the
/// runner-up, else the hard rules (`Only`) that excluded the others, else that it was the only option.
/// The `why` a decision gives: the deciding rule's sentence for its reader (its doc, else its `//` comment), else the
/// rule as written with its place; the rule's text stays in the record's provenance either way (paper-research N-101
/// p1091: the mending agent was shown `Only for whose-red When call.carries-count == 1: …`).
/// Decided in facts it was handed ([`decide_in`]), each sentence has its rule's place and the facts that made the rule
/// hold, as its [`coord_types::Grounds`] say them (proto-mind N-1446 p1502 (9), paper-research p1617), so a step the
/// answer's `why` is handed to (`again`, `read`) shows them: the preference's, then what kept each rule that would
/// leave the chosen option out from holding (a job that ran green); else those of each rule that left the others out
/// (a job that failed, and its failing tests).
fn rule_text(d: &coord_types::Decision) -> String {
    if !d.grounds.is_empty() {
        return grounded_text(d);
    }
    if let Some(c) = &d.decided_by {
        return c.why.clone().unwrap_or_else(|| c.rule.clone());
    }
    let mut rules: Vec<&str> = d.excluded.iter().filter(|e| Some(&e.option) != d.chosen.as_ref()).map(|e| e.why.as_deref().unwrap_or(&e.rule)).collect();
    rules.dedup();
    if rules.is_empty() { String::from("the only option") } else { rules.join("; ") }
}

/// [`rule_text`] of decision D decided in facts it was handed: each sentence without its full stop, then its rule's
/// place and the facts that made the rule hold, the whole ending with one.
fn grounded_text(d: &coord_types::Decision) -> String {
    let grounds = |rule: &str| d.grounds.iter().find(|g| g.rule == rule);
    let listed = |xs: &[String]| if xs.len() > 24 { format!("{} and {} more", xs[..24].join(", "), xs.len() - 24) } else { xs.join(", ") };
    let said = |g: &coord_types::Grounds| if g.from.is_empty() { listed(&g.facts) } else { format!("{}, from {}", listed(&g.facts), listed(&g.from)) };
    let placed = |s: &str, g: Option<&coord_types::Grounds>| {
        let s = s.trim_end_matches('.');
        match g {
            Some(g) if g.facts.is_empty() => format!("{s} ({})", g.at),
            Some(g) => format!("{s} ({}): {}", g.at, said(g)),
            None => s.to_string(),
        }
    };
    let text = if let Some(c) = &d.decided_by {
        let s = placed(c.why.as_deref().unwrap_or(&c.rule), grounds(&c.rule));
        let kept: Vec<String> = d.grounds.iter().filter(|g| !g.held && d.chosen.as_ref().is_some_and(|o| g.bars.contains(o))).map(|g| format!("{} ({})", said(g), g.at)).collect();
        if kept.is_empty() { s } else { format!("{s}; no rule left it out, as {}", kept.join(" and ")) }
    } else {
        let mut rules: Vec<String> = d.excluded.iter().filter(|e| Some(&e.option) != d.chosen.as_ref()).map(|e| placed(e.why.as_deref().unwrap_or(&e.rule), grounds(&e.rule).filter(|g| g.held))).collect();
        rules.dedup();
        if rules.is_empty() {
            return String::from("the only option");
        }
        rules.join("; ")
    };
    format!("{text}.")
}

/// Decision NAME for the call's FIELDS (blocking, off the lock): with SITUATION, the facts it is handed
/// ([`Core::decision_facts`]), on a solver of its own made for this question ([`solved`]), as those facts are this
/// call's alone, with what made its rules hold in them, for its `why` ([`rule_text`]; proto-mind p1502 (9)); else the
/// coord's engine's, as before. A pattern's `decide` (`Task::Decide`) and a `when D is o` binding's ask (`clocks`)
/// both go through it (proto-mind N-1446 p1502 (1), p1592).
fn decide_in(engine: &crate::policy::Engine, name: &str, fields: BTreeMap<String, String>, situation: Option<Situation>) -> Result<coord_types::Decision, String> {
    match situation {
        Some((given, places)) => solved(&places, name, given, fields.clone(), true).map(|o| decision_of(name, fields, o)),
        None => engine.decide(name, fields),
    }
}

/// The record of decision KIND for the call's fields CALL that [`solved`] gave as O, as the engine's `decide` records one
/// (`Engine::decided`): its options, costs and rules, and what made its rules hold, no attempt; `fault` when no
/// option is left.
fn decision_of(kind: &str, call: BTreeMap<String, String>, o: crate::policy::solve::Outcome) -> coord_types::Decision {
    let fault = if o.chosen.is_some() { None } else { o.fault.or(Some("no option left".into())) };
    coord_types::Decision { at: crate::core::now(), decision: kind.into(), call, chosen: o.chosen, optimal: o.optimal, excluded: o.excluded, costs: o.costs, runner_up: o.runner_up, decided_by: o.decided_by, fault, grounds: o.grounds, ..Default::default() }
}

/// The `model-for` decided line for decision D, made for step STEP of T: its answer the model chosen, D its provenance.
fn model_for(t: u64, step: u32, d: &coord_types::Decision) -> Input {
    let answer = d.chosen.clone().map_or(Value::Null, Value::String);
    Input::Decided { t, step, question: "model-for".into(), answer, provenance: serde_json::to_value(d).unwrap_or(Value::Null) }
}

/// PROVENANCE as a `decided` line writes it: each rule that left options out written once, with the options it left
/// out in the order they came (`options`), at the top (`model-for`, `decide`) and under `policy` (`fills`). The same
/// facts as one entry per option ([`coord_types::Exclusion`]), which the decision in hand keeps and a line written before
/// this has; [`coord_types::excluded_options`] reads both. Research's record was 230 MB, 150.6 MB of it `excluded`: each
/// of its 264,360 entries with its rule and the rule's sentence again (paper-research N-1446 p2667).
fn excluded_by_rule(mut provenance: Value) -> Value {
    fn by_rule(excluded: &mut Value) {
        let Some(list) = excluded.as_array() else { return };
        // a list written by rule already, or of another shape, stays as it is
        if !list.iter().all(|e| e.get("option").is_some() && e.get("options").is_none()) {
            return;
        }
        let mut rules: Vec<Value> = Vec::new();
        for e in list {
            match rules.iter_mut().find(|g| ["reason", "rule", "why"].iter().all(|k| g.get(*k) == e.get(*k))) {
                Some(g) => g["options"].as_array_mut().expect("made with its options").push(e["option"].clone()),
                None => {
                    let mut g = serde_json::Map::new();
                    for (k, v) in e.as_object().into_iter().flatten().filter(|(k, _)| *k != "option") {
                        g.insert(k.clone(), v.clone());
                    }
                    g.insert("options".into(), json!([e["option"]]));
                    rules.push(Value::Object(g));
                }
            }
        }
        *excluded = Value::Array(rules);
    }
    if let Some(e) = provenance.get_mut("excluded") {
        by_rule(e);
    }
    if let Some(e) = provenance.get_mut("policy").and_then(|p| p.get_mut("excluded")) {
        by_rule(e);
    }
    provenance
}

/// Which of GOALS do not hold for TICKET, each the policy's act of its name for ACTOR (blocking, off the lock), and
/// why each does or does not. A goal the policy has no act for does not hold.
fn holds(places: &crate::policy::Places, facts: &[(String, Vec<String>)], actor: &str, ticket: &str, goals: &[String]) -> Result<(Vec<String>, Vec<String>), String> {
    let norms = crate::policy::interp::acts(places)?;
    let asked: Vec<&str> = goals.iter().map(String::as_str).collect();
    let shown = norms.solve_for(&asked, facts)?;
    let (mut unmet, mut why) = (Vec::new(), Vec::new());
    for g in goals {
        let Some(act) = norms.acts.get(g) else {
            unmet.push(g.clone());
            why.push(format!("{g}: the policy has no act {g}"));
            continue;
        };
        let args: Vec<&str> = act.fields.iter().map(|(_, ty)| if ty == "agent" { actor } else { ticket }).collect();
        match norms.verdict(&shown, g, &args)? {
            Some(v) if v.enabled => why.push(format!("{g} holds: {}", v.why)),
            Some(v) => {
                unmet.push(g.clone());
                why.push(format!("{g} does not hold: {}", v.why));
            }
            None => unmet.push(g.clone()),
        }
    }
    Ok((unmet, why))
}

/// Each text in a list among ARGS, a decision's or a step's arguments, as among(X, I), I from 0 across the lists in
/// their order: the agents a decision among agents is given (`among_facts`), the names a `fills` step is about (a
/// text change's read: the names it touches, paper-research N-1446 p2894).
fn among_given(args: &Value) -> Vec<(String, Vec<String>)> {
    let given = args.as_object().into_iter().flat_map(|o| o.values()).filter_map(Value::as_array).flatten().filter_map(Value::as_str);
    given.enumerate().map(|(i, a)| ("among".to_string(), vec![a.to_string(), i.to_string()])).collect()
}

/// A step's ARGS that are text, numbers or truths, each as text by its name: what `fills` is given of them in its
/// call's fields (`call.by`). A list is given as among(X, I) instead ([`among_given`]) and a record not at all.
fn step_fields(args: &Value) -> BTreeMap<String, String> {
    let text = |v: &Value| match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(_) | Value::Bool(_) => Some(v.to_string()),
        _ => None,
    };
    args.as_object().into_iter().flatten().filter_map(|(k, v)| Some((k.clone(), text(v)?))).collect()
}

/// Decide `fills` for FIELDS (role, call, ticket) in the situation FACTS (the team's and the ticket's) (`solved`): the
/// agent chosen, and the outcome as provenance (options excluded and why).
fn fills(places: &crate::policy::Places, facts: Vec<(String, Vec<String>)>, fields: BTreeMap<String, String>) -> Result<(Option<String>, Value), String> {
    let o = solved(places, "fills", facts, fields, false)?;
    Ok((o.chosen.clone(), serde_json::to_value(&o).unwrap_or(Value::Null)))
}

/// Decide KIND for FIELDS in the situation FACTS, on a solver of its own made for this question (blocking, off the
/// lock): `fills`, and a `decide` among agents (`Task::Among`). Its own solver, because the facts are the ticket's: two
/// tickets asked at once never share them. The coord's decider says what answers (`policy::Decider`): in `both` the
/// solver's answer stands and the picker's, made from the same facts, is set beside it (`pick::beside`). WHY: with
/// what made each of its rules hold or not in FACTS (`Solver::explain`, for a decision's `why`; proto-mind N-1446
/// p1502 (9)); the picker says none.
fn solved(places: &crate::policy::Places, kind: &str, facts: Vec<(String, Vec<String>)>, fields: BTreeMap<String, String>, why: bool) -> Result<crate::policy::solve::Outcome, String> {
    use crate::policy::{Decider, pick, solve::Instance};
    let mode = places.decider();
    let (sources, compiled, facts) = typed(places, facts)?;
    let inst = Instance { kind: kind.into(), call: fields, given: Vec::new(), unmet: Vec::new() };
    let picker = || pick::guarded(|| pick::Picker::new(&sources, &facts));
    let o = if mode == Decider::Picker {
        crate::policy::solve::supplied(&compiled.facts, &facts)?;
        pick::guarded(|| picker()?.pick(&inst, &[]))?.0
    } else {
        let mut s = crate::policy::solve::Solver::new(compiled);
        s.set_facts(facts.clone())?;
        let d = s.open(&inst)?;
        let mut outcome = s.solve(&d, &inst);
        if let Some(o) = outcome.as_mut().ok().filter(|_| why) {
            // the decision stands without them
            match s.explain(&d) {
                Ok(g) => o.grounds = g,
                Err(e) => eprintln!("coordd: policy: {kind}: what made its rules hold could not be read: {e}"),
            }
        }
        s.close(&d)?;
        let o = outcome?;
        if mode == Decider::Both {
            if let Some(line) = pick::beside(picker().as_ref().map_err(String::as_str), &inst, &[], Ok(&o)) {
                eprintln!("{line}");
            }
        }
        o
    };
    Ok(o)
}

/// The policy's declarations, compiled, and FACTS supplied as it declares them: only the facts it declares with
/// their number of identifiers, an Int identifier as a number.
#[allow(clippy::type_complexity)]
fn typed(places: &crate::policy::Places, facts: Vec<(String, Vec<String>)>) -> Result<(Vec<(crate::policy::compile::Scope, Vec<crate::policy::syntax::Decl>)>, crate::policy::compile::Compiled, Vec<(String, Vec<Value>)>), String> {
    let sources = places.sources()?;
    let compiled = crate::policy::compile::compile(&sources)?;
    let facts: Vec<(String, Vec<Value>)> = facts
        .into_iter()
        .filter_map(|(f, ids)| {
            let types = compiled.facts.get(&f).filter(|t| t.len() == ids.len())?.clone();
            Some((f, ids.into_iter().zip(types).map(|(i, t)| if t == "Int" { i.parse::<i64>().map_or(json!(i), |n| json!(n)) } else { json!(i) }).collect()))
        })
        .collect();
    Ok((sources, compiled, facts))
}

/// Who holds which role (`Core::record_roles`; blocking: a policy solve): each agent's roles as the policy has them,
/// FACTS' `role` facts (the registry's) and the policy's own `+role(A, R)` lines (agent-base/policy/prelude.eflint
/// `Fact role`). `step role R` and an ask's lead are the policy's `fills`, which reads the same facts; what the
/// machine and coordd name for a role besides is the record's role lines made from this (`Machine::holders`). As
/// [`fills`], the coord's decider says what answers, and in `both` the picker's holding is set beside the solver's.
pub fn roles_held(places: &crate::policy::Places, facts: Vec<(String, Vec<String>)>) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    use crate::policy::{Decider, pick};
    let mode = places.decider();
    let (sources, compiled, facts) = typed(places, facts)?;
    let picker = || pick::guarded(|| pick::Picker::new(&sources, &facts));
    let held = if mode == Decider::Picker {
        crate::policy::solve::supplied(&compiled.facts, &facts)?;
        let p = picker()?;
        pick::guarded(|| Ok(p.holding("role")))?
    } else {
        let mut s = crate::policy::solve::Solver::new(compiled);
        s.set_facts(facts.clone())?;
        let held = s.holding("role")?;
        if mode == Decider::Both {
            if let Some(line) = pick::holding_beside(picker().as_ref().map_err(String::as_str), "role", &held) {
                eprintln!("{line}");
            }
        }
        held
    };
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for a in held {
        if let [agent, role] = &a[..] {
            out.entry(agent.clone()).or_default().insert(role.clone());
        }
    }
    Ok(out)
}

/// Who fills `lead` (`Core::lead_ask`; blocking: a policy solve), for the owner's words typed to nobody.
pub fn lead_of(places: &crate::policy::Places, facts: Vec<(String, Vec<String>)>) -> Option<String> {
    let fields = BTreeMap::from([("role".to_string(), "lead".to_string()), ("call".to_string(), "owner_message".to_string()), ("ticket".to_string(), "none".to_string())]);
    fills(places, facts, fields).ok().and_then(|(a, _)| a)
}

/// The agents a post's TEXT names (`@NAME`) among KNOWN.
fn named_in(text: &str, known: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in text.split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '(' || c == ')') {
        if let Some(n) = w.strip_prefix('@') {
            let n = n.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_');
            if known(n) && !out.iter().any(|o| o == n) {
                out.push(n.to_string());
            }
        }
    }
    out
}

/// Where a part of a turn's last message goes ([`reply_parts`]).
#[derive(Debug, Clone, PartialEq)]
pub enum To {
    /// The thread handed at this index, under the post it answers.
    Thread(usize, u32),
    /// A ticket here the turn was not handed, by its id, and the post an address `#ID.pN` named: the part is posted
    /// there as `coord post` posts it, under that post (paper-research N-412.p2208: such a part was dropped whole).
    Ticket(String, Option<u32>),
}

/// A turn's last message TEXT, given posts on THREADS (`Machine::reply_threads`, in the order handed), as the parts it
/// answers them with (N-807): where each goes, and the part's words, one per thread, in the order of each thread's
/// first part. A line that begins with an address starts a part, up to the next such line: `#N-12` a thread given (or
/// `#N-12.p3`, a post's id on it), `#2` the second thread given, `@NAME` the first thread given where NAME's post was
/// (the line kept as written, so it names NAME too), and `#N-40` (or `#N-40.p3`), a ticket here not given (TICKET: its
/// id when ID is one, as written), the ticket itself, under the post named. A `#` address is taken out of the words. An
/// address that fits none of these is words; nothing inside a ``` fence is an address. Words before the first address
/// (or all of them, with none) go on the thread given at DEFAULT ([`first_told`] of several; of one, the thread the
/// rule Reply would have posted the message on, if any: consult-3, N-1397 p132), else nowhere.
/// A part that is empty or the single word Nothing is no part. A part by `#` or by no address answers the owner's
/// `@all` the agent holds there (`ReplyThread::all`) in place of the last post, so it is the claim, as a one-thread
/// turn's reply is (consult-3, N-807 post 118); a part by `@NAME` answers NAME's post.
pub fn reply_parts(text: &str, threads: &[docket_machine::machine::ReplyThread], default: Option<usize>, ticket: &dyn Fn(&str) -> Option<String>) -> Vec<(To, String)> {
    let owed = |i: usize| To::Thread(i, threads[i].all.unwrap_or(threads[i].last));
    let address = |line: &str| -> Option<(To, String)> {
        let word = line.split_whitespace().next()?;
        let bare = word.trim_end_matches(|c: char| matches!(c, ':' | ',' | '.' | ';' | ')' | '·'));
        if let Some(h) = bare.strip_prefix('#') {
            let to = match h.parse::<usize>() {
                Ok(k) if k >= 1 && k <= threads.len() => owed(k - 1),
                Ok(_) => return None,
                Err(_) => {
                    let (id, n) = h.split_once(".p").and_then(|(t, n)| Some((t, Some(n.parse::<u32>().ok()?)))).unwrap_or((h, None));
                    match threads.iter().position(|x| x.id == id) {
                        Some(i) => owed(i),
                        None => To::Ticket(ticket(id)?, n),
                    }
                }
            };
            let rest = line.trim_start()[word.len()..].trim_start();
            let rest = rest.trim_start_matches(|c: char| matches!(c, '·' | '-' | '—' | ':')).trim_start();
            return Some((to, rest.to_string()));
        }
        let name = bare.strip_prefix('@')?;
        threads.iter().enumerate().find_map(|(i, x)| x.authors.iter().find(|(a, _)| a == name).map(|(_, n)| (To::Thread(i, *n), line.to_string())))
    };
    let mut parts: Vec<(Option<To>, Vec<String>)> = vec![(None, Vec::new())];
    let mut fenced = false;
    for line in text.lines() {
        let at = if fenced { None } else { address(line) };
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        match at {
            Some((to, first)) => parts.push((Some(to), vec![first])),
            None => parts.last_mut().unwrap().1.push(line.to_string()),
        }
    }
    let default = default.filter(|i| *i < threads.len());
    let mut out: Vec<(To, String)> = Vec::new();
    for (to, lines) in parts {
        let words = lines.join("\n").trim().to_string();
        if words.is_empty() || says_nothing(&words) {
            continue;
        }
        let Some(to) = to.or_else(|| default.map(owed)) else { continue };
        let same = |o: &To| match (o, &to) {
            (To::Thread(j, _), To::Thread(i, _)) => j == i,
            (To::Ticket(a, _), To::Ticket(b, _)) => a == b,
            _ => false,
        };
        match out.iter_mut().find(|(o, _)| same(o)) {
            Some((o, w)) => {
                // two parts for one thread: the held `@all` if either answers it, else the later post
                match (o, &to) {
                    (To::Thread(i, e), To::Thread(_, n)) => *e = threads[*i].all.filter(|a| *a == *e || *a == *n).unwrap_or((*e).max(*n)),
                    (To::Ticket(_, e), To::Ticket(_, n)) => *e = (*e).max(*n),
                    _ => {}
                }
                *w = format!("{w}\n\n{words}");
            }
            None => out.push((to, words)),
        }
    }
    out
}

/// Of the several THREADS a turn was given, the one its words without an address go on ([`reply_parts`]): the first
/// that told the agent something and that it has not answered by hand.
pub fn first_told(threads: &[docket_machine::machine::ReplyThread]) -> Option<usize> {
    threads.iter().position(|x| x.told && !x.answered)
}

/// Whether a turn's last message TEXT is the single word Nothing: any case, one full stop allowed, nothing else
/// (paper-research N-17). 'Nothing more from me.' is a reply. Also when every line of it is a thread's address with
/// Nothing after it, or the address alone, and Nothing is said at least once (`#N-12 Nothing`, `#N-12.p3: Nothing.`,
/// `#2 · nothing`): a turn that answers each thread it was given with Nothing in parts posts nothing, on one thread
/// or on several (paper-research N-1397 p113: `#N-1397.p109 Nothing` was posted).
pub fn says_nothing(text: &str) -> bool {
    let word = |t: &str| {
        let t = t.trim();
        t.strip_suffix('.').unwrap_or(t).eq_ignore_ascii_case("nothing")
    };
    if word(text) {
        return true;
    }
    let mut said = false;
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let rest = match line.split_whitespace().next().filter(|w| w.len() > 1 && w.starts_with('#')) {
            Some(w) => line[w.len()..].trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '·' | '-' | '—' | ':')),
            None => line,
        };
        if rest.is_empty() {
            continue;
        }
        if !word(rest) {
            return false;
        }
        said = true;
    }
    said
}

/// A name of another project, PROJECT/NAME (N-44): its project and its name there. None for a name here.
pub fn far(n: &str) -> Option<(&str, &str)> {
    n.split_once('/').filter(|(p, a)| !p.is_empty() && !a.is_empty() && !a.contains('/'))
}

/// The relay's name, here (coord-relay) or as another project wrote it (X/coord-relay). Records from before N-65 hold
/// its posts (its "Not delivered" notices, carried between projects in a loop); it takes part in no ticket: it is
/// never named, told or addressed, and nothing it wrote leaves the project.
pub fn is_relay(n: &str) -> bool {
    n == "coord-relay" || far(n).is_some_and(|(_, a)| a == "coord-relay")
}

/// Why a project without the relay carries no message out (N-44).
pub const NOT_CARRIED: &str = "this project's messages are not carried to other projects (no host_relay = true in its project file)";

/// The form of a thread an agent's message opens (`coord msg`, design part 9b): to the first it names, typed to all
/// of them, addressed Sent(BY).
pub fn sent_form(by: &str, names: &[String], text: &str) -> Value {
    json!({"to": names.first().cloned().unwrap_or_default(), "typed": names, "names": names, "text": text, "addressed": {"head": "Sent", "by": by}, "follows": null})
}

/// The ticket a request names by S, written as its id (`Machine::ticket_named`): `412`, `n-412` and an agent's name
/// that reads as a number name none. A ticket's number written another way (alone, in small letters, `I-1` for
/// `I-001`) is refused with how its id is written, so the answer never reads as if the ticket were gone (consult-3
/// N-1397.p100); an agent's name (`consult-3`) is refused plainly, never pointed at a ticket.
fn ticket_of(m: &Machine, s: &str) -> Result<u64, String> {
    m.ticket_named(s).ok_or_else(|| match docket_machine::parse_ticket(s).filter(|t| m.tickets.contains_key(t)).map(|t| m.id(t)) {
        Some(id) if written_otherwise(s, &id) => format!("There is no ticket {s}: its id is written {id}."),
        _ => format!("There is no ticket {s}."),
    })
}

/// Whether S is the ticket ID written another way: its digits alone, or after its own letter in either case.
fn written_otherwise(s: &str, id: &str) -> bool {
    let s = s.trim();
    let (letter, digits) = s.split_once('-').unwrap_or(("", s));
    let own = id.split_once('-').map_or("", |(l, _)| l);
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) && (letter.is_empty() || letter.eq_ignore_ascii_case(own))
}

impl Core {
    /// Load the tickets: kinds and calls of the system and the project, the log replayed. The first ticket number is
    /// above every item's (items keep their numbers).
    pub fn load_tickets(&mut self) {
        // a text-change's load that stopped before its commit: its files as they were, before they are read
        self.text_recover_files();
        let first = crate::core::first_ticket(self.item_ids().iter().map(String::as_str));
        let project = self.project.as_ref().map(|p| p.dir.clone());
        // the places index readers hold already (`Shared`) stays the one: the reloaded record's places go into it
        let places = Arc::clone(&self.tickets.places);
        self.tickets = Tickets::load_joint(&self.coord, &self.base, project.as_deref(), first, crate::joint::switched_on(&self.coord), true);
        let loaded = std::mem::take(&mut *self.tickets.places.write().unwrap_or_else(|e| e.into_inner()));
        *places.write().unwrap_or_else(|e| e.into_inner()) = loaded;
        self.tickets.places = places;
        // whether an ask waits in the asker's turn (the ask tool; the project file's block_ask)
        self.tickets.m.defs.edit().block_ask = self.project.as_ref().is_some_and(|p| p.project.block_ask);
        // what every post names (threads piece 3), before anything new lands
        let started = std::time::Instant::now();
        let Tickets { m, mentions, .. } = &mut self.tickets;
        mentions.read_all(m, &crate::label::read);
        let Tickets { m, holds, .. } = &mut self.tickets;
        holds.read_all(m);
        let (posts, ids) = self.tickets.mentions.size();
        eprintln!("coordd: mentions: {posts} posts name {ids} ids, read in {:.2} s", started.elapsed().as_secs_f64());
        // and the record graph's `named` edges from them, as docket-edge6's printout adds them to `graph::of`'s
        // (`named_into`), each at its post's line and time (proto-window N-1446 p1383, head 1)
        let mut g = std::mem::take(&mut self.tickets.graph.graph);
        self.tickets.mentions.named_into(&self.tickets.m, &|a| self.is_agent(a), &mut g);
        self.tickets.graph.graph = g;
        // the kept index of what his asks and the canvas's `now` read, made here once, so no read under the lock
        // makes it (paper-research N-1446 p1828)
        let started = std::time::Instant::now();
        self.tickets.graph.graph.keep();
        eprintln!("coordd: graph: kept index of {} edges made in {:.2} s", self.tickets.graph.graph.edges.len(), started.elapsed().as_secs_f64());
        // the registry's agents named as the program names its own that this record did not make (a record set aside,
        // the registry kept): taken, so a fresh step never gets one (docket-core3 N-101 p609, paper-research p610)
        let names: Vec<String> = self.agents.keys().cloned().collect();
        let taken = self.tickets.m.not_made_here(&names);
        if !taken.is_empty() {
            if let Err(e) = self.ticket_input(Input::Taken { names: taken }) {
                eprintln!("coordd: tickets: taken names: {e}");
            }
        }
        self.refusals();
        self.release_picks();
        // the first start of an engine that hands the owner's held posts (paper-research N-807.p78, 5): the posts held
        // now under a question to him are handed to every agent on their threads, by one record line, never again
        if !self.tickets.m.held_handed && !self.tickets.m.tickets.is_empty() {
            let held = self.tickets.m.unanswered_holds();
            let values: Vec<Value> = if held.is_empty() { vec![json!({})] } else { held.iter().map(|(t, n, _)| json!({"t": t, "n": n})).collect() };
            for value in values {
                if let Err(e) = self.ticket_input(Input::Event { name: docket_machine::HELD_HANDED.into(), value }) {
                    eprintln!("coordd: tickets: the owner's held posts: {e}");
                }
            }
        }
        // the record's era (docket_machine::ERA; consult-3, N-807 post 118), begun at the first start of an engine
        // with its rules, so the lines before replay as they ran: names (ERA_NAMES) in a record with an agent on a
        // ticket named like a ticket's number, claims (ERA_CLAIMS) in one holding an `@all` of his, posts put in
        // (ERA_PUT_IN) in one where a post put into a turn is given now: its line reads those turns before it carried.
        // A record with none of them begins it just before the input that brings one (`ticket_input`)
        let m = &self.tickets.m;
        let era = if m.era < docket_machine::ERA_PUT_IN && m.holds_put_in() {
            docket_machine::ERA_PUT_IN
        } else if m.era < docket_machine::ERA_NAMES && m.has_names_like_tickets() {
            docket_machine::ERA_NAMES
        } else if m.era < docket_machine::ERA_CLAIMS && m.has_all() {
            docket_machine::ERA_CLAIMS
        } else {
            0
        };
        if era > m.era {
            self.begin_era(json!({"era": era}));
        }
        // the quiet era (ERA_QUIET; paper-research N-1446 p2367): from this start delivery hands an agent what is for it
        // and no more; on a record in the era of his posts handed (ERA_HIS_HANDED), as eras add up
        if (docket_machine::ERA_HIS_HANDED..docket_machine::ERA_QUIET).contains(&self.tickets.m.era) {
            self.begin_era(json!({"era": docket_machine::ERA_QUIET}));
        }
        // the way back (proto-lead N-1446 p431, the after-replay step): each of his posts that no line told, whose
        // handler this release did not load (a later release's program it cannot parse), goes to this release's own
        // `owner_post` as if it had just arrived, under the policy's hold (UNTOLD's doc): one line each, after replay
        for (t, n) in self.tickets.m.untold_posts() {
            if let Err(e) = self.ticket_input(Input::Event { name: docket_machine::UNTOLD.into(), value: json!({"t": t, "n": n}) }) {
                eprintln!("coordd: tickets: his post not told: {e}");
            }
        }
        // an agent whose process ended while coordd was down was not cut at its exit: cut now (after `recover`, which
        // finds the processes still running)
        let stopped: Vec<String> = self.agents.iter().filter(|(n, a)| !a.running() && !self.tickets.m.given_since(n).is_empty()).map(|(n, _)| n.clone()).collect();
        for n in stopped {
            self.ticket_lost_with_process(&n);
            self.ticket_turn_cut(&n);
        }
        // a text-change's load that stopped after its commit: its line written now; and the flavour's look read
        self.text_recover_line();
        self.read_look();
    }

    /// The record's era begins (docket_machine::ERA), VALUE its line's: one line, after which its rules hold. Written
    /// only where the record has or is about to have what they change (`load_tickets`, `era_for`), or has had it just
    /// now (`ticket_input`: ERA_HIS_HANDED).
    fn begin_era(&mut self, value: Value) {
        if let Err(e) = self.ticket_input(Input::Event { name: docket_machine::ERA.into(), value }) {
            eprintln!("coordd: tickets: the record's era: {e}");
        }
    }

    /// The era INPUT needs the record in before it is applied (docket_machine::ERA), 0 for none: names (ERA_NAMES) when
    /// it has the name of one of coordd's agents named like a ticket's number (`consult-3`) in it and can put it on a
    /// thread (not an esc, a delivery or a session's line), claims (ERA_CLAIMS)
    /// when it carries an `@all` of his, posts put in (ERA_PUT_IN) when it gives one into a turn. An agent's name in
    /// words a reference holds is not seen: its era begins at the next input or start that has it, and until then the
    /// rules from before hold.
    fn era_for(&self, input: &Input) -> u32 {
        let m = &self.tickets.m;
        // posts put into a turn, or a cut that holds his post (which it leaves due again, paper-research N-412.p2202, 2)
        if m.era < docket_machine::ERA_PUT_IN && (matches!(input, Input::GivenInto { .. }) || matches!(input, Input::TurnCut { agent } if m.cut_holds_his(agent))) {
            return docket_machine::ERA_PUT_IN;
        }
        if m.era >= docket_machine::ERA_NAMES {
            return 0;
        }
        let names: Vec<&String> = self.agents.keys().filter(|a| m.named_like_a_ticket(a)).collect();
        // an input to an agent's session, its deliveries or its asks writes no post or opener with its name: it puts
        // the agent on no thread (`Machine::participants`), so the era of names waits for one that does
        let on_no_thread = matches!(input, Input::Esc { .. } | Input::TurnCut { .. } | Input::Unread { .. } | Input::Given { .. } | Input::GivenInto { .. } | Input::Typed { .. } | Input::Held { .. } | Input::StruckFor { .. } | Input::AskLater { .. });
        if !names.is_empty() && !on_no_thread {
            let seen = format!("{input:?}");
            if names.iter().any(|a| seen.contains(a.as_str())) {
                return docket_machine::ERA_NAMES;
            }
        }
        if m.era < docket_machine::ERA_CLAIMS && m.carries_all(input) {
            return docket_machine::ERA_CLAIMS;
        }
        0
    }

    /// Apply INPUT: its lines appended, its effects carried out; the reply.
    /// The owner's messages held for him to pick their ticket (intake's Unsure on a thread opened before 16 Nov, its
    /// step `owner pick`): given now to whom he wrote them, each as a thread of its own, its step withdrawn for
    /// `return Thread`. Nothing of his is held (the owner, N-354/N-355: "o cannot go rule o nthat he. psot on both for
    /// all i care"; paper-research N-101.p1285); a thread opened since never holds one (thread.kind's Unsure).
    fn release_picks(&mut self) {
        let held: Vec<u64> = self.tickets.m.tickets.values()
            .filter(|x| x.is_open() && x.kind == "thread" && matches!(&x.status, docket_machine::Status::Awaits { agent, owed, .. } if agent == "owner" && owed.call == "pick"))
            .map(|x| x.t)
            .collect();
        for t in held {
            let why = "the owner's message was held for him to pick its ticket; given now to whom he wrote it, as a thread of its own (owner, N-354/N-355)".to_string();
            match self.ticket_input(Input::Withdraw { t, by: "coordd".into(), program: "return Thread".into(), why }) {
                Ok(_) => eprintln!("coordd: tickets: {} held for the owner's pick: given now, as a thread of its own", self.tickets.m.id(t)),
                Err(e) => eprintln!("coordd: tickets: {} held for the owner's pick, not released: {e}", self.tickets.m.id(t)),
            }
        }
    }

    pub fn ticket_input(&mut self, input: Input) -> Result<String, String> {
        // the first input with an agent named like a ticket's number, or his first `@all`: the record's era begins with
        // the line before it (`load_tickets`)
        let era = self.era_for(&input);
        if era > self.tickets.m.era {
            self.begin_era(json!({"era": era}));
        }
        // an answer in `layer on X`: its commits laid over X before the machine takes it (runner.rs, N-159 post 125)
        // an answer in a layer: completed with every repository whose layer branch moved (moves.rs, W1, N-101 p466),
        // before `layer on X` fills the rest from X
        let input = self.completed_in_layer(input);
        let input = self.completed_on_layer(input);
        // an answer naming an agent that is not registered is refused, and formed again (item 11, G5)
        let input = self.unregistered(input);
        // a commit given as a bare hash in a project of several repositories: keyed by the one that has it, else
        // refused (paper-research N-101 p953, 4; runner.rs)
        let input = self.bare_refs(input);
        // an answer whose commit is not on its step's layer branch is refused, and formed again (N-101 p406, moves.rs)
        let input = self.off_layer(input);
        // a decision's line writes each rule that left options out once (`excluded_by_rule`)
        let input = match input {
            Input::Decided { t, step, question, answer, provenance } => Input::Decided { t, step, question, answer, provenance: excluded_by_rule(provenance) },
            input => input,
        };
        // a decision among agents that named one stopped since its facts were taken is asked again, once (hold 1)
        if let Input::Decided { t, step, question, answer: Value::String(a), .. } = &input {
            if question == "decide" {
                if let Some((name, args)) = self.among_again(*t, *step, a) {
                    self.ask_among(*t, *step, name, &args);
                    return Ok(format!("{a} cannot be woken now: the decision is asked again"));
                }
            }
        }
        // the log cannot be written: nothing more is taken, so the log stays what a restart replays (paper-research N-8)
        if let Some(why) = self.log_fault() {
            return Err(why);
        }
        self.tickets.m.readers = Some(self.reply_readers());
        // his typed post is held as the policy says (`lapse-after("gather", N)`, E1b): written on its line; and so are
        // his post a later release left untold, sent on after a replay (UNTOLD; proto-lead N-1446 p819 (b)), and his
        // message's words, a new thread's first post (I-1474)
        // his post to an agent in Step (N-1788) is not gathered: it goes as he wrote it, staged at once, no burst waited for
        if matches!(&input, Input::Post { by, .. } if by == "owner") || matches!(&input, Input::Event { name, .. } if name == docket_machine::UNTOLD || name == "owner_message") {
            self.tickets.m.owner_hold = if self.to_step_agent(&input) { None } else { self.lapse_after(docket_machine::syntax::GATHER) };
        }
        let out = self.tickets.m.apply(input, now());
        // the lines go in before anything is answered or done (paper-research N-8 post 16): one that does not leaves
        // the machine as the log has it, and the input is refused with the notice's words
        let lines: Vec<String> = out.lines.iter().map(|l| serde_json::to_string(l).expect("a line is JSON")).collect();
        if let Err(e) = crate::jsonl::record(&self.tickets.log, &lines) {
            eprintln!("coordd: tickets: {} line(s) not in the log, the input refused: {e}", lines.len());
            self.tickets.m = self.tickets.replayed();
            return Err(self.log_fault().unwrap_or(e));
        }
        out.lines.iter().for_each(|l| self.tickets.quiet.take(l));
        if self.tickets.posted.is_none() {
            let m = &self.tickets.m;
            let n_of = |t: u64, by: &str, first: bool| -> Option<(String, u32)> {
                let x = m.tickets.get(&t)?;
                let mut by_them = x.posts.iter().filter(|p| p.by == by);
                let p = if first { by_them.next() } else { by_them.last() }?;
                Some((m.id(t), p.n))
            };
            self.tickets.posted = out.lines.iter().find_map(|l| match l {
                Line::Post { t, by, .. } => n_of(*t, by, false),
                Line::Open { t, opener, text: Some(_), .. } => n_of(*t, opener, true),
                _ => None,
            });
        }
        out.lines.iter().for_each(|l| self.tickets.graph_git.take(l));
        // and done as the acts they are, for the facts acts make and end (effects.rs); facts that moved are a new
        // situation for the norms (`norm_gen`), as an agent's change is
        let (places, before) = (self.places(), self.tickets.effects.held.clone());
        if let Some(why) = self.tickets.effects.take(&places, &self.tickets.log, &lines) {
            eprintln!("coordd: tickets: {why}");
        }
        if self.tickets.effects.held != before {
            self.norm_gen += 1;
        }
        // and the places they give a text at, from now on read by `Request::Text` (p1419, 2)
        {
            let mut places = self.tickets.places.write().unwrap_or_else(|e| e.into_inner());
            out.lines.iter().for_each(|l| places.take(l));
        }
        // the graph takes them as the start's replay took each input: at their place in the record, the machine after them
        let mut asks = Vec::new();
        if !out.lines.is_empty() {
            let Tickets { m, graph, graph_lines, errors, log, side, .. } = &mut self.tickets;
            // what the start's replay had handed in is not raised: only what these lines do
            graph.first_handed.clear();
            graph.after(m, *graph_lines, &out.lines);
            asks = std::mem::take(&mut graph.first_handed);
            side.take(&out.lines, *graph_lines, &|t, n| docket_machine::graph::post_id(m, t, n));
            *graph_lines += out.lines.len();
            // where it and the record disagree, among the errors as the start's are (p1378)
            for p in std::mem::take(&mut graph.problems) {
                eprintln!("coordd: tickets: {log}: {p}");
                errors.push(format!("{log}: {p}"));
            }
        }
        // the joint answers the same lines on its own thread (N-568 part 5): what the machine shows of them goes with them
        if let Some((tx, at, watch)) = &mut self.tickets.joint {
            if !watch.still_on() {
                eprintln!("coordd: joint: run-acts no longer says both: the joint's thread stops (on again only at coordd's start)");
                self.tickets.joint = None;
            } else if !out.lines.is_empty() {
                let seen = crate::joint::seen(&self.tickets.m, &out.lines);
                *at += out.lines.len();
                if tx.send((*at - out.lines.len() + 1, out.lines.clone(), seen)).is_err() {
                    self.tickets.joint = None;
                }
            }
        }
        // a say of his cleaned words: the thread its event opened is looked at from now on (`cleaned_pending_round`)
        for l in &out.lines {
            if let Line::Event { name, value, fired, .. } = l {
                if let (true, Some(w)) = (name == "owner_message", value["cleaned"].as_str().and_then(docket_machine::parse_ticket)) {
                    self.cleaned_pending.extend(fired.iter().filter(|f| f.kind == "thread").map(|f| (f.t, w)));
                }
                if let (true, Some(from)) = (name == "owner_message", value.get("split_from").and_then(|v| serde_json::from_value::<docket_machine::SplitFrom>(v.clone()).ok())) {
                    self.split_pending.extend(fired.iter().filter(|f| f.kind == "thread").map(|f| (f.t, from.clone())));
                }
            }
        }
        // an agent's post read his post not handed to it, in a record before the era ERA_HIS_HANDED, from which it would
        // not have (consult-3 N-1479 p151): nothing said before it that it comes, so the era begins on the line after
        // it, naming those posts, which it tells to their agents late (`Machine::passed_his`); one a hand-over on its way
        // carries is given by that hand-over's `given`, or, not taken, as told late (`ToldLate`; consult-3 p212)
        if self.tickets.m.era < docket_machine::ERA_HIS_HANDED && !self.tickets.m.passed_his.is_empty() {
            let passed = std::mem::take(&mut self.tickets.m.passed_his);
            self.begin_era(json!({"era": docket_machine::ERA_HIS_HANDED, "passed": passed}));
        }
        let changed: Vec<u64> = if self.cleaned_pending.is_empty() && self.split_pending.is_empty() { Vec::new() } else { out.effects.iter().filter_map(|e| match e { Effect::Changed { t } => Some(*t), _ => None }).collect() };
        self.ticket_effects(out.effects);
        self.asks_raised(asks);
        self.end_asks();
        self.cleaned_pending_round(&changed);
        self.split_pending_round(&changed);
        out.reply
    }

    /// His posts the first head was handed in for at the input just taken (`graph::Builder::first_handed`): an `asked`
    /// line for each while a binding is on it, sent as coordd sends its own events (N-1511 p47; paper-research N-1446
    /// p2204, p2354: his goal opens at its first head, not at his post). The machine writes its value
    /// (`Machine::asked_value`) and refuses a second for a post.
    fn asks_raised(&self, asks: Vec<(u64, u32)>) {
        for (t, n) in asks {
            let post = docket_machine::graph::post_id(&self.tickets.m, t, n);
            self.event_sent(docket_machine::ASKED, &json!({"post": post}));
        }
    }

    /// The `violated` and `duties` asks running whose ticket no longer asks them (closed, its answer gone another way,
    /// a goal's step no longer awaiting its duties): stopped, so that none runs on for a ticket that will not take its
    /// answer.
    fn end_asks(&mut self) {
        let Tickets { m, asks, .. } = &mut self.tickets;
        asks.retain(|(t, step, question), stop| {
            let stands = if *question == "duties" { m.asks_duties(*t, *step) } else { m.asks_violated(*t, *step) };
            if !stands {
                stop.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            stands
        });
    }

    pub fn ticket_effects(&mut self, effects: Vec<Effect>) {
        // the agents whose delivery acted: each one's view emitted once, after the rest (it carries its last acts)
        let mut acted: Vec<String> = Vec::new();
        for e in effects {
            match e {
                Effect::Deliver { to } => {
                    // a step whose call's `where` its agent's model does not meet is refused, not given (models.rs)
                    self.refuse_unmet(&to);
                    self.poke.notify_one()
                }
                Effect::Ask { t, step, question: Question::Fills { role, call } } => {
                    let ticket = self.tickets.m.id(t);
                    // who fills a step may turn on what it is given, as a decision on its arguments does
                    // (paper-research N-1446 p2894: who reads a text change is `fills` given its writer and its paths)
                    let asked = self.asked_args(t, step);
                    let mut facts = self.fills_facts(t);
                    facts.extend(among_given(&asked));
                    // and a commit it is given, what a decision on that commit is handed (`decision_facts`' (1)): who
                    // merges a head may turn on what its commit changes (paper-research N-1446 p2954)
                    facts.extend(self.commits_facts(&self.ref_args(&call, &|p| asked.get(p).and_then(Value::as_str).map(str::to_string))));
                    let args = step_fields(&asked);
                    let places = self.places();
                    self.ticket_task(Task::Fills { t, step, ticket, role, call, args, facts, places });
                }
                Effect::Ask { t, step, question: Question::Holds { goals } } => {
                    let ticket = self.tickets.m.id(t);
                    let actor = self.tickets.m.tickets.get(&t).and_then(|x| x.opener.clone()).unwrap_or_else(|| "owner".into());
                    let mut facts = self.goal_facts(t);
                    // the actor is whoever opened it, the owner too, who is no agent of the team's: a goal is the
                    // ticket's, not a permission, so the act is asked for the opener as it is (live round, G-10)
                    if !facts.iter().any(|(f, a)| f == "agent" && a.first() == Some(&actor)) {
                        facts.push(("agent".into(), vec![actor.clone()]));
                    }
                    let places = self.places();
                    self.ticket_task(Task::Holds { t, step, ticket, actor, goals, facts, places });
                }
                Effect::Ask { t, step, question: Question::Decide { name, args } } if self.tickets.m.defs.calls.get(&format!("{}{name}", docket_machine::syntax::DECIDE)).is_some_and(|s| s.head == Type::Agent) => {
                    self.ask_among(t, step, name, &args);
                }
                Effect::Ask { t, step, question: Question::Decide { name, args } } => {
                    let ticket = self.tickets.m.id(t);
                    let facts = self.tickets.m.decide_facts(t, &name, &args, &crate::core::now());
                    let why = self.tickets.m.defs.calls.get(&format!("{}{name}", docket_machine::syntax::DECIDE)).is_some_and(|s| {
                        matches!(&s.head, docket_machine::Type::Variant(cs) if cs.iter().any(|(_, fs)| fs.iter().any(|(f, _)| f == "why")))
                            && !s.params.iter().any(|(p, _)| p == "why")
                    });
                    let situation = self.decision_facts(&name, &|p| args.get(p).and_then(Value::as_str).map(str::to_string));
                    self.ticket_task(Task::Decide { t, step, ticket, name, args, facts, why, situation });
                }
                Effect::Ask { t, step, question: Question::Violated { agent, call, answered } } => {
                    let head = answered.get("head").and_then(Value::as_str).unwrap_or_default().to_string();
                    let tree = self.ticket_tree(t);
                    let log = self.tickets.log.clone();
                    let policy = self.tickets.duties.clone();
                    // an edit counts only in the project's code: its repositories, where a layer's view stands too
                    let repos = self.project.as_ref().map(|p| {
                        let mut v: Vec<String> = p.repos().into_iter().map(|(_, r)| r).collect();
                        v.extend(p.repo().filter(|r| !v.contains(r)));
                        v
                    }).unwrap_or_default();
                    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
                    self.tickets.asks.insert((t, step, "violated"), stop.clone());
                    self.ticket_task(Task::Violated { t, step, answering: violated::Answering { agent, call, head }, policy, log, tree, repos, stop });
                }
                Effect::Ask { t, step, question: Question::Duties { goal, lines } } => {
                    // the project's duties file, whether or not answers' duties are asked of it (`violated::declared`)
                    let policy = self.places().project.map(|p| format!("{p}/policy/{}", violated::FILE)).filter(|p| std::path::Path::new(p).is_file());
                    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
                    self.tickets.asks.insert((t, step, "duties"), stop.clone());
                    self.ticket_task(Task::Duties { t, step, goal, lines, policy, stop });
                }
                Effect::Form { t, step, agent, head, text, words, call } => {
                    // the step as the owner was shown it, for the owner's words; no ticket id is in it (m1308)
                    let asked = words.is_some().then(|| {
                        let args = match self.tickets.m.view(t).map(|v| v.status) {
                            Some(docket_machine::ViewStatus::Awaits { args, .. }) => args,
                            _ => Value::Null,
                        };
                        (call.clone(), args, self.places())
                    });
                    // the step an agent's answer answers: its first delivery's place in the agent's session (not a
                    // reminder's or a refusal's), or the last time it was handed again in its layer after a move there
                    // (`sN.back`, paper-research N-101 p629), and its call and arguments should that not read
                    let shown = words.is_none().then(|| {
                        let m = &self.tickets.m;
                        let at = shown_place(m, t, step, &agent);
                        let args = match m.view(t).map(|v| v.status) {
                            Some(docket_machine::ViewStatus::Awaits { args, .. }) => args,
                            _ => Value::Null,
                        };
                        (at, call, args, self.places())
                    });
                    let coord = self.coord.clone();
                    self.ticket_task(Task::Form { t, step, agent, head, text, words, asked, shown, coord })
                }
                Effect::Notify { t, event, actor, post } => {
                    let ticket = self.tickets.m.id(t);
                    let mut facts = self.ticket_facts(Some(t)).0;
                    // the owner archived it: who holds work at or under it, told once (notify.eflint)
                    if event == "archived" {
                        facts.extend(self.tickets.m.works_under(t).into_iter().map(|a| ("works-under".to_string(), vec![a, ticket.clone()])));
                    }
                    // how it ended, for a close: `closed-failed` when it closed with a raise (paper-research N-101 p939)
                    if event == "closed" && matches!(self.tickets.m.tickets.get(&t).map(|x| &x.status), Some(docket_machine::Status::Failed(_))) {
                        facts.push(("closed-failed".into(), vec![ticket.clone()]));
                    }
                    let places = self.places();
                    self.ticket_task(Task::Notify { t, event, actor, post, ticket, facts, places });
                }
                Effect::Changed { t } => {
                    self.request_closed(t);
                    let Tickets { m, holds, .. } = &mut self.tickets;
                    holds.see(m, t);
                    // the posts it gained: what they name is shown at once, on its view and theirs (threads piece 3)
                    let upto = self.tickets.mentions.read_upto(t);
                    let named = crate::counts::timed_sync("mentions (a landing post's ids)", || {
                        let Tickets { m, mentions, .. } = &mut self.tickets;
                        mentions.read_posts(m, &crate::label::read, t)
                    });
                    // their `named` edges into the kept graph, as the start's `named_into` gives them (p1383)
                    let mut g = std::mem::take(&mut self.tickets.graph.graph);
                    self.tickets.mentions.named_of_into(&self.tickets.m, &|a| self.is_agent(a), &mut g, t, upto);
                    self.tickets.graph.graph = g;
                    if let Some(body) = crate::counts::timed_sync("emit ticket (view and its posts' hashes)", || self.ticket_shown(t).map(|v| self.ticket_body(v))) {
                        self.emit(body);
                    }
                    let named: Vec<u64> = named.iter().filter_map(|id| self.tickets.m.ticket_named(id).filter(|u| *u != t && *u < docket_machine::HIDDEN)).collect();
                    // each named ticket gets only its new lines back, not its view rebuilt (paper-research N-427 p11)
                    for u in named {
                        let id = self.tickets.m.id(u);
                        let lines = crate::counts::timed_sync("mentions (a named ticket's new lines back)", || {
                            let agent = |n: &str| self.is_agent(n);
                            self.tickets.mentions.lines_new(&self.tickets.m, &agent, &|k| docket_machine::label::ticket(&self.tickets.m, k, &crate::label::read), &id, t, upto)
                        });
                        if !lines.is_empty() {
                            self.emit(Body::Mentioned { id, lines });
                        }
                    }
                    // what agents wait on is derived from the tickets (plan/edge.md S4)
                    if let Some(jobs) = &self.jobs {
                        let _ = jobs.send(crate::core::Job::Waits);
                    }
                }
                Effect::Start { agent, t, call, layer } => match (self.tickets.m.view(t), self.jobs.clone(), self.started_owing.insert(agent.clone(), (std::time::Instant::now(), None))) {
                    (Some(view), Some(jobs), _) => {
                        let _ = jobs.send(crate::core::Job::Start { agent, call, view, layer, from: None, cut: None });
                    }
                    _ => eprintln!("coordd: tickets: cannot start {agent} for ticket {t}"),
                },
                // a fork (N-101): started as any scoped agent is, from a copy of FROM's conversation (fork.rs)
                Effect::Fork { agent, t, call, layer, from } => match (self.tickets.m.view(t), self.jobs.clone()) {
                    (Some(view), Some(jobs)) => {
                        let _ = jobs.send(crate::core::Job::Start { agent, call, view, layer, from: Some(from), cut: None });
                    }
                    _ => eprintln!("coordd: tickets: cannot start {agent}, a fork of {from}, for ticket {t}"),
                },
                // a copy (N-1104 p1154): a fork of FROM, its session cut where PLACE was given, in FROM's own folder
                Effect::Copy { agent, t, call, from, place } => match (self.tickets.m.view(t), self.jobs.clone()) {
                    (Some(view), Some(jobs)) => {
                        let _ = jobs.send(crate::core::Job::Start { agent, call, view, layer: None, from: Some(from), cut: Some(place) });
                    }
                    _ => eprintln!("coordd: tickets: cannot start {agent}, a copy of {from}, for ticket {t}"),
                },
                Effect::Answered { agent, text } => {
                    self.tickets.answered.insert(agent, text);
                    self.tickets.answered_tx.send_modify(|n| *n += 1);
                }
                // the ask was about a step since withdrawn: its answer is the withdrawal, in its own words
                Effect::AnsweredWith { agent, d } => {
                    let text = match self.tickets.m.view(d.t).map(|v| crate::render::delivery(&self.places(), &d, &v, true, true, false, false, None)) {
                        Some(Ok((text, _))) => text,
                        _ => format!("Step {} of {} was withdrawn.", match &d.what { docket_machine::What::Withdrawn { step, .. } => *step, _ => 0 }, d.ticket),
                    };
                    self.tickets.answered.insert(agent, text);
                    self.tickets.answered_tx.send_modify(|n| *n += 1);
                }
                // the delivery program's one message (0.2 A1, docket-machine delivery.rs): coordd performs it once it
                // raises the delivery events (head 2); until then nothing raises them, and this is never made
                // the delivery program's message: handed by the next round's `hand_over` (delivery.rs `hand_now`), which
                // reads what the machine holds as being handed; after a start, those no `given` line covers again
                Effect::Hand { .. } => self.poke.notify_one(),
                Effect::Interrupt { agent, .. } => {
                    // esc stops a running turn, as the old esc mail did; an agent out of a turn just gets the words
                    // (docket-edge m1137: an Escape sent to an idle agent killed the turn the words began)
                    if self.agents.get(&agent).is_some_and(|a| a.live.state == "working" && a.ended_turn != a.live.turn) {
                        if self.tickets.stopping.insert(agent.clone()) {
                            self.interrupt(&agent);
                        }
                    } else {
                        // one held on a rate limit: the turn it waits for is not given (proto-lead N-1397 p189)
                        self.limit_dropped(&agent, "an esc or the program's stop on the record");
                    }
                }
                Effect::End { agent } => {
                    // a scoped agent is named fT-N for its ticket T
                    let t = agent.strip_prefix('f').and_then(|r| r.split_once('-')).and_then(|(t, _)| t.parse::<u64>().ok());
                    let view = t.and_then(|t| self.tickets.m.view(t));
                    if let Some(jobs) = self.jobs.clone() {
                        let _ = jobs.send(crate::core::Job::End { agent, view });
                    }
                }
                // the machine's own event (the brain's owner_to, 0.3): sent as coordd sends its own, a recorded line
                Effect::Event { name, value } => self.ticket_event(&name, &value),
                Effect::Acted { agent } => {
                    if !acted.contains(&agent) {
                        acted.push(agent);
                    }
                }
            }
        }
        acted.retain(|a| self.agents.contains_key(a));
        for agent in &acted {
            self.changed(agent);
        }
        self.layers_reconcile();
        self.layers_tidy();
    }

    /// The layers active in the machine against those coordd has asked for: each newly active one made (its view),
    /// each no longer active ended. The first time, the registry's open layers count as asked, so one whose scope
    /// ended while coordd was down is ended now. Whatever ends a scope (its end, a raise, a cancel, an undo, a repair)
    /// ends its layer, since only the frames say what is active (`Machine::active_layers`).
    fn layers_reconcile(&mut self) {
        let Some(jobs) = self.jobs.clone() else { return };
        let coord = self.places().coord.clone();
        let asked = self.tickets.layers.get_or_insert_with(|| crate::layer::load(&coord).into_values().filter(|l| l.state == "open").map(|l| l.id).collect());
        let active: BTreeSet<String> = self.tickets.m.active_layers().into_iter().map(|(id, _, _)| id).collect();
        let new: Vec<String> = active.difference(asked).cloned().collect();
        let gone: Vec<String> = asked.difference(&active).cloned().collect();
        for id in &gone {
            asked.remove(id);
        }
        for id in gone {
            let _ = jobs.send(crate::core::Job::Layer { id, spec: None });
        }
        // made off the lock; one that cannot be made (no repo, the kernel's refusal) raises NoLayer there
        for id in new {
            let spec = self.layer_spec(&id);
            self.tickets.layers.get_or_insert_with(BTreeSet::new).insert(id.clone());
            let _ = jobs.send(crate::core::Job::Layer { id, spec: Some(spec) });
        }
    }

    /// The layers to remove (plan/layers.md, removal; N-95.p5): each one whose ticket has closed and that is not
    /// removed yet, and each kept branch whose top ticket has closed, asked of `layer::remove` once each since coordd
    /// started. So at its start every layer whose ticket closed while it was down is removed, and what failed before
    /// is tried again; once more each time a new removal is asked.
    fn layers_tidy(&mut self) {
        let Some(jobs) = self.jobs.clone() else { return };
        let reg = crate::layer::load(&self.places().coord);
        if reg.is_empty() {
            return;
        }
        let closed = |t: u64| self.tickets.m.tickets.get(&t).is_some_and(|x| !x.is_open());
        let mut due = Vec::new();
        let mut retry = Vec::new();
        for l in reg.values() {
            // the layer's ticket in this coord's machine, closed (an id from another life of the tickets: left)
            let Some(t) = self.tickets.m.ticket_named(&l.ticket) else { continue };
            if !closed(t) {
                continue;
            }
            let top = self.top_of(t);
            let top_closed = closed(top);
            // still to do: its folder, a kept branch whose top has closed, or what failed before
            if !(l.removed.is_none() || (l.kept && top_closed) || !l.left.is_empty()) {
                continue;
            }
            let k = (l.id.clone(), top_closed);
            if !self.tickets.tidied.contains(&k) {
                due.push(k);
            } else if !l.left.is_empty() {
                retry.push(k);
            }
        }
        if due.is_empty() {
            return;
        }
        for k in &due {
            self.tickets.tidied.insert(k.clone());
        }
        let _ = jobs.send(crate::core::Job::LayersRemove { ids: due.into_iter().chain(retry).collect() });
    }

    /// The top ticket of T's tree: up its parents (a ticket its pattern opened) and the ticket it was opened under.
    fn top_of(&self, t: u64) -> u64 {
        let mut t = t;
        let mut seen = BTreeSet::new();
        while seen.insert(t) {
            match self.tickets.m.tickets.get(&t).and_then(|x| x.parent.or(x.under)) {
                Some(p) => t = p,
                None => break,
            }
        }
        t
    }

    /// What active layer ID is made from: its ticket, what it is on, and from the project file the project's folder
    /// (`repo`), the root branch (`root`) and the build output folders (`outputs`). A layer never guesses: without
    /// `repo`, or without `root` when it has no `on`, it cannot be made (N-89.p4).
    pub fn layer_spec(&self, id: &str) -> Result<crate::layer::Spec, String> {
        let (_, t, on) = self.tickets.m.active_layers().into_iter().find(|(l, _, _)| l == id).ok_or_else(|| format!("layer {id} is not active"))?;
        let ticket = self.tickets.m.id(t);
        let p = self.project.as_ref().ok_or("the project has no project file")?;
        let repo = p.repo().ok_or("the project file sets no repo, the project's folder")?;
        if on.is_none() && p.project.root.is_none() {
            return Err(String::from("the project file sets no root branch, so a layer without `on` has no commit to start from"));
        }
        let home = std::env::var("HOME").unwrap_or_default();
        let codex_home = std::env::var("CODEX_HOME").unwrap_or_else(|_| format!("{}/codex-home", self.places().coord));
        let (name, also) = p.layer_repos();
        Ok(crate::layer::Spec { id: id.to_string(), ticket, on, repo, root: p.project.root.clone(), outputs: p.project.outputs.clone(), home, codex_home, boxed: p.project.boxed, also, name, warm: p.warm() })
    }

    /// What the line's read-only view is made from (`layer::line_view`): the project's folder at its root branch's
    /// head, the rest as `layer_spec` has it (N-101 p406, B). Why not, where the project file sets no repo or no root.
    pub fn line_spec(&self) -> Result<crate::layer::Spec, String> {
        let p = self.project.as_ref().ok_or("the project has no project file")?;
        let repo = p.repo().ok_or("the project file sets no repo, the project's folder")?;
        let root = p.project.root.clone().ok_or("the project file sets no root branch, so there is no line to view")?;
        let home = std::env::var("HOME").unwrap_or_default();
        let codex_home = std::env::var("CODEX_HOME").unwrap_or_else(|_| format!("{}/codex-home", self.places().coord));
        let (name, also) = p.layer_repos();
        Ok(crate::layer::Spec { id: "line".into(), ticket: String::new(), on: None, repo, root: Some(root), outputs: p.project.outputs.clone(), home, codex_home, boxed: p.project.boxed, also, name, warm: p.warm() })
    }

    /// A request T that has just closed Granted(n): WHO's team grant is N, by whoever answered (once).
    fn request_closed(&mut self, t: u64) {
        let Some(x) = self.tickets.m.tickets.get(&t).filter(|x| x.kind == "request" && !x.is_open()) else { return };
        if !self.tickets.granted.insert(t) {
            return;
        }
        let docket_machine::Status::Done(v) = &x.status else { return };
        if v["head"] != "Granted" {
            return;
        }
        let (who, n) = (x.form["who"].as_str().unwrap_or_default().to_string(), v["n"].as_u64().unwrap_or(0) as u32);
        let by = x.posts.last().map(|p| p.by.clone()).unwrap_or_else(|| "owner".into());
        if let Err(e) = self.apply_grant(&who, n, &by) {
            eprintln!("coordd: tickets: {}: {e}", self.tickets.m.id(t));
        }
    }

    /// `coord team ask TEXT [N]`: agent WHO asks to form a team (as `ask-team` allows), by a request ticket that waits
    /// on whom `approve-team` lets approve it (its lead first), else the owner.
    pub fn ticket_team_ask(&mut self, who: &Who, text: &str, n: Option<u32>) -> Reply {
        let Who::Agent(me) = who else { return err("coord team ask: an agent asks; the owner grants with `coord team grant`".into()) };
        if text.trim().is_empty() {
            return err("usage: coord team ask TEXT  (why a team, what it does, how many agents)".into());
        }
        if let Err(why) = self.permitted("ask-team", &[me], "none") {
            return err(format!("coord team ask: {me} may not ask for a team ({why})"));
        }
        let may = |a: &str| matches!(self.norm_verdict("approve-team", &[a, me.as_str()], "none"), Ok(Some(v)) if v.enabled);
        let parent = self.agents.get(me).map(|a| a.parent.clone()).filter(|p| self.agents.contains_key(p) && may(p));
        let approver = parent.or_else(|| self.agents.keys().find(|a| *a != me && may(a)).cloned()).unwrap_or_else(|| "owner".into());
        let mut form = json!({"who": me, "approver": approver, "why": text});
        if let Some(n) = n {
            form["n"] = json!(n);
        }
        let input = Input::Open { kind: "request".into(), under: None, opener: me.clone(), form, text: None, names: Vec::new(), told: Vec::new() };
        match self.ticket_input(input) {
            Ok(r) => ok(format!("{r} It waits on {approver}.")),
            Err(e) => err(e),
        }
    }

    /// Event NAME with VALUE: when an `on` binding is on it, it is fed to the machine, which opens what the bindings
    /// name (Trigger). It goes through the task runner, so a caller need not hold the core mutably. Not every event
    /// of the nets comes here: the owner's mail is not filed, only what the owner types in the general composer,
    /// which raises `owner_message` here and is not mailed (paper-research m716).
    pub fn ticket_event(&self, name: &str, value: &Value) {
        // the machine's own events are written by coordd alone (the owner's hold, line 327; the install's hand-over; the
        // release his window runs, I-1457; his writing in a ticket's box, 0.3; his posts sent on after a replay; his
        // switch of an agent's brain, N-1446 p857.2; his sign-off of an ask, p1786; a train's move, N-1446 p2139; a
        // turn begun, N-1104 p1154): an event of the runner's or a net's by that name is not one of them
        if [docket_machine::MAIL_HOLD, docket_machine::HELD_HANDED, docket_machine::WINDOW_RELEASE, docket_machine::syntax::WRITING, docket_machine::UNTOLD, docket_machine::BRAIN_SWITCH, docket_machine::SIGNED_OFF, docket_machine::MERGED, docket_machine::ASKED, docket_machine::TURN_BEGAN].contains(&name) {
            return;
        }
        self.event_sent(name, value);
    }

    /// NAME with VALUE sent to the machine as an `event` line, after the input being taken: `ticket_event`'s, and his
    /// ask taken at its first head, which coordd raises itself (`asks_raised`).
    fn event_sent(&self, name: &str, value: &Value) {
        let mut value = value.clone();
        // an `act` (N-101): something an agent did while it holds a step, `{by, is, text}`. The runner says which runs
        // are checks (it runs them: `is: "check"`), naming the agent; the ticket is the one the agent owes a step on
        if name == "act" && value.get("ticket").is_none() {
            if let (Some(id), Value::Object(o)) = (value.get("by").and_then(Value::as_str).and_then(|by| self.tickets.m.owing(by)), &mut value) {
                o.insert("ticket".into(), Value::String(id));
            }
        }
        // a binding is on it, or a ticket handles it (a kind's `when`, a `during`); an act and the runner's events the
        // record keeps are recorded either way (docket-machine runner.rs)
        if name != "act" && !docket_machine::runner::recorded(name) && !self.tickets.m.bound(name) && !self.tickets.m.takes_event(name, &value) {
            return;
        }
        if let Some(tx) = &self.tickets.tasks {
            let _ = tx.send(Task::Event { name: name.into(), value });
        }
    }

    /// The tickets of T's tree: its top ticket (by parent, or the ticket an ask is under) and all below it.
    fn ticket_tree(&self, t: u64) -> BTreeSet<u64> {
        let m = &self.tickets.m;
        let top = |mut u: u64| {
            while let Some(p) = m.tickets.get(&u).and_then(|x| x.parent.or(x.under)) {
                u = p;
            }
            u
        };
        let root = top(t);
        m.tickets.keys().copied().filter(|u| top(*u) == root).collect()
    }

    fn ticket_task(&mut self, task: Task) {
        match &self.tickets.tasks {
            Some(tx) => {
                let _ = tx.send(task);
            }
            None => eprintln!("coordd: tickets: no task runner for {task:?}"),
        }
    }

    /// The situation `fills` is asked in for ticket T: the team (as the norms have it), who is running, and the ticket.
    fn fills_facts(&self, t: u64) -> Vec<(String, Vec<String>)> {
        // the scoped agents that ended stay in the registry (stopped); they are no option, so they are left out
        let m = &self.tickets.m;
        let live: BTreeSet<&String> = m.tickets.values().flat_map(|x| &x.live).collect();
        let mut ended: BTreeSet<String> = m.tickets.iter().flat_map(|(t, x)| (1..=x.scoped).map(move |k| format!("f{t}-{k}"))).filter(|n| !live.contains(n)).collect();
        // and those the owner ended (`coord ended`)
        ended.extend(self.agents.values().filter(|a| a.ended.is_some()).map(|a| a.name.clone()));
        let mut f = self.norm_facts(&[]);
        f.retain(|(_, args)| !args.iter().any(|a| ended.contains(a)));
        f.extend(self.agents.values().filter(|a| a.running()).map(|a| ("running".to_string(), vec![a.name.clone()])));
        if let Some(x) = self.tickets.m.tickets.get(&t) {
            let id = self.tickets.m.id(t);
            f.push(("ticket".into(), vec![id.clone()]));
            f.push(("ticket-kind".into(), vec![id.clone(), x.kind.clone()]));
            if self.tickets.m.archived_by(t).is_some() {
                f.push(("archived".into(), vec![id.clone()]));
            }
            if let Some(o) = &x.opener {
                f.push(("opener".into(), vec![id, o.clone()]));
            }
        }
        f
    }

    /// The arguments of the step T asks who fills at STEP (`Status::Asking`, set as the machine asks it); null when T
    /// asks nothing there.
    fn asked_args(&self, t: u64, step: u32) -> Value {
        match self.tickets.m.tickets.get(&t).map(|x| &x.status) {
            Some(docket_machine::Status::Asking { owed, .. }) if owed.step == step => owed.args.clone(),
            _ => Value::Null,
        }
    }

    /// Ask the policy's decision NAME among agents for T's step STEP with ARGS (`Task::Among`), its facts taken now.
    fn ask_among(&mut self, t: u64, step: u32, name: String, args: &Value) {
        let text = |v: &Value| v.as_str().map_or_else(|| v.to_string(), str::to_string);
        let mut fields: BTreeMap<String, String> = args.as_object().map(|o| o.iter().map(|(k, v)| (k.clone(), text(v))).collect()).unwrap_or_default();
        fields.insert("ticket".into(), self.tickets.m.id(t));
        let facts = self.among_facts(t, args);
        let places = self.places();
        self.ticket_task(Task::Among { t, step, name, fields, facts, places });
    }

    /// Whether agent NAME takes a hand-over now (the policy's `wakeable`): running, and not ended.
    pub(crate) fn wakeable(&self, name: &str) -> bool {
        self.agents.get(name).is_some_and(|a| a.running() && a.ended.is_none())
    }

    /// The decision among agents T's step STEP asks, when its answer A names an agent that can no longer be woken
    /// (stopped between the facts and the answer; consult-lead N-807 post 90, hold 1): asked again once, with the facts
    /// as they are now, so his `@all` is not left with an agent that cannot take it. None: the answer stands.
    fn among_again(&mut self, t: u64, step: u32, a: &str) -> Option<(String, Value)> {
        if self.wakeable(a) || !self.among_asked_again.insert((t, step)) {
            return None;
        }
        let x = self.tickets.m.tickets.get(&t)?;
        let docket_machine::Status::Asking { owed, .. } = &x.status else { return None };
        let name = owed.call.strip_prefix(docket_machine::syntax::DECIDE)?;
        (owed.step == step && owed.head == Type::Agent).then(|| (name.to_string(), owed.args.clone()))
    }

    /// The situation a decision among agents is asked in for ticket T (`Task::Among`): `fills`'s, with among(A, I) for
    /// the agents its ARGS give it, in their order from 0, in-turn(A) for each agent in a turn now, and about(A) for each
    /// agent whose work the words in its ARGS (`text`) are about (`Machine::about`; consult-lead N-1158 post 1).
    fn among_facts(&self, t: u64, args: &Value) -> Vec<(String, Vec<String>)> {
        let mut f = self.fills_facts(t);
        f.extend(among_given(args));
        f.extend(self.agents.values().filter(|a| self.in_turn(a)).map(|a| ("in-turn".to_string(), vec![a.name.clone()])));
        f.extend(self.agents.values().filter(|a| self.wakeable(&a.name)).map(|a| ("wakeable".to_string(), vec![a.name.clone()])));
        let words = match args.get("text") {
            Some(Value::String(w)) => Some(w.clone()),
            Some(v) => match serde_json::from_value::<Text>(v.clone()).ok().and_then(|x| self.tickets.m.text(&x)) {
                Some(Text::New(w)) => Some(w),
                _ => None,
            },
            None => None,
        };
        if let Some(words) = words {
            f.extend(self.tickets.m.about(&words).into_iter().map(|a| ("about".to_string(), vec![a])));
        }
        f
    }

    /// The `may` question a request needs, with its facts (under the lock); None when none is needed: the owner is not
    /// bound by the norms, and only `Open` and `Post` are asked.
    pub fn ticket_may(&self, who: &Who, req: &coord_types::Request) -> Option<MayAsk> {
        let (name, bound) = match who {
            Who::Agent(name) => (name.clone(), true),
            _ => ("owner".to_string(), false),
        };
        let (act, ticket, kind) = match req {
            coord_types::Request::Open { kind, under, .. } => ("open", under.as_deref().and_then(|u| ticket_of(&self.tickets.m, u).ok()), kind.clone()),
            coord_types::Request::Post { t, .. } => {
                let t = ticket_of(&self.tickets.m, t).ok()?;
                ("post", Some(t), self.tickets.m.tickets[&t].kind.clone())
            }
            // `coord msg`: a post on the ticket it replies to, else a thread opened (design part 9b)
            coord_types::Request::Msg { re, .. } => match re.as_deref().and_then(|r| reply_of(&self.tickets.m, r)) {
                Some((t, _)) => ("post", Some(t), self.tickets.m.tickets[&t].kind.clone()),
                None => ("open", None, "thread".to_string()),
            },
            coord_types::Request::Ask { .. } => ("open", None, "question".to_string()),
            _ => return None,
        };
        let (facts, id) = self.ticket_facts(ticket);
        Some(MayAsk { places: self.places(), facts, act, who: name, ticket: id, kind, bound })
    }

    /// The `may edit` question for WHO on ticket T (`coord cancel`, `coord withdraw`): None for the owner, who is not
    /// bound by the norms, or when there is no such ticket (the act itself then says so).
    pub fn ticket_may_edit(&self, who: &Who, t: &str) -> Option<MayAsk> {
        let Who::Agent(name) = who else { return None };
        let t = ticket_of(&self.tickets.m, t).ok()?;
        let kind = self.tickets.m.tickets[&t].kind.clone();
        let (facts, id) = self.ticket_facts(Some(t));
        Some(MayAsk { places: self.places(), facts, act: "edit", who: name.clone(), ticket: id, kind, bound: true })
    }

    /// For an answer to T by WHO: the `may edit` question when T's step is a hold WHO does not hold (anyone policy lets
    /// may answer a hold, as `coord item close` is); None otherwise.
    pub fn ticket_may_answer(&self, who: &Who, t: &str) -> Option<MayAsk> {
        let n = ticket_of(&self.tickets.m, t).ok()?;
        match &self.tickets.m.tickets[&n].status {
            docket_machine::Status::Awaits { agent, owed, .. } if owed.hold && *agent != who.name() => self.ticket_may_edit(who, t),
            _ => None,
        }
    }

    /// The facts a goal of T is asked with: T's situation, and every ticket below T (children, and those opened under
    /// it, all the way down) that has closed, as `closed-under(T, KIND, HEAD)` (HEAD: the result's constructor, or
    /// the exception it raised). So "a check under it was green" is `closed-under(ticket, "check", "Green")`.
    fn goal_facts(&self, t: u64) -> Vec<(String, Vec<String>)> {
        let (mut facts, id) = self.ticket_facts(Some(t));
        let id = id.unwrap_or_default();
        let m = &self.tickets.m;
        let mut todo = vec![t];
        let mut seen = BTreeSet::new();
        while let Some(u) = todo.pop() {
            for x in m.tickets.values().filter(|x| x.parent == Some(u) || x.under == Some(u)) {
                if !seen.insert(x.t) {
                    continue;
                }
                todo.push(x.t);
                let head = match &x.status {
                    docket_machine::Status::Done(v) => v.get("head").and_then(Value::as_str).map(str::to_string).or_else(|| v.as_str().map(|_| "value".to_string())),
                    docket_machine::Status::Failed(r) => Some(r.exc.clone()),
                    _ => None,
                };
                if let Some(h) = head {
                    facts.push(("closed-under".into(), vec![id.clone(), x.kind.clone(), h]));
                }
            }
        }
        facts
    }

    /// The situation the norms are asked about ticket T in: the team, and T and the tickets above it with their kinds
    /// and openers; with T's id.
    fn ticket_facts(&self, ticket: Option<u64>) -> (Vec<(String, Vec<String>)>, Option<String>) {
        let mut facts = self.norm_facts(&[]);
        let mut id = None;
        // tickets opened by a pattern have no opener: theirs is the nearest above
        let mut unowned: Vec<String> = Vec::new();
        let mut at = ticket;
        while let Some(u) = at {
            let Some(x) = self.tickets.m.tickets.get(&u) else { break };
            let uid = self.tickets.m.id(u);
            facts.push(("ticket".into(), vec![uid.clone()]));
            facts.push(("ticket-kind".into(), vec![uid.clone(), x.kind.clone()]));
            match &x.opener {
                Some(o) => {
                    for b in unowned.drain(..) {
                        facts.push(("opener".into(), vec![b, o.clone()]));
                    }
                    facts.push(("opener".into(), vec![uid.clone(), o.clone()]));
                }
                None => unowned.push(uid.clone()),
            }
            // the owner archived it, or a ticket it is under (tickets.eflint `archived`)
            if self.tickets.m.archived_by(u).is_some() {
                facts.push(("archived".into(), vec![uid.clone()]));
            }
            id.get_or_insert(uid);
            at = x.parent.or(x.under);
        }
        // who takes part in the ticket asked about: its opener, its posts' authors and those they named, and whoever
        // works on it (`coord item start`), so a post on an item is in its workers' digest (I-1480, N-1481 p15)
        if let (Some(t), Some(uid)) = (ticket, &id) {
            let mut takers = self.takers(t);
            for a in self.tickets.m.tickets.get(&t).map(|x| x.started.clone()).unwrap_or_default() {
                if !takers.contains(&a) {
                    takers.push(a);
                }
            }
            for a in takers {
                facts.push(("takes-part".into(), vec![a, uid.clone()]));
            }
        }
        // who works on which open item (`started` lines)
        for x in self.tickets.m.tickets.values().filter(|x| x.is_open()) {
            for a in &x.started {
                facts.push(("started".into(), vec![a.clone(), self.tickets.m.id(x.t)]));
            }
        }
        // the agent tree, closed: A is above B when B's parent, or its parent's, ... is A
        for a in self.agents.values() {
            let mut seen = BTreeSet::new();
            let mut p = a.parent.clone();
            while let Some(up) = self.agents.get(&p) {
                if !seen.insert(up.name.clone()) {
                    break;
                }
                facts.push(("above".into(), vec![up.name.clone(), a.name.clone()]));
                p = up.parent.clone();
            }
        }
        (facts, id)
    }

    /// The ticket deliveries due to agent NAME that are not on their way.
    pub fn ticket_due(&self, name: &str) -> Vec<Delivery> {
        self.tickets.due(name)
    }

    /// The steps due to agent NAME whose call carries a `where` its model does not meet, each refused (`ModelRefused`:
    /// the step raises NoWorker, the model and the condition said). The owner, the runner and the program agents run
    /// no model; an agent not registered yet is one being made for the step, its model chosen to meet it.
    pub fn refuse_unmet(&mut self, name: &str) {
        let Some(a) = self.agents.get(name).filter(|a| crate::core::harness(&a.kind)) else { return };
        let model = a.model.split(':').next().unwrap_or("").to_string();
        // a step is due to NAME only from a ticket awaiting it (`due_on`): what is due is read on those whose call
        // carries a `where`, and not at all when none does, where each Deliver made a pass over every ticket
        // (paper-research N-1396 p424)
        let m = &self.tickets.m;
        let awaiting: Vec<_> = m.tickets.values().filter(|x| matches!(&x.status, docket_machine::Status::Awaits { agent, owed, .. } if agent == name && m.call_of(x.t, &owed.call).is_some_and(|s| s.cond.is_some()))).collect();
        if awaiting.is_empty() {
            return;
        }
        let due: Vec<Delivery> = m.due_among(name, awaiting.into_iter()).into_iter().filter(|d| !self.tickets.handed.contains_key(&(name.to_string(), d.id.clone()))).collect();
        let mut owed: Vec<(u64, u32, String, String)> = Vec::new();
        for d in due {
            if let docket_machine::What::Step { step, call, .. } = &d.what {
                let cond = self.tickets.m.call_of(d.t, call).and_then(|s| s.cond);
                if let Some(c) = cond.filter(|_| !owed.iter().any(|(t, s, _, _)| *t == d.t && s == step)) {
                    owed.push((d.t, *step, call.clone(), c));
                }
            }
        }
        if owed.is_empty() {
            return;
        }
        let order = match crate::models::Order::read(&self.places(), self.models()) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("coordd: tickets: the calls' models for {name}: the policy: {e}");
                return;
            }
        };
        for (t, step, call, c) in owed {
            let why = match docket_machine::syntax::model_cond(&c, 1).and_then(|e| order.meets(&e, &model)) {
                Ok(true) => continue,
                Ok(false) => format!("{name} runs {}, and call {call} needs `where {c}`", order.show(&model)),
                Err(e) => format!("{name} runs {model}, and call {call} needs `where {c}`, which cannot be told of it: {e}"),
            };
            if let Err(e) = self.ticket_input(Input::ModelRefused { t, step, agent: name.to_string(), why }) {
                eprintln!("coordd: tickets: step {step} of {t} to {name}: its refusal not taken: {e}");
            }
        }
    }

    /// Mark DS as handed to NAME's session, awaiting its answer; with each its ticket's view, for the rendering.
    pub fn ticket_handing(&mut self, name: &str, ds: Vec<Delivery>) -> Vec<(Delivery, TicketView)> {
        ds.into_iter()
            .filter_map(|d| {
                let h = Handing::of(&self.tickets.m, name, &d);
                self.tickets.handed.insert((name.to_string(), d.id.clone()), (d.t, d.upto, h));
                let v = self.tickets.m.view(d.t)?;
                Some((self.agents_filled(d), v))
            })
            .collect()
    }

    /// INPUT, or, when it is an answer (given or formed) one of whose Agent fields names no registered agent, its
    /// refusal for that (`Input::Unfit`, item 11, G5): only coordd knows who is registered.
    fn unregistered(&self, input: Input) -> Input {
        // only an input the machine would take as the answer to the step owed: an Answer by the agent it awaits; a
        // Formed of that step while it is being formed, not of the owner's words (those are a post if they do not fit)
        let (t, head, by, step, call) = match &input {
            Input::Answer { t, head, by, .. } => (*t, head, Some(by), None, None),
            Input::Formed { t, head: Some(head), step, call, .. } => (*t, head, None, Some(*step), call.clone()),
            _ => return input,
        };
        let Some(x) = self.tickets.m.tickets.get(&t).filter(|x| x.is_open()) else { return input };
        let docket_machine::Status::Awaits { owed, agent, forming, .. } = &x.status else { return input };
        let takes = match (by, step) {
            (Some(by), _) => by == agent,
            (None, Some(step)) => step == owed.step && forming.is_some() && x.owner_words.is_none(),
            _ => false,
        };
        if owed.hold || !takes {
            return input;
        }
        let mut named = Vec::new();
        agents_named(&owed.head, head, &mut named);
        if let Some(n) = named.into_iter().find(|n| n != "owner" && !self.agents.contains_key(n)) {
            let names: Vec<&str> = self.agents.values().filter(|a| crate::core::harness(&a.kind)).map(|a| a.name.as_str()).collect();
            return Input::Unfit { t, step: owed.step, why: format!("{n} is not a registered agent (they are {})", names.join(", ")), call };
        }
        // an agent named where the program gives it calls later (`agents_given`, paper-research N-101 p980): its model
        // meets each of their classes, or the answer is not taken; the one check, before any of those steps is due
        match self.unmet_given(&x.kind, &owed.call, &owed.head, head) {
            Some(why) => Input::Unfit { t, step: owed.step, why, call },
            None => input,
        }
    }

    /// Why answer HEAD (of type TY) to a step of CALL in a ticket of KIND names an agent whose model does not meet the class of a call the
    /// program gives the agent at that place (`Defs.agents_given`): which agent, which call and which class, as the
    /// step's gives names them. None when each meets them, or when the policy cannot say.
    fn unmet_given(&self, kind: &str, call: &str, ty: &docket_machine::Type, head: &Value) -> Option<String> {
        let m = &self.tickets.m.defs;
        let places = m.agents_given.get(&(kind.to_string(), call.to_string()))?;
        let mut at = Vec::new();
        agents_at(ty, head, &mut Vec::new(), &mut at);
        let mut order = None;
        for (place, n) in at {
            let Some(cs) = places.get(&place) else { continue };
            let Some(a) = self.agents.get(&n).filter(|a| crate::core::harness(&a.kind)) else { continue };
            let model = a.model.split(':').next().unwrap_or("").to_string();
            for c in cs {
                let Some(w) = m.calls.get(c).and_then(|s| s.cond.clone()) else { continue };
                if order.is_none() {
                    order = Some(crate::models::Order::read(&self.places(), self.models()).ok()?);
                }
                let o = order.as_ref()?;
                if let Ok(false) = docket_machine::syntax::model_cond(&w, 1).and_then(|e| o.meets(&e, &model)) {
                    let one = docket_machine::doc::given_one(c, &m.calls, &m.call_docs);
                    return Some(format!("{n} runs {}, and the agent named at {place} is given {one}", o.show(&model)));
                }
            }
        }
        None
    }

    /// D with each `agents` marker among its step's arguments (`Type::Agents`, item 11, G1) filled in with the
    /// project's agents as they are now: those that can still be given work (paper-research N-101 p595, p599). One line
    /// each, its name, role, model, in a turn, free or not running: first those made for this thread, then the
    /// standing agents (made by no ticket), then those made for another thread still open, marked with it. Left out,
    /// in one closing line that counts them: the agents the program made for a step or a scope (fT-N), and those made
    /// for a thread now closed. Only what is handed over has it: the record keeps the marker.
    fn agents_filled(&self, mut d: Delivery) -> Delivery {
        let t = d.t;
        if let docket_machine::What::Step { args, .. } = &mut d.what {
            self.agents_into(t, args);
        }
        d
    }

    /// ARGS, a step's arguments on ticket T, with each `agents` marker filled in as `agents_filled` does: for the
    /// hand-over, and for the give halt's prompt, which shows what would be sent (paper-research N-101 p1256: the halt
    /// showed `{"agents":"project"}`).
    fn agents_into(&self, t: u64, args: &mut serde_json::Value) {
        let Some(o) = args.as_object_mut() else { return };
        if !o.values().any(docket_machine::types::is_agents) {
            return;
        }
        let m = &self.tickets.m;
        let here = m.root_of(t);
        let (mut this, mut standing, mut other, mut left) = (Vec::new(), Vec::new(), Vec::new(), 0usize);
        for a in self.agents.values().filter(|a| crate::core::harness(&a.kind)) {
            if m.program_made(&a.name) || a.ended.is_some() {
                left += 1;
                continue;
            }
            // the roles the record says it holds (its own and the policy's), else the registry's
            let roles = m.roles_of(&a.name);
            let role = if !roles.is_empty() { roles.join(" and ") } else if a.role.is_empty() { "member".to_string() } else { a.role.clone() };
            let model = if a.model.is_empty() { "-" } else { a.model.as_str() };
            let turn = if !a.running() {
                "not running"
            } else if a.live.state == "working" && a.ended_turn != a.live.turn {
                "in a turn"
            } else {
                "free"
            };
            let line = format!("{}: {role}, {model}, {turn}", a.name);
            match m.made_in(&a.name) {
                None => standing.push(line),
                Some(r) if r == here => this.push(format!("{line}, made for this thread")),
                Some(r) if m.thread_open(r) => other.push(format!("{line}, made for {}", m.id(r))),
                Some(_) => left += 1,
            }
        }
        let mut lines: Vec<String> = this.into_iter().chain(standing).chain(other).collect();
        if left > 0 {
            let them = if left == 1 { "1 agent is".to_string() } else { format!("{left} agents are") };
            lines.push(format!("({them} left out: made for one step or for a thread now closed, or ended by the owner; `coord ls` lists every agent.)"));
        }
        let text = if lines.is_empty() { "(no agents)".to_string() } else { lines.join("\n") };
        for v in o.values_mut().filter(|v| docket_machine::types::is_agents(v)) {
            *v = serde_json::Value::String(text.clone());
        }
    }

    /// The words ticket WORDS (`W-12`) a post of WHO's is the cleaned text of: the owner's own, ended in Send (N-487).
    pub fn ticket_cleaned_words(&self, who: &Who, words: &str) -> Result<u64, String> {
        if !matches!(who, Who::Owner) {
            return Err("only the owner's posts are his words cleaned".into());
        }
        let w = ticket_of(&self.tickets.m, words)?;
        self.tickets.m.cleaned_words(w).map_err(|why| format!("{words} {why}; nothing was posted."))?;
        Ok(w)
    }

    /// The thread a say of his cleaned words opened (its `owner_message` event, whose value names WORDS as `cleaned`,
    /// and the record keeps it) is looked at when an input changes it: the `cleaned` line goes on its first post of
    /// his, the words he sent, or, when the intake moved them, on the post of his that move made on the ticket named
    /// (N-564). It ends by the thread's own course, never by the clock (docket-core3 N-487 p73); a thread that closes
    /// with neither is dropped with a log line. Kept in memory: a coordd restart between the say and his post loses
    /// the link (the words ticket keeps what he typed, and the event's line names it).
    fn cleaned_pending_round(&mut self, changed: &[u64]) {
        if self.cleaned_pending.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.cleaned_pending);
        for (t, words) in pending {
            if !changed.contains(&t) {
                self.cleaned_pending.push((t, words));
                continue;
            }
            let Some((place, open)) = self.said_post(t) else { continue };
            match place {
                Some((u, n)) => {
                    if let Err(e) = self.ticket_input(Input::Cleaned { t: u, n, words }) {
                        eprintln!("coordd: tickets: {}.p{n} not recorded as {} cleaned: {e}", self.tickets.m.id(u), self.tickets.m.id(words));
                    }
                }
                None if !open => eprintln!("coordd: tickets: {} closed with no post of his words; not recorded as {} cleaned", self.tickets.m.id(t), self.tickets.m.id(words)),
                None => self.cleaned_pending.push((t, words)),
            }
        }
    }

    /// Where the say that opened thread T put his words (`cleaned_pending_round`): its first post of his, or, when the
    /// intake moved them, the post of his that move made on the ticket named; and whether T is open. None: no such T.
    fn said_post(&self, t: u64) -> Option<(Option<(u64, u32)>, bool)> {
        let x = self.tickets.m.tickets.get(&t)?;
        let on_thread = x.posts.iter().find(|p| p.by == "owner").map(|p| (t, p.n));
        let moved = match &x.status {
            docket_machine::Status::Done(v) if v["head"] == "Moved" => {
                let to = v["to"].as_u64().or_else(|| v["to"].as_str().and_then(docket_machine::parse_ticket));
                to.and_then(|u| Some((u, self.tickets.m.tickets.get(&u)?.posts.iter().rev().find(|p| p.by == "owner")?.n)))
            }
            _ => None,
        };
        Some((on_thread.or(moved), x.is_open()))
    }

    /// `cleaned_pending_round` for the points of his split messages (N-696): the `split` line goes on the post of his
    /// the say made, as the `cleaned` line does.
    fn split_pending_round(&mut self, changed: &[u64]) {
        if self.split_pending.is_empty() {
            return;
        }
        for (t, from) in std::mem::take(&mut self.split_pending) {
            if !changed.contains(&t) {
                self.split_pending.push((t, from));
                continue;
            }
            let Some((place, open)) = self.said_post(t) else { continue };
            match place {
                Some((u, n)) => {
                    if let Err(e) = self.ticket_input(Input::Split { t: u, n, from: from.clone() }) {
                        eprintln!("coordd: tickets: {}.p{n} not recorded as point {} of {}: {e}", self.tickets.m.id(u), from.point, from.message);
                    }
                }
                None if !open => eprintln!("coordd: tickets: {} closed with no post of his words; not recorded as point {} of {}", self.tickets.m.id(t), from.point, from.message),
                None => self.split_pending.push((t, from)),
            }
        }
    }

    /// The owner's post just made on ticket T (T's last post was BEFORE) is point FROM of a message of his split by
    /// point (N-696): recorded as `split`; R with what was refused added.
    pub fn ticket_split_post(&mut self, who: &Who, t: &str, before: u32, from: docket_machine::SplitFrom, r: Reply) -> Reply {
        let Ok(t) = ticket_of(&self.tickets.m, t) else { return r };
        let Some(n) = self.tickets.m.tickets.get(&t).and_then(|x| x.posts.iter().rev().find(|p| p.by == who.name() && p.n > before)).map(|p| p.n) else { return r };
        match self.ticket_input(Input::Split { t, n, from }) {
            Ok(_) => r,
            Err(e) => ok(format!("{} Not recorded as a point of his split message: {e}", r.out)),
        }
    }

    /// The number of ticket T's last post (0 when none or no such ticket): taken before a post, so the post a request
    /// made is told from an older one (`ticket_cleaned`).
    pub fn ticket_last_post(&self, t: &str) -> u32 {
        ticket_of(&self.tickets.m, t).ok().and_then(|t| self.tickets.m.tickets.get(&t)).map_or(0, |x| x.last())
    }

    /// The owner's post just made on ticket T (its reply R; T's last post was BEFORE) is his words ticket WORDS
    /// cleaned: recorded as `cleaned` (N-487); R with what was refused added. Nothing is given to anyone for it. When
    /// his words were read as the answer to a step instead, no post of his was made on T and nothing is recorded.
    pub fn ticket_cleaned(&mut self, who: &Who, t: &str, before: u32, words: u64, r: Reply) -> Reply {
        let Ok(t) = ticket_of(&self.tickets.m, t) else { return r };
        let Some(n) = self.tickets.m.tickets.get(&t).and_then(|x| x.posts.iter().rev().find(|p| p.by == who.name() && p.n > before)).map(|p| p.n) else { return r };
        match self.ticket_input(Input::Cleaned { t, n, words }) {
            Ok(_) => r,
            Err(e) => ok(format!("{} Not recorded as his words cleaned: {e}", r.out)),
        }
    }

    /// WHO's post just made on ticket T (its reply R) answers the messages ALSO too: the reply tool's `to` list
    /// (paper-research N-101 p1361), recorded as `answers_also`; R with what was refused added.
    pub fn ticket_answers_also(&mut self, who: &Who, t: &str, also: &[String], r: Reply) -> Reply {
        let Ok(t) = ticket_of(&self.tickets.m, t) else { return r };
        let Some(n) = self.tickets.m.tickets.get(&t).and_then(|x| x.posts.iter().rev().find(|p| p.by == who.name())).map(|p| p.n) else { return r };
        match self.ticket_input(Input::AnswersAlso { t, n, also: also.to_vec() }) {
            Ok(said) => ok(format!("{} {said}", r.out)),
            Err(e) => ok(format!("{} Not recorded as answering {}: {e}", r.out, also.join(", "))),
        }
    }

    /// WHO's post just made on ticket T (its reply R; T's last post was BEFORE) goes on the threads ON too (N-725): the
    /// same TEXT, by the same writer, each a post of its own with its own policy answer (SAID; those the policy refused,
    /// REFUSED, are said by name), then the posts made say each other, recorded as `posted_on`. R with each further
    /// post or why not added. A post made is the writer's post after the thread's last before the call, never an older
    /// one (consult-3, N-581.p35): on a thread not open the owner's words open a new thread following it instead (his
    /// `say`, whose post comes later), so that thread is named in the reply and tied to nothing. Each is of KIND, as
    /// the first is (I-1480).
    pub fn ticket_post_on(&mut self, who: &Who, t: &str, before: u32, text: &str, to: Option<String>, lead: Option<String>, kind: Option<PostKind>, on: Vec<(String, Said)>, refused: Vec<(String, String)>, r: Reply) -> Reply {
        let me = who.name();
        let made = |core: &Self, t: &str, before: u32| -> Option<(u64, u32)> {
            let t = ticket_of(&core.tickets.m, t).ok()?;
            core.tickets.m.tickets.get(&t)?.posts.iter().find(|p| p.n > before && p.by == me).map(|p| (t, p.n))
        };
        let closed = |core: &Self, t: &str| ticket_of(&core.tickets.m, t).ok().and_then(|t| core.tickets.m.tickets.get(&t)).is_some_and(|x| !x.is_open());
        let mut out = r.out;
        let mut posts = Vec::new();
        match made(self, t, before) {
            Some(p) => posts.push(p),
            None if closed(self, t) => out = format!("{out} ({t} is not open: these words went to a new thread following it, which is not tied to the others.)"),
            None => {}
        }
        for (o, said) in on {
            let before = self.ticket_last_post(&o);
            let r = self.ticket_post_typed(who, &o, text.to_string(), to.clone(), None, said, lead.clone(), false, kind.clone());
            match made(self, &o, before).filter(|_| r.ok) {
                Some(p) => {
                    posts.push(p);
                    out = format!("{out} {}", r.out);
                }
                None if r.ok && closed(self, &o) => out = format!("{out} {} ({o} is not open: these words went to a new thread following it, which is not tied to the others.)", r.out),
                None => out = format!("{out} Not posted on {o}: {}", r.out),
            }
        }
        for (o, why) in refused {
            out = format!("{out} Not posted on {o}: {why}");
        }
        if posts.len() > 1 {
            let ids: Vec<String> = posts.iter().map(|(t, n)| format!("{}.p{n}", self.tickets.m.id(*t))).collect();
            for (i, (t, n)) in posts.iter().enumerate() {
                let others: Vec<String> = ids.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, id)| id.clone()).collect();
                if let Err(e) = self.ticket_input(Input::PostedOn { t: *t, n: *n, on: others }) {
                    out = format!("{out} Not recorded as also on the others: {e}");
                }
            }
        }
        ok(out)
    }

    /// The owner's posts, as (ticket, post), and each ticket as (ticket, 0): what a request of his adds is the
    /// difference (`ticket_wait_posts`).
    pub fn ticket_owner_posts(&self) -> BTreeSet<(u64, u32)> {
        self.tickets.m.tickets.values().flat_map(|x| std::iter::once((x.t, 0)).chain(x.posts.iter().filter(|p| p.by == "owner").map(move |p| (x.t, p.n)))).collect()
    }

    /// The owner's posts made since BEFORE wait for the turn's end of whom they reach (his "when done", N-101 p1216,
    /// p1390): not put into a running turn, and not read past by their own posts before they are given them. A ticket
    /// opened since with no post of his yet (a `msg`'s or `say`'s intake) carries the message: the post intake makes
    /// of it waits. Recorded as `waits`.
    pub fn ticket_wait_posts(&mut self, before: &BTreeSet<(u64, u32)>) {
        let new: Vec<(u64, u32)> = self.ticket_owner_posts().difference(before).copied().collect();
        for &(t, n) in &new {
            let n = match n {
                0 if new.iter().any(|&(u, m)| u == t && m > 0) => continue,
                0 => None,
                n => Some(n),
            };
            if let Err(e) = self.ticket_input(Input::Waits { t, n }) {
                eprintln!("coordd: tickets: waits {t} {n:?}: {e}");
            }
        }
    }

    /// Whether NAME's turn, which TICKETS were handed into, still runs, read under the lock the rendering is made
    /// under: one that has ended since takes them as handed out of a turn, given as such (not put in), so its footer
    /// says where the next turn's last message goes (paper-research N-807.p37).
    pub fn ticket_still_in_turn(&mut self, name: &str, tickets: &[(Delivery, TicketView)]) -> bool {
        if self.agents.get(name).is_some_and(|a| self.in_turn(a)) {
            return true;
        }
        for (d, _) in tickets {
            self.tickets.into.remove(&(name.to_string(), d.id.clone()));
        }
        false
    }

    /// The sender's answer for the ticket deliveries among IDS: taken (the semantics' Take) or not (handed again
    /// later).
    pub fn ticket_handed(&mut self, name: &str, ids: &[String], took: bool) {
        for id in ids {
            let backs = self.tickets.backs.remove(&(name.to_string(), id.clone())).unwrap_or_default();
            let Some((t, upto, h)) = self.tickets.handed.remove(&(name.to_string(), id.clone())) else { continue };
            let skipped = h.skipped(&self.tickets.m, name, t, upto);
            let into = self.tickets.into.remove(&(name.to_string(), id.clone()));
            if took {
                let input = if into { Input::GivenInto { t, to: name.into(), delivery: id.clone(), upto, skipped, place: None } } else { Input::Given { t, to: name.into(), delivery: id.clone(), upto, skipped, place: None } };
                if let Err(e) = self.ticket_input(input) {
                    eprintln!("coordd: tickets: given {id} to {name}: {e}");
                }
                self.ticket_backs(name, backs);
            }
        }
    }

    /// Posts BACKS, taken back while a hand-over carrying them was on its way to NAME, which took it: NAME had them, so
    /// each is struck and NAME told (`Input::StruckFor`; 0.2 B8, N-1396; consult-3, N-818 p74).
    fn ticket_backs(&mut self, name: &str, backs: Vec<(u64, u32)>) {
        for (t, n) in backs {
            if let Err(e) = self.ticket_input(Input::StruckFor { t, n, to: name.into() }) {
                eprintln!("coordd: tickets: {name} took post {n} of {t}, taken back on its way: {e}");
            }
        }
    }

    /// NAME's transcript holds ticket delivery ID at LOC: where it was given.
    pub fn ticket_seen(&mut self, name: &str, id: &str, loc: Loc) {
        let Some((t, _)) = docket_machine::parse_delivery(id) else { return };
        // the very entry a giving of it to NAME already records, read or made due again since (`Unread`): the same
        // giving, seen again by a follower's first read after a start. A delivery handed again is a new entry at a new
        // place (consult-3 N-412.p1596: N-412.p1373 and p1336, marked unread after their places were found, were given
        // again at the 20:38 start and counted in the next turns' ends; docket-core5 N-807.p105)
        let same = |l: &Loc| l.file == loc.file && l.offset == loc.offset;
        if self.tickets.m.tickets.get(&t).is_some_and(|x| x.given.iter().any(|g| g.to == name && g.delivery == id && g.place.as_ref().is_some_and(same))) {
            return;
        }
        // the place found after the sender said it was taken: the upto it was given with
        let given = self.tickets.m.tickets.get(&t).and_then(|x| x.given.iter().find(|g| g.to == name && g.delivery == id)).map(|g| g.upto);
        // nothing handed to say how far (seen after a restart): a posts delivery `N-k.pN` gives up to post N (N-101 p1334)
        let named = docket_machine::post_id(id).map(|(_, n)| n);
        let upto = given.filter(|g| *g > 0).or_else(|| self.tickets.handed.get(&(name.to_string(), id.to_string())).map(|h| h.1)).or(named).or(given).unwrap_or(0);
        if let Some(g) = self.inflight.get_mut(name) {
            g.ids.retain(|i| i != id);
            if g.ids.is_empty() {
                self.inflight.remove(name);
            }
        }
        // what the hand-over left out; nothing to say after a start, when nothing was handed (`Line::Given`): the
        // machine works it out (`Ticket::skipped_now`), and his post told late while the hand-over was on its way goes
        // once more, as `.told`, never lost (consult-3 N-1479 p353, paper-research p355)
        let skipped = self.tickets.handed.remove(&(name.to_string(), id.to_string())).and_then(|h| h.2.skipped(&self.tickets.m, name, t, upto));
        let into = self.tickets.into.remove(&(name.to_string(), id.to_string()));
        // in NAME's transcript, so taken: a post taken back on its way there is NAME's to be told of
        let backs = self.tickets.backs.remove(&(name.to_string(), id.to_string())).unwrap_or_default();
        self.ticket_backs(name, backs);
        // given to NAME already, with its place and as far: the machine would refuse it ("given already"), so it is not
        // put to it. A follower's first read after a restart or a switch sees every delivery in the transcript's whole
        // history; each went through the input under the lock, and research's first round held it 28554 ms (yaaihh
        // N-560 p28, paper-research p29)
        if self.tickets.m.tickets.get(&t).is_some_and(|x| x.given.iter().any(|g| g.to == name && g.delivery == id && !g.again() && g.place.is_some() && upto <= g.upto)) {
            return;
        }
        let input = if into { Input::GivenInto { t, to: name.into(), delivery: id.into(), upto, skipped, place: Some(loc) } } else { Input::Given { t, to: name.into(), delivery: id.into(), upto, skipped, place: Some(loc) } };
        if self.ticket_input(input).is_ok() {
            // its session has it: messages reach the sessions of the account it runs on (0.2 line 22; arrival is the
            // session's line for it, its enqueue, not its text entering the model, which a compaction can hold for
            // minutes: consult-3, N-412 post 2085, 1)
            self.account_reached(name);
            // and its session's own mark of deliveries never seen there is over (`Live::handed_unseen`)
            if let Some(a) = self.agents.get_mut(name).filter(|a| a.live.handed_unseen.is_some()) {
                a.live.handed_unseen = None;
                self.changed(name);
            }
        }
    }

    /// NAME's turn was cut short (its process exited, or a new session started): what it was given since its last turn
    /// ended is forgotten, as a logged line when there was any, so its next turn is counted afresh for Reply; but for
    /// the owner's posts it held, which come again as at a turn's end that did not answer them (the machine's
    /// `TurnCut`; paper-research N-412.p2202, 2).
    pub fn ticket_turn_cut(&mut self, name: &str) {
        // a `coord go` in a turn cut short: the move is made all the same (moves.rs)
        crate::moves::turn_went(&self.coord, name);
        // work carried from earlier turns and still running keeps its thread (an esc of a turn on C does not lose B's
        // rest); carried work whose notice came into the turn cut ends there, with no post, as at a turn given something
        // (consult-3, N-412.p1510); work running and carried by none, begun in the turn cut, is carried with no thread:
        // its notice posts nowhere, and a later turn does not take it for its own (consult-3, N-412.p1253, 2)
        let running: Vec<String> = self.agents.get_mut(name).and_then(|a| a.background.as_mut()).map(|b| b.still_running()).unwrap_or_default();
        let carry = self.tickets.carry.entry(name.to_string()).or_default();
        carry.retain(|(task, _)| running.contains(task));
        let new: Vec<String> = running.into_iter().filter(|task| !carry.iter().any(|(c, _)| c == task)).collect();
        carry.extend(new.into_iter().map(|task| (task, None)));
        if carry.is_empty() {
            self.tickets.carry.remove(name);
        }
        let _ = self.ticket_input(Input::TurnCut { agent: name.into() });
    }

    /// NAME was given deliveries and never began a turn on them ([`Core::not_read`]): each is made due again once, and
    /// one given twice is said on its ticket (paper-research m1005, 3).
    pub fn ticket_unread(&mut self, name: &str) {
        match self.ticket_input(Input::Unread { agent: name.into() }) {
            // what is due again, and what was given twice and not read: the journal's, never a post (the machine's
            // post by coordd is shown to nobody, paper-research N-412.p1844, 2)
            Ok(out) => self.reconciled(name, &out),
            Err(e) => eprintln!("coordd: tickets: unread {name}: {e}"),
        }
    }

    /// Ticket T's head as an agent is shown it beside the id: its title, else its first post's first line, else the
    /// ticket's own label. The update's `label`, and a part's head in a message joining several (`render::handover`).
    pub fn ticket_head(&self, t: u64) -> String {
        let m = &self.tickets.m;
        let label = m.tickets.get(&t).and_then(|x| docket_machine::label::given_label(m, x, &crate::label::read).or_else(|| x.posts.first().and_then(|p| crate::label::read(&p.text)).map(|s| docket_machine::label::first_line(&s, 60)).filter(|s| !s.is_empty())));
        label.unwrap_or_else(|| docket_machine::label::ticket(m, t, &crate::label::read))
    }

    /// DELIVERIES, given to NAME since its last turn ended, are due again from a fact of its session (0.2 A4): EXITED,
    /// its process ended before their text entered (not counted toward "given twice, not read"); else they left its
    /// queue without entering, or were never seen there. When that is all it was given, and not at an exit, it is the
    /// `unread` line as before; otherwise the `event` DUE_AGAIN names them, and the rest stays given.
    pub fn ticket_due_again(&mut self, name: &str, deliveries: &[String], exited: bool) {
        let all = self.tickets.m.given_since(name);
        let input = if !exited && all.len() == deliveries.len() && all.iter().all(|d| deliveries.contains(d)) {
            Input::Unread { agent: name.into() }
        } else {
            Input::Event { name: docket_machine::DUE_AGAIN.into(), value: json!({"agent": name, "deliveries": deliveries, "exited": exited}) }
        };
        match self.ticket_input(input) {
            Ok(out) => self.reconciled(name, &out),
            Err(e) => eprintln!("coordd: tickets: due again {name}: {e}"),
        }
    }

    /// NAME's process ended (or was found ended at a start): what it was given since its last turn ended and never put
    /// into its model's input is due again, its queue gone with it (0.2 A4: 27 messages were still queued when their
    /// session files ended in one day, a restart with items queued among them). A giving that entered was read. The
    /// posts it missed twice are due again with them, to the next process (consult-3, N-818.p76, 2).
    pub fn ticket_lost_with_process(&mut self, name: &str) {
        if !self.agents.get(name).is_some_and(|a| a.kind == "claude" || a.kind == "codex") {
            return;
        }
        let lost: Vec<String> = self.tickets.m.given_rows(name).into_iter().filter(|g| g.delivered.is_none()).map(|g| g.delivery).collect();
        if let Some(a) = self.agents.get_mut(name) {
            a.live.queue.clear();
        }
        // posts it missed twice wait for this too (the machine's `missed`): the same line hands them again
        if lost.is_empty() && !self.tickets.m.missed(name) {
            return;
        }
        if !lost.is_empty() {
            self.reconciled(name, &format!("its process ended with {} given and not in its input", lost.join(" ")));
        }
        self.ticket_due_again(name, &lost, true);
    }

    /// Whether delivery ID was handed to NAME before and not read: its text says it is the second hand-over.
    pub fn ticket_handed_before(&self, name: &str, id: &str) -> bool {
        self.tickets.m.handed_before(name, id)
    }

    /// NAME's session put delivery ID into the model's input (0.2 A1: delivered is the text in its input, not queued):
    /// the giving whose `where` is PLACE (its enqueue, paired by its text in the follower), AT as the session wrote it,
    /// BEGAN as the prompt of a turn. Recorded once per giving as the `event` kind DELIVERED; nothing when no giving of
    /// ID to NAME is at PLACE (an older one's remove read again at a start, a newer giving between read and act), or
    /// that one was delivered already (consult-3, N-1397 post 6, 1). One that began a turn whose `turn_began` line named
    /// none: that line again, naming it.
    pub fn ticket_delivered(&mut self, name: &str, id: &str, place: Loc, at: &str, began: bool) {
        let Some((t, _)) = docket_machine::parse_delivery(id) else { return };
        let same = |l: &Loc| l.file == place.file && l.offset == place.offset;
        let Some(g) = self.tickets.m.tickets.get(&t).and_then(|x| x.given.iter().find(|g| g.to == name && g.delivery == id && g.place.as_ref().is_some_and(same))) else { return };
        if g.delivered.is_some() {
            return;
        }
        let value = json!({"t": t, "to": name, "delivery": id, "at": at, "place": place, "began": began});
        if let Err(e) = self.ticket_input(Input::Event { name: docket_machine::DELIVERED.into(), value }) {
            eprintln!("coordd: tickets: delivered {id} to {name}: {e}");
        }
        // the turn it began, when that turn's `turn_began` line named none: coordd began the turn a read before its
        // session showed the prompt (its hooks or screen said it runs; main.rs `follow`), so no giving had a place
        // then. Its `turn_began` line again, naming this one, as `begin_turn` would have: the record says where the turn
        // began, the machine's `began` is it, so a copy can be cut there (`arrived`'s `began`), and an engine before this
        // reads the same. In the usual order the giving's place is read first, the agent not working yet, and its one
        // line names it. Every turn of an unboxed project on engine 80 (try-80; proto-mind's copy proof,
        // paper-research N-1446 p3297, p3298, p3314)
        if began && self.agents.get(name).is_some_and(|a| a.live.state == "working") && self.tickets.m.began(name).is_none() {
            let value = json!({"agent": name, "delivery": id});
            if let Err(e) = self.ticket_input(Input::Event { name: docket_machine::TURN_BEGAN.into(), value }) {
                eprintln!("coordd: {name}: its turn began with {id}, and the record refused the line: {e}");
            }
        }
    }

    /// The deliveries NAME was given since its last turn ended or was cut ([`Machine::given_since`]): the hand-over
    /// hands at most one step, and while this is not empty only more on the same tickets.
    pub fn ticket_given_since(&self, name: &str) -> Vec<String> {
        self.tickets.m.given_since(name)
    }

    /// Who can read a reply (paper-research N-17 post 5, 4): the owner, every agent but a program agent (the runner,
    /// whose deliveries go to a command), and the host sessions where the relay runs. Not coord, nor a name coordd does
    /// not know.
    pub fn reply_readers(&self) -> std::collections::BTreeSet<String> {
        let mut r: std::collections::BTreeSet<String> = self.agents.iter().filter(|(_, a)| a.kind != "program").map(|(n, _)| n.clone()).collect();
        r.insert("owner".into());
        if self.project.as_ref().is_some_and(|p| p.project.host_relay) {
            r.extend(self.hosts());
            // and the agents of other projects on its tickets, whose replies the relay carries (N-44)
            r.extend(self.far_names());
        }
        r
    }

    /// This project's name: its coord folder's, what $COORD ends in (N-44: PROJECT/NAME).
    pub fn project_name(&self) -> String {
        std::path::Path::new(&self.coord).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }

    /// N as this project reads it: OWN/NAME is NAME; any other name as it is.
    pub fn local_name(&self, n: &str) -> String {
        match far(n) {
            Some((p, a)) if p == self.project_name() => a.to_string(),
            _ => n.to_string(),
        }
    }

    /// N as a post names it here, the one test of who can be named (consult-3 N-1481 p51): its name here
    /// (`local_name`) when that is the owner, an agent, a host session (`is_host`) or another project's agent, the last
    /// only where the relay carries messages. Err(None): nobody of that name; Err(Some(why)): someone not reached from
    /// here. Each route says what it does with either: an `@NAME` in a post's words that names nobody is words, a
    /// kind's `to` or msg's NAME is refused.
    pub(crate) fn named_here(&self, n: &str) -> Result<String, Option<&'static str>> {
        let n = self.local_name(n);
        if far(&n).is_some() {
            return if self.relays() { Ok(n) } else { Err(Some(NOT_CARRIED)) };
        }
        if n == "owner" || self.agents.contains_key(&n) || self.is_host(&n) {
            Ok(n)
        } else {
            Err(None)
        }
    }

    /// Whether this coord's feed is carried by the host's relay (host_relay = true): to host sessions and to other
    /// projects.
    pub fn relays(&self) -> bool {
        self.project.as_ref().is_some_and(|p| p.project.host_relay)
    }

    /// The refusal of CMD naming N when N is another project's agent: only messages cross projects (N-44).
    pub fn not_across(&self, cmd: &str, n: &str) -> Result<String, String> {
        let n = self.local_name(n);
        match far(&n) {
            Some(_) => Err(format!("coord {cmd}: {n} is in another project; only messages cross projects")),
            None => Ok(n),
        }
    }

    /// Who takes part in ticket T for naming, telling or addressing them: its opener, its posts' authors and those they
    /// named (`Machine::participants`); the relay never (N-65), nor coordd, the author of its own notes (N-70).
    pub fn takers(&self, t: u64) -> Vec<String> {
        self.tickets.m.participants(t).into_iter().filter(|n| !is_relay(n) && n != "coordd").collect()
    }

    /// The names of other projects' agents on the tickets: an opener, an author, a name a post names; not the relay.
    pub fn far_names(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for x in self.tickets.m.tickets.values() {
            for n in x.opener.iter().chain(x.posts.iter().flat_map(|p| std::iter::once(&p.by).chain(&p.names))) {
                if far(n).is_some() && !is_relay(n) {
                    out.insert(n.clone());
                }
            }
        }
        out
    }

    /// The thread here that came in from another project's ticket ORIGIN (`P:N-12`).
    pub fn origin_thread(&self, origin: &str) -> Option<u64> {
        self.tickets.m.tickets.values().find(|x| x.kind == "thread" && x.form.get("origin").and_then(Value::as_str) == Some(origin)).map(|x| x.t)
    }

    /// What is due to other projects' agents goes out through the feed, one line per post and far project, for the
    /// relay to post there (N-44); then it is given. A thread opened here: a post goes to project X unless its author
    /// is of X. A thread that came in from P (its origin): a post written here goes to P, `to` holding every far name
    /// it is due to; nothing else leaves it. So no post is carried twice or in a circle.
    pub fn hand_far(&mut self) {
        if !self.relays() {
            return;
        }
        let mut lines: BTreeMap<(u64, u32, String), (Vec<String>, Value)> = BTreeMap::new();
        let mut given = Vec::new();
        // their due read in one pass of the tickets, as `to_hand_over` reads the agents' (one pass per far name each
        // second was a third of research's follow round, a2h-speed N-459)
        let names: Vec<String> = self.far_names().into_iter().collect();
        let mut all = self.tickets.due_all(&names);
        for f in names {
            let Some((fp, _)) = far(&f).map(|(p, a)| (p.to_string(), a.to_string())) else { continue };
            for d in self.due_now_of(all.remove(&f).unwrap_or_default()) {
                let unread: &[docket_machine::Post] = match &d.what {
                    docket_machine::What::Posts { unread } | docket_machine::What::Closed { unread, .. } | docket_machine::What::Step { unread, .. } => unread,
                    _ => &[],
                };
                let Some(x) = self.tickets.m.tickets.get(&d.t) else { continue };
                let origin = x.form.get("origin").and_then(Value::as_str).map(str::to_string);
                // nothing nobody here wrote leaves: not the relay's posts, nor coordd's own (N-70: "AGENT was given D
                // twice and did not read it.", carried as the context of the next post due there)
                for p in unread.iter().filter(|p| !is_relay(&p.by) && p.by != "coordd") {
                    let dest = match &origin {
                        None if !p.by.starts_with(&format!("{fp}/")) => fp.clone(),
                        Some(o) if far(&p.by).is_none() => o.split_once(':').map(|(pp, _)| pp.to_string()).unwrap_or_default(),
                        _ => continue,
                    };
                    if dest.is_empty() {
                        continue;
                    }
                    let e = lines.entry((d.t, p.n, dest.clone())).or_insert_with(|| {
                        let mut line = json!({"at": now(), "kind": "msg", "delivery": "project", "id": format!("{}.p{}", d.ticket, p.n), "project": dest, "to": [], "from": p.by, "ticket": d.ticket, "text": crate::render::resolve(&p.text)});
                        if let Some(o) = origin.as_ref().filter(|o| o.split_once(':').is_some_and(|(pp, _)| pp == dest)) {
                            line["origin"] = json!(o);
                        }
                        (Vec::new(), line)
                    });
                    if !e.0.contains(&f) {
                        e.0.push(f.clone());
                    }
                }
                given.push((f.clone(), d));
            }
        }
        for (_, (to, mut line)) in lines {
            line["to"] = json!(to);
            append(&self.path("feed.jsonl"), &line);
        }
        let mut carried: Vec<String> = Vec::new();
        for (f, d) in given {
            self.ticket_given(&f, &d);
            if !carried.contains(&f) {
                carried.push(f);
            }
        }
        // carried is this side's part done (docket-edge, N-44 post 7): a far agent has no turn here whose end would
        // close what it was given, and its Nothing, or the relay's "Not delivered", never comes back as one; so no
        // ticket here keeps waiting on it. Its answer, when one comes, is a post as any other.
        for f in carried {
            self.ticket_turn_cut(&f);
        }
    }

    /// Whom AGENT's turn-end TEXT names with `@` that its reply may name too (paper-research N-581.p82), in the order
    /// written, once each: an agent here (`@OWN/NAME` read as NAME), not itself, and only one the team's norms let it
    /// name on the ticket the reply goes to (`may_name`, as `coord post` asks); a name they refuse is said in the
    /// journal, since the writer gets no answer to a reply as it does to `coord post` (consult-3, N-581.p90, 1).
    /// Not the owner: a reply's `@owner` tells him of nothing, as before this, until he says it should (turn-end
    /// replies are many more than posts made by hand; consult-3, N-581.p90, 2; paper-research 19 Nov). Nobody when
    /// the turn would be replied on no ticket. Who of them can read a reply is the machine's to say (`readers`).
    fn reply_at_names(&self, agent: &str, text: Option<&str>, later: &[String]) -> Vec<String> {
        let Some(text) = text else { return Vec::new() };
        let Some((t, _)) = self.tickets.m.reply_names(agent, later) else { return Vec::new() };
        let mut out: Vec<String> = Vec::new();
        for n in named_in(text, &|n| self.agents.contains_key(n) || far(n).is_some()) {
            let n = self.local_name(&n);
            if n == agent || out.contains(&n) || !self.agents.contains_key(&n) {
                continue;
            }
            match self.may_name(agent, std::slice::from_ref(&n), Some(t)) {
                Ok(()) => out.push(n),
                Err(why) => eprintln!("coordd: tickets: {agent}'s reply on {} does not name {n}: {why}", self.tickets.m.id(t)),
            }
        }
        out
    }

    /// TEXT's parts for THREADS ([`reply_parts`], DEFAULT as there), a `#ID` address naming a ticket here that is not
    /// among them read as that ticket (its id exactly as the record writes it).
    fn reply_parts_here(&self, text: &str, threads: &[docket_machine::machine::ReplyThread], default: Option<usize>) -> Vec<(To, String)> {
        let m = &self.tickets.m;
        reply_parts(text, threads, default, &|id| ticket_of(m, id).ok().map(|t| m.id(t)).filter(|c| c == id))
    }

    /// AGENT's turn-end PARTS posted on the THREADS it was given (`reply_parts`, N-807): each part as the reply tool
    /// posts it (under the post it answers, naming that post's author and whom it names with `@`), and the others on the
    /// thread hear of it by policy, as of the rule Reply's post. A part for a thread it was not handed is posted there as
    /// `coord post` posts it, the norms asked off the lock (`Job::Post`; paper-research N-412.p2208). A part refused (a
    /// name the norms do not let it name) is said in the journal; its words stay in the session.
    fn reply_in_parts(&mut self, agent: &str, parts: Vec<(To, String)>, threads: &[docket_machine::machine::ReplyThread]) {
        for (to, words) in parts {
            let (i, n) = match to {
                To::Thread(i, n) => (i, n),
                To::Ticket(t, answers) => {
                    // a `.pN` that names no post there answers none: `coord post` would refuse the part whole for it
                    // (consult-3, N-1397 p134); posts are not taken out, so what is read here holds when it is posted
                    let m = &self.tickets.m;
                    let answers = answers.filter(|n| ticket_of(m, &t).ok().and_then(|u| m.tickets.get(&u)).is_some_and(|x| x.posts.iter().any(|p| p.n == *n)));
                    if let Some(jobs) = &self.jobs {
                        let _ = jobs.send(crate::core::Job::Post { agent: agent.into(), t, text: words, answers });
                    }
                    continue;
                }
            };
            let x = &threads[i];
            let r = self.ticket_post(&Who::Agent(agent.into()), &x.id, words, None, Some(n), Said::default(), None, false);
            if !r.ok {
                eprintln!("coordd: tickets: {agent}'s last message: its part for {} is not posted: {}", x.id, r.out);
                continue;
            }
            let Some(p) = self.tickets.m.tickets.get(&x.t).and_then(|y| y.posts.iter().rev().find(|p| p.by == agent)).map(|p| p.n) else { continue };
            let facts = self.ticket_facts(Some(x.t)).0;
            let places = self.places();
            self.ticket_task(Task::Notify { t: x.t, event: "post".into(), actor: agent.into(), post: Some(p), ticket: x.id.clone(), facts, places });
        }
    }

    /// Whom the rule Reply will name for posts delivery D, for its footer (N-17 post 14): the authors of its posts
    /// told to the reader, once each, in order, who can read a reply. None for anything else.
    pub fn ticket_reply_names(&self, d: &Delivery) -> Option<Vec<String>> {
        let docket_machine::What::Posts { unread } = &d.what else { return None };
        let readers = self.reply_readers();
        let mut out: Vec<String> = Vec::new();
        for p in unread.iter().filter(|p| p.by != d.to && p.wakes().any(|w| *w == d.to) && readers.contains(&p.by)) {
            if !out.contains(&p.by) {
                out.push(p.by.clone());
            }
        }
        Some(out)
    }

    /// The ticket NAME's turn would be replied on (the rule Reply) if it is handed IDS now besides what it was given
    /// this turn; None when they are several things (posts on two tickets, or a step among them). The hand-over's
    /// footer asks this, so it says what the rule will do: "End your turn with your reply; it is posted here", or
    /// "reply with coord post" (paper-research m988).
    pub fn ticket_replies_on(&self, name: &str, ids: &[String]) -> Option<String> {
        self.tickets.m.reply_ticket(name, ids).map(|t| self.tickets.m.id(t))
    }

    /// NAME's turn ended with its final output at LOC: it answers the step it owes, if any (owner m613); else, when the
    /// turn was given exactly one post on a ticket and did not post there itself, the machine posts its last message
    /// there (the rule Reply, paper-research m952).
    pub fn ticket_turn_ended(&mut self, name: &str, loc: Loc) {
        self.ticket_turn_ended_queued(name, loc, &[]);
    }

    /// [`Core::ticket_turn_ended`], QUEUED the places of what the session held queued when the turn ended (given, not yet
    /// in the model's input): each giving there is the next turn's, recorded in `later` (0.2 A1, N-1397 post 5, 3).
    pub fn ticket_turn_ended_queued(&mut self, name: &str, loc: Loc, queued: &[Loc]) {
        // taken once: the hooks' end took the text said so far, and the transcript's stop shows the same place again
        if loc.len > 0 {
            let end = loc.offset + loc.len;
            if self.tickets.taken_at.get(name).is_some_and(|(f, e)| *f == loc.file && *e >= end) {
                return;
            }
            self.tickets.taken_at.insert(name.to_string(), (loc.file.clone(), end));
        }
        // what the turn did, as its transcript's follower read it, by where its output ends (insight (1)); one ending
        // before it, or in a session file before this one, is a turn whose end was never kept: dropped
        let worked = self.tickets.worked.get_mut(name).and_then(|ws| {
            let end = loc.offset + loc.len;
            let w = ws.iter().position(|(f, e, _)| *f == loc.file && *e == end).map(|i| ws.remove(i).2);
            ws.retain(|(f, e, _)| *f == loc.file && *e > end);
            w
        });
        // its last message as the session gave it, for the bindings on turn_end (the rule Reply may empty `loc` below)
        let said = loc.clone();
        // background work still running as it stops: its notice will wake it for the rest of this answer
        let (running, before) = self.agents.get_mut(name).and_then(|a| a.background.as_mut()).map(|b| (b.still_running(), b.before.clone())).unwrap_or_default();
        // carried work whose notice is in the transcript now: this turn is the one it began, or took it in
        let carried = self.tickets.carry.get(name).cloned().unwrap_or_default();
        let (done, still): (Vec<_>, Vec<_>) = carried.into_iter().partition(|(task, _)| !running.contains(task));
        let mut carry = still;
        // an edit made through Bash since the last look, recorded before the turn's end is taken (found.rs): also for a
        // turn given nothing from a ticket (a minder's)
        self.look_found(name, "turn_end");
        // what the turn is about, from the machine's count (the log's, so a restart of coordd mid-turn keeps it): the
        // step it was given (at most one), else anything it was given, so the end is kept and the count cleared and
        // the rule Reply reads the rest (paper-research m995: one count)
        let (given, later) = self.turn_holds(name, &loc, queued);
        // what it was on, for its next `free` (the delivery program's `doing`)
        self.desk_turn_ended(name, &given, &said);
        // where it stood in the turn, and the layer it asked to go to in it (moves.rs, N-101 p414): the turn's end
        // answers only a step there or in no layer, and none at all when it asked to go
        let go = crate::moves::turn_went(&self.coord, name);
        let stood = Some(crate::moves::stand(&self.coord, name).unwrap_or_default());
        // a turn given nothing from a ticket is none of the tickets' business (paper-research m1027: the gate is the
        // machine's count, not a flag the transcript's follower carried)
        // work begun in this turn and still running is carried with the thread this turn replies on and whom its reply
        // names (none for a turn given nothing): the rest of that answer reaches the same readers, also on a thread
        // closed meanwhile. Work carried already keeps its thread
        let reply = if given.is_empty() { None } else { self.tickets.m.reply_ticket(name, &[]).map(|t| (t, self.tickets.m.reply_names(name, &later).map(|(_, n)| n).unwrap_or_default())) };
        // (work running already when the reader was made, at coordd's start, was begun in no turn seen here: none's)
        let new: Vec<String> = running.iter().filter(|task| !carry.iter().any(|(c, _)| c == *task) && !before.contains(*task)).cloned().collect();
        carry.extend(new.into_iter().map(|task| (task, reply.clone())));
        if carry.is_empty() {
            self.tickets.carry.remove(name);
        } else {
            self.tickets.carry.insert(name.to_string(), carry);
        }
        if given.is_empty() {
            // a turn given nothing, begun by the notice of work a turn stopped on: its final output is the rest of that
            // answer, posted on that turn's thread, once per thread. A turn given something answers by the rule alone
            let mut threads: Vec<(u64, Vec<String>)> = Vec::new();
            for (_, to) in done {
                if let Some((t, names)) = to.filter(|(t, _)| !threads.iter().any(|(u, _)| u == t)) {
                    threads.push((t, names));
                }
            }
            let text = (loc.len > 0 && !threads.is_empty()).then(|| crate::follow::text_at(&loc.file, loc.offset, loc.len)).flatten();
            if text.as_deref().is_some_and(|x| !says_nothing(x)) {
                for (t, names) in threads {
                    if let Err(e) = self.ticket_input(Input::Post { t, by: name.into(), text: Text::Loc { loc: loc.clone() }, names, told: vec![], inform: vec![], answers: None, kind: None }) {
                        eprintln!("coordd: tickets: {name}'s answer after its background work, on {}: {e}", self.tickets.m.id(t));
                    }
                }
            }
            self.turn_end_event(name, &said, false);
            return;
        }
        // what it was given began with the warning for its last answer, if any (moves.rs): said; this turn's own
        // answer may keep another below
        crate::moves::warned(&self.coord, name);
        // what it began with: its own step, else what it holds first; a step put into it is answered there (`coord
        // answer`) or comes again, never by its last message
        let delivery = self.tickets.m.turn_delivery(name, &given);
        let began = delivery.is_some();
        // Nothing (paper-research N-17): a turn that answers no step and ends with the single word Nothing has nothing
        // posted by the rule Reply, which posts nothing for an empty text; the turn end is kept, so what it was given
        // counts as read, and nobody is woken
        // and a reply nobody named could read (a program agent's post, coord's notice: N-17 post 5, 4) is not made: the
        // same empty text, so no post and no hand-over to drop
        let mut loc = loc;
        self.tickets.m.readers = Some(self.reply_readers());
        let answers = go.is_none() && self.tickets.m.turn_answers_step(name, delivery.as_deref(), &later, stood.as_deref());
        // whom its last message names with `@` (paper-research N-581.p82): the rule Reply names them too, so an order
        // to a third agent in a reply wakes it, as in a post made with `coord post`; and a reply nobody is owed is
        // still posted when it names someone who can read it
        let mut named: Vec<String> = Vec::new();
        // handed only notices of coordd's before the install that shows them to nobody (paper-research N-412.p1844, 2):
        // no thread shows it anything to answer, so its last message is posted nowhere, in parts or whole, whomever it
        // names (consult-3 N-412.p1982, 2)
        let hidden = loc.len > 0 && !answers && go.is_none() && self.tickets.m.turn_shows_nothing(name, &later);
        if hidden {
            self.reconciled(name, "its turn held only posts shown to nobody (coordd's notices): its last message is posted nowhere");
            loc.len = 0;
        }
        if loc.len > 0 && !answers && go.is_none() {
            let text = crate::follow::text_at(&loc.file, loc.offset, loc.len);
            if !text.as_deref().is_some_and(says_nothing) {
                named = self.reply_at_names(name, text.as_deref(), &later);
            }
            let unread = named.is_empty() && self.tickets.m.reply_names(name, &later).is_some_and(|(_, names)| names.is_empty());
            // the threads it passed his `@all` on in this turn: nothing of its last message is posted there
            let passed = self.passed_in_turn(name, &given);
            // posts on several threads (N-807): its last message answers each in a part of its own, posted there under
            // the post it answers; the rule Reply posts nothing for such a turn, as before. So too a turn on one thread,
            // or given only what informs it, whose last message has a part for a thread it was not handed (paper-research
            // N-412.p2208): that part goes there, and the rest as the parts say, its words without an address where the
            // rule Reply would have posted them (on its thread, or nowhere when nobody would read them); the rule Reply
            // then posts none of it, so no part shows twice (consult-3, N-1397 p132)
            let mut parted = false;
            if let Some(t) = text.as_deref().filter(|t| !says_nothing(t)) {
                let several = self.tickets.m.reply_threads(name, &later, false);
                let one = several.is_none();
                let threads = several.or_else(|| self.tickets.m.reply_threads(name, &later, true)).unwrap_or_default();
                let replied = if one && !unread { self.tickets.m.reply_names(name, &later).map(|(t, _)| t) } else { None };
                let rule = replied.and_then(|t| threads.iter().position(|x| x.t == t));
                let parts = self.reply_parts_here(t, &threads, if one { rule } else { first_told(&threads) });
                // the rule Reply posts the whole message on a thread not among those read from the record: not parted,
                // so a part for another thread shows once, inside that post, as before (consult-3, N-1397 p134). Only a
                // fallback now, for a delivery the record holds no giving of: a take-back's notice is read on its ticket
                // and parts as any other turn (proto-lead N-1397 p143)
                let whole = replied.is_some() && rule.is_none();
                if !one || (!whole && parts.iter().any(|(to, _)| matches!(to, To::Ticket(..)))) {
                    // a part for a thread it passed his `@all` on in this turn is not posted; its words stay in the session
                    let m = &self.tickets.m;
                    let (gone, parts): (Vec<(To, String)>, Vec<(To, String)>) = parts.into_iter().partition(|(to, _)| {
                        let t = match to {
                            To::Thread(i, _) => Some(threads[*i].t),
                            To::Ticket(id, _) => ticket_of(m, id).ok(),
                        };
                        t.is_some_and(|t| passed.contains(&t))
                    });
                    for (to, _) in &gone {
                        let id = match to {
                            To::Thread(i, _) => threads[*i].id.clone(),
                            To::Ticket(id, _) => id.clone(),
                        };
                        self.reconciled(name, &format!("its turn passed the owner's @all on {id}: its last message's part for {id} is posted nowhere"));
                    }
                    self.reply_in_parts(name, parts, &threads);
                    // all of it went in its parts
                    parted = one && (rule.is_some() || unread);
                }
            }
            if unread || parted || text.as_deref().is_some_and(says_nothing) {
                loc.len = 0;
            }
            // the rule Reply's post (`reply`, or `kept_reply` after a post of its own there that only passed it on), on a
            // thread it passed his `@all` on in this turn: none
            if loc.len > 0
                && let Some(t) = self.tickets.m.reply_names(name, &later).map(|(t, _)| t).filter(|t| passed.contains(t))
            {
                self.reconciled(name, &format!("its turn passed the owner's @all on {}: its last message is posted nowhere", self.tickets.m.id(t)));
                loc.len = 0;
            }
        }
        self.tickets.m.reply_named = (!named.is_empty()).then(|| (name.to_string(), named));
        self.tickets.m.turn_worked = worked.map(|w| (name.to_string(), w));
        let took = self.ticket_input(Input::TurnEnded { agent: name.into(), delivery, text: loc.clone(), later, stood: stood.clone(), go }).is_ok();
        self.tickets.m.reply_named = None;
        self.tickets.m.turn_worked = None;
        // a process of its turn killed at its box's memory limit: told now, not at the next minute (`oom_round`)
        if let Some(n) = Core::oom_counts().remove(name) {
            let mut counts = self.oom_seen.clone();
            counts.insert(name.to_string(), n);
            self.oom_round(counts);
        }
        // an answer given by ending the turn in a layer: its next delivery says what it left uncommitted (p466, 2)
        if answers && took {
            crate::moves::mark_answered(&self.coord, name, stood.as_deref());
        }
        self.turn_end_event(name, &said, began && took);
    }

    /// To the bindings on turn_end, if any (`Machine::bound`; none in agent-base): NAME's turn ended, each turn once,
    /// one given nothing or ending with Nothing too (the brain counts turns, N-1444 p16). TEXT its last message by place
    /// (none when empty), ANSWERED also true for a turn the rule Reply posted, and EVERY the policy's `check-in-every`
    /// for it, read as written (`policy::parts`), so the record holds the N the turn was counted against.
    fn turn_end_event(&self, name: &str, said: &Loc, answered: bool) {
        if !self.tickets.m.bound(docket_machine::syntax::TURN_END) {
            return;
        }
        let mut value = json!({"agent": name, "answered": answered});
        if said.len > 0 {
            value["text"] = Text::Loc { loc: said.clone() }.value();
        }
        if let Some(n) = self.parts().check_in.get(name) {
            value["every"] = json!(n);
        }
        self.ticket_event(docket_machine::syntax::TURN_END, &value);
    }

    /// Of GIVEN (NAME's deliveries since its last turn ended), those handed after its message at LOC was written:
    /// found in its transcript after LOC; or not found there yet and taken in a later second than the message's
    /// timestamp (a hand-over in the same second is counted for this turn, as before).
    /// Or held in the session's queue when it ended (STILL, the enqueue places, which the giving's `where` holds): given,
    /// not in its input (0.2 A1: 194 of research's deliveries in a week were queued as a turn ended and taken for it).
    /// What NAME's turn ending with the message at LOC was given, from the machine's count (`given_since`), and apart
    /// what was handed after that message was written, the next turn's (N-17 post 10): the session looked idle as soon
    /// as the message was on screen, and the follower found the turn's end only on its next read, after the hand-over
    /// was taken. STILL: what the session still held queued at that end (`handed_after`).
    pub(crate) fn turn_holds(&self, name: &str, loc: &Loc, still: &[Loc]) -> (Vec<String>, Vec<String>) {
        let given = self.tickets.m.given_since(name);
        let later = self.handed_after(name, &given, loc, still);
        (given.into_iter().filter(|d| !later.contains(d)).collect(), later)
    }

    /// The tickets NAME's turn ending now passed the owner's `@all` on (the pass tool's post, `Post::passes`): a pass of
    /// its own, not dropped, written in a later second than the turn's first giving among GIVEN (`turn_holds`), as the
    /// record keeps time. Its last message posts nothing there, whole or in parts: the `@all` is the other agent's now,
    /// and its words under his post would answer what it passed on (N-1104 p1015, under p1013 after the pass p1014;
    /// paper-research N-1540 p34, until placement takes this over). A pass of an earlier turn is not counted, nor one in
    /// the second its first giving was written, so such a message is posted as before rather than lost. Read live, as
    /// `turn_shows_nothing` is: the TurnEnded line holds the text left, so a replay reads what happened.
    fn passed_in_turn(&self, name: &str, given: &[String]) -> BTreeSet<u64> {
        let m = &self.tickets.m;
        let at = |d: &String| docket_machine::parse_delivery(d).and_then(|(t, _)| m.tickets.get(&t)).and_then(|x| x.given.iter().rev().find(|g| g.to == name && g.delivery == *d)).map(|g| g.at.clone());
        let Some(began) = given.iter().filter_map(at).min() else { return BTreeSet::new() };
        m.tickets.range(..docket_machine::HIDDEN).filter(|(_, x)| x.posts.iter().any(|p| p.passes.is_some() && p.by == name && !p.dropped && p.at > began)).map(|(t, _)| *t).collect()
    }

    fn handed_after(&self, name: &str, given: &[String], loc: &Loc, still: &[Loc]) -> Vec<String> {
        let written = crate::follow::written_at(&loc.file, loc.offset);
        given
            .iter()
            .filter(|d| {
                let Some(x) = docket_machine::parse_delivery(d).and_then(|(t, _)| self.tickets.m.tickets.get(&t)) else { return false };
                let Some(g) = x.given.iter().rev().find(|g| g.to == name && g.delivery == **d) else { return false };
                match (&g.place, &written) {
                    (Some(p), _) if still.iter().any(|q| q.file == p.file && q.offset == p.offset) => true,
                    (Some(p), _) => p.file == loc.file && p.offset > loc.offset,
                    (None, Some(w)) => g.at.get(..19).zip(w.get(..19)).is_some_and(|(g, w)| g > w),
                    (None, None) => false,
                }
            })
            .cloned()
            .collect()
    }

    /// A model chosen for a call made for step STEP of T (the clerk's split; a forming's goes through `run_task`): recorded on
    /// the ticket as a `decided` line, `model-for`, before the input it served (plan/core.md §6.2). Changes nothing.
    pub fn ticket_decision(&mut self, t: u64, step: u32, d: &coord_types::Decision) {
        if let Err(e) = self.ticket_input(model_for(t, step, d)) {
            eprintln!("coordd: tickets: model-for on {t}: {e}");
        }
    }

    /// A delivery to `clerk`, given: for a filing's `file` step, what the clerk's call needs ([`crate::clerk::answer`]);
    /// None for anything else (a notice to the clerk needs nothing).
    pub fn clerk_job(&mut self, d: &Delivery) -> Option<crate::clerk::Job> {
        // given first, so the log reads in order: given, then answered (docket-core m772)
        self.ticket_given(&d.to, d);
        let docket_machine::What::Step { step, args, .. } = &d.what else { return None };
        let args = crate::render::resolved(args);
        let text = args["text"].as_str().unwrap_or_default().to_string();
        if d.to == docket_machine::WATCHER && matches!(&d.what, docket_machine::What::Step { call, .. } if call == "route") {
            return Some(self.route_job(d, *step, &args));
        }
        if d.to == docket_machine::WATCHER && matches!(&d.what, docket_machine::What::Step { call, .. } if call == "asks") {
            // what an agent's post asks the owner is typed, `PostKind::Ask` to him, and no call reads it (paper-research
            // N-1446 p1081, his yes at N-1104 p728): untyped, he sees it whole, with no mark, as a failed call left it.
            // A step from before still open, or one a kind from before still makes, is answered Nothing here, at once
            if let Err(e) = self.ticket_answer(&d.to, d.t, serde_json::json!({"head": "Nothing"}), Text::New(String::new())) {
                eprintln!("coordd: tickets: watcher on {}: asks: {e}", d.ticket);
            }
            return None;
        }
        if d.to == docket_machine::WATCHER && matches!(&d.what, docket_machine::What::Step { call, .. } if call == "clean") {
            // the owner's words cleaned (the `words` kind, clean.rs, N-487): his text alone goes to the call
            return Some(crate::clerk::Job {
                worker: d.to.clone(),
                to: String::new(),
                t: d.t,
                step: *step,
                ticket: d.ticket.clone(),
                text,
                open: String::new(),
                split: None,
                places: self.places(),
                route: None,
                // the project's words and his own (the form's `keep`, the window's list) stay as typed with the agents'
                clean: Some(crate::clean::kept_words(&self.places(), self.agents.keys().cloned(), &serde_json::from_value::<Vec<String>>(args["keep"].clone()).unwrap_or_default())),
                brain: None,
                fold: None,
            });
        }
        if d.to == docket_machine::WATCHER {
            // a thread's intake: the open tickets the agent written to takes part in
            let to = args["to"].as_str().unwrap_or_default().to_string();
            let views: Vec<_> = self.ticket_views().into_iter().filter(|v| v.kind != "filing" && v.n != d.t).collect();
            // typed to nobody (TO is whoever fills lead): the open tickets the owner takes part in (design §16, 3)
            let typed = self.tickets.m.tickets.get(&d.t).is_none_or(|x| x.form.get("typed").and_then(Value::as_array).is_none_or(|a| !a.is_empty()));
            return Some(crate::clerk::Job {
                worker: d.to.clone(),
                open: crate::clerk::open_lines_of(&views, if typed { &to } else { "owner" }),
                to,
                t: d.t,
                step: *step,
                ticket: d.ticket.clone(),
                text,
                split: self.tickets.m.defs.calls.get("sort").map(|c| c.head.clone()),
                places: self.places(),
                route: None,
                clean: None,
                brain: None,
                fold: None,
            });
        }
        // the owner's words may be for any open ticket
        let views: Vec<_> = self.ticket_views().into_iter().filter(|v| v.kind != "filing").collect();
        Some(crate::clerk::Job {
            worker: d.to.clone(),
            to: String::new(),
            t: d.t,
            step: *step,
            ticket: d.ticket.clone(),
            text,
            open: crate::clerk::open_lines(&views),
            split: self.tickets.m.defs.calls.get("place").map(|c| c.head.clone()),
            places: self.places(),
            route: None,
            clean: None,
            brain: None,
            fold: None,
        })
    }

    /// The watcher's `route` step D (STEP, ARGS resolved) as the clerk's job takes it (design §13, route.call): the held
    /// post's words, whom they were typed to, the others on the ticket, and RECENT, the ticket's last three posts
    /// before it that an agent may see (none held, none dropped).
    fn route_job(&self, d: &Delivery, step: u32, args: &Value) -> crate::clerk::Job {
        let names = |v: &Value| v.as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect::<Vec<String>>();
        // the handler runs in its own ticket under T, and holds post N of T (`post_of`): found so, not from the args,
        // whose text reaches here already resolved (docket-live, 12 Nov: RECENT came out empty)
        let m = &self.tickets.m;
        let of = m.tickets.get(&d.t).and_then(|h| h.post_of.as_ref()).map(|p| (p.t, p.n));
        // his burst, when `gather` held more than one post (E1b): each post with its number, so a Split can name them;
        // its posts that say `@all` among his choices are not route's, claims has handed them (base.kind's `p.claimed`,
        // `Machine::his_claimed`; proto-lead N-1446 p475), so the rest is the burst, and one post left is the post
        let posts: Vec<u32> = m.tickets.get(&d.t).and_then(|h| h.post_of.as_ref()).map(|p| p.posts()).unwrap_or_default();
        let claimed = of.map(|(t, _)| m.his_claimed(t, &posts)).unwrap_or_default();
        let rest: Vec<u32> = posts.iter().copied().filter(|n| !claimed.contains(n)).collect();
        let burst: Vec<u32> = Some(rest.clone()).filter(|ns| ns.len() > 1).unwrap_or_default();
        // each post of it that carries his own choice, when they do not all carry one same (E1b hold 1; base.kind's
        // `p.chosen`, the machine's one reading): said on its line, and held to by `route_ok`
        let chosen: Vec<(u32, Vec<String>)> = of.map(|(t, _)| m.his_choices(t, &posts)).unwrap_or_default().into_iter().filter(|(n, _)| burst.contains(n)).collect();
        let on = of.and_then(|(t, _)| m.view(t));
        let held = on.as_ref().and_then(|v| v.posts.iter().find(|q| Some(q.n) == of.map(|(_, n)| n)));
        // with posts claimed, his message for route begins at the first post left (those claimed before it are his
        // posts before it, with whom they went to)
        let held = match (claimed.is_empty(), rest.first()) {
            (false, Some(n)) => on.as_ref().and_then(|v| v.posts.iter().find(|q| q.n == *n)),
            _ => held,
        };
        let line = |q: &docket_machine::Post| match chosen.iter().find(|(n, _)| *n == q.n) {
            Some((_, to)) => format!("#{} (his own choice: it goes to {} and no one else): {}", q.n, to.join(", "), crate::render::resolve(&q.text)),
            None => format!("#{}: {}", q.n, crate::render::resolve(&q.text)),
        };
        let text = match (held, on.as_ref()) {
            (Some(_), Some(v)) if !burst.is_empty() => v.posts.iter().filter(|q| burst.contains(&q.n)).map(line).collect::<Vec<_>>().join("\n"),
            (Some(q), _) => crate::render::resolve(&q.text),
            _ => args["text"].as_str().unwrap_or_default().to_string(),
        };
        // each post with whom it went to (consult-3 N-1443 p2, 2a: p587's whom saw p586 but not where it went): the
        // agents its handler told, else those it named
        let went = |q: &docket_machine::Post| if q.routed || !q.told.is_empty() { q.told.clone() } else { q.names.clone() };
        // his own words, as the machine tells them (`Machine::his_words`): not the words an agent opened the ticket with,
        // which thread.kind before ls-opener posted again as his (N-1445 #2; proto-lead N-1446 p83, p105; consult-3 p110)
        let on_t = of.map_or(0, |(t, _)| t);
        let his_words = |q: &docket_machine::Post| m.his_words(on_t, q.n);
        let recent = on.as_ref().map(|v| {
            // the opener's words posted as his are listed, under the agent that wrote them (proto-lead p105)
            let opener = v.posts.iter().find(|q| q.n == 1).map_or_else(String::new, |q| q.by.clone());
            let posts: Vec<(u32, String, String)> = v
                .posts
                .iter()
                .filter(|q| q.held.is_none() && !q.dropped && q.moved.is_none())
                .map(|q| {
                    let to = went(q);
                    let by = if q.by == "owner" && !his_words(q) { format!("{opener} (its opener, posted as the thread's first words)") } else { q.by.clone() };
                    (q.n, if to.is_empty() { by } else { format!("{by}, to {}", to.join(", ")) }, crate::render::resolve(&q.text))
                })
                .collect();
            crate::clerk::recent_lines(&posts, held.map_or(u32::MAX, |q| q.n))
        });
        let to = names(&args["to"]);
        let others = names(&args["others"]);
        // the thread's report for its holder (route.call; the owner's fundamentals, N-1104 p593): what the thread is and
        // whom it is for, what each agent on it holds there now, and his posts just before this one with where each
        // went; all of it the machine's, read from the record (consult-3 N-1443 p2, 3.1)
        let (thread, yours, his, fallback) = match (on.as_ref(), held) {
            (Some(v), Some(h)) => {
                let label = v.form.get("label").and_then(Value::as_str).filter(|l| !l.trim().is_empty()).map_or_else(|| v.title.clone(), String::from);
                let mut opened_for: Vec<String> = v.form.get("to").and_then(Value::as_str).filter(|a| !a.is_empty()).map(String::from).into_iter().collect();
                for a in names(&v.form["names"]) {
                    if !opened_for.contains(&a) {
                        opened_for.push(a);
                    }
                }
                let mine = |a: &str| -> Vec<u32> {
                    // the post each delivery ends at, his post told late by its own (`delivered_post`, (c2); consult-3 N-1446 p621)
                    let given: Vec<u32> = m.given_since(a).iter().filter_map(|d| docket_machine::delivered_post(d)).filter(|(t, _)| Some(*t) == of.map(|(t, _)| t)).map(|(_, n)| n).collect();
                    let last_own = v.posts.iter().filter(|q| q.by == a).map(|q| q.n).max().unwrap_or(0);
                    given.into_iter().filter(|n| *n > last_own).collect()
                };
                let holding: Vec<(String, Vec<u32>)> = to.iter().chain(&others).map(|a| (a.clone(), mine(a))).collect();
                let thread = crate::clerk::thread_line(&v.id, &label, &opened_for, v.under.as_deref().or(v.parent.as_deref()), &holding);
                // his posts since an agent last posted there, before this one: the words an agent opened it with, which
                // thread.kind posts again as his (N-1445 #2, paper-research's opener; proto-reader N-1446 p71, proto-lead
                // p74, 2), are that agent's post, so the run starts after them
                let before: Vec<&docket_machine::Post> = v.posts.iter().filter(|q| q.n < h.n && !q.dropped && !q.taken_back && q.moved.is_none()).collect();
                let from = before.iter().rposition(|q| !his_words(q)).map_or(0, |i| i + 1);
                let run: Vec<crate::clerk::Yours> = before[from..]
                    .iter()
                    .map(|q| {
                        let to = if q.held.is_some() { None } else { Some(went(q)) };
                        // given by a row that handed it (`ViewGiven::hands`, (c2); consult-3 N-1446 p621)
                        let given = to.as_ref().and_then(|to| v.given.iter().filter(|g| g.hands(q.n) && to.contains(&g.to)).map(|g| g.delivered.clone().unwrap_or_else(|| g.at.clone())).min());
                        crate::clerk::Yours { n: q.n, at: q.at.clone(), went: to, given, text: crate::render::resolve(&q.text) }
                    })
                    .collect();
                let (yours, his) = crate::clerk::yours_lines(&run, &h.at);
                // a post claims handed is the agent's who takes it: `joins` names none of them
                let his: Vec<u32> = his.into_iter().filter(|n| !claimed.contains(n)).collect();
                // where his words go should the call fail (proto-lead N-1446 p57.2): the agent the thread is for (its
                // form's `to`), when it is on the ticket; else where his last post on it went, those of them still on
                // the ticket, as `route_ok` holds a model's answer to (consult-3 p110, 2); else an earlier post's; else
                // none, and he is asked
                let on_it = |a: &String| to.contains(a) || others.contains(a);
                let thread_to: Vec<String> = v.form.get("to").and_then(Value::as_str).map(String::from).filter(on_it).into_iter().collect();
                let fallback = if thread_to.is_empty() { before.iter().rev().filter(|q| his_words(q) && q.held.is_none()).map(|q| went(q).into_iter().filter(|a| on_it(a)).collect::<Vec<String>>()).find(|w| !w.is_empty()).unwrap_or_default() } else { thread_to };
                (thread, yours, his, fallback)
            }
            _ => ("(not found)".to_string(), "(none)".to_string(), Vec::new(), Vec::new()),
        };
        eprintln!(
            "coordd: tickets: watcher on {}: route for {} post {}, {} recent line(s)",
            d.ticket,
            on.as_ref().map_or("no ticket found".to_string(), |v| v.id.clone()),
            held.map_or("not found".to_string(), |q| format!("#{}", q.n)),
            recent.as_deref().map_or(0, |r| if r == "(none)" { 0 } else { r.lines().count() })
        );
        crate::clerk::Job {
            worker: d.to.clone(),
            to: to.join(", "),
            t: d.t,
            step,
            ticket: d.ticket.clone(),
            text,
            open: String::new(),
            split: self.tickets.m.defs.calls.get("whom").map(|c| c.head.clone()),
            places: self.places(),
            route: Some(crate::clerk::Route { to, others, recent: recent.unwrap_or_else(|| "(none)".into()), thread, yours, his, fallback, burst, chosen }),
            clean: None,
            brain: None,
            fold: None,
        }
    }

    /// The clerk's answer HEAD to JOB, with the DECISION that chose its model: the decision first, then the answer.
    ///
    /// His words that fit more than one open thread (the intake's Unsure, two or more, each open: `clerk::verdict_ok`)
    /// stand on each of them, and nothing is asked of him (the owner, N-354.p1/N-355.p1: "psot on both for all i
    /// care"; paper-research N-586.p10): the step is answered Place on the first, which the machine posts as his, and
    /// the same words go on the others as his posts (`ticket_post_on`), each saying the others.
    pub fn clerk_answered(&mut self, job: &crate::clerk::Job, head: Value, decision: Option<coord_types::Decision>, failed: Option<String>) {
        let mut head = head;
        let mut also: Vec<String> = Vec::new();
        let mut beyond: Vec<String> = Vec::new();
        if job.worker == docket_machine::WATCHER && job.route.is_none() && head["head"] == "Unsure" {
            let ts: Vec<String> = head["tickets"].as_array().map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect()).unwrap_or_default();
            // two or more of them waiting on his own answer: a short `yes` would answer questions he did not answer,
            // so it stays a thread of its own with them beside it, as before (paper-research N-581.p57, 1)
            let waiting = ts.iter().filter(|t| self.awaits_owner(t)).count();
            if ts.len() >= 2 && waiting < 2 {
                // his words go on the first two the intake names, no more (N-581.p46); the rest are named to him
                also = ts[1..2].to_vec();
                beyond = ts[2..].to_vec();
                head = serde_json::json!({"head": "Place", "ticket": ts[0]});
            }
        }
        let first = head["ticket"].as_str().map(String::from).filter(|_| !also.is_empty());
        let before = first.as_deref().map(|t| self.ticket_last_post(t));
        if let Some(d) = &decision {
            self.ticket_decision(job.t, job.step, d);
        }
        // route's failure is its answer line's text, in coordd's words, and no notice (paper-research N-1446 p186)
        let said = if job.route.is_some() { failed.clone().unwrap_or_default() } else { String::new() };
        // the watcher's intake: a call that failed is said on the ticket, not only in coordd's log; a later call of the
        // same kind there that goes through clears it (paper-research N-8). The clerk's says it in its answer already
        // (Unplaced).
        if job.worker == docket_machine::WATCHER && job.route.is_none() {
            self.own_call_failed(&job.ticket, "intake", decision.as_ref(), failed, |e| format!("your words were filed as a thread without being sorted: the call failed ({e})"));
        }
        // typed to whom he typed them, as the machine's Place posts them; read before the answer moves the ticket on
        let typed = self.tickets.m.tickets.get(&job.t).and_then(|x| x.form.get("typed")).and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_str).map(String::from);
        let answered = match self.ticket_answer(&job.worker, job.t, head, Text::New(said)) {
            Ok(r) => {
                eprintln!("coordd: tickets: {} on {}: {r}", job.worker, job.ticket);
                true
            }
            Err(e) => {
                eprintln!("coordd: tickets: {} on {}: {e}", job.worker, job.ticket);
                false
            }
        };
        // only when the answer went through: a step he cancelled or an edit withdrew meanwhile puts nothing anywhere
        // (consult-3, N-581.p48)
        if let (true, Some(first), Some(before)) = (answered, first, before) {
            let on = also.into_iter().map(|t| (t, Said::default())).collect();
            let r = self.ticket_post_on(&crate::core::Who::Owner, &first, before, &job.text, typed, None, None, on, Vec::new(), ok(String::new()));
            eprintln!("coordd: tickets: watcher on {}: his words fit more than one thread: on {first}{}", job.ticket, r.out);
            // what he is told, on the first thread: the threads it fitted beyond two, by label and id, and what did
            // not go where it fitted
            let mut said: Vec<String> = Vec::new();
            if !beyond.is_empty() {
                let named: Vec<String> = beyond.iter().map(|t| self.ticket_label(t)).collect();
                said.push(format!("Your words fitted more threads; they went on two, not on {}.", named.join(", ")));
            }
            let tied = ticket_of(&self.tickets.m, &first).ok().and_then(|t| self.tickets.m.tickets.get(&t)).is_some_and(|x| x.posts.iter().any(|p| p.n > before && p.by == "owner" && !p.on.is_empty()));
            // ticket_post's own refusals name the command; the window names none (N-581.p57, 4)
            let out = r.out.replace("coord post: ", "");
            let out = out.trim();
            if !tied && !out.is_empty() {
                said.push(out.to_string());
            }
            if !beyond.is_empty() {
                // the notice is kept in memory only: the journal keeps which threads it fitted beyond two (N-581.p57, 3)
                eprintln!("coordd: tickets: watcher on {}: his words fitted {} too, not posted there", job.ticket, beyond.join(", "));
            }
            self.ticket_notice(&first, "fits", (!said.is_empty()).then(|| said.join(" ")));
        }
    }

    /// Whether ticket T waits on the owner's own answer (paper-research N-581.p67, consult-3): the view's `owed` (`waits_on`
    /// names him (a step of it or of a ticket under it owed by him, a handler awaiting his `clarify`, a slow-mode halt,
    /// asks the minder left with him)), or an agent's post on it asks him something still open. Not `owner.unanswered`
    /// (an agent spoke last), which would hold back nearly every message fitting two.
    fn awaits_owner(&self, t: &str) -> bool {
        let m = &self.tickets.m;
        let Some(x) = ticket_of(m, t).ok().and_then(|t| m.tickets.get(&t)) else { return false };
        m.view(x.t).is_some_and(|v| v.owed) || x.posts.iter().any(|p| m.asks_open(x, p.n))
    }

    /// Ticket T as he reads it: its title and id ("the window's colours (N-12)"); its id alone when it has no title.
    fn ticket_label(&self, t: &str) -> String {
        let m = &self.tickets.m;
        let title = ticket_of(m, t).ok().and_then(|x| m.view(x)).map(|v| v.title).unwrap_or_default();
        if title.trim().is_empty() { t.to_string() } else { format!("{} ({t})", title.trim()) }
    }

    /// Step STEP of T, delivery ID, could not be rendered for its agent NAME (WHY): it fails at once with NoCall, rather
    /// than stay owed by an agent never told of it (paper-research m737). The delivery is no longer on its way to
    /// NAME, so a sender's later "taken" gives nothing.
    pub fn ticket_unrendered(&mut self, name: &str, id: &str, t: u64, step: u32, why: String) {
        self.tickets.handed.remove(&(name.to_string(), id.to_string()));
        if let Err(e) = self.ticket_input(Input::Unrendered { t, step, why }) {
            eprintln!("coordd: tickets: unrendered step {step} of {t}: {e}");
        }
    }

    /// Hand the deliveries due to `clerk` (the filings' steps) to its job, which answers them (`Job::Clerk`). They stay
    /// on their way until the job gives them.
    /// The watcher's (the threads' intake steps) go the same way: coordd answers both itself.
    pub fn hand_clerk(&mut self) {
        let Some(jobs) = self.jobs.clone() else { return };
        for worker in ["clerk", docket_machine::WATCHER] {
            let due = self.ticket_due(worker);
            // a pass of an agent's brain on its own queue; a delivery run's `next` is answered where its `free` is
            // raised (delivery.rs `hand_passes`)
            let due = if worker == docket_machine::WATCHER { self.hand_passes(due) } else { due };
            if !due.is_empty() {
                let deliveries = self.ticket_handing(worker, due);
                let _ = jobs.send(crate::core::Job::Clerk { deliveries });
            }
        }
    }

    /// A program agent (the runner) ran delivery D: given at once, with no session and no place.
    pub fn ticket_given(&mut self, name: &str, d: &Delivery) {
        // what it leaves out, as it was handed (`Handing::skipped`), else as D carries it (`Machine::left_out`)
        let skipped = match self.tickets.handed.remove(&(name.to_string(), d.id.clone())) {
            Some((_, _, h)) => h.skipped(&self.tickets.m, name, d.t, d.upto),
            None => self.tickets.m.left_out(name, d),
        };
        let backs = self.tickets.backs.remove(&(name.to_string(), d.id.clone())).unwrap_or_default();
        if let Err(e) = self.ticket_input(Input::Given { t: d.t, to: name.into(), delivery: d.id.clone(), upto: d.upto, skipped, place: None }) {
            eprintln!("coordd: tickets: given {} to {name}: {e}", d.id);
        }
        self.ticket_backs(name, backs);
    }

    /// A fresh agent NAME (fT-N) ended: what is still due to it (a withdrawal of its step, a closing notice) is given
    /// as a program's is, with no place, so nothing waits on a session that is gone (paper-research m971, 14). The ids.
    pub fn ticket_drop_ended(&mut self, name: &str) -> Vec<String> {
        let due = self.ticket_due(name);
        for d in &due {
            self.ticket_given(name, d);
        }
        due.into_iter().map(|d| d.id).collect()
    }

    /// The text at LOC was copied to TO (its transcript is about to be removed): references move to the copy.
    pub fn ticket_pinned(&mut self, loc: Loc, to: Loc) {
        let _ = self.ticket_input(Input::Pinned { loc, to });
    }

    /// An answer from BY to the step ticket T awaits, given by a program (the runner's verdict) rather than a request.
    pub fn ticket_answer(&mut self, by: &str, t: u64, head: Value, text: Text) -> Result<String, String> {
        self.ticket_input(Input::Answer { t, by: by.into(), head, text })
    }

    /// Ticket T's view as it leaves coordd: a step halted before it is given (slow mode) shows the prompt its agent
    /// would read, rendered here (the engine holds no templates); an edited one shows the owner's text.
    /// The event for ticket V after it changed: whole the first time, or when every post changed; else a
    /// `ticketchange` with only the posts new or changed since its last event (paper-research N-101 p1165: N-101 whole
    /// was 2.8 MB a change).
    pub(crate) fn ticket_body(&mut self, mut v: TicketView) -> Body {
        use std::hash::{Hash, Hasher};
        let hashes: Vec<u64> = v.posts.iter().map(|p| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            serde_json::to_string(p).unwrap_or_default().hash(&mut h);
            h.finish()
        }).collect();
        let old = self.sent_posts.insert(v.id.clone(), hashes.clone());
        let Some(old) = old else { return Body::Ticket(v) };
        let count = v.posts.len() as u32;
        let changed: Vec<bool> = hashes.iter().enumerate().map(|(i, h)| old.get(i) != Some(h)).collect();
        if old.len() > hashes.len() || changed.iter().all(|c| *c) {
            return Body::Ticket(v);
        }
        let mut i = 0;
        v.posts.retain(|_| {
            i += 1;
            changed[i - 1]
        });
        Body::TicketChange { view: v, posts: count }
    }

    pub fn ticket_shown(&self, t: u64) -> Option<TicketView> {
        let mut v = self.tickets.m.view(t)?;
        // its line in the outline, with the threads under it (N-101 p1676, c)
        v.outline = self.tickets.m.tickets.get(&t).map(|x| docket_machine::view::ViewOutline::of(&self.tickets.m, x, true));
        // the posts that name it, and what its posts name (threads piece 3)
        let agent = |n: &str| self.is_agent(n);
        v.mentioned = self.tickets.mentions.lines(&self.tickets.m, &agent, &|u| docket_machine::label::ticket(&self.tickets.m, u, &crate::label::read), &v.id);
        v.mentions = self.tickets.mentions.held(&self.tickets.m, &agent, t);
        self.fill_prompts(t, &mut v);
        self.fill_unsent(t, &mut v);
        v.lapses_at = self.lapses_at(t);
        // and of the handlers it carries: when his held burst goes by itself (E1b, `gather`; the held mark's "goes at")
        for hv in v.handlers.iter_mut() {
            if let Ok(u) = ticket_of(&self.tickets.m, &hv.id) {
                hv.lapses_at = self.lapses_at(u);
            }
        }
        // what follows from it, said once here for every reader (proto-reader N-1446 p136, proto-lead p140): a step of
        // his that goes on by itself is not his to answer, so it is out of `waits_on`, `owed` is false, and its outline
        // line says when it goes through; threads.rs, Needs you and the margin read what is sent
        self.lapse_shown(&mut v);
        for hv in v.handlers.iter_mut() {
            self.lapse_shown(hv);
        }
        if let Some(o) = v.outline.as_mut() {
            self.lapse_outline(o);
        }
        Some(v)
    }

    /// V's waits on the owner without his steps that go on by themselves (`lapses_at`; the machine's `owner_steps` says
    /// which line each is): those lines out of `waits_on`, and `owed` read again off what is left, as the machine reads
    /// it. Under slow mode none goes by itself, so nothing changes.
    fn lapse_shown(&self, v: &mut TicketView) {
        let Some(x) = ticket_of(&self.tickets.m, &v.id).ok().and_then(|t| self.tickets.m.tickets.get(&t)) else { return };
        let steps = self.tickets.m.owner_steps(x);
        let lapsing: Vec<(String, String)> = steps.into_iter().filter(|(u, _, _)| self.lapses_at(*u).is_some()).map(|(_, call, named)| (named, call)).collect();
        if lapsing.is_empty() {
            return;
        }
        v.waits_on.retain(|w| !(w.agent == "owner" && w.post.is_none() && lapsing.iter().any(|(named, call)| *named == w.ticket && *call == w.call)));
        v.owed = v.waits_on.iter().any(|w| w.agent == "owner");
    }

    /// When X's wait on the owner goes through by itself: each step of his it waits on goes on by itself (the latest of
    /// their times), and nothing else waits on him (a halt, an ask).
    fn goes_through(&self, x: &docket_machine::Ticket) -> Option<String> {
        let steps = self.tickets.m.owner_steps(x);
        if steps.is_empty() || self.tickets.m.owner_waits_else(x) {
            return None;
        }
        steps.iter().map(|(u, _, _)| self.lapses_at(*u)).collect::<Option<Vec<String>>>()?.into_iter().max()
    }

    /// O's line and the lines under it: one owed only by steps of his that go on by themselves is not owed, and its
    /// state is when it goes through, on his clock ("goes through at 20:07"), in place of "waits on you".
    fn lapse_outline(&self, o: &mut docket_machine::view::ViewOutline) {
        if o.owed {
            let at = ticket_of(&self.tickets.m, &o.id).ok().and_then(|u| self.tickets.m.tickets.get(&u)).and_then(|x| self.goes_through(x));
            if let Some(at) = at.and_then(|a| chrono::DateTime::parse_from_rfc3339(&a).ok()) {
                o.owed = false;
                o.state = crate::lapse::goes_through_words(at.to_utc(), &chrono::Local);
            }
        }
        for c in o.children.iter_mut() {
            self.lapse_outline(c);
        }
    }

    /// READ ([`git_read`]) into the kept graph ([`Tickets::graph_git_add`]), and each commit it put in that an agent
    /// here wrote → what its `Item:` trailers and its subject name ([`mentions::commits_into`]), as `load_tickets` adds
    /// the posts' `named` (paper-research N-1446 p1433: without it his asks reach no commit in his window).
    pub fn graph_git_add(&mut self, read: Vec<work_repos::RepoRead>) {
        // which repositories count (p1476): the project's, and those with a commit by an agent here
        let repos = self.project.as_ref().map(|p| p.repos()).unwrap_or_default();
        let project: BTreeSet<String> = repos.iter().map(|(_, d)| d.clone()).collect();
        let here: BTreeSet<String> = self.tickets.graph_git.names(&read).into_iter().filter(|a| self.is_agent(a)).collect();
        let dirs: BTreeSet<String> = read.iter().map(|r| r.dir.clone()).collect();
        let named = self.tickets.graph_git.repos(&repos);
        let new = self.tickets.graph_git_add(read, &project, &here);
        self.trains_moved(&named, &dirs);
        if new.is_empty() {
            return;
        }
        let mut g = std::mem::take(&mut self.tickets.graph.graph);
        mentions::commits_into(&self.tickets.m, &|a| self.is_agent(a), &new, &mut g);
        self.tickets.graph.graph = g;
        self.tickets.graph.graph.keep();
    }

    /// The `merged` lines (`docket_machine::MERGED`; N-1511 p43, paper-research N-1446 p2139, 1, p2152) of the
    /// repositories the kept graph counts (p1476: the project's, and one an agent here wrote in, as research's
    /// soft-swarm), REPOS by name and folder, whose refs this round read (DIRS): one for each `train-` branch whose
    /// tip is not where its last line says, or with none, where this coordd last read it, with what the move brought
    /// ([`GraphGit::brought`]). At a repository's first read since the start, a branch no line names is taken as it
    /// stands; one new after it brings its tip alone. A line the machine refuses is said and left: the next move of its
    /// branch tries again.
    fn trains_moved(&mut self, repos: &[(String, String)], dirs: &BTreeSet<String>) {
        let counted: Vec<&(String, String)> = repos.iter().filter(|(_, d)| dirs.contains(d) && self.tickets.graph_git.counted.contains(d)).collect();
        for (name, dir) in counted {
            let g = &mut self.tickets.graph_git;
            let Some(refs) = g.refs.get(dir) else { continue };
            let heads = refs.lines().filter_map(|l| l.split_once(' ')).filter_map(|(h, r)| Some((r.strip_prefix("refs/heads/")?.to_string(), h.to_string())));
            let now: BTreeMap<String, String> = heads.filter(|(b, _)| b.starts_with(TRAIN)).collect();
            let first = !g.trains.contains_key(dir);
            let before = g.trains.insert(dir.clone(), now.clone()).unwrap_or_default();
            for (branch, tip) in now {
                let from = match self.tickets.m.seen.tips.get(&format!("{name}/{branch}")).or(before.get(&branch)) {
                    Some(f) if *f == tip => continue,
                    Some(f) => Some(f.clone()),
                    None if first => continue,
                    None => None,
                };
                let Some(value) = self.tickets.graph_git.brought(name, &branch, &tip, from.as_deref()) else { continue };
                if let Err(e) = self.ticket_input(Input::Event { name: docket_machine::MERGED.into(), value }) {
                    eprintln!("coordd: tickets: {name}/{branch} at {tip}: {e}");
                }
            }
        }
    }

    /// A round of git's side, read off the lock on the side's clock (`read_sides`, paper-research N-1446 p1476), into
    /// the kept graph: ROUND's folders placed, the jobs from before the record (their commits count a repository) when
    /// BEFORE has them, ROUND's read, then those jobs. A round taken before the record was loaded again puts nothing:
    /// the next reads the new graph's whole.
    pub fn graph_git_put(&mut self, round: work_repos::GitRead, before: Option<Vec<docket_machine::graph_git::Job>>) {
        if round.epoch != self.tickets.graph_git.epoch {
            return;
        }
        self.tickets.graph_git.keep_placed(round.placed);
        if let Some(b) = &before {
            self.tickets.graph_git.take_jobs(b);
        }
        self.graph_git_add(round.read);
        if let Some(b) = before {
            self.tickets.graph_jobs_add(&b);
        }
    }

    /// The facts decision NAME is handed beside its call fields, ARG giving each of its arguments by name (a pattern's
    /// `decide`'s, a `when` binding's ask's), with the policy's places: the one place a decision's facts are gathered,
    /// under the lock, for [`decide_in`] (proto-mind N-1446 p1592: what (10) hands `cut-due` goes here too). None when
    /// it is handed none, and the coord's engine decides it as before.
    ///
    /// (1) (proto-mind p1502; paper-research p1495, p1519, p1559; proto-mind p1541): for each of its `Ref` parameters
    /// given as one commit, [`docket_machine::graph_git::facts`] from the kept graph, git's commits as last read (their
    /// trees and files) and their repositories' packages, with what production's coordd runs: the commit of the last
    /// switch or return of this coord whose release act's checkout is the one coordd's own release is in
    /// (`live_commit`). None of them when no such commit was read (a decision on no commit, or on one the graph has not
    /// read). Fresh to the side's last round of git (5 s).
    ///
    /// (10) (proto-mind p1502, p1615; paper-research p1505, p1616): for a decision a `when D is o` binding asks (D, a
    /// rule of the policy's with no call of its own, `cut-due`), where the open tickets stand now
    /// ([`docket_machine::Machine::standing`]: STANDS, STANDS-WITH, HANDS-IN), which a project that reads them declares
    /// in its own policy. Beside (1)'s when its fields name a commit, and (1)'s for each commit the tickets stand with,
    /// a `Ref` argument of the step or hold they wait at (a head's `commit` at `merge` and `cut_of`), so a rule reads what
    /// a standing head changes (draft 2's `engine-head`; proto-mind N-1446 p1765).
    ///
    /// A role's step is handed (1)'s too, for its own call's `Ref` arguments ([`Core::ref_args`], [`Core::commits_facts`],
    /// the same two this gathers them by), so `fills` may rule on what a commit it is given changes (paper-research
    /// N-1446 p2954: who merges a head).
    fn decision_facts(&self, name: &str, arg: &dyn Fn(&str) -> Option<String>) -> Option<Situation> {
        let calls = &self.tickets.m.defs.calls;
        let mut commits = self.ref_args(&format!("{}{name}", docket_machine::syntax::DECIDE), arg);
        let standing = self.tickets.m.defs.bindings.iter().any(|b| matches!(&b.every, docket_machine::syntax::Every::When { decision, .. } if decision == name)).then(|| self.tickets.m.standing());
        if let Some(s) = &standing {
            let at: BTreeMap<&str, &str> = s.iter().filter(|(f, _)| f == "stands").map(|(_, a)| (a[0].as_str(), a[1].as_str())).collect();
            let is_ref = |t: &str, p: &str| at.get(t).and_then(|c| calls.get(*c)).is_some_and(|sig| sig.params.iter().any(|(q, ty)| q == p && *ty == Type::Ref));
            commits.extend(s.iter().filter(|(f, a)| f == "stands-with" && is_ref(&a[0], &a[1])).map(|(_, a)| a[2].clone()));
            commits.sort();
            commits.dedup();
        }
        let mut given = self.commits_facts(&commits);
        given.extend(standing.into_iter().flatten());
        // the facts the record's events made (effects.rs), to a decision whose rules read them: a program reads them
        // with `decide` (move 6, piece 2)
        if self.tickets.effects.rules.readers.contains(name) {
            given.extend(self.tickets.effects.facts());
        }
        (!given.is_empty()).then(|| (given, self.places()))
    }

    /// The commits CALL's `Ref` parameters are given, one commit each, ARG giving each argument by name: what a decision
    /// (`decide D`, CALL its `decide D` signature; [`Core::decision_facts`]) or a role's step (`fills`, CALL the step's
    /// own) is on. None for a call the project does not have.
    fn ref_args(&self, call: &str, arg: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
        let sig = self.tickets.m.defs.calls.get(call);
        sig.into_iter().flat_map(|c| &c.params).filter(|(_, ty)| *ty == Type::Ref).filter_map(|(p, _)| arg(p.as_str())).collect()
    }

    /// (1)'s facts ([`Core::decision_facts`]) for each of COMMITS: [`docket_machine::graph_git::facts`] from the kept
    /// graph, with the commit production's coordd runs (`live_commit`); none for a commit the graph has not read.
    fn commits_facts(&self, commits: &[String]) -> Vec<(String, Vec<String>)> {
        use docket_machine::graph_git::{facts, live_commit};
        if commits.is_empty() {
            return Vec::new();
        }
        let (g, graph) = (&self.tickets.graph_git, &self.tickets.graph.graph);
        let coord = std::path::Path::new(&self.coord).file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let checkout = crate::own_release().and_then(|r| Some(r.parent()?.parent()?.display().to_string()));
        let live = checkout.and_then(|c| live_commit(graph, coord, &c).map(str::to_string));
        commits.iter().flat_map(|c| facts(graph, &g.commits, &g.packages, c, live.as_deref())).collect()
    }

    /// The repositories the kept graph reads git from (paper-research N-1446 p1455, [`work_repos`]): the project's,
    /// then each one the folders where agents work sit in, whether its commits count or not (a test's).
    #[cfg(test)]
    pub fn graph_repos(&self) -> Vec<(String, String)> {
        self.tickets.graph_git.repos(&self.project.as_ref().map(|p| p.repos()).unwrap_or_default())
    }

    /// What the side's clock reads git by off the lock ([`work_repos::GitReading::read`]): the folders not placed yet,
    /// the project's repositories and the refs as last read.
    pub fn graph_git_reading(&self) -> work_repos::GitReading {
        self.tickets.graph_git.reading(self.project.as_ref().map(|p| p.repos()).unwrap_or_default())
    }

    /// A round of the agents' side (`follow::KeptSide`, paper-research N-1446 p1458): each session a `given` names for
    /// an agent here ([`Core::is_agent`]; never another project's, PROJECT/NAME, whose given makes no session,
    /// `KeptSide::take`, p1828), out to be read on off the lock (`follow::SideRound::read`) and put back
    /// ([`Core::side_put`]).
    pub fn side_round(&mut self) -> crate::follow::SideRound {
        let agents = self.tickets.side.agents().filter(|a| self.is_agent(a)).map(String::from).collect();
        self.tickets.side.round(&agents)
    }

    /// ROUND, read, into the kept graph.
    pub fn side_put(&mut self, round: crate::follow::SideRound) {
        let Tickets { side, graph, .. } = &mut self.tickets;
        side.put(round, &mut graph.graph);
        // the kept index caught up with what the round put in, here, not at the next read (p1828)
        graph.graph.keep();
    }

    /// ROUND's next part into the kept graph, about MOST edges (`follow::KeptSide::put_part`), the kept index caught up
    /// with it: whether ROUND is all in. coordd's clock puts a round so, each part under a hold of the lock of its own
    /// (main.rs `put_side`, with `follow::SIDE_PART`; paper-research N-1446 p1922 (a)).
    pub fn side_put_part(&mut self, round: &mut crate::follow::SideRound, most: usize) -> bool {
        let Tickets { side, graph, .. } = &mut self.tickets;
        let done = side.put_part(round, &mut graph.graph, most);
        graph.graph.keep();
        done
    }

    /// Whether NAME is an agent as `coord label` says so: in the registry, holding a role, or made in this record.
    fn is_agent(&self, name: &str) -> bool {
        self.agents.contains_key(name) || !self.tickets.m.roles_of(name).is_empty() || self.tickets.m.made_in(name).is_some()
    }

    /// `coord mentions ID` (threads piece 3): the posts whose words name ID — a ticket (it or one of its posts), a post
    /// `N-412.p30`, an agent by NAME or `@NAME` — oldest first, as JSON lines of `ViewMention`; none for an id that is
    /// not there.
    pub fn ticket_mentions(&self, id: &str) -> Reply {
        let id = id.trim().trim_start_matches('@');
        if id.is_empty() {
            return err("coord mentions: which id? A ticket, a post (N-412.p30) or an agent".into());
        }
        let agent = |n: &str| self.is_agent(n);
        let lines = self.tickets.mentions.lines(&self.tickets.m, &agent, &|u| docket_machine::label::ticket(&self.tickets.m, u, &crate::label::read), id);
        ok(lines.iter().filter_map(|l| serde_json::to_string(l).ok()).collect::<Vec<_>>().join("\n"))
    }

    /// V's (ticket T's) runner steps held back unsent because the runner's command could not start (`runner_unsent`):
    /// the step, its call and worker, why and since when (the first failure), and how many tries could not start
    /// (docket-edge N-236; paper-research N-101 p448).
    fn fill_unsent(&self, t: u64, v: &mut TicketView) {
        v.unsent = self.runner_unsent_of(t);
    }

    /// The give halts of V (ticket T) and of the handler tickets it carries (`handlers`, shown nowhere else: the
    /// watcher's give on an agent_post ticket, docket-edge N-88.p4) with the text that would be sent.
    fn fill_prompts(&self, t: u64, v: &mut TicketView) {
        for h in v.halts.iter_mut() {
            if let docket_machine::ViewHalt::Give { n, prompt, edited: false, .. } = h {
                if let Some((call, mut args, template, edit, refs)) = self.tickets.m.halted_give(t, *n) {
                    self.agents_into(t, &mut args);
                    *prompt = if call == "asks" {
                        // the watcher's asks step makes no call: coordd answers it Nothing (`clerk_job`)
                        String::from("(nothing is sent: coordd answers this step Nothing, with no call)")
                    } else {
                        crate::render::step_prompt(&self.places(), &call, &args, template.as_deref(), edit.as_deref(), &refs).unwrap_or_else(|e| format!("(no prompt: {e})"))
                    };
                }
            }
        }
        for hv in v.handlers.iter_mut() {
            if let Ok(u) = ticket_of(&self.tickets.m, &hv.id) {
                self.fill_prompts(u, hv);
            }
        }
    }

    /// The tickets for a snapshot: the open ones and those closed in the last day.
    pub fn ticket_views(&self) -> Vec<TicketView> {
        self.ticket_views_where(|_| true)
    }

    /// The views of `ticket_views` whose ticket also meets KEEP, the others not built (`Core::ticket_waits`, N-459).
    pub fn ticket_views_where(&self, keep: impl Fn(&docket_machine::Ticket) -> bool) -> Vec<TicketView> {
        self.ticket_view_ids(keep).into_iter().filter_map(|t| self.ticket_shown(t)).collect()
    }

    /// The tickets `ticket_views_where` shows, in its order, their views not built.
    pub fn ticket_view_ids(&self, keep: impl Fn(&docket_machine::Ticket) -> bool) -> Vec<u64> {
        let day_ago = {
            let t = std::time::SystemTime::now() - std::time::Duration::from_secs(86_400);
            let secs = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            crate::core::at_secs(secs as i64)
        };
        // an item ticket always, closed or not: the board takes its items' closed from them (E5), and an item closed
        // long ago (Tovrin's, imported closed on 3 Nov) is still closed (docket-cutover dry run, 11 Nov: 49 closed items
        // showed open on the board because their tickets were older than a day). A ticket a handler runs in (design §13)
        // is never shown: its parent's view carries it (`handlers`), for the clarify chip
        self.tickets.m.tickets.values().filter(|x| x.is_open() || x.kind == "item" || x.closed_at.as_ref().is_some_and(|c| *c >= day_ago)).filter(|x| x.handling.is_none()).filter(|x| keep(x)).map(|x| x.t).collect()
    }

    // ---------------------------------------------------------------- requests

    pub fn ticket_kind(&self, kind: &str) -> Reply {
        let Some(k) = self.tickets.m.defs.kinds.get(kind) else {
            return err(format!("There is no kind `{kind}`. The kinds are: {}.", self.tickets.m.defs.kinds.keys().cloned().collect::<Vec<_>>().join(", ")));
        };
        let form = k.form.iter().map(|(n, t)| format!("  {n}: {t}")).collect::<Vec<_>>().join("\n");
        ok(format!(
            "A {kind} takes:\n{form}\nIt ends with {}{}. At most {} steps.\nOpen one with: coord open {kind} {} [TEXT, a first post]\n\n{}",
            k.result,
            if k.raises.is_empty() { String::new() } else { format!(", or fails with {}", k.raises.join(", ")) },
            docket_machine::check::bound(&k.pattern, &self.tickets.m.defs.kinds),
            k.form.iter().map(|(n, _)| format!("--{n} V")).collect::<Vec<_>>().join(" "),
            k.text
        ))
    }

    pub fn ticket_kinds(&self) -> Reply {
        let defs = &self.tickets.m.defs;
        let all: Vec<Value> = defs
            .kinds
            .values()
            .map(|k| {
                let v = json!({
                    "kind": k.name, "letter": k.letter.or_else(|| k.name.chars().next()).map(|c| c.to_ascii_uppercase().to_string()),
                    "form": k.form, "result": k.result, "raises": k.raises,
                    "bound": docket_machine::check::bound(&k.pattern, &defs.kinds), "text": k.text,
                    // the bound is per round of an iterate with no count (N-101.p33)
                    "per_round": docket_machine::check::rounds_unbounded(&k.pattern),
                    // its `///` doc and its form fields' (the open form names a kind and its fields by them)
                    "doc": k.doc.text, "docs": k.doc.parts,
                    // where it is written, whose own program it is (`kinds/agents/NAME.kind`), and the bindings that
                    // open it as written: what the Docket's page draws what starts each program from (0.2 line 26)
                    "file": k.file, "by": docket_machine::view::written_by(&k.file),
                    "bindings": defs.bindings.iter().filter(|b| b.kind == k.name).map(|b| b.text.as_str()).collect::<Vec<_>>(),
                    // the calls its steps name, as read, with the model of a call coordd runs (paper-research N-1396
                    // p174, p235): solved off the lock, only read here (consult-3 p237)
                    "calls": kind_calls(&self.tickets.call_files, &self.task_models.by_task, &self.tickets.m.calls_edited, std::iter::once(&k.pattern).chain(k.handlers.iter().map(|h| &h.body))),
                });
                // its last live edit, from the record (`Line::Reload`): a restart shows the same
                with_edit(v, self.tickets.m.kinds_edited.get(&k.name))
            })
            .collect();
        ok(Value::Array(all).to_string())
    }

    /// UNCHECKED: why `may` could not ask the policy, written before the act (`Line::Unchecked`).
    pub fn ticket_open(&mut self, who: &Who, kind: String, under: Option<String>, form: Value, text: Option<String>, said: Said) -> Reply {
        let Said { unchecked, told, .. } = said;
        let under = match under.map(|u| ticket_of(&self.tickets.m, &u)).transpose() {
            Ok(u) => u,
            Err(e) => return err(e),
        };
        if let Some(why) = unchecked {
            let _ = self.ticket_input(Input::Unchecked { act: "open".into(), by: who.name(), t: under, kind: kind.clone(), why });
        }
        let text = text.filter(|t| !t.trim().is_empty());
        let names = text.as_deref().map(|t| named_in(t, &|n| n == "owner" || self.agents.contains_key(n))).unwrap_or_default();
        if let Err(e) = self.may_name(&who.name(), &names, None) {
            return err(format!("coord open: {e}"));
        }
        if let Some(k) = self.tickets.m.defs.kinds.get(&kind) {
            if let Err(e) = crate::runner::one_value_fields(&k.form, &form) {
                return err(e);
            }
        }
        // an item takes a number above every note in the notes folder too, which agents also write by hand (orch m14266)
        let input = match self.item_number().filter(|n| kind == "item" && under.is_none() && *n > self.tickets.m.next_number()) {
            Some(t) => Input::Import { t, kind, opener: who.name(), form, text: text.map(Text::New), closed: None },
            None => Input::Open { kind, under, opener: who.name(), form, text: text.map(Text::New), names, told },
        };
        match self.ticket_input(input) {
            Ok(r) => ok(r),
            Err(e) => err(e),
        }
    }

    /// `coord esc NAME[,NAME...]` (owner, 13 Nov; paper-research N-36): stop each named agent's running turn, and do
    /// nothing else: no text, no ticket, no post. One in a turn gets the Escape (Effect::Interrupt, through its holder;
    /// Codex: interrupt) and an `esc` line on the record says who stopped it; one out of a turn has nothing typed and
    /// nothing written, but one held on a rate limit is not given that turn again (`Core::limit_dropped`, proto-lead
    /// N-1397 p189). Whatever is sent to it meanwhile waits until the stop has landed (`stopping`). Who may: the
    /// norms' act `esc` (tickets.eflint; the owner always), every name checked before any is stopped.
    pub fn ticket_esc(&mut self, who: &Who, to: Vec<String>) -> Reply {
        let mut names: Vec<String> = Vec::new();
        for n in to.iter().flat_map(|s| s.split(',')).map(str::trim).filter(|s| !s.is_empty()) {
            let n = match self.not_across("esc", n) {
                Ok(n) => n,
                Err(e) => return err(e),
            };
            if !names.contains(&n) {
                names.push(n);
            }
        }
        if names.is_empty() {
            return err("usage: coord esc NAME[,NAME...]".into());
        }
        if let Some(n) = names.iter().find(|n| !self.agents.contains_key(*n)) {
            return err(format!("coord esc: no agent named {n}"));
        }
        for n in &names {
            if let Err(why) = self.may_esc(who, n) {
                return err(format!("coord esc: {why}"));
            }
        }
        let mut said = Vec::new();
        for a in names {
            let busy = self.agents.get(&a).is_some_and(|x| x.live.state == "working" && x.ended_turn != x.live.turn);
            if !busy {
                if self.limit_dropped(&a, &format!("{}'s esc", who.name())) {
                    said.push(format!("{a} is not in a turn; the turn it was held for on a rate limit will not be given"));
                } else {
                    said.push(format!("{a} is not in a turn"));
                }
            } else if self.tickets.stopping.contains(&a) {
                said.push(format!("{a}'s turn is already being stopped"));
            } else {
                if let Err(e) = self.ticket_input(Input::Esc { by: who.name(), agent: a.clone(), also: Vec::new(), all: false }) {
                    return err(format!("coord esc: {e}"));
                }
                said.push(format!("{a}'s turn is being stopped"));
            }
        }
        self.poke.notify_one();
        ok(said.join("; "))
    }

    /// Whether INPUT, the owner's post or message, names or tells an agent in Step (`Agent::control`): `names` and
    /// `told` of a post, `names`, `typed` and `to` of a message's event.
    fn to_step_agent(&self, input: &Input) -> bool {
        let step = |n: &str| self.agents.get(n).is_some_and(|a| a.control == coord_types::AgentMode::Step);
        match input {
            Input::Post { names, told, .. } => names.iter().chain(told).any(|n| step(n)),
            Input::Event { value, .. } => {
                let strs = |k: &str| -> Vec<String> {
                    match value.get(k) {
                        Some(Value::String(s)) => vec![s.clone()],
                        Some(Value::Array(xs)) => xs.iter().filter_map(Value::as_str).map(str::to_string).collect(),
                        _ => Vec::new(),
                    }
                };
                ["names", "typed", "to"].iter().flat_map(|k| strs(k)).any(|n| step(&n))
            }
            _ => false,
        }
    }

    /// `Request::Esc` with STEP (N-1788): the stop as `ticket_esc`, and each named agent put in Step in the same
    /// hold of the lock (`Core::set_agent_mode`), so no turn is handed to it between the two. The owner's; nothing is
    /// changed when the stop is refused. Without STEP, `ticket_esc`.
    pub fn ticket_esc_step(&mut self, who: &Who, to: Vec<String>, step: bool) -> Reply {
        if !step {
            return self.ticket_esc(who, to);
        }
        if !matches!(who, Who::Owner) {
            return err("coord esc --step: only the owner puts an agent in Step".into());
        }
        let names: Vec<String> = to.iter().flat_map(|s| s.split(',')).map(str::trim).filter(|s| !s.is_empty()).map(|n| self.local_name(n)).collect();
        let r = self.ticket_esc(who, to);
        if !r.ok {
            return r;
        }
        let mut said = vec![r.out];
        for n in names {
            let m = self.set_agent_mode(who, &n, coord_types::AgentMode::Step);
            if !m.ok {
                return err(format!("{}; {}", said.join("; "), m.out));
            }
            said.push(m.out);
        }
        ok(said.join("; "))
    }

    /// `Request::DeliverUrgent` (N-1788): the owner's posts waiting for AGENT go to it now, as one message
    /// (docket-machine's `urgent`, on the record). Without INTERRUPT into its running turn, after its running tool call
    /// (Claude Code takes a message mid-turn after the tool's result), nothing stopped; with INTERRUPT its turn is
    /// stopped as `coord esc` stops it and the message goes once the stop has landed. Refused, with the reason and
    /// nothing changed (his posts still wait, none dropped): not the owner's, no such agent, an agent the delivery
    /// program does not hand to, nothing of his waiting for it.
    pub fn deliver_urgent(&mut self, who: &Who, agent: &str, interrupt: bool) -> Reply {
        if !matches!(who, Who::Owner) {
            return err("deliver urgent: the owner's".into());
        }
        let agent = self.local_name(agent);
        let Some(a) = self.agents.get(&agent) else { return err(format!("deliver urgent: no agent {agent}")) };
        if !self.runs_delivery(a) {
            return err(format!("deliver urgent: {agent} is not handed to by the delivery program (a {} agent); his posts to it wait as they were", a.kind));
        }
        let busy = a.live.state == "working" && a.ended_turn != a.live.turn;
        let said = match self.ticket_input(Input::Urgent { agent: agent.clone(), by: who.name() }) {
            Ok(s) => s,
            Err(e) => return err(format!("deliver urgent: {e}")),
        };
        if interrupt && busy {
            let r = self.ticket_esc(who, vec![agent.clone()]);
            if !r.ok {
                return err(format!("{said} Its turn was not stopped: {}; the message goes at the end of its running tool call.", r.out));
            }
            return ok(format!("{said} {}; it goes once the stop has landed.", r.out));
        }
        self.poke.notify_one();
        ok(if busy { format!("{said} It goes into its running turn, after its running tool call.") } else { format!("{said} It is not in a turn: it goes now.") })
    }

    /// `coord esc --all`, the window's Esc all (line 200; the owner, 10 Nov: "i reallyt need them to stop … send ana esc
    /// again super easy task jsut do it"; plan N-1324): one Escape to every agent of this project in a turn at the
    /// press, Claude Code's and Codex's (the kinds an Escape stops), and nothing else, as `coord esc` is for one. One
    /// `esc` line for the press names them all (`also`, `all`); with none in a turn nothing is typed or written. Each
    /// is `stopping` until its Escape is written, as after one esc, and no more: no hand-over is held and a turn that
    /// begins after the press runs (paper-research N-1324 p2: a hold one stuck session could keep open would put his
    /// own words behind it). Keeping them stopped is another act (line 327). An agent held on a rate limit out of a
    /// turn is not given that turn again (`Core::limit_dropped`): its resume continues a turn from before the press,
    /// stopped as a running one is (proto-lead N-1397 p196, p201). Who may: `may_esc` on each, every one checked
    /// before any is stopped; an agent asking is not among them (its own turn asks).
    pub fn ticket_esc_all(&mut self, who: &Who) -> Reply {
        // an agent asking is in its own turn: that one is not among them
        let me = match who {
            Who::Agent(me) => Some(me.as_str()),
            Who::Owner => None,
        };
        let in_turn: Vec<String> = self.agents.values().filter(|a| matches!(a.kind.as_str(), "claude" | "codex") && self.in_turn(a) && Some(a.name.as_str()) != me).map(|a| a.name.clone()).collect();
        // held on a rate limit, out of a turn: the turn its resume would go on with is stopped too
        let held: Vec<String> = self.agents.values().filter(|a| a.live.limit.is_some() && !self.in_turn(a) && Some(a.name.as_str()) != me).map(|a| a.name.clone()).collect();
        for n in in_turn.iter().chain(&held) {
            if let Err(why) = self.may_esc(who, n) {
                return err(format!("coord esc --all: {why}; nobody was stopped"));
            }
        }
        let (already, names): (Vec<String>, Vec<String>) = in_turn.into_iter().partition(|a| self.tickets.stopping.contains(a));
        let mut said = Vec::new();
        if let Some((agent, also)) = names.split_first() {
            let input = Input::Esc { by: who.name(), agent: agent.clone(), also: also.to_vec(), all: true };
            if let Err(e) = self.ticket_input(input) {
                return err(format!("coord esc --all: {e}"));
            }
            said.push(format!("being stopped: {}", names.join(", ")));
        }
        if !already.is_empty() {
            said.push(format!("already being stopped: {}", already.join(", ")));
        }
        let dropped: Vec<String> = held.into_iter().filter(|a| self.limit_dropped(a, &format!("{}'s Esc all", who.name()))).collect();
        if !dropped.is_empty() {
            said.push(format!("not given again the turn a rate limit held: {}", dropped.join(", ")));
        }
        if said.is_empty() {
            return ok("No agent is in a turn: nothing was stopped.".into());
        }
        self.poke.notify_one();
        ok(said.join("; "))
    }

    /// `coord hold-mail on|off`, the window's Hold and Release (line 327; the owner, 10 Nov: "keep them interupted /
    /// esc'ed so i can be the one to sedn them messages"; plan N-1324 post 17): the owner's hold on agents' mail, one
    /// `mail_hold` line (MAIL_HOLD) each way. The machine keeps the fact, each ticket's mark taken as it goes on, and
    /// says what it keeps (`Machine::mail_held`); what is handed meanwhile is the delivery program's to decide
    /// (`delivery.kind`'s hold arms: kept with the hold's words). Release lets what it kept be decided again
    /// (`Machine::hold_lifted`): an agent it still keeps a post from, below its read mark, is owed a `free`
    /// (`desk_released`), raised by the round the poke starts. A Hold while held, or a Release with none, writes
    /// nothing and says so. The owner's alone: no act of the norms lets an agent.
    pub fn ticket_mail_hold(&mut self, who: &Who, on: bool) -> Reply {
        if !matches!(who, Who::Owner) {
            return err("coord hold-mail: holding agents' mail is the owner's".into());
        }
        let held = match (&self.tickets.m.mail_hold, on) {
            (Some(h), true) => return ok(format!("Agents' mail is held already, since {}.", h.at)),
            (None, false) => return ok("Agents' mail is not held.".into()),
            (Some(h), false) => self.desk_held(&h.why()),
            (None, true) => Vec::new(),
        };
        match self.ticket_input(Input::Event { name: docket_machine::MAIL_HOLD.into(), value: json!({"by": who.name(), "on": on}) }) {
            Ok(r) => {
                if !on {
                    self.desk_released(&held);
                }
                // the window's Hold and its muted line read the snapshot's `mail_hold`: a new one carries it
                let snap = self.snapshot();
                self.emit(snap.body);
                self.poke.notify_one();
                ok(r)
            }
            Err(e) => err(format!("coord hold-mail: {e}")),
        }
    }

    /// The window's Brain switch on AGENT's card (`Request::Brain`; proto-lead N-1446 p857.2): his switch of AGENT's
    /// brain, one `brain_switch` line (BRAIN_SWITCH) each way, as his hold on agents' mail is (`ticket_mail_hold`). The
    /// machine keeps it (`Machine::brain_switched`); while it is off the binding that opens AGENT's brain does not fire,
    /// and stays in the project's kind file: coordd writes no kind file. A press that changes nothing writes nothing and
    /// says so. The owner's alone.
    pub fn ticket_brain(&mut self, who: &Who, agent: &str, on: bool) -> Reply {
        if !matches!(who, Who::Owner) {
            return err("coordd: only the owner switches an agent's brain".into());
        }
        if !self.agents.contains_key(agent) {
            return err(format!("coordd: there is no agent {agent}"));
        }
        let word = if on { "on" } else { "off" };
        if self.tickets.m.brain_is_on(agent) == on {
            return ok(format!("{agent}'s brain is {word} already."));
        }
        match self.ticket_input(Input::Event { name: docket_machine::BRAIN_SWITCH.into(), value: json!({"by": who.name(), "agent": agent, "on": on}) }) {
            Ok(r) => {
                // its card reads the agent's `brain`: a new snapshot carries it
                let snap = self.snapshot();
                self.emit(snap.body);
                ok(r)
            }
            Err(e) => err(format!("coordd: {e}")),
        }
    }

    /// His sign-off of his ask POST (`N-1104.p113`) with his word of worth WORTH (`Request::SignOff`; paper-research
    /// N-1446 p1786): one `signed_off` line (SIGNED_OFF), as his switch of an agent's brain is one `brain_switch` line
    /// (`ticket_brain`); the machine refuses a post that is no ask of his and a word that is none of its own. A press
    /// that changes nothing writes nothing and says so. The owner's alone.
    pub fn ticket_sign_off(&mut self, who: &Who, post: &str, worth: &str) -> Reply {
        if !matches!(who, Who::Owner) {
            return err("coordd: only the owner signs off his asks".into());
        }
        let Some((t, n)) = docket_machine::post_ref(post).and_then(|(t, n)| Some((self.tickets.m.ticket_named(t)?, n))) else {
            return err(format!("coordd: there is no post {post}"));
        };
        match (self.tickets.m.signed_off(t, n), worth) {
            (None, "none") => return ok(format!("{post} is not signed off.")),
            (Some(w), _) if w == worth => return ok(format!("{post} is signed off {w} already.")),
            _ => {}
        }
        match self.ticket_input(Input::Event { name: docket_machine::SIGNED_OFF.into(), value: json!({"by": who.name(), "t": t, "n": n, "worth": worth}) }) {
            Ok(r) => ok(r),
            Err(e) => err(format!("coordd: {e}")),
        }
    }

    /// The release his window runs, as its hello named it, and the one coordd runs (I-1457; proto-lead N-1446 p509,
    /// p610): one `window_release` line, written only when either half differs from the last line recorded: RELEASE or
    /// coordd's own release, or the releases each one's list says it contains ([`contains_of`]). So a hello on every
    /// bridge, or a window back on the same build under the same coordd, writes nothing; an engine put in alone
    /// (`release.sh live --no-exe`) ends the bridge's stream, and the next pipe's first hello writes its line; the
    /// one-time `release.sh contains` over folders cut before it makes the next hello write the lists. OWN: the folder
    /// of coordd's own release (`own_release`), whose parent holds every release's folder; with none (a build run
    /// where it was built), the line has no engine and lists none. The owner's alone: his window's bridge sends it.
    /// SINCE: when his window became his, by its own clock (`Hello::since`): the line carries it, in UTC as the record
    /// writes a time, so the graph gives the build in his window then and not at the line's own time, which is coordd
    /// hearing of it, minutes late when the build's engine started coordd again (paper-research N-1446 p1392; p1410).
    /// It is no part of what is compared: the same build again under the same coordd writes nothing, whatever time a
    /// window started again says. One that is not a time is not written, and the answer says so.
    pub fn window_release(&mut self, who: &Who, release: &str, since: Option<&str>, own: Option<&std::path::Path>) -> Reply {
        if !matches!(who, Who::Owner) {
            return err("coordd: the release his window runs is his window's to say".into());
        }
        if !release_name(release) {
            return err(format!("coordd: not a release's name: {release}"));
        }
        let releases = own.and_then(std::path::Path::parent);
        let running = |name: &str| docket_machine::Running { release: name.to_string(), contains: releases.map(|r| contains_of(r, name)).unwrap_or_default() };
        let engine = own.and_then(|o| o.file_name()?.to_str()).filter(|n| release_name(n)).map(running);
        let now = docket_machine::WindowRelease { window: running(release), engine };
        if self.tickets.m.window_release.as_ref() == Some(&now) {
            return ok(format!("His window runs {release}, as recorded."));
        }
        let mut value = now.value();
        let at = since.map(|s| chrono::DateTime::parse_from_rfc3339(s).map(|t| t.to_utc().format("%Y-%m-%dT%H:%M:%SZ").to_string()).map_err(|_| s));
        if let Some(Ok(at)) = &at {
            value["since"] = json!(at);
        }
        match self.ticket_input(Input::Event { name: docket_machine::WINDOW_RELEASE.into(), value }) {
            Ok(r) => match at {
                Some(Err(s)) => ok(format!("{r} His window's time \"{s}\" is not a time, so the line has none.")),
                _ => ok(r),
            },
            Err(e) => err(format!("coordd: {e}")),
        }
    }

    /// Whether WHO may stop NAME's turn: the owner always; an agent as the norms' act `esc(WHO, NAME)` says (the base
    /// rule: the agents below it). A policy without the act, or one that cannot be read, falls back to the base rule
    /// (`above`), not to letting everyone stop everyone.
    fn may_esc(&self, who: &Who, name: &str) -> Result<(), String> {
        let Who::Agent(me) = who else { return Ok(()) };
        match self.norm_verdict("esc", &[me, name], "none") {
            Ok(Some(v)) if v.enabled => Ok(()),
            Ok(Some(v)) => Err(format!("the team's norms do not let you stop {name}'s turn: {}", v.why)),
            other => {
                if let Err(e) = other {
                    eprintln!("coordd: norms: esc({me}, {name}): {e}; the base rule decides");
                }
                if self.above(who, name) { Ok(()) } else { Err(format!("stopping a turn is for agents below you; {name} is not")) }
            }
        }
    }

    /// `coord msg` (S5, plan/core.md; paper-research m883): a note opened by the sender naming TO, or, with RE naming
    /// a ticket or one of its posts (`C-12`, `C-12.p7`), a post on that ticket. `--re` names the author of the post
    /// replied to (the ticket's opener for the ticket itself) with TO; `--thread` (RE and no TO) names everyone in it
    /// but the sender. RE naming an old mail opens a new note to that mail's sender (or its thread), its text
    /// beginning `re mN`; old mail is only read. ESC (an old `--esc`) is refused: stopping a turn is `coord esc`'s
    /// alone (N-36). Whom it names is held to the team's `name` act (tickets.eflint). SAID: `may` and `notify`, asked off the lock.
    #[allow(clippy::too_many_arguments)]
    pub fn ticket_msg(&mut self, who: &Who, as_host: Option<String>, to: Vec<String>, body: &str, esc: bool, re: Option<String>, quotes: Vec<coord_types::Quote>, said: Said) -> Reply {
        self.ticket_msg_origin(who, as_host, to, body, esc, re, None, quotes, said)
    }

    /// `ticket_msg`, and a message from another project (N-44): the relay, as the owner side, sends for AS_HOST
    /// PROJECT/NAME either on a thread here it already takes part in (`re`), or on the thread whose ORIGIN is that
    /// project's ticket (`P:N-12`), opened if there is none: an agent's thread, opened by the far author, the origin in
    /// its form. A name PROJECT/NAME in TO is another project's agent, carried there by `hand_far`; OWN/NAME is NAME.
    #[allow(clippy::too_many_arguments)]
    pub fn ticket_msg_origin(&mut self, who: &Who, as_host: Option<String>, to: Vec<String>, body: &str, esc: bool, re: Option<String>, origin: Option<String>, quotes: Vec<coord_types::Quote>, said: Said) -> Reply {
        self.ticket_msg_under(who, as_host, to, body, esc, re, origin, quotes, said, None, None, None)
    }

    /// The place U names for a thread to open at (`--under`, `say`'s `under`; paper-research N-447): `T.pN` or `T` (its
    /// last post), and after `:` the words of that post it was opened on, `FROM+LEN` in characters
    /// (`N-412.p30:120+64`, a selection's start and length; N-447 p71). The words are checked against the post's
    /// words and kept as they are now (`{"from", "len", "words"}`, the thread's form's `quote`); a post whose words
    /// cannot be read now (an agent's turn, its session file gone) keeps the span unchecked, with no words.
    pub fn under_place(&self, u: &str) -> Result<(u64, u32, Option<Value>), String> {
        let (place, span) = match u.split_once(':') {
            Some((p, s)) => (p, Some(s)),
            None => (u, None),
        };
        let Some((t, n)) = self.tickets.m.place_of(place) else { return Err(format!("there is no ticket or post {place} to open the thread under")) };
        let Some(span) = span else { return Ok((t, n, None)) };
        let bad = || format!("{u}: the words are FROM+LEN after the post, in characters (N-412.p30:120+64)");
        let (from, len) = span.split_once('+').ok_or_else(bad)?;
        let (from, len) = (from.trim().parse::<usize>().map_err(|_| bad())?, len.trim().parse::<usize>().map_err(|_| bad())?);
        // FROM and LEN are anyone's numbers: the end is a checked sum, never one that wraps (docket-core3, N-447 p124).
        let Some(end) = from.checked_add(len).filter(|_| len > 0) else { return Err(bad()) };
        if !place.contains(".p") {
            return Err(format!("{u}: words are of one post: name it, {place}.pN:{span}"));
        }
        let m = &self.tickets.m;
        let post = m.tickets[&t].posts.iter().find(|p| p.n == n).expect("place_of found it");
        let words = crate::label::read(&m.text(&post.text).unwrap_or_else(|| post.text.clone()));
        let quoted = match words {
            Some(w) => {
                let cs: Vec<char> = w.chars().collect();
                if end > cs.len() {
                    return Err(format!("{u}: post {n} of {} has {} characters; {from}+{len} runs past them", m.id(t), cs.len()));
                }
                Some(cs[from..end].iter().collect::<String>())
            }
            None => None,
        };
        Ok((t, n, Some(json!({"from": from, "len": len, "words": quoted}))))
    }

    /// `coord msg`, and with UNDER (`--under T[.pN]`, paper-research N-447, piece 2) a thread opened at post N of T
    /// (T's last post when none), tied there from its open (`tied_at` in its form, as `origin` is); LABEL its label.
    /// KIND: what its post on the thread `re` names is (I-1480), taken as `kind_taken` takes it; a thread it opens
    /// takes none.
    #[allow(clippy::too_many_arguments)]
    pub fn ticket_msg_under(&mut self, who: &Who, as_host: Option<String>, to: Vec<String>, body: &str, esc: bool, re: Option<String>, origin: Option<String>, quotes: Vec<coord_types::Quote>, said: Said, under: Option<String>, label: Option<String>, kind: Option<PostKind>) -> Reply {
        let tied_at = match under.as_deref() {
            None => None,
            Some(_) if re.is_some() => return err("coord msg: --under opens a thread at that post, --re posts on it: give one".into()),
            Some(u) => match self.under_place(u) {
                Ok((t, n, quote)) => Some((format!("{}.p{n}", self.tickets.m.id(t)), quote)),
                Err(e) => return err(format!("coord msg: {e}")),
            },
        };
        let label = label.map(|l| l.trim().to_string()).filter(|l| !l.is_empty());
        let from = match (who, as_host) {
            (Who::Owner, Some(h)) => self.local_name(&h),
            (_, Some(_)) => return err("coord msg: only the owner side sends for a host session".into()),
            _ => who.name(),
        };
        if esc {
            return err(ESC_APART.into());
        }
        let to: Vec<String> = to.iter().map(|n| self.local_name(n)).collect();
        let far_from = far(&from).is_some();
        if (far_from || origin.is_some() || to.iter().any(|n| far(n).is_some())) && !self.relays() {
            return err(format!("coord msg: {NOT_CARRIED}"));
        }
        if origin.is_some() && (!far_from || re.is_some()) {
            return err("coord msg: --origin is the relay's: with --as PROJECT/NAME, without --re".into());
        }
        if far_from && origin.is_none() && re.is_none() {
            return err(format!("coord msg: a message from {from} goes on a thread (--origin or --re)"));
        }
        if body.trim().is_empty() || (re.is_none() && to.is_empty()) {
            return err("usage: coord msg [--re ID] NAME[,NAME...] TEXT | coord msg --thread ID TEXT".into());
        }
        let thread = re.is_some() && to.is_empty();
        let mut text = body.trim().to_string();
        if !quotes.is_empty() {
            let q: Vec<String> = quotes.iter().map(|q| format!("{}:\n> {}", q.source, q.text.trim().replace('\n', "\n> "))).collect();
            text = format!("{}\n\n{text}", q.join("\n\n"));
        }
        // the owner answering an old mail (an open question asked before Docket): kept as a mail too (m1602)
        let mut old_answer = None;
        // where it goes, and whom it names
        let (target, mut names) = match re.as_deref() {
            // from another project: its thread here, if one came in already
            None => (origin.as_deref().and_then(|o| self.origin_thread(o)), to.clone()),
            Some(r) => match reply_of(&self.tickets.m, r) {
                Some((t, post)) => {
                    let m = &self.tickets.m;
                    let mut names = if thread { self.takers(t) } else { to.clone() };
                    if !thread {
                        let x = &m.tickets[&t];
                        let author = match post {
                            Some(n) => x.posts.get(n as usize - 1).map(|p| p.by.clone()),
                            None => x.opener.clone().or_else(|| self.takers(t).into_iter().next()),
                        };
                        names.extend(author.filter(|a| !is_relay(a)));
                    }
                    (Some(t), names)
                }
                None => {
                    let Some(old) = self.find_mail(r).cloned() else { return err(format!("coord msg: no ticket, post or mail {r}")) };
                    let mut names = if thread { self.root_of(r).map(|root| self.participants(&root)).unwrap_or_default() } else { to.clone() };
                    if !thread {
                        names.push(old.from.clone());
                    }
                    if matches!(who, Who::Owner) && from == "owner" {
                        old_answer = Some((r.to_string(), body.trim().to_string(), quotes.clone()));
                    }
                    text = format!("re {r}\n\n{text}");
                    (None, names)
                }
            },
        };
        // a far agent posts only on a thread it is on (N-44)
        if far_from && origin.is_none() {
            match target {
                Some(t) if self.tickets.m.participants(t).contains(&from) => {}
                Some(t) => return err(format!("coord msg: {from} does not take part in {}", self.tickets.m.id(t))),
                None => return err(format!("coord msg: no ticket {}", re.clone().unwrap_or_default())),
            }
        }
        let kind = match kind.map(|k| self.kind_taken(&from, k, &mut names)).transpose() {
            Ok(k) => k,
            Err(e) => return err(format!("coord msg: {e}")),
        };
        if kind.is_some() && target.is_none() {
            return err("coord msg: a kind is a post's on a thread (--re ID, --thread ID, or coord post); a thread opened takes none".into());
        }
        let mut seen = BTreeSet::new();
        names.retain(|n| *n != from && seen.insert(n.clone()));
        if names.is_empty() {
            return err(format!("coord msg: nobody to tell (the only one in {} is you)", re.unwrap_or_default()));
        }
        let bad: Vec<&String> = names.iter().filter(|n| self.named_here(n) == Err(None)).collect();
        if !bad.is_empty() {
            let all: Vec<&String> = self.agents.keys().collect();
            return err(format!("coord: no agent or host named {} (agents: {})", bad.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "), all.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" ")));
        }
        if let Err(e) = self.may_name(&from, &names, target) {
            return err(format!("coord msg: {e}"));
        }
        let Said { unchecked, told, inform } = said;
        if let Some(why) = unchecked {
            let (act, kind) = match target {
                Some(t) => ("post", self.tickets.m.tickets[&t].kind.clone()),
                None => ("open", "thread".to_string()),
            };
            let _ = self.ticket_input(Input::Unchecked { act: act.into(), by: from.clone(), t: target, kind, why });
        }
        let input = match target {
            Some(t) => Input::Post { t, by: from.clone(), text: Text::New(text), names: names.clone(), told, inform, answers: None, kind },
            // an agent's message opens a thread (design part 9b): its first post the sender's, naming them; Sent, so
            // the kind ends it at once as Between, nothing run on it
            None => {
                let mut form = sent_form(&from, &names, &text);
                // where a thread from another project came from: in its form, which the log keeps and any release
                // replays (a kind's form check reads only the fields it declares)
                if let Some(o) = &origin {
                    form["origin"] = json!(o);
                }
                // a subthread: where it stands from its open, and its label (N-447)
                if let Some((a, quote)) = &tied_at {
                    form["tied_at"] = json!(a);
                    if let Some(q) = quote {
                        form["quote"] = q.clone();
                    }
                }
                if let Some(l) = &label {
                    form["label"] = json!(l);
                }
                Input::Open { kind: "thread".into(), under: None, opener: from.clone(), form, text: Some(Text::New(text)), names: names.clone(), told }
            }
        };
        match self.ticket_input(input) {
            Ok(r) => {
                if let Some((re, body, quotes)) = old_answer {
                    let note = self.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().map(|(t, _)| self.tickets.m.id(*t)).unwrap_or_default();
                    self.keep_owner_answer(&re, &names, &body, quotes, &note);
                }
                ok(format!("{r} Told {}.", names.join(", ")))
            }
            Err(e) => err(e),
        }
    }

    /// `coord ask SUBJECT` (S5): a `question` ticket, its first line the ask and the rest its context; its pattern
    /// takes it to the lead, then up to the owner. `--on Q` says more on question Q, a post naming whoever its step
    /// awaits; an old mail question (q12) is asked anew with `re q12`. SAID: `may open question`, asked off the lock.
    pub fn ticket_ask(&mut self, who: &Who, body: &str, on: Option<String>, said: Said) -> Reply {
        let Who::Agent(me) = who else { return err("coord ask: agents ask; the owner answers on the question's ticket".into()) };
        let body = body.trim();
        if body.is_empty() {
            return err("usage: coord ask [--on Q] SUBJECT  (the full explanation on stdin)".into());
        }
        if on.is_none() {
            if let Err(e) = self.may_speak_to_owner(me) {
                return err(format!("coord ask: {e}"));
            }
        }
        let mut body = body.to_string();
        if let Some(q) = &on {
            match reply_of(&self.tickets.m, q) {
                Some((t, _)) => {
                    let x = &self.tickets.m.tickets[&t];
                    if x.kind != "question" || x.opener.as_deref() != Some(me.as_str()) {
                        return err(format!("coord ask --on: {q} is not a question of yours"));
                    }
                    let names: Vec<String> = match &x.status {
                        docket_machine::Status::Awaits { agent, .. } => vec![agent.clone()],
                        _ => Vec::new(),
                    };
                    let Said { unchecked, told, inform } = said;
                    if let Some(why) = unchecked {
                        let _ = self.ticket_input(Input::Unchecked { act: "post".into(), by: me.clone(), t: Some(t), kind: "question".into(), why });
                    }
                    return match self.ticket_input(Input::Post { t, by: me.clone(), text: Text::New(body), names, told, inform, answers: None, kind: None }) {
                        Ok(r) => ok(r),
                        Err(e) => err(e),
                    };
                }
                None if self.find_mail(q).is_some() => body = format!("re {q}\n\n{body}"),
                None => return err(format!("coord ask --on: no question {q}")),
            }
        }
        let (ask, context) = match body.split_once('\n') {
            Some((a, c)) if !c.trim().is_empty() => (a.trim().to_string(), Some(c.trim().to_string())),
            _ => (body.clone(), None),
        };
        let mut r = self.ticket_open(who, "question".into(), None, json!({"ask": ask, "context": context}), None, said);
        if r.ok {
            // asked while it holds a step: that step waits on the question (paper-research N-101 p524, 2)
            let m = &self.tickets.m;
            let held = m.tickets.range(..docket_machine::HIDDEN).next_back().and_then(|(_, x)| x.ask.as_ref().filter(|a| a.own && a.by == *me && !a.replied)).map(|a| m.id(a.held));
            r.out = match held {
                Some(h) => format!("{} It is your question on {h}'s step: that step waits on it and comes to you again with the answer.", r.out),
                None => format!("{} Its answer comes to you when it closes; go on with your work.", r.out),
            };
        }
        r
    }

    /// The ask tool's ask (block_ask; docket-edge's 8): ME asks TEXT (CONTEXT the rest) and its command waits. Ok:
    /// it waits on the ticket it holds a step of, and the answer lands in `answered[ME]` (Effect::Answered); Err: why
    /// it cannot wait (block_ask off, ME holds no step, another ask of ME's waits), and the caller asks as `coord ask`
    /// does. The command line's `coord ask` never waits.
    pub fn ticket_ask_wait(&mut self, me: &str, text: &str, context: Option<String>) -> Result<String, String> {
        self.may_speak_to_owner(me)?;
        // an answer left from before is not this one's (one that comes at once, from a handler that answers itself, is)
        self.tickets.answered.remove(me);
        self.ticket_input(Input::Ask { by: me.to_string(), text: text.to_string(), context })
    }

    /// ME's waiting ask stops waiting (its 10 minutes, or the tool call cancelled): the answer comes as a delivery.
    pub fn ticket_ask_later(&mut self, me: &str) -> Result<String, String> {
        self.ticket_input(Input::AskLater { by: me.to_string() })
    }

    pub fn ticket_answer_request(&mut self, who: &Who, t: &str, head: Value, text: Option<String>) -> Reply {
        // an edit made through Bash since the last look, recorded before the answer is taken (found.rs)
        if let Who::Agent(me) = who {
            self.look_found(me, "answer");
        }
        let t = match ticket_of(&self.tickets.m, t) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        // a step owed to the box (`step box judge(...)`, N-487) is answered by the owner's window that sent the words:
        // his answer to it is the box's, as the runner's is the runner's
        let by = match (&self.tickets.m.tickets.get(&t).map(|x| &x.status), who.name().as_str()) {
            (Some(docket_machine::Status::Awaits { agent, .. }), "owner") if agent == BOX => BOX.to_string(),
            _ => who.name(),
        };
        match self.ticket_answer(&by, t, head, Text::New(text.unwrap_or_default())) {
            Ok(r) => ok(r),
            Err(e) => err(e),
        }
    }

    /// The running agents the groups the owner's TEXT tags reach (`@team`, `@PART`): the window's rule, one for both
    /// sides (`coord_types::groups`; 0.2 B10, consult-3 N-1402.p2). An agent's own name is the agent, never a group.
    pub(crate) fn owner_groups(&self, text: &str) -> Vec<String> {
        if !text.contains('@') {
            return Vec::new();
        }
        let parts = self.parts();
        let agents: Vec<coord_types::groups::Member> = self
            .agents
            .iter()
            .map(|(n, a)| coord_types::groups::Member { name: n.as_str(), part: parts.of.get(n).map_or("", String::as_str), running: a.live.state != "stopped" })
            .collect();
        coord_types::groups::reached(text, &parts.places, &agents).into_iter().map(String::from).collect()
    }

    /// The names WHO's words TEXT give a post, by the one test (`named_here`): each `@NAME` that is a name here;
    /// `@PROJECT/NAME`, another project's agent, where the relay carries messages (N-44), refused where it does not;
    /// `@OWN/NAME` is NAME; an `@` that names nobody here is words. His `@team` and `@PART` add each running agent of
    /// the group (0.2 B10). One for his post on an open ticket and his words on a closed one (paper-research N-1481 p97).
    fn post_names(&self, who: &str, text: &str) -> Result<Vec<String>, String> {
        let mut names: Vec<String> = Vec::new();
        for n in named_in(text, &|n| self.named_here(n) != Err(None)) {
            match self.named_here(&n) {
                Ok(n) if !names.contains(&n) => names.push(n),
                Err(Some(why)) => return Err(format!("coord post: {why}")),
                _ => {}
            }
        }
        if who == "owner" {
            for n in self.owner_groups(text) {
                if !names.contains(&n) {
                    names.push(n);
                }
            }
        }
        Ok(names)
    }

    /// TO and ANSWERS are the owner's (design-two-systems §16): whom he typed the words to, and the post whose asks they
    /// answer. His words for a closed ticket are not posted there: they open a new thread that follows it (§16, item
    /// 10; paper-research m1392), typed as they were.
    #[allow(clippy::too_many_arguments)]
    /// QUOTED: the owner's reply quoting a post of T (`Request::Say` with `follows`, paper-research N-101 p474): it goes
    /// on T even when T is closed, under the post quoted (ANSWERS), instead of opening a thread that follows T.
    #[allow(clippy::too_many_arguments)]
    pub fn ticket_post(&mut self, who: &Who, t: &str, text: String, to: Option<String>, answers: Option<u32>, said: Said, lead: Option<String>, quoted: bool) -> Reply {
        self.ticket_post_typed(who, t, text, to, answers, said, lead, quoted, None)
    }

    /// `ticket_post` of a post of KIND, as its writer typed it (I-1480: `coord post --ask NAME`, the reply tool's
    /// `kind`), taken as `kind_taken` takes it; None, untyped.
    #[allow(clippy::too_many_arguments)]
    pub fn ticket_post_typed(&mut self, who: &Who, t: &str, text: String, to: Option<String>, answers: Option<u32>, said: Said, lead: Option<String>, quoted: bool, kind: Option<PostKind>) -> Reply {
        let Said { unchecked, told, inform } = said;
        let t = match ticket_of(&self.tickets.m, t) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        // an agent answers a post by its number with the reply tool (paper-research N-101.p1285, 1): the post records
        // `answers`, and names the answered post's author; only the owner's are typed to someone or quote replies
        if (to.is_some() || quoted) && who.name() != "owner" {
            return err("coord post: only the owner's posts are typed to someone".into());
        }
        if let Some(n) = answers.filter(|n| !self.tickets.m.tickets.get(&t).is_some_and(|x| x.posts.iter().any(|p| p.n == *n))) {
            return err(format!("{} has no post {n}", self.tickets.m.id(t)));
        }
        if let Some(a) = to.as_ref().filter(|a| !self.agents.contains_key(*a)) {
            return err(format!("coord post: no agent named {a}"));
        }
        if !quoted && who.name() == "owner" && self.tickets.m.tickets.get(&t).is_some_and(|x| !x.is_open()) {
            // his words there open a thread that follows it, and a thread opened takes no kind: refused, never dropped
            // (consult-3 N-1481 p51)
            if kind.is_some() {
                return err(format!("coord post: {} is closed, and his words on it open a thread, which takes no kind", self.tickets.m.id(t)));
            }
            let names = match self.post_names(&who.name(), &text) {
                Ok(n) => n,
                Err(e) => return err(e),
            };
            let (to, typed) = match to {
                Some(a) => (a.clone(), vec![a]),
                None => match lead {
                    Some(l) => (l, Vec::new()),
                    None => return err(format!("{} is closed, and nobody fills lead to take words typed to nobody: pick an agent", self.tickets.m.id(t))),
                },
            };
            return self.ticket_say(who, to, typed, names, text, Some(t));
        }
        if let Some(why) = unchecked {
            let kind = self.tickets.m.tickets.get(&t).map(|x| x.kind.clone()).unwrap_or_default();
            let _ = self.ticket_input(Input::Unchecked { act: "post".into(), by: who.name(), t: Some(t), kind, why });
        }
        let mut names = match self.post_names(&who.name(), &text) {
            Ok(n) => n,
            Err(e) => return err(e),
        };
        let kind = match kind.map(|k| self.kind_taken(&who.name(), k, &mut names)).transpose() {
            Ok(k) => k,
            Err(e) => return err(format!("coord post: {e}")),
        };
        if let Err(e) = self.may_name(&who.name(), &names, Some(t)) {
            return err(format!("coord post: {e}"));
        }
        if let Some(a) = to.filter(|a| !names.contains(a)) {
            names.insert(0, a);
        }
        // the agent's reply to post N reaches N's author
        if who.name() != "owner" {
            let author = answers.and_then(|n| self.tickets.m.tickets.get(&t)?.posts.iter().find(|p| p.n == n).map(|p| p.by.clone()));
            if let Some(a) = author.filter(|a| *a != who.name() && (a == "owner" || self.agents.contains_key(a)) && !names.contains(a)) {
                names.insert(0, a);
            }
        }
        // his post on a ticket he archived, or under one, unarchives it first, one line before the post: writing on it
        // says it matters to him again, and his words are never held (docket-core3 N-867.p27; paper-research N-867.p29)
        if who.name() == "owner" {
            if let Some(a) = self.tickets.m.archived_by(t) {
                if let Err(e) = self.ticket_input(Input::Archive { t: a, on: false, by: "owner".into() }) {
                    return err(e);
                }
            }
        }
        let id = self.tickets.m.id(t);
        match self.ticket_input(Input::Post { t, by: who.name(), text: Text::New(text), names, told, inform, answers, kind }) {
            // the reply tool's post: its own number and the one it answers (a star on it names its number)
            Ok(r) if !quoted && answers.is_some() => ok(format!("{}, under post {}.", r.trim_end_matches('.'), answers.unwrap_or_default())),
            Ok(_) if quoted => ok(match answers {
                Some(n) => format!("Posted on {id} under post {n}."),
                None => format!("Posted on {id}."),
            }),
            Ok(r) => ok(r),
            Err(e) => err(e),
        }
    }

    /// Whether agent FROM may speak to the owner first (the owner's "who may speak to you", settings.rs `speakers`):
    /// on a thread he is not on, or in a question. Ok, or what to say.
    pub(crate) fn may_speak_to_owner(&self, from: &str) -> Result<(), String> {
        if !self.agents.contains_key(from) {
            return Ok(());
        }
        let ok = match crate::settings::speakers(&self.coord, self.project.as_ref()) {
            crate::settings::Speakers::Any => true,
            crate::settings::Speakers::Leads => self.agents.get(from).is_some_and(|a| a.role == "lead"),
            crate::settings::Speakers::Named(names) => names.iter().any(|n| n == from),
        };
        let lead = self.agents.get(from).map_or("owner".to_string(), |a| a.parent.clone());
        if ok { Ok(()) } else { Err(format!("the owner's settings do not let you speak to him first; take it to your lead ({lead})")) }
    }

    /// The author of post N of ticket T, when an agent wrote it: whom a quote reply typed to nobody is to.
    pub fn ticket_quoted_author(&self, t: &str, n: Option<u32>) -> Option<String> {
        let t = ticket_of(&self.tickets.m, t).ok()?;
        let p = self.tickets.m.tickets.get(&t)?.posts.iter().find(|p| Some(p.n) == n)?;
        self.agents.contains_key(&p.by).then(|| p.by.clone())
    }

    /// Whether agent FROM may name NAMES on ticket ON (none: a new one): the team's `name` act (tickets.eflint), asked
    /// for each agent named. Ok, or what to say: who it may not name, why, and whom to take it to. The owner and coord
    /// are not held to it.
    pub(crate) fn may_name(&self, from: &str, names: &[String], on: Option<u64>) -> Result<(), String> {
        if !self.agents.contains_key(from) {
            return Ok(());
        }
        if names.iter().any(|n| n == "owner") && !on.is_some_and(|t| self.takers(t).iter().any(|n| n == "owner")) {
            self.may_speak_to_owner(from)?;
        }
        let on = on.map_or_else(|| "none".to_string(), |t| self.tickets.m.id(t));
        for n in names.iter().filter(|n| self.agents.contains_key(*n)) {
            if let Err(why) = self.permitted("name", &[from, n, &on], &on) {
                let lead = self.agents.get(from).map_or("owner".to_string(), |a| a.parent.clone());
                let at = if on == "none" { String::new() } else { format!(" on {on}") };
                return Err(format!("the team's norms do not let you name {n}{at}: {why}. Take it up with your lead ({lead}) or the owner"));
            }
        }
        Ok(())
    }

    /// A typed post's KIND as coordd takes it from FROM (I-1480): each TO it asks is one `named_here` takes, as an
    /// `@NAME` and msg's NAME are, written by its name here, never FROM, and named in NAMES, so `may_name` asks for it
    /// (a name it does not take is refused, never skipped); an ask, read or blocker asks someone,
    /// and a read names its head. A hold's ON is an item here, written by its id. So `--ask w9` is refused, never
    /// recorded as a name nobody holds (consult-3 N-1481 p35). Or what to say.
    pub(crate) fn kind_taken(&self, from: &str, kind: PostKind, names: &mut Vec<String>) -> Result<PostKind, String> {
        let is = kind.is();
        let mut to: Vec<String> = Vec::new();
        for n in kind.to() {
            let n = match self.named_here(n) {
                Ok(n) => n,
                Err(Some(why)) => return Err(why.into()),
                Err(None) => return Err(format!("no agent or host named {}", self.local_name(n))),
            };
            if n == from {
                return Err(format!("{is} names {n}, its writer: name whom it is for"));
            }
            if !names.contains(&n) {
                names.push(n.clone());
            }
            if !to.contains(&n) {
                to.push(n);
            }
        }
        Ok(match kind {
            PostKind::Ask { .. } | PostKind::Read { .. } | PostKind::Blocker { .. } if to.is_empty() => return Err(format!("{is} names nobody: name whom it asks")),
            PostKind::Read { head, .. } if head.trim().is_empty() => return Err("a read names the head to read".into()),
            PostKind::Ask { .. } => PostKind::Ask { to },
            PostKind::Read { head, .. } => PostKind::Read { to, head },
            PostKind::Blocker { .. } => PostKind::Blocker { to },
            PostKind::Hold { on } => {
                let Ok(t) = ticket_of(&self.tickets.m, &on) else { return Err(format!("a hold is on an item (I-N): there is no {on}")) };
                let x = &self.tickets.m.tickets[&t];
                if x.kind != "item" {
                    return Err(format!("a hold is on an item (I-N): {} is a {}", self.tickets.m.id(t), x.kind));
                }
                PostKind::Hold { on: self.tickets.m.id(t) }
            }
            k => k,
        })
    }

    /// Cancel (`coord cancel T WHY`): T fails as Cancelled, its open children with it. SAID: `ticket_may_edit`'s
    /// answer, asked off the lock (the default for the owner).
    pub fn ticket_cancel(&mut self, who: &Who, t: &str, why: String, said: Said) -> Reply {
        let t = match ticket_of(&self.tickets.m, t) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        if why.trim().is_empty() {
            return err("usage: coord cancel T WHY".into());
        }
        if let Some(u) = said.unchecked {
            let kind = self.tickets.m.tickets[&t].kind.clone();
            let _ = self.ticket_input(Input::Unchecked { act: "edit".into(), by: who.name(), t: Some(t), kind, why: u });
        }
        match self.ticket_input(Input::Cancel { t, by: who.name(), why }) {
            Ok(_) => ok(format!("{} cancelled.", self.tickets.m.id(t))),
            Err(e) => err(e),
        }
    }

    /// Withdraw (`coord withdraw T WHY`, the program on stdin): PROGRAM takes the place of the step T awaits, checked
    /// in that step's scope against its head; its agent is told. SAID: `ticket_may_edit`'s answer, asked off the lock.
    pub fn ticket_withdraw(&mut self, who: &Who, t: &str, program: String, why: String, said: Said) -> Reply {
        let t = match ticket_of(&self.tickets.m, t) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        if why.trim().is_empty() {
            return err("usage: coord withdraw T WHY (the program on stdin)".into());
        }
        if let Some(u) = said.unchecked {
            let kind = self.tickets.m.tickets[&t].kind.clone();
            let _ = self.ticket_input(Input::Unchecked { act: "edit".into(), by: who.name(), t: Some(t), kind, why: u });
        }
        match self.ticket_input(Input::Withdraw { t, by: who.name(), program, why }) {
            Ok(r) => ok(r),
            Err(e) => err(e),
        }
    }

    /// Repair (`coord repair T WHY`, the program on stdin): the failed ticket T runs PROGRAM, checked as its kind's
    /// pattern. Rekind (`coord rekind T KIND [--FIELD V]...`): T becomes a KIND with FORM. Both after `may edit`
    /// (SAID, asked off the lock), as Cancel and Withdraw.
    pub fn ticket_repair(&mut self, who: &Who, t: &str, program: String, why: String, said: Said) -> Reply {
        self.ticket_edit(who, t, said, |t, by| Input::Repair { t, by, program, why })
    }

    /// Edit (`coord edit T WHY`, the program on stdin): what T does with the answer it awaits, or the value of the
    /// tickets below it it waits on, `answer` naming it, from now on; the step in flight and the tickets below are left
    /// alone. TicketView.continuation is the program to start from.
    pub fn ticket_edit_program(&mut self, who: &Who, t: &str, program: String, why: String, said: Said) -> Reply {
        self.ticket_edit(who, t, said, |t, by| Input::Edit { t, by, program, why })
    }

    pub fn ticket_rekind(&mut self, who: &Who, t: &str, kind: String, form: Value, why: String, said: Said) -> Reply {
        self.ticket_edit(who, t, said, |t, by| Input::Rekind { t, by, kind, form, why })
    }

    /// The owner's once-edit of CALL's prompt on T (`Request::Template`): TEXT is read as a template of CALL, its slots
    /// the call's, before the machine takes it (Input::Template); SCOPE `step` or `tree`.
    pub fn ticket_template(&mut self, who: &Who, t: &str, call: String, scope: String, text: String, why: String) -> Reply {
        if who.name() != "owner" {
            return err("only the owner edits a prompt".into());
        }
        let t = match ticket_of(&self.tickets.m, t) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        let places = self.places();
        if let Err(e) = crate::policy::call::Calls::load(&places.call_dirs(), &places.overlay).and_then(|c| c.check_edited(&call, &text)) {
            return err(format!("your prompt for {call} is refused: {e}"));
        }
        match self.ticket_input(Input::Template { t, call, scope, text, by: who.name(), why }) {
            Ok(r) => ok(r),
            Err(e) => err(e),
        }
    }

    /// The owner has the kinds and calls read again (`Request::Reload`, after COMMIT): read as at start and taken only
    /// when all of them pass and nothing the tickets in the log stand on moved: no kind or call gone, no kind's letter
    /// changed, no form, result or raises changed of a kind that has tickets in the log, no call
    /// changed that a ticket runs as today's files have it (one from before calls on the record). Otherwise the old ones
    /// stay, and why is the answer. Recorded as `Line::Reload` with the kinds and calls changed or new. Then every open
    /// ticket of a kind whose text changed takes the new text for what it has not run (live edits, paper-research N-101
    /// p424: `Input::Retext`, an `edited` line each), or keeps its old text; the answer says which, one line a ticket.
    pub fn ticket_reload(&mut self, who: &Who, commit: Option<String>, why: String) -> Reply {
        if who.name() != "owner" {
            return err("only the owner has the kinds and calls read again".into());
        }
        let read = Tickets::read_defs(&self.places(), self.tickets.m.defs.first);
        let reread = match self.reload_checked(read) {
            Ok(r) => r,
            Err(refused) => return err(not_read_again(&refused)),
        };
        match self.reload_take(reread, who.name(), why, commit) {
            Ok(v) => ok(v.to_string()),
            Err(e) => err(e),
        }
    }

    /// The kinds and calls READ as a reload reads them, checked against those in place: what a reload takes, or every
    /// refusal (`not_read_again` words them for Reload and Reach). Reload, Reach and the text-change kind's check and
    /// load (change.rs, N-865) read through this alone; it changes nothing (the new definitions are put in only to ask
    /// the open tickets, `Machine::with_defs`, nothing counted).
    pub(crate) fn reload_checked(&mut self, read: DefsRead) -> Result<Reread, Vec<String>> {
        let (mut defs, errors, duties, call_files) = read;
        let mut refused = errors;
        let old = &self.tickets.m.defs;
        let m = &self.tickets.m;
        let open_of = |n: &str| -> Vec<String> { m.tickets.values().filter(|x| x.kind == n && x.is_open() && x.t < docket_machine::HIDDEN).map(|x| m.id(x.t)).collect() };
        // the tickets in the log that stand on a kind or call NAME being there: of that kind, or whose text names it (a
        // step's call, a kind it opens), or that recorded it as a call; a kind or call none stands on may go (the way
        // back of a text-change that added it, N-865), as the record replays without it
        let words = |text: &str| -> Vec<String> { text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-')).filter(|w| !w.is_empty()).map(String::from).collect() };
        // a kind: opened or reached by name (`open k(...)`, `reach k`); a call: named as a step's or a decision's
        let names = |text: &str, name: &str, kind: bool| {
            let ws = words(text);
            if kind {
                ws.windows(2).any(|w| (w[0] == "open" || w[0] == "reach") && w[1] == name)
            } else {
                ws.iter().any(|w| w == name || format!("{}{w}", docket_machine::syntax::DECIDE) == name)
            }
        };
        let stand_on = |n: &str, kind: bool| -> Vec<String> {
            let mut ts: Vec<String> = m
                .tickets
                .values()
                .filter(|x| (if kind { x.kind == n } else { x.calls.contains_key(n) }) || names(&x.program, n, kind))
                .map(|x| if x.t < docket_machine::HIDDEN { m.id(x.t) } else { format!("a piece of {}", x.parent.or(x.under).map_or_else(|| m.id(x.t), |p| m.id(p))) })
                .collect();
            ts.dedup();
            ts
        };
        let few = |ts: Vec<String>| if ts.len() > 5 { format!("{} and {} more", ts[..5].join(", "), ts.len() - 5) } else { ts.join(", ") };
        // the kinds whose form, result or raises change: what a ticket opens one with, or reads of it, as it was
        let mut changed = BTreeMap::new();
        for (n, k) in &old.kinds {
            match defs.kinds.get(n) {
                None => {
                    let on = stand_on(n, true);
                    if !on.is_empty() {
                        refused.push(format!("the kind {n} is gone, and the record has tickets that stand on it ({}): a kind stays once tickets have it", few(on)));
                    }
                }
                Some(d) if (d.letter, d.width) != (k.letter, k.width) => refused.push(format!("the kind {n}'s letter changed (its tickets' ids are made from it)")),
                // an open ticket of it runs to its form and promised its result: they stay while one is open; a closed
                // one's text is replayed with the kinds it opens as they were (`Ticket::loose`)
                Some(d) if (&d.form, &d.result, &d.raises) != (&k.form, &k.result, &k.raises) => {
                    let what = [(d.form != k.form, "form"), (d.result != k.result, "result"), (d.raises != k.raises, "raises")].iter().filter(|(c, _)| *c).map(|(_, w)| *w).collect::<Vec<_>>().join(", ");
                    let open = open_of(n);
                    if open.is_empty() && !m.tickets.values().any(|x| x.kind == *n && x.is_open()) {
                        changed.insert(n.clone(), what);
                    } else {
                        refused.push(format!("the {what} of {n} changed, and tickets of it are open ({}): they run to the {what} they were opened with; keep it until they close", if open.is_empty() { String::from("pieces of other tickets") } else { open.join(", ") }));
                    }
                }
                _ => {}
            }
        }
        for (n, c) in &old.calls {
            match defs.calls.get(n) {
                None => {
                    let on = stand_on(n, false);
                    if !on.is_empty() {
                        refused.push(format!("the call {n} is gone, and the record has tickets that stand on it ({}): a call stays once tickets have run it", few(on)));
                    }
                }
                Some(d) if d != c => {
                    let today: Vec<String> = m.runs_today(n).into_iter().filter(|t| m.tickets[t].is_open()).map(|t| m.id(t)).collect();
                    if !today.is_empty() {
                        refused.push(format!("the call {n} changed, and {} run it as the files have it, not as recorded", today.join(", ")));
                    }
                }
                _ => {}
            }
        }
        let kinds: Vec<String> = defs.kinds.iter().filter(|(n, k)| old.kinds.get(*n).is_none_or(|o| o.text != k.text)).map(|(n, _)| n.clone()).collect();
        defs.block_ask = old.block_ask;
        // an open ticket that would still open a kind whose form, result or raises change, by its own text or by the
        // new one it would take, would open it with the fields it had: refused, the ticket named; editing its kind in
        // the same reload, so that what it has not run opens it as it is now, lets it through (N-101 p460)
        if refused.is_empty() && !changed.is_empty() {
            let (more, back) = self.tickets.m.with_defs(defs, |m| {
                let mut out = Vec::new();
                for x in m.tickets.values().filter(|x| x.is_open()) {
                    for k in m.opens_after_reload(x.t, &kinds).iter().filter(|k| changed.contains_key(*k)) {
                        let shown = |t: u64| if t < docket_machine::HIDDEN { m.id(t) } else { format!("a piece of {}", x.parent.or(x.under).map_or_else(|| m.id(t), |p| m.id(p))) };
                        out.push(format!("the {} of {k} changed, and {} ({}) would still open a {k} as it was: edit {} in the same reload so that what it has not run opens it as it is now", changed[k], shown(x.t), x.kind, x.kind));
                    }
                }
                out
            });
            defs = back;
            refused.extend(more);
        }
        if !refused.is_empty() {
            return Err(refused);
        }
        let calls: Vec<String> = call_files.defs.iter().filter(|(n, d)| self.tickets.call_files.defs.get(*n).map(CallFiles::text) != Some(CallFiles::text(d))).map(|(n, _)| n.clone()).collect();
        Ok(Reread { defs, duties, call_files, kinds, calls })
    }

    /// What [`Core::reload_checked`] gave, taken: its definitions in place of the machine's, the reload's line (BY, WHY,
    /// COMMIT, the kinds edited and the calls whose text changed), its duties file and calls kept, and each open ticket
    /// of an edited kind given the new text for what it has not run, or keeping its old one and why: the answer,
    /// docket-edge's shape (N-235). Refused by the record, the definitions before stay. The owner's Reload, and the
    /// text-change kind's load (change.rs, N-865).
    pub(crate) fn reload_take(&mut self, reread: Reread, by: String, why: String, commit: Option<String>) -> Result<Value, String> {
        let Reread { defs, duties, call_files, kinds, calls } = reread;
        // the definitions changed outside `apply`: `edit` counts it, so nothing kept from before is given again
        // (`machine_due`, a delivery's id carries its kind's prefix)
        let before = std::mem::replace(self.tickets.m.defs.edit(), defs);
        let edited = kinds.clone();
        let changed_calls = calls.clone();
        match self.ticket_input(Input::Reload { by: by.clone(), why: why.clone(), commit, kinds, calls }) {
            Ok(r) => {
                self.tickets.duties = duties;
                self.tickets.call_files = call_files;
                // each open ticket of an edited kind, in order: took the new text, or kept its old text and why
                let open: Vec<u64> = self.tickets.m.tickets.values().filter(|x| x.is_open() && x.t < docket_machine::HIDDEN && x.kind != "part" && edited.contains(&x.kind)).map(|x| x.t).collect();
                let mut tickets = Vec::new();
                for t in open {
                    let (id, kind) = (self.tickets.m.id(t), self.tickets.m.tickets[&t].kind.clone());
                    let one = match self.ticket_input(Input::Retext { t, by: by.clone(), why: why.clone() }) {
                        Ok(said) => {
                            let withdrawn = self.tickets.m.tickets[&t].reloaded.as_ref().map(|r| r.withdrawn.clone()).unwrap_or_default();
                            json!({"id": id, "kind": kind, "took": true, "said": said, "withdrawn": withdrawn})
                        }
                        Err(e) => json!({"id": id, "kind": kind, "took": false, "why": e.strip_prefix("kept its old text: ").unwrap_or(&e)}),
                    };
                    tickets.push(one);
                }
                Ok(json!({"said": r, "kinds": edited, "calls": changed_calls, "tickets": tickets}))
            }
            Err(e) => {
                *self.tickets.m.defs.edit() = before;
                Err(e)
            }
        }
    }

    /// The owner's Reach of a draft, PATH (a kind's or a call's file, or one to be made there) as TEXT: who may, and
    /// where it is read, taken under the lock: the core's places (`Core::places`, as Reload reads) with the draft as
    /// their overlay, and the first ticket number, for `Tickets::read_defs` off it. Err: the refusal.
    pub fn ticket_reach_from(&self, who: &Who, path: &str, text: &str) -> Result<(crate::policy::Places, u64), Reply> {
        if who.name() != "owner" {
            return Err(err("only the owner has a draft read as a reload would read it".into()));
        }
        Ok((self.draft_places(path, text, true)?, self.tickets.m.defs.first))
    }

    /// The core's places (`Core::places`, as Reload reads) with the owner's draft TEXT of the file at PATH as their
    /// overlay, as `Reach` and a dry run read a draft: a call's file, or with KINDS a kind's too, of a folder the kinds
    /// and calls are read from (or one to be made there). Nothing is read at PATH. Err: the refusal.
    fn draft_places(&self, path: &str, text: &str, kinds_too: bool) -> Result<crate::policy::Places, Reply> {
        let mut places = self.places();
        let (kinds, calls) = defs_dirs(&folders(&places.coord, &places.base, places.project.as_deref()));
        let p = std::path::Path::new(path);
        let dir = p.parent().map(|d| d.display().to_string()).unwrap_or_default();
        let read = match p.extension().and_then(|e| e.to_str()) {
            Some("kind") => kinds_too && kinds.contains(&dir),
            Some("call") => calls.contains(&dir),
            _ => false,
        };
        if !read {
            return Err(err(if kinds_too {
                format!("{path} is no kind or call file of a folder the kinds and calls are read from ({})", kinds.iter().chain(&calls).cloned().collect::<Vec<_>>().join(", "))
            } else {
                format!("{path} is no call file of a folder the calls are read from ({})", calls.join(", "))
            }));
        }
        places.overlay = crate::policy::Overlay::from([(path.to_string(), text.to_string())]);
        Ok(places)
    }

    /// The owner's dry run of a prompt (`Request::DryRun`, H2; his N-216.p1) as taken under the lock: who may; where
    /// the calls are read, with his draft of the call file at PATH as their overlay (`draft_places`), or with no PATH,
    /// TEXT his prompt for ON's thread alone; and the step that fills it (`Machine::owed_give`, its agents filled in as
    /// a halt's are, `fill_prompts`): the step ON waits on when it is one of CALL, else, for a draft of the program, the
    /// newest open ticket's that is. Rendered off the lock (`render::dry_run`). Err: the refusal.
    pub fn ticket_dry_from(&self, who: &Who, call: &str, on: Option<&str>, path: Option<&str>, text: &str) -> Result<crate::render::Dry, Reply> {
        if who.name() != "owner" {
            return Err(err("only the owner has a step's prompt shown as it would be sent".into()));
        }
        let (places, once) = match path {
            Some(p) => (self.draft_places(p, text, false)?, None),
            None => (self.places(), Some(text.to_string())),
        };
        let m = &self.tickets.m;
        let of_call = |t: u64| m.owed_give(t).filter(|o| o.call == call);
        let asked = match on {
            Some(id) => Some(ticket_of(m, id).map_err(err)?),
            None => None,
        };
        let found = match asked.and_then(|t| of_call(t).map(|o| (t, true, o))) {
            Some(f) => Some(f),
            // his prompt for one thread fills only from that thread's step
            None if once.is_some() => None,
            None => m.tickets.values().rev().filter(|x| x.is_open() && x.t < docket_machine::HIDDEN && x.kind != "part").find_map(|x| of_call(x.t).map(|o| (x.t, false, o))),
        };
        let step = found.map(|(t, asked, mut o)| {
            self.agents_into(t, &mut o.args);
            (m.id(t), m.tickets[&t].kind.clone(), asked, o)
        });
        Ok(crate::render::Dry { places, step, once })
    }

    /// In tests: the owner's Reach of PATH as TEXT as main.rs answers it (`ticket_reach_from`, `Tickets::read_defs`,
    /// `ticket_reach`), with no lock to read off.
    #[cfg(test)]
    pub(crate) fn reach_of(&mut self, path: &std::path::Path, text: &str) -> Reply {
        match self.ticket_reach_from(&Who::Owner, &path.display().to_string(), text) {
            Ok((places, first)) => self.ticket_reach(Tickets::read_defs(&places, first)),
            Err(r) => r,
        }
    }

    /// The owner's Reach (`Request::Reach`, N-1396 p301): READ, the kinds and calls with his draft in place, read off
    /// the lock (`Tickets::read_defs`), checked as Reload checks them (`reload_checked`): its refusal in Reload's words;
    /// else what Reload would do, nothing changed, nothing recorded or counted. Reload's answer with each ticket's
    /// `reach` and `said` (`Machine::reach`: a ticket of an edited kind, and each a step of an edited call reaches), and
    /// `clocks`: each clock binding the draft edits, with its next firing (`Machine::clock_next`, as the clock reads it).
    pub fn ticket_reach(&mut self, read: DefsRead) -> Reply {
        let Reread { defs, call_files, kinds, calls, .. } = match self.reload_checked(read) {
            Ok(r) => r,
            Err(refused) => return err(not_read_again(&refused)),
        };
        // the calls a step names that are rendered from an edited one: its own, and a watcher's task's (`fold`, whose
        // steps coordd renders from `choose`, `clerk::runs`)
        let mut steps: Vec<(String, String)> = calls.iter().map(|c| (c.clone(), c.clone())).collect();
        for (n, d) in &call_files.defs {
            if let Some(r) = d.attr("task").and_then(|t| call_files.tasks.get(t)).filter(|r| *r != n && calls.contains(r)) {
                steps.push((n.clone(), r.clone()));
            }
        }
        let m = &self.tickets.m;
        let handed: BTreeSet<(u64, String)> = self.tickets.handed.iter().map(|((_, id), (t, ..))| (*t, id.clone())).collect();
        let was: BTreeSet<String> = m.defs.bindings.iter().map(|b| b.text.clone()).collect();
        let now = crate::core::now();
        let ((reach, clocks), _) = self.tickets.m.with_defs(defs, |m| {
            let handed = |t: u64, step: u32| handed.contains(&(t, format!("{}.s{step}", m.id(t))));
            let clocks: Vec<Value> = m.defs.bindings.iter().filter(|b| !was.contains(&b.text)).filter_map(|b| Some(json!({"binding": b.text, "next": m.clock_next(b, &now)?}))).collect();
            (m.reach(&now, &kinds, &steps, &handed), clocks)
        });
        let m = &self.tickets.m;
        let tickets: Vec<Value> = reach
            .into_iter()
            .map(|r| {
                let mut v = json!({"id": m.id(r.t), "kind": m.tickets.get(&r.t).map(|x| x.kind.clone()), "reach": r.case, "said": r.said});
                for (k, x) in [("took", r.took.map(Value::Bool)), ("why", r.why.map(Value::String)), ("agent", r.agent.map(Value::String))] {
                    if let Some(x) = x {
                        v[k] = x;
                    }
                }
                v
            })
            .collect();
        ok(json!({"kinds": kinds, "calls": calls, "tickets": tickets, "clocks": clocks}).to_string())
    }

    fn ticket_edit(&mut self, who: &Who, t: &str, said: Said, input: impl FnOnce(u64, String) -> Input) -> Reply {
        let t = match ticket_of(&self.tickets.m, t) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        if let Some(u) = said.unchecked {
            let kind = self.tickets.m.tickets[&t].kind.clone();
            let _ = self.ticket_input(Input::Unchecked { act: "edit".into(), by: who.name(), t: Some(t), kind, why: u });
        }
        match self.ticket_input(input(t, who.name())) {
            Ok(r) => ok(r),
            Err(e) => err(e),
        }
    }

    /// The owner's message to agent TO (the composer, `Request::Say`): the event `owner_message`, which opens a thread
    /// (agent-base thread.kind). NAMES are the @NAMEs in it; ADDRESSED says whether there were any, a head the thread
    /// matches on. The owner's only. The words wait for TO's turn to end; to stop it, `coord esc` first (N-36).
    /// TYPED: whom the owner typed the words to ([TO], or none when he picked nobody and TO is whoever fills lead).
    /// FOLLOWS: the closed ticket they were written on (`ticket_post`).
    #[allow(clippy::too_many_arguments)]
    pub fn ticket_say(&mut self, who: &Who, to: String, typed: Vec<String>, names: Vec<String>, text: String, follows: Option<u64>) -> Reply {
        self.ticket_say_under(who, to, typed, names, text, follows, None, None, false)
    }

    /// The owner's message, and with UNDER his answer "in a subthread" (paper-research N-447, piece 2): a post
    /// `N-234.p11`, or a ticket `N-234` at its last post. The words open a thread tied there from its open
    /// (`tied_at` on the event's value), a thread for sure as when he names an agent (Named: no intake); LABEL its
    /// label, else cut from the first line. THREAD: a new top thread for sure, no intake (N-447 p35).
    #[allow(clippy::too_many_arguments)]
    pub fn ticket_say_under(&mut self, who: &Who, to: String, typed: Vec<String>, names: Vec<String>, text: String, follows: Option<u64>, under: Option<String>, label: Option<String>, thread: bool) -> Reply {
        self.ticket_say_cleaned(who, to, typed, names, text, follows, under, label, thread, None)
    }

    /// As [`Core::ticket_say_under`], the words his words ticket CLEANED's cleaned text (N-564): it rides in the
    /// event's value as `cleaned`, so the record says it, and the thread it opens is looked at for the post.
    #[allow(clippy::too_many_arguments)]
    pub fn ticket_say_cleaned(&mut self, who: &Who, to: String, typed: Vec<String>, names: Vec<String>, text: String, follows: Option<u64>, under: Option<String>, label: Option<String>, thread: bool, cleaned: Option<u64>) -> Reply {
        self.ticket_say_marked(who, to, typed, names, text, follows, under, label, thread, cleaned, None)
    }

    /// `ticket_say_cleaned` for a point of his message split by point (N-696): FROM goes in the event's value as
    /// `split_from`, and the `split` line on the post of his it makes (`split_pending_round`).
    pub fn ticket_say_split(&mut self, who: &Who, to: String, typed: Vec<String>, names: Vec<String>, text: String, under: Option<String>, label: Option<String>, thread: bool, cleaned: Option<u64>, from: Option<docket_machine::SplitFrom>) -> Reply {
        self.ticket_say_marked(who, to, typed, names, text, None, under, label, thread, cleaned, from)
    }

    fn ticket_say_marked(&mut self, who: &Who, to: String, typed: Vec<String>, names: Vec<String>, text: String, follows: Option<u64>, under: Option<String>, label: Option<String>, thread: bool, cleaned: Option<u64>, split_from: Option<docket_machine::SplitFrom>) -> Reply {
        let tied_at = match under.as_deref() {
            None => None,
            Some(_) if follows.is_some() => return err("a subthread (under) and words on a closed ticket (follows) are two things: give one".into()),
            Some(u) => match self.under_place(u) {
                Ok((t, n, quote)) => Some((self.tickets.m.id(t), n, quote)),
                Err(e) => return err(e),
            },
        };
        let label = label.map(|l| l.trim().to_string()).filter(|l| !l.is_empty());
        if who.name() != "owner" {
            return err("only the owner's composer says things to an agent this way".into());
        }
        if !self.tickets.m.defs.bindings.iter().any(|b| b.ev == "owner_message" && b.kind == "thread") {
            return err("no binding opens a thread on the owner's message here; send it as mail instead".into());
        }
        if text.trim().is_empty() {
            return err("nothing to say".into());
        }
        // placed by him at a post: a thread for sure, as when he names an agent (no intake)
        let addressed = if names.is_empty() && tied_at.is_none() && !thread { "Unnamed" } else { "Named" };
        let follows = follows.map(|t| self.tickets.m.id(t));
        let mut value = json!({"from": "owner", "to": to, "typed": typed, "names": names, "text": text, "addressed": {"head": addressed}, "follows": follows});
        if let Some((t, n, quote)) = &tied_at {
            value["tied_at"] = json!(format!("{t}.p{n}"));
            if let Some(q) = quote {
                value["quote"] = q.clone();
            }
        }
        if let Some(l) = &label {
            value["label"] = json!(l);
        }
        if let Some(w) = cleaned {
            value["cleaned"] = json!(self.tickets.m.id(w));
        }
        if let Some(f) = &split_from {
            value["split_from"] = json!(f);
        }
        self.ticket_event("owner_message", &value);
        // to an agent whose session takes no messages: not delivered, and why, never only "taken" (paper-research N-1446
        // p2424; `AgentView::deaf`)
        let deaf = self.agents.get(&to).and_then(|a| a.deaf_words()).map(|(w, _)| format!(" Not delivered: {w}.")).unwrap_or_default();
        match (follows, tied_at) {
            (Some(f), _) => ok(format!("{f} is closed: a new thread follows it. Taken: to {to}.{deaf}")),
            (None, Some((t, n, _))) => ok(format!("Taken: to {to}, under {t} at post {n}.{deaf}")),
            (None, None) => ok(format!("Taken: to {to}.{deaf}")),
        }
    }

    /// The question who fills `lead` (for words typed to nobody, design-two-systems §16, 3): the team's facts and the
    /// policy, under the lock; `lead_of` answers it off it.
    pub fn lead_ask(&self) -> (crate::policy::Places, Vec<(String, Vec<String>)>) {
        (self.places(), self.fills_facts(u64::MAX))
    }

    /// Whether REQ, the owner's, needs to know who fills lead: words typed to nobody that open a thread (a Say with no
    /// agent picked, a post for a closed ticket).
    pub fn needs_lead(&self, who: &Who, req: &coord_types::Request) -> bool {
        if who.name() != "owner" {
            return false;
        }
        match req {
            coord_types::Request::Say { to: None, .. } => true,
            coord_types::Request::Post { t, to: None, .. } => ticket_of(&self.tickets.m, t).ok().and_then(|t| self.tickets.m.tickets.get(&t)).is_some_and(|x| !x.is_open()),
            _ => false,
        }
    }

    /// The owner: post N of T asks him nothing to answer (§16, 4).
    pub fn ticket_nothing_to_answer(&mut self, who: &Who, t: &str, n: u32) -> Reply {
        let t = match ticket_of(&self.tickets.m, t) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        match self.ticket_input(Input::NothingToAnswer { t, n, by: who.name() }) {
            Ok(r) => ok(r),
            Err(e) => err(e),
        }
    }

    /// WHO withdraws the asks of post N of T, with WHY (paper-research N-461, N-447 p5): the post's author, or an agent
    /// above it (`Core::above`); the owner is refused by the machine, naming his own act.
    pub fn ticket_withdraw_asks(&mut self, who: &Who, t: &str, n: u32, why: String) -> Reply {
        let t = match ticket_of(&self.tickets.m, t) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        if why.trim().is_empty() {
            return err("usage: coord ask --drop T.pN WHY".into());
        }
        let id = self.tickets.m.id(t);
        let Some(author) = self.tickets.m.tickets[&t].posts.iter().find(|p| p.n == n).map(|p| p.by.clone()) else {
            return err(format!("{id} has no post {n}."));
        };
        // the owner's post: the machine says it asks nothing
        if let Who::Agent(me) = who
            && *me != author
            && author != "owner"
            && !self.above(who, &author)
        {
            return err(format!("Post {n} of {id} is {author}'s: its asks are withdrawn by {author} or an agent above it."));
        }
        match self.ticket_input(Input::WithdrawAsks { t, n, by: who.name(), why }) {
            Ok(r) => ok(r),
            Err(e) => err(e),
        }
    }

    /// The hand-overs on their way that carry post N of T, as their `given` rows will read: (agent, delivery id), by
    /// what each leaves out given now (`Handing::skipped`), nothing when none of his up to its mark was asked or told
    /// late (`Ticket::hands_in`, (c2); proto-lead N-1446 p631). For the take-back.
    fn carrying(&self, t: u64, n: u32) -> Vec<(String, String)> {
        self.tickets.handed.iter().filter(|((a, d), (dt, upto, h))| *dt == t && docket_machine::Ticket::hands_in(d, *upto, &h.skipped(&self.tickets.m, a, t, *upto).unwrap_or_default(), n)).map(|(k, _)| k.clone()).collect()
    }

    /// WHO takes back its post N of T (paper-research N-447 p16, 2): the machine checks it is WHO's and nobody has it.
    pub fn ticket_take_back(&mut self, who: &Who, t: &str, n: u32) -> Reply {
        let t = match ticket_of(&self.tickets.m, t) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        // a delivery written to an agent's session is not in the record until the agent's side says it was taken (the
        // `given` line): while it is on its way the post counts as handed (docket-core3, N-447 p97). One of T's own
        // carrying it: taken back all the same, and its agent, if it takes it, had the post and is told
        // (`ticket_backs`; consult-3, N-818 p74). One of its handler's: refused, as before, its step on its way
        let reaches = self.tickets.m.take_back_reaches(t, n);
        let mut on_way: Vec<_> = self.tickets.handed.iter().filter(|(_, (dt, ..))| *dt != t && reaches.contains(dt)).map(|((a, _), (dt, ..))| (a.clone(), *dt)).collect();
        on_way.sort();
        if let Some((a, dt)) = on_way.first() {
            let id = self.tickets.m.id(t);
            return err(format!("Post {n} of {id} is being handed to {a} for its handler {}: it cannot be taken back.", self.tickets.m.id(*dt)));
        }
        let by = who.name();
        let carrying: Vec<(String, String)> = self.carrying(t, n).into_iter().filter(|(a, _)| *a != by).collect();
        let named = self.tickets.mentions.named_by(t, n);
        match self.ticket_input(Input::TakeBack { t, n, by }) {
            Ok(r) => {
                for k in carrying {
                    self.tickets.backs.entry(k).or_default().push((t, n));
                }
                // the tickets it named lose its line back: their views go out again, whole lines back included
                for id in named {
                    let Some(u) = self.tickets.m.ticket_named(&id).filter(|u| *u != t && *u < docket_machine::HIDDEN) else { continue };
                    if let Some(body) = self.ticket_shown(u).map(|v| self.ticket_body(v)) {
                        self.emit(body);
                    }
                }
                ok(r)
            }
            Err(e) => err(e),
        }
    }

    /// WHO passes the owner's `@all`, post N of T, on to agent TO, with WHY if given (`Request::Pass`; consult-lead
    /// N-412 post 1900): one post by WHO naming TO, which TO is handed as any post that names it. The machine checks
    /// what the record holds (`Machine::pass_refused`: WHO holds it, nobody claimed it, TO is not WHO and never held it
    /// before); coordd what only it knows, under its lock with the act: TO is an agent of the project, it can be woken
    /// now (running, not ended), so the `@all` is never left with one that cannot take it, and the team's norms let
    /// WHO name it there. Any refusal says why, and the `@all` stays with its holder.
    pub fn ticket_pass(&mut self, who: &Who, t: &str, n: u32, to: &str, why: Option<String>) -> Reply {
        let t = match ticket_of(&self.tickets.m, t) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        let (id, by, to) = (self.tickets.m.id(t), who.name(), to.trim().trim_start_matches('@'));
        if !self.agents.contains_key(to) {
            return err(format!("coord pass: no agent named {to} in this project: {id}.p{n} stays with whom it is handed to."));
        }
        if let Some(why) = self.tickets.m.pass_refused(t, n, &by, to) {
            return err(format!("coord pass: {why}"));
        }
        if !self.wakeable(to) {
            return err(format!("coord pass: {to} cannot be woken now (not running): {id}.p{n}, the owner's @all, stays with you. Answer it, or pass it to an agent that runs."));
        }
        if let Err(e) = self.may_name(&by, &[to.to_string()], Some(t)) {
            return err(format!("coord pass: {e}"));
        }
        let text = match why.as_deref().map(str::trim).filter(|w| !w.is_empty()) {
            Some(w) => format!("Passed {id}.p{n}, the owner's @all, to @{to}: {w}"),
            None => format!("Passed {id}.p{n}, the owner's @all, to @{to}."),
        };
        match self.ticket_input(Input::Pass { t, by, n, to: to.to_string(), text }) {
            Ok(r) => ok(format!("{} {to} holds {id}.p{n} now, to answer or pass on; you owe nothing on it.", r.trim_end())),
            Err(e) => err(format!("coord pass: {e}")),
        }
    }

    /// The owner archives T (`N-221`) or unarchives it (`coord archive`, `coord unarchive`, the window's `Archive`; the
    /// owner N-1073): the machine takes it from him alone, and says so in words to anyone else.
    pub fn ticket_archive(&mut self, who: &Who, t: &str, on: bool) -> Reply {
        let t = match ticket_of(&self.tickets.m, t) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        match self.ticket_input(Input::Archive { t, on, by: who.name() }) {
            Ok(r) => ok(r),
            Err(e) => err(e),
        }
    }

    /// The owner's star on T, or off it (§16, 3).
    /// BY stars T (`N-221`) or its post (`N-221.p5`), or takes its star off: the owner from the app, an agent by
    /// `coord star` (the owner N-234.p3).
    pub fn ticket_star(&mut self, who: &Who, t: &str, on: bool) -> Reply {
        // a post or a ticket, as a quote reply reads one (`post_or_ticket`): `N-221.p+5` is no post
        let Some((t, post)) = docket_machine::post_or_ticket(t) else { return err(format!("coord star: {t} is no post")) };
        let t = match ticket_of(&self.tickets.m, t) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        match self.ticket_input(Input::Star { t, on, by: who.name(), post }) {
            Ok(r) => ok(r),
            Err(e) => err(e),
        }
    }

    /// `coord tie T TO` (`-`: untie): T stands under TO in the outline from now on (N-101 p1676); the machine says who
    /// may and refuses a loop.
    pub fn ticket_tie(&mut self, who: &Who, t: &str, to: &str) -> Reply {
        let t = match ticket_of(&self.tickets.m, t) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        let to = if to == "-" {
            None
        } else {
            match ticket_of(&self.tickets.m, to) {
                Ok(p) => Some(p),
                Err(e) => return err(e),
            }
        };
        match self.ticket_input(Input::Tie { t, to, by: who.name() }) {
            Ok(r) => ok(r),
            Err(e) => err(e),
        }
    }

    /// `coord end T HOW WHY` (paper-research N-447, piece 1): thread T ends done, dropped or kept, and leaves the
    /// outline. WHO's `may edit` was asked before (SAID); the machine refuses a thread at work.
    pub fn ticket_end(&mut self, who: &Who, t: &str, how: &str, why: &str, said: Said) -> Reply {
        self.ticket_edit(who, t, said, |t, by| Input::End { t, by, how: how.to_string(), why: why.to_string() })
    }

    /// `coord end T [T...] HOW WHY` (N-447 p2): each of ENDS (its `may edit` asked, SAID) ended in turn; REFUSED, by
    /// the policy, said by name. One line each: ended, or refused and why. Ok when at least one ended.
    pub fn ticket_end_all(&mut self, who: &Who, ends: Vec<(String, Said)>, refused: Vec<(String, String)>, how: &str, why: &str) -> Reply {
        let mut lines: Vec<String> = refused.iter().map(|(t, e)| format!("{t}: refused: {e}")).collect();
        let mut any = false;
        for (t, said) in ends {
            let r = self.ticket_end(who, &t, how, why, said);
            any |= r.ok;
            lines.push(if r.ok { r.out } else { format!("{t}: refused: {}", r.out) });
        }
        let out = lines.join("\n");
        if any { ok(out) } else { err(out) }
    }

    /// Whether NAME's hand-over of T says how to title it (render's `untitled`): the owner's thread with no title, and
    /// NAME a lead or the agent he wrote it to, who may give its first title (`Line::Titled`, N-807.p4; N-985).
    pub fn ticket_untitled_for(&self, name: &str, t: u64) -> bool {
        let m = &self.tickets.m;
        let lead = m.roles_of(name).iter().any(|r| r == "lead");
        m.tickets.get(&t).is_some_and(|x| x.kind == "thread" && x.opener.as_deref() == Some("owner") && x.titled.is_none() && (lead || x.form.get("to").and_then(Value::as_str) == Some(name)))
    }

    /// `coord title T TEXT`: T's label from now on.
    pub fn ticket_title(&mut self, who: &Who, t: &str, label: &str) -> Reply {
        let t = match ticket_of(&self.tickets.m, t) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        match self.ticket_input(Input::Title { t, label: label.to_string(), by: who.name() }) {
            Ok(r) => ok(r),
            Err(e) => err(e),
        }
    }

    /// `coordc outline`: every open thread and item as one row (`ViewOutline`, without its children), JSON lines; and
    /// each one closed whose parent there is open, so the parent's line for it stays, marked so (N-447).
    pub fn ticket_outline(&self) -> Reply {
        ok(outline_rows(&self.tickets.m).iter().filter_map(|o| serde_json::to_string(o).ok()).collect::<Vec<_>>().join("\n"))
    }

    /// `Request::Insight` (S6, proto-lead N-1446 p388), the owner's: insight WHICH's rows as JSON lines. (3)'s from the
    /// machine under the lock, one pass over its tickets as `coordc outline`'s is, and its asks from the graph kept
    /// along the record ([`Core::insight_asks`]); (1)'s are the record's turns, which the machine does not keep: read
    /// off the lock by `main`'s `insight_off_lock` ([`insight_turns`]), never here. Nothing is recorded; no model or
    /// other process is asked.
    pub fn insight(&self, who: &Who, which: coord_types::Insight) -> Reply {
        if !matches!(who, Who::Owner) {
            return err("an insight into the data is the owner's".into());
        }
        let rows: Vec<String> = match which {
            coord_types::Insight::OpenToDone => docket_machine::insight::threads(&self.tickets.m).iter().filter_map(|x| serde_json::to_string(x).ok()).collect(),
            coord_types::Insight::WorkTalk => return err("coordd: the record's turns are read off the lock".into()),
            coord_types::Insight::Asks => self.insight_asks().iter().filter_map(|x| serde_json::to_string(x).ok()).collect(),
        };
        ok(rows.join("\n"))
    }

    /// Open → done's asks (paper-research N-1446 p1786): each of his asks in the graph kept along the record
    /// (`Graph::asks_now`), at the last point of its course reached and with his sign-off, in the order he asked them;
    /// his post's first line and its thread's label as `coord label` gives them (`label::of`); what it cost
    /// (`Graph::costs_at`, from the sessions read so far, `KeptSide`); and his posts on it once it was in his window
    /// (`Graph::his_after`).
    pub fn insight_asks(&self) -> Vec<docket_machine::insight::Ask> {
        let agents: Vec<String> = self.agents.keys().cloned().collect();
        Core::asks_rows(&self.tickets.graph.graph, &self.tickets.m, &agents)
    }

    /// [`Core::insight_asks`] of graph G and machine M, with AGENTS the registry's names (`label::of`): one function
    /// for coordd and the copy that times it under the lock (`a_coords_record_now`; paper-research N-1446 p1786, 2).
    pub(crate) fn asks_rows(g: &docket_machine::graph::Graph, m: &Machine, agents: &[String]) -> Vec<docket_machine::insight::Ask> {
        let mut costs: BTreeMap<String, docket_machine::graph::Cost> = g.costs_at(None).into_iter().map(|c| (c.post.clone(), c)).collect();
        let asks = g.asks_now(None);
        let after = g.his_after(&asks.iter().map(|a| &a.times).collect::<Vec<_>>());
        asks.into_iter()
            .zip(after)
            .map(|(ask, after)| {
                let l = crate::label::of(m, agents, &ask.times.post);
                let words = l.label.strip_prefix("owner: ").map_or_else(|| l.label.clone(), str::to_string);
                docket_machine::insight::Ask { cost: costs.remove(&ask.times.post), words, on: l.on.unwrap_or_default(), after, ask }
            })
            .collect()
    }

    /// The record's file: tickets/log.jsonl in the coord's folder, which `Request::Record` and insight (1) read off the
    /// lock.
    pub fn record_file(&self) -> std::path::PathBuf {
        std::path::Path::new(&self.coord).join("tickets/log.jsonl")
    }

    /// The owner switches thread T to slow or back (plan/layers.md I §5, 3); the engine refuses anyone else.
    pub fn ticket_slow(&mut self, who: &Who, t: &str, on: bool) -> Reply {
        if docket_machine::SCOPES.contains(&t) {
            return self.ticket_slowed(who, t, on);
        }
        let t = match ticket_of(&self.tickets.m, t) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        match self.ticket_input(Input::Slow { t, on, by: who.name() }) {
            Ok(r) => ok(r),
            Err(e) => err(e),
        }
    }

    /// Slow set over SCOPE (ON) or lifted (N-412 ask 5): the owner is not asked; an agent as the policy's act
    /// `set-slow(ME, SCOPE)` or `lift-slow(ME, SCOPE)` says, and a policy without the act or one that cannot be read
    /// lets no agent (the base rule: the owner alone).
    fn ticket_slowed(&mut self, who: &Who, scope: &str, on: bool) -> Reply {
        if let Who::Agent(me) = who {
            let act = if on { "set-slow" } else { "lift-slow" };
            match self.norm_verdict(act, &[me, scope], "none") {
                Ok(Some(v)) if v.enabled => {}
                Ok(Some(v)) => return err(format!("the team's norms do not let you {} the {scope}: {}", if on { "set slow over" } else { "lift slow from" }, v.why)),
                other => {
                    if let Err(e) = other {
                        eprintln!("coordd: norms: {act}({me}, {scope}): {e}; the base rule decides");
                    }
                    return err(format!("setting slow over the {scope} is the owner's"));
                }
            }
        }
        let before = self.tickets.m.slowed.clone();
        match self.ticket_input(Input::Slowed { scope: scope.into(), on, by: who.name() }) {
            Ok(r) => {
                // the app's project menu says on or off from the snapshot's `slowed`: a new one carries it
                if self.tickets.m.slowed != before {
                    let snap = self.snapshot();
                    self.emit(snap.body);
                }
                ok(r)
            }
            Err(e) => err(e),
        }
    }

    /// The owner lets halt N of T through.
    pub fn ticket_approve(&mut self, who: &Who, t: &str, n: u32) -> Reply {
        let t = match ticket_of(&self.tickets.m, t) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        match self.ticket_input(Input::Approve { t, n, by: who.name() }) {
            Ok(r) => ok(r),
            Err(e) => err(e),
        }
    }

    /// The owner refuses halt N of ticket T (N-1788), for REASON: what it held back is not done (docket-machine's
    /// `reject`). The ticket is named by its number, as the window has it.
    pub fn ticket_halt_reject(&mut self, who: &Who, t: u64, n: u32, reason: Option<String>) -> Reply {
        if !self.tickets.m.tickets.contains_key(&t) {
            return err(format!("There is no ticket {t}."));
        }
        match self.ticket_input(Input::HaltReject { t, n, by: who.name(), reason }) {
            Ok(r) => ok(r),
            Err(e) => err(e),
        }
    }

    /// The owner edits halt N of T (it still waits for his approval).
    pub fn ticket_halt_edit(&mut self, who: &Who, t: &str, n: u32, value: serde_json::Value) -> Reply {
        let t = match ticket_of(&self.tickets.m, t) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        match self.ticket_input(Input::HaltEdit { t, n, by: who.name(), value }) {
            Ok(r) => ok(r),
            Err(e) => err(e),
        }
    }

    /// The owner's Undo of the minder's last answer at T's hole (§16, 3): T rests there again.
    pub fn ticket_undo(&mut self, who: &Who, t: &str) -> Reply {
        if who.name() != "owner" {
            return err("only the owner undoes what the minder answered".into());
        }
        let t = match ticket_of(&self.tickets.m, t) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        match self.ticket_input(Input::Undo { t, by: who.name() }) {
            Ok(r) => ok(r),
            Err(e) => err(e),
        }
    }

    /// Move (the owner's): post N of T goes to ticket TO, or to a new thread (a note) when TO is None; T keeps a stub.
    pub fn ticket_move(&mut self, who: &Who, t: &str, n: u32, to: Option<String>) -> Reply {
        if who.name() != "owner" {
            return err("only the owner moves posts".into());
        }
        let t = match ticket_of(&self.tickets.m, t) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        let to = match to.map(|u| ticket_of(&self.tickets.m, &u)).transpose() {
            Ok(u) => u,
            Err(e) => return err(e),
        };
        match self.ticket_input(Input::Move { t, n, to, by: who.name() }) {
            Ok(r) => ok(r),
            Err(e) => err(e),
        }
    }

    /// Keep (the owner's "make this a ticket"): the one-offs TS, a run to one agent, become one resting thread.
    pub fn ticket_keep(&mut self, who: &Who, ts: Vec<String>) -> Reply {
        if who.name() != "owner" {
            return err("only the owner keeps one-offs".into());
        }
        let ts = match ts.iter().map(|t| ticket_of(&self.tickets.m, t)).collect::<Result<Vec<u64>, String>>() {
            Ok(ts) => ts,
            Err(e) => return err(e),
        };
        match self.ticket_input(Input::Keep { ts, by: who.name() }) {
            Ok(r) => ok(r),
            Err(e) => err(e),
        }
    }

    /// Drop (the owner's "not a ticket"): a resting thread ends a one-off, by an edit at its hole (`return OneOff`).
    pub fn ticket_drop(&mut self, who: &Who, t: &str) -> Reply {
        if who.name() != "owner" {
            return err("only the owner drops a thread".into());
        }
        self.ticket_edit(who, t, Said::default(), |t, by| Input::Edit { t, by, program: "return OneOff".into(), why: "dropped: a one-off".into() })
    }

    /// The owner's find (the owner N-234.p3: "search by id etc is needed"), over every ticket the record holds: with
    /// STARRED each star, the owner's first, then by when; else each post whose words hold every word of Q (case apart),
    /// newest first, at most 100, with the line they were found in.
    /// The starred list (STARRED), or what `coord find` by words reads: every shown post's text (a part resolved
    /// to the span it names, `Machine::text`), gathered under the lock and searched off it ([`find_words`]): a post's
    /// text is read from its place in a session file, and a record holds hundreds of posts (docket-core N-247.p2).
    /// `Request::Forming`'s part under the lock: ticket T's number and the coord's folder.
    pub fn ticket_forming(&self, t: &str) -> Result<(u64, String), Reply> {
        ticket_of(&self.tickets.m, t).map(|n| (n, self.coord.clone())).map_err(err)
    }

    /// `Request::Record`'s part under the lock, as [`Core::ticket_forming`]'s is: ticket T's number and those of every
    /// hidden ticket under it (its handlers' runs and what they opened, and so on), and the time of its post AT,
    /// where the lines start. The file is read off the lock ([`record_lines`]).
    pub fn ticket_record(&self, t: &str, at: Option<u32>) -> Result<RecordScope, Reply> {
        let m = &self.tickets.m;
        let n = ticket_of(m, t).map_err(err)?;
        let since = match at {
            None => None,
            Some(a) => match m.tickets[&n].posts.iter().find(|p| p.n == a) {
                Some(p) => serde_json::to_value(&p.at).ok().and_then(|v| v.as_str().map(str::to_string)),
                None => return Err(err(format!("{t} has no post {a}."))),
            },
        };
        // a handler's run stands `under` its ticket and is in no `children` (machine.rs `handler_ticket`): one pass
        // over the hidden tickets gives each ticket its runs and the tickets they opened. A shown ticket under T (a
        // subthread, an item) is a node of its own, with its own record
        let mut kids: HashMap<u64, Vec<u64>> = HashMap::new();
        for x in m.tickets.range(docket_machine::HIDDEN..).map(|(_, x)| x) {
            for up in [x.under, x.parent.filter(|p| Some(*p) != x.under)].into_iter().flatten() {
                kids.entry(up).or_default().push(x.t);
            }
        }
        for x in m.tickets.values() {
            kids.entry(x.t).or_default().extend(x.children.iter().copied().filter(|c| *c >= docket_machine::HIDDEN));
        }
        let mut ts = std::collections::HashSet::from([n]);
        let mut next = vec![n];
        while let Some(x) = next.pop() {
            for c in kids.get(&x).into_iter().flatten() {
                if ts.insert(*c) {
                    next.push(*c);
                }
            }
        }
        Ok(RecordScope { file: self.record_file(), id: m.id(n), ts, since })
    }

    /// `Request::Raw` of a slice of a session (`docket_machine::graph::slice_id`; paper-research N-1446 p1380), the
    /// owner's, under the lock, a moment: agent TO given DELIVERY at record line LINE, as [`SliceScope`] holds it for
    /// [`slice_words`] to read off the lock. The window asks by the giving, never by a place: the places are the
    /// machine's, the line is read from the record, and nothing but the id comes from the ask.
    pub fn ticket_slice(&self, w: &Who, id: &str) -> Result<SliceScope, String> {
        if !matches!(w, Who::Owner) {
            return Err("a session's words are the owner's, read in his window".into());
        }
        let Some((to, delivery, line)) = docket_machine::graph::slice_of(id) else { return Err(format!("{id} is no slice of a session")) };
        let m = &self.tickets.m;
        let Some(x) = docket_machine::parse_delivery(delivery).and_then(|(t, _)| m.tickets.get(&t)) else { return Err(format!("{delivery} is no delivery of a ticket here")) };
        let mine: Vec<_> = x.given.iter().filter(|g| g.to == to && g.delivery == delivery).collect();
        if mine.is_empty() {
            return Err(format!("{delivery} was never given to {to}"));
        }
        // each giving's place, with the posts it brought that are still shown: a post moved away or struck since is not
        // looked for
        let givings: Vec<(Loc, Vec<(String, Text)>)> = mine
            .iter()
            .filter_map(|g| {
                let posts = x.brought(g).filter(|p| p.visible() && p.moved.is_none()).map(|p| (docket_machine::graph::post_id(m, x.t, p.n), m.text(&p.text).unwrap_or_else(|| p.text.clone())));
                Some((g.place.clone()?, posts.collect()))
            })
            .collect();
        if givings.is_empty() {
            return Err(format!("{delivery} was given to {to} with no place in its session (a first read after a start, words typed into a terminal): there is nothing of it to read"));
        }
        Ok(SliceScope { file: self.record_file(), line: line as u64, t: x.t, to: to.to_string(), delivery: delivery.to_string(), givings })
    }

    /// `Request::Raw` of a system prompt block's text node (paper-research N-1446 p1642, 2), the owner's, under the
    /// lock, a moment: the request the graph says carried it (the first of its `carried` edges), and where that
    /// request's session's reader found the request's shape ([`crate::follow::KeptSide::shape_at`]), as
    /// [`BlockScope`] holds it for [`block_words`] to read off the lock. A text a start gave Claude Code beside its own
    /// (`started_with`; fix 4, docket-edge6 N-1446 p2529), which no request carries as a block: the requests sent under
    /// a start that gave it ([`Self::started_requests`]), each of their shapes once, the most used first, up to
    /// [`SHAPES_TRIED`], for its last block's tail (a subagent's or a compaction's request runs with a system prompt of
    /// its own; accounts-lead N-1446 p2587, 2). Nothing but the id comes from the ask.
    pub fn ticket_block(&self, w: &Who, id: &str) -> Result<BlockScope, String> {
        use docket_machine::graph::{EdgeKind, NodeKind, Source};
        if !matches!(w, Who::Owner) {
            return Err("a session's words are the owner's, read in his window".into());
        }
        let g = &self.tickets.graph.graph;
        if !g.get(id).is_some_and(|n| n.kind == NodeKind::Text) {
            return Err(format!("{id} is no text of the record's graph"));
        }
        let (es, tail) = match g.edges.iter().find(|e| e.kind == EdgeKind::Carried && e.to == id) {
            Some(e) => (vec![e], false),
            None => (self.started_requests(id)?, true),
        };
        let (mut shapes, mut refused): (Vec<((String, u64, String), usize)>, Option<String>) = Default::default();
        for e in es {
            let Source::Session(stem, line) = &e.source else { continue };
            match self.tickets.side.shape_at(stem, *line) {
                Ok(s) => match shapes.iter_mut().find(|(t, _)| t.2 == s.2) {
                    Some(t) => t.1 += 1,
                    None => shapes.push((s, 1)),
                },
                Err(m) => {
                    refused.get_or_insert(m);
                }
            }
        }
        if shapes.is_empty() {
            return Err(refused.unwrap_or_else(|| format!("the request that carried {id} is in no session")));
        }
        shapes.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        let shapes = shapes.into_iter().take(SHAPES_TRIED).map(|(s, _)| s).collect();
        Ok(BlockScope { shapes, id: id.to_string(), tail })
    }

    /// The `carried` edges, one a request, of the requests an agent sent under a start that gave it ID (`started_with`;
    /// fix 4, p2529), those its system prompt's version ID lists (`Graph::prompt`): Claude Code joined that text onto
    /// the last block of the shape each ran with ([`Self::ticket_block`]). The standing text is every agent's that has
    /// none of its own, and the first start that gave it may have sent nothing (accounts-lead N-1446 p2587, 1): the
    /// starts in record order, each agent once, the first whose requests under it the graph holds. One pass over the
    /// graph passes over each agent that sent nothing since its start (all of them just after a restart), so only an
    /// agent that did takes a `Graph::prompt` pass. Under the lock, on his press.
    fn started_requests(&self, id: &str) -> Result<Vec<&docket_machine::graph::Edge>, String> {
        use docket_machine::graph::{Edge, EdgeKind};
        use std::collections::HashSet;
        fn second(e: &Edge) -> &str {
            e.when.as_deref().and_then(|w| w.get(..19)).unwrap_or("")
        }
        let g = &self.tickets.graph.graph;
        // one pass: the starts that gave it, each start's agent, each session's agent and its latest request
        let (mut gave, mut start_of, mut session_of, mut latest): (Vec<&Edge>, HashMap<&str, &Edge>, HashMap<&str, &str>, HashMap<&str, &str>) = Default::default();
        for e in &g.edges {
            match e.kind {
                EdgeKind::StartedWith if e.to == id => gave.push(e),
                EdgeKind::StartOf => {
                    start_of.insert(e.from.as_str(), e);
                }
                EdgeKind::SessionOf => {
                    session_of.insert(e.from.as_str(), e.to.as_str());
                }
                EdgeKind::Sent => {
                    let at = latest.entry(e.from.as_str()).or_insert("");
                    *at = (*at).max(second(e));
                }
                _ => {}
            }
        }
        if gave.is_empty() {
            return Err(format!("no request carried {id} and no start gave it: only a system prompt's text is served"));
        }
        let mut last: HashMap<&str, &str> = HashMap::new();
        for (s, at) in &latest {
            if let Some(&a) = session_of.get(s) {
                let l = last.entry(a).or_insert("");
                *l = (*l).max(*at);
            }
        }
        let mut tried = HashSet::new();
        for s in gave.iter() {
            let Some(start) = start_of.get(s.from.as_str()) else { continue };
            let a = start.to.as_str();
            if !tried.insert(a) || last.get(a).is_none_or(|l| *l < second(start)) {
                continue;
            }
            let Some(name) = a.strip_prefix("agent:") else { continue };
            // its requests: the rows under the version's steps and turns
            let rows = g.prompt(name, Some(id));
            let (mut under, mut requests): (Vec<&docket_machine::graph_slice::Row>, HashSet<&str>) = (rows.iter().collect(), Default::default());
            while let Some(r) = under.pop() {
                if r.rows.is_empty() {
                    requests.insert(r.of.as_str());
                } else {
                    under.extend(&r.rows);
                }
            }
            let mut once = HashSet::new();
            let found: Vec<&Edge> = g.edges.iter().filter(|e| e.kind == EdgeKind::Carried && requests.contains(e.from.as_str()) && once.insert(e.from.as_str())).collect();
            if !found.is_empty() {
                return Ok(found);
            }
        }
        Err(format!("no request sent under a start that gave {id} is in the record's graph yet"))
    }

    pub fn ticket_find(&self, q: &str, starred: bool, wide: bool) -> Result<Reply, FindTexts> {
        let m = &self.tickets.m;
        // a ticket's title as its view gives it (`TicketView::of` takes `label::title`), without building the whole
        // view of every ticket under the lock for it: research's Find held the lock about 1.6 s a call, 31 s of the
        // minute he typed in (docket-edge6, paper-research N-412 p2161)
        let title = |x: &docket_machine::Ticket| docket_machine::label::title(m, x);
        let shown = m.tickets.values().filter(|x| x.t < docket_machine::HIDDEN);
        if !starred {
            let mut posts = Vec::new();
            let mut titles = std::collections::HashMap::new();
            for x in shown {
                titles.insert(x.t, title(x));
                let id = m.id(x.t);
                for p in &x.posts {
                    let text = m.text(&p.text).unwrap_or_else(|| p.text.clone());
                    // an answer given with its fields and no text (the owner's, in the app): found by its fields
                    let text = match (&text, &p.head) {
                        (Text::New(w), Some(h)) if w.trim().is_empty() => head_text(h),
                        _ => text,
                    };
                    posts.push(FoundPost { id: id.clone(), t: x.t, n: p.n, by: p.by.clone(), at: p.at.to_string(), text });
                }
            }
            let (mails, sessions) = if wide { self.find_sources() } else { Default::default() };
            let archived = titles.keys().copied().filter(|t| m.archived_by(*t).is_some()).collect();
            return Err(FindTexts { q: q.to_string(), posts, titles, archived, mails, sessions });
        }
        let mut hits: Vec<coord_types::FindHit> = Vec::new();
        for x in shown {
            for s in &x.stars {
                let post = s.post.and_then(|n| x.posts.iter().find(|p| p.n == n));
                let (by, at, words) = match post {
                    Some(p) => (p.by.clone(), p.at.to_string(), first_line(&crate::render::resolve(&m.text(&p.text).unwrap_or_else(|| p.text.clone())))),
                    None => (x.opener.clone().unwrap_or_default(), x.opened_at.to_string(), String::new()),
                };
                let id = s.post.map_or_else(|| m.id(x.t), |n| format!("{}.p{n}", m.id(x.t)));
                hits.push(coord_types::FindHit { id, t: x.t, post: s.post, title: title(x), by, at, words, star: Some((s.by.clone(), s.at.to_string())), archived: m.archived_by(x.t).is_some() });
            }
        }
        hits.sort_by(|a, b| {
            let owner = |h: &coord_types::FindHit| h.star.as_ref().is_none_or(|(by, _)| by != "owner");
            (owner(a), std::cmp::Reverse(a.star.as_ref().map(|s| s.1.clone()))).cmp(&(owner(b), std::cmp::Reverse(b.star.as_ref().map(|s| s.1.clone()))))
        });
        Ok(ok(hits.iter().map(|h| serde_json::to_string(h).unwrap_or_default()).collect::<Vec<_>>().join("\n")))
    }

    pub fn ticket_show(&self, t: &str) -> Reply {
        match ticket_of(&self.tickets.m, t).map(|t| self.ticket_shown(t)) {
            Ok(Some(v)) => {
                // the form, the step's args, the result and the posts in words: a place or a span of one read here,
                // since the client cannot (docket-edge m950: a question's ask printed as 'part at 288; len 70; t 13';
                // live round: an automatic reply printed as '(in FILE, byte N)')
                let mut v = serde_json::to_value(&v).unwrap_or_default();
                for k in ["form", "status", "posts"] {
                    if let Some(x) = v.get_mut(k) {
                        *x = worded(x);
                    }
                }
                // where each agent it waits on stands (N-101 p414): a layer, the line's view, or the project's folder
                let stands = |a: &str| self.agents.contains_key(a).then(|| json!(crate::moves::shown(crate::moves::stand(&self.coord, a).as_deref())));
                if let Some(s) = v["status"]["agent"].as_str().and_then(stands) {
                    v["status"]["stands"] = s;
                }
                for w in v.get_mut("waits_on").and_then(Value::as_array_mut).into_iter().flatten() {
                    if let Some(s) = w["agent"].as_str().and_then(stands) {
                        w["stands"] = s;
                    }
                }
                ok(serde_json::to_string_pretty(&v).unwrap_or_default())
            }
            Ok(None) => err(format!("There is no ticket {t}.")),
            Err(e) => err(e),
        }
    }

    pub fn ticket_list(&self, who: &Who, filter: &Value) -> Reply {
        let me = who.name();
        let m = &self.tickets.m;
        let mut out = Vec::new();
        // a ticket a handler runs in (design §13) is listed nowhere, as in ticket_views (docket-live, 12 Nov: coord
        // tickets showed O-1099511627778)
        for x in m.tickets.values().filter(|x| x.handling.is_none()) {
            let keep = if filter.get("waiting").and_then(Value::as_bool) == Some(true) {
                // owed by me, or a handler under it is (the clarify chip's step, paper-research m1314: T waits on me)
                m.owes(&me).iter().any(|(t, ..)| *t == x.t || m.tickets.get(t).is_some_and(|h| h.handling.is_some() && h.parent == Some(x.t)))
            } else if filter.get("mine").and_then(Value::as_bool) == Some(true) {
                x.opener.as_deref() == Some(me.as_str())
            } else if let Some(k) = filter.get("kind").and_then(Value::as_str) {
                x.kind == k
            } else {
                x.is_open()
            };
            if keep {
                if let Some(v) = self.ticket_shown(x.t) {
                    out.push(serde_json::to_string(&v).unwrap_or_default());
                }
            }
        }
        if !self.tickets.errors.is_empty() && filter.as_object().is_none_or(|o| o.is_empty()) {
            out.extend(self.tickets.errors.iter().map(|e| json!({"error": e}).to_string()));
        }
        ok(out.join("\n"))
    }
}

/// V with each place or span of one in it (`{"loc": ..}`, `{"span": ..}`) read to its words; the rest as it is.
fn worded(v: &Value) -> Value {
    match v {
        Value::Object(o) if o.len() == 1 && (o.contains_key("loc") || o.contains_key("span") || o.contains_key("joined")) => match Text::of(v) {
            Some(t @ (Text::Loc { .. } | Text::Span { .. } | Text::Joined { .. })) => Value::String(crate::render::resolve(&t)),
            _ => v.clone(),
        },
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), worded(x))).collect()),
        Value::Array(xs) => Value::Array(xs.iter().map(worded).collect()),
        _ => v.clone(),
    }
}


/// A post `coord find` reads: ticket ID (number T), post N by BY at AT, its TEXT (parts resolved).
/// An answer's head H as one text to search: its outcome, then each field's words (a text held by reference stays
/// where it is, read off the lock with the rest).
fn head_text(h: &Value) -> Text {
    fn walk(v: &Value, out: &mut Vec<Text>) {
        match Text::of(v) {
            Some(t @ (Text::Loc { .. } | Text::Span { .. } | Text::Joined { .. } | Text::Part { .. } | Text::Post { .. })) => out.push(t),
            _ => match v {
                Value::String(s) => out.push(Text::New(s.clone())),
                Value::Object(o) => o.iter().filter(|(k, _)| k.as_str() != "head" && !k.starts_with('@')).for_each(|(_, x)| walk(x, out)),
                Value::Array(xs) => xs.iter().for_each(|x| walk(x, out)),
                _ => {}
            },
        }
    }
    let mut out = vec![Text::New(h.get("head").and_then(Value::as_str).unwrap_or_default().to_string())];
    walk(h, &mut out);
    let mut joined = Vec::new();
    for t in out {
        joined.push(t);
        joined.push(Text::New("\n".into()));
    }
    Text::Joined { joined }
}

pub struct FoundPost {
    pub id: String,
    pub t: u64,
    pub n: u32,
    pub by: String,
    pub at: String,
    pub text: Text,
}

/// What `coord find` by words searches, gathered under the lock: query Q, the posts, the tickets' titles.
pub struct FindTexts {
    pub q: String,
    pub posts: Vec<FoundPost>,
    pub titles: std::collections::HashMap<u64, String>,
    /// The tickets the owner archived, or that are under one (`FindHit::archived`).
    pub archived: BTreeSet<u64>,
    /// `coord find`'s wider reach (find.rs): the mail, and each agent's sessions; empty for the app's Find.
    pub mails: Vec<crate::core::Mail>,
    pub sessions: Vec<(String, Vec<String>)>,
}

/// L cut to about 160 characters at a word, "…" where cut.
fn cut(l: &str) -> String {
    if l.chars().count() <= 160 {
        return l.to_string();
    }
    let head: String = l.chars().take(160).collect();
    format!("{}…", head.rsplit_once(' ').map_or(head.as_str(), |(h, _)| h))
}

/// S's first non-empty line, cut.
fn first_line(s: &str) -> String {
    cut(s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default())
}

/// What one `Request::Record` reads, taken under the lock ([`Core::ticket_record`]): the record's file, the tickets
/// whose lines it gives, and the time its first line is at or after.
pub struct RecordScope {
    pub file: std::path::PathBuf,
    /// The ticket's own id (`N-1104`): the lines that name its posts by id (`N-1104.p587`) are about it.
    pub id: String,
    pub ts: std::collections::HashSet<u64>,
    pub since: Option<String>,
}

/// Whether record line V is about SCOPE's tickets: by its own `t`; for an event, by its value's `t` or a ticket it
/// fired (`fired[].t`: the `owner_message` that opened the thread); or by a post of the ticket it names by id: the
/// turns his posts went into (proto-reader N-1446 p128), a turn's `delivery` and `informs` (consult-3 p174);
/// `delivery_acted`'s `items`, which are about the post and a turn's only on a `deliver`: a `keep` left them waiting,
/// a `report` reported them (proto-reader p223, p225), and a deliver's `rest` (what waited) is not read; `owner_to`'s
/// `posts`. A turn given the thread beside others names only one of them; [`record_lines`] takes it by its `given`.
fn about(v: &Value, scope: &RecordScope) -> bool {
    let num = |x: &Value| x.as_u64().is_some_and(|t| scope.ts.contains(&t));
    let post = |x: &Value| x.as_str().and_then(|p| p.split_once(".p")).is_some_and(|(t, _)| t == scope.id);
    let value = &v["value"];
    num(&v["t"])
        || (v["ev"] == "event" && num(&value["t"]))
        || v["fired"].as_array().is_some_and(|f| f.iter().any(|x| num(&x["t"])))
        || post(&v["delivery"])
        || post(&value["delivery"])
        || [&v["informs"], &value["items"], &value["posts"]].iter().any(|xs| xs.as_array().is_some_and(|xs| xs.iter().any(post)))
}

/// Whether line L may be about SCOPE: one of its tickets as a `"t":` number anywhere in it, or a post of its own by id
/// (`"N-1104.p`). A cheap look before the line is parsed, so the 100k lines of a long record are not all read as JSON.
fn names_any(l: &str, scope: &RecordScope) -> bool {
    l.contains(&format!("\"{}.p", scope.id))
        || l.match_indices("\"t\":").any(|(i, k)| {
            let digits: &str = &l[i + k.len()..];
            let end = digits.find(|c: char| !c.is_ascii_digit()).unwrap_or(digits.len());
            digits[..end].parse::<u64>().is_ok_and(|n| scope.ts.contains(&n))
        })
}

/// The `Request::Record` reply, off the lock: SCOPE's lines of the record as written, each with its number, oldest
/// first, at or after its time and after line AFTER, CAP at most (0: RECORD_CAP; never more). The file is read a line
/// at a time, so a record of 140 MB is never held whole.
///
/// A turn that was given the thread is in it though its end names another thread (consult-3 N-1446 p174: 107816,
/// consult-3's turn ended on `N-1334.p14`, was given N-1446.p109 at 106453): once a `given` of SCOPE's to agent A is
/// read, A's next `turn_ended` is SCOPE's. The `given` counts before AFTER and before the time too, so a page that
/// starts between the two keeps the turn.
pub fn record_lines(scope: &RecordScope, after: Option<u64>, cap: u32) -> Reply {
    use std::io::BufRead;
    let cap = if cap == 0 { coord_types::RECORD_CAP } else { cap.min(coord_types::RECORD_CAP) } as usize;
    let file = match std::fs::File::open(&scope.file) {
        Ok(f) => f,
        Err(e) => return err(format!("coordd: the record: {e}")),
    };
    let mut out = Vec::new();
    // the agents given one of SCOPE's posts whose turn has not ended since
    let mut given: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (i, l) in std::io::BufReader::new(file).lines().enumerate() {
        let (line, Ok(l)) = (i as u64 + 1, l) else { break };
        let ended = !given.is_empty() && l.contains("\"turn_ended\"");
        if !ended && !names_any(&l, scope) {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&l) else { continue };
        let mut is = about(&v, scope);
        match v["ev"].as_str() {
            Some("given") if is => {
                given.extend(v["to"].as_str().map(str::to_string));
            }
            Some("turn_ended") => is |= v["agent"].as_str().is_some_and(|a| given.remove(a)),
            _ => {}
        }
        if !is || after.is_some_and(|a| line <= a) {
            continue;
        }
        if scope.since.as_deref().is_some_and(|s| v.get("at").and_then(Value::as_str).is_some_and(|at| at < s)) {
            continue;
        }
        out.push(serde_json::to_string(&coord_types::RecordLine { line, text: l }).unwrap_or_default());
        if out.len() == cap {
            break;
        }
    }
    ok(out.join("\n"))
}

/// What one `Request::Raw` of a slice of a session reads ([`Core::ticket_slice`]): the record's file; the giving asked
/// for, DELIVERY of ticket T to TO at record line LINE; and each place the machine holds for a giving of DELIVERY to TO,
/// with the posts that giving brought, still shown, by id and text.
pub struct SliceScope {
    pub file: std::path::PathBuf,
    pub line: u64,
    pub t: u64,
    pub to: String,
    pub delivery: String,
    pub givings: Vec<(Loc, Vec<(String, Text)>)>,
}

/// What one `Request::Raw` of a system prompt block's text reads ([`Core::ticket_block`]): each shape it may be read
/// from, in the order tried, as its session's file, where the shape's line starts in it and the shape's hash; the
/// block's node; and whether it is a text a start gave Claude Code beside its own, read as the tail of the shape's last
/// block (fix 4).
pub struct BlockScope {
    shapes: Vec<(String, u64, String)>,
    id: String,
    tail: bool,
}

/// The most shapes one `Request::Raw` of a text a start gave tries off the lock, the most used first: a session's
/// requests run with one shape, a subagent's or a compaction's with its own (accounts-lead N-1446 p2587, 2).
const SHAPES_TRIED: usize = 8;

/// The text of SCOPE's block, off the lock ([`crate::follow::block_text`]), from the first of its shapes that holds
/// it, else refused as the first was: for the asker alone, nothing of it logged or kept.
pub fn block_words(scope: &BlockScope) -> Result<String, String> {
    let mut refused = None;
    for (file, at, shape) in &scope.shapes {
        match crate::follow::block_text(file, *at, shape, &scope.id, scope.tail) {
            Ok(t) => return Ok(t),
            Err(m) => {
                refused.get_or_insert(m);
            }
        }
    }
    Err(refused.unwrap_or_else(|| format!("no shape holds {}", scope.id)))
}

/// Line N of the record FILE, counted from 1 as `RecordLine::line` counts it, as JSON; None when the record has fewer
/// lines, or that one is not JSON. Read a line at a time up to it, so a long record is never held whole.
pub fn record_line(file: &std::path::Path, n: u64) -> Option<Value> {
    use std::io::BufRead;
    if n == 0 {
        return None;
    }
    let mut r = std::io::BufReader::new(std::fs::File::open(file).ok()?);
    let mut buf = Vec::new();
    for _ in 0..n {
        buf.clear();
        if r.read_until(b'\n', &mut buf).ok()? == 0 {
            return None;
        }
    }
    serde_json::from_slice(&buf).ok()
}

/// The words of a slice of a session, off the lock ([`Core::ticket_slice`]; paper-research N-1446 p1380): line LINE of
/// the record must be the `given` of SCOPE's delivery to its agent, with a place (`where`) the machine holds for a
/// giving of it. The text there ([`crate::follow::text_at`]) is given only when it holds the words of each post that
/// giving brought, and at least one has words; otherwise refused, saying why. For the asker alone: nothing of it is
/// logged or kept, and a refusal names posts by id, never their words.
pub fn slice_words(scope: &SliceScope) -> Result<String, String> {
    let at = format!("{} to {} at line {} of the record", scope.delivery, scope.to, scope.line);
    let v = record_line(&scope.file, scope.line).ok_or_else(|| format!("{at}: the record has no such line"))?;
    if v["ev"] != "given" || v["t"].as_u64() != Some(scope.t) || v["to"] != scope.to.as_str() || v["delivery"] != scope.delivery.as_str() {
        return Err(format!("{at}: that line is no giving of it"));
    }
    let place: Loc = v.get("where").and_then(|w| serde_json::from_value(w.clone()).ok()).ok_or_else(|| format!("{at}: that giving has no place in its session"))?;
    let Some((_, posts)) = scope.givings.iter().find(|(l, _)| *l == place) else { return Err(format!("{at}: the machine holds no giving of it at that place")) };
    let text = crate::follow::text_at(&place.file, place.offset, place.len).ok_or_else(|| format!("{at}: its session no longer has those bytes"))?;
    let mut held = 0;
    for (id, t) in posts {
        let words = crate::label::read(t).ok_or_else(|| format!("{at}: the words of {id}, which it brought, are gone"))?;
        let words = words.trim();
        if words.is_empty() {
            continue;
        }
        if !text.contains(words) {
            return Err(format!("{at}: its place does not hold the words of {id}, which it brought: not given"));
        }
        held += 1;
    }
    if held == 0 {
        return Err(format!("{at}: it brought no post with words (a step, a notice), so nothing shows its place holds it"));
    }
    Ok(text)
}

/// Whether NAME can be a release's folder name (`DATE-TIME-COMMIT[-dirty]`, a window-only `TIME-edge-HEAD-on-ENGINE`):
/// letters, digits, `-` and `_` only, so it names one folder in `releases/` and nothing above it.
fn release_name(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The releases release.sh says RELEASE contains, one name a line: RELEASES/RELEASE/contains.txt (written at its cut,
/// or by `release.sh contains` over folders cut before that step), or for a window-only edge, which has no folder,
/// RELEASES/edges/RELEASE.txt (written where `exe` puts it; proto-lead N-1446 p599); none without either.
fn contains_of(releases: &std::path::Path, release: &str) -> BTreeSet<String> {
    let listed = std::fs::read_to_string(releases.join(release).join("contains.txt"))
        .or_else(|_| std::fs::read_to_string(releases.join("edges").join(format!("{release}.txt"))))
        .unwrap_or_default();
    listed.lines().map(str::trim).filter(|n| release_name(n)).map(String::from).collect()
}

/// Insight (1)'s rows, off the lock (`main`'s `insight_off_lock`): every `turn_ended` line of the record's FILE
/// ([`Core::record_file`]), oldest first, as `docket_machine::insight::Turn` JSON lines with its line's number. Read a
/// line at a time, a cheap look before a line is parsed, as [`record_lines`] reads it; nothing else is opened.
pub fn insight_turns(file: &std::path::Path) -> Reply {
    use std::io::BufRead;
    let record = match std::fs::File::open(file) {
        Ok(f) => f,
        Err(e) => return err(format!("coordd: the record: {e}")),
    };
    let mut out = Vec::new();
    for (i, l) in std::io::BufReader::new(record).lines().enumerate() {
        let Ok(l) = l else { break };
        if !l.contains("\"turn_ended\"") {
            continue;
        }
        let turn = serde_json::from_str::<Line>(&l).ok().and_then(|x| docket_machine::insight::turn(&x, i as u64 + 1));
        out.extend(turn.and_then(|x| serde_json::to_string(&x).ok()));
    }
    ok(out.join("\n"))
}

/// Ticket T's forming files under COORD (`tickets/forming/T.sSTEP.N.json`), by step and N, each with its `formed` line
/// from the record's log (matched by the file it names): the `Request::Forming` reply, off the lock.
pub fn forming_reads(coord: &str, t: u64) -> Reply {
    let root = std::path::Path::new(coord);
    let mut lines: std::collections::HashMap<String, (String, Option<String>, Option<String>, Option<String>)> = std::collections::HashMap::new();
    if let Ok(log) = std::fs::read_to_string(root.join("tickets/log.jsonl")) {
        for l in log.lines().filter(|l| l.contains("\"formed\"")) {
            if let Ok(Line::Formed { at, t: lt, head, why, not_taken, call: Some(call), .. }) = serde_json::from_str::<Line>(l) {
                if lt == t {
                    let name = head.as_ref().and_then(|h| h.get("head")).and_then(Value::as_str).map(str::to_string);
                    lines.insert(call, (at.to_string(), name, why, not_taken));
                }
            }
        }
    }
    let mut reads = Vec::new();
    let prefix = format!("{t}.s");
    for e in std::fs::read_dir(root.join("tickets/forming")).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Some((step, n)) = name.strip_prefix(&prefix).and_then(|r| r.strip_suffix(".json")).and_then(|r| r.split_once('.')) else { continue };
        let (Ok(step), Ok(n)) = (step.parse::<u32>(), n.parse::<u32>()) else { continue };
        let file = format!("tickets/forming/{name}");
        let body: Value = std::fs::read_to_string(e.path()).ok().and_then(|b| serde_json::from_str(&b).ok()).unwrap_or(Value::Null);
        let calls = body.get("calls").cloned().and_then(|c| serde_json::from_value(c).ok()).unwrap_or_default();
        let (at, head, why, not_taken) = lines.remove(&file).unwrap_or_default();
        let agent = body.get("agent").and_then(Value::as_str).unwrap_or_default().to_string();
        reads.push(coord_types::FormingRead { file, step, n, agent, at, head, why, not_taken, calls });
    }
    reads.sort_by_key(|r| (r.step, r.n));
    ok(serde_json::to_string(&reads).unwrap_or_default())
}

/// `coord find` by words, off the lock: the posts holding every word of the query (any case), newest first, 100 at
/// most, each as a FindHit line. The line shown with a hit is the first line holding the query's first word (from a
/// little before it), a pointer to the post, not necessarily a line holding every word.
pub fn find_words(f: FindTexts) -> Reply {
    let words = coord_types::query_words(&f.q);
    if words.is_empty() {
        return ok(String::new());
    }
    let mut hits: Vec<coord_types::FindHit> = Vec::new();
    for p in &f.posts {
        let text = crate::render::resolve(&p.text);
        if !coord_types::holds_words(&text, &words) {
            continue;
        }
        let line = text.lines().find(|l| l.to_lowercase().contains(words[0].as_str())).map(|l| {
            let l = l.trim();
            let at = l.to_lowercase().find(words[0].as_str()).unwrap_or(0);
            let from = l.get(..at).unwrap_or_default().char_indices().rev().filter(|(_, c)| *c == ' ').nth(5).map_or(0, |(i, _)| i + 1);
            let from = if l.is_char_boundary(from) { from } else { 0 };
            if from == 0 { cut(l) } else { format!("…{}", cut(&l[from..])) }
        }).unwrap_or_else(|| first_line(&text));
        hits.push(coord_types::FindHit { id: format!("{}.p{}", p.id, p.n), t: p.t, post: Some(p.n), title: f.titles.get(&p.t).cloned().unwrap_or_default(), by: p.by.clone(), at: p.at.clone(), words: line, star: None, archived: f.archived.contains(&p.t) });
    }
    hits.extend(crate::find::hits(&words, &f.mails, &f.sessions));
    hits.sort_by(|a, b| b.at.cmp(&a.at));
    hits.truncate(100);
    ok(hits.iter().map(|h| serde_json::to_string(h).unwrap_or_default()).collect::<Vec<_>>().join("\n"))
}
#[cfg(test)]
mod tests {
    use super::*;

    /// N-807.p20, for part 3's choice: in a copy of a record, the owner's posts put into a running turn (`given` with
    /// `into`) or typed into one (`typed`) that the agent never answered on their thread, by age.
    /// `OWED_COORD=DIR cargo test -p coordd owed_in_a_record -- --ignored --nocapture`; it writes nothing.
    #[test]
    #[ignore]
    fn owed_in_a_record() {
        let Ok(coord) = std::env::var("OWED_COORD") else { return };
        let text = std::fs::read_to_string(format!("{coord}/tickets/log.jsonl")).unwrap();
        let mut put: Vec<(u64, u32, String, String)> = Vec::new();
        for l in text.lines() {
            let Ok(v) = serde_json::from_str::<Value>(l) else { continue };
            let at = v["at"].as_str().unwrap_or("").to_string();
            match v["ev"].as_str() {
                Some("given") if v["into"] == json!(true) => put.push((v["t"].as_u64().unwrap_or(0), v["upto"].as_u64().unwrap_or(0) as u32, v["to"].as_str().unwrap_or("").into(), at)),
                Some("typed") => put.push((v["t"].as_u64().unwrap_or(0), v["n"].as_u64().unwrap_or(0) as u32, v["agent"].as_str().unwrap_or("").into(), at)),
                _ => {}
            }
        }
        let m = Tickets::load(&coord, &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), crate::project::Project::load(&coord).ok().flatten().map(|p| p.dir.clone()).as_deref(), 1).m;
        let mut owed: Vec<(String, String)> = Vec::new();
        for (t, n, a, at) in put {
            let Some(x) = m.tickets.get(&t) else { continue };
            let answered = x.posts.iter().any(|p| p.by == a && p.n > n && !p.dropped);
            if !answered && x.posts.iter().any(|p| p.n == n && p.by == "owner") {
                owed.push((at, format!("{}.p{n} to {a}{}", m.id(t), if x.is_open() { "" } else { " (closed)" })));
            }
        }
        owed.sort();
        owed.dedup_by(|a, b| a.1 == b.1);
        println!("unanswered put-in or typed posts of his: {}", owed.len());
        for (at, w) in &owed {
            println!("  {at} {w}");
        }
    }

    /// The owner's "Waits on you" asks in a copy of a record, as the old rule counts them and as `asks_waiting` does
    /// now (paper-research N-800.p12): `ASKS_COORD=DIR ASKS_BASE=DIR cargo test -p coordd asks_that_only_inform --
    /// --ignored --nocapture`, DIR a copy holding the tickets log and the project file; it writes nothing in it.
    #[test]
    #[ignore]
    fn asks_that_only_inform_the_owner_in_a_record() {
        let (Ok(coord), Ok(base)) = (std::env::var("ASKS_COORD"), std::env::var("ASKS_BASE")) else { return };
        let project = crate::project::Project::load(&coord).unwrap().map(|p| p.dir.clone());
        let t = Tickets::load(&coord, &base, project.as_deref(), 1);
        let m = &t.m;
        let (mut before, mut after) = (Vec::new(), Vec::new());
        for x in m.tickets.values() {
            for (_, n) in m.asks_waiting(x) {
                after.push(format!("{}.p{n}", m.id(x.t)));
            }
            // the old rule: asks_waiting without the only-informed test
            if !matches!(x.status, docket_machine::Status::Rests { .. }) && x.is_open() {
                continue;
            }
            let minder = x.is_open() && x.handlers.iter().any(|h| h.ev == "hole");
            let mut out: Vec<(String, u32)> = Vec::new();
            let mut k = 0;
            for p in &x.posts {
                if p.by != "coordd" {
                    k += 1;
                }
                if (!minder || x.asked_at.is_some_and(|a| a.0 >= k)) && m.asks_open(x, p.n) {
                    out.retain(|(a, _)| *a != p.by);
                    out.push((p.by.clone(), p.n));
                }
            }
            before.extend(out.into_iter().map(|(_, n)| format!("{}.p{n}", m.id(x.t))));
        }
        let gone: Vec<&String> = before.iter().filter(|b| !after.contains(b)).collect();
        println!("asks waiting on the owner: {} before, {} after; no longer: {}", before.len(), after.len(), gone.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" "));
    }

    /// Each forming call's prompt and reply are kept whole, one file per forming, numbered on: the `formed` line points
    /// at it (paper-research N-101 p554, 1); with no call, nothing is kept.
    #[test]
    fn the_forming_calls_are_kept() {
        let coord = std::env::temp_dir().join(format!("docket-forming-{}", std::process::id()));
        let c = coord.display().to_string();
        let kept = crate::form::Kept { call: "form-head".into(), model: Some("luna".into()), class: Some("luna (1, small)".into()), rules: vec!["tier(option) >= 2".into()], prompt: "You turn an agent's answer".into(), reply: Some(json!({"head": "None", "why": "It names no agent."})) };
        assert_eq!(keep_forming(&c, 37, 2, "a", &[]), None);
        assert_eq!(keep_forming(&c, 37, 2, "a", &[kept.clone()]).as_deref(), Some("tickets/forming/37.s2.1.json"));
        assert_eq!(keep_forming(&c, 37, 2, "a", &[kept]).as_deref(), Some("tickets/forming/37.s2.2.json"));
        let v: Value = serde_json::from_str(&std::fs::read_to_string(coord.join("tickets/forming/37.s2.1.json")).unwrap()).unwrap();
        let _ = std::fs::remove_dir_all(&coord);
        assert_eq!((v["agent"].clone(), v["calls"][0]["prompt"].clone(), v["calls"][0]["reply"]["why"].clone()), (json!("a"), json!("You turn an agent's answer"), json!("It names no agent.")));
        // why that model ran: its class, and the rules that left the others out (paper-research N-101 p631)
        assert_eq!((v["calls"][0]["class"].clone(), v["calls"][0]["rules"].clone()), (json!("luna (1, small)"), json!(["tier(option) >= 2"])));
    }

    /// `Request::Forming`: a ticket's forming files by step and number, each with its `formed` line; another
    /// ticket's (37 beside 3) left out (paper-research N-101 p623).
    #[test]
    fn a_tickets_readings_are_listed_with_their_formed_lines() {
        let coord = std::env::temp_dir().join(format!("docket-reads-{}", std::process::id()));
        let c = coord.display().to_string();
        let kept = |why: &str| crate::form::Kept { call: "form-head".into(), model: Some("luna".into()), class: None, rules: Vec::new(), prompt: "You turn an agent's answer".into(), reply: Some(json!({"head": "None", "why": why})) };
        let f1 = keep_forming(&c, 3, 1, "hk", &[kept("No commit is named.")]).unwrap();
        let f2 = keep_forming(&c, 3, 1, "hk", &[kept("-")]).unwrap();
        let f0 = keep_forming(&c, 3, 0, "hk", &[kept("-")]).unwrap();
        keep_forming(&c, 37, 1, "a", &[kept("-")]).unwrap();
        let lines = [
            Line::Formed { at: "2026-11-16T12:48:50Z".into(), t: 3, step: 1, head: None, why: Some("No commit is named.".into()), not_taken: None, call: Some(f1.clone()) },
            Line::Formed { at: "2026-11-16T12:48:56Z".into(), t: 3, step: 1, head: Some(json!({"head": "Commit", "commit": "abc"})), why: None, not_taken: None, call: Some(f2.clone()) },
        ];
        std::fs::write(coord.join("tickets/log.jsonl"), lines.iter().map(|l| serde_json::to_string(l).unwrap() + "\n").collect::<String>()).unwrap();
        let r = forming_reads(&c, 3);
        let _ = std::fs::remove_dir_all(&coord);
        assert!(r.ok, "{}", r.out);
        let reads: Vec<coord_types::FormingRead> = serde_json::from_str(&r.out).unwrap();
        assert_eq!(reads.iter().map(|r| r.file.as_str()).collect::<Vec<_>>(), [f0.as_str(), f1.as_str(), f2.as_str()]);
        assert_eq!((reads[1].head.clone(), reads[1].why.as_deref(), reads[1].at.as_str()), (None, Some("No commit is named."), "2026-11-16T12:48:50Z"));
        assert_eq!((reads[2].head.as_deref(), reads[2].agent.as_str(), reads[2].calls[0].prompt.as_str()), (Some("Commit"), "hk", "You turn an agent's answer"));
        assert_eq!(reads[0].at, "", "a file with no formed line");
    }

    #[test]
    fn the_real_kinds_and_calls_load() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-load-{}", std::process::id()));
        let t = Tickets::load(&coord.display().to_string(), &base, None, 1);
        let _ = std::fs::remove_dir_all(&coord);
        // `delivery`: the message runtime's program (0.2 head 2), run by the delivery events, opened by no one; `brain`:
        // an agent's brain's passes (0.3), opened by the agent's own program where it binds them; `owner_model`, the
        // project's owner model, opened by its lead, and `owner_update`, an update of it, opened by coordd (brain.rs);
        // `text-change`, a change to the project's own text (change.rs, N-865)
        assert_eq!(t.m.defs.kinds.keys().cloned().collect::<Vec<_>>(), vec!["base", "brain", "check", "claim", "decision", "delivery", "filing", "fix", "item", "note", "order", "owner_model", "owner_update", "question", "request", "text-change", "thread"]);
        assert!(t.errors.is_empty(), "{:#?}", t.errors);
        // the same calls through the renderer's parser, so a call file either refuses fails here too
        if let Err(e) = crate::policy::call::Calls::load(&[format!("{base}/calls")], &Default::default()) {
            panic!("the calls do not load for rendering: {e}");
        }
    }

    /// `the_real_kinds_and_calls_load` for each project in DEFS_PROJECTS (flavor's `projects/`) on the agent-base at
    /// DEFS_BASE: its kinds, calls and policy read as its coordd reads them (`read_defs`: agent-base's, then its
    /// library's, as its project.toml names it, then its own), with nothing left out. A library may hold its own copy of
    /// a base kind, which then stands on agent-base's calls: Tovrin's `thread` still called `asks` when agent-base's
    /// asks.call went (consult-3 N-1479 p420). It runs where flavor is: `DEFS_PROJECTS=DIR DEFS_BASE=DIR <coordd test
    /// binary> --ignored --nocapture --exact ticket::tests::each_projects_kinds_and_calls_load`. The runner sends a
    /// worktree's nested flavor/ too (build-runner `tree_list`), but not to the worktree's path, so a runner job names
    /// the folders from this crate's: `DEFS_PROJECTS=../../flavor/projects DEFS_BASE=../../agent-base`, each
    /// passed with `--env`.
    #[test]
    #[ignore]
    fn each_projects_kinds_and_calls_load() {
        let (Ok(projects), Ok(base)) = (std::env::var("DEFS_PROJECTS"), std::env::var("DEFS_BASE")) else { return };
        let mut dirs: Vec<std::path::PathBuf> = std::fs::read_dir(&projects).unwrap().flatten().map(|e| e.path()).filter(|p| p.join("project.toml").is_file()).collect();
        dirs.sort();
        assert!(!dirs.is_empty(), "no project.toml under {projects}");
        let mut left_out = Vec::new();
        for dir in dirs {
            let name = dir.file_name().unwrap().to_string_lossy().to_string();
            // a coord whose project file names this project, so its library is found as coordd finds it
            let coord = std::env::temp_dir().join(format!("docket-defs-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&coord);
            std::fs::create_dir_all(&coord).unwrap();
            std::fs::write(coord.join("project"), dir.join("project.toml").display().to_string()).unwrap();
            let (c, d) = (coord.display().to_string(), dir.display().to_string());
            let folders = crate::project::folders(&c, &base, Some(&d));
            let (defs, errors, ..) = Tickets::read_defs(&crate::policy::Places { base: base.clone(), project: Some(d), coord: c, ..Default::default() }, 1);
            let _ = std::fs::remove_dir_all(&coord);
            println!("{name}: {} kinds from {} folders {folders:?}, {} left out", defs.kinds.len(), folders.len(), errors.len());
            for e in &errors {
                println!("  {e}");
            }
            if !errors.is_empty() {
                left_out.push(name);
            }
        }
        assert!(left_out.is_empty(), "something is left out on {left_out:?}");
    }

    #[test]
    fn a_minute_with_nothing_owed_asks_no_solve() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-expiry-{}", std::process::id()));
        std::fs::create_dir_all(&coord).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.tickets = Tickets::load(&coord.display().to_string(), &base, None, 1);
        let q = expiry_question(&core, chrono::Utc::now());
        let _ = std::fs::remove_dir_all(&coord);
        assert!(core.tickets.m.awaiting().is_empty());
        assert!(q.is_none(), "nothing owed: no question, so no solve");
    }

    #[test]
    fn a_step_expires_by_the_policy() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let places = crate::policy::Places { base, coord: std::env::temp_dir().display().to_string(), ..Default::default() };
        let facts = vec![("agent".to_string(), vec!["lead".to_string()]), ("role".to_string(), vec!["lead".to_string(), "lead".to_string()])];
        let owed = vec![(1, 1, "lead".to_string(), "C-1".to_string(), 30), (2, 1, "f2-1".to_string(), "P-2".to_string(), 200), (3, 1, "owner".to_string(), "Q-3".to_string(), 600)];
        let found = expired(&places, facts, &owed).unwrap();
        assert_eq!(found.iter().map(|(t, s, _)| (*t, *s)).collect::<Vec<_>>(), vec![(2, 1)]);
    }

    #[test]
    fn a_project_may_shorten_the_wait() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let project = std::env::temp_dir().join(format!("docket-silent-{}", std::process::id()));
        std::fs::create_dir_all(project.join("policy")).unwrap();
        std::fs::write(project.join("policy/silent.eflint"), "+silent-after(2).\n").unwrap();
        let places = crate::policy::Places { base, project: Some(project.display().to_string()), coord: std::env::temp_dir().display().to_string(), ..Default::default() };
        let owed = vec![(1, 1, "lead".to_string(), "C-1".to_string(), 1), (2, 1, "dev".to_string(), "C-2".to_string(), 3)];
        let found = expired(&places, Vec::new(), &owed);
        let _ = std::fs::remove_dir_all(&project);
        assert_eq!(found.unwrap().iter().map(|(t, _, _)| *t).collect::<Vec<_>>(), vec![2]);
    }

    /// Replay the log in $DOCKET_COORD (`$DOCKET_COORD/tickets/log.jsonl`) with the worktree's kinds and calls:
    /// `DOCKET_COORD=~/agent-coord/docket-dev cargo test --release -p coordd replays_a_log -- --ignored`.
    #[test]
    #[ignore]
    fn replays_a_log() {
        let coord = std::env::var("DOCKET_COORD").expect("DOCKET_COORD names a coord folder");
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        // with the coord's own project, as coordd loads it (docket-live's kinds and calls are its project's)
        let project = crate::project::Project::load(&coord).unwrap().map(|p| p.dir);
        let t = Tickets::load(&coord, &base, project.as_deref(), 1);
        assert!(t.errors.is_empty(), "{:#?}", t.errors);
        assert!(!t.m.tickets.is_empty());
    }

    /// A project's kinds, calls and policy load with agent-base's, with nothing refused
    /// (`DOCKET_PROJECT=DIR cargo test -p coordd loads_a_project -- --ignored`), and its policy compiles.
    #[test]
    #[ignore]
    fn loads_a_project() {
        let project = std::env::var("DOCKET_PROJECT").expect("DOCKET_PROJECT names a project folder");
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-project-{}", std::process::id()));
        std::fs::create_dir_all(&coord).unwrap();
        let t = Tickets::load(&coord.display().to_string(), &base, Some(&project), 1);
        assert!(t.errors.is_empty(), "{:#?}", t.errors);
        eprintln!("kinds: {:?}", t.m.defs.kinds.keys().collect::<Vec<_>>());
        eprintln!("bindings: {:#?}", t.m.defs.bindings.iter().map(|b| b.text.clone()).collect::<Vec<_>>());
        let places = crate::policy::Places { base, project: Some(project), coord: coord.display().to_string(), ..Default::default() };
        crate::policy::norms::load(&places).expect("the norms load");
        places.sources().and_then(|s| crate::policy::compile::compile(&s)).expect("the policy compiles");
        let _ = std::fs::remove_dir_all(&coord);
    }

    #[test]
    fn the_role_is_filled_by_the_policy() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let places = crate::policy::Places { base, coord: std::env::temp_dir().display().to_string(), ..Default::default() };
        let mut facts = Vec::new();
        for (a, role, running) in [("orch", "lead", false), ("lead2", "lead", true), ("dev", "fixer", true)] {
            facts.push(("agent".to_string(), vec![a.to_string()]));
            facts.push(("role".to_string(), vec![a.to_string(), role.to_string()]));
            if running {
                facts.push(("running".to_string(), vec![a.to_string()]));
            }
        }
        facts.push(("ticket".to_string(), vec!["C-1".to_string()]));
        let fields = |r: &str| BTreeMap::from([("role".to_string(), r.to_string()), ("call".to_string(), "triage".to_string()), ("ticket".to_string(), "C-1".to_string())]);
        assert_eq!(fills(&places, facts.clone(), fields("lead")).unwrap().0.as_deref(), Some("lead2"));
        assert_eq!(fills(&places, facts.clone(), fields("fixer")).unwrap().0.as_deref(), Some("dev"));
        assert_eq!(fills(&places, facts, fields("reviewer")).unwrap().0, None);
    }

    /// A role the project's policy gives an agent besides its own (`+role(A, R)`) is found by `fills`, so by `step role
    /// R` and an ask's lead, and by `roles_held`, from which the record's role lines and `Machine::holders` come
    /// (paper-research N-101 p508: one way of asking who holds a role).
    #[test]
    fn a_role_the_policy_gives_is_found_by_fills_and_by_roles_held() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let project = std::env::temp_dir().join(format!("docket-held-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&project);
        std::fs::create_dir_all(project.join("policy")).unwrap();
        // the role given by name, and the rule that names no agent (paper-research N-101 p512): the same answers
        for policy in ["// the orchestrator leads\n+role(\"orch\", \"lead\").\n", "// whoever orchestrates leads\nExtend Fact role\n  Derived from role(agent, \"lead\") When role(agent, \"orchestrator\")\n"] {
        std::fs::write(project.join("policy/roles.eflint"), policy).unwrap();
        let places = crate::policy::Places { base: base.clone(), project: Some(project.display().to_string()), coord: std::env::temp_dir().display().to_string(), ..Default::default() };
        let mut facts = Vec::new();
        for (a, role) in [("orch", "orchestrator"), ("dev", "member")] {
            facts.push(("agent".to_string(), vec![a.to_string()]));
            facts.push(("role".to_string(), vec![a.to_string(), role.to_string()]));
            facts.push(("running".to_string(), vec![a.to_string()]));
        }
        facts.push(("ticket".to_string(), vec!["C-1".to_string()]));
        let fields = |r: &str, call: &str| BTreeMap::from([("role".to_string(), r.to_string()), ("call".to_string(), call.to_string()), ("ticket".to_string(), "C-1".to_string())]);
        // `step role lead`, and an ask's answer passed up to the lead
        assert_eq!(fills(&places, facts.clone(), fields("lead", "triage")).unwrap().0.as_deref(), Some("orch"));
        assert_eq!(fills(&places, facts.clone(), fields("lead", "answer")).unwrap().0.as_deref(), Some("orch"));
        assert_eq!(fills(&places, facts.clone(), fields("orchestrator", "assign")).unwrap().0.as_deref(), Some("orch"));
        assert_eq!(lead_of(&places, facts.clone()).as_deref(), Some("orch"));
        let held = roles_held(&places, facts.clone()).unwrap();
        // and text-reader, which agent-base gives the lead (policy/text-reader.eflint; paper-research N-1446 p2894)
        assert_eq!(held["orch"], BTreeSet::from(["orchestrator".to_string(), "lead".to_string(), "text-reader".to_string()]));
        assert_eq!(held["dev"], BTreeSet::from(["member".to_string()]));
        // the record's lines from it: orch is orchestrator, and lead and text-reader; dev, a member with nothing besides,
        // has none
        assert_eq!(crate::runner::roles_now(false, [("orch".to_string(), "orchestrator".to_string()), ("dev".to_string(), String::new())].into_iter(), &held), vec![("orch".to_string(), "orchestrator".to_string(), vec!["lead".to_string(), "text-reader".to_string()])]);
        // without the line: the registry's roles alone, and nobody fills lead
        std::fs::remove_file(project.join("policy/roles.eflint")).unwrap();
        assert_eq!(fills(&places, facts.clone(), fields("lead", "triage")).unwrap().0, None);
        assert_eq!(roles_held(&places, facts).unwrap()["orch"], BTreeSet::from(["orchestrator".to_string()]));
        }
        let _ = std::fs::remove_dir_all(&project);
    }

    /// The coord's decider (paper-research N-568 p14): `fills` and `roles_held` answer the same under `solver`, `both`
    /// and `picker`, and under no file (`both`) and a word that is none of the three (`both`, journalled).
    #[test]
    fn fills_and_roles_held_answer_the_same_under_each_decider() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let dir = std::env::temp_dir().join(format!("docket-decider-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("project/policy")).unwrap();
        std::fs::create_dir_all(dir.join("coord")).unwrap();
        std::fs::write(dir.join("project/policy/roles.eflint"), "// whoever orchestrates leads\nExtend Fact role\n  Derived from role(agent, \"lead\") When role(agent, \"orchestrator\")\n").unwrap();
        let places = crate::policy::Places { base, project: Some(dir.join("project").display().to_string()), coord: dir.join("coord").display().to_string(), ..Default::default() };
        let mut facts = Vec::new();
        for (a, role, running) in [("orch", "orchestrator", true), ("dev", "fixer", false), ("dev2", "fixer", true)] {
            facts.push(("agent".to_string(), vec![a.to_string()]));
            facts.push(("role".to_string(), vec![a.to_string(), role.to_string()]));
            if running {
                facts.push(("running".to_string(), vec![a.to_string()]));
            }
        }
        facts.push(("ticket".to_string(), vec!["C-1".to_string()]));
        let fields = |r: &str| BTreeMap::from([("role".to_string(), r.to_string()), ("call".to_string(), "triage".to_string()), ("ticket".to_string(), "C-1".to_string())]);
        let mut answers = Vec::new();
        for word in [None, Some("solver"), Some("both"), Some("picker"), Some("clingo")] {
            match word {
                Some(w) => std::fs::write(dir.join("coord").join(crate::policy::DECIDER_FILE), format!("{w}\n")).unwrap(),
                None => assert!(!dir.join("coord").join(crate::policy::DECIDER_FILE).exists()),
            }
            let expect = match word {
                Some("solver") => crate::policy::Decider::Solver,
                Some("picker") => crate::policy::Decider::Picker,
                _ => crate::policy::Decider::Both,
            };
            assert_eq!(places.decider(), expect, "{word:?}");
            let lead = fills(&places, facts.clone(), fields("lead")).unwrap();
            let fixer = fills(&places, facts.clone(), fields("fixer")).unwrap();
            let none = fills(&places, facts.clone(), fields("reviewer")).unwrap();
            assert_eq!((lead.0.as_deref(), fixer.0.as_deref(), none.0.as_deref()), (Some("orch"), Some("dev2"), None), "{word:?}");
            answers.push((lead.1, fixer.1, none.1, roles_held(&places, facts.clone()).unwrap()));
        }
        assert_eq!(answers[0].3["orch"], BTreeSet::from(["orchestrator".to_string(), "lead".to_string(), "text-reader".to_string()]));
        for (a, word) in answers.iter().zip(["none", "solver", "both", "picker", "clingo"]) {
            assert_eq!(a, &answers[0], "{word}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An agent whose role is set back to none gets a `member` line when the record says otherwise (paper-research N-101
    /// p579): its old `orchestrator` line no longer makes it an orchestrator, nor, by the derived fact, a lead.
    #[test]
    fn a_role_set_back_to_none_is_recorded_as_member() {
        let mut m = Machine::load(Defs::default());
        m.apply(Input::Role { agent: "orchm".into(), role: "orchestrator".into(), also: vec!["lead".into()] }, "2026-11-16T12:00:00Z".into()).reply.unwrap();
        m.apply(Input::Role { agent: "dev".into(), role: "member".into(), also: vec![] }, "2026-11-16T12:00:01Z".into()).reply.unwrap();
        let names = ["orchm".to_string(), "dev".to_string(), "new".to_string()];
        // orchm now has no role of its own and none from the policy: roles_now gives it no line
        let want = crate::runner::roles_now(false, names.iter().map(|n| (n.clone(), String::new())), &BTreeMap::new());
        assert!(want.is_empty(), "{want:?}");
        let back = crate::runner::back_to_member(&m, names.iter(), &want);
        assert_eq!(back, vec![("orchm".to_string(), "member".to_string(), Vec::new())], "dev is a member already; new has no line");
        let (agent, role, also) = back[0].clone();
        m.apply(Input::Role { agent, role, also }, "2026-11-16T12:00:02Z".into()).reply.unwrap();
        assert!(m.holders("orchestrator").is_empty() && m.holders("lead").is_empty());
        assert!(crate::runner::back_to_member(&m, names.iter(), &want).is_empty(), "written once");
    }

    #[test]
    fn may_post_by_the_projects_norms() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let project = std::env::temp_dir().join(format!("docket-may-{}", std::process::id()));
        std::fs::create_dir_all(project.join("policy")).unwrap();
        std::fs::write(project.join("policy/posts.eflint"), "Act post\n  Actor agent\n  Related to ticket\n  Holds when opener(ticket, actor)\n").unwrap();
        let places = crate::policy::Places { base, project: Some(project.display().to_string()), coord: std::env::temp_dir().display().to_string(), ..Default::default() };
        let facts = |who: &str| vec![
            ("agent".to_string(), vec![who.to_string()]),
            ("ticket".to_string(), vec!["C-1".to_string()]),
            ("opener".to_string(), vec!["C-1".to_string(), "DEBUG".to_string()]),
        ];
        let ask = |who: &str| MayAsk { places: places.clone(), facts: facts(who), act: "post", who: who.into(), ticket: Some("C-1".into()), kind: "claim".into(), bound: true };
        let ok = |s: Result<Said, String>| s.map(|s| s.unchecked);
        let (yes, no) = (ok(may(ask("DEBUG"))), ok(may(ask("dev"))));
        std::fs::write(project.join("policy/posts.eflint"), "Act post\n  Actor agent\n  Holds when nonsense((\n").unwrap();
        let unread = ok(may(ask("dev")));
        let _ = std::fs::remove_dir_all(&project);
        assert_eq!(yes, Ok(None));
        assert!(matches!(unread, Ok(Some(_))), "{unread:?}");
        assert!(no.is_err_and(|e| e.starts_with("You may not post on C-1")));
    }

    #[test]
    fn may_edit_by_the_base_policy() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let places = crate::policy::Places { base, project: None, coord: std::env::temp_dir().display().to_string(), ..Default::default() };
        let f = |a: &str, b: &[&str]| (a.to_string(), b.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        let facts = vec![
            f("agent", &["orch"]), f("agent", &["dev"]), f("agent", &["sub"]), f("agent", &["other"]),
            f("role", &["orch", "lead"]), f("role", &["dev", "member"]), f("role", &["sub", "member"]), f("role", &["other", "member"]),
            f("parent", &["sub", "dev"]), f("above", &["dev", "sub"]),
            f("ticket", &["C-1"]), f("ticket-kind", &["C-1", "claim"]), f("opener", &["C-1", "sub"]),
        ];
        let ask = |who: &str| may(MayAsk { places: places.clone(), facts: facts.clone(), act: "edit", who: who.into(), ticket: Some("C-1".into()), kind: "claim".into(), bound: true });
        // the opener, an agent above it, a lead; nobody else; and nobody is told of an edit by `may`
        assert_eq!(ask("sub"), Ok(Said::default()));
        assert_eq!(ask("dev"), Ok(Said::default()));
        assert_eq!(ask("orch"), Ok(Said::default()));
        let no = ask("other");
        assert!(no.as_ref().is_err_and(|e| e.starts_with("You may not edit on C-1")), "{no:?}");
    }

    #[test]
    fn a_goal_holds_by_its_act_in_policy() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let project = std::env::temp_dir().join(format!("docket-goal-{}", std::process::id()));
        std::fs::create_dir_all(project.join("policy")).unwrap();
        std::fs::write(project.join("policy/goals.eflint"), "Act green\n  Actor agent\n  Related to ticket\n  Holds when closed-under(ticket, \"check\", \"Green\")\n").unwrap();
        let places = crate::policy::Places { base, project: Some(project.display().to_string()), coord: std::env::temp_dir().display().to_string(), ..Default::default() };
        let f = |a: &str, b: &[&str]| (a.to_string(), b.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        let mut facts = vec![f("agent", &["dev"]), f("ticket", &["C-1"]), f("closed-under", &["C-1", "check", "Red"])];
        let red = holds(&places, &facts, "dev", "C-1", &["green".into(), "audited".into()]);
        facts.push(f("closed-under", &["C-1", "check", "Green"]));
        let green = holds(&places, &facts, "dev", "C-1", &["green".into()]);
        let _ = std::fs::remove_dir_all(&project);
        // red: green does not hold, and a goal with no act does not either
        assert_eq!(red.unwrap().0, vec!["green".to_string(), "audited".to_string()]);
        assert_eq!(green.unwrap().0, Vec::<String>::new());
    }

    /// A check_end answers a check only when its name is the check's own id; a runner's own check whose name holds
    /// the same number (Tovrin's `printer-1`) is nothing to the tickets (paper-research N-27 post 35, 3).
    #[test]
    fn a_check_end_answers_only_the_ticket_it_names_by_its_id() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-check-end-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        let project = coord.join("files");
        std::fs::create_dir_all(project.join("kinds")).unwrap();
        std::fs::write(project.join("kinds/g.kind"), "kind guarded\n  letter G\n  form what: Text, commit: Ref\n  result Text\n  pattern\n    c <- open check(commit, none);\n    return what\n").unwrap();
        std::fs::write(project.join("project.toml"), "[project]\nname = \"g\"\n").unwrap();
        std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        core.load_tickets();
        core.ticket_input(Input::Open { kind: "guarded".into(), under: None, opener: "owner".into(), form: json!({"what": "w", "commit": "abc"}), text: None, names: vec![], told: vec![] }).unwrap();
        let k = core.tickets.m.tickets.values().find(|x| x.kind == "check").map(|x| x.t).expect("a check");
        let id = core.tickets.m.id(k);
        let awaits = |core: &Core| matches!(&core.tickets.m.tickets[&k].status, docket_machine::Status::Awaits { agent, .. } if agent == "runner");
        assert!(awaits(&core), "{:?}", core.tickets.m.tickets[&k].status);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let lines = |coord: &std::path::Path| std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap().lines().filter(|l| !l.contains("\"ev\":\"event\"")).count();
        let before = lines(&coord);
        // the runner's own check, its name holding the check's number
        let own = format!("printer-{k}");
        assert_eq!(docket_machine::parse_ticket(&own), Some(k));
        rt.block_on(core.project_event(json!({"event": "check_end", "name": own, "result": "green", "reason": ""})));
        assert!(awaits(&core), "the runner's own check answers nothing");
        assert_eq!(lines(&coord), before, "and writes no ticket line but the event");
        // named by its id: answered
        rt.block_on(core.project_event(json!({"event": "check_end", "name": id, "result": "green", "reason": "ok"})));
        assert!(!core.tickets.m.tickets[&k].is_open(), "{:?}", core.tickets.m.tickets[&k].status);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// The runner's contract, coordd's half (N-101): a step of any call but `check` goes out as `RUNNER step TICKET --to
    /// AGENT CALL 'ARGS-AS-JSON'`; its `step_end` answers it when it names the ticket and the call it owes, held to the
    /// call's answer type; `outcome` names its ticket as `name` too.
    #[test]
    fn a_runner_step_goes_out_with_its_call_and_arguments_and_step_end_answers_it() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-runner-step-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        let project = coord.join("files");
        std::fs::create_dir_all(project.join("kinds")).unwrap();
        std::fs::create_dir_all(project.join("calls")).unwrap();
        std::fs::write(project.join("calls/h.call"), "call hand_in(commit: Ref) -> OnLine(top: Ref) | Red(errors: Text)\n  template\n    Check {{ commit }}.\n").unwrap();
        std::fs::write(project.join("kinds/h.kind"), "kind piece\n  letter H\n  form c: Ref\n  result Ref\n  pattern\n    v <- step runner hand_in(c);\n    match v {\n      OnLine(top) => return top\n      Red(e) => return c\n    }\n\nkind change\n  letter G\n  form cs: [Ref]\n  result [Ref]\n  raises Exhausted\n  pattern\n    tops <- par c in cs up to 4 {\n      v <- step runner hand_in(c);\n      match v {\n        OnLine(top) => return top\n        Red(e) => return c\n      }\n    };\n    return tops\n").unwrap();
        std::fs::write(project.join("runner.sh"), "#!/bin/sh\nprintf '%s\\n' \"$@\" > args.txt\n").unwrap();
        std::fs::write(project.join("project.toml"), "[project]\nname = \"h\"\nrunner = \"sh runner.sh\"\n").unwrap();
        std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        core.load_tickets();
        assert!(core.tickets.errors.is_empty(), "{:?}", core.tickets.errors);
        core.ticket_input(Input::Open { kind: "piece".into(), under: None, opener: "owner".into(), form: json!({"c": "abc123"}), text: None, names: vec![], told: vec![] }).unwrap();
        let k = core.tickets.m.tickets.values().find(|x| x.kind == "piece").map(|x| x.t).unwrap();
        let id = core.tickets.m.id(k);
        let d = core.ticket_due("runner");
        assert_eq!(d.len(), 1, "{d:?}");
        crate::project::to_runner(&project.display().to_string(), "sh runner.sh", &d[0], &core.runner_place(k)).unwrap();
        let args = std::fs::read_to_string(project.join("args.txt")).unwrap();
        // the work it is for, `of` (N-101 p166): the piece itself
        let json = format!("{{\"commits\":\"abc123\",\"of\":\"{id}\"}}");
        assert_eq!(args.lines().collect::<Vec<_>>(), vec!["step", id.as_str(), "--to", "runner", "hand_in", json.as_str()]);
        // with a root in the project file, every step carries it as its line (N-159 post 25)
        assert_eq!(core.runner_place(k).line, None);
        core.project.as_mut().unwrap().project.root = Some("trial/one-engine".into());
        crate::project::to_runner(&project.display().to_string(), "sh runner.sh", &d[0], &core.runner_place(k)).unwrap();
        let args = std::fs::read_to_string(project.join("args.txt")).unwrap();
        assert_eq!(args.lines().last(), Some(format!("{{\"commits\":\"abc123\",\"line\":\"trial/one-engine\",\"of\":\"{id}\"}}").as_str()));
        // a runner that refuses: the step is told the runner's own message, its last line (main.rs gives it to the machine as
        // RunnerRefused, which the step raises: docket-machine tests/runner_refused.rs; N-159 post 43)
        std::fs::write(project.join("runner.sh"), "#!/bin/sh\necho 'coord-runner step: working' >&2\necho 'coord-runner step: refused: abc123 is no commit of a' >&2\nexit 1\n").unwrap();
        let n = crate::project::to_runner(&project.display().to_string(), "sh runner.sh", &d[0], &core.runner_place(k)).unwrap_err();
        assert_eq!(n.why, "coord-runner step: refused: abc123 is no commit of a");
        assert!(n.ran && n.journal.contains("sh runner.sh step") && n.journal.contains("working"), "{n:?}");
        // a runner command that cannot run at all (not there): not ran, so the step is held back, not raised
        let n = crate::project::to_runner(&project.display().to_string(), "./no-such-runner", &d[0], &core.runner_place(k)).unwrap_err();
        assert!(!n.ran, "{n:?}");
        // killed before it could answer: not ran either
        std::fs::write(project.join("runner.sh"), "#!/bin/sh\nkill -9 $$\n").unwrap();
        let n = crate::project::to_runner(&project.display().to_string(), "sh runner.sh", &d[0], &core.runner_place(k)).unwrap_err();
        assert!(!n.ran, "{n:?}");
        std::fs::write(project.join("runner.sh"), "#!/bin/sh\nprintf '%s\\n' \"$@\" > args.txt\n").unwrap();
        core.runner_held("runner", &d[0], &n.why);
        assert!(core.due_now("runner").is_empty(), "held back until the runner's command runs");
        core.runner_again();
        assert_eq!(core.due_now("runner").len(), 1);
        std::fs::write(project.join("runner.sh"), "#!/bin/sh\nprintf '%s\\n' \"$@\" > args.txt\n").unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let open = |core: &Core| core.tickets.m.tickets[&k].is_open();
        // another call's answer, and one that does not fit the call's answers: refused, the step still owed
        rt.block_on(core.project_event(json!({"event": "step_end", "name": id, "call": "cases", "head": "Green", "top": "t"})));
        assert!(open(&core));
        rt.block_on(core.project_event(json!({"event": "step_end", "name": id, "call": "hand_in", "head": "Landed"})));
        assert!(open(&core));
        rt.block_on(core.project_event(json!({"event": "step_end", "name": id, "call": "hand_in", "head": "OnLine", "top": "def456"})));
        assert!(matches!(&core.tickets.m.tickets[&k].status, docket_machine::Status::Done(v) if v == "def456"), "{:?}", core.tickets.m.tickets[&k].status);
        // a step inside a par runs on the engine's own branch ticket: `of` names the change it is part of
        core.ticket_input(Input::Open { kind: "change".into(), under: None, opener: "owner".into(), form: json!({"cs": ["d00d"]}), text: None, names: vec![], told: vec![] }).unwrap();
        let g = core.tickets.m.tickets.values().find(|x| x.kind == "change").map(|x| x.t).unwrap();
        let branch = core.ticket_due("runner").into_iter().find(|x| x.t != k).expect("the branch's hand_in");
        assert_eq!(core.tickets.m.tickets[&branch.t].kind, "part");
        crate::project::to_runner(&project.display().to_string(), "sh runner.sh", &branch, &core.runner_place(branch.t)).unwrap();
        let args = std::fs::read_to_string(project.join("args.txt")).unwrap();
        let gid = core.tickets.m.id(g);
        assert_eq!(args.lines().nth(1), Some(branch.ticket.as_str()), "answered by its own ticket");
        assert_eq!(args.lines().last(), Some(format!("{{\"commits\":\"d00d\",\"line\":\"trial/one-engine\",\"of\":\"{gid}\"}}").as_str()));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A goal on a ticket the owner opened (live round: G-10 failed Owed with its check Green below it): the owner is
    /// the actor, and no agent of the team's, but the goal holds.
    #[test]
    fn a_goal_holds_on_a_ticket_the_owner_opened() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-goal-owner-{}", std::process::id()));
        let project = coord.join("files");
        std::fs::create_dir_all(project.join("policy")).unwrap();
        std::fs::create_dir_all(project.join("kinds")).unwrap();
        std::fs::write(project.join("policy/goals.eflint"), "Act checked\n  Actor agent\n  Related to ticket\n  Holds when closed-under(ticket, \"check\", \"Green\")\n").unwrap();
        std::fs::write(project.join("kinds/g.kind"), "kind guarded\n  letter G\n  form what: Text, commit: Ref\n  result Text\n  goal checked\n  pattern\n    c <- open check(commit, none);\n    return what\n").unwrap();
        std::fs::write(project.join("project.toml"), "[project]\nname = \"g\"\n").unwrap();
        std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        core.load_tickets();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.tickets.tasks = Some(tx);
        let r = core.ticket_input(Input::Open { kind: "guarded".into(), under: None, opener: "owner".into(), form: json!({"what": "w", "commit": "abc"}), text: None, names: vec![], told: vec![] });
        assert!(r.is_ok(), "{r:?}");
        let find = |core: &Core, kind: &str| core.tickets.m.tickets.values().find(|x| x.kind == kind).map(|x| x.t).expect(kind);
        let (g, k) = (core.tickets.m.id(find(&core, "guarded")), find(&core, "check"));
        let r = core.ticket_input(Input::Answer { t: k, by: "runner".into(), head: json!({"head": "Green"}), text: docket_machine::Text::New("ok".into()) });
        assert!(r.is_ok(), "{r:?}");
        let task = std::iter::from_fn(|| rx.try_recv().ok()).find(|t| matches!(t, Task::Holds { .. })).expect("the goal is asked");
        let Task::Holds { ticket, actor, goals, facts, places, .. } = task else { unreachable!() };
        assert_eq!((ticket.as_str(), actor.as_str()), (g.as_str(), "owner"));
        let h = holds(&places, &facts, &actor, &ticket, &goals).unwrap();
        let _ = std::fs::remove_dir_all(&coord);
        assert_eq!(h.0, Vec::<String>::new(), "{:?} {facts:?}", h.1);
    }

    #[test]
    fn notify_tells_whom_the_policy_adds() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let project = std::env::temp_dir().join(format!("docket-notify-{}", std::process::id()));
        std::fs::create_dir_all(project.join("policy")).unwrap();
        std::fs::write(project.join("policy/notify.eflint"), "Act notify\n  Actor agent\n  Recipient agent\n  Related to ticket, event\n  Holds when event == \"post\" and ticket-kind(ticket, \"claim\") and role(recipient, \"lead\")\n").unwrap();
        let places = crate::policy::Places { base, project: Some(project.display().to_string()), coord: std::env::temp_dir().display().to_string(), ..Default::default() };
        let facts = vec![
            ("agent".to_string(), vec!["dev".to_string()]),
            ("agent".to_string(), vec!["orch".to_string()]),
            ("role".to_string(), vec!["dev".to_string(), "member".to_string()]),
            ("role".to_string(), vec!["orch".to_string(), "lead".to_string()]),
            ("ticket".to_string(), vec!["C-1".to_string()]),
            ("ticket-kind".to_string(), vec!["C-1".to_string(), "claim".to_string()]),
        ];
        let said = may(MayAsk { places: places.clone(), facts: facts.clone(), act: "post", who: "dev".into(), ticket: Some("C-1".into()), kind: "claim".into(), bound: true });
        // the lead posting is not told of its own post
        let own = may(MayAsk { places: places.clone(), facts: facts.clone(), act: "post", who: "orch".into(), ticket: Some("C-1".into()), kind: "claim".into(), bound: true });
        // a close is no post
        let closed = run_task(&crate::policy::Engine::start(places.clone()), Task::Notify { t: 1, event: "closed".into(), actor: "dev".into(), post: None, ticket: "C-1".into(), facts, places });
        let _ = std::fs::remove_dir_all(&project);
        assert_eq!(said, Ok(Said { unchecked: None, told: vec!["orch".into()], inform: vec![] }));
        assert_eq!(own, Ok(Said::default()));
        assert!(matches!(closed.as_slice(), [Input::Notified { to, .. }] if to.is_empty()), "{closed:?}");
    }

    /// agent-base's notify.eflint (paper-research N-101 p939): a ticket with no parent that closes by its program's own
    /// raise is told to the lead (coordd gives `closed-failed` on its close); one that closes done is told to nobody.
    #[test]
    fn a_top_level_ticket_that_closes_failed_wakes_the_lead_and_one_done_nobody() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-closed-failed-{}", std::process::id()));
        let project = coord.join("files");
        std::fs::create_dir_all(project.join("kinds")).unwrap();
        std::fs::write(project.join("kinds/k.kind"), "kind failing\n  letter X\n  result Text\n  raises Gone\n  pattern\n    raise Gone(\"no\")\n\nkind fine\n  letter Y\n  result Text\n  pattern\n    return \"ok\"\n").unwrap();
        std::fs::write(project.join("project.toml"), "[project]\nname = \"f\"\n").unwrap();
        std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        core.load_tickets();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.tickets.tasks = Some(tx);
        let mut told = Vec::new();
        for kind in ["failing", "fine"] {
            let r = core.ticket_input(Input::Open { kind: kind.into(), under: None, opener: "owner".into(), form: json!({}), text: None, names: vec!["owner".into()], told: vec![] });
            assert!(r.is_ok(), "{r:?}");
            let task = std::iter::from_fn(|| rx.try_recv().ok()).find(|t| matches!(t, Task::Notify { .. })).expect("the close is asked");
            let Task::Notify { t, event, actor, post, ticket, mut facts, places } = task else { unreachable!() };
            assert_eq!(facts.iter().any(|(f, v)| f == "closed-failed" && v == &vec![ticket.clone()]), kind == "failing", "{facts:?}");
            facts.push(("agent".into(), vec!["orch".into()]));
            facts.push(("role".into(), vec!["orch".into(), "lead".into()]));
            let out = run_task(&crate::policy::Engine::start(places.clone()), Task::Notify { t, event, actor, post, ticket, facts, places });
            let [Input::Notified { to, .. }] = out.as_slice() else { panic!("{out:?}") };
            told.push(to.clone());
        }
        let _ = std::fs::remove_dir_all(&coord);
        assert_eq!(told, vec![vec!["orch".to_string()], Vec::new()]);
    }

    /// A copy of a coord's record loaded as coordd loads it, with agent-base REPLAY_BASE (paper-research N-101.p1308:
    /// before an engine hotfix goes to prod, prod's records load with it with no "now makes" and no "now refused").
    /// `REPLAY_COORD=DIR REPLAY_BASE=DIR [REPLAY_PROJECT=DIR [REPLAY_LIBRARY=DIR]] cargo test -p coordd a_coords_record_replays -- --ignored --nocapture`.
    /// It only reads REPLAY_COORD (its tickets folder made if missing; nothing written in it).
    #[test]
    #[ignore]
    /// Piece 1's trial on a copy of a record (paper-research N-101 p1676; N-233.p244): TRIAL_COORD's record loaded,
    /// TRIAL_TIES ("T:P,T:P,…", P `-` to untie) applied by the owner, then a loop, a tie by an agent with no say, a
    /// title; the outlines printed; and the copy with the new lines written to TRIAL_OUT/tickets/log.jsonl loaded again.
    /// `TRIAL_COORD=DIR TRIAL_OUT=DIR REPLAY_BASE=DIR [REPLAY_PROJECT=DIR] TRIAL_TIES=… TRIAL_LOOP=T:P TRIAL_TITLE=T:TEXT
    /// cargo test -p coordd ties_on_a_coords_record -- --ignored --nocapture`. It only reads TRIAL_COORD.
    fn ties_on_a_coords_record() {
        let (Ok(coord), Ok(out), Ok(base)) = (std::env::var("TRIAL_COORD"), std::env::var("TRIAL_OUT"), std::env::var("REPLAY_BASE")) else { return };
        let project = std::env::var("REPLAY_PROJECT").ok();
        let t = Tickets::load(&coord, &base, project.as_deref(), 1);
        println!("loaded {coord}: {} problem lines", t.errors.len());
        let mut m = t.m;
        let mut new: Vec<docket_machine::Line> = Vec::new();
        let clock = std::cell::Cell::new(0u32);
        let mut go = |m: &mut docket_machine::Machine, what: String, i: Input| {
            clock.set(clock.get() + 1);
            let o = m.apply(i, format!("2026-11-17T12:00:{:02}Z", clock.get()));
            println!("  {what}: {:?} ({} line)", o.reply, o.lines.len());
            new.extend(o.lines);
        };
        let id = |s: &str| docket_machine::parse_ticket(s).unwrap_or_else(|| panic!("no ticket {s}"));
        for pair in std::env::var("TRIAL_TIES").unwrap_or_default().split(',').filter(|s| !s.is_empty()) {
            let (a, b) = pair.split_once(':').unwrap();
            go(&mut m, format!("coord tie {a} {b}"), Input::Tie { t: id(a), to: (b != "-").then(|| id(b)), by: "owner".into() });
        }
        if let Some((a, b)) = std::env::var("TRIAL_LOOP").ok().as_deref().and_then(|s| s.split_once(':')) {
            go(&mut m, format!("coord tie {a} {b}"), Input::Tie { t: id(a), to: Some(id(b)), by: "owner".into() });
            go(&mut m, format!("coord tie {a} {b} (by trial-agent, no role)"), Input::Tie { t: id(a), to: Some(id(b)), by: "trial-agent".into() });
        }
        if let Some((a, text)) = std::env::var("TRIAL_TITLE").ok().as_deref().and_then(|s| s.split_once(':')) {
            go(&mut m, format!("coord title {a} {text}"), Input::Title { t: id(a), label: text.into(), by: "owner".into() });
        }
        let show = |m: &docket_machine::Machine| {
            let tops: Vec<u64> = std::env::var("TRIAL_SHOW").unwrap_or_default().split(',').filter(|s| !s.is_empty()).map(id).collect();
            for t in tops {
                let o = docket_machine::ViewOutline::of(m, &m.tickets[&t], true);
                println!("  outline {}: {}", o.id, serde_json::to_string(&o).unwrap().chars().take(1500).collect::<String>());
            }
        };
        show(&m);
        // the copy with the trial's lines, loaded again as coordd loads it
        std::fs::create_dir_all(format!("{out}/tickets")).unwrap();
        let mut text = std::fs::read_to_string(format!("{coord}/tickets/log.jsonl")).unwrap();
        for l in &new {
            text.push_str(&serde_json::to_string(l).unwrap());
            text.push('\n');
        }
        std::fs::write(format!("{out}/tickets/log.jsonl"), text).unwrap();
        let again = Tickets::load(&out, &base, project.as_deref(), 1);
        let makes = again.errors.iter().filter(|e| e.contains("now makes")).count();
        let refused = again.errors.iter().filter(|e| e.contains("now refused")).count();
        println!("reloaded with {} new lines: {} problem lines, {makes} now makes, {refused} now refused", new.len(), again.errors.len());
        for e in again.errors.iter().take(8) {
            println!("  {}", e.chars().take(200).collect::<String>());
        }
        show(&again.m);
        // TRIAL_OPEN / TRIAL_CLOSED: the threads `coord outline` lists, and those it leaves out (closed ones)
        let rows = |m: &docket_machine::Machine| -> Vec<String> { m.tickets.values().filter(|x| x.t < docket_machine::HIDDEN && docket_machine::view::outlined(m, x)).map(|x| docket_machine::ViewOutline::of(m, x, false)).filter(|o| o.open).map(|o| o.id).collect() };
        let listed = rows(&again.m);
        for id in std::env::var("TRIAL_OPEN").unwrap_or_default().split(',').filter(|s| !s.is_empty()) {
            assert!(listed.iter().any(|l| l == id), "{id} is open work: listed");
        }
        for id in std::env::var("TRIAL_CLOSED").unwrap_or_default().split(',').filter(|s| !s.is_empty()) {
            assert!(!listed.iter().any(|l| l == id), "{id} is closed: not listed");
        }
        assert!(std::env::var("TRIAL_SHOW").unwrap_or_default().split(',').filter(|s| !s.is_empty()).all(|s| {
            let t = id(s);
            again.m.view(t).map(|v| v.outline) == m.view(t).map(|v| v.outline) && docket_machine::ViewOutline::of(&again.m, &again.m.tickets[&t], true) == docket_machine::ViewOutline::of(&m, &m.tickets[&t], true)
        }), "the outlines as before the reload");
    }

    /// The views a coordd on a copy of a record would send the app, `outline` filled as `ticket_shown` fills it
    /// (docket-edge, N-397: the stills from the real fields when no coord may be started): VIEWS_COORD's record loaded,
    /// every open thread's view and those of the threads under them written to VIEWS_OUT as one JSON list.
    /// `VIEWS_COORD=DIR VIEWS_OUT=FILE REPLAY_BASE=DIR cargo test -p coordd views_of_a_coords_record -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn views_of_a_coords_record() {
        let (Ok(coord), Ok(out), Ok(base)) = (std::env::var("VIEWS_COORD"), std::env::var("VIEWS_OUT"), std::env::var("REPLAY_BASE")) else { return };
        let t = Tickets::load(&coord, &base, std::env::var("REPLAY_PROJECT").ok().as_deref(), 1);
        println!("loaded {coord}: {} problem lines", t.errors.len());
        let m = &t.m;
        let mut want: std::collections::BTreeSet<u64> = m.tickets.values().filter(|x| docket_machine::view::outlined(m, x) && x.is_open()).map(|x| x.t).collect();
        for x in m.tickets.values().filter(|x| docket_machine::view::outlined(m, x)) {
            if let Some((p, _)) = m.outline_parent(x.t) {
                want.insert(p);
                want.insert(x.t);
            }
        }
        // VIEWS_ALL=1: the snapshot's tickets instead, the open ones and those closed in the day before the record's
        // last line (`ticket_views`), as the app is sent them (docket-edge2, N-412.p42: the agents' pages timed on
        // prod's own record)
        if std::env::var("VIEWS_ALL").is_ok() {
            let last = m.tickets.values().flat_map(|x| x.posts.iter().map(|p| p.at.clone()).chain(x.closed_at.clone())).max().unwrap_or_default();
            let day_ago = chrono::DateTime::parse_from_rfc3339(&last).map(|t| (t - chrono::Duration::days(1)).to_utc().format("%Y-%m-%dT%H:%M:%SZ").to_string()).unwrap_or_default();
            want = m.tickets.values().filter(|x| x.is_open() || x.kind == "item" || x.closed_at.as_ref().is_some_and(|c| *c >= day_ago)).filter(|x| x.handling.is_none()).map(|x| x.t).collect();
        }
        let views: Vec<TicketView> = want.into_iter().filter_map(|n| {
            let mut v = m.view(n)?;
            v.outline = m.tickets.get(&n).map(|x| docket_machine::view::ViewOutline::of(m, x, true));
            Some(v)
        }).collect();
        println!("{} views to {out}", views.len());
        std::fs::write(&out, serde_json::to_string(&views).unwrap()).unwrap();
    }

    /// What waits on the owner in a copy of a record (paper-research N-461: the outline's "47 wait on you"): every
    /// ticket whose outline line is owed (the outline counts the open threads of these) and every post whose asks wait
    /// on him, each as one JSON line to ASKS_OUT: the ticket, its kind, label and state, whether the outline counts it,
    /// and per ask its post, asker, time and the first line of what it asks (its first mark; else the post's).
    /// `ASKS_COORD=DIR ASKS_OUT=FILE REPLAY_BASE=DIR [REPLAY_PROJECT=DIR] cargo test -p coordd owners_waits_of_a_coords_record -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn owners_waits_of_a_coords_record() {
        let (Ok(coord), Ok(out), Ok(base)) = (std::env::var("ASKS_COORD"), std::env::var("ASKS_OUT"), std::env::var("REPLAY_BASE")) else { return };
        let t = Tickets::load(&coord, &base, std::env::var("REPLAY_PROJECT").ok().as_deref(), 1);
        println!("loaded {coord}: {} problem lines", t.errors.len());
        let m = &t.m;
        let mut lines = Vec::new();
        for x in m.tickets.values() {
            let o = docket_machine::view::ViewOutline::of(m, x, false);
            let waiting = m.asks_waiting(x);
            let owner_leaves: Vec<Value> = m.leaves(x.t).into_iter().filter(|(_, a, _)| a == "owner").map(|(u, _, c)| json!({"ticket": m.id(u), "call": c})).collect();
            if !o.owed && waiting.is_empty() && owner_leaves.is_empty() {
                continue;
            }
            let v = m.view(x.t);
            let asks: Vec<Value> = waiting.iter().map(|(by, n)| {
                let p = v.as_ref().and_then(|v| v.posts.iter().find(|p| p.n == *n));
                let words = p.map(|p| crate::render::resolve(&p.text)).unwrap_or_default();
                let marks = p.map(|p| p.asks.clone()).unwrap_or_default();
                let ask = marks.first().and_then(|k| words.get(k.at[0]..k.at[1])).unwrap_or(&words);
                let first = ask.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").chars().take(200).collect::<String>();
                json!({"post": n, "by": by, "at": p.map(|p| p.at.clone()), "marks": marks.len(), "first": first})
            }).collect();
            lines.push(json!({
                "ticket": m.id(x.t), "kind": x.kind, "label": o.label, "state": o.state, "open": o.open, "owed": o.owed,
                "counted": x.kind == "thread" && o.open && o.owed, "status": format!("{:?}", x.status).chars().take(40).collect::<String>(),
                "closed_at": x.closed_at, "asks": asks, "owner_leaves": owner_leaves,
            }).to_string());
        }
        println!("{} tickets wait on the owner, to {out}", lines.len());
        std::fs::write(&out, lines.join("\n") + "\n").unwrap();
    }

    /// The posts a record copy owes again under `read_own`'s widened rule (paper-research N-560 p10): each post due to an
    /// agent now (its deliveries as the machine makes them) that the agent itself posted past on the same ticket, and
    /// that was not an owner's waiting post (already kept before). One TSV line each: agent, post id, its writer, when
    /// written, the agent's first post past it, the ticket open or closed, and the delivery's kind; then the counts.
    /// `REPLAY_COORD=DIR REPLAY_BASE=DIR [REPLAY_PROJECT=FILE] cargo test -p coordd posts_owed_again_on_a_coords_record
    /// -- --ignored --nocapture`. It only reads REPLAY_COORD.
    #[test]
    #[ignore]
    fn posts_owed_again_on_a_coords_record() {
        let (Ok(coord), Ok(base)) = (std::env::var("REPLAY_COORD"), std::env::var("REPLAY_BASE")) else { return };
        let t = Tickets::load(&coord, &base, std::env::var("REPLAY_PROJECT").ok().as_deref(), 1);
        println!("replay {coord}: {} problem lines", t.errors.len());
        let m = &t.m;
        let agents: BTreeSet<String> = m.tickets.values().flat_map(|x| x.posts.iter().flat_map(|p| p.wakes().cloned())).filter(|a| a != "owner").collect();
        let mut rows = Vec::new();
        for a in &agents {
            for d in m.due(a) {
                let unread = match &d.what {
                    docket_machine::What::Posts { unread } | docket_machine::What::Closed { unread, .. } => unread,
                    docket_machine::What::Step { unread, .. } => unread,
                    _ => continue,
                };
                let x = &m.tickets[&d.t];
                for p in unread.iter().filter(|p| p.by != *a && !x.waits.contains(&p.n) && p.wakes().any(|w| w == a)) {
                    let Some(q) = x.posts.iter().find(|q| q.n > p.n && q.by == *a) else { continue };
                    rows.push(format!("{a}\t{}.p{}\t{}\t{}\t{}.p{} at {}\t{}\t{}", d.ticket, p.n, p.by, p.at, d.ticket, q.n, q.at, if x.is_open() { "open" } else { "closed" }, d.id.rsplit('.').next().unwrap_or("")));
                }
            }
        }
        for r in &rows {
            println!("{r}");
        }
        println!("{} posts owed again, to {} agents", rows.len(), rows.iter().map(|r| r.split('\t').next().unwrap()).collect::<BTreeSet<_>>().len());
    }

    /// The posts an agent's own post passed without its ever being shown them, as the record stands (paper-research
    /// N-560 p10): run with the machine from before `read_own`'s widened rule, whose deliveries keep the read marks of
    /// the time (`GivenRec.before`). A post P that names or tells agent X (not X's own, not typed into its terminal),
    /// with a later post of X's on the ticket, that no delivery to X showed (`before < P <= upto`). An owner's put into
    /// X's running turn only from the record's first giving into a turn (before it such a post had no line), and only
    /// once X's mark passed it: from the era ERA_HIS_HANDED `read_own` reads his only once handed (consult-3 N-1479
    /// p151), so these are what that era gives now, "given after" when the delivery P names went into the turn after
    /// X's post, its range empty, else "never". One TSV line each: X, P's id, its writer, when, X's first post past it,
    /// the minutes between, for his which; then the counts by day, by gap, by agent and of his. Same variables as
    /// `posts_owed_again_on_a_coords_record`.
    #[test]
    #[ignore]
    fn posts_skipped_on_a_coords_record() {
        let (Ok(coord), Ok(base)) = (std::env::var("REPLAY_COORD"), std::env::var("REPLAY_BASE")) else { return };
        let t = Tickets::load(&coord, &base, std::env::var("REPLAY_PROJECT").ok().as_deref(), 1);
        println!("replay {coord}: {} problem lines", t.errors.len());
        let log = std::fs::read_to_string(format!("{coord}/tickets/log.jsonl")).unwrap_or_default();
        let into_from = log.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).find(|v| v["ev"] == "given" && v["into"] == true).and_then(|v| v["at"].as_str().map(str::to_string));
        println!("the first giving into a turn: {into_from:?}");
        let secs = |a: &str| chrono::DateTime::parse_from_rfc3339(a).map(|d| d.timestamp()).unwrap_or(0);
        let (mut rows, mut by_day, mut by_gap, mut by_agent, mut his_by) = (Vec::new(), BTreeMap::<String, u32>::new(), BTreeMap::<&str, u32>::new(), BTreeMap::<String, u32>::new(), BTreeMap::<&str, u32>::new());
        for x in t.m.tickets.values() {
            let id = t.m.id(x.t);
            for p in x.posts.iter().filter(|p| p.moved.is_none() && !p.dropped) {
                let his = p.by == "owner" && !x.waits.contains(&p.n);
                if his && into_from.as_ref().is_none_or(|f| p.at < *f) {
                    continue;
                }
                for a in p.wakes().filter(|a| **a != p.by && *a != "owner" && p.typed.as_deref() != Some(a.as_str())) {
                    let Some(q) = x.posts.iter().find(|q| q.n > p.n && q.by == *a) else { continue };
                    if x.given.iter().any(|g| g.to == *a && g.before < p.n && p.n <= g.upto) {
                        continue;
                    }
                    let how = if !his {
                        ""
                    } else if x.mu.get(a.as_str()).copied().unwrap_or(0) < p.n {
                        continue;
                    } else if x.given.iter().any(|g| g.to == *a && g.delivery == format!("{id}.p{}", p.n)) {
                        "given after"
                    } else {
                        "never"
                    };
                    let gap = (secs(&q.at) - secs(&p.at)).max(0) / 60;
                    rows.push(format!("{a}\t{id}.p{}\t{}\t{}\t{id}.p{} at {}\t{gap}\t{how}", p.n, p.by, p.at, q.n, q.at));
                    *by_day.entry(p.at.chars().take(10).collect()).or_default() += 1;
                    *by_gap.entry(if gap < 1 { "under 1 min" } else if gap < 10 { "1-10 min" } else { "10 min or more" }).or_default() += 1;
                    *by_agent.entry(a.clone()).or_default() += 1;
                    if his {
                        *his_by.entry(how).or_default() += 1;
                    }
                }
            }
        }
        rows.sort();
        for r in &rows {
            println!("{r}");
        }
        println!("{} posts skipped; by day {by_day:?}; by gap {by_gap:?}; by agent {by_agent:?}; his {his_by:?}", rows.len());
    }

    /// Where a load's replay of a coord's record goes (paper-research N-853): reading the definitions, reading the file,
    /// parsing its lines, the machine's replay by the kind of input (the time between one input's end and the next's,
    /// with its recorded outputs compared), and what `load` does after it. REPLAY_COORD (a copy), REPLAY_BASE,
    /// REPLAY_PROJECT, as `a_coords_record_replays`. One table, the slowest kinds first.
    #[test]
    #[ignore]
    fn where_a_coords_replay_goes() {
        let (Ok(coord), Ok(base)) = (std::env::var("REPLAY_COORD"), std::env::var("REPLAY_BASE")) else { return };
        let project = std::env::var("REPLAY_PROJECT").ok();
        let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
        let all = std::time::Instant::now();
        let at = std::time::Instant::now();
        let (defs, errors, _, _) = Tickets::read_defs(&crate::policy::Places { base: base.clone(), project: project.clone(), coord: coord.clone(), ..Default::default() }, 1);
        let defs_ms = ms(at.elapsed());
        let at = std::time::Instant::now();
        let text = std::fs::read_to_string(format!("{coord}/tickets/log.jsonl")).unwrap();
        let read_ms = ms(at.elapsed());
        let at = std::time::Instant::now();
        let lines: Vec<Line> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        let parse_ms = ms(at.elapsed());
        let at = std::time::Instant::now();
        let mut m = Machine::load(defs);
        let machine_ms = ms(at.elapsed());
        let kind = |l: &Line| serde_json::to_value(l).ok().and_then(|v| v.get("ev").and_then(Value::as_str).map(str::to_string)).unwrap_or_default();
        let mut by: BTreeMap<String, (usize, f64, f64)> = BTreeMap::new();
        let mut last = std::time::Instant::now();
        let replay_at = last;
        let problems = m.replay_with(&lines, &mut |_, _, here| {
            let took = ms(last.elapsed());
            let e = by.entry(kind(&here[0])).or_default();
            e.0 += 1;
            e.1 += took;
            e.2 = e.2.max(took);
            last = std::time::Instant::now();
        });
        let replay_ms = ms(replay_at.elapsed());
        let counted: f64 = by.values().map(|e| e.1).sum();
        let at = std::time::Instant::now();
        let mut quiet = quiet::Quiet::default();
        lines.iter().for_each(|l| quiet.take(l));
        let quiet_ms = ms(at.elapsed());
        println!("where {coord}: {} lines, {} inputs, {} problems, {} definition errors", lines.len(), by.values().map(|e| e.0).sum::<usize>(), problems.len(), errors.len());
        println!("part\tms");
        for (part, t) in [("definitions read", defs_ms), ("file read", read_ms), ("lines parsed", parse_ms), ("machine made", machine_ms), ("machine replay", replay_ms), ("  of it, outside the inputs (the replay's first pass, its end)", replay_ms - counted), ("turn ends and jobs (quiet)", quiet_ms), ("whole", ms(all.elapsed()))] {
            println!("{part}\t{t:.0}");
        }
        let mut rows: Vec<_> = by.into_iter().collect();
        rows.sort_by(|a, b| b.1.1.total_cmp(&a.1.1));
        println!("input\tcount\tms\tmean ms\tmax ms");
        for (k, (n, t, max)) in rows {
            println!("{k}\t{n}\t{t:.0}\t{:.3}\t{max:.1}", t / n as f64);
        }
    }

    /// What the delivery-close head changes on a coord's record (proto-lead N-1479 p109, 1; each way on its line,
    /// p144; paper-research p136). From CLOSE_FROM on (a time, "2026-11-21T16:10:00Z"; else the whole record):
    /// - each post that goes to an agent otherwise by the ledger's close than by the rule before the era ERA_CLOSES
    ///   (`Machine::close_changes`), a line for each way the two closes part;
    /// - the turn ends whose rule Reply now posts where the record has no reply, or names more than its reply did.
    /// Each is read on the record as replayed, just after its post is made or just before its turn ends. The replay is
    /// as recorded (no era line, and a Replied line's names are the record's), so each is what this engine would have
    /// done there live. REPLAY_COORD (a copy), REPLAY_BASE and REPLAY_PROJECT, as `a_coords_record_replays`;
    /// CLOSE_SHOW=N: the first N of each line, by id.
    #[test]
    #[ignore]
    fn what_the_close_changes_on_a_coords_record() {
        let (Ok(coord), Ok(base)) = (std::env::var("REPLAY_COORD"), std::env::var("REPLAY_BASE")) else { return };
        let project = std::env::var("REPLAY_PROJECT").ok();
        let from = std::env::var("CLOSE_FROM").unwrap_or_default();
        let show: usize = std::env::var("CLOSE_SHOW").ok().and_then(|s| s.parse().ok()).unwrap_or(12);
        let (defs, _, _, _) = Tickets::read_defs(&crate::policy::Places { base, project, coord: coord.clone(), ..Default::default() }, 1);
        let text = std::fs::read_to_string(format!("{coord}/tickets/log.jsonl")).unwrap();
        let lines: Vec<Line> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        let mut m = Machine::load(defs);
        let at_of = |l: &Line| serde_json::to_value(l).ok().and_then(|v| v.get("at").and_then(Value::as_str).map(str::to_string)).unwrap_or_default();
        // way -> the posts it was read at: "N-1479.p12 to w2 (reply -> pulled; parts at p11)"
        let mut ways: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let (mut posts, mut ends) = (0usize, 0usize);
        let mut seen: BTreeSet<(u64, u32)> = BTreeSet::new();
        let problems = m.replay_with(&lines, &mut |m, i, here| {
            let at = at_of(&here[0]);
            // the turn end next, read before it is replayed
            let j = i + here.len();
            if let Some(Line::TurnEnded { agent, delivery, text, later, stood, go: None, .. }) = lines.get(j) {
                if at_of(&lines[j]) >= from && text.len > 0 {
                    ends += 1;
                    let rest = lines[j + 1..].iter().take_while(|l| !l.is_input());
                    let replied: Option<Vec<String>> = rest.filter_map(|l| match l { Line::Replied { names, .. } => Some(names.clone()), _ => None }).next();
                    let step = m.turn_answers_step(agent, delivery.as_deref(), later, stood.as_deref());
                    let nothing = m.turn_shows_nothing(agent, later);
                    if let Some((t, names)) = m.reply_names(agent, later).filter(|(_, n)| !n.is_empty() && !step && !nothing) {
                        let id = format!("{} at {}, {agent}'s turn on {}: names {names:?}", m.id(t), at_of(&lines[j]), delivery.as_deref().unwrap_or("-"));
                        match replied {
                            None => ways.entry("turn end: now posts where the record has no reply".into()).or_default().push(id),
                            Some(r) if names.iter().any(|a| !r.contains(a)) => ways.entry("turn end: names more than its reply did".into()).or_default().push(format!("{id}, recorded {r:?}")),
                            Some(_) => {}
                        }
                    }
                }
            }
            if at < from {
                return;
            }
            // the posts made at this input, on the tickets its lines name
            let ts: BTreeSet<u64> = here.iter().filter_map(|l| serde_json::to_value(l).ok()?.get("t")?.as_u64()).collect();
            for t in ts {
                let Some(x) = m.tickets.get(&t) else { continue };
                for p in x.posts.iter().filter(|p| p.at == at && seen.insert((t, p.n))) {
                    posts += 1;
                    for k in m.close_changes(t, p.n) {
                        let word = |w: Option<&str>| w.unwrap_or("pulled").to_string();
                        let way = format!("post: {} -> {}, {}", word(k.before), word(k.by), k.how);
                        ways.entry(way).or_default().push(format!("{}.p{} to {} (parts at p{})", m.id(t), p.n, k.agent, k.at));
                    }
                }
            }
        });
        println!("close on {coord} from {}: {} lines, {posts} posts and {ends} turn ends read, {} problem lines", if from.is_empty() { "its start" } else { &from }, lines.len(), problems.len());
        for p in problems.iter().take(4) {
            println!("  problem: {}", p.chars().take(300).collect::<String>());
        }
        for (way, at) in &ways {
            println!("{}\t{way}", at.len());
            for a in at.iter().take(show) {
                println!("\t  {a}");
            }
        }
        if ways.is_empty() {
            println!("0\tno post and no turn end goes otherwise");
        }
    }

    /// The record as one graph (paper-research N-1446 p1337, points 1 to 3), on a copy: REPLAY_COORD, REPLAY_BASE and
    /// REPLAY_PROJECT as `a_coords_record_replays` reads them. Prints (1) each node kind and edge kind with its count;
    /// (2) GRAPH_FROM (his N-1104.p745) walked GRAPH_HOPS (2) out, edge by edge, and the posts naming GRAPH_FILE
    /// (`paper-research.kind`, which proto-mind edited in flavor, outside coordd: p1339, p1342); (3) GRAPH_GAP (his
    /// N-1104.p678): its own edges, the shortest way from it to a commit and to a build that passes through no agent,
    /// ticket or program, and the edge kinds the graph has none of. GRAPH_OUT=FILE: the whole walk there, a line an edge.
    #[test]
    #[ignore]
    fn a_coords_record_graph() {
        use docket_machine::graph::{self, Edge, EdgeKind, NodeKind, Source};
        use std::collections::VecDeque;
        let (Ok(coord), Ok(base)) = (std::env::var("REPLAY_COORD"), std::env::var("REPLAY_BASE")) else { return };
        let project = std::env::var("REPLAY_PROJECT").ok();
        let var = |k: &str, or: &str| std::env::var(k).unwrap_or_else(|_| or.to_string());
        let (from, file, gap) = (var("GRAPH_FROM", "N-1104.p745"), var("GRAPH_FILE", "paper-research.kind"), var("GRAPH_GAP", "N-1104.p678"));
        let hops: usize = var("GRAPH_HOPS", "2").parse().unwrap_or(2);
        let started = std::time::Instant::now();
        let places = crate::policy::Places { base: base.clone(), project: project.clone(), coord: coord.clone(), ..Default::default() };
        let (defs, mut errors, _, _) = Tickets::read_defs(&places, 1);
        let log = format!("{coord}/tickets/log.jsonl");
        let (text, lines) = Tickets::read_log(&log, &mut errors);
        let mut m = Machine::load(defs);
        let (mut g, problems) = graph::of(&mut m, &lines);
        // `named` is coordd's Mentions index, read on the replayed machine (docket-edge8 p1360); an agent is a graph's agent
        let agents: BTreeSet<String> = g.nodes.iter().filter(|n| n.kind == NodeKind::Agent).filter_map(|n| n.id.strip_prefix("agent:").map(str::to_string)).collect();
        let mut idx = mentions::Mentions::default();
        idx.read_all(&m, &crate::label::read);
        idx.named_into(&m, &|a| agents.contains(a), &mut g);
        // git's commits beside the record (docket-edge5's graph_git), from GRAPH_GIT's `git log --all`, and GRAPH_SIDE's
        // side from the sessions its `given` lines name (p1376 point 4, p1388)
        let (who, repo) = (var("GRAPH_SIDE", "docket-edge5"), var("GRAPH_GIT", "/home/t/Documents/papers/soft-swarm-docket-edge6-graph"));
        let format = format!("--format={}", docket_machine::graph_git::Commit::FORMAT);
        let git_log = std::process::Command::new("git").args(["-C", &repo, "log", "--all", &format]).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        let commits = docket_machine::graph_git::Commit::parse_log(&git_log, &repo);
        docket_machine::graph_git::add(&mut g, &commits, &[]);
        // the side as coordd's start reads it (`follow::KeptSide`, p1458): GRAPH_SIDE's sessions, or with GRAPH_SIDE_ALL
        // every session a `given` names; read off the lock, then put in as `read_sides` puts it, each session under a
        // hold of its own (`SideRound::each`, p1474), timed with the longest hold, and a round after it with nothing
        // new, as each 5 s's
        let all = std::env::var("GRAPH_SIDE_ALL").is_ok();
        let mut kept = crate::follow::KeptSide::default();
        kept.take(&lines, 0, &|t, n| graph::post_id(&m, t, n));
        let picked: BTreeSet<String> = kept.agents().filter(|a| all || *a == who).map(String::from).collect();
        let files: BTreeSet<&str> = lines.iter().filter_map(|l| if let Line::Given { to, place: Some(p), .. } = l { picked.contains(to).then_some(p.file.as_str()) } else { None }).collect();
        let bytes: u64 = files.iter().filter_map(|f| std::fs::metadata(f).ok()).map(|x| x.len()).sum();
        let (before, at) = ((g.nodes.len(), g.edges.len()), std::time::Instant::now());
        let round = kept.round(&picked).read();
        let read = at.elapsed().as_secs_f64();
        let unopened = round.unopened();
        let (at, mut holds, mut longest) = (std::time::Instant::now(), 0, 0.0f64);
        for one in round.each() {
            let held = std::time::Instant::now();
            kept.put(one, &mut g);
            (holds, longest) = (holds + 1, longest.max(held.elapsed().as_secs_f64()));
        }
        let put = at.elapsed().as_secs_f64();
        let at = std::time::Instant::now();
        let round = kept.round(&picked).read();
        let (again, at) = (at.elapsed().as_secs_f64(), std::time::Instant::now());
        for one in round.each() {
            kept.put(one, &mut g);
        }
        let peak = std::fs::read_to_string("/proc/self/status").ok().and_then(|s| s.lines().find(|l| l.starts_with("VmHWM:")).map(|l| l.split_whitespace().skip(1).collect::<Vec<_>>().join(" "))).unwrap_or_default();
        println!("side as coordd's start reads it: {} agents, {} session files, {} MB; read off the lock in {read:.1} s, put in under {holds} holds in {put:.3} s, the longest {longest:.3} s, adding {} nodes and {} edges; a round with nothing new: read in {again:.3} s, put in {:.4} s; the test's peak memory {peak}", picked.len(), files.len(), bytes / 1_000_000, g.nodes.len() - before.0, g.edges.len() - before.1, at.elapsed().as_secs_f64());
        // the file view (p1441, p1530): GRAPH_SIDE's folder as coordd answers an ask that names it, under the lock,
        // timed; its rows, its size as JSON, and each group's head with how many rows it holds, its first rows and
        // theirs; then every agent's folder, timed, the slowest first
        let at = std::time::Instant::now();
        let folder = g.folder(&who);
        let took = at.elapsed().as_secs_f64();
        fn folder_rows(rows: &[docket_machine::graph_slice::Row]) -> usize {
            rows.iter().map(|r| 1 + folder_rows(&r.rows)).sum()
        }
        let json = serde_json::to_string(&folder).map_or(0, |j| j.len());
        println!("{who}'s folder: {} rows in {took:.3} s, {json} bytes as JSON", folder_rows(&folder));
        for r in &folder {
            println!("  {}: {} rows under it", r.text, folder_rows(&r.rows));
            for x in r.rows.iter().take(6) {
                println!("    {} ({} under it)", x.text.chars().take(160).collect::<String>(), folder_rows(&x.rows));
                for y in x.rows.iter().take(4) {
                    println!("      {} ({} under it)", y.text.chars().take(160).collect::<String>(), folder_rows(&y.rows));
                }
            }
        }
        let mut times: Vec<(f64, &str, usize)> = agents
            .iter()
            .map(|a| {
                let at = std::time::Instant::now();
                let f = g.folder(a);
                (at.elapsed().as_secs_f64(), a.as_str(), folder_rows(&f))
            })
            .collect();
        times.sort_by(|x, y| y.0.total_cmp(&x.0));
        let slowest: Vec<String> = times.iter().take(4).map(|(t, a, n)| format!("{a} in {t:.3} s ({n} rows)")).collect();
        println!("every agent's folder: {} agents in {:.3} s; the slowest {}", times.len(), times.iter().map(|t| t.0).sum::<f64>(), slowest.join(", "));
        println!("graph of {coord} with {base}: {} lines, {} problem lines, {} load errors; {} nodes, {} edges; in {} s", lines.len(), problems.len(), errors.len(), g.nodes.len(), g.edges.len(), started.elapsed().as_secs());
        for p in problems.iter().take(4) {
            println!("  problem: {}", p.chars().take(300).collect::<String>());
        }
        let word = |v: Result<Value, serde_json::Error>| v.ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| "?".into());
        let kind = |k: EdgeKind| word(serde_json::to_value(k));
        // an id on one line, cut at 60 characters (a handler's id is its program's whole text)
        let short = |id: &str| {
            let one = id.split_whitespace().collect::<Vec<_>>().join(" ");
            if one.chars().count() > 60 { format!("{}…", one.chars().take(60).collect::<String>()) } else { one }
        };
        let edge = |e: &Edge| format!("{} {} -> {} (line {})", kind(e.kind), short(&e.from), short(&e.to), e.line().unwrap_or(0));
        let (nodes, edges) = g.counts();
        let node_kinds = [NodeKind::Post, NodeKind::Ticket, NodeKind::Agent, NodeKind::Turn, NodeKind::Step, NodeKind::Call, NodeKind::CallFile, NodeKind::Template, NodeKind::Program, NodeKind::Handler, NodeKind::File, NodeKind::Commit, NodeKind::Session, NodeKind::Start, NodeKind::Text, NodeKind::Request, NodeKind::Tools, NodeKind::Job, NodeKind::Branch, NodeKind::Build, NodeKind::Account, NodeKind::Slice, NodeKind::Worktree];
        let edge_kinds = [EdgeKind::Wrote, EdgeKind::PostedIn, EdgeKind::Answered, EdgeKind::AnsweredUpto, EdgeKind::Named, EdgeKind::Handed, EdgeKind::Woke, EdgeKind::TurnOf, EdgeKind::Prompted, EdgeKind::StartOf, EdgeKind::Ran, EdgeKind::Ended, EdgeKind::Started, EdgeKind::Closed, EdgeKind::StepOf, EdgeKind::Called, EdgeKind::CameFrom, EdgeKind::Edited, EdgeKind::Read, EdgeKind::MergedInto, EdgeKind::BuiltInto, EdgeKind::On, EdgeKind::Installed, EdgeKind::SessionOf, EdgeKind::Loaded, EdgeKind::Briefed, EdgeKind::VersionOf, EdgeKind::Sent, EdgeKind::Carried, EdgeKind::For, EdgeKind::Took, EdgeKind::Informed, EdgeKind::WrittenIn, EdgeKind::SentIn, EdgeKind::RanWith, EdgeKind::Became, EdgeKind::In, EdgeKind::HandledBy, EdgeKind::SignedOff, EdgeKind::MadeIn, EdgeKind::StartedWith];
        println!("1. nodes by kind");
        for k in node_kinds {
            println!("  {}\t{}", word(serde_json::to_value(k)), nodes.get(&k).copied().unwrap_or(0));
        }
        println!("1. edges by kind");
        for k in edge_kinds {
            println!("  {}\t{}", kind(k), edges.get(&k).copied().unwrap_or(0));
        }
        // point 3 (p1458): each agent's post in the turn it was written in
        let written: BTreeSet<&str> = g.edges.iter().filter(|e| e.kind == EdgeKind::WrittenIn).map(|e| e.from.as_str()).collect();
        let theirs: Vec<&str> = g.edges.iter().filter(|e| e.kind == EdgeKind::Wrote && e.from.starts_with("agent:") && e.from != graph::agent_id("owner") && g.get(&e.to).is_some_and(|n| n.kind == NodeKind::Post)).map(|e| e.to.as_str()).collect();
        println!("1. written in: {} of the agents' {} posts are in the turn they were written in; {} have no turn end of their writer's after them", theirs.iter().filter(|p| written.contains(**p)).count(), theirs.len(), theirs.iter().filter(|p| !written.contains(**p)).count());
        // 2: hop 1 whole; from hop 2 on, by the node each edge leaves from and its kind, whole up to 6 a group
        let walk = g.walk(&from, hops);
        println!("2. {from} walked {hops} hops out: {} edges", walk.len());
        let mut dist: BTreeMap<&str, usize> = BTreeMap::from([(from.as_str(), 0)]);
        let mut groups: BTreeMap<(usize, String, EdgeKind), Vec<&Edge>> = BTreeMap::new();
        for (h, e) in &walk {
            let near = if dist.get(e.from.as_str()).is_some_and(|d| d < h) { e.from.as_str() } else { e.to.as_str() };
            let far = if near == e.from { e.to.as_str() } else { e.from.as_str() };
            dist.entry(far).or_insert(*h);
            groups.entry((*h, near.to_string(), e.kind)).or_default().push(*e);
        }
        for ((h, near, k), es) in &groups {
            if *h == 1 {
                es.iter().for_each(|e| println!("  hop 1: {}", edge(*e)));
                continue;
            }
            println!("  hop {h}, from {}, {}: {} edges, lines {}..{}", short(near), kind(*k), es.len(), es.first().and_then(|e| e.line()).unwrap_or(0), es.last().and_then(|e| e.line()).unwrap_or(0));
            let shown: Vec<&&Edge> = if es.len() <= 6 { es.iter().collect() } else { es.iter().take(3).chain(es.last()).collect() };
            shown.iter().for_each(|e| println!("    {}", edge(**e)));
        }
        if let Ok(out) = std::env::var("GRAPH_OUT") {
            let text: String = walk.iter().map(|(h, e)| format!("{h}\t{}\n", edge(*e))).collect();
            std::fs::write(&out, text).expect("GRAPH_OUT written");
            println!("  the whole walk is in {out}");
        }
        // each edge's line is log.jsonl's own (paper-research p1347): the file's lines against those replayed, and the
        // first edge of the walk with the file's line at its number
        let record: Vec<&str> = text.lines().collect();
        let unread = record.len() - lines.len();
        println!("2. lines: {log} has {} lines, {} of them replayed, {unread} did not read: {}", record.len(), lines.len(), if unread == 0 { "an edge's line is the file's line" } else { "an edge's line is NOT the file's line after the first that did not read" });
        if let Some((_, e)) = walk.first() {
            let n = e.line().unwrap_or(0);
            println!("  {}: the file's line {n} is {}", edge(*e), record.get(n.wrapping_sub(1)).map(|l| l.chars().take(240).collect::<String>()).unwrap_or_default());
        }
        let reached: BTreeSet<&str> = walk.iter().flat_map(|(_, e)| [e.from.as_str(), e.to.as_str()]).collect();
        let files: Vec<&str> = g.nodes.iter().filter(|n| n.kind == NodeKind::File && n.id.contains(file.as_str())).map(|n| n.id.as_str()).collect();
        println!("2. {file}: {} file nodes {files:?}, {} of them in the walk; {} edited edges in the graph", files.len(), files.iter().filter(|f| reached.contains(*f)).count(), edges.get(&EdgeKind::Edited).copied().unwrap_or(0));
        for (t, x) in m.tickets.iter() {
            for p in x.posts.iter().filter(|p| p.text.value().to_string().contains(file.as_str())) {
                let id = graph::post_id(&m, *t, p.n);
                println!("  named in {id} by {}, line {:?}, {}", p.by, g.get(&id).and_then(|n| n.line()), if reached.contains(id.as_str()) { "in the walk" } else { "not in the walk" });
            }
        }
        if !g.edges.iter().any(|e| e.kind == EdgeKind::Edited && files.contains(&e.to.as_str())) {
            println!("  edited: missing. proto-mind's write of {file} was in flavor, outside coordd, so no line of the record says it; the record reaches the file only through the posts above that name it");
        }
        // 3: the shortest way from his post to a commit and to a build, through no agent, ticket or program
        let mut by: BTreeMap<&str, Vec<&Edge>> = BTreeMap::new();
        for e in &g.edges {
            by.entry(e.from.as_str()).or_default().push(e);
            by.entry(e.to.as_str()).or_default().push(e);
        }
        println!("3. {gap}: {} edges of its own", by.get(gap.as_str()).map_or(0, Vec::len));
        for e in by.get(gap.as_str()).into_iter().flatten() {
            println!("  {}", edge(*e));
        }
        let hub = |id: &str| g.get(id).is_some_and(|n| matches!(n.kind, NodeKind::Agent | NodeKind::Ticket | NodeKind::Program));
        for target in [NodeKind::Commit, NodeKind::Build] {
            let mut back: BTreeMap<&str, &Edge> = BTreeMap::new();
            let mut seen = BTreeSet::from([gap.as_str()]);
            let mut front = VecDeque::from([gap.as_str()]);
            let mut found = None;
            while let Some(at) = front.pop_front() {
                if g.get(at).is_some_and(|n| n.kind == target) {
                    found = Some(at);
                    break;
                }
                if at != gap && hub(at) {
                    continue;
                }
                for e in by.get(at).into_iter().flatten() {
                    let other = if e.from == at { e.to.as_str() } else { e.from.as_str() };
                    if seen.insert(other) {
                        back.insert(other, *e);
                        front.push_back(other);
                    }
                }
            }
            let name = word(serde_json::to_value(target));
            match found {
                Some(mut at) => {
                    let mut way = Vec::new();
                    while let Some(e) = back.get(at) {
                        way.push(edge(*e));
                        at = if e.from == at { e.to.as_str() } else { e.from.as_str() };
                    }
                    println!("  to a {name}: {} edges", way.len());
                    way.iter().rev().for_each(|w| println!("    {w}"));
                }
                None => println!("  to a {name}: none ({} {name} nodes in the graph; {} nodes reached through no agent, ticket or program)", nodes.get(&target).copied().unwrap_or(0), seen.len()),
            }
        }
        let none: Vec<String> = edge_kinds.iter().filter(|k| !edges.contains_key(*k)).map(|k| kind(*k)).collect();
        println!("3. edge kinds the graph has none of: {}", if none.is_empty() { "none".to_string() } else { none.join(", ") });
        // 4: GRAPH_SIDE's side, each of its sessions as the file view shows it (p1376 point 4)
        let me = graph::agent_id(&who);
        let sline = |e: &Edge| if let Source::Session(_, n) = &e.source { *n } else { 0 };
        println!("4. {who}'s side: {} commits read from {repo}'s git; sessions that did not open: {unopened:?}", commits.len());
        println!("  kind: missing. No line of the record says an agent's kind (p1388: roster's start line is to say it)");
        for se in g.edges.iter().filter(|e| e.kind == EdgeKind::SessionOf && e.to == me) {
            let (s, n) = (se.from.as_str(), se.line().unwrap_or(0));
            let (file, delivery, at) = match lines.get(n.wrapping_sub(1)) {
                Some(Line::Given { place: Some(p), delivery, at, .. }) => (p.file.clone(), delivery.clone(), at.clone()),
                _ => Default::default(),
            };
            println!("  {s}: the record's line {n} first names it, {delivery} given at {at}");
            let from = |k: EdgeKind| -> Vec<&Edge> { g.edges.iter().filter(|e| e.from == s && e.kind == k).collect() };
            // the file read again, with its givens' places, for what the graph does not keep: the brief's words, how
            // each step was joined
            let offsets: Vec<u64> = lines.iter().filter_map(|l| match l {
                Line::Given { to, place: Some(p), .. } if *to == who && p.file == file => Some(p.offset),
                _ => None,
            }).collect();
            let side = crate::follow::side(&file, &offsets).unwrap_or_default();
            let words = side.brief.as_ref().map(|b| b.text.split_whitespace().take(30).collect::<Vec<_>>().join(" ")).unwrap_or_default();
            for e in from(EdgeKind::Briefed) {
                println!("    briefed: {} at session line {}, {}, {}; its first words: {words}", e.to, sline(e), e.when.as_deref().unwrap_or("?"), g.get(&e.to).map_or("", |x| x.what.as_str()));
            }
            let loads = from(EdgeKind::Loaded);
            let mut texts: Vec<(&str, usize, usize, Option<String>)> = Vec::new();
            for e in &loads {
                match texts.iter_mut().find(|t| t.0 == e.to) {
                    Some(t) => t.2 += 1,
                    None => texts.push((e.to.as_str(), sline(e), 1, e.when.clone())),
                }
            }
            println!("    loaded: {} loads of {} texts as they stood (Claude Code's instructions: CLAUDE.md, AGENTS.md, memory)", loads.len(), texts.len());
            for (t, first, count, when) in &texts {
                println!("      {t} {}: first at session line {first}, {}, loaded {count} times", g.get(t).map_or("", |x| x.what.as_str()), when.as_deref().unwrap_or("?"));
            }
            let reads = from(EdgeKind::Read);
            let mut files: BTreeMap<&str, usize> = BTreeMap::new();
            reads.iter().for_each(|e| *files.entry(e.to.as_str()).or_default() += 1);
            let mut most: Vec<(&str, usize)> = files.into_iter().collect();
            most.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            println!("    read: {} Read calls of {} files; the most read:", reads.len(), most.len());
            most.iter().take(8).for_each(|(f, c)| println!("      {c}\t{f}"));
            // its requests (p1398 5): each step by its first request, and each request in the last step begun by it
            let sent = from(EdgeKind::Sent);
            let mut whats: BTreeMap<&str, usize> = BTreeMap::new();
            sent.iter().for_each(|e| *whats.entry(g.get(&e.to).map_or("?", |x| x.what.as_str())).or_default() += 1);
            println!("    sent: {} requests ({} in the file); by model and effort: {whats:?}; {} shapes", sent.len(), side.requests.len(), side.shapes.len());
            // their tokens (p1429 2), from the request nodes: each from its response, where the session holds one
            let mut cost: BTreeMap<&str, (usize, docket_machine::graph::Tokens)> = BTreeMap::new();
            for x in sent.iter().filter_map(|e| g.get(&e.to)) {
                if let Some(t) = x.tokens {
                    let c = cost.entry(x.what.as_str()).or_default();
                    c.0 += 1;
                    c.1 += t;
                }
            }
            let carried: usize = cost.values().map(|c| c.0).sum();
            println!("    tokens: {carried} of {} requests have a response that carries them (`message.usage`, by `requestRef`); by model and effort, (requests, input, cache write, cache read, output, of it thinking):", sent.len());
            for (w, (n, t)) in &cost {
                println!("      {w}: ({n}, {}, {}, {}, {}, {})", t.input, t.cache_write, t.cache_read, t.output, t.reasoning);
            }
            let unanswered: Vec<usize> = side.requests.iter().filter(|q| q.tokens.is_none()).map(|q| q.line).collect();
            let first_request = side.requests.first().map(|q| q.line);
            println!("    not carried: {} requests with no response (session lines {:?}); {} responses name no request line before them (session lines {:?} to {:?}; the first request line is {first_request:?})", unanswered.len(), unanswered.iter().take(10).collect::<Vec<_>>(), side.unrequested.len(), side.unrequested.first(), side.unrequested.last());
            let starts: BTreeSet<usize> = side.began.iter().flatten().map(|b| b.request).collect();
            let by_hash = side.began.iter().flatten().filter(|b| b.by_hash).count();
            let joined = side.began.iter().flatten().count();
            println!("    steps: {} givens name this file; {by_hash} began by hash (a blob the request's tail names holds the given's text), {} by order (the first request after its text went in), {} with no request after; {} steps (givens sharing a first request share its step); {} requests before the first", offsets.len(), joined - by_hash, offsets.len() - joined, starts.len(), starts.first().copied().unwrap_or(side.requests.len()));
            let standing = project.as_ref().and_then(|p| std::fs::read_to_string(format!("{p}/standing.md")).ok()).map(|t| t.trim().to_string());
            let mut blocks: BTreeMap<String, (usize, BTreeSet<usize>, usize, bool)> = BTreeMap::new();
            let mut tools: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
            for (k, q) in side.requests.iter().enumerate() {
                let step = starts.range(..=k).next_back().copied();
                let Some(shape) = side.shapes.get(&q.shape) else { continue };
                for b in &shape.system {
                    let x = blocks.entry(crate::follow::text_id(b)).or_insert((0, BTreeSet::new(), b.len(), standing.as_ref().is_some_and(|t| b.contains(t.as_str()))));
                    x.0 += 1;
                    x.1.extend(step);
                }
                tools.entry(shape.tools.as_str()).or_insert((0, shape.tool_count)).0 += 1;
            }
            println!("    system prompt blocks it carried (each kept once by its hash; `holds standing.md`: holds {}/standing.md whole, as the copy has it):", project.as_deref().unwrap_or("?"));
            for (t, (n, st, bytes, holds)) in &blocks {
                println!("      {t} ({bytes} bytes): {n} requests in {} steps{}", st.len(), if *holds { "; holds standing.md" } else { "" });
            }
            for (t, (n, count)) in &tools {
                println!("      {t} ({count} tools): {n} requests");
            }
            let mine: BTreeSet<&str> = sent.iter().map(|e| e.to.as_str()).collect();
            let fors: Vec<&Edge> = g.edges.iter().filter(|e| e.kind == EdgeKind::For && mine.contains(e.from.as_str())).collect();
            let mut per: BTreeMap<&str, usize> = BTreeMap::new();
            fors.iter().for_each(|e| *per.entry(e.to.as_str()).or_default() += 1);
            let mut most: Vec<(&str, usize)> = per.into_iter().collect();
            most.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            println!("    for: {} request → post edges; the posts with the most requests: {:?}", fors.len(), most.iter().take(6).collect::<Vec<_>>());
            // point 3 (p1458): the turn each request was sent in; the texts its loaded files stood as at it, each with
            // the steps that ran on it; each text's next version in the session, by its words' counts (the words are
            // the edge's)
            let into: BTreeMap<&str, &str> = g.edges.iter().filter(|e| e.kind == EdgeKind::SentIn && mine.contains(e.from.as_str())).map(|e| (e.from.as_str(), e.to.as_str())).collect();
            let second = |id: &str| g.get(id).and_then(|x| x.when.as_deref()).and_then(|t| t.get(..19));
            let same_second = into.iter().filter(|&(&q, &t)| second(q).is_some() && second(q) == second(t)).count();
            let none: Vec<usize> = side.requests.iter().filter(|q| !into.contains_key(format!("request:{}", q.id).as_str())).map(|q| q.line).collect();
            let turns: BTreeSet<&str> = into.values().copied().collect();
            let ended = g.edges.iter().filter(|e| e.kind == EdgeKind::TurnOf && e.to == me).count();
            println!("    sent in: {} of {} requests are in one of {} turns (of the {ended} turns the agent ended or had cut); {same_second} in the same whole second as their turn's end, counted in it; {} in none, after the last turn's end (session lines {:?} to {:?})", into.len(), mine.len(), turns.len(), none.len(), none.first(), none.last());
            let index: BTreeMap<String, usize> = side.requests.iter().enumerate().map(|(k, q)| (format!("request:{}", q.id), k)).collect();
            let mut ran: BTreeMap<&str, (usize, BTreeSet<usize>)> = BTreeMap::new();
            for e in g.edges.iter().filter(|e| e.kind == EdgeKind::RanWith && mine.contains(e.from.as_str())) {
                let x = ran.entry(e.to.as_str()).or_default();
                x.0 += 1;
                x.1.extend(index.get(&e.from).and_then(|k| starts.range(..=*k).next_back().copied()));
            }
            println!("    ran with: {} request → loaded text edges, to {} texts; each, with its requests and the steps they were in:", ran.values().map(|x| x.0).sum::<usize>(), ran.len());
            for (t, (n, st)) in &ran {
                println!("      {t} {}: {n} requests in {} steps", g.get(t).map_or("", |x| x.what.as_str()), st.len());
            }
            let became: Vec<&Edge> = g.edges.iter().filter(|e| e.kind == EdgeKind::Became && matches!(&e.source, Source::Session(f, _) if s.strip_prefix("session:") == Some(f.as_str()))).collect();
            println!("    became: {} next versions in this session; each with its words' counts (the words are on the edge, for the window):", became.len());
            for e in became.iter().take(12).chain(became.last().filter(|_| became.len() > 12)) {
                println!("      {} -> {} at session line {}, {}: {} ({} bytes of words)", e.from, e.to, sline(e), e.when.as_deref().unwrap_or("?"), e.what.split(':').next().unwrap_or(""), e.what.len());
            }
            let made: Vec<&Edge> = g.edges.iter().filter(|e| e.kind == EdgeKind::CameFrom && e.to == s && e.from.starts_with("commit:")).collect();
            println!("    commits from it (their `Session:`): {}", made.len());
            for e in made.iter().take(5).chain(made.last().filter(|_| made.len() > 5)) {
                println!("      {} {}", e.from, g.get(&e.from).map_or("", |x| x.what.as_str()));
            }
        }
        let to_me = |k: EdgeKind| g.edges.iter().filter(|e| e.kind == k && e.to == me).count();
        println!("  turns: {}; handed: {}; woke: {}; commits it wrote (`Agent:`): {}", to_me(EdgeKind::TurnOf), to_me(EdgeKind::Handed), to_me(EdgeKind::Woke), g.edges.iter().filter(|e| e.kind == EdgeKind::Wrote && e.from == me && e.to.starts_with("commit:")).count());
        println!("  worktree: not read yet (next: git's worktrees against the branches its commits are on)");
    }

    /// `Graph::now` on a copy of a coord's record (docket-edge5, paper-research N-1446 p1399; ignored, as
    /// `a_coords_record_graph`): REPLAY_COORD, REPLAY_BASE and REPLAY_PROJECT as there; beside the record, the commits
    /// and branch moves of each repository in GRAPH_REPOS (`NAME=DIR,…`, git read only), where each commit was made (its
    /// worktrees' HEAD reflogs, `work_repos::head_reflogs`), and the job files in GRAPH_JOBS
    /// sent from a coord of the copy's name. Printed at NOW_AT (else the latest time of the record's edges): the agents in
    /// NOW_AGENTS (`a,b,…`), each ask NOW_ASKS names (`N-1104.p678,…`) with its whole course, his last ten asks with what
    /// each has reached, and every node and edge kind with its count. Git's commits join what they name through
    /// docket-edge8's `commits_into`, as in the kept graph (docket-core p1426). With COST_SIDES (`a,b,…`), those agents'
    /// sessions as the record's `given` lines name them (`follow::side_into`, docket-edge6's reader), and what each named
    /// ask cost, with the costliest ten by then (paper-research p1429 2; its `handed` alone, named by beside it, p1535);
    /// a session under the accounts root is never
    /// opened: its givens go to the reader with no place. COST_SIDES `*` is every agent of the graph; another project's
    /// (PROJECT/NAME) is a name with nothing read behind it, its sessions never opened (`KeptSide::take`, paper-research
    /// N-1446 p1828). Then Open → done's
    /// asks as coordd gives them under its lock (`Core::asks_rows`), each part timed, the whole and its JSON, and the
    /// row's numbers (paper-research N-1446 p1786, 2).
    #[test]
    #[ignore]
    fn a_coords_record_now() {
        use docket_machine::graph::{self, Now, Point};
        use docket_machine::graph_git::{self, Commit, Job, Moved};
        let (Ok(coord), Ok(base)) = (std::env::var("REPLAY_COORD"), std::env::var("REPLAY_BASE")) else { return };
        let project = std::env::var("REPLAY_PROJECT").ok();
        let var = |k: &str| std::env::var(k).unwrap_or_default();
        let started = std::time::Instant::now();
        let places = crate::policy::Places { base: base.clone(), project: project.clone(), coord: coord.clone(), ..Default::default() };
        let (defs, mut errors, _, _) = Tickets::read_defs(&places, 1);
        let (_, lines) = Tickets::read_log(&format!("{coord}/tickets/log.jsonl"), &mut errors);
        let mut m = Machine::load(defs);
        let (mut g, problems) = graph::of(&mut m, &lines);
        let agents: BTreeSet<String> = g.nodes.iter().filter_map(|n| n.id.strip_prefix("agent:").map(str::to_string)).collect();
        let mut idx = mentions::Mentions::default();
        idx.read_all(&m, &crate::label::read);
        idx.named_into(&m, &|a| agents.contains(a), &mut g);
        let (mut commits, mut moves, mut made) = (Vec::new(), Vec::new(), Vec::new());
        for (name, dir) in var("GRAPH_REPOS").split(',').filter_map(|r| r.split_once('=')) {
            let git = |args: &[&str]| std::process::Command::new("git").arg("-C").arg(dir).args(args).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
            commits.extend(Commit::parse_log(&git(&["log", "--all", &format!("--format={}", Commit::FORMAT)]), name));
            moves.extend(Moved::parse_log(&git(&["log", "-g", "--all", "--date=unix", &format!("--format={}", Moved::FORMAT)]), name));
            let common = git(&["rev-parse", "--path-format=absolute", "--git-common-dir"]);
            made.extend(Some(common.trim()).filter(|c| !c.is_empty()).map(|c| work_repos::head_reflogs(std::path::Path::new(c), name)).unwrap_or_default());
        }
        let ours = std::path::Path::new(&coord).file_name().map(|f| f.to_os_string());
        let mut jobs: Vec<Job> = std::fs::read_dir(var("GRAPH_JOBS")).into_iter().flatten().flatten().filter_map(|e| {
            let p = e.path();
            let id = p.file_stem()?.to_str()?.to_string();
            Job::parse(&id, &std::fs::read_to_string(&p).ok()?)
        }).filter(|j| std::path::Path::new(&j.coord).file_name().map(|f| f.to_os_string()) == ours).collect();
        jobs.sort_by(|a, b| a.id.cmp(&b.id));
        graph_git::add(&mut g, &commits, &jobs);
        mentions::commits_into(&m, &|a| agents.contains(a), &commits, &mut g);
        graph_git::add_moves(&mut g, &moves);
        graph_git::add_made_in(&mut g, &made);
        // the kept index (paper-research N-1446 p1828, (b)), made as coordd's start makes it, before the sessions' side;
        // with the graph's heap and the index's beside it, and the process's resident and peak memory (p1865)
        let vm = |k: &str| std::fs::read_to_string("/proc/self/status").unwrap_or_default().lines().find(|l| l.starts_with(k)).and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok()).map_or(0, |kb| kb / 1024);
        let mem = |when: &str, g: &graph::Graph| {
            let (graph, parts) = g.heap_bytes();
            let kept: usize = parts.iter().map(|(_, b)| b).sum();
            println!("memory {when}: the graph ~{} MB, its kept index ~{} MB ({:.1}% of the graph); resident {} MB, peak {} MB", graph >> 20, kept >> 20, 100.0 * kept as f64 / graph.max(1) as f64, vm("VmRSS:"), vm("VmHWM:"));
            println!("memory {when}, the index by part (MB): {}", parts.iter().map(|(n, b)| format!("{n} {:.1}", *b as f64 / 1048576.0)).collect::<Vec<_>>().join(", "));
        };
        let resident = vm("VmRSS:");
        let t = std::time::Instant::now();
        g.keep();
        println!("kept index at the start: {} nodes, {} edges, made from nothing in {:.1} ms; resident memory grew {} MB as it was made", g.nodes.len(), g.edges.len(), t.elapsed().as_secs_f64() * 1000.0, vm("VmRSS:").saturating_sub(resident));
        mem("at the start, the index made", &g);
        let side_edges = g.edges.len();
        let mut sides: BTreeSet<String> = var("COST_SIDES").split(',').map(str::trim).filter(|a| !a.is_empty()).map(str::to_string).collect();
        if sides.remove("*") {
            sides.extend(agents.iter().cloned());
        }
        let accounts = format!("{}/agent-coord/accounts/", var("HOME"));
        let mut kept_out = 0;
        // the graph's length (nodes, edges) before the sessions' side and after each put (a part of a session's),
        // saved with SAVE_GRAPH for a test to grow it again as it grew
        let mut grew: Vec<(usize, usize)> = vec![(g.nodes.len(), g.edges.len())];
        let unopened = if sides.is_empty() {
            Vec::new()
        } else {
            let placed: Vec<docket_machine::Line> = lines
                .iter()
                .cloned()
                .map(|mut l| {
                    if let docket_machine::Line::Given { place, .. } = &mut l {
                        if place.as_ref().is_some_and(|p| p.file.starts_with(&accounts)) {
                            *place = None;
                            kept_out += 1;
                        }
                    }
                    l
                })
                .collect();
            // the side as `follow::side_into` makes it, one round of every session read whole; put as coordd puts a
            // round (main.rs `put_side`): each session in parts of about `follow::SIDE_PART` edges, each part under a
            // hold of its own, the index caught up after each (`Core::side_put_part`), so each hold is timed as
            // coordd's lock would be held
            let ms = |t: std::time::Instant| t.elapsed().as_secs_f64() * 1000.0;
            let t = std::time::Instant::now();
            let mut side = crate::follow::KeptSide::default();
            side.take(&placed, 0, &|t, n| graph::post_id(&m, t, n));
            let picked: BTreeSet<String> = side.agents().filter(|a| sides.contains(*a)).map(String::from).collect();
            let round = side.round(&picked).read();
            let read_ms = ms(t);
            let unopened = round.unopened();
            let mut holds: Vec<(f64, f64, f64, usize)> = Vec::new();
            let mut sessions = 0;
            for mut one in round.each() {
                sessions += 1;
                loop {
                    let before = g.edges.len();
                    let t = std::time::Instant::now();
                    let done = side.put_part(&mut one, &mut g, crate::follow::SIDE_PART);
                    let put = ms(t);
                    grew.push((g.nodes.len(), g.edges.len()));
                    let t = std::time::Instant::now();
                    g.keep();
                    let keep = ms(t);
                    holds.push((put + keep, put, keep, g.edges.len().saturating_sub(before)));
                    if done {
                        break;
                    }
                }
            }
            let (put_ms, keep_ms): (f64, f64) = (holds.iter().map(|h| h.1).sum(), holds.iter().map(|h| h.2).sum());
            println!("kept index: the sessions' side read {sessions} sessions off the lock in {read_ms:.1} ms, and put {} edges in {} parts of about {} edges, in {put_ms:.1} ms; caught up with them in {keep_ms:.1} ms, in all", g.edges.len() - side_edges, holds.len(), crate::follow::SIDE_PART);
            holds.sort_by(|a, b| b.0.total_cmp(&a.0));
            println!(
                "kept index: the side's holds of the lock, one a part of a session (its put, then the index caught up): {} over 16 ms, {} over 100 ms; the longest ten (hold = put + catch-up, edges): {}",
                holds.iter().filter(|h| h.0 > 16.0).count(),
                holds.iter().filter(|h| h.0 > 100.0).count(),
                holds.iter().take(10).map(|h| format!("{:.1} = {:.1} + {:.1} ms, {} edges", h.0, h.1, h.2, h.3)).collect::<Vec<_>>().join("; ")
            );
            let mut keeps: Vec<f64> = holds.iter().map(|h| h.2).collect();
            keeps.sort_by(|a, b| b.total_cmp(a));
            println!("kept index: the catch-ups alone, the longest five: {}", keeps.iter().take(5).map(|k| format!("{k:.1} ms")).collect::<Vec<_>>().join(", "));
            mem("after the sessions' side, the index caught up", &g);
            unopened
        };
        let at = Some(var("NOW_AT")).filter(|a| !a.is_empty()).or_else(|| g.edges.iter().filter(|e| e.line().is_some()).filter_map(|e| e.when.clone()).max()).unwrap_or_default();
        let now: Now = g.now(&at);
        println!("now of {coord} at {at}: {} lines, {} problem lines, {} load errors; {} commits, {} branch moves, {} made in a worktree, {} jobs of {:?}; {} nodes, {} edges; in {} s", lines.len(), problems.len(), errors.len(), commits.len(), moves.len(), made.len(), jobs.len(), ours, g.nodes.len(), g.edges.len(), started.elapsed().as_secs());
        let short = |s: &str| if s.chars().count() > 70 { format!("{}…", s.chars().take(70).collect::<String>()) } else { s.to_string() };
        // a time on the day of AT by its clock alone, else with its month and day (`11-21 10:49:23`)
        let hm = |t: &str| match (t.get(5..10), t.get(11..19)) {
            (Some(d), Some(c)) if t.get(..10) != at.get(..10) => format!("{d} {c}"),
            (_, Some(c)) => c.to_string(),
            _ => t.to_string(),
        };
        for name in var("NOW_AGENTS").split(',').filter(|a| !a.is_empty()) {
            let Some(a) = now.agents.iter().find(|a| a.agent == name) else {
                println!("{name}: no row: no edge by then, or no turn or start of its own in the graph");
                continue;
            };
            let turn = match (&a.turn, &a.idle_since) {
                (Some(t), _) => format!("in a turn since {} ({} {})", hm(&t.since), serde_json::to_value(t.kind).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default(), t.by),
                (None, Some(i)) => format!("not in a turn, since {}", hm(i)),
                (None, None) => "not in a turn".into(),
            };
            println!("{name}: {turn}");
            println!("  last post: {}", a.last_post.as_ref().map_or("none".into(), |p| format!("{} {}", hm(&p.at), p.id)));
            println!("  items started, not closed: {}", if a.items.is_empty() { "none".into() } else { a.items.iter().map(|i| format!("{} (since {})", i.id, hm(&i.at))).collect::<Vec<_>>().join(", ") });
            println!("  last commit: {}", a.last_commit.as_ref().map_or("none".into(), |c| format!("{} {} on {}: {}", hm(&c.at), c.id, c.on.as_deref().unwrap_or("no branch"), short(&c.what))));
            println!("  last job: {}", a.last_job.as_ref().map_or("none".into(), |j| format!("{} {} {} on {}: {}", hm(&j.at), j.id, short(&j.what), j.commit.as_deref().unwrap_or("?"), match (&j.ended, j.ok) { (None, _) => "running".to_string(), (Some(e), Some(true)) => format!("green at {}", hm(e)), (Some(e), Some(false)) => format!("red at {}", hm(e)), (Some(e), None) => format!("ended at {} (no rc)", hm(e)) })));
            println!("  last start: {}", a.began.as_ref().map_or("none in the record".into(), |b| format!("{} {} ({})", hm(&b.at), b.what, b.on.as_deref().unwrap_or("no session"))));
            println!("  its session's last line: {}", a.session.as_ref().map_or("none read".into(), |x| format!("{} line {} of {}", hm(&x.at), x.line, x.session)));
        }
        // every agent in a turn by then, the one in it longest first: work alone opens none of an agent the graph shows
        // no turn of (proto-window N-1446 p1481)
        let mut turns: Vec<(&str, &str)> = now.agents.iter().filter_map(|a| Some((a.turn.as_ref()?.since.as_str(), a.agent.as_str()))).collect();
        turns.sort();
        println!("in a turn by then: {} of {} agents, the one in it longest first: {}", turns.len(), now.agents.len(), turns.iter().map(|(s, a)| format!("{a} (since {})", hm(s))).collect::<Vec<_>>().join(", "));
        // stuck work (p1429 4): in a turn, its session quiet longest first; each branch red with no green since
        let session_at = |name: &str| now.agents.iter().find(|a| a.agent == name).and_then(|a| a.session.as_ref()).map_or(String::new(), |x| hm(&x.at));
        println!("in a turn, the session written nothing in for longest first: {}", now.quiet.iter().map(|a| format!("{a} (last line {})", session_at(a))).collect::<Vec<_>>().join(", "));
        println!(
            "branches an agent works on whose last job ended red, no green since: {} (and {} more no agent works on by then); the ten red longest: {}",
            now.reds.len(),
            now.old_reds,
            now.reds.iter().take(10).map(|r| format!("{} since {} ({} {} at {})", r.branch, hm(&r.since), r.job.id, r.job.commit.as_deref().unwrap_or("?"), r.job.ended.as_deref().map(hm).unwrap_or_default())).collect::<Vec<_>>().join("; ")
        );
        // his last ten asks by then, each in his window or at its last point; an ask is a post of his the ASKED ledger
        // says asks an agent (p1561): of his posts by then, how many; and of his answers to a step (a question's, a
        // request's) at the record's end, those the ledger says ask an agent, which `his_ask` counts by its edge
        let all = g.ask_times_at(Some(&at));
        let open: BTreeMap<&str, &graph::AskNow> = now.asks.iter().map(|a| (a.times.post.as_str(), a)).collect();
        let his_posts = g.edges.iter().filter(|e| e.kind == graph::EdgeKind::Wrote && e.from == "agent:owner" && e.when.as_deref().is_some_and(|w| w <= at.as_str()) && g.get(&e.to).is_some_and(|n| n.kind == graph::NodeKind::Post) && docket_machine::post_or_ticket(&e.to).and_then(|(t, _)| docket_machine::parse_ticket(t)).is_some_and(|t| t < docket_machine::HIDDEN)).count();
        let answers: Vec<(u64, u32)> = m.tickets.range(..docket_machine::HIDDEN).flat_map(|(t, x)| x.posts.iter().filter(|p| p.by == "owner" && p.step.is_some() && p.visible()).map(move |p| (*t, p.n))).collect();
        let answers_asking: Vec<String> = answers.iter().filter(|(t, n)| !m.asked(*t, *n).is_empty()).map(|(t, n)| format!("{}.p{n}", m.id(*t))).collect();
        println!(
            "his posts by then: {his_posts}, of them asks (the ASKED ledger says each asks an agent): {}; his answers to a step at the record's end: {}, of them asking an agent: {} {answers_asking:?}",
            all.len(),
            answers.len(),
            answers_asking.len()
        );
        println!(
            "his asks not yet in his window: {}; reached and never taken: {}; taken with no commit: {} (open in the ASKED ledger {}); closed in words with no commit: {}",
            now.asks.len(),
            now.not_taken.len(),
            now.no_commit.len(),
            now.open.len(),
            now.answered.len()
        );
        for t in all.iter().rev().take(10).rev() {
            let state = match (&t.in_window, open.get(t.post.as_str())) {
                (Some(w), _) => format!("in his window at {} ({})", hm(&w.at), w.id),
                (None, Some(a)) if a.point == Point::Asked => "nobody has it".to_string(),
                (None, Some(a)) => format!("{} at {} by {}{}", serde_json::to_value(a.point).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default(), a.last.as_ref().map_or(String::new(), |r| hm(&r.at)), a.who.as_deref().unwrap_or("?"), a.last.as_ref().and_then(|r| r.via.as_deref()).map_or(String::new(), |v| format!(" via {v}"))),
                (None, None) => "?".into(),
            };
            let flag = if now.not_taken.contains(&t.post) {
                "  << reached, never taken"
            } else if now.open.contains(&t.post) {
                "  << taken, open in the ledger, no commit"
            } else if now.answered.contains(&t.post) {
                "  << closed in words"
            } else {
                ""
            };
            println!("  {} asked {}: {state}{flag}", t.post, t.asked.as_deref().map(hm).unwrap_or_default());
        }
        // the window's two rows of now's lane (proto-window p1560): each whole, in the order he asked them, with when
        let asked_at = |p: &str| all.iter().find(|t| t.post == p).and_then(|t| t.asked.as_deref()).map(hm).unwrap_or_default();
        for (row, xs) in [("reached and never taken", &now.not_taken), ("open in the ledger, no commit", &now.open)] {
            println!("{row}, all {}: {}", xs.len(), xs.iter().map(|p| format!("{p} ({})", asked_at(p))).collect::<Vec<_>>().join(", "));
        }
        println!("taken with no commit, the last ten: {}", now.no_commit.iter().rev().take(10).rev().cloned().collect::<Vec<_>>().join(" "));
        println!("closed in words, the last ten: {}", now.answered.iter().rev().take(10).rev().cloned().collect::<Vec<_>>().join(" "));
        // what his asks cost by then (p1429 2), from COST_SIDES' sessions
        let costs: BTreeMap<String, graph::Cost> = if sides.is_empty() { BTreeMap::new() } else { g.costs_at(Some(&at)).into_iter().map(|c| (c.post.clone(), c)).collect() };
        let spent = |xs: &[graph::Spent]| -> String {
            if xs.is_empty() {
                return "none".into();
            }
            xs.iter().map(|x| format!("{} {}: {} requests, {} carried (input {}, cache write {}, cache read {}, output {}, of it thinking {})", x.agent, x.what, x.requests, x.carried, x.tokens.input, x.tokens.cache_write, x.tokens.cache_read, x.tokens.output, x.tokens.reasoning)).collect::<Vec<_>>().join("; ")
        };
        let total = |xs: &[graph::Spent]| xs.iter().fold(graph::Tokens::default(), |mut t, x| {
            t += x.tokens;
            t
        });
        if !sides.is_empty() {
            let requests: Vec<&graph::Node> = g.nodes.iter().filter(|n| n.kind == graph::NodeKind::Request && n.when.as_deref().is_none_or(|w| w <= at.as_str())).collect();
            // a cost is its `handed` alone; `named` is shown beside it as named by, never added in (p1535)
            let counted: usize = costs.values().map(|c| c.handed.iter().map(|x| x.requests).sum::<usize>()).sum();
            let named_by: usize = costs.values().map(|c| c.named.iter().map(|x| x.requests).sum::<usize>()).sum();
            println!(
                "cost: the sessions of {} ({} givens under the accounts root read with no place; {} files unopened: {unopened:?}); {} requests by then, {} of them carried; counted {counted} times over his asks, {} of those for one shared with another ask; named by: {named_by} more, in no cost",
                sides.iter().cloned().collect::<Vec<_>>().join(","),
                kept_out,
                unopened.len(),
                requests.len(),
                requests.iter().filter(|n| n.tokens.is_some()).count(),
                costs.values().map(|c| c.shared).sum::<usize>()
            );
            let mut most: Vec<(&String, u64)> = costs.iter().map(|(k, c)| (k, total(&c.handed).total())).filter(|(_, t)| *t > 0).collect();
            most.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            println!("cost: the costliest ten by tokens (handed alone): {}", most.iter().take(10).map(|(k, t)| format!("{k} {t}")).collect::<Vec<_>>().join(", "));
        }
        // Open → done's asks as coordd gives them under its lock (`Core::insight_asks`, paper-research N-1446 p1786, 2):
        // each part over the whole graph, as coordd reads it, then the whole, its JSON, and the row's numbers
        {
            let names: Vec<String> = agents.iter().cloned().collect();
            let ms = |t: std::time::Instant| t.elapsed().as_secs_f64() * 1000.0;
            let t = std::time::Instant::now();
            let costs = g.costs_at(None);
            let costs_ms = ms(t);
            let t = std::time::Instant::now();
            let asks = g.asks_now(None);
            let asks_ms = ms(t);
            let t = std::time::Instant::now();
            let after = g.his_after(&asks.iter().map(|a| &a.times).collect::<Vec<_>>());
            let after_ms = ms(t);
            let t = std::time::Instant::now();
            let labels = asks.iter().filter(|a| !crate::label::of(&m, &names, &a.times.post).label.is_empty()).count();
            let labels_ms = ms(t);
            let t = std::time::Instant::now();
            let rows = Core::asks_rows(&g, &m, &names);
            let whole_ms = ms(t);
            let t = std::time::Instant::now();
            let json: usize = rows.iter().filter_map(|x| serde_json::to_string(x).ok()).map(|l| l.len() + 1).sum();
            let json_ms = ms(t);
            println!(
                "asks under the lock: costs_at {} costs in {costs_ms:.1} ms; asks_now {} asks in {asks_ms:.1} ms; his_after {} posts in {after_ms:.1} ms; labels {labels} in {labels_ms:.1} ms; the whole asks_rows {} rows in {whole_ms:.1} ms; its JSON {} KB in {json_ms:.1} ms",
                costs.len(),
                asks.len(),
                after.iter().map(Vec::len).sum::<usize>(),
                rows.len(),
                json / 1024
            );
            let n = docket_machine::insight::asks(&rows);
            let with_cost = rows.iter().filter(|r| r.cost.as_ref().is_some_and(|c| !c.handed.is_empty())).count();
            println!("asks row: {} asked, {} nobody moves, {} answered in words, {} in his window, {} signed off, {} his to sign; {with_cost} with a cost", n.asked.len(), n.stuck.len(), n.words.len(), n.window.len(), n.signed.len(), n.yours.len());
            let some = |v: &[&docket_machine::insight::Ask]| v.iter().take(10).map(|a| format!("{} ({})", a.ask.times.post, a.ask.times.asked.as_deref().map(hm).unwrap_or_default())).collect::<Vec<_>>().join(", ");
            println!("asks row, nobody moves, the oldest ten: {}", some(&n.stuck));
            println!("asks row, in his window, the latest ten: {}", some(&n.window));
        }
        // each kept read against its whole pass (p1828's leg), row for row, each with its time; and `Graph::now` at the
        // canvas's own ask, the wall clock's second as label.rs asks it, kept and as train 68's whole pass
        {
            fn twin<T: PartialEq + std::fmt::Debug>(name: &str, kept: &[T], whole: &[T], kept_ms: f64, whole_ms: f64) -> bool {
                let apart = kept.iter().zip(whole).position(|(a, b)| a != b).or_else(|| (kept.len() != whole.len()).then(|| kept.len().min(whole.len())));
                println!("kept twin {name}: {} rows kept, {} whole, {}; kept {kept_ms:.1} ms, whole pass {whole_ms:.1} ms", kept.len(), whole.len(), if apart.is_none() { "the same row for row" } else { "APART" });
                if let Some(i) = apart {
                    println!("  first apart at row {i}: kept {:?}; whole {:?}", kept.get(i), whole.get(i));
                }
                apart.is_none()
            }
            let ms = |t: std::time::Instant| t.elapsed().as_secs_f64() * 1000.0;
            let canvas = crate::core::at_secs(chrono::Utc::now().timestamp());
            let mut same = true;
            for (name, upto) in [("ask_times_at, the whole graph", None), ("ask_times_at, at AT", Some(at.as_str()))] {
                let t = std::time::Instant::now();
                let k = g.ask_times_at(upto);
                let k_ms = ms(t);
                let t = std::time::Instant::now();
                let w = g.ask_times_walked(upto);
                same &= twin(name, &k, &w, k_ms, ms(t));
            }
            let t = std::time::Instant::now();
            let k = g.costs_at(None);
            let k_ms = ms(t);
            let t = std::time::Instant::now();
            let w = g.costs_walked(None);
            same &= twin("costs_at, the whole graph", &k, &w, k_ms, ms(t));
            let t = std::time::Instant::now();
            let k = g.asks_now(None);
            let k_ms = ms(t);
            let t = std::time::Instant::now();
            let w = g.asks_now_walked(None);
            same &= twin("asks_now, the whole graph", &k, &w, k_ms, ms(t));
            let times: Vec<&graph::AskTimes> = k.iter().map(|a| &a.times).collect();
            let t = std::time::Instant::now();
            let ka = g.his_after(&times);
            let k_ms = ms(t);
            let t = std::time::Instant::now();
            let wa = g.his_after_walked(&times);
            same &= twin("his_after", &ka, &wa, k_ms, ms(t));
            for (name, at) in [("now at the canvas's ask", canvas.as_str()), ("now at AT", at.as_str())] {
                let t = std::time::Instant::now();
                let k = g.now(at);
                let k_ms = ms(t);
                // its asks are `asks_now` at that moment; the rest is its agents, their reds and its counts
                let t = std::time::Instant::now();
                let asks = g.asks_now(Some(at));
                let asks_ms = ms(t);
                println!("{name}, its parts: its asks (asks_now at {at}, {} asks) {asks_ms:.1} ms, so its agents and the rest about {:.1} ms", asks.len(), k_ms - asks_ms);
                let t = std::time::Instant::now();
                let w = g.now_walked(at);
                let w_ms = ms(t);
                same &= twin(&format!("{name} ({at}), its agents"), &k.agents, &w.agents, k_ms, w_ms);
                same &= twin(&format!("{name}, its asks"), &k.asks, &w.asks, k_ms, w_ms);
                same &= twin(&format!("{name}, the whole answer"), std::slice::from_ref(&k), std::slice::from_ref(&w), k_ms, w_ms);
            }
            println!("kept twins: {}", if same { "each the same as its whole pass" } else { "SOME APART" });
            mem("after the reads", &g);
            // with SAVE_GRAPH=DIR, the graph as it stands and how it grew, for its reads timed apart off the lock
            // (docket-machine's `tests::kept::a_saved_graph_s_reads_timed`, paper-research N-1446 p1865)
            if let Some(dir) = Some(var("SAVE_GRAPH")).filter(|d| !d.is_empty()) {
                let t = std::time::Instant::now();
                let write = || -> std::io::Result<u64> {
                    std::fs::create_dir_all(&dir)?;
                    let file = format!("{dir}/graph.json");
                    let mut w = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(&file)?);
                    serde_json::to_writer(&mut w, &g)?;
                    std::io::Write::flush(&mut w)?;
                    std::fs::write(format!("{dir}/moments.json"), serde_json::to_string(&serde_json::json!({"canvas": canvas, "at": at, "grew": grew}))?)?;
                    Ok(std::fs::metadata(&file)?.len())
                };
                match write() {
                    Ok(n) => println!("saved: the graph in {dir}/graph.json, {} MB, and how it grew ({} lengths) in {dir}/moments.json, in {:.1} s", n >> 20, grew.len(), t.elapsed().as_secs_f64()),
                    Err(e) => println!("saved: not: {e}"),
                }
            }
            assert!(same, "a kept read is apart from its whole pass");
        }
        // each ask NOW_ASKS names, its whole course by then: each point's time, node and `via`
        for name in var("NOW_ASKS").split(',').map(str::trim).filter(|a| !a.is_empty()) {
            let Some(t) = all.iter().find(|t| t.post == name) else {
                println!("{name}: no ask of his by then");
                continue;
            };
            println!("{name}: asked {}", t.asked.as_deref().unwrap_or("at no time"));
            println!("  asks: {}", if t.asks.is_empty() { "no one, by the ledger".to_string() } else { t.asks.join(", ") });
            for (point, r) in [("reached", &t.reached), ("taken", &t.taken), ("closed", &t.closed), ("first_commit", &t.first_commit), ("merged", &t.merged), ("first_green", &t.first_green), ("in_window", &t.in_window)] {
                println!("  {point}: {}", r.as_ref().map_or("none".into(), |r| format!("{} {}{}", r.at, r.id, r.via.as_deref().map_or(String::new(), |v| format!(" via {v}")))));
            }
            if let Some(c) = costs.get(name) {
                println!("  cost: {} requests, {} tokens; shared with another ask: {} of them", c.handed.iter().map(|x| x.requests).sum::<usize>(), total(&c.handed).total(), c.shared);
                println!("  cost, handed: {}", spent(&c.handed));
                println!("  named by, in no cost: {}", spent(&c.named));
            }
        }
        // every node and edge kind with its count
        let (nodes, edges) = g.counts();
        let named = |v: Result<serde_json::Value, serde_json::Error>| v.ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
        println!("node kinds: {}", nodes.iter().map(|(k, n)| format!("{} {n}", named(serde_json::to_value(k)))).collect::<Vec<_>>().join(", "));
        println!("edge kinds: {}", edges.iter().map(|(k, n)| format!("{} {n}", named(serde_json::to_value(k)))).collect::<Vec<_>>().join(", "));
    }

    #[test]
    #[ignore]
    fn a_coords_record_replays() {
        let (Ok(coord), Ok(base)) = (std::env::var("REPLAY_COORD"), std::env::var("REPLAY_BASE")) else { return };
        let project = std::env::var("REPLAY_PROJECT").ok();
        let t = Tickets::load(&coord, &base, project.as_deref(), 1);
        let makes = t.errors.iter().filter(|e| e.contains("now makes")).count();
        let refused = t.errors.iter().filter(|e| e.contains("now refused")).count();
        println!("replay {coord} with {base}: {} problem lines, {makes} now makes, {refused} now refused", t.errors.len());
        // REPLAY_PROBLEMS=N: the first N problem lines, 8 when not said (a leg that changes a copy reads them all)
        for e in t.errors.iter().take(std::env::var("REPLAY_PROBLEMS").ok().and_then(|n| n.parse().ok()).unwrap_or(8)) {
            println!("  {}", e.chars().take(400).collect::<String>());
        }
        // the program's clock (0.2 line 26): each ticket a binding opened names it after the replay; the clocks the defs hold
        println!("  bindings: {} tickets name the binding that opened them; {} clocks", t.m.tickets.values().filter(|x| x.binding.is_some()).count(), t.m.clock_views().len());
        // the goals' `duties` questions (machine/duties.rs): asked only at the lines that touch an open goal (p2204)
        println!("  goals: {} open with their lines kept; their duties asked {} times", t.m.goal_lines.len(), t.m.duties_asked);
        // every number the record gave, as this build's replay leaves it (paper-research N-1446 p327; consult-3 p330,
        // proto-lead p346.3): the tickets seen, hidden and the message runtime's, each with the next it would give, and
        // the cards on them; counts and hashes, the same from a build before the counter when nothing was refused
        let hash = |s: String| {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            s.hash(&mut h);
            format!("{:016x}", h.finish())
        };
        println!("  numbers: problem lines hash {}", hash(t.errors.join("\n")));
        for (name, from, to, next) in [("seen", 0, docket_machine::HIDDEN, t.m.next_number()), ("hidden", docket_machine::HIDDEN, docket_machine::DESK, t.m.next_hidden()), ("desk", docket_machine::DESK, u64::MAX, t.m.next_desk())] {
            let ts: Vec<u64> = t.m.tickets.range(from..to).map(|(t, _)| *t).collect();
            println!("  numbers: {name} {} tickets, {:?}..={:?}, next {next}, hash {}", ts.len(), ts.first(), ts.last(), hash(format!("{ts:?}")));
        }
        let cards: Vec<(u64, u32)> = t.m.tickets.values().filter(|x| x.halted > 0).map(|x| (x.t, x.halted)).collect();
        println!("  numbers: cards on {} tickets, {} in all, hash {}", cards.len(), cards.iter().map(|c| u64::from(c.1)).sum::<u64>(), hash(format!("{cards:?}")));
        // REPLAY_SHOW=ID,ID...: how each of these tickets stands after the load, and what it has given or holds
        for id in std::env::var("REPLAY_SHOW").unwrap_or_default().split(',').filter(|s| !s.trim().is_empty()) {
            match docket_machine::parse_ticket(id).and_then(|n| t.m.view(n)) {
                Some(v) => println!("  show {id}: {} ({}), {:?}, spent {}, closed {:?}, last post by and head {:?}", v.id, v.kind, v.status, v.spent, v.closed_at, v.posts.last().map(|p| (p.n, p.by.clone(), p.head.clone()))),
                None => println!("  show {id}: no such ticket"),
            }
        }
        // his posts each agent's deliveries name at their foot as owed (`owed_posts`), and the owed ones an agent they
        // were handed to answered, which the foot names no more (`owed_answered`; paper-research N-1446 p2348)
        let to: std::collections::BTreeSet<&str> = t.m.tickets.values().flat_map(|x| x.given.iter().map(|g| g.to.as_str())).collect();
        let feet: Vec<(&str, Vec<String>, Vec<String>)> = to.into_iter().map(|a| (a, t.m.owed_posts(a), t.m.owed_answered(a))).filter(|(_, o, n)| !o.is_empty() || !n.is_empty()).collect();
        println!("  owed at the foot: {} agents, {} posts; answered by one they ask or were handed to, named no more: {} agents, {} posts", feet.iter().filter(|f| !f.1.is_empty()).count(), feet.iter().map(|f| f.1.len()).sum::<usize>(), feet.iter().filter(|f| !f.2.is_empty()).count(), feet.iter().map(|f| f.2.len()).sum::<usize>());
        for (a, owed, answered) in &feet {
            println!("  owed {a}: at the foot {owed:?}, named no more {answered:?}");
        }
        // his posts asked with no answer yet (`TicketView::unanswered_owner`, the Threads page's "Asked, no answer yet"):
        // by the ledger's rule before (an agent asked with no answer of its own) and now, answered for all once one it asks
        // or was handed answers (`Machine::his_answered`, the foot's rule; paper-research N-1446 p2355); all tickets, and
        // those neither closed nor archived, with the ids that leave the list there
        let (mut all, mut open, mut gone) = ((0, 0), (0, 0), Vec::new());
        for k in t.m.tickets.values().map(|x| x.t).collect::<Vec<u64>>() {
            let Some(v) = t.m.view(k) else { continue };
            let mut before: Vec<u32> = v.asked.iter().filter(|a| a.answer.is_none()).map(|a| a.post).collect();
            before.dedup();
            all = (all.0 + before.len(), all.1 + v.unanswered_owner.len());
            if v.closed_at.is_none() && !v.archived {
                open = (open.0 + before.len(), open.1 + v.unanswered_owner.len());
                gone.extend(before.iter().filter(|n| !v.unanswered_owner.contains(n)).map(|n| format!("{}.p{n}", v.id)));
            }
        }
        println!("  asked, no answer yet: {} posts before, {} now; on tickets neither closed nor archived {} before, {} now", all.0, all.1, open.0, open.1);
        println!("  asked, no answer yet, answered by one it asks or was handed, neither closed nor archived: {gone:?}");
        // the givings that now show nothing (consult-3 N-412.p1982, 1): deliveries of posts whose posts by others are
        // all shown to nobody (coordd's notices, N-412.p1844, 2), how many were made due again unread, and in how many
        // agents' turns still counted (`given_since`): those meet `turn_shows_nothing` at the install's first turn end
        let mut hidden: Vec<String> = Vec::new();
        let mut unread = 0;
        for x in t.m.tickets.values() {
            for g in &x.given {
                if docket_machine::post_id(&g.delivery).is_none() {
                    continue;
                }
                let others: Vec<&docket_machine::Post> = x.posts.iter().filter(|p| p.n > g.before && p.n <= g.upto && p.by != g.to).collect();
                if !others.is_empty() && !others.iter().any(|p| p.visible()) {
                    unread += usize::from(g.unread);
                    hidden.push(format!("{} to {}{}", g.delivery, g.to, if g.unread { " (unread)" } else { "" }));
                }
            }
        }
        let counted: Vec<String> = t.m.tickets.values().flat_map(|x| x.given.iter().map(|g| g.to.clone())).collect::<BTreeSet<_>>().into_iter().filter(|a| t.m.turn_shows_nothing(a, &[])).collect();
        println!("givings that now show nothing: {}, {unread} of them made due again; turns counted on nothing else now: {counted:?}", hidden.len());
        for h in hidden.iter().take(12) {
            println!("  {h}");
        }
        // what `Core::release_picks` would do at the load, in memory only
        let mut m = t.m;
        let held: Vec<u64> = m.tickets.values()
            .filter(|x| x.is_open() && x.kind == "thread" && matches!(&x.status, docket_machine::Status::Awaits { agent, owed, .. } if agent == "owner" && owed.call == "pick"))
            .map(|x| x.t)
            .collect();
        for t in held {
            let out = m.apply(Input::Withdraw { t, by: "coordd".into(), program: "return Thread".into(), why: "replay".into() }, "2026-11-16T23:00:00Z".into());
            let x = &m.tickets[&t];
            println!("  release {}: {:?}; then {:?}, last post naming {:?}", m.id(t), out.reply.map(|_| "ok"), m.view(t).map(|v| format!("{:?}", v.status)), x.posts.last().map(|p| p.names.clone()));
        }
        // what the install's first start does to the record (consult-3 N-412.p2203, 4; p2210, 4), in memory only: the
        // era ERA_PUT_IN's line, when `load_tickets` writes it (a post put into a turn given now), and then a cut of each
        // turn that holds his post now, as the start makes for an agent whose process it finds ended (and any later cut
        // of that turn). Each giving either reads or makes due again, by agent: the posts of his in it, when it was given
        let marks = |m: &docket_machine::Machine| -> BTreeMap<(u64, usize), (bool, bool)> { m.tickets.iter().flat_map(|(t, x)| x.given.iter().enumerate().map(move |(i, g)| ((*t, i), (g.read.is_some(), g.left)))).collect() };
        let changed = |m: &docket_machine::Machine, before: &BTreeMap<(u64, usize), (bool, bool)>| -> Vec<String> {
            let mut rows: Vec<(String, String)> = Vec::new();
            for (t, x) in m.tickets.iter() {
                for (i, g) in x.given.iter().enumerate() {
                    let Some((read, left)) = before.get(&(*t, i)) else { continue };
                    let what = if !left && g.left { "made due again" } else if !read && g.read.is_some() { "read" } else { continue };
                    let his: Vec<String> = x.posts.iter().filter(|p| p.n > g.before && p.n <= g.upto && p.by == "owner" && p.visible() && p.wakes().any(|w| *w == g.to)).map(|p| format!("{}.p{}", m.id(*t), p.n)).collect();
                    let his = if his.is_empty() { String::from("none of his") } else { format!("his {}", his.join(", ")) };
                    rows.push((g.to.clone(), format!("{}: {} ({his}), given {}: {what}", g.to, g.delivery, g.at)));
                }
            }
            rows.sort();
            rows.into_iter().map(|(_, r)| r).collect()
        };
        let show = |what: &str, rows: &[String]| {
            println!("{what}: {} givings, {} of them made due again", rows.len(), rows.iter().filter(|r| r.ends_with("made due again")).count());
            for r in rows.iter().take(200) {
                println!("  {r}");
            }
            if rows.len() > 200 {
                println!("  and {} more", rows.len() - 200);
            }
        };
        let at = String::from("2026-11-20T23:00:00Z");
        let agents: BTreeSet<String> = m.tickets.values().flat_map(|x| x.given.iter().map(|g| g.to.clone())).collect();
        let cut: Vec<String> = agents.iter().filter(|a| m.cut_holds_his(a)).cloned().collect();
        if m.era < docket_machine::ERA_PUT_IN && (m.holds_put_in() || !cut.is_empty()) {
            let before = marks(&m);
            let by = if m.holds_put_in() { "the start (a post put into a turn is given now)" } else { "the first cut that holds his post" };
            let out = m.apply(Input::Event { name: docket_machine::ERA.into(), value: json!({"era": docket_machine::ERA_PUT_IN}) }, at.clone());
            show(&format!("the era line, written by {by}: {:?}", out.reply.map(|_| "ok")), &changed(&m, &before));
        } else {
            println!("the era line: none written (era {}, no post put into a turn given now, no turn holding his post)", m.era);
        }
        let before = marks(&m);
        for a in &cut {
            let _ = m.apply(Input::TurnCut { agent: a.clone() }, at.clone());
        }
        show(&format!("a cut of each turn holding his post now ({}: {})", cut.len(), cut.join(", ")), &changed(&m, &before));
    }

    /// The snapshot coordd would send for a copy of a coord, read as the window reads a bridge line (yaaihh link.rs
    /// `Line`: `{"event": coord_types::Event}`): a release check fails when the window could not read it. 697780a's
    /// mentions (`BTreeMap<u32, _>` under the flattened body) went out to three releases unread by the window
    /// (paper-research N-542). REPLAY_COORD (a copy: Core::load may write in it), REPLAY_BASE.
    #[test]
    #[ignore]
    fn a_coords_snapshot_reads_in_the_window() {
        let (Ok(coord), Ok(base)) = (std::env::var("REPLAY_COORD"), std::env::var("REPLAY_BASE")) else { return };
        let mut core = crate::core::Core::load(&coord, &base);
        core.project = crate::project::Project::load(&coord).unwrap();
        core.load_tickets();
        // what a whole snapshot costs under the lock, part by part (a watch takes it there: 2-4.8 s in research,
        // paper-research N-560 p29, 2)
        let time = |what: &str, f: &dyn Fn() -> usize| {
            let at = std::time::Instant::now();
            let n = f();
            println!("snapshot part {what}: {n} in {} ms", at.elapsed().as_millis());
        };
        time("agents", &|| core.agents.keys().map(|n| core.view(n)).count());
        time("tickets", &|| core.ticket_views().len());
        time("notices", &|| core.notices().len());
        time("whole", &|| { let s = core.snapshot(); s.seq as usize });
        let line = serde_json::to_string(&serde_json::json!({"event": core.snapshot()})).unwrap();
        // REPLAY_SNAPSHOT_OUT=F: the line as the window reads it, for a stand-in bridge's stills from a copy of a record
        if let Ok(f) = std::env::var("REPLAY_SNAPSHOT_OUT") {
            std::fs::write(f, &line).unwrap();
        }
        #[derive(serde::Deserialize)]
        struct Line {
            event: Option<coord_types::Event>,
        }
        let read = serde_json::from_str::<Line>(&line);
        let said = match &read {
            Ok(Line { event: Some(_) }) => "the window reads it".to_string(),
            Ok(_) => "no event in it".to_string(),
            Err(e) => format!("the window cannot read it: {e}"),
        };
        println!("snapshot {coord}: {} KB, {} tickets with mentions; {said}", line.len() / 1024, core.ticket_views().iter().filter(|v| !v.mentions.is_empty()).count());
        assert!(matches!(read, Ok(Line { event: Some(_) })), "{said}");
    }

    /// An edit tried on a copy of a coord (`coord edit` and the reload, paper-research N-101 p1554), nothing else touched:
    /// EDIT_COORD (a scratch coord: its `project` file and its tickets log, which this appends to), EDIT_BASE, EDIT_TICKET;
    /// EDIT_NEXT: a folder whose files are put over the project's before a reload by the owner; EDIT_PROGRAM: a program
    /// given as the owner's `coord edit` before the reload and after it. Prints the ticket's status and continuation at
    /// each point, and what the reload and the edits answered.
    #[test]
    #[ignore]
    fn an_edit_tried_on_a_copy() {
        let (Ok(coord), Ok(base), Ok(id)) = (std::env::var("EDIT_COORD"), std::env::var("EDIT_BASE"), std::env::var("EDIT_TICKET")) else { return };
        let mut core = crate::core::Core::load(&coord, &base);
        core.project = crate::project::Project::load(&coord).unwrap();
        core.load_tickets();
        let t = docket_machine::parse_ticket(&id).unwrap();
        let show = |core: &crate::core::Core, what: &str| {
            let v = core.tickets.m.view(t);
            println!("=== {what}: {id} {}\n{}", v.as_ref().map(|v| format!("{:?}", v.status)).unwrap_or_default().chars().take(300).collect::<String>(), v.and_then(|v| v.continuation).unwrap_or_else(|| "(no continuation)".into()));
        };
        println!("=== load: {} problem lines", core.tickets.errors.len());
        show(&core, "loaded");
        let program = std::env::var("EDIT_PROGRAM").ok().map(|p| std::fs::read_to_string(p).unwrap());
        let who = crate::core::Who::Owner;
        if let Some(p) = &program {
            let r = core.ticket_edit_program(&who, &id, p.clone(), "tried on a copy".into(), Default::default());
            println!("=== edit before the reload: ok {}: {}", r.ok, r.out);
        }
        if let Ok(next) = std::env::var("EDIT_NEXT") {
            let dir = core.project.as_ref().unwrap().dir.clone();
            let ok = std::process::Command::new("cp").args(["-a", &format!("{next}/."), &dir]).status().unwrap();
            assert!(ok.success());
            let r = core.ticket_reload(&who, None, "tried on a copy".into());
            println!("=== reload: ok {}: {}", r.ok, r.out);
            show(&core, "after the reload");
            if let Some(p) = &program {
                let r = core.ticket_edit_program(&who, &id, p.clone(), "tried on a copy".into(), Default::default());
                println!("=== edit after the reload: ok {}: {}", r.ok, r.out);
                show(&core, "after the edit");
            }
        }
    }

    /// An edit taken on a copy of a coord's record, as `coord edit T WHY < PROGRAM` takes it, and the record replayed with
    /// its line (paper-research N-101 p1631). `REPLAY_COORD=DIR REPLAY_BASE=DIR REPLAY_PROJECT=DIR REPLAY_EDIT=T
    /// REPLAY_PROGRAM=FILE REPLAY_INTO=DIR cargo test -p coordd an_edit_on_a_coords_record -- --ignored --nocapture`.
    /// It prints `coord show --program T` before and after, the edit's answer, and the problem lines of the load before
    /// it and of the replay of REPLAY_INTO (a new folder: the record with the edit's lines after it). REPLAY_COORD is
    /// only read.
    #[test]
    #[ignore]
    fn an_edit_on_a_coords_record() {
        let var = |k: &str| std::env::var(k).ok();
        let (Some(coord), Some(base), Some(id), Some(file), Some(into)) = (var("REPLAY_COORD"), var("REPLAY_BASE"), var("REPLAY_EDIT"), var("REPLAY_PROGRAM"), var("REPLAY_INTO")) else { return };
        let project = var("REPLAY_PROJECT");
        assert!(!std::path::Path::new(&into).exists(), "REPLAY_INTO {into} exists");
        let count = |t: &Tickets, what: &str| {
            let makes = t.errors.iter().filter(|e| e.contains("now makes")).count();
            let refused = t.errors.iter().filter(|e| e.contains("now refused")).count();
            println!("{what}: {} problem lines, {makes} now makes, {refused} now refused", t.errors.len());
            for e in t.errors.iter().take(4) {
                println!("  {}", e.chars().take(300).collect::<String>());
            }
        };
        let mut t = Tickets::load(&coord, &base, project.as_deref(), 1);
        count(&t, &format!("load of {coord}"));
        let n = docket_machine::parse_ticket(&id).expect("REPLAY_EDIT is a ticket");
        let show = |m: &Machine| m.view(n).and_then(|v| v.continuation).unwrap_or_else(|| "coord show --program: it awaits no step now, so there is nothing to edit".into());
        println!("$ coord show --program {id}  (before)\n{}", show(&t.m));
        // as coordc sends it: only what follows `answer <- (in flight);`
        let text = std::fs::read_to_string(&file).expect("REPLAY_PROGRAM");
        let program = match text.split_once("answer <- (in flight);") {
            Some((_, rest)) => rest.trim_start_matches('\n').to_string(),
            None => text,
        };
        let at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let out = t.m.apply(Input::Edit { t: n, by: "owner".into(), program, why: "paper-research N-101 p1631, item 1".into() }, at.into());
        match &out.reply {
            Ok(r) => println!("$ coord edit {id} ... < {file}\n{r}"),
            Err(e) => println!("$ coord edit {id} ... < {file}\ncoord: {e}"),
        }
        println!("{} line(s) made", out.lines.len());
        println!("$ coord show --program {id}  (after)\n{}", show(&t.m));
        std::fs::create_dir_all(format!("{into}/tickets")).unwrap();
        let mut log = std::fs::read_to_string(format!("{coord}/tickets/log.jsonl")).unwrap();
        if !log.is_empty() && !log.ends_with('\n') {
            log.push('\n');
        }
        for l in &out.lines {
            log.push_str(&serde_json::to_string(l).unwrap());
            log.push('\n');
        }
        std::fs::write(format!("{into}/tickets/log.jsonl"), log).unwrap();
        let again = Tickets::load(&into, &base, project.as_deref(), 1);
        count(&again, &format!("replay of {into} (the record and the edit)"));
        println!("after the replay, coord show --program {id} is the same: {}", show(&again.m) == show(&t.m));
    }

    /// agent-base's notify.eflint (paper-research m1237, N-101.p1285): on a thread the owner's post tells every agent on
    /// it; an agent's post informs everyone else on it, the owner too, waking nobody and telling him nothing (a reply
    /// to him names him: machine.rs `unanswered`); elsewhere it adds nobody.
    #[test]
    fn on_a_thread_an_agents_post_informs_and_the_owners_is_left_to_its_handler() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let places = crate::policy::Places { base, project: None, coord: std::env::temp_dir().display().to_string(), ..Default::default() };
        let f = |a: &str, b: &[&str]| (a.to_string(), b.iter().map(|x| x.to_string()).collect::<Vec<_>>());
        let on = |kind: &str| vec![
            f("agent", &["lead"]), f("agent", &["fixer"]), f("agent", &["runner"]),
            f("role", &["lead", "lead"]), f("role", &["fixer", "fixer"]),
            f("ticket", &["N-6"]), f("ticket-kind", &["N-6", kind]),
            f("takes-part", &["owner", "N-6"]), f("takes-part", &["lead", "N-6"]), f("takes-part", &["fixer", "N-6"]),
        ];
        let ask = |who: &str, kind: &str| may(MayAsk { places: places.clone(), facts: on(kind), act: "post", who: who.into(), ticket: Some("N-6".into()), kind: kind.into(), bound: who != "owner" });
        // the owner's post is base.kind's owner_post handler's to tell (design §13): policy tells nobody of it
        assert_eq!(ask("owner", "thread"), Ok(Said::default()));
        assert_eq!(ask("lead", "thread"), Ok(Said { unchecked: None, told: vec![], inform: vec!["fixer".into(), "owner".into()] }));
        assert_eq!(ask("fixer", "thread"), Ok(Said { unchecked: None, told: vec![], inform: vec!["lead".into(), "owner".into()] }));
        // runner takes no part; and on a claim nobody more is told
        assert_eq!(ask("lead", "claim"), Ok(Said::default()));
    }

    /// I-1480 (paper-research N-1481 p15): whoever works on a ticket (`coord item start`, `Ticket::started`) takes part
    /// in it, so an agent's post on an item reaches its workers' digest, as notify.eflint informs a thread's takers:
    /// pulled, waking nobody.
    #[test]
    fn an_items_worker_takes_part_in_it_and_is_informed_of_its_posts() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let places = crate::policy::Places { base: base.clone(), project: None, coord: std::env::temp_dir().display().to_string(), ..Default::default() };
        let f = |a: &str, b: &[&str]| (a.to_string(), b.iter().map(|x| x.to_string()).collect::<Vec<_>>());
        let on = vec![
            f("agent", &["lead"]), f("agent", &["fixer"]), f("role", &["lead", "lead"]), f("role", &["fixer", "fixer"]),
            f("ticket", &["I-143"]), f("ticket-kind", &["I-143", "item"]), f("takes-part", &["lead", "I-143"]), f("takes-part", &["fixer", "I-143"]),
        ];
        let ask = |who: &str| may(MayAsk { places: places.clone(), facts: on.clone(), act: "post", who: who.into(), ticket: Some("I-143".into()), kind: "item".into(), bound: true });
        assert_eq!(ask("lead"), Ok(Said { unchecked: None, told: vec![], inform: vec!["fixer".into()] }));
        // coordd's facts: the item's worker takes part, though it never posted there
        let coord = std::env::temp_dir().join(format!("docket-works-on-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let r = core.ticket_input(Input::Open { kind: "item".into(), under: None, opener: "owner".into(), form: json!({"title": "the median", "note": null}), text: None, names: vec![], told: vec![] }).unwrap();
        let t = docket_machine::parse_ticket(r.trim_start_matches("Opened ").trim_end_matches('.')).unwrap();
        let id = core.tickets.m.id(t);
        let takes = |core: &Core| core.ticket_facts(Some(t)).0.into_iter().filter(|(n, v)| n == "takes-part" && v[1] == id).map(|(_, v)| v[0].clone()).collect::<Vec<_>>();
        assert!(!takes(&core).contains(&"w2".to_string()), "{:?}", takes(&core));
        core.ticket_input(Input::Started { t, by: "w2".into() }).unwrap();
        assert!(takes(&core).contains(&"w2".to_string()), "{:?}", takes(&core));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// S5: `coord msg` opens a thread or posts; `--re` names the author replied to, `--thread` everyone else; `--esc`
    /// is refused (N-36: `coord esc` first); old mail ids are not tickets; coord's notices are notes; `coord ask`
    /// opens a question.
    #[test]
    fn mail_is_notes_and_posts() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-msg-{}", std::process::id()));
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(
            coord.join("registry.jsonl"),
            "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"orch\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n{\"ev\":\"add\",\"id\":\"AGT-002\",\"name\":\"dev\",\"parent\":\"orch\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n",
        )
        .unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let (orch, dev) = (Who::Agent("orch".into()), Who::Agent("dev".into()));
        let msg = |core: &mut Core, who: &Who, to: &[&str], text: &str, esc: bool, re: Option<&str>| {
            core.ticket_msg(who, None, to.iter().map(|s| s.to_string()).collect(), text, esc, re.map(String::from), Vec::new(), Said::default())
        };
        // a note naming whom it is to, closed at once, out of the open list
        let r = msg(&mut core, &dev, &["orch"], "the build is green", false, None);
        assert!(r.ok, "{}", r.out);
        let n = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        let x = &core.tickets.m.tickets[&n];
        assert_eq!((x.kind.as_str(), x.posts[0].names.clone(), x.is_open()), ("thread", vec!["orch".to_string()], false));
        assert!(!core.tickets.m.open().contains(&n));
        // design part 9b: an agent's message is a thread, Sent by dev, ended at once as Between; no intake, no hole,
        // not among the tickets dev or orch is on, and no closing notice to dev for its own message
        assert_eq!((x.between(), x.form["addressed"].clone(), x.form["to"].clone()), (true, json!({"head": "Sent", "by": "dev"}), json!("orch")));
        assert!(core.ticket_due(docket_machine::WATCHER).is_empty(), "no model call on an agents' thread");
        assert!(!core.tickets.m.open_on("dev").contains(&n) && !core.tickets.m.open_on("orch").contains(&n));
        assert!(core.ticket_due("dev").iter().all(|d| d.t != n), "{:?}", core.ticket_due("dev"));
        let id = core.tickets.m.id(n);
        // --re a post: its author, and whom it names besides
        let r = msg(&mut core, &orch, &["owner"], "thanks", false, Some(&format!("{id}.p1")));
        assert!(r.ok, "{}", r.out);
        assert_eq!(core.tickets.m.tickets[&n].posts[1].names, vec!["owner".to_string(), "dev".to_string()]);
        // --thread: everyone in it but the sender
        let r = msg(&mut core, &dev, &[], "and the docs", false, Some(&id));
        assert!(r.ok, "{}", r.out);
        assert_eq!(core.tickets.m.tickets[&n].posts[2].names, vec!["orch".to_string(), "owner".to_string()]);
        // --esc is refused with the one line, and nothing is posted (N-36)
        let posts = core.tickets.m.tickets[&n].posts.len();
        let tickets = core.tickets.m.tickets.len();
        let r = msg(&mut core, &orch, &["dev"], "stop that", true, Some(&id));
        assert!(!r.ok && r.out == coord_types::ESC_APART, "{}", r.out);
        assert_eq!((core.tickets.m.tickets[&n].posts.len(), core.tickets.m.tickets.len()), (posts, tickets));
        // an old mail's id is not a ticket, even when the number is one
        assert!(reply_of(&core.tickets.m, &format!("m{n}")).is_none());
        assert_eq!(reply_of(&core.tickets.m, &format!("{id}.p2")), Some((n, Some(2))));
        // coord's notice to the owner: a note, closed
        let before = core.tickets.m.open().len();
        core.note_from_coord("owner", "New dialog on dev's screen");
        assert_eq!(core.tickets.m.open().len(), before);
        // coord ask: a question, its first line the ask
        let r = core.ticket_ask(&dev, "Which branch?\nlanded or main", None, Said::default());
        assert!(r.ok, "{}", r.out);
        let q = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        let x = &core.tickets.m.tickets[&q];
        assert_eq!((x.kind.as_str(), x.form.clone()), ("question", json!({"ask": "Which branch?", "context": "landed or main"})));
        let log = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&coord);
        assert!(!log.contains("\"esc\""), "no new line carries an esc: {log}");
    }

    #[test]
    fn a_span_is_shown_as_words() {
        let span = json!({"span": {"loc": {"file": "/nonexistent/t.jsonl", "offset": 0, "len": 9}, "at": 2, "len": 3}});
        let v = worded(&json!({"ask": span.clone(), "n": 3, "s": "words"}));
        assert_eq!(v, json!({"ask": "(the text at /nonexistent/t.jsonl:0 is gone)", "n": 3, "s": "words"}));
        assert_eq!(crate::render::resolved(&span), v["ask"]);
    }

    /// A turn given one note, taken as coordd takes it (handed, then seen), ends: the machine replies for it (Reply).
    /// Live round: coordd once kept the turn's delivery for steps only, so a turn given posts reached the machine with
    /// no delivery; it now reads the machine's count.
    #[test]
    fn a_turn_given_a_note_is_replied_for() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-reply-{}", std::process::id()));
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let r = core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("what is 6 times 7?".into())), names: vec!["w2".into()], told: vec![] });
        assert!(r.is_ok(), "{r:?}");
        let d = core.ticket_due("w2").into_iter().next().expect("the note is due");
        core.tickets.handed.insert(("w2".into(), d.id.clone()), (d.t, d.upto, Default::default()));
        core.ticket_handed("w2", std::slice::from_ref(&d.id), true);
        let loc = Loc { file: "/nonexistent/w2.jsonl".into(), offset: 10, len: 5 };
        core.ticket_seen("w2", &d.id, loc.clone());
        core.ticket_turn_ended("w2", loc.clone());
        let x = &core.tickets.m.tickets[&d.t];
        let _ = std::fs::remove_dir_all(&coord);
        let p = x.posts.last().unwrap();
        assert_eq!((x.posts.len(), p.by.as_str(), p.auto, p.names.clone()), (2, "w2", true, vec!["owner".to_string()]));
        assert_eq!(p.text, Text::Loc { loc });
    }

    /// A delivery seen again in the transcript, as a follower's first read after a restart or a switch sees every one in
    /// its history (yaaihh N-560 p28: research's first round held the lock 28554 ms), is not put to the machine once it
    /// is given with its place and as far: nothing is recorded and nothing changes. One seen first with no place, or now
    /// further, still is.
    #[test]
    fn a_delivery_seen_again_with_its_place_is_not_put_to_the_machine() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-seen-again-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let r = core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("what is 6 times 7?".into())), names: vec!["w2".into()], told: vec![] });
        assert!(r.is_ok(), "{r:?}");
        let d = core.ticket_due("w2").into_iter().next().expect("the note is due");
        // handed and taken with no place yet: the transcript's entry gives it its place
        core.tickets.handed.insert(("w2".into(), d.id.clone()), (d.t, d.upto, Default::default()));
        core.ticket_handed("w2", std::slice::from_ref(&d.id), true);
        let given = |core: &Core| core.tickets.m.tickets[&d.t].given.iter().filter(|g| g.to == "w2" && g.delivery == d.id).map(|g| (g.upto, g.place.clone())).collect::<Vec<_>>();
        assert_eq!(given(&core), [(d.upto, None)]);
        let loc = Loc { file: "/nonexistent/w2.jsonl".into(), offset: 10, len: 5 };
        core.ticket_seen("w2", &d.id, loc.clone());
        assert_eq!(given(&core), [(d.upto, Some(loc.clone()))], "its place found");
        let path = core.tickets.log.clone();
        let log = || std::fs::read_to_string(&path).unwrap_or_default().lines().count();
        let lines = log();
        // seen again, many times (each round of a first read): nothing recorded, nothing changed, and quick
        let at = std::time::Instant::now();
        for _ in 0..2000 {
            core.ticket_seen("w2", &d.id, loc.clone());
        }
        assert!(at.elapsed() < std::time::Duration::from_millis(500), "{:?}", at.elapsed());
        assert_eq!((log(), given(&core)), (lines, vec![(d.upto, Some(loc.clone()))]));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A slice of a session asked by its giving (`Request::Raw`; paper-research N-1446 p1380): its place is the one the
    /// record holds at the giving's line, and its words are given only when they hold the post that giving brought.
    /// Refused, each saying why: an agent's ask, an id that is no slice, a line that is no giving of it or is past the
    /// record, the giving's line before its place was found, a place whose words are not the post's, a session cut
    /// short. Made up: w2's session is a file of two entries, the delivery the second.
    #[test]
    fn a_slice_is_read_by_its_giving_and_given_only_with_the_posts_it_brought() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-slice-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let r = core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("what is 6 times 7?".into())), names: vec!["w2".into()], told: vec![] });
        assert!(r.is_ok(), "{r:?}");
        let d = core.ticket_due("w2").into_iter().next().expect("the note is due");
        core.tickets.handed.insert(("w2".into(), d.id.clone()), (d.t, d.upto, Default::default()));
        core.ticket_handed("w2", std::slice::from_ref(&d.id), true);
        // w2's session: an entry before, then the delivery as it took it
        let session = coord.join("w2.jsonl");
        let first = json!({"type": "user", "message": {"role": "user", "content": "an earlier turn"}}).to_string() + "\n";
        let entry = |q: &str| json!({"type": "user", "message": {"role": "user", "content": format!("<channel source=\"coord\">owner wrote:\n{q}\n</channel>")}}).to_string() + "\n";
        std::fs::write(&session, first.clone() + &entry("what is 6 times 7?")).unwrap();
        let loc = Loc { file: session.display().to_string(), offset: first.len() as u64, len: entry("what is 6 times 7?").len() as u64 };
        core.ticket_seen("w2", &d.id, loc.clone());
        let lines: Vec<String> = std::fs::read_to_string(&core.tickets.log).unwrap().lines().map(String::from).collect();
        let given = |placed: bool| 1 + lines.iter().position(|l| l.contains("\"ev\":\"given\"") && l.contains("\"where\"") == placed).expect("its given line");
        let (line, unplaced) = (given(true), given(false));
        let id = docket_machine::graph::slice_id("w2", &d.id, line);
        let read = |core: &Core, w: &Who, id: &str| core.ticket_slice(w, id).and_then(|s| super::slice_words(&s));
        let text = read(&core, &Who::Owner, &id).expect("served");
        assert_eq!(text, "<channel source=\"coord\">owner wrote:\nwhat is 6 times 7?\n</channel>");
        let refused = |w: &Who, id: &str, why: &str| {
            let e = read(&core, w, id).expect_err(id);
            assert!(e.contains(why), "{id}: {e}");
            assert!(!e.contains("6 times 7"), "a refusal names no words: {e}");
        };
        refused(&Who::Agent("w2".into()), &id, "the owner's");
        refused(&Who::Owner, &d.id, "no slice");
        refused(&Who::Owner, &docket_machine::graph::slice_id("w2", "N-99.p1", line), "no delivery");
        refused(&Who::Owner, &docket_machine::graph::slice_id("w3", &d.id, line), "never given to w3");
        refused(&Who::Owner, &docket_machine::graph::slice_id("w2", &d.id, 1), "no giving of it");
        refused(&Who::Owner, &docket_machine::graph::slice_id("w2", &d.id, lines.len() + 1), "no such line");
        refused(&Who::Owner, &docket_machine::graph::slice_id("w2", &d.id, unplaced), "no place");
        // the same bytes at the place, other words: not his post
        std::fs::write(&session, first.clone() + &entry("what is 7 times 6?")).unwrap();
        refused(&Who::Owner, &id, "does not hold the words of");
        std::fs::write(&session, first.clone()).unwrap();
        refused(&Who::Owner, &id, "no longer has those bytes");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A delivery made due again (`Unread`) after its place was found, then seen at that same place by a follower's
    /// first read after a start, is not given again: it is the same entry, so no line is written and it stays due.
    /// Handed again, it is a new entry at a new place, and that is given (consult-3 N-412.p1596: N-412.p1373 to
    /// consult-3 and p1336 to docket-edge5, given again at the 20:38 start; docket-core5 N-807.p105).
    #[test]
    fn a_delivery_made_due_again_and_seen_at_its_old_place_is_not_given_again() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-seen-unread-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let r = core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("what is 6 times 7?".into())), names: vec!["w2".into()], told: vec![] });
        assert!(r.is_ok(), "{r:?}");
        let d = core.ticket_due("w2").into_iter().next().expect("the note is due");
        core.tickets.handed.insert(("w2".into(), d.id.clone()), (d.t, d.upto, Default::default()));
        core.ticket_handed("w2", std::slice::from_ref(&d.id), true);
        let loc = Loc { file: "/nonexistent/w2.jsonl".into(), offset: 10, len: 5 };
        core.ticket_seen("w2", &d.id, loc.clone());
        let given = |core: &Core| core.tickets.m.tickets[&d.t].given.iter().filter(|g| g.to == "w2" && g.delivery == d.id).map(|g| (g.place.clone(), g.unread)).collect::<Vec<_>>();
        assert_eq!(given(&core), [(Some(loc.clone()), false)]);
        // it never began a turn on it, so coordd makes it due again
        core.ticket_unread("w2");
        assert_eq!(given(&core), [(Some(loc.clone()), true)]);
        let due = |core: &Core| core.ticket_due("w2").iter().any(|x| x.id == d.id);
        assert!(due(&core), "made due again");
        let path = core.tickets.log.clone();
        let log = || std::fs::read_to_string(&path).unwrap_or_default().lines().count();
        let lines = log();
        // a start: the follower's first read sees the old entry at its old place
        core.ticket_seen("w2", &d.id, loc.clone());
        assert_eq!((log(), given(&core)), (lines, vec![(Some(loc.clone()), true)]), "the same entry: nothing written");
        assert!(due(&core), "still due");
        assert!(core.ticket_given_since("w2").is_empty(), "not counted in the turn going on");
        // handed again: a new entry at a new place is given
        let loc2 = Loc { file: "/nonexistent/w2.jsonl".into(), offset: 900, len: 5 };
        core.ticket_seen("w2", &d.id, loc2.clone());
        assert_eq!(given(&core), [(Some(loc.clone()), true), (Some(loc2.clone()), false)]);
        assert!(!due(&core), "given at its new place");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// Nothing (paper-research N-17): a turn given a note that ends with the single word Nothing posts nothing and
    /// wakes nobody, the note read; 'Nothing more from me.' is a reply and is posted. The thread's address before it
    /// changes nothing (paper-research N-1397 p113), words after it do.
    #[test]
    fn a_turn_that_says_nothing_has_nothing_posted() {
        for (word, posted) in [("Nothing", false), ("nothing.", false), ("  NOTHING.\n", false), ("Nothing more from me.", true), ("Nothing!", true), ("#N-1 Nothing", false), ("#N-1.p1: Nothing.", false), ("#N-1 Nothing yet", true)] {
            let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
            let coord = std::env::temp_dir().join(format!("docket-nothing-{}-{}", std::process::id(), word.trim().len()));
            let _ = std::fs::remove_dir_all(&coord);
            std::fs::create_dir_all(&coord).unwrap();
            std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
            // the turn's last message, as Claude Code writes it in its transcript
            let transcript = coord.join("w2.jsonl");
            let entry = format!("{}\n", json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": word}]}}));
            std::fs::write(&transcript, &entry).unwrap();
            let loc = Loc { file: transcript.display().to_string(), offset: 0, len: entry.len() as u64 };
            let mut core = Core::load(&coord.display().to_string(), &base);
            core.load_tickets();
            core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("FYI: the build is green.".into())), names: vec!["w2".into()], told: vec![] }).unwrap();
            let d = core.ticket_due("w2").into_iter().next().expect("the note is due");
            core.tickets.handed.insert(("w2".into(), d.id.clone()), (d.t, d.upto, Default::default()));
            core.ticket_handed("w2", std::slice::from_ref(&d.id), true);
            core.ticket_seen("w2", &d.id, Loc { file: transcript.display().to_string(), offset: 0, len: 0 });
            core.ticket_turn_ended("w2", loc.clone());
            let x = &core.tickets.m.tickets[&d.t];
            assert_eq!(x.posts.len(), if posted { 2 } else { 1 }, "{word:?}");
            assert!(core.ticket_given_since("w2").is_empty(), "{word:?}: the note counts as read");
            assert_eq!(core.ticket_due("owner").iter().any(|d| d.t == x.t), posted, "{word:?}: the owner is woken only by a reply");
            let quiet = |core: &Core| core.tickets.m.view(d.t).unwrap().quiet.iter().map(|q| (q.agent.clone(), q.upto)).collect::<Vec<_>>();
            if posted {
                assert_eq!(x.posts[1].text, Text::Loc { loc });
                assert!(quiet(&core).is_empty());
            } else {
                // read, no reply (N-19): on the view, and after a restart, from the log
                assert_eq!(quiet(&core), vec![("w2".to_string(), 1)], "{word:?}");
                let mut again = Core::load(&coord.display().to_string(), &base);
                again.load_tickets();
                assert_eq!(quiet(&again), vec![("w2".to_string(), 1)], "{word:?} after a restart");
            }
            let _ = std::fs::remove_dir_all(&coord);
        }
        assert!(says_nothing("Nothing") && says_nothing(" nothing. ") && !says_nothing("Nothing..") && !says_nothing("nothing at all"));
        // addresses with Nothing, one thread or several (paper-research N-1397 p113)
        assert!(says_nothing("#N-12 Nothing") && says_nothing("#N-12.p3: Nothing.\n#2 · nothing") && says_nothing("#N-12\nNothing"));
        assert!(!says_nothing("#N-12") && !says_nothing("#N-12 Nothing\nbut one thing") && !says_nothing("# Nothing") && !says_nothing(""));
    }

    /// The rule Reply after N-17 post 5: a reply to an automatic reply names its author (a chain ends with Nothing),
    /// and its footer says so; a post by a program agent (the runner) promises no reply and none is made.
    #[test]
    fn a_reply_to_a_reply_is_given_and_the_runner_is_not_replied_to() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-footer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let reg = |id: &str, n: &str, kind: &str| format!("{{\"ev\":\"add\",\"id\":\"{id}\",\"name\":\"{n}\",\"parent\":\"owner\",\"kind\":\"{kind}\",\"model\":\"m\",\"at\":\"x\"}}\n");
        std::fs::write(coord.join("registry.jsonl"), reg("AGT-001", "w2", "claude") + &reg("AGT-002", "w3", "claude") + &reg("AGT-003", "runner", "program")).unwrap();
        // a turn of AGENT ending with TEXT, as its transcript has it
        let said = |agent: &str, text: &str| {
            let f = coord.join(format!("{agent}-{}.jsonl", text.len()));
            let entry = format!("{}\n", json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": text}]}}));
            std::fs::write(&f, &entry).unwrap();
            Loc { file: f.display().to_string(), offset: 0, len: entry.len() as u64 }
        };
        let take = |core: &mut Core, agent: &str, t: u64| {
            let d = core.ticket_due(agent).into_iter().find(|d| d.t == t).unwrap_or_else(|| panic!("{agent} owed nothing on {t}"));
            core.tickets.handed.insert((agent.into(), d.id.clone()), (d.t, d.upto, Default::default()));
            core.ticket_handed(agent, std::slice::from_ref(&d.id), true);
        };
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let places = core.places();
        let footer = |core: &Core, agent: &str, t: u64| {
            let d = core.ticket_due(agent).into_iter().find(|d| d.t == t).unwrap();
            crate::render::delivery(&places, &d, &core.tickets.m.view(t).unwrap(), true, false, false, false, core.ticket_reply_names(&d).as_deref()).unwrap().0
        };
        core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "w2".into(), form: json!({"to": "w2", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "w2"}, "follows": null}), text: Some(Text::New("What is 6 times 7?".into())), names: vec!["w3".into()], told: vec![] }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        assert!(footer(&core, "w3", t).ends_with("If the sender needs nothing from you, end with the single word Nothing: nothing is posted and nobody is woken. To add something before then: coord post N-1 TEXT"));
        take(&mut core, "w3", t);
        core.ticket_turn_ended("w3", said("w3", "42. Shall I add it to the table?"));
        let text = footer(&core, "w2", t);
        assert!(text.ends_with("This is w3's reply to you. If it asks you something, or something is still open, answer it: your last message is posted here and given to w3. If nothing is open, end your turn with the single word Nothing."), "{text}");
        // w2 answers the question in w3's reply: given to w3, though w3's post was automatic
        take(&mut core, "w2", t);
        core.ticket_turn_ended("w2", said("w2", "Yes, add it."));
        let p = core.tickets.m.tickets[&t].posts.last().unwrap().clone();
        assert_eq!((p.n, p.by.as_str(), p.auto, p.names.clone()), (3, "w2", true, vec!["w3".to_string()]));
        assert!(core.ticket_due("w3").iter().any(|d| d.t == t));
        // the runner's note: no footer promises a reply, and none is made, nor handed to the runner
        core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "runner".into(), form: json!({"to": "runner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "runner"}, "follows": null}), text: Some(Text::New("check t12: green".into())), names: vec!["w2".into()], told: vec![] }).unwrap();
        let r = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        let text = footer(&core, "w2", r);
        assert!(text.ends_with("check t12: green"), "no footer: {text}");
        take(&mut core, "w2", r);
        core.ticket_turn_ended("w2", said("w2", "Good, thanks."));
        let _ = std::fs::remove_dir_all(&coord);
        assert_eq!(core.tickets.m.tickets[&r].posts.len(), 1, "no reply posted");
        assert!(core.ticket_due("runner").is_empty(), "nothing for the runner");
        assert!(core.ticket_given_since("w2").is_empty(), "the note is read");
    }

    // ---- a reply's `@` (paper-research N-581.p80, p82): a turn-end reply names whom its text names with `@`, beside
    // the authors it owes an answer, as a post made with `coord post` does

    /// The release check's replay catches a line this engine refuses (paper-research N-853, 3), the morning's case
    /// (N-807 p12): a `Titled` on the owner's thread by an agent the engine does not let title it, written by another
    /// engine, is "now refused", one problem line, and the thread is marked broken. The title is by w4 (neither opener,
    /// lead nor addressee), refused by every release; the addressee's own first title, the line of that morning, is
    /// taken on replay from core5-back 4439e015 on, by design. Nothing shown goes: the record with the line gives the
    /// same tickets, posts and labels as the record without it.
    #[test]
    fn the_replay_check_catches_a_title_this_engine_refuses() {
        let (mut core, coord) = reply_at_core("replay-titled");
        core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "w3", "typed": ["w3"], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("the circle at the top is wrong, it should drag".into())), names: vec!["w3".into()], told: vec![] }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        // this engine refuses the title live
        assert!(!core.ticket_title(&Who::Agent("w4".into()), &core.tickets.m.id(t), "the compaction circle drags").ok);
        let c = coord.display().to_string();
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let log = coord.join("tickets/log.jsonl");
        let views = |tk: &Tickets| -> Vec<serde_json::Value> {
            tk.m.tickets.keys().filter_map(|&n| tk.m.view(n)).map(|v| { let mut v = serde_json::to_value(v).unwrap(); v.as_object_mut().unwrap().remove("broken"); v }).collect()
        };
        let before = Tickets::load(&c, &base, None, 1);
        assert!(before.errors.is_empty(), "{:?}", before.errors);
        // the newer engine's line, as it wrote it
        let mut text = std::fs::read_to_string(&log).unwrap();
        text += &format!("{}\n", json!({"ev": "titled", "at": "2026-11-19T07:00:00Z", "t": t, "label": "the compaction circle drags", "by": "w4"}));
        std::fs::write(&log, text).unwrap();
        let after = Tickets::load(&c, &base, None, 1);
        let refused = after.errors.iter().filter(|e| e.contains("now refused")).count();
        assert_eq!((after.errors.len(), refused), (1, 1), "{:?}", after.errors);
        assert!(after.m.tickets[&t].broken.is_some(), "the owner's thread is marked broken");
        assert_eq!(views(&after), views(&before), "the same tickets, posts and labels");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A coord with agents w2, w3, w4 (claude) and runner (a program agent), loaded on agent-base; its folder.
    fn reply_at_core(tag: &str) -> (Core, std::path::PathBuf) {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-reply-at-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let reg = |id: &str, n: &str, kind: &str| format!("{{\"ev\":\"add\",\"id\":\"{id}\",\"name\":\"{n}\",\"parent\":\"owner\",\"kind\":\"{kind}\",\"model\":\"m\",\"at\":\"x\"}}\n");
        std::fs::write(coord.join("registry.jsonl"), reg("AGT-001", "w2", "claude") + &reg("AGT-002", "w3", "claude") + &reg("AGT-003", "w4", "claude") + &reg("AGT-004", "runner", "program")).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        (core, coord)
    }

    /// A turn of AGENT ending with TEXT, as its transcript has it, in COORD.
    fn reply_at_said(coord: &std::path::Path, agent: &str, text: &str) -> Loc {
        let f = coord.join(format!("{agent}-{}.jsonl", text.len()));
        let entry = format!("{}\n", json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": text}]}}));
        std::fs::write(&f, &entry).unwrap();
        Loc { file: f.display().to_string(), offset: 0, len: entry.len() as u64 }
    }

    /// AGENT is handed what it is owed on T, as a hand-over does.
    fn reply_at_take(core: &mut Core, agent: &str, t: u64) {
        let d = core.ticket_due(agent).into_iter().find(|d| d.t == t).unwrap_or_else(|| panic!("{agent} owed nothing on {t}"));
        reply_at_hand(core, agent, &d);
    }

    /// AGENT is handed delivery D, as a hand-over does.
    fn reply_at_hand(core: &mut Core, agent: &str, d: &docket_machine::Delivery) {
        core.tickets.handed.insert((agent.into(), d.id.clone()), (d.t, d.upto, Default::default()));
        core.ticket_handed(agent, std::slice::from_ref(&d.id), true);
    }

    /// A thread BY opens naming TO: its number.
    fn reply_at_open(core: &mut Core, by: &str, to: &str, text: &str) -> u64 {
        core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: by.into(), form: json!({"to": by, "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": by}, "follows": null}), text: Some(Text::New(text.into())), names: vec![to.into()], told: vec![] }).unwrap();
        *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0
    }

    /// Whether AGENT has a turn due on T (not only an inform).
    fn reply_at_woken(core: &Core, agent: &str, t: u64) -> bool {
        core.ticket_due(agent).iter().any(|d| d.t == t && !d.inform)
    }

    /// N-586 post 64: a reply that gives an order to a third agent by `@` wakes it, though the rule Reply owes it
    /// nothing (it never posted there, or was answered already); the author it owes is named first, as before. Itself
    /// and a name nobody has name nobody (a name without `@`: the next test).
    #[test]
    fn a_turn_end_reply_wakes_whom_it_names_with_at() {
        let (mut core, coord) = reply_at_core("names");
        let t = reply_at_open(&mut core, "w2", "w3", "What is 6 times 7?");
        reply_at_take(&mut core, "w3", t);
        assert!(!reply_at_woken(&core, "w4", t), "w4 is owed nothing yet");
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", "42. @w4, add it to the table; @w3, @owner and @nobody are no one to wake."));
        let p = core.tickets.m.tickets[&t].posts.last().unwrap().clone();
        assert_eq!((p.n, p.by.as_str(), p.auto, p.names.clone()), (2, "w3", true, vec!["w2".to_string(), "w4".to_string()]));
        assert!(reply_at_woken(&core, "w4", t), "the order reaches w4: {:?}", core.ticket_due("w4"));
        assert!(reply_at_woken(&core, "w2", t), "and the answer w2 was owed");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A reply owed to nobody who can read it (a program agent's note) was not posted at all, so an order in it went
    /// nowhere. Now it is posted when it names, by `@`, someone who can read it; without one it is still not posted.
    #[test]
    fn a_reply_owed_to_nobody_is_posted_when_it_names_someone_with_at() {
        let (mut core, coord) = reply_at_core("nobody");
        let r = reply_at_open(&mut core, "runner", "w3", "check t12: green");
        reply_at_take(&mut core, "w3", r);
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", "Green, so @w4 take t12 into the table."));
        let x = &core.tickets.m.tickets[&r];
        assert_eq!(x.posts.len(), 2, "the reply is posted");
        let p = x.posts.last().unwrap();
        assert_eq!((p.by.as_str(), p.auto, p.names.clone()), ("w3", true, vec!["w4".to_string()]));
        assert!(reply_at_woken(&core, "w4", r), "w4 is woken: {:?}", core.ticket_due("w4"));
        assert!(core.ticket_due("runner").is_empty(), "nothing for the runner");
        // without an @: as before, nothing posted and nobody woken
        let s = reply_at_open(&mut core, "runner", "w3", "check t13: green");
        reply_at_take(&mut core, "w3", s);
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", "Good, w4 will want this."));
        assert_eq!(core.tickets.m.tickets[&s].posts.len(), 1, "no reply posted");
        assert!(!reply_at_woken(&core, "w4", s));
        assert!(core.ticket_given_since("w3").is_empty(), "the note is read");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// The record replays to the same state: a reply's names come from its `replied` line, whatever its text names. A
    /// record written before the change (its reply named only whom it owed, though its text had an @) replays as it
    /// was: the @ adds nobody on replay.
    #[test]
    fn replies_replay_with_the_names_their_record_gives() {
        let (mut core, coord) = reply_at_core("replay");
        let t = reply_at_open(&mut core, "w2", "w3", "What is 6 times 7?");
        reply_at_take(&mut core, "w3", t);
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", "42. @w4, add it to the table."));
        let r = reply_at_open(&mut core, "runner", "w3", "check t12: green");
        reply_at_take(&mut core, "w3", r);
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", "Green, so @w4 take t12."));
        let state = |core: &Core| -> Vec<(u64, u32, String, bool, Vec<String>)> {
            [t, r].iter().flat_map(|u| core.tickets.m.tickets[u].posts.iter().map(move |p| (*u, p.n, p.by.clone(), p.auto, p.names.clone()))).collect()
        };
        let due = |core: &Core, a: &str| -> Vec<String> { core.ticket_due(a).into_iter().filter(|d| !d.inform).map(|d| d.id).collect() };
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let mut again = Core::load(&coord.display().to_string(), &base);
        again.load_tickets();
        assert_eq!(state(&again), state(&core), "the same posts and names after a restart");
        assert_eq!(due(&again, "w4"), due(&core, "w4"));
        assert_eq!(due(&core, "w4").len(), 2, "w4 owed both: {:?}", due(&core, "w4"));
        // the same record as written before the change: post 2 on T named w2 only
        let log = coord.join("tickets/log.jsonl");
        let old: String = std::fs::read_to_string(&log)
            .unwrap()
            .lines()
            .map(|l| {
                let mut v: Value = serde_json::from_str(l).unwrap();
                if v["ev"] == "replied" && v["t"] == json!(t) && v["n"] == json!(2) {
                    v["names"] = json!(["w2"]);
                }
                format!("{v}\n")
            })
            .collect();
        std::fs::write(&log, old).unwrap();
        let mut before = Core::load(&coord.display().to_string(), &base);
        before.load_tickets();
        let p = before.tickets.m.tickets[&t].posts.iter().find(|p| p.n == 2).unwrap().clone();
        assert_eq!((p.by.as_str(), p.names.clone()), ("w3", vec!["w2".to_string()]), "the record's names, not the text's");
        assert!(!reply_at_woken(&before, "w4", t), "nobody the old record did not wake");
        let _ = std::fs::remove_dir_all(&coord);
    }

    // ---- one reply, several answers (paper-research N-807, part 1): a turn given posts on several threads answers
    // each in a part of its last message

    /// The words of post P as posted (a part is posted as words, not by reference).
    fn parts_words(p: &docket_machine::line::Post) -> String {
        match &p.text {
            Text::New(s) => s.clone(),
            _ => String::new(),
        }
    }

    /// w3 is handed a question on each of two threads, from w2 and from w4, and answers both in one last message: a
    /// part by `#ID`, a part by `@NAME`, and words before any address, which go on the first thread handed. Each
    /// part stands on its thread under the post it answers and wakes its author.
    #[test]
    fn one_last_message_answers_several_threads_in_parts() {
        let (mut core, coord) = reply_at_core("parts");
        let a = reply_at_open(&mut core, "w2", "w3", "What is 6 times 7?");
        let b = reply_at_open(&mut core, "w4", "w3", "Is 43 in the table?");
        reply_at_take(&mut core, "w3", a);
        reply_at_take(&mut core, "w3", b);
        let (ia, ib) = (core.tickets.m.id(a), core.tickets.m.id(b));
        let text = format!("Both read.\n#{ib}: Not yet, I add it now.\n@w2 42, as you thought.");
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", &text));
        let pa = core.tickets.m.tickets[&a].posts.last().unwrap().clone();
        assert_eq!((pa.n, pa.by.as_str(), pa.answers, parts_words(&pa)), (2, "w3", Some(1), "Both read.\n\n@w2 42, as you thought.".to_string()));
        assert!(pa.names.contains(&"w2".to_string()), "{:?}", pa.names);
        let pb = core.tickets.m.tickets[&b].posts.last().unwrap().clone();
        assert_eq!((pb.n, pb.by.as_str(), pb.answers, parts_words(&pb)), (2, "w3", Some(1), "Not yet, I add it now.".to_string()));
        assert!(pb.names.contains(&"w4".to_string()), "{:?}", pb.names);
        assert!(reply_at_woken(&core, "w2", a) && reply_at_woken(&core, "w4", b), "each author is given its answer");
        assert!(!reply_at_woken(&core, "w4", a) && !reply_at_woken(&core, "w2", b), "and not the other's");
        assert!(core.ticket_given_since("w3").is_empty(), "the turn is read");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// `#2` is the second thread handed; an address inside a ``` fence is words; a part that says Nothing, or a
    /// whole message that does (or each of its parts by address), is posted nowhere. With one thread given, the rule Reply posts the message as before,
    /// address and all.
    #[test]
    fn parts_by_position_leave_fences_and_nothing_alone_and_one_thread_is_as_before() {
        let (mut core, coord) = reply_at_core("positions");
        let a = reply_at_open(&mut core, "w2", "w3", "What is 6 times 7?");
        let b = reply_at_open(&mut core, "w4", "w3", "Is 43 in the table?");
        reply_at_take(&mut core, "w3", a);
        reply_at_take(&mut core, "w3", b);
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", "#1 Nothing\n#2 Added, as:\n```\n#1 not an address\n```"));
        assert_eq!(core.tickets.m.tickets[&a].posts.len(), 1, "a part saying Nothing is no post");
        let pb = core.tickets.m.tickets[&b].posts.last().unwrap().clone();
        assert_eq!((pb.by.as_str(), parts_words(&pb)), ("w3", "Added, as:\n```\n#1 not an address\n```".to_string()));
        // the whole message Nothing: no post anywhere
        let c = reply_at_open(&mut core, "w2", "w3", "And 7 times 7?");
        let d = reply_at_open(&mut core, "w4", "w3", "And 49?");
        reply_at_take(&mut core, "w3", c);
        reply_at_take(&mut core, "w3", d);
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", "Nothing"));
        assert_eq!((core.tickets.m.tickets[&c].posts.len(), core.tickets.m.tickets[&d].posts.len()), (1, 1));
        // each thread answered Nothing by its address, by id, by a post's id, or by position: no post anywhere
        // (paper-research N-1397 p113)
        let f = reply_at_open(&mut core, "w2", "w3", "And 8 times 7?");
        let g = reply_at_open(&mut core, "w4", "w3", "And 56?");
        reply_at_take(&mut core, "w3", f);
        reply_at_take(&mut core, "w3", g);
        let (iff, ig) = (core.tickets.m.id(f), core.tickets.m.id(g));
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", &format!("#{iff} Nothing\n#{ig}.p1: nothing.")));
        assert_eq!((core.tickets.m.tickets[&f].posts.len(), core.tickets.m.tickets[&g].posts.len()), (1, 1));
        // one thread: the rule Reply's post, by reference, the address not taken out
        let e = reply_at_open(&mut core, "w2", "w3", "Done?");
        reply_at_take(&mut core, "w3", e);
        let ie = core.tickets.m.id(e);
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", &format!("#{ie} yes")));
        let pe = core.tickets.m.tickets[&e].posts.last().unwrap().clone();
        assert_eq!((pe.by.as_str(), pe.auto, pe.names.clone()), ("w3", true, vec!["w2".to_string()]));
        assert!(matches!(pe.text, Text::Loc { .. }), "posted as before: {:?}", pe.text);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A thread the agent answered with the reply tool during its turn takes no unaddressed words: they go on the
    /// next thread handed that it has not answered; with every thread answered, nowhere (no repeat of what it said).
    #[test]
    fn words_without_an_address_skip_a_thread_answered_by_hand() {
        let (mut core, coord) = reply_at_core("byhand");
        let a = reply_at_open(&mut core, "w2", "w3", "What is 6 times 7?");
        let b = reply_at_open(&mut core, "w4", "w3", "Is 43 in the table?");
        reply_at_take(&mut core, "w3", a);
        reply_at_take(&mut core, "w3", b);
        let ia = core.tickets.m.id(a);
        let r = core.ticket_post(&Who::Agent("w3".into()), &ia, "42.".into(), None, Some(1), Said::default(), None, false);
        assert!(r.ok, "{}", r.out);
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", "Added to the table."));
        assert_eq!(core.tickets.m.tickets[&a].posts.len(), 2, "only the answer by hand on the first");
        let pb = core.tickets.m.tickets[&b].posts.last().unwrap().clone();
        assert_eq!((pb.by.as_str(), parts_words(&pb)), ("w3", "Added to the table.".to_string()));
        // both answered by hand: the last message goes nowhere
        let c = reply_at_open(&mut core, "w2", "w3", "And 7 times 7?");
        let d = reply_at_open(&mut core, "w4", "w3", "And 49?");
        reply_at_take(&mut core, "w3", c);
        reply_at_take(&mut core, "w3", d);
        for (t, w) in [(c, "49."), (d, "Yes.")] {
            let id = core.tickets.m.id(t);
            assert!(core.ticket_post(&Who::Agent("w3".into()), &id, w.into(), None, Some(1), Said::default(), None, false).ok);
        }
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", "Both answered above."));
        assert_eq!((core.tickets.m.tickets[&c].posts.len(), core.tickets.m.tickets[&d].posts.len()), (2, 2));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// Its own post on a thread, about something else, made after the question there and before it was handed it,
    /// answers nothing there (proto-mind N-1446 p2631, docket-core p2632: an answer to N-1446 p2623 posted on N-1540):
    /// the words without an address go on that thread, the first handed, under the question, not on the next. The
    /// question is handed as an item of its own, up to it, as the delivery program handed p2623 (`upto` 2623, beside
    /// p2624).
    #[test]
    fn a_post_made_before_the_handing_leaves_the_words_on_its_thread() {
        let (mut core, coord) = reply_at_core("before");
        let a = reply_at_open(&mut core, "w2", "w3", "Your half of the list?");
        let ia = core.tickets.m.id(a);
        let r = core.ticket_post(&Who::Agent("w3".into()), &ia, "Fix 6's head is up.".into(), None, None, Said::default(), None, false);
        assert!(r.ok, "{}", r.out);
        let b = reply_at_open(&mut core, "w4", "w3", "Is 43 in the table?");
        let mut q = core.ticket_due("w3").into_iter().find(|d| d.t == a).expect("w3 owed the question");
        (q.id, q.upto) = (format!("{ia}.p1"), 1);
        reply_at_hand(&mut core, "w3", &q);
        reply_at_take(&mut core, "w3", b);
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", "My half is posted."));
        let pa = core.tickets.m.tickets[&a].posts.last().unwrap().clone();
        assert_eq!((pa.n, pa.by.as_str(), pa.answers, parts_words(&pa)), (3, "w3", Some(1), "My half is posted.".to_string()), "on the thread handed first");
        assert_eq!(core.tickets.m.tickets[&b].posts.len(), 1, "nothing on the next");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// The parser alone: the address forms, a name given on no thread, a header that is no address; a post's id on a
    /// thread given, a ticket here not given, and one not here (paper-research N-412.p2208).
    #[test]
    fn reply_parts_reads_the_addresses() {
        let th = |t: u64, id: &str, by: &str, told: bool, answered: bool| docket_machine::machine::ReplyThread { t, id: id.into(), last: 3, authors: vec![(by.into(), 3)], told, answered, all: None };
        let threads = vec![th(1, "N-1", "ann", false, false), th(2, "N-2", "bob", true, false)];
        let here = |id: &str| (id == "N-9").then(|| id.to_string());
        let on = |i: usize| To::Thread(i, 3);
        assert_eq!(reply_parts("hello", &threads, first_told(&threads), &here), vec![(on(1), "hello".to_string())], "words alone go on the first thread that told it something");
        assert_eq!(reply_parts("# Heading\n@carl hi\n#N-1 · ok", &threads, first_told(&threads), &here), vec![(on(1), "# Heading\n@carl hi".to_string()), (on(0), "ok".to_string())]);
        assert_eq!(reply_parts("#3 x\n#0 y", &threads, first_told(&threads), &here), vec![(on(1), "#3 x\n#0 y".to_string())], "a position past the threads is words");
        assert_eq!(reply_parts("@bob yes\n#N-2 and more", &threads, first_told(&threads), &here), vec![(on(1), "@bob yes\n\nand more".to_string())], "two parts for one thread are one post");
        assert_eq!(
            reply_parts("#N-1.p3 ok\n#N-9: elsewhere\n#N-77 not here\n#N-9 again", &threads, first_told(&threads), &here),
            vec![(on(0), "ok".to_string()), (To::Ticket("N-9".into(), None), "elsewhere\n#N-77 not here\n\nagain".to_string())],
            "a post's id is its thread; a ticket not given is that ticket, two parts one post; one not here is words"
        );
        assert_eq!(reply_parts("#N-9 only there", &[], None, &here), vec![(To::Ticket("N-9".into(), None), "only there".to_string())], "with no thread given");
        // a post's id on a ticket not given: the post it is under (consult-3, N-1397 p132)
        assert_eq!(reply_parts("#N-9.p2 under it", &threads, first_told(&threads), &here), vec![(To::Ticket("N-9".into(), Some(2)), "under it".to_string())]);
        // words without an address on the thread given as DEFAULT, or nowhere
        let alone = vec![th(1, "N-1", "ann", true, false)];
        assert_eq!(reply_parts("hi\n#N-9 x", &alone, Some(0), &here), vec![(on(0), "hi".to_string()), (To::Ticket("N-9".into(), None), "x".to_string())]);
        assert_eq!(reply_parts("hi\n#N-9 x", &alone, None, &here), vec![(To::Ticket("N-9".into(), None), "x".to_string())], "a told thread is no default by itself");
    }

    /// A turn handed only A ends with a part for B, a thread it was handed before (paper-research N-412.p2208): the
    /// part is posted on B as its `coord post` posts it, off the lock, under the post its address names, and the rest of
    /// the message on A, answering A's question, the rule Reply posting nothing more. Its own address alone, and a
    /// thread not here, change nothing.
    #[test]
    fn a_part_for_a_thread_not_handed_in_the_turn_is_posted_there() {
        use crate::core::Job;
        let (mut core, coord) = reply_at_core("elsewhere");
        let b = reply_at_open(&mut core, "w4", "w3", "Is 43 in the table?");
        reply_at_take(&mut core, "w3", b);
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", "Looking."));
        let on_b = core.tickets.m.tickets[&b].posts.len();
        let a = reply_at_open(&mut core, "w2", "w3", "What is 6 times 7?");
        reply_at_take(&mut core, "w3", a);
        let ib = core.tickets.m.id(b);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", &format!("42, as you thought.\n#{ib}.p1: 43 is not in it; the table stops at 42.")));
        let pa = core.tickets.m.tickets[&a].posts.last().unwrap().clone();
        assert_eq!((pa.by.as_str(), pa.answers, parts_words(&pa)), ("w3", Some(1), "42, as you thought.".to_string()), "the rest on A, not the whole message");
        assert_eq!(core.tickets.m.tickets[&a].posts.len(), 2, "one post on A");
        assert_eq!(core.tickets.m.tickets[&b].posts.len(), on_b, "B's part waits for its norms, off the lock");
        let post = std::iter::from_fn(|| rx.try_recv().ok()).find(|j| matches!(j, Job::Post { .. })).expect("B's part is sent to be posted");
        let Job::Post { agent, t, text, answers } = &post else { unreachable!() };
        assert_eq!((agent.as_str(), t.as_str(), text.as_str(), *answers), ("w3", ib.as_str(), "43 is not in it; the table stops at 42.", Some(1)));
        let shared = crate::Shared::new(core);
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(crate::run_job(&shared, post));
        let mut core = shared.0.try_lock().unwrap();
        let pb = core.tickets.m.tickets[&b].posts.last().unwrap().clone();
        assert_eq!((core.tickets.m.tickets[&b].posts.len(), pb.by.as_str(), parts_words(&pb), pb.answers), (on_b + 1, "w3", "43 is not in it; the table stops at 42.".to_string(), Some(1)), "posted on B, under its post 1");
        // a post's number B has not (a slip, or another thread's): the part goes on B all the same, under none, not
        // refused whole by `coord post` (consult-3, N-1397 p134)
        let d = reply_at_open(&mut core, "w2", "w3", "And 8 times 8?");
        reply_at_take(&mut core, "w3", d);
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", &format!("64.\n#{ib}.p99: nor is 64.")));
        let post = std::iter::from_fn(|| rx.try_recv().ok()).find(|j| matches!(j, Job::Post { .. })).expect("B's part is sent to be posted");
        assert!(matches!(&post, Job::Post { answers: None, .. }), "sent under no post");
        drop(core);
        rt.block_on(crate::run_job(&shared, post));
        let mut core = shared.0.try_lock().unwrap();
        let pb = core.tickets.m.tickets[&b].posts.last().unwrap().clone();
        assert_eq!((core.tickets.m.tickets[&b].posts.len(), pb.by.as_str(), parts_words(&pb), pb.answers), (on_b + 2, "w3", "nor is 64.".to_string(), None), "posted on B");
        // its own thread's address and a ticket not here: one thread, the rule Reply's post as before
        let c = reply_at_open(&mut core, "w2", "w3", "And 7 times 7?");
        reply_at_take(&mut core, "w3", c);
        let ic = core.tickets.m.id(c);
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", &format!("#{ic} 49.\n#N-999 none such")));
        let pc = core.tickets.m.tickets[&c].posts.last().unwrap().clone();
        assert!(matches!(pc.text, Text::Loc { .. }) && pc.auto, "the rule Reply's post: {:?}", pc.text);
        assert!(!std::iter::from_fn(|| rx.try_recv().ok()).any(|j| matches!(j, Job::Post { .. })), "nothing posted elsewhere");
        drop(core);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A turn given only a post that informs it on A ends with a part for B (consult-3, N-1397 p132): B's part shows
    /// on B alone, never again inside a post of the whole message on A. A part by `@NAME` goes on A; words without an
    /// address go on A where the rule Reply would have posted them (the message names someone), and nowhere where it
    /// would not.
    #[test]
    fn a_part_for_a_thread_not_handed_shows_once_after_a_turn_that_only_informs() {
        use crate::core::Job;
        let (mut core, coord) = reply_at_core("informs");
        let b = reply_at_open(&mut core, "w4", "w3", "Is 43 in the table?");
        reply_at_take(&mut core, "w3", b);
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", "Looking."));
        let ib = core.tickets.m.id(b);
        let a = reply_at_open(&mut core, "w2", "w4", "The table is due today.");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        let shared = crate::Shared::new(core);
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        // w2 informs w3 on A; w3's turn ends with TEXT; what A and B got
        let mut turn = |text: &str, fyi: &str| -> (Vec<String>, Vec<String>) {
            let (on_a, on_b) = {
                let mut core = shared.0.try_lock().unwrap();
                core.ticket_input(Input::Post { t: a, by: "w2".into(), text: Text::New(fyi.into()), names: vec![], told: vec![], inform: vec!["w3".into()], answers: None, kind: None }).unwrap();
                reply_at_take(&mut core, "w3", a);
                let lens = (core.tickets.m.tickets[&a].posts.len(), core.tickets.m.tickets[&b].posts.len());
                core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", text));
                lens
            };
            for job in std::iter::from_fn(|| rx.try_recv().ok()).filter(|j| matches!(j, Job::Post { .. })) {
                rt.block_on(crate::run_job(&shared, job));
            }
            let core = shared.0.try_lock().unwrap();
            let words = |t: u64, from: usize| core.tickets.m.tickets[&t].posts[from..].iter().map(|p| match &p.text { Text::New(w) => w.clone(), other => format!("{other:?}") }).collect::<Vec<_>>();
            (words(a, on_a), words(b, on_b))
        };
        // first, before w3 has posted on A: words nobody would read go nowhere
        assert_eq!(turn(&format!("Noted.\n#{ib}: the part"), "FYI: the table is done."), (vec![], vec!["the part".to_string()]), "nobody to read them: nowhere");
        assert_eq!(turn(&format!("@w2 see below\n#{ib}: more"), "FYI: and checked."), (vec!["@w2 see below".to_string()], vec!["more".to_string()]), "the `@w2` part on A, B's on B, nothing twice");
        assert_eq!(turn(&format!("Noted, @w2: see below.\n#{ib}: still more"), "FYI: and filed."), (vec!["Noted, @w2: see below.".to_string()], vec!["still more".to_string()]), "named: the words where the rule Reply would have put them");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// w3 is told a post of w2's on C and its turn ends; w2 then takes that post back, so w3 is owed the notice
    /// (`N-x.pN.taken_back`), which shows it nothing. C's number and w2's post's.
    fn reply_at_struck(core: &mut Core, coord: &std::path::Path) -> (u64, u32) {
        let c = reply_at_open(core, "w2", "w3", "The table.");
        reply_at_take(core, "w3", c);
        core.ticket_turn_ended("w3", reply_at_said(coord, "w3", "Seen."));
        core.ticket_input(Input::Post { t: c, by: "w2".into(), text: Text::New("Use 43 too.".into()), names: vec!["w3".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let n = core.tickets.m.tickets[&c].posts.last().unwrap().n;
        reply_at_take(core, "w3", c);
        core.ticket_turn_ended("w3", reply_at_said(coord, "w3", "OK."));
        let ic = core.tickets.m.id(c);
        let r = core.ticket_take_back(&Who::Agent("w2".into()), &ic, n);
        assert!(r.ok, "{}", r.out);
        assert!(core.ticket_due("w3").iter().any(|d| d.t == c && d.id.ends_with(".taken_back")), "w3 is owed the notice");
        (c, n)
    }

    /// A turn handed A's question and the notice that a post of C it had was taken back ends with a part for B
    /// (consult-3, N-1397 p134, hold 2): the notice is read on its ticket and makes no thread of its own (proto-lead
    /// N-1397 p143), so the turn is parted as one handed A alone: B's part is posted on B, under its post 1, and the
    /// rest on A, under A's question, the rule Reply posting nothing more. The part shows once, on B.
    #[test]
    fn a_part_for_a_thread_not_handed_shows_once_beside_a_post_taken_back() {
        use crate::core::Job;
        let (mut core, coord) = reply_at_core("struck");
        let b = reply_at_open(&mut core, "w4", "w3", "Is 43 in the table?");
        reply_at_take(&mut core, "w3", b);
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", "Looking."));
        let (c, _) = reply_at_struck(&mut core, &coord);
        let a = reply_at_open(&mut core, "w2", "w3", "What is 6 times 7?");
        reply_at_take(&mut core, "w3", a);
        reply_at_take(&mut core, "w3", c);
        assert_eq!(core.tickets.m.reply_names("w3", &[]).map(|(t, _)| t), Some(a), "the rule Reply's thread is A");
        let (ib, on_b, on_c) = (core.tickets.m.id(b), core.tickets.m.tickets[&b].posts.len(), core.tickets.m.tickets[&c].posts.len());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", &format!("42.\n#{ib}.p1: 43 is not in it.")));
        let pa = core.tickets.m.tickets[&a].posts.last().unwrap().clone();
        assert_eq!((pa.by.as_str(), pa.answers, parts_words(&pa)), ("w3", Some(1), "42.".to_string()), "the rest on A, not the whole message: {:?}", pa.text);
        assert_eq!(core.tickets.m.tickets[&a].posts.len(), 2, "one post on A");
        assert_eq!(core.tickets.m.tickets[&c].posts.len(), on_c, "nothing on C");
        let posts: Vec<Job> = std::iter::from_fn(|| rx.try_recv().ok()).filter(|j| matches!(j, Job::Post { .. })).collect();
        assert_eq!(posts.len(), 1, "B's part alone is sent to be posted");
        let post = posts.into_iter().next().unwrap();
        let Job::Post { agent, t, text, answers } = &post else { unreachable!() };
        assert_eq!((agent.as_str(), t.as_str(), text.as_str(), *answers), ("w3", ib.as_str(), "43 is not in it.", Some(1)));
        let shared = crate::Shared::new(core);
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(crate::run_job(&shared, post));
        let core = shared.0.try_lock().unwrap();
        let pb = core.tickets.m.tickets[&b].posts.last().unwrap().clone();
        assert_eq!((core.tickets.m.tickets[&b].posts.len(), pb.by.as_str(), parts_words(&pb), pb.answers), (on_b + 1, "w3", "43 is not in it.".to_string(), Some(1)), "posted on B, under its post 1");
        drop(core);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A turn on two threads, A from w2 and D from w4, that also holds the notice that a post of C it had was taken
    /// back, handed first (proto-lead N-1397 p143): the notice makes no thread, so the turn is several as without it.
    /// Its words without an address go on A, the first thread that told it something, and `#2` is D, each posted
    /// there under the question it answers; nothing goes elsewhere. On 4948dd0a the notice made no thread be read
    /// at all: the words went nowhere, and `#2`, a position past no threads, with them.
    #[test]
    fn a_turn_on_two_threads_beside_a_post_taken_back_is_parted() {
        use crate::core::Job;
        let (mut core, coord) = reply_at_core("struck-two");
        let (c, _) = reply_at_struck(&mut core, &coord);
        let a = reply_at_open(&mut core, "w2", "w3", "What is 6 times 7?");
        let d = reply_at_open(&mut core, "w4", "w3", "And 7 times 7?");
        reply_at_take(&mut core, "w3", c);
        reply_at_take(&mut core, "w3", a);
        reply_at_take(&mut core, "w3", d);
        let on_c = core.tickets.m.tickets[&c].posts.len();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", "42.\n#2 49."));
        let last = |t: u64| core.tickets.m.tickets[&t].posts.last().map(|p| (p.by.clone(), p.answers, parts_words(p))).unwrap();
        assert_eq!(last(a), ("w3".to_string(), Some(1), "42.".to_string()), "the words without an address on A");
        assert_eq!(last(d), ("w3".to_string(), Some(1), "49.".to_string()), "`#2` on D");
        assert_eq!(core.tickets.m.tickets[&c].posts.len(), on_c, "nothing on C");
        assert!(!std::iter::from_fn(|| rx.try_recv().ok()).any(|j| matches!(j, Job::Post { .. })), "nothing sent elsewhere");
        assert!(reply_at_woken(&core, "w2", a) && reply_at_woken(&core, "w4", d), "each author is given its answer");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A turn that holds his post told late (`N-T.pA.told`, (c2)) and a take-back's notice together, beside w2's
    /// question on A (proto-lead N-1397 p146): the told delivery is its post on T (`delivered_post`), the notice is read
    /// on C and makes no thread (`post_of`), so the turn is several, A and T. Its words without an address go on A, the
    /// first thread that told it something, and its part by T's id on T, under his post told late; nothing on C, nothing
    /// sent elsewhere. On 367fbb84 the notice made no thread be read: the words went nowhere, and T's part went by
    /// `coord post`, as to a thread not handed.
    #[test]
    fn a_turn_holding_his_post_told_late_and_a_take_backs_notice_is_parted() {
        use crate::core::Job;
        let (mut core, coord) = reply_at_core("struck-told");
        let (c, _) = reply_at_struck(&mut core, &coord);
        let a = reply_at_open(&mut core, "w2", "w3", "What is 6 times 7?");
        // his thread T to w3, w4 brought in by w3's @; his post typed to nobody, asked about, then sent to w3
        let v = json!({"from": "owner", "to": "w3", "typed": ["w3"], "names": [], "text": "the dashboard", "addressed": {"head": "Unnamed"}, "esc": []});
        core.ticket_input(Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        core.ticket_answer(docket_machine::WATCHER, t, json!({"head": "Thread"}), Text::New(String::new())).unwrap();
        core.ticket_input(Input::Post { t, by: "w3".into(), text: Text::New("Taken. @w4 the ledger is yours".into()), names: vec!["w4".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("the median on it looks off".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let n = core.tickets.m.tickets[&t].last();
        let route = core.ticket_due(docket_machine::WATCHER).into_iter().filter(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "route")).last().expect("a route step for his post");
        core.ticket_answer(docket_machine::WATCHER, route.t, json!({"head": "Unclear", "why": "which work is the median?"}), Text::New(String::new())).unwrap();
        core.ticket_answer("owner", route.t, json!({"head": "Send", "to": ["w3"]}), Text::New(String::new())).unwrap();
        let told = format!("{}.p{n}.{}", core.tickets.m.id(t), docket_machine::TOLD);
        // w3 is handed A, then all it is owed on T (his post told late among it), then C's notice
        reply_at_take(&mut core, "w3", a);
        let on_t: Vec<docket_machine::Delivery> = core.ticket_due("w3").into_iter().filter(|d| d.t == t).collect();
        assert!(on_t.iter().any(|d| d.id == told), "his post told late to w3: {:?}", on_t.iter().map(|d| &d.id).collect::<Vec<_>>());
        for d in &on_t {
            reply_at_hand(&mut core, "w3", d);
        }
        reply_at_take(&mut core, "w3", c);
        let (it, on_c) = (core.tickets.m.id(t), core.tickets.m.tickets[&c].posts.len());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        core.ticket_turn_ended("w3", reply_at_said(&coord, "w3", &format!("42.\n#{it}: the one on the release page; I take it.")));
        let last = |t: u64| core.tickets.m.tickets[&t].posts.last().map(|p| (p.by.clone(), p.answers, parts_words(p))).unwrap();
        assert_eq!(last(a), ("w3".to_string(), Some(1), "42.".to_string()), "the words without an address on A");
        assert_eq!(last(t), ("w3".to_string(), Some(n), "the one on the release page; I take it.".to_string()), "T's part on T, under his post told late");
        assert_eq!(core.tickets.m.tickets[&c].posts.len(), on_c, "nothing on C");
        assert!(!std::iter::from_fn(|| rx.try_recv().ok()).any(|j| matches!(j, Job::Post { .. })), "nothing sent elsewhere");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// The agent a thread of the owner's was written to gives it its first title (N-807.p4), live (N-985), and the line
    /// replays; another agent gives none, a second title is refused, and the owner may (docket-core3, N-807.p12).
    #[test]
    fn the_agent_the_owner_wrote_to_titles_his_thread_once() {
        let (mut core, coord) = reply_at_core("title");
        core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "w3", "typed": ["w3"], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("the circle at the top is wrong, it should drag".into())), names: vec!["w3".into()], told: vec![] }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        let id = core.tickets.m.id(t);
        assert!(!core.ticket_title(&Who::Agent("w4".into()), &id, "not mine").ok, "w4 was not written to");
        // w3, no lead, is shown how to title it; w4 is not
        let lead = core.tickets.m.roles_of("w3").iter().any(|r| r == "lead");
        let (hint, other) = (core.ticket_untitled_for("w3", t), core.ticket_untitled_for("w4", t));
        assert!(!lead && hint && !other, "the hint: w3 {hint}, w4 {other}, w3 a lead {lead}");
        let d = core.ticket_due("w3").into_iter().find(|d| d.t == t).expect("his thread due to w3");
        let view = core.tickets.m.view(t).unwrap();
        let text = crate::render::delivery_as(&core.places(), &d, &view, false, true, false, false, None, false, crate::render::Hints { no_parts: false, untitled: hint, batch: false }).unwrap().0;
        assert!(text.contains(&format!("coord title {id}")), "{text}");
        assert!(core.ticket_title(&Who::Agent("w3".into()), &id, "the compaction circle drags").ok, "live in this release");
        assert!(!core.ticket_untitled_for("w3", t), "titled: no hint any more");
        core.load_tickets();
        let errors = core.tickets.errors.clone();
        let titled = core.tickets.m.tickets[&t].titled.clone();
        let again = core.ticket_title(&Who::Agent("w3".into()), &id, "again").ok;
        let his = core.ticket_title(&Who::Owner, &id, "his own words").ok;
        let _ = std::fs::remove_dir_all(&coord);
        assert!(errors.is_empty(), "the addressee's title replays: {errors:?}");
        assert_eq!(titled.as_deref(), Some("the compaction circle drags"));
        assert!(!again, "the first title only");
        assert!(his);
    }

    /// plan/layers.md I §3: a `when D is o` binding's decision, asked as coordd's clock asks it: the record's factors
    /// as the call, a rule of the project's policy answering. Wait while fewer than two bits closed since the last
    /// batch, build after; a batch open, wait. The decision needs the proxy's models (a stand-in that serves none).
    #[test]
    fn a_when_binding_is_decided_by_the_policy_from_the_records_factors() {
        use std::io::{Read, Write};
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let dir = std::env::temp_dir().join(format!("docket-when-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("policy")).unwrap();
        std::fs::write(
            dir.join("policy/cut.eflint"),
            "Fact cut-option Identified by String\n+cut-option(\"build\").\n+cut-option(\"wait\").\nDecision cut-build\n  Options cut-option\nPrefer for cut-build:\n  option == \"wait\" > option == \"build\"\nOnly for cut-build When call.closed-bit >= 2:\n  option == \"build\"\nOnly for cut-build When call.open >= 1:\n  option == \"wait\"\nDecision cut-tie\n  Options cut-option\n",
        )
        .unwrap();
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        std::thread::spawn(move || {
            for mut s in l.incoming().flatten() {
                let mut buf = [0u8; 65536];
                let _ = s.read(&mut buf);
                let body = r#"{"data": []}"#;
                let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
            }
        });
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = Some(url));
        let places = crate::policy::Places { base, project: Some(dir.display().to_string()), coord: dir.display().to_string(), ..Default::default() };
        let engine = crate::policy::Engine::start(places);
        let ask = |f: &[(&str, &str)]| engine.decide("cut-build", f.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()).unwrap();
        let r = ask(&[("open", "0"), ("closed-bit", "1")]);
        assert_eq!(r.chosen.as_deref(), Some("wait"), "{r:?}");
        let r = ask(&[("open", "0"), ("closed-bit", "2"), ("oldest-bit", "30")]);
        assert_eq!(r.chosen.as_deref(), Some("build"), "{r:?}");
        let r = ask(&[("open", "1"), ("closed-bit", "3")]);
        assert_eq!(r.chosen, None, "both rules: no option is left ({r:?})");
        // a decision the rules leave open (no preference between the two): nothing fires, though the solver's record
        // names the first by name (paper-research N-89.p2, 4)
        let r = engine.decide("cut-tie", BTreeMap::new()).unwrap();
        assert_eq!(r.optimal, vec!["build".to_string(), "wait".to_string()], "{r:?}");
        let (fire, open) = super::when_split(vec![("b".into(), "build".into(), r)]);
        assert!(fire.is_empty() && open.len() == 1);
        let r = ask(&[("open", "0"), ("closed-bit", "2")]);
        let (fire, open) = super::when_split(vec![("b".into(), "build".into(), r.clone()), ("c".into(), "wait".into(), r)]);
        assert!(fire.len() == 1 && fire[0].0 == "b" && open.is_empty(), "{fire:?}");
        // a policy without the decision answers nothing, and nothing fires
        let r = engine.decide("cut-nothing", BTreeMap::new()).unwrap();
        assert!(r.chosen.is_none() && r.fault.is_some(), "{r:?}");
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Design part 9b, with the stand-in proxy (it counts what reaches it and refuses all): a thread between agents
    /// through its open, a reply, a post and a Nothing asks no model anything; nor does a post told to the owner there,
    /// since what an agent asks him is typed and no call reads it (paper-research N-1446 p1081). Twice: on today's
    /// kinds, whose thread.kind has no `agent_post` handler and so makes no step; and on a thread opened on 64's
    /// thread.kind, whose `agent_post` handler a thread keeps from its open (proto-engine N-1479 p273): its asks step is
    /// answered Nothing, with no call and no mark (proto-lead p274). And that thread as one opened before 16 Nov
    /// 20:05, whose lines record no calls, read again by coordd on today's agent-base alone: its recorded program checks
    /// against today's calls, asks.call among them, and its asks step is answered so (paper-research N-1479 p462,
    /// consult-3 p465, proto-lead p466).
    #[test]
    fn an_agents_thread_makes_no_model_call_the_owner_told_or_not() {
        for (at_64, again) in [(true, false), (true, true), (false, false)] {
            agents_thread_makes_no_model_call(at_64, again);
        }
    }

    /// `an_agents_thread_makes_no_model_call_the_owner_told_or_not`, AT_64: the project's thread.kind and asks.call
    /// are 64's (docket-machine testdata), and a project's kind of the same name wins. AGAIN: before the owner is told a
    /// post there, the record loses its `calls` as a record from before 16 Nov 20:05 has none (docket-machine
    /// tests.rs `without_calls`), 64's files go, and coordd starts again on it with today's agent-base alone.
    fn agents_thread_makes_no_model_call(at_64: bool, again: bool) {
        use std::io::{Read, Write};
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-between-{at_64}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        if at_64 {
            let project = coord.join("files");
            let testdata = concat!(env!("CARGO_MANIFEST_DIR"), "/../docket/machine/testdata");
            for (d, from, to) in [("kinds", "thread-at-64.kind", "thread.kind"), ("calls", "asks-at-64.call", "asks.call")] {
                std::fs::create_dir_all(project.join(d)).unwrap();
                std::fs::copy(format!("{testdata}/{from}"), project.join(d).join(to)).unwrap();
            }
            std::fs::write(project.join("project.toml"), "[project]\nname = \"between\"\n").unwrap();
            std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
        }
        let reg = |id: &str, n: &str| format!("{{\"ev\":\"add\",\"id\":\"{id}\",\"name\":\"{n}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}}\n");
        std::fs::write(coord.join("registry.jsonl"), reg("AGT-001", "pipe") + &reg("AGT-002", "cone")).unwrap();
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        let h = hits.clone();
        std::thread::spawn(move || {
            for mut s in l.incoming().flatten() {
                h.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let mut buf = [0u8; 65536];
                let _ = s.read(&mut buf);
                let _ = s.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 7\r\nConnection: close\r\n\r\nrefused");
            }
        });
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = Some(url));
        let said = |agent: &str, n: usize, text: &str| {
            let f = coord.join(format!("{agent}-{n}.jsonl"));
            let entry = format!("{}\n", json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": text}]}}));
            std::fs::write(&f, &entry).unwrap();
            Loc { file: f.display().to_string(), offset: 0, len: entry.len() as u64 }
        };
        let take = |core: &mut Core, agent: &str, t: u64| {
            let d = core.ticket_due(agent).into_iter().find(|d| d.t == t).unwrap_or_else(|| panic!("{agent} owed nothing on {t}"));
            core.tickets.handed.insert((agent.into(), d.id.clone()), (d.t, d.upto, Default::default()));
            core.ticket_handed(agent, std::slice::from_ref(&d.id), true);
        };
        // what the watcher is owed, called as coordd calls it
        let calls = |core: &mut Core| {
            let due = core.ticket_due(docket_machine::WATCHER);
            for d in &due {
                if let Some(job) = core.clerk_job(d) {
                    let _ = crate::clerk::answer(&job);
                }
            }
            due.len()
        };
        let mut core = Core::load(&coord.display().to_string(), &base);
        if at_64 {
            core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        }
        core.load_tickets();
        assert!(core.tickets.errors.is_empty(), "{:?}", core.tickets.errors);
        let msg = |core: &mut Core, who: &str, to: &[&str], text: &str, re: Option<&str>| {
            let r = core.ticket_msg(&Who::Agent(who.into()), None, to.iter().map(|s| s.to_string()).collect(), text, false, re.map(String::from), Vec::new(), Said::default());
            assert!(r.ok, "{}", r.out);
        };
        msg(&mut core, "pipe", &["cone"], "add median(xs) to calc.py", None);
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        assert!(core.tickets.m.tickets[&t].between());
        assert_eq!(calls(&mut core), 0);
        take(&mut core, "cone", t);
        core.ticket_turn_ended("cone", said("cone", 1, "Done: median(xs) with a test. Population or sample?"));
        assert_eq!(core.tickets.m.tickets[&t].posts.len(), 2, "rule Reply on the agents' thread");
        assert_eq!(calls(&mut core), 0);
        let id = core.tickets.m.id(t);
        msg(&mut core, "pipe", &["cone"], "Sample.", Some(&id));
        assert_eq!(calls(&mut core), 0);
        take(&mut core, "cone", t);
        core.ticket_turn_ended("cone", said("cone", 2, "Nothing"));
        assert_eq!(core.tickets.m.tickets[&t].posts.len(), 3, "Nothing posts nothing");
        assert_eq!(calls(&mut core), 0);
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 0, "no model was asked anything");
        if again {
            drop(core);
            let log = coord.join("tickets/log.jsonl");
            let old: String = std::fs::read_to_string(&log).unwrap().lines().map(|l| {
                let mut v: Value = serde_json::from_str(l).unwrap();
                let Some(o) = v.as_object_mut() else { return format!("{l}\n") };
                let had = o.remove("calls").is_some() | o.get_mut("fired").and_then(Value::as_array_mut).into_iter().flatten().filter_map(Value::as_object_mut).fold(false, |had, f| f.remove("calls").is_some() | had);
                if !had {
                    return format!("{l}\n");
                }
                let line: docket_machine::Line = serde_json::from_value(v).unwrap();
                format!("{}\n", serde_json::to_string(&line).unwrap())
            }).collect();
            assert!(old.contains("when agent_post") && !old.contains("\"calls\""), "64's handler on the record, and no calls");
            std::fs::write(&log, old).unwrap();
            std::fs::remove_file(coord.join("project")).unwrap();
            core = Core::load(&coord.display().to_string(), &base);
            core.load_tickets();
            let x = &core.tickets.m.tickets[&t];
            assert!(!x.unchecked && x.broken.is_none(), "its recorded program checks against today's calls: {:?}", x.broken);
        }
        // the owner told a post there: a kind's asks step on it, where one still makes it, is answered with no call
        msg(&mut core, "pipe", &["owner"], "Should median live in calc.py or stats.py?", Some(&id));
        let owed = calls(&mut core);
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 0, "no model was asked anything");
        assert!(core.ticket_due(docket_machine::WATCHER).is_empty(), "the watcher owes nothing");
        if at_64 {
            // 64's handler made the step, and it went as Nothing: no model chose (no `decided` line), no mark
            assert_eq!(owed, 1, "64's thread.kind makes the asks step");
            let lines: Vec<Value> = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
            let step = lines.iter().find(|v| v["ev"] == "step" && v["call"] == "asks").expect("the asks step on the record")["t"].clone();
            let of: Vec<(&Value, &Value)> = lines.iter().filter(|v| v["t"] == step && (v["ev"] == "answer" || v["ev"] == "decided")).map(|v| (&v["ev"], &v["head"])).collect();
            assert_eq!(of, [(&json!("answer"), &json!({"head": "Nothing"}))], "answered Nothing, with no model chosen");
            assert!(core.tickets.m.view(t).unwrap().posts.iter().all(|p| p.asks.is_empty() && !p.asks_open), "no mark");
        } else {
            // today's thread.kind has no `agent_post` handler, so no step is made at all (proto-reader N-1479 p306)
            assert_eq!(owed, 0, "today's thread.kind makes no asks step");
        }
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-233's shape (design part 9b, N-27 point 3c and 3g): one agent asks five, two answer by their turns' replies
    /// and a third by hand, all naming nobody. Each answer is told to the asker alone, the other agents are informed by
    /// policy, and only the asker is woken; an informed agent handed its inform with the next told post is not
    /// "several" there.
    #[test]
    fn one_agent_to_five_two_answer_unnamed_and_only_the_asker_is_woken() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-n233-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let agents = ["orch", "a1", "a2", "a3", "a4", "a5"];
        let reg: String = agents.iter().enumerate().map(|(i, n)| format!("{{\"ev\":\"add\",\"id\":\"AGT-00{i}\",\"name\":\"{n}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}}\n")).collect();
        std::fs::write(coord.join("registry.jsonl"), reg).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let engine = crate::policy::Engine::start(core.places());
        // policy's notify for post N of T, run as coordd's task runner runs it
        let notify = |core: &mut Core, t: u64, n: u32, actor: &str| {
            let (facts, ticket) = core.ticket_facts(Some(t));
            for input in run_task(&engine, Task::Notify { t, event: "post".into(), actor: actor.into(), post: Some(n), ticket: ticket.unwrap(), facts, places: core.places() }) {
                core.ticket_input(input).unwrap();
            }
        };
        let said = |agent: &str, text: &str| {
            let f = coord.join(format!("{agent}.jsonl"));
            let entry = format!("{}\n", json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": text}]}}));
            std::fs::write(&f, &entry).unwrap();
            Loc { file: f.display().to_string(), offset: 0, len: entry.len() as u64 }
        };
        let take_all = |core: &mut Core, agent: &str| {
            for d in core.ticket_due(agent) {
                core.tickets.handed.insert((agent.into(), d.id.clone()), (d.t, d.upto, Default::default()));
                core.ticket_handed(agent, std::slice::from_ref(&d.id), true);
            }
        };
        let r = core.ticket_msg(&Who::Agent("orch".into()), None, ["a1", "a2", "a3", "a4", "a5"].iter().map(|s| s.to_string()).collect(), "Who has the build log?", false, None, Vec::new(), Said::default());
        assert!(r.ok, "{}", r.out);
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        notify(&mut core, t, 1, "orch");
        // a4 and a5 read the question and have nothing to say
        for a in ["a4", "a5"] {
            take_all(&mut core, a);
            core.ticket_turn_ended(a, said(a, "Nothing"));
        }
        // a1 and a2 answer by their turns' last messages; a2 is handed a1's answer (an inform) with nothing else new
        take_all(&mut core, "a1");
        core.ticket_turn_ended("a1", said("a1", "I have it: /tmp/b.log"));
        notify(&mut core, t, 2, "a1");
        take_all(&mut core, "a2");
        core.ticket_turn_ended("a2", said("a2", "Not me."));
        notify(&mut core, t, 3, "a2");
        // a3 answers by hand, naming nobody
        take_all(&mut core, "a3");
        let id = core.tickets.m.id(t);
        let r = core.ticket_post(&Who::Agent("a3".into()), &id, "Same as a1.".into(), None, None, Said::default(), None, false);
        assert!(r.ok, "{}", r.out);
        notify(&mut core, t, 4, "a3");
        let x = &core.tickets.m.tickets[&t];
        let told: Vec<(String, Vec<String>)> = x.posts.iter().skip(1).map(|p| (p.by.clone(), p.names.clone())).collect();
        assert_eq!(told, vec![("a1".to_string(), vec!["orch".to_string()]), ("a2".to_string(), vec!["orch".to_string()]), ("a3".to_string(), vec!["orch".to_string()])], "each answer is told to the asker alone");
        let informed: Vec<usize> = x.posts.iter().skip(1).map(|p| p.inform.len()).collect();
        assert_eq!(informed, vec![4, 4, 4], "the other four agents on it are informed of each answer: {:?}", x.posts.iter().map(|p| &p.inform).collect::<Vec<_>>());
        let woken: Vec<&str> = agents.iter().copied().filter(|a| core.ticket_due(a).iter().any(|d| !d.inform)).collect();
        assert_eq!(woken, vec!["orch"], "only the asker is woken");
        // nobody is both told and informed of one post, in the log either (N-27 post 27, 2)
        let log = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap();
        let notified: Vec<Value> = log.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|l| l["ev"] == "notified" && l["t"] == json!(t)).collect();
        assert_eq!(notified.len(), 3, "the answers' three: {notified:?}");
        for l in &notified {
            let p = &x.posts[l["post"].as_u64().unwrap() as usize - 1];
            assert!(l["inform"].as_array().unwrap().iter().all(|a| *a != json!(p.by) && !p.names.iter().any(|n| *a == json!(n))), "{l}");
        }
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-17 post 10: the session looks idle once its last message is on screen, so the next hand-over can be taken
    /// before the follower reads the turn's end. What is in the transcript after the message, or taken in a later
    /// second than the message's timestamp and not seen yet, is the next turn's.
    #[test]
    fn a_turn_end_does_not_answer_what_was_handed_after_its_message() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-stale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let note = |core: &mut Core, text: &str| {
            core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New(text.into())), names: vec!["w2".into()], told: vec![] }).unwrap();
            let d = core.ticket_due("w2").into_iter().next().unwrap();
            core.tickets.handed.insert(("w2".into(), d.id.clone()), (d.t, d.upto, Default::default()));
            core.ticket_handed("w2", std::slice::from_ref(&d.id), true);
            d
        };
        // the transcript: the first note, the turn's message (written long ago), the second note
        let f = coord.join("w2.jsonl");
        let user = |id: &str| format!("{}\n", json!({"type": "user", "message": {"role": "user", "content": format!("<channel id=\"{id}\">...</channel>")}}));
        let a = note(&mut core, "is the build green?");
        let (ua, msg) = (user(&a.id), format!("{}\n", json!({"type": "assistant", "timestamp": "2026-01-01T00:00:00.000Z", "message": {"role": "assistant", "content": [{"type": "text", "text": "Green."}]}})));
        let b = note(&mut core, "and C-1?");
        let ub = user(&b.id);
        std::fs::write(&f, format!("{ua}{msg}{ub}")).unwrap();
        let file = f.display().to_string();
        core.ticket_seen("w2", &a.id, Loc { file: file.clone(), offset: 0, len: ua.len() as u64 });
        core.ticket_seen("w2", &b.id, Loc { file: file.clone(), offset: (ua.len() + msg.len()) as u64, len: ub.len() as u64 });
        // the third is taken, not in the transcript yet, and taken after the message's timestamp
        let c = note(&mut core, "and C-2?");
        let loc = Loc { file: file.clone(), offset: ua.len() as u64, len: msg.len() as u64 };
        core.ticket_turn_ended("w2", loc);
        assert_eq!(core.tickets.m.tickets[&a.t].posts.len(), 2, "the message replies to the note it read");
        assert_eq!(core.tickets.m.tickets[&b.t].posts.len(), 1);
        assert_eq!(core.tickets.m.tickets[&c.t].posts.len(), 1);
        let mut left = core.ticket_given_since("w2");
        left.sort();
        assert_eq!(left, vec![b.id.clone(), c.id.clone()], "the next turn's");
        let _ = std::fs::remove_dir_all(&coord);
    }

    #[test]
    fn a_turn_ended_after_coordd_restarted_still_clears_what_it_was_given() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-restarted-{}", std::process::id()));
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let r = core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("what is 6 times 7?".into())), names: vec!["w2".into()], told: vec![] });
        assert!(r.is_ok(), "{r:?}");
        let d = core.ticket_due("w2").into_iter().next().expect("the note is due");
        core.tickets.handed.insert(("w2".into(), d.id.clone()), (d.t, d.upto, Default::default()));
        core.ticket_handed("w2", std::slice::from_ref(&d.id), true);
        // coordd restarts mid-turn, the agent's process still running (`recover` finds it): the new one loads the log,
        // and the count with it
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.agents.get_mut("w2").unwrap().live.pid = Some(1);
        core.load_tickets();
        assert_eq!(core.ticket_given_since("w2"), vec![d.id.clone()]);
        let loc = Loc { file: "/nonexistent/w2.jsonl".into(), offset: 10, len: 5 };
        core.ticket_turn_ended("w2", loc);
        let posts = core.tickets.m.tickets[&d.t].posts.len();
        let _ = std::fs::remove_dir_all(&coord);
        assert!(core.ticket_given_since("w2").is_empty(), "the end was kept, so the hand-over is free again");
        assert_eq!(posts, 2, "and the note was replied");
    }

    #[test]
    fn an_agent_that_stopped_while_coordd_was_down_is_cut_at_load() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-stopped-{}", std::process::id()));
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let r = core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("what is 6 times 7?".into())), names: vec!["w2".into()], told: vec![] });
        assert!(r.is_ok(), "{r:?}");
        let d = core.ticket_due("w2").into_iter().next().expect("the note is due");
        core.tickets.handed.insert(("w2".into(), d.id.clone()), (d.t, d.upto, Default::default()));
        core.ticket_handed("w2", std::slice::from_ref(&d.id), true);
        // coordd restarts; w2's process is gone (recover found nothing): what it was given and never put into its
        // model's input is due again, its process's end (0.2 A4), so its turn holds nothing more to cut
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let cut = core.ticket_given_since("w2").is_empty();
        let again = core.ticket_due("w2").iter().any(|x| x.id == d.id);
        let log = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&coord);
        assert!(cut && again, "{log}");
    }

    #[test]
    fn a_cut_turn_does_not_reply_in_the_next() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-cut-{}", std::process::id()));
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let r = core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("what is 6 times 7?".into())), names: vec!["w2".into()], told: vec![] });
        assert!(r.is_ok(), "{r:?}");
        let d = core.ticket_due("w2").into_iter().next().expect("the note is due");
        // the footer's count and the rule's agree: the note alone is replied on its ticket
        assert_eq!(core.ticket_replies_on("w2", std::slice::from_ref(&d.id)), Some(core.tickets.m.id(d.t)));
        core.tickets.handed.insert(("w2".into(), d.id.clone()), (d.t, d.upto, Default::default()));
        core.ticket_handed("w2", std::slice::from_ref(&d.id), true);
        let loc = Loc { file: "/nonexistent/w2.jsonl".into(), offset: 10, len: 5 };
        core.ticket_seen("w2", &d.id, loc.clone());
        assert_eq!(core.ticket_given_since("w2"), vec![d.id.clone()]);
        core.ticket_turn_cut("w2"); // the process exited mid-turn
        assert!(core.ticket_given_since("w2").is_empty());
        assert_eq!(core.ticket_replies_on("w2", &[]), None);
        core.ticket_turn_ended("w2", loc); // the next session's turn, given nothing
        let x = &core.tickets.m.tickets[&d.t];
        let log = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&coord);
        assert_eq!(x.posts.len(), 1, "nothing replied after the cut");
        assert!(log.contains("\"turn_cut\""), "the cut is logged so replay agrees");
    }

    #[test]
    fn an_agents_turn_is_not_filed() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-turn-{}", std::process::id()));
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"sc1\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        // paper-research m952: no binding opens a filing on turn_end, so the clerk owes nothing and no ticket opens
        assert!(!core.tickets.m.defs.bindings.iter().any(|b| b.ev == "turn_end"));
        let r = core.ticket_input(Input::Event { name: "turn_end".into(), value: json!({"agent": "sc1", "text": "Replied on N-1.", "answered": false}) });
        assert!(r.is_err(), "{r:?}");
        assert!(core.ticket_due("clerk").is_empty());
        assert!(core.tickets.m.tickets.is_empty());
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// The reply tool's post (paper-research N-101.p1285, 1): an agent answers a post by its number; the post records
    /// `answers` and names that post's author. A post that is not there is refused; only the owner types to someone.
    #[test]
    fn an_agents_reply_answers_a_post_and_names_its_author() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        // a folder of its own: `a_turn_given_a_note_is_replied_for`, run beside it, writes and empties docket-reply-PID
        let coord = std::env::temp_dir().join(format!("docket-reply-names-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let add = |id: &str, name: &str, parent: &str| format!("{{\"ev\":\"add\",\"id\":\"{id}\",\"name\":\"{name}\",\"parent\":\"{parent}\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}}\n");
        std::fs::write(coord.join("registry.jsonl"), [add("AGT-001", "orch", "owner"), add("AGT-002", "dev", "orch")].concat()).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let (orch, dev) = (Who::Agent("orch".into()), Who::Agent("dev".into()));
        let r = core.ticket_msg(&orch, None, vec!["dev".into()], "which test fails?", false, None, Vec::new(), Said::default());
        assert!(r.ok, "{}", r.out);
        let n = core.tickets.m.id(*core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0);
        let r = core.ticket_post(&dev, &n, "the parser's".into(), None, Some(1), Said::default(), None, false);
        assert!(r.ok, "{}", r.out);
        assert_eq!(r.out, format!("Posted on {n} as post 2, under post 1."));
        let t = docket_machine::parse_ticket(&n).unwrap();
        let p = core.tickets.m.tickets[&t].posts.last().unwrap().clone();
        assert_eq!((p.by.as_str(), p.answers, p.names.clone()), ("dev", Some(1), vec!["orch".to_string()]));
        let r = core.ticket_post(&dev, &n, "and more".into(), None, Some(9), Said::default(), None, false);
        assert!(!r.ok && r.out.contains("has no post 9"), "{}", r.out);
        let r = core.ticket_post(&dev, &n, "typed".into(), Some("orch".into()), None, Said::default(), None, false);
        assert!(!r.ok, "{}", r.out);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A typed post (I-1480, proto-engine N-1481 p14): an ask, read or blocker names whom it is for, by its name here,
    /// and its line says what it is; a name nobody holds, its writer, an ask to nobody or a read of no head is refused,
    /// and nothing is posted (consult-3 N-1481 p35: never recorded as a name nobody holds), and so is one the team's
    /// norms do not let its writer name, as an `@` (`may_name`; proto-reader p47's red). A hold is on an item, by
    /// its id. `coord msg --re` takes a kind as a post does; a thread it opens takes none. A host session is named by
    /// msg's NAME, a kind's `to` and a post's `@` alike, one test (`named_here`; consult-3 N-1481 p51). His typed post
    /// on a closed ticket is refused, not posted untyped on the thread it would open; his words there name by the same
    /// test, a host and a far agent not carried as on an open ticket (paper-research p97). The request reads as the tools
    /// send it, and one without a kind is written as before.
    #[test]
    fn a_typed_post_names_whom_it_asks_and_says_what_it_is() {
        use docket_machine::PostKind;
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-typed-post-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let add = |id: &str, name: &str, parent: &str| format!("{{\"ev\":\"add\",\"id\":\"{id}\",\"name\":\"{name}\",\"parent\":\"{parent}\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}}\n");
        let role = |name: &str, role: &str| format!("{{\"ev\":\"role\",\"name\":\"{name}\",\"role\":\"{role}\",\"at\":\"x\"}}\n");
        std::fs::write(coord.join("registry.jsonl"), [add("AGT-001", "orch", "owner"), add("AGT-002", "dev", "orch"), add("AGT-003", "adv", "owner"), role("orch", "lead"), role("adv", "consultant")].concat()).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let (orch, dev) = (Who::Agent("orch".into()), Who::Agent("dev".into()));
        let kind = |v: Value| serde_json::from_value::<PostKind>(v).unwrap();
        let r = core.ticket_msg(&orch, None, vec!["dev".into()], "which test fails?", false, None, Vec::new(), Said::default());
        assert!(r.ok, "{}", r.out);
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        let n = core.tickets.m.id(t);
        // dev's ask to orch, with no @: it names orch, and its line says it is an ask
        let ask = kind(json!({"is": "ask", "to": ["orch"]}));
        let r = core.ticket_post_typed(&dev, &n, "is the lock yours?".into(), None, None, Said::default(), None, false, Some(ask.clone()));
        assert!(r.ok, "{}", r.out);
        let p = core.tickets.m.tickets[&t].posts.last().unwrap().clone();
        assert_eq!((p.names.clone(), p.kind.clone()), (vec!["orch".to_string()], Some(ask)));
        let line = |w: &str| {
            let log = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap();
            log.lines().find(|l| l.contains("\"ev\":\"post\"") && l.contains(w)).map(String::from).expect(w)
        };
        assert!(line("is the lock yours?").contains(r#""kind":{"is":"ask","to":["orch"]}"#), "{}", line("is the lock yours?"));
        // an item to hold on, and what is refused
        let r = core.ticket_input(Input::Open { kind: "item".into(), under: None, opener: "owner".into(), form: json!({"title": "the lock"}), text: None, names: vec![], told: vec![] });
        assert!(r.is_ok(), "{r:?}");
        let item = core.tickets.m.id(*core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0);
        assert!(item.starts_with("I-"), "{item}");
        let refused = [
            (json!({"is": "ask", "to": ["w9"]}), "no agent or host named w9".to_string()),
            (json!({"is": "blocker", "to": ["dev"]}), "blocker names dev, its writer: name whom it is for".to_string()),
            (json!({"is": "ask", "to": []}), "ask names nobody: name whom it asks".to_string()),
            (json!({"is": "read", "to": ["orch"], "head": " "}), "a read names the head to read".to_string()),
            (json!({"is": "hold", "on": n.clone()}), format!("a hold is on an item (I-N): {n} is a thread")),
            (json!({"is": "hold", "on": "I-777"}), "a hold is on an item (I-N): there is no I-777".to_string()),
        ];
        for (k, why) in refused {
            let last = core.tickets.m.tickets[&t].last();
            let r = core.ticket_post_typed(&dev, &n, "x".into(), None, None, Said::default(), None, false, Some(kind(k)));
            assert_eq!((r.ok, r.out), (false, format!("coord post: {why}")));
            assert_eq!(core.tickets.m.tickets[&t].last(), last, "nothing posted");
        }
        // a `to` the team's norms do not let its writer name is refused as an `@` is (`may_name`): dev may not name the
        // consultant on a thread it is not on
        let last = core.tickets.m.tickets[&t].last();
        let r = core.ticket_post_typed(&dev, &n, "is this right?".into(), None, None, Said::default(), None, false, Some(kind(json!({"is": "ask", "to": ["adv"]}))));
        assert!(!r.ok && r.out.contains(&format!("do not let you name adv on {n}")), "{}", r.out);
        assert_eq!(core.tickets.m.tickets[&t].last(), last, "nothing posted");
        let r = core.ticket_post_typed(&dev, &n, "the lock waits on this".into(), None, None, Said::default(), None, false, Some(kind(json!({"is": "hold", "on": item}))));
        assert!(r.ok, "{}", r.out);
        assert_eq!(core.tickets.m.tickets[&t].posts.last().unwrap().kind, Some(PostKind::Hold { on: item.clone() }));
        // `coord msg --thread N`: an fyi, naming everyone on it but its writer; a thread opened takes no kind
        let r = core.ticket_msg_under(&orch, None, vec![], "and the docs pass", false, Some(n.clone()), None, Vec::new(), Said::default(), None, None, Some(PostKind::Fyi));
        assert!(r.ok, "{}", r.out);
        let p = core.tickets.m.tickets[&t].posts.last().unwrap().clone();
        assert_eq!((p.by.as_str(), p.names.clone(), p.kind.clone()), ("orch", vec!["dev".to_string()], Some(PostKind::Fyi)));
        let opened = core.tickets.m.tickets.len();
        let r = core.ticket_msg_under(&orch, None, vec!["dev".into()], "a new one", false, None, None, Vec::new(), Said::default(), None, None, Some(PostKind::Result));
        assert!(!r.ok && r.out.starts_with("coord msg: a kind is a post's on a thread"), "{}", r.out);
        assert_eq!(core.tickets.m.tickets.len(), opened, "nothing opened");
        // a host session (`hosts/hal`): `coord msg --re N hal`, `coord msg --thread N --ask hal`, `coord post --ask hal`
        // and an `@hal` in a post's words each name it
        std::fs::create_dir_all(coord.join("hosts")).unwrap();
        std::fs::write(coord.join("hosts/hal"), "00000000-0000-0000-0000-000000000000\n").unwrap();
        let hal = || kind(json!({"is": "ask", "to": ["hal"]}));
        let names = |core: &Core| core.tickets.m.tickets[&t].posts.last().unwrap().names.clone();
        let r = core.ticket_msg_under(&orch, None, vec!["hal".into()], "for the host", false, Some(n.clone()), None, Vec::new(), Said::default(), None, None, None);
        assert!(r.ok && names(&core).contains(&"hal".to_string()), "{} {:?}", r.out, names(&core));
        let r = core.ticket_msg_under(&orch, None, vec![], "is it yours?", false, Some(n.clone()), None, Vec::new(), Said::default(), None, None, Some(hal()));
        assert!(r.ok && names(&core).contains(&"hal".to_string()), "{} {:?}", r.out, names(&core));
        let r = core.ticket_post_typed(&dev, &n, "and this?".into(), None, None, Said::default(), None, false, Some(hal()));
        assert!(r.ok && names(&core).contains(&"hal".to_string()), "{} {:?}", r.out, names(&core));
        let r = core.ticket_post(&dev, &n, "@hal and the docs".into(), None, None, Said::default(), None, false);
        assert!(r.ok && names(&core).contains(&"hal".to_string()), "{} {:?}", r.out, names(&core));
        // another project's agent where this coord's messages are not carried (no host_relay): refused, never left out,
        // by `post` and by `msg`, as an `@` and as a kind's `to`; an `@` that names nobody is words (consult-3 p62)
        let last = core.tickets.m.tickets[&t].last();
        let far_to = || kind(json!({"is": "ask", "to": ["cav/w2"]}));
        let refused = [
            core.ticket_post(&dev, &n, "and @cav/w2".into(), None, None, Said::default(), None, false),
            core.ticket_post_typed(&dev, &n, "and this".into(), None, None, Said::default(), None, false, Some(far_to())),
            core.ticket_msg_under(&orch, None, vec!["cav/w2".into()], "and this", false, Some(n.clone()), None, Vec::new(), Said::default(), None, None, None),
            core.ticket_msg_under(&orch, None, vec![], "and this", false, Some(n.clone()), None, Vec::new(), Said::default(), None, None, Some(far_to())),
        ];
        let outs: Vec<String> = refused.into_iter().map(|r| if r.ok { format!("posted: {}", r.out) } else { r.out }).collect();
        assert_eq!(outs, ["post", "post", "msg", "msg"].map(|c| format!("coord {c}: {NOT_CARRIED}")));
        assert_eq!(core.tickets.m.tickets[&t].last(), last, "nothing posted");
        let r = core.ticket_post(&dev, &n, "@w9 is no name here".into(), None, None, Said::default(), None, false);
        assert!(r.ok && !names(&core).contains(&"w9".to_string()), "{} {:?}", r.out, names(&core));
        // his typed post on a closed ticket: refused, nothing opened
        let before = core.tickets.m.tickets.len();
        let r = core.ticket_msg(&orch, None, vec!["dev".into()], "another thread", false, None, Vec::new(), Said::default());
        assert!(r.ok && core.tickets.m.tickets.len() > before, "{}", r.out);
        let c = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        // an agent's `coord msg` thread is closed from its open, and takes its posts (pp-alias-line2.log: "N-3 is
        // closed already" to a cancel)
        assert!(!core.tickets.m.tickets[&c].is_open());
        let (cid, opened) = (core.tickets.m.id(c), core.tickets.m.tickets.len());
        let r = core.ticket_post_typed(&Who::Owner, &cid, "one more".into(), None, None, Said::default(), Some("orch".into()), false, Some(PostKind::Fyi));
        assert_eq!((r.ok, r.out), (false, format!("coord post: {cid} is closed, and his words on it open a thread, which takes no kind")));
        assert_eq!(core.tickets.m.tickets.len(), opened, "nothing opened");
        // his words there are named by the same test (`post_names`; paper-research N-1481 p97): his `@hal` names the host
        // on the thread they open (his `owner_message`, run as coordd's task runner runs it), and his `@cav/w2`, not
        // carried here, is refused, nothing said
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.tickets.tasks = Some(tx);
        let r = core.ticket_post(&Who::Owner, &cid, "@hal one more".into(), None, None, Said::default(), Some("orch".into()), false);
        assert!(r.ok, "{}", r.out);
        let said = std::iter::from_fn(|| rx.try_recv().ok()).find_map(|t| match t {
            Task::Event { name, value } if name == "owner_message" => Some(value),
            _ => None,
        });
        let said = said.expect("his words are said");
        assert_eq!((said["names"].clone(), said["follows"].clone()), (json!(["hal"]), json!(cid)));
        core.ticket_input(Input::Event { name: "owner_message".into(), value: said }).unwrap();
        let o = &core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().1.posts;
        assert!(o.first().is_some_and(|p| p.by == "owner" && p.names.contains(&"hal".to_string())), "{o:?}");
        let r = core.ticket_post(&Who::Owner, &cid, "@cav/w2 one more".into(), None, None, Said::default(), Some("orch".into()), false);
        assert_eq!((r.ok, r.out), (false, format!("coord post: {NOT_CARRIED}")));
        assert!(rx.try_recv().is_err(), "nothing said");
        core.tickets.tasks = None;
        // the request as coordc and the reply tool send it; without a kind, as before
        let req: coord_types::Request = serde_json::from_value(json!({"op": "post", "t": n, "text": "x", "answers": 2, "kind": {"is": "read", "to": ["orch"], "head": "core2-pp 1a2b3c4d"}})).unwrap();
        assert!(matches!(&req, coord_types::Request::Post { kind: Some(PostKind::Read { to, head }), .. } if to == &vec!["orch".to_string()] && head == "core2-pp 1a2b3c4d"), "{req:?}");
        let req: coord_types::Request = serde_json::from_value(json!({"op": "post", "t": n, "text": "x"})).unwrap();
        assert!(matches!(&req, coord_types::Request::Post { kind: None, .. }) && serde_json::to_value(&req).unwrap().get("kind").is_none(), "{req:?}");
        let r = core.ticket_post(&dev, &n, "untyped, as before".into(), None, None, Said::default(), None, false);
        assert!(r.ok, "{}", r.out);
        assert!(!line("untyped, as before").contains("\"kind\""), "{}", line("untyped, as before"));
        let _ = std::fs::remove_dir_all(&coord);
    }

    #[test]
    fn a_consultant_is_named_by_a_lead_or_on_a_ticket_it_is_on() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-name-{}", std::process::id()));
        std::fs::create_dir_all(&coord).unwrap();
        let add = |id: &str, name: &str, parent: &str| format!("{{\"ev\":\"add\",\"id\":\"{id}\",\"name\":\"{name}\",\"parent\":\"{parent}\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}}\n");
        let role = |name: &str, role: &str| format!("{{\"ev\":\"role\",\"name\":\"{name}\",\"role\":\"{role}\",\"at\":\"x\"}}\n");
        let reg = [add("AGT-001", "orch", "owner"), add("AGT-002", "dev", "orch"), add("AGT-003", "adv", "owner"), role("orch", "lead"), role("adv", "consultant")].concat();
        std::fs::write(coord.join("registry.jsonl"), reg).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let (orch, dev) = (Who::Agent("orch".into()), Who::Agent("dev".into()));
        let msg = |core: &mut Core, who: &Who, to: &[&str], text: &str, re: Option<&str>| {
            core.ticket_msg(who, None, to.iter().map(|s| s.to_string()).collect(), text, false, re.map(String::from), Vec::new(), Said::default())
        };
        // a member may not name the consultant on a new note; the refusal says why and whom to take it to
        let r = msg(&mut core, &dev, &["adv"], "is this right?", None);
        assert!(!r.ok && r.out.contains("do not let you name adv") && r.out.contains("lead (orch)"), "{}", r.out);
        // nor with @ on a post on a ticket the consultant is not on
        let r = msg(&mut core, &dev, &["orch"], "a note to my lead", None);
        assert!(r.ok, "{}", r.out);
        let n = core.tickets.m.id(*core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0);
        let r = core.ticket_post(&dev, &n, "and @adv should see it".into(), None, None, Said::default(), None, false);
        assert!(!r.ok && r.out.contains(&format!("name adv on {n}")), "{}", r.out);
        // the lead may
        let r = msg(&mut core, &orch, &["adv"], "please review", None);
        assert!(r.ok, "{}", r.out);
        let q = core.tickets.m.id(*core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0);
        // and on that ticket, which the consultant takes part in, the member may too
        let r = msg(&mut core, &dev, &["adv"], "my part is done", Some(&q));
        assert!(r.ok, "{}", r.out);
        // naming anyone who is no consultant needs nothing
        let r = msg(&mut core, &dev, &["orch"], "fine", None);
        assert!(r.ok, "{}", r.out);
        let _ = std::fs::remove_dir_all(&coord);
    }

    #[test]
    fn names_in_a_post() {
        let known = |n: &str| n == "dev" || n == "owner";
        assert_eq!(named_in("@dev, see this (and @owner). @nobody", &known), vec!["dev", "owner"]);
    }

    /// §13 on docket-live (12 Nov): the watcher's route reads the held post's words and the ticket's last posts. The
    /// handler runs in a ticket under T with no parent, so both are found through what it holds (`post_of`).
    #[test]
    fn route_is_given_the_held_words_and_the_posts_before_them() {
        use crate::core::Core;
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-route-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let reg = |id: &str, name: &str| format!("{{\"ev\":\"add\",\"id\":\"{id}\",\"name\":\"{name}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}}\n");
        std::fs::write(coord.join("registry.jsonl"), reg("AGT-001", "lead") + &reg("AGT-002", "fixer")).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let v = json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": [], "text": "add mode(xs) to calc", "addressed": {"head": "Unnamed"}, "esc": []});
        core.ticket_input(Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        core.ticket_answer(docket_machine::WATCHER, t, json!({"head": "Thread"}), Text::New(String::new())).unwrap();
        let post = |core: &mut Core, by: &str, text: &str, names: &[&str]| {
            core.ticket_input(Input::Post { t, by: by.into(), text: Text::New(text.into()), names: names.iter().map(|n| n.to_string()).collect(), told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        };
        post(&mut core, "lead", "Design: Counter, smallest of ties. @fixer review it", &["fixer"]);
        post(&mut core, "fixer", "Approved.", &["lead"]);
        // the owner's words to lead, placed on T by intake as on a live system, name nobody, and two agents are on T: held, and the watcher owes route
        let v = json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": [], "text": "looks good, go ahead", "addressed": {"head": "Unnamed"}, "esc": []});
        core.ticket_input(Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let u = *core.tickets.m.tickets.keys().filter(|k| **k < 1 << 40).next_back().unwrap();
        let tid = core.tickets.m.id(t);
        core.ticket_answer(docket_machine::WATCHER, u, json!({"head": "Place", "ticket": tid}), Text::New(String::new())).unwrap();
        let due = core.ticket_due(docket_machine::WATCHER);
        let d = due.iter().find(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "route")).expect("a route step for the watcher");
        let job = core.clerk_job(d).expect("route's job");
        let r = job.route.as_ref().expect("a route job, not intake's");
        assert_eq!(job.text, "looks good, go ahead");
        assert_eq!((r.to.clone(), r.others.clone()), (vec!["lead".to_string()], vec!["fixer".to_string()]));
        // each post says whom it went to (consult-3 N-1443 p2, 2a)
        assert!(r.recent.contains("lead, to fixer: Design: Counter") && r.recent.ends_with("fixer, to lead: Approved."), "{}", r.recent);
        assert!(!r.recent.contains("go ahead"), "the held post is the message, not one of the posts before it");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// The thread's holder (route.call; N-1104 p586/p587, 20 Nov, research's record 101194-101265): his second post's
    /// route is given the first's place: whom it went to and when it was given (consult-3 N-1443 p2, 2a), whom the
    /// thread was opened for, and that the agent holds it in the turn it is in; the first post is the only one `joins`
    /// may name. All of it the machine's, read from the record.
    #[test]
    fn the_holder_is_given_where_his_post_before_went() {
        use crate::core::Core;
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-holder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let reg = |id: &str, name: &str| format!("{{\"ev\":\"add\",\"id\":\"{id}\",\"name\":\"{name}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}}\n");
        std::fs::write(coord.join("registry.jsonl"), reg("AGT-001", "lead") + &reg("AGT-002", "fixer")).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let v = json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": [], "text": "special actions for limits", "addressed": {"head": "Unnamed"}, "esc": []});
        core.ticket_input(Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        core.ticket_answer(docket_machine::WATCHER, t, json!({"head": "Thread"}), Text::New(String::new())).unwrap();
        core.ticket_input(Input::Post { t, by: "lead".into(), text: Text::New("Taken. @fixer the source table is yours".into()), names: vec!["fixer".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let routes = |core: &Core| -> Vec<docket_machine::Delivery> { core.ticket_due(docket_machine::WATCHER).into_iter().filter(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "route")).collect() };
        // his p586: on the thread itself, typed to nobody; the holder's form To lead; lead is given it
        core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("eh yeah but wait lets analyze where we need special actions vs not".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let a = core.tickets.m.tickets[&t].last();
        let d = routes(&core).pop().expect("a route step for his first post");
        let first = core.clerk_job(&d).unwrap();
        assert_eq!(first.route.as_ref().unwrap().yours, "(none)", "an agent posted last: nothing of his before it");
        core.ticket_answer(docket_machine::WATCHER, d.t, json!({"head": "To", "agents": ["lead"]}), Text::New(String::new())).unwrap();
        let give = core.ticket_due("lead").into_iter().find(|d| d.t == t).expect("his post is due to lead");
        core.ticket_given("lead", &give);
        // his p587, 22 s later on the record: its route's report
        core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("like ofc for reset apply can be special action".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let d = routes(&core).pop().expect("a route step for his second post");
        let job = core.clerk_job(&d).unwrap();
        let r = job.route.as_ref().unwrap();
        let id = core.tickets.m.id(t);
        assert!(r.thread.starts_with(&format!("{id} \"special actions for limits\", opened for lead.")), "{}", r.thread);
        assert!(r.thread.contains(&format!("lead has been given #{a} in the turn it is in")), "{}", r.thread);
        assert!(r.yours.starts_with(&format!("#{a} at ")) && r.yours.contains("went to lead (given ") && r.yours.ends_with(": eh yeah but wait lets analyze where we need special actions vs not"), "{}", r.yours);
        assert_eq!(r.his, vec![a]);
        // should its call fail, p587 goes to lead, whom the thread is for (proto-lead N-1446 p57.2), and he is not asked
        assert_eq!(r.fallback, vec!["lead".to_string()]);
        assert_eq!(crate::clerk::route_failed(r), json!({"head": "To", "agents": ["lead"]}));
        assert!(r.recent.contains(&format!("#{a} owner, to lead: eh yeah")), "{}", r.recent);
        // what the holder read is on the record, on the `decided` line with the model that read it (made up here: no
        // model is called in a test)
        let docket_machine::What::Step { step, .. } = &d.what else { panic!("a step") };
        let decision = coord_types::Decision { decision: "model-for".into(), chosen: Some("gpt-6.2-sol".into()), prompt: Some(format!("The ticket: {}", r.thread)), ..Default::default() };
        core.ticket_input(model_for(d.t, *step, &decision)).unwrap();
        let log = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap();
        let decided: Vec<Value> = log.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|l| l["ev"] == "decided" && l["t"] == json!(d.t)).collect();
        assert_eq!(decided.len(), 1, "{log}");
        assert_eq!(decided[0]["provenance"]["prompt"], json!(format!("The ticket: {}", r.thread)));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A decision's `decided` line writes each rule that left options out once, with the options it left out, in the
    /// order they came; an option two rules left out is under each (paper-research N-1446 p2667: 150.6 MB of research's
    /// 230 MB record was `excluded`, its 16 rule sentences written 228,994 times). The rest of the provenance is as it
    /// was, and `coord_types::excluded_options` reads an entry of either shape.
    #[test]
    fn a_decided_line_writes_each_rule_that_left_options_out_once() {
        let (mut core, coord) = reply_at_core("excluded");
        let t = reply_at_open(&mut core, "w2", "w3", "Which model?");
        let only = "Only for model-for: family(option, \"codex\") (/x/policy/decisions.eflint:4)";
        let level = "Requires level(option) >= 2 (/x/policy/teams.eflint:9)";
        let why = Some(String::from("The clerk runs on Codex only."));
        let ex = |o: &str, reason: &str, rule: &str, why: &Option<String>| coord_types::Exclusion { option: o.into(), reason: reason.into(), rule: rule.into(), why: why.clone() };
        let excluded = vec![ex("haiku", "only", only, &why), ex("luna", "requires", level, &None), ex("sonnet", "only", only, &why), ex("luna", "only", only, &why)];
        let decision = coord_types::Decision { decision: "model-for".into(), chosen: Some("sol".into()), excluded, ..Default::default() };
        core.ticket_input(model_for(t, 0, &decision)).unwrap();
        let log = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap();
        let line = log.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).find(|l| l["ev"] == "decided" && l["t"] == json!(t)).expect("its decided line");
        let by_rule = json!([
            {"reason": "only", "rule": only, "why": "The clerk runs on Codex only.", "options": ["haiku", "sonnet", "luna"]},
            {"reason": "requires", "rule": level, "options": ["luna"]},
        ]);
        assert_eq!(line["provenance"]["excluded"], by_rule, "{line}");
        assert_eq!(line["provenance"]["chosen"], json!("sol"));
        let named: Vec<&str> = by_rule.as_array().unwrap().iter().flat_map(coord_types::excluded_options).collect();
        assert_eq!(named, ["haiku", "sonnet", "luna", "luna"]);
        assert_eq!(coord_types::excluded_options(&json!({"option": "haiku", "reason": "only", "rule": only})), ["haiku"]);
        // a `fills` outcome's, under `policy`; a list by rule already stays as it is
        let fills = excluded_by_rule(json!({"policy": {"excluded": [{"option": "dev", "reason": "requires", "rule": level}, {"option": "ops", "reason": "requires", "rule": level}]}, "default": "the minder"}));
        assert_eq!(fills, json!({"policy": {"excluded": [{"reason": "requires", "rule": level, "options": ["dev", "ops"]}]}, "default": "the minder"}));
        assert_eq!(excluded_by_rule(fills.clone()), fills);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// His post told late ((c2); consult-3 N-1446 p621): his post, which an Unclear held under his question, goes to lead
    /// by a delivery of its own, `N-T.pA.told`, given with lead's mark below it. His next post's route reads lead as
    /// holding #A in the turn it is in (`delivered_post`), and #A as given at that delivery (`ViewGiven::hands`), not
    /// "not given yet".
    #[test]
    fn the_holder_reads_his_post_told_late_as_given_by_its_own_delivery() {
        use crate::core::Core;
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-holder-told-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let reg = |id: &str, name: &str| format!("{{\"ev\":\"add\",\"id\":\"{id}\",\"name\":\"{name}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}}\n");
        std::fs::write(coord.join("registry.jsonl"), reg("AGT-001", "lead") + &reg("AGT-002", "fixer")).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let v = json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": [], "text": "the dashboard", "addressed": {"head": "Unnamed"}, "esc": []});
        core.ticket_input(Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        core.ticket_answer(docket_machine::WATCHER, t, json!({"head": "Thread"}), Text::New(String::new())).unwrap();
        core.ticket_input(Input::Post { t, by: "lead".into(), text: Text::New("Taken. @fixer the ledger is yours".into()), names: vec!["fixer".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let routes = |core: &Core| -> Vec<docket_machine::Delivery> { core.ticket_due(docket_machine::WATCHER).into_iter().filter(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "route")).collect() };
        // his post, typed to nobody: route cannot tell whom it is for, and he is asked about it
        core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("the median on it looks off".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let a = core.tickets.m.tickets[&t].last();
        let d = routes(&core).pop().expect("a route step for his post");
        core.ticket_answer(docket_machine::WATCHER, d.t, json!({"head": "Unclear", "why": "which work is the median?"}), Text::New(String::new())).unwrap();
        assert!(core.tickets.m.tickets[&t].asked.contains(&a), "held under his question");
        // his press sends it to lead: a delivery of its own, given with lead's mark below it
        core.ticket_answer("owner", d.t, json!({"head": "Send", "to": ["lead"]}), Text::New(String::new())).unwrap();
        let told = format!("{}.p{a}.{}", core.tickets.m.id(t), docket_machine::TOLD);
        let give = core.ticket_due("lead").into_iter().find(|d| d.id == told).expect("his post told late to lead");
        assert!(give.upto < a, "{give:?}");
        core.ticket_given("lead", &give);
        // his next post: its route reads lead as holding the first in the turn it is in, given at that delivery
        core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("the one on the release page".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let d = routes(&core).pop().expect("a route step for his next post");
        let job = core.clerk_job(&d).unwrap();
        let r = job.route.as_ref().unwrap();
        assert!(r.thread.contains(&format!("lead has been given #{a} in the turn it is in")), "{}", r.thread);
        assert!(r.yours.contains(&format!("#{a} at ")) && r.yours.contains("went to lead (given "), "{}", r.yours);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A hand-over made while his post is asked about, and given after his Send (proto-reader N-1446 p621, red at
    /// reader-c2-race c767d66c; proto-lead p631): what it left out of his posts is kept with it (`Machine::left_out`),
    /// so his take-back while it is on its way marks it for nothing (`backs`), and its `given` line carries it: fixer's
    /// row does not hand his post, which went to lead alone.
    #[test]
    fn a_hand_over_made_while_he_is_asked_keeps_what_it_left_out_to_its_given_line() {
        use crate::core::{Core, Who};
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-left-out-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let reg = |id: &str, name: &str| format!("{{\"ev\":\"add\",\"id\":\"{id}\",\"name\":\"{name}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}}\n");
        std::fs::write(coord.join("registry.jsonl"), reg("AGT-001", "lead") + &reg("AGT-002", "fixer")).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let v = json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": [], "text": "the dashboard", "addressed": {"head": "Unnamed"}, "esc": []});
        core.ticket_input(Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        core.ticket_answer(docket_machine::WATCHER, t, json!({"head": "Thread"}), Text::New(String::new())).unwrap();
        core.ticket_input(Input::Post { t, by: "lead".into(), text: Text::New("Taken. @fixer the ledger is yours".into()), names: vec!["fixer".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let routes = |core: &Core| -> Vec<docket_machine::Delivery> { core.ticket_due(docket_machine::WATCHER).into_iter().filter(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "route")).collect() };
        let id = core.tickets.m.id(t);
        // his post typed to nobody, then one to fixer by name, before route reads the first (his next words after a
        // question to him would hand the first to all: `hand_unanswered`): route cannot tell whom the first is for, so
        // the second goes to fixer, past it, and he is asked about the first
        core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("the median on it looks off".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let a = core.tickets.m.tickets[&t].last();
        core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("@fixer and the ledger should mark p614 kept".into()), names: vec!["fixer".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let b = core.tickets.m.tickets[&t].last();
        let d = routes(&core).pop().expect("a route step for his first post");
        core.ticket_answer(docket_machine::WATCHER, d.t, json!({"head": "Unclear", "why": "which work is the median?"}), Text::New(String::new())).unwrap();
        assert!(core.tickets.m.tickets[&t].asked.contains(&a), "held under his question");
        // fixer's part, handed now, leaves the first out
        let due = |core: &Core, agent: &str| -> Vec<(String, u32)> { core.ticket_due(agent).iter().map(|d| (d.id.clone(), d.upto)).collect() };
        let part = core.ticket_due("fixer").into_iter().find(|d| d.t == t && d.upto >= b).unwrap_or_else(|| panic!("fixer's part: {:?}", due(&core, "fixer")));
        let _ = core.ticket_handing("fixer", vec![part.clone()]);
        assert_eq!(core.tickets.handed[&("fixer".to_string(), part.id.clone())].2.left_out, Some(BTreeSet::from([a])), "kept with the hand-over");
        // his press sends the first to lead, by a delivery of its own, handed too
        core.ticket_answer("owner", d.t, json!({"head": "Send", "to": ["lead"]}), Text::New(String::new())).unwrap();
        let told = format!("{id}.p{a}.{}", docket_machine::TOLD);
        let give = core.ticket_due("lead").into_iter().find(|d| d.id == told).unwrap_or_else(|| panic!("his post told late to lead: {:?}", due(&core, "lead")));
        let _ = core.ticket_handing("lead", vec![give]);
        // his take-back while both are on their way: lead's carries the first, fixer's does not
        let r = core.ticket_take_back(&Who::Owner, &id, a);
        assert!(r.ok, "{}", r.out);
        let marked: Vec<(String, String)> = core.tickets.backs.keys().cloned().collect();
        assert_eq!(marked, vec![("lead".to_string(), told.clone())]);
        // fixer takes its part: its `given` line says what it left out, and the row does not hand the first
        core.ticket_handed("fixer", std::slice::from_ref(&part.id), true);
        assert!(!core.tickets.m.tickets[&t].handed("fixer", a));
        let log = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap();
        let line = log.lines().find(|l| l.contains("\"ev\":\"given\"") && l.contains("\"to\":\"fixer\"") && l.contains(&format!("\"delivery\":\"{}\"", part.id))).expect("fixer's given line");
        assert!(line.contains(&format!("\"skipped\":[{a}]")), "{line}");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// An agent's thread naming another (N-1445, research's record 103218-103418): thread.kind before ls-opener posted
    /// the opener's words again as his (#2; today's posts them once, as the opener's #1), so they are not his; his #3's
    /// YOURS leaves them out, `joins` can name none, and a failed call sends #3 to the agent the thread is for
    /// (proto-reader N-1446 p71; proto-lead p74, 2 and p57.2).
    #[test]
    fn the_holder_leaves_out_an_agents_opener_posted_as_his_and_a_failed_call_goes_where_the_thread_is_for() {
        use crate::core::Core;
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-opener-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let reg = |id: &str, name: &str| format!("{{\"ev\":\"add\",\"id\":\"{id}\",\"name\":\"{name}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}}\n");
        std::fs::write(coord.join("registry.jsonl"), reg("AGT-001", "lead") + &reg("AGT-002", "fixer")).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        // lead opens a thread naming fixer, as paper-research opened N-1445 naming paper-research-brain
        let words = "lead · **Brain fundamentals: this talk now has its own thread.** @fixer carries it, here.";
        let form = json!({"to": "fixer", "typed": ["fixer"], "names": ["fixer"], "text": words, "addressed": {"head": "Named"}, "label": "Brain fundamentals"});
        core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "lead".into(), form, text: Some(Text::New(words.into())), names: vec!["fixer".into()], told: vec![] }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        let by: Vec<(u32, String)> = core.tickets.m.tickets[&t].posts.iter().map(|p| (p.n, p.by.clone())).collect();
        assert_eq!(by, vec![(1, "lead".to_string())], "ls-opener: the opener's words once, as its own #1");
        // a thread opened under thread.kind before ls-opener, as N-1445 was (103219), has them again as #2 by the
        // owner; a post of his with those very words, routed to fixer, stands for it
        core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New(words.into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let d = core.ticket_due(docket_machine::WATCHER).into_iter().find(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "route")).expect("a route step for #2");
        core.ticket_input(Input::Answer { t: d.t, by: docket_machine::WATCHER.into(), head: json!({"head": "To", "agents": ["fixer"]}), text: Text::New(String::new()) }).unwrap();
        let by: Vec<(u32, String, Vec<String>)> = core.tickets.m.tickets[&t].posts.iter().map(|p| (p.n, p.by.clone(), p.told.clone())).collect();
        assert_eq!(by[1], (2, "owner".to_string(), vec!["fixer".to_string()]), "as on the record: #2 is the opener's words as his");
        // his #3, typed to nobody
        core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("okay nice so can we run through some real world examples".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let d = core.ticket_due(docket_machine::WATCHER).into_iter().filter(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "route")).next_back().expect("a route step for his #3");
        let job = core.clerk_job(&d).unwrap();
        let r = job.route.as_ref().unwrap();
        assert_eq!((r.yours.as_str(), r.his.clone()), ("(none)", Vec::<u32>::new()), "#2 is not his: {}", r.yours);
        assert!(r.thread.contains("\"Brain fundamentals\", opened for fixer."), "{}", r.thread);
        // RECENT lists #2, under the agent that wrote it (proto-lead p105)
        assert!(r.recent.contains("\n#2 lead (its opener, posted as the thread's first words), to fixer: lead · **Brain fundamentals"), "{}", r.recent);
        assert!(!r.recent.contains("#2 owner"), "{}", r.recent);
        let joined = json!({"head": "To", "agents": ["fixer"], "joins": 2});
        assert_eq!(crate::clerk::joins_kept(joined, r, &d.ticket), json!({"head": "To", "agents": ["fixer"]}), "joining the opener is left out");
        assert_eq!(r.fallback, vec!["fixer".to_string()]);
        assert_eq!(crate::clerk::route_failed(r), json!({"head": "To", "agents": ["fixer"]}));
        // the call itself, failing with no model reached: a stand-in on loopback turns it away as the gateway did
        // tonight (coordd.log 27044), and route's answer line says why, in coordd's words, with no notice (paper-research
        // N-1446 p186, p203)
        let seen = crate::clean::tests::stand_in(|_| {
            json!({"refused": 429, "error": {"code": "model_cooldown", "last_upstream_error": "usage_limit_reached: The usage limit has been reached", "message": "All credentials for model gpt-6.2-sol are cooling down (last error: usage_limit_reached: The usage limit has been reached)", "model": "gpt-6.2-sol", "reset_seconds": 683, "reset_time": "11m23s"}})
        });
        let (head, decision, failed) = crate::clerk::answer(&job);
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
        let asked: Vec<String> = seen.lock().unwrap().iter().map(|q| q["model"].as_str().unwrap_or_default().to_string()).collect();
        assert!(!asked.is_empty() && asked.iter().all(|m| m == "gpt-6.2-sol"), "the policy's one model: {asked:?}");
        let at = decision.as_ref().map(|d| d.at.clone()).expect("the decision is kept for the record");
        // the gateway's time rounded up to the next minute, on his clock (proto-lead, N-1446)
        let back = chrono::DateTime::parse_from_rfc3339(&at).unwrap() + chrono::Duration::seconds(683);
        let back = back + chrono::Duration::seconds((60 - back.timestamp().rem_euclid(60)) % 60);
        let again = back.with_timezone(&chrono::Local).format("%H:%M").to_string();
        let said = format!("the routing call failed: Codex is at its usage limit (gpt-6.2-sol, 429); the gateway tries it again at {again}");
        assert_eq!((&head, &failed), (&json!({"head": "To", "agents": ["fixer"]}), &Some(said.clone())));
        core.clerk_answered(&job, head, decision, failed);
        let log = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap();
        let lines: Vec<Value> = log.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|l| l["t"] == json!(d.t)).collect();
        let answers: Vec<&Value> = lines.iter().filter(|l| l["ev"] == "answer").collect();
        assert_eq!(answers.len(), 1, "{log}");
        assert_eq!((&answers[0]["head"], &answers[0]["text"]), (&json!({"head": "To", "agents": ["fixer"]}), &json!(said)), "{log}");
        assert_eq!(lines.iter().filter(|l| l["ev"] == "decided").count(), 1, "the failed decision is on the record: {log}");
        assert_eq!(core.tickets.m.tickets[&t].posts.iter().find(|p| p.n == 3).unwrap().told, vec!["fixer".to_string()], "#3 went where the thread is for");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A failed call's "where his last post went" (proto-lead N-1446 p57.2), through a real Core: only the agents of it
    /// still on the ticket, as `route_ok` holds a model's answer to (consult-3 p110, 2; proto-reader p126). The branch
    /// is reached when the ticket's `to` is no agent on it, here an agent's thread to him. His post that went to an
    /// agent not on the ticket (a made-up answer) is passed over for the post before it; with none left, he is asked.
    #[test]
    fn a_failed_calls_last_post_fallback_keeps_only_agents_on_the_ticket() {
        use crate::core::Core;
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-fallback-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let reg = |id: &str, name: &str| format!("{{\"ev\":\"add\",\"id\":\"AGT-{id}\",\"name\":\"{name}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}}\n");
        std::fs::write(coord.join("registry.jsonl"), reg("001", "lead") + &reg("002", "fixer") + &reg("003", "ghost")).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let words = "lead · a question for you about the source table; @fixer has the data";
        let form = json!({"to": "owner", "typed": ["owner"], "names": ["fixer"], "text": words, "addressed": {"head": "Named"}, "label": "Source table"});
        core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "lead".into(), form, text: Some(Text::New(words.into())), names: vec!["fixer".into()], told: vec![] }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        let route = |core: &mut Core, words: &str| -> (docket_machine::Delivery, crate::clerk::Route) {
            core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New(words.into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
            let d = core.ticket_due(docket_machine::WATCHER).into_iter().filter(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "route")).next_back().expect("a route step for his post");
            let r = core.clerk_job(&d).unwrap().route.unwrap();
            (d, r)
        };
        // his first post: the thread's `to` is him, no agent on it, and he has no post before (#2 is lead's words, not
        // his); it goes to `ghost`, an agent of the project that is not on the ticket (never named there, never posting)
        let (d, r) = route(&mut core, "which one is newer");
        assert!(r.fallback.is_empty(), "{:?}", r.fallback);
        core.ticket_answer(docket_machine::WATCHER, d.t, json!({"head": "To", "agents": ["ghost"]}), Text::New(String::new())).unwrap();
        // his second: his last post went only to ghost, so nothing is left, and a failed call asks him
        let (d, r) = route(&mut core, "the one from tuesday i mean");
        assert!(r.fallback.is_empty(), "ghost is not on the ticket: {:?}", r.fallback);
        assert_eq!(crate::clerk::route_failed(&r), json!({"head": "Unclear", "why": "the call that says whom it is for failed"}));
        core.ticket_answer(docket_machine::WATCHER, d.t, json!({"head": "To", "agents": ["fixer"]}), Text::New(String::new())).unwrap();
        // his third, after one to ghost again: passed over, the post before it went to fixer
        let (d, _) = route(&mut core, "and check the totals");
        core.ticket_answer(docket_machine::WATCHER, d.t, json!({"head": "To", "agents": ["ghost"]}), Text::New(String::new())).unwrap();
        let (_, r) = route(&mut core, "then post it here");
        assert_eq!(r.fallback, vec!["fixer".to_string()]);
        assert_eq!(crate::clerk::route_failed(&r), json!({"head": "To", "agents": ["fixer"]}));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-1788: his words to an agent in Step are not gathered (no `hold` on their line: staged as he wrote them, no
    /// burst waited for), while his to an agent in Run are, as the policy says; the record's own line gives the post
    /// a request made (`Tickets::posted`, `Reply::post`). Esc with step puts the agent in Step in the same act, the
    /// owner's only.
    #[test]
    fn his_words_to_an_agent_in_step_are_not_gathered_and_name_their_post() {
        use coord_types::AgentMode;
        let (mut core, _rx, coord) = esc_coord("stepgather", &[("lead", "owner", ""), ("fixer", "owner", "")], Some("+lapse-after(\"gather\", 15).\n"), None);
        assert!(!core.ticket_esc_step(&Who::Agent("fixer".into()), vec!["lead".into()], true).ok, "the owner's");
        assert_eq!(core.agents["lead"].control, AgentMode::Run);
        let r = core.ticket_esc_step(&Who::Owner, vec!["lead".into()], true);
        assert!(r.ok && r.out.contains("lead is in Step now"), "{}", r.out);
        assert_eq!(core.agents["lead"].control, AgentMode::Step);
        core.tickets.posted = None;
        let v = json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": [], "text": "the build", "addressed": {"head": "Unnamed"}, "esc": []});
        core.ticket_input(Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        let log = || -> Vec<Value> { std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).collect() };
        let message = log().into_iter().rfind(|l| l["ev"] == "event" && l["name"] == "owner_message").unwrap();
        assert_eq!(message["value"].get("hold"), None, "{message}");
        let his = |core: &mut Core, to: &str| {
            core.tickets.posted = None;
            core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New(format!("@{to} and the brain")), names: vec![to.into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
            let n = core.tickets.m.tickets[&t].last();
            assert_eq!(core.tickets.posted, Some((core.tickets.m.id(t), n)));
            n
        };
        let a = his(&mut core, "lead");
        let b = his(&mut core, "fixer");
        let posts: Vec<Value> = log().into_iter().filter(|l| l["ev"] == "post" && l["t"] == json!(t) && l["by"] == "owner").collect();
        assert_eq!(posts.iter().map(|l| l["hold"].clone()).collect::<Vec<_>>(), vec![Value::Null, json!(15)], "to lead in Step, to fixer in Run");
        assert!(a < b);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// His burst under the policy's hold (E1b; p590+p591, 20 Nov, 9 s apart): with `+lapse-after("gather", 15).` his
    /// typed post's line carries the hold, the held mark reads when it goes (the handler's `lapses_at`, his first post
    /// + 15 s), his second post joins the first, and when coordd's lapse round lets it go the holder reads both, each
    /// with its number, as one message; a Split by whole posts tells each part to its own agent. The answer is made up.
    #[test]
    fn his_burst_is_held_by_the_policy_and_the_holder_reads_it_numbered() {
        let (mut core, _rx, coord) = esc_coord("burst", &[("lead", "owner", ""), ("fixer", "owner", "")], Some("+lapse-after(\"gather\", 15).\n"), None);
        let v = json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": [], "text": "the build and the brain", "addressed": {"head": "Unnamed"}, "esc": []});
        core.ticket_input(Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        // his opening words held before intake (N-1104 p935/p936; I-1474): they go first, as their 15 s would let them
        opener_goes(&mut core, t);
        core.ticket_answer(docket_machine::WATCHER, t, json!({"head": "Thread"}), Text::New(String::new())).unwrap();
        core.ticket_input(Input::Post { t, by: "lead".into(), text: Text::New("Taken. @fixer the brain is yours".into()), names: vec!["fixer".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let his = |core: &mut Core, words: &str| {
            core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New(words.into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
            core.tickets.m.tickets[&t].last()
        };
        let a = his(&mut core, "the build: 36 fails on the runner, look at the lock");
        let b = his(&mut core, "and the brain: the ledger should mark p614 kept");
        let log = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap();
        let posts: Vec<Value> = log.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|l| l["ev"] == "post" && l["by"] == "owner" && l["t"] == json!(t)).collect();
        assert_eq!(posts.iter().map(|l| l["hold"].clone()).collect::<Vec<_>>(), vec![json!(15), json!(15)], "{log}");
        // one handler holds both; the held mark's "goes at": since his first post + 15 s
        let view = core.ticket_shown(t).unwrap();
        let held: Vec<&docket_machine::TicketView> = view.handlers.iter().filter(|h| h.lapses_at.is_some()).collect();
        assert_eq!(held.len(), 1, "{:?}", view.handlers.iter().map(|h| (&h.id, &h.lapses_at)).collect::<Vec<_>>());
        let h = ticket_of(&core.tickets.m, &held[0].id).unwrap();
        let docket_machine::Status::Awaits { since, owed, .. } = &core.tickets.m.tickets[&h].status else { panic!() };
        assert_eq!(owed.call, "gather");
        let goes = chrono::DateTime::parse_from_rfc3339(since).unwrap().to_utc() + chrono::Duration::seconds(15);
        assert_eq!(held[0].lapses_at.as_deref(), Some(goes.format("%Y-%m-%dT%H:%M:%SZ").to_string().as_str()));
        // what follows from it (proto-reader N-1446 p136, proto-lead p140): held, it is not his to answer. No `owner` in
        // the thread's or the handler's `waits_on`, neither is owed, and the thread's outline line says when it goes
        // through, on his clock, in place of "waits on you"
        let owner_waits = |v: &docket_machine::TicketView| v.waits_on.iter().filter(|w| w.agent == "owner").count();
        assert_eq!((owner_waits(&view), view.owed), (0, false), "{:?}", view.waits_on);
        assert_eq!((owner_waits(held[0]), held[0].owed), (0, false), "{:?}", held[0].waits_on);
        let line = view.outline.as_ref().unwrap();
        assert_eq!((line.state.clone(), line.owed), (format!("goes through at {}", crate::core::his_clock(goes, &chrono::Local)), false));
        // under slow mode nothing goes by itself: no `lapses_at`, and the step stays his, as before
        let held_id = held[0].id.clone();
        core.ticket_input(Input::Slow { t, on: true, by: "owner".into() }).unwrap();
        let slow = core.ticket_shown(t).unwrap();
        let hv = slow.handlers.iter().find(|v| v.id == held_id).unwrap();
        assert_eq!((slow.lapses_at.clone(), hv.lapses_at.clone()), (None, None));
        assert_eq!((owner_waits(&slow), slow.owed, owner_waits(hv), hv.owed), (1, true, 1, true), "{:?} {:?}", slow.waits_on, hv.waits_on);
        assert_eq!((slow.outline.as_ref().unwrap().state.as_str(), slow.outline.as_ref().unwrap().owed), ("waits on you", true));
        core.ticket_input(Input::Slow { t, on: false, by: "owner".into() }).unwrap();
        let routes = |core: &Core| -> Vec<docket_machine::Delivery> { core.ticket_due(docket_machine::WATCHER).into_iter().filter(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "route")).collect() };
        assert!(routes(&core).is_empty(), "held: the holder is not asked yet");
        // 14 s on, still held; 15 s on, it goes by itself
        core.lapse_round(goes - chrono::Duration::seconds(1));
        assert!(routes(&core).is_empty());
        core.lapse_round(goes);
        let d = routes(&core).pop().expect("the holder is asked once the hold has passed");
        let job = core.clerk_job(&d).unwrap();
        let r = job.route.as_ref().unwrap();
        assert_eq!(job.text, format!("#{a}: the build: 36 fails on the runner, look at the lock\n#{b}: and the brain: the ledger should mark p614 kept"));
        assert_eq!(r.burst, vec![a, b]);
        let split = json!({"head": "Split", "parts": [{"posts": [a], "to": ["lead"]}, {"posts": [b], "to": ["fixer"]}]});
        core.ticket_answer(docket_machine::WATCHER, d.t, split, Text::New(String::new())).unwrap();
        let x = &core.tickets.m.tickets[&t];
        let told = |n: u32| x.posts.iter().find(|p| p.n == n).unwrap().told.clone();
        assert_eq!((told(a), told(b)), (vec!["lead".to_string()], vec!["fixer".to_string()]));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// His own @NAME in a burst (E1b hold 1; consult-3 N-1446 p281, proto-reader p307, proto-lead p325; made up on
    /// p590+p591): his first post names nobody, his second is "@docket-core look at the lock". The burst goes to the
    /// holder, not to docket-core as a whole; the holder reads which post he chose for, and only an answer that sends
    /// it to docket-core alone is taken. Should the call fail, the fallback splits by his choice: his first post to the
    /// agent the thread is for, his second to docket-core.
    #[test]
    fn his_own_name_in_a_burst_sends_that_post_where_he_chose_and_no_further() {
        let (mut core, _rx, coord) = esc_coord("chosen", &[("paper-research", "owner", ""), ("docket-core", "owner", "")], Some("+lapse-after(\"gather\", 15).\n"), None);
        let v = json!({"from": "owner", "to": "paper-research", "typed": ["paper-research"], "names": [], "text": "the special actions", "addressed": {"head": "Unnamed"}, "esc": []});
        core.ticket_input(Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        // his opening words held before intake (N-1104 p935/p936; I-1474): they go first, as their 15 s would let them
        opener_goes(&mut core, t);
        core.ticket_answer(docket_machine::WATCHER, t, json!({"head": "Thread"}), Text::New(String::new())).unwrap();
        core.ticket_input(Input::Post { t, by: "paper-research".into(), text: Text::New("Taken. @docket-core the lock is yours".into()), names: vec!["docket-core".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let his = |core: &mut Core, words: &str, names: &[&str]| {
            core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New(words.into()), names: names.iter().map(|a| a.to_string()).collect(), told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
            core.tickets.m.tickets[&t].last()
        };
        let a = his(&mut core, "the median on the dashboard looks off", &[]);
        let b = his(&mut core, "@docket-core look at the lock", &["docket-core"]);
        let h = core.tickets.m.tickets.values().find(|x| x.post_of.as_ref().is_some_and(|p| p.t == t && p.n == a)).map(|x| x.t).unwrap();
        assert_eq!(core.tickets.m.tickets[&h].post_of.as_ref().unwrap().posts(), vec![a, b], "one burst");
        let docket_machine::Status::Awaits { since, .. } = &core.tickets.m.tickets[&h].status else { panic!() };
        core.lapse_round(chrono::DateTime::parse_from_rfc3339(since).unwrap().to_utc() + chrono::Duration::seconds(15));
        // not To(docket-core) for both: the holder is asked
        let d = core.ticket_due(docket_machine::WATCHER).into_iter().find(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "route")).expect("the holder decides a burst with his own @NAME in one post");
        let job = core.clerk_job(&d).unwrap();
        let r = job.route.as_ref().unwrap();
        assert_eq!((r.burst.clone(), r.chosen.clone(), r.to.clone(), r.fallback.clone()), (vec![a, b], vec![(b, vec!["docket-core".to_string()])], Vec::<String>::new(), vec!["paper-research".to_string()]));
        assert_eq!(job.text, format!("#{a}: the median on the dashboard looks off\n#{b} (his own choice: it goes to docket-core and no one else): @docket-core look at the lock"));
        let part = |n: u32, to: &str| json!({"posts": [n], "to": [to]});
        let split = json!({"head": "Split", "parts": [part(a, "paper-research"), part(b, "docket-core")]});
        assert!(!crate::clerk::route_ok(&json!({"head": "To", "agents": ["paper-research"]}), r));
        assert!(!crate::clerk::route_ok(&json!({"head": "To", "agents": ["paper-research", "docket-core"]}), r));
        assert!(crate::clerk::route_ok(&split, r));
        assert_eq!(crate::clerk::route_failed(r), split);
        core.ticket_answer(docket_machine::WATCHER, d.t, split, Text::New(String::new())).unwrap();
        let x = &core.tickets.m.tickets[&t];
        let told = |n: u32| x.posts.iter().find(|p| p.n == n).unwrap().told.clone();
        assert_eq!((told(a), told(b)), (vec!["paper-research".to_string()], vec!["docket-core".to_string()]));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// His burst held while he types (E1b; the 15 s his, N-1445 p9; the 60 s proto-engine's, with paper-research's yes
    /// at N-1446 p40): with `lapse-at-most` 60 beside `lapse-after` 15, his writing in the thread's box (docket-core4's
    /// `writing`) holds his burst past 15 s; it goes once he stops, and a draft left open still lets it go at 60 s from
    /// his first post. The held mark and the thread's line say the time it goes at as it moves. The answers are made
    /// up.
    #[test]
    fn his_writing_holds_his_burst_up_to_the_ceiling() {
        let policy = "+lapse-after(\"gather\", 15).\n+lapse-at-most(\"gather\", 60).\n";
        let (mut core, _rx, coord) = esc_coord("writing", &[("lead", "owner", ""), ("fixer", "owner", "")], Some(policy), None);
        let v = json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": [], "text": "the build and the brain", "addressed": {"head": "Unnamed"}, "esc": []});
        core.ticket_input(Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        // his opening words held before intake (N-1104 p935/p936; I-1474): they go first, as their 15 s would let them
        opener_goes(&mut core, t);
        core.ticket_answer(docket_machine::WATCHER, t, json!({"head": "Thread"}), Text::New(String::new())).unwrap();
        // two agents on it, so the holder decides
        core.ticket_input(Input::Post { t, by: "lead".into(), text: Text::New("Taken. @fixer the brain is yours".into()), names: vec!["fixer".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let his = |core: &mut Core, words: &str| {
            core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New(words.into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
            core.tickets.m.tickets[&t].last()
        };
        let writing = |core: &mut Core, on: bool| core.ticket_input(Input::Event { name: docket_machine::syntax::WRITING.into(), value: json!({"t": t, "on": on}) }).unwrap();
        let routes = |core: &Core| core.ticket_due(docket_machine::WATCHER).into_iter().filter(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "route")).count();
        // the held handler of his post N, when its hold began, and when the view says it goes
        let held = |core: &Core, n: u32| -> (chrono::DateTime<chrono::Utc>, Option<String>, String) {
            let h = core.tickets.m.tickets.values().find(|x| x.post_of.as_ref().is_some_and(|p| p.t == t && p.posts().contains(&n))).map(|x| x.t).unwrap();
            let docket_machine::Status::Awaits { since, .. } = &core.tickets.m.tickets[&h].status else { panic!("{:?}", core.tickets.m.tickets[&h].status) };
            let view = core.ticket_shown(t).unwrap();
            let hv = view.handlers.iter().find(|v| v.id == core.tickets.m.id(h)).unwrap();
            (chrono::DateTime::parse_from_rfc3339(since).unwrap().to_utc(), hv.lapses_at.clone(), view.outline.unwrap().state)
        };
        let at = |t: chrono::DateTime<chrono::Utc>| Some(t.format("%Y-%m-%dT%H:%M:%SZ").to_string());
        let line = |t: chrono::DateTime<chrono::Utc>| format!("goes through at {}", crate::core::his_clock(t, &chrono::Local));
        let secs = chrono::Duration::seconds;
        // his first post, and he goes on typing in the box: held to 60 s from it, not 15
        let a = his(&mut core, "the build: 36 fails on the runner, look at the lock");
        let (since, _, _) = held(&core, a);
        assert_eq!(held(&core, a).1, at(since + secs(15)), "not writing: 15 s");
        writing(&mut core, true);
        assert_eq!(held(&core, a), (since, at(since + secs(60)), line(since + secs(60))));
        core.lapse_round(since + secs(15));
        core.lapse_round(since + secs(59));
        assert_eq!(routes(&core), 0, "held while he types");
        // his second post joins it; a round between his post and his `off` (the window sends the post first: consult-3
        // N-1446 p309, docket-core4 p411) leaves it held; he stops typing, and it goes at the next round, both as one
        let b = his(&mut core, "and the brain: the ledger should mark p614 kept");
        core.lapse_round(since + secs(59));
        assert_eq!(routes(&core), 0, "his post in, his writing still on: held");
        writing(&mut core, false);
        assert_eq!(held(&core, a).1, at(since + secs(15)), "he stopped: back to 15 s, long past");
        core.lapse_round(since + secs(59));
        assert_eq!(routes(&core), 1);
        let d = core.ticket_due(docket_machine::WATCHER).into_iter().find(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "route")).unwrap();
        assert_eq!(core.clerk_job(&d).unwrap().route.unwrap().burst, vec![a, b]);
        core.ticket_answer(docket_machine::WATCHER, d.t, json!({"head": "To", "agents": ["lead"]}), Text::New(String::new())).unwrap();
        // a draft left open with no keystroke: it still goes at the ceiling, 60 s from his post
        let c = his(&mut core, "one more on the lock");
        writing(&mut core, true);
        let (since, _, _) = held(&core, c);
        core.lapse_round(since + secs(59));
        assert_eq!(routes(&core), 0);
        core.lapse_round(since + secs(60));
        assert_eq!(routes(&core), 1, "the ceiling");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// His opening words on a new thread are gathered before intake sorts them (N-1104 p935/p936; I-1474; N-1445 p9:
    /// "hold them for 15s by default"): under `+lapse-after("gather", 15).` coordd writes the hold on his message's
    /// line, the thread waits at `gather` before `intake` is asked, nobody is handed it, its `lapses_at` and outline
    /// line say when it goes (since + 15 s), his next message to the same agent in the 15 s joins it, and the lapse
    /// round lets the burst go to `intake` once and then to lead; a restart replays the record as it ran and writes
    /// nothing. The words are made up.
    #[test]
    fn his_opening_words_are_held_by_the_policy_as_his_typed_post_is() {
        let (mut core, _rx, coord) = esc_coord("opener", &[("lead", "owner", ""), ("fixer", "owner", "")], Some("+lapse-after(\"gather\", 15).\n"), None);
        let v = json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": [], "text": "the lock on the runner fails again", "addressed": {"head": "Unnamed"}, "esc": []});
        core.ticket_input(Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        let path = coord.join("tickets/log.jsonl");
        let log = std::fs::read_to_string(&path).unwrap();
        let message = log.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).find(|l| l["ev"] == "event" && l["name"] == "owner_message").expect("his message's line");
        assert_eq!(message["value"]["hold"], json!(15), "{message}");
        // the thread is held at its own `gather` step before `intake`: given to nobody, going at since + 15 s
        let view = core.ticket_shown(t).unwrap();
        let docket_machine::Status::Awaits { since, owed, .. } = &core.tickets.m.tickets[&t].status else { panic!("{:?}", core.tickets.m.tickets[&t].status) };
        assert_eq!(owed.call, docket_machine::syntax::GATHER);
        let goes = chrono::DateTime::parse_from_rfc3339(since).unwrap().to_utc() + chrono::Duration::seconds(15);
        assert_eq!(view.lapses_at.as_deref(), Some(goes.format("%Y-%m-%dT%H:%M:%SZ").to_string().as_str()));
        assert_eq!(view.outline.as_ref().unwrap().state, format!("goes through at {}", crate::core::his_clock(goes, &chrono::Local)));
        let lead = |core: &Core| core.ticket_due("lead").into_iter().filter(|d| d.t == t).count();
        let intakes = |core: &Core| core.ticket_due(docket_machine::WATCHER).into_iter().filter(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "intake")).count();
        assert_eq!((intakes(&core), lead(&core)), (0, 0), "held: neither intake nor lead is handed anything yet");
        // his second message in the 15 s joins the burst on T: no second thread, still no intake
        let v2 = json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": [], "text": "and the window build too", "addressed": {"head": "Unnamed"}, "esc": []});
        core.ticket_input(Input::Event { name: "owner_message".into(), value: v2 }).unwrap();
        assert_eq!(*core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0, t);
        assert_eq!((intakes(&core), lead(&core)), (0, 0));
        // the hold's end lets the burst go to intake once, and on Thread to lead with no second hold
        core.lapse_round(goes);
        assert_eq!(intakes(&core), 1);
        core.ticket_answer(docket_machine::WATCHER, t, json!({"head": "Thread"}), Text::New(String::new())).unwrap();
        let n = core.tickets.m.tickets[&t].last();
        assert_eq!(lead(&core), 1);
        assert!(core.tickets.m.tickets[&t].posts.iter().find(|p| p.n == n).is_some_and(|p| p.held.is_none() && p.told == ["lead"] && p.text == Text::New("the lock on the runner fails again\nand the window build too".into())));
        // a restart replays it as it ran, and writes nothing of it (the start's own hand-over line aside)
        let before = std::fs::read_to_string(&path).unwrap().lines().count();
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let mut again = Core::load(&coord.display().to_string(), &base);
        again.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        again.load_tickets();
        let new: Vec<Value> = std::fs::read_to_string(&path).unwrap().lines().skip(before).map(|l| serde_json::from_str::<Value>(l).unwrap()).filter(|v| v["name"] != docket_machine::HELD_HANDED).collect();
        assert!(new.is_empty(), "{new:#?}");
        assert_eq!(again.tickets.m.view(t), core.tickets.m.view(t));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A coord for the esc tests: agents AGENTS (name, parent, role), each a Claude Code agent, the jobs' channel kept;
    /// a project with POLICY as its esc.eflint and KIND as a kind of its own, when given.
    fn esc_coord(tag: &str, agents: &[(&str, &str, &str)], policy: Option<&str>, kind: Option<&str>) -> (Core, tokio::sync::mpsc::UnboundedReceiver<crate::core::Job>, std::path::PathBuf) {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-esc-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let mut reg = String::new();
        for (i, (name, parent, role)) in agents.iter().enumerate() {
            reg += &format!("{{\"ev\":\"add\",\"id\":\"AGT-{i:03}\",\"name\":\"{name}\",\"parent\":\"{parent}\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}}\n");
            if !role.is_empty() {
                reg += &format!("{{\"ev\":\"role\",\"name\":\"{name}\",\"role\":\"{role}\",\"at\":\"x\"}}\n");
            }
        }
        std::fs::write(coord.join("registry.jsonl"), reg).unwrap();
        if policy.is_some() || kind.is_some() {
            let project = coord.join("files");
            std::fs::create_dir_all(project.join("policy")).unwrap();
            std::fs::create_dir_all(project.join("kinds")).unwrap();
            if let Some(p) = policy {
                std::fs::write(project.join("policy/esc.eflint"), p).unwrap();
            }
            if let Some(k) = kind {
                std::fs::write(project.join("kinds/work.kind"), k).unwrap();
            }
            std::fs::write(project.join("project.toml"), "[project]\nname = \"e\"\n").unwrap();
            std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
        }
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        core.load_tickets();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        (core, rx, coord)
    }

    /// His opening words on thread T, held while the policy holds his posts (I-1474), let go as the hold's end lets
    /// them: coordd's lapse round at their handler's `lapses_at`. A test of his later posts starts after them, as when
    /// his opening words went at once; nothing is held without such a policy, and nothing is done.
    pub(super) fn opener_goes(core: &mut Core, t: u64) {
        let Some(at) = core.ticket_shown(t).and_then(|v| v.lapses_at.or_else(|| v.handlers.iter().find_map(|h| h.lapses_at.clone()))) else { return };
        core.lapse_round(chrono::DateTime::parse_from_rfc3339(&at).unwrap().to_utc());
        let v = core.ticket_shown(t).unwrap();
        assert!(v.lapses_at.is_none() && v.handlers.iter().all(|h| h.lapses_at.is_none()), "his opening words went");
    }

    /// How many Escapes (Job::Interrupt) went out since the last look.
    fn interrupts(rx: &mut tokio::sync::mpsc::UnboundedReceiver<crate::core::Job>) -> usize {
        let mut n = 0;
        while let Ok(j) = rx.try_recv() {
            n += usize::from(matches!(j, crate::core::Job::Interrupt { .. }));
        }
        n
    }

    fn working(core: &mut Core, name: &str) {
        core.agents.get_mut(name).unwrap().live = crate::core::Live { pid: Some(std::process::id() as i32), state: "working".into(), turn: 1, ..Default::default() };
    }

    /// `coord can` (paper-research N-568 p15, part 6): an agent's acts as the base norms stand, enabled or greyed with
    /// why; at a ticket, the thread field is the ticket; at one or more identifiers (N-604.p3), the acts naming any;
    /// the owner names the agent; the refusals.
    #[test]
    fn can_lists_an_agents_acts_enabled_and_greyed() {
        let (mut core, _rx, coord) = esc_coord("can", &[("orch", "owner", "lead"), ("dev", "orch", "member"), ("sub", "dev", "member")], None, None);
        let acts = |r: Reply| -> Vec<coord_types::ActAnswer> {
            assert!(r.ok, "{}", r.out);
            r.out.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
        };
        let orch = acts(core.enabled(&Who::Agent("orch".into()), None, None));
        assert!(orch.iter().any(|a| a.act == "esc" && a.objects == ["dev"] && a.enabled), "{orch:?}");
        let dev = acts(core.enabled(&Who::Owner, None, Some("dev")));
        assert!(!dev.iter().any(|a| a.act == "esc" && a.objects == ["orch"] && a.enabled), "{dev:?}");
        assert!(dev.iter().filter(|a| !a.enabled).all(|a| a.why.is_some()), "a greyed act says why: {dev:?}");
        // at a ticket: no object is the situation's "t"
        let r = core.ticket_open(&Who::Agent("dev".into()), "question".into(), None, json!({"ask": "x?", "context": ""}), None, Said::default());
        let t = r.out.trim_start_matches("Opened ").trim_end_matches('.').to_string();
        assert!(r.ok && t.contains('-'), "{}", r.out);
        let at = acts(core.enabled(&Who::Agent("dev".into()), Some(&t), None));
        assert!(!at.is_empty() && at.iter().all(|a| !a.objects.iter().any(|o| o == "t")), "{at:?}");
        let no = |r: Reply| if r.ok { panic!("not refused: {}", r.out) } else { r.out };
        assert_eq!(no(core.enabled(&Who::Agent("dev".into()), None, Some("orch"))), "coord can: an agent asks for its own acts, not orch's");
        assert_eq!(no(core.enabled(&Who::Owner, None, None)), "coord can: the owner is bound by no act of the norms; name an agent (--as NAME) to see its acts");
        assert_eq!(no(core.enabled(&Who::Owner, None, Some("nobody"))), "coord can: no agent named nobody");
        assert_eq!(no(core.enabled(&Who::Agent("dev".into()), Some("N-999"), None)), "coord can: no ticket N-999");
        // at an agent's row: only the acts that name it; at several (a team's row, as the window draws it): those naming any
        let at_dev = acts(core.enabled(&Who::Agent("orch".into()), Some("dev"), None));
        assert!(at_dev.iter().any(|a| a.act == "esc" && a.objects == ["dev"] && a.enabled), "{at_dev:?}");
        assert!(at_dev.iter().all(|a| a.objects.iter().any(|o| o == "dev")), "{at_dev:?}");
        assert!(at_dev.len() < orch.len());
        let at_team = acts(core.enabled(&Who::Agent("orch".into()), Some("dev, sub"), None));
        assert!(at_team.iter().all(|a| a.objects.iter().any(|o| o == "dev" || o == "sub")), "{at_team:?}");
        assert!(at_team.iter().any(|a| a.act == "esc" && a.objects == ["sub"] && a.enabled), "{at_team:?}");
        assert!(at_team.len() > at_dev.len());
        // no `team:` word: an identifier no act names answers nothing
        assert!(acts(core.enabled(&Who::Agent("orch".into()), Some("team:dev"), None)).is_empty());
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A greyed act says the rule's own words (`ActAnswer::says`, docket-edge6 N-747): the comment above the first
    /// `Conditioned by` line that fails, or the act's own comment when that line has none; an enabled one says nothing.
    #[test]
    fn a_greyed_act_says_the_rules_own_words() {
        let esc = "// A lead escalates to an agent below it.\nAct esc\n  Actor agent\n  Recipient agent\n  Holds when below(recipient, actor)\n  // Only a lead escalates.\n  Conditioned by role(actor, \"lead\")\n  Conditioned by recipient != \"sub\"\n";
        let (core, _rx, coord) = esc_coord("says", &[("orch", "owner", "lead"), ("dev", "orch", "member"), ("sub", "dev", "member")], Some(esc), None);
        let acts = |r: Reply| -> Vec<coord_types::ActAnswer> {
            assert!(r.ok, "{}", r.out);
            r.out.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
        };
        let one = |all: &[coord_types::ActAnswer], to: &str| all.iter().find(|a| a.act == "esc" && a.objects == [to]).cloned().unwrap_or_else(|| panic!("no esc {to}: {all:?}"));
        let dev = acts(core.enabled(&Who::Agent("dev".into()), None, None));
        let e = one(&dev, "sub");
        assert!(!e.enabled && e.why.is_some(), "{e:?}");
        assert_eq!(e.says.as_deref(), Some("Only a lead escalates."), "the first line that fails, its own comment");
        let orch = acts(core.enabled(&Who::Agent("orch".into()), None, None));
        let e = one(&orch, "dev");
        assert!(e.enabled && e.says.is_none(), "{e:?}");
        let e = one(&orch, "sub");
        assert!(!e.enabled, "{e:?}");
        assert_eq!(e.says.as_deref(), Some("A lead escalates to an agent below it."), "no comment above that line: the act's");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A duties ask does not hold the core's lock while the interpreter runs (consult-3 N-581.p19, paper-research
    /// N-581.p22): with the job held running, another request takes the lock and is answered; the duties' answer
    /// comes after, and is kept.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_duties_ask_does_not_hold_the_lock_while_the_interpreter_runs() {
        let esc = "Act esc\n  Actor agent\n  Recipient agent\n  Holds when below(recipient, actor)\n";
        let (core, _rx, coord) = esc_coord("duties-lock", &[("orch", "owner", "lead"), ("dev", "orch", "member")], Some(esc), None);
        std::fs::create_dir_all(coord.join("files/policy/eflint")).unwrap();
        std::fs::write(
            coord.join("files/policy/eflint/owe.eflint"),
            "Act greet\n  Actor agent1\n  Recipient agent2\n  Holds when role(agent1, \"member\").\nDuty greet-lead\n  Holder agent1\n  Claimant agent2\n  Holds when role(agent1, \"member\") && role(agent2, \"lead\")\n  Violated when role(agent1, \"member\")\n  Terminated by greet.\n",
        )
        .unwrap();
        let shared = std::sync::Arc::new(crate::Shared::new(core));
        let (running, is_running) = tokio::sync::oneshot::channel::<()>();
        let (go_on, may_go_on) = std::sync::mpsc::channel::<()>();
        let s2 = shared.clone();
        let asked = tokio::spawn(async move {
            crate::duties_off_lock(&s2, &Who::Agent("dev".into()), None, move |job| {
                running.send(()).unwrap();
                may_go_on.recv().unwrap();
                let got = job.run();
                (job, got)
            })
            .await
        });
        is_running.await.unwrap();
        // the interpreter's job is running now: the lock is free, and another request is answered
        let other = shared.0.try_lock().expect("the core's lock is held while the interpreter runs");
        let r = other.enabled(&Who::Agent("orch".into()), None, None);
        assert!(r.ok, "{}", r.out);
        drop(other);
        go_on.send(()).unwrap();
        let r = asked.await.unwrap();
        assert!(r.ok && r.out.contains("greet-lead"), "{}", r.out);
        // kept: asked again with nothing changed, answered under the lock at once
        let again = shared.0.lock().await.duties_ask(&Who::Agent("dev".into()), None);
        assert!(matches!(again, crate::core::DutiesAsk::Done(r) if r.ok && r.out.contains("greet-lead")));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// `coord duties` (N-568 part 6): the duties that hold now, from the interpreter: an agent's own, the owner's of
    /// one agent or all, each with holder, claimant, violated and the acts that end it; another agent's refused.
    #[test]
    fn duties_lists_what_the_interpreter_says_is_owed() {
        let esc = "Act esc\n  Actor agent\n  Recipient agent\n  Holds when below(recipient, actor)\n";
        let (core, _rx, coord) = esc_coord("duties", &[("orch", "owner", "lead"), ("dev", "orch", "member")], Some(esc), None);
        std::fs::create_dir_all(coord.join("files/policy/eflint")).unwrap();
        std::fs::write(
            coord.join("files/policy/eflint/owe.eflint"),
            "Act greet\n  Actor agent1\n  Recipient agent2\n  Holds when role(agent1, \"member\").\nDuty greet-lead\n  Holder agent1\n  Claimant agent2\n  Holds when role(agent1, \"member\") && role(agent2, \"lead\")\n  Violated when role(agent1, \"member\")\n  Terminated by greet.\n",
        )
        .unwrap();
        let duties = |r: Reply| -> Vec<coord_types::DutyAnswer> {
            assert!(r.ok, "{}", r.out);
            r.out.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
        };
        let dev = duties(core.duties(&Who::Agent("dev".into()), None));
        assert_eq!(dev.len(), 1, "{dev:?}");
        let d = &dev[0];
        assert_eq!((d.duty.as_str(), d.holder.as_deref(), d.claimant.as_deref(), d.violated, d.ended_by.clone()), ("greet-lead", Some("dev"), Some("orch"), true, vec!["greet".to_string()]));
        assert!(duties(core.duties(&Who::Agent("orch".into()), None)).is_empty(), "orch holds none");
        assert_eq!(duties(core.duties(&Who::Owner, None)), dev, "the owner: all");
        assert!(duties(core.duties(&Who::Owner, Some("orch"))).is_empty());
        let r = core.duties(&Who::Agent("dev".into()), Some("orch"));
        assert!(!r.ok && r.out == "coord duties: an agent asks for its own duties, not orch's", "{}", r.out);
        // off the lock (consult-3, N-581.p19): the job runs without the core, its answer is kept, and asked again on
        // the same facts and files it is given at once, each asker's own
        use crate::core::DutiesAsk;
        let job = match core.duties_ask(&Who::Owner, None) {
            DutiesAsk::Run(j) => j,
            DutiesAsk::Done(r) => panic!("nothing kept yet, answered: {}", r.out),
        };
        let list = job.run().unwrap();
        core.duties_keep(job, list);
        assert!(matches!(core.duties_ask(&Who::Owner, None), DutiesAsk::Done(r) if r.ok && r.out.contains("greet-lead")));
        assert!(matches!(core.duties_ask(&Who::Agent("orch".into()), None), DutiesAsk::Done(r) if r.ok && r.out.is_empty()));
        assert!(matches!(core.duties_ask(&Who::Agent("dev".into()), Some("orch")), DutiesAsk::Done(r) if !r.ok));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// `coord esc` (N-36, 1): it stops a running turn and does nothing else: no ticket, no post, one `esc` line;
    /// a second esc before the stop lands types nothing more.
    #[test]
    fn esc_alone_stops_a_running_turn_and_posts_nothing() {
        let (mut core, mut rx, coord) = esc_coord("alone", &[("w2", "owner", "")], None, None);
        working(&mut core, "w2");
        let log = coord.join("tickets/log.jsonl");
        let before = std::fs::read_to_string(&log).unwrap_or_default();
        let tickets = core.tickets.m.tickets.len();
        let r = core.ticket_esc(&Who::Owner, vec!["w2".into()]);
        assert!(r.ok && r.out == "w2's turn is being stopped", "{}", r.out);
        assert_eq!(interrupts(&mut rx), 1);
        assert_eq!(core.tickets.m.tickets.len(), tickets, "no ticket opened");
        let after = std::fs::read_to_string(&log).unwrap_or_default();
        let new: Vec<Value> = after[before.len()..].lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(new.len(), 1, "{new:?}");
        assert_eq!((new[0]["ev"].as_str(), new[0]["by"].as_str(), new[0]["agent"].as_str()), (Some("esc"), Some("owner"), Some("w2")));
        let r = core.ticket_esc(&Who::Owner, vec!["w2".into()]);
        assert!(r.ok && r.out.contains("already being stopped"), "{}", r.out);
        assert_eq!(interrupts(&mut rx), 0, "a second press stops nothing more");
        assert_eq!(std::fs::read_to_string(&log).unwrap_or_default(), after, "and writes nothing");
        // the whole log replays as written
        assert!(Tickets::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), None, 1).errors.is_empty());
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// The registry's last line, and the model agents/NAME/session.json names.
    fn last_record(coord: &std::path::Path, name: &str) -> (Value, String) {
        let reg = std::fs::read_to_string(coord.join("registry.jsonl")).unwrap();
        let last: Value = serde_json::from_str(reg.lines().last().unwrap()).unwrap();
        let session: Value = serde_json::from_slice(&std::fs::read(coord.join(format!("agents/{name}/session.json"))).unwrap_or_default()).unwrap_or(Value::Null);
        (last, session["model"].as_str().unwrap_or_default().to_string())
    }

    fn idle(core: &mut Core, name: &str, pid: i32) {
        core.agents.get_mut(name).unwrap().live = crate::core::Live { pid: Some(pid), state: "idle".into(), turn: 1, ..Default::default() };
    }

    /// N-39, 3: `coord effort` on an agent not in a turn: its record says MODEL:LEVEL at once (the registry's model
    /// line, session.json), and it is restarted on its own session now, once.
    #[test]
    fn effort_an_idle_agent_takes_it_now_and_its_record_says_it() {
        let (mut core, _rx, coord) = esc_coord("effort-idle", &[("w2", "owner", "")], None, None);
        let pid = std::process::id() as i32;
        idle(&mut core, "w2", pid);
        let r = core.effort(&Who::Owner, "w2", "low", None);
        assert!(r.ok && r.out == "w2: effort default → low; restarted on its session (its background tasks ended)", "{}", r.out);
        let (last, session) = last_record(&coord, "w2");
        assert_eq!((last["ev"].as_str(), last["name"].as_str(), last["model"].as_str()), (Some("model"), Some("w2"), Some("m:low")));
        assert_eq!(session, "m:low");
        assert_eq!(core.agents["w2"].model, "m:low");
        assert_eq!(core.to_restart(), vec![("w2".to_string(), Some(pid))]);
        assert!(core.to_restart().is_empty(), "one restart at a time");
        // its new process (`coord run`) starts without the mark; the restart's end leaves it alone
        idle(&mut core, "w2", 1);
        core.restarted("w2", Some(pid), "");
        assert!(core.to_restart().is_empty() && core.agents["w2"].live.effort_due.is_none());
        // a second change says the level before
        let r = core.effort(&Who::Owner, "w2", "max", None);
        assert!(r.ok && r.out == "w2: effort low → max; restarted on its session (its background tasks ended)", "{}", r.out);
        assert_eq!(core.agents["w2"].model, "m:max");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// `coord compact` (paper-research N-490 p8): the point is kept at once (the registry's `compact` line, session.json,
    /// which roster gives the next start), the running session is not restarted for it, and the view says the point
    /// it runs with and, while they differ, the one its next start takes; its status line gives the model's whole
    /// window; 0 is the coord's own again; out of bounds, a Codex agent, an agent not above it: refused. The view's
    /// new fields read back through `Event` as the window reads them (N-541).
    #[test]
    fn compact_is_kept_for_the_next_start_and_the_view_says_both() {
        let (mut core, _rx, coord) = esc_coord("compact", &[("w2", "owner", ""), ("w3", "owner", "")], None, None);
        let pid = std::process::id() as i32;
        core.new_process("w2", pid, false);
        core.set_context("w2", Some(coord_types::AgentContext { used: 100, window: Some(350_000), ..Default::default() }));
        let r = core.compact(&Who::Owner, "w2", 500_000);
        assert!(r.ok && r.out == "w2: compacts at the coord's → 500k from its next start", "{}", r.out);
        let (last, _) = last_record(&coord, "w2");
        assert_eq!((last["ev"].as_str(), last["name"].as_str(), last["tokens"].as_u64()), (Some("compact"), Some("w2"), Some(500_000)));
        let session: Value = serde_json::from_slice(&std::fs::read(coord.join("agents/w2/session.json")).unwrap()).unwrap();
        assert_eq!(session["compact"], json!(500_000));
        assert!(core.to_restart().is_empty(), "a running session is not restarted for it");
        let ctx = |core: &Core| core.view("w2").context.unwrap();
        // no terminal to type it into (none here): a restart takes it
        assert_eq!(ctx(&core), coord_types::AgentContext { used: 100, window: Some(350_000), max: None, next: Some(500_000), next_by: Some("restart".into()), ..Default::default() });
        // its status line names the model's whole window
        core.hook("w2", "statusline", &json!({"context_window": {"context_window_size": 1_000_000, "used_percentage": 1}}));
        assert_eq!(ctx(&core).max, Some(1_000_000));
        // read back as the window reads a line: an agent's event, and a snapshot holding it
        for body in [Body::Agent(core.view("w2")), Body::Snapshot(coord_types::Snapshot { agents: vec![core.view("w2")], ..Default::default() })] {
            let line = serde_json::to_string(&coord_types::Event { seq: 7, body }).unwrap();
            let back: coord_types::Event = serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line}"));
            let view = match back.body { Body::Agent(v) => v, Body::Snapshot(s) => s.agents[0].clone(), _ => panic!("{line}") };
            assert_eq!(view.context, Some(coord_types::AgentContext { used: 100, window: Some(350_000), max: Some(1_000_000), next: Some(500_000), next_by: Some("restart".into()), ..Default::default() }));
        }
        // its next start takes it: no next any more
        core.new_process("w2", pid, false);
        assert_eq!((ctx(&core).window, ctx(&core).next), (Some(500_000), None));
        let r = core.compact(&Who::Owner, "w2", 500_000);
        assert!(r.ok && r.out == "w2 already compacts at 500k from its next start", "{}", r.out);
        // back to the coord's own
        let r = core.compact(&Who::Owner, "w2", 0);
        assert!(r.ok && r.out == "w2: compacts at 500k → the coord's from its next start", "{}", r.out);
        assert_eq!((ctx(&core).window, ctx(&core).next), (Some(500_000), Some(350_000)));
        let session: Value = serde_json::from_slice(&std::fs::read(coord.join("agents/w2/session.json")).unwrap()).unwrap();
        assert!(session.get("compact").is_none(), "{session}");
        // set back to what it runs at
        let r = core.compact(&Who::Owner, "w2", 500_000);
        assert!(r.ok && r.out == "w2: compacts at the coord's → 500k; its session runs at it already", "{}", r.out);
        assert_eq!(ctx(&core).next, None);
        // refusals
        for (who, name, tokens) in [(Who::Owner, "w2", 99_999), (Who::Owner, "w2", 1_000_001), (Who::Agent("w3".into()), "w2", 200_000), (Who::Owner, "nobody", 200_000)] {
            assert!(!core.compact(&who, name, tokens).ok, "{name} {tokens}");
        }
        assert_eq!(core.agents["w2"].compact, Some(500_000));
        // `/autocompact` typed in its session (its transcript, `follow::local`; the owner, N-541 post 34): the point is
        // its own from then on and the session runs at it at once; one typed before the point was last set here is
        // not taken again (the transcript read again after a coordd restart); a session another setting holds stays
        // where it is and takes the point at its next start; `auto` leaves it with no point of its own
        use crate::follow::Typed;
        core.compact_typed("w2", Typed { asked: Some(600_000), held_at: None }, "2000-01-01T00:00:00.000Z");
        assert_eq!(core.agents["w2"].compact, Some(500_000), "typed before the point was set here");
        core.compact_typed("w2", Typed { asked: Some(600_000), held_at: None }, "2099-01-01T00:00:00.000Z");
        assert_eq!((ctx(&core).window, ctx(&core).next), (Some(600_000), None));
        let (last, _) = last_record(&coord, "w2");
        assert_eq!((last["ev"].as_str(), last["tokens"].as_u64(), last["by"].as_str(), last["at"].as_str()), (Some("compact"), Some(600_000), Some("session"), Some("2099-01-01T00:00:00Z")));
        let session: Value = serde_json::from_slice(&std::fs::read(coord.join("agents/w2/session.json")).unwrap()).unwrap();
        assert_eq!(session["compact"], json!(600_000));
        core.compact_typed("w2", Typed { asked: Some(700_000), held_at: Some(350_000) }, "2099-01-02T00:00:00.000Z");
        assert_eq!((ctx(&core).window, ctx(&core).next), (Some(350_000), Some(700_000)));
        core.compact_typed("w2", Typed { asked: None, held_at: None }, "2099-01-03T00:00:00.000Z");
        assert_eq!((core.agents["w2"].compact, ctx(&core).window, ctx(&core).next), (None, Some(1_000_000), Some(350_000)), "auto: Claude Code's own now, the coord's at its next start");
        assert!(core.compact(&Who::Owner, "w2", 500_000).ok);
        // a Codex agent's point is kept the same way (roster gives it as model_auto_compact_token_limit; the owner,
        // N-541.p28): its view's window is none until it has one, never the coord's Claude setting; a point above
        // its model's window, as its session's file gave it, is refused; another kind's is not kept
        core.agents.get_mut("w3").unwrap().kind = "codex".into();
        core.set_context("w3", Some(coord_types::AgentContext { used: 100, max: Some(272_000), ..Default::default() }));
        let view = |core: &Core| core.view("w3").context.unwrap();
        assert_eq!(view(&core), coord_types::AgentContext { used: 100, window: None, max: Some(272_000), next: None, ..Default::default() });
        let r = core.compact(&Who::Owner, "w3", 300_000);
        assert!(!r.ok && r.out.contains("from 100k to 272k tokens, or 0 for Codex's own"), "{}", r.out);
        let r = core.compact(&Who::Owner, "w3", 200_000);
        assert!(r.ok && r.out == "w3: compacts at Codex's own → 200k from its next start", "{}", r.out);
        assert_eq!(view(&core), coord_types::AgentContext { used: 100, window: Some(200_000), max: Some(272_000), next: None, ..Default::default() });
        let session: Value = serde_json::from_slice(&std::fs::read(coord.join("agents/w3/session.json")).unwrap()).unwrap();
        assert_eq!(session["compact"], json!(200_000));
        core.agents.get_mut("w3").unwrap().kind = "pi".into();
        assert!(!core.compact(&Who::Owner, "w3", 150_000).ok);
        // kept over a coordd restart, and a stopped agent's view says the point its next start takes
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let mut again = Core::load(&coord.display().to_string(), &base);
        assert_eq!(again.agents["w2"].compact, Some(500_000));
        again.agents.get_mut("w2").unwrap().live.pid = None;
        again.set_context("w2", Some(coord_types::AgentContext { used: 100, window: Some(350_000), ..Default::default() }));
        assert_eq!((ctx(&again).window, ctx(&again).next), (Some(500_000), None));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A point set for a running Claude Code agent reaches its session at once (N-804.p16): its terminal's holder is
    /// given `/autocompact N` (`Job::Command`, typed once no turn runs and its input box is empty), the view says so
    /// (`next_by: typing`) until the session's answer in its record, which is what it runs at; an answer that another
    /// window holds the session (started before E6) leaves a restart to apply it. A Codex agent takes it at its next
    /// start; the agent itself and those below it are refused.
    #[test]
    fn a_point_set_for_a_running_claude_session_is_typed_into_it_and_its_answer_holds() {
        let (mut core, mut rx, coord) = esc_coord("compact-typed", &[("w2", "owner", ""), ("w3", "w2", "")], None, None);
        let pid = std::process::id() as i32;
        core.new_process("w2", pid, false);
        core.set_context("w2", Some(coord_types::AgentContext { used: 100, window: Some(350_000), ..Default::default() }));
        let sock = coord_types::pty::socket(&coord.display().to_string(), "w2");
        std::fs::create_dir_all(sock.parent().unwrap()).unwrap();
        std::fs::write(&sock, "").unwrap();
        let commands = |rx: &mut tokio::sync::mpsc::UnboundedReceiver<crate::core::Job>| {
            let mut out = Vec::new();
            while let Ok(j) = rx.try_recv() {
                if let crate::core::Job::Command { name, pty, line } = j {
                    out.push((name, pty, line));
                }
            }
            out
        };
        commands(&mut rx);
        let r = core.compact(&Who::Owner, "w2", 600_000);
        assert!(r.ok && r.out == "w2: compacts at the coord's → 600k; typed into its session as /autocompact once no turn runs and its input box is empty", "{}", r.out);
        assert_eq!(commands(&mut rx), vec![("w2".to_string(), sock.display().to_string(), "/autocompact 600000".to_string())]);
        let ctx = |core: &Core| core.view("w2").context.unwrap();
        assert_eq!((ctx(&core).window, ctx(&core).next, ctx(&core).next_by.as_deref()), (Some(350_000), Some(600_000), Some("typing")), "the line shows at the new place meanwhile");
        // its answer, as Claude Code writes it in the record ("Auto-compact window for Opus 5.5 set to 600k tokens")
        use crate::follow::Typed;
        core.compact_typed("w2", Typed { asked: Some(600_000), held_at: None }, "2099-01-01T00:00:00.000Z");
        assert_eq!((ctx(&core).window, ctx(&core).next, ctx(&core).next_by), (Some(600_000), None, None));
        assert_eq!(core.agents["w2"].live.compact_typing, None);
        // a session another window holds ("…, but a higher-priority override is active (350k tokens)"): a restart
        assert!(core.compact(&Who::Owner, "w2", 700_000).ok);
        assert_eq!(commands(&mut rx).len(), 1);
        core.compact_typed("w2", Typed { asked: Some(700_000), held_at: Some(350_000) }, "2099-01-02T00:00:00.000Z");
        assert_eq!((ctx(&core).window, ctx(&core).next, ctx(&core).next_by.as_deref()), (Some(350_000), Some(700_000), Some("restart")));
        // the agent itself, and one below it, are refused; nothing is typed
        assert!(!core.compact(&Who::Agent("w2".into()), "w2", 400_000).ok, "not its own");
        assert!(!core.compact(&Who::Agent("w3".into()), "w2", 400_000).ok, "not from below");
        assert!(commands(&mut rx).is_empty());
        // above it, an agent may; a stopped agent's is kept for its start
        let r = core.compact(&Who::Agent("w2".into()), "w3", 400_000);
        assert!(r.ok && r.out.ends_with("from its next start"), "{}", r.out);
        // a running Codex agent: kept for its next start, nothing typed
        core.agents.get_mut("w3").unwrap().kind = "codex".into();
        core.new_process("w3", pid, false);
        core.set_context("w3", Some(coord_types::AgentContext { used: 100, max: Some(272_000), ..Default::default() }));
        std::fs::write(coord_types::pty::socket(&coord.display().to_string(), "w3"), "").unwrap();
        let r = core.compact(&Who::Owner, "w3", 200_000);
        assert!(r.ok && r.out.ends_with("from its next start"), "{}", r.out);
        assert!(commands(&mut rx).is_empty());
        assert_eq!(core.view("w3").context.unwrap().next_by.as_deref(), Some("start"));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// `coord model` (paper-research N-101 p1259 point 5, p1434), on effort's path: the record says MODEL:EFFORT at once
    /// and the idle session is restarted on it; set back before the restart, nothing restarts; with effort due as
    /// well, one restart takes both; a gpt-* model is refused.
    #[test]
    fn model_an_idle_agent_takes_it_on_efforts_path() {
        let (mut core, _rx, coord) = esc_coord("model-idle", &[("w2", "owner", "")], None, None);
        let pid = std::process::id() as i32;
        idle(&mut core, "w2", pid);
        assert!(!core.model(&Who::Owner, "w2", "gpt-6-luna").ok, "a Codex model is a new agent");
        assert!(!core.model(&Who::Owner, "w2", "m2:loud").ok, "an effort Claude Code does not take");
        let r = core.model(&Who::Owner, "w2", "m2");
        assert!(r.ok && r.out == "w2: model m → m2; restarted on its session (its background tasks ended)", "{}", r.out);
        assert_eq!(last_record(&coord, "w2").1, "m2");
        // set back before the restart: nothing restarts
        let r = core.model(&Who::Owner, "w2", "m");
        assert!(r.ok && r.out.ends_with("its session runs on m already, so it does not restart"), "{}", r.out);
        assert!(core.to_restart().is_empty() && core.agents["w2"].live.effort_due.is_none());
        // model and effort together: one restart, the effort kept
        assert!(core.effort(&Who::Owner, "w2", "low", None).ok);
        assert!(core.model(&Who::Owner, "w2", "m3").ok);
        assert_eq!(core.agents["w2"].model, "m3:low");
        assert_eq!(core.to_restart(), vec![("w2".to_string(), Some(pid))]);
        // the owner's Restart (N-804.p9): one restart with its record as it is; an agent below it is refused
        core.restarting.remove("w2");
        idle(&mut core, "w2", pid);
        assert!(!core.restart(&Who::Agent("w2".into()), "w2").ok, "not by itself");
        let r = core.restart(&Who::Owner, "w2");
        assert!(r.ok && r.out == "w2: restarted on its session (its background tasks ended)", "{}", r.out);
        assert_eq!(core.to_restart(), vec![("w2".to_string(), Some(pid))]);
        // MODEL:EFFORT in one request (N-804.p7): both said, one record line, one restart
        core.restarting.remove("w2");
        idle(&mut core, "w2", pid);
        let r = core.model(&Who::Owner, "w2", "m4:high");
        assert!(r.ok && r.out == "w2: model m3 → m4, effort low → high; restarted on its session (its background tasks ended)", "{}", r.out);
        assert_eq!(last_record(&coord, "w2").1, "m4:high");
        assert_eq!(core.to_restart(), vec![("w2".to_string(), Some(pid))]);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// An opencode agent (tool-scan, N-591 post 109) is a harness's session reached as a Codex agent is: the owner's
    /// post goes to it as `Job::Codex` through `opencode-rpc`, at its session and its coord folder, and an esc
    /// interrupts it through the same program; its model names its kind (`PROVIDER/MODEL`), which `coord model` refuses.
    #[test]
    fn an_opencode_agent_takes_mail_and_an_esc_through_opencode_rpc() {
        use crate::core::{harness, kind_of_model, rpc_bin, served};
        assert_eq!((kind_of_model("anthropic/claude-sonnet-5-5"), kind_of_model("gpt-6-luna"), kind_of_model("claude-opus-5-5")), ("opencode", "codex", "claude"));
        assert!(harness("opencode") && served("opencode") && served("codex") && !served("claude") && !harness("pi"));
        assert_eq!((rpc_bin("/b", "opencode"), rpc_bin("/b", "codex")), ("/b/bin/opencode-rpc".to_string(), "/b/bin/codex-rpc".to_string()));
        // an agy agent (N-1446 p2038) likewise, through agy-rpc; its model names its kind before any `/` does
        assert_eq!((kind_of_model("agy/gemini-3.8-flash-high"), rpc_bin("/b", "agy")), ("agy", "/b/bin/agy-rpc".to_string()));
        assert!(harness("agy") && served("agy"));
        let (mut core, mut rx, coord) = esc_coord("opencode", &[("w2", "owner", "")], None, None);
        let pid = std::process::id() as i32;
        {
            let a = core.agents.get_mut("w2").unwrap();
            a.kind = "opencode".into();
            a.model = "anthropic/claude-sonnet-5-5".into();
            a.session = json!({"session_id": "ses_1", "socket": "/c/agents/w2/opencode/coord"});
        }
        idle(&mut core, "w2", pid);
        let v = json!({"from": "owner", "to": "w2", "typed": ["w2"], "names": [], "text": "say hello", "addressed": {"head": "Unnamed"}});
        core.ticket_input(Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        core.ticket_answer(docket_machine::WATCHER, t, json!({"head": "Thread"}), Text::New(String::new())).unwrap();
        assert_eq!(core.to_hand_over(), vec!["w2".to_string()]);
        core.hand_over("w2");
        let mut sent = Vec::new();
        while let Ok(j) = rx.try_recv() {
            match j {
                crate::core::Job::Codex { rpc, session, socket, tickets, .. } => sent.push((rpc, session, socket, tickets.len())),
                crate::core::Job::Channel { .. } => panic!("an opencode agent has no channel"),
                _ => {}
            }
        }
        let rpc = format!("{}/bin/opencode-rpc", core.base);
        assert_eq!(sent, vec![(rpc.clone(), "ses_1".to_string(), "/c/agents/w2/opencode/coord".to_string(), 1)]);
        core.interrupt("w2");
        let mut esc = Vec::new();
        while let Ok(j) = rx.try_recv() {
            if let crate::core::Job::Interrupt { codex, .. } = j {
                esc.push(codex);
            }
        }
        assert_eq!(esc, vec![Some((rpc, "ses_1".to_string(), "/c/agents/w2/opencode/coord".to_string()))]);
        // its model is opencode's, set at its start; a Claude Code agent is never moved onto one by `coord model`
        let r = core.model(&Who::Owner, "w2", "anthropic/claude-opus-5-5");
        assert!(!r.ok && r.out.contains("is an opencode agent; its model is set at its start"), "{}", r.out);
        core.agents.get_mut("w2").unwrap().kind = "claude".into();
        let r = core.model(&Who::Owner, "w2", "anthropic/claude-sonnet-5-5");
        assert!(!r.ok && r.out.contains("no PROVIDER/MODEL"), "{}", r.out);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-39, 3 and post 4, 4: in a turn nothing is done until the turn ends; then the restart comes before the post
    /// written during the turn is handed: nothing is typed into the closing terminal, and the post is handed to the
    /// restarted session, once.
    #[test]
    fn effort_in_a_turn_waits_for_its_end_and_comes_before_what_waits() {
        let (mut core, mut rx, coord) = esc_coord("effort-turn", &[("w2", "owner", "")], None, None);
        let pid = std::process::id() as i32;
        working(&mut core, "w2");
        let v = json!({"from": "owner", "to": "w2", "typed": ["w2"], "names": [], "text": "use the median", "addressed": {"head": "Unnamed"}});
        core.ticket_input(Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        core.ticket_answer(docket_machine::WATCHER, t, json!({"head": "Thread"}), Text::New(String::new())).unwrap();
        let r = core.effort(&Who::Owner, "w2", "high", None);
        assert!(r.ok && r.out == "w2: effort default → high when its turn ends (it restarts on its session; its background tasks end)", "{}", r.out);
        assert_eq!(last_record(&coord, "w2").1, "m:high", "the record at once");
        assert!(core.to_restart().is_empty(), "no turn is stopped for it");
        assert!(core.to_hand_over().is_empty());
        // the turn ends: the restart, and nothing handed to the session that runs at the old level
        core.agents.get_mut("w2").unwrap().live.state = "idle".into();
        assert_eq!(core.to_restart(), vec![("w2".to_string(), Some(pid))]);
        assert!(core.to_hand_over().is_empty(), "the waiting post goes to the restarted session");
        core.hand_over("w2");
        let mut typed = Vec::new();
        while let Ok(j) = rx.try_recv() {
            if matches!(j, crate::core::Job::Channel { .. } | crate::core::Job::Codex { .. } | crate::core::Job::Interrupt { .. }) {
                typed.push(j.name().to_string());
            }
        }
        assert!(typed.is_empty(), "nothing typed into the closing terminal: {typed:?}");
        idle(&mut core, "w2", 1);
        core.restarted("w2", Some(pid), "");
        assert_eq!(core.to_hand_over(), vec!["w2".to_string()]);
        core.hand_over("w2");
        let mut handed = Vec::new();
        while let Ok(j) = rx.try_recv() {
            if let crate::core::Job::Channel { ids, tickets, .. } = j {
                assert_eq!(tickets.len(), 1, "the post written during the turn");
                handed.push(ids);
            }
        }
        assert_eq!(handed.len(), 1, "one hand-over to the restarted session");
        core.handed("w2", &handed[0], true, "channel");
        assert!(core.to_hand_over().is_empty(), "handed once");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-39, 3 and 6: the level outlives a restart of the agent and of coordd (the registry, session.json); a restart
    /// still due outlives coordd's (the journalled Live); one that failed is not tried again and the record stays.
    #[test]
    fn effort_a_restart_afterwards_keeps_the_level() {
        let (mut core, _rx, coord) = esc_coord("effort-keep", &[("w2", "owner", "")], None, None);
        let pid = std::process::id() as i32;
        idle(&mut core, "w2", pid);
        assert!(core.effort(&Who::Owner, "w2", "xhigh", None).ok);
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let again = Core::load(&coord.display().to_string(), &base);
        assert_eq!(again.agents["w2"].model, "m:xhigh");
        assert_eq!(last_record(&coord, "w2").1, "m:xhigh", "what roster restart reads");
        let live: crate::core::Live = serde_json::from_value(serde_json::to_value(&core.agents["w2"].live).unwrap()).unwrap();
        assert_eq!(live.effort_due.as_deref(), Some("m"), "journalled, with what the session runs on (no level set)");
        // roster restart failed: the old process runs on; the mark goes, the record stays
        assert_eq!(core.to_restart().len(), 1);
        core.restarted("w2", Some(pid), "roster restart failed: no terminal");
        assert!(core.agents["w2"].live.effort_due.is_none() && core.to_restart().is_empty());
        assert_eq!(core.agents["w2"].model, "m:xhigh");
        // a stopped agent: its record only
        core.agents.get_mut("w2").unwrap().live = Default::default();
        let r = core.effort(&Who::Owner, "w2", "low", None);
        assert!(r.ok && r.out == "w2: effort xhigh → low from its next start", "{}", r.out);
        assert!(core.agents["w2"].live.effort_due.is_none() && core.to_restart().is_empty());
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-39 post 4: a second `coord effort` while one is due replaces the due level and answers with the new pair;
    /// one back to the level the session runs at drops the restart.
    #[test]
    fn effort_a_second_change_while_one_is_due() {
        let (mut core, _rx, coord) = esc_coord("effort-again", &[("w2", "owner", "")], None, None);
        let pid = std::process::id() as i32;
        core.agents.get_mut("w2").unwrap().model = "m:low".into();
        working(&mut core, "w2");
        assert!(core.effort(&Who::Owner, "w2", "high", None).ok);
        let r = core.effort(&Who::Owner, "w2", "max", None);
        assert!(r.ok && r.out == "w2: effort high → max when its turn ends (it restarts on its session; its background tasks end)", "{}", r.out);
        assert_eq!(core.agents["w2"].live.effort_due.as_deref(), Some("m:low"), "the session still runs at low");
        let r = core.effort(&Who::Owner, "w2", "low", None);
        assert!(r.ok && r.out == "w2: effort max → low; its session runs at low already, so it does not restart", "{}", r.out);
        assert!(core.agents["w2"].live.effort_due.is_none());
        assert_eq!(last_record(&coord, "w2").1, "m:low");
        core.agents.get_mut("w2").unwrap().live.state = "idle".into();
        assert!(core.to_restart().is_empty(), "nothing restarts");
        // idle, a change then back before the restart ran: the same
        idle(&mut core, "w2", pid);
        assert!(core.effort(&Who::Owner, "w2", "medium", None).ok);
        assert!(core.effort(&Who::Owner, "w2", "low", None).ok);
        assert!(core.to_restart().is_empty());
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-39, 1, 2 and 5: only the agents above NAME (and the owner); a level the harness does not take is refused
    /// naming those it takes; the same level does nothing; a Codex agent's levels are its model's.
    #[test]
    fn effort_refusals_and_the_same_level() {
        let (mut core, _rx, coord) = esc_coord("effort-no", &[("w2", "owner", ""), ("w3", "w2", "")], None, None);
        idle(&mut core, "w2", std::process::id() as i32);
        let no = |r: Reply| if r.ok { panic!("not refused: {}", r.out) } else { r.out };
        assert_eq!(no(core.effort(&Who::Owner, "nobody", "low", None)), "coord effort: no agent named nobody");
        assert_eq!(no(core.effort(&Who::Agent("w3".into()), "w2", "low", None)), "coord effort: only the agents above w2 change its effort");
        assert_eq!(no(core.effort(&Who::Agent("w2".into()), "w2", "low", None)), "coord effort: only the agents above w2 change its effort");
        assert!(core.effort(&Who::Agent("w2".into()), "w3", "low", None).ok, "an agent above");
        assert_eq!(no(core.effort(&Who::Owner, "w2", "turbo", None)), "coord effort: turbo is not a level w2's model takes (low, medium, high, xhigh, max)");
        assert_eq!(core.agents["w2"].model, "m");
        assert!(core.effort(&Who::Owner, "w2", "medium", None).ok);
        let reg = std::fs::read_to_string(coord.join("registry.jsonl")).unwrap();
        assert_eq!(core.to_restart().len(), 1);
        let r = core.effort(&Who::Owner, "w2", "medium", None);
        assert!(r.ok && r.out == "w2 already runs at medium", "{}", r.out);
        assert_eq!(std::fs::read_to_string(coord.join("registry.jsonl")).unwrap(), reg, "nothing written");
        // a Codex agent: its model's levels; its next turn takes it (no restart, nothing held)
        let c = core.agents.get_mut("w3").unwrap();
        c.kind = "codex".into();
        c.model = "gpt-x:low".into();
        idle(&mut core, "w3", std::process::id() as i32);
        let levels = || Some(Ok(vec!["low".to_string(), "medium".to_string(), "ultra".to_string()]));
        assert_eq!(no(core.effort(&Who::Owner, "w3", "max", levels())), "coord effort: max is not a level w3's model takes (low, medium, ultra)");
        assert_eq!(no(core.effort(&Who::Owner, "w3", "low", Some(Err("Codex's catalog has no model gpt-x".into())))), "coord effort: the levels gpt-x takes are not known: Codex's catalog has no model gpt-x");
        let r = core.effort(&Who::Owner, "w3", "ultra", levels());
        assert!(r.ok && r.out == "w3: effort low → ultra from its next turn", "{}", r.out);
        assert!(core.agents["w3"].live.effort_due.is_none() && core.to_restart().is_empty() && core.to_hand_over().is_empty());
        // in a turn too: nothing held, the next turn takes it
        core.agents.get_mut("w3").unwrap().live.state = "working".into();
        let r = core.effort(&Who::Owner, "w3", "medium", levels());
        assert!(r.ok && r.out == "w3: effort ultra → medium from its next turn", "{}", r.out);
        assert!(core.agents["w3"].live.effort_due.is_none());
        assert_eq!(core.agents["w3"].model, "gpt-x:medium");
        let _ = std::fs::remove_dir_all(&coord);
    }

    fn session_mode(coord: &std::path::Path, name: &str) -> Option<String> {
        let session: Value = serde_json::from_slice(&std::fs::read(coord.join(format!("agents/{name}/session.json"))).unwrap_or_default()).unwrap_or(Value::Null);
        session["mode"].as_str().map(String::from)
    }

    /// What the view says of the mode: what runs (`mode`), what he asked while that differs (`mode_due`).
    fn modes(core: &Core, name: &str) -> (Option<String>, Option<String>) {
        let v = core.view(name);
        (v.mode, v.mode_due)
    }

    fn some(s: &str) -> Option<String> {
        Some(s.to_string())
    }

    /// 0.2 line 25 (N-1400): the owner sets an idle agent to Yolo: its record says it at once (the registry's `mode`
    /// line, session.json's `mode`, which roster gives the start), its view's `mode_due` too, while its `mode` stays
    /// what its session runs in; it is restarted on its own session now, once, and its view says `restarting` from the
    /// restart's take until its end. The new process, started with the flag, is what turns `mode` (hs-ls and consult-3,
    /// N-1400 p6, p7). Back to Safe: the file's `mode` goes, the view's once the new process runs in Safe.
    #[test]
    fn mode_an_idle_agent_restarts_now_and_its_record_and_view_say_it() {
        let (mut core, _rx, coord) = esc_coord("mode-idle", &[("w2", "owner", "")], None, None);
        let pid = std::process::id() as i32;
        idle(&mut core, "w2", pid);
        let r = core.mode(&Who::Owner, "w2", "yolo");
        assert!(r.ok && r.out == "w2: Safe → Yolo; restarted on its session (its background tasks ended)", "{}", r.out);
        let (last, model) = last_record(&coord, "w2");
        assert_eq!((last["ev"].as_str(), last["name"].as_str(), last["mode"].as_str()), (Some("mode"), Some("w2"), Some("yolo")));
        assert_eq!((session_mode(&coord, "w2").as_deref(), model.as_str()), (Some("yolo"), "m"), "session.json: the mode beside the model");
        assert_eq!(modes(&core, "w2"), (None, some("yolo")), "the session runs in Safe yet; Yolo is due");
        assert!(!core.view("w2").restarting, "not before the restart is taken");
        assert_eq!(core.to_restart(), vec![("w2".to_string(), Some(pid))]);
        assert!(core.view("w2").restarting, "the restart runs");
        assert!(core.to_restart().is_empty(), "one restart at a time");
        // its new process (`coord run`), started with the flag: the view follows it; the restart's end leaves it alone
        core.new_process("w2", 1, true);
        core.restarted("w2", Some(pid), "");
        assert!(!core.view("w2").restarting && core.to_restart().is_empty() && !core.agents["w2"].live.yolo_due);
        assert_eq!(modes(&core, "w2"), (some("yolo"), None), "runs in Yolo, nothing due");
        // an effort afterwards writes session.json again: the mode stays in it
        assert!(core.effort(&Who::Owner, "w2", "high", None).ok);
        assert_eq!(session_mode(&coord, "w2").as_deref(), Some("yolo"), "a later write keeps the mode");
        let p = core.to_restart().first().and_then(|(_, p)| *p);
        core.restarted("w2", p, "");
        let r = core.mode(&Who::Owner, "w2", "safe");
        assert!(r.ok && r.out == "w2: Yolo → Safe; restarted on its session (its background tasks ended)", "{}", r.out);
        assert_eq!(session_mode(&coord, "w2"), None, "Safe: no mode in the file");
        assert_eq!(modes(&core, "w2"), (some("yolo"), some("safe")), "the header reads Yolo until a Safe process runs");
        assert_eq!(core.to_restart().len(), 1);
        core.new_process("w2", 2, false);
        core.restarted("w2", Some(1), "");
        assert_eq!(modes(&core, "w2"), (None, None));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-1929: Guarded and Yolo differ in the safety check alone, so a change of it restarts an idle Claude Code
    /// agent on its session now, as a mode does, and its view reads the check its session was started with until a
    /// new one runs; set back to what runs, nothing restarts; a failed restart drops the mark; a session whose check
    /// was not read restarts on the change too. The check is read in the start's environment.
    #[test]
    fn a_safety_check_change_restarts_the_session_and_its_view_reads_what_runs() {
        assert!(crate::core::started_check(b"A=1\0B=2\0"));
        assert!(!crate::core::started_check(b"A=1\0CLAUDE_CODE_EVEN_MORE_DANGEROUSLY_SKIP_PERMISSIONS=1\0"));
        let (mut core, _rx, coord) = esc_coord("check-idle", &[("w2", "owner", "")], None, None);
        let pid = std::process::id() as i32;
        idle(&mut core, "w2", pid);
        core.agents.get_mut("w2").unwrap().live.check_runs = Some(true);
        let r = core.harness_switches(&Who::Owner, "w2", Some(false), None, None);
        assert!(r.ok && r.out.ends_with("; its safety check changes, so it restarts on its session now"), "{}", r.out);
        assert_eq!(core.view("w2").safety_check_runs, Some(true), "Guarded runs till the new session");
        assert_eq!(core.to_restart(), vec![("w2".to_string(), Some(pid))]);
        core.new_process("w2", 1, false);
        core.agents.get_mut("w2").unwrap().live.check_runs = Some(false);
        core.restarted("w2", Some(pid), "");
        assert!(core.to_restart().is_empty() && !core.agents["w2"].live.check_due);
        assert_eq!(core.view("w2").safety_check_runs, Some(false), "Yolo runs");
        let r = core.harness_switches(&Who::Owner, "w2", Some(false), None, None);
        assert!(r.ok && !r.out.contains("restarts") && core.to_restart().is_empty(), "{}", r.out);
        // asked back to on, the restart fails: the mark goes, the view still reads what runs
        assert!(core.harness_switches(&Who::Owner, "w2", Some(true), None, None).ok);
        assert_eq!(core.to_restart().len(), 1);
        core.restarted("w2", Some(1), "roster restart failed: no terminal");
        assert!(!core.agents["w2"].live.check_due && core.to_restart().is_empty());
        assert_eq!(core.view("w2").safety_check_runs, Some(false));
        // a session whose check is not known: the change restarts it
        core.agents.get_mut("w2").unwrap().live.check_runs = None;
        let r = core.clear_switch(&Who::Owner, "w2", "safety_check");
        assert!(r.ok && r.out.ends_with("restarts on its session now"), "{}", r.out);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-1400: in a turn nothing is cut; the restart comes at the turn's end. A change back to the mode the session
    /// runs in, while one is due, drops the restart.
    #[test]
    fn mode_in_a_turn_waits_for_its_end_and_a_change_back_drops_it() {
        let (mut core, _rx, coord) = esc_coord("mode-turn", &[("w2", "owner", "")], None, None);
        let pid = std::process::id() as i32;
        working(&mut core, "w2");
        let r = core.mode(&Who::Owner, "w2", "yolo");
        assert!(r.ok && r.out == "w2: Safe → Yolo when its turn ends (it restarts on its session; its background tasks end)", "{}", r.out);
        assert_eq!(session_mode(&coord, "w2").as_deref(), Some("yolo"), "the record at once");
        assert!(core.to_restart().is_empty() && !core.view("w2").restarting, "no turn is stopped for it");
        assert_eq!(modes(&core, "w2"), (None, some("yolo")));
        let r = core.mode(&Who::Owner, "w2", "safe");
        assert!(r.ok && r.out == "w2: Yolo → Safe; its session runs in Safe already, so it does not restart", "{}", r.out);
        assert!(!core.agents["w2"].live.yolo_due);
        assert_eq!(modes(&core, "w2"), (None, None));
        assert!(core.mode(&Who::Owner, "w2", "yolo").ok);
        core.agents.get_mut("w2").unwrap().live.state = "idle".into();
        assert_eq!(core.to_restart(), vec![("w2".to_string(), Some(pid))], "at the turn's end");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-1400 (consult-3's 3 and 4, p5, p7): only the owner; every agent is refused, the agent itself and one above it
    /// alike; a mode that is none of the two, an agent that is not there, a Codex agent, a coord whose sessions run in
    /// boxes: refused; the same mode writes nothing. The mode outlives coordd's restart (the registry), a restart still
    /// due too (the journalled Live); a stopped agent takes it at its next start. A failed restart drops the mark and
    /// keeps the record, and the view reads what runs: in Yolo with Safe asked, the header stays Yolo, also after
    /// coordd's restart; asked again, it is tried again.
    #[test]
    fn mode_is_the_owners_only_and_outlives_restarts() {
        let (mut core, _rx, coord) = esc_coord("mode-no", &[("w2", "owner", ""), ("w3", "w2", "")], None, None);
        let pid = std::process::id() as i32;
        idle(&mut core, "w2", pid);
        idle(&mut core, "w3", pid);
        let no = |r: Reply| if r.ok { panic!("not refused: {}", r.out) } else { r.out };
        assert_eq!(no(core.mode(&Who::Agent("w2".into()), "w2", "yolo")), "coord mode: an agent's Safe · Yolo is the owner's");
        assert_eq!(no(core.mode(&Who::Agent("w2".into()), "w3", "yolo")), "coord mode: an agent's Safe · Yolo is the owner's", "not an agent above it either");
        assert_eq!(no(core.mode(&Who::Owner, "w2", "auto")), "coord mode: auto is not a mode: safe or yolo");
        assert_eq!(no(core.mode(&Who::Owner, "nobody", "yolo")), "coord mode: no agent named nobody");
        let reg = std::fs::read_to_string(coord.join("registry.jsonl")).unwrap();
        let r = core.mode(&Who::Owner, "w2", "safe");
        assert!(r.ok && r.out == "w2 is in Safe already", "{}", r.out);
        assert_eq!(std::fs::read_to_string(coord.join("registry.jsonl")).unwrap(), reg, "nothing written");
        assert!(!core.agents["w2"].yolo && !core.agents["w3"].yolo);
        // kept over coordd's restart, and the restart still due with it
        assert!(core.mode(&Who::Owner, "w2", "yolo").ok);
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let again = Core::load(&coord.display().to_string(), &base);
        assert!(again.agents["w2"].yolo && !again.agents["w3"].yolo);
        let live: crate::core::Live = serde_json::from_value(serde_json::to_value(&core.agents["w2"].live).unwrap()).unwrap();
        assert!(live.yolo_due && !live.yolo_runs, "journalled, with what the session runs in");
        // the restart failed: the old process runs on; the mark goes, the record stays, the view reads Safe with Yolo due
        assert_eq!(core.to_restart().len(), 1);
        core.restarted("w2", Some(pid), "roster restart failed: no terminal");
        assert!(!core.agents["w2"].live.yolo_due && core.to_restart().is_empty() && core.agents["w2"].yolo);
        assert_eq!(modes(&core, "w2"), (None, some("yolo")));
        // consult-3's case: it runs in Yolo, he asks Safe, the restart fails: the header still reads Yolo
        core.new_process("w2", pid, true);
        assert_eq!(modes(&core, "w2"), (some("yolo"), None));
        let r = core.mode(&Who::Owner, "w2", "safe");
        assert!(r.ok && r.out == "w2: Yolo → Safe; restarted on its session (its background tasks ended)", "{}", r.out);
        assert_eq!(core.to_restart(), vec![("w2".to_string(), Some(pid))]);
        core.restarted("w2", Some(pid), "roster restart failed: no terminal");
        assert_eq!(modes(&core, "w2"), (some("yolo"), some("safe")), "a failed Safe restart still reads Yolo");
        // then coordd restarts: the registry gives what he asked, the journal what runs (`recover`)
        let mut again = Core::load(&coord.display().to_string(), &base);
        assert!(!again.agents["w2"].yolo);
        again.agents.get_mut("w2").unwrap().live = serde_json::from_value(serde_json::to_value(&core.agents["w2"].live).unwrap()).unwrap();
        assert_eq!(modes(&again, "w2"), (some("yolo"), some("safe")), "after coordd's restart too");
        // asked again: tried again, the record written once
        let reg = std::fs::read_to_string(coord.join("registry.jsonl")).unwrap();
        let r = core.mode(&Who::Owner, "w2", "safe");
        assert!(r.ok && r.out == "w2: Safe again (its session still runs in Yolo); restarted on its session (its background tasks ended)", "{}", r.out);
        assert_eq!(std::fs::read_to_string(coord.join("registry.jsonl")).unwrap(), reg, "nothing written again");
        assert_eq!(core.to_restart(), vec![("w2".to_string(), Some(pid))]);
        // a stopped agent: its record only
        core.agents.get_mut("w3").unwrap().live = Default::default();
        let r = core.mode(&Who::Owner, "w3", "yolo");
        assert!(r.ok && r.out == "w3: Safe → Yolo from its next start", "{}", r.out);
        assert!(core.to_restart().iter().all(|(n, _)| n != "w3"));
        assert_eq!(modes(&core, "w3"), (some("yolo"), None), "a stopped one reads its next start's");
        // a Codex agent has no such mode
        core.agents.get_mut("w3").unwrap().kind = "codex".into();
        assert_eq!(no(core.mode(&Who::Owner, "w3", "safe")), "coord mode: w3 is a codex agent; Safe · Yolo is a Claude Code or agy agent's (its permission mode)");
        // nor a boxed project's agents: the box is the boundary
        core.project = Some(crate::project::Project { project: crate::project::Header { boxed: true, ..Default::default() }, ..Default::default() });
        assert_eq!(no(core.mode(&Who::Owner, "w2", "yolo")), "coord mode: this coord's sessions run in boxes, which bypass prompts already (the box is the boundary)");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-1956 (consult-lead-c p3): an agy agent has the owner's Safe · Yolo as a Claude Code agent has: the record and
    /// session.json at once, which roster starts agy-rpc in, and a restart into it now; back to Safe the same way.
    #[test]
    fn an_agy_agent_takes_the_owners_mode() {
        let (mut core, _rx, coord) = esc_coord("mode-agy", &[("g1", "owner", "")], None, None);
        core.agents.get_mut("g1").unwrap().kind = "agy".into();
        let pid = std::process::id() as i32;
        idle(&mut core, "g1", pid);
        let r = core.mode(&Who::Owner, "g1", "yolo");
        assert!(r.ok && r.out == "g1: Safe → Yolo; restarted on its session (its background tasks ended)", "{}", r.out);
        assert_eq!(session_mode(&coord, "g1").as_deref(), Some("yolo"), "session.json, which roster reads at its start");
        assert_eq!(core.to_restart(), vec![("g1".to_string(), Some(pid))]);
        assert_eq!(modes(&core, "g1"), (None, some("yolo")), "Safe runs until the new process, Yolo due");
        core.new_process("g1", pid + 1, true);
        assert_eq!(modes(&core, "g1"), (some("yolo"), None));
        let r = core.mode(&Who::Owner, "g1", "safe");
        assert!(r.ok && r.out == "g1: Yolo → Safe; restarted on its session (its background tasks ended)", "{}", r.out);
        assert_eq!(session_mode(&coord, "g1"), None);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-1400 (consult-3 p7): what a session runs in is read from its own command line at its `coord run`, where the
    /// process is still coordc with the session's after `--`: only the flag there counts, and only for a host Claude
    /// Code or agy agent; a box's sessions bypass prompts whatever he set.
    #[test]
    fn a_sessions_mode_is_read_from_its_command_line() {
        assert!(crate::core::started_yolo(b"coordc\0run\0w2\0--\0claude\0--model\0m\0--dangerously-skip-permissions\0--settings\0s\0"));
        let agy = b"coordc\0run\0g1\0--\0/ab/bin/roster\0said\0x\0--\0/ab/bin/agy-rpc\0serve\0g1\0/w\0gemini-3.8-flash-high\0s1\0";
        assert!(crate::core::started_yolo(&[&agy[..], b"--dangerously-skip-permissions\0"].concat()), "agy-rpc's, last");
        assert!(!crate::core::started_yolo(agy), "agy-rpc's in Safe");
        assert!(!crate::core::started_yolo(b"coordc\0run\0w2\0--\0claude\0--model\0m\0--settings\0s\0"));
        assert!(!crate::core::started_yolo(b"--dangerously-skip-permissions\0claude\0"), "not without coordc's --");
        let (mut core, _rx, coord) = esc_coord("mode-argv", &[("w2", "owner", ""), ("w3", "owner", "")], None, None);
        let run = |yolo: bool| {
            let mut args = vec!["-c", "sleep 30; :", "coordc", "run", "w2", "--", "claude", "--model", "m"];
            if yolo {
                args.push("--dangerously-skip-permissions");
            }
            std::process::Command::new("sh").args(args).spawn().unwrap()
        };
        let (mut y, mut s) = (run(true), run(false));
        // spawn returns while the child is still in its exec: the kernel sets the new command line a moment later,
        // and a read before that is empty (of 2,000 spawns read at once, 791 were; docket-edge5, N-1446). Read once
        // it shows coordc's, as `coord run` reads a coordc that is running already.
        for c in [&y, &s] {
            let at = format!("/proc/{}/cmdline", c.id());
            let shown = (0..5000).any(|_| std::fs::read(&at).is_ok_and(|b| b.windows(6).any(|w| w == b"coordc")) || {
                std::thread::sleep(std::time::Duration::from_millis(1));
                false
            });
            assert!(shown, "the child's command line never showed coordc's");
        }
        assert!(core.starts_in_yolo("w2", y.id() as i32));
        assert!(!core.starts_in_yolo("w2", s.id() as i32));
        core.agents.get_mut("w3").unwrap().kind = "agy".into();
        assert!(core.starts_in_yolo("w3", y.id() as i32), "an agy agent's, read the same way");
        core.agents.get_mut("w3").unwrap().kind = "codex".into();
        assert!(!core.starts_in_yolo("w3", y.id() as i32), "a Codex agent has no such mode");
        core.project = Some(crate::project::Project { project: crate::project::Header { boxed: true, ..Default::default() }, ..Default::default() });
        assert!(!core.starts_in_yolo("w2", y.id() as i32), "a boxed project's");
        for c in [&mut y, &mut s] {
            let _ = c.kill();
            let _ = c.wait();
        }
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// The project lines (N-44) written to the feed since BEFORE lines.
    fn project_lines(coord: &std::path::Path, before: &mut usize) -> Vec<Value> {
        let all: Vec<Value> = std::fs::read_to_string(coord.join("feed.jsonl")).unwrap_or_default().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        let new = all[*before..].iter().filter(|l| l["delivery"] == "project").cloned().collect();
        *before = all.len();
        new
    }

    fn relaying(core: &mut Core) {
        core.project = Some(crate::project::Project { project: crate::project::Header { host_relay: true, ..Default::default() }, ..Default::default() });
    }

    /// N-44, names and OUT: PROJECT/NAME is taken where a message names someone, only with host_relay = true; a post
    /// due to it is one project line in the feed, then given; OWN/NAME is NAME; esc, effort and log refuse it.
    #[test]
    fn a_message_to_another_projects_agent_is_a_project_line() {
        let (mut core, _rx, coord) = esc_coord("far-out", &[("a", "owner", ""), ("a2", "owner", "")], None, None);
        let own = core.project_name();
        let r = core.ticket_msg(&Who::Agent("a".into()), None, vec!["x/b".into()], "hi", false, None, Vec::new(), Said::default());
        assert_eq!(r.out, "coord msg: this project's messages are not carried to other projects (no host_relay = true in its project file)");
        relaying(&mut core);
        let r = core.ticket_msg(&Who::Agent("a".into()), None, vec!["x/b".into()], "hi \"there\"\n$(not run)", false, None, Vec::new(), Said::default());
        assert!(r.ok && r.out.ends_with("Told x/b."), "{}", r.out);
        let n = r.out.split_whitespace().find(|w| w.starts_with("N-")).unwrap().trim_end_matches('.').to_string();
        let mut seen = 0;
        core.hand_far();
        let lines = project_lines(&coord, &mut seen);
        assert_eq!(lines.len(), 1, "{lines:?}");
        let l = &lines[0];
        assert_eq!((l["kind"].as_str(), l["project"].as_str(), l["from"].as_str(), l["ticket"].as_str()), (Some("msg"), Some("x"), Some("a"), Some(n.as_str())));
        assert_eq!(l["to"], json!(["x/b"]));
        assert_eq!(l["id"].as_str(), Some(format!("{n}.p1").as_str()));
        assert_eq!(l["text"].as_str(), Some("hi \"there\"\n$(not run)"));
        assert!(l.get("origin").is_none());
        let t = docket_machine::parse_ticket(&n).unwrap();
        assert!(core.tickets.m.waits_between(t).is_empty(), "carried: nothing here waits on x/b");
        core.hand_far();
        assert!(project_lines(&coord, &mut seen).is_empty(), "given: not carried twice");
        assert!(core.reply_readers().contains("x/b"), "the rule Reply may name it");
        // OWN/NAME is NAME
        let r = core.ticket_msg(&Who::Agent("a".into()), None, vec![format!("{own}/a2")], "local", false, None, Vec::new(), Said::default());
        assert!(r.ok && r.out.ends_with("Told a2."), "{}", r.out);
        // nothing else takes a far name
        assert_eq!(core.ticket_esc(&Who::Owner, vec!["x/b".into()]).out, "coord esc: x/b is in another project; only messages cross projects");
        assert_eq!(core.effort(&Who::Owner, "x/b", "low", None).out, "coord effort: x/b is in another project; only messages cross projects");
        assert_eq!(core.log(&Who::Owner, "x/b", 5).out, "coord log: x/b is in another project; only messages cross projects");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-44, IN and the two rules, on the side the message comes to: --origin opens the thread once and posts on it
    /// after; the origin is in the log and replays; the reply written here goes back with the origin; the far author's
    /// own post is not sent back; a far author posts only on a thread it is on.
    #[test]
    fn a_message_from_another_project_opens_its_thread_and_the_reply_goes_back() {
        let (mut core, _rx, coord) = esc_coord("far-in", &[("b", "owner", "")], None, None);
        let r = core.ticket_msg_origin(&Who::Owner, Some("p/a".into()), vec!["b".into()], "hi", false, None, Some("p:N-12".into()), Vec::new(), Said::default());
        assert_eq!(r.out, "coord msg: this project's messages are not carried to other projects (no host_relay = true in its project file)");
        relaying(&mut core);
        let r = core.ticket_msg_origin(&Who::Agent("b".into()), Some("p/a".into()), vec!["b".into()], "hi", false, None, Some("p:N-12".into()), Vec::new(), Said::default());
        assert!(!r.ok, "only the owner side sends for another");
        let tickets = core.tickets.m.tickets.len();
        let r = core.ticket_msg_origin(&Who::Owner, Some("p/a".into()), vec!["b".into()], "hi", false, None, Some("p:N-12".into()), Vec::new(), Said::default());
        assert!(r.ok, "{}", r.out);
        assert_eq!(core.tickets.m.tickets.len(), tickets + 1);
        let t = core.origin_thread("p:N-12").expect("the thread that came in");
        let id = core.tickets.m.id(t);
        let x = &core.tickets.m.tickets[&t];
        assert_eq!((x.kind.as_str(), x.opener.as_deref()), ("thread", Some("p/a")));
        assert!(core.ticket_due("b").iter().any(|d| d.t == t), "b is handed it");
        let mut seen = 0;
        core.hand_far();
        assert!(project_lines(&coord, &mut seen).is_empty(), "the far author's own post is not sent back");
        // the next message from there: the same thread
        let r = core.ticket_msg_origin(&Who::Owner, Some("p/a".into()), vec!["b".into()], "and more", false, None, Some("p:N-12".into()), Vec::new(), Said::default());
        assert!(r.ok, "{}", r.out);
        assert_eq!(core.tickets.m.tickets.len(), tickets + 1, "no second thread");
        // b's answer (as the rule Reply posts it, naming p/a): back to p, with the origin
        let r = core.ticket_msg(&Who::Agent("b".into()), None, vec!["p/a".into()], "the answer", false, Some(id.clone()), Vec::new(), Said::default());
        assert!(r.ok, "{}", r.out);
        core.hand_far();
        let lines = project_lines(&coord, &mut seen);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!((lines[0]["project"].as_str(), lines[0]["from"].as_str(), lines[0]["origin"].as_str(), lines[0]["text"].as_str()), (Some("p"), Some("b"), Some("p:N-12"), Some("the answer")));
        assert_eq!(lines[0]["to"], json!(["p/a"]));
        // a far author not on a thread: refused; one on it: posts
        let r = core.ticket_msg(&Who::Owner, Some("p/z".into()), vec!["b".into()], "me too", false, Some(id.clone()), Vec::new(), Said::default());
        assert_eq!(r.out, format!("coord msg: p/z does not take part in {id}"));
        assert!(core.ticket_msg(&Who::Owner, Some("p/a".into()), vec!["b".into()], "ok", false, Some(id.clone()), Vec::new(), Said::default()).ok);
        // the origin replays
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let again = Tickets::load(&coord.display().to_string(), &base, None, 1);
        assert!(again.errors.is_empty(), "{:?}", again.errors);
        assert_eq!(again.m.tickets[&t].form["origin"].as_str(), Some("p:N-12"));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-44, rule 1 on the side the thread was opened: the far agent's reply that came in is not carried back to its
    /// project; the answer to it is, without an origin; a post by an agent of another project Y goes to X still.
    #[test]
    fn a_thread_opened_here_carries_only_what_was_not_written_there() {
        let (mut core, _rx, coord) = esc_coord("far-rule1", &[("a", "owner", "")], None, None);
        relaying(&mut core);
        let r = core.ticket_msg(&Who::Agent("a".into()), None, vec!["x/b".into()], "question", false, None, Vec::new(), Said::default());
        let id = r.out.split_whitespace().find(|w| w.starts_with("N-")).unwrap().trim_end_matches('.').to_string();
        let mut seen = 0;
        core.hand_far();
        assert_eq!(project_lines(&coord, &mut seen).len(), 1);
        // x/b's reply, as the relay posts it here
        let r = core.ticket_msg(&Who::Owner, Some("x/b".into()), vec!["a".into()], "reply", false, Some(id.clone()), Vec::new(), Said::default());
        assert!(r.ok, "{}", r.out);
        core.hand_far();
        assert!(project_lines(&coord, &mut seen).is_empty(), "x/b's own post is not sent back to x");
        let r = core.ticket_msg(&Who::Agent("a".into()), None, vec!["x/b".into()], "thanks", false, Some(id.clone()), Vec::new(), Said::default());
        assert!(r.ok, "{}", r.out);
        core.hand_far();
        let lines = project_lines(&coord, &mut seen);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!((lines[0]["project"].as_str(), lines[0]["text"].as_str()), (Some("x"), Some("thanks")));
        assert!(lines[0].get("origin").is_none());
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-65: records from before hold the relay's notices, posted as coord-relay and as x/coord-relay (one carried
    /// from another project). The relay takes part in nothing: no project line for its posts; a local agent's turn-final
    /// reply and its `coord msg --re` on the thread are accepted and name the real participants only; the project lines
    /// go to the real far names only. On a thread that came in from p, a notice posted here is not carried to p.
    #[test]
    fn the_relays_old_posts_name_nobody_and_go_nowhere() {
        let (mut core, _rx, coord) = esc_coord("relay-posts", &[("a", "owner", ""), ("b", "owner", "")], None, None);
        relaying(&mut core);
        let said = |agent: &str, text: &str| {
            let f = coord.join(format!("{agent}-{}.jsonl", text.len()));
            let entry = format!("{}\n", json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": text}]}}));
            std::fs::write(&f, &entry).unwrap();
            Loc { file: f.display().to_string(), offset: 0, len: entry.len() as u64 }
        };
        let r = core.ticket_msg(&Who::Agent("a".into()), None, vec!["x/b".into()], "question", false, None, Vec::new(), Said::default());
        let id = r.out.split_whitespace().find(|w| w.starts_with("N-")).unwrap().trim_end_matches('.').to_string();
        let t = docket_machine::parse_ticket(&id).unwrap();
        let mut seen = 0;
        core.hand_far();
        assert_eq!(project_lines(&coord, &mut seen).len(), 1);
        // x/b's reply, as the relay posts it here; then the relay's old notices, as the record holds them
        assert!(core.ticket_msg(&Who::Owner, Some("x/b".into()), vec!["a".into()], "reply", false, Some(id.clone()), Vec::new(), Said::default()).ok);
        for by in ["coord-relay", "x/coord-relay"] {
            core.ticket_input(Input::Post { t, by: by.into(), text: Text::New("Not delivered to x/b: …".into()), names: vec!["a".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        }
        assert_eq!(core.takers(t), vec!["a".to_string(), "x/b".to_string()]);
        assert!(!core.far_names().contains("x/coord-relay") && !core.reply_readers().contains("x/coord-relay"));
        core.hand_far();
        assert!(project_lines(&coord, &mut seen).is_empty(), "the relay's posts are not carried");
        // a's turn ends with a reply: it names x/b only, and goes to x/b only
        let d = core.ticket_due("a").into_iter().find(|d| d.t == t).expect("a is owed the posts");
        core.tickets.handed.insert(("a".into(), d.id.clone()), (d.t, d.upto, Default::default()));
        core.ticket_handed("a", std::slice::from_ref(&d.id), true);
        core.ticket_turn_ended("a", said("a", "Thanks; here is more."));
        let p = core.tickets.m.tickets[&t].posts.last().unwrap().clone();
        assert_eq!((p.by.as_str(), p.names.clone()), ("a", vec!["x/b".to_string()]));
        core.hand_far();
        let lines = project_lines(&coord, &mut seen);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!((lines[0]["from"].as_str(), lines[0]["to"].clone()), (Some("a"), json!(["x/b"])));
        // a's `coord msg --re` on the thread (everyone on it): x/b only
        let r = core.ticket_msg(&Who::Agent("a".into()), None, vec![], "and one more thing", false, Some(id.clone()), Vec::new(), Said::default());
        assert!(r.ok && r.out.ends_with("Told x/b."), "{}", r.out);
        core.hand_far();
        let lines = project_lines(&coord, &mut seen);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0]["to"], json!(["x/b"]));
        // a thread that came in from p: the relay's notice posted here is not carried back to p
        assert!(core.ticket_msg_origin(&Who::Owner, Some("p/a".into()), vec!["b".into()], "hi", false, None, Some("p:N-12".into()), Vec::new(), Said::default()).ok);
        let u = core.origin_thread("p:N-12").unwrap();
        core.ticket_input(Input::Post { t: u, by: "coord-relay".into(), text: Text::New("Not delivered to p/a: …".into()), names: vec!["b".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        core.hand_far();
        assert!(project_lines(&coord, &mut seen).is_empty(), "no project line for the relay's notice");
        let r = core.ticket_msg(&Who::Agent("b".into()), None, vec![], "the answer", false, Some(core.tickets.m.id(u)), Vec::new(), Said::default());
        assert!(r.ok && r.out.ends_with("Told p/a."), "{}", r.out);
        core.hand_far();
        let lines = project_lines(&coord, &mut seen);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!((lines[0]["project"].as_str(), lines[0]["from"].as_str(), lines[0]["to"].clone()), (Some("p"), Some("b"), json!(["p/a"])));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-70: a post coordd writes itself (the machine's "AGENT was given D twice and did not read it.", by coordd, auto)
    /// on a thread with another project's agent: hand_far writes no project line for it, alone or as the context of
    /// the next post due there; and coordd is not someone a thread's `coord msg --re` names.
    #[test]
    fn coordds_own_post_does_not_leave_the_project() {
        let (mut core, _rx, coord) = esc_coord("coordd-post", &[("a", "owner", "")], None, None);
        relaying(&mut core);
        let r = core.ticket_msg(&Who::Agent("a".into()), None, vec!["x/b".into()], "question", false, None, Vec::new(), Said::default());
        let id = r.out.split_whitespace().find(|w| w.starts_with("N-")).unwrap().trim_end_matches('.').to_string();
        let t = docket_machine::parse_ticket(&id).unwrap();
        let mut seen = 0;
        core.hand_far();
        assert_eq!(project_lines(&coord, &mut seen).len(), 1);
        // the post as machine.rs writes it
        let x = core.tickets.m.tickets.edit().get_mut(&t).unwrap();
        let n = x.last() + 1;
        x.posts.push(docket_machine::Post { n, at: "2026-11-14T12:00:00Z".into(), by: String::from("coordd"), text: Text::New(format!("a was given {id}.p1 twice and did not read it.")), head: None, step: None, names: Vec::new(), told: Vec::new(), auto: true, moved: None, from: None, held: None, halted: false, dropped: false, routed: false, asks: Vec::new(), asks_open: false, asks_closed: None, answers: None, inform: Vec::new(), typed: None, cleaned_from: None, split_from: None, also: Vec::new(), on: Vec::new(), would_close: None, taken_back: false, joins: None, passes: None, comments: Vec::new(), into_turn: Vec::new(), kind: None, answering: Vec::new() });
        core.hand_far();
        assert!(project_lines(&coord, &mut seen).is_empty(), "alone: nothing");
        // a's next post naming x/b: x/b is owed it with coordd's post before it; only a's goes out
        let r = core.ticket_msg(&Who::Agent("a".into()), None, vec!["x/b".into()], "and more", false, Some(id.clone()), Vec::new(), Said::default());
        assert!(r.ok, "{}", r.out);
        core.hand_far();
        let lines = project_lines(&coord, &mut seen);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!((lines[0]["from"].as_str(), lines[0]["text"].as_str(), lines[0]["to"].clone()), (Some("a"), Some("and more"), json!(["x/b"])));
        // to everyone on the thread: x/b, not coordd (was: "coord: no agent or host named coordd")
        let r = core.ticket_msg(&Who::Agent("a".into()), None, vec![], "and the rest", false, Some(id.clone()), Vec::new(), Said::default());
        assert!(r.ok && r.out.ends_with("Told x/b."), "{}", r.out);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-40: coordd says where the proxy's log is as the project file writes it, and never opens it: the folder need
    /// not exist where coordd runs (a box does not see it). Today's refusals stay, word for word.
    #[test]
    fn the_raw_source_is_the_project_files_folder_as_written_and_the_session() {
        let (mut core, _rx, coord) = esc_coord("raw", &[("w2", "owner", ""), ("w3", "w2", "")], None, Some("kind k\n  letter K\n  form what: Text\n  result Text\n  pattern\n    return what\n"));
        let r = core.proxy_source(&Who::Owner, "w2");
        assert_eq!(r, Err("coord raw: the project file names no proxy_log folder".to_string()));
        let file = coord.join("files/project.toml");
        std::fs::write(&file, "[project]\nname = \"e\"\nproxy_log = \"~/no/such/folder\"\n").unwrap();
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        assert_eq!(core.proxy_source(&Who::Owner, "w2"), Err("coord raw: w2 has no session yet".to_string()));
        core.agents.get_mut("w2").unwrap().session = json!({"session_id": "s-1"});
        let src = core.proxy_source(&Who::Owner, "w2").unwrap();
        assert_eq!(src, coord_types::RawSource { dir: "~/no/such/folder".into(), session: "s-1".into() });
        // a relative folder is passed on as written too (the app does not read it)
        std::fs::write(&file, "[project]\nname = \"e\"\nproxy_log = \"logs\"\n").unwrap();
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        assert_eq!(core.proxy_source(&Who::Owner, "w2").unwrap().dir, "logs");
        // only the agents above it
        assert_eq!(core.proxy_source(&Who::Agent("w3".into()), "w2"), Err("coord raw: only the agents above w2 read its session".to_string()));
        assert_eq!(core.proxy_source(&Who::Agent("w2".into()), "w3"), Err("coord raw: w3 has no session yet".to_string()));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// An opencode agent's raw calls are the files its plugin writes in its coord folder (`raw/`), under its opencode
    /// session: no proxy_log is needed for it, and before its session is made it has none yet. The agents above it only.
    #[test]
    fn an_opencode_agents_raw_source_is_its_own_folder_and_session() {
        let (mut core, _rx, coord) = esc_coord("raw-oc", &[("w2", "owner", ""), ("w3", "w2", "")], None, None);
        core.agents.get_mut("w2").unwrap().kind = "opencode".into();
        assert_eq!(core.proxy_source(&Who::Owner, "w2"), Err("coord raw: w2 has no session yet".to_string()));
        core.agents.get_mut("w2").unwrap().session = json!({"session_id": "ses_1", "socket": "/c/agents/w2/opencode/coord/"});
        let src = core.proxy_source(&Who::Owner, "w2").unwrap();
        assert_eq!(src, coord_types::RawSource { dir: "/c/agents/w2/opencode/coord/raw".into(), session: "ses_1".into() });
        assert_eq!(core.proxy_source(&Who::Agent("w3".into()), "w2"), Err("coord raw: only the agents above w2 read its session".to_string()));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-802: `RawPage` is read by coordd where it runs, for the agents above the agent and the owner: the agent's own
    /// session's requests, bodies only (no header, no `metadata.user_id`); a folder coordd cannot see (a boxed one) is
    /// said in words, and the pane gets nothing else.
    #[test]
    fn the_raw_page_is_read_by_coordd_bodies_only_and_only_for_those_above() {
        let (mut core, _rx, coord) = esc_coord("rawpage", &[("w2", "owner", ""), ("w3", "w2", "")], None, Some("kind k\n  letter K\n  form what: Text\n  result Text\n  pattern\n    return what\n"));
        let logs = coord.join("proxy-logs");
        std::fs::create_dir_all(&logs).unwrap();
        let log = |session: &str, said: &str| {
            format!(
                "=== REQUEST INFO ===\nTimestamp: 2026-11-11T08:41:49+01:00\n\n=== HEADERS ===\nAuthorization: Bearer key-1\nX-Claude-Code-Session-Id: {session}\n\n=== API REQUEST 1 ===\nAuth: provider=claude, label=someone@example.com\n\nHeaders:\nX-Api-Key: secret-2\n\nBody:\n{{\"model\":\"m\",\"messages\":[{{\"role\":\"user\",\"content\":\"{said}\"}}],\"metadata\":{{\"user_id\":\"user_7f3e\"}}}}\n\n=== API RESPONSE 1 ===\nStatus: 200\nHeaders:\nX: y\n\nevent: message_stop\ndata: {{\"type\":\"message_stop\"}}\n"
            )
        };
        std::fs::write(logs.join("v1-messages-2026-11-11T084149-a.log"), log("s-1", "fix mean")).unwrap();
        std::fs::write(logs.join("v1-messages-2026-11-11T084150-b.log"), log("s-2", "another agent's")).unwrap();
        std::fs::write(logs.join("request-body-1.tmp"), "Authorization: Bearer key-3").unwrap();
        let file = coord.join("files/project.toml");
        std::fs::write(&file, format!("[project]\nname = \"e\"\nproxy_log = \"{}\"\n", logs.display())).unwrap();
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        core.agents.get_mut("w2").unwrap().session = json!({"session_id": "s-1"});
        let shared = crate::Shared::new(core);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let ask = |w: Who| rt.block_on(crate::raw_page(&shared, &w, "w2".into(), None, None, None));
        let r = ask(Who::Owner);
        assert!(r.ok, "{}", r.out);
        let page: proxylog::RawPage = serde_json::from_str(&r.out).unwrap();
        let all = page.lines.join("\n");
        assert!(page.lines[0].starts_with("══ request ") && all.contains("fix mean"), "{all}");
        for never in ["Bearer", "key-", "secret-2", "someone@example.com", "user_7f3e", "another agent's"] {
            assert!(!all.contains(never), "{never} sent: {all}");
        }
        // only the agents above w2 read it
        assert_eq!(ask(Who::Agent("w3".into())).out, "coord raw: only the agents above w2 read its session");
        // a folder that is not where coordd runs
        std::fs::write(&file, "[project]\nname = \"e\"\nproxy_log = \"/no/such/proxy-logs\"\n").unwrap();
        rt.block_on(shared.lock()).project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        let r = ask(Who::Owner);
        assert_eq!((r.ok, r.out.as_str()), (false, "coord raw: the proxy's log cannot be read where this project's server runs"));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-36, 1: an agent out of a turn has nothing typed and nothing written; the reply says so.
    #[test]
    fn esc_to_an_idle_agent_types_nothing() {
        let (mut core, mut rx, coord) = esc_coord("idle", &[("w2", "owner", "")], None, None);
        let log = coord.join("tickets/log.jsonl");
        let before = std::fs::read_to_string(&log).unwrap_or_default();
        let r = core.ticket_esc(&Who::Owner, vec!["w2".into()]);
        assert!(r.ok && r.out == "w2 is not in a turn", "{}", r.out);
        assert_eq!(interrupts(&mut rx), 0);
        assert_eq!(std::fs::read_to_string(&log).unwrap_or_default(), before);
        // no such agent: refused
        assert!(!core.ticket_esc(&Who::Owner, vec!["nobody".into()]).ok);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// His message TEXT to w2 as ticket_say makes it: the owner_message event (its own path waits for the clerk's
    /// verdict), no esc key in it, and the watcher's Thread for it. Gives the thread it is on.
    fn owner_says(core: &mut Core, text: &str) -> u64 {
        let v = json!({"from": "owner", "to": "w2", "typed": ["w2"], "names": [], "text": text, "addressed": {"head": "Unnamed"}});
        core.ticket_input(Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        core.ticket_answer(docket_machine::WATCHER, t, json!({"head": "Thread"}), Text::New(String::new())).unwrap();
        *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0
    }

    /// N-36, 3: a message sent right after an esc waits until the stop has landed and the turn is over; a message
    /// alone stops nothing, and goes into the running turn (N-101 p1214). On the text before his posts waited for the
    /// turn's end (`his_into`); on the text now, [`an_esc_then_his_messages_go_after_the_stop_as_one_given_once`].
    #[test]
    fn a_message_after_an_esc_waits_for_the_stop() {
        let (mut core, mut rx, coord) = esc_coord("after", &[("w2", "owner", "")], None, None);
        crate::core::delivery_tests::his_into(&coord, &mut core);
        working(&mut core, "w2");
        // a message alone, mid-turn: no stop; it is put into the running turn (N-101 p1214)
        owner_says(&mut core, "use the median");
        assert_eq!(interrupts(&mut rx), 0, "a message stops no turn");
        assert_eq!(core.to_hand_over(), vec!["w2".to_string()]);
        // esc, then a message: one stop; nothing handed until the Escape has gone out and the turn is over
        assert!(core.ticket_esc(&Who::Owner, vec!["w2".into()]).ok);
        owner_says(&mut core, "stop, wrong branch");
        assert_eq!(interrupts(&mut rx), 1);
        assert!(core.to_hand_over().is_empty());
        core.agents.get_mut("w2").unwrap().live.state = "idle".into();
        assert!(core.to_hand_over().is_empty(), "the turn looks over, but the Escape has not been typed yet");
        core.interrupted("w2");
        assert_eq!(core.to_hand_over(), vec!["w2".to_string()]);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// [`a_message_after_an_esc_waits_for_the_stop`] on the text now, where his posts wait for the turn's end too (the
    /// owner, N-1104 p716; paper-research N-1446 p2111 point 1): his message mid-turn waits and stops nothing; after an
    /// esc, the one he sends waits too; once the Escape has gone out and the turn is over, the two go as one message,
    /// with nothing else to end their wait, and once.
    #[test]
    fn an_esc_then_his_messages_go_after_the_stop_as_one_given_once() {
        use crate::core::delivery_tests::{channel, t_of};
        let (mut core, mut rx, coord) = esc_coord("after-now", &[("w2", "owner", "")], None, None);
        working(&mut core, "w2");
        let first = owner_says(&mut core, "use the median");
        assert_eq!(interrupts(&mut rx), 0, "a message stops no turn");
        assert!(core.to_hand_over().is_empty(), "his message waits for the turn's end");
        assert!(core.ticket_esc(&Who::Owner, vec!["w2".into()]).ok);
        let second = owner_says(&mut core, "stop, wrong branch");
        assert_eq!(interrupts(&mut rx), 1, "one stop");
        assert!(core.to_hand_over().is_empty());
        let kept: Vec<String> = core.tickets.m.kept("w2").values().cloned().collect();
        assert_eq!(kept, ["comes when its turn ends", "comes when its turn ends"], "both his, kept for the turn's end");
        core.agents.get_mut("w2").unwrap().live.state = "idle".into();
        assert!(core.to_hand_over().is_empty(), "the turn looks over, but the Escape has not been typed yet");
        core.interrupted("w2");
        assert_eq!(core.to_hand_over(), vec!["w2".to_string()], "after the stop");
        let (ids, _, _, _, into, rest) = channel(&mut core, &mut rx).expect("handed");
        assert_eq!((ids.iter().map(|i| t_of(i)).collect::<Vec<u64>>(), into, rest), (vec![first, second], false, Vec::<String>::new()), "the two as one: {ids:?}");
        core.handed("w2", &ids, true, "channel");
        assert!(core.to_hand_over().is_empty() && channel(&mut core, &mut rx).is_none(), "given once");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// Who may speak to the owner first is his settings': anyone with none, then only leads, then only those named; a
    /// question is held to it too.
    #[test]
    fn only_those_his_settings_let_speak_to_the_owner_first() {
        let (mut core, _rx, coord) = esc_coord("speak", &[("lead1", "owner", "lead"), ("w2", "lead1", "")], None, None);
        let owner = ["owner".to_string()];
        assert!(core.may_name("w2", &owner, None).is_ok(), "nothing set: anyone");
        let file = coord.join("project.toml");
        std::fs::write(coord.join("project"), file.display().to_string()).unwrap();
        for (set, w2, lead1) in [("\"leads\"", false, true), ("[\"w2\"]", true, false), ("\"any\"", true, true)] {
            std::fs::write(&file, format!("[project]\nowner_speakers = {set}\n")).unwrap();
            core.project = crate::project::Project::load(&core.coord).unwrap();
            assert_eq!((core.may_name("w2", &owner, None).is_ok(), core.may_name("lead1", &owner, None).is_ok()), (w2, lead1), "{set}");
            assert_eq!(core.ticket_ask_wait("w2", "which branch?", None).is_err_and(|e| e.contains("do not let you speak to him first")), !w2, "{set}");
        }
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// paper-research N-36 post 6, (b): an esc is not words and changes no hold. An agent waiting in its ask is
    /// stopped: the Escape cancels the ask's tool call (AskLater), a message sent after the esc as a new thread is
    /// handed once the stop has landed, and a post on the ticket of the step it asked on still waits for the answer.
    #[test]
    fn an_esc_to_an_agent_in_its_ask_lets_the_next_message_through_and_no_more() {
        let kind = "kind work\n  letter W\n  form w: Agent\n  result ()\n  pattern\n    v <- step w intake(\"do it\", w);\n    return ()\n  when ask(q) do\n    a <- step role fixer answer(q.text, q.context);\n    reply q a\n";
        let (mut core, mut rx, coord) = esc_coord("ask", &[("w2", "owner", "")], None, Some(kind));
        core.tickets.m.defs.edit().block_ask = true;
        core.ticket_input(Input::Open { kind: "work".into(), under: None, opener: "owner".into(), form: json!({"w": "w2"}), text: None, names: vec![], told: vec![] }).unwrap();
        let work = core.tickets.m.tickets.values().find(|x| x.kind == "work").map(|x| x.t).unwrap();
        let d = core.ticket_due("w2");
        assert_eq!(d.len(), 1, "{d:?}");
        core.ticket_input(Input::Given { t: d[0].t, to: "w2".into(), delivery: d[0].id.clone(), upto: d[0].upto, skipped: Default::default(), place: None }).unwrap();
        working(&mut core, "w2");
        // 1. w2 waits in its ask
        core.ticket_ask_wait("w2", "which branch?", None).unwrap();
        // 2. coord esc to it; the Escape cancels the ask's tool call, which says later (main::ask_wait)
        let r = core.ticket_esc(&Who::Owner, vec!["w2".into()]);
        assert!(r.ok && r.out == "w2's turn is being stopped", "{}", r.out);
        assert_eq!(interrupts(&mut rx), 1);
        core.ticket_ask_later("w2").unwrap();
        // 3. coord msg to it, a new thread; and a post on the asked step's ticket
        let r = core.ticket_msg(&Who::Owner, None, vec!["w2".into()], "use main", false, None, Vec::new(), Said::default());
        assert!(r.ok, "{}", r.out);
        let thread = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        assert_eq!(core.tickets.m.tickets[&thread].kind, "thread");
        core.ticket_input(Input::Post { t: work, by: "owner".into(), text: Text::New("and the docs".into()), names: vec!["w2".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        assert!(core.to_hand_over().is_empty(), "nothing before the stop has landed");
        // 4. the stop lands and the turn is over (its transcript's interrupt entry is the cut, main::follow): the
        // message is handed; the post on the asked step's ticket waits
        core.interrupted("w2");
        core.agents.get_mut("w2").unwrap().live.state = "idle".into();
        core.ticket_input(Input::TurnCut { agent: "w2".into() }).unwrap();
        assert_eq!(core.to_hand_over(), vec!["w2".to_string()]);
        let due = core.ticket_due("w2");
        assert!(due.iter().any(|d| d.t == thread), "the message after the esc: {due:?}");
        assert!(due.iter().all(|d| d.t != work), "the asked step's ticket waits for the answer: {due:?}");
        assert_eq!(interrupts(&mut rx), 0);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// paper-research N-36 post 8, 1: "esc, then the message" holds for a Codex agent too. Its interrupt landing is the
    /// cut a Claude Code agent's transcript gives (a Codex agent has no follower): what the stopped turn was given is
    /// forgotten, so the message after the esc is handed once the stop has landed.
    #[test]
    fn a_codex_agent_is_handed_the_message_after_an_esc() {
        let kind = "kind work\n  letter W\n  form w: Agent\n  result ()\n  pattern\n    v <- step w intake(\"do it\", w);\n    return ()\n";
        let (mut core, mut rx, coord) = esc_coord("codex", &[("cx", "owner", "")], None, Some(kind));
        core.agents.get_mut("cx").unwrap().kind = "codex".into();
        core.ticket_input(Input::Open { kind: "work".into(), under: None, opener: "owner".into(), form: json!({"w": "cx"}), text: None, names: vec![], told: vec![] }).unwrap();
        // cx in a turn, given its step in it
        let d = core.ticket_due("cx");
        assert_eq!(d.len(), 1, "{d:?}");
        core.ticket_input(Input::Given { t: d[0].t, to: "cx".into(), delivery: d[0].id.clone(), upto: d[0].upto, skipped: Default::default(), place: None }).unwrap();
        working(&mut core, "cx");
        assert!(core.ticket_esc(&Who::Owner, vec!["cx".into()]).ok);
        assert_eq!(interrupts(&mut rx), 1);
        let r = core.ticket_msg(&Who::Owner, None, vec!["cx".into()], "use main", false, None, Vec::new(), Said::default());
        assert!(r.ok, "{}", r.out);
        let thread = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        assert!(core.to_hand_over().is_empty(), "nothing before the stop has landed");
        // the interrupt lands (main: Job::Interrupt, then interrupted): the message goes
        core.interrupted("cx");
        assert_eq!(core.agents["cx"].live.state, "idle");
        assert_eq!(core.to_hand_over(), vec!["cx".to_string()]);
        let due = core.ticket_due("cx");
        assert!(due.iter().any(|d| d.t == thread), "the message after the esc: {due:?}");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-36, 2: the norms' act `esc` decides who may. The base rule: an agent stops those below it (any depth), not
    /// its parent or a peer, and the refusal names the rule; the owner may stop anyone. A project's own esc.eflint
    /// (replacing agent-base's) lets more (Tovrin's consultant).
    #[test]
    fn the_policy_decides_who_may_esc() {
        let team = [("lead", "owner", "lead"), ("dev", "lead", ""), ("sub", "dev", ""), ("cons", "owner", "consultant")];
        let (mut core, mut rx, coord) = esc_coord("policy", &team, None, None);
        for a in ["lead", "dev", "sub", "cons"] {
            working(&mut core, a);
        }
        let esc = |core: &mut Core, by: &str, to: &str| core.ticket_esc(&Who::Agent(by.into()), vec![to.into()]);
        let r = esc(&mut core, "dev", "lead");
        assert!(!r.ok && r.out.contains("do not let you stop lead's turn") && r.out.contains("below(recipient, actor)"), "{}", r.out);
        assert!(!esc(&mut core, "cons", "dev").ok, "the base rule: a consultant is above nobody");
        assert_eq!(interrupts(&mut rx), 0);
        assert!(esc(&mut core, "lead", "sub").ok, "two levels down");
        assert!(esc(&mut core, "dev", "sub").ok);
        assert_eq!(interrupts(&mut rx), 1, "sub stopped once");
        // a list is checked whole before anything is stopped
        let r = core.ticket_esc(&Who::Agent("dev".into()), vec!["sub,lead".into()]);
        assert!(!r.ok, "{}", r.out);
        assert!(core.ticket_esc(&Who::Owner, vec!["cons".into(), "lead".into()]).ok);
        assert_eq!(interrupts(&mut rx), 2);
        let _ = std::fs::remove_dir_all(&coord);
        // a project's line lets its consultant stop anyone (the owner, 13 Nov: CONSULT in Tovrin)
        let (mut core, mut rx, coord) = esc_coord("project", &team, Some("Act esc\n  Actor agent\n  Recipient agent\n  Holds when below(recipient, actor)\n  Holds when role(actor, \"consultant\")\n"), None);
        working(&mut core, "dev");
        let r = esc(&mut core, "cons", "dev");
        assert!(r.ok, "{}", r.out);
        assert_eq!(interrupts(&mut rx), 1);
        assert!(!esc(&mut core, "dev", "lead").ok, "the base rule still holds for the rest");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// Line 200 (N-1324): `coord esc --all` stops every turn running at the press, Claude Code's and Codex's, with one
    /// `esc` line naming them all; one out of a turn, or a program, has nothing typed. No ticket, no post. A second
    /// press before the stops land types and writes nothing; the log replays as written.
    #[test]
    fn esc_all_stops_every_turn_with_one_line() {
        let (mut core, mut rx, coord) = esc_coord("all", &[("w1", "owner", ""), ("w2", "owner", ""), ("w3", "owner", ""), ("cx", "owner", ""), ("run", "owner", "")], None, None);
        core.agents.get_mut("cx").unwrap().kind = "codex".into();
        core.agents.get_mut("run").unwrap().kind = "program".into();
        for a in ["w1", "w2", "cx", "run"] {
            working(&mut core, a);
        }
        idle(&mut core, "w3", std::process::id() as i32);
        let log = coord.join("tickets/log.jsonl");
        let before = std::fs::read_to_string(&log).unwrap_or_default();
        let tickets = core.tickets.m.tickets.len();
        let r = core.ticket_esc_all(&Who::Owner);
        assert!(r.ok && r.out == "being stopped: cx, w1, w2", "{}", r.out);
        assert_eq!(interrupts(&mut rx), 3);
        assert_eq!(core.tickets.stopping.iter().map(String::as_str).collect::<Vec<_>>(), ["cx", "w1", "w2"]);
        assert_eq!(core.tickets.m.tickets.len(), tickets, "no ticket opened");
        let after = std::fs::read_to_string(&log).unwrap_or_default();
        let new: Vec<Value> = after[before.len()..].lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(new.len(), 1, "one line for the press: {new:?}");
        assert_eq!((new[0]["ev"].as_str(), new[0]["by"].as_str(), new[0]["agent"].as_str()), (Some("esc"), Some("owner"), Some("cx")));
        assert_eq!((new[0]["also"].clone(), new[0]["all"].clone()), (json!(["w1", "w2"]), json!(true)));
        let r = core.ticket_esc_all(&Who::Owner);
        assert!(r.ok && r.out == "already being stopped: cx, w1, w2", "{}", r.out);
        assert_eq!(interrupts(&mut rx), 0, "a second press stops nothing more");
        assert_eq!(std::fs::read_to_string(&log).unwrap_or_default(), after, "and writes nothing");
        assert!(Tickets::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), None, 1).errors.is_empty());
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// Line 200 with a resume held (proto-lead N-1397 p196, p201): Esc all stops the turn running and drops the resume
    /// of an agent held on a rate limit, a turn from before his press: neither runs, and the held one, past its reset
    /// and so given its turn again but for the press, is not handed (pressed past the reset, so that check fails
    /// without the drop: proto-reader N-1397 p204). The reconcile log names his Esc all.
    #[test]
    fn esc_all_drops_the_resume_of_an_agent_held_on_a_rate_limit() {
        let (mut core, mut rx, coord) = esc_coord("all-held", &[("w1", "owner", ""), ("w2", "owner", "")], None, None);
        working(&mut core, "w1");
        idle(&mut core, "w2", std::process::id() as i32);
        let now = crate::accounts::mover::now_s();
        let limit = crate::core::Limit { at: now - 60, until: now - 1, why: "API Error: Request rejected (429)".into(), served: None, what: None };
        core.agents.get_mut("w2").unwrap().live.limit = Some(limit);
        assert!(core.to_hand_over().contains(&"w2".to_string()), "past its reset, given its turn again but for the press");
        let r = core.ticket_esc_all(&Who::Owner);
        assert!(r.ok && r.out == "being stopped: w1; not given again the turn a rate limit held: w2", "{}", r.out);
        assert_eq!(interrupts(&mut rx), 1, "w1's turn stopped");
        assert!(core.tickets.stopping.contains("w1"));
        assert_eq!((core.agents["w2"].live.limit.as_ref(), core.agents["w2"].live.limit_wait), (None, None), "w2's resume dropped");
        assert!(!core.to_hand_over().contains(&"w2".to_string()), "past its reset, not handed");
        let log = std::fs::read_to_string(core.path("reconcile.jsonl")).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&coord);
        assert!(log.contains("stopped by owner's Esc all"), "{log}");
    }

    /// Line 200: with no agent in a turn the press types nothing and writes nothing, and says so.
    #[test]
    fn esc_all_with_none_in_a_turn_writes_nothing() {
        let (mut core, mut rx, coord) = esc_coord("all-none", &[("w1", "owner", ""), ("w2", "owner", "")], None, None);
        idle(&mut core, "w1", std::process::id() as i32);
        let log = coord.join("tickets/log.jsonl");
        let before = std::fs::read_to_string(&log).unwrap_or_default();
        let r = core.ticket_esc_all(&Who::Owner);
        assert!(r.ok && r.out == "No agent is in a turn: nothing was stopped.", "{}", r.out);
        assert_eq!(interrupts(&mut rx), 0);
        assert_eq!(std::fs::read_to_string(&log).unwrap_or_default(), before);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// Line 200: an agent's press is checked on every agent in a turn before any is stopped, by the norms' `esc`, as
    /// `coord esc A,B` is: one it may not stop and nobody is stopped. Its own turn is not among them.
    #[test]
    fn esc_all_by_an_agent_refused_on_one_stops_nobody() {
        let team = [("lead", "owner", "lead"), ("dev", "lead", ""), ("sub", "dev", ""), ("cons", "owner", "consultant")];
        let (mut core, mut rx, coord) = esc_coord("all-policy", &team, None, None);
        for a in ["lead", "dev", "sub"] {
            working(&mut core, a);
        }
        let log = coord.join("tickets/log.jsonl");
        let before = std::fs::read_to_string(&log).unwrap_or_default();
        let r = core.ticket_esc_all(&Who::Agent("dev".into()));
        assert!(!r.ok && r.out.contains("do not let you stop lead's turn") && r.out.ends_with("nobody was stopped"), "{}", r.out);
        assert_eq!(interrupts(&mut rx), 0, "sub, whom dev may stop, is not stopped either");
        assert_eq!(std::fs::read_to_string(&log).unwrap_or_default(), before);
        // the lead may stop both below it; its own turn, the one asking, goes on
        let r = core.ticket_esc_all(&Who::Agent("lead".into()));
        assert!(r.ok && r.out == "being stopped: dev, sub", "{}", r.out);
        assert_eq!(interrupts(&mut rx), 2);
        assert!(!core.tickets.stopping.contains("lead"));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// Line 200, the turn that begins between the press and the stop (paper-research N-1324 p2, (a) and (b)): the press
    /// stops the turns running then and holds nothing. w1 in a turn is stopped; w2, whose hand-over is on its way at
    /// the press, begins its turn after it and runs, as after one esc; w3, idle with the owner's message due, is handed
    /// it at once. Only w1 waits, and only until its Escape is written.
    #[test]
    fn esc_all_holds_nothing_and_a_turn_begun_after_it_runs() {
        let (mut core, mut rx, coord) = esc_coord("all-race", &[("w1", "owner", ""), ("w2", "owner", ""), ("w3", "owner", "")], None, None);
        working(&mut core, "w1");
        let pid = std::process::id() as i32;
        idle(&mut core, "w2", pid);
        idle(&mut core, "w3", pid);
        core.inflight.insert("w2".into(), crate::core::Inflight { ids: vec!["m1".into()], tries: 0, resume: None });
        let r = core.ticket_msg(&Who::Owner, None, vec!["w3".into()], "use main", false, None, Vec::new(), Said::default());
        assert!(r.ok, "{}", r.out);
        let log = coord.join("tickets/log.jsonl");
        let before = std::fs::read_to_string(&log).unwrap_or_default();
        let r = core.ticket_esc_all(&Who::Owner);
        assert!(r.ok && r.out == "being stopped: w1", "{}", r.out);
        assert_eq!(interrupts(&mut rx), 1);
        let after = std::fs::read_to_string(&log).unwrap_or_default();
        let new: Vec<Value> = after[before.len()..].lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(new.len(), 1, "{new:?}");
        assert_eq!((new[0]["agent"].as_str(), new[0].get("also"), new[0]["all"].as_bool()), (Some("w1"), None, Some(true)));
        // nothing held for the others: w3 takes its message now
        assert_eq!(core.to_hand_over(), vec!["w3".to_string()]);
        // w2's hand-over is taken and its turn begins: it runs; no second Escape, no second line. What the record gains
        // is w3's message handed over by the message runtime, as Esc all holds nothing: w3 is `free`, its delivery is
        // opened and acted on, `deliver` of the owner's post (build 31's check, paper-research N-1396.p191); and w2's
        // turn's beginning, as every turn's (`turn_began`, where a copy of it is cut: paper-research N-1446 p3020, 1)
        core.inflight.remove("w2");
        core.begin_turn("w2");
        assert_eq!(core.agents["w2"].live.state, "working");
        assert_eq!(interrupts(&mut rx), 0);
        assert!(!core.tickets.stopping.contains("w2"));
        let now = std::fs::read_to_string(&log).unwrap_or_default();
        assert!(now.starts_with(&after), "{now}");
        let more: Vec<Value> = now[after.len()..].lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        let seen: Vec<_> = more.iter().map(|l| (l["ev"].as_str(), l["name"].as_str(), l["value"]["agent"].as_str())).collect();
        let w3 = |name| (Some("event"), Some(name), Some("w3"));
        assert_eq!(seen, [w3("free"), w3("delivery_opened"), w3("delivery_acted"), (Some("event"), Some(docket_machine::TURN_BEGAN), Some("w2"))], "{more:?}");
        assert_eq!((more[2]["value"]["act"].as_str(), &more[2]["value"]["items"]), (Some("deliver"), &json!(["N-1.p1"])));
        assert!(more[..3].iter().all(|l| !l.to_string().contains("\"w1\"") && !l.to_string().contains("\"w2\"")), "{more:?}");
        // w1's Escape is written: the press is over
        core.interrupted("w1");
        assert!(core.tickets.stopping.is_empty());
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// Line 200 on core5-born (consult-3, N-1324 p11): after an `Esc all` each stopped agent's transcript ends on the
    /// interrupt. A coordd start's first read takes it as a cut once more (f7636fec: an interrupt from before the
    /// follower began with nothing after it is a cut); for an agent that had no turn since, that second cut writes
    /// nothing and changes nothing due.
    #[test]
    fn a_start_after_esc_all_reads_the_last_interrupt_and_changes_nothing() {
        let (mut core, mut rx, coord) = esc_coord("all-start", &[("w1", "owner", "")], None, None);
        let pid = std::process::id() as i32;
        idle(&mut core, "w1", pid);
        let r = core.ticket_msg(&Who::Owner, None, vec!["w1".into()], "use main", false, None, Vec::new(), Said::default());
        assert!(r.ok, "{}", r.out);
        let d = core.ticket_due("w1").pop().expect("due");
        core.ticket_input(Input::Given { t: d.t, to: "w1".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).unwrap();
        // its transcript: the message, the turn, the interrupt last
        let ticket = core.tickets.m.id(d.t);
        let lines = [
            json!({"type": "user", "isMeta": true, "origin": {"kind": "channel"}, "message": {"role": "user", "content": format!("<channel source=\"coord\" from=\"owner\" id=\"{}\" kind=\"thread\" ticket=\"{ticket}\">\nowner wrote:\nuse main\n</channel>", d.id)}, "timestamp": "2026-11-20T07:20:00Z"}),
            json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": "switching to main"}]}, "timestamp": "2026-11-20T07:20:05Z"}),
            json!({"type": "user", "message": {"role": "user", "content": [{"type": "text", "text": "[Request interrupted by user]"}]}, "timestamp": "2026-11-20T07:20:09Z"}),
        ];
        let transcript = coord.join("w1.jsonl");
        let written = |n: usize| std::fs::write(&transcript, lines[..n].iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
        // the live follower reads the message where the session wrote it: its given row has that place
        written(1);
        let mut live = crate::follow::Follower::new(&transcript.display().to_string());
        let r = live.read();
        let path = live.path.clone();
        core.transcript_read("w1", r.at.into_iter().map(|(id, o, l)| (id, path.clone(), o, l)).collect(), Vec::new(), Vec::new(), &[], r.cut, &r.begins, r.running_old);
        assert!(core.tickets.m.tickets[&d.t].given.iter().any(|g| g.to == "w1" && g.delivery == d.id && g.place.is_some()), "its place read live");
        working(&mut core, "w1");
        assert!(core.ticket_esc_all(&Who::Owner).ok);
        assert_eq!(interrupts(&mut rx), 1);
        core.interrupted("w1");
        core.agents.get_mut("w1").unwrap().live.state = "idle".into();
        core.ticket_input(Input::TurnCut { agent: "w1".into() }).unwrap();
        written(3);
        // its turn began on his message and was cut before answering it: the cut leaves it due again, as a turn's end
        // that did not answer it (paper-research N-412.p2202, 2), to be handed again at the next `free`
        let due: Vec<String> = core.ticket_due("w1").into_iter().map(|d| d.id).collect();
        assert_eq!(due, vec![d.id.clone()], "the stopped turn's message, his, comes again");
        // coordd starts again: the record replayed, a new follower over the whole transcript
        let log = coord.join("tickets/log.jsonl");
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        core.load_tickets();
        idle(&mut core, "w1", pid);
        // what the start itself writes (a record's first start on this engine: one `held_handed` line) is before the read
        let before = std::fs::read_to_string(&log).unwrap();
        assert_eq!(core.ticket_due("w1").into_iter().map(|d| d.id).collect::<Vec<_>>(), due, "the replay has it due too");
        let mut f = crate::follow::Follower::new(&transcript.display().to_string());
        let r = f.read();
        assert!(r.cut.is_some(), "f7636fec: the interrupt with nothing after it is read as a cut");
        let path = f.path.clone();
        let at = r.at.into_iter().map(|(id, o, l)| (id, path.clone(), o, l)).collect();
        core.transcript_read("w1", at, Vec::new(), Vec::new(), &[], r.cut, &r.begins, r.running_old);
        assert_eq!(std::fs::read_to_string(&log).unwrap(), before, "the second cut writes nothing");
        assert_eq!(core.ticket_due("w1").into_iter().map(|d| d.id).collect::<Vec<_>>(), due, "and changes nothing due");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// Line 327 (N-1324): the owner's Hold and Release, one `mail_hold` line each way; the snapshot says since when (the
    /// window's Hold and its muted line), and a restart reads the hold back from the record. A second Hold, or a
    /// Release with none, writes nothing and says so; an agent is refused and writes nothing.
    #[test]
    fn the_owners_hold_on_agents_mail_is_one_line_each_way() {
        let (mut core, _rx, coord) = esc_coord("hold-mail", &[("w1", "owner", "")], None, None);
        let log = coord.join("tickets/log.jsonl");
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let lines = || std::fs::read_to_string(&log).unwrap_or_default().lines().filter(|l| l.contains("\"mail_hold\"")).count();
        let shown = |core: &Core| match core.snapshot().body {
            Body::Snapshot(s) => s.mail_hold,
            other => panic!("{other:?}"),
        };
        let r = core.ticket_mail_hold(&Who::Agent("w1".into()), true);
        assert!(!r.ok && r.out == "coord hold-mail: holding agents' mail is the owner's", "{}", r.out);
        let r = core.ticket_mail_hold(&Who::Owner, false);
        assert!(r.ok && r.out == "Agents' mail is not held.", "{}", r.out);
        assert_eq!((lines(), shown(&core)), (0, None));
        let r = core.ticket_mail_hold(&Who::Owner, true);
        assert!(r.ok && r.out == "Agents' mail is held: until Release, only the owner's posts pass.", "{}", r.out);
        let at = core.tickets.m.mail_hold.as_ref().unwrap().at.clone();
        assert_eq!((lines(), shown(&core)), (1, Some(at.clone())));
        let r = core.ticket_mail_hold(&Who::Owner, true);
        assert!(r.ok && r.out == format!("Agents' mail is held already, since {at}."), "{}", r.out);
        assert_eq!(lines(), 1);
        // a restart: the record's line gives the hold back before any round
        let again = Tickets::load(&coord.display().to_string(), &base, None, 1);
        assert!(again.errors.is_empty(), "{:?}", again.errors);
        assert_eq!(again.m.mail_hold, core.tickets.m.mail_hold);
        let r = core.ticket_mail_hold(&Who::Owner, false);
        assert!(r.ok && r.out == format!("Agents' mail is let go (held since {at})."), "{}", r.out);
        assert_eq!((lines(), shown(&core)), (2, None));
        assert_eq!(Tickets::load(&coord.display().to_string(), &base, None, 1).m.mail_hold, None);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// I-1457: the release his window runs and coordd's own, one `window_release` line when either changes. An agent
    /// is refused, and so is a name that is not one folder's; the same release again writes nothing; its folder's
    /// `contains.txt`, written by the one-time step after the first line, makes the next hello write the list; a
    /// restart reads the last line back, and a `back` to the older release is one more line; an engine put in alone
    /// (`live --no-exe`) is one more line, and a build it contains is in his window (p610); an edge's list is read
    /// from releases/edges.
    #[test]
    fn the_window_release_is_one_line_when_it_changes() {
        let (mut core, _rx, coord) = esc_coord("window-release", &[("w1", "owner", "")], None, None);
        let log = coord.join("tickets/log.jsonl");
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let lines = || std::fs::read_to_string(&log).unwrap_or_default().lines().filter(|l| l.contains("\"window_release\"")).count();
        let rel = coord.join("releases");
        let (older, newer, later) = ("20261120-220601-5815e775", "20261120-232357-a633aa96", "20261121-003000-b1e55ed0");
        for r in [older, newer, later] {
            std::fs::create_dir_all(rel.join(r)).unwrap();
        }
        let own = rel.join(newer);
        let r = core.window_release(&Who::Agent("w1".into()), newer, None, Some(&own));
        assert!(!r.ok && r.out == "coordd: the release his window runs is his window's to say", "{}", r.out);
        let r = core.window_release(&Who::Owner, "../20261120-232357-a633aa96", None, Some(&own));
        assert!(!r.ok && r.out == "coordd: not a release's name: ../20261120-232357-a633aa96", "{}", r.out);
        assert_eq!(lines(), 0);
        let r = core.window_release(&Who::Owner, newer, None, Some(&own));
        assert!(r.ok && r.out == format!("His window runs {newer}; coordd runs {newer}."), "{}", r.out);
        assert_eq!(lines(), 1);
        let r = core.window_release(&Who::Owner, newer, None, Some(&own));
        assert!(r.ok && r.out == format!("His window runs {newer}, as recorded."), "{}", r.out);
        assert_eq!(lines(), 1);
        // the one-time step writes the folder's list: the next hello records it, in both halves
        std::fs::write(rel.join(newer).join("contains.txt"), format!("{older}\n20261119-101752-e0eb4d1\n")).unwrap();
        core.window_release(&Who::Owner, newer, None, Some(&own));
        core.window_release(&Who::Owner, newer, None, Some(&own));
        assert_eq!(lines(), 2);
        let w = core.tickets.m.window_release.clone().unwrap();
        assert_eq!(w.window.contains.iter().map(String::as_str).collect::<Vec<_>>(), ["20261119-101752-e0eb4d1", older]);
        assert_eq!(w.engine.as_ref(), Some(&w.window));
        // a restart reads it back, and the same hello then writes nothing
        let again = Tickets::load(&coord.display().to_string(), &base, None, 1);
        assert!(again.errors.is_empty(), "{:?}", again.errors);
        assert_eq!(again.m.window_release.as_ref(), Some(&w));
        // `back`: the older release, with no list of its own, is one more line; coordd still runs the newer one
        core.window_release(&Who::Owner, older, None, Some(&own));
        let w = core.tickets.m.window_release.clone().unwrap();
        assert_eq!((lines(), w.window.release.as_str(), w.window.contains.len(), w.engine.map(|e| e.release)), (3, older, 0, Some(newer.to_string())));
        // his window on the older release, then `live --no-exe` puts in a later engine alone: coordd restarts from its
        // folder, the bridge's stream ends, and the next pipe's hello writes one line; a build it contains is in
        std::fs::write(rel.join(later).join("contains.txt"), format!("{older}\n{newer}\n")).unwrap();
        let r = core.window_release(&Who::Owner, older, None, Some(&rel.join(later)));
        assert!(r.ok && r.out == format!("His window runs {older}; coordd runs {later}."), "{}", r.out);
        let w = core.tickets.m.window_release.clone().unwrap();
        assert_eq!((lines(), w.window.has(later), w.has(later), w.has(newer)), (4, false, true, true));
        // a window-only edge has no folder: its list is releases/edges/NAME.txt
        let edge = "20261121-010203-edge-c0ffee1-on-b1e55ed0";
        std::fs::create_dir_all(rel.join("edges")).unwrap();
        std::fs::write(rel.join("edges").join(format!("{edge}.txt")), format!("{older}\n{newer}\n")).unwrap();
        core.window_release(&Who::Owner, edge, None, Some(&rel.join(later)));
        let w = core.tickets.m.window_release.clone().unwrap();
        assert_eq!((lines(), w.window.release.as_str(), w.window.has(newer), w.window.has(older)), (5, edge, true, true));
        // his window's own time (paper-research N-1446 p1392): the line carries it in UTC as the record writes a time,
        // a window started again on the same build says another and writes nothing, and one that is not a time is not
        // written, the answer saying so
        let last = || std::fs::read_to_string(&log).unwrap().lines().filter(|l| l.contains("\"window_release\"")).last().map(|l| serde_json::from_str::<Value>(l).unwrap()).unwrap();
        core.window_release(&Who::Owner, newer, Some("2026-11-21T10:34:49+01:00"), Some(&rel.join(later)));
        assert_eq!((lines(), last()["value"]["release"].as_str(), last()["value"]["since"].as_str()), (6, Some(newer), Some("2026-11-21T09:34:49Z")));
        let r = core.window_release(&Who::Owner, newer, Some("2026-11-21T11:00:00Z"), Some(&rel.join(later)));
        assert!(r.ok && r.out == format!("His window runs {newer}, as recorded."), "{}", r.out);
        assert_eq!(lines(), 6);
        let r = core.window_release(&Who::Owner, older, Some("not yet"), Some(&rel.join(later)));
        assert!(r.ok && r.out.ends_with("His window's time \"not yet\" is not a time, so the line has none."), "{}", r.out);
        assert_eq!((lines(), last()["value"].get("since")), (7, None));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// The start cases' setup (paper-research N-1397 p43): w1 given the owner's "which branch?", its live follower
    /// reading the message where the session wrote it, its turn running, the prompt hook come. Returned with the
    /// thread and the transcript's lines: the message, its answer, the Stop hook's summary.
    #[allow(clippy::type_complexity)]
    fn turn_running(tag: &str) -> (Core, tokio::sync::mpsc::UnboundedReceiver<crate::core::Job>, crate::follow::Follower, std::path::PathBuf, std::path::PathBuf, u64, Vec<Value>) {
        let (mut core, rx, coord) = esc_coord(tag, &[("w1", "owner", "")], None, None);
        idle(&mut core, "w1", std::process::id() as i32);
        let r = core.ticket_msg(&Who::Owner, None, vec!["w1".into()], "which branch?", false, None, Vec::new(), Said::default());
        assert!(r.ok, "{}", r.out);
        let d = core.ticket_due("w1").pop().expect("due");
        core.ticket_input(Input::Given { t: d.t, to: "w1".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).unwrap();
        let ticket = core.tickets.m.id(d.t);
        let lines = vec![
            json!({"type": "user", "isMeta": true, "origin": {"kind": "channel"}, "message": {"role": "user", "content": format!("<channel source=\"coord\" from=\"owner\" id=\"{}\" kind=\"thread\" ticket=\"{ticket}\">\nowner wrote:\nwhich branch?\n</channel>", d.id)}, "timestamp": "2026-11-20T07:20:00Z"}),
            json!({"type": "assistant", "message": {"role": "assistant", "stop_reason": "end_turn", "content": [{"type": "text", "text": "main, at its tip"}]}, "timestamp": "2026-11-20T07:20:05Z"}),
            json!({"type": "system", "subtype": "stop_hook_summary", "timestamp": "2026-11-20T07:20:06Z"}),
        ];
        let transcript = coord.join("w1.jsonl");
        std::fs::write(&transcript, format!("{}\n", lines[0])).unwrap();
        let mut live = crate::follow::Follower::new(&transcript.display().to_string());
        let r = live.read();
        let path = live.path.clone();
        core.transcript_read("w1", r.at.into_iter().map(|(id, o, l)| (id, path.clone(), o, l)).collect(), Vec::new(), Vec::new(), &[], r.cut, &r.begins, r.running_old);
        recovered(&mut core);
        (core, rx, live, coord, transcript, d.t, lines)
    }

    /// w1 as live.json keeps it (`Core::recover`): working since before its turn's end, its hooks last said busy.
    fn recovered(core: &mut Core) {
        core.agents.get_mut("w1").unwrap().live = crate::core::Live { pid: Some(std::process::id() as i32), state: "working".into(), since: "2026-11-20T07:20:00Z".into(), turn: 2, hook_busy: Some(true), ..Default::default() };
    }

    /// coordd started on COORD's record as `main` starts it: `recover`, then `load_tickets`.
    fn started(coord: &std::path::Path) -> Core {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        recovered(&mut core);
        core.load_tickets();
        core
    }

    /// One round of the follow loop (`main::follow`) on w1's follower F, GREW standing for when its transcript last
    /// grew (F's own when None): whether coordd's count ended w1's turn in it.
    fn follow_round(core: &mut Core, f: &mut crate::follow::Follower, grew: Option<std::time::Instant>) -> bool {
        let r = f.read();
        let path = f.path.clone();
        let output: Vec<(String, u64, u64)> = f.take_outputs().into_iter().map(|(o, l)| (path.clone(), o, l)).collect();
        let start_end = f.take_start_end().map(|(o, l)| Loc { file: path.clone(), offset: o, len: l });
        let output: Vec<(String, u64, u64)> = core.start_output("w1", start_end).into_iter().chain(output).collect();
        let at = r.at.into_iter().map(|(id, o, l)| (id, path.clone(), o, l)).collect();
        core.transcript_read("w1", at, Vec::new(), Vec::new(), &output, r.cut, &r.begins, r.running_old);
        let ended = core.turn_ended_now("w1", f.ended, &f.running_since, grew.unwrap_or(f.grew), r.cut.is_some(), f.unanswered());
        let ends = ended && core.agents["w1"].live.state == "working" && core.end_turn("w1").is_some();
        if ends {
            core.owe_output("w1");
            if output.is_empty() {
                core.turn_end_without_stop("w1", f.said_span().map(|(o, l)| Loc { file: path.clone(), offset: o, len: l }));
            }
        }
        if !output.is_empty() || f.ended || r.cut.is_some() {
            core.output_read("w1");
        }
        ends
    }

    /// W1's posts on T, as read.
    fn posts_of_w1(core: &Core, t: u64) -> Vec<String> {
        core.tickets.m.tickets[&t].posts.iter().filter(|p| p.by == "w1").filter_map(|p| crate::label::read(&p.text)).collect()
    }

    /// a) A turn that ends while coordd is stopped, its Stop hook finding no coordd (paper-research N-1397 p43): at the
    /// start, the first read of its transcript (a new follower) posts its last message as at any turn's end, once,
    /// the record holding the turn still; coordd's count ends it by the 60 s rule (`presence_agrees`, consult-3 N-1397
    /// p53, 1), and the agent is idle.
    #[test]
    fn a_turn_ended_while_coordd_was_stopped_is_replied_to_once_at_its_start() {
        let (core, _rx, _live, coord, transcript, t, lines) = turn_running("stopped-end");
        drop(core);
        std::fs::write(&transcript, lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
        let mut core = started(&coord);
        assert!(!core.tickets.m.given_since("w1").is_empty(), "the record holds the turn after the start");
        let mut f = crate::follow::Follower::new(&transcript.display().to_string());
        assert!(!follow_round(&mut core, &mut f, None), "the hooks' turn runs on in coordd's count until its rule");
        assert_eq!(posts_of_w1(&core, t), vec!["main, at its tip".to_string()], "its last message posted at the first read");
        assert!(core.tickets.m.given_since("w1").is_empty(), "the record's turn is over");
        // coord-pty tells the screen idle once coordd is up; 60 s on, the transcript unchanged: the screen decides
        let ago = std::time::Instant::now().checked_sub(std::time::Duration::from_secs(61)).unwrap();
        let a = core.agents.get_mut("w1").unwrap();
        a.live.screen_busy = Some(false);
        a.live.disagree = Some((ago, true));
        assert!(follow_round(&mut core, &mut f, Some(ago)), "the 60 s rule ends the turn");
        assert_eq!(posts_of_w1(&core, t), vec!["main, at its tip".to_string()], "posted once");
        assert_eq!(core.agent_state("w1"), Some(crate::core::AgentState::Idle), "and the agent is idle");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// b) The reverse (consult-3, N-1397 p53): the turn's end taken and posted while coordd ran, then a stop and a
    /// start, live.json still saying working: the first read of a new follower posts nothing again, writes nothing.
    #[test]
    fn a_turn_end_taken_before_coordd_stopped_is_not_replied_to_again_at_its_start() {
        let (mut core, _rx, mut live, coord, transcript, t, lines) = turn_running("taken-end");
        std::fs::write(&transcript, lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
        core.hook("w1", "stop", &json!({}));
        assert!(follow_round(&mut core, &mut live, None), "the Stop hook and the transcript end the turn");
        assert_eq!(posts_of_w1(&core, t), vec!["main, at its tip".to_string()]);
        drop(core);
        let mut core = started(&coord);
        let log = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap();
        let mut f = crate::follow::Follower::new(&transcript.display().to_string());
        follow_round(&mut core, &mut f, None);
        assert_eq!(posts_of_w1(&core, t), vec!["main, at its tip".to_string()], "nothing posted again");
        assert_eq!(std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap(), log, "the start's read writes nothing");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// c) A delivery handed after the last turn's end, then coordd stopped before the session showed it (consult-3,
    /// N-1397 p60, 3): at the start the window's last ended output is the earlier turn's, and the delivery handed after
    /// it is the next turn's (`turn_holds`, `handed_after`), so nothing is posted; it stays given to w1, and the rule
    /// for a giving never read makes it due again (`Core::ticket_unread`).
    #[test]
    fn a_delivery_handed_after_the_last_turns_end_gets_no_old_reply_at_a_start() {
        let (mut core, _rx, mut live, coord, transcript, t, lines) = turn_running("handed-after");
        std::fs::write(&transcript, lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
        core.hook("w1", "stop", &json!({}));
        assert!(follow_round(&mut core, &mut live, None));
        assert_eq!(posts_of_w1(&core, t), vec!["main, at its tip".to_string()]);
        let r = core.ticket_msg(&Who::Owner, None, vec!["w1".into()], "and the release?", false, None, Vec::new(), Said::default());
        assert!(r.ok, "{}", r.out);
        let d = core.ticket_due("w1").pop().expect("due");
        core.ticket_input(Input::Given { t: d.t, to: "w1".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).unwrap();
        drop(core);
        let mut core = started(&coord);
        let mut f = crate::follow::Follower::new(&transcript.display().to_string());
        follow_round(&mut core, &mut f, None);
        assert_eq!(posts_of_w1(&core, t), vec!["main, at its tip".to_string()], "nothing more on the earlier thread");
        assert!(posts_of_w1(&core, d.t).is_empty(), "the earlier turn's message is no reply to what it never saw");
        assert_eq!(core.tickets.m.given_since("w1"), vec![d.id.clone()], "still given to it");
        core.ticket_unread("w1");
        assert!(core.ticket_due("w1").iter().any(|e| e.id == d.id), "and due again once found never read");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-412 ask 5: `coord slow project on|off` sets slow over the whole project. The owner is not asked; an agent is
    /// held to the policy's `set-slow` / `lift-slow` (agent-base/policy/slow.eflint: no agent), the refusal naming
    /// the rule; a project's own slow.eflint lets its lead. A ticket's `coord slow T` goes on as before.
    #[test]
    fn slow_over_the_project_is_the_owners_and_the_policys() {
        let team = [("lead", "owner", "lead"), ("dev", "lead", "")];
        let (mut core, _rx, coord) = esc_coord("slow", &team, None, None);
        let r = core.ticket_slow(&Who::Agent("lead".into()), "project", true);
        assert!(!r.ok && r.out.contains("do not let you set slow over the project") && r.out.contains("role(actor, \"owner\")"), "{}", r.out);
        assert!(core.tickets.m.slowed.is_empty());
        let r = core.ticket_slow(&Who::Owner, "project", true);
        assert!(r.ok && r.out.starts_with("Slow is set over the project"), "{}", r.out);
        assert!(core.tickets.m.slowed.contains("project"));
        let coord_types::Body::Snapshot(s) = core.snapshot().body else { panic!() };
        assert_eq!(s.slowed, vec!["project".to_string()], "the app's menu reads it");
        let log = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap();
        assert!(log.lines().any(|l| l.contains("\"ev\":\"slowed\"") && l.contains("\"scope\":\"project\"")), "{log}");
        assert!(!core.ticket_slow(&Who::Agent("dev".into()), "project", false).ok, "lifting is held to the policy too");
        assert!(core.ticket_slow(&Who::Owner, "project", false).ok);
        assert!(core.tickets.m.slowed.is_empty());
        // a ticket's switch: the owner's, as before
        core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("a thread".into())), names: vec!["dev".into()], told: vec![] }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        let id = core.tickets.m.id(t);
        assert!(!core.ticket_slow(&Who::Agent("lead".into()), &id, true).ok);
        assert!(core.ticket_slow(&Who::Owner, &id, true).ok && core.tickets.m.tickets[&t].slow);
        let _ = std::fs::remove_dir_all(&coord);
        // a project's own slow.eflint lets its lead
        let esc = std::fs::read_to_string(format!("{}/../../agent-base/policy/esc.eflint", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let (mut core, _rx, coord) = esc_coord("slow-lead", &team, Some(&esc), None);
        let base = std::fs::read_to_string(format!("{}/../../agent-base/policy/slow.eflint", env!("CARGO_MANIFEST_DIR"))).unwrap();
        std::fs::write(coord.join("files/policy/slow.eflint"), base.replace("Holds when role(actor, \"owner\")", "Holds when role(actor, \"lead\")")).unwrap();
        let r = core.ticket_slow(&Who::Agent("lead".into()), "project", true);
        assert!(r.ok, "{}", r.out);
        assert!(!core.ticket_slow(&Who::Agent("dev".into()), "project", false).ok);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// docket-edge m1153: a turn cut by esc has no output; the cut clears what it was given, so the esc's words on
    /// another ticket are handed at once, not after a minute as unread.
    #[test]
    fn a_cut_turn_lets_the_next_words_through_at_once() {
        use crate::core::{Core, Live};
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-cut-words-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), turn: 1, ..Default::default() };
        let say = |core: &mut Core, text: &str| {
            let v = json!({"from": "owner", "to": "w2", "typed": ["w2"], "names": [], "text": text, "addressed": {"head": "Unnamed"}});
            core.ticket_input(Input::Event { name: "owner_message".into(), value: v }).unwrap();
            let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
            core.ticket_answer(docket_machine::WATCHER, t, json!({"head": "Thread"}), Text::New(String::new())).unwrap();
        };
        say(&mut core, "compute something long");
        let d = core.ticket_due("w2").remove(0);
        core.ticket_given("w2", &d);
        say(&mut core, "stop, wrong branch");
        assert!(core.to_hand_over().is_empty(), "held behind what the turn was given");
        core.ticket_turn_cut("w2");
        assert!(core.ticket_given_since("w2").is_empty());
        assert_eq!(core.to_hand_over(), vec!["w2".to_string()]);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// §16 at coordd: words typed to nobody go to whoever fills lead, and intake reads the owner's open tickets; his
    /// words for a closed thread open a new one that follows it; the star and "nothing to answer" are his alone.
    #[test]
    fn the_owners_words_to_nobody_a_closed_thread_the_star_and_nothing_to_answer() {
        use crate::core::{Core, Who};
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-s16-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let reg = |id: &str, name: &str| format!("{{\"ev\":\"add\",\"id\":\"{id}\",\"name\":\"{name}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}}\n");
        std::fs::write(coord.join("registry.jsonl"), reg("AGT-001", "lead") + &reg("AGT-002", "fixer")).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.tickets.tasks = Some(tx);
        // the events Say raises, run as coordd's task runner runs them
        let pump = |core: &mut Core, rx: &mut tokio::sync::mpsc::UnboundedReceiver<Task>| {
            while let Ok(t) = rx.try_recv() {
                if let Task::Event { name, value } = t {
                    core.ticket_input(Input::Event { name, value }).unwrap();
                }
            }
        };
        // who fills lead: the policy's answer, asked off the lock
        let places = crate::policy::Places { base: base.clone(), coord: coord.display().to_string(), ..Default::default() };
        let facts: Vec<(String, Vec<String>)> = [("agent", vec!["lead"]), ("role", vec!["lead", "lead"]), ("running", vec!["lead"]), ("agent", vec!["fixer"]), ("role", vec!["fixer", "fixer"]), ("running", vec!["fixer"])]
            .into_iter()
            .map(|(f, a)| (f.to_string(), a.into_iter().map(str::to_string).collect()))
            .collect();
        assert_eq!(lead_of(&places, facts).as_deref(), Some("lead"));
        let last = |core: &Core| *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        let intake_open = |core: &mut Core, t: u64| {
            let due = core.ticket_due(docket_machine::WATCHER);
            let d = due.iter().find(|d| d.t == t && matches!(&d.what, docket_machine::What::Step { call, .. } if call == "intake")).expect("intake owed");
            core.clerk_job(d).expect("intake's job").open
        };
        // a thread to the fixer, which the lead is not on
        assert!(core.ticket_say(&Who::Owner, "fixer".into(), vec!["fixer".into()], vec![], "the parser drops comments".into(), None).ok);
        pump(&mut core, &mut rx);
        let a = last(&core);
        let aid = core.tickets.m.id(a);
        core.ticket_answer(docket_machine::WATCHER, a, json!({"head": "Thread"}), Text::New(String::new())).unwrap();
        // typed to the lead: intake reads the lead's tickets; typed to nobody (to the lead, as lead): the owner's
        assert!(core.ticket_say(&Who::Owner, "lead".into(), vec!["lead".into()], vec![], "and the lexer?".into(), None).ok);
        pump(&mut core, &mut rx);
        let b = last(&core);
        assert!(!intake_open(&mut core, b).contains(&aid), "{}", intake_open(&mut core, b));
        assert!(core.ticket_say(&Who::Owner, "lead".into(), vec![], vec![], "and the lexer, again?".into(), None).ok);
        pump(&mut core, &mut rx);
        let c = last(&core);
        assert!(intake_open(&mut core, c).contains(&aid), "{}", intake_open(&mut core, c));
        assert_eq!(core.tickets.m.tickets[&c].form["typed"], json!([]));
        // the owner's words from the thread's foot, typed to nobody: lead alone on it, no route call; lead and fixer,
        // route as before (paper-research m1418)
        core.ticket_answer(docket_machine::WATCHER, b, json!({"head": "Thread"}), Text::New(String::new())).unwrap();
        let bid = core.tickets.m.id(b);
        let route_owed = |core: &Core| core.ticket_due(docket_machine::WATCHER).iter().any(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "route"));
        assert!(core.ticket_post(&Who::Owner, &bid, "and the tests".into(), None, None, Said::default(), None, false).ok);
        assert!(!route_owed(&core), "lead alone: no call");
        assert!(core.ticket_due("lead").iter().any(|d| d.t == b), "lead is given the words");
        core.ticket_input(Input::Post { t: b, by: "lead".into(), text: Text::New("@fixer have a look".into()), names: vec!["fixer".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        assert!(core.ticket_post(&Who::Owner, &bid, "and the docs".into(), None, None, Said::default(), None, false).ok);
        assert!(route_owed(&core), "lead and fixer: route");
        // undo: the owner's; with no answer of the minder's to undo, refused
        assert!(!core.ticket_undo(&Who::Agent("lead".into()), &aid).ok);
        let r = core.ticket_undo(&Who::Owner, &aid);
        assert!(!r.ok && r.out.contains("no answer at a hole to undo"), "{}", r.out);
        // the star: an agent's is kept as its own and is not the owner's (N-101.p481); the owner's marks the thread;
        // a post's is the post's
        assert!(core.ticket_star(&Who::Agent("lead".into()), &aid, true).ok);
        assert!(!core.tickets.m.tickets[&a].starred);
        assert!(core.ticket_star(&Who::Owner, &aid, true).ok);
        assert!(core.tickets.m.tickets[&a].starred);
        assert!(core.ticket_star(&Who::Owner, &format!("{aid}.p1"), true).ok);
        assert!(!core.ticket_star(&Who::Owner, &format!("{aid}.p99"), true).ok);
        // a post's id read as every reader reads one (`post_ref`, consult-3 N-1396 p280): a sign is no post, here or
        // in its label
        let r = core.ticket_star(&Who::Owner, &format!("{aid}.p+1"), true);
        assert!(!r.ok && r.out == format!("coord star: {aid}.p+1 is no post"), "{}", r.out);
        assert_eq!(crate::label::of(&core.tickets.m, &[], &format!("{aid}.p1")).is, "post");
        assert_eq!(crate::label::of(&core.tickets.m, &[], &format!("{aid}.p+1")).is, "");
        // a request's ticket is its id as written (`Machine::ticket_named`, consult-3 N-1396 p310): its number alone or
        // in small letters names none, here or in its label, and the answer says how its id is written (consult-3
        // N-1397.p100); an agent's name that reads as its number is refused plainly
        for other in [a.to_string(), aid.to_lowercase()] {
            let r = core.ticket_star(&Who::Owner, &other, true);
            assert!(!r.ok && r.out == format!("There is no ticket {other}: its id is written {aid}."), "{other}: {}", r.out);
            assert_eq!(crate::label::of(&core.tickets.m, &[], &other).is, "", "{other}");
        }
        let r = core.ticket_star(&Who::Owner, &format!("consult-{a}"), true);
        assert!(!r.ok && r.out == format!("There is no ticket consult-{a}."), "{}", r.out);
        let stars = &core.tickets.m.tickets[&a].stars;
        assert_eq!(stars.iter().map(|x| (x.post, x.by.as_str())).collect::<Vec<_>>(), vec![(None, "lead"), (None, "owner"), (Some(1), "owner")]);
        let found = core.ticket_find("", true, false).unwrap_or_else(|_| unreachable!());
        assert!(found.ok && found.out.lines().count() == 3 && found.out.lines().next().unwrap().contains("\"owner\""), "{}", found.out);
        let found = match core.ticket_find("THE tests", false, false) { Err(f) => find_words(f), Ok(r) => r };
        assert!(found.ok && found.out.contains(&format!("\"{bid}.p")) && found.out.contains("and the tests"), "{}", found.out);
        // each hit's title is its ticket's as its view gives it, though no view is built for it (docket-edge6, N-412 p2161)
        for h in found.out.lines().map(|l| serde_json::from_str::<coord_types::FindHit>(l).unwrap()) {
            assert_eq!(h.title, core.tickets.m.view(h.t).unwrap().title, "{}", h.id);
        }
        // a post whose text is a part of a ticket's text is found by the words of that part (docket-core N-247.p2)
        let part = docket_machine::Text::Part { part: docket_machine::Part { t: b, at: 8, len: 5 } };
        assert_eq!(core.tickets.m.text(&part), Some(Text::New("lexer".into())), "b's text: {:?}", core.tickets.m.tickets[&b].form);
        core.ticket_input(Input::Post { t: b, by: "lead".into(), text: part, names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let found = match core.ticket_find("lexer", false, false) { Err(f) => find_words(f), Ok(r) => r };
        assert!(found.ok && found.out.lines().any(|l| l.contains("\"by\":\"lead\"") && l.contains("\"words\":\"lexer\"")), "{}", found.out);
        // an answer the owner gave in the app, with its fields and no text, is found by its fields (paper-research N-101
        // p1022: what the search before a question looks hardest for)
        let mut answer = core.tickets.m.tickets[&b].posts.last().unwrap().clone();
        answer.n += 1;
        (answer.by, answer.text, answer.head) = ("owner".into(), Text::New(String::new()), Some(json!({"head": "Answer", "text": "keep the lexer's tokens"})));
        core.tickets.m.tickets.edit().get_mut(&b).unwrap().posts.push(answer);
        let found = match core.ticket_find("lexer's tokens", false, false) { Err(f) => find_words(f), Ok(r) => r };
        assert!(found.ok && found.out.lines().any(|l| l.contains("\"by\":\"owner\"") && l.contains("\"words\":\"keep the lexer's tokens\"")), "{}", found.out);
        // nothing to answer: the owner's; on a post that asks nothing it changes nothing
        assert!(!core.ticket_nothing_to_answer(&Who::Agent("lead".into()), &aid, 1).ok);
        let r = core.ticket_nothing_to_answer(&Who::Owner, &aid, 1);
        assert!(r.ok && r.out.contains("asks nothing open"), "{}", r.out);
        assert!(!core.ticket_nothing_to_answer(&Who::Owner, &aid, 9).ok, "no post 9");
        // a post's asks withdrawn (paper-research N-447 p5): its author's or an agent's above it; the owner names his act
        core.ticket_input(Input::Post { t: a, by: "fixer".into(), text: Text::New("which branch?".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let fx = core.tickets.m.tickets[&a].posts.last().unwrap().n;
        let r = core.ticket_withdraw_asks(&Who::Agent("lead".into()), &aid, fx, "overtaken".into());
        assert!(!r.ok && r.out.contains("withdrawn by fixer or an agent above it"), "{}", r.out);
        let r = core.ticket_withdraw_asks(&Who::Agent("fixer".into()), &aid, fx, "overtaken".into());
        assert!(!r.ok && r.out.contains("asks nothing open"), "the author passes coordd's check: {}", r.out);
        let r = core.ticket_withdraw_asks(&Who::Owner, &aid, fx, "overtaken".into());
        assert!(!r.ok && r.out.contains("nothing to answer"), "{}", r.out);
        assert!(!core.ticket_withdraw_asks(&Who::Agent("fixer".into()), &aid, fx, " ".into()).ok, "a why");
        assert!(!core.ticket_withdraw_asks(&Who::Agent("fixer".into()), &aid, 99, "x".into()).out.contains("fixer's"), "no post 99");
        // closed: the owner's words for it open a thread that follows it, and nothing is posted on it
        assert!(core.ticket_cancel(&Who::Owner, &aid, "done with it".into(), Said::default()).ok);
        let posts = core.tickets.m.tickets[&a].posts.len();
        let r = core.ticket_post(&Who::Owner, &aid, "one more thing".into(), None, None, Said::default(), Some("lead".into()), false);
        pump(&mut core, &mut rx);
        assert!(r.ok && r.out.contains(&format!("{aid} is closed")), "{}", r.out);
        let d = last(&core);
        assert_ne!(d, a);
        let f = &core.tickets.m.tickets[&d].form;
        assert_eq!((f["follows"].clone(), f["to"].clone(), f["typed"].clone(), f["text"].clone()), (json!(aid), json!("lead"), json!([]), json!("one more thing")));
        assert_eq!(core.tickets.m.tickets[&a].posts.len(), posts);
        let r = core.ticket_post(&Who::Owner, &aid, "and to the fixer".into(), Some("fixer".into()), None, Said::default(), None, false);
        pump(&mut core, &mut rx);
        assert!(r.ok, "{}", r.out);
        assert_eq!(core.tickets.m.tickets[&last(&core)].form["typed"], json!(["fixer"]));
        assert!(!core.ticket_post(&Who::Owner, &aid, "to nobody, and nobody fills lead".into(), None, None, Said::default(), None, false).ok);
        assert!(!core.ticket_post(&Who::Agent("lead".into()), &aid, "typed to".into(), Some("fixer".into()), None, Said::default(), None, false).ok, "to is the owner's");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A quote reply (paper-research N-101 p474; the owner, N-234.p5): the owner's words quoting a post go on its
    /// ticket under it, open or closed, to its author, with no intake and no thread; a closed ticket takes its author's
    /// reply too, new for the owner. Words that quote nothing go to intake as before.
    #[test]
    fn a_quote_reply_lands_under_its_post_with_no_intake() {
        use crate::core::{Core, Who};
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-quote-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let reg = |id: &str, name: &str| format!("{{\"ev\":\"add\",\"id\":\"{id}\",\"name\":\"{name}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}}\n");
        std::fs::write(coord.join("registry.jsonl"), reg("AGT-001", "lead") + &reg("AGT-002", "fixer")).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.tickets.tasks = Some(tx);
        let pump = |core: &mut Core, rx: &mut tokio::sync::mpsc::UnboundedReceiver<Task>| {
            while let Ok(t) = rx.try_recv() {
                if let Task::Event { name, value } = t {
                    core.ticket_input(Input::Event { name, value }).unwrap();
                }
            }
        };
        let tickets = |core: &Core| core.tickets.m.tickets.range(..docket_machine::HIDDEN).count();
        let intakes = |core: &Core| core.ticket_due(docket_machine::WATCHER).iter().filter(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "intake")).count();
        let given_fixer = |core: &Core, t: u64| core.ticket_due("fixer").iter().any(|d| d.t == t);
        // a thread to the fixer, and the fixer's answer on it: post 2
        assert!(core.ticket_say(&Who::Owner, "fixer".into(), vec!["fixer".into()], vec![], "the parser drops comments".into(), None).ok);
        pump(&mut core, &mut rx);
        let a = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        let aid = core.tickets.m.id(a);
        core.ticket_answer(docket_machine::WATCHER, a, json!({"head": "Thread"}), Text::New(String::new())).unwrap();
        core.ticket_input(Input::Post { t: a, by: "fixer".into(), text: Text::New("Found it: the lexer skips them.".into()), names: vec!["owner".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        for d in core.ticket_due("fixer") {
            core.ticket_input(Input::Given { t: d.t, to: "fixer".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).unwrap();
        }
        let n = core.tickets.m.tickets[&a].posts.last().unwrap().n;
        let (count, waiting) = (tickets(&core), intakes(&core));
        // quoted, typed to nobody: to the post's author, under the post, no thread and no intake
        let to = core.ticket_quoted_author(&aid, Some(n));
        assert_eq!(to.as_deref(), Some("fixer"));
        let r = core.ticket_post(&Who::Owner, &aid, "> Found it (from your message at 10:24)\nGood, fix it there.".into(), to, Some(n), Said::default(), Some("lead".into()), true);
        pump(&mut core, &mut rx);
        assert!(r.ok && r.out == format!("Posted on {aid} under post {n}."), "{}", r.out);
        assert_eq!(core.tickets.m.tickets[&a].posts.last().map(|p| (p.by.as_str(), p.answers)), Some(("owner", Some(n))));
        assert_eq!((tickets(&core), intakes(&core)), (count, waiting), "no thread, no intake");
        assert!(given_fixer(&core, a), "the author is given it");
        // closed: still on it, under the post, to its author; the author's reply lands there and is new for the owner
        assert!(core.ticket_cancel(&Who::Owner, &aid, "done with it".into(), Said::default()).ok);
        for d in core.ticket_due("fixer") {
            core.ticket_input(Input::Given { t: d.t, to: "fixer".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).unwrap();
        }
        let r = core.ticket_post(&Who::Owner, &aid, "> the lexer skips them\nAnd the docs?".into(), core.ticket_quoted_author(&aid, Some(n)), Some(n), Said::default(), Some("lead".into()), true);
        pump(&mut core, &mut rx);
        assert!(r.ok, "{}", r.out);
        assert!(!core.tickets.m.tickets[&a].is_open());
        assert_eq!(core.tickets.m.tickets[&a].posts.last().map(|p| (p.by.as_str(), p.answers)), Some(("owner", Some(n))));
        assert_eq!((tickets(&core), intakes(&core)), (count, waiting), "no thread follows it, no intake");
        assert!(given_fixer(&core, a), "the author is given it on the closed ticket");
        core.ticket_input(Input::Post { t: a, by: "fixer".into(), text: Text::New("The docs too.".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        assert!(core.tickets.m.tickets[&a].unread("owner").iter().any(|p| p.by == "fixer" && matches!(&p.text, Text::New(t) if t == "The docs too.")), "new for the owner");
        // no such post: refused; a post of no agent's: typed to nobody, to lead as ever
        assert!(!core.ticket_post(&Who::Owner, &aid, "x".into(), None, Some(99), Said::default(), Some("lead".into()), true).ok);
        assert_eq!(core.ticket_quoted_author(&aid, Some(1)), None, "post 1 is the owner's");
        assert!(!core.ticket_post(&Who::Agent("lead".into()), &aid, "x".into(), None, None, Said::default(), None, true).ok, "the owner's only");
        // words quoting nothing: intake, as before
        assert!(core.ticket_say(&Who::Owner, "lead".into(), vec![], vec![], "and the lexer?".into(), None).ok);
        pump(&mut core, &mut rx);
        assert_eq!((tickets(&core), intakes(&core)), (count + 1, waiting + 1));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// An ask about a step since withdrawn is answered with the withdrawal in its own words (withdrawn.tmpl): what the
    /// ask tool returns (paper-research m1469, 2).
    #[test]
    fn an_ask_answered_with_a_withdrawal_reads_as_the_withdrawal() {
        use crate::core::{Core, Who};
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-withdrawn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let reg = "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"lead\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n";
        std::fs::write(coord.join("registry.jsonl"), reg).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.tickets.tasks = Some(tx);
        assert!(core.ticket_say(&Who::Owner, "lead".into(), vec!["lead".into()], vec![], "add range(xs)".into(), None).ok);
        while let Ok(Task::Event { name, value }) = rx.try_recv() {
            core.ticket_input(Input::Event { name, value }).unwrap();
        }
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        let id = core.tickets.m.id(t);
        let what = docket_machine::What::Withdrawn { step: 3, call: "go".into(), by: Some("owner".into()), why: Some("undone".into()) };
        let d = Delivery { id: format!("{id}.s3.withdrawn"), to: "lead".into(), t, ticket: id.clone(), kind: "thread".into(), upto: 1, what, inform: false };
        core.ticket_effects(vec![Effect::AnsweredWith { agent: "lead".into(), d }]);
        let said = core.tickets.answered.get("lead").cloned().unwrap_or_default();
        assert!(said.starts_with(&format!("Step 3 of {id} (go) is withdrawn by owner: undone. It is no longer needed")), "{said}");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// The vault's item notes count (orch m14266, m14290; paper-research m1518): an item opened takes a number above
    /// every note; `coord item start` on an id with only a note takes it in (closed when its note says so); on a closed
    /// item whose note still says open it reopens it, held by the lead again. A note that says closed keeps it closed.
    #[test]
    fn the_vaults_item_notes_count() {
        use crate::core::{Core, Who};
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-notes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        let project = coord.join("files");
        let notes = project.join("notes");
        std::fs::create_dir_all(&notes).unwrap();
        let note = |id: &str, title: &str, status: &str| std::fs::write(notes.join(format!("{id}-x.md")), format!("---\ntitle: {title}\nstatus: {status}\n---\nbody\n")).unwrap();
        note("I-300", "written by hand", "open");
        note("I-301", "done by hand", "landed");
        std::fs::write(project.join("project.toml"), format!("[project]\nname = \"g\"\nnotes = \"{}\"\n", notes.display())).unwrap();
        std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
        let reg = "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"orch\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n";
        std::fs::write(coord.join("registry.jsonl"), reg).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        core.load_tickets();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        core.tickets.tasks = Some(tx);
        let orch = Who::Agent("orch".into());
        // a note only: taken in at its number, started
        let r = core.item_start(&orch, "I-300");
        assert!(r.ok, "{}", r.out);
        let x = &core.tickets.m.tickets[&300];
        assert_eq!((x.kind.as_str(), x.is_open(), x.form["title"].clone(), x.started.clone()), ("item", true, json!("written by hand"), vec!["orch".to_string()]));
        // a note that says landed: taken in closed, and the start refused
        let r = core.item_start(&orch, "I-301");
        assert!(!r.ok && r.out.contains("I-301 is closed"), "{}", r.out);
        assert!(!core.tickets.m.tickets[&301].is_open());
        // no note, no ticket
        assert!(!core.item_start(&orch, "I-400").ok);
        // an item opened goes above the notes
        note("I-310", "another by hand", "open");
        let r = core.ticket_open(&orch, "item".into(), None, json!({"title": "new"}), None, Said::default());
        assert_eq!(r.out, "Opened I-311.", "{}", r.out);
        // closed while its note still says open: start reopens it; the vault wins
        let fills = |core: &mut Core| {
            let step = core.tickets.m.tickets[&300].steps;
            core.ticket_input(Input::Decided { t: 300, step, question: "fills".into(), answer: json!("orch"), provenance: json!({}) }).unwrap();
        };
        fills(&mut core);
        let r = core.ticket_input(Input::Answer { t: 300, by: "orch".into(), head: json!({"head": "Done"}), text: Text::New("done".into()) });
        assert!(r.is_ok(), "{r:?}");
        let r = core.item_start(&orch, "I-300");
        assert!(r.ok, "{}", r.out);
        assert!(core.tickets.m.tickets[&300].is_open());
        // and the note says landed now: closed stays closed
        fills(&mut core);
        core.ticket_input(Input::Answer { t: 300, by: "orch".into(), head: json!({"head": "Done"}), text: Text::New("done".into()) }).unwrap();
        note("I-300", "written by hand", "landed");
        assert!(!core.item_start(&orch, "I-300").ok);
        let _ = std::fs::remove_dir_all(&coord);
    }
}

/// agent-base/limited (paper-research m1586): the limited form's project files, as a project folder holding them.
#[cfg(test)]
mod limited_tests {
    use super::*;
    use crate::core::{Core, Who};

    fn limited() -> String {
        format!("{}/../../agent-base/limited", env!("CARGO_MANIFEST_DIR"))
    }

    /// items.eflint: only a lead opens an item; no agent opens a claim, fix, check or decision; the rest as before;
    /// the owner is not bound.
    #[test]
    fn in_the_limited_form_only_the_lead_opens_items_and_nobody_opens_work() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let places = crate::policy::Places { base, project: Some(limited()), coord: std::env::temp_dir().display().to_string(), ..Default::default() };
        let f = |a: &str, b: &[&str]| (a.to_string(), b.iter().map(|x| x.to_string()).collect::<Vec<_>>());
        let facts = vec![f("agent", &["orch"]), f("agent", &["dev"]), f("role", &["orch", "lead"]), f("role", &["dev", "member"])];
        let open = |who: &str, kind: &str| may(MayAsk { places: places.clone(), facts: facts.clone(), act: "open", who: who.into(), ticket: None, kind: kind.into(), bound: who != "owner" });
        assert!(open("orch", "item").is_ok());
        assert_eq!(open("dev", "item").map(|_| ()).map_err(|e| e.starts_with("You may not open a item")), Err(true));
        for kind in ["claim", "fix", "check", "decision"] {
            assert!(open("orch", kind).is_err(), "orch opens a {kind}");
            assert!(open("dev", kind).is_err(), "dev opens a {kind}");
            assert!(open("owner", kind).is_ok(), "the owner opens a {kind}");
        }
        for kind in ["thread", "question"] {
            assert!(open("dev", kind).is_ok(), "dev opens a {kind}");
            assert!(open("orch", kind).is_ok(), "orch opens a {kind}");
        }
        assert!(open("owner", "item").is_ok());
    }

    /// question.kind: `coord ask` is a question whose one step is the owner's, with no lead in between (an instance with
    /// no lead too); his answer closes it, and the asker is told once, by the closing notice. The lead asking goes the
    /// same way.
    #[test]
    fn in_the_limited_form_a_question_goes_straight_to_the_owner() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let root = std::env::temp_dir().join(format!("docket-limited-q-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (coord, project) = (root.join("coord"), root.join("project"));
        std::fs::create_dir_all(&coord).unwrap();
        let out = std::process::Command::new("cp").arg("-a").arg(limited()).arg(&project).output().unwrap();
        assert!(out.status.success());
        std::fs::write(project.join("project.toml"), "[project]\nname = \"t\"\n").unwrap();
        std::fs::write(coord.join("project"), format!("{}\n", project.join("project.toml").display())).unwrap();
        // no agent has the role lead
        std::fs::write(
            coord.join("registry.jsonl"),
            "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"orch\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n{\"ev\":\"add\",\"id\":\"AGT-002\",\"name\":\"dev\",\"parent\":\"orch\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n",
        )
        .unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        // as coordd's start does (main.rs)
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        assert!(core.project.is_some());
        core.load_tickets();
        assert!(core.tickets.errors.is_empty(), "{:?}", core.tickets.errors);
        for (asker, answer, result) in [("dev", json!({"head": "Answer", "text": "raise ValueError"}), "Answered"), ("orch", json!({"head": "Dismiss", "why": "not now"}), "Dismissed")] {
            let r = core.ticket_ask(&Who::Agent(asker.into()), "Should mean([]) raise or return None?\nmean([]) divides by zero today.", None, Said::default());
            assert!(r.ok, "{}", r.out);
            assert!(r.out.contains("Its answer comes to you when it closes"), "{}", r.out);
            let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
            let x = &core.tickets.m.tickets[&t];
            assert_eq!(x.kind, "question");
            assert!(matches!(&x.status, docket_machine::Status::Awaits { agent, .. } if agent == "owner"), "{asker}: {:?}", x.status);
            assert!(core.tickets.due(asker).is_empty(), "{asker} is handed nothing before the answer");
            core.ticket_answer("owner", t, answer, Text::New(String::new())).unwrap();
            let x = &core.tickets.m.tickets[&t];
            assert!(matches!(&x.status, docket_machine::Status::Done(v) if v["head"] == result), "{asker}: {:?}", x.status);
            let due = core.tickets.due(asker);
            assert!(due.iter().any(|d| d.t == t && d.id.ends_with(".c")), "{asker}: the closing notice, {due:?}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// roster's standing(): the flavor's root text first, when the repository the project folder is in has a
    /// standing.md at its top (followed by an empty line); then the project's standing.md when the project folder has
    /// one, else agent-base's. With no project file, agent-base's alone.
    #[test]
    fn a_projects_standing_text_wins_over_agent_bases() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let roster = std::fs::read_to_string(format!("{base}/bin/roster")).unwrap();
        // the whole function, from its first line to its closing brace
        let def: String = roster.lines().skip_while(|l| !l.starts_with("standing() ")).scan(false, |done, l| {
            if *done { return None; }
            *done = l == "}";
            Some(format!("{l}\n"))
        }).collect();
        assert!(def.starts_with("standing() ") && def.ends_with("}\n"), "standing() in roster: {def}");
        let root = std::env::temp_dir().join(format!("docket-standing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (coord, repo) = (root.join("coord"), root.join("flavor"));
        let project = repo.join("projects").join("p");
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        let git = std::process::Command::new("git").args(["init", "-q"]).arg(&repo).output().unwrap();
        assert!(git.status.success(), "{}", String::from_utf8_lossy(&git.stderr));
        let standing = || {
            let out = std::process::Command::new("bash").arg("-c").arg(format!("{def}standing")).env("COORD", &coord).env("AGENT_BASE", &base).output().unwrap();
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
            String::from_utf8(out.stdout).unwrap()
        };
        let agent_base = std::fs::read_to_string(format!("{base}/claude/standing.md")).unwrap();
        assert_eq!(standing(), agent_base, "no project file");
        std::fs::write(project.join("project.toml"), "[project]\n").unwrap();
        std::fs::write(coord.join("project"), format!("{}\n", project.join("project.toml").display())).unwrap();
        assert_eq!(standing(), agent_base, "a project with no standing.md, its repository with none at the top");
        std::fs::write(project.join("standing.md"), "Only what works here.\n").unwrap();
        assert_eq!(standing(), "Only what works here.\n", "the project's");
        std::fs::write(repo.join("standing.md"), "The flavor's.\n").unwrap();
        assert_eq!(standing(), "The flavor's.\n\nOnly what works here.\n", "the flavor's root text first, then the project's");
        std::fs::remove_file(project.join("standing.md")).unwrap();
        assert_eq!(standing(), format!("The flavor's.\n\n{agent_base}"), "the flavor's root text first, then agent-base's");
        let _ = std::fs::remove_dir_all(&root);
    }
}
