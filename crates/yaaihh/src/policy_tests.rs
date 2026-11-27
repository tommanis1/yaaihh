use super::*;

use crate::permission::KEY;

fn levels(v: Value) -> Levels {
    serde_json::from_value(v).unwrap()
}

/// Each level inherits from the one above it, nothing set giving coordd's default, and its own wins; Permissions
/// reads Safe, Guarded and Yolo from the two settings at every level, as one row.
#[test]
fn each_level_inherits_from_the_one_above_and_its_own_wins() {
    let (_held, mut s) = crate::home::tests::made();
    let p = &mut s.projects[0];
    let agent = Scope::Agent("try-1".into());
    assert_eq!(resolve(p, &Levels::default(), &Scope::Project, KEY), (None, "safe".into(), "the default"));
    for (mode, check, want) in [("safe", true, "safe"), ("yolo", true, "guarded"), ("yolo", false, "yolo")] {
        let l = levels(serde_json::json!({"yaaihh": {"agent_mode": mode, "safety_check": check}}));
        assert_eq!(resolve(p, &l, &Scope::Global, KEY), (Some(want.into()), "safe".into(), "the default"), "{want}");
        assert_eq!(resolve(p, &l, &Scope::Project, KEY), (None, want.into(), "every project"), "{want}");
        assert_eq!(resolve(p, &l, &agent, KEY), (None, want.into(), "every project"), "{want}");
        let l = levels(serde_json::json!({"project": {"agent_mode": mode, "safety_check": check}}));
        assert_eq!(resolve(p, &l, &Scope::Project, KEY), (Some(want.into()), "safe".into(), "the default"), "{want}");
        assert_eq!(resolve(p, &l, &agent, KEY), (None, want.into(), "this project"), "{want}");
    }
    // one of the two set at a level makes the row its own there, the other taken from above
    let l = levels(serde_json::json!({"yaaihh": {"agent_mode": "yolo", "safety_check": null}, "project": {"agent_mode": null, "safety_check": false}}));
    assert_eq!(resolve(p, &l, &Scope::Project, KEY), (Some("yolo".into()), "guarded".into(), "every project"));
    assert_eq!(resolve(p, &l, &agent, KEY), (None, "yolo".into(), "this project"));
    let a = p.live.agents.iter_mut().find(|a| a.name == "try-1").unwrap();
    (a.mode, a.mode_from) = (Some("safe".into()), Some("agent".into()));
    assert_eq!(resolve(p, &l, &agent, KEY), (Some("safe".into()), "yolo".into(), "this project"));
}

/// The page draws one Permissions row and never the two settings under it: inherited it is muted with Override,
/// which writes both at this level; its own it has the three choices and Reset, which unsets both; a pick writes
/// both; at an agent's level only what its harness takes is written, and a harness that takes neither says so.
#[test]
fn permissions_are_one_row_that_overrides_resets_and_picks_both_settings() {
    use crate::home::tests::{keyed, settle, texts};
    let (_held, mut s) = crate::home::tests::made();
    let out = std::env::temp_dir().join(format!("yaaihh-settings-{}.jsonl", std::process::id()));
    let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
    s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
    s.settings.asked = Some("walk".into());
    came(&mut s, false, Ok(serde_json::json!({"yaaihh": {"agent_mode": "yolo"}, "project": {}}).to_string()));
    open(&mut s, Scope::Project);
    let mut rt = crate::view_cost::window(s);
    settle(&mut rt);
    let t = texts(&rt);
    for said in ["Permissions", "Guarded", "Inherited from every project", "Override"] {
        assert!(t.iter().any(|x| x == said), "{said}: {t:?}");
    }
    let one = |rt: &rune::runtime::Runtime<App>| {
        let t = texts(rt);
        assert!(!t.iter().any(|x| x == "Agents start in" || x.starts_with("Claude Code's check") || x.starts_with("Safe:")), "no second row, its meaning in its help: {t:?}");
        assert!(rt.find("settings-row-agent_mode").is_none() && rt.find("settings-row-safety_check").is_none() && rt.find("settings-row-permissions").is_some());
    };
    one(&rt);
    keyed(&mut rt, "settings-override-permissions");
    assert_eq!(rt.state.settings.setting, 2, "two writes in flight");
    rt.update(|s, _| {
        came(s, true, Ok(serde_json::json!({"yaaihh": {"agent_mode": "yolo"}, "project": {"agent_mode": "yolo", "safety_check": true}}).to_string()));
        came(s, true, Ok(serde_json::json!({"yaaihh": {"agent_mode": "yolo"}, "project": {"agent_mode": "yolo", "safety_check": true}}).to_string()));
    });
    settle(&mut rt);
    let t = texts(&rt);
    for said in ["Safe", "Guarded", "Yolo", "Reset to global"] {
        assert!(t.iter().any(|x| x == said), "{said}: {t:?}");
    }
    one(&rt);
    keyed(&mut rt, "settings-reset-permissions");
    rt.update(|s, _| s.settings.setting = 0);
    rt.update(|s, _| set(s, 0, KEY, "yolo"));
    rt.update(|s, _| (s.settings.setting, s.settings.scope) = (0, Scope::Agent("try-1".into())));
    settle(&mut rt);
    one(&rt);
    rt.update(|s, _| set(s, 0, KEY, "safe"));
    rt.update(|s, _| {
        s.settings.setting = 0;
        let a = &mut s.projects[0].live.agents[0];
        (a.kind, a.capabilities.safety_toggle) = ("codex".into(), true);
    });
    rt.update(|s, _| set(s, 0, KEY, "guarded"));
    rt.update(|s, _| (s.settings.setting, s.projects[0].live.agents[0].capabilities.safety_toggle) = (0, false));
    settle(&mut rt);
    assert!(texts(&rt).iter().any(|x| x == "Its harness does not take this"), "{:?}", texts(&rt));
    rt.update(|s, _| s.projects[0].link = None);
    drop(child.stdin.take());
    let _ = child.wait();
    let sent: Vec<Value> = std::fs::read_to_string(&out).unwrap().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    let _ = std::fs::remove_file(&out);
    let set: Vec<(&str, &str, &str)> = sent.iter().filter_map(|v| v.get("req")).filter(|r| r["op"] == "settings").map(|r| (r["scope"].as_str().unwrap(), r["key"].as_str().unwrap(), r["value"].as_str().unwrap())).collect();
    assert_eq!(set, [
        ("project", "safety_check", "true"), ("project", "agent_mode", "yolo"),
        ("project", "safety_check", ""), ("project", "agent_mode", ""),
        ("project", "safety_check", "false"), ("project", "agent_mode", "yolo"),
        ("agent:try-1", "agent_mode", "safe"),
        ("agent:try-1", "safety_check", "true"),
    ], "the check before the mode, so a restart for the mode starts with it: Override, Reset, Yolo, then try-1's mode alone (no switch for its check), then a Codex agent's check alone");
}

/// The box, who may speak to him and each provider's default model inherit like the rest; at an agent's level the
/// box is the project's; the defaults in force are what New agent marks.
#[test]
fn the_box_the_speakers_and_each_providers_model_inherit_and_new_agent_marks_the_defaults() {
    use crate::home::tests::{settle, texts};
    let (_held, mut s) = crate::home::tests::made();
    s.settings.asked = Some("walk".into());
    s.settings.models = vec!["claude-opus-5-5".into(), "agy/gemini-3-pro".into()];
    came(&mut s, false, Ok(serde_json::json!({"yaaihh": {"boxed": true, "owner_speakers": "leads", "models.claude-gateway": "claude-opus-5-5"}, "project": {"owner_speakers": "w2,lead1", "models.agy-personal": "agy/gemini-3-pro"}}).to_string()));
    let p = &s.projects[0];
    let l = s.settings.shown.as_ref().unwrap().1.clone().unwrap();
    assert_eq!(resolve(p, &l, &Scope::Project, "boxed"), (None, "true".into(), "every project"));
    assert_eq!(resolve(p, &l, &Scope::Project, "owner_speakers"), (Some("w2,lead1".into()), "leads".into(), "every project"));
    assert_eq!(default_models(&s, p), vec![("Claude gateway", "claude-opus-5-5".to_string()), ("agy personal", "agy/gemini-3-pro".to_string())]);
    let options = crate::projects::model_options(&s.settings.models, &Default::default(), &default_models(&s, p));
    assert!(options[0].detail.starts_with("Default for Claude gateway"), "{:?}", options[0].detail);
    open(&mut s, Scope::Project);
    let mut rt = crate::view_cost::window(s);
    settle(&mut rt);
    let t = texts(&rt);
    for said in ["Agents run in a box", "On", "Inherited from every project", "Who may speak to you", "w2 ×", "lead1 ×", "Default model per provider", "Claude gateway", "agy sign-in", "OpenCode", "Reset to global"] {
        assert!(t.iter().any(|x| x == said), "{said}: {t:?}");
    }
    rt.update(|s, _| s.settings.scope = Scope::Agent("try-1".into()));
    settle(&mut rt);
    assert!(texts(&rt).iter().any(|x| x == "Set for the whole project"), "the box is the project's");
}

/// N-1956: Safe · Yolo at an agent's level is a Claude Code or an agy agent's, an agy one's only from a coordd that
/// takes it for agy; a Codex agent's harness does not take it.
#[test]
fn safe_and_yolo_at_an_agents_level_are_claude_codes_and_agys() {
    let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "r".into(), id: "r".into(), boxed: false }, Vec::new());
    let mut a: AgentView = serde_json::from_value(serde_json::json!({"id": "", "name": "g", "parent": "", "kind": "agy", "model": "", "state": "idle", "since": "",
        "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null})).unwrap();
    p.protocol = coord_types::AGY_MODE_PROTOCOL - 1;
    assert!(!agent_takes(&p, &a, "agent_mode"), "a coordd that refuses it for agy");
    p.protocol = coord_types::AGY_MODE_PROTOCOL;
    assert!(agent_takes(&p, &a, "agent_mode") && agent_takes(&p, &a, KEY) && !agent_takes(&p, &a, "safety_check"));
    // with no check of its own an agy agent in Yolo is in Yolo, where a Claude Code one keeps its check: Guarded
    p.live.agents = vec![a.clone()];
    let l = levels(serde_json::json!({"yaaihh": {"agent_mode": "yolo"}, "project": {}}));
    assert_eq!(resolve(&p, &l, &Scope::Agent("g".into()), KEY).1, "yolo");
    p.live.agents[0].kind = "claude".into();
    assert_eq!(resolve(&p, &l, &Scope::Agent("g".into()), KEY).1, "guarded");
    a.kind = "codex".into();
    assert!(!agent_takes(&p, &a, "agent_mode"));
    a.kind = "claude".into();
    assert!(agent_takes(&p, &a, "agent_mode"));
}
