//! Each repository as git says it now: its branch, its last commit, how many files differ from it, how far it is from
//! its upstream, and the PDF at its top, if any.

use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Git's output for ARGS in DIR, or None when it fails; with no optional lock, so a status never rewrites the index.
fn git(dir: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("--no-optional-locks").arg("-C").arg(dir).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Repository NAME at DIR: `{name, branch, head, subject, at, dirty, ahead, behind, pdf}`, what is not there left out.
pub fn one(name: &str, dir: &str) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), json!(name));
    m.insert("path".into(), json!(dir));
    if let Some(status) = git(dir, &["status", "--porcelain=v2", "--branch"]) {
        let mut dirty = 0u64;
        for l in status.lines() {
            match l.strip_prefix("# ") {
                Some(h) => {
                    if let Some(b) = h.strip_prefix("branch.head ").filter(|b| *b != "(detached)") {
                        m.insert("branch".into(), json!(b));
                    }
                    if let Some(ab) = h.strip_prefix("branch.ab ") {
                        let mut it = ab.split(' ').map(|n| n.trim_start_matches(['+', '-']).parse::<u64>().unwrap_or(0));
                        m.insert("ahead".into(), json!(it.next().unwrap_or(0)));
                        m.insert("behind".into(), json!(it.next().unwrap_or(0)));
                    }
                }
                None if !l.is_empty() => dirty += 1,
                None => {}
            }
        }
        m.insert("dirty".into(), json!(dirty));
    }
    if let Some(log) = git(dir, &["log", "-1", "--format=%h%x1f%s%x1f%cI"]) {
        let mut it = log.trim_end().splitn(3, '\u{1f}');
        for k in ["head", "subject", "at"] {
            if let Some(v) = it.next().filter(|v| !v.is_empty()) {
                m.insert(k.into(), json!(v));
            }
        }
    }
    if let Some(pdf) = top_pdf(dir) {
        m.insert("pdf".into(), json!(pdf));
    }
    Value::Object(m)
}

/// The folders a repository's built PDF is looked for in, its top last.
const BUILT: [&str; 4] = [".build", "build", "out", ""];

/// The repository's PDF, as a path under DIR: in `.build/`, `build/`, `out/` or the top, nothing deeper; `main.pdf`
/// or `paper.pdf` first, else the newest. Only plain files in plain folders: a link is never followed.
fn top_pdf(dir: &str) -> Option<String> {
    let root = Path::new(dir);
    let mut found: Vec<(bool, std::time::SystemTime, String)> = Vec::new();
    for sub in BUILT {
        let at = root.join(sub);
        if !sub.is_empty() && !std::fs::symlink_metadata(&at).is_ok_and(|m| m.is_dir()) {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&at) else { continue };
        for e in rd.flatten() {
            let (Ok(t), Ok(n)) = (e.file_type(), e.file_name().into_string()) else { continue };
            if !t.is_file() || !n.to_lowercase().ends_with(".pdf") || n.contains(['/', '\\']) {
                continue;
            }
            let named = matches!(n.to_lowercase().as_str(), "main.pdf" | "paper.pdf");
            let at = e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
            found.push((named, at, if sub.is_empty() { n } else { format!("{sub}/{n}") }));
        }
    }
    // named first, then newest, then by path
    found.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
    found.into_iter().next().map(|(_, _, p)| p)
}

/// Every repository of REPOS (name, folder), in their order, for the owner's HOME; a place never read is refused, its
/// links followed, and git never runs there.
pub fn all(repos: &[(String, String)], home: &Path) -> Value {
    let (canon, home) = (|p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()), PathBuf::from(home));
    let home = canon(&home);
    let each = |(n, d): &(String, String)| match crate::kept::never_served(&home, &canon(Path::new(d))) {
        Some(why) => json!({"name": n, "refused": why}),
        None => one(n, d),
    };
    Value::Array(repos.iter().map(each).collect())
}

#[cfg(test)]
#[path = "repos_tests.rs"]
mod pdf_tests;

#[cfg(test)]
mod tests {
    use super::*;

    /// A repository's branch, last commit, its uncommitted and untracked files counted, the PDF at its top; a folder git
    /// does not know gives its name alone.
    #[test]
    fn a_repositorys_branch_last_commit_and_uncommitted_files() {
        let d = std::env::temp_dir().join(format!("machine-state-repos-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("r")).unwrap();
        std::fs::create_dir_all(d.join("plain")).unwrap();
        let r = d.join("r").display().to_string();
        let g = |args: &[&str]| assert!(Command::new("git").arg("-C").arg(&r).args(["-c", "user.name=t", "-c", "user.email=t@x"]).args(args).output().unwrap().status.success(), "{args:?}");
        g(&["init", "-q", "-b", "main"]);
        std::fs::write(d.join("r/a.txt"), "a").unwrap();
        g(&["add", "a.txt"]);
        g(&["commit", "-qm", "the first commit"]);
        std::fs::write(d.join("r/a.txt"), "b").unwrap();
        std::fs::write(d.join("r/new.txt"), "n").unwrap();
        std::fs::write(d.join("r/main.pdf"), "%PDF").unwrap();
        let v = one("r", &r);
        assert_eq!((v["branch"].as_str(), v["subject"].as_str(), v["dirty"].as_u64()), (Some("main"), Some("the first commit"), Some(3)), "{v}");
        assert_eq!(v["pdf"], "main.pdf");
        assert!(v["head"].as_str().is_some_and(|h| h.len() >= 7) && v["at"].as_str().is_some());
        assert!(v.get("ahead").is_none(), "no upstream");
        let plain = d.join("plain").display().to_string();
        assert_eq!(one("plain", &plain), json!({"name": "plain", "path": plain}));
        assert_eq!(v["path"], r);
        std::fs::create_dir_all(d.join("home/.ssh")).unwrap();
        std::os::unix::fs::symlink(d.join("home/.ssh"), d.join("linked")).unwrap();
        let asked = [("s".to_string(), d.join("linked").display().to_string()), ("plain".to_string(), plain.clone())];
        assert_eq!(all(&asked, &d.join("home")), json!([{"name": "s", "refused": "the owner's logins and settings"}, {"name": "plain", "path": plain}]), "through a link too");
        let _ = std::fs::remove_dir_all(&d);
    }
}
