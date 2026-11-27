//! opencode 1.18.34 in its own terms (std/bridges/opencode.docket): its plugin's hooks and events by name, its
//! plugin's answer to each HookAction its bridge lets, and a tool call's parts read from its input.
//!
//! What its plugin can do at each was read in opencode 1.18.34's own program text: `chat.message`,
//! `tool.execute.before`, `tool.execute.after` and `experimental.session.compacting` are awaited and their output
//! object is used after them (the message's parts, the call's args, the result, the context and prompt), and a
//! throw in `tool.execute.before` fails the call; `permission.asked`, `session.idle` and `session.error` are events
//! it only tells, and a permission request waits on its server until a reply to `/permission/ID/reply` answers it.

use docket_hooks::frame::{Native, Out, ToolUse};
use docket_hooks::patch;
use docket_hooks::Action;
use docket_v2::run::value::Value;
use serde_json::{json, Value as Json};

pub const HARNESS: &str = "opencode";
pub const VERSION: &str = "1.18.34";

/// Its plugin's hooks and events, each with the moment it reaches: its bridge's `Native-Hook` facts. Its plugin,
/// agent-base's opencode/plugin/relay.js, runs the relay under these names and applies each answer as given.
pub const HOOKS: [(&str, &str); 7] = [
    ("chat.message", "turn_begins"),
    ("tool.execute.before", "tool_called"),
    ("tool.execute.after", "tool_result"),
    ("permission.asked", "permission_asked"),
    ("session.idle", "turn_end"),
    ("experimental.session.compacting", "compacting"),
    ("session.error", "overloaded"),
];

pub struct Opencode;

/// A Modify's patch: the keys to set on the hook's output, as a JSON object.
fn keys(patch: &str) -> Result<Json, String> {
    match serde_json::from_str::<Json>(patch) {
        Ok(v @ Json::Object(_)) => Ok(v),
        _ => Err("a Modify for opencode is the keys to set on its hook's output, as a JSON object".into()),
    }
}

impl Native for Opencode {
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
            ("tool_called", Deny(r)) => Out::json(json!({"deny": r})),
            ("turn_begins" | "tool_called" | "tool_result" | "compacting", Modify(p)) => Out::json(json!({"modify": keys(p)?})),
            // once, never always: always would let the calls after it go unasked
            ("permission_asked", Answer(a)) if a == "allow" => Out::json(json!({"reply": "once"})),
            ("permission_asked", Answer(a)) if a == "deny" => Out::json(json!({"reply": "reject"})),
            ("permission_asked", Answer(a)) => return Err(format!("a permission is answered allow or deny, not {a:?}")),
            (m, a) => return Err(format!("opencode {VERSION} has no answer {a:?} at {m}")),
        })
    }

    /// The relay's input is the plugin's `{input, output, directory}`: the tool in `input.tool`, its arguments in
    /// `output.args`, as opencode names them, and the agent's working folder.
    fn tool_use(&self, input: &str) -> ToolUse {
        let Ok(v) = serde_json::from_str::<Json>(input) else { return ToolUse::default() };
        let (a, cwd) = (&v["output"]["args"], v["directory"].as_str().unwrap_or_default());
        let s = |k: &str| a[k].as_str().unwrap_or_default().to_string();
        let path = s("filePath");
        match v["input"]["tool"].as_str().unwrap_or_default() {
            "bash" => ToolUse::command(&s("command")),
            "edit" => ToolUse::edit(vec![patch::edit(cwd, &path, &s("oldString"), &s("newString"))]),
            "write" => ToolUse::edit(vec![patch::write(cwd, &path, &s("content"))]),
            "apply_patch" => ToolUse::edit(patch::apply_patch(&s("patchText"), cwd)),
            "" => ToolUse::default(),
            other => ToolUse::named(other),
        }
    }
}

pub mod start;

#[cfg(test)]
mod tests;
