use crate::{App, Proj, Sel};

/// The texts and keys drawn inside pane J, not exiting.
fn in_pane(rt: &rune::runtime::Runtime<App>, j: usize) -> (Vec<String>, Vec<String>) {
    let pane = super::key(j);
    let els: Vec<_> = rt.elements().into_iter().filter(|e| !e.exiting && e.path.split('/').any(|k| k == pane)).collect();
    (els.iter().filter_map(|e| e.text.clone()).collect(), els.iter().filter_map(|e| e.key.clone()).collect())
}

/// The owner's N-2003 (1): a pane that is not focused keeps drawing the page it holds, of its own project, and never a
/// word to click its header in place of it.
#[test]
fn a_pane_not_focused_keeps_drawing_its_own_page_of_its_own_project() {
    let (_held, mut s) = crate::view_cost::test_app();
    let mut other = Proj::new(&crate::projects::ProjectConfig { name: "other".into(), id: "other".into(), boxed: false }, Vec::new());
    other.down = None;
    other.live.agents = vec![serde_json::from_value(serde_json::json!({"id": "AGT-9", "name": "zed-9", "parent": "owner", "kind": "claude", "model": "claude-opus-5-5", "state": "idle", "since": "2026-11-27T09:00:00Z", "tools": [], "items": []})).unwrap()];
    s.projects = vec![crate::view_cost::project_for_tests(), other];
    let ours = s.projects[0].live.agents[0].name.clone();
    // focused: research's Threads; beside it: other's Agents, then research's Overview and Settings
    (s.cur, s.sel, s.focus) = (0, Sel::Tickets, 0);
    s.panes = vec![(0, Sel::Tickets), (1, Sel::Agents), (0, Sel::View(String::new())), (0, Sel::View(crate::policy::ID.into()))];
    let mut rt = crate::view_cost::window(s);
    rt.frame(0.016);
    for j in 1..4 {
        let (texts, _) = in_pane(&rt, j);
        assert!(!texts.iter().any(|t| t.contains("Click the header")), "pane {j}: {texts:?}");
    }
    let (_, keys) = in_pane(&rt, 1);
    assert!(keys.iter().any(|k| k == "tile-zed-9"), "other's agent in its Agents pane: {keys:?}");
    assert!(!keys.iter().any(|k| *k == format!("tile-{ours}")), "not the focused project's agents");
    assert!(in_pane(&rt, 2).1.iter().any(|k| k == "home-repos"), "research's Overview in its pane");
    assert!(in_pane(&rt, 3).1.iter().any(|k| k == "settings-scopes"), "Settings in its pane");
    // a click inside the Agents pane focuses it, its page still drawn, now as the focused one
    rt.update(|s, _| s.focus_pane(1));
    rt.invalidate();
    rt.frame(0.016);
    assert_eq!((rt.state.cur, &rt.state.sel), (1, &Sel::Agents));
    assert!(in_pane(&rt, 1).1.iter().any(|k| k == "tile-zed-9"), "focused: other's agents");
    assert!(in_pane(&rt, 0).1.iter().all(|k| k != "tile-zed-9"), "the pane it left draws research's Threads");
}

/// Two projects, research focused on its Threads and other's Agents beside it.
fn split() -> App {
    let mut s = crate::view_cost::test_app().1;
    let mut other = Proj::new(&crate::projects::ProjectConfig { name: "other".into(), id: "other".into(), boxed: false }, Vec::new());
    other.down = None;
    other.live.agents = ["zed-9", "zed-10"]
        .iter()
        .map(|n| serde_json::from_value(serde_json::json!({"id": format!("AGT-{n}"), "name": n, "parent": "owner", "kind": "claude", "model": "claude-opus-5-5", "state": "idle", "since": "2026-11-27T09:00:00Z", "tools": [], "items": []})).unwrap())
        .collect();
    s.projects = vec![crate::view_cost::project_for_tests(), other];
    (s.cur, s.sel, s.focus) = (0, Sel::Tickets, 0);
    s.panes = vec![(0, Sel::Tickets), (1, Sel::Agents)];
    s
}

/// A project's Overview, its two repositories' cards: one with a built PDF in `.build/`, one with none.
fn card() -> App {
    let mut s = crate::view_cost::test_app().1;
    let mut p = Proj::new(&crate::projects::ProjectConfig { name: "walk".into(), id: "walk".into(), boxed: false }, Vec::new());
    p.down = None;
    p.protocol = coord_types::PROTOCOL;
    p.live.project = serde_json::json!({"project": {"repos": {"paper": "/srv/paper", "tools": "/srv/tools"}}});
    s.repo_state.shown.insert(
        "walk".into(),
        vec![
            crate::home::RepoState { name: "paper".into(), path: Some("/srv/paper".into()), branch: Some("main".into()), head: Some("3f2a1c9".into()), subject: Some("Section 2".into()), dirty: Some(2), pdf: Some(".build/main.pdf".into()), ..Default::default() },
            crate::home::RepoState { name: "tools".into(), path: Some("/srv/tools".into()), branch: Some("dev".into()), dirty: Some(0), ..Default::default() },
        ],
    );
    s.repo_state.asked.insert("walk".into(), std::time::Instant::now());
    s.projects = vec![p];
    s.cur = 0;
    s.sel = crate::home::sel(&s.projects[0]);
    s
}

/// The stills of the owner's N-2003 (1) and (3): `STILL_OUT=DIR cargo test n2003_stills -- --ignored`. Made-up data:
/// a split whose pane beside holds another project's Agents, and repository cards, one with its PDF.
#[test]
#[ignore]
fn n2003_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let shots: [(&str, fn() -> App); 2] = [("split-pane.png", split), ("repo-card.png", card)];
    for (name, start) in shots {
        let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), start, crate::view) }).args(args).try_run().unwrap();
    }
}
