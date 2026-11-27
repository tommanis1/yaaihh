//! Storage: on the window's Home, every part of the store (the build machine's compiled crates, this machine's
//! binaries) with how much it holds against its cap and what it did today, and every project's space outside it; on a
//! project's Overview, that project's folders. A press on a part opens its entries, newest use first, 200 at a time.
//! coordd hands over what `build-runner store` answers, every field optional, read leniently; a part names its machine
//! as Devices does. Asked as `Request::Store { row, from }` while either page shows: the rows every [`EVERY`]
//! ([`ask`]), a page when a press wants one ([`ask_page`]).

use crate::App;
use crate::devices::{Device, items, lenient};
use rune::{El, col, row, text};
use rune_kit::Kit;
use serde::Deserialize;

/// Entries a press asks for at a time (p2815).
pub const PAGE: u64 = 200;

/// How often the rows are asked again while Home shows: the runner answers the build machine's row from its last
/// reading at once and reads again behind it, so a slow pace is enough.
const EVERY: std::time::Duration = std::time::Duration::from_secs(30);

/// The store's answer (p2815): its rows. `states` comes later with its own piece; a row the window does not know is
/// shown by its own words all the same. `outside` is the walk's (build-runner `walk`): what lies outside the store in
/// the agents' folders, the projects' own folders and their repositories' worktrees, each project's part and the whole
/// (the owner's walk, items 20 and 23: "what takes space is shown on a project's page, and once for the whole").
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Answer {
    #[serde(default, deserialize_with = "items")]
    pub rows: Vec<Row>,
}

/// A part of the store: `crates` (the tower's compiled crates) or `binaries` (this machine's built binaries).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Row {
    #[serde(default, deserialize_with = "lenient")]
    pub id: Option<String>,
    /// In his words: "Compiled crates", "Binaries".
    #[serde(default, deserialize_with = "lenient")]
    pub what: Option<String>,
    /// The machine it is on, as Devices names it ([`machine`]).
    #[serde(default, deserialize_with = "lenient")]
    pub device: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub held_bytes: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    pub cap_bytes: Option<u64>,
    /// How many it holds, and of what ("entries", "blobs").
    #[serde(default, deserialize_with = "lenient")]
    pub count: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    pub count_of: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub today: Option<Today>,
    /// The binaries row's most-linked first.
    #[serde(default, deserialize_with = "items")]
    pub holds: Vec<Hold>,
    /// Why the build machine's row is not read now (the runner then gives its last reading, if any).
    #[serde(default, deserialize_with = "lenient")]
    pub unread: Option<String>,
    /// When the row was read on its machine.
    #[serde(default, deserialize_with = "lenient")]
    pub as_of: Option<String>,
    /// The walk's row: when it walked, what it found outside the store and how big the folders it found it in are.
    #[serde(default, deserialize_with = "lenient")]
    pub at: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub total_bytes: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    pub folder_bytes: Option<u64>,
    /// Each project's part, the largest first ("" the folders no project names).
    #[serde(default, deserialize_with = "items")]
    pub projects: Vec<Part>,
    /// The folders it found anything in, the most outside the store first.
    #[serde(default, deserialize_with = "items")]
    pub folders: Vec<Folder>,
}

/// A project's part of the walk: its folders' bytes outside the store, their size and how many.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Part {
    #[serde(default, deserialize_with = "lenient")]
    pub id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub bytes: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    pub folder_bytes: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    pub folders: Option<u64>,
}

/// A folder the walk found something in: an agent's scratch folder, a worktree or a project's own folder (`kind`), whose
/// and which project's, its bytes outside the store and its whole size (`over` 1 GB), what it holds, and where its
/// findings start in a press's pages (`from`, `count`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Folder {
    #[serde(default, deserialize_with = "lenient")]
    pub path: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub kind: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub owner: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub project: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub bytes: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    pub folder_bytes: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    pub over: Option<bool>,
    #[serde(default, deserialize_with = "lenient")]
    pub targets: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    pub copies: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    pub binaries: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    pub from: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    pub count: Option<u64>,
}

/// Today's counts (p2815): the crates row's jobs, compiles and those of them taken from the store; the binaries row's
/// files handed back and how many of those had to be fetched.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Today {
    #[serde(default, deserialize_with = "lenient")]
    pub jobs: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    pub compiles: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    pub from_store: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    pub handed: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    pub fetched: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Hold {
    #[serde(default, deserialize_with = "lenient")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub hash8: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub links: Option<u64>,
}

/// A page of a row's entries (p2815): newest use first, and how many there are in all.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Page {
    #[serde(default, deserialize_with = "items")]
    pub entries: Vec<Entry>,
    #[serde(default, deserialize_with = "lenient")]
    pub total: Option<u64>,
}

/// An entry: a compiled crate (`made_from`, `by`) or a binary (`names`, `hash8`, `made_by`, `links`); both with when it
/// was last used and its size.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Entry {
    #[serde(default, deserialize_with = "lenient")]
    pub made_from: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub by: Option<By>,
    #[serde(default, deserialize_with = "lenient")]
    pub made_at: Option<String>,
    #[serde(default, deserialize_with = "items")]
    pub names: Vec<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub hash8: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub made_by: Option<By>,
    #[serde(default, deserialize_with = "lenient")]
    pub links: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    pub used_at: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub bytes: Option<u64>,
    /// A finding of the walk: what it is (`target`, `copy`, `binary`, `folder`), its path, its folder, whose it is,
    /// the record it copies, its files and since when the walk has found it.
    #[serde(default, deserialize_with = "lenient")]
    pub what: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub path: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub folder: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub owner: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub of: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub files: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    pub since: Option<String>,
}

/// Who made an entry: the agent (a crate's `who`) or the job (a binary's `job`), on which branch and commit.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct By {
    #[serde(default, deserialize_with = "lenient")]
    pub who: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub job: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub branch: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub commit: Option<String>,
}

/// The row a press opened and the entries it holds of it so far.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Opened {
    pub row: String,
    pub entries: Vec<Entry>,
    pub total: Option<u64>,
    /// A page is asked and has not come.
    pub asking: bool,
    /// Why the last page did not come.
    pub why: Option<String>,
}

/// The section's state (`App::store`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Store {
    /// The rows, once coordd answered; its why when it could not.
    pub shown: Option<Result<Answer, String>>,
    pub open: Option<Opened>,
    /// When the rows were last asked.
    pub asked: Option<std::time::Instant>,
    /// Whether the walk's row shows this project's folders (a press on its head), or only the whole and the parts.
    pub folders_open: bool,
    /// The walk's folder whose findings a press opened (its path).
    pub folder: Option<String>,
    /// The finding whose Delete was pressed once: the second press deletes it.
    pub confirming: Option<String>,
    /// The finding being deleted now, and what the last delete said (its path, coordd's words).
    pub freeing: Option<String>,
    pub freed: Option<(String, Result<String, String>)>,
}

/// Whether project PI's coordd takes `Request::Store` and its pipe is up, or it is the v2 project with its reader.
pub(crate) fn served(s: &App, pi: usize) -> bool {
    crate::machine::reads(s, pi) || s.projects.get(pi).is_some_and(|p| p.protocol >= coord_types::STORE_PROTOCOL && p.link.is_some())
}

/// While Home shows (devices.rs `poll`): ask project PI's coordd for the store's rows every [`EVERY`].
pub fn ask(s: &mut App, pi: usize) {
    if !served(s, pi) || s.store.asked.is_some_and(|t| t.elapsed() < EVERY) {
        return;
    }
    s.store.asked = Some(std::time::Instant::now());
    if crate::machine::reads(s, pi) {
        crate::machine::ask(s, crate::machine::Ask::Store, 0);
    } else {
        s.request_in(pi, coord_types::Request::Store { row: None, from: None }, crate::Pending::Store);
    }
}

/// While Home shows: the page the open row [`wanted`], asked once.
pub fn ask_page(s: &mut App, pi: usize) {
    if !served(s, pi) {
        return;
    }
    let Some((row, from)) = wanted(&s.store) else { return };
    if let Some(o) = s.store.open.as_mut() {
        o.asking = true;
    }
    if crate::machine::reads(s, pi) {
        crate::machine::ask(s, crate::machine::Ask::Page(row), from);
    } else {
        s.request_in(pi, coord_types::Request::Store { row: Some(row.clone()), from: Some(from) }, crate::Pending::StorePage { row });
    }
}

/// coordd's answer R to the store's rows.
pub fn came(s: &mut App, r: Result<String, String>) {
    s.store.shown = Some(r.and_then(|o| serde_json::from_str::<Answer>(&o).map_err(|e| format!("the store's answer could not be read: {e}"))));
}

/// coordd's answer R to a page of ROW's entries: added after those held, unless the row was closed meanwhile.
pub fn paged(s: &mut App, row: &str, r: Result<String, String>) {
    let Some(o) = s.store.open.as_mut().filter(|o| o.row == row) else { return };
    o.asking = false;
    match r.and_then(|t| serde_json::from_str::<Page>(&t).map_err(|e| format!("the page could not be read: {e}"))) {
        // the walk's: the open folder's findings alone (a page runs on into the next folders')
        Ok(p) if row == "outside" => {
            let folder = s.store.folder.clone();
            let o = s.store.open.as_mut().expect("held above");
            o.entries.extend(p.entries.into_iter().filter(|e| e.folder.is_some() && e.folder == folder));
            o.why = None;
        }
        Ok(p) => {
            o.entries.extend(p.entries);
            o.total = p.total.or(o.total);
            o.why = None;
        }
        Err(e) => o.why = Some(crate::accounts::plain(&e)),
    }
}

/// The page the window asks for next: the open row from the entries it holds, when none is asked and more remain
/// ([`ask_page`] sends it).
pub fn wanted(st: &Store) -> Option<(String, u64)> {
    let o = st.open.as_ref().filter(|o| !o.asking)?;
    let held = o.entries.len() as u64;
    // the walk's: the open folder's findings alone, from where they start, as many as it has
    if o.row == "outside" {
        let f = st.shown.as_ref()?.as_ref().ok()?.rows.iter().find(|r| r.id.as_deref() == Some("outside"))?.folders.iter().find(|f| f.path == st.folder)?;
        let (from, n) = (f.from.unwrap_or(0), f.count.unwrap_or(0));
        return (held < n && (held == 0 || held % PAGE == 0)).then(|| ("outside".into(), from + held));
    }
    (o.total.is_none_or(|t| held < t) && (held == 0 || held % PAGE == 0)).then(|| (o.row.clone(), held))
}

/// His second press on a finding's Delete (the owner's walk, item 20: "a way to deal with it"): project PI's coordd asked
/// to delete PATH ([`coord_types::Request::StoreFree`]); one at a time.
pub fn free(s: &mut App, pi: usize, path: &str) {
    if s.store.freeing.is_some() || !s.projects.get(pi).is_some_and(|p| p.protocol >= coord_types::STORE_FREE_PROTOCOL) {
        return;
    }
    s.store.confirming = None;
    s.store.freeing = Some(path.to_string());
    s.request_in(pi, coord_types::Request::StoreFree { path: path.to_string() }, crate::Pending::StoreFree { path: path.to_string() });
}

/// coordd's answer R to the delete of PATH: said under its folder; the rows and the open folder's findings asked again.
pub fn freed(s: &mut App, path: &str, r: Result<String, String>) {
    s.store.freeing = None;
    s.store.freed = Some((path.to_string(), r.map_err(|e| crate::accounts::plain(&e))));
    s.store.asked = None;
    if let Some(o) = s.store.open.as_mut() {
        o.entries.clear();
        o.total = None;
    }
}

/// Whether THIS project's Home shows folder F as its own: the walk names it so, or its owner is one of its agents.
fn of_project(f: &Folder, id: &str, agents: &[&str]) -> bool {
    f.project.as_deref() == Some(id) || f.owner.as_deref().is_some_and(|o| !o.is_empty() && agents.contains(&o))
}

/// The walk's row's support line: "58.2 GB outside the store in 191 folders · walked 21:41".
pub fn outside_words(r: &Row) -> String {
    let size = crate::files::size;
    let mut parts = vec![format!("{} outside the store in {} {}", size(r.total_bytes.unwrap_or(0)), count(r.folders.len() as u64), if r.folders.len() == 1 { "folder" } else { "folders" })];
    if let Some(at) = &r.at {
        parts.push(format!("walked {}", crate::when::at(at)));
    }
    parts.join(" · ")
}

/// A folder's words: its name with whose it is, and "12.3 GB outside the store of 14.0 GB · 2 build folders, 1 record
/// copy" (its kind said when it is a project's own folder).
pub fn folder_words(f: &Folder) -> (String, String) {
    let size = crate::files::size;
    let path = f.path.clone().unwrap_or_default();
    let name = path.rsplit('/').next().unwrap_or(&path).to_string();
    let whose = match (f.kind.as_deref(), f.owner.as_deref().filter(|o| !o.is_empty()), f.project.as_deref().filter(|p| !p.is_empty())) {
        (Some("coord"), _, Some(p)) => format!("{p}'s own folder"),
        (_, Some(o), _) => o.to_string(),
        (_, None, Some(p)) => p.to_string(),
        _ => "nobody known".into(),
    };
    let n = |c: Option<u64>, one: &str, many: &str| c.filter(|c| *c > 0).map(|c| format!("{c} {}", if c == 1 { one } else { many }));
    let holds: Vec<String> = [n(f.targets, "build folder", "build folders"), n(f.copies, "record copy", "record copies"), n(f.binaries, "binary", "binaries")].into_iter().flatten().collect();
    let mut sup = vec![match f.folder_bytes {
        Some(w) if w > f.bytes.unwrap_or(0) => format!("{} outside the store of {}", size(f.bytes.unwrap_or(0)), size(w)),
        _ => size(f.bytes.unwrap_or(0)),
    }];
    if !holds.is_empty() {
        sup.push(holds.join(", "));
    }
    (format!("{name} · {whose}"), sup.join(" · "))
}

/// A finding's words: "build folder target" with "3.1 GB · 12,402 files · found 20:41", what a record copy
/// copies, a binary's release.
pub fn finding_words(e: &Entry) -> (String, String) {
    let path = e.path.clone().unwrap_or_default();
    let rel = e.folder.as_deref().and_then(|f| path.strip_prefix(f)).map(|r| r.trim_start_matches('/').to_string()).filter(|r| !r.is_empty()).unwrap_or_else(|| path.clone());
    let what = match e.what.as_deref() {
        Some("target") => "build folder",
        Some("copy") => "record copy",
        Some("binary") => "binary",
        Some("folder") => "the folder, over 1 GB",
        _ => "found",
    };
    let mut parts = Vec::new();
    if let Some(b) = e.bytes {
        parts.push(crate::files::size(b));
    }
    if let Some(n) = e.files {
        parts.push(format!("{} files", count(n)));
    }
    if let Some(of) = &e.of {
        parts.push(format!("of {of}'s record"));
    }
    if let Some(t) = &e.since {
        parts.push(format!("found {}", crate::when::at(t)));
    }
    (format!("{what} {rel}"), parts.join(" · "))
}

/// A press on a walk's folder: its findings open under it (its page from where they start), or close.
pub(crate) fn press_folder(st: &mut Store, path: &str) {
    st.confirming = None;
    if st.folder.as_deref() == Some(path) {
        st.folder = None;
        st.open = None;
    } else {
        st.folder = Some(path.to_string());
        st.open = Some(Opened { row: "outside".into(), ..Opened::default() });
    }
}

/// A press on ROW: open it from its first page, or close it when it is the one open.
pub fn press(st: &mut Store, row: &str) {
    if st.open.as_ref().is_some_and(|o| o.row == row) {
        st.open = None;
    } else {
        st.open = Some(Opened { row: row.to_string(), ..Opened::default() });
    }
}

/// The machine DEVICE names, as Devices names it (p2904, a2h-speed's yes p2927): the string itself, which is the name
/// Devices uses (the runner's own name for the tower, `yaaihh-build-1`, as a job's `machine` gives it); the engine's own
/// machine, as Devices names it, when it names none. There are no ids to map.
pub fn machine(device: Option<&str>, devices: &[Device]) -> Option<String> {
    match device {
        None | Some("") => devices.iter().find(|d| d.here == Some(true)).and_then(|d| d.name.clone()),
        Some(n) => Some(n.to_string()),
    }
}

/// A count as he reads it, thousands apart: "36,214".
pub(crate) fn count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// R's support line: "60.2 GB of 60.0 GB · 36,214 entries · today 883 jobs, 98% from the store" (crates), "6.2 GB of
/// 20.0 GB · coordd, yaaihh.exe and 14 more · today 31 handed back, 4 fetched" (binaries).
pub fn support(r: &Row) -> String {
    let (before, held, after) = support_parts(r);
    [before, held, after].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" · ")
}

/// R's support line in three parts, so the held part alone can take the danger colour ([`over`]): what comes before it
/// (not read now), held against cap, and the rest.
fn support_parts(r: &Row) -> (String, String, String) {
    let size = crate::files::size;
    let mut parts = Vec::new();
    // not read now: why, and the reading it shows instead, with its time
    if let Some(why) = &r.unread {
        parts.push(match &r.as_of {
            Some(at) => format!("not read now ({why}); as of {}", crate::when::at(at)),
            None => format!("not read now ({why})"),
        });
    }
    let before = parts.join(" · ");
    parts.clear();
    let held = match (r.held_bytes, r.cap_bytes) {
        (Some(h), Some(c)) => format!("{} of {}", size(h), size(c)),
        (Some(h), None) => size(h),
        _ => String::new(),
    };
    if !r.holds.is_empty() {
        let names: Vec<String> = r.holds.iter().filter_map(|h| h.name.clone()).take(2).collect();
        let more = r.holds.len().saturating_sub(names.len());
        parts.push(match more {
            0 => names.join(", "),
            m => format!("{} and {m} more", names.join(", ")),
        });
    } else if let Some(n) = r.count {
        parts.push(format!("{} {}", count(n), r.count_of.clone().unwrap_or_else(|| "held".into())));
    }
    if let Some(t) = &r.today {
        let mut said = Vec::new();
        if let Some(j) = t.jobs {
            said.push(format!("{} {}", count(j), if j == 1 { "job" } else { "jobs" }));
        }
        if let (Some(f), Some(c)) = (t.from_store, t.compiles.filter(|c| *c > 0)) {
            said.push(format!("{}% from the store", (f as f64 * 100.0 / c as f64).round() as u64));
        }
        if let Some(h) = t.handed {
            said.push(format!("{} handed back", count(h)));
        }
        if let Some(f) = t.fetched {
            said.push(format!("{} fetched", count(f)));
        }
        if !said.is_empty() {
            parts.push(format!("today {}", said.join(", ")));
        }
    }
    (before, held, parts.join(" · "))
}

/// Whether R holds more than its cap: its held part and its bar in the danger colour, as a device card's full disk
/// (paper-research's read on H-1702).
pub fn over(r: &Row) -> bool {
    matches!((r.held_bytes, r.cap_bytes), (Some(h), Some(c)) if c > 0 && h > c)
}

/// How full R is, held against its cap.
fn fill(r: &Row) -> Option<f32> {
    let (h, c) = (r.held_bytes?, r.cap_bytes.filter(|c| *c > 0)?);
    Some((h as f64 / c as f64).clamp(0.0, 1.0) as f32)
}

/// An entry's label and its support line: a crate as what it was made from, "by docket-core on edge8-store 4b153958 ·
/// used 18:40 · 2.1 MB"; a binary as its names, "00aba796 · 41 links · job j12 on main · used 18:40 · 41.0 MB".
pub fn entry_words(e: &Entry) -> (String, String) {
    let label = e.made_from.clone().or_else(|| (!e.names.is_empty()).then(|| e.names.join(", "))).unwrap_or_else(|| "an entry".into());
    let mut parts = Vec::new();
    if let Some(h) = &e.hash8 {
        parts.push(h.clone());
    }
    if let Some(l) = e.links {
        parts.push(format!("{} {}", count(l), if l == 1 { "link" } else { "links" }));
    }
    if let Some(b) = e.by.as_ref().or(e.made_by.as_ref()) {
        let who = b.who.clone().or_else(|| b.job.as_ref().map(|j| format!("job {j}")));
        let at: Vec<String> = [b.branch.clone(), b.commit.as_ref().map(|c| c.chars().take(8).collect())].into_iter().flatten().collect();
        match (who, at.is_empty()) {
            (Some(w), false) => parts.push(format!("by {w} on {}", at.join(" "))),
            (Some(w), true) => parts.push(format!("by {w}")),
            (None, false) => parts.push(format!("on {}", at.join(" "))),
            (None, true) => {}
        }
    }
    if let Some(t) = &e.used_at {
        parts.push(format!("used {}", crate::when::at(t)));
    }
    if let Some(b) = e.bytes {
        parts.push(crate::files::size(b));
    }
    (label, parts.join(" · "))
}

/// The Store section for project P's Home, under HEADING's look; none until coordd answered.
/// The storage section: with TOTAL every part of the store and every project's space (the window's Home), else where
/// project P's own space goes (its Overview).
pub fn section(s: &App, p: &crate::Proj, k: &Kit, heading: &dyn Fn(String) -> El, total: bool) -> Option<El> {
    let c = col().key("home-store").w_full().gap(2.0).child(heading(if total { "Total storage" } else { "Storage" }.into()));
    let a = match s.store.shown.as_ref()? {
        Err(why) => return Some(c.child(text(why.clone()).font(k.ty.body).color(k.c.text_muted).truncate())),
        Ok(a) => a,
    };
    if !total {
        let r = a.rows.iter().find(|r| r.id.as_deref() == Some("outside"))?;
        return Some(c.child(project_row(s, p, r, k)));
    }
    let devices = crate::devices::listed(&s.devices);
    let mut c = c;
    for (i, r) in a.rows.iter().enumerate() {
        let id = r.id.clone().unwrap_or_else(|| i.to_string());
        if id == "outside" {
            c = c.child(outside_row(p, r, k));
            continue;
        }
        let open = s.store.open.as_ref().filter(|o| o.row == id);
        c = c.child(store_row(r, &id, machine(r.device.as_deref(), &devices), open, k));
    }
    Some(c)
}

/// The walk's row on the window's Home: the whole and how fresh, then every project's part against the largest.
fn outside_row(p: &crate::Proj, r: &Row, k: &Kit) -> El {
    let quiet = |t: String| text(t).font(k.ty.small).color(k.c.text_faint).truncate().shrink();
    let what = r.what.clone().unwrap_or_else(|| "Outside the store".into());
    let head = col().w_full().gap(3.0).py(4.0).child(text(what).font(k.ty.body).color(k.c.text_base).truncate()).child(text(outside_words(r)).font(k.ty.small).color(k.c.text_muted).truncate());
    let mut c = col().w_full().key("store-row-outside").gap(2.0).child(head);
    let most = r.projects.iter().filter_map(|x| x.bytes).max().unwrap_or(0).max(1);
    for x in r.projects.iter().filter(|x| x.bytes.unwrap_or(0) > 0) {
        let id = x.id.clone().unwrap_or_default();
        let this = id == p.id;
        let name = if id.is_empty() { "no project".to_string() } else { id.clone() };
        let n = x.folders.unwrap_or(0);
        let line = row().w_full().gap(8.0).center_y().child(text(name.clone()).font(if this { k.ty.small_medium } else { k.ty.small }).color(k.c.text_base).truncate().shrink())
            .child(quiet(format!("{} in {} {}", crate::files::size(x.bytes.unwrap_or(0)), n, if n == 1 { "folder" } else { "folders" })));
        let f = x.bytes.unwrap_or(0) as f32 / most as f32;
        c = c.child(col().w_full().gap(2.0).pl(12.0).py(2.0).key(format!("store-part-{id}")).child(line).child(crate::devices::bar(f, false, &name, k)));
    }
    c
}

/// Project P's part of the walk's row on its Overview: its total, folding open its folders, the most outside the store
/// first, each with whose it is, in the danger colour over 1 GB. A press on a folder opens its findings; a build folder
/// or a record copy has Delete, which asks once more before coordd deletes it.
fn project_row(s: &App, p: &crate::Proj, r: &Row, k: &Kit) -> El {
    let quiet = |t: String| text(t).font(k.ty.small).color(k.c.text_faint).truncate().shrink();
    let agents: Vec<&str> = p.live.agents.iter().map(|a| a.name.as_str()).collect();
    let mine: Vec<&Folder> = r.folders.iter().filter(|f| of_project(f, &p.id, &agents)).collect();
    let bytes: u64 = mine.iter().map(|f| f.bytes.unwrap_or(0)).sum();
    let open = s.store.folders_open;
    let n = mine.len();
    let mut sup = format!("{} outside the store in {n} {}", crate::files::size(bytes), if n == 1 { "folder" } else { "folders" });
    if let Some(at) = &r.at {
        sup.push_str(&format!(" · walked {}", crate::when::at(at)));
    }
    let chevron = text(if n == 0 { " " } else { crate::home::FOLD[open as usize] }).font(k.ty.body).color(k.c.text_muted).w(14.0);
    let head = row().w_full().gap(6.0).center_y().py(4.0).key("store-outside").child(chevron).child(
        col().grow(1.0).shrink().min_w(0.0).gap(3.0).child(text("This project's folders").font(k.ty.body).color(k.c.text_base).truncate()).child(text(sup).font(k.ty.small).color(k.c.text_muted).truncate()),
    );
    let mut c = col().w_full().key("store-row-outside").gap(2.0).child(k.interactive(head).on_press(|s: &mut App| {
        s.store.folders_open = !s.store.folders_open;
        if !s.store.folders_open {
            s.store.folder = None;
            s.store.confirming = None;
            if s.store.open.as_ref().is_some_and(|o| o.row == "outside") {
                s.store.open = None;
            }
        }
    }));
    // this project's folders
    let pi = s.projects.iter().position(|q| q.id == p.id).unwrap_or(s.cur);
    let can_free = p.protocol >= coord_types::STORE_FREE_PROTOCOL;
    for f in mine.into_iter().filter(|_| open) {
        let path = f.path.clone().unwrap_or_default();
        let (label, sup) = folder_words(f);
        let over = f.over == Some(true);
        let head = col().w_full().pl(12.0).py(3.0).key(format!("store-folder-{path}"))
            .child(text(label).font(k.ty.small_medium).color(k.c.text_base).truncate())
            .child(text(sup).font(k.ty.small).color(if over { k.c.danger.fg } else { k.c.text_muted }).truncate());
        let key = path.clone();
        c = c.child(k.interactive(head).on_press(move |s: &mut App| press_folder(&mut s.store, &key)));
        if let Some((at, said)) = s.store.freed.as_ref().filter(|(at, _)| at.starts_with(&format!("{path}/"))) {
            let (t, bad) = match said {
                Ok(_) => (format!("Deleted {}", at.strip_prefix(&format!("{path}/")).unwrap_or(at)), false),
                Err(e) => (e.clone(), true),
            };
            c = c.child(text(t).font(k.ty.small).color(if bad { k.c.danger.fg } else { k.c.text_muted }).pl(24.0).key(format!("store-freed-{path}")));
        }
        if s.store.folder.as_deref() != Some(path.as_str()) {
            continue;
        }
        let o = s.store.open.as_ref().filter(|o| o.row == "outside");
        for e in o.map(|o| o.entries.as_slice()).unwrap_or_default() {
            let (l, sup) = finding_words(e);
            let at = e.path.clone().unwrap_or_default();
            let mut line = row().w_full().gap(8.0).center_y().pl(24.0).py(2.0).child(col().grow(1.0).min_w(0.0).child(text(l).font(k.ty.small).color(k.c.text_base).truncate()).child(quiet(sup)));
            if can_free && matches!(e.what.as_deref(), Some("target" | "copy")) {
                let busy = s.store.freeing.is_some();
                line = if s.store.confirming.as_deref() == Some(at.as_str()) {
                    let size = e.bytes.map(crate::files::size).unwrap_or_default();
                    let (a1, a2) = (at.clone(), at.clone());
                    line.child(rune_kit::Button::new(format!("Delete {size} for good")).variant(rune_kit::ButtonVariant::Danger).disabled(busy)
                        .on_press(move |s: &mut App| free(s, pi, &a1)).build(k).key(format!("store-free-go-{a2}")))
                        .child(rune_kit::Button::new("Keep").variant(rune_kit::ButtonVariant::Ghost).on_press(|s: &mut App| s.store.confirming = None).build(k).key(format!("store-free-keep-{at}")))
                } else {
                    let (a1, a2) = (at.clone(), at.clone());
                    line.child(rune_kit::Button::new(if s.store.freeing.as_deref() == Some(at.as_str()) { "Deleting…" } else { "Delete" }).variant(rune_kit::ButtonVariant::Ghost).disabled(busy)
                        .on_press(move |s: &mut App| s.store.confirming = Some(a1.clone())).build(k).key(format!("store-free-{a2}")))
                };
            }
            c = c.child(line);
        }
        let n = f.count.unwrap_or(0) as usize;
        let foot = match o {
            Some(o) if o.why.is_some() => o.why.clone(),
            Some(o) if o.entries.len() < n => Some("asking…".into()),
            _ => None,
        };
        c = c.children(foot.map(|t| quiet(t).pl(24.0).py(2.0)));
    }
    c
}

/// Row R: what it is with its machine quiet beside it, its support line under it, how full it is as the device cards'
/// bar under that (paper-research's read on H-1702); a press opens its entries under it, a second closes them.
fn store_row(r: &Row, id: &str, machine: Option<String>, open: Option<&Opened>, k: &Kit) -> El {
    let quiet = |t: String| text(t).font(k.ty.small).color(k.c.text_faint).truncate().shrink();
    let mut label = row().gap(6.0).center_y().shrink().min_w(0.0).child(text(r.what.clone().unwrap_or_else(|| id.to_string())).font(k.ty.body).color(k.c.text_base).truncate());
    if let Some(m) = machine {
        label = label.child(quiet(m));
    }
    // its support line with the held part in the danger colour when over its cap, and the device cards' bar under it
    let (before, held, after) = support_parts(r);
    let muted = |t: String| text(t).font(k.ty.small).color(k.c.text_muted);
    let mut line = row().w_full().center_y().min_w(0.0);
    let dot = |a: &str, b: &str| if a.is_empty() || b.is_empty() { "" } else { " · " };
    if !before.is_empty() {
        line = line.child(muted(format!("{before}{}", dot(&before, &held))));
    }
    if !held.is_empty() {
        line = line.child(text(held.clone()).font(k.ty.small).color(if over(r) { k.c.danger.fg } else { k.c.text_muted }).key(format!("store-held-{id}")));
    }
    if !after.is_empty() {
        line = line.child(muted(format!("{}{after}", if before.is_empty() && held.is_empty() { "" } else { " · " })).truncate().shrink());
    }
    let what = r.what.clone().unwrap_or_else(|| id.to_string());
    let left = col().w_full().gap(3.0).child(label).child(line).children(fill(r).map(|f| crate::devices::bar(f, over(r), &what, k).key(format!("store-bar-{id}"))));
    let head = row().w_full().gap(12.0).center_y().py(4.0).child(left).key(format!("store-{id}"));
    let key = id.to_string();
    let mut c = col().w_full().key(format!("store-row-{id}")).child(k.interactive(head).on_press(move |s: &mut App| press(&mut s.store, &key)));
    let Some(o) = open else { return c };
    for e in &o.entries {
        let (l, sup) = entry_words(e);
        c = c.child(col().pl(12.0).py(2.0).child(text(l).font(k.ty.small_medium).color(k.c.text_base).truncate()).child(quiet(sup)));
    }
    let foot = match (&o.why, o.asking, o.total) {
        (Some(w), _, _) => Some(w.clone()),
        (None, true, _) => Some("asking…".into()),
        (None, false, Some(t)) if (o.entries.len() as u64) < t => Some(format!("{} of {}", count(o.entries.len() as u64), count(t))),
        (None, false, Some(0)) => Some("none held".into()),
        _ => None,
    };
    c.children(foot.map(|f| quiet(f).pl(12.0).py(2.0)))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    /// The store's answer in a2h-speed's shape (p2815), with the figures of his example (N-1104 p1069).
    pub(crate) fn answer() -> String {
        json!({"rows": [
            {"id": "crates", "what": "Compiled crates", "device": "yaaihh-build-1", "held_bytes": 64_639_041_536u64, "cap_bytes": 64_424_509_440u64,
             "count": 36214, "count_of": "entries", "today": {"jobs": 883, "compiles": 41000, "from_store": 40180}},
            {"id": "binaries", "what": "Binaries", "held_bytes": 6_657_199_309u64, "cap_bytes": 21_474_836_480u64, "count": 412,
             "count_of": "blobs", "today": {"handed": 31, "fetched": 4},
             "holds": [{"name": "coordd", "hash8": "00aba796", "links": 41}, {"name": "yaaihh.exe", "hash8": "5d1e0c22", "links": 12},
                       {"name": "coordd test binary", "hash8": "77f0aa10", "links": 3}]}
        ]})
        .to_string()
    }

    fn devices() -> Vec<Device> {
        let a: crate::devices::Answer = serde_json::from_value(json!({"devices": [
            {"name": "WSL", "here": true, "roles": ["server"]}, {"name": "yaaihh-build-1", "roles": ["jobs"]}]}))
        .unwrap();
        a.devices
    }

    /// The walk's row as build-runner's `outside_row` gives it (made-up folders; figures shaped as the walk of 25 Nov
    /// 20:41Z, which found 27.4 GB outside the store in 120 folders): the whole, each project's part, its folders.
    pub(crate) fn outside() -> serde_json::Value {
        json!({"id": "outside", "what": "Outside the store", "at": "2026-11-25T20:41:57Z", "total_bytes": 9_663_676_416u64, "folder_bytes": 12_884_901_888u64,
               "projects": [{"id": "walk", "bytes": 7_516_192_768u64, "folder_bytes": 9_663_676_416u64, "folders": 2},
                            {"id": "other", "bytes": 2_147_483_648u64, "folder_bytes": 3_221_225_472u64, "folders": 1}],
               "folders": [
                   {"path": "/w/soft-swarm-try-1-fix", "kind": "worktree", "owner": "try-1", "project": "", "bytes": 5_368_709_120u64, "folder_bytes": 5_637_144_576u64,
                    "over": true, "targets": 1, "copies": 0, "binaries": 2, "from": 0, "count": 3},
                   {"path": "/w/agent-coord/walk", "kind": "coord", "owner": "", "project": "walk", "bytes": 2_147_483_648u64, "folder_bytes": 4_026_531_840u64,
                    "over": true, "targets": 0, "copies": 1, "binaries": 0, "from": 3, "count": 1},
                   {"path": "/w/agent-coord/docket-try-saved/someone", "kind": "scratch", "owner": "someone", "project": "other", "bytes": 2_147_483_648u64,
                    "folder_bytes": 3_221_225_472u64, "over": true, "targets": 0, "copies": 1, "binaries": 0, "from": 4, "count": 1}]})
    }

    /// The walk's row (the owner's walk, items 20 and 23): the whole and this project's part on its support line; a
    /// folder by its name and whose, what it holds; a finding by what it is under its folder; a folder this project's
    /// when the walk names it so or its owner is one of its agents.
    #[test]
    fn the_walks_row_says_the_whole_this_projects_part_and_each_folder_and_finding() {
        let r: Row = serde_json::from_value(outside()).unwrap();
        let at = crate::when::at("2026-11-25T20:41:57Z");
        assert_eq!(outside_words(&r), format!("9.0 GB outside the store in 3 folders · walked {at}"));
        assert_eq!(folder_words(&r.folders[0]), ("soft-swarm-try-1-fix · try-1".to_string(), "5.0 GB outside the store of 5.2 GB · 1 build folder, 2 binaries".to_string()));
        assert_eq!(folder_words(&r.folders[1]).0, "walk · walk's own folder");
        let agents = ["try-1", "try-2"];
        let mine: Vec<bool> = r.folders.iter().map(|f| of_project(f, "walk", &agents)).collect();
        assert_eq!(mine, vec![true, true, false], "try-1's by its owner, the project's own folder by the walk's word, another's not");
        let e = Entry { what: Some("target".into()), path: Some("/w/soft-swarm-try-1-fix/yaaihh/target".into()), folder: Some("/w/soft-swarm-try-1-fix".into()), bytes: Some(3_328_599_654), files: Some(12_402), since: Some("2026-11-25T20:41:57Z".into()), ..Entry::default() };
        assert_eq!(finding_words(&e), ("build folder yaaihh/target".to_string(), format!("3.1 GB · 12,402 files · found {at}")));
        let c = Entry { what: Some("copy".into()), path: Some("/w/agent-coord/walk/runs/log.jsonl".into()), folder: Some("/w/agent-coord/walk".into()), of: Some("research".into()), ..Entry::default() };
        assert_eq!(finding_words(&c).0, "record copy runs/log.jsonl");
        assert!(finding_words(&c).1.contains("of research's record"));
    }

    /// A press on a walk's folder asks its findings alone, from where they start, and a page that runs on into the next
    /// folder's keeps only its own; a second press closes it.
    #[test]
    fn a_press_on_a_walks_folder_asks_its_own_findings() {
        let (_held, mut s) = crate::view_cost::test_app();
        came(&mut s, Ok(json!({"rows": [outside()]}).to_string()));
        press_folder(&mut s.store, "/w/agent-coord/walk");
        assert_eq!(wanted(&s.store), Some(("outside".into(), 3)), "its findings start at 3");
        s.store.open.as_mut().unwrap().asking = true;
        let page = json!({"entries": [{"what": "copy", "path": "/w/agent-coord/walk/x.jsonl", "folder": "/w/agent-coord/walk", "owner": ""},
                                      {"what": "copy", "path": "/w/agent-coord/docket-try-saved/someone/y.jsonl", "folder": "/w/agent-coord/docket-try-saved/someone"}], "total": 5}).to_string();
        paged(&mut s, "outside", Ok(page));
        assert_eq!(s.store.open.as_ref().unwrap().entries.len(), 1, "the next folder's finding is left out");
        assert_eq!(wanted(&s.store), None, "its one finding held");
        press_folder(&mut s.store, "/w/agent-coord/walk");
        assert!(s.store.open.is_none() && s.store.folder.is_none());
    }

    /// The rows as Home says them: held against cap, what it holds, today's counts with the share from the store
    /// worked out of them, each row's machine named as Devices names it.
    #[test]
    fn the_stores_rows_say_held_against_cap_and_today_and_name_their_machine_as_devices_does() {
        let a: Answer = serde_json::from_str(&answer()).unwrap();
        assert_eq!(a.rows.len(), 2);
        assert_eq!(support(&a.rows[0]), "60.2 GB of 60.0 GB · 36,214 entries · today 883 jobs, 98% from the store");
        assert_eq!(support(&a.rows[1]), "6.2 GB of 20.0 GB · coordd, yaaihh.exe and 1 more · today 31 handed back, 4 fetched");
        assert_eq!(fill(&a.rows[0]), Some(1.0), "over its cap, the bar full");
        assert!(over(&a.rows[0]) && !over(&a.rows[1]), "only the crates are over their cap");
        let d = devices();
        assert_eq!((machine(Some("yaaihh-build-1"), &d), machine(None, &d)), (Some("yaaihh-build-1".into()), Some("WSL".into())));
        // a row of another type in a field leaves only that field out
        let odd: Answer = serde_json::from_value(json!({"rows": [{"id": "crates", "held_bytes": "lots", "count": 3}]})).unwrap();
        assert_eq!(support(&odd.rows[0]), "3 held");
    }

    /// The live answer's shape and figures (`coordc store` on engine 80 at 03:15Z, a2h-speed N-1446 p3304; build-runner's
    /// `crates_row`, `binaries_row`, `machine_store`): read and said as Home says them, and a row the build machine did
    /// not answer for says so with the time of the reading it shows.
    #[test]
    fn the_live_answer_reads_and_an_unread_row_says_why_and_when() {
        let real = json!({"rows": [
            {"id": "crates", "what": "Compiled crates", "device": "yaaihh-build-1", "as_of": "2026-11-25T03:15:00Z", "held_bytes": 66_571_993_088u64,
             "cap_bytes": 64_424_509_440u64, "count": 2940, "count_of": "entries", "today": {"jobs": 236, "compiles": 89944, "from_store": 81001}},
            {"id": "binaries", "what": "Binaries", "held_bytes": 21_474_836_480u64, "cap_bytes": 21_474_836_480u64, "count": 176, "count_of": "blobs",
             "today": {"handed": 63, "fetched": 45}, "holds": [{"name": "coordc", "hash8": "edea3b2d", "links": 4}, {"name": "coordd", "hash8": "0660dd68", "links": 3}]}
        ]});
        let a: Answer = serde_json::from_value(real).unwrap();
        assert_eq!(support(&a.rows[0]), "62.0 GB of 60.0 GB · 2,940 entries · today 236 jobs, 90% from the store");
        assert_eq!(support(&a.rows[1]), "20.0 GB of 20.0 GB · coordc, coordd · today 63 handed back, 45 fetched");
        let unread: Answer = serde_json::from_value(json!({"rows": [{"id": "crates", "what": "Compiled crates", "device": "yaaihh-build-1", "unread": "the build machine does not answer", "as_of": "2026-11-25T03:15:00Z", "held_bytes": 66_571_993_088u64}]})).unwrap();
        let at = crate::when::at("2026-11-25T03:15:00Z");
        assert_eq!(support(&unread.rows[0]), format!("not read now (the build machine does not answer); as of {at} · 62.0 GB"));
    }

    /// Home asks the rows as `Request::Store` of a coordd that takes it (STORE_PROTOCOL), once per round, and a press
    /// asks its row's first page; an older coordd is asked nothing.
    #[test]
    fn home_asks_the_store_and_a_press_its_page() {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::STORE_PROTOCOL - 1;
        let out = std::env::temp_dir().join(format!("yaaihh-store-asks-{}.jsonl", std::process::id()));
        let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        p.link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        s.projects = vec![p];
        s.cur = 0;
        ask(&mut s, 0);
        assert!(s.store.asked.is_none(), "an older coordd is asked nothing");
        s.projects[0].protocol = coord_types::STORE_PROTOCOL;
        ask(&mut s, 0);
        ask(&mut s, 0);
        press(&mut s.store, "crates");
        ask_page(&mut s, 0);
        ask_page(&mut s, 0);
        assert!(s.store.open.as_ref().is_some_and(|o| o.asking));
        s.projects[0].link = None;
        drop(child.stdin.take());
        let _ = child.wait();
        let sent: Vec<serde_json::Value> = std::fs::read_to_string(&out).unwrap().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        let _ = std::fs::remove_file(&out);
        let stores: Vec<&serde_json::Value> = sent.iter().filter_map(|v| v.get("req")).filter(|r| r["op"] == "store").collect();
        assert_eq!(stores.len(), 2, "the rows once, the page once: {sent:?}");
        assert!(stores[0].get("row").is_none() && stores[1]["row"] == "crates" && stores[1]["from"] == 0, "{stores:?}");
    }

    /// A press opens a row from its first page; a full page asks the next from where it ended, the last asks none; a
    /// second press closes it; a page for a row closed meanwhile is dropped.
    #[test]
    fn a_press_opens_a_rows_entries_two_hundred_at_a_time() {
        let mut st = Store::default();
        press(&mut st, "crates");
        assert_eq!(wanted(&st), Some(("crates".into(), 0)));
        let o = st.open.as_mut().unwrap();
        o.entries = vec![Entry::default(); 200];
        o.total = Some(450);
        assert_eq!(wanted(&st), Some(("crates".into(), 200)));
        st.open.as_mut().unwrap().entries.extend(vec![Entry::default(); 200]);
        assert_eq!(wanted(&st), Some(("crates".into(), 400)));
        st.open.as_mut().unwrap().entries.extend(vec![Entry::default(); 50]);
        assert_eq!(wanted(&st), None, "all 450 held");
        st.open.as_mut().unwrap().asking = true;
        assert_eq!(wanted(&st), None, "one page asked at a time");
        press(&mut st, "crates");
        assert_eq!(st.open, None);
        let crate_ = Entry { made_from: Some("serde 1.0.217".into()), by: Some(By { who: Some("docket-core".into()), branch: Some("edge8-store".into()), commit: Some("4b1539580".into()), job: None }), used_at: Some("2026-11-24T17:40:00Z".into()), bytes: Some(2_202_010), ..Entry::default() };
        let at = crate::when::at("2026-11-24T17:40:00Z");
        assert_eq!(entry_words(&crate_), ("serde 1.0.217".to_string(), format!("by docket-core on edge8-store 4b153958 · used {at} · 2.1 MB")));
        let bin = Entry { names: vec!["target/debug/coordd".into()], hash8: Some("00aba796".into()), links: Some(41), made_by: Some(By { job: Some("j12".into()), branch: Some("main".into()), ..By::default() }), ..Entry::default() };
        assert_eq!(entry_words(&bin), ("target/debug/coordd".to_string(), "00aba796 · 41 links · by job j12 on main".to_string()));
    }
}
