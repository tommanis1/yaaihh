//! `Model`: one request to a party's model, run as the clerk's command with the party's name, answered later.

use crate::Config;
use docket_v2::run::driver::Reply;
use docket_v2::run::value::Value;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The most of an answer the record carries, as compute's answers are bounded.
pub const ANSWER_BYTES: usize = 256 * 1024;

/// What a model's answer costs, in std.budgets' dimensions: tokens as its command counts them, wall_ms by the clerk.
pub const MEASURES: [&str; 2] = ["tokens", "wall_ms"];

/// What the command prints: the model's text, and every token it read and wrote.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Said {
    answer: String,
    tokens: u64,
}

type Job = (u64, String, String, Arc<AtomicBool>);

/// Requests taken by as many workers as the file allows; answers queued, the daemon woken by a byte.
pub struct Asks {
    jobs: mpsc::Sender<Job>,
    rx: mpsc::Receiver<(u64, Reply)>,
    wake: UnixStream,
    running: HashMap<u64, Arc<AtomicBool>>,
}

impl Asks {
    pub fn new(config: Config) -> Result<Self, String> {
        let (wake, poke) = UnixStream::pair().map_err(|e| e.to_string())?;
        wake.set_nonblocking(true).map_err(|e| e.to_string())?;
        let (tx, rx) = mpsc::channel();
        let (jobs, queue) = mpsc::channel::<Job>();
        let (config, queue) = (Arc::new(config), Arc::new(Mutex::new(queue)));
        for _ in 0..config.at_once {
            let (config, queue, tx) = (config.clone(), queue.clone(), tx.clone());
            let mut poke = poke.try_clone().map_err(|e| e.to_string())?;
            std::thread::spawn(move || {
                while let Ok((line, party, prompt, stop)) = { queue.lock().map_err(|_| ()).and_then(|q| q.recv().map_err(|_| ())) } {
                    if stop.load(Ordering::Relaxed) {
                        continue;
                    }
                    let reply = run(&config, &party, &prompt, &stop).unwrap_or_else(Reply::Failed);
                    if tx.send((line, reply)).is_err() || poke.write_all(&[1]).is_err() {
                        break;
                    }
                }
            });
        }
        Ok(Asks { jobs, rx, wake, running: HashMap::new() })
    }

    /// The request for the `effect` line at `line`, queued.
    pub fn ask(&mut self, line: u64, party: &str, prompt: &str) -> Result<(), String> {
        let stop = Arc::new(AtomicBool::new(false));
        self.jobs.send((line, party.to_string(), prompt.to_string(), stop.clone())).map_err(|_| "no worker takes requests".to_string())?;
        self.running.insert(line, stop);
        Ok(())
    }

    pub fn stop(&mut self, line: u64) {
        if let Some(stop) = self.running.remove(&line) {
            stop.store(true, Ordering::Relaxed);
        }
    }

    pub fn waits(&self) -> Vec<RawFd> {
        vec![self.wake.as_raw_fd()]
    }

    /// The answers ready for lines still wanted; never waits.
    pub fn ready(&mut self) -> Vec<(u64, Reply)> {
        let mut b = [0u8; 64];
        while matches!((&self.wake).read(&mut b), Ok(1..)) {}
        let got: Vec<(u64, Reply)> = self.rx.try_iter().collect();
        got.into_iter().filter(|(line, _)| self.running.remove(line).is_some()).collect()
    }
}

/// The command run for `party` with `prompt` on its stdin: its stdout the answer when it exits 0, else a failure.
fn run(config: &Config, party: &str, prompt: &str, stop: &AtomicBool) -> Result<Reply, String> {
    let (program, args) = config.command.split_first().ok_or("no command")?;
    let mut c = Command::new(program);
    c.args(args).arg(party).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).process_group(0);
    let started = Instant::now();
    let mut child = c.spawn().map_err(|e| format!("{program}: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let prompt = prompt.to_string();
    let writer = std::thread::spawn(move || stdin.write_all(prompt.as_bytes()));
    let (out, err) = (drain(child.stdout.take().ok_or("no stdout")?), drain(child.stderr.take().ok_or("no stderr")?));
    let deadline = Instant::now() + Duration::from_secs(config.seconds);
    let ends = loop {
        if ended(child.id()) {
            break Ok(());
        }
        if stop.load(Ordering::Relaxed) {
            break Err("stopped: its run no longer wants the answer".to_string());
        }
        if Instant::now() >= deadline {
            break Err(format!("no answer from {party}'s model in {} s", config.seconds));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    // SAFETY: the child is not yet reaped, so its group's id is no one else's; what it left running ends, pipes close
    unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
    let status = child.wait().map_err(|e| e.to_string())?;
    let _ = writer.join();
    let (out, err) = (out.join().unwrap_or_default(), err.join().unwrap_or_default());
    if let Err(why) = ends {
        return Ok(Reply::Failed(why));
    }
    if !status.success() {
        let said = String::from_utf8_lossy(&err);
        let tail: String = said.trim_end().chars().rev().take(2000).collect::<Vec<_>>().into_iter().rev().collect();
        return Ok(Reply::Failed(format!("{party}'s model: {program} {status}: {tail}")));
    }
    if out.len() > ANSWER_BYTES {
        return Ok(Reply::Failed(format!("{party}'s model answered over the {ANSWER_BYTES} bytes an answer carries")));
    }
    let said: Said = match serde_json::from_slice(&out) {
        Ok(s) => s,
        Err(e) => return Ok(Reply::Failed(format!("{party}'s model: {program} printed no answer with its count ({e})"))),
    };
    let ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    let costs = vec![(MEASURES[0].into(), i64::try_from(said.tokens).unwrap_or(i64::MAX)), (MEASURES[1].into(), ms)];
    Ok(Reply::Answer { value: Value::Text(said.answer), costs, events: Vec::new() })
}

/// Whether process `pid` has ended, left unreaped (compute's `ended`, local.rs, with no wait).
fn ended(pid: u32) -> bool {
    // SAFETY: waitid writes only into `info`, a zeroed siginfo_t of its own
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let r = unsafe { libc::waitid(libc::P_PID, pid, &mut info, libc::WEXITED | libc::WNOWAIT | libc::WNOHANG) };
    r == 0 && unsafe { info.si_pid() } != 0
}

/// All of `r` read on a thread of its own, kept up to one byte over the bound so an answer too long is told.
fn drain(mut r: impl Read + Send + 'static) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let (mut kept, mut b) = (Vec::new(), [0u8; 8192]);
        while let Ok(n @ 1..) = r.read(&mut b) {
            kept.extend_from_slice(&b[..n.min((ANSWER_BYTES + 1).saturating_sub(kept.len()))]);
        }
        kept
    })
}
