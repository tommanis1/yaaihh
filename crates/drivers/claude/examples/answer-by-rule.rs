//! The Claude Code driver with a fixed rule in place of an engine, to hold it to the real Claude Code.
//!
//! answer-by-rule settings RELAY: prints the hooks block of Claude Code's settings, each hook running RELAY.
//! answer-by-rule DIR AGENT LOG [MOMENT=Action:TEXT]...: opens AGENT's socket in DIR (its path on stdout), and answers
//! each moment reached with the rule's action for it, Allow for any other, one JSON line per moment in LOG. It ends
//! when DIR/stop exists.

use docket_driver_claude::{hooks_settings, Claude};
use docket_hooks::frame::Frame;
use docket_hooks::Action;
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::value::Value;
use std::io::Write;
use std::time::Duration;

fn action(spec: &str) -> Action {
    let (name, text) = spec.split_once(':').unwrap_or((spec, ""));
    let t = text.to_string();
    match name {
        "Deny" => Action::Deny(t),
        "Modify" => Action::Modify(t),
        "Inject" => Action::Inject(t),
        "Answer" => Action::Answer(t),
        "KeepGoing" => Action::KeepGoing(t),
        "Suspend" => Action::Suspend(t),
        _ => Action::Allow,
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.get(1).map(String::as_str) == Some("settings") {
        println!("{}", hooks_settings(&a[2]));
        return;
    }
    let (dir, agent, log) = (std::path::PathBuf::from(&a[1]), a[2].clone(), &a[3]);
    let rules: Vec<(String, Action)> = a[4..].iter().filter_map(|r| r.split_once('=')).map(|(m, s)| (m.to_string(), action(s))).collect();
    let mut f = Frame::new(Claude);
    let sock = f.open(&agent, &dir).expect("its socket");
    println!("{}", sock.display());
    let mut log = std::fs::File::create(log).expect("the log");
    let who = Value::text(&agent);
    let mut line = 0u64;
    while !dir.join("stop").exists() {
        line += 1;
        let mut got = f.carry("std.hooks.Next-Moment", &[who.clone()], line);
        // a daemon waits on f.waits(); this stand-in looks every 20 ms
        while got == Reply::Later && !dir.join("stop").exists() {
            std::thread::sleep(Duration::from_millis(20));
            // one Next-Moment rests at a time here, so whatever is ready is its answer
            for (_, r) in f.ready() {
                got = r;
            }
        }
        let Reply::Answer { value: Value::Record { fields, .. }, .. } = got else { continue };
        let (moment, payload) = (fields[0].1.clone(), fields[1].1.to_string());
        let m = match &moment {
            Value::Inst(i) => i.args[0].to_string().trim_matches('"').to_string(),
            v => v.to_string(),
        };
        let act = rules.iter().find(|(r, _)| *r == m).map(|(_, a)| a.clone()).unwrap_or(Action::Allow);
        let applied = f.carry("std.hooks.Apply", &[who.clone(), moment, act.value()], line);
        writeln!(log, "{}", serde_json::json!({"moment": m, "action": format!("{act:?}"), "applied": format!("{applied:?}"), "payload_bytes": payload.len()})).unwrap();
    }
    f.close(&agent);
}
