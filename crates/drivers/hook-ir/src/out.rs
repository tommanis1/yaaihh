//! The answers a harness reads as JSON on the relay's output, built once for every driver.

use crate::frame::Out;
use serde_json::{json, Value as Json};

impl Out {
    /// `v` printed for the harness, with exit code 0.
    pub fn json(v: Json) -> Out {
        Out { stdout: v.to_string(), code: 0 }
    }

    /// `fields` under `hookSpecificOutput`, named for hook `event`: the shape Claude Code's and Codex's hooks share.
    pub fn specific(event: &str, mut fields: Json) -> Out {
        fields["hookEventName"] = json!(event);
        Out::json(json!({"hookSpecificOutput": fields}))
    }
}
