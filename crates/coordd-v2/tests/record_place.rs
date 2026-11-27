//! The booted daemon with `--record-place`: its record sent sealed to a folder, a second daemon restored from it, the first refused after.

use record_place::{Key, LocalDirPlace, restore};
use serde_json::{Value as Json, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const TEXT: &str = "module t\nFact Note Identified by String\nAct Add(by: String, note: Note)\n  Creates Note(note)\n";

/// Waits until WHAT holds, or fails naming it.
fn until(what: &str, mut ok: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(30);
    while !ok() {
        assert!(Instant::now() < end, "never: {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// One daemon: its process, socket and stderr file.
struct Daemon {
    child: Child,
    sock: PathBuf,
    err: PathBuf,
}

impl Daemon {
    fn err(&self) -> String {
        std::fs::read_to_string(&self.err).unwrap_or_default()
    }
    fn ask(&self, req: Json) -> Json {
        let s = UnixStream::connect(&self.sock).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        (&s).write_all(format!("{req}\n").as_bytes()).unwrap();
        let mut line = String::new();
        BufReader::new(&s).read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }
    fn add(&self, note: &str) {
        let r = self.ask(json!({"do": "act", "name": "t.Add", "args": ["w1", note]}));
        assert_eq!(r["is"], "done", "{r}");
    }
    fn holds(&self, note: &str) -> bool {
        let r = self.ask(json!({"do": "query", "query": format!("Holds(Note(\"{note}\"))")}));
        assert_eq!(r["is"], "done", "{r}");
        r["value"] == json!(true)
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The daemon on RECORD in DIR, as NAME, sending to the place CONFIG names; it serves or ends.
fn boot(dir: &Path, name: &str, record: &Path) -> Result<Daemon, String> {
    let (sock, err) = (dir.join(format!("{name}.sock")), dir.join(format!("{name}.err")));
    let child = Command::new(env!("CARGO_BIN_EXE_coordd-v2"))
        .arg("--socket").arg(&sock).arg("--record").arg(record).arg("--parties").arg(dir.join("parties")).args(["--top", "t"])
        .arg("--record-place").arg(dir.join("place.json")).arg(dir.join("t.docket"))
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(std::fs::File::create(&err).unwrap()).spawn().unwrap();
    let mut d = Daemon { child, sock, err };
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        if d.err().contains("serving") {
            return Ok(d);
        }
        if let Some(st) = d.child.try_wait().unwrap() {
            return Err(format!("{st}: {}", d.err()));
        }
        assert!(Instant::now() < end, "{name} never served: {}", d.err());
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn note(i: usize) -> String {
    format!("note-{i:04}-{}", "x".repeat(80))
}

#[test]
fn a_daemon_sends_its_record_sealed_a_second_restores_from_the_place_and_the_first_takes_no_more_acts() {
    let dir = std::env::temp_dir().join(format!("record-place-daemon-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("a")).unwrap();
    std::fs::create_dir_all(dir.join("b")).unwrap();
    let cgroup = std::fs::read_to_string("/proc/self/cgroup").unwrap().lines().find_map(|l| l.strip_prefix("0::")).unwrap().to_string();
    std::fs::write(dir.join("parties"), format!("{cgroup} w1\n")).unwrap();
    std::fs::write(dir.join("t.docket"), TEXT).unwrap();
    std::fs::write(dir.join("key"), [9u8; 32]).unwrap();
    std::fs::set_permissions(dir.join("key"), std::os::unix::fs::PermissionsExt::from_mode(0o600)).unwrap();
    std::fs::write(dir.join("place.json"), r#"{"dir": "place", "key": "key", "part_size": 2048, "every_ms": 20, "check_ms": 50}"#).unwrap();
    let (key, place) = (Key::from_bytes([9; 32]).place("record"), dir.join("place"));
    let (ra, rb) = (dir.join("a/record.jsonl"), dir.join("b/record.jsonl"));
    let restored = || restore(&LocalDirPlace::open(&place).unwrap(), &key, None).ok();

    // A writes; the place catches up with A's file, cut into closed parts, nothing in them readable
    let a = boot(&dir, "a", &ra).unwrap();
    assert!(a.err().contains("record place: a new place"), "{}", a.err());
    for i in 0..60 {
        a.add(&note(i));
    }
    until("the place holds A's record", || restored().is_some_and(|r| r.record == std::fs::read(&ra).unwrap()));
    let parts: Vec<_> = std::fs::read_dir(place.join("parts")).unwrap().map(|e| std::fs::read(e.unwrap().path()).unwrap()).collect();
    assert!(parts.len() >= 3, "{} closed parts", parts.len());
    assert!(parts.iter().chain([&std::fs::read(place.join("open.part")).unwrap()]).all(|p| !p.windows(5).any(|w| w == b"note-")), "a part left unsealed");

    // B, on no record, restores A's from the place and serves its facts, then writes on
    let mut b = boot(&dir, "b", &rb).unwrap();
    assert!(b.err().contains("restored from the place: "), "{}", b.err());
    assert!(b.holds(&note(0)) && b.holds(&note(59)) && !b.holds(&note(60)));
    b.add(&note(100));
    until("the place holds B's record", || restored().is_some_and(|r| r.record == std::fs::read(&rb).unwrap()));

    // A, the stale writer, finds at its next check that the place is B's: its next act is refused with the place's
    // words, its record unchanged, and it serves reads on
    until("A sees the place is another's", || a.err().contains("no more changes"));
    let before = std::fs::read(&ra).unwrap();
    let r = a.ask(json!({"do": "act", "name": "t.Add", "args": ["w1", note(61)]}));
    assert!(r["is"] == "no" && r["why"].as_str().is_some_and(|w| w.contains("another engine")), "{r}");
    assert_eq!(std::fs::read(&ra).unwrap(), before, "A wrote nothing");
    assert!(a.err().contains("no more changes"), "{}", a.err());
    assert!(a.holds(&note(59)) && !a.holds(&note(61)), "A serves reads");
    b.add(&note(101));
    until("the place holds B's record again", || restored().is_some_and(|r| r.record == std::fs::read(&rb).unwrap()));

    // A started again on its own record is refused before it serves: it does not go on from the place
    drop(a);
    let again = boot(&dir, "a2", &ra).err().unwrap_or_default();
    assert!(again.contains("does not go on from the place's"), "{again}");
    // B, ended by SIGTERM, sends what its record gained last before it ends
    b.add(&note(102));
    assert!(Command::new("kill").arg("-TERM").arg(b.child.id().to_string()).status().unwrap().success());
    until("B ends", || matches!(b.child.try_wait(), Ok(Some(_))));
    assert!(b.err().contains("record place at shutdown: all sent"), "{}", b.err());
    assert_eq!(restored().unwrap().record, std::fs::read(&rb).unwrap());
    drop(b);
    let _ = std::fs::remove_dir_all(&dir);
}
