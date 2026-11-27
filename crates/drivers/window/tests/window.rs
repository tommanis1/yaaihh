//! His window on the driver's socket: only his connection is kept, a post keeps his words and gives his Events, a
//! decision answers only a line the driver holds, and on the real package, read with yaaihh's file, each fits.

use docket_driver_window::{Config, Window, APPROVAL, BUDGET, SAYS, SEATED};
use docket_v2::check::names::Sym;
use docket_v2::norm::Core;
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::machine::{Machine, State};
use docket_v2::run::record::Line;
use docket_v2::run::value::{Instance, Value};
use docket_v2::syntax::tree::Norm;
use serde_json::{json, Value as Json};
use std::cell::{Cell, RefCell};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Made up: his party, an agent's, a thread, and when he wrote.
const OWNER: &str = "o1";
const AGENT: &str = "w1";
const AT: i64 = 1_795_000_000;

/// yaaihh's Events, as its defaults file names them.
const POSTS: &str = "yaaihh.desk.Owner-Posts";
const WRITTEN: &str = "yaaihh.desk.Written";

/// The package's folder.
fn docket() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../docket")
}

/// A folder of its own for test `name`.
fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("window-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The driver in `d`, vouching for its connections as `parties` say, in turn.
fn window(d: &Path, parties: &'static [&'static str]) -> Window {
    let next = Rc::new(Cell::new(0));
    let whose = Box::new(move |_: &UnixStream| {
        let k = next.replace(next.get() + 1);
        parties.get(k).map(|p| p.to_string())
    });
    let mut file: Json = serde_json::from_str(&std::fs::read_to_string(docket().join("defaults/window.json")).unwrap()).unwrap();
    file.as_object_mut().unwrap().extend([("owner".into(), json!(OWNER)), ("socket".into(), json!(d.join("window.sock"))), ("words".into(), json!(d.join("his-words")))]);
    let config = Config::read(&file.to_string()).unwrap();
    Window::new(config, whose).unwrap()
}

/// A connection to the driver, and what it is told.
struct His(UnixStream, BufReader<UnixStream>);

impl His {
    fn open(d: &Path) -> His {
        let s = UnixStream::connect(d.join("window.sock")).unwrap();
        His(s.try_clone().unwrap(), BufReader::new(s))
    }
    fn say(&mut self, j: Json) {
        writeln!(self.0, "{j}").unwrap();
    }
    fn told(&mut self) -> Json {
        let mut l = String::new();
        self.1.read_line(&mut l).unwrap();
        serde_json::from_str(&l).unwrap()
    }
}

fn post(text: &str) -> Json {
    json!({"words": {"text": text, "on": "t1", "to": [AGENT], "at": AT}})
}

fn decision(line: usize, answer: Json) -> Json {
    json!({"decision": {"line": line, "answer": answer}})
}

/// The words at passage `id`, a span `t:PATH:OFF:LEN` of a file, read as the turns driver reads them.
fn span(id: &str) -> String {
    let (rest, len) = id.strip_prefix("t:").unwrap().rsplit_once(':').unwrap();
    let (path, off) = rest.rsplit_once(':').unwrap();
    let bytes = std::fs::read(path).unwrap();
    let (off, len) = (off.parse::<usize>().unwrap(), len.parse::<usize>().unwrap());
    String::from_utf8(bytes[off..off + len].to_vec()).unwrap()
}

/// A connection the daemon vouches for as another party is told no; his first seats him, and a post of his keeps
/// its words and is said to the agent, put on its thread and timed, its passage told to his window.
#[test]
fn only_his_window_is_kept_and_his_post_keeps_his_words_and_gives_his_events() {
    let d = dir("post");
    let mut w = window(&d, &[AGENT, OWNER, OWNER]);
    let mut other = His::open(&d);
    assert_eq!(w.ready(), (vec![], vec![]));
    assert!(other.told()["no"].as_str().unwrap().contains("w1 is not o1"));
    let mut his = His::open(&d);
    let (_, seat) = w.ready();
    assert_eq!(seat, [Instance { ty: SEATED.into(), args: vec![Value::text(OWNER)] }]);
    let mut node = Vec::new();
    for words in ["Keep the outline small.", "Two levels at most:\nno more."] {
        his.say(post(words));
        let (answers, events) = w.ready();
        let id = his.told()["ok"].as_str().unwrap().to_string();
        assert_eq!(span(&id), words);
        let n = Value::text(&id);
        let want = [(SAYS, vec![n.clone(), Value::text(AGENT)]), (POSTS, vec![n.clone(), Value::text("t1")]), (WRITTEN, vec![n, Value::Int(AT)])];
        assert_eq!((answers, events), (vec![], want.map(|(ty, args)| Instance { ty: ty.into(), args }).to_vec()));
        node.push(id);
    }
    assert_ne!(node[0], node[1], "each post its own span");
    let mut again = His::open(&d);
    again.say(post(""));
    assert_eq!(w.ready(), (vec![], vec![]), "seated once; an empty post gives nothing");
    assert!(again.told()["no"].is_string());
}

/// His decision answers a line the driver carries, with the answer of that line's Effect, once; a line it does not
/// hold, or no longer, or none named, is told no.
#[test]
fn a_decision_answers_only_a_line_the_driver_holds_and_only_as_its_effect_takes() {
    let d = dir("decide");
    let mut w = window(&d, &[OWNER]);
    assert_eq!((w.carry(APPROVAL, &[], 7), w.carry(BUDGET, &[], 9)), (Reply::Later, Reply::Later));
    let mut his = His::open(&d);
    w.ready();
    let said = [
        decision(7, json!({"granted": 5})),
        decision(7, json!({"approved": "Run the check, then say."})),
        decision(7, json!({"rejected": "again"})),
        decision(8, json!({"denied": "no"})),
        json!({"decision": {"answer": {"granted": 5}}}),
        json!({"ask": {"line": 9}}),
    ];
    for s in &said {
        his.say(s.clone());
    }
    let (answers, _) = w.ready();
    let told: Vec<Json> = said.iter().map(|_| his.told()).collect();
    assert_eq!(told.iter().map(|t| t.get("ok").is_some()).collect::<Vec<_>>(), [false, true, false, false, false, false]);
    assert!(told[4]["no"].as_str().unwrap().contains("missing field `line`"), "{}", told[4]);
    let approved = Value::ctor("std.control.Approved", vec![Value::text("Run the check, then say.")]);
    assert_eq!(answers, [(7, Reply::Answer { value: approved, costs: vec![], events: vec![] })]);
    w.dropped(9);
    his.say(decision(9, json!({"granted": 5})));
    w.ready();
    assert!(his.told()["no"].as_str().unwrap().contains("line 9"), "dropped, it waits no more");
}

/// What the test adds to the package: an agent asks him to pass its words, and asks him for more to spend.
const T: &str = "module t
import std.hooks as H
import std.control as Ctrl
import std.budgets as B
Act Ask(by: H.Party, words: String) Returns Ctrl.Approval Does { a <- perform Ctrl.Await-Approval(by, words); return a }
Act Spend(by: H.Party, b: B.Bucket, d: B.Dimension, n: Int) Returns B.BudgetDecision Does { a <- perform B.Request-Budget(b, d, n); return a }
";

/// The driver as the daemon links it: held by the machine and by the loop that takes what it has ready.
struct Linked(Rc<RefCell<Window>>);

impl Driver for Linked {
    fn name(&self) -> Vec<Value> {
        self.0.borrow().name()
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        self.0.borrow().effects()
    }
    fn events(&self) -> Vec<String> {
        self.0.borrow().events()
    }
    fn carry(&mut self, op: &str, args: &[Value], line: u64) -> Reply {
        self.0.borrow_mut().carry(op, args, line)
    }
    fn dropped(&mut self, line: u64) {
        self.0.borrow_mut().dropped(line)
    }
}

/// Every other Effect the package leaves to a driver; a passage and a delivery answered as turns answers them.
struct Rest(Vec<String>);

impl Driver for Rest {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("rest")]
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        self.0.iter().map(|e| (e.clone(), Vec::new())).collect()
    }
    fn events(&self) -> Vec<String> {
        Vec::new()
    }
    fn carry(&mut self, op: &str, args: &[Value], _: u64) -> Reply {
        let value = match (op, args.first()) {
            ("std.comms.TextOf", Some(Value::Text(id))) => Value::text(&span(id)),
            ("std.control.Deliver", _) => Value::text(""),
            _ => return Reply::Failed(format!("{op} is not this test's")),
        };
        Reply::Answer { value, costs: vec![], events: vec![] }
    }
    fn dropped(&mut self, _: u64) {}
}

fn walk(d: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(d).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "docket") {
            out.push(p);
        }
    }
}

/// The package (its standard library and defaults) and the test's acts, on the window's driver, then the rest.
fn machine(w: Rc<RefCell<Window>>) -> Machine<Core> {
    let root = docket();
    let mut files = Vec::new();
    walk(&root.join("std"), &mut files);
    walk(&root.join("defaults"), &mut files);
    let mut modules: Vec<_> = files.iter().map(|p| docket_v2::syntax::read(&std::fs::read_to_string(p).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).collect();
    modules.push(docket_v2::syntax::read(T).unwrap());
    let loaded = docket_v2::check::load(modules).unwrap_or_else(|e| panic!("the package: {e}"));
    let its: Vec<String> = w.borrow().effects().into_iter().map(|(e, _)| e).collect();
    let rest = loaded.syms.iter().filter(|(n, s)| matches!(s, Sym::Effect { does: None, .. }) && !its.contains(n)).map(|(n, _)| n.clone()).collect();
    Machine::new(loaded, Core::default(), vec![Box::new(Linked(w)), Box::new(Rest(rest))]).unwrap()
}

/// The Events the driver has ready performed, as the daemon's loop performs them.
fn perform(m: &mut Machine<Core>, w: &Rc<RefCell<Window>>) -> Vec<(u64, Reply)> {
    let (answers, events) = w.borrow_mut().ready();
    for e in events {
        m.event(0, e);
    }
    m.run();
    answers
}

/// The value run `r` ended with.
fn ended(m: &Machine<Core>, r: usize) -> Option<Value> {
    m.record.iter().find_map(|l| match l {
        Line::Ended { run, value } if *run == m.run_name(r) => Some(value.clone()),
        _ => None,
    })
}

/// On the real package, with yaaihh's file: the driver's names are the package's Events and Effects; his words reach
/// the agent by the package's own handler; his decisions are answers of their Effects' types, ending the runs that asked.
#[test]
fn on_the_package_his_post_reaches_the_agent_and_his_decisions_end_the_runs_that_asked() {
    let d = dir("package");
    let w = Rc::new(RefCell::new(window(&d, &[OWNER])));
    let mut m = machine(w.clone());
    for e in w.borrow().events() {
        assert!(matches!(m.loaded.syms.get(&e), Some(Sym::Norm { kind: Norm::Event, .. })), "{e} is no Event of the package");
    }
    for (e, _) in w.borrow().effects() {
        assert!(matches!(m.loaded.syms.get(&e), Some(Sym::Effect { does: None, .. })), "{e} is no Effect a driver carries");
    }
    let mut his = His::open(&d);
    perform(&mut m, &w);
    let seat = m.start(Value::text(OWNER), Some(0), "", Instance { ty: "std.comms.Seat".into(), args: [OWNER, AGENT, "front"].map(Value::text).to_vec() }).unwrap();
    m.run();
    assert_eq!(m.state(seat), State::Done, "seated by his window, he seats the agent");

    let from = m.record.len();
    his.say(post("Keep the outline small."));
    perform(&mut m, &w);
    assert_eq!(his.told()["ok"].as_str().map(span).as_deref(), Some("Keep the outline small."));
    let lines: Vec<String> = m.record[from..].iter().map(|l| l.to_string()).collect();
    for done in [SAYS, POSTS, WRITTEN, "yaaihh.desk.Handed"] {
        assert!(lines.iter().any(|l| l.starts_with(&format!("performed({done}("))), "{done}: {lines:?}");
    }
    assert!(lines.iter().any(|l| l.starts_with("effect(std.control.Deliver(") && l.contains("Keep the outline small.")), "{lines:?}");

    let ask = m.start(Value::text(AGENT), None, "", Instance { ty: "t.Ask".into(), args: [AGENT, "A note for w2."].map(Value::text).to_vec() }).unwrap();
    let spend = m.start(Value::text(AGENT), None, "", Instance { ty: "t.Spend".into(), args: vec![Value::text(AGENT), Value::text("front"), Value::text("wall_ms"), Value::Int(60_000)] }).unwrap();
    m.run();
    let asked = |op: &str| (0..m.record.len()).rev().find(|&k| matches!(&m.record[k], Line::Effect { op: o, .. } if o == op)).unwrap();
    let (a, s) = (asked(APPROVAL), asked(BUDGET));
    assert_eq!((m.carrier(a), m.carrier(s), m.state(ask), m.state(spend)), (Some(0), Some(0), State::Resting(a), State::Resting(s)));
    his.say(decision(a, json!({"approved": "A note for w2, shorter."})));
    his.say(decision(s, json!({"denied": "not today"})));
    let answers = perform(&mut m, &w);
    assert!(his.told()["ok"].is_null() && his.told()["ok"].is_null());
    for (line, reply) in answers {
        m.answer(line as usize, reply).unwrap();
    }
    m.run();
    assert_eq!(ended(&m, ask), Some(Value::ctor("std.control.Approved", vec![Value::text("A note for w2, shorter.")])));
    assert_eq!(ended(&m, spend), Some(Value::ctor("std.budgets.Denied", vec![Value::text("not today")])));
}
