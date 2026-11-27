//! Editing a kind, or a call's template, from the app (N-117 post 25; the owner's corrections, N-117.p35).
//!
//! Permanently: a kind on the Kinds page ("Edit kind"); in place in a program's picture (paper-research N-1396 p294),
//! the lines a press lit in its kind's text (a case, a `keep` or `report` reason), a clock's line in its file, or a step's call:
//! its lines after its signature (`task`, `model`, its template), a watcher's step with no template the call coordd
//! runs for it. The edit is checked as coordd checks the project at its start: the kinds and calls of agent-base, the
//! project's library and the project parsed with docket-machine's own parser and built with `Defs::build_with`; only
//! an error the edit adds counts (one the files had before is not the edit's). A template's slots must be the call's
//! fields. While it checks, once he pauses, coordd is asked what it would reach (`Request::Reach`, the reading and the
//! refusals a reload makes, nothing changed): each running ticket it reaches, is given its step now, is past it, or
//! keeps its old text, in coordd's words. Save writes the project's file and commits it there when it is in a git
//! repository; a kind or a call of agent-base's or the library's is written into the project's kinds or calls as a
//! copy, which replaces it by name. Then one line says what the edit applies to: a kind to the tickets opened from now
//! on and to what the open ones have not run yet, a call to the steps sent from now on, in running tickets too (a
//! prompt is made when its step is sent). After the commit coordd is asked to read the kinds and calls again
//! (`Request::Reload`, docket-core's (ii)); its answer follows the line, a refusal in red with coordd's reasons, and
//! under it each open ticket of an edited kind on its own line: took the new text, or kept its old text and why. The
//! picture marks what is being edited, and when each kind and call was last edited (`Marks`).
//!
//! On a ticket's step (its Thread scale, its whole program): one Save, for the program (the default: the call's lines,
//! as above) or only this thread (p296): its prompt for the ticket and the tickets under it, sent as
//! `Request::Template` (docket-core's (i)), which coordd checks in the call's slots and writes as a `template` line of
//! the record. An inline prompt (`step a "…"`) has no call: it is the kind's text, edited there. What follows the
//! step, for this ticket only, is the existing Edit (coord edit), moved here beside the step it follows.

use crate::App;
use docket_machine::syntax;
use rune::{El, TextEdit, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit, RadioItem, TextArea, TextInput};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// What is being edited.
#[derive(Default)]
pub struct State {
    pub kind: Option<KindEdit>,
    /// The call whose template shows (a step of a picture clicked), and its edit when one is under way.
    pub call: Option<String>,
    pub call_edit: Option<CallEdit>,
    /// A ticket page's step clicked in its picture.
    pub step: Option<Step>,
    /// On a ticket's step, what Save writes (p296): false (the default), the program's text, the call's lines; true,
    /// its prompt for only this thread, the ticket and the tickets under it (`Template` with scope `tree`).
    pub only_thread: bool,
    pub once: TextEdit,
    /// Each call as read for a step's prompt, by project id and name: its template and fields (None: not found). The
    /// Map draws steps of every project's island, each with its own project's calls (consult-3 N-1479 p27).
    once_of: Mutex<std::collections::HashMap<(String, String), Option<(String, Vec<String>)>>>,
    /// The why a Save commits with.
    pub why: TextEdit,
    /// What the last Save or Edit said (Err: refused or failed).
    pub said: Option<Result<String, String>>,
    /// The last reload's reply, ticket by ticket (live edits), with when it came: kept while the window lives, for the
    /// Save's line and each ticket's page.
    pub reloaded: Option<(String, ReloadReply)>,
    /// On Windows, the kinds and calls files as the project's coordd served them (`Sources`), by project id; and
    /// whether they came for the edit being started (N-744).
    pub served: Option<(String, Served)>,
    fresh: bool,
    /// What coordd answered a Reach of the file the edit under way would write (p294): that file's text, and the
    /// answer (Err: why coordd would refuse it, in its words). Dropped when the edit closes.
    pub reach: Option<(String, Result<ReloadReply, String>)>,
    /// That file's text as it last changed, and when: its Reach is asked once he pauses ([`REACH_PAUSE`]); and the end
    /// of the pause a wake was asked for (find.rs's `paused`).
    typing: (String, Option<std::time::Instant>),
    woken: Option<std::time::Instant>,
    /// "Show it as it would be sent" pressed under the prompt being edited (H2, his N-216.p1): coordd's dry run of it
    /// is asked once he pauses (`Request::DryRun`, [`dry_poll`]). Off again when no prompt is being edited.
    pub dry_open: bool,
    /// The last dry run's ask and coordd's answer (Err: why it would not be sent, in its words). Its text is drawn
    /// under the editor only, behind his press (p1380), and dropped when he hides it or the edit ends.
    pub dry: Option<(DryAsk, Result<coord_types::DryRunAnswer, String>)>,
    /// The ask as it last changed, and when; and the end of the pause a wake was asked for, as `typing` and `woken`.
    dry_typing: (Option<DryAsk>, Option<std::time::Instant>),
    dry_woken: Option<std::time::Instant>,
    /// Lines of a kind to edit in place once its files have come (Windows): the kind, its text as coordd read it, the
    /// lines ([`start_kind_lines`]).
    lines_wanted: Option<(String, String, (usize, usize))>,
    /// A clock to edit in place once its files have come (Windows): the kind it opens, its file, line and binding as
    /// coordd read them ([`start_binding`]).
    binding_wanted: Option<(String, String, usize, String)>,
    /// The window's look, edited where the kinds are (look.rs; N-866.p6, 2).
    pub look: crate::look::Edit,
}

/// The kinds and calls files coordd reads, with their texts, as `Sources` answers: on Windows the edit's files.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
pub struct Served {
    #[serde(default)]
    pub kinds: Vec<FileText>,
    #[serde(default)]
    pub calls: Vec<FileText>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct FileText {
    pub path: String,
    pub text: String,
}

/// Where an edit's files are read: in folders on this machine (agent-base, the library, the project), or as the
/// project's coordd served them (Windows: the WSL machine's files).
#[derive(Clone, Debug)]
pub enum Src {
    Here(Vec<PathBuf>),
    There(Served),
}

impl Src {
    fn kinds(&self) -> Vec<(String, String)> {
        match self {
            Src::Here(f) => f.iter().flat_map(|d| files(&place(d, "kinds"), "kind")).collect(),
            Src::There(s) => s.kinds.iter().map(|f| (f.path.clone(), f.text.clone())).collect(),
        }
    }

    fn calls(&self) -> Vec<(String, String)> {
        match self {
            Src::Here(f) => f.iter().flat_map(|d| files(&place(d, "calls"), "call")).collect(),
            Src::There(s) => s.calls.iter().map(|f| (f.path.clone(), f.text.clone())).collect(),
        }
    }

    /// FILE's text, as read now (here) or as served.
    fn text_of(&self, file: &Path) -> Option<String> {
        match self {
            Src::Here(_) => std::fs::read_to_string(file).ok(),
            Src::There(s) => s.kinds.iter().chain(&s.calls).find(|f| Path::new(&f.path) == file).map(|f| f.text.clone()),
        }
    }
}

/// What the edit asked the project's coordd (Windows): the files, to start editing kind or call NAME; or a Save.
pub enum Asked {
    Sources { kind: bool, name: String },
    /// The files, for the prompts the Map draws on the current project's island (map/deep.rs `near`): kept as an
    /// edit's are, and no edit starts.
    Prompts,
    /// A Save: COPY, what says it is the project's new copy of agent-base's or the library's, and where (else empty).
    Saved { kind: bool, name: String, why: String, copy: String },
    /// What the file TEXT, written, would reach (`Request::Reach`).
    Reach { text: String },
    /// The prompt being edited, as it would be sent (`Request::DryRun`).
    DryRun { ask: DryAsk },
}

/// What a dry run asks of coordd (`Request::DryRun`): the call whose prompt is edited, the ticket the editor came from,
/// and the draft: the file Save would write and its text, or with no PATH his prompt for that ticket's thread alone.
#[derive(Clone, Debug, PartialEq)]
pub struct DryAsk {
    pub call: String,
    pub on: Option<String>,
    pub path: Option<String>,
    pub text: String,
}

/// coordd's reply to a reload under live edits (docket-edge's shape, N-235): the kinds and calls whose text changed,
/// and each open ticket of an edited kind: whether it took the new text (SAID, the machine's words), or kept its old
/// one (WHY); the steps in flight it withdrew.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
pub struct ReloadReply {
    #[serde(default)]
    pub kinds: Vec<String>,
    #[serde(default)]
    pub calls: Vec<String>,
    #[serde(default)]
    pub tickets: Vec<Reloaded>,
    /// A Reach's: the clocks the edit changes, each with its next firing as coordd would have it.
    #[serde(default)]
    pub clocks: Vec<ClockNext>,
}

/// A clock an edit changes, by its binding as written, and when it would fire next.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct ClockNext {
    pub binding: String,
    pub next: String,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct Reloaded {
    pub id: String,
    #[serde(default)]
    pub kind: String,
    /// Whether it took the new text of its kind; a Reach's ticket reached only through a call has none (false).
    #[serde(default)]
    pub took: bool,
    #[serde(default)]
    pub said: Option<String>,
    #[serde(default)]
    pub why: Option<String>,
    #[serde(default)]
    pub withdrawn: Vec<Withdrawn>,
    /// A Reach's: where the edit finds the ticket, docket-machine's reach (docket-edge5, N-1396 p307): `ahead` (what it
    /// has not run holds what is edited), `given` (its step is given now, with the prompt it was sent), `own` (his own
    /// prompt stands: only this thread's, or edited at its halt), `past`, `withdrawn` (what it stands at is gone), or
    /// `kept` (its old text, WHY). SAID is coordd's words for it.
    #[serde(default)]
    pub reach: Option<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct Withdrawn {
    pub step: u32,
    #[serde(default)]
    pub call: String,
    #[serde(default)]
    pub worker: String,
}

/// A ticket's line of a reload's reply: "took the new text; withdrawn: step 4 (assign_parts, to orch)" or "kept its old
/// text: WHY".
pub fn reloaded_words(r: &Reloaded) -> String {
    if !r.took {
        return format!("kept its old text: {}", r.why.clone().unwrap_or_else(|| "no reason given".into()));
    }
    let mut w = String::from("took the new text");
    if !r.withdrawn.is_empty() {
        let steps: Vec<String> = r.withdrawn.iter().map(|x| format!("step {} ({}, to {})", x.step, x.call, x.worker)).collect();
        w.push_str(&format!("; withdrawn, no place in the new text: {}", steps.join(", ")));
    }
    w
}

/// The reply's first line: what was read again, and how many tickets took it and kept their own.
fn reload_summary(r: &ReloadReply) -> String {
    let mut parts = Vec::new();
    if !r.kinds.is_empty() {
        parts.push(format!("kinds {}", r.kinds.join(", ")));
    }
    if !r.calls.is_empty() {
        parts.push(format!("calls {}", r.calls.join(", ")));
    }
    let changed = if parts.is_empty() { "nothing changed".to_string() } else { parts.join("; ") };
    let took = r.tickets.iter().filter(|t| t.took).count();
    let kept = r.tickets.len() - took;
    let tickets = match (took, kept) {
        (0, 0) => "no open ticket runs an edited kind".to_string(),
        _ => format!("{took} open took it, {kept} kept the old"),
    };
    format!("{changed}; {tickets}")
}

pub struct KindEdit {
    pub kind: String,
    /// Where Save writes it: where it is written, or for a kind of agent-base's or the library's the project's own copy,
    /// which replaces it by name (NEW: no file there yet).
    pub file: PathBuf,
    /// The files the check reads, as coordd does: agent-base's, the library's, the project's (or a draft's folder).
    pub src: Src,
    /// The file as it was read (a new copy: the kind's lines as written where it is), and the bytes the edit replaces:
    /// the kind, or the LINES of it a press lit.
    pub orig: String,
    pub span: (usize, usize),
    pub new: bool,
    /// The lines of the kind's text edited in place (1-based, first and last; picture.rs `lit_lines`): none, all of it.
    pub lines: Option<(usize, usize)>,
    /// A NEW copy of agent-base's or the library's: what it means, said in the editor and by Save ([`copy_words`]).
    pub copy: Option<String>,
    /// A clock's binding, as coordd read it, whose line of FILE is edited in place ([`start_binding`]): its kind's text is
    /// not edited. None: a kind's.
    pub binding: Option<String>,
    pub text: TextEdit,
    /// The last check: the text it was of, and the errors the edit adds (empty: it fits).
    check: Mutex<Option<(String, Vec<String>)>>,
}

pub struct CallEdit {
    pub call: String,
    pub file: PathBuf,
    pub src: Src,
    pub orig: String,
    /// Its lines after its signature in the file (bytes: its attributes, `task …`, `model …`, and its template), and
    /// their indent; the box holds them with that indent cut.
    pub span: (usize, usize),
    pub indent: String,
    pub params: Vec<String>,
    pub text: TextEdit,
    /// The last check: the text it was of, and the errors the edit adds.
    check: Mutex<Option<(String, Vec<String>)>>,
}

#[derive(Clone, PartialEq)]
pub struct Step {
    pub t: u64,
    pub key: String,
    pub worker: String,
    pub call: String,
    pub inline: Option<String>,
    /// The step the ticket waits on now: what follows it can be edited (coord edit).
    pub awaited: bool,
}

// ---- where the files are --------------------------------------------------------------------------------------------

/// Where FOLDER keeps SUB, as coordd's project::place: FOLDER/SUB, or FOLDER itself when it has neither a kinds nor a
/// calls folder.
fn place(folder: &Path, sub: &str) -> PathBuf {
    if folder.join("kinds").is_dir() || folder.join("calls").is_dir() { folder.join(sub) } else { folder.to_path_buf() }
}

fn files(dir: &Path, ext: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == ext))
        .filter_map(|p| std::fs::read_to_string(&p).ok().map(|t| (p.display().to_string(), t)))
        .collect();
    out.sort();
    out
}

/// The project's folder: the one its project file (named in ~/agent-coord/ID/project) is in.
pub fn project_dir(p: &crate::Proj) -> Option<PathBuf> {
    // the server's, from a coordd that sends its places (N-412 p129); on Windows a path there, not here
    if let Some(d) = p.live.places.as_ref().and_then(|x| x.definition.as_deref()) {
        return Some(PathBuf::from(d)).filter(|d| crate::elsewhere::away() || d.is_dir());
    }
    let f = std::fs::read_to_string(format!("{}/project", crate::projects::coord_dir(&p.id))).ok()?;
    Path::new(f.trim()).parent().map(Path::to_path_buf).filter(|d| d.is_dir())
}

/// The folders coordd reads kinds and calls from for P: agent-base, the library its file names, the project's.
pub(crate) fn folders(s: &App, p: &crate::Proj) -> Vec<PathBuf> {
    let mut out = vec![PathBuf::from(&s.base)];
    if let Some(dir) = project_dir(p) {
        if let Some(lib) = p.live.project.pointer("/project/library").and_then(|x| x.as_str()) {
            let l = Path::new(lib);
            out.push(if l.is_absolute() { l.to_path_buf() } else { dir.join(l) });
        }
        out.push(dir);
    }
    out
}

/// A draft kinds file's folders (the Kinds page's YAAIHH_PICTURE_FILE): agent-base and the draft's own folder.
pub(crate) fn draft_folders(s: &App, file: &Path) -> Vec<PathBuf> {
    vec![PathBuf::from(&s.base), file.parent().map(Path::to_path_buf).unwrap_or_default()]
}

/// Where the current edit's files are read: a draft's folders; on Windows the files P's coordd served (none yet:
/// empty); else P's folders here.
fn src_of(s: &App, p: Option<&crate::Proj>) -> Src {
    if let Some(f) = &s.tickets.picture_file {
        return Src::Here(draft_folders(s, Path::new(f)));
    }
    let Some(p) = p else { return Src::Here(Vec::new()) };
    if crate::elsewhere::away() {
        return Src::There(s.tickets.edits.served.as_ref().filter(|(id, _)| *id == p.id).map(|(_, f)| f.clone()).unwrap_or_default());
    }
    Src::Here(folders(s, p))
}

/// Windows: an edit of kind or call NAME starts with the files as coordd serves them now. True when they have just
/// come (go on); false when they were asked for or cannot be (the line says why).
fn served_now(s: &mut App, kind: bool, name: &str) -> bool {
    if std::mem::take(&mut s.tickets.edits.fresh) {
        return true;
    }
    if !s.proj().is_some_and(crate::files::served) {
        s.tickets.edits.said = Some(Err(crate::elsewhere::LINE.into()));
        return false;
    }
    let what = Asked::Sources { kind, name: name.to_string() };
    if s.request_in(s.cur, coord_types::Request::Sources, crate::Pending::Edit(what)) {
        s.tickets.edits.said = Some(Ok("Reading the files on the WSL machine…".into()));
    }
    false
}

/// A reply to what the edit asked the coordd of project PI (Windows).
pub fn came(s: &mut App, pi: usize, what: Asked, ok: bool, out: String) {
    match what {
        Asked::Sources { kind, name } => {
            let got = if ok { serde_json::from_str::<Served>(&out).map_err(|e| format!("unreadable files: {e}")) } else { Err(out) };
            match (got, s.projects.get(pi).map(|p| p.id.clone())) {
                (Ok(f), Some(id)) if pi == s.cur => {
                    s.tickets.edits.served = Some((id, f));
                    s.tickets.edits.once_of.lock().unwrap().clear();
                    s.tickets.edits.said = None;
                    s.tickets.edits.fresh = true;
                    if kind { start_kind(s, &name, None) } else { start_call(s, &name) }
                    s.tickets.edits.fresh = false;
                }
                (Err(e), _) => {
                    s.tickets.edits.said = Some(Err(e));
                    (s.tickets.edits.lines_wanted, s.tickets.edits.binding_wanted) = (None, None);
                }
                _ => {}
            }
        }
        Asked::Prompts => {
            let got = if ok { serde_json::from_str::<Served>(&out).map_err(|e| format!("unreadable files: {e}")) } else { Err(out.trim().to_string()) };
            match (got, s.projects.get(pi).map(|p| p.id.clone())) {
                // kept only for the project still current, and never over the files an edit under way was read from
                (Ok(f), Some(id)) if pi == s.cur && s.tickets.edits.served.as_ref().is_none_or(|(x, _)| *x != id) => {
                    s.tickets.edits.served = Some((id, f));
                    s.tickets.edits.once_of.lock().unwrap().clear();
                    s.deep.files.remove(&pi);
                }
                (Ok(_), _) => _ = s.deep.files.remove(&pi),
                // asked again once the pipe to it ends and is back (map/deep.rs `ended`), not on the next frame
                (Err(e), _) => crate::log::line(&format!("map: step prompts missing: {e}")),
            }
        }
        Asked::Saved { kind, name, why, copy } => {
            let r = if ok { Ok(out.trim().to_string()) } else { Err(out.trim().to_string()) };
            saved(s, kind, &name, &why, &copy, r);
        }
        Asked::Reach { text } => {
            let r = if ok { serde_json::from_str::<ReloadReply>(&out).map_err(|e| format!("unreadable answer: {e}")) } else { Err(out.trim().to_string()) };
            s.tickets.edits.reach = Some((text, r));
        }
        Asked::DryRun { ask } => {
            let r = if ok { serde_json::from_str::<coord_types::DryRunAnswer>(&out).map_err(|e| format!("unreadable answer: {e}")) } else { Err(out.trim().to_string()) };
            // an answer that comes after he hid it is not kept
            if s.tickets.edits.dry_open {
                s.tickets.edits.dry = Some((ask, r));
            }
        }
    }
}

// ---- what an edit reaches -------------------------------------------------------------------------------------------

/// How long his typing pauses before what the edit reaches is asked: coordd reads every kind and call again for it,
/// so not at each key.
const REACH_PAUSE: std::time::Duration = std::time::Duration::from_millis(600);

/// The file the edit under way would write and its text, when it checks: what a Reach asks of. None while no edit
/// writes a file (none under way; a step's prompt for only this thread).
pub(crate) fn reach_of(s: &App) -> Option<(PathBuf, String)> {
    let e = &s.tickets.edits;
    if let Some(k) = &e.kind {
        return k.check().is_empty().then(|| (k.file.clone(), k.whole()));
    }
    let c = e.call_edit.as_ref().filter(|_| !(e.step.is_some() && e.only_thread))?;
    if !bad_slots(&c.text.text, &c.params).is_empty() || !c.check().is_empty() {
        return None;
    }
    call_written(s, c).ok().map(|(f, _, t)| (f, t))
}

/// Whether the project's coordd answers a Reach: its hello says `REACH_PROTOCOL` or more. Below it nothing is asked,
/// and Save is as it was.
fn reach_served(s: &App) -> bool {
    s.proj().is_some_and(|p| p.protocol >= coord_types::REACH_PROTOCOL)
}

/// Whether a Reach is on its way: one at a time.
fn reach_on_its_way(s: &App) -> bool {
    s.projects.iter().any(|p| p.pending.values().any(|w| matches!(w, crate::Pending::Edit(Asked::Reach { .. }))))
}

/// Each frame: the file the edit under way would write, once he pauses, asked of coordd (`Request::Reach`), one on its
/// way at a time, the answer coming as [`came`]; with no edit, the last answer dropped. True when that changed what
/// is drawn.
pub fn reach_poll(s: &mut App, tx: &mut rune::Tx) -> bool {
    let Some((file, text)) = reach_of(s) else {
        let e = &mut s.tickets.edits;
        e.typing = (String::new(), None);
        return e.reach.take().is_some();
    };
    let e = &mut s.tickets.edits;
    if e.typing.0 != text {
        e.typing = (text.clone(), Some(std::time::Instant::now()));
    }
    if e.reach.as_ref().is_some_and(|(t, _)| *t == text) || !crate::find::paused(e.typing.1, REACH_PAUSE, &mut e.woken, tx) {
        return false;
    }
    if reach_on_its_way(s) || !reach_served(s) || !s.proj().is_some_and(|p| p.link.is_some()) {
        return false;
    }
    let req = coord_types::Request::Reach { path: file.to_string_lossy().replace('\\', "/"), text: text.clone() };
    s.request_in(s.cur, req, crate::Pending::Edit(Asked::Reach { text }));
    false
}

/// What the edit under way reaches, as coordd answered (p294): the running tickets it reaches, those whose step is given
/// now, those past it and those that keep their old text, each `label · id` with coordd's words; a clock's next firing;
/// or why coordd would refuse it.
fn reach_part(s: &App, k: &Kit) -> El {
    let small = |t: String, c: rune::Color| text(t).font(k.ty.small).color(c).wrap();
    let mut c = col().key("edit-reach").w_full().gap(4.0);
    let Some((t, r)) = s.tickets.edits.reach.as_ref() else {
        let w = if reach_served(s) { "Pause to see what it reaches." } else { "Needs a newer engine." };
        return c.child(small(w.into(), k.c.text_muted));
    };
    // an answer for the text before his last change is never drawn as the current one: faint, under a line that says
    // so, until the text as it is now is answered (consult-3 p334)
    let stale = reach_of(s).is_none_or(|(_, now)| now != *t);
    let (base, muted) = if stale { (k.c.text_faint, k.c.text_faint) } else { (k.c.text_base, k.c.text_muted) };
    if stale {
        let w = if s.proj().is_some_and(|p| p.link.is_some()) {
            "Before your last change; checking…"
        } else {
            "Before your last change; rechecked on reconnect."
        };
        c = c.child(small(w.into(), k.c.text_muted).key("edit-reach-stale"));
    }
    let r = match r {
        Err(w) => return c.child(small(format!("Would be refused: {w}"), if stale { muted } else { k.c.danger.fg })),
        Ok(r) => r,
    };
    if r.tickets.is_empty() {
        let w = if s.tickets.edits.kind.as_ref().is_some_and(|e| e.binding.is_some()) { "No running ticket changes." } else { "Applies to new tickets only." };
        c = c.child(small(w.into(), muted));
    }
    // coordd's cases in this order; one this window does not know yet after them, under coordd's word: no ticket left out
    let case = |t: &Reloaded| t.reach.clone().unwrap_or_else(|| if t.took { "ahead" } else { "kept" }.to_string());
    let mut cases: Vec<(String, String)> = [("ahead", "Reaches"), ("given", "Given now"), ("own", "Your own prompt stands"), ("past", "Past it"), ("withdrawn", "Withdrawn"), ("kept", "Keeps its old text")].map(|(c, h)| (c.to_string(), h.to_string())).into();
    for c in r.tickets.iter().map(case) {
        if !cases.iter().any(|(x, _)| *x == c) {
            cases.push((c.clone(), c));
        }
    }
    for (reach, head) in cases {
        let ts: Vec<&Reloaded> = r.tickets.iter().filter(|t| case(t) == reach).collect();
        if ts.is_empty() {
            continue;
        }
        c = c.child(text(format!("{head} · {}", ts.len())).font(k.ty.small_medium).color(if reach == "withdrawn" && !stale { k.c.warning.fg } else { base }));
        for t in ts {
            let words = t.said.clone().or_else(|| t.why.clone()).unwrap_or_default();
            let who = match s.proj().and_then(|p| p.live.tickets.values().find(|v| v.id == t.id).map(|v| (p, v))) {
                Some((p, v)) => crate::refs::el(&crate::refs::ticket(Some(p), v), k.ty.small, base, k),
                None => text(t.id.clone()).font(k.ty.small).color(base),
            };
            c = c.child(col().key(format!("edit-reach-{}", t.id)).w_full().gap(1.0).pl(8.0).child(who).child(small(words, muted)));
        }
    }
    for x in &r.clocks {
        c = c.child(small(format!("{} — fires next {}", x.binding, crate::when::at(&x.next)), base));
    }
    c
}

// ---- the prompt as it would be sent (H2) -----------------------------------------------------------------------------

/// Whether a prompt is being edited: a call's lines (for the program), or a step's prompt for only its thread.
fn prompt_editing(s: &App) -> bool {
    let e = &s.tickets.edits;
    e.call_edit.is_some() || (e.step.is_some() && e.only_thread)
}

/// The template a step's thread-only box starts from: his own saved for the thread (the ticket's view, from the
/// record), else the call's (CALL_TEMPLATE).
fn thread_template(v: &coord_types::TicketView, call: &str, call_template: String) -> String {
    v.templates.iter().find(|t| t.call == call).map_or(call_template, |t| t.text.clone())
}

/// What a dry run asks of now, while it is shown and the text checks: for the program, the file Save would write and
/// its text ([`call_written`], as Reach asks), from the ticket whose step panel is open on that call, if one is; for
/// only the thread, the text in its box, on that ticket. None while hidden, while the text does not check, and for an
/// inline step (no call to edit).
pub(crate) fn dry_of(s: &App) -> Option<DryAsk> {
    let e = &s.tickets.edits;
    if !e.dry_open {
        return None;
    }
    let p = s.proj()?;
    let view = move |n: u64| p.live.tickets.values().find(|v| v.n == n);
    if let Some(st) = e.step.as_ref().filter(|st| e.only_thread && st.inline.is_none()) {
        let v = view(st.t)?;
        let (call_template, params) = once_of(s, p, &st.call)?;
        let text = if e.once.text.is_empty() { thread_template(v, &st.call, call_template) } else { e.once.text.clone() };
        return bad_slots(&text, &params).is_empty().then(|| DryAsk { call: st.call.clone(), on: Some(v.id.clone()), path: None, text });
    }
    let c = e.call_edit.as_ref().filter(|_| !(e.step.is_some() && e.only_thread))?;
    if !bad_slots(&c.text.text, &c.params).is_empty() || !c.check().is_empty() {
        return None;
    }
    let (file, _, text) = call_written(s, c).ok()?;
    let on = e.step.as_ref().filter(|st| st.call == c.call).and_then(|st| view(st.t)).map(|v| v.id.clone());
    Some(DryAsk { call: c.call.clone(), on, path: Some(file.to_string_lossy().replace('\\', "/")), text })
}

/// Whether the project's coordd renders a dry run: its hello says `DRY_RUN_PROTOCOL` or more.
fn dry_served(s: &App) -> bool {
    s.proj().is_some_and(|p| p.protocol >= coord_types::DRY_RUN_PROTOCOL)
}

/// Each frame: while it is shown, the prompt being edited asked of coordd once he pauses (`Request::DryRun`), one on
/// its way at a time, the answer coming as [`came`]; with nothing to ask, the last answer dropped, and with no prompt
/// being edited, hidden again. True when that changed what is drawn.
pub fn dry_poll(s: &mut App, tx: &mut rune::Tx) -> bool {
    if !prompt_editing(s) {
        let e = &mut s.tickets.edits;
        e.dry_typing = (None, None);
        let was = e.dry_open || e.dry.is_some();
        (e.dry_open, e.dry) = (false, None);
        return was;
    }
    let Some(ask) = dry_of(s) else {
        let e = &mut s.tickets.edits;
        e.dry_typing = (None, None);
        return e.dry.take().is_some();
    };
    let e = &mut s.tickets.edits;
    if e.dry_typing.0.as_ref() != Some(&ask) {
        e.dry_typing = (Some(ask.clone()), Some(std::time::Instant::now()));
    }
    if e.dry.as_ref().is_some_and(|(a, _)| *a == ask) || !crate::find::paused(e.dry_typing.1, REACH_PAUSE, &mut e.dry_woken, tx) {
        return false;
    }
    let on_its_way = s.projects.iter().any(|p| p.pending.values().any(|w| matches!(w, crate::Pending::Edit(Asked::DryRun { .. }))));
    if on_its_way || !dry_served(s) || !s.proj().is_some_and(|p| p.link.is_some()) {
        return false;
    }
    let req = coord_types::Request::DryRun { call: ask.call.clone(), on: ask.on.clone(), path: ask.path.clone(), text: ask.text.clone() };
    s.request_in(s.cur, req, crate::Pending::Edit(Asked::DryRun { ask }));
    false
}

/// Under the prompt being edited (H2, his "we can dry run it ??", N-216.p1): "Show it as it would be sent"; pressed,
/// the step's text as its agent would get it, as coordd's dry run rendered it with a waiting step's arguments, and
/// which step that is; nothing is sent. Or why there is none, or why it would not be sent. Its words are drawn here
/// only, behind his press (p1380).
fn dry_part(s: &App, k: &Kit) -> El {
    let small = |t: String, c: rune::Color| text(t).font(k.ty.small).color(c).wrap();
    let e = &s.tickets.edits;
    let c = col().key("edit-dry").w_full().gap(6.0);
    if !e.dry_open {
        let show = Button::new("Show it as it would be sent").variant(ButtonVariant::Neutral).on_press(|s: &mut App| s.tickets.edits.dry_open = true);
        return c.child(row().w_full().child(show.build(k).key("edit-dry-show")));
    }
    let hide = Button::new("Hide").variant(ButtonVariant::Ghost).on_press(|s: &mut App| (s.tickets.edits.dry_open, s.tickets.edits.dry) = (false, None));
    let foot = |c: El, line: Option<El>| c.child(row().w_full().gap(8.0).center_y().child(line.map_or_else(|| col().grow(1.0), |l| l.grow(1.0).shrink())).child(hide.build(k).key("edit-dry-hide")));
    if !dry_served(s) {
        return foot(c, Some(small("Needs a newer engine.".into(), k.c.text_muted)));
    }
    let now = dry_of(s);
    let Some((ask, r)) = e.dry.as_ref() else {
        let w = if now.is_some() { "Pause to preview." } else { "Preview once it checks." };
        return foot(c, Some(small(w.into(), k.c.text_muted)));
    };
    // an answer for the text before his last change is never drawn as the current one (as the reach's, consult-3 p334)
    let stale = now.as_ref() != Some(ask);
    let (base, muted) = if stale { (k.c.text_faint, k.c.text_faint) } else { (k.c.text_base, k.c.text_muted) };
    let mut c = c;
    if stale {
        c = c.child(small("Before your last change; rendering…".into(), k.c.text_muted).key("edit-dry-stale"));
    }
    let call = &ask.call;
    let a = match r {
        Err(w) => return foot(c, Some(small(format!("✗ It would not be sent: {w}"), if stale { muted } else { k.c.danger.fg }))),
        Ok(a) => a,
    };
    let Some(on) = a.on.as_ref() else {
        return foot(c, Some(small(format!("No ticket waits on {call} yet."), muted)));
    };
    // the step's holder as he reads it: "you" for his own step
    let (holds, unread) = match a.by.as_str() {
        "owner" => ("which you hold now".to_string(), "Posts you haven't read".to_string()),
        by => (format!("which {by} holds now"), format!("Posts {by} hasn't read")),
    };
    let whose = if a.asked { format!("with {on}'s step {} arguments, {holds}", a.step) } else { format!("with {on}'s step {} arguments", a.step) };
    c = c.child(small(format!("Preview · {whose}"), base).key("edit-dry-head"));
    if a.own {
        c = c.child(small(format!("{on} uses your prompt for this step."), base).key("edit-dry-own"));
    }
    c = c.child(col().key("edit-dry-text").w_full().padding(10.0).radius(k.radius.sm).fill(k.c.layer_01).scroll_x().child(text(a.text.clone()).font(k.ty.mono).color(base).selectable(true)));
    foot(c, Some(small(format!("{unread} go with it."), muted)))
}

// ---- what the picture marks -----------------------------------------------------------------------------------------

/// What a program's picture marks of its live edits (p294): what is being edited now, and when each kind and call was
/// last edited, from the record (the Kinds answer's `edited`), or by this window's last Save until it says.
#[derive(Clone, Debug, Default)]
pub struct Marks {
    /// The call being edited: its own steps are marked, and the watcher's steps of a call coordd runs it for
    /// ([`Marks::step_editing`]).
    pub call: Option<String>,
    /// The kind being edited, and the lines of its text being edited (1-based): all of it when None.
    pub kind: Option<(String, Option<(usize, usize)>)>,
    /// The clock whose line is being edited, by its binding as written.
    pub binding: Option<String>,
    /// When each kind, and each call, was last edited live, by its own name; a step's, by [`Marks::step_edited`].
    pub kinds_at: BTreeMap<String, String>,
    pub calls_at: BTreeMap<String, String>,
    /// The calls coordd runs another call for (`CallForm::runs`), by name: an edit of that other call reaches a step of
    /// one only as `runs_for` gives it, on the watcher's step (consult-3 N-1446 p1231).
    pub runs: BTreeMap<String, crate::kinds::CallForm>,
}

impl Marks {
    /// Whether line LINE of KIND's text is being edited in place (a whole kind's edit draws its own picture).
    pub fn editing(&self, kind: &str, line: usize) -> bool {
        matches!(&self.kind, Some((k, Some((a, b)))) if k == kind && (*a..=*b).contains(&line))
    }

    /// The calls whose lines a step of CALL taken by BY reads: its own, and the one coordd runs for it (`runs_for`, as
    /// the call panel and a step's prompt read it).
    fn calls_read<'a>(&'a self, call: &'a str, by: Option<&str>) -> impl Iterator<Item = &'a str> {
        let runs = self.runs.get(call).and_then(|d| runs_for(d, by));
        std::iter::once(call).chain(runs.map(|r| r.name.as_str()))
    }

    /// Whether the edit under way reaches a step of CALL taken by BY.
    pub fn step_editing(&self, call: &str, by: Option<&str>) -> bool {
        self.call.as_deref().is_some_and(|e| self.calls_read(call, by).any(|c| c == e))
    }

    /// When a step of CALL taken by BY was last edited live: the latest of the calls it reads.
    pub fn step_edited(&self, call: &str, by: Option<&str>) -> Option<&str> {
        self.calls_read(call, by).filter_map(|c| self.calls_at.get(c)).max().map(String::as_str)
    }
}

/// The marks of PG's edit under way and of the last live edits.
pub fn marks(pg: &crate::threads::Page) -> Marks {
    let forms: &[crate::kinds::KindForm] = match &pg.kinds {
        Some(Ok(ks)) => ks,
        _ => &[],
    };
    let mut m = Marks { call: pg.edits.call_edit.as_ref().map(|e| e.call.clone()), kind: pg.edits.kind.as_ref().filter(|e| e.binding.is_none()).map(|e| (e.kind.clone(), e.lines)), binding: pg.edits.kind.as_ref().and_then(|e| e.binding.clone()), ..Default::default() };
    let at = |map: &mut BTreeMap<String, String>, name: String, when: &str| {
        if map.get(&name).is_none_or(|w| w.as_str() < when) {
            map.insert(name, when.to_string());
        }
    };
    if let Some((when, r)) = &pg.edits.reloaded {
        for n in &r.kinds {
            at(&mut m.kinds_at, n.clone(), when);
        }
        for n in &r.calls {
            at(&mut m.calls_at, n.clone(), when);
        }
    }
    for f in forms {
        if let Some(x) = &f.edited {
            at(&mut m.kinds_at, f.kind.clone(), &x.at);
        }
        for c in &f.calls {
            if let Some(x) = &c.edited {
                at(&mut m.calls_at, c.name.clone(), &x.at);
            }
            // a call coordd runs another for: that call's last edit, by its own name, and the call's form for `runs_for`
            if let Some(r) = &c.runs {
                if let Some(x) = &r.edited {
                    at(&mut m.calls_at, r.name.clone(), &x.at);
                }
                m.runs.insert(c.name.clone(), c.clone());
            }
        }
    }
    m
}

/// Where a top-level item starting with HEAD (`kind piece`, `call mind(`) stands in TEXT: from its line through the
/// lines under it (indented, or blank), trailing blank lines and comments left to what follows.
pub fn item_span(text: &str, head: &str) -> Option<(usize, usize)> {
    let mut at = 0;
    let mut start = None;
    let mut end = 0;
    for l in text.split_inclusive('\n') {
        let body = l.trim_end_matches(['\n', '\r']);
        match start {
            None => {
                let rest = body.strip_prefix(head);
                if rest.is_some_and(|r| head.ends_with('(') || r.is_empty() || r.starts_with(char::is_whitespace)) {
                    start = Some(at);
                    end = at + body.len();
                }
            }
            Some(_) => {
                if body.starts_with(char::is_whitespace) && !body.trim().is_empty() {
                    end = at + body.len();
                } else if !body.trim().is_empty() {
                    break;
                }
            }
        }
        at += l.len();
    }
    start.map(|s| (s, end))
}

// ---- the check ------------------------------------------------------------------------------------------------------

/// The calls of FOLDERS as coordd reads them (agent-base's first, a later folder's replacing by name).
fn calls_of(src: &Src) -> (BTreeMap<String, syntax::CallDef>, syntax::Named, Vec<String>) {
    calls_in(&src.calls())
}

/// The calls of SOURCES (path and text, in order) as coordd reads them, and why any could not be read.
fn calls_in(sources: &[(String, String)]) -> (BTreeMap<String, syntax::CallDef>, syntax::Named, Vec<String>) {
    let mut named = syntax::Named::new();
    let mut errors = Vec::new();
    if let Err(e) = syntax::parse_types("system", syntax::SYSTEM_TYPES, &mut named) {
        errors.push(e);
    }
    let (defs, errs) = syntax::parse_call_files(sources, &mut named);
    errors.extend(errs);
    (defs, named, errors)
}

/// FILES (path and text) with FILE read as TEXT: in its place, or after them when it is none of them (a new copy).
fn with_file(files: Vec<(String, String)>, file: &Path, text: &str) -> Vec<(String, String)> {
    let f = file.display().to_string();
    let mut out: Vec<(String, String)> = files.into_iter().map(|(p, t)| if p == f { (p, text.to_string()) } else { (p, t) }).collect();
    if !out.iter().any(|(p, _)| *p == f) {
        out.push((f, text.to_string()));
    }
    out
}

/// The errors of the project's kinds with FILE read as TEXT.
fn errors_with(src: &Src, file: &Path, text: &str) -> Vec<String> {
    let (defs, named, mut errors) = calls_of(src);
    let calls = defs.into_iter().map(|(n, c)| (n, c.sig)).collect();
    let kinds = with_file(src.kinds(), file, text);
    let (_, errs) = docket_machine::machine::Defs::build_with(&kinds, calls, named, 1, false);
    errors.extend(errs);
    errors
}

/// The errors of the project's calls with FILE read as TEXT.
fn call_errors_with(src: &Src, file: &Path, text: &str) -> Vec<String> {
    calls_in(&with_file(src.calls(), file, text)).2
}

/// The errors WHOLE, the file as the edit would write it, adds to those of ORIG, the file as read (ERRORS reading the
/// files with it), checked again only when TEXT, the edit's box, changed (CACHE): a kind's edit's check and a call's.
fn added(cache: &Mutex<Option<(String, Vec<String>)>>, text: &str, orig: &str, whole: &str, errors: impl Fn(&str) -> Vec<String>) -> Vec<String> {
    let mut g = cache.lock().unwrap();
    if let Some((_, e)) = g.as_ref().filter(|(t, _)| t == text) {
        return e.clone();
    }
    let before = errors(orig);
    let added: Vec<String> = errors(whole).into_iter().filter(|e| !before.contains(e)).collect();
    *g = Some((text.to_string(), added.clone()));
    added
}

impl KindEdit {
    fn whole(&self) -> String {
        format!("{}{}{}", &self.orig[..self.span.0], self.text.text, &self.orig[self.span.1..])
    }

    /// The errors this edit adds, checked again only when its text changed.
    pub fn check(&self) -> Vec<String> {
        added(&self.check, &self.text.text, &self.orig, &self.whole(), |t| errors_with(&self.src, &self.file, t))
    }
}

impl CallEdit {
    /// The file with the box's lines in place of its lines, each given back the indent they had.
    fn whole(&self) -> String {
        let lines: Vec<String> = self.text.text.trim_end().lines().map(|l| if l.trim().is_empty() { String::new() } else { format!("{}{l}", self.indent) }).collect();
        format!("{}{}{}", &self.orig[..self.span.0], lines.join("\n"), &self.orig[self.span.1..])
    }

    /// The errors this edit adds to the calls, checked again only when its text changed.
    pub fn check(&self) -> Vec<String> {
        added(&self.check, &self.text.text, &self.orig, &self.whole(), |t| call_errors_with(&self.src, &self.file, t))
    }
}

/// The slots of a template that are not the call's fields: `{{ x }}`, `{{ x.y }}`, `{{ if x }}`, `{{ for a in x }}`.
pub fn bad_slots(template: &str, params: &[String]) -> Vec<String> {
    let mut bound: Vec<String> = Vec::new();
    let mut bad = Vec::new();
    let mut rest = template;
    while let Some(a) = rest.find("{{") {
        let r = &rest[a + 2..];
        let Some(b) = r.find("}}") else {
            bad.push("a {{ with no }}".to_string());
            break;
        };
        let inner = r[..b].trim();
        rest = &r[b + 2..];
        let words: Vec<&str> = inner.split_whitespace().collect();
        let name = match words.as_slice() {
            ["end"] => continue,
            ["if", x, ..] => *x,
            ["for", v, "in", x, ..] => {
                bound.push(v.to_string());
                *x
            }
            [x, ..] => *x,
            [] => continue,
        };
        let root = name.split(['.', '[', '|']).next().unwrap_or(name).trim_start_matches('!');
        if !params.iter().any(|p| p == root) && !bound.iter().any(|v| v == root) && !bad.contains(&root.to_string()) {
            bad.push(root.to_string());
        }
    }
    bad
}

// ---- starting an edit -----------------------------------------------------------------------------------------------

/// Edit kind KIND: found in the project's own kinds (a draft: in FILE), else as the project's own copy of agent-base's
/// or the library's (`kind_found`).
pub fn start_kind(s: &mut App, kind: &str, draft: Option<&str>) {
    // on Windows the files are the WSL machine's: asked of coordd first, the edit goes on when they come (N-744)
    let away = crate::elsewhere::away() && draft.is_none();
    if away && !served_now(s, true, kind) {
        return;
    }
    if let Some((_, file, line, binding)) = s.tickets.edits.binding_wanted.take().filter(|(k, ..)| k == kind) {
        match binding_found(s, kind, &file, line, &binding) {
            Ok(e) => {
                s.tickets.edits.kind = Some(e);
                s.tickets.edits.said = None;
            }
            Err(w) => s.tickets.edits.said = Some(Err(w)),
        }
        return;
    }
    let wanted = s.tickets.edits.lines_wanted.take().filter(|(k, _, _)| k == kind);
    match kind_found(s, kind, draft) {
        Ok(mut e) => {
            if let Some((_, text, lines)) = wanted {
                if let Err(w) = only_lines(&mut e, &text, lines) {
                    s.tickets.edits.said = Some(Err(w));
                    return;
                }
            }
            s.tickets.edits.kind = Some(e);
            s.tickets.edits.said = None;
        }
        Err(w) => s.tickets.edits.said = Some(Err(w)),
    }
}

/// Edit LINES (1-based, first and last) of kind KIND, whose TEXT is as coordd read it: the lines a press lit in its
/// picture (picture.rs `lit_lines`), in place, the rest of the kind as it is (p294).
pub fn start_kind_lines(s: &mut App, kind: &str, text: &str, lines: (usize, usize)) {
    s.tickets.edits.binding_wanted = None;
    s.tickets.edits.lines_wanted = Some((kind.to_string(), text.to_string(), lines));
    start_kind(s, kind, None);
}

/// Edit in place the clock that opens KIND (p294): line LINE (1-based) of FILE, which coordd read as BINDING (the
/// snapshot's clock, `ClockView`). Its file's whole text is checked as a kind's is, and Reach and Save are a kind's.
pub fn start_binding(s: &mut App, kind: &str, file: &str, line: usize, binding: &str) {
    s.tickets.edits.lines_wanted = None;
    s.tickets.edits.binding_wanted = Some((kind.to_string(), file.to_string(), line, binding.to_string()));
    start_kind(s, kind, None);
}

/// The clock's line as an edit opens it: line LINE of FILE, when that line is BINDING as coordd read it (bindings are
/// read with their spaces made single, syntax.rs) and FILE is one of the project's own; Err says why not.
fn binding_found(s: &App, kind: &str, file: &str, line: usize, binding: &str) -> Result<KindEdit, String> {
    let p = s.proj().ok_or_else(|| String::from("No project is open."))?;
    let src = src_of(s, Some(p));
    let dir = project_dir(p).ok_or_else(|| String::from("Project not on this machine."))?;
    let f = PathBuf::from(file);
    // as kind_found's own kinds: under the project's folder
    if !f.starts_with(&dir) {
        return Err(format!("Edit it in {file}."));
    }
    let orig = src.text_of(&f).ok_or_else(|| format!("{file} cannot be read"))?;
    // from its first line, as many lines as it is written over: their code, comments cut, spaces made single
    let (mut at, mut from, mut code) = (0, None, String::new());
    for (i, l) in orig.split_inclusive('\n').enumerate() {
        if i + 1 == line {
            from = Some(at);
        }
        if let Some(a) = from {
            let body = l.trim_end_matches(['\n', '\r']);
            code = format!("{code} {}", body.split("//").next().unwrap_or(body)).split_whitespace().collect::<Vec<_>>().join(" ");
            if code == binding {
                let span = (a, at + body.len());
                return Ok(KindEdit { kind: kind.to_string(), file: f, src, text: TextEdit::new(&orig[span.0..span.1]), orig, span, new: false, lines: None, copy: None, binding: Some(binding.to_string()), check: Mutex::new(None) });
            }
            if code.len() >= binding.len() {
                break;
            }
        }
        at += l.len();
    }
    Err(format!("Line {line} of {file} changed; reopen it."))
}

/// Kind edit E narrowed to LINES of the kind's TEXT as coordd read it: the same lines in its file, or Err (the file
/// changed since coordd read it).
fn only_lines(e: &mut KindEdit, text: &str, lines: (usize, usize)) -> Result<(), String> {
    let body = &e.orig[e.span.0..e.span.1];
    let (want, have): (Vec<&str>, Vec<&str>) = (text.lines().collect(), body.lines().collect());
    let (a, b) = lines;
    if a == 0 || b < a || b > want.len() || want.get(a - 1..b) != have.get(a - 1..b) {
        return Err(format!("kind {}'s file changed; Edit kind opens it whole.", e.kind));
    }
    let (mut at, mut from, mut to) = (e.span.0, None, e.span.0);
    for (i, l) in body.split_inclusive('\n').enumerate() {
        if i + 1 == a {
            from = Some(at);
        }
        if i + 1 == b {
            to = at + l.trim_end_matches(['\n', '\r']).len();
        }
        at += l.len();
    }
    let from = from.unwrap_or(e.span.0);
    e.span = (from, to);
    e.lines = Some(lines);
    e.text = TextEdit::new(&e.orig[from..to]);
    Ok(())
}

/// Kind KIND as an edit opens it: in the project's own kinds (a draft: in FILE), else where agent-base or the library
/// writes it, saved as the project's own copy, which replaces it by name (as a call's is, `call_written`); Err says
/// why it cannot be.
fn kind_found(s: &App, kind: &str, draft: Option<&str>) -> Result<KindEdit, String> {
    let head = format!("kind {kind}");
    let edit = |file: PathBuf, src: Src, orig: String, span: (usize, usize), new: bool| KindEdit { kind: kind.to_string(), file, src, text: TextEdit::new(&orig[span.0..span.1]), orig, span, new, lines: None, copy: None, binding: None, check: Mutex::new(None) };
    if let Some(f) = draft {
        let (f, src) = (PathBuf::from(f), Src::Here(draft_folders(s, Path::new(f))));
        let orig = src.text_of(&f).ok_or_else(|| format!("{} cannot be read", f.display()))?;
        let span = item_span(&orig, &head).ok_or_else(|| format!("kind {kind} is not in {}", f.display()))?;
        return Ok(edit(f, src, orig, span, false));
    }
    let p = s.proj().ok_or_else(|| String::from("No project is open."))?;
    let src = src_of(s, Some(p));
    let dir = project_dir(p).ok_or_else(|| String::from("Project not on this machine."))?;
    let own: Vec<PathBuf> = match &src {
        // the project's own kinds: those coordd read under its folder
        Src::There(_) => src.kinds().into_iter().map(|(f, _)| PathBuf::from(f)).filter(|f| f.starts_with(&dir)).collect(),
        Src::Here(_) => files(&place(&dir, "kinds"), "kind").into_iter().map(|(f, _)| PathBuf::from(f)).collect(),
    };
    for f in own {
        let Some(orig) = src.text_of(&f) else { continue };
        if let Some(span) = item_span(&orig, &head) {
            return Ok(edit(f, src, orig, span, false));
        }
    }
    // agent-base's or the library's, as coordd reads it (a later folder's replacing an earlier's): its lines only, so
    // that the copy declares no event or type a second time
    let (from, theirs) = src.kinds().into_iter().rev().find_map(|(f, t)| item_span(&t, &head).map(|(a, b)| (f, t[a..b].to_string()))).ok_or_else(|| format!("kind {kind} is not in this project"))?;
    let file = match &src {
        Src::There(served) => there_place(&dir, served, "kinds").join_server(&format!("{kind}.kind")),
        Src::Here(_) => place(&dir, "kinds").join(format!("{kind}.kind")),
    };
    if matches!(src, Src::Here(_)) && file.exists() {
        return Err(coord_types::save_words::exists(&file.to_string_lossy()));
    }
    let span = (0, theirs.len());
    let copy = copy_words(s, Path::new(&from), &file);
    let mut e = edit(file, src, format!("{theirs}\n"), span, true);
    e.copy = Some(copy);
    Ok(e)
}

/// Open call CALL's template for editing.
pub fn start_call(s: &mut App, call: &str) {
    if crate::elsewhere::away() && s.tickets.picture_file.is_none() && !served_now(s, false, call) {
        return;
    }
    if s.tickets.picture_file.is_none() && s.proj().is_none() {
        return;
    }
    let src = src_of(s, s.proj());
    let (defs, _, _) = calls_of(&src);
    let Some(d) = defs.get(call) else {
        s.tickets.edits.said = Some(Err(format!("call {call} is not in this project's calls")));
        return;
    };
    let file = PathBuf::from(&d.file);
    let Some(orig) = src.text_of(&file) else { return };
    let Some((span, indent)) = body_span(&orig, call) else {
        s.tickets.edits.said = Some(Err(format!("call {call} is built in")));
        return;
    };
    let params = d.sig.params.iter().map(|(n, _)| n.clone()).collect();
    let text = TextEdit::new(orig[span.0..span.1].lines().map(|l| l.strip_prefix(indent.as_str()).unwrap_or(l.trim_start())).collect::<Vec<_>>().join("\n"));
    s.tickets.edits.call_edit = Some(CallEdit { call: call.to_string(), file, src, orig, span, indent, params, text, check: Mutex::new(None) });
    s.tickets.edits.said = None;
}

/// The lines of call CALL in TEXT after its signature, which may run over several lines: from its first attribute
/// line (`task`, `template`, …: syntax's `CALL_ATTRS`) to the call's end; and their indent.
fn body_span(text: &str, call: &str) -> Option<((usize, usize), String)> {
    let (s, e) = item_span(text, &format!("call {call}("))?;
    let mut at = s;
    for l in text[s..e].split_inclusive('\n') {
        let body = l.trim_end_matches(['\n', '\r']);
        let word = body.split_whitespace().next().unwrap_or("");
        if at > s && body.starts_with(char::is_whitespace) && syntax::CALL_ATTRS.contains(&word) {
            return Some(((at, e), body[..body.len() - body.trim_start().len()].to_string()));
        }
        at += l.len();
    }
    None
}

// ---- saving ---------------------------------------------------------------------------------------------------------

/// Write TEXT to FILE (unchanged since ORIG was read) and commit it where it is, with MSG: the commit's short name, or
/// why it was not committed, in the words coordd's `EditFile` uses (`coord_types::save_words`, N-802).
fn write_commit(file: &Path, orig: Option<&str>, text: &str, msg: &str) -> Result<String, String> {
    use coord_types::save_words as W;
    let path = file.to_string_lossy();
    if let Some(o) = orig {
        if std::fs::read_to_string(file).ok().as_deref() != Some(o) {
            return Err(W::changed(&path));
        }
    }
    if let Some(d) = file.parent() {
        std::fs::create_dir_all(d).map_err(|e| W::unwritten(&path, e))?;
    }
    std::fs::write(file, text).map_err(|e| W::unwritten(&path, e))?;
    let dir = file.parent().unwrap_or(Path::new("."));
    let git = |args: &[&str]| crate::places::command("git").arg("-C").arg(dir).args(args).output();
    match git(&["rev-parse", "--is-inside-work-tree"]) {
        Ok(o) if o.status.success() => {}
        _ => return Ok("written, not committed".into()),
    }
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let add = git(&["add", "--", &name]).map_err(|e| W::uncommitted("add", &e.to_string()))?;
    if !add.status.success() {
        return Err(W::uncommitted("add", &String::from_utf8_lossy(&add.stderr)));
    }
    let c = git(&["commit", "-q", "-m", msg, "--", &name]).map_err(|e| W::uncommitted("commit", &e.to_string()))?;
    if !c.status.success() {
        return Err(W::uncommitted("commit", &String::from_utf8_lossy(&c.stderr)));
    }
    let h = git(&["rev-parse", "--short", "HEAD"]).map_err(|e| e.to_string())?;
    Ok(format!("commit {}", String::from_utf8_lossy(&h.stdout).trim()))
}

fn why_of(s: &App) -> Result<String, ()> {
    let w = s.tickets.edits.why.text.trim().to_string();
    if w.is_empty() { Err(()) } else { Ok(w) }
}

pub fn save_kind(s: &mut App) {
    if let Some((commit, why)) = save_kind_file(s) {
        reload(s, commit, &why);
    }
}

/// Save's first half: kind E written and committed, the edit closed; Some(the commit, the why) for the reload that
/// follows, None when nothing was saved (the line says why).
pub(crate) fn save_kind_file(s: &mut App) -> Option<(Option<String>, String)> {
    let Ok(why) = why_of(s) else {
        s.tickets.edits.said = Some(Err("Say why first: the commit carries it.".into()));
        return None;
    };
    let e = s.tickets.edits.kind.as_ref()?;
    if !e.check().is_empty() {
        return None;
    }
    let msg = match e.binding {
        Some(_) => format!("kind {}'s clock: {why}\n\nEdited in yaaihh.", e.kind),
        None => format!("kind {}: {why}\n\nEdited in yaaihh.", e.kind),
    };
    let orig = (!e.new).then_some(e.orig.as_str());
    let copy = e.copy.as_ref().map(|c| format!(" {c}")).unwrap_or_default();
    if matches!(e.src, Src::There(_)) {
        let (req, kind) = (edit_file(&e.file, orig, &e.whole(), &msg), e.kind.clone());
        send_save(s, req, true, kind, why, copy);
        return None;
    }
    let r = write_commit(&e.file, orig, &e.whole(), &msg);
    let commit = r.as_ref().ok().and_then(|c| commit_of(c));
    // the reload's reply says, under this line, what each open ticket of it did (live edits)
    let said = r.map(|c| saved_words(e, &c, &copy));
    s.tickets.edits.said = Some(said);
    s.tickets.edits.said.as_ref()?.as_ref().ok()?;
    s.tickets.edits.kind = None;
    s.tickets.edits.why = TextEdit::new("");
    Some((commit, why))
}

/// What Save writes for call edit E: the file, its text as read (None: a new file), and its new text. A call of
/// agent-base's or the library's goes to the project's own copy, which replaces it by name: the call's lines only.
/// What a Reach asks of too (`reach_of`).
fn call_written(s: &App, e: &CallEdit) -> Result<(PathBuf, Option<String>, String), String> {
    let whole = e.whole();
    let project = s.proj().and_then(project_dir).filter(|_| s.tickets.picture_file.is_none());
    let Some(dir) = project.filter(|d| !e.file.starts_with(d)) else { return Ok((e.file.clone(), Some(e.orig.clone()), whole)) };
    let (s0, e0) = item_span(&whole, &format!("call {}(", e.call)).unwrap_or((0, whole.len()));
    let item = format!("{}\n", &whole[s0..e0]);
    let to = match &e.src {
        Src::There(served) => there_place(&dir, served, "calls").join_server(&format!("{}.call", e.call)),
        Src::Here(_) => place(&dir, "calls").join(format!("{}.call", e.call)),
    };
    if matches!(e.src, Src::Here(_)) && to.exists() {
        return Err(coord_types::save_words::exists(&to.to_string_lossy()));
    }
    Ok((to, None, item))
}

pub fn save_call(s: &mut App) {
    let Ok(why) = why_of(s) else {
        s.tickets.edits.said = Some(Err("Say why first: the commit carries it.".into()));
        return;
    };
    let Some(e) = s.tickets.edits.call_edit.as_ref() else { return };
    if !bad_slots(&e.text.text, &e.params).is_empty() || !e.check().is_empty() {
        return;
    }
    let msg = format!("call {}: {why}\n\nEdited in yaaihh.", e.call);
    let (call, there) = (e.call.clone(), matches!(e.src, Src::There(_)));
    let (file, orig, text) = match call_written(s, e) {
        Ok(w) => w,
        Err(w) => {
            s.tickets.edits.said = Some(Err(w));
            return;
        }
    };
    let copy = if orig.is_none() { format!(" {}", copy_words(s, &e.file, &file)) } else { String::new() };
    if there {
        let req = edit_file(&file, orig.as_deref(), &text, &msg);
        send_save(s, req, false, call, why, copy);
        return;
    }
    let r = write_commit(&file, orig.as_deref(), &text, &msg);
    let commit = r.as_ref().ok().and_then(|c| commit_of(c));
    s.tickets.edits.said = Some(r.map(|c| format!("Saved, {c}; applies to {call}'s next steps.{copy}")));
    if matches!(s.tickets.edits.said, Some(Ok(_))) {
        s.tickets.edits.call_edit = None;
        s.tickets.edits.why = TextEdit::new("");
        reload(s, commit, &why);
    }
}

/// Windows: the request that writes FILE (still ORIG; none: a new file) as TEXT and commits it with MSG, on the WSL
/// machine (coordd's `EditFile`, the same checks and answer as `write_commit`).
fn edit_file(file: &Path, orig: Option<&str>, text: &str, msg: &str) -> coord_types::Request {
    coord_types::Request::EditFile { path: file.to_string_lossy().replace('\\', "/"), old: orig.map(str::to_string), text: text.to_string(), message: msg.to_string() }
}

/// Windows: the project's folder SUB (`kinds`, `calls`) as coordd reads it (`place`): DIR/SUB when it has a kinds or
/// calls folder among the files served, else DIR.
fn there_place(dir: &Path, served: &Served, sub: &str) -> ServerPath {
    let d = dir.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_string();
    let has = |sub: &str| served.kinds.iter().chain(&served.calls).any(|f| f.path.starts_with(&format!("{d}/{sub}/")));
    ServerPath(if has("kinds") || has("calls") { format!("{d}/{sub}") } else { d })
}

/// A folder on the server, joined with `/` whatever this machine's separator.
struct ServerPath(String);

impl ServerPath {
    fn join_server(&self, name: &str) -> PathBuf {
        PathBuf::from(format!("{}/{name}", self.0))
    }
}

/// Windows: send a Save's request REQ (of kind or call NAME, with WHY; COPY as `Asked::Saved`); its answer finishes it
/// (`saved`).
fn send_save(s: &mut App, req: coord_types::Request, kind: bool, name: String, why: String, copy: String) {
    if s.request_in(s.cur, req, crate::Pending::Edit(Asked::Saved { kind, name, why, copy })) {
        s.tickets.edits.said = Some(Ok("Saving on the WSL machine…".into()));
    }
}

/// A Save's outcome R on Windows, as `save_kind_file` and `save_call` say it here (COPY: that it is the project's new
/// copy): the edit closes and coordd reads the files again, or the line says why not.
fn saved(s: &mut App, kind: bool, name: &str, why: &str, copy: &str, r: Result<String, String>) {
    let commit = r.as_ref().ok().and_then(|c| commit_of(c));
    let ok = r.is_ok();
    let said = r.map(|c| match s.tickets.edits.kind.as_ref().filter(|_| kind) {
        Some(e) => saved_words(e, &c, copy),
        None if kind => format!("Saved, {c}; applies to {name} from now.{copy}"),
        None => format!("Saved, {c}; applies to {name}'s next steps.{copy}"),
    });
    s.tickets.edits.said = Some(said);
    if !ok {
        return;
    }
    if kind {
        s.tickets.edits.kind = None;
    } else {
        s.tickets.edits.call_edit = None;
    }
    s.tickets.edits.why = TextEdit::new("");
    // the files changed: the next edit reads them again
    s.tickets.edits.served = None;
    reload(s, commit, why);
}

/// What Save says of kind edit E written as C (COPY: what the project's new copy means, `copy_words`): a clock's from coordd's next
/// read of it, a kind's for its tickets.
fn saved_words(e: &KindEdit, c: &str, copy: &str) -> String {
    match e.binding {
        Some(_) => format!("Saved, {c}; {} opens by its new line.", e.kind),
        None => format!("Saved, {c}; applies to {} from now.{copy}", e.kind),
    }
}

/// Whose FILE is, of the folders a project reads besides its own: the library's (its place in the snapshot's places;
/// from a coordd before them, here any folder that is not agent-base's checkout), else agent-base's. Of the project
/// shown ([`whose_in`]).
fn whose(s: &App, file: &Path) -> &'static str {
    whose_in(s, s.proj(), file)
}

/// [`whose`] of project P's FILE: the cloud's cards say it of the project whose island they are on (map/cloud.rs
/// `whose_words`, paper-research N-1446 p1844).
pub(crate) fn whose_in(s: &App, p: Option<&crate::Proj>, file: &Path) -> &'static str {
    // the places name the library's folder, so all else is agent-base's: a release's coordd reads the agent-base beside
    // it (coordd main.rs `beside`), not the checkout `s.base` names, so on Linux its base.kind read as the library's
    let lib = match p.and_then(|p| p.live.places.as_ref()) {
        Some(x) => x.library.as_deref().is_some_and(|l| file.starts_with(l)),
        None => !crate::elsewhere::away() && !file.starts_with(&s.base),
    };
    if lib { "the library" } else { "agent-base" }
}

/// What a project copy means (consult-3 p334), said in its editor and by its Save: from then on the project runs its own
/// TO, made from ORIGINAL, whose later changes no longer reach it while the copy is there; and the way back.
fn copy_words(s: &App, original: &Path, to: &Path) -> String {
    let project = s.proj().map_or("this project", |p| p.name.as_str());
    let name = to.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let whose = whose(s, original);
    format!("{project} now runs its own {name} ({}), not {whose}'s.", to.display())
}

/// The commit named in write_commit's answer ("commit abc1234…").
fn commit_of(said: &str) -> Option<String> {
    said.strip_prefix("commit ").map(|r| r.split([',', ' ']).next().unwrap_or(r).to_string()).filter(|c| !c.is_empty())
}

/// coordd asked to read the kinds and calls again after COMMIT; its answer is added to the Save's line. Each Save that
/// wrote comes here, on Linux and on Windows (`saved`).
fn reload(s: &mut App, commit: Option<String>, why: &str) {
    // a call saved has a new template: each step's prompt reads it again (cleared only on Windows before, so a step
    // saved on Linux kept drawing the old one: map/paths.rs, path 5)
    s.tickets.edits.once_of.lock().unwrap().clear();
    // the files changed: their docs are asked again
    if let Some(id) = s.proj().map(|p| p.id.clone()) {
        s.tickets.docs.forget(&id);
        s.tickets.labels.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
    }
    let req = coord_types::Request::Reload { commit, why: why.to_string() };
    if !s.request_in(s.cur, req, crate::Pending::TicketDone { clear: crate::threads::Cleared::Reload }) {
        if let Some(Ok(t)) = s.tickets.edits.said.take() {
            s.tickets.edits.said = Some(Err(format!("{t} Applies at the next start (disconnected).")));
        }
    }
}

/// The answer to a Reload or a Template: added to what the Save said (Reload), or the once-panel's line (Template).
pub fn replied(s: &mut App, reload: bool, ok: bool, out: &str) {
    // the files read again: the kinds and calls as coordd now reads them are asked for again (texts.rs), the ones shown
    // until they come
    if reload && ok {
        s.tickets.kinds_asked = false;
    }
    let e = &mut s.tickets.edits;
    let out = out.trim();
    if reload {
        let before = match e.said.take() {
            Some(Ok(t)) | Some(Err(t)) => format!("{t} "),
            None => String::new(),
        };
        // a reply ticket by ticket (live edits), else coordd's sentence as it is
        let reply = if ok { serde_json::from_str::<ReloadReply>(out).ok() } else { None };
        let words = reply.as_ref().map_or_else(|| out.to_string(), reload_summary);
        e.said = Some(if ok { Ok(format!("{before}Reloaded: {words}")) } else { Err(format!("{before}Not reloaded, old kept: {out}")) });
        if let Some(r) = reply {
            e.reloaded = Some((chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(), r));
        }
    } else {
        if ok {
            e.once = TextEdit::new("");
            e.why = TextEdit::new("");
        }
        e.said = Some(if ok { Ok(format!("Taken: {out}")) } else { Err(out.to_string()) });
    }
}

/// A step's call template and fields in project P, read once per call (the calls coordd reads for that project). They
/// are kept by project and call, since the Map can draw several steps' prompts at once, on every project's island
/// (map/deep.rs), and read again after a Save or once the files served change.
fn once_of(s: &App, p: &crate::Proj, call: &str) -> Option<(String, Vec<String>)> {
    let key = (p.id.clone(), call.to_string());
    let mut g = s.tickets.edits.once_of.lock().unwrap();
    if let Some(v) = g.get(&key) {
        return v.clone();
    }
    // on Windows the files P's coordd served (none yet, or another project's: nothing found, and nothing kept, so the
    // files' coming is seen)
    let away = crate::elsewhere::away();
    if away && s.tickets.edits.served.as_ref().is_none_or(|(id, _)| *id != p.id) {
        return None;
    }
    let src = if away { src_of(s, Some(p)) } else { Src::Here(folders(s, p)) };
    let (defs, _, _) = calls_of(&src);
    let v = defs.get(call).and_then(|d| Some((d.template.clone()?, d.sig.params.iter().map(|(n, _)| n.clone()).collect())));
    g.insert(key, v.clone());
    v
}

/// Windows: whether the Map's steps on the current project's island (PI) want its files for their prompts, which only
/// an edit's start asks for otherwise: they are not held, and its bridge serves them.
pub(crate) fn prompts_unserved(s: &App, pi: usize) -> bool {
    let Some(p) = s.projects.get(pi).filter(|_| pi == s.cur && crate::elsewhere::away() && s.tickets.picture_file.is_none()) else { return false };
    crate::files::served(p) && s.tickets.edits.served.as_ref().is_none_or(|(id, _)| *id != p.id)
}

/// Windows: ask the current project's coordd for its files, for the prompts the Map draws there (`Asked::Prompts`).
/// True when asked.
pub(crate) fn serve_prompts(s: &mut App) -> bool {
    s.request_in(s.cur, coord_types::Request::Sources, crate::Pending::Edit(Asked::Prompts))
}

/// Call CALL's prompt in project PI for a step drawn with it, when the call has a template there (else None: coordd
/// answers it itself, or it is not among the project's calls). A step on the Map draws it once near enough
/// (map/deep.rs; p715 level 6, path 5: "The template is there … Click Edit: it shows what it reaches … Save"), as a
/// ticket's step panel does ([`step_panel`]). EDITING: this step's edit is under way. PRESS: its press's key. STARTED:
/// what else its press marks, beside starting the edit.
///
/// A watcher's step with no template of its own draws the template of the call coordd runs for it ([`runs_for`], BY
/// the step's taker), saying so as the Docket's call panel does ([`runs_words`]), and its Edit edits that call's lines:
/// the template and the step's arguments, until the prompt as sent is read (proto-lead N-1446 p1112). These are
/// coordd's clerk.rs `RUNS`: a brain's `attend` and `elect` (`attention`, `election`), and base.kind's watcher steps
/// `intake`, `route` and `asks` (`sort`, `whom`, `find`; consult-3 p1160). The Kinds answer is the current project's,
/// so a step of another project's island draws none.
///
/// An edit opens on the current project's files and its Save writes there, so a step on another project's island
/// draws its prompt with no Edit, and says where it is edited (consult-3 N-1479 p27).
#[allow(clippy::too_many_arguments)]
pub(crate) fn prompt(s: &App, pi: usize, call: &str, by: Option<&str>, editing: bool, press: &str, started: impl Fn(&mut App) + 'static, k: &Kit) -> Option<Vec<El>> {
    let p = s.projects.get(pi)?;
    let runs = || (pi == s.cur).then(|| crate::kinds::said_call(s, call)).flatten().and_then(|c| runs_for(c, by)).map(|r| r.name.clone());
    let (call, template, ran) = match once_of(s, p, call) {
        Some((template, _)) => (call.to_string(), template, false),
        None => match runs().and_then(|r| Some((once_of(s, p, &r)?.0, r))) {
            Some((template, r)) => (r, template, true),
            None => {
                // on Windows only the current project's files are served here (consult-3 N-1479 p33)
                let away = pi != s.cur && crate::elsewhere::away();
                return away.then(|| vec![text(format!("Its prompt shows with {} open.", p.name)).font(k.ty.small).color(k.c.text_faint).wrap()]);
            }
        },
    };
    if pi != s.cur {
        return Some(vec![template_box(template, k), text(format!("Its prompt is edited with {} open.", p.name)).font(k.ty.small).color(k.c.text_faint).wrap()]);
    }
    let said = ran.then(|| text(runs_words(&call)).font(k.ty.small).color(k.c.text_muted).wrap());
    Some(said.into_iter().chain(prompt_of(s, &call, template, editing, press, started, k)).collect())
}

/// The call coordd runs for a step of call D taken by BY, when D has no template of its own (`CallForm::runs`): only for
/// a watcher's step, since coordd runs it only when the watcher takes the step. BY is None when nothing says who takes
/// it. The Docket's call panel and a step's prompt on the Map both read this, so the two can't part (consult-3 N-1446
/// p1160).
fn runs_for<'a>(d: &'a crate::kinds::CallForm, by: Option<&str>) -> Option<&'a crate::kinds::RunsCall> {
    d.runs.as_ref().filter(|_| d.template.is_none() && by.is_none_or(|w| w == docket_machine::WATCHER))
}

/// What a watcher's step with no template says of the call coordd runs for it, RUNS, above that call's template: on the
/// Docket's call panel and in a step on the Map.
fn runs_words(runs: &str) -> String {
    format!("Runs {runs}, with its template:")
}

/// A call's TEMPLATE as it stands, in the step panel's box.
fn template_box(template: String, k: &Kit) -> El {
    col().w_full().padding(10.0).radius(k.radius.sm).fill(k.c.layer_01).scroll_x().child(text(template).font(k.ty.mono).color(k.c.text_base))
}

/// TEMPLATE, call CALL's, with the press that edits it for the program (every step of it sent from now on; what that
/// reaches is asked as he types, `reach_poll`), or, while EDITING, its editor.
fn prompt_of(s: &App, call: &str, template: String, editing: bool, press: &str, started: impl Fn(&mut App) + 'static, k: &Kit) -> Vec<El> {
    if let Some(e) = s.tickets.edits.call_edit.as_ref().filter(|e| editing && e.call == call) {
        return vec![call_editor(s, e, k)];
    }
    let call = call.to_string();
    let edit = Button::new("Edit the prompt").variant(ButtonVariant::Contrast).on_press(move |s: &mut App| {
        started(s);
        start_call(s, &call)
    });
    vec![template_box(template, k), row().w_full().justify_end().child(edit.build(k).key(press.to_string()))]
}

/// The once-panel's Save: the prompt typed for the next step of the call on ticket ID (or every later one on it and the
/// tickets under it), sent to coordd.
fn save_once(s: &mut App, id: &str, call: &str, tree: bool) {
    match once_request(s, id, call, tree) {
        Ok(req) => _ = s.request_in(s.cur, req, crate::Pending::TicketDone { clear: crate::threads::Cleared::Once }),
        Err(e) => s.tickets.edits.said = Some(Err(e)),
    }
}

/// The request the once-panel's Save sends.
pub fn once_request(s: &App, id: &str, call: &str, tree: bool) -> Result<coord_types::Request, String> {
    let why = why_of(s).map_err(|_| "Say why first: the record line carries it.".to_string())?;
    let text = s.tickets.edits.once.text.clone();
    Ok(coord_types::Request::Template { t: id.to_string(), call: call.to_string(), scope: if tree { "tree" } else { "step" }.into(), text, why })
}

// ---- drawing --------------------------------------------------------------------------------------------------------

pub(crate) fn said(s: &App, k: &Kit) -> Option<El> {
    let line = crate::message_box::answer_line(s.tickets.edits.said.as_ref()?, k);
    // under the Save's line, the reload's reply ticket by ticket, each a way to its page
    let Some((_, r)) = s.tickets.edits.reloaded.as_ref().filter(|_| matches!(s.tickets.edits.said, Some(Ok(_)))) else { return Some(line) };
    let mut c = col().key("reload-reply").w_full().gap(4.0).child(line);
    for t in &r.tickets {
        let n = s.proj().and_then(|p| p.live.tickets.values().find(|v| v.id == t.id).map(|v| v.n));
        let id = text(t.id.clone()).font(k.ty.small_medium).color(if n.is_some() { k.c.text_accent } else { k.c.text_base });
        let id = match n {
            Some(n) => k.interactive(id).on_press(move |s: &mut App| s.tickets.shown = Some(n)),
            None => id,
        };
        c = c.child(row().w_full().gap(6.0).child(id).child(text(reloaded_words(t)).font(k.ty.small).color(if t.took { k.c.text_base } else { k.c.warning.fg }).wrap().grow(1.0).shrink()));
    }
    Some(c)
}

fn why_input(s: &App, k: &Kit) -> El {
    col().key("edit-why").w_full().child(TextInput::new(&s.tickets.edits.why).placeholder("Why: the commit says it").width(None).on_edit(|s: &mut App| &mut s.tickets.edits.why).build(k))
}

/// Kind E being edited: its text editable, and beside it the picture drawn from that text; under them the check, the
/// why, Save and Discard.
pub fn kind_editor(s: &App, e: &KindEdit, k: &Kit) -> El {
    let words = |x: &serde_json::Value| crate::texts::words(&s.tickets, x);
    let texts = crate::picture::kind_texts(&s.tickets);
    let pic = crate::picture::Pic { k, p: s.proj(), kinds: texts, tickets: Vec::new(), open: &s.tickets.pic_open, marks: marks(&s.tickets), words: &words, pick: None, picked: None };
    let status = kind_status(e, k);
    let bar = kind_bar(s, e, status, k);
    let area = col().key(format!("edit-kind-{}", e.kind)).w_full().child(TextArea::new(&e.text).width(None).spellcheck(false).spans(code_spans(&e.text.text, false, k)).auto_grow(8, 22).on_edit(|s: &mut App| &mut s.tickets.edits.kind.as_mut().unwrap().text).build(k));
    let pic_el = match crate::picture::parse_kind(&e.text.text) {
        Ok(_) => pic.kind(&e.text.text, None, &format!("edit-{}", e.kind)),
        Err(err) => text(format!("Not drawn while it does not parse: {err}")).font(k.ty.small).color(k.c.danger.fg).wrap(),
    };
    col().w_full().gap(10.0).child(bar).child(area).child(text("Its picture:".to_string()).font(k.ty.small).color(k.c.text_muted)).child(pic_el)
}

/// Kind edit E's bar: STATUS (what it is, its check), what it applies to, what it reaches (coordd's), the why,
/// Discard and Save. The kind editor's, and a kind's lines edited in place (kinds.rs's card).
pub fn kind_bar(s: &App, e: &KindEdit, status: El, k: &Kit) -> El {
    let added = e.check();
    let refused = s.tickets.edits.reach.as_ref().is_some_and(|(t, r)| r.is_err() && *t == e.whole());
    let applies = if e.binding.is_some() {
        "Applies on Save."
    } else {
        "Applies after Save, to new and open tickets."
    };
    let status = status.child(text(applies).font(k.ty.small).color(k.c.text_muted).wrap());
    col()
        .key("kind-bar")
        .w_full()
        .gap(8.0)
        .padding(12.0)
        .radius(k.radius.md)
        .fill(k.c.layer_02)
        .border(1.0, k.c.border_focus)
        .child(
            row()
                .w_full()
                .gap(8.0)
                .child(status)
                .child(Button::new("Discard").variant(ButtonVariant::Neutral).on_press(|s: &mut App| s.tickets.edits.kind = None).build(k))
                .child(Button::new("Save to the project").variant(ButtonVariant::Contrast).disabled(!added.is_empty() || refused).on_press(save_kind).build(k).key("kind-save")),
        )
        .child(reach_part(s, k))
        .child(why_input(s, k))
}

/// The clock's line being edited in place (`start_binding`), under the Docket's board where its start was pressed: the
/// line in a box, and the kind bar (its check, what it reaches, the why, Discard and Save). None while no clock is.
pub fn clock_editor(s: &App, k: &Kit) -> Option<El> {
    let e = s.tickets.edits.kind.as_ref().filter(|e| e.binding.is_some())?;
    let area = col().key("edit-clock").w_full().child(TextArea::new(&e.text).width(None).spellcheck(false).spans(code_spans(&e.text.text, false, k)).auto_grow(1, 4).on_edit(|s: &mut App| &mut s.tickets.edits.kind.as_mut().unwrap().text).build(k));
    Some(col().key("clock-editor").w_full().gap(8.0).child(area).child(kind_bar(s, e, kind_status(e, k), k)))
}

/// Kind edit E's lines in place in the kind's text KIND_TEXT (as coordd read it): the lines before and after them as
/// they are, the edited ones in a box between (p294).
pub fn lines_in_place(e: &KindEdit, kind_text: &str, k: &Kit) -> El {
    let (a, b) = e.lines.unwrap_or((1, 0));
    let ls: Vec<&str> = kind_text.trim_end().lines().collect();
    let (before, after) = (ls[..(a.max(1) - 1).min(ls.len())].join("\n"), ls[b.min(ls.len())..].join("\n"));
    let mono = |t: String| text(t).font(k.ty.mono).color(k.c.text_muted);
    let mut c = col().key(format!("edit-lines-{}", e.kind)).w_full().gap(2.0);
    if !before.is_empty() {
        c = c.child(mono(before));
    }
    let rows = (b + 1).saturating_sub(a).max(1);
    c = c.child(col().key(format!("edit-kind-{}", e.kind)).w_full().child(TextArea::new(&e.text).width(None).spellcheck(false).spans(code_spans(&e.text.text, false, k)).auto_grow(rows, 24).on_edit(|s: &mut App| &mut s.tickets.edits.kind.as_mut().unwrap().text).build(k)));
    if !after.is_empty() {
        c = c.child(mono(after));
    }
    c
}

/// Kind edit E's status: where it is saved, and its check.
pub fn kind_status(e: &KindEdit, k: &Kit) -> El {
    let added = e.check();
    let what = match e.lines {
        _ if e.binding.is_some() => format!("Editing {}'s clock, line {} of", e.kind, e.orig[..e.span.0].matches('\n').count() + 1),
        Some((a, b)) if a == b => format!("Editing line {a} of kind {} in place", e.kind),
        Some((a, b)) => format!("Editing lines {a}–{b} of kind {} in place", e.kind),
        None => format!("Editing kind {} permanently", e.kind),
    };
    let to = match (e.new, e.binding.is_some()) {
        (true, _) => format!("{what}: saved as the project's own copy, {}", e.file.display()),
        (false, true) => format!("{what} {}", e.file.display()),
        (false, false) => format!("{what}: {}", e.file.display()),
    };
    let status = col().grow(1.0).shrink().min_w(0.0).gap(4.0).child(text(to).font(k.ty.small_medium).color(k.c.text_base).wrap());
    let status = status.children(e.copy.as_ref().map(|c| text(c.clone()).font(k.ty.small).color(k.c.text_base).wrap().key("edit-copy")));
    if added.is_empty() {
        status.child(text("✓ fits the project".to_string()).font(k.ty.small).color(k.c.success.fg))
    } else {
        added.iter().fold(status, |f, e| f.child(text(format!("✗ {e}")).font(k.ty.small).color(k.c.danger.fg).wrap()))
    }
}

/// TEXT in the mono face; with SLOTS, a template's `{{ … }}` in the accent colour.
fn code_spans(text: &str, slots: bool, k: &Kit) -> Vec<rune::Span> {
    let mut out = vec![rune::Span::new(0, text.len()).mono()];
    if slots {
        let mut at = 0;
        while let Some(a) = text[at..].find("{{") {
            let s0 = at + a;
            let Some(b) = text[s0..].find("}}") else { break };
            out.push(rune::Span::new(s0, s0 + b + 2).mono().color(k.c.text_accent));
            at = s0 + b + 2;
        }
    }
    out
}

/// The template of the call a step was clicked for: read, or being edited.
/// Call edit E in place: the call's lines after its signature in a box, the check, what it reaches (coordd's), the
/// why, Discard and Save. The call panel's, and a ticket's step panel's for the program.
fn call_editor(s: &App, e: &CallEdit, k: &Kit) -> El {
    let call = &e.call;
    let bad = bad_slots(&e.text.text, &e.params);
    let added = e.check();
    // where Save writes: the file the call is in, or the project's own copy of agent-base's or the library's
    let written = call_written(s, e);
    let to = match &written {
        Ok((to, None, _)) => format!("saved as the project's own copy, {}", to.display()),
        _ => e.file.display().to_string(),
    };
    let mut c = col()
        .key("call-editor")
        .w_full()
        .gap(8.0)
        .child(text(format!("Editing call {call} in place: {to}")).font(k.ty.small_medium).color(k.c.text_base).wrap())
        .children(match &written {
            Ok((to, None, _)) => Some(text(copy_words(s, &e.file, to)).font(k.ty.small).color(k.c.text_base).wrap().key("edit-copy")),
            _ => None,
        })
        .child(col().key("edit-call").w_full().child(TextArea::new(&e.text).width(None).spellcheck(false).spans(code_spans(&e.text.text, true, k)).auto_grow(10, 40).on_edit(|s: &mut App| &mut s.tickets.edits.call_edit.as_mut().unwrap().text).build(k)))
        .child(text(format!("Its slots are the call's fields: {}", e.params.join(", "))).font(k.ty.small).color(k.c.text_muted).wrap());
    c = if !bad.is_empty() {
        c.child(text(format!("✗ not fields of {call}: {}", bad.join(", "))).font(k.ty.small).color(k.c.danger.fg).wrap())
    } else if !added.is_empty() {
        added.iter().fold(c, |c, e| c.child(text(format!("✗ {e}")).font(k.ty.small).color(k.c.danger.fg).wrap()))
    } else {
        c.child(text("✓ slots and calls check".to_string()).font(k.ty.small).color(k.c.success.fg))
    };
    // coordd's refusal of this very text holds Save until the text changes
    let refused = s.tickets.edits.reach.as_ref().is_some_and(|(t, r)| r.is_err() && written.as_ref().is_ok_and(|(_, _, w)| w == t));
    c.child(text("Applies to every step after Save.").font(k.ty.small).color(k.c.text_muted).wrap())
        .child(reach_part(s, k))
        .child(dry_part(s, k))
        .child(why_input(s, k))
        .child(
            row()
                .w_full()
                .gap(8.0)
                .justify_end()
                .child(Button::new("Discard").variant(ButtonVariant::Neutral).on_press(|s: &mut App| s.tickets.edits.call_edit = None).build(k))
                .child(Button::new("Save for the program").variant(ButtonVariant::Contrast).disabled(!bad.is_empty() || !added.is_empty() || refused).on_press(save_call).build(k).key("edit-call-save")),
        )
}

pub fn call_panel(s: &App, k: &Kit) -> Option<El> {
    let call = s.tickets.edits.call.clone()?;
    let mut c = col().key("call-panel").w_full().gap(8.0).padding(14.0).radius(k.radius.md).fill(k.c.layer_02).border(1.0, k.c.border_focus);
    // as coordd read it (the Kinds answer, paper-research N-1396 p174); from a coordd before that, the files here
    let form = crate::kinds::said_call(s, &call).cloned().or_else(|| calls_of(&src_of(s, s.proj())).0.get(&call).map(crate::kinds::CallForm::of));
    // who takes the step whose call this is, as its picture names it (`open_call`)
    let by = crate::picture::docket::picked(&s.tickets.pic_open, crate::picture::docket::CALL_BY);
    // the call whose lines a Save writes: its own; a watcher's step with no template, the call coordd runs for it, by
    // `runs_for`, so an agent's step on such a call edits nothing of the call coordd runs (consult-3 N-1446 p1182)
    let target = form.as_ref().and_then(|d| runs_for(d, by)).map_or_else(|| call.clone(), |r| r.name.clone());
    if let Some(e) = s.tickets.edits.call_edit.as_ref().filter(|e| e.call == target) {
        if target != call {
            c = c.child(text(format!("call {call} runs {target}; its lines:")).font(k.ty.small).color(k.c.text_muted).wrap());
        }
        c = c.child(call_editor(s, e, k));
    } else {
        match form {
            Some(d) => {
                let small = |t: String| text(t).font(k.ty.small).color(k.c.text_muted).wrap();
                let mono = |t: String| col().w_full().padding(10.0).radius(k.radius.sm).fill(k.c.layer_01).scroll_x().child(text(t).font(k.ty.mono).color(k.c.text_base));
                // a call with no template of its own is no step's prompt: coordd runs another for it, or answers it
                let prompt = if d.template.is_some() { format!(" · the prompt of every step of {call}") } else { String::new() };
                c = c.child(row().w_full().gap(8.0).child(text(format!("call {call}")).font(k.ty.ui_medium).color(k.c.text_base)).child(small(format!("{}, line {}{prompt}", d.file, d.line)).grow(1.0).shrink()));
                if !d.doc.trim().is_empty() {
                    c = c.child(text(d.doc.trim().to_string()).font(k.ty.small).color(k.c.text_base).wrap());
                }
                c = c.child(col().w_full().scroll_x().child(text(d.sig.clone()).font(k.ty.mono).color(k.c.text_base)));
                // the model it runs on (paper-research p235); a line that names the task says it for the attrs too
                let model = crate::kinds::model_words(s.proj(), &d, by);
                let attrs: Vec<String> = d
                    .attrs
                    .iter()
                    .filter(|(w, r)| !(w == "task" && model.as_deref().is_some_and(|m| m.starts_with(&format!("task {r}")))))
                    .map(|(w, r)| if r.is_empty() { w.clone() } else { format!("{w} {r}") })
                    .collect();
                if !attrs.is_empty() {
                    c = c.child(small(attrs.join(" · ")));
                }
                if let Some(m) = model {
                    c = c.child(text(m).font(k.ty.small).color(k.c.text_base).wrap().key("call-model"));
                }
                // its template as written; a watcher's step with none, the template of the call coordd runs for it; else who
                // answers it: its taker, or coordd for the watcher's (p1182)
                c = match (&d.template, runs_for(&d, by)) {
                    (Some(t), _) => c.child(mono(t.clone())),
                    (None, Some(r)) => c.child(small(runs_words(&r.name))).child(mono(r.template.clone().unwrap_or_default())),
                    (None, None) => c.child(mono(match by.filter(|w| *w != docket_machine::WATCHER) {
                        Some(w) => format!("(no template: answered by {})", crate::refs::person(w)),
                        None => "(built in)".into(),
                    })),
                };
                let name = target.clone();
                let mut r = row().w_full().gap(8.0).center_y();
                // its lines edited in place: its own template, or the one of the call coordd runs for a watcher's step
                if d.template.is_some() || runs_for(&d, by).is_some() {
                    r = r
                        .child(Button::new("Edit the template").variant(ButtonVariant::Contrast).on_press(move |s: &mut App| start_call(s, &name)).build(k).key("call-edit"))
                        .child(text("Every step from now on").font(k.ty.small).color(k.c.text_muted).wrap().grow(1.0).shrink());
                } else {
                    r = r.child(col().grow(1.0));
                }
                c = c.child(r.child(Button::new("Close").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.tickets.edits.call = None).build(k)));
            }
            None => c = c.child(text(format!("call {call} is not among this project's calls")).font(k.ty.small).color(k.c.danger.fg)),
        }
    }
    if let Some(e) = said(s, k) {
        c = c.child(e);
    }
    Some(c)
}

/// A ticket page's step panel: the step clicked in its picture, edited once.
pub fn step_panel(s: &App, v: &coord_types::TicketView, k: &Kit) -> Option<El> {
    let st = s.tickets.edits.step.clone().filter(|st| st.t == v.n)?;
    let only = s.tickets.edits.only_thread;
    let whom = format!("{} and the tickets under it", v.id);
    let mut c = col().key("step-panel").w_full().gap(8.0).padding(14.0).radius(k.radius.md).fill(k.c.layer_02).border(1.0, k.c.border_focus);
    let what = st.inline.clone().map_or_else(|| format!("{}(…)", st.call), |t| format!("“{t}”"));
    c = c
        .child(row().w_full().gap(8.0).child(text(format!("{}: {what}", st.worker)).font(k.ty.small_medium).color(k.c.text_base).wrap().grow(1.0).shrink()).child(Button::new("Close").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.tickets.edits.step = None).build(k)))
        .child(text("Its prompt".to_string()).font(k.ty.small_medium).color(k.c.text_muted));
    match (&st.inline, s.proj().and_then(|p| once_of(s, p, &st.call))) {
        (Some(t), _) => {
            c = c
                .child(text(t.clone()).font(k.ty.small).color(k.c.text_base).wrap())
                .child(text("Inline prompt: edit it in the kind.").font(k.ty.small).color(k.c.text_faint).wrap());
        }
        (None, None) => {
            c = c.child(text(format!("call {} has no template to edit.", st.call)).font(k.ty.small).color(k.c.text_faint).wrap());
        }
        (None, Some((call_template, params))) => {
            // the owner's once-edit that reaches this step, nearest first (the ticket's view, from the record): what
            // coordd makes its prompt from
            let saved = v.templates.iter().find(|t| t.call == st.call).cloned();
            if let Some(t) = &saved {
                let reach = match (t.step, t.tree) {
                    (Some(n), _) => format!("step {n} of {}, owed now", v.id),
                    (None, true) => format!("{}'s next steps on {} and below", st.call, t.on),
                    (None, false) => format!("the next step of {} on {}", st.call, t.on),
                };
                c = c.child(text(format!("Your version for {reach}, in place of the call's.")).font(k.ty.small).color(k.c.success.fg).wrap());
            }
            // one Save: for the program (the call's lines, as on Project) or only this thread (p296)
            c = c
                .child(RadioItem::new(format!("Every {} step from now on", st.call), !only).on_select(|s: &mut App| s.tickets.edits.only_thread = false).build(k).key("step-for-program"))
                .child(RadioItem::new(format!("Only this thread: {whom}"), only).on_select(|s: &mut App| s.tickets.edits.only_thread = true).build(k).key("step-for-thread"));
            if !only {
                let editing = s.tickets.edits.call_edit.as_ref().is_some_and(|e| e.call == st.call);
                c = c.children(prompt_of(s, &st.call, call_template, editing, "step-edit-program", |_| {}, k));
            } else {
                let template = thread_template(v, &st.call, call_template);
                let once = if s.tickets.edits.once.text.is_empty() { TextEdit::new(template.clone()) } else { s.tickets.edits.once.clone() };
                let bad = bad_slots(&once.text, &params);
                let same = once.text == template;
                let base = template.clone();
                c = c
                    .child(col().key("step-once").w_full().child(TextArea::new(&once).width(None).spellcheck(false).spans(code_spans(&once.text, true, k)).auto_grow(3, 16).on_edit(move |s: &mut App| {
                        if s.tickets.edits.once.text.is_empty() {
                            s.tickets.edits.once = TextEdit::new(base.clone());
                        }
                        &mut s.tickets.edits.once
                    }).build(k)))
                    .child(if bad.is_empty() {
                        text(format!("✓ slots fit {}: {}", st.call, params.join(", "))).font(k.ty.small).color(k.c.success.fg).wrap()
                    } else {
                        text(format!("✗ not fields of {}: {}", st.call, bad.join(", "))).font(k.ty.small).color(k.c.danger.fg).wrap()
                    })
                    .child(dry_part(s, k))
                    .child(why_input(s, k));
                let (id, call) = (v.id.clone(), st.call.clone());
                let note = if same { "Change the prompt to save it." } else { "The call's file stays as it is." };
                c = c.child(row().w_full().gap(8.0).center_y().child(text(note.to_string()).font(k.ty.small).color(k.c.text_faint).wrap().grow(1.0).shrink()).child(
                    Button::new("Save for this thread only").variant(ButtonVariant::Contrast).disabled(same || !bad.is_empty()).on_press(move |s: &mut App| save_once(s, &id, &call, true)).build(k).key("step-save-thread"),
                ));
            }
        }
    }
    if st.awaited {
        if let Some(cont) = v.continuation.clone() {
            let n = v.n;
            let pg = &s.tickets;
            let e = pg.edit.get(&n).cloned().unwrap_or_else(|| TextEdit::new(cont.clone()));
            let why = pg.why.get(&n).cloned().unwrap_or_else(|| TextEdit::new(""));
            c = c
                .child(text(format!("What follows this step, for {} only", v.id)).font(k.ty.small_medium).color(k.c.text_muted))
                .child(text("`answer` is the step's answer.").font(k.ty.small).color(k.c.text_muted).wrap())
                .child(col().key(format!("step-edit-{}", v.id)).w_full().child(TextArea::new(&e).width(None).spellcheck(false).wrap_anywhere().auto_grow(6, 24).on_edit(move |s: &mut App| s.tickets.edit.entry(n).or_insert_with(|| TextEdit::new(cont.clone()))).build(k)))
                .child(col().key(format!("step-why-{}", v.id)).w_full().child(TextInput::new(&why).label("Why").width(None).on_edit(move |s: &mut App| s.tickets.why.entry(n).or_insert_with(|| TextEdit::new(""))).build(k)))
                .child(row().w_full().justify_end().child(Button::new(format!("Edit what follows, for {}", v.id)).variant(ButtonVariant::Neutral).on_press(move |s: &mut App| crate::ticket::change_edit(s, n)).build(k)));
        }
    } else {
        c = c.child(text("Editable while the ticket waits.").font(k.ty.small).color(k.c.text_faint).wrap());
    }
    let kind = v.kind.clone();
    let mut perm = row().w_full().gap(12.0).center_y().child(text("Permanently instead:".to_string()).font(k.ty.small_medium).color(k.c.text_muted));
    perm = perm.child(k.interactive(text(format!("kind {kind} ›")).font(k.ty.small).color(k.c.text_accent)).on_press(move |s: &mut App| {
        s.sel = crate::Sel::Kinds;
        s.kinds_open.insert(kind.clone());
        start_kind(s, &kind, None);
    }));
    c = c.child(perm);
    if let Some(e) = said(s, k) {
        c = c.child(e);
    }
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On Windows a kind is edited from the files the project's coordd served (`Sources`), and its Save closes it once
    /// coordd's `EditFile` answers; a call's project copy goes where coordd places the project's calls (N-744).
    #[test]
    fn on_windows_a_kind_is_edited_from_the_files_coordd_served() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects.push(crate::Proj::new(&crate::projects::ProjectConfig { name: "w".into(), id: "w".into(), boxed: false }, Vec::new()));
        s.cur = s.projects.len() - 1;
        let cur = s.cur;
        s.projects[cur].protocol = crate::files::FILES;
        s.projects[cur].live.places = Some(coord_types::Places { coord: "/c".into(), definition: Some("/srv/proj".into()), ..Default::default() });
        let text = "kind a\n  pattern\n    return 1\n";
        let served = serde_json::json!({"kinds": [{"path": "/base/kinds/b.kind", "text": "kind b\n  pattern\n    return 2\n"}, {"path": "/srv/proj/kinds/a.kind", "text": text}], "calls": []}).to_string();
        crate::elsewhere::as_on_windows(|| {
            // the files are asked for first (no pipe here: nothing opens yet)
            start_kind(&mut s, "a", None);
            assert!(s.tickets.edits.kind.is_none());
            came(&mut s, cur, Asked::Sources { kind: true, name: "a".into() }, true, served.clone());
            let e = s.tickets.edits.kind.as_ref().expect("the kind opens from the files served");
            assert_eq!((e.file.to_string_lossy().replace('\\', "/"), e.orig.as_str(), e.text.text.as_str()), ("/srv/proj/kinds/a.kind".to_string(), text, text.trim_end()));
            assert!(matches!(e.src, Src::There(_)) && e.check().is_empty());
            // agent-base's b: the project's own copy, its lines only, a new file where coordd places the project's kinds
            came(&mut s, cur, Asked::Sources { kind: true, name: "b".into() }, true, served.clone());
            let e = s.tickets.edits.kind.as_ref().expect("agent-base's kind opens as the project's copy");
            assert_eq!((e.file.to_string_lossy().replace('\\', "/"), e.new, e.text.text.as_str()), ("/srv/proj/kinds/b.kind".to_string(), true, "kind b\n  pattern\n    return 2"));
            assert!(e.check().is_empty(), "the copy replaces agent-base's by name: {:?}", e.check());
            let means = "w now runs its own b.kind (/srv/proj/kinds/b.kind), not agent-base's.";
            assert_eq!(e.copy.as_deref().map(|c| c.replace('\\', "/")), Some(means.to_string()), "what the copy means, and the way back (consult-3 p334)");
            let r = edit_file(&e.file, (!e.new).then_some(e.orig.as_str()), &e.whole(), "m");
            assert!(matches!(r, coord_types::Request::EditFile { ref path, old: None, ref text, .. } if path == "/srv/proj/kinds/b.kind" && text == "kind b\n  pattern\n    return 2\n"), "{r:?}");
            // its Save on the WSL machine says it is the project's copy, and where, as a Save here does
            came(&mut s, cur, Asked::Saved { kind: true, name: "b".into(), why: "w".into(), copy: format!(" {means}") }, true, "commit abc1234".into());
            assert!(matches!(&s.tickets.edits.said, Some(Ok(t)) | Some(Err(t)) if t.starts_with("Saved, commit abc1234; applies to b from now.") && t.contains(means)), "{:?}", s.tickets.edits.said);
            came(&mut s, cur, Asked::Sources { kind: true, name: "a".into() }, true, served.clone());
            // EditFile's answer closes it, and the next edit reads the files again
            saved(&mut s, true, "a", "why", "", Ok("commit abc1234".into()));
            assert!(s.tickets.edits.kind.is_none() && s.tickets.edits.served.is_none());
            assert!(matches!(&s.tickets.edits.said, Some(Ok(t)) | Some(Err(t)) if t.starts_with("Saved, commit abc1234;")), "{:?}", s.tickets.edits.said);
            // a refusal keeps the edit's line
            saved(&mut s, false, "post", "why", "", Err("x.call changed since it was read: open the edit again".into()));
            assert_eq!(s.tickets.edits.said, Some(Err("x.call changed since it was read: open the edit again".into())));
        });
        let srv: Served = serde_json::from_str(&served).unwrap();
        assert_eq!(there_place(Path::new("/srv/proj"), &srv, "calls").join_server("post.call"), PathBuf::from("/srv/proj/calls/post.call"));
        assert_eq!(there_place(Path::new("/srv/proj"), &srv, "kinds").join_server("b.kind"), PathBuf::from("/srv/proj/kinds/b.kind"));
        assert_eq!(there_place(Path::new("/elsewhere/"), &srv, "calls").join_server("post.call"), PathBuf::from("/elsewhere/post.call"));
        let r = edit_file(Path::new("/srv/proj/kinds/a.kind"), Some("old"), "new", "m");
        assert!(matches!(r, coord_types::Request::EditFile { ref path, old: Some(ref o), .. } if path == "/srv/proj/kinds/a.kind" && o == "old"));
    }

    /// A reload's reply ticket by ticket (N-235's shape) reads as one line of counts and a line per ticket; a reply
    /// that is coordd's sentence stays that sentence.
    #[test]
    fn a_reload_reply_reads_ticket_by_ticket() {
        let out = r#"{"kinds":["change"],"calls":[],"tickets":[{"id":"C-3","kind":"change","took":true,"said":"x","withdrawn":[{"step":4,"call":"assign_parts","worker":"orch"}]},{"id":"C-5","kind":"change","took":false,"why":"its new remainder reads `parts`, never bound"}]}"#;
        let r: ReloadReply = serde_json::from_str(out).unwrap();
        assert_eq!(reload_summary(&r), "kinds change; 1 open took it, 1 kept the old");
        assert_eq!(reloaded_words(&r.tickets[0]), "took the new text; withdrawn, no place in the new text: step 4 (assign_parts, to orch)");
        assert_eq!(reloaded_words(&r.tickets[1]), "kept its old text: its new remainder reads `parts`, never bound");
        assert!(serde_json::from_str::<ReloadReply>("Read again: 2 kinds.").is_err());
    }

    #[test]
    fn a_kind_runs_to_the_next_item() {
        let t = "// the first\nkind a\n  form x: Text\n\n  pattern\n    return x\n\n// the second\nkind ab\n  form y: Text\n";
        let (s, e) = item_span(t, "kind a").unwrap();
        assert_eq!(&t[s..e], "kind a\n  form x: Text\n\n  pattern\n    return x");
        let (s, e) = item_span(t, "kind ab").unwrap();
        assert_eq!(&t[s..e], "kind ab\n  form y: Text");
        assert!(item_span(t, "kind b").is_none());
    }

    /// A call's lines after its signature (p294: its `task`, `model` and template are edited in place), from its first
    /// attribute line, past a signature that runs over several lines.
    #[test]
    fn a_calls_lines_after_its_signature_and_their_indent() {
        let t = "call mind(moment: Text) -> Rest\n  task mind\n  template\n    The moment: {{ moment }}\n\n    Go on.\n\ncall other() -> Rest\n";
        let ((s, e), ind) = body_span(t, "mind").unwrap();
        assert_eq!(&t[s..e], "  task mind\n  template\n    The moment: {{ moment }}\n\n    Go on.");
        assert_eq!(ind, "  ");
        let t = "call stop(\n    place: Text,\n    task: Text) -> Rest\n  task stopped\n";
        let ((s, e), _) = body_span(t, "stop").unwrap();
        assert_eq!(&t[s..e], "  task stopped");
        assert!(body_span("call other() -> Rest\n", "other").is_none(), "a call with no lines after its signature has none to edit");
    }

    /// A kind's lines edited in place (p294): agent-base's kind opens as the project's own copy, the lines pressed in
    /// the box and the rest as it is; it checks, and Save writes the copy with his words in those lines.
    // `cfg(not(windows))`: not in the Windows test exe (consult-3 N-1479 p191). It edits from this machine's files, and
    // the Windows window edits from the files coordd served (`elsewhere::LINE` with no coordd): that is
    // `on_windows_a_kind_is_edited_from_the_files_coordd_served`.
    #[test]
    #[cfg(not(windows))]
    fn a_kinds_lines_are_edited_in_place_and_agent_bases_is_saved_as_the_projects_copy() {
        let d = std::env::temp_dir().join(format!("yaaihh-lines-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let (base, proj) = (d.join("base"), d.join("proj"));
        for f in [base.join("kinds"), base.join("calls"), proj.join("kinds"), proj.join("calls")] {
            std::fs::create_dir_all(f).unwrap();
        }
        let kind = "kind k\n  form x: Text, y: Text\n  result Text\n  pattern\n    return x\n";
        std::fs::write(base.join("kinds/k.kind"), format!("// agent-base's\n{kind}")).unwrap();
        let (_held, mut s) = crate::view_cost::test_app();
        s.base = base.display().to_string();
        s.projects.push(crate::Proj::new(&crate::projects::ProjectConfig { name: "l".into(), id: "l".into(), boxed: false }, Vec::new()));
        s.cur = s.projects.len() - 1;
        let cur = s.cur;
        s.projects[cur].live.places = Some(coord_types::Places { coord: "/c".into(), definition: Some(proj.display().to_string()), ..Default::default() });
        // line 5, `return x`, as coordd's text of the kind has it
        start_kind_lines(&mut s, "k", kind, (5, 5));
        let e = s.tickets.edits.kind.as_ref().unwrap_or_else(|| panic!("the line opens in place: {:?}", s.tickets.edits.said));
        assert_eq!((e.file.clone(), e.new, e.lines, e.text.text.as_str()), (proj.join("kinds/k.kind"), true, Some((5, 5)), "    return x"));
        assert!(e.check().is_empty(), "{:?}", e.check());
        assert_eq!(reach_of(&s).map(|(f, _)| f), Some(proj.join("kinds/k.kind")), "a Reach asks of the file Save writes");
        // a text that does not check holds Save, in the checker's words, and asks no Reach
        s.tickets.edits.kind.as_mut().unwrap().text = TextEdit::new("    return z");
        assert!(!s.tickets.edits.kind.as_ref().unwrap().check().is_empty() && reach_of(&s).is_none(), "z is bound nowhere");
        s.tickets.edits.kind.as_mut().unwrap().text = TextEdit::new("    return y");
        assert!(s.tickets.edits.kind.as_ref().unwrap().check().is_empty(), "{:?}", s.tickets.edits.kind.as_ref().unwrap().check());
        s.tickets.edits.why = TextEdit::new("one more");
        save_kind(&mut s);
        assert_eq!(std::fs::read_to_string(proj.join("kinds/k.kind")).unwrap(), "kind k\n  form x: Text, y: Text\n  result Text\n  pattern\n    return y\n");
        let means = format!("l now runs its own k.kind ({}), not agent-base's.", proj.join("kinds/k.kind").display());
        assert!(matches!(&s.tickets.edits.said, Some(Ok(t)) | Some(Err(t)) if t.contains(&means)), "{:?}", s.tickets.edits.said);
        // lines that are not the file's: refused, Edit kind opens it whole
        start_kind_lines(&mut s, "k", "kind k\n  form x: Text, y: Text\n  result Text\n  pattern\n    return 7\n", (5, 5));
        assert!(matches!(&s.tickets.edits.said, Some(Err(w)) if w.contains("file changed; Edit kind opens it whole")), "{:?}", s.tickets.edits.said);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A Reach's answer (docket-edge5's reach, N-1396 p305, p307): each ticket's case and coordd's words, `took` only
    /// for a ticket of an edited kind; a refusal is coordd's sentence.
    #[test]
    fn a_reach_reads_each_tickets_case() {
        let (_held, mut s) = crate::view_cost::test_app();
        let out = r#"{"kinds":["fix"],"calls":["repair"],"tickets":[{"id":"F-1","kind":"fix","took":true,"reach":"ahead","said":"repair is ahead"},{"id":"F-2","kind":"fix","reach":"own","said":"its own prompt stands"},{"id":"F-5","kind":"fix","took":false,"reach":"kept","why":"its form changes"}],"clocks":[{"binding":"every 30m open tidy()","next":"2026-11-20T19:00:00Z"}]}"#;
        came(&mut s, 0, Asked::Reach { text: "x".into() }, true, out.into());
        let Some((t, Ok(r))) = s.tickets.edits.reach.as_ref() else { panic!("{:?}", s.tickets.edits.reach) };
        assert_eq!((t.as_str(), r.tickets.len(), r.tickets[1].took, r.tickets[1].reach.as_deref(), r.clocks[0].binding.as_str()), ("x", 3, false, Some("own"), "every 30m open tidy()"));
        came(&mut s, 0, Asked::Reach { text: "y".into() }, false, "nothing is read again, the kinds and calls stay as they were: the call repair is gone\n".into());
        assert_eq!(s.tickets.edits.reach, Some(("y".into(), Err("nothing is read again, the kinds and calls stay as they were: the call repair is gone".into()))));
    }

    /// Save: the file written and committed in its repository, and refused when the file changed since it was read.
    // `cfg(not(windows))`: not in the Windows test exe (consult-3 N-1479 p191). It makes its repository with git, which
    // the machine that runs that exe does not have ("program not found"); a Windows Save goes through coordd's EditFile.
    #[test]
    #[cfg(not(windows))]
    fn a_save_is_a_commit_of_that_file_only() {
        let d = std::env::temp_dir().join(format!("yaaihh-edit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("kinds")).unwrap();
        let git = |a: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(&d).args(a).output().unwrap().status.success(), "{a:?}");
        git(&["init", "-q"]);
        git(&["config", "user.name", "t"]);
        git(&["config", "user.email", "t@t"]);
        let f = d.join("kinds/a.kind");
        std::fs::write(&f, "kind a\n  pattern\n    return 1\n").unwrap();
        std::fs::write(d.join("other.txt"), "x").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-qm", "base"]);
        std::fs::write(d.join("other.txt"), "changed, not to be committed").unwrap();
        let r = write_commit(&f, Some("kind a\n  pattern\n    return 1\n"), "kind a\n  pattern\n    return 2\n", "kind a: two").unwrap();
        assert!(r.starts_with("commit "), "{r}");
        let show = std::process::Command::new("git").arg("-C").arg(&d).args(["show", "--stat", "--format=%s", "HEAD"]).output().unwrap();
        let show = String::from_utf8_lossy(&show.stdout);
        assert!(show.contains("kind a: two") && show.contains("kinds/a.kind") && !show.contains("other.txt"), "{show}");
        // refused in the words coordd's EditFile uses: the file by its folder and name, not its whole path (N-802)
        assert_eq!(write_commit(&f, Some("kind a\n  pattern\n    return 1\n"), "x", "m"), Err("Not saved: kinds/a.kind changed since you opened it. Open the edit again to start from its new text.".into()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn slots_not_fields() {
        let p = vec!["moment".to_string(), "seen".to_string(), "parts".to_string()];
        assert!(bad_slots("{{ moment }} {{ if seen }}{{ seen }}{{ end }} {{ for x in parts }}{{ x.name }}{{ end }}", &p).is_empty());
        assert_eq!(bad_slots("{{ moment }} {{ when }} {{ if who }}{{ end }}", &p), vec!["when".to_string(), "who".to_string()]);
    }
}

/// A scene's edit typed (lib.rs's scenes): YAAIHH_EDIT_TEXT=FROM=>TO in the text being edited (a kind, a template, a
/// step's prompt), YAAIHH_EDIT_WHY the why.
pub fn scene_text(s: &mut App) {
    if let Some((from, to)) = std::env::var("YAAIHH_EDIT_TEXT").ok().as_deref().and_then(|x| x.split_once("=>")).map(|(a, b)| (a.replace("\\n", "\n"), b.replace("\\n", "\n"))) {
        let step = s.tickets.edits.step.clone().filter(|st| st.inline.is_none()).and_then(|st| s.proj().and_then(|p| once_of(s, p, &st.call)));
        let e = &mut s.tickets.edits;
        if let Some(k) = e.kind.as_mut() {
            k.text = TextEdit::new(k.text.text.replacen(&from, &to, 1));
        } else if let Some(c) = e.call_edit.as_mut() {
            c.text = TextEdit::new(c.text.text.replacen(&from, &to, 1));
        } else if let Some((base, _)) = step {
            e.once = TextEdit::new(base.replacen(&from, &to, 1));
        }
    }
    if let Ok(w) = std::env::var("YAAIHH_EDIT_WHY") {
        s.tickets.edits.why = TextEdit::new(w);
    }
}
