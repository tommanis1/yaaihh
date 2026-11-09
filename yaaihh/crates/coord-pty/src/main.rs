//! coord-pty: the holder of one agent's terminal
//! .
//!
//!   coord-pty NAME [--rows R] [--cols C] [--confirm TEXT] -- COMMAND...
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

/// How long after the `--confirm` text appears its Enter is pressed.
const CONFIRM_AFTER: Duration = Duration::from_millis(500);

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
    let (mut rows, mut cols, mut confirm) = (40, 120, None);
    let mut it = opts[1..].iter();
    while let Some(o) = it.next() {
        let Some(v) = it.next() else { usage() };
        let size = || v.parse::<u16>().ok().filter(|&v| v > 0).unwrap_or_else(|| usage());
        match o.as_str() {
            "--rows" => rows = size(),
            "--cols" => cols = size(),
            "--confirm" if !v.is_empty() => confirm = Some(v.clone()),
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
        0 => hold(name, coord, rows, cols, confirm, command, File::from(ready_w)),
        _ => unsafe { libc::_exit(0) }, // the middle one; a failed fork ends here too, and the caller hears of it
    }
}

/// The holder itself, from the socket to the end of COMMAND.
fn hold(name: String, coord: String, rows: u16, cols: u16, confirm: Option<String>, command: Vec<String>, mut ready: File) -> ! {
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
    }));
    {
        let (term, input) = (term.clone(), input.clone());
        std::thread::spawn(move || {
            for s in listener.incoming().flatten() {
                let (term, input) = (term.clone(), input.clone());
                std::thread::spawn(move || serve(&term, &input, s));
            }
        });
    }
    {
        let (term, input) = (term.clone(), input.clone());
        std::thread::spawn(move || read_output(&term, &input, output));
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
fn serve(term: &Mutex<Term>, input: &File, s: UnixStream) {
    let Ok(mut r) = s.try_clone() else { return };
    let mut me = None;
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
                {
                    let mut t = term.lock().unwrap();
                    t.confirm = None; // someone answers it themselves
                    if let Some(id) = me {
                        t.latest(id);
                    }
                }
                let mut w = input;
                let _ = w.write_all(&p);
            }
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

/// What the holder at SOCK runs, if one answers.
fn info_of(sock: &Path) -> Option<Info> {
    let mut s = UnixStream::connect(sock).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
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
