//! `coordd-v2 act SOCKET NAME ARG...`: a run of Act NAME(ARG...) asked for on the daemon's SOCKET, as the party the
//! table vouches this process's cgroup for, by a process started in a session's own scope. It asks again each
//! [`EVERY`] until the daemon starts the run and the run lives, each try a line of its log, for as long as another
//! process of its cgroup lives; then it holds that one run's connection until the daemon closes it or its cgroup has
//! no other process left. A cgroup it cannot read is logged and taken as one that lives. It holds no Act's name of
//! its own: whoever starts it names the Act.

use serde_json::{json, Value as Json};
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

/// The time between two tries, and between two looks at the cgroup while a run is held.
pub const EVERY: Duration = Duration::from_secs(1);

/// What one try came to.
enum Tried {
    /// The run lives, named so; the connection that asked for it.
    Taken(String, BufReader<UnixStream>),
    /// Not taken, and why.
    Not(String),
}

/// Asks for NAME(ARGS) on `socket` until a run of it lives, then holds it; `alone` says the cgroup has no other
/// process, or why that could not be read (logged, and taken as a scope that lives).
pub fn act(socket: &Path, name: &str, args: &[String], log: &mut dyn Write, alone: &dyn Fn() -> Result<bool, String>) {
    let request = json!({"do": "act", "name": name, "args": args}).to_string() + "\n";
    let ended = |log: &mut dyn Write| match alone() {
        Ok(a) => a,
        Err(why) => {
            let _ = writeln!(log, "act: {name}: the scope's processes not read: {why}");
            false
        }
    };
    let mut conn = loop {
        if ended(log) {
            let _ = writeln!(log, "act: {name}: the scope has no other process, so nothing is asked");
            return;
        }
        match try_once(socket, &request) {
            Tried::Taken(run, conn) => {
                let _ = writeln!(log, "act: {name}: run {run} lives; held");
                break conn;
            }
            Tried::Not(why) => {
                let _ = writeln!(log, "act: {name}: not taken: {why}");
                std::thread::sleep(EVERY);
            }
        }
    };
    let _ = conn.get_ref().set_read_timeout(Some(EVERY));
    let mut line = String::new();
    loop {
        line.clear();
        match conn.read_line(&mut line) {
            Ok(0) => {
                let _ = writeln!(log, "act: {name}: the daemon closed the connection");
                return;
            }
            Ok(_) => {}
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) => {
                let _ = writeln!(log, "act: {name}: {e}");
                return;
            }
        }
        if ended(log) {
            let _ = writeln!(log, "act: {name}: the scope has no other process; the run is left to the daemon");
            return;
        }
    }
}

/// One request on a new connection, and its first reply that is not a record line.
fn try_once(socket: &Path, request: &str) -> Tried {
    let mut s = match UnixStream::connect(socket) {
        Ok(s) => s,
        Err(e) => return Tried::Not(format!("{}: {e}", socket.display())),
    };
    if let Err(e) = s.write_all(request.as_bytes()) {
        return Tried::Not(e.to_string());
    }
    let mut conn = BufReader::new(s);
    let mut line = String::new();
    loop {
        line.clear();
        match conn.read_line(&mut line) {
            Ok(0) => return Tried::Not("the daemon closed the connection with no reply".into()),
            Err(e) => return Tried::Not(e.to_string()),
            Ok(_) => {}
        }
        let reply: Json = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => return Tried::Not(format!("a reply that is not JSON: {e}")),
        };
        match reply["is"].as_str() {
            Some("line") => continue,
            Some("done") => {
                let v = &reply["value"];
                // a run that rests, waits or is ready lives; one done, raised or cancelled already is no run held
                let live = matches!(v["state"].as_str(), Some("ready" | "waiting")) || v["state"].get("resting").is_some();
                let run = v["run"].as_str().unwrap_or_default().to_string();
                return if live { Tried::Taken(run, conn) } else { Tried::Not(format!("run {run} is {}", v["state"])) };
            }
            _ => return Tried::Not(reply["why"].as_str().map_or_else(|| reply.to_string(), str::to_string)),
        }
    }
}

/// Whether the cgroup of this process holds no other process (cgroup v2's `cgroup.procs`), or why it was not read.
pub fn alone() -> Result<bool, String> {
    let me = std::process::id();
    let cgroup = crate::peer::cgroup_of(me as i32).map_err(|e| format!("/proc/{me}/cgroup: {e}"))?;
    let procs = format!("/sys/fs/cgroup{cgroup}/cgroup.procs");
    let procs = std::fs::read_to_string(&procs).map_err(|e| format!("{procs}: {e}"))?;
    Ok(procs.lines().filter_map(|p| p.trim().parse::<u32>().ok()).all(|p| p == me))
}
