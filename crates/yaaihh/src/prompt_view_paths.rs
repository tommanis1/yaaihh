//! System prompt through the window (prompt_view.rs, N-1864.p1): the text an agent is sent, whole, each section under a
//! quiet marker of where it came from; the standing texts' files asked of coordd once.

use crate::App;
use docket_machine::graph_slice::Row as FolderRow;
use coord_types::{GivenPrompt, Request, StandingPart};
use rune::runtime::Runtime;
use serde_json::json;

const AGENT: &str = "lead";

const IDENTITY: &str = "You are Claude Code, Anthropic's official CLI for Claude.";

const MAIN: &str = "You are an interactive agent that helps users with software engineering tasks. Use the instructions below and the tools available to you to assist the user.\n\n# System\n - All text you output outside of tool use is displayed to the user.\n - Tools are executed in a user-selected permission mode.\n\n# Doing tasks\n - The user will primarily request you to perform software engineering tasks: solving bugs, adding new functionality, refactoring code.\n - Do not add features, refactor code, or make improvements beyond what was asked.\n\n# Environment\n - Primary working directory: /home/t/research\n - Platform: linux";

const GLOBAL: &str = "For every agent of every project of the owner's.\n\nNever make up an estimate of time. When a tool or a job has an expected time that was measured, give that, and say it is the expectation.\n";

const PROJECT: &str = "# Rules for agents working on this project\n\nWhat is written by hand goes in git: code, programs, the policy, tests, and every text meant to last.\n\nA run deletes what it made when it ends.\n";

/// The standing texts as roster joins them (the global one with a line after it).
fn standing() -> String {
    format!("{GLOBAL}\n{PROJECT}")
}

/// Its system prompt as coordd's `Graph::prompt` gives it: started with the standing text; Claude Code's own in two
/// blocks, the identity line and its main text with the standing text appended.
fn prompt() -> Vec<FolderRow> {
    let row = |text: &str, of: &str, rows: Vec<FolderRow>| FolderRow { text: text.into(), of: of.into(), rows, ..Default::default() };
    let started = vec![row("the standing text, appended: carried in 4 requests, 2 turns, 1 step", "text:std", Vec::new())];
    let own = vec![row("block 1: 1 version", "text:c1", vec![row("version 1: carried in 4 requests", "text:c1", Vec::new())]), row("block 2: 1 version", "text:d1", vec![row("version 1: carried in 4 requests", "text:d1", Vec::new())])];
    vec![row("started with: 1 version", "", started), row("Claude Code's own: 2 blocks, 1 version", "", own)]
}

/// Project research at PROTOCOL, lead's System prompt shown, its rows and texts held.
fn shown(protocol: u32) -> App {
    let (_held, mut s) = crate::view_cost::test_app();
    s.notice = None;
    let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
    p.down = None;
    p.protocol = protocol;
    p.live.agents = vec![serde_json::from_value(json!({"id": "AGT-2", "name": AGENT, "parent": "owner", "kind": "claude", "model": "claude-opus-5-5", "state": "idle", "since": "2026-11-26T09:00:00Z", "tools": [], "items": []})).unwrap()];
    s.projects = vec![p];
    s.cur = 0;
    s.pins.list.clear();
    s.sel = crate::Sel::Lens(crate::lens::Lens::of(crate::lens::Filter::SystemPrompt { agent: AGENT.into() }));
    s.folders.requests.insert((0, AGENT.into(), crate::sidebar::PROMPT.into()), (String::new(), Ok(prompt())));
    for (of, text) in [("text:std", standing()), ("text:c1", IDENTITY.to_string()), ("text:d1", format!("{MAIN}\n\n{}", standing()))] {
        s.folders.texts.insert((0, of.into()), Ok(text));
    }
    s
}

/// The standing texts' files, as coordd's `Request::Standing` answers.
fn files() -> String {
    let part = |from: &str, path: &str, text: &str| StandingPart { from: from.into(), path: path.into(), text: text.into() };
    serde_json::to_string(&[part("global", "/home/t/flavor/standing.md", GLOBAL), part("project", "/home/t/flavor/projects/research/standing.md", PROJECT)]).unwrap()
}

fn poll(s: &mut App, _: f64, _: &mut rune::Tx) -> bool {
    crate::prompt_view::poll(s)
}

fn window(s: App) -> Runtime<App> {
    let mut rt = Runtime::new(s, crate::view, (1400.0, 900.0), rune::Env::new(false, 1.0));
    rt.poll = Some(poll);
    frames(&mut rt);
    rt
}

fn frames(rt: &mut Runtime<App>) {
    for _ in 0..4 {
        rt.invalidate();
        rt.frame(0.05);
    }
}

/// The markers drawn, top to bottom.
fn markers(rt: &Runtime<App>) -> Vec<String> {
    let mut m: Vec<(f32, String)> = rt.elements().into_iter().filter(|e| !e.exiting && e.key.as_deref().is_some_and(|k| k.starts_with("prompt-from-"))).filter_map(|e| Some((e.rect[1], e.text?))).collect();
    m.sort_by(|a, b| a.0.total_cmp(&b.0));
    m.into_iter().map(|(_, t)| t).collect()
}

/// The pane asks coordd for the standing texts' files once; until they come the appended text is one "standing text";
/// once they come each file is its own section. The text is drawn whole, Claude Code's own under "harness built-in",
/// with no line about what a system prompt is. Its given file is asked beside them (`Request::Given`). A coordd before
/// `STANDING_PROTOCOL` is asked nothing.
#[test]
#[cfg(not(windows))]
fn the_system_prompt_is_its_text_each_section_marked_by_where_it_came_from() {
    let mut s = shown(coord_types::PROTOCOL);
    let out = std::env::temp_dir().join(format!("yaaihh-standing-{}.jsonl", std::process::id()));
    let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
    s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
    let mut rt = window(s);
    assert_eq!(markers(&rt), ["harness built-in", "standing text"], "the files not said yet");
    rt.state.projects[0].link = None;
    _ = child.wait();
    let sent: Vec<Request> = std::fs::read_to_string(&out).unwrap_or_default().lines().filter_map(|l| serde_json::from_value(serde_json::from_str::<serde_json::Value>(l).ok()?["req"].take()).ok()).collect();
    _ = std::fs::remove_file(&out);
    assert!(matches!(&sent[..], [Request::Given { name }, Request::Standing] if name == AGENT), "asked once: {sent:?}");
    crate::prompt_view::came(&mut rt.state, "research".into(), Ok(files()));
    frames(&mut rt);
    assert_eq!(markers(&rt), ["harness built-in", "global standing.md", "project standing.md"]);
    let texts: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect();
    for want in [IDENTITY, "Never make up an estimate of time. When a tool or a job has an expected time that was measured, give that, and say it is the expectation.", "A run deletes what it made when it ends."] {
        assert!(texts.iter().any(|t| t.contains(want)), "{want}: {texts:?}");
    }
    assert!(!texts.iter().any(|t| t.starts_with("A system prompt is what") || t.starts_with("It runs with") || t == "Your words"), "{texts:?}");
    let older = window(shown(coord_types::STANDING_PROTOCOL - 1));
    assert!(older.state.standing.of("research").is_empty() && markers(&older).last().is_some_and(|m| m == "standing text"), "an older coordd: one standing text");
}

/// lead before its first request on a coordd before `GIVEN_PROTOCOL`: the standing texts whole under their files'
/// markers, from coordd's Standing; one muted line where the harness's own goes.
#[test]
#[cfg(not(windows))]
fn a_fresh_agents_system_prompt_is_what_it_starts_with_marked() {
    let rt = window(fresh());
    assert_eq!(markers(&rt), ["global standing.md", "project standing.md"]);
    let texts: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect();
    assert!(texts.iter().any(|t| t == "Harness built-in prompt captured after first turn"), "{texts:?}");
    assert!(texts.iter().any(|t| t.contains("A run deletes what it made when it ends.")) && !texts.iter().any(|t| t.starts_with("No request yet")), "{texts:?}");
}

/// pa-temp on live v0.4 (the owner, N-1927.p1): 0 requests, the record's one start naming a text no request carried
/// (its text not read), Claude Code's own with 0 blocks; its `system-prompt.md` on disk, given by coordd. The view is
/// that file's text, not the standing files alone, cut under its files' markers and his, Edit beside the head opening
/// its box in the pane on that text, one muted line for the harness's own; none
/// of the record's words. Before the record's rows come at all, the file shows the same.
#[test]
#[cfg(not(windows))]
fn pa_temp_with_no_request_shows_its_system_prompt_file() {
    let rt = window(pa_temp(true));
    let texts: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect();
    let whole = texts.join("\n");
    for want in ["Never make up an estimate of time.", "A run deletes what it made when it ends.", "And pa-temp's own last line.", "Harness built-in prompt captured after first turn"] {
        assert!(whole.contains(want), "{want}: {texts:?}");
    }
    for gone in ["before anything else", "its text not read", "0 blocks", "0 version", "started with", "on the Map", "It runs with", "Your words"] {
        assert!(!whole.contains(gone), "{gone}: {texts:?}");
    }
    assert_eq!(markers(&rt), ["global standing.md", "project standing.md", "agent override"]);
    // Edit right there: its box opens in the pane holding the file it was given
    let mut rt = rt;
    let r = rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some("prompt-edit-pa-temp")).expect("Edit right there").rect;
    let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    rt.pointer_move(x, y);
    rt.frame(0.016);
    rt.pointer_down(x, y);
    rt.frame(0.016);
    rt.pointer_up(x, y);
    frames(&mut rt);
    let pe = &rt.state.prompt_edit;
    assert_eq!(pe.of, Some((0, "pa-temp".to_string())), "opened in the pane");
    assert!(pe.text.text.contains("And pa-temp's own last line.") && pe.text.text.contains("Never make up an estimate of time."), "{}", pe.text.text);
    let early = window(pa_temp(false));
    let whole: String = early.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect::<Vec<_>>().join("\n");
    assert!(whole.contains("And pa-temp's own last line.") && !whole.contains("Asking coordd"), "{whole}");
}

/// pa-temp after its first turn: Claude Code's own captured, the standing text appended to it, as its start says.
fn pa_temp_turned() -> App {
    let mut s = pa_temp(true);
    s.folders.requests.insert((0, "pa-temp".into(), crate::sidebar::PROMPT.into()), (String::new(), Ok(prompt())));
    for (of, text) in [("text:std", standing()), ("text:c1", IDENTITY.to_string()), ("text:d1", format!("{MAIN}\n\n{}", standing()))] {
        s.folders.texts.insert((0, of.into()), Ok(text));
    }
    s
}

/// pa-temp's System prompt: its file given by coordd, the standing text and a line of his after it; ROWS, the record's
/// rows as live v0.4 gave them, else none yet.
fn pa_temp(rows: bool) -> App {
    let mut s = shown(coord_types::PROTOCOL);
    s.projects[0].live.agents[0].name = "pa-temp".into();
    s.sel = crate::Sel::Lens(crate::lens::Lens::of(crate::lens::Filter::SystemPrompt { agent: "pa-temp".into() }));
    s.folders.requests.clear();
    s.folders.texts.clear();
    if rows {
        let row = |text: &str, of: &str, rows: Vec<FolderRow>| FolderRow { text: text.into(), of: of.into(), rows, ..Default::default() };
        let started = vec![row("the standing text, appended", "text:7244a597494b", Vec::new())];
        let list = vec![row("started with: 1 version", "", started), row("Claude Code's own: 0 blocks, 0 versions", "", Vec::new())];
        s.folders.requests.insert((0, "pa-temp".into(), crate::sidebar::PROMPT.into()), (String::new(), Ok(list)));
        s.folders.texts.insert((0, "text:7244a597494b".into()), Err("coordd: no request sent under a start that gave text:7244a597494b is in the record's graph yet".into()));
    }
    crate::prompt_view::came(&mut s, "research".into(), Ok(files()));
    let file = GivenPrompt { path: "/home/t/agent-coord/yaaihh/agents/pa-temp/system-prompt.md".into(), text: format!("{}\nAnd pa-temp's own last line.", standing()), replace: false, his: true, diff: None };
    crate::prompt_view::came_given(&mut s, "research".into(), "pa-temp".into(), Ok(serde_json::to_string(&file).unwrap()));
    s
}

/// lead before its first request on a coordd at PROTOCOL, the standing texts' files held from `STANDING_PROTOCOL`.
fn fresh() -> App {
    fresh_at(coord_types::STANDING_PROTOCOL)
}

fn fresh_at(protocol: u32) -> App {
    let mut s = shown(protocol);
    let not_said = FolderRow { text: "the standing text, appended; no start line says which version".into(), of: docket_machine::graph_slice::NOT_SAID.into(), ..Default::default() };
    let fresh = vec![FolderRow { text: "started with: 1 version".into(), rows: vec![not_said], ..Default::default() }, FolderRow { text: "Claude Code's own: nothing sent yet".into(), ..Default::default() }];
    s.folders.requests.insert((0, AGENT.into(), crate::sidebar::PROMPT.into()), (String::new(), Ok(fresh)));
    s.folders.texts.clear();
    if protocol >= coord_types::STANDING_PROTOCOL {
        crate::prompt_view::came(&mut s, "research".into(), Ok(files()));
    }
    s
}

/// The texts drawn, joined.
fn drawn(rt: &Runtime<App>) -> String {
    rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect::<Vec<_>>().join("\n")
}

/// From a coordd that gives the file, an agent with none says so in one line, and no standing text stands in for it;
/// any other refusal, or an answer that does not read, is "Not read: why", never that;
/// while the answer is on its way, "Loading…". A coordd below `STANDING_PROTOCOL`, asked nothing, draws no waiting line.
#[test]
#[cfg(not(windows))]
fn no_file_is_said_and_nothing_stands_in_for_it() {
    let mut s = fresh_at(coord_types::GIVEN_PROTOCOL);
    crate::prompt_view::came_given(&mut s, "research".into(), AGENT.into(), Err(format!("{AGENT} {}: roster writes it at its start", coord_types::NO_GIVEN_FILE)));
    let t = drawn(&window(s));
    assert!(t.contains(crate::sidebar::NO_FILE), "{t}");
    for gone in ["Never make up an estimate of time.", "A run deletes what it made when it ends.", "Loading", "Harness built-in"] {
        assert!(!t.contains(gone), "{gone}: {t}");
    }
    // any other refusal, or an answer that does not read, is said as it is: never "no file yet"
    for (r, said) in [(Err("coordd: /c/agents/lead/system-prompt.md: Permission denied (os error 13)".to_string()), "Not read: /c/agents/lead/system-prompt.md: Permission denied"), (Ok("not json".to_string()), "Not read: expected ident")] {
        let mut s = fresh_at(coord_types::GIVEN_PROTOCOL);
        crate::prompt_view::came_given(&mut s, "research".into(), AGENT.into(), r);
        let t = drawn(&window(s));
        assert!(t.contains(said) && !t.contains(crate::sidebar::NO_FILE) && !t.contains("coordd:"), "{said}: {t}");
    }
    let mut s = fresh_at(coord_types::GIVEN_PROTOCOL);
    let out = std::env::temp_dir().join(format!("yaaihh-given-{}.jsonl", std::process::id()));
    let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
    s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
    let mut rt = window(s);
    assert!(drawn(&rt).contains("Loading…") && !drawn(&rt).contains(crate::sidebar::NO_FILE), "asked, not answered: {}", drawn(&rt));
    rt.state.projects[0].link = None;
    _ = child.wait();
    _ = std::fs::remove_file(&out);
    let t = drawn(&window(fresh_at(coord_types::STANDING_PROTOCOL - 1)));
    assert!(!t.contains("Loading") && t.contains("Harness built-in prompt captured after first turn"), "{t}");
}

/// lead's System prompt with the standing texts' files held.
fn marked() -> App {
    let mut s = shown(coord_types::PROTOCOL);
    crate::prompt_view::came(&mut s, "research".into(), Ok(files()));
    s
}

/// The stills, 1400 by 900: lead's System prompt, its text whole under quiet markers (system-prompt-view.png); before
/// its first request (system-prompt-fresh.png); pa-temp at 0 requests, its file given (system-prompt-pa-temp.png), and
/// after its first turn, Claude Code's own captured (system-prompt-pa-temp-turned.png).
/// `STILL_OUT=DIR yaaihh-tests prompt_view_paths::system_prompt_still --ignored --exact`.
#[test]
#[ignore]
fn system_prompt_still() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let shots: [(&str, fn() -> App); 4] = [("system-prompt-view.png", marked), ("system-prompt-fresh.png", fresh), ("system-prompt-pa-temp.png", || pa_temp(true)), ("system-prompt-pa-temp-turned.png", pa_temp_turned)];
    for (name, start) in shots {
        let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), start, crate::view) }).args(args).try_run().unwrap();
    }
}

/// A link to a child that writes what it is sent to a file of its own, LABEL its name.
fn linked(s: &mut App, label: &str) -> (std::process::Child, std::path::PathBuf) {
    let out = std::env::temp_dir().join(format!("yaaihh-prompt-{label}-{}.jsonl", std::process::id()));
    let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
    s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
    (child, out)
}

/// What the child of [`linked`] was sent, once the link is dropped.
fn sent(mut child: std::process::Child, out: std::path::PathBuf) -> Vec<Request> {
    _ = child.wait();
    let got = std::fs::read_to_string(&out).unwrap_or_default().lines().filter_map(|l| serde_json::from_value(serde_json::from_str::<serde_json::Value>(l).ok()?["req"].take()).ok()).collect();
    _ = std::fs::remove_file(&out);
    got
}

/// The pipe ends while the file is asked: the ask is let go, the pane not left on "Loading…" for good; once the pipe is
/// back it is asked again, and its answer draws the file. Before any ask, an agent of the project's is "Loading…",
/// never "no file"; one that is not says so.
#[test]
#[cfg(not(windows))]
fn a_lost_ask_is_asked_again_and_not_asked_is_no_no_file() {
    use crate::prompt_view::Given;
    let t = drawn(&window(fresh_at(coord_types::GIVEN_PROTOCOL)));
    assert!(t.contains("Loading…") && !t.contains(crate::sidebar::NO_FILE), "not asked yet: {t}");
    let mut s = fresh_at(coord_types::GIVEN_PROTOCOL);
    let (child, out) = linked(&mut s, "lost");
    let mut rt = window(s);
    assert_eq!(rt.state.standing.given("research", AGENT), Given::Waiting);
    rt.update(|s, _| _ = s.take(0, crate::link::Incoming::Down("the pipe ended".into())));
    assert_eq!(rt.state.standing.given("research", AGENT), Given::NotAsked, "let go with the pipe");
    rt.state.projects[0].link = None;
    assert_eq!(sent(child, out).iter().filter(|r| matches!(r, Request::Given { .. })).count(), 1);
    back(&mut rt, "back");
}

/// The pipe comes back to RT: the file asked again, and its answer, coordd's, drawn.
fn back(rt: &mut Runtime<App>, label: &str) {
    use crate::prompt_view::Given;
    let (child, out) = linked(&mut rt.state, label);
    rt.update(|s, _| (s.projects[0].down, s.projects[0].protocol) = (None, coord_types::GIVEN_PROTOCOL));
    frames(rt);
    assert_eq!(rt.state.standing.given("research", AGENT), Given::Waiting, "asked again");
    rt.state.projects[0].link = None;
    assert!(sent(child, out).iter().any(|r| matches!(r, Request::Given { name } if name == AGENT)));
    let file = GivenPrompt { path: "/c/agents/lead/system-prompt.md".into(), text: format!("{}\nAnd lead's own last line.", standing()), replace: false, his: true, diff: None };
    rt.update(|s, _| crate::prompt_view::came_given(s, "research".into(), AGENT.into(), Ok(serde_json::to_string(&file).unwrap())));
    frames(rt);
    let t = drawn(rt);
    assert!(t.contains("And lead's own last line.") && !t.contains("Loading"), "the file: {t}");
}

/// A send that fails (the pipe's other end gone) marks nothing asked: no "Loading…" for good; the next working pipe asks
/// it, and the file comes. An agent not of the project's: neither "Loading…" nor "no file".
#[test]
#[cfg(not(windows))]
fn a_failed_send_is_not_asked_and_is_asked_again() {
    use crate::prompt_view::Given;
    let mut s = fresh_at(coord_types::GIVEN_PROTOCOL);
    let mut dead = std::process::Command::new("true").stdin(std::process::Stdio::piped()).spawn().unwrap();
    let stdin = dead.stdin.take().unwrap();
    _ = dead.wait();
    s.projects[0].link = Some(crate::link::Link::to_child(stdin));
    let mut rt = window(s);
    assert_eq!(rt.state.standing.given("research", AGENT), Given::NotAsked, "not sent, not asked");
    back(&mut rt, "after-dead");
    let mut s = fresh_at(coord_types::GIVEN_PROTOCOL);
    s.sel = crate::Sel::Lens(crate::lens::Lens::of(crate::lens::Filter::SystemPrompt { agent: "gone".into() }));
    let t = drawn(&window(s));
    assert!(t.contains("gone is not an agent of this box") && !t.contains("Loading") && !t.contains(crate::sidebar::NO_FILE), "{t}");
}

/// pa-temp started with his version in place of the harness's own, before its first request: Edit opens on it in place,
/// so Go on keeps it so.
#[test]
#[cfg(not(windows))]
fn edit_before_the_first_request_keeps_a_version_in_place() {
    let mut s = pa_temp(false);
    let file = GivenPrompt { path: "/c/agents/pa-temp/system-prompt.md".into(), text: "His whole prompt.".into(), replace: true, his: true, diff: None };
    crate::prompt_view::came_given(&mut s, "research".into(), "pa-temp".into(), Ok(serde_json::to_string(&file).unwrap()));
    let mut rt = window(s);
    let r = rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some("prompt-edit-pa-temp")).expect("Edit").rect;
    let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    rt.pointer_move(x, y);
    rt.frame(0.016);
    rt.pointer_down(x, y);
    rt.frame(0.016);
    rt.pointer_up(x, y);
    frames(&mut rt);
    let pe = &rt.state.prompt_edit;
    assert!(pe.of.is_some() && pe.replace && pe.text.text == "His whole prompt.", "in place: {} {}", pe.replace, pe.text.text);
}

/// A text of the harness's own coordd could not read says so, without the engine's name, not "Loading…" for good; and
/// for an agent with no request yet no text of the record is asked, which coordd could only refuse.
#[test]
#[cfg(not(windows))]
fn an_unread_text_is_said_and_no_doomed_text_is_asked() {
    let mut s = shown(coord_types::GIVEN_PROTOCOL);
    s.folders.texts.insert((0, "text:d1".into()), Err("coordd: the session's file is gone".into()));
    let t = drawn(&window(s));
    assert!(t.contains("Not read: the session's file is gone") && !t.contains("coordd:"), "{t}");
    let mut s = pa_temp(true);
    let (child, out) = linked(&mut s, "doomed");
    crate::sidebar::apart_poll(&mut s);
    s.projects[0].link = None;
    let asked = sent(child, out);
    assert!(!asked.iter().any(|r| serde_json::to_string(r).unwrap().contains("7244a597494b")), "{asked:?}");
}

/// Claude Code's own main text in [`replaced`], and his version in place of it: the guidance and Doing tasks left out,
/// Environment in his words, a line of his own.
const GUIDANCE: &str = "# Session-specific guidance\n - If you need the user to run a shell command themselves, suggest they type `! <command>` in the prompt.";
const HIS: &str = "You are an interactive agent that helps users with software engineering tasks. Use the instructions below and the tools available to you to assist the user.\n\n# System\n - All text you output outside of tool use is displayed to the user.\n - Tools are executed in a user-selected permission mode.\n\n# Environment\n - Primary working directory: /home/t/research\n - Platform: linux, on the tower\n\nAnswer in short lines, results first.";

/// lead running his version in place of Claude Code's own (started with it, `IN_PLACE`): Claude Code's main text as
/// it first sent it, then his, the standing texts after each; their files held.
fn replaced() -> App {
    let mut s = shown(coord_types::PROTOCOL);
    let row = |text: &str, of: &str, rows: Vec<FolderRow>| FolderRow { text: text.into(), of: of.into(), rows, ..Default::default() };
    let started = vec![row("the standing text, appended: carried in 4 requests", "text:std", Vec::new()), row(&format!("version 2, {}: carried in 2 requests", docket_machine::graph::IN_PLACE), "text:his", Vec::new())];
    let own = vec![
        row("block 1: 1 version", "text:c1", vec![row("version 1: carried in 6 requests", "text:c1", Vec::new())]),
        row("block 2: 2 versions", "text:d2", vec![row("version 1: carried in 4 requests", "text:d1", Vec::new()), row("version 2: carried in 2 requests", "text:d2", Vec::new())]),
    ];
    let list = vec![row("started with: 2 versions", "", started), row("Claude Code's own: 2 blocks, 2 versions", "", own)];
    s.folders.requests.insert((0, AGENT.into(), crate::sidebar::PROMPT.into()), (String::new(), Ok(list)));
    s.folders.texts.clear();
    let his = format!("{HIS}\n\n{}", standing());
    for (of, text) in [("text:std", standing()), ("text:his", his.clone()), ("text:c1", IDENTITY.to_string()), ("text:d1", format!("{MAIN}\n\n{GUIDANCE}\n\n{}", standing())), ("text:d2", his)] {
        s.folders.texts.insert((0, of.into()), Ok(text));
    }
    crate::prompt_view::came(&mut s, "research".into(), Ok(files()));
    s
}

/// His version in place of Claude Code's own (N-1980): the prompt sent is one document read against Claude Code's own:
/// the parts left out pills of what each saves, his changed and his own parts and the standing texts marked added, the
/// sum over them; a pill opens to its text.
#[test]
#[cfg(not(windows))]
fn his_version_reads_against_claude_codes_own_parts_left_out_as_pills() {
    let mut rt = window(replaced());
    let pills: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.key).filter(|k| k.starts_with("prompt-pill-")).collect();
    assert_eq!(pills, ["prompt-pill-Doing tasks", "prompt-pill-Session-specific guidance"]);
    let texts: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect();
    let t = crate::prompt_diff::tokens;
    let sum = texts.iter().find(|x| x.starts_with("Base ")).expect("the sum");
    assert!(sum.ends_with(" tok") && sum.contains(&format!("− {}", t(GUIDANCE) + t(MAIN.split("\n\n").nth(2).unwrap()) + t(MAIN.split("\n\n").nth(3).unwrap()))), "{sum}");
    assert!(texts.iter().any(|x| x.starts_with("Environment · changed")) && texts.iter().filter(|x| *x == "+").count() >= 3, "{texts:?}");
    assert_eq!(markers(&rt), ["harness built-in", "agent override", "global standing.md", "project standing.md"]);
    assert!(!texts.iter().any(|x| x.contains("suggest they type")), "folded");
    let r = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("prompt-pill-Session-specific guidance")).unwrap().rect;
    rt.pointer_move(r[0] + 8.0, r[1] + 10.0);
    rt.frame(0.016);
    rt.pointer_down(r[0] + 8.0, r[1] + 10.0);
    rt.frame(0.016);
    rt.pointer_up(r[0] + 8.0, r[1] + 10.0);
    frames(&mut rt);
    assert!(rt.elements().into_iter().filter_map(|e| e.text).any(|x| x.contains("suggest they type")), "opened");
}

/// The still of [`replaced`], the guidance's pill open (N-1980). `STILL_OUT=DIR … prompt_diff_still --ignored --exact`.
#[test]
#[ignore]
fn prompt_diff_still() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    std::fs::create_dir_all(&out).unwrap();
    fn init() -> App {
        let mut s = replaced();
        s.prompt_edit.pills.insert((0, AGENT.into(), "Session-specific guidance".into()));
        s
    }
    let args = vec!["--snap".to_string(), std::path::Path::new(&out).join("prompt-diff.png").display().to_string(), "--size".into(), "1400x1000".into(), "--at".into(), "1.0".into()];
    rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 1000.0), init, crate::view) }).args(args).try_run().unwrap();
}

/// A prompt recipe's account beside the file (roster's diff file, gap-homes N-1981 p2): drawn as it says, its cut a pill
/// that opens to its text with no Restore (the recipe's to undo), a pack's addition marked, a cut that matched nothing
/// said under the sum.
#[test]
#[cfg(not(windows))]
fn a_recipes_account_is_drawn_as_it_says() {
    let mut s = shown(coord_types::PROTOCOL);
    s.folders.requests.clear();
    let seg = |kind: &str, label: &str, text: &str, tokens: usize| coord_types::PromptSegment { kind: kind.into(), label: label.into(), text: text.into(), tokens, source: String::new() };
    let diff = coord_types::PromptDiff {
        harness: "claude-code".into(),
        how: "replace".into(),
        notes: Vec::new(),
        segments: vec![seg("kept", "", "You are an agent.", 5), seg("stripped", "#4 Pronouns", "When you use a pronoun for someone, use they/them.", 91), seg("missing", "#11 Games", "", 0), seg("added", "pack fable", "Answer in short lines.", 6)],
    };
    let file = GivenPrompt { path: "/c/agents/lead/system-prompt.md".into(), text: "You are an agent.\n\nAnswer in short lines.".into(), replace: true, his: true, diff: Some(diff) };
    crate::prompt_view::came_given(&mut s, "research".into(), AGENT.into(), Ok(serde_json::to_string(&file).unwrap()));
    let mut rt = window(s);
    let texts = |rt: &Runtime<App>| rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect::<Vec<_>>();
    let t = texts(&rt);
    for want in ["Base 96 − 91 + 6 = 11 tok", "−91 tok · #4 Pronouns", "#11 Games not found in the harness's text"] {
        assert!(t.iter().any(|x| x == want), "{want}: {t:?}");
    }
    assert_eq!(markers(&rt), ["harness built-in", "prompt pack"]);
    let r = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("prompt-pill-#4 Pronouns")).unwrap().rect;
    rt.pointer_move(r[0] + 8.0, r[1] + 10.0);
    rt.frame(0.016);
    rt.pointer_down(r[0] + 8.0, r[1] + 10.0);
    rt.frame(0.016);
    rt.pointer_up(r[0] + 8.0, r[1] + 10.0);
    frames(&mut rt);
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "Cut by the agent's prompt recipe") && !t.iter().any(|x| x == "Restore"), "{t:?}");
}
