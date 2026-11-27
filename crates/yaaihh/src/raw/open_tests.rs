//! The owner's live trial: Raw log opened and said "not read". Opening it now asks coordd at once, says Loading… while
//! the ask is out, then shows the lines; an agent with no turns says so.

use super::*;
use crate::home::tests::{made, settle, texts};

/// The raw pane's ask now out for agent NAME of the project, if any.
fn asked(s: &App, name: &str) -> bool {
    s.projects[0].pending.values().any(|p| matches!(p, Pending::Raw { agent, older: false, page: true } if agent == name))
}

#[test]
fn opening_raw_log_asks_at_once_says_loading_then_shows_its_lines() {
    let (_held, mut s) = made();
    let out = std::env::temp_dir().join(format!("yaaihh-raw-open-{}.jsonl", std::process::id()));
    let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
    s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
    // what an earlier open of the pane kept: a refusal, asked just now
    s.raw.logs.insert((0, "try-1".into()), Log { refused: Some("try-1 has no session yet".into()), asked: Some(Instant::now()), ..Default::default() });
    s.open_raw(0, "try-1");
    let mut rt = crate::view_cost::window(s);
    rt.update(|s, tx| {
        poll(s, tx);
    });
    settle(&mut rt);
    let t = texts(&rt);
    rt.update(|s, _| assert!(asked(s, "try-1"), "asked on opening, not a second later"));
    assert!(t.iter().any(|x| x == LOADING) && !t.iter().any(|x| x.contains("cannot be shown") || x.contains("not read")), "{t:?}");
    let page = RawPage { lines: vec![r#"{"type":"session_meta"}"#.into(), "fix mean".into()], start: 0, next: 40, session: "session s-1".into(), heads: vec![0, 24], about: Some("its own session file".into()) };
    rt.update(move |s, _| replied(s, 0, "try-1".into(), false, true, true, serde_json::to_string(&page).unwrap()));
    settle(&mut rt);
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "fix mean") && t.iter().any(|x| x == "2 lines held · session s-1"), "{t:?}");
    assert!(!t.iter().any(|x| x == LOADING), "{t:?}");
    // the next ask reads on from the last line held, so nothing comes twice
    rt.update(|s, _| assert!(matches!(ask_for(coord_types::PROTOCOL, "try-1", &s.raw.logs[&(0, "try-1".to_string())], false).0, Request::RawPage { after: Some(40), .. })));
    // an agent with no session file yet: no turns, not a refusal
    rt.update(|s, tx| {
        s.open_raw(0, "try-2");
        poll(s, tx);
        assert!(asked(s, "try-2"));
        replied(s, 0, "try-2".into(), false, true, true, serde_json::to_string(&RawPage::default()).unwrap());
    });
    settle(&mut rt);
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == NO_TURNS) && !t.iter().any(|x| x.contains("not read") || x.contains("cannot be shown") || x.starts_with("0 lines")), "{t:?}");
    // its first turn: the pane's next ask, a second on, brings it in by itself
    rt.update(|s, tx| {
        s.raw.logs.get_mut(&(0, "try-2".to_string())).unwrap().asked = Some(Instant::now() - EVERY);
        poll(s, tx);
        assert!(asked(s, "try-2"), "asked again while it shows no turns");
        let page = RawPage { lines: vec!["first turn".into()], start: 0, next: 11, session: "session s-2".into(), heads: vec![0], about: Some("its own session file".into()) };
        replied(s, 0, "try-2".into(), false, true, true, serde_json::to_string(&page).unwrap());
    });
    settle(&mut rt);
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "first turn") && !t.iter().any(|x| x == NO_TURNS), "{t:?}");
    rt.update(|s, _| s.projects[0].link = None);
    drop(child.stdin.take());
    let _ = child.wait();
    let sent: Vec<serde_json::Value> = std::fs::read_to_string(&out).unwrap().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    let _ = std::fs::remove_file(&out);
    let asks: Vec<&serde_json::Value> = sent.iter().filter_map(|v| v.get("req")).filter(|r| r["op"] == "rawpage").collect();
    assert_eq!(asks.iter().map(|r| r["name"].as_str().unwrap_or_default()).collect::<Vec<_>>(), vec!["try-1", "try-2", "try-2"], "{sent:?}");
}

/// A session file's lines carry no request heads; past KEEP the oldest go, and Earlier reads back from the first kept.
#[test]
fn a_session_files_lines_are_cut_from_the_front() {
    let mut log = Log::default();
    let n = KEEP + 10;
    let page = RawPage { lines: (0..n).map(|i| format!("{{\"n\":{i}}}")).collect(), start: 0, next: n as u64 * 10, session: "session f".into(), heads: (0..n as u64).map(|i| i * 10).collect(), about: None };
    merge(&mut log, page, false);
    assert_eq!((log.lines.len(), log.start, log.lines[0].as_str()), (KEEP, 100, "{\"n\":10}"));
}
