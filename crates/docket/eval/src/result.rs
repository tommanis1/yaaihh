//! A run's result: its compact figures kept, its trace put in cold storage and replaced by a pointer.

use cold_store::{Target, stow};
use serde::{Deserialize, Serialize};

/// What one candidate scored on one trial of one task; `trace_ref` is the trace's key in cold storage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunResult {
    pub task: String,
    pub candidate: String,
    pub trial: u32,
    pub pass: bool,
    pub usd_cents: u64,
    pub tokens: u64,
    pub wall_ms: u64,
    pub trace_ref: String,
}

/// The figures of a run that the verifier's job and the meters give.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Figures {
    pub pass: bool,
    pub usd_cents: u64,
    pub tokens: u64,
    pub wall_ms: u64,
}

/// CANDIDATE's result on trial TRIAL of TASK, with TRACE (a file or a folder) moved to TARGET under `eval-traces`
/// and a pointer left in its place.
pub fn record(task: &str, candidate: &str, trial: u32, f: Figures, trace: &str, target: &Target) -> Result<RunResult, String> {
    let p = stow(trace, "eval-traces", target)?;
    let path = std::path::Path::new(trace);
    let gone = if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
    gone.map_err(|e| format!("{trace}: put away and checked, but not removed: {e}"))?;
    Ok(RunResult { task: task.into(), candidate: candidate.into(), trial, pass: f.pass, usd_cents: f.usd_cents, tokens: f.tokens, wall_ms: f.wall_ms, trace_ref: p.key })
}
