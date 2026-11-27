//! A task as the evaluation knows it, whatever suite it comes from, and the manifest that lists tasks.

use compute::Digest;
use serde::Deserialize;

/// The folder of the verifier's job that holds the verifier's tree; its command is one of its files.
pub const VERIFIER: &str = "verifier";

/// What one run may spend: tokens and cents are charged by the norms; time, CPUs and memory bound each job.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub tokens: u64,
    pub usd_cents: u64,
    pub wall_ms: u64,
    pub cpus: u32,
    pub memory_bytes: u64,
}

/// One task: the pinned root filesystem, the input tree the candidate works on, the verifier's own tree, the
/// command that verifies (a file of that tree), and the budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub image_digest: Digest,
    pub input_digest: Digest,
    pub verifier_digest: Digest,
    pub verify_cmd: Vec<String>,
    pub budget: Budget,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: String,
    image: String,
    input: String,
    verifier: String,
    verify: Vec<String>,
    budget: Budget,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    task: Vec<Entry>,
}

/// A digest written `HASH/SIZE`, as the remote execution API writes one.
pub fn digest(s: &str) -> Result<Digest, String> {
    let (hash, size) = s.split_once('/').ok_or(format!("{s}: not HASH/SIZE"))?;
    let size: i64 = size.parse().map_err(|_| format!("{s}: the size is no number"))?;
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) || size < 0 {
        return Err(format!("{s}: not a SHA-256 and a size"));
    }
    Ok(Digest { hash: hash.to_ascii_lowercase(), size })
}

/// One manifest entry checked: an id of letters, digits, `.`, `_` and `-`; every digest; a verifier tree that is not
/// the input; a verify command that runs a file of the verifier's tree; no budget dimension at zero.
fn task(e: Entry) -> Result<Task, String> {
    let id = e.id;
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)) {
        return Err(format!("{id:?}: an id is letters, digits, `.`, `_` and `-`"));
    }
    let image_digest = digest(&e.image).map_err(|x| format!("{id}: image {x}"))?;
    let input_digest = digest(&e.input).map_err(|x| format!("{id}: input {x}"))?;
    let verifier_digest = digest(&e.verifier).map_err(|x| format!("{id}: verifier {x}"))?;
    if verifier_digest == input_digest {
        return Err(format!("{id}: the verifier's tree is the candidate's input"));
    }
    let runs_its_own = e.verify.first().and_then(|c| c.strip_prefix(&format!("{VERIFIER}/"))).is_some_and(|rest| !rest.is_empty() && !rest.split('/').any(|p| p.is_empty() || p == "." || p == ".."));
    if !runs_its_own {
        return Err(format!("{id}: the verify command runs a file of {VERIFIER}/, nothing the candidate left"));
    }
    let b = e.budget;
    if b.tokens == 0 || b.usd_cents == 0 || b.wall_ms == 0 || b.cpus == 0 || b.memory_bytes == 0 {
        return Err(format!("{id}: a budget of zero in some dimension"));
    }
    Ok(Task { id, image_digest, input_digest, verifier_digest, verify_cmd: e.verify, budget: b })
}

/// The tasks of a manifest: `[[task]]` tables of `id`, `image`, `input`, `verifier`, `verify` and `budget`; ids are
/// unique.
pub fn read(text: &str) -> Result<Vec<Task>, String> {
    let m: Manifest = toml::from_str(text).map_err(|e| format!("manifest: {e}"))?;
    if m.task.is_empty() {
        return Err("manifest: no task".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    m.task
        .into_iter()
        .map(|e| {
            if !seen.insert(e.id.clone()) {
                return Err(format!("{}: listed twice", e.id));
            }
            task(e)
        })
        .collect()
}
