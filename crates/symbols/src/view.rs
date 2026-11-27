//! The view a server is given (tickets-patterns plan/views/design.md, 4.1; N-101 post 290): each repository's base
//! commit and the path the agent sees it at, then, as data, every file of it that differs from the base and every
//! path deleted, kept current before each call. A server that takes it says so at `initialize`
//! (`capabilities.experimental.view`); one that does not is given files by `didOpen`, as before.
//!
//! The difference is found with git in the agent's own view: the working tree against the base (commits on the
//! layer's branch and uncommitted changes alike), and the files git neither tracks nor ignores.

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One repository of the view: its name, the path the agent sees it at, its base commit.
#[derive(Clone, Debug, PartialEq)]
pub struct Repo {
    pub name: String,
    pub root: PathBuf,
    pub base: String,
}

/// The repositories of the view the agent at ROOT works in: in a layer (COORD_LAYER), the layer's own record in
/// `$COORD/layers/layers.json`; outside one, each repository's HEAD, from the project file's `repo`/`repos`, or ROOT's
/// own repository when it names none. Never empty when ROOT is in a git repository.
pub fn repos(root: &Path) -> Result<Vec<Repo>> {
    let names = project_repos().unwrap_or_default();
    if let (Ok(coord), Ok(id)) = (std::env::var("COORD"), std::env::var("COORD_LAYER")) {
        let id = id.trim();
        if !id.is_empty() {
            let file = Path::new(&coord).join("layers/layers.json");
            let text = std::fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
            return of_layer(&text, id, &names);
        }
    }
    let mut out = Vec::new();
    for (path, name) in &names {
        if path.starts_with(root) || root.starts_with(path) {
            if let Some(base) = head(path) {
                out.push(Repo { name: name.clone(), root: path.clone(), base });
            }
        }
    }
    if !out.iter().any(|r| root.starts_with(&r.root)) {
        let top = toplevel(root).ok_or_else(|| anyhow!("{} is in no git repository: no base to give the server", root.display()))?;
        let base = head(&top).ok_or_else(|| anyhow!("{}: no HEAD", top.display()))?;
        out.insert(0, Repo { name: name_of(&top, &names), root: top, base });
    }
    // the one the agent works in first (`skip` is relative to it)
    out.sort_by_key(|r| (!root.starts_with(&r.root), r.root.components().count()));
    Ok(out)
}

/// The repositories of layer ID in layers.json's TEXT (coordd layer.rs `Layer`: `repo`, `base`, `also[]` with
/// `name`, `repo`, `base`), named by NAMES where the project file names them.
pub fn of_layer(text: &str, id: &str, names: &BTreeMap<PathBuf, String>) -> Result<Vec<Repo>> {
    let v: Value = serde_json::from_str(text).context("layers.json is not JSON")?;
    let l = v.get(id).ok_or_else(|| anyhow!("layers.json has no layer {id}"))?;
    let field = |x: &Value, k: &str| x.get(k).and_then(Value::as_str).map(String::from).ok_or_else(|| anyhow!("layer {id}: no {k}"));
    let root = PathBuf::from(field(l, "repo")?);
    let mut out = vec![Repo { name: name_of(&root, names), root, base: field(l, "base")? }];
    for p in l.get("also").and_then(Value::as_array).cloned().unwrap_or_default() {
        out.push(Repo { name: field(&p, "name")?, root: PathBuf::from(field(&p, "repo")?), base: field(&p, "base")? });
    }
    Ok(out)
}

/// The project file's repositories, by path: `repos = { NAME = PATH }` and `repo` (a path, or one of the names),
/// paths as coordd takes them (relative to the file's folder, absolute, or `~/`).
/// The project's main repository's name (`[project] repo`), whose commit names the published sets.
pub fn main_repo() -> Option<String> {
    let file = crate::index::project_file().ok()?;
    let v: toml::Value = toml::from_str(&std::fs::read_to_string(&file).ok()?).ok()?;
    let r = v.get("project")?.get("repo")?.as_str()?;
    // a name of `repos`, or a path whose folder's name it is
    Some(if r.contains('/') { Path::new(r).file_name()?.to_string_lossy().into_owned() } else { r.to_string() })
}

fn project_repos() -> Result<BTreeMap<PathBuf, String>> {
    let file = crate::index::project_file()?;
    let v: toml::Value = toml::from_str(&std::fs::read_to_string(&file)?)?;
    let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
    let place = |s: &str| -> PathBuf {
        let p = match s.strip_prefix("~/") {
            Some(rest) => PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(rest),
            None => dir.join(s),
        };
        p.canonicalize().unwrap_or(p)
    };
    let mut out = BTreeMap::new();
    let p = v.get("project");
    if let Some(t) = p.and_then(|p| p.get("repos")).and_then(toml::Value::as_table) {
        for (name, path) in t {
            if let Some(path) = path.as_str() {
                out.insert(place(path), name.clone());
            }
        }
    }
    if let Some(r) = p.and_then(|p| p.get("repo")).and_then(toml::Value::as_str) {
        if !out.values().any(|n| n == r) {
            let path = place(r);
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            out.entry(path).or_insert(name);
        }
    }
    Ok(out)
}

fn name_of(root: &Path, names: &BTreeMap<PathBuf, String>) -> String {
    let canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    names.get(&canon).or_else(|| names.get(root)).cloned().unwrap_or_else(|| root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
}

fn git(dir: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let o = Command::new("git").arg("-C").arg(dir).args(args).output().with_context(|| format!("git {}", args.join(" ")))?;
    if !o.status.success() {
        bail!("git {} in {}: {}", args.join(" "), dir.display(), String::from_utf8_lossy(&o.stderr).trim());
    }
    Ok(o.stdout)
}

fn head(dir: &Path) -> Option<String> {
    git(dir, &["rev-parse", "HEAD"]).ok().map(|o| String::from_utf8_lossy(&o).trim().to_string()).filter(|s| s.len() >= 40)
}

fn toplevel(dir: &Path) -> Option<PathBuf> {
    git(dir, &["rev-parse", "--show-toplevel"]).ok().map(|o| PathBuf::from(String::from_utf8_lossy(&o).trim()))
}

/// What differs in REPO from its base, now: the paths changed or added (the working tree against the base, and
/// files git neither tracks nor ignores), and the paths deleted, each relative to the repository's root.
pub fn difference(r: &Repo) -> Result<(Vec<String>, Vec<String>)> {
    let mut changed = Vec::new();
    let mut deleted = Vec::new();
    let out = git(&r.root, &["diff", "--name-status", "-z", "--no-renames", "--no-ext-diff", &r.base, "--"])?;
    let mut parts = out.split(|&b| b == 0).filter(|s| !s.is_empty());
    while let Some(st) = parts.next() {
        let Some(path) = parts.next() else { break };
        let path = String::from_utf8_lossy(path).into_owned();
        match st.first() {
            Some(b'D') => deleted.push(path),
            _ => changed.push(path),
        }
    }
    let others = git(&r.root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    for p in others.split(|&b| b == 0).filter(|s| !s.is_empty()) {
        let p = String::from_utf8_lossy(p).into_owned();
        // a nested repository the outer one does not ignore is its own repo of the view
        if !p.ends_with('/') {
            changed.push(p);
        }
    }
    changed.sort();
    changed.dedup();
    Ok((changed, deleted))
}

/// PATH as (repository, path from its root), by the deepest repository root it lies under.
pub fn locate<'a>(repos: &'a [Repo], path: &Path) -> Option<(&'a Repo, String)> {
    repos
        .iter()
        .filter(|r| path.starts_with(&r.root))
        .max_by_key(|r| r.root.components().count())
        .map(|r| (r, path.strip_prefix(&r.root).unwrap().to_string_lossy().into_owned()))
}

/// A file's entry of `changed`: its text, exactly; a file that is not UTF-8 by its git blob id.
pub fn entry(r: &Repo, rel: &str, bytes: Vec<u8>) -> Value {
    match String::from_utf8(bytes) {
        Ok(text) => json!({"repo": r.name, "path": rel, "text": text}),
        Err(e) => {
            let sha = blob_sha(&r.root, e.as_bytes()).unwrap_or_default();
            json!({"repo": r.name, "path": rel, "binary": true, "sha": sha})
        }
    }
}

fn blob_sha(dir: &Path, bytes: &[u8]) -> Result<String> {
    use std::io::Write;
    let mut c = Command::new("git").arg("-C").arg(dir).args(["hash-object", "--stdin"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn()?;
    c.stdin.take().unwrap().write_all(bytes)?;
    let o = c.wait_with_output()?;
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// `initializationOptions.view`.
pub fn init(repos: &[Repo], skip: &[String]) -> Value {
    json!({
        "repos": repos.iter().map(|r| json!({"name": r.name, "root": r.root.display().to_string(), "base": r.base})).collect::<Vec<_>>(),
        "skip": skip,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(dir: &Path, args: &[&str]) {
        let st = Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).status().unwrap();
        assert!(st.success(), "git {args:?}");
    }

    /// A layer's repositories come from its record, named by the project file where it names them.
    #[test]
    fn a_layers_repositories_are_its_record() {
        let text = r#"{"E-3.l1": {"repo": "/p/tovrin", "base": "aaaa", "also": [{"name": "p4", "repo": "/p/tovrin/p4", "base": "bbbb", "branch": "layer/E-3.l1"}]}}"#;
        let names: BTreeMap<PathBuf, String> = [(PathBuf::from("/p/tovrin"), "tovrin".to_string())].into();
        let r = of_layer(text, "E-3.l1", &names).unwrap();
        assert_eq!(r[0], Repo { name: "tovrin".into(), root: "/p/tovrin".into(), base: "aaaa".into() });
        assert_eq!(r[1], Repo { name: "p4".into(), root: "/p/tovrin/p4".into(), base: "bbbb".into() });
        assert!(of_layer(text, "E-4.l1", &names).is_err());
        assert_eq!(locate(&r, Path::new("/p/tovrin/p4/a.p4")).map(|(r, p)| (r.name.clone(), p)), Some(("p4".into(), "a.p4".into())));
        assert_eq!(locate(&r, Path::new("/p/tovrin/src/A.hs")).map(|(r, p)| (r.name.clone(), p)), Some(("tovrin".into(), "src/A.hs".into())));
    }

    /// The difference against the base: a commit after it, an uncommitted change, a new file, a deletion; not what
    /// git ignores.
    #[test]
    fn the_difference_is_against_the_base() {
        let d = std::env::temp_dir().join(format!("symbols-diff-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        sh(&d, &["init", "-q"]);
        for (f, t) in [("a.txt", "a\n"), ("b.txt", "b\n"), ("c.txt", "c\n"), (".gitignore", "out/\n")] {
            std::fs::write(d.join(f), t).unwrap();
        }
        sh(&d, &["add", "."]);
        sh(&d, &["commit", "-qm", "base"]);
        let base = head(&d).unwrap();
        std::fs::write(d.join("a.txt"), "a2\n").unwrap();
        sh(&d, &["commit", "-qam", "on the layer"]);
        std::fs::write(d.join("b.txt"), "b2\n").unwrap();
        std::fs::remove_file(d.join("c.txt")).unwrap();
        std::fs::write(d.join("new.txt"), "n\n").unwrap();
        std::fs::create_dir_all(d.join("out")).unwrap();
        std::fs::write(d.join("out/built"), "x").unwrap();
        let r = Repo { name: "r".into(), root: d.clone(), base };
        let (changed, deleted) = difference(&r).unwrap();
        assert_eq!(changed, vec!["a.txt", "b.txt", "new.txt"]);
        assert_eq!(deleted, vec!["c.txt"]);
        let e = entry(&r, "x.bin", vec![0xff, 0xfe, 0]);
        assert_eq!(e["binary"], true);
        assert_eq!(e["sha"].as_str().unwrap().len(), 40);
        let _ = std::fs::remove_dir_all(&d);
    }
}
