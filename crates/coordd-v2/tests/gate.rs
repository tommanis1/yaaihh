//! The gate's scripts run on the booted daemon: each `.steps` file of the folders given, its text loaded, each step asked for over the socket as its party, its results read from the record a watch gives (two-library-gate.md).

#[path = "gate/script.rs"]
mod script;

use coordd_v2::codec::{of_value, read_line};
use coordd_v2::docket::Docket;
use coordd_v2::link::{link, wait_on};
use coordd_v2::peer::{Party, Vouch};
use coordd_v2::serve::{Engine, Terminals};
use docket_v2::check::Loaded;
use docket_v2::run::record::Line;
use docket_v2::run::value::{Instance, Value};
use script::{answer, call, faults, fields, instance, outcomes, qualify, stated, value, written, Answer, StandIn};
use std::io::Write;
use serde_json::{json, Value as Json};
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};
use tokio::task::{spawn_local, LocalSet};

/// The folders whose scripts run, under the design folder: the gate's, and the programs' scenarios.
const FOLDERS: [&str; 2] = ["gate", "scenarios"];

fn design() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docket")
}

/// Stands in for the parties' table: each new connection is the next party named, all of them served by the
/// stand-in, as one test process is one cgroup.
struct Next(Rc<RefCell<VecDeque<String>>>);

impl Vouch for Next {
    fn party(&self, _: &str) -> Option<Party> {
        self.0.borrow_mut().pop_front().map(|name| Party::of(&name, Some(0)))
    }
}

struct NoTerminals;

impl Terminals for NoTerminals {
    fn open(&self, _: &Party, s: &str) -> Result<(mpsc::UnboundedSender<Vec<u8>>, broadcast::Receiver<Vec<u8>>), String> {
        Err(format!("no terminal {s}"))
    }
}

/// One connection to the daemon: what it reads, and what it writes.
struct Conn(Lines<BufReader<OwnedReadHalf>>, OwnedWriteHalf);

impl Conn {
    async fn ask(&mut self, req: Json) -> Result<(), String> {
        self.1.write_all(format!("{req}\n").as_bytes()).await.map_err(|e| e.to_string())
    }
    async fn next(&mut self) -> Result<Json, String> {
        let l = tokio::time::timeout(Duration::from_secs(5), self.0.next_line()).await.map_err(|_| "no reply in 5 s")?;
        serde_json::from_str(&l.map_err(|e| e.to_string())?.ok_or("the daemon closed the connection")?).map_err(|e| e.to_string())
    }
    /// The `done` value of a request; `no` is an error with its reason.
    async fn done(&mut self, req: Json) -> Result<Json, String> {
        self.ask(req).await?;
        let r = self.next().await?;
        match r["is"].as_str() {
            Some("done") => Ok(r["value"].clone()),
            _ => Err(format!("not taken: {}", r["why"])),
        }
    }
}

/// The booted daemon of a script: its record's folder, the parties still to connect, a connection for each party,
/// and the watch's lines read so far.
struct Daemon {
    dir: PathBuf,
    next: Rc<RefCell<VecDeque<String>>>,
    sock: PathBuf,
    conns: BTreeMap<String, Conn>,
    watch: Conn,
    lines: Vec<Line>,
}

impl Daemon {
    async fn conn(&mut self, party: &str) -> Result<&mut Conn, String> {
        if !self.conns.contains_key(party) {
            self.next.borrow_mut().push_back(party.to_string());
            let (rd, wr) = UnixStream::connect(&self.sock).await.map_err(|e| e.to_string())?.into_split();
            self.conns.insert(party.to_string(), Conn(BufReader::new(rd).lines(), wr));
        }
        Ok(self.conns.get_mut(party).unwrap())
    }

    /// Every line the record has now, read from the watch: the last line's place is asked of the record first.
    async fn caught_up(&mut self) -> Result<(), String> {
        let c = self.conn("gate").await?;
        c.ask(json!({"do": "older", "before": u64::MAX, "n": 1})).await?;
        let last = c.next().await?["at"].as_u64().ok_or("the record has no line")?;
        c.next().await?;
        while (self.lines.len() as u64) <= last {
            let r = self.watch.next().await?;
            self.lines.push(read_line(&r["line"])?);
        }
        Ok(())
    }
}

/// A script being run: its texts until the first step, then the names loaded and the daemon, the stand-in's
/// table, and where the last step's lines begin.
struct Script {
    path: PathBuf,
    texts: Vec<docket_v2::syntax::tree::Module>,
    names: Option<Loaded>,
    d: Option<Daemon>,
    table: Rc<RefCell<Vec<Answer>>>,
    pending: Vec<String>,
    since: usize,
    /// Every `stand-in:` line of the script, read before its first step: what the stand-in carries and performs.
    answers: Vec<String>,
    /// The Effects of the script's `carries:` lines and the Events of its `driver:` lines, read before its first step.
    carries: Vec<String>,
    performs: Vec<String>,
    missed: Rc<RefCell<Vec<String>>>,
    queued: Rc<RefCell<Vec<Instance>>>,
    wake: Option<std::os::unix::net::UnixStream>,
    /// The `Fault`s the last step's runs raised that its results do not state: the next line must.
    faults: Vec<String>,
}

impl Script {
    /// The daemon, booted at the first step on an empty record: the stand-in carries every Effect and Event its
    /// table names, and is linked to the daemon's loop.
    async fn boot(&mut self) -> Result<(), String> {
        if self.d.is_some() {
            return Ok(());
        }
        let l = docket_v2::check::load(std::mem::take(&mut self.texts)).map_err(|e| e.to_string())?;
        let name = l.lets.0.iter().find(|line| written(&line[0]) == "stand-in").map(|line| line[..2].to_vec()).unwrap_or_else(|| vec![Value::text("stand-in"), Value::text("1")]);
        let read: Vec<Answer> = self.pending.drain(..).map(|s| answer(&l, &s)).collect::<Result<_, _>>()?;
        let all: Vec<Answer> = self.answers.iter().map(|s| answer(&l, s)).collect::<Result<_, _>>()?;
        let mut effects: Vec<(String, Vec<String>)> = vec![("Gap".into(), vec![])];
        for c in &self.carries {
            effects.push((qualify(&l, c)?, vec![]));
        }
        for a in &all {
            let op = if a.op == "Gap" { a.op.clone() } else { qualify(&l, &a.op)? };
            let dims: Vec<String> = a.costs.iter().map(|(d, _)| d.clone()).collect();
            match effects.iter_mut().find(|(e, _)| *e == op) {
                Some((_, had)) => dims.into_iter().for_each(|d| if !had.contains(&d) { had.push(d) }),
                None => effects.push((op, dims)),
            }
        }
        let mut events: Vec<String> = all.iter().flat_map(|a| a.events.iter().map(|e| e.ty.clone())).collect();
        events.extend(self.performs.iter().map(|e| instance(&l, e).map(|i| i.ty)).collect::<Result<Vec<_>, _>>()?);
        self.table.borrow_mut().extend(read);
        let (wake, theirs) = std::os::unix::net::UnixStream::pair().map_err(|e| e.to_string())?;
        theirs.set_nonblocking(true).map_err(|e| e.to_string())?;
        self.wake = Some(wake);
        let (driver, waits) = link(StandIn { name, table: self.table.clone(), effects, events, missed: self.missed.clone(), queued: self.queued.clone(), wake: theirs });
        let top = l.modules.last().cloned().unwrap_or_default();
        // every script on a core checking each step against the whole derivation: the gate's own by every binding, the scenarios' guided
        let core = if self.path.parent().is_some_and(|d| d.ends_with(FOLDERS[0])) { docket_v2::norm::Core::checked() } else { docket_v2::norm::Core::checked_guided() };
        let runs = docket_v2::run::machine::Machine::new(l.clone(), core, vec![driver])?;
        let dir = std::env::temp_dir().join(format!("gate-{}-{}", std::process::id(), self.path.file_stem().unwrap().to_string_lossy()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let next = Rc::new(RefCell::new(VecDeque::from(["watch".to_string()])));
        let engine = Engine::open(&dir.join("record.jsonl"), Box::new(Docket::new(runs, &top, vec![], Box::new(|_, _| (None, String::new())), Box::new(|_| Err("a gate record starts empty".into())))), Box::new(Next(next.clone())), Box::new(NoTerminals))?;
        let sock = dir.join("s");
        spawn_local(engine.clone().serve(UnixListener::bind(&sock).map_err(|e| e.to_string())?));
        spawn_local(wait_on(engine, waits, 0));
        let (rd, wr) = UnixStream::connect(&sock).await.map_err(|e| e.to_string())?.into_split();
        let mut watch = Conn(BufReader::new(rd).lines(), wr);
        watch.done(json!({"do": "watch", "from": 0})).await?;
        self.names = Some(l);
        self.d = Some(Daemon { dir, next, sock, conns: BTreeMap::new(), watch, lines: Vec::new() });
        Ok(())
    }

    /// One line of the script.
    async fn step(&mut self, line: &str) -> Result<(), String> {
        if let Some(f) = line.strip_prefix("load ") {
            let t = std::fs::read_to_string(self.path.with_file_name(f.trim())).map_err(|e| e.to_string())?;
            self.texts.push(docket_v2::syntax::read(&t).map_err(|e| e.to_string())?);
            return Ok(());
        }
        if line.starts_with("carries: ") {
            return Ok(());
        }
        if let Some(a) = line.strip_prefix("stand-in: ") {
            match &self.names {
                None => self.pending.push(a.to_string()),
                Some(l) => {
                    let read = answer(l, a)?;
                    self.table.borrow_mut().push(read);
                }
            }
            return Ok(());
        }
        self.boot().await?;
        let (l, d) = (self.names.as_ref().unwrap(), self.d.as_mut().unwrap());
        let check = |ok: bool, what: String| if ok { Ok(()) } else { Err(what) };
        let asked = [("holds ", "Holds", true), ("not ", "Holds", false), ("violated ", "Violated", true), ("unviolated ", "Violated", false)];
        if let Some((f, ask, want)) = asked.iter().find_map(|(p, a, w)| line.strip_prefix(p).map(|f| (f, a, *w))) {
            let (name, args) = call(f)?;
            let name = seen(l, &qualify(l, &name)?)?;
            let args: Vec<String> = args.iter().map(|a| value(a).map(|v| v.to_string())).collect::<Result<_, _>>()?;
            let got = d.conn("gate").await?.done(json!({"do": "query", "query": format!("{ask}({name}({}))", args.join(", "))})).await?;
            return check(got == json!(want), format!("{ask}({f}) is {got}"));
        }
        if let Some((want, t)) = line.strip_prefix("record has ").map(|t| (true, t)).or(line.strip_prefix("record lacks ").map(|t| (false, t))) {
            let found = d.lines[self.since..].iter().any(|l| written(l).contains(t));
            return check(found == want, format!("the last step's lines {} {t}", if want { "lack" } else { "hold" }));
        }
        let (party, rest) = line.split_once(": ").ok_or("not a step")?;
        let mut parts = rest.split(" -> ");
        let act = parts.next().unwrap_or_default();
        let want: Vec<String> = parts.map(|p| p.trim().to_string()).collect();
        if want.is_empty() {
            return Err(format!("{act}: no result stated"));
        }
        let i = instance(l, act)?;
        d.caught_up().await?;
        let start = d.lines.len();
        if party == "driver" {
            // the stand-in performs the Event on its own, woken by a byte; its line is the one with no run
            self.queued.borrow_mut().push(i.clone());
            self.wake.as_ref().unwrap().write_all(b"x").map_err(|e| e.to_string())?;
            let ours = |l: &Line| matches!(l, Line::Performed { act, run: None, .. } | Line::Refused { act, run: None, .. } if *act == i);
            while !d.lines[start..].iter().any(ours) {
                let r = d.watch.next().await?;
                d.lines.push(read_line(&r["line"])?);
            }
            d.caught_up().await?;
            let got: Vec<String> = d.lines[start..].iter().filter(|l| ours(l)).flat_map(|l| outcomes(std::slice::from_ref(&with_run(l)), "", &i)).collect();
            self.since = start;
            self.faults = faults(&d.lines[start..], &want);
            self.missing(&want)?;
            check(got == want, format!("{act} gave {got:?}, not {want:?}"))?;
            return self.unused();
        }
        let args: Vec<Json> = i.args.iter().map(of_value).collect();
        let done = d.conn(party.trim()).await?.done(json!({"do": "act", "name": i.ty, "args": args})).await?;
        let run = done["run"].as_str().ok_or(format!("no run in {done}"))?.to_string();
        d.caught_up().await?;
        let got = outcomes(&d.lines[start..], &run, &i);
        let raised: Vec<String> = d.lines[start..].iter().filter(|l| matches!(l, Line::Raised { .. })).map(|l| written(l)).collect();
        self.since = start;
        self.faults = faults(&d.lines[start..], &want);
        self.missing(&want)?;
        check(got == want, format!("{act} gave {got:?}, not {want:?}; {}", raised.join("; ")))?;
        self.unused()
    }

    /// An error naming each Effect the stand-in carried with no answer left for it, in whichever run, unless the
    /// step states the `Fault` among its results: it never answers by itself.
    fn missing(&mut self, want: &[String]) -> Result<(), String> {
        let missed: Vec<String> = self.missed.borrow_mut().drain(..).collect();
        if missed.is_empty() || want.iter().any(|w| w == "raised Fault") { Ok(()) } else { Err(format!("the stand-in had no answer for {}", missed.join(", "))) }
    }

    /// An error naming each answer of the table no Effect took, which it then forgets.
    fn unused(&mut self) -> Result<(), String> {
        let left: Vec<String> = self.table.borrow_mut().drain(..).map(|a| a.text).chain(self.pending.drain(..)).collect();
        if left.is_empty() { Ok(()) } else { Err(format!("answers no Effect took: {}", left.join(" | "))) }
    }
}

/// Declared name `q` as the module queries are read in sees it: its own by its last part, an import's through the import.
fn seen(l: &Loaded, q: &str) -> Result<String, String> {
    let top = l.modules.last().cloned().unwrap_or_default();
    let (module, last) = q.rsplit_once('.').ok_or(format!("{q} is not qualified"))?;
    if module == top {
        return Ok(last.to_string());
    }
    let i = l.imports.get(&top).into_iter().flatten().find(|i| i.path.to_string() == module).ok_or(format!("{top}, where queries are read, does not import {module}"))?;
    Ok(format!("{}.{last}", i.alias.clone().unwrap_or_else(|| module.to_string())))
}

/// An Event's line, which has no run, as if of run "", so that `outcomes` reads it.
fn with_run(l: &Line) -> Line {
    match l.clone() {
        Line::Performed { act, party, .. } => Line::Performed { act, party, run: Some(String::new()) },
        Line::Refused { act, party, why, .. } => Line::Refused { act, party, why, run: Some(String::new()) },
        other => other,
    }
}

/// Runs the script in `path` on a daemon of its own; each line that does not give its result, by its place.
async fn run_script(path: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(path).unwrap();
    let answers = text.lines().filter_map(|l| l.strip_prefix("stand-in: ")).map(str::to_string).collect();
    let carries = text.lines().filter_map(|l| l.strip_prefix("carries: ")).flat_map(fields).collect();
    let performs = text.lines().filter_map(|l| l.strip_prefix("driver: ")).map(|l| l.split(" -> ").next().unwrap_or_default().to_string()).collect();
    let mut s = Script { path: path.to_path_buf(), texts: Vec::new(), names: None, d: None, table: Rc::default(), pending: Vec::new(), since: 0, answers, carries, performs, missed: Rc::default(), queued: Rc::default(), wake: None, faults: Vec::new() };
    let file = path.file_name().unwrap().to_string_lossy().to_string();
    let mut wrong = Vec::new();
    for (k, line) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty() && !l.starts_with('#')) {
        let booted = s.d.is_some();
        if let Err(e) = stated(&std::mem::take(&mut s.faults), line) {
            wrong.push(format!("{file}:{}: {e}", k + 1));
        }
        if let Err(e) = s.step(line).await {
            wrong.push(format!("{file}:{}: {e}", k + 1));
            // a text that did not load leaves nothing to ask
            if !booted && s.d.is_none() && s.names.is_none() && s.texts.is_empty() {
                return wrong;
            }
        }
    }
    for e in [s.unused(), stated(&s.faults, "")].into_iter().filter_map(Result::err) {
        wrong.push(format!("{file}: at its end: {e}"));
    }
    if let Some(d) = s.d {
        let _ = std::fs::remove_dir_all(d.dir);
    }
    wrong
}

#[tokio::test]
async fn every_script_gives_its_stated_result_at_every_step_on_the_daemon() {
    let mut scripts = Vec::new();
    for f in FOLDERS {
        // a folder not on this branch has no scripts
        scripts.extend(std::fs::read_dir(design().join(f)).into_iter().flatten().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "steps")));
    }
    scripts.sort();
    assert!(!scripts.is_empty());
    let mut wrong = Vec::new();
    for s in &scripts {
        wrong.extend(LocalSet::new().run_until(run_script(s)).await);
    }
    assert!(wrong.is_empty(), "{} lines wrong:\n{}", wrong.len(), wrong.join("\n"));
}
