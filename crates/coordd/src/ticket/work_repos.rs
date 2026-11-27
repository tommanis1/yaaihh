//! The repositories the kept graph reads git from (paper-research N-1446 p1455): those the folders where agents work
//! sit in, as the record names them, with no list kept by hand. Each `edit` act's files' folders and each job's
//! worktree (`wt`, on the record's job lines and on the runner's files from before the record had them) are placed
//! once by git (`rev-parse --show-toplevel` and the common git folder), so a repository is read once whatever worktree
//! of it the folders are in, and refreshed when its refs move ([`super::git_read`]). The project's own `repos` come
//! first, under their names. A folder that is gone, or whose repository git no longer reads, is named
//! ([`GraphGit::gone`]), never put under another repository. Of those read, a repository's commits come in once it
//! counts (p1476): the project's, or one with a commit an agent here wrote or a job from here ran on. With them, where
//! each was made: the worktree whose own HEAD reflog first has git make it ([`head_reflogs`], p1530, p1554).

use super::GraphGit;
use docket_machine::graph_git::{Commit, Job, MadeIn, Moved, Packages};
use docket_machine::Line;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Component, Path, PathBuf};

/// Where a folder sits for git.
#[derive(Clone, Debug, PartialEq)]
pub enum Placed {
    /// In a repository: the top of its worktree and the repository's common git folder, as git gives them.
    In { top: String, common: String },
    /// In none, or in a place never read (`files::Places::never_served`), where git is not asked.
    Nowhere,
    /// Gone: the topmost of its folders that is no longer there, or the folder holding a `.git` link git reads no
    /// repository from (a worktree whose repository took its record of it away).
    Gone(String),
}

/// P as the record names it, when it is an absolute path, with its `.` and `..` taken out.
fn normal(p: &str) -> Option<PathBuf> {
    let p = Path::new(p);
    if !p.is_absolute() {
        return None;
    }
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    Some(out)
}

/// A commit's first 8 characters, as a job names it; none for an empty one.
fn short(hash: &str) -> Option<String> {
    Some(hash.get(..8).unwrap_or(hash).to_string()).filter(|h| !h.is_empty())
}

/// A job's worktree WT as a folder ([`normal`]).
fn worktree(wt: &str) -> Option<String> {
    normal(wt).map(|w| w.display().to_string())
}

/// Which kept graph's git side a [`GraphGit`] is: each one made has its own, so a round of git's side taken from one
/// before a reload of the record puts nothing in the next ([`GitRead`]), as the agents' side's (`KeptSide`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Epoch(u64);

impl Default for Epoch {
    fn default() -> Epoch {
        static EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        Epoch(EPOCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

/// A round of git's side as [`GitReading::read`] gives it: the folders newly placed, for [`GraphGit::keep_placed`], and
/// the repositories read, of the [`GraphGit`] whose EPOCH it is.
#[derive(Debug)]
pub struct GitRead {
    pub epoch: Epoch,
    pub placed: Vec<(String, Placed)>,
    pub read: Vec<RepoRead>,
}

/// What [`super::git_read`] read of one repository: its folder, its refs, its commits, its branches' moves and the
/// commits made in each of its worktrees (each with the repository's name), and its cargo packages ([`manifests`],
/// [`packages_of`]; proto-mind N-1446 p1502 (1)), none when git could not say them.
#[derive(Clone, Debug, Default)]
pub struct RepoRead {
    pub dir: String,
    pub refs: String,
    pub commits: Vec<Commit>,
    pub moves: Vec<Moved>,
    pub packages: Option<Packages>,
    pub made: Vec<MadeIn>,
}

/// The cargo manifests of DIR's repository at its `main`, else at its HEAD: each `Cargo.toml`'s path from the top and
/// its text, by one `ls-tree` and one `cat-file --batch` ([`git`], read only); none when git cannot say.
pub fn manifests(dir: &str) -> Option<Vec<(String, String)>> {
    use std::io::Write;
    use std::process::Stdio;
    let path = Path::new(dir);
    let tree = ["main", "HEAD"].iter().find_map(|rev| git(path).args(["ls-tree", "-r", "--full-tree", *rev]).output().ok().filter(|o| o.status.success()))?;
    let tree = String::from_utf8_lossy(&tree.stdout).into_owned();
    // each line: MODE blob HASH, a tab, the path
    let files: Vec<(&str, &str)> = tree
        .lines()
        .filter_map(|l| {
            let (meta, p) = l.split_once('\t')?;
            let f: Vec<&str> = meta.split(' ').collect();
            (f.len() == 3 && f[1] == "blob" && (p == "Cargo.toml" || p.ends_with("/Cargo.toml"))).then_some((f[2], p))
        })
        .collect();
    if files.is_empty() {
        return Some(Vec::new());
    }
    let mut child = git(path).args(["cat-file", "--batch"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let input: String = files.iter().map(|(h, _)| format!("{h}\n")).collect();
    let mut stdin = child.stdin.take()?;
    // written beside the read, so that neither pipe filling stalls the other
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let out = child.wait_with_output().ok()?;
    let _ = writer.join();
    if !out.status.success() {
        return None;
    }
    // each: `HASH blob SIZE`, a newline, SIZE bytes, a newline
    let (mut rest, mut read) = (&out.stdout[..], Vec::new());
    for (_, p) in &files {
        let nl = rest.iter().position(|&b| b == b'\n')?;
        let size: usize = std::str::from_utf8(&rest[..nl]).ok()?.rsplit(' ').next()?.parse().ok()?;
        read.push((p.to_string(), String::from_utf8_lossy(rest.get(nl + 1..nl + 1 + size)?).into_owned()));
        rest = rest.get(nl + 2 + size..).unwrap_or_default();
    }
    Some(read)
}

/// REPO's cargo packages from its MANIFESTS (each `Cargo.toml`'s path from the top and its text, [`manifests`]): each
/// package's folder and name; its workspaces' members (a workspace's `members`, a `DIR/*` one each package folder right
/// under DIR, and the workspace's own package); and each (user, used) where a package names another's folder as a
/// `path` dependency (of its dependencies, dev- or build-dependencies, a target's too), directly or through its
/// workspace's `dependencies` (`X.workspace = true`). A manifest that does not read is passed over.
pub fn packages_of(repo: &str, manifests: &[(String, String)]) -> Packages {
    let mut p = Packages { repo: repo.to_string(), ..Default::default() };
    let tables: Vec<(String, toml::Table)> = manifests.iter().filter_map(|(path, text)| Some((path.strip_suffix("Cargo.toml")?.trim_end_matches('/').to_string(), toml::from_str::<toml::Table>(text).ok()?))).collect();
    for (dir, t) in &tables {
        if let Some(n) = t.get("package").and_then(|x| x.get("name")).and_then(|n| n.as_str()) {
            p.dirs.insert(dir.clone(), n.to_string());
        }
    }
    let workspaces: Vec<(&str, &toml::Table)> = tables.iter().filter_map(|(d, t)| Some((d.as_str(), t.get("workspace")?.as_table()?))).collect();
    for (dir, ws) in &workspaces {
        p.members.extend(p.dirs.get(*dir).cloned());
        for m in ws.get("members").and_then(|m| m.as_array()).into_iter().flatten().filter_map(|m| m.as_str()) {
            let at = joined(dir, m);
            match at.strip_suffix("/*") {
                Some(under) => p.members.extend(p.dirs.iter().filter(|(d, _)| d.rsplit_once('/').map_or("", |(a, _)| a) == under).map(|(_, n)| n.clone())),
                None => p.members.extend(p.dirs.get(&at).cloned()),
            }
        }
    }
    static KINDS: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];
    for (dir, t) in &tables {
        let Some(user) = p.dirs.get(dir).cloned() else { continue };
        let ws = workspaces.iter().filter(|(w, _)| w.is_empty() || dir == w || dir.starts_with(&format!("{w}/"))).max_by_key(|(w, _)| w.len());
        let targets = t.get("target").and_then(|x| x.as_table()).into_iter().flat_map(|ts| ts.values());
        let deps = KINDS.iter().filter_map(|k| t.get(*k)?.as_table()).chain(targets.flat_map(|x| KINDS.iter().filter_map(move |k| x.get(*k)?.as_table())));
        for (name, d) in deps.flat_map(|d| d.iter()) {
            let at = match d.get("path").and_then(|v| v.as_str()) {
                Some(rel) => Some(joined(dir, rel)),
                None if d.get("workspace").and_then(|v| v.as_bool()) == Some(true) => ws.and_then(|(w, wt)| Some(joined(w, wt.get("dependencies")?.get(name)?.get("path")?.as_str()?))),
                None => None,
            };
            if let Some(used) = at.and_then(|at| p.dirs.get(&at)) {
                p.uses.insert((user.clone(), used.clone()));
            }
        }
    }
    p
}

/// REL (a manifest's path: `../coord-types`, `crates/*`) from folder DIR of a repository, with its `.` and `..` taken
/// out: a path from the repository's top, empty for the top.
fn joined(dir: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    for s in rel.split('/') {
        match s {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// The commits git made in each worktree of the repository NAME whose common git folder is COMMON, from each one's own
/// HEAD reflog, its file read whole (paper-research N-1446 p1530, p1554): the main worktree's `logs/HEAD`, its top the
/// folder COMMON is the `.git` of (a bare repository's has none); each other one's `worktrees/W/logs/HEAD`, its top the
/// folder `worktrees/W/gitdir` names the `.git` of, as `git worktree list` gives it (git writes it as the real path, so
/// it is the top `rev-parse --show-toplevel` gives in it, [`Placed::In`]; a relative one, from git's
/// `worktree.useRelativePaths`, is taken from `worktrees/W`). A worktree pruned has no reflog left, so what it made is
/// in none; one whose folder is gone but not pruned still says it.
pub fn head_reflogs(common: &Path, name: &str) -> Vec<MadeIn> {
    let text = |p: &Path| std::fs::read(p).ok().map(|b| String::from_utf8_lossy(&b).into_owned());
    let mut made = Vec::new();
    if let Some(top) = common.to_str().and_then(|c| c.strip_suffix("/.git")) {
        made.extend(MadeIn::parse_reflog(&text(&common.join("logs/HEAD")).unwrap_or_default(), name, top));
    }
    let mut linked: Vec<PathBuf> = std::fs::read_dir(common.join("worktrees")).into_iter().flatten().flatten().map(|e| e.path()).collect();
    linked.sort();
    for w in linked {
        let Some(gitdir) = text(&w.join("gitdir")) else { continue };
        let Some(dot) = normal(&w.join(gitdir.trim()).display().to_string()) else { continue };
        let Some(top) = dot.to_str().and_then(|d| d.strip_suffix("/.git")) else { continue };
        made.extend(MadeIn::parse_reflog(&text(&w.join("logs/HEAD")).unwrap_or_default(), name, top));
    }
    made
}

/// git asked in DIR, only to read: what a repository's own config could make a read run is switched off
/// (`log.showSignature` runs its `gpg.program`, `core.fsmonitor` its hook), since an agent names the folders it is
/// asked in.
pub fn git(dir: &Path) -> std::process::Command {
    let mut c = std::process::Command::new("git");
    c.args(["-c", "log.showSignature=false", "-c", "core.fsmonitor=false"]).arg("-C").arg(dir);
    c
}

impl GraphGit {
    /// The folders line L names where agents work: an `edit` act's files' folders, a `job` act's worktree; and the
    /// commit a job act names, which counts its repository.
    pub fn take(&mut self, l: &Line) {
        let Line::Act { is, more, .. } = l else { return };
        match is.as_str() {
            "edit" => {
                let files = more.get("files").and_then(|v| v.as_array()).into_iter().flatten().filter_map(|v| v.as_str());
                self.named.extend(files.filter_map(normal).filter_map(|f| Some(f.parent()?.display().to_string())));
            }
            "job" => {
                self.named.extend(more.get("wt").and_then(|v| v.as_str()).and_then(worktree));
                self.job_commits.extend(more.get("commit").and_then(|v| v.as_str()).and_then(short));
            }
            _ => {}
        }
    }

    /// What the record's lines alone put in (`take`), to save with a replay stopped at a cut (replay_state.rs): the
    /// folders named and the jobs' commits.
    pub fn saved(&self) -> (BTreeSet<String>, BTreeSet<String>) {
        (self.named.clone(), self.job_commits.clone())
    }

    /// A git side of its own (a new epoch) holding SAVED, as `take` leaves one.
    pub fn from_saved((named, job_commits): (BTreeSet<String>, BTreeSet<String>)) -> GraphGit {
        GraphGit { named, job_commits, ..Default::default() }
    }

    /// The worktrees of the runner's JOBS from before the record had them, and their commits.
    pub fn take_jobs(&mut self, jobs: &[Job]) {
        self.named.extend(jobs.iter().filter_map(|j| worktree(&j.wt)));
        self.job_commits.extend(jobs.iter().filter_map(|j| short(&j.commit)));
    }

    /// READ set aside until its repository counts: its log and its worktrees' HEAD reflogs are read whole each time, its
    /// moves only those of the branches that moved since, each such branch's whole, so a branch's moves read again stand
    /// for those before.
    pub(super) fn set_aside(&mut self, read: RepoRead) {
        match self.left.get_mut(&read.dir) {
            Some(l) => {
                let again: std::collections::HashSet<&str> = read.moves.iter().map(|m| m.branch.as_str()).collect();
                l.moves.retain(|m| !again.contains(m.branch.as_str()));
                l.moves.extend(read.moves);
                (l.refs, l.commits, l.made) = (read.refs, read.commits, read.made);
                l.packages = read.packages.or(l.packages.take());
            }
            None => {
                self.left.insert(read.dir.clone(), read);
            }
        }
    }

    /// Whether commit C counts its repository: one of its `Agent:` trailers names one of HERE, or a job from here ran on
    /// it.
    pub fn counts(&self, c: &Commit, here: &BTreeSet<String>) -> bool {
        c.agent.split(',').map(str::trim).any(|a| here.contains(a)) || short(&c.hash).is_some_and(|h| self.job_commits.contains(&h))
    }

    /// The `Agent:` names of the commits set aside and of READ's: which of them are agents here is the core's to say.
    pub fn names(&self, read: &[RepoRead]) -> BTreeSet<String> {
        self.left.values().chain(read).flat_map(|r| &r.commits).flat_map(|c| c.agent.split(',')).map(str::trim).filter(|a| !a.is_empty()).map(String::from).collect()
    }

    /// The commit in the kept graph whose id is ID (`commit:HASH8`): its subject and its repository's name label it.
    pub fn commit(&self, id: &str) -> Option<&Commit> {
        self.by_id.get(id).map(|&i| &self.commits[i])
    }

    /// The folders of the repositories read whose commits are in, and of those set aside.
    pub fn counted_and_left(&self) -> (Vec<&String>, Vec<&String>) {
        (self.counted.iter().collect(), self.left.keys().collect())
    }

    /// What [`GitReading::read`] reads off the lock, with the project's repositories PROJECT (`Project::repos`).
    pub fn reading(&self, project: Vec<(String, String)>) -> GitReading {
        let unplaced = self.named.iter().chain(project.iter().map(|(_, d)| d)).filter(|f| !self.placed.contains_key(*f)).cloned().collect();
        GitReading { epoch: self.epoch, project, placed: self.placed.clone(), unplaced, refs: self.refs.clone() }
    }

    /// PLACED, as [`GitReading::read`] gives it, kept: each folder is placed once.
    pub fn keep_placed(&mut self, placed: Vec<(String, Placed)>) {
        self.placed.extend(placed);
    }

    /// The repositories read, with the project's PROJECT ([`repos`]): a test's, and `Core::graph_git_add`'s, whose
    /// `trains_moved` reads a train branch's tip in them (N-1511 p43, `merged`).
    pub fn repos(&self, project: &[(String, String)]) -> Vec<(String, String)> {
        repos(project, &self.placed)
    }

    /// The folders named that are gone, each by the topmost of its folders gone, once.
    pub fn gone(&self) -> Vec<String> {
        let gone: BTreeSet<&String> = self.named.iter().filter_map(|f| match self.placed.get(f) {
            Some(Placed::Gone(g)) => Some(g),
            _ => None,
        }).collect();
        gone.into_iter().cloned().collect()
    }
}

/// What the kept graph reads git by, taken under the lock ([`GraphGit::reading`]) and read off it.
#[derive(Debug, Default)]
pub struct GitReading {
    epoch: Epoch,
    project: Vec<(String, String)>,
    placed: BTreeMap<String, Placed>,
    unplaced: BTreeSet<String>,
    refs: HashMap<String, String>,
}

impl GitReading {
    /// Off the lock: the folders not placed yet, with the worktrees of JOBS (the runner's from before the record had
    /// them) not placed yet, placed ([`place`], HOME the owner's); then git's log and reflogs of each repository whose
    /// refs moved ([`super::git_read`]): the round ([`GitRead`]).
    pub fn read(mut self, jobs: &[Job], home: &str) -> GitRead {
        let more: Vec<String> = jobs.iter().filter_map(|j| worktree(&j.wt)).filter(|w| !self.placed.contains_key(w)).collect();
        self.unplaced.extend(more);
        let new = place(&self.unplaced, home);
        self.placed.extend(new.iter().cloned());
        let read = super::git_read(&repos(&self.project, &self.placed), &self.refs);
        GitRead { epoch: self.epoch, placed: new, read }
    }
}

/// Where each of FOLDERS sits for git, HOME the owner's. A place never read is in none, and git is not asked there;
/// a folder that is not there is gone, by the topmost of its folders not there; else git is asked once in the nearest
/// folder that holds a `.git` (git's own search gives the same from there), and a folder with none above it is in no
/// repository.
pub fn place(folders: &BTreeSet<String>, home: &str) -> Vec<(String, Placed)> {
    let never = crate::files::Places { home: home.to_string(), ..Default::default() };
    let mut asked: HashMap<PathBuf, Placed> = HashMap::new();
    folders
        .iter()
        .map(|f| {
            let p = Path::new(f);
            let real = std::fs::canonicalize(p).ok();
            let at = if never.never_served(p).is_some() || real.as_deref().is_some_and(|r| never.never_served(r).is_some()) {
                Placed::Nowhere
            } else if let Some(real) = real {
                match real.ancestors().find(|a| a.join(".git").exists()) {
                    Some(holds) => asked.entry(holds.to_path_buf()).or_insert_with(|| rev_parse(holds)).clone(),
                    None => Placed::Nowhere,
                }
            } else {
                Placed::Gone(p.ancestors().take_while(|a| !a.exists()).last().unwrap_or(p).display().to_string())
            };
            (f.clone(), at)
        })
        .collect()
}

/// git's top and common folder of DIR, which holds a `.git`. When git reads none: gone if that `.git` is a link (a
/// worktree's, its repository's record of it taken away, where git's search stops); a `.git` folder git does not take
/// for a repository it passes, as git does, so none was found above it either (the home's own `.git` folder on
/// p1455's copy run).
fn rev_parse(dir: &Path) -> Placed {
    let out = git(dir).args(["rev-parse", "--path-format=absolute", "--show-toplevel", "--git-common-dir"]).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned());
    match out.as_deref().unwrap_or_default().lines().collect::<Vec<_>>()[..] {
        [top, common] => Placed::In { top: top.to_string(), common: common.to_string() },
        _ if dir.join(".git").is_file() => Placed::Gone(dir.display().to_string()),
        _ => Placed::Nowhere,
    }
}

/// The repositories read for the kept graph: the project's PROJECT as they are, then each other repository a folder
/// of PLACED is in, once by its common git folder, read in its main folder (the common folder's, when that is a
/// `.git`) and named by that folder's name; two of one name, or one of a name the project has, by the folder whole.
pub fn repos(project: &[(String, String)], placed: &BTreeMap<String, Placed>) -> Vec<(String, String)> {
    let common = |p: &Placed| match p {
        Placed::In { common, .. } => Some(common.clone()),
        _ => None,
    };
    let theirs: BTreeSet<String> = project.iter().filter_map(|(_, d)| placed.get(d).and_then(common)).collect();
    let found: BTreeSet<String> = placed.values().filter_map(common).filter(|c| !theirs.contains(c)).collect();
    let main = |c: &str| c.strip_suffix("/.git").unwrap_or(c).to_string();
    let name = |d: &str| Path::new(d).file_name().map(|n| n.to_string_lossy().trim_end_matches(".git").to_string()).unwrap_or_default();
    let mut names: HashMap<String, usize> = HashMap::new();
    for n in project.iter().map(|(n, _)| n.clone()).chain(found.iter().map(|c| name(&main(c)))) {
        *names.entry(n).or_default() += 1;
    }
    let found = found.iter().map(|c| {
        let dir = main(c);
        let n = name(&dir);
        (if n.is_empty() || names.get(&n) != Some(&1) { dir.clone() } else { n }, dir)
    });
    project.iter().cloned().chain(found).collect()
}
