//! Claude Code 2.1.332 in its own terms (std/bridges/claude-code.docket): its hook names, its answer to each
//! HookAction its bridge lets, a tool call's parts read from its input, and the hooks block of its settings.
//!
//! The answers' shapes were read in Claude Code 2.1.332's own schema for a hook's output: `decision` and `reason`
//! at the top; under `hookSpecificOutput`, PreToolUse's `permissionDecision`, `permissionDecisionReason`,
//! `updatedInput` and `additionalContext`, UserPromptSubmit's and PostToolUse's `additionalContext`, PostToolUse's
//! `updatedToolOutput`, and PermissionRequest's `decision` with `behavior` allow or deny.

use docket_hooks::frame::{Native, Out, ToolUse};
use docket_hooks::{patch, Action};
use docket_v2::run::value::Value;
use serde_json::{json, Value as Json};

pub const HARNESS: &str = "claude-code";
pub const VERSION: &str = "2.1.332";

/// Its hooks, as our settings register them, each with the moment it reaches: its bridge's `Native-Hook` facts.
pub const HOOKS: [(&str, &str); 7] = [
    ("UserPromptSubmit", "turn_begins"),
    ("PreToolUse", "tool_called"),
    ("PostToolUse", "tool_result"),
    ("PermissionRequest", "permission_asked"),
    ("Stop", "turn_end"),
    ("PreCompact", "compacting"),
    ("StopFailure", "overloaded"),
];

/// Its tools by its own names, each with the library's name for what it does (std.hooks' `ToolUse.tool`).
pub const TOOLS: [(&str, &str); 4] = [("Bash", "command"), ("Read", "read"), ("Edit", "edit"), ("Write", "edit")];

/// How long it waits for a hook's answer, as our settings set it for every hook (the bridge's `Waits`).
pub const WAITS_SECONDS: u32 = 600;

pub struct Claude;

/// A Modify's patch for a tool's input: the whole new input, as a JSON object.
fn new_input(patch: &str) -> Result<Json, String> {
    match serde_json::from_str::<Json>(patch) {
        Ok(v @ Json::Object(_)) => Ok(v),
        _ => Err("a Modify of a tool call's input is the whole new input, as a JSON object".into()),
    }
}

impl Native for Claude {
    fn name(&self) -> Vec<Value> {
        vec![Value::text(HARNESS), Value::text(VERSION)]
    }

    fn hooks(&self) -> Vec<(&'static str, &'static str)> {
        HOOKS.to_vec()
    }

    fn answer(&self, moment: &str, action: &Action) -> Result<Out, String> {
        use Action::*;
        Ok(match (moment, action) {
            (_, Allow) => Out { stdout: String::new(), code: 0 },
            ("turn_begins", Deny(r)) => Out::json(json!({"decision": "block", "reason": r})),
            ("turn_begins", Inject(t)) => Out::specific("UserPromptSubmit", json!({"additionalContext": t})),
            ("tool_called", Deny(r)) => Out::specific("PreToolUse", json!({"permissionDecision": "deny", "permissionDecisionReason": r})),
            ("tool_called", Modify(p)) => Out::specific("PreToolUse", json!({"updatedInput": new_input(p)?})),
            ("tool_called", Inject(t)) => Out::specific("PreToolUse", json!({"additionalContext": t})),
            ("tool_result", Modify(t)) => Out::specific("PostToolUse", json!({"updatedToolOutput": t})),
            ("tool_result", Inject(t)) => Out::specific("PostToolUse", json!({"additionalContext": t})),
            ("permission_asked", Answer(a)) if a == "allow" || a == "deny" => Out::specific("PermissionRequest", json!({"decision": {"behavior": a}})),
            ("permission_asked", Answer(a)) => return Err(format!("a permission is answered allow or deny, not {a:?}")),
            ("permission_asked", Modify(p)) => Out::specific("PermissionRequest", json!({"decision": {"behavior": "allow", "updatedInput": new_input(p)?}})),
            ("turn_end", KeepGoing(p)) => Out::json(json!({"decision": "block", "reason": p})),
            (m, a) => return Err(format!("Claude Code {VERSION} has no answer {a:?} at {m}")),
        })
    }

    fn tool_use(&self, input: &str) -> ToolUse {
        let Ok(v) = serde_json::from_str::<Json>(input) else { return ToolUse::default() };
        let (i, cwd) = (&v["tool_input"], v["cwd"].as_str().unwrap_or_default());
        let s = |k: &str| i[k].as_str().unwrap_or_default().to_string();
        let native = v["tool_name"].as_str().unwrap_or_default();
        if native.is_empty() {
            return ToolUse::default();
        }
        // the library's name from the table, else the harness's own; the parts by what the input holds
        let tool = TOOLS.iter().find(|(n, _)| *n == native).map_or(native, |(_, ours)| ours);
        let path = s("file_path");
        let changes = match (path.is_empty(), i.get("old_string"), i.get("content")) {
            (false, Some(_), _) => vec![patch::edit(cwd, &path, &s("old_string"), &s("new_string"))],
            (false, None, Some(_)) => vec![patch::write(cwd, &path, &s("content"))],
            _ => vec![],
        };
        ToolUse { tool: tool.into(), command: s("command"), changes }
    }
}

/// The hooks block of Claude Code's settings for an agent: each of its hooks runs `RELAY NAME`, waiting
/// `WAITS_SECONDS` for the answer. The relay reaches the agent's socket by DOCKET_HOOK_SOCKET in its environment.
pub fn hooks_settings(relay: &str) -> Json {
    let mut h = serde_json::Map::new();
    for (name, _) in HOOKS {
        h.insert(name.into(), json!([{"matcher": "", "hooks": [{"type": "command", "command": format!("{relay} {name}"), "timeout": WAITS_SECONDS}]}]));
    }
    json!({"hooks": h})
}

pub mod start;

#[cfg(test)]
mod tests;
