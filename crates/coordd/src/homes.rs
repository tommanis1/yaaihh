//! Each agent's home (`agents/NAME/home`) holds links into its project, kept up by coordd (the owner's walk item 25):
//! `project/repos/R` to each of the project's repositories and `project/agents/OTHER` to every other agent's home.
//! Only links are made or removed, and only inside those two folders; anything else found there is left and said.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// One reconcile at a time: a sweep and a change may be sent close together.
static LOCK: Mutex<()> = Mutex::new(());

/// How often the links are gone over with nothing changed, so one removed by hand comes back.
const SWEEP: std::time::Duration = std::time::Duration::from_secs(600);

/// What the links follow: the live agents by name, and the project's repositories by name and path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Want {
    pub agents: Vec<String>,
    pub repos: Vec<(String, String)>,
}

/// What one reconcile did: links made or pointed again, stale links removed, and what it left as it found it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub made: usize,
    pub removed: usize,
    pub left: Vec<String>,
}

/// Agent NAME's home under COORD (roster gives it to the agent as AGENT_HOME).
pub fn home(coord: &str, name: &str) -> PathBuf {
    Path::new(coord).join("agents").join(name).join("home")
}

/// Agent NAME's home made ready to start in: it, `sources/` for what it saves from the web, and `project/` for the
/// links [`reconcile`] keeps there.
pub fn made(coord: &str, name: &str) -> Result<String, String> {
    let h = home(coord, name);
    for d in [h.join("sources"), h.join("project/repos"), h.join("project/agents")] {
        std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    Ok(h.display().to_string())
}

/// A name a link may carry: one path part, not hidden.
fn plain(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('.') && !name.contains('/') && !name.contains('\0')
}

/// What CORE's links should be now: its agents not ended and not program tabs, and the project's repositories (its
/// own folder as `repo` when it names no `repos`).
pub fn want(core: &crate::core::Core) -> Want {
    let agents = core.agents.values().filter(|a| a.ended.is_none() && a.kind != "program" && plain(&a.name)).map(|a| a.name.clone()).collect();
    let mut repos: Vec<(String, String)> = core.project.as_ref().map(|p| p.repos()).unwrap_or_default();
    if repos.is_empty() {
        repos.extend(core.project.as_ref().and_then(|p| p.repo()).map(|r| ("repo".to_string(), r)));
    }
    repos.retain(|(n, _)| plain(n));
    Want { agents, repos }
}

/// DIR is a real folder (made if missing, PARENT first); a link or a file in its place is said and refused.
fn real_dir(dir: &Path, left: &mut Vec<String>) -> bool {
    match std::fs::symlink_metadata(dir) {
        Ok(m) if m.is_dir() => true,
        Ok(_) => {
            left.push(format!("{} is not a folder: its links are not kept", dir.display()));
            false
        }
        Err(_) => match std::fs::create_dir(dir) {
            Ok(()) => true,
            Err(e) => {
                left.push(format!("{}: {e}", dir.display()));
                false
            }
        },
    }
}

/// Folder DIR holds exactly the links WANT (name, target): each missing one made, one pointing elsewhere pointed
/// again, a link of no wanted name removed. A real file or folder is never touched.
fn keep(dir: &Path, want: &[(String, PathBuf)], r: &mut Report) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let path = e.path();
            let Ok(m) = std::fs::symlink_metadata(&path) else { continue };
            let wanted = want.iter().find(|(n, _)| *n == name);
            if !m.file_type().is_symlink() {
                if wanted.is_some() {
                    r.left.push(format!("{} is not a link: left as it is", path.display()));
                }
                continue;
            }
            let right = wanted.is_some_and(|(_, to)| std::fs::read_link(&path).is_ok_and(|t| t == *to));
            if !right {
                match std::fs::remove_file(&path) {
                    Ok(()) if wanted.is_none() => r.removed += 1,
                    Ok(()) => {}
                    Err(e) => r.left.push(format!("{}: {e}", path.display())),
                }
            }
        }
    }
    for (name, to) in want {
        let at = dir.join(name);
        if std::fs::symlink_metadata(&at).is_ok() {
            continue;
        }
        match std::os::unix::fs::symlink(to, &at) {
            Ok(()) => r.made += 1,
            Err(e) => r.left.push(format!("{}: {e}", at.display())),
        }
    }
}

/// Every agent's `home/project` under COORD made to hold the links WANT says; an agent's folder that WANT does not
/// name (ended, renamed away) keeps no links there.
pub fn reconcile(coord: &str, want: &Want) -> Report {
    let _one = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut r = Report::default();
    let repos: Vec<(String, PathBuf)> = want.repos.iter().map(|(n, p)| (n.clone(), PathBuf::from(p))).collect();
    let mut names: Vec<String> = std::fs::read_dir(Path::new(coord).join("agents")).into_iter().flatten().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| plain(n)).collect();
    names.extend(want.agents.iter().filter(|a| !names.contains(a)).cloned().collect::<Vec<_>>());
    for name in names {
        let live = want.agents.contains(&name);
        let home = home(coord, &name);
        let project = home.join("project");
        if !live {
            // an agent gone from the roster: only its links go, its home stays
            for part in ["repos", "agents"] {
                if std::fs::symlink_metadata(project.join(part)).is_ok_and(|m| m.is_dir()) {
                    keep(&project.join(part), &[], &mut r);
                }
            }
            continue;
        }
        let agent_dir = Path::new(coord).join("agents").join(&name);
        if !(real_dir(&agent_dir, &mut r.left) && real_dir(&home, &mut r.left) && real_dir(&project, &mut r.left)) {
            continue;
        }
        real_dir(&home.join("sources"), &mut r.left);
        let others: Vec<(String, PathBuf)> = want.agents.iter().filter(|o| **o != name).map(|o| (o.clone(), self::home(coord, o))).collect();
        for (part, links) in [("repos", &repos), ("agents", &others)] {
            let dir = project.join(part);
            if real_dir(&dir, &mut r.left) {
                keep(&dir, links, &mut r);
            }
        }
    }
    r
}

/// The follow loop's keeper: reconciles off the lock when what the links follow changes, and every SWEEP.
#[derive(Default)]
pub struct Keeper {
    seen: Option<Want>,
    swept: Option<std::time::Instant>,
}

impl Keeper {
    /// Called each round with CORE: a reconcile sent to a blocking thread when due, its report in coordd's log.
    pub fn round(&mut self, core: &crate::core::Core) {
        let want = want(core);
        if self.seen.as_ref() == Some(&want) && self.swept.is_some_and(|t| t.elapsed() < SWEEP) {
            return;
        }
        (self.seen, self.swept) = (Some(want.clone()), Some(std::time::Instant::now()));
        let coord = core.coord.clone();
        tokio::task::spawn_blocking(move || {
            let r = reconcile(&coord, &want);
            if r.made + r.removed > 0 || !r.left.is_empty() {
                eprintln!("coordd: agents' homes: {} links made, {} stale removed{}", r.made, r.removed, r.left.iter().map(|l| format!("; {l}")).collect::<String>());
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("homes-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("coord/agents")).unwrap();
        for r in ["tovrin", "paper"] {
            std::fs::create_dir_all(d.join(r)).unwrap();
        }
        d
    }

    fn want(d: &Path, agents: &[&str], repos: &[&str]) -> Want {
        Want { agents: agents.iter().map(|a| a.to_string()).collect(), repos: repos.iter().map(|r| (r.to_string(), d.join(r).display().to_string())).collect() }
    }

    fn links(dir: &Path) -> Vec<(String, PathBuf)> {
        let mut v: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().map(|e| (e.file_name().to_string_lossy().into_owned(), std::fs::read_link(e.path()).unwrap())).collect();
        v.sort();
        v
    }

    #[test]
    fn add_makes_each_agents_links() {
        let d = tmp("add");
        let c = d.join("coord").display().to_string();
        let r = reconcile(&c, &want(&d, &["ann", "bob"], &["tovrin", "paper"]));
        assert_eq!((r.made, r.removed, r.left.len()), (6, 0, 0), "{r:?}");
        let ann = home(&c, "ann").join("project");
        assert_eq!(links(&ann.join("repos")), vec![("paper".into(), d.join("paper")), ("tovrin".into(), d.join("tovrin"))]);
        assert_eq!(links(&ann.join("agents")), vec![("bob".into(), home(&c, "bob"))]);
        assert!(ann.join("agents/bob/project/repos/tovrin").is_dir(), "an agent reads another's home through its link");
        // a third agent added: it gets every link, and each of the others one to it
        let r = reconcile(&c, &want(&d, &["ann", "bob", "cid"], &["tovrin", "paper"]));
        assert_eq!((r.made, r.removed), (2 + 2 + 2, 0), "{r:?}");
        assert_eq!(reconcile(&c, &want(&d, &["ann", "bob", "cid"], &["tovrin", "paper"])), Report::default(), "a second reconcile does nothing");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rename_and_remove_leave_no_stale_link() {
        let d = tmp("rename");
        let c = d.join("coord").display().to_string();
        reconcile(&c, &want(&d, &["ann", "bob", "cid"], &["tovrin"]));
        // coordd's rename moves the agent's folder (Core::rename)
        std::fs::rename(Path::new(&c).join("agents/bob"), Path::new(&c).join("agents/bea")).unwrap();
        let r = reconcile(&c, &want(&d, &["ann", "bea", "cid"], &["tovrin"]));
        assert_eq!((r.made, r.removed), (2, 2), "{r:?}");
        assert_eq!(links(&home(&c, "ann").join("project/agents")), vec![("bea".into(), home(&c, "bea")), ("cid".into(), home(&c, "cid"))]);
        assert_eq!(links(&home(&c, "bea").join("project/agents")), vec![("ann".into(), home(&c, "ann")), ("cid".into(), home(&c, "cid"))], "the renamed one's own links still hold");
        // cid ended: gone from everyone's links, and its own links go while its home and files stay
        std::fs::write(home(&c, "cid").join("notes.md"), "mine").unwrap();
        let r = reconcile(&c, &want(&d, &["ann", "bea"], &["tovrin"]));
        assert_eq!((r.made, r.removed), (0, 2 + 3), "{r:?}");
        assert_eq!(links(&home(&c, "ann").join("project/agents")), vec![("bea".into(), home(&c, "bea"))]);
        assert!(links(&home(&c, "cid").join("project/agents")).is_empty() && links(&home(&c, "cid").join("project/repos")).is_empty());
        assert_eq!(std::fs::read_to_string(home(&c, "cid").join("notes.md")).unwrap(), "mine");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn repo_update_points_again_and_drops_the_gone() {
        let d = tmp("repos");
        let c = d.join("coord").display().to_string();
        reconcile(&c, &want(&d, &["ann"], &["tovrin", "paper"]));
        // paper leaves `repos`; tovrin's folder moves (the same name, another path)
        std::fs::create_dir_all(d.join("tovrin2")).unwrap();
        let w = Want { agents: vec!["ann".into()], repos: vec![("tovrin".into(), d.join("tovrin2").display().to_string())] };
        let r = reconcile(&c, &w);
        assert_eq!((r.made, r.removed), (1, 1), "{r:?}");
        assert_eq!(links(&home(&c, "ann").join("project/repos")), vec![("tovrin".into(), d.join("tovrin2"))]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn real_files_and_outside_links_are_never_touched() {
        let d = tmp("real");
        let c = d.join("coord").display().to_string();
        let repos = home(&c, "ann").join("project/repos");
        std::fs::create_dir_all(&repos).unwrap();
        // a real folder where a link is wanted, a real file of no wanted name, a dangling link
        std::fs::create_dir(repos.join("tovrin")).unwrap();
        std::fs::write(repos.join("todo.txt"), "keep").unwrap();
        symlink(d.join("nowhere"), repos.join("old")).unwrap();
        // bob's `project` is a link out of his home: nothing is written through it
        std::fs::create_dir_all(home(&c, "bob")).unwrap();
        std::fs::create_dir_all(d.join("outside")).unwrap();
        symlink(d.join("outside"), home(&c, "bob").join("project")).unwrap();
        let r = reconcile(&c, &want(&d, &["ann", "bob"], &["tovrin"]));
        assert_eq!((r.made, r.removed), (1, 1), "ann's link to bob made, the dangling one removed: {r:?}");
        assert_eq!(r.left.len(), 2, "{r:?}");
        assert!(repos.join("tovrin").is_dir() && !repos.join("tovrin").is_symlink());
        assert_eq!(std::fs::read_to_string(repos.join("todo.txt")).unwrap(), "keep");
        assert!(std::fs::read_dir(d.join("outside")).unwrap().next().is_none(), "nothing written outside bob's home");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Through coordd itself: agents added, renamed and ended, and `repos` edited in the project file and read again.
    #[test]
    fn coordds_roster_and_project_drive_the_links() {
        use crate::core::{Core, Who};
        let d = tmp("core");
        let coord = d.join("coord");
        let files = coord.join("files");
        std::fs::create_dir_all(&files).unwrap();
        for r in ["tovrin", "paper"] {
            let o = std::process::Command::new("git").args(["init", "-q"]).current_dir(d.join(r)).output().unwrap();
            assert!(o.status.success());
        }
        let file = |repos: &str| std::fs::write(files.join("project.toml"), format!("[project]\nname = \"h\"\nrepos = {{ {repos} }}\n")).unwrap();
        file(&format!("tovrin = \"{}\"", d.join("tovrin").display()));
        std::fs::write(coord.join("project"), files.join("project.toml").display().to_string()).unwrap();
        let c = coord.display().to_string();
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let mut core = Core::load(&c, &base);
        core.project = crate::project::Project::load(&c).expect("the project file");
        for (n, k) in [("ann", "claude"), ("bob", "claude"), ("cid", "claude"), ("runner", "program")] {
            assert!(core.add(&Who::Owner, n, k, "m").ok, "{n}");
        }
        reconcile(&c, &super::want(&core));
        assert_eq!(links(&home(&c, "ann").join("project/agents")), vec![("bob".into(), home(&c, "bob")), ("cid".into(), home(&c, "cid"))], "no link to a program tab");
        assert_eq!(links(&home(&c, "ann").join("project/repos")), vec![("tovrin".into(), d.join("tovrin"))]);
        assert!(core.rename(&Who::Owner, "bob", "bea").ok);
        assert!(core.set_ended(&Who::Owner, "cid", false).ok);
        file(&format!("tovrin = \"{}\", paper = \"{}\"", d.join("tovrin").display(), d.join("paper").display()));
        core.project = crate::project::Project::load(&c).expect("the project file read again");
        reconcile(&c, &super::want(&core));
        assert_eq!(links(&home(&c, "ann").join("project/agents")), vec![("bea".into(), home(&c, "bea"))]);
        assert_eq!(links(&home(&c, "bea").join("project/repos")), vec![("paper".into(), d.join("paper")), ("tovrin".into(), d.join("tovrin"))]);
        assert!(links(&home(&c, "cid").join("project/agents")).is_empty(), "an ended agent keeps no links");
        let _ = std::fs::remove_dir_all(&d);
    }
}
