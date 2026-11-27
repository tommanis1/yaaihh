//! A run's evidence: every look's files and what was expected of them, by digest, in one manifest named by its own digest.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use yaaihh_e2e::{digest, digest_text, Look};

/// What one run of a plan did: its looks, each expectation and whether it held, the steps that could not be done.
#[derive(Default)]
pub struct Run {
    pub folder: PathBuf,
    pub window_sha256: String,
    pub looks: Vec<Look>,
    pub expects: Vec<(String, Vec<String>)>,
    pub failed_steps: Vec<String>,
    pub server_log: String,
    /// What the window showed when it opened, before any step of the plan.
    pub before: String,
}

impl Run {
    /// Why this run attests nothing, if it does not: no look, no expectation, one that failed, a step not done, or no
    /// required text that was not already shown before the first step.
    pub fn short_of_a_pass(&self) -> Option<String> {
        if self.looks.is_empty() {
            return Some("the window was never looked at".into());
        }
        if self.expects.is_empty() {
            return Some("nothing was expected of what it showed".into());
        }
        if let Some(s) = self.failed_steps.first() {
            return Some(format!("a step could not be done: {s}"));
        }
        if let Some((w, f)) = self.expects.iter().find(|(_, f)| !f.is_empty()) {
            return Some(format!("expected {w:?}, not shown: {}", f.join(", ")));
        }
        let mut shown: Vec<&str> = Vec::new();
        for t in self.expects.iter().flat_map(|(w, _)| w.lines().map(str::trim)).filter(|l| !l.is_empty() && !l.starts_with('!')) {
            if !shown.contains(&t) {
                shown.push(t);
            }
        }
        let new = shown.iter().any(|t| !self.before.contains(t));
        (!new).then(|| format!("every text it required ({}) was on the screen before its first step, so it shows nothing the change did", shown.join(", ")))
    }
}

/// The sealed run: the manifest written, its digest the evidence's name, a copy kept by that name under SEALED.
pub fn seal(run: &Run, header: Value, sealed: &Path) -> Result<String, String> {
    let file = |p: &Path| -> Result<Value, String> { Ok(json!({ "path": p.display().to_string(), "sha256": digest(p)? })) };
    let looks = run
        .looks
        .iter()
        .map(|l| Ok(json!({ "step": l.step, "picture": file(&l.png)?, "tree": file(&l.tree)?, "log": file(&l.log)?, "seen": l.seen() })))
        .collect::<Result<Vec<Value>, String>>()?;
    let log = run.folder.join("coordd.log");
    std::fs::write(&log, &run.server_log).map_err(|e| format!("{}: {e}", log.display()))?;
    let expects: Vec<Value> = run.expects.iter().map(|(w, f)| json!({ "wanted": w, "held": f.is_empty(), "not_shown": f })).collect();
    let mut m = header;
    m["window_sha256"] = run.window_sha256.clone().into();
    m["looks"] = looks.into();
    m["expects"] = expects.into();
    m["failed_steps"] = run.failed_steps.clone().into();
    m["server_log"] = file(&log)?;
    m["passed"] = run.short_of_a_pass().is_none().into();
    let text = serde_json::to_string_pretty(&m).map_err(|e| e.to_string())?;
    let hex = digest_text(&text);
    std::fs::create_dir_all(sealed).map_err(|e| format!("{}: {e}", sealed.display()))?;
    for p in [run.folder.join("manifest.json"), sealed.join(format!("{hex}.json"))] {
        std::fs::write(&p, &text).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    Ok(format!("sha256:{hex}"))
}
