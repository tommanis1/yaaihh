use crate::App;
use crate::home::tests::{keyed, made, texts};
use rune::runtime::Runtime;

/// try-1 as coordd's event SEQ about it gives it: a dialog standing on its terminal (TITLE), or none.
fn told(s: &mut App, seq: u64, title: Option<&str>) {
    let mut a = s.projects[0].live.agents.iter().find(|a| a.name == "try-1").unwrap().clone();
    a.dialog = title.map(str::to_string);
    s.take(0, crate::link::Incoming::Event(coord_types::Event { seq, body: coord_types::Body::Agent(a) }));
}

fn settle(rt: &mut Runtime<App>) {
    rt.invalidate();
    for _ in 0..3 {
        rt.frame(0.016);
    }
}

/// N-1863 p3: an agent stuck on a terminal prompt is never drawn as one that runs normally: the sidebar's project and
/// Agents rows and its tile say so, and its page leads with the prompt and a press that shows the terminal.
#[test]
fn a_dialog_on_an_agents_terminal_is_said_in_the_sidebar_on_its_tile_and_on_its_page() {
    let (_held, mut s) = made();
    let title = "Adding a new directory: /srv/other (not a folder of the project: Claude Code waits for an answer in its terminal)";
    told(&mut s, 90, Some(title));
    assert_eq!(s.projects[0].live.agents.iter().find(|a| a.name == "try-1").unwrap().dialog.as_deref(), Some(title), "the event's dialog is the agent's");
    s.sel = crate::Sel::Agents;
    let mut rt = crate::view_cost::window(s);
    settle(&mut rt);
    let t = texts(&rt);
    assert!(t.iter().any(|x| x.starts_with("Waiting on terminal prompt · ") && x.ends_with("running")), "the project's row: {t:?}");
    assert!(t.iter().any(|x| x == "Waiting on terminal prompt"), "the Agents row and the tile: {t:?}");
    assert!(rt.find("agent-waiting-try-1").is_some(), "its tile");
    rt.state.select_agent("try-1");
    settle(&mut rt);
    assert!(rt.find("agent-waiting-card-try-1").is_some(), "its page leads with the prompt");
    assert!(texts(&rt).iter().any(|x| x == title), "the prompt's title on its page");
    assert!(!rt.state.term.pages.contains(&(0, "try-1".to_string())));
    keyed(&mut rt, "agent-waiting-open-try-1");
    assert!(rt.state.term.pages.contains(&(0, "try-1".to_string())), "the press shows its terminal");

    told(&mut rt.state, 91, None);
    settle(&mut rt);
    assert_eq!(rt.state.projects[0].live.agents.iter().find(|a| a.name == "try-1").unwrap().dialog, None, "the event's");
    assert!(rt.find("agent-waiting-card-try-1").is_none(), "answered: off its page");
    let t = texts(&rt);
    assert!(!t.iter().any(|x| x.contains("Waiting on terminal prompt")), "answered: gone everywhere: {t:?}");
}

#[test]
fn the_sidebar_line_says_the_prompt_before_anything_else() {
    let (_held, mut s) = made();
    let p = &mut s.projects[0];
    let mut a = p.live.agents[1].clone();
    a.deaf = Some("its session takes nothing".into());
    assert_eq!(super::side_line(p, &a).as_deref(), Some("takes no messages"));
    a.dialog = Some("Do you want to proceed?".into());
    assert_eq!(super::side_line(p, &a).as_deref(), Some(super::WAITING));
    p.live.agents[1] = a;
    assert_eq!((super::waiting(p), super::waiting_words(1), super::waiting_words(2)), (1, super::WAITING.to_string(), "2 waiting on a terminal prompt".to_string()));
}
