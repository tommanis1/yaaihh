//! coordd's side of cold storage: `Request::Cold` answered off the lock, and the daily run of the automatic rules.

use super::{Config, Item, Places, answer, evict_all, load, plan, restore, set_auto};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

/// How long a dry run is shown again before it is walked anew: a worktree's walk is slow, Home asks often.
const FRESH: Duration = Duration::from_secs(600);

/// How often the automatic rules run.
const DAILY: Duration = Duration::from_secs(86_400);

/// The last dry run: for which coord and config, when, and what it found.
static LAST: Mutex<Option<(String, Config, Instant, Vec<(usize, Item)>)>> = Mutex::new(None);

/// What CORE's rules look at: its repositories, Claude Code's projects folder under its coord, and the transcripts its
/// live agents write.
pub fn places(core: &crate::core::Core) -> Places {
    let mut repos: Vec<String> = core.project.as_ref().map(|p| p.repos().into_iter().map(|(_, r)| r).collect()).unwrap_or_default();
    if repos.is_empty() {
        repos.extend(core.project.as_ref().and_then(|p| p.repo()));
    }
    let claude = format!("{}/claude-home/projects", core.coord);
    let transcripts = if std::path::Path::new(&claude).is_dir() { vec![claude] } else { Vec::new() };
    let live = core.agents.values().filter(|a| a.ended.is_none()).filter_map(|a| a.transcript()).collect();
    Places { repos, transcripts, live }
}

/// The dry run for COORD's CFG over PLACES: the last one while FRESH, else walked now.
fn dry(coord: &str, cfg: &Config, places: &Places, again: bool) -> Vec<(usize, Item)> {
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((c, k, at, p)) = last.as_ref() {
        if !again && c == coord && k == cfg && at.elapsed() < FRESH {
            return p.clone();
        }
    }
    let p = plan(cfg, places, SystemTime::now());
    *last = Some((coord.to_string(), cfg.clone(), Instant::now(), p.clone()));
    p
}

/// `Request::Cold`: with no ACT the answer; `evict` every item of the dry run moved now; `restore` the item that was
/// at PATH brought back; `auto` rule RULE made automatic or manual (ON). All but the answer are the owner's.
pub fn serve(coord: &str, places: &Places, owner: bool, act: Option<&str>, path: Option<&str>, rule: Option<usize>, on: Option<bool>) -> Result<String, String> {
    if act.is_some() && !owner {
        return Err("coord cold: only the owner moves things to or from cold storage".into());
    }
    let cfg = load(coord)?;
    let said = match act {
        None => None,
        Some("evict") => {
            let p = dry(coord, &cfg, places, true);
            let (done, failed) = evict_all(coord, &cfg, &p, |_| true);
            let bytes: u64 = done.iter().map(|d| d.manifest.bytes).sum();
            let mut s = format!("{} moved to cold storage ({})", done.len(), size(bytes));
            if !failed.is_empty() {
                s.push_str(&format!("; {} not moved: {}", failed.len(), failed.join("; ")));
            }
            Some(s)
        }
        Some("restore") => {
            let path = path.ok_or("coord cold restore: which path")?;
            let p = restore(coord, &cfg, path)?;
            Some(format!("{} is back from {} ({})", p.path, p.target, size(p.manifest.bytes)))
        }
        Some("auto") => Some(set_auto(coord, rule.ok_or("coord cold auto: which rule")?, on.unwrap_or(false))?),
        Some(a) => return Err(format!("coord cold: no act {a} (evict, restore, auto)")),
    };
    let cfg = load(coord)?;
    let p = dry(coord, &cfg, places, act.is_some());
    Ok(answer(coord, &cfg, &p, said.as_deref()))
}

/// Bytes as the window says them.
fn size(b: u64) -> String {
    match b {
        b if b >= 1 << 30 => format!("{:.1} GB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        b => format!("{b} bytes"),
    }
}

/// Once a day, the automatic rules' items moved, each said in coordd's log; the day's mark is `cold-auto` in the coord.
pub fn daily(shared: crate::Shared) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            let (coord, places) = {
                let core = shared.lock().await;
                (core.coord.clone(), places(&core))
            };
            let mark = format!("{coord}/cold-auto");
            if std::fs::metadata(&mark).and_then(|m| m.modified()).is_ok_and(|t| t.elapsed().unwrap_or_default() < DAILY) {
                continue;
            }
            let _ = tokio::task::spawn_blocking(move || {
                let _ = std::fs::write(&mark, "");
                let Ok(cfg) = load(&coord) else { return };
                if !cfg.rule.iter().any(|r| r.auto) {
                    return;
                }
                let p = dry(&coord, &cfg, &places, true);
                let (done, failed) = evict_all(&coord, &cfg, &p, |r| r.auto);
                for d in &done {
                    eprintln!("coordd: cold storage: {} moved to {} as {}", d.path, d.target, d.key);
                }
                for f in &failed {
                    eprintln!("coordd: cold storage: not moved: {f}");
                }
            })
            .await;
        }
    });
}
