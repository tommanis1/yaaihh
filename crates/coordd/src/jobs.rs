//! An agent's jobs on the build runner (N-1334): the files the runner's caller side keeps, one per job
//! (`~/agent-coord/build-runner/jobs/PID-WHEN.json`, `build-runner`'s `JobFile`), read off the lock each second
//! (`main::read_jobs`) and made the agents' `AgentView.jobs` by `Core::set_jobs`. Nothing is written here. A link,
//! a file over 64 KiB, or one not in the runner's shape is passed over. kpi-data's file of the machines
//! (`~/agent-coord/machines/machines.json`) is the one source of whether the build machine answers (consult-lead,
//! N-1334 p2: the same fact as the Machines page's tower card).

use coord_types::Job;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The largest job file read: the runner's are a few hundred bytes.
const MOST: u64 = 64 * 1024;

/// One job file, read: the caller's agent and coord as the runner found them, whether its caller still runs, when
/// the file was last written, and the job as the view says it (times in coordd's own form, `core::at_secs`).
pub struct Read {
    pub agent: String,
    pub coord: String,
    pub alive: bool,
    pub written: String,
    pub job: Job,
}

/// The build machine as kpi-data's file last saw it: its name, and whether it answers.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Machine {
    pub name: String,
    pub on: bool,
}

/// A job file as `build-runner` writes it: the view's fields and the caller's own.
#[derive(Deserialize)]
struct File {
    pid: i32,
    agent: Option<String>,
    coord: Option<String>,
    verb: String,
    #[serde(default)]
    packages: Vec<String>,
    target: Option<String>,
    branch: Option<String>,
    commit: Option<String>,
    machine: Option<String>,
    since: String,
    step: String,
    step_since: String,
    what: Option<String>,
    done: Option<u64>,
    of: Option<u64>,
    ahead: Option<u64>,
    failed: Option<u64>,
    passed: Option<u64>,
    rc: Option<i32>,
}

/// The runner's folder of job files (its `JOBS_DIR`, under the `BUILD_RUNNER_HOME` it runs with by default).
pub fn dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("agent-coord/build-runner/jobs")
}

/// The runner's job files from before the record had its jobs, in one folder made once and never written again
/// (paper-research N-1446 p1422, 2): the files of `dir` and of its 09:10 copy, each job sent before the runner's first
/// `act` `job` line (461553-1795338165, 22 Nov 09:02:45Z); what the kept graph reads once ([`of_coord`]).
pub fn before_record_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("agent-coord/build-runner/jobs-before-record")
}

/// kpi-data's file of the machines.
pub fn machines_file() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("agent-coord/machines/machines.json")
}

/// Each job file in DIR (`.json`, a file, at most MOST bytes): its path, its metadata and its bytes.
fn files(dir: &Path) -> Vec<(PathBuf, std::fs::Metadata, Vec<u8>)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| {
            let meta = std::fs::symlink_metadata(&p).ok().filter(|m| m.is_file() && m.len() <= MOST)?;
            let bytes = std::fs::read(&p).ok()?;
            Some((p, meta, bytes))
        })
        .collect()
}

/// The jobs of DIR's files sent from COORD, as the graph reads a job file (`graph_git::Job::parse`), each its file's
/// name for its id, oldest first; a file that does not read is left out.
pub fn of_coord(dir: &Path, coord: &str) -> Vec<docket_machine::graph_git::Job> {
    let mut jobs: Vec<_> = files(dir)
        .into_iter()
        .filter_map(|(p, _, bytes)| docket_machine::graph_git::Job::parse(&p.file_stem()?.to_string_lossy(), std::str::from_utf8(&bytes).ok()?))
        .filter(|j| Path::new(&j.coord) == Path::new(coord))
        .collect();
    jobs.sort_by(|a, b| (&a.since, &a.id).cmp(&(&b.since, &b.id)));
    jobs
}

/// Every job file in DIR that names an agent.
pub fn read(dir: &Path) -> Vec<Read> {
    files(dir)
        .into_iter()
        .filter_map(|(p, meta, bytes)| {
            let f: File = serde_json::from_slice(&bytes).ok()?;
            let id = p.file_stem()?.to_string_lossy().into_owned();
            let written = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
            Some(Read {
                agent: f.agent?,
                coord: f.coord?,
                alive: alive(f.pid),
                written: crate::core::at_secs(written),
                job: Job {
                    id,
                    verb: f.verb,
                    packages: f.packages,
                    target: f.target,
                    branch: f.branch,
                    commit: f.commit,
                    machine: f.machine,
                    since: time(&f.since)?,
                    step: f.step,
                    step_since: time(&f.step_since)?,
                    what: f.what,
                    done: f.done,
                    of: f.of,
                    ahead: f.ahead,
                    failed: f.failed,
                    passed: f.passed,
                    rc: f.rc,
                    fresh: false,
                },
            })
        })
        .collect()
}

// how long the machines' file speaks for the build machine: the sampler's own freshness window, one number
pub use machine_state::machines::FRESH_SECS;

/// The build machine in kpi-data's FILE (`build_machine`: `name`, `on`) when it was written less than [`FRESH_SECS`]
/// before NOW (one written after NOW, by another clock, counts as just written); None when it names none, cannot be
/// read or is older: not known, and the runner's own step stands (`Core::set_jobs`).
pub fn machine(file: &Path, now: std::time::SystemTime) -> Option<Machine> {
    #[derive(Deserialize)]
    struct Machines {
        build_machine: Option<Machine>,
    }
    let meta = std::fs::symlink_metadata(file).ok().filter(|m| m.is_file() && m.len() <= MOST)?;
    let age = now.duration_since(meta.modified().ok()?).unwrap_or_default();
    if age.as_secs() >= FRESH_SECS {
        return None;
    }
    serde_json::from_slice::<Machines>(&std::fs::read(file).ok()?).ok()?.build_machine
}

/// A time as the runner writes it (`%FT%T%z`, `2026-11-20T07:16:02+0100`), or in RFC 3339, in coordd's own form.
fn time(s: &str) -> Option<String> {
    let t = chrono::DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%z").or_else(|_| chrono::DateTime::parse_from_rfc3339(s)).ok()?;
    Some(crate::core::at_secs(t.timestamp()))
}

/// Whether process PID is a build-runner still (a pid comes again: one that runs something else is gone).
pub fn alive(pid: i32) -> bool {
    std::fs::read(format!("/proc/{pid}/cmdline")).is_ok_and(|c| c.windows(12).any(|w| w == b"build-runner"))
}

/// The layers of the replays the runner runs now (Home's Layers now, a2h-speed N-1446 p2815, p2927): each job file in
/// DIR that holds a `layer` (a replay on a view of a live record writes one), whose job has not ended and whose runner
/// still runs (ALIVE, given its pid), as a layer of `kind` "run": the job's id (its file's name), its agent and coord,
/// its ticket (its first item) and items, its verb, `state` open, and the layer's `up`, `at` and `under` as the
/// runner wrote them. A file that does not read is passed over.
pub fn run_layers(dir: &Path, alive: impl Fn(i32) -> bool) -> Vec<serde_json::Value> {
    let mut out: Vec<serde_json::Value> = files(dir)
        .into_iter()
        .filter_map(|(p, _, bytes)| {
            let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
            let l = v.get("layer")?.as_object()?;
            if v["step"] == "ended" || !alive(i32::try_from(v["pid"].as_i64()?).ok()?) {
                return None;
            }
            let items = v["items"].as_array().cloned().unwrap_or_default();
            Some(serde_json::json!({"kind": "run", "id": p.file_stem()?.to_string_lossy(), "agent": v["agent"], "coord": v["coord"],
                "ticket": items.first().cloned().unwrap_or_default(), "items": items, "verb": v["verb"], "state": "open",
                "up": l.get("up"), "at": l.get("at"), "under": l.get("under").cloned().unwrap_or_else(|| serde_json::json!([]))}))
        })
        .collect();
    out.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    out
}

/// The clock time of coordd's time AT, on this machine's clock (`07:13`): what an agent and the owner read.
pub fn clock(at: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(at).map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string()).unwrap_or_else(|_| at.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A job file as the runner writes it is read with its times in coordd's form; one with no agent, a link, one
    /// not in the runner's shape and another kind of file are passed over; kpi-data's build machine is read.
    #[test]
    fn the_runners_job_files_are_read_and_the_rest_passed_over() {
        let d = std::env::temp_dir().join(format!("coordd-jobs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let job = serde_json::json!({"pid": 1, "agent": "w2", "coord": "/c/research", "who": "w2", "wt": "/wt", "branch": "edge5-typing",
            "commit": "ec7f09cb", "verb": "test", "packages": ["yaaihh"], "target": null, "machine": "yaaihh-build-1", "front": false,
            "train": false, "since": "2026-11-20T07:16:02+0100", "step_since": "2026-11-20T07:18:40+0100", "step": "testing",
            "at": "2026-11-20T07:18:41+0100", "what": "yaaihh", "done": 181, "of": 293, "failed": 1});
        std::fs::write(d.join("41233-1763619362.json"), job.to_string()).unwrap();
        let mut no_agent = job.clone();
        no_agent["agent"] = serde_json::Value::Null;
        std::fs::write(d.join("1-2.json"), no_agent.to_string()).unwrap();
        std::fs::write(d.join("3-4.json"), "{\"pid\": 3}").unwrap();
        std::fs::write(d.join("5-6.tmp"), job.to_string()).unwrap();
        std::os::unix::fs::symlink(d.join("41233-1763619362.json"), d.join("7-8.json")).unwrap();
        let r = read(&d);
        assert_eq!(r.len(), 1, "only the runner's own file");
        let j = &r[0].job;
        assert_eq!((r[0].agent.as_str(), r[0].coord.as_str()), ("w2", "/c/research"));
        assert_eq!((j.id.as_str(), j.since.as_str(), j.step_since.as_str()), ("41233-1763619362", "2026-11-20T06:16:02Z", "2026-11-20T06:18:40Z"));
        assert_eq!((j.step.as_str(), j.what.as_deref(), j.done, j.of, j.failed), ("testing", Some("yaaihh"), Some(181), Some(293), Some(1)));
        assert_eq!(j.reference(), "test yaaihh · edge5-typing ec7f09cb");
        std::fs::write(d.join("machines.json"), r#"{"read": "x", "build_machine": {"name": "yaaihh-build-1", "on": false, "cores": 24}}"#).unwrap();
        let now = std::time::SystemTime::now();
        assert_eq!(machine(&d.join("machines.json"), now), Some(Machine { name: "yaaihh-build-1".into(), on: false }));
        assert_eq!(machine(&d.join("none.json"), now), None);
        // the same file FRESH_SECS later, nobody having written it since: it no longer says whether the tower answers
        // (p2778); a little before that it still does
        let later = |s| now + std::time::Duration::from_secs(s);
        assert_eq!(machine(&d.join("machines.json"), later(FRESH_SECS + 1)), None);
        assert_eq!(machine(&d.join("machines.json"), later(FRESH_SECS - 5)).map(|m| m.on), Some(false));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The replays the runner runs now, as layers (Home's Layers now, a2h-speed N-1446 p2815): a job file with a `layer`
    /// whose job runs is one, of kind "run", with its job's id, agent, ticket and the layer's fields; one ended, one
    /// whose runner is gone, and one with no layer are not.
    #[test]
    fn a_running_replays_layer_is_read_from_its_job_file() {
        let d = std::env::temp_dir().join(format!("coordd-runlayers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let layer = serde_json::json!({"up": "/r/replays/7-x/up", "at": "2026-11-24T17:51:00Z",
            "under": [{"what": "record", "record": "research", "line": 185206, "bytes": 1300000000u64}, {"what": "binary", "path": "/wt/coordd-f235052c", "md5": "05431864", "bytes": 212}]});
        let job = serde_json::json!({"pid": 7, "agent": "docket-core", "coord": "/c/research", "verb": "replay", "since": "2026-11-24T18:51:00+0100",
            "step": "replaying", "step_since": "2026-11-24T18:51:00+0100", "items": ["N-1446", "I-1478"], "layer": layer});
        std::fs::write(d.join("7-1795542660.json"), job.to_string()).unwrap();
        let mut ended = job.clone();
        ended["step"] = "ended".into();
        std::fs::write(d.join("8-1795542661.json"), ended.to_string()).unwrap();
        let mut gone = job.clone();
        gone["pid"] = 9.into();
        std::fs::write(d.join("9-1795542662.json"), gone.to_string()).unwrap();
        let mut none = job.clone();
        none.as_object_mut().unwrap().remove("layer");
        std::fs::write(d.join("10-1795542663.json"), none.to_string()).unwrap();
        let r = run_layers(&d, |pid| pid != 9);
        assert_eq!(r, vec![serde_json::json!({"kind": "run", "id": "7-1795542660", "agent": "docket-core", "coord": "/c/research",
            "ticket": "N-1446", "items": ["N-1446", "I-1478"], "verb": "replay", "state": "open",
            "up": "/r/replays/7-x/up", "at": "2026-11-24T17:51:00Z", "under": layer["under"]})]);
        let _ = std::fs::remove_dir_all(&d);
    }
}
