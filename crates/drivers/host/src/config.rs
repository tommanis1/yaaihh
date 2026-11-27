//! The host driver's file: what it watches, where, and the names of the project's own Events and trailers.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// A project's Event, and where the host finds it: a trailer's name, a file or a folder.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Named {
    pub event: String,
    pub from: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The repository whose HEAD Git-Head answers.
    pub head: Option<PathBuf>,
    /// Repositories whose new commits are looked at.
    #[serde(default)]
    pub repos: Vec<PathBuf>,
    /// A commit's Event `(party, commit)`, for each party its trailer `from` names.
    pub committed: Option<Named>,
    /// The trailer whose values are the artifacts a commit changed (std.rails.Artifact-Changed).
    pub changed_by: Option<String>,
    /// A time's Event `(key)`, once it is past, for each key the file `from` lists, one a line.
    pub times: Option<Named>,
    /// Kept folders, each a key, Sized once a day.
    #[serde(default)]
    pub sized: Vec<PathBuf>,
    /// A proof's Event `(piece, condition, file, release)` for each `*.proof` file of folder `from`.
    pub proofs: Option<Named>,
    /// Where the watch keeps the times it said and the tips it saw, so a restart neither repeats nor loses them.
    pub state: Option<PathBuf>,
}

impl Config {
    /// The file's text, its relative paths read from folder `dir`, the file's own.
    pub fn read(text: &str, dir: &Path) -> Result<Self, String> {
        let mut c: Config = serde_json::from_str(text).map_err(|e| e.to_string())?;
        for p in [&mut c.head, &mut c.state].into_iter().flatten().chain(&mut c.repos).chain(&mut c.sized) {
            *p = dir.join(&*p);
        }
        for n in [&mut c.times, &mut c.proofs].into_iter().flatten() {
            n.from = dir.join(&n.from).display().to_string();
        }
        let trailers = c.committed.iter().map(|n| n.from.as_str()).chain(c.changed_by.as_deref());
        if let Some(t) = trailers.into_iter().find(|t| t.is_empty() || !t.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-')) {
            return Err(format!("{t:?} is not a trailer's name"));
        }
        Ok(c)
    }
}
