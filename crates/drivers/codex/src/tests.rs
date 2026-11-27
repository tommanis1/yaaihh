//! The driver held to its bridge's text, its answers to the shapes Codex 0.160.0 reads.

use super::*;
use docket_hooks::frame::FileChange;
use docket_hooks::bridge::{action_needs, facts, power};

fn bridge() -> String {
    let p = format!("{}/../../../docket/std/bridges/codex.docket", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

#[test]
fn its_hooks_are_its_bridges_native_hooks_and_every_line_is_for_its_version() {
    let b = bridge();
    let mut said: Vec<(String, String)> = facts(&b, "H.Native-Hook").unwrap().into_iter().map(|f| (f[1].clone(), f[2].clone())).collect();
    let mut ours: Vec<(String, String)> = HOOKS.iter().map(|(n, m)| (n.to_string(), m.to_string())).collect();
    said.sort();
    ours.sort();
    assert_eq!(ours, said);
    let mut reached: Vec<String> = facts(&b, "H.Reaches").unwrap().into_iter().map(|f| f[2].clone()).collect();
    let mut moments: Vec<String> = HOOKS.iter().map(|(_, m)| m.to_string()).collect();
    reached.sort();
    moments.sort();
    assert_eq!(moments, reached, "a hook for each moment it reaches, and no other");
    for l in facts(&b, "H.Lets").unwrap().into_iter().chain(facts(&b, "H.Reaches").unwrap()).chain(facts(&b, "H.Native-Hook").unwrap()) {
        assert_eq!(l[0], HARNESS);
    }
    for l in facts(&b, "H.Lets").unwrap().into_iter().chain(facts(&b, "H.Reaches").unwrap()) {
        assert_eq!(l[1], VERSION);
    }
}

fn each_power() -> Vec<Action> {
    vec![Action::Deny("not here".into()), Action::Modify("{}".into()), Action::Inject("a note".into()), Action::Answer("allow".into()), Action::KeepGoing("go on".into()), Action::Suspend("e1".into())]
}

#[test]
fn it_has_an_answer_for_exactly_what_its_bridge_lets_and_none_for_anything_else() {
    let needs = action_needs(env!("CARGO_MANIFEST_DIR"));
    let lets: Vec<(String, String)> = facts(&bridge(), "H.Lets").unwrap().into_iter().map(|f| (f[2].clone(), f[3].clone())).collect();
    assert!(facts(&bridge(), "H.Waits").unwrap().is_empty() && !lets.iter().any(|(_, p)| p == "suspend"), "no wait was read, so nothing is held");
    for (_, moment) in HOOKS {
        assert_eq!(Codex.answer(moment, &Action::Allow).unwrap(), Out { stdout: String::new(), code: 0 });
        for a in each_power() {
            let let_here = lets.contains(&(moment.to_string(), power(&needs, &a).unwrap()));
            assert_eq!(Codex.answer(moment, &a).is_ok(), let_here, "{moment}, {a:?}: answered exactly where let");
        }
    }
}

#[test]
fn each_answer_is_in_the_shape_codex_reads() {
    let j = |m: &str, a: Action| serde_json::from_str::<Json>(&Codex.answer(m, &a).unwrap().stdout).unwrap();
    assert_eq!(j("tool_called", Action::Deny("no".into())), json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": "no"}}));
    assert_eq!(j("tool_called", Action::Inject("n".into())), json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": "n"}}));
    assert_eq!(j("turn_begins", Action::Deny("r".into())), json!({"decision": "block", "reason": "r"}));
    assert_eq!(j("turn_begins", Action::Inject("n".into())), json!({"hookSpecificOutput": {"hookEventName": "UserPromptSubmit", "additionalContext": "n"}}));
    assert_eq!(j("tool_result", Action::Inject("n".into())), json!({"hookSpecificOutput": {"hookEventName": "PostToolUse", "additionalContext": "n"}}));
    assert_eq!(j("permission_asked", Action::Answer("deny".into())), json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {"behavior": "deny"}}}));
    assert_eq!(j("turn_end", Action::KeepGoing("more".into())), json!({"decision": "block", "reason": "more"}));
    assert!(Codex.answer("permission_asked", &Action::Answer("maybe".into())).is_err());
}

#[test]
fn a_shell_call_and_an_edit_are_read_into_their_parts() {
    let call = |name: &str, input: Json| Codex.tool_use(&json!({"hook_event_name": "PreToolUse", "cwd": "/w", "tool_name": name, "tool_input": input}).to_string());
    assert_eq!(call("Bash", json!({"command": "ls -l"})), ToolUse::command("ls -l"));
    // the input Codex 0.160.0 gave its PreToolUse hook for an edit, on loopback
    let e = call("apply_patch", json!({"command": "*** Begin Patch\n*** Update File: a.txt\n@@\n-one\n+uno\n*** End Patch\n"}));
    let patch = "--- a/a.txt\n+++ b/a.txt\n@@ -1,1 +1,1 @@\n-one\n+uno\n".to_string();
    assert_eq!(e, ToolUse::edit(vec![FileChange { kind: "edit".into(), path: "a.txt".into(), patch, added: "uno".into(), ..Default::default() }]));
    assert_eq!(call("mcp__x__y", json!({})), ToolUse::named("mcp__x__y"));
    assert_eq!(Codex.tool_use("not json"), ToolUse::default());
}
