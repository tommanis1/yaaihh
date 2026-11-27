//! New agent through the window (the QA walk's P1 #8, N-1890.p1): the project's default model, Effort and the first
//! message, to the requests its Start sends.

use crate::App;
use crate::home::tests::{made, settle, texts};
use coord_types::Request;
use rune::runtime::Runtime;

/// The requests project 0 sends, written by a child to a file.
struct Asked {
    _child: std::process::Child,
    out: std::path::PathBuf,
}

impl Asked {
    fn on(s: &mut App) -> Asked {
        let out = std::env::temp_dir().join(format!("yaaihh-new-agent-{}.jsonl", std::process::id()));
        let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        Asked { _child: child, out }
    }

    /// The requests sent by now, waiting for the Nth to be written.
    fn sent(&self, n: usize) -> Vec<Request> {
        let read = || std::fs::read_to_string(&self.out).unwrap_or_default().lines().filter_map(|l| serde_json::from_value(serde_json::from_str::<serde_json::Value>(l).ok()?["req"].take()).ok()).collect::<Vec<Request>>();
        for _ in 0..200 {
            if read().len() >= n {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        read()
    }
}

impl Drop for Asked {
    fn drop(&mut self) {
        _ = std::fs::remove_file(&self.out);
    }
}

/// New agent open on project walk, its models read (Haiku first, as coordd lists them), its settings asked.
fn opened() -> (std::sync::MutexGuard<'static, ()>, App) {
    let (held, mut s) = made();
    s.notice = None;
    s.settings.asked = Some("walk".into());
    s.new_agent = crate::projects::NewAgent { open: true, project: 0, models: vec!["claude-haiku-5-5".into(), "claude-opus-5-5".into(), "gpt-6.2-sol".into()], model: Some("claude-haiku-5-5".into()), ..Default::default() };
    (held, s)
}

/// The project's settings read, its Claude default Opus 5.5.
fn defaults_read(s: &mut App) {
    crate::policy::came(s, false, Ok(serde_json::json!({"yaaihh": {"models.claude-gateway": "claude-opus-5-5"}, "project": {}}).to_string()));
}

/// The middle of the element drawn with words W, pressed.
fn press_words(rt: &mut Runtime<App>, w: &str) {
    let r = rt.elements().into_iter().find(|e| !e.exiting && e.text.as_deref() == Some(w)).unwrap_or_else(|| panic!("{w} drawn")).rect;
    let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    rt.pointer_move(x, y);
    rt.frame(0.016);
    rt.pointer_down(x, y);
    rt.frame(0.016);
    rt.pointer_up(x, y);
    settle(rt);
}

/// The project's default model is picked once its settings are read, though its models came first, and not Haiku;
/// his own pick then stays.
#[test]
fn new_agent_starts_on_the_projects_default_model_and_keeps_his_pick() {
    let (_held, mut s) = opened();
    defaults_read(&mut s);
    assert!(crate::projects::new_agent_ended(&mut s));
    assert_eq!(s.new_agent.model.as_deref(), Some("claude-opus-5-5"), "the project's default, read after the list");
    (s.new_agent.model, s.new_agent.chosen) = (Some("claude-haiku-5-5".into()), true);
    crate::projects::new_agent_ended(&mut s);
    assert_eq!(s.new_agent.model.as_deref(), Some("claude-haiku-5-5"), "his pick stays");
}

/// The dialog has Effort, with Claude Code's levels, and a first message box; a Codex model has no Effort. Start sends
/// the first message as its prompt and the effort on its model (`MODEL:EFFORT`, as roster takes it), so its first turn
/// runs at it; nothing more is sent once roster says yes.
#[test]
#[cfg(not(windows))]
fn new_agent_has_effort_and_a_first_message_and_start_sends_both() {
    let (_held, mut s) = opened();
    defaults_read(&mut s);
    crate::projects::new_agent_ended(&mut s);
    let asked = Asked::on(&mut s);
    let mut rt = crate::view_cost::window(s);
    settle(&mut rt);
    let t = texts(&rt);
    for w in ["Effort", "low", "medium", "high", "xhigh", "First message", "Its model's default unless you pick one."] {
        assert!(t.iter().any(|x| x == w), "{w}: {t:?}");
    }
    press_words(&mut rt, "xhigh");
    assert_eq!(rt.state.new_agent.effort.as_deref(), Some("xhigh"));
    rt.update(|s, _| {
        s.new_agent.name = rune::TextEdit::new("try-9");
        s.new_agent.first = rune::TextEdit::new("Read the lexer and say where it loses positions");
    });
    settle(&mut rt);
    press_words(&mut rt, "Start");
    let sent = asked.sent(1);
    assert!(sent.iter().any(|r| matches!(r, Request::AddAgent { name, model, text, .. } if name == "try-9" && model == "claude-opus-5-5:xhigh" && text == "Read the lexer and say where it loses positions")), "{sent:?}");
    // roster's yes
    rt.update(|s, _| {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Ok("try-9: started".into())).unwrap();
        s.new_agent.running = Some(rx);
        crate::projects::new_agent_ended(s);
    });
    let after = asked.sent(sent.len() + 1);
    assert!(!after.iter().any(|r| matches!(r, Request::Effort { .. })), "no effort after the start: {after:?}");
    rt.update(|s, _| {
        s.new_agent.open = true;
        s.new_agent.model = Some("gpt-6.2-sol".into());
    });
    settle(&mut rt);
    assert!(rt.find("new-agent-effort").is_none(), "Codex's levels are its model's: no Effort");
}

/// No first message (the owner's N-1997): Start sends none of ours in its place, so the agent waits at its prompt for
/// his first words, and no turn runs before them.
#[test]
#[cfg(not(windows))]
fn new_agent_with_no_first_message_sends_no_words_of_ours() {
    let (_held, mut s) = opened();
    defaults_read(&mut s);
    crate::projects::new_agent_ended(&mut s);
    let asked = Asked::on(&mut s);
    let mut rt = crate::view_cost::window(s);
    settle(&mut rt);
    rt.update(|s, _| {
        s.new_agent.name = rune::TextEdit::new("try-8");
        s.new_agent.first = rune::TextEdit::new("  ");
    });
    settle(&mut rt);
    press_words(&mut rt, "Start");
    let sent = asked.sent(1);
    assert!(sent.iter().any(|r| matches!(r, Request::AddAgent { name, text, .. } if name == "try-8" && text.is_empty())), "{sent:?}");
}

/// New agent as it opens, the project's default picked, its Effort and first message (new-agent-fields.png).
/// `STILL_OUT=DIR yaaihh-tests new_agent_paths::new_agent_still --ignored --exact`.
#[test]
#[ignore = "a still: STILL_OUT"]
fn new_agent_still() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    fn init() -> App {
        let (_held, mut s) = opened();
        defaults_read(&mut s);
        crate::projects::new_agent_ended(&mut s);
        s.new_agent.name = rune::TextEdit::new("try-9");
        s.new_agent.effort = Some("high".into());
        s.new_agent.first = rune::TextEdit::new("Read the lexer and say where it loses positions.");
        s
    }
    let args = vec!["--snap".to_string(), out.join("new-agent-fields.png").display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
    rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) }).args(args).try_run().unwrap();
}

/// A New agent started with no first message, its page open before he types (consult-lead-c N-1997 p3): its box, and
/// no words of ours in its chat (new-agent-empty-page.png). `STILL_OUT=DIR yaaihh-tests
/// new_agent_paths::new_agent_empty_page_still --ignored --exact`.
#[test]
#[ignore = "a still: STILL_OUT"]
fn new_agent_empty_page_still() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    fn init() -> App {
        let (_held, mut s) = made();
        let mut a = s.projects[0].live.agents[0].clone();
        a.name = "try-8".into();
        s.projects[0].live.agents.push(a);
        s.select_agent("try-8");
        // its session read, and empty: nothing in it before his first message
        s.projects[0].convs.insert("try-8".into(), crate::live::Conv { loaded: true, ..Default::default() });
        s
    }
    let args = vec!["--snap".to_string(), out.join("new-agent-empty-page.png").display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
    rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) }).args(args).try_run().unwrap();
}
