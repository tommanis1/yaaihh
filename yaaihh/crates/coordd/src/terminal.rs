//! coordc's side of the agents' terminals (coord_types::pty): `attach` puts one in the caller's terminal, `keys`,
//! `screen`, `hangup` and `terms` serve roster, and [`Relay`] is the bridge's: it attaches yaaihh to any number of
//! terminals at once and turns their frames into `{"pty": …}` lines.

use coord_types::pty::{self, Info};
use coord_types::{PtyEvent, Reply};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// The key that detaches `attach` (Ctrl-\, as dtach's).
const DETACH: u8 = 0x1c;

fn coord() -> String {
    std::env::var("COORD").unwrap_or_else(|_| {
        eprintln!("coord: COORD is not set");
        std::process::exit(2)
    })
}

fn holder(name: &str) -> std::io::Result<UnixStream> {
    UnixStream::connect(pty::socket(&coord(), name))
}

/// NAME's holder, or the end with why there is none and the last screen it left.
fn holder_or_exit(name: &str) -> UnixStream {
    holder(name).unwrap_or_else(|e| {
        let last = pty::last(&coord(), name);
        match std::fs::read_to_string(&last) {
            Ok(text) => {
                eprintln!("coord: {name} has no terminal ({e}); its last screen, from {}:", last.display());
                print!("{text}");
            }
            Err(_) => eprintln!("coord: {name} has no terminal ({e})"),
        }
        std::process::exit(1)
    })
}

/// The local terminal's size, (0, 0) if it has none.
fn local_size() -> (u16, u16) {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    if unsafe { libc::ioctl(0, libc::TIOCGWINSZ, &mut ws as *mut libc::winsize) } != 0 {
        return (0, 0);
    }
    (ws.ws_row, ws.ws_col)
}

fn send(w: &Mutex<UnixStream>, kind: u8, payload: &[u8]) -> bool {
    w.lock().unwrap().write_all(&pty::frame(kind, payload)).is_ok()
}

/// The local terminal's modes before `attach` made it raw.
static SAVED: OnceLock<libc::termios> = OnceLock::new();
/// Whether the program has the local terminal on its alternate screen.
static ALT: AtomicBool = AtomicBool::new(false);

/// Leave `attach`: the local terminal as it was (modes, attributes, cursor, main screen), then WHY.
fn leave(why: String) -> ! {
    let mut reset = String::from("\x1b[0m\x1b[?25h\x1b[?1l\x1b>\x1b[?2004l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l");
    if ALT.load(Ordering::Relaxed) {
        reset += "\x1b[?1049l";
    }
    let mut out = std::io::stdout();
    let _ = out.write_all(reset.as_bytes()).and_then(|_| out.flush());
    if let Some(t) = SAVED.get() {
        unsafe { libc::tcsetattr(0, libc::TCSANOW, t) };
    }
    eprintln!("\r\n[{why}]");
    std::process::exit(0)
}

/// Follow the program's switches to and from the alternate screen in B (the last one wins).
fn track_alt(b: &[u8]) {
    let last = |pats: [&[u8]; 3]| pats.iter().filter_map(|p| b.windows(p.len()).rposition(|w| w == *p)).max();
    let on = last([&b"\x1b[?1049h"[..], &b"\x1b[?1047h"[..], &b"\x1b[?47h"[..]]);
    let off = last([&b"\x1b[?1049l"[..], &b"\x1b[?1047l"[..], &b"\x1b[?47l"[..]]);
    match (on, off) {
        (Some(on), Some(off)) => ALT.store(on > off, Ordering::Relaxed),
        (Some(_), None) => ALT.store(true, Ordering::Relaxed),
        (None, Some(_)) => ALT.store(false, Ordering::Relaxed),
        (None, None) => {}
    }
}

/// `coord attach NAME`: NAME's terminal in this one, both ways, at this one's size (it follows SIGWINCH), until
/// Ctrl-\ detaches or the program ends.
pub fn attach(name: &str) -> ! {
    let s = holder_or_exit(name);
    let mut saved: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(0, &mut saved) } != 0 {
        eprintln!("coord attach: stdin is not a terminal");
        std::process::exit(2)
    }
    let _ = SAVED.set(saved);
    let mut raw = saved;
    unsafe {
        libc::cfmakeraw(&mut raw);
        libc::tcsetattr(0, libc::TCSANOW, &raw);
    }
    // SIGWINCH is taken by one thread's sigwait, so it is blocked before any thread starts
    let mut winch: libc::sigset_t = unsafe { std::mem::zeroed() };
    unsafe {
        libc::sigemptyset(&mut winch);
        libc::sigaddset(&mut winch, libc::SIGWINCH);
        libc::sigprocmask(libc::SIG_BLOCK, &winch, std::ptr::null_mut());
    }
    let Ok(mut r) = s.try_clone() else { leave(format!("{name}: the terminal's holder closed the connection")) };
    let w = Arc::new(Mutex::new(s));
    let (rows, cols) = local_size();
    if !send(&w, pty::ATTACH, &pty::size(rows, cols)) {
        leave(format!("{name}: the terminal's holder closed the connection"))
    }
    let who = name.to_string();
    std::thread::spawn(move || {
        loop {
            let data = match pty::read(&mut r) {
                Ok((pty::SCREEN, p)) => p.get(4..).unwrap_or_default().to_vec(),
                Ok((pty::OUTPUT, p)) => p,
                Ok((pty::EXIT, p)) => leave(format!("{who}: {}", String::from_utf8_lossy(&p))),
                Ok(_) => continue,
                Err(_) => leave(format!("{who}: the terminal's holder closed the connection")),
            };
            track_alt(&data);
            let mut out = std::io::stdout();
            if out.write_all(&data).and_then(|_| out.flush()).is_err() {
                leave(format!("{who}: this terminal's output closed"))
            }
        }
    });
    {
        let w = w.clone();
        std::thread::spawn(move || {
            loop {
                let mut sig = 0;
                if unsafe { libc::sigwait(&winch, &mut sig) } == 0 && sig == libc::SIGWINCH {
                    let (rows, cols) = local_size();
                    if !send(&w, pty::RESIZE, &pty::size(rows, cols)) {
                        return;
                    }
                }
            }
        });
    }
    let mut buf = [0u8; 4096];
    let mut stdin = std::io::stdin();
    loop {
        let n = match stdin.read(&mut buf) {
            Ok(0) | Err(_) => leave(format!("{name}: this terminal's input closed")),
            Ok(n) => n,
        };
        let typed = &buf[..n];
        let cut = typed.iter().position(|&b| b == DETACH);
        let keys = &typed[..cut.unwrap_or(n)];
        if !keys.is_empty() && !send(&w, pty::KEYS, keys) {
            leave(format!("{name}: the terminal's holder closed the connection"))
        }
        if cut.is_some() {
            leave(format!("detached from {name}"))
        }
    }
}

/// `coordc keys NAME TEXT`: type TEXT into NAME's terminal.
pub fn keys(name: &str, text: &str) -> ! {
    let mut s = holder_or_exit(name);
    match s.write_all(&pty::frame(pty::KEYS, text.as_bytes())) {
        Ok(()) => std::process::exit(0),
        Err(e) => {
            eprintln!("coord: {name}'s terminal: {e}");
            std::process::exit(1)
        }
    }
}

/// `coordc screen NAME`: NAME's screen as text.
pub fn screen(name: &str) -> ! {
    let mut s = holder_or_exit(name);
    let text = s.write_all(&pty::frame(pty::TEXT, &[])).and_then(|_| pty::read(&mut s));
    match text {
        Ok((pty::TEXT, p)) => {
            println!("{}", String::from_utf8_lossy(&p));
            std::process::exit(0)
        }
        _ => {
            eprintln!("coord: {name}'s terminal did not answer");
            std::process::exit(1)
        }
    }
}

/// `coordc hangup NAME`: hang up NAME's terminal, as closing its window does, and wait for its program to end. No
/// terminal: nothing to do.
pub fn hangup(name: &str) -> ! {
    let Ok(mut s) = holder(name) else { std::process::exit(0) };
    let _ = s.set_read_timeout(Some(Duration::from_secs(15)));
    if s.write_all(&pty::frame(pty::HANGUP, &[])).is_err() {
        std::process::exit(0) // its holder is just ending
    }
    loop {
        match pty::read(&mut s) {
            Ok((pty::EXIT, _)) => std::process::exit(0),
            Ok(_) => {}
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                eprintln!("coord: {name} still runs 15 s after its terminal hung up");
                std::process::exit(1)
            }
            Err(_) => std::process::exit(0),
        }
    }
}

/// `coordc terms`: each terminal: name, size, the title its program set, its command (and its end, once it ended).
pub fn terms() -> ! {
    let mut names: Vec<String> = std::fs::read_dir(pty::dir(&coord()))
        .map(|d| d.flatten().filter_map(|e| e.file_name().to_str()?.strip_suffix(".sock").map(String::from)).collect())
        .unwrap_or_default();
    names.sort();
    for name in names {
        let Ok(mut s) = holder(&name) else { continue }; // a holder that died without removing its socket
        let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
        let Ok((pty::INFO, p)) = s.write_all(&pty::frame(pty::INFO, &[])).and_then(|_| pty::read(&mut s)) else { continue };
        let Ok(i) = serde_json::from_slice::<Info>(&p) else { continue };
        let ended = i.exited.map(|e| format!("\t(ended: {e})")).unwrap_or_default();
        println!("{}\t{}x{}\t{}\t{}{ended}", i.name, i.cols, i.rows, i.title, i.command.join(" "));
    }
    std::process::exit(0)
}

/// The bridge's terminals: one connection per attached agent, its frames printed as `{"pty": …}` lines.
pub struct Relay {
    out: Arc<Mutex<std::io::Stdout>>,
    /// Each attached agent's connection, with the attach it belongs to.
    attached: Arc<Mutex<HashMap<String, (u64, UnixStream)>>>,
    next: AtomicU64,
}

impl Relay {
    pub fn new(out: Arc<Mutex<std::io::Stdout>>) -> Relay {
        Relay { out, attached: Arc::default(), next: AtomicU64::new(0) }
    }

    /// Answer a terminal request (Attach, Keys, Resize, Detach).
    pub fn handle(&self, req: coord_types::Request) -> Reply {
        use coord_types::Request::*;
        let fail = |out: String| Reply { ok: false, out };
        match req {
            Attach { name, rows, cols } => {
                self.detach(&name);
                let mut s = match holder(&name) {
                    Ok(s) => s,
                    Err(e) => return self.last_screen(&name, e),
                };
                // the screen goes out before the reply, so a reader holding the reply holds the screen
                let screen = s.write_all(&pty::frame(pty::ATTACH, &pty::size(rows, cols))).and_then(|_| pty::read(&mut s));
                let (Ok((pty::SCREEN, p)), Ok(w)) = (screen, s.try_clone()) else {
                    return fail(format!("{name}: the terminal's holder closed the connection"));
                };
                let (rows, cols) = pty::parse_size(&p).unwrap_or((24, 80));
                self.emit(PtyEvent::Screen { name: name.clone(), rows, cols, data: p.get(4..).unwrap_or_default().to_vec() });
                let id = self.next.fetch_add(1, Ordering::Relaxed);
                self.attached.lock().unwrap().insert(name.clone(), (id, w));
                let (out, attached) = (self.out.clone(), self.attached.clone());
                std::thread::spawn(move || {
                    loop {
                        let ev = match pty::read(&mut s) {
                            Ok((pty::OUTPUT, data)) => PtyEvent::Output { name: name.clone(), data },
                            Ok((pty::SIZE, p)) => match pty::parse_size(&p) {
                                Some((rows, cols)) => PtyEvent::Size { name: name.clone(), rows, cols },
                                None => continue,
                            },
                            Ok((pty::EXIT, p)) => PtyEvent::Exit { name: name.clone(), status: String::from_utf8_lossy(&p).into_owned() },
                            Ok(_) => continue,
                            Err(_) => break,
                        };
                        print(&out, json!({ "pty": ev }));
                    }
                    // the connection ended: forget it, unless a newer attach took the name
                    let mut a = attached.lock().unwrap();
                    if a.get(&name).is_some_and(|(i, _)| *i == id) {
                        a.remove(&name);
                    }
                });
                Reply { ok: true, out: String::new() }
            }
            Keys { name, data } => self.send(&name, pty::KEYS, &data),
            Resize { name, rows, cols } => self.send(&name, pty::RESIZE, &pty::size(rows, cols)),
            Detach { name } => {
                self.detach(&name);
                Reply { ok: true, out: String::new() }
            }
            _ => fail("coord bridge: not a terminal request".into()),
        }
    }

    fn detach(&self, name: &str) {
        if let Some((_, s)) = self.attached.lock().unwrap().remove(name) {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
    }

    /// A frame to NAME's terminal: through its attached connection, else on one of its own.
    fn send(&self, name: &str, kind: u8, payload: &[u8]) -> Reply {
        let f = pty::frame(kind, payload);
        let sent = match self.attached.lock().unwrap().get(name) {
            Some((_, s)) => (&*s).write_all(&f),
            None => holder(name).and_then(|mut s| s.write_all(&f)),
        };
        match sent {
            Ok(()) => Reply { ok: true, out: String::new() },
            Err(e) => Reply { ok: false, out: format!("{name}'s terminal: {e}") },
        }
    }

    /// No holder for NAME: the last screen it left, if any, as a screen that has ended.
    fn last_screen(&self, name: &str, e: std::io::Error) -> Reply {
        let Ok(text) = std::fs::read_to_string(pty::last(&coord(), name)) else {
            return Reply { ok: false, out: format!("{name} has no terminal ({e})") };
        };
        let rows = text.lines().count().clamp(1, u16::MAX as usize) as u16;
        let cols = text.lines().map(|l| l.chars().count()).max().unwrap_or(0).clamp(1, u16::MAX as usize) as u16;
        let data = text.trim_end().replace('\n', "\r\n").into_bytes();
        self.emit(PtyEvent::Screen { name: name.into(), rows, cols, data });
        self.emit(PtyEvent::Exit { name: name.into(), status: "not running".into() });
        Reply { ok: false, out: format!("{name} has no terminal ({e}); its last screen is shown") }
    }

    fn emit(&self, ev: PtyEvent) {
        print(&self.out, json!({ "pty": ev }));
    }
}

/// One line on the bridge's stdout; a closed stdout (yaaihh gone) ends the bridge.
pub fn print(out: &Mutex<std::io::Stdout>, v: Value) {
    let mut o = out.lock().unwrap();
    if writeln!(o, "{v}").and_then(|_| o.flush()).is_err() {
        std::process::exit(0)
    }
}
