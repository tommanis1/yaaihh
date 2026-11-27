//! The record sent, sealed, to a place of its own as it grows (`--record-place FILE`), and restored from it on a device that has none.

// Off unless the flag is given. The place holds sealed parts and the texts the record is read under. Beside the
// record, `RECORD.place.json` keeps the record's id and the last send's number: a place holding another record,
// or rolled back to an older send, is refused at start. No network call runs on the engine's thread: a task off it
// sends and checks the place, and each change asks only what that task last found. A change is taken only while
// the place was found this engine's at the last send or check (at most `check_ms` ago). A line taken in that while,
// after another engine has the place, is refused at the send: it stays in this record, never in the place's, and the
// refusal names its lines (as a start on this record does after); every change after is refused.

use crate::serve::Engine;
use record_place::{GitRepoPlace, Key, LocalDirPlace, PART_SIZE, PlaceError, PlaceKey, RecordId, RecordPlace, Restored, Sender, restore};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A place picked at run time.
pub type Place = Box<dyn RecordPlace + Send>;

/// The file `--record-place` names: a folder (`dir`) or a git branch (`git`, `work`, `branch`), the key file, the
/// place's name the keys are bound to, the record id it must hold if known, part size, and the send and check delays.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    dir: Option<PathBuf>,
    #[serde(default)]
    git: Option<String>,
    #[serde(default)]
    work: Option<PathBuf>,
    #[serde(default)]
    branch: Option<String>,
    key: PathBuf,
    #[serde(default = "name")]
    name: String,
    #[serde(default)]
    record: Option<String>,
    #[serde(default = "part_size")]
    part_size: usize,
    #[serde(default = "every_ms")]
    every_ms: u64,
    #[serde(default = "check_ms")]
    check_ms: u64,
    #[serde(default = "git_timeout_s")]
    git_timeout_s: u64,
}

fn name() -> String {
    "record".into()
}
fn part_size() -> usize {
    PART_SIZE
}
fn every_ms() -> u64 {
    1000
}
fn check_ms() -> u64 {
    5000
}
fn git_timeout_s() -> u64 {
    60
}

/// ID read from hex.
fn record_id(hex: &str) -> Result<RecordId, String> {
    let b = (0..hex.len()).step_by(2).map(|i| hex.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok())).collect::<Option<Vec<u8>>>();
    b.and_then(|b| b.try_into().ok()).ok_or(format!("{hex:?} is no record id (32 hex digits)"))
}

impl Config {
    /// The config in TEXT; relative paths are read from DIR, the config file's folder.
    pub fn read(text: &str, dir: &Path) -> Result<Config, String> {
        let mut c: Config = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let at = |p: &mut PathBuf| *p = dir.join(&*p);
        at(&mut c.key);
        c.dir.as_mut().map(at);
        c.work.as_mut().map(at);
        if c.part_size == 0 || c.check_ms == 0 {
            return Err("part_size and check_ms are more than 0".into());
        }
        c.record.as_deref().map(record_id).transpose()?;
        Ok(c)
    }

    /// The place the config names.
    pub fn place(&self) -> Result<Place, String> {
        match (&self.dir, &self.git, &self.work, &self.branch) {
            (Some(d), None, None, None) => Ok(Box::new(LocalDirPlace::open(d).map_err(|e| e.to_string())?)),
            (None, Some(url), Some(work), Some(b)) => Ok(Box::new(GitRepoPlace::open(url, work, b).map_err(|e| e.to_string())?.with_timeout(Duration::from_secs(self.git_timeout_s)))),
            _ => Err("a place is `dir`, or `git` with `work` and `branch`".into()),
        }
    }

    /// The key of this place, from the key file; the file's bytes are zeroed once it is made.
    pub fn key(&self) -> Result<PlaceKey, String> {
        Ok(Key::from_file(&self.key).map_err(|e| e.to_string())?.place(&self.name))
    }

    /// How long after a change the next send goes, and how often the place is checked when nothing changes.
    pub fn delays(&self) -> (Duration, Duration) {
        (Duration::from_millis(self.every_ms), Duration::from_millis(self.check_ms))
    }
}

/// What this device knows of the record's place: its id, the last send's number, and the bytes and lines it held.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Known {
    pub record: String,
    pub sends: u64,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub lines: u64,
}

fn beside(record: &Path, end: &str) -> PathBuf {
    let mut p = record.as_os_str().to_owned();
    p.push(end);
    p.into()
}

/// What `RECORD.place.json` says, if it is there.
pub fn known(record: &Path) -> Result<Option<Known>, String> {
    let at = beside(record, ".place.json");
    match std::fs::read(&at) {
        Ok(b) => serde_json::from_slice(&b).map(Some).map_err(|e| format!("{}: {e}", at.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", at.display())),
    }
}

fn keep_known(record: &Path, s: &Sender<Place>) -> Result<(), String> {
    let k = Known { record: record_place::sync::hex(&s.record()), sends: s.sends(), bytes: s.sent() as u64, lines: s.sent_lines() };
    let at = beside(record, ".place.json");
    write_whole(&at, &serde_json::to_vec(&k).unwrap_or_default()).map_err(|e| format!("{}: {e}", at.display()))
}

/// The record's file read whole, empty when there is none.
fn read_all(record: &Path) -> Result<Vec<u8>, String> {
    match std::fs::read(record) {
        Ok(b) => Ok(b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{}: {e}", record.display())),
    }
}

/// The record's file from byte FROM on.
fn read_from(record: &Path, from: usize) -> std::io::Result<Vec<u8>> {
    let mut f = std::fs::File::open(record)?;
    f.seek(SeekFrom::Start(from as u64))?;
    let mut b = Vec::new();
    f.read_to_end(&mut b)?;
    Ok(b)
}

/// BYTES written to TO whole and on disk, and its name too, before this returns.
fn write_whole(to: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = to.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let new = beside(to, ".restoring");
    let mut f = std::fs::File::create(&new)?;
    std::io::Write::write_all(&mut f, bytes)?;
    f.sync_all()?;
    std::fs::rename(&new, to)?;
    std::fs::File::open(dir)?.sync_all()
}

/// The texts kept beside RECORD (`RECORD.texts/HASH.json`) the sender has not yet sent, given to it.
fn add_texts(s: &mut Sender<Place>, record: &Path) -> Result<(), String> {
    let dir = crate::texts::place(record, "x").parent().map(Path::to_path_buf).unwrap_or_default();
    let Ok(all) = std::fs::read_dir(&dir) else { return Ok(()) };
    for e in all.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.ends_with(".json") && !s.has_text(&name) {
            let b = std::fs::read(e.path()).map_err(|e| format!("{}: {e}", dir.display()))?;
            s.add_text(&name, b).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// R's texts put beside RECORD where they are not, each checked to load to the hash it is named by.
fn put_texts(r: &Restored, record: &Path) -> Result<(), String> {
    for (name, b) in &r.texts {
        let Some(hash) = name.strip_suffix(".json") else { continue };
        let to = crate::texts::place(record, hash);
        let texts: Vec<(String, String)> = serde_json::from_slice(b).map_err(|e| format!("the place's text {name}: {e}"))?;
        if crate::codec::hex(&crate::texts::load(&texts)?.hash) != hash {
            return Err(format!("the place's text {name} loads to another hash"));
        }
        if !to.exists() {
            write_whole(&to, b).map_err(|e| format!("{}: {e}", to.display()))?;
        }
    }
    Ok(())
}

/// Before the engine opens RECORD: the place the config names claimed, as [`begin_at`] does it.
pub fn begin(c: &Config, record: &Path) -> Result<(Sender<Place>, String), String> {
    let pin = c.record.as_deref().map(record_id).transpose()?;
    begin_at(c.place()?, &c.key()?, c.part_size, pin, record)
}

/// Before the engine opens RECORD: restored from PLACE if empty, refused if it does not go on from it, then sent
/// to claim it. Refused when the place holds another record than PIN or the one known here, is older than the
/// send known here, or moved on between its restore and this claim.
pub fn begin_at(place: Place, key: &PlaceKey, size: usize, pin: Option<RecordId>, record: &Path) -> Result<(Sender<Place>, String), String> {
    let known = known(record)?;
    let had = known.as_ref().map(|k| record_id(&k.record)).transpose()?;
    if let (Some(p), Some(h)) = (pin, had) && p != h {
        return Err(format!("the config pins record {}, but this record was sent as {}", record_place::sync::hex(&p), record_place::sync::hex(&h)));
    }
    let expect = pin.or(had);
    let mut here = read_all(record)?;
    let (mut s, said) = if place.head().map_err(|e| e.to_string())?.rev.is_empty() {
        if let Some(e) = expect {
            return Err(format!("the place is empty, but it held record {} before: it was reset, or it is another place", record_place::sync::hex(&e)));
        }
        (Sender::start(place, key, size).map_err(|e| e.to_string())?, "a new place".to_string())
    } else {
        let r = restore(&place, key, expect.as_ref()).map_err(|e| e.to_string())?;
        if let Some(k) = known.as_ref().filter(|k| r.sends < k.sends) {
            return Err(format!("the place is at send {}, older than send {} made from here: it was rolled back; restore it from a copy that is not older", r.sends, k.sends));
        }
        let said = if here.iter().all(u8::is_ascii_whitespace) {
            write_whole(record, &r.record).map_err(|e| format!("{}: {e}", record.display()))?;
            here = r.record.clone();
            format!("restored from the place: {}", r.said())
        } else if here.starts_with(&r.record) {
            format!("going on from the place's {}", r.said())
        } else {
            let never = known.as_ref().map_or(String::new(), |k| unsent(&here[(k.bytes as usize).min(here.len())..], k.lines));
            return Err(format!("the record here does not go on from the place's ({}): another device wrote to it since, or this one handed it away; restore from the place to a new record file{never}", r.said()));
        };
        put_texts(&r, record)?;
        (Sender::resume(place, key, size, &r), said)
    };
    add_texts(&mut s, record)?;
    s.send(&here).map_err(|e| e.to_string())?;
    keep_known(record, &s)?;
    Ok((s, said))
}

/// The words naming TAIL's whole lines, from line FIRST of the record on, as lines the place never took; none when there are none.
fn unsent(tail: &[u8], first: u64) -> String {
    match tail.iter().filter(|b| **b == b'\n').count() as u64 {
        0 => String::new(),
        k => format!("; lines {first} to {} of this record ({k} lines) were never sent to the place: they are in this record only", first + k - 1),
    }
}

/// Why changes are not taken now, as the last send or check found the place: none while it is this engine's.
#[derive(Clone, Default)]
pub struct Standing(Arc<Mutex<Option<String>>>);

impl Standing {
    /// Why changes are refused now, if they are.
    pub fn why(&self) -> Option<String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    fn set(&self, why: Option<String>) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = why;
    }
}

/// The guard ENGINE asks before each change, put in before it serves: it reads the standing only, and waits on nothing.
pub fn guard(engine: &Engine) -> Standing {
    let st = Standing::default();
    let g = st.clone();
    engine.guard(Box::new(move || g.why().map_or(Ok(()), Err)));
    st
}

/// What one send or check found.
#[derive(Debug, PartialEq, Eq)]
pub enum Tick {
    Ours,
    Taken(String),
    Unreachable(String),
}

/// How many times, and after how long, a send or check that could not reach the place is tried again.
const RETRIES: [Duration; 3] = [Duration::from_millis(200), Duration::from_millis(1000), Duration::from_millis(3000)];

/// One send of what RECORD gained since the last, or a check when nothing did, tried again while the place cannot be reached.
pub fn tick(s: &mut Sender<Place>, record: &Path) -> Tick {
    let once = |s: &mut Sender<Place>| -> Result<(), PlaceError> {
        add_texts(s, record).map_err(PlaceError::Io)?;
        let size = std::fs::metadata(record).map_err(|e| PlaceError::Io(format!("{}: {e}", record.display())))?.len() as usize;
        if size == s.sent() && !s.waiting() {
            return s.check();
        }
        let tail = read_from(record, s.sent_to()).map_err(|e| PlaceError::Io(format!("{}: {e}", record.display())))?;
        s.send_tail(&tail)?;
        keep_known(record, s).map_err(PlaceError::Io)
    };
    let mut last = String::new();
    for wait in std::iter::once(Duration::ZERO).chain(RETRIES) {
        std::thread::sleep(wait);
        match once(s) {
            Ok(()) => return Tick::Ours,
            Err(PlaceError::Io(e)) => last = e,
            Err(e) => {
                let tail = read_from(record, s.sent()).unwrap_or_default();
                return Tick::Taken(format!("{e}{}", unsent(&tail, s.sent_lines())));
            }
        }
    }
    Tick::Unreachable(last)
}

/// The words a change is refused with once the place is another writer's.
fn refused(e: &str) -> String {
    format!("the record's place took a send from another engine, or is not this record's ({e}); this record no longer goes on from it: restart on the record restored from the place")
}

/// The writer of the place and the record it sends, shared by the sending task and the last send at shutdown.
#[derive(Clone)]
pub struct Keeper {
    s: Arc<Mutex<Sender<Place>>>,
    record: PathBuf,
    standing: Standing,
}

impl Keeper {
    /// The keeper of S sending RECORD, its findings in STANDING.
    pub fn new(s: Sender<Place>, record: PathBuf, standing: Standing) -> Keeper {
        Keeper { s: Arc::new(Mutex::new(s)), record, standing }
    }

    /// One tick, off the engine's thread.
    async fn once(&self) -> Result<Tick, String> {
        let (s, at) = (self.s.clone(), self.record.clone());
        tokio::task::spawn_blocking(move || tick(&mut s.lock().unwrap_or_else(|e| e.into_inner()), &at)).await.map_err(|e| e.to_string())
    }

    /// Sends what the record gains EVERY after each change, and checks the place each CHECK when nothing changes;
    /// once the place is another writer's, every change is refused, and while it cannot be reached, none is taken.
    pub async fn run(self, engine: Rc<Engine>, every: Duration, check: Duration) {
        let mut changes = engine.changes();
        loop {
            match self.once().await {
                Ok(Tick::Ours) => self.standing.set(None),
                Ok(Tick::Unreachable(e)) => {
                    let why = format!("the record's place cannot be reached ({e}); no change is taken until it can, so none is made past another engine's");
                    eprintln!("coordd-v2: record place: {why}");
                    self.standing.set(Some(why));
                }
                Ok(Tick::Taken(e)) => {
                    self.standing.set(Some(refused(&e)));
                    return engine.refuse_changes(refused(&e));
                }
                Err(e) => return engine.refuse_changes(format!("the send to the record's place stopped: {e}")),
            }
            tokio::select! {
                c = changes.changed() => {
                    if c.is_err() {
                        return;
                    }
                    tokio::time::sleep(every).await;
                    changes.borrow_and_update();
                }
                () = tokio::time::sleep(check) => {}
            }
        }
    }

    /// The last send, at shutdown, while the place is still this engine's: what it found.
    pub async fn flush(&self) -> String {
        if let Some(w) = self.standing.why() {
            return format!("not sent: {w}");
        }
        match self.once().await {
            Ok(Tick::Ours) => "all sent".into(),
            Ok(t) => format!("not all sent: {t:?}"),
            Err(e) => format!("not all sent: {e}"),
        }
    }
}
