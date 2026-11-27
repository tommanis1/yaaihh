//! The raw pane of an agent no gateway's log is named for (fix 5, accounts-lead N-1446 p2446): walk-a's pane said only
//! "not read: the proxy's log is named in the project file, and there is none". coordd now reads the agent's session's
//! own record (`transcript::record_page`) and says over it what is missing, in his words (`Core::record_source`).

use super::{raw_page, Shared};
use crate::core::{Core, Who};
use crate::follow::tests::typed;
use serde_json::json;

/// A coord with no project file of lead, w2 below it and w3 (Claude Code agents), w2's session the made-up record
/// SESSION (one request, failed as walk-a's first turn did: Claude Code's error naming no request), at rest.
fn coord(tag: &str) -> (std::path::PathBuf, Core) {
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let coord = std::env::temp_dir().join(format!("coordd-raw-record-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&coord);
    std::fs::create_dir_all(&coord).unwrap();
    let reg: String = [("AGT-001", "lead", "owner"), ("AGT-002", "w2", "lead"), ("AGT-003", "w3", "owner")].iter().map(|(id, n, p)| format!("{}\n", json!({"ev": "add", "id": id, "name": n, "parent": p, "kind": "claude", "model": "m", "at": "x"}))).collect();
    std::fs::write(coord.join("registry.jsonl"), reg).unwrap();
    let session = coord.join("s-w2.jsonl");
    let lines = [
        typed("api-request-shape", json!({"sessionId": "s-w2", "shapeHash": "sh1", "timestamp": "2026-11-24T10:00:00.000Z", "shape": {"system": [{"type": "text", "text": "You are a fixer."}], "tools": [{"name": "Read", "input_schema": {}}]}})),
        typed("api-request-blob", json!({"sessionId": "s-w2", "hash": "b1".repeat(32), "message": {"role": "user", "content": [{"type": "text", "text": "fix mean"}]}})),
        typed("api-request", json!({"sessionId": "s-w2", "id": "r1", "timestamp": "2026-11-24T10:00:01.000Z", "querySource": "repl_main_thread", "shapeHash": "sh1", "params": {"model": "claude-haiku-5-5", "max_tokens": 100}, "keep": 0, "tail": ["b1".repeat(32)]})),
        json!({"isSidechain": false, "type": "assistant", "message": {"id": "msg_e1", "model": "<synthetic>", "role": "assistant", "content": [{"type": "text", "text": "Not logged in · Please run /login"}], "stop_reason": "stop_sequence"}, "error": "authentication_failed", "isApiErrorMessage": true, "timestamp": "2026-11-24T10:00:02.000Z"}).to_string(),
    ];
    std::fs::write(&session, lines.join("\n") + "\n").unwrap();
    let back = std::time::SystemTime::now() - std::time::Duration::from_secs(10);
    std::fs::File::options().write(true).open(&session).unwrap().set_modified(back).unwrap();
    let mut core = Core::load(&coord.display().to_string(), &base);
    core.load_tickets();
    assert!(core.session(&Who::Owner, "w2", &json!({"session_id": "s-w2", "transcript_path": session.display().to_string()})).ok);
    (coord, core)
}

/// With no project file, w2's raw pane is its session's own record: its request as the proxy's log would show it,
/// failed with Claude Code's own words, and over it what is missing (no gateway log, so no HTTP status). Only those
/// above it read it; with a project file that names a proxy_log, the log is read as before, but for an agent on one of
/// his own accounts.
#[tokio::test]
async fn an_agent_no_gateway_log_is_named_for_reads_its_sessions_own_record_said_so() {
    let (dir, core) = coord("none");
    let shared = Shared::new(core);
    let r = raw_page(&shared, &Who::Owner, "w2".into(), None, None, None).await;
    assert!(r.ok, "{}", r.out);
    let page: proxylog::RawPage = serde_json::from_str(&r.out).unwrap();
    let about = page.about.clone().unwrap_or_default();
    assert!(about.starts_with("No gateway log in this project, so this is the agent's own session record") && about.contains("no HTTP status"), "{about}");
    assert_eq!(page.session, "session s-w2");
    let all = page.lines.join("\n");
    assert!(page.lines[0].ends_with(" · claude-haiku-5-5 · failed (authentication_failed) ══"), "{all}");
    for want in ["[system]\nYou are a fixer.", "[tools] 1", "[messages] 1", "[0 user text]\nfix mean", "[reply error] authentication_failed", "[reply text]\nNot logged in · Please run /login"] {
        assert!(all.contains(want), "{want:?} in {all}");
    }
    // a turn's span reads it too, said the same
    let r = raw_page(&shared, &Who::Owner, "w2".into(), None, None, Some((0, u64::MAX))).await;
    let turn: proxylog::RawPage = serde_json::from_str(&r.out).unwrap();
    assert_eq!((turn.heads.len(), turn.about.is_some()), (1, true), "{turn:?}");
    // w3 is not above w2: refused as before
    let r = raw_page(&shared, &Who::Agent("w3".into()), "w2".into(), None, None, None).await;
    assert!(!r.ok && r.out.contains("only the agents above w2"), "{}", r.out);
    // a project file that names no proxy_log: the same words; one that names it: its log, as before, not the record
    let loaded = |toml: &str| {
        std::fs::write(dir.join("project.toml"), toml).unwrap();
        std::fs::write(dir.join("project"), format!("{}\n", dir.join("project.toml").display())).unwrap();
        let mut core = Core::load(&dir.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
        core.project = crate::project::Project::load(&dir.display().to_string()).expect("the project file");
        assert!(core.project.is_some(), "the project file read");
        assert!(core.session(&Who::Owner, "w2", &json!({"session_id": "s-w2", "transcript_path": dir.join("s-w2.jsonl").display().to_string()})).ok);
        core
    };
    let about = loaded("[project]\nnotes = \"walk\"\n").record_source(&Who::Owner, "w2").map(|(_, about)| about).unwrap_or_default();
    assert_eq!(Some(about), page.about, "a project file that names none: the same words");
    assert_eq!(loaded("[project]\nproxy_log = \"/nonexistent/proxy-log\"\n").record_source(&Who::Owner, "w2"), None);
    // with a proxy_log named: an agent on one of his own accounts reads its record, said so, as its requests never
    // went through the gateway; one riding the gateway meanwhile (its account out) reads the log (accounts-lead p2502)
    let mut core = loaded("[project]\nproxy_log = \"/nonexistent/proxy-log\"\n");
    let w2 = core.agents.get_mut("w2").unwrap();
    (w2.native, w2.account) = (true, Some("acct-1".into()));
    w2.gateway = Some(crate::accounts::mover::OnGateway { left: Some("acct-1".into()), since: "2026-11-24T10:00:00Z".into(), back: 0 });
    assert_eq!(core.record_source(&Who::Owner, "w2"), None, "riding the gateway meanwhile: the log, as before");
    core.agents.get_mut("w2").unwrap().gateway = None;
    let r = raw_page(&Shared::new(core), &Who::Owner, "w2".into(), None, None, None).await;
    assert!(r.ok, "{}", r.out);
    let page: proxylog::RawPage = serde_json::from_str(&r.out).unwrap();
    let about = page.about.unwrap_or_default();
    assert!(about.starts_with("This agent runs on your own account, not through the gateway") && about.contains("no HTTP status"), "{about}");
    assert_eq!(page.heads.len(), 1, "its record's one request");
    std::fs::remove_dir_all(&dir).ok();
}

/// The owner's live trial: a Codex agent's and an agy agent's Raw log said "not read" (no gateway log holds their
/// requests). Each now reads its own session file's lines as written, from the end, then only what was added; an agent
/// with no session yet gets an empty page, which the pane says as no turns recorded.
#[tokio::test]
async fn a_codex_or_agy_agent_reads_its_session_files_lines_and_one_with_none_an_empty_page() {
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let dir = std::env::temp_dir().join(format!("coordd-raw-file-agents-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let reg: String = [("AGT-001", "cx", "codex"), ("AGT-002", "ag", "agy"), ("AGT-003", "new", "claude")].iter().map(|(id, n, k)| format!("{}\n", json!({"ev": "add", "id": id, "name": n, "parent": "owner", "kind": k, "model": "m", "at": "x"}))).collect();
    std::fs::write(dir.join("registry.jsonl"), reg).unwrap();
    let mut core = Core::load(&dir.display().to_string(), &base);
    core.load_tickets();
    for (n, file) in [("cx", "rollout-cx.jsonl"), ("ag", "transcript.jsonl")] {
        std::fs::write(dir.join(file), format!("{}\n{}\n", json!({"type": "session_meta", "who": n}), json!({"type": "response_item", "text": "fix mean"}))).unwrap();
        assert!(core.session(&Who::Owner, n, &json!({"session_id": format!("s-{n}"), "socket": dir.join(n).display().to_string(), "transcript_path": dir.join(file).display().to_string()})).ok);
    }
    let shared = Shared::new(core);
    for (n, file) in [("cx", "rollout-cx.jsonl"), ("ag", "transcript.jsonl")] {
        let r = raw_page(&shared, &Who::Owner, n.into(), None, None, None).await;
        assert!(r.ok, "{n}: {}", r.out);
        let page: proxylog::RawPage = serde_json::from_str(&r.out).unwrap();
        assert_eq!(page.lines, vec![json!({"type": "session_meta", "who": n}).to_string(), json!({"type": "response_item", "text": "fix mean"}).to_string()], "{n}");
        assert!(page.about.unwrap_or_default().ends_with(file), "{n}");
        std::fs::OpenOptions::new().append(true).open(dir.join(file)).map(|mut f| std::io::Write::write_all(&mut f, b"{\"type\":\"turn\"}\n")).unwrap().unwrap();
        let r = raw_page(&shared, &Who::Owner, n.into(), None, Some(page.next), None).await;
        let more: proxylog::RawPage = serde_json::from_str(&r.out).unwrap();
        assert_eq!((more.start, more.lines), (page.next, vec!["{\"type\":\"turn\"}".to_string()]), "{n}: only the line added");
    }
    let r = raw_page(&shared, &Who::Owner, "new".into(), None, None, None).await;
    assert!(r.ok, "{}", r.out);
    let page: proxylog::RawPage = serde_json::from_str(&r.out).unwrap();
    assert!(page.lines.is_empty(), "no session yet: no turns, not a refusal");
    let r = raw_page(&shared, &Who::Agent("cx".into()), "ag".into(), None, None, None).await;
    assert!(!r.ok && r.out.contains("only the agents above ag"), "{}", r.out);
    std::fs::remove_dir_all(&dir).ok();
}

/// The owner's note: a fresh agent with no turns, its session file not written yet, has an empty page, not an error;
/// once its first turn is in the file the next ask after that page reads it.
#[tokio::test]
async fn a_fresh_agents_raw_page_is_empty_until_its_first_turn_then_reads_it() {
    let (dir, mut core) = coord("fresh");
    let file = dir.join("s-fresh.jsonl");
    assert!(core.session(&Who::Owner, "w3", &json!({"session_id": "s-fresh", "transcript_path": file.display().to_string()})).ok);
    let shared = Shared::new(core);
    let r = raw_page(&shared, &Who::Owner, "w3".into(), None, None, None).await;
    assert!(r.ok, "{}", r.out);
    let empty: proxylog::RawPage = serde_json::from_str(&r.out).unwrap();
    assert!(empty.lines.is_empty() && empty.next == 0, "{empty:?}");
    std::fs::copy(dir.join("s-w2.jsonl"), &file).unwrap();
    let back = std::time::SystemTime::now() - std::time::Duration::from_secs(10);
    std::fs::File::options().write(true).open(&file).unwrap().set_modified(back).unwrap();
    let r = raw_page(&shared, &Who::Owner, "w3".into(), None, Some(empty.next), None).await;
    let first: proxylog::RawPage = serde_json::from_str(&r.out).unwrap();
    assert_eq!((first.session.clone(), first.start, first.heads.len()), (empty.session, empty.next, 1), "the same session, read on from the empty page: {first:?}");
    assert!(first.lines.join("\n").contains("fix mean"));
    std::fs::remove_dir_all(&dir).ok();
}
