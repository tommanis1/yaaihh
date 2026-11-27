//! The Files page (owner 14 Nov, N-77: "a vscode file view in every project"): a project's folders as a tree on the
//! left, the file clicked on the right, read only.
//!
//! The roots, in this order: each folder of the project file's `[project] files`; each folder its agents' sessions
//! run in (stopped agents too); `conversations` (~/agent-coord/ID/conversations, when there is one); `definition`
//! (the folder the project file is in). A root that is an earlier root, or inside one, is left out; one that is no
//! folder here (a box's own path) shows grey, "in the box only", and does not open.
//!
//! A folder is listed when it is opened, never the tree walked: folders first, then files, each by name with case
//! aside; `.git` is not listed. A file shows as Markdown (rendered, or its source), an image, numbered lines (2,000 at
//! a time), or "binary, SIZE".
//!
//! Nothing is read while drawing: `poll` (the update step) and the clicks read, the page draws what they read,
//! memoised by `Files::rev`. While the page shows, the open folders are listed again and the shown file is read
//! again when its size or change time moved, on the window's 10-second tick (`App::wake_every`, the Agents page's).
//!
//! Kept per project in ui.json (state.rs): the open folders, the file shown, Rendered or Source, and both scroll
//! places.
//!
//! On Windows the folders are the WSL machine's: the same page asks coordd (docket-core6's Roots, ListDir and ReadFile,
//! N-412 p128) when the project's bridge speaks them, and draws the answers as they come; the paths are the server's,
//! joined with `/`. A bridge older than that gives the page's one line (elsewhere.rs).

use crate::{App, Sel};
use rune::{El, ScrollPos, ScrollSpec, Tx, col, icon, row, text};
use rune_kit::markdown::Markdown;
use rune_kit::{Button, ButtonVariant, Kit, SegmentedControl, glyphs};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// How often what the page shows is looked at again: the window's 10-second tick.
const TICK: Duration = Duration::from_secs(10);

/// How often a PDF in the web view is looked at again, so a rebuilt paper shows within a second or two.
const PDF_TICK: Duration = Duration::from_secs(1);

/// The lines of a text file shown at first, and how many more each "N more lines" shows.
const LINES: usize = 2000;

/// A file larger than this is not read (its size and change time still show).
const MAX_READ: u64 = 32 << 20;

/// The tree's width.
const TREE_W: f32 = 300.0;

/// The protocol from which a bridge serves the file requests (docket-core6's, p128): docket-core gives the number in
/// the release that brings them (p137), so it is set from that merge.
pub(crate) const FILES: u32 = 4;

/// A file request asked of coordd (Windows), for its reply ([`replied`]).
pub(crate) enum Asked {
    /// Project ID's roots.
    Roots(String),
    List(PathBuf),
    /// The shown file's size and mtime (`len: 0`), looked at on the tick.
    Stat(PathBuf),
    /// A piece of the shown file from FROM, its size and mtime as the stat gave them.
    Piece { path: PathBuf, size: u64, modified: Option<SystemTime>, from: u64 },
    /// Page PAGE of the shown PDF, its size and mtime as the stat or the disk gave them.
    Pdf { path: PathBuf, size: u64, modified: Option<SystemTime>, page: u32 },
}

/// What is kept of a project's page across a change of page and a restart.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Kept {
    /// The folders open, by path.
    pub open: BTreeSet<String>,
    /// The file shown, by path.
    pub shown: Option<String>,
    /// A Markdown file shown as its source.
    pub source: bool,
    /// Where the tree and the file are scrolled to (points from the top).
    pub tree_at: f32,
    pub file_at: f32,
}

/// A root of the tree.
#[derive(Clone, Debug, PartialEq, Hash)]
pub struct Root {
    pub name: String,
    pub path: PathBuf,
    /// A folder on this host (else a box's own path).
    pub here: bool,
}

/// What git says of an entry (the owner, N-606: generated files set apart from the "real" ones).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Git {
    /// Tracked, or no git here: drawn as ever.
    #[default]
    Plain,
    /// Untracked and not ignored: a real file being written; a faint `new` after its name.
    New,
    /// Ignored by git (build output): faint, after the others under a thin line.
    Generated,
}

impl Git {
    /// coordd's `git` field (N-608, N-743: git's own word "ignored"; the window says "generated"): an unknown value reads
    /// as none.
    fn served(v: Option<&str>) -> Git {
        match v {
            Some("new") => Git::New,
            Some("ignored") => Git::Generated,
            _ => Git::Plain,
        }
    }
}

/// A folder's entries as last listed: (name, a folder, what git says), in the tree's order; or why it could not be
/// listed.
type Listing = Result<Vec<(String, bool, Git)>, String>;

/// The shown file as last read.
pub struct Read {
    pub path: PathBuf,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub body: Body,
}

pub enum Body {
    Markdown(String),
    /// An image, in `App::images` under [`image_key`].
    Image,
    /// UTF-8 text, and its number of lines.
    Text(String, usize),
    Binary,
    TooLarge,
    Unread(String),
    /// A PDF's page PAGE of PAGES, drawn by coordd (`Request::PdfPage`), in `App::images` under [`image_key`]; PAGES
    /// 0 while the first is asked.
    Pdf { page: u32, pages: u32 },
    /// A PDF whole in the window's web view, served on loopback (pdf_view.rs).
    PdfView(crate::pdf_view::Served),
}

/// The Files page's state.
#[derive(Default)]
pub struct Files {
    /// By project id.
    pub kept: HashMap<String, Kept>,
    /// The roots as last worked out, by project id.
    pub(crate) roots: HashMap<String, Vec<Root>>,
    lists: HashMap<PathBuf, Listing>,
    pub file: Option<Read>,
    /// The lines of the shown text file drawn.
    limit: usize,
    /// Counts what the page shows changing: it is built again when this moves.
    rev: u64,
    /// When the page last looked at the disk, and for which project.
    checked: Option<(Instant, String)>,
    /// When a PDF in the web view was last looked at alone, between the ticks.
    pdf_checked: Option<Instant>,
    /// The scroll places as the viewports report them, by (project id, "tree" or "file").
    at: Arc<Mutex<HashMap<(String, &'static str), f32>>>,
    /// The places asked of the viewports when the page came on, until they report them (or 3 s pass): what they
    /// report meanwhile (their first layout's 0, a shorter tree) is not kept.
    restoring: Option<(String, Instant)>,
    /// Whether the page showed at the last poll.
    showing: bool,
    /// Windows: the requests asked of coordd and not answered, by what they ask, so none is asked twice.
    asked: BTreeSet<String>,
    /// Windows: the shown file's bytes as its pieces come.
    pieces: Option<Transfer>,
    /// A PDF is shown whole in the web view (pdf_view.rs), not page by page: on Windows with a window, where WebView2
    /// draws it; headless and elsewhere, coordd's pages.
    pub(crate) web_pdf: bool,
}

/// One file's read from coordd in pieces: whose, the size and change time its first piece gave, the bytes so far. A
/// piece of another file, out of order, or of the file changed since its first is never put in.
pub(crate) struct Transfer {
    path: PathBuf,
    seen: (u64, Option<u64>),
    bytes: Vec<u8>,
}

pub fn image_key(path: &Path) -> String {
    format!("files|{}", path.display())
}

impl Files {
    /// The page's state as a window starts it: PDFs whole in the web view on Windows, unless headless.
    pub fn for_window() -> Files {
        Files { web_pdf: cfg!(windows) && !rune::is_headless(), ..Files::default() }
    }

    fn kept(&self, id: &str) -> Kept {
        self.kept.get(id).cloned().unwrap_or_default()
    }
}

/// `~/x` as a path under HOME.
fn expand(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(r) => Path::new(&std::env::var("HOME").unwrap_or_default()).join(r),
        None if p == "~" => PathBuf::from(std::env::var("HOME").unwrap_or_default()),
        None => PathBuf::from(p),
    }
}

fn last_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

/// Project P's roots, in their order, the later ones inside an earlier one left out.
pub fn roots(p: &crate::Proj) -> Vec<Root> {
    let coord = expand(&format!("~/agent-coord/{}", p.id));
    let mut all: Vec<(String, PathBuf)> = Vec::new();
    // its repositories first, by the names Home gives them (paper-research N-1446 p3387, 1 and 3)
    for (name, folder) in crate::home::resolved_repos(p) {
        all.push((name, expand(&folder)));
    }
    for f in crate::views::strings(p.live.project.pointer("/project/files")) {
        let path = expand(&f);
        all.push((last_name(&path), path));
    }
    // each agent's home under its own name, before the folders agents work in (one may be its home)
    for a in &p.live.agents {
        if let Some(h) = crate::home::home_of(p, &a.name).filter(|h| Path::new(h).is_dir()) {
            all.push((format!("{} home", a.name), PathBuf::from(h)));
        }
    }
    for a in &p.live.agents {
        if let Some(c) = a.cwd.as_deref().filter(|c| !c.is_empty()) {
            let path = PathBuf::from(c);
            all.push((last_name(&path), path));
        }
    }
    let conv = coord.join("conversations");
    if conv.is_dir() {
        all.push(("conversations".into(), conv));
    }
    if let Some(def) = std::fs::read_to_string(coord.join("project")).ok().map(|t| PathBuf::from(t.trim())).and_then(|f| f.parent().map(Path::to_path_buf)).filter(|d| !d.as_os_str().is_empty()) {
        all.push(("definition".into(), def));
    }
    let mut out: Vec<Root> = Vec::new();
    for (name, path) in all {
        // a path as the host sees it (symlinks followed), so a folder named two ways is one root
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        if out.iter().any(|r| path.starts_with(&r.path)) {
            continue;
        }
        let here = path.is_dir();
        out.push(Root { name, path, here });
    }
    distinct(&mut out);
    out
}

/// Roots of one name told apart by the nearest folder above them that differs: `home (codex-pa)`, `home (neon-pa)`.
pub fn distinct(roots: &mut [Root]) {
    let names: Vec<String> = roots.iter().map(|r| r.name.clone()).collect();
    for name in &names {
        let same: Vec<usize> = (0..roots.len()).filter(|i| roots[*i].name == *name).collect();
        if same.len() < 2 {
            continue;
        }
        let above = |i: usize, up: usize| roots[i].path.ancestors().nth(up).and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned());
        let up = (1..8).find(|up| {
            let mut seen: Vec<Option<String>> = same.iter().map(|i| above(*i, *up)).collect();
            seen.sort();
            seen.dedup();
            seen.len() == same.len() && seen.iter().all(Option::is_some)
        });
        let ats: Vec<String> = same.iter().map(|i| up.and_then(|u| above(*i, u)).unwrap_or_else(|| roots[*i].path.parent().map(|p| p.display().to_string()).unwrap_or_default())).collect();
        for (i, at) in same.into_iter().zip(ats) {
            roots[i].name = format!("{name} ({at})");
        }
    }
}

/// Folder DIR's entries: folders first, then files, by name with case aside, the generated ones after all the others;
/// no `.git`.
fn list(dir: &Path) -> Listing {
    let rd = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    let states = git_states(dir);
    let mut v: Vec<(String, bool, Git)> = rd
        .filter_map(Result::ok)
        .map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            let g = states.get(&n).copied().unwrap_or_default();
            (n, e.path().is_dir(), g)
        })
        .filter(|(n, _, _)| n != ".git")
        .collect();
    order(&mut v);
    Ok(v)
}

/// The tree's order: the generated ones last; in each part folders first, then files, by name with case aside.
fn order(v: &mut [(String, bool, Git)]) {
    v.sort_by(|a, b| {
        (a.2 == Git::Generated)
            .cmp(&(b.2 == Git::Generated))
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
            .then_with(|| a.0.cmp(&b.0))
    });
}

/// What git says of folder DIR's own entries, by name: those untracked (`New`) and ignored (`Generated`) as a whole;
/// tracked ones, mixed folders and anything outside a repository are not in it. Git is only read (no optional locks),
/// asked for this folder alone, and stops at an ignored or untracked folder (`--directory`) instead of walking it.
fn git_states(dir: &Path) -> HashMap<String, Git> {
    let mut out = HashMap::new();
    for (g, ignored) in [(Git::New, false), (Git::Generated, true)] {
        // no console window where the window reads a Windows folder itself
        let mut c = crate::places::command("git");
        c.arg("--no-optional-locks").arg("-C").arg(dir).args(["ls-files", "-z", "--others", "--exclude-standard", "--directory"]);
        if ignored {
            c.arg("--ignored");
        }
        let Ok(o) = c.stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output() else { return HashMap::new() };
        if !o.status.success() {
            // not in a repository (or no git): no marks
            return HashMap::new();
        }
        for p in String::from_utf8_lossy(&o.stdout).split('\0').filter(|p| !p.is_empty()) {
            // only this folder's own entries, whole: `name` or `name/`
            let p = p.strip_suffix('/').unwrap_or(p);
            if !p.contains('/') {
                // ignored wins over untracked
                out.insert(p.to_string(), g);
            }
        }
    }
    out
}

fn is_image(p: &Path) -> bool {
    matches!(p.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref(), Some("png" | "jpg" | "jpeg" | "gif" | "webp"))
}

pub(crate) fn is_pdf(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

/// Whether the current project's coordd draws a PDF's pages (HOME_PROTOCOL; the owner's walk item 22).
fn draws_pdf(s: &App) -> bool {
    s.projects.get(s.cur).is_some_and(|p| p.protocol >= coord_types::HOME_PROTOCOL && p.link.is_some())
}

/// The page of PDF PATH shown now, else its first.
fn pdf_page_of(s: &App, path: &Path) -> u32 {
    match s.files.file.as_ref() {
        Some(Read { path: p, body: Body::Pdf { page, .. }, .. }) if p == path => *page,
        _ => 1,
    }
}

/// Page PAGE of the shown PDF PATH asked of coordd, drawn as wide as the page shows it.
fn ask_pdf(s: &mut App, path: PathBuf, size: u64, modified: Option<SystemTime>, page: u32) {
    let req = coord_types::Request::PdfPage { path: wire(&path), page, width: Some(2000) };
    ask(s, format!("pdf:{}", path.display()), req, Asked::Pdf { path, size, modified, page });
}

/// Whether the full-size viewer shows the shown PDF's page.
pub fn viewing_pdf(s: &App) -> bool {
    matches!(&s.files.file, Some(Read { path, body: Body::Pdf { .. }, .. }) if s.viewing.as_deref() == Some(image_key(path).as_str()))
}

/// Previous or Next, here or in the viewer: the shown PDF's page BY pages on, within its pages.
pub fn turn_pdf(s: &mut App, by: i32) {
    let Some(Read { path, size, modified, body: Body::Pdf { page, pages } }) = s.files.file.as_ref() else { return };
    let to = (*page as i32 + by).clamp(1, (*pages).max(1) as i32) as u32;
    if to != *page {
        let (path, size, modified) = (path.clone(), *size, *modified);
        ask_pdf(s, path, size, modified, to);
    }
}

fn is_markdown(p: &Path) -> bool {
    matches!(p.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref(), Some("md" | "markdown"))
}

/// Read file PATH (its size and change time already looked at); an image goes to the window's image cache.
fn read(s: &mut App, path: &Path, size: u64, modified: Option<SystemTime>) -> Read {
    // a PDF in the web view: read whole and served, the view reloading it in place when it changed
    if is_pdf(path) && s.files.web_pdf && size <= MAX_READ {
        let why = match std::fs::read(path).map_err(|e| e.to_string()).and_then(|b| serve_pdf(path, size, modified, b)) {
            Ok(body) => return Read { path: path.to_path_buf(), size, modified, body },
            Err(why) => why,
        };
        if let Some(Read { size, body: Body::PdfView(v), .. }) = s.files.file.as_ref().filter(|f| f.path == path) {
            return Read { path: path.to_path_buf(), size: *size, modified: None, body: Body::PdfView(v.clone()) };
        }
        if !draws_pdf(s) {
            return Read { path: path.to_path_buf(), size, modified, body: Body::Unread(why) };
        }
    }
    // a PDF: its page drawn by coordd, whatever its size; the page shown kept when the file changed
    if is_pdf(path) && draws_pdf(s) {
        let page = pdf_page_of(s, path);
        ask_pdf(s, path.to_path_buf(), size, modified, page);
        return Read { path: path.to_path_buf(), size, modified, body: Body::Pdf { page, pages: 0 } };
    }
    let body = if size > MAX_READ {
        Body::TooLarge
    } else {
        match std::fs::read(path) {
            Err(e) => Body::Unread(e.to_string()),
            Ok(bytes) => body_of(s, path, bytes),
        }
    };
    Read { path: path.to_path_buf(), size, modified, body }
}

/// File PATH's BYTES as the page shows them; an image goes to the window's image cache.
fn body_of(s: &mut App, path: &Path, bytes: Vec<u8>) -> Body {
    if is_image(path) {
        let cache = s.image_cache.get_or_insert_with(|| rune::ImageCache::new(Arc::new(|| {})));
        let img = cache.load(bytes);
        if rune::is_headless() {
            let _ = img.wait();
        }
        s.images.insert(image_key(path), crate::chat::Img::Ready(img));
        return Body::Image;
    }
    match String::from_utf8(bytes) {
        Ok(t) if is_markdown(path) => Body::Markdown(t),
        Ok(t) => {
            let n = t.lines().count();
            Body::Text(t, n)
        }
        Err(_) => Body::Binary,
    }
}

/// NAME in folder DIR: on Windows a path on the server, joined with `/` (Windows' own join puts `\`).
fn child(dir: &Path, name: &str) -> PathBuf {
    if crate::elsewhere::away() {
        PathBuf::from(format!("{}/{name}", dir.display().to_string().trim_end_matches('/')))
    } else {
        dir.join(name)
    }
}

/// A path as the server reads it.
pub(crate) fn wire(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

/// Whether the project's bridge serves its files (Windows).
pub(crate) fn served(p: &crate::Proj) -> bool {
    p.protocol >= FILES
}

/// Ask the current project's coordd REQ, as WHAT, once while it is unanswered (KEY).
fn ask(s: &mut App, key: String, req: coord_types::Request, what: Asked) {
    if !s.files.asked.insert(key.clone()) {
        return;
    }
    let p = &mut s.projects[s.cur];
    match p.link.as_mut().and_then(|l| l.ask(&req)) {
        Some(id) => {
            p.pending.insert(id, crate::Pending::Files(what));
        }
        None => {
            s.files.asked.remove(&key);
        }
    }
}

/// The page's refresh on Windows: the roots, the open folders and the shown file's stat asked of coordd.
fn refresh_remote(s: &mut App, id: &str) {
    if !served(&s.projects[s.cur]) {
        return;
    }
    use coord_types::Request;
    ask(s, format!("roots:{id}"), Request::Roots, Asked::Roots(id.to_string()));
    let kept = s.files.kept(id);
    let roots = s.files.roots.get(id).cloned().unwrap_or_default();
    for dir in kept.open.iter().map(PathBuf::from).filter(|d| roots.iter().any(|r| r.here && d.starts_with(&r.path))) {
        let req = list_request(&s.projects[s.cur], &dir);
        ask(s, format!("list:{}", dir.display()), req, Asked::List(dir));
    }
    match kept.shown.as_deref() {
        None => {
            if s.files.file.take().is_some() {
                s.files.rev += 1;
            }
        }
        Some(_) => look_shown(s, id),
    }
}

/// Folder DIR asked of project P's coordd: with git's states only of one that knows them (N-608: below
/// `FILE_STATES_PROTOCOL` coordd runs no git, and nothing is marked).
fn list_request(p: &crate::Proj, dir: &Path) -> coord_types::Request {
    coord_types::Request::ListDir { path: wire(dir), git: p.protocol >= coord_types::FILE_STATES_PROTOCOL }
}

/// coordd's answer to a file request (Windows): what the page shows, as the disk's read would have made it.
pub(crate) fn replied(s: &mut App, what: Asked, ok: bool, out: String) {
    #[derive(Deserialize)]
    struct Entry {
        name: String,
        #[serde(default)]
        dir: bool,
        #[serde(default)]
        outside: bool,
        /// "new", "ignored" (N-608, N-743); absent: tracked or no git.
        #[serde(default)]
        git: Option<String>,
    }
    #[derive(Deserialize)]
    struct Piece {
        size: u64,
        modified: Option<u64>,
        #[serde(default)]
        data: String,
    }
    #[derive(Deserialize)]
    struct ServedRoot {
        name: String,
        path: String,
        here: bool,
    }
    use base64::Engine;
    let f = &mut s.files;
    match what {
        Asked::Roots(id) => {
            f.asked.remove(&format!("roots:{id}"));
            match serde_json::from_str::<Vec<ServedRoot>>(&out).ok().filter(|_| ok) {
                Some(v) => {
                    let mut roots: Vec<Root> = v.into_iter().map(|r| Root { name: r.name, path: PathBuf::from(r.path), here: r.here }).collect();
                    distinct(&mut roots);
                    if f.roots.get(&id) != Some(&roots) {
                        f.roots.insert(id, roots);
                        f.rev += 1;
                    }
                }
                None => crate::log::line(&format!("{id}: the Files page's roots: {out}")),
            }
        }
        Asked::List(dir) => {
            f.asked.remove(&format!("list:{}", dir.display()));
            // an entry coordd would not open (a link out of the roots, a credential place) is not listed
            let l: Listing = if ok {
                serde_json::from_str::<Vec<Entry>>(&out)
                    .map(|v| {
                        let mut v: Vec<_> = v.into_iter().filter(|e| !e.outside).map(|e| (e.name, e.dir, Git::served(e.git.as_deref()))).collect();
                        order(&mut v);
                        v
                    })
                    .map_err(|e| e.to_string())
            } else {
                Err(out)
            };
            if f.lists.get(&dir) != Some(&l) {
                f.lists.insert(dir, l);
                f.rev += 1;
            }
        }
        Asked::Stat(path) => {
            f.asked.remove(&format!("stat:{}", path.display()));
            let p = match (ok, serde_json::from_str::<Piece>(&out)) {
                (true, Ok(p)) => p,
                (true, Err(e)) => return unread(s, path, e.to_string()),
                (false, _) => return unread(s, path, out),
            };
            let modified = p.modified.map(|ms| SystemTime::UNIX_EPOCH + Duration::from_millis(ms));
            if f.file.as_ref().is_some_and(|x| x.path == path && x.size == p.size && x.modified == modified && !matches!(x.body, Body::Unread(_))) {
                return;
            }
            if is_pdf(&path) && draws_pdf(s) && !(s.files.web_pdf && p.size <= MAX_READ) {
                let page = pdf_page_of(s, &path);
                return ask_pdf(s, path, p.size, modified, page);
            }
            if p.size > MAX_READ {
                return shown(s, Read { path, size: p.size, modified, body: Body::TooLarge });
            }
            // one read of a file at a time: a stat while its pieces still come waits for them
            if s.files.asked.contains(&format!("read:{}", path.display())) {
                return;
            }
            s.files.pieces = None;
            let req = coord_types::Request::ReadFile { path: wire(&path), from: Some(0), len: None };
            ask(s, format!("read:{}", path.display()), req, Asked::Piece { path, size: p.size, modified, from: 0 });
        }
        Asked::Piece { path, size, modified, from } => {
            f.asked.remove(&format!("read:{}", path.display()));
            let p = match serde_json::from_str::<Piece>(&out) {
                Ok(p) if ok => p,
                _ => return unread(s, path, out),
            };
            let Ok(data) = base64::engine::general_purpose::STANDARD.decode(&p.data) else { return unread(s, path, "unreadable answer".into()) };
            let seen = (p.size, p.modified);
            if from == 0 {
                f.pieces = Some(Transfer { path: path.clone(), seen, bytes: Vec::new() });
            }
            let Some(t) = f.pieces.as_mut().filter(|t| t.path == path && t.bytes.len() as u64 == from) else { return };
            if t.seen != seen {
                // the file changed between its pieces: nothing of it shown, and the next look reads it again
                f.pieces = None;
                if let Some(x) = f.file.as_mut().filter(|x| x.path == path) {
                    x.modified = None;
                }
                return;
            }
            t.bytes.extend_from_slice(&data);
            let got = t.bytes.len() as u64;
            if got < size.min(p.size) && !data.is_empty() {
                let req = coord_types::Request::ReadFile { path: wire(&path), from: Some(got), len: None };
                return ask(s, format!("read:{}", path.display()), req, Asked::Piece { path, size, modified, from: got });
            }
            let bytes = f.pieces.take().map(|t| t.bytes).unwrap_or_default();
            if is_pdf(&path) && s.files.web_pdf {
                return shown_pdf(s, path, p.size, modified, bytes);
            }
            let body = body_of(s, &path, bytes);
            shown(s, Read { path, size: p.size, modified, body });
        }
        Asked::Pdf { path, size, modified, page } => {
            f.asked.remove(&format!("pdf:{}", path.display()));
            #[derive(Deserialize)]
            struct Drawn {
                pages: u32,
                page: u32,
                data: String,
            }
            let d = match serde_json::from_str::<Drawn>(&out) {
                Ok(d) if ok => d,
                _ => return unread(s, path, if ok { format!("page {page} unreadable") } else { out }),
            };
            let Ok(png) = base64::engine::general_purpose::STANDARD.decode(&d.data) else { return unread(s, path, "unreadable page".into()) };
            let cache = s.image_cache.get_or_insert_with(|| rune::ImageCache::new(Arc::new(|| {})));
            let img = cache.load(png);
            if rune::is_headless() {
                let _ = img.wait();
            }
            s.images.insert(image_key(&path), crate::chat::Img::Ready(img));
            shown(s, Read { path, size, modified, body: Body::Pdf { page: d.page, pages: d.pages } });
        }
    }
}

/// PDF PATH's BYTES served to the web view and shown there. Bytes pdflatex is still writing are not shown: what the
/// view had stays, its change time forgotten so the next look reads the file again. A PDF the view cannot serve goes
/// to coordd's pages when it draws them.
fn shown_pdf(s: &mut App, path: PathBuf, size: u64, modified: Option<SystemTime>, bytes: Vec<u8>) {
    match serve_pdf(&path, size, modified, bytes) {
        Ok(body) => shown(s, Read { path, size, modified, body }),
        Err(why) => {
            let in_view = s.files.file.as_ref().is_some_and(|f| f.path == path && matches!(f.body, Body::PdfView(_)));
            if in_view {
                if let Some(f) = s.files.file.as_mut() {
                    f.modified = None;
                }
            } else if draws_pdf(s) {
                let page = pdf_page_of(s, &path);
                ask_pdf(s, path, size, modified, page);
            } else {
                unread(s, path, why);
            }
        }
    }
}

/// PDF PATH's BYTES served to the web view, as its size and change time stamp them.
fn serve_pdf(path: &Path, size: u64, modified: Option<SystemTime>, bytes: Vec<u8>) -> Result<Body, String> {
    let ms = modified.and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64);
    crate::pdf_view::server().and_then(|v| v.serve(path, bytes, (size, ms))).map(Body::PdfView)
}

/// Whether the shown file is a PDF in the web view, looked at each [`PDF_TICK`].
fn viewing_pdf_whole(s: &App) -> bool {
    matches!(&s.files.file, Some(Read { body: Body::PdfView(_), .. }))
}

/// The shown file is R now (from its top when it is another file).
fn shown(s: &mut App, r: Read) {
    if s.files.file.as_ref().is_none_or(|f| f.path != r.path) {
        s.files.limit = LINES;
    }
    s.files.file = Some(r);
    s.files.rev += 1;
}

fn unread(s: &mut App, path: PathBuf, why: String) {
    shown(s, Read { path, size: 0, modified: None, body: Body::Unread(why) });
}

/// Look at the disk for the current project's page: its roots, the open folders listed again, the shown file read
/// again when it changed (or another is shown). FORCE: whatever the tick.
pub fn refresh(s: &mut App, force: bool) {
    let Some(id) = s.proj().map(|p| p.id.clone()) else { return };
    let due = force || s.files.checked.as_ref().is_none_or(|(at, was)| *was != id || at.elapsed() >= TICK);
    if !due {
        // between the ticks only a PDF in the web view is looked at, and only it: no roots, folders or git
        if viewing_pdf_whole(s) && s.files.pdf_checked.is_none_or(|at| at.elapsed() >= PDF_TICK) {
            s.files.pdf_checked = Some(Instant::now());
            look_shown(s, &id);
        }
        return;
    }
    s.files.checked = Some((Instant::now(), id.clone()));
    s.files.pdf_checked = Some(Instant::now());
    if crate::elsewhere::away() {
        return refresh_remote(s, &id);
    }
    let mut changed = false;
    let roots = roots(&s.projects[s.cur]);
    if s.files.roots.get(&id) != Some(&roots) {
        s.files.roots.insert(id.clone(), roots.clone());
        changed = true;
    }
    let kept = s.files.kept(&id);
    // the open folders under a root here, each listed again
    for dir in kept.open.iter().map(PathBuf::from).filter(|d| roots.iter().any(|r| r.here && d.starts_with(&r.path))) {
        let l = list(&dir);
        if s.files.lists.get(&dir) != Some(&l) {
            s.files.lists.insert(dir, l);
            changed = true;
        }
    }
    if changed {
        s.files.rev += 1;
    }
    look_shown(s, &id);
}

/// The shown file of project ID read again when its size or change time moved: on Windows its stat asked of coordd,
/// here the disk's.
fn look_shown(s: &mut App, id: &str) {
    let shown = s.files.kept(id).shown.map(PathBuf::from);
    if crate::elsewhere::away() {
        if let Some(path) = shown.filter(|_| served(&s.projects[s.cur])) {
            ask(s, format!("stat:{}", path.display()), coord_types::Request::ReadFile { path: wire(&path), from: None, len: Some(0) }, Asked::Stat(path));
        }
        return;
    }
    let changed = match shown {
        None => s.files.file.take().is_some(),
        Some(path) => {
            let meta = std::fs::metadata(&path);
            let (size, modified) = meta.as_ref().map_or((0, None), |m| (m.len(), m.modified().ok()));
            let same = s.files.file.as_ref().is_some_and(|f| f.path == path && f.size == size && f.modified == modified && meta.is_ok());
            if !same {
                let r = match meta {
                    Ok(_) => read(s, &path, size, modified),
                    Err(e) => Read { path: path.clone(), size: 0, modified: None, body: Body::Unread(e.to_string()) },
                };
                if s.files.file.as_ref().is_none_or(|f| f.path != path) {
                    s.files.limit = LINES;
                }
                s.files.file = Some(r);
            }
            !same
        }
    };
    if changed {
        s.files.rev += 1;
    }
}

/// The update step: while the page shows, the disk looked at on the tick, and the scroll places kept (or put back
/// when the page comes on). Whether something it shows changed.
pub fn poll(s: &mut App, tx: &mut Tx) -> bool {
    let shows = s.sel == Sel::Files && s.proj().is_some();
    let came_on = shows && !s.files.showing;
    s.files.showing = shows;
    if !shows {
        s.files.restoring = None;
        return false;
    }
    let rev = s.files.rev;
    refresh(s, came_on);
    s.wake_every(if viewing_pdf_whole(s) { PDF_TICK } else { TICK }, tx);
    let id = s.projects[s.cur].id.clone();
    if came_on || s.files.restoring.as_ref().is_some_and(|(was, _)| *was != id) {
        s.files.restoring = Some((id.clone(), Instant::now()));
    }
    let kept = s.files.kept(&id);
    let at = s.files.at.lock().unwrap().clone();
    let reported = |what: &'static str| at.get(&(id.clone(), what)).copied();
    if let Some((_, since)) = &s.files.restoring {
        let there = |what, want: f32| reported(what).is_some_and(|y| (y - want).abs() < 1.0);
        // a PDF in the web view keeps its own place: no file scroll to wait for
        let whole = viewing_pdf_whole(s);
        if (there("tree", kept.tree_at) && (whole || there("file", kept.file_at))) || since.elapsed() > Duration::from_secs(3) {
            s.files.restoring = None;
        } else {
            tx.scroll_to_instant("files-tree", 0.0, kept.tree_at);
            if !whole {
                tx.scroll_to_instant("files-file", 0.0, kept.file_at);
            }
            tx.wake_after(Duration::from_millis(100));
        }
    } else {
        let k = s.files.kept.entry(id.clone()).or_default();
        if let Some(y) = reported("tree") {
            k.tree_at = y;
        }
        if let Some(y) = reported("file") {
            k.file_at = y;
        }
    }
    s.files.rev != rev
}

/// A click on folder DIR: it opens (listed now) or closes.
fn toggle(s: &mut App, dir: String) {
    let Some(id) = s.proj().map(|p| p.id.clone()) else { return };
    let k = s.files.kept.entry(id).or_default();
    if !k.open.remove(&dir) {
        k.open.insert(dir);
    }
    s.files.rev += 1;
    refresh(s, true);
}

/// A click on file PATH: it is shown, from its top.
fn show(s: &mut App, path: String, tx: &mut Tx) {
    let Some(id) = s.proj().map(|p| p.id.clone()) else { return };
    let k = s.files.kept.entry(id.clone()).or_default();
    if k.shown.as_deref() == Some(path.as_str()) {
        return;
    }
    k.shown = Some(path.clone());
    k.file_at = 0.0;
    s.files.at.lock().unwrap().insert((id, "file"), 0.0);
    tx.scroll_to_instant("files-file", 0.0, 0.0);
    s.files.rev += 1;
    refresh(s, true);
    in_code(s, &path);
}

/// File PATH, just shown, opened in the code view too when it is the page now (`code::shows`).
fn in_code(s: &mut App, path: &str) {
    if crate::code::shows(s, s.cur) {
        crate::code::open(s, s.cur, coord_types::codeview::CodeTarget::File { path: path.to_string(), line: None });
    }
}

/// The Files page showing PATH (a thread's value, ran.rs): as written when absolute, else under the first root that
/// has it (the agents' folders, the project's), its folders open down to it. A `:line` after it is not a file's.
pub fn open_at(s: &mut App, path: &str) {
    let Some(id) = s.proj().map(|p| p.id.clone()) else { return };
    let mut bare = path;
    for _ in 0..2 {
        match bare.rsplit_once(':') {
            Some((a, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => bare = a,
            _ => break,
        }
    }
    // on Windows the roots are coordd's, and a path is the server's (absolute when it starts with `/`)
    let away = crate::elsewhere::away();
    let roots = if away { s.files.roots.get(&id).cloned().unwrap_or_default() } else { roots(&s.projects[s.cur]) };
    let want = PathBuf::from(bare);
    let (root, file) = if want.is_absolute() || (away && bare.starts_with('/')) {
        (roots.iter().find(|r| want.starts_with(&r.path)).map(|r| r.path.clone()), want)
    } else {
        match roots.iter().find(|r| r.here && !away && r.path.join(&want).exists()).or(roots.first()) {
            Some(r) => (Some(r.path.clone()), child(&r.path, bare)),
            None => (None, want),
        }
    };
    let k = s.files.kept.entry(id.clone()).or_default();
    if let Some(root) = root {
        let mut d = file.parent();
        while let Some(dir) = d.filter(|d| d.starts_with(&root)) {
            k.open.insert(dir.to_string_lossy().into_owned());
            d = dir.parent();
        }
    }
    let shown = file.to_string_lossy().into_owned();
    k.shown = Some(shown.clone());
    k.file_at = 0.0;
    s.files.at.lock().unwrap().insert((id, "file"), 0.0);
    s.sel = Sel::Files;
    s.files.rev += 1;
    refresh(s, true);
    in_code(s, &shown);
}

/// The Files page with repository NAME, at FOLDER as the project file writes it, open: the root that is that folder
/// (here, `~/` taken as home; on Windows coordd's root by its path or its name), else the folder as written, its tree
/// shown open at the top.
pub fn open_folder(s: &mut App, name: &str, folder: &str) {
    _ = open_root(s, name, folder);
}

/// FILE of repository NAME at FOLDER shown in Files, its folder open.
pub fn open_file(s: &mut App, name: &str, folder: &str, file: &str) {
    let Some(root) = open_root(s, name, folder) else { return };
    let Some(id) = s.proj().map(|p| p.id.clone()) else { return };
    let path = root.join(file).to_string_lossy().into_owned();
    s.files.kept.entry(id).or_default().shown = Some(path.clone());
    s.files.rev += 1;
    refresh(s, true);
    in_code(s, &path);
}

/// [`open_folder`], answering the root it opened.
fn open_root(s: &mut App, name: &str, folder: &str) -> Option<PathBuf> {
    let id = s.proj().map(|p| p.id.clone())?;
    let away = crate::elsewhere::away();
    let roots = if away { s.files.roots.get(&id).cloned().unwrap_or_default() } else { roots(&s.projects[s.cur]) };
    let want = if away { PathBuf::from(folder) } else { std::fs::canonicalize(expand(folder)).unwrap_or_else(|_| expand(folder)) };
    let path = roots.iter().find(|r| r.path == want).or_else(|| roots.iter().find(|r| r.name == name)).map(|r| r.path.clone()).unwrap_or(want);
    let k = s.files.kept.entry(id.clone()).or_default();
    k.open.insert(path.to_string_lossy().into_owned());
    k.tree_at = 0.0;
    s.files.at.lock().unwrap().insert((id, "tree"), 0.0);
    s.sel = Sel::Files;
    s.files.rev += 1;
    refresh(s, true);
    Some(path)
}

/// The disk looked at on the tick with the page not shown, for an agent's Repos drawer (drawer.rs): whether something
/// it shows changed.
pub(crate) fn look(s: &mut App) -> bool {
    let rev = s.files.rev;
    refresh(s, false);
    s.files.rev != rev
}

/// Folder DIR of project PI opened in the tree, down from the root that holds it, the page not shown: an agent's
/// Repos drawer shows it (drawer.rs).
pub(crate) fn open_in(s: &mut App, pi: usize, dir: &Path) {
    let Some(id) = s.projects.get(pi).map(|p| p.id.clone()) else { return };
    let dir = if crate::elsewhere::away() { dir.to_path_buf() } else { std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf()) };
    refresh(s, true);
    let roots = s.files.roots.get(&id).cloned().unwrap_or_default();
    if let Some(r) = roots.iter().find(|r| dir.starts_with(&r.path)) {
        let k = s.files.kept.entry(id).or_default();
        for a in dir.ancestors().take_while(|a| a.starts_with(&r.path)) {
            k.open.insert(a.display().to_string());
        }
    }
    s.files.rev += 1;
    refresh(s, true);
}

/// The root of P that holds folder DIR as the page draws it, its tree over the file he shows, for an agent's Repos
/// drawer (drawer.rs): the folders he opens and the file he shows are the page's own.
pub(crate) fn tree_at(s: &App, p: &crate::Proj, dir: &Path, k: &Kit) -> El {
    let dir = if crate::elsewhere::away() { dir.to_path_buf() } else { std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf()) };
    let roots = s.files.roots.get(&p.id).cloned().unwrap_or_default();
    let Some(root) = roots.iter().find(|r| dir.starts_with(&r.path)) else {
        return col().px(12.0).child(text("Not in the project's folders yet.").font(k.ty.small).color(k.c.text_faint).wrap());
    };
    let kept = s.files.kept(&p.id);
    let mut rows = Vec::new();
    tree_rows(s, &kept, root, &root.path, (root.name.clone(), true, Git::Plain), 0, k, &mut rows);
    let tree = col().key("drawer-files-tree").w_full().max_h(280.0).min_h(0.0).border_sides([1.0, 0.0, 1.0, 0.0], k.c.border_weak).scroll(ScrollSpec { stick_bottom: false }).child(col().w_full().py(6.0).children(rows));
    col().w_full().grow(1.0).min_h(0.0).child(tree).child(file_pane(s, p, &roots, &kept, k))
}

/// The page, built again only when what it shows changed.
pub fn page(s: &App, p: &crate::Proj, k: &Kit) -> El {
    // where coordd serves the code view, Files is it: every repository in one workspace (code.rs); a PDF is drawn here
    let pi = s.place_of(p);
    if crate::code::shows(s, pi) {
        return crate::code::page(s, pi, k);
    }
    if crate::elsewhere::away() && !served(p) {
        return crate::elsewhere::page(k);
    }
    let kept = s.files.kept(&p.id);
    let mut v = crate::version::Version::default();
    v.add((s.files.rev, &p.id, s.files.limit, k.dark)).debug(&kept.open).debug(&kept.shown).add(kept.source);
    // the marks of his comments on the shown file, and who reads them (`reader`)
    if let Some(f) = &s.files.file {
        crate::ask::marks_version(s, p, &file_source(&f.path), v.hasher());
        for a in &p.live.agents {
            v.add((&a.name, &a.role, &a.cwd));
        }
    }
    if let Some(f) = &s.files.file
        && let Some(crate::chat::Img::Ready(img)) = s.images.get(&image_key(&f.path))
    {
        v.add((img.size(), img.error().map(|e| e.to_string())));
    }
    v.add(chrono::Utc::now().timestamp() / 60);
    rune::memo::memo_with_hash(format!("files-{}", p.id), v.finish(), || built(s, p, &kept, k))
}

fn built(s: &App, p: &crate::Proj, kept: &Kept, k: &Kit) -> El {
    let roots = s.files.roots.get(&p.id).cloned().unwrap_or_default();
    let mut rows = Vec::new();
    if roots.is_empty() {
        rows.push(col().padding(12.0).child(text("This project has no folders to show yet.").font(k.ty.small).color(k.c.text_muted).wrap()));
    }
    for r in &roots {
        tree_rows(s, kept, r, &r.path, (r.name.clone(), true, Git::Plain), 0, k, &mut rows);
    }
    let (id, at) = (p.id.clone(), s.files.at.clone());
    let tree = col()
        .key("files-tree")
        .w(TREE_W)
        .h_full()
        .min_h(0.0)
        .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_weak)
        .scroll(ScrollSpec { stick_bottom: false })
        .on_scroll_offset(move |pos: ScrollPos| _ = at.lock().unwrap().insert((id.clone(), "tree"), pos.offset[1]))
        .child(col().w_full().py(6.0).children(rows));
    row().key("files").w_full().h_full().min_h(0.0).child(tree).child(file_pane(s, p, &roots, kept, k))
}

/// Folder or file PATH (NAME, a folder, what git says; at DEPTH) and, for an open folder, what it holds. What a
/// generated folder holds is generated too.
#[allow(clippy::too_many_arguments)]
fn tree_rows(s: &App, kept: &Kept, root: &Root, path: &Path, (name, dir, git): (String, bool, Git), depth: usize, k: &Kit, out: &mut Vec<El>) {
    let key = path.display().to_string();
    let open = dir && root.here && kept.open.contains(&key);
    let selected = !dir && kept.shown.as_deref() == Some(key.as_str());
    let glyph = if !dir {
        rune_kit::file_icon(&name)
    } else if open {
        &glyphs::FOLDER_OPEN
    } else {
        &glyphs::FOLDER
    };
    let faint = !root.here || git == Git::Generated;
    let ink = if faint { k.c.text_faint } else { k.c.text_base };
    let chevron = if dir {
        icon(&glyphs::CHEVRON_DOWN, 12.0).color(k.c.icon_muted).rotate(if open { 0.0 } else { -std::f32::consts::FRAC_PI_2 })
    } else {
        rune::el().w(12.0).h(12.0)
    };
    let mut line = row()
        .key(format!("f:{key}"))
        .w_full()
        .h(24.0)
        .pl(8.0 + depth as f32 * 14.0)
        .pr(8.0)
        .gap(4.0)
        .center_y()
        .radius(k.radius.sm)
        .child(chevron)
        .child(icon(glyph, 14.0).color(if faint { k.c.icon_faint } else { k.c.icon_muted }))
        .child(text(name).font(k.ty.ui).color(ink).truncate().shrink());
    if git == Git::New {
        line = line.child(text("new").font(k.ty.small).color(k.c.text_faint));
    }
    line = line.child(rune::el().grow(1.0));
    if depth == 0 && !root.here {
        line = line.child(text("in the box only").font(k.ty.small).color(k.c.text_faint));
    }
    if selected {
        line = line.fill(k.c.layer_03);
    }
    let line = if !root.here {
        line
    } else if dir {
        let d = key.clone();
        k.interactive(line).on_press(move |s: &mut App| toggle(s, d.clone()))
    } else {
        let f = key.clone();
        k.interactive(line).on_press_tx(move |s: &mut App, tx: &mut Tx| show(s, f.clone(), tx))
    };
    out.push(line);
    if !open {
        return;
    }
    match s.files.lists.get(path) {
        None => out.push(note_row("…", depth + 1, k)),
        Some(Err(e)) => out.push(note_row(e, depth + 1, k)),
        Some(Ok(v)) if v.is_empty() => out.push(note_row("empty", depth + 1, k)),
        Some(Ok(v)) => {
            let mut set_apart = false;
            for (n, d, g) in v {
                // inside a generated folder all is generated, without another line; inside a new one the folder's
                // `new` says it for all
                let g = match (git, *g) {
                    (Git::Generated, _) => Git::Generated,
                    (Git::New, Git::New) => Git::Plain,
                    (_, g) => g,
                };
                if g == Git::Generated && git != Git::Generated && !set_apart {
                    set_apart = true;
                    out.push(generated_row(depth + 1, k));
                }
                tree_rows(s, kept, root, &child(path, n), (n.clone(), *d, g), depth + 1, k, out);
            }
        }
    }
}

/// The thin line the generated entries of a folder come under, with the faint word.
fn generated_row(depth: usize, k: &Kit) -> El {
    row()
        .w_full()
        .h(20.0)
        .pl(8.0 + depth as f32 * 14.0 + 16.0)
        .pr(8.0)
        .gap(6.0)
        .center_y()
        .child(text("generated").font(k.ty.small).color(k.c.text_faint))
        .child(rune::el().grow(1.0).h(1.0).fill(k.c.border_weak))
}

/// What git says of PATH as the listings have it: generated when it or a folder above it is.
fn git_of(s: &App, path: &Path) -> Git {
    let mut own = Git::Plain;
    let mut at = path;
    while let (Some(parent), Some(name)) = (at.parent(), at.file_name()) {
        let n = name.to_string_lossy();
        if let Some(Ok(v)) = s.files.lists.get(parent)
            && let Some((_, _, g)) = v.iter().find(|(e, _, _)| *e == n)
        {
            if *g == Git::Generated {
                return Git::Generated;
            }
            if at == path {
                own = *g;
            }
        }
        at = parent;
    }
    own
}

fn note_row(t: &str, depth: usize, k: &Kit) -> El {
    row().w_full().h(24.0).pl(8.0 + depth as f32 * 14.0 + 16.0).center_y().child(text(t.to_string()).font(k.ty.small).color(k.c.text_faint).truncate())
}

/// SIZE as people read it.
pub fn size(n: u64) -> String {
    match n {
        n if n < 1024 => format!("{n} B"),
        n if n < 1024 * 1024 => format!("{:.1} KB", n as f64 / 1024.0),
        n if n < 1024 * 1024 * 1024 => format!("{:.1} MB", n as f64 / (1024.0 * 1024.0)),
        n => format!("{:.1} GB", n as f64 / (1024.0 * 1024.0 * 1024.0)),
    }
}

/// The shown file: its header and its body, under its own scroll.
fn file_pane(s: &App, p: &crate::Proj, roots: &[Root], kept: &Kept, k: &Kit) -> El {
    let pane = col().grow(1.0).min_w(0.0).h_full().min_h(0.0);
    let Some(f) = s.files.file.as_ref().filter(|f| kept.shown.as_deref() == Some(&*f.path.to_string_lossy())) else {
        return pane.child(col().padding(24.0).child(text("Pick a file").font(k.ty.body).color(k.c.text_muted).wrap()));
    };
    let shown = match roots.iter().find(|r| f.path.starts_with(&r.path)) {
        Some(r) => match f.path.strip_prefix(&r.path).ok().filter(|x| !x.as_os_str().is_empty()) {
            Some(rest) => format!("{}/{}", r.name, rest.display()),
            None => r.name.clone(),
        },
        None => f.path.display().to_string(),
    };
    let (label, pi) = (shown.clone(), s.projects.iter().position(|q| std::ptr::eq(q, p)).unwrap_or(s.cur));
    let when = f.modified.map(|t| crate::when::at(&chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())).unwrap_or_default();
    let target = format!("file:{}", f.path.display());
    let generated = git_of(s, &f.path) == Git::Generated;
    let mut head = row()
        .w_full()
        .px(16.0)
        .py(8.0)
        .gap(12.0)
        .center_y()
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_weak)
        .child(text(shown).font(k.ty.ui_medium).color(if generated { k.c.text_faint } else { k.c.text_base }).truncate().shrink())
        .child(if generated { text("generated").font(k.ty.small).color(k.c.text_faint) } else { rune::el() })
        .child(rune::el().grow(1.0))
        .child(text(size(f.size)).font(k.ty.small).color(k.c.text_muted))
        .child(text(when).font(k.ty.small).color(k.c.text_muted));
    if matches!(f.body, Body::Markdown(_)) {
        head = head.child(
            SegmentedControl::new(&["Rendered", "Source"], Some(kept.source as usize))
                .label("Show")
                .fit()
                .on_pick(|s: &mut App, i: usize, _: &mut Tx| {
                    let Some(id) = s.proj().map(|p| p.id.clone()) else { return };
                    s.files.kept.entry(id).or_default().source = i == 1;
                    s.files.rev += 1;
                })
                .build(k),
        );
    }
    head = head.child(Button::new("Open in the code view").variant(ButtonVariant::Ghost).on_press(move |_: &mut App| crate::paths::open(&target, None)).build(k));
    let dir = f.path.parent().map(|d| d.display().to_string());
    let body: El = match &f.body {
        Body::Markdown(t) if !kept.source => {
            let cwd = dir.clone();
            // a passage selected offers Comment, as in a chat or a thread; the comment goes to the agent whose folder
            // holds the file, else the project's answerer (0.2 line C15; consult-lead N-1335 p4, (C)), its mark here
            let src = crate::paths::linkify(t);
            let source = file_source(&f.path);
            let marks = crate::ask::marks(s, p, &source, &src, k);
            let who = reader(p, &f.path);
            let words = |src: String, marks: Vec<crate::ask::Mark>| {
                let cwd = cwd.clone();
                Markdown::new(src).highlights(marks).selectable(true).on_link(move |_: &mut App, url| crate::paths::open(url, cwd.clone())).build(k)
            };
            // while the comment box stands in the flow at a passage here, the lines after it move down under it, its
            // room between (I-1476; paper-research N-1477 p9)
            let shown = match crate::ask::cut(s, &source, &src, crate::ask::Here::default()) {
                Some(cut) => crate::ask::split(s, &source, &src, &marks, cut, "file-words", |part, marks, id| words(part, marks).key(id)),
                None => words(src, marks),
            };
            col()
                .key(FILE_KEY)
                .w_full()
                .padding(24.0)
                .max_w(900.0)
                .selection_group(true)
                .selection_menu_item("comment", "Comment", move |s: &mut App, sel, _| crate::ask::comment_on_file(s, pi, &source, &label, who.as_deref(), sel))
                .on_highlight_press(|s: &mut App, key, _| crate::ask::highlight_pressed(s, key))
                .child(shown)
                .children(crate::ask::room_at(s, FILE_KEY))
        }
        Body::Markdown(t) => lines(t, t.lines().count(), s.files.limit, k),
        Body::Text(t, n) => lines(t, *n, s.files.limit, k),
        Body::Image => match s.images.get(&image_key(&f.path)) {
            Some(crate::chat::Img::Ready(img)) if img.error().is_none() => {
                let w = img.size().map_or(720.0, |[w, _]| (w as f32 / 2.0).min(720.0));
                let open = image_key(&f.path);
                col().padding(24.0).child(rune::el().w(w).radius(k.radius.sm).border(1.0, k.c.border_weak).pixels(img.pixels()).cursor(rune::Cursor::Pointer).on_press(move |s: &mut App| crate::viewer::show(s, open.clone())))
            }
            Some(crate::chat::Img::Ready(img)) => faint(format!("image: {}", img.error().map(|e| e.to_string()).unwrap_or_default()), k),
            _ => faint("image: reading…".into(), k),
        },
        // a PDF (the owner's walk item 22): its page as coordd drew it, the page's width at most the pane's, with
        // Previous and Next and where it is; a press on the page opens it full size, as a picture's does
        Body::Pdf { pages: 0, .. } => faint("PDF: drawing its page…".into(), k),
        Body::Pdf { page, pages } => {
            let (page, pages) = (*page, *pages);
            let turn = |label: &'static str, by: i32, off: bool| Button::new(label).variant(ButtonVariant::Ghost).disabled(off).on_press(move |s: &mut App| turn_pdf(s, by)).build(k).key(format!("files-pdf-{}", label.to_lowercase()));
            let bar = row().key("files-pdf-bar").gap(8.0).center_y().px(24.0).pt(12.0)
                .child(turn("Previous", -1, page <= 1))
                .child(text(format!("Page {page} of {pages}")).font(k.ty.small).color(k.c.text_muted).key("files-pdf-at"))
                .child(turn("Next", 1, page >= pages));
            let pic = match s.images.get(&image_key(&f.path)) {
                Some(crate::chat::Img::Ready(img)) if img.error().is_none() => {
                    let w = img.size().map_or(720.0, |[w, _]| (w as f32 / 2.0).min(760.0));
                    let open = image_key(&f.path);
                    col().padding(24.0).child(rune::el().key("files-pdf-page").w(w).radius(k.radius.sm).border(1.0, k.c.border_weak).pixels(img.pixels()).cursor(rune::Cursor::Pointer).on_press(move |s: &mut App| crate::viewer::show(s, open.clone())))
                }
                Some(crate::chat::Img::Ready(img)) => faint(format!("PDF: {}", img.error().map(|e| e.to_string()).unwrap_or_default()), k),
                _ => faint("PDF: drawing its page…".into(), k),
            };
            col().child(bar).child(pic)
        }
        // the PDF whole in the web view: every page scrolled through, zoomed, searched; it fills the pane
        Body::PdfView(v) => {
            let surface = rune::WebSurface::new("files-pdf", v.url.clone()).script(format!("document.documentElement.style.setProperty('--yaaihh-bg','{}');", k.c.bg_base.hex()));
            return pane.child(head).child(rune::el().key("files-pdf-view").w_full().grow(1.0).min_h(0.0).fill(k.c.bg_base).web_surface(surface));
        }
        Body::Binary => faint(format!("binary, {}", size(f.size)), k),
        Body::TooLarge => faint(format!("{}: too large to show here", size(f.size)), k),
        Body::Unread(e) => faint(format!("could not be read: {e}"), k),
    };
    let (id, at) = (kept_id(s), s.files.at.clone());
    pane.child(head).child(
        col()
            .key("files-file")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: false })
            .on_scroll_offset(move |pos: ScrollPos| _ = at.lock().unwrap().insert((id.clone(), "file"), pos.offset[1]))
            .child(body),
    )
}

/// The key of a Markdown file's text as rendered, where a passage is commented on.
pub(crate) const FILE_KEY: &str = "file-md";

/// What a comment on the file at PATH is on (ask.rs `Comment::source`).
pub(crate) fn file_source(path: &Path) -> String {
    format!("file:{}", path.display())
}

/// Who reads a comment on the file at PATH (consult-lead N-1335 p4, (C)): the agent whose folder holds it (the deepest
/// such folder; of two agents in one folder, the first), else the project's `answerer`; None when neither is there,
/// and then his comment names one with @NAME.
pub(crate) fn reader(p: &crate::Proj, path: &Path) -> Option<String> {
    let mut best: Option<(usize, &str)> = None;
    for a in &p.live.agents {
        let Some(c) = a.cwd.as_deref().filter(|c| !c.is_empty()).map(Path::new) else { continue };
        let depth = c.components().count();
        if path.starts_with(c) && best.is_none_or(|(n, _)| depth > n) {
            best = Some((depth, &a.name));
        }
    }
    best.map(|(_, n)| n.to_string()).or_else(|| p.live.agents.iter().find(|a| a.role == "answerer").map(|a| a.name.clone()))
}

fn kept_id(s: &App) -> String {
    s.proj().map(|p| p.id.clone()).unwrap_or_default()
}

fn faint(t: String, k: &Kit) -> El {
    col().padding(24.0).child(text(t).font(k.ty.body).color(k.c.text_muted).wrap())
}

/// Text T (N lines) as numbered fixed-width lines, the first LIMIT of them, then "N more lines".
fn lines(t: &str, n: usize, limit: usize, k: &Kit) -> El {
    let digits = n.max(1).to_string().len() as f32;
    let num_w = digits * 8.0 + 12.0;
    let mut c = col().w_full().py(8.0);
    for (i, l) in t.lines().take(limit).enumerate() {
        let l = l.replace('\t', "    ");
        c = c.child(
            row()
                .w_full()
                .px(8.0)
                .child(text((i + 1).to_string()).font(k.ty.mono).color(k.c.text_faint).w(num_w).min_w(num_w))
                .child(text(if l.is_empty() { " ".to_string() } else { l }).font(k.ty.mono).color(k.c.text_base).wrap_anywhere().grow(1.0).shrink()),
        );
    }
    if n > limit {
        let more = row()
            .w_full()
            .px(8.0 + num_w)
            .py(8.0)
            .child(text(format!("{} more lines", n - limit)).font(k.ty.small_medium).color(k.c.text_accent));
        c = c.child(k.interactive(more).on_press(|s: &mut App| {
            s.files.limit += LINES;
            s.files.rev += 1;
        }));
    }
    c
}

/// The scene `files`: a folder it makes (a README, src/main.rs, notes/ with two files, a picture) as the first
/// project's root, opened, with YAAIHH_FILE (a path in it; default README.md) shown, YAAIHH_SOURCE set: as source.
pub fn scene(s: &mut App) {
    crate::read_once(s, Sel::Files);
    let dir = std::env::temp_dir().join(format!("yaaihh-files-scene-{}", std::process::id())).join("demo");
    make_demo(&dir);
    let Some(p) = s.projects.first_mut() else { return };
    if !p.live.project.is_object() {
        p.live.project = serde_json::json!({});
    }
    let obj = p.live.project.as_object_mut().unwrap();
    let proj = obj.entry("project").or_insert_with(|| serde_json::json!({}));
    if !proj.is_object() {
        *proj = serde_json::json!({});
    }
    proj["files"] = serde_json::json!([dir.display().to_string()]);
    let id = p.id.clone();
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let shown = dir.join(std::env::var("YAAIHH_FILE").unwrap_or_else(|_| "README.md".into()));
    let mut kept = Kept { source: std::env::var("YAAIHH_SOURCE").is_ok(), shown: Some(shown.display().to_string()), ..Default::default() };
    kept.open.insert(dir.display().to_string());
    for d in shown.ancestors().skip(1).take_while(|a| a.starts_with(&dir)) {
        kept.open.insert(d.display().to_string());
    }
    kept.open.insert(dir.join("notes").display().to_string());
    s.files.kept.insert(id, kept);
    refresh(s, true);
}

/// A still's PDF on the stand-in (YAAIHH_PDF=PATH|PNG|PAGES, the devices scene): the Files page on PATH's folder as
/// coordd's root, PATH shown as page 1 of PAGES with PNG (a file of this machine) as coordd's drawing of it, all through
/// the answers' own path ([`replied`]); a stand-in serves no request of the page itself.
pub fn scene_pdf(s: &mut App) {
    use base64::Engine;
    let Ok(v) = std::env::var("YAAIHH_PDF") else { return };
    let mut it = v.splitn(3, '|');
    let (Some(path), Some(png), Some(pages)) = (it.next(), it.next(), it.next()) else { return };
    let Some(id) = s.proj().map(|p| p.id.clone()) else { return };
    let path = PathBuf::from(path);
    let Some(dir) = path.parent().map(Path::to_path_buf) else { return };
    let name = last_name(&dir);
    s.sel = Sel::Files;
    let roots = serde_json::json!([{"name": name, "path": wire(&dir), "here": true}]).to_string();
    replied(s, Asked::Roots(id.clone()), true, roots);
    let list = serde_json::json!([{"name": "figures", "dir": true}, {"name": last_name(&path), "dir": false}, {"name": "main.tex", "dir": false}]).to_string();
    replied(s, Asked::List(dir.clone()), true, list);
    let k = s.files.kept.entry(id).or_default();
    k.open.insert(dir.to_string_lossy().into_owned());
    k.shown = Some(path.to_string_lossy().into_owned());
    let data = std::fs::read(png).map(|b| base64::engine::general_purpose::STANDARD.encode(b)).unwrap_or_default();
    let drawn = serde_json::json!({"pages": pages.parse::<u32>().unwrap_or(1), "page": 1, "data": data}).to_string();
    replied(s, Asked::Pdf { path, size: 0, modified: None, page: 1 }, true, drawn);
    s.files.rev += 1;
}

fn make_demo(dir: &Path) {
    let _ = std::fs::create_dir_all(dir.join("src"));
    let _ = std::fs::create_dir_all(dir.join("notes"));
    let readme = "# Demo project\n\nA small folder the Files page shows, to see how each kind of file is drawn.\n\n\
        ## What is in it\n\n- `src/main.rs`: thirty lines of Rust\n- `notes/`: two notes\n- `logo.png`: a picture\n\n\
        ## Running it\n\n```sh\ncargo run --release\n```\n\nIts entry point is src/main.rs.\n";
    let _ = std::fs::write(dir.join("README.md"), readme);
    let mut main = String::from("//! A demo: counts the words of each line it reads.\n\nuse std::io::{self, BufRead};\n\n");
    main.push_str("/// Reads standard input to its end and prints each line's count,\n/// then the total.\nfn main() {\n    let stdin = io::stdin();\n    let mut total = 0;\n    for (n, line) in stdin.lock().lines().enumerate() {\n");
    main.push_str("        let line = line.expect(\"a line\");\n        let words = count(&line);\n        total += words;\n");
    main.push_str("        println!(\"{:>4}: {words}\", n + 1);\n    }\n    println!(\"total: {total}\");\n}\n\n");
    main.push_str("/// The words of LINE: runs of anything but white space.\nfn count(line: &str) -> usize {\n    line.split_whitespace().count()\n}\n\n");
    main.push_str("#[cfg(test)]\nmod tests {\n    #[test]\n    fn counts() {\n        assert_eq!(super::count(\"a b  c\"), 3);\n    }\n}\n");
    let _ = std::fs::write(dir.join("src/main.rs"), main);
    let _ = std::fs::write(dir.join("notes/ideas.md"), "# Ideas\n\n- count characters too\n");
    let _ = std::fs::write(dir.join("notes/todo.txt"), "write the tests\nread a file named on the command line\n");
    let _ = std::fs::write(dir.join("logo.png"), demo_png());
}

/// A 96×96 PNG: a blue square with a lighter disc, made here (no file of the repo needed).
/// The demo's picture, for the viewer's tests.
#[cfg(test)]
pub(crate) fn demo_png_bytes() -> Vec<u8> {
    demo_png()
}

fn demo_png() -> Vec<u8> {
    let (w, h) = (96u32, 96u32);
    let mut raw = Vec::with_capacity((w * 3 + 1) as usize * h as usize);
    for y in 0..h {
        raw.push(0);
        for x in 0..w {
            let (dx, dy) = (x as f32 - 48.0, y as f32 - 48.0);
            let px: [u8; 3] = if dx * dx + dy * dy < 30.0 * 30.0 { [140, 190, 255] } else { [40, 90, 200] };
            raw.extend_from_slice(&px);
        }
    }
    // zlib with stored (uncompressed) blocks
    let mut z = vec![0x78, 0x01];
    for (i, chunk) in raw.chunks(65535).enumerate() {
        let last = (i + 1) * 65535 >= raw.len();
        z.push(last as u8);
        let n = chunk.len() as u16;
        z.extend_from_slice(&n.to_le_bytes());
        z.extend_from_slice(&(!n).to_le_bytes());
        z.extend_from_slice(chunk);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut chunk = |kind: &[u8], data: &[u8]| {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut c = kind.to_vec();
        c.extend_from_slice(data);
        png.extend_from_slice(&c);
        png.extend_from_slice(&crc(&c).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    png
}

fn crc(data: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

#[cfg(test)]
mod remote_tests {
    use super::*;
    use base64::Engine;

    fn piece(size: u64, data: &[u8]) -> String {
        serde_json::json!({"size": size, "modified": 1_763_460_000_000u64, "data": base64::engine::general_purpose::STANDARD.encode(data)}).to_string()
    }

    /// On Windows the page is coordd's answers: its roots, a folder (an entry it would not open left out), and a file
    /// in pieces, put together and shown as the disk's read would show it; a refusal shows why.
    #[test]
    fn on_windows_the_files_page_draws_coordds_answers() {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "w".into(), id: "w".into(), boxed: false }, Vec::new());
        p.protocol = FILES;
        // git's states asked only of a coordd that knows them
        assert!(matches!(list_request(&p, Path::new("/x")), coord_types::Request::ListDir { git: false, .. }));
        p.protocol = coord_types::FILE_STATES_PROTOCOL;
        assert!(matches!(list_request(&p, Path::new("/x")), coord_types::Request::ListDir { git: true, .. }));
        p.protocol = FILES;
        s.projects = vec![p];
        s.cur = 0;
        crate::elsewhere::as_on_windows(|| {
            replied(&mut s, Asked::Roots("w".into()), true, r#"[{"name":"flavour","path":"/home/t/flavour","here":true}]"#.into());
            assert_eq!(s.files.roots["w"], [Root { name: "flavour".into(), path: "/home/t/flavour".into(), here: true }]);
            let dir = PathBuf::from("/home/t/flavour");
            replied(&mut s, Asked::List(dir.clone()), true, r#"[{"name":"src","dir":true},{"name":"keys","dir":true,"outside":true},{"name":"a.md","dir":false}]"#.into());
            assert_eq!(s.files.lists[&dir], Ok(vec![("src".to_string(), true, Git::Plain), ("a.md".to_string(), false, Git::Plain)]));
            // what git says, as coordd serves it (N-608): generated after the others, an unknown word as none
            replied(
                &mut s,
                Asked::List(dir.clone()),
                true,
                r#"[{"name":"target","dir":true,"git":"ignored"},{"name":"src","dir":true},{"name":"b.md","dir":false,"git":"new"},{"name":"a.md","dir":false,"git":"later"}]"#.into(),
            );
            assert_eq!(
                s.files.lists[&dir],
                Ok(vec![("src".to_string(), true, Git::Plain), ("a.md".to_string(), false, Git::Plain), ("b.md".to_string(), false, Git::New), ("target".to_string(), true, Git::Generated)])
            );
            assert_eq!(git_of(&s, &dir.join("target/debug/x")), Git::Generated);
            assert_eq!(git_of(&s, &dir.join("b.md")), Git::New);
            assert_eq!(child(&dir, "a.md"), PathBuf::from("/home/t/flavour/a.md"));
            assert_eq!(wire(&child(&dir, "a.md")), "/home/t/flavour/a.md");
            // a file in two pieces
            let path = child(&dir, "notes.txt");
            let text = b"one\ntwo\nthree\n";
            replied(&mut s, Asked::Piece { path: path.clone(), size: text.len() as u64, modified: None, from: 0 }, true, piece(text.len() as u64, &text[..6]));
            assert!(s.files.file.is_none(), "not shown until whole");
            replied(&mut s, Asked::Piece { path: path.clone(), size: text.len() as u64, modified: None, from: 6 }, true, piece(text.len() as u64, &text[6..]));
            assert!(matches!(&s.files.file, Some(Read { body: Body::Text(t, 3), .. }) if t == "one\ntwo\nthree\n"));
            // refused: why, in the pane
            replied(&mut s, Asked::Stat(child(&dir, ".git/config")), false, "coord w: never served: a .git part".into());
            assert!(matches!(&s.files.file, Some(Read { body: Body::Unread(e), .. }) if e.contains("never served")));
        });
    }

    /// A PDF (the owner's walk item 22), from a coordd that draws its pages (HOME_PROTOCOL): its stat asks page 1 as
    /// `Request::PdfPage`, whatever its size; the page comes as a picture with where it is; Next asks the next page and
    /// none past the last; below the protocol it is a file as any other.
    #[test]
    fn a_pdf_is_shown_page_by_page_as_coordd_draws_it() {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "w".into(), id: "w".into(), boxed: false }, Vec::new());
        p.protocol = coord_types::HOME_PROTOCOL;
        let out = std::env::temp_dir().join(format!("yaaihh-files-pdf-{}.jsonl", std::process::id()));
        let mut child_ = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        p.link = Some(crate::link::Link::to_child(child_.stdin.take().unwrap()));
        s.projects = vec![p];
        s.cur = 0;
        let path = PathBuf::from("/srv/paper/main.pdf");
        crate::elsewhere::as_on_windows(|| {
            replied(&mut s, Asked::Stat(path.clone()), true, piece(40 << 20, b""));
            assert!(s.files.asked.contains("pdf:/srv/paper/main.pdf"), "page 1 asked, though over what a file may be: {:?}", s.files.asked);
            let page = |n: u32| serde_json::json!({"pages": 2, "page": n, "data": base64::engine::general_purpose::STANDARD.encode(demo_png_bytes())}).to_string();
            replied(&mut s, Asked::Pdf { path: path.clone(), size: 9 << 20, modified: None, page: 1 }, true, page(1));
            assert!(matches!(&s.files.file, Some(Read { body: Body::Pdf { page: 1, pages: 2 }, .. })));
            assert!(s.images.contains_key(&image_key(&path)), "the page is a picture of the window's");
            turn_pdf(&mut s, -1);
            assert!(!s.files.asked.contains("pdf:/srv/paper/main.pdf"), "no page before the first");
            turn_pdf(&mut s, 1);
            replied(&mut s, Asked::Pdf { path: path.clone(), size: 9 << 20, modified: None, page: 2 }, true, page(2));
            assert!(matches!(&s.files.file, Some(Read { body: Body::Pdf { page: 2, pages: 2 }, .. })));
            turn_pdf(&mut s, 1);
            assert!(!s.files.asked.contains("pdf:/srv/paper/main.pdf"), "none past the last");
            // full size in the viewer: left turns back a page, the viewer staying on it
            s.viewing = Some(image_key(&path));
            assert!(viewing_pdf(&s));
            assert!(crate::viewer::key(&mut s, &rune::KeyEvent { key: rune::Key::Left, mods: Default::default() }));
            assert!(s.files.asked.contains("pdf:/srv/paper/main.pdf") && s.viewing.is_some(), "page 1 asked from the viewer");
            replied(&mut s, Asked::Pdf { path: path.clone(), size: 40 << 20, modified: None, page: 1 }, true, page(1));
            assert!(matches!(&s.files.file, Some(Read { body: Body::Pdf { page: 1, pages: 2 }, .. })) && viewing_pdf(&s));
            s.viewing = None;
            // a refusal: why, in the pane
            replied(&mut s, Asked::Pdf { path: path.clone(), size: 9 << 20, modified: None, page: 1 }, false, "/srv/paper/main.pdf: pdftoppm did not draw it (Syntax Error)".into());
            assert!(matches!(&s.files.file, Some(Read { body: Body::Unread(e), .. }) if e.contains("did not draw it")));
            // an older coordd: a file too large, as before
            s.files.file = None;
            s.projects[0].protocol = coord_types::HOME_PROTOCOL - 1;
            replied(&mut s, Asked::Stat(path.clone()), true, piece(40 << 20, b""));
            assert!(matches!(&s.files.file, Some(Read { body: Body::TooLarge, .. })));
        });
        s.projects[0].link = None;
        drop(child_.stdin.take());
        let _ = child_.wait();
        let sent: Vec<serde_json::Value> = std::fs::read_to_string(&out).unwrap().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        let _ = std::fs::remove_file(&out);
        let pages: Vec<u64> = sent.iter().filter_map(|v| v.get("req")).filter(|r| r["op"] == "pdfpage").map(|r| r["page"].as_u64().unwrap()).collect();
        assert_eq!(pages, vec![1, 2, 1], "page 1 on the stat, 2 on Next, 1 on Left in the viewer: {sent:?}");
    }

    /// A PDF in the web view (post-v041-smooth-pdf-viewer): its stat reads it whole from coordd, not page by page, and
    /// serves it on loopback; a new change time reads it again and moves its version (what the page's reload waits
    /// for) at the same address; bytes pdflatex is still writing leave the view as it was and are read again at the
    /// next look; a PDF past what a file may be is drawn by coordd page by page, as before.
    #[test]
    fn a_pdf_in_the_web_view_is_served_whole_and_reloads_when_it_changes() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.files.web_pdf = true;
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "w".into(), id: "w".into(), boxed: false }, Vec::new());
        p.protocol = coord_types::HOME_PROTOCOL;
        let out = std::env::temp_dir().join(format!("yaaihh-files-pdfview-{}.jsonl", std::process::id()));
        let mut child_ = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        p.link = Some(crate::link::Link::to_child(child_.stdin.take().unwrap()));
        s.projects = vec![p];
        s.cur = 0;
        let path = PathBuf::from("/srv/paper/main.pdf");
        let at = |ms: u64, size: usize, data: &[u8]| serde_json::json!({"size": size, "modified": ms, "data": base64::engine::general_purpose::STANDARD.encode(data)}).to_string();
        let when = |ms: u64| Some(SystemTime::UNIX_EPOCH + Duration::from_millis(ms));
        let version = |v: &crate::pdf_view::Served| {
            let tok = v.url.rsplit('/').next().unwrap();
            let port = v.origin.rsplit(':').next().unwrap();
            let mut c = std::net::TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
            std::io::Write::write_all(&mut c, format!("GET /version/{tok} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n").as_bytes()).unwrap();
            let mut got = String::new();
            std::io::Read::read_to_string(&mut c, &mut got).unwrap();
            got.rsplit("\r\n\r\n").next().unwrap().to_string()
        };
        let one: &[u8] = b"%PDF-1.7\npage one\n%%EOF\n";
        let two: &[u8] = b"%PDF-1.7\npage one, rebuilt\n%%EOF\n";
        crate::elsewhere::as_on_windows(|| {
            replied(&mut s, Asked::Stat(path.clone()), true, at(1_000, one.len(), b""));
            assert!(s.files.asked.contains("read:/srv/paper/main.pdf") && !s.files.asked.contains("pdf:/srv/paper/main.pdf"), "read whole: {:?}", s.files.asked);
            replied(&mut s, Asked::Piece { path: path.clone(), size: one.len() as u64, modified: when(1_000), from: 0 }, true, at(1_000, one.len(), one));
            let Some(Read { body: Body::PdfView(first), .. }) = &s.files.file else { panic!("not in the web view") };
            let first = first.clone();
            assert!(rune::web_surface::loopback(&first.url) && first.url.contains("/view/"));
            assert_eq!(version(&first), "1");
            assert!(viewing_pdf_whole(&s), "looked at each second while shown");
            // pdflatex starts writing: a half file, not shown, and the view stays as it was
            let half: &[u8] = b"%PDF-1.7\npage o";
            replied(&mut s, Asked::Stat(path.clone()), true, at(2_000, half.len(), b""));
            replied(&mut s, Asked::Piece { path: path.clone(), size: half.len() as u64, modified: when(2_000), from: 0 }, true, at(2_000, half.len(), half));
            assert!(matches!(&s.files.file, Some(Read { body: Body::PdfView(v), modified: None, .. }) if *v == first), "the old pages stand");
            assert_eq!(version(&first), "1", "no reload for a half file");
            // the next look: the file whole again, read, its version moved at the same address
            replied(&mut s, Asked::Stat(path.clone()), true, at(3_000, two.len(), b""));
            assert!(s.files.asked.contains("read:/srv/paper/main.pdf"), "read again after the half file");
            replied(&mut s, Asked::Piece { path: path.clone(), size: two.len() as u64, modified: when(3_000), from: 0 }, true, at(3_000, two.len(), two));
            let Some(Read { body: Body::PdfView(next), .. }) = &s.files.file else { panic!("not in the web view") };
            assert_eq!((next.url.as_str(), next.version), (first.url.as_str(), 2));
            assert_eq!(version(&first), "2", "the page's reload sees it");
            // the same stat again: nothing read
            s.files.asked.clear();
            replied(&mut s, Asked::Stat(path.clone()), true, at(3_000, two.len(), b""));
            assert!(s.files.asked.is_empty(), "unchanged: nothing asked");
            // past what a file may be: coordd's pages, as before
            replied(&mut s, Asked::Stat(path.clone()), true, at(4_000, 40 << 20, b""));
            assert!(s.files.asked.contains("pdf:/srv/paper/main.pdf"));
        });
        s.projects[0].link = None;
        drop(child_.stdin.take());
        let _ = child_.wait();
        let _ = std::fs::remove_file(&out);
    }

    /// While a PDF shows in the web view, between the ticks only its stat is asked (no roots, folders or git), and only
    /// once a second; its pieces are put together only from one read of it: a stat while they come asks nothing, a
    /// piece out of order or of another file is dropped, and a file changed between its pieces is not shown and is
    /// read again at the next look.
    #[test]
    fn a_shown_pdf_alone_is_looked_at_and_its_pieces_come_from_one_read() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.files.web_pdf = true;
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "w".into(), id: "w".into(), boxed: false }, Vec::new());
        p.protocol = coord_types::HOME_PROTOCOL;
        let out = std::env::temp_dir().join(format!("yaaihh-files-pdfpieces-{}.jsonl", std::process::id()));
        let mut child_ = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        p.link = Some(crate::link::Link::to_child(child_.stdin.take().unwrap()));
        s.projects = vec![p];
        s.cur = 0;
        let path = PathBuf::from("/srv/paper/main.pdf");
        s.files.kept.entry("w".into()).or_default().shown = Some(path.display().to_string());
        let at = |ms: u64, size: usize, data: &[u8]| serde_json::json!({"size": size, "modified": ms, "data": base64::engine::general_purpose::STANDARD.encode(data)}).to_string();
        let when = |ms: u64| Some(SystemTime::UNIX_EPOCH + Duration::from_millis(ms));
        let one: &[u8] = b"%PDF-1.7\npage one\n%%EOF\n";
        let piece = |from: u64| Asked::Piece { path: path.clone(), size: one.len() as u64, modified: when(1_000), from };
        crate::elsewhere::as_on_windows(|| {
            // a file in two pieces, with a stat and a stray piece of another file between them
            replied(&mut s, Asked::Stat(path.clone()), true, at(1_000, one.len(), b""));
            replied(&mut s, piece(0), true, at(1_000, one.len(), &one[..9]));
            s.files.asked.clear();
            s.files.asked.insert("read:/srv/paper/main.pdf".into());
            replied(&mut s, Asked::Stat(path.clone()), true, at(1_000, one.len(), b""));
            assert_eq!(s.files.asked.len(), 1, "a stat while its pieces come reads nothing more: {:?}", s.files.asked);
            let other = PathBuf::from("/srv/other.pdf");
            replied(&mut s, Asked::Piece { path: other.clone(), size: 30, modified: when(5), from: 9 }, true, at(5, 30, b"stitched in"));
            replied(&mut s, piece(4), true, at(1_000, one.len(), b"out of order"));
            replied(&mut s, piece(9), true, at(1_000, one.len(), &one[9..]));
            let Some(Read { body: Body::PdfView(first), .. }) = &s.files.file else { panic!("not in the web view: {:?}", s.files.file.as_ref().map(|f| &f.path)) };
            let first = first.clone();
            // the same bytes came, and nothing else
            let tok = first.url.rsplit('/').next().unwrap();
            let port = first.origin.rsplit(':').next().unwrap();
            let mut c = std::net::TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
            std::io::Write::write_all(&mut c, format!("GET /pdf/{tok} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n").as_bytes()).unwrap();
            let mut got = Vec::new();
            std::io::Read::read_to_end(&mut c, &mut got).unwrap();
            assert!(got.ends_with(one));
            // rebuilt between its pieces: nothing of it shown, read again at the next look
            let two: &[u8] = b"%PDF-1.7\npage one, rebuilt\n%%EOF\n";
            replied(&mut s, Asked::Stat(path.clone()), true, at(2_000, two.len(), b""));
            let p2 = |from: u64| Asked::Piece { path: path.clone(), size: two.len() as u64, modified: when(2_000), from };
            replied(&mut s, p2(0), true, at(2_000, two.len(), &two[..9]));
            replied(&mut s, p2(9), true, at(3_000, 40, b"a newer file's end"));
            assert!(matches!(&s.files.file, Some(Read { body: Body::PdfView(v), modified: None, .. }) if *v == first), "the old pages stand");
            // between the ticks: only its stat
            s.files.checked = Some((Instant::now(), "w".into()));
            s.files.pdf_checked = None;
            s.files.asked.clear();
            refresh(&mut s, false);
            assert_eq!(s.files.asked.iter().collect::<Vec<_>>(), ["stat:/srv/paper/main.pdf"], "no roots, folders or git");
            s.files.asked.clear();
            refresh(&mut s, false);
            assert!(s.files.asked.is_empty(), "once a second: {:?}", s.files.asked);
        });
        s.projects[0].link = None;
        drop(child_.stdin.take());
        let _ = child_.wait();
        let _ = std::fs::remove_file(&out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_lists_folders_first_by_name_with_case_aside_and_no_git() {
        let d = std::env::temp_dir().join(format!("yaaihh-files-list-{}", std::process::id()));
        for sub in [".git", "b", "A"] {
            std::fs::create_dir_all(d.join(sub)).unwrap();
        }
        for f in ["c.txt", "B.md", "a.rs"] {
            std::fs::write(d.join(f), "x").unwrap();
        }
        let names: Vec<String> = list(&d).unwrap().into_iter().map(|(n, _, _)| n).collect();
        assert_eq!(names, ["A", "b", "a.rs", "B.md", "c.txt"]);
        std::fs::remove_dir_all(&d).ok();
    }

    /// Git's three states, from git itself: tracked plain, untracked `new`, ignored generated and listed last; a folder
    /// git reports whole carries its state, a mixed one none; outside a repository nothing is marked.
    #[test]
    fn a_folder_in_git_sets_generated_entries_apart_and_marks_new_ones() {
        let d = std::env::temp_dir().join(format!("yaaihh-files-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let git = |a: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(&d).args(a).output().unwrap().status.success(), "{a:?}");
        git(&["init", "-q"]);
        for f in ["target/debug/app", "src/main.rs", "src/scratch.rs", "notes/today.md", "a.rs", "z.log", "b.rs"] {
            std::fs::create_dir_all(d.join(f).parent().unwrap()).unwrap();
            std::fs::write(d.join(f), "x").unwrap();
        }
        std::fs::write(d.join(".gitignore"), "target/\n*.log\n").unwrap();
        git(&["add", ".gitignore", "a.rs", "src/main.rs"]);
        let l = list(&d).unwrap();
        let st = |n: &str| l.iter().find(|e| e.0 == n).map(|e| e.2);
        assert_eq!(st("a.rs"), Some(Git::Plain));
        assert_eq!(st("b.rs"), Some(Git::New));
        assert_eq!(st("notes"), Some(Git::New), "untracked as a whole");
        assert_eq!(st("src"), Some(Git::Plain), "mixed: nothing");
        assert_eq!(st("target"), Some(Git::Generated));
        assert_eq!(st("z.log"), Some(Git::Generated));
        let names: Vec<&str> = l.iter().map(|e| e.0.as_str()).collect();
        assert_eq!(names, ["notes", "src", ".gitignore", "a.rs", "b.rs", "target", "z.log"]);
        // inside src: the untracked file is new
        assert_eq!(list(&d.join("src")).unwrap().iter().find(|e| e.0 == "scratch.rs").map(|e| e.2), Some(Git::New));
        std::fs::remove_dir_all(&d).ok();
        // no repository: nothing marked
        let o = std::env::temp_dir().join(format!("yaaihh-files-nogit-{}", std::process::id()));
        std::fs::create_dir_all(&o).unwrap();
        std::fs::write(o.join("x.txt"), "x").unwrap();
        if !std::process::Command::new("git").arg("-C").arg(&o).args(["rev-parse"]).output().unwrap().status.success() {
            assert_eq!(list(&o).unwrap(), vec![("x.txt".to_string(), false, Git::Plain)]);
        }
        std::fs::remove_dir_all(&o).ok();
    }

    #[test]
    fn a_root_inside_an_earlier_one_is_left_out_and_a_box_path_is_not_here() {
        let d = std::env::temp_dir().join(format!("yaaihh-files-roots-{}", std::process::id()));
        std::fs::create_dir_all(d.join("inner")).unwrap();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "x".into(), id: format!("no-such-project-{}", std::process::id()), boxed: true }, Vec::new());
        p.live.project = serde_json::json!({"project": {"files": [d.display().to_string()]}});
        let agent = |cwd: &str| coord_types::AgentView { cwd: Some(cwd.into()), ..serde_json::from_value(serde_json::json!({"id": "AGT-001", "name": "a", "parent": "", "role": "", "kind": "", "model": "", "state": "", "since": "", "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null})).unwrap() };
        p.live.agents = vec![agent(&d.join("inner").display().to_string()), agent("/no/such/box/path")];
        let r = roots(&p);
        assert_eq!(r.len(), 2, "{r:?}");
        assert!(r[0].here && r[0].name.starts_with("yaaihh-files-roots-"));
        assert_eq!((r[1].name.as_str(), r[1].here), ("path", false));
        std::fs::remove_dir_all(&d).ok();
    }

    /// Two agents' `home` folders and two `flavor`s, one a repository's and one an agent's, each shown apart (N-1888).
    #[test]
    fn roots_of_one_name_are_told_apart_by_the_folder_above() {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "x".into(), id: format!("no-such-project-{}", std::process::id()), boxed: false }, Vec::new());
        p.live.project = serde_json::json!({"project": {"files": ["/no/such/qa/repo/flavor"]}});
        let agent = |cwd: &str| coord_types::AgentView { cwd: Some(cwd.into()), ..serde_json::from_value(serde_json::json!({"id": "AGT-001", "name": "a", "parent": "", "role": "", "kind": "", "model": "", "state": "", "since": "", "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null})).unwrap() };
        p.live.agents = ["/no/such/qa/agents/codex-pa/home", "/no/such/qa/agents/neon-pa/home", "/no/such/papers/soft-swarm/flavor", "/no/such/qa/work"].map(agent).to_vec();
        let names: Vec<String> = roots(&p).into_iter().map(|r| r.name).collect();
        assert_eq!(names, ["flavor (repo)", "home (codex-pa)", "home (neon-pa)", "flavor (soft-swarm)", "work"]);
        // coordd's roots (Windows) alike, and a name met once stays as it is
        let mut served = vec![Root { name: "home".into(), path: "/a/x/home".into(), here: true }, Root { name: "home".into(), path: "/b/x/home".into(), here: true }, Root { name: "notes".into(), path: "/n".into(), here: true }];
        distinct(&mut served);
        assert_eq!(served.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["home (a)", "home (b)", "notes"]);
    }

    #[test]
    fn the_demo_picture_is_a_png_the_decoder_reads() {
        let png = demo_png();
        assert_eq!(&png[1..4], b"PNG");
        let cache = rune::ImageCache::new(Arc::new(|| {}));
        assert_eq!(cache.load(png).wait().ok(), Some([96, 96]));
    }
}

#[cfg(test)]
mod comment_tests {
    //! A passage of a file commented on in the file view (0.2 line C15; consult-lead N-1335 p4, (C)): it goes to the
    //! agent whose folder holds the file, else the project's answerer; an @NAME in the comment sends it to that agent.
    use super::*;
    use crate::message_box::{COMMENT_PLACE, Target};
    use rune::TextEdit;
    use serde_json::{Value, json};

    const TEXT: &str = "# Plan\n\nThe lock waits 7 s on every start, then the window opens.\n";
    const PASSAGE: &str = "The lock waits 7 s";
    const PATH: &str = "/w/edge8/plan.md";

    fn agent(name: &str, role: &str, cwd: &str) -> coord_types::AgentView {
        let v = json!({"id": format!("AGT-{name}"), "name": name, "parent": "owner", "role": role, "kind": "claude", "model": "opus", "state": "idle", "since": "",
            "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null});
        coord_types::AgentView { cwd: Some(cwd.into()), ..serde_json::from_value(v).unwrap() }
    }

    /// Project research: docket-edge8 in /w/edge8, paper-research (the answerer) in /w/research.
    fn proj() -> crate::Proj {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        // a coordd whose Files is this page, not the code view
        p.protocol = coord_types::CODE_VIEW_PROTOCOL - 1;
        p.live.agents = vec![agent("docket-edge8", "", "/w/edge8"), agent("paper-research", "answerer", "/w/research")];
        p
    }

    /// The window on the Files page showing PATH as Markdown, its requests written to OUT.
    fn app(out: &std::path::Path) -> (std::sync::MutexGuard<'static, ()>, App, std::process::Child) {
        let (held, mut s) = crate::view_cost::test_app();
        s.projects = vec![proj()];
        s.cur = 0;
        s.sel = crate::Sel::Files;
        s.files.kept.entry("research".into()).or_default().shown = Some(PATH.into());
        s.files.file = Some(Read { path: PathBuf::from(PATH), size: TEXT.len() as u64, modified: None, body: Body::Markdown(TEXT.into()) });
        let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        (held, s, child)
    }

    fn requests(s: &mut App, child: std::process::Child, out: &std::path::Path) -> Vec<Value> {
        s.projects[0].link = None;
        let mut child = child;
        let _ = child.wait();
        std::fs::read_to_string(out).unwrap_or_default().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["req"]["op"] == "say" || v["req"]["op"] == "post").map(|v| v["req"].clone()).collect()
    }

    fn out(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("yaaihh-file-comments-{name}-{}.jsonl", std::process::id()))
    }

    fn settle(rt: &mut rune::runtime::Runtime<App>) {
        rt.invalidate();
        for _ in 0..3 {
            rt.frame(0.016);
        }
    }

    /// The words drawn in the comment box, in order.
    fn boxed(rt: &rune::runtime::Runtime<App>) -> Vec<String> {
        rt.elements().into_iter().filter(|e| !e.exiting && e.path.split('/').any(|k| k == "cm-box")).filter_map(|e| e.text.or(e.label)).collect()
    }

    fn write_note(s: &mut App, w: &str) {
        let mut e = TextEdit::new(w);
        e.focused = true;
        e.set_caret(w.len());
        s.composers.insert((0, COMMENT_PLACE.into()), e);
    }

    #[test]
    fn a_files_reader_is_the_agent_whose_folder_holds_it_else_the_answerer() {
        let mut p = proj();
        assert_eq!(reader(&p, Path::new("/w/edge8/docs/plan.md")).as_deref(), Some("docket-edge8"));
        assert_eq!(reader(&p, Path::new("/w/edge8b/plan.md")).as_deref(), Some("paper-research"), "a folder, not a prefix of its name");
        assert_eq!(reader(&p, Path::new("/elsewhere/x.md")).as_deref(), Some("paper-research"), "no agent's folder holds it: the answerer");
        p.live.agents.push(agent("docket-edge8-docs", "", "/w/edge8/docs"));
        assert_eq!(reader(&p, Path::new("/w/edge8/docs/plan.md")).as_deref(), Some("docket-edge8-docs"), "the deepest folder");
        p.live.agents.retain(|a| a.role != "answerer");
        assert_eq!(reader(&p, Path::new("/elsewhere/x.md")), None);
    }

    /// His comment on a passage of a file: its box says whom it goes to, and Add comment holds it for that agent's next
    /// message, nothing sent (the owner, N-1790 part 3): its passage stays lit, its box saying where it waits; another
    /// comment there with @paper-research is held for paper-research. Each agent's Send takes its own, quoting the
    /// passage.
    #[test]
    fn a_comment_on_a_files_passage_goes_to_its_reader_and_an_at_name_sends_it_elsewhere() {
        let file = out("reader");
        let (_held, s, child) = app(&file);
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        // the passage selected and Comment pressed, as comment_on_file leaves it
        let i = crate::paths::linkify(TEXT).find(PASSAGE).unwrap();
        rt.update(|s, _| {
            let who = reader(&s.projects[0], Path::new(PATH)).unwrap_or_default();
            let p = crate::ask::Picked { target: who, source: file_source(Path::new(PATH)), label: "edge8/plan.md".into(), quote: PASSAGE.into(), at: vec![i..i + PASSAGE.len()], pi: 0, anchor: None, block: None, pane: None, texts: Vec::new() };
            crate::ask::write(s, p, "");
        });
        settle(&mut rt);
        let t = boxed(&rt);
        assert!(t.iter().any(|x| x == "to docket-edge8"), "whom it goes to: {t:?}");
        let picked = rt.state.notes.picked.clone().expect("its box");
        rt.update(|s, _| {
            write_note(s, "why 7 s?");
            crate::message_box::send(s, &Target::Comment { project: 0 });
        });
        assert!(rt.state.notes.picked.is_none() && crate::ask::writing(&rt.state).is_none(), "the box closes at the press");
        assert!(rt.state.outgoing.list.is_empty(), "nothing goes at Add comment");
        let notes = |s: &App, who: &str| crate::ask::held(s, 0, who).iter().map(|c| c.note.clone()).collect::<Vec<_>>();
        assert_eq!(notes(&rt.state, "docket-edge8"), ["why 7 s?"], "held for the file's reader's next message");
        settle(&mut rt);
        let lit = crate::ask::marks(&rt.state, &rt.state.projects[0], &file_source(Path::new(PATH)), &crate::paths::linkify(TEXT), &rune_kit::Kit::neutral(false));
        assert_eq!(lit.len(), 1, "its passage stays lit while it is held");
        rt.update(|s, _| crate::ask::open_mark(s, "ch-file:/w/edge8/plan.md-0"));
        settle(&mut rt);
        assert!(boxed(&rt).iter().any(|x| x == "held · goes with your next to docket-edge8"), "its box says where it waits: {:?}", boxed(&rt));
        rt.update(|s, _| crate::ask::close_mark(s));
        rt.update(|s, _| {
            crate::ask::write(s, picked, "");
            write_note(s, "@paper-research is this in the record?");
        });
        settle(&mut rt);
        assert!(boxed(&rt).iter().any(|x| x == "to paper-research"), "@NAME changes it: {:?}", boxed(&rt));
        rt.update(|s, _| crate::message_box::send(s, &Target::Comment { project: 0 }));
        assert!(rt.state.outgoing.list.is_empty(), "nothing goes at Add comment");
        assert_eq!(notes(&rt.state, "paper-research"), ["@paper-research is this in the record?"], "held for paper-research");
        // each agent's Send takes its own
        rt.update(|s, _| {
            s.send(0, "docket-edge8", false);
            s.send(0, "paper-research", false);
        });
        assert!(notes(&rt.state, "docket-edge8").is_empty() && notes(&rt.state, "paper-research").is_empty(), "taken by their Sends");
        let sent = requests(&mut rt.state, child, &file);
        assert_eq!(sent.len(), 2, "{sent:?}");
        assert_eq!((sent[0]["to"].as_str(), sent[1]["to"].as_str()), (Some("docket-edge8"), Some("paper-research")));
        assert_eq!(sent[0]["text"], format!("> {PASSAGE}\n> (from edge8/plan.md)\n\nwhy 7 s?"));
        let text = sent[1]["text"].as_str().unwrap();
        assert!(text.starts_with(&format!("> {PASSAGE}\n> (from edge8/plan.md)\n\n@paper-research")), "{text}");
        let _ = std::fs::remove_file(&file);
    }

    /// No agent's folder holds the file and the project has no answerer: the box asks for an @NAME, and Add comment
    /// keeps the words in the box until one is tagged; then it holds them for that agent's next message.
    #[test]
    fn a_files_comment_with_no_reader_asks_for_an_at_name() {
        let file = out("none");
        let (_held, mut s, child) = app(&file);
        s.projects[0].live.agents.retain(|a| a.role != "answerer");
        let path = "/elsewhere/notes.md";
        s.files.kept.entry("research".into()).or_default().shown = Some(path.into());
        s.files.file = Some(Read { path: PathBuf::from(path), size: TEXT.len() as u64, modified: None, body: Body::Markdown(TEXT.into()) });
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        rt.update(|s, _| {
            assert_eq!(reader(&s.projects[0], Path::new(path)), None);
            let i = crate::paths::linkify(TEXT).find(PASSAGE).unwrap();
            let p = crate::ask::Picked { target: String::new(), source: file_source(Path::new(path)), label: "notes.md".into(), quote: PASSAGE.into(), at: vec![i..i + PASSAGE.len()], pi: 0, anchor: None, block: None, pane: None, texts: Vec::new() };
            crate::ask::write(s, p, "");
            write_note(s, "why 7 s?");
        });
        settle(&mut rt);
        assert!(boxed(&rt).iter().any(|x| x.ends_with("tag an @agent")), "{:?}", boxed(&rt));
        rt.update(|s, _| crate::message_box::send(s, &Target::Comment { project: 0 }));
        assert!(rt.state.outgoing.list.is_empty(), "nothing goes without a reader");
        assert!(crate::ask::writing(&rt.state).is_some_and(|w| w.text == "why 7 s?"), "his words stay in the box");
        rt.update(|s, _| {
            write_note(s, "@docket-edge8 why 7 s?");
            crate::message_box::send(s, &Target::Comment { project: 0 });
        });
        assert!(rt.state.outgoing.list.is_empty(), "nothing goes at Add comment");
        assert_eq!(crate::ask::held(&rt.state, 0, "docket-edge8").len(), 1, "tagged: held for docket-edge8");
        rt.update(|s, _| s.send(0, "docket-edge8", false));
        let sent = requests(&mut rt.state, child, &file);
        assert_eq!(sent.len(), 1, "{sent:?}");
        let _ = std::fs::remove_file(&file);
    }

    /// A passage selected in the middle of a long file and commented on (I-1476; his N-1104 p706), the box covering
    /// none of the file (paper-research, N-1477 p9): narrow, under the lines he highlighted, the words after them moved
    /// down under it; wide, beside them in the pane's right margin.
    #[test]
    fn a_files_comment_box_opens_where_he_highlighted_covering_none_of_it() {
        use crate::ask::anchor_tests::{NARROW, WIDE, beside, in_room, lit, select, settle, under_box, window};
        for size in [NARROW, WIDE] {
            let file = out("under");
            let (_held, mut s, child) = app(&file);
            let long: String = (0..12)
                .map(|i| match i {
                    5 => format!("Step {i}: {PASSAGE} on every start, then the window opens."),
                    _ => format!("Step {i}: the runner reads the record again before the window opens."),
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            s.files.file = Some(Read { path: PathBuf::from(PATH), size: long.len() as u64, modified: None, body: Body::Markdown(long) });
            let mut rt = window(s, size);
            let sel = select(&mut rt, FILE_KEY, PASSAGE, "").expect("the passage drawn");
            rt.update(move |s, _| crate::ask::comment_on_file(s, 0, &file_source(Path::new(PATH)), "edge8/plan.md", Some("docket-edge8"), &sel));
            settle(&mut rt);
            let drawn = |key: &str| rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).map(|e| e.rect);
            let why = format!("{size:?}: box {:?}, room {:?}, lit {:?}, pane {:?}, under {:?}, anchor {:?}", drawn("cm-box"), drawn(crate::ask::ROOM), lit(&rt), drawn("files-file"), under_box(&rt), rt.state.notes.picked.as_ref().map(|p| &p.anchor));
            if size == NARROW {
                in_room(&rt, FILE_KEY, "on every start, then", &why);
            } else {
                beside(&rt, FILE_KEY, "files-file", &why);
            }
            let _ = requests(&mut rt.state, child, &file);
            let _ = std::fs::remove_file(&file);
        }
    }
}
