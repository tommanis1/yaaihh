//! Cold storage and eviction (the owner's walk item 20): `yaaihh.toml`'s `[cold]` names the targets and the rules,
//! e.g. worktrees idle 30 days to a disk. A dry run says what a rule would move and how many bytes; an eviction copies
//! each item to its target, checks the copy, writes a pointer `PATH.cold.json` beside it and only then removes it.
//! Restore brings it back from the pointer. Every move is a line of `cold.jsonl` beside the coords.

pub mod serve;

pub use cold_store as driver;
pub use cold_store::{Pointer, pointer_of};
use driver::Target;
#[cfg(test)]
use driver::Manifest;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Folders not looked into when a worktree's last change is read: they change without anyone working there.
const QUIET: [&str; 3] = ["target", "node_modules", ".git"];

/// How many entries a worktree's walk reads at most: past it, it counts as in use.
const WALK_CAP: usize = 1_000_000;

/// A rule as `[[cold.rule]]` gives it: WHAT (`worktrees`, `builds` or `transcripts`) idle IDLE_DAYS goes TO a target;
/// AUTO: coordd moves it once a day on its own; UNDER: where to look instead of the project's own places.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Rule {
    pub what: String,
    pub idle_days: u64,
    pub to: String,
    #[serde(default)]
    pub auto: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub under: Vec<String>,
}

/// `[cold]` of `yaaihh.toml`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Config {
    #[serde(default)]
    pub target: Vec<Target>,
    #[serde(default)]
    pub rule: Vec<Rule>,
}

/// The yaaihh-wide folder of COORD: where `yaaihh.toml` and `cold.jsonl` are.
pub fn wide(coord: &str) -> PathBuf {
    Path::new(coord).parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
}

pub fn load(coord: &str) -> Result<Config, String> {
    let file = wide(coord).join("yaaihh.toml");
    let Ok(text) = std::fs::read_to_string(&file) else { return Ok(Config::default()) };
    #[derive(Deserialize)]
    struct File {
        #[serde(default)]
        cold: Config,
    }
    toml::from_str::<File>(&text).map(|f| f.cold).map_err(|e| format!("{}: [cold]: {e}", file.display()))
}

/// Where the project's own places are, for rules with no `under`: its repositories, and Claude Code's projects folder.
#[derive(Clone, Debug, Default)]
pub struct Places {
    pub repos: Vec<String>,
    pub transcripts: Vec<String>,
    /// Transcripts a live agent writes: never moved.
    pub live: Vec<String>,
}

/// One thing a rule would move.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Item {
    pub path: String,
    pub what: String,
    pub bytes: u64,
    pub idle_days: u64,
    pub to: String,
}

/// Bytes under PATH and its newest change, not looking into QUIET folders for the latter; None past WALK_CAP.
fn size_and_newest(path: &Path) -> Option<(u64, SystemTime)> {
    let (mut bytes, mut newest, mut seen) = (0u64, SystemTime::UNIX_EPOCH, 0usize);
    let mut stack = vec![(path.to_path_buf(), true)];
    while let Some((p, counts)) = stack.pop() {
        seen += 1;
        if seen > WALK_CAP {
            return None;
        }
        let Ok(m) = std::fs::symlink_metadata(&p) else { continue };
        if counts {
            newest = newest.max(m.modified().unwrap_or(SystemTime::UNIX_EPOCH));
        }
        if m.is_dir() {
            for e in std::fs::read_dir(&p).into_iter().flatten().flatten() {
                let quiet = QUIET.iter().any(|q| e.file_name() == *q);
                stack.push((e.path(), counts && !quiet));
            }
        } else if m.is_file() {
            bytes += m.len();
        }
    }
    Some((bytes, newest))
}

fn days(since: SystemTime, now: SystemTime) -> u64 {
    now.duration_since(since).unwrap_or(Duration::ZERO).as_secs() / 86_400
}

/// The worktrees of REPO besides its own folder, as git lists them; none locked.
fn worktrees(repo: &str) -> Vec<PathBuf> {
    let Ok(o) = std::process::Command::new("git").args(["-C", repo, "worktree", "list", "--porcelain"]).output() else { return Vec::new() };
    let text = String::from_utf8_lossy(&o.stdout);
    let mut out = Vec::new();
    for block in text.split("\n\n").skip(1) {
        let Some(p) = block.lines().find_map(|l| l.strip_prefix("worktree ")) else { continue };
        if !block.lines().any(|l| l.starts_with("locked") || l == "bare") {
            out.push(PathBuf::from(p));
        }
    }
    out
}

/// Build folders in ROOT or one folder down (`target`): a `target` that cargo made.
fn build_dirs(root: &Path) -> Vec<PathBuf> {
    let is_build = |p: &Path| p.join("CACHEDIR.TAG").is_file() || p.join(".rustc_info.json").is_file();
    let mut out: Vec<PathBuf> = [root.join("target")].into_iter().filter(|p| is_build(p)).collect();
    for e in std::fs::read_dir(root).into_iter().flatten().flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())) {
        let t = e.path().join("target");
        if is_build(&t) {
            out.push(t);
        }
    }
    out
}

/// `*.jsonl` files in DIR or one folder down.
fn transcripts(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(std::fs::read_dir(&p).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jsonl") && p.is_file()));
        } else if p.extension().is_some_and(|x| x == "jsonl") {
            out.push(p);
        }
    }
    out
}

/// What RULE would move now: each item idle at least its days, the largest first.
pub fn candidates(rule: &Rule, places: &Places, now: SystemTime) -> Vec<Item> {
    let roots: Vec<String> = if rule.under.is_empty() { if rule.what == "transcripts" { places.transcripts.clone() } else { places.repos.clone() } } else { rule.under.clone() };
    let found: Vec<PathBuf> = match rule.what.as_str() {
        "worktrees" => roots.iter().flat_map(|r| worktrees(r)).collect(),
        "builds" => roots.iter().flat_map(|r| std::iter::once(PathBuf::from(r)).chain(worktrees(r))).flat_map(|w| build_dirs(&w)).collect(),
        "transcripts" => roots.iter().flat_map(|r| transcripts(Path::new(r))).filter(|p| !places.live.iter().any(|l| Path::new(l) == p)).collect(),
        _ => Vec::new(),
    };
    let mut items: Vec<Item> = found
        .into_iter()
        .filter_map(|p| {
            let (bytes, newest) = size_and_newest(&p)?;
            let idle = days(newest, now);
            (idle >= rule.idle_days).then(|| Item { path: p.display().to_string(), what: rule.what.clone(), bytes, idle_days: idle, to: rule.to.clone() })
        })
        .collect();
    items.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.path.cmp(&b.path)));
    items.dedup_by(|a, b| a.path == b.path);
    items
}

fn record(coord: &str, line: Value) {
    use std::io::Write;
    let file = wide(coord).join("cold.jsonl");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&file) {
        let _ = writeln!(f, "{line}");
    }
}

/// ITEM moved to TARGET: copied, checked, the pointer written, and only then removed here (a worktree locked in git
/// first, so git keeps its record of it).
pub fn evict(coord: &str, item: &Item, target: &Target) -> Result<Pointer, String> {
    let p = cold_store::stow(&item.path, &item.what, target)?;
    let path = Path::new(&item.path);
    if item.what == "worktrees" {
        let reason = format!("in cold storage: {}", pointer_of(&item.path).display());
        let _ = std::process::Command::new("git").args(["-C", &item.path, "worktree", "lock", "--reason", &reason, &item.path]).output();
    }
    let gone = if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
    gone.map_err(|e| format!("{}: copied and checked, but not removed: {e}", item.path))?;
    record(coord, json!({"ev": "evicted", "path": p.path, "what": p.what, "target": p.target, "key": p.key, "bytes": p.manifest.bytes, "at": p.at}));
    Ok(p)
}

/// The item whose pointer is beside PATH brought back from its target, checked, and the pointer removed.
pub fn restore(coord: &str, cfg: &Config, path: &str) -> Result<Pointer, String> {
    let ptr = pointer_of(path);
    let p: Pointer = serde_json::from_str(&std::fs::read_to_string(&ptr).map_err(|e| format!("{}: {e}", ptr.display()))?).map_err(|e| format!("{}: {e}", ptr.display()))?;
    if std::fs::symlink_metadata(&p.path).is_ok() {
        return Err(format!("{} is there again: restore it by hand or move it first", p.path));
    }
    let target = cfg.target.iter().find(|t| t.name == p.target).ok_or_else(|| format!("cold target {} is no longer in yaaihh.toml", p.target))?;
    let staging = PathBuf::from(format!("{}.cold-restoring", p.path));
    let _ = if staging.is_dir() { std::fs::remove_dir_all(&staging) } else { std::fs::remove_file(&staging) };
    let name = Path::new(&p.path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    target.get(&p.key, &name, &staging, &p.manifest)?;
    std::fs::rename(&staging, &p.path).map_err(|e| format!("{}: {e}", p.path))?;
    if p.what == "worktrees" {
        let _ = std::process::Command::new("git").args(["-C", &p.path, "worktree", "unlock", &p.path]).output();
    }
    std::fs::remove_file(&ptr).map_err(|e| format!("{}: {e}", ptr.display()))?;
    record(coord, json!({"ev": "restored", "path": p.path, "target": p.target, "key": p.key, "at": chrono::Utc::now().to_rfc3339()}));
    Ok(p)
}

/// What is in cold storage now: each eviction of `cold.jsonl` not restored since and whose pointer still stands.
pub fn held(coord: &str) -> Vec<Value> {
    let text = std::fs::read_to_string(wide(coord).join("cold.jsonl")).unwrap_or_default();
    let mut out: Vec<Value> = Vec::new();
    for v in text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()) {
        let path = v["path"].as_str().unwrap_or_default().to_string();
        out.retain(|o| o["path"].as_str() != Some(&path));
        if v["ev"] == "evicted" && pointer_of(&path).is_file() {
            out.push(v);
        }
    }
    out
}

/// The dry run over every rule: what each would move, by rule index.
pub fn plan(cfg: &Config, places: &Places, now: SystemTime) -> Vec<(usize, Item)> {
    let mut seen = std::collections::BTreeSet::new();
    cfg.rule.iter().enumerate().flat_map(|(i, r)| candidates(r, places, now).into_iter().map(move |it| (i, it))).filter(|(_, it)| seen.insert(it.path.clone())).collect()
}

/// Every item of PLAN moved whose rule passes ONLY; what was moved and what was not, in words.
pub fn evict_all(coord: &str, cfg: &Config, plan: &[(usize, Item)], only: impl Fn(&Rule) -> bool) -> (Vec<Pointer>, Vec<String>) {
    let (mut done, mut failed) = (Vec::new(), Vec::new());
    for (i, it) in plan.iter().filter(|(i, _)| cfg.rule.get(*i).is_some_and(&only)) {
        let Some(t) = cfg.target.iter().find(|t| t.name == cfg.rule[*i].to) else {
            failed.push(format!("{}: rule {} names no target {}", it.path, i + 1, cfg.rule[*i].to));
            continue;
        };
        match t.reachable().and_then(|_| evict(coord, it, t)) {
            Ok(p) => done.push(p),
            Err(e) => failed.push(format!("{}: {e}", it.path)),
        }
    }
    (done, failed)
}

/// The window's answer: targets with whether each is reachable, the rules, the dry run's count and bytes with its items,
/// what is held in cold storage, the file they are set in, and SAID.
pub fn answer(coord: &str, cfg: &Config, plan: &[(usize, Item)], said: Option<&str>) -> String {
    let targets: Vec<Value> = cfg.target.iter().map(|t| match t.reachable() {
        Ok(()) => json!({"name": t.name, "kind": if t.rclone.is_some() { "rclone" } else { "local" }, "where": t.place(), "ok": true}),
        Err(why) => json!({"name": t.name, "kind": if t.rclone.is_some() { "rclone" } else { "local" }, "where": t.place(), "ok": false, "why": why}),
    }).collect();
    let rules: Vec<Value> = cfg.rule.iter().enumerate().map(|(i, r)| {
        let mine: Vec<&Item> = plan.iter().filter(|(j, _)| *j == i).map(|(_, it)| it).collect();
        json!({"what": r.what, "idle_days": r.idle_days, "to": r.to, "auto": r.auto, "count": mine.len(), "bytes": mine.iter().map(|it| it.bytes).sum::<u64>()})
    }).collect();
    let items: Vec<&Item> = plan.iter().map(|(_, it)| it).collect();
    json!({
        "file": wide(coord).join("yaaihh.toml").display().to_string(),
        "targets": targets,
        "rules": rules,
        "plan": {"count": items.len(), "bytes": items.iter().map(|it| it.bytes).sum::<u64>(), "items": items},
        "held": held(coord),
        "said": said,
    })
    .to_string()
}

/// Rule I's `auto` set to ON in `yaaihh.toml`, its other text kept.
pub fn set_auto(coord: &str, i: usize, on: bool) -> Result<String, String> {
    let file = wide(coord).join("yaaihh.toml");
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("{}: {e}", file.display()))?;
    let rule = doc.get_mut("cold").and_then(|c| c.get_mut("rule")).and_then(|r| r.as_array_of_tables_mut()).and_then(|a| a.get_mut(i)).ok_or_else(|| format!("{}: no cold rule {}", file.display(), i + 1))?;
    rule["auto"] = toml_edit::value(on);
    std::fs::write(&file, doc.to_string()).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(format!("rule {} is {} now", i + 1, if on { "automatic" } else { "manual" }))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
