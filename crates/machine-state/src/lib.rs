//! What Home and a project's Overview show of the machine, read when a page asks: each repository as git says it, the
//! devices' file and the build-runner's store. One line of JSON asked, one answered; nothing written to any record, and
//! nothing anywhere but the devices' file, which the sampler writes.

pub mod kept;
pub mod machines;
pub mod repos;
pub mod store;

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Where the answers are read: the owner's home, the devices' folder, the sampler that refreshes it, the runner's socket.
pub struct Places {
    pub home: PathBuf,
    pub machines: Result<PathBuf, String>,
    pub sampler: Option<PathBuf>,
    pub runner: PathBuf,
}

impl Places {
    /// This user's own, under HOME; the sampler beside this binary, else GIVEN.
    pub fn home(given: Option<PathBuf>) -> Places {
        let beside = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
        let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
        Places { home, machines: machines::folder(), sampler: sampler(beside.as_deref(), given), runner: store::socket() }
    }
}

/// The sampler `machines-sample` in the folder BESIDE, so a release swaps the two together; else GIVEN.
fn sampler(beside: Option<&Path>, given: Option<PathBuf>) -> Option<PathBuf> {
    beside.map(|d| d.join("machines-sample")).filter(|p| p.is_file()).or(given)
}

/// The answer to one asked line Q: `{"repos": [[name, folder], …]}`, `{"machines": {}}` or `{"store": {"row", "from"}}`;
/// anything else `{"error"}`.
pub fn answer(q: &str, at: &Places) -> Value {
    let q: Value = match serde_json::from_str(q) {
        Ok(q) => q,
        Err(e) => return json!({"error": format!("not JSON: {e}")}),
    };
    if let Some(list) = q.get("repos") {
        let pair = |p: &Value| Some((p.get(0)?.as_str()?.to_string(), p.get(1)?.as_str()?.to_string()));
        return match list.as_array().and_then(|a| a.iter().map(pair).collect::<Option<Vec<_>>>()) {
            Some(repos) => repos::all(&repos, &at.home),
            None => json!({"error": "repos takes [[name, folder], …]"}),
        };
    }
    if q.get("machines").is_some() {
        return match &at.machines {
            Ok(folder) => {
                let now = SystemTime::now();
                if let Some(s) = &at.sampler {
                    machines::refresh(folder, s, now);
                }
                machines::read(folder, now)
            }
            Err(why) => json!({"text": null, "why": why}),
        };
    }
    if let Some(s) = q.get("store") {
        let (row, from) = (s.get("row").and_then(Value::as_str), s.get("from").and_then(Value::as_u64).unwrap_or(0));
        return match store::rows(&at.runner, row, from) {
            Ok(line) => serde_json::from_str(&line).unwrap_or_else(|e| json!({"error": e.to_string()})),
            Err(why) => json!({"error": why}),
        };
    }
    json!({"error": "asks repos, machines or store"})
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each request has its answer; a malformed or unknown one says what is asked.
    #[test]
    fn each_request_is_answered_and_anything_else_says_what_it_asks() {
        let d = std::env::temp_dir().join(format!("machine-state-answer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("plain")).unwrap();
        std::fs::write(d.join("machines.json"), r#"{"machines": []}"#).unwrap();
        let sock = d.join("sock");
        let serve = store::tests::runner(&sock, &[r#"{"rows": []}"#]);
        let at = Places { home: d.clone(), machines: Ok(d.clone()), sampler: None, runner: sock };
        let plain = d.join("plain").display().to_string();
        assert_eq!(answer(&json!({"repos": [["plain", plain]]}).to_string(), &at), json!([{"name": "plain", "path": plain}]));
        assert_eq!(answer(r#"{"machines": {}}"#, &at)["text"], r#"{"machines": []}"#);
        assert_eq!(answer(r#"{"store": {"row": null, "from": 0}}"#, &at), json!({"rows": []}));
        assert_eq!(serve.join().unwrap(), [json!({"store": {"row": null, "from": 0}})]);
        assert_eq!(answer(r#"{"repos": ["plain"]}"#, &at), json!({"error": "repos takes [[name, folder], …]"}));
        let store = d.join("agent-coord/accounts").display().to_string();
        assert_eq!(answer(&json!({"repos": [["a", store]]}).to_string(), &at), json!([{"name": "a", "refused": "the owner's logins and settings"}]));
        assert_eq!(answer(r#"{"free": "/w/target"}"#, &at), json!({"error": "asks repos, machines or store"}), "no delete");
        assert!(answer("repos", &at)["error"].as_str().is_some_and(|e| e.starts_with("not JSON")));
        let at = Places { home: d.clone(), machines: Err("no home".into()), sampler: None, runner: d.join("none") };
        assert_eq!(answer(r#"{"machines": {}}"#, &at), json!({"text": null, "why": "no home"}));
        assert!(answer(r#"{"store": {}}"#, &at)["error"].as_str().is_some_and(|e| e.contains("the runner's service at")));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The sampler beside the binary wins over the one given; with none beside it, the given one.
    #[test]
    fn the_sampler_beside_the_binary_comes_first() {
        let d = std::env::temp_dir().join(format!("machine-state-sampler-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("release")).unwrap();
        let given = Some(PathBuf::from("/given/machines-sample"));
        assert_eq!(sampler(Some(&d.join("release")), given.clone()), given, "none beside it");
        assert_eq!(sampler(None, None), None);
        std::fs::write(d.join("release/machines-sample"), "").unwrap();
        assert_eq!(sampler(Some(&d.join("release")), given), Some(d.join("release/machines-sample")));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The reader's code names none of the places v1 keeps out of every walk and every answer, but in kept.rs, which
    /// names them to refuse them.
    #[test]
    fn the_reader_names_no_path_v1_keeps_out() {
        let kept_out = ["agent-coord/accounts", "/route", "\"route\"", "codex-home", "claude-home", ".proxy-key", "telemetry.token", "usage.db", "proxy"];
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for e in std::fs::read_dir(&src).unwrap().map(Result::unwrap).filter(|e| e.file_name() != "kept.rs") {
            let text = std::fs::read_to_string(e.path()).unwrap();
            let code = text.split("#[cfg(test)]").next().unwrap();
            for p in kept_out {
                assert!(!code.contains(p), "{} names {p}", e.path().display());
            }
        }
    }
}
