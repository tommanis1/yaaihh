//! The driver held to its bridge's text, for both ways agy runs.

use super::*;
use docket_hooks::bridge::{action_needs, facts, power};
use docket_hooks::frame::FileChange;

fn bridge() -> String {
    let p = format!("{}/../../../docket/std/bridges/agy.docket", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

#[test]
fn its_names_are_its_bridges_native_hooks_one_for_each_moment_it_reaches_in_some_mode() {
    let b = bridge();
    let mut said: Vec<(String, String)> = facts(&b, "H.Native-Hook").unwrap().into_iter().map(|f| (f[1].clone(), f[2].clone())).collect();
    let mut ours: Vec<(String, String)> = HOOKS.iter().map(|(n, m)| (n.to_string(), m.to_string())).collect();
    said.sort();
    ours.sort();
    assert_eq!(ours, said);
    let mut reached: Vec<String> = facts(&b, "H.Reaches").unwrap().into_iter().map(|f| f[2].clone()).chain(facts(&b, "H.Reaches-In").unwrap().into_iter().map(|f| f[3].clone())).collect();
    reached.sort();
    reached.dedup();
    let mut moments: Vec<String> = HOOKS.iter().map(|(_, m)| m.to_string()).collect();
    moments.sort();
    assert_eq!(moments, reached, "a name for each moment it reaches, and no other");
    for l in ["H.Reaches", "H.Reaches-In", "H.Lets"].into_iter().flat_map(|f| facts(&b, f).unwrap()) {
        assert!(l[0] == HARNESS && VERSIONS.contains(&l[1].as_str()), "{l:?}");
    }
}

#[test]
fn it_has_an_answer_for_exactly_what_its_bridge_lets_and_none_for_anything_else() {
    let needs = action_needs(env!("CARGO_MANIFEST_DIR"));
    let t = || "x".to_string();
    for version in VERSIONS {
        let lets: Vec<(String, String)> = facts(&bridge(), "H.Lets").unwrap().into_iter().filter(|f| f[1] == version).map(|f| (f[2].clone(), f[3].clone())).collect();
        let agy = Agy { version };
        for (_, moment) in HOOKS {
            assert_eq!(agy.answer(moment, &Action::Allow).unwrap(), Out { stdout: String::new(), code: 0 }, "Allow: as it would have gone on");
            for a in [Action::Deny(t()), Action::Modify(t()), Action::Inject(t()), Action::Answer("allow".into()), Action::KeepGoing(t()), Action::Suspend(t())] {
                let let_here = lets.contains(&(moment.to_string(), power(&needs, &a).unwrap()));
                assert_eq!(agy.answer(moment, &a).is_ok(), let_here, "{version}, {moment}, {a:?}: answered exactly where let");
            }
        }
    }
    let agy = Agy { version: VERSIONS[0] };
    let j = |m: &str, a: Action| serde_json::from_str::<Json>(&agy.answer(m, &a).unwrap().stdout).unwrap();
    assert_eq!(j("tool_called", Action::Deny("no".into())), json!({"decline": "no"}));
    assert_eq!(j("permission_asked", Action::Answer("allow".into())), json!({"answer": "once"}));
    assert_eq!(j("permission_asked", Action::Answer("deny".into())), json!({"answer": "decline"}));
    assert!(agy.answer("permission_asked", &Action::Answer("always".into())).is_err(), "once or declined, never for the session");
}

#[test]
fn a_call_asked_about_is_read_into_its_parts_and_a_call_done_keeps_its_name() {
    let agy = Agy { version: VERSIONS[1] };
    let call = |name: &str, input: Json| agy.tool_use(&json!({"hook_event_name": "PreToolUse", "cwd": "/w", "tool_name": name, "tool_input": input}).to_string());
    assert_eq!(call("Bash", json!({"command": "ls"})), ToolUse::command("ls"));
    let e = call("Edit", json!({"file_path": "/w/f", "old_string": "a", "new_string": "b"}));
    assert_eq!(e, ToolUse::edit(vec![FileChange { kind: "edit".into(), path: "f".into(), patch: "--- a/f\n+++ b/f\n@@ -1,1 +1,1 @@\n-a\n+b\n".into(), added: "b".into(), ..Default::default() }]));
    let made = call("Edit", json!({"file_path": "/w/no/such/new.txt", "old_string": "", "new_string": "x"}));
    assert_eq!((made.changes[0].kind.as_str(), made.changes[0].path.as_str()), ("create", "no/such/new.txt"), "a new file made by Edit");
    assert_eq!(agy.tool_use(&json!({"session_id": "s", "tool_use_id": "c", "tool_name": "Grep"}).to_string()).tool, "Grep");
    // agy's own tools, should one come under its own name: each file it names is a change a guard on files sees
    let w = call("write_to_file", json!({"TargetFile": "/w/no/such/n.txt", "CodeContent": "x"}));
    assert_eq!((w.tool.as_str(), w.changes.iter().map(|c| (c.kind.as_str(), c.path.as_str())).collect::<Vec<_>>()), ("edit", vec![("create", "no/such/n.txt")]));
    let m = call("rename_file", json!({"SourcePath": "/w/a", "DestinationPath": "/w/b", "Path": ["/w/a", "/w/c"]}));
    assert_eq!(m.changes.iter().map(|c| c.path.as_str()).collect::<Vec<_>>(), ["a", "c", "b"], "each file once, sources and lists too");
    assert_eq!(call("Read", json!({"file_path": "/w/f"})), ToolUse::named("Read"), "a read stays a read");
    assert_eq!(call("think", json!({"thought": "x"})), ToolUse::named("think"), "a call naming no file keeps its name");
}
