//! Reads over the socket on the real machine: instance queries, enabled_for, and subscriptions whose deltas hold
//! only what moved; a subscriber that never reads, or is gone, never holds up an act.

use coordd_v2::docket::{Bind, Docket, Texts};
use coordd_v2::machine::Machine;
use coordd_v2::peer::{Party, Vouch};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use coordd_v2::serve::{Engine, Terminals};
use serde_json::{json, Value as Json};
use std::collections::BTreeSet;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};
use tokio::task::{spawn_local, LocalSet};

const LENDING: &str = r#"
module t
Fact Person Identified by String
Fact Book Identified by String
Fact Shelf(book: Book, room: String)
Fact Lent(book: Book, to: Person, by: Person)
Fact Overdue(book: Book)
Duty Give-Back(holder: Person, claimant: Person, book: Book)
  Holds when Lent(book, holder, claimant)
  Violated when Overdue(book)
Act Lend(by: Person, book: Book, to: Person)
  Conditioned by by != to && Not(Exists p, q : Lent(book, p, q))
  Creates Lent(book, to, by)
Act Take-Back(by: Person, book: Book)
  Conditioned by Exists p : Lent(book, p, by)
  Terminates Lent(book, p, by)
Act Mark(by: Person, book: Book)
  Creates Overdue(book)
+Person("w1"). +Person("bob"). +Person("cy").
+Shelf("b1", "hall"). +Shelf("b2", "attic").
"#;

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

/// The engine on LENDING, served at a socket of its own; the socket's path.
fn served(name: &str) -> std::path::PathBuf {
    engine(name).0
}

/// A machine that counts its views.
struct Counted<M>(M, Arc<AtomicUsize>);

impl<M: Machine> Machine for Counted<M> {
    fn restore(&mut self, l: &[Json]) -> Result<(), String> {
        self.0.restore(l)
    }
    fn act(&mut self, p: &Party, n: &str, a: &[Json]) -> Result<Json, String> {
        self.0.act(p, n, a)
    }
    fn answer(&mut self, p: &Party, l: u64, a: &coordd_v2_wire::Answer) -> Result<Json, String> {
        self.0.answer(p, l, a)
    }
    fn late(&mut self, l: u64, r: docket_v2::run::driver::Reply) -> Result<(), String> {
        self.0.late(l, r)
    }
    fn event(&mut self, d: usize, e: docket_v2::run::value::Instance) -> Result<(), String> {
        self.0.event(d, e)
    }
    fn query(&mut self, p: &Party, q: &str) -> Result<Json, String> {
        self.0.query(p, q)
    }
    fn holding(&mut self, p: &Party, n: &str) -> Result<Json, String> {
        self.0.holding(p, n)
    }
    fn inst_query(&mut self, p: &Party, e: &str) -> Result<Json, String> {
        self.0.inst_query(p, e)
    }
    fn enabled_for(&mut self, p: &Party, a: &Json, t: &Json) -> Result<Json, String> {
        self.0.enabled_for(p, a, t)
    }
    fn view(&mut self, t: Option<&[String]>) -> Result<coordd_v2::machine::View, String> {
        self.1.fetch_add(1, Ordering::SeqCst);
        self.0.view(t)
    }
    fn delta(&mut self, v: &mut coordd_v2::machine::View, seq: u64) -> Result<coordd_v2_wire::StepDelta, String> {
        self.0.delta(v, seq)
    }
    fn unwatch(&mut self) {
        self.0.unwatch()
    }
    fn holds(&mut self, m: &str, c: &str) -> Result<bool, String> {
        self.0.holds(m, c)
    }
    fn written(&mut self) -> Vec<Json> {
        self.0.written()
    }
}

/// The engine on LENDING, served at a socket of its own: the socket's path and the engine.
fn engine(name: &str) -> (std::path::PathBuf, std::rc::Rc<Engine>) {
    let (sock, e, _) = counted(name);
    (sock, e)
}

/// The engine on LENDING, served at a socket of its own: the socket's path, the engine, and how many views it read.
fn counted(name: &str) -> (std::path::PathBuf, std::rc::Rc<Engine>, Arc<AtomicUsize>) {
    let dir = std::env::temp_dir().join(format!("subscribe-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let loaded = docket_v2::check::load(vec![docket_v2::syntax::read(LENDING).unwrap()]).unwrap();
    let runs = docket_v2::run::machine::Machine::new(loaded, docket_v2::norm::Core::default(), vec![]).unwrap();
    let (bind, texts): (Bind, Texts) = (Box::new(|_, _| (None, String::new())), Box::new(|_| Err("this record starts empty".into())));
    let views = Arc::new(AtomicUsize::new(0));
    let machine = Counted(Docket::new(runs, "t", vec![], bind, texts), views.clone());
    let engine = Engine::open(&dir.join("record.jsonl"), Box::new(machine), Box::new(W1), Box::new(NoTerminals)).unwrap();
    let sock = dir.join("s");
    spawn_local(engine.clone().serve(UnixListener::bind(&sock).unwrap()));
    (sock, engine, views)
}

struct Conn {
    rd: Lines<BufReader<OwnedReadHalf>>,
    wr: OwnedWriteHalf,
}

impl Conn {
    async fn open(sock: &std::path::Path) -> Conn {
        let (rd, wr) = UnixStream::connect(sock).await.unwrap().into_split();
        Conn { rd: BufReader::new(rd).lines(), wr }
    }
    async fn send(&mut self, j: Json) {
        self.wr.write_all(format!("{j}\n").as_bytes()).await.unwrap();
    }
    async fn next(&mut self) -> Json {
        serde_json::from_str(&tokio::time::timeout(Duration::from_secs(5), self.rd.next_line()).await.unwrap().unwrap().unwrap()).unwrap()
    }
    /// The next reply that is not a delta.
    async fn reply(&mut self) -> Json {
        loop {
            let r = self.next().await;
            if r["is"] != "delta" {
                return r;
            }
        }
    }
    async fn ask(&mut self, j: Json) -> Json {
        self.send(j).await;
        self.reply().await
    }
    async fn act(&mut self, name: &str, args: &[&str]) {
        let r = self.ask(json!({"do": "act", "name": name, "args": args})).await;
        assert_eq!(r["is"], "done", "{name}{args:?}: {r}");
    }
}

fn lent(book: &str, to: &str, by: &str) -> Json {
    json!({"inst": "t.Lent", "args": [book, to, by]})
}

fn give_back(holder: &str, claimant: &str, book: &str) -> Json {
    json!({"inst": "t.Give-Back", "args": [holder, claimant, book]})
}

#[tokio::test]
async fn instance_queries_and_enabled_for_answer_over_the_socket() {
    LocalSet::new()
        .run_until(async {
            let sock = served("reads");
            let mut c = Conn::open(&sock).await;
            c.act("t.Lend", &["w1", "b1", "bob"]).await;
            c.act("t.Lend", &["w1", "b2", "cy"]).await;
            let r = c.ask(json!({"do": "inst_query", "expr": r#"Foreach b, p : (b, p) When Lent(b, p, "w1") && Shelf(b, "hall")"#})).await;
            assert_eq!(r["value"], json!([{"tuple": ["b1", "bob"]}]), "{r}");
            let r = c.ask(json!({"do": "inst_query", "expr": "Foreach h, c, b : Give-Back(h, c, b) When Give-Back(h, c, b) && h == \"cy\""})).await;
            assert_eq!(r["value"], json!([give_back("cy", "w1", "b2")]), "{r}");
            let r = c.ask(json!({"do": "holding", "name": "t.Lent"})).await;
            assert_eq!(r["value"], json!([lent("b1", "bob", "w1"), lent("b2", "cy", "w1")]), "holding as before: {r}");
            let target = json!([{"inst": "t.Lend", "args": ["w1", "b3", "bob"]}, {"inst": "t.Lend", "args": ["w1", "b1", "cy"]}]);
            let r = c.ask(json!({"do": "enabled_for", "actor": "w1", "target": target})).await;
            assert_eq!(r["value"][0]["enabled"], true, "{r}");
            assert_eq!((r["value"][1]["enabled"].clone(), r["value"][1]["why"].clone()), (json!(false), json!("not enabled")), "{r}");
            let r = c.ask(json!({"do": "inst_query", "expr": "Lent"})).await;
            assert_eq!(r["is"], "no", "{r}");
            // a join of twelve over three people would try 3^12 bindings: it is refused at its cap, and the next ask is answered
            let xs: Vec<String> = (0..12).map(|k| format!("a{k}")).collect();
            let join = format!("Foreach {} : a0 When {}", xs.join(", "), xs.iter().map(|x| format!("Person({x})")).collect::<Vec<_>>().join(" && "));
            let r = c.ask(json!({"do": "inst_query", "expr": join})).await;
            assert!(r["is"] == "no" && r["why"].as_str().unwrap().contains("its cap"), "{r}");
            let many: Vec<Json> = (0..65).map(|k| json!({"inst": "t.Lend", "args": ["w1", format!("b{k}"), "bob"]})).collect();
            let r = c.ask(json!({"do": "enabled_for", "actor": "w1", "target": many})).await;
            assert!(r["is"] == "no" && r["why"].as_str().unwrap().contains("at most 64"), "{r}");
            let r = c.ask(json!({"do": "holding", "name": "t.Lent"})).await;
            assert_eq!(r["value"].as_array().unwrap().len(), 2, "{r}");
        })
        .await;
}

/// Every instance a delta lists in FIELD, its frames in order.
fn listed(frames: &[Json], field: &str) -> Vec<Json> {
    frames.iter().flat_map(|f| f[field].as_array().unwrap().clone()).collect()
}

#[tokio::test]
async fn a_subscriber_gets_only_what_moved_and_one_that_never_reads_or_is_gone_holds_nothing_up() {
    LocalSet::new()
        .run_until(async {
            let (sock, engine, views) = counted("deltas");
            let mut sub = Conn::open(&sock).await;
            let snap = sub.ask(json!({"do": "subscribe", "types": ["t.Lent", "t.Give-Back"]})).await;
            assert_eq!(snap["value"]["facts"], json!({"t.Lent": []}), "{snap}");
            assert_eq!(snap["value"]["duties"], json!({"t.Give-Back": []}), "{snap}");
            let mut stuck = Conn::open(&sock).await;
            assert_eq!(stuck.ask(json!({"do": "subscribe"})).await["is"], "done");
            assert_eq!(engine.listening(), 2);
            {
                let mut gone = Conn::open(&sock).await;
                assert_eq!(gone.ask(json!({"do": "subscribe"})).await["is"], "done");
                assert_eq!(engine.listening(), 3);
            }
            let end = Instant::now() + Duration::from_secs(5);
            while engine.listening() > 2 {
                assert!(Instant::now() < end, "the gone subscriber's task ends");
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let mut a = Conn::open(&sock).await;
            let empty = json!([]);
            a.act("t.Lend", &["w1", "b1", "bob"]).await;
            let d = sub.next().await;
            assert_eq!((d["created_facts"].clone(), d["new_duties"].clone()), (json!([lent("b1", "bob", "w1")]), json!([give_back("bob", "w1", "b1")])), "{d}");
            assert_eq!((&d["terminated_facts"], &d["terminated_duties"], &d["violations"]), (&empty, &empty, &empty), "{d}");
            assert_eq!(d["since"], snap["value"]["seq"]);
            a.act("t.Mark", &["w1", "b1"]).await;
            let v = sub.next().await;
            assert_eq!(v["violations"], json!([give_back("bob", "w1", "b1")]), "only the violation: Overdue is not subscribed: {v}");
            assert_eq!((&v["created_facts"], &v["new_duties"], v["since"].clone()), (&empty, &empty, d["seq"].clone()), "{v}");
            a.act("t.Take-Back", &["w1", "b1"]).await;
            let t = sub.next().await;
            assert_eq!((t["terminated_facts"].clone(), t["terminated_duties"].clone(), &t["created_facts"]), (json!([lent("b1", "bob", "w1")]), json!([give_back("bob", "w1", "b1")]), &empty), "{t}");

            // the stuck connection never reads; its frames, each over 40 kB, fill its socket long before these end
            let big = "x".repeat(20_000);
            let read = views.load(Ordering::SeqCst);
            let began = Instant::now();
            for i in 0..50 {
                a.act("t.Lend", &["w1", &format!("{big}{i}"), "cy"]).await;
            }
            let took = began.elapsed();
            assert_eq!(views.load(Ordering::SeqCst), read, "no change reads a whole view, however many subscribe or stop reading");
            assert!(took < Duration::from_secs(20), "50 acts took {took:?}");
            let mut got = Vec::new();
            while listed(&got, "created_facts").iter().filter(|f| f["inst"] == "t.Lent").count() < 51 {
                got.push(stuck.next().await);
            }
            assert!(got.iter().all(|f| f["is"] == "delta"));
            assert!(got.windows(2).all(|w| w[1]["since"] == w[0]["seq"]), "each frame goes on from the last");
            let books: BTreeSet<String> = listed(&got, "created_facts").iter().filter(|f| f["inst"] == "t.Lent").map(|f| f["args"][0].as_str().unwrap().to_string()).collect();
            assert_eq!(books.len(), 51, "none lost: b1 and the 50");
            assert!(got.len() < 53, "a reader behind gets one frame for many changes: {} frames for 53 changes", got.len());
            eprintln!("50 acts with a stuck subscriber: {took:?}; it got {} frames for 53 changes", got.len());
        })
        .await;
}

/// A projection (`Engine::step_deltas`) and a subscriber get the same one delta of a change.
#[tokio::test]
async fn a_projection_and_a_subscriber_get_the_one_delta_of_a_change() {
    LocalSet::new()
        .run_until(async {
            let (sock, engine) = engine("projection");
            let (view, mut rx) = engine.step_deltas().unwrap();
            assert!(view.facts["t.Lent"].is_empty());
            let mut sub = Conn::open(&sock).await;
            assert_eq!(sub.ask(json!({"do": "subscribe"})).await["is"], "done");
            let mut a = Conn::open(&sock).await;
            a.act("t.Lend", &["w1", "b1", "bob"]).await;
            let d = rx.recv().await.unwrap();
            let frame = sub.next().await;
            assert_eq!(serde_json::to_value(&d).unwrap(), json!({"seq": frame["seq"], "created_facts": frame["created_facts"], "terminated_facts": [], "new_duties": frame["new_duties"], "terminated_duties": [], "violations": [], "cleared_violations": []}));
            assert_eq!(d.created_facts, [lent("b1", "bob", "w1")]);
            assert_eq!(d.new_duties, [give_back("bob", "w1", "b1")]);
        })
        .await;
}

/// A frame goes on from the change its client was last given, though changes to types it does not take came between;
/// the kept view goes with the last subscriber, so the next one reads it whole again.
#[tokio::test]
async fn a_frame_goes_on_from_the_change_last_given_and_the_view_goes_with_the_last_subscriber() {
    LocalSet::new()
        .run_until(async {
            let (sock, engine, views) = counted("since");
            let mut a = Conn::open(&sock).await;
            {
                let mut sub = Conn::open(&sock).await;
                let snap = sub.ask(json!({"do": "subscribe", "types": ["t.Lent"]})).await;
                a.act("t.Lend", &["w1", "b1", "bob"]).await;
                let one = sub.next().await;
                assert_eq!(one["since"], snap["value"]["seq"], "{one}");
                // Overdue and the duty's violation move; Lent does not
                a.act("t.Mark", &["w1", "b1"]).await;
                a.act("t.Lend", &["w1", "b2", "bob"]).await;
                let two = sub.next().await;
                assert_eq!(two["since"], one["seq"], "the client stands at the first frame's change: {two}");
                assert_eq!(two["created_facts"], json!([lent("b2", "bob", "w1")]));
            }
            let end = Instant::now() + Duration::from_secs(5);
            while engine.listening() > 0 {
                assert!(Instant::now() < end, "the subscriber's task ends");
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let read = views.load(Ordering::SeqCst);
            let mut again = Conn::open(&sock).await;
            assert_eq!(again.ask(json!({"do": "subscribe", "types": ["t.Lent"]})).await["is"], "done");
            assert_eq!(views.load(Ordering::SeqCst), read + 1, "the view went with the last subscriber, no change between");
        })
        .await;
}
