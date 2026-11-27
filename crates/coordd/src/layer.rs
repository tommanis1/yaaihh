//! Layers (plan/layers.md Part I §2 and §6; paper-research N-89.p2): the workspace a `layer [on e] { p }` scope's
//! steps share. All of it lives under `$COORD/layers/`:
//!
//! - `bases/SHA/`: the tree of commit SHA (`git archive`), made once per commit, with empty folders as mount points:
//!   `.git`, `.conversations` and each build output folder the project declares (`outputs`). A lower layer only:
//!   the overlay never writes it.
//! - `ID/up`, `ID/work`: the layer (what was written on top) and the overlay's work folder.
//! - `ID/git`: the view's own small git folder (`HEAD` on the layer's branch `layer/ID`, `index`, `commondir`), bound
//!   at `.git` (test 4). Objects and refs are the project's, so a commit in the view is at once on the branch in the
//!   project's repository. No `git worktree add` (the owner: no worktrees), so git does not guard the branch: coordd
//!   refuses two views on one branch, and one view per layer.
//! - `ID/out/NAME`: each build output folder, bound at NAME in the view from outside the layer (test 5); the view's
//!   own, removed when the view ends.
//! - `ID/mnt/common`, `ID/mnt/convs`: inside the view's namespace, the project's real git folder and conversations
//!   folder, bound there before the overlay hides the project's path.
//! - `layers.json`: every layer, open or ended (`Layer`).
//!
//! The view is an overlay with `userxattr` in a user and mount namespace of its own (test 2), mounted at the
//! project's path, so its agents have the project's path as cwd. A holder process (`sleep infinity`, coordd's child,
//! in its own session) keeps the namespace for as long as the scope lasts. It stays in the user namespace that owns
//! the mount (root there is the calling user): a process joins with `nsenter --user --mount` on it and then maps
//! itself back to the calling user with a user namespace of its own (`joiner`; roster's ROSTER_VIEW), since joining
//! a mount needs the rights of the user namespace that owns it. From outside, what the view shows is `/proc/HOLDER/root/PATH` (follow-up F2), what the layer changed
//! is `ID/up` (deletions are character devices 0/0). When the scope ends, the holder is stopped, the view goes with
//! the last process in it, and the layer is made read-only; uncommitted files stay in it. When its ticket closes, the
//! layer is removed (`remove`; N-95.p5): its folder, its branch unless it holds commits on no line (then when the top
//! ticket of its tree closes), its base once no layer stands on it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// One layer operation at a time (views, the registry).
static LOCK: Mutex<()> = Mutex::new(());

/// What a layer is made from: the scope's ID (`E-3.l1`), its TICKET, ON (a commit or branch; none, the root's head),
/// the project's path REPO, the root branch ROOT if the project names one, the build output folders OUTPUTS; HOME
/// (Claude Code's `~/.claude` is in it) and CODEX_HOME, where the conversations' links go.
#[derive(Clone, Debug)]
pub struct Spec {
    pub id: String,
    pub ticket: String,
    pub on: Option<String>,
    pub repo: String,
    pub root: Option<String>,
    pub outputs: Vec<String>,
    pub home: String,
    pub codex_home: String,
    /// The project's other repositories (`repos`, N-101), each NAME and path: one layer over all of them, each its
    /// own part of the view at its own path. NAME: the project's own repository's name among them (an `on` of commits
    /// by repository names it so).
    pub also: Vec<(String, String)>,
    pub name: String,
    /// The project is boxed (`box = true`, sandbox.rs): the view's namespace maps the owner's sub-uids, the layer's
    /// files are the box group's, and its git folder is its own (refs and objects; the project's objects as an
    /// alternate), brought into the project's repository by coordd (`sync`).
    pub boxed: bool,
    /// The line's built outputs (`warm`, project.rs; warm.rs): a folder, or a link to one, holding each output and a
    /// file `commit`. Each output found there starts from it instead of empty. None: every output starts empty.
    pub warm: Option<String>,
}

/// A layer as `layers.json` keeps it and `coord layers` shows it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Layer {
    pub id: String,
    pub ticket: String,
    /// The project's path: where the view is mounted.
    pub repo: String,
    /// The commit its base is, and what it was asked on.
    pub base: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
    pub branch: String,
    /// open (a view while the scope lasts) or ended (read-only).
    pub state: String,
    /// The holder process of the view, while open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holder: Option<i32>,
    /// What the view shows, read from outside: `/proc/HOLDER/root/REPO` (while open).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    /// What the layer changed: its upper folder.
    pub up: String,
    pub outputs: Vec<String>,
    pub at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<String>,
    /// What was found and not changed when it was made (a conversations entry that is a real folder, ...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// Made for a boxed project (`Spec::boxed`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub boxed: bool,
    /// The project's root branch when it was made (`Spec::root`): a branch contained in it has nothing to keep.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// When its folder went (`remove`): its ticket had closed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed: Option<String>,
    /// Its branch was kept when its folder went (commits of its own on no line): it goes when the top ticket of its
    /// tree closes.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub kept: bool,
    /// What `remove` could not do, each said in the journal: tried again at the next removal and at coordd's start.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub left: Vec<String>,
    /// The project's other repositories in this layer (N-101): each its own base, branch `layer/ID`, overlay and git
    /// folder under `ID/parts/NAME/`, mounted at its own path in the same view.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<Part>,
    /// The pinned outputs its view's outputs start from (`warm/NAME`, warm.rs): NAME is the commit, `-box` after it
    /// for a boxed layer's copy. None: every output started empty (`notes` says why, when the project has `warm`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warm: Option<String>,
}

/// One more repository of a layer: its NAME in the project's `repos`, its path REPO, the commit its BASE is, its
/// BRANCH, and the commit or branch ON it was asked on for it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Part {
    pub name: String,
    pub repo: String,
    pub base: String,
    pub branch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
}

/// What a layer starts from in repository NAME: ON as given (a branch, meaning that branch in every repository, or a
/// commit), or ON's commit for NAME when ON is commits by repository (`{"tovrin": "a1b2", ...}`, N-101); with no ON,
/// the root branch. Commits by repository that do not name NAME: no layer (NoLayer).
fn start_of(on: &Option<String>, root: &Option<String>, name: &str) -> Result<String, String> {
    match on {
        Some(o) if o.trim_start().starts_with('{') => {
            let m: BTreeMap<String, String> = serde_json::from_str(o).map_err(|e| format!("`on` {o}: commits by repository, each a text: {e}"))?;
            match m.get(name) {
                Some(c) => Ok(c.clone()),
                // a project of one repository: the one commit given
                None if name.is_empty() && m.len() == 1 => Ok(m.into_values().next().unwrap_or_default()),
                None => Err(format!("`on` names commits of {}, and none of the repository {name}", m.keys().cloned().collect::<Vec<_>>().join(", "))),
            }
        }
        Some(o) => Ok(o.clone()),
        None => root.clone().ok_or_else(|| String::from("no root branch is set, so a layer without `on` has no commit to start from")),
    }
}

fn dir(coord: &str) -> String {
    format!("{coord}/layers")
}

pub fn load(coord: &str) -> BTreeMap<String, Layer> {
    std::fs::read_to_string(format!("{}/layers.json", dir(coord))).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save(coord: &str, reg: &BTreeMap<String, Layer>) -> Result<(), String> {
    let p = format!("{}/layers.json", dir(coord));
    let tmp = format!("{p}.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(reg).unwrap()).map_err(|e| format!("{tmp}: {e}"))?;
    std::fs::rename(&tmp, &p).map_err(|e| format!("{p}: {e}"))
}

/// Whether HOLDER still holds the view of layer ID at REPO (its git folder, seen through it, names the layer).
fn holds(holder: i32, repo: &str, id: &str) -> bool {
    std::fs::read_to_string(format!("/proc/{holder}/root{repo}/.git/layer")).is_ok_and(|s| s.trim() == id)
}

fn git(repo: &str, args: &[&str]) -> Result<String, String> {
    let o = Command::new("git").arg("-C").arg(repo).args(args).stdin(Stdio::null()).output().map_err(|e| format!("git: {e}"))?;
    if !o.status.success() {
        return Err(format!("git {}: {}", args.join(" "), String::from_utf8_lossy(&o.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Claude Code's name for a project folder: each character not a letter or digit a dash.
fn claude_entry(path: &str) -> String {
    path.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

/// LINK is to be a link to the real folder TO (made if missing). A link already there to TO is fine; anything else
/// is left as it is, and said in the result.
fn link(link: &str, to: &str) -> Option<String> {
    let l = Path::new(link);
    match std::fs::symlink_metadata(l) {
        Err(_) => {
            if let Some(p) = l.parent() {
                let _ = std::fs::create_dir_all(p);
            }
            std::os::unix::fs::symlink(to, l).err().map(|e| format!("{link}: {e}"))
        }
        Ok(m) if m.file_type().is_symlink() && std::fs::read_link(l).is_ok_and(|t| t == Path::new(to)) => None,
        Ok(_) => Some(format!("{link} is there already and is not a link to {to}: left as it is, so conversations started in the view are written there")),
    }
}

/// A boxed layer L's own git folder G (in layer folder D): HEAD on its branch, the branch at its base, an index of
/// the base, the objects folder with the project's (COMMON's, through the view's `mnt/common`) as an alternate, and
/// `core.sharedRepository = group` so two agents of the layer write each other's objects. The alternate's path is
/// there only inside the view's namespace: the index is read before it is written. What the repository's own
/// `info/exclude` ignores the view ignores too, a copy of it in G's `info/` (a view through `commondir` shares the
/// repository's): with none, `git status` in the layer and coordd's uncommitted check listed it (`.conversations/`
/// on the stand-in, docket-core3, paper-research N-101 p856).
fn own_git(g: &str, inside: &str, common: &str, branch: &str, base: &str, line: &[String]) -> Result<(), String> {
    let refs = format!("{g}/refs/heads/{branch}");
    for sub in [format!("{g}/objects/info"), format!("{g}/info"), Path::new(&refs).parent().map(|p| p.display().to_string()).unwrap_or_default()] {
        std::fs::create_dir_all(&sub).map_err(|e| format!("{sub}: {e}"))?;
    }
    if let Ok(x) = std::fs::read(format!("{common}/info/exclude")) {
        std::fs::write(format!("{g}/info/exclude"), x).map_err(|e| format!("{g}/info/exclude: {e}"))?;
    }
    std::fs::write(&refs, format!("{base}\n")).map_err(|e| e.to_string())?;
    // LINE, the line's branches of the repository (its root, `landed` on the stand-in, and `dev`), under their own names
    // at the heads they have when the layer is made: a plan written in the line's view names them, and the minder
    // diffs against them (docket-core3, N-101 p935; paper-research p939). Copies: a branch of the repository's is
    // written only in its own git folder, which the layer's does not share (no commondir: its branches would be the
    // repository's), so one that moves later is not followed. Moving one here moves nothing in the repository:
    // `sync` brings back only BRANCH
    let o = Command::new("git").args(["for-each-ref", "--format=%(objectname) %(refname)", "refs/heads"]).env("GIT_DIR", common).stdin(Stdio::null()).output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(format!("the repository's branches: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    let keep: Vec<String> = line.iter().filter(|b| *b != branch).map(|b| format!("refs/heads/{b}")).collect();
    let packed: String = String::from_utf8_lossy(&o.stdout).lines().filter(|l| l.split_once(' ').is_some_and(|(_, r)| keep.iter().any(|k| k == r))).map(|l| format!("{l}\n")).collect();
    std::fs::write(format!("{g}/packed-refs"), format!("# pack-refs with: peeled fully-peeled sorted \n{packed}")).map_err(|e| format!("{g}/packed-refs: {e}"))?;
    std::fs::write(format!("{g}/config"), "[core]\n\trepositoryformatversion = 0\n\tfilemode = true\n\tbare = false\n\tsharedRepository = group\n").map_err(|e| e.to_string())?;
    std::fs::write(format!("{g}/HEAD"), format!("ref: refs/heads/{branch}\n")).map_err(|e| e.to_string())?;
    let o = Command::new("git")
        .args(["read-tree", base])
        .env("GIT_DIR", g)
        .env("GIT_OBJECT_DIRECTORY", format!("{g}/objects"))
        .env("GIT_ALTERNATE_OBJECT_DIRECTORIES", format!("{common}/objects"))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(format!("the view's index: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    std::fs::write(format!("{g}/objects/info/alternates"), format!("{inside}/objects\n")).map_err(|e| e.to_string())
}

/// The line's branches a boxed layer's git carries (`own_git`): the root, if the project names one, and `dev`.
fn line_branches(root: &Option<String>) -> Vec<String> {
    let mut b: Vec<String> = root.iter().cloned().collect();
    if !b.iter().any(|r| r == "dev") {
        b.push("dev".into());
    }
    b
}

/// A view's git folder G (not boxed) for BRANCH at BASE: HEAD, the project's objects and refs through INSIDE (the
/// repository's git folder as the view's namespace has it bound), an index of the base.
fn view_git(g: &str, inside: &str, common: &str, branch: &str, base: &str) -> Result<(), String> {
    std::fs::write(format!("{g}/HEAD"), format!("ref: refs/heads/{branch}\n")).map_err(|e| e.to_string())?;
    std::fs::write(format!("{g}/commondir"), format!("{inside}\n")).map_err(|e| e.to_string())?;
    let o = Command::new("git").args(["read-tree", base]).env("GIT_DIR", g).env("GIT_COMMON_DIR", common).stdin(Stdio::null()).output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(format!("the view's index: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    Ok(())
}

/// Bring boxed layer ID's branch into the project's repository: its commits' objects fetched from the view's own git
/// folder by coordd as root of the view's namespace (what is written is the owner's), the branch set to the view's.
/// Nothing for a layer not boxed (its commits are the project's at once) or not open.
pub fn sync(coord: &str, id: &str) -> Result<(), String> {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let reg = load(coord);
    let Some(l) = reg.get(id) else { return Err(format!("no layer {id}")) };
    let Some(h) = l.holder.filter(|h| l.boxed && l.state == "open" && holds(*h, &l.repo, &l.id)) else { return Ok(()) };
    let d = format!("{}/{id}", dir(coord));
    let mut each = vec![(format!("{d}/mnt/common"), format!("{d}/git"), l.branch.clone(), l.repo.clone())];
    each.extend(l.also.iter().map(|p| (format!("{d}/parts/{}/mnt/common", p.name), format!("{d}/parts/{}/git", p.name), p.branch.clone(), p.repo.clone())));
    let mut failed = Vec::new();
    for (common, g, branch, repo) in each {
        let o = Command::new("nsenter")
            .args(["--user", "--mount", "--preserve-credentials", "-t", &h.to_string(), "--", "git", "-C", &common, "-c", "safe.directory=*", "fetch", "-q"])
            .arg(&g)
            .arg(format!("+refs/heads/{0}:refs/heads/{0}", branch))
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("nsenter: {e}"))?;
        if !o.status.success() {
            failed.push(format!("bringing {branch} into {repo}: {}", String::from_utf8_lossy(&o.stderr).trim()));
        }
    }
    if failed.is_empty() { Ok(()) } else { Err(failed.join("; ")) }
}

/// `sync` every open boxed layer of COORD; what failed, said.
pub fn sync_open(coord: &str) -> Vec<String> {
    let ids: Vec<String> = load(coord).into_values().filter(|l| l.boxed && l.state == "open").map(|l| l.id).collect();
    ids.iter().filter_map(|id| sync(coord, id).err()).collect()
}

/// The base of commit SHA of REPO, made if missing, with OUTPUTS' mount points.
fn base(coord: &str, repo: &str, sha: &str, outputs: &[String]) -> Result<String, String> {
    let b = format!("{}/bases/{sha}", dir(coord));
    if !Path::new(&b).is_dir() {
        let tmp = format!("{}/bases/.{sha}.{}", dir(coord), std::process::id());
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).map_err(|e| format!("{tmp}: {e}"))?;
        let o = Command::new("sh").arg("-c").arg("git -C \"$1\" archive \"$2\" | tar -x -C \"$3\"").args(["base", repo, sha, &tmp]).stdin(Stdio::null()).output().map_err(|e| e.to_string())?;
        if !o.status.success() {
            return Err(format!("the tree of {sha}: {}", String::from_utf8_lossy(&o.stderr).trim()));
        }
        std::fs::rename(&tmp, &b).map_err(|e| format!("{b}: {e}"))?;
    }
    for m in [".git", ".conversations"].iter().map(|s| s.to_string()).chain(outputs.iter().cloned()) {
        std::fs::create_dir_all(format!("{b}/{m}")).map_err(|e| format!("{b}/{m}: {e}"))?;
    }
    Ok(b)
}

/// Open layer SPEC's view, or say why not. Once open it stays (a second call gives the same); a holder found gone
/// (the machine restarted) is made again from the base and the layer, as the view was. An ended layer is not
/// opened again. Blocking: it waits for the view to be up.
pub fn open(coord: &str, spec: &Spec, now: &str) -> Result<Layer, String> {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut reg = load(coord);
    let d = format!("{}/{}", dir(coord), spec.id);
    let mut notes = Vec::new();
    let mut l = match reg.get(&spec.id) {
        Some(l) if l.state == "ended" => return Err(format!("layer {} has ended", spec.id)),
        Some(l) if l.holder.is_some_and(|h| holds(h, &l.repo, &l.id)) => return Ok(l.clone()),
        Some(l) => l.clone(),
        None => {
            let branch = format!("layer/{}", spec.id);
            if let Some(o) = reg.values().find(|o| o.branch == branch && o.state == "open") {
                return Err(format!("{branch} has a view already (layer {})", o.id));
            }
            // every repository's start, and its branch free, before any branch is made
            let mut starts = Vec::new();
            for (name, repo) in std::iter::once((spec.name.clone(), spec.repo.clone())).chain(spec.also.iter().cloned()) {
                let rev = start_of(&spec.on, &spec.root, &name)?;
                let sha = git(&repo, &["rev-parse", "--verify", "--quiet", &format!("{rev}^{{commit}}")]).map_err(|_| format!("{rev} is no commit of {repo}"))?;
                if git(&repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_ok() {
                    return Err(format!("the branch {branch} exists already in {repo}"));
                }
                starts.push((name, repo, rev, sha));
            }
            let sha = starts[0].3.clone();
            let also: Vec<Part> = starts[1..].iter().map(|(name, repo, rev, sha)| Part { name: name.clone(), repo: repo.clone(), base: sha.clone(), branch: branch.clone(), on: spec.on.as_ref().map(|_| rev.clone()) }).collect();
            // conversations are in no layer and in no commit
            if git(&spec.repo, &["check-ignore", "-q", ".conversations/x"]).is_err() {
                return Err(format!("{} does not ignore .conversations/, so a commit in the view would take the conversations", spec.repo));
            }
            git(&spec.repo, &["branch", &branch, &sha])?;
            for p in &also {
                git(&p.repo, &["branch", &branch, &p.base])?;
            }
            Layer {
                id: spec.id.clone(),
                ticket: spec.ticket.clone(),
                repo: spec.repo.clone(),
                base: sha,
                on: spec.on.clone(),
                branch,
                state: "open".into(),
                holder: None,
                view: None,
                up: format!("{d}/up"),
                outputs: spec.outputs.clone(),
                at: now.to_string(),
                ended: None,
                notes: Vec::new(),
                boxed: spec.boxed,
                root: spec.root.clone(),
                removed: None,
                kept: false,
                left: Vec::new(),
                also,
                warm: None,
            }
        }
    };
    let made = l.holder.is_none() && !Path::new(&d).exists();
    let b = base(coord, &l.repo, &l.base, &l.outputs)?;
    // warm outputs (warm.rs): pinned when the layer is made, kept for its life; what differs from the warm commit is
    // touched in the view before it counts as up
    if made {
        if let Some(path) = &spec.warm {
            let p = crate::warm::pin(&format!("{}/warm", dir(coord)), &l.repo, &spec.name, path, &l.outputs, l.boxed);
            for n in &p.notes {
                eprintln!("coordd: layers: {}: {n}", l.id);
            }
            notes.extend(p.notes);
            if let (Some(name), Some(sha)) = (p.name, p.sha) {
                if !p.warm.is_empty() {
                    std::fs::create_dir_all(&d).map_err(|e| format!("{d}: {e}"))?;
                    match crate::warm::changed(&l.repo, &sha, &l.base) {
                        Ok(c) => std::fs::write(format!("{d}/touch"), c.iter().map(|f| format!("{f}\0")).collect::<String>()).map_err(|e| format!("{d}/touch: {e}"))?,
                        Err(e) => notes.push(format!("no file touched: {e}")),
                    }
                    l.warm = Some(name);
                }
            }
        }
    }
    let warm_dir = l.warm.as_ref().map(|w| format!("{}/warm/{w}", dir(coord))).filter(|w| Path::new(w).is_dir());
    let warms: Vec<String> = match &warm_dir {
        Some(w) => l.outputs.iter().filter(|o| Path::new(&format!("{w}/{o}")).is_dir()).cloned().collect(),
        None => Vec::new(),
    };
    let common = git(&l.repo, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    for sub in ["up", "work", "git", "mnt/common", "mnt/convs"] {
        std::fs::create_dir_all(format!("{d}/{sub}")).map_err(|e| format!("{d}/{sub}: {e}"))?;
    }
    for o in &l.outputs {
        let subs: &[&str] = if warms.contains(o) { &["/up", "/work"] } else { &[""] };
        for sub in subs {
            std::fs::create_dir_all(format!("{d}/out/{o}{sub}")).map_err(|e| format!("{d}/out/{o}{sub}: {e}"))?;
        }
    }
    // the view's git folder: on the layer's branch, the project's objects and refs through mnt/common; boxed, its
    // own refs and objects with the project's objects as an alternate (an agent's uid cannot write the project's)
    let g = format!("{d}/git");
    if l.boxed && !Path::new(&format!("{g}/HEAD")).exists() {
        own_git(&g, &format!("{d}/mnt/common"), &common, &l.branch, &l.base, &line_branches(&l.root))?;
    }
    if !Path::new(&format!("{g}/HEAD")).exists() {
        view_git(&g, &format!("{d}/mnt/common"), &common, &l.branch, &l.base)?;
    }
    // the other repositories (N-101): each its base, overlay and git folder; mounted in the order of their paths, so
    // one inside another (a nested checkout the outer one ignores) is mounted on a folder of the outer one's view,
    // made in the outer one's base
    let mut parts = Vec::new();
    let mut part_dirs = Vec::new();
    let mut mounted: Vec<(String, String)> = vec![(l.repo.clone(), b.clone())];
    let mut also = l.also.clone();
    also.sort_by_key(|p| p.repo.len());
    for p in &also {
        let pb = base(coord, &p.repo, &p.base, &[])?;
        if let Some((outer, ob)) = mounted.iter().filter(|(r, _)| p.repo.starts_with(&format!("{r}/"))).max_by_key(|(r, _)| r.len()) {
            let at = format!("{ob}/{}", &p.repo[outer.len() + 1..]);
            std::fs::create_dir_all(&at).map_err(|e| format!("{at}: {e}"))?;
        }
        mounted.push((p.repo.clone(), pb.clone()));
        let pd = format!("{d}/parts/{}", p.name);
        for sub in ["up", "work", "git", "mnt/common"] {
            std::fs::create_dir_all(format!("{pd}/{sub}")).map_err(|e| format!("{pd}/{sub}: {e}"))?;
        }
        let pcommon = git(&p.repo, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
        let pg = format!("{pd}/git");
        if !Path::new(&format!("{pg}/HEAD")).exists() {
            if l.boxed {
                own_git(&pg, &format!("{pd}/mnt/common"), &pcommon, &p.branch, &p.base, &line_branches(&l.root))?;
            } else {
                view_git(&pg, &format!("{pd}/mnt/common"), &pcommon, &p.branch, &p.base)?;
            }
        }
        parts.push(format!("{}|{}|{pb}|{pcommon}", p.name, p.repo));
        part_dirs.extend([pb, format!("{pd}/up"), pg]);
    }
    std::fs::write(format!("{g}/layer"), format!("{}\n", l.id)).map_err(|e| e.to_string())?;
    // conversations: one real folder inside the project, Claude Code's entry and Codex's sessions links to it
    let convs = format!("{}/.conversations", l.repo);
    for sub in ["claude", "codex"] {
        std::fs::create_dir_all(format!("{convs}/{sub}")).map_err(|e| format!("{convs}/{sub}: {e}"))?;
    }
    // boxed, each agent's own home links to it (agent-base's box-enter): the owner's home is not touched
    if !l.boxed {
        notes.extend(link(&format!("{}/.claude/projects/{}", spec.home, claude_entry(&l.repo)), &format!("{convs}/claude")));
        notes.extend(link(&format!("{}/sessions", spec.codex_home), &format!("{convs}/codex")));
    }
    // the holder: mounts in a namespace of its own and holds it
    // boxed: the layer's files, its build outputs and git folder, and the base it stands on, the box group's and
    // writable by it (folders setgid, so what is made in them stays the group's); the conversations folder the
    // owner's and the box group's only (sandbox.rs). Done as root of the namespace, which may change what a sub-uid
    // wrote; only what is not so already.
    let script = r#"set -e
# the index G holds of the tree at T made to fit its files as this namespace sees them, before G is mounted where
# nobody can write it: `read-tree` keeps no stat data, so every `git diff BASE` or `git status` in the view hashed
# every file again (0.3 s for Tovrin's 2128 files at each symbols call; docket-core5, N-101 p1586, p1589). A refresh
# that cannot be made leaves the index as it was, and the view comes up all the same
fit() { GIT_DIR="$1" GIT_WORK_TREE="$2" git -c safe.directory='*' update-index -q --refresh >/dev/null || :; }
if [ -n "$L_BOX" ]; then
  for p in "$L_BASE" "$L_D/up" "$L_D/git" $(for o in $L_OUTPUTS; do printf '%s ' "$L_D/out/$o"; done) $L_PARTDIRS $L_WARM; do
    find "$p" ! -type l ! -gid 1 -exec chgrp 1 {} +
    find "$p" ! -type l ! -perm -g+w -exec chmod g+w {} +
    find "$p" -type d ! -perm -g+s -exec chmod g+s {} +
  done
  for c in "$L_REPO/.conversations" "$L_REPO/.conversations/claude" "$L_REPO/.conversations/codex"; do
    chgrp 1 "$c"; chmod 2770 "$c"
    # with an ACL, chmod's group bits are its mask: the owning group's own entry (r-x on Tovrin's folder, made by the
    # container) still refused the box group's writes, so an agent's transcript was never written (N-101 p1223)
    if command -v setfacl >/dev/null; then setfacl -m g::rwx,d:g::rwx "$c"; fi
  done
fi
mount --bind "$L_COMMON" "$L_D/mnt/common"
mount --bind "$L_REPO/.conversations" "$L_D/mnt/convs"
# the other repositories' git folders, bound before any overlay hides their paths
printf '%s\n' "$L_PARTS" | while IFS='|' read -r n p b c; do [ -z "$n" ] || mount --bind "$c" "$L_D/parts/$n/mnt/common"; done
mount -t overlay overlay -o "userxattr,lowerdir=$L_BASE,upperdir=$L_D/up,workdir=$L_D/work" "$L_REPO"
mount --bind "$L_D/mnt/convs" "$L_REPO/.conversations"
# each build output: an overlay on its pinned outputs when it starts warm (warm.rs), else its own empty folder
for o in $L_OUTPUTS; do
  case " $L_WARMS " in
    *" $o "*) mount -t overlay overlay -o "userxattr,lowerdir=$L_WARM/$o,upperdir=$L_D/out/$o/up,workdir=$L_D/out/$o/work" "$L_REPO/$o" ;;
    *) mount --bind "$L_D/out/$o" "$L_REPO/$o" ;;
  esac
done
printf '%s\n' "$L_PARTS" | while IFS='|' read -r n p b c; do
  [ -n "$n" ] || continue
  mount -t overlay overlay -o "userxattr,lowerdir=$b,upperdir=$L_D/parts/$n/up,workdir=$L_D/parts/$n/work" "$p"
  fit "$L_D/parts/$n/git" "$p"
  mount --bind "$L_D/parts/$n/git" "$p/.git"
done
# the files that differ from the warm commit made newer than the warm outputs, once
if [ -s "$L_D/touch" ]; then
  (cd "$L_REPO" && xargs -0 touch -c -- < "$L_D/touch")
  rm -f "$L_D/touch"
fi
fit "$L_D/git" "$L_REPO"
# last: the project's own git folder, by which coordd sees the view is up (`holds`), so all of it is there by then
mount --bind "$L_D/git" "$L_REPO/.git"
exec sleep infinity"#;
    let log = std::fs::File::create(format!("{d}/holder.log")).map_err(|e| e.to_string())?;
    let mut cmd = Command::new("unshare");
    if l.boxed {
        cmd.args(crate::sandbox::map_args()?).env("L_BOX", "1");
    } else {
        cmd.args(["--user", "--map-root-user"]).env("L_BOX", "");
    }
    cmd.args(["--mount", "--", "sh", "-c", script, "layer"])
        .env("L_COMMON", &common)
        .env("L_D", &d)
        .env("L_REPO", &l.repo)
        .env("L_BASE", &b)
        .env("L_OUTPUTS", l.outputs.join(" "))
        .env("L_PARTS", parts.join("\n"))
        .env("L_PARTDIRS", part_dirs.join(" "))
        .env("L_WARMS", warms.join(" "))
        .env("L_WARM", warm_dir.as_deref().unwrap_or(""))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log);
    // its own session: a restarted coordd leaves the view standing, as it leaves the agents in it
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = cmd.spawn().map_err(|e| format!("unshare: {e}"))?;
    let pid = child.id() as i32;
    // up once its git folder shows through it; failed if it ends first (no time limit: it does one or the other)
    loop {
        if holds(pid, &l.repo, &l.id) {
            break;
        }
        if let Ok(Some(s)) = child.try_wait() {
            let why = std::fs::read_to_string(format!("{d}/holder.log")).unwrap_or_default();
            return Err(format!("the view of {} did not come up ({s}): {}", l.id, why.trim()));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    l.holder = Some(pid);
    l.view = Some(format!("/proc/{pid}/root{}", l.repo));
    l.state = "open".into();
    l.notes = notes;
    reg.insert(l.id.clone(), l.clone());
    save(coord, &reg)?;
    // what is in it now, for the edits found later (found.rs)
    crate::found::opened(coord, &l);
    Ok(l)
}

/// What runs CMD in the view HOLDER holds, at DIR, as the calling user: nsenter into the holder's namespaces, then a
/// user namespace mapping the calling user back, then into DIR (roster's ROSTER_VIEW does the same for an agent).
/// Not `nsenter --wd=DIR`: it opens DIR before it enters, so the process would be in the project's own folder under
/// the view, and write there (seen on 14 Nov). Used by the test; roster writes the same in shell.
#[cfg(test)]
pub fn joiner(holder: i32, dir: &str) -> Vec<String> {
    let (u, g) = unsafe { (libc::getuid(), libc::getgid()) };
    ["nsenter", "--user", "--mount", "--preserve-credentials", "-t", &holder.to_string(), "--", "unshare", "--user", &format!("--map-user={u}"), &format!("--map-group={g}"), "--", "env", "-C", dir, "--"].iter().map(|s| s.to_string()).collect()
}

/// The processes in the mount namespace MNT (a `/proc/PID/ns/mnt` link's target) or in the user namespace USER: a
/// boxed agent is in a mount namespace of its own under the view's, in the view's user namespace.
fn in_namespace(mnt: &str, user: Option<&str>) -> Vec<i32> {
    let Ok(rd) = std::fs::read_dir("/proc") else { return Vec::new() };
    let is = |p: i32, ns: &str, want: &str| std::fs::read_link(format!("/proc/{p}/ns/{ns}")).is_ok_and(|t| t.to_string_lossy() == want);
    rd.flatten()
        .filter_map(|e| e.file_name().to_str().and_then(|n| n.parse::<i32>().ok()))
        .filter(|p| is(*p, "mnt", mnt) || user.is_some_and(|u| is(*p, "user", u)))
        .collect()
}

/// End layer ID: its holder stopped, the view gone with the last process in it, the layer read-only. Uncommitted
/// files stay in it. Blocking: waits for the processes still in the view (its agents being stopped) to end.
pub fn end(coord: &str, id: &str, now: &str) -> Result<Layer, String> {
    let (holder, repo, boxed) = {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let reg = load(coord);
        let l = reg.get(id).ok_or_else(|| format!("no layer {id}"))?;
        if l.state == "ended" {
            return Ok(l.clone());
        }
        (l.holder, l.repo.clone(), l.boxed)
    };
    // boxed: its branch brought in while the view still stands
    if let Err(e) = sync(coord, id) {
        eprintln!("coordd: layers: {id}: {e}");
    }
    if let Some(h) = holder.filter(|h| holds(*h, &repo, id)) {
        let ns = std::fs::read_link(format!("/proc/{h}/ns/mnt")).map(|t| t.to_string_lossy().into_owned()).ok();
        let user = boxed.then(|| std::fs::read_link(format!("/proc/{h}/ns/user")).map(|t| t.to_string_lossy().into_owned()).ok()).flatten();
        // the holder is coordd's own; nothing else is signalled
        unsafe { libc::kill(h, libc::SIGTERM) };
        if let Some(ns) = ns {
            let mut said = false;
            loop {
                let left = in_namespace(&ns, user.as_deref());
                if left.is_empty() {
                    break;
                }
                if !said {
                    eprintln!("coordd: layers: {id} waits for {left:?} to leave its view");
                    said = true;
                }
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
        }
    }
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut reg = load(coord);
    let l = reg.get_mut(id).ok_or_else(|| format!("no layer {id}"))?;
    let out = format!("{}/{id}/out", dir(coord));
    if l.boxed {
        // what its agents wrote is theirs: frozen and removed as root of a namespace with the box's map
        if let Err(e) = crate::sandbox::in_ns("find \"$1\" ! -type l -exec chmod a-w {} + && rm -rf \"$2\"", &[&l.up, &out]) {
            eprintln!("coordd: layers: {id}: freezing its layer: {e}");
        }
        for p in &l.also {
            if let Err(e) = crate::sandbox::in_ns("find \"$1\" ! -type l -exec chmod a-w {} +", &[&format!("{}/{id}/parts/{}/up", dir(coord), p.name)]) {
                eprintln!("coordd: layers: {id}: freezing its part {}: {e}", p.name);
            }
        }
    } else {
        read_only(Path::new(&l.up));
        for p in &l.also {
            read_only(Path::new(&format!("{}/{id}/parts/{}/up", dir(coord), p.name)));
        }
        // its build outputs go with the view (N-89.p4: each view its own, never shared); a warm one's overlay work
        // folder has no bits
        if Path::new(&out).exists() {
            let _ = remove_tree(&out);
        }
    }
    l.state = "ended".into();
    l.holder = None;
    l.view = None;
    l.ended = Some(now.to_string());
    let l = l.clone();
    save(coord, &reg)?;
    Ok(l)
}

/// The repositories of layer L, each with its branch, its base and what it was started on (N-101): the project's own
/// first, then the others; `remove` treats each alike.
fn branches(l: &Layer) -> Vec<(String, String, String, Option<String>)> {
    let first = (l.repo.clone(), l.branch.clone(), l.base.clone(), l.on.clone().filter(|o| !o.trim_start().starts_with('{')));
    std::iter::once(first).chain(l.also.iter().map(|p| (p.repo.clone(), p.branch.clone(), p.base.clone(), p.on.clone()))).collect()
}

/// Whether NAME (a layer's id, a base's commit) is one plain path component: what `remove` joins to the layers'
/// folder is never anything else.
fn plain(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/')
}

/// Folder P removed, the owner's: its bits given back first (an ended layer is read-only, the overlay's work folder
/// has none).
fn remove_tree(p: &str) -> Result<(), String> {
    let o = Command::new("chmod").args(["-R", "u+rwX", "--", p]).stdin(Stdio::null()).output().map_err(|e| format!("chmod: {e}"))?;
    if !o.status.success() {
        return Err(format!("chmod: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    std::fs::remove_dir_all(p).map_err(|e| e.to_string())
}

/// Remove layer ID, its ticket closed (plan/layers.md, removal; paper-research N-95.p5): ended first if it was not
/// (its branch brought in when boxed, its view gone), then
/// - its folder `$COORD/layers/ID` (boxed: as root of a namespace with the box's map, what its agents wrote being
///   theirs);
/// - its branch, when it has no commit of its own (its head is its base) or its head is contained in its start (`on`)
///   or the root; else it is kept (`kept`) until TOP_CLOSED, the top ticket of its tree closed, and goes then. Only
///   the ref the registry names, never the start branch or the root, deleted only if it is still the head just read
///   (`git update-ref -d REF OLD`);
/// - its base, when no layer that is not removed stands on it.
/// Only paths and refs layers.json records. What fails goes to the journal and into `left`: called again (the next
/// removal, coordd's start), it does what is still to do. Boxed agents' homes and uids stay; the conversations folder
/// is never touched (it is the project's, not the layer's). What it did, said.
pub fn remove(coord: &str, id: &str, top_closed: bool, now: &str) -> Result<String, String> {
    end(coord, id, now)?;
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut reg = load(coord);
    let mut l = reg.get(id).cloned().ok_or_else(|| format!("no layer {id}"))?;
    let (mut done, mut left) = (Vec::new(), Vec::new());
    // its folder
    let d = format!("{}/{id}", dir(coord));
    if !plain(id) {
        left.push(format!("{id} is no plain name: its folder is left"));
    } else if Path::new(&d).exists() {
        let r = if l.boxed { crate::sandbox::in_ns("chmod -R u+rwX -- \"$1\" && rm -rf -- \"$1\"", &[&d]) } else { remove_tree(&d) };
        match r {
            Ok(()) => done.push(format!("{d} removed")),
            Err(e) => left.push(format!("{d}: {e}")),
        }
    }
    if l.removed.is_none() && !Path::new(&d).exists() {
        l.removed = Some(now.to_string());
    }
    // its branches
    let mut kept = false;
    for (repo, b, base, on) in branches(&l) {
        if !b.starts_with("layer/") || on.as_deref() == Some(b.as_str()) || l.root.as_deref() == Some(b.as_str()) {
            left.push(format!("{b} in {repo} is not a layer's own branch: left"));
            continue;
        }
        let r = format!("refs/heads/{b}");
        // gone already: nothing to do
        let Ok(head) = git(&repo, &["rev-parse", "--verify", "--quiet", &format!("{r}^{{commit}}")]) else { continue };
        let within = |rev: &Option<String>| rev.as_ref().is_some_and(|rev| git(&repo, &["merge-base", "--is-ancestor", &head, rev]).is_ok());
        let why = if head == base {
            Some("no commit of its own")
        } else if within(&on) {
            Some("contained in its start")
        } else if within(&l.root) {
            Some("contained in the root")
        } else if top_closed {
            Some("the top ticket of its tree closed")
        } else {
            None
        };
        match why {
            Some(w) => match git(&repo, &["update-ref", "-d", &r, &head]) {
                Ok(_) => done.push(format!("{b} deleted at {} ({w})", &head[..head.len().min(9)])),
                Err(e) => left.push(format!("{b} in {repo}: {e}")),
            },
            None => {
                kept = true;
                done.push(format!("{b} kept at {} (commits on no line, until the top ticket of its tree closes)", &head[..head.len().min(9)]));
            }
        }
    }
    l.kept = kept;
    // its bases, each when nothing else stands on it
    for base in std::iter::once(l.base.clone()).chain(l.also.iter().map(|p| p.base.clone())) {
        let stands = reg.values().any(|o| o.id != l.id && o.removed.is_none() && (o.base == base || o.also.iter().any(|p| p.base == base)))
            || load_lines(coord).values().any(|o| o.base == base || o.also.iter().any(|p| p.base == base));
        let b = format!("{}/bases/{base}", dir(coord));
        if l.removed.is_some() && !stands && plain(&base) && Path::new(&b).exists() {
            match remove_tree(&b) {
                Ok(()) => done.push(format!("{b} removed")),
                Err(e) => left.push(format!("{b}: {e}")),
            }
        }
    }
    // its warm outputs, when no other layer that is not removed names them
    if let Some(w) = l.warm.clone() {
        let named = reg.values().any(|o| o.id != l.id && o.removed.is_none() && o.warm.as_deref() == Some(w.as_str()));
        let p = format!("{}/warm/{w}", dir(coord));
        if l.removed.is_some() && !named && plain(&w) && Path::new(&p).exists() {
            match crate::warm::remove(&p) {
                Ok(()) => done.push(format!("{p} removed")),
                Err(e) => left.push(format!("{p}: {e}")),
            }
        }
    }
    for e in &left {
        eprintln!("coordd: layers: {id}: removing: {e}");
    }
    l.left = left;
    reg.insert(l.id.clone(), l);
    save(coord, &reg)?;
    Ok(if done.is_empty() { format!("{id}: nothing left to remove") } else { format!("{id}: {}", done.join("; ")) })
}

// ---- the line's view (agents and layers, paper-research N-101 p405, B; docket-core2's and docket-core5's proposal) ----
//
// An agent of a boxed project with no step in a layer stands in the line's view: the tree of the root's head, read
// only, at the project's path, in a holder's namespace as a layer's view is (so a boxed agent is a user of its own
// there too). No overlay to write, no branch, nothing to commit: `git status` and `git log` work, a commit fails on the
// read-only file system. The conversations folder is the project's one, writable, as in every view; each build output
// a folder of the view's own. One view per commit of the line: `line@` and the commit (`line_id`), kept in
// `layers/lines.json`; when the line moves a new one is made, and one no agent stands in any more goes (`line_tidy`).

fn lines_file(coord: &str) -> String {
    format!("{}/lines.json", dir(coord))
}

/// Every line's view as `lines.json` keeps it.
pub fn load_lines(coord: &str) -> BTreeMap<String, Layer> {
    std::fs::read_to_string(lines_file(coord)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save_lines(coord: &str, reg: &BTreeMap<String, Layer>) -> Result<(), String> {
    let p = lines_file(coord);
    let tmp = format!("{p}.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(reg).unwrap()).map_err(|e| format!("{tmp}: {e}"))?;
    std::fs::rename(&tmp, &p).map_err(|e| format!("{p}: {e}"))
}

/// Whether ID names a line's view (`line@...`), not a layer.
pub fn is_line(id: &str) -> bool {
    id.starts_with("line@")
}

/// The id of the line's view at commits SHAS, the project's own first: `line@` and its first twelve characters, and
/// with other repositories `+` and a hash of all of them (one moves without the other).
fn line_id(shas: &[String]) -> String {
    let own = &shas[0][..shas[0].len().min(12)];
    if shas.len() == 1 {
        return format!("line@{own}");
    }
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    shas.hash(&mut h);
    format!("line@{own}+{:08x}", h.finish() as u32)
}

/// A read-only git folder G for the line's view at commit SHA: HEAD detached there, the project's objects and refs
/// through INSIDE (bound in the view's namespace), an index of the commit.
fn line_git(g: &str, inside: &str, common: &str, sha: &str) -> Result<(), String> {
    std::fs::write(format!("{g}/HEAD"), format!("{sha}\n")).map_err(|e| e.to_string())?;
    std::fs::write(format!("{g}/commondir"), format!("{inside}\n")).map_err(|e| e.to_string())?;
    let o = Command::new("git").args(["read-tree", sha]).env("GIT_DIR", g).env("GIT_COMMON_DIR", common).stdin(Stdio::null()).output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(format!("the line view's index: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    Ok(())
}

/// The line's view of SPEC's project at the root's head now (SPEC's `on` and `id` are not read), open: made if it is
/// not, its holder made again if it is gone. Blocking: it waits for the view to be up.
/// The line's heads as SPEC names them (the root branch in the project's repository and each other one), the project's
/// first.
fn line_shas(spec: &Spec) -> Result<Vec<String>, String> {
    let root = spec.root.clone().ok_or("the project file sets no root branch, so there is no line to view")?;
    let mut shas = Vec::new();
    for repo in std::iter::once(&spec.repo).chain(spec.also.iter().map(|(_, r)| r)) {
        shas.push(git(repo, &["rev-parse", "--verify", "--quiet", &format!("{root}^{{commit}}")]).map_err(|_| format!("{root} is no branch of {repo}"))?);
    }
    Ok(shas)
}

/// The id of the line's view as the line is now (what `line_view` would make or find), without making it: an agent
/// standing in another `line@` id stands in an older line (moves.rs `move_due`, paper-research N-101 p1528).
pub fn line_now(spec: &Spec) -> Result<String, String> {
    line_shas(spec).map(|s| line_id(&s))
}

pub fn line_view(coord: &str, spec: &Spec, now: &str) -> Result<Layer, String> {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = spec.root.clone().ok_or("the project file sets no root branch, so there is no line to view")?;
    let shas = line_shas(spec)?;
    let id = line_id(&shas);
    let mut reg = load_lines(coord);
    if let Some(l) = reg.get(&id).filter(|l| l.holder.is_some_and(|h| holds(h, &l.repo, &l.id))) {
        return Ok(l.clone());
    }
    if git(&spec.repo, &["check-ignore", "-q", ".conversations/x"]).is_err() {
        return Err(format!("{} does not ignore .conversations/, so a commit in a view would take the conversations", spec.repo));
    }
    let d = format!("{}/{id}", dir(coord));
    let also: Vec<Part> = spec.also.iter().zip(&shas[1..]).map(|((name, repo), sha)| Part { name: name.clone(), repo: repo.clone(), base: sha.clone(), branch: String::new(), on: Some(root.clone()) }).collect();
    let b = base(coord, &spec.repo, &shas[0], &spec.outputs)?;
    let common = git(&spec.repo, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    for sub in ["git", "empty", "mnt/common", "mnt/convs"] {
        std::fs::create_dir_all(format!("{d}/{sub}")).map_err(|e| format!("{d}/{sub}: {e}"))?;
    }
    for o in &spec.outputs {
        std::fs::create_dir_all(format!("{d}/out/{o}")).map_err(|e| format!("{d}/out/{o}: {e}"))?;
    }
    let g = format!("{d}/git");
    if !Path::new(&format!("{g}/HEAD")).exists() {
        line_git(&g, &format!("{d}/mnt/common"), &common, &shas[0])?;
    }
    std::fs::write(format!("{g}/layer"), format!("{id}\n")).map_err(|e| e.to_string())?;
    let mut parts = Vec::new();
    let mut part_gits = Vec::new();
    let mut mounted: Vec<(String, String)> = vec![(spec.repo.clone(), b.clone())];
    let mut sorted = also.clone();
    sorted.sort_by_key(|p| p.repo.len());
    for p in &sorted {
        let pb = base(coord, &p.repo, &p.base, &[])?;
        if let Some((outer, ob)) = mounted.iter().filter(|(r, _)| p.repo.starts_with(&format!("{r}/"))).max_by_key(|(r, _)| r.len()) {
            let at = format!("{ob}/{}", &p.repo[outer.len() + 1..]);
            std::fs::create_dir_all(&at).map_err(|e| format!("{at}: {e}"))?;
        }
        mounted.push((p.repo.clone(), pb.clone()));
        let pd = format!("{d}/parts/{}", p.name);
        for sub in ["git", "mnt/common"] {
            std::fs::create_dir_all(format!("{pd}/{sub}")).map_err(|e| format!("{pd}/{sub}: {e}"))?;
        }
        let pcommon = git(&p.repo, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
        let pg = format!("{pd}/git");
        if !Path::new(&format!("{pg}/HEAD")).exists() {
            line_git(&pg, &format!("{pd}/mnt/common"), &pcommon, &p.base)?;
        }
        parts.push(format!("{}|{}|{pb}|{pcommon}", p.name, p.repo));
        part_gits.push(pg);
    }
    let convs = format!("{}/.conversations", spec.repo);
    for sub in ["claude", "codex"] {
        std::fs::create_dir_all(format!("{convs}/{sub}")).map_err(|e| format!("{convs}/{sub}: {e}"))?;
    }
    if !spec.boxed {
        for n in link(&format!("{}/.claude/projects/{}", spec.home, claude_entry(&spec.repo)), &format!("{convs}/claude")).into_iter().chain(link(&format!("{}/sessions", spec.codex_home), &format!("{convs}/codex"))) {
            eprintln!("coordd: layers: {id}: {n}");
        }
    }
    // read-only: each tree an overlay of its base over an empty folder, with no upper folder to write; the git folders
    // the same; the conversations folder and the build outputs bound writable over it
    let script = r#"set -e
# the index G holds of the tree at T made to fit its files as this namespace sees them, before G is mounted where
# nobody can write it: `read-tree` keeps no stat data, so every `git diff BASE` or `git status` in the view hashed
# every file again (0.3 s for Tovrin's 2128 files at each symbols call; docket-core5, N-101 p1586, p1589). A refresh
# that cannot be made leaves the index as it was, and the view comes up all the same
fit() { GIT_DIR="$1" GIT_WORK_TREE="$2" git -c safe.directory='*' update-index -q --refresh >/dev/null || :; }
if [ -n "$L_BOX" ]; then
  for p in $(for o in $L_OUTPUTS; do printf '%s ' "$L_D/out/$o"; done); do
    find "$p" ! -type l ! -gid 1 -exec chgrp 1 {} +
    find "$p" ! -type l ! -perm -g+w -exec chmod g+w {} +
    find "$p" -type d ! -perm -g+s -exec chmod g+s {} +
  done
  for c in "$L_REPO/.conversations" "$L_REPO/.conversations/claude" "$L_REPO/.conversations/codex"; do
    chgrp 1 "$c"; chmod 2770 "$c"
    # with an ACL, chmod's group bits are its mask: the owning group's own entry (r-x on Tovrin's folder, made by the
    # container) still refused the box group's writes, so an agent's transcript was never written (N-101 p1223)
    if command -v setfacl >/dev/null; then setfacl -m g::rwx,d:g::rwx "$c"; fi
  done
fi
mount --bind "$L_COMMON" "$L_D/mnt/common"
mount --bind "$L_REPO/.conversations" "$L_D/mnt/convs"
printf '%s\n' "$L_PARTS" | while IFS='|' read -r n p b c; do [ -z "$n" ] || mount --bind "$c" "$L_D/parts/$n/mnt/common"; done
mount -t overlay overlay -o "userxattr,lowerdir=$L_BASE:$L_D/empty" "$L_REPO"
mount --bind "$L_D/mnt/convs" "$L_REPO/.conversations"
for o in $L_OUTPUTS; do mount --bind "$L_D/out/$o" "$L_REPO/$o"; done
printf '%s\n' "$L_PARTS" | while IFS='|' read -r n p b c; do
  [ -n "$n" ] || continue
  mount -t overlay overlay -o "userxattr,lowerdir=$b:$L_D/empty" "$p"
  fit "$L_D/parts/$n/git" "$p"
  mount -t overlay overlay -o "userxattr,lowerdir=$L_D/parts/$n/git:$L_D/empty" "$p/.git"
done
fit "$L_D/git" "$L_REPO"
mount -t overlay overlay -o "userxattr,lowerdir=$L_D/git:$L_D/empty" "$L_REPO/.git"
exec sleep infinity"#;
    let log = std::fs::File::create(format!("{d}/holder.log")).map_err(|e| e.to_string())?;
    let mut cmd = Command::new("unshare");
    if spec.boxed {
        cmd.args(crate::sandbox::map_args()?).env("L_BOX", "1");
    } else {
        cmd.args(["--user", "--map-root-user"]).env("L_BOX", "");
    }
    cmd.args(["--mount", "--", "sh", "-c", script, "line"])
        .env("L_COMMON", &common)
        .env("L_D", &d)
        .env("L_REPO", &spec.repo)
        .env("L_BASE", &b)
        .env("L_OUTPUTS", spec.outputs.join(" "))
        .env("L_PARTS", parts.join("\n"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log);
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = cmd.spawn().map_err(|e| format!("unshare: {e}"))?;
    let pid = child.id() as i32;
    loop {
        if holds(pid, &spec.repo, &id) {
            break;
        }
        if let Ok(Some(st)) = child.try_wait() {
            let why = std::fs::read_to_string(format!("{d}/holder.log")).unwrap_or_default();
            return Err(format!("the line's view {id} did not come up ({st}): {}", why.trim()));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    let l = Layer {
        id: id.clone(),
        ticket: String::new(),
        repo: spec.repo.clone(),
        base: shas[0].clone(),
        on: Some(root.clone()),
        branch: String::new(),
        state: "open".into(),
        holder: Some(pid),
        view: Some(format!("/proc/{pid}/root{}", spec.repo)),
        up: String::new(),
        outputs: spec.outputs.clone(),
        at: now.to_string(),
        ended: None,
        notes: Vec::new(),
        boxed: spec.boxed,
        root: Some(root),
        removed: None,
        kept: false,
        left: Vec::new(),
        also,
        warm: None,
    };
    reg.insert(id, l.clone());
    save_lines(coord, &reg)?;
    Ok(l)
}

/// The line's views but KEEP that no process stands in any more (only their holders): their holders stopped, their
/// folders removed, their bases with them when no layer or other line's view stands on them. What went, said.
pub fn line_tidy(coord: &str, keep: &str) -> Vec<String> {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut reg = load_lines(coord);
    let mut said = Vec::new();
    let ids: Vec<String> = reg.keys().filter(|id| id.as_str() != keep).cloned().collect();
    for id in ids {
        let l = reg[&id].clone();
        if let Some(h) = l.holder.filter(|h| holds(*h, &l.repo, &l.id)) {
            let Ok(ns) = std::fs::read_link(format!("/proc/{h}/ns/mnt")).map(|t| t.to_string_lossy().into_owned()) else { continue };
            let user = l.boxed.then(|| std::fs::read_link(format!("/proc/{h}/ns/user")).map(|t| t.to_string_lossy().into_owned()).ok()).flatten();
            if in_namespace(&ns, user.as_deref()).iter().any(|p| *p != h) {
                continue;
            }
            unsafe { libc::kill(h, libc::SIGTERM) };
        }
        reg.remove(&id);
        let d = format!("{}/{id}", dir(coord));
        if plain(&id) && Path::new(&d).exists() {
            let r = if l.boxed { crate::sandbox::in_ns("chmod -R u+rwX -- \"$1\" && rm -rf -- \"$1\"", &[&d]) } else { remove_tree(&d) };
            if let Err(e) = r {
                said.push(format!("{d}: {e}"));
            }
        }
        let layers = load(coord);
        for base in std::iter::once(l.base.clone()).chain(l.also.iter().map(|p| p.base.clone())) {
            let stands = layers.values().any(|o| o.removed.is_none() && (o.base == base || o.also.iter().any(|p| p.base == base))) || reg.values().any(|o| o.base == base || o.also.iter().any(|p| p.base == base));
            let b = format!("{}/bases/{base}", dir(coord));
            if !stands && plain(&base) && Path::new(&b).exists() {
                if let Err(e) = remove_tree(&b) {
                    said.push(format!("{b}: {e}"));
                }
            }
        }
        said.push(format!("{id} removed: no agent stands in it"));
    }
    if let Err(e) = save_lines(coord, &reg) {
        said.push(e);
    }
    said
}

/// The files left uncommitted in open layer ID's view (`git status --porcelain`, each repository's, another one's
/// files after its name), read as root of the view's namespace; none when it is not open.
pub fn uncommitted(coord: &str, id: &str) -> Result<Vec<String>, String> {
    let l = match load(coord).get(id) {
        Some(l) if l.state == "open" => l.clone(),
        _ => return Ok(Vec::new()),
    };
    let Some(h) = l.holder.filter(|h| holds(*h, &l.repo, &l.id)) else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    for (name, repo) in std::iter::once((String::new(), l.repo.clone())).chain(l.also.iter().map(|p| (p.name.clone(), p.repo.clone()))) {
        let o = Command::new("nsenter")
            .args(["--user", "--mount", "--preserve-credentials", "-t", &h.to_string(), "--", "git", "-C", &repo, "-c", "safe.directory=*", "status", "--porcelain", "--untracked-files=all"])
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("nsenter: {e}"))?;
        if !o.status.success() {
            return Err(format!("git status in {id}'s view of {repo}: {}", String::from_utf8_lossy(&o.stderr).trim()));
        }
        for line in String::from_utf8_lossy(&o.stdout).lines().filter(|l| l.len() > 3) {
            let f = &line[3..];
            out.push(if name.is_empty() { f.to_string() } else { format!("{name}: {f}") });
        }
    }
    Ok(out)
}

/// Every file and folder under P without its write bits (links are left; a deletion marker is a device file and
/// takes the mode as any file).
fn read_only(p: &Path) {
    let Ok(m) = std::fs::symlink_metadata(p) else { return };
    if m.file_type().is_symlink() {
        return;
    }
    if m.is_dir() {
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                read_only(&e.path());
            }
        }
    }
    let mode = m.permissions().mode() & !0o222;
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode));
}

/// The most of a layer's own files `Request::Layers` names (`wrote`), the largest first.
pub const WROTE: usize = 50;

/// The most entries of a layer's own folder walked to measure it: past this, its `up_bytes` counts those walked and
/// `up_partial` says so.
const WALKED: usize = 200_000;

/// How long `Request::Layers` keeps a layer's measure of its own folder before it walks the folder again (consult-lead
/// N-1446 p2980: Home asking often is not a walk of every open layer's files at each ask). The answer says when the
/// measure was made (`up_as_of`), as the store's rows say when they were read.
pub const MEASURE_SEC: u64 = 60;

/// What [`measure_up`] finds: the bytes, the largest files, whether the walk stopped short.
type Measure = (u64, Vec<(String, u64)>, bool);

/// Each open layer's last measure, by its `up`: when it was made (to age it, and as text) and what it found.
static MEASURED: Mutex<BTreeMap<String, (std::time::Instant, String, Measure)>> = Mutex::new(BTreeMap::new());

/// UP's measure: the one kept, while it is younger than MEASURE_SEC; else made now and kept. With when it was made.
fn measured(up: &str) -> (Measure, String) {
    if let Some((at, as_of, m)) = MEASURED.lock().unwrap_or_else(|e| e.into_inner()).get(up) {
        if at.elapsed().as_secs() < MEASURE_SEC {
            return (m.clone(), as_of.clone());
        }
    }
    let m = measure_up(Path::new(up));
    let as_of = crate::core::now();
    MEASURED.lock().unwrap_or_else(|e| e.into_inner()).insert(up.to_string(), (std::time::Instant::now(), as_of.clone(), m.clone()));
    (m, as_of)
}

/// A layer's own folder UP measured (Home's Layers now, a2h-speed N-1446 p2815 point 2; what it wrote, docket-edge8
/// p2957): its files' bytes summed, the largest WROTE of them by their path in it, and whether the walk stopped at
/// WALKED. A link counts as itself and is never followed; a folder that is not there is nothing.
pub fn measure_up(up: &Path) -> (u64, Vec<(String, u64)>, bool) {
    let (mut total, mut files, mut seen, mut todo) = (0u64, Vec::new(), 0usize, vec![up.to_path_buf()]);
    let mut partial = false;
    'walk: while let Some(d) = todo.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            seen += 1;
            if seen > WALKED {
                partial = true;
                break 'walk;
            }
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() {
                todo.push(e.path());
            } else {
                total += m.len();
                let rel = e.path().strip_prefix(up).map(|r| r.display().to_string()).unwrap_or_default();
                files.push((rel, m.len()));
            }
        }
    }
    files.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    files.truncate(WROTE);
    (total, files, partial)
}

/// What lies under a layer, as one key: the same record to the same line, a binary of the same md5, a tree of the same
/// commit (`shared` counts the open layers on each).
fn under_key(it: &serde_json::Value) -> String {
    match it["what"].as_str() {
        Some("record") => format!("record {} {}", it["record"], it["line"]),
        Some("binary") => format!("binary {}", it["md5"]),
        Some("tree") => format!("tree {}", it["commit"]),
        _ => String::new(),
    }
}

/// `Request::Layers`'s answer (Home's Layers now, a2h-speed N-1446 p2815; docket-edge8's names p2904, agreed p2927):
/// each layer of REG as layers.json keeps it, with `under`, what lies under it (the tree of its base commit); then RUNS,
/// the runner's running replays' layers (`kind` "run", jobs::run_layers). Each open one also says `up_bytes` and
/// `wrote` ([`measure_up`] of its own folder, `up`; `up_partial` when the walk stopped; the measure kept MEASURE_SEC,
/// `up_as_of` when it was made), and each item under it `shared`: how many open layers lie on the same item. Today's
/// fields stay as they are. Walks folders: off the lock.
pub fn answer(reg: BTreeMap<String, Layer>, runs: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    let mut out: Vec<serde_json::Value> = reg
        .into_values()
        .map(|l| {
            let mut v = serde_json::to_value(&l).unwrap_or_default();
            v["under"] = serde_json::json!([{"what": "tree", "commit": l.base}]);
            v
        })
        .chain(runs)
        .collect();
    let open = |v: &serde_json::Value| v["state"] == "open";
    let mut on: BTreeMap<String, u64> = BTreeMap::new();
    let mut ups = std::collections::BTreeSet::new();
    for v in out.iter_mut().filter(|v| open(v)) {
        if let Some(up) = v["up"].as_str().map(str::to_string) {
            let ((bytes, wrote, partial), as_of) = measured(&up);
            v["up_bytes"] = bytes.into();
            v["wrote"] = wrote.into_iter().map(|(path, bytes)| serde_json::json!({"path": path, "bytes": bytes})).collect();
            v["up_as_of"] = as_of.into();
            if partial {
                v["up_partial"] = true.into();
            }
            ups.insert(up);
        }
        for it in v["under"].as_array().into_iter().flatten() {
            *on.entry(under_key(it)).or_default() += 1;
        }
    }
    for v in out.iter_mut().filter(|v| open(v)) {
        for it in v["under"].as_array_mut().into_iter().flatten() {
            it["shared"] = on.get(&under_key(it)).copied().unwrap_or(0).into();
        }
    }
    MEASURED.lock().unwrap_or_else(|e| e.into_inner()).retain(|up, _| ups.contains(up));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_names_a_project_folder_with_dashes() {
        assert_eq!(claude_entry("/home/t/.cache/soft_swarm/x"), "-home-t--cache-soft-swarm-x");
    }

    /// The whole of a layer, without root: a view of a commit at the repository's own path, a commit made in it on
    /// the layer's branch and seen outside, the view gone and the layer read-only once ended. Needs unprivileged user
    /// namespaces (`--ignored`).
    #[test]
    #[ignore]
    fn a_layer_is_a_view_a_branch_and_a_read_only_folder() {
        let t = std::env::temp_dir().join(format!("docket-layer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let (coord, repo) = (t.join("coord").display().to_string(), t.join("repo").display().to_string());
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        let sh = |c: &str| assert!(Command::new("sh").arg("-c").arg(c).current_dir(&repo).status().unwrap().success(), "{c}");
        sh("git init -q -b dev && printf '.conversations/\\ntarget/\\n' > .gitignore && echo hello > a.txt && git add -A && git -c user.name=t -c user.email=t@x commit -qm base");
        let (home, codex_home) = (t.join("home").display().to_string(), t.join("codex-home").display().to_string());
        let spec = Spec { id: "E-3.l1".into(), ticket: "E-3".into(), on: None, repo: repo.clone(), root: Some("dev".into()), outputs: vec!["target".into()], home, codex_home, boxed: false, also: vec![], name: String::new(), warm: None };
        let l = open(&coord, &spec, "now").unwrap();
        assert_eq!(open(&coord, &spec, "now").unwrap(), l, "a second open gives the same view");
        let h = l.holder.unwrap();
        // the holder stopped however the test ends
        struct Stop(i32);
        impl Drop for Stop {
            fn drop(&mut self) {
                if Path::new(&format!("/proc/{}", self.0)).exists() {
                    unsafe { libc::kill(self.0, libc::SIGTERM) };
                }
            }
        }
        let _stop = Stop(h);
        let enter = |c: &str| {
            let j = joiner(h, &repo);
            let o = Command::new(&j[0]).args(&j[1..]).args(["sh", "-c", c]).output().unwrap();
            assert!(o.status.success(), "{c}: {}", String::from_utf8_lossy(&o.stderr));
            String::from_utf8_lossy(&o.stdout).into_owned()
        };
        assert_eq!(enter("id -u").trim(), unsafe { libc::getuid() }.to_string(), "the calling user inside");
        enter("echo more > b.txt && echo built > target/out && git add -A && git -c user.name=t -c user.email=t@x commit -qm 'in the view' && echo loose > c.txt");
        let log = git(&repo, &["log", "--format=%s", &l.branch]).unwrap();
        assert_eq!(log, "in the view\nbase", "the commit is on the branch outside at once");
        assert!(!Path::new(&format!("{repo}/b.txt")).exists(), "the project's own folder is untouched");
        assert!(Path::new(&format!("{}/b.txt", l.up)).exists() && !Path::new(&format!("{}/target/out", l.up)).exists(), "the build output is out of the layer");
        assert_eq!(std::fs::read_to_string(format!("{}/b.txt", l.view.clone().unwrap())).unwrap(), "more\n", "read from outside through the holder");
        assert!(Path::new(&format!("{}/home/.claude/projects/{}", t.display(), claude_entry(&repo))).is_symlink());
        let e = end(&coord, "E-3.l1", "later").unwrap();
        assert_eq!(e.state, "ended");
        assert!(!holds(h, &repo, "E-3.l1"), "the view is gone");
        // the holder ended; its waiting thread reaps it
        while Path::new(&format!("/proc/{h}")).exists() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(Path::new(&format!("{}/c.txt", l.up)).exists(), "uncommitted files stay");
        assert!(std::fs::metadata(format!("{}/c.txt", l.up)).unwrap().permissions().readonly(), "and are read-only");
        assert!(open(&coord, &spec, "again").unwrap_err().contains("has ended"));
        assert!(!Path::new(&format!("{coord}/layers/E-3.l1/out")).exists(), "its build outputs went with the view");
        sh(&format!("chmod -R u+rwx {}", t.display()));
        let _ = std::fs::remove_dir_all(&t);
    }

    /// The line's view (agents and layers, B): the root's head at the project's path, read only (a write and a commit
    /// fail, `git status` and `git log` work), the conversations folder and the build outputs writable; one per commit
    /// of the line, a second call the same; a new one when the line moves, the old one tidied once nobody stands in it,
    /// its base with it. And `uncommitted` lists what a layer's view leaves. Needs unprivileged user namespaces
    /// (`--ignored`).
    #[test]
    #[ignore]
    fn the_lines_view_is_the_roots_head_read_only() {
        let t = std::env::temp_dir().join(format!("docket-line-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let (coord, repo) = (t.join("coord").display().to_string(), t.join("repo").display().to_string());
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        let sh = |c: &str| assert!(Command::new("sh").arg("-c").arg(c).current_dir(&repo).status().unwrap().success(), "{c}");
        let commit = "git add -A && git -c user.name=t -c user.email=t@x commit -qm";
        sh(&format!("git init -q -b dev && printf '.conversations/\\ntarget/\\n' > .gitignore && echo hello > a.txt && {commit} base && git branch landed"));
        let (home, codex_home) = (t.join("home").display().to_string(), t.join("codex-home").display().to_string());
        let spec = Spec { id: String::new(), ticket: String::new(), on: None, repo: repo.clone(), root: Some("landed".into()), outputs: vec!["target".into()], home, codex_home, boxed: false, also: vec![], name: String::new(), warm: None };
        struct Stop(Vec<i32>);
        impl Drop for Stop {
            fn drop(&mut self) {
                for h in &self.0 {
                    if Path::new(&format!("/proc/{h}")).exists() {
                        unsafe { libc::kill(*h, libc::SIGTERM) };
                    }
                }
            }
        }
        let l = line_view(&coord, &spec, "now").unwrap();
        assert!(is_line(&l.id) && l.id.starts_with("line@"), "{}", l.id);
        assert_eq!(line_view(&coord, &spec, "now").unwrap(), l, "a second call gives the same view");
        let h = l.holder.unwrap();
        let mut stop = Stop(vec![h]);
        let run = |h: i32, c: &str| {
            let j = joiner(h, &repo);
            Command::new(&j[0]).args(&j[1..]).args(["sh", "-c", c]).output().unwrap()
        };
        let ok = |h: i32, c: &str| {
            let o = run(h, c);
            assert!(o.status.success(), "{c}: {}", String::from_utf8_lossy(&o.stderr));
            String::from_utf8_lossy(&o.stdout).into_owned()
        };
        assert_eq!(ok(h, "cat a.txt"), "hello\n");
        assert_eq!(ok(h, "git log --format=%s"), "base\n", "git log works");
        assert_eq!(ok(h, "git status --porcelain"), "", "git status works, nothing changed");
        // its index fits its files (read-tree's has no stat data, and the view's git folder is read-only): a git diff
        // there does not hash every file again
        let debug = ok(h, "git ls-files --debug a.txt");
        assert!(debug.contains("mtime: ") && !debug.contains("mtime: 0:0"), "{debug}");
        let w = run(h, "echo x > b.txt");
        assert!(!w.status.success() && String::from_utf8_lossy(&w.stderr).contains("Read-only"), "{}", String::from_utf8_lossy(&w.stderr));
        let c = run(h, &format!("echo x > a.txt; {commit} nope"));
        assert!(!c.status.success(), "no commit in the line's view");
        ok(h, "echo built > target/out && echo said > .conversations/claude/s.jsonl");
        assert!(Path::new(&format!("{repo}/.conversations/claude/s.jsonl")).exists(), "the project's one conversations folder");
        assert!(!Path::new(&format!("{repo}/target/out")).exists(), "the outputs are the view's own");
        // the line moves: a new view; the old one goes once nobody stands in it, and its base with it
        sh(&format!("echo more > a.txt && {commit} second && git branch -f landed"));
        let l2 = line_view(&coord, &spec, "later").unwrap();
        assert_ne!(l2.id, l.id);
        stop.0.push(l2.holder.unwrap());
        assert_eq!(ok(l2.holder.unwrap(), "cat a.txt"), "more\n");
        let said = line_tidy(&coord, &l2.id);
        assert!(said.iter().any(|s| s.contains(&l.id)), "{said:?}");
        assert!(!Path::new(&format!("{coord}/layers/bases/{}", l.base)).exists(), "its base went");
        assert!(Path::new(&format!("{coord}/layers/bases/{}", l2.base)).exists());
        assert_eq!(load_lines(&coord).keys().cloned().collect::<Vec<_>>(), vec![l2.id.clone()]);
        // a layer's view: what it leaves uncommitted
        let lspec = Spec { id: "E-3.l1".into(), ticket: "E-3".into(), on: None, ..spec.clone() };
        let ll = open(&coord, &lspec, "now").unwrap();
        stop.0.push(ll.holder.unwrap());
        ok(ll.holder.unwrap(), "echo c > c.txt && echo d > d.txt && git add d.txt && git -c user.name=t -c user.email=t@x commit -qm d && echo e >> a.txt");
        let mut left = uncommitted(&coord, "E-3.l1").unwrap();
        left.sort();
        assert_eq!(left, vec!["a.txt".to_string(), "c.txt".to_string()]);
        // the warning a turn's end that answered there gets on its next delivery (moves.rs, p466, 2)
        crate::moves::mark_answered(&coord, "bob", Some("E-3.l1"));
        let places = crate::policy::Places { base: format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), coord: coord.clone(), ..Default::default() };
        let (said, _) = crate::moves::turn_warning(&places, "bob").unwrap();
        assert!(said.starts_with("Warning: your last turn's end answered your step in layer E-3.l1") && said.contains("a.txt, c.txt"), "{said}");
        end(&coord, "E-3.l1", "later").unwrap();
        assert_eq!(uncommitted(&coord, "E-3.l1").unwrap(), Vec::<String>::new(), "an ended layer: none");
        drop(stop);
        sh(&format!("chmod -R u+rwx {}", t.display()));
        let _ = std::fs::remove_dir_all(&t);
    }

    /// A layer over several repositories (N-101): the project's own, one nested in it (which it ignores) and one beside
    /// it; one view, each at its own path on `layer/ID`, started on commits by repository; commits in each seen outside
    /// at once; removed whole. An `on` that does not name every repository makes no layer; a nested repository the
    /// outer one does not ignore refuses the project file. Needs unprivileged user namespaces (`--ignored`).
    #[test]
    #[ignore]
    fn one_layer_over_several_repositories() {
        let t = std::env::temp_dir().join(format!("docket-repos-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let coord = t.join("coord").display().to_string();
        let (repo, p4, go) = (t.join("tovrin").display().to_string(), t.join("tovrin/p4").display().to_string(), t.join("go").display().to_string());
        for d in [&coord, &repo, &go] {
            std::fs::create_dir_all(d).unwrap();
        }
        let sh = |dir: &str, c: &str| assert!(Command::new("sh").arg("-c").arg(c).current_dir(dir).status().unwrap().success(), "{c}");
        let commit = "git add -A && git -c user.name=t -c user.email=t@x commit -qm";
        sh(&repo, &format!("git init -q -b dev && printf '.conversations/\\np4/\\n' > .gitignore && echo t > t.txt && {commit} base"));
        std::fs::create_dir_all(&p4).unwrap();
        sh(&p4, &format!("git init -q -b dev && echo p > p.txt && {commit} base"));
        sh(&go, &format!("git init -q -b dev && echo g > g.txt && {commit} base && echo g2 > g.txt && {commit} second"));
        let first_go = git(&go, &["rev-parse", "dev~1"]).unwrap();
        // the project file: refused while tovrin does not ignore p4, read once it does
        let files = t.join("files");
        std::fs::create_dir_all(&files).unwrap();
        std::fs::write(files.join("project.toml"), format!("[project]\nrepo = \"{repo}\"\nrepos = {{ tovrin = \"{repo}\", p4 = \"{p4}\", go = \"{go}\" }}\n")).unwrap();
        std::fs::write(format!("{coord}/project"), files.join("project.toml").display().to_string()).unwrap();
        sh(&repo, "printf '.conversations/\\n' > .gitignore");
        assert!(crate::project::Project::load(&coord).unwrap_err().contains("does not ignore it"));
        sh(&repo, "printf '.conversations/\\np4/\\n' > .gitignore");
        let p = crate::project::Project::load(&coord).unwrap().unwrap();
        let (name, also) = p.layer_repos();
        assert_eq!(name, "tovrin");
        assert_eq!(also, vec![("go".to_string(), go.clone()), ("p4".to_string(), p4.clone())]);
        let (home, codex_home) = (t.join("home").display().to_string(), t.join("codex-home").display().to_string());
        let spec = |id: &str, on: &str| Spec { id: id.into(), ticket: "E-7".into(), on: Some(on.into()), repo: repo.clone(), root: Some("dev".into()), outputs: vec![], home: home.clone(), codex_home: codex_home.clone(), boxed: false, also: also.clone(), name: name.clone(), warm: None };
        // commits by repository that leave one out: no layer, and no branch made
        assert!(open(&coord, &spec("E-7.l0", r#"{"tovrin": "dev", "p4": "dev"}"#), "now").unwrap_err().contains("none of the repository go"));
        assert!(git(&go, &["branch", "--list", "layer/*"]).unwrap().is_empty());
        let on = format!(r#"{{"tovrin": "dev", "p4": "dev", "go": "{first_go}"}}"#);
        let l = open(&coord, &spec("E-7.l1", &on), "now").unwrap();
        struct Stop(i32);
        impl Drop for Stop {
            fn drop(&mut self) {
                if Path::new(&format!("/proc/{}", self.0)).exists() {
                    unsafe { libc::kill(self.0, libc::SIGTERM) };
                }
            }
        }
        let h = l.holder.unwrap();
        let _stop = Stop(h);
        assert_eq!(l.also.len(), 2);
        assert_eq!(l.also.iter().find(|p| p.name == "go").unwrap().base, first_go, "go on the commit named for it");
        let enter = |dir: &str, c: &str| {
            let j = joiner(h, dir);
            let o = Command::new(&j[0]).args(&j[1..]).args(["sh", "-c", c]).output().unwrap();
            assert!(o.status.success(), "{c}: {}", String::from_utf8_lossy(&o.stderr));
            String::from_utf8_lossy(&o.stdout).into_owned()
        };
        assert_eq!(enter(&go, "cat g.txt").trim(), "g", "the view of go is its commit, not its head");
        assert_eq!(enter(&p4, "git rev-parse --abbrev-ref HEAD").trim(), "layer/E-7.l1");
        enter(&repo, &format!("echo t2 > t.txt && {commit} in-tovrin && echo p2 > p4/p.txt && cd p4 && {commit} in-p4"));
        enter(&go, &format!("echo g3 > g.txt && {commit} in-go"));
        for (r, m) in [(&repo, "in-tovrin"), (&p4, "in-p4"), (&go, "in-go")] {
            assert_eq!(git(r, &["log", "-1", "--format=%s", "layer/E-7.l1"]).unwrap(), m, "on the branch in {r} at once");
        }
        assert_eq!(std::fs::read_to_string(format!("{p4}/p.txt")).unwrap(), "p\n", "the repositories' own folders untouched");
        let said = remove(&coord, "E-7.l1", true, "later").unwrap();
        assert_eq!(said.matches("deleted").count(), 3, "{said}");
        for r in [&repo, &p4, &go] {
            assert!(git(r, &["branch", "--list", "layer/*"]).unwrap().is_empty(), "{r}");
        }
        assert!(!Path::new(&format!("{coord}/layers/E-7.l1")).exists());
        assert!(load(&coord)["E-7.l1"].left.is_empty());
        sh(&t.display().to_string(), "chmod -R u+rwx .");
        let _ = std::fs::remove_dir_all(&t);
    }

    /// A boxed layer over two repositories (N-101), one nested in the other: an agent's sub-uid commits in both, each
    /// brought in by `sync` to its own repository, and the whole removed by the box's map.
    #[test]
    #[ignore = "mounts in namespaces and needs the owner's sub-uids: cargo test -- --ignored a_boxed_layer"]
    fn a_boxed_layer_over_two_repositories() {
        if crate::sandbox::map_args().is_err() || !Command::new("unshare").args(crate::sandbox::map_args().unwrap()).args(["true"]).status().is_ok_and(|s| s.success()) {
            return eprintln!("no sub-uids to map for this user: skipped");
        }
        let t = std::env::temp_dir().join(format!("docket-boxed-repos-{}", std::process::id()));
        let _ = crate::sandbox::in_ns("rm -rf \"$1\"", &[&t.display().to_string()]);
        let (coord, repo, p4) = (t.join("coord").display().to_string(), t.join("tovrin").display().to_string(), t.join("tovrin/p4").display().to_string());
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::create_dir_all(&p4).unwrap();
        let sh = |dir: &str, c: &str| assert!(Command::new("sh").arg("-c").arg(c).current_dir(dir).status().unwrap().success(), "{c}");
        let commit = "git add -A && git -c user.name=t -c user.email=t@x commit -qm";
        sh(&repo, &format!("git init -q -b dev && printf '.conversations/\\np4/\\n' > .gitignore && echo t > t.txt && {commit} base"));
        sh(&p4, &format!("git init -q -b dev && echo p > p.txt && {commit} base"));
        let (home, codex_home) = (t.join("home").display().to_string(), t.join("codex-home").display().to_string());
        let spec = Spec { id: "E-6.l1".into(), ticket: "E-6".into(), on: None, repo: repo.clone(), root: Some("dev".into()), outputs: vec![], home, codex_home, boxed: true, also: vec![("p4".into(), p4.clone())], name: "tovrin".into(), warm: None };
        let l = open(&coord, &spec, "now").unwrap();
        let h = l.holder.unwrap();
        struct Stop(i32);
        impl Drop for Stop {
            fn drop(&mut self) {
                if Path::new(&format!("/proc/{}", self.0)).exists() {
                    unsafe { libc::kill(self.0, libc::SIGTERM) };
                }
            }
        }
        let _stop = Stop(h);
        let o = Command::new("nsenter")
            .args(["--user", "--mount", "--preserve-credentials", "-t", &h.to_string(), "--", "unshare", "--mount", "--", "setpriv", "--reuid=7", "--regid=7", "--groups=1", "--", "env", "-C", &repo])
            .args(["GIT_CONFIG_COUNT=1", "GIT_CONFIG_KEY_0=safe.directory", "GIT_CONFIG_VALUE_0=*", "HOME=/nonexistent", "sh", "-c"])
            .arg(format!("umask 002; echo t2 > t.txt && {commit} in-tovrin && cd p4 && echo p2 > p.txt && {commit} in-p4"))
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        assert_eq!(git(&p4, &["log", "--format=%s", "layer/E-6.l1"]).unwrap(), "base", "not the project's before it is brought in");
        sync(&coord, "E-6.l1").unwrap();
        assert_eq!(git(&repo, &["log", "-1", "--format=%s", "layer/E-6.l1"]).unwrap(), "in-tovrin");
        assert_eq!(git(&p4, &["log", "-1", "--format=%s", "layer/E-6.l1"]).unwrap(), "in-p4");
        end(&coord, "E-6.l1", "later").unwrap();
        while Path::new(&format!("/proc/{h}")).exists() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let said = remove(&coord, "E-6.l1", true, "later").unwrap();
        assert_eq!(said.matches("deleted").count(), 2, "{said}");
        assert!(!Path::new(&format!("{coord}/layers/E-6.l1")).exists(), "{said}");
        let _ = crate::sandbox::in_ns("rm -rf \"$1\"", &[&t.display().to_string()]);
    }

    /// box-enter itself on a boxed view over two repositories side by side, as docket-try's a and b (docket-core3,
    /// N-101 p536): the agent at the project's repository sees the other one at its own path and writes it, reads both
    /// histories with git, and sees nothing else of the folder that holds them.
    #[test]
    #[ignore = "mounts in namespaces and needs the owner's sub-uids: cargo test -- --ignored box_enter"]
    fn box_enter_binds_the_views_repositories_and_nothing_beside_them() {
        if crate::sandbox::map_args().is_err() || !Command::new("unshare").args(crate::sandbox::map_args().unwrap()).args(["true"]).status().is_ok_and(|s| s.success()) {
            return eprintln!("no sub-uids to map for this user: skipped");
        }
        let t = std::env::temp_dir().join(format!("docket-box-also-{}", std::process::id()));
        let _ = crate::sandbox::in_ns("rm -rf \"$1\"", &[&t.display().to_string()]);
        let at = |p: &str| t.join(p).display().to_string();
        let (coord, a, b, d) = (at("coord"), at("standin/a"), at("standin/b"), at("coord/box/x"));
        for f in [&coord, &a, &b, &format!("{d}/home/.codex"), &format!("{d}/tmp")] {
            std::fs::create_dir_all(f).unwrap();
        }
        std::fs::write(at("standin/secret.txt"), "beside the repositories\n").unwrap();
        // the project's `box_read` (N-101 p863): a runner's builds folder with a case's log, beside a file not named
        std::fs::create_dir_all(at("runner/builds/B-9")).unwrap();
        std::fs::write(at("runner/builds/B-9/case-4.log"), "case-4: red\n").unwrap();
        std::fs::write(at("runner/other.txt"), "beside the builds\n").unwrap();
        // the coord's folder as a coord has it (N-101 p573): of it the box sees only its own parts, the coord's
        // programs (opt/) and the language servers' sockets' folder
        for (f, text) in [("tickets/log.jsonl", "{}\n"), ("registry.jsonl", "{}\n"), ("agents/other/note", "x\n"), ("agents/x/note", "x\n"), ("box/other/home/f", "x\n"), (".proxy-key", "k\n"), ("route", "r\n"), ("codex-home/auth.json", "{}\n"), ("codex-home/config.toml", "\n"), ("views/v/f", "x\n"), ("opt/codex/bin/codex", "x\n"), ("box/x/claude-settings.json", "{}\n"), ("box/x/home/.codex/config.toml", "\n")] {
            let f = format!("{coord}/{f}");
            std::fs::create_dir_all(Path::new(&f).parent().unwrap()).unwrap();
            std::fs::write(&f, text).unwrap();
        }
        // the project's MCP server (a link in mcp/ to a program elsewhere) and the project file $COORD/project names,
        // in a folder of the project's other files
        for (f, text) in [("tools/sym", "#!/bin/sh\necho sym\n"), ("coord/files/project.toml", "[project]\n"), ("coord/files/kinds/k.kind", "x\n")] {
            std::fs::create_dir_all(Path::new(&at(f)).parent().unwrap()).unwrap();
            std::fs::write(at(f), text).unwrap();
        }
        std::fs::set_permissions(at("tools/sym"), std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        std::fs::create_dir_all(format!("{coord}/mcp")).unwrap();
        std::os::unix::fs::symlink(at("tools/sym"), format!("{coord}/mcp/symbols")).unwrap();
        // one that names nothing, through a link in opt/ as Tovrin's `mcp/agda` (N-101 p1218): said, and the box starts
        std::fs::create_dir_all(format!("{coord}/opt")).unwrap();
        std::os::unix::fs::symlink(at("tools/gone/agda-mcp"), format!("{coord}/opt/agda-mcp")).unwrap();
        std::os::unix::fs::symlink(format!("{coord}/opt/agda-mcp"), format!("{coord}/mcp/agda")).unwrap();
        std::fs::write(format!("{coord}/project"), format!("{}\n", at("coord/files/project.toml"))).unwrap();
        std::fs::create_dir_all(format!("{coord}/lsp")).unwrap();
        let _lsp = std::os::unix::net::UnixListener::bind(format!("{coord}/lsp/agda.sock")).unwrap();
        std::fs::set_permissions(format!("{coord}/lsp/agda.sock"), std::os::unix::fs::PermissionsExt::from_mode(0o777)).unwrap();
        let sh = |dir: &str, c: &str| assert!(Command::new("sh").arg("-c").arg(c).current_dir(dir).status().unwrap().success(), "{c}");
        let commit = "git add -A && git -c user.name=t -c user.email=t@x commit -qm";
        sh(&a, &format!("git init -q -b dev && printf '.conversations/\\n' > .gitignore && echo a > a.txt && {commit} base-a"));
        // what a's own exclude ignores (not its .gitignore): the boxed layer's git ignores it too (N-101 p856)
        sh(&a, "printf 'x-*\\n' >> .git/info/exclude");
        // a conversations folder with an ACL whose owning group's entry refuses writing (Tovrin's, N-101 p1223)
        sh(&a, "mkdir -p .conversations/claude && setfacl -m u:$(id -u):rwx,g::r-x,d:g::r-x .conversations .conversations/claude");
        // a's own branches beside dev: its root as the stand-in's runner keeps it, and another layer's (N-101 p935)
        sh(&a, "git branch landed && git branch feature && git branch layer/Q-1.l1");
        sh(&b, &format!("git init -q -b dev && echo b > b.txt && {commit} base-b && git branch landed"));
        let spec = Spec { id: "E-7.l1".into(), ticket: "E-7".into(), on: None, repo: a.clone(), root: Some("landed".into()), outputs: vec![], home: at("home"), codex_home: at("codex-home"), boxed: true, also: vec![("b".into(), b.clone())], name: "a".into(), warm: None };
        let l = open(&coord, &spec, "now").unwrap();
        let h = l.holder.unwrap();
        struct Stop(i32);
        impl Drop for Stop {
            fn drop(&mut self) {
                if Path::new(&format!("/proc/{}", self.0)).exists() {
                    unsafe { libc::kill(self.0, libc::SIGTERM) };
                }
            }
        }
        let _stop = Stop(h);
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let also = l.also.iter().map(|p| p.repo.as_str()).collect::<Vec<_>>().join(":");
        // as roster's open runs it (viewed): the view's user namespace, a mount namespace of its own, box-enter at DIR
        let o = Command::new("nsenter")
            .args(["--user", "--mount", "--preserve-credentials", "-t", &h.to_string(), "--", "unshare", "--mount", "--ipc", "--"])
            .arg(format!("{base}/bin/box-enter"))
            .arg(&a)
            .args(["env", "GIT_CONFIG_COUNT=1", "GIT_CONFIG_KEY_0=safe.directory", "GIT_CONFIG_VALUE_0=*", "sh", "-c"])
            .arg(concat!(
                "ls -A ..; echo two > ../b/b2.txt && git -C ../b log --format=%s && git log --format=%s && { cat ../secret.txt 2>/dev/null || echo unseen; }; ",
                // every folder its uid may write, and what is mounted there (N-101 p564)
                "find / \\( -path /proc -o -path /sys -o -path /usr -o -path /nix -o -path /snap \\) -prune -o -type d -writable -print 2>/dev/null | ",
                "while read -r p; do echo \"W $p $(findmnt -n -o FSTYPE,SOURCE -T \"$p\" | tail -n 1 | tr -s ' ')\"; done; ",
                // the owner's folders outside the system's: empty
                "for e in /personal /app /media /srv; do [ ! -d $e ] || echo \"H $e $(ls -A $e | wc -l)\"; done; ",
                // of the coord's folder: not the record, other agents' parts, the route and its key, codex-home, views;
                // its own parts, opt/ and the language server's socket, which it reaches
                "for p in tickets registry.jsonl agents/other box/other .proxy-key route codex-home views; do [ ! -e \"$COORD/$p\" ] || echo \"C $p seen\"; done; ",
                "for p in agents/x/note box/x/claude-settings.json box/x/home/.codex/config.toml opt/codex/bin/codex lsp/agda.sock; do [ -e \"$COORD/$p\" ] || echo \"C $p missing\"; done; ",
                "python3 -c 'import socket,sys; socket.socket(socket.AF_UNIX).connect(sys.argv[1])' \"$COORD/lsp/agda.sock\" || echo 'C lsp/agda.sock refused'; ",
                // the MCP server runs; symbols' reads: the project file (not its folder's other files), and of the
                // layers' record only its own view's entry
                "[ \"$(\"$COORD/mcp/symbols\")\" = sym ] || echo 'C mcp/symbols does not run'; ",
                // its transcript can be written (the session folder links to the project's conversations folder)
                ": > .conversations/claude/t.jsonl 2>/dev/null || echo 'C the conversations folder refuses its transcript'; ",
                "[ -r \"$COORD/project\" ] && [ -r \"$(head -n 1 \"$COORD/project\")\" ] || echo 'C the project file missing'; ",
                "[ ! -e \"$COORD/files/kinds\" ] || echo 'C files/kinds seen'; ",
                "k=$(jq -r 'keys | join(\",\")' \"$COORD/layers/layers.json\"); [ \"$k\" = E-7.l1 ] || echo \"C layers.json holds $k\"; ",
                "[ \"$(jq -r '.[\"E-7.l1\"].repo' \"$COORD/layers/layers.json\")\" = \"$PWD\" ] || echo 'C layers.json: not its repo'; ",
                // `box_read`: each folder read, not written, and nothing beside it; one that does not exist skipped
                "R=$READ_AT; [ \"$(cat $R/builds/B-9/case-4.log)\" = 'case-4: red' ] || echo 'C box_read not read'; ",
                "! touch $R/builds/w 2>/dev/null || echo 'C box_read written'; [ ! -e $R/other.txt ] || echo 'C beside box_read seen'; ",
                // git in the step's layer: `git log -1` and `git status` in each repository (N-101 p658)
                // the repository's own branches, as in the line's view; not another layer's
                "git diff --quiet landed || echo 'C git diff landed fails'; [ \"$(git log dev -1 --format=%s)\" = base-a ] || echo 'C git log dev -1 fails'; ",
                "echo \"B $(for r in landed dev feature layer/Q-1.l1; do git rev-parse --verify -q $r >/dev/null && printf ' %s' $r; done)\"; ",
                "git switch -q -c from-landed landed && git switch -q - || echo 'C git switch -c from landed fails'; ",
                "echo x > x-one; for r in . ../b; do g=$(git -C $r log -1 --format=%s) || echo \"C git log -1 fails in $r\"; echo \"G $r $g\"; ",
                "s=$(git -C $r status --porcelain) || echo \"C git status fails in $r\"; echo \"S $r $s\"; done"
            ))
            .env("COORD", &coord)
            .env("AGENT_BASE", &base)
            .env("ROSTER_BOX", "7")
            .env("ROSTER_BOX_DIR", &d)
            .env("ROSTER_BOX_ALSO", &also)
            .env("ROSTER_BOX_SOCKETS", format!("{coord}/lsp"))
            .env("ROSTER_BOX_READ", format!("{}:{}", at("runner/builds"), at("runner/none")))
            .env("READ_AT", at("runner"))
            .env("COORD_LAYER", "E-7.l1")
            .output()
            .unwrap();
        let out = String::from_utf8_lossy(&o.stdout).into_owned();
        assert!(o.status.success(), "{out}{}", String::from_utf8_lossy(&o.stderr));
        let err = String::from_utf8_lossy(&o.stderr).into_owned();
        assert!(err.contains("mcp/agda is a link to") && err.contains("which is not there: that MCP server is not in the box"), "{err}");
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[..5], ["a", "b", "base-b", "base-a", "unseen"], "{out}{}", String::from_utf8_lossy(&o.stderr));
        // what it may write is its own: its view (a, b), its box (home, tmp at /tmp and /var/tmp), or a mount the box
        // made for it (box-shm, box-mq, box-w); nothing of the machine's
        let own = |p: &str| [a.as_str(), b.as_str(), d.as_str(), "/tmp", "/var/tmp"].iter().any(|o| p == *o || p.starts_with(&format!("{o}/")));
        let written: Vec<&str> = lines.iter().filter_map(|l| l.strip_prefix("W ")).collect();
        assert!(written.iter().any(|w| w.starts_with("/dev/shm tmpfs box-shm")), "{out}");
        let foreign: Vec<&&str> = written.iter().filter(|w| {
            let mut f = w.split(' ');
            let (p, fs, src) = (f.next().unwrap_or(""), f.next().unwrap_or(""), f.next().unwrap_or(""));
            !(own(p) || (fs == "tmpfs" && src.starts_with("box-")) || (fs == "mqueue" && src == "box-mq"))
        }).collect();
        assert!(foreign.is_empty(), "written outside its own: {foreign:?}");
        for l in lines.iter().filter(|l| l.starts_with("H ")) {
            assert!(l.ends_with(" 0"), "seen in the box: {l}");
        }
        let coord_parts: Vec<&&str> = lines.iter().filter(|l| l.starts_with("C ")).collect();
        assert!(coord_parts.is_empty(), "of the coord's folder: {coord_parts:?}");
        let gits: Vec<&&str> = lines.iter().filter(|l| l.starts_with("G ") || l.starts_with("S ")).collect();
        assert_eq!(lines.iter().filter(|l| l.starts_with("B ")).collect::<Vec<_>>(), [&"B  landed dev"], "a's branches in the step's layer: {out}");
        assert_eq!(gits, [&"G . base-a", &"S . ", &"G ../b base-b", &"S ../b ?? b2.txt"], "git in the step's layer: {out}");
        // and coordd's uncommitted check: b's new file, not a's excluded one
        assert_eq!(uncommitted(&coord, "E-7.l1").unwrap(), ["b: b2.txt"], "uncommitted in E-7.l1");
        eprintln!("{}", written.join("\n"));
        // the line's view, boxed: its git folder reaches the project's history through commondir (docket-core3,
        // N-101 p656), so git reads it there too, a's and b's
        let lv = line_view(&coord, &spec, "now").unwrap();
        let lh = lv.holder.unwrap();
        let _lstop = Stop(lh);
        let lalso = lv.also.iter().map(|p| p.repo.as_str()).collect::<Vec<_>>().join(":");
        let o = Command::new("nsenter")
            .args(["--user", "--mount", "--preserve-credentials", "-t", &lh.to_string(), "--", "unshare", "--mount", "--ipc", "--"])
            .arg(format!("{base}/bin/box-enter"))
            .arg(&a)
            .args(["env", "GIT_CONFIG_COUNT=1", "GIT_CONFIG_KEY_0=safe.directory", "GIT_CONFIG_VALUE_0=*", "sh", "-c", "git log --format=%s && git -C ../b log --format=%s && git log -1 --format=%s && git -C ../b log -1 --format=%s && echo \"S $(git status --porcelain)\" && echo \"S $(git -C ../b status --porcelain)\""])
            .env("COORD", &coord)
            .env("AGENT_BASE", &base)
            .env("ROSTER_BOX", "7")
            .env("ROSTER_BOX_DIR", &d)
            .env("ROSTER_BOX_ALSO", &lalso)
            .env("COORD_LAYER", &lv.id)
            .output()
            .unwrap();
        let out = String::from_utf8_lossy(&o.stdout).into_owned();
        assert!(o.status.success(), "the line's view: {out}{}", String::from_utf8_lossy(&o.stderr));
        assert_eq!(out.lines().collect::<Vec<_>>(), ["base-a", "base-b", "base-a", "base-b", "S ", "S "], "the line's view: {out}");
        // coordd started again under a running box (`dev.sh move`, N-101 p1235): its socket made anew, the box still
        // reaches it (box-enter binds the socket's folder, main.rs link_socket)
        let csock = format!("{coord}/run/coordd.sock");
        std::fs::create_dir_all(format!("{coord}/run")).unwrap();
        let listen = || {
            let l = std::os::unix::net::UnixListener::bind(&csock).unwrap();
            std::fs::set_permissions(&csock, std::os::unix::fs::PermissionsExt::from_mode(0o666)).unwrap();
            crate::link_socket(&csock);
            l
        };
        let first = listen();
        let conn = "python3 -c 'import socket; socket.socket(socket.AF_UNIX).connect(\"/tmp/.coordd.sock\")'";
        let mut boxed = Command::new("nsenter")
            .args(["--user", "--mount", "--preserve-credentials", "-t", &h.to_string(), "--", "unshare", "--mount", "--ipc", "--"])
            .arg(format!("{base}/bin/box-enter"))
            .arg(&a)
            .args(["sh", "-c", &format!("{conn} && echo first; read go; {conn} && echo second")])
            .env("COORD", &coord)
            .env("AGENT_BASE", &base)
            .env("ROSTER_BOX", "7")
            .env("ROSTER_BOX_DIR", &d)
            .env("ROSTER_BOX_ALSO", &also)
            .env("COORD_LAYER", "E-7.l1")
            .env("COORDD_SOCK", &csock)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        // its first connection made, coordd restarts: the socket removed and made anew, then the box goes on
        let mut said = String::new();
        std::io::BufRead::read_line(&mut std::io::BufReader::new(boxed.stdout.as_mut().unwrap()), &mut said).unwrap();
        drop(first);
        std::fs::remove_file(&csock).unwrap();
        let _second = listen();
        std::io::Write::write_all(boxed.stdin.as_mut().unwrap(), b"go\n").unwrap();
        let o = boxed.wait_with_output().unwrap();
        let out = format!("{said}{}", String::from_utf8_lossy(&o.stdout));
        assert_eq!(out.lines().collect::<Vec<_>>(), ["first", "second"], "coordd's socket after its restart: {out}{}", String::from_utf8_lossy(&o.stderr));
        end(&coord, "E-7.l1", "later").unwrap();
        while Path::new(&format!("/proc/{h}")).exists() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let _ = crate::sandbox::in_ns("rm -rf \"$1\"", &[&t.display().to_string()]);
    }

    /// Removal (N-95.p5): three layers of one tree on two bases. One with no commit of its own, one whose commit got
    /// onto the root, one holding a commit on no line. Each closed: its folder goes; the first two branches go, the
    /// third is kept until the top ticket closes; a base goes once no layer stands on it. Needs unprivileged user
    /// namespaces (`--ignored`).
    #[test]
    #[ignore]
    fn a_closed_layer_goes_and_its_branch_unless_it_holds_commits_of_its_own() {
        let t = std::env::temp_dir().join(format!("docket-remove-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let (coord, repo) = (t.join("coord").display().to_string(), t.join("repo").display().to_string());
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        let sh = |c: &str| assert!(Command::new("sh").arg("-c").arg(c).current_dir(&repo).status().unwrap().success(), "{c}");
        sh("git init -q -b dev && printf '.conversations/\\n' > .gitignore && echo hello > a.txt && git add -A && git -c user.name=t -c user.email=t@x commit -qm base && git branch first");
        let (home, codex_home) = (t.join("home").display().to_string(), t.join("codex-home").display().to_string());
        let spec = |id: &str, on: Option<&str>| Spec { id: id.into(), ticket: "E-1".into(), on: on.map(String::from), repo: repo.clone(), root: Some("dev".into()), outputs: vec![], home: home.clone(), codex_home: codex_home.clone(), boxed: false, also: vec![], name: String::new(), warm: None };
        let commit = |l: &Layer, msg: &str| {
            let h = l.holder.unwrap();
            let j = joiner(h, &repo);
            let c = format!("echo {msg} > {msg}.txt && git add -A && git -c user.name=t -c user.email=t@x commit -qm {msg}");
            assert!(Command::new(&j[0]).args(&j[1..]).args(["sh", "-c", &c]).status().unwrap().success());
        };
        let a = open(&coord, &spec("E-2.l1", None), "now").unwrap();
        let b = open(&coord, &spec("E-3.l1", None), "now").unwrap();
        commit(&b, "landed");
        sh("git merge -q --ff-only layer/E-3.l1");
        // a second base: the root moved
        let c = open(&coord, &spec("E-4.l1", None), "now").unwrap();
        commit(&c, "held");
        assert_ne!(a.base, c.base);
        let branches = || git(&repo, &["branch", "--list", "layer/*", "--format=%(refname:short)"]).unwrap();
        let has = |p: String| Path::new(&p).exists();
        assert!(remove(&coord, "E-2.l1", false, "t1").unwrap().contains("no commit of its own"));
        assert!(!has(format!("{coord}/layers/E-2.l1")) && has(format!("{coord}/layers/bases/{}", a.base)), "its folder went; its base stays under E-3.l1");
        assert!(remove(&coord, "E-3.l1", false, "t2").unwrap().contains("contained in the root"));
        assert!(!has(format!("{coord}/layers/bases/{}", a.base)), "no layer stands on the first base");
        let said = remove(&coord, "E-4.l1", false, "t3").unwrap();
        assert!(said.contains("kept"), "{said}");
        assert_eq!(branches(), "layer/E-4.l1");
        let reg = load(&coord);
        assert!(reg["E-4.l1"].kept && reg["E-4.l1"].removed.is_some() && reg.values().all(|l| l.left.is_empty()));
        assert!(!has(format!("{coord}/layers/bases/{}", c.base)) && !has(format!("{coord}/layers/E-4.l1")));
        assert!(remove(&coord, "E-4.l1", true, "t4").unwrap().contains("the top ticket of its tree closed"));
        assert_eq!(branches(), "");
        assert_eq!(git(&repo, &["branch", "--list", "--format=%(refname:short)"]).unwrap(), "dev\nfirst", "the root and the others untouched");
        assert_eq!(remove(&coord, "E-4.l1", true, "t5").unwrap(), "E-4.l1: nothing left to remove");
        let _ = std::fs::remove_dir_all(&t);
    }

    /// A boxed layer (sandbox.rs, N-95): agents as sub-uids write it through the view, each other's files too; a
    /// commit is the project's once brought in; what is outside level one is refused by the kernel; the end freezes
    /// what they wrote, and only a namespace with the box's map removes it.
    #[test]
    #[ignore = "mounts in namespaces and needs the owner's sub-uids: cargo test -- --ignored a_boxed_layer"]
    fn a_boxed_layer_is_written_by_sub_uids_and_brought_in() {
        if crate::sandbox::map_args().is_err() || !Command::new("unshare").args(crate::sandbox::map_args().unwrap()).args(["true"]).status().is_ok_and(|s| s.success()) {
            return eprintln!("no sub-uids to map for this user: skipped");
        }
        let t = std::env::temp_dir().join(format!("docket-boxed-{}", std::process::id()));
        let _ = crate::sandbox::in_ns("rm -rf \"$1\"", &[&t.display().to_string()]);
        let (coord, repo) = (t.join("coord").display().to_string(), t.join("repo").display().to_string());
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        let sh = |c: &str| assert!(Command::new("sh").arg("-c").arg(c).current_dir(&repo).status().unwrap().success(), "{c}");
        sh("git init -q -b dev && printf '.conversations/\\ntarget/\\n' > .gitignore && echo hello > a.txt && git add -A && git -c user.name=t -c user.email=t@x commit -qm base");
        let (home, codex_home) = (t.join("home").display().to_string(), t.join("codex-home").display().to_string());
        let spec = Spec { id: "E-5.l1".into(), ticket: "E-5".into(), on: None, repo: repo.clone(), root: Some("dev".into()), outputs: vec!["target".into()], home, codex_home, boxed: true, also: vec![], name: String::new(), warm: None };
        let l = open(&coord, &spec, "now").unwrap();
        assert!(l.boxed);
        let h = l.holder.unwrap();
        struct Stop(i32);
        impl Drop for Stop {
            fn drop(&mut self) {
                if Path::new(&format!("/proc/{}", self.0)).exists() {
                    unsafe { libc::kill(self.0, libc::SIGTERM) };
                }
            }
        }
        let _stop = Stop(h);
        // as box-enter makes an agent: the view's user namespace, a mount namespace of its own, its own uid
        let agent = |k: u32, c: &str| {
            let o = Command::new("nsenter")
                .args(["--user", "--mount", "--preserve-credentials", "-t", &h.to_string(), "--", "unshare", "--mount", "--", "setpriv", &format!("--reuid={k}"), &format!("--regid={k}"), "--groups=1", "--", "env", "-C", &repo])
                .args(["GIT_CONFIG_COUNT=1", "GIT_CONFIG_KEY_0=safe.directory", "GIT_CONFIG_VALUE_0=*", "HOME=/nonexistent", "sh", "-c", &format!("umask 002; {c}")])
                .output()
                .unwrap();
            (o.status.success(), String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr))
        };
        let (ok, out) = agent(7, "id -u && echo more >> a.txt && echo new > b.txt && echo built > target/out && git add -A && git -c user.name=t -c user.email=t@x commit -qm 'in the box' && echo loose > c.txt");
        assert!(ok, "{out}");
        assert_eq!(out.lines().next(), Some("7"));
        let (ok, out) = agent(8, "echo second >> b.txt");
        assert!(ok, "a second agent of the layer writes the first one's file: {out}");
        let host7 = crate::sandbox::host_uid(7).unwrap();
        use std::os::unix::fs::MetadataExt;
        assert_eq!(std::fs::metadata(format!("{}/c.txt", l.up)).unwrap().uid(), host7, "what it wrote is its sub-uid's on disk");
        assert_eq!(git(&repo, &["log", "--format=%s", &l.branch]).unwrap(), "base", "not the project's before it is brought in");
        sync(&coord, "E-5.l1").unwrap();
        assert_eq!(git(&repo, &["log", "--format=%s", &l.branch]).unwrap(), "in the box\nbase", "brought in");
        assert!(Command::new("find").args([&format!("{repo}/.git"), "!", "-user", &unsafe { libc::getuid() }.to_string()]).output().unwrap().stdout.is_empty(), "the project's repository stays the owner's");
        // outside level one: the project's real git folder, the owner's folder beside the project
        let (ok, out) = agent(7, &format!("echo x > {coord}/layers/E-5.l1/mnt/common/x"));
        assert!(!ok && out.contains("Permission denied"), "{out}");
        let (ok, out) = agent(7, &format!("echo x > {}/x", t.display()));
        assert!(!ok && out.contains("Permission denied"), "{out}");
        let e = end(&coord, "E-5.l1", "later").unwrap();
        assert_eq!(e.state, "ended");
        while Path::new(&format!("/proc/{h}")).exists() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(std::fs::metadata(format!("{}/c.txt", l.up)).unwrap().permissions().readonly(), "frozen");
        assert!(!Path::new(&format!("{coord}/layers/E-5.l1/out")).exists(), "its build outputs went with the view");
        assert!(std::fs::remove_dir_all(&l.up).is_err(), "the owner cannot remove what a sub-uid wrote from outside");
        // its ticket closed: the folder goes by a namespace with the box's map; its branch holds a commit on no line
        let said = remove(&coord, "E-5.l1", false, "closed").unwrap();
        assert!(said.contains("kept") && !Path::new(&format!("{coord}/layers/E-5.l1")).exists(), "{said}");
        assert!(load(&coord)["E-5.l1"].left.is_empty());
        crate::sandbox::in_ns("rm -rf \"$1\"", &[&t.display().to_string()]).unwrap();
        assert!(!t.exists(), "a namespace with the box's map removes the rest");
    }

    /// Outputs started warm (warm.rs), not boxed and boxed: the line's built outputs read through the view, an output
    /// rewritten in place (as Agda writes an interface) and a new one written into the layer's own folder, the runner's
    /// snapshot and the pinned one unchanged, the file that differs from the warm commit touched (copied up) and no
    /// other, the pinned outputs removed with the last layer naming them. Needs unprivileged user namespaces, and
    /// sub-uids for the boxed half (`--ignored`).
    #[test]
    #[ignore]
    fn outputs_start_warm_and_writes_stay_in_the_layer() {
        use std::os::unix::fs::MetadataExt;
        let boxes = crate::sandbox::map_args().is_ok_and(|a| Command::new("unshare").args(a).args(["true"]).status().is_ok_and(|s| s.success()));
        for boxed in [false, true] {
            if boxed && !boxes {
                eprintln!("no sub-uids to map for this user: the boxed half skipped");
                continue;
            }
            let t = std::env::temp_dir().join(format!("docket-warm-{}-{boxed}", std::process::id()));
            let _ = crate::sandbox::in_ns("rm -rf \"$1\"", &[&t.display().to_string()]);
            let _ = std::fs::remove_dir_all(&t);
            let (coord, repo) = (t.join("coord").display().to_string(), t.join("repo").display().to_string());
            std::fs::create_dir_all(&coord).unwrap();
            std::fs::create_dir_all(&repo).unwrap();
            let sh = |c: &str| assert!(Command::new("sh").arg("-c").arg(c).current_dir(&repo).status().unwrap().success(), "{c}");
            let commit = "git add -A && git -c user.name=t -c user.email=t@x commit -qm";
            sh(&format!("git init -q -b dev && printf '.conversations/\\n_build/\\n' > .gitignore && echo a > A.agda && echo b > B.agda && {commit} one && echo B > B.agda && {commit} two"));
            let one = git(&repo, &["rev-parse", "HEAD~1"]).unwrap().trim().to_string();
            // the line's build at commit one, published under its commit; PATH a link to it
            let snap = t.join("runner").join(&one);
            std::fs::create_dir_all(snap.join("_build/x")).unwrap();
            std::fs::write(snap.join("_build/x/A.agdai"), "iface A\n").unwrap();
            std::fs::write(snap.join("commit"), format!("{one}\n")).unwrap();
            std::os::unix::fs::symlink(&snap, t.join("runner/last")).unwrap();
            let mode = std::fs::metadata(snap.join("_build/x/A.agdai")).unwrap().mode();
            let (home, codex_home) = (t.join("home").display().to_string(), t.join("codex-home").display().to_string());
            let spec = Spec { id: "E-8.l1".into(), ticket: "E-8".into(), on: None, repo: repo.clone(), root: Some("dev".into()), outputs: vec!["_build".into()], home, codex_home, boxed, also: vec![], name: String::new(), warm: Some(t.join("runner/last").display().to_string()) };
            let l = open(&coord, &spec, "now").unwrap();
            let name = if boxed { format!("{one}-box") } else { one.clone() };
            assert_eq!(l.warm.as_deref(), Some(name.as_str()), "{:?}", l.notes);
            let h = l.holder.unwrap();
            struct Stop(i32);
            impl Drop for Stop {
                fn drop(&mut self) {
                    if Path::new(&format!("/proc/{}", self.0)).exists() {
                        unsafe { libc::kill(self.0, libc::SIGTERM) };
                    }
                }
            }
            let _stop = Stop(h);
            let run = |c: &str| {
                let o = if boxed {
                    Command::new("nsenter")
                        .args(["--user", "--mount", "--preserve-credentials", "-t", &h.to_string(), "--", "unshare", "--mount", "--", "setpriv", "--reuid=7", "--regid=7", "--groups=1", "--", "env", "-C", &repo, "sh", "-c", &format!("umask 002; {c}")])
                        .output()
                        .unwrap()
                } else {
                    let j = joiner(h, &repo);
                    Command::new(&j[0]).args(&j[1..]).args(["sh", "-c", c]).output().unwrap()
                };
                assert!(o.status.success(), "{c}: {}", String::from_utf8_lossy(&o.stderr));
                String::from_utf8_lossy(&o.stdout).into_owned()
            };
            assert_eq!(run("cat _build/x/A.agdai"), "iface A\n", "the line's output, through the view");
            // rewritten in place (open with truncation, no unlink: as Agda writes an interface), and one made
            run("echo 'iface A2' > _build/x/A.agdai && echo 'iface B' > _build/x/B.agdai");
            assert_eq!(run("cat _build/x/A.agdai _build/x/B.agdai"), "iface A2\niface B\n");
            let out = format!("{coord}/layers/E-8.l1/out/_build/up/x");
            assert_eq!(std::fs::read_to_string(format!("{out}/A.agdai")).unwrap(), "iface A2\n", "the write is in the layer's own folder");
            assert_eq!(std::fs::read_to_string(snap.join("_build/x/A.agdai")).unwrap(), "iface A\n", "the runner's snapshot unchanged");
            let pinned = format!("{coord}/layers/warm/{name}/_build/x/A.agdai");
            assert_eq!(std::fs::read_to_string(&pinned).unwrap(), "iface A\n", "the pinned outputs unchanged");
            let linked = std::fs::metadata(&pinned).unwrap().ino() == std::fs::metadata(snap.join("_build/x/A.agdai")).unwrap().ino();
            assert_eq!(linked, !boxed, "linked unless boxed");
            if !boxed {
                assert_eq!(std::fs::metadata(&pinned).unwrap().mode(), mode, "nothing linked is chmodded");
            }
            // B.agda differs from the warm commit: touched, so copied up; A.agda is not
            assert!(Path::new(&format!("{}/B.agda", l.up)).exists() && !Path::new(&format!("{}/A.agda", l.up)).exists());
            assert!(!Path::new(&format!("{coord}/layers/E-8.l1/touch")).exists(), "touched once");
            assert_eq!(git(&repo, &["diff", "--stat", "HEAD"]).unwrap(), "", "the project's folder untouched");
            end(&coord, "E-8.l1", "later").unwrap();
            while Path::new(&format!("/proc/{h}")).exists() {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(!Path::new(&format!("{coord}/layers/E-8.l1/out")).exists(), "its build outputs went with the view");
            let said = remove(&coord, "E-8.l1", false, "closed").unwrap();
            assert!(said.contains(&format!("warm/{name} removed")), "{said}");
            assert!(snap.join("_build/x/A.agdai").exists(), "the runner's snapshot stays");
            if boxed {
                crate::sandbox::in_ns("rm -rf \"$1\"", &[&t.display().to_string()]).unwrap();
            } else {
                remove_tree(&t.display().to_string()).unwrap();
            }
            assert!(!t.exists());
        }
    }

    /// Home's Layers now (a2h-speed N-1446 p2815; docket-edge8 p2904, p2957): each layer keeps its fields and gains
    /// `under`; an open one, and a run's, its own folder's bytes and its largest files (`up_bytes`, `wrote`); each item
    /// under an open layer says how many open layers lie on it; an ended one is not measured or counted.
    #[test]
    fn the_layers_answer_says_what_lies_under_each_and_what_it_wrote() {
        let t = std::env::temp_dir().join(format!("coordd-layers-answer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        for (f, n) in [("piece/up/src/a.rs", 10), ("piece/up/target/big", 3000), ("run/up/tickets/log.jsonl", 70)] {
            std::fs::create_dir_all(t.join(f).parent().unwrap()).unwrap();
            std::fs::write(t.join(f), vec![b'x'; n]).unwrap();
        }
        std::os::unix::fs::symlink("/etc/hostname", t.join("piece/up/link")).unwrap();
        let layer = |id: &str, base: &str, state: &str, up: &str| Layer { id: id.into(), ticket: format!("I-{id}"), repo: "/repo".into(), base: base.into(), on: None,
            branch: format!("layer/{id}"), state: state.into(), holder: None, view: None, up: up.into(), outputs: vec![], at: "2026-11-24T16:00:00Z".into(),
            ended: None, notes: vec![], boxed: false, root: None, removed: None, kept: false, left: vec![], also: vec![], warm: None };
        let mut reg = BTreeMap::new();
        reg.insert("p1".to_string(), layer("p1", "00aba796", "open", &t.join("piece/up").display().to_string()));
        reg.insert("p2".to_string(), layer("p2", "00aba796", "ended", "/nowhere"));
        let run = |id: &str| serde_json::json!({"kind": "run", "id": id, "state": "open", "up": t.join("run/up").display().to_string(),
            "under": [{"what": "record", "record": "research", "line": 185206, "bytes": 1300}, {"what": "binary", "path": "/b", "md5": "05431864", "bytes": 212}]});
        let a = answer(reg, vec![run("7-1"), run("8-2")]);
        assert_eq!(a.len(), 4);
        assert_eq!((a[0]["id"].as_str(), a[0]["base"].as_str(), a[0]["branch"].as_str()), (Some("p1"), Some("00aba796"), Some("layer/p1")), "its fields as they were");
        assert_eq!(a[0]["under"], serde_json::json!([{"what": "tree", "commit": "00aba796", "shared": 1}]));
        let link = std::fs::symlink_metadata(t.join("piece/up/link")).unwrap().len();
        assert_eq!(a[0]["up_bytes"], 3010 + link, "a link counted as itself");
        assert_eq!(a[0]["wrote"][0], serde_json::json!({"path": "target/big", "bytes": 3000}), "the largest first");
        assert_eq!(a[0]["wrote"].as_array().unwrap().len(), 3);
        assert_eq!(a[1]["under"], serde_json::json!([{"what": "tree", "commit": "00aba796"}]), "an ended layer: not counted");
        assert!(a[1].get("up_bytes").is_none() && a[1].get("wrote").is_none(), "nor measured");
        assert_eq!(a[2]["up_bytes"], 70);
        assert_eq!(a[2]["wrote"], serde_json::json!([{"path": "tickets/log.jsonl", "bytes": 70}]));
        assert_eq!(a[2]["under"][0]["shared"], 2, "two runs on the same record to the same line");
        assert_eq!(a[3]["under"][1]["shared"], 2, "and the same binary");
        let (_, wrote, partial) = measure_up(&t.join("nothing"));
        assert!(wrote.is_empty() && !partial);
        let many = t.join("many");
        std::fs::create_dir_all(&many).unwrap();
        for i in 0..(WROTE + 5) {
            std::fs::write(many.join(format!("f{i:03}")), vec![b'x'; i]).unwrap();
        }
        let (bytes, wrote, _) = measure_up(&many);
        assert_eq!((bytes, wrote.len(), wrote[0].clone()), ((0..WROTE + 5).sum::<usize>() as u64, WROTE, (format!("f{:03}", WROTE + 4), (WROTE + 4) as u64)));
        // asked again within the minute, the measure is the one kept, and says when it was made (consult-lead p2980)
        let up1 = t.join("piece/up").display().to_string();
        let one = || BTreeMap::from([("p1".to_string(), layer("p1", "00aba796", "open", &up1))]);
        assert!(a[0]["up_as_of"].as_str().is_some_and(|s| s.ends_with('Z')), "when it was measured");
        std::fs::write(t.join("piece/up/src/b.rs"), vec![b'x'; 5]).unwrap();
        let kept = answer(one(), vec![]);
        assert_eq!((&kept[0]["up_bytes"], &kept[0]["up_as_of"]), (&a[0]["up_bytes"], &a[0]["up_as_of"]), "kept: not walked again");
        if let Some(e) = MEASURED.lock().unwrap().get_mut(&up1) {
            e.0 = e.0.checked_sub(std::time::Duration::from_secs(MEASURE_SEC)).unwrap_or(e.0);
        }
        let again = answer(one(), vec![]);
        assert_eq!(again[0]["up_bytes"], a[0]["up_bytes"].as_u64().unwrap() + 5, "a minute on: walked again");
        let _ = answer(BTreeMap::new(), vec![]);
        assert!(!MEASURED.lock().unwrap().contains_key(&up1), "a layer no longer open: its kept measure goes");
        let _ = std::fs::remove_dir_all(&t);
    }
}
