//! An opencode account. coordd runs opencode's own sign-in, `opencode auth login`, in the account's folders, and then
//! asks opencode what it keeps, `opencode auth list` (opencode 1.18.34, `packages/opencode/src/cli/cmd/providers.ts`).
//!
//! - **The program** is the pinned opencode in the coord's own folder ([`program`]), never the owner's own install; it
//!   runs nothing whose sha256 is not [`SHA256`].
//! - **Its folders** are under the account's `home/` ([`Store::home`]). That folder is its `HOME`, and its XDG folders
//!   are inside it ([`folder_env`]). Its environment is cleared except for those, `PATH`, `TERM` and `LANG`. opencode
//!   keeps the sign-in in `home/data/opencode/auth.json`, and coordd reads nothing under `home/`.
//! - **Its terminal** is one of its own ([`Run`]), because `auth login` asks through @clack/prompts. Only coordd holds
//!   its master end; once the run ends nothing of the screen is kept but a failure's words.
//! - **A key** is what the owner pasted into the masked `API key` field (`Request::AccountKeyStore`). It is written into
//!   that terminal once, with Enter, only when opencode's prompt is exactly its masked `Enter your API key` and the
//!   terminal echoes nothing ([`Run::type_key`]); otherwise the run ends in words. The bytes are overwritten after. No
//!   argument, environment variable or stdin carries a key. An OpenCode Go account's key is also kept, to read Go's
//!   usage with and to send its plain calls ([`keep_usage_key`], [`super::plain`]), and for nothing else.
//! - **A browser sign-in** (`auth login -p PROVIDER -m METHOD`) prints `Go to: URL` ([`Run::link`]); the window opens it
//!   only when it is `https` on a host in `coord_types::OPENCODE_SIGN_IN_HOSTS`. A pasted code goes only into opencode's
//!   `Paste the authorization code here:` ([`Run::paste`]). The ChatGPT way needs Codex's callback port; while another
//!   program holds it, nothing starts ([`codex_port_free`]).
//! - **Done** is what `opencode auth list` says afterwards ([`status`]). The exit code of `auth login` says nothing:
//!   after "Failed to authorize" it still prints "Done" and exits 0.
//! - **Its models** are what `opencode models PROVIDER` names at each check while signed in ([`models`]), kept in its
//!   state and served by the router.
//! - **Go's usage** of an OpenCode Go account is read from Go's usage page ([`go_usage`]) with the key kept for it, as
//!   its 5-hour, weekly and monthly windows ([`from_go_usage`]).
//!
//! No time limit is ours. A run lasts as long as opencode's process lives, and the owner's Cancel ends it.

use super::limits::{self, Said, Verdict};
use super::signin::{Ended, how, last_words, plain};
use super::{Harness, State, Store, Window};
use coord_types::Secret;
use std::ffi::OsString;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::unix::AsyncFd;
use tokio::sync::watch;

/// The opencode these runs were read against and the coord's folder holds ([`program`]).
pub const VERSION: &str = "1.18.34";

/// The sha256 of that release's `opencode` (linux x64), pinned here, so a binary and hash file both changed in the
/// folder are still refused.
pub const SHA256: &str = "9ca0b9953d49997601655e54f846a3efa464f237e47c6f1b04716d0f2e64c4c2";

/// The folders under an account's `home/` (`Store::layout`): opencode's XDG data, config, state and cache, and its
/// TMPDIR.
pub const HOME_DIRS: [&str; 5] = ["data", "config", "state", "cache", "tmp"];

/// opencode's masked prompt for a key (`Prompt.password`, providers.ts), the only one a key is written into.
pub const KEY_PROMPT: &str = "Enter your API key";

/// opencode's prompt for the code a browser shows (`Prompt.text`), the only one a pasted code is written into.
pub const CODE_PROMPT: &str = "Paste the authorization code here:";

/// Codex's own callback port. opencode's [`ON_CODEX_PORT`] way listens on it for the browser's redirect.
pub const CODEX_PORT: u16 = 1455;

/// The sign-in way of opencode's `openai` that listens on [`CODEX_PORT`] (codex.ts).
pub const ON_CODEX_PORT: &str = "ChatGPT Pro/Plus (browser)";

/// The terminal's size: wide enough that no prompt's line is wrapped.
const ROWS: u16 = 50;
const COLS: u16 = 200;

/// The pinned opencode of the coord at COORD: `COORD/opt/opencode/1.18.34/bin/opencode`, never the owner's. Its sha256
/// is checked before every run against [`SHA256`]; an Err in words when it is missing or another binary. It reads the
/// whole file (185 MB): call it off the async threads.
pub fn program(coord: &Path) -> Result<PathBuf, String> {
    pinned(&coord.join(format!("opt/opencode/{VERSION}/bin/opencode")), SHA256)
}

/// P, when it is an executable file whose sha256 is SHA; else why not, in words.
fn pinned(p: &Path, sha: &str) -> Result<PathBuf, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(p) {
        Ok(m) if m.is_file() && m.permissions().mode() & 0o111 != 0 => {}
        Ok(_) => return Err(format!("opencode: {} is not a program", p.display())),
        Err(_) => return Err(format!("opencode {VERSION} is not installed for this coord ({} is not there)", p.display())),
    }
    let unread = |e: std::io::Error| format!("opencode: {} could not be read to check it ({e}); it is not run", p.display());
    let mut f = std::fs::File::open(p).map_err(unread)?;
    let (mut h, mut buf) = (Sha256::new(), vec![0u8; 1 << 20]);
    loop {
        match f.read(&mut buf).map_err(unread)? {
            0 => break,
            n => h.update(&buf[..n]),
        }
    }
    let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if got != sha {
        return Err(format!("opencode: {} is not the opencode {VERSION} that was fetched (its sha256 is {got}, not {sha}); it is not run", p.display()));
    }
    Ok(p.to_path_buf())
}

/// Why PROVIDER cannot be passed as opencode's `-p` (an id is letters, digits, `-`, `_` and `.`). Whether opencode
/// knows it is opencode's to say: it ends with `Unknown provider` then.
pub fn bad_provider(provider: &str) -> Option<String> {
    let ok = !provider.is_empty() && provider.len() <= 64 && !provider.starts_with('-') && provider.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    (!ok).then(|| format!("{provider:?} is not a provider's id (letters, digits, - _ and .)"))
}

/// What opencode is given, beside its own, on the account whose folder is DIR (`ROOT/opencode/ID`, or the folder as
/// it was set aside): its XDG folders and TMPDIR under DIR's `home/`. The other three keep out its project's config,
/// the owner's `~/.claude`, and autoupdate, which would rewrite the pinned binary.
pub fn folder_env(dir: &Path) -> Vec<(String, String)> {
    let home = dir.join("home");
    let at = |d: &str| home.join(d).display().to_string();
    vec![
        ("XDG_DATA_HOME".into(), at("data")),
        ("XDG_CONFIG_HOME".into(), at("config")),
        ("XDG_STATE_HOME".into(), at("state")),
        ("XDG_CACHE_HOME".into(), at("cache")),
        ("TMPDIR".into(), at("tmp")),
        ("OPENCODE_DISABLE_PROJECT_CONFIG".into(), "1".into()),
        ("OPENCODE_DISABLE_CLAUDE_CODE".into(), "1".into()),
        ("OPENCODE_DISABLE_AUTOUPDATE".into(), "1".into()),
    ]
}

/// The whole environment of a run coordd makes on the account whose folder is DIR. It has nothing of coordd's but
/// `PATH` and `LANG`: `HOME` is DIR's `home/`, `TERM` a terminal's, plus [`folder_env`].
fn run_env(dir: &Path) -> Vec<(OsString, OsString)> {
    let mut env: Vec<(OsString, OsString)> = vec![
        ("HOME".into(), dir.join("home").into()),
        ("PATH".into(), std::env::var_os("PATH").unwrap_or_else(|| "/usr/local/bin:/usr/bin:/bin".into())),
        ("TERM".into(), "xterm-256color".into()),
        ("LANG".into(), std::env::var_os("LANG").filter(|l| !l.is_empty()).unwrap_or_else(|| "C.UTF-8".into())),
    ];
    env.extend(folder_env(dir).into_iter().map(|(k, v)| (k.into(), v.into())));
    env
}

/// What the terminal showed, as read so far.
#[derive(Default)]
struct Screen {
    text: String,
    /// The end of a character split between two reads.
    pending: Vec<u8>,
    /// The codes pasted into it ([`Run::paste`]), which opencode's text prompt shows as typed: kept only to take them
    /// out of what is shown, as Claude Code's sign-in does (signin.rs).
    pasted: Vec<String>,
}

impl Screen {
    /// The text as it may be shown: escape sequences taken out, and any pasted code replaced.
    fn shown(&self) -> String {
        self.pasted.iter().fold(plain(&self.text), |t, code| t.replace(code.as_str(), "[the pasted code]"))
    }

    fn push(&mut self, bytes: &[u8], more: bool) {
        self.pending.extend_from_slice(bytes);
        let whole = match std::str::from_utf8(&self.pending) {
            Err(e) if more && e.error_len().is_none() => e.valid_up_to(),
            _ => self.pending.len(),
        };
        let piece = String::from_utf8_lossy(&self.pending[..whole]).into_owned();
        self.pending.drain(..whole);
        self.text.push_str(&piece);
    }
}

/// One run of opencode's `auth login` on a terminal of its own. Every method takes `&self`, so the window's requests
/// (the key, a code, cancel) can come from anywhere while it runs. Dropping the handle before opencode ended ends it.
pub struct Run {
    /// opencode's process. It leads a session of its own, and a cancel ends its process group.
    pid: i32,
    /// The terminal's master end, which only this handle and its reader hold.
    tty: Arc<AsyncFd<OwnedFd>>,
    screen: Arc<Mutex<Screen>>,
    /// Raised at each read, so that a wait for a prompt or a link looks again.
    seen: watch::Receiver<u64>,
    end: watch::Receiver<Option<Ended>>,
    /// Nothing more is written: cancelled.
    closed: AtomicBool,
    /// Its process is reaped: raised the moment the wait returns, before its end is told, so no signal goes to a group
    /// id another process may have by then.
    reaped: Arc<AtomicBool>,
}

impl Run {
    /// Start PROGRAM with ARGS (`auth login -p PROVIDER [-m METHOD]`) on the account whose folder is DIR, on a new
    /// terminal whose only other end is ours, in a session of its own. Must be called inside a tokio runtime.
    pub fn start(program: &Path, dir: &Path, args: &[&str]) -> Result<Run, String> {
        let (master, slave) = open_pty()?;
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(args).env_clear().envs(run_env(dir)).current_dir(dir.join("home"));
        let io = |fd: &OwnedFd| fd.try_clone().map(Stdio::from).map_err(|e| format!("opencode: its terminal: {e}"));
        cmd.stdin(io(&slave)?).stdout(io(&slave)?).stderr(io(&slave)?);
        // SAFETY: between fork and exec only async-signal-safe calls: a session of its own, with the terminal (its
        // stdin by now) as its controlling one
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let spawned = cmd.spawn();
        // the terminal's other end is opencode's alone: ours are closed, so that its end is read as one
        drop(cmd);
        drop(slave);
        let mut child = spawned.map_err(|e| format!("opencode: {} {}: {e}", program.display(), args.join(" ")))?;
        let pid = child.id().map(|p| p as i32).ok_or("opencode: it ended before it began")?;
        nonblocking(&master)?;
        // SAFETY: the master end is ours alone (opencode has only the slave end) and lives inside the AsyncFd until it
        // is dropped with it
        let tty = Arc::new(unsafe { AsyncFd::register(master) }.map_err(|e| format!("opencode: its terminal: {e}"))?);
        let screen = Arc::new(Mutex::new(Screen::default()));
        let (seen_tx, seen) = watch::channel(0u64);
        let (end_tx, end) = watch::channel(None);
        let (t, sc) = (tty.clone(), screen.clone());
        let reaped = Arc::new(AtomicBool::new(false));
        let gone = reaped.clone();
        tokio::spawn(async move {
            let (mut status, mut open) = (None, true);
            while status.is_none() {
                tokio::select! {
                    s = child.wait() => {
                        gone.store(true, Ordering::SeqCst);
                        status = Some(s);
                    }
                    more = read_some(&t, &sc), if open => {
                        open = more;
                        seen_tx.send_modify(|n| *n += 1);
                    }
                }
            }
            // what it printed last, still in the terminal: read until nothing is left
            if open {
                drain(&t, &sc);
            }
            let shown = {
                let mut s = sc.lock().unwrap();
                s.push(&[], false);
                s.shown()
            };
            let ended = match status.expect("set by the loop") {
                Ok(s) if s.success() => Ended::Ok,
                Ok(s) => Ended::Failed(last_words(&shown, &how(s))),
                Err(e) => Ended::Failed(last_words(&shown, &format!("could not be waited for: {e}"))),
            };
            let _ = end_tx.send(Some(ended));
            seen_tx.send_modify(|n| *n += 1);
        });
        Ok(Run { pid, tty, screen, seen, end, closed: AtomicBool::new(false), reaped })
    }

    /// opencode's process, the leader of its own session: for whoever started it to put it in a cgroup of its own.
    pub fn pid(&self) -> i32 {
        self.pid
    }

    /// What the terminal showed so far, escape sequences taken out. Never holds a key, which is written only where
    /// the terminal does not echo ([`Run::type_key`]).
    pub fn shown(&self) -> String {
        self.screen.lock().unwrap().shown()
    }

    /// The prompt opencode waits on now, once it waits on one. None when it ended first: its words are then in
    /// [`Run::wait`].
    pub async fn prompt(&self) -> Option<String> {
        loop {
            if self.end.borrow().is_some() {
                return None;
            }
            if let Some(p) = current_prompt(&self.shown()) {
                return Some(p);
            }
            self.changed().await;
        }
    }

    /// The link opencode says to open (`Go to: URL`), and the line it printed after it, its instructions (`Enter code:
    /// …`), once both are printed. An Err with its words when it ended without one.
    pub async fn link(&self) -> Result<(String, Option<String>), String> {
        loop {
            let ended = self.end.borrow().is_some();
            if let Some(l) = go_to(&self.shown(), ended) {
                return Ok(l);
            }
            if ended {
                return Err(match self.wait().await {
                    Ended::Failed(why) => why,
                    Ended::Ok => last_words(&self.shown(), "ended without a link to open"),
                });
            }
            self.changed().await;
        }
    }

    /// Until the terminal has shown more, or opencode ended.
    async fn changed(&self) {
        let (mut seen, mut end) = (self.seen.clone(), self.end.clone());
        tokio::select! {
            _ = seen.changed() => {}
            _ = end.wait_for(Option::is_some) => {}
        }
    }

    /// Write KEY into the terminal, once, followed by Enter: only when opencode's current prompt is exactly its masked
    /// [`KEY_PROMPT`] and the terminal echoes nothing (@clack/prompts reads in raw mode), so that no screen shows it.
    /// The bytes written are overwritten after the write, and KEY, dropped here, overwrites its own. An Err says why
    /// nothing was written, and holds no part of the key.
    pub async fn type_key(&self, key: Secret) -> Result<(), String> {
        let word = match crate::keyfile::word(key.expose()) {
            Ok(w) if w.chars().any(char::is_control) => return Err("a key has no control characters: paste it again".into()),
            Ok(w) => w,
            Err(why) => return Err(why.into()),
        };
        match current_prompt(&self.shown()) {
            Some(p) if p == KEY_PROMPT => {}
            Some(p) => return Err(format!("opencode asks {p:?}, not for an API key: nothing was typed into it")),
            None => return Err("opencode asks for nothing now: nothing was typed into it".into()),
        }
        if self.echoes()? {
            return Err("opencode's terminal would show what is typed: nothing was typed into it".into());
        }
        let mut bytes = Vec::with_capacity(word.len() + 1);
        bytes.extend_from_slice(word.as_bytes());
        bytes.push(b'\r');
        drop(key);
        let wrote = self.write_all(&bytes).await;
        bytes.fill(0);
        drop(std::hint::black_box(bytes));
        wrote
    }

    /// KEY into opencode's own sign-in on this run, the way `Request::AccountKeyStore` gives it: wait for the prompt
    /// opencode asks first, write KEY into it by [`Run::type_key`]'s rules alone, and wait for opencode's end. Err
    /// with why nothing was written: the run was cancelled (the window's Cancel beside "Setting the key…") or ended
    /// before it asked, or it asked something else and was ended then. KEY, dropped on every path, overwrites its
    /// bytes; no Err holds any part of it.
    pub async fn give_key(&self, key: Secret) -> Result<Ended, String> {
        if self.prompt().await.is_none() {
            if self.closed.load(Ordering::SeqCst) {
                return Err("cancelled before opencode asked for the key: nothing was typed into it".into());
            }
            return Err(match self.wait().await {
                Ended::Failed(why) => why,
                Ended::Ok => format!("opencode ended without asking for a key: {}", self.shown().trim()),
            });
        }
        if let Err(why) = self.type_key(key).await {
            self.cancel().await;
            return Err(why);
        }
        Ok(self.wait().await)
    }

    /// Hand in the code the browser showed, followed by Enter: only into opencode's [`CODE_PROMPT`]. Whether it is
    /// right is opencode's to say (`auth list` afterwards).
    pub async fn paste(&self, code: &str) -> Result<(), String> {
        let code = code.trim();
        if code.is_empty() || code.chars().any(char::is_control) {
            return Err("a code is one line, not empty".into());
        }
        match current_prompt(&self.shown()) {
            Some(p) if p == CODE_PROMPT => {
                self.screen.lock().unwrap().pasted.push(code.to_string());
                self.write_all(format!("{code}\r").as_bytes()).await
            }
            _ => Err("opencode does not ask for a code now".into()),
        }
    }

    /// Whether the terminal echoes what is written into it: its line settings, read through the master end.
    fn echoes(&self) -> Result<bool, String> {
        // SAFETY: tcgetattr fills the termios given, on a descriptor this handle owns
        let mut t: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(self.tty.get_ref().as_raw_fd(), &mut t) } != 0 {
            return Err(format!("opencode's terminal: {}", std::io::Error::last_os_error()));
        }
        Ok(t.c_lflag & libc::ECHO != 0)
    }

    async fn write_all(&self, mut bytes: &[u8]) -> Result<(), String> {
        if self.closed.load(Ordering::SeqCst) || self.end.borrow().is_some() {
            return Err("opencode has ended: nothing was typed into it".into());
        }
        while !bytes.is_empty() {
            let mut ready = self.tty.writable().await.map_err(|e| format!("opencode's terminal: {e}"))?;
            match ready.try_io(|fd| write_fd(fd.get_ref().as_raw_fd(), bytes)) {
                Ok(Ok(n)) => bytes = &bytes[n..],
                Ok(Err(e)) => return Err(format!("opencode's terminal: {e}")),
                Err(_would_block) => {}
            }
        }
        Ok(())
    }

    /// Wait for opencode's end, however long that is.
    pub async fn wait(&self) -> Ended {
        let mut rx = self.end.clone();
        match rx.wait_for(Option::is_some).await {
            Ok(e) => e.clone().unwrap_or_else(|| Ended::Failed("lost".into())),
            Err(_) => Ended::Failed("opencode: its watcher ended without a word".into()),
        }
    }

    /// How it ended, if it has (without waiting).
    pub fn ended(&self) -> Option<Ended> {
        self.end.borrow().clone()
    }

    /// End it: nothing more is written, and its process group is ended. What opencode had written under the
    /// account's `home/` is opencode's: nothing there is touched.
    pub async fn cancel(&self) -> Ended {
        self.closed.store(true, Ordering::SeqCst);
        self.kill();
        self.wait().await
    }

    fn kill(&self) {
        if !self.reaped.load(Ordering::SeqCst) {
            // SAFETY: signals the process group this handle started, whose leader was not reaped when this looked, so
            // the id is still its own (no new process is given it in the instant between the look and the signal)
            unsafe { libc::kill(-self.pid, libc::SIGTERM) };
        }
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        self.kill();
    }
}

/// A new terminal of [`ROWS`] by [`COLS`]: its master and its slave end, neither passed on to a program but as its
/// stdio.
fn open_pty() -> Result<(OwnedFd, OwnedFd), String> {
    let (mut m, mut s) = (0, 0);
    let ws = libc::winsize { ws_row: ROWS, ws_col: COLS, ws_xpixel: 0, ws_ypixel: 0 };
    // SAFETY: openpty fills the two descriptors it opened, which are owned from here on
    if unsafe { libc::openpty(&mut m, &mut s, std::ptr::null_mut(), std::ptr::null(), &ws) } != 0 {
        return Err(format!("opencode: a terminal for it: {}", std::io::Error::last_os_error()));
    }
    // SAFETY: both were just opened by openpty and are owned by nothing else
    let (m, s) = unsafe { (OwnedFd::from_raw_fd(m), OwnedFd::from_raw_fd(s)) };
    for fd in [&m, &s] {
        // SAFETY: fcntl on a descriptor owned here
        unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) };
    }
    Ok((m, s))
}

fn nonblocking(fd: &OwnedFd) -> Result<(), String> {
    // SAFETY: fcntl on a descriptor owned by the caller
    let set = unsafe {
        let flags = libc::fcntl(fd.as_raw_fd(), libc::F_GETFL);
        flags >= 0 && libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) >= 0
    };
    if set { Ok(()) } else { Err(format!("opencode: its terminal: {}", std::io::Error::last_os_error())) }
}

fn read_fd(fd: i32, buf: &mut [u8]) -> std::io::Result<usize> {
    // SAFETY: reads into a buffer of the length given
    let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
    if n < 0 { Err(std::io::Error::last_os_error()) } else { Ok(n as usize) }
}

fn write_fd(fd: i32, buf: &[u8]) -> std::io::Result<usize> {
    // SAFETY: writes from a buffer of the length given
    let n = unsafe { libc::write(fd, buf.as_ptr().cast(), buf.len()) };
    if n < 0 { Err(std::io::Error::last_os_error()) } else { Ok(n as usize) }
}

/// Read what the terminal has into SC once it has something. False once it is closed: every process that had it
/// ended (Linux reads EIO then).
async fn read_some(t: &AsyncFd<OwnedFd>, sc: &Mutex<Screen>) -> bool {
    let Ok(mut ready) = t.readable().await else { return false };
    let mut buf = [0u8; 4096];
    match ready.try_io(|fd| read_fd(fd.get_ref().as_raw_fd(), &mut buf)) {
        Ok(Ok(0)) | Ok(Err(_)) => false,
        Ok(Ok(n)) => {
            sc.lock().unwrap().push(&buf[..n], true);
            true
        }
        Err(_would_block) => true,
    }
}

/// Read into SC what the terminal holds now, without waiting: after opencode ended, its last words.
fn drain(t: &AsyncFd<OwnedFd>, sc: &Mutex<Screen>) {
    let mut buf = [0u8; 4096];
    while let Ok(n) = read_fd(t.get_ref().as_raw_fd(), &mut buf) {
        if n == 0 {
            return;
        }
        sc.lock().unwrap().push(&buf[..n], true);
    }
}

/// The prompt @clack/prompts shows at the end of TEXT (escape sequences taken out), if it waits on one there: its
/// frame is the title `◆  MESSAGE` (`▲  MESSAGE` after a refused answer), its own lines, each beginning with `│`, and
/// last the line `└` (`└  WHY` after a refused answer). A log line (`●`, a success's `◆`, a warning's `▲`), a prompt
/// already answered (`◇`), the end's `└  Done`, or a frame not wholly printed yet, is no prompt. The message trimmed.
fn current_prompt(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().map(str::trim_end).filter(|l| !l.is_empty()).collect();
    let (last, rest) = lines.split_last()?;
    let refused = match *last {
        "└" => false,
        l if l.starts_with("└  ") => true,
        _ => return None,
    };
    let own = rest.iter().rev().take_while(|l| l.starts_with('│')).count();
    if own == 0 || own == rest.len() {
        return None;
    }
    let title = rest[rest.len() - own - 1];
    let message = if refused { title.strip_prefix("▲  ") } else { title.strip_prefix("◆  ").or_else(|| title.strip_prefix("▲  ")) }?;
    Some(message.trim().to_string())
}

/// The link of TEXT's `Go to: URL` line, once that line is whole, and the log line printed after it, its
/// instructions (`●  Enter code: …`), once something follows it or opencode ENDed.
fn go_to(text: &str, end: bool) -> Option<(String, Option<String>)> {
    let at = text.find("Go to: ")?;
    let rest = &text[at + "Go to: ".len()..];
    let line = rest.find('\n')?;
    let url = rest[..line].trim().to_string();
    if url.is_empty() {
        return None;
    }
    let next = rest[line + 1..].lines().map(str::trim).find(|l| !l.is_empty() && *l != "│");
    match next {
        None if !end => None,
        next => Some((url, next.and_then(|l| l.strip_prefix("●")).map(|l| l.trim().to_string()).filter(|l| !l.is_empty()))),
    }
}

/// What opencode keeps for an account (`opencode auth list`): each stored sign-in, by the provider's name and its
/// kind (`api`, `oauth`, `wellknown`). Names only: opencode prints no secret there.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub stored: Vec<(String, String)>,
}

/// Ask opencode what it keeps for the account whose folder is DIR: PROGRAM `auth list` with the run's environment,
/// which has no provider's key variable, so only what is stored counts. Read from its output, never from its file.
pub async fn status(program: &Path, dir: &Path) -> Result<Status, String> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(["auth", "list"]).env_clear().envs(run_env(dir)).current_dir(dir.join("home")).stdin(Stdio::null());
    let out = cmd.output().await.map_err(|e| format!("opencode auth list: {e}"))?;
    let text = plain(&String::from_utf8_lossy(&out.stdout));
    let said = || format!("{} {}", text.trim(), plain(&String::from_utf8_lossy(&out.stderr)).trim());
    if !out.status.success() {
        return Err(format!("opencode auth list {}: {}", how(out.status), said()));
    }
    stored(&text).ok_or_else(|| format!("opencode auth list: not its answer as read in {VERSION}: {}", said()))
}

/// The sign-ins `auth list` names in TEXT: each `●  NAME KIND` line before its `└  N credentials`, N being their
/// number. None for any other text.
fn stored(text: &str) -> Option<Status> {
    let mut stored = Vec::new();
    for l in text.lines().map(str::trim) {
        if let Some(n) = l.strip_prefix('└').and_then(|r| r.trim().strip_suffix(" credentials")).and_then(|n| n.trim().parse::<usize>().ok()) {
            return (n == stored.len()).then_some(Status { stored });
        }
        if let Some(item) = l.strip_prefix('●') {
            let (name, kind) = item.trim().rsplit_once(' ')?;
            stored.push((name.trim().to_string(), kind.to_string()));
        }
    }
    None
}

/// The models opencode offers on the account whose folder is DIR for PROVIDER: PROGRAM `models PROVIDER` with the
/// run's environment, each `PROVIDER/MODEL` line of its answer, in its order. That is how an opencode agent's model is
/// named (roster's `*/*`). opencode lists a provider only while it keeps a sign-in for it, and ends with `Provider not
/// found` otherwise: an Err in its words then.
pub async fn models(program: &Path, dir: &Path, provider: &str) -> Result<Vec<String>, String> {
    if let Some(why) = bad_provider(provider) {
        return Err(why);
    }
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(["models", provider]).env_clear().envs(run_env(dir)).current_dir(dir.join("home")).stdin(Stdio::null());
    let out = cmd.output().await.map_err(|e| format!("opencode models: {e}"))?;
    let text = plain(&String::from_utf8_lossy(&out.stdout));
    if !out.status.success() {
        return Err(format!("opencode models {provider} {}: {} {}", how(out.status), text.trim(), plain(&String::from_utf8_lossy(&out.stderr)).trim()));
    }
    Ok(listed(&text, provider))
}

/// The models of PROVIDER that TEXT, the answer of `opencode models PROVIDER`, names: each line `PROVIDER/MODEL`, with
/// no space in it.
fn listed(text: &str, provider: &str) -> Vec<String> {
    let of = |l: &str| l.strip_prefix(provider).and_then(|m| m.strip_prefix('/')).is_some_and(|m| !m.is_empty() && !m.contains(char::is_whitespace));
    text.lines().map(str::trim).filter(|l| of(l)).map(String::from).collect()
}

/// Put what [`status`] said into opencode account ID's `state.json`, at NOW (seconds): signed in when opencode keeps a
/// sign-in for it, and its kind as `auth_method`. MODELS, when opencode was asked ([`models`]), become its
/// [`State::models`]. None keeps those it had while it is signed in, and a sign-in that is gone takes them away.
pub fn record(store: &Store, id: &str, s: &Status, models: Option<Vec<String>>, now: i64) -> Result<State, String> {
    store.update(Harness::Opencode, id, |st| {
        st.signed_in = !s.stored.is_empty();
        st.signed_in_seen = now;
        st.auth_method = s.stored.first().map(|(_, kind)| kind.clone());
        if st.signed_in {
            st.put_back = None;
        }
        match models {
            Some(m) => st.models = m,
            None if !st.signed_in => st.models.clear(),
            None => {}
        }
    })
}

/// opencode's own sign-out of PROVIDER on the account whose folder is DIR (as it was set aside): PROGRAM `auth logout
/// PROVIDER`, which names it, so asks nothing. The sign-in is opencode's to end as it was opencode's to make.
pub async fn logout(program: &Path, dir: &Path, provider: &str) -> Result<(), String> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(["auth", "logout", provider]).env_clear().envs(run_env(dir)).current_dir(dir.join("home")).stdin(Stdio::null());
    let out = cmd.output().await.map_err(|e| format!("opencode auth logout: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let said = format!("{} {}", plain(&String::from_utf8_lossy(&out.stdout)).trim(), plain(&String::from_utf8_lossy(&out.stderr)).trim());
    Err(format!("opencode auth logout {}: {}", how(out.status), said.trim()))
}

/// OpenCode Go's provider, the one whose usage coordd reads ([`go_usage`]).
pub const GO: &str = "opencode-go";

/// OpenCode Go's usage page (opencode.ai/docs/go: "You can track your current usage in the console, or with the usage
/// endpoint"): one meter for each of the plan's limits, `rolling` (its 5 hours), `weekly` and `monthly`, each `status`
/// (`ok`, or `rate-limited` once used up), `percent` (a whole number, 0 to 100) and `resetsAt` (ISO 8601). It takes the
/// account's own key as a Bearer token, and answers 401 to a key it does not know and 403 to one Go does not cover.
/// opencode 1.18.34 never reads it.
pub const GO_USAGE_URL: &str = "https://opencode.ai/inference/go/v1/usage";

/// Keep KEY, the one OpenCode Go account ID was just signed in with, to read Go's usage with and to send its plain
/// calls ([`super::plain`]), and for nothing else: as
/// the one header Go's usage page takes, written as a key file ([`crate::keyfile::write`]) to [`Store::usage_key_file`].
/// Nothing here prints it, and coordd never reads it back ([`go_usage`]).
pub fn keep_usage_key(store: &Store, id: &str, key: &Secret) -> Result<(), String> {
    let header = Secret::new(format!("Authorization: Bearer {}", crate::keyfile::word(key.expose())?));
    let file = store.usage_key_file(id);
    crate::keyfile::write(&file, header.expose()).map_err(|e| format!("{}: {e}", file.display()))
}

/// Go's usage of the account whose usage key is FILE ([`Store::usage_key_file`]), from [`GO_USAGE_URL`] by CURL (`curl`;
/// a stand-in in tests): curl takes the header from FILE itself (`-H @FILE`), so the key is in no argument and coordd
/// never reads it; https only, no `.curlrc`. An Err says why in words that hold no key: curl's own failure, Go's 401
/// (it does not know the key), another answer with its words, or an answer without a meter ([`from_go_usage`]).
pub async fn go_usage(curl: &Path, file: &Path) -> Result<Said, String> {
    let mut header = OsString::from("@");
    header.push(file);
    let mut cmd = tokio::process::Command::new(curl);
    cmd.args(["--disable", "--silent", "--show-error", "--proto", "=https", "--write-out", "\n%{http_code}", "--header"]).arg(header).arg(GO_USAGE_URL);
    let out = cmd.stdin(Stdio::null()).kill_on_drop(true).output().await.map_err(|e| format!("curl: {e}"))?;
    if !out.status.success() {
        return Err(format!("curl {}: {}", how(out.status), String::from_utf8_lossy(&out.stderr).trim()));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let (body, code) = text.rsplit_once('\n').unwrap_or(("", &text));
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    match code.trim() {
        "200" => from_go_usage(&v["usage"]),
        "401" => Err("Go does not know the key kept for its usage (401): give the account its key again".into()),
        c => {
            let said = v["error"]["message"].as_str().map_or_else(|| body.trim().chars().take(200).collect(), str::to_string);
            Err(format!("Go's usage page answered {c}: {said}"))
        }
    }
}

/// What Go's usage page says of the account (its `usage`, U), as a [`Said`] from [`limits::GO_USAGE`]: `rolling` is the
/// 5-hour window, `weekly` the week's and `monthly` the month's, each `percent` used until `resetsAt`. A meter Go says
/// is `rate-limited` makes the account out until the latest such reset, for its limit ([`limits::RATE_LIMIT`]), as a
/// window used up on a Claude Code account does; with none it can take requests. A meter that cannot be read is left
/// out; Err when none can.
pub fn from_go_usage(u: &serde_json::Value) -> Result<Said, String> {
    let meter = |name: &str| -> Option<(Window, bool)> {
        let m = &u[name];
        let resets_at = chrono::DateTime::parse_from_rfc3339(m["resetsAt"].as_str()?).ok()?.timestamp();
        Some((Window { used_percent: m["percent"].as_f64()?, resets_at }, m["status"] == "rate-limited"))
    };
    let meters = [("5-hour", meter("rolling")), ("weekly", meter("weekly")), ("monthly", meter("monthly"))];
    if meters.iter().all(|(_, m)| m.is_none()) {
        return Err(format!("Go's usage page named no meter: {}", u.to_string().chars().take(200).collect::<String>()));
    }
    let limited: Vec<(&str, i64)> = meters.iter().filter_map(|(name, m)| m.as_ref().filter(|(_, l)| *l).map(|(w, _)| (*name, w.resets_at))).collect();
    let until = limited.iter().map(|&(_, t)| t).max();
    let words = until.map(|_| format!("Go: its {} limit is used up", limited.iter().map(|&(n, _)| n).collect::<Vec<_>>().join(" and ")));
    let [(_, five_hour), (_, seven_day), (_, month)] = meters.map(|(n, m)| (n, m.map(|(w, _)| w)));
    // the plan has a 5-hour limit whether or not its meter was read: one read says so (`limits::apply`), none says nothing
    Ok(Said {
        five_hour,
        seven_day,
        month,
        out_for: until.map(|_| limits::RATE_LIMIT),
        words,
        ..Said::of(until.map_or(Verdict::In, |t| Verdict::Out(Some(t))), limits::GO_USAGE)
    })
}

/// Whether Codex's callback port ([`CODEX_PORT`]) is free on this machine: asked by taking it and letting it go at
/// once. While a Codex sign-in holds it, opencode's [`ON_CODEX_PORT`] way could not get the browser's redirect.
pub fn codex_port_free() -> bool {
    std::net::TcpListener::bind(("127.0.0.1", CODEX_PORT)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::super::testing::Tmp;
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    /// F, given at most 20 s: a stand-in that hangs fails the test instead of the queue.
    async fn soon<T>(f: impl std::future::Future<Output = T>) -> T {
        tokio::time::timeout(Duration::from_secs(20), f).await.expect("the stand-in took too long")
    }

    /// A stand-in `opencode` in the test's folder. Its `auth login` records its environment's names (no values), then
    /// runs LOGIN (shell) as its prompts. Its `auth list` answers as 1.18.34 does, with one stored line once LOGIN has
    /// written `$XDG_DATA_HOME/opencode/auth.json`. Its `auth logout` removes that file. Its `models opencode-go` names
    /// two models while that file is there, and otherwise ends as 1.18.34 does for a provider it keeps no sign-in for.
    fn stand_in(t: &Tmp, login: &str) -> PathBuf {
        let script = format!(
            r#"#!/bin/sh
case "$1 $2" in
"auth login")
  env | cut -d= -f1 | sort > "$HOME/../env-names"
  printf '┌  Add credential\n│\n'
  {login}
  ;;
"auth list")
  printf '┌  Credentials ~/.local/share/opencode/auth.json\n│\n'
  if [ -s "$XDG_DATA_HOME/opencode/auth.json" ]; then printf '●  OpenCode Go api\n│\n└  1 credentials\n'; else printf '└  0 credentials\n'; fi
  ;;
"auth logout")
  rm -f "$XDG_DATA_HOME/opencode/auth.json"; printf '┌  Remove credential\n│\n└  Logout successful\n'
  ;;
"models "*)
  if [ -s "$XDG_DATA_HOME/opencode/auth.json" ] && [ "$2" = opencode-go ]; then printf 'opencode-go/kimi-k3\nopencode-go/glm-5.3\n'; else printf 'Error: Provider not found: %s\n' "$2" >&2; exit 1; fi
  ;;
esac
"#
        );
        t.program("opencode", &script)
    }

    /// opencode's key prompt as @clack/prompts draws it, in raw mode (no echo) as it reads, the line read stored where
    /// opencode keeps it.
    const KEY: &str = r#"stty -echo
  printf '◆  Enter your API key\n│  _\n└\n'
  IFS= read -r key
  mkdir -p "$XDG_DATA_HOME/opencode"; printf '%s' "$key" > "$XDG_DATA_HOME/opencode/auth.json"
  printf '◇  Enter your API key\n│  ▪▪▪▪\n│\n└  Done\n'"#;

    fn account(t: &Tmp) -> PathBuf {
        t.store.create_opencode("go", "Go", 30, "opencode-go", "2026-11-19T12:00:00Z").unwrap();
        t.store.dir(Harness::Opencode, "go")
    }

    /// Every file under DIR but those under SKIP, with its text.
    fn files(dir: &Path, skip: &Path) -> Vec<(PathBuf, String)> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.starts_with(skip) {
                continue;
            }
            if p.is_dir() {
                out.extend(files(&p, skip));
            } else {
                out.push((p.clone(), String::from_utf8_lossy(&std::fs::read(&p).unwrap()).into_owned()));
            }
        }
        out
    }

    #[tokio::test]
    async fn the_key_goes_only_into_its_masked_prompt_and_auth_list_says_signed_in() {
        let t = Tmp::new("opencode-key");
        let (oc, dir) = (stand_in(&t, KEY), account(&t));
        assert!(soon(status(&oc, &dir)).await.unwrap().stored.is_empty());
        let none = soon(models(&oc, &dir, "opencode-go")).await.unwrap_err();
        assert!(none.contains("Provider not found"), "no sign-in, no models: {none}");
        let run = Run::start(&oc, &dir, &["auth", "login", "-p", "opencode-go"]).unwrap();
        assert_eq!(soon(run.prompt()).await.as_deref(), Some(KEY_PROMPT));
        soon(run.type_key(Secret::new("  sk-made-up-go-4711\n".into()))).await.unwrap();
        assert_eq!(soon(run.wait()).await, Ended::Ok);
        // what was shown holds no part of it
        assert!(!run.shown().contains("4711") && run.shown().contains("Done"), "{}", run.shown());
        let st = soon(status(&oc, &dir)).await.unwrap();
        assert_eq!(st.stored, [("OpenCode Go".to_string(), "api".to_string())]);
        let named = soon(models(&oc, &dir, "opencode-go")).await.unwrap();
        assert_eq!(named, ["opencode-go/kimi-k3", "opencode-go/glm-5.3"], "the models opencode offers once signed in");
        let state = record(&t.store, "go", &st, Some(named.clone()), 1_000).unwrap();
        assert!(state.signed_in && state.signed_in_seen == 1_000 && state.auth_method.as_deref() == Some("api"));
        assert_eq!(state.models, named);
        assert_eq!(record(&t.store, "go", &st, None, 1_001).unwrap().models, named, "a check that did not ask keeps them");
        // opencode has it under the account's home/, the pasted word alone; no file of ours holds any part of it
        assert_eq!(std::fs::read_to_string(dir.join("home/data/opencode/auth.json")).unwrap(), "sk-made-up-go-4711");
        for (path, text) in files(&t.dir, &dir.join("home")) {
            assert!(!text.contains("4711"), "{} holds the key", path.display());
        }
        // nothing of the test's environment reached it: the run's own and opencode's folders under home/
        let names = std::fs::read_to_string(dir.join("env-names")).unwrap();
        let ours = ["HOME", "PATH", "TERM", "LANG", "TMPDIR", "XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME", "OPENCODE_DISABLE_PROJECT_CONFIG", "OPENCODE_DISABLE_CLAUDE_CODE", "OPENCODE_DISABLE_AUTOUPDATE", "PWD", "SHLVL", "_", "OLDPWD"];
        for n in names.lines() {
            assert!(ours.contains(&n), "{n} reached opencode");
        }
        // signed out: a check that finds no sign-in takes its models away
        soon(logout(&oc, &dir, "opencode-go")).await.unwrap();
        let st = soon(status(&oc, &dir)).await.unwrap();
        assert!(record(&t.store, "go", &st, None, 1_002).unwrap().models.is_empty());
    }

    #[tokio::test]
    async fn a_key_run_cancelled_before_its_prompt_types_nothing_and_a_second_key_is_taken() {
        let t = Tmp::new("opencode-cancel");
        let dir = account(&t);
        // opencode still starting: its key prompt comes only after a while
        let slow = stand_in(&t, &format!("sleep 30\n  {KEY}"));
        let run = Arc::new(Run::start(&slow, &dir, &["auth", "login", "-p", "opencode-go"]).unwrap());
        let giving = tokio::spawn({
            let run = run.clone();
            async move { run.give_key(Secret::new("sk-made-up-go-0901".into())).await }
        });
        soon(async {
            while !run.shown().contains("Add credential") {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await;
        assert!(!giving.is_finished(), "the key waits for its prompt");
        // the window's Cancel (`AccountCancel` on the account's running sign-in)
        assert!(matches!(soon(run.cancel()).await, Ended::Failed(_)));
        let why = soon(giving).await.unwrap().unwrap_err();
        assert!(why.contains("cancelled before opencode asked") && why.contains("nothing was typed") && !why.contains("0901"), "{why}");
        assert!(!run.shown().contains("0901"));
        assert!(!dir.join("home/data/opencode/auth.json").exists(), "nothing was typed into it");
        assert!(soon(status(&slow, &dir)).await.unwrap().stored.is_empty());
        // a Cancel that came before the key's run started: coordd ends the run as it starts, and the key goes nowhere
        let run = Run::start(&slow, &dir, &["auth", "login", "-p", "opencode-go"]).unwrap();
        soon(run.cancel()).await;
        let why = soon(run.give_key(Secret::new("sk-made-up-go-0903".into()))).await.unwrap_err();
        assert!(why.contains("cancelled before opencode asked") && !why.contains("0903"), "{why}");
        // a second paste is taken, by a run of its own
        let oc = stand_in(&t, KEY);
        let run = Run::start(&oc, &dir, &["auth", "login", "-p", "opencode-go"]).unwrap();
        assert_eq!(soon(run.give_key(Secret::new("sk-made-up-go-0902".into()))).await, Ok(Ended::Ok));
        assert_eq!(std::fs::read_to_string(dir.join("home/data/opencode/auth.json")).unwrap(), "sk-made-up-go-0902");
        assert_eq!(soon(status(&oc, &dir)).await.unwrap().stored.len(), 1);
        for (path, text) in files(&t.dir, &dir.join("home")) {
            assert!(!["0901", "0902", "0903"].iter().any(|k| text.contains(k)), "{} holds a key", path.display());
        }
    }

    #[tokio::test]
    async fn another_prompt_gets_no_key_and_neither_its_screen_nor_its_words_hold_any() {
        let t = Tmp::new("opencode-other");
        let dir = account(&t);
        // the provider list's search field, which shows what is typed
        let oc = stand_in(&t, r#"stty -echo
  printf '◆  Select provider\n│  Search: _\n│  ● OpenCode Go (recommended)\n│  ○ OpenAI\n└\n'
  IFS= read -r got; printf '%s' "$got" > "$HOME/../got""#);
        let run = Run::start(&oc, &dir, &["auth", "login", "-p", "opencode-go"]).unwrap();
        assert_eq!(soon(run.prompt()).await.as_deref(), Some("Select provider"));
        let why = soon(run.type_key(Secret::new("sk-made-up-go-0815".into()))).await.unwrap_err();
        assert!(why.contains("\"Select provider\"") && why.contains("nothing was typed") && !why.contains("0815"), "{why}");
        let Ended::Failed(words) = soon(run.cancel()).await else { panic!("a cancelled run ended well") };
        assert!(!words.contains("0815") && !run.shown().contains("0815"), "{words}");
        assert!(!dir.join("got").exists() || std::fs::read_to_string(dir.join("got")).unwrap().is_empty());
        // a terminal that would echo it gets none either, even at the right prompt
        let oc = stand_in(&t, r#"printf '◆  Enter your API key\n│  _\n└\n'
  IFS= read -r got; printf '%s' "$got" > "$HOME/../got""#);
        let run = Run::start(&oc, &dir, &["auth", "login", "-p", "opencode-go"]).unwrap();
        assert_eq!(soon(run.prompt()).await.as_deref(), Some(KEY_PROMPT));
        let why = soon(run.type_key(Secret::new("sk-made-up-go-0816".into()))).await.unwrap_err();
        assert!(why.contains("would show"), "{why}");
        soon(run.cancel()).await;
        assert!(!run.shown().contains("0816"));
        // a version whose prompt never comes: the run ends, nothing waits for a key
        let oc = stand_in(&t, r#"printf '●  Something else\n│\n└  Done\n'"#);
        let run = Run::start(&oc, &dir, &["auth", "login", "-p", "opencode-go"]).unwrap();
        assert_eq!(soon(run.prompt()).await, None);
        assert!(soon(run.type_key(Secret::new("sk-made-up-go-0817".into()))).await.is_err());
        assert!(soon(status(&oc, &dir)).await.unwrap().stored.is_empty());
    }

    #[tokio::test]
    async fn a_browser_sign_in_says_its_link_and_instructions_and_a_code_goes_only_into_its_prompt() {
        let t = Tmp::new("opencode-browser");
        let dir = account(&t);
        let oc = stand_in(&t, r#"printf '●  Go to: https://auth.openai.com/codex/device\n│\n●  Enter code: ABCD-1234\n│\n'
  stty -echo
  printf '◆  Paste the authorization code here: \n│  _\n└\n'
  IFS= read -r code
  printf '◇  Paste the authorization code here: \n│  %s\n' "$code"
  if [ "$code" = good ]; then mkdir -p "$XDG_DATA_HOME/opencode"; printf 'oauth' > "$XDG_DATA_HOME/opencode/auth.json"; printf '◆  Login successful\n'; else printf '■  Failed to authorize\n'; fi
  printf '│\n└  Done\n'"#);
        let run = Run::start(&oc, &dir, &["auth", "login", "-p", "openai", "-m", "ChatGPT Pro/Plus (headless)"]).unwrap();
        assert_eq!(soon(run.link()).await.unwrap(), ("https://auth.openai.com/codex/device".to_string(), Some("Enter code: ABCD-1234".to_string())));
        assert_eq!(soon(run.prompt()).await.as_deref(), Some(CODE_PROMPT));
        // not a key's prompt: no key goes into it
        assert!(soon(run.type_key(Secret::new("sk-made-up-0818".into()))).await.unwrap_err().contains("not for an API key"));
        soon(run.paste(" good ")).await.unwrap();
        assert_eq!(soon(run.wait()).await, Ended::Ok);
        // the code its prompt shows as typed is taken out of what is shown
        assert!(!run.shown().contains("good") && run.shown().contains("Login successful"), "{}", run.shown());
        assert!(soon(run.paste("again")).await.is_err(), "nothing is written after its end");
        assert_eq!(soon(status(&oc, &dir)).await.unwrap().stored.len(), 1);
        // its own sign-out, in the folder as it was set aside
        soon(logout(&oc, &dir, "openai")).await.unwrap();
        assert!(soon(status(&oc, &dir)).await.unwrap().stored.is_empty());
    }

    #[test]
    fn a_prompt_is_read_from_its_frame_and_auth_list_from_its_lines() {
        let frame = "┌  Add credential\n│\n◆  Enter your API key\n│  _\n└\n";
        assert_eq!(current_prompt(frame).as_deref(), Some(KEY_PROMPT));
        assert_eq!(current_prompt("◆  Enter your API key\n│  _\n").as_deref(), None, "not wholly printed");
        assert_eq!(current_prompt("▲  Enter your API key\n│  _\n└  Required\n").as_deref(), Some(KEY_PROMPT));
        assert_eq!(current_prompt("◇  Enter your API key\n│  ▪▪\n│\n└  Done\n"), None, "answered");
        assert_eq!(current_prompt("◆  Login successful\n│\n└  Done\n"), None, "a success's log line");
        assert_eq!(current_prompt("●  Go to: https://x\n│\n└\n"), None);
        assert_eq!(stored("┌  Credentials ~/x/auth.json\n│\n●  OpenCode Go api\n│\n●  OpenAI oauth\n│\n└  2 credentials\n").unwrap().stored.len(), 2);
        assert_eq!(stored("┌  Credentials ~/x\n│\n└  0 credentials\n").unwrap().stored, []);
        assert_eq!(stored("┌  Credentials ~/x\n│\n●  OpenCode Go api\n└  2 credentials\n"), None, "a count that is not its lines");
        assert_eq!(stored("Error: no such command\n"), None);
        let answer = "opencode-go/kimi-k3\n  opencode-go/glm-5.3  \nopencode/big-pickle\nopencode-gox/a\nopencode-go/\nopencode-go/a b\nError: x\n";
        assert_eq!(listed(answer, "opencode-go"), ["opencode-go/kimi-k3", "opencode-go/glm-5.3"], "its provider's lines alone");
        assert_eq!(go_to("●  Go to: https://a.b/c\n", false), None, "its instructions may follow");
        assert_eq!(go_to("●  Go to: https://a.b/c\n", true), Some(("https://a.b/c".into(), None)));
        assert_eq!(go_to("●  Go to: http://localhost:1455/x\n│\n◆  Paste the authorization code here:\n", false), Some(("http://localhost:1455/x".into(), None)));
    }

    #[test]
    fn the_program_is_the_coords_pinned_one_and_a_provider_is_an_id() {
        let t = Tmp::new("opencode-program");
        let coord = t.dir.join("coord");
        let why = program(&coord).unwrap_err();
        assert!(why.contains("1.18.34 is not installed") && why.contains("opt/opencode/1.18.34/bin/opencode"), "{why}");
        let p = coord.join("opt/opencode/1.18.34/bin/opencode");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "#!/bin/sh\n").unwrap();
        assert!(program(&coord).unwrap_err().contains("not a program"));
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        // a program there that is not the one fetched is not run
        let why = program(&coord).unwrap_err();
        assert!(why.contains("is not the opencode 1.18.34 that was fetched") && why.contains(SHA256), "{why}");
        // the same file is taken under its own hash
        use sha2::Digest;
        let own: String = sha2::Sha256::digest(b"#!/bin/sh\n").iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(pinned(&p, &own).unwrap(), p);
        assert_eq!(bad_provider("opencode-go"), None);
        for bad in ["", "-p", "OpenCode Go", "a/b", "x;y"] {
            assert!(bad_provider(bad).is_some(), "{bad:?}");
        }
    }

    /// Go's usage page as its docs give it (opencode.ai/docs/go; made-up times): the 5-hour window is `rolling`, the
    /// week and the month their own. A used-up meter makes the account out until its reset, for its limit, in words
    /// that name it; recorded, the three windows stand, seen from Go's page. The week begun again, it takes requests and
    /// the month is kept as read. A page with no meter is an Err.
    #[test]
    fn gos_usage_page_is_its_three_windows_and_a_used_up_one_is_out_until_its_reset() {
        let at = |t: &str| chrono::DateTime::parse_from_rfc3339(t).unwrap().timestamp();
        let page = serde_json::json!({
            "rolling": {"status": "ok", "percent": 30, "resetsAt": "2026-11-22T15:00:00Z"},
            "weekly": {"status": "rate-limited", "percent": 100, "resetsAt": "2026-11-23T00:00:00Z"},
            "monthly": {"status": "ok", "percent": 50, "resetsAt": "2026-12-01T00:00:00Z"},
        });
        let said = from_go_usage(&page).unwrap();
        let w = |used: f64, t: &str| Some(Window { used_percent: used, resets_at: at(t) });
        assert_eq!((&said.five_hour, &said.seven_day, &said.month), (&w(30.0, "2026-11-22T15:00:00Z"), &w(100.0, "2026-11-23T00:00:00Z"), &w(50.0, "2026-12-01T00:00:00Z")));
        assert_eq!(said.verdict, Verdict::Out(Some(at("2026-11-23T00:00:00Z"))));
        assert_eq!((said.out_for, said.words.as_deref(), said.from), (Some(limits::RATE_LIMIT), Some("Go: its weekly limit is used up"), limits::GO_USAGE));
        let now = at("2026-11-22T12:00:00Z");
        let mut st = State { signed_in: true, ..State::default() };
        limits::apply(&mut st, &said, now);
        assert_eq!((&st.month, st.out_until, st.out_for.as_deref(), st.five_hour_limit), (&said.month, Some(at("2026-11-23T00:00:00Z")), Some(limits::RATE_LIMIT), Some(true)));
        assert_eq!((st.limits_seen, st.limits_from.as_str()), (now, limits::GO_USAGE));
        let again = serde_json::json!({
            "rolling": {"status": "ok", "percent": 0, "resetsAt": "2026-11-23T05:00:00Z"},
            "weekly": {"status": "ok", "percent": 1, "resetsAt": "2026-11-30T00:00:00Z"},
            "monthly": {"status": "ok", "percent": 51, "resetsAt": "2026-12-01T00:00:00Z"},
        });
        let said = from_go_usage(&again).unwrap();
        assert_eq!((&said.verdict, said.out_for, &said.words), (&Verdict::In, None, &None));
        limits::apply(&mut st, &said, at("2026-11-23T00:01:00Z"));
        assert_eq!((st.out_until, st.month.map(|w| w.used_percent)), (None, Some(51.0)));
        // one meter unread is left out; none read is no reading
        let two = from_go_usage(&serde_json::json!({"weekly": page["weekly"], "monthly": {"status": "ok", "percent": "half"}})).unwrap();
        assert!(two.five_hour.is_none() && two.month.is_none() && two.seven_day.is_some());
        assert!(from_go_usage(&serde_json::json!({})).unwrap_err().starts_with("Go's usage page named no meter"));
    }

    /// The key kept for Go's usage: one header in a file of the owner's alone, outside opencode's `home/`, given to curl
    /// as `--header @FILE`, https only, no `.curlrc`. Go's answers from a stand-in curl: 200, 401, other words, curl's own
    /// failure. Removed with the account.
    #[tokio::test]
    async fn the_usage_key_is_one_header_in_a_file_of_its_own_and_curl_reads_it_there() {
        let t = Tmp::new("go-usage");
        let dir = account(&t);
        assert!(!t.store.usage_key_set("go"));
        assert!(keep_usage_key(&t.store, "go", &Secret::new("  ".into())).is_err(), "nothing typed is no key");
        keep_usage_key(&t.store, "go", &Secret::new(" sk-made-up-go-4711\n".into())).unwrap();
        let file = t.store.usage_key_file("go");
        assert_eq!(file, dir.join("usage-key"));
        // one line, as curl reads a header file
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "Authorization: Bearer sk-made-up-go-4711\n");
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(t.store.usage_key_set("go"));
        // a stand-in curl: its arguments kept, its answer the test's
        let args = t.dir.join("curl-args");
        let answer = t.dir.join("curl-answer");
        let curl = t.program("curl", &format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\ncat {}\n", args.display(), answer.display()));
        let answers = |body: &str, code: u16| std::fs::write(&answer, format!("{body}\n{code}")).unwrap();
        answers(r#"{"usage":{"rolling":{"status":"ok","percent":4,"resetsAt":"2026-11-22T15:00:00Z"},"weekly":{"status":"ok","percent":9,"resetsAt":"2026-11-23T00:00:00Z"},"monthly":{"status":"ok","percent":20,"resetsAt":"2026-12-01T00:00:00Z"}}}"#, 200);
        let said = soon(go_usage(&curl, &file)).await.unwrap();
        assert_eq!((said.five_hour.map(|w| w.used_percent), said.seven_day.map(|w| w.used_percent), said.month.map(|w| w.used_percent)), (Some(4.0), Some(9.0), Some(20.0)));
        let given = std::fs::read_to_string(&args).unwrap();
        let given: Vec<&str> = given.lines().collect();
        assert_eq!(given[0], "--disable", "no .curlrc: first, as curl takes it");
        assert!(given.windows(2).any(|p| p == ["--proto", "=https"]) && given.windows(2).any(|p| p[0] == "--header" && p[1] == format!("@{}", file.display())), "{given:?}");
        assert_eq!(given.last(), Some(&GO_USAGE_URL));
        assert!(!given.iter().any(|a| a.contains("4711")), "the key is in no argument: {given:?}");
        answers(r#"{"error":{"message":"Invalid API key"}}"#, 401);
        assert!(soon(go_usage(&curl, &file)).await.unwrap_err().contains("(401): give the account its key again"));
        answers(r#"{"error":{"message":"This key is not on a Go plan"}}"#, 403);
        assert_eq!(soon(go_usage(&curl, &file)).await.unwrap_err(), "Go's usage page answered 403: This key is not on a Go plan");
        t.program("curl", "#!/bin/sh\necho 'curl: (6) Could not resolve host' >&2\nexit 6\n");
        let why = soon(go_usage(&curl, &file)).await.unwrap_err();
        assert!(why.starts_with("curl ") && why.contains("Could not resolve host"), "{why}");
        // removed, the account's folder set aside holds no key of ours
        let aside = t.store.remove(Harness::Opencode, "go", "20261122-120000").unwrap();
        assert!(!aside.join("usage-key").exists() && aside.join("account.json").is_file());
        assert!(!t.store.usage_key_set("go"));
    }

    #[test]
    fn the_codex_port_held_is_not_free() {
        // held here, or by another program already: either way not free while held
        let held = std::net::TcpListener::bind(("127.0.0.1", CODEX_PORT));
        assert!(!codex_port_free());
        drop(held);
    }
}
