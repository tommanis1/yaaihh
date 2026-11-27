//! The joint (one-interpreter.md section 4.3; paper-research N-568, part 5): each line of a ticket's run as a phrase of
//! the yaaihh library's `run.eflint`, sent to eflint-rs beside the machine. An act of a person is an act with that
//! actor; what the program or the clock does is an event; what coordd reports (a session took the words) is a physical
//! act of the driver. An awaited step is the duty `await(agent, ticket, step)`.
//!
//! The switch `run-acts` (the coord's file of that name: `both`, else off; none: off). In `both` the machine stays
//! the truth: the lines go to a thread of their own after they are recorded, never under the core's lock, and what the
//! interpreter answers otherwise is one journal line per difference ([`Difference`]). The way back is `off`, written
//! into the file: the next input stops the thread ([`Watch`]).
//!
//! What it compares, for each input and the outputs it made:
//! - an act the machine took must be enabled in the interpreter (performed although not enabled is a difference);
//! - for each ticket the lines touch, the interpreter's `await` duties of that ticket must be the step the machine
//!   awaits there (none, or one: its agent and number).

use docket_machine::{Line, Machine, Status};
use eflint::replay::{DutyState, Event, Replay, Value};
use std::collections::{BTreeMap, BTreeSet};

/// The library's folder in eFLINT whole, under agent-base and under a project (part 3's, `policy::interp`).
pub const LIBRARY: &str = "policy/eflint";

/// The run's file in it.
pub const RUN: &str = "run.eflint";

/// The switch's file in the coord's folder. While the joint runs, the file is written only to turn it off: `both`
/// written over a running `both` empties the file for an instant, and an input landing then reads it as off until
/// coordd's next start (consult-3, N-580 p67).
pub const SWITCH: &str = "run-acts";

/// Whether the joint runs for the coord COORD: only when its `run-acts` file's first word is `both`. No file, an
/// empty one, `off` or anything else: off, so that an install starts it on no coord nobody switched (paper-research
/// N-580 p45; consult-3). Read once, at coordd's start.
pub fn switched_on(coord: &str) -> bool {
    std::fs::read_to_string(format!("{coord}/{SWITCH}")).is_ok_and(|s| s.split_whitespace().next() == Some("both"))
}

/// The switch's file as coordd last read it: its modification time. The joint is started only at coordd's start (or
/// a reload of the tickets); `off` written into the file stops it without a restart: before each send coordd stats the
/// file, and once it has changed and its first word is no longer `both` (or it is gone) the sender is dropped and the
/// thread ends with its loop (paper-research N-788). One stat per input with the joint on; the file is read only when
/// its time moved. `both` written back starts nothing until coordd's next start.
#[derive(Debug)]
pub struct Watch {
    path: String,
    seen: Option<std::time::SystemTime>,
}

impl Watch {
    pub fn new(coord: &str) -> Watch {
        let path = format!("{coord}/{SWITCH}");
        let seen = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        Watch { path, seen }
    }

    /// Whether the switch still says `both`: true while its file's time stands; read again when it moved.
    pub fn still_on(&mut self) -> bool {
        let now = std::fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        if now == self.seen {
            return true;
        }
        self.seen = now;
        std::fs::read_to_string(&self.path).is_ok_and(|s| s.split_whitespace().next() == Some("both"))
    }
}

/// Whether the joint also holds the agents' threads from the start (`both threads` in the switch's file): their
/// history is fed at start, at the joint's own time (research's copy: 47-104 s against 3-6 s, quick profile).
/// Off by default (paper-research N-580 p35, 3). The start's one replay keeps the agents' threads' history either way
/// (`History::cut`), so the switch can change between starts without a whole replay.
pub fn threads_on(coord: &str) -> bool {
    std::fs::read_to_string(format!("{coord}/{SWITCH}")).is_ok_and(|s| s.split_whitespace().any(|w| w == "threads"))
}

/// The library's run (agent-base's `policy/eflint/run.eflint`) and then the project's `policy/eflint/*.eflint` (its
/// `Extend` clauses), as one text. Only the run of the library's folder: its other files (memory.eflint) read the
/// prelude's facts, which only part 3's translation brings. Once part 3's `policy::interp::Spec` is in, its `text`
/// takes this one's place, so that roles, grants and the run are one state (docket-core8, N-568 p26).
pub fn library_text(base: &str, project: Option<&str>) -> Result<String, String> {
    let path = format!("{base}/{LIBRARY}/{RUN}");
    // the two types the dialect's files declare (part 3 loads them first), which run.eflint reads and leaves to them
    let mut text = String::from("Fact agent Identified by String\nFact ticket Identified by String\n");
    text.push_str(&std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?);
    if let Some(p) = project {
        let mut files: Vec<_> = std::fs::read_dir(format!("{p}/{LIBRARY}")).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "eflint")).collect();
        files.sort();
        for f in files {
            text.push('\n');
            text.push_str(&std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?);
        }
    }
    Ok(text)
}

/// What the machine shows of the tickets a group of lines touched, after it took them: the step each awaits (agent,
/// number) and how many posts each has. Saved with a start's replay (I-1509: [`History`]).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Seen {
    pub awaited: BTreeMap<u64, Option<(String, u32)>>,
    pub posts: BTreeMap<u64, u32>,
    /// Each ticket's id (`N-447`): what the library's `ticket` is.
    pub ids: BTreeMap<u64, String>,
    /// The tickets that ended (Done or Failed) and are no agents' thread: nothing more comes on them but an undo, so
    /// the joint lets their interpreters go (docket-core3, N-412 p485).
    pub gone: BTreeSet<u64>,
    /// The posts the machine holds back (an owner's post its handler has not told or dropped yet): they wake nobody
    /// until let through, so their asks closers wait (research's N-409 p12).
    pub held: BTreeSet<(u64, u32)>,
    /// The threads tied at a post that answer its asks (the machine's `Subthread` closer): the ticket and post tied
    /// at, and the post's writer. Whether the tied thread is typed to the writer or its first post wakes it is the
    /// machine's reading of that thread, which the joint may not hold (an agents' thread opened before the start).
    pub tied: BTreeSet<(u64, u32, String)>,
}

/// The tickets LINES name: each line's `t`, a move's `to`, a keep's `ts`, the tickets an event's bindings opened.
pub fn touched(lines: &[Line]) -> BTreeSet<u64> {
    let mut out = BTreeSet::new();
    for l in lines {
        let v = serde_json::to_value(l).unwrap_or_default();
        out.extend(v["t"].as_u64());
        out.extend(v["to"].as_u64());
        out.extend(v["ts"].as_array().into_iter().flatten().filter_map(|t| t.as_u64()));
        out.extend(v["fired"].as_array().into_iter().flatten().filter_map(|f| f["t"].as_u64()));
    }
    out
}

/// The tickets one recorded line names, as [`touched`] finds them, read from its text: at start each line of the
/// record is in hand as text, and reading these four fields costs far less than turning every line back into JSON
/// (the start's replay: 2.9 s more with the joint on, paper-research N-580 p18).
pub fn touched_text(raw: &str) -> Vec<u64> {
    #[derive(serde::Deserialize)]
    struct Names {
        t: Option<serde_json::Value>,
        to: Option<serde_json::Value>,
        ts: Option<serde_json::Value>,
        fired: Option<Vec<serde_json::Value>>,
    }
    let Ok(n) = serde_json::from_str::<Names>(raw) else { return Vec::new() };
    let mut out: Vec<u64> = n.t.and_then(|v| v.as_u64()).into_iter().chain(n.to.and_then(|v| v.as_u64())).collect();
    out.extend(n.ts.as_ref().and_then(|v| v.as_array()).into_iter().flatten().filter_map(|t| t.as_u64()));
    out.extend(n.fired.iter().flatten().filter_map(|f| f["t"].as_u64()));
    out
}

/// What M shows of the tickets LINES touched.
pub fn seen(m: &Machine, lines: &[Line]) -> Seen {
    seen_of_tickets(m, touched(lines))
}

/// What M shows of the tickets TS.
pub fn seen_of_tickets(m: &Machine, ts: impl IntoIterator<Item = u64>) -> Seen {
    let mut s = Seen::default();
    for t in ts {
        let Some(x) = m.tickets.get(&t) else { continue };
        let awaits = match &x.status {
            Status::Awaits { agent, owed, .. } => Some((agent.clone(), owed.step)),
            _ => None,
        };
        s.awaited.insert(t, awaits);
        s.posts.insert(t, x.posts.len() as u32);
        s.ids.insert(t, m.id(t));
        if !x.is_open() && !x.between() {
            s.gone.insert(t);
        }
        s.held.extend(x.posts.iter().filter(|p| p.held.is_some()).map(|p| (t, p.n)));
        // a thread tied at post N of X, opened after it, typed to its writer or first waking it (machine.rs `closers`)
        if let Some(Some((on, n))) = x.tie {
            let post = m.tickets.get(&on).filter(|_| on != t).and_then(|o| o.posts.iter().find(|p| p.n == n));
            if let Some(p) = post.filter(|p| x.opened_at > p.at) {
                let a = &p.by;
                let typed = x.form.get("to").and_then(serde_json::Value::as_str) == Some(a.as_str()) || x.form.get("typed").and_then(serde_json::Value::as_array).is_some_and(|ts| ts.iter().any(|v| v.as_str() == Some(a.as_str())));
                if typed || x.posts.first().is_some_and(|f| f.wakes().any(|w| w == a)) {
                    s.tied.insert((on, n, a.clone()));
                }
            }
        }
    }
    s
}

/// A way the interpreter answered other than the machine.
#[derive(Clone, Debug, PartialEq)]
pub enum Difference {
    /// The machine took the act; the interpreter says it was not enabled.
    NotEnabled { line: usize, label: String, act: String },
    /// The interpreter could not take the phrase at all.
    Error { line: usize, label: String, phrase: String, why: String },
    /// After the lines, ticket T awaits MACHINE in the machine and JOINT in the interpreter (its `await` duties).
    Duties { line: usize, t: u64, machine: Option<(String, u32)>, joint: Vec<(String, u32)> },
}

impl std::fmt::Display for Difference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Difference::NotEnabled { line, label, act } => write!(f, "line {line} ({label}): {act} was taken by the machine and is not enabled"),
            Difference::Error { line, label, phrase, why } => write!(f, "line {line} ({label}): {phrase} not taken: {why}"),
            Difference::Duties { line, t, machine, joint } => write!(f, "after line {line}: ticket {t} awaits {machine:?} in the machine, {joint:?} in the interpreter"),
        }
    }
}

/// One row of the table: a label (the record line's `ev`), how many lines of it, the phrases sent for them, those
/// performed although not enabled, and those not taken.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub lines: usize,
    pub phrases: usize,
    pub not_enabled: usize,
    pub errors: usize,
    /// The time the interpreter took on them.
    pub secs: f64,
    /// The longest one phrase took, and how many took over 50 ms.
    pub worst: f64,
    pub over_50ms: usize,
}

/// One ticket's interpreter: every fact of run.eflint is keyed by its ticket, so each ticket's run is its own state,
/// and a phrase costs what one ticket holds, not the whole record (paper-research N-762: no seconds).
struct One {
    replay: Replay,
    /// The instances of the library's plain types stated so far in it (`agent("ann")`, `ticket("N-5")`,
    /// `number(2)`): an act or event is enabled only for objects that hold, so the driver states each before its first
    /// phrase.
    stated: BTreeSet<(&'static str, String)>,
}

pub struct Joint {
    /// Whether post and step numbers are stated as instances before their first phrase (a measurement's switch).
    numbers: bool,
    /// The tickets it answers for: those whose run it has from their start (open when coordd started, so their
    /// history was fed; or opened since). None: every ticket (the table on a whole record). A ticket outside, such as
    /// an agents' thread opened before the start (`Between`: not open, yet posted on), has no state here, and its lines
    /// would each read as not enabled (docket-core4, N-580 p13)
    only: Option<BTreeSet<u64>>,
    /// The lines passed over as on a ticket not held (a quiet journal told from a blind one, paper-research N-580 p17).
    pub passed: usize,
    /// The library loaded once: each ticket's interpreter starts as a copy of it.
    loaded: Replay,
    /// Each ticket's interpreter, by the ticket's literal.
    ones: BTreeMap<String, One>,
    /// The step each ticket awaits as this driver sent it (`owe` without its end yet): what `release` names.
    owed: BTreeMap<u64, (String, u32)>,
    /// The posts of each ticket the machine showed after the last group, for numbering a `post` input's post.
    posts: BTreeMap<u64, u32>,
    /// The step each ticket's worker last answered with no post: owed again if the machine refuses that answer.
    answered: BTreeMap<u64, (String, u32)>,
    /// The tickets closed (an agents' thread from its start): there a post asks the owner only if it wakes him.
    shut: BTreeSet<u64>,
    /// The posts that wake the owner (name him, or are told to him).
    wake_owner: BTreeSet<(u64, u32)>,
    /// The highest post of each ticket a session took (`give` sent up to it).
    handed: BTreeMap<u64, u32>,
    /// Each ticket's id, as the machine showed it.
    ids: BTreeMap<u64, String>,
    /// Who wrote each post (ticket, number): the asker of what the watcher marks in it.
    wrote: BTreeMap<(u64, u32), String>,
    /// For each ticket and agent, the number below which its asks are closed already: its own later post, a post
    /// waking it, a step given it (up to the posts it shows, +1). The watcher marks a post seconds after it, and its
    /// closer may come first (research's N-315 p2, the owner's answer before the mark): the mark is sent, then this.
    reached: BTreeMap<(u64, String), u32>,
    /// The posts the machine held back after the group being taken, and those of them whose wakes wait: sent when
    /// the post is let through (`released`), dropped when it is told by its handler or made (`posted`, then `told`
    /// wakes whom it names) or dropped.
    held_now: BTreeSet<(u64, u32)>,
    /// The posts each ticket had after the group being taken: a `post` line whose number is beyond them made no post
    /// yet (research's N-409: the owner's words become post 13 at their `posted` line, seconds later).
    made_now: BTreeMap<u64, u32>,
    /// Each such post's wakes and the post whose asks it answers (the owner's card).
    waking: BTreeMap<(u64, u32), (Vec<String>, Option<u32>)>,
    /// The subthread closers sent: (ticket, post, writer).
    tied_sent: BTreeSet<(u64, u32, String)>,
    pub table: BTreeMap<String, Row>,
    /// Tickets compared after a group, and those that differed.
    pub compared: usize,
    pub differed: usize,
}

/// A value as an eFLINT literal: a string quoted, a number as it is.
fn lit(v: &Value) -> String {
    match v {
        Value::Str(x) => format!("\"{}\"", x.replace('\\', "\\\\").replace('"', "\\\"")),
        Value::Int(n) => n.to_string(),
        other => format!("{other:?}"),
    }
}

fn s(x: &str) -> Value {
    Value::Str(x.to_string())
}

fn i(x: impl Into<i64>) -> Value {
    Value::Int(x.into())
}

fn act(name: &str, actor: &str, objects: Vec<Value>) -> Event {
    Event::Act { name: name.into(), actor: s(actor), objects }
}

fn event(name: &str, objects: Vec<Value>) -> Event {
    Event::Event { name: name.into(), objects }
}

/// The phrase as text, for a journal line.
fn shown(e: &Event) -> String {
    let vals = |vs: &[Value]| vs.iter().map(|v| format!("{v:?}")).collect::<Vec<_>>().join(", ");
    match e {
        Event::Act { name, actor, objects } => format!("{name}({}{})", vals(std::slice::from_ref(actor)), if objects.is_empty() { String::new() } else { format!(", {}", vals(objects)) }),
        Event::Event { name, objects } => format!("{name}({})", vals(objects)),
        Event::Fact { name, objects, .. } => format!("{name}({})", vals(objects)),
        Event::Phrase(p) => p.clone(),
    }
}

impl Joint {
    /// The joint over TEXT (`library_text`).
    pub fn new(text: &str) -> Result<Joint, String> {
        Ok(Joint { numbers: true, only: None, passed: 0, loaded: Replay::from_text(text)?, ones: BTreeMap::new(), owed: BTreeMap::new(), answered: BTreeMap::new(), shut: BTreeSet::new(), wake_owner: BTreeSet::new(), posts: BTreeMap::new(), handed: BTreeMap::new(), ids: BTreeMap::new(), wrote: BTreeMap::new(), reached: BTreeMap::new(), held_now: BTreeSet::new(), made_now: BTreeMap::new(), waking: BTreeMap::new(), tied_sent: BTreeSet::new(), table: BTreeMap::new(), compared: 0, differed: 0 })
    }

    /// The joint over TEXT answering for the tickets ONLY from their start (and those opened since), as coordd's start
    /// holds them ([`only`]).
    pub fn holding(text: &str, only: BTreeSet<u64>) -> Result<Joint, String> {
        Ok(Joint { only: Some(only), ..Joint::new(text)? })
    }

    /// The library's types of a phrase's objects, in order (the actor of an act is an `agent`); `None`: a field of
    /// no type to state (a string).
    fn types(name: &str) -> &'static [Option<&'static str>] {
        const T: Option<&str> = Some("ticket");
        const N: Option<&str> = Some("number");
        const S: Option<&str> = Some("step");
        const A: Option<&str> = Some("agent");
        match name {
            "open-ticket" | "change" | "undo" | "end-thread" | "begin" | "rest" | "drive" | "close" => &[T],
            "answer" => &[T, S],
            "owe" | "release" | "expire" | "lapse" => &[A, T, S],
            "mark" => &[A, A, T, N],
            "give" | "wake" | "step-given" | "subthread" => &[A, T, N],
            "halt" => &[T, N, None],
            _ => &[T, N],
        }
    }

    /// The phrases (eFLINT text) that state each object of E (and its actor) as an instance of its type, the first time
    /// it is met.
    fn state(stated: &mut BTreeSet<(&'static str, String)>, e: &Event, numbers: bool, ends: &mut String) -> String {
        let (name, actor, objects) = match e {
            Event::Act { name, actor, objects } => (name, Some(actor), objects),
            Event::Event { name, objects } => (name, None, objects),
            _ => return String::new(),
        };
        let typed: Vec<(&'static str, String)> = actor.map(|a| ("agent", lit(a))).into_iter().chain(Joint::types(name).iter().zip(objects).filter_map(|(t, v)| t.map(|t| (t, lit(v))))).collect();
        let mut out = String::new();
        for (t, v) in typed {
            if !numbers && (t == "number" || t == "step") {
                // numbers stated for the phrase only and ended after it: kept, each post's and step's number makes
                // every later phrase on the ticket dearer (88 s against 29 s for research's threads)
                out.push_str(&format!("+{t}({v}).\n"));
                ends.push_str(&format!("-{t}({v}).\n"));
                continue;
            }
            if stated.insert((t, v.clone())) {
                out.push_str(&format!("+{t}({v}).\n"));
            }
        }
        out
    }

    /// E as one text of phrases: the instances it names stated first; for an act, its actor and ticket (and the number
    /// it names) stated as `acting` just before it and ended after, so that it is derived for this one instance, and a
    /// post's number, the record's, stated ahead. One text, so eflint-rs takes it in one step.
    fn text(stated: &mut BTreeSet<(&'static str, String)>, e: &Event, numbers: bool) -> String {
        let mut ends = String::new();
        let mut out = Joint::state(stated, e, numbers, &mut ends);
        match e {
            Event::Act { name, actor, objects } => {
                let a = lit(actor);
                let t = objects.first().map(lit).unwrap_or_default();
                let n = (Joint::types(name).get(1) == Some(&Some("number"))).then(|| objects.get(1).map(lit)).flatten();
                out.push_str(&format!("+acting({a}, {t}).\n"));
                if let Some(n) = &n {
                    out.push_str(&format!("+acting-on({n}).\n"));
                }
                if name == "post" {
                    out.push_str(&format!("+next-post({t}, {}).\n", n.clone().unwrap_or_default()));
                }
                let args: Vec<String> = std::iter::once(a.clone()).chain(objects.iter().map(lit)).collect();
                out.push_str(&format!("{name}({}).\n-acting({a}, {t}).\n", args.join(", ")));
                if let Some(n) = &n {
                    out.push_str(&format!("-acting-on({n}).\n"));
                }
            }
            Event::Event { name, objects } => out.push_str(&format!("{name}({}).\n", objects.iter().map(lit).collect::<Vec<_>>().join(", "))),
            _ => {}
        }
        out.push_str(&ends);
        out
    }

    /// The interpreter of the ticket E names, made from the loaded library the first time.
    fn one(&mut self, e: &Event) -> &mut One {
        let (name, objects) = match e {
            Event::Act { name, objects, .. } | Event::Event { name, objects } => (name.as_str(), objects.as_slice()),
            _ => ("", &[][..]),
        };
        let at = Joint::types(name).iter().position(|t| *t == Some("ticket"));
        let key = at.and_then(|k| objects.get(k)).map(lit).unwrap_or_default();
        let loaded = &self.loaded;
        self.ones.entry(key).or_insert_with(|| One { replay: Replay { cfg: loaded.cfg.clone() }, stated: BTreeSet::new() })
    }

    /// Ticket T as the library's `ticket`: its id.
    fn tk(&self, t: u64) -> Value {
        s(&self.ids.get(&t).cloned().unwrap_or_else(|| format!("T-{t}")))
    }

    /// The step's end with no answer: the duty this driver sent for T, if any, released.
    fn release(&mut self, t: u64, out: &mut Vec<Event>) {
        if let Some((a, n)) = self.owed.remove(&t) {
            let tk = self.tk(t);
            out.push(event("release", vec![s(&a), tk, i(n)]));
        }
    }

    /// Agent A's asks on T are closed below N (see `reached`).
    fn reach(&mut self, t: u64, a: &str, n: u32) {
        let r = self.reached.entry((t, a.to_string())).or_default();
        *r = (*r).max(n);
    }

    /// The wakes of post N of T by BY: each named or told (not BY) has its earlier asks answered.
    fn wakes(&mut self, t: u64, n: u32, by: &str, who: &[String], out: &mut Vec<Event>) {
        for a in who.iter().filter(|a| *a != by) {
            self.reach(t, a, n);
            out.push(event("wake", vec![s(a), self.tk(t), i(n)]));
        }
    }

    /// The phrases for LINE. A `post` input's number is the post after those the machine showed before.
    fn phrases(&mut self, line: &Line) -> Vec<Event> {
        let mut out = Vec::new();
        match line {
            Line::Open { t, opener, text, closed, .. } => {
                out.push(act("open-ticket", opener, vec![self.tk(*t)]));
                if text.is_some() {
                    self.wrote.insert((*t, 1), opener.clone());
                    out.push(act("post", opener, vec![self.tk(*t), i(1)]));
                }
                if closed.is_some() {
                    out.push(event("close", vec![self.tk(*t)]));
                }
            }
            Line::Opened { t, .. } => out.push(event("begin", vec![self.tk(*t)])),
            Line::Event { fired, .. } => out.extend(fired.iter().map(|f| event("begin", vec![self.tk(f.t)]))),
            Line::Post { t, by, answers, names, told, .. } => {
                let n = self.posts.get(t).copied().unwrap_or(0) + 1;
                self.posts.insert(*t, n);
                self.wrote.insert((*t, n), by.clone());
                out.push(act("post", by, vec![self.tk(*t), i(n)]));
                self.reach(*t, by, n);
                // whom it wakes (its names and whom it is told to) have their earlier asks answered; a post the machine
                // holds back wakes them only once let through
                if names.iter().chain(told).any(|a| a == "owner") {
                    self.wake_owner.insert((*t, n));
                }
                let who: Vec<String> = names.iter().chain(told).cloned().collect();
                // not on the ticket after its group (the owner's words, made a post only by `posted`), or held there
                if self.held_now.contains(&(*t, n)) || self.made_now.get(t).is_some_and(|m| *m < n) {
                    self.waking.insert((*t, n), (who, *answers));
                } else {
                    if let Some(a) = answers {
                        out.push(act("answer-asks", by, vec![self.tk(*t), i(*a)]));
                    }
                    self.wakes(*t, n, by, &who, &mut out);
                }
            }
            Line::Posted { t, n, by, step, .. } => {
                // the post that carries an answer: the answer itself, unless the answer line before it gave it (the
                // owner's pick: `answer`, then `posted` with its step; N-213, N-219 on research's record)
                if let Some(st) = (*step).filter(|st| !self.answered.get(t).is_some_and(|(_, a)| a == st)) {
                    out.push(act("answer", by, vec![self.tk(*t), i(st)]));
                    if self.owed.get(t).is_some_and(|(a, m)| a == by && *m == st) {
                        self.owed.remove(t);
                    }
                }
                self.wrote.insert((*t, *n), by.clone());
                self.reach(*t, by, *n);
                // a post made or told only now: it answers the card it carried, and wakes only whom the `told` line
                // names (the machine's `routed`)
                if let Some((_, Some(a))) = self.waking.remove(&(*t, *n)) {
                    out.push(act("answer-asks", by, vec![self.tk(*t), i(a)]));
                }
                out.push(act("post", by, vec![self.tk(*t), i(*n)]));
            }
            Line::Released { t, n, .. } => {
                if let Some((who, answers)) = self.waking.remove(&(*t, *n)) {
                    let by = self.wrote.get(&(*t, *n)).cloned().unwrap_or_default();
                    if let Some(a) = answers {
                        out.push(act("answer-asks", &by, vec![self.tk(*t), i(a)]));
                    }
                    self.wakes(*t, *n, &by, &who, &mut out);
                }
            }
            Line::Dropped { t, n, .. } => {
                self.waking.remove(&(*t, *n));
            }
            // the worker's answer to the step it was given, with no post (the watcher's sort, the minder's shape)
            Line::Answer { t, by, .. } => {
                if let Some((w, st)) = self.owed.remove(t) {
                    out.push(act("answer", by, vec![self.tk(*t), i(st)]));
                    self.answered.insert(*t, (w, st));
                }
            }
            // the answer did not type: the step is the worker's again, as the machine keeps it awaiting
            Line::Refused { t, step, .. } => {
                if let Some((w, st)) = self.answered.remove(t).filter(|(_, st)| st == step) {
                    out.push(event("owe", vec![s(&w), self.tk(*t), i(st)]));
                    self.owed.insert(*t, (w, st));
                }
            }
            Line::Replied { t, n, by, names, .. } => {
                self.wrote.insert((*t, *n), by.clone());
                out.push(act("post", by, vec![self.tk(*t), i(*n)]));
                self.reach(*t, by, *n);
                self.wakes(*t, *n, by, names, &mut out);
                if names.iter().any(|a| a == "owner") {
                    self.wake_owner.insert((*t, *n));
                }
            }
            Line::Step { t, step, worker, .. } => {
                if self.owed.get(t).is_some_and(|(a, n)| a != worker || n != step) {
                    self.release(*t, &mut out);
                }
                out.push(event("owe", vec![s(worker), self.tk(*t), i(*step)]));
                self.owed.insert(*t, (worker.clone(), *step));
                // a step owed anew: an answer given before is not this one's (consult-3, N-580 p35, 1)
                self.answered.remove(t);
            }
            Line::PassedUp { t, step, to, .. } => {
                self.release(*t, &mut out);
                out.push(event("owe", vec![s(to), self.tk(*t), i(*step)]));
                self.owed.insert(*t, (to.clone(), *step));
                self.answered.remove(t);
            }
            Line::Expired { t, .. } => {
                if let Some((a, n)) = self.owed.remove(t) {
                    out.push(event("expire", vec![s(&a), self.tk(*t), i(n)]));
                }
            }
            Line::Lapsed { t, step, .. } => out.push(event("lapse", vec![s("owner"), self.tk(*t), i(*step)])),
            Line::Stopped { t, .. } => self.release(*t, &mut out),
            Line::Closed { t, .. } => {
                self.shut.insert(*t);
                self.release(*t, &mut out);
                out.push(event("close", vec![self.tk(*t)]));
            }
            Line::Rests { t, .. } => out.push(event("rest", vec![self.tk(*t)])),
            Line::Shaped { t, refused: None, .. } => out.push(event("drive", vec![self.tk(*t)])),
            Line::Edited { t, by, what, .. } => {
                out.push(act("change", by, vec![self.tk(*t)]));
                if what != "edit" {
                    self.release(*t, &mut out);
                }
            }
            Line::Undo { t, by, .. } | Line::Reopen { t, by, .. } => {
                self.shut.remove(t);
                out.push(act("undo", by, vec![self.tk(*t)]));
            }
            Line::Ended { t, by, .. } => out.push(act("end-thread", by, vec![self.tk(*t)])),
            Line::TakenBack { t, n, by, .. } => out.push(act("take-back", by, vec![self.tk(*t), i(*n)])),
            Line::Given { t, to, upto, delivery, .. } => {
                // a step's delivery (`N-11.s2`, the step itself: `step_id`) closes the asks of its worker up to the
                // posts it shows
                if docket_machine::step_id(delivery).is_some_and(|(_, _, itself)| itself) {
                    out.push(event("step-given", vec![s(to), self.tk(*t), i(*upto)]));
                    self.reach(*t, to, upto + 1);
                }
                let from = self.handed.get(t).copied().unwrap_or(0);
                for n in from + 1..=*upto {
                    out.push(event("give", vec![s(to), self.tk(*t), i(n)]));
                }
                if *upto > from {
                    self.handed.insert(*t, *upto);
                }
            }
            // what a post asks the owner: on a closed ticket only a post that wakes him asks him (the machine's
            // `asks_open`, design-two-systems section 16, 10)
            Line::Marked { t, n, .. } if !self.shut.contains(t) || self.wake_owner.contains(&(*t, *n)) => {
                if let Some(by) = self.wrote.get(&(*t, *n)).cloned() {
                    out.push(event("mark", vec![s("owner"), s(&by), self.tk(*t), i(*n)]));
                    // closed before the mark came: the closer, stated again after it
                    if let Some(r) = self.reached.get(&(*t, by.clone())).copied().filter(|r| r > n) {
                        out.push(event("wake", vec![s(&by), self.tk(*t), i(r)]));
                    }
                    if self.tied_sent.iter().any(|(x, m, a)| x == t && m == n && *a == by) {
                        out.push(event("subthread", vec![s(&by), self.tk(*t), i(*n)]));
                    }
                }
            }
            // whom policy tells of a post made without asking it (an agent's turn reply): they are woken by it as by
            // its names, and told to the owner on a closed ticket it asks him (research's N-101 p1257, a reply naming
            // two agents that policy tells the owner)
            Line::Notified { t, to, post: Some(n), .. } => {
                if to.iter().any(|a| a == "owner") {
                    self.wake_owner.insert((*t, *n));
                }
                let by = self.wrote.get(&(*t, *n)).cloned().unwrap_or_default();
                self.wakes(*t, *n, &by, to, &mut out);
            }
            Line::Told { t, n, to, .. } => {
                let by = self.wrote.get(&(*t, *n)).cloned().unwrap_or_default();
                self.wakes(*t, *n, &by, to, &mut out);
            }
            Line::AsksWithdrawn { t, n, by, .. } => out.push(act("withdraw-asks", by, vec![self.tk(*t), i(*n)])),
            Line::NothingToAnswer { t, n, by, .. } => out.push(act("nothing-to-answer", by, vec![self.tk(*t), i(*n)])),
            Line::Halted { t, n, what, .. } => out.push(event("halt", vec![self.tk(*t), i(*n), s(what)])),
            Line::Approve { t, n, by, .. } => out.push(act("approve", by, vec![self.tk(*t), i(*n)])),
            Line::HaltEdit { t, n, by, .. } => out.push(act("halt-edit", by, vec![self.tk(*t), i(*n)])),
            _ => {}
        }
        out
    }

    /// The `await` duties of DUTIES held for T: (agent, step) each.
    fn duties_of(&self, duties: &[DutyState], t: u64) -> Vec<(String, u32)> {
        let claimant = format!("ticket({:?})", self.ids.get(&t).cloned().unwrap_or_else(|| format!("T-{t}")));
        let mut out: Vec<(String, u32)> = duties
            .iter()
            .filter(|d| d.claimant.as_ref().is_some_and(|c| c.to_string() == claimant))
            .filter_map(|d| {
                let text = d.duty.to_string();
                let agent = text.strip_prefix("await(agent(\"")?.split("\"),").next()?.to_string();
                let step = text.rsplit("step(").next()?.trim_end_matches(')').parse().ok()?;
                Some((agent, step))
            })
            .collect();
        out.sort();
        out
    }

    /// Take GROUP (an input and the outputs it made; LINE, the input's number in the record) as the machine took it,
    /// SEEN what it shows after: the differences.
    pub fn take(&mut self, line: usize, group: &[Line], seen: &Seen) -> Vec<Difference> {
        let mut diffs = Vec::new();
        self.ids.extend(seen.ids.iter().map(|(t, id)| (*t, id.clone())));
        self.held_now.clone_from(&seen.held);
        self.made_now.clone_from(&seen.posts);
        for l in group {
            let mut reseed = None;
            if let Some(only) = &mut self.only {
                match l {
                    Line::Open { t, .. } | Line::Opened { t, .. } => {
                        only.insert(*t);
                    }
                    // an ended ticket undone (or an item reopened) is held again: its interpreter went at its end, so
                    // it starts anew from `closed`, which is what the undo reads (consult-3, N-580 p26, 2)
                    Line::Undo { t, .. } | Line::Reopen { t, .. } if !only.contains(t) => {
                        only.insert(*t);
                        reseed = Some(*t);
                    }
                    // the tickets an event opens (his messages: `owner_message`) come with no open line of their own
                    // (consult-3, N-580 p20)
                    Line::Event { fired, .. } => only.extend(fired.iter().map(|f| f.t)),
                    _ => {}
                }
                if !touched(std::slice::from_ref(l)).is_subset(only) {
                    self.passed += 1;
                    continue;
                }
            }
            if let Some(t) = reseed {
                let tk = self.tk(t);
                self.ones.remove(&lit(&tk));
                let one = self.one(&event("close", vec![tk.clone()]));
                one.stated.insert(("ticket", lit(&tk)));
                one.replay.apply(&Event::Phrase(format!("+ticket({0}).\n+closed({0}).\n", lit(&tk))));
            }
            let label = serde_json::to_value(l).ok().and_then(|v| v["ev"].as_str().map(str::to_string)).unwrap_or_default();
            let phrases = self.phrases(l);
            let row = self.table.entry(label.clone()).or_default();
            row.lines += 1;
            row.phrases += phrases.len();
            for p in phrases {
                let started = std::time::Instant::now();
                let numbers = self.numbers;
                let one = self.one(&p);
                let text = Joint::text(&mut one.stated, &p, numbers);
                let a = one.replay.apply(&Event::Phrase(text));
                let row = self.table.get_mut(&label).expect("entered above");
                let took = started.elapsed().as_secs_f64();
                row.secs += took;
                row.worst = row.worst.max(took);
                row.over_50ms += usize::from(took > 0.05);
                if !a.ok || !a.errors.is_empty() {
                    row.errors += 1;
                    diffs.push(Difference::Error { line, label: label.clone(), phrase: shown(&p), why: a.errors.join("; ") });
                } else if !a.disabled.is_empty() {
                    row.not_enabled += 1;
                    diffs.push(Difference::NotEnabled { line, label: label.clone(), act: shown(&p) });
                }
            }
        }
        // a thread tied at a post, answering its asks: on the ticket tied at, if held here, once
        for (x, n, a) in &seen.tied {
            if self.only.as_ref().is_some_and(|o| !o.contains(x)) || !self.tied_sent.insert((*x, *n, a.clone())) {
                continue;
            }
            let p = event("subthread", vec![s(a), self.tk(*x), i(*n)]);
            let numbers = self.numbers;
            let one = self.one(&p);
            let text = Joint::text(&mut one.stated, &p, numbers);
            let r = one.replay.apply(&Event::Phrase(text));
            if !r.ok || !r.errors.is_empty() {
                diffs.push(Difference::Error { line, label: "tied".into(), phrase: shown(&p), why: r.errors.join("; ") });
            }
        }
        self.posts.extend(seen.posts.iter().map(|(t, n)| (*t, *n)));
        // an ended ticket's interpreter goes, and the joint answers for it no more (an undo takes it back, from `closed`); on
        // the whole record (no `only`) each is kept, as a ticket's lines may follow its end there
        if let Some(only) = &mut self.only {
            for t in &seen.gone {
                if only.remove(t) {
                    let key = lit(&s(&self.ids.get(t).cloned().unwrap_or_else(|| format!("T-{t}"))));
                    self.ones.remove(&key);
                }
            }
        }
        if !seen.awaited.is_empty() {
            for (t, machine) in &seen.awaited {
                if self.only.as_ref().is_some_and(|o| !o.contains(t)) {
                    continue;
                }
                self.compared += 1;
                let duties = self.ones.get(&lit(&self.tk(*t))).map(|o| o.replay.duties()).unwrap_or_default();
                let joint = self.duties_of(&duties, *t);
                let same = match machine {
                    None => joint.is_empty(),
                    Some(m) => joint.len() == 1 && &joint[0] == m,
                };
                if !same {
                    self.differed += 1;
                    diffs.push(Difference::Duties { line, t: *t, machine: machine.clone(), joint });
                }
            }
        }
        diffs
    }
}

/// An input's lines as the live coordd recorded them: the input's line number in the record, the lines, what the
/// machine showed after.
pub type Group = (usize, Vec<Line>, Seen);

/// How many differences of the record's history are journalled one by one at start; the rest are counted.
const HISTORY_SHOWN: usize = 20;

/// The tickets the joint holds from its start, as M ends the record's replay: those open, and with BETWEEN the agents'
/// threads (paper-research N-762; N-580 p17).
pub fn only(m: &Machine, between: bool) -> BTreeSet<u64> {
    m.tickets.values().filter(|x| x.is_open() || (between && x.between())).map(|x| x.t).collect()
}

/// An input of the record the joint's history keeps: its index among the record's lines that read and the end of its
/// outputs, the bytes of the record its lines stand in (their place: the lines are read back from the record, not
/// saved), and what the machine showed after it.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Kept {
    pub i: usize,
    pub j: usize,
    pub bytes: (usize, usize),
    pub seen: Seen,
}

/// The joint's history as coordd's one start replay takes it along (I-1509; paper-research N-1446 p1644), so that the
/// joint's thread no longer replays the record on a second machine: each input with what the machine showed after it,
/// kept at each cut only where it names a ticket held there, and saved with the replay's state, the joint on or off.
/// At the start the joint is fed those of them that name a ticket it holds ([`only`]): what a whole second replay gave.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct History {
    pub kept: Vec<Kept>,
    /// The tickets ended at the last cut: neither open nor an agents' thread, nor left one by the log at the replay's
    /// end. Their inputs before the cut are not kept, so lines after it that hold one again (reopened, undone) need the
    /// whole record replayed ([`History::held_again`]).
    pub ended: BTreeSet<u64>,
}

/// Whether the joint's history keeps the inputs of X at a cut in M: it may be held at the record's end.
fn held_at_cut(m: &Machine, x: &docket_machine::Ticket) -> bool {
    x.is_open() || x.between() || m.log_leaves_between(x.t)
}

impl History {
    /// The input I of the record taken as M stands after it, PLACES the bytes of its lines (the input, then its outputs)
    /// in TEXT, the record: what M shows of the tickets they name.
    pub fn take(&mut self, m: &Machine, i: usize, text: &str, places: &[(usize, usize)]) {
        let (Some(first), Some(last)) = (places.first(), places.last()) else { return };
        let ts: BTreeSet<u64> = places.iter().flat_map(|(a, b)| touched_text(&text[*a..*b])).collect();
        self.kept.push(Kept { i, j: i + places.len(), bytes: (first.0, last.1), seen: seen_of_tickets(m, ts) });
    }

    /// Cut where M stands: only the inputs that name a ticket held there kept, the others' tickets ended.
    pub fn cut(&mut self, m: &Machine) {
        self.ended = m.tickets.values().filter(|x| !held_at_cut(m, x)).map(|x| x.t).collect();
        let ended = &self.ended;
        self.kept.retain(|k| k.seen.ids.keys().any(|t| !ended.contains(t)));
    }

    /// A ticket ended at the last cut that M, gone on since, holds again: its inputs before the cut were not kept.
    pub fn held_again(&self, m: &Machine) -> Option<u64> {
        self.ended.iter().copied().find(|t| m.tickets.get(t).is_some_and(|x| held_at_cut(m, x)))
    }

    /// The inputs kept that name a ticket of ONLY, in order, with their lines read back from TEXT (the record) at their
    /// place; a line there that does not read was not among the record's lines either, and is left out as it was.
    pub fn fed<'a>(&'a self, text: &'a str, only: &'a BTreeSet<u64>) -> impl Iterator<Item = (usize, Vec<Line>, &'a Seen)> + 'a {
        self.kept.iter().filter(|k| k.seen.ids.keys().any(|t| only.contains(t))).map(move |k| (k.i, text[k.bytes.0..k.bytes.1].lines().filter_map(|l| serde_json::from_str::<Line>(l).ok()).collect(), &k.seen))
    }
}

/// The history the joint was fed at start before I-1509, and the reference its [`History`] is tested against: RECORD's
/// first AT lines (as coordd read them), replayed on a second machine with DEFS; the inputs that touch a ticket open at
/// the end, each with what the machine showed after it; and those tickets. Also: the heap the replay held at its end
/// (glibc's count of the heap in use, then less before), the second machine of the record with its parsed lines,
/// dropped when this returns (paper-research N-580 p35, 3).
#[cfg(test)]
pub fn history(defs: docket_machine::Defs, record: &str, at: usize, between: bool) -> (Vec<Line>, Vec<(usize, usize, Seen)>, BTreeSet<u64>, i64) {
    let in_use = || unsafe { libc::mallinfo2() }.uordblks as i64;
    let before = in_use();
    let (lines, names): (Vec<Line>, Vec<Vec<u64>>) = record.lines().filter_map(|l| serde_json::from_str::<Line>(l).ok().map(|x| (x, touched_text(l)))).take(at).unzip();
    let mut m = Machine::load(defs);
    let mut seen: Vec<(usize, usize, Seen)> = Vec::new();
    m.replay_with(&lines, &mut |m, i, group| seen.push((i, i + group.len(), seen_of_tickets(m, names[i..i + group.len()].iter().flatten().copied().collect::<BTreeSet<u64>>()))));
    let only = only(&m, between);
    seen.retain(|(_, _, s)| s.ids.keys().any(|t| only.contains(t)));
    let held = in_use() - before;
    (lines, seen, only, held)
}

/// The start's history fed to JOINT: the inputs of HISTORY that name a ticket it holds, their lines read from TEXT, the
/// record (I-1509). Each difference to EACH; how many inputs were fed.
pub fn feed(joint: &mut Joint, text: &str, history: &History, each: &mut dyn FnMut(Difference)) -> usize {
    let only = joint.only.clone().unwrap_or_default();
    let mut n = 0;
    for (i, lines, seen) in history.fed(text, &only) {
        joint.take(i + 1, &lines, seen).into_iter().for_each(&mut *each);
        n += 1;
    }
    n
}

/// What a joint holds after a feed, its timings left out (I-1509's test of the history against [`history`]'s).
#[cfg(test)]
#[derive(Debug, PartialEq)]
pub struct Held {
    pub compared: usize,
    pub differed: usize,
    pub passed: usize,
    /// The table's counts by label: lines, phrases, not enabled, not taken.
    pub table: BTreeMap<String, (usize, usize, usize, usize)>,
    /// The tickets held, and each ticket's duties in its interpreter.
    pub only: Option<BTreeSet<u64>>,
    pub duties: BTreeMap<String, Vec<String>>,
}

#[cfg(test)]
impl Joint {
    pub fn held(&self) -> Held {
        let table = self.table.iter().map(|(k, r)| (k.clone(), (r.lines, r.phrases, r.not_enabled, r.errors))).collect();
        let duties = self
            .ones
            .iter()
            .map(|(k, o)| {
                let mut d: Vec<String> = o.replay.duties().iter().map(|d| format!("{} {:?}", d.duty, d.claimant.as_ref().map(|c| c.to_string()))).collect();
                d.sort();
                (k.clone(), d)
            })
            .collect();
        Held { compared: self.compared, differed: self.differed, passed: self.passed, table, only: self.only.clone(), duties }
    }
}

/// coordd's resident memory in MB (`/proc/self/statm`), 0 where it cannot be read.
fn resident_mb() -> u64 {
    let pages = std::fs::read_to_string("/proc/self/statm").ok().and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok()).unwrap_or(0);
    pages * (unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(0) as u64) / 1048576
}

/// Start the joint's thread (`run-acts both`): the HISTORY coordd's start replay took along (each input with what the
/// machine showed after it), those of its inputs that name a ticket of ONLY fed with their lines read from RECORD, then
/// each group the live coordd sends. ONLY: the tickets held from the start ([`only`]; with THREADS, the agents' threads
/// too). Differences are journal lines; nothing here changes what the machine does. None: the library cannot be read
/// or the thread not started (said once in the journal), and the joint does not run.
pub fn start(base: &str, project: Option<&str>, record: String, history: History, only: BTreeSet<u64>, threads: bool) -> Option<std::sync::mpsc::Sender<Group>> {
    let text = match library_text(base, project) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("coordd: joint: the library's run cannot be read, the joint does not run: {e}");
            return None;
        }
    };
    let (tx, rx) = std::sync::mpsc::channel::<Group>();
    // the interpreter holds `Rc`s: it is made in its thread and never leaves it (docket-core8, N-568 p26). One that
    // does not load ends the thread: the next send fails, and coordd stops sending
    let spawned = std::thread::Builder::new().name("joint".into()).spawn(move || {
        // only the history of the tickets held is fed: a closed ticket's facts and duties are ended, and its lines would
        // cost the joint's start for nothing (paper-research N-762); with `both threads`, the agents' threads too: they
        // carry most of the traffic, and a thread opened before the start is not compared otherwise (N-580 p17). The
        // machine's view after each input was taken along coordd's own replay, so no second machine replays the record
        // here (I-1509, p1644): only the inputs fed are read back from the record, at their place
        let text_mb = record.len() / 1048576;
        let mut joint = match Joint::holding(&text, only) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("coordd: joint: the library's run does not load, the joint does not run: {e}");
                return;
            }
        };
        let started = std::time::Instant::now();
        let mut n = 0usize;
        let fed = feed(&mut joint, &record, &history, &mut |d| {
            if n < HISTORY_SHOWN {
                eprintln!("coordd: joint: {d}");
            }
            n += 1;
        });
        let kept = history.kept.len();
        drop(history);
        drop(record);
        // the history and the record's text were freed on this thread, into its own arena, which glibc keeps (and RSS
        // counts) until handed back (paper-research N-580 p62)
        // glibc walks every arena, not only this thread's, holding each one's lock while it walks it: a worker allocating
        // there waits meanwhile, so the line says how long it took; and the figure after is coordd's, not the joint's
        // alone (consult-3, N-580 p67)
        let (heap, resident) = (unsafe { libc::mallinfo2() }.uordblks / 1048576, resident_mb());
        let trimming = std::time::Instant::now();
        unsafe { libc::malloc_trim(0) };
        let took = trimming.elapsed().as_millis();
        eprintln!("coordd: joint: the start's memory handed back in {took} ms: heap in use {heap} MB; coordd resident {resident} MB before, {} MB after", resident_mb());
        eprintln!("coordd: joint: the held tickets' history through the joint in {:.1} s, {fed} of the {kept} inputs the start's replay kept, read back from the record's {text_mb} MB: {n} differences ({} shown above), {} tickets compared, {} differing; {} tickets held from their start ({}), {} lines passed over", started.elapsed().as_secs_f64(), n.min(HISTORY_SHOWN), joint.compared, joint.differed, joint.only.as_ref().map_or(0, |o| o.len()), if threads { "open, and agents' threads" } else { "open; agents' threads opened before it passed over" }, joint.passed);
        // the running count of lines passed over, said with the first difference of each hour (paper-research N-580
        // p35, 2): a quiet journal told from a blind one, with no timer of its own
        let mut said_hour: Option<u64> = None;
        for (at, group, seen) in rx {
            // the live lines come as one input with its outputs, as the machine made them
            for d in joint.take(at, &group, &seen) {
                let hour = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() / 3600);
                if said_hour != Some(hour) {
                    said_hour = Some(hour);
                    eprintln!("coordd: joint: this hour's first difference follows; since the start {} lines passed over (tickets not held), {} tickets held", joint.passed, joint.only.as_ref().map_or(0, |o| o.len()));
                }
                eprintln!("coordd: joint: {d}");
            }
        }
        eprintln!("coordd: joint: its thread ends ({} tickets compared, {} differing since the start); started again only at coordd's start with run-acts both", joint.compared, joint.differed);
    });
    match spawned {
        Ok(_) => Some(tx),
        Err(e) => {
            eprintln!("coordd: joint: its thread could not start, the joint does not run: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base() -> String {
        format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"))
    }

    fn line(v: serde_json::Value) -> Line {
        serde_json::from_value(v).unwrap()
    }

    fn seen(t: u64, awaits: Option<(&str, u32)>, posts: u32) -> Seen {
        Seen { awaited: [(t, awaits.map(|(a, n)| (a.to_string(), n)))].into(), posts: [(t, posts)].into(), ids: [(t, format!("N-{t}"))].into(), ..Seen::default() }
    }

    #[test]
    fn a_tickets_run_is_acts_and_its_awaited_step_a_duty() {
        let mut j = Joint::new(&library_text(&base(), None).unwrap()).unwrap();
        let at = "2026-11-18T20:00:00Z";
        let open = [line(json!({"ev": "open", "at": at, "t": 5, "kind": "note", "opener": "ann", "form": {}, "program": ""})), line(json!({"ev": "step", "at": at, "t": 5, "step": 1, "worker": "bob", "call": "reply"}))];
        assert_eq!(j.take(1, &open, &seen(5, Some(("bob", 1)), 0)), vec![]);
        // a post by someone else changes nothing owed
        assert_eq!(j.take(2, &[line(json!({"ev": "post", "at": at, "t": 5, "by": "ann", "text": "x"}))], &seen(5, Some(("bob", 1)), 1)), vec![]);
        // the session took posts 1: given; ann may no longer take it back
        assert_eq!(j.take(3, &[line(json!({"ev": "given", "at": at, "t": 5, "to": "bob", "delivery": "d", "upto": 1}))], &seen(5, Some(("bob", 1)), 1)), vec![]);
        let back = j.take(4, &[line(json!({"ev": "taken_back", "at": at, "t": 5, "n": 1, "by": "ann"}))], &Seen::default());
        assert!(matches!(&back[..], [Difference::NotEnabled { label, .. }] if label == "taken_back"), "{back:?}");
        // only the worker answers the step: bob's answer ends the duty, and the ticket closes
        let answer = [line(json!({"ev": "posted", "at": at, "t": 5, "n": 2, "by": "bob", "step": 1})), line(json!({"ev": "closed", "at": at, "t": 5}))];
        assert_eq!(j.take(5, &answer, &seen(5, None, 2)), vec![]);
        // the machine still awaiting a step the interpreter has no duty for is a difference
        let d = j.take(6, &[], &seen(5, Some(("bob", 1)), 2));
        assert!(matches!(&d[..], [Difference::Duties { t: 5, joint, .. }] if joint.is_empty()), "{d:?}");
        assert_eq!(j.table["taken_back"].not_enabled, 1);
        assert_eq!(j.table["given"].phrases, 1);
    }

    /// Started on the tickets open then, the joint answers for those and for the tickets opened since; a thread
    /// opened before the start (no history fed) is passed over, not read as a post not enabled. A refused answer
    /// leaves the step owed, and a post may answer asks that have ended.
    /// The posts of T whose asks the joint holds open.
    fn asks(j: &Joint, t: u64) -> BTreeSet<u32> {
        let key = lit(&s(&format!("N-{t}")));
        j.ones.get(&key).map(|o| o.replay.duties().iter().filter(|d| d.duty.dom.to_string() == "asked").filter_map(|d| d.duty.to_string().rsplit("number(").next()?.trim_end_matches(')').parse().ok()).collect()).unwrap_or_default()
    }

    #[test]
    fn asks_close_as_the_machine_closes_them_whichever_line_comes_first() {
        let mut j = Joint { only: Some([5].into()), ..Joint::new(&library_text(&base(), None).unwrap()).unwrap() };
        let at = "2026-11-18T20:00:00Z";
        let post = |by: &str, names: &[&str]| line(json!({"ev": "post", "at": at, "t": 5, "by": by, "text": "x", "names": names}));
        let marked = |n: u32| line(json!({"ev": "marked", "at": at, "t": 5, "n": n, "marks": [{"at": [0, 1]}]}));
        assert_eq!(j.take(1, &[line(json!({"ev": "open", "at": at, "t": 5, "kind": "note", "opener": "ann", "form": {}, "program": ""}))], &seen(5, None, 0)), vec![]);
        // the owner answers ann's post 1 before the watcher marks it (research's N-315 p2): marked, it asks nothing
        assert_eq!(j.take(2, &[post("ann", &["owner"])], &seen(5, None, 1)), vec![]);
        assert_eq!(j.take(3, &[post("owner", &["ann"])], &seen(5, None, 2)), vec![]);
        assert_eq!(j.take(4, &[marked(1)], &seen(5, None, 2)), vec![]);
        assert_eq!(asks(&j, 5), BTreeSet::new());
        // the owner's post 4, held back by its handler, answers ann's post 3 only once let through (N-409 p12)
        assert_eq!(j.take(5, &[post("ann", &["owner"]), marked(3)], &seen(5, None, 3)), vec![]);
        assert_eq!(asks(&j, 5), [3].into());
        let held = Seen { held: [(5, 4)].into(), ..seen(5, None, 4) };
        assert_eq!(j.take(6, &[post("owner", &["ann"])], &held), vec![]);
        assert_eq!(asks(&j, 5), [3].into());
        assert_eq!(j.take(7, &[line(json!({"ev": "released", "at": at, "t": 5, "n": 4}))], &seen(5, None, 4)), vec![]);
        assert_eq!(asks(&j, 5), BTreeSet::new());
        // the owner's words made post 6 only by their `posted` line (N-409 p12): ann's post 5 is answered then, not before
        assert_eq!(j.take(8, &[post("ann", &["owner"]), marked(5)], &seen(5, None, 5)), vec![]);
        let card = line(json!({"ev": "post", "at": at, "t": 5, "by": "owner", "text": "x", "names": ["ann"], "answers": 5}));
        assert_eq!(j.take(9, &[card], &seen(5, None, 5)), vec![]);
        assert_eq!(asks(&j, 5), [5].into());
        let made = [line(json!({"ev": "posted", "at": at, "t": 5, "n": 6, "by": "owner"})), line(json!({"ev": "told", "at": at, "t": 5, "n": 6, "to": ["ann"]}))];
        assert_eq!(j.take(10, &made, &seen(5, None, 6)), vec![]);
        assert_eq!(asks(&j, 5), BTreeSet::new());
        // a thread tied at ann's post 7 and typed to her answers it (N-446 p4, N-234 p12), though the joint does not
        // hold that thread (an agents' thread from before the start)
        assert_eq!(j.take(11, &[post("ann", &["owner"]), marked(7)], &seen(5, None, 7)), vec![]);
        assert_eq!(asks(&j, 5), [7].into());
        let tied = Seen { tied: [(5, 7, "ann".to_string())].into(), ..seen(8, None, 1) };
        assert_eq!(j.take(12, &[line(json!({"ev": "tied", "at": at, "t": 8, "to": 5, "n": 7, "by": "ann"}))], &tied), vec![]);
        assert_eq!(asks(&j, 5), BTreeSet::new());
        // on a closed ticket, a reply naming another agent that policy tells the owner asks him (N-101 p1257)
        assert_eq!(j.take(13, &[line(json!({"ev": "closed", "at": at, "t": 5}))], &seen(5, None, 7)), vec![]);
        let reply = [line(json!({"ev": "replied", "at": at, "t": 5, "n": 8, "by": "ann", "names": ["bob"]})), line(json!({"ev": "notified", "at": at, "t": 5, "event": "post", "to": ["owner"], "post": 8, "provenance": {}}))];
        assert_eq!(j.take(14, &reply, &seen(5, None, 8)), vec![]);
        assert_eq!(j.take(15, &[marked(8)], &seen(5, None, 8)), vec![]);
        assert_eq!(asks(&j, 5), [8].into());
    }

    #[test]
    fn the_joint_answers_for_the_tickets_it_has_from_their_start() {
        let mut j = Joint { only: Some([5].into()), ..Joint::new(&library_text(&base(), None).unwrap()).unwrap() };
        let at = "2026-11-18T20:00:00Z";
        // ticket 9, an agents' thread from before the start: its post is not answered for
        assert_eq!(j.take(1, &[line(json!({"ev": "replied", "at": at, "t": 9, "n": 40, "by": "ann"}))], &seen(9, None, 40)), vec![]);
        assert_eq!(j.compared, 0);
        // ticket 7 opened since: answered for from its open
        let open = [line(json!({"ev": "open", "at": at, "t": 7, "kind": "note", "opener": "ann", "form": {}, "program": ""})), line(json!({"ev": "step", "at": at, "t": 7, "step": 1, "worker": "bob", "call": "reply"}))];
        assert_eq!(j.take(2, &open, &seen(7, Some(("bob", 1)), 0)), vec![]);
        // bob's answer does not type: still his
        let refused = [line(json!({"ev": "answer", "at": at, "t": 7, "by": "bob", "head": {"head": "Granted"}, "text": ""})), line(json!({"ev": "refused", "at": at, "t": 7, "step": 1, "why": "n: null is not Int"}))];
        assert_eq!(j.take(3, &refused, &seen(7, Some(("bob", 1)), 0)), vec![]);
        // the answer, then its post with the step (as the owner's pick is recorded): one answer
        let typed = [line(json!({"ev": "answer", "at": at, "t": 7, "by": "bob", "head": {"head": "Granted", "n": "1"}, "text": "1"})), line(json!({"ev": "posted", "at": at, "t": 7, "n": 1, "by": "bob", "step": 1}))];
        assert_eq!(j.take(4, &typed, &seen(7, None, 1)), vec![]);
        // the same step owed anew (passed up to carol) and answered by her post: no duty left
        let again = [line(json!({"ev": "passed_up", "at": at, "t": 7, "step": 1, "from": "bob", "to": "carol", "why": "x"})), line(json!({"ev": "posted", "at": at, "t": 7, "n": 2, "by": "carol", "step": 1}))];
        assert_eq!(j.take(4, &again, &seen(7, None, 2)), vec![]);
        // ann's post answers post 1, which asked nothing (no mark): taken as the machine takes it
        assert_eq!(j.take(5, &[line(json!({"ev": "post", "at": at, "t": 7, "by": "ann", "text": "x", "answers": 1}))], &seen(7, None, 1)), vec![]);
        assert_eq!(j.compared, 5);
        // ticket 8, opened by his message: answered for from the event
        let fired = [line(json!({"ev": "event", "at": at, "name": "owner_message", "value": {}, "fired": [{"t": 8, "kind": "thread", "binding": "m", "form": {}, "program": ""}]}))];
        assert_eq!(j.take(8, &fired, &seen(8, None, 0)), vec![]);
        assert_eq!(j.take(9, &[line(json!({"ev": "replied", "at": at, "t": 8, "n": 1, "by": "ann"}))], &seen(8, None, 1)), vec![]);
        assert_eq!(j.compared, 7);
        // ticket 7 closes and ends (no thread): its interpreter goes, and a later line on it is passed over
        let closed = Seen { gone: [7].into(), ..seen(7, None, 1) };
        assert_eq!(j.take(6, &[line(json!({"ev": "closed", "at": at, "t": 7}))], &closed), vec![]);
        assert!(!j.ones.contains_key("\"N-7\"") && !j.only.as_ref().unwrap().contains(&7));
        assert_eq!(j.take(7, &[line(json!({"ev": "replied", "at": at, "t": 7, "n": 2, "by": "ann"}))], &seen(7, None, 2)), vec![]);
        // undone, it is held again from `closed`: the undo is enabled, and a post after it too
        assert_eq!(j.take(10, &[line(json!({"ev": "undo", "at": at, "t": 7, "by": "owner"}))], &seen(7, None, 2)), vec![]);
        assert!(j.only.as_ref().unwrap().contains(&7));
        assert_eq!(j.take(11, &[line(json!({"ev": "replied", "at": at, "t": 7, "n": 3, "by": "ann"}))], &seen(7, None, 3)), vec![]);
        assert_eq!(j.table["undo"].phrases, 1);
    }

    /// run.eflint loads after the dialect's files through part 3's `interp::load` (way 1: `agent` and `ticket` are
    /// the dialect's, N-580 p8), and the joint runs on that one text. Only run.eflint of the library's folder: the
    /// memory text's `rests` is renamed `thing-rests` on its own branch (docket-core5, E3).
    #[test]
    fn the_run_loads_with_the_dialects_files_through_part_3() {
        let root = std::env::temp_dir().join(format!("joint-part3-{}", std::process::id()));
        std::fs::create_dir_all(root.join("policy/eflint")).unwrap();
        for e in std::fs::read_dir(format!("{}/policy", base())).unwrap().flatten() {
            if e.path().extension().is_some_and(|x| x == "eflint") {
                std::fs::copy(e.path(), root.join("policy").join(e.file_name())).unwrap();
            }
        }
        std::fs::copy(format!("{}/{LIBRARY}/{RUN}", base()), root.join("policy/eflint").join(RUN)).unwrap();
        // and the system's files in eFLINT whole that take a dialect file's place (`// replaces:`), as part 3 reads
        // them: text-reader.eflint's `Extend Fact` is written out by its own
        for e in std::fs::read_dir(format!("{}/policy/eflint", base())).unwrap().flatten() {
            if std::fs::read_to_string(e.path()).is_ok_and(|t| t.starts_with("// replaces:")) {
                std::fs::copy(e.path(), root.join("policy/eflint").join(e.file_name())).unwrap();
            }
        }
        let places = crate::policy::Places { base: root.display().to_string(), ..Default::default() };
        let spec = crate::policy::interp::load(&places).unwrap();
        assert!(spec.text.contains("Duty await"), "the run is in part 3's text");
        let mut j = Joint::new(&spec.text).unwrap();
        let at = "2026-11-18T22:00:00Z";
        let open = [line(json!({"ev": "open", "at": at, "t": 5, "kind": "note", "opener": "ann", "form": {}, "program": ""})), line(json!({"ev": "step", "at": at, "t": 5, "step": 1, "worker": "bob", "call": "reply"}))];
        assert_eq!(j.take(1, &open, &seen(5, Some(("bob", 1)), 0)), vec![]);
        let answer = [line(json!({"ev": "posted", "at": at, "t": 5, "n": 1, "by": "bob", "step": 1})), line(json!({"ev": "closed", "at": at, "t": 5}))];
        assert_eq!(j.take(2, &answer, &seen(5, None, 1)), vec![]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_switch_is_on_only_when_its_file_says_both() {
        let dir = std::env::temp_dir().join(format!("joint-threads-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let coord = dir.to_string_lossy().to_string();
        // the agents' threads are held only with `both threads`
        assert!(!threads_on(&coord));
        std::fs::write(dir.join(SWITCH), "both\n").unwrap();
        assert!(!threads_on(&coord));
        std::fs::write(dir.join(SWITCH), "both threads\n").unwrap();
        assert!(threads_on(&coord) && switched_on(&coord));
        std::fs::remove_dir_all(&dir).unwrap();
        let dir = std::env::temp_dir().join(format!("joint-switch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let coord = dir.to_string_lossy().to_string();
        // no file: off
        assert!(!switched_on(&coord));
        std::fs::write(dir.join(SWITCH), "off\n").unwrap();
        assert!(!switched_on(&coord));
        std::fs::write(dir.join(SWITCH), "").unwrap();
        assert!(!switched_on(&coord));
        std::fs::write(dir.join(SWITCH), "on\n").unwrap();
        assert!(!switched_on(&coord));
        std::fs::write(dir.join(SWITCH), "both\n").unwrap();
        assert!(switched_on(&coord));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The table on a coord's record (N-568 part 5): JOINT_COORD, a copy of its folder; JOINT_BASE, agent-base;
    /// JOINT_PROJECT, its project's folder. Prints a row per label and the first differences.
    #[test]
    #[ignore]
    fn a_coords_record_through_the_joint() {
        let (Ok(coord), Ok(base)) = (std::env::var("JOINT_COORD"), std::env::var("JOINT_BASE")) else { return };
        let project = std::env::var("JOINT_PROJECT").ok();
        // JOINT_LINES=N[,M...]: the record's first N lines only, then M, each afresh (cuts that end in minutes,
        // paper-research N-762); none: the whole record
        // ("all": the whole record; "open": the whole record, the tickets open at its end only, as JOINT_OPEN)
        let open = std::env::var("JOINT_OPEN").is_ok();
        let cuts: Vec<(Option<usize>, bool)> = match std::env::var("JOINT_LINES") {
            Ok(v) => v
                .split(',')
                .filter_map(|c| match c.trim() {
                    "all" => Some((None, open)),
                    "open" => Some((None, true)),
                    c => c.parse().ok().map(|n| (Some(n), open)),
                })
                .collect(),
            Err(_) => vec![(None, open)],
        };
        for (cut, open) in cuts {
            table(&coord, &base, project.as_deref(), cut, open);
        }
    }

    fn table(coord: &str, base: &str, project: Option<&str>, cut: Option<usize>, open: bool) {
        let (defs, errors, _, _) = crate::ticket::Tickets::read_defs(&crate::policy::Places { base: base.to_string(), project: project.map(str::to_string), coord: coord.to_string(), ..Default::default() }, 1);
        assert!(errors.is_empty(), "{errors:?}");
        let mut m = Machine::load(defs);
        let text = std::fs::read_to_string(format!("{coord}/tickets/log.jsonl")).unwrap();
        let (mut lines, mut names): (Vec<Line>, Vec<Vec<u64>>) = text.lines().filter_map(|l| serde_json::from_str(l).ok().map(|x| (x, touched_text(l)))).unzip();
        if let Some(n) = cut {
            lines.truncate(n);
            names.truncate(n);
        }
        let started = std::time::Instant::now();
        let read: usize = text.lines().take(lines.len()).map(|l| touched_text(l).len()).sum();
        println!("reading each line's tickets from its text (paid at coordd's load with the joint on): {:.2} s, {read} names", started.elapsed().as_secs_f64());
        // the tickets read from each line's text are those `touched` finds in the line
        let unlike = lines.iter().zip(&names).filter(|(l, n)| touched(std::slice::from_ref(*l)) != n.iter().copied().collect::<BTreeSet<u64>>()).count();
        println!("lines whose tickets read from the text are not touched's: {unlike}");
        assert_eq!(unlike, 0);
        let mut j = Joint::new(&library_text(base, project).unwrap()).unwrap();
        // JOINT_NUMBERS=0: post and step numbers not stated as instances
        j.numbers = std::env::var("JOINT_NUMBERS").map_or(true, |v| v != "0");
        let mut diffs: Vec<Difference> = Vec::new();
        let mut groups: Vec<(usize, usize, Seen)> = Vec::new();
        // the start's replay with the switch off (`replay`) and on (`replay_with` and `seen` per input), as coordd's
        // load pays it before the socket answers (consult-3, N-580 p18)
        let (defs_off, _, _, _) = crate::ticket::Tickets::read_defs(&crate::policy::Places { base: base.to_string(), project: project.map(str::to_string), coord: coord.to_string(), ..Default::default() }, 1);
        let mut off = Machine::load(defs_off);
        let started = std::time::Instant::now();
        off.replay(&lines);
        let secs_off = started.elapsed().as_secs_f64();
        drop(off);
        let started = std::time::Instant::now();
        // the asks the machine holds open after each input, by ticket (the table only: too dear for coordd's lock)
        let mut asks_at: BTreeMap<usize, BTreeMap<u64, BTreeSet<u32>>> = BTreeMap::new();
        let problems = m.replay_with(&lines, &mut |m, i, group| {
            let ts: BTreeSet<u64> = names[i..i + group.len()].iter().flatten().copied().collect();
            let asks = ts.iter().filter_map(|t| m.tickets.get(t).map(|x| (*t, x.posts.iter().filter(|p| p.asks_open && m.asks_open(x, p.n)).map(|p| p.n).collect()))).collect();
            asks_at.insert(i, asks);
            groups.push((i, i + group.len(), seen_of_tickets(m, ts)))
        });
        println!("the start's replay: {secs_off:.1} s in coordd's load (the same with run-acts on: the joint's own replay runs on its thread), {:.1} s on the joint's thread with what the machine shows per input", started.elapsed().as_secs_f64());
        // JOINT_OPEN=1: only the groups of the tickets open at the end, as coordd's start feeds the joint
        let all = groups.len();
        if open {
            // JOINT_BETWEEN: the agents' threads (Between) too, open or not
            let between = std::env::var("JOINT_BETWEEN").is_ok();
            let only: BTreeSet<u64> = m.tickets.values().filter(|x| x.is_open() || (between && x.between())).map(|x| x.t).collect();
            println!("tickets held from the start: {} ({} open, {} agents' threads)", only.len(), m.tickets.values().filter(|x| x.is_open()).count(), m.tickets.values().filter(|x| x.between()).count());
            groups.retain(|(_, _, s)| s.ids.keys().any(|t| only.contains(t)));
            // what the joint's thread replays for itself at start (`history`) is this, and what it costs there
            let (defs_h, _, _, _) = crate::ticket::Tickets::read_defs(&crate::policy::Places { base: base.to_string(), project: project.map(str::to_string), coord: coord.to_string(), ..Default::default() }, 1);
            let started = std::time::Instant::now();
            let (_, h, only_h, held) = history(defs_h, &text, lines.len(), between);
            println!("the joint's thread's own history (parse, replay, what the machine shows): {:.1} s; its machine and parsed lines held {:.0} MB of heap at the replay's end, beside the record's text ({:.0} MB)", started.elapsed().as_secs_f64(), held as f64 / 1048576.0, text.len() as f64 / 1048576.0);
            assert!(h == groups && only_h == only, "history() differs from the table's own replay");
            j.only = Some(only);
        }
        let rss = || std::fs::read_to_string("/proc/self/status").unwrap_or_default().lines().find(|l| l.starts_with("VmRSS")).map(|l| l.split_whitespace().nth(1).and_then(|k| k.parse::<u64>().ok()).unwrap_or(0)).unwrap_or(0);
        let before = rss();
        // the heap in use (glibc's count): what the feed allocates and keeps, which resident memory cannot show when
        // memory freed before (the machine replayed with the switch off) is reused
        let in_use = || unsafe { libc::mallinfo2() }.uordblks as u64;
        let in_use_before = in_use();
        let started = std::time::Instant::now();
        let (mut asks_compared, mut asks_differ, mut asks_shown) = (0usize, 0usize, Vec::new());
        for (i, k, seen) in &groups {
            diffs.extend(j.take(i + 1, &lines[*i..*k], seen));
            for (t, machine) in asks_at.get(i).into_iter().flatten() {
                if j.only.as_ref().is_some_and(|o| !o.contains(t)) {
                    continue;
                }
                let key = lit(&j.tk(*t));
                let joint: BTreeSet<u32> = j.ones.get(&key).map(|o| o.replay.duties().iter().filter(|d| d.duty.dom.to_string() == "asked").filter_map(|d| d.duty.to_string().rsplit("number(").next()?.trim_end_matches(')').parse().ok()).collect()).unwrap_or_default();
                asks_compared += 1;
                if &joint != machine {
                    asks_differ += 1;
                    if asks_shown.len() < 10 {
                        asks_shown.push(format!("after line {}: {key} asks open in the machine {machine:?}, in the interpreter {joint:?}", i + 1));
                    }
                }
            }
        }
        println!("asks compared after a group: {asks_compared}, differing: {asks_differ}");
        for a in &asks_shown {
            println!("  {a}");
        }
        println!("memory: resident {} MB before the joint's feed, {} MB after; heap in use {:.1} MB more after it (the other tests of the binary run beside it), for {} tickets' interpreters", before / 1024, rss() / 1024, (in_use() as f64 - in_use_before as f64) / 1048576.0, j.ones.len());
        println!("{} lines{}, {} replay problems; the joint took {} of {all} inputs in {:.1} s, {} tickets' interpreters", lines.len(), if open { " (open tickets only)" } else { "" }, problems.len(), groups.len(), started.elapsed().as_secs_f64(), j.ones.len());
        println!("| label | lines | phrases | not enabled | not taken | s | ms per phrase | worst ms | over 50 ms |");
        for (label, r) in &j.table {
            if r.phrases > 0 || r.not_enabled > 0 || r.errors > 0 {
                println!("| {label} | {} | {} | {} | {} | {:.1} | {:.2} | {:.1} | {} |", r.lines, r.phrases, r.not_enabled, r.errors, r.secs, 1000.0 * r.secs / r.phrases.max(1) as f64, 1000.0 * r.worst, r.over_50ms);
            }
        }
        let unmapped: usize = j.table.values().filter(|r| r.phrases == 0).map(|r| r.lines).sum();
        println!("lines with no phrase (the driver's own): {unmapped}");
        println!("tickets compared after a group: {}, differing: {}", j.compared, j.differed);
        // what the largest tickets' interpreters hold, by type (and how many transitions and duties they list)
        let mut sizes: Vec<(usize, &String)> = j.ones.iter().map(|(k, o)| (o.replay.cfg.state.holding().count(), k)).collect();
        sizes.sort_by(|a, b| b.cmp(a));
        for (n, k) in sizes.iter().take(3) {
            let o = &j.ones[*k];
            let mut by: BTreeMap<String, usize> = BTreeMap::new();
            for te in o.replay.cfg.state.holding() {
                *by.entry(te.dom.to_string()).or_default() += 1;
            }
            println!("  {k}: {n} holding {by:?}; {} transitions, {} duties", o.replay.cfg.transitions.len(), o.replay.cfg.duties.len());
        }
        let mut by: BTreeMap<String, usize> = BTreeMap::new();
        for d in &diffs {
            let k = match d {
                Difference::NotEnabled { label, act, .. } => format!("not enabled: {label} {}", act.split('(').next().unwrap_or("")),
                Difference::Error { label, .. } => format!("not taken: {label}"),
                Difference::Duties { machine, joint, .. } => format!("duties: machine {} joint {}", if machine.is_some() { "one" } else { "none" }, joint.len()),
            };
            *by.entry(k).or_default() += 1;
        }
        for (k, n) in &by {
            println!("  {n:6} {k}");
        }
        let show: usize = std::env::var("JOINT_SHOW").ok().and_then(|s| s.parse().ok()).unwrap_or(12);
        for d in diffs.iter().take(show) {
            println!("  {d}");
        }
    }

}
