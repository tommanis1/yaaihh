//! The places never read nor written, whatever a request names: a coord's credentials, the owner's logins and
//! settings, a repository's own folder. The one file naming them: coordd's Files page and its kept graph refuse by it too.

use std::path::{Component, Path};

/// Why P (links followed) is never read nor written, for the owner's HOME (links followed); None when it may be.
pub fn never_served(home: &Path, p: &Path) -> Option<String> {
    let names: Vec<&std::ffi::OsStr> = p.components().filter_map(|c| if let Component::Normal(n) = c { Some(n) } else { None }).collect();
    let is = |n: &std::ffi::OsStr, s: &str| n == std::ffi::OsStr::new(s);
    if names.iter().any(|n| is(n, ".git")) {
        return Some("a repository's own folder".into());
    }
    // a coord's keys and homes, the gateway's usage, and what is named from one (a writer's `.proxy-key.new.XXXXXX`)
    let named_from = |n: &std::ffi::OsStr, s: &str| n.as_encoded_bytes().starts_with(s.as_bytes());
    let kept = [".proxy-key", "telemetry.token", "envdir", "claude-home", "codex-home", "claude-settings.json", "usage.db"];
    if names.iter().any(|n| kept.iter().any(|s| named_from(n, s))) {
        return Some("a coord's credentials".into());
    }
    // `route` only in a coord's own folder, which holds registry.jsonl: a file of that name elsewhere is no key
    let coords = home.join("agent-coord");
    for a in p.ancestors() {
        if a.file_name().is_some_and(|n| is(n, "route")) && a.parent().is_some_and(|c| c.parent() == Some(coords.as_path()) || c.join("registry.jsonl").exists()) {
            return Some("a coord's credentials".into());
        }
    }
    if names.windows(3).any(|w| is(w[0], "box") && is(w[1], "run") && is(w[2], "ffprof")) {
        return Some("the browser's profile".into());
    }
    if let Ok(rest) = p.strip_prefix(home) {
        let first = rest.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned()).unwrap_or_default();
        if first.starts_with(".claude") || [".codex", ".ssh", ".config"].contains(&first.as_str()) || rest.starts_with(".local/opt") || rest.starts_with("agent-coord/accounts") {
            return Some("the owner's logins and settings".into());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every place v1 keeps closed, under a scratch home; what is beside them is not.
    #[test]
    fn each_kept_out_place_is_refused_and_what_is_beside_it_is_not() {
        let h = std::env::temp_dir().join(format!("machine-state-kept-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&h);
        std::fs::create_dir_all(h.join("agent-coord/p")).unwrap();
        std::fs::write(h.join("agent-coord/p/registry.jsonl"), "").unwrap();
        let logins = [".claude", ".claude-2/x", ".codex/x", ".ssh", ".config/x", ".local/opt/x", "agent-coord/accounts/a1"];
        let keys = ["agent-coord/p/envdir", "elsewhere/usage.db", "agent-coord/p/claude-settings.json", "agent-coord/p/codex-home", "agent-coord/p/claude-home", "agent-coord/p/.proxy-key.new.ab12", "agent-coord/p/telemetry.token", "agent-coord/p/route"];
        for (said, places) in [("the owner's logins and settings", &logins[..]), ("a coord's credentials", &keys[..]), ("a repository's own folder", &["src/r/.git"][..])] {
            for p in places {
                assert_eq!(never_served(&h, &h.join(p)).as_deref(), Some(said), "{p}");
            }
        }
        for p in ["src/r", "src/route", ".local/share", "agent-coord/p/work", "agent-coord"] {
            assert_eq!(never_served(&h, &h.join(p)), None, "{p}");
        }
        let _ = std::fs::remove_dir_all(&h);
    }
}
