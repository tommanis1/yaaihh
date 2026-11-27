//! The frame on a real socket, around a stand-in machine, a stand-in vouching table and an echoing terminal.

use coordd_v2::machine::Machine;
use coordd_v2::peer::{Party, Vouch};
use coordd_v2::serve::{Engine, Terminals};
use coordd_v2_wire::Answer;
use docket_v2::run::driver::Reply;
use serde_json::{json, Value as Json};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};
use tokio::task::{spawn_local, LocalSet};

/// A machine with one number: `Add(n)` adds n and is a `performed` line, `Add` of a negative n a `refused` line; an
/// answer is an `answered` line and a `charged` line per cost, a failure a `raised` line; it starts with a `loaded`
/// line and reads nothing it did not write.
#[derive(Default)]
struct Tally {
    n: i64,
    new: Vec<Json>,
    started: bool,
}

impl Machine for Tally {
    fn restore(&mut self, lines: &[Json]) -> Result<(), String> {
        self.started = true;
        self.n += lines.iter().map(|l| l["performed"]["args"][0].as_i64().unwrap_or(0)).sum::<i64>();
        Ok(())
    }
    fn act(&mut self, party: &Party, name: &str, args: &[Json]) -> Result<Json, String> {
        let n = match (name, args) {
            ("Add", [n]) => n.as_i64().ok_or("Add takes a number")?,
            _ => return Err(format!("no Act {name}")),
        };
        if n < 0 {
            self.new.push(json!({"refused": {"act": name, "args": args, "party": party.name, "why": "policy"}}));
        } else {
            self.n += n;
            self.new.push(json!({"performed": {"act": name, "args": args, "party": party.name}}));
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
    fn late(&mut self, line: u64, reply: Reply) -> Result<(), String> {
        match reply {
            Reply::Answer { costs, .. } => self.answer(&Party::of("", None), line, &Answer::Value { value: json!("late"), costs }).map(|_| ()),
            Reply::Failed(why) => self.answer(&Party::of("", None), line, &Answer::Failed { failed: why }).map(|_| ()),
            Reply::Later => Err("a late answer is not Later".into()),
        }
    }
    fn event(&mut self, driver: usize, ev: docket_v2::run::value::Instance) -> Result<(), String> {
        self.new.push(json!({"performed": {"act": ev.ty, "driver": driver}}));
        Ok(())
    }
    fn query(&mut self, party: &Party, query: &str) -> Result<Json, String> {
        (query == "n").then(|| json!({"n": self.n, "as": party.name})).ok_or_else(|| format!("no query {query}"))
    }
    fn holding(&mut self, party: &Party, name: &str) -> Result<Json, String> {
        (name == "n").then(|| json!([{"n": self.n, "as": party.name}])).ok_or_else(|| format!("no type {name}"))
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

/// A terminal that sends back what is typed, in capitals.
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

struct Client {
    rd: Lines<BufReader<OwnedReadHalf>>,
    wr: OwnedWriteHalf,
}

impl Client {
    async fn to(sock: &Path) -> Client {
        let (rd, wr) = UnixStream::connect(sock).await.unwrap().into_split();
        Client { rd: BufReader::new(rd).lines(), wr }
    }
    async fn send(&mut self, req: Json) {
        self.wr.write_all(format!("{req}\n").as_bytes()).await.unwrap();
    }
    async fn next(&mut self) -> Json {
        let line = tokio::time::timeout(Duration::from_secs(5), self.rd.next_line()).await.expect("a reply in time").unwrap().expect("open");
        serde_json::from_str(&line).unwrap()
    }
    async fn ask(&mut self, req: Json) -> Json {
        self.send(req).await;
        self.next().await
    }
}

fn place(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("frame-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The engine on `dir`'s record, served on a socket there; its socket's path.
fn start(dir: &Path, vouched: bool) -> (Rc<Engine>, PathBuf) {
    let engine = Engine::open(&dir.join("record.jsonl"), Box::new(Tally::default()), Box::new(Word(vouched)), Box::new(Echo)).unwrap();
    let sock = dir.join("s");
    let _ = std::fs::remove_file(&sock);
    spawn_local(engine.clone().serve(UnixListener::bind(&sock).unwrap()));
    (engine, sock)
}

/// The machine's own first line is written at open; an act's lines are in the file before a watcher has them; a
/// watcher gets the old lines, then the new, once each and in order; `older` pages back; a query and an answer go to
/// the machine as the connection's party; a request the machine cannot take is `no`, a refused act is a line.
#[tokio::test]
async fn acts_are_written_then_watched_in_order_and_paged_back() {
    LocalSet::new().run_until(async {
        let dir = place("acts");
        let (_e, sock) = start(&dir, true);
        let mut c = Client::to(&sock).await;
        assert_eq!(c.ask(json!({"do": "act", "name": "Add", "args": [2]})).await, json!({"is": "done", "value": 2}));
        let mut w = Client::to(&sock).await;
        w.send(json!({"do": "watch", "from": 0})).await;
        assert_eq!(w.next().await, json!({"is": "done", "value": null}), "a watch's done comes first");
        assert_eq!(w.next().await, json!({"is": "line", "at": 0, "line": {"loaded": "00"}}), "written at open");
        assert_eq!(w.next().await["line"]["performed"]["args"], json!([2]));
        assert_eq!(c.ask(json!({"do": "act", "name": "Add", "args": [3]})).await["value"], 5);
        assert_eq!(c.ask(json!({"do": "act", "name": "Add", "args": [-1]})).await["value"], 5);
        assert_eq!(w.next().await["at"], 2);
        assert_eq!(w.next().await["line"]["refused"]["why"], "policy", "a refusal is a line");
        assert_eq!(std::fs::read_to_string(dir.join("record.jsonl")).unwrap().lines().count(), 4);
        assert_eq!(c.ask(json!({"do": "answer", "line": 1, "answer": {"value": "ok", "costs": [["tokens", 120]]}})).await, json!({"is": "done", "value": null}));
        assert_eq!(w.next().await["line"], json!({"answered": {"line": 1, "value": "ok"}}));
        assert_eq!(w.next().await["line"], json!({"charged": {"dim": "tokens", "n": 120}}), "its cost reaches the machine");
        assert_eq!(c.ask(json!({"do": "query", "query": "n"})).await, json!({"is": "done", "value": {"n": 5, "as": "p1"}}));
        assert_eq!(c.ask(json!({"do": "holding", "name": "n"})).await, json!({"is": "done", "value": [{"n": 5, "as": "p1"}]}));
        assert_eq!(c.ask(json!({"do": "act", "name": "Take", "args": []})).await, json!({"is": "no", "why": "no Act Take"}));
        assert_eq!(c.ask(json!({"do": "send", "text": "x"})).await["is"], "no", "not one of the seven");
        c.send(json!({"do": "older", "before": 3, "n": 2})).await;
        assert_eq!(c.next().await["at"], 1);
        assert_eq!(c.next().await["at"], 2);
        assert_eq!(c.next().await, json!({"is": "done", "value": null}));
        let mut late = Client::to(&sock).await;
        late.send(json!({"do": "watch", "from": 3})).await;
        assert_eq!(late.next().await, json!({"is": "done", "value": null}), "a watch's done comes first");
        assert_eq!(late.next().await["at"], 3, "from the place asked");
        assert_eq!(late.next().await["at"], 4);
        let _ = std::fs::remove_dir_all(&dir);
    })
    .await
}

/// A linked driver's late answer, or its failure, goes in without the socket, and its lines reach a watcher as any
/// change's do.
#[tokio::test]
async fn a_linked_drivers_late_answer_goes_in_without_the_socket() {
    LocalSet::new().run_until(async {
        let dir = place("late");
        let (engine, sock) = start(&dir, true);
        let mut w = Client::to(&sock).await;
        w.send(json!({"do": "watch", "from": 1})).await;
        assert_eq!(w.next().await, json!({"is": "done", "value": null}), "a watch's done comes first");
        let r = engine.late(0, Reply::Answer { value: docket_v2::run::value::Value::Unit, costs: vec![("tokens".into(), 7)], events: vec![] });
        assert_eq!(serde_json::to_value(r).unwrap(), json!({"is": "done", "value": null}));
        assert_eq!(w.next().await["line"], json!({"answered": {"line": 0, "value": "late"}}));
        assert_eq!(w.next().await["line"], json!({"charged": {"dim": "tokens", "n": 7}}));
        engine.late(0, Reply::Failed("the session ended".into()));
        assert_eq!(w.next().await["line"], json!({"raised": {"effect": "Fault", "why": "the session ended"}}), "a failure goes in by the same step");
        let _ = std::fs::remove_dir_all(&dir);
    })
    .await
}

/// A connection nothing vouches for is told so and served nothing; the file holds only the machine's own line.
#[tokio::test]
async fn a_connection_nothing_vouches_for_is_served_nothing() {
    LocalSet::new().run_until(async {
        let dir = place("none");
        let (_e, sock) = start(&dir, false);
        let mut c = Client::to(&sock).await;
        c.send(json!({"do": "act", "name": "Add", "args": [1]})).await;
        let r = c.next().await;
        assert!(r["is"] == "no" && r["why"].as_str().unwrap().contains("nothing vouches"), "{r}");
        assert_eq!(std::fs::read_to_string(dir.join("record.jsonl")).unwrap().lines().count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    })
    .await
}

/// The engine started again on its file has the machine restore it before it serves.
#[tokio::test]
async fn an_engine_started_again_replays_its_record_first() {
    LocalSet::new().run_until(async {
        let dir = place("again");
        std::fs::write(dir.join("record.jsonl"), "{\"performed\":{\"args\":[4]}}\n{\"performed\":{\"args\":[6]}}\n").unwrap();
        let (_e, sock) = start(&dir, true);
        let mut c = Client::to(&sock).await;
        assert_eq!(c.ask(json!({"do": "query", "query": "n"})).await["value"]["n"], 10);
        assert_eq!(c.ask(json!({"do": "act", "name": "Add", "args": [1]})).await["value"], 11);
        let _ = std::fs::remove_dir_all(&dir);
    })
    .await
}

/// Requests sent at once get their `done` or `no` in the order they were sent, a watch's among them.
#[tokio::test]
async fn each_request_gets_one_reply_in_the_order_sent() {
    LocalSet::new().run_until(async {
        let dir = place("order");
        let (_e, sock) = start(&dir, true);
        let mut c = Client::to(&sock).await;
        let reqs = [json!({"do": "act", "name": "Add", "args": [1]}), json!({"do": "act", "name": "Take", "args": []}), json!({"do": "query", "query": "n"}), json!({"do": "act", "name": "Add", "args": [2]})];
        let text: String = reqs.iter().map(|r| format!("{r}\n")).collect();
        c.wr.write_all(text.as_bytes()).await.unwrap();
        let mut replies = Vec::new();
        while replies.len() < 4 {
            let r = c.next().await;
            if r["is"] != "line" {
                replies.push(r);
            }
        }
        assert_eq!(replies, [json!({"is": "done", "value": 1}), json!({"is": "no", "why": "no Act Take"}), json!({"is": "done", "value": {"n": 1, "as": "p1"}}), json!({"is": "done", "value": 3})]);
        let _ = std::fs::remove_dir_all(&dir);
    })
    .await
}

/// Bytes typed into a terminal reach it, and its output comes back on the same connection; a terminal no driver
/// holds is `no`.
#[tokio::test]
async fn a_terminals_bytes_go_in_and_its_output_comes_back() {
    LocalSet::new().run_until(async {
        let dir = place("bytes");
        let (_e, sock) = start(&dir, true);
        let mut c = Client::to(&sock).await;
        c.send(json!({"do": "bytes", "session": "s1", "data": "aGk="})).await;
        let mut two = vec![c.next().await.to_string(), c.next().await.to_string()];
        two.sort();
        // the output may come before the reply: they go out from two places
        assert_eq!(two, [json!({"is": "bytes", "session": "s1", "data": "SEk="}).to_string(), json!({"is": "done", "value": null}).to_string()], "hi, echoed as HI");
        assert_eq!(c.ask(json!({"do": "bytes", "session": "s2", "data": "aGk="})).await, json!({"is": "no", "why": "no terminal s2"}));
        assert_eq!(c.ask(json!({"do": "bytes", "session": "s1", "data": "%%"})).await["is"], "no", "not base64");
        let _ = std::fs::remove_dir_all(&dir);
    })
    .await
}
