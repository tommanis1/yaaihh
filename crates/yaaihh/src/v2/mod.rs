//! The window on a coordd-v2 engine: one project whose threads are the engine's record (desk.rs), kept live by the link.

// The core's thread pages draw the engine's project as any other; a page asks its facts through `query`.

pub mod asked;
#[cfg(all(test, unix))]
pub(crate) mod boot;
pub mod desk;
mod lines;
pub mod map;
pub mod reader;
pub mod seat;
#[cfg(all(test, unix))]
mod seated;

use crate::engine_v2::{self, Reach};
use crate::ext::Extension;
use crate::App;
use coordd_v2_link::{Incoming, Link};
use desk::Desk;
use docket_v2::check::Loaded;
use docket_v2::syntax::tree::Module;
use map::Map;
use rune::{col, text, El, Tx};
use rune_kit::Kit;
use serde::Deserialize;
use serde_json::Value as Json;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

pub const NAME: &str = "v2";

/// The longest a scene waits for the engine's record before it draws what it has.
const SCENE_WAIT: Duration = Duration::from_secs(20);

/// projects.toml's `[v2]`: the project's name, its engine's socket (or a command), the texts it loaded, their threads file,
/// the module its queries are read in, the window's party and the window driver's socket (or a command).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Config {
    pub name: String,
    pub socket: Option<String>,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub texts: Vec<String>,
    pub threads: Option<String>,
    /// The module the engine reads its queries in (the daemon's `--top`).
    #[serde(default)]
    pub top: String,
    /// The party the engine knows the window's link for: the actor of the acts the window sends.
    #[serde(default)]
    pub party: String,
    /// The command that runs the machine's reader (`yaaihh-machine-state`) for Home and Overview.
    #[serde(default)]
    pub reader: Vec<String>,
    /// The project's repositories by name, each its folder, as Overview lists them.
    #[serde(default)]
    pub repos: std::collections::BTreeMap<String, String>,
    /// The window driver's socket (coordd-v2's `--window` file names it): his seat, his words and his decisions.
    pub window: Option<String>,
    #[serde(default)]
    pub window_command: Vec<String>,
}

#[derive(Deserialize)]
struct File {
    v2: Option<Config>,
}

/// A query asked: out (with the answer it had before), answered, or answered before the last line came.
enum Asked<T> {
    Out(u64, Option<T>),
    Got(T),
    Old(T),
}

/// The engine's project: its config, link and record, where it is among the projects, and the queries asked.
#[derive(Default)]
pub struct V2 {
    pub config: Option<Config>,
    link: Option<Link>,
    pub desk: Desk,
    pub at: Option<usize>,
    pub up: bool,
    pub down: Option<String>,
    /// The engine's word when it refused the watch.
    pub refused: Option<String>,
    /// The texts as read: the Policy page reads its rows from their clauses.
    pub modules: Vec<Module>,
    /// The loaded texts and the threads file read against them, or what is wrong.
    pub loaded: Option<Loaded>,
    pub map: Option<Result<Map, Vec<String>>>,
    asked: Answers,
    /// The answers to `holding` and `explain` (programs/), by what was asked.
    held: Answers<Result<Json, String>>,
    /// The acts sent and not yet answered, by request id.
    acts: HashMap<u64, String>,
    /// The last act the engine would not take at all, and why (a refusal by its policy is a line of the record).
    pub no: Option<String>,
    /// The machine's reader, where the projects file names one.
    reader: Option<reader::Reader>,
    /// His seat at the window driver, where the projects file names one.
    pub seat: Option<seat::Seat>,
    /// What he typed in each field of the questions that wait for him (asked.rs), by line and field.
    pub edits: HashMap<(u64, &'static str), rune::TextEdit>,
}

/// The answers to the queries asked, each kept until its next answer comes.
struct Answers<T = Result<bool, String>>(RefCell<HashMap<String, Asked<T>>>);

impl<T> Default for Answers<T> {
    fn default() -> Self {
        Answers(RefCell::default())
    }
}

impl<T: Clone> Answers<T> {
    /// The last answer to `q`; asked by `send` when none is out and none is current.
    fn want(&self, q: &str, send: impl FnOnce() -> Option<u64>) -> Option<T> {
        let mut asked = self.0.borrow_mut();
        let last = match asked.get(q) {
            Some(Asked::Got(a)) => return Some(a.clone()),
            Some(Asked::Out(_, last)) => return last.clone(),
            Some(Asked::Old(a)) => Some(a.clone()),
            None => None,
        };
        if let Some(id) = send() {
            asked.insert(q.to_string(), Asked::Out(id, last.clone()));
        }
        last
    }

    /// Whether request `id` is one of these.
    fn asks(&self, id: u64) -> bool {
        self.0.borrow().values().any(|x| matches!(x, Asked::Out(o, _) if *o == id))
    }

    /// Request `id`'s answer came.
    fn got(&self, id: u64, a: T) {
        for x in self.0.borrow_mut().values_mut() {
            if matches!(x, Asked::Out(o, _) if *o == id) {
                *x = Asked::Got(a.clone());
            }
        }
    }

    /// A line came: every answer is asked again at its next want.
    fn moved(&self) {
        for a in self.0.borrow_mut().values_mut() {
            if let Asked::Got(x) = a {
                *a = Asked::Old(x.clone());
            }
        }
    }
}

/// The `[v2]` of the projects file, if it has one.
pub fn config() -> Option<Config> {
    std::fs::read_to_string(crate::projects::config_path()).ok().and_then(|t| toml::from_str::<File>(&t).ok()).and_then(|f| f.v2)
}

/// The engine's pages, its project and its Policy page, where the projects file names an engine; none otherwise.
pub fn extensions() -> Vec<Extension> {
    if config().is_none() {
        return Vec::new();
    }
    vec![extension(), crate::policy_v2::extension()]
}

pub fn extension() -> Extension {
    Extension {
        name: NAME,
        init,
        setup,
        poll,
        rows: |_, _, _| Vec::new(),
        nav_key: |s| s.ext.try_get::<V2>().map_or(String::new(), |v| format!("{}{}", v.up, v.desk.rev)),
        page,
        title: |_| "Engine".into(),
        reply: |_, _, _, _| {},
        prices: None,
        scenes: &[("v2-thread", scene)],
        replaces: &[],
        window_pages: &[],
        accounts: None,
    }
}

/// Where the engine's project is among the projects, once it is added.
pub fn at(s: &App) -> Option<usize> {
    s.ext.try_get::<V2>()?.at
}

/// Fact `q` of the engine: its last answer, asked again after each new line; None until the first comes.
pub fn query(s: &App, q: &str) -> Option<Result<bool, String>> {
    let v = s.ext.try_get::<V2>()?;
    v.asked.want(q, || v.link.as_ref().and_then(|l| l.query(q)))
}

/// Every instance of Fact or Duty `name`, a full name, that holds now: its last answer, asked again after each line
/// that moves the facts.
pub fn holding(s: &App, name: &str) -> Option<Result<Json, String>> {
    let v = s.ext.try_get::<V2>()?;
    v.held.want(&format!("holding {name}"), || v.link.as_ref().and_then(|l| l.holding(name)))
}

/// Why instance `inst`, written in the top module's names, holds or not, or is not enabled (coordd-v2's why.rs).
pub fn explain(s: &App, inst: &str) -> Option<Result<Json, String>> {
    let v = s.ext.try_get::<V2>()?;
    v.held.want(&format!("explain {inst}"), || v.link.as_ref().and_then(|l| l.explain(inst)))
}

/// The machine reader's last answer to `q` (`{"repos": …}`, `{"machines": {}}`, `{"store": …}`); None until it comes,
/// or where the projects file names no reader.
pub fn machine(s: &App, q: &Json) -> Option<Json> {
    s.ext.try_get::<V2>()?.reader.as_ref()?.want(q)
}

/// Every answer of the machine's reader asked again at its next want; the last stays until the new one comes.
pub fn machine_again(s: &App) {
    if let Some(r) = s.ext.try_get::<V2>().and_then(|v| v.reader.as_ref()) {
        r.again();
    }
}

/// Why the machine's reader is not answering, if it is not.
pub fn machine_down(s: &App) -> Option<String> {
    s.ext.try_get::<V2>()?.reader.as_ref()?.down()
}

/// Perform act `name` with `args` as the window's party.
pub fn act(s: &mut App, name: &str, args: Vec<Json>) {
    let Some(v) = s.ext.try_get_mut::<V2>() else { return };
    v.no = None;
    match v.link.as_ref().and_then(|l| l.act(name, args)) {
        Some(id) => drop(v.acts.insert(id, name.to_string())),
        None => v.no = Some(format!("{name}: not sent, the engine is not connected")),
    }
}

/// The texts and threads file of `c`: the texts as read and loaded, and the file checked against them.
pub fn plan(c: &Config) -> (Vec<Module>, Option<Loaded>, Result<Map, Vec<String>>) {
    let read = |p: &String| std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"));
    let modules = c.texts.iter().map(|p| read(p).and_then(|t| docket_v2::syntax::read(&t).map_err(|e| format!("{p}: {e}")))).collect::<Result<Vec<_>, _>>();
    let (modules, loaded) = match modules.and_then(|m| docket_v2::check::load(m.clone()).map(|l| (m, l)).map_err(|e| e.to_string())) {
        Ok(ml) => ml,
        Err(e) => return (Vec::new(), None, Err(vec![e])),
    };
    let map = match &c.threads {
        None => Err(vec!["No threads file in [v2]".to_string()]),
        Some(p) => read(p).map_err(|e| vec![e]).and_then(|t| Map::read(&t, &loaded).map_err(|f| f.into_iter().map(|f| format!("{p}: {f}")).collect())),
    };
    (modules, Some(loaded), map)
}

/// The texts the engine loaded, as the projects file names them.
pub fn texts(s: &App) -> &[String] {
    s.ext.try_get::<V2>().and_then(|v| v.config.as_ref()).map_or(&[], |c| c.texts.as_slice())
}

fn init(s: &mut App) {
    if s.ext.try_get::<V2>().is_none() {
        s.ext.insert(V2 { config: config(), ..V2::default() });
    }
}

fn reach(c: &Config) -> Option<Reach> {
    Reach::of(c.socket.as_deref(), &c.command)
}

/// The engine's project, added after the others: the core neither brings it up nor opens a coordd's pipe for it.
fn add_project(s: &mut App, name: &str) -> usize {
    let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: name.into(), id: name.into(), boxed: false }, Vec::new());
    p.down = None;
    s.projects.push(p);
    s.projects.len() - 1
}

fn setup(s: &mut App, wake: rune::Waker) {
    let Some(c) = s.ext.try_get::<V2>().and_then(|v| v.config.clone()) else { return };
    if !c.reader.is_empty() {
        s.ext.get_mut::<V2>().reader = Some(reader::Reader::new(c.reader.clone(), wake.clone()));
    }
    let Some(r) = reach(&c) else { return };
    let i = add_project(s, &c.name);
    let (modules, loaded, map) = plan(&c);
    let seat = Reach::of(c.window.as_deref(), &c.window_command).map(|w| seat::Seat::new(w.connect(), wake.clone()));
    let link = engine_v2::start(r, wake);
    link.watch(0);
    let v = s.ext.get_mut::<V2>();
    (v.link, v.at, v.modules, v.loaded, v.map, v.seat) = (Some(link), Some(i), modules, loaded, Some(map), seat);
}

/// The engine's threads into the project's: its thread pages read them again.
fn show(s: &mut App, i: usize, tickets: BTreeMap<u64, coord_types::TicketView>) {
    let live = &mut s.projects[i].live;
    live.tickets = tickets;
    live.tickets_rev += 1;
    live.seq += 1;
}

fn poll(s: &mut App, _: &mut Tx) -> bool {
    let seated = seat::drain(s);
    let Some(v) = s.ext.try_get_mut::<V2>() else { return false };
    let read = v.reader.as_ref().is_some_and(reader::Reader::drain) || seated;
    let came = v.link.as_ref().map(Link::drain).unwrap_or_default();
    if came.is_empty() {
        return read;
    }
    let (mut lines, mut moved) = (false, false);
    for c in came {
        let got = |id: u64, a: Result<bool, String>, v: &V2| v.asked.got(id, a);
        match c {
            Incoming::Line { at, line } => {
                let facts = moves_facts(&line);
                if v.desk.take(at, line) {
                    (lines, moved) = (true, moved || facts);
                }
            }
            Incoming::Done { id, .. } if v.acts.remove(&id).is_some() => {}
            Incoming::No { id: Some(id), why } | Incoming::Lost { id, why } if v.acts.contains_key(&id) => v.no = v.acts.remove(&id).map(|a| format!("{a}: {why}")),
            Incoming::Done { id, value } if v.held.asks(id) => v.held.got(id, Ok(value)),
            Incoming::No { id: Some(id), why } | Incoming::Lost { id, why } if v.held.asks(id) => v.held.got(id, Err(why)),
            Incoming::Done { id, value } => got(id, value.as_bool().ok_or(format!("not a yes or no: {value}")), v),
            Incoming::No { id: Some(id), why } | Incoming::Lost { id, why } => got(id, Err(why), v),
            // the engine is up again, and with it the window driver: he is seated again
            Incoming::Up { .. } => {
                (v.up, v.down, v.refused) = (true, None, None);
                if let Some(seat) = v.seat.as_mut() {
                    seat.sit();
                }
            }
            Incoming::Down(why) => (v.up, v.down) = (false, Some(why)),
            Incoming::No { id: None, why } => v.refused = Some(why),
            // the window subscribes to nothing, so no delta comes to it
            Incoming::Bytes { .. } | Incoming::Delta(_) => {}
        }
    }
    if moved {
        v.asked.moved();
        v.held.moved();
    }
    if lines {
        if let (Some(i), Some(Ok(m))) = (v.at, &v.map) {
            let t = v.desk.tickets(m);
            show(s, i, t);
        }
    }
    true
}

/// Whether a record line can change the facts: the engine changes them only as it performs an act or Event, or loads a text.
fn moves_facts(line: &Json) -> bool {
    line.get("performed").is_some() || line.get("loaded").is_some()
}

fn page(s: &App, k: &Kit, _: &str) -> (String, El) {
    let line = |t: String| text(t).font(k.ty.small).color(k.c.text_muted);
    let Some(v) = s.ext.try_get::<V2>() else { return ("Engine".into(), col()) };
    let at = v.config.as_ref().map_or(String::new(), |c| c.socket.clone().unwrap_or_else(|| c.command.join(" ")));
    let state = match (&v.refused, &v.down, v.up) {
        (Some(why), _, _) => format!("{at} refused to show its record: {why}"),
        (None, _, true) => format!("Connected to {at}"),
        (None, Some(why), false) => format!("Not connected to {at}: {why}"),
        (None, None, false) => format!("Connecting to {at}"),
    };
    let warn = |t: String| text(t).font(k.ty.small).color(k.c.warning.fg).wrap();
    let faults = match &v.map {
        Some(Err(f)) => f.clone(),
        _ => Vec::new(),
    };
    let c = col().w_full().gap(10.0).padding(24.0).child(line(state)).child(line(format!("{} lines of the record", v.desk.len())));
    let c = c.children(faults.into_iter().map(warn)).child(asked::el(s, k));
    ("Engine".into(), col().key("v2-page").w_full().h_full().scroll(rune::ScrollSpec { stick_bottom: false }).child(c))
}

/// A still of the engine's project (`--scene v2-thread`): its record read once, its first thread open.
fn scene(s: &mut App) {
    init(s);
    let Some(c) = s.ext.get::<V2>().config.clone() else { return };
    let Some(r) = reach(&c) else { return };
    let (modules, loaded, map) = plan(&c);
    let link = engine_v2::start(r, std::sync::Arc::new(|| {}));
    let (mut desk, end) = (Desk::default(), Instant::now() + SCENE_WAIT);
    // up, then every line by `older` from the first, then its `done`
    let (mut asked, mut whole) = (None, false);
    while let Some(c) = link.next(end.saturating_duration_since(Instant::now())) {
        match c {
            Incoming::Up { .. } => asked = link.older(u64::MAX, u64::MAX),
            Incoming::Line { at, line } => drop(desk.take(at, line)),
            Incoming::Done { id, .. } if Some(id) == asked => {
                whole = true;
                break;
            }
            _ => {}
        }
    }
    if !whole {
        s.notice = Some(format!("Partial record ({} s timeout)", SCENE_WAIT.as_secs()));
    }
    let i = add_project(s, &c.name);
    let t = map.as_ref().map(|m| desk.tickets(m)).unwrap_or_default();
    let first = t.keys().next().copied();
    show(s, i, t);
    (s.cur, s.sel, s.tickets.shown) = (i, crate::Sel::Tickets, first);
    let v = s.ext.get_mut::<V2>();
    (v.desk, v.at, v.up, v.modules, v.loaded, v.map) = (desk, Some(i), whole, modules, loaded, Some(map));
}

#[cfg(test)]
mod tests {
    use super::{moves_facts, Answers};
    use serde_json::json;

    /// The answers are asked again after a line that performs or loads, and not after one that only runs, asks or refuses.
    #[test]
    fn only_a_performed_or_loaded_line_moves_the_facts() {
        let act = json!({"act": {"inst": "p.Set", "args": ["o1"]}, "party": "o1", "run": "r1"});
        assert!(moves_facts(&json!({"performed": act})) && moves_facts(&json!({"loaded": "ab"})));
        let others = [
            json!({"refused": {"act": {"inst": "p.Set", "args": ["w1"]}, "party": "w1", "run": "r2", "why": "source"}}),
            json!({"began": {"run": "r1", "by": "o1"}}),
            json!({"effect": {"op": "p.Words", "args": ["n1"], "run": "r1", "head": "", "key": ""}}),
            json!({"answered": {"line": 3, "value": "x"}}),
            json!({"charged": {"run": "r1", "dim": "tokens", "n": 9}}),
            json!({"ended": {"run": "r1", "value": null}}),
        ];
        assert!(others.iter().all(|l| !moves_facts(l)));
    }

    /// An answer stays drawn while it is asked again after a line, until the new one comes.
    #[test]
    fn an_answer_is_kept_until_its_next_one_comes() {
        let a: Answers = Answers::default();
        assert_eq!(a.want("q", || Some(1)), None, "asked, nothing yet");
        assert_eq!(a.want("q", || panic!("asked twice")), None);
        a.got(1, Ok(true));
        assert_eq!(a.want("q", || panic!("answered already")), Some(Ok(true)));
        a.moved();
        assert_eq!(a.want("q", || Some(2)), Some(Ok(true)), "asked again, the old answer drawn meanwhile");
        assert_eq!(a.want("q", || panic!("out already")), Some(Ok(true)));
        a.got(2, Ok(false));
        assert_eq!(a.want("q", || panic!("answered already")), Some(Ok(false)));
    }
}

/// The v2 project at AT with a stand-in reader run as CMD and REPOS named, for a page's test.
#[cfg(test)]
pub(crate) fn with_reader(s: &mut App, at: usize, cmd: Vec<String>, repos: BTreeMap<String, String>) {
    let config = Config { reader: cmd.clone(), repos, ..Config::default() };
    s.ext.insert(V2 { config: Some(config), at: Some(at), reader: Some(reader::Reader::new(cmd, std::sync::Arc::new(|| {}))), ..V2::default() });
}

/// The stand-in reader's answers taken in, as the window's loop takes them.
#[cfg(test)]
pub(crate) fn drain_reader(s: &App) -> bool {
    s.ext.try_get::<V2>().and_then(|v| v.reader.as_ref()).is_some_and(reader::Reader::drain)
}
