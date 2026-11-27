//! A task as the two jobs the evaluation runs: the candidate's, sealed, then the verifier's on what it produced.

use crate::task::{Task, VERIFIER};
use compute::{Digest, JobSpec, Limits};

/// The folder of the verifier's job that holds what the candidate's job left.
pub const WORK: &str = "work";

fn sealed(task: &Task, input_tree_digest: Digest, cmd: Vec<String>) -> JobSpec {
    let b = task.budget;
    let limits = Limits { cpus: Some(b.cpus), memory_bytes: Some(b.memory_bytes), wall_ms: Some(b.wall_ms) };
    JobSpec { rootfs_digest: Some(task.image_digest.clone()), input_tree_digest, cmd, env: Default::default(), limits, network_off: true }
}

/// The candidate's job: the pinned image, the task's input tree and nothing of the verifier's, no network, bounded.
pub fn agent(task: &Task, cmd: Vec<String>) -> JobSpec {
    sealed(task, task.input_digest.clone(), cmd)
}

/// The verifier's job: `work/` holds what the candidate's job left and `verifier/` the task's verifier tree, which
/// the candidate never had and whose files nothing it left can stand in for; the command is the verifier's own.
/// The root directory's blob goes to the driver's store before the job runs.
pub fn verify(task: &Task, produced: Digest) -> Result<(JobSpec, Vec<u8>), String> {
    let (root, blob) = compute::nest(&[(WORK, produced), (VERIFIER, task.verifier_digest.clone())])?;
    Ok((sealed(task, root, task.verify_cmd.clone()), blob))
}
