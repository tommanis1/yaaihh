//! A scratch project: a built coordd on an empty coord folder under a home of its own, stopped and removed on drop.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The project's id, and so its coord folder's name under the scratch home.
pub const ID: &str = "e2e";

/// The scratch projects this process started, for each scope's own unit name.
static RUNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The room a Unix socket's path has, its end included.
const SUN_PATH: usize = 108;

/// How long coordd has to make its socket.
const UP_WITHIN: Duration = Duration::from_secs(30);

/// The programs a scratch project runs: coordd, the coordc its pipe runs, and the agent-base coordd reads.
#[derive(Clone, Debug)]
pub struct Release {
    pub coordd: PathBuf,
    pub coordc: PathBuf,
    pub agent_base: PathBuf,
}

impl Release {
    /// A release folder as release.sh lays it out: coordd and coordc at its top, agent-base beside them.
    pub fn folder(dir: &Path) -> Release {
        Release { coordd: dir.join("coordd"), coordc: dir.join("coordc"), agent_base: dir.join("agent-base") }
    }
}

/// A running scratch coordd and the folders it was given.
pub struct Scratch {
    pub root: PathBuf,
    pub home: PathBuf,
    pub coord: PathBuf,
    release: Release,
    coordd: Child,
    unit: String,
}

impl Scratch {
    /// The coord folder `new-project host` makes, under ROOT (which must not exist), and coordd started on it in a
    /// delegated scope of its own, as host-coordd starts one; ready once its socket is there.
    pub fn up(release: &Release, root: &Path) -> Result<Scratch, String> {
        if root.exists() {
            return Err(format!("{} is there already: a scratch project starts on nothing", root.display()));
        }
        let home = root.join("home");
        let coord = home.join("agent-coord").join(ID);
        let sock = coord.join("coordd.sock");
        if sock.as_os_str().len() >= SUN_PATH {
            return Err(format!("{}: a socket's path is under {SUN_PATH} bytes; start it in a shorter folder", sock.display()));
        }
        for d in [coord.join("definition"), coord.join("work"), coord.join("claude-home"), root.join("run")] {
            std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        for d in [coord.join("claude-home"), root.join("run")] {
            std::fs::set_permissions(&d, std::os::unix::fs::PermissionsExt::from_mode(0o700)).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        let def = coord.join("definition/project.toml");
        write(&def, "[project]\nhost_relay = true\n")?;
        write(&coord.join("project"), &format!("{}\n", def.display()))?;
        write(&coord.join("repo"), &format!("{}\n", coord.join("work").display()))?;
        let base = release.agent_base.display().to_string();
        write(&root.join("projects.toml"), &format!("agent_base = \"{base}\"\n\n[[project]]\nname = \"{ID}\"\nid = \"{ID}\"\nboxed = false\n"))?;
        let unit = format!("yaaihh-e2e-{}-{}", std::process::id(), RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        let log = std::fs::File::create(root.join("coordd.log")).map_err(|e| e.to_string())?;
        // systemd-run keeps the caller's bus; coordd alone gets the scratch environment, and a cgroup below its scope's
        let mut env: Vec<String> = base_env(&home, &release.agent_base, &root.join("run")).into_iter().map(|(k, v)| format!("{k}={v}")).collect();
        env.extend([format!("COORD={}", coord.display()), format!("AGENT_BASE={base}"), format!("COORDD_SOCK={}", coord.join("coordd.sock").display())]);
        env.push(format!("COORDD_LIVE={}", root.join("run/live.json").display()));
        let child = Command::new("systemd-run")
            .args(["--user", "--scope", "--quiet", "-p", "Delegate=yes", "--unit", &unit, "env", "-i"])
            .args(&env)
            .args(["sh", "-c", r#"COORDD_CGROUP=/sys/fs/cgroup$(sed -n "s/^0:://p" /proc/self/cgroup)/swarm exec "$0""#])
            .arg(&release.coordd)
            .stdin(Stdio::null())
            .stdout(log.try_clone().map_err(|e| e.to_string())?)
            .stderr(log)
            .spawn()
            .map_err(|e| format!("systemd-run: {e}"))?;
        let mut s = Scratch { root: root.into(), home, coord, release: release.clone(), coordd: child, unit };
        let began = Instant::now();
        while !s.coord.join("coordd.sock").exists() {
            if let Ok(Some(st)) = s.coordd.try_wait() {
                return Err(format!("coordd ended ({st}) before its socket: {}", s.log_tail()));
            }
            if began.elapsed() > UP_WITHIN {
                return Err(format!("no socket in {} s: {}", UP_WITHIN.as_secs(), s.log_tail()));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(s)
    }

    /// A scratch project in a folder of its own under the temp folder, short enough for coordd's socket.
    pub fn up_in_temp(release: &Release) -> Result<Scratch, String> {
        let n = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Scratch::up(release, &std::env::temp_dir().join(format!("yaaihh-e2e-{}-{n}", std::process::id())))
    }

    /// The window's config: this project alone.
    pub fn config(&self) -> PathBuf {
        self.root.join("projects.toml")
    }

    /// What the window and coordc run with: the scratch home and this coord, coordc first on PATH.
    pub fn env(&self) -> Vec<(String, String)> {
        let mut env = base_env(&self.home, &self.release.agent_base, &self.root.join("run"));
        let bin = self.release.coordc.parent().map(|p| p.display().to_string()).unwrap_or_default();
        if let Some(p) = env.iter_mut().find(|(k, _)| k == "PATH") {
            p.1 = format!("{bin}:{}", p.1);
        }
        env.push(("COORD".into(), self.coord.display().to_string()));
        env.push(("COORDD_SOCK".into(), self.coord.join("coordd.sock").display().to_string()));
        env
    }

    /// coordc ARGS as the project's owner side, its stdout on success.
    pub fn coordc(&self, args: &[&str]) -> Result<String, String> {
        let out = Command::new(&self.release.coordc).args(args).env_clear().envs(self.env()).output().map_err(|e| format!("coordc: {e}"))?;
        let said = String::from_utf8_lossy(&out.stdout).into_owned();
        if out.status.success() { Ok(said) } else { Err(format!("coordc {}: {}{}", args.join(" "), said, String::from_utf8_lossy(&out.stderr))) }
    }

    /// coordd's log as it stands.
    pub fn log(&self) -> String {
        std::fs::read_to_string(self.root.join("coordd.log")).unwrap_or_default()
    }

    fn log_tail(&self) -> String {
        let l = self.log();
        l.lines().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" / ")
    }
}

impl Drop for Scratch {
    /// coordd killed, its scope stopped, and everything under the root removed.
    fn drop(&mut self) {
        let _ = self.coordd.kill();
        let _ = self.coordd.wait();
        let _ = Command::new("systemctl").args(["--user", "stop", &format!("{}.scope", self.unit)]).stderr(Stdio::null()).status();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// HOME the scratch home, PATH agent-base's bin and the system's, XDG_RUNTIME_DIR a folder of the run's.
fn base_env(home: &Path, agent_base: &Path, run: &Path) -> Vec<(String, String)> {
    vec![
        ("HOME".into(), home.display().to_string()),
        ("PATH".into(), format!("{}/bin:/usr/bin:/bin", agent_base.display())),
        ("XDG_RUNTIME_DIR".into(), run.display().to_string()),
        ("LANG".into(), "C.UTF-8".into()),
    ]
}

fn write(p: &Path, text: &str) -> Result<(), String> {
    std::fs::write(p, text).map_err(|e| format!("{}: {e}", p.display()))
}
