//! The repositories: a new commit's Events by its trailers, as the config names them; HEAD.

use crate::config::Named;
use docket_v2::run::value::{Instance, Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const ARTIFACT_CHANGED: &str = "std.rails.Artifact-Changed";

fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git").arg("-C").arg(repo).args(args).output().map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!("git {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The commit HEAD is at in `repo`.
pub fn head(repo: &Path) -> Result<String, String> {
    git(repo, &["rev-parse", "HEAD"]).map(|s| s.trim().to_string())
}

/// What a commit's trailers say: its parties' Event, and the trailer of the artifacts it changed.
pub struct Said<'a> {
    pub committed: Option<&'a Named>,
    pub changed_by: Option<&'a str>,
}

/// Each repository's branch tips as last seen; a repository's first look only notes them.
#[derive(Default, Serialize, Deserialize)]
pub struct Commits {
    seen: BTreeMap<PathBuf, Vec<String>>,
}

impl Commits {
    /// The Events of the commits made on any branch since the last look, oldest first. A repository whose log
    /// cannot be read keeps its old tips, so its commits come at a later look.
    pub fn look(&mut self, repos: &[PathBuf], said: &Said) -> Vec<Instance> {
        let mut out = Vec::new();
        for repo in repos {
            let Ok(tips) = git(repo, &["for-each-ref", "--format=%(objectname)", "refs/heads"]) else { continue };
            let tips: Vec<String> = tips.lines().map(str::to_string).collect();
            let new = match self.seen.get(repo) {
                Some(old) if *old != tips => new_commits(repo, &tips, old, said),
                _ => Ok(Vec::new()),
            };
            match new {
                Ok(evs) => {
                    out.extend(evs);
                    self.seen.insert(repo.clone(), tips);
                }
                Err(why) => eprintln!("docket-driver-host: {}: {why}; its new commits wait for the next look", repo.display()),
            }
        }
        out
    }
}

fn new_commits(repo: &Path, tips: &[String], old: &[String], said: &Said) -> Result<Vec<Instance>, String> {
    // an old tip that is gone (a branch deleted and collected) is no bound; with none left, nothing is said
    let old: Vec<&str> = old.iter().map(String::as_str).filter(|t| git(repo, &["cat-file", "-e", &format!("{t}^{{commit}}")]).is_ok()).collect();
    if old.is_empty() {
        eprintln!("docket-driver-host: {}: no tip seen before is left, so its commits until now are not said", repo.display());
        return Ok(Vec::new());
    }
    let field = |t: Option<&str>| t.map_or(String::new(), |t| format!("%(trailers:key={t},valueonly,separator=%x1f)"));
    let format = format!("--format=%H%x1e{}%x1e{}%x00", field(said.committed.map(|n| n.from.as_str())), field(said.changed_by));
    let mut args = vec!["log", "--reverse", format.as_str()];
    args.extend(tips.iter().map(String::as_str));
    args.push("--not");
    args.extend(old);
    let mut out = Vec::new();
    for c in git(repo, &args)?.split('\0').map(str::trim).filter(|c| !c.is_empty()) {
        let mut f = c.split('\x1e');
        let (hash, parties, changed) = (f.next().unwrap_or(""), f.next().unwrap_or(""), f.next().unwrap_or(""));
        if let Some(n) = said.committed {
            out.extend(split(parties).map(|p| Instance { ty: n.event.clone(), args: vec![Value::text(p), Value::text(hash)] }));
        }
        out.extend(split(changed).map(|a| Instance { ty: ARTIFACT_CHANGED.into(), args: vec![Value::text(a)] }));
    }
    Ok(out)
}

/// A trailer's values: one per trailer line, and commas within a line.
fn split(values: &str) -> impl Iterator<Item = &str> {
    values.split(['\x1f', ',', '\n']).map(str::trim).filter(|v| !v.is_empty())
}
