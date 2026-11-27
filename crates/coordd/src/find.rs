//! `coord find` beyond the tickets (the owner N-308.p3: "any relvant fining ealrier claude chats messgaes form me";
//! paper-research N-101 p1026): the mail coordd holds (the owner's messages from before tickets, by their old ids) and
//! the conversations the registry names, read on the host, off the lock. Each hit is the passage that holds the words,
//! who wrote it, when, and where it is; no transcript folder is bound into a box for it.

use crate::core::Mail;
use coord_types::FindHit;
use std::io::BufRead;

/// The passages of MAILS and of SESSIONS (each agent's transcripts) that hold every one of WORDS (lowercased).
pub fn hits(words: &[String], mails: &[Mail], sessions: &[(String, Vec<String>)]) -> Vec<FindHit> {
    let mut out = Vec::new();
    for m in mails {
        if let Some(p) = passage(&m.body, words) {
            out.push(FindHit { id: m.id.clone(), t: 0, post: None, title: format!("mail to {}", m.to.join(", ")), by: m.from.clone(), at: m.at.clone(), words: p, star: None, archived: false });
        }
    }
    for (name, files) in sessions {
        for f in files {
            in_session(name, f, words, &mut out);
        }
    }
    out
}

/// The hits in session file FILE of agent NAME: what was said in it (the agent's words, and what it was given), not
/// its tools' calls or results.
fn in_session(name: &str, file: &str, words: &[String], out: &mut Vec<FindHit>) {
    let Ok(f) = std::fs::File::open(file) else { return };
    let stem: String = std::path::Path::new(file).file_stem().map(|s| s.to_string_lossy().chars().take(8).collect()).unwrap_or_default();
    let mut offset = 0u64;
    for line in std::io::BufReader::new(f).split(b'\n') {
        let Ok(line) = line else { break };
        let at = offset;
        offset += line.len() as u64 + 1;
        // most lines hold none of the words: looked at as bytes before they are parsed
        if !coord_types::holds_words(&String::from_utf8_lossy(&line), words) {
            continue;
        }
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&line) else { continue };
        for e in crate::transcript::said(at, &v) {
            let Some(p) = passage(unwrapped(&e.text), words) else { continue };
            let by = match e.kind.as_str() {
                "assistant" => name.to_string(),
                _ => from_of(&e.text).unwrap_or_else(|| format!("to {name}")),
            };
            out.push(FindHit { id: format!("{name}.{stem}@{at}"), t: 0, post: None, title: format!("{name}'s conversation"), by, at: e.at.clone(), words: p, star: None, archived: false });
        }
    }
}

/// TEXT without a coord channel's tag around it.
fn unwrapped(text: &str) -> &str {
    let t = text.trim();
    match t.strip_prefix("<channel source=\"coord\"").and_then(|r| r.split_once('>')) {
        Some((_, body)) => body.trim_end().strip_suffix("</channel>").unwrap_or(body).trim(),
        None => t,
    }
}

/// Who a message given to an agent is from: a coord channel's `from`, or a Codex turn headed `[ID · from NAME]`.
fn from_of(text: &str) -> Option<String> {
    let t = text.trim_start();
    if let Some(rest) = t.strip_prefix("<channel source=\"coord\"") {
        let tag = rest.split('>').next()?;
        let from = tag.split("from=\"").nth(1)?.split('"').next()?;
        return Some(from.to_string());
    }
    let head = t.strip_prefix('[')?.split(']').next()?;
    head.split(" · from ").nth(1).map(|n| n.trim().to_string())
}

/// The paragraph of TEXT that holds the first of WORDS, when TEXT holds every one; cut to about 800 characters
/// around it.
pub fn passage(text: &str, words: &[String]) -> Option<String> {
    if words.is_empty() || !coord_types::holds_words(text, words) {
        return None;
    }
    let paras: Vec<&str> = text.split("\n\n").collect();
    let p = paras.iter().find(|p| p.to_lowercase().contains(words[0].as_str())).unwrap_or(&paras[0]).trim();
    let n = p.chars().count();
    if n <= 800 {
        return Some(p.to_string());
    }
    let at = p.to_lowercase().find(words[0].as_str()).unwrap_or(0);
    let before = p[..at.min(p.len())].chars().count();
    let from = before.saturating_sub(300);
    let s: String = p.chars().skip(from).take(800).collect();
    Some(format!("{}{s}{}", if from > 0 { "…" } else { "" }, if from + 800 < n { "…" } else { "" }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(q: &str) -> Vec<String> {
        coord_types::query_words(q)
    }

    /// The owner's mail and an agent's conversation are found by their words, each hit with who, when and where.
    #[test]
    fn the_mail_and_the_conversations_are_searched() {
        let dir = std::env::temp_dir().join(format!("docket-find-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("c38e0f9d-6cad.jsonl");
        let lines = [
            serde_json::json!({"type": "user", "timestamp": "2026-11-02T10:00:00Z", "message": {"content": "<channel source=\"coord\" from=\"owner\">\nNo patches to these tools.\n\nWhere no stock route exists, a verdict-only claim.\n</channel>"}}),
            serde_json::json!({"type": "assistant", "timestamp": "2026-11-02T10:01:00Z", "message": {"content": [{"type": "text", "text": "I will give a verdict-only claim for nuXmv."}, {"type": "tool_use", "name": "Bash", "input": {"command": "echo verdict-only claim"}}]}}),
        ];
        std::fs::write(&file, lines.iter().map(|l| l.to_string() + "\n").collect::<String>()).unwrap();
        let mail = Mail { id: "m13298".into(), at: "2026-10-01T09:00:00Z".into(), from: "owner".into(), to: vec!["orch".into()], body: "what I don't want is patches to these tools".into(), esc: false, soon: false, kind: String::new(), re: None, quotes: vec![] };
        let hits = hits(&w("verdict-only CLAIM"), &[mail.clone()], &[("designer".into(), vec![file.display().to_string()])]);
        let shown: Vec<(&str, &str, &str)> = hits.iter().map(|h| (h.by.as_str(), h.at.as_str(), h.words.as_str())).collect();
        assert_eq!(shown, vec![("owner", "2026-11-02T10:00:00Z", "Where no stock route exists, a verdict-only claim."), ("designer", "2026-11-02T10:01:00Z", "I will give a verdict-only claim for nuXmv.")]);
        assert!(hits[0].id.starts_with("designer.c38e0f9d@0") && hits[1].id.starts_with("designer.c38e0f9d@"), "{hits:?}");
        let hits = super::hits(&w("patches tools"), &[mail], &[]);
        assert!(matches!(&hits[..], [h] if h.id == "m13298" && h.by == "owner" && h.title == "mail to orch"), "{hits:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A session the registry names under a folder that moved (Tovrin's container path) is read where `sessions-moved`
    /// says it is now (docket-core2's map, core.rs `moved_transcript`), so `coord find` reaches it among the sessions;
    /// one not there stays as named, and the registry is not rewritten.
    #[test]
    fn a_session_named_where_it_no_longer_is_is_read_where_it_moved() {
        let coord = std::env::temp_dir().join(format!("docket-find-moved-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        let now = coord.join("claude-projects/-home-agent");
        std::fs::create_dir_all(&now).unwrap();
        std::fs::write(now.join("aaa.jsonl"), "{}\n").unwrap();
        let old = "/nonexistent-container/home/agent/.claude/projects";
        let reg = [
            serde_json::json!({"ev": "add", "id": "AGT-001", "name": "orch", "parent": "owner", "kind": "claude", "model": "m", "at": "x"}),
            serde_json::json!({"ev": "session", "name": "orch", "session": {"transcript_path": format!("{old}/-home-agent/aaa.jsonl")}, "at": "x"}),
            serde_json::json!({"ev": "add", "id": "AGT-002", "name": "cone", "parent": "owner", "kind": "claude", "model": "m", "at": "x"}),
            serde_json::json!({"ev": "session", "name": "cone", "session": {"transcript_path": format!("{old}/-home-agent/gone.jsonl")}, "at": "x"}),
        ];
        let registry = reg.iter().map(|l| l.to_string() + "\n").collect::<String>();
        std::fs::write(coord.join("registry.jsonl"), &registry).unwrap();
        std::fs::write(coord.join("sessions-moved"), format!("# Tovrin's container\n{old} {}\n", coord.join("claude-projects").display())).unwrap();
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let core = crate::core::Core::load(&coord.display().to_string(), &base);
        let mut s = core.sessions();
        s.sort();
        assert_eq!(s, vec![("cone".to_string(), vec![format!("{old}/-home-agent/gone.jsonl")]), ("orch".to_string(), vec![now.join("aaa.jsonl").display().to_string()])]);
        assert_eq!(core.agents["orch"].transcript(), Some(now.join("aaa.jsonl").display().to_string()));
        assert_eq!(std::fs::read_to_string(coord.join("registry.jsonl")).unwrap(), registry);
        let _ = std::fs::remove_dir_all(&coord);
    }
}
