//! Docket's Command and Measure effects as compute jobs: the line under `sh -c` in the configured setting,
//! answered later.

use compute::{ComputeDriver, Config, JobSpec, Stream};
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::value::Value;
use std::collections::HashMap;
use std::io::Read;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

pub const COMMAND: &str = "std.budgets.Command";
pub const MEASURE: &str = "std.budgets.Measure";

/// The most of a job's stdout an answer carries: the record keeps what was decided, not outputs.
pub const ANSWER_BYTES: usize = 64 * 1024;

/// A job: its line, its text, whether its answer is a number (Measure), and its stop.
type Job = (u64, String, bool, Arc<AtomicBool>);

/// Jobs taken by as many workers as the file allows, each with its own driver; answers queued, the daemon woken by a byte.
pub struct Compute {
    jobs: mpsc::Sender<Job>,
    rx: mpsc::Receiver<(u64, Reply)>,
    wake: UnixStream,
    running: HashMap<u64, Arc<AtomicBool>>,
}

impl Compute {
    pub fn new(config: Config) -> Result<Self, String> {
        let (wake, poke) = UnixStream::pair().map_err(|e| e.to_string())?;
        wake.set_nonblocking(true).map_err(|e| e.to_string())?;
        let (tx, rx) = mpsc::channel();
        let (jobs, queue) = mpsc::channel::<Job>();
        let (config, queue) = (Arc::new(config), Arc::new(Mutex::new(queue)));
        for _ in 0..config.at_once {
            let (mut driver, config, queue, tx) = (config.driver()?, config.clone(), queue.clone(), tx.clone());
            let mut poke = poke.try_clone().map_err(|e| e.to_string())?;
            std::thread::spawn(move || {
                while let Ok((line, text, number, stop)) = { queue.lock().map_err(|_| ()).and_then(|q| q.recv().map_err(|_| ())) } {
                    if stop.load(Ordering::Relaxed) {
                        continue;
                    }
                    let reply = run(&config, driver.as_mut(), &text, number, &stop);
                    if tx.send((line, reply)).is_err() || std::io::Write::write_all(&mut poke, &[1]).is_err() {
                        break;
                    }
                }
            });
        }
        Ok(Compute { jobs, rx, wake, running: HashMap::new() })
    }

    /// What the daemon waits on.
    pub fn waits(&self) -> Vec<RawFd> {
        vec![self.wake.as_raw_fd()]
    }

    /// The answers ready, by their effect lines' places, for lines still wanted; never waits.
    pub fn ready(&mut self) -> Vec<(u64, Reply)> {
        let mut b = [0u8; 64];
        while matches!((&self.wake).read(&mut b), Ok(1..)) {}
        let got: Vec<(u64, Reply)> = self.rx.try_iter().collect();
        got.into_iter().filter(|(line, _)| self.running.remove(line).is_some()).collect()
    }
}

/// The line run as a job: its stdout the answer when it exits 0 and is at most `ANSWER_BYTES`, else a failure;
/// for a `number`, the one integer the stdout prints.
fn run(config: &Config, d: &mut dyn ComputeDriver, line: &str, number: bool, stop: &AtomicBool) -> Reply {
    let mut go = || -> Result<Reply, String> {
        let job = JobSpec { cmd: vec!["/bin/sh".into(), "-c".into(), line.into()], ..config.base.clone() };
        let r = d.run(&job, &mut |_: Stream, _: &[u8]| {}, stop)?;
        if r.exit_code == 0 && r.stdout_digest.size as usize > ANSWER_BYTES {
            return Ok(Reply::Failed(format!("stdout of {} bytes, over the {ANSWER_BYTES} an answer carries; it is {}-{} in the store", r.stdout_digest.size, r.stdout_digest.hash, r.stdout_digest.size)));
        }
        let digest = if r.exit_code == 0 { &r.stdout_digest } else { &r.stderr_digest };
        let said = String::from_utf8_lossy(&d.download(digest)?).into_owned();
        if r.exit_code == 0 {
            let value = match said.trim().parse::<i64>() {
                _ if !number => Value::text(&said),
                Ok(n) => Value::Int(n),
                Err(_) => return Ok(Reply::Failed(format!("printed {:?}, not one number", said.trim()))),
            };
            return Ok(Reply::Answer { value, costs: Vec::new(), events: Vec::new() });
        }
        let tail: String = said.chars().rev().take(2000).collect::<Vec<_>>().into_iter().rev().collect();
        Ok(Reply::Failed(format!("exit {}: {}", r.exit_code, tail.trim_end())))
    };
    go().unwrap_or_else(Reply::Failed)
}

impl Driver for Compute {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("compute")]
    }

    fn effects(&self) -> Vec<(String, Vec<String>)> {
        vec![(COMMAND.into(), Vec::new()), (MEASURE.into(), Vec::new())]
    }

    fn events(&self) -> Vec<String> {
        Vec::new()
    }

    fn carry(&mut self, op: &str, args: &[Value], line: u64) -> Reply {
        let (COMMAND | MEASURE, [Value::Text(text)]) = (op, args) else { return Reply::Failed(format!("{op}: not a command this driver carries")) };
        let stop = Arc::new(AtomicBool::new(false));
        if self.jobs.send((line, text.clone(), op == MEASURE, stop.clone())).is_err() {
            return Reply::Failed("no worker takes jobs".into());
        }
        self.running.insert(line, stop);
        Reply::Later
    }

    /// The job is stopped: on this host its processes end; on an endpoint its stream is dropped, and the server decides.
    fn dropped(&mut self, line: u64) {
        if let Some(stop) = self.running.remove(&line) {
            stop.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests;
