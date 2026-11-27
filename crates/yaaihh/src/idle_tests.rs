//! The window at rest (N-1933.p1: "no more thrashing CPU or memory on the laptop"): what its poll asks
//! when nothing comes, and how a stream of agent counters is taken.

use crate::{App, Sel};
use std::time::Duration;

/// The wakes TX was asked for, in seconds: rune keeps them to itself, and its `Debug` says them.
fn wakes(tx: &rune::Tx) -> Vec<f64> {
    let d = format!("{tx:?}");
    let list = d.split("wakes: [").nth(1).and_then(|r| r.split(']').next()).unwrap_or_default();
    list.split(", ").filter(|w| !w.is_empty()).map(secs).collect()
}

/// A `Duration`'s `Debug` ("10s", "80ms", "1.5µs") in seconds.
fn secs(w: &str) -> f64 {
    let (n, scale) = [("ns", 1e-9), ("µs", 1e-6), ("ms", 1e-3), ("s", 1.0)].into_iter().find_map(|(u, f)| w.strip_suffix(u).map(|n| (n, f))).unwrap();
    n.parse::<f64>().unwrap() * scale
}

#[test]
fn wakes_read_back_as_asked() {
    let mut tx = rune::Tx::default();
    for d in [Duration::from_secs(10), Duration::from_millis(80), Duration::from_micros(1500)] {
        tx.wake_after(d);
    }
    assert_eq!(wakes(&tx), [10.0, 0.08, 0.0015]);
}

/// With nothing coming on its pipes, no page asks the window to wake sooner than a second: an agent's chat, the
/// Agents page, the tickets, Home. Its state, changed by going there, is saved once it settles (state.rs `SETTLE`);
/// after that nothing is due.
#[test]
fn an_idle_window_asks_no_wake_sooner_than_a_second() {
    let (_held, mut s, _cat) = crate::view_cost::linked_app(&["dev", "qa", "orch"]);
    s.devices.reader = || None;
    let home = crate::home::sel(&s.projects[0]);
    for sel in [Sel::Agent("dev".into()), Sel::Agents, Sel::Tickets, home] {
        s.sel = sel;
        crate::poll(&mut s, 0.0, &mut rune::Tx::default());
        std::thread::sleep(Duration::from_millis(600));
        crate::poll(&mut s, 0.0, &mut rune::Tx::default());
        let mut asked = Vec::new();
        for i in 0..20 {
            let mut tx = rune::Tx::default();
            crate::poll(&mut s, i as f64 * 0.5, &mut tx);
            asked.extend(wakes(&tx));
        }
        assert!(asked.iter().all(|w| *w >= 1.0), "{:?} asked {asked:?}", s.sel);
    }
}

/// A working agent's counters come many times a second: each poll within `STREAM_EVERY` of the last batch takes none
/// and asks one wake for the rest of it, so the window builds for them at most 1 / `STREAM_EVERY` times a second.
#[test]
fn a_burst_of_counters_is_taken_once_per_stream_every() {
    let (_held, mut s, _cat) = crate::view_cost::linked_app(&["dev"]);
    s.sel = Sel::Agent("dev".into());
    let burst = |s: &mut App, from: u64| {
        for i in 0..50 {
            let mut a = s.projects[0].live.agents[0].clone();
            a.mem_mb = Some(from + i);
            s.projects[0].inbox.push(crate::link::Incoming::Event(coord_types::Event { seq: i, body: coord_types::Body::Agent(a) }));
        }
    };
    burst(&mut s, 400);
    crate::poll(&mut s, 0.0, &mut rune::Tx::default());
    assert!(s.projects[0].inbox.is_empty(), "the first batch taken at once");
    assert_eq!(s.projects[0].live.agents[0].mem_mb, Some(449));
    burst(&mut s, 500);
    let mut tx = rune::Tx::default();
    crate::poll(&mut s, 0.01, &mut tx);
    assert_eq!((s.projects[0].inbox.len(), s.projects[0].live.agents[0].mem_mb), (50, Some(449)), "held for the next batch");
    assert!(wakes(&tx).iter().any(|w| *w > 0.0 && *w <= crate::pace::STREAM_EVERY.as_secs_f64()), "one wake for the batch: {:?}", wakes(&tx));
    std::thread::sleep(crate::pace::STREAM_EVERY);
    crate::poll(&mut s, 0.1, &mut rune::Tx::default());
    assert!(s.projects[0].inbox.is_empty(), "taken after STREAM_EVERY");
    assert_eq!(s.projects[0].live.agents[0].mem_mb, Some(549), "the second batch taken, its last counter standing");
}

/// N-1933.p9: a terminal's output came as fast as it was written, each chunk a wake and a build. A burst is taken at
/// once after a quiet stretch; the next within `PTY_EVERY` is held, with one wake for the rest of it, and taken then;
/// an agent's message meanwhile is not held behind the terminal's.
#[test]
fn a_burst_of_terminal_output_is_taken_once_per_pty_every() {
    use crate::pace::PTY_EVERY;
    let (_held, mut s, _cat) = crate::view_cost::linked_app(&["dev"]);
    s.sel = Sel::Agent("dev".into());
    let burst = |s: &mut App| {
        for _ in 0..50 {
            s.projects[0].inbox.push(crate::link::Incoming::Pty(coord_types::PtyEvent::Output { name: "dev".into(), data: b"x".to_vec() }));
        }
    };
    let ptys = |s: &App| s.projects[0].inbox.iter().filter(|m| matches!(m, crate::link::Incoming::Pty(_))).count();
    burst(&mut s);
    crate::poll(&mut s, 0.0, &mut rune::Tx::default());
    assert_eq!(ptys(&s), 0, "the first burst taken at once");
    burst(&mut s);
    let mut a = s.projects[0].live.agents[0].clone();
    a.mem_mb = Some(700);
    s.projects[0].inbox.push(crate::link::Incoming::Event(coord_types::Event { seq: 1, body: coord_types::Body::Agent(a) }));
    let mut tx = rune::Tx::default();
    crate::poll(&mut s, 0.01, &mut tx);
    assert_eq!(ptys(&s), 50, "held for the next batch");
    assert_eq!(s.projects[0].live.agents[0].mem_mb, Some(700), "the agent's message taken in its own batch");
    assert!(wakes(&tx).iter().any(|w| *w > 0.0 && *w <= PTY_EVERY.as_secs_f64()), "one wake for the batch: {:?}", wakes(&tx));
    std::thread::sleep(PTY_EVERY);
    crate::poll(&mut s, 0.05, &mut rune::Tx::default());
    assert_eq!(ptys(&s), 0, "taken after PTY_EVERY");
}
