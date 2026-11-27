//! The raw pane (owner m750: "when testing I also want to see the raw that is sent to the models"): what an agent's
//! model was sent and what it answered, line by line, beside its chat and terminal, from the model proxy's log of each
//! request (the proxylog crate; read by coordd, `RawPage`, from `RAW_PAGE_PROTOCOL` on, on Windows and Linux alike,
//! N-802; from an older coordd, read here on the host where coordd says the log is, N-40): system prompt, tools,
//! messages, reply. Its one source (owner m817: the transcript
//! switch is gone). It follows the agent: a finished request shows at the bottom about a second later. No summary; cut
//! only when a line is very long (proxylog::RawPage). What a request repeats of the one before it in the session
//! folds to one line, a click opening it (owner m983): its leading messages that are the same, and its system prompt
//! and tools when unchanged; from the first message that differs (a compaction, an edit) nothing folds. The fold is
//! the pane's only: coordd sends every line. Shown as a chat (owner, m904): each request
//! a block under a head across the pane (time, model, status, how many messages), each labelled entry of it a bubble
//! with its label as a caption, on the left what is sent to the model (system, tools, user entries and tool results,
//! params), on the right what the model said (assistant entries of the history, the reply); JSON indented. Every
//! entry folds to its caption and opens again on a click on its caption, and a click on a request's head folds the
//! whole request (owner m1361, design §14); what is folded is kept per pane while the window lives.

use crate::{App, Pending, Sel};
use proxylog::RawPage;
use coord_types::{RawSource, Request};
use rune::{El, GrowthSpec, ScrollSpec, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// How often a shown raw pane asks for what was added.
const EVERY: Duration = Duration::from_millis(1000);
/// The most lines a pane keeps; older ones go (and come back with Earlier).
const KEEP: usize = 4000;

/// The raw panes' logs, by project and agent.
#[derive(Default)]
pub struct Raws {
    pub logs: HashMap<(usize, String), Log>,
    /// The one thread that reads the proxy's log for every pane (megabytes a page: never where frames are built).
    reader: Option<Reader>,
    /// The `raw` scene with YAAIHH_RAW_LIVE=N: the pane's read is asked at its Nth poll, after it has opened.
    scene_read: Option<(usize, String, u32)>,
    /// The panes shown at the last poll: one shown since is asked at once, what it kept from before dropped.
    was_shown: Vec<(usize, String)>,
}

/// The panes' reader: the reads asked, one at a time in order, and what came of them. A pane asks no second read
/// while one is in flight (`Log::newer_pending`, `older_pending`), so a slow read does not pile up.
struct Reader {
    jobs: std::sync::mpsc::Sender<Job>,
    done: std::sync::mpsc::Receiver<Done>,
}

/// A read of agent AGENT's requests in project PI's proxy log at DIR (a folder on this host) for SESSION.
struct Job {
    pi: usize,
    agent: String,
    older: bool,
    dir: String,
    session: String,
    before: Option<u64>,
    after: Option<u64>,
    /// A turn's requests: all those between the two times.
    span: Option<(u64, u64)>,
}

struct Done {
    pi: usize,
    agent: String,
    older: bool,
    page: RawPage,
}

/// The reader, started on its first read.
fn reader(r: &mut Option<Reader>) -> &Reader {
    r.get_or_insert_with(|| {
        let (jobs, asked) = std::sync::mpsc::channel::<Job>();
        let (said, done) = std::sync::mpsc::channel::<Done>();
        let _ = std::thread::Builder::new().name("raw-pane".into()).spawn(move || {
            for j in asked {
                let page = match j.span {
                    Some((since, until)) => proxylog::turn_page(&j.dir, &j.session, since, until),
                    None => proxylog::page(&j.dir, &j.session, j.before, j.after),
                };
                if said.send(Done { pi: j.pi, agent: j.agent, older: j.older, page }).is_err() {
                    break;
                }
            }
        });
        Reader { jobs, done }
    })
}

/// Where coordd's reply OUT (OK or refused) says agent's log is, on this host with home HOME: the folder and the
/// session, or what the pane says instead (coordd's refusal; a `proxy_log` that is not `~/` or absolute).
fn located(ok: bool, out: &str, home: &str) -> Result<(String, String), String> {
    let src = match serde_json::from_str::<RawSource>(out) {
        Ok(src) if ok => src,
        _ => return Err(said(out)),
    };
    Ok((proxylog::host_dir(&src.dir, home)?, src.session))
}

/// What the reader has read since: into each pane. Whether any came.
fn take_read(s: &mut App) -> bool {
    let Some(r) = &s.raw.reader else { return false };
    let done: Vec<Done> = r.done.try_iter().collect();
    let any = !done.is_empty();
    for d in done {
        let log = s.raw.logs.entry((d.pi, d.agent)).or_default();
        if d.older { log.older_pending = false } else { log.newer_pending = false }
        log.refused = None;
        merge(log, d.page, d.older);
    }
    any
}

/// What a raw pane holds of an agent's session. A turn's, once read, is copied whole into a window of its own
/// (sent_window.rs), its folds its own from then on.
#[derive(Clone, Default)]
pub struct Log {
    pub session: String,
    pub lines: Vec<String>,
    /// The time of the first request held, and just after the last (proxylog.rs `RawPage`).
    pub start: u64,
    pub next: u64,
    /// The time of each request whose head line is held, in their order (`RawPage::heads`).
    times: Vec<u64>,
    /// Each row's key: what it is (its request's head line, which holds its time, and its place in that request),
    /// not where it sits among the lines held, so the cut at KEEP leaves the rows that stay as they were.
    keys: Vec<String>,
    pub loaded: bool,
    newer_pending: bool,
    older_pending: bool,
    asked: Option<Instant>,
    pub refused: Option<String>,
    /// What its lines are read from when no gateway's log holds them, in his words (`RawPage::about`: a session's own
    /// record, fix 5), on a hover over the pane's count; none from a log.
    pub about: Option<String>,
    /// What the pane shows of LINES, folds made (`layout`), and the folds opened, by key: open while the pane lives.
    rows: Vec<Row>,
    open: HashSet<String>,
    /// The entries and requests folded by a click, by key (`entry_key`; a request's key is its head line): folded
    /// while the pane lives.
    shut: HashSet<String>,
    /// A turn's requests under a reply (turn.rs): from its prompt's time to just after its reply, read once, each
    /// request folded to its head when it first comes (`seen`).
    until: Option<(u64, u64)>,
    seen: HashSet<String>,
}

/// A row of the pane.
#[derive(Clone, Debug, PartialEq)]
enum Row {
    /// Line I of the log; for a request's head or entry, its fold key and whether it is folded.
    Line(usize, Option<(String, bool)>),
    /// What a request repeats of the one before: KEY (its head line and what), its words, whether it is open (its
    /// lines then follow as rows).
    Fold { key: String, label: String, open: bool },
    /// A word about the request above it.
    Note(String),
}

/// What part of a request line L is.
#[derive(Clone, Copy, PartialEq)]
enum Part {
    System,
    Tools,
    /// Message N of the history.
    Message(u32),
    Other,
}

fn part(l: &str) -> Part {
    if l.starts_with("[system]") {
        return Part::System;
    }
    if l.starts_with("[tools] ") || l.starts_with("[tool ") {
        return Part::Tools;
    }
    match l.strip_prefix('[').and_then(|r| r.split([' ', ']']).next()).and_then(|n| n.parse::<u32>().ok()) {
        Some(n) => Part::Message(n),
        None => Part::Other,
    }
}

/// The rows LINES shows: each request after the first held compared with the one before it (the true one before: the
/// held lines are contiguous, and Earlier puts the older ones above). HELD_BEFORE: the log starts after the session's
/// first request, so the first request held has one before it that is not held: it is shown whole, and says so.
fn layout(lines: &[String], held_before: bool, open: &HashSet<String>, shut: &HashSet<String>) -> Vec<Row> {
    let heads: Vec<usize> = lines.iter().enumerate().filter(|(_, l)| l.starts_with("══ ")).map(|(i, _)| i).collect();
    let mut rows: Vec<Row> = (0..heads.first().copied().unwrap_or(lines.len())).map(|i| Row::Line(i, None)).collect();
    // the request before's parts: its system lines, its tools lines, its messages in order
    type Parts<'a> = (Vec<&'a str>, Vec<&'a str>, Vec<(u32, Vec<&'a str>)>);
    let parts_of = |r: std::ops::Range<usize>| -> Parts {
        let (mut sys, mut tools, mut msgs): Parts = (Vec::new(), Vec::new(), Vec::new());
        for l in &lines[r] {
            match part(l) {
                Part::System => sys.push(l),
                Part::Tools => tools.push(l),
                Part::Message(n) => match msgs.last_mut() {
                    Some((m, ls)) if *m == n => ls.push(l),
                    _ => msgs.push((n, vec![l])),
                },
                Part::Other => {}
            }
        }
        (sys, tools, msgs)
    };
    let mut before: Option<Parts> = None;
    for (j, &h) in heads.iter().enumerate() {
        let end = heads.get(j + 1).copied().unwrap_or(lines.len());
        let now = parts_of(h + 1..end);
        // which lines fold, and under which fold: (what, words)
        let mut fold_of: HashMap<usize, (&str, String)> = HashMap::new();
        if let Some((sys, tools, msgs)) = &before {
            let at = |want: &dyn Fn(Part) -> bool| (h + 1..end).filter(|&i| want(part(&lines[i]))).collect::<Vec<_>>();
            // the system prompt's parts the same as the request before's at the same place; one that changed (a
            // header that varies per request) shows
            let sys_at = at(&|p| p == Part::System);
            let kept: Vec<usize> = sys_at.iter().copied().enumerate().filter(|&(n, i)| sys.get(n) == Some(&lines[i].as_str())).map(|(_, i)| i).collect();
            if !kept.is_empty() {
                let words = if kept.len() == sys_at.len() && sys.len() == sys_at.len() { "system prompt, as before".to_string() } else { format!("{} of {} parts unchanged", kept.len(), sys_at.len()) };
                for i in kept {
                    fold_of.insert(i, ("system", words.clone()));
                }
            }
            if !now.1.is_empty() && now.1 == *tools {
                let n = now.1.iter().filter(|l| l.starts_with("[tool ")).count();
                for i in at(&|p| p == Part::Tools) {
                    fold_of.insert(i, ("tools", format!("{n} tool{}, as before", if n == 1 { "" } else { "s" })));
                }
            }
            // the leading messages the same as the request before's; from the first that differs, none
            let same: Vec<u32> = now.2.iter().zip(msgs.iter()).take_while(|(a, b)| a == b).map(|(a, _)| a.0).collect();
            if !same.is_empty() {
                let words = format!("{} message{} as in the request before", same.len(), if same.len() == 1 { "" } else { "s" });
                for i in at(&|p| matches!(p, Part::Message(n) if same.contains(&n))) {
                    fold_of.insert(i, ("messages", words.clone()));
                }
            }
        }
        let folded = shut.contains(&lines[h]);
        rows.push(Row::Line(h, Some((lines[h].clone(), folded))));
        if folded {
            before = Some(now);
            continue;
        }
        // line I of this request as a row: an entry folds by its own key
        let line = |i: usize| match entry(&lines[i]) {
            Entry::Said { .. } => {
                let key = entry_key(&lines[h], i - h);
                let folded = shut.contains(&key);
                Row::Line(i, Some((key, folded)))
            }
            _ => Row::Line(i, None),
        };
        if before.is_none() && held_before {
            rows.push(Row::Note("shown whole".into()));
        }
        let mut made: HashSet<&str> = HashSet::new();
        for i in h + 1..end {
            match fold_of.get(&i) {
                None => rows.push(line(i)),
                Some((what, words)) => {
                    let key = format!("{}|{what}", lines[h]);
                    let opened = open.contains(&key);
                    if made.insert(what) {
                        rows.push(Row::Fold { key, label: words.clone(), open: opened });
                    }
                    if opened {
                        rows.push(line(i));
                    }
                }
            }
        }
        before = Some(now);
    }
    rows
}

/// The fold key of the entry N lines below the request whose head line is HEAD: the same when Earlier puts older
/// lines above it.
fn entry_key(head: &str, n: usize) -> String {
    format!("{head}|{n}")
}

impl Log {
    fn relayout(&mut self) {
        self.rows = layout(&self.lines, self.start > 0, &self.open, &self.shut);
        self.keys = row_keys(&self.lines, &self.rows);
    }
}

/// Each of ROWS' keys, by what it is: a line by its request's head line and its place below it (a line before any
/// request by its own words), with "-folded" when folded; a fold by its own key; the note by itself.
fn row_keys(lines: &[String], rows: &[Row]) -> Vec<String> {
    let mut head_of: Vec<Option<usize>> = Vec::with_capacity(lines.len());
    let mut h = None;
    for (i, l) in lines.iter().enumerate() {
        if l.starts_with("══ ") {
            h = Some(i);
        }
        head_of.push(h);
    }
    rows.iter()
        .map(|r| match r {
            Row::Line(j, f) => {
                let at = match head_of.get(*j).copied().flatten() {
                    Some(h) => entry_key(&lines[h], j - h),
                    None => format!("before|{}", lines.get(*j).map_or("", String::as_str)),
                };
                if f.as_ref().is_some_and(|f| f.1) { format!("{at}-folded") } else { at }
            }
            Row::Fold { key, .. } => format!("fold|{key}"),
            Row::Note(t) => format!("note|{t}"),
        })
        .collect()
}

/// Where a log of requests is held for its fold presses: the window's panes and turns (`App`), or a window of its own
/// that holds one turn's requests (sent_window.rs). A press acts on the state of the window it is drawn in.
pub trait Holds: 'static {
    /// The log of agent NAME's pane (a turn's: [`turn_name`]) in project PI.
    fn log_mut(&mut self, pi: usize, name: &str) -> Option<&mut Log>;
}

impl Holds for App {
    fn log_mut(&mut self, pi: usize, name: &str) -> Option<&mut Log> {
        self.raw.logs.get_mut(&(pi, name.to_string()))
    }
}

/// Open fold KEY of agent NAME's pane, or fold it again: a repeat's fold (SHUT false, folded until opened) or an
/// entry's or a request's (SHUT true, open until folded).
fn toggle<S: Holds>(s: &mut S, pi: usize, name: &str, key: &str, shut: bool) {
    if let Some(log) = s.log_mut(pi, name) {
        let set = if shut { &mut log.shut } else { &mut log.open };
        if !set.remove(key) {
            set.insert(key.to_string());
        }
        log.relayout();
    }
}

impl App {
    /// Open a raw pane on agent NAME of project PI beside the focused pane (the main area, when there is no split);
    /// focus moves to it. When the split is full, the focused pane shows it instead.
    pub fn open_raw(&mut self, pi: usize, name: &str) {
        self.open_pane_beside(pi, Sel::Raw(name.to_string()));
    }
}

/// The raw panes in view: the focused page and the split's others.
fn shown(s: &App) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    if let Sel::Raw(n) = &s.sel {
        out.push((s.cur, n.clone()));
    }
    // an Inspect drawer on screen reads its agent's requests as a pane does (inspect.rs: Diff, Tools, Messages, Raw JSON)
    for (pi, n) in crate::drawer::inspected(s) {
        if !out.iter().any(|(p, m)| *p == pi && *m == n) {
            out.push((pi, n));
        }
    }
    for (j, (pi, sel)) in s.panes.iter().enumerate() {
        if let (Sel::Raw(n), true) = (sel, j != s.focus) {
            if !out.iter().any(|(p, m)| p == pi && m == n) {
                out.push((*pi, n.clone()));
            }
        }
    }
    out
}

/// Take in what the reader read, and ask for what was added to each shown raw pane's session, at most once a second
/// each (coordd says where the log is, then the reader reads it); wakes the app again while one is shown. Whether a
/// request went out or a read came in.
pub fn poll(s: &mut App, tx: &mut rune::Tx) -> bool {
    if let Some((pi, name, n)) = s.raw.scene_read.take() {
        if n == 0 {
            scene_ask(s, pi, &name);
        } else {
            s.raw.scene_read = Some((pi, name, n - 1));
        }
    }
    let mut any = take_read(s);
    // a turn's requests being read under a reply: woken until they come
    if s.raw.logs.values().any(|l| l.until.is_some() && (l.newer_pending || l.older_pending)) {
        s.wake_every(EVERY, tx);
    }
    let shown = shown(s);
    for at in shown.iter().filter(|at| !s.raw.was_shown.contains(at)) {
        let log = s.raw.logs.entry(at.clone()).or_default();
        (log.refused, log.asked) = (None, None);
    }
    s.raw.was_shown = shown.clone();
    if shown.is_empty() {
        return any;
    }
    // one timer a tick: a `wake_after` on every poll leaves a timer for each frame drawn, and each of those draws
    // a frame a second later (the window then kept 60 frames a second while a raw pane was shown, 13 Nov)
    s.wake_every(EVERY, tx);
    for (pi, name) in shown {
        // a pane that cannot be filled here asks nothing (Windows, an older coordd: the one line)
        if !readable(s, pi) {
            continue;
        }
        let log = s.raw.logs.entry((pi, name.clone())).or_default();
        if log.newer_pending || log.asked.is_some_and(|t| t.elapsed() < EVERY) {
            continue;
        }
        let Some(p) = s.projects.get_mut(pi) else { continue };
        let (req, page) = ask_for(p.protocol, &name, log, false);
        let Some(link) = p.link.as_mut() else { continue };
        if let Some(id) = link.ask(&req) {
            p.pending.insert(id, Pending::Raw { agent: name, older: false, page });
            log.newer_pending = true;
            log.asked = Some(Instant::now());
            any = true;
        }
    }
    any
}

/// Ask for the lines before what agent NAME's raw pane holds.
fn read_older(s: &mut App, pi: usize, name: &str) {
    let Some(log) = s.raw.logs.get_mut(&(pi, name.to_string())) else { return };
    if log.older_pending || !log.loaded || log.start == 0 {
        return;
    }
    let Some(p) = s.projects.get_mut(pi) else { return };
    let (req, page) = ask_for(p.protocol, agent_of(name), log, true);
    let Some(link) = p.link.as_mut() else { return };
    if let Some(id) = link.ask(&req) {
        s.projects[pi].pending.insert(id, Pending::Raw { agent: name.into(), older: true, page });
        log.older_pending = true;
    }
}

/// The request for the lines of the log NAME (a pane's, or a turn's under a reply) of agent AGENT, OLDER or newer
/// than what LOG holds, of a coordd speaking PROTOCOL: the lines themselves (`RawPage`, read by coordd: true), or where
/// the log is (`RawSource`, read here: false).
fn ask_for(protocol: u32, agent: &str, log: &Log, older: bool) -> (Request, bool) {
    if protocol < coord_types::RAW_PAGE_PROTOCOL {
        return (Request::RawSource { name: agent_of(agent).into() }, false);
    }
    let (before, after) = if older { (Some(log.start), None) } else { (None, log.loaded.then_some(log.next)) };
    (Request::RawPage { name: agent_of(agent).into(), before, after, span: log.until }, true)
}

/// What a pane says while its first page is on its way.
pub(crate) const LOADING: &str = "Loading…";
/// What a pane says when the agent's session holds no turn yet.
const NO_TURNS: &str = "No turns recorded yet.";

/// Why a pane cannot show its log, WHY coordd's words.
pub(crate) fn cannot(why: &str) -> String {
    format!("This log cannot be shown: {why}")
}

/// coordd's refusal OUT as the pane says it: its words without the command it names.
fn said(out: &str) -> String {
    out.strip_prefix("coord raw: ").unwrap_or(out).to_string()
}

/// coordd's reply to a raw pane's request for agent AGENT of project PI: its lines (PAGE: `RawPage`) into the pane;
/// where the log is (`RawSource`), the read handed to the reader (the pane stays pending until it comes back); or why
/// not on the pane.
pub fn replied(s: &mut App, pi: usize, agent: String, older: bool, page: bool, ok: bool, out: String) {
    if page {
        let log = s.raw.logs.entry((pi, agent)).or_default();
        took(log, older, ok, &out);
        return;
    }
    let at = located(ok, &out, &std::env::var("HOME").unwrap_or_default());
    let Raws { logs, reader: r, .. } = &mut s.raw;
    let log = logs.entry((pi, agent.clone())).or_default();
    let why = match at {
        Ok((dir, session)) => {
            let (before, after) = if older { (Some(log.start), None) } else { (None, log.loaded.then_some(log.next)) };
            match reader(r).jobs.send(Job { pi, agent, older, dir, session, before, after, span: log.until }) {
                Ok(()) => return,
                Err(_) => "the raw pane's reader stopped".to_string(),
            }
        }
        Err(why) => why,
    };
    if older { log.older_pending = false } else { log.newer_pending = false }
    log.refused = Some(why);
}

/// The pipe to project PI's coordd ended before its reply to a read of log NAME (a pane's agent, or a turn's name):
/// that read is no longer on its way, so the log is asked again once the pipe is back, at a pane's next poll or a call
/// near on the Map (`open_turn`), where its pending mark had stopped every ask after (consult-3 N-1479 p42).
pub(crate) fn lost(r: &mut Raws, pi: usize, name: &str, older: bool) {
    if let Some(log) = r.logs.get_mut(&(pi, name.to_string())) {
        if older { log.older_pending = false } else { log.newer_pending = false }
    }
}

/// coordd's `RawPage` reply OUT (OK or refused) into LOG: its lines, or why not.
fn took(log: &mut Log, older: bool, ok: bool, out: &str) {
    if older { log.older_pending = false } else { log.newer_pending = false }
    match serde_json::from_str::<RawPage>(out) {
        Ok(page) if ok => {
            log.refused = None;
            merge(log, page, older);
        }
        _ => log.refused = Some(said(out)),
    }
}

/// PAGE into LOG: a new session starts it again; newer lines go at the bottom, older at the top. Its rows follow.
fn merge(log: &mut Log, page: RawPage, older: bool) {
    merge_lines(log, page, older);
    if log.until.is_some() {
        for h in log.lines.iter().filter(|l| l.starts_with("══ ")) {
            if log.seen.insert(h.clone()) {
                log.shut.insert(h.clone());
            }
        }
    }
    log.relayout();
}

fn merge_lines(log: &mut Log, page: RawPage, older: bool) {
    if log.loaded && page.session != log.session {
        if older {
            return;
        }
        *log = Log { asked: log.asked, open: std::mem::take(&mut log.open), shut: std::mem::take(&mut log.shut), ..Default::default() };
    }
    log.session = page.session;
    log.about = page.about;
    if older {
        // nothing before what is held: the session's first request is held
        if page.lines.is_empty() {
            log.start = 0;
            return;
        }
        if page.next == log.start {
            let mut lines = page.lines;
            lines.append(&mut log.lines);
            log.lines = lines;
            let mut times = page.heads;
            times.append(&mut log.times);
            log.times = times;
            log.start = page.start;
        }
        return;
    }
    if !log.loaded {
        log.lines = page.lines;
        log.times = page.heads;
        log.start = page.start;
        log.loaded = true;
    } else if page.start == log.next {
        log.lines.extend(page.lines);
        log.times.extend(page.heads);
    } else {
        // the log was cut or replaced: start from what it holds now
        log.lines = page.lines;
        log.times = page.heads;
        log.start = page.start;
    }
    log.next = page.next;
    cut(log);
}

/// Past KEEP lines, whole requests go from the front (the last one stays, however long), and the first kept
/// request's time is where Earlier reads back from.
fn cut(log: &mut Log) {
    if log.lines.len() <= KEEP {
        return;
    }
    let heads: Vec<usize> = log.lines.iter().enumerate().filter(|(_, l)| l.starts_with("══ ")).map(|(i, _)| i).collect();
    // a session file's lines (no heads, each line's offset its time): the oldest lines go, Earlier reads back from the first kept
    if heads.is_empty() && log.times.len() == log.lines.len() {
        let n = log.lines.len() - KEEP;
        log.lines.drain(..n);
        log.times.drain(..n);
        log.start = log.times[0];
        return;
    }
    let Some(k) = heads.iter().position(|&h| log.lines.len() - h <= KEEP).or(heads.len().checked_sub(1)) else { return };
    if heads[k] == 0 {
        return;
    }
    log.lines.drain(..heads[k]);
    log.times.drain(..k.min(log.times.len()));
    if let Some(&t) = log.times.first() {
        log.start = t;
    }
}

/// A text for showing: indented when it is JSON, else as it is.
fn shown_line(l: &str) -> Cow<'_, str> {
    let t = l.trim_start();
    if !(t.starts_with('{') || t.starts_with('[')) {
        return Cow::Borrowed(l);
    }
    match serde_json::from_str::<serde_json::Value>(l) {
        Ok(v) => Cow::Owned(serde_json::to_string_pretty(&v).unwrap_or_else(|_| l.to_string())),
        Err(_) => Cow::Borrowed(l),
    }
}

/// What one of proxylog.rs's lines is.
#[derive(Debug, PartialEq)]
enum Entry<'a> {
    /// `══ request TIME · MODEL · status N ══`: what is between the rules.
    Head(&'a str),
    /// `[LABEL] BODY` (or `[LABEL]\nBODY`): its caption, whether the model said it, and its body.
    Said { caption: String, model: bool, body: &'a str },
    /// A line of neither shape (a cut line, an older log's).
    Plain(&'a str),
}

fn entry(l: &str) -> Entry<'_> {
    if let Some(h) = l.strip_prefix("══ ").and_then(|h| h.strip_suffix(" ══")) {
        return Entry::Head(h.strip_prefix("request ").unwrap_or(h));
    }
    let Some((label, body)) = l.strip_prefix('[').and_then(|r| r.split_once(']')) else { return Entry::Plain(l) };
    let body = body.strip_prefix('\n').or_else(|| body.strip_prefix(' ')).unwrap_or(body);
    let words: Vec<&str> = label.split(' ').collect();
    fn kind(k: &str) -> &str {
        match k {
            "tool_use" => "tool call",
            "tool_result" => "tool result",
            k => k,
        }
    }
    let (caption, model) = match words.as_slice() {
        // a message of the history: `[3 assistant tool_use]`, `[0 user text]`, `[2 user]`
        [n, role, rest @ ..] if n.parse::<u32>().is_ok() => {
            let what = rest.iter().map(|k| kind(k)).collect::<Vec<_>>().join(" ");
            (format!("{role}{}{what} · message {n}", if what.is_empty() { "" } else { " " }), *role == "assistant")
        }
        ["reply", rest @ ..] => (format!("reply {}", rest.iter().map(|k| kind(k)).collect::<Vec<_>>().join(" ")).trim_end().to_string(), true),
        _ => (label.to_string(), false),
    };
    Entry::Said { caption, model, body }
}

/// The number of messages the request whose head is line I of LINES carries (its `[messages] N` line).
fn messages_of(lines: &[String], i: usize) -> Option<&str> {
    lines[i + 1..].iter().take_while(|l| !l.starts_with("══ ")).find_map(|l| l.strip_prefix("[messages] "))
}

/// A click on EL folds or opens KEY of agent NAME's pane (an entry or a request).
fn folds<S: Holds>(el: El, pi: usize, name: &str, key: &str, folded: bool, k: &Kit) -> El {
    let (who, key) = (name.to_string(), key.to_string());
    k.interactive(el).help(if folded { "Open it" } else { "Fold it" }).on_press(move |s: &mut S| toggle(s, pi, &who, &key, true))
}

/// Line I of LINES as a row of the pane; FOLD: its key and whether it is folded, when it folds.
fn row_of(lines: &[String], i: usize, fold: Option<&(String, bool)>, pi: usize, name: &str, k: &Kit) -> El {
    row_in::<App>(lines, i, fold, pi, name, None, k)
}

/// A request's head `2026-11-16 21:40:01.000 · …` under a reply made on DAY (`2026-11-16`): its clock to the second,
/// its date only when another day (paper-research N-101.p1434).
fn short_head(h: &str, day: &str) -> String {
    let Some((when, rest)) = h.split_once(" · ") else { return h.to_string() };
    let Some((date, clock)) = when.split_once(' ') else { return h.to_string() };
    let clock = clock.split('.').next().unwrap_or(clock);
    if date == day { format!("{clock} · {rest}") } else { format!("{date} {clock} · {rest}") }
}

/// Line I as [`row_of`] draws it, its folds pressed on S's log; DAY: under a reply made that day (a turn's requests),
/// its heads shortened.
#[allow(clippy::too_many_arguments)]
fn row_in<S: Holds>(lines: &[String], i: usize, fold: Option<&(String, bool)>, pi: usize, name: &str, day: Option<&str>, k: &Kit) -> El {
    let l = &lines[i];
    let mark = |folded: bool| if folded { "▸ " } else { "▾ " };
    match entry(l) {
        Entry::Head(h) => {
            let short = day.map(|d| short_head(h, d));
            let h = short.as_deref().unwrap_or(h);
            let n = messages_of(lines, i).map(|n| format!(" · {n} message{}", if n == "1" { "" } else { "s" })).unwrap_or_default();
            let head = match fold {
                Some((key, folded)) => folds::<S>(row().child(text(format!("{}Request · {h}{n}", mark(*folded))).font(k.ty.small_medium).color(k.c.text_base).wrap_anywhere()), pi, name, key, *folded, k),
                None => text(format!("Request · {h}{n}")).font(k.ty.small_medium).color(k.c.text_base).wrap_anywhere().selectable(true),
            };
            col().w_full().px(16.0).pt(if i == 0 { 8.0 } else { 18.0 }).pb(6.0).gap(6.0).child(rune_kit::Divider::horizontal().build(k)).child(head)
        }
        Entry::Said { caption, model, body } => {
            let folded = fold.is_some_and(|f| f.1);
            let t = if folded { String::new() } else { shown_line(body).into_owned() };
            let caption = match fold {
                Some((key, folded)) => folds::<S>(row().child(text(format!("{}{caption}", mark(*folded))).font(k.ty.small).color(k.c.text_faint)), pi, name, key, *folded, k),
                None => text(caption).font(k.ty.small).color(k.c.text_faint),
            };
            let bubble = col()
                .w_frac(0.86)
                .px(12.0)
                .py(if folded { 4.0 } else { 8.0 })
                .gap(4.0)
                .radius(k.radius.md)
                .fill(if model { k.c.layer_03 } else { k.c.layer_02 })
                .child(caption)
                .children((!t.is_empty()).then(|| text(t).font(k.ty.mono).color(k.c.text_base).wrap_anywhere().selectable(true)));
            let r = row().w_full().px(16.0).py(3.0).child(bubble);
            if model { r.justify_end() } else { r }
        }
        Entry::Plain(t) => row().w_full().px(16.0).py(3.0).child(text(shown_line(t).into_owned()).font(k.ty.mono).color(k.c.text_muted).wrap_anywhere().selectable(true)),
    }
}

/// A fold: its words and a click to open it (or fold it again) in S's log, on the side of what was sent.
fn fold_row<S: Holds>(pi: usize, name: &str, key: &str, label: &str, open: bool, k: &Kit) -> El {
    let (who, key) = (name.to_string(), key.to_string());
    let words = if open { format!("▾ {label} (click to fold)") } else { format!("▸ {label}") };
    row().w_full().px(16.0).py(3.0).child(
        k.interactive(row().px(12.0).py(6.0).radius(k.radius.md).border(1.0, k.c.border_weak).child(text(words).font(k.ty.small).color(k.c.text_muted)))
            .help(if open { "Fold it again" } else { "Show what it repeats" })
            .on_press(move |s: &mut S| toggle(s, pi, &who, &key, false)),
    )
}

/// Whether project PI's raw panes can be filled here: on Windows the log is the WSL machine's, and only a coordd that
/// reads it (`RawPage`) fills them; elsewhere the pane reads it itself from an older one.
fn readable(s: &App, pi: usize) -> bool {
    fills(s.projects.get(pi).map(|p| p.protocol))
}

/// [`readable`] for a project whose coordd speaks PROTOCOL (none: no such project).
fn fills(protocol: Option<u32>) -> bool {
    !crate::elsewhere::away() || protocol.is_some_and(|p| p >= coord_types::RAW_PAGE_PROTOCOL)
}

/// The heading over what the model was sent, the left side of a pane.
const SENT: &str = "Sent to the model";
/// The heading over what it answered, the right side.
const SAID: &str = "What it said back";

/// The page: what agent NAME of project PI's model was sent and answered, as the proxy logged it.
pub fn page(s: &App, pi: usize, name: &str, k: &Kit) -> El {
    if !readable(s, pi) {
        return crate::elsewhere::page(k);
    }
    let key = (pi, name.to_string());
    let log = s.raw.logs.get(&key);
    // the two sides headed over their bubbles, what was sent left and the answer right, instead of a sentence about
    // them (his walk, item 1: docs/archive/walk.md)
    let side = |t: &str| text(t.to_string()).font(k.ty.small_medium).color(k.c.text_base);
    let mut bar = row().w_full().px(16.0).pt(10.0).pb(6.0).gap(12.0).center_y().child(side(SENT).key("raw-side-sent"));
    if log.is_some_and(|l| l.loaded && l.start > 0) {
        let who = name.to_string();
        bar = bar.child(Button::new(if log.is_some_and(|l| l.older_pending) { "Reading…" } else { "Earlier" }).variant(ButtonVariant::Ghost).on_press(move |s: &mut App| read_older(s, pi, &who)).build(k));
    }
    let bar = bar.child(row().grow(1.0)).child(side(SAID).key("raw-side-said"));
    let about = match log {
        Some(Log { refused: Some(why), .. }) => cannot(why),
        Some(l) if l.loaded && l.lines.is_empty() => String::new(),
        Some(l) if l.loaded => format!("{} lines held · {}", l.lines.len(), l.session),
        _ => LOADING.to_string(),
    };
    // no gateway's log: what the lines are read from instead and what that leaves out (fix 5), on a hover over the
    // count, not as a paragraph over the pane (his walk, item 1)
    let held = row().w_full().px(16.0).pb(6.0).child(text(about).font(k.ty.small).color(k.c.text_faint).truncate());
    let held = match log.and_then(|l| l.about.clone()) {
        Some(source) => held.help(source),
        None => held,
    };
    let head = col()
        .w_full()
        .child(bar)
        .child(held)
        .child(rune_kit::Divider::horizontal().build(k));
    let n = log.map_or(0, |l| l.rows.len());
    let (who, kit) = (name.to_string(), k.clone());
    let who2 = who.clone();
    let rows = col()
        .key(format!("raw-rows-{pi}-{name}"))
        .w_full()
        .growth(GrowthSpec::default())
        .virtual_rows(
            n,
            40.0,
            move |s: &App, i| Cow::Owned(s.raw.logs.get(&(pi, who2.clone())).and_then(|l| l.keys.get(i).cloned()).unwrap_or_else(|| format!("r{i}"))),
            move |s: &App, i| {
                let Some(log) = s.raw.logs.get(&(pi, who.clone())) else { return col() };
                let r = match log.rows.get(i) {
                    Some(Row::Line(j, f)) if *j < log.lines.len() => row_of(&log.lines, *j, f.as_ref(), pi, &who, &kit),
                    Some(Row::Fold { key, label, open }) => fold_row::<App>(pi, &who, key, label, *open, &kit),
                    Some(Row::Note(t)) => row().w_full().px(16.0).py(3.0).child(text(t.clone()).font(kit.ty.small).color(kit.c.text_faint).wrap()),
                    _ => col(),
                };
                // a log, not a timeline: a row is there or not, it does not fade in (N-40 post 5)
                r.transition(rune::Transition::none())
            },
        );
    // nothing in this pane animates (N-40 post 5; `instant_changes` on the pane, below): the rows are a growth stack
    // because virtual rows must be one, and its spring and speed limits made each measured row (a [params] line runs
    // to 58,000 characters, estimated at 40 pt) grow for seconds while the pinned end moved with it; now every height,
    // the head's and the follow of the end land in the frame they are known
    let body = col()
        .key(format!("raw-{pi}-{name}"))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: !(rune::is_headless() && std::env::var_os("YAAIHH_RAW_TOP").is_some()) })
        .selection_group(true)
        .child(rows);
    // nothing held: no turns yet, the same words for a fresh agent whatever its log; a proxy's log adds why on a hover
    if log.is_some_and(|l| l.loaded && l.refused.is_none() && l.lines.is_empty()) {
        let line = row().w_full().px(16.0).py(10.0).key("raw-no-turns").child(text(NO_TURNS).font(k.ty.small).color(k.c.text_muted).wrap());
        let line = match log.is_some_and(|l| l.about.is_none() && !l.session.is_empty()) {
            true => line.help("Nothing recent. Its next turn shows here."),
            false => line,
        };
        return col().w_full().h_full().instant_changes().child(head).child(line);
    }
    col().w_full().h_full().instant_changes().child(head).child(body)
}

/// Why nothing of LOG, agent's requests, can be shown yet: loading, refused, or none held; none once one is held.
pub(crate) fn not_yet(log: Option<&Log>) -> Option<String> {
    match log {
        Some(Log { refused: Some(why), .. }) => Some(cannot(why)),
        Some(l) if !l.loaded => Some(LOADING.into()),
        Some(l) if l.lines.is_empty() => Some(NO_TURNS.into()),
        Some(l) if newest_head(l).is_none() => Some("Its next turn shows here.".into()),
        None => Some(LOADING.into()),
        Some(_) => None,
    }
}

/// The line the newest request held in LOG begins at (its head); none before one is held.
fn newest_head(log: &Log) -> Option<usize> {
    log.lines.iter().rposition(|l| l.starts_with("══ "))
}

/// The lines of the newest request held in LOG, after its head.
fn newest_lines(log: &Log) -> impl Iterator<Item = (usize, &str)> {
    let h = newest_head(log);
    log.lines.iter().enumerate().skip(h.map_or(log.lines.len(), |h| h + 1)).map(|(i, l)| (i, l.as_str()))
}

/// What the newest request held in LOG sends that the request before did not, in characters: the lines the pane does
/// not fold as repeated ([`layout`]), less its reply and a line the same as one of the request before's (its params);
/// and of all it sends. Whether the request before is held, else all of it is new to the pane.
pub(crate) fn new_chars(log: &Log) -> Option<(usize, usize, bool)> {
    let h = newest_head(log)?;
    let from = log.lines[..h].iter().rposition(|l| l.starts_with("══ "));
    let slice = &log.lines[from.unwrap_or(h)..];
    let at = h - from.unwrap_or(h);
    let before: HashSet<&str> = slice[..at].iter().map(String::as_str).collect();
    let sent = |l: &str| !l.starts_with("[reply");
    let rows = layout(slice, false, &HashSet::new(), &HashSet::new());
    let new = rows.iter().filter_map(|r| match r {
        Row::Line(i, _) if *i > at && sent(&slice[*i]) && !before.contains(slice[*i].as_str()) => Some(slice[*i].chars().count()),
        _ => None,
    });
    let all: usize = slice[at + 1..].iter().filter(|l| sent(l)).map(|l| l.chars().count()).sum();
    Some((new.sum(), all, from.is_some()))
}

/// The newest request held of agent NAME's in project PI, as the raw pane lays it out: what it repeats of the request
/// before folded, each fold opened by a click (the Inspect drawer's Diff, inspect.rs).
pub(crate) fn newest(s: &App, pi: usize, name: &str, k: &Kit) -> El {
    let log = s.raw.logs.get(&(pi, name.to_string()));
    let Some(log) = log.filter(|l| not_yet(Some(l)).is_none()) else { return note_row(&not_yet(log).unwrap_or_default(), k) };
    let h = newest_head(log).unwrap_or_default();
    let from = log.rows.iter().rposition(|r| matches!(r, Row::Line(j, _) if *j == h)).unwrap_or(log.rows.len());
    col().w_full().key(format!("inspect-newest-{pi}-{name}")).children(log.rows[from..].iter().map(|r| match r {
        Row::Line(j, f) if *j < log.lines.len() => row_of(&log.lines, *j, f.as_ref(), pi, name, k),
        Row::Fold { key, label, open } => fold_row::<App>(pi, name, key, label, *open, k),
        Row::Note(t) => note_row(t, k),
        _ => col(),
    }))
}

/// The messages of the newest request held of agent NAME's in project PI, each whole (the Inspect drawer's Messages).
pub(crate) fn newest_messages(s: &App, pi: usize, name: &str, k: &Kit) -> El {
    let log = s.raw.logs.get(&(pi, name.to_string()));
    let Some(log) = log.filter(|l| not_yet(Some(l)).is_none()) else { return note_row(&not_yet(log).unwrap_or_default(), k) };
    let rows: Vec<El> = newest_lines(log).filter(|(_, l)| matches!(part(l), Part::Message(_))).map(|(i, _)| row_of(&log.lines, i, None, pi, name, k)).collect();
    match rows.is_empty() {
        true => note_row("Its newest request carries no messages.", k),
        false => col().w_full().key(format!("inspect-messages-{pi}-{name}")).children(rows),
    }
}

/// The tools the newest request held of agent NAME's in project PI carried: each its name and its description's first
/// line (`[tool NAME] {json}`); none before a request is held.
pub(crate) fn newest_tools(s: &App, pi: usize, name: &str) -> Option<Vec<(String, String)>> {
    let log = s.raw.logs.get(&(pi, name.to_string())).filter(|l| not_yet(Some(l)).is_none())?;
    Some(
        newest_lines(log)
            .filter_map(|(_, l)| l.strip_prefix("[tool "))
            .filter_map(|r| r.split_once(']'))
            .map(|(n, body)| {
                let about = serde_json::from_str::<serde_json::Value>(body.trim()).ok().and_then(|v| v.get("description").and_then(|d| d.as_str()).map(|d| d.lines().next().unwrap_or("").to_string()));
                (n.to_string(), about.unwrap_or_default())
            })
            .collect(),
    )
}

/// When the newest request held of agent NAME's in project PI was made, and just after the last held: what a window of
/// its own reads (sent_window.rs, the Inspect drawer's Pop out); none before one is held.
pub(crate) fn newest_span(s: &App, pi: usize, name: &str) -> Option<(u64, u64)> {
    let log = s.raw.logs.get(&(pi, name.to_string()))?;
    newest_head(log)?;
    Some((*log.times.last()?, log.next))
}

fn note_row(t: &str, k: &Kit) -> El {
    row().w_full().px(16.0).py(6.0).child(text(t.to_string()).font(k.ty.small).color(k.c.text_faint).wrap())
}

/// The name a turn's requests are held under among the panes' logs: its agent and its reply, so that its folds and
/// reads go as a pane's do.
pub(crate) fn turn_name(agent: &str, post: &str) -> String {
    format!("{agent}\u{1f}{post}")
}

/// Whether the requests of agent AGENT's turn behind its post POST, in project PI, are read and held (map/deep.rs
/// says, while the pipe is down, why a call holds none).
pub(crate) fn turn_held(s: &App, pi: usize, agent: &str, post: &str) -> bool {
    s.raw.logs.get(&(pi, turn_name(agent, post))).is_some_and(|l| l.loaded)
}

/// The agent a log's NAME is of (a pane's name is the agent's own).
fn agent_of(name: &str) -> &str {
    name.split('\u{1f}').next().unwrap_or(name)
}

/// Read the requests of agent AGENT's turn behind its post POST, which ended before UNTIL (turn.rs), once: through the
/// link, or in a scene at once.
pub fn open_turn(s: &mut App, pi: usize, agent: &str, post: &str, since: u64, until: u64) {
    let name = turn_name(agent, post);
    let log = s.raw.logs.entry((pi, name.clone())).or_default();
    if log.loaded || log.newer_pending {
        return;
    }
    log.until = Some((since, until));
    log.refused = None;
    let Some(p) = s.projects.get_mut(pi) else { return };
    let (req, page) = ask_for(p.protocol, agent, log, false);
    if let Some(link) = p.link.as_mut() {
        if let Some(id) = link.ask(&req) {
            p.pending.insert(id, Pending::Raw { agent: name, older: false, page });
            log.newer_pending = true;
        }
        return;
    }
    // a window's projects each have a link (lib.rs `setup`, projects.rs `new_project_done`): one with none is not
    // connected, said so, and nothing runs the bridge on the window's thread to wait for it (consult-3 N-1446 p386, on
    // docket-edge8's p345: Wire puts this one press away)
    if !rune::is_headless() {
        log.refused = Some(format!("{} is not connected", p.name));
        return;
    }
    // a scene has no link: asked once, read by coordd or here
    let out = p.ask_once(&req);
    if page {
        let ok = out.is_ok();
        return took(log, false, ok, &out.unwrap_or_else(|e| e));
    }
    match out.and_then(|out| located(true, &out, &std::env::var("HOME").unwrap_or_default())) {
        Ok((dir, session)) => merge(log, proxylog::turn_page(&dir, &session, since, until), false),
        Err(why) => log.refused = Some(why),
    }
}

/// Agent AGENT's turn behind POST in project PI held as if read from the proxy's log, from SINCE to UNTIL: LINES, the
/// time of each head line in HEADS (a test's made-up requests).
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn hold_turn(s: &mut App, pi: usize, agent: &str, post: &str, since: u64, until: u64, lines: &[&str], heads: Vec<u64>) {
    s.raw.logs.entry((pi, turn_name(agent, post))).or_default().until = Some((since, until));
    hold(s, pi, &turn_name(agent, post), lines, heads);
}

/// Agent NAME's requests in project PI held as if read from the proxy's log: LINES, the time of each head line in
/// HEADS (a test's made-up requests).
#[cfg(test)]
pub(crate) fn hold(s: &mut App, pi: usize, name: &str, lines: &[&str], heads: Vec<u64>) {
    let log = s.raw.logs.entry((pi, name.to_string())).or_default();
    merge(log, RawPage { lines: lines.iter().map(|l| l.to_string()).collect(), start: 0, next: lines.len() as u64, session: "s".into(), heads, about: None }, false);
}

/// The requests of agent AGENT's turn behind POST in project PI unfolded, as presses on their heads leave them (a
/// still's).
#[cfg(test)]
pub(crate) fn unfold_turn(s: &mut App, pi: usize, agent: &str, post: &str) {
    if let Some(log) = s.raw.logs.get_mut(&(pi, turn_name(agent, post))) {
        log.shut.clear();
        log.relayout();
    }
}

/// The requests of agent AGENT's turn behind POST since SINCE (the prompt's time), each folded to its head until
/// clicked, as the raw pane draws them (a lens's wire, lens.rs).
pub fn turn(s: &App, pi: usize, agent: &str, post: &str, since: u64, k: &Kit) -> El {
    let name = turn_name(agent, post);
    requests::<App>(turn_log(s, pi, agent, post), pi, &name, since, k)
}

/// What is held of agent AGENT's turn behind POST in project PI ([`open_turn`]); none until it is asked.
pub(crate) fn turn_log<'a>(s: &'a App, pi: usize, agent: &str, post: &str) -> Option<&'a Log> {
    s.raw.logs.get(&(pi, turn_name(agent, post)))
}

/// LOG, the requests of a turn held as NAME in project PI ([`turn_name`]), from SINCE: each folded to its head until
/// clicked, the press folding S's log, the state of the window it is drawn in (`App`, or a window of its own,
/// sent_window.rs); while it cannot be shown, why. None: not asked yet.
pub(crate) fn requests<S: Holds>(log: Option<&Log>, pi: usize, name: &str, since: u64, k: &Kit) -> El {
    let note = |t: &str| text(t.to_string()).font(k.ty.small).color(k.c.text_faint).wrap();
    let Some(log) = log else { return note(LOADING) };
    if let Some(why) = &log.refused {
        return note(&cannot(why));
    }
    if !log.loaded {
        return note(LOADING);
    }
    // each head line's time, in order (`Log::times`)
    let heads: Vec<usize> = log.lines.iter().enumerate().filter(|(_, l)| l.starts_with("══ ")).map(|(i, _)| i).collect();
    let time: HashMap<usize, u64> = heads.iter().copied().zip(log.times.iter().copied()).collect();
    // the reply's day, as the heads write it (local)
    let day = chrono::DateTime::from_timestamp_nanos(since as i64).with_timezone(&chrono::Local).format("%Y-%m-%d").to_string();
    let mut c = col().w_full().gap(0.0);
    let mut cur: Option<u64> = None;
    let mut n = 0;
    for r in &log.rows {
        if let Row::Line(j, _) = r
            && let Some(t) = time.get(j)
        {
            cur = Some(*t);
            if *t >= since {
                n += 1;
            }
        }
        if cur.is_none_or(|t| t < since) {
            continue;
        }
        let el = match r {
            Row::Line(j, f) if *j < log.lines.len() => row_in::<S>(&log.lines, *j, f.as_ref(), pi, name, Some(&day), k),
            Row::Fold { key, label, open } => fold_row::<S>(pi, name, key, label, *open, k),
            _ => continue,
        };
        c = c.child(el);
    }
    if n == 0 {
        return note("Not in the log anymore.");
    }
    c
}

/// For the `raw` scene with YAAIHH_RAW_LIVE: agent NAME's page asked of the pane's reader, as the app does once coordd
/// has said where the log is; it comes in when read, after the pane has opened.
pub fn scene_read(s: &mut App, pi: usize, name: &str, polls: u32) {
    s.raw.logs.entry((pi, name.to_string())).or_default().newer_pending = true;
    s.raw.scene_read = Some((pi, name.to_string(), polls));
}

fn scene_ask(s: &mut App, pi: usize, name: &str) {
    let Some(p) = s.projects.get(pi) else { return };
    let (req, page) = ask_for(p.protocol, name, &Log::default(), false);
    let out = p.ask_once(&req);
    let ok = out.is_ok();
    replied(s, pi, name.to_string(), false, page, ok, out.unwrap_or_else(|e| e));
}

/// For the `raw` scene: agent NAME's pane filled once, where coordd says the log is (YAAIHH_RAW_TOP: the pane at its
/// top, not following the end).
pub fn scene_fill(s: &mut App, pi: usize, name: &str) {
    let Some(p) = s.projects.get(pi) else { return };
    let (req, page) = ask_for(p.protocol, name, &Log::default(), false);
    let out = p.ask_once(&req);
    let log = s.raw.logs.entry((pi, name.to_string())).or_default();
    if page {
        let ok = out.is_ok();
        return took(log, false, ok, &out.unwrap_or_else(|e| e));
    }
    let at = out.and_then(|out| located(true, &out, &std::env::var("HOME").unwrap_or_default()));
    match at {
        Ok((dir, session)) => merge(log, proxylog::page(&dir, &session, None, None), false),
        Err(why) => log.refused = Some(why),
    }
}

#[cfg(test)]
mod open_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn page(lines: &[&str], start: u64, next: u64, session: &str) -> RawPage {
        RawPage { lines: lines.iter().map(|l| l.to_string()).collect(), start, next, session: session.into(), heads: Vec::new(), about: None }
    }

    #[test]
    fn a_turns_request_head_says_its_clock_and_the_date_only_when_another_day() {
        assert_eq!(short_head("2026-11-16 21:40:01.000 · claude-opus-5-5 · status 200", "2026-11-16"), "21:40:01 · claude-opus-5-5 · status 200");
        assert_eq!(short_head("2026-11-17 00:00:02.500 · m · status 200", "2026-11-16"), "2026-11-17 00:00:02 · m · status 200");
    }

    #[test]
    fn newer_lines_go_at_the_bottom_older_at_the_top_a_new_session_starts_again() {
        let mut log = Log::default();
        merge(&mut log, page(&["c", "d"], 10, 14, "s1"), false);
        merge(&mut log, page(&["e"], 14, 16, "s1"), false);
        assert_eq!(log.lines, ["c", "d", "e"]);
        merge(&mut log, page(&["a", "b"], 6, 10, "s1"), true);
        assert_eq!((log.lines.clone(), log.start, log.next), (vec!["a".to_string(), "b".into(), "c".into(), "d".into(), "e".into()], 6, 16));
        // an older page that does not end where the held lines start is not glued on
        merge(&mut log, page(&["x"], 0, 3, "s1"), true);
        assert_eq!(log.start, 6);
        merge(&mut log, page(&["new"], 0, 4, "s2"), false);
        assert_eq!((log.lines.clone(), log.session.as_str()), (vec!["new".to_string()], "s2"));
    }

    fn lines(ls: &[&str]) -> Vec<String> {
        ls.iter().map(|l| l.to_string()).collect()
    }

    /// Each row in words: a line as itself, a fold as `fold: WORDS`.
    fn shown(rows: &[Row], ls: &[String]) -> Vec<String> {
        rows.iter()
            .map(|r| match r {
                Row::Line(i, Some((_, true))) => format!("folded: {}", ls[*i].lines().next().unwrap_or("")),
                Row::Line(i, _) => ls[*i].clone(),
                Row::Fold { label, open, .. } => format!("fold{}: {label}", if *open { " open" } else { "" }),
                Row::Note(t) => format!("note: {t}"),
            })
            .collect()
    }

    const TWO: &[&str] = &[
        "══ request a ══", "[system]\nYou are", "[tools] 1", "[tool Bash] {}", "[messages] 1", "[0 user text]\nfix mean", "[params] {}", "[reply text]\nOn it",
        "══ request b ══", "[system]\nYou are", "[tools] 1", "[tool Bash] {}", "[messages] 3", "[0 user text]\nfix mean", "[1 assistant text]\nOn it", "[1 assistant tool_use]\n{}", "[2 user tool_result]\nok", "[params] {}", "[reply text]\nDone",
    ];

    #[test]
    fn what_a_request_repeats_of_the_one_before_folds() {
        let ls = lines(TWO);
        let rows = layout(&ls, false, &HashSet::new(), &HashSet::new());
        assert_eq!(
            shown(&rows, &ls),
            [
                // the first request shown whole: nothing before it
                "══ request a ══", "[system]\nYou are", "[tools] 1", "[tool Bash] {}", "[messages] 1", "[0 user text]\nfix mean", "[params] {}", "[reply text]\nOn it",
                "══ request b ══", "fold: system prompt, as before", "fold: 1 tool, as before", "[messages] 3", "fold: 1 message as in the request before", "[1 assistant text]\nOn it", "[1 assistant tool_use]\n{}", "[2 user tool_result]\nok", "[params] {}", "[reply text]\nDone",
            ]
        );
        // opened, a fold shows its lines under it
        let open: HashSet<String> = ["══ request b ══|messages".to_string()].into();
        let rows = layout(&ls, false, &open, &HashSet::new());
        let s = shown(&rows, &ls);
        let at = s.iter().position(|l| l == "fold open: 1 message as in the request before").unwrap();
        assert_eq!(s[at + 1], "[0 user text]\nfix mean");
    }

    #[test]
    fn an_entry_folds_to_its_caption_and_a_head_folds_its_whole_request() {
        let ls = lines(TWO);
        // request b's reply (line 18, ten lines below its head) folded; request a folded whole
        let shut: HashSet<String> = [entry_key(&ls[8], 10), ls[0].clone()].into();
        let s = shown(&layout(&ls, false, &HashSet::new(), &shut), &ls);
        assert_eq!(s[0], "folded: ══ request a ══");
        assert_eq!(s[1], "══ request b ══");
        assert_eq!(s.last().unwrap(), "folded: [reply text]");
        // request b still compared with request a, folded or not
        assert!(s.contains(&"fold: system prompt, as before".to_string()));
        // a key stays the line's when Earlier puts older lines above
        let mut older = lines(&["══ request z ══", "[0 user text]\nhi"]);
        older.extend(ls.clone());
        let s2 = shown(&layout(&older, false, &HashSet::new(), &shut), &older);
        assert!(s2.contains(&"folded: ══ request a ══".to_string()) && s2.last().unwrap() == "folded: [reply text]", "{s2:#?}");
        // the rows of a pane: open, each entry carries its key
        let rows = layout(&ls, false, &HashSet::new(), &HashSet::new());
        assert_eq!(rows[1], Row::Line(1, Some((entry_key(&ls[0], 1), false))));
    }

    #[test]
    fn a_history_that_differs_does_not_fold_from_there_on() {
        let mut ls = lines(TWO);
        // a compaction: the first message is a summary now, so nothing of the history folds
        ls[13] = "[0 user text]\nSummary of before".into();
        ls[10] = "[tools] 2".into();
        let s = shown(&layout(&ls, false, &HashSet::new(), &HashSet::new()), &ls);
        assert!(s.contains(&"fold: system prompt, as before".to_string()));
        assert!(!s.iter().any(|l| l.contains("message as in") || l.contains("tool, as before")), "{s:#?}");
        assert!(s.contains(&"[0 user text]\nSummary of before".to_string()) && s.contains(&"[tools] 2".to_string()));
    }

    #[test]
    fn a_system_part_that_changed_shows_the_rest_fold() {
        let mut ls = lines(TWO);
        ls.insert(1, "[system]\nheader cch=1".into());
        ls.insert(10, "[system]\nheader cch=2".into());
        let s = shown(&layout(&ls, false, &HashSet::new(), &HashSet::new()), &ls);
        let b = s.iter().position(|l| l == "══ request b ══").unwrap();
        assert_eq!(s[b + 1..b + 3], ["[system]\nheader cch=2".to_string(), "fold: 1 of 2 parts unchanged".into()]);
    }

    #[test]
    fn a_first_request_with_one_before_not_held_says_so() {
        let ls = lines(TWO);
        let s = shown(&layout(&ls, true, &HashSet::new(), &HashSet::new()), &ls);
        assert_eq!(s[1], "note: shown whole");
        assert_eq!(s[2], "[system]\nYou are");
    }

    /// A real session's page (PROXY_LOG=DIR PROXY_SESSION=ID), its rows in short: to look at its folds.
    #[test]
    #[ignore]
    fn a_real_page_folds() {
        let (Ok(dir), Ok(session)) = (std::env::var("PROXY_LOG"), std::env::var("PROXY_SESSION")) else { return };
        let p = proxylog::page(&dir, &session, None, None);
        for r in shown(&layout(&p.lines, p.start > 0, &HashSet::new(), &HashSet::new()), &p.lines) {
            println!("{}", r.lines().next().unwrap_or("").chars().take(90).collect::<String>());
        }
    }

    /// N-40: coordd says where the log is as the project file writes it; the app reads it on the host, from a folder
    /// coordd (in a box) cannot see: `~/` is the app's home. Anything but `~/` or absolute is not read; a refusal is
    /// the pane's words.
    #[test]
    fn a_projects_page_is_read_from_a_folder_its_coordd_cannot_see() {
        let home = std::env::temp_dir().join(format!("yaaihh-raw-home-{}", std::process::id()));
        let dir = home.join("plogs");
        std::fs::create_dir_all(&dir).unwrap();
        let log = "=== REQUEST INFO ===\nTimestamp: 2026-11-13T10:00:00+01:00\n\n=== HEADERS ===\nAuthorization: Bearer xxx\nX-Claude-Code-Session-Id: s-1\n\n=== REQUEST BODY ===\n{}\n\n=== API REQUEST 1 ===\nHeaders:\nX-Api-Key: secret\n\nBody:\n{\"model\":\"claude-haiku-5-5\",\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}]}\n\n=== API RESPONSE 1 ===\nStatus: 200\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n";
        std::fs::write(dir.join("v1-messages-2026-11-13T100000-1.log"), log).unwrap();
        let h = home.display().to_string();
        let out = serde_json::to_string(&RawSource { dir: "~/plogs".into(), session: "s-1".into() }).unwrap();
        let (d, session) = located(true, &out, &h).unwrap();
        assert_eq!(d, format!("{h}/plogs"));
        let p = proxylog::page(&d, &session, None, None);
        assert_eq!(p.lines.iter().filter(|l| l.starts_with("══ request")).count(), 1, "{:#?}", p.lines);
        let all = p.lines.join("\n");
        assert!(!all.contains("Bearer") && !all.contains("secret"), "{all}");
        // an absolute folder is itself; a relative one is not read
        let abs = serde_json::to_string(&RawSource { dir: d.clone(), session: "s-1".into() }).unwrap();
        assert_eq!(located(true, &abs, "/elsewhere").unwrap().0, d);
        let rel = serde_json::to_string(&RawSource { dir: "logs".into(), session: "s-1".into() }).unwrap();
        assert_eq!(located(true, &rel, &h), Err("proxy_log must be ~/ or absolute: the app reads it on the host".to_string()));
        // coordd's refusal is what the pane says, without the command it names
        assert_eq!(located(false, "coord raw: the project file names no proxy_log folder", &h), Err("the project file names no proxy_log folder".to_string()));
        std::fs::remove_dir_all(&home).ok();
    }

    /// N-40 post 5: past KEEP, whole requests go from the front, Earlier reads on from the first kept request's time,
    /// and the rows that stay keep their keys (rune keeps them as they were: nothing is built again).
    /// N-802: from `RAW_PAGE_PROTOCOL` on the pane asks coordd for its lines, on Windows and Linux alike, and takes them
    /// as it takes its own reads; an older coordd is asked where the log is (on Windows the pane is then the one line).
    #[test]
    fn from_raw_page_on_coordd_reads_the_lines_and_the_pane_takes_them() {
        let p = coord_types::RAW_PAGE_PROTOCOL;
        let mut log = Log::default();
        assert!(matches!(ask_for(p, "w2", &log, false), (Request::RawPage { ref name, before: None, after: None, span: None }, true) if name == "w2"));
        assert!(matches!(ask_for(p - 1, "w2", &log, false), (Request::RawSource { ref name }, false) if name == "w2"));
        // a turn's requests under a reply: its span, asked by its agent's name
        log.until = Some((5, 9));
        assert!(matches!(ask_for(p, &turn_name("w2", "N-1.p2"), &log, false), (Request::RawPage { ref name, span: Some((5, 9)), .. }, true) if name == "w2"));
        // the reply's lines into the pane; the next ask reads on after them, Earlier before them
        let mut log = Log { newer_pending: true, ..Default::default() };
        let got = RawPage { lines: vec!["══ request 2026-11-19 08:00:00.000 · m · status 200 ══".into(), "[0 user]\nhi".into()], start: 10, next: 11, session: "session s-1".into(), heads: vec![10], about: None };
        took(&mut log, false, true, &serde_json::to_string(&got).unwrap());
        assert!(log.loaded && !log.newer_pending && log.lines.len() == 2 && log.refused.is_none());
        assert!(matches!(ask_for(p, "w2", &log, false).0, Request::RawPage { before: None, after: Some(11), .. }));
        assert!(matches!(ask_for(p, "w2", &log, true).0, Request::RawPage { before: Some(10), after: None, .. }));
        // a refusal in its words, without the command it names
        took(&mut log, true, false, "coord raw: the proxy's log cannot be read where this project's server runs");
        assert_eq!(log.refused.as_deref(), Some("the proxy's log cannot be read where this project's server runs"));
        assert_eq!(log.lines.len(), 2, "what is held stays");
        // no gateway's log: the page says what its lines are read from (fix 5), and the pane keeps it while they are
        let words = "No gateway log in this project, so this is the agent's own session record of each request: system prompt, tools, messages, settings and the answer. It has no HTTP status and nothing a gateway adds.";
        let record = RawPage { about: Some(words.into()), ..got.clone() };
        let mut log = Log { newer_pending: true, ..Default::default() };
        took(&mut log, false, true, &serde_json::to_string(&record).unwrap());
        assert_eq!((log.about.as_deref(), log.lines.len()), (Some(words), 2));
        let on = RawPage { lines: Vec::new(), start: 11, next: 11, heads: Vec::new(), ..record };
        took(&mut log, false, true, &serde_json::to_string(&on).unwrap());
        assert_eq!(log.about.as_deref(), Some(words), "read on: still its record");
        took(&mut log, false, true, &serde_json::to_string(&RawPage { start: 11, ..got }).unwrap());
        assert_eq!(log.about, None, "a gateway's log since: said no more");
    }

    /// His walk, item 1 (docs/archive/walk.md): the pane heads its two sides, "Sent to the model" over the left and
    /// "What it said back" over the right, where the bubbles are, and says no sentence about them; a session record's
    /// words are on a hover over the count, not a paragraph over the pane.
    #[test]
    fn the_pane_heads_its_two_sides_and_says_no_sentence_about_them() {
        let (_held, mut s) = crate::home::tests::made();
        let words = "No gateway log in this project, so this is the agent's own session record of each request.";
        let lines = vec!["══ request 2026-11-25 08:00:00.000 · m · status 200 ══".to_string(), "[0 user]\nhi".into(), "[reply text]\nhello".into()];
        let got = RawPage { lines, start: 10, next: 11, session: "session s-1".into(), heads: vec![10], about: Some(words.into()) };
        let mut log = Log { newer_pending: true, ..Default::default() };
        took(&mut log, false, true, &serde_json::to_string(&got).unwrap());
        s.raw.logs.insert((0, "try-1".into()), log);
        s.sel = Sel::Raw("try-1".into());
        let mut rt = crate::view_cost::window(s);
        rt.invalidate();
        for _ in 0..3 {
            rt.frame(0.016);
        }
        let els = rt.elements();
        let el = |key: &str| els.iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no {key}"));
        let (sent, said, rows) = (el("raw-side-sent"), el("raw-side-said"), el("raw-rows-0-try-1"));
        assert_eq!((sent.text.as_deref(), said.text.as_deref()), (Some("Sent to the model"), Some("What it said back")));
        // the left heading where the sent bubbles start, the right one ending where the answers end
        let (left, right) = (rows.rect[0] + 16.0, rows.rect[0] + rows.rect[2] - 16.0);
        assert!((sent.rect[0] - left).abs() < 1.0, "sent at {:?}, rows {:?}", sent.rect, rows.rect);
        assert!((said.rect[0] + said.rect[2] - right).abs() < 1.0, "said at {:?}, rows {:?}", said.rect, rows.rect);
        let texts: Vec<&str> = els.iter().filter(|e| !e.exiting).filter_map(|e| e.text.as_deref()).collect();
        assert!(texts.iter().all(|t| !t.contains("on the left") && !t.contains("session record")), "no sentence about the pane: {texts:?}");
        assert!(texts.contains(&"hello") && texts.iter().any(|t| t.starts_with("4 lines held") || t.starts_with("3 lines held")), "{texts:?}");
    }

    /// On Windows the pane is the one line only while the project's coordd is older than `RawPage`; on Linux it is
    /// always drawn.
    #[test]
    fn on_windows_the_pane_is_drawn_once_coordd_reads_the_log() {
        let older = Some(coord_types::RAW_PAGE_PROTOCOL - 1);
        assert!(fills(older) && !crate::elsewhere::as_on_windows(|| fills(older)));
        assert!(crate::elsewhere::as_on_windows(|| fills(Some(coord_types::RAW_PAGE_PROTOCOL))));
    }

    #[test]
    fn the_cut_at_keep_drops_whole_requests_and_the_kept_rows_keep_their_keys() {
        let request = |t: u64, n: usize| -> Vec<String> {
            std::iter::once(format!("══ request {t} · m · status 200 ══")).chain((0..n).map(|k| format!("[reply text]\nline {k} of {t}"))).collect()
        };
        let mut log = Log::default();
        let first: Vec<String> = request(100, 1500).into_iter().chain(request(200, 1500)).collect();
        merge(&mut log, RawPage { lines: first, start: 100, next: 201, session: "s".into(), heads: vec![100, 200], about: None }, false);
        assert_eq!((log.lines.len(), log.start), (3002, 100));
        let of = |log: &Log, t: u64| -> Vec<String> { log.keys.iter().filter(|k| k.contains(&format!("request {t} "))).cloned().collect() };
        let kept_before = of(&log, 200);
        assert_eq!(kept_before.len(), 1501);
        merge(&mut log, RawPage { lines: request(300, 1500), start: 201, next: 301, session: "s".into(), heads: vec![300], about: None }, false);
        // the first request went whole; Earlier reads from the first kept one's time
        assert_eq!((log.lines.len(), log.start, log.times.clone()), (3002, 200, vec![200, 300]));
        assert!(log.lines[0].starts_with("══ request 200 "));
        assert_eq!(of(&log, 200), kept_before, "the kept rows' keys are as they were");
        assert!(of(&log, 100).is_empty());
        // keys are unique: no two rows are taken for one
        assert_eq!(log.keys.iter().collect::<HashSet<_>>().len(), log.keys.len());
        // Earlier puts the dropped request back above, its keys its own again
        merge(&mut log, RawPage { lines: request(100, 1500), start: 100, next: 200, session: "s".into(), heads: vec![100], about: None }, true);
        assert_eq!((log.start, log.times.clone()), (100, vec![100, 200, 300]));
        assert_eq!(of(&log, 200), kept_before);
    }

    #[test]
    fn json_is_shown_indented_other_text_as_it_is() {
        assert_eq!(shown_line("{\"a\":1}"), "{\n  \"a\": 1\n}");
        assert_eq!(shown_line("not json"), "not json");
    }

    #[test]
    fn each_line_is_a_head_or_an_entry_by_who_said_it() {
        assert_eq!(entry("══ request 2026-11-11 12:01:35.610 · claude-haiku-5-5 · status 200 ══"), Entry::Head("2026-11-11 12:01:35.610 · claude-haiku-5-5 · status 200"));
        let said = |l| match entry(l) {
            Entry::Said { caption, model, body } => (caption, model, body.to_string()),
            e => panic!("{e:?}"),
        };
        assert_eq!(said("[system]\nYou are"), ("system".into(), false, "You are".into()));
        assert_eq!(said("[tool Bash] {\"a\":1}"), ("tool Bash".into(), false, "{\"a\":1}".into()));
        assert_eq!(said("[0 user text]\nfix mean"), ("user text · message 0".into(), false, "fix mean".into()));
        assert_eq!(said("[4 user tool_result]\nok"), ("user tool result · message 4".into(), false, "ok".into()));
        assert_eq!(said("[3 assistant tool_use]\n{}"), ("assistant tool call · message 3".into(), true, "{}".into()));
        assert_eq!(said("[reply text]\nDone"), ("reply text".into(), true, "Done".into()));
        assert_eq!(said("[reply stop] end_turn"), ("reply stop".into(), true, "end_turn".into()));
        assert_eq!(said("[params] {}"), ("params".into(), false, "{}".into()));
        let lines: Vec<String> = ["══ request a ══", "[tools] 1", "[messages] 2", "══ request b ══"].iter().map(|s| s.to_string()).collect();
        assert_eq!((messages_of(&lines, 0), messages_of(&lines, 3)), (Some("2"), None));
    }
}
