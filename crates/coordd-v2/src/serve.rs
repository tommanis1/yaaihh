//! Serving the socket: each connection's party fixed once, its requests carried to the machine, every new line to every
//! watcher, and to each subscriber what moved.

// Every task of the daemon runs in one local task set: the machine and the file are borrowed together, so the lines
// in the file are the machine's lines in its order. A line is in the file before any watcher is sent it. If
// the file takes no more lines, the machine is ahead of the record, so nothing more is changed or answered until
// the engine starts again and replays the file.

#[path = "subscribe.rs"]
mod subscribe;

use crate::file::File;
use crate::machine::Machine;
use crate::peer::{vouched, Party, Vouch};
use coordd_v2_wire::{Reply, Request};
use base64::Engine as _;
use docket_v2::run::driver::Reply as Late;
use docket_v2::run::value::Instance;
use serde_json::Value as Json;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::spawn_local;

/// How many lines a watcher is sent from the file per borrow of it.
const CHUNK: u64 = 1000;

/// The terminals of the sessions the drivers hold.
pub trait Terminals {
    /// Terminal `session` as `party` may use it: where what is typed goes, and its output from now on.
    fn open(&self, party: &Party, session: &str) -> Result<(mpsc::UnboundedSender<Vec<u8>>, broadcast::Receiver<Vec<u8>>), String>;
}

struct Core {
    machine: Box<dyn Machine>,
    file: File,
    /// Why the file took no more lines, once it has failed to.
    stopped: Option<String>,
    /// Every Fact and Duty type's view, kept while anyone takes the deltas.
    view: Option<crate::machine::View>,
    /// Why changes are refused while reads go on, once they are.
    refusing: Option<String>,
    /// Asked before each change: an error refuses it and every one after.
    guard: Option<Box<dyn Fn() -> Result<(), String>>>,
}

/// The frame around one machine and its record.
pub struct Engine {
    core: RefCell<Core>,
    lines: broadcast::Sender<(u64, Json)>,
    /// How many changes the machine has taken: a driver's loop gathers its descriptors again after each.
    changes: watch::Sender<u64>,
    /// The one delta of each change, for the subscriptions and any projection ([`Engine::step_deltas`]).
    deltas: broadcast::Sender<coordd_v2_wire::StepDelta>,
    vouch: Box<dyn Vouch>,
    terminals: Box<dyn Terminals>,
}

/// One connection's terminals: where what it types into each session goes.
type Typed = HashMap<String, mpsc::UnboundedSender<Vec<u8>>>;

impl Engine {
    /// Open the record at `path`, have `machine` restore it, and write what the machine wrote beyond it (a fresh
    /// record's `loaded` line), before anything is served.
    pub fn open(path: &Path, mut machine: Box<dyn Machine>, vouch: Box<dyn Vouch>, terminals: Box<dyn Terminals>) -> Result<Rc<Engine>, String> {
        let (mut file, lines) = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if !lines.is_empty() {
            machine.restore(&lines).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        for line in machine.written() {
            file.append(&line).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        let (tx, _) = broadcast::channel(4096);
        let core = Core { machine, file, stopped: None, refusing: None, guard: None, view: None };
        Ok(Rc::new(Engine { core: RefCell::new(core), lines: tx, changes: watch::Sender::new(0), deltas: broadcast::channel(256).0, vouch, terminals }))
    }

    /// Serve every connection to `listener` until it fails. Runs inside a `LocalSet`.
    pub async fn serve(self: Rc<Self>, listener: UnixListener) -> std::io::Result<()> {
        loop {
            let (s, _) = listener.accept().await?;
            spawn_local(self.clone().connection(s));
        }
    }

    /// A receiver that sees each change the machine takes.
    pub fn changes(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }

    /// The party of the process at the other end of `s`, as the table vouches.
    fn party_of(&self, s: &UnixStream) -> Result<Party, String> {
        let pid = s.peer_cred().map_err(|e| e.to_string())?.pid().ok_or("the kernel gave no process for this connection")?;
        vouched(&*self.vouch, pid)
    }

    async fn connection(self: Rc<Self>, s: UnixStream) {
        let party = self.party_of(&s);
        let (rd, mut wr) = s.into_split();
        let (out, mut replies) = mpsc::unbounded_channel::<Reply>();
        let slot = Rc::new(subscribe::Slot::default());
        let frames = slot.clone();
        let writer = spawn_local(async move {
            loop {
                // a reply first: a subscription's `done` goes before its first frame
                let r = tokio::select! {
                    biased;
                    r = replies.recv() => match r {
                        Some(r) => r,
                        None => break,
                    },
                    () = frames.ready.notified() => match frames.take() {
                        Some(f) => f,
                        None => continue,
                    },
                };
                let mut text = serde_json::to_vec(&r).unwrap_or_default();
                text.push(b'\n');
                if wr.write_all(&text).await.is_err() {
                    // nobody reads: the subscription stops making frames for it
                    frames.close();
                    break;
                }
            }
        });
        match party {
            Err(why) => drop(out.send(Reply::No { why })),
            Ok(party) => {
                let (mut lines, mut typed) = (BufReader::new(rd).lines(), Typed::new());
                while let Ok(Some(text)) = lines.next_line().await {
                    match serde_json::from_str::<Request>(&text) {
                        Ok(req) => self.request(&party, req, &out, &slot, &mut typed),
                        Err(e) => drop(out.send(Reply::No { why: format!("not a request: {e}") })),
                    }
                }
            }
        }
        slot.end();
        drop(out);
        let _ = writer.await;
    }

    /// Carry out `req` from `party`; its replies go to `out`. A watch and a terminal's output keep coming after it
    /// returns, until `out` closes.
    fn request(self: &Rc<Self>, party: &Party, req: Request, out: &mpsc::UnboundedSender<Reply>, slot: &Rc<subscribe::Slot>, typed: &mut Typed) {
        let reply = match req {
            Request::Watch { from } => {
                spawn_local(self.clone().watch(from, out.clone()));
                Reply::Done { value: Json::Null }
            }
            Request::Older { before, n } => {
                let before = before.min(self.core.borrow().file.len());
                match self.fill(before.saturating_sub(n), before, out) {
                    Some(_) => Reply::Done { value: Json::Null },
                    None => Reply::No { why: "the record's file could not be read".into() },
                }
            }
            Request::Act { name, args } => self.change(|m| m.act(party, &name, &args)),
            Request::Answer { line, answer } => self.change(|m| m.answer(party, line, &answer)),
            Request::Query { query } => self.read(|m| m.query(party, &query)),
            Request::Holding { name } => self.read(|m| m.holding(party, &name)),
            Request::InstQuery { expr } => self.read(|m| m.inst_query(party, &expr)),
            Request::EnabledFor { actor, target } => self.read(|m| m.enabled_for(party, &actor, &target)),
            Request::Explain { inst } => self.read(|m| m.explain(party, &inst)),
            Request::Subscribe { types } => self.subscribe(types, slot),
            Request::Bytes { session, data } => self.typed(party, session, &data, out, typed),
        };
        let _ = out.send(reply);
    }

    /// A linked driver's answer, given `Later`, to the `effect` line at `line`.
    pub fn late(&self, line: u64, reply: Late) -> Reply {
        self.change(|m| m.late(line, reply).map(|()| Json::Null))
    }

    /// Whether `cond` holds now in module `module`'s names, asked by the daemon itself; false while the machine is
    /// busy or the condition cannot be read.
    pub fn holds(&self, module: &str, cond: &str) -> bool {
        self.core.try_borrow_mut().is_ok_and(|mut c| c.machine.holds(module, cond) == Ok(true))
    }

    /// An Event the driver at `driver` performs on its own.
    pub fn event(&self, driver: usize, ev: Instance) -> Reply {
        self.change(|m| m.event(driver, ev).map(|()| Json::Null))
    }

    /// Have GUARD asked before each change from now on: a change it refuses is refused, and the next one asks again.
    pub fn guard(&self, guard: Box<dyn Fn() -> Result<(), String>>) {
        self.core.borrow_mut().guard = Some(guard);
    }

    /// Refuse every change from now on, saying WHY; reads are served as before.
    pub fn refuse_changes(&self, why: String) {
        eprintln!("coordd-v2: no more changes: {why}");
        self.core.borrow_mut().refusing.get_or_insert(why);
    }

    /// Ask the machine for a change, put the lines it wrote in the file, then send them to every watcher.
    /// A request that writes no line: its answer, or why it was not taken.
    fn read(&self, ask: impl FnOnce(&mut dyn Machine) -> Result<Json, String>) -> Reply {
        let mut core = self.core.borrow_mut();
        match core.stopped.clone().map_or_else(|| ask(core.machine.as_mut()), Err) {
            Ok(value) => Reply::Done { value },
            Err(why) => Reply::No { why },
        }
    }

    fn change(&self, ask: impl FnOnce(&mut dyn Machine) -> Result<Json, String>) -> Reply {
        let mut core = self.core.borrow_mut();
        if let Some(why) = core.stopped.as_ref().or(core.refusing.as_ref()) {
            return Reply::No { why: why.clone() };
        }
        if let Some(Err(why)) = core.guard.as_ref().map(|g| g()) {
            return Reply::No { why };
        }
        let got = ask(core.machine.as_mut());
        for line in core.machine.written() {
            match core.file.append(&line) {
                Ok(at) => drop(self.lines.send((at, line))),
                Err(e) => {
                    let why = format!("the record took no more lines ({e}); the engine must start again, and replays it");
                    core.stopped = Some(why.clone());
                    return Reply::No { why };
                }
            }
        }
        let seq = *self.changes.borrow() + 1;
        self.step_delta(&mut core, seq);
        drop(core);
        self.changes.send_modify(|n| *n += 1);
        match got {
            Ok(value) => Reply::Done { value },
            Err(why) => Reply::No { why },
        }
    }

    /// The one delta of change SEQ, the view brought up to it, sent, while anyone takes them; else the view is let go.
    fn step_delta(&self, core: &mut Core, seq: u64) {
        if self.deltas.receiver_count() == 0 {
            if core.view.take().is_some() {
                core.machine.unwatch();
            }
            return;
        }
        let Some(view) = &mut core.view else { return };
        let got = core.machine.delta(view, seq).or_else(|_| {
            // what moved could not be read: the whole once, as a machine that cannot say does
            let now = core.machine.view(None)?;
            let d = crate::delta::between(view, &now, seq);
            *view = now;
            Ok::<_, String>(d)
        });
        if let Some(d) = got.ok().filter(|d| !d.is_empty()) {
            drop(self.deltas.send(d));
        }
    }

    /// Send `out` the lines from `next` up to `to` from the file, a chunk per borrow; where it got to.
    fn fill(&self, mut next: u64, to: u64, out: &mpsc::UnboundedSender<Reply>) -> Option<u64> {
        while next < to {
            let lines = self.core.borrow().file.read(next, to.min(next + CHUNK)).ok()?;
            for line in lines {
                out.send(Reply::Line { at: next, line }).ok()?;
                next += 1;
            }
        }
        Some(next)
    }

    /// The record from `from` on, then each new line, until `out` closes. What a slow watcher missed is read from
    /// the file, so it sees every line once and in order.
    async fn watch(self: Rc<Self>, from: u64, out: mpsc::UnboundedSender<Reply>) {
        let mut rx = self.lines.subscribe();
        let end = self.core.borrow().file.len();
        let Some(next) = self.fill(from, end, &out) else { return };
        let mut next = next.max(from);
        loop {
            match rx.recv().await {
                Ok((at, line)) if at >= next => {
                    let Some(n) = self.fill(next, at, &out) else { return };
                    if at == n && out.send(Reply::Line { at, line }).is_err() {
                        return;
                    }
                    next = at + 1;
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let end = self.core.borrow().file.len();
                    let Some(n) = self.fill(next, end, &out) else { return };
                    next = n;
                }
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    }

    /// Type `data` into `session`; on the connection's first bytes for it, its output starts coming to `out`.
    fn typed(&self, party: &Party, session: String, data: &str, out: &mpsc::UnboundedSender<Reply>, typed: &mut Typed) -> Reply {
        let bytes = match base64::engine::general_purpose::STANDARD.decode(data) {
            Ok(b) => b,
            Err(e) => return Reply::No { why: format!("not base64: {e}") },
        };
        if !typed.contains_key(&session) {
            let (input, mut output) = match self.terminals.open(party, &session) {
                Ok(t) => t,
                Err(why) => return Reply::No { why },
            };
            let (out, name) = (out.clone(), session.clone());
            spawn_local(async move {
                while let Ok(b) = output.recv().await {
                    if out.send(Reply::Bytes { session: name.clone(), data: base64::engine::general_purpose::STANDARD.encode(b) }).is_err() {
                        return;
                    }
                }
            });
            typed.insert(session.clone(), input);
        }
        match typed[&session].send(bytes) {
            Ok(()) => Reply::Done { value: Json::Null },
            Err(_) => {
                typed.remove(&session);
                Reply::No { why: format!("terminal {session} has closed") }
            }
        }
    }
}
