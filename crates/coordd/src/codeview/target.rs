//! A code view target resolved against the project's Files roots: the root it is in and what to open there.

use coord_types::codeview::CodeTarget;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// One open: the root it is in, and the action our extension performs there.
#[derive(Debug, PartialEq)]
pub struct Open {
    pub folder: String,
    pub action: Value,
}

/// TARGET resolved: names and relative paths against ROOTS, everything kept inside one of them and out of HOME's
/// credential places and `.git` (as Files serves); the workspace alone opens nothing.
pub fn resolve(roots: &[(String, String)], home: &Path, target: &CodeTarget) -> Result<Option<Open>, String> {
    let roots: Vec<(String, PathBuf)> = roots.iter().filter_map(|(n, p)| std::fs::canonicalize(p).ok().map(|c| (n.clone(), c))).collect();
    match target {
        CodeTarget::File { path, line } => {
            let file = locate(&roots, path)?;
            if !file.is_file() {
                return Err(format!("coord codeview: {path} is not a file"));
            }
            let root = root_of(&roots, &file).ok_or_else(|| format!("coord codeview: {path} is in no folder of this project's Files"))?;
            if let Some(why) = crate::files::never_served(&std::fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf()), &file) {
                return Err(format!("coord codeview: {path} is never shown ({why})"));
            }
            Ok(Some(Open { folder: show(&root), action: json!({"is": "file", "path": show(&file), "line": line.unwrap_or(1).max(1)}) }))
        }
        CodeTarget::Commit { repo, sha } => {
            let root = repo_root(&roots, repo)?;
            if !(4..=64).contains(&sha.len()) || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(format!("coord codeview: {sha} is not a commit id"));
            }
            let full = commit(&root, sha)?;
            Ok(Some(Open { folder: show(&root), action: json!({"is": "commit", "sha": full}) }))
        }
        CodeTarget::Repo { path } => {
            let root = repo_root(&roots, path)?;
            Ok(Some(Open { folder: show(&root), action: json!({"is": "repo"}) }))
        }
        CodeTarget::Workspace => Ok(None),
    }
}

fn show(p: &Path) -> String {
    p.display().to_string()
}

/// PATH as a file: absolute, or relative to the first root that has it.
fn locate(roots: &[(String, PathBuf)], path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path);
    let found = if p.is_absolute() { Some(p.to_path_buf()) } else { roots.iter().map(|(_, r)| r.join(p)).find(|f| f.exists()) };
    found.and_then(|f| std::fs::canonicalize(f).ok()).ok_or_else(|| format!("coord codeview: {path} is not there"))
}

/// The root that holds FILE; the deepest one when they nest.
fn root_of(roots: &[(String, PathBuf)], file: &Path) -> Option<PathBuf> {
    roots.iter().map(|(_, r)| r).filter(|r| file.starts_with(r)).max_by_key(|r| r.components().count()).cloned()
}

/// A root by its name, or by its folder or one inside it.
fn repo_root(roots: &[(String, PathBuf)], repo: &str) -> Result<PathBuf, String> {
    if let Some((_, r)) = roots.iter().find(|(n, _)| n == repo) {
        return Ok(r.clone());
    }
    std::fs::canonicalize(repo).ok().and_then(|p| root_of(roots, &p)).ok_or_else(|| format!("coord codeview: {repo} is no folder of this project's Files"))
}

/// SHA's full id in ROOT, refused when it names no commit there.
fn commit(root: &Path, sha: &str) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--verify", "--quiet", &format!("{sha}^{{commit}}")])
        .output()
        .map_err(|e| format!("coord codeview: git: {e}"))?;
    let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() && !id.is_empty() { Ok(id) } else { Err(format!("coord codeview: {sha} is no commit of {}", root.display())) }
}
