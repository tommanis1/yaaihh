//! Docket v2's host driver: the Events this machine knows of on its own (the clock, new commits, kept folders'
//! sizes, captured proofs), and the Effects only it can answer (Pick, Pick-Other, Git-Head, Sleep).

mod clock;
mod config;
mod git;
mod store;
mod watch;

pub use config::{Config, Named};

use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::value::{Instance, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const GIT_HEAD: &str = "std.control.Git-Head";
pub const SLEEP: &str = "std.control.Sleep";
pub const PICK: &str = "std.comms.Pick";
pub const PICK_OTHER: &str = "std.rails.Pick-Other";

/// A late answer by its effect line's place, or an Event performed on its own.
enum Out {
    Answer(u64, Reply),
    Event(Instance),
}

/// What the daemon lets Pick see: the parties with a live session, in its order, and whether a condition holds now,
/// read in a module's names.
pub struct Seen {
    pub live: Box<dyn Fn() -> Vec<String>>,
    pub holds: Box<dyn Fn(&str, &str) -> bool>,
}

/// The driver; its watch runs on a thread of its own and wakes the daemon by a byte.
pub struct Host {
    config: Arc<Config>,
    seen: Seen,
    tx: mpsc::Sender<Out>,
    rx: mpsc::Receiver<Out>,
    wake: UnixStream,
    poke: UnixStream,
    sleeping: HashMap<u64, Arc<AtomicBool>>,
    /// Picks by line: the role and whom the one picked is of another family than, answered at the next `ready`.
    picks: Vec<(u64, String, Vec<String>)>,
}

impl Host {
    /// The driver, its watch looking at once and then at each whole `period` (a minute when serving).
    pub fn new(config: Config, period: Duration, seen: Seen) -> Result<Self, String> {
        let (wake, poke) = UnixStream::pair().map_err(|e| e.to_string())?;
        wake.set_nonblocking(true).map_err(|e| e.to_string())?;
        let (tx, rx) = mpsc::channel();
        let config = Arc::new(config);
        let mut watch = watch::Watch::load(config.state.as_deref())?;
        let (c, t, mut p) = (config.clone(), tx.clone(), poke.try_clone().map_err(|e| e.to_string())?);
        std::thread::spawn(move || {
            loop {
                for ev in watch.look(&c, SystemTime::now()) {
                    if t.send(Out::Event(ev)).is_err() {
                        return;
                    }
                }
                if let Some(Err(why)) = c.state.as_deref().map(|s| watch.save(s)) {
                    eprintln!("docket-driver-host: {why}; a restart may say again what this look said");
                }
                if p.write_all(&[1]).is_err() {
                    return;
                }
                let into = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() % period.as_nanos().max(1);
                std::thread::sleep(period.saturating_sub(Duration::from_nanos(into as u64)));
            }
        });
        Ok(Host { config, seen, tx, rx, wake, poke, sleeping: HashMap::new(), picks: Vec::new() })
    }

    /// What the daemon waits on.
    pub fn waits(&self) -> Vec<RawFd> {
        vec![self.wake.as_raw_fd()]
    }

    /// The late answers (sleeps over, picks made) for lines still wanted, and the Events performed; never waits.
    pub fn ready(&mut self) -> (Vec<(u64, Reply)>, Vec<Instance>) {
        let mut b = [0u8; 64];
        while matches!((&self.wake).read(&mut b), Ok(1..)) {}
        let (mut answers, mut events) = (Vec::new(), Vec::new());
        for o in self.rx.try_iter().collect::<Vec<_>>() {
            match o {
                Out::Answer(line, r) if self.sleeping.remove(&line).is_some() => answers.push((line, r)),
                Out::Answer(..) => {}
                Out::Event(ev) => events.push(ev),
            }
        }
        for (line, role, than) in std::mem::take(&mut self.picks) {
            answers.push((line, self.pick(&role, &than)));
        }
        (answers, events)
    }

    /// The first party with a live session that holds `role` now and is of another family than each of `than`.
    fn pick(&self, role: &str, than: &[String]) -> Reply {
        let holds = &self.seen.holds;
        let other = |p: &String| than.iter().all(|w| holds("std.rails", &format!("Other-Family({p:?}, {w:?})")));
        match (self.seen.live)().into_iter().find(|p| holds("std.comms", &format!("Role({p:?}, {role:?})")) && other(p)) {
            Some(p) => Reply::Answer { value: Value::text(&p), costs: Vec::new(), events: Vec::new() },
            None if than.is_empty() => Reply::Failed(format!("no party with a live session holds the role {role:?}")),
            None => Reply::Failed(format!("no party with a live session holds the role {role:?} of another family than {}", than.join(", "))),
        }
    }

    /// A pick for `line`, answered at the next `ready`.
    fn ask(&mut self, line: u64, role: &str, than: Vec<String>) -> Reply {
        self.picks.push((line, role.to_string(), than));
        drop((&self.poke).write_all(&[1]));
        Reply::Later
    }

    /// A sleep of `secs`, answered by `line` when it is over unless it was dropped first.
    fn sleep(&mut self, secs: i64, line: u64) -> Result<(), String> {
        let stop = Arc::new(AtomicBool::new(false));
        let (s, tx, mut poke) = (stop.clone(), self.tx.clone(), self.poke.try_clone().map_err(|e| e.to_string())?);
        let until = Instant::now() + Duration::from_secs(secs as u64);
        std::thread::spawn(move || {
            while Instant::now() < until && !s.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(50).min(until.saturating_duration_since(Instant::now())));
            }
            if !s.load(Ordering::Relaxed) && tx.send(Out::Answer(line, Reply::Answer { value: Value::Unit, costs: Vec::new(), events: Vec::new() })).is_ok() {
                drop(poke.write_all(&[1]));
            }
        });
        self.sleeping.insert(line, stop);
        Ok(())
    }
}

impl Driver for Host {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("host")]
    }

    fn effects(&self) -> Vec<(String, Vec<String>)> {
        [PICK, PICK_OTHER, GIT_HEAD, SLEEP].map(|e| (e.to_string(), Vec::new())).to_vec()
    }

    fn events(&self) -> Vec<String> {
        let c = &self.config;
        let named = [&c.committed, &c.times, &c.proofs].into_iter().flatten().map(|n| n.event.clone());
        let changed = c.changed_by.as_ref().map(|_| git::ARTIFACT_CHANGED.to_string());
        [clock::TICK, store::SIZED].map(String::from).into_iter().chain(changed).chain(named).collect()
    }

    fn carry(&mut self, op: &str, args: &[Value], line: u64) -> Reply {
        let now = |value| Reply::Answer { value, costs: Vec::new(), events: Vec::new() };
        match (op, args) {
            (GIT_HEAD, []) => match self.config.head.as_deref().ok_or("no repository is named for Git-Head".to_string()).and_then(git::head) {
                Ok(h) => now(Value::text(&h)),
                Err(why) => Reply::Failed(why),
            },
            (SLEEP, [Value::Int(s)]) if *s <= 0 => now(Value::Unit),
            (SLEEP, [Value::Int(s)]) => self.sleep(*s, line).map_or_else(Reply::Failed, |()| Reply::Later),
            (PICK, [Value::Text(role)]) => self.ask(line, role, Vec::new()),
            (PICK_OTHER, [Value::Text(role), Value::List(than)]) => match than.iter().map(named).collect::<Option<Vec<_>>>() {
                Some(than) => self.ask(line, role, than),
                None => Reply::Failed(format!("{op}: a party given as no name")),
            },
            _ => Reply::Failed(format!("{op} is not carried here as given")),
        }
    }

    /// A sleep stops, and it or a pick gets no answer.
    fn dropped(&mut self, line: u64) {
        if let Some(stop) = self.sleeping.remove(&line) {
            stop.store(true, Ordering::Relaxed);
        }
        self.picks.retain(|(l, ..)| *l != line);
    }
}

/// A party's name: its text, as the engine gives a type identified by a String (spec 5.4); nothing else is one.
fn named(v: &Value) -> Option<String> {
    match v {
        Value::Text(t) => Some(t.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
