//! The pipe to one box's coordd: `coordc bridge` run in the box (`box.sh sh BOX coordc bridge`). Its stdout carries
//! the event stream, the replies and the output of the terminals it attached; requests go to its stdin. When the pipe ends (the box or coordd stopped), it is
//! started again after a pause, and the new stream opens with a snapshot.

use coord_types::{Event, PtyEvent, Reply, Request};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The pause before starting a failed pipe again: from PAUSE_MIN, doubling to PAUSE_MAX while it fails at once.
const PAUSE_MIN: Duration = Duration::from_secs(3);
const PAUSE_MAX: Duration = Duration::from_secs(60);
/// A pipe that ran this long was up, not failing at once: the next pause is short again.
const RAN: Duration = Duration::from_secs(30);

/// What the pipe brought.
pub enum Incoming {
    Event(Event),
    Reply(u64, Reply),
    /// An attached terminal's screen, output, size or end.
    Pty(PtyEvent),
    /// The pipe ended; why, as far as its stderr says.
    Down(String),
}

pub struct Link {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    rx: Receiver<Incoming>,
    next: u64,
}

impl Link {
    /// Keep CMD running in the background; WAKE is called whenever something arrives.
    pub fn start(cmd: Vec<String>, wake: rune::Waker) -> Link {
        let (tx, rx) = channel();
        let stdin = Arc::new(Mutex::new(None));
        let shared = stdin.clone();
        std::thread::spawn(move || {
            // the pause doubles while the pipe keeps failing at once, up to a minute, and is short again after a pipe
            // that ran: a box whose coordd is gone was retried every 3 s, each try an `sbx exec` opening dozens of
            // Secret Service sessions, which crashed gnome-keyring every hour or two
            let mut pause = PAUSE_MIN;
            loop {
                let began = std::time::Instant::now();
                let why = run(&cmd, &shared, &tx, &wake);
                *shared.lock().unwrap() = None;
                if tx.send(Incoming::Down(why)).is_err() {
                    return;
                }
                wake();
                pause = if began.elapsed() > RAN { PAUSE_MIN } else { (pause * 2).min(PAUSE_MAX) };
                std::thread::sleep(pause);
            }
        });
        Link { stdin, rx, next: 0 }
    }

    /// Send REQ; its reply comes back with the returned id. None while the pipe is down.
    pub fn ask(&mut self, req: &Request) -> Option<u64> {
        self.next += 1;
        let mut guard = self.stdin.lock().unwrap();
        let w = guard.as_mut()?;
        writeln!(w, "{}", json!({"id": self.next, "req": req})).and_then(|_| w.flush()).ok()?;
        Some(self.next)
    }

    pub fn drain(&self) -> Vec<Incoming> {
        self.rx.try_iter().collect()
    }
}

/// One run of CMD: until its stdout ends.
fn run(cmd: &[String], stdin: &Mutex<Option<ChildStdin>>, tx: &Sender<Incoming>, wake: &rune::Waker) -> String {
    let child = Command::new(&cmd[0]).args(&cmd[1..]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return format!("{}: {e}", cmd[0]),
    };
    *stdin.lock().unwrap() = child.stdin.take();
    let name = cmd.join(" ");
    crate::log::line(&format!("pipe up: {name}"));
    // what the pipe brings, logged once a minute: a stream that floods the window shows here
    let (mut minute, mut bytes, mut lines) = (std::time::Instant::now(), 0usize, 0usize);
    for line in BufReader::new(child.stdout.take().expect("piped")).lines() {
        let Ok(line) = line else { break };
        (bytes, lines) = (bytes + line.len(), lines + 1);
        if minute.elapsed() >= Duration::from_secs(60) {
            crate::log::line(&format!("pipe {name}: {} KB in {lines} lines this minute", bytes / 1024));
            (minute, bytes, lines) = (std::time::Instant::now(), 0, 0);
        }
        if let Some(i) = parse(&line) {
            if tx.send(i).is_err() {
                break;
            }
            wake();
        }
    }
    *stdin.lock().unwrap() = None;
    let mut err = String::new();
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut err);
    }
    let status = child.wait().map(|s| s.to_string()).unwrap_or_default();
    let err = err.trim();
    crate::log::line(&format!("pipe down: {name} ({status}){}", if err.is_empty() { String::new() } else { format!(": {}", err.lines().last().unwrap_or(err)) }));
    if err.is_empty() { format!("the pipe to coordd ended ({status})") } else { err.lines().last().unwrap_or(err).to_string() }
}

fn parse(line: &str) -> Option<Incoming> {
    let v: Value = serde_json::from_str(line).ok()?;
    if let Some(e) = v.get("event") {
        return serde_json::from_value(e.clone()).ok().map(Incoming::Event);
    }
    if let Some(t) = v.get("pty") {
        return serde_json::from_value(t.clone()).ok().map(Incoming::Pty);
    }
    let id = v.get("id")?.as_u64()?;
    serde_json::from_value(v.get("reply")?.clone()).ok().map(|r| Incoming::Reply(id, r))
}

/// Run CMD once with REQS (ids 1, 2, …), to its end: the snapshot and their replies. For the headless scenes, which
/// draw a box's real state in one frame.
pub fn once(cmd: &[String], reqs: &[Request]) -> Vec<Incoming> {
    let Ok(mut child) = Command::new(&cmd[0]).args(&cmd[1..]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()
    else {
        return Vec::new();
    };
    if let Some(mut w) = child.stdin.take() {
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
    let out: Vec<Incoming> = BufReader::new(stdout).lines().map_while(Result::ok).filter_map(|l| parse(&l)).collect();
    let _ = child.lock().unwrap().wait();
    out
}
