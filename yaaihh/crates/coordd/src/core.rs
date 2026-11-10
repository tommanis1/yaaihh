//! coordd's state and what changes it (design §5-7): the agents (registry, process, turn), mail and its delivery,
//! items and rights, project views, and the numbered events every change emits.
//!
//! History is append-only JSON lines in the coord folder: `registry.jsonl` (agents: id, name, parent, kind, model,
//! sessions), `mail/log.jsonl` (mail and deliveries), `items/log.jsonl` (opened, closed, started). Live state
//! (processes, states, turns) is journalled to /run/coordd/live.json (COORDD_LIVE), which a box restart empties.
//! The design: vault designs/coordd-core-2026-11-10.md (indexes kept per record, no disk or solve under the lock, one
//! record per agent kind, delivery acknowledged once, norms solved per situation and kept).

use crate::cgroup;
use crate::project::Project;
use crate::transcript::{self, Background};
use coord_types::{subject, AgentView, Body, Event, ItemRecord, MailView, ModelUsage, NetEffect, NetRun, Quote, Recognised, Reply, Sent, Snapshot, TeamGrant, Tool, ViewData};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::time::Duration;
use tokio::sync::broadcast;

const RING: usize = 20_000;

/// Where live state is journalled: COORDD_LIVE, else /run/coordd/live.json (a box's; a host coordd's is under
/// $XDG_RUNTIME_DIR, bin/host-coordd).

fn live_path() -> String {
    std::env::var("COORDD_LIVE").unwrap_or_else(|_| "/run/coordd/live.json".into())
}

pub fn now() -> String {
    // UTC, seconds, as transcripts write it, so the two compare as text
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    let (days, secs) = (t.div_euclid(86_400), t.rem_euclid(86_400));
    let (y, m, d) = civil(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", secs / 3600, secs % 3600 / 60, secs % 60)
}

fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// A mail record as the log keeps it. Records of the old mail layer read as well (their other fields are ignored).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mail {
    pub id: String,
    pub at: String,
    pub from: String,
    pub to: Vec<String>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub esc: bool,
    /// Goes in while the agent's turn runs, after its next tool call, without stopping it (Claude Code takes a channel
    /// message mid-turn as typed input: after the tool result); a Codex agent's waits for its turn's end.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub soon: bool,
    /// `question` for a question to the owner; empty for a message.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    /// The mail this one answers; the old mail layer's `in_reply_to`, so its threads carry over.
    #[serde(default, alias = "in_reply_to", skip_serializing_if = "Option::is_none")]
    pub re: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quotes: Vec<Quote>,
}

/// What `run` and the hooks know of an agent's live session.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Live {
    pub pid: Option<i32>,
    pub state: String,
    pub since: String,
    /// The tool calls running (from its transcript; a Codex agent's from its hooks): not journalled, the record
    /// gives them again.
    #[serde(skip)]
    pub tools: BTreeMap<String, Tool>,
    #[serde(skip)]
    pub last_tool: Option<String>,
    /// When its process began: background work started before it died with the old process.
    pub started: String,
    /// Counts turns: a turn end that finishes after a new turn began leaves that turn alone.
    #[serde(default)]
    pub turn: u64,
    /// Whether its screen shows a turn running, as its terminal holder last told (`Core::busy`); None until one tells
    /// (a holder from before), when the transcript decides. Journalled: a holder tells only a change, so a coordd
    /// restart that forgot it fell back to the transcript until the screen next changed (10 Nov, MEANING).
    #[serde(default)]
    pub screen_busy: Option<bool>,
}

/// What coordd hands an agent's session, off the core's lock (`main::senders`): one sender per agent does them in
/// order, so a session slow to answer holds up only its own mail, never the core (owner m249, 9 Nov: a 10 s push to
/// one agent under the lock made orch's Stop hook miss its 5 s, and the mail in its late answer was lost).
pub enum Job {
    /// Mail IDS through Claude Code's coord channel SOCK (LINE: the channel's events).
    Channel { name: String, ids: Vec<String>, sock: String, line: String },
    /// Mail IDS through a Codex app-server (TEXT: the batch).
    Codex { name: String, ids: Vec<String>, base: String, session: String, socket: String, text: String },
    /// Escape typed into NAME's terminal (Claude Code), or a Codex interrupt.
    Interrupt { name: String, pty: String, codex: Option<(String, String, String)> },
}

/// Mail on its way to an agent's session: its sender has not had the session's answer yet.
pub struct Inflight {
    pub ids: Vec<String>,
    pub tries: u32,
}

pub struct Agent {
    pub id: String,
    pub name: String,
    pub parent: String,
    /// Its role (registry `role` events; empty: a member).
    pub role: String,
    pub kind: String,
    pub model: String,
    pub session: Value,
    pub live: Live,
    pub items: Vec<String>,
    pub background: Option<Background>,
    /// When it was first registered.
    pub created: String,
    /// Its sessions' transcripts, in the order they began.
    pub sessions: Vec<String>,
    /// Its tokens over all of its sessions, and its transcripts' latest time (counted by the usage reader, main.rs).
    pub usage: Vec<ModelUsage>,
    pub last_active: Option<String>,
    /// Why its last turn ended without finishing (Claude Code's StopFailure: an API error), until a new turn begins.
    pub failed: Option<String>,
    /// What it sent through coordd in the turn running now (`coord msg`, `coord ask`), for its turn end: the clerk
    /// reads these as facts instead of picking them out of the transcript.
    pub sent: Vec<Value>,
    /// Its last turn end's text and sent mail, to tell a repeat (an idle loop saying the same again).
    pub last_turn_end: Option<String>,
    /// The last turn whose end coordd took (`Core::end_turn`): taken once, however often a poll sees it ended.
    pub ended_turn: u64,
    /// Its memory (MB) and CPU time (s) as the timer last read them from its cgroup (`main::count_usage`).
    pub mem_mb: Option<u64>,
    pub cpu_s: Option<f64>,
    /// Its session's transcript, as `note_session` found it.
    transcript_file: Option<String>,
    /// Its team grant (`coord team grant`, registry `team`), with the agents added under it.
    pub team: Option<TeamGrant>,
    /// The agent under whose grant it was added (registry `add` with `member_of`).
    pub member_of: Option<String>,
    /// What its last turn's background work waited on (`Core::turn_end_state`), kept so a refresh of the project's
    /// waits (`Core::refresh_waits`) can restate the whole wait.
    pub bg_waits: Vec<String>,
}

impl Agent {
    pub fn running(&self) -> bool {
        self.live.pid.is_some()
    }
    /// Its session's transcript: the file the session writes (`written_transcript`), not only the path its hook named.
    /// Found once, when the session is named (`note_session`), not on every look.
    pub fn transcript(&self) -> Option<String> {
        self.transcript_file.clone()
    }
    fn socket(&self) -> String {
        self.session.get("socket").and_then(Value::as_str).unwrap_or("").to_string()
    }
    /// Count the session's transcript among its sessions, once.
    fn note_session(&mut self) {
        self.transcript_file = self.session.get("transcript_path").and_then(Value::as_str).map(|named| written_transcript(named).unwrap_or_else(|| named.to_string()));
        if let Some(t) = self.transcript().filter(|t| !self.sessions.contains(t)) {
            self.sessions.push(t);
        }
    }
}

/// The mail's indexes (design coordd-core-2026-11-10 §1), by position in `Core::mails`.
#[derive(Default)]
struct MailIndex {
    /// A mail's position by its id (the first, should an id repeat).
    id: HashMap<String, usize>,
    /// Each mail's thread root, by position.
    root: Vec<usize>,
    /// Each thread's mails, oldest first, by its root.
    thread: HashMap<usize, Vec<usize>>,
    /// Each sender's mails, oldest first.
    by_from: HashMap<String, Vec<usize>>,
    /// Each recipient's mail not delivered or taken back.
    held: HashMap<String, std::collections::BTreeSet<usize>>,
    /// How many messages (`mN`) and questions (`qN`) there are: the next numbers.
    messages: usize,
    questions: usize,
}

/// The caller's name as a sender: the owner, or the agent.
pub enum Who {
    Owner,
    Agent(String),
}

impl Who {
    pub fn name(&self) -> String {
        match self {
            Who::Owner => "owner".into(),
            Who::Agent(a) => a.clone(),
        }
    }
}

pub struct Core {
    pub coord: String,
    pub base: String,
    pub agents: BTreeMap<String, Agent>,
    mails: Vec<Mail>,
    /// The mail indexes, kept as each mail is added (design coordd-core-2026-11-10 §1): never a scan of the mail.
    ix: MailIndex,
    delivered: HashSet<(String, String)>,
    /// (mail, recipient) its sender took back before it was delivered (`unsend`).
    withdrawn: HashSet<(String, String)>,
    /// Questions withdrawn (`coord ask --drop`).
    dropped: HashSet<String>,
    items: Vec<ItemRecord>,
    closed: HashSet<String>,
    pub views: BTreeMap<String, ViewData>,
    pub project: Option<Project>,
    seq: u64,
    /// The events a reconnecting watcher may be sent instead of a snapshot: each still news, at most [`RING`]. An
    /// event that carries whole state (an agent's view, a project view, a snapshot) leaves the ring when a
    /// newer one replaces it, so the ring's size follows what changed, not how often a view source prints (8 Nov:
    /// 20,000 kept copies of a 470 KB runner view ran coordd to 23 GB and the box's OOM killer).
    ring: VecDeque<(u64, Option<Event>)>,
    /// Where in the ring the latest event of each thing that whole-state events carry is (`event_key`), by its seq:
    /// the one a newer event of it empties, found without a scan.
    latest: HashMap<String, u64>,
    /// How many events the ring holds (its emptied places not counted).
    held_events: usize,
    /// The last event no longer in the ring that a watcher would still need: one at or after it is caught up from
    /// the ring.
    floor: u64,
    pub tx: broadcast::Sender<Event>,
    /// The agents' model requests as their transcripts record them, by request id (`coord_types::Sent`; requests
    /// already older than [`SENT_DAYS`] when read are not kept).
    sent: HashMap<String, Sent>,
    /// The questions the recogniser found in agents' plain mails to the owner, by mail (vault
    /// designs/decisions-policies-forks-2026-11-08.md §8): such a mail starts a question thread like `coord ask`.
    recognised: HashMap<String, Recognised>,
    /// Where an agent's new mail to the owner goes to be read for a question (`main::recognise`), when it runs.
    pub recognise: Option<tokio::sync::mpsc::UnboundedSender<Mail>>,
    /// Where events go to the nets (`crate::net`), when they run.
    pub nets: Option<tokio::sync::mpsc::UnboundedSender<crate::net::Raised>>,
    /// The latest net runs (at most [`NET_RUNS`]), and the nets' check's latest findings.
    pub net_runs: VecDeque<NetRun>,
    pub net_errors: Vec<String>,
    /// The mails a net filed from an agent's turn (a live `ask` or `post`), with the run that filed each.
    filed: HashMap<String, String>,
    /// The filed mails the owner took back (`Request::Unfile`): no longer a question, hidden from threads.
    unfiled: HashSet<String>,
    /// Mails held for their message net (`deliver` live): delivered only as the net says, or unchanged when it takes
    /// nothing. Not kept over a restart: a mail still held then is delivered unchanged.
    clerking: HashSet<String>,
    /// Mails their message net did not pass on as they were, with what it did instead (the mails it delivered in their
    /// place, or why it sent them back): never delivered themselves unless the owner says so (`Request::Release`).
    clerked: HashMap<String, String>,
    /// Mail being handed to each agent's session, until the session's answer (`handed`).
    pub inflight: HashMap<String, Inflight>,
    /// When an agent's session may be tried again after it did not take mail.
    retry_at: HashMap<String, std::time::Instant>,
    /// How often an agent's held mail was handed over and not taken: its pause grows with them.
    retry_tries: HashMap<String, u32>,
    /// Where jobs for the agents' sessions go (`main::senders`).
    pub jobs: Option<tokio::sync::mpsc::UnboundedSender<Job>>,
    /// Wakes the transcript follower (`main::follow`) at once: a hook, new mail.
    pub poke: std::sync::Arc<tokio::sync::Notify>,
    /// What `journal` last wrote (live.json, running.json): written again only when it changes.
    journalled: (Vec<u8>, Vec<u8>),
    /// The compiled norms and the situations solved with them, until the policy files, an agent or a role change.
    norm_cache: std::cell::RefCell<Option<NormCache>>,
    /// Counts the changes of agents and roles the norms' situations are made of (`NormCache::generation`).
    norm_gen: u64,
    /// The nets' live set as last read (`nets/live`), read again when 2 s old: not a file read per mail.
    live_cache: std::cell::RefCell<Option<(std::time::Instant, crate::net::check::LiveSet)>>,
    /// The proxy's models and their families (`Core::set_models`), the norms' `model` and `family` facts.
    models: Vec<(String, String)>,
    /// When the owner was last mailed about each project event fingerprint (`Core::project_event`).
    notified: HashMap<String, std::time::Instant>,
    /// The owner's hold (`Core::hold`): when it was last looked at, and the first mail it lets go.
    hold: std::cell::RefCell<(Option<std::time::Instant>, Option<usize>)>,
    /// The dialog titles the terminal holders reported (`Core::popup`), read from `popups.jsonl` when first needed.
    popups: Option<HashSet<String>>,
}

/// The norms compiled from the policy files as STAMP had them, with agents and roles as of GENERATION, and the situations
/// solved with them, by the agents brought into the thread asked about (design coordd-core-2026-11-10 §5).
struct NormCache {
    stamp: Vec<(String, Option<std::time::SystemTime>)>,
    generation: u64,
    /// When the policy files were last compared with STAMP (at most every 2 s).
    checked: std::time::Instant,
    norms: Result<crate::policy::norms::Norms, String>,
    solved: HashMap<Vec<String>, Result<HashSet<String>, String>>,
}

/// How many net runs a snapshot carries.
const NET_RUNS: usize = 200;

/// How many days of the agents' requests coordd keeps for the usage page.
const SENT_DAYS: i64 = 32;

fn read_jsonl(path: &str) -> Vec<Value> {
    std::fs::read_to_string(path).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

/// How many earlier mails an owner message carries to the clerk as its context.
const CONTEXT: usize = 8;
/// How much of each context mail goes with it.
const CONTEXT_CHARS: usize = 1500;

/// Append V to the log at PATH, on the writer thread (`jsonl::start_writer`): no disk write under the core's lock.
fn append(path: &str, v: &Value) {
    crate::jsonl::later(path, v.to_string());
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

impl Core {
    pub fn load(coord: &str, base: &str) -> Core {
        let (tx, _) = broadcast::channel(4096);
        let mut core = Core {
            coord: coord.into(),
            base: base.into(),
            agents: BTreeMap::new(),
            mails: Vec::new(),
            ix: MailIndex::default(),
            delivered: HashSet::new(),
            withdrawn: HashSet::new(),
            dropped: HashSet::new(),
            items: Vec::new(),
            closed: HashSet::new(),
            views: BTreeMap::new(),
            project: None,
            seq: 0,
            ring: VecDeque::new(),
            latest: HashMap::new(),
            held_events: 0,
            floor: 0,
            tx,
            sent: HashMap::new(),
            recognised: HashMap::new(),
            recognise: None,
            nets: None,
            net_runs: VecDeque::new(),
            net_errors: Vec::new(),
            filed: HashMap::new(),
            unfiled: HashSet::new(),
            clerking: HashSet::new(),
            clerked: HashMap::new(),
            inflight: HashMap::new(),
            retry_at: HashMap::new(),
            retry_tries: HashMap::new(),
            jobs: None,
            poke: Default::default(),
            journalled: Default::default(),
            norm_cache: Default::default(),
            norm_gen: 0,
            live_cache: Default::default(),
            hold: Default::default(),
            models: Vec::new(),
            notified: HashMap::new(),
            popups: None,
        };
        core.load_registry();
        for r in read_jsonl(&core.path("mail/log.jsonl")) {
            match r.get("ev").and_then(Value::as_str) {
                Some("mail") => {
                    if let Ok(m) = serde_json::from_value::<Mail>(r) {
                        core.index_mail(m);
                    }
                }
                Some("delivered") => {
                    core.delivered.insert((s(&r, "id"), s(&r, "to")));
                }
                Some("withdrawn") => {
                    core.withdrawn.insert((s(&r, "id"), s(&r, "to")));
                }
                Some("soon") => {
                    if let Some(&i) = core.ix.id.get(&s(&r, "id")) {
                        core.mails[i].soon = true;
                    }
                }
                Some("drop") => {
                    core.dropped.insert(s(&r, "id"));
                }
                Some("recognised") => {
                    if let Ok(q) = serde_json::from_value::<Recognised>(r["recognised"].clone()) {
                        core.recognised.insert(s(&r, "id"), q);
                    }
                }
                Some("filed") => {
                    core.filed.insert(s(&r, "id"), s(&r, "run"));
                }
                Some("unfile") => {
                    core.unfiled.insert(s(&r, "id"));
                }
                Some("clerked") => {
                    core.clerked.insert(s(&r, "id"), s(&r, "what"));
                }
                Some("released") => {
                    core.clerked.remove(&s(&r, "id"));
                }
                _ => {}
            }
        }
        // each recipient's held mail: what no delivery or withdrawal took
        for (i, m) in core.mails.iter().enumerate() {
            for t in &m.to {
                let key = (m.id.clone(), t.clone());
                if !core.delivered.contains(&key) && !core.withdrawn.contains(&key) {
                    core.ix.held.entry(t.clone()).or_default().insert(i);
                }
            }
        }
        for r in read_jsonl(&core.path("items/log.jsonl")) {
            if let Ok(rec) = serde_json::from_value::<ItemRecord>(r) {
                core.apply_item(&rec);
                core.items.push(rec);
            }
        }
        core
    }

    pub fn path(&self, rel: &str) -> String {
        format!("{}/{rel}", self.coord)
    }

    /// The registry, or (first start) the agents the old layer knew: agents/NAME/{parent,session.json}.
    fn load_registry(&mut self) {
        let path = self.path("registry.jsonl");
        if !std::path::Path::new(&path).exists() {
            let mut names: Vec<String> = std::fs::read_dir(self.path("agents"))
                .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect())
                .unwrap_or_default();
            names.sort();
            for name in names {
                let dir = self.path(&format!("agents/{name}"));
                let Ok(session) = std::fs::read_to_string(format!("{dir}/session.json")) else { continue };
                let session: Value = serde_json::from_str(&session).unwrap_or(Value::Null);
                let parent = std::fs::read_to_string(format!("{dir}/parent")).map(|p| p.trim().to_string()).unwrap_or_default();
                let id = format!("AGT-{:03}", self.agents.len() + 1);
                append(&path, &json!({"ev": "add", "id": id, "name": name, "parent": if parent.is_empty() { "owner" } else { &parent },
                    "kind": s(&session, "kind"), "model": s(&session, "model"), "at": now(), "why": "carried over from the old layer"}));
                append(&path, &json!({"ev": "session", "name": name, "session": session, "at": now()}));
                let mut a = self.new_agent(id, name.clone(), parent, s(&session, "kind"), s(&session, "model"), now());
                a.sessions.extend(session.get("transcript_path").and_then(Value::as_str).map(String::from));
                self.agents.insert(name, a);
            }
            return;
        }
        for r in read_jsonl(&path) {
            let name = s(&r, "name");
            match r.get("ev").and_then(Value::as_str) {
                Some("add") => {
                    let mut a = self.new_agent(s(&r, "id"), name.clone(), s(&r, "parent"), s(&r, "kind"), s(&r, "model"), s(&r, "at"));
                    if let Some(o) = self.agents.remove(&name) {
                        a.id = o.id;
                        a.parent = o.parent;
                        a.session = o.session;
                        a.transcript_file = o.transcript_file;
                        a.created = o.created;
                        a.sessions = o.sessions;
                        a.team = o.team;
                        a.member_of = o.member_of;
                    } else if let Some(lead) = r.get("member_of").and_then(Value::as_str) {
                        // a new member counts against its lead's grant at the time
                        if let Some(g) = self.agents.get_mut(lead).and_then(|l| l.team.as_mut()) {
                            g.used += 1;
                        }
                        a.member_of = Some(lead.into());
                    }
                    self.agents.insert(name, a);
                }
                Some("team") => {
                    if let Some(a) = self.agents.get_mut(&name) {
                        a.team = Some(TeamGrant { granted: r["granted"].as_u64().unwrap_or(0) as u32, used: 0, by: s(&r, "by") });
                    }
                }
                Some("rename") => self.renamed(&name, &s(&r, "to")),
                Some("role") => {
                    if let Some(a) = self.agents.get_mut(&name) {
                        a.role = s(&r, "role");
                    }
                }
                Some("model") => {
                    if let Some(a) = self.agents.get_mut(&name) {
                        a.model = s(&r, "model");
                    }
                }
                Some("session") => {
                    if let Some(a) = self.agents.get_mut(&name) {
                        a.session = r.get("session").cloned().unwrap_or(Value::Null);
                        a.note_session();
                    }
                }
                _ => {}
            }
        }
    }

    fn new_agent(&self, id: String, name: String, parent: String, kind: String, model: String, created: String) -> Agent {
        Agent {
            id,
            name,
            parent: if parent.is_empty() { "owner".into() } else { parent },
            role: String::new(),
            kind: if kind.is_empty() { "claude".into() } else { kind },
            model,
            session: Value::Null,
            live: Live { state: "stopped".into(), since: now(), ..Default::default() },
            items: Vec::new(),
            background: None,
            created,
            sessions: Vec::new(),
            usage: Vec::new(),
            last_active: None,
            failed: None,
            sent: Vec::new(),
            last_turn_end: None,
            ended_turn: u64::MAX, // no turn end taken yet (a turn number never reaches it)
            mem_mb: None,
            cpu_s: None,
            transcript_file: None,
            team: None,
            member_of: None,
            bg_waits: Vec::new(),
        }
    }

    // ---- events ------------------------------------------------------------------------------------------------

    pub fn emit(&mut self, body: Body) {
        self.seq += 1;
        let ev = Event { seq: self.seq, body };
        if matches!(ev.body, Body::Snapshot(_)) {
            // a snapshot carries everything: the ring before it is no longer news
            self.ring.clear();
            self.latest.clear();
            self.held_events = 0;
        } else if let Some(key) = event_key(&ev.body) {
            if let Some(old) = self.latest.insert(key, ev.seq) {
                let at = self.ring.partition_point(|(s, _)| *s < old);
                if let Some((s, slot)) = self.ring.get_mut(at) {
                    if *s == old && slot.take().is_some() {
                        self.held_events -= 1;
                    }
                }
            }
        }
        self.ring.push_back((ev.seq, Some(ev.clone())));
        self.held_events += 1;
        while self.held_events > RING {
            if let Some((s, Some(old))) = self.ring.pop_front() {
                self.held_events -= 1;
                self.floor = s;
                if let Some(k) = event_key(&old.body).filter(|k| self.latest.get(k) == Some(&s)) {
                    self.latest.remove(&k);
                }
            }
        }
        while matches!(self.ring.front(), Some((_, None))) {
            self.ring.pop_front();
        }
        let _ = self.tx.send(ev);
    }

    /// Events after FROM, when the ring still holds all of them.
    pub fn since(&self, from: u64) -> Option<Vec<Event>> {
        (from >= self.floor && from <= self.seq).then(|| {
            let at = self.ring.partition_point(|(s, _)| *s <= from);
            self.ring.range(at..).filter_map(|(_, e)| e.clone()).collect()
        })
    }

    pub fn snapshot(&self) -> Event {
        let snap = Snapshot {
            project: self.project.as_ref().map(|p| p.raw.clone()),
            agents: self.agents.keys().map(|n| self.view(n)).collect(),
            // the latest mail, and every question's thread however old (the owner's inbox lists the open ones)
            mail: self
                .mails
                .iter()
                .enumerate()
                .filter(|(i, m)| *i + 500 >= self.mails.len() || self.thread_root(&m.id).is_some())
                .map(|(_, m)| self.mail_view(m))
                .collect(),
            items: self.items.clone(),
            views: self.views.values().cloned().collect(),
            nets: self.net_runs.iter().cloned().collect(),
            net_errors: self.net_errors.clone(),
        };
        Event { seq: self.seq, body: Body::Snapshot(snap) }
    }

    pub fn view(&self, name: &str) -> AgentView {
        let a = &self.agents[name];
        let (mem_mb, cpu_s) = if a.running() { (a.mem_mb, a.cpu_s) } else { (None, None) };
        AgentView {
            id: a.id.clone(),
            name: a.name.clone(),
            parent: a.parent.clone(),
            role: a.role.clone(),
            kind: a.kind.clone(),
            model: a.model.clone(),
            state: a.live.state.clone(),
            since: a.live.since.clone(),
            last_tool: a.live.last_tool.clone(),
            tools: a.live.tools.values().cloned().collect(),
            items: a.items.iter().filter(|i| !self.closed.contains(*i)).cloned().collect(),
            transcript: a.transcript(),
            cwd: a.session.get("cwd").and_then(Value::as_str).map(String::from),
            mem_mb,
            cpu_s,
            created: a.created.clone(),
            sessions: a.sessions.len(),
            last_active: a.last_active.clone(),
            usage: a.usage.clone(),
            team: a.team.clone(),
            member_of: a.member_of.clone(),
        }
    }

    fn mail_view(&self, m: &Mail) -> MailView {
        MailView {
            id: m.id.clone(),
            at: m.at.clone(),
            from: m.from.clone(),
            to: m.to.clone(),
            subject: subject(&m.body),
            body: m.body.clone(),
            esc: m.esc,
            soon: m.soon,
            delivered: m.to.iter().filter(|t| self.delivered.contains(&(m.id.clone(), (*t).clone()))).cloned().collect(),
            withdrawn: m.to.iter().filter(|t| self.withdrawn.contains(&(m.id.clone(), (*t).clone()))).cloned().collect(),
            kind: m.kind.clone(),
            re: m.re.clone(),
            quotes: m.quotes.clone(),
            state: (self.is_question(m) && m.re.is_none()).then(|| self.question_state(&m.id).into()),
            recognised: self.recognised.get(&m.id).cloned(),
            filed: self.filed.get(&m.id).cloned(),
            unfiled: self.unfiled.contains(&m.id),
            clerk: if self.clerking.contains(&m.id) { Some("with the clerk".into()) } else { self.clerked.get(&m.id).cloned() },
        }
    }

    /// Add mail M to the mails and their indexes: its id, its thread (the root of the mail it answers; a mail that
    /// answers none, or one not known, is its own root), its sender's mails, the numbering. Not its recipients' held
    /// mail: [`Core::new_mail`] adds that, the log's load after its deliveries are read.
    fn index_mail(&mut self, m: Mail) -> usize {
        let i = self.mails.len();
        let root = m.re.as_deref().and_then(|r| self.ix.id.get(r)).map_or(i, |&p| self.ix.root[p]);
        self.ix.id.entry(m.id.clone()).or_insert(i);
        self.ix.root.push(root);
        self.ix.thread.entry(root).or_default().push(i);
        self.ix.by_from.entry(m.from.clone()).or_default().push(i);
        if m.kind == "question" { self.ix.questions += 1 } else { self.ix.messages += 1 }
        self.mails.push(m);
        i
    }

    fn find_mail(&self, id: &str) -> Option<&Mail> {
        self.ix.id.get(id).map(|&i| &self.mails[i])
    }

    /// The first mail of the thread ID is in (a question or a plain mail).
    fn root_of(&self, id: &str) -> Option<String> {
        self.ix.id.get(id).map(|&i| self.mails[self.ix.root[i]].id.clone())
    }

    /// The mails of the thread whose first mail is ROOT, oldest first (none when ROOT is not a thread's first mail).
    fn thread(&self, root: &str) -> Vec<&Mail> {
        match self.ix.id.get(root) {
            Some(&i) if self.ix.root[i] == i => self.ix.thread.get(&i).map_or(Vec::new(), |t| t.iter().map(|&j| &self.mails[j]).collect()),
            _ => Vec::new(),
        }
    }

    /// The mails NAME sent, oldest first.
    fn mails_from<'a>(&'a self, name: &str) -> impl Iterator<Item = &'a Mail> + 'a {
        self.ix.by_from.get(name).into_iter().flatten().map(|&i| &self.mails[i])
    }

    /// Mail ID no longer waits for NAME (delivered or taken back).
    fn unhold(&mut self, id: &str, name: &str) {
        if let (Some(&i), Some(h)) = (self.ix.id.get(id), self.ix.held.get_mut(name)) {
            h.remove(&i);
        }
    }

    /// Who is in the thread ROOT (owner 8 Nov): whoever wrote in it or was sent a mail in it, in order of joining.
    fn participants(&self, root: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for m in self.thread(root) {
            for n in std::iter::once(&m.from).chain(&m.to) {
                if !out.contains(n) {
                    out.push(n.clone());
                }
            }
        }
        out
    }

    /// The first question of the thread ID is in, when it is in one.
    fn thread_root(&self, id: &str) -> Option<String> {
        let m = &self.mails[self.ix.root[*self.ix.id.get(id)?]];
        self.is_question(m).then(|| m.id.clone())
    }

    /// M asks the owner: a question (`coord ask`), or a mail the recogniser found a question in.
    /// A filed question the owner took back is none.
    fn is_question(&self, m: &Mail) -> bool {
        (m.kind == "question" || self.recognised.contains_key(&m.id)) && !self.unfiled.contains(&m.id)
    }

    /// Record that mail ID asks Q (the recogniser's finding) and show the mail again, now as a question.
    pub fn record_recognised(&mut self, id: &str, q: Recognised) {
        append(&self.path("mail/log.jsonl"), &json!({"ev": "recognised", "id": id, "recognised": q, "at": now()}));
        self.recognised.insert(id.into(), q);
        if let Some(m) = self.find_mail(id) {
            let v = self.mail_view(m);
            self.emit(Body::Mail(v));
        }
    }

    /// A thread's state: dropped, else open while one of its questions has no word of the owner's after it (an
    /// agent's plain reply in the thread does not reopen it), else answered.
    fn question_state(&self, root: &str) -> &'static str {
        if self.dropped.contains(root) {
            return "dropped";
        }
        let mut waiting = false;
        for m in self.thread(root) {
            if self.is_question(m) {
                waiting = true;
            } else if m.from == "owner" {
                waiting = false;
            }
        }
        if waiting { "open" } else { "answered" }
    }

    /// What an agent is given for M: a mail in a thread names what it answers and that mail's subject, so the agent
    /// has the context after its own context was compacted, and knows the id to reply in the thread with.
    ///
    /// An agent brought into a thread it was never in (no earlier mail of the thread from or to it) gets the thread so
    /// far first, each mail clipped, so it has the conversation it joins (owner 8 Nov).
    fn delivery_text(&self, m: &Mail, to: &str) -> String {
        let mut text = self.thread_text(m);
        if m.kind == "question" && to != "owner" && to != m.from {
            let root = m.re.as_deref().and_then(|r| self.root_of(r)).unwrap_or_else(|| m.id.clone());
            text = format!(
                "({} asked the owner this as {}; you get it as its lead. If you can answer or settle it, reply in its thread with coord msg --thread {} TEXT: {} gets your answer and the owner sees it there, so the owner need not. If it needs the owner, leave it.)\n{text}",
                m.from, m.id, root, m.from
            );
        }
        // mail the recipient may not answer (the norms' send-mail): said, so it does not try (a lookup, §5)
        if self.agents.contains_key(to) && self.agents.contains_key(&m.from) {
            let root = m.re.as_deref().and_then(|r| self.root_of(r)).unwrap_or_else(|| m.id.clone());
            if let Err(why) = self.permitted("send-mail", &[to, &m.from, &root], &root) {
                let role = self.agents.get(&m.from).map_or("", |a| a.role.as_str());
                let lead = self.agents.get(to).map_or("owner".to_string(), |a| a.parent.clone());
                text = format!("(from {}{}: the team's norms do not let you mail it back ({why}). Act on it, or if you disagree take it to your lead ({lead}) or the owner.)\n{text}",
                    m.from, if role.is_empty() { String::new() } else { format!(", role {role}") });
            }
        }
        let Some(root) = m.re.as_deref().and_then(|r| self.root_of(r)) else { return text };
        let before: Vec<&Mail> = self.thread(&root).into_iter().take_while(|x| x.id != m.id).collect();
        if before.iter().any(|x| x.from == to || x.to.iter().any(|t| t == to)) {
            return text;
        }
        let so_far: Vec<String> = before.iter().map(|x| format!("[{} · from {} to {}]\n{}", x.id, x.from, x.to.join(", "), crate::transcript::clip(&x.body, 2000))).collect();
        format!(
            "(you are brought into the thread of {root}: {}; it so far, oldest first; reply to everyone in it with coord msg --thread {})\n{}\n(the mail for you:)\n{text}",
            subject(&self.find_mail(&root).map_or(String::new(), |r| r.body.clone())),
            m.id,
            so_far.join("\n\n")
        )
    }

    fn thread_text(&self, m: &Mail) -> String {
        match m.re.as_deref().and_then(|r| Some((r, self.find_mail(r)?))) {
            Some((r, q)) => {
                let what = if m.kind == "question" { "follows up" } else if q.kind == "question" { "answers" } else { "re" };
                format!("({what} {r}: {}; reply to everyone in this thread with coord msg --thread {})\n{}", subject(&q.body), m.id, m.body)
            }
            None => m.body.clone(),
        }
    }

    fn batch_text(&self, to: &str, ms: &[Mail]) -> String {
        ms.iter().map(|m| format!("[{} · from {}]\n{}", m.id, m.from, self.delivery_text(m, to))).collect::<Vec<_>>().join("\n\n")
    }

    pub fn changed(&mut self, name: &str) {
        let v = self.view(name);
        self.emit(Body::Agent(v));
        self.journal();
    }

    fn set_state(&mut self, name: &str, state: &str) {
        let a = self.agents.get_mut(name).unwrap();
        if a.live.state != state {
            a.live.state = state.into();
            a.live.since = now();
        }
    }

    /// A new turn of NAME began (its transcript shows one running; Codex: a prompt or mail pushed to it).
    pub fn begin_turn(&mut self, name: &str) {
        let a = self.agents.get_mut(name).unwrap();
        a.live.turn += 1;
        a.failed = None;
        a.sent.clear();
        self.set_state(name, "working");
    }

    /// Journal the running agents' live state (process, state, turn, start; not their tool calls) to /run, and who
    /// runs to the project's disk, each only when it changed (design §6), on the writer thread.
    fn journal(&mut self) {
        let live: BTreeMap<&String, &Live> = self.agents.iter().filter(|(_, a)| a.running()).map(|(n, a)| (n, &a.live)).collect();
        let bytes = serde_json::to_vec(&live).unwrap_or_default();
        // who runs, on the project's disk: the journal above is in /run, which a reboot empties
        let names = serde_json::to_vec(&live.keys().collect::<Vec<_>>()).unwrap_or_default();
        if bytes != self.journalled.0 {
            crate::jsonl::replace_later(&live_path(), bytes.clone());
            self.journalled.0 = bytes;
        }
        if names != self.journalled.1 {
            crate::jsonl::replace_later(&self.running_path(), names.clone());
            self.journalled.1 = names;
        }
    }

    /// `$COORD/running.json`: the agents running, as of coordd's last change (`journal`).
    fn running_path(&self) -> String {
        format!("{}/running.json", self.coord)
    }

    /// At start, after `recover`: the agents that ran when coordd last went down and run no more (a reboot, owner
    /// m172, 9 Nov) go to `$COORD/resume.json`, for `roster resume` (box.sh start, host-coordd start) to resume each
    /// from its session as its own user; coordd itself runs as root in a box. Not called again: an agent that ends
    /// later while coordd runs (its /exit, a roster down) was stopped and stays stopped.
    pub fn hand_over_running(&mut self, recovered: &[(String, i32)]) {
        let was: Vec<String> = std::fs::read(self.running_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let gone: Vec<&String> = was.iter().filter(|n| self.agents.contains_key(*n) && !recovered.iter().any(|(r, _)| r == *n)).collect();
        if !gone.is_empty() {
            let _ = std::fs::write(format!("{}/resume.json", self.coord), serde_json::to_vec(&gone).unwrap_or_default());
        }
        self.journal();
    }

    /// After a coordd restart: the live state of agents whose cgroups still hold processes.
    pub fn recover(&mut self) -> Vec<(String, i32)> {
        let saved: HashMap<String, Live> =
            std::fs::read(live_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let mut watch = Vec::new();
        for (name, procs) in cgroup::live_agents() {
            let Some(a) = self.agents.get_mut(&name) else { continue };
            let live = saved.get(&name).cloned().unwrap_or_default();
            let pid = live.pid.filter(|p| procs.contains(p)).unwrap_or(procs[0]);
            a.live = Live { pid: Some(pid), state: if live.state.is_empty() { "idle".into() } else { live.state }, ..live };
            // the state goes on from the journal; the transcript's reader (`main::follow`) corrects it from the record
            if let Some(t) = a.transcript() {
                a.background = Some(Background::new(&t, &a.live.started));
            }
            watch.push((name, pid));
        }
        watch
    }

    // ---- tokens ------------------------------------------------------------------------------------------------

    /// Each agent's session transcripts, for the usage reader.
    pub fn sessions(&self) -> Vec<(String, Vec<String>)> {
        self.agents.values().map(|a| (a.name.clone(), a.sessions.clone())).collect()
    }

    /// NAME's tokens and latest activity as the usage reader counted them; an agent event when they changed.
    pub fn set_usage(&mut self, name: &str, usage: Vec<ModelUsage>, last: Option<String>) {
        let Some(a) = self.agents.get_mut(name) else { return };
        if (&a.usage, &a.last_active) != (&usage, &last) {
            (a.usage, a.last_active) = (usage, last);
            self.changed(name);
        }
    }

    /// The running agents, for the timer that reads their cgroups.
    pub fn running_names(&self) -> Vec<String> {
        self.agents.values().filter(|a| a.running()).map(|a| a.name.clone()).collect()
    }

    /// NAME's memory and CPU as the timer read them (off the lock); its view is told again when they moved (a whole MB,
    /// a whole second).
    pub fn set_cgroup(&mut self, name: &str, mem_mb: Option<u64>, cpu_s: Option<f64>) {
        let Some(a) = self.agents.get_mut(name) else { return };
        let moved = a.mem_mb != mem_mb || a.cpu_s.map(f64::floor) != cpu_s.map(f64::floor);
        (a.mem_mb, a.cpu_s) = (mem_mb, cpu_s);
        if moved {
            self.changed(name);
        }
    }

    /// Keep the requests the usage reader read of NAME's sessions (a request read again replaces what it was).
    pub fn add_sent(&mut self, name: &str, ended: Vec<transcript::Ended>) {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
        let cutoff = now - SENT_DAYS * 86_400_000;
        for e in ended.into_iter().filter(|e| e.at >= cutoff) {
            self.sent.insert(e.id, Sent { agent: name.into(), at: e.at, model: e.model, tokens: e.tokens, cache: e.cache });
        }
    }

    /// The requests of the agents below WHO (all for the owner) that ended at or after SINCE, oldest first.
    pub fn requests(&self, who: &Who, since: i64) -> Reply {
        let mut out: Vec<&Sent> = self.sent.values().filter(|s| s.at >= since && self.above(who, &s.agent)).collect();
        out.sort_by_key(|s| s.at);
        ok(serde_json::to_string(&out).unwrap())
    }

    // ---- references and rights ---------------------------------------------------------------------------------

    /// The owner is level 0, the agents under the owner 1, and so on.
    fn level(&self, who: &Who) -> usize {
        match who {
            Who::Owner => 0,
            Who::Agent(a) => {
                let (mut n, mut cur, mut seen) = (1, a.clone(), HashSet::new());
                while let Some(ag) = self.agents.get(&cur) {
                    if ag.parent == "owner" || !seen.insert(cur.clone()) {
                        break;
                    }
                    n += 1;
                    cur = ag.parent.clone();
                }
                n
            }
        }
    }

    /// WHO is above agent NAME (the owner is above everyone).
    pub fn above(&self, who: &Who, name: &str) -> bool {
        let Who::Agent(me) = who else { return true };
        let (mut cur, mut seen) = (name.to_string(), HashSet::new());
        while let Some(a) = self.agents.get(&cur) {
            if &a.parent == me {
                return true;
            }
            if !seen.insert(cur.clone()) {
                break;
            }
            cur = a.parent.clone();
        }
        false
    }

    fn may(&self, who: &Who, right: &str) -> Result<(), String> {
        let granted = self.project.as_ref().map(|p| p.level_of(right)).unwrap_or(0);
        if self.level(who) <= granted {
            Ok(())
        } else {
            Err(format!("{right} is granted to level {granted} and the levels above it (the owner is 0); {} is at level {}", who.name(), self.level(who)))
        }
    }

    fn is_host(&self, name: &str) -> bool {
        std::path::Path::new(&self.path(&format!("hosts/{name}"))).is_file()
    }

    // ---- agents ------------------------------------------------------------------------------------------------

    pub fn add(&mut self, who: &Who, name: &str, kind: &str, model: &str) -> Reply {
        if name.is_empty() || name.contains('/') || name == "owner" || name == "coord" {
            return err(format!("coord add: not a name: {name:?}"));
        }
        let parent = who.name();
        if let Some(a) = self.agents.get_mut(name) {
            a.kind = kind.into();
            a.model = model.into();
            let (id, parent) = (a.id.clone(), a.parent.clone());
            append(&self.path("registry.jsonl"), &json!({"ev": "add", "id": id, "name": name, "parent": parent, "kind": kind, "model": model, "at": now()}));
            self.changed(name);
            return ok(id);
        }
        // an agent staffing: as the norms allow (`add-agent`); under its team grant, a member counted against it
        let mut member_of = None;
        if let Who::Agent(me) = who {
            if let Err(why) = self.permitted("add-agent", &[me], "none") {
                return err(format!("coord add: {me} may not add agents ({why}); a team is asked for with `coord team ask`"));
            }
            if let Some(g) = self.agents.get_mut(me).and_then(|a| a.team.as_mut()).filter(|g| g.used < g.granted) {
                g.used += 1;
                member_of = Some(me.clone());
            }
        }
        let id = format!("AGT-{:03}", self.agents.len() + 1);
        let mut rec = json!({"ev": "add", "id": id, "name": name, "parent": parent, "kind": kind, "model": model, "at": now()});
        if let Some(l) = &member_of {
            rec["member_of"] = json!(l);
            self.changed(l);
        }
        append(&self.path("registry.jsonl"), &rec);
        let mut a = self.new_agent(id.clone(), name.into(), parent, kind.into(), model.into(), now());
        a.member_of = member_of;
        self.agents.insert(name.into(), a);
        self.norm_gen += 1;
        self.changed(name);
        ok(id)
    }

    /// Agent NAME is called TO from now on: its id, session, folder (agents/TO) and children go with it; mail and
    /// items already written keep the name they were written with. Only a stopped agent: a running one's processes
    /// carry its name (AGENT_NAME, its cgroup), so it is renamed between a stop and its restart (`roster rename`).
    pub fn rename(&mut self, who: &Who, name: &str, to: &str) -> Reply {
        if to.is_empty() || to.contains('/') || to == "owner" || to == "coord" {
            return err(format!("coord rename: not a name: {to:?}"));
        }
        if self.agents.contains_key(to) || self.is_host(to) {
            return err(format!("coord rename: {to} is taken"));
        }
        let Some(a) = self.agents.get(name) else { return err(format!("coord rename: no agent {name}")) };
        if !self.above(who, name) {
            return err(format!("coord rename: only the agents above {name} rename it"));
        }
        if a.live.pid.is_some_and(|p| std::path::Path::new(&format!("/proc/{p}")).exists()) {
            return err(format!("coord rename: {name} is running; stop it first (roster rename does both)"));
        }
        let (from_dir, to_dir) = (self.path(&format!("agents/{name}")), self.path(&format!("agents/{to}")));
        if std::path::Path::new(&from_dir).exists() {
            if let Err(e) = std::fs::rename(&from_dir, &to_dir) {
                return err(format!("coord rename: {from_dir}: {e}"));
            }
        }
        cgroup::forget(name);
        append(&self.path("registry.jsonl"), &json!({"ev": "rename", "name": name, "to": to, "at": now()}));
        self.renamed(name, to);
        self.norm_gen += 1;
        let snap = self.snapshot();
        self.emit(snap.body);
        self.journal();
        ok(String::new())
    }

    /// Agent NAME's role is ROLE from now on: the owner's to set (the teams are the owner's).
    pub fn set_role(&mut self, who: &Who, name: &str, role: &str) -> Reply {
        if !matches!(who, Who::Owner) {
            return err("coord role: only the owner sets roles".into());
        }
        // a role is a word the policy's norms speak of (agent-base/policy/norms.eflint: lead, consultant, ...)
        if role.is_empty() || !role.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return err(format!("coord role: not a role: {role:?} (a word, as the policy's norms name it)"));
        }
        let Some(a) = self.agents.get_mut(name) else { return err(format!("coord role: no agent {name}")) };
        a.role = role.into();
        self.norm_gen += 1;
        append(&self.path("registry.jsonl"), &json!({"ev": "role", "name": name, "role": role, "at": now()}));
        self.changed(name);
        ok(String::new())
    }

    /// `coord team ask TEXT`: agent WHO asks to form a team (as `ask-team` allows). TEXT goes as mail to every agent the
    /// norms let approve it (`approve-team`), else to the owner (vault designs/teams-and-tiers-2026-11-10.md §4).
    pub async fn team_ask(&mut self, who: &Who, text: &str) -> Reply {
        let Who::Agent(me) = who else { return err("coord team ask: an agent asks; the owner grants with `coord team grant`".into()) };
        if text.trim().is_empty() {
            return err("usage: coord team ask TEXT  (why a team, what it does, how many agents)".into());
        }
        if let Err(why) = self.permitted("ask-team", &[me], "none") {
            return err(format!("coord team ask: {me} may not ask for a team ({why})"));
        }
        let mut to: Vec<String> = self
            .agents
            .keys()
            .filter(|a| *a != me && matches!(self.norm_verdict("approve-team", &[a.as_str(), me.as_str()], "none"), Ok(Some(v)) if v.enabled))
            .cloned()
            .collect();
        if to.is_empty() {
            to.push("owner".into());
        }
        let body = format!("Team request from {me}\n\n{text}\n\nTo approve: `coord team grant {me} N`, N the number of agents its team may have.");
        self.msg(who, None, to, &body, false, false, None, Vec::new()).await
    }

    /// `coord team grant NAME N`: NAME's team may have N agents from now on (a new grant starts its count again). The
    /// owner, or an agent the norms let approve NAME's team (`approve-team`).
    pub async fn team_grant(&mut self, who: &Who, name: &str, granted: u32) -> Reply {
        if !self.agents.contains_key(name) {
            return err(format!("coord team grant: no agent {name}"));
        }
        let by = who.name();
        if let Who::Agent(me) = who {
            match self.norm_verdict("approve-team", &[me, name], "none") {
                Ok(Some(v)) if v.enabled => {}
                Ok(Some(v)) => return err(format!("coord team grant: {me} may not approve {name}'s team ({})", v.why)),
                Ok(None) => return err("coord team grant: the policy has no act approve-team: only the owner grants".into()),
                Err(e) => return err(format!("coord team grant: the norms: {e}")),
            }
        }
        self.agents.get_mut(name).unwrap().team = Some(TeamGrant { granted, used: 0, by: by.clone() });
        append(&self.path("registry.jsonl"), &json!({"ev": "team", "name": name, "granted": granted, "by": by, "at": now()}));
        self.norm_gen += 1;
        self.changed(name);
        let body = format!("Team granted: up to {granted} agent(s) in your team (by {by}). Add them with `roster fork {name} --name NEW TEXT` (a copy of your conversation) or `roster add NEW MODEL PROMPT-FILE` (a fresh agent).");
        self.msg(who, None, vec![name.into()], &body, false, false, None, Vec::new()).await
    }

    /// The proxy's models and their families (`main::read_models`), for the norms' `model` and `family` facts.
    pub fn set_models(&mut self, models: Vec<(String, String)>) {
        if models != self.models {
            self.models = models;
            self.norm_gen += 1;
        }
    }

    /// The model Claude Code's Agent tool runs for NAME's call INPUT: its `model` (an alias is the newest undated model
    /// of that line the proxy serves), else NAME's own (no model, or the `fork` type, which runs the parent's).
    fn subagent_model(&self, name: &str, input: &Value) -> Option<String> {
        let own = || self.agents[name].model.split(':').next().unwrap_or("").to_string();
        let ti = &input["tool_input"];
        let Some(m) = ti["model"].as_str().filter(|m| !m.is_empty() && ti["subagent_type"].as_str() != Some("fork")) else { return Some(own()) };
        if self.models.iter().any(|(id, _)| id == m) {
            return Some(m.into());
        }
        let line = format!("claude-{m}-");
        self.models
            .iter()
            .filter_map(|(id, _)| {
                let v: Option<Vec<u64>> = id.strip_prefix(&line)?.split('-').map(|p| if p.len() < 8 { p.parse().ok() } else { None }).collect();
                Some((v?, id))
            })
            .max()
            .map(|(_, id)| id.clone())
    }

    /// Claude Code's PreToolUse of the Agent tool by NAME: refused unless the norms enable `start-subagent` for its model;
    /// the answer is Claude Code's hook output (vault designs/teams-and-tiers-2026-11-10.md §4).
    fn subagent_check(&self, name: &str, input: &Value) -> String {
        let why = match self.subagent_model(name, input) {
            None => Some(format!("no model the proxy serves is called {:?}", input["tool_input"]["model"])),
            Some(m) => self.permitted("start-subagent", &[name, &m], "none").err().map(|w| format!("a subagent on {m}: {w}")),
        };
        match why {
            None => String::new(),
            Some(why) => json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "deny",
                "permissionDecisionReason": format!("The team policy refuses this subagent: {why}. A subagent runs a lower model tier of your own family: pass `model` (haiku, sonnet, opus, fable) below yours. For work that needs your tier or another family, ask for a team (`coord team ask`).")}})
            .to_string(),
        }
    }

    /// The agents the owner wrote to in THREAD ("none": no thread), sorted: the one part of the norms' situation a
    /// thread changes.
    fn brought_in(&self, thread: &str) -> Vec<String> {
        let mut out: Vec<String> = self.thread(thread).into_iter().filter(|m| m.from == "owner").flat_map(|m| m.to.iter().filter(|t| self.agents.contains_key(*t)).cloned()).collect();
        out.sort();
        out.dedup();
        out
    }

    /// The situation the norms are asked in (policy/norms.rs): every agent and its role (a member when it has none),
    /// the threads "none" and "t", and BROUGHT brought into "t". Every thread is asked as "t" with its own brought-in
    /// agents, so a situation solved once answers every thread with the same ones (the norms name no thread).
    fn norm_facts(&self, brought: &[String]) -> Vec<(String, Vec<String>)> {
        let mut f: Vec<(String, Vec<String>)> = Vec::new();
        for a in self.agents.values() {
            f.push(("agent".into(), vec![a.name.clone()]));
            f.push(("role".into(), vec![a.name.clone(), if a.role.is_empty() { "member".into() } else { a.role.clone() }]));
        }
        // teams and tiers (vault designs/teams-and-tiers-2026-11-10.md §3)
        for (m, fam) in &self.models {
            f.push(("model".into(), vec![m.clone()]));
            f.push(("family".into(), vec![m.clone(), fam.clone()]));
        }
        for a in self.agents.values() {
            f.push(("runs".into(), vec![a.name.clone(), a.model.split(':').next().unwrap_or("").to_string()]));
            if self.agents.contains_key(&a.parent) {
                f.push(("parent".into(), vec![a.name.clone(), a.parent.clone()]));
            }
            if let Some(g) = &a.team {
                f.push(("granted".into(), vec![a.name.clone(), g.granted.to_string()]));
                f.push(("used".into(), vec![a.name.clone(), g.used.to_string()]));
            }
            if a.member_of.is_some() {
                f.push(("member".into(), vec![a.name.clone()]));
            }
        }
        f.push(("thread".into(), vec!["none".into()]));
        f.push(("thread".into(), vec!["t".into()]));
        for b in brought {
            f.push(("brought-in".into(), vec![b.clone(), "t".into()]));
        }
        f
    }

    /// The norms' verdict on ACT for ARGS about THREAD: from the situations solved already (`NormCache`), solving only
    /// a situation not met since the policy, an agent or a role last changed (design coordd-core-2026-11-10 §5).
    /// None: the policy has no such act.
    fn norm_verdict(&self, act: &str, args: &[&str], thread: &str) -> Result<Option<crate::policy::norms::Verdict>, String> {
        let mut cache = self.norm_cache.borrow_mut();
        let now = std::time::Instant::now();
        let stale = match cache.as_mut() {
            None => true,
            Some(c) if c.generation != self.norm_gen => true,
            Some(c) if now.duration_since(c.checked) >= Duration::from_secs(2) => {
                c.checked = now;
                self.places().stamp() != c.stamp
            }
            Some(_) => false,
        };
        if stale {
            let places = self.places();
            *cache = Some(NormCache { stamp: places.stamp(), generation: self.norm_gen, checked: now, norms: crate::policy::norms::load(&places), solved: HashMap::new() });
        }
        let c = cache.as_mut().unwrap();
        let norms = c.norms.as_ref().map_err(|e| e.clone())?;
        let Some(a) = norms.acts.get(act) else { return Ok(None) };
        let brought = if thread == "none" { Vec::new() } else { self.brought_in(thread) };
        if !c.solved.contains_key(&brought) {
            let shown = norms.solve(&self.norm_facts(&brought));
            c.solved.insert(brought.clone(), shown);
        }
        let shown = c.solved[&brought].as_ref().map_err(|e| e.clone())?;
        // the thread fields are asked as "t" (or "none"), as the situation has them
        let args: Vec<&str> = args.iter().zip(a.fields.iter().map(|(_, ty)| ty.as_str()).chain(std::iter::repeat(""))).map(|(v, ty)| if ty == "thread" { if thread == "none" { "none" } else { "t" } } else { *v }).collect();
        norms.verdict(shown, act, &args)
    }

    /// Whether the norms enable ACT on ARGS (about THREAD): Ok, or why not. A policy without the act lets it through;
    /// one that cannot be read or solved does too, said on stderr, so a bad edit does not stop the team.
    fn permitted(&self, act: &str, args: &[&str], thread: &str) -> Result<(), String> {
        match self.norm_verdict(act, args, thread) {
            Ok(Some(v)) if !v.enabled => Err(v.why),
            Ok(_) => Ok(()),
            Err(e) => {
                eprintln!("coordd: norms: {act}({}): {e}; let through", args.join(", "));
                Ok(())
            }
        }
    }

    /// The norms' verdict on ACT for ARGS about THREAD (`coord norm`, the owner's: why mail is refused).
    pub fn norm(&self, act: &str, args: &[String], thread: &str) -> Reply {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        match self.norm_verdict(act, &args, thread) {
            Ok(Some(v)) => ok(format!("{}: {}", if v.enabled { "enabled" } else { "not enabled" }, v.why)),
            Ok(None) => ok(format!("the policy has no act {act}: nothing rules on it")),
            Err(e) => err(format!("coord norm: {e}")),
        }
    }

    /// The registry's side of a rename: the agent under its new name, its children under it.
    fn renamed(&mut self, name: &str, to: &str) {
        if let Some(mut a) = self.agents.remove(name) {
            a.name = to.into();
            self.agents.insert(to.into(), a);
        }
        for a in self.agents.values_mut().filter(|a| a.parent == name) {
            a.parent = to.into();
        }
    }

    /// PID becomes agent NAME (its cgroup), watched by its pidfd from then on.
    pub fn run(&mut self, caller: &cgroup::Caller, name: &str, pid: i32) -> Result<(), String> {
        let a = self.agents.get(name).ok_or_else(|| format!("coord run: no agent {name}; coord add registers it first"))?;
        let who = match caller {
            cgroup::Caller::Owner => Who::Owner,
            cgroup::Caller::Agent(c) => Who::Agent(c.clone()),
            cgroup::Caller::Unclaimed => Who::Owner, // a terminal holder's program: only a registered, stopped agent (below)
            cgroup::Caller::Project => return Err("coord run: a project source cannot start agents".into()),
        };
        if !self.above(&who, name) {
            return Err(format!("coord run: only the agents above {name} start it"));
        }
        if let Some(p) = a.live.pid {
            if std::path::Path::new(&format!("/proc/{p}")).exists() {
                return Err(format!("coord run: {name} is running (pid {p})"));
            }
        }
        cgroup::delegate(name).and_then(|_| cgroup::place(pid, &cgroup::agent_cgroup(name))).map_err(|e| format!("coord run: cgroup: {e}"))?;
        let a = self.agents.get_mut(name).unwrap();
        a.live = Live { pid: Some(pid), state: "idle".into(), since: now(), started: now(), ..Default::default() };
        a.background = None;
        // a new process's holder tells its own screen (`Live` starts with none)
        self.changed(name);
        Ok(())
    }

    /// Merge INPUT's session facts into NAME's session (a key INPUT lacks keeps its value: the Codex socket roster
    /// gave), record it, and write agents/NAME/session.json, which `roster restart` reads (coordd its one writer).
    fn merge_session(&mut self, name: &str, input: &Value) {
        let a = self.agents.get_mut(name).unwrap();
        let mut session = a.session.as_object().cloned().unwrap_or_default();
        for (k, v) in input.as_object().into_iter().flatten() {
            if ["session_id", "transcript_path", "cwd", "socket", "source"].contains(&k.as_str()) && !v.is_null() {
                session.insert(k.clone(), v.clone());
            }
        }
        a.session = Value::Object(session.clone());
        a.note_session();
        self.write_session(name);
        append(&self.path("registry.jsonl"), &json!({"ev": "session", "name": name, "session": Value::Object(session), "at": now()}));
    }

    /// Write agents/NAME/session.json from NAME's session, kind and model.
    fn write_session(&self, name: &str) {
        let a = &self.agents[name];
        let mut file = a.session.as_object().cloned().unwrap_or_default();
        file.insert("name".into(), json!(name));
        file.insert("kind".into(), json!(a.kind));
        file.insert("model".into(), json!(a.model));
        crate::jsonl::replace_later(&self.path(&format!("agents/{name}/session.json")), serde_json::to_vec(&file).unwrap());
    }

    /// NAME was set to MODEL with `/model` (its transcript), so a restart resumes it on MODEL, not the one it was added
    /// with (owner, 10 Nov: CONSULT, set to Fable 5.2 with `/model`, came back on Fable 5.1 at every restart; "if i
    /// change a model with /model it should never auto overwrite that"). Only a pick counts, not a reply's model: a
    /// fallback Claude Code makes on its own is no choice. An effort (`MODEL:EFFORT`, roster) stays.
    pub fn set_model(&mut self, name: &str, model: &str) {
        let Some(a) = self.agents.get_mut(name) else { return };
        let (old, effort) = match a.model.split_once(':') {
            Some((m, e)) => (m.to_string(), format!(":{e}")),
            None => (a.model.clone(), String::new()),
        };
        if model == old {
            return;
        }
        a.model = format!("{model}{effort}");
        let m = a.model.clone();
        append(&self.path("registry.jsonl"), &json!({"ev": "model", "name": name, "model": m, "at": now()}));
        self.write_session(name);
        self.changed(name);
    }

    /// `coord session NAME JSON`: session facts from an agent above NAME (roster, for a Codex thread).
    pub fn session(&mut self, who: &Who, name: &str, input: &Value) -> Reply {
        if !self.agents.contains_key(name) {
            return err(format!("coord session: no agent {name}"));
        }
        if !self.above(who, name) {
            return err(format!("coord session: only the agents above {name} set it"));
        }
        self.merge_session(name, input);
        self.changed(name);
        ok(String::new())
    }

    /// Agent NAME's process PID has exited.
    pub fn exited(&mut self, name: &str, pid: i32) {
        let Some(a) = self.agents.get_mut(name) else { return };
        if a.live.pid != Some(pid) {
            return;
        }
        a.live.pid = None;
        a.live.tools.clear();
        let kind = a.kind.clone();
        self.set_state(name, "stopped");
        self.changed(name);
        // liveness (supervision-nets §Shape, coordd 4): a process that ends is an event, whoever watches for it
        self.to_nets("agent_exit", Some(name.into()), json!({"name": name, "pid": pid, "kind": kind, "at": now()}));
    }

    // ---- hooks -------------------------------------------------------------------------------------------------

    /// A hook event of agent NAME. A Claude Code agent's turns, tool calls and mail are read from its transcript
    /// (`main::follow`): its hooks wake that reader and name the session (`start`), and change nothing else; an API
    /// error's reason (`failed`) is kept for its idle notice. A Codex agent's turns still follow its hooks.
    pub fn hook(&mut self, name: &str, event: &str, input: &Value) -> HookNext {
        if !self.agents.contains_key(name) {
            return HookNext::Done(String::new());
        }
        self.poke.notify_one();
        let claude = self.agents[name].kind != "codex";
        match event {
            "start" => {
                self.merge_session(name, input);
                let a = self.agents.get_mut(name).unwrap();
                if let Some(t) = a.transcript() {
                    a.background = Some(Background::new(&t, &a.live.started));
                }
                if self.agents[name].live.state != "working" {
                    self.set_state(name, "idle");
                }
                self.changed(name);
                HookNext::Done(String::new())
            }
            "prompt" => {
                // held mail goes in by one way only, the channel after the turn (`main::follow`)
                if !claude {
                    self.begin_turn(name);
                    self.changed(name);
                }
                HookNext::Done(String::new())
            }
            // a Claude Code agent's turns and tool calls are its transcript's (`main::follow`): its hooks only wake the
            // reader (above), so a hook costs nothing under the lock (design coordd-core-2026-11-10 §3)
            // the one hook that decides: Claude Code's Agent tool, by the team policy (vault designs/teams-and-tiers-2026-11-10.md)
            "pre" if claude && matches!(input["tool_name"].as_str(), Some("Agent" | "Task")) => HookNext::Done(self.subagent_check(name, input)),
            "pre" | "post" | "stop" if claude => HookNext::Done(String::new()),
            "pre" => {
                let id = s(input, "tool_use_id");
                let ti = input.get("tool_input").cloned().unwrap_or(Value::Null);
                let what = ["command", "file_path", "pattern", "description", "prompt"]
                    .iter()
                    .find_map(|k| ti.get(*k).and_then(Value::as_str))
                    .unwrap_or("")
                    .chars()
                    .take(200)
                    .collect();
                if !claude && self.agents[name].live.state != "working" {
                    // A tool call starts a turn only when it is the agent's own (in its transcript). Claude Code also runs tools outside any turn after one ends (8 Nov: DEBUG and LAZY
                    // stayed "working" for hours on a ScheduleWakeup and a `true` that no transcript holds, likely
                    // its run-ahead of the suggested next prompt), with no turn end to follow, so their mail was held.
                    // A subagent's tool (`agent_id`) outside a turn is a background agent's: the agent itself waits on
                    // it ("waiting on ..."), which takes mail like the end of a turn (owner m191, 9 Nov); a foreground
                    // subagent runs inside its parent's turn, which is already "working".
                    let own = input.get("agent_id").is_none();
                    if !own {
                        if input.get("agent_id").is_some() {
                            return HookNext::Done(String::new());
                        }
                        eprintln!("coordd: {name}: tool {} ({}) outside a turn and not in its transcript; no turn starts", s(input, "tool_name"), id);
                        return HookNext::Done(String::new());
                    }
                    self.begin_turn(name);
                }
                let a = self.agents.get_mut(name).unwrap();
                let at = now();
                a.live.last_tool = Some(at.clone());
                if !id.is_empty() {
                    a.live.tools.insert(id.clone(), Tool { id, tool: s(input, "tool_name"), what, at });
                }
                self.changed(name);
                HookNext::Done(String::new())
            }
            "post" => {
                let id = s(input, "tool_use_id");
                self.agents.get_mut(name).unwrap().live.tools.remove(&id);
                self.changed(name);
                HookNext::Done(String::new())
            }
            "failed" => {
                // the turn ended on an API error (Claude Code's StopFailure; Stop does not run then): it ended all the
                // same, and its parent is told why
                let why = [s(input, "error"), s(input, "error_details")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(": ");
                let a = self.agents.get_mut(name).unwrap();
                a.live.tools.clear();
                a.failed = Some(if why.is_empty() { "an API error".into() } else { why.chars().take(300).collect() });
                if claude { HookNext::Done(String::new()) } else { HookNext::CodexStop }
            }
            "stop" => {
                self.agents.get_mut(name).unwrap().live.tools.clear();
                HookNext::CodexStop // the Codex turn still runs at Stop: coordd waits for its end first
            }
            _ => HookNext::Done(String::new()),
        }
    }

    /// Agent NAME's turn ended (its transcript says so, or Codex's app-server): taken once per turn. Its turn number,
    /// the background work still running, and the turn's snapshot for the nets ([`Core::turn_end_snapshot`]); None
    /// when this turn's end was taken already. The waits, the state and the idle notice follow (`main::turn_waits`).
    pub fn end_turn(&mut self, name: &str) -> Option<(u64, Vec<String>, Option<TurnSnap>)> {
        let snap = self.turn_end_snapshot(name, None);
        let a = self.agents.get_mut(name)?;
        if a.ended_turn == a.live.turn {
            return None;
        }
        a.ended_turn = a.live.turn;
        a.live.tools.clear();
        let bg = a.background.as_mut().map(|b| b.still_running()).unwrap_or_default();
        Some((a.live.turn, bg, snap))
    }

    /// A correction the transcript made to what the hooks said (or did not), logged to count how often they miss.
    pub fn reconciled(&self, name: &str, what: &str) {
        eprintln!("coordd: reconcile {name}: {what}");
        append(&self.path("reconcile.jsonl"), &json!({"at": now(), "agent": name, "what": what}));
    }

    /// The second half: the state the turn ended in, and "X idle" to the parent when it waits on nothing.
    pub async fn turn_end_state(&mut self, name: &str, turn: u64, waits: Vec<String>, domain: Vec<String>) {
        if self.agents.get(name).map(|a| (a.live.turn, a.running())) != Some((turn, true)) {
            return; // a new turn began meanwhile, or the agent stopped
        }
        self.agents.get_mut(name).unwrap().bg_waits = waits.clone();
        self.settle_waits(name, waits, domain).await;
    }

    /// The project's waits read again (a runner event: up, down, a check ended): every running agent shown waiting
    /// restated from its background waits and its new project waits; one left waiting on nothing goes idle, with
    /// its parent's notice, as at a turn end. An agent in a turn is left to its turn end.
    pub async fn refresh_waits(&mut self, domain: HashMap<String, Vec<String>>) {
        let waiting: Vec<String> =
            self.agents.values().filter(|a| a.running() && a.live.state.starts_with("waiting on ")).map(|a| a.name.clone()).collect();
        for name in waiting {
            let bg = self.agents[&name].bg_waits.clone();
            self.settle_waits(&name, bg, domain.get(&name).cloned().unwrap_or_default()).await;
        }
    }

    /// NAME's state out of a turn: waiting on WAITS and DOMAIN, or idle with its parent told.
    async fn settle_waits(&mut self, name: &str, mut waits: Vec<String>, domain: Vec<String>) {
        waits.extend(domain);
        if waits.is_empty() {
            self.set_state(name, "idle");
            let parent = self.agents[name].parent.clone();
            let notice = match &self.agents[name].failed {
                Some(why) => format!("{name} idle: its turn ended on an API error ({why})"),
                None => format!("{name} idle"),
            };
            self.changed(name);
            self.notice(&parent, &notice).await;
        } else {
            self.set_state(name, &format!("waiting on {}", waits.join(", ")));
            self.changed(name);
        }
    }

    // ---- nets ---------------------------------------------------------------------------------------------------

    /// One line of the project's event source (`project::Header::events`): raised to the nets as its `event`, about
    /// its `to` agent if it names one; with `"notify": "owner"`, mailed to the owner from coord unless the same
    /// `fingerprint` was mailed within the hour (supervision-nets principle 5: one word per fault per window).
    pub async fn project_event(&mut self, v: Value) {
        let Some(event) = v.get("event").and_then(Value::as_str).map(str::to_owned) else {
            return eprintln!("coordd: project event without `event`: {v}");
        };
        let agent = v.get("to").and_then(Value::as_str).filter(|a| self.agents.contains_key(*a)).map(str::to_owned);
        self.to_nets(&event, agent, v.clone());
        if v.get("notify").and_then(Value::as_str) != Some("owner") {
            return;
        }
        let fp = v.get("fingerprint").and_then(Value::as_str).map_or_else(|| event.clone(), str::to_owned);
        let hour = Duration::from_secs(3600);
        if self.notified.get(&fp).is_some_and(|t| t.elapsed() < hour) {
            return;
        }
        self.notified.retain(|_, t| t.elapsed() < hour);
        self.notified.insert(fp.clone(), std::time::Instant::now());
        let field = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
        let what = [field("cause"), field("reason"), field("why"), field("excerpt")].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n");
        let body = format!("{event}: {fp}\n\n{what}\n\nat {} (the project's event source; the same fingerprint is not mailed again within the hour)", field("at"));
        self.notice("owner", &body).await;
    }

    /// Raise EVENT (about AGENT) with VALUE to the nets, when they run.
    pub fn to_nets(&self, event: &str, agent: Option<String>, value: Value) {
        if let Some(tx) = &self.nets {
            let _ = tx.send(crate::net::Raised { event: event.into(), agent, value, depth: 0, net: None });
        }
    }

    /// A new mail M to the nets: the owner's as `owner_message` (and `answer` when it answers a question), an
    /// agent's as `agent_message`. Mail of coord itself (notices) and of the nets is not raised again (§16.1).
    fn message_to_nets(&self, m: &Mail) {
        if m.from == "coord" || m.from.starts_with("net:") {
            return;
        }
        let thread = m.re.as_deref().and_then(|r| self.root_of(r));
        if m.from == "owner" {
            let agent = self.message_agent(m);
            let open: Vec<Value> = m.to.iter().flat_map(|t| self.open_questions(t)).collect();
            let quoted: Vec<Value> = m.quotes.iter().map(|q| json!({"source": q.source, "text": q.text})).collect();
            self.to_nets("owner_message", agent, json!({"id": m.id, "from": m.from, "text": m.body, "thread": thread, "to": m.to,
                "open": open, "context": self.context_of(m), "quoted": quoted}));
            if let Some(q) = m.re.as_deref().and_then(|r| self.thread_root(r)) {
                let asker = self.find_mail(&q).map(|x| x.from.clone());
                self.to_nets("answer", asker, json!({"q": q, "from": m.from, "text": m.body}));
            }
        } else if self.agents.contains_key(&m.from) || self.is_host(&m.from) {
            // a host session (a Claude Code session on the host, `coord msg` with COORD_AS) is one of the agents here
            self.to_nets("agent_message", Some(m.from.clone()), json!({"id": m.id, "from": m.from, "to": m.to, "text": m.body, "thread": thread}));
        }
    }

    /// What M is said in, for the clerk to read it by (9 Nov: "yes" to a point made in chat was unclear without it):
    /// the mails of its thread before it, else the latest mails between its sender and its recipients; at most
    /// [`CONTEXT`], oldest first, as the nets' `Said`.
    fn context_of(&self, m: &Mail) -> Vec<Value> {
        let between = |x: &Mail| {
            x.id != m.id && x.from != "coord" && !x.from.starts_with("net:")
                && ((x.from == m.from && x.to.iter().any(|t| m.to.contains(t))) || (m.to.contains(&x.from) && x.to.contains(&m.from)))
        };
        let before: Vec<&Mail> = match m.re.as_deref().and_then(|r| self.root_of(r)) {
            Some(root) => self.thread(&root).into_iter().filter(|x| x.id != m.id).collect(),
            None => {
                // the sender's mails and its recipients' mails: in mail order
                let mut ix: Vec<usize> = std::iter::once(&m.from).chain(&m.to).flat_map(|n| self.ix.by_from.get(n).into_iter().flatten().copied()).collect();
                ix.sort_unstable();
                ix.dedup();
                ix.into_iter().map(|i| &self.mails[i]).filter(|x| between(x)).collect()
            }
        };
        before[before.len().saturating_sub(CONTEXT)..]
            .iter()
            .map(|x| {
                // a long report is context, not the message: its start says what it is about
                let text = if x.body.chars().count() > CONTEXT_CHARS { format!("{}…", x.body.chars().take(CONTEXT_CHARS).collect::<String>()) } else { x.body.clone() };
                json!({"id": x.id, "from": x.from, "to": x.to, "text": text, "thread": x.re, "question": self.is_question(x)})
            })
            .collect()
    }

    /// Where the policy's, calls' and nets' files are (`crate::policy::Places`).
    pub fn places(&self) -> crate::policy::Places {
        crate::policy::Places { base: self.base.clone(), project: self.project.as_ref().map(|p| p.dir.clone()), coord: self.coord.clone(), ..Default::default() }
    }

    /// The agent a message's event is about: the owner's mail's one agent recipient (none for several), an agent's
    /// mail its sender.
    fn message_agent(&self, m: &Mail) -> Option<String> {
        if m.from == "owner" {
            let agents: Vec<&String> = m.to.iter().filter(|t| self.agents.contains_key(*t)).collect();
            (agents.len() == 1).then(|| agents[0].clone())
        } else {
            self.agents.contains_key(&m.from).then(|| m.from.clone())
        }
    }

    /// Whether M waits for its message net before delivery: the nets run and the live set has `deliver` on for it.
    fn gated(&self, m: &Mail) -> bool {
        if self.nets.is_none() || m.from == "coord" || m.from.starts_with("net:") || !(m.from == "owner" || self.agents.contains_key(&m.from) || self.is_host(&m.from)) {
            return false;
        }
        let mut live = self.live_cache.borrow_mut();
        if live.as_ref().is_none_or(|(at, _)| at.elapsed() >= Duration::from_secs(2)) {
            *live = Some((std::time::Instant::now(), crate::net::live(&self.places()).0));
        }
        live.as_ref().unwrap().1.is_live("deliver", self.message_agent(m).as_deref())
    }

    /// Mail ID's message net ran (or there was none): what it took. EFFECTS are the run's live `deliver`s and
    /// `reject`s on it, in order (FAULT: the run's fault, which passes the mail on as it was). Each taken
    /// effect gets its outcome; the mails to deliver now are returned. A deliver of the mail as it was (same text,
    /// same thread) passes it on; one that changes it is a new mail from the sender in its place; a reject is a mail
    /// from the clerk to the sender, and the mail is held until the owner releases it. A run that took none, or that
    /// stopped on a fault, passes it on unchanged (a failed call never loses or stalls a mail: 9 Nov, 22 failed calls
    /// in a row would each have sent the owner's message back).
    pub fn settle(&mut self, id: &str, effects: &mut [&mut NetEffect], fault: Option<&str>) -> Vec<(String, Mail)> {
        if !self.clerking.remove(id) {
            for e in effects.iter_mut() {
                e.done = Some("not taken: the mail was delivered already (deliver was dry when it was sent)".into());
            }
            return Vec::new();
        }
        let orig = self.find_mail(id).cloned().expect("a held mail is a mail");
        let orig_thread = orig.re.as_deref().and_then(|r| self.root_of(r));
        let mut out: Vec<Mail> = Vec::new();
        let (mut pass, mut made, mut back) = (Vec::<String>::new(), Vec::<String>::new(), None::<String>);
        for e in effects.iter_mut() {
            let a = e.args.clone();
            match e.effect.as_str() {
                "deliver" => {
                    let to: Vec<String> = match &a["to"] {
                        Value::Array(xs) => xs.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
                        Value::String(x) => vec![x.clone()],
                        _ => Vec::new(),
                    };
                    let (text, thread) = (a["text"].as_str().unwrap_or_default().trim().to_string(), a["thread"].as_str().map(String::from));
                    let bad: Vec<&String> = to.iter().filter(|t| !(self.agents.contains_key(*t) || *t == "owner")).collect();
                    if to.is_empty() || text.is_empty() || !bad.is_empty() {
                        e.done = Some(format!("not taken: no recipient, no text or unknown {bad:?}"));
                        continue;
                    }
                    if text == orig.body.trim() && thread == orig_thread && to.iter().all(|t| orig.to.contains(t)) {
                        pass.extend(to);
                        e.done = Some(format!("{id} passed on"));
                    } else {
                        if let Some(q) = &thread {
                            if self.find_mail(q).is_none() {
                                e.done = Some(format!("not taken: no thread {q}"));
                                continue;
                            }
                        }
                        let m = self.new_mail(&orig.from, to, &text, orig.esc, "", thread.or(orig.re.clone()), Vec::new());
                        e.done = Some(m.id.clone());
                        made.push(m.id.clone());
                        out.push(m);
                    }
                }
                "reject" => {
                    let why = a["why"].as_str().unwrap_or_default();
                    let m = self.new_mail("net:clerk", vec![orig.from.clone()], &format!("Not delivered ({id}): {why}\n\nYour message:\n> {}", orig.body.trim().replace('\n', "\n> ")), false, "", Some(id.into()), Vec::new());
                    e.done = Some(m.id.clone());
                    back = Some(why.to_string());
                    out.push(m);
                }
                _ => {}
            }
        }
        // a fault (a failed call, a net's own fault) never holds a mail: it goes on as it was, beside whatever the run
        // made before it stopped, and the fault stays in the run's log
        if let (Some(f), None) = (fault, &back) {
            append(&self.path("mail/log.jsonl"), &json!({"ev": "clerk_fault", "id": id, "fault": f, "at": now()}));
            let v = self.mail_view(&orig);
            self.emit(Body::Mail(v));
            let mut deliver: Vec<(String, Mail)> = out.into_iter().flat_map(|m| m.to.clone().into_iter().map(move |t| (t, m.clone()))).collect();
            deliver.extend(orig.to.iter().filter(|t| !pass.contains(t)).map(|t| (t.clone(), orig.clone())));
            deliver.extend(pass.into_iter().map(|t| (t, orig.clone())));
            deliver.retain(|(t, _)| self.agents.contains_key(t));
            return deliver;
        }
        let what = match (&back, made.is_empty()) {
            (Some(why), _) => Some(format!("sent back: {why}")),
            (None, false) => Some(format!("delivered as {}", made.join(", "))),
            (None, true) => None,
        };
        if let Some(w) = what.filter(|_| pass.is_empty()) {
            append(&self.path("mail/log.jsonl"), &json!({"ev": "clerked", "id": id, "what": w, "at": now()}));
            self.clerked.insert(id.into(), w);
        } else {
            pass = orig.to.clone();
        }
        let v = self.mail_view(&orig);
        self.emit(Body::Mail(v));
        let mut deliver: Vec<(String, Mail)> = out.into_iter().flat_map(|m| m.to.clone().into_iter().map(move |t| (t, m.clone()))).collect();
        if !pass.is_empty() && self.clerked.get(id).is_none() {
            deliver.extend(pass.into_iter().map(|t| (t, orig.clone())));
        }
        deliver.retain(|(t, _)| self.agents.contains_key(t));
        deliver
    }

    /// Hand DELIVER (agent, mail) to the agents now, as `coord msg` does.
    pub async fn deliver_all(&mut self, deliver: Vec<(String, Mail)>) {
        for (t, m) in deliver {
            self.deliver(&t, &m).await;
        }
    }

    /// `Request::Release`: the owner has a mail its net held or sent back delivered as it was.
    pub async fn release(&mut self, who: &Who, id: &str) -> Reply {
        if who.name() != "owner" {
            return err("release: the owner releases a held mail".into());
        }
        let Some(m) = self.find_mail(id).cloned() else { return err(format!("release: no mail {id}")) };
        let was = self.clerking.remove(id) | self.clerked.remove(id).is_some();
        if !was {
            return err(format!("release: {id} is not held by a net"));
        }
        append(&self.path("mail/log.jsonl"), &json!({"ev": "released", "id": id, "at": now()}));
        let v = self.mail_view(&m);
        self.emit(Body::Mail(v));
        for t in m.to.iter().filter(|t| self.agents.contains_key(*t)).cloned().collect::<Vec<_>>() {
            self.deliver(&t, &m).await;
        }
        ok(format!("{id} released"))
    }

    /// Agent NAME's turn ended: what its `turn_end` event needs that the next turn would change, taken at the turn's
    /// end (before held mail starts the next) and cheap, with no transcript read under the lock: where its transcript
    /// ended, its final message as the Stop hook gives it (LAST), what it sent through coordd, the question it asked
    /// with `coord ask` (`asked`) and its open questions to the owner (as they were then, so a replay sees the same).
    /// The turn's text is read from the transcript afterwards, off the lock ([`TurnSnap::text`]): reading it under the
    /// lock outlasted the Stop hook's 5 s on orch's 680 MB session, so its turn end was lost and its mail stayed held
    /// (owner m228, 9 Nov). None while the nets do not run.
    pub fn turn_end_snapshot(&self, name: &str, last: Option<&str>) -> Option<TurnSnap> {
        self.nets.as_ref()?;
        let a = self.agents.get(name)?;
        let path = a.transcript();
        let end = path.as_ref().and_then(|p| std::fs::metadata(p).ok()).map_or(0, |m| m.len());
        let sent: Vec<Value> = a
            .sent
            .iter()
            .map(|x| json!({"id": x["id"], "from": name, "to": x["to"], "text": x["text"], "thread": x["thread"], "question": x["kind"] == "ask"}))
            .collect();
        // the question it asked with `coord ask` in the turn, its last: the tool is the record, so the clerk files the
        // turn's plain-text restatements in its thread (owner m219, 9 Nov: q67 restated q66 as a thread of its own)
        let asked = a.sent.iter().rev().find(|x| x["kind"] == "ask").map(|x| x["id"].clone()).unwrap_or(Value::Null);
        Some(TurnSnap { name: name.into(), path, end, last: last.map(str::to_string), sent, asked, turn: a.live.turn, open: self.open_questions(name) })
    }

    /// The `turn_end` event's value from SNAP and the turn's TEXT ([`TurnSnap::text`]), and whether it is the same as the
    /// agent's previous turn end (an idle loop's repeat). Where it goes on (`continues`) and what it waits on (`waits`)
    /// come with [`Core::turn_end_to_nets`].
    pub fn turn_end_value(&mut self, snap: TurnSnap, text: String) -> Option<Value> {
        let a = self.agents.get_mut(&snap.name)?;
        let key = serde_json::to_string(&(&text, &snap.sent)).unwrap_or_default();
        let repeat = !text.is_empty() && a.last_turn_end.as_deref() == Some(key.as_str());
        a.last_turn_end = Some(key);
        Some(json!({"agent": snap.name, "turn": snap.turn, "text": text, "sent": snap.sent, "asked": snap.asked, "open": snap.open, "repeat": repeat}))
    }

    /// The turn end VALUE (from [`Core::turn_end_value`]) to the nets: CONTINUES when held mail went in as its next
    /// turn, WAITS what it waits on now (its background work and the project's waits: checks, landings).
    pub fn turn_end_to_nets(&self, name: &str, mut value: Value, continues: bool, waits: Vec<String>) {
        value["continues"] = json!(continues);
        value["waits"] = json!(waits);
        self.to_nets("turn_end", Some(name.into()), value);
    }

    /// What a net run may read of coordd now: each agent's open questions, and the nudges in a row from the run log.
    pub fn net_world(&self) -> crate::net::run::World {
        crate::net::run::World {
            questions: self.agents.keys().map(|n| (n.clone(), self.open_questions(n))).collect(),
            nudged: crate::net::nudge_counts(&self.coord),
            agents: self.watched(),
        }
    }

    /// The running agents as the nets' `Watched` (`agents()`, supervision-nets: agent_watch): its oldest held mail's
    /// age, how long it has been idle (0 when not), how long a turn has gone with no tool running and none begun
    /// (`quiet_min`), how long its oldest running tool has run (`tool_min`), in whole minutes, and its items' count.
    fn watched(&self) -> Vec<Value> {
        let mins = |at: &str| chrono::DateTime::parse_from_rfc3339(at).map_or(0, |t| (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_minutes().max(0));
        self.agents
            .values()
            .filter(|a| a.running())
            .map(|a| {
                let held = self.ix.held.get(&a.name).and_then(|h| h.iter().map(|&i| mins(&self.mails[i].at)).max()).unwrap_or(0);
                let state = a.live.state.as_str();
                let idle = if state == "idle" { mins(&a.live.since) } else { 0 };
                let working = state == "working";
                let moved = a.live.last_tool.as_deref().filter(|t| *t > a.live.since.as_str()).unwrap_or(&a.live.since);
                let quiet = if working && a.live.tools.is_empty() { mins(moved) } else { 0 };
                let tool = if working { a.live.tools.values().map(|t| mins(&t.at)).max().unwrap_or(0) } else { 0 };
                json!({"name": a.name, "parent": a.parent, "state": state, "held_min": held, "idle_min": idle, "quiet_min": quiet,
                       "tool_min": tool, "items": a.items.len()})
            })
            .collect()
    }

    /// AGENT's open questions to the owner, as the nets' `Question` (id, from, text, thread).
    pub fn open_questions(&self, agent: &str) -> Vec<Value> {
        self.mails_from(agent)
            .filter(|m| m.re.is_none() && self.is_question(m) && self.question_state(&m.id) == "open")
            .map(|m| json!({"id": m.id, "from": m.from, "text": m.body, "thread": m.id}))
            .collect()
    }

    /// A net's live `ask` (ARGS from, text, on): agent FROM asks the owner TEXT, or follows up in thread ON, as
    /// `coord ask` would; its id. Not when FROM asked it already (the same words, or one inside the other, in a
    /// question of its own not dropped): the clerk should have filed that piece in its thread, and a question is
    /// never opened twice.
    pub fn net_ask(&mut self, args: &Value) -> Result<Mail, String> {
        let (Some(from), Some(text)) = (args["from"].as_str(), args["text"].as_str()) else { return Err(format!("ask without from and text: {args}")) };
        let text = text.trim();
        if !self.agents.contains_key(from) {
            return Err(format!("no agent {from}"));
        }
        if text.is_empty() {
            return Err("an empty question".into());
        }
        let on = args["on"].as_str().map(String::from);
        if let Some(q) = &on {
            let root = self.thread_root(q).ok_or(format!("{q} is not a question"))?;
            if self.find_mail(&root).is_some_and(|r| r.from != from) {
                return Err(format!("{root} is another agent's question"));
            }
        }
        if let Some(q) = self.mails_from(from).find(|m| self.is_question(m) && !self.dropped.contains(&m.id) && said_already(&m.body, text)) {
            return Err(format!("{from} asked it already as {}", q.id));
        }
        let to = self.asked_to(from);
        Ok(self.new_mail(from, to, text, false, "question", on, Vec::new()))
    }

    /// Who a question of agent FROM goes to: the owner, and FROM's lead (its parent agent, e.g. orch) so the lead
    /// can take it on before the owner reaches it (owner m242, 9 Nov). An agent the owner started has no lead.
    fn asked_to(&self, from: &str) -> Vec<String> {
        let lead = self.agents.get(from).map(|a| a.parent.clone()).filter(|p| self.agents.contains_key(p));
        std::iter::once("owner".to_string()).chain(lead).collect()
    }

    /// Question M to its lead (`asked_to`) now, or held until the lead's turn ends.
    pub async fn ask_lead(&mut self, m: &Mail) {
        for t in m.to.iter().filter(|t| *t != "owner").cloned().collect::<Vec<_>>() {
            self.deliver(&t, m).await;
        }
    }

    /// A net's live `post` (ARGS from, text, thread): agent FROM's words TEXT put in its question THREAD, to the
    /// owner; its id.
    pub fn net_post(&mut self, args: &Value) -> Result<String, String> {
        let (Some(from), Some(text), Some(thread)) = (args["from"].as_str(), args["text"].as_str(), args["thread"].as_str()) else {
            return Err(format!("post without from, text and thread: {args}"));
        };
        let root = self.thread_root(thread).ok_or(format!("{thread} is not a question"))?;
        if self.find_mail(&root).is_some_and(|r| r.from != from) {
            return Err(format!("{root} is another agent's question"));
        }
        if text.trim().is_empty() {
            return Err("nothing to post".into());
        }
        // an agent that asked in the same turn has its words in the thread already: filing them again repeats them
        let said = self.thread(&root).into_iter().find(|m| m.from == from && !self.unfiled.contains(&m.id) && said_already(&m.body, text));
        if let Some(m) = said {
            return Err(format!("already in {root} as {}", m.id));
        }
        Ok(self.new_mail(from, vec!["owner".into()], text.trim(), false, "", Some(root), Vec::new()).id)
    }

    /// Run RUN filed the mails IDS: recorded, and each told again with it.
    pub fn mark_filed(&mut self, ids: &[String], run: &str) {
        for id in ids {
            append(&self.path("mail/log.jsonl"), &json!({"ev": "filed", "id": id, "run": run, "at": now()}));
            self.filed.insert(id.clone(), run.into());
            if let Some(m) = self.find_mail(id) {
                let v = self.mail_view(m);
                self.emit(Body::Mail(v));
            }
        }
    }

    /// `Request::Unfile`: the owner takes back what a net filed: a filed question is a question no more, a filed reply
    /// is hidden from its thread.
    pub fn unfile(&mut self, who: &Who, id: &str) -> Reply {
        if who.name() != "owner" {
            return err("unfile: the owner takes back what a net filed".into());
        }
        let Some(run) = self.filed.get(id).cloned() else { return err(format!("unfile: {id} was not filed by a net")) };
        if self.unfiled.insert(id.into()) {
            append(&self.path("mail/log.jsonl"), &json!({"ev": "unfile", "id": id, "run": run, "at": now()}));
            let v = self.mail_view(self.find_mail(id).expect("a filed id is a mail"));
            self.emit(Body::Mail(v));
            self.thread_changed(id);
        }
        ok(format!("{id} unfiled (filed by {run})"))
    }

    /// A net ran: kept for the snapshot and told.
    pub fn net_ran(&mut self, run: NetRun) {
        self.net_runs.push_back(run.clone());
        while self.net_runs.len() > NET_RUNS {
            self.net_runs.pop_front();
        }
        self.emit(Body::Net(run));
    }

    /// The owner marked a part of a run: the run, if still among the recent ones, told again with its marks.
    pub fn net_marked(&mut self, m: &coord_types::NetMark) {
        let marks = crate::net::marks(&self.coord);
        if let Some(r) = self.net_runs.iter_mut().find(|r| r.id == m.run) {
            *r = crate::net::with_marks(r.clone(), &marks);
            let r = r.clone();
            self.emit(Body::Net(r));
        }
    }

    /// The nets' check found ERRORS (told when they changed).
    pub fn net_check(&mut self, errors: Vec<String>) {
        if errors != self.net_errors {
            self.net_errors = errors.clone();
            self.emit(Body::Neterrors(errors));
        }
    }

    // ---- mail --------------------------------------------------------------------------------------------------

    /// Whether the owner holds the agents' mail (`$COORD/hold` exists, owner 10 Nov: the box back up with every agent
    /// idle until the owner writes to them), looked at again when 2 s old: then only the owner's mail sent since the
    /// hold began goes; the rest stays held until the file is removed. Some(first mail position the hold lets go).
    fn hold(&self) -> Option<usize> {
        let mut h = self.hold.borrow_mut();
        if h.0.is_none_or(|at| at.elapsed() >= Duration::from_secs(2)) {
            let on = std::path::Path::new(&self.path("hold")).exists();
            h.1 = match (on, h.1) {
                (true, None) => Some(self.mails.len()),
                (true, was) => was,
                (false, _) => None,
            };
            h.0 = Some(std::time::Instant::now());
        }
        h.1
    }

    fn pending(&self, name: &str) -> Vec<Mail> {
        let hold = self.hold();
        let going = self.inflight.get(name);
        let waits = |m: &&Mail| {
            let key = (m.id.clone(), name.to_string());
            !self.delivered.contains(&key) && !self.withdrawn.contains(&key) && !self.clerking.contains(&m.id) && !self.clerked.contains_key(&m.id)
                && !going.is_some_and(|g| g.ids.contains(&m.id))
        };
        let lets_go = |i: &usize| hold.is_none_or(|from| *i >= from && self.mails[*i].from == "owner");
        self.ix.held.get(name).map_or(Vec::new(), |h| h.iter().filter(|i| lets_go(i)).map(|&i| &self.mails[i]).filter(waits).cloned().collect())
    }

    /// The held mail A takes now: all of it out of a turn; in one (a Claude Code agent's) only the mail sent to go in
    /// soon.
    fn handed_now(&self, a: &Agent) -> Vec<Mail> {
        let held = self.pending(&a.name);
        if a.live.state != "working" || a.ended_turn == a.live.turn {
            return held;
        }
        if a.kind != "claude" {
            return Vec::new();
        }
        held.into_iter().filter(|m| m.soon).collect()
    }

    fn mark(&mut self, name: &str, ms: &[Mail], how: &str) {
        for m in ms {
            append(&self.path("mail/log.jsonl"), &json!({"ev": "delivered", "id": m.id, "to": name, "at": now(), "how": how}));
            self.delivered.insert((m.id.clone(), name.into()));
            self.unhold(&m.id, name);
            self.emit(Body::Delivered { id: m.id.clone(), to: name.into(), how: how.into() });
        }
    }

    /// A new mail (KIND `question`: a question, numbered apart as `qN`); a thread it joins shows its new state.
    fn new_mail(&mut self, from: &str, to: Vec<String>, body: &str, esc: bool, kind: &str, re: Option<String>, quotes: Vec<Quote>) -> Mail {
        let n = if kind == "question" { self.ix.questions } else { self.ix.messages } + 1;
        let id = if kind == "question" { format!("q{n}") } else { format!("m{n}") };
        let m = Mail { id, at: now(), from: from.into(), to, body: body.into(), esc, soon: false, kind: kind.into(), re, quotes };
        let mut rec = serde_json::to_value(&m).unwrap();
        rec["ev"] = json!("mail");
        append(&self.path("mail/log.jsonl"), &rec);
        let i = self.index_mail(m.clone());
        for t in &m.to {
            self.ix.held.entry(t.clone()).or_default().insert(i);
        }
        let v = self.mail_view(&m);
        self.emit(Body::Mail(v));
        self.thread_changed(&m.id);
        m
    }

    /// The first question of ID's thread again, with its new state.
    fn thread_changed(&mut self, id: &str) {
        if let Some(root) = self.thread_root(id).filter(|r| r != id) {
            let v = self.mail_view(self.find_mail(&root).expect("a thread's root is a mail"));
            self.emit(Body::Mail(v));
        }
    }

    /// `coord ask`: a question to the owner, or with ON a follow-up in the thread of ON. It does not wait.
    pub async fn ask(&mut self, who: &Who, body: &str, on: Option<String>) -> Reply {
        let Who::Agent(me) = who else { return err("coord ask: agents ask the owner; the owner answers with mail".into()) };
        if body.trim().is_empty() {
            return err("usage: coord ask [--on QID] SUBJECT  (the full explanation on stdin)".into());
        }
        if let Some(q) = &on {
            let Some(root) = self.thread_root(q) else { return err(format!("coord ask: {q} is not a question or in a question's thread")) };
            if self.find_mail(&root).is_some_and(|r| r.from != *me) {
                return err(format!("coord ask --on: {root} is another agent's question"));
            }
        }
        let to = self.asked_to(me);
        let m = self.new_mail(&me.clone(), to, body, false, "question", on, Vec::new());
        if let Some(a) = self.agents.get_mut(me) {
            a.sent.push(json!({"kind": "ask", "id": m.id, "to": ["owner"], "text": m.body, "thread": m.re}));
        }
        self.to_nets("question", Some(me.clone()), json!({"q": m.id, "from": me, "text": m.body}));
        self.ask_lead(&m).await;
        ok(format!("{}: asked; the owner's answer comes as mail. Go on with your work (do not wait for it).", m.id))
    }

    /// `coord ask --list`: WHO's questions and the question threads WHO was brought into (the owner: every one), oldest first, each as its id, state, when and
    /// subject, then the rest of its thread: the owner's words in full (the answers), the agents' follow-ups and
    /// replies by their subject.
    pub fn questions(&self, who: &Who) -> Reply {
        let me = who.name();
        let roots: Vec<&Mail> = self
            .mails
            .iter()
            .filter(|m| self.is_question(m) && m.re.is_none() && (me == "owner" || m.from == me || self.participants(&m.id).contains(&me)))
            .collect();
        if roots.is_empty() {
            return ok("no questions".into());
        }
        let when = |at: &str| at.get(..16).unwrap_or(at).replace('T', " ");
        let out: Vec<String> = roots
            .iter()
            .map(|q| {
                let by = if q.from != me { format!(" · {}", q.from) } else { String::new() };
                let mut lines = vec![format!("{} · {}{by} · asked {} · {}", q.id, self.question_state(&q.id), when(&q.at), coord_types::subject(&q.body))];
                for m in self.thread(&q.id).into_iter().filter(|m| m.id != q.id) {
                    let text = if m.from == "owner" { m.body.trim().replace('\n', "\n    ") } else { coord_types::subject(&m.body) };
                    let what = if m.from == "owner" { "answer" } else if m.kind == "question" { "follow-up" } else { "reply" };
                    lines.push(format!("  {} · {what} from {} · {}\n    {text}", m.id, m.from, when(&m.at)));
                }
                lines.join("\n")
            })
            .collect();
        ok(out.join("\n\n"))
    }

    /// `coord ask --drop`: the agent that asked a question, or the owner, withdraws it (its whole thread).
    pub fn drop_question(&mut self, who: &Who, id: &str, why: Option<String>) -> Reply {
        let Some(root) = self.thread_root(id) else { return err(format!("coord ask --drop: {id} is not a question")) };
        let me = who.name();
        if me != "owner" && self.find_mail(&root).is_some_and(|r| r.from != me) {
            return err(format!("coord ask --drop: {root} is another agent's question"));
        }
        if self.dropped.insert(root.clone()) {
            append(&self.path("mail/log.jsonl"), &json!({"ev": "drop", "id": root, "by": me, "why": why, "at": now()}));
            let v = self.mail_view(self.find_mail(&root).expect("a thread's root is a mail"));
            self.emit(Body::Mail(v));
        }
        ok(format!("{root} dropped"))
    }

    /// The recipients of mail ID still waiting for it, when WHO sent it.
    fn waiting_for(&self, who: &Who, id: &str, verb: &str) -> Result<(Mail, Vec<String>), String> {
        let Some(m) = self.find_mail(id).cloned() else { return Err(format!("coord {verb}: no mail {id}")) };
        if m.from != who.name() {
            return Err(format!("coord {verb}: {id} is {}'s, not yours", m.from));
        }
        let left: Vec<String> = m.to.iter().filter(|t| {
            let key = (m.id.clone(), (*t).clone());
            !self.delivered.contains(&key) && !self.withdrawn.contains(&key) && self.agents.contains_key(*t)
        }).cloned().collect();
        if left.is_empty() {
            return Err(format!("coord {verb}: {id} has reached everyone it was for"));
        }
        Ok((m, left))
    }

    /// Mark mail ID to go in mid-turn (`Mail::soon`), journalled.
    fn set_soon(&mut self, id: &str) {
        let Some(&i) = self.ix.id.get(id) else { return };
        if self.mails[i].soon {
            return;
        }
        self.mails[i].soon = true;
        append(&self.path("mail/log.jsonl"), &json!({"ev": "soon", "id": id, "at": now()}));
        let v = self.mail_view(&self.mails[i].clone());
        self.emit(Body::Mail(v));
    }

    /// Take mail ID back from the recipients it has not reached (the composer's Cancel and Edit).
    pub fn unsend(&mut self, who: &Who, id: &str) -> Reply {
        let (m, left) = match self.waiting_for(who, id, "unsend") {
            Ok(x) => x,
            Err(e) => return err(e),
        };
        for t in &left {
            append(&self.path("mail/log.jsonl"), &json!({"ev": "withdrawn", "id": m.id, "to": t, "at": now()}));
            self.withdrawn.insert((m.id.clone(), t.clone()));
            self.unhold(&m.id, t);
        }
        let v = self.mail_view(&m);
        self.emit(Body::Mail(v));
        for t in &left {
            self.changed(t);
        }
        ok(format!("{id} taken back from {}", left.join(", ")))
    }

    /// Deliver mail ID now to the recipients still waiting for it, stopping their turns first (the composer's Send now).
    /// SOON: mail ID goes in after its recipients' next tool calls, no turn stopped (`Mail::soon`); else their turns
    /// stop first.
    pub async fn send_now(&mut self, who: &Who, id: &str, soon: bool) -> Reply {
        let (mut m, left) = match self.waiting_for(who, id, "now") {
            Ok(x) => x,
            Err(e) => return err(e),
        };
        if soon {
            self.set_soon(id);
            self.poke.notify_one();
            return ok(format!("{id} goes in after the next tool call"));
        }
        if let Some(n) = left.iter().find(|t| !self.above(who, t)) {
            return err(format!("coord now: stopping a turn is for agents below you; {n} is not"));
        }
        m.esc = true;
        let mut notes = Vec::new();
        for t in &left {
            if let Some(n) = self.deliver(t, &m).await {
                notes.push(n);
            }
        }
        ok(std::iter::once(format!("{id} sent now")).chain(notes).collect::<Vec<_>>().join("; "))
    }

    pub async fn msg(&mut self, who: &Who, as_host: Option<String>, to: Vec<String>, body: &str, esc: bool, soon: bool, re: Option<String>, quotes: Vec<Quote>) -> Reply {
        let from = match (&who, as_host) {
            (Who::Owner, Some(h)) => h,
            (_, Some(_)) => return err("coord msg: only the owner side sends for a host session".into()),
            _ => who.name(),
        };
        // `coord msg --thread ID`: everyone in ID's thread but the sender
        let to = match (&re, to.is_empty()) {
            (Some(r), true) => match self.root_of(r) {
                // a reply in a thread leaves out those the norms do not let its sender mail
                Some(root) => self.participants(&root).into_iter().filter(|n| *n != from && !(self.agents.contains_key(&from) && self.agents.contains_key(n) && self.permitted("send-mail", &[&from, n, &root], &root).is_err())).collect(),
                None => return err(format!("coord msg: no mail or question {r}")),
            },
            _ => to,
        };
        if to.is_empty() || body.trim().is_empty() {
            return err("usage: coord msg [--esc] [--re ID] NAME[,NAME...] TEXT | coord msg --thread ID TEXT".into());
        }
        let bad: Vec<&String> = to.iter().filter(|t| !(self.agents.contains_key(*t) || self.is_host(t) || *t == "owner")).collect();
        if !bad.is_empty() {
            let names: Vec<&String> = self.agents.keys().collect();
            return err(format!("coord: no agent or host named {} (agents: {})", bad.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
                names.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" ")));
        }
        let root = re.as_deref().and_then(|r| self.root_of(r));
        // an agent's mail to another goes by the team's norms (agent-base/policy/norms.eflint, send-mail)
        if self.agents.contains_key(&from) {
            let thread = root.clone().unwrap_or_else(|| "none".into());
            for n in to.iter().filter(|t| self.agents.contains_key(*t)) {
                if let Err(why) = self.permitted("send-mail", &[&from, n, &thread], &thread) {
                    let lead = self.agents.get(&from).map_or("owner".to_string(), |a| a.parent.clone());
                    return err(format!("coord msg: the team's norms do not let you mail {n}: {why}. Take it up with your lead ({lead}) or the owner"));
                }
            }
        }
        if esc {
            if let Some(n) = to.iter().find(|t| self.agents.contains_key(*t) && !self.above(who, t)) {
                return err(format!("coord msg: --esc only to agents below you; {n} is not"));
            }
        }
        if let Some(r) = &re {
            if self.find_mail(r).is_none() {
                return err(format!("coord msg: no mail or question {r}"));
            }
        }
        let mut m = self.new_mail(&from, to.clone(), body, esc, "", re, quotes);
        if soon && !esc {
            self.set_soon(&m.id);
            m.soon = true;
        }
        if let Some(a) = self.agents.get_mut(&from) {
            a.sent.push(json!({"kind": "msg", "id": m.id, "to": m.to, "text": m.body, "thread": m.re}));
        }
        if self.gated(&m) {
            self.clerking.insert(m.id.clone());
            let v = self.mail_view(&m);
            self.emit(Body::Mail(v));
            self.message_to_nets(&m);
            return ok(format!("{}; with the clerk (its message net) before delivery", m.id));
        }
        self.message_to_nets(&m);
        // an agent's new thread to the owner is read for a question (a reply belongs to its thread's kind)
        if self.agents.contains_key(&from) && to.iter().any(|t| t == "owner") && m.re.is_none() {
            if let Some(tx) = &self.recognise {
                let _ = tx.send(m.clone());
            }
        }
        let mut notes = Vec::new();
        for t in &to {
            if *t == from {
                continue;
            }
            if self.agents.contains_key(t) {
                if let Some(n) = self.deliver(t, &m).await {
                    notes.push(n);
                }
            } else if self.is_host(t) {
                append(&self.path("feed.jsonl"), &json!({"at": now(), "from": from, "kind": "msg", "id": m.id, "to": t,
                    "text": format!("[{} · from {}]\n{}", m.id, m.from, m.body), "delivery": "relay"}));
                notes.push(format!("queued for host {t} (coord-relay delivers it)"));
            }
        }
        ok(std::iter::once(m.id.clone()).chain(notes).collect::<Vec<_>>().join("; "))
    }

    /// Mail M for agent NAME: it goes when NAME is not in a turn (`main::follow` hands it over); esc stops a running
    /// turn first. A note when it waits.
    async fn deliver(&mut self, name: &str, m: &Mail) -> Option<String> {
        if !self.agents[name].running() {
            return Some(format!("{name} is not running; held, delivered with its next turn"));
        }
        if !matches!(self.agents[name].kind.as_str(), "claude" | "codex") {
            return Some(format!("{name} is a program with no session; held for coord mail"));
        }
        self.poke.notify_one();
        if m.esc && self.agents[name].live.state == "working" {
            self.interrupt(name);
            return Some(format!("{name}'s turn is being stopped; it gets the mail then"));
        }
        if m.soon && self.agents[name].kind == "claude" && self.agents[name].live.state == "working" {
            return Some(format!("{name} is working; it gets the mail after its next tool call"));
        }
        if self.agents[name].live.state == "working" {
            return Some(format!("{name} is working; held until its turn ends"));
        }
        None
    }

    /// After an interrupt: a Codex agent's turn is over (a Claude Code agent's transcript says so itself).
    pub fn interrupted(&mut self, name: &str) {
        if self.agents.get(name).is_some_and(|a| a.kind == "codex") {
            self.agents.get_mut(name).unwrap().live.tools.clear();
            self.set_state(name, "idle");
            self.changed(name);
            self.poke.notify_one();
        }
    }

    /// NAME's running tool calls as its transcript shows them now (Claude Code: the record, `follow.rs`).
    pub fn set_tools(&mut self, name: &str, tools: Vec<Tool>) {
        let Some(a) = self.agents.get_mut(name) else { return };
        if let Some(t) = tools.iter().map(|t| t.at.clone()).max() {
            a.live.last_tool = Some(t);
        }
        a.live.tools = tools.into_iter().map(|t| (t.id.clone(), t)).collect();
        self.changed(name);
    }

    /// The running agents that may take mail now (not in a turn, nothing on its way to them, not waiting to be tried
    /// again) and have mail held.
    pub fn to_hand_over(&self) -> Vec<String> {
        let now = std::time::Instant::now();
        self.agents
            .values()
            // a program (the runner's terminal) has no session to take mail: it stays held, readable with coord mail
            // not in a turn: idle, waiting, or its turn's end taken while its waits are still being found (`turn_waits`
            // reads the project's waits, seconds): held mail goes first, as the Stop hook's answer did
            // a Claude Code agent in a turn takes the mail sent to go in soon: its session queues it for after the
            // running tool call
            .filter(|a| a.running() && (a.kind == "claude" || a.kind == "codex") && !self.inflight.contains_key(&a.name))
            .filter(|a| self.retry_at.get(&a.name).is_none_or(|t| *t <= now))
            .filter(|a| !self.handed_now(a).is_empty())
            .map(|a| a.name.clone())
            .collect()
    }

    /// Hand NAME's held mail to its session: a job for its sender (`main::senders`); the mail is on its way until the
    /// sender has the session's answer (`handed`): taken is delivered, not taken is held and tried again.
    pub fn hand_over(&mut self, name: &str) {
        let held = self.handed_now(&self.agents[name]);
        let Some(jobs) = self.jobs.clone() else { return };
        if held.is_empty() || !self.agents[name].running() {
            return;
        }
        let a = &self.agents[name];
        let ids: Vec<String> = held.iter().map(|m| m.id.clone()).collect();
        let job = if a.kind == "codex" {
            Job::Codex { name: name.into(), ids: ids.clone(), base: self.base.clone(), session: s(&a.session, "session_id"), socket: a.socket(), text: self.batch_text(name, &held) }
        } else {
            let events: Vec<Value> = held.iter().map(|m| json!({"content": self.delivery_text(m, name), "meta": {"from": m.from, "id": m.id}})).collect();
            // the channel of the session the hooks named (channel.py: one socket per session), else the shared one
            let own = self.path(&format!("agents/{name}/channel-{}.sock", s(&a.session, "session_id")));
            let sock = if std::path::Path::new(&own).exists() { own } else { self.path(&format!("agents/{name}/channel.sock")) };
            Job::Channel { name: name.into(), ids: ids.clone(), sock, line: format!("{}\n", json!({"events": events})) }
        };
        let tries = self.retry_tries.remove(name).unwrap_or(0);
        for id in &ids {
            append(&self.path("mail/log.jsonl"), &json!({"ev": "sent", "id": id, "to": name, "at": now(), "try": tries + 1}));
        }
        self.inflight.insert(name.into(), Inflight { ids, tries: tries + 1 });
        let _ = jobs.send(job);
    }

    /// The sender's answer for NAME's mail IDS: its session took them (Codex: delivered; Claude Code: now waiting to
    /// be seen in the transcript), or did not (held again, tried again after a pause).
    pub fn handed(&mut self, name: &str, ids: &[String], took: bool, how: &str) {
        if !self.inflight.get(name).is_some_and(|g| g.ids == ids) {
            return; // seen meanwhile, or handed again
        }
        if !took {
            let g = self.inflight.remove(name).unwrap();
            let pause = Duration::from_secs((5u64 << g.tries.min(4)).min(60));
            self.retry_at.insert(name.into(), std::time::Instant::now() + pause);
            self.reconciled(name, &format!("its session did not take {}; again in {} s", ids.join(" "), pause.as_secs()));
            self.retry_tries.insert(name.into(), g.tries); // the next hand-over counts on from them
            return;
        }
        // taken is delivered (design coordd-core-2026-11-10 §4): the session has it, queued behind a menu or not
        self.inflight.remove(name);
        self.retry_tries.remove(name);
        let ms: Vec<Mail> = ids.iter().filter_map(|id| self.find_mail(id).cloned()).collect();
        self.mark(name, &ms, how);
        if how == "codex" {
            self.begin_turn(name);
        }
        self.changed(name);
    }

    /// NAME's transcript showed mail IDS: delivered, if no acknowledgement said so already (a session from before).
    pub fn seen(&mut self, name: &str, ids: &[String]) {
        let mut done = Vec::new();
        for id in ids {
            let key = (id.clone(), name.to_string());
            if self.delivered.contains(&key) {
                continue;
            }
            // a queued message and its entry may come in one read: once
            if let Some(m) = self.find_mail(id).filter(|m| m.to.iter().any(|t| t == name) && !done.iter().any(|d: &Mail| d.id == m.id)).cloned() {
                done.push(m);
            }
        }
        if let Some(g) = self.inflight.get_mut(name) {
            g.ids.retain(|i| !ids.contains(i));
            if g.ids.is_empty() {
                self.inflight.remove(name);
                self.retry_tries.remove(name);
            }
        }
        if !done.is_empty() {
            self.mark(name, &done, "channel");
            self.changed(name);
        }
    }

    /// A mail from coord, delivered like any other.
    async fn notice(&mut self, to: &str, body: &str) {
        let m = self.new_mail("coord", vec![to.into()], body, false, "", None, Vec::new());
        if self.agents.contains_key(to) {
            let _ = self.deliver(to, &m).await;
        }
    }


    /// esc: stop NAME's running turn, by its sender (`main::senders`). Claude Code: the one keystroke Escape, typed
    /// through its terminal's holder (`coord-pty`), after which its transcript shows the turn ended; Codex: interrupt.
    fn interrupt(&mut self, name: &str) {
        let Some(jobs) = self.jobs.clone() else { return };
        let a = &self.agents[name];
        let codex = (a.kind == "codex").then(|| (self.base.clone(), s(&a.session, "session_id"), a.socket()));
        let _ = jobs.send(Job::Interrupt { name: name.into(), pty: coord_types::pty::socket(&self.coord, name).to_string_lossy().into_owned(), codex });
    }

    pub fn mail_list(&self, who: &Who, id: Option<&str>) -> Reply {
        let me = who.name();
        if let Some(id) = id {
            let Some(m) = self.find_mail(id) else { return err(format!("coord mail: no mail {id}")) };
            let mut out = format!("{}  {}  from {} to {}{}\n", m.id, m.at, m.from, m.to.join(", "), if m.esc { "  (esc)" } else { "" });
            for t in &m.to {
                let key = (m.id.clone(), t.clone());
                let got = if self.delivered.contains(&key) { "delivered" } else if self.withdrawn.contains(&key) { "withdrawn" } else { "held" };
                out += &format!("  {t}: {got}\n");
            }
            out += &m.body;
            return ok(out);
        }
        let lines: Vec<String> = self
            .mails
            .iter()
            .filter(|m| me == "owner" || m.from == me || m.to.contains(&me))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .take(40)
            .rev()
            .map(|m| format!("{:>6} {} {} -> {}: {}", m.id, m.at.get(11..16).unwrap_or(""), m.from, m.to.join(","), subject(&m.body)))
            .collect();
        ok(lines.join("\n"))
    }

    pub fn state_lines(&self) -> Reply {
        let lines: Vec<String> = self.agents.values().map(|a| format!("{}\t{}\t{}\t{}", a.name, a.id, a.live.state, a.parent)).collect();
        ok(lines.join("\n"))
    }

    pub fn log(&self, who: &Who, name: &str, last: usize) -> Reply {
        let Some(a) = self.agents.get(name) else { return err(format!("coord log: no agent {name}")) };
        // beyond the agents above it, whom the norms let read it (read-session: the consultant)
        // (a permission to read: refused when the norms cannot say, unlike mail, which goes through)
        let allowed = || match who {
            Who::Agent(me) => matches!(self.norm_verdict("read-session", &[me, name], "none"), Ok(Some(v)) if v.enabled),
            _ => false,
        };
        if !self.above(who, name) && !allowed() {
            return err(format!("coord log: only the agents above {name}, and those the norms let (read-session), read its session"));
        }
        let Some(path) = a.transcript() else { return err(format!("coord log: {name} has no session yet")) };
        ok(transcript::log_text(name, &path, last.max(1)))
    }

    /// A page of NAME's session as entries, for yaaihh (agents above NAME, as for `coord log`).
    pub fn transcript_page(&self, who: &Who, name: &str, before: Option<u64>, after: Option<u64>) -> Reply {
        let Some(a) = self.agents.get(name) else { return err(format!("coord transcript: no agent {name}")) };
        if !self.above(who, name) {
            return err(format!("coord transcript: only the agents above {name} read its session"));
        }
        let page = a.transcript().map(|p| coord_types::Transcript { session: p.clone(), ..transcript::page(&p, before, after) }).unwrap_or_default();
        ok(serde_json::to_string(&page).unwrap())
    }

    /// Image N of the line at OFFSET of NAME's transcript SESSION (a file its sessions were in: the current one or an
    /// earlier one of the same agent's, as a chat may still show an older part).
    pub fn transcript_image(&self, who: &Who, name: &str, session: &str, offset: u64, n: u32) -> Reply {
        let Some(a) = self.agents.get(name) else { return err(format!("coord image: no agent {name}")) };
        if !self.above(who, name) {
            return err(format!("coord image: only the agents above {name} read its session"));
        }
        let Some(current) = a.transcript() else { return err(format!("coord image: {name} has no session")) };
        // only a transcript beside the agent's current one: a request cannot name any file
        let beside = std::path::Path::new(&current).parent() == std::path::Path::new(session).parent() && session.ends_with(".jsonl");
        if session != current && !beside {
            return err(format!("coord image: {session} is not one of {name}'s sessions"));
        }
        match transcript::image(session, offset, n) {
            Some((media, data)) => ok(serde_json::json!({"media": media, "data": data}).to_string()),
            None => err(format!("coord image: no image {n} at byte {offset} of {session}")),
        }
    }

    // ---- items -------------------------------------------------------------------------------------------------

    fn apply_item(&mut self, r: &ItemRecord) {
        match r.ev.as_str() {
            "closed" => {
                self.closed.insert(r.id.clone());
            }
            "opened" => {
                self.closed.remove(&r.id);
            }
            "started" => {
                if let Some(a) = self.agents.get_mut(&r.by) {
                    if !a.items.contains(&r.id) {
                        a.items.push(r.id.clone());
                    }
                }
            }
            _ => {}
        }
    }

    fn record_item(&mut self, rec: ItemRecord) {
        append(&self.path("items/log.jsonl"), &serde_json::to_value(&rec).unwrap());
        self.apply_item(&rec);
        self.items.push(rec.clone());
        self.emit(Body::Item(rec));
    }

    pub fn item(&mut self, who: &Who, action: &str, arg: &str, why: Option<String>) -> Reply {
        let by = who.name();
        match action {
            "start" => {
                let Who::Agent(me) = who else { return err("coord item start: an agent says what it works on".into()) };
                if !self.items.iter().any(|r| r.ev == "opened" && r.id == arg) {
                    return err(format!("coord item start: no item {arg}"));
                }
                if self.closed.contains(arg) {
                    return err(format!("coord item start: {arg} is closed"));
                }
                self.record_item(ItemRecord { ev: "started".into(), id: arg.into(), at: now(), by: by.clone(), title: None, why: None });
                let me = me.clone();
                self.changed(&me);
                ok(format!("{me} works on {arg}"))
            }
            "open" => {
                if let Err(e) = self.may(who, "open-item") {
                    return err(format!("coord item open: {e}"));
                }
                let prefix = self.project.as_ref().map(|p| p.project.item_prefix.clone()).unwrap_or_else(|| "I-".into());
                let next = self.items.iter().filter_map(|r| r.id.strip_prefix(&prefix).and_then(|n| n.parse::<u32>().ok())).max().unwrap_or(0) + 1;
                let id = format!("{prefix}{next:03}");
                let note = self.project.as_ref().and_then(|p| p.project.notes.clone()).map(|dir| write_note(&dir, &id, arg, &by));
                self.record_item(ItemRecord { ev: "opened".into(), id: id.clone(), at: now(), by, title: Some(arg.into()), why: None });
                match note {
                    Some(Ok(path)) => ok(format!("{id}\t{path}")),
                    Some(Err(e)) => ok(format!("{id} (no note: {e})")),
                    None => ok(id),
                }
            }
            "close" => {
                if let Err(e) = self.may(who, "close-item") {
                    return err(format!("coord item close: {e}"));
                }
                let Some(why) = why.filter(|w| !w.trim().is_empty()) else { return err("usage: coord item close ID WHY".into()) };
                if self.closed.contains(arg) {
                    return err(format!("coord item close: {arg} is closed already"));
                }
                self.record_item(ItemRecord { ev: "closed".into(), id: arg.into(), at: now(), by, title: None, why: Some(why) });
                let working: Vec<String> = self.agents.values().filter(|a| a.items.contains(&arg.to_string())).map(|a| a.name.clone()).collect();
                for n in working {
                    self.changed(&n);
                }
                ok(format!("{arg} closed"))
            }
            _ => err("usage: coord item start ID | open TITLE | close ID WHY".into()),
        }
    }

    /// Agent NAME's screen shows a turn running (BUSY) or none, as its terminal holder saw it: the state of a Claude
    /// Code agent from then on, taken by the transcript's reader (`main::follow`), woken for it.
    pub fn busy(&mut self, name: &str, busy: bool) {
        if let Some(a) = self.agents.get_mut(name).filter(|a| a.kind == "claude") {
            a.live.screen_busy = Some(busy);
            self.poke.notify_one();
            self.journal();
        }
    }

    /// A dialog TITLE on agent NAME's screen, as its terminal holder saw it (`Request::Popup`): the owner is told the
    /// first time a title is seen (`$COORD/popups.jsonl` keeps every report), so a new popup is noticed; nothing answers
    /// it (owner, 10 Nov).
    pub fn popup(&mut self, name: &str, title: &str, screen: &str) {
        if self.popups.is_none() {
            self.popups = Some(read_jsonl(&self.path("popups.jsonl")).iter().map(|r| s(r, "title")).collect());
        }
        let new = self.popups.as_mut().unwrap().insert(title.to_string());
        append(&self.path("popups.jsonl"), &json!({"at": now(), "agent": name, "title": title, "new": new, "screen": screen}));
        if new {
            let body = format!("New dialog on {name}'s screen: {title}\n\nFirst time coord-pty saw this one; nothing answered it. The screen:\n\n{screen}");
            self.new_mail("coord", vec!["owner".into()], &body, false, "", None, Vec::new());
        }
    }

    /// A view source's line: its new data (or, with none, the data it had) and error; sent on only when it changed.
    pub fn set_view(&mut self, id: &str, data: Option<Value>, error: Option<String>) {
        let v = self.views.entry(id.into()).or_insert_with(|| ViewData { id: id.into(), data: None, error: None });
        let data = data.or_else(|| v.data.take());
        if (&v.data, &v.error) == (&data, &error) {
            return;
        }
        (v.data, v.error) = (data, error);
        let v = v.clone();
        self.emit(Body::View(v));
    }
}

/// A turn end as coordd took it from the transcript ([`Core::turn_end_snapshot`]).
pub struct TurnSnap {
    name: String,
    path: Option<String>,
    /// The transcript's length when the turn ended: its text is read up to here, so the next turn's entries are not.
    end: u64,
    last: Option<String>,
    sent: Vec<Value>,
    asked: Value,
    turn: u64,
    open: Vec<Value>,
}

impl TurnSnap {
    /// What the agent wrote in the turn: its messages since the turn's prompt (the last user entry or mail), ending
    /// with the final message the Stop hook gave. Reads the transcript: call it off the lock.
    pub fn text(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(path) = &self.path {
            let page = transcript::page(path, Some(self.end), None);
            let from = page.entries.iter().rposition(|e| e.kind == "user" || e.kind == "mail").map_or(0, |i| i + 1);
            parts = page.entries[from..].iter().filter(|e| e.kind == "assistant").map(|e| e.text.trim().to_string()).filter(|t| !t.is_empty()).collect();
        }
        if let Some(l) = self.last.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
            if parts.last().map(String::as_str) != Some(l) {
                parts.push(l.to_string());
            }
        }
        parts.join("\n\n")
    }
}

pub enum HookNext {
    Done(String),
    /// A Codex turn ends: coordd waits for its app-server to say so, then takes it (`Core::end_turn`).
    CodexStop,
}

impl Job {
    pub fn name(&self) -> &str {
        match self {
            Job::Channel { name, .. } | Job::Codex { name, .. } | Job::Interrupt { name, .. } => name,
        }
    }
}

/// The file a Claude Code session really writes, when the TRANSCRIPT path its hook gave does not exist: the same file
/// name in another project folder. Claude Code names the path after the session's current directory, but a session
/// resumed from another directory goes on writing the file in the folder where it began (CONE and MEANING, begun in
/// .wt/cone and .wt/meaning and resumed from their agda/ subfolders, showed empty chats; 9 Nov, m151). None when the
/// path exists or no other folder has the file.
fn written_transcript(transcript: &str) -> Option<String> {
    let path = std::path::Path::new(transcript);
    if path.exists() {
        return None;
    }
    let file = path.file_name()?;
    let projects = path.parent()?.parent()?;
    std::fs::read_dir(projects).ok()?.flatten().map(|d| d.path().join(file)).find(|p| p.is_file()).map(|p| p.to_string_lossy().into_owned())
}

/// What a whole-state event is of (the agent, the project view, the mail, the nets' findings): a newer event of the
/// same carries all it did, so the ring keeps only the newer.
fn event_key(b: &Body) -> Option<String> {
    match b {
        Body::Agent(a) => Some(format!("a {}", a.name)),
        Body::View(v) => Some(format!("v {}", v.id)),
        Body::Mail(m) => Some(format!("m {}", m.id)),
        Body::Neterrors(_) => Some("n".into()),
        _ => None,
    }
}

/// The note of a new item: DIR/ID-SLUG.md, owned like DIR (coordd runs as root; the agents edit notes).
fn write_note(dir: &str, id: &str, title: &str, by: &str) -> Result<String, String> {
    use std::os::unix::fs::MetadataExt;
    let slug: String = title
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    let path = format!("{dir}/{id}-{slug}.md");
    let day = &now()[..10];
    let text = format!("---\ntitle: {title}\nopened: {day}\nby: {by}\n---\n\n# {id} {title}\n\n## Log\n- {day} opened by {by}.\n");
    let meta = std::fs::metadata(dir).map_err(|e| format!("{dir}: {e}"))?;
    std::fs::write(&path, text).map_err(|e| format!("{path}: {e}"))?;
    let _ = std::os::unix::fs::chown(&path, Some(meta.uid()), Some(meta.gid()));
    Ok(path)
}

pub fn ok(out: String) -> Reply {
    Reply { ok: true, out }
}

pub fn err(out: String) -> Reply {
    Reply { ok: false, out }
}

/// Whether NEW says what OLD says (or a part of it, or OLD a part of NEW): the same words in the same order, ignoring
/// case, spacing, punctuation and markup (a clerk's piece drops a list's "1." or a heading's line break); a part counts
/// from 20 characters on.
fn said_already(old: &str, new: &str) -> bool {
    let norm = |t: &str| t.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ").to_lowercase();
    let (old, new) = (norm(old), norm(new));
    old == new || (new.len() >= 20 && old.contains(&new)) || (old.len() >= 20 && new.contains(&old))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With `deliver` live, the owner's mail waits for its net: a run that changes it delivers the new mail in its
    /// place and never the original; a reject sends it back and holds it until the owner releases it; a run that took
    /// nothing passes it on unchanged.
    #[test]
    fn a_held_message_goes_as_its_net_says() {
        let tmp = std::env::temp_dir().join(format!("coordd-held-{}", std::process::id()));
        let (coord, base) = (tmp.join("coord"), tmp.join("base"));
        std::fs::create_dir_all(base.join("nets")).unwrap();
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(base.join("nets/live"), "live deliver\nlive reject\n").unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"orch\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base.display().to_string());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.nets = Some(tx);
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let send = |core: &mut Core, text: &str| {
            let r = rt.block_on(core.msg(&Who::Owner, None, vec!["orch".into()], text, false, None, Vec::new()));
            r.out.split(';').next().unwrap().to_string()
        };
        let eff = |effect: &str, args: Value| NetEffect { line: 1, effect: effect.into(), args, live: true, done: None };

        // replaced: the spell-checked text goes, the original never
        let id = send(&mut core, "plese merge it");
        assert!(core.clerking.contains(&id) && core.pending("orch").is_empty());
        assert_eq!(rx.try_recv().unwrap().event, "owner_message");
        let mut d = eff("deliver", json!({"to": ["orch"], "text": "please merge it", "thread": null}));
        let out = core.settle(&id, &mut [&mut d], None);
        let new = d.done.clone().unwrap();
        assert_eq!(out.iter().map(|(t, m)| (t.as_str(), m.id.as_str())).collect::<Vec<_>>(), vec![("orch", new.as_str())]);
        assert_eq!(core.pending("orch").iter().map(|m| m.id.clone()).collect::<Vec<_>>(), vec![new.clone()]);
        assert!(core.clerked[&id].contains(&new));

        // sent back, then released by the owner
        let id = send(&mut core, "do the thing");
        let mut r = eff("reject", json!({"to": "owner", "why": "which thing?"}));
        let out = core.settle(&id, &mut [&mut r], None);
        assert!(out.is_empty(), "the reject goes to the owner, not an agent");
        assert!(core.clerked[&id].starts_with("sent back") && !core.pending("orch").iter().any(|m| m.id == id));
        assert!(core.mails.iter().any(|m| m.from == "net:clerk" && m.to == vec!["owner".to_string()] && m.body.contains("which thing?")));
        assert!(rt.block_on(core.release(&Who::Owner, &id)).out.contains("released"));
        assert!(core.pending("orch").iter().any(|m| m.id == id));

        // nothing taken (no net bound, a failed call): passed on unchanged
        let id = send(&mut core, "go on");
        let out = core.settle(&id, &mut [], None);
        assert_eq!(out.len(), 1);
        assert!(core.pending("orch").iter().any(|m| m.id == id) && !core.clerked.contains_key(&id));
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// A net files an agent's words in its question once: words already in the thread (the question it asked in the
    /// same turn, or an earlier post) are not posted again; new words are.
    #[test]
    fn a_post_never_repeats_the_thread() {
        let tmp = std::env::temp_dir().join(format!("coordd-post-{}", std::process::id()));
        let (coord, base) = (tmp.join("coord"), tmp.join("base"));
        std::fs::create_dir_all(base.join("nets")).unwrap();
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"orch\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base.display().to_string());
        let q = core.net_ask(&json!({"from": "orch", "text": "Two consumers can't be restated alone:\n\n1. **Should** I restate StartBlank over a term?"})).unwrap().id;
        let post = |core: &mut Core, text: &str| core.net_post(&json!({"from": "orch", "text": text, "thread": q}));
        assert!(post(&mut core, "Should I  restate StartBlank over a term?").unwrap_err().contains(&q));
        let m = post(&mut core, "The runner is green again.").unwrap();
        assert!(post(&mut core, "the runner is green again.").unwrap_err().contains(&m));
        std::fs::remove_dir_all(&tmp).ok();
    }
}

/// Run `codex-rpc ARGS` against the Codex app-server SOCKET for thread SESSION; whether it succeeded.
pub async fn codex_rpc(base: &str, session: &str, socket: &str, args: &[&str]) -> bool {
    let mut cmd = tokio::process::Command::new(format!("{base}/bin/codex-rpc"));
    cmd.arg(args[0]).arg(session).args(&args[1..]).env("CODEX_SOCK", socket);
    cmd.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    cmd.status().await.map(|s| s.success()).unwrap_or(false)
}
