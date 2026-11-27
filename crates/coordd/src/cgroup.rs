//! Processes and identity through cgroups (design §6-7). Each agent's processes live in `ROOT/agents/NAME`, the
//! terminal holders (`coord-pty`) in `ROOT/pty`, the project's view sources in `ROOT/project`, an account's sign-in
//! (Claude Code's or opencode's own program) in `ROOT/signin/ID`. A caller's cgroup names it: an agent; a holder or
//! what it started (not yet claimed by `coord run`); the project; a sign-in; anything else is the owner side (the
//! box's own shells, `box.sh sh` from the host).

use std::fs;
use std::os::fd::{FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};

/// Who a caller is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Caller {
    Owner,
    Agent(String),
    /// A terminal holder or what it started, not yet an agent: it may only claim a registered agent with `run`.
    Unclaimed,
    Project,
    /// An account's sign-in program or what it started (`signin/ID`): Claude Code's or opencode's, an agy account's
    /// sign-in and checks (`signin/agy-ID`), or the t3code server of one t3code signed in (`signin/t3-ID`). It is not the
    /// owner: coordd answers it nothing. The coord commands of that server's own agents are those agents instead
    /// (`accounts::agy::declares`).
    SignIn,
}

pub fn root() -> PathBuf {
    PathBuf::from(std::env::var("COORDD_CGROUP").unwrap_or_else(|_| "/sys/fs/cgroup/swarm".into()))
}

/// The cgroup path of ROOT as it appears in /proc/PID/cgroup ("/swarm").
pub(crate) fn root_rel() -> String {
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

/// Cap ROOT at [`WORKLOAD_SHARE`] of the memory coordd's machine has, so that when the workload runs the machine short
/// the kernel's OOM killer acts inside ROOT, on its biggest process, not machine-wide. coordd runs in ROOT's leaf
/// `coordd` (in an sbx box, outside ROOT). The machine's memory is the smaller of its RAM and the limit of every cgroup
/// above coordd (a container's is at the top of its cgroup namespace).
fn limit_workload() {
    let ram = fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|m| m.lines().find_map(|l| l.strip_prefix("MemTotal:")?.trim().strip_suffix("kB")?.trim().parse::<u64>().ok()))
        .map(|kb| kb * 1024);
    let own = fs::read_to_string("/proc/self/cgroup").ok().and_then(|t| t.lines().find_map(|l| l.strip_prefix("0::").map(String::from)));
    let container = own.and_then(|p| outside_root(Path::new("/sys/fs/cgroup"), &p, &root_rel()));
    let Some(total) = [ram, container].into_iter().flatten().min() else { return eprintln!("coordd: no memory total; workload not capped") };
    let cap = (total as f64 * WORKLOAD_SHARE) as u64;
    match fs::write(root().join("memory.max"), cap.to_string()) {
        Ok(()) => eprintln!("coordd: workload capped at {} MB of {} MB", cap >> 20, total >> 20),
        Err(e) => eprintln!("coordd: workload not capped ({}/memory.max: {e})", root().display()),
    }
}

/// The project's own limits on all of ROOT (its header's `memory`, `cpu`, `pids`; `Project::whole_limits`), after
/// [`limit_workload`]: its memory only below the cap that wrote, never above it. Each said in the log; what is not
/// given stays as it is.
pub fn limit_whole(l: &crate::sandbox::Limits) {
    let mut l = l.clone();
    if let Some(m) = &l.memory {
        let cap = fs::read_to_string(root().join("memory.max")).ok().and_then(|c| c.trim().parse::<u64>().ok());
        match (crate::sandbox::bytes(m), cap) {
            (None, _) => {
                eprintln!("coordd: the project's memory = {m:?} is no size; not set");
                l.memory = None;
            }
            (Some(b), Some(c)) if b > c => {
                eprintln!("coordd: the project's memory ({} MB) is above the workload's cap ({} MB); the cap stays", b >> 20, c >> 20);
                l.memory = None;
            }
            _ => {}
        }
    }
    for (f, v) in l.files() {
        match fs::write(root().join(f), &v) {
            Ok(()) => eprintln!("coordd: the project's {f} = {v}"),
            Err(e) => eprintln!("coordd: the project's {f} = {v} not set ({}/{f}: {e})", root().display()),
        }
    }
}

/// The least memory limit above coordd in TREE that coordd did not write itself: from its cgroup OWN, with ROOT
/// (ROOT_REL) and what is inside it left out, since ROOT's memory.max is the cap the last start wrote; read with it, each
/// start would cap at 0.9 of the last.
fn outside_root(tree: &Path, own: &str, root_rel: &str) -> Option<u64> {
    let root_rel = root_rel.trim_end_matches('/');
    let inside = !root_rel.is_empty() && (own == root_rel || own.starts_with(&format!("{root_rel}/")));
    if !inside {
        return above(tree, own);
    }
    match root_rel.rsplit_once('/') {
        Some((parent, _)) => above(tree, if parent.is_empty() { "/" } else { parent }),
        None => None,
    }
}

/// The smallest `memory.max` of cgroup REL (as /proc/PID/cgroup names it, "/swarm/coordd") and every cgroup above it,
/// under the cgroup tree's mount TREE; none when all say `max`.
fn above(tree: &Path, rel: &str) -> Option<u64> {
    let mut p = tree.join(rel.trim_start_matches('/'));
    let mut least: Option<u64> = None;
    loop {
        if let Some(m) = fs::read_to_string(p.join("memory.max")).ok().and_then(|m| m.trim().parse::<u64>().ok()) {
            least = Some(least.map_or(m, |l| l.min(m)));
        }
        if p == tree || !p.pop() || !p.starts_with(tree) {
            return least;
        }
    }
}

/// PID's cgroup, as the `0::` line of `/proc/PID/cgroup` names it; empty when that cannot be read.
pub fn path_of(pid: i32) -> String {
    let text = fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap_or_default();
    text.lines().find_map(|l| l.strip_prefix("0::")).unwrap_or("").to_string()
}

/// Who calls from cgroup PATH ([`path_of`]), by its place under ROOT: `agents/NAME/...` that agent, `pty` unclaimed,
/// `project` the project's sources, `signin` an account's sign-in; anything else [`Caller::Owner`]: `launch`, `coordd` and
/// anything outside ROOT. The one exception, an agy agent's declared name, is serve's (main.rs `declared_caller`).
pub fn identify_path(path: &str) -> Caller {
    let base = root_rel();
    let Some(rest) = path.strip_prefix(&base) else { return Caller::Owner };
    let rest = rest.trim_start_matches('/');
    if let Some(name) = rest.strip_prefix("agents/") {
        return Caller::Agent(name.split('/').next().unwrap_or(name).to_string());
    }
    match rest.split('/').next() {
        Some("pty") => Caller::Unclaimed,
        Some("project") => Caller::Project,
        Some("signin") => Caller::SignIn,
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

/// End every process left in agent NAME's subtree (its leaf and its job cgroups) once its session has ended: a process
/// left behind would keep the old view open at a move, and a layer's end would wait on it. What it ended, one
/// `PID COMMAND` each, for the move line; none, nothing written.
#[allow(dead_code)] // called once a session has ended, by the moves still to come
pub fn kill_left(name: &str) -> Vec<String> {
    kill_in(&root().join("agents").join(name), &procs(name))
}

/// `cgroup.kill` (SIGKILL to the whole subtree, the kernel's own) on DIR, which holds PIDS: what they were.
fn kill_in(dir: &Path, pids: &[i32]) -> Vec<String> {
    if pids.is_empty() {
        return Vec::new();
    }
    let what = pids
        .iter()
        .map(|p| {
            let cmd = fs::read(format!("/proc/{p}/cmdline")).map(|b| b.split(|c| *c == 0).filter(|w| !w.is_empty()).map(|w| String::from_utf8_lossy(w).to_string()).collect::<Vec<_>>().join(" ")).unwrap_or_default();
            format!("{p} {cmd}").trim_end().to_string()
        })
        .collect();
    if let Err(e) = fs::write(dir.join("cgroup.kill"), "1") {
        eprintln!("coordd: {}: cgroup.kill: {e}", dir.display());
    }
    what
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

#[cfg(test)]
mod tests {
    use super::*;

    /// An account's sign-in program (`signin/ID`) is no owner: opencode's is fetched from outside, and Claude Code's sits in
    /// the same cgroup.
    #[test]
    fn a_sign_in_is_not_the_owner() {
        let base = root_rel();
        assert_eq!(identify_path(&format!("{base}/signin/go-plan")), Caller::SignIn);
        assert_eq!(identify_path(&format!("{base}/signin/work/x")), Caller::SignIn);
        assert_eq!(identify_path(&format!("{base}/project")), Caller::Project);
        assert_eq!(identify_path(&format!("{base}/agents/a/main")), Caller::Agent("a".into()));
    }

    /// The workload's total is the least limit above coordd: a container's limit sits at the top of its cgroup
    /// namespace, coordd's own leaf says `max` (a docker box, box.sh coordd_up_agent).
    #[test]
    fn the_memory_above_coordd_is_the_least_limit_of_its_ancestors() {
        let t = std::env::temp_dir().join(format!("coordd-cgroup-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        fs::create_dir_all(t.join("swarm/coordd")).unwrap();
        fs::write(t.join("memory.max"), "42949672960\n").unwrap();
        fs::write(t.join("swarm/memory.max"), "max\n").unwrap();
        fs::write(t.join("swarm/coordd/memory.max"), "max\n").unwrap();
        assert_eq!(above(&t, "/swarm/coordd"), Some(42949672960));
        fs::write(t.join("swarm/memory.max"), "1000\n").unwrap();
        assert_eq!(above(&t, "/swarm/coordd"), Some(1000));
        fs::write(t.join("memory.max"), "max\n").unwrap();
        fs::write(t.join("swarm/memory.max"), "max\n").unwrap();
        assert_eq!(above(&t, "/swarm/coordd"), None);
        assert_eq!(above(&t, "/"), None);
        fs::remove_dir_all(&t).unwrap();
    }

    /// ROOT's own memory.max is coordd's last cap, not the machine's: it is left out, so a restart does not cap at 0.9 of
    /// the cap before.
    #[test]
    fn the_cap_is_not_taken_from_the_cap_before() {
        let t = std::env::temp_dir().join(format!("coordd-cap-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        fs::create_dir_all(t.join("app/coordd-x.service/coordd")).unwrap();
        fs::write(t.join("memory.max"), "max\n").unwrap();
        fs::write(t.join("app/memory.max"), "42949672960\n").unwrap();
        fs::write(t.join("app/coordd-x.service/memory.max"), "28179275776\n").unwrap();
        fs::write(t.join("app/coordd-x.service/coordd/memory.max"), "max\n").unwrap();
        assert_eq!(above(&t, "/app/coordd-x.service/coordd"), Some(28179275776), "the old reading: the cap before");
        assert_eq!(outside_root(&t, "/app/coordd-x.service/coordd", "/app/coordd-x.service"), Some(42949672960));
        // coordd outside ROOT (an sbx box): its own ancestors, as before
        assert_eq!(outside_root(&t, "/app", "/app/coordd-x.service"), Some(42949672960));
        // ROOT at the top: nothing above it
        assert_eq!(outside_root(&t, "/swarm/coordd", "/swarm"), None);
        fs::remove_dir_all(&t).unwrap();
    }

    /// What is left in an agent's subtree is ended by its cgroup.kill, and named; nothing left, nothing written.
    #[test]
    fn what_an_agent_left_is_killed_and_named() {
        let t = std::env::temp_dir().join(format!("coordd-kill-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        fs::create_dir_all(&t).unwrap();
        assert!(kill_in(&t, &[]).is_empty());
        assert!(!t.join("cgroup.kill").exists(), "nothing left: nothing written");
        let me = std::process::id() as i32;
        let what = kill_in(&t, &[me]);
        assert_eq!(what.len(), 1);
        assert!(what[0].starts_with(&format!("{me} ")) && what[0].contains("coordd"), "{what:?}");
        assert_eq!(fs::read_to_string(t.join("cgroup.kill")).unwrap(), "1");
        fs::remove_dir_all(&t).unwrap();
    }
}
