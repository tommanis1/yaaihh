//! The driver held to its bridge's text, its plugin's answers, and a tool call's parts.

use super::*;
use docket_hooks::frame::FileChange;
use docket_hooks::bridge::{action_needs, facts, power};

/// Its plugin, agent-base's opencode/plugin/relay.js.
fn plugin() -> String {
    let p = format!("{}/../../../agent-base/opencode/plugin/relay.js", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

fn bridge() -> String {
    let p = format!("{}/../../../docket/std/bridges/opencode.docket", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

#[test]
fn its_hooks_are_its_bridges_native_hooks_and_its_plugin_relays_each() {
    let b = bridge();
    let mut said: Vec<(String, String)> = facts(&b, "H.Native-Hook").unwrap().into_iter().map(|f| (f[1].clone(), f[2].clone())).collect();
    let mut ours: Vec<(String, String)> = HOOKS.iter().map(|(n, m)| (n.to_string(), m.to_string())).collect();
    said.sort();
    ours.sort();
    assert_eq!(ours, said);
    for l in facts(&b, "H.Lets").unwrap().into_iter().chain(facts(&b, "H.Reaches").unwrap()) {
        assert_eq!((l[0].as_str(), l[1].as_str()), (HARNESS, VERSION));
    }
    for (name, _) in HOOKS {
        assert_eq!(plugin().matches(&format!("\"{name}\"")).count(), 1, "the plugin relays {name}");
    }
}

fn each_power() -> Vec<Action> {
    vec![Action::Deny("not here".into()), Action::Modify("{\"args\":{}}".into()), Action::Inject("a note".into()), Action::Answer("allow".into()), Action::KeepGoing("go on".into())]
}

#[test]
fn it_has_an_answer_for_exactly_what_its_bridge_lets_and_none_for_anything_else() {
    let needs = action_needs(env!("CARGO_MANIFEST_DIR"));
    let lets: Vec<(String, String)> = facts(&bridge(), "H.Lets").unwrap().into_iter().map(|f| (f[2].clone(), f[3].clone())).collect();
    for (_, moment) in HOOKS {
        assert_eq!(Opencode.answer(moment, &Action::Allow).unwrap(), Out { stdout: String::new(), code: 0 });
        for a in each_power() {
            let let_here = lets.contains(&(moment.to_string(), power(&needs, &a).unwrap()));
            assert_eq!(Opencode.answer(moment, &a).is_ok(), let_here, "{moment}, {a:?}: answered exactly where let");
        }
    }
    // a Suspend is held by the frame: only where the request waits on the server until it is answered
    let held: Vec<&str> = lets.iter().filter(|(_, p)| p == "suspend").map(|(m, _)| m.as_str()).collect();
    assert_eq!(held, ["permission_asked"]);
}

#[test]
fn each_answer_is_what_its_plugin_applies() {
    let j = |m: &str, a: Action| serde_json::from_str::<Json>(&Opencode.answer(m, &a).unwrap().stdout).unwrap();
    assert_eq!(j("tool_called", Action::Deny("no".into())), json!({"deny": "no"}));
    assert_eq!(j("tool_called", Action::Modify(r#"{"args":{"command":"ls"}}"#.into())), json!({"modify": {"args": {"command": "ls"}}}));
    assert_eq!(j("permission_asked", Action::Answer("allow".into())), json!({"reply": "once"}));
    assert_eq!(j("permission_asked", Action::Answer("deny".into())), json!({"reply": "reject"}));
    assert!(Opencode.answer("permission_asked", &Action::Answer("always".into())).is_err());
    assert!(Opencode.answer("compacting", &Action::Modify("more".into())).is_err(), "keys to set are a JSON object");
    for key in ["deny", "modify", "reply"] {
        assert!(plugin().contains(&format!("a.{key}")), "the plugin applies {key}");
    }
}

#[test]
fn a_tool_call_is_read_from_opencodes_own_names() {
    let dir = std::env::temp_dir().join(format!("docket-opencode-tool-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("a.txt");
    std::fs::write(&f, "one\ntwo\n").unwrap();
    let p = f.display().to_string();
    let d = dir.display().to_string();
    let call = |tool: &str, args: Json| Opencode.tool_use(&json!({"input": {"tool": tool, "sessionID": "s", "callID": "c"}, "output": {"args": args}, "directory": d}).to_string());
    assert_eq!(call("bash", json!({"command": "ls -l"})), ToolUse::command("ls -l"));
    assert_eq!(call("read", json!({"filePath": p})), ToolUse::named("read"));
    let e = call("edit", json!({"filePath": p, "oldString": "two", "newString": "2"}));
    let patch = "--- a/a.txt\n+++ b/a.txt\n@@ -2,1 +2,1 @@\n-two\n+2\n".to_string();
    assert_eq!(e, ToolUse::edit(vec![FileChange { kind: "edit".into(), path: "a.txt".into(), patch, added: "2".into(), ..Default::default() }]));
    assert_eq!(call("write", json!({"filePath": p, "content": "x"})).changes[0].kind, "edit");
    assert_eq!(call("write", json!({"filePath": dir.join("new.txt"), "content": "x"})).changes[0].kind, "create");
    let a = call("apply_patch", json!({"patchText": "*** Begin Patch\n*** Update File: a.txt\n@@\n-one\n+1\n*** Add File: b.txt\n+b\n*** End Patch"}));
    let files: Vec<(&str, &str)> = a.changes.iter().map(|c| (c.kind.as_str(), c.path.as_str())).collect();
    assert_eq!((a.tool.as_str(), files), ("edit", vec![("edit", "a.txt"), ("create", "b.txt")]), "a change for each file");
    assert_eq!(Opencode.tool_use("[]"), ToolUse::default());
    let _ = std::fs::remove_dir_all(&dir);
}
