//! The binary as a window runs it: each request answered on its line, and no file written but the sampler's own.

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Lines, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

/// Every file under DIR with its length and when it was last written.
fn files(dir: &Path) -> BTreeMap<PathBuf, (u64, SystemTime)> {
    let mut seen = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().map(Result::unwrap) {
            let m = std::fs::symlink_metadata(e.path()).unwrap();
            if m.is_dir() {
                stack.push(e.path());
            } else {
                seen.insert(e.path(), (m.len(), m.modified().unwrap()));
            }
        }
    }
    seen
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("machine-state-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("agent-coord/machines")).unwrap();
    d
}

/// The binary at BIN run with HOME and ARGS, asked one line at a time.
struct Run {
    child: Child,
    asked: Option<std::process::ChildStdin>,
    said: Lines<BufReader<ChildStdout>>,
}

impl Run {
    fn new(bin: &Path, home: &Path, args: &[&str]) -> Run {
        let mut child = Command::new(bin).args(args).env("HOME", home).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        let (asked, said) = (child.stdin.take(), BufReader::new(child.stdout.take().unwrap()).lines());
        Run { child, asked, said }
    }

    fn ask(&mut self, q: Value) -> Value {
        writeln!(self.asked.as_mut().unwrap(), "{q}").unwrap();
        serde_json::from_str(&self.said.next().unwrap().unwrap()).unwrap()
    }

    /// Its stdin closed: it ends, and well.
    fn end(mut self) {
        drop(self.asked.take());
        assert!(self.child.wait().unwrap().success(), "it ends when stdin ends");
    }
}

/// A HOME with a repository, a fresh devices' file and a stand-in runner: three requests and a refused one each get
/// their line, and afterwards every file under it is as it was.
#[test]
fn the_binary_answers_each_request_and_writes_nothing() {
    let home = scratch("reads-only");
    let (repo, runner) = (home.join("r"), home.join("agent-coord/build-runner"));
    for d in [&repo, &runner] {
        std::fs::create_dir_all(d).unwrap();
    }
    let git = |args: &[&str]| assert!(Command::new("git").arg("-C").arg(&repo).args(["-c", "user.name=t", "-c", "user.email=t@x"]).args(args).output().unwrap().status.success(), "{args:?}");
    git(&["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), "a").unwrap();
    git(&["add", "a.txt"]);
    git(&["commit", "-qm", "the first commit"]);
    std::fs::write(repo.join("a.txt"), "b").unwrap();
    std::fs::write(home.join("agent-coord/machines/machines.json"), r#"{"machines": [{"name": "d1", "on": true}]}"#).unwrap();
    let l = std::os::unix::net::UnixListener::bind(runner.join("sock")).unwrap();
    let serve = std::thread::spawn(move || {
        let (s, _) = l.accept().unwrap();
        let mut q = String::new();
        BufReader::new(s.try_clone().unwrap()).read_line(&mut q).unwrap();
        (&s).write_all(b"{\"rows\": [{\"id\": \"crates\", \"count\": 3}]}\n").unwrap();
        serde_json::from_str::<Value>(&q).unwrap()
    });
    let before = files(&home);

    let mut run = Run::new(Path::new(env!("CARGO_BIN_EXE_yaaihh-machine-state")), &home, &[]);
    let repos = run.ask(json!({"repos": [["r", repo.display().to_string()]]}));
    assert_eq!((repos[0]["branch"].as_str(), repos[0]["subject"].as_str(), repos[0]["dirty"].as_u64()), (Some("main"), Some("the first commit"), Some(1)), "{repos}");
    assert_eq!(run.ask(json!({"machines": {}}))["text"], r#"{"machines": [{"name": "d1", "on": true}]}"#);
    assert_eq!(run.ask(json!({"store": {"row": null, "from": 0}}))["rows"][0]["count"], 3);
    assert_eq!(run.ask(json!({"free": "/w/target"}))["error"], "asks repos, machines or store");
    let logins = home.join(".ssh").display().to_string();
    assert_eq!(run.ask(json!({"repos": [["k", logins]]})), json!([{"name": "k", "refused": "the owner's logins and settings"}]));
    run.end();
    assert_eq!(serve.join().unwrap(), json!({"store": {"row": null, "from": 0}}));

    assert_eq!(files(&home), before, "not one file written or changed");
    let _ = std::fs::remove_dir_all(&home);
}

/// An old devices' file: an ask starts the sampler beside the binary, not the one given; the sample shows at a later ask,
/// and the devices' file is the one file written.
#[test]
fn an_old_devices_file_starts_the_sampler_beside_the_binary_which_writes_it_alone() {
    let home = scratch("sampler");
    let release = home.join("release");
    std::fs::create_dir_all(&release).unwrap();
    let bin = release.join("yaaihh-machine-state");
    std::fs::copy(env!("CARGO_BIN_EXE_yaaihh-machine-state"), &bin).unwrap();
    std::fs::write(release.join("machines-sample"), "import os\nopen(os.environ['MACHINES_OUT'], 'w').write('{\"read\": \"sampled\"}')\n").unwrap();
    let file = home.join("agent-coord/machines/machines.json");
    std::fs::write(&file, r#"{"read": "old"}"#).unwrap();
    std::fs::File::options().write(true).open(&file).unwrap().set_modified(SystemTime::now() - Duration::from_secs(120)).unwrap();
    let mut before = files(&home);

    let mut run = Run::new(&bin, &home, &["--sampler", "/nowhere/machines-sample"]);
    let t0 = Instant::now();
    while run.ask(json!({"machines": {}}))["text"] != r#"{"read": "sampled"}"# {
        assert!(t0.elapsed() < Duration::from_secs(30), "the sampler beside the binary wrote nothing");
        std::thread::sleep(Duration::from_millis(50));
    }
    run.end();

    let mut after = files(&home);
    assert_ne!(after.remove(&file), before.remove(&file), "the sample written over the old file");
    assert_eq!(after, before, "the devices' file is the one written");
    let _ = std::fs::remove_dir_all(&home);
}
