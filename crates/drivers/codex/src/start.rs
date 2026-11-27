//! How a Codex session's start takes the switches and runs the relay: its system text from a file, tools off, what
//! it cannot take, and its hooks file.
//!
//! Each was measured by the first request Codex 0.160.0 sent to a stand-in model API on loopback: a replace takes
//! its own instructions out whole (16,979 bytes) and puts the file's text there; an append goes in as a developer
//! message; and the tools below leave the request only by the switch named beside them.

use crate::HOOKS;
use docket_hooks::start::{Launch, Prompt, Switches};
use serde_json::{json, Value as Json};

/// Its built-in tools that a switch takes out of the request, each with that switch. The goal tools go together,
/// as one feature. Its others (exec_command, write_stdin, request_user_input, view_image) have no switch.
pub const TOOL_SWITCHES: [(&str, &str); 5] = [
    ("web_search", "web_search=\"disabled\""),
    ("get_goal", "features.goals=false"),
    ("create_goal", "features.goals=false"),
    ("update_goal", "features.goals=false"),
    ("multi_agent_v1", "features.multi_agent=false"),
];

/// The start's arguments for `s`, with the system text in file `prompt` given as `how`. An append has no key that
/// takes a file, so its text is read here and goes as one argument, bounded by Linux's 128 KiB.
pub fn launch(s: &Switches, prompt: Option<(&str, Prompt)>) -> Result<Launch, String> {
    let mut l = Launch::default();
    let mut c = |v: String| l.args.extend(["-c".to_string(), v]);
    match prompt {
        Some((file, Prompt::Replace)) => c(format!("model_instructions_file={}", serde_json::to_string(file).unwrap())),
        Some((file, Prompt::Append)) => {
            let text = std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
            c(format!("developer_instructions={}", serde_json::to_string(&text).unwrap()))
        }
        None => {}
    }
    let mut took: Vec<&str> = Vec::new();
    for t in &s.tools_off {
        match TOOL_SWITCHES.iter().find(|(n, _)| n == t) {
            None => {
                let can: Vec<&str> = TOOL_SWITCHES.iter().map(|(n, _)| *n).collect();
                l.tools_on.push(format!("tool {t}: Codex {} has no switch that takes it out; it can take out {}", crate::VERSION, can.join(", ")));
            }
            Some((_, switch)) if !took.contains(switch) => {
                took.push(switch);
                l.args.extend(["-c".to_string(), switch.to_string()]);
                let with: Vec<&str> = TOOL_SWITCHES.iter().filter(|(n, w)| w == switch && !s.tools_off.iter().any(|o| o == n)).map(|(n, _)| *n).collect();
                if !with.is_empty() {
                    l.not_taken.push(format!("tool {t}: Codex takes it out only with {}, which go too", with.join(", ")));
                }
            }
            Some(_) => {}
        }
    }
    l.not_taken.extend(s.parts_off.iter().map(|p| format!("part {:?}: Codex's own instructions have no parts to leave out", p.chars().take(40).collect::<String>())));
    if s.safety_check {
        l.not_taken.push("safety_check: Codex has no check of its own to keep on".into());
    }
    // with the check off the program answers its asks (std.hooks 6); with it on, nothing here runs it unasked
    if s.unasked && s.safety_check {
        l.not_taken.push("unasked: Codex has no switch to run commands unasked, so it asks as before".into());
    }
    Ok(l)
}

/// Codex's hooks file (`$CODEX_HOME/hooks.json`) for an agent: each of its own hooks runs `RELAY NAME`. The relay
/// reaches the agent's socket by DOCKET_HOOK_SOCKET in its environment. Codex runs a hook only once its hash is
/// trusted in its config, which the start does through the app-server (`hooks/list`, `config/value/write`).
pub fn hooks_file(relay: &str) -> Json {
    let mut h = serde_json::Map::new();
    for (name, _) in HOOKS.iter().filter(|(n, _)| *n != "failed") {
        let on = json!({"hooks": [{"type": "command", "command": format!("{relay} {name}")}]});
        let on = if name.ends_with("ToolUse") { json!({"matcher": "*", "hooks": on["hooks"]}) } else { on };
        h.insert(name.to_string(), json!([on]));
    }
    json!({"hooks": h})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(f: impl FnOnce(&mut Switches)) -> Switches {
        let mut s = Switches::default();
        f(&mut s);
        s
    }

    #[test]
    fn its_start_takes_each_switch_as_measured_and_says_what_it_cannot() {
        let off = |f: fn(&mut Switches)| s(|s| (s.safety_check = false, f(s)).1);
        assert_eq!(launch(&off(|_| ()), None).unwrap(), Default::default(), "no check of its own to lift, and nothing set");
        assert_eq!(launch(&Switches::default(), None).unwrap().not_taken, ["safety_check: Codex has no check of its own to keep on"]);
        let l = launch(&off(|s| s.tools_off = vec!["web_search".into(), "get_goal".into(), "exec_command".into()]), None).unwrap();
        assert_eq!(l.args, ["-c", "web_search=\"disabled\"", "-c", "features.goals=false"]);
        assert_eq!(l.not_taken.len(), 1, "{:?}", l.not_taken);
        assert!(l.not_taken[0].starts_with("tool get_goal: Codex takes it out only with create_goal, update_goal"), "{:?}", l.not_taken);
        assert_eq!(l.tools_on.len(), 1, "{:?}", l.tools_on);
        assert!(l.tools_on[0].starts_with("tool exec_command: Codex 0.160.0 has no switch"), "{:?}", l.tools_on);
        let all: Vec<String> = TOOL_SWITCHES.iter().map(|(n, _)| n.to_string()).collect();
        let l = launch(&s(|s| (s.tools_off, s.safety_check) = (all, false)), None).unwrap();
        assert_eq!((l.args.len(), l.not_taken.len() + l.tools_on.len()), (6, 0), "each switch once, and nothing goes that was not asked");
        let l = launch(&s(|s| s.parts_off = vec!["x".into()]), None).unwrap();
        assert!(l.args.is_empty() && l.not_taken.len() == 2, "{:?}", l.not_taken);
        assert_eq!(launch(&Switches::default(), Some(("/a/system-prompt.md", Prompt::Replace))).unwrap().args, ["-c", "model_instructions_file=\"/a/system-prompt.md\""]);
    }

    #[test]
    fn an_appended_text_goes_whole_as_its_developer_instructions() {
        let f = std::env::temp_dir().join(format!("docket-codex-system-{}.md", std::process::id()));
        std::fs::write(&f, "Say \"they\".\nTwo lines.\n").unwrap();
        let l = launch(&Switches::default(), Some((f.to_str().unwrap(), Prompt::Append))).unwrap();
        assert_eq!(l.args, ["-c", r#"developer_instructions="Say \"they\".\nTwo lines.\n""#]);
        std::fs::remove_file(&f).unwrap();
        assert!(launch(&Switches::default(), Some(("/no/such/file", Prompt::Append))).is_err());
    }

    #[test]
    fn its_hooks_file_runs_the_relay_for_each_of_its_own_hooks() {
        let f = hooks_file("/r/docket-hook-relay");
        let own: Vec<&str> = HOOKS.iter().map(|(n, _)| *n).filter(|n| *n != "failed").collect();
        assert_eq!(f["hooks"].as_object().unwrap().len(), own.len());
        for name in own {
            assert_eq!(f["hooks"][name][0]["hooks"][0]["command"], format!("/r/docket-hook-relay {name}"));
        }
        assert_eq!(f["hooks"]["PreToolUse"][0]["matcher"], "*");
        assert!(f["hooks"]["Stop"][0].get("matcher").is_none());
    }
}
