//! A start's replay saved at a cut (I-1509; paper-research N-1446 p1526, p1538). The record stays the one truth: a saved
//! state is a cache of a replay of its lines up to a cut, so that a start loads it and replays only the lines after
//! the cut ([`start`]). Three make one: a start that replayed the whole record, just before its replay's end; coordd's
//! own program run beside it as the record grows (`coordd save-replay COORD`, [`save_on`]), which reads only the
//! record's file and takes no lock; and release.sh's coord-switch, which runs the new release's program so before it
//! stops the old coordd, so the new one's start replays only what came after.
//!
//! What is saved is all `Tickets::load_joint` makes of the record's lines before its replay's end ([`AtCut`]): the
//! machine but for its defs and its views' caches, the graph's builder with its problems, the agents' side and git's
//! side as the lines make them, the text places, the quiet's jobs and turn ends, the lines that did not read and what
//! the replay found diverging; and the joint's history (`joint::History`, p1644): each input that names a ticket held
//! at the cut, by its place in the record, with what the machine showed after it. It is `$COORD/tickets/replay/NAME.state` (NAME from the key), written beside it and
//! renamed; the states of the two newest keys are kept (this release's and the one before it, for a way back).
//!
//! Its key ([`Key`]): the sha256 of coordd's own program, so a state made by any other build is never loaded; and of
//! the defs as read, since a replay checks a recorded program against today's kinds, each folder they were read from
//! standing as its place, so the gate's save on a copy of the record keys as the install's start does (`coordd
//! save-replay COORD SEED...` goes on from it; paper-research N-1446 p2658). Its cut ([`Cut`]): the line of
//! the record it stands at, its byte, and the sha256 of the record's bytes before it, so a record set aside, put back
//! or edited is replayed whole. The lines after the cut must not answer differently what the lines before it looked
//! up ahead (`Machine::reaches_back`), nor rekind a ticket (the side takes its posts' ids from the machine as it ends),
//! nor hold again a ticket ended at the cut (reopened, undone: the joint's history kept none of its inputs).
//! Any of these not so, or a state that does not read: the whole record is replayed, as before, and the journal says
//! why.

use super::quiet::Quiet;
use super::text_places::TextPlaces;
use super::GraphGit;
use crate::follow::{KeptSide, SideSaved};
use crate::joint::History;
use docket_machine::graph::Builder;
use docket_machine::graph_git::{Job, ReleaseAct};
use docket_machine::machine::Counted;
use docket_machine::{Defs, Line, Machine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufReader, BufWriter, Read, Write};

/// A saved state's first bytes, its format's version with them.
const MAGIC: &[u8] = b"coordd replay state 3\n";

/// The saved states' folder, in $COORD.
pub const FOLDER: &str = "tickets/replay";

/// How many keys' states are kept: this release's and the one before it.
const KEPT: usize = 2;

/// Whose a saved state is: the sha256 of the program that made it, and of the defs it replayed with (their Debug form,
/// which holds the kinds, calls, events and duties as read and the first ticket number).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Key {
    pub exe: String,
    pub defs: String,
}

impl Key {
    /// This program's key for DEFS (as `Tickets::read_defs` gave them, before anything is changed in them), read from
    /// FOLDERS (`ticket::folders`: agent-base, the library, the project): in it each folder stands as its place among
    /// them ([`placeless`]), so the same files read from another place give the same key. The gate's replay of a copy
    /// (its release folder's agent-base, its project copy) saves the state the install then goes on from (paper-research
    /// N-1446 p2658).
    pub fn of(defs: &Defs, folders: &[String]) -> Result<Key, String> {
        Ok(Key { exe: exe_digest()?, defs: hex(&Sha256::digest(placeless(&format!("{defs:?}"), folders).as_bytes())) })
    }

    /// Its state's file name, without the folder.
    fn name(&self) -> String {
        format!("{}.state", &hex(&Sha256::digest(format!("{} {}", self.exe, self.defs).as_bytes()))[..16])
    }
}

/// The sha256 of this program as it runs (`/proc/self/exe`, the file it was started from even after another replaced
/// it), read once.
fn exe_digest() -> Result<String, String> {
    static EXE: std::sync::OnceLock<Result<String, String>> = std::sync::OnceLock::new();
    EXE.get_or_init(|| {
        let mut f = std::fs::File::open("/proc/self/exe").map_err(|e| format!("this program: {e}"))?;
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 1 << 20];
        loop {
            match f.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => h.update(&buf[..n]),
                Err(e) => return Err(format!("this program: {e}")),
            }
        }
        Ok(hex(&h.finalize()))
    })
    .clone()
}

/// TEXT, the defs' Debug form, with each of FOLDERS as Debug writes it (each kind's and binding's file starts with one)
/// standing as its place among them, `<folder N>`; the longest first, so that a folder inside another stays whole.
fn placeless(text: &str, folders: &[String]) -> String {
    let mut by_length: Vec<(usize, String)> = folders.iter().map(|f| format!("{f:?}")).map(|f| f[1..f.len() - 1].to_string()).enumerate().filter(|(_, f)| !f.is_empty()).collect();
    by_length.sort_by_key(|(_, f)| std::cmp::Reverse(f.len()));
    by_length.into_iter().fold(text.to_string(), |t, (i, f)| t.replace(&f, &format!("<folder {i}>")))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}

/// Where a saved state stands in the record: the lines before it that read (the replay's count of lines), its byte,
/// the file's lines before it, and the sha256 of the record's bytes before it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cut {
    pub lines: usize,
    pub bytes: usize,
    pub file_lines: usize,
    pub prefix: String,
}

impl Cut {
    /// The cut before byte BYTES of TEXT (the record), the lines that read and the file's lines before it given.
    fn of(text: &str, bytes: usize, lines: usize, file_lines: usize) -> Cut {
        Cut { lines, bytes, file_lines, prefix: hex(&Sha256::digest(&text.as_bytes()[..bytes])) }
    }

    /// Why TEXT, the record now, is not the record this cut was made on: shorter, or other bytes before it.
    fn reaches(&self, text: &str) -> Option<String> {
        if text.len() < self.bytes {
            return Some(format!("the record is shorter ({} bytes) than where the state was cut ({})", text.len(), self.bytes));
        }
        (hex(&Sha256::digest(&text.as_bytes()[..self.bytes])) != self.prefix).then(|| format!("the record's first {} bytes are not those the state was cut from", self.bytes))
    }
}

/// A replay of the record from its first line, stopped at a cut: all `Tickets::load_joint` makes of the lines before
/// its replay's end, with the lines that did not read (as `read_log` says them) and what the replay found diverging
/// (the graph's own problems are its builder's).
pub struct AtCut {
    pub m: Machine,
    pub graph: Builder,
    pub side: KeptSide,
    pub places: TextPlaces,
    pub quiet: Quiet,
    pub git: GraphGit,
    pub unread: Vec<String>,
    pub problems: Vec<String>,
    /// The joint's history, taken along the replay and kept at the cut (`joint::History`; p1644): always, the joint
    /// on or off, so that a state serves a start with it on.
    pub joint: History,
    pub cut: Cut,
}

/// An [`AtCut`] as it is written: the parts the lines make, in an order that does not change with how they are held.
#[derive(Serialize)]
struct Writing<'a> {
    m: &'a Machine,
    graph: &'a Builder,
    /// The graph's runner jobs and release acts, which its own serde leaves out (the window is not sent them) and
    /// decisions read (`graph_git::facts`, `live_commit`; docket-core N-1446 p1702).
    graph_jobs: &'a BTreeMap<String, Job>,
    graph_releases: &'a [ReleaseAct],
    side: Vec<SideSaved>,
    places: BTreeMap<String, BTreeSet<(u64, u64)>>,
    quiet: &'a Quiet,
    git: (BTreeSet<String>, BTreeSet<String>),
    unread: &'a [String],
    problems: &'a [String],
    joint: &'a History,
}

/// An [`AtCut`] as it is read.
#[derive(Deserialize)]
struct Reading {
    m: Machine,
    graph: Builder,
    graph_jobs: BTreeMap<String, Job>,
    graph_releases: Vec<ReleaseAct>,
    side: Vec<SideSaved>,
    places: BTreeMap<String, BTreeSet<(u64, u64)>>,
    quiet: Quiet,
    git: (BTreeSet<String>, BTreeSet<String>),
    unread: Vec<String>,
    problems: Vec<String>,
    joint: History,
}

impl AtCut {
    /// LINES, the record's from its first line to CUT, replayed with DEFS; PLACES, each line's bytes in TEXT, the
    /// record; UNREAD: the lines before the cut that did not read.
    pub fn replayed(defs: Defs, text: &str, lines: &[Line], places: &[(usize, usize)], unread: Vec<String>, cut: Cut) -> AtCut {
        let mut m = Machine::load(defs);
        let mut graph = Builder::default();
        let mut joint = History::default();
        let problems = m.replay_to(lines, &mut |m, i, group| {
            graph.after(m, i, group);
            joint.take(m, i, text, &places[i..i + group.len()]);
        });
        joint.cut(&m);
        let mut at = AtCut { m, graph, side: KeptSide::default(), places: TextPlaces::default(), quiet: Quiet::default(), git: GraphGit::default(), unread, problems, joint, cut };
        at.take(lines, 0);
        at
    }

    /// LINES, the record's from its line FROM (from 0), taken by the side, the places, the quiet and git's side, as
    /// the start's replay takes the record's lines.
    fn take(&mut self, lines: &[Line], from: usize) {
        let m = &self.m;
        self.side.take(lines, from, &|t, n| docket_machine::graph::post_id(m, t, n));
        lines.iter().for_each(|l| self.places.take(l));
        lines.iter().for_each(|l| self.quiet.take(l));
        lines.iter().for_each(|l| self.git.take(l));
    }

    /// Gone on with TAIL, the record's lines after its cut up to CUT (PLACES: each one's bytes in TEXT, the record;
    /// UNREAD: those between that did not read). Err, when only a replay from the record's first line gives what TAIL
    /// makes: it answers differently what the lines before looked up ahead, rekinds a ticket, or holds again a ticket
    /// ended at the cut, whose inputs the joint's history did not keep (p1644).
    pub fn on(mut self, text: &str, tail: &[Line], places: &[(usize, usize)], unread: Vec<String>, cut: Cut) -> Result<AtCut, String> {
        let from = self.cut.lines;
        if let Some(i) = tail.iter().position(|l| matches!(l, Line::Edited { what, .. } if what == "rekind")) {
            return Err(format!("line {} rekinds a ticket, and the agents' side takes the ids of its posts as the machine ends", from + i + 1));
        }
        let AtCut { m, graph, joint, .. } = &mut self;
        let problems = m.replay_on(tail, from, &mut |m, i, group| {
            graph.after(m, i, group);
            joint.take(m, i, text, &places[i - from..i - from + group.len()]);
        })?;
        if let Some(t) = self.joint.held_again(&self.m) {
            let named = places.iter().position(|(a, b)| crate::joint::touched_text(&text[*a..*b]).contains(&t));
            return Err(format!("{} ended at the cut and is held again after it{}: the joint's history kept none of its inputs before the cut", self.m.id(t), named.map_or(String::new(), |i| format!(" (named again at line {})", from + i + 1))));
        }
        self.joint.cut(&self.m);
        self.problems.extend(problems);
        self.unread.extend(unread);
        self.take(tail, from);
        self.cut = cut;
        Ok(self)
    }

    /// Ended at the record's last line, as a start's replay ends: the machine's end ([`Machine::replay_end`]), then
    /// everything it found, in LOG's words: the lines that did not read, what the replay found diverging, and where
    /// the graph and the record disagree.
    pub fn end(mut self, log: &str) -> Ended {
        self.m.replay_end();
        let mut errors = self.unread;
        errors.extend(self.problems.into_iter().chain(std::mem::take(&mut self.graph.problems)).map(|p| format!("{log}: {p}")));
        Ended { m: self.m, graph: self.graph, side: self.side, places: self.places, quiet: self.quiet, git: self.git, errors, joint: self.joint, lines: self.cut.lines }
    }

    /// Written in COORD's folder for KEY (beside it, then renamed; then only the newest [`KEPT`] keys' states kept):
    /// its size.
    pub fn save(&self, coord: &str, key: &Key) -> Result<u64, String> {
        let dir = format!("{coord}/{FOLDER}");
        std::fs::create_dir_all(&dir).map_err(|e| format!("{dir}: {e}"))?;
        clear_unfinished(&dir);
        let file = format!("{dir}/{}", key.name());
        let tmp = format!("{file}.{}.tmp", std::process::id());
        let written = (|| -> Result<u64, String> {
            let f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
            let mut w = BufWriter::with_capacity(1 << 20, f);
            w.write_all(MAGIC).map_err(|e| e.to_string())?;
            rmp_serde::encode::write_named(&mut w, &(key, &self.cut)).map_err(|e| e.to_string())?;
            let parts = Writing { m: &self.m, graph: &self.graph, graph_jobs: &self.graph.graph.jobs, graph_releases: &self.graph.graph.releases, side: self.side.saved(), places: self.places.saved(), quiet: &self.quiet, git: self.git.saved(), unread: &self.unread, problems: &self.problems, joint: &self.joint };
            rmp_serde::encode::write_named(&mut w, &parts).map_err(|e| e.to_string())?;
            let f = w.into_inner().map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
            Ok(f.metadata().map_err(|e| e.to_string())?.len())
        })();
        let size = written.and_then(|n| std::fs::rename(&tmp, &file).map(|_| n).map_err(|e| e.to_string())).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("{file}: {e}")
        })?;
        keep_newest(&dir);
        Ok(size)
    }

    /// The state saved in COORD's folder for KEY, its machine given DEFS (the ones KEY was made of).
    pub fn read(coord: &str, key: &Key, defs: Defs) -> Result<AtCut, String> {
        let file = format!("{coord}/{FOLDER}/{}", key.name());
        if std::fs::metadata(&file).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
            return Err(why_none(coord, key));
        }
        AtCut::read_file(&file, key, defs)
    }

    /// The state in FILE, when it was saved for KEY, its machine given DEFS: one in a coord's folder ([`AtCut::read`]),
    /// or a seed, saved on a copy of the record ([`save_on`]).
    pub fn read_file(file: &str, key: &Key, defs: Defs) -> Result<AtCut, String> {
        let f = std::fs::File::open(file).map_err(|e| format!("{file}: {e}"))?;
        let mut r = BufReader::with_capacity(1 << 20, f);
        let (k, cut) = header(&mut r).map_err(|e| format!("{file}: {e}"))?;
        if k != *key {
            return Err(format!("{file} was made by another program or with other defs"));
        }
        let read: Reading = rmp_serde::from_read(&mut r).map_err(|e| format!("{file}: {e}"))?;
        let mut m = read.m;
        m.defs = Counted::new(defs);
        let mut graph = read.graph;
        graph.graph.jobs = read.graph_jobs;
        graph.graph.releases = read.graph_releases;
        Ok(AtCut { m, graph, side: KeptSide::from_saved(read.side), places: TextPlaces::from_saved(read.places), quiet: read.quiet, git: GraphGit::from_saved(read.git), unread: read.unread, problems: read.problems, joint: read.joint, cut })
    }
}

/// A saved state's first part, R read past its magic: its key and its cut.
fn header(r: &mut impl Read) -> Result<(Key, Cut), String> {
    let mut magic = vec![0u8; MAGIC.len()];
    r.read_exact(&mut magic).map_err(|e| e.to_string())?;
    if magic != MAGIC {
        return Err("not a saved replay of this format".into());
    }
    rmp_serde::from_read(r).map_err(|e| e.to_string())
}

/// Why COORD has no state for KEY, as the newest state it has says: made by another program, with other defs, or none.
fn why_none(coord: &str, key: &Key) -> String {
    let dir = format!("{coord}/{FOLDER}");
    let newest = states(&dir).into_iter().next();
    let Some(path) = newest else { return "no state is saved".into() };
    let read = std::fs::File::open(&path).map_err(|e| e.to_string()).and_then(|f| header(&mut BufReader::new(f)));
    match read {
        Ok((k, _)) => match (k.exe == key.exe, k.defs == key.defs) {
            (false, false) => format!("the state saved ({path}) was made by another program, with other defs"),
            (false, true) => format!("the state saved ({path}) was made by another program"),
            (true, false) => format!("the state saved ({path}) was made with other defs"),
            (true, true) => format!("the state saved ({path}) is under another name"),
        },
        Err(e) => format!("the state saved ({path}) does not read: {e}"),
    }
}

/// The saved states in DIR, the newest first.
fn states(dir: &str) -> Vec<String> {
    let mut found: Vec<(std::time::SystemTime, String)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".state"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path().display().to_string())))
        .collect();
    found.sort_by(|a, b| b.cmp(a));
    found.into_iter().map(|(_, p)| p).collect()
}

/// Only the newest [`KEPT`] states in DIR kept.
fn keep_newest(dir: &str) {
    for old in states(dir).into_iter().skip(KEPT) {
        let _ = std::fs::remove_file(old);
    }
}

/// The states left half written in DIR by a process that is gone (`NAME.state.PID.tmp`) taken away.
fn clear_unfinished(dir: &str) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(pid) = name.strip_suffix(".tmp").and_then(|n| n.rsplit('.').next()) else { continue };
        if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// One line of the record as read: its byte in the text read and the byte after its text (its end of line not with
/// it), its line in the file (from 1), and the line, or why it does not read.
pub struct Read1 {
    pub byte: usize,
    pub end: usize,
    pub file_line: usize,
    pub line: Result<Line, String>,
}

/// TEXT, the record's from its byte BYTE0 and its file line FILE0 + 1 on, line by line as `str::lines` cuts it.
pub fn read_lines(text: &str, byte0: usize, file0: usize) -> Vec<Read1> {
    let mut out = Vec::new();
    let mut at = 0;
    for (i, piece) in text.split_inclusive('\n').enumerate() {
        let l = piece.strip_suffix('\n').map_or(piece, |l| l.strip_suffix('\r').unwrap_or(l));
        out.push(Read1 { byte: byte0 + at, end: byte0 + at + l.len(), file_line: file0 + i + 1, line: serde_json::from_str::<Line>(l).map_err(|e| e.to_string()) });
        at += piece.len();
    }
    out
}

/// READ's lines that read with each one's place (its bytes in the text read), and LOG's words for each that did not.
pub fn split(log: &str, read: Vec<Read1>) -> (Vec<Line>, Vec<(usize, usize)>, Vec<String>) {
    let mut lines = Vec::new();
    let mut places = Vec::new();
    let mut unread = Vec::new();
    for r in read {
        match r.line {
            Ok(l) => {
                lines.push(l);
                places.push((r.byte, r.end));
            }
            Err(e) => unread.push(format!("{log}:{}: not a line of the ticket log: {e}", r.file_line)),
        }
    }
    (lines, places, unread)
}

/// Where the record TEXT is cut when lines may still be being written after it: the start of the last input among its
/// whole lines. A half-written line is left out, and so is an input whose outputs may still be being written. [`save_on`]
/// saves its state there. A replay of a live record stops there when no line is given (`REPLAY_BYTES=input`,
/// [`read_record`]). Read back from the last whole line, one line at a time, each line read as [`read_lines`] reads it.
/// None when the whole lines hold no input.
pub fn last_input_at(text: &str) -> Option<usize> {
    let mut end = text.rfind('\n').map_or(0, |i| i + 1);
    while end > 0 {
        let start = text[..end - 1].rfind('\n').map_or(0, |i| i + 1);
        let l = &text[start..end - 1];
        if serde_json::from_str::<Line>(l.strip_suffix('\r').unwrap_or(l)).is_ok_and(|l| l.is_input()) {
            return Some(start);
        }
        end = start;
    }
    None
}

/// The coord's record LOG as a load reads it (`Tickets::load_joint`, `Tickets::read_log`). Normally it is read whole.
///
/// The exception is coordd's test binary, which is what a replay runs, when LOG is REPLAY_COORD's record and REPLAY_BYTES
/// is set. Then the record is read only up to the cut REPLAY_BYTES gives (TO, [`read_to`]). That is how a replay reads a
/// live record through `build-runner replay --record`'s view with no copy (a2h-speed H-1590 p1; paper-research H-1590
/// p2). The second value is what was read, said once; an Err is why the record reads as nothing.
pub fn read_record(log: &str) -> Result<(String, Option<String>), String> {
    #[cfg(test)]
    if let Some(to) = replay_to(log) {
        return read_to(log, &to).map(|(text, said)| (text, Some(said)));
    }
    Ok((std::fs::read_to_string(log).unwrap_or_default(), None))
}

/// REPLAY_BYTES, when it is set and LOG is REPLAY_COORD's record: the cut a replay's view reads up to.
#[cfg(test)]
fn replay_to(log: &str) -> Option<String> {
    let coord = std::env::var("REPLAY_COORD").ok()?;
    let to = std::env::var("REPLAY_BYTES").ok().filter(|b| !b.is_empty())?;
    (std::path::Path::new(log) == std::path::Path::new(&format!("{coord}/tickets/log.jsonl"))).then_some(to)
}

/// The record LOG read only up to TO, and what was read, in one line. TO is either:
/// - a number N: the first N bytes, which must end a whole line (`--line N`'s byte), and nothing past them is read;
/// - `input`: up to [`last_input_at`] in what the file holds now.
///
/// It is an Err when N is past the record's end or is no line's end, when TO is neither, and when the whole lines hold
/// no input. Only coordd's test binary has it.
#[cfg(test)]
pub fn read_to(log: &str, to: &str) -> Result<(String, String), String> {
    use std::io::Read;
    let n = match to {
        "input" => None,
        _ => Some(to.parse::<u64>().map_err(|_| format!("{log}: REPLAY_BYTES={to}: neither a byte nor `input`"))?),
    };
    let mut f = std::fs::File::open(log).map_err(|e| format!("{log}: {e}"))?;
    let mut bytes = Vec::new();
    match n {
        Some(n) => Read::take(&mut f, n).read_to_end(&mut bytes),
        None => f.read_to_end(&mut bytes),
    }
    .map_err(|e| format!("{log}: {e}"))?;
    let whole = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    if let Some(n) = n {
        if bytes.len() as u64 != n {
            return Err(format!("{log}: REPLAY_BYTES={n}: past the record's end, at byte {}", bytes.len()));
        }
        if whole != bytes.len() {
            return Err(format!("{log}: REPLAY_BYTES={n}: not the end of a line"));
        }
    }
    bytes.truncate(whole);
    let mut text = String::from_utf8(bytes).map_err(|e| format!("{log}: not text up to byte {whole}: {e}"))?;
    if n.is_none() {
        let cut = last_input_at(&text).ok_or_else(|| format!("{log}: REPLAY_BYTES=input: no input among its {} whole lines", text.lines().count()))?;
        text.truncate(cut);
    }
    let said = format!("coordd: tickets: {log} read up to byte {}, its first {} lines (REPLAY_BYTES={to})", text.len(), text.lines().count());
    Ok((text, said))
}

/// A replay ended at the record's last line ([`AtCut::end`]): all the start's replay makes, everything it found, and the
/// record's lines that read.
pub struct Ended {
    pub m: Machine,
    pub graph: Builder,
    pub side: KeptSide,
    pub places: TextPlaces,
    pub quiet: Quiet,
    pub git: GraphGit,
    pub errors: Vec<String>,
    /// The joint's history to its last cut, and every input after it: what the joint's start is fed of it.
    pub joint: History,
    pub lines: usize,
}

/// What a start's replay made ([`start`]): the replay ended, the line the newest state saved stands at, and what the
/// journal says of how it went.
pub struct Started {
    pub ended: Ended,
    pub saved_at: Option<usize>,
    pub said: String,
}

/// The start's replay of LOG (TEXT, the record as read) in COORD with DEFS, read from FOLDERS ([`Key::of`]): with STATES,
/// from the state saved for this program and these defs and the record's lines after it, when the state is there and holds, else the whole record,
/// its state saved at its last line before the replay's end (when the record ends with a whole line); without, the
/// whole record and nothing saved (`Tickets::load`, the tests' and the harnesses' load of a copy).
pub fn start(coord: &str, log: &str, text: &str, defs: Defs, folders: &[String], states: bool) -> Started {
    let began = std::time::Instant::now();
    let key = if states { Key::of(&defs, folders) } else { Err("states not used".into()) };
    let from_saved = key.as_ref().map_err(String::clone).and_then(|key| {
        let at = AtCut::read(coord, key, defs.clone())?;
        if let Some(why) = at.cut.reaches(text) {
            return Err(why);
        }
        let read_s = began.elapsed().as_secs_f64();
        let (tail, places, unread) = split(log, read_lines(&text[at.cut.bytes..], at.cut.bytes, at.cut.file_lines));
        let (saved, n) = (at.cut.lines, tail.len());
        // the cut at the record's end (never saved: only its line is read on)
        let cut = Cut { lines: saved + n, bytes: text.len(), file_lines: at.cut.file_lines + text[at.cut.bytes..].lines().count(), prefix: String::new() };
        let at = at.on(text, &tail, &places, unread, cut)?;
        Ok((at, saved, format!("from the state saved at line {saved} (read in {read_s:.1} s) and the {n} lines after it")))
    });
    let (at, said, saved_at) = match from_saved {
        Ok((at, saved, said)) => (at, said, Some(saved)),
        Err(why) => {
            let (lines, places, unread) = split(log, read_lines(text, 0, 0));
            let n = lines.len();
            let whole = text.is_empty() || text.ends_with('\n');
            let cut = Cut::of(text, if whole { text.len() } else { 0 }, n, text.lines().count());
            let at = AtCut::replayed(defs, text, &lines, &places, unread, cut);
            let replay_s = began.elapsed().as_secs_f64();
            let mut saved_at = None;
            let saved = match &key {
                Ok(key) if whole => {
                    let t = std::time::Instant::now();
                    match at.save(coord, key) {
                        Ok(size) => {
                            saved_at = Some(n);
                            format!("; its state saved at line {n} in {:.1} s ({} MB)", t.elapsed().as_secs_f64(), size >> 20)
                        }
                        Err(e) => format!("; its state not saved: {e}"),
                    }
                }
                Ok(_) => "; its state not saved: the record ends in the middle of a line".into(),
                Err(_) if !states => String::new(),
                Err(e) => format!("; its state not saved: {e}"),
            };
            (at, format!("whole ({why}), {n} lines in {replay_s:.1} s{saved}"), saved_at)
        }
    };
    let ended = at.end(log);
    let said = format!("coordd: tickets: the record replayed {said}; {:.1} s in all", began.elapsed().as_secs_f64());
    Started { ended, saved_at, said }
}

/// `coordd save-replay COORD [SEED...]` (main.rs): the state of COORD's record saved at its last input but one, for this
/// program and the defs as read now (from BASE and PROJECT, the tickets numbered from FIRST), from the state saved
/// before for them and the lines after it, or the whole record. Only the record's file is read: the coordd that writes
/// it goes on. The last input read is left out with its outputs, which may still be being written. What it did, one line.
/// Each of SEEDS is a state saved by this program on a copy of this record, as the gate saves one (`coordd save-replay
/// COPY`; paper-research N-1446 p2658): the first that is this key's and reaches the record is gone on from, before the
/// one saved here, so that an install replays only the lines written since the gate copied the record.
pub fn save_on(coord: &str, base: &str, project: Option<&str>, first: u64, seeds: &[String]) -> Result<String, String> {
    let began = std::time::Instant::now();
    let places = crate::policy::Places { base: base.to_string(), project: project.map(str::to_string), coord: coord.to_string(), ..Default::default() };
    let (defs, ..) = super::Tickets::read_defs(&places, first);
    let key = Key::of(&defs, &super::folders(coord, base, project))?;
    let log = format!("{coord}/tickets/log.jsonl");
    let text = std::fs::read_to_string(&log).map_err(|e| format!("{log}: {e}"))?;
    // the cut before the last input among the whole lines (`last_input_at`, where a replay of a live record stops too),
    // and the lines before it read
    let Some(bytes) = last_input_at(&text) else { return Ok("nothing to save: the record holds no input before its last".into()) };
    let read = read_lines(&text[..bytes], 0, 0);
    let file_lines = read.len();
    let (lines, places, unread) = split(&log, read);
    let cut = Cut::of(&text, bytes, lines.len(), file_lines);
    let n = cut.lines;
    // the state gone on from: the first seed that is this key's and stands in the record before the cut, else the one
    // saved here; each read only when the ones before it do not serve
    let fits = |at: AtCut| {
        if let Some(why) = at.cut.reaches(&text) {
            return Err(why);
        }
        if at.cut.bytes > bytes {
            return Err(format!("the state saved stands at line {}, past the cut at line {n}", at.cut.lines));
        }
        Ok(at)
    };
    let mut why_not = Vec::new();
    let mut found = None;
    for s in seeds {
        match AtCut::read_file(s, &key, defs.clone()).and_then(fits) {
            Ok(at) => {
                found = Some((at, format!(" of the seed {s}")));
                break;
            }
            Err(why) => why_not.push(format!("the seed {s}: {why}")),
        }
    }
    let found = match found {
        Some(f) => Ok(f),
        None => AtCut::read(coord, &key, defs.clone()).and_then(fits).map(|at| (at, String::new())),
    };
    let saved = found.and_then(|(at, of)| {
        let from = at.cut.lines;
        let unread_after: Vec<String> = unread.iter().skip(at.unread.len()).cloned().collect();
        Ok((at.on(&text, &lines[from..], &places[from..], unread_after, cut.clone())?, from, of))
    });
    let (at, how) = match saved {
        Ok((at, from, of)) => (at, format!("from the state at line {from}{of} and the {} lines after it", n - from)),
        Err(why) => {
            why_not.push(why);
            (AtCut::replayed(defs, &text, &lines, &places, unread, cut), format!("from the record's first line ({})", why_not.join("; ")))
        }
    };
    let replay_s = began.elapsed().as_secs_f64();
    let t = std::time::Instant::now();
    let size = at.save(coord, &key)?;
    Ok(format!("saved at line {n} {how}: replayed in {replay_s:.1} s, written in {:.1} s ({} MB), peak {} MB", t.elapsed().as_secs_f64(), size >> 20, peak_mb()))
}

/// The lock every replay of a record on this machine takes, one at a time (paper-research N-1446 p1638): the one
/// `build-runner replay` and the scripts' `flock LOCK` take, $BUILD_RUNNER_REPLAY_LOCK, else
/// ~/agent-coord/docket-try-saved/replay.lock.
pub fn replay_lock() -> String {
    std::env::var("BUILD_RUNNER_REPLAY_LOCK").ok().filter(|p| !p.is_empty()).unwrap_or_else(|| format!("{}/agent-coord/docket-try-saved/replay.lock", std::env::var("HOME").unwrap_or_default()))
}

/// [`save_on`] (its SEEDS with it) behind the replay lock LOCK ([`replay_lock`]): waited for while another replay holds it, as long as that
/// takes, then held until the state is saved. The saver is a replay of the record like any other: three side by side
/// took the laptop to 1 GB free (p1638). What it did, one line, how long it waited first.
/// A lock that cannot be opened (a box's coordd whose box.sh could not link the host's into $COORD): saved without it,
/// and said, in the line coordd journals (p1644).
pub fn save_locked(lock: &str, coord: &str, base: &str, project: Option<&str>, first: u64, seeds: &[String]) -> Result<String, String> {
    // opened to append, as build-runner opens it; flock(2), the lock flock(1) takes, so all of them queue together
    let opened = std::path::Path::new(lock).parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::OpenOptions::new().append(true).create(true).open(lock));
    let held = match opened {
        Ok(f) => f,
        Err(e) => {
            let said = save_on(coord, base, project, first, seeds)?;
            return Ok(format!("{said}; without the replay lock, which does not open ({lock}: {e}): right only while this coord's record stays small, as a box's do"));
        }
    };
    let t = std::time::Instant::now();
    // SAFETY: a lock on a file this process holds open
    if unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&held), libc::LOCK_EX) } != 0 {
        return Err(format!("{lock}: {}", std::io::Error::last_os_error()));
    }
    let waited = t.elapsed().as_secs_f64();
    let said = save_on(coord, base, project, first, seeds)?;
    drop(held);
    Ok(format!("{said}; after {waited:.0} s waiting for the replay lock"))
}

#[cfg(test)]
#[path = "replay_state_tests.rs"]
mod tests;

/// This process's peak resident memory (VmHWM), in MB.
pub fn peak_mb() -> u64 {
    std::fs::read_to_string("/proc/self/status").ok().and_then(|s| s.lines().find_map(|l| l.strip_prefix("VmHWM:").and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok()))).map_or(0, |kb| kb >> 10)
}
