//! The link against coordd-v2's own engine on a real socket, around a stand-in machine, vouching word and terminal.
#![cfg(unix)]

use coordd_v2::machine::Machine;
use coordd_v2::peer::{Party, Vouch};
use coordd_v2::serve::{Engine, Terminals};
use coordd_v2_link::pipe::{command, socket, Connect, Pipe};
use coordd_v2_link::{Answer, Incoming, Link};
use docket_v2::run::driver::Reply;
use serde_json::{json, Value as Json};
use std::collections::BTreeSet;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc};
use tokio::task::{spawn_local, LocalSet};

/// One number: `Add(n)` adds n as a `performed` line; `Slow` holds the engine a while and writes nothing; an answer
/// is an `answered` line and a `charged` line per cost; it starts with a `loaded` line.
#[derive(Default)]
struct Tally {
    n: i64,
    new: Vec<Json>,
    started: bool,
}

impl Machine for Tally {
    fn restore(&mut self, lines: &[Json]) -> Result<(), String> {
        self.started = !lines.is_empty();
        Ok(())
    }
    fn act(&mut self, party: &Party, name: &str, args: &[Json]) -> Result<Json, String> {
        match (name, args) {
            ("Add", [n]) => {
                self.n += n.as_i64().ok_or("Add takes a number")?;
                self.new.push(json!({"performed": {"act": name, "args": args, "party": party.name}}));
            }
            ("Slow", []) => std::thread::sleep(Duration::from_millis(400)),
            _ => return Err(format!("no Act {name}")),
        }
        Ok(json!(self.n))
    }
    fn answer(&mut self, _: &Party, line: u64, answer: &Answer) -> Result<Json, String> {
        match answer {
            Answer::Value { value, costs } => {
                self.new.push(json!({"answered": {"line": line, "value": value}}));
                self.new.extend(costs.iter().map(|(dim, n)| json!({"charged": {"dim": dim, "n": n}})));
            }
            Answer::Failed { failed } => self.new.push(json!({"raised": {"effect": "Fault", "why": failed}})),
        }
        Ok(Json::Null)
    }
    fn late(&mut self, _: u64, _: Reply) -> Result<(), String> {
        Err("no late answers here".into())
    }
    fn event(&mut self, _: usize, _: docket_v2::run::value::Instance) -> Result<(), String> {
        Err("no driver here".into())
    }
    fn query(&mut self, party: &Party, query: &str) -> Result<Json, String> {
        (query == "n").then(|| json!({"n": self.n, "as": party.name})).ok_or_else(|| format!("no query {query}"))
    }
    fn holding(&mut self, party: &Party, name: &str) -> Result<Json, String> {
        (name == "n").then(|| json!([{"n": self.n, "as": party.name}])).ok_or_else(|| format!("no type {name}"))
    }
    fn view(&mut self, _: Option<&[String]>) -> Result<coordd_v2::machine::View, String> {
        Ok(coordd_v2::machine::View { facts: [("n".to_string(), coordd_v2::machine::held([json!({"inst": "n", "args": [self.n]})]))].into(), ..Default::default() })
    }
    fn holds(&mut self, _: &str, cond: &str) -> Result<bool, String> {
        Err(format!("the tally keeps no facts: {cond}"))
    }
    fn written(&mut self) -> Vec<Json> {
        if !self.started {
            self.started = true;
            self.new.insert(0, json!({"loaded": "00"}));
        }
        std::mem::take(&mut self.new)
    }
}

/// Vouches for every cgroup as `p1`, or for none.
struct Word(bool);

impl Vouch for Word {
    fn party(&self, _: &str) -> Option<Party> {
        self.0.then(|| Party::of("p1", None))
    }
}

/// Terminal `s1`, which sends back what is typed in capitals.
struct Echo;

impl Terminals for Echo {
    fn open(&self, _: &Party, session: &str) -> Result<(mpsc::UnboundedSender<Vec<u8>>, broadcast::Receiver<Vec<u8>>), String> {
        if session != "s1" {
            return Err(format!("no terminal {session}"));
        }
        let (tx, mut typed) = mpsc::unbounded_channel::<Vec<u8>>();
        let (shown, rx) = broadcast::channel(16);
        spawn_local(async move {
            while let Some(b) = typed.recv().await {
                let _ = shown.send(b.to_ascii_uppercase());
            }
        });
        Ok((tx, rx))
    }
}

/// An engine serving at a new socket, on a thread of its own.
fn engine(tag: &str, vouched: bool) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("v2-link-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let sock = dir.join("s");
    let (ready, up) = std::sync::mpsc::channel();
    let at = sock.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        LocalSet::new().block_on(&rt, async move {
            let e = Engine::open(&at.with_file_name("record.jsonl"), Box::new(Tally::default()), Box::new(Word(vouched)), Box::new(Echo)).unwrap();
            let l = tokio::net::UnixListener::bind(&at).unwrap();
            ready.send(()).unwrap();
            let _ = e.serve(l).await;
        });
    });
    up.recv().unwrap();
    sock
}

fn woken() -> (coordd_v2_link::Wake, Arc<AtomicUsize>) {
    let n = Arc::new(AtomicUsize::new(0));
    let m = n.clone();
    (Arc::new(move || { m.fetch_add(1, Ordering::SeqCst); }), n)
}

/// What comes until `done` holds of it, failing after five seconds.
fn until(l: &Link, mut done: impl FnMut(&[Incoming]) -> bool) -> Vec<Incoming> {
    let (mut got, end) = (Vec::new(), Instant::now() + Duration::from_secs(5));
    while !done(&got) {
        assert!(Instant::now() < end, "in time: {got:?}");
        got.extend(l.next(Duration::from_millis(50)));
    }
    got
}

fn answer_of(got: &[Incoming], id: u64) -> Option<Result<Json, String>> {
    got.iter().find_map(|i| match i {
        Incoming::Done { id: k, value } if *k == id => Some(Ok(value.clone())),
        Incoming::No { id: Some(k), why } if *k == id => Some(Err(why.clone())),
        _ => None,
    })
}

fn places(got: &[Incoming]) -> BTreeSet<u64> {
    got.iter().filter_map(|i| if let Incoming::Line { at, .. } = i { Some(*at) } else { None }).collect()
}

fn up(l: &Link) {
    until(l, |g| g.iter().any(|i| matches!(i, Incoming::Up { .. })));
}

fn down(got: &[Incoming]) -> Option<&str> {
    got.iter().find_map(|i| if let Incoming::Down(why) = i { Some(why.as_str()) } else { None })
}

/// Each of the five requests and a terminal's bytes gets its own reply by id, however the replies and the watched
/// lines interleave; the watch sees every line; the window is woken for what comes.
#[test]
fn each_request_gets_its_own_reply_and_the_watch_sees_every_line() {
    let sock = engine("five", true);
    let (wake, wakes) = woken();
    let l = Link::start(socket(sock), wake);
    up(&l);
    assert!(l.watch(0));
    let add = l.act("Add", vec![json!(2)]).unwrap();
    let query = l.query("n").unwrap();
    let take = l.act("Take", vec![]).unwrap();
    let answer = l.answer(1, Answer::Value { value: json!("ok"), costs: vec![("tokens".into(), 3)] }).unwrap();
    let older = l.older(2, 2).unwrap();
    let typed = l.bytes("s1", b"hi").unwrap();
    let none = l.bytes("s2", b"hi").unwrap();
    let ids = [add, query, take, answer, older, typed, none];
    let got = until(&l, |g| ids.iter().all(|id| answer_of(g, *id).is_some()) && g.iter().any(|i| matches!(i, Incoming::Bytes { .. })) && places(g).len() == 4);
    assert_eq!(answer_of(&got, add), Some(Ok(json!(2))));
    assert_eq!(answer_of(&got, query), Some(Ok(json!({"n": 2, "as": "p1"}))), "as the party it was vouched for");
    assert_eq!(answer_of(&got, take), Some(Err("no Act Take".into())));
    assert_eq!(answer_of(&got, answer), Some(Ok(Json::Null)));
    assert_eq!(answer_of(&got, older), Some(Ok(Json::Null)));
    assert_eq!(answer_of(&got, typed), Some(Ok(Json::Null)));
    assert_eq!(answer_of(&got, none), Some(Err("no terminal s2".into())));
    assert!(got.contains(&Incoming::Bytes { session: "s1".into(), data: b"HI".to_vec() }), "the terminal's output");
    assert!(got.contains(&Incoming::Line { at: 3, line: json!({"charged": {"dim": "tokens", "n": 3}}) }), "the answer's cost, watched");
    assert!(wakes.load(Ordering::SeqCst) >= got.len(), "woken for each thing that came");
    assert!(got.iter().all(|i| !matches!(i, Incoming::Done { id, .. } if !ids.contains(id))), "the watch's own `done` is taken by the link");
}

/// A dropped connection reports the request still waiting as lost, not refused, says it is down, and the next
/// connection watches again from the first line not seen: the lines written meanwhile come, none twice.
#[test]
fn a_dropped_connection_loses_what_waits_and_the_next_watches_from_the_first_unseen_line() {
    let sock = engine("drop", true);
    let cut: Arc<Mutex<Option<UnixStream>>> = Arc::default();
    let (path, held) = (sock.clone(), cut.clone());
    let connect: Connect = Box::new(move || {
        let s = UnixStream::connect(&path)?;
        *held.lock().unwrap() = Some(s.try_clone()?);
        let (rd, wr, end) = (s.try_clone()?, s.try_clone()?, s);
        Ok(Pipe { rd: Box::new(rd), wr: Box::new(wr), end: Box::new(move || drop(end.shutdown(std::net::Shutdown::Both))), said: Box::new(|| None) })
    });
    let (wake, _) = woken();
    let l = Link::start(connect, wake);
    up(&l);
    l.watch(0);
    let first = l.act("Add", vec![json!(1)]).unwrap();
    until(&l, |g| answer_of(g, first).is_some() && places(g).contains(&1));
    let slow = l.act("Slow", vec![]).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    cut.lock().unwrap().take().unwrap().shutdown(std::net::Shutdown::Both).unwrap();
    let got = until(&l, |g| down(g).is_some());
    assert!(got.iter().any(|i| matches!(i, Incoming::Lost { id, .. } if *id == slow)), "lost, not refused: {got:?}");
    assert_eq!(answer_of(&got, slow), None);
    let (other_wake, _) = woken();
    let other = Link::start(socket(sock), other_wake);
    up(&other);
    let second = other.act("Add", vec![json!(5)]).unwrap();
    until(&other, |g| answer_of(g, second).is_some());
    let again = until(&l, |g| !places(g).is_empty());
    assert!(again.contains(&Incoming::Up { from: Some(2) }), "watching again from line 2: {again:?}");
    let first_line = again.iter().find(|i| matches!(i, Incoming::Line { .. })).unwrap();
    assert_eq!(first_line, &Incoming::Line { at: 2, line: json!({"performed": {"act": "Add", "args": [5], "party": "p1"}}) }, "the line written while it was down, not an old one again");
    let after = l.act("Add", vec![json!(1)]).unwrap();
    let got = until(&l, |g| answer_of(g, after).is_some());
    assert_eq!(answer_of(&got, after), Some(Ok(json!(7))), "the watch asked again took its `done`: replies still match requests");
}

/// A connection nothing vouches for is told so; the window hears why.
#[test]
fn a_connection_nothing_vouches_for_hears_why() {
    let sock = engine("none", false);
    let (wake, _) = woken();
    let l = Link::start(socket(sock), wake);
    up(&l);
    l.act("Add", vec![json!(1)]);
    let got = until(&l, |g| g.iter().any(|i| matches!(i, Incoming::No { why, .. } if why.contains("vouches"))));
    assert!(!got.iter().any(|i| matches!(i, Incoming::Done { .. })), "served nothing: {got:?}");
}

/// Requests made while no connection is open are not sent, and say so; the watch asked waits for the next one.
#[test]
fn nothing_is_sent_while_down() {
    let dir = std::env::temp_dir().join(format!("v2-link-nowhere-{}", std::process::id()));
    let (wake, _) = woken();
    let l = Link::start(socket(dir.join("no-socket")), wake);
    until(&l, |g| down(g).is_some());
    assert_eq!(l.act("Add", vec![json!(1)]), None);
    assert!(!l.watch(0), "asked, for when a connection opens");
}

/// A bridge command that cannot reach the engine says why on its error output; the window hears its last line.
#[test]
fn a_bridge_that_fails_is_heard_by_its_last_error_line() {
    let (wake, _) = woken();
    let l = Link::start(command(vec!["sh".into(), "-c".into(), "echo starting >&2; echo 'no socket at /run/x' >&2; exit 1".into()]), wake);
    let got = until(&l, |g| down(g).is_some());
    assert!(down(&got).unwrap().ends_with("it said: no socket at /run/x"), "{got:?}");
}

/// A `done` no request waits for ends the connection: every reply after it would go to the wrong request.
#[test]
fn a_reply_no_request_waits_for_ends_the_connection() {
    let (wake, _) = woken();
    let l = Link::start(command(vec!["sh".into(), "-c".into(), r#"printf '{"is":"done","value":1}\n'; sleep 2"#.into()]), wake);
    let got = until(&l, |g| down(g).is_some());
    assert!(down(&got).unwrap().contains("no request waited"), "{got:?}");
    assert!(!got.iter().any(|i| matches!(i, Incoming::Done { .. })));
}

/// A subscription's `done` holds what holds; each change after it comes as a delta of what moved alone.
#[test]
fn a_subscription_gives_its_view_then_a_delta_for_each_change() {
    let sock = engine("subscribe", true);
    let (wake, _) = woken();
    let l = Link::start(socket(sock), wake);
    up(&l);
    let sub = l.subscribe(None).unwrap();
    let got = until(&l, |g| answer_of(g, sub).is_some());
    let snap = answer_of(&got, sub).unwrap().unwrap();
    assert_eq!(snap["facts"], json!({"n": [{"inst": "n", "args": [0]}]}), "{snap}");
    let add = l.act("Add", vec![json!(5)]).unwrap();
    let got = until(&l, |g| answer_of(g, add).is_some() && g.iter().any(|i| matches!(i, Incoming::Delta(_))));
    let Some(Incoming::Delta(coordd_v2_wire::Reply::Delta { since, delta })) = got.iter().find(|i| matches!(i, Incoming::Delta(_))) else { panic!() };
    assert!(delta.violations.is_empty() && delta.cleared_violations.is_empty(), "{delta:?}");
    assert_eq!((delta.created_facts.clone(), delta.terminated_facts.clone(), *since), (vec![json!({"inst": "n", "args": [5]})], vec![json!({"inst": "n", "args": [0]})], snap["seq"].as_u64().unwrap()));
}
