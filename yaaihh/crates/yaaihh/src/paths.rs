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
pub fn linkify(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 64);
    let mut fence = false;
    for line in text.split_inclusive('\n') {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            fence = !fence;
        }
        if fence || line.trim_start().starts_with("```") || line.starts_with("    ") {
            out.push_str(line);
        } else {
            line_links(line, &mut out);
        }
    }
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

/// Open LINK from a chat: a path link in VS Code (resolved against CWD), any other with the desktop's handler.
pub fn open(link: &str, cwd: Option<String>) {
    let Some(written) = link.strip_prefix(SCHEME) else { return crate::views::open(link.to_string()) };
    let written = written.to_string();
    // a search of the work folders can take a moment: off the UI thread
    std::thread::spawn(move || {
        let (path, at) = split_line(&written);
        if let Some(file) = resolve(path, cwd.as_deref()) {
            let target = format!("{}{at}", file.display());
            let _ = std::process::Command::new("code").arg("-g").arg(target).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
        }
    });
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
fn resolve(path: &str, cwd: Option<&str>) -> Option<PathBuf> {
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
    let mut cmd = std::process::Command::new("find");
    cmd.args(&dirs).args(skip);
    let out = cmd.args(["-path", &pattern, "-print", "-quit"]).stderr(std::process::Stdio::null()).output().ok()?;
    let first = String::from_utf8_lossy(&out.stdout).lines().next()?.to_string();
    Some(PathBuf::from(first))
}
