//! Processes and identity through cgroups (design §6-7). Each agent's processes live in `ROOT/agents/NAME`, the
//! terminal holders (`coord-pty`) in `ROOT/pty`, the project's view sources in `ROOT/project`. A caller's cgroup names
//! it: an agent; a holder or what it started (not yet claimed by `coord run`); the project; anything else is the
//! owner side (the box's own shells, `box.sh sh` from the host).

use std::fs;
use std::os::fd::{FromRawFd, OwnedFd};
use std::path::PathBuf;

/// Who a caller is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Caller {
    Owner,
    Agent(String),
    /// A terminal holder or what it started, not yet an agent: it may only claim a registered agent with `run`.
    Unclaimed,
    Project,
}

pub fn root() -> PathBuf {
    PathBuf::from(std::env::var("COORDD_CGROUP").unwrap_or_else(|_| "/sys/fs/cgroup/swarm".into()))
}

/// The cgroup path of ROOT as it appears in /proc/PID/cgroup ("/swarm").
fn root_rel() -> String {
    let root = root();
    let rel = root.strip_prefix("/sys/fs/cgroup").map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    format!("/{}", rel.trim_start_matches('/'))
}

pub fn setup() -> std::io::Result<()> {
    for d in ["agents", "pty", "project"] {
        fs::create_dir_all(root().join(d))?;
    }
    // memory and pids are counted per agent (yaaihh shows them; limits later) only when enabled down to its cgroup;
    // ROOT and ROOT/agents hold no processes themselves, as cgroup v2 requires of a cgroup that enables controllers
    for d in [root(), root().join("agents")] {
        for c in ["+cpu", "+memory", "+pids"] {
            let _ = fs::write(d.join("cgroup.subtree_control"), c);
        }
    }
    limit_workload();
    Ok(())
}

/// The share of the machine's memory that the agents, their jobs, the terminal holders and the view sources (all of
/// ROOT) may use together; the rest stays for coordd, yaaihh's link and the system.
const WORKLOAD_SHARE: f64 = 0.9;

/// Cap ROOT at [`WORKLOAD_SHARE`] of the memory coordd's machine has (owner 8 Nov): when the workload together runs
/// the machine short, the kernel's OOM killer acts inside ROOT, on its biggest process (an agent or a job), not
/// machine-wide. In a box coordd runs outside ROOT; on the host it runs in ROOT's leaf `coordd` (host-coordd), where
/// it is the process killed only when it is itself the one that took the memory, as it is machine-wide. The
/// machine's memory is the smaller of its RAM and the limit of the cgroup coordd runs in (a container's).
fn limit_workload() {
    let ram = fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|m| m.lines().find_map(|l| l.strip_prefix("MemTotal:")?.trim().strip_suffix("kB")?.trim().parse::<u64>().ok()))
        .map(|kb| kb * 1024);
    let own = fs::read_to_string("/proc/self/cgroup").ok().and_then(|t| t.lines().find_map(|l| l.strip_prefix("0::").map(String::from)));
    let container = own.and_then(|p| fs::read_to_string(format!("/sys/fs/cgroup{p}/memory.max")).ok()).and_then(|m| m.trim().parse::<u64>().ok());
    let Some(total) = [ram, container].into_iter().flatten().min() else { return eprintln!("coordd: no memory total; workload not capped") };
    let cap = (total as f64 * WORKLOAD_SHARE) as u64;
    match fs::write(root().join("memory.max"), cap.to_string()) {
        Ok(()) => eprintln!("coordd: workload capped at {} MB of {} MB", cap >> 20, total >> 20),
        Err(e) => eprintln!("coordd: workload not capped ({}/memory.max: {e})", root().display()),
    }
}

pub fn identify(pid: i32) -> Caller {
    let text = fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap_or_default();
    let path = text.lines().find_map(|l| l.strip_prefix("0::")).unwrap_or("").to_string();
    let base = root_rel();
    let Some(rest) = path.strip_prefix(&base) else { return Caller::Owner };
    let rest = rest.trim_start_matches('/');
    if let Some(name) = rest.strip_prefix("agents/") {
        return Caller::Agent(name.split('/').next().unwrap_or(name).to_string());
    }
    match rest.split('/').next() {
        Some("pty") => Caller::Unclaimed,
        Some("project") => Caller::Project,
        _ => Caller::Owner,
    }
}

/// Move PID into cgroup REL (under ROOT), creating it.
pub fn place(pid: i32, rel: &str) -> std::io::Result<()> {
    let dir = root().join(rel);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("cgroup.procs"), pid.to_string())
}

/// The leaf an agent's own processes go in: `agents/NAME/main`, inside the agent's delegated subtree.
pub fn agent_cgroup(name: &str) -> String {
    format!("agents/{name}/main")
}

/// Hand `agents/NAME` to the agent user (cgroup v2 delegation, as systemd's Delegate=yes): with cpu, memory and pids
/// enabled below it and its directory, `cgroup.procs`, `cgroup.subtree_control` and `cgroup.threads` the agent
/// user's, the agent makes memory-limited job cgroups beside its leaf (`agents/NAME/run-X`) and moves its own
/// processes into them, with no root. A move needs write access to the common ancestor's `cgroup.procs` too, and
/// `agents/` stays root's, so no agent can move a process into another agent's subtree (all agents share one uid;
/// the cgroup is what names a caller). Done before any process is in the subtree (cgroup v2 keeps processes out of a

/// Remove agent NAME's empty cgroups (its leaf, its jobs', its own), as after a rename: a cgroup still holding a
/// process stays.
pub fn forget(name: &str) {
    let dir = root().join("agents").join(name);
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten().filter(|e| e.path().is_dir()) {
            let _ = std::fs::remove_dir(e.path());
        }
    }
    let _ = std::fs::remove_dir(&dir);
}

/// cgroup that enables controllers); in a box without the `agent` user (coordd on the host) the subtree stays root's.
pub fn delegate(name: &str) -> std::io::Result<()> {
    let dir = root().join("agents").join(name);
    fs::create_dir_all(&dir)?;
    for c in ["+cpu", "+memory", "+pids"] {
        let _ = fs::write(dir.join("cgroup.subtree_control"), c);
    }
    let Some((uid, gid)) = agent_user() else { return Ok(()) };
    for f in ["", "cgroup.procs", "cgroup.subtree_control", "cgroup.threads"] {
        std::os::unix::fs::chown(dir.join(f), Some(uid), Some(gid))?;
    }
    Ok(())
}

/// The box's `agent` user (uid, gid), the one every agent runs as.
fn agent_user() -> Option<(u32, u32)> {
    let name = std::ffi::CString::new("agent").ok()?;
    let pw = unsafe { libc::getpwnam(name.as_ptr()) };
    (!pw.is_null()).then(|| unsafe { ((*pw).pw_uid, (*pw).pw_gid) })
}

/// The processes still in an agent's subtree (its leaf and the job cgroups it made).
pub fn procs(name: &str) -> Vec<i32> {
    fn walk(dir: &std::path::Path, out: &mut Vec<i32>) {
        out.extend(fs::read_to_string(dir.join("cgroup.procs")).unwrap_or_default().lines().filter_map(|l| l.trim().parse::<i32>().ok()));
        for e in fs::read_dir(dir).into_iter().flatten().flatten() {
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                walk(&e.path(), out);
            }
        }
    }
    let mut out = Vec::new();
    walk(&root().join("agents").join(name), &mut out);
    out
}

/// Agents whose cgroup still holds processes (after a coordd restart: they kept running).
pub fn live_agents() -> Vec<(String, Vec<i32>)> {
    let Ok(dir) = fs::read_dir(root().join("agents")) else { return Vec::new() };
    dir.flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .map(|name| { let p = procs(&name); (name, p) })
        .filter(|(_, p)| !p.is_empty())
        .collect()
}

/// Memory (MB) and CPU time (s) of an agent's cgroup.
pub fn usage(name: &str) -> (Option<u64>, Option<f64>) {
    let dir = root().join("agents").join(name);
    let mem = fs::read_to_string(dir.join("memory.current")).ok().and_then(|s| s.trim().parse::<u64>().ok()).map(|b| b >> 20);
    let cpu = fs::read_to_string(dir.join("cpu.stat")).ok().and_then(|s| {
        s.lines().find_map(|l| l.strip_prefix("usage_usec ")).and_then(|v| v.trim().parse::<f64>().ok()).map(|u| u / 1e6)
    });
    (mem, cpu)
}

/// A pidfd for PID: readable once the process has exited, however it died.
pub fn pidfd(pid: i32) -> std::io::Result<OwnedFd> {
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { OwnedFd::from_raw_fd(fd as i32) })
}
