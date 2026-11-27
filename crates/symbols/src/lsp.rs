//! A language server as a child process, or a connection to the project's shared one (`[[lsp]] socket`): JSON-RPC
//! over its stdin and stdout or the socket (Content-Length frames), a reader thread
//! that takes the answers, the diagnostics it publishes and its progress, and answers what the server asks of us.

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::os::unix::net::UnixStream;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::os::unix::process::CommandExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::time::{Duration, Instant};

/// What the reader thread keeps, behind one lock.
#[derive(Default)]
struct Shared {
    answers: HashMap<u64, Value>,
    /// The diagnostics last published for each file (by URI), and when.
    diagnostics: HashMap<String, Vec<Value>>,
    /// Bumped with every publish: a wait sees that something came.
    published: u64,
    last_publish: Option<Instant>,
    /// The work the server says it is doing ($/progress begin without its end yet), by token.
    busy: HashMap<String, Work>,
    /// Bumped with every $/progress end.
    ended: u64,
    /// The server has reported work at least once (an agda-ls before progress never does).
    reports_work: bool,
    /// When the last `$/progress` came (begin, report or end): a wait with none since its start is a silent one.
    last_progress: Option<Instant>,
    closed: bool,
    /// The server's load of its view's files, as it said (`tovrin/loading`, `tovrin/loadState`: tool-scan's agda-ls
    /// 9eb098e, hs-ls the same; N-233 p142/p166); None from a server that says nothing of it.
    load: Option<Load>,
    /// The ids of `tovrin/loadState` asked and not yet answered: their answers set `load`, nobody waits on them.
    load_asks: std::collections::HashSet<u64>,
    /// When the last `tovrin/loading` came: a wait that heard one was a wait on the load, not on a silent server.
    last_load: Option<Instant>,
}

/// A server's load: DONE of TOTAL files, READY once all are, STARTED once it has begun (the first full view/update).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Load {
    done: u64,
    total: u64,
    ready: bool,
    started: bool,
}

impl Load {
    /// From `tovrin/loading`'s params or `tovrin/loadState`'s result; None for anything else (an error, null).
    fn of(v: &Value, notified: bool) -> Option<Load> {
        let done = v.get("done")?.as_u64()?;
        let total = v.get("total")?.as_u64()?;
        // a notification is sent once the load has begun, and once more with done == total at its end
        let ready = v.get("ready").and_then(Value::as_bool).unwrap_or(notified && done >= total);
        // hs-ls 475c15a has no `started`: total 0 until its load begins (N-233 p171)
        let started = v.get("started").and_then(Value::as_bool).unwrap_or(notified || total > 0);
        Some(Load { done, total, ready, started })
    }
}

/// A work a server reported begun and not yet ended: its title, its last message and percentage, since when.
struct Work {
    title: String,
    message: String,
    percentage: Option<u64>,
    since: Instant,
}

/// The client's limit on a tool call (paper-research, N-101 post 119): Claude Code ends a call that has not answered
/// within its MCP_TOOL_TIMEOUT (in ms; agent-base's claude/settings.json sets 660000, and the MCP servers it starts
/// get its environment). Read once; unset or unreadable, there is none and the waits are on no clock.
pub fn client_limit() -> Option<Duration> {
    static L: std::sync::OnceLock<Option<Duration>> = std::sync::OnceLock::new();
    *L.get_or_init(|| std::env::var("MCP_TOOL_TIMEOUT").ok().and_then(|v| v.trim().parse::<u64>().ok()).filter(|&ms| ms > 0).map(Duration::from_millis))
}

/// How long before the client's limit a call answers: a twentieth of the limit, at least 5 s (the answer must still
/// reach the client, and a call's clock there began a little before symbols read it).
pub fn margin(limit: Duration) -> Duration {
    (limit / 20).max(Duration::from_secs(5)).min(limit / 2)
}

// When the running call must answer by (its arrival plus the client's limit, less the margin); None: no clock.
// The thread's own: a call runs and waits on one thread (and tests run side by side).
thread_local! {
    static DEADLINE: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
}

fn set_deadline(d: Option<Instant>) {
    DEADLINE.with(|c| c.set(d));
}

fn deadline() -> Option<Instant> {
    DEADLINE.with(|c| c.get())
}

/// A wait cut at the deadline: the work goes on in the server, and the next call takes it up again. Its text says
/// what the server is doing and for how long.
#[derive(Debug)]
pub struct StillWorking(pub String);

impl std::fmt::Display for StillWorking {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StillWorking {}

/// Whether E is a wait cut at the deadline.
pub fn still_working(e: &anyhow::Error) -> Option<&StillWorking> {
    e.downcast_ref::<StillWorking>()
}

/// "for 9 min 12 s"
pub fn took(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 60 { format!("{} min {} s", s / 60, s % 60) } else { format!("{s} s") }
}

/// What NAME is doing, as it reported it: each work begun and not ended, with its message and how long so far.
fn at_work(name: &str, g: &Shared) -> String {
    // the load first: what an agent waits on before references and rename (agda-ls: they wait for it)
    let load = match g.load {
        Some(l) if l.ready => String::new(),
        Some(l) if l.started => format!("{name} is still loading: {} of {} files; ", l.done, l.total),
        Some(_) => format!("{name} has not begun loading its files (it begins at the first full view); "),
        None => String::new(),
    };
    if g.busy.is_empty() {
        return if load.is_empty() { format!("{name} reports no work in progress") } else { load.trim_end_matches("; ").to_string() };
    }
    let mut ws: Vec<&Work> = g.busy.values().collect();
    ws.sort_by_key(|w| w.since);
    let each: Vec<String> = ws
        .iter()
        .map(|w| {
            let mut t = w.title.clone();
            if !w.message.is_empty() {
                t.push_str(&format!(": {}", w.message));
            }
            if let Some(p) = w.percentage {
                t.push_str(&format!(" ({p}%)"));
            }
            format!("{t}, {} so far", took(w.since.elapsed()))
        })
        .collect();
    format!("{load}{name} is at work: {}", each.join("; "))
}

/// The deadline has passed for a wait on WHAT that began at START: the wait is cut, saying what the server does.
fn cut(name: &str, g: &Shared, start: Instant, what: &str) -> anyhow::Error {
    let limit = client_limit().map(|l| l.as_secs()).unwrap_or(0);
    let t = format!("{}; this call waited {} on {what}, and answers now because Claude Code ends a tool call after {limit} s (MCP_TOOL_TIMEOUT)", at_work(name, g), took(start.elapsed()));
    eprintln!("symbols: cut: {t}");
    anyhow::Error::new(StillWorking(t))
}

/// The silent wait (paper-research, N-117.p29): a wait that heard no `$/progress` from its server since it began, for
/// at least this long, is named in the call's answer, so a hang is recognisable afterwards. It ends no wait.
const SILENT: Duration = Duration::from_secs(10);

/// The running tool call was cancelled by the MCP client (the session was interrupted): every wait ends at once.
static CANCELLED: AtomicBool = AtomicBool::new(false);

thread_local! {
    // what the waits of the running call had to say (a silent server); the call's answer ends with them. Kept by the thread that runs the call (the waits run on it), so that two calls on two threads (the tests) do not
    // take each other's
    static NOTES: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A note for the running call's answer.
pub fn note(text: String) {
    NOTES.with(|n| n.borrow_mut().push(text));
}

/// A new tool call that came at ARRIVED: nothing cancelled, nothing noted, its deadline set from the client's limit
/// (None for a call not from an MCP client).
pub fn begin_call(arrived: Option<Instant>) {
    CANCELLED.store(false, Ordering::SeqCst);
    set_deadline(arrived.zip(client_limit()).map(|(a, l)| a + l - margin(l)));
    NOTES.with(|n| n.borrow_mut().clear());
}

/// Whether the running call (or the outline behind the calls) was cancelled.
pub fn cancelled() -> bool {
    CANCELLED.load(Ordering::SeqCst)
}

/// The notes of the running call, taken.
pub fn take_notes() -> Vec<String> {
    NOTES.with(|n| std::mem::take(&mut *n.borrow_mut()))
}

/// The MCP client cancelled the running call: each wait wakes, sees it and ends (main.rs's reader of stdin).
pub fn cancel() {
    CANCELLED.store(true, Ordering::SeqCst);
    for l in LINKS.lock().unwrap().iter().filter_map(Weak::upgrade) {
        let _guard = l.shared.0.lock().unwrap();
        l.shared.1.notify_all();
    }
}

/// "SERVER reported no work for N s before this", when a wait of WAITED since START heard no progress from it.
fn silence(name: &str, g: &Shared, start: Instant) -> Option<String> {
    let waited = start.elapsed();
    let heard = g.last_progress.is_some_and(|t| t >= start);
    // a wait through the server's load (agda-ls: references and rename wait for it) is said as that, not as no work:
    // Tovrin-dev's f1634-1 waited 451 s on a load beside three others, told "agda reported no work" (N-101 p1447)
    if waited >= SILENT && g.last_load.is_some_and(|t| t >= start) {
        return Some(match g.load {
            Some(l) if !l.ready => format!("{name} is still loading: {} of {} files, {} s of this wait", l.done, l.total, waited.as_secs()),
            Some(l) => format!("this waited {} s for {name} to load its {} files; the next call does not wait for it", waited.as_secs(), l.total),
            None => format!("{name} was loading its files for {} s of this wait", waited.as_secs()),
        });
    }
    (waited >= SILENT && !heard).then(|| format!("{name} reported no work for {} s before this", waited.as_secs()))
}

/// A wait ends: its silence, if any, goes in the notes and in symbols' log; cancelled, the wait is an error saying it.
fn ended(name: &str, g: &Shared, start: Instant, what: &str) -> Result<()> {
    let s = silence(name, g, start);
    if let Some(s) = &s {
        eprintln!("symbols: {what}: {s}");
    }
    if CANCELLED.load(Ordering::SeqCst) {
        // interrupted, the silence is said however short the wait was: it is what the one interrupting wants to know
        let heard = g.last_progress.is_some_and(|t| t >= start);
        let secs = start.elapsed().as_secs();
        let why = format!("interrupted while waiting on {what}, after {secs} s; {name} {}", if heard { "had reported work during it" } else { "reported no work during it" });
        eprintln!("symbols: {why}");
        bail!("{why}");
    }
    if let Some(s) = s {
        NOTES.with(|n| n.borrow_mut().push(s));
    }
    Ok(())
}

pub struct Server {
    /// What it is: a child of ours, or a connection to a server the project started (`socket`).
    conn: Conn,
    shared: Arc<(Mutex<Shared>, Condvar)>,
    /// What ends it, also from the signal thread (stop_all).
    link: Arc<Link>,
    /// Open documents and their versions.
    versions: Mutex<HashMap<String, i64>>,
    /// What was last sent of each file (a hash of its text): a file whose text on disk differs was changed outside
    /// symbols (Claude Code's Edit, the shell) and is sent again before the next call (paper-research, N-101 post 219).
    sent: Mutex<HashMap<PathBuf, u64>>,
    /// The server takes the view as data (design.md 4.1: `capabilities.experimental.view` at initialize): it is given
    /// the bases and `view/update`s, and no file is opened in it.
    pub view: bool,
    /// What the view's updates last told it of each path, and the repositories (with their bases) last given.
    view_sent: Mutex<(HashMap<PathBuf, Sent>, Vec<crate::view::Repo>)>,
    /// The position encoding agreed at initialize: "utf-8" or "utf-16".
    pub utf8: bool,
    language: String,
    /// hs-ls's `hs/index` has been asked for (once a session; a server without it answers an error, once).
    indexed: AtomicBool,
    /// hs/index sent and not yet answered (a call cut while waiting on it): its id and when it was sent.
    index_pending: Mutex<Option<(u64, Instant)>>,
}

/// How symbols reaches a server: a child on stdin and stdout, or a Unix socket (`[[lsp]] socket`: one server for
/// the project, each agent's symbols a client of it; paper-research, N-101 post 145, item 7).
enum Conn {
    /// The child, and its stdin's own handle, held as long as the server; every write goes through link.out (a
    /// duplicate).
    Child(Child, #[allow(dead_code)] ChildStdin),
    /// The connection: shut down when the server is dropped, as the child is killed.
    Socket(UnixStream, PathBuf),
}

/// Where a server is, as `[[lsp]]` says: a command to start, or the socket of one already running.
pub enum How<'a> {
    Command(&'a [String]),
    Socket(&'a Path),
}

/// What `initialize` tells a server of the client, beside the settings (N-181, N-184): the folders kept out of it
/// (`[[lsp]] skip`, relative to the root as written), the root as the agent sees it, and the layer the agent is in
/// (coordd's COORD_LAYER; none outside a layer), and `viewPid`, the pid of the process holding the layer's view
/// (coordd's ROSTER_VIEW, kept by roster in the agent's environment). A shared server sees every layer's view at the
/// same path, and reads this one's files at /proc/VIEWPID/root, the same for the layer's whole life (N-101 posts 171
/// and 174); a server of its own ignores what it does not know.
///
/// EXTRA is the rest: `skip`, `view` (design.md 4.1: the repositories and their bases) and, under the language's
/// name, the `[[lsp]]` entry's own `options`, merged over the settings.
fn client_options(lang: &str, root: &Path, extra: &Value) -> Value {
    let mut o = settings(lang);
    o["skip"] = extra.get("skip").cloned().unwrap_or(json!([]));
    if let Some(v) = extra.get("view") {
        o["view"] = v.clone();
    }
    if let Some(Value::Object(opts)) = extra.get(lang) {
        if !o.get(lang).is_some_and(Value::is_object) {
            o[lang] = json!({});
        }
        for (k, v) in opts {
            o[lang][k] = v.clone();
        }
    }
    o["root"] = json!(root.display().to_string());
    if let Ok(l) = std::env::var("COORD_LAYER") {
        if !l.trim().is_empty() {
            o["layer"] = json!(l.trim());
        }
    }
    if let Some(pid) = std::env::var("ROSTER_VIEW").ok().and_then(|v| v.trim().parse::<u32>().ok()).filter(|&p| p > 0) {
        o["viewPid"] = json!(pid);
    }
    o
}

/// Settings the server asks for (workspace/configuration) and is given at initialize, by server: HLS renames across
/// modules only with this (and even then, 2.13 leaves the other modules' uses: see ops::rename).
fn settings(lang: &str) -> Value {
    match lang {
        "haskell" => json!({"haskell": {"plugin": {"rename": {"config": {"crossModule": true}}}}}),
        _ => json!({}),
    }
}

/// What a view's update last said of a path: its content (by hash), or that it is deleted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sent {
    Text(u64),
    Deleted,
}

/// The hash of a file's bytes.
pub fn hash_bytes(b: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b.hash(&mut h);
    h.finish()
}

/// The hash of a file's text, to tell whether what is on disk is what was sent.
pub fn hash(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// A server as its stopping needs it: a way to write to it, its answers, its process group.
pub struct Link {
    name: String,
    /// The one way to write to the server: requests from here and the reader's answers share its lock.
    out: Arc<Writer>,
    shared: Arc<(Mutex<Shared>, Condvar)>,
    next: Mutex<u64>,
    /// The child's process group; none for a socket (nothing of ours to kill).
    pid: Option<i32>,
    done: AtomicBool,
}

/// Every server started, for stop_all.
static LINKS: Mutex<Vec<Weak<Link>>> = Mutex::new(Vec::new());

/// End every server still running (symbols is told to stop: SIGTERM, SIGINT, SIGHUP; main.rs).
pub fn stop_all() {
    let links: Vec<Arc<Link>> = LINKS.lock().unwrap().iter().filter_map(Weak::upgrade).collect();
    let ts: Vec<_> = links.into_iter().map(|l| std::thread::spawn(move || l.stop())).collect();
    for t in ts {
        let _ = t.join();
    }
}

impl Link {
    fn write(&self, msg: &Value) -> Result<()> {
        self.out.send(msg)
    }

    /// A request and its answer: waits for the answer or the server's end, on no clock.
    fn request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.send_request(method, params)?;
        self.wait_answer(id, method)
    }

    /// A request sent; its id, for wait_answer.
    fn send_request(&self, method: &str, params: Value) -> Result<u64> {
        let id = {
            let mut n = self.next.lock().unwrap();
            *n += 1;
            *n
        };
        self.write(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        Ok(id)
    }

    /// The answer to request ID: waits for it or the server's end, until the call's deadline (a cut leaves the
    /// answer to come; a later wait on the same id takes it).
    fn wait_answer(&self, id: u64, method: &str) -> Result<Value> {
        let (lock, cv) = &*self.shared;
        let start = Instant::now();
        let mut g = lock.lock().unwrap();
        loop {
            if CANCELLED.load(Ordering::SeqCst) {
                ended(&self.name, &g, start, &format!("{} ({method})", self.name))?;
            }
            if let Some(a) = g.answers.remove(&id) {
                ended(&self.name, &g, start, &format!("{} ({method})", self.name))?;
                if let Some(e) = a.get("error") {
                    bail!("{} ({method}): {}", self.name, e.get("message").and_then(Value::as_str).unwrap_or("an error"));
                }
                return Ok(a.get("result").cloned().unwrap_or(Value::Null));
            }
            if g.closed {
                bail!("{}: the server ended", self.name);
            }
            g = match deadline() {
                Some(d) => {
                    let now = Instant::now();
                    if now >= d {
                        return Err(cut(&self.name, &g, start, &format!("its answer to {method}")));
                    }
                    cv.wait_timeout(g, d - now).unwrap().0
                }
                None => cv.wait(g).unwrap(),
            };
        }
    }

    /// LSP `shutdown` and `exit`, then at once, by pid, the server and every process of its group (its own children,
    /// as rust-analyzer's proc-macro servers). Nothing waits: a language server holds nothing that needs a graceful
    /// end. Once.
    pub fn stop(&self) {
        if self.done.swap(true, Ordering::SeqCst) {
            return;
        }
        let id = {
            let mut n = self.next.lock().unwrap();
            *n += 1;
            *n
        };
        let _ = self.write(&json!({"jsonrpc": "2.0", "id": id, "method": "shutdown", "params": null}));
        let _ = self.write(&json!({"jsonrpc": "2.0", "method": "exit", "params": null}));
        // SAFETY: a signal to the server's own process group (its pid: process_group(0) at its start)
        if let Some(pid) = self.pid {
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
    }
}

impl Server {
    /// A child server with no skip list (the tests' fake servers).
    #[cfg(test)]
    pub fn start(name: &str, lang: &str, command: &[String], root: &Path) -> Result<Server> {
        Server::begin(name, lang, How::Command(command), root, &json!({}))
    }

    /// A server started (a command) or connected to (a socket), initialized with the client's options.
    pub fn begin(name: &str, lang: &str, how: How, root: &Path, extra: &Value) -> Result<Server> {
        match how {
            How::Command(command) => Server::spawn(name, lang, command, root, extra),
            How::Socket(path) => {
                let stream = UnixStream::connect(path).with_context(|| format!("server {name}: connecting to {}", path.display()))?;
                let reader = stream.try_clone().context("the socket's reader")?;
                let out = Arc::new(Writer::new(std::fs::File::from(std::os::fd::OwnedFd::from(stream.try_clone().context("the socket's writer")?))));
                eprintln!("symbols: the {lang} server: connected to {}", path.display());
                Server::with(name, lang, Conn::Socket(stream, path.to_path_buf()), reader, out, None, root, extra)
            }
        }
    }

    fn spawn(name: &str, lang: &str, command: &[String], root: &Path, extra: &Value) -> Result<Server> {
        let (prog, args) = command.split_first().ok_or_else(|| anyhow!("server {name}: an empty command"))?;
        let mut cmd = Command::new(prog);
        cmd.args(args)
            .current_dir(root)
            // a process group of its own: its children are ended with it (Link::stop)
            .process_group(0);
        // SAFETY: async-signal-safe calls only, between fork and exec. The signals symbols waits for in its signal
        // thread are blocked in it, and a child inherits the mask: unblocked here. And if symbols is killed outright,
        // the kernel ends the server too (PR_SET_PDEATHSIG; its own children are then its matter).
        unsafe {
            cmd.pre_exec(|| {
                let mut set: libc::sigset_t = std::mem::zeroed();
                libc::sigemptyset(&mut set);
                libc::pthread_sigmask(libc::SIG_SETMASK, &set, std::ptr::null_mut());
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                Ok(())
            });
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // its log goes where ours goes (the session's MCP log), not into the answers
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("server {name}: starting {prog}"))?;
        let stdout = child.stdout.take().unwrap();
        let stdin = child.stdin.take().unwrap();
        let pid = child.id() as i32;
        let out = Arc::new(dup_writer(&stdin)?);
        Server::with(name, lang, Conn::Child(child, stdin), stdout, out, Some(pid), root, extra)
    }

    #[allow(clippy::too_many_arguments)]
    fn with(name: &str, lang: &str, conn: Conn, input: impl Read + Send + 'static, out: Arc<Writer>, pid: Option<i32>, root: &Path, extra: &Value) -> Result<Server> {
        let shared = Arc::new((Mutex::new(Shared::default()), Condvar::new()));
        let link = Arc::new(Link { name: name.into(), out, shared: shared.clone(), next: Mutex::new(0), pid, done: AtomicBool::new(false) });
        LINKS.lock().unwrap().push(Arc::downgrade(&link));
        let s = Server {
            conn,
            shared: shared.clone(),
            link,
            versions: Mutex::new(HashMap::new()),
            sent: Mutex::new(HashMap::new()),
            view: false,
            view_sent: Mutex::new((HashMap::new(), Vec::new())),
            utf8: false,
            language: lang.into(),
            indexed: AtomicBool::new(false),
            index_pending: Mutex::new(None),
        };
        let writer = s.link.out.clone();
        let lang2 = lang.to_string();
        std::thread::spawn(move || read_loop(BufReader::new(input), shared, writer, lang2));
        let mut s = s;
        let init = s.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": uri(root),
                "workspaceFolders": [{"uri": uri(root), "name": "root"}],
                "initializationOptions": client_options(lang, root, extra),
                "capabilities": {
                    "general": {"positionEncodings": ["utf-8", "utf-16"]},
                    "textDocument": {
                        "documentSymbol": {"hierarchicalDocumentSymbolSupport": true},
                        "hover": {"contentFormat": ["markdown", "plaintext"]},
                        "rename": {"prepareSupport": true},
                        "references": {},
                        "definition": {},
                        "publishDiagnostics": {"relatedInformation": false},
                        "synchronization": {"didSave": true}
                    },
                    "workspace": {"workspaceEdit": {"documentChanges": true}, "configuration": true, "workspaceFolders": true},
                    "window": {"workDoneProgress": true}
                }
            }),
        )?;
        s.utf8 = init.pointer("/capabilities/positionEncoding").and_then(Value::as_str) == Some("utf-8");
        s.view = extra.get("view").is_some() && init.pointer("/capabilities/experimental/view").is_some_and(|v| v.as_u64().is_some_and(|n| n >= 1) || v.as_bool() == Some(true));
        eprintln!("symbols: the {lang} server {}", if s.view { "takes the view as data (view/update)" } else { "is given its files by didOpen" });
        s.notify("initialized", json!({}))?;
        s.notify("workspace/didChangeConfiguration", json!({"settings": settings(lang)}))?;
        s.ask_load();
        Ok(s)
    }

    fn write(&self, msg: &Value) -> Result<()> {
        self.link.write(msg)
    }

    pub fn notify(&self, method: &str, params: Value) -> Result<()> {
        self.write(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    /// A request and its answer's result (an error answer is an Err with the server's message).
    pub fn request(&self, method: &str, params: Value) -> Result<Value> {
        self.link.request(method, params)
    }

    /// The server said it has begun or will begin a load of its view's files and has not said it is done; false for
    /// one that says nothing of a load.
    pub fn loading(&self) -> bool {
        self.shared.0.lock().unwrap().load.is_some_and(|l| !l.ready)
    }

    /// `tovrin/loadState` asked, not waited on: its answer sets the load (`took_load`). A server without it answers
    /// an error, which is dropped. Asked at connect, a shared server's load another client began is known at once.
    fn ask_load(&self) {
        let Ok(id) = self.link.send_request("tovrin/loadState", json!({})) else { return };
        let (lock, _) = &*self.shared;
        let mut g = lock.lock().unwrap();
        // the answer may have come already, before its id was noted
        match g.answers.remove(&id) {
            Some(a) => took_load(&self.language, &mut g, &a),
            None => {
                g.load_asks.insert(id);
            }
        }
    }

    /// hs-ls answers references from a file not open for the whole project only after `hs/index` has loaded every
    /// module (N-163.p3): asked once a session, before the first references or rename. Plain HLS has no such method;
    /// its error is logged and not asked again. Cut at the call's deadline (808 s cold on Tovrin's checker), the
    /// request stays pending and the next call waits on its answer: references before it would cover the open files
    /// only, with nothing to say so.
    pub fn index_once(&self, path: &Path) -> Result<()> {
        if self.language != "haskell" || self.indexed.load(Ordering::SeqCst) {
            return Ok(());
        }
        let mut pending = self.index_pending.lock().unwrap();
        let (id, since) = match *pending {
            Some(p) => p,
            None => {
                let p = (self.link.send_request("hs/index", json!({"textDocument": {"uri": uri(path)}}))?, Instant::now());
                *pending = Some(p);
                p
            }
        };
        match self.link.wait_answer(id, "hs/index") {
            Ok(_) => eprintln!("symbols: hs/index took {:.1} s", since.elapsed().as_secs_f64()),
            Err(e) if still_working(&e).is_some() => {
                return Err(anyhow::Error::new(StillWorking(format!("hs/index (loading and indexing every module, so that references cover the project), {} so far: {e}", took(since.elapsed())))));
            }
            Err(e) => eprintln!("symbols: no hs/index: {e}"),
        }
        *pending = None;
        self.indexed.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn open(&self, path: &Path, text: &str) -> Result<()> {
        let u = uri(path);
        let mut v = self.versions.lock().unwrap();
        if v.contains_key(&u) {
            return Ok(());
        }
        v.insert(u.clone(), 1);
        drop(v);
        self.sent.lock().unwrap().insert(path.to_path_buf(), hash(text));
        self.notify("textDocument/didOpen", json!({"textDocument": {"uri": u, "languageId": self.language, "version": 1, "text": text}}))
    }

    /// The file's new text, given whole, then saved (servers that check on save, as rust-analyzer's cargo check).
    pub fn changed(&self, path: &Path, text: &str) -> Result<()> {
        let u = uri(path);
        self.sent.lock().unwrap().insert(path.to_path_buf(), hash(text));
        let ver = {
            let mut v = self.versions.lock().unwrap();
            let n = v.entry(u.clone()).or_insert(0);
            *n += 1;
            *n
        };
        if ver == 1 {
            self.notify("textDocument/didOpen", json!({"textDocument": {"uri": u, "languageId": self.language, "version": 1, "text": text}}))?;
        } else {
            self.notify("textDocument/didChange", json!({"textDocument": {"uri": u, "version": ver}, "contentChanges": [{"text": text}]}))?;
        }
        self.notify("textDocument/didSave", json!({"textDocument": {"uri": u}, "text": text}))
    }

    /// What the view's updates last told it, and the repositories last given.
    pub fn view_sent(&self) -> (HashMap<PathBuf, Sent>, Vec<crate::view::Repo>) {
        self.view_sent.lock().unwrap().clone()
    }

    /// A `view/update` (design.md 4.1): CHANGED as (path, entry, hash of its bytes), DELETED as (path, entry), FULL
    /// with REPOS when the view is given whole. Waits for the answer (the server has taken it, before any check);
    /// what was told is kept only then. The number the server says it took.
    pub fn view_update(&self, full: bool, repos: &[crate::view::Repo], changed: Vec<(PathBuf, Value, u64)>, deleted: Vec<(PathBuf, Value)>) -> Result<u64> {
        let mut params = json!({
            "full": full,
            "changed": changed.iter().map(|(_, e, _)| e.clone()).collect::<Vec<_>>(),
            "deleted": deleted.iter().map(|(_, e)| e.clone()).collect::<Vec<_>>(),
        });
        let (_, last) = self.view_sent();
        if full && !last.is_empty() && last != repos {
            params["base"] = json!(repos.iter().map(|r| (r.name.clone(), json!(r.base))).collect::<serde_json::Map<_, _>>());
        }
        let id = self.link.send_request("view/update", params)?;
        let a = self.link.wait_answer(id, "view/update")?;
        if full {
            // the load begins at the first full view (agda-ls): its count from now, before its first notification
            self.ask_load();
        }
        let mut g = self.view_sent.lock().unwrap();
        if full {
            g.0.clear();
            g.1 = repos.to_vec();
        }
        for (p, _, h) in changed {
            g.0.insert(p, Sent::Text(h));
        }
        for (p, e) in deleted {
            if e.get("same_as_base").is_some() {
                g.0.remove(&p);
            } else {
                g.0.insert(p, Sent::Deleted);
            }
        }
        Ok(a.get("taken").and_then(Value::as_u64).unwrap_or(0))
    }

    /// The files sent to it, each with the hash of the text last sent.
    pub fn sent(&self) -> Vec<(PathBuf, u64)> {
        self.sent.lock().unwrap().iter().map(|(p, h)| (p.clone(), *h)).collect()
    }

    /// A file gone from disk: closed in the server, and no longer sent.
    pub fn closed_file(&self, path: &Path) -> Result<()> {
        self.sent.lock().unwrap().remove(path);
        let u = uri(path);
        if self.versions.lock().unwrap().remove(&u).is_none() {
            return Ok(());
        }
        self.notify("textDocument/didClose", json!({"textDocument": {"uri": u}}))
    }

    /// Where the server's reports stand now, for a later settle: what came after it is the answer to what was sent.
    pub fn mark(&self) -> Mark {
        let g = self.shared.0.lock().unwrap();
        Mark { published: g.published, ended: g.ended }
    }

    /// Wait until the server has finished the check of what was sent after SINCE, and then QUIET has passed with no
    /// new diagnostics. agda-ls reports each check as $/progress work: there a check is finished when a work that
    /// ended after SINCE ended and none is in progress, on no clock (once it has reported work at all). The others: when no work is in progress
    /// and something was published after SINCE (or nothing came at all within 4 × QUIET: the edit changed no
    /// diagnostics). Returns false if the server ended.
    pub fn settle(&self, since: Mark, quiet: Duration) -> Result<bool> {
        let (lock, cv) = &*self.shared;
        let start = Instant::now();
        let what = format!("the {} server's check", self.language);
        let mut g = lock.lock().unwrap();
        loop {
            let now = Instant::now();
            if g.closed {
                return Ok(false);
            }
            if CANCELLED.load(Ordering::SeqCst) {
                ended(&self.language, &g, start, &what)?;
            }
            let calm = g.busy.is_empty() && g.last_publish.is_none_or(|t| now.duration_since(t) >= quiet);
            if self.language == "agda" && g.reports_work {
                if calm && g.ended > since.ended {
                    ended(&self.language, &g, start, &what)?;
                    return Ok(true);
                }
            } else {
                let heard = g.published > since.published;
                // something came and all is quiet; or nothing at all came for a long while (the edit changed no diagnostics)
                if calm && (heard || now.duration_since(start) >= quiet * 4) {
                    ended(&self.language, &g, start, &what)?;
                    return Ok(true);
                }
            }
            if deadline().is_some_and(|d| now >= d) {
                return Err(cut(&self.language, &g, start, &what));
            }
            g = cv.wait_timeout(g, Duration::from_millis(200)).unwrap().0;
        }
    }

    /// Whether its output has ended: the child gone, or the shared server's connection closed (it was restarted).
    pub fn closed(&self) -> bool {
        self.shared.0.lock().unwrap().closed
    }

    /// Where it is: its socket, or none for a child.
    pub fn socket(&self) -> Option<&Path> {
        match &self.conn {
            Conn::Socket(_, p) => Some(p),
            Conn::Child(..) => None,
        }
    }

    pub fn diagnostics(&self) -> HashMap<String, Vec<Value>> {
        self.shared.0.lock().unwrap().diagnostics.clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        match &mut self.conn {
            Conn::Child(child, _) => {
                // whether it had ended on its own, and how, before it is stopped
                if let Ok(Some(st)) = child.try_wait() {
                    eprintln!("symbols: the {} server had ended: {st}", self.language);
                }
                self.link.stop();
                let _ = child.wait();
            }
            Conn::Socket(stream, path) => {
                // shutdown and exit end this client's session only; the server stays for the others. Its answer to
                // them is not waited for beyond a moment: the connection is closed either way
                self.link.stop();
                let (lock, cv) = &*self.shared;
                let g = lock.lock().unwrap();
                let _ = cv.wait_timeout_while(g, Duration::from_secs(2), |g| !g.closed);
                let _ = stream.shutdown(std::net::Shutdown::Both);
                eprintln!("symbols: the {} server: disconnected from {}", self.language, path.display());
            }
        }
    }
}

/// Where a server's reports stood (Server::mark): how many publishes and how many ends of work had come.
#[derive(Clone, Copy, Default)]
pub struct Mark {
    published: u64,
    ended: u64,
}

/// A second handle on a pipe's descriptor (the reader answers the server's own requests; Link writes when stopping).
fn dup_writer(stdin: &ChildStdin) -> Result<Writer> {
    use std::os::fd::{AsRawFd, FromRawFd};
    // SAFETY: a duplicate of our own pipe's descriptor, owned by the Writer from here on
    let dup = unsafe { libc::dup(stdin.as_raw_fd()) };
    if dup < 0 {
        bail!("dup failed");
    }
    Ok(Writer::new(unsafe { std::fs::File::from_raw_fd(dup) }))
}

/// The one way to write to a server. Every frame, ours and the reader's answers to the server's own requests
/// (agda-ls's window/workDoneProgress/create, HLS's registrations), goes whole into a queue, and one thread writes
/// them in order. So no frame is cut by another, and nothing that sends waits on the pipe.
///
/// Both matter (docket-core2's stdlib runs, N-158). agda-ls reads nothing while it indexes. Once our writes filled
/// its pipe, a writer waiting on the pipe while the reader needed to answer agda-ls stopped the reader; agda-ls then
/// blocked on its own output and never read again. With a lock for each, the reader's answer went into the middle
/// of our half-written frame instead: agda-ls read a broken body ("bad message") and lost its framing.
struct Writer(Mutex<std::sync::mpsc::Sender<Vec<u8>>>);

impl Writer {
    fn new(mut f: std::fs::File) -> Writer {
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            for frame in rx {
                if let Err(e) = f.write_all(&frame).and_then(|_| f.flush()) {
                    // the server's input is closed: what is queued after this is dropped, and each send says so
                    eprintln!("symbols: writing to the server: {e}");
                    break;
                }
            }
        });
        Writer(Mutex::new(tx))
    }

    fn send(&self, msg: &Value) -> Result<()> {
        let body = serde_json::to_vec(msg)?;
        let mut frame = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
        frame.extend_from_slice(&body);
        self.0.lock().unwrap().send(frame).map_err(|_| anyhow!("the server's input is closed"))
    }
}

fn read_loop(mut r: BufReader<impl Read>, shared: Arc<(Mutex<Shared>, Condvar)>, w: Arc<Writer>, lang: String) {
    let (lock, cv) = &*shared;
    loop {
        let msg = match read_frame(&mut r) {
            Frame::End(why) => {
                eprintln!("symbols: the {lang} server's output ended ({why})");
                break;
            }
            Frame::Bad(why) => {
                // one frame that cannot be read is skipped: it does not end the server
                eprintln!("symbols: a frame from the {lang} server skipped: {why}");
                continue;
            }
            Frame::Msg(m) => m,
        };
        let method = msg.get("method").and_then(Value::as_str).map(str::to_string);
        match (msg.get("id"), method) {
            // the server asks: configuration, progress tokens, registrations; answered at once
            (Some(id), Some(m)) => {
                let result = match m.as_str() {
                    "workspace/configuration" => {
                        let n = msg.pointer("/params/items").and_then(Value::as_array).map(Vec::len).unwrap_or(1);
                        let s = settings(&lang);
                        let one = s.get("haskell").cloned().unwrap_or(s);
                        Value::Array(vec![one; n])
                    }
                    "workspace/workspaceFolders" => json!([]),
                    _ => Value::Null,
                };
                if let Err(e) = w.send(&json!({"jsonrpc": "2.0", "id": id, "result": result})) {
                    eprintln!("symbols: answering the {lang} server's {m}: {e}");
                }
            }
            (Some(id), None) => {
                if let Some(n) = id.as_u64() {
                    let mut g = lock.lock().unwrap();
                    if g.load_asks.remove(&n) {
                        took_load(&lang, &mut g, &msg);
                    } else {
                        g.answers.insert(n, msg);
                    }
                    cv.notify_all();
                }
            }
            (None, Some(m)) if m == "tovrin/loading" => {
                let mut g = lock.lock().unwrap();
                if let Some(l) = msg.get("params").and_then(|p| Load::of(p, true)) {
                    if g.load.is_none_or(|o| !o.started) {
                        eprintln!("symbols: the {lang} server begins loading {} files", l.total);
                    }
                    if l.ready && g.load.is_some_and(|o| !o.ready) {
                        eprintln!("symbols: the {lang} server has loaded its {} files", l.total);
                    }
                    g.load = Some(l);
                    g.last_load = Some(Instant::now());
                }
                cv.notify_all();
            }
            (None, Some(m)) if m == "textDocument/publishDiagnostics" => {
                let u = msg.pointer("/params/uri").and_then(Value::as_str).unwrap_or("").to_string();
                let d = msg.pointer("/params/diagnostics").and_then(Value::as_array).cloned().unwrap_or_default();
                let mut g = lock.lock().unwrap();
                g.diagnostics.insert(u, d);
                g.published += 1;
                g.last_publish = Some(Instant::now());
                cv.notify_all();
            }
            (None, Some(m)) if m == "$/progress" => {
                let token = msg.pointer("/params/token").map(|t| t.to_string()).unwrap_or_default();
                let kind = msg.pointer("/params/value/kind").and_then(Value::as_str).unwrap_or("");
                let mut g = lock.lock().unwrap();
                g.last_progress = Some(Instant::now());
                // in symbols' log: a check that does not end leaves its token here, and settle waits on it
                match kind {
                    "begin" => {
                        let title = msg.pointer("/params/value/title").and_then(Value::as_str).unwrap_or("");
                        let message = msg.pointer("/params/value/message").and_then(Value::as_str).unwrap_or("").to_string();
                        eprintln!("symbols: the {lang} server begins {token} {title:?}");
                        g.busy.insert(token, Work { title: title.to_string(), message, percentage: msg.pointer("/params/value/percentage").and_then(Value::as_u64), since: Instant::now() });
                        g.reports_work = true;
                    }
                    "end" => {
                        eprintln!("symbols: the {lang} server ends {token}");
                        g.busy.remove(&token);
                        g.ended += 1;
                    }
                    "report" => {
                        if let Some(w) = g.busy.get_mut(&token) {
                            if let Some(m) = msg.pointer("/params/value/message").and_then(Value::as_str) {
                                w.message = m.to_string();
                            }
                            if let Some(p) = msg.pointer("/params/value/percentage").and_then(Value::as_u64) {
                                w.percentage = Some(p);
                            }
                        }
                    }
                    _ => {}
                }
                cv.notify_all();
            }
            _ => {}
        }
    }
    lock.lock().unwrap().closed = true;
    cv.notify_all();
}

/// The answer MSG to a `tovrin/loadState`: the load as it is now; an error (a server without it) leaves none.
fn took_load(lang: &str, g: &mut Shared, msg: &Value) {
    match msg.get("result").and_then(|r| Load::of(r, false)) {
        Some(l) => {
            eprintln!("symbols: the {lang} server's load: {} of {} files{}", l.done, l.total, if l.ready { ", ready" } else if l.started { "" } else { ", not begun" });
            g.load = Some(l);
        }
        None => {}
    }
}

enum Frame {
    Msg(Value),
    /// A frame that could not be read; the stream goes on after it.
    Bad(String),
    /// The server's output closed (or could not be read at all).
    End(String),
}

fn read_frame(r: &mut BufReader<impl Read>) -> Frame {
    let mut len = None;
    loop {
        let mut line = Vec::new();
        match r.read_until(b'\n', &mut line) {
            Ok(0) => return Frame::End("end of file".into()),
            Ok(_) => {}
            Err(e) => return Frame::End(e.to_string()),
        }
        let l = String::from_utf8_lossy(&line);
        let l = l.trim_end();
        if l.is_empty() {
            if len.is_some() {
                break;
            }
            // a blank line before any header: between frames
            continue;
        }
        if let Some(v) = l.to_ascii_lowercase().strip_prefix("content-length:") {
            len = v.trim().parse::<usize>().ok();
            if len.is_none() {
                return Frame::Bad(format!("a header line {l:?}"));
            }
        }
    }
    let mut buf = vec![0; len.unwrap()];
    if let Err(e) = r.read_exact(&mut buf) {
        return Frame::End(e.to_string());
    }
    match serde_json::from_slice(&buf) {
        Ok(v) => Frame::Msg(v),
        Err(e) => Frame::Bad(format!("{} bytes that are not JSON ({e}): {:?}", buf.len(), String::from_utf8_lossy(&buf[..buf.len().min(200)]))),
    }
}

pub fn uri(p: &Path) -> String {
    let mut s = String::from("file://");
    for b in p.to_string_lossy().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => s.push(b as char),
            _ => s.push_str(&format!("%{b:02X}")),
        }
    }
    s
}

pub fn path_of(u: &str) -> PathBuf {
    let raw = u.strip_prefix("file://").unwrap_or(u);
    let mut out = Vec::new();
    let b = raw.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(x) = u8::from_str_radix(&raw[i + 1..i + 3], 16) {
                out.push(x);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    PathBuf::from(String::from_utf8_lossy(&out).into_owned())
}

/// A position (line, character in the agreed encoding) as a byte offset in TEXT.
pub fn offset(text: &str, line: u64, ch: u64, utf8: bool) -> usize {
    let mut start = 0;
    for _ in 0..line {
        match text[start..].find('\n') {
            Some(i) => start += i + 1,
            None => return text.len(),
        }
    }
    let end = text[start..].find('\n').map(|i| start + i).unwrap_or(text.len());
    let l = &text[start..end];
    if utf8 {
        return start + (ch as usize).min(l.len());
    }
    let mut units = 0u64;
    for (i, c) in l.char_indices() {
        if units >= ch {
            return start + i;
        }
        units += c.len_utf16() as u64;
    }
    end
}

/// A byte offset in TEXT as a position.
pub fn position(text: &str, off: usize, utf8: bool) -> (u64, u64) {
    let before = &text[..off.min(text.len())];
    let line = before.matches('\n').count() as u64;
    let ls = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let l = &before[ls..];
    let ch = if utf8 { l.len() as u64 } else { l.chars().map(|c| c.len_utf16() as u64).sum() };
    (line, ch)
}

/// A range's start and end as byte offsets.
pub fn span<R: Ends + ?Sized>(text: &str, range: &R, utf8: bool) -> (usize, usize) {
    let ((sl, sc), (el, ec)) = (range.start(), range.end());
    (offset(text, sl, sc, utf8), offset(text, el, ec, utf8))
}

/// A range's two ends as (line, character): a server's range as it sent it (`{"start": {"line", "character"},
/// "end": …}`, a missing part 0) or one kept as `Rng`.
pub trait Ends {
    fn start(&self) -> (u64, u64);
    fn end(&self) -> (u64, u64);
}

impl Ends for Value {
    fn start(&self) -> (u64, u64) {
        let g = |p: &str| self.pointer(p).and_then(Value::as_u64).unwrap_or(0);
        (g("/start/line"), g("/start/character"))
    }
    fn end(&self) -> (u64, u64) {
        let g = |p: &str| self.pointer(p).and_then(Value::as_u64).unwrap_or(0);
        (g("/end/line"), g("/end/character"))
    }
}

/// A range as the index keeps it: four numbers, not the server's JSON. Kept as `Value`, a range took about 2 KB
/// (three maps with their keys), and a view's 35000 symbols with their five ranges each about 370 MB in every agent's
/// symbols (paper-research N-101 p1565, p1569); as `Rng`, 16 bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rng {
    pub start_line: u32,
    pub start_char: u32,
    pub end_line: u32,
    pub end_char: u32,
}

impl Rng {
    /// The server's range V (a part it does not give: 0, as `span` reads one).
    pub fn of(v: &Value) -> Rng {
        let n = |x: u64| u32::try_from(x).unwrap_or(u32::MAX);
        let ((sl, sc), (el, ec)) = (v.start(), v.end());
        Rng { start_line: n(sl), start_char: n(sc), end_line: n(el), end_char: n(ec) }
    }
}

impl Ends for Rng {
    fn start(&self) -> (u64, u64) {
        (self.start_line.into(), self.start_char.into())
    }
    fn end(&self) -> (u64, u64) {
        (self.end_line.into(), self.end_char.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two threads sending big frames through one Writer (as a request and the reader's answer do) leave a stream
    /// that reads back frame by frame, every body whole.
    #[test]
    fn frames_from_two_threads_do_not_interleave() {
        let path = std::env::temp_dir().join(format!("symbols-frames-{}", std::process::id()));
        let w = Arc::new(Writer::new(std::fs::File::create(&path).unwrap()));
        let ts: Vec<_> = (0..2)
            .map(|k| {
                let w = w.clone();
                std::thread::spawn(move || {
                    for i in 0..200 {
                        w.send(&json!({"k": k, "i": i, "pad": "é".repeat(5000)})).unwrap();
                    }
                })
            })
            .collect();
        for t in ts {
            t.join().unwrap();
        }
        // the queue is written by its thread: wait until all of it is in the file
        let want = 400;
        let mut bytes = Vec::new();
        for _ in 0..500 {
            bytes = std::fs::read(&path).unwrap();
            if bytes.windows(4).filter(|x| x == b"\r\n\r\n").count() >= want {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = std::fs::remove_file(&path);
        let (mut at, mut n) = (0, 0);
        while at < bytes.len() {
            let end = at + bytes[at..].windows(4).position(|x| x == b"\r\n\r\n").unwrap();
            let len: usize = std::str::from_utf8(&bytes[at..end]).unwrap().strip_prefix("Content-Length: ").unwrap().parse().unwrap();
            let v: Value = serde_json::from_slice(&bytes[end + 4..end + 4 + len]).unwrap();
            assert_eq!(v["pad"].as_str().unwrap().chars().count(), 5000);
            at = end + 4 + len;
            n += 1;
        }
        assert_eq!(n, want);
    }

    /// A server that reads nothing (agda-ls while it indexes): sending more than its pipe holds does not block, so
    /// the reader can still answer it.
    #[test]
    fn a_server_that_reads_nothing_does_not_block_a_send() {
        use std::os::fd::FromRawFd;
        let mut fds = [0; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let w = Writer::new(unsafe { std::fs::File::from_raw_fd(fds[1]) });
        let start = Instant::now();
        for i in 0..64 {
            w.send(&json!({"i": i, "pad": "x".repeat(16_000)})).unwrap(); // 1 MB in all, a pipe holds 64 KB
        }
        w.send(&json!({"id": "agda-ls/create/1", "result": null})).unwrap();
        assert!(start.elapsed() < Duration::from_secs(2));
        unsafe { libc::close(fds[0]) };
    }

    /// A server still checking at the call's deadline (agda-ls checking a library cold): the wait is cut, saying
    /// what the server reported, and the server goes on; a wait after it with a later deadline sees the check end.
    #[test]
    fn a_wait_cut_at_the_deadline_is_taken_up_again() {
        let dir = std::env::temp_dir().join(format!("symbols-cut-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("fake.py");
        std::fs::write(&fake, r#"
import sys, json, time, threading
o = sys.stdout.buffer
def send(m):
    b = json.dumps(m).encode()
    o.write(b"Content-Length: %d\r\n\r\n" % len(b) + b); o.flush()
def read():
    n = 0
    while True:
        l = sys.stdin.buffer.readline()
        if not l: return None
        if l.strip() == b"": break
        if l.lower().startswith(b"content-length:"): n = int(l.split(b":")[1])
    return json.loads(sys.stdin.buffer.read(n))
def work():
    send({"jsonrpc": "2.0", "method": "$/progress", "params": {"token": "t1", "value": {"kind": "begin", "title": "Indexing", "message": "12 of 1187 modules"}}})
    time.sleep(0.5)
    send({"jsonrpc": "2.0", "method": "$/progress", "params": {"token": "t1", "value": {"kind": "report", "message": "412 of 1187 modules", "percentage": 34}}})
    time.sleep(2.5)
    send({"jsonrpc": "2.0", "method": "$/progress", "params": {"token": "t1", "value": {"kind": "end"}}})
while True:
    m = read()
    if m is None: break
    if "id" in m and "method" in m:
        send({"jsonrpc": "2.0", "id": m["id"], "result": {"capabilities": {}} if m["method"] == "initialize" else None})
    if m.get("method") == "initialized":
        threading.Thread(target=work, daemon=True).start()
"#)
        .unwrap();
        let s = Server::start("agda", "agda", &["python3".to_string(), fake.display().to_string()], &dir).unwrap();
        std::thread::sleep(Duration::from_millis(800));
        let start = Instant::now();
        set_deadline(Some(start + Duration::from_millis(500)));
        let e = s.settle(Mark::default(), Duration::from_millis(100)).unwrap_err();
        let cut = still_working(&e).expect("cut at the deadline").to_string();
        assert!(start.elapsed() < Duration::from_secs(2), "cut late: {:?}", start.elapsed());
        assert!(cut.contains("Indexing: 412 of 1187 modules (34%)"), "{cut}");
        set_deadline(Some(Instant::now() + Duration::from_secs(20)));
        assert!(s.settle(Mark::default(), Duration::from_millis(100)).unwrap());
        set_deadline(None);
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// hs/index cut at the deadline (808 s cold on Tovrin's checker) is not sent again: the next call waits on the
    /// same request's answer, and only then is the server taken as indexed.
    #[test]
    fn a_cut_hs_index_is_waited_on_not_sent_again() {
        let dir = std::env::temp_dir().join(format!("symbols-index-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("fake.py");
        std::fs::write(&fake, r#"
import sys, json, time, threading
o = sys.stdout.buffer; lock = threading.Lock(); seen = [0]
def send(m):
    b = json.dumps(m).encode()
    with lock:
        o.write(b"Content-Length: %d\r\n\r\n" % len(b) + b); o.flush()
def read():
    n = 0
    while True:
        l = sys.stdin.buffer.readline()
        if not l: return None
        if l.strip() == b"": break
        if l.lower().startswith(b"content-length:"): n = int(l.split(b":")[1])
    return json.loads(sys.stdin.buffer.read(n))
def later(i):
    time.sleep(1.5); send({"jsonrpc": "2.0", "id": i, "result": {"indexed": 2}})
while True:
    m = read()
    if m is None: break
    if "id" not in m or "method" not in m: continue
    if m["method"] == "hs/index":
        seen[0] += 1; threading.Thread(target=later, args=(m["id"],), daemon=True).start()
    elif m["method"] == "count":
        send({"jsonrpc": "2.0", "id": m["id"], "result": seen[0]})
    else:
        send({"jsonrpc": "2.0", "id": m["id"], "result": {"capabilities": {}} if m["method"] == "initialize" else None})
"#)
        .unwrap();
        let s = Server::start("haskell", "haskell", &["python3".to_string(), fake.display().to_string()], &dir).unwrap();
        let f = dir.join("A.hs");
        set_deadline(Some(Instant::now() + Duration::from_millis(300)));
        let e = s.index_once(&f).unwrap_err();
        assert!(still_working(&e).is_some(), "{e}");
        set_deadline(Some(Instant::now() + Duration::from_secs(20)));
        s.index_once(&f).unwrap();
        s.index_once(&f).unwrap();
        assert_eq!(s.request("count", json!({})).unwrap(), json!(1));
        set_deadline(None);
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The load as the server says it (tovrin/loadState at connect, then tovrin/loading): a wait cut at the deadline
    /// begins "still loading: N of M files" until it is done (N-233 p142/p166).
    #[test]
    fn a_cut_wait_says_how_far_the_load_is() {
        let dir = std::env::temp_dir().join(format!("symbols-load-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("fake.py");
        std::fs::write(&fake, r#"
import sys, json
o = sys.stdout.buffer
def send(m):
    b = json.dumps(m).encode()
    o.write(b"Content-Length: %d\r\n\r\n" % len(b) + b); o.flush()
def read():
    n = 0
    while True:
        l = sys.stdin.buffer.readline()
        if not l: return None
        if l.strip() == b"": break
        if l.lower().startswith(b"content-length:"): n = int(l.split(b":")[1])
    return json.loads(sys.stdin.buffer.read(n))
def loading(d): send({"jsonrpc": "2.0", "method": "tovrin/loading", "params": {"done": d, "total": 602}})
while True:
    m = read()
    if m is None: break
    k = m.get("method")
    if k == "tovrin/loadState":
        send({"jsonrpc": "2.0", "id": m["id"], "result": {"done": 3, "total": 602, "ready": False}})
    elif k == "more": loading(40)
    elif k == "all": loading(602)
    elif k == "slow": pass
    elif "id" in m:
        send({"jsonrpc": "2.0", "id": m["id"], "result": {"capabilities": {}} if k == "initialize" else None})
"#)
        .unwrap();
        let s = Server::start("agda", "agda", &["python3".to_string(), fake.display().to_string()], &dir).unwrap();
        let cut_text = |s: &Server| {
            set_deadline(Some(Instant::now() + Duration::from_millis(400)));
            let e = s.request("slow", json!({})).unwrap_err();
            set_deadline(None);
            still_working(&e).expect("cut at the deadline").to_string()
        };
        let first = cut_text(&s);
        assert!(first.starts_with("agda is still loading: 3 of 602 files; this call waited"), "{first}");
        assert!(s.loading(), "3 of 602: a definition answers as written");
        s.notify("more", json!({})).unwrap();
        let more = cut_text(&s);
        assert!(more.starts_with("agda is still loading: 40 of 602 files"), "{more}");
        s.notify("all", json!({})).unwrap();
        let done = cut_text(&s);
        assert!(done.starts_with("agda reports no work in progress"), "{done}");
        assert!(!s.loading(), "all loaded: a generalised type is the server's again");
        // a wait the load held: its note says so, not "no work" (f1634-1's 451 s, N-101 p1447)
        take_notes();
        let start = Instant::now() - SILENT;
        // the loads above came within these 10 s: as if they had come before it
        s.shared.0.lock().unwrap().last_load = None;
        let g = s.shared.0.lock().unwrap();
        let note = silence("agda", &g, start);
        drop(g);
        assert_eq!(note.as_deref(), Some("agda reported no work for 10 s before this"), "no load during it: silent");
        s.notify("more", json!({})).unwrap();
        s.notify("all", json!({})).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let g = s.shared.0.lock().unwrap();
        let note = silence("agda", &g, start).unwrap();
        drop(g);
        assert!(note.starts_with("this waited 1") && note.ends_with("s for agda to load its 602 files; the next call does not wait for it"), "{note}");
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
