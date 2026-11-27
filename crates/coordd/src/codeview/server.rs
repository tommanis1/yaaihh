//! The openvscode-server coordd runs: found again by its state file, else started detached on 127.0.0.1 from the
//! install's links (install.rs); one started otherwise is stopped, its whole process group, and so is one no window
//! has been connected to for [`IDLE`].

use super::{Ctx, install};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A running server: its process (its group's leader), port, connection token and how it was started
/// (`codeview/server.json`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Server {
    pub pid: u32,
    pub port: u16,
    pub token: String,
    #[serde(default)]
    pub how: String,
}

/// How long a server is kept with no window connected to it: a look at another project and back within it finds the
/// view warm; past it the server, about 220 MB with its extension host, is let go, and the next open waits for its
/// start (a few seconds).
pub const IDLE: Duration = Duration::from_secs(10 * 60);

/// How long a server the window has left is looked at for its last connection to close, before the idle wait has it.
pub const LEFT: Duration = Duration::from_secs(30);

/// Held while a server is found, started or stopped, so an idle stop never ends one an open was just given.
static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The program: our pinned install under HOME, else `openvscode-server` on PATH.
fn program(home: &str) -> Option<PathBuf> {
    let pinned = Path::new(home).join("agent-coord/tools/openvscode-server/current/bin/openvscode-server");
    if pinned.is_file() {
        return Some(pinned);
    }
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join("openvscode-server")).find(|b| b.is_file()))
}

/// The server that answers, started as install.rs says when none does or one started otherwise; it outlives coordd's
/// start in place and the window, and ends with coordd's unit.
pub fn ensure(ctx: &Ctx) -> Result<Server, String> {
    let bin = program(&ctx.home).ok_or("coord codeview: openvscode-server is not installed on this machine")?;
    let root = install::root(&bin)?;
    let how = install::how(&root);
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let state = ctx.dir().join("server.json");
    if let Some(s) = read(&state) {
        match alive(&s) {
            true if s.how == how => return Ok(s),
            true => stop(&s),
            false => {}
        }
    }
    let s = start(ctx, &root, how)?;
    let tmp = state.with_extension("json.new");
    std::fs::write(&tmp, serde_json::to_string(&s).unwrap_or_default()).and_then(|_| std::fs::rename(&tmp, &state)).map_err(|e| format!("{}: {e}", state.display()))?;
    Ok(s)
}

fn read(state: &Path) -> Option<Server> {
    std::fs::read_to_string(state).ok().and_then(|t| serde_json::from_str(&t).ok())
}

/// The server of codeview folder DIR stopped, and its state file removed, once no window has been connected to it
/// for IDLE: QUIET holds since when none has been, NOW is the time; whether it was stopped.
pub fn let_go(dir: &Path, quiet: &mut Option<Instant>, now: Instant) -> bool {
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let state = dir.join("server.json");
    let Some(s) = read(&state) else {
        *quiet = None;
        return false;
    };
    if connected(s.port) {
        *quiet = None;
        return false;
    }
    if now.saturating_duration_since(*quiet.get_or_insert(now)) < IDLE {
        return false;
    }
    if ours(&s) {
        stop(&s);
    }
    let _ = std::fs::remove_file(&state);
    *quiet = None;
    true
}

/// The server of codeview folder DIR stopped and its state removed if no window is connected to it; whether none is
/// left, stopped now or none there.
pub fn end_if_alone(dir: &Path) -> bool {
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let state = dir.join("server.json");
    let Some(s) = read(&state) else { return true };
    if !ours(&s) {
        let _ = std::fs::remove_file(&state);
        return true;
    }
    if connected(s.port) {
        return false;
    }
    stop(&s);
    let _ = std::fs::remove_file(&state);
    true
}

/// The server of codeview folder DIR stopped and its state removed, whoever is on it.
pub fn end(dir: &Path) {
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let state = dir.join("server.json");
    if let Some(s) = read(&state).filter(ours) {
        stop(&s);
    }
    let _ = std::fs::remove_file(&state);
}

/// Whether a connection to PORT on this machine is established: a window's page on the server.
fn connected(port: u16) -> bool {
    let local = format!(":{port:04X}");
    ["/proc/net/tcp", "/proc/net/tcp6"].iter().any(|f| {
        std::fs::read_to_string(f).unwrap_or_default().lines().skip(1).any(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            c.len() > 3 && c[1].ends_with(&local) && c[3] == "01"
        })
    })
}

/// Its process is still a server's: a process of that id running the server's code, so a signal to its group is the
/// server's.
fn ours(s: &Server) -> bool {
    let cmd = std::fs::read(format!("/proc/{}/cmdline", s.pid)).unwrap_or_default();
    let cmd = String::from_utf8_lossy(&cmd);
    cmd.contains("openvscode-server") || cmd.contains("server-main.js")
}

/// Its process is still a server's and its port answers.
fn alive(s: &Server) -> bool {
    ours(s) && listens(s.port)
}

/// The processes of group PGID, by their ids.
pub fn group(pgid: u32) -> Vec<u32> {
    let Ok(d) = std::fs::read_dir("/proc") else { return Vec::new() };
    d.filter_map(|e| e.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter(|p| std::fs::read_to_string(format!("/proc/{p}/stat")).ok().and_then(|t| t.rsplit_once(") ").map(|(_, r)| r.split(' ').nth(2) == Some(&pgid.to_string()))).unwrap_or(false))
        .collect()
}

/// S's whole group ended: asked to, then made to after 3 s.
pub fn stop(s: &Server) {
    let pg = -(s.pid as i32);
    // SAFETY: signals to the server's own process group, which it leads
    unsafe { libc::kill(pg, libc::SIGTERM) };
    let until = Instant::now() + Duration::from_secs(3);
    while !group(s.pid).is_empty() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
    if !group(s.pid).is_empty() {
        // SAFETY: as above
        unsafe { libc::kill(pg, libc::SIGKILL) };
    }
}

fn listens(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(200)).is_ok()
}

fn token() -> Result<String, String> {
    let mut b = [0u8; 16];
    std::io::Read::read_exact(&mut std::fs::File::open("/dev/urandom").map_err(|e| e.to_string())?, &mut b).map_err(|e| e.to_string())?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

/// ROOT's server started from its links in its own process group, so a restart of coordd leaves it running; waited
/// for until it listens.
fn start(ctx: &Ctx, root: &Path, how: String) -> Result<Server, String> {
    let dir = ctx.dir();
    let app = dir.join("app");
    install::link(root, &app)?;
    let port = std::net::TcpListener::bind(("127.0.0.1", 0)).and_then(|l| l.local_addr()).map(|a| a.port()).map_err(|e| format!("no free port on 127.0.0.1: {e}"))?;
    let token = token()?;
    // the token in a file of its own, not on a command line any process may read; the log names it too
    let secret = |f: &str, text: &str| {
        let p = dir.join(f);
        std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&p).and_then(|mut h| std::io::Write::write_all(&mut h, text.as_bytes())).map(|_| p).map_err(|e| format!("{f}: {e}"))
    };
    let token_file = secret("token", &token)?;
    let log = secret("server.log", "").and_then(|p| std::fs::OpenOptions::new().append(true).open(&p).map_err(|e| format!("server.log: {e}")))?;
    let err = log.try_clone().map_err(|e| format!("server.log: {e}"))?;
    let mut child = Command::new(app.join("node"))
        .args(["--preserve-symlinks-main", "--preserve-symlinks"])
        .arg(app.join("out/server-main.js"))
        .args(["--host", "127.0.0.1", "--port", &port.to_string(), "--telemetry-level", "off"])
        .args(install::FLAGS)
        .arg("--connection-token-file")
        .arg(&token_file)
        .arg("--server-data-dir")
        .arg(dir.join("server"))
        .arg("--extensions-dir")
        .arg(dir.join("extensions"))
        .env("YAAIHH_CODEVIEW_DIR", &dir)
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(err)
        .process_group(0)
        .spawn()
        .map_err(|e| format!("{}: {e}", app.join("node").display()))?;
    let until = Instant::now() + Duration::from_secs(30);
    while !listens(port) {
        if let Ok(Some(st)) = child.try_wait() {
            stop(&Server { pid: child.id(), port, token: String::new(), how: String::new() });
            return Err(format!("openvscode-server ended at its start ({st}); {} says why", dir.join("server.log").display()));
        }
        if Instant::now() > until {
            stop(&Server { pid: child.id(), port, token: String::new(), how: String::new() });
            let _ = child.wait();
            return Err("openvscode-server did not listen within 30 s".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let pid = child.id();
    // reaped in the background so it never lingers as a zombie of coordd's
    std::thread::spawn(move || child.wait());
    Ok(Server { pid, port, token, how })
}
