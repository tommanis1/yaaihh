//! The seated party's window as a driver: its seat, its words and its decisions, over a socket of its own.
//!
//! The window connects to the socket the config names; the daemon says whose each connection is, and only the
//! seated party's is kept, its first connection seating it. Words it says are appended to the words file and named
//! by their span there (`t:PATH:OFF:LEN`, which the turns driver's `TextOf` reads as it reads every passage), then
//! said, put where they go and timed, by the Events the config names. Its decisions answer the lines carried here.

mod said;

pub use said::{Answer, Said};

use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::value::{Instance, Value};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::{ErrorKind, Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

pub const NAME: &str = "window";

pub const SEATED: &str = "std.comms.Owner-Seated";
pub const SAYS: &str = "std.comms.Owner-Says";

pub const APPROVAL: &str = "std.control.Await-Approval";
pub const BUDGET: &str = "std.budgets.Request-Budget";

/// A project's Event, named by the file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Named {
    pub event: String,
}

/// `--window FILE`: the seated party, its socket, the plain file its words are kept in, and the project's Events.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub owner: String,
    pub socket: PathBuf,
    pub words: PathBuf,
    /// `(passage, place)`: words put somewhere.
    pub posts: Named,
    /// `(passage, seconds since 1970)`: when they were written.
    pub written: Named,
}

impl Config {
    pub fn read(text: &str) -> Result<Config, String> {
        serde_json::from_str(text).map_err(|e| e.to_string())
    }
}

/// The party the daemon vouches for at the other end of a connection, if any.
pub type Whose = Box<dyn Fn(&UnixStream) -> Option<String>>;

/// One connection of the window, and what came on it short of a whole line.
struct Conn {
    s: UnixStream,
    part: Vec<u8>,
}

impl Conn {
    /// The whole lines come since the last look, and whether the connection is still open.
    fn lines(&mut self) -> (Vec<String>, bool) {
        let mut buf = [0u8; 4096];
        let open = loop {
            match self.s.read(&mut buf) {
                Ok(0) => break false,
                Ok(n) => self.part.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => break e.kind() == ErrorKind::WouldBlock,
            }
        };
        let end = self.part.iter().rposition(|b| *b == b'\n').map_or(0, |k| k + 1);
        let whole: Vec<u8> = self.part.drain(..end).collect();
        (String::from_utf8_lossy(&whole).lines().map(str::to_string).collect(), open)
    }
}

/// The driver: its socket, the window's connections, and the lines it carries that wait for a decision.
pub struct Window {
    config: Config,
    listener: UnixListener,
    whose: Whose,
    conns: Vec<Conn>,
    seated: bool,
    carried: BTreeMap<u64, String>,
}

fn inst(ty: &str, args: Vec<Value>) -> Instance {
    Instance { ty: ty.to_string(), args }
}

/// What the window is told of a line it said: `{"ok": …}` with its words' passage, or `{"no": why}`.
fn reply(s: &UnixStream, r: &Result<Option<String>, String>) -> std::io::Result<()> {
    let j = match r {
        Ok(v) => serde_json::json!({ "ok": v }),
        Err(why) => serde_json::json!({ "no": why }),
    };
    (&*s).write_all(format!("{j}\n").as_bytes())
}

impl Window {
    /// The driver on `config`, listening on its socket (one an earlier daemon left is taken over).
    pub fn new(config: Config, whose: Whose) -> Result<Window, String> {
        let at = &config.socket;
        if std::fs::symlink_metadata(at).is_ok_and(|m| std::os::unix::fs::FileTypeExt::is_socket(&m.file_type())) {
            std::fs::remove_file(at).map_err(|e| format!("{}: {e}", at.display()))?;
        }
        let listener = UnixListener::bind(at).map_err(|e| format!("{}: {e}", at.display()))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        Ok(Window { config, listener, whose, conns: Vec::new(), seated: false, carried: BTreeMap::new() })
    }

    pub fn waits(&self) -> Vec<RawFd> {
        std::iter::once(self.listener.as_raw_fd()).chain(self.conns.iter().map(|c| c.s.as_raw_fd())).collect()
    }

    /// Never waits: the decisions by their lines, then the Events of what the window said.
    pub fn ready(&mut self) -> (Vec<(u64, Reply)>, Vec<Instance>) {
        let (mut answers, mut events) = (Vec::new(), Vec::new());
        while let Ok((s, _)) = self.listener.accept() {
            let whose = (self.whose)(&s);
            if whose.as_deref() != Some(self.config.owner.as_str()) || s.set_nonblocking(true).is_err() {
                let who = whose.unwrap_or_else(|| "a process no party is vouched for".into());
                drop(reply(&s, &Err(format!("{who} is not {}: this socket is its window's", self.config.owner))));
                continue;
            }
            if !std::mem::replace(&mut self.seated, true) {
                events.push(inst(SEATED, vec![Value::text(&self.config.owner)]));
            }
            self.conns.push(Conn { s, part: Vec::new() });
        }
        let mut conns = std::mem::take(&mut self.conns);
        conns.retain_mut(|c| {
            let (lines, open) = c.lines();
            let told = lines.iter().all(|l| reply(&c.s, &self.take(l, &mut answers, &mut events)).is_ok());
            open && told
        });
        self.conns = conns;
        (answers, events)
    }

    /// One line the window said: its words' passage, or a decision's answer kept for its line.
    fn take(&mut self, line: &str, answers: &mut Vec<(u64, Reply)>, events: &mut Vec<Instance>) -> Result<Option<String>, String> {
        match serde_json::from_str(line).map_err(|e| format!("not a line this socket takes: {e}"))? {
            Said::Words { text, on, to, at } => self.put(&text, &on, &to, at, events).map(Some),
            Said::Decision { line, answer } => {
                let op = self.carried.get(&line).ok_or(format!("line {line} is no question that waits here"))?;
                let value = answer.value(op)?;
                self.carried.remove(&line);
                answers.push((line, Reply::Answer { value, costs: Vec::new(), events: Vec::new() }));
                Ok(None)
            }
        }
    }

    /// Words kept, then said to each party, put on `on` and timed; their passage.
    fn put(&self, text: &str, on: &str, to: &[String], at: i64, events: &mut Vec<Instance>) -> Result<String, String> {
        if text.trim().is_empty() || on.is_empty() {
            return Err("words are said somewhere and are not empty".into());
        }
        let node = self.keep(text)?;
        events.extend(to.iter().map(|w| inst(SAYS, vec![Value::text(&node), Value::text(w)])));
        events.push(inst(&self.config.posts.event, vec![Value::text(&node), Value::text(on)]));
        events.push(inst(&self.config.written.event, vec![Value::text(&node), Value::Int(at)]));
        Ok(node)
    }

    /// `text` appended to the words file on a line of its own; its span there, as its passage's id.
    fn keep(&self, text: &str) -> Result<String, String> {
        let p = &self.config.words;
        let fail = |e: std::io::Error| format!("{}: {e}", p.display());
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(p).map_err(fail)?;
        let off = f.metadata().map_err(fail)?.len();
        f.write_all(format!("{text}\n").as_bytes()).map_err(fail)?;
        Ok(format!("t:{}:{off}:{}", p.display(), text.len()))
    }
}

impl Driver for Window {
    fn name(&self) -> Vec<Value> {
        vec![Value::text(NAME)]
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        [APPROVAL, BUDGET].map(|e| (e.to_string(), Vec::new())).to_vec()
    }
    fn events(&self) -> Vec<String> {
        let c = &self.config;
        vec![SEATED.to_string(), SAYS.to_string(), c.posts.event.clone(), c.written.event.clone()]
    }
    // the window asks from the record's line; the decision comes back on the socket
    fn carry(&mut self, op: &str, _: &[Value], line: u64) -> Reply {
        self.carried.insert(line, op.to_string());
        Reply::Later
    }
    fn dropped(&mut self, line: u64) {
        self.carried.remove(&line);
    }
}
