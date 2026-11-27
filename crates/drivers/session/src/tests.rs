//! A start carried through to the spawn, and the session's Events it performs.

use super::*;

fn args(agent: Value, boxed: bool, check: bool, model: &str) -> [Value; 5] {
    [agent, Value::Bool(boxed), Value::Bool(false), Value::Bool(check), Value::text(model)]
}

fn agy_profile(mode: Option<&str>) -> Profile {
    Profile { harness: "agy".into(), version: "t3code-0.1.11".into(), mode: mode.map(str::to_string), prompt: Some(("/a/p.md".into(), Prompt::Append)), ..Default::default() }
}

#[test]
fn a_start_spawns_the_harness_and_states_what_it_runs_on_and_its_mode() {
    let agent = Value::text("a1");
    let mut spawned = Vec::new();
    let mut s = Session { relay: "/r/relay".into(), profile: |_: &Value| Some(agy_profile(Some("approval-required"))), spawn: |a: &Value, p: &Profile, l: Launch| Ok(spawned.push((a.clone(), p.harness.clone(), l.env))) };
    let Reply::Answer { value, events, .. } = s.carry(START, &args(agent.clone(), false, true, ""), 7) else { panic!() };
    assert_eq!(value, Value::Unit);
    assert_eq!(events.iter().map(|e| e.ty.as_str()).collect::<Vec<_>>(), [STARTED, MODE]);
    assert_eq!(events[0].args, [agent.clone(), Value::text("agy"), Value::text("t3code-0.1.11")]);
    assert_eq!(events[1].args, [agent.clone(), Value::text("approval-required")]);
    drop(s);
    let env = &spawned[0].2;
    assert!(env.contains(&("AGY_STANDING".into(), "/a/p.md".into())) && env.contains(&("DOCKET_HOOK_RELAY".into(), "/r/relay".into())), "{env:?}");
}

#[test]
fn no_mode_no_mode_line_and_what_cannot_start_fails_the_effect() {
    let agent = Value::text("a1");
    let mut s = Session { relay: "/r".into(), profile: |_: &Value| Some(agy_profile(None)), spawn: |_: &Value, _: &Profile, _: Launch| Ok(()) };
    let Reply::Answer { events, .. } = s.carry(START, &args(agent.clone(), false, true, ""), 1) else { panic!() };
    assert_eq!(events.len(), 1, "a session that names no mode states none");
    let mut none = Session { relay: "/r".into(), profile: |_: &Value| None, spawn: |_: &Value, _: &Profile, _: Launch| Ok(()) };
    assert!(matches!(none.carry(START, &args(agent.clone(), false, true, ""), 1), Reply::Failed(w) if w.starts_with("no profile")));
    let mut refused = Session { relay: "/r".into(), profile: |_: &Value| Some(Profile { prompt: Some(("/a/p.md".into(), Prompt::Replace)), ..agy_profile(None) }), spawn: |_: &Value, _: &Profile, _: Launch| Ok(()) };
    assert!(matches!(refused.carry(START, &args(agent.clone(), false, true, ""), 1), Reply::Failed(w) if w.contains("no replace")), "agy has no replace");
    let mut down = Session { relay: "/r".into(), profile: |_: &Value| Some(agy_profile(None)), spawn: |_: &Value, _: &Profile, _: Launch| Err("its box did not start".into()) };
    assert_eq!(down.carry(START, &args(agent.clone(), false, true, ""), 1), Reply::Failed("its box did not start".into()));
}

#[test]
fn claude_code_and_codex_get_their_hooks_files_and_each_harness_the_relay() {
    let p = Profile { harness: "codex".into(), version: "0.160.0".into(), ..Default::default() };
    let l = launch(&p, "/r/relay").unwrap();
    assert_eq!(l.files[0].0, "hooks.json");
    assert!(l.files[0].1.contains("/r/relay PreToolUse"), "{}", l.files[0].1);
    let c = launch(&Profile { harness: "claude-code".into(), ..p.clone() }, "/r/relay").unwrap();
    assert_eq!(c.files[0].0, "settings.json", "read from its config folder");
    assert!(c.files[0].1.contains("/r/relay PreToolUse"), "{}", c.files[0].1);
    assert!(launch(&Profile { harness: "opencode".into(), ..p.clone() }, "/r/relay").unwrap().env.contains(&("DOCKET_HOOK_RELAY".into(), "/r/relay".into())));
    assert!(launch(&Profile { harness: "other".into(), ..p.clone() }, "/r").is_err());
}

#[test]
fn a_tool_asked_off_that_would_stay_on_fails_the_start_before_any_spawn() {
    let codex = Profile { harness: "codex".into(), switches: Switches { tools_off: vec!["exec_command".into()], ..Default::default() }, ..Default::default() };
    let mut spawned = 0;
    let mut s = Session { relay: "/r".into(), profile: |_: &Value| Some(codex.clone()), spawn: |_: &Value, _: &Profile, _: Launch| Ok(spawned += 1) };
    assert!(matches!(s.carry(START, &args(Value::text("a1"), false, true, ""), 1), Reply::Failed(w) if w.starts_with("tool exec_command: Codex 0.160.0 has no switch")));
    drop(s);
    assert_eq!(spawned, 0);
    let agy = Profile { switches: Switches { tools_off: vec!["web_search".into()], ..Default::default() }, ..agy_profile(None) };
    assert_eq!(launch(&agy, "/r"), Err("tool web_search: agy takes no tool switch".into()));
    let told = Profile { switches: Switches { parts_off: vec!["x".into()], ..Default::default() }, ..agy_profile(None) };
    assert_eq!(launch(&told, "/r").unwrap().not_taken.len(), 2, "switches that are no tool (the part, a check agy lacks) are told, and the start made");
}

#[test]
fn the_program_decides_the_box_the_check_and_the_model_whatever_the_profile_says() {
    let agent = Value::text("a1");
    let claude = Profile { harness: "claude-code".into(), version: "2.1.332".into(), switches: Switches { safety_check: true, ..Default::default() }, model: "from-the-file".into(), ..Default::default() };
    let mut seen = Vec::new();
    let mut s = Session { relay: "/r".into(), profile: |_: &Value| Some(claude.clone()), spawn: |_: &Value, p: &Profile, l: Launch| Ok(seen.push((p.model.clone(), l.env))) };
    assert!(matches!(s.carry(START, &args(agent.clone(), false, false, "m-1"), 1), Reply::Answer { .. }));
    assert!(matches!(s.carry(START, &args(agent.clone(), false, true, ""), 2), Reply::Answer { .. }));
    assert!(matches!(s.carry(START, &args(agent.clone(), true, true, "m-1"), 3), Reply::Failed(w) if w.contains("no box yet")));
    assert!(matches!(s.carry(START, &[agent.clone()], 4), Reply::Failed(w) if w.contains("not carried here as given")));
    drop(s);
    let lifted = |env: &Vec<(String, String)>| env.iter().any(|(k, _)| k == "CLAUDE_CODE_EVEN_MORE_DANGEROUSLY_SKIP_PERMISSIONS");
    assert_eq!(seen.len(), 2, "a boxed start never reaches the spawn");
    assert_eq!((seen[0].0.as_str(), lifted(&seen[0].1)), ("m-1", true), "its check off as the program said");
    assert_eq!((seen[1].0.as_str(), lifted(&seen[1].1)), ("", false), "on as the program said; no model: the harness's own");
}

/// Safe, Guarded and Yolo as the program passes them (unasked, check): Claude Code asks first; runs commands
/// unasked with its check kept; runs everything with its check lifted.
#[test]
fn each_permission_starts_claude_code_with_its_switches() {
    let claude = Profile { harness: "claude-code".into(), version: "2.1.332".into(), ..Default::default() };
    let mut seen = Vec::new();
    let mut s = Session { relay: "/r".into(), profile: |_: &Value| Some(claude.clone()), spawn: |_: &Value, _: &Profile, l: Launch| Ok(seen.push(l)) };
    for (unasked, check) in [(false, true), (true, true), (true, false)] {
        let a = [Value::text("a1"), Value::Bool(false), Value::Bool(unasked), Value::Bool(check), Value::text("")];
        assert!(matches!(s.carry(START, &a, 1), Reply::Answer { .. }));
    }
    drop(s);
    let got: Vec<(bool, bool)> = seen.iter().map(|l| (l.args.iter().any(|a| a == "--dangerously-skip-permissions"), l.env.iter().any(|(k, _)| k == "CLAUDE_CODE_EVEN_MORE_DANGEROUSLY_SKIP_PERMISSIONS"))).collect();
    assert_eq!(got, [(false, false), (true, false), (true, true)], "Safe, Guarded, Yolo");
}
