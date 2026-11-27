//! The window's state, kept across a restart or reopen (owner 8 Nov): the split and its panes, the sidebar, what each
//! pane shows (the project and page, the agent, Chat or Terminal, and on an agent's page which of its views and the
//! thread beside With you's list), the zoom, every text typed and not sent (the composers, the answers, the comment
//! being written) and the boxes open (replies, the new-project dialog).
//! Mail sent and not delivered yet is coordd's and comes back from it.
//!
//! One file, `ui.json` beside the config (`ui-dev.json` for `--dev`), written whole to a temporary file and renamed
//! over it, half a second after the last change; read once at start. A missing file, or one that does not parse,
//! means the defaults; projects and agents that are gone are left out.
//!
//! His window's states, so it can go back too (paper-research N-1446 p2206, p2207; his N-687 p8: "rollback change fork
//! the whole ui local window etc by deltas"): each write of ui.json adds a line to `ui-deltas.jsonl` beside it
//! ([`Delta`]), the fields of his window that changed since the last ([`window_of`]), each with its whole value before
//! and after, and his pins and the Map's moment beside them. The first write of a run puts the window whole first, as
//! it was put at the start. What he typed and has not sent, his mail on its way, what he has seen and this window's id
//! are no part of it ([`NOT_UNDONE`]): going back undoes none of them. History's "Your window" lists the states, and a
//! press on one puts his window back as it was then ([`go_back_to`]), which is itself a state: nothing after it is
//! lost, and a change from there is the fork, in the same file.

use crate::{App, Sel, ask};
use rune::TextEdit;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::time::{Duration, Instant};

/// How long the state stays unchanged before it is written.
const SETTLE: Duration = Duration::from_millis(500);

#[derive(Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
struct Saved {
    /// The focused project, by id.
    project: Option<String>,
    sel: Option<Sel>,
    /// The split's other panes: (project id, page).
    panes: Vec<(String, Sel)>,
    focus: usize,
    sidebar_width: Option<f32>,
    sidebar_open: Option<bool>,
    /// The sidebar closed to its icons (sidebar.rs `rail`); its width then is the one it opens to.
    sidebar_rail: Option<bool>,
    /// His look from the bar's menu (lib.rs `App::dark`); none: the system's.
    dark: Option<bool>,
    /// The whole window's zoom (zoom.rs).
    zoom: Option<f32>,
    /// (project id, agent, text).
    composers: Vec<(String, String, String)>,
    /// Where the caret and the selection stood in each of them: (project id, agent, anchor, focus), in bytes. A field
    /// of its own, so a ui.json from before it still reads.
    carets: Vec<(String, String, usize, usize)>,
    /// (project id, mail id, typed, comments): what a queued mail was made of, for Edit.
    drafts: Vec<(String, String, String, Vec<ask::Comment>)>,
    /// Comments waiting for his next message, per place, as a window before Add comment sent at once kept them (N-1396
    /// p329): read into their places' boxes (ask.rs `from_before`); this one writes none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    comments: Vec<(String, Vec<ask::Comment>)>,
    /// (project id, place, comments): the comments held for each place's next Send (ask.rs `Notes::held`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    held: Vec<(String, String, Vec<ask::Comment>)>,
    /// Answers being written, per question, as an older window kept them (this one keeps them in `composers`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    answers: Vec<(String, String)>,
    picked: Option<ask::Picked>,
    /// `picked`'s box closed with his words kept as its draft (ask.rs `close_mark`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    comment_shut: bool,
    /// The note being written for `picked`, as an older window kept it (this one keeps it in `composers`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    writing: Option<String>,
    replying: Vec<String>,
    show_all: bool,
    /// Agents shown as their terminal: (project id, agent), on their page and as tiles.
    term_pages: Vec<(String, String)>,
    /// His drafts of agents' system prompt parts, per agent (prompt_edit.rs `saved`; the owner, N-1790 part 1).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    prompt_drafts: Vec<crate::prompt_edit::SavedDraft>,
    term_tiles: Vec<(String, String)>,
    /// The runs and tool calls opened in the chats.
    expanded: Vec<String>,
    agents_tab: usize,
    open_mail: Option<String>,
    new_project: Option<NewProject>,
    /// The tabs (tabs.rs): each its windows, the one shown; and the tab shown.
    tabs: Vec<SavedTab>,
    tab: usize,
    /// The Files page of each project (project id, what is kept of it: files.rs).
    files: Vec<(String, crate::files::Kept)>,
    /// This window's id for coordd (`box_id`).
    box_id: Option<String>,
    /// The Threads and Tickets page: the thread shown and what he was writing on it (N-730: a swap brings back what
    /// he had open).
    tickets: SavedTickets,
    /// Where a box's words go when he chose it (message_box.rs): (project id, place, target). A step's answer is not
    /// kept: the box finds the step open again, or not.
    box_targets: Vec<(String, String, crate::message_box::Target)>,
    /// The Add account sheet (accounts.rs `add_kept`): opened, and on one of the core's two steps the name typed and
    /// the tier.
    add_account: Option<(bool, String, usize)>,
    /// The split proposed for a box and his clicks on its chips (split.rs): (project id, place, proposal). It stands
    /// only while the box holds the text it was made for.
    splits: Vec<(String, String, crate::split::Proposal)>,
    /// His messages in cleaning (words.rs `kept`): (project id, message). A swap or a close loses none (N-815).
    #[serde(default)]
    words: Vec<(String, serde_json::Value)>,
    /// His messages on their way, sent or not (outgoing.rs `kept`): (project id, message). A restart loses none
    /// (consult-lead N-412 post 1900, 2).
    #[serde(default)]
    outgoing: Vec<(String, serde_json::Value)>,
    /// What a box's stopped split Send already posted (split.rs `carried`): (project id, place, carry).
    split_carry: Vec<(String, String, crate::split::Carry)>,
    /// The lines at a thread's foot for his messages sent wholly elsewhere (split.rs `left_kept`): (project id,
    /// thread, each where, label, words).
    split_left: Vec<(String, String, Vec<(String, String, String)>)>,
    /// The Add account sheet's API key step was the one open (accounts.rs `add_key_kept`).
    add_account_key: bool,
    /// The account that sheet had already sent, its id and name (accounts.rs `add_made_kept`): it comes back on that
    /// account's step, not on a form that would make a second one.
    add_account_made: Option<(String, String)>,
    /// Where he was reading: (view, the key of the row at its top, "" at a chat's end) (read_place.rs).
    read_places: Vec<(String, String)>,
    /// The session file each chat's place is in: (view, file) (read_place.rs).
    read_files: Vec<(String, String)>,
    /// The places he left and the places gone back from, for Back, forward and History (history.rs).
    history_back: Vec<crate::history::Place>,
    history_ahead: Vec<crate::history::Place>,
    /// The lenses (lens.rs) among the pages above, each by where it stands (`sel`, `panes.J`, `tabs.T.W.sel`,
    /// `tabs.T.W.panes.J`, `history_back.I`, `history_ahead.I`). Those pages are written as the page each lens is drawn
    /// from ([`kept_sel`]), so that a window from before lenses reads the file whole and opens them (`release.sh back`;
    /// consult-3, N-1396 p342: ui.json is read as one, and a page it does not know would start it plain); this window
    /// puts the lenses back over them ([`lensed`]). Each lens is read on its own, so one of a kind this window does not
    /// know (a newer window's) leaves its page as written and the rest of the file read.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    lenses: Vec<(String, serde_json::Value)>,
    /// With you drawn as blocks (withyou.rs until N-873.p14): gone, kept so an older ui.json still reads.
    with_you_blocks: bool,
    /// What he has seen of each thread: (project id, ticket id, its last post seen) (seen.rs), so a swap or a restart
    /// shows as new only what came since.
    seen: Vec<(String, String, u32)>,
    /// The thread beside each agent's list in With you, as he picked it (withyou.rs `inside`): (project id, agent,
    /// ticket). Without it a start showed the list's first thread in its place (his "i keep going into some other
    /// chat", "the content is still there just the view is reset"; paper-research, N-1396 p322).
    with_you: Vec<(String, String, u64)>,
    /// Where his words to each agent go, as he chose it (withyou.rs `on`): (project id, agent, the thread's id; none:
    /// a new thread).
    with_you_on: Vec<(String, String, Option<String>)>,
    /// Which of its views each agent's chat shows (streams.rs `which`): (project id, agent, view).
    which: Vec<(String, String, crate::streams::Which)>,
    /// The projects whose structure he left unfolded (sidebar.rs `structure`), by id: every other project's is folded
    /// at a start, and every project's after a file from before it (paper-research N-1446 p1938, 2; his N-282.p3: "yo
    /// udont have ot revela everytihng at onc").
    structure_open: Vec<String>,
    /// His other folds in the sidebar's structure and in the outline (`streams.open`'s keys of [`FOLD_KINDS`]):
    /// (project id, the key without its project's place). Before them every fold he made was lost at each start and
    /// each swap (p1938, 2).
    folds: Vec<(String, String)>,
    /// The moment he put everything up to then into the Archive, the new chat it opened and whether the Archive is
    /// shown (archive.rs); none while nothing is archived. A state of his window from before it brings all back.
    #[serde(skip_serializing_if = "Option::is_none")]
    archive: Option<crate::archive::Mark>,
    /// How the Map draws time (spacetime.rs): Glass or Flat, the wake, the tilt; none while as it opens.
    #[serde(skip_serializing_if = "Option::is_none")]
    map_space: Option<crate::map::spacetime::Space>,
}

#[derive(Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
struct SavedTickets {
    shown: Option<u64>,
    /// The open form: its kind and its fields by name.
    opening: Option<String>,
    fields: Vec<(String, String)>,
    /// His answers being written, by ticket, and their other fields.
    answer: Vec<(u64, String)>,
    answer_fields: Vec<(u64, String, String)>,
    /// The change boxes, by ticket: the why, the program edited, the program in place of a withdrawn step.
    why: Vec<(u64, String)>,
    edit: Vec<(u64, String)>,
    withdraw: Vec<(u64, String)>,
    /// The post being moved and where to.
    moving: Option<(u64, u32)>,
    move_to: String,
    /// The end boxes open: (ticket, how, why).
    ending: Vec<(u64, usize, String)>,
    /// How the outline and the threads were left: folded, opened, by time, between agents.
    folded: Vec<(u64, u32)>,
    how: Vec<u64>,
    details: Vec<u64>,
    more: Vec<u64>,
    pic_hidden: Vec<u64>,
    between: bool,
    by_time: bool,
    /// The search field's titles only (find.rs `titles`).
    titles_only: bool,
}

fn sorted<T: Ord>(mut v: Vec<T>) -> Vec<T> {
    v.sort();
    v
}

fn texts<K: Clone + Ord>(m: &std::collections::HashMap<K, TextEdit>) -> Vec<(K, String)> {
    sorted(m.iter().filter(|(_, t)| !t.text.is_empty()).map(|(k, t)| (k.clone(), t.text.clone())).collect())
}

/// The carets put back where they stood, in a box whose text came back; one that no longer falls on a character's
/// edge stays at the end.
fn put_carets(s: &mut App, carets: &[(String, String, usize, usize)], ix: &dyn Fn(&str) -> Option<usize>) {
    for (p, a, anchor, focus) in carets {
        let Some(e) = ix(p).and_then(|i| s.composers.get_mut(&(i, a.clone()))) else { continue };
        let ok = |b: usize| b <= e.text.len() && e.text.is_char_boundary(b);
        if ok(*anchor) && ok(*focus) {
            e.set_selection(*anchor, *focus);
        }
    }
}

fn take_tickets(s: &App) -> SavedTickets {
    let t = &s.tickets;
    SavedTickets {
        shown: t.shown,
        opening: t.opening.clone(),
        fields: if t.opening.is_some() { texts(&t.fields) } else { Vec::new() },
        answer: texts(&t.answer),
        answer_fields: texts(&t.answer_fields).into_iter().map(|((n, f), x)| (n, f, x)).collect(),
        why: texts(&t.why),
        edit: texts(&t.edit),
        withdraw: texts(&t.withdraw),
        moving: t.moving,
        move_to: if t.moving.is_some() { t.move_to.text.clone() } else { String::new() },
        // the why is its box's, in `composers` (ending.rs `place`)
        ending: sorted(t.ending.iter().map(|(n, e)| (*n, e.how, String::new())).collect()),
        folded: sorted(t.folded.iter().copied().collect()),
        how: sorted(t.how.iter().copied().collect()),
        details: sorted(t.details.iter().copied().collect()),
        more: sorted(t.more.iter().copied().collect()),
        pic_hidden: sorted(t.pic_hidden.iter().copied().collect()),
        between: t.between,
        by_time: t.by_time,
        titles_only: t.find.titles,
    }
}

/// The texts of the Threads and Tickets page put back (at the start, and again at a swap's go).
fn put_ticket_texts(s: &mut App, v: &SavedTickets) {
    let t = &mut s.tickets;
    t.fields = v.fields.iter().map(|(n, x)| (n.clone(), edit(x))).collect();
    t.answer = v.answer.iter().map(|(n, x)| (*n, edit(x))).collect();
    t.answer_fields = v.answer_fields.iter().map(|(n, f, x)| ((*n, f.clone()), edit(x))).collect();
    t.why = v.why.iter().map(|(n, x)| (*n, edit(x))).collect();
    t.edit = v.edit.iter().map(|(n, x)| (*n, edit(x))).collect();
    t.withdraw = v.withdraw.iter().map(|(n, x)| (*n, edit(x))).collect();
    t.move_to = edit(&v.move_to);
    t.ending = v.ending.iter().map(|(n, how, _)| (*n, crate::ending::Ending { how: *how })).collect();
    // a why an older window kept here: into its box, in the project shown
    for (n, _, why) in v.ending.iter().filter(|(_, _, w)| !w.is_empty()) {
        s.composers.entry((s.cur, crate::ending::place(*n))).or_insert_with(|| edit(why));
    }
}

fn put_tickets(s: &mut App, v: &SavedTickets) {
    let t = &mut s.tickets;
    t.shown = v.shown.or(t.shown);
    t.opening = v.opening.clone();
    t.moving = v.moving;
    t.folded = v.folded.iter().copied().collect();
    t.how = v.how.iter().copied().collect();
    t.details = v.details.iter().copied().collect();
    t.more = v.more.iter().copied().collect();
    t.pic_hidden = v.pic_hidden.iter().copied().collect();
    t.between = v.between;
    t.by_time = v.by_time;
    t.find.titles = v.titles_only;
    put_ticket_texts(s, v);
}

#[derive(Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
struct SavedTab {
    windows: Vec<SavedLayout>,
    cur: usize,
}

/// A window: (project id, page), the split's panes, the focused pane.
#[derive(Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
struct SavedLayout {
    project: String,
    sel: Option<Sel>,
    panes: Vec<(String, Sel)>,
    focus: usize,
}

#[derive(Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
struct NewProject {
    name: String,
    file: String,
}

/// What was last written, and since when the state differs from it.
#[derive(Default)]
pub struct Keeper {
    /// The hash of the state last written (or read): each poll hashes the state's JSON as it is written out, without
    /// building the text, and writes only when the hash moved.
    last: u64,
    since: Option<Instant>,
    /// His window as the last delta left it ([`window_of`]): the next is taken against it. Put at the start
    /// ([`restore`]) or a swap's take-over ([`retake`]); none before.
    window: Option<Map<String, Value>>,
    /// This run has written its first line ([`note`]).
    started: bool,
    /// There was a file of his window's states at the start.
    had: bool,
    /// His window's states ([`deltas_path`]), read at History's first open in this run ([`read_deltas`]), each written
    /// since added.
    deltas: Option<Vec<Delta>>,
    /// The state he went back to, for the next line's words ([`go_back_to`]).
    going_to: Option<String>,
}

fn path() -> std::path::PathBuf {
    let name = if crate::DEV.load(std::sync::atomic::Ordering::Relaxed) { "ui-dev.json" } else { "ui.json" };
    std::path::Path::new(&crate::projects::config_path()).with_file_name(name)
}

/// The file of his window's states beside ui.json: `ui-deltas.jsonl` (`ui-dev-deltas.jsonl` for `--dev`).
pub(crate) fn deltas_path() -> std::path::PathBuf {
    let p = path();
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("ui").to_string();
    p.with_file_name(format!("{stem}-deltas.jsonl"))
}

/// The states' file is moved to `.1` past this at a run's first line, as log.rs moves its files at the start: so the
/// file starts with a run's first line, and History reads it whole.
const DELTAS_ROTATE: u64 = 4 << 20;

/// What going back to an earlier state of his window never undoes (p2206, p2207): what he typed and has not sent and
/// the boxes it is in (a field inside one: `tickets.fields`, the forms and the change boxes on a thread), his mail on
/// its way, what he has seen, this window's id; and where he was in it rather than how it is laid out, the rows
/// scrolled to and the places Back walks (history.rs), each a whole list written again at every page he opens.
const NOT_UNDONE: &[&str] = &[
    "composers",
    "carets",
    "drafts",
    "comments",
    "held",
    "answers",
    "picked",
    "comment_shut",
    "writing",
    "replying",
    "box_targets",
    "splits",
    "split_carry",
    "split_left",
    "new_project",
    "add_account",
    "add_account_key",
    "add_account_made",
    "tickets.opening",
    "tickets.fields",
    "tickets.answer",
    "tickets.answer_fields",
    "tickets.why",
    "tickets.edit",
    "tickets.withdraw",
    "tickets.moving",
    "tickets.move_to",
    "tickets.ending",
    "words",
    "outgoing",
    "seen",
    "box_id",
    "read_places",
    "read_files",
    "history_back",
    "history_ahead",
];

/// One state of his window, a line of [`deltas_path`]: when; the fields that changed since the line before, each
/// with its whole value before (`was`) and after (`now`); what changed in his words, for History. A run's first line
/// (`start`) holds the window whole as the run put it, so a state is built forward from its run's first line
/// ([`rebuilt`]), whatever changed while this window was not writing (another window's ui.json, the Map's moment,
/// which a start sets to now: p2207).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub(crate) struct Delta {
    pub at: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub start: bool,
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub was: Map<String, Value>,
    pub now: Map<String, Value>,
    pub said: String,
}

/// His window as a delta is taken over: SAVED as ui.json writes it, but for [`NOT_UNDONE`]; and beside it his pins
/// (sidebar.rs, files of their own) and the moment the Map shows (cloud.rs `Cloud::at`), which ui.json does not hold.
fn window_of(s: &App, saved: &Saved) -> Map<String, Value> {
    let Ok(Value::Object(mut w)) = serde_json::to_value(saved) else { return Map::new() };
    for path in NOT_UNDONE {
        match path.split_once('.') {
            Some((top, inner)) => {
                if let Some(Value::Object(o)) = w.get_mut(top) {
                    o.remove(inner);
                }
            }
            None => _ = w.remove(*path),
        }
    }
    w.insert("pins".into(), serde_json::to_value(&s.pins.list).unwrap_or_default());
    w.insert("map_at".into(), serde_json::to_value(s.cloud.at).unwrap_or_default());
    w
}

/// After ui.json is written from SAVED: his window's state a line of [`deltas_path`] when it changed. The run's first
/// line is the window as it was put at the start, whole.
fn note(s: &mut App, saved: &Saved) {
    let w = window_of(s, saved);
    let at = docket_machine::machine::at_secs(chrono::Utc::now().timestamp());
    let mut lines = Vec::new();
    let base = s.keeper.window.take().unwrap_or_else(|| w.clone());
    if !s.keeper.started {
        s.keeper.started = true;
        let p = deltas_path();
        if std::fs::metadata(&p).is_ok_and(|m| m.len() > DELTAS_ROTATE) {
            let _ = std::fs::rename(&p, p.with_extension("jsonl.1"));
        }
        lines.push(Delta { at: at.clone(), start: true, now: base.clone(), said: "opened".into(), ..Delta::default() });
    }
    let changed: Vec<String> = base.keys().chain(w.keys()).filter(|k| base.get(*k) != w.get(*k)).cloned().collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    if !changed.is_empty() {
        let pick = |m: &Map<String, Value>| changed.iter().map(|k| (k.clone(), m.get(k).cloned().unwrap_or(Value::Null))).collect::<Map<String, Value>>();
        let (was, now) = (pick(&base), pick(&w));
        let said = said(s, &was, &now);
        lines.push(Delta { at, start: false, was, now, said });
    }
    s.keeper.window = Some(w);
    if lines.is_empty() {
        return;
    }
    let text: String = lines.iter().filter_map(|d| serde_json::to_string(d).ok()).map(|l| l + "\n").collect();
    let wrote = std::fs::OpenOptions::new().create(true).append(true).open(deltas_path()).and_then(|mut f| std::io::Write::write_all(&mut f, text.as_bytes()));
    if let Err(e) = wrote {
        crate::log::line(&format!("ui-deltas.jsonl: not written ({e})"));
    }
    if let Some(d) = s.keeper.deltas.as_mut() {
        d.extend(lines);
    }
}

/// What changed from WAS to NOW in his words, for History's "Your window": the page shown (History's own label), then
/// what else, as `thread 8 · 2 panes · pinned pr · the Map at Thu 12:00`; first, the state gone back to.
fn said(s: &mut App, was: &Map<String, Value>, now: &Map<String, Value>) -> String {
    let mut words: Vec<String> = Vec::new();
    if let Some(at) = s.keeper.going_to.take() {
        words.push(format!("back to {}", crate::when::secs(docket_machine::machine::secs(&at).unwrap_or(0))));
    }
    words.extend(crate::history::shown_label(s));
    let pins = |m: &Map<String, Value>| -> Vec<String> {
        let list: Vec<crate::sidebar::Pin> = m.get("pins").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
        list.into_iter().map(|p| p.agent).collect()
    };
    for k in now.keys() {
        let more: Vec<String> = match k.as_str() {
            "panes" => vec![match s.panes.len() + 1 {
                1 => "one pane".into(),
                n => format!("{n} panes"),
            }],
            "tabs" | "tab" if !s.tabs.tabs.is_empty() => vec![format!("tab {} of {}", s.tabs.cur + 1, s.tabs.tabs.len())],
            "pins" => {
                let (before, after) = (pins(was), pins(now));
                let on = after.iter().filter(|a| !before.contains(a)).map(|a| format!("pinned {a}"));
                on.chain(before.iter().filter(|b| !after.contains(b)).map(|b| format!("unpinned {b}"))).collect()
            }
            "map_at" => vec![match s.cloud.at {
                Some(t) => format!("the Map at {}", crate::when::secs(t + 1)),
                None => "the Map now".into(),
            }],
            "sidebar_width" | "sidebar_open" | "sidebar_rail" => vec!["the sidebar".into()],
            "zoom" => vec![format!("zoom {:.0}%", s.zoom.level() * 100.0)],
            "dark" => vec![match s.dark {
                Some(true) => "dark",
                Some(false) => "light",
                None => "the system's look",
            }
            .into()],
            "structure_open" | "folds" => vec!["folds".into()],
            "archive" => vec![crate::archive::said(s.archive.as_ref())],
            "map_space" => vec!["the Map's time view".into()],
            _ => Vec::new(),
        };
        for w in more {
            if !words.contains(&w) {
                words.push(w);
            }
        }
    }
    words.join(" · ")
}

/// His window's states read from [`deltas_path`] once in a run, at History's open: a line this window does not read
/// is left out.
pub(crate) fn read_deltas(s: &mut App) {
    if s.keeper.deltas.is_none() {
        let text = std::fs::read_to_string(deltas_path()).unwrap_or_default();
        s.keeper.deltas = Some(text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect());
    }
}

/// Whether his window has states to go back to, for History's mark: a file of them at the start, or one written since.
pub(crate) fn has_states(s: &App) -> bool {
    s.keeper.had || s.keeper.started
}

/// History's "Your window": the states read ([`read_deltas`]) newest first, the one he is in (the last) left out, up
/// to SHOWN, each its index, its words and its time as the window writes times. Only those a run's first line comes
/// before, as each is built from it.
pub(crate) fn window_states(s: &App, shown: usize) -> Vec<(usize, String, String)> {
    let Some(d) = s.keeper.deltas.as_ref() else { return Vec::new() };
    let Some(first) = d.iter().position(|x| x.start) else { return Vec::new() };
    (first..d.len().saturating_sub(1))
        .rev()
        .take(shown)
        .map(|k| (k, d[k].said.clone(), crate::when::secs(docket_machine::machine::secs(&d[k].at).unwrap_or(0))))
        .collect()
}

/// His window at its state K of D: its run's first line, then each line after it to K.
fn rebuilt(d: &[Delta], k: usize) -> Option<Map<String, Value>> {
    let from = d.get(..=k)?.iter().rposition(|x| x.start)?;
    let mut w = d[from].now.clone();
    for x in &d[from + 1..=k] {
        w.extend(x.now.clone());
    }
    Some(w)
}

/// He goes back to his window's state K (History's "Your window"): its fields put over the window as it is now, as
/// [`retake`] puts ui.json, the window's own parts only ([`put_window`]): what he typed, his mail, what he has seen
/// stay as they are, and in a field of both (`tickets`) its typed parts too. His pins and the Map's moment come back
/// with it. Then written at once: going back is a state of its own.
pub(crate) fn go_back_to(s: &mut App, k: usize) {
    let Some((then, at)) = s.keeper.deltas.as_ref().and_then(|d| Some((rebuilt(d, k)?, d[k].at.clone()))) else { return };
    let Ok(Value::Object(mut v)) = serde_json::to_value(take(s)) else { return };
    let (mut pins, mut map_at) = (None, None);
    for (key, mut val) in then {
        match key.as_str() {
            "pins" => pins = serde_json::from_value::<Vec<crate::sidebar::Pin>>(val).ok(),
            "map_at" => map_at = Some(serde_json::from_value::<Option<i64>>(val).unwrap_or(None)),
            _ => {
                // a field of both: its typed parts as they are now
                if let (Value::Object(o), Some(Value::Object(cur))) = (&mut val, v.get(&key)) {
                    for inner in NOT_UNDONE.iter().filter_map(|p| p.strip_prefix(key.as_str())?.strip_prefix('.')) {
                        if let Some(c) = cur.get(inner) {
                            o.insert(inner.to_string(), c.clone());
                        }
                    }
                }
                v.insert(key, val);
            }
        }
    }
    let Ok(mut saved) = serde_json::from_value::<Saved>(Value::Object(v)) else { return };
    lensed(&mut saved);
    s.tabs.tabs.clear();
    put_window(s, &mut saved);
    if let Some(p) = pins {
        s.pins.put_back(p);
    }
    if let Some(t) = map_at
        && t != s.cloud.at
    {
        s.cloud.at = t;
        s.map.stay();
    }
    s.keeper.going_to = Some(at);
    write_now(s);
}

/// This window's id for coordd (`coordc bridge --box ID`; paper-research N-487.p51): kept in ui.json across restarts
/// and reused, a new one only when there is none. coordd counts the window open while a watch with it is up, and a
/// words ticket's look (its form's `window`) goes on at once when none is.
pub fn box_id() -> String {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(|| {
        let saved: Option<Saved> = std::fs::read_to_string(path()).ok().and_then(|t| readable(&t).ok()).map(|(v, _)| v);
        saved.and_then(|s| s.box_id).filter(|b| !b.trim().is_empty()).unwrap_or_else(fresh_box_id)
    })
    .clone()
}

/// A new window id: `w-` and twelve hex digits from the time and the process.
fn fresh_box_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default());
    h.write_u32(std::process::id());
    format!("w-{:012x}", h.finish() & 0xffff_ffff_ffff)
}

fn edit(t: &str) -> TextEdit {
    TextEdit::new(t.to_string())
}

/// Each page V holds, with where it stands (the places of `Saved::lenses`).
fn each_sel(v: &mut Saved, mut f: impl FnMut(String, &mut Sel)) {
    if let Some(sel) = v.sel.as_mut() {
        f("sel".into(), sel);
    }
    for (j, (_, sel)) in v.panes.iter_mut().enumerate() {
        f(format!("panes.{j}"), sel);
    }
    for (t, tab) in v.tabs.iter_mut().enumerate() {
        for (w, l) in tab.windows.iter_mut().enumerate() {
            if let Some(sel) = l.sel.as_mut() {
                f(format!("tabs.{t}.{w}.sel"), sel);
            }
            for (j, (_, sel)) in l.panes.iter_mut().enumerate() {
                f(format!("tabs.{t}.{w}.panes.{j}"), sel);
            }
        }
    }
    for (i, place) in v.history_back.iter_mut().enumerate() {
        f(format!("history_back.{i}"), &mut place.sel);
    }
    for (i, place) in v.history_ahead.iter_mut().enumerate() {
        f(format!("history_ahead.{i}"), &mut place.sel);
    }
}

/// The page a window from before lenses opens for lens L: the Threads page for Threads and a search over it, the Kinds
/// page for the Docket, else its agent's page.
fn kept_sel(l: &crate::lens::Lens) -> Sel {
    if l.filter == crate::lens::Filter::Docket {
        return Sel::Kinds;
    }
    l.agent().map_or(Sel::Tickets, |a| Sel::Agent(a.to_string()))
}

/// The lenses among V's pages into `lenses`, each page written as [`kept_sel`] gives it.
fn unlensed(v: &mut Saved) {
    let mut lenses = Vec::new();
    each_sel(v, |at, sel| {
        if let Sel::Lens(l) = sel {
            let page = kept_sel(l);
            if let Sel::Lens(l) = std::mem::replace(sel, page)
                && let Ok(l) = serde_json::to_value(l)
            {
                lenses.push((at, l));
            }
        }
    });
    v.lenses = lenses;
}

/// `lenses` put back over the pages they stand at, before anything reads them (a file from before lenses has none; a
/// lens whose place is not in the file is left out). A lens this window cannot read leaves its page as written, a line
/// in yaaihh.log.
fn lensed(v: &mut Saved) {
    let mut lenses = std::collections::HashMap::new();
    for (at, l) in std::mem::take(&mut v.lenses) {
        match serde_json::from_value::<crate::lens::Lens>(l) {
            Ok(l) => {
                lenses.insert(at, l);
            }
            Err(e) => crate::log::line(&format!("ui.json: the lens at {at} is not one this window knows ({e}); its page opens as written")),
        }
    }
    each_sel(v, |at, sel| {
        if let Some(l) = lenses.remove(&at) {
            *sel = Sel::Lens(l);
        }
    });
}

/// The kinds of `streams.open`'s keys that are folds, each `KIND|the project's place|id`: the sidebar's structure's
/// (sidebar.rs `tree_key`) and the outline's (outline.rs `fold_key`). Its other keys (a post opened whole, a session, a
/// box) are not kept.
const FOLD_KINDS: [&str; 2] = ["tree", "outline"];

/// Whether KEY of `streams.open` is a fold ([`FOLD_KINDS`]).
fn is_fold(key: &str) -> bool {
    key.split_once('|').is_some_and(|(kind, _)| FOLD_KINDS.contains(&kind))
}

/// S's folds as kept ([`Saved::structure_open`], [`Saved::folds`]): the projects whose structure is unfolded, and the
/// other folds by project id.
fn take_folds(s: &App) -> (Vec<String>, Vec<(String, String)>) {
    let open = s.projects.iter().enumerate().filter(|(i, _)| !s.streams.open.contains(&crate::sidebar::section_key(*i))).map(|(_, p)| p.id.clone()).collect();
    let mut folds: Vec<(String, String)> = s
        .streams
        .open
        .iter()
        .filter(|k| is_fold(k))
        .filter_map(|k| {
            let mut part = k.splitn(3, '|');
            let (kind, at, id) = (part.next()?, part.next()?, part.next()?);
            let p = s.projects.get(at.parse::<usize>().ok()?)?;
            (!(kind == "tree" && id == "section")).then(|| (p.id.clone(), format!("{kind}|{id}")))
        })
        .collect();
    folds.sort();
    (open, folds)
}

/// Folds kept ([`take_folds`]) put into S in place of its own: the structure folded in every project not in OPEN, each
/// other fold in the project of its id, one of a project gone left out. With none: a first start's, every project's
/// structure folded.
fn put_folds(s: &mut App, open: &[String], folds: &[(String, String)]) {
    s.streams.open.retain(|k| !is_fold(k));
    let ids: Vec<String> = s.projects.iter().map(|p| p.id.clone()).collect();
    for (i, id) in ids.iter().enumerate() {
        if !open.contains(id) {
            s.streams.open.insert(crate::sidebar::section_key(i));
        }
    }
    for (id, key) in folds {
        let (Some(i), Some((kind, rest))) = (ids.iter().position(|p| p == id), key.split_once('|')) else { continue };
        if FOLD_KINDS.contains(&kind) && !(kind == "tree" && rest == "section") {
            s.streams.open.insert(format!("{kind}|{i}|{rest}"));
        }
    }
}

fn take(s: &App) -> Saved {
    let id = |i: usize| s.projects.get(i).map(|p| p.id.clone());
    let pairs = |set: &std::collections::HashSet<(usize, String)>| {
        let mut v: Vec<(String, String)> = set.iter().filter_map(|(i, a)| Some((id(*i)?, a.clone()))).collect();
        v.sort();
        v
    };
    let mut composers: Vec<_> = s.composers.iter().filter(|(_, t)| !t.text.is_empty()).filter_map(|((i, a), t)| Some((id(*i)?, a.clone(), t.text.clone()))).collect();
    composers.sort();
    let mut carets: Vec<_> = s.composers.iter().filter(|(_, t)| !t.text.is_empty()).filter_map(|((i, a), t)| Some((id(*i)?, a.clone(), t.anchor, t.focus))).collect();
    carets.sort();
    let mut drafts: Vec<_> = s.drafts.iter().filter_map(|((i, m), (t, c))| Some((id(*i)?, m.clone(), t.clone(), c.clone()))).collect();
    drafts.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    let mut held: Vec<_> = s.notes.held.iter().filter(|(_, l)| !l.is_empty()).filter_map(|((i, place), l)| Some((id(*i)?, place.clone(), l.clone()))).collect();
    held.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    // a reply being written is its box's, in `composers` (message_box::Target::Reply)
    let answers: Vec<(String, String)> = Vec::new();
    let mut replying: Vec<String> = s.notes.replying.iter().cloned().collect();
    replying.sort();
    let mut expanded: Vec<String> = s.expanded.iter().cloned().collect();
    expanded.sort();
    let np = &s.new_project;
    let (tabs, tab) = if s.tabs.tabs.is_empty() { (Vec::new(), 0) } else { crate::tabs::current(s) };
    let tabs = tabs
        .iter()
        .map(|t| SavedTab {
            windows: t.windows.iter().map(|l| SavedLayout { project: id(l.cur).unwrap_or_default(), sel: Some(l.sel.clone()), panes: l.panes.iter().filter_map(|(i, sel)| Some((id(*i)?, sel.clone()))).collect(), focus: l.focus }).collect(),
            cur: t.cur,
        })
        .collect();
    let mut files: Vec<(String, crate::files::Kept)> = s.files.kept.iter().filter(|(_, k)| **k != crate::files::Kept::default()).map(|(p, k)| (p.clone(), k.clone())).collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut box_targets: Vec<_> = s
        .box_targets
        .iter()
        .filter(|(_, t)| !matches!(t, crate::message_box::Target::Thread { way: crate::message_box::Way::Step { .. }, .. }))
        .filter_map(|((i, place), t)| Some((id(*i)?, place.clone(), t.clone())))
        .collect();
    box_targets.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    let mut seen: Vec<(String, String, u32)> = s.tickets.seen.upto.iter().filter_map(|((i, t), n)| Some((id(*i)?, t.clone(), *n))).collect();
    seen.sort();
    // an agent's page: the thread beside With you's list, where his words go, which view
    let mut with_you: Vec<(String, String, u64)> = s.withyou.inside.iter().filter_map(|((i, a), n)| Some((id(*i)?, a.clone(), *n))).collect();
    with_you.sort();
    let mut with_you_on: Vec<(String, String, Option<String>)> = s.withyou.on.iter().filter_map(|((i, a), t)| Some((id(*i)?, a.clone(), t.clone()))).collect();
    with_you_on.sort();
    let mut which: Vec<(String, String, crate::streams::Which)> = s.streams.which.iter().filter_map(|((i, a), w)| Some((id(*i)?, a.clone(), *w))).collect();
    which.sort_by(|x, y| (&x.0, &x.1).cmp(&(&y.0, &y.1)));
    let (structure_open, folds) = take_folds(s);
    let mut saved = Saved {
        archive: s.archive.clone(),
        map_space: Some(s.cloud.space).filter(|m| *m != crate::map::spacetime::Space::default()),
        with_you_blocks: false,
        seen,
        with_you,
        with_you_on,
        which,
        structure_open,
        folds,
        tickets: take_tickets(s),
        add_account: crate::accounts::add_kept(s),
        add_account_key: crate::accounts::add_key_kept(s),
        add_account_made: crate::accounts::add_made_kept(s),
        read_places: {
            let mut v: Vec<(String, String)> = s.read_places.kept.iter().map(|(a, b)| (a.clone(), b.clone())).collect();
            v.sort();
            v
        },
        read_files: {
            let mut v: Vec<(String, String)> = s.read_places.files.iter().map(|(a, b)| (a.clone(), b.clone())).collect();
            v.sort();
            v
        },
        history_back: s.history.back.clone(),
        history_ahead: s.history.ahead.clone(),
        box_targets,
        splits: crate::split::kept(s).into_iter().filter_map(|((i, place), p)| Some((id(i)?, place, p))).collect(),
        split_carry: crate::split::carried(s).into_iter().filter_map(|((i, place), c)| Some((id(i)?, place, c))).collect(),
        words: crate::words::kept(s).into_iter().filter_map(|(p, w)| Some((p, serde_json::to_value(w).ok()?))).collect(),
        outgoing: crate::outgoing::kept(s).into_iter().filter_map(|(p, o)| Some((p, serde_json::to_value(o).ok()?))).collect(),
        split_left: crate::split::left_kept(s).into_iter().filter_map(|((i, t), l)| Some((id(i)?, t, l))).collect(),
        box_id: Some(box_id()),
        files,
        tabs,
        tab,
        project: id(s.cur),
        sel: Some(s.sel.clone()),
        panes: s.panes.iter().filter_map(|(i, sel)| Some((id(*i)?, sel.clone()))).collect(),
        focus: s.focus,
        sidebar_width: Some(if s.rail { s.rail_from } else { s.sidebar.width() }),
        sidebar_open: Some(s.sidebar.is_open()),
        sidebar_rail: Some(s.rail),
        zoom: Some(s.zoom.level()),
        dark: s.dark,
        composers,
        carets,
        drafts,
        comments: Vec::new(),
        held,
        answers,
        picked: s.notes.picked.clone(),
        comment_shut: s.notes.shut && s.notes.picked.is_some(),
        // the comment's words are the box's, in `composers` (message_box::COMMENT_PLACE)
        writing: None,
        replying,
        show_all: s.notes.show_all,
        term_pages: pairs(&s.term.pages),
        prompt_drafts: crate::prompt_edit::saved(s),
        term_tiles: pairs(&s.term.tiles),
        expanded,
        agents_tab: s.agents_tab,
        open_mail: s.open_mail.clone(),
        new_project: np.open.then(|| NewProject { name: np.name.text.clone(), file: np.file.text.clone() }),
        lenses: Vec::new(),
    };
    unlensed(&mut saved);
    saved
}

/// The state saved last, put back into S (just made, its projects not connected yet). A file that does not parse is
/// kept beside it as `ui.json.unread` (the save that follows writes ui.json over it), the window starts plain, and
/// that is a line in yaaihh.log, with nothing shown to him (consult-3, N-1396 p323). Plain, as with no file, is every
/// project's structure folded ([`put_folds`]).
pub fn restore(s: &mut App) {
    put_saved(s);
    // his window as it starts, the first line of its states in this run (`note`)
    s.keeper.window = Some(window_of(s, &take(s)));
    s.keeper.had = deltas_path().exists();
}

/// ui.json's TEXT as this window can read it: whole, or, when a later window wrote a part it does not know (a page it
/// added, a new kind of anything), every top-level part it can read, with the names of those left out. So the way back
/// to the previous exe keeps his words in their boxes, his mail and his places, all but the parts it cannot read; one
/// unknown page used to leave the whole file unread and the window plain (proto-window N-1446 p2659, 2). Err: not a
/// JSON object at all.
fn readable(text: &str) -> Result<(Saved, Vec<String>), String> {
    let e = match serde_json::from_str::<Saved>(text) {
        Ok(v) => return Ok((v, Vec::new())),
        Err(e) => e,
    };
    let Ok(Value::Object(mut m)) = serde_json::from_str::<Value>(text) else { return Err(e.to_string()) };
    let alone = |k: &String, v: &Value| serde_json::from_value::<Saved>(Value::Object(Map::from_iter([(k.clone(), v.clone())]))).is_ok();
    let left: Vec<String> = m.iter().filter(|(k, v)| !alone(k, v)).map(|(k, _)| k.clone()).collect();
    for k in &left {
        m.remove(k);
    }
    serde_json::from_value::<Saved>(Value::Object(m)).map(|v| (v, left)).map_err(|_| e.to_string())
}

fn put_saved(s: &mut App) {
    put_folds(s, &[], &[]);
    let Ok(text) = std::fs::read_to_string(path()) else { return };
    let v = match readable(&text) {
        Ok((v, left)) => {
            if !left.is_empty() {
                let kept = path().with_extension("json.unread");
                let _ = std::fs::write(&kept, &text);
                crate::log::line(&format!("ui.json: {} not read (a later window's); kept as {}, the rest read", left.join(", "), kept.display()));
            }
            v
        }
        Err(e) => {
            let kept = path().with_extension("json.unread");
            let _ = std::fs::write(&kept, &text);
            crate::log::line(&format!("ui.json: not read ({e}); kept as {}, the window starts plain", kept.display()));
            return;
        }
    };
    put(s, v);
    // an agent's page as he left it, checked against its project's first snapshot (withyou.rs `check_restored`): until
    // then a thread not in the snapshot yet is shown once it comes (withyou.rs `view`)
    s.withyou.restored = s.withyou.inside.keys().chain(s.withyou.on.keys()).chain(s.streams.which.keys()).map(|(i, _)| *i).collect();
    let saved = take(s);
    s.keeper.last = hash(s, &saved);
}

/// V put into S: by [`restore`] at the start, and by [`retake`] again at a swap's "go". A part that V has nothing of
/// is left as S has it, but for the rail and the new-project dialog, which V says are off. The window's own parts
/// first ([`put_window`]), then what he typed, his mail, what he has seen and where he was.
fn put(s: &mut App, mut v: Saved) {
    // a page shown as a lens (S1) is read back from the page it is drawn from (`unlensed` when written)
    lensed(&mut v);
    put_window(s, &mut v);
    let ids: Vec<String> = s.projects.iter().map(|p| p.id.clone()).collect();
    let ix = |id: &str| ids.iter().position(|p| p == id);
    for (p, a, t) in v.composers {
        if let Some(i) = ix(&p) {
            s.composers.insert((i, a), edit(&t));
        }
    }
    put_carets(s, &v.carets, &ix);
    for (p, m, t, c) in v.drafts {
        if let Some(i) = ix(&p) {
            s.drafts.insert((i, m), (t, c));
        }
    }
    crate::ask::from_before(s, v.comments);
    for (p, place, l) in v.held {
        if let Some(i) = ix(&p).filter(|_| !l.is_empty()) {
            s.notes.held.insert((i, place), l);
        }
    }
    crate::split::put_back(s, v.splits.iter().filter_map(|(p, place, pr)| Some(((ix(p)?, place.clone()), pr.clone()))).collect());
    crate::split::put_after(
        s,
        v.split_carry.iter().filter_map(|(p, place, c)| Some(((ix(p)?, place.clone()), c.clone()))).collect(),
        v.split_left.iter().filter_map(|(p, t, l)| Some(((ix(p)?, t.clone()), l.clone()))).collect(),
    );
    crate::words::put_back(s, v.words.iter().filter_map(|(p, w)| Some((p.clone(), serde_json::from_value(w.clone()).ok()?))).collect());
    crate::outgoing::put_back(s, v.outgoing.iter().filter_map(|(p, o)| Some((p.clone(), serde_json::from_value(o.clone()).ok()?))).collect());
    put_answers(s, v.answers);
    put_picked(s, v.picked, v.writing);
    s.notes.shut = v.comment_shut && s.notes.picked.is_some();
    s.notes.replying = v.replying.into_iter().collect();
    s.tickets.seen.upto = v.seen.into_iter().filter_map(|(p, t, n)| Some(((ix(&p)?, t), n))).collect();
    s.read_places.kept = v.read_places.into_iter().collect();
    s.read_places.files = v.read_files.into_iter().collect();
    (s.history.back, s.history.ahead) = (v.history_back, v.history_ahead);
    s.new_project.open = v.new_project.is_some();
    if let Some(np) = v.new_project {
        (s.new_project.name, s.new_project.file) = (edit(&np.name), edit(&np.file));
    }
    if let Some((open, label, tier)) = &v.add_account {
        crate::accounts::add_put_back(s, *open, label, *tier);
    }
    crate::accounts::add_key_put_back(s, v.add_account_key);
    crate::accounts::add_made_put_back(s, v.add_account_made);
    for (p, place, t) in v.box_targets {
        if let Some(i) = ix(&p) {
            s.box_targets.insert((i, place), t.in_project(i));
        }
    }
}

/// The window's own parts of V put into S, its lenses read back already ([`lensed`]): the page and the panes, the
/// sidebar, the zoom and the look, what each agent's page shows, the folds, the terminals, the runs opened, the Files
/// pages, the mail open, the Threads page (its typed parts as V has them) and the tabs; what a state of his window
/// puts back ([`go_back_to`]), and [`put`]'s first part. The tabs are added to: empty them first.
fn put_window(s: &mut App, v: &mut Saved) {
    let ids: Vec<String> = s.projects.iter().map(|p| p.id.clone()).collect();
    let ix = |id: &str| ids.iter().position(|p| p == id);
    // an agent page read once the project connects (its snapshot reads every chat it holds)
    let mut chats: Vec<(usize, String)> = Vec::new();
    if let Some(i) = v.project.as_deref().and_then(ix) {
        s.cur = i;
        // a page of an extension this yaaihh lacks is not restored
        if let Some(sel) = v.sel.take().filter(|sel| !matches!(sel, Sel::Ext(e, _) if crate::ext::get(e).is_none())) {
            if let Sel::Agent(a) = &sel {
                chats.push((i, a.clone()));
            }
            s.sel = sel;
        }
    }
    s.panes = std::mem::take(&mut v.panes).into_iter().filter_map(|(p, sel)| Some((ix(&p)?, sel))).collect();
    for (i, sel) in &s.panes {
        if let Sel::Agent(a) = sel {
            chats.push((*i, a.clone()));
        }
    }
    s.focus = if s.panes.is_empty() { 0 } else { v.focus.min(s.panes.len() - 1) };
    if let Some(w) = v.sidebar_width {
        s.sidebar.set_width(w);
    }
    if let Some(o) = v.sidebar_open {
        s.sidebar.set_open(o);
    }
    match v.sidebar_rail {
        Some(true) => {
            s.rail_from = s.sidebar.width();
            s.rail = true;
            s.sidebar.set_width(crate::sidebar::RAIL_W);
        }
        Some(false) => s.rail = false,
        None => {}
    }
    if let Some(z) = v.zoom {
        s.zoom.restore(z);
    }
    s.dark = v.dark;
    for (i, a) in chats {
        s.projects[i].convs.entry(a).or_default();
    }
    s.withyou.inside = std::mem::take(&mut v.with_you).into_iter().filter_map(|(p, a, n)| Some(((ix(&p)?, a), n))).collect();
    s.withyou.on = std::mem::take(&mut v.with_you_on).into_iter().filter_map(|(p, a, t)| Some(((ix(&p)?, a), t))).collect();
    s.streams.which = std::mem::take(&mut v.which).into_iter().filter_map(|(p, a, w)| Some(((ix(&p)?, a), w))).collect();
    put_folds(s, &v.structure_open, &v.folds);
    s.archive = v.archive.take();
    s.cloud.space = v.map_space.unwrap_or_default().clamped();
    s.notes.show_all = v.show_all;
    s.term.pages = std::mem::take(&mut v.term_pages).into_iter().filter_map(|(p, a)| Some((ix(&p)?, a))).collect();
    crate::prompt_edit::restore(s, std::mem::take(&mut v.prompt_drafts), ix);
    s.term.tiles = std::mem::take(&mut v.term_tiles).into_iter().filter_map(|(p, a)| Some((ix(&p)?, a))).collect();
    s.expanded = std::mem::take(&mut v.expanded).into_iter().collect();
    s.agents_tab = v.agents_tab;
    s.files.kept = std::mem::take(&mut v.files).into_iter().collect();
    s.open_mail = v.open_mail.take();
    put_tickets(s, &v.tickets);
    // a search shown as the page: its words back in the field (lens.rs `find_words`)
    if let Sel::Lens(l) = s.sel.clone() {
        s.find_words(&l);
    }
    // the tabs: a window of a project that is gone is left out, and a tab left with none
    let mut shown = (0, 0);
    for (ti, t) in std::mem::take(&mut v.tabs).into_iter().enumerate() {
        let mut windows = Vec::new();
        let mut cur = 0;
        for (wi, l) in t.windows.into_iter().enumerate() {
            let (Some(p), Some(sel)) = (ix(&l.project), l.sel) else { continue };
            if wi == t.cur {
                cur = windows.len();
            }
            let panes: Vec<(usize, Sel)> = l.panes.into_iter().filter_map(|(p, sel)| Some((ix(&p)?, sel))).collect();
            let focus = if panes.is_empty() { 0 } else { l.focus.min(panes.len() - 1) };
            windows.push(crate::tabs::Layout { cur: p, sel, panes, focus });
        }
        if windows.is_empty() {
            continue;
        }
        if ti == v.tab {
            shown = (s.tabs.tabs.len(), cur);
        }
        s.tabs.tabs.push(crate::tabs::Tab { windows, cur });
    }
    s.tabs.cur = shown.0;
    if let Some(t) = s.tabs.tabs.get_mut(shown.0) {
        t.cur = shown.1;
    }
}

/// The replies an older window kept apart (by thread id), into their boxes in the project shown: a reply is written in
/// the project its chat is in.
fn put_answers(s: &mut App, answers: Vec<(String, String)>) {
    for (id, t) in answers {
        s.composers.entry((s.cur, id)).or_insert_with(|| edit(&t));
    }
}

/// The comment box as it stood (ask.rs `Picked`): its words are in its box's place (an older window kept them in
/// WRITING), its project is the one that place came back in, and its pane the one it was in (an older window kept
/// none: the one it opens in now, `ask::pane_of`).
fn put_picked(s: &mut App, picked: Option<crate::ask::Picked>, writing: Option<String>) {
    let place = crate::message_box::COMMENT_PLACE;
    let at = s.composers.keys().find(|(_, a)| a == place).map(|(i, _)| *i);
    s.notes.picked = picked.map(|mut p| {
        p.pi = at.unwrap_or(p.pi.min(s.projects.len().saturating_sub(1)));
        p.pane = p.pane.or_else(|| crate::ask::pane_of(s, p.anchor.as_deref()));
        p
    });
    match (&s.notes.picked, writing) {
        (Some(p), Some(t)) if at.is_none() => {
            s.composers.insert((p.pi, place.into()), edit(&t));
        }
        (None, _) => s.composers.retain(|(_, a), _| a != place),
        _ => {}
    }
}

/// The whole state as the window this one takes over from wrote it at "go" (restart.rs), put back as [`restore`]
/// puts it: what he typed there while this one started, and what he pressed: the page, the panes, the tabs, the thread
/// beside With you's list. Taking back only the boxes' texts lost a press made in those seconds (consult-3, docket-edge6,
/// N-1396 p342, p343). What `put` adds to rather than sets is emptied first. These picks are his of a moment ago, in a
/// window that was connected, so they are not marked to check again at a project's first snapshot.
pub fn retake(s: &mut App) {
    let Some((v, _)) = std::fs::read_to_string(path()).ok().and_then(|t| readable(&t).ok()) else { return };
    s.composers.clear();
    s.drafts.clear();
    s.notes.held.clear();
    s.box_targets.clear();
    s.tabs.tabs.clear();
    put(s, v);
    let saved = take(s);
    s.keeper.last = hash(s, &saved);
    // his window as the other one left it: its states go on from there (`note`)
    s.keeper.window = Some(window_of(s, &saved));
}

/// SAVED's hash as it is written out, with what a state of his window holds beside it (`window_of`): a pin or the
/// Map's moment changed is a write too, so it is a state.
fn hash(s: &App, saved: &Saved) -> u64 {
    let mut v = crate::version::Version::plain();
    let _ = serde_json::to_writer(&mut v, saved);
    let _ = serde_json::to_writer(&mut v, &(&s.pins.list, s.cloud.at));
    v.finish()
}

/// Each poll: a state that changed and then stayed so for [`SETTLE`] is written; until then the window wakes for it.
pub fn keep(s: &mut App, tx: &mut rune::Tx) {
    if rune::is_headless() {
        return;
    }
    let saved = take(s);
    let now = hash(s, &saved);
    if now == s.keeper.last {
        s.keeper.since = None;
        return;
    }
    let since = *s.keeper.since.get_or_insert_with(Instant::now);
    let waited = since.elapsed();
    if waited < SETTLE {
        tx.wake_after(SETTLE - waited);
        return;
    }
    write(s, &saved, now);
}

/// Write the state now, whatever has settled: before a restart onto a new release (restart.rs). True when written.
pub fn write_now(s: &mut App) -> bool {
    let saved = take(s);
    let now = hash(s, &saved);
    write(s, &saved, now)
}

fn write(s: &mut App, saved: &Saved, now: u64) -> bool {
    let ok = write_whole(&path(), &serde_json::to_string(saved).unwrap_or_default()).is_ok();
    if ok {
        s.keeper.last = now;
        s.keeper.since = None;
        note(s, saved);
    }
    ok
}

/// Write TEXT to the file at P whole or not at all: to `P.tmp` beside it, then renamed over it, so a write cut off
/// half-way leaves the file as it was (ui.json; the pins' files, sidebar.rs).
pub(crate) fn write_whole(p: &std::path::Path, text: &str) -> std::io::Result<()> {
    let mut tmp = p.as_os_str().to_owned();
    tmp.push(".tmp");
    std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::ProjectConfig;

    fn app(dir: &std::path::Path) -> App {
        unsafe { std::env::set_var("YAAIHH_CONFIG", dir.join("projects.toml")) };
        let mut s = crate::init();
        s.projects = ["a", "b"].iter().map(|id| crate::Proj::new(&ProjectConfig { name: id.to_string(), id: id.to_string(), boxed: true }, Vec::new())).collect();
        s
    }

    /// The search field's titles only (find.rs) is kept across a restart; a ui.json from before it reads as off.
    #[test]
    fn titles_only_is_kept_and_an_older_file_reads_as_off() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.tickets.find.titles = true;
        let kept = serde_json::to_string(&take_tickets(&s)).unwrap();
        s.tickets.find.titles = false;
        put_tickets(&mut s, &serde_json::from_str(&kept).unwrap());
        assert!(s.tickets.find.titles, "kept");
        let before: SavedTickets = serde_json::from_str(r#"{"by_time": true}"#).unwrap();
        assert!(!before.titles_only, "a ui.json from before it: off");
    }

    #[test]
    fn the_window_comes_back_as_it_was_left_and_a_bad_file_means_defaults() {
        // YAAIHH_CONFIG is the process's: not while another test reads or points it (view_cost.rs `one`)
        let _one = crate::view_cost::one();
        let dir = std::env::temp_dir().join(format!("yaaihh-state-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = app(&dir);
        s.cur = 1;
        s.sel = Sel::Agent("LAZY".into());
        s.panes = vec![(0, Sel::Tickets)];
        s.composers.insert((1, "LAZY".into()), edit("half a thought"));
        s.composers.get_mut(&(1, "LAZY".into())).unwrap().set_caret(4);
        s.composers.insert((1, "q3".into()), edit("yes, but"));
        // his answer written under a question and not sent (threads.rs `answer_under`; consult-lead N-1335 p4, 1)
        s.composers.insert((1, crate::message_box::answer_place("N-12", 3, 1)), edit("orbit first"));
        s.notes.replying.insert("m7".into());
        s.term.pages.insert((0, "orch".into()));
        let files = crate::files::Kept { open: ["/x".to_string()].into(), shown: Some("/x/a.md".into()), source: true, tree_at: 120.0, file_at: 40.0 };
        s.files.kept.insert("b".into(), files.clone());
        s.read_places.kept.insert("thread b N-12".into(), "N-12-p7".into());
        s.read_places.files.insert("chat b orch".into(), "s1.jsonl".into());
        let place = crate::history::Place { project: "b".into(), sel: Sel::Tickets, thread: Some(12), panel: None, top: Some("N-12-p7".into()), focus: None, cam: None, picked: None, tab: None, board: Vec::new(), label: "thread".into(), id: "N-12".into() };
        // a place on the Map's cloud keeps its camera and the node selected (paper-research N-1446 p2116, 1-3)
        let cloud = Sel::Lens(crate::lens::Lens { filter: crate::lens::Filter::Threads, depth: crate::lens::Depth::Cloud, at: None });
        let on_map = crate::history::Place { sel: cloud, thread: None, top: None, focus: Some("N-1".into()), cam: Some([102.5, -591.25, 0.75]), picked: Some(("project:b".into(), "N-1.p5".into())), ..place.clone() };
        s.history.back = vec![on_map.clone(), place.clone()];
        s.history.ahead = vec![crate::history::Place { sel: Sel::Agent("orch".into()), thread: None, ..place.clone() }];
        s.panes = vec![(0, Sel::Tickets), (1, Sel::Files)];
        // an agent's page: N-12 beside LAZY's list, his words to it going there and orch's going to a new thread, orch
        // on its Work view
        s.withyou.inside.insert((1, "LAZY".into()), 12);
        s.withyou.on.insert((1, "LAZY".into()), Some("N-12".into()));
        s.withyou.on.insert((0, "orch".into()), None);
        s.streams.which.insert((1, "orch".into()), crate::streams::Which::Work);
        let saved = serde_json::to_string(&take(&s)).unwrap();
        std::fs::write(path(), &saved).unwrap();

        let mut back = app(&dir);
        // the projects come back in another order: they are found by id
        back.projects.reverse();
        restore(&mut back);
        assert_eq!((back.cur, &back.sel), (0, &Sel::Agent("LAZY".into())));
        assert_eq!(back.files.kept.get("b"), Some(&files), "the Files page comes back as it was left");
        assert_eq!(back.read_places.kept.get("thread b N-12").map(String::as_str), Some("N-12-p7"), "where he was reading");
        assert_eq!(back.read_places.files.get("chat b orch").map(String::as_str), Some("s1.jsonl"), "the file a chat's place is in");
        assert_eq!(back.history.back, vec![on_map.clone(), place.clone()], "the places he left, for Back (history.rs), the Map's camera and node with its");
        assert_eq!(back.history.ahead.len(), 1, "and those gone back from");
        assert_eq!(back.panes, vec![(1, Sel::Tickets), (0, Sel::Files)]);
        assert_eq!(back.composers.get(&(0, "LAZY".into())).map(|t| t.text.as_str()), Some("half a thought"));
        assert_eq!(back.composers.get(&(0, "LAZY".into())).map(|t| (t.anchor, t.focus)), Some((4, 4)), "the caret where it stood");
        assert_eq!(back.composers.get(&(0, "q3".into())).map(|t| t.text.as_str()), Some("yes, but"), "a reply being written is its box's");
        assert_eq!(back.composers.get(&(0, crate::message_box::answer_place("N-12", 3, 1))).map(|t| t.text.as_str()), Some("orbit first"), "an answer not sent stays under its question");
        assert!(back.notes.replying.contains("m7") && back.term.pages.contains(&(1, "orch".into())));
        assert!(back.projects[0].convs.contains_key("LAZY"), "the agent shown is read once its project connects");
        assert_eq!(back.withyou.inside.get(&(0, "LAZY".into())), Some(&12), "the thread beside LAZY's list");
        assert_eq!(back.withyou.on.get(&(0, "LAZY".into())), Some(&Some("N-12".to_string())), "where his words go");
        assert_eq!(back.withyou.on.get(&(1, "orch".into())), Some(&None), "a new thread, as he chose");
        assert_eq!(back.streams.which.get(&(0, "orch".into())), Some(&crate::streams::Which::Work), "orch's view");
        let before: Saved = serde_json::from_str(r#"{"project": "b", "zoom": 1.0}"#).unwrap();
        assert!(before.with_you.is_empty() && before.with_you_on.is_empty() && before.which.is_empty(), "a ui.json from before them");

        // a later window's file: a page this window does not know (a Sel added after it), as its page and in its panes;
        // the rest comes back, his words in their boxes first, and the whole file is kept beside it (proto-window N-1446
        // p2659, 2: the way back to the previous exe)
        let mut later: Value = serde_json::from_str(&saved).unwrap();
        later["sel"] = serde_json::json!({"Later": "x"});
        later["panes"] = serde_json::json!([["b", {"Later": "x"}]]);
        let later = later.to_string();
        std::fs::write(path(), &later).unwrap();
        let mut after = app(&dir);
        restore(&mut after);
        assert_eq!(after.composers.get(&(1, "LAZY".into())).map(|t| t.text.as_str()), Some("half a thought"), "his words come back from a later window's file");
        assert_eq!(after.read_places.kept.get("thread b N-12").map(String::as_str), Some("N-12-p7"), "and where he was reading");
        assert!(after.panes.is_empty(), "the panes holding the page it does not know are left out");
        assert_eq!(std::fs::read_to_string(path().with_extension("json.unread")).ok(), Some(later), "the whole file kept beside it");

        std::fs::write(path(), "{ not json").unwrap();
        let mut fresh = app(&dir);
        restore(&mut fresh);
        assert_eq!((fresh.cur, &fresh.sel, fresh.panes.len()), (0, &Sel::View(String::new()), 0));
        assert_eq!(std::fs::read_to_string(path().with_extension("json.unread")).ok().as_deref(), Some("{ not json"), "the file not read, kept beside it");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// His folds come back after a start and a swap (paper-research N-1446 p1938, 2: "Every fold he makes is lost at each
    /// start and each release swap"): a's structure folded by a press on its head stays folded, b's left unfolded stays
    /// so, and a thread he unfolded in b's and the outline's group he folded in a's come back in the project of their
    /// id; a post opened whole is not a fold and is not kept. A first start, and a file from before folds were kept,
    /// fold every project's structure. Red: the old `Saved`, which kept no fold.
    #[test]
    fn his_folds_come_back_after_a_start_and_a_swap_and_a_first_start_folds_the_structure() {
        use crate::sidebar::section_key;
        // YAAIHH_CONFIG is the process's: not while another test reads or points it (view_cost.rs `one`)
        let _one = crate::view_cost::one();
        let dir = std::env::temp_dir().join(format!("yaaihh-state-folds-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = app(&dir);
        // a thread of a's: the structure is drawn only in a project that has one (sidebar.rs `structure`, p3085)
        let v: coord_types::TicketView = serde_json::from_value(serde_json::json!({"id": "N-12", "n": 12, "kind": "thread", "opener": "owner", "ctx": {},
            "form": {}, "title": "the look", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1,
            "opened_at": "2026-11-19T08:00:00Z", "given": [], "posts": []})).unwrap();
        s.projects[0].live.tickets.insert(12, v);
        let _ = std::fs::remove_file(path());
        let mut rt = crate::view_cost::window(s);
        crate::picture::docket::tests::frames(&mut rt);
        assert!(rt.find("structure/section-header").is_some() && !rt.state.streams.open.contains(&section_key(0)), "a's structure drawn unfolded");
        crate::picture::docket::tests::press(&mut rt, "structure/section-header");
        assert!(rt.state.streams.open.contains(&section_key(0)), "folded by the press on its head");
        rt.update(|s, _| s.streams.open.extend(["tree|1|N-12", "outline|0|group:All active", "N-12.p3"].map(String::from)));
        std::fs::write(path(), serde_json::to_string(&take(&rt.state)).unwrap()).unwrap();
        drop(rt);
        let folds = |s: &App| -> Vec<String> {
            let mut v: Vec<String> = s.streams.open.iter().filter(|k| is_fold(k)).cloned().collect();
            v.sort();
            v
        };

        let mut back = app(&dir);
        // the projects come back in another order: they are found by id
        back.projects.reverse();
        restore(&mut back);
        assert_eq!(folds(&back), ["outline|1|group:All active", "tree|0|N-12", "tree|1|section"], "as he left them, each in its project");
        assert!(!back.streams.open.contains("N-12.p3"), "a post opened whole is not a fold");
        // a swap's go puts them alike, in place of what the new window held
        let mut swapped = app(&dir);
        swapped.streams.open.insert("tree|1|N-40".into());
        retake(&mut swapped);
        assert_eq!(folds(&swapped), ["outline|0|group:All active", "tree|0|section", "tree|1|N-12"], "at a swap");

        std::fs::write(path(), r#"{"project": "b", "zoom": 1.0}"#).unwrap();
        let mut before = app(&dir);
        restore(&mut before);
        assert_eq!(folds(&before), ["tree|0|section", "tree|1|section"], "a file from before folds were kept: every structure folded");
        std::fs::remove_file(path()).unwrap();
        let mut first = app(&dir);
        restore(&mut first);
        assert_eq!(folds(&first), ["tree|0|section", "tree|1|section"], "a first start: every structure folded");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// consult-3, N-1396 p331: comments an older window kept waiting for his next message (a ui.json from before Add
    /// comment sent at once) are neither lost nor sent by themselves: each place's become its box's draft in the project
    /// shown, quoted as they would have gone, before what he typed there; a start and a swap read them alike, and the
    /// file this window writes holds none.
    #[test]
    fn comments_waiting_in_an_older_file_become_their_boxes_drafts() {
        let _one = crate::view_cost::one();
        let dir = std::env::temp_dir().join(format!("yaaihh-state-before-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = app(&dir);
        // as a window from before this head wrote it (train-two 1b182214's `Saved::comments`, per target)
        let before = r#"{"project": "b", "composers": [["b", "LAZY", "and the tests"]],
            "comments": [["LAZY", [{"source": "LAZY@10", "label": "your message at 10:02", "quote": "the lock is held", "note": "by whom?", "at": [{"start": 4, "end": 20}]}]],
                         ["thread:N-7", [{"source": "N-7.p2", "label": "N-7.p2, lead's message at 10:01", "quote": "on every start", "note": "since when?"},
                                         {"source": "N-7.p3", "label": "N-7.p3, lead's message at 10:04", "quote": "a second lock", "note": "which one?"}]]]}"#;
        std::fs::write(path(), before).unwrap();
        let chat = "> the lock is held\n> (from your message at 10:02)\n\nby whom?\n\nand the tests";
        let thread = "> on every start\n> (from N-7.p2, lead's message at 10:01)\n\nsince when?\n\n> a second lock\n> (from N-7.p3, lead's message at 10:04)\n\nwhich one?";
        let check = |s: &App| {
            assert_eq!(s.composers.get(&(1, "LAZY".into())).map(|e| e.text.as_str()), Some(chat), "the chat's box: the comment quoted, then what he typed");
            assert_eq!(s.composers.get(&(1, "thread:N-7".into())).map(|e| e.text.as_str()), Some(thread), "the thread's box: each comment, oldest first");
            assert!(s.outgoing.list.is_empty() && s.notes.picked.is_none(), "nothing goes by itself");
            let kept = take(s);
            assert!(kept.comments.is_empty(), "none written again");
            assert!(serde_json::to_value(&kept).unwrap().get("comments").is_none());
            assert!(kept.composers.iter().any(|(p, a, t)| (p.as_str(), a.as_str(), t.as_str()) == ("b", "thread:N-7", thread)), "the drafts are kept as any box's words");
        };
        restore(&mut s);
        check(&s);
        // a swap: the window taking over takes the old one's state back at "go" (`retake`), its boxes read the same way
        let mut w = app(&dir);
        retake(&mut w);
        check(&w);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// His walk item 11 (paper-research N-1446 p3521): the comments held for a place's next Send (ask.rs `Notes::held`)
    /// are written by their project's id and come back to their places at a start and at a swap, none sent by itself;
    /// `comments`, the older windows' field, stays unwritten.
    #[test]
    fn comments_held_for_a_send_are_kept_over_a_restart() {
        let _one = crate::view_cost::one();
        let dir = std::env::temp_dir().join(format!("yaaihh-state-held-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = app(&dir);
        let c = |q: &str, n: &str| ask::Comment { source: "N-7.p2".into(), label: "N-7.p2, lead's message at 10:01".into(), quote: q.into(), note: n.into(), at: Vec::new() };
        let held = vec![c("on every start", "since when?"), c("a second lock", "which one?")];
        s.notes.held.insert((1, "thread:N-7".into()), held.clone());
        assert!(write_now(&mut s));
        let text: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path()).unwrap()).unwrap();
        assert_eq!(text["held"][0][0], "b", "by the project's id: {}", text["held"]);
        assert!(text.get("comments").is_none());
        let check = |w: &App| {
            assert_eq!(w.notes.held.get(&(1, "thread:N-7".to_string())), Some(&held), "held for the thread's box again");
            assert!(w.outgoing.list.is_empty() && w.composers.is_empty(), "nothing sent, nothing put in a box");
        };
        let mut back = app(&dir);
        restore(&mut back);
        check(&back);
        let mut w = app(&dir);
        retake(&mut w);
        check(&w);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A swap brings back the thread he had open and what he was writing on it, how the outline was folded, and where
    /// he had pointed a box; a step's answer target is not kept (the step may be gone). Through the file's JSON, with
    /// no file written (the test above holds the config's place).
    #[test]
    fn a_swap_brings_back_the_thread_open_and_what_was_written_on_it() {
        use crate::message_box::{Target, Way};
        let projects = |s: &mut App, ids: [&str; 2]| {
            s.projects = ids.iter().map(|id| crate::Proj::new(&ProjectConfig { name: id.to_string(), id: id.to_string(), boxed: true }, Vec::new())).collect();
        };
        let mut s = crate::init();
        projects(&mut s, ["a", "b"]);
        s.tickets.shown = Some(450);
        s.tickets.answer.insert(450, edit("Go, but slower"));
        s.tickets.answer_fields.insert((450, "why".into()), edit("the queue"));
        s.tickets.why.insert(12, edit("because"));
        s.tickets.opening = Some("change".into());
        s.tickets.fields.insert(String::new(), edit("first post"));
        s.tickets.moving = Some((450, 3));
        s.tickets.move_to = edit("N-12");
        s.tickets.ending.insert(450, crate::ending::Ending { how: 1 });
        s.tickets.folded.insert((450, 2));
        s.tickets.by_time = true;
        s.box_targets.insert((1, "thread:N-450".into()), Target::Thread { project: 1, t: "N-450".into(), post: Some(3), way: Way::Subthread });
        s.box_targets.insert((1, "thread:N-451".into()), Target::Thread { project: 1, t: "N-451".into(), post: None, way: Way::Step { s: "s2".into() } });
        let saved: Saved = serde_json::from_str(&serde_json::to_string(&take(&s)).unwrap()).unwrap();

        let mut back = crate::init();
        projects(&mut back, ["b", "a"]);
        put_tickets(&mut back, &saved.tickets);
        let ix = |id: &str| back.projects.iter().position(|p| p.id == id);
        for (p, place, t) in saved.box_targets {
            let i = ix(&p).unwrap();
            back.box_targets.insert((i, place), t.in_project(i));
        }
        let t = &back.tickets;
        assert_eq!(t.shown, Some(450));
        assert_eq!(t.answer.get(&450).map(|e| e.text.as_str()), Some("Go, but slower"));
        assert_eq!(t.answer_fields.get(&(450, "why".into())).map(|e| e.text.as_str()), Some("the queue"));
        assert_eq!(t.why.get(&12).map(|e| e.text.as_str()), Some("because"));
        assert_eq!((t.opening.as_deref(), t.fields.get("").map(|e| e.text.as_str())), (Some("change"), Some("first post")));
        assert_eq!((t.moving, t.move_to.text.as_str()), (Some((450, 3)), "N-12"));
        assert_eq!(t.ending.get(&450).map(|e| e.how), Some(1));
        assert!(t.folded.contains(&(450, 2)) && t.by_time);
        // project b is first in this window: the target follows it there; the step's answer target is not kept
        assert_eq!(back.box_targets.get(&(0, "thread:N-450".into())), Some(&Target::Thread { project: 0, t: "N-450".into(), post: Some(3), way: Way::Subthread }));
        assert_eq!(back.box_targets.len(), 1);
    }

    /// The Add account sheet comes back opened on its step, with the name typed, the tier and the account it had
    /// already sent; a sheet never opened is not kept.
    #[test]
    fn the_add_account_form_comes_back_as_it_was_left() {
        let mut s = crate::init();
        if s.ext.try_get::<crate::accounts::Accounts>().is_none() {
            s.ext.insert(crate::accounts::Accounts::default());
        }
        assert_eq!(crate::accounts::add_kept(&s), None);
        crate::accounts::add_put_back(&mut s, true, "work", 1);
        crate::accounts::add_made_put_back(&mut s, Some(("work".into(), "work".into())));
        // the account it had sent, on Claude Code's sign-in step, comes back with it: its step, not a form that would
        // make another
        let made: Saved = serde_json::from_str(&serde_json::to_string(&take(&s)).unwrap()).unwrap();
        assert_eq!(made.add_account_made, Some(("work".to_string(), "work".to_string())));
        crate::accounts::add_key_put_back(&mut s, true);
        let saved: Saved = serde_json::from_str(&serde_json::to_string(&take(&s)).unwrap()).unwrap();
        assert_eq!(saved.add_account, Some((true, "work".to_string(), 1)));
        let mut back = crate::init();
        if back.ext.try_get::<crate::accounts::Accounts>().is_none() {
            back.ext.insert(crate::accounts::Accounts::default());
        }
        let (open, label, tier) = saved.add_account.unwrap();
        crate::accounts::add_put_back(&mut back, open, &label, tier);
        assert_eq!(crate::accounts::add_kept(&back), Some((true, "work".to_string(), 1)));
        // the API key step comes back as that step, so Add does not make a sign-in account where he meant a key
        assert!(saved.add_account_key);
        crate::accounts::add_key_put_back(&mut back, saved.add_account_key);
        assert!(crate::accounts::add_key_kept(&back));
        // a key step's account is not: its key is never kept, so it could not go on
        assert_eq!(saved.add_account_made, None);
    }

    /// Fix 4's lenses, an agent's System prompt, Initial prompt and Home, are written as the agent's page, so a window
    /// from before them opens that page, as it opens a2a's below (lens.rs `Filter::folder_part`).
    #[test]
    fn an_agents_system_prompt_initial_prompt_and_home_are_written_as_its_page() {
        use crate::lens::{Filter, Lens};
        for f in [Filter::SystemPrompt { agent: "pr".into() }, Filter::InitialPrompt { agent: "pr".into() }, Filter::AgentHome { agent: "pr".into() }] {
            assert_eq!(kept_sel(&Lens::of(f)), Sel::Agent("pr".into()));
        }
    }

    /// A lens is written as the page it is drawn from wherever a page is kept (the page shown, a pane, a tab's window
    /// and its panes, Back and forward), the lens beside it in `lenses`. A window from before lenses (`release.sh back`)
    /// reads the file whole and opens those pages; this one opens the lenses (consult-3, N-1396 p342).
    #[test]
    fn a_lens_is_written_as_the_page_it_is_drawn_from_so_an_older_window_reads_the_file() {
        use crate::lens::{Filter, Lens};
        let _one = crate::view_cost::one();
        let was = std::env::var_os("YAAIHH_CONFIG");
        let dir = std::env::temp_dir().join(format!("yaaihh-state-lens-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = app(&dir);
        let a2a = Sel::Lens(Lens::of(Filter::A2a { agent: "pr".into() }));
        let threads = Sel::Lens(Lens::threads_at("N-7"));
        let find = Sel::Lens(Lens::of(Filter::Find { words: "brain".into(), titles: true, agent: None }));
        let docket = Sel::Lens(Lens { at: Some("N-7".into()), ..Lens::of(Filter::Docket) });
        (s.cur, s.sel, s.focus) = (1, a2a.clone(), 1);
        s.panes = vec![(0, threads.clone()), (1, a2a.clone())];
        // the window shown is the live one, written from the page; the other keeps its own
        let other = crate::tabs::Layout { cur: 0, sel: find.clone(), panes: vec![(0, threads.clone()), (1, Sel::Files), (0, docket.clone())], focus: 0 };
        s.tabs.tabs = vec![crate::tabs::Tab { windows: vec![s.layout(), other], cur: 0 }];
        let place = crate::history::Place { project: "b".into(), sel: threads.clone(), thread: Some(7), panel: None, top: None, focus: None, cam: None, picked: None, tab: None, board: Vec::new(), label: "thread".into(), id: "N-7".into() };
        s.history.back = vec![place.clone()];
        s.history.ahead = vec![crate::history::Place { sel: Sel::Tickets, ..place.clone() }, crate::history::Place { sel: a2a.clone(), ..place }];
        assert!(write_now(&mut s));
        let text = std::fs::read_to_string(path()).unwrap();
        // the file as this window writes it, for build 36's own code to read (consult-3, N-1446 p181): LENS_UI_OUT=FILE,
        // read by edge6-b36read's `a_ui_json_with_lenses_reads_whole_in_build_36` (yaaihh-dev/edge6/lens/pair-b36.sh)
        if let Some(out) = std::env::var_os("LENS_UI_OUT") {
            std::fs::write(out, &text).unwrap();
        }
        let mut older: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(older["lenses"].as_array().map(Vec::len), Some(11), "each lens by its place: {}", older["lenses"]);
        older.as_object_mut().unwrap().remove("lenses");
        assert!(!older.to_string().contains("\"Lens\""), "no page an older window does not know: {older}");
        // as a window from before lenses opens it: the file whole, each lens as its page
        std::fs::write(path(), older.to_string()).unwrap();
        let mut back = app(&dir);
        restore(&mut back);
        let agent = Sel::Agent("pr".into());
        assert_eq!((back.cur, &back.sel), (1, &agent), "a2a: pr's page");
        assert_eq!(back.panes, vec![(0, Sel::Tickets), (1, agent.clone())]);
        assert_eq!(back.tabs.tabs[0].windows[1].sel, Sel::Tickets, "a search over Threads: Threads");
        assert_eq!(back.tabs.tabs[0].windows[1].panes[0].1, Sel::Tickets);
        assert_eq!(back.tabs.tabs[0].windows[1].panes[2].1, Sel::Kinds, "the Docket at a thread: the Kinds page");
        assert_eq!(back.history.back[0].sel, Sel::Tickets);
        assert_eq!(back.history.ahead.iter().map(|p| p.sel.clone()).collect::<Vec<_>>(), vec![Sel::Tickets, agent]);
        // this window: every lens where it stood
        std::fs::write(path(), &text).unwrap();
        let mut now = app(&dir);
        restore(&mut now);
        assert_eq!((now.cur, &now.sel), (1, &a2a));
        assert_eq!(now.panes, vec![(0, threads.clone()), (1, a2a.clone())]);
        assert_eq!(now.tabs.tabs[0].windows[1].sel, find);
        assert_eq!(now.tabs.tabs[0].windows[1].panes, vec![(0, threads.clone()), (1, Sel::Files), (0, docket)]);
        assert_eq!(now.history.back[0].sel, threads);
        assert_eq!(now.history.ahead[1].sel, a2a);
        assert!(serde_json::to_value(take(&now)).unwrap()["lenses"].as_array().is_some_and(|l| l.len() == 11), "and written so again");
        // a search shown as the page comes back with its words in the field, searched as typed
        now.show_sel(find.clone());
        assert!(write_now(&mut now));
        let mut found = app(&dir);
        restore(&mut found);
        assert_eq!((&found.sel, found.tickets.find.q.text.as_str(), found.tickets.find.titles), (&find, "brain", true), "the search shown, its words in the field");
        // a lens of a kind this window does not know (a newer window's): its page as written, the rest as they were
        let mut newer: serde_json::Value = serde_json::from_str(&text).unwrap();
        let entry = newer["lenses"].as_array_mut().unwrap().iter_mut().find(|e| e[0] == "panes.0").unwrap();
        entry[1] = serde_json::json!({"filter": {"Mentions": {"of": "N-7"}}, "at": "N-7"});
        std::fs::write(path(), newer.to_string()).unwrap();
        let mut next = app(&dir);
        restore(&mut next);
        assert_eq!(next.panes, vec![(0, Sel::Tickets), (1, a2a.clone())], "the unknown lens's page as written, the other lens read");
        assert_eq!(next.sel, a2a);
        let _ = std::fs::remove_dir_all(&dir);
        match was {
            Some(v) => unsafe { std::env::set_var("YAAIHH_CONFIG", v) },
            None => unsafe { std::env::remove_var("YAAIHH_CONFIG") },
        }
    }

    /// An insight lens (S6) is written as the Threads page wherever a page is kept, which a window from before lenses
    /// opens; a window with lenses but before insights opens that page too (its lens unknown, as `Mentions` above).
    #[test]
    fn an_insight_lens_is_written_as_the_threads_page() {
        let l = crate::lens::Lens { at: Some("days".into()), ..crate::lens::Lens::of(crate::lens::Filter::OpenToDone) };
        assert_eq!(kept_sel(&l), Sel::Tickets);
        let l = crate::lens::Lens { at: Some("agent:fixer".into()), ..crate::lens::Lens::of(crate::lens::Filter::WorkTalk) };
        assert_eq!(kept_sel(&l), Sel::Tickets, "insight (1) too");
    }
}
