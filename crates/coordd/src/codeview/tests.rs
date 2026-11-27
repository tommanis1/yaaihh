use super::*;
use coord_types::codeview::{CodeLook, CodeLooks, CodeTarget};
use std::path::Path;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

/// A scratch HOME with a stand-in install (its `node` a server that listens and starts one child in its group), a
/// coord, agent-base's codeview files, a repository with two commits and a folder of notes, the project's roots.
fn scratch(tag: &str) -> (PathBuf, Ctx, String) {
    let d = std::env::temp_dir().join(format!("coordd-codeview-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let root = d.join("home/agent-coord/tools/openvscode-server/current");
    let repo = d.join("repo");
    for f in [&root.join("bin"), &root.join("out"), &root.join("extensions/git"), &root.join("extensions/rust"), &root.join("extensions/typescript-language-features"), &d.join("coord"), &repo.join("src")] {
        std::fs::create_dir_all(f).unwrap();
    }
    std::fs::write(root.join("bin/openvscode-server"), "").unwrap();
    std::fs::write(root.join("out/server-main.js"), "").unwrap();
    let node = root.join("node");
    std::fs::write(&node, format!("#!/usr/bin/env python3\nimport socket, subprocess, sys, os\nopen('{}', 'a').write(' '.join(sys.argv[1:]) + ' DIR=' + os.environ.get('YAAIHH_CODEVIEW_DIR', '') + '\\n')\nsubprocess.Popen(['sleep', '600'])\ns = socket.socket(); s.bind(('127.0.0.1', int(sys.argv[sys.argv.index('--port') + 1]))); s.listen()\nheld = []\nwhile True: held.append(s.accept()[0])\n", d.join("args").display())).unwrap();
    std::fs::set_permissions(&node, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    git(&repo, &["init", "-q"]);
    std::fs::write(repo.join("src/core.rs"), "fn a() {}\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "first"]);
    std::fs::write(repo.join("src/core.rs"), "fn a() {}\nfn b() {}\n").unwrap();
    git(&repo, &["commit", "-qam", "second"]);
    let sha = git(&repo, &["rev-parse", "HEAD"]);
    let agent_base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../agent-base");
    std::fs::create_dir_all(d.join("notes")).unwrap();
    std::fs::write(d.join("notes/plan.md"), "# plan\n").unwrap();
    let roots = vec![("yaaihh".into(), repo.display().to_string()), ("notes".into(), d.join("notes").display().to_string()), ("gone".into(), d.join("gone").display().to_string())];
    let ctx = Ctx { coord: d.join("coord"), home: d.join("home").display().to_string(), agent_base, roots };
    (d, ctx, sha)
}

fn home(ctx: &Ctx) -> &Path {
    Path::new(&ctx.home)
}

fn state(ctx: &Ctx) -> Option<server::Server> {
    std::fs::read_to_string(ctx.dir().join("server.json")).ok().and_then(|t| serde_json::from_str(&t).ok())
}

fn stop(ctx: &Ctx) {
    if let Some(s) = state(ctx) {
        server::stop(&s);
    }
}

/// The window's two looks, made up.
fn looks() -> CodeLooks {
    let look = |b: &str| CodeLook { bg: format!("#{b}0000ff"), layer: "#111111".into(), raised: "#222222".into(), text: "#eeeeee".into(), muted: "#aaaaaa".into(), faint: "#777777".into(), border: "#333333".into(), accent: "#3366ff".into(), selection: "#334455".into() };
    CodeLooks { dark: look("10"), light: look("f0") }
}

#[test]
fn targets_resolve_inside_the_projects_roots_only() {
    let (d, ctx, sha) = scratch("targets");
    let root = std::fs::canonicalize(d.join("repo")).unwrap().display().to_string();
    let file = target::resolve(&ctx.roots, home(&ctx), &CodeTarget::File { path: "src/core.rs".into(), line: Some(2) }).unwrap().unwrap();
    assert_eq!(file.folder, root);
    assert_eq!(file.action, serde_json::json!({"is": "file", "path": format!("{root}/src/core.rs"), "line": 2}));
    let abs = target::resolve(&ctx.roots, home(&ctx), &CodeTarget::File { path: format!("{root}/src/core.rs"), line: None }).unwrap().unwrap();
    assert_eq!(abs.action["line"], 1);
    let c = target::resolve(&ctx.roots, home(&ctx), &CodeTarget::Commit { repo: "yaaihh".into(), sha: sha[..9].into() }).unwrap().unwrap();
    assert_eq!(c.action, serde_json::json!({"is": "commit", "sha": sha}), "the full id");
    let r = target::resolve(&ctx.roots, home(&ctx), &CodeTarget::Repo { path: format!("{root}/src") }).unwrap().unwrap();
    assert_eq!(target::resolve(&ctx.roots, home(&ctx), &CodeTarget::Workspace).unwrap(), None, "the workspace alone opens nothing");
    let notes = std::fs::canonicalize(d.join("notes")).unwrap().display().to_string();
    let n = target::resolve(&ctx.roots, home(&ctx), &CodeTarget::File { path: "plan.md".into(), line: None }).unwrap().unwrap();
    assert_eq!(n.folder, notes, "a file of a root that is no repository");
    assert_eq!((r.folder.as_str(), r.action["is"].as_str()), (root.as_str(), Some("repo")));
    std::fs::write(d.join("outside.rs"), "").unwrap();
    for (t, why) in [
        (CodeTarget::File { path: d.join("outside.rs").display().to_string(), line: None }, "in no folder of this project's Files"),
        (CodeTarget::File { path: ".git/config".into(), line: None }, "never shown"),
        (CodeTarget::File { path: "src/none.rs".into(), line: None }, "not there"),
        (CodeTarget::File { path: "src".into(), line: None }, "not a file"),
        (CodeTarget::Commit { repo: "yaaihh".into(), sha: "zz12".into() }, "not a commit id"),
        (CodeTarget::Commit { repo: "yaaihh".into(), sha: "abcdef12".into() }, "no commit of"),
        (CodeTarget::Repo { path: "other".into() }, "no folder of this project's Files"),
        (CodeTarget::Commit { repo: "notes".into(), sha: "abcdef12".into() }, "no commit of"),
    ] {
        let e = target::resolve(&ctx.roots, home(&ctx), &t).unwrap_err();
        assert!(e.contains(why), "{t:?}: {e}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_server_starts_once_on_loopback_and_each_open_is_queued_for_its_workspace() {
    let (d, ctx, sha) = scratch("open");
    let his = d.join("his");
    std::fs::create_dir_all(&his).unwrap();
    std::fs::write(his.join("settings.json"), "{\n  // mine\n  \"editor.fontSize\": 15,\n  \"workbench.colorTheme\": \"Monokai\",\n  \"extensions.autoUpdate\": true,\n}\n").unwrap();
    std::fs::write(his.join("keybindings.json"), "[{\"key\": \"ctrl+p\", \"command\": \"x\"}, // mine\n{\"key\": \"ctrl+q\", \"command\": \"-y\"},]").unwrap();
    std::fs::create_dir_all(ctx.dir()).unwrap();
    std::fs::write(ctx.dir().join("inherit"), his.display().to_string()).unwrap();

    let a = open(&ctx, &CodeTarget::File { path: "src/core.rs".into(), line: Some(2) }, Some("light"), Some(&looks())).unwrap();
    let s = state(&ctx).unwrap();
    let w = ctx.dir().join("repos.code-workspace");
    assert_eq!((a.origin.clone(), a.folder.clone()), (format!("http://127.0.0.1:{}", s.port), w.display().to_string()));
    assert_eq!(a.url, format!("http://127.0.0.1:{}/?workspace={}&tkn={}", s.port, a.folder, s.token));
    assert_eq!(s.token.len(), 32);
    let root = std::fs::canonicalize(d.join("repo")).unwrap().display().to_string();
    let ws: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&w).unwrap()).unwrap();
    let notes = std::fs::canonicalize(d.join("notes")).unwrap().display().to_string();
    assert_eq!(ws, serde_json::json!({"folders": [{"name": "yaaihh", "path": root}, {"name": "notes", "path": notes}]}), "every root that is a folder here");
    // started from the install's links, its heavy built-ins left out, with no trust prompt, telemetry or experiments
    let args = std::fs::read_to_string(d.join("args")).unwrap();
    let app = ctx.dir().join("app");
    let token_file = ctx.dir().join("token");
    assert!(args.starts_with(&format!("--preserve-symlinks-main --preserve-symlinks {} --host 127.0.0.1 --port {} --telemetry-level off --disable-workspace-trust --disable-telemetry --disable-experiments --connection-token-file {}", app.join("out/server-main.js").display(), s.port, token_file.display())), "{args}");
    let mut kept: Vec<String> = std::fs::read_dir(app.join("extensions")).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    kept.sort();
    assert_eq!(kept, ["git", "rust"]);
    assert!(std::fs::read_link(app.join("node")).unwrap().ends_with("current/node"));
    assert!(!args.contains(&s.token), "the token on no command line");
    assert_eq!(std::fs::read_to_string(&token_file).unwrap(), s.token);
    assert_eq!(std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&token_file).unwrap().permissions()) & 0o777, 0o600);
    assert!(args.trim_end().ends_with(&format!("DIR={}", ctx.dir().display())));
    // the same server for the next open, a commit in the same workspace
    let b = open(&ctx, &CodeTarget::Commit { repo: "yaaihh".into(), sha: sha[..7].into() }, None, None).unwrap();
    assert_eq!((b.url, b.folder), (a.url.clone(), a.folder.clone()));
    assert_eq!(std::fs::read_to_string(d.join("args")).unwrap().lines().count(), 1, "started once");
    let q: Vec<serde_json::Value> = std::fs::read_to_string(ctx.dir().join("open.jsonl")).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!((q[0]["is"].as_str(), q[0]["line"].as_u64(), q[0]["folder"].as_str()), (Some("file"), Some(2), Some(root.as_str())));
    assert_eq!((q[1]["is"].as_str(), q[1]["sha"].as_str()), (Some("commit"), Some(sha.as_str())));
    assert_ne!(q[0]["id"], q[1]["id"]);
    // the workspace alone queues nothing
    open(&ctx, &CodeTarget::Workspace, None, None).unwrap();
    assert_eq!(std::fs::read_to_string(ctx.dir().join("open.jsonl")).unwrap().lines().count(), 2);
    // coordd's own extension: every prompt kept away, his keybindings but a removal
    let own: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(ctx.dir().join("extensions/yaaihh.yaaihh-own-0.0.1/package.json")).unwrap()).unwrap();
    let defaults = &own["contributes"]["configurationDefaults"];
    for (k, v) in [
        ("security.workspace.trust.enabled", serde_json::json!(false)),
        ("workbench.startupEditor", "none".into()),
        ("workbench.tips.enabled", false.into()),
        ("workbench.welcomePage.walkthroughs.openOnInstall", false.into()),
        ("extensions.ignoreRecommendations", true.into()),
        ("update.mode", "none".into()),
        ("telemetry.telemetryLevel", "off".into()),
        ("window.commandCenter", false.into()),
        ("editor.minimap.enabled", false.into()),
        ("workbench.layoutControl.enabled", false.into()),
        ("workbench.activityBar.location", "hidden".into()),
    ] {
        assert_eq!(defaults[k], v, "{k}");
    }
    assert_eq!(own["contributes"]["keybindings"], serde_json::json!([{"key": "ctrl+p", "command": "x"}]), "a removal left out");
    let listed: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(ctx.dir().join("extensions/extensions.json")).unwrap()).unwrap();
    assert_eq!(listed.as_array().unwrap().iter().map(|e| e["identifier"]["id"].as_str().unwrap()).collect::<Vec<_>>(), ["yaaihh.yaaihh-open", "yaaihh.yaaihh-own"]);
    let link = ctx.dir().join("extensions/yaaihh.yaaihh-open-0.0.1");
    let ours: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(link.join("package.json")).unwrap()).unwrap();
    assert!(ours.get("contributes").is_none(), "no settings of its own");
    // the machine settings: the light theme in the window's colours, his editor keys only
    let set: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(workbench::settings_file(&ctx.dir())).unwrap()).unwrap();
    assert_eq!((set["workbench.colorTheme"].as_str(), set["editor.fontSize"].as_u64()), (Some("Default Light Modern"), Some(15)));
    assert!(set.get("extensions.autoUpdate").is_none() && set.get("workbench.startupEditor").is_none(), "his install keys not taken: {set}");
    let c = set["workbench.colorCustomizations"].clone();
    assert_eq!((c["[Default Dark Modern]"]["editor.background"].as_str(), c["[Default Light Modern]"]["editor.background"].as_str()), (Some("#100000ff"), Some("#f00000ff")));
    for k in ["sideBar.background", "tab.activeBackground", "editorGroup.border", "foreground", "editor.selectionBackground", "focusBorder"] {
        assert!(c["[Default Light Modern]"][k].is_string(), "{k}");
    }
    // opted out: no keybindings and no editor keys of his; a theme and colours not asked keep the last
    std::fs::remove_file(ctx.dir().join("inherit")).unwrap();
    open(&ctx, &CodeTarget::Repo { path: "yaaihh".into() }, None, None).unwrap();
    let own: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(ctx.dir().join("extensions/yaaihh.yaaihh-own-0.0.1/package.json")).unwrap()).unwrap();
    assert_eq!(own["contributes"]["keybindings"], serde_json::json!([]));
    let set: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(workbench::settings_file(&ctx.dir())).unwrap()).unwrap();
    assert_eq!(set, serde_json::json!({"workbench.colorTheme": "Default Light Modern", "workbench.colorCustomizations": c}));
    // a server that died is started again; an unknown theme and no server are refused in words
    stop(&ctx);
    let c = open(&ctx, &CodeTarget::Repo { path: "yaaihh".into() }, None, None).unwrap();
    assert_ne!(c.url, a.url, "a new port and token");
    assert!(open(&ctx, &CodeTarget::Repo { path: "yaaihh".into() }, Some("blue"), None).unwrap_err().contains("neither dark nor light"));
    stop(&ctx);
    std::fs::remove_dir_all(d.join("home")).unwrap();
    assert!(open(&ctx, &CodeTarget::Repo { path: "yaaihh".into() }, None, None).unwrap_err().contains("not installed"));
    let _ = std::fs::remove_dir_all(&d);
}

/// The window's first ask, the workspace, starts the server with nothing opened; a server started otherwise (an older
/// coordd's) is replaced, and a stopped one leaves no process of its group, a child of its own included, and no
/// zombie.
#[test]
fn the_workspace_ask_starts_the_server_and_a_stopped_one_leaves_nothing() {
    let (d, ctx, _) = scratch("warm");
    assert!(state(&ctx).is_none(), "none before an ask");
    open(&ctx, &CodeTarget::Workspace, None, None).unwrap();
    let s = state(&ctx).unwrap();
    assert!(!ctx.dir().join("open.jsonl").exists(), "nothing opened");
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while server::group(s.pid).len() < 2 && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(server::group(s.pid).len(), 2, "the server and its child");
    open(&ctx, &CodeTarget::Workspace, None, None).unwrap();
    assert_eq!(state(&ctx).unwrap(), s, "a second ask finds it");
    // one started otherwise: stopped, its child with it, and one started as now
    std::fs::write(ctx.dir().join("server.json"), serde_json::to_string(&server::Server { how: "older".into(), ..s.clone() }).unwrap()).unwrap();
    let n = open(&ctx, &CodeTarget::Workspace, None, None).unwrap();
    let t = state(&ctx).unwrap();
    assert_ne!(t.pid, s.pid);
    assert!(n.url.contains(&format!(":{}/", t.port)));
    assert_eq!(server::group(s.pid), Vec::<u32>::new(), "the old group gone");
    assert!(std::fs::read_to_string(format!("/proc/{}/stat", s.pid)).map_or(true, |st| !st.contains(") Z ")), "reaped");
    server::stop(&t);
    assert_eq!(server::group(t.pid), Vec::<u32>::new());
    // a project with no folder here starts none
    let none = Ctx { roots: vec![("gone".into(), d.join("gone").display().to_string())], ..ctx };
    std::fs::remove_file(none.dir().join("server.json")).unwrap();
    assert!(open(&none, &CodeTarget::Workspace, None, None).unwrap_err().contains("no folder here"));
    assert!(state(&none).is_none());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn settings_with_comments_and_trailing_commas_read_as_json() {
    let t = "{\n // a \"quoted\" note\n \"a\": \"x // not a comment\", /* b */ \"c\": [1, 2,],\n}";
    let v: serde_json::Value = serde_json::from_str(&workbench::jsonc(t)).unwrap();
    assert_eq!(v, serde_json::json!({"a": "x // not a comment", "c": [1, 2]}));
    assert_eq!(workbench::query("/a b/ü"), "/a%20b/%C3%BC");
}

/// By hand: each target of CODEVIEW_TARGETS (a JSON list) opened as coordd opens it, in CODEVIEW_COORD's codeview, the
/// roots of CODEVIEW_REPOS (`{"name": "/folder"}`), agent-base CODEVIEW_BASE; the answers to CODEVIEW_OUT.
#[test]
#[ignore]
fn opened_by_hand() {
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let (Some(coord), Some(targets), Some(repos), Some(base), Some(out)) = (var("CODEVIEW_COORD"), var("CODEVIEW_TARGETS"), var("CODEVIEW_REPOS"), var("CODEVIEW_BASE"), var("CODEVIEW_OUT")) else { return };
    let repos: std::collections::BTreeMap<String, String> = serde_json::from_str(&repos).unwrap();
    let ctx = super::Ctx { coord: coord.into(), home: std::env::var("HOME").unwrap_or_default(), agent_base: base.into(), roots: repos.into_iter().collect() };
    let targets: Vec<CodeTarget> = serde_json::from_str(&targets).unwrap();
    let answers: Vec<serde_json::Value> = targets.iter().map(|t| match super::open(&ctx, t, Some("dark"), None) {
        Ok(a) => serde_json::to_value(a).unwrap(),
        Err(e) => serde_json::json!({"error": e}),
    }).collect();
    std::fs::write(out, serde_json::to_string(&answers).unwrap()).unwrap();
}

/// What he edits stays highlighted, diffed and edited: the grammars of Rust, LaTeX, Python, Markdown, JSON and shell,
/// the default themes, git and its diffs are kept; TOML has no built-in grammar to keep.
#[test]
fn the_grammars_he_edits_and_git_are_kept() {
    for kept in ["rust", "latex", "python", "markdown-basics", "markdown-language-features", "json", "shellscript", "theme-defaults", "git", "git-base", "merge-conflict", "diff"] {
        assert!(!install::LEFT_OUT.contains(&kept), "{kept}");
    }
}

/// A server no window is connected to is stopped once IDLE has passed, its group and state file gone and no zombie
/// left; one a window is on is kept however long, and its wait starts again when the window goes.
#[test]
fn a_server_no_window_is_on_is_let_go_after_its_wait() {
    let (d, ctx, _) = scratch("idle");
    open(&ctx, &CodeTarget::Workspace, None, None).unwrap();
    let s = state(&ctx).unwrap();
    let t0 = std::time::Instant::now();
    let mut quiet = None;
    assert!(!server::let_go(&ctx.dir(), &mut quiet, t0), "its wait starts");
    let window = std::net::TcpStream::connect(("127.0.0.1", s.port)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(!server::let_go(&ctx.dir(), &mut quiet, t0 + server::IDLE * 2), "a window on it: kept");
    assert!(quiet.is_none());
    drop(window);
    std::thread::sleep(std::time::Duration::from_millis(100));
    let t1 = t0 + server::IDLE * 2;
    assert!(!server::let_go(&ctx.dir(), &mut quiet, t1), "its wait starts again");
    assert!(!server::let_go(&ctx.dir(), &mut quiet, t1 + server::IDLE / 2));
    assert!(server::let_go(&ctx.dir(), &mut quiet, t1 + server::IDLE), "let go");
    assert_eq!(server::group(s.pid), Vec::<u32>::new(), "nothing of its group left");
    assert!(state(&ctx).is_none());
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(std::fs::read_to_string(format!("/proc/{}/stat", s.pid)).map_or(true, |st| !st.contains(") Z ")), "reaped");
    // the next open starts it again
    open(&ctx, &CodeTarget::Workspace, None, None).unwrap();
    assert_ne!(state(&ctx).unwrap().pid, s.pid);
    stop(&ctx);
    let _ = std::fs::remove_dir_all(&d);
}

/// The window left: the server stopped as soon as no window is on it, kept while one is; coordd ending stops it
/// whoever is on it; one that dies at its start takes its whole group with it.
#[test]
fn a_server_left_or_ended_and_one_dead_at_its_start_leave_nothing() {
    let (d, ctx, _) = scratch("left");
    open(&ctx, &CodeTarget::Workspace, None, None).unwrap();
    let s = state(&ctx).unwrap();
    let window = std::net::TcpStream::connect(("127.0.0.1", s.port)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(!server::end_if_alone(&ctx.dir()), "a window on it: kept");
    drop(window);
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(server::end_if_alone(&ctx.dir()), "left alone: stopped");
    assert_eq!(server::group(s.pid), Vec::<u32>::new());
    assert!(state(&ctx).is_none());
    // coordd ending: stopped, a window on it or not
    open(&ctx, &CodeTarget::Workspace, None, None).unwrap();
    let t = state(&ctx).unwrap();
    let _window = std::net::TcpStream::connect(("127.0.0.1", t.port)).unwrap();
    end(&ctx.coord);
    assert_eq!(server::group(t.pid), Vec::<u32>::new());
    assert!(state(&ctx).is_none());
    // a server that dies at its start, its child left: the group goes
    let node = d.join("home/agent-coord/tools/openvscode-server/current/node");
    let pid = d.join("pid");
    std::fs::write(&node, format!("#!/usr/bin/env python3\nimport os, subprocess, sys\nopen('{}', 'w').write(str(os.getpid()))\nsubprocess.Popen(['sleep', '600'])\nsys.exit(3)\n", pid.display())).unwrap();
    let _ = std::fs::remove_dir_all(ctx.dir().join("app"));
    let e = open(&ctx, &CodeTarget::Workspace, None, None).unwrap_err();
    assert!(e.contains("ended at its start"), "{e}");
    let leader: u32 = std::fs::read_to_string(&pid).unwrap().trim().parse().unwrap();
    assert_eq!(server::group(leader), Vec::<u32>::new(), "its child gone with it");
    let _ = std::fs::remove_dir_all(&d);
}
