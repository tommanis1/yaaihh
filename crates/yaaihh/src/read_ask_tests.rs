use super::*;
use crate::home::tests::{keyed, made, settle, texts};

fn ask(id: u64, path: &str, folder: Option<&str>) -> ReadAsk {
    ReadAsk { id, path: path.into(), folder: folder.map(str::to_string), at: format!("2026-11-27T10:0{id}:00Z") }
}

#[test]
fn a_home_is_said_as_a_tilde() {
    assert_eq!(short("/home/t/.gemini/antigravity-cli"), "~/.gemini/antigravity-cli");
    assert_eq!(short("/root/notes/a.md"), "~/notes/a.md");
    assert_eq!(short("/etc/hosts"), "/etc/hosts");
}

/// The owner's case on ai-detection: clerk-paper-setup reads past notes under ~/.gemini; one card in its chat, one
/// line under Waits on you, and his press is the call's answer, sent to its coordd.
#[test]
fn a_read_waiting_on_him_is_one_card_in_the_agents_chat_and_one_line_under_waits_on_you() {
    let (_held, mut s) = made();
    s.projects[0].live.agents[0].name = "clerk-paper-setup".into();
    let (paper, folder) = ("/home/t/.gemini/antigravity-cli/paper.md", "/home/t/.gemini/antigravity-cli");
    s.projects[0].live.agents[0].read_asks = vec![ask(1, paper, Some(folder))];
    let sent = std::env::temp_dir().join(format!("read-ask-sent-{}", std::process::id()));
    let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > {}", sent.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
    s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
    let mut rt = crate::view_cost::window(s);
    rt.update(|s, _| s.sel = crate::Sel::Tickets);
    settle(&mut rt);
    let want = "clerk-paper-setup wants read-only access to ~/.gemini/antigravity-cli";
    let t = texts(&rt);
    assert_eq!(t.iter().filter(|x| x.as_str() == want).count(), 1, "one line under Waits on you: {t:?}");
    assert!(t.iter().any(|x| x == "Waits on you · 1"), "{t:?}");
    assert!(!t.iter().any(|x| x.contains("Q-") || x.contains("block_ask") || x.contains("NoWorker")), "{t:?}");
    keyed(&mut rt, "need-read-1");
    rt.update(|s, _| assert_eq!(s.sel, crate::Sel::Agent("clerk-paper-setup".into()), "its chat"));
    let t = texts(&rt);
    assert_eq!(t.iter().filter(|x| x.as_str() == want).count(), 1, "one card in its chat, and no line beside it: {t:?}");
    for b in ["Allow read-only for this folder", "Allow once", "Deny"] {
        assert!(t.iter().any(|x| x == b), "{b}: {t:?}");
    }
    keyed(&mut rt, "read-folder-1");
    rt.update(|s, _| s.projects[0].link = None);
    let _ = child.wait();
    let line = std::fs::read_to_string(&sent).unwrap_or_default();
    assert!(line.contains(r#""op":"read_answer""#) && line.contains(r#""name":"clerk-paper-setup""#) && line.contains(r#""id":1"#) && line.contains(r#""answer":"folder""#), "{line}");
    assert!(!line.contains(".gemini"), "no path goes from the window: {line}");
    let _ = std::fs::remove_file(&sent);
}

#[test]
fn a_read_whose_folder_is_too_wide_offers_once_and_deny_alone() {
    let (_held, mut s) = made();
    s.projects[0].live.agents[0].read_asks = vec![ask(2, "/etc/hosts", None)];
    let mut rt = crate::view_cost::window(s);
    rt.update(|s, _| s.select_agent("try-1"));
    settle(&mut rt);
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "try-1 wants read-only access to /etc/hosts") && t.iter().any(|x| x == "Allow once") && !t.iter().any(|x| x == "Allow read-only for this folder"), "{t:?}");
    rt.update(|s, _| assert!(crate::waits::agent_waits(&s.projects[0], "try-1") && crate::waits::count(&s.projects[0]) == 1));
}

/// The owner's case drawn: on Threads (STILL_OUT/read-ask-threads.png) and in the agent's chat (read-ask-chat.png).
#[test]
#[ignore]
fn still_read_ask() {
    fn base() -> App {
        let (_held, mut s) = made();
        let a = &mut s.projects[0].live.agents[0];
        a.name = "clerk-paper-setup".into();
        a.read_asks = vec![ask(1, "/home/t/.gemini/antigravity-cli/paper-notes.md", Some("/home/t/.gemini/antigravity-cli"))];
        s
    }
    fn threads() -> App {
        let mut s = base();
        s.sel = crate::Sel::Tickets;
        s
    }
    fn chat() -> App {
        let mut s = base();
        s.select_agent("clerk-paper-setup");
        s
    }
    let Ok(dir) = std::env::var("STILL_OUT") else { return };
    for (name, init) in [("read-ask-threads.png", threads as fn() -> App), ("read-ask-chat.png", chat)] {
        let path = format!("{dir}/{name}");
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) }).args(["--snap", path.as_str(), "--size", "1400x900", "--at", "1.0"].map(String::from)).try_run().unwrap();
    }
}
