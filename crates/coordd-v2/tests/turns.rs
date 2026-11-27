//! The turns driver in the daemon on the std texts: an agent's hooks reach it through its harness's frame, its turn ends when the answer lets it, and its turn's passages and usage are record lines.

use coordd_v2::docket::Docket;
use coordd_v2::link::{link, linked, wait_on};
use coordd_v2::peer::{Party, Vouch};
use coordd_v2::serve::{Engine, Terminals};
use docket_hooks::{wire, Frame};
use docket_v2::check::names::Sym;
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::value::Value;
use serde_json::{json, Value as Json};
use std::cell::RefCell;
use std::io::{BufRead, Write};
use std::rc::Rc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};
use tokio::task::{spawn_local, LocalSet};

/// Carries every other Effect of the texts and fails each: this test reaches none of them.
struct Rest(Vec<String>);

impl Driver for Rest {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("rest")]
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        self.0.iter().map(|e| (e.clone(), vec![])).collect()
    }
    fn events(&self) -> Vec<String> {
        vec![]
    }
    fn carry(&mut self, op: &str, _: &[Value], _: u64) -> Reply {
        Reply::Failed(format!("{op} is not reached in this test"))
    }
    fn dropped(&mut self, _: u64) {}
}

struct Anyone;

impl Vouch for Anyone {
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

/// A hook of `w1`'s Claude Code as its relay sends it, waiting for no answer.
fn hook(sock: &std::path::Path, name: &str, input: Json) -> std::os::unix::net::UnixStream {
    let mut c = std::os::unix::net::UnixStream::connect(sock).unwrap();
    writeln!(c, "{}", wire::ask(name, &input.to_string())).unwrap();
    c
}

#[tokio::test]
async fn a_claude_turns_passages_and_usage_reach_the_record_from_its_hooks() {
    LocalSet::new().run_until(async {
        let dir = std::env::temp_dir().join(format!("turns-daemon-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let std_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docket/std");
        let texts = ["hooks", "comms", "budgets", "control"].map(|m| docket_v2::syntax::read(&std::fs::read_to_string(std_dir.join(format!("{m}.docket"))).unwrap()).unwrap());
        let loaded = docket_v2::check::load(texts.to_vec()).unwrap();
        let turns = Rc::new(RefCell::new(docket_driver_turns::Turns::new(coordd_v2::turns::daemon(&docket_driver_turns::Config { terminals: dir.display().to_string(), ..Default::default() }, Default::default()), Default::default()).unwrap()));
        let frame = coordd_v2::turns::watched(Frame::new(docket_driver_claude::Claude), &Some(turns.clone()), docket_driver_claude::HARNESS);
        let (frame_d, frame) = link(frame);
        let turns_d = linked(turns.clone());
        let carried: Vec<String> = [&frame_d, &turns_d].iter().flat_map(|d| d.effects().into_iter().map(|(e, _)| e)).collect();
        let rest = loaded.syms.iter().filter(|(n, s)| matches!(s, Sym::Effect { .. }) && !carried.contains(n)).map(|(n, _)| n.clone()).collect();
        let runs = docket_v2::run::machine::Machine::new(loaded, docket_v2::norm::Core::default(), vec![frame_d, turns_d, Box::new(Rest(rest))]).unwrap();
        let docket = Docket::new(runs, "std.control", vec![], Box::new(|_, _| (None, String::new())), Box::new(|_| Err("starts empty".into())));
        let engine = Engine::open(&dir.join("record.jsonl"), Box::new(docket), Box::new(Anyone), Box::new(NoTerminals)).unwrap();
        let sock = dir.join("s");
        spawn_local(engine.clone().serve(UnixListener::bind(&sock).unwrap()));
        spawn_local(wait_on(engine.clone(), frame.clone(), 0));
        spawn_local(wait_on(engine.clone(), turns.clone(), 1));
        let hooks = frame.borrow_mut().open("w1", &dir).unwrap();
        let transcript = dir.join("w1.jsonl");
        std::fs::write(&transcript, "").unwrap();
        let path = transcript.display().to_string();
        // w1's terminal, as coord-pty serves it under the turns driver's folder
        std::fs::create_dir_all(dir.join("pty")).unwrap();
        let terminal = std::os::unix::net::UnixListener::bind(coord_types::pty::socket(&dir.display().to_string(), "w1")).unwrap();
        terminal.set_nonblocking(true).unwrap();
        let typed = || {
            let mut all = Vec::new();
            while let Ok((mut c, _)) = terminal.accept() {
                c.set_nonblocking(false).unwrap();
                std::io::Read::read_to_end(&mut c, &mut all).unwrap();
            }
            String::from_utf8_lossy(&all).into_owned()
        };
        // the program's side: the next moment of w1 taken and answered with ACTION
        let answer = |action: Value| {
            let mut f = frame.borrow_mut();
            let Reply::Answer { value: Value::Record { fields, .. }, .. } = f.carry("std.hooks.Next-Moment", &[Value::text("w1")], 900) else { panic!("a moment waits") };
            let moment = docket_v2::run::value::Value::to_string(&fields[0].1);
            assert!(matches!(f.carry("std.hooks.Apply", &[Value::text("w1"), fields[0].1.clone(), action], 901), Reply::Answer { .. }));
            moment
        };
        // a HookAction as the engine names it, by its module (std.hooks.Allow)
        let ctor = |n: &str, t: &str| Value::ctor(&format!("std.hooks.{n}"), if t.is_empty() { vec![] } else { vec![Value::text(t)] });
        let _begun = hook(&hooks, "UserPromptSubmit", json!({"transcript_path": path, "prompt": "look"}));
        tokio::time::sleep(Duration::from_millis(200)).await;
        answer(ctor("Allow", ""));
        // in its turn: a word and a notice are held
        assert!(matches!(turns.borrow_mut().carry(docket_driver_turns::DELIVER, &[Value::text("him"), Value::text("w1"), Value::text("look again")], 902), Reply::Later));
        assert!(matches!(turns.borrow_mut().carry(docket_driver_turns::NOTIFY, &[Value::text("w1"), Value::text("refused: too long")], 903), Reply::Later));
        let said = json!({"type": "assistant", "timestamp": "2026-11-26T14:00:00Z", "message": {"id": "m1", "content": [{"type": "text", "text": "the plan holds"}], "usage": {"input_tokens": 3, "output_tokens": 5}}});
        std::fs::OpenOptions::new().append(true).open(&transcript).unwrap().write_all(format!("{said}\n").as_bytes()).unwrap();
        let _stop = hook(&hooks, "Stop", json!({"transcript_path": path}));
        tokio::time::sleep(Duration::from_millis(200)).await;
        answer(ctor("KeepGoing", "check the tests too"));
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(typed(), "", "a turn kept going takes no held words");
        assert!(turns.borrow_mut().ready().0.is_empty(), "and answers nothing");
        let _ended = hook(&hooks, "Stop", json!({"transcript_path": path}));
        tokio::time::sleep(Duration::from_millis(200)).await;
        answer(ctor("Allow", ""));
        let words = typed();
        assert!(words.contains("look again") && !words.contains("refused"), "the turn's end types the oldest held words alone: {words:?}");
        let (rd, mut wr) = UnixStream::connect(&sock).await.unwrap().into_split();
        let mut rd = BufReader::new(rd).lines();
        wr.write_all(b"{\"do\":\"watch\",\"from\":0}\n").await.unwrap();
        let (mut performed, mut seen) = (Vec::new(), Vec::new());
        while performed.len() < 3 {
            let Ok(next) = tokio::time::timeout(Duration::from_secs(5), rd.next_line()).await else { panic!("the turn's lines; seen: {seen:?}") };
            let l: Json = serde_json::from_str(&next.unwrap().unwrap()).unwrap();
            seen.push(l.clone());
            if let Some(p) = l["line"].get("performed") {
                performed.push(p["act"].clone());
            }
        }
        assert_eq!(performed[0]["inst"], "std.comms.Spoke", "{performed:?}");
        assert_eq!(performed[0]["args"][0], "w1");
        let node = performed[0]["args"][1].as_str().unwrap().to_string();
        assert!(node.starts_with(&format!("t:{path}:0:")), "{node}");
        let usage: Vec<(Json, Json)> = performed[1..].iter().map(|u| (u["args"][2].clone(), u["args"][3].clone())).collect();
        assert_eq!(usage, [(json!("input"), json!(3)), (json!("output"), json!(5))], "{performed:?}");
        assert!(std::io::BufReader::new(std::fs::File::open(dir.join("record.jsonl")).unwrap()).lines().count() >= 4);
        let _ = std::fs::remove_dir_all(&dir);
    })
    .await
}

/// How a party is handed words is chosen by the harness its start kept: an agy party's terminal shows agy-rpc's screen
/// and takes none, so they go through `agy-rpc send` to the session agy-rpc serves from its home; any other party's
/// are pasted in its terminal, an agy session folder in its home or not.
#[test]
fn an_agy_partys_words_go_through_agy_rpc_send_and_anyone_elses_into_its_terminal() {
    let dir = std::env::temp_dir().join(format!("turns-agy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let homes = dir.join("homes");
    let started = |party: &str, harness: &str| {
        let c = homes.join(party).join("agy/coord");
        std::fs::create_dir_all(&c).unwrap();
        std::fs::write(homes.join(party).join(coordd_v2::start::STARTED), json!({"harness": harness, "model": "m"}).to_string()).unwrap();
        c
    };
    let c = started("g1", "agy");
    std::fs::write(c.join("session"), "s-1\n").unwrap();
    std::fs::write(started("w1", "claude-code").join("session"), "s-9\n").unwrap();
    started("g2", "agy");
    let rpc = dir.join("agy-rpc");
    let said = dir.join("said");
    std::fs::write(&rpc, format!("#!/bin/sh\necho \"$CODEX_SOCK $*\" >>{}\n[ \"$4\" != fails ] || {{ echo no >&2; exit 2; }}\n", said.display())).unwrap();
    std::fs::set_permissions(&rpc, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    std::fs::create_dir_all(dir.join("pty")).unwrap();
    let terminal = std::os::unix::net::UnixListener::bind(coord_types::pty::socket(&dir.display().to_string(), "w1")).unwrap();
    let config = docket_driver_turns::Config { terminals: dir.display().to_string(), homes: homes.display().to_string(), agy_rpc: rpc.display().to_string(), ..Default::default() };
    let mut d = coordd_v2::turns::daemon(&config, Default::default());
    (d.type_in)(&Value::text("g1"), "look at the plan").unwrap();
    assert_eq!(std::fs::read_to_string(&said).unwrap(), format!("{} send s-1 next look at the plan\n", c.display()));
    let Err(why) = (d.type_in)(&Value::text("g1"), "fails") else { panic!("a send that fails is said") };
    assert!(why.contains("g1's agy-rpc send ended") && why.contains("no"), "{why}");
    let Err(why) = (d.type_in)(&Value::text("g2"), "hello") else { panic!("an agy party with no session yet") };
    assert!(why.contains("agy-rpc serves no session of g2 yet"), "{why}");
    (d.type_in)(&Value::text("w1"), "hello").unwrap();
    let (mut s, _) = terminal.accept().unwrap();
    let mut got = Vec::new();
    std::io::Read::read_to_end(&mut s, &mut got).unwrap();
    assert_eq!(got, coord_types::pty::frame(coord_types::pty::PASTE, b"hello"), "Claude Code: its terminal, though an agy session folder is in its home");
    assert_eq!(std::fs::read_to_string(&said).unwrap().lines().count(), 2, "nothing more to agy-rpc");
}
