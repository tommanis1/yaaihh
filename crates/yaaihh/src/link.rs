//! The pipe to one box's coordd: `coordc bridge` run in the box (`box.sh sh BOX coordc bridge`). Its stdout carries
//! the event stream, the replies and the output of the terminals it attached; requests go to its stdin. When the pipe ends (the box or coordd stopped), it is
//! started again after a pause, and the new stream opens with a snapshot.

use coord_types::{Event, Hello, PtyEvent, Reply, Request};
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The pause before starting a failed pipe again: from PAUSE_MIN, doubling to PAUSE_MAX while it fails at once.
const PAUSE_MIN: Duration = coord_types::PIPE_PAUSE_MIN;
const PAUSE_MAX: Duration = Duration::from_secs(60);
/// A pipe that ran this long was up, not failing at once: the next pause is short again.
const RAN: Duration = Duration::from_secs(30);
/// An answer slower than this is written to the app's log with its request's kind (paper-research, N-412.p92): it
/// tells whether what the owner calls slow is the wait on coordd and not the drawing.
const SLOW: Duration = Duration::from_millis(500);

/// The requests asked and not yet answered: id, when it was written and the request's kind.
type Sent = Arc<Mutex<HashMap<u64, (Instant, String)>>>;

/// What the pipe brought.
pub enum Incoming {
    Event(Event),
    Reply(u64, Reply),
    /// An attached terminal's screen, output, size or end.
    Pty(PtyEvent),
    /// The bridge's first line (protocol 1): the protocol it speaks, its release, and why it refused this app, if it
    /// did. A bridge from before protocol 1 sends none.
    Hello(Hello),
    /// The pipe ended; why, as far as its stderr says.
    Down(String),
    /// A line of the bridge this build cannot read (once a pipe): an engine newer than the window (N-447 post 19).
    /// What it carried is lost (why is in the log); the window keeps what it had and says to restart it.
    Unread,
}

/// The bridges this window runs (each `wsl.exe … coordc bridge`, or the box's), so a window leaving for a new release
/// ends them all (restart.rs; N-514: two old exes held four of them). Once ENDED, no pipe is started again.
static BRIDGES: Mutex<Vec<std::sync::Weak<Mutex<Child>>>> = Mutex::new(Vec::new());
static ENDED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// End every bridge this window started, and start none again. How many were running.
pub fn end_all() -> usize {
    ENDED.store(true, std::sync::atomic::Ordering::SeqCst);
    let mut ended = 0;
    for c in BRIDGES.lock().unwrap().drain(..).filter_map(|w| w.upgrade()) {
        let mut c = c.lock().unwrap();
        if c.try_wait().ok().flatten().is_none() && c.kill().is_ok() {
            ended += 1;
        }
    }
    ended
}

pub struct Link {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    rx: Receiver<Incoming>,
    next: u64,
    sent: Sent,
    lsp: LspPipe,
}

/// The ids of the language-server messages (`Request::Lsp`) start here, apart from the requests' (`Link::ask`), so a
/// reply is known for whose it is without a lock between them.
const LSP_IDS: u64 = 1 << 48;

/// What a language server's relay brings (`{"lsp":…}` lines, protocol 2): one of its messages, or the end of its
/// connection with why (also when the bridge refused a message, or the pipe ended).
#[derive(Debug, PartialEq)]
pub enum LspIn {
    Message(serde_json::Value),
    Closed(String),
}

/// The bridge's relay to a coord's shared language servers (`Request::Lsp`, docket-core2's form, N-412 p130): its
/// messages go straight to the one who opened the lang, from the pipe's reader, not through the window's loop, so a
/// request made from the loop (a hover) can wait for its answer. Cloned freely; all clones are the same relay.
#[derive(Clone, Default)]
pub struct LspPipe {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    routes: Arc<Mutex<HashMap<String, Sender<LspIn>>>>,
    /// The lang of each message not yet answered, by id.
    waiting: Arc<Mutex<HashMap<u64, String>>>,
    next: Arc<std::sync::atomic::AtomicU64>,
    /// Which pipe this is, counted up as the bridge is started again: a server's connection is the bridge's, so a
    /// session opened on an earlier pipe is gone.
    pipe: Arc<std::sync::atomic::AtomicU64>,
}

impl LspPipe {
    /// LANG's messages from now on (an earlier opener of LANG gets no more), and which pipe they come on.
    pub fn open(&self, lang: &str) -> (Receiver<LspIn>, u64) {
        let (tx, rx) = channel();
        self.routes.lock().unwrap().insert(lang.to_string(), tx);
        (rx, self.pipe())
    }

    /// The pipe now running (see `pipe`).
    pub fn pipe(&self) -> u64 {
        self.pipe.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// MESSAGE (one JSON-RPC message) to LANG's shared server. Its answer comes to the opener of LANG; a refusal too,
    /// as Closed.
    pub fn send(&self, lang: &str, message: &serde_json::Value) -> Result<(), String> {
        let mut guard = self.stdin.lock().unwrap();
        let w = guard.as_mut().ok_or("disconnected")?;
        let id = LSP_IDS + self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let line = json!({"id": id, "req": Request::Lsp { lang: lang.to_string(), message: message.clone() }});
        self.waiting.lock().unwrap().insert(id, lang.to_string());
        writeln!(w, "{line}").and_then(|_| w.flush()).map_err(|e| format!("disconnected: {e}"))
    }

    /// The reply to message ID (true when it was one of these): a refusal ends its lang's session.
    fn replied(&self, id: u64, r: &Reply) -> bool {
        if id < LSP_IDS {
            return false;
        }
        if let Some(lang) = self.waiting.lock().unwrap().remove(&id).filter(|_| !r.ok) {
            self.came(&lang, LspIn::Closed(r.out.clone()));
        }
        true
    }

    fn came(&self, lang: &str, m: LspIn) {
        let mut routes = self.routes.lock().unwrap();
        if routes.get(lang).is_some_and(|tx| tx.send(m).is_err()) {
            routes.remove(lang);
        }
    }

    /// The pipe ended: every session with it.
    fn ended(&self, why: &str) {
        self.pipe.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.waiting.lock().unwrap().clear();
        for (_, tx) in self.routes.lock().unwrap().drain() {
            let _ = tx.send(LspIn::Closed(why.to_string()));
        }
    }
}

impl Link {
    /// Keep CMD running in the background; WAKE is called whenever something arrives, PTY instead for a terminal's
    /// output, paced on its own (pace.rs): a key's echo is not held for the agents' batch (N-412: echoes waited up to
    /// 80 ms for it).
    pub fn start(cmd: Vec<String>, wake: rune::Waker, pty: rune::Waker) -> Link {
        let (tx, rx) = channel();
        let stdin = Arc::new(Mutex::new(None));
        let shared = stdin.clone();
        let lsp = LspPipe { stdin: stdin.clone(), ..LspPipe::default() };
        let relay = lsp.clone();
        let sent: Sent = Arc::default();
        let waiting = sent.clone();
        std::thread::spawn(move || {
            // the pause doubles while the pipe keeps failing at once, up to a minute, and is short again after a pipe
            // that ran: a box whose coordd is gone was retried every 3 s, each try an `sbx exec` opening dozens of
            // Secret Service sessions, which crashed gnome-keyring every hour or two (8 Nov, m95)
            let mut pause = PAUSE_MIN;
            // where the stream got to, for the next pipe to resume there (protocol 1)
            let mut at = Resume::default();
            loop {
                if ENDED.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                let began = std::time::Instant::now();
                let why = run(&at.cmd(&cmd), &shared, &tx, (&wake, &pty), &waiting, &relay, &mut at);
                *shared.lock().unwrap() = None;
                relay.ended(&why);
                // their answers will not come: a new pipe numbers on, and nothing is left to time
                waiting.lock().unwrap().clear();
                if tx.send(Incoming::Down(why)).is_err() {
                    return;
                }
                wake();
                pause = if began.elapsed() > RAN { PAUSE_MIN } else { (pause * 2).min(PAUSE_MAX) };
                std::thread::sleep(pause);
            }
        });
        Link { stdin, rx, next: 0, sent, lsp }
    }

    /// A link whose requests go into STDIN (a test's child process), with no bridge behind it.
    #[cfg(test)]
    pub fn to_child(stdin: ChildStdin) -> Link {
        let (_tx, rx) = channel();
        Link { stdin: Arc::new(Mutex::new(Some(stdin))), rx, next: 0, sent: Sent::default(), lsp: LspPipe::default() }
    }

    /// The relay to this coord's shared language servers (protocol 2).
    pub fn lsp(&self) -> LspPipe {
        self.lsp.clone()
    }

    /// A second hello on this bridge, naming the release this window runs: restart.rs `ready`, once it has taken over
    /// from an older window (I-1457). Nothing while the pipe is down: its next start says the first hello with it.
    pub fn hello(&self) {
        if let Some(w) = self.stdin.lock().unwrap().as_mut() {
            let _ = say_hello(w, release_named());
        }
    }

    /// Send REQ; its reply comes back with the returned id. None while the pipe is down.
    pub fn ask(&mut self, req: &Request) -> Option<u64> {
        self.next += 1;
        let mut guard = self.stdin.lock().unwrap();
        let w = guard.as_mut()?;
        let line = json!({"id": self.next, "req": req});
        let kind = kind_of(&line["req"]);
        writeln!(w, "{line}").and_then(|_| w.flush()).ok()?;
        self.sent.lock().unwrap().insert(self.next, (Instant::now(), kind));
        Some(self.next)
    }

    pub fn drain(&self) -> Vec<Incoming> {
        self.rx.try_iter().collect()
    }
}

/// Where the stream got to (protocol 1): the coordd run of its last snapshot and the number of the last event passed
/// on, and whether the bridge said protocol 1, which takes `--from`. A pipe started again resumes there, so a dropped
/// `wsl.exe` or a WSL restart costs the events missed, not a whole snapshot with every view's data in it.
#[derive(Default)]
struct Resume {
    run: Option<String>,
    seq: u64,
    speaks: bool,
}

impl Resume {
    /// CMD for the next pipe: with `--from RUN:SEQ` when it can resume, quoted for the remote shell when CMD goes by
    /// ssh (places.rs `more`).
    fn cmd(&self, cmd: &[String]) -> Vec<String> {
        let mut c = cmd.to_vec();
        if let (true, Some(run)) = (self.speaks, &self.run) {
            crate::places::more(&mut c, ["--from".to_string(), format!("{run}:{}", self.seq)]);
        }
        c
    }

    /// What I, passed on, says of where the stream is.
    fn saw(&mut self, i: &Incoming) {
        match i {
            Incoming::Hello(h) => self.speaks = h.protocol >= 1,
            Incoming::Event(e) => {
                if let coord_types::Body::Snapshot(s) = &e.body {
                    self.run = s.run.clone();
                }
                self.seq = e.seq;
            }
            _ => {}
        }
    }
}

/// One run of CMD: until its stdout ends.
fn run(cmd: &[String], stdin: &Mutex<Option<ChildStdin>>, tx: &Sender<Incoming>, (wake, pty_wake): (&rune::Waker, &rune::Waker), sent: &Sent, lsp: &LspPipe, at: &mut Resume) -> String {
    // a bridge that does not say protocol 1 this time is not given --from next time
    at.speaks = false;
    let child = crate::places::command(&cmd[0]).args(&cmd[1..]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return format!("{}: {e}", cmd[0]),
    };
    let (stdout, stderr) = (child.stdout.take().expect("piped"), child.stderr.take());
    // this app's hello first (protocol 1); a bridge from before it answers the line as a request it cannot read, with
    // no id, which nothing waits for
    if let Some(mut w) = child.stdin.take() {
        let _ = say_hello(&mut w, release_named());
        *stdin.lock().unwrap() = Some(w);
    }
    let child = Arc::new(Mutex::new(child));
    {
        let mut all = BRIDGES.lock().unwrap();
        all.retain(|w| w.strong_count() > 0);
        all.push(Arc::downgrade(&child));
    }
    if ENDED.load(std::sync::atomic::Ordering::SeqCst) {
        let _ = child.lock().unwrap().kill(); // started as the window left
    }
    let name = cmd.join(" ");
    crate::log::line(&format!("pipe up: {name}"));
    // what the pipe brings, logged once a minute: a stream that floods the window shows here
    // a line this build cannot read is said once a pipe, not dropped in silence
    let mut unread = false;
    let (mut minute, mut bytes, mut lines) = (std::time::Instant::now(), 0usize, 0usize);
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        (bytes, lines) = (bytes + line.len(), lines + 1);
        if minute.elapsed() >= Duration::from_secs(60) {
            crate::log::line(&format!("pipe {name}: {} KB in {lines} lines this minute", bytes / 1024));
            (minute, bytes, lines) = (std::time::Instant::now(), 0, 0);
        }
        let i = match parse(&line) {
            Some(Parsed::Line(i)) => i,
            // a language server's: to its opener, straight from here, not through the window's loop
            Some(Parsed::Lsp(l)) => {
                let lang = l.lang.clone();
                lsp.came(&lang, l.into_in());
                continue;
            }
            None => {
                if !unread && line.starts_with('{') {
                    unread = true;
                    let why = serde_json::from_str::<Line>(&line).err().map(|e| e.to_string()).unwrap_or_default();
                    crate::log::line(&format!("pipe {name}: a line this window cannot read ({} KB): {why}", line.len() / 1024));
                    if tx.send(Incoming::Unread).is_ok() {
                        wake();
                    }
                }
                continue;
            }
        };
        crate::keystamp::read(line.len(), if let Incoming::Pty(coord_types::PtyEvent::Output { name, .. }) = &i { Some(name.as_str()) } else { None });
        if let Incoming::Reply(id, r) = &i {
            if lsp.replied(*id, r) {
                continue;
            }
            if let Some(text) = answered(sent, *id, Instant::now()) {
                crate::log::line(&text);
            }
        }
        at.saw(&i);
        let pty = matches!(i, Incoming::Pty(_));
        if tx.send(i).is_err() {
            break;
        }
        if pty { pty_wake() } else { wake() }
    }
    *stdin.lock().unwrap() = None;
    let mut err = String::new();
    if let Some(mut e) = stderr {
        let _ = e.read_to_string(&mut err);
    }
    let status = child.lock().unwrap().wait().map(|s| s.to_string()).unwrap_or_default();
    let err = err.trim();
    crate::log::line(&format!("pipe down: {name} ({status}){}", if err.is_empty() { String::new() } else { format!(": {}", err.lines().last().unwrap_or(err)) }));
    if err.is_empty() { format!("disconnected ({status})") } else { err.lines().last().unwrap_or(err).to_string() }
}

/// A request's kind: its `op` (coord-types tags each request with it).
fn kind_of(req: &serde_json::Value) -> String {
    req["op"].as_str().unwrap_or("?").to_string()
}

/// Request ID answered AT: no longer waiting; the log's line when it took longer than SLOW.
fn answered(sent: &Sent, id: u64, at: Instant) -> Option<String> {
    let (asked, kind) = sent.lock().unwrap().remove(&id)?;
    let took = at.saturating_duration_since(asked);
    (took > SLOW).then(|| format!("slow answer: {kind} took {} ms", took.as_millis()))
}

/// Whether this window's hellos name its release (I-1457; proto-lead N-1446 p509, docket-edge7 p511). Not while it
/// takes over from an older window, until it has (restart.rs `take_args`, `ready`), and not once it has said "go" to a
/// newer one, unless that swap did not happen (restart.rs `follow`, `fail`). So a hello names only the release on his
/// screen, and the old window's bridges, a pipe started again in the gap among them, cannot name it back.
static NAMES_RELEASE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// From now on this window's hellos name its release (ON), or none.
pub fn names_release(on: bool) {
    NAMES_RELEASE.store(on, std::sync::atomic::Ordering::SeqCst);
}

/// When this window became his, by its own clock, in UTC as the record writes a time (`Hello::since`): its start
/// (lib.rs, before any pipe), or for a window that took over from an older one, the "go" it was shown at (restart.rs
/// `ready`). A hello naming its release says it, so coordd's `window_release` line has the time the build was in his
/// window and not the time coordd heard of it, which a build with an engine made 3 to 8 minutes late on 21 Nov: coordd
/// started again as the window swapped, and the line waited for the next bridge's hello after its start
/// (paper-research N-1446 p1392; proto-window p1410).
static SINCE: Mutex<Option<String>> = Mutex::new(None);

/// This window is his from NOW on ([`SINCE`]).
pub fn his_since(now: chrono::DateTime<chrono::Utc>) {
    *SINCE.lock().unwrap() = Some(now.format("%Y-%m-%dT%H:%M:%SZ").to_string());
}

/// The release this window's hello names: its build's (`restart::ours`) while [`NAMES_RELEASE`] says so, and never a
/// still's (`KEEP`, set where a still starts: `--native --frames N --out DIR` reaches the live pipe, and a still from a
/// release build would write his window's line; consult-3 N-1446 p603, p606).
fn release_named() -> Option<String> {
    named(KEEP.load(std::sync::atomic::Ordering::Relaxed), NAMES_RELEASE.load(std::sync::atomic::Ordering::SeqCst), crate::restart::ours())
}

/// What a hello names, the build being OURS: none in a STILL, or while naming is not ON; read apart from the statics
/// so a test can give a build a name (`ours` is fixed at compile time, and none in a test build).
fn named(still: bool, on: bool, ours: Option<&str>) -> Option<String> {
    (!still && on).then_some(ours).flatten().map(String::from)
}

/// This app's hello naming RELEASE, as the first line to a bridge: the window's pipe names its release
/// ([`release_named`]); a scene's pipes (`once`, kept) name none, since a scene is not his window, and coordc hands a
/// named one to coordd as the release his window runs.
fn say_hello(w: &mut impl Write, release: Option<String>) -> std::io::Result<()> {
    writeln!(w, "{}", json!({"hello": hello(release, SINCE.lock().unwrap().clone())}))?;
    w.flush()
}

/// The hello naming RELEASE, with SINCE, when this window became his ([`SINCE`]), only beside a release: a hello that
/// names none is no window's of his, and coordc asks nothing for it.
fn hello(release: Option<String>, since: Option<String>) -> Hello {
    Hello { since: release.as_ref().and(since), ..Hello::ours(release) }
}

/// A line of the bridge: an event, a terminal's event, the reply to request ID, its hello, or a language server's.
#[derive(serde::Deserialize)]
struct Line {
    hello: Option<Hello>,
    event: Option<Event>,
    pty: Option<PtyEvent>,
    id: Option<u64>,
    reply: Option<Reply>,
    lsp: Option<LspLine>,
}

/// `{"lsp":{"lang","message"}}`, or `{"lsp":{"lang","closed"}}` at the end of the connection.
#[derive(serde::Deserialize)]
struct LspLine {
    lang: String,
    message: Option<serde_json::Value>,
    closed: Option<String>,
}

impl LspLine {
    fn into_in(self) -> LspIn {
        match (self.message, self.closed) {
            (Some(m), _) => LspIn::Message(m),
            (None, why) => LspIn::Closed(why.unwrap_or_default()),
        }
    }
}

enum Parsed {
    Line(Incoming),
    Lsp(LspLine),
}

/// Read once, into what it is: a view's event is its whole data (the runner's is over a megabyte), which was parsed
/// into a JSON value and then copied and parsed again (11 Nov).
fn parse(line: &str) -> Option<Parsed> {
    let l: Line = serde_json::from_str(line).ok()?;
    if let Some(h) = l.hello {
        return Some(Parsed::Line(Incoming::Hello(h)));
    }
    if let Some(e) = l.event {
        return Some(Parsed::Line(Incoming::Event(e)));
    }
    if let Some(t) = l.pty {
        return Some(Parsed::Line(Incoming::Pty(t)));
    }
    if let Some(m) = l.lsp {
        return Some(Parsed::Lsp(m));
    }
    Some(Parsed::Line(Incoming::Reply(l.id?, l.reply?)))
}

/// Whether `once` keeps one pipe per command for the whole run (`keep_pipes`): a still's.
static KEEP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// From now on `once` asks over one pipe per command, started at its first call and kept to the end of the run, not a
/// pipe of its own each time. Called by a still: each pipe is a whole snapshot on coordd, under its lock, and a still
/// of a long thread read every post's text on its own pipe, about 70 snapshots a still (docket-edge4, N-518.p14).
pub fn keep_pipes() {
    KEEP.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// A pipe kept for the run (`keep_pipes`): its stdin, what its reader passes on (None: it ended), the next id.
struct Kept {
    child: std::process::Child,
    stdin: ChildStdin,
    rx: Receiver<Option<Incoming>>,
    next: u64,
}

/// The kept pipes, by command.
static KEPT: Mutex<Vec<(Vec<String>, Kept)>> = Mutex::new(Vec::new());

/// How long `once` waits for what it asked before giving up on a pipe.
const ONCE_WAIT: Duration = Duration::from_secs(60);

/// Run CMD once with REQS (ids 1, 2, …), to its end: the snapshot and their replies. For the headless scenes, which
/// draw a box's real state in one frame. After `keep_pipes`, the same over the command's one kept pipe (`kept_once`),
/// except an attach, whose terminal output comes after its reply and ends only with the pipe. An empty CMD, a project
/// with no command (a test's made-up one), answers nothing, as a command that does not start does.
pub fn once(cmd: &[String], reqs: &[Request]) -> Vec<Incoming> {
    let Some(prog) = cmd.first() else { return Vec::new() };
    if KEEP.load(std::sync::atomic::Ordering::Relaxed) && !reqs.iter().any(|r| matches!(r, Request::Attach { .. })) {
        return kept_once(cmd, reqs);
    }
    let Ok(mut child) = crate::places::command(prog).args(&cmd[1..]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()
    else {
        return Vec::new();
    };
    if let Some(mut w) = child.stdin.take() {
        let _ = say_hello(&mut w, None);
        for (i, r) in reqs.iter().enumerate() {
            let _ = writeln!(w, "{}", json!({"id": i + 1, "req": r}));
        }
    }
    // a box that does not answer (sbx stuck) gives an empty still after a minute, not a hung scene
    let stdout = child.stdout.take().expect("piped");
    let child = Arc::new(Mutex::new(child));
    let watch = child.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(60));
        let _ = watch.lock().unwrap().kill();
    });
    let out: Vec<Incoming> = BufReader::new(stdout).lines().map_while(Result::ok).filter_map(|l| match parse(&l)? { Parsed::Line(i) => Some(i), Parsed::Lsp(_) => None }).collect();
    let _ = child.lock().unwrap().wait();
    out
}

/// `once` over CMD's kept pipe: REQS written with ids of the pipe's own, read back as 1, 2, … as `once` gives them.
/// What comes back: on a new pipe, all up to its snapshot and REQS' replies; on a kept one, the events since the last
/// call and REQS' replies. A pipe that ends, or does not answer within a minute, is let go, and the next call starts
/// another; what came is given.
fn kept_once(cmd: &[String], reqs: &[Request]) -> Vec<Incoming> {
    let mut kept = KEPT.lock().unwrap();
    let fresh = !kept.iter().any(|(c, _)| c == cmd);
    if fresh {
        let Some(k) = start_kept(cmd) else { return Vec::new() };
        kept.push((cmd.to_vec(), k));
    }
    let at = kept.iter().position(|(c, _)| c == cmd).expect("just found or put");
    let k = &mut kept[at].1;
    // our id of each request, and its number in REQS
    let mut ours: HashMap<u64, u64> = HashMap::new();
    // a pipe that ended takes no request; its reader says so (None) after what it gave
    for (i, r) in reqs.iter().enumerate() {
        k.next += 1;
        ours.insert(k.next, i as u64 + 1);
        let _ = writeln!(k.stdin, "{}", json!({"id": k.next, "req": r}));
    }
    let _ = k.stdin.flush();
    let mut out = Vec::new();
    let mut snapshot = !fresh;
    let end = Instant::now() + ONCE_WAIT;
    let mut ended = false;
    while !ended && (!snapshot || !ours.is_empty()) {
        match k.rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
            Ok(Some(Incoming::Reply(id, r))) => match ours.remove(&id) {
                Some(n) => out.push(Incoming::Reply(n, r)),
                None => {} // an earlier call's, given up on
            },
            Ok(Some(i)) => {
                snapshot |= matches!(&i, Incoming::Event(e) if matches!(e.body, coord_types::Body::Snapshot(_)));
                out.push(i);
            }
            Ok(None) | Err(_) => ended = true,
        }
    }
    if ended {
        let (_, mut k) = kept.remove(at);
        let _ = k.child.kill();
        let _ = k.child.wait();
        return out;
    }
    // the events that came meanwhile, as the pipe of its own gave them up to its end; a read alone on a kept pipe (the
    // state after a scene's request, which a pipe of its own gave as its snapshot) waits for the events to go quiet,
    // since a change's event may come just after its request's reply
    let quiet = if reqs.is_empty() && !fresh { QUIET } else { Duration::ZERO };
    let end = Instant::now() + QUIET_MAX;
    while let Ok(Some(i)) = k.rx.recv_timeout(quiet.min(end.saturating_duration_since(Instant::now()))) {
        match i {
            Incoming::Reply(..) => {}
            i => out.push(i),
        }
    }
    out
}

/// On CMD's kept pipe (a still's, `keep_pipes`, once `once` started it): what comes until DONE takes one, that one
/// with it, for as long as the pipe lives; nothing when CMD has no kept pipe. A stand-in window's wait for a change
/// to come on its link (`YAAIHH_STILL_AFTER`, coordd's text-change still, N-412 p1203).
pub fn until(cmd: &[String], done: impl Fn(&Incoming) -> bool) -> Vec<Incoming> {
    let mut kept = KEPT.lock().unwrap();
    let Some(at) = kept.iter().position(|(c, _)| c == cmd) else { return Vec::new() };
    let mut out = Vec::new();
    while let Ok(Some(i)) = kept[at].1.rx.recv() {
        let end = done(&i);
        out.push(i);
        if end {
            return out;
        }
    }
    let (_, mut k) = kept.remove(at);
    let _ = k.child.kill();
    let _ = k.child.wait();
    out
}

/// A read alone on a kept pipe takes the events until none came for this long, or QUIET_MAX has passed.
const QUIET: Duration = Duration::from_millis(250);
const QUIET_MAX: Duration = Duration::from_secs(2);

fn start_kept(cmd: &[String]) -> Option<Kept> {
    let mut child = crate::places::command(&cmd[0]).args(&cmd[1..]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let mut stdin = child.stdin.take()?;
    let _ = say_hello(&mut stdin, None);
    let stdout = child.stdout.take()?;
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        for l in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(Parsed::Line(i)) = parse(&l)
                && tx.send(Some(i)).is_err()
            {
                return;
            }
        }
        let _ = tx.send(None);
    });
    Some(Kept { child, stdin, rx, next: 0 })
}

#[cfg(test)]
mod tests {
    /// A stand-in window waits on its kept pipe for the look that comes after its snapshot, and takes it with what came
    /// before it, never what came after (coordd's text-change still, N-412 p1203); a pipe that ends gives what came.
    #[test]
    fn a_stand_in_waits_on_its_link_for_the_look() {
        use coord_types::{Body, Event};
        let dir = std::env::temp_dir().join(format!("yaaihh-until-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let line = |seq, body| serde_json::json!({"event": Event { seq, body }}).to_string();
        let first = [serde_json::json!({"hello": coord_types::Hello::ours(None)}).to_string(), line(1, Body::Snapshot(Default::default()))].join("\n");
        let look = serde_json::json!({"colour": {"attention": "#336699"}});
        let then = [line(2, Body::Look(look.clone())), line(3, Body::Look(serde_json::Value::Null))].join("\n");
        std::fs::write(dir.join("first"), first + "\n").unwrap();
        std::fs::write(dir.join("then"), then + "\n").unwrap();
        let d = dir.display();
        let cmd = vec!["sh".to_string(), "-c".into(), format!("cat {d}/first; sleep 0.3; cat {d}/then; sleep 1")];
        let k = super::start_kept(&cmd).expect("started");
        super::KEPT.lock().unwrap().push((cmd.clone(), k));
        let got = super::until(&cmd, |i| matches!(i, super::Incoming::Event(e) if matches!(e.body, Body::Look(_))));
        let seqs: Vec<u64> = got.iter().filter_map(|i| if let super::Incoming::Event(e) = i { Some(e.seq) } else { None }).collect();
        assert_eq!(seqs, [1, 2], "the snapshot, then the look, and not the event after it");
        assert!(matches!(got.last(), Some(super::Incoming::Event(Event { body: Body::Look(v), .. })) if *v == look));
        // what came after waits on the pipe; once the pipe ends, `until` gives what came and lets the pipe go
        let rest = super::until(&cmd, |_| false);
        assert!(matches!(&rest[..], [super::Incoming::Event(Event { seq: 3, .. })]), "{} came after the look", rest.len());
        assert!(super::until(&cmd, |_| true).is_empty(), "no kept pipe: nothing");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A still's live pipe names no release, whatever its build (I-1457; proto-lead N-1446 p607): a release build's
    /// window names its own while naming is on, and a still, or a window that has said "go", names none.
    #[test]
    fn a_still_names_no_release_whatever_its_build() {
        let r = "20261121-003000-b1e55ed0";
        let got = [super::named(true, true, Some(r)), super::named(false, true, Some(r)), super::named(false, false, Some(r)), super::named(false, true, None)];
        assert_eq!(got, [None, Some(r.to_string()), None, None]);
    }

    /// A hello naming its release says when this window became his, as the record writes a time (paper-research
    /// N-1446 p1392); one naming none says no time, whatever the window holds.
    #[test]
    fn a_hello_naming_its_release_says_since_when_the_window_is_his() {
        let at = chrono::DateTime::parse_from_rfc3339("2026-11-21T10:34:49.312+01:00").unwrap().to_utc();
        let since = at.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let v = serde_json::to_value(super::hello(Some("20261121-101928-4ee5c14e".into()), Some(since.clone()))).unwrap();
        assert_eq!((v["release"].as_str(), v["since"].as_str()), (Some("20261121-101928-4ee5c14e"), Some("2026-11-21T09:34:49Z")));
        assert_eq!(serde_json::to_value(super::hello(None, Some(since))).unwrap().get("since"), None);
        super::his_since(at);
        assert_eq!(super::SINCE.lock().unwrap().as_deref(), Some("2026-11-21T09:34:49Z"), "his_since writes the same");
    }

    /// A line this window cannot read is said once a pipe, not dropped in silence (N-447 post 19).
    #[test]
    fn a_line_this_window_cannot_read_is_said_once() {
        let script = r#"echo '{"event":{"seq":"new"}}'; echo '{"event":{"seq":"newer"}}'; echo '{"id":1,"reply":{"ok":true,"out":""}}'"#;
        let (tx, rx) = channel();
        let cmd = ["sh".to_string(), "-c".into(), script.into()];
        let wake: rune::Waker = Arc::new(|| {});
        run(&cmd, &Mutex::new(None), &tx, (&wake, &wake), &Sent::default(), &LspPipe::default(), &mut Resume::default());
        let got: Vec<Incoming> = rx.try_iter().collect();
        assert_eq!(got.iter().filter(|i| matches!(i, Incoming::Unread)).count(), 1);
        assert!(got.iter().any(|i| matches!(i, Incoming::Reply(1, _))), "what it can read still comes");
    }

    /// A terminal's output wakes the window at once (NOW); the rest is paced (WAKE).
    #[test]
    fn a_terminals_output_wakes_at_once_and_the_rest_is_paced() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let (paced, now) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let (p, n) = (paced.clone(), now.clone());
        let (wake, at_once): (rune::Waker, rune::Waker) = (Arc::new(move || { p.fetch_add(1, Ordering::SeqCst); }), Arc::new(move || { n.fetch_add(1, Ordering::SeqCst); }));
        let script = r#"echo '{"pty":{"kind":"output","name":"a","data":"YQ=="}}'; echo '{"id":1,"reply":{"ok":true,"out":""}}'"#;
        let (tx, rx) = channel();
        let cmd = ["sh".to_string(), "-c".into(), script.into()];
        run(&cmd, &Mutex::new(None), &tx, (&wake, &at_once), &Sent::default(), &LspPipe::default(), &mut Resume::default());
        assert!(matches!(rx.try_recv(), Ok(Incoming::Pty(_))));
        assert_eq!((now.load(Ordering::SeqCst), paced.load(Ordering::SeqCst)), (1, 1));
    }

    use super::*;

    /// A still's reads go over one kept pipe (N-518.p15): a stand-in bridge that counts its starts, gives a snapshot
    /// and answers each request with its id. Its replies come back numbered as `once` numbers them; a pipe that ended
    /// is started again at the next call.
    #[test]
    fn a_stills_reads_go_over_one_pipe() {
        let dir = std::env::temp_dir().join(format!("yaaihh-kept-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let starts = dir.join("starts");
        let snap = json!({"event": Event { seq: 1, body: coord_types::Body::Snapshot(coord_types::Snapshot::default()) }});
        let bridge = |then: &str| {
            let script = format!(
                r#"echo x >> '{}'; echo '{snap}'; {then}"#,
                starts.display()
            );
            vec!["sh".to_string(), "-c".into(), script]
        };
        let answers = r#"while read -r l; do id=$(printf '%s' "$l" | sed -n 's/.*"id":\([0-9]*\).*/\1/p'); [ -n "$id" ] && echo "{\"id\":$id,\"reply\":{\"ok\":true,\"out\":\"r$id\"}}"; done"#;
        let cmd = bridge(answers);
        let is_snapshot = |i: &Incoming| matches!(i, Incoming::Event(e) if matches!(e.body, coord_types::Body::Snapshot(_)));
        let replies = |got: &[Incoming]| -> Vec<(u64, String)> {
            got.iter().filter_map(|i| match i { Incoming::Reply(n, r) => Some((*n, r.out.clone())), _ => None }).collect()
        };
        let first = kept_once(&cmd, &[]);
        assert!(first.iter().any(is_snapshot), "a new pipe gives its snapshot");
        let got = kept_once(&cmd, &[Request::Kinds, Request::Kinds]);
        assert_eq!(replies(&got), vec![(1, "r1".to_string()), (2, "r2".to_string())]);
        assert!(!got.iter().any(is_snapshot), "a kept pipe gives no second snapshot");
        for k in 3..=12 {
            assert_eq!(replies(&kept_once(&cmd, &[Request::Kinds])), vec![(1, format!("r{k}"))], "the pipe's own id, given as 1");
        }
        assert_eq!(std::fs::read_to_string(&starts).unwrap().lines().count(), 1, "one pipe for twelve reads");
        // a bridge that ends after its snapshot (coordd went): what came is given, and the next call starts another
        let gone = bridge("exit 0");
        assert!(kept_once(&gone, &[Request::Kinds]).iter().any(is_snapshot));
        kept_once(&gone, &[Request::Kinds]);
        assert_eq!(std::fs::read_to_string(&starts).unwrap().lines().count(), 3);
        KEPT.lock().unwrap().clear();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// docket-edge2 (N-412.p92): an answer over 500 ms gives one line with the request's kind and the time; a quick
    /// one gives none; each id is timed once.
    #[test]
    fn a_slow_answer_is_logged_with_its_kind() {
        let sent: Sent = Arc::default();
        let t0 = Instant::now();
        let req = json!({"id": 1, "req": Request::Keep { ts: vec!["N-1".into()] }});
        sent.lock().unwrap().insert(1, (t0, kind_of(&req["req"])));
        sent.lock().unwrap().insert(2, (t0, "transcript".into()));
        assert_eq!(answered(&sent, 2, t0 + Duration::from_millis(120)), None, "quick");
        assert_eq!(answered(&sent, 1, t0 + Duration::from_millis(730)).as_deref(), Some("slow answer: keep took 730 ms"));
        assert_eq!(answered(&sent, 1, t0 + Duration::from_millis(900)), None, "once");
    }

    /// A pipe started again resumes with `--from RUN:SEQ`: as it is on this PC's server, quoted for the remote shell
    /// when the pipe goes by ssh to another device; none before the bridge has said protocol 1.
    #[test]
    fn a_pipe_by_ssh_resumes_with_its_words_quoted() {
        let local = vec!["/b/bin/host-coordd".to_string(), "sh".into(), "research".into(), "coordc".into(), "bridge".into()];
        let d = crate::places::Device { name: "tower".into(), ssh: "t@tower-server".into(), agent_base: None };
        let by_ssh = crate::places::on(local.clone(), Some(&d));
        let mut at = super::Resume { run: Some("2026-11-18T10:01:56Z 4242".into()), seq: 17, speaks: false };
        assert_eq!(at.cmd(&by_ssh), by_ssh, "no protocol 1 yet: no --from");
        at.speaks = true;
        assert_eq!(at.cmd(&local)[local.len()..], ["--from", "2026-11-18T10:01:56Z 4242:17"]);
        assert_eq!(at.cmd(&by_ssh)[by_ssh.len()..], ["--from", "'2026-11-18T10:01:56Z 4242:17'"]);
    }
}
