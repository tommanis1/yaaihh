//! REAPI v2's messages, as the fields this crate writes and reads.

use crate::pb::{Enc, Msg};
use crate::{Digest, JobResult, JobSpec};

pub const SHA256: u64 = 1;
pub const OUT: &str = "out";

pub fn digest(d: &Digest) -> Enc {
    Enc::new().str(1, &d.hash).int(2, d.size)
}

pub fn read_digest(m: &Msg) -> Digest {
    Digest { hash: m.str(1), size: m.int(2) }
}

fn opt_digest(m: &Msg, field: u32) -> Result<Option<Digest>, String> {
    Ok(m.msg(field)?.map(|d| read_digest(&d)))
}

/// The worker properties a job asks for, sorted by name as the API wants.
fn platform(job: &JobSpec) -> Enc {
    let mut props = vec![("network", if job.network_off { "off" } else { "on" }.to_string())];
    if let Some(r) = &job.rootfs_digest {
        props.push(("rootfs", format!("{}-{}", r.hash, r.size)));
    }
    if let Some(c) = job.limits.cpus {
        props.push(("cpu_count", c.to_string()));
    }
    if let Some(m) = job.limits.memory_bytes {
        props.push(("memory_kb", m.div_ceil(1024).to_string()));
    }
    props.sort();
    props.into_iter().fold(Enc::new(), |e, (n, v)| e.msg(1, Enc::new().str(1, n).str(2, &v)))
}

pub fn command(job: &JobSpec) -> Vec<u8> {
    let mut e = job.cmd.iter().fold(Enc::new(), |e, a| e.str(1, a));
    for (n, v) in &job.env {
        e = e.msg(2, Enc::new().str(1, n).str(2, v));
    }
    e.msg(5, platform(job)).str(7, OUT).done()
}

pub fn action(job: &JobSpec, command: &Digest) -> Vec<u8> {
    let mut e = Enc::new().msg(1, digest(command)).msg(2, digest(&job.input_tree_digest));
    if let Some(ms) = job.limits.wall_ms {
        e = e.msg(6, Enc::new().uint(1, ms / 1000).uint(2, ms % 1000 * 1_000_000));
    }
    e.msg(10, platform(job)).done()
}

/// An action result as the job's result, and the `Tree` to unpack when `out/` came only as one; `cached` and
/// `wall_ms` are the caller's to set.
pub fn result_and_tree(b: &[u8]) -> Result<(JobResult, Option<Digest>), String> {
    let m = Msg::read(b)?;
    let dirs = m.all(3).into_iter().map(Msg::read).collect::<Result<Vec<_>, _>>()?;
    let out = dirs.iter().find(|d| d.str(1) == OUT);
    let (output_tree_digest, tree) = match out {
        Some(d) => match opt_digest(d, 5)? {
            Some(root) => (Some(root), None),
            None => (None, opt_digest(d, 3)?),
        },
        None => (None, None),
    };
    let raw = |field| m.bytes(field).map(Digest::of);
    let r = JobResult {
        exit_code: m.int(4) as i32,
        stdout_digest: opt_digest(&m, 6)?.or_else(|| raw(5)).unwrap_or_else(|| Digest::of(b"")),
        stderr_digest: opt_digest(&m, 8)?.or_else(|| raw(7)).unwrap_or_else(|| Digest::of(b"")),
        output_tree_digest,
        cached: false,
        wall_ms: 0,
    };
    Ok((r, tree))
}

/// An action result this crate's own drivers wrote, which always names `out/`'s root directory.
pub fn result(b: &[u8]) -> Result<JobResult, String> {
    result_and_tree(b).map(|(r, _)| r)
}

/// The action result a job's result is, as this crate's own store keeps it.
pub fn encode_result(r: &JobResult) -> Vec<u8> {
    let mut e = Enc::new();
    if let Some(t) = &r.output_tree_digest {
        e = e.msg(3, Enc::new().str(1, OUT).msg(5, digest(t)));
    }
    e.int(4, r.exit_code.into()).msg(6, digest(&r.stdout_digest)).msg(8, digest(&r.stderr_digest)).done()
}

/// A blob's name for ByteStream: read from `{instance}/blobs/…`, written to `{instance}/uploads/{id}/blobs/…`.
pub fn resource(instance: &str, upload: Option<&str>, d: &Digest) -> String {
    let at = if instance.is_empty() { String::new() } else { format!("{instance}/") };
    let up = upload.map(|id| format!("uploads/{id}/")).unwrap_or_default();
    format!("{at}{up}blobs/{}/{}", d.hash, d.size)
}

/// One message of an Execute stream: its live output's names, and when done its result or failure.
pub struct Progress {
    pub stdout_stream: String,
    pub stderr_stream: String,
    pub done: Option<Result<(Vec<u8>, bool), String>>,
}

pub fn operation(b: &[u8]) -> Result<Progress, String> {
    let op = Msg::read(b)?;
    let any_value = |field| -> Result<Option<Vec<u8>>, String> { Ok(op.msg(field)?.and_then(|a| a.bytes(2).map(<[u8]>::to_vec))) };
    let meta = any_value(2)?;
    let meta = meta.as_deref().map(Msg::read).transpose()?;
    let (stdout_stream, stderr_stream) = meta.map(|m| (m.str(3), m.str(4))).unwrap_or_default();
    let done = if op.uint(3) == 0 {
        None
    } else if let Some(err) = op.msg(4)? {
        Some(Err(format!("status {}: {}", err.uint(1), err.str(2))))
    } else {
        let resp = any_value(5)?.ok_or("a finished operation with no response")?;
        let r = Msg::read(&resp)?;
        match r.msg(3)? {
            Some(s) if s.uint(1) != 0 => Some(Err(format!("status {}: {}", s.uint(1), s.str(2)))),
            _ => Some(Ok((r.bytes(1).unwrap_or_default().to_vec(), r.uint(2) != 0))),
        }
    };
    Ok(Progress { stdout_stream, stderr_stream, done })
}
