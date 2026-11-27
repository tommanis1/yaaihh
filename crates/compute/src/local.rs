//! A driver on this host, for tests and work offline: a store in a folder, the job a process in a fresh folder.

use crate::reapi::{encode_result, result};
use crate::tree::{collect, materialize, tree};
use crate::{ComputeDriver, Digest, JobResult, JobSpec, Stream};
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How often a running job looks whether it was stopped.
const STOP_CHECK: Duration = Duration::from_millis(50);

/// Blobs in `cas/`, results by action in `ac/`, each job run in a folder of `work/` removed after.
pub struct LocalFallbackDriver {
    root: PathBuf,
}

impl LocalFallbackDriver {
    pub fn new(root: &Path) -> Result<Self, String> {
        for d in ["cas", "ac", "work"] {
            std::fs::create_dir_all(root.join(d)).map_err(|e| format!("{}: {e}", root.display()))?;
        }
        Ok(LocalFallbackDriver { root: root.to_path_buf() })
    }

    fn put(&self, b: &[u8]) -> Result<Digest, String> {
        let d = Digest::of(b);
        let at = self.root.join("cas").join(&d.hash);
        if !at.exists() {
            // a name of its own per write, as several drivers may share the folder
            static WRITES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let tmp = self.root.join("cas").join(format!(".{}.{}.{}", d.hash, std::process::id(), WRITES.fetch_add(1, Ordering::Relaxed)));
            std::fs::write(&tmp, b).and_then(|()| std::fs::rename(&tmp, &at)).map_err(|e| format!("{}: {e}", at.display()))?;
        }
        Ok(d)
    }

    fn get(&self, d: &Digest) -> Result<Vec<u8>, String> {
        if d.size == 0 {
            return Ok(Vec::new());
        }
        let b = std::fs::read(self.root.join("cas").join(&d.hash)).map_err(|e| format!("{}: not in the store: {e}", d.hash))?;
        if Digest::of(&b) != *d {
            return Err(format!("{}: the stored bytes are not this blob", d.hash));
        }
        Ok(b)
    }

    /// Runs `job` in folder `dir`, its output handed to `out` as it comes; its exit code and whole output. Once it has
    /// ended, output still held open by a process outside its own (one that called setsid) is not waited for.
    fn spawn(&self, job: &JobSpec, dir: &Path, out: &mut dyn FnMut(Stream, &[u8]), stop: &AtomicBool) -> Result<(i32, Vec<u8>, Vec<u8>), String> {
        let (program, args) = job.cmd.split_first().ok_or("a job with no command")?;
        // no network: a new user and network namespace, which needs no privilege
        let mut c = if job.network_off { Command::new("unshare") } else { Command::new(program) };
        if job.network_off {
            c.args(["--user", "--map-root-user", "--net", "--"]).arg(program);
        }
        let mut child = c.args(args).env_clear().envs(&job.env).current_dir(dir).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).process_group(0).spawn().map_err(|e| format!("{program}: {e}"))?;
        let (tx, rx) = mpsc::channel::<(Stream, Vec<u8>)>();
        let pipes: [(Stream, Box<dyn Read + Send>); 2] = [(Stream::Stdout, Box::new(child.stdout.take().expect("piped"))), (Stream::Stderr, Box::new(child.stderr.take().expect("piped")))];
        for (s, mut p) in pipes {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let mut b = [0u8; 8192];
                while let Ok(n @ 1..) = p.read(&mut b) {
                    if tx.send((s, b[..n].to_vec())).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        let deadline = job.limits.wall_ms.map(|ms| Instant::now() + Duration::from_millis(ms));
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        // all of the job's processes, killed while its first is unreaped so their shared id is still theirs
        let pid = child.id();
        let kill = || unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
        let end = |child: &mut std::process::Child| {
            kill();
            drop(child.wait());
        };
        let mut killed = false;
        loop {
            if stop.load(Ordering::Relaxed) {
                end(&mut child);
                return Err("stopped".into());
            }
            let wait = deadline.map_or(STOP_CHECK, |d| d.saturating_duration_since(Instant::now()).min(STOP_CHECK));
            match rx.recv_timeout(wait) {
                Ok((s, b)) => {
                    out(s, &b);
                    (if s == Stream::Stdout { &mut stdout } else { &mut stderr }).extend(b);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) if deadline.is_some_and(|d| Instant::now() >= d) => {
                    end(&mut child);
                    return Err(format!("{program}: over its {} ms", job.limits.wall_ms.unwrap_or(0)));
                }
                // the job has ended but something it started holds its output open: that ends too, and a tick later
                // whatever still holds it is not the job's
                Err(mpsc::RecvTimeoutError::Timeout) if killed => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if ended(pid, false) {
                        kill();
                        killed = true;
                    }
                }
            }
        }
        // output closed, the job may still run: stop and time still reach it until it ends
        while !ended(pid, false) {
            if stop.load(Ordering::Relaxed) {
                end(&mut child);
                return Err("stopped".into());
            }
            if deadline.is_some_and(|d| Instant::now() >= d) {
                end(&mut child);
                return Err(format!("{program}: over its {} ms", job.limits.wall_ms.unwrap_or(0)));
            }
            std::thread::sleep(STOP_CHECK);
        }
        // what it left running ends with it, before it is reaped
        kill();
        let status = child.wait().map_err(|e| e.to_string())?;
        let code = status.code().unwrap_or_else(|| 128 + std::os::unix::process::ExitStatusExt::signal(&status).unwrap_or(0));
        Ok((code, stdout, stderr))
    }
}

/// Whether process `pid` has ended, left unreaped; `block` waits until it has.
fn ended(pid: u32, block: bool) -> bool {
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let flags = libc::WEXITED | libc::WNOWAIT | if block { 0 } else { libc::WNOHANG };
    let r = unsafe { libc::waitid(libc::P_PID, pid, &mut info, flags) };
    r == 0 && unsafe { info.si_pid() } != 0
}

impl ComputeDriver for LocalFallbackDriver {
    fn upload(&mut self, blobs: &[Vec<u8>]) -> Result<Vec<Digest>, String> {
        blobs.iter().map(|b| self.put(b)).collect()
    }

    fn download(&mut self, d: &Digest) -> Result<Vec<u8>, String> {
        self.get(d)
    }

    fn run(&mut self, job: &JobSpec, out: &mut dyn FnMut(Stream, &[u8]), stop: &AtomicBool) -> Result<JobResult, String> {
        if job.rootfs_digest.is_some() {
            return Err("this host's driver runs on the host's own root; a root filesystem wants a worker".into());
        }
        if job.limits.cpus.is_some() || job.limits.memory_bytes.is_some() {
            return Err("this host's driver keeps no CPU or memory limit; a job with one wants a worker".into());
        }
        let start = Instant::now();
        let (command, action, key) = crate::action(job);
        self.put(&command)?;
        self.put(&action)?;
        let cached = self.root.join("ac").join(&key.hash);
        if let Ok(b) = std::fs::read(&cached) {
            let mut r = result(&b)?;
            (r.cached, r.wall_ms) = (true, start.elapsed().as_millis() as u64);
            return Ok(r);
        }
        static RUNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = self.root.join("work").join(format!("{}-{}-{}", &key.hash[..16], std::process::id(), RUNS.fetch_add(1, Ordering::Relaxed)));
        drop(std::fs::remove_dir_all(&dir));
        materialize(&job.input_tree_digest, &mut |d| self.get(d), &dir)?;
        std::fs::create_dir_all(dir.join(crate::reapi::OUT)).map_err(|e| e.to_string())?;
        let ran = self.spawn(job, &dir, out, stop);
        let outputs = ran.as_ref().ok().map(|_| collect(&dir.join(crate::reapi::OUT)));
        drop(std::fs::remove_dir_all(&dir));
        let (exit_code, stdout, stderr) = ran?;
        let files = outputs.expect("ran")?;
        let output_tree_digest = if files.is_empty() {
            None
        } else {
            let (root, blobs) = tree(&files)?;
            self.upload(&blobs)?;
            Some(root)
        };
        let r = JobResult { exit_code, stdout_digest: self.put(&stdout)?, stderr_digest: self.put(&stderr)?, output_tree_digest, cached: false, wall_ms: start.elapsed().as_millis() as u64 };
        // only a success is kept, as a remote cache keeps it
        if exit_code == 0 {
            std::fs::write(&cached, encode_result(&r)).map_err(|e| format!("{}: {e}", cached.display()))?;
        }
        Ok(r)
    }
}
