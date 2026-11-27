//! `coordd-v2 ask`: an agy session's pull and report asked for once each over the daemon's socket, on the package's
//! delivery texts, and the run's value read back plain; a pull for another party, a refused one and no daemon, told.

use coordd_v2::ask::{ask, plain, told, Asked};
use coordd_v2::docket::{Bind, Docket, Texts};
use coordd_v2::link::{link, Ready, Waits};
use coordd_v2::peer::{Party, Vouch};
use coordd_v2::serve::{Engine, Terminals};
use docket_v2::check::names::Sym;
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::value::{Instance, Value};
use serde_json::json;
use std::os::fd::RawFd;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::UnixListener;
use tokio::sync::{broadcast, mpsc};
use tokio::task::{spawn_local, LocalSet};

const TEXTS: [&str; 9] = [
    "std/hooks.docket", "std/comms.docket", "std/rails.docket", "std/budgets.docket", "std/delivery.docket",
    "std/bridges/claude-code.docket", "std/bridges/codex.docket", "std/bridges/agy.docket", "scenarios/s11-delivery-world.docket",
];
const TICK: &str = "std.budgets.Tick";
const KICKED: &str = "std.bridges.agy.Session-Kicked-Off";
const PULL: &str = "std.bridges.agy.Pull-Brain";
const POST: &str = "std.delivery.Post";
const REPORT: &str = "std.delivery.Report-Work";

/// Names every Effect a driver carries, so the texts load; carries none of them (nothing here may reach a
/// session), and performs the clock's and agy-rpc's Events.
struct Nothing(Vec<String>);

impl Driver for Nothing {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("nothing")]
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        self.0.iter().map(|e| (e.clone(), vec![])).collect()
    }
    fn events(&self) -> Vec<String> {
        vec![TICK.into(), KICKED.into()]
    }
    fn carry(&mut self, op: &str, _: &[Value], _: u64) -> Reply {
        Reply::Failed(format!("{op} reaches no session in this test"))
    }
    fn dropped(&mut self, _: u64) {}
}

impl Waits for Nothing {
    fn waits(&self) -> Vec<RawFd> {
        vec![]
    }
    fn ready(&mut self) -> Vec<Ready> {
        vec![]
    }
}

/// Vouches for every connection as the party set now.
struct Now(Arc<Mutex<&'static str>>);

impl Vouch for Now {
    fn party(&self, _: &str) -> Option<Party> {
        Some(Party::of(*self.0.lock().unwrap(), Some(0)))
    }
}

struct NoTerminals;

impl Terminals for NoTerminals {
    fn open(&self, _: &Party, s: &str) -> Result<(mpsc::UnboundedSender<Vec<u8>>, broadcast::Receiver<Vec<u8>>), String> {
        Err(format!("no terminal {s}"))
    }
}

fn design() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docket")
}

#[tokio::test]
async fn a_pull_and_a_report_asked_once_each_give_their_runs_values_plain_and_a_wrong_caller_is_told() {
    LocalSet::new().run_until(async {
        let dir = std::env::temp_dir().join(format!("ask-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let modules = TEXTS.iter().map(|t| docket_v2::syntax::read(&std::fs::read_to_string(design().join(t)).unwrap()).unwrap_or_else(|e| panic!("{t}: {e}"))).collect();
        let loaded = docket_v2::check::load(modules).unwrap();
        let effects = loaded.syms.iter().filter(|(_, s)| matches!(s, Sym::Effect { does: None, .. })).map(|(n, _)| n.clone()).collect();
        let runs = docket_v2::run::machine::Machine::new(loaded, docket_v2::norm::Core::default(), vec![link(Nothing(effects)).0]).unwrap();
        let (bind, texts): (Bind, Texts) = (Box::new(|_, _| (None, String::new())), Box::new(|_| Err("empty".into())));
        let who = Arc::new(Mutex::new("o1"));
        let engine = Engine::open(&dir.join("record.jsonl"), Box::new(Docket::new(runs, "scenario.s11", vec![], bind, texts)), Box::new(Now(who.clone())), Box::new(NoTerminals)).unwrap();
        let sock = dir.join("s");
        spawn_local(engine.clone().serve(UnixListener::bind(&sock).unwrap()));
        let event = |ty: &str, args: Vec<Value>| engine.event(0, Instance { ty: ty.into(), args });
        // one ask from its own thread, as `coordd-v2 ask` is a process of its own, while the engine serves here
        let asked = async |party: &'static str, name: &'static str, args: &[&str]| {
            *who.lock().unwrap() = party;
            let (sock, args): (PathBuf, Vec<String>) = (sock.clone(), args.iter().map(|a| a.to_string()).collect());
            let t = std::thread::spawn(move || ask(&sock, name, &args, Duration::from_secs(10)));
            while !t.is_finished() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            t.join().unwrap()
        };

        event(TICK, vec![Value::Int(100)]);
        assert!(matches!(asked("g1", PULL, &["g1", "starting"]).await, Asked::Refused(why) if why.contains("policy")), "not kicked off: refused, said so");
        event(KICKED, vec![Value::text("g1"), Value::Bool(true), Value::Int(100)]);
        assert_eq!(asked("o1", POST, &["o1", "g1", "k1", "read the spec", "Normal"]).await, Asked::Ended(json!(null)));
        let Asked::Ended(b) = asked("g1", PULL, &["g1", "starting"]).await else { panic!("the pull ends with its briefing") };
        assert_eq!(b, json!({"mode": "work", "priority_directive": "", "new_directives": [], "unread_messages": ["[k1 from o1] read the spec"], "assigned_items": [], "suggested_wake_mins": 10, "wake_hint": "pull again in 10 min (read_brain); a pull later than minute 110 and the grace is told to the owner"}));
        assert_eq!(told(&Asked::Ended(b.clone())), (b.to_string(), 0), "printed alone, on stdout");
        // the pull is the caller's own: as another party it is refused at the source
        assert!(matches!(asked("w1", PULL, &["g1", "x"]).await, Asked::Refused(why) if why == "source"));
        assert_eq!(asked("g1", REPORT, &["g1", "read it", "post p3", ""]).await, Asked::Ended(json!("reported")));
        let unknown = asked("g1", "std.delivery.No-Such-Act", &["g1"]).await;
        assert!(matches!(&unknown, Asked::Refused(w) if w == "std.delivery.No-Such-Act is no Act or Event") && told(&unknown).1 == 3, "{unknown:?}");
        let Asked::Ended(b) = asked("g1", PULL, &["g1", "after"]).await else { panic!("a second pull") };
        assert_eq!((b["mode"].clone(), b["unread_messages"].clone(), b["suggested_wake_mins"].clone()), (json!("sleep"), json!([]), json!(20)), "the report read what the first pull handed");
        let gone = ask(&dir.join("none"), PULL, &[], Duration::from_secs(1));
        assert!(matches!(&gone, Asked::NotTaken(w) if w.contains("none")) && told(&gone).1 == 2, "{gone:?}");
        let _ = std::fs::remove_dir_all(&dir);
    })
    .await;
}

#[test]
fn a_record_value_is_written_plain_its_records_as_objects_and_tuples_as_arrays() {
    let v = json!({"record": "a.B", "fields": [["xs", [{"tuple": ["m", 1]}]], ["c", {"ctor": "Some", "args": [true]}]]});
    assert_eq!(plain(&v), json!({"xs": [["m", 1]], "c": {"ctor": "Some", "args": [true]}}));
}
