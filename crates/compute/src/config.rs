//! The endpoints file: where jobs can go, and the setting a job takes when its caller gives only a command.

use crate::{ComputeDriver, Digest, Endpoint, JobSpec, Limits, LocalFallbackDriver, RemoteDriver};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    endpoint: Vec<EndpointEntry>,
    job: Option<JobEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EndpointEntry {
    name: String,
    url: String,
    #[serde(default = "main")]
    instance: String,
    ca: Option<PathBuf>,
    cert: Option<PathBuf>,
    key: Option<PathBuf>,
}

fn main() -> String {
    "main".into()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JobEntry {
    endpoint: Option<String>,
    local: Option<PathBuf>,
    rootfs: Option<String>,
    tree: Option<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default = "yes")]
    network_off: bool,
    cpus: Option<u32>,
    memory_bytes: Option<u64>,
    wall_ms: Option<u64>,
    #[serde(default = "one")]
    at_once: usize,
}

fn one() -> usize {
    1
}

fn yes() -> bool {
    true
}

/// A digest written `HASH-SIZE`, as the platform property names it.
pub fn digest(s: &str) -> Result<Digest, String> {
    let (hash, size) = s.rsplit_once('-').ok_or_else(|| format!("{s}: HASH-SIZE wanted"))?;
    let ok = hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase());
    let size = size.parse().ok().filter(|n: &i64| *n >= 0 && ok).ok_or_else(|| format!("{s}: HASH-SIZE wanted"))?;
    Ok(Digest { hash: hash.into(), size })
}

/// The file read: its endpoints, and the job setting with where it runs.
pub struct Config {
    pub endpoints: Vec<Endpoint>,
    /// A job with no command; callers put theirs in.
    pub base: JobSpec,
    /// How many jobs may run at once.
    pub at_once: usize,
    target: Target,
}

enum Target {
    Endpoint(Endpoint),
    Local(PathBuf),
}

impl Config {
    pub fn read(text: &str) -> Result<Self, String> {
        let f: File = toml::from_str(text).map_err(|e| e.to_string())?;
        let endpoints: Vec<Endpoint> = f.endpoint.into_iter().map(|e| Endpoint { name: e.name, url: e.url, instance: e.instance, ca: e.ca, cert: e.cert, key: e.key }).collect();
        let j = f.job.ok_or("no [job]")?;
        let target = match (&j.endpoint, j.local) {
            (Some(n), None) => Target::Endpoint(endpoints.iter().find(|e| e.name == *n).cloned().ok_or_else(|| format!("[job] endpoint {n}: no such [[endpoint]]"))?),
            (None, Some(dir)) => Target::Local(dir),
            _ => return Err("[job] wants endpoint or local, one of them".into()),
        };
        let base = JobSpec {
            rootfs_digest: j.rootfs.as_deref().map(digest).transpose()?,
            // no tree given is the empty directory
            input_tree_digest: j.tree.as_deref().map(digest).transpose()?.unwrap_or_else(|| Digest::of(b"")),
            cmd: Vec::new(),
            env: j.env,
            limits: Limits { cpus: j.cpus, memory_bytes: j.memory_bytes, wall_ms: j.wall_ms },
            network_off: j.network_off,
        };
        if j.at_once == 0 {
            return Err("[job] at_once: at least 1".into());
        }
        Ok(Config { endpoints, base, at_once: j.at_once, target })
    }

    /// A driver for where `[job]` says jobs run.
    pub fn driver(&self) -> Result<Box<dyn ComputeDriver + Send>, String> {
        Ok(match &self.target {
            Target::Endpoint(e) => Box::new(RemoteDriver::new(e.clone())?),
            Target::Local(dir) => Box::new(LocalFallbackDriver::new(dir)?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_names_its_endpoints_and_the_job_setting() {
        let h = "a".repeat(64);
        let c = Config::read(&format!("[[endpoint]]\nname = \"tower\"\nurl = \"grpcs://tower:50051\"\nca = \"/c/ca.pem\"\n\n[job]\nendpoint = \"tower\"\nrootfs = \"{h}-10\"\nwall_ms = 5000\n")).unwrap();
        assert_eq!((c.endpoints[0].instance.as_str(), c.endpoints[0].ca.clone()), ("main", Some("/c/ca.pem".into())));
        assert_eq!((c.base.rootfs_digest.clone().unwrap().size, c.base.network_off, c.base.limits.wall_ms), (10, true, Some(5000)));
        assert_eq!((c.base.input_tree_digest.clone(), c.at_once), (Digest::of(b""), 1));
        assert!(Config::read("[job]\nlocal = \"/x\"\nat_once = 0\n").is_err());
        assert!(Config::read("[job]\nendpoint = \"none\"\n").is_err());
        assert!(Config::read("[job]\nlocal = \"/x\"\nendpoint = \"y\"\n").is_err());
        assert!(Config::read("[job]\nlocal = \"/x\"\nflag = 1\n").is_err(), "an unknown key is refused");
        assert!(digest("abc-1").is_err() && digest(&format!("{h}-x")).is_err());
    }
}
