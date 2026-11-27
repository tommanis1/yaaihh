//! The daemon's parts together: docket-v2's machine and its eFLINT core behind the frame, and a linked driver's late answer.

use coordd_v2::docket::{Bind, Docket, Texts};
use coordd_v2::machine::Machine;
use coordd_v2_wire::Answer;
use coordd_v2::link::{link, wait_on, Ready, Waits};
use coordd_v2::peer::{Party, Vouch};
use coordd_v2::serve::{Engine, Terminals};
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::value::{Instance, Value};
use serde_json::{json, Value as Json};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream as OsStream;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};
use tokio::task::{spawn_local, LocalSet};

/// Carries `t.Work` by answering `Later`, and answers 9 at a cost of 4 tokens once a byte reaches its socket; on the
/// byte `e` it performs Event `t.Ping(1)` on its own.
struct Slow {
    sock: OsStream,
    resting: Option<u64>,
}

impl Driver for Slow {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("stand-in"), Value::text("1")]
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        vec![("t.Work".into(), vec!["tokens".into()])]
    }
    fn events(&self) -> Vec<String> {
        vec!["t.Ping".into()]
    }
    fn carry(&mut self, _: &str, _: &[Value], line: u64) -> Reply {
        self.resting = Some(line);
        Reply::Later
    }
    fn dropped(&mut self, _: u64) {
        self.resting = None;
    }
}

impl Waits for Slow {
    fn waits(&self) -> Vec<RawFd> {
        vec![self.sock.as_raw_fd()]
    }
    fn ready(&mut self) -> Vec<Ready> {
        let mut b = [0u8; 16];
        match (&self.sock).read(&mut b) {
            Ok(n) if n > 0 && b[0] == b'e' => vec![Ready::Event(Instance { ty: "t.Ping".into(), args: vec![Value::Int(1)] })],
            Ok(n) if n > 0 => self.resting.take().map(|l| Ready::Answer(l, Reply::Answer { value: Value::Int(9), costs: vec![("tokens".into(), 4)], events: vec![] })).into_iter().collect(),
            _ => vec![],
        }
    }
}

/// For a record that starts empty: nothing is restored, so nothing is bound and no old text is asked for.
fn unbound() -> (Bind, Texts) {
    (Box::new(|_, _| (None, String::new())), Box::new(|_| Err("this record starts empty".into())))
}

struct W1;

impl Vouch for W1 {
    fn party(&self, _: &str) -> Option<Party> {
        Some(Party::of("w1", None))
    }
}

struct NoTerminals;

impl Terminals for NoTerminals {
    fn open(&self, _: &Party, s: &str) -> Result<(mpsc::UnboundedSender<Vec<u8>>, broadcast::Receiver<Vec<u8>>), String> {
        Err(format!("no terminal {s}"))
    }
}

/// An act begins a run on the real machine; the run rests on its driver's `Later`; a byte on the driver's socket
/// wakes the daemon's loop, which gives the answer to the machine; the run ends with it, every line in the file and
/// at the watcher in order. An Event the driver then performs on its own is a line too.
#[tokio::test]
async fn an_act_runs_on_the_machine_and_a_late_answer_off_a_drivers_socket_ends_it() {
    LocalSet::new().run_until(async {
        let dir = std::env::temp_dir().join(format!("daemon-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let text = "module t\nEffect Work() -> Int\nEvent Ping(n: Int)\nAct Go(by: String) Returns Int Does { perform Work() }";
        let loaded = docket_v2::check::load(vec![docket_v2::syntax::read(text).unwrap()]).unwrap();
        let (ours, theirs) = OsStream::pair().unwrap();
        theirs.set_nonblocking(true).unwrap();
        let (driver, waits) = link(Slow { sock: theirs, resting: None });
        let runs = docket_v2::run::machine::Machine::new(loaded, docket_v2::norm::Core::default(), vec![driver]).unwrap();
        let engine = Engine::open(&dir.join("record.jsonl"), Box::new({
            let (bind, texts) = unbound();
            Docket::new(runs, "t", vec![], bind, texts)
        }), Box::new(W1), Box::new(NoTerminals)).unwrap();
        let sock = dir.join("s");
        spawn_local(engine.clone().serve(UnixListener::bind(&sock).unwrap()));
        spawn_local(wait_on(engine.clone(), waits, 0));
        let (rd, mut wr) = UnixStream::connect(&sock).await.unwrap().into_split();
        let mut rd = BufReader::new(rd).lines();
        let mut next = async || -> Json { serde_json::from_str(&tokio::time::timeout(Duration::from_secs(5), rd.next_line()).await.unwrap().unwrap().unwrap()).unwrap() };
        wr.write_all(b"{\"do\":\"act\",\"name\":\"t.Go\",\"args\":[\"w1\"]}\n").await.unwrap();
        let done = next().await;
        assert_eq!((done["is"].clone(), done["value"]["run"].clone()), (json!("done"), json!("r1")), "{done}");
        assert_eq!(done["value"]["state"], json!({"resting": 3}), "{done}");
        (&ours).write_all(b"x").unwrap();
        wr.write_all(b"{\"do\":\"watch\",\"from\":0}\n").await.unwrap();
        assert_eq!(next().await["is"], "done");
        let mut kinds = Vec::new();
        while kinds.last().map(String::as_str) != Some("ended") {
            let l = next().await;
            kinds.push(l["line"].as_object().unwrap().keys().next().unwrap().clone());
            if kinds.last().map(String::as_str) == Some("ended") {
                assert_eq!(l["line"]["ended"]["value"], 9, "{l}");
            }
        }
        assert_eq!(kinds, ["loaded", "began", "performed", "effect", "answered", "charged", "ended"]);
        assert_eq!(std::fs::read_to_string(dir.join("record.jsonl")).unwrap().lines().count(), 7);
        (&ours).write_all(b"e").unwrap();
        let ping = next().await;
        assert_eq!((ping["at"].clone(), ping["line"]["performed"]["act"].clone()), (json!(7), json!({"inst": "t.Ping", "args": [1]})), "{ping}");
        let _ = std::fs::remove_dir_all(&dir);
    })
    .await
}

const TEXT: &str = "module t\nFact F Identified by String\nEffect Work() -> Int\nAct Go(by: String) Returns Int Does { perform Work() }\nAct Put(by: String, x: String) Creates F(x)";

/// docket-v2's machine on `TEXT`, its one driver carrying `t.Work` and answered for over the socket if `by_socket`.
fn docket(by_socket: bool) -> Docket<docket_v2::norm::Core> {
    let loaded = docket_v2::check::load(vec![docket_v2::syntax::read(TEXT).unwrap()]).unwrap();
    let (_, theirs) = OsStream::pair().unwrap();
    let runs = docket_v2::run::machine::Machine::new(loaded, docket_v2::norm::Core::default(), vec![Box::new(Slow { sock: theirs, resting: None })]).unwrap();
    let (bind, texts) = unbound();
    let mut d = Docket::new(runs, "t", if by_socket { vec![0] } else { vec![] }, bind, texts);
    d.written();
    d
}

/// An answer over the socket is taken only from the party the table names for the driver that carries the line,
/// and only for a driver not linked; any other is refused with nothing written.
#[test]
fn an_answer_from_another_party_is_refused() {
    let helper = Party::of("h1", Some(0));
    let other = Party::of("w2", None);
    let ok = json!(9);
    for by_socket in [true, false] {
        let mut d = docket(by_socket);
        d.act(&Party::of("w1", Some(0)), "t.Go", &[json!("w1")]).unwrap();
        let effect = d.written().iter().position(|l| l.get("effect").is_some()).unwrap() as u64 + 1;
        let answer = Answer::Value { value: ok.clone(), costs: vec![] };
        assert!(d.answer(&other, effect, &answer).is_err(), "another party");
        assert!(d.written().is_empty(), "nothing written");
        assert_eq!(d.answer(&helper, effect, &answer).is_ok(), by_socket, "the carrying driver's party, only when not linked");
        assert_eq!(d.written().last().and_then(|l| l.get("ended")).is_some(), by_socket);
    }
}

/// A query or a list reads the facts as no run does: the next effect's key is the one it would have been with neither.
#[test]
fn a_query_or_a_list_leaves_no_read_in_the_next_key() {
    let key = |ask: bool| {
        let mut d = docket(false);
        for x in ["b", "a"] {
            d.act(&Party::of("w1", Some(0)), "t.Put", &[json!("w1"), json!(x)]).unwrap();
        }
        if ask {
            assert_eq!(d.query(&Party::of("w1", None), "Holds(F(\"a\"))").unwrap(), json!(true));
            assert_eq!(d.holding(&Party::of("w1", None), "t.F").unwrap(), json!([{"inst": "t.F", "args": ["a"]}, {"inst": "t.F", "args": ["b"]}]));
            assert!(d.holding(&Party::of("w1", None), "t.Go").is_err(), "an Act lists nothing");
        }
        d.act(&Party::of("w1", Some(0)), "t.Go", &[json!("w1")]).unwrap();
        d.written().into_iter().find_map(|l| l.get("effect").map(|e| e["key"].clone())).unwrap()
    };
    assert_eq!(key(true), key(false));
}

/// A restart restores the record line for line, writing nothing, reading no head and putting no folder, so an
/// agent's folder, changed and not committed, is as it was; the run after it reads the head as any run does.
#[test]
fn a_restart_restores_the_record_and_leaves_an_agents_folder_alone() {
    use std::cell::Cell;
    use std::rc::Rc;
    let dir = std::env::temp_dir().join(format!("restart-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let folder = dir.to_str().unwrap().to_string();
    let git = |args: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(&folder).args(["-c", "user.name=t", "-c", "user.email=t"]).args(args).output().unwrap().status.success(), "{args:?}");
    git(&["init", "-q"]);
    std::fs::write(dir.join("a"), "1").unwrap();
    git(&["add", "a"]);
    git(&["commit", "-q", "-m", "one"]);
    std::fs::write(dir.join("a"), "2").unwrap();
    let make = |heads: Rc<Cell<usize>>| {
        let loaded = docket_v2::check::load(vec![docket_v2::syntax::read(TEXT).unwrap()]).unwrap();
        let (_, theirs) = OsStream::pair().unwrap();
        let mut runs = docket_v2::run::machine::Machine::new(loaded, docket_v2::norm::Core::default(), vec![Box::new(Slow { sock: theirs, resting: None })]).unwrap();
        runs.head = Box::new(move |f| {
            heads.set(heads.get() + 1);
            coordd_v2::head::head(f)
        });
        runs.put = Box::new(|f, h| panic!("{f} put at {h}"));
        let f = folder.clone();
        Docket::new(runs, "t", vec![], Box::new(move |_, _| (Some(0), f.clone())), Box::new(|_| Err("one text only".into())))
    };
    let w1 = Party { folder: folder.clone(), ..Party::of("w1", Some(0)) };
    let (heads, again) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let mut first = make(heads.clone());
    let mut lines = first.written();
    first.act(&w1, "t.Go", &[json!("w1")]).unwrap();
    lines.extend(first.written());
    let effect = lines.iter().position(|l| l.get("effect").is_some()).unwrap() as u64;
    first.late(effect, Reply::Answer { value: Value::Int(9), costs: vec![], events: vec![] }).unwrap();
    lines.extend(first.written());
    assert_eq!(heads.get(), 1, "the head read where the effect line was written");
    assert_eq!(lines.last().and_then(|l| l.get("ended")).map(|e| e["value"].clone()), Some(json!(9)));
    let index = || std::fs::read(dir.join(".git/index")).unwrap();
    let before = (index(), std::fs::read(dir.join("a")).unwrap());
    let mut second = make(again.clone());
    second.written();
    second.restore(&lines).unwrap();
    assert!(second.written().is_empty(), "the record made again, nothing written beyond it");
    assert_eq!(again.get(), 0, "no head read at a restart");
    assert_eq!((index(), std::fs::read(dir.join("a")).unwrap()), before, "the folder and its index as they were");
    let next = second.act(&w1, "t.Go", &[json!("w1")]).unwrap();
    assert_eq!((next["run"].clone(), again.get()), (json!(format!("r{}", lines.len())), 1), "the next run reads the head as any run does");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A start as the daemon makes it, from files: a second engine on the first one's record, with a changed text,
/// restores it with the old text from `RECORD.texts`, loads the changed text and replaces the run not ended by one
/// of the new text, which takes over the effect it rests on with no line written for it, and ends with its answer
/// (11); with the kept file gone the start is refused, naming it.
#[tokio::test]
async fn a_start_on_a_record_restores_it_from_the_kept_texts_and_loads_a_changed_text() {
    LocalSet::new().run_until(async {
        let dir = std::env::temp_dir().join(format!("start-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let record = dir.join("record.jsonl");
        let changed = format!("{TEXT}\nAct Again(by: String) Returns Int Does {{ perform Work() }}");
        let open = |text: &str, asked: std::rc::Rc<std::cell::Cell<usize>>| {
            let texts = [("t.docket".to_string(), text.to_string())];
            let loaded = coordd_v2::texts::load(&texts).unwrap();
            coordd_v2::texts::keep(&record, &coordd_v2::codec::hex(&loaded.hash), &texts).unwrap();
            let (_, theirs) = OsStream::pair().unwrap();
            let (driver, slow) = link(Slow { sock: theirs, resting: None });
            let runs = docket_v2::run::machine::Machine::new(loaded, docket_v2::norm::Core::default(), vec![driver]).unwrap();
            let kept = record.clone();
            let texts: Texts = Box::new(move |h: &[u8; 32]| {
                asked.set(asked.get() + 1);
                coordd_v2::texts::kept(&kept, h)
            });
            Engine::open(&record, Box::new(Docket::new(runs, "t", vec![], Box::new(|_, _| (Some(0), String::new())), texts)), Box::new(W1), Box::new(NoTerminals)).map(|e| (e, slow))
        };
        let (first, _) = open(TEXT, Default::default()).unwrap();
        let sock = dir.join("s");
        spawn_local(first.clone().serve(UnixListener::bind(&sock).unwrap()));
        let (rd, mut wr) = UnixStream::connect(&sock).await.unwrap().into_split();
        let mut rd = BufReader::new(rd).lines();
        wr.write_all(b"{\"do\":\"act\",\"name\":\"t.Go\",\"args\":[\"w1\"]}\n").await.unwrap();
        let done: Json = serde_json::from_str(&tokio::time::timeout(Duration::from_secs(5), rd.next_line()).await.unwrap().unwrap().unwrap()).unwrap();
        assert_eq!(done["value"]["state"], json!({"resting": 3}), "{done}");
        let read = || std::fs::read_to_string(&record).unwrap().lines().map(|l| serde_json::from_str::<Json>(l).unwrap()).collect::<Vec<_>>();
        let before = read();
        assert_eq!(before.len(), 4, "loaded, began, performed, effect");
        let asked = std::rc::Rc::new(std::cell::Cell::new(0));
        let (second, slow) = open(&changed, asked.clone()).unwrap();
        let after = read();
        assert_eq!(after[..4], before[..], "the record as it stood");
        assert!(after[4].get("loaded").is_some_and(|h| h != &before[0]["loaded"]), "the changed text loaded: {}", after[4]);
        assert_eq!(after[5..], [json!({"cancelled": {"run": "r1", "by": "reload"}}), json!({"began": {"run": "r6", "by": "w1"}})], "r1 replaced, its effect taken over and not carried again");
        assert_eq!(asked.get(), 1, "the old text read from the kept file");
        // a fresh daemon's driver holds nothing of the old one's, so the effect r6 took over is asked once, as any
        // resting at a restart; no `Work` line is written for it
        assert_eq!(slow.borrow().resting, Some(3), "the fresh driver asked for line 3, the effect r6 took over");
        assert_eq!(second.late(3, Reply::Answer { value: Value::Int(9), costs: vec![("tokens".into(), 4)], events: vec![] }), coordd_v2_wire::Reply::Done { value: Json::Null });
        let ended = read();
        assert_eq!(ended[7..].iter().map(|l| l.as_object().unwrap().keys().next().unwrap().as_str()).collect::<Vec<_>>(), ["answered", "charged", "prefix", "ended"]);
        assert_eq!((ended[9]["prefix"]["run"].clone(), ended[9]["prefix"]["old"].clone()), (json!("r6"), json!("r1")), "r6 read r1's lines up to there: {}", ended[9]);
        assert_eq!(ended.last().unwrap()["ended"], json!({"run": "r6", "value": 9}), "the answer reaches r6, which ends with r1's value");
        drop(second);
        let gone = coordd_v2::texts::place(&record, before[0]["loaded"].as_str().unwrap());
        std::fs::remove_file(&gone).unwrap();
        let why = open(&changed, Default::default()).err().map(|e| e.to_string()).unwrap();
        assert!(why.contains(gone.to_str().unwrap()), "{why}");
        assert_eq!(read(), ended, "a refused start writes nothing");
        let _ = std::fs::remove_dir_all(&dir);
    })
    .await
}
