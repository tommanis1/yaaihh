//! A driver made of one harness's `Native` and the frame: the agents' sockets, the moments waiting on them, and
//! std.hooks' three Effects carried out (`Next-Moment`, `Apply`, `Tool-Use`).

use crate::action::Action;
use crate::wire;
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::value::Value;
use std::collections::{BTreeMap, VecDeque};
use std::io::{ErrorKind, Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

/// What the relay prints for the harness, and the exit code it ends with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Out {
    pub stdout: String,
    pub code: i32,
}

/// std.hooks' `FileChange`: one file a tool call changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileChange {
    pub kind: String,
    pub path: String,
    pub from: String,
    pub patch: String,
    pub added: String,
}

/// std.hooks' `ToolUse`: a tool call's parts, read once from the harness's own input (std.hooks, section 4).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolUse {
    pub tool: String,
    pub command: String,
    pub changes: Vec<FileChange>,
}

impl ToolUse {
    /// A call named `tool` that runs no command and changes no file.
    pub fn named(tool: &str) -> ToolUse {
        ToolUse { tool: tool.into(), ..Default::default() }
    }

    /// A command tool's call, running `line`.
    pub fn command(line: &str) -> ToolUse {
        ToolUse { tool: "command".into(), command: line.into(), ..Default::default() }
    }

    /// An edit of the files in `changes`.
    pub fn edit(changes: Vec<FileChange>) -> ToolUse {
        ToolUse { tool: "edit".into(), changes, ..Default::default() }
    }

    pub fn value(&self) -> Value {
        let t = |k: &str, v: &str| (k.to_string(), Value::text(v));
        let change = |c: &FileChange| Value::Record {
            ty: "std.hooks.FileChange".into(),
            fields: vec![t("kind", &c.kind), t("path", &c.path), t("from", &c.from), t("patch", &c.patch), t("added", &c.added)],
        };
        Value::Record {
            ty: "std.hooks.ToolUse".into(),
            fields: vec![t("tool", &self.tool), t("command", &self.command), ("changes".into(), Value::List(self.changes.iter().map(change).collect()))],
        }
    }
}

/// What one harness does in its own terms: the only part of a driver that names it.
pub trait Native {
    /// The harness and its version, as its bridge's `Lets` lines name them.
    fn name(&self) -> Vec<Value>;
    /// Its hooks by its own names, each with its moment: its bridge's `Native-Hook` facts.
    fn hooks(&self) -> Vec<(&'static str, &'static str)>;
    /// Its answer at `moment` for `action`; `Err` where it has none.
    fn answer(&self, moment: &str, action: &Action) -> Result<Out, String>;
    /// The parts of the tool call in `input`, a `tool_called` or `tool_result` input; all empty for any other.
    fn tool_use(&self, input: &str) -> ToolUse;
}

/// A moment one agent's harness reached and waits at, with the connection its answer goes back on.
struct Reached {
    conn: UnixStream,
    moment: String,
    input: String,
}

struct Agent {
    listener: UnixListener,
    path: PathBuf,
    /// Connected, its line not yet whole: read as it can be read, dropped when the relay closes it.
    pending: Vec<(UnixStream, Vec<u8>)>,
    /// Reached, not yet given to `Next-Moment`.
    arrived: VecDeque<Reached>,
    /// Given to `Next-Moment`, waiting for `Apply`.
    given: VecDeque<Reached>,
    /// Held by an `Apply` of `Suspend`, by its event: the harness waits until a later `Apply` at that moment.
    held: VecDeque<(String, Reached)>,
    /// The places of the `Next-Moment` lines answered `Later`, in order.
    wants: VecDeque<u64>,
}

/// A driver: harness `N`'s native terms in the shared frame.
pub struct Frame<N: Native> {
    pub native: N,
    agents: BTreeMap<String, Agent>,
    late: Vec<(u64, Reply)>,
    /// Told of each hook that reaches a moment, as it arrives: the agent, the moment and the hook's input.
    pub seen: Option<Box<dyn FnMut(&str, &str, &str)>>,
    /// Told of each answer a moment's hook was sent: the agent, the moment, and whether its turn goes on (a KeepGoing
    /// the harness took).
    pub answered: Option<Box<dyn FnMut(&str, &str, bool)>>,
}

/// The string a value of a type `Identified by String` is, as the engine gives it: the text itself.
fn named(v: &Value) -> Option<&str> {
    match v {
        Value::Text(s) => Some(s),
        _ => None,
    }
}

fn now(value: Value) -> Reply {
    Reply::Answer { value, costs: vec![], events: vec![] }
}

impl<N: Native> Frame<N> {
    pub fn new(native: N) -> Self {
        Frame { native, agents: BTreeMap::new(), late: Vec::new(), seen: None, answered: None }
    }

    /// Whether agent `agent` has a session here.
    pub fn holds(&self, agent: &str) -> bool {
        self.agents.contains_key(agent)
    }

    /// Agent `agent`'s socket, made in `dir`, readable by its own user alone, when its session starts: the relay of
    /// its harness's hooks is given its path, and whatever reaches it is that agent's.
    pub fn open(&mut self, agent: &str, dir: &Path) -> std::io::Result<PathBuf> {
        use std::os::unix::fs::PermissionsExt;
        self.close(agent);
        let path = dir.join(format!("{agent}.hooks"));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let a = Agent { listener, path: path.clone(), pending: Vec::new(), arrived: VecDeque::new(), given: VecDeque::new(), held: VecDeque::new(), wants: VecDeque::new() };
        self.agents.insert(agent.to_string(), a);
        Ok(path)
    }

    /// Agent `agent`'s session ended: each moment still waiting is let go as Allow lets it, and its socket removed.
    pub fn close(&mut self, agent: &str) {
        let Some(mut a) = self.agents.remove(agent) else { return };
        let left: Vec<Reached> = a.arrived.drain(..).chain(a.given.drain(..)).chain(a.held.drain(..).map(|(_, r)| r)).collect();
        for r in left {
            let _ = send(r.conn, &self.allow(&r.moment));
            self.tell(agent, &r.moment, false);
        }
        let _ = std::fs::remove_file(&a.path);
    }

    /// The descriptors the daemon waits on for this driver: the agents' sockets, and each connection whose line is
    /// not yet whole. When one can be read, it calls `ready`.
    pub fn waits(&self) -> Vec<RawFd> {
        self.agents.values().flat_map(|a| std::iter::once(a.listener.as_raw_fd()).chain(a.pending.iter().map(|(c, _)| c.as_raw_fd()))).collect()
    }

    /// The answers of the `Next-Moment`s that were `Later`, by their lines' places, from the hooks that have reached
    /// the sockets. It never waits: it takes what can be read now.
    pub fn ready(&mut self) -> Vec<(u64, Reply)> {
        self.take_in();
        std::mem::take(&mut self.late)
    }

    /// What the harness does at `moment` with no answer from the run.
    fn allow(&self, moment: &str) -> Out {
        self.native.answer(moment, &Action::Allow).unwrap_or(Out { stdout: String::new(), code: 0 })
    }

    /// Takes in each hook waiting on a socket, and gives it to the first `Next-Moment` waiting, if any.
    fn take_in(&mut self) {
        let hooks = self.native.hooks();
        for (name, a) in self.agents.iter_mut() {
            while let Ok((conn, _)) = a.listener.accept() {
                // an accepted connection is blocking whatever its listener is
                if conn.set_nonblocking(true).is_ok() {
                    a.pending.push((conn, Vec::new()));
                }
            }
            for (conn, mut buf) in std::mem::take(&mut a.pending) {
                match read_now(&conn, &mut buf) {
                    Whole::Not => a.pending.push((conn, buf)),
                    Whole::Gone => {}
                    Whole::Line => {
                        let _ = conn.set_nonblocking(false);
                        let end = buf.iter().position(|&b| b == b'\n').unwrap_or(buf.len());
                        let asked = String::from_utf8(buf[..end].to_vec()).map_err(|e| e.to_string()).and_then(|l| wire::read_ask(&l));
                        match asked {
                            Ok((hook, input)) => match hooks.iter().find(|(n, _)| *n == hook) {
                                Some((_, moment)) => {
                                    if let Some(seen) = self.seen.as_mut() {
                                        seen(name, moment, &input);
                                    }
                                    a.arrived.push_back(Reached { conn, moment: moment.to_string(), input });
                                }
                                // a hook its bridge does not name reaches no moment: the harness goes on as it would
                                None => {
                                    let _ = send(conn, &Out { stdout: String::new(), code: 0 });
                                }
                            },
                            Err(e) => {
                                eprintln!("docket-hooks: {e}");
                                let _ = send(conn, &Out { stdout: String::new(), code: 1 });
                            }
                        }
                    }
                }
            }
            while !a.wants.is_empty() && !a.arrived.is_empty() {
                let (line, r) = (a.wants.pop_front().unwrap(), a.arrived.pop_front().unwrap());
                self.late.push((line, now(reached(&r))));
                a.given.push_back(r);
            }
        }
    }

    fn apply(&mut self, agent: &str, moment: &str, v: &Value) -> Result<(), String> {
        // the action read first: a value that is none leaves the moment waiting for a right one
        let action = Action::of(v)?;
        let a = self.agents.get_mut(agent).ok_or_else(|| format!("{agent} has no session here"))?;
        let r = match a.given.iter().position(|r| r.moment == moment) {
            Some(k) => a.given.remove(k).unwrap(),
            None => match a.held.iter().position(|(_, r)| r.moment == moment) {
                Some(k) => a.held.remove(k).unwrap().1,
                None => return Err(format!("{agent} waits at no {moment}")),
            },
        };
        if let Action::Suspend(event) = &action {
            a.held.push_back((event.clone(), r));
            return Ok(());
        }
        let sent = match self.native.answer(moment, &action) {
            // an answer the harness did not get (its hook gave up, its session ended) was not applied
            Ok(out) => send(r.conn, &out).map_err(|e| format!("{agent}'s harness did not take the answer at {moment}: {e}")),
            Err(e) => {
                let _ = send(r.conn, &self.allow(moment));
                Err(e)
            }
        };
        self.tell(agent, moment, matches!(action, Action::KeepGoing(_)) && sent.is_ok());
        sent
    }

    fn tell(&mut self, agent: &str, moment: &str, goes_on: bool) {
        if let Some(answered) = self.answered.as_mut() {
            answered(agent, moment, goes_on);
        }
    }
}

impl<N: Native> Drop for Frame<N> {
    fn drop(&mut self) {
        let agents: Vec<String> = self.agents.keys().cloned().collect();
        agents.iter().for_each(|a| self.close(a));
    }
}

/// `Next-Moment`'s answer; its `Moment` is `Identified by String`, so the field holds the string itself, as the
/// machine checks an answer's type where it crosses (12).
fn reached(r: &Reached) -> Value {
    Value::Record { ty: "std.hooks.Reached".into(), fields: vec![("moment".into(), Value::text(&r.moment)), ("payload".into(), Value::text(&r.input))] }
}

fn send(mut conn: UnixStream, out: &Out) -> std::io::Result<()> {
    let line = wire::answer(&out.stdout, out.code) + "\n";
    conn.write_all(line.as_bytes()).and_then(|_| conn.flush())
}

/// What a connection's bytes so far are.
enum Whole {
    Line,
    Not,
    Gone,
}

/// Reads what connection `conn` holds now into `buf`, without waiting.
fn read_now(mut conn: &UnixStream, buf: &mut Vec<u8>) -> Whole {
    let mut chunk = [0u8; 8192];
    loop {
        match conn.read(&mut chunk) {
            Ok(0) => return if buf.contains(&b'\n') { Whole::Line } else { Whole::Gone },
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.contains(&b'\n') {
                    return Whole::Line;
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => return Whole::Not,
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => return Whole::Gone,
        }
    }
}

impl<N: Native> Driver for Frame<N> {
    fn name(&self) -> Vec<Value> {
        self.native.name()
    }

    fn effects(&self) -> Vec<(String, Vec<String>)> {
        ["std.hooks.Next-Moment", "std.hooks.Apply", "std.hooks.Tool-Use"].iter().map(|e| (e.to_string(), vec![])).collect()
    }

    fn events(&self) -> Vec<String> {
        vec![]
    }

    fn dropped(&mut self, line: u64) {
        // the next hook goes to a run still waiting, or waits in `arrived`
        self.agents.values_mut().for_each(|a| a.wants.retain(|&l| l != line));
    }

    fn carry(&mut self, op: &str, args: &[Value], line: u64) -> Reply {
        let agent = args.first().and_then(named).unwrap_or_default().to_string();
        match op {
            "std.hooks.Next-Moment" => {
                self.take_in();
                let Some(a) = self.agents.get_mut(&agent) else { return Reply::Failed(format!("{agent} has no session here")) };
                match a.arrived.pop_front() {
                    Some(r) => {
                        let value = reached(&r);
                        a.given.push_back(r);
                        now(value)
                    }
                    None => {
                        a.wants.push_back(line);
                        Reply::Later
                    }
                }
            }
            "std.hooks.Apply" => match (args.get(1).and_then(named), args.get(2)) {
                (Some(moment), Some(v)) => match self.apply(&agent, &moment.to_string(), v) {
                    Ok(()) => now(Value::Unit),
                    Err(e) => Reply::Failed(e),
                },
                _ => Reply::Failed("Apply takes an agent, a moment and a HookAction".into()),
            },
            "std.hooks.Tool-Use" => match args.get(1) {
                Some(Value::Text(input)) => now(self.native.tool_use(input).value()),
                _ => Reply::Failed("Tool-Use takes an agent and the input as a text".into()),
            },
            _ => Reply::Failed(format!("{op} is not an Effect this driver carries out")),
        }
    }
}
