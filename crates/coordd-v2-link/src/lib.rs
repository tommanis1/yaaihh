//! A window's side of coordd-v2's socket: its requests and a terminal's bytes, each reply matched to its request, and a
//! subscription's deltas.

// The socket answers a connection's requests in the order they came, each with one `done` or `no` (coordd-v2-wire),
// so a reply is the oldest waiting request's; a watch's is taken here. What waits when a connection drops is lost, not
// refused: the record says whether it was taken. The next connection watches again from the first line not yet seen.

pub mod pipe;

pub use coordd_v2_wire::Answer;
use base64::Engine as _;
use coordd_v2_wire::{Reply, Request};
use pipe::{Connect, Pipe};
use serde_json::Value as Json;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The shortest and longest pause before a new connection is tried.
const PAUSE: (Duration, Duration) = (Duration::from_millis(200), Duration::from_secs(30));

/// A connection that stayed up this long was no failure at once: the next pause is the shortest.
const RAN: Duration = Duration::from_secs(10);

/// What comes back.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    /// The record's line at place `at`, from the watch or from `older`.
    Line { at: u64, line: Json },
    /// Request `id` gave `value`.
    Done { id: u64, value: Json },
    /// Request `id` was not taken, and why; `None` for the watch, or when nothing waited (the connection was refused).
    No { id: Option<u64>, why: String },
    /// Request `id` was sent and its connection ended before its reply: whether it was taken, only the record says.
    Lost { id: u64, why: String },
    /// Output of terminal `session`.
    Bytes { session: String, data: Vec<u8> },
    /// What moved in the subscription's types (`subscribe`), as [`coordd_v2_wire::Reply::Delta`] gives it, or what holds
    /// of them anew ([`coordd_v2_wire::Reply::Reset`]) for a reader too far behind.
    Delta(Reply),
    /// A connection is open, watching from `from` if a watch was asked.
    Up { from: Option<u64> },
    /// The connection ended, and why; a new one is tried.
    Down(String),
}

/// Called whenever something has come, so the window draws it.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct Shared {
    wr: Option<Box<dyn Write + Send>>,
    end: Option<Box<dyn FnMut() + Send>>,
    next: u64,
    /// The requests sent on this connection whose `done` or `no` has not come, oldest first; `None` is a watch.
    waiting: VecDeque<Option<u64>>,
    /// The place of the first watched line not yet seen, once a watch is asked.
    watch: Option<u64>,
}

/// The engine as a window reaches it, kept open in the background.
pub struct Link {
    shared: Arc<Mutex<Shared>>,
    rx: Receiver<Incoming>,
    ended: Arc<AtomicBool>,
}

impl Link {
    /// Keep a connection open by `connect`, opening a new one when it ends; `wake` is called when something comes.
    pub fn start(mut connect: Connect, wake: Wake) -> Link {
        let (tx, rx) = channel();
        let (shared, ended) = (Arc::new(Mutex::new(Shared::default())), Arc::new(AtomicBool::new(false)));
        let (held, stop) = (shared.clone(), ended.clone());
        std::thread::spawn(move || {
            let mut pause = PAUSE.0;
            while !stop.load(Ordering::SeqCst) {
                let began = Instant::now();
                let why = match connect() {
                    Ok(mut p) => {
                        let mut said = std::mem::replace(&mut p.said, Box::new(|| None));
                        let why = serve(p, &held, &tx, &wake);
                        let end = held.lock().unwrap().end.take();
                        if let Some(mut end) = end {
                            end();
                        }
                        said().map_or(why.clone(), |s| format!("{why}; it said: {s}"))
                    }
                    Err(e) => e.to_string(),
                };
                let mut s = held.lock().unwrap();
                (s.wr, s.end) = (None, None);
                let failed: Vec<u64> = s.waiting.drain(..).flatten().collect();
                drop(s);
                for id in failed {
                    let _ = tx.send(Incoming::Lost { id, why: why.clone() });
                }
                if tx.send(Incoming::Down(why)).is_err() {
                    return;
                }
                wake();
                pause = if began.elapsed() > RAN { PAUSE.0 } else { (pause * 2).min(PAUSE.1) };
                std::thread::sleep(pause);
            }
        });
        Link { shared, rx, ended }
    }

    /// The record from line `from` on, then each line as it is written; after a dropped connection, from the first
    /// line not yet seen. False while no connection is open: the next one watches from `from`.
    pub fn watch(&self, from: u64) -> bool {
        let mut s = self.shared.lock().unwrap();
        s.watch = Some(from);
        let line = to_line(&Request::Watch { from });
        let sent = s.wr.as_mut().is_some_and(|w| w.write_all(&line).and_then(|_| w.flush()).is_ok());
        if sent {
            s.waiting.push_back(None);
        }
        sent
    }

    /// At most `n` lines before line `before`, oldest first, as [`Incoming::Line`], then its `done`.
    pub fn older(&self, before: u64, n: u64) -> Option<u64> {
        self.ask(Request::Older { before, n })
    }

    /// Perform the Act or Event `name` with `args`, as the party the engine knows this connection for.
    pub fn act(&self, name: &str, args: Vec<Json>) -> Option<u64> {
        self.ask(Request::Act { name: name.into(), args })
    }

    /// This party's answer to the `effect` line at `line`.
    pub fn answer(&self, line: u64, answer: Answer) -> Option<u64> {
        self.ask(Request::Answer { line, answer })
    }

    /// The answer to `query` in the facts that hold now.
    pub fn query(&self, query: &str) -> Option<u64> {
        self.ask(Request::Query { query: query.into() })
    }

    /// Every instance of Fact or Duty `name`, a full name, that holds now.
    pub fn holding(&self, name: &str) -> Option<u64> {
        self.ask(Request::Holding { name: name.into() })
    }

    /// The values of instance query `expr`, a `Foreach x, ... : value When condition`.
    pub fn inst_query(&self, expr: &str) -> Option<u64> {
        self.ask(Request::InstQuery { expr: expr.into() })
    }

    /// For each act instance in `target` (one, or a list), whether `actor` may perform it now, and why not.
    pub fn enabled_for(&self, actor: Json, target: Json) -> Option<u64> {
        self.ask(Request::EnabledFor { actor, target })
    }

    /// What of `types` holds now, then an [`Incoming::Delta`] per change; a new connection must subscribe again once `Up`.
    pub fn subscribe(&self, types: Option<Vec<String>>) -> Option<u64> {
        self.ask(Request::Subscribe { types })
    }

    /// Why instance `inst` holds or not, or an Act or Event is enabled or not, now.
    pub fn explain(&self, inst: &str) -> Option<u64> {
        self.ask(Request::Explain { inst: inst.into() })
    }

    /// `data` typed into terminal `session`; its output comes as [`Incoming::Bytes`] from the first of these on.
    pub fn bytes(&self, session: &str, data: &[u8]) -> Option<u64> {
        self.ask(Request::Bytes { session: session.into(), data: base64::engine::general_purpose::STANDARD.encode(data) })
    }

    /// Send `req`; its `done` or `no` comes with the id returned. None while no connection is open.
    fn ask(&self, req: Request) -> Option<u64> {
        let mut s = self.shared.lock().unwrap();
        s.next += 1;
        let id = s.next;
        let line = to_line(&req);
        let w = s.wr.as_mut()?;
        w.write_all(&line).and_then(|_| w.flush()).ok()?;
        s.waiting.push_back(Some(id));
        Some(id)
    }

    /// What has come since the last call.
    pub fn drain(&self) -> Vec<Incoming> {
        self.rx.try_iter().collect()
    }

    /// The next thing to come, waiting up to `d`.
    pub fn next(&self, d: Duration) -> Option<Incoming> {
        match self.rx.recv_timeout(d) {
            Ok(i) => Some(i),
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => None,
        }
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.ended.store(true, Ordering::SeqCst);
        if let Some(end) = self.shared.lock().unwrap().end.as_mut() {
            end();
        }
    }
}

/// One connection, until it ends: the watch asked again, then each reply passed on. Why it ended.
fn serve(p: Pipe, held: &Mutex<Shared>, tx: &Sender<Incoming>, wake: &Wake) -> String {
    let Pipe { rd, mut wr, end, .. } = p;
    let mut s = held.lock().unwrap();
    s.end = Some(end);
    let from = s.watch;
    if let Some(from) = from
        && let Err(e) = wr.write_all(&to_line(&Request::Watch { from })).and_then(|_| wr.flush())
    {
        return e.to_string();
    }
    if from.is_some() {
        s.waiting.push_back(None);
    }
    s.wr = Some(wr);
    drop(s);
    let _ = tx.send(Incoming::Up { from });
    wake();
    for text in BufReader::new(rd).lines() {
        let text = match text {
            Ok(t) => t,
            Err(e) => return e.to_string(),
        };
        let reply = match serde_json::from_str::<Reply>(&text) {
            Ok(r) => r,
            Err(e) => return format!("not a reply: {e}"),
        };
        let mut s = held.lock().unwrap();
        let came = match reply {
            Reply::Line { at, line } => {
                if s.watch == Some(at) {
                    s.watch = Some(at + 1);
                }
                Incoming::Line { at, line }
            }
            Reply::Done { value } => match s.waiting.pop_front() {
                Some(Some(id)) => Incoming::Done { id, value },
                Some(None) => continue,
                None => return "a `done` came that no request waited for: replies no longer match requests".into(),
            },
            Reply::No { why } => Incoming::No { id: s.waiting.pop_front().flatten(), why },
            d @ (Reply::Delta { .. } | Reply::Reset { .. }) => Incoming::Delta(d),
            Reply::Bytes { session, data } => match base64::engine::general_purpose::STANDARD.decode(&data) {
                Ok(data) => Incoming::Bytes { session, data },
                Err(e) => return format!("terminal {session}'s output is not base64: {e}"),
            },
        };
        drop(s);
        if tx.send(came).is_err() {
            return "the window has gone".into();
        }
        wake();
    }
    "the engine closed the connection".into()
}

fn to_line(req: &Request) -> Vec<u8> {
    let mut line = serde_json::to_vec(req).unwrap_or_default();
    line.push(b'\n');
    line
}
