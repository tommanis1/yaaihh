//! How an opencode session's start takes the switches: its config, given whole in OPENCODE_CONFIG_CONTENT.
//!
//! Each was measured by the first request opencode 1.18.34 sent to a stand-in model API on loopback: `instructions`
//! appends the file; `agent.build.prompt` as `{file:PATH}` replaces its main text (opencode-rpc names no agent, so
//! `build` is the one that runs); a permission `deny` takes the tool out of the request, where `tools: {x: false}`
//! changed nothing.

use docket_hooks::start::{Launch, Prompt, Switches};
use serde_json::{json, Map, Value as Json};

/// The start's environment for `s`, with the system text in file `prompt` given as `how`.
pub fn launch(s: &Switches, prompt: Option<(&str, Prompt)>) -> Launch {
    let mut c = Map::new();
    match prompt {
        Some((file, Prompt::Append)) => {
            c.insert("instructions".into(), json!([file]));
        }
        Some((file, Prompt::Replace)) => {
            c.insert("instructions".into(), json!([]));
            c.insert("agent".into(), json!({"build": {"prompt": format!("{{file:{file}}}")}}));
        }
        None => {}
    }
    if !s.tools_off.is_empty() {
        c.insert("permission".into(), Json::Object(s.tools_off.iter().map(|t| (t.clone(), json!("deny"))).collect()));
    }
    let mut l = Launch::default();
    if !c.is_empty() {
        l.env.push(("OPENCODE_CONFIG_CONTENT".into(), Json::Object(c).to_string()));
    }
    l.not_taken.extend(s.parts_off.iter().map(|p| format!("part {:?}: opencode's own prompt has no parts to leave out", p.chars().take(40).collect::<String>())));
    if s.safety_check {
        l.not_taken.push("safety_check: opencode has no check of its own to keep on".into());
    }
    // with the check off the program answers its asks (std.hooks 6); with it on, nothing here runs it unasked
    if s.unasked && s.safety_check {
        l.not_taken.push("unasked: opencode has no switch to run commands unasked, so it asks as before".into());
    }
    l
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
    fn its_start_gives_one_config_with_each_switch_as_measured() {
        assert_eq!(launch(&Switches::default(), None).not_taken, ["safety_check: opencode has no check of its own to keep on"]);
        let config = |l: &docket_hooks::start::Launch| serde_json::from_str::<Json>(&l.env[0].1).unwrap();
        let l = launch(&s(|s| s.tools_off = vec!["webfetch".into(), "bash".into()]), Some(("/a/system-prompt.md", Prompt::Append)));
        assert_eq!(l.env[0].0, "OPENCODE_CONFIG_CONTENT");
        assert_eq!(config(&l), json!({"instructions": ["/a/system-prompt.md"], "permission": {"webfetch": "deny", "bash": "deny"}}));
        let l = launch(&Switches::default(), Some(("/a/p.md", Prompt::Replace)));
        assert_eq!(config(&l), json!({"instructions": [], "agent": {"build": {"prompt": "{file:/a/p.md}"}}}));
        let l = launch(&s(|s| s.parts_off = vec!["x".into()]), None);
        assert!(l.env.is_empty() && l.not_taken.len() == 2, "{:?}", l.not_taken);
        assert!(launch(&s(|s| s.safety_check = false), None).not_taken.is_empty(), "off: no check to lift");
    }
}
