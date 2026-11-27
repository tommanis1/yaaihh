//! An agent's drawers, `Repos · Inspect · Terminal`: toggles in its page's head, one drawer open beside the chat.
//!
//! - **Repos**: its home (`agents/NAME/home`) and the project's repositories, each with its folder and a button to it
//!   (the code view for a repository, else Files), what git sees changed where it works, its commits.
//! - **Inspect**: what its model is sent, in tabs over a footer that stages and applies (inspect.rs).
//! - **Terminal**: its terminal fitted to the drawer; Full size makes it the page.

use crate::App;
use coord_types::AgentView;
use rune::{El, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A drawer's width beside the chat; Inspect's, whose six tabs and footer need more.
const WIDE: f32 = 460.0;
const WIDE_INSPECT: f32 = 600.0;
/// How often an open Repos drawer reads again what changed in the agent's folder.
const CHANGES_EVERY: Duration = Duration::from_secs(5);

/// The drawers an agent's page opens beside its chat.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Drawer {
    Repos,
    Inspect,
    Terminal,
}

impl Drawer {
    const ALL: [Drawer; 3] = [Drawer::Repos, Drawer::Inspect, Drawer::Terminal];

    fn word(self) -> &'static str {
        match self {
            Drawer::Repos => "Repos",
            Drawer::Inspect => "Inspect",
            Drawer::Terminal => "Terminal",
        }
    }

    fn help(self) -> &'static str {
        match self {
            Drawer::Repos => "Its folder, changes and commits",
            Drawer::Inspect => "What its model is sent",
            Drawer::Terminal => "Its terminal, beside the chat",
        }
    }
}

/// The open drawers, and what changed in the folders the Repos drawers show.
#[derive(Default)]
pub struct Drawers {
    /// The drawer open beside each agent's chat, by project and name.
    pub open: HashMap<(usize, String), Drawer>,
    /// What changed in each folder an open Repos drawer shows ([`changes`]), and when it was read.
    pub changes: HashMap<PathBuf, (Instant, Option<String>)>,
    /// The tab each agent's Inspect drawer shows, by project and name: Diff until he picks another.
    pub tabs: HashMap<(usize, String), crate::inspect::Tab>,
    /// The agent whose Inspect footer has its Branch from turn list open.
    pub branching: Option<(usize, String)>,
    /// The harness switches staged in each agent's Inspect drawer, by project and name.
    pub switches: HashMap<(usize, String), crate::inspect::Switches>,
}

/// The agents whose Inspect drawer is on screen, by project and name: beside the page shown, or a split's other page.
pub fn inspected(s: &App) -> Vec<(usize, String)> {
    let others = s.panes.iter().enumerate().filter(|(j, _)| *j != s.focus).map(|(_, (pi, sel))| (*pi, sel));
    let mut out = Vec::new();
    for (pi, sel) in std::iter::once((s.cur, &s.sel)).chain(others) {
        if let crate::Sel::Agent(n) = sel {
            if is_open(s, pi, n, Drawer::Inspect) && !out.iter().any(|(p, m)| *p == pi && m == n) {
                out.push((pi, n.clone()));
            }
        }
    }
    out
}

/// Whether drawer D is open beside agent NAME's chat in project PI.
pub fn is_open(s: &App, pi: usize, name: &str, d: Drawer) -> bool {
    s.drawers.open.get(&(pi, name.to_string())) == Some(&d)
}

/// The three toggles for agent NAME of project PI, the open one lit; Terminal lit too while the terminal is the page.
pub fn toggles(s: &App, pi: usize, name: &str, k: &Kit) -> El {
    let key = (pi, name.to_string());
    let open = s.drawers.open.get(&key).copied();
    let full = s.term.pages.contains(&key);
    row().gap(2.0).center_y().children(Drawer::ALL.map(|d| {
        let on = open == Some(d) || (d == Drawer::Terminal && full);
        let at = key.clone();
        let b = Button::new(d.word()).variant(if on { ButtonVariant::Outline } else { ButtonVariant::Ghost }).on_press(move |s: &mut App| press(s, &at, d)).build(k).key(format!("drawer-{}-{name}", d.word().to_lowercase()));
        // an open drawer's toggle says nothing over it: its tip covered the tabs it had just opened
        if on { b } else { b.help(d.help()) }
    }))
}

/// A press on toggle D of agent KEY: the terminal shown as the page goes back to the chat; the open drawer shuts;
/// another opens in its place.
fn press(s: &mut App, key: &(usize, String), d: Drawer) {
    if d == Drawer::Terminal && s.term.pages.remove(key) {
        return;
    }
    if s.drawers.open.get(key) == Some(&d) {
        s.drawers.open.remove(key);
        return;
    }
    s.drawers.open.insert(key.clone(), d);
    if d == Drawer::Repos
        && let Some(dir) = folder(s, key.0, &key.1)
    {
        crate::files::open_in(s, key.0, &dir);
        read_changes(s, &dir);
    }
}

/// Agent NAME's folder in project PI, as coordd gives it.
fn folder(s: &App, pi: usize, name: &str) -> Option<PathBuf> {
    s.projects.get(pi)?.live.agents.iter().find(|a| a.name == name)?.cwd.as_deref().filter(|c| !c.is_empty()).map(PathBuf::from)
}

/// The update step: an open Repos drawer's folder looked at as Files looks at its own, and what changed in it read
/// again every [`CHANGES_EVERY`]. Whether something it shows changed.
pub fn poll(s: &mut App) -> bool {
    let dirs: Vec<PathBuf> = s.drawers.open.iter().filter(|(_, d)| **d == Drawer::Repos).filter_map(|((pi, n), _)| folder(s, *pi, n)).collect();
    if dirs.is_empty() {
        return false;
    }
    let mut changed = crate::files::look(s);
    for d in dirs {
        if s.drawers.changes.get(&d).is_none_or(|(at, _)| at.elapsed() >= CHANGES_EVERY) {
            changed |= read_changes(s, &d);
        }
    }
    changed
}

/// What changed in DIR read again; whether its line changed. Where the window runs apart from the folders (Windows,
/// elsewhere.rs) it reads none: coordd gives no such summary.
fn read_changes(s: &mut App, dir: &Path) -> bool {
    let now = if crate::elsewhere::away() { None } else { changes(dir) };
    let was = s.drawers.changes.insert(dir.to_path_buf(), (Instant::now(), now.clone()));
    was.map(|w| w.1) != Some(now)
}

/// What changed in folder DIR as git sees it, in one line ([`summary`]); None outside a repository or with no git.
/// Git is only read (no optional locks).
fn changes(dir: &Path) -> Option<String> {
    let git = |args: &[&str]| {
        let mut c = crate::places::command("git");
        c.arg("--no-optional-locks").arg("-C").arg(dir).args(args).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        c.output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
    };
    let status = git(&["status", "--porcelain=v1", "--branch"])?;
    let stat = git(&["diff", "--shortstat", "HEAD"]).unwrap_or_default();
    Some(summary(&status, &stat))
}

/// `git status --porcelain=v1 --branch` and `git diff --shortstat HEAD` in one line: `on BRANCH · 3 changed · 1 new ·
/// +120 −14`, or `on BRANCH · nothing changed`.
fn summary(status: &str, shortstat: &str) -> String {
    let mut lines = status.lines();
    let branch = lines.next().and_then(|l| l.strip_prefix("## ")).map(|b| {
        let b = b.strip_prefix("No commits yet on ").unwrap_or(b);
        if b.starts_with("HEAD (no branch)") { "a detached head".to_string() } else { b.split("...").next().unwrap_or(b).split(' ').next().unwrap_or(b).to_string() }
    });
    let (mut changed, mut new) = (0, 0);
    for l in lines.filter(|l| !l.is_empty()) {
        if l.starts_with("??") {
            new += 1;
        } else {
            changed += 1;
        }
    }
    let count = |word: &str| shortstat.split(',').find(|p| p.contains(word)).and_then(|p| p.split_whitespace().next()?.parse::<u64>().ok()).unwrap_or(0);
    let (plus, minus) = (count("insertion"), count("deletion"));
    let mut parts: Vec<String> = branch.into_iter().map(|b| format!("on {b}")).collect();
    if changed + new == 0 {
        parts.push("nothing changed".into());
    }
    if changed > 0 {
        parts.push(format!("{changed} changed"));
    }
    if new > 0 {
        parts.push(format!("{new} new"));
    }
    if plus + minus > 0 {
        parts.push(format!("+{plus} −{minus}"));
    }
    parts.join(" · ")
}

/// BODY with the drawer DRAWER at its right, when one is open.
pub fn beside(body: El, drawer: Option<El>) -> El {
    match drawer {
        None => body,
        Some(d) => row().w_full().grow(1.0).min_h(0.0).child(col().grow(1.0).min_w(0.0).h_full().min_h(0.0).child(body)).child(d),
    }
}

/// The drawer open beside agent A's chat in project PI, if any: its name and ×, then what it holds.
pub fn view(s: &App, pi: usize, a: &AgentView, k: &Kit) -> Option<El> {
    let d = s.drawers.open.get(&(pi, a.name.clone())).copied()?;
    let body = match d {
        Drawer::Repos => repos(s, pi, a, k),
        Drawer::Inspect => crate::inspect::view(s, pi, a, k),
        Drawer::Terminal => terminal(s, pi, &a.name, k),
    };
    let key = (pi, a.name.clone());
    let close = Button::new("×")
        .variant(ButtonVariant::Ghost)
        .help("Shut it")
        .on_press(move |s: &mut App| {
            s.drawers.open.remove(&key);
        })
        .build(k)
        .key(format!("drawer-close-{}", a.name));
    let head = row().w_full().px(12.0).py(6.0).gap(8.0).center_y().child(text(d.word()).font(k.ty.small_medium).color(k.c.text_muted)).child(rune::el().grow(1.0)).child(close);
    Some(
        col()
            .key(format!("drawer-{pi}-{}", a.name))
            .w(if d == Drawer::Inspect { WIDE_INSPECT } else { WIDE })
            .h_full()
            .min_h(0.0)
            .border_sides([0.0, 0.0, 0.0, 1.0], k.c.border_weak)
            .child(head)
            .child(col().w_full().grow(1.0).min_h(0.0).child(body)),
    )
}

/// The Repos drawer of agent A in project PI: Agent Home, then Project Repos, each a name, its folder and its button;
/// where A works, git's line of what changed under it.
fn repos(s: &App, pi: usize, a: &AgentView, k: &Kit) -> El {
    let Some(p) = s.projects.get(pi) else { return col() };
    let dir = folder(s, pi, &a.name);
    let same = |f: &str| dir.as_deref().is_some_and(|d| d.to_string_lossy().trim_end_matches('/') == f.trim_end_matches('/'));
    let changes = |f: &str| dir.as_ref().filter(|_| same(f)).and_then(|d| s.drawers.changes.get(d)).and_then(|(_, l)| l.clone());
    let place = |key: String, name: &str, f: &str, b: El| {
        let mut left = col().grow(1.0).shrink().min_w(0.0).gap(2.0).child(text(name.to_string()).font(k.ty.ui).color(k.c.text_base));
        left = left.child(text(f.to_string()).font(k.ty.small).color(k.c.text_muted).wrap().key(format!("{key}-path")));
        if let Some(line) = changes(f) {
            left = left.child(text(line).font(k.ty.small).color(k.c.text_base).key("drawer-changes"));
        }
        row().key(key).w_full().gap(8.0).center_y().child(left).child(b)
    };
    let heading = |t: &str| text(t.to_string()).font(k.ty.small_medium).color(k.c.text_muted);
    let mut c = col().w_full().px(12.0).gap(6.0).key("drawer-folder").child(heading("Agent Home"));
    match crate::home::home_of(p, &a.name) {
        Some(h) => {
            let (n, f) = (a.name.clone(), h.clone());
            let open = Button::new("Files").variant(ButtonVariant::Ghost).help("Its home in Files").on_press(move |s: &mut App| crate::files::open_folder(s, &n, &f)).build(k);
            c = c.child(place("drawer-home".into(), "Home", &h, open));
        }
        None => c = c.child(text("Its home is not known.").font(k.ty.small).color(k.c.text_muted)),
    }
    c = c.child(heading("Project Repos"));
    let list = crate::home::resolved_repos(p);
    if list.is_empty() {
        c = c.child(text("No repository yet").font(k.ty.small).color(k.c.text_muted).key("drawer-no-repo"));
    }
    for (n, f) in list {
        let b = crate::home::files_button(pi, &n, &f, k);
        c = c.child(place(format!("drawer-repo-{n}"), &n, &f, b));
    }
    let mut out = col().w_full().h_full().min_h(0.0).gap(8.0).child(c).child(crate::chat::commits(s, pi, &a.name, k));
    if let Some(d) = dir {
        out = out.child(crate::files::tree_at(s, p, &d, k));
    }
    out
}

/// The Terminal drawer of agent NAME in project PI: Full size, then its terminal fitted to the drawer.
fn terminal(s: &App, pi: usize, name: &str, k: &Kit) -> El {
    let key = (pi, name.to_string());
    let full = Button::new("Full size")
        .variant(ButtonVariant::Ghost)
        .help("Terminal, full size")
        .on_press(move |s: &mut App| {
            s.drawers.open.remove(&key);
            s.term.pages.insert(key.clone());
        })
        .build(k)
        .key(format!("drawer-term-full-{name}"));
    col().w_full().h_full().min_h(0.0).px(8.0).pb(8.0).gap(6.0).child(row().w_full().justify_end().child(full)).child(crate::term::view(s, k, pi, name, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What changed in a folder, in one line, from git's own words.
    #[test]
    fn a_folders_changes_are_one_line() {
        assert_eq!(summary("## main...origin/main [ahead 1]\n M src/a.rs\nM  src/b.rs\n?? notes.md\n", " 2 files changed, 120 insertions(+), 14 deletions(-)\n"), "on main · 2 changed · 1 new · +120 −14");
        assert_eq!(summary("## phase1-window-hci\n", ""), "on phase1-window-hci · nothing changed");
        assert_eq!(summary("## No commits yet on main\n?? a\n", ""), "on main · 1 new");
        assert_eq!(summary("## HEAD (no branch)\n D gone.rs\n", " 1 file changed, 3 deletions(-)\n"), "on a detached head · 1 changed · +0 −3");
    }

    /// A made-up agent `try-1` working in a made-up repository with one file changed and one new, its project naming the
    /// repository, two commits of its own; drawer D open beside its chat.
    fn still_app(d: Drawer) -> App {
        let (_held, mut s) = crate::home::tests::made();
        let dir = std::env::temp_dir().join(format!("yaaihh-drawer-still-{}", std::process::id()));
        if !dir.join(".git").is_dir() {
            std::fs::create_dir_all(dir.join("src")).unwrap();
            std::fs::write(dir.join("src/lexer.rs"), "pub fn lex(s: &str) -> Vec<&str> {\n    s.split_whitespace().collect()\n}\n").unwrap();
            std::fs::write(dir.join("README.md"), "# walk\n\nA lexer, step by step.\n").unwrap();
            let git = |a: &[&str]| std::process::Command::new("git").arg("-C").arg(&dir).args(a).output().unwrap();
            git(&["init", "-q", "-b", "lexer"]);
            git(&["add", "."]);
            // a commit here carries an Item line, as every commit on this machine does
            git(&["-c", "user.name=t", "-c", "user.email=t@example.invalid", "-c", "commit.gpgsign=false", "commit", "-q", "-m", "the lexer", "-m", "Item: N-1790"]);
            std::fs::write(dir.join("src/lexer.rs"), "/// Its words, split at white space.\npub fn lex(s: &str) -> Vec<&str> {\n    s.split_whitespace().collect()\n}\n").unwrap();
            std::fs::write(dir.join("src/parse.rs"), "// the parser, next\n").unwrap();
        }
        let path = dir.display().to_string();
        let p = &mut s.projects[0];
        p.live.project = serde_json::json!({"project": {"repos": {"walk": path}}});
        p.live.agents[0].cwd = Some(path);
        p.commits = vec![
            crate::Commit { hash: "3b2c4ac5e0".into(), at: "2026-11-24T09:12:00Z".into(), subject: "the lexer splits at white space".into(), agent: "try-1".into(), ..Default::default() },
            crate::Commit { hash: "28684eeb11".into(), at: "2026-11-24T08:40:00Z".into(), subject: "a first test of the lexer".into(), agent: "try-1".into(), ..Default::default() },
        ];
        s.sel = crate::Sel::Agent("try-1".into());
        s.drawers.open.insert((0, "try-1".into()), d);
        if d == Drawer::Repos {
            crate::files::open_in(&mut s, 0, &dir.join("src"));
            let lexer = std::fs::canonicalize(dir.join("src/lexer.rs")).unwrap();
            s.files.kept.entry("walk".into()).or_default().shown = Some(lexer.display().to_string());
            crate::files::refresh(&mut s, true);
            read_changes(&mut s, &dir);
        }
        s
    }

    fn shoot(name: &str, head: fn() -> App) {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        let snap = out.join(name).display().to_string();
        let a: [&str; 6] = ["--snap", &snap, "--size", "1400x900", "--at", "1"];
        let spec = rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), head, crate::view) };
        rune::App::new(spec).args(a.map(String::from).to_vec()).try_run().unwrap();
    }

    /// The Repos drawer, made up, in `STILL_OUT`: `STILL_OUT=DIR yaaihh-tests drawer::tests::repos_still --ignored
    /// --exact`.
    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn repos_still() {
        shoot("drawer-repos.png", || still_app(Drawer::Repos));
    }

    /// The Terminal drawer, made up (no terminal attached), in `STILL_OUT`.
    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn terminal_still() {
        shoot("drawer-terminal.png", || still_app(Drawer::Terminal));
    }

    /// Repos lists Agent Home, its folder in the coord's (`agents/NAME/home`), then Project Repos, each repository by
    /// name with the folder coordd resolved for it (a relative or `~/` one in the file is whole here), each with its
    /// button; Home's opens Files at that folder, a root of the project's Files. From an older coordd that sends no
    /// resolved folders, the file's own; with none, "No repository yet".
    #[test]
    fn repos_lists_the_agents_home_and_the_projects_repositories_with_their_folders() {
        let (_held, mut s) = crate::home::tests::made();
        let coord = std::env::temp_dir().join(format!("yaaihh-drawer-home-{}", std::process::id()));
        let home = coord.join("agents/try-1/home");
        std::fs::create_dir_all(home.join("sources")).unwrap();
        let (c, h) = (coord.display().to_string(), home.display().to_string());
        let p = &mut s.projects[0];
        p.live.project = serde_json::json!({"project": {"repos": {"repo-1": "../src/one", "repo-2": "~/two"}}});
        p.live.places = Some(coord_types::Places { coord: c.clone(), repos: vec![("repo-1".into(), "/srv/src/one".into()), ("repo-2".into(), "/home/u/two".into())], ..Default::default() });
        p.live.agents[0].cwd = Some(h.clone());
        s.sel = crate::Sel::Agent("try-1".into());
        s.drawers.open.insert((0, "try-1".into()), Drawer::Repos);
        let mut rt = crate::view_cost::window(s);
        crate::home::tests::settle(&mut rt);
        let t = crate::home::tests::texts(&rt);
        for w in ["Agent Home", "Home", h.as_str(), "Project Repos", "repo-1", "/srv/src/one", "repo-2", "/home/u/two"] {
            assert!(t.iter().any(|x| x == w), "{w}: {t:?}");
        }
        assert!(!t.iter().any(|x| x == "../src/one" || x == "~/two"), "the resolved folders, not the file's: {t:?}");
        let roots = crate::files::roots(&rt.state.projects[0]);
        assert!(roots.iter().any(|r| r.name == "try-1 home" && r.path == std::fs::canonicalize(&home).unwrap()), "its home is a root of Files: {roots:?}");
        rt.update(|s, _| s.projects[0].live.places = Some(coord_types::Places { coord: c.clone(), ..Default::default() }));
        crate::home::tests::settle(&mut rt);
        let t = crate::home::tests::texts(&rt);
        assert!(t.iter().any(|x| x == "../src/one"), "an older coordd: the file's own: {t:?}");
        rt.update(|s, _| s.projects[0].live.project = serde_json::json!({"project": {}}));
        crate::home::tests::settle(&mut rt);
        assert!(crate::home::tests::texts(&rt).iter().any(|x| x == "No repository yet"));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// The agent's head holds Repos · Inspect · Terminal; each opens its drawer beside the chat, one at a time, a press
    /// on the open one shuts it; the Terminal drawer's Full size makes the terminal the page, and its lit toggle takes
    /// the page back to the chat.
    #[test]
    fn an_agents_drawers_open_one_at_a_time_beside_its_chat() {
        let (_held, mut s) = crate::home::tests::made();
        let name = "try-1".to_string();
        s.sel = crate::Sel::Agent(name.clone());
        let mut rt = crate::view_cost::window(s);
        rt.frame(0.016);
        let press = |rt: &mut rune::runtime::Runtime<App>, key: &str| {
            let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} drawn"));
            rt.pointer_down(x, y);
            rt.pointer_up(x, y);
            rt.frame(0.016);
            rt.frame(0.016);
        };
        let drawer = format!("drawer-0-{name}");
        for d in ["repos", "inspect", "terminal"] {
            assert!(rt.find(&format!("drawer-{d}-{name}")).is_some(), "{d} in the head");
        }
        assert!(rt.find(&drawer).is_none(), "none open at first");
        press(&mut rt, &format!("drawer-repos-{name}"));
        assert_eq!(rt.state.drawers.open.get(&(0, name.clone())), Some(&Drawer::Repos));
        assert!(rt.find(&drawer).is_some() && rt.find("drawer-folder").is_some(), "Repos open beside the chat");
        press(&mut rt, &format!("drawer-inspect-{name}"));
        assert_eq!(rt.state.drawers.open.get(&(0, name.clone())), Some(&Drawer::Inspect), "one at a time");
        assert!(rt.find("drawer-folder").is_none() && rt.find(&format!("inspect-tabs-{name}")).is_some(), "Inspect's tabs (inspect.rs)");
        press(&mut rt, &format!("drawer-inspect-{name}"));
        assert!(rt.find(&drawer).is_none(), "a press on the open one shuts it");
        press(&mut rt, &format!("drawer-terminal-{name}"));
        assert!(is_open(&rt.state, 0, &name, Drawer::Terminal) && rt.find(&format!("term-0-{name}")).is_some(), "its terminal in the drawer");
        press(&mut rt, &format!("drawer-term-full-{name}"));
        assert!(rt.state.term.pages.contains(&(0, name.clone())) && rt.find(&drawer).is_none(), "Full size: the terminal is the page");
        press(&mut rt, &format!("drawer-terminal-{name}"));
        assert!(!rt.state.term.pages.contains(&(0, name.clone())), "its lit toggle takes the page back to the chat");
    }
}
