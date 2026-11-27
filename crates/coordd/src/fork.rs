//! A pattern's fork (N-101; docket-machine's `Effect::Fork`): agent FROM's conversation, copied as it stands now, put
//! where the new agent's `claude --resume SID --fork-session` finds it (main.rs `Job::Start`, then `roster fork`).
//!
//! Claude Code finds a session only under its working directory's projects entry. So the copy goes:
//! - in a layer, to `REPO/.conversations/claude/SID.jsonl`. That is the project's real conversations folder, bound into
//!   every view of the project (layer.rs), and every projects entry in a view links to it: the owner's when not
//!   boxed, the agent's own home's when boxed (sandbox::home). Boxed, the copy is the new agent's uid's and its alone:
//!   the copy in its own home at creation (paper-research, N-101 post 11).
//! - outside a layer, to `CLAUDE/projects/ENTRY/SID.jsonl`, ENTRY that of the folder it runs in and CLAUDE the folder
//!   its Claude Code runs with ([`claude_dir`]): the project's own `claude-home` when it has one, else `HOME/.claude`.
//!   When that is where FROM's transcript is already, nothing is copied.
//! A copy already there (an earlier fork of FROM to the same place) is replaced by this one, which is newer.
//!
//! In a boxed project every layer's view binds the same conversations folder, so a boxed FROM's transcript is already
//! where a boxed fork looks: nothing is copied, and the fork reads FROM's own file (0644, the box group's), not a copy
//! of its own. That rests on one assumption (paper-research, N-101 post 56): `claude --resume SID --fork-session` only
//! reads SID's file and appends to a new one under the new session id; nothing writes to FROM's file but FROM. Should
//! Claude Code ever write to the resumed file, a boxed fork must get a private copy under a new name instead.

use std::path::Path;

/// Where the fork's copy of session SID goes, for an agent running in DIR (in a layer: the project's path) whose Claude
/// Code runs with folder CLAUDE ([`claude_dir`]).
pub fn place(dir: &str, layered: bool, claude: &str, sid: &str) -> String {
    if layered {
        format!("{dir}/.conversations/claude/{sid}.jsonl")
    } else {
        let entry: String = dir.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
        format!("{claude}/projects/{entry}/{sid}.jsonl")
    }
}

/// The folder an unboxed agent's Claude Code runs with, as roster gives it (`own_home`; paper-research N-1446 p3051):
/// the project's own `COORD/claude-home` when it has one and the agent is on none of the owner's accounts, else
/// `HOME/.claude` (an account's folder links its `projects` there, accounts/mod.rs).
pub fn claude_dir(coord: &str, home: &str, on_account: bool) -> String {
    let own = format!("{coord}/claude-home");
    if !on_account && Path::new(&own).is_dir() { own } else { format!("{home}/.claude") }
}

/// Copy transcript FROM to TO: written beside TO and renamed onto it. With K (boxed), as root of a namespace with the
/// box's map (sandbox::in_ns), so a transcript a sub-uid wrote is read, and the copy is K's own (0600). Nothing when
/// FROM is TO already. Returns whether a copy was made.
pub fn copy(from: &str, to: &str, k: Option<u32>) -> Result<bool, String> {
    if same(from, to) {
        return Ok(false);
    }
    match k {
        Some(k) => {
            let script = r#"set -e
mkdir -p "$(dirname "$2")"
cp "$1" "$2.fork"
chown "$3:$3" "$2.fork"
chmod 600 "$2.fork"
mv -f "$2.fork" "$2""#;
            crate::sandbox::in_ns(script, &[from, to, &k.to_string()]).map_err(|e| format!("copying {from} to {to}: {e}"))?;
        }
        None => {
            let tmp = format!("{to}.fork");
            if let Some(d) = Path::new(to).parent() {
                std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
            }
            std::fs::copy(from, &tmp).and_then(|_| std::fs::rename(&tmp, to)).map_err(|e| format!("copying {from} to {to}: {e}"))?;
        }
    }
    Ok(true)
}

/// Where a copy of a session cuts transcript FILE so that it resumes on a settled conversation (the owner's N-1794:
/// "support forking a busy agent cleanly"): the end of its last whole line after which every tool call of the main
/// conversation has had its result. A copy of an agent in the middle of a call, cut at the file's end, resumes on a
/// `tool_use` with no `tool_result`, which the model's API refuses; cut here it resumes from just before that call, with
/// all the turn had done until then. An idle agent's settles at the file's end, so its copy is the whole conversation.
/// A line being written (no newline yet) is not counted; a subagent's lines (`isSidechain`) open and close nothing
/// here. None when FILE cannot be read or no line settles.
pub fn settled_end(file: &str) -> Option<u64> {
    use std::io::BufRead;
    let mut f = std::io::BufReader::new(std::fs::File::open(file).ok()?);
    let (mut at, mut settled) = (0u64, None);
    let mut open: std::collections::BTreeSet<String> = Default::default();
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = f.read_until(b'\n', &mut line).ok()?;
        if n == 0 || line.last() != Some(&b'\n') {
            return settled;
        }
        at += n as u64;
        if let Ok(e) = serde_json::from_slice::<serde_json::Value>(&line) {
            if e["isSidechain"] != true {
                for c in e["message"]["content"].as_array().into_iter().flatten() {
                    match (c["type"].as_str(), c["id"].as_str(), c["tool_use_id"].as_str()) {
                        (Some("tool_use"), Some(id), _) => {
                            open.insert(id.to_string());
                        }
                        (Some("tool_result"), _, Some(id)) => {
                            open.remove(id);
                        }
                        _ => {}
                    }
                }
            }
        }
        if open.is_empty() {
            settled = Some(at);
        }
    }
}

/// Whether A and B are one file (through links); false when either is not there.
fn same(a: &str, b: &str) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_copy_goes_where_the_forks_session_is_found() {
        assert_eq!(place("/p/tovrin", true, "/h/.claude", "s1"), "/p/tovrin/.conversations/claude/s1.jsonl");
        assert_eq!(place("/p/tovrin.x", false, "/h/.claude", "s1"), "/h/.claude/projects/-p-tovrin-x/s1.jsonl");
        // a project with its own Claude Code folder: there, unless the agent is on one of the owner's accounts
        let t = std::env::temp_dir().join(format!("fork-claude-dir-{}", std::process::id()));
        let coord = t.display().to_string();
        assert_eq!(claude_dir(&coord, "/h", false), "/h/.claude", "no claude-home: the owner's, as before");
        std::fs::create_dir_all(t.join("claude-home")).unwrap();
        assert_eq!(claude_dir(&coord, "/h", false), format!("{coord}/claude-home"));
        assert_eq!(claude_dir(&coord, "/h", true), "/h/.claude", "an account's folder links its projects to the owner's");
        assert_eq!(place("/w", false, &claude_dir(&coord, "/h", false), "s1"), format!("{coord}/claude-home/projects/-w/s1.jsonl"));
        std::fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn a_busy_session_is_cut_where_every_call_has_its_result() {
        let t = std::env::temp_dir().join(format!("coordd-settled-{}.jsonl", std::process::id()));
        let file = t.display().to_string();
        let user = r#"{"type":"user","message":{"role":"user","content":"go"}}"#;
        let said = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"looking"}]}}"#;
        let call = |id: &str| format!(r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","id":"{id}","name":"Bash","input":{{}}}}]}}}}"#);
        let result = |id: &str| format!(r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"{id}","content":"ok"}}]}}}}"#);
        let side = r#"{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"tool_use","id":"s1","name":"Read","input":{}}]}}"#;
        let write = |lines: &[&str], tail: &str| std::fs::write(&t, lines.iter().map(|l| format!("{l}\n")).collect::<String>() + tail).unwrap();
        let end = |lines: &[&str]| lines.iter().map(|l| l.len() as u64 + 1).sum::<u64>();
        // idle: the whole conversation
        let (c1, r1) = (call("t1"), result("t1"));
        let idle = [user, said, &c1, &r1, said];
        write(&idle, "");
        assert_eq!(settled_end(&file), Some(end(&idle)));
        // in the middle of a call: cut before the call, with what the turn did until then
        let busy = [user, said, &c1, &r1, said, &call("t2")];
        write(&busy, "");
        assert_eq!(settled_end(&file), Some(end(&busy[..5])));
        // a line being written counts for nothing; a subagent's open call keeps nothing open
        let (c2, r2) = (call("t2"), result("t2"));
        let done = [user, &c2, side, &r2];
        write(&done, r#"{"type":"assistant","mess"#);
        assert_eq!(settled_end(&file), Some(end(&done)));
        // two calls at once, answered one by one: settled only once both have their results
        let (c3, r3) = (call("t3"), result("t3"));
        let two = [user, &c2, &c3, &r2];
        write(&two, "");
        assert_eq!(settled_end(&file), Some(end(&two[..1])));
        let both = [user, &c2, &c3, &r2, &r3];
        write(&both, "");
        assert_eq!(settled_end(&file), Some(end(&both)));
        std::fs::remove_file(&t).unwrap();
        assert_eq!(settled_end(&file), None);
    }

    #[test]
    fn a_copy_is_made_replaced_and_never_made_onto_its_own_source() {
        let t = std::env::temp_dir().join(format!("coordd-fork-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        std::fs::create_dir_all(&t).unwrap();
        let src = t.join("a.jsonl").display().to_string();
        std::fs::write(&src, "{\"n\":1}\n").unwrap();
        let to = t.join("view/.conversations/claude/a.jsonl").display().to_string();
        assert_eq!(copy(&src, &to, None), Ok(true));
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "{\"n\":1}\n");
        // the source grew: a second fork to the same place gets the newer copy
        std::fs::write(&src, "{\"n\":1}\n{\"n\":2}\n").unwrap();
        assert_eq!(copy(&src, &to, None), Ok(true));
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "{\"n\":1}\n{\"n\":2}\n");
        // where the source is already (a link to it, as a projects entry is): nothing copied, nothing lost
        let link = t.join("entry");
        std::os::unix::fs::symlink(&t, &link).unwrap();
        assert_eq!(copy(&src, &link.join("a.jsonl").display().to_string(), None), Ok(false));
        assert_eq!(std::fs::read_to_string(&src).unwrap(), "{\"n\":1}\n{\"n\":2}\n");
        assert!(copy(&t.join("none.jsonl").display().to_string(), &to, None).is_err());
        std::fs::remove_dir_all(&t).unwrap();
    }
}
