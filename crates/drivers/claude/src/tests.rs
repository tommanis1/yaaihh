//! The driver held to its bridge's text, and its answers to the shapes Claude Code 2.1.332 reads.

use super::*;
use docket_hooks::frame::FileChange;

/// The bridge's text: std/bridges/claude-code.docket, among the programs.
fn bridge() -> String {
    let p = format!("{}/../../../docket/std/bridges/claude-code.docket", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

fn lines(text: &str, fact: &str) -> Vec<Vec<String>> {
    docket_hooks::bridge::facts(text, fact).unwrap()
}

#[test]
fn its_hooks_are_its_bridges_native_hooks_and_its_name_is_the_bridges() {
    let b = bridge();
    let mut said: Vec<(String, String)> = lines(&b, "H.Native-Hook").into_iter().map(|f| (f[1].clone(), f[2].clone())).collect();
    let mut ours: Vec<(String, String)> = HOOKS.iter().map(|(n, m)| (n.to_string(), m.to_string())).collect();
    said.sort();
    ours.sort();
    assert_eq!(ours, said);
    for l in lines(&b, "H.Lets").into_iter().chain(lines(&b, "H.Reaches")) {
        assert_eq!((l[0].as_str(), l[1].as_str()), (HARNESS, VERSION), "every line is for this harness at this version");
    }
    for l in lines(&b, "H.Waits") {
        assert_eq!(l[3], WAITS_SECONDS.to_string(), "it waits as long as our settings let a hook take");
    }
}

/// An action of each power, with what Claude Code takes for it where a field has a form.
fn each_power() -> Vec<Action> {
    vec![
        Action::Deny("not here".into()),
        Action::Modify(r#"{"command":"ls"}"#.into()),
        Action::Inject("a note".into()),
        Action::Answer("allow".into()),
        Action::KeepGoing("go on".into()),
        Action::Suspend("e1".into()),
    ]
}

#[test]
fn it_has_an_answer_for_exactly_what_its_bridge_lets_and_none_for_anything_else() {
    let lets: Vec<(String, String)> = lines(&bridge(), "H.Lets").into_iter().map(|f| (f[2].clone(), f[3].clone())).collect();
    let waits: Vec<String> = lines(&bridge(), "H.Waits").into_iter().map(|f| f[2].clone()).collect();
    let needs = docket_hooks::bridge::action_needs(env!("CARGO_MANIFEST_DIR"));
    for (_, moment) in HOOKS {
        assert_eq!(Claude.answer(moment, &Action::Allow).unwrap(), Out { stdout: String::new(), code: 0 }, "Allow: as it would have gone on");
        for a in each_power() {
            let let_here = lets.contains(&(moment.to_string(), docket_hooks::bridge::power(&needs, &a).unwrap()));
            if let Action::Suspend(_) = a {
                // held by the frame, not answered: the harness waits as long as its `Waits` says
                assert_eq!(let_here, waits.contains(&moment.to_string()), "{moment}: a Suspend only where it says how long it waits");
                continue;
            }
            assert_eq!(Claude.answer(moment, &a).is_ok(), let_here, "{moment}, {a:?}: answered exactly where let");
        }
    }
}

#[test]
fn each_answer_is_in_the_shape_claude_code_reads() {
    let j = |m: &str, a: Action| serde_json::from_str::<Json>(&Claude.answer(m, &a).unwrap().stdout).unwrap();
    assert_eq!(j("tool_called", Action::Deny("no".into())), json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": "no"}}));
    assert_eq!(j("tool_called", Action::Modify(r#"{"command":"ls -l"}"#.into())), json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "updatedInput": {"command": "ls -l"}}}));
    assert_eq!(j("turn_begins", Action::Inject("n".into())), json!({"hookSpecificOutput": {"hookEventName": "UserPromptSubmit", "additionalContext": "n"}}));
    assert_eq!(j("turn_begins", Action::Deny("r".into())), json!({"decision": "block", "reason": "r"}));
    assert_eq!(j("tool_result", Action::Modify("out".into())), json!({"hookSpecificOutput": {"hookEventName": "PostToolUse", "updatedToolOutput": "out"}}));
    assert_eq!(j("permission_asked", Action::Answer("deny".into())), json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {"behavior": "deny"}}}));
    assert_eq!(j("turn_end", Action::KeepGoing("more".into())), json!({"decision": "block", "reason": "more"}));
    assert!(Claude.answer("permission_asked", &Action::Answer("maybe".into())).is_err());
    assert!(Claude.answer("tool_called", &Action::Modify("ls".into())).is_err(), "a new input is a JSON object");
}

#[test]
fn a_tool_call_is_read_into_its_parts_once() {
    let dir = std::env::temp_dir().join(format!("docket-claude-tool-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("a.txt");
    std::fs::write(&f, "one\ntwo\nthree\n").unwrap();
    let p = f.display().to_string();
    let d = dir.display().to_string();
    let call = |name: &str, input: Json| Claude.tool_use(&json!({"hook_event_name": "PreToolUse", "cwd": d, "tool_name": name, "tool_input": input}).to_string());
    assert_eq!(call("Bash", json!({"command": "ls -l"})), ToolUse::command("ls -l"));
    assert_eq!(call("Read", json!({"file_path": p})), ToolUse::named("read"), "a read changes no file");
    let e = call("Edit", json!({"file_path": p, "old_string": "two", "new_string": "2\n2b"}));
    let patch = "--- a/a.txt\n+++ b/a.txt\n@@ -2,1 +2,2 @@\n-two\n+2\n+2b\n".to_string();
    assert_eq!(e, ToolUse::edit(vec![FileChange { kind: "edit".into(), path: "a.txt".into(), patch, added: "2\n2b".into(), ..Default::default() }]), "the hunk at the old text's line, the path from the working folder");
    assert_eq!(call("Write", json!({"file_path": p, "content": "x"})).changes[0].kind, "edit");
    let new = dir.join("b.txt").display().to_string();
    let c = call("Write", json!({"file_path": new, "content": "new text\n"}));
    assert_eq!((c.tool.as_str(), c.changes[0].kind.as_str(), c.changes[0].path.as_str()), ("edit", "create", "b.txt"));
    assert_eq!(call("WebFetch", json!({"url": "u"})).tool, "WebFetch", "another tool by its own name, no parts");
    assert_eq!(Claude.tool_use("not json"), ToolUse::default());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn its_settings_run_the_relay_for_each_hook_and_wait_as_long_as_the_bridge_says() {
    let s = hooks_settings("/r/docket-hook-relay");
    for (name, _) in HOOKS {
        assert_eq!(s["hooks"][name][0]["hooks"][0]["command"], format!("/r/docket-hook-relay {name}"));
        assert_eq!(s["hooks"][name][0]["hooks"][0]["timeout"], WAITS_SECONDS);
    }
    assert_eq!(s["hooks"].as_object().unwrap().len(), HOOKS.len());
}
