//! What the code view keeps (`keep`): nothing asked before he uses it, only the views a pane shows and the shown
//! project's hidden for a while, a coordd left told, the look followed, a moved server's page followed.
use super::*;

/// Project ID linked to a pipe whose lines go to D/ID, at CODE_WARM_PROTOCOL; its pipe's child.
fn linked(d: &std::path::Path, id: &str) -> (crate::Proj, std::process::Child) {
    let out = d.join(id);
    let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
    let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: id.into(), id: id.into(), boxed: false }, Vec::new());
    p.protocol = coord_types::CODE_WARM_PROTOCOL;
    p.link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
    (p, child)
}

/// The lines project ID's coordd was sent holding NEEDLE, once they are AT LEAST many or a second has passed.
fn sent(d: &std::path::Path, id: &str, needle: &str, at_least: usize) -> usize {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(1);
    loop {
        let n = std::fs::read_to_string(d.join(id)).unwrap_or_default().lines().filter(|l| l.contains(needle)).count();
        if n >= at_least || std::time::Instant::now() > until {
            return n;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

const ASK: &str = r#""is":"workspace""#;
const LEFT: &str = r#""op":"codeviewleft""#;

/// Workspace FOLDER of this test run.
fn folder(tag: &str) -> String {
    format!("/c/{tag}-{}/repos.code-workspace", std::process::id())
}

/// coordd's answer to the ask under way: workspace FOLDER on PORT.
fn answer(s: &mut App, folder: &str, port: u16) {
    let (tx, r) = std::sync::mpsc::channel();
    s.code.asked.as_mut().expect("an ask under way").2 = r;
    tx.send(Ok(json!({"url": format!("http://127.0.0.1:{port}/?workspace={folder}&tkn=t"), "origin": format!("http://127.0.0.1:{port}"), "folder": folder}).to_string())).unwrap();
    assert!(ended(s));
}

/// Nothing is asked of a coordd before he uses the code view: Files shown asks for its workspace once, in the window's
/// look and both its colour sets, and its view is made; a refusal of that ask is said in Files and not asked again.
#[cfg(unix)]
#[test]
fn nothing_is_asked_until_files_is_shown_then_its_workspace_once() {
    let (_held, mut s, d) = super::walk();
    let (p, mut child) = linked(&d, "a");
    s.projects = vec![p];
    s.cur = 0;
    s.sel = crate::home::sel(&s.projects[0]);
    s.dark = Some(false);
    keep(&mut s);
    keep(&mut s);
    assert_eq!(sent(&d, "a", ASK, 1), 0, "nothing before he uses it");
    s.sel = Sel::Files;
    keep(&mut s);
    keep(&mut s);
    assert_eq!(sent(&d, "a", ASK, 2), 1, "Files shown: asked once");
    let line = std::fs::read_to_string(d.join("a")).unwrap();
    assert!(line.contains(r#""theme":"light""#), "{line}");
    let (dark, light) = (crate::look::kit(&s, true), crate::look::kit(&s, false));
    for c in [dark.c.bg_base, light.c.bg_base, light.c.layer_01, light.c.border_weak, light.c.text_base, light.c.text_accent, light.selection()] {
        assert!(line.contains(&format!("\"{}\"", c.hex())), "{} sent: {line}", c.hex());
    }
    let f = folder("first");
    answer(&mut s, &f, 4000);
    assert_eq!(s.sel, Sel::Files, "no pane opened");
    assert!(rune::web_surface::warm_waiting().iter().any(|w| w.id == format!("code:{f}")), "its view made");
    keep(&mut s);
    assert_eq!(sent(&d, "a", ASK, 2), 1, "nothing more");
    // refused: said in Files, not asked again
    s.code.open.clear();
    keep(&mut s);
    let (tx, r) = std::sync::mpsc::channel();
    s.code.asked.as_mut().unwrap().2 = r;
    tx.send(Err("openvscode-server is not installed on this machine".into())).unwrap();
    assert!(ended(&mut s));
    assert!(s.code.error.as_deref().is_some_and(|e| e.contains("not installed")));
    keep(&mut s);
    assert!(s.code.asked.is_none(), "not asked again");
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&d);
}

/// Three projects shown in turn: each one's view closed when he leaves it and its coordd told once, so one view is
/// kept; a view a pane beside still shows is kept and its coordd told nothing; going back asks again.
#[cfg(unix)]
#[test]
fn only_what_a_pane_shows_is_kept_and_a_coordd_left_is_told() {
    let (_held, mut s, d) = super::walk();
    let mut kids = Vec::new();
    s.projects.clear();
    for id in ["p0", "p1", "p2"] {
        let (p, c) = linked(&d, id);
        s.projects.push(p);
        kids.push(c);
    }
    s.sel = Sel::Files;
    for i in 0..3 {
        s.cur = i;
        assert_eq!(keep(&mut s), i > 0, "the last one's view closed");
        answer(&mut s, &folder(&format!("p{i}")), 4000 + i as u16);
        keep(&mut s);
    }
    assert_eq!(s.code.open.keys().cloned().collect::<Vec<_>>(), [folder("p2")], "one view kept");
    assert!(rune::web_surface::warm_waiting().iter().all(|w| w.id != format!("code:{}", folder("p0"))), "p0's closed before it was made");
    assert_eq!((sent(&d, "p0", LEFT, 1), sent(&d, "p1", LEFT, 1), sent(&d, "p2", LEFT, 1)), (1, 1, 0), "each coordd left told once");
    assert_eq!((sent(&d, "p0", ASK, 1), sent(&d, "p1", ASK, 1)), (1, 1), "and asked nothing more");
    // p1 in a pane beside, p2 focused: both kept, nobody told
    s.panes = vec![(1, Sel::Files), (2, Sel::Files)];
    s.focus = 1;
    keep(&mut s);
    answer(&mut s, &folder("p1"), 4001);
    assert!(!keep(&mut s), "nothing closed");
    assert_eq!(s.code.open.len(), 2, "the pane beside keeps its view");
    s.cur = 1;
    s.focus = 0;
    s.panes = vec![(1, Sel::Files), (2, Sel::Files)];
    assert!(!keep(&mut s), "focusing the other pane closes nothing");
    assert_eq!(sent(&d, "p2", LEFT, 1), 0);
    // back to p0 alone: asked again
    s.panes.clear();
    s.cur = 0;
    keep(&mut s);
    assert_eq!(sent(&d, "p0", ASK, 2), 2, "asked again");
    assert!(s.code.open.is_empty(), "the others closed");
    // an unpressed answer for a project no pane shows any more makes nothing
    s.cur = 1;
    s.sel = crate::home::sel(&s.projects[1]);
    answer(&mut s, &folder("p0"), 4000);
    assert!(s.code.open.is_empty());
    for mut c in kids {
        let _ = c.kill();
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// The shown project's view, no pane showing it, is kept hidden for KEPT_HIDDEN, then closed and its coordd told.
#[cfg(unix)]
#[test]
fn the_shown_projects_hidden_view_is_kept_a_while_then_let_go() {
    let (_held, mut s, d) = super::walk();
    let (p, mut child) = linked(&d, "h");
    s.projects = vec![p];
    s.cur = 0;
    s.sel = Sel::Files;
    keep(&mut s);
    answer(&mut s, &folder("hidden"), 4100);
    s.sel = crate::home::sel(&s.projects[0]);
    assert!(!keep(&mut s), "kept, hidden");
    assert_eq!(sent(&d, "h", LEFT, 1), 0);
    for o in s.code.open.values_mut() {
        o.seen -= KEPT_HIDDEN;
    }
    assert!(keep(&mut s), "let go");
    assert!(s.code.open.is_empty());
    assert_eq!(sent(&d, "h", LEFT, 1), 1, "its coordd told");
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&d);
}

/// A view follows the window's look, his or else the system's, asked again of its coordd in the new one; an answer
/// from a server that moved moves its view to the new page.
#[cfg(unix)]
#[test]
fn a_view_follows_the_windows_look_and_its_servers_page() {
    let (_held, mut s, d) = super::walk();
    let (p, mut child) = linked(&d, "t");
    s.projects = vec![p];
    s.cur = 0;
    s.sel = Sel::Files;
    s.dark = Some(true);
    let f = folder("look");
    keep(&mut s);
    answer(&mut s, &f, 4200);
    keep(&mut s);
    assert!(s.code.asked.is_none(), "in the window's look already");
    s.dark = Some(false);
    keep(&mut s);
    assert_eq!(sent(&d, "t", r#""theme":"light""#, 1), 1, "asked in the new look");
    answer(&mut s, &f, 4200);
    keep(&mut s);
    assert!(s.code.asked.is_none());
    // none picked: the system's
    s.dark = None;
    s.code.system_dark.set(Some(true));
    keep(&mut s);
    assert_eq!(sent(&d, "t", r#""theme":"dark""#, 2), 2, "the system's look");
    // its server moved: the view moved to the new page
    answer(&mut s, &f, 4201);
    assert!(s.code.open[&f].url.contains(":4201/"));
    let w = rune::web_surface::warm_waiting().into_iter().find(|w| w.id == format!("code:{f}")).expect("its view");
    assert!(w.url.contains(":4201/"), "{}", w.url);
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&d);
}
