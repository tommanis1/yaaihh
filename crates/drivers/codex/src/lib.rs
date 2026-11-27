//! Codex 0.160.0 in its own terms (std/bridges/codex.docket): its hook names, its answer to each HookAction its
//! bridge lets, and a tool call's parts read from its input.
//!
//! The answers' shapes were read in Codex 0.160.0's own schema for each hook's output: `decision` (`block`) and
//! `reason` at the top; under `hookSpecificOutput`, PreToolUse's `permissionDecision` and
//! `permissionDecisionReason`, UserPromptSubmit's, PreToolUse's and PostToolUse's `additionalContext`, and
//! PermissionRequest's `decision` with `behavior` allow or deny.

use docket_hooks::frame::{Native, Out, ToolUse};
use docket_hooks::{patch, Action};
use docket_v2::run::value::Value;
use serde_json::{json, Value as Json};

pub const HARNESS: &str = "codex";
pub const VERSION: &str = "0.160.0";

/// Its hooks, as its hooks file registers them, each with the moment it reaches: its bridge's `Native-Hook` facts.
/// `failed` is no hook of Codex's: its app-server tells only a connection that a turn failed, and codex-rpc, which
/// follows that connection, runs the relay under this name.
pub const HOOKS: [(&str, &str); 7] = [
    ("UserPromptSubmit", "turn_begins"),
    ("PreToolUse", "tool_called"),
    ("PostToolUse", "tool_result"),
    ("PermissionRequest", "permission_asked"),
    ("Stop", "turn_end"),
    ("PreCompact", "compacting"),
    ("failed", "overloaded"),
];

pub struct Codex;

impl Native for Codex {
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
            ("tool_called", Inject(t)) => Out::specific("PreToolUse", json!({"additionalContext": t})),
            ("tool_result", Inject(t)) => Out::specific("PostToolUse", json!({"additionalContext": t})),
            ("permission_asked", Answer(a)) if a == "allow" || a == "deny" => Out::specific("PermissionRequest", json!({"decision": {"behavior": a}})),
            ("permission_asked", Answer(a)) => return Err(format!("a permission is answered allow or deny, not {a:?}")),
            ("turn_end", KeepGoing(p)) => Out::json(json!({"decision": "block", "reason": p})),
            (m, a) => return Err(format!("Codex {VERSION} has no answer {a:?} at {m}")),
        })
    }

    /// Its shell tool reaches its hooks as `Bash` with the command line in `tool_input.command`, and an edit as
    /// `apply_patch` with the patch's text there, its paths relative to the hook's `cwd` (seen on loopback).
    fn tool_use(&self, input: &str) -> ToolUse {
        let Ok(v) = serde_json::from_str::<Json>(input) else { return ToolUse::default() };
        let text = v["tool_input"]["command"].as_str().unwrap_or_default();
        match v["tool_name"].as_str().unwrap_or_default() {
            "Bash" => ToolUse::command(text),
            "apply_patch" => ToolUse::edit(patch::apply_patch(text, v["cwd"].as_str().unwrap_or_default())),
            "" => ToolUse::default(),
            other => ToolUse::named(other),
        }
    }
}

pub mod start;

#[cfg(test)]
mod tests;
