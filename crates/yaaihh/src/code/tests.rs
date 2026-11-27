use super::*;
use rune::runtime::Runtime;
use serde_json::json;

pub(super) fn settle(rt: &mut Runtime<App>) {
    rt.invalidate();
    for _ in 0..3 {
        rt.frame(0.016);
    }
}

fn keyed(rt: &mut Runtime<App>, key: &str) {
    let r = rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key}")).rect;
    let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    rt.pointer_move(x, y);
    rt.frame(0.016);
    rt.pointer_down(x, y);
    rt.frame(0.016);
    rt.pointer_up(x, y);
    settle(rt);
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect()
}

fn keys(rt: &Runtime<App>, prefix: &str) -> Vec<String> {
    rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.key).filter(|k| k.starts_with(prefix)).collect()
}

/// Project "walk" at the code view's protocol, its one repository at a folder of its own, Home shown.
pub(super) fn walk() -> (std::sync::MutexGuard<'static, ()>, App, std::path::PathBuf) {
    let (_held, mut s) = crate::home::tests::made();
    let d = std::env::temp_dir().join(format!("yaaihh-code-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    s.projects[0].live.project = json!({"project": {"name": "walk", "repos": {"soft-swarm": d.display().to_string()}}});
    s.projects[0].protocol = coord_types::CODE_VIEW_PROTOCOL;
    s.sel = crate::home::sel(&s.projects[0]);
    (_held, s, d)
}

const W: &str = "/c/codeview/repos.code-workspace";

/// coordd's answer for workspace W at URL, to an ask that SHOWs Files or not.
fn answered(rt: &mut Runtime<App>, show: bool, url: &str) {
    let (tx, rx) = std::sync::mpsc::channel();
    rt.update(|s, _| s.code.asked = Some((0, show, rx)));
    tx.send(Ok(json!({"url": url, "origin": "http://127.0.0.1:4000", "folder": W}).to_string())).unwrap();
    assert!(rt.update_if(|s, _| ended(s)));
    settle(rt);
}

/// The one entry, Files, asks the code view; its answer shows in Files, in a pane beside, focused, its web view in it
/// and in sight when the pane beside is focused; a second answer of the same server keeps the page.
#[test]
fn a_repositorys_files_asks_the_code_view_and_its_answer_shows_in_files_beside() {
    let (_held, s, d) = walk();
    let mut rt = crate::view_cost::window(s);
    settle(&mut rt);
    assert!(keys(&rt, "home-code-").is_empty(), "no Code beside Files");
    keyed(&mut rt, "home-repo-soft-swarm");
    assert!(texts(&rt).iter().all(|t| !t.contains("browser")));
    assert_eq!(rt.state.code.error.as_deref(), Some("walk is not reached now."));
    answered(&mut rt, true, &format!("http://127.0.0.1:4000/?workspace={W}&tkn=t0"));
    assert_eq!(rt.state.sel, Sel::Files, "Files in a pane beside, focused");
    assert!(!rt.state.panes.is_empty(), "Home stays in the pane it was in");
    let webs = rt.web_surfaces();
    assert_eq!(webs.len(), 1, "one web view");
    assert_eq!((webs[0].0.id.as_str(), webs[0].0.url.as_str()), (format!("code:{W}").as_str(), format!("http://127.0.0.1:4000/?workspace={W}&tkn=t0").as_str()));
    let (pane, _) = rt.presented("code-view").expect("its pane");
    let [x, y, w, h] = webs[0].1;
    assert!(x >= pane[0] - 0.5 && y >= pane[1] - 0.5 && x + w <= pane[0] + pane[2] + 0.5 && y + h <= pane[1] + pane[3] + 0.5, "{:?} in {pane:?}", webs[0].1);
    assert!(texts(&rt).iter().all(|t| !t.contains("tkn=")), "the token is drawn nowhere");
    // the pane beside focused: the code view stays in sight
    let files = rt.state.focus;
    rt.update(|s, _| s.focus_pane(files - 1));
    settle(&mut rt);
    assert_eq!(rt.web_surfaces().len(), 1, "in sight beside");
    rt.update(|s, _| s.focus_pane(files));
    settle(&mut rt);
    // a second answer of the same server keeps its page
    answered(&mut rt, true, &format!("http://127.0.0.1:4000/?workspace={W}&tkn=t1"));
    assert!(rt.web_surfaces()[0].0.url.ends_with("tkn=t0"), "not reloaded");
    // the pane closed: its view goes with it
    let j = rt.state.focus;
    rt.update(|s, _| s.close_pane(j));
    settle(&mut rt);
    assert!(rt.web_surfaces().is_empty(), "hidden with its pane");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_answer_off_this_machine_or_off_its_origin_opens_nothing() {
    let (_held, s, d) = walk();
    let mut rt = crate::view_cost::window(s);
    settle(&mut rt);
    for url in ["http://evil.example:4000/?workspace=/r", "http://127.0.0.1:4001/?workspace=/r", "http://127.0.0.1:4000.evil.example/"] {
        answered(&mut rt, true, url);
        assert_eq!(rt.state.code.error.as_deref(), Some("Code view not on this machine."), "{url}");
        assert!(rt.web_surfaces().is_empty());
    }
    let (tx, rx) = std::sync::mpsc::channel();
    rt.update(|s, _| s.code.asked = Some((0, true, rx)));
    tx.send(Err("openvscode-server is not installed on this machine".into())).unwrap();
    assert!(rt.update_if(|s, _| ended(s)));
    assert_eq!(rt.state.code.error.as_deref(), Some("openvscode-server is not installed on this machine"));
    let _ = std::fs::remove_dir_all(&d);
}

/// The answer [`code_live`]'s window shows.
static LIVE: std::sync::Mutex<Option<CodeViewAnswer>> = std::sync::Mutex::new(None);

/// [`walk`] with LIVE's workspace in Files, focused.
fn live() -> App {
    let (_held, mut s, _) = walk();
    s.dark = Some(true);
    let a = LIVE.lock().unwrap().clone().expect("an answer");
    let project = s.projects[0].id.clone();
    s.code.open.insert(a.folder.clone(), Opened { url: a.url, origin: a.origin, project, dark: true, seen: std::time::Instant::now() });
    s.sel = Sel::Files;
    s
}

/// By hand, on Windows: a real window (not headless) showing coordd's answer CODE_ANSWER in its code pane, for
/// CODE_SECONDS, so a capture of it shows the page inside the pane. `CODE_ANSWER=FILE yaaihh-tests code::tests::code_live --ignored --exact`.
#[test]
#[ignore]
fn code_live() {
    let Ok(f) = std::env::var("CODE_ANSWER") else { return };
    let a: CodeViewAnswer = serde_json::from_str(&std::fs::read_to_string(f).unwrap()).unwrap();
    *LIVE.lock().unwrap() = Some(a);
    let secs: u64 = std::env::var("CODE_SECONDS").ok().and_then(|s| s.parse().ok()).unwrap_or(40);
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(secs));
        std::process::exit(0);
    });
    let spec = rune::AppSpec { poll: Some(crate::poll), on_key: Some(crate::on_key), families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh code view", (1400.0, 900.0), live, crate::view) };
    rune::App::new(spec).args(Vec::<String>::new()).try_run().unwrap();
}

/// What a test's window would have opened in his browser, named by NEEDLE.
fn opened(needle: &str) -> Vec<String> {
    crate::views::OPENED.lock().unwrap().iter().filter(|t| t.contains(needle)).cloned().collect()
}

/// Overview's row and the Repos drawer have one entry each, Files (on a card, the card and its Files icon, the
/// owner's N-2003), which asks the code view, and none opens his browser; a coordd without the code view shows the
/// Files page under the same entry.
#[test]
fn every_repository_entry_is_files_and_nothing_opens_outside() {
    let (_held, s, d) = walk();
    let mut rt = crate::view_cost::window(s);
    settle(&mut rt);
    assert!(texts(&rt).iter().all(|t| !t.contains("Klaus") && !t.contains("browser")), "{:?}", texts(&rt));
    assert!(keys(&rt, "home-code-").is_empty(), "no Code entry");
    keyed(&mut rt, "home-files-soft-swarm");
    assert_eq!(rt.state.code.error.as_deref(), Some("walk is not reached now."), "the Files icon asks the code view, as the card does");
    rt.update(|s, _| (s.code.error, s.sel) = (None, crate::home::sel(&s.projects[0])));
    settle(&mut rt);
    assert!(!texts(&rt).iter().any(|t| t.starts_with("Code")), "no Code entry: {:?}", texts(&rt));
    keyed(&mut rt, "home-repo-soft-swarm");
    assert_eq!(rt.state.code.error.as_deref(), Some("walk is not reached now."), "the code view asked");
    let folder = d.display().to_string();
    rt.update(|s, _| (s.projects[0].protocol, s.code.error) = (coord_types::REPO_VIEW_PROTOCOL, None));
    settle(&mut rt);
    keyed(&mut rt, "home-repo-soft-swarm");
    assert_eq!(rt.state.sel, Sel::Files);
    assert!(rt.state.code.asked.is_none(), "below the code view: the Files page, nothing asked");
    assert!(opened("soft-swarm").is_empty() && opened(&folder).is_empty(), "nothing opened outside: {:?}", opened(""));
    let _ = std::fs::remove_dir_all(&d);
}

/// A file a post links goes to coordd, and once found opens in the code view at its line; a coordd without the code
/// view says so on the window's line, and neither opens anything outside. A web link alone opens in his browser.
#[test]
fn a_file_link_opens_in_the_code_view_and_only_a_web_link_outside() {
    let (_held, mut s, d) = walk();
    crate::paths::open("file:src/kick_out.rs:12", Some(d.display().to_string()));
    crate::paths::ask_located(&mut s);
    assert!(s.notice.as_deref().is_some_and(|n| n.starts_with("src/kick_out.rs: ")), "asked of coordd, not opened here: {:?}", s.notice);
    let found = json!({"path": "/srv/walk/src/kick_out.rs", "readable": true}).to_string();
    crate::paths::located(&mut s, ":12".into(), false, true, found.clone());
    assert_eq!(s.code.error.as_deref(), Some("walk is not reached now."), "the code view asked");
    (s.projects[0].protocol, s.code.error, s.notice) = (coord_types::REPO_VIEW_PROTOCOL, None, None);
    crate::paths::located(&mut s, ":12".into(), false, true, found);
    assert!(s.notice.as_deref().is_some_and(|n| n.ends_with(crate::paths::CODE_VIEW_NEEDED)), "said on the window's line: {:?}", s.notice);
    crate::views::open("/srv/walk/src/kick_out.rs".into());
    crate::paths::open("src/kick_out.rs", None);
    assert!(opened("kick_out").is_empty(), "no file opened outside: {:?}", opened("kick_out"));
    crate::paths::open("https://example.org/kick-out-web", None);
    assert_eq!(opened("kick-out-web"), ["https://example.org/kick-out-web"], "a web link, in his browser");
    crate::paths::ask_located(&mut s);
    let _ = std::fs::remove_dir_all(&d);
}

/// A PDF is drawn by Files' own view where the code view is served: opened from a path, Files shows its page and asks
/// the code view nothing; a press of a repository's Files after it shows the code view again.
#[cfg(unix)]
#[test]
fn a_pdf_opens_in_files_own_view_where_the_code_view_is_served() {
    use crate::files::{Asked, Body, Read};
    use base64::Engine;
    let (_held, mut s, d) = walk();
    s.projects[0].protocol = coord_types::CODE_WARM_PROTOCOL;
    let out = d.join("sent");
    let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
    s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
    let path = std::path::PathBuf::from("/srv/paper/main.pdf");
    crate::elsewhere::as_on_windows(|| {
        crate::files::open_at(&mut s, "/srv/paper/main.pdf");
        assert!(!shows(&s, 0) && s.code.asked.is_none(), "the code view asked nothing");
        let page = json!({"pages": 2, "page": 1, "data": base64::engine::general_purpose::STANDARD.encode(crate::files::demo_png_bytes())}).to_string();
        crate::files::replied(&mut s, Asked::Pdf { path: path.clone(), size: 9 << 20, modified: None, page: 1 }, true, page);
        assert!(matches!(&s.files.file, Some(Read { body: Body::Pdf { page: 1, pages: 2 }, .. })));
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        assert_eq!(rt.state.sel, Sel::Files);
        assert!(rt.presented("code-view").is_none() && rt.web_surfaces().is_empty(), "Files' own page");
        assert!(rt.state.images.contains_key(&crate::files::image_key(&path)), "its page drawn");
        rt.update(|s, _| crate::home::show_files(s, 0, "soft-swarm", ""));
        assert!(rt.state.code.asked.is_some() && shows(&rt.state, 0), "the code view again");
    });
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&d);
}

#[path = "keep_tests.rs"]
mod keep;
