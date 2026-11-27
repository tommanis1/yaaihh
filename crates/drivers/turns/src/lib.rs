//! The turns driver: agents' passages and usage read from their harnesses' own transcripts, words carried to them.

use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::value::{Instance, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use transcript::Shape;

pub mod transcript;

pub const TEXT_OF: &str = "std.comms.TextOf";
pub const DELIVER: &str = "std.control.Deliver";
pub const NOTIFY: &str = "std.control.Notify";
pub const SPIN_OFF: &str = "std.control.Spin-Off";
pub const JOIN_BACK: &str = "std.control.Join-Back";
pub const DELETE_FORK: &str = "std.control.Delete-Fork";
pub const SPOKE: &str = "std.comms.Spoke";
pub const SHOWN: &str = "std.comms.Shown";
pub const USAGE: &str = "std.budgets.Usage";
/// The bridges' deliveries (std.delivery, std/bridges): words for a Claude Code session, typed when it is idle and
/// held through a turn as a `Notify` is, or typed at once into the turn; a Codex turn's packet, held as a `Notify`,
/// and a word steered into Codex's running turn, typed at once. Each is answered once typed.
pub const PUSH: &str = "std.bridges.claude_code.Push-Channel";
pub const PUSH_NOW: &str = "std.bridges.claude_code.Push-Channel-Now";
pub const START_TURN: &str = "std.bridges.codex.Codex-Start-Turn";
pub const STEER: &str = "std.bridges.codex.Codex-Steer";
/// The owner's own word to an agy session, typed as a turn of its own when no turn runs (N-1998 p3, point 1).
pub const AGY_KICK: &str = "std.bridges.agy.Agy-Kick";

/// What the daemon does for this driver: words typed into a session's terminal, a copy of a session started from its
/// transcript's path (the copy's party back), a session's end.
pub struct Daemon {
    pub type_in: Box<dyn FnMut(&Value, &str) -> Result<(), String>>,
    pub fork: Box<dyn FnMut(&Value, &str) -> Result<Value, String>>,
    pub end: Box<dyn FnMut(&Value) -> Result<(), String>>,
}

/// The driver's file (`--turns`): the plain-text files whose spans are passages too, as the window's driver writes
/// them, and where the agents' terminals are.
#[derive(Debug, Default, serde::Deserialize)]
pub struct Config {
    #[serde(default)]
    pub plain: Vec<String>,
    /// The folder whose `pty/` holds the agents' terminals' sockets (spawn's coord-pty COORD).
    #[serde(default)]
    pub terminals: String,
    /// The parties' homes (coordd-v2's `--homes`), where an agy party's session is `NAME/agy/coord` (spawn-v2's).
    #[serde(default)]
    pub homes: String,
    /// The agy-rpc that serves those sessions: an agy party's words go through its `send`, its terminal taking none.
    #[serde(default)]
    pub agy_rpc: String,
}

/// An agent's transcript as read: where reading stopped, its message read but not yet counted, its turn's last
/// passage.
struct Reading {
    shape: Shape,
    path: String,
    at: u64,
    counting: Option<(String, String, Vec<(&'static str, i64)>, i64)>,
    last: Option<String>,
}

/// One agent given words: its transcript once a hook has named it (none before its first), the `Deliver` lines typed
/// in and waiting for a turn's end, whether it is in a turn, and the words given while it was (with whether a turn's
/// end answers them), each to be typed at a turn's end, oldest first.
struct Follow {
    party: Value,
    read: Option<Reading>,
    waiting: VecDeque<u64>,
    busy: bool,
    held: VecDeque<(u64, String, bool)>,
}

pub struct Turns {
    daemon: Daemon,
    follows: HashMap<String, Follow>,
    /// The plain files and every transcript followed so far, with their shapes: TextOf reads no other file.
    paths: HashMap<String, Shape>,
    answers: Vec<(u64, Reply)>,
    events: Vec<Instance>,
    wake: UnixStream,
    poke: UnixStream,
}

/// The name a party value is written with: the machine gives an `Identified by String` value as its text, a
/// driver may give it as an instance of its one field.
fn named(v: &Value) -> String {
    match v {
        Value::Text(s) => s.clone(),
        Value::Inst(Instance { args, .. }) if matches!(args.as_slice(), [Value::Text(_)]) => named(&args[0]),
        _ => v.to_string(),
    }
}

fn event(ty: &str, args: Vec<Value>) -> Instance {
    Instance { ty: ty.into(), args }
}

/// The key of a thing written out: its content's hash.
pub fn key(text: &str) -> Value {
    Value::Text(format!("sha256:{:x}", Sha256::digest(text.as_bytes())))
}

impl Turns {
    /// Whether this driver reads transcripts of `harness`.
    pub fn reads(harness: &str) -> bool {
        Shape::of(harness).is_some()
    }

    pub fn new(daemon: Daemon, config: Config) -> Result<Self, String> {
        let (wake, poke) = UnixStream::pair().map_err(|e| e.to_string())?;
        wake.set_nonblocking(true).map_err(|e| e.to_string())?;
        let paths = config.plain.into_iter().map(|p| (p, Shape::Plain)).collect();
        Ok(Turns { daemon, follows: HashMap::new(), paths, answers: Vec::new(), events: Vec::new(), wake, poke })
    }

    /// Follow `party`'s transcript at `path`, written by `harness`, from its end now; again the same path: as it was.
    pub fn follow(&mut self, party: &Value, harness: &str, path: &str) -> Result<(), String> {
        let shape = Shape::of(harness).ok_or(format!("no transcript reader for harness {harness}"))?;
        let name = named(party);
        if self.follows.get(&name).and_then(|f| f.read.as_ref()).is_some_and(|r| r.path == path) {
            return Ok(());
        }
        let at = std::fs::metadata(path).map_err(|e| format!("{path}: {e}"))?.len();
        self.paths.insert(path.to_string(), shape);
        let (waiting, busy, held) = self.follows.remove(&name).map(|f| (f.waiting, f.busy, f.held)).unwrap_or_default();
        let read = Some(Reading { shape, path: path.into(), at, counting: None, last: None });
        self.follows.insert(name.clone(), Follow { party: Value::Text(name), read, waiting, busy, held });
        Ok(())
    }

    /// `party` has begun a turn: words given to it from now on are held until a turn's end.
    pub fn turn_began(&mut self, party: &Value) -> Result<(), String> {
        self.follows.get_mut(&named(party)).ok_or(format!("{party} is not followed"))?.busy = true;
        Ok(())
    }

    /// `party`'s turn has ended: its new passages and usage performed, its oldest waiting `Deliver` answered with the
    /// turn's last passage, and the oldest words held typed in.
    pub fn turn_ended(&mut self, party: &Value) -> Result<(), String> {
        let f = self.follows.get_mut(&named(party)).ok_or(format!("{party} is not followed"))?;
        let rd = f.read.as_mut().ok_or(format!("{party}'s transcript is not followed"))?;
        let (lines, at) = transcript::lines(&rd.path, rd.at).map_err(|e| format!("{}: {e}", rd.path))?;
        rd.at = at;
        for (off, line) in lines {
            let r = transcript::read(rd.shape, &line);
            let id = transcript::id(&rd.path, off, line.len() as u64);
            if r.text.is_some() {
                self.events.push(event(SPOKE, vec![f.party.clone(), Value::text(&id)]));
                rd.last = Some(id.clone());
            }
            if r.usage.is_empty() {
                continue;
            }
            match r.message {
                Some(m) => {
                    if rd.counting.as_ref().is_some_and(|c| c.0 != m) {
                        count(&mut self.events, &f.party, rd.counting.take());
                    }
                    rd.counting = Some((m, id, r.usage, r.minute));
                }
                None => count(&mut self.events, &f.party, Some((String::new(), id, r.usage, r.minute))),
            }
        }
        count(&mut self.events, &f.party, rd.counting.take());
        if let Some(line) = f.waiting.pop_front() {
            let said = match rd.last.take() {
                Some(id) => transcript::text_of(rd.shape, &id).map(|t| Reply::Answer { value: Value::Text(t), costs: vec![], events: vec![] }).unwrap_or_else(Reply::Failed),
                None => Reply::Failed(format!("{} ended its turn with no passage read", f.party)),
            };
            self.answers.push((line, said));
        }
        rd.last = None;
        f.busy = false;
        if let Some((line, msg, replied)) = f.held.pop_front() {
            let party = f.party.clone();
            self.type_in(&party, &msg, line, replied);
        }
        self.wake_up()
    }

    /// `msg` typed into `party`'s terminal for line `line`: the line then waits for the reply a turn's end gives, or
    /// if not `replied` is answered now; it fails if the words cannot be typed.
    fn type_in(&mut self, party: &Value, msg: &str, line: u64, replied: bool) {
        match (self.daemon.type_in)(party, msg) {
            Ok(()) => {
                self.events.push(event(SHOWN, vec![party.clone(), key(msg)]));
                if !replied {
                    self.answers.push((line, Reply::Answer { value: Value::Unit, costs: vec![], events: vec![] }));
                }
                // the words begin a turn: what comes before its hook is held as in one
                self.follows.get_mut(&named(party)).into_iter().for_each(|f| {
                    if replied {
                        f.waiting.push_back(line);
                    }
                    f.busy = true;
                });
            }
            Err(why) => self.answers.push((line, Reply::Failed(why))),
        }
    }

    /// Words for `to` typed at once, in a turn too, and answered once typed: an interrupting word. Typed into an idle
    /// session they begin its turn, as `give`'s do.
    fn give_now(&mut self, to: &Value, msg: &str, line: u64) -> Reply {
        let party = self.follows.get(&named(to)).map_or_else(|| to.clone(), |f| f.party.clone());
        self.type_in(&party, msg, line, false);
        match self.wake_up() {
            Ok(()) => Reply::Later,
            Err(why) => Reply::Failed(why),
        }
    }

    fn wake_up(&mut self) -> Result<(), String> {
        std::io::Write::write_all(&mut self.poke, &[1]).map_err(|e| e.to_string())
    }

    pub fn waits(&self) -> Vec<RawFd> {
        vec![self.wake.as_raw_fd()]
    }

    /// The late answers ready, by their lines, then the Events performed; never waits.
    pub fn ready(&mut self) -> (Vec<(u64, Reply)>, Vec<Instance>) {
        let mut b = [0u8; 64];
        while matches!((&self.wake).read(&mut b), Ok(1..)) {}
        (std::mem::take(&mut self.answers), std::mem::take(&mut self.events))
    }

    fn text_of(&self, id: &str) -> Result<String, String> {
        let (path, _, _) = transcript::span(id).ok_or(format!("{id} names no passage of a transcript or plain file"))?;
        transcript::text_of(*self.paths.get(path).ok_or(format!("{path} is no followed transcript or plain file"))?, id)
    }

    /// Words for `to`: typed in now, or held while it is in a turn; a `Deliver` (`replied`) is answered with the
    /// reply its words' turn gives, a `Notify` once they are typed. A session no hook has named yet takes them in its
    /// terminal, and the first hook names its transcript; with no terminal they fail.
    fn give(&mut self, to: &Value, msg: &str, line: u64, replied: bool) -> Reply {
        let name = named(to);
        let f = self.follows.entry(name.clone()).or_insert_with(|| Follow { party: Value::Text(name), read: None, waiting: VecDeque::new(), busy: false, held: VecDeque::new() });
        if f.busy {
            f.held.push_back((line, msg.to_string(), replied));
            return Reply::Later;
        }
        let party = f.party.clone();
        self.type_in(&party, msg, line, replied);
        // words that could not be typed to a session never named leave nothing of it behind
        if self.follows.get(&named(to)).is_some_and(|f| f.read.is_none() && !f.busy) {
            self.follows.remove(&named(to));
        }
        match self.wake_up() {
            Ok(()) => Reply::Later,
            Err(why) => Reply::Failed(why),
        }
    }
}

/// The usage of one message, a line for each kind its harness reported.
fn count(events: &mut Vec<Instance>, party: &Value, c: Option<(String, String, Vec<(&'static str, i64)>, i64)>) {
    let Some((_, node, usage, minute)) = c else { return };
    for (kind, n) in usage {
        events.push(event(USAGE, vec![party.clone(), Value::text(&node), Value::text(kind), Value::Int(n), Value::Int(minute)]));
    }
}

fn unit(r: Result<(), String>) -> Reply {
    match r {
        Ok(()) => Reply::Answer { value: Value::Unit, costs: vec![], events: vec![] },
        Err(why) => Reply::Failed(why),
    }
}

impl Driver for Turns {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("turns")]
    }

    fn effects(&self) -> Vec<(String, Vec<String>)> {
        [TEXT_OF, DELIVER, NOTIFY, SPIN_OFF, JOIN_BACK, DELETE_FORK, PUSH, PUSH_NOW, START_TURN, STEER, AGY_KICK].iter().map(|e| (e.to_string(), vec![])).collect()
    }

    fn events(&self) -> Vec<String> {
        vec![SPOKE.into(), SHOWN.into(), USAGE.into()]
    }

    fn carry(&mut self, op: &str, args: &[Value], line: u64) -> Reply {
        match (op, args) {
            (TEXT_OF, [Value::Text(id)]) => match self.text_of(id) {
                Ok(t) => Reply::Answer { value: Value::Text(t), costs: vec![], events: vec![] },
                Err(why) => Reply::Failed(why),
            },
            (DELIVER, [_, to, Value::Text(msg)]) => self.give(to, msg, line, true),
            (NOTIFY | PUSH | START_TURN | AGY_KICK, [to, Value::Text(msg)]) => self.give(to, msg, line, false),
            (PUSH_NOW | STEER, [to, Value::Text(msg)]) => self.give_now(to, msg, line),
            (SPIN_OFF, [parent]) => match self.follows.get(&named(parent)).and_then(|f| f.read.as_ref()).map(|r| r.path.clone()).ok_or(format!("{parent} has no session followed here")).and_then(|path| (self.daemon.fork)(parent, &path)) {
                Ok(fork) => Reply::Answer { value: fork, costs: vec![], events: vec![] },
                Err(why) => Reply::Failed(why),
            },
            (JOIN_BACK, [_, parent, Value::Text(summary)]) => unit((self.daemon.type_in)(parent, summary)),
            (DELETE_FORK, [fork]) => {
                if let Some(f) = self.follows.remove(&named(fork)) {
                    let lines = f.waiting.into_iter().chain(f.held.into_iter().map(|(l, _, _)| l));
                    self.answers.extend(lines.map(|l| (l, Reply::Failed(format!("{fork} was deleted before its turn ended")))));
                }
                unit((self.daemon.end)(fork).and_then(|()| self.wake_up()))
            }
            _ => Reply::Failed(format!("{op} is not carried here as given")),
        }
    }

    fn dropped(&mut self, line: u64) {
        self.follows.values_mut().for_each(|f| {
            f.waiting.retain(|l| *l != line);
            f.held.retain(|(l, _, _)| *l != line);
        });
    }
}

#[cfg(test)]
mod tests;
