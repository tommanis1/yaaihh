//! A start that cannot read the standing text, through the daemon (docket-core N-1895 p325): production's spawn-v2.sh
//! refuses it, and its reason is in the record the window watches.

use coordd_v2::docket::{Bind, Docket, Texts};
use coordd_v2::link::Sessions;
use coordd_v2::peer::{Party, Table, Vouch};
use coordd_v2::serve::{Engine, Terminals};
use coordd_v2::start::{profiles, session, Places};
use docket_hooks::Frame;
use docket_v2::run::driver::Driver;
use serde_json::{json, Value as Json};
use std::cell::RefCell;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};
use tokio::task::{spawn_local, LocalSet};

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

fn exe(p: &Path, text: &str) {
    std::fs::write(p, text).unwrap();
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[tokio::test]
async fn a_start_with_no_standing_text_is_refused_and_its_reason_reaches_the_record_the_window_watches() {
    LocalSet::new()
        .run_until(async {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let dir = std::env::temp_dir().join(format!("start-refused-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            // a release whose roster gives no standing text, its route.sh production's; the coord folder empty
            let ab = dir.join("r/agent-base");
            for d in ["bin", "lib"].map(|d| ab.join(d)).into_iter().chain([dir.join("c"), dir.join("w"), dir.join("hooks")]) {
                std::fs::create_dir_all(d).unwrap();
            }
            std::fs::copy(root.join("agent-base/lib/route.sh"), ab.join("lib/route.sh")).unwrap();
            exe(&ab.join("bin/roster"), "#!/bin/sh\necho 'roster: standing.md: not there' >&2\nexit 1\n");
            let spawn = dir.join("spawn");
            exe(&spawn, &format!("#!/bin/sh\nSPAWN_COORD={} SPAWN_PROGS={} exec {} \"$@\"\n", dir.join("c").display(), dir.join("r").display(), root.join("ops/release/spawn-v2.sh").display()));

            let ps = profiles(&json!({"w1": {"harness": "codex", "version": "1", "folder": dir.join("w")}}).to_string()).unwrap();
            let codex: Rc<RefCell<dyn Sessions>> = Rc::new(RefCell::new(Frame::new(docket_driver_codex::Codex)));
            let places = Places { hooks: dir.join("hooks"), homes: dir.join("homes"), spawn, carrier: 0 };
            let (s, _) = session("/relay".into(), ps, vec![("codex/1".into(), codex)], Rc::new(Table::read("", &[]).unwrap()), places, None);
            let hooks = std::fs::read_to_string(root.join("docket/std/hooks.docket")).unwrap();
            let go = "module t\nimport std.hooks as H\nAct Go(agent: H.Party) Returns Unit Does { perform H.Start-Session(agent, false, false, true, \"m-1\") }";
            let loaded = coordd_v2::texts::load(&[("std/hooks.docket".into(), hooks), ("t.docket".into(), go.into())]).unwrap();
            let drivers: Vec<Box<dyn Driver>> = vec![Box::new(s)];
            let runs = docket_v2::run::machine::Machine::new(loaded, docket_v2::norm::Core::default(), drivers).unwrap();
            let (bind, texts): (Bind, Texts) = (Box::new(|_, _| (None, String::new())), Box::new(|_| Err("this record starts empty".into())));
            let engine = Engine::open(&dir.join("record.jsonl"), Box::new(Docket::new(runs, "t", vec![], bind, texts)), Box::new(W1), Box::new(NoTerminals)).unwrap();
            let sock = dir.join("s");
            spawn_local(engine.clone().serve(UnixListener::bind(&sock).unwrap()));

            let (rd, mut wr) = UnixStream::connect(&sock).await.unwrap().into_split();
            let mut rd = BufReader::new(rd).lines();
            let mut next = async || -> Json { serde_json::from_str(&tokio::time::timeout(Duration::from_secs(20), rd.next_line()).await.unwrap().unwrap().unwrap()).unwrap() };
            wr.write_all(format!("{}\n", json!({"do": "act", "name": "t.Go", "args": ["w1"]})).as_bytes()).await.unwrap();
            let r = next().await;
            assert_eq!(r["is"], "done", "{r}");
            // what the window's link asks: the record from its first line on
            wr.write_all(format!("{}\n", json!({"do": "watch", "from": 0})).as_bytes()).await.unwrap();
            let why = "w1: roster gave no standing text: not started, since it would run without the standing rules";
            let mut seen = Vec::new();
            let raised = loop {
                let l = next().await;
                seen.push(l.to_string());
                if l.to_string().contains("\"raised\"") {
                    break l;
                }
                assert!(seen.len() < 50, "no raised line: {seen:#?}");
            };
            assert!(raised.to_string().contains(why), "the reason in the raised line: {raised}");
            assert!(!dir.join("homes/w1/system-prompt.md").exists() && !dir.join("hooks/w1.hooks").exists(), "nothing started, its socket closed");
            let _ = std::fs::remove_dir_all(&dir);
        })
        .await;
}
