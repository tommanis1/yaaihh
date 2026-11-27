//! `coordd-v2 act`: a session's run asked for from its own scope, again until the table vouches for the scope, then held.

use coordd_v2::docket::{Bind, Docket, Texts};
use coordd_v2::link::{link, Ready, Waits};
use coordd_v2::peer::{Party, Vouch};
use coordd_v2::serve::{Engine, Terminals};
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::value::Value;
use std::cell::RefCell;
use std::io::Write;
use std::os::fd::RawFd;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::UnixListener;
use tokio::sync::{broadcast, mpsc};
use tokio::task::{spawn_local, LocalSet};

/// Carries `t.Work`, answered never: a run that performs it rests, as a session's run rests on its next moment.
struct Rest;

impl Driver for Rest {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("rest")]
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        vec![("t.Work".into(), vec![])]
    }
    fn events(&self) -> Vec<String> {
        vec![]
    }
    fn carry(&mut self, _: &str, _: &[Value], _: u64) -> Reply {
        Reply::Later
    }
    fn dropped(&mut self, _: u64) {}
}

impl Waits for Rest {
    fn waits(&self) -> Vec<RawFd> {
        vec![]
    }
    fn ready(&mut self) -> Vec<Ready> {
        vec![]
    }
}

/// Vouches for every connection as the party it is set to now, or for none.
struct Now(Rc<RefCell<Option<&'static str>>>);

impl Vouch for Now {
    fn party(&self, _: &str) -> Option<Party> {
        self.0.borrow().map(|p| Party::of(p, Some(0)))
    }
}

struct NoTerminals;

impl Terminals for NoTerminals {
    fn open(&self, _: &Party, s: &str) -> Result<(mpsc::UnboundedSender<Vec<u8>>, broadcast::Receiver<Vec<u8>>), String> {
        Err(format!("no terminal {s}"))
    }
}

/// The client's log, shared with the test.
#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<u8>>>);

impl Write for Log {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Log {
    fn lines(&self, has: &str) -> usize {
        String::from_utf8_lossy(&self.0.lock().unwrap()).lines().filter(|l| l.contains(has)).count()
    }
}

#[tokio::test]
async fn it_asks_again_until_its_scope_is_vouched_for_then_holds_one_run_and_another_partys_act_is_refused() {
    LocalSet::new().run_until(async {
        let dir = std::env::temp_dir().join(format!("act-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let text = "module t\nEffect Work() -> Int\nAct Go(by: String) Returns Int Does { perform Work() }";
        let loaded = docket_v2::check::load(vec![docket_v2::syntax::read(text).unwrap()]).unwrap();
        let runs = docket_v2::run::machine::Machine::new(loaded, docket_v2::norm::Core::default(), vec![link(Rest).0]).unwrap();
        let (bind, texts): (Bind, Texts) = (Box::new(|_, _| (None, String::new())), Box::new(|_| Err("empty".into())));
        let who = Rc::new(RefCell::new(None));
        let engine = Engine::open(&dir.join("record.jsonl"), Box::new(Docket::new(runs, "t", vec![], bind, texts)), Box::new(Now(who.clone())), Box::new(NoTerminals)).unwrap();
        let sock = dir.join("s");
        spawn_local(engine.clone().serve(UnixListener::bind(&sock).unwrap()));
        let wait = async |log: &Log, has: &str, n: usize| {
            for _ in 0..200 {
                if log.lines(has) >= n {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            panic!("no {n} lines with {has:?}: {}", String::from_utf8_lossy(&log.0.lock().unwrap()));
        };
        // `ended` set: the scope has no other process; `unread` set: its processes cannot be read
        let start = |args: &'static [&'static str], log: &Log, ended: &Arc<AtomicBool>, unread: &Arc<AtomicBool>| {
            let (sock, mut log, ended, unread) = (sock.clone(), log.clone(), ended.clone(), unread.clone());
            let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            let alone = move || if unread.load(Ordering::SeqCst) { Err("no cgroup.procs".to_string()) } else { Ok(ended.load(Ordering::SeqCst)) };
            std::thread::spawn(move || coordd_v2::act::act(&sock, "t.Go", &args, &mut log, &alone))
        };
        let began = || std::fs::read_to_string(dir.join("record.jsonl")).unwrap().lines().filter(|l| l.contains("\"began\"")).count();

        // nothing vouches for its scope yet, nor are its processes read: refused, each said, asked again
        let (log, ended, unread) = (Log::default(), Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(true)));
        let client = start(&["w1"], &log, &ended, &unread);
        wait(&log, "not taken: nothing vouches for", 2).await;
        assert_eq!((log.lines("not read: no cgroup.procs") >= 2, log.lines("no other process"), began()), (true, 0, 0));
        unread.store(false, Ordering::SeqCst);
        *who.borrow_mut() = Some("w1");
        wait(&log, "lives; held", 1).await;
        assert_eq!(began(), 1, "one run, once its scope is vouched for");
        tokio::time::sleep(coordd_v2::act::EVERY * 2).await;
        assert_eq!((began(), log.lines("not taken")), (1, log.lines("not taken")), "held: asked no more");
        ended.store(true, Ordering::SeqCst);
        wait(&log, "the scope has no other process", 1).await;
        client.join().unwrap();

        // from w2's scope, an act as w1 is refused by the machine, and asked again while the scope lives
        *who.borrow_mut() = Some("w2");
        let (log, ended) = (Log::default(), Arc::new(AtomicBool::new(false)));
        let client = start(&["w1"], &log, &ended, &Arc::new(AtomicBool::new(false)));
        wait(&log, "not taken", 2).await;
        assert_eq!(log.lines("lives; held"), 0, "{}", String::from_utf8_lossy(&log.0.lock().unwrap()));
        ended.store(true, Ordering::SeqCst);
        wait(&log, "the scope has no other process", 1).await;
        client.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    })
    .await
}
