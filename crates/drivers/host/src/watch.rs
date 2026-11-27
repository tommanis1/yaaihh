//! The watch: one look at the host each minute, and what it remembers between looks, kept over restarts.

use crate::config::Config;
use crate::{clock, git, store};
use docket_v2::run::value::Instance;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Default, Serialize, Deserialize)]
pub struct Watch {
    clock: clock::Clock,
    commits: git::Commits,
    #[serde(skip)]
    day: Option<u64>,
}

impl Watch {
    /// The watch as the state file at `state` left it; a fresh one when there is none yet.
    pub fn load(state: Option<&Path>) -> Result<Watch, String> {
        match state.map(std::fs::read_to_string) {
            None => Ok(Watch::default()),
            Some(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(Watch::default()),
            Some(r) => r.map_err(|e| e.to_string()).and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string())).map_err(|e| format!("{}: {e}", state.unwrap_or(Path::new("")).display())),
        }
    }

    /// Kept at `state`, written whole and then renamed into place.
    pub fn save(&self, state: &Path) -> Result<(), String> {
        let new = state.with_extension("new");
        let text = serde_json::to_string(self).map_err(|e| e.to_string())?;
        std::fs::write(&new, text).and_then(|()| std::fs::rename(&new, state)).map_err(|e| format!("{}: {e}", state.display()))
    }

    /// The Events of one look at `now`: the clock's, new commits', a new day's sizes, and proofs left since.
    pub fn look(&mut self, c: &Config, now: SystemTime) -> Vec<Instance> {
        let secs = now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let mut out = self.clock.tick((secs / 60) as i64, c.times.as_ref());
        out.extend(self.commits.look(&c.repos, &git::Said { committed: c.committed.as_ref(), changed_by: c.changed_by.as_deref() }));
        if self.day.replace(secs / 86_400) != Some(secs / 86_400) {
            out.extend(store::sized(&c.sized, now));
        }
        out.extend(c.proofs.as_ref().map(|p| store::proofs(Path::new(&p.from), &p.event)).unwrap_or_default());
        out
    }
}
