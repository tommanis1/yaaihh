//! File paths in an agent's chat as links (owner 7 Nov night: "make that clickable in the chat view so I can have it
//! open in VS Code directly"). Before an entry is drawn as Markdown, each path it names (in inline code, or a bare
//! word starting with `/` or `~/`, or a word with a `/` that ends in a file name with an extension), with an optional
//! `:line` or `:line:col`, becomes a link to `file:PATH`. A click resolves PATH against the agent's folder (its
//! session's `cwd`) and opens it in VS Code at the line (`code -g`); other links open with the desktop's handler.

use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// The scheme of a path link: what follows is the path as the agent wrote it.
const SCHEME: &str = "file:";

/// A path, with `:line[:col]`: absolute, under `~`, or relative with a `/` and a file name with an extension.
static PATH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:(?:/|~/)[\w.@+\-/]*[\w\-]|[\w.@+\-]+(?:/[\w.@+\-]+)*/[\w@+\-]+\.[A-Za-z0-9]{1,8}|[\w@+\-]+\.(?:rs|py|sh|md|toml|json|jsonl|yaml|yml|hs|agda|lagda|p4|go|ts|tsx|js|svg|html|css|txt|cabal|nix|lean))(?::\d+(?::\d+)?)?$").unwrap()
});

/// TEXT with the paths it names as Markdown links (fenced code blocks and existing links left as they are).
/// On Windows a link the agent wrote that the window cannot open there is drawn as its label alone (`plain_dead`).
pub fn linkify(text: &str) -> String {
    let away = crate::elsewhere::away();
    let mut out = String::with_capacity(text.len() + 64);
    let mut fence = false;
    for line in text.split_inclusive('\n') {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            fence = !fence;
        }
        if fence || line.trim_start().starts_with("```") || line.starts_with("    ") {
            out.push_str(line);
        } else if away {
            line_links(&plain_dead(line, opens_away), &mut out);
        } else {
            line_links(line, &mut out);
        }
    }
    out
}

/// What coordd said of a file a message names (`Request::Locate`), by (project id, the path as written without its
/// `:line`, the writer's folder): its full path on coordd's machine, not there, or not asked back yet (yaaihh/fresh
/// N-1729 p10, the owner's walk item 9: "it's blue, looks like a link but I cannot click it").
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Known {
    Asking,
    Found(String),
    Missing,
}

type KnownKey = (String, String, String);

/// Each path asked of coordd, with what it said ([`Known`]).
static KNOWN: LazyLock<std::sync::Mutex<std::collections::HashMap<KnownKey, Known>>> = LazyLock::new(Default::default);
/// Counts what coordd said of paths ([`KNOWN`] taking an answer): a message drawn from them is drawn again when it moves
/// (post.rs `as_drawn`).
static KNOWN_REV: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The count of answers [`KNOWN`] took.
pub(crate) fn known_rev() -> u64 {
    KNOWN_REV.load(std::sync::atomic::Ordering::Relaxed)
}

/// The paths a drawn message named that are not asked yet, for the window's loop ([`ask_found`]).
static TO_ASK: std::sync::Mutex<Vec<KnownKey>> = std::sync::Mutex::new(Vec::new());

fn known_key(pid: &str, link: &str, cwd: Option<&str>) -> KnownKey {
    let path = link.strip_prefix(SCHEME).unwrap_or(link);
    (pid.to_string(), split_line(path).0.to_string(), cwd.unwrap_or("").to_string())
}

/// The full path of the file PATH names in a message of project PID written in CWD, once coordd has found it; else
/// none, and it is asked once (the window's loop, [`ask_found`]).
pub(crate) fn found(pid: &str, path: &str, cwd: Option<&str>) -> Option<String> {
    let key = known_key(pid, path, cwd);
    let mut known = KNOWN.lock().unwrap();
    match known.get(&key) {
        Some(Known::Found(full)) => Some(full.clone()),
        Some(_) => None,
        None => {
            known.insert(key.clone(), Known::Asking);
            TO_ASK.lock().unwrap().push(key);
            if let Some(w) = crate::viewer::WAKE.get() {
                w();
            }
            None
        }
    }
}

/// For a test or a still: what coordd said of PATH in project PID written in CWD.
#[cfg(test)]
pub(crate) fn set_known(pid: &str, path: &str, cwd: Option<&str>, k: Known) {
    KNOWN.lock().unwrap().insert(known_key(pid, path, cwd), k);
    KNOWN_REV.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// The window's loop: each path a message named asked of its project's coordd, where the writer works and in the
/// project's repositories (Files' roots: on Windows coordd's). A project with no pipe leaves it not found.
pub fn ask_found(s: &mut crate::App) {
    let asks = std::mem::take(&mut *TO_ASK.lock().unwrap());
    for key in asks {
        let (pid, path, cwd) = key.clone();
        let Some(pi) = s.projects.iter().position(|p| p.id == pid) else { continue };
        let roots: Vec<String> = if crate::elsewhere::away() { s.files.roots.get(&pid).cloned().unwrap_or_default() } else { crate::files::roots(&s.projects[pi]) }
            .into_iter()
            .map(|r| r.path.to_string_lossy().into_owned())
            .filter(|r| r.starts_with('/'))
            .collect();
        let search = roots.into_iter().chain(SEARCH.get().into_iter().flatten().filter(|d| d.starts_with('/')).cloned()).collect();
        let req = coord_types::Request::Locate { path, cwd: (!cwd.is_empty()).then_some(cwd), search };
        match s.projects[pi].link.as_mut().and_then(|l| l.ask(&req)) {
            Some(id) => {
                s.projects[pi].pending.insert(id, crate::Pending::Found { key });
            }
            None => {
                KNOWN.lock().unwrap().insert(key, Known::Missing);
            }
        }
    }
}

/// coordd's answer on a path a message named: found at its full path, or not there.
pub(crate) fn found_came(key: KnownKey, ok: bool, out: &str) {
    #[derive(serde::Deserialize)]
    struct At {
        path: String,
    }
    let k = match serde_json::from_str::<At>(out) {
        Ok(a) if ok => Known::Found(a.path),
        _ => Known::Missing,
    };
    KNOWN.lock().unwrap().insert(key, k);
    KNOWN_REV.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// TEXT as [`linkify`] makes it for a message of project PID written in CWD: only the paths coordd found are links;
/// one not found, or not asked back yet, stays as written, so nothing looks like a link that does nothing.
pub fn linkify_found(text: &str, pid: &str, cwd: Option<&str>) -> String {
    static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]\n]*)\]\((file:[^)\n]*)\)").unwrap());
    let linked = linkify(text);
    LINK.replace_all(&linked, |c: &regex::Captures| if found(pid, &c[2], cwd).is_some() { c[0].to_string() } else { c[1].to_string() }).into_owned()
}

/// A click on path LINK in a message of project PID, written in CWD: the file coordd found, on the Files page. False
/// when it is no path coordd found, or PID is not the project focused.
pub fn open_found(s: &mut crate::App, pid: &str, link: &str, cwd: Option<&str>) -> bool {
    if !link.starts_with(SCHEME) {
        return false;
    }
    // the Files page is the focused project's: a message of another project's keeps its old click
    if s.proj().is_none_or(|p| p.id != pid) {
        return false;
    }
    let Some(full) = found(pid, link, cwd) else { return false };
    crate::files::open_at(s, &full);
    true
}

/// A Markdown link LINE's writer made itself (`[label](target)`, not an image, not in inline code) whose target OPENS
/// refuses, as its label alone: plain text, not a link that does nothing when clicked (paper-research N-763.p1).
fn plain_dead(line: &str, opens: impl Fn(&str) -> bool) -> String {
    static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]\n]*)\]\(([^)\n]*)\)").unwrap());
    let mut out = String::with_capacity(line.len());
    let mut at = 0;
    for c in LINK.captures_iter(line) {
        let whole = c.get(0).unwrap();
        let before = &line[..whole.start()];
        // an image, or inside inline code (an odd number of backticks before it): left as written
        if before.ends_with('!') || before.matches('`').count() % 2 == 1 {
            continue;
        }
        let target = c[2].split_whitespace().next().unwrap_or("").trim_start_matches('<').trim_end_matches('>');
        if opens(target) {
            continue;
        }
        out.push_str(&line[at..whole.start()]);
        out.push_str(&c[1]);
        at = whole.end();
    }
    out.push_str(&line[at..]);
    out
}

/// Whether a link to TARGET does something in the Windows window: a web link, a path (found through coordd), a
/// mention, or an id or agent written as the target (opened in the window, `crate::mentions::open`).
fn opens_away(target: &str) -> bool {
    crate::views::web_link(target) || target.starts_with(SCHEME) || written_path(target).is_some() || crate::mentions::named(target)
}

/// The path a link the agent wrote names, when it is one (`/abs`, `~/x`, `src/a.rs:12`, `file:///abs`): a click opens
/// it as a path link (`open`). None for a link that is no path.
fn written_path(link: &str) -> Option<&str> {
    if let Some(p) = link.strip_prefix("file://") {
        return p.starts_with('/').then_some(p);
    }
    // `//host/x` is a host, not a path on the WSL machine
    (!link.starts_with("//") && (is_path(link) || (link.len() > 1 && link.starts_with('/')))).then_some(link)
}

/// The paths TEXT names, as `linkify` finds them, by byte range: CODE's spans (inline code, its ticks already gone)
/// that are paths, and bare path words outside them, each with the path as written (for a line drawn with chips).
pub fn ranges(text: &str, code: &[std::ops::Range<usize>]) -> Vec<(std::ops::Range<usize>, String)> {
    let mut out: Vec<(std::ops::Range<usize>, String)> = code.iter().filter_map(|r| text.get(r.clone()).filter(|b| is_path(b)).map(|b| (r.clone(), b.to_string()))).collect();
    let mut at = 0;
    for w in text.split_inclusive(char::is_whitespace) {
        let (s, word) = (at, w.trim_end());
        at += w.len();
        let core = word.trim_end_matches(['.', ',', ';', ')', '!', '?', '"', '\'']).trim_start_matches(['(', '"', '\'']);
        let from = s + word.find(core).unwrap_or(0);
        let r = from..from + core.len();
        if is_path(core) && core.contains('/') && !code.iter().any(|c| c.start < r.end && r.start < c.end) {
            out.push((r, core.to_string()));
        }
    }
    out.sort_by_key(|(r, _)| r.start);
    out
}

fn is_path(s: &str) -> bool {
    !s.contains("://") && s.len() > 2 && PATH.is_match(s)
}

/// One line: inline code spans that are paths, and bare path words, as links.
fn line_links(line: &str, out: &mut String) {
    let mut rest = line;
    while !rest.is_empty() {
        // the next inline code span; the text before it gets its bare paths linked
        let (plain, after) = match rest.find('`') {
            Some(i) => (&rest[..i], Some(&rest[i..])),
            None => (rest, None),
        };
        bare_links(plain, out);
        let Some(code) = after else { break };
        let ticks = code.len() - code.trim_start_matches('`').len();
        let body_from = ticks;
        match code[body_from..].find(&code[..ticks]) {
            Some(j) => {
                let body = &code[body_from..body_from + j];
                let span = &code[..body_from + j + ticks];
                // already a link's text (`[`x`](…)`): left alone
                if ticks == 1 && is_path(body) && !out.ends_with('[') {
                    out.push_str(&format!("[{span}]({SCHEME}{body})"));
                } else {
                    out.push_str(span);
                }
                rest = &code[body_from + j + ticks..];
            }
            None => {
                out.push_str(code);
                break;
            }
        }
    }
}

/// Bare words of TEXT that are paths, as links; a word's trailing punctuation stays outside its link.
fn bare_links(text: &str, out: &mut String) {
    // a word inside Markdown's own link (`[x](/a/b)`) starts with `[` and is no path, so it stays as written
    let mut last = 0;
    for (i, w) in text.split_inclusive(char::is_whitespace).scan(0, |at, w| {
        let s = *at;
        *at += w.len();
        Some((s, w))
    }) {
        let word = w.trim_end();
        let core = word.trim_end_matches(['.', ',', ';', ')', '!', '?', '"', '\'']).trim_start_matches(['(', '"', '\'']);
        if is_path(core) && (core.starts_with('/') || core.starts_with("~/") || core.contains('/')) {
            let at = i + word.find(core).unwrap_or(0);
            out.push_str(&text[last..at]);
            out.push_str(&format!("[{core}]({SCHEME}{core})"));
            last = at + core.len();
        }
    }
    out.push_str(&text[last..]);
}

/// Open LINK from a chat: a path link in the code view, inside the window (resolved against CWD by coordd); a picture
/// in the window's viewer; a web link, and only that, in his browser.
pub fn open(link: &str, cwd: Option<String>) {
    // `file:///abs`, as an agent may write it: the path alone
    if let Some(p) = link.strip_prefix("file://").filter(|p| p.starts_with('/')) {
        return open(&format!("{SCHEME}{p}"), cwd);
    }
    if !link.starts_with(SCHEME) {
        // a path the agent linked itself (`[x](src/a.rs:12)`) opens as the window's own path links do (N-763.p1)
        if let Some(p) = written_path(link) {
            return open(&format!("{SCHEME}{p}"), cwd);
        }
        return crate::views::open(link.to_string());
    }
    let link = link.to_string();
    // a picture on this machine opens in the app's viewer (owner N-313), found off the frame
    let (path, _) = split_line(link.strip_prefix(SCHEME).unwrap_or(&link));
    if !crate::elsewhere::away() && crate::viewer::is_image(path) {
        let (path, cwd) = (path.to_string(), cwd.clone());
        std::thread::spawn(move || {
            if let Some(file) = resolve(&path, cwd.as_deref()) {
                crate::viewer::open_path(file);
            }
        });
        return;
    }
    // any other file: coordd finds it, from the window's loop (`ask_located`), and the code view opens it
    LOCATE.lock().unwrap().push((link, cwd));
    if let Some(w) = crate::viewer::WAKE.get() {
        w();
    }
}

/// The path links clicked, with their folders, for coordd to find (`ask_located`).
static LOCATE: std::sync::Mutex<Vec<(String, Option<String>)>> = std::sync::Mutex::new(Vec::new());

/// The window's loop: each path clicked asked of the current project's coordd (`Request::Locate`, docket-core6's p128),
/// with the config's absolute `search` folders (coordd refuses others). A coordd older than the file requests gives
/// the one line.
pub fn ask_located(s: &mut crate::App) {
    let links = std::mem::take(&mut *LOCATE.lock().unwrap());
    for (link, cwd) in links {
        let (path, at) = split_line(link.strip_prefix(SCHEME).unwrap_or(&link));
        let Some(p) = s.projects.get_mut(s.cur).filter(|p| crate::files::served(p)) else {
            s.notice = Some(format!("{path}: {}", crate::elsewhere::LINE));
            continue;
        };
        let search = SEARCH.get().into_iter().flatten().filter(|d| d.starts_with('/')).cloned().collect();
        let req = coord_types::Request::Locate { path: path.to_string(), cwd, search };
        match p.link.as_mut().and_then(|l| l.ask(&req)) {
            Some(id) => {
                p.pending.insert(id, crate::Pending::Located { at: at.to_string(), image: crate::viewer::is_image(path) });
            }
            None => s.notice = Some(format!("{path}: disconnected")),
        }
    }
}

/// A headless scene's click on PATH (the `ticket-picture` scene): coordd asked at once, as `ask_located` asks it.
pub fn locate_once(s: &mut crate::App, path: &str) {
    let Some(p) = s.projects.get(s.cur) else { return };
    let r = p.ask_once(&coord_types::Request::Locate { path: path.to_string(), cwd: None, search: Vec::new() });
    let (ok, out) = match r {
        Ok(o) => (true, o),
        Err(e) => (false, e),
    };
    located(s, String::new(), crate::viewer::is_image(path), ok, out);
}

/// coordd found the path: a picture it serves opens in the viewer, a PDF in Files; any other file in the code view, at
/// AT (`:line[:col]`). When neither can be, one line says so.
pub(crate) fn located(s: &mut crate::App, at: String, image: bool, ok: bool, out: String) {
    #[derive(serde::Deserialize)]
    struct Found {
        path: String,
        #[serde(default)]
        readable: bool,
    }
    let found = match serde_json::from_str::<Found>(&out) {
        Ok(f) if ok => f,
        _ => return s.notice = Some(out),
    };
    if image && found.readable {
        return crate::viewer::open_path(PathBuf::from(found.path));
    }
    // a PDF in Files' own view; anything else in the code view, in a pane of the window, at its line; nothing opens
    // it outside the window
    if crate::files::is_pdf(std::path::Path::new(&found.path)) {
        return crate::files::open_at(s, &found.path);
    }
    if !crate::code::served(s, s.cur) {
        return s.notice = Some(format!("{}: {CODE_VIEW_NEEDED}", coord_types::short_path(&found.path)));
    }
    let line = at.trim_start_matches(':').split(':').next().and_then(|l| l.parse().ok());
    crate::code::open(s, s.cur, coord_types::codeview::CodeTarget::File { path: found.path, line });
}

/// Why a file is not opened: this project's coordd has no code view, and nothing outside the window opens it.
pub const CODE_VIEW_NEEDED: &str = "not opened: needs a newer engine";

/// A link's path without its scheme and its `:line[:col]` (thumbs.rs: whether it names a picture).
pub(crate) fn without_line(link: &str) -> &str {
    split_line(link.strip_prefix(SCHEME).unwrap_or(link)).0
}

/// PATH and its `:line[:col]` suffix (empty when it has none).
fn split_line(s: &str) -> (&str, &str) {
    let mut cut = s.len();
    for _ in 0..2 {
        match s[..cut].rfind(':') {
            Some(i) if !s[i + 1..cut].is_empty() && s[i + 1..cut].bytes().all(|b| b.is_ascii_digit()) => cut = i,
            _ => break,
        }
    }
    (&s[..cut], &s[cut..])
}

/// The folders a relative path a chat names is looked for in, after its own folder (the config's `search`).
pub static SEARCH: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

/// The file PATH names: itself when absolute (`~` expanded); else under CWD or one of its parents; else the first
/// file under the `search` folders whose path ends in it.
pub(crate) fn resolve(path: &str, cwd: Option<&str>) -> Option<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let path = match path.strip_prefix("~/") {
        Some(r) => format!("{home}/{r}"),
        None => path.to_string(),
    };
    if Path::new(&path).is_absolute() {
        return Path::new(&path).exists().then(|| PathBuf::from(&path));
    }
    if let Some(found) = cwd.and_then(|c| Path::new(c).ancestors().map(|d| d.join(&path)).find(|f| f.exists())) {
        return Some(found);
    }
    let pattern = format!("*/{}", path.trim_start_matches("./"));
    // build and VCS folders are not searched
    let skip = ["(", "-name", ".git", "-o", "-name", "target", "-o", "-name", "_build", "-o", "-name", "dist-newstyle", "-o", "-name", "node_modules", ")", "-prune", "-o"];
    let dirs: Vec<String> = SEARCH.get()?.iter().map(|d| d.strip_prefix("~/").map_or_else(|| d.clone(), |r| format!("{home}/{r}"))).collect();
    if dirs.is_empty() {
        return None;
    }
    let mut cmd = crate::places::command("find");
    cmd.args(&dirs).args(skip);
    let out = cmd.args(["-path", &pattern, "-print", "-quit"]).stderr(std::process::Stdio::null()).output().ok()?;
    let first = String::from_utf8_lossy(&out.stdout).lines().next()?.to_string();
    Some(PathBuf::from(first))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The owner's walk, item 9 (yaaihh/fresh N-1729 p10): in a message, a file it names is a link only once coordd has
    /// found it, and a click opens it on the Files page; one coordd did not find, or has not answered on yet, is drawn
    /// as written, in its code mark, never as a link; a project with no pipe to ask leaves it not found.
    #[test]
    fn a_file_a_message_names_is_a_link_only_once_found_and_opens_on_files() {
        let (_held, mut s) = crate::home::tests::made();
        let cwd = Some("/found-test/agent");
        set_known("walk", "yaaihh/docs/why.md", cwd, Known::Found("/srv/yaaihh/docs/why.md".into()));
        set_known("walk", "docs/gone.md", cwd, Known::Missing);
        let text = "Read `yaaihh/docs/why.md:3` and `docs/gone.md`, then `docs/new.md`.";
        assert_eq!(linkify_found(text, "walk", cwd), "Read [`yaaihh/docs/why.md:3`](file:yaaihh/docs/why.md:3) and `docs/gone.md`, then `docs/new.md`.");
        // the one not asked yet is asked once; with no pipe it is not found, and stays as written
        assert!(TO_ASK.lock().unwrap().iter().any(|k| k.1 == "docs/new.md" && k.2 == "/found-test/agent"));
        s.projects[0].link = None;
        ask_found(&mut s);
        assert_eq!(KNOWN.lock().unwrap().get(&known_key("walk", "docs/new.md", cwd)), Some(&Known::Missing));
        // coordd's answer, as `Locate` gives it
        found_came(known_key("walk", "docs/later.md", cwd), true, r#"{"path": "/srv/yaaihh/docs/later.md", "readable": true}"#);
        assert_eq!(found("walk", "docs/later.md", cwd).as_deref(), Some("/srv/yaaihh/docs/later.md"));
        found_came(known_key("walk", "docs/no.md", cwd), false, "docs/no.md: no such file here");
        assert_eq!(found("walk", "docs/no.md", cwd), None);
        // a click: the found one on the Files page, at its full path; the other not opened there
        assert!(!open_found(&mut s, "walk", "file:docs/gone.md", cwd));
        assert!(!open_found(&mut s, "elsewhere", "file:yaaihh/docs/why.md:3", cwd), "another project's message");
        assert!(open_found(&mut s, "walk", "file:yaaihh/docs/why.md:3", cwd));
        assert_eq!((s.sel.clone(), s.files.kept["walk"].shown.as_deref()), (crate::Sel::Files, Some("/srv/yaaihh/docs/why.md")));
    }

    #[test]
    fn a_link_the_agent_wrote_that_cannot_open_on_windows_is_its_label() {
        let opens = |t: &str| opens_away(t);
        assert_eq!(plain_dead("see [the run](vscode://x/y) and [docs](https://a.org/b).\n", opens), "see the run and [docs](https://a.org/b).\n");
        assert_eq!(plain_dead("[a](src/a.rs:12) [b](/home/t/x) [c](file:///home/t/y) [d](N-12.p3) [e](@docket-core) [f](#top) [g](C-1.s2)", opens), "[a](src/a.rs:12) [b](/home/t/x) [c](file:///home/t/y) [d](N-12.p3) [e](@docket-core) f g");
        // an image and a link inside inline code stay as written
        assert_eq!(plain_dead("![pic](x:y) `[k](x:y)` [z](x:y)", opens), "![pic](x:y) `[k](x:y)` z");
    }

    #[test]
    fn a_path_the_agent_linked_opens_as_a_path_link() {
        assert_eq!(written_path("src/a.rs:12"), Some("src/a.rs:12"));
        assert_eq!(written_path("file:///home/t/y"), Some("/home/t/y"));
        assert_eq!(written_path("~/notes/a.md"), Some("~/notes/a.md"));
        assert_eq!(written_path("/home/t/dir"), Some("/home/t/dir"));
        assert_eq!(written_path("https://a.org/b"), None);
        assert_eq!(written_path("N-12"), None);
        assert_eq!(written_path("//host/x"), None);
    }
}
