//! A Claude Code agent's hooks that need nothing of the core, answered without its lock (paper-research N-788:
//! `request hook` was E3's largest lock wait, 69 a minute at 19 ms each, all of it waiting). For such an agent a
//! hook only wakes the follower, which reads the turn from the transcript (`Core::hook`: "a hook costs nothing under
//! the lock"), except where it decides something: the Agent tool's check (`pre` on Agent or Task), an edit's act
//! (`post` on an editing tool), the account's windows (`statusline`, `failed`, `notification`), and the turn's start
//! and end (`start`, `prompt`, `stop`, `compact`). Those still go through the lock; `pre` and `post` on any other tool,
//! one each per tool call and so most of them, are answered here.
//!
//! What is read here is set by the follow round, under the lock, each second: the follower's wake-up and the names of
//! the agents that are not Codex. A name not yet in it (an agent started this second) goes through the lock, as before.

use serde_json::Value;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

struct Quick {
    poke: Arc<tokio::sync::Notify>,
    claude: HashSet<String>,
}

static QUICK: Mutex<Option<Quick>> = Mutex::new(None);

/// The follow round's view: POKE, and the agents whose hooks are Claude Code's (every kind but Codex, as `Core::hook`
/// takes them).
pub fn set(poke: Arc<tokio::sync::Notify>, claude: impl IntoIterator<Item = String>) {
    *QUICK.lock().unwrap_or_else(|e| e.into_inner()) = Some(Quick { poke, claude: claude.into_iter().collect() });
}

/// Whether agent NAME's hook EVENT with INPUT is one that needs nothing of the core: when it is, the follower has been
/// woken, as `Core::hook` would have, and the hook's answer is the empty one.
pub fn answered(name: &str, event: &str, input: &Value) -> bool {
    let quick = match event {
        "pre" => !matches!(input["tool_name"].as_str(), Some("Agent" | "Task")),
        "post" => crate::runner::edit_of_hook(input).is_none(),
        _ => false,
    };
    if !quick {
        return false;
    }
    let q = QUICK.lock().unwrap_or_else(|e| e.into_inner());
    match q.as_ref() {
        Some(q) if q.claude.contains(name) => {
            q.poke.notify_one();
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_claude_agents_plain_tool_hooks_wake_the_follower_without_the_lock_and_the_deciding_ones_do_not() {
        let poke: Arc<tokio::sync::Notify> = Default::default();
        set(poke.clone(), ["w1".to_string()]);
        let tool = |t: &str| json!({"tool_name": t, "tool_input": {"file_path": "/x/a.rs", "command": "ls"}});
        // pre and post on a tool that decides nothing: answered here, the follower woken
        assert!(answered("w1", "pre", &tool("Bash")));
        assert!(answered("w1", "post", &tool("Bash")));
        assert!(answered("w1", "post", &tool("Read")));
        let woken = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap().block_on(async { tokio::time::timeout(std::time::Duration::from_millis(100), poke.notified()).await.is_ok() });
        assert!(woken, "the follower is woken, as Core::hook wakes it");
        // the Agent tool's check, an edit's act, the account's and the turn's events, and an agent not (yet) known
        // as Claude: through the lock
        assert!(!answered("w1", "pre", &tool("Agent")));
        assert!(!answered("w1", "pre", &tool("Task")));
        assert!(!answered("w1", "post", &tool("Edit")));
        assert!(!answered("w1", "post", &tool("Write")));
        for e in ["start", "prompt", "stop", "compact", "failed", "statusline", "notification"] {
            assert!(!answered("w1", e, &tool("Bash")), "{e}");
        }
        assert!(!answered("codex1", "pre", &tool("Bash")));
        // what Core::hook answers for the ones answered here: the empty answer, after the same wake-up
        assert_eq!(crate::runner::edit_of_hook(&tool("Bash")), None);
    }
}
