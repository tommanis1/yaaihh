//! Codex's own models: those its catalog lists, while the coord's Codex is signed in.

use serde_json::Value;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

/// How long one of Codex's commands may take, in seconds.
const BOUND: &str = "20";

/// Whether the log was told the coord has no Codex: once a process, not at every model list.
static NO_CODEX_SAID: AtomicBool = AtomicBool::new(false);

/// The models Codex's catalog lists to pick from (`codex debug models`, visibility `list`), with Codex PROGRAM at the
/// coord's CODEX_HOME (COORD's `codex-home`, as roster runs it), while `codex login status` says it is signed in. None
/// when it has no such folder or is not signed in; a Codex that is not there, or a catalog that does not read, says why on
/// stderr. Blocking.
pub fn models(program: &Path, coord: &str) -> Vec<String> {
    let home = Path::new(coord).join("codex-home");
    if !home.is_dir() {
        return Vec::new();
    }
    let run = |args: &[&str]| Command::new("timeout").arg(BOUND).arg(program).args(args).env("CODEX_HOME", &home).stdin(Stdio::null()).output();
    match run(&["login", "status"]) {
        Ok(o) if o.status.success() => {}
        // not signed in is no fault; a Codex that is not there (127, bin/codex says where) is, once in the log
        Ok(o) => {
            if o.status.code() == Some(127) && !NO_CODEX_SAID.swap(true, Ordering::Relaxed) {
                eprintln!("coordd: {}", String::from_utf8_lossy(&o.stderr).trim());
            }
            return Vec::new();
        }
        Err(e) => {
            eprintln!("coordd: codex login status: {e}");
            return Vec::new();
        }
    }
    match run(&["debug", "models"]).map_err(|e| e.to_string()).and_then(|o| if o.status.success() { listed(&o.stdout) } else { Err(format!("exit {}", o.status)) }) {
        Ok(ids) => ids,
        Err(why) => {
            eprintln!("coordd: codex debug models: {why}");
            Vec::new()
        }
    }
}

/// The ids CATALOG lists to pick from, in its order.
fn listed(catalog: &[u8]) -> Result<Vec<String>, String> {
    let v: Value = serde_json::from_slice(catalog).map_err(|e| e.to_string())?;
    let models = v["models"].as_array().ok_or("no models")?;
    Ok(models.iter().filter(|m| m["visibility"] == "list").filter_map(|m| m["slug"].as_str().map(str::to_string)).collect())
}
