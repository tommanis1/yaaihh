//! Jobs as content-addressed actions: run through a Remote Execution API v2 endpoint, or on this host.

mod config;
mod grpc;
mod local;
#[cfg(test)]
mod mock;
mod pb;
mod reapi;
mod remote;
#[cfg(test)]
mod tests;
mod tree;
mod warm;

pub use config::Config;
pub use local::LocalFallbackDriver;
pub use remote::{Endpoint, RemoteDriver};
pub use tree::{nest, tree, File};
pub use warm::WarmLayerHandle;

use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;

/// A blob by its SHA-256 and its length.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Digest {
    pub hash: String,
    pub size: i64,
}

impl Digest {
    pub fn of(b: &[u8]) -> Self {
        let hash = Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect();
        Digest { hash, size: b.len() as i64 }
    }
}

/// What a job may use; none set leaves it to the worker.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Limits {
    pub cpus: Option<u32>,
    pub memory_bytes: Option<u64>,
    pub wall_ms: Option<u64>,
}

/// A command over an input tree, in a root filesystem; its outputs are what it leaves in `out/`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobSpec {
    /// The tree mounted as `/`; none takes the worker's own.
    pub rootfs_digest: Option<Digest>,
    pub input_tree_digest: Digest,
    pub cmd: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub limits: Limits,
    pub network_off: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobResult {
    pub exit_code: i32,
    pub stdout_digest: Digest,
    pub stderr_digest: Digest,
    /// The root directory of `out/`, when the job left one.
    pub output_tree_digest: Option<Digest>,
    /// Answered from the action cache, nothing run.
    pub cached: bool,
    pub wall_ms: u64,
}

/// Which of a job's outputs a chunk of live output belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// What runs jobs: blobs into and out of its store, and a job run with its output handed over as it comes.
pub trait ComputeDriver {
    /// Puts the blobs the store lacks; their digests, in order.
    fn upload(&mut self, blobs: &[Vec<u8>]) -> Result<Vec<Digest>, String>;
    fn download(&mut self, digest: &Digest) -> Result<Vec<u8>, String>;
    /// Runs `job`, or answers it from the action cache; `out` gets its output as it comes; `stop` set ends it.
    fn run(&mut self, job: &JobSpec, out: &mut dyn FnMut(Stream, &[u8]), stop: &AtomicBool) -> Result<JobResult, String>;
}

/// The command and action a job is, as blobs; the action's digest is the cache's key.
pub(crate) fn action(job: &JobSpec) -> (Vec<u8>, Vec<u8>, Digest) {
    let command = reapi::command(job);
    let action = reapi::action(job, &Digest::of(&command));
    let key = Digest::of(&action);
    (command, action, key)
}
