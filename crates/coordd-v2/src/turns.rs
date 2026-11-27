//! The turns driver's daemon side: words typed into an agent's coord-pty (an agy party's given to agy-rpc), its
//! hang-up, and the frames' hooks it follows.

use coord_types::pty;
use crate::start::{Copy, STARTED};
use docket_driver_turns::{Config, Daemon, Turns};
use docket_hooks::{Frame, Native};
use docket_v2::run::value::Value;
use std::cell::RefCell;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::Command;
use std::rc::Rc;

/// One frame of kind `kind` to `party`'s terminal under `coord` (spawn's coord-pty COORD).
fn send(coord: &str, party: &Value, kind: u8, payload: &[u8]) -> Result<(), String> {
    let name = crate::peer::written(std::slice::from_ref(party));
    let sock = pty::socket(coord, &name);
    UnixStream::connect(&sock).and_then(|mut s| s.write_all(&pty::frame(kind, payload))).map_err(|e| format!("{name}'s terminal {}: {e}", sock.display()))
}

/// The harness `party` was started with, as its start kept it in its home under `homes` ([`STARTED`]).
fn harness_of(homes: &str, party: &Value) -> Option<String> {
    let name = crate::peer::written(std::slice::from_ref(party));
    let kept: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(Path::new(homes).join(name).join(STARTED)).ok()?).ok()?;
    kept["harness"].as_str().map(str::to_string)
}

/// Words for agy party `party` through `agy-rpc send`, into its running turn or as a new one (`next`), to the session
/// agy-rpc serves from its home under `homes` (spawn-v2's `HOME/agy/coord`): its terminal shows agy-rpc's screen.
fn agy_send(homes: &str, rpc: &str, party: &Value, text: &str) -> Result<(), String> {
    let name = crate::peer::written(std::slice::from_ref(party));
    let c = Path::new(homes).join(&name).join("agy").join("coord");
    let sid = std::fs::read_to_string(c.join("session")).ok().filter(|s| !s.trim().is_empty()).ok_or(format!("agy-rpc serves no session of {name} yet ({})", c.display()))?;
    if rpc.is_empty() {
        return Err(format!("{name} runs agy, and the turns file names no agy-rpc to give it words"));
    }
    match Command::new(rpc).args(["send", sid.trim(), "next", text]).env("CODEX_SOCK", &c).output() {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(format!("{name}'s agy-rpc send ended {}: {}", o.status, String::from_utf8_lossy(&o.stderr).trim())),
        Err(e) => Err(format!("{name}'s agy-rpc {rpc}: {e}")),
    }
}

/// The daemon's calls on the agents' terminals under `c.terminals`: words given to be pasted in and entered once the
/// program reads a paste (`pty::PASTE`), an agy party's (by the harness its start kept) through agy-rpc; a hang-up; and the copy of a session, by the
/// session driver once it is linked (`copy`).
pub fn daemon(c: &Config, copy: Rc<RefCell<Option<Copy>>>) -> Daemon {
    let (coord, end, homes, rpc) = (c.terminals.clone(), c.terminals.clone(), c.homes.clone(), c.agy_rpc.clone());
    Daemon {
        // how a party is handed words is its harness's: agy's through agy-rpc, every other's pasted in its terminal
        type_in: Box::new(move |p, text| match harness_of(&homes, p) {
            Some(h) if h == docket_driver_agy::HARNESS => agy_send(&homes, &rpc, p, text),
            _ => send(&coord, p, pty::PASTE, text.as_bytes()),
        }),
        fork: Box::new(move |p, transcript| match copy.borrow_mut().as_mut() {
            Some(c) => c(p, transcript),
            None => Err(format!("no copy of {p}'s session: no session driver is linked to start one")),
        }),
        end: Box::new(move |p| send(&end, p, pty::HANGUP, &[])),
    }
}

/// `frame`, its hooks fed to `turns` when the turns driver is linked.
pub fn watched<N: Native>(mut frame: Frame<N>, turns: &Option<Rc<RefCell<Turns>>>, harness: &'static str) -> Frame<N> {
    if let Some(t) = turns {
        watch(&mut frame, t.clone(), harness);
    }
    frame
}

/// `frame`'s hooks, for an agent of `harness`, fed to `turns`: the transcript path a hook gives is followed and its
/// turn's start marked as the hook arrives; its turn's end once the answer sent lets the turn end (not a KeepGoing).
fn watch<N: Native>(frame: &mut Frame<N>, turns: Rc<RefCell<Turns>>, harness: &'static str) {
    let ends = turns.clone();
    frame.seen = Some(Box::new(move |agent, moment, input| {
        let party = Value::text(agent);
        let path = serde_json::from_str::<serde_json::Value>(input).ok().and_then(|v| v["transcript_path"].as_str().map(str::to_string));
        let mut t = turns.borrow_mut();
        let done = path.map_or(Ok(()), |p| t.follow(&party, harness, &p)).and_then(|()| if moment == "turn_begins" { t.turn_began(&party) } else { Ok(()) });
        if let Err(why) = done {
            eprintln!("coordd-v2: {agent}'s {moment}: {why}");
        }
    }));
    frame.answered = Some(Box::new(move |agent, moment, goes_on| {
        if moment == "turn_end" && !goes_on {
            if let Err(why) = ends.borrow_mut().turn_ended(&Value::text(agent)) {
                eprintln!("coordd-v2: {agent}'s turn end: {why}");
            }
        }
    }));
}
