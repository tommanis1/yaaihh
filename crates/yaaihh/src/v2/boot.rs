//! The window on the real engine (coordd-v2 with the stdlib and yaaihh package) over its socket, drawing the record's thread.

// Made up: the parties, their words, the stand-in's answers; vouching goes by connection order (one cgroup in a test).

use super::desk::Desk;
use coordd_v2::docket::Docket;
use coordd_v2::hand::Hand;
use coordd_v2::peer::{Party, Vouch};
use coordd_v2::serve::{Engine, Terminals};
use coordd_v2_link::{Answer, Incoming, Link};
use docket_v2::check::names::Sym;
use serde_json::{json, Value as Json};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc};

/// The longest any one step waits for the engine before the test fails.
const STEP: Duration = Duration::from_secs(20);

/// The package's texts as a scenario loads them (docket/scenarios).
pub(crate) const TEXTS: [&str; 18] = [
    "std/hooks.docket", "std/bridges/claude-code.docket", "std/bridges/codex.docket", "std/bridges/opencode.docket",
    "std/bridges/agy.docket", "std/delivery.docket", "std/comms.docket", "std/rails.docket", "std/budgets.docket", "std/control.docket", "std/adapt.docket",
    "defaults/audit.docket", "defaults/train.docket", "defaults/hygiene.docket", "defaults/brain.docket", "defaults/desk.docket", "defaults/project.docket", "defaults/v1.docket",
];

/// The package's names the stand-in and the parties use (the window reads them from the threads file).
pub(crate) const SEATED: &str = "std.comms.Owner-Seated";
pub(crate) const OPEN: &str = "yaaihh.desk.Open-Thread";
const SAYS: &str = "std.comms.Owner-Says";
const HIS_POST: &str = "yaaihh.desk.Owner-Posts";
const TO_OWNER: &str = "std.comms.Says-To-Owner";
const POST_ON: &str = "yaaihh.desk.Post-On";
const TEXT_OF: &str = "std.comms.TextOf";
const WRITTEN: &str = "yaaihh.desk.Written";

/// The made-up clock: his post written at the first second, the engine's `Now()` a minute later.
const HIS_AT: i64 = 1_795_000_000;

const HIS: &str = "Keep the outline small: two levels at most, for now.";
const REPLY: &str = "Done: the outline has two levels, and nothing deeper is drawn.";

/// Each connection in turn as the next party; once they are used up, the last again.
struct InTurn(Mutex<VecDeque<Party>>);

impl Vouch for InTurn {
    fn party(&self, _: &str) -> Option<Party> {
        let mut q = self.0.lock().unwrap();
        if q.len() > 1 { q.pop_front() } else { q.front().cloned() }
    }
}

struct NoTerminals;

impl Terminals for NoTerminals {
    fn open(&self, _: &Party, s: &str) -> Result<(mpsc::UnboundedSender<Vec<u8>>, broadcast::Receiver<Vec<u8>>), String> {
        Err(format!("no terminal {s}"))
    }
}

/// The package's folder, here.
pub(crate) fn root() -> PathBuf {
    // set at run time too, so a test binary built on the build machine reads this folder's texts
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").into());
    Path::new(&crate_dir).join("../../docket")
}

pub(crate) fn texts() -> Vec<PathBuf> {
    TEXTS.iter().map(|t| root().join(t)).collect()
}

/// The engine on its own thread, serving a socket: one stand-in driver over it carries every driver's Effect and Event.
pub(crate) fn boot(dir: &Path) -> PathBuf {
    let socket = dir.join("engine.sock");
    let (record, at) = (dir.join("record.jsonl"), socket.clone());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let modules = texts().iter().map(|p| docket_v2::syntax::read(&std::fs::read_to_string(p).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).collect();
        let loaded = docket_v2::check::load(modules).unwrap();
        let effects = loaded.syms.iter().filter(|(_, s)| matches!(s, Sym::Effect { does: None, .. })).map(|(n, _)| n.clone()).collect();
        let events = [SEATED, SAYS, HIS_POST, WRITTEN, "std.comms.Spoke"].map(str::to_string).to_vec();
        let hand = Hand { name: "stand-in".into(), effects, events };
        let mut runs = docket_v2::run::machine::Machine::new(loaded, docket_v2::norm::Core::default(), vec![Box::new(hand)]).unwrap();
        runs.clock = Box::new(|| HIS_AT + 60);
        let parties = InTurn(Mutex::new(["o1", "w1", "o1"].map(|p| Party::of(p, Some(0))).into()));
        let engine = Engine::open(&record, Box::new(Docket::new(runs, "yaaihh.project", vec![0], Box::new(|_, _| (None, String::new())), Box::new(|_| Err("this record starts empty".into())))), Box::new(parties), Box::new(NoTerminals)).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        tokio::task::LocalSet::new().block_on(&rt, async move {
            let listener = tokio::net::UnixListener::bind(&at).unwrap();
            tx.send(()).unwrap();
            let _ = engine.serve(listener).await;
        });
    });
    rx.recv_timeout(STEP).expect("the engine loads the package and listens");
    socket
}

/// What a party sees: its link, and the record's lines it has watched so far.
pub(crate) struct Seen {
    link: Link,
    lines: Vec<(u64, Json)>,
}

impl Seen {
    pub(crate) fn open(socket: &Path) -> Seen {
        let link = crate::engine_v2::start(crate::engine_v2::Reach::Socket(socket.into()), std::sync::Arc::new(|| {}));
        link.watch(0);
        let mut s = Seen { link, lines: Vec::new() };
        s.until(|c| matches!(c, Incoming::Up { .. }).then_some(())).expect("the link comes up");
        s
    }

    /// The first thing to come that `f` takes; the lines that come meanwhile are kept.
    fn until<T>(&mut self, mut f: impl FnMut(&Incoming) -> Option<T>) -> Option<T> {
        let end = Instant::now() + STEP;
        while let Some(c) = self.link.next(end.saturating_duration_since(Instant::now())) {
            if let Incoming::Line { at, line } = &c {
                self.lines.push((*at, line.clone()));
            }
            if let Some(t) = f(&c) {
                return Some(t);
            }
        }
        None
    }

    /// Act or Event `name` with `args`; what the engine gave for it.
    pub(crate) fn act(&mut self, name: &str, args: Json) -> Json {
        let id = self.link.act(name, args.as_array().unwrap().clone()).expect("sent");
        let got = self.until(|c| match c {
            Incoming::Done { id: d, value } if *d == id => Some(Ok(value.clone())),
            Incoming::No { id: Some(d), why } if *d == id => Some(Err(why.clone())),
            _ => None,
        });
        got.unwrap_or_else(|| panic!("{name}: no reply")).unwrap_or_else(|why| panic!("{name}: {why}"))
    }

    /// The next `effect` line asking `op` answered with `value`; an Effect asked meanwhile is answered as failed.
    fn answer(&mut self, op: &str, value: Json) {
        let from = self.lines.len();
        let mut seen = from;
        loop {
            let asked = self.lines[seen..].iter().find_map(|(at, l)| Some((*at, l.get("effect")?.get("op")?.as_str()?.to_string())));
            seen = self.lines.len();
            match asked {
                Some((at, o)) if o == op => {
                    self.link.answer(at, Answer::Value { value, costs: vec![] }).expect("sent");
                    return;
                }
                Some((at, o)) => drop(self.link.answer(at, Answer::Failed { failed: format!("the stand-in has no answer for {o}") })),
                None => assert!(self.until(|c| matches!(c, Incoming::Line { .. }).then_some(())).is_some(), "no {op} was asked"),
            }
        }
    }
}

/// His lines and the agent's acts on the engine; its record is the thread the window draws, words and all.
#[test]
fn the_window_boots_on_coordd_v2_and_draws_the_thread_its_record_makes() {
    let dir = std::env::temp_dir().join(format!("yaaihh-v2-boot-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let socket = boot(&dir);
    let mut o = Seen::open(&socket);
    assert_eq!(o.act(SEATED, json!(["o1"])), json!({"event": "performed"}));
    o.act("std.comms.Seat", json!(["o1", "w1", "front"]));
    o.act(OPEN, json!(["o1", "t1", "The outline's depth"]));
    assert_eq!(o.act(SAYS, json!(["h1", "w1"])), json!({"event": "performed"}));
    o.answer(TEXT_OF, json!(HIS));
    o.answer("std.control.Deliver", json!(""));
    o.act(HIS_POST, json!(["h1", "t1"]));
    // his window's own line: it knows when he wrote
    assert_eq!(o.act(WRITTEN, json!(["h1", HIS_AT])), json!({"event": "performed"}));
    let mut w = Seen::open(&socket);
    assert_eq!(w.act("std.comms.Spoke", json!(["w1", "n1"])), json!({"event": "performed"}));
    w.act("std.hooks.Reach", json!(["w1", "turn_end"]));
    o.answer("std.comms.Pick", json!("cl1"));
    o.answer(TEXT_OF, json!(REPLY));
    w.act(TO_OWNER, json!(["w1", "n1"]));
    w.act(POST_ON, json!(["w1", "n1", "t1"]));
    let refused: Vec<&Json> = w.lines.iter().map(|(_, l)| l).filter(|l| l.get("refused").is_some()).collect();
    assert!(refused.is_empty(), "every act and Event of the thread is performed: {refused:?}");

    // the window: the projects file names the engine; the scene reads its record once and opens the thread
    drop((o, w));
    let (held, _) = crate::view_cost::test_app();
    let config = dir.join("projects.toml");
    let texts: Vec<String> = texts().iter().map(|p| format!("{:?}", p.display().to_string())).collect();
    let threads = root().join("defaults/threads.toml");
    std::fs::write(&config, format!("[v2]\nname = \"yaaihh\"\nsocket = {:?}\ntexts = [{}]\nthreads = {:?}\n", socket.display().to_string(), texts.join(", "), threads.display().to_string())).unwrap();
    // SAFETY: under the tests' lock, held
    unsafe { std::env::set_var("YAAIHH_V2_CONFIG", &config) };
    drop(held);
    let s = drawn();
    assert_eq!(s.notice, None, "the window reads the projects file that names the engine");
    let p = &s.projects[s.cur];
    let t = &p.live.tickets[&1];
    let posts: Vec<(&str, Json)> = t.posts.iter().map(|p| (p.by.as_str(), p.text.value())).collect();
    assert_eq!((t.title.as_str(), posts), ("The outline's depth", vec![("owner", json!(HIS)), ("w1", json!(REPLY))]));
    assert_eq!(t.posts[1].told, ["owner"], "the reply is said to him, by the agent's own act");
    let w = crate::whereabouts::of(p, t, &t.posts[0]).expect("his post");
    assert_eq!(w.entries[0].is, crate::whereabouts::Is::Answered { post: 2 }, "handed to w1 by the package's own line, and answered");
    let times: Vec<&str> = t.posts.iter().map(|p| p.at.as_str()).collect();
    assert_eq!(times, ["2026-11-18T11:06:40Z", "2026-11-18T11:07:40Z"], "his from his window's line, the reply's from the engine's Now()");
    let v = s.ext.get::<super::V2>();
    assert!(v.desk.len() > 10 && v.loaded.is_some() && matches!(v.map, Some(Ok(_))));
    assert_eq!(Desk::default().len(), 0);

    if let Ok(out) = std::env::var("STILL_OUT") {
        let out = PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        let args = ["--snap", &out.join("v2-thread.png").display().to_string(), "--size", "1400x900", "--at", "1.0", "--tree", &out.join("v2-thread.json").display().to_string()].map(str::to_string).to_vec();
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), drawn, crate::view) }).args(args).try_run().unwrap();
    }
}

/// The window as the scene leaves it: the engine's project, its first thread open.
fn drawn() -> crate::App {
    let (_held, _) = crate::view_cost::test_app();
    // SAFETY: under the tests' lock, held
    unsafe { std::env::set_var("YAAIHH_CONFIG", std::env::var_os("YAAIHH_V2_CONFIG").unwrap()) };
    // the window as it starts with that projects file
    let mut s = crate::init();
    let began = Instant::now();
    super::scene(&mut s);
    eprintln!("the scene: texts loaded, threads file checked, record read in {} ms", began.elapsed().as_millis());
    s
}
