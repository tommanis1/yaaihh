//! coord-pty: the holder of one agent's terminal (vault designs/yaaihh-2026-11-07.md, "Terminals: a holder per
//! agent").
//!
//!   coord-pty NAME [--rows R] [--cols C] [--confirm TEXT] [--trust DIR]... [--home DIR] -- COMMAND...
//!
//! It leaves its caller (forked twice into a session of its own, so it is nobody's child: neither the caller's end
//! nor a coordd restart takes it along), asks coordd to put it in the cgroup `pty` (what it starts is unclaimed there
//! until `coord run`), runs COMMAND on a new pseudo-terminal (TERM=xterm-256color, 40 rows of 120 unless given) and
//! keeps a terminal emulator's screen of it. It serves `$COORD/pty/NAME.sock` (mode 0600; the wire is
//! `coord_types::pty`). The caller returns once the socket is up, or with COMMAND's error.
//!
//! The holder is the terminal: it answers the program's queries for the cursor position and the device attributes
//! itself, whether a client is attached or not, and leaves them out of what clients get (their terminals would
//! answer a second time). The latest client to type or resize sets the size (tmux's "latest"). When COMMAND ends,
//! its last screen goes to `$COORD/pty/NAME.last` as text and, with how it ended, to the attached clients; clients
//! that attach during the next HOLD still get it; then the socket goes and the holder ends.
//!
//! With `--confirm TEXT` the holder presses Enter once, the first time TEXT is on the screen, unless someone typed
//! first: the start dialog a program shows every time (Claude Code's development-channel warning). It is the
//! holder's because the holder outlives whoever started the program (roster's poller died with its caller, 9 Nov).
//!
//! With `--trust DIR` (roster: the project's repositories and the agent's own folder) Claude Code's popups asking to
//! trust a folder ("Adding a new directory:", "Accessing workspace:") are answered Yes for DIR or a folder inside it,
//! with a line in `$COORD/pty/trusted.log`; one for any other folder is told to coordd at once and never answered
//! ([`trust`]). Only Claude Code's own dialog at the foot of the screen is one; `~` in it is `--home`, the agent's.
//!
//! Any other dialog on the screen (a numbered choice with its pointer, `❯ 1.`) is told to coordd (`Request::Popup`):
//! its title, the line above its options, and the screen's last lines, once it has stood [`POPUP_STANDS`] unanswered.
//! The holder never answers it; coordd tells the owner the first time a title appears, so a new popup of a new Claude
//! Code version is noticed (owner, 10 Nov).
//!
//! Whether a turn runs is on the screen too: Claude Code's footer reads "esc to interrupt" exactly while one does.
//! The holder looks every [`BUSY_EVERY`] and tells coordd each change (`Request::Busy`), an end only once it has held
//! for [`IDLE_AFTER`]: a Claude Code agent's working or idle is what its screen shows (owner, 10 Nov; a transcript
//! can end on a prompt Claude Code never ran a turn for, a stopped background task's notice).
//!
//! A client may give the holder a line to type (`pty::COMMAND`: coordd's `/autocompact N`, N-804.p16). The holder keeps
//! the latest one and types it with its Enter at the first look of [`BUSY_EVERY`] that finds no turn running (as
//! `Request::Busy` counts it), no dialog, and an empty input box ([`box_empty`]: nothing in it but Claude Code's
//! faint suggestion, and every key a client sent drawn since). Clients' keys and the line go through one lock
//! (`TYPING`), so none is written between that look and the line: a line half typed by the owner is never added to.

use coord_types::pty::{self, Info};
use coord_types::{Reply, Request};
use std::ffi::CString;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::num::NonZeroU16;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

mod trust;

/// How long after the `--confirm` text appears its Enter is pressed.
const CONFIRM_AFTER: Duration = Duration::from_millis(500);

/// How often the screen's footer is looked at for a running turn (`Request::Busy`).
const BUSY_EVERY: Duration = Duration::from_millis(250);
/// How long the footer must show no running turn before the turn counts as ended.
const IDLE_AFTER: Duration = Duration::from_millis(1500);

/// How long a dialog stands unanswered on the screen before coordd is told of it (`Request::Popup`): one answered
/// meanwhile (the start dialog `--confirm` answers, or a person) is no news (paper-research m1039).
const POPUP_STANDS: Duration = Duration::from_secs(10);

/// How long a holder whose program ended still shows the last screen to clients that attach.
const HOLD: Duration = Duration::from_secs(10);
/// Frames queued for one client before it counts as gone: a client that stopped reading must not stall the program.
const BACKLOG: usize = 4096;

/// Lines scrolled off the main screen that the holder keeps, and sends a client that attaches.
const HISTORY: usize = 2000;

fn usage() -> ! {
    for line in include_str!("main.rs").lines().take_while(|l| l.starts_with("//!")) {
        eprintln!("{}", line.trim_start_matches("//!").strip_prefix(' ').unwrap_or(""));
    }
    std::process::exit(2)
}

fn fail(msg: impl std::fmt::Display) -> ! {
    eprintln!("coord-pty: {msg}");
    std::process::exit(1)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let sep = args.iter().position(|a| a == "--").unwrap_or_else(|| usage());
    let (opts, command) = (&args[..sep], args[sep + 1..].to_vec());
    let Some(name) = opts.first().cloned() else { usage() };
    if command.is_empty() {
        usage()
    }
    let (mut rows, mut cols, mut confirm, mut trusted, mut home) = (40, 120, None, Vec::new(), None);
    let mut it = opts[1..].iter();
    while let Some(o) = it.next() {
        let Some(v) = it.next() else { usage() };
        let size = || v.parse::<u16>().ok().filter(|&v| v > 0).unwrap_or_else(|| usage());
        match o.as_str() {
            "--rows" => rows = size(),
            "--cols" => cols = size(),
            "--confirm" if !v.is_empty() => confirm = Some(v.clone()),
            "--trust" if !v.is_empty() => trusted.push(v.clone()),
            "--home" if !v.is_empty() => home = Some(v.clone()),
            _ => usage(),
        }
    }
    if name.is_empty() || name.contains('/') {
        fail("NAME is an agent's name (no slashes)")
    }
    let coord = std::env::var("COORD").unwrap_or_else(|_| fail("COORD is not set"));
    let dir = pty::dir(&coord);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        fail(format!("{}: {e}", dir.display()))
    }
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    let sock = pty::socket(&coord, &name);
    // a holder whose program ended only shows its last screen: a new one takes its place
    if let Some(i) = info_of(&sock).filter(|i| i.exited.is_none()) {
        fail(format!("{name} has a terminal already (holder {}: {})", i.pid, i.command.join(" ")))
    }
    // the holder tells its caller through this pipe: "ok", "warning: …" lines, or "error: …"
    let (ready_r, ready_w) = pipe();
    match unsafe { libc::fork() } {
        -1 => fail(std::io::Error::last_os_error()),
        0 => {}
        mid => {
            drop(ready_w);
            let mut said = String::new();
            let _ = File::from(ready_r).read_to_string(&mut said);
            unsafe { libc::waitpid(mid, std::ptr::null_mut(), 0) };
            let mut ok = false;
            for line in said.lines() {
                match line {
                    "ok" => ok = true,
                    l => eprintln!("coord-pty: {name}: {}", l.trim_start_matches("warning: ").trim_start_matches("error: ")),
                }
            }
            if !ok && said.is_empty() {
                eprintln!("coord-pty: {name}: the holder ended before its socket was up");
            }
            std::process::exit(if ok { 0 } else { 1 })
        }
    }
    drop(ready_r);
    unsafe { libc::setsid() };
    match unsafe { libc::fork() } {
        0 => hold(name, coord, rows, cols, confirm, trust::Trust::new(&trusted, home.as_deref()), command, File::from(ready_w)),
        _ => unsafe { libc::_exit(0) }, // the middle one; a failed fork ends here too, and the caller hears of it
    }
}

/// The holder itself, from the socket to the end of COMMAND.
#[allow(clippy::too_many_arguments)]
fn hold(name: String, coord: String, rows: u16, cols: u16, confirm: Option<String>, trust: trust::Trust, command: Vec<String>, mut ready: File) -> ! {
    let sock = pty::socket(&coord, &name);
    let _ = std::fs::remove_file(&sock);
    let umask = unsafe { libc::umask(0o077) };
    let listener = UnixListener::bind(&sock).unwrap_or_else(|e| {
        let _ = writeln!(ready, "error: {}: {e}", sock.display());
        std::process::exit(1)
    });
    unsafe { libc::umask(umask) };
    // the socket is removed at the end only while it is still this holder's (a new holder may have taken the name):
    // the inode alone does not tell, as the file system may give the new socket the inode just freed (seen in the
    // box 8 Nov: each restarted agent's old holder removed its successor's socket after HOLD), so its change time too
    let mine = |m: std::fs::Metadata| (m.ino(), m.ctime(), m.ctime_nsec());
    let id = std::fs::metadata(&sock).map(mine).ok();
    let _ = std::fs::remove_file(pty::last(&coord, &name));
    if let Err(why) = enter_pty_cgroup() {
        let _ = writeln!(ready, "warning: {why}");
    }
    let (master, child) = match spawn(&command, rows, cols) {
        Ok(x) => x,
        Err(e) => {
            let _ = writeln!(ready, "error: {e}");
            let _ = std::fs::remove_file(&sock);
            std::process::exit(1)
        }
    };
    let _ = writeln!(ready, "ok");
    drop(ready);
    if let Ok(null) = File::options().read(true).write(true).open("/dev/null") {
        for fd in 0..3 {
            unsafe { libc::dup2(null.as_raw_fd(), fd) };
        }
    }
    let (Ok(input), Ok(output)) = (master.try_clone(), master.try_clone()) else { std::process::exit(1) };
    let input = Arc::new(input);
    let term = Arc::new(Mutex::new(Term {
        name,
        command,
        child,
        master,
        parser: vt100::Parser::new_with_callbacks(nz(rows), nz(cols), HISTORY, Title::default()),
        clients: Vec::new(),
        next: 0,
        pending: Vec::new(),
        exited: None,
        confirm,
        confirming: false,
        due: None,
        pastes: std::collections::VecDeque::new(),
        keyed: None,
        drawn: None,
    }));
    {
        let (term, input) = (term.clone(), input.clone());
        let keys = pty::keys(&coord, &term.lock().unwrap().name);
        std::thread::spawn(move || {
            for s in listener.incoming().flatten() {
                let (term, input, keys) = (term.clone(), input.clone(), keys.clone());
                std::thread::spawn(move || serve(&term, &input, s, &keys));
            }
        });
    }
    {
        let (term, input) = (term.clone(), input.clone());
        std::thread::spawn(move || read_output(&term, &input, output));
    }
    {
        let (term, input) = (term.clone(), input.clone());
        let coord = std::path::PathBuf::from(&coord);
        std::thread::spawn(move || watch_busy(&term, &input, trust, &coord));
    }
    let status = wait(child);
    std::thread::sleep(Duration::from_millis(200)); // the program's last output, still being read
    finish(&term, &coord, status);
    std::thread::sleep(HOLD);
    if id.is_some() && std::fs::metadata(&sock).map(mine).ok() == id {
        let _ = std::fs::remove_file(&sock);
    }
    std::process::exit(0)
}

/// The window title a program sets, for [`Info`] and the screen sent at attach.
#[derive(Default)]
struct Title(String);

impl vt100::Callbacks for Title {
    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        self.0 = String::from_utf8_lossy(title).chars().filter(|c| !c.is_control()).collect();
    }
}

/// An attached client: its frames go out through its own writer thread.
struct Client {
    id: u64,
    tx: SyncSender<Vec<u8>>,
    /// Its size, once it gave one.
    size: Option<(u16, u16)>,
}

/// The terminal: the emulator of the program's screen, the attached clients, and how the program ended.
struct Term {
    name: String,
    command: Vec<String>,
    child: i32,
    /// The pseudo-terminal's master, for its size and foreground process group (keys go through their own handle,
    /// outside the lock: a program that does not read must not stop its output being read).
    master: File,
    parser: vt100::Parser<Title>,
    clients: Vec<Client>,
    next: u64,
    /// The end of the output read so far, while it stops inside an escape sequence.
    pending: Vec<u8>,
    exited: Option<String>,
    /// The text whose first appearance on the screen is answered with Enter (`--confirm`), until then or until a
    /// client types.
    confirm: Option<String>,
    /// The `--confirm` text was seen and its Enter is on its way.
    confirming: bool,
    /// The line a client gave to be typed (`pty::COMMAND`), not typed yet ([`type_due`]).
    due: Option<Vec<u8>>,
    /// Words given to be typed as pastes (`pty::PASTE`), oldest first, not typed yet ([`type_paste`]).
    pastes: std::collections::VecDeque<Vec<u8>>,
    /// When a client's keys (or the line) were last written, and when the program last drew: the screen shows every
    /// key sent only once it drew after them.
    keyed: Option<std::time::Instant>,
    drawn: Option<std::time::Instant>,
}

/// A terminal query the holder answers.
enum Query {
    /// DSR 6: the cursor's position.
    Cursor,
    /// DSR 5: the terminal's status.
    Status,
    /// DA1 and DA2: what terminal this is.
    Primary,
    Secondary,
}

enum Scan {
    /// A query of that many bytes.
    Query(usize, Query),
    /// The output ends inside a control sequence.
    Partial,
    Other,
}

/// What the escape sequence at the start of B is.
fn scan(b: &[u8]) -> Scan {
    match b.get(1) {
        None => return Scan::Partial,
        Some(b'[') => {}
        Some(_) => return Scan::Other,
    }
    // CSI: parameter bytes, intermediate bytes, one final byte
    let Some(end) = b[2..].iter().position(|c| !(0x20..=0x3f).contains(c)).map(|p| p + 2) else {
        return if b.len() < 64 { Scan::Partial } else { Scan::Other };
    };
    let q = match (&b[2..end], b[end]) {
        (b"6", b'n') => Query::Cursor,
        (b"5", b'n') => Query::Status,
        (b"" | b"0", b'c') => Query::Primary,
        (b">" | b">0", b'c') => Query::Secondary,
        _ => return Scan::Other,
    };
    Scan::Query(end + 1, q)
}

impl Term {
    /// Take the program's output: answer the queries meant for the terminal, feed the rest to the emulator and the
    /// clients. Returns the answers, to be typed back to the program.
    fn output(&mut self, data: &[u8]) -> Vec<u8> {
        let mut buf = std::mem::take(&mut self.pending);
        buf.extend_from_slice(data);
        let (mut shown, mut answers, mut from, mut i) = (Vec::new(), Vec::new(), 0, 0);
        while i < buf.len() {
            if buf[i] != 0x1b {
                i += 1;
                continue;
            }
            match scan(&buf[i..]) {
                Scan::Other => i += 1,
                Scan::Partial => {
                    self.pending = buf.split_off(i);
                    break;
                }
                Scan::Query(len, q) => {
                    // the answer reflects the screen up to the query
                    self.parser.process(&buf[from..i]);
                    shown.extend_from_slice(&buf[from..i]);
                    answers.extend_from_slice(self.answer(q).as_bytes());
                    i += len;
                    from = i;
                }
            }
        }
        self.parser.process(&buf[from..]);
        shown.extend_from_slice(&buf[from..]);
        if !shown.is_empty() {
            self.broadcast(pty::frame(pty::OUTPUT, &shown));
        }
        answers
    }

    fn answer(&self, q: Query) -> String {
        match q {
            Query::Cursor => {
                let (row, col) = self.parser.screen().cursor_position();
                format!("\x1b[{};{}R", row + 1, col + 1)
            }
            Query::Status => "\x1b[0n".into(),
            Query::Primary => "\x1b[?62;22c".into(), // a VT220 with colour
            Query::Secondary => "\x1b[>1;10;0c".into(),
        }
    }

    fn broadcast(&mut self, frame: Vec<u8>) {
        self.clients.retain(|c| c.tx.try_send(frame.clone()).is_ok());
    }

    /// The screen as it is, as a SCREEN frame: the size, then the title, the lines scrolled off the main screen (so a
    /// client attaching now can scroll back through them, owner 8 Nov), the alternate screen when it is on, and the
    /// contents, cursor and input modes.
    fn screen(&self) -> Vec<u8> {
        let s = self.parser.screen();
        let (rows, cols) = s.size();
        let mut p = pty::size(rows.get(), cols.get()).to_vec();
        let title = &self.parser.callbacks().0;
        if !title.is_empty() {
            p.extend_from_slice(format!("\x1b]2;{title}\x07").as_bytes());
        }
        if !s.alternate_screen() {
            history(s, &mut p);
        }
        if s.alternate_screen() {
            p.extend_from_slice(b"\x1b[?1049h");
        }
        p.extend_from_slice(s.state_formatted().as_bytes());
        pty::frame(pty::SCREEN, &p)
    }

    /// A new client on W: the screen first when SCREEN, then everything that comes; at SIZE when given.
    fn attach(&mut self, w: UnixStream, size: Option<(u16, u16)>, screen: bool) -> u64 {
        if let Some(size) = size {
            self.resize(size);
        }
        let (tx, rx) = sync_channel::<Vec<u8>>(BACKLOG);
        std::thread::spawn(move || {
            let mut w = w;
            for f in rx {
                if w.write_all(&f).is_err() {
                    break;
                }
            }
            // dropped (gone, or too far behind): the client sees the end
            let _ = w.shutdown(std::net::Shutdown::Both);
        });
        if screen {
            let _ = tx.try_send(self.screen());
        }
        if let Some(status) = &self.exited {
            let _ = tx.try_send(pty::frame(pty::EXIT, status.as_bytes()));
        }
        self.next += 1;
        self.clients.push(Client { id: self.next, tx, size });
        self.next
    }

    /// The pseudo-terminal and the emulator at SIZE; the clients are told.
    fn resize(&mut self, (rows, cols): (u16, u16)) {
        let (r, c) = self.parser.screen().size();
        if (r.get(), c.get()) == (rows, cols) {
            return;
        }
        let ws = winsize(rows, cols);
        unsafe { libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ, &ws as *const libc::winsize) };
        self.parser.set_size(nz(rows), nz(cols));
        self.broadcast(pty::frame(pty::SIZE, &pty::size(rows, cols)));
    }

    /// Client ID typed: its size becomes the terminal's.
    fn latest(&mut self, id: u64) {
        if let Some(size) = self.clients.iter().find(|c| c.id == id).and_then(|c| c.size) {
            self.resize(size);
        }
    }

    /// What closing a terminal window does: SIGHUP to the program's session and its foreground job.
    fn hangup(&self) {
        let fg = unsafe { libc::tcgetpgrp(self.master.as_raw_fd()) };
        unsafe { libc::kill(-self.child, libc::SIGHUP) };
        if fg > 0 && fg != self.child {
            unsafe { libc::kill(-fg, libc::SIGHUP) };
        }
    }

    fn info(&self) -> Info {
        let (rows, cols) = self.parser.screen().size();
        Info {
            name: self.name.clone(),
            pid: std::process::id() as i32,
            child: self.child,
            command: self.command.clone(),
            rows: rows.get(),
            cols: cols.get(),
            title: self.parser.callbacks().0.clone(),
            exited: self.exited.clone(),
        }
    }
}

/// The lines scrolled off screen S, oldest first, appended to OUT as a terminal of S's size would show them; then
/// enough newlines that the last of them leave the screen too, into the history, before the screen is drawn over.
fn history(s: &vt100::Screen, out: &mut Vec<u8>) {
    let n = s.scrollback_len();
    if n == 0 {
        return;
    }
    let (rows, cols) = s.size();
    let mut view = s.clone();
    let mut off = n;
    while off > 0 {
        // scrolled back OFF lines, the screen's top rows are history lines n - OFF onwards
        view.set_scrollback(off);
        let take = off.min(rows.get() as usize);
        for row in view.rows_formatted(0, cols.get()).take(take) {
            out.extend_from_slice(b"\x1b[m");
            out.extend_from_slice(row.as_bytes());
            out.extend_from_slice(b"\r\n");
        }
        off -= take;
    }
    out.extend_from_slice(b"\x1b[m");
    // the cursor is on the line after the last one written: ROWS - 1 more lines scroll every written line off
    out.extend_from_slice("\r\n".repeat(rows.get() as usize - 1).as_bytes());
}

/// One client connection, until it closes.
fn serve(term: &Mutex<Term>, input: &File, s: UnixStream, keys: &std::path::Path) {
    let Ok(mut r) = s.try_clone() else { return };
    let mut me = None;
    // who is at the other end, for `NAME.keys`: its pid and cgroup, as the kernel says (SO_PEERCRED)
    let pid = peer_pid(&s);
    let cgroup = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap_or_default().lines().find_map(|l| l.strip_prefix("0::")).unwrap_or("").to_string();
    // an answer goes through the client's writer once it is attached, so frames never interleave
    let reply = |me: Option<u64>, f: Vec<u8>| match me {
        Some(id) => {
            if let Some(c) = term.lock().unwrap().clients.iter().find(|c| c.id == id) {
                let _ = c.tx.try_send(f);
            }
        }
        None => {
            let _ = (&s).write_all(&f);
        }
    };
    let size = |p: &[u8]| pty::parse_size(p).filter(|&(r, c)| r > 0 && c > 0);
    while let Ok((kind, p)) = pty::read(&mut r) {
        match kind {
            pty::ATTACH | pty::HANGUP if me.is_some() => {} // one subscription per connection
            pty::ATTACH => {
                let Ok(w) = s.try_clone() else { break };
                me = Some(term.lock().unwrap().attach(w, size(&p), true));
            }
            pty::HANGUP => {
                let Ok(w) = s.try_clone() else { break };
                let mut t = term.lock().unwrap();
                t.hangup();
                me = Some(t.attach(w, None, false)); // to hear the EXIT
            }
            pty::KEYS => {
                // who typed: a line when another sender than the last types, and after each pause of a second
                {
                    let mut last = LAST_KEYS.lock().unwrap();
                    if !last.is_some_and(|(p, t)| p == pid && t.elapsed() < Duration::from_secs(1)) {
                        let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
                        let line = serde_json::json!({"ms": ms as u64, "pid": pid, "cgroup": cgroup}).to_string() + "\n";
                        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(keys) {
                            let _ = f.write_all(line.as_bytes());
                        }
                    }
                    *last = Some((pid, std::time::Instant::now()));
                }
                // one writer at a time with the holder's own line (type_due)
                let _typing = TYPING.lock().unwrap();
                {
                    let mut t = term.lock().unwrap();
                    t.confirm = None; // someone answers it themselves
                    if let Some(id) = me {
                        t.latest(id);
                    }
                    t.keyed = Some(std::time::Instant::now());
                }
                let mut w = input;
                let _ = w.write_all(&p);
            }
            pty::COMMAND => {
                // the latest replaces one not typed yet; typed by watch_busy's look
                let line: Vec<u8> = p.into_iter().filter(|b| *b != b'\r' && *b != b'\n').collect();
                term.lock().unwrap().due = (!line.is_empty()).then_some(line);
            }
            pty::PASTE => term.lock().unwrap().pastes.push_back(p),
            pty::RESIZE => {
                if let Some(sz) = size(&p) {
                    let mut t = term.lock().unwrap();
                    if let Some(c) = t.clients.iter_mut().find(|c| Some(c.id) == me) {
                        c.size = Some(sz);
                    }
                    t.resize(sz);
                }
            }
            pty::TEXT => {
                let text = term.lock().unwrap().parser.screen().contents();
                reply(me, pty::frame(pty::TEXT, text.as_bytes()));
            }
            pty::INFO => {
                let info = term.lock().unwrap().info();
                reply(me, pty::frame(pty::INFO, serde_json::to_string(&info).unwrap_or_default().as_bytes()));
            }
            _ => {}
        }
    }
    if let Some(id) = me {
        term.lock().unwrap().clients.retain(|c| c.id != id);
    }
}

/// Held while a client's keys or the holder's own line are written to the program (`serve`, [`type_due`]).
static TYPING: Mutex<()> = Mutex::new(());

/// The last sender of keys to this terminal (its pid) and when (`serve`).
static LAST_KEYS: Mutex<Option<(i32, std::time::Instant)>> = Mutex::new(None);

/// The pid of the process at the other end of S, 0 when the kernel does not say.
fn peer_pid(s: &UnixStream) -> i32 {
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let r = unsafe { libc::getsockopt(s.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, &mut cred as *mut libc::ucred as *mut libc::c_void, &mut len) };
    if r == 0 { cred.pid } else { 0 }
}

/// The program's output, until no one holds the terminal open any more.
fn read_output(term: &Arc<Mutex<Term>>, input: &Arc<File>, mut master: File) {
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = match master.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return, // EIO: the program's side is closed
        };
        let (answers, seen) = {
            let mut t = term.lock().unwrap();
            let a = t.output(&buf[..n]);
            t.drawn = Some(std::time::Instant::now());
            let seen = !t.confirming && t.confirm.as_ref().is_some_and(|c| t.parser.screen().contents().contains(c.as_str()));
            t.confirming |= seen;
            (a, seen)
        };
        if seen {
            // Enter once the dialog has drawn and reads its keys, unless someone typed meanwhile (KEYS clears it)
            let (term, input) = (term.clone(), input.clone());
            std::thread::spawn(move || {
                std::thread::sleep(CONFIRM_AFTER);
                if term.lock().unwrap().confirm.take().is_some() {
                    let _ = (&*input).write_all(b"\r");
                }
            });
        }
        if !answers.is_empty() {
            let _ = (&**input).write_all(&answers);
        }
    }
}

/// The program ended: its last screen to `NAME.last` and, with STATUS, to the clients.
fn finish(term: &Mutex<Term>, coord: &str, status: String) {
    let mut t = term.lock().unwrap();
    let text = format!("{}\n\n[{}: {} ended: {status}]\n", t.parser.screen().contents().trim_end(), t.name, t.command.join(" "));
    let last = pty::last(coord, &t.name);
    let tmp = last.with_extension("last.tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, &last);
    }
    t.exited = Some(status.clone());
    t.broadcast(pty::frame(pty::EXIT, status.as_bytes()));
}

/// Wait for CHILD; how it ended, as text.
fn wait(child: i32) -> String {
    let mut st = 0;
    loop {
        let r = unsafe { libc::waitpid(child, &mut st, 0) };
        if r == child {
            break;
        }
        if r < 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
            return format!("unknown ({})", std::io::Error::last_os_error());
        }
    }
    if libc::WIFEXITED(st) {
        format!("exited with {}", libc::WEXITSTATUS(st))
    } else if libc::WIFSIGNALED(st) {
        format!("killed by signal {}", libc::WTERMSIG(st))
    } else {
        format!("status {st}")
    }
}

/// COMMAND on a new pseudo-terminal of that size, in a session of its own with the terminal as its controlling one.
/// The master and the program's pid, or why it did not start (exec's error included).
fn spawn(command: &[String], rows: u16, cols: u16) -> Result<(File, i32), String> {
    let argv: Vec<CString> = command.iter().map(|a| CString::new(a.as_bytes())).collect::<Result<_, _>>().map_err(|_| "COMMAND holds a NUL byte".to_string())?;
    let mut ptrs: Vec<*const libc::c_char> = argv.iter().map(|a| a.as_ptr()).collect();
    ptrs.push(std::ptr::null());
    let (mut m, mut s) = (0, 0);
    let ws = winsize(rows, cols);
    if unsafe { libc::openpty(&mut m, &mut s, std::ptr::null_mut(), std::ptr::null(), &ws) } != 0 {
        return Err(format!("openpty: {}", std::io::Error::last_os_error()));
    }
    // SAFETY: no other thread runs yet
    unsafe {
        std::env::set_var("TERM", "xterm-256color");
        std::env::set_var("COLORTERM", "truecolor");
    }
    // exec's own error comes back through this pipe, which a successful exec closes
    let (err_r, err_w) = pipe();
    match unsafe { libc::fork() } {
        -1 => Err(format!("fork: {}", std::io::Error::last_os_error())),
        0 => unsafe {
            libc::close(m);
            libc::close(err_r.as_raw_fd());
            libc::setsid();
            libc::ioctl(s, libc::TIOCSCTTY, 0);
            for fd in 0..3 {
                libc::dup2(s, fd);
            }
            if s > 2 {
                libc::close(s);
            }
            // the program starts with default signals, whatever its caller ignored or blocked (Rust ignores SIGPIPE;
            // a shell's background job, SIGINT and SIGQUIT)
            for sig in [libc::SIGPIPE, libc::SIGINT, libc::SIGQUIT, libc::SIGTERM, libc::SIGHUP, libc::SIGTSTP, libc::SIGTTIN, libc::SIGTTOU, libc::SIGCHLD] {
                libc::signal(sig, libc::SIG_DFL);
            }
            let mut none: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut none);
            libc::sigprocmask(libc::SIG_SETMASK, &none, std::ptr::null_mut());
            libc::execvp(ptrs[0], ptrs.as_ptr());
            let msg = format!("{}: {}", command[0], std::io::Error::last_os_error());
            libc::write(err_w.as_raw_fd(), msg.as_ptr().cast(), msg.len());
            libc::_exit(127)
        },
        pid => {
            unsafe { libc::close(s) };
            drop(err_w);
            let mut why = String::new();
            let _ = File::from(err_r).read_to_string(&mut why);
            if why.is_empty() {
                Ok((unsafe { File::from_raw_fd(m) }, pid))
            } else {
                unsafe {
                    libc::waitpid(pid, std::ptr::null_mut(), 0);
                    libc::close(m);
                }
                Err(why)
            }
        }
    }
}

/// The dialog on screen TEXT, if one is: a numbered option with Claude Code's pointer (`❯ 1.`). Its title (the nearest
/// line above its options, its frame taken off) and the screen's last 20 lines.
fn dialog(text: &str) -> Option<(String, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let first = lines.iter().position(|l| {
        let l = l.trim_start_matches(['│', ' ']);
        l.starts_with("❯ 1.") || l.starts_with("› 1.")
    })?;
    let clean = |l: &str| l.trim().trim_matches(['│', '╭', '╮', '╰', '╯', '─', ' ']).trim().to_string();
    let title = lines[..first].iter().rev().map(|l| clean(l)).find(|l| !l.is_empty()).unwrap_or_default();
    let screen = lines[lines.len().saturating_sub(20)..].iter().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n");
    Some((title, screen.trim().to_string()))
}

/// The flag Claude Code's start notice about channels begins with (bin/roster passes it the coord channel, `server:coord`).
const CHANNELS_FLAG: &str = "--dangerously-load-development-channels";

/// Claude Code's word on screen TEXT that this session takes no channel message, if its start notice says so: a line
/// not indented that begins with [`CHANNELS_FLAG`] once its left bar (`▎`) is taken off, says it is "ignored", "blocked by org
/// policy" or "waiting for your organization's policy" and names `server:coord` (the coord channel, bin/roster), and
/// the line under it says why ("Channels are not currently available": Claude Code 2.1.331 when its flag for channels
/// is off, which it is until the session's login has fetched its flags; "Channels are not available on third-party
/// providers"; "Inbound messages will be silently dropped"; "This network does not serve it, so messages are dropped
/// this session"). The reason line, else the start line itself. Only the lines over the conversation are read: Claude
/// Code prints the notice at start, under its logo and over its first `❯` or `●` line at the left edge (accounts-lead's
/// walk, N-1446 p2423, path/walk/run/shots/06-term-try2), and everything from that line on is what was said, a resumed
/// session's too, so a tool's output (under its `● Bash(…)`) is never read as the notice. paper-research N-1446 p2424:
/// a session that drops what it is handed is told, never shown as handed; p2485: from its start notice only.
fn channels_off(text: &str) -> Option<String> {
    let clean = |l: &str| l.trim().trim_matches(['▎', '│', ' ']).trim().to_string();
    let raw: Vec<&str> = text.lines().take_while(|l| !l.starts_with(['❯', '●', '⏺'])).collect();
    let lines: Vec<String> = raw.iter().map(|l| clean(l)).collect();
    // the notice is not indented: its left bar, or the flag, at the left edge. The rows on screen can begin inside a
    // tool's output, its `● Bash(…)` scrolled off (a resumed session's), whose lines are indented under its `⎿`
    // (paper-research N-1446 p2500)
    let unindented = |at: usize| raw[at].starts_with('▎') || raw[at].starts_with(CHANNELS_FLAG);
    let said = [" ignored (", " blocked by org policy (", " waiting for your organization's policy ("];
    let names_coord = |l: &str| {
        let Some(rest) = l.strip_prefix(CHANNELS_FLAG) else { return false };
        let Some(s) = said.iter().find(|s| rest.starts_with(**s)) else { return false };
        rest[s.len()..].split(')').next().is_some_and(|servers| servers.split([',', ' ']).any(|n| n == "server:coord"))
    };
    let at = (0..lines.len()).find(|&at| unindented(at) && names_coord(&lines[at]))?;
    let start = &lines[at];
    // the reason: what follows the start line's closing bracket on it, else the next line with words
    let after = start.rsplit_once(')').map(|(_, r)| r.trim()).unwrap_or_default();
    let reason = if after.is_empty() { lines[at + 1..].iter().take(3).find(|l| !l.is_empty()).cloned().unwrap_or_default() } else { after.to_string() };
    Some(if reason.is_empty() { start.clone() } else { reason })
}

/// Claude Code's word that this session takes no channel message ([`channels_off`]), as its start notice gives it: kept
/// once read (the notice scrolls away), and read only until the session's first turn has been seen, the look that
/// sees it included. Claude Code prints the notice at start; what shows later (a tool's output, the logo and notices
/// drawn again) takes nothing new (paper-research N-1446 p2485).
#[derive(Default)]
struct StartNotice {
    off: Option<String>,
    turn_seen: bool,
}

impl StartNotice {
    /// One look at SCREEN, BUSY when it shows a turn running.
    fn look(&mut self, screen: &str, busy: bool) {
        if self.off.is_none() && !self.turn_seen {
            self.off = channels_off(screen);
        }
        self.turn_seen |= busy;
    }
}

/// Whether screen TEXT shows a turn running. Claude Code shows a spinner line above its input box while a turn runs:
/// a glyph, a word ending in '…', then the time it has run in brackets (`✢ Crafting… (1m 42s · ↓ 769 tokens)`); older
/// versions said "esc to interrupt" there. Only the lines just above the input box's top rule are looked at (a todo
/// list may sit between), so words in the conversation never count; a finished turn's line (`✻ Worked for 1m 42s`)
/// has no '…' and does not. docket-edge m1145: the footer of three lines this looked at once no longer has either.
fn busy(text: &str) -> bool {
    above_box(text).into_iter().take(12).any(|l| l.contains("esc to interrupt") || spinner(l))
}

/// The lines of screen TEXT above Claude Code's input box ([`box_top`]), the nearest first, empty ones left out.
fn above_box(text: &str) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().collect();
    lines[..box_top(&lines)].iter().rev().filter(|l| !l.trim().is_empty()).copied().collect()
}

/// Where Claude Code's input box begins among screen LINES: its top rule (the last rule but one, its bottom being the
/// last), or the only rule; the screen's end when there is none.
fn box_top(lines: &[&str]) -> usize {
    let rule = |l: &str| {
        let t = l.trim();
        t.chars().count() >= 20 && t.chars().all(|c| c == '─' || c == '━')
    };
    let rules: Vec<usize> = lines.iter().enumerate().filter(|(_, l)| rule(l)).map(|(i, _)| i).collect();
    match rules.len() {
        0 => lines.len(),
        1 => rules[0],
        n => rules[n - 2],
    }
}

/// Claude Code's error lines for a sign-in missing or spent, as 2.1.331 writes them in place of a turn's answer.
const NO_SIGN_IN: [&str; 8] = [
    "Not logged in · Please run /login",
    "Authentication required · Sign in again to continue",
    "Invalid API key · Fix external API key",
    "OAuth token revoked · Please run /login",
    "Login expired · Please run /login",
    "Login reached its time limit · Please run /login",
    "Invalid authentication · Run /login to sign in with your claude.ai account",
    "Your organization has disabled API key authentication · Sign in again with your claude.ai account",
];

/// Claude Code's word just above its input box that the turn ended on a sign-in missing or spent ([`NO_SIGN_IN`], a line
/// of its own whole; accounts-lead N-1446 p2446: a new agent on a coord with no sign-in read "working" for good), as
/// Claude Code writes it: its own `●` line, or its answer (`  ⎿  `) directly under the prompt (`❯`, with the prompt's
/// wrapped lines under it); never a tool's output, which stands under its `● Bash(…)` line (paper-research N-1446
/// p2485). Only the few lines nearest the box, as for a running turn ([`busy`]), so the words in the conversation never
/// count; read at each look, so a /login typed in its terminal ends it.
fn no_sign_in(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let said = |at: usize| -> Option<String> {
        let line = lines[at];
        let words = match line.strip_prefix("● ").or_else(|| line.strip_prefix("⏺ ")) {
            Some(words) => words,
            None => {
                let words = line.strip_prefix("  ⎿")?;
                // the block an answer belongs to: the nearest line above at the left edge, or another answer (lines
                // indented under them, and empty ones, go on)
                let head = lines[..at].iter().rev().find(|l| !l.trim().is_empty() && (!l.starts_with(' ') || l.trim_start().starts_with('⎿')))?;
                if !head.starts_with('❯') {
                    return None;
                }
                words
            }
        };
        NO_SIGN_IN.iter().find(|s| **s == words.trim()).map(|s| s.to_string())
    };
    (0..box_top(&lines)).rev().filter(|&at| !lines[at].trim().is_empty()).take(6).find_map(said)
}

/// A spinner line: a glyph (not a letter or digit), a word ending in '…', then '(' and a time run (`12s`, `1m 42s`).
fn spinner(l: &str) -> bool {
    let t = l.trim();
    let mut chars = t.chars();
    if !chars.next().is_some_and(|c| !c.is_alphanumeric() && !c.is_whitespace()) {
        return false;
    }
    let rest = chars.as_str().trim_start();
    let Some((word, after)) = rest.split_once('…') else { return false };
    if word.is_empty() || word.contains(char::is_whitespace) && word.split_whitespace().count() > 3 {
        return false;
    }
    let Some(inner) = after.trim_start().strip_prefix('(') else { return false };
    let first = inner.split([' ', '·', ')']).next().unwrap_or("");
    first.len() >= 2 && first[..first.len() - 1].bytes().all(|b| b.is_ascii_digit()) && matches!(first.as_bytes()[first.len() - 1], b's' | b'm' | b'h')
}

/// Look at the footer every [`BUSY_EVERY`] and tell coordd each change of whether a turn runs; idle only once it held
/// for [`IDLE_AFTER`].
fn watch_busy(term: &Mutex<Term>, input: &File, mut trust: trust::Trust, coord: &Path) {
    let (mut told, mut idle_since): (Option<bool>, Option<std::time::Instant>) = (None, None);
    // told again every minute unchanged, so a coordd that restarted (and lost it) learns it soon
    let mut told_at = std::time::Instant::now();
    // the dialog on the screen since when, and the title of the last one told
    let (mut standing, mut popup_told): (Option<(String, std::time::Instant)>, Option<String>) = (None, None);
    // Claude Code's word that this session takes no channel message, from its start notice ([`StartNotice`]), and as told
    let (mut notice, mut chan_told): (StartNotice, Option<String>) = (StartNotice::default(), None);
    // its word that the turn ended on a sign-in missing or spent, as told (each look reads it afresh)
    let mut login_told: Option<String> = None;
    loop {
        std::thread::sleep(BUSY_EVERY);
        let (name, now, found, login, asked, paste_ready) = {
            let t = term.lock().unwrap();
            if t.exited.is_some() {
                return;
            }
            let contents = t.parser.screen().contents();
            let asked = trust.look(&contents, std::time::Instant::now());
            // the start dialog --confirm answers is not one to tell, while it is being answered
            // and a trust popup the holder does not answer, so it shows as waiting rather than as nothing
            let found = dialog(&contents).or_else(|| trust::drawn(&contents)).filter(|_| t.confirm.as_ref().is_none_or(|c| !contents.contains(c.as_str())));
            let now = busy(&contents);
            notice.look(&contents, now);
            let paste_ready = paste_ready(t.parser.screen().bracketed_paste(), t.confirm.is_some(), &contents);
            (t.name.clone(), now, found, no_sign_in(&contents), asked, paste_ready)
        };
        match asked {
            Some(trust::Act::Press(keys, head, dir)) => {
                let _typing = TYPING.lock().unwrap();
                trust::press(input, &keys);
                trust::log(coord, &name, &head, &dir);
            }
            Some(trust::Act::Tell(title, screen)) => {
                let name = name.clone();
                std::thread::spawn(move || tell_popup(name, title, screen));
            }
            None => {}
        }
        let chan_off = notice.off.clone();
        match found {
            Some((title, screen)) => {
                if standing.as_ref().is_none_or(|(t, _)| *t != title) {
                    standing = Some((title.clone(), std::time::Instant::now()));
                }
                let stood = standing.as_ref().is_some_and(|(_, since)| since.elapsed() >= POPUP_STANDS);
                if stood && popup_told.as_ref() != Some(&title) {
                    popup_told = Some(title.clone());
                    let name = name.clone();
                    std::thread::spawn(move || tell_popup(name, title, screen));
                }
            }
            None => (standing, popup_told) = (None, None),
        }
        let state = if now {
            idle_since = None;
            true
        } else {
            let since = *idle_since.get_or_insert_with(std::time::Instant::now);
            if told == Some(true) && since.elapsed() < IDLE_AFTER { true } else { false }
        };
        if (told != Some(state) || chan_told != chan_off || login_told != login || told_at.elapsed() >= Duration::from_secs(60)) && tell(&Request::Busy { name, busy: state, channels_off: chan_off.clone(), no_sign_in: login.clone() }) {
            told = Some(state);
            chan_told = chan_off.clone();
            login_told = login.clone();
            told_at = std::time::Instant::now();
        }
        // a line to type, once no turn runs (an end held for IDLE_AFTER) and no dialog stands
        if !state && standing.is_none() && !trust.standing() {
            type_due(term, input);
        }
        if paste_ready {
            type_paste(term, input);
        }
    }
}

/// Type the line a client gave (`Term::due`), with its Enter, if the input box is empty now: nothing in it of
/// anyone's ([`box_empty`]) and every key sent drawn since (`keyed`, `drawn`). TYPING is held from the look to the
/// write, so no client's key comes in between; a key sent the same instant as the look, before the program read it,
/// is the one case this cannot see.
fn type_due(term: &Mutex<Term>, input: &File) {
    let _typing = TYPING.lock().unwrap();
    let line = {
        let mut t = term.lock().unwrap();
        let drawn_since = match (t.keyed, t.drawn) {
            (Some(k), Some(d)) => d > k,
            (Some(_), None) => false,
            (None, _) => true,
        };
        if t.due.is_none() || t.exited.is_some() || !drawn_since || busy(&t.parser.screen().contents()) || !box_empty(t.parser.screen()) {
            return;
        }
        t.keyed = Some(std::time::Instant::now());
        t.due.take().unwrap_or_default()
    };
    let mut w = input;
    let _ = w.write_all(&[line.as_slice(), b"\r"].concat());
}

/// Whether the program reads a paste on screen CONTENTS: it said so (bracketed paste on, BRACKETED), and neither its start
/// dialog (CONFIRMING), another dialog, nor a trust popup stands, one the holder answers or one it only tells.
fn paste_ready(bracketed: bool, confirming: bool, contents: &str) -> bool {
    bracketed && !confirming && dialog(contents).is_none() && trust::ask(contents).is_none() && trust::drawn(contents).is_none()
}

/// Type the oldest words given as a paste (`Term::pastes`), with no client's key written in between.
fn type_paste(term: &Mutex<Term>, input: &File) {
    let _typing = TYPING.lock().unwrap();
    let Some(words) = term.lock().unwrap().pastes.pop_front() else { return };
    let mut w = input;
    let _ = w.write_all(&paste(&words));
}

/// WORDS as one bracketed paste and Enter. Their ESC bytes are taken out, so no part of them can end the paste early
/// and be typed as keys.
fn paste(words: &[u8]) -> Vec<u8> {
    let inner = words.iter().copied().filter(|b| *b != 0x1b);
    b"\x1b[200~".iter().copied().chain(inner).chain(b"\x1b[201~\r".iter().copied()).collect()
}

/// Whether screen S shows Claude Code's input box with nothing typed in it: the rows between its two rules (the last
/// two rows of `─`, the top one may carry the agent's name) begin with its prompt `❯`, and every other mark in them is
/// the caret (inverse) or the faint suggestion Claude Code shows there (dim: `❯ ` then the suggestion's first letter
/// inverse, the rest dim). Anything else, a letter of normal weight, a dialog, the `!` of its shell mode, is someone's.
fn box_empty(s: &vt100::Screen) -> bool {
    let (rows, cols) = s.size();
    let (rows, cols) = (rows.get(), cols.get());
    let row_text = |r: u16| (0..cols).filter_map(|c| s.cell(r, c)).map(|c| c.contents()).collect::<String>();
    let rule = |r: u16| {
        let t = row_text(r);
        let t = t.trim();
        t.starts_with('─') && t.chars().filter(|c| *c == '─').count() >= 20
    };
    let rules: Vec<u16> = (0..rows).filter(|r| rule(*r)).collect();
    let [.., top, bottom] = rules[..] else { return false };
    if bottom <= top + 1 || !row_text(top + 1).trim_start().starts_with('❯') {
        return false;
    }
    let mut prompt = false;
    for r in top + 1..bottom {
        for c in 0..cols {
            let Some(cell) = s.cell(r, c) else { continue };
            let t = cell.contents();
            if t.trim().is_empty() {
                continue;
            }
            if !prompt && t == "❯" {
                prompt = true;
                continue;
            }
            if !(cell.dim() || cell.inverse()) {
                return false;
            }
        }
    }
    true
}

/// Send REQ to coordd and wait for its answer, for as long as coordd holds the connection (no time limit: the owner,
/// N-68; a coordd that ends closes it); whether it answered.
fn tell(req: &Request) -> bool {
    let path = std::env::var("COORDD_SOCK").unwrap_or_else(|_| coord_types::SOCKET.into());
    let Ok(mut s) = UnixStream::connect(&path) else { return false };
    if writeln!(s, "{}", serde_json::to_string(req).unwrap_or_default()).is_err() {
        return false;
    }
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).is_ok_and(|n| n > 0)
}

/// Tell coordd of a dialog on NAME's screen (`Request::Popup`); nothing more when coordd does not answer.
fn tell_popup(name: String, title: String, screen: String) {
    tell(&Request::Popup { name, title, screen });
}

/// Ask coordd to put this process in the cgroup `pty`.
fn enter_pty_cgroup() -> Result<(), String> {
    let path = std::env::var("COORDD_SOCK").unwrap_or_else(|_| coord_types::SOCKET.into());
    let mut s = UnixStream::connect(&path).map_err(|e| format!("coordd is not running ({path}: {e}); the holder stays in its caller's cgroup"))?;
    writeln!(s, "{}", serde_json::to_string(&Request::Pty).unwrap_or_default()).map_err(|e| format!("coordd: {e}"))?;
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).map_err(|e| format!("coordd: {e}"))?;
    let r: Reply = serde_json::from_str(&line).map_err(|_| format!("coordd: bad reply {line:?}"))?;
    if r.ok { Ok(()) } else { Err(r.out) }
}

/// What the holder at SOCK runs, if one answers: a live holder is waited for (no time limit: the owner, N-68; one
/// that did not answer in 2 s was taken for none, and a second holder started in its place); a dead one refuses the
/// connection or closes it.
fn info_of(sock: &Path) -> Option<Info> {
    let mut s = UnixStream::connect(sock).ok()?;
    s.write_all(&pty::frame(pty::INFO, &[])).ok()?;
    match pty::read(&mut s).ok()? {
        (pty::INFO, p) => serde_json::from_slice(&p).ok(),
        _ => None,
    }
}

fn pipe() -> (OwnedFd, OwnedFd) {
    let mut fds = [0; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        fail(format!("pipe: {}", std::io::Error::last_os_error()))
    }
    unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) }
}

fn winsize(rows: u16, cols: u16) -> libc::winsize {
    libc::winsize { ws_row: rows, ws_col: cols, ws_xpixel: 0, ws_ypixel: 0 }
}

fn nz(v: u16) -> NonZeroU16 {
    NonZeroU16::new(v).unwrap_or(NonZeroU16::MIN)
}

#[cfg(test)]
mod tests {
    /// consult-lead-b, N-1863 p13: a paste waits while a trust popup stands, answered or only told, and goes once it is gone.
    #[test]
    fn a_paste_waits_while_a_trust_popup_stands_and_goes_after() {
        let popup = "Adding a new directory:\n/x\n\nThis session hasn't worked here before.\n\n> No, exit\n  Yes, add it\n\nEnter to confirm · Esc to cancel\n";
        let below = format!("{popup}  a line under its footer\n");
        let gone = "● Added /x\n\n────────\n❯ \n────────\n  ? for shortcuts\n";
        assert!(super::trust::ask(popup).is_some() && super::trust::drawn(&below).is_some());
        assert!(!super::paste_ready(true, false, popup), "one the holder answers");
        assert!(!super::paste_ready(true, false, &below), "one it only tells");
        assert!(super::paste_ready(true, false, gone), "typed once it went");
        assert!(!super::paste_ready(false, false, gone) && !super::paste_ready(true, true, gone), "as before: bracketed paste on, no start dialog");
    }

    #[test]
    fn words_holding_a_paste_end_stay_inside_the_one_paste() {
        let typed = super::paste(b"hi\x1b[201~\r/clear\x1b[20\x1b[201~1~");
        assert_eq!(typed, b"\x1b[200~hi[201~\r/clear[20[201~1~\x1b[201~\r".to_vec());
        assert_eq!(typed.iter().filter(|&&b| b == 0x1b).count(), 2, "only the paste's own start and end");
    }

    use super::*;

    const RULE: &str = "────────────────────────────────────────────────────────────────────────────────";

    fn screen(above: &[&str]) -> String {
        let mut l: Vec<&str> = vec!["● I'll look at the median function.", ""];
        l.extend_from_slice(above);
        l.extend_from_slice(&[RULE, "> ", RULE, "  ? for shortcuts", "  ⏵⏵ auto mode on (shift+tab to cycle)"]);
        l.join("\n")
    }

    #[test]
    fn a_spinner_above_the_input_box_is_a_running_turn() {
        // docket-edge m1145: Claude Code's screen with a turn running
        assert!(busy(&screen(&["✢ Crafting… (1m 42s · ↓ 769 tokens)"])));
        assert!(busy(&screen(&["· Thinking… (12s)"])));
        // a todo list under the spinner
        assert!(busy(&screen(&["✻ Computing… (3s · ↑ 20 tokens)", "  ⎿  ☐ write the test", "     ☐ run it"])));
        // older versions
        assert!(busy(&screen(&["✻ Pondering… (esc to interrupt)"])));
    }

    #[test]
    fn a_finished_turn_or_words_in_the_conversation_are_not() {
        assert!(!busy(&screen(&[])));
        assert!(!busy(&screen(&["✻ Worked for 1m 42s"])));
        // the same words higher up, in what was said, and the spinner text below the box (never there) do not count
        let mut far = vec!["✢ Crafting… (1m 42s · ↓ 769 tokens)"];
        far.extend(std::iter::repeat_n("text", 13));
        assert!(!busy(&screen(&far)));
        assert!(!busy(&screen(&["he said: wait… (later)"])));
    }

    /// Claude Code 2.1.331's screen as it starts (accounts-lead's walk, N-1446 p2423, path/walk/run/shots/06-term-try2):
    /// its logo, its NOTICES with their left bar, the standing agent's first prompt wrapped over two lines, what was SAID
    /// after it, then the input box.
    fn start_screen(notices: &[&str], said: &[&str]) -> String {
        let mut l = vec![" ▐▛███▜▌   Claude Code v2.1.331", "▝▜█████▛▘  Haiku 5.5 · API Usage Billing", "  ▘▘ ▝▝    /home/t/agent-coord/path-d/work", ""];
        l.extend_from_slice(notices);
        l.extend_from_slice(&["", "❯ You are a standing agent, started by the owner from yaaihh's New agent to try things out. There is no task yet: wait", "  for messages and answer each one as it asks.", ""]);
        l.extend_from_slice(said);
        l.extend_from_slice(&["", RULE, "❯ ", RULE, "  ⏵⏵ auto mode on (shift+tab to cycle)"]);
        l.join("\n")
    }

    const START: &str = "--dangerously-load-development-channels ignored (server:coord)";

    /// Claude Code's start notice when it takes no channel message (accounts-lead's walk, N-1446 p2423: a session in a
    /// scratch HOME, signed in nowhere; the reasons as Claude Code 2.1.331 writes them): the reason is read, under the
    /// start line or after it on the same line, with the notice's left bar taken off; a session with no such notice, the
    /// words alone, or another server's, read nothing.
    #[test]
    fn claude_codes_word_that_it_takes_no_channel_message_is_read_with_its_reason() {
        let bar = |l: &str| format!("▎ {l}");
        let off = start_screen(&[&bar(START), &bar("Channels are not currently available")], &["● Hello."]);
        assert_eq!(channels_off(&off).as_deref(), Some("Channels are not currently available"));
        // before the first turn: nothing said yet, the input box's prompt the first `❯`
        let before = [" ▐▛███▜▌   Claude Code v2.1.331", "", bar(START).as_str(), bar("Channels are not available on third-party providers").as_str(), "", RULE, "❯ ", RULE].join("\n");
        assert_eq!(channels_off(&before).as_deref(), Some("Channels are not available on third-party providers"));
        let org = "--dangerously-load-development-channels blocked by org policy (server:coord)";
        assert_eq!(channels_off(&start_screen(&[&bar(org), &bar("Inbound messages will be silently dropped")], &[])).as_deref(), Some("Inbound messages will be silently dropped"));
        assert_eq!(channels_off(&start_screen(&[&bar(&format!("{START} Channels are not currently available"))], &[])).as_deref(), Some("Channels are not currently available"));
        assert_eq!(channels_off(&start_screen(&[START, "Channels are not currently available"], &[])).as_deref(), Some("Channels are not currently available"), "no bar");
        // a session that takes them shows no such notice; the words alone, or another server's, read nothing
        assert_eq!(channels_off(&start_screen(&[], &[])), None);
        assert_eq!(channels_off(&start_screen(&[&bar("he said the channels were ignored (no idea why)")], &[])), None);
        assert_eq!(channels_off(&start_screen(&[&bar("--dangerously-load-development-channels ignored (server:other)"), &bar("Channels are not currently available")], &[])), None);
    }

    /// paper-research N-1446 p2485: the notice's words as a tool's output read nothing: the `⎿` form under its
    /// `● Bash(…)` line, a line further down its output that begins with the flag, this file's own test line; and with
    /// nothing said above them, neither the `⎿` form nor the test line begins with the flag.
    #[test]
    fn the_channels_notice_as_a_tools_output_reads_nothing() {
        let grep = format!("  ⎿  {START}");
        assert_eq!(channels_off(&start_screen(&[], &["● Bash(grep -rh \"development-channels\" notes)", &grep, "     Channels are not currently available"])), None);
        let further = format!("     {START}");
        assert_eq!(channels_off(&start_screen(&[], &["● Bash(cat notes.txt)", "  ⎿  what the walk saw:", &further, "     Channels are not currently available"])), None);
        let own = format!("        const START: &str = \"{START}\";");
        assert_eq!(channels_off(&start_screen(&[], &["● Bash(grep -n \"const START\" crates/coord-pty/src/main.rs)", &format!("  ⎿  {}", own.trim_start()), &own])), None);
        assert_eq!(channels_off(&[grep.as_str(), "     Channels are not currently available", RULE, "❯ ", RULE].join("\n")), None);
        assert_eq!(channels_off(&[own.as_str(), RULE, "❯ ", RULE].join("\n")), None);
        // p2500: rows that begin inside a tool's output, its `● Bash(…)` scrolled off after a --resume, nothing said
        // above: a line 5 spaces in that begins with the flag is not the notice
        assert_eq!(channels_off(&[further.as_str(), "     Channels are not currently available", "", "● Done.", "", RULE, "❯ ", RULE].join("\n")), None);
    }

    /// paper-research N-1446 p2485: the notice is read until the session's first turn has been seen (on that look
    /// too) and kept once its lines scroll away; after it, a notice shown again takes nothing new.
    #[test]
    fn the_start_notice_is_read_until_the_first_turn_is_seen() {
        let notice = start_screen(&[&format!("▎ {START}"), "▎ Channels are not currently available"], &[]);
        let quiet = start_screen(&[], &["● Hello."]);
        let mut at_start = StartNotice::default();
        at_start.look(&notice, false);
        at_start.look(&quiet, true);
        at_start.look(&quiet, false);
        assert_eq!(at_start.off.as_deref(), Some("Channels are not currently available"), "read at start, kept");
        let mut with_turn = StartNotice::default();
        with_turn.look(&notice, true);
        assert_eq!(with_turn.off.as_deref(), Some("Channels are not currently available"), "read on the look that sees the first turn");
        let mut later = StartNotice::default();
        later.look(&quiet, false);
        later.look(&quiet, true);
        later.look(&quiet, false);
        later.look(&notice, false);
        assert_eq!(later.off, None, "after the first turn nothing new");
    }

    /// A turn that ended on Claude Code's sign-in error (accounts-lead N-1446 p2446: "Not logged in · Please run /login"
    /// on a coord with no sign-in): read from the line above the input box, whole; the same words in the conversation
    /// higher up, or inside a longer line, read nothing.
    #[test]
    fn a_turn_ended_on_claude_codes_sign_in_error_is_read_from_above_the_input_box() {
        let rule = "─".repeat(40);
        let failed = screen(&["❯ You are a standing agent", "  ⎿  Not logged in · Please run /login", "", &rule, "❯ ", &rule]);
        assert_eq!(no_sign_in(&failed).as_deref(), Some("Not logged in · Please run /login"));
        let revoked = screen(&["❯ hello", "● OAuth token revoked · Please run /login", &rule, "❯ ", &rule]);
        assert_eq!(no_sign_in(&revoked).as_deref(), Some("OAuth token revoked · Please run /login"));
        // the standing agent's prompt wrapped over two lines (06-term-try2): the answer under its last
        let prompt = "❯ You are a standing agent, started by the owner from yaaihh's New agent to try things out. There is no task yet: wait";
        let wrapped = screen(&[prompt, "  for messages and answer each one as it asks.", "  ⎿  Not logged in · Please run /login", "", &rule, "❯ ", &rule]);
        assert_eq!(no_sign_in(&wrapped).as_deref(), Some("Not logged in · Please run /login"));
        // the words in a longer line, or far above, or with no turn ended on them, read nothing
        assert_eq!(no_sign_in(&screen(&["● It said Not logged in · Please run /login earlier", &rule, "❯ ", &rule])), None);
        let far = ["● Not logged in · Please run /login", "a", "b", "c", "d", "e", "f", "g"];
        assert_eq!(no_sign_in(&screen(&[&far[..], &[rule.as_str(), "❯ ", rule.as_str()]].concat())), None);
        assert_eq!(no_sign_in(&screen(&["● done", &rule, "❯ ", &rule])), None);
    }

    /// paper-research N-1446 p2485: the sign-in words as a tool's output read nothing, just above the input box as they
    /// are: the `⎿` form under its `● Bash(…)` line, a line further down its output, this file's own test line, and that
    /// line's words printed as they stand (a `⎿` inside the tool's).
    #[test]
    fn the_sign_in_words_as_a_tools_output_read_nothing() {
        let rule = "─".repeat(40);
        let boxed = |said: &[&str]| screen(&[said, &[rule.as_str(), "❯ ", rule.as_str()]].concat());
        assert_eq!(no_sign_in(&boxed(&["❯ what did it say?", "● Bash(grep -rh \"Please run /login\" notes)", "  ⎿  Not logged in · Please run /login"])), None);
        assert_eq!(no_sign_in(&boxed(&["● Bash(cat notes.txt)", "  ⎿  the walk:", "     Not logged in · Please run /login"])), None);
        let own = "        let failed = screen(&[\"❯ You are a standing agent\", \"  ⎿  Not logged in · Please run /login\", \"\", &rule, \"❯ \", &rule]);";
        assert_eq!(no_sign_in(&boxed(&["● Bash(grep -n \"let failed\" crates/coord-pty/src/main.rs)", &format!("  ⎿  {}", own.trim_start())])), None);
        assert_eq!(no_sign_in(&boxed(&["❯ print the fixture", "● Bash(printf '  ⎿  Not logged in · Please run /login\\n')", "  ⎿    ⎿  Not logged in · Please run /login"])), None);
    }

    /// The input box as Claude Code 2.1.328 draws it (N-804.p16, docket-edge3's try of 19 Nov, its bytes): `❯`, a
    /// no-break space, then the caret as an inverse space when empty; a suggestion's first letter inverse and the rest
    /// dim (its placeholder: `h(n[0]) + dim(n.slice(1))`); typed text in the normal weight.
    #[test]
    fn the_input_box_is_empty_when_only_the_caret_or_the_faint_suggestion_is_in_it() {
        let shown = |input: &str| {
            let mut p = vt100::Parser::new(nz(12), nz(80), 0);
            let top = format!("{} docket-edge5 ─", "─".repeat(60));
            let bytes = format!("● stub reply\r\n\r\n✻ Cooked for 14s\r\n\r\n{top}\r\n{input}\r\n{}\r\n  Opus 5.5 | 219k / 1.0m\r\n", "─".repeat(80));
            p.process(bytes.as_bytes());
            box_empty(p.screen())
        };
        assert!(shown("❯\u{a0}\x1b[7m \x1b[27m"), "empty: the caret only");
        assert!(shown("❯\u{a0}\x1b[7mh\x1b[27m\x1b[2mand 4a55f79 to docket-edge once the exe builds\x1b[22m"), "the faint suggestion is no one's");
        assert!(!shown("❯\u{a0}abc\x1b[7m \x1b[27m"), "typed, the caret after it");
        assert!(!shown("❯\u{a0}\x1b[7ma\x1b[27mbc"), "typed, the caret moved to its start");
        assert!(!shown("!\u{a0}\x1b[7m \x1b[27m"), "its shell mode");
        let mut p = vt100::Parser::new(nz(12), nz(80), 0);
        p.process("● working\r\n❯ 1. Yes\r\n  2. No\r\n".as_bytes());
        assert!(!box_empty(p.screen()), "no input box: a dialog");
        // a second line of the box, typed
        let mut p = vt100::Parser::new(nz(12), nz(80), 0);
        p.process(format!("{}\r\n❯\u{a0}\r\n  more\x1b[7m \x1b[27m\r\n{}\r\n", "─".repeat(80), "─".repeat(80)).as_bytes());
        assert!(!box_empty(p.screen()), "a second line typed");
    }
}
