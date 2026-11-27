//! agy in its own terms (std/bridges/agy.docket): the names agy-rpc relays its moments by, its answer to each
//! HookAction its bridge lets, and a tool call's parts.
//!
//! agy runs no hook of ours: agy-rpc follows it, through t3code 0.1.11 or over ACP, and runs the relay at each
//! moment. Its answers are agy-rpc's to give the harness: `{"decline": REASON}` refuses the call asked about
//! (t3code's `decline`, ACP's `reject_once`); `{"answer": "once"}` or `{"answer": "decline"}` answers the question
//! (t3code's `accept` or `decline`, ACP's `allow_once` or `reject_once`). agy-rpc reads nothing else: no answer to
//! the question, or one it cannot read, declines it (nobody is at agy's own form), and an answer it cannot read at
//! `tool_called` refuses the call.

use docket_hooks::frame::{Native, Out, ToolUse};
use docket_hooks::{patch, Action};
use docket_v2::run::value::Value;
use serde_json::{json, Value as Json};

pub const HARNESS: &str = "agy";
/// Its two ways of running, as its bridge's versions name them.
pub const VERSIONS: [&str; 2] = ["t3code-0.1.11", "agy-1.5.18"];

/// The names agy-rpc runs the relay under, each the moment it reaches: its bridge's `Native-Hook` facts.
pub const HOOKS: [(&str, &str); 6] = [
    ("turn_begins", "turn_begins"),
    ("tool_called", "tool_called"),
    ("tool_result", "tool_result"),
    ("permission_asked", "permission_asked"),
    ("turn_end", "turn_end"),
    ("overloaded", "overloaded"),
];

/// agy run the way `version` names, one of [`VERSIONS`].
pub struct Agy {
    pub version: &'static str,
}

impl Native for Agy {
    fn name(&self) -> Vec<Value> {
        vec![Value::text(HARNESS), Value::text(self.version)]
    }

    fn hooks(&self) -> Vec<(&'static str, &'static str)> {
        HOOKS.to_vec()
    }

    fn answer(&self, moment: &str, action: &Action) -> Result<Out, String> {
        use Action::*;
        Ok(match (moment, action) {
            (_, Allow) => Out { stdout: String::new(), code: 0 },
            ("tool_called", Deny(r)) => Out::json(json!({"decline": r})),
            ("permission_asked", Answer(a)) if a == "allow" => Out::json(json!({"answer": "once"})),
            ("permission_asked", Answer(a)) if a == "deny" => Out::json(json!({"answer": "decline"})),
            ("permission_asked", Answer(a)) => return Err(format!("a permission is answered allow or deny, not {a:?}")),
            (m, a) => return Err(format!("agy {} has no answer {a:?} at {m}", self.version)),
        })
    }

    /// agy-rpc gives a call asked about with its tool and input as Claude Code names them (`Bash`, `Edit`, `Grep`,
    /// `WebSearch`) and the tool's own working folder, and a call done with its name, and its input when it ran. Every
    /// file change comes as `Edit`, once for each file it names, agy's own tools' too (`write_to_file`,
    /// `replace_file_content`, `apply_patch`, …): a new file is an `Edit` with no old text, read as a create. A call
    /// under any other name but a read's that still names files where agy's tools name them ([`FILES_IN`]) is read as a
    /// change of each, so a guard on files sees it.
    fn tool_use(&self, input: &str) -> ToolUse {
        let Ok(v) = serde_json::from_str::<Json>(input) else { return ToolUse::default() };
        let (i, cwd) = (&v["tool_input"], v["cwd"].as_str().unwrap_or_default());
        let s = |k: &str| i[k].as_str().unwrap_or_default().to_string();
        match v["tool_name"].as_str().unwrap_or_default() {
            "Bash" => ToolUse::command(&s("command")),
            "Edit" => ToolUse::edit(vec![patch::edit(cwd, &s("file_path"), &s("old_string"), &s("new_string"))]),
            "" => ToolUse::default(),
            read @ ("Read" | "Grep" | "Glob" | "WebFetch" | "WebSearch") => ToolUse::named(read),
            other => match files(i) {
                f if f.is_empty() => ToolUse::named(other),
                f => ToolUse::edit(f.iter().map(|p| patch::edit(cwd, p, "", "")).collect()),
            },
        }
    }
}

/// Where agy's tools name the files they touch (agy 1.5.18 reads `Path`, `AbsolutePath`, `TargetFile`; a move its
/// source and destination), and ACP's and Claude Code's own names, as agy-rpc's `PATH_IN`.
const FILES_IN: [&str; 13] =
    ["TargetFile", "AbsolutePath", "FilePath", "Path", "SourceFile", "SourcePath", "DestinationPath", "TargetPath", "TargetFiles", "file_path", "filePath", "path", "paths"];

/// The files a call's input I names ([`FILES_IN`]), each a path or a list of them, once each in the order named.
fn files(i: &Json) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for k in FILES_IN {
        let named: Vec<&str> = match &i[k] {
            Json::String(p) => vec![p.as_str()],
            Json::Array(ps) => ps.iter().filter_map(Json::as_str).collect(),
            _ => vec![],
        };
        for p in named.into_iter().map(str::trim).filter(|p| !p.is_empty()) {
            if !out.iter().any(|o| o == p) {
                out.push(p.to_string());
            }
        }
    }
    out
}

pub mod start;

#[cfg(test)]
mod tests;
