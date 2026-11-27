//! What the app read on the server's disk itself, served by coordd (N-412 ask 13, the owner's Windows window; form in
//! N-412 p129, p135): the project's folders (`Snapshot.places`), a commit's subject (`Request::Subject`), and the
//! Harness page's data (`Request::Harness`, `Request::HarnessLatest`). An app on another machine has only the
//! connection; these are what its pages read on the machine coordd runs on.
//!
//! The proxy is reached on the coord's own route (`$COORD/route`'s `ANTHROPIC_BASE_URL`, `$COORD/.proxy-key`), never
//! through the owner's `~/.claude` (paper-research p135). The key goes to curl on its stdin, never on a command line,
//! and never into a reply or a log line.
//!
//! The page's switches (`Request::HarnessSet`, N-802.p23): coordd is the one writer of the proxy's rules file,
//! `harness-models.json`, for both windows. Its name is the one the proxy's `/v1/harness-models` answers; coordd
//! never opens the proxy's config, writes nothing else in that folder, and keeps the file as it found it, once per
//! life, as `harness-models.json.previous` (the way back).

use coord_types::Places;
use serde_json::{Value, json};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// The project's folders as the server sees them: the coord's, the project file's, `repo`, `library`, `files`.
pub fn places(coord: &str, project: Option<&crate::project::Project>) -> Places {
    let p = project.filter(|p| !p.dir.is_empty());
    Places {
        coord: coord.to_string(),
        definition: p.map(|p| p.dir.clone()),
        repo: p.and_then(|p| p.repo()),
        library: p.and_then(|p| p.project.library.as_deref().map(|l| p.path(l))),
        files: p.and_then(|p| p.raw.pointer("/project/files")?.as_array().cloned()).unwrap_or_default().iter().filter_map(|f| f.as_str()).map(expand).collect(),
        start: None,
        own_homes: false,
        repos: p.map(|p| p.repos()).unwrap_or_default(),
    }
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

/// `~/x` as a path under HOME, as the app's Files page took `[project] files`.
fn expand(p: &str) -> String {
    match p.strip_prefix("~/") {
        Some(r) => home().join(r).display().to_string(),
        None if p == "~" => home().display().to_string(),
        None => p.to_string(),
    }
}

/// Why HASH is not a commit's name to give git (4 to 64 hex characters), if it is not.
pub fn bad_hash(hash: &str) -> Option<String> {
    let ok = (4..=64).contains(&hash.len()) && hash.chars().all(|c| c.is_ascii_hexdigit());
    (!ok).then(|| format!("{hash:?} is not a commit's hash (4 to 64 hex characters)"))
}

/// The repository REPO names among the project's `repos`, or the project's `repo` when none is named.
pub fn repo_for(project: Option<&crate::project::Project>, repo: Option<&str>) -> Result<String, String> {
    let p = project.ok_or("the project has no project file, so no repository")?;
    match repo.filter(|r| !r.is_empty()) {
        Some(name) => p.repos().into_iter().find(|(n, _)| n == name).map(|(_, r)| r).ok_or_else(|| format!("the project has no repository {name:?} among its repos")),
        None => p.repo().ok_or_else(|| "the project names no repository (`repo`)".to_string()),
    }
}

/// Commit HASH's subject in the repository at REPO.
pub fn subject(repo: &str, hash: &str) -> Result<String, String> {
    if let Some(why) = bad_hash(hash) {
        return Err(why);
    }
    let o = Command::new("git").args(["-C", repo, "log", "-1", "--format=%s", hash, "--"]).stdin(Stdio::null()).output().map_err(|e| format!("git: {e}"))?;
    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    if o.status.success() && !s.is_empty() {
        Ok(s)
    } else {
        Err(format!("{repo} holds no commit {hash}"))
    }
}

fn first_line(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout).lines().next()?.trim().to_string();
    (out.status.success() && !s.is_empty()).then_some(s)
}

/// Claude Code's and Codex's versions on the server: installed and kept, read on the machine at once. The newest
/// upstream is `latest`'s, asked only when the owner asks (`Request::HarnessLatest`).
pub fn versions() -> Value {
    let mut kept: Vec<String> = std::fs::read_dir(home().join(".local/share/claude/versions")).map(|d| d.filter_map(|e| e.ok()?.file_name().into_string().ok()).collect()).unwrap_or_default();
    kept.sort();
    let claude = std::fs::read_link(home().join(".local/bin/claude")).ok().and_then(|p| Some(p.file_name()?.to_string_lossy().into_owned())).unwrap_or_else(|| "not found".into());
    let codex = first_line("codex", &["--version"]).map(|v| v.trim_start_matches("codex-cli ").to_string()).unwrap_or_else(|| "not found".into());
    json!([
        {"name": "Claude Code", "installed": claude, "kept": kept, "latest": null, "how": "native install; updates itself"},
        {"name": "Codex", "installed": codex, "kept": [], "latest": null, "how": "npm global @openai/codex"},
    ])
}

/// The newest Codex upstream, as npm says it (`npm view @openai/codex version`): as long as npm runs, its error as it is.
pub fn latest() -> Result<String, String> {
    let o = Command::new("npm").args(["view", "@openai/codex", "version"]).stdin(Stdio::null()).output().map_err(|e| format!("npm: {e}"))?;
    let s = String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").trim().to_string();
    if o.status.success() && !s.is_empty() {
        Ok(s)
    } else {
        Err(format!("npm view @openai/codex version ({}): {}", o.status, String::from_utf8_lossy(&o.stderr).trim()))
    }
}

/// The coord's route to the proxy: `ANTHROPIC_BASE_URL` in `$COORD/route`, the key in `$COORD/.proxy-key`.
fn route(coord: &str) -> Result<(String, String), String> {
    let file = format!("{coord}/route");
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{file}: {e}"))?;
    let base = text.lines().find_map(|l| l.trim().strip_prefix("ANTHROPIC_BASE_URL=")).map(|v| v.trim().trim_matches('"').to_string()).ok_or_else(|| format!("{file} names no ANTHROPIC_BASE_URL"))?;
    let key = std::fs::read_to_string(format!("{coord}/.proxy-key")).map(|k| k.trim().to_string()).unwrap_or_default();
    if key.is_empty() {
        return Err(format!("{coord}/.proxy-key holds no key"));
    }
    Ok((base, key))
}

/// The proxy's answer to /v1/harness-models, as it is.
pub fn pairs(coord: &str) -> Result<Value, String> {
    let (base, key) = route(coord)?;
    let mut child = Command::new("curl")
        .args(["-sS", "-f", "-H", "@-", &format!("{}/v1/harness-models", base.trim_end_matches('/'))])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl: {e}"))?;
    if let Some(mut i) = child.stdin.take() {
        let _ = writeln!(i, "Authorization: Bearer {key}");
    }
    let out = child.wait_with_output().map_err(|e| format!("curl: {e}"))?;
    if !out.status.success() {
        return Err(format!("the proxy did not answer /v1/harness-models: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("the proxy's answer: {e}"))
}

/// The rules file's one name: any other the proxy might answer is refused.
const RULES_FILE: &str = "harness-models.json";

/// A rule of the proxy's rules file (its internal/harness.Rule): nothing else is taken in it.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Rule {
    model: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    provider: String,
    on: bool,
}

/// The rules file: a harness's name to its ordered rules, the last match winning.
type Rules = std::collections::BTreeMap<String, Vec<Rule>>;

/// What a switch changes (`Request::HarnessSet`).
pub enum Change {
    /// Pairs switched on or off: each pair's exact rules replaced by one at the end.
    Set(Vec<coord_types::RulePair>, bool),
    /// Every pair of the harness on again: its rules dropped.
    AllOn,
}

/// TEXT read as the rules file's shape (`{harness: [{model, provider?, on}]}`, every model named), or why not.
fn shape(text: &str) -> Result<Rules, String> {
    let r: Rules = serde_json::from_str(text).map_err(|e| format!("not the rules file's shape: {e}"))?;
    if r.values().flatten().any(|x| x.model.is_empty()) {
        return Err("not the rules file's shape: a rule names no model".into());
    }
    Ok(r)
}

/// The writes, one at a time (two switches pressed quickly both land).
static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A new file at PATH, never through what is there: a name already taken (a link planted there too) is refused.
fn create_new(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new().write(true).create_new(true).open(path)
}

/// CHANGE made to harness HARNESS's rules in FILE, whole, through a temporary file and a rename; the rules as written.
fn change_rules(file: &std::path::Path, harness: &str, change: Change) -> Result<Rules, String> {
    let name = file.display().to_string();
    if !file.is_absolute() || file.file_name().and_then(|n| n.to_str()) != Some(RULES_FILE) {
        return Err(format!("not set: the proxy names {name} as its rules file, which is not a {RULES_FILE}"));
    }
    if harness.is_empty() {
        return Err("not set: no harness named".into());
    }
    let _one = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    // and one at a time across coordds sharing the proxy (research's, cav-paper's): an exclusive lock on the folder
    // itself, which makes no file there (docket-core3, N-802.p25 remark 1)
    let folder = std::fs::File::open(file.parent().unwrap_or(std::path::Path::new("/"))).map_err(|e| format!("not set: {name}'s folder: {e}"))?;
    folder.lock().map_err(|e| format!("not set: {name}'s folder: {e}"))?;
    let current = match std::fs::symlink_metadata(file) {
        Ok(m) if !m.file_type().is_file() => return Err(format!("not set: {name} is not a plain file (a link is not followed)")),
        Ok(_) => {
            let text = std::fs::read_to_string(file).map_err(|e| format!("not set: {name}: {e}"))?;
            shape(&text).map_err(|e| format!("not set: {name} is {e}; it is left as it is"))?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Rules::new(),
        Err(e) => return Err(format!("not set: {name}: {e}")),
    };
    let mut rules = current;
    match change {
        Change::Set(pairs, on) => {
            let list = rules.entry(harness.to_string()).or_default();
            for p in pairs {
                if p.model.is_empty() {
                    return Err("not set: a pair names no model".into());
                }
                list.retain(|r| !(r.model == p.model && r.provider == p.provider));
                list.push(Rule { model: p.model, provider: p.provider, on });
            }
        }
        Change::AllOn => {
            rules.remove(harness);
        }
    }
    let body = serde_json::to_string_pretty(&rules).map_err(|e| e.to_string())? + "\n";
    // what is written is the rules file's shape, checked as the proxy will read it
    shape(&body)?;
    // the file as it was before coordd ever wrote it: the way back, made once and never replaced (docket-core3,
    // N-802.p25 remarks 2 and 3; anything at that name, a link too, is left as it is)
    let previous = file.with_file_name(format!("{RULES_FILE}.previous"));
    if file.exists() && std::fs::symlink_metadata(&previous).is_err() {
        let kept = create_new(&previous).and_then(|mut to| std::io::copy(&mut std::fs::File::open(file)?, &mut to));
        kept.map_err(|e| format!("not set: keeping {name} as .previous: {e}"))?;
    }
    // the temporary file made new, after whatever had its name is removed
    let tmp = file.with_file_name(format!("{RULES_FILE}.tmp.{}", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    create_new(&tmp).and_then(|mut f| f.write_all(body.as_bytes())).and_then(|_| std::fs::rename(&tmp, file)).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("not set: {name}: {e}")
    })?;
    Ok(rules)
}

/// `Request::HarnessSet`: the rules file named by the proxy, changed; `{"file", "rules"}` as written.
pub fn set_rules(coord: &str, harness: &str, change: Change) -> Result<String, String> {
    let p = pairs(coord)?;
    let file = p.get("file").and_then(Value::as_str).filter(|f| !f.is_empty()).ok_or("not set: the proxy names no rules file")?.to_string();
    let known = p.get("harnesses").and_then(Value::as_array).is_some_and(|h| h.iter().any(|x| x.as_str() == Some(harness)));
    if !known {
        return Err(format!("not set: the proxy has no harness {harness:?}"));
    }
    let rules = change_rules(std::path::Path::new(&file), harness, change)?;
    Ok(json!({"file": file, "rules": rules}).to_string())
}

/// `Request::Harness`'s answer: the versions, and the proxy's pairs or why there are none.
pub fn harness(coord: &str) -> Value {
    // no route file: a coord that reaches no proxy, which is no fault (N-1888)
    if !std::path::Path::new(&format!("{coord}/route")).exists() {
        return json!({"versions": versions(), "pairs": null, "error": null});
    }
    let (pairs, error) = match pairs(coord) {
        Ok(p) => (p, Value::Null),
        Err(e) => (Value::Null, Value::String(e)),
    };
    json!({"versions": versions(), "pairs": pairs, "error": error})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hash_is_hex_of_a_commits_length() {
        assert_eq!(bad_hash("7d22506"), None);
        assert_eq!(bad_hash(&"a".repeat(40)), None);
        for bad in ["", "abc", "--all", "HEAD", "7d22506 x", &"a".repeat(65)] {
            assert!(bad_hash(bad).is_some(), "{bad:?} taken");
        }
    }

    #[test]
    fn the_route_is_the_coords_own() {
        let dir = std::env::temp_dir().join(format!("coordd-route-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let c = dir.display().to_string();
        assert!(route(&c).is_err(), "no route file");
        let none = harness(&c);
        assert!(none["pairs"].is_null() && none["error"].is_null(), "no route is no proxy, not an error: {none}");
        std::fs::write(dir.join("route"), "# a route\nANTHROPIC_BASE_URL=http://127.0.0.1:9\nKEY_COMMAND=/x\n").unwrap();
        assert_eq!(route(&c).unwrap_err(), format!("{c}/.proxy-key holds no key"), "and the key not guessed");
        std::fs::write(dir.join(".proxy-key"), "k1\n").unwrap();
        assert_eq!(route(&c).unwrap(), ("http://127.0.0.1:9".to_string(), "k1".to_string()));
        // the proxy not there: the reason, without the key
        let h = harness(&c);
        assert!(h["pairs"].is_null() && h["error"].as_str().is_some_and(|e| !e.contains("k1")), "{h}");
        assert_eq!(h["versions"].as_array().map(Vec::len), Some(2));
        assert!(h["versions"][1]["latest"].is_null(), "npm is asked only on its own request");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn pair(m: &str, p: &str) -> coord_types::RulePair {
        coord_types::RulePair { model: m.into(), provider: p.into() }
    }

    /// A stand-in for the proxy's `/v1/harness-models` on a local port: the rules file FILE named, its rules read from
    /// it at each request; the coord at DIR routed to it.
    fn stand_in_proxy(dir: &std::path::Path, file: &std::path::Path) -> String {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let file = file.to_path_buf();
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                use std::io::{BufRead, BufReader};
                let mut r = BufReader::new(&s);
                let mut line = String::new();
                while r.read_line(&mut line).is_ok_and(|n| n > 2) {
                    line.clear();
                }
                let rules: Value = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(json!({}));
                let body = json!({"version": "stand-in", "file": file, "harnesses": ["claude-code", "codex"], "rules": rules, "pairs": []}).to_string();
                let _ = write!(&s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("route"), format!("ANTHROPIC_BASE_URL=http://127.0.0.1:{port}\n")).unwrap();
        std::fs::write(dir.join(".proxy-key"), "made-up-key\n").unwrap();
        dir.display().to_string()
    }

    #[test]
    fn a_switch_is_written_by_coordd_and_read_back_from_the_proxy() {
        let dir = std::env::temp_dir().join(format!("coordd-rules-{}", std::process::id()));
        let proxy = dir.join("proxy");
        std::fs::create_dir_all(&proxy).unwrap();
        let file = proxy.join("harness-models.json");
        std::fs::write(&file, r#"{"codex": [{"model": "*", "on": false}]}"#).unwrap();
        std::fs::write(proxy.join("config.yaml"), "untouched").unwrap();
        let coord = stand_in_proxy(&dir.join("coord"), &file);
        let out = set_rules(&coord, "claude-code", Change::Set(vec![pair("claude-opus-5-5", "claude")], false)).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["rules"]["claude-code"], json!([{"model": "claude-opus-5-5", "provider": "claude", "on": false}]));
        // read back through the Harness route: what the proxy has
        let h = harness(&coord);
        assert_eq!(h["pairs"]["rules"]["claude-code"][0]["on"], json!(false), "{h}");
        assert_eq!(h["pairs"]["rules"]["codex"], json!([{"model": "*", "on": false}]), "another harness's rules kept");
        // the same pair again replaces its rule; All on drops the harness's rules
        set_rules(&coord, "claude-code", Change::Set(vec![pair("claude-opus-5-5", "claude")], true)).unwrap();
        assert_eq!(harness(&coord)["pairs"]["rules"]["claude-code"], json!([{"model": "claude-opus-5-5", "provider": "claude", "on": true}]));
        set_rules(&coord, "claude-code", Change::AllOn).unwrap();
        assert!(harness(&coord)["pairs"]["rules"].get("claude-code").is_none());
        // the file as it was before coordd ever wrote it is kept beside it, once, and not replaced by a later write;
        // the proxy's config never touched
        assert_eq!(std::fs::read_to_string(proxy.join("harness-models.json.previous")).unwrap(), r#"{"codex": [{"model": "*", "on": false}]}"#);
        assert_eq!(std::fs::read_to_string(proxy.join("config.yaml")).unwrap(), "untouched");
        let mut names: Vec<String> = std::fs::read_dir(&proxy).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["config.yaml", "harness-models.json", "harness-models.json.previous"]);
        // a harness the proxy does not have
        assert!(set_rules(&coord, "nobody", Change::AllOn).unwrap_err().contains("no harness"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_wrong_name_a_link_and_a_bad_shape_are_refused_and_the_file_left_as_it_is() {
        let dir = std::env::temp_dir().join(format!("coordd-rules-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let set = || Change::Set(vec![pair("m", "p")], false);
        // a name that is not the rules file's
        let other = dir.join("config.yaml");
        std::fs::write(&other, "keys: here").unwrap();
        assert!(change_rules(&other, "claude-code", set()).unwrap_err().contains("not a harness-models.json"));
        assert!(change_rules(std::path::Path::new("harness-models.json"), "claude-code", set()).is_err(), "a relative name");
        assert_eq!(std::fs::read_to_string(&other).unwrap(), "keys: here");
        // a link at the name
        let linked = dir.join("linked");
        std::fs::create_dir_all(&linked).unwrap();
        std::os::unix::fs::symlink(&other, linked.join("harness-models.json")).unwrap();
        assert!(change_rules(&linked.join("harness-models.json"), "claude-code", set()).unwrap_err().contains("not a plain file"));
        assert_eq!(std::fs::read_to_string(&other).unwrap(), "keys: here");
        // a file not of the rules' shape
        let bad = dir.join("bad");
        std::fs::create_dir_all(&bad).unwrap();
        for text in [r#"{"claude-code": [{"model": "m", "on": true, "key": "x"}]}"#, r#"{"claude-code": {"model": "m"}}"#, r#"{"claude-code": [{"model": "", "on": true}]}"#, "not json"] {
            std::fs::write(bad.join("harness-models.json"), text).unwrap();
            assert!(change_rules(&bad.join("harness-models.json"), "claude-code", set()).unwrap_err().contains("shape"), "{text}");
            assert_eq!(std::fs::read_to_string(bad.join("harness-models.json")).unwrap(), text);
        }
        assert!(!bad.join("harness-models.json.previous").exists() && !linked.join("harness-models.json.previous").exists());
        // a link planted at `.previous` is not written through; an old `.previous` stays the way back
        let planted = dir.join("planted");
        std::fs::create_dir_all(&planted).unwrap();
        std::fs::write(planted.join("harness-models.json"), "{}").unwrap();
        std::os::unix::fs::symlink(&other, planted.join("harness-models.json.previous")).unwrap();
        change_rules(&planted.join("harness-models.json"), "claude-code", set()).unwrap();
        assert_eq!(std::fs::read_to_string(&other).unwrap(), "keys: here");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_switch_waits_while_another_coordd_holds_the_folder() {
        let dir = std::env::temp_dir().join(format!("coordd-rules-held-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("harness-models.json");
        // another coordd's lock: its own open of the folder
        let other = std::fs::File::open(&dir).unwrap();
        other.lock().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let f = file.clone();
        std::thread::spawn(move || tx.send(change_rules(&f, "claude-code", Change::AllOn)).unwrap());
        assert!(rx.recv_timeout(std::time::Duration::from_millis(300)).is_err(), "written while the folder was held");
        assert!(!file.exists());
        other.unlock().unwrap();
        assert!(rx.recv().unwrap().is_ok());
        assert!(file.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_switches_pressed_at_once_both_land() {
        let dir = std::env::temp_dir().join(format!("coordd-rules-two-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("harness-models.json");
        let ts: Vec<_> = (0..8)
            .map(|i| {
                let f = file.clone();
                std::thread::spawn(move || change_rules(&f, "claude-code", Change::Set(vec![pair(&format!("m{i}"), "p")], false)).unwrap())
            })
            .collect();
        for t in ts {
            t.join().unwrap();
        }
        let r = shape(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(r["claude-code"].len(), 8, "{r:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_places_are_the_project_files_as_the_server_sees_them() {
        let dir = std::env::temp_dir().join(format!("coordd-places-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        let mut p: crate::project::Project = toml::from_str("[project]\nname = \"x\"\nrepo = \".\"\nlibrary = \"lib\"\nfiles = [\"~/notes\", \"/srv/x\"]\n").unwrap();
        p.dir = dir.display().to_string();
        p.raw = json!({"project": {"files": ["~/notes", "/srv/x"]}});
        let pl = places("/c", Some(&p));
        assert_eq!(pl.coord, "/c");
        assert_eq!(pl.definition.as_deref(), Some(p.dir.as_str()));
        assert_eq!(pl.repo, p.repo());
        assert_eq!(pl.library, Some(p.path("lib")));
        assert_eq!(pl.files, vec![home().join("notes").display().to_string(), "/srv/x".to_string()]);
        assert_eq!(places("/c", None), Places { coord: "/c".into(), ..Default::default() });
        assert_eq!(repo_for(Some(&p), Some("nope")).unwrap_err(), "the project has no repository \"nope\" among its repos");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
