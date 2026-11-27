//! His edit of an agent's system prompt, as a branch in its history (path fix 6; the owner, N-1104 p996: "be albe ot
//! change eg the sytem prompt etc becuasee aclaude allows that iwth --- so allow me to wedit it as a hsitry brnahc/jump
//! ofc"; the shape accounts-lead N-1446 p2447, go paper-research p2456; the editor accounts-lead's, p2704 item 2).
//!
//! On the System prompt pane (sidebar.rs `prompt_part`), Edit on Started with's line opens a text box in place under
//! it, holding the newest version's text when the window has it. One switch puts the text in place of Claude Code's own
//! (its `--system-prompt`, not `--append-system-prompt`); it opens the way the newest version runs, so that his first
//! press changes only the text (consult-lead N-1446 p2829). He goes on from now, on the conversation as it stands, or
//! from one of its turns, picked by its time and first line: a copy of the conversation is cut after that turn and the
//! agent goes on from there; a pick on a session the agent has since left goes back to Now (proto-mind p2830). Sent as
//! `Request::SystemPrompt` (protocol 51). The engine writes the version as a file of its own and restarts the agent
//! with it once its turn ends; the old conversation stays as it was. The branch in its history is drawn from the
//! record (docket-machine graph.rs's `began`), not here.
//!
//! On the Map, a version's card has the same Edit (cloud.rs `card`; paper-research N-1446 p3041, 3): it opens this
//! editor apart over the Map (popup.rs `What::PromptEdit`), its box holding that version's text.

use crate::App;
use coord_types::{Entry, Request};
use rune::{El, TextEdit, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit, RadioItem, Switch, TextArea};
use std::sync::mpsc::{Receiver, TryRecvError};

/// How many of its turns the editor lists before he asks for all of them, newest first.
const TURNS_SHOWN: usize = 8;

/// The edit he has open on a System prompt pane, and what the engine answered his last one.
#[derive(Default)]
pub struct PromptEdit {
    /// The agent whose edit is open, by its project and name; None while none is.
    pub of: Option<(usize, String)>,
    pub text: TextEdit,
    /// In place of Claude Code's own system prompt, rather than after it.
    pub replace: bool,
    /// Where it goes on from: None, now; else the session he picked a turn of, as the chat names it, and the byte
    /// there where the turn after the one he picked begins ([`Turn::after`]).
    pub after: Option<(String, u64)>,
    /// Every turn the window holds is listed, not only the newest [`TURNS_SHOWN`].
    pub all: bool,
    pub running: Option<Receiver<Result<String, String>>>,
    pub error: Option<String>,
    /// His last edit the engine took: its agent by project and name, and what the pane says of it until he edits again.
    pub sent: Option<(usize, String, String)>,
    /// Opened from a version's card on the Map ([`Start::Node`]): drawn apart over it (popup.rs `What::PromptEdit`), not
    /// under the System prompt pane's line, and its panel's Close cancels it.
    pub apart: bool,
    /// The version's node (`text:…`) whose text the box waits for, asked of coordd as a panel's text is (sidebar.rs
    /// `apart_poll`): one the window did not hold when he pressed Edit on its card. The box takes it as it comes.
    pub reading: Option<String>,
    /// The parts editor's draft (sidebar.rs `parts_rows`; p2995): the agent whose parts he switched, by project and name,
    /// and each part he switched, by name, with what it goes on as and its text where that is his.
    pub parts: Option<(usize, String)>,
    pub picks: Picks,
    /// The edit sent came from the parts editor ([`send_parts`]): its answer is said under the parts, not under Your words.
    pub from_parts: bool,
    /// The drafts of the other agents' parts, by project and name, kept while he drafts another's (the owner, N-1790
    /// part 1: drafts per agent) and in ui.json ([`saved`]).
    pub kept: std::collections::HashMap<(usize, String), Picks>,
    /// The parts whose text he shows beside Claude Code's own (Diff), by agent and part.
    pub diff: std::collections::HashSet<(usize, String, String)>,
    /// The pills of parts left out that he opened on the system prompt as sent (prompt_diff.rs), by agent and part.
    pub pills: std::collections::HashSet<(usize, String, String)>,
    /// What the parts editor drew last of the agent whose draft is open (sidebar.rs `parts_rows`): the version it
    /// makes, in place of Claude Code's own or not, and how many characters longer it is than what it runs with now,
    /// for the Inspect drawer's footer (inspect.rs).
    pub made: std::cell::RefCell<Option<Made>>,
}

/// A draft of an agent's parts: each part he switched, by name, with what it goes on as and its text where that is his.
pub type Picks = std::collections::BTreeMap<String, (crate::prompt_parts::Pick, TextEdit)>;

/// What a draft of agent NAME's parts in project PI makes ([`PromptEdit::made`]): the version, None when every part
/// is empty, and its length less what it runs with now, in characters.
#[derive(Clone, Debug, PartialEq)]
pub struct Made {
    pub pi: usize,
    pub name: String,
    pub version: Option<(String, bool)>,
    pub grows: i64,
}

/// A draft kept in ui.json: project id, agent, and each part's name, pick and text.
pub type SavedDraft = (String, String, Vec<(String, String, String)>);

/// The drafts of every agent's parts, the open one too, for ui.json (state.rs).
pub(crate) fn saved(s: &App) -> Vec<SavedDraft> {
    let e = &s.prompt_edit;
    let id = |i: usize| s.projects.get(i).map(|p| p.id.clone());
    let open = e.parts.iter().map(|(pi, n)| ((*pi, n.clone()), &e.picks));
    let mut out: Vec<SavedDraft> = open
        .chain(e.kept.iter().map(|(k, v)| (k.clone(), v)))
        .filter(|(_, v)| !v.is_empty())
        .filter_map(|((pi, n), v)| Some((id(pi)?, n, v.iter().map(|(part, (p, t))| (part.clone(), p.word().to_string(), t.text.clone())).collect())))
        .collect();
    out.sort();
    out
}

/// The drafts ui.json kept ([`saved`]), each kept for its agent until he opens its parts again; IX finds a project by id.
pub(crate) fn restore(s: &mut App, v: Vec<SavedDraft>, ix: impl Fn(&str) -> Option<usize>) {
    for (p, name, parts) in v {
        let Some(pi) = ix(&p) else { continue };
        let picks: Picks = parts.into_iter().filter_map(|(part, w, t)| Some((part, (crate::prompt_parts::Pick::of_word(&w)?, TextEdit::new(&t))))).collect();
        if !picks.is_empty() {
            s.prompt_edit.kept.insert((pi, name), picks);
        }
    }
}

/// The draft of agent NAME's parts in project PI, open or kept; none when he has none.
pub(crate) fn draft_of<'a>(s: &'a App, pi: usize, name: &str) -> Option<&'a Picks> {
    let e = &s.prompt_edit;
    match e.parts.as_ref().is_some_and(|(p, n)| *p == pi && n == name) {
        true => Some(&e.picks),
        false => e.kept.get(&(pi, name.to_string())),
    }
}

/// Where an edit's box takes its first text from ([`edit_press`]): the newest version's text as System prompt's pane
/// holds it; or a version's node on the Map (`text:…`), from its card there (cloud.rs `card`), its text as the window
/// holds it, else asked of coordd ([`PromptEdit::reading`]).
pub enum Start {
    Text(String),
    Node(String),
}

/// A turn of an agent's conversation he can go on from: the prompt that began it, and AFTER, the byte where the next
/// turn's prompt begins in its session, so that a copy cut there keeps this turn whole. The engine checks that the
/// bytes before AFTER end with a turn that ended (coordd follow.rs `turn_end`) and refuses the edit if they do not.
#[derive(Debug, PartialEq)]
pub struct Turn<'a> {
    pub prompt: &'a Entry,
    pub after: u64,
}

/// The turns of ENTRIES (a chat's, oldest first) he can go on from, oldest first. A turn begins at a prompt, a `user`
/// or `mail` entry as turn.rs `span` reads one, given after the agent's own work since the turn before: a reply, a tool
/// call or a result, on another line than the prompt. The newest turn is left out: it has no turn after it, so going on
/// after it is Now. An interrupt's note ("[Request interrupted by user]") begins no turn: it ends the one it stands in.
pub fn turns(entries: &[Entry]) -> Vec<Turn<'_>> {
    let mut starts: Vec<&Entry> = Vec::new();
    // the first prompt held begins a turn; WORKED_AT is the line of the agent's work after the last turn's start
    let mut worked_at: Option<u64> = Some(u64::MAX);
    for e in entries {
        match e.kind.as_str() {
            "user" | "mail" if e.text.starts_with("[Request interrupted") => {}
            "user" | "mail" if worked_at.is_some_and(|w| w != e.offset) => {
                starts.push(e);
                worked_at = None;
            }
            "user" | "mail" => {}
            _ => worked_at = Some(e.offset),
        }
    }
    starts.windows(2).map(|w| Turn { prompt: w[0], after: w[1].offset }).collect()
}

/// Why agent NAME's system prompt cannot be edited in project PI ([`editable`]), none when it can.
pub fn why_not(s: &App, pi: usize, name: &str) -> Option<String> {
    editable(s, pi, name).err()
}

/// The text box of part PART of agent NAME's in project PI on the parts editor, switched to PICK, TEXT its first text:
/// the draft switched to another agent's parts starts again.
pub fn part_text<'a>(s: &'a mut App, pi: usize, name: &str, part: &str, pick: crate::prompt_parts::Pick, text: &str) -> &'a mut TextEdit {
    take_up(s, pi, name);
    let e = &mut s.prompt_edit;
    &mut e.picks.entry(part.to_string()).or_insert_with(|| (pick, TextEdit::new(text))).1
}

/// Part PART of agent NAME's in project PI switched to PICK, its box holding TEXT where it is his; back to Stock or Off
/// where it goes on so now (NOW), no longer drafted.
pub fn pick_part(s: &mut App, pi: usize, name: &str, part: &str, pick: crate::prompt_parts::Pick, text: &str, now: crate::prompt_parts::Pick) {
    *part_text(s, pi, name, part, pick, text) = focused(text);
    let e = &mut s.prompt_edit;
    match pick == now && matches!(pick, crate::prompt_parts::Pick::Stock | crate::prompt_parts::Pick::Off) {
        true => drop(e.picks.remove(part)),
        false => e.picks.get_mut(part).expect("just drafted").0 = pick,
    }
    if e.picks.is_empty() {
        e.parts = None;
    }
}

/// Agent NAME's draft of parts in project PI made the open one: another agent's open draft kept for it, this one's
/// taken back up where he left it ([`PromptEdit::kept`]).
pub(crate) fn take_up(s: &mut App, pi: usize, name: &str) {
    let e = &mut s.prompt_edit;
    if e.parts.as_ref().is_some_and(|(p, n)| *p == pi && n == name) {
        return;
    }
    let back = e.kept.remove(&(pi, name.to_string())).unwrap_or_default();
    let was = std::mem::replace(&mut e.picks, back);
    if let Some(of) = e.parts.take().filter(|_| !was.is_empty()) {
        e.kept.insert(of, was);
    }
    (e.parts, e.error) = (Some((pi, name.to_string())), None);
}

/// The parts editor's draft of agent NAME's in project PI dropped.
pub fn drop_parts(s: &mut App) {
    let e = &mut s.prompt_edit;
    (e.parts, e.error) = (None, None);
    e.picks.clear();
}

/// His draft of part PART of agent NAME's in project PI dropped: it goes on as it does now (Reset).
pub fn reset_part(s: &mut App, pi: usize, name: &str, part: &str) {
    let e = &mut s.prompt_edit;
    if e.parts.as_ref().is_some_and(|(p, n)| *p == pi && n == name) {
        e.picks.remove(part);
        if e.picks.is_empty() {
            e.parts = None;
        }
    } else if let Some(k) = e.kept.get_mut(&(pi, name.to_string())) {
        k.remove(part);
    }
}

/// The version the parts editor made of agent NAME's in project PI sent, TEXT in place of Claude Code's own with
/// REPLACE, else after it, going on from the turn picked in the Inspect drawer's footer (Branch from turn), else from
/// now: as [`send`] sends Edit's, its answer said under the parts.
pub fn send_parts(s: &mut App, pi: usize, name: &str, text: String, replace: bool) {
    let session = s.projects.get(pi).and_then(|p| p.convs.get(name)).map(|c| c.session.clone());
    let e = &mut s.prompt_edit;
    if e.running.is_some() || e.of.is_some() {
        return;
    }
    let after = e.after.take().filter(|(of, _)| Some(of) == session.as_ref());
    (e.of, e.after, e.replace, e.from_parts) = (Some((pi, name.to_string())), after, replace, true);
    e.text = TextEdit::new(&text);
    send(s);
    let e = &mut s.prompt_edit;
    if e.running.is_none() {
        (e.of, e.from_parts) = (None, false);
    }
}

/// Whether agent NAME's system prompt can be edited in project PI, else why not, as Edit's hover says it: from an
/// engine that takes the edit (protocol 51), the pipe up, of one of its agents. Which harness and which project take
/// it is the engine's to say (the owner, N-1790 part 1: no gate here on the harness or a box): coordd's refusal is said
/// under the edit, in the Inspect drawer's footer.
fn editable(s: &App, pi: usize, name: &str) -> Result<(), String> {
    let Some(p) = s.projects.get(pi) else { return Err("Its project is gone.".into()) };
    if p.down.is_some() {
        return Err("The pipe to the engine is down.".into());
    }
    if p.protocol < coord_types::SYSTEM_PROMPT_PROTOCOL {
        return Err(format!("Needs a newer engine (protocol {}).", coord_types::SYSTEM_PROMPT_PROTOCOL));
    }
    match p.live.agents.iter().any(|a| a.name == name) {
        false => Err(format!("{name} is not one of its agents now.")),
        true => Ok(()),
    }
}

/// Open the edit of agent NAME's system prompt in project PI, TEXT its newest version's text as the pane holds it (empty
/// when it holds none) and REPLACE whether that version is in place of Claude Code's own, going on from now; its
/// conversation read, if the window holds none of it, for its turns. The box takes his first key, its caret at the
/// start, as New agent's field does (proto-window's walk, N-1446 p3038 6). From a version's card on the Map, START a
/// node: the box holds that version's text, or waits for it, and the edit opens apart over the Map.
fn open(s: &mut App, pi: usize, name: &str, start: &Start, replace: bool) {
    if s.prompt_edit.running.is_some() {
        return;
    }
    let (text, apart, reading) = match start {
        Start::Text(t) => (t.clone(), false, None),
        Start::Node(id) => match s.folders.texts.get(&(pi, id.clone())) {
            Some(Ok(t)) => (t.clone(), true, None),
            _ => (String::new(), true, Some(id.clone())),
        },
    };
    // his drafts of parts stay as they are
    let e = &mut s.prompt_edit;
    let (kept, parts, picks, diff) = (std::mem::take(&mut e.kept), e.parts.take(), std::mem::take(&mut e.picks), std::mem::take(&mut e.diff));
    *e = PromptEdit { of: Some((pi, name.to_string())), text: focused(&text), replace, apart, reading, kept, parts, picks, diff, ..Default::default() };
    if let Some(p) = s.projects.get_mut(pi) {
        p.open_conv(name);
    }
    if apart {
        crate::popup::open(s, crate::popup::title(&format!("{name}'s system prompt"), "", "your edit"), crate::popup::What::PromptEdit { pi, agent: name.to_string() });
    }
}

/// TEXT in the edit's box, which takes his first key, its caret at the start.
fn focused(text: &str) -> TextEdit {
    let mut t = TextEdit::new(text);
    (t.anchor, t.focus, t.focused) = (0, 0, true);
    t
}

/// Cancel the edit open; opened from the Map, its panel closes with it.
fn cancel(s: &mut App) {
    let e = &mut s.prompt_edit;
    let Some((pi, agent)) = e.of.take() else { return };
    e.reading = None;
    if std::mem::take(&mut e.apart) {
        let what = crate::popup::What::PromptEdit { pi, agent };
        s.popups.retain(|p| p.what != what);
    }
}

/// Send the edit open: refused here only with no text, the rest as the engine refuses it.
fn send(s: &mut App) {
    let e = &mut s.prompt_edit;
    let Some((pi, name)) = e.of.clone() else { return };
    let text = e.text.text.trim_end().to_string();
    if e.running.is_some() || text.trim().is_empty() {
        return;
    }
    let req = Request::SystemPrompt { name, text, replace: e.replace, after: e.after.as_ref().map(|(_, b)| *b) };
    e.error = None;
    match s.projects.get_mut(pi).and_then(|p| p.ask_roster(&req)) {
        Some(rx) => e.running = Some(rx),
        None => e.error = Some("Disconnected; send once it is back.".into()),
    }
}

/// Each frame's step of the edit open: a pick on a session its agent has since left goes back to Now, said under the
/// edit; and the engine's answer to the edit sent, once it came: the edit closed, the pane saying it was taken; or its
/// refusal said under the edit, as is an answer that can no longer come. True when either changed it.
pub fn update(s: &mut App) -> bool {
    let read = took_text(s);
    let left = match (&s.prompt_edit.of, &s.prompt_edit.after) {
        (Some((pi, name)), Some((session, _))) if s.prompt_edit.running.is_none() => s.projects.get(*pi).and_then(|p| p.convs.get(name)).is_some_and(|c| c.session != *session),
        _ => false,
    };
    let e = &mut s.prompt_edit;
    if left {
        e.after = None;
        e.error = Some("New session; pick a turn again.".into());
    }
    let r = match e.running.as_ref().map(|rx| rx.try_recv()) {
        None | Some(Err(TryRecvError::Empty)) => return left || read,
        Some(Ok(r)) => r,
        Some(Err(TryRecvError::Disconnected)) => Err("No answer; check its versions before resending.".into()),
    };
    e.running = None;
    match (r, e.of.take()) {
        (Ok(_), Some((pi, name))) if e.from_parts => {
            (e.from_parts, e.parts) = (false, None);
            e.picks.clear();
            let from = if e.after.take().is_some() { "from the picked turn, on a copy" } else { "on its conversation as it stands" };
            let when = chrono::Local::now().format("%H:%M");
            e.sent = Some((pi, name.clone(), format!("Sent at {when}: these parts {from}, after its turn.")));
        }
        (Err(why), _) if e.from_parts => {
            e.from_parts = false;
            e.error = Some(why);
        }
        (Ok(_), Some((pi, name))) => {
            let from = if e.after.is_some() { "from the picked turn, on a copy" } else { "on its conversation as it stands" };
            let when = chrono::Local::now().format("%H:%M");
            e.sent = Some((pi, name.clone(), format!("Sent at {when}: this prompt {from}, after its turn.")));
        }
        (Ok(_), None) => {}
        (Err(why), of) => {
            e.of = of;
            e.error = Some(why);
        }
    }
    true
}

/// The text the box waits for ([`PromptEdit::reading`]), once the window holds it: in the box, or why it was not read
/// said under the edit; and the edit opened apart cancelled once its panel is closed, but while it is being sent. True
/// when either changed it.
fn took_text(s: &mut App) -> bool {
    let e = &s.prompt_edit;
    let shut = e.apart && e.running.is_none() && e.of.as_ref().is_some_and(|(pi, agent)| !crate::popup::shows(s, &crate::popup::What::PromptEdit { pi: *pi, agent: agent.clone() }));
    if shut {
        cancel(s);
        return true;
    }
    let got = match (&e.of, &e.reading) {
        (Some((pi, _)), Some(id)) => s.folders.texts.get(&(*pi, id.clone())).cloned(),
        _ => None,
    };
    let Some(got) = got else { return false };
    let e = &mut s.prompt_edit;
    e.reading = None;
    match got {
        Ok(t) => e.text = focused(&t),
        Err(why) => e.error = Some(format!("Its text not read: {}", crate::sidebar::first_line(&why))),
    }
    true
}

/// What turn T is shown as in the list: its time, and the first line of its prompt, a delivery's by what it is.
fn turn_words(t: &Turn) -> (String, String) {
    let e = t.prompt;
    let first = match e.kind.as_str() {
        "mail" => crate::chat::channel_heading(&e.text).unwrap_or_else(|| crate::texts::first_line_cut(&e.text)),
        _ => crate::texts::first_line_cut(&e.text),
    };
    (crate::when::at(&e.at), first)
}

/// Edit on Started with's line of agent NAME's system prompt in project PI (sidebar.rs `prompt_part`), its box holding
/// START (the newest version's text as the pane holds it), REPLACE whether that version is in place of Claude Code's
/// own; or on a version's card on the Map, START its node (cloud.rs `card`); quiet, and with why not on its hover where
/// it cannot be edited. None while its edit is open.
pub fn edit_press(s: &App, pi: usize, name: &str, start: Start, replace: bool, k: &Kit) -> Option<El> {
    if s.prompt_edit.of.as_ref().is_some_and(|(p, n)| *p == pi && n == name) {
        return None;
    }
    let why = editable(s, pi, name).err();
    let key = match start {
        Start::Text(_) => format!("prompt-edit-{name}"),
        Start::Node(_) => format!("map-prompt-edit-{name}"),
    };
    let n = name.to_string();
    let b = Button::new("Edit").variant(ButtonVariant::Ghost).disabled(why.is_some() || s.prompt_edit.running.is_some()).on_press(move |s: &mut App| open(s, pi, &n, &start, replace)).build(k).key(key);
    Some(match why {
        Some(w) => b.help(w),
        None => b.help("Edit its system prompt"),
    })
}

/// The edit of agent NAME's system prompt in project PI, under Started with's line while it is open; else what the
/// engine answered his last edit of it, while there is one. None for neither.
pub fn editor(s: &App, pi: usize, name: &str, k: &Kit) -> Option<El> {
    let e = &s.prompt_edit;
    let note = |t: String| text(t).font(k.ty.small).color(k.c.text_faint).wrap();
    if !e.of.as_ref().is_some_and(|(p, n)| *p == pi && n == name) {
        let (_, _, said) = e.sent.as_ref().filter(|(p, n, _)| *p == pi && n == name)?;
        return Some(note(said.clone()).key(format!("prompt-edit-sent-{name}")));
    }
    if e.from_parts {
        return None;
    }
    let running = e.running.is_some();
    let area = TextArea::new(&e.text)
        .width(None)
        .spellcheck(false)
        .placeholder(if e.reading.is_some() { "Reading the version's text…" } else { "The system prompt it goes on with" })
        .disabled(running || e.reading.is_some())
        .auto_grow(6, 24)
        .on_edit(|s: &mut App| &mut s.prompt_edit.text)
        .build(k);
    let switch = Switch::new(e.replace).label("Replace Claude Code's own").on_toggle(|s: &mut App| s.prompt_edit.replace = !s.prompt_edit.replace).build(k);
    let replace_note = if e.replace { "On: replaces Claude Code's prompt." } else { "Off: appended to Claude Code's prompt." };
    let mut c = col()
        .key(format!("prompt-editor-{name}"))
        .w_full()
        .gap(8.0)
        .pt(6.0)
        .pb(10.0)
        .child(col().key("prompt-edit-text").w_full().child(area))
        .child(col().w_full().gap(2.0).child(row().child(switch.key("prompt-edit-replace"))).child(note(replace_note.into())))
        .child(text("Go on from").font(k.ty.small_medium).color(k.c.text_muted))
        .child(turn_list(s, pi, name, k));
    let how = match e.after {
        None => "Once its running turn ends.",
        Some(_) => "Goes on from a copy at that turn.",
    };
    c = c.child(note(how.into()));
    if let Some(why) = &e.error {
        c = c.child(text(crate::sidebar::first_line(why)).font(k.ty.small).color(k.c.danger.fg).wrap().key("prompt-edit-error"));
    }
    let empty = e.text.text.trim().is_empty();
    let go = Button::new(if running { "Sending…" } else { "Go on with it" }).variant(ButtonVariant::Contrast).disabled(running || empty).on_press(send).build(k);
    let cancel = Button::new("Cancel").variant(ButtonVariant::Ghost).disabled(running).on_press(cancel).build(k);
    Some(c.child(row().w_full().justify_end().gap(8.0).child(cancel.key("prompt-edit-cancel")).child(go.key("prompt-edit-go"))))
}

/// Where an edit of agent NAME's system prompt in project PI goes on from: Now, or after one of its turns, the newest
/// first (its edit's Go on from; the Inspect drawer's Branch from turn, inspect.rs).
pub(crate) fn turn_list(s: &App, pi: usize, name: &str, k: &Kit) -> El {
    let e = &s.prompt_edit;
    let note = |t: String| text(t).font(k.ty.small).color(k.c.text_faint).wrap();
    let mut c = col().w_full().gap(2.0).child(RadioItem::new("Now, after its turn", e.after.is_none()).on_select(|s: &mut App| s.prompt_edit.after = None).build(k).key("prompt-edit-now"));
    let conv = s.projects.get(pi).and_then(|p| p.convs.get(name));
    let list = conv.map(|c| turns(&c.entries)).unwrap_or_default();
    let shown = if e.all { list.len() } else { list.len().min(TURNS_SHOWN) };
    let session = conv.map(|c| c.session.as_str()).unwrap_or_default();
    for t in list.iter().rev().take(shown) {
        let after = t.after;
        let (when, first) = turn_words(t);
        let picked = e.after.as_ref().is_some_and(|(of, b)| of == session && *b == after);
        let of = session.to_string();
        let pick = move |s: &mut App| s.prompt_edit.after = Some((of.clone(), after));
        // its first line beside the radio, cut with "…" at the pane's edge: rune-kit's radio draws its label on one line
        // uncut, and it ran past the edge (proto-window's walk, N-1446 p3038 8); a press on the words picks it too
        let radio = RadioItem::new(format!("After {when}"), picked).on_select(pick.clone()).build(k);
        let words = text(format!("· {first}")).font(k.ty.ui.lh(20.0)).color(k.c.text_base).truncate().grow(1.0).help(first);
        c = c.child(row().w_full().gap(6.0).items_start().cursor(rune::Cursor::Pointer).on_press(pick).child(radio).child(words).key(format!("prompt-edit-turn-{after}")));
    }
    if shown < list.len() {
        let more = format!("All {} turns", list.len());
        c = c.child(row().child(Button::new(more).variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.prompt_edit.all = true).build(k).key("prompt-edit-all")));
    }
    match conv {
        None => c = c.child(note("Reading its conversation for its turns…".into())),
        Some(v) if !v.loaded => c = c.child(note(v.refused.as_ref().map_or_else(|| "Reading its conversation for its turns…".into(), |r| format!("Its turns not read: {}", crate::sidebar::first_line(r))))),
        Some(v) if e.all && v.start > 0 => {
            let n = name.to_string();
            let earlier = Button::new(if v.older_pending { "Reading earlier turns…" } else { "Earlier turns" }).variant(ButtonVariant::Ghost).disabled(v.older_pending).on_press(move |s: &mut App| {
                if let Some(p) = s.projects.get_mut(pi) {
                    p.read_older(&n);
                }
            });
            c = c.child(row().child(earlier.build(k).key("prompt-edit-earlier")));
        }
        Some(_) => {}
    }
    c
}

/// What the turn picked to go on from is called (the Inspect drawer's Branch from turn ▾): Now, or after its time.
pub(crate) fn after_words(s: &App, pi: usize, name: &str) -> String {
    let e = &s.prompt_edit;
    let conv = s.projects.get(pi).and_then(|p| p.convs.get(name));
    let picked = e.after.as_ref().filter(|(of, _)| conv.is_some_and(|c| c.session == *of)).and_then(|(_, b)| conv.and_then(|c| turns(&c.entries).into_iter().find(|t| t.after == *b)).map(|t| turn_words(&t).0));
    match picked {
        Some(when) => format!("After {when}"),
        None => "From now".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(offset: u64, kind: &str, text: &str) -> Entry {
        Entry { offset, at: "2026-11-24T10:00:00Z".into(), kind: kind.into(), tool: None, text: text.into(), patch: None, images: Vec::new() }
    }

    /// A turn begins at a prompt given after the agent's own work, on another line; the newest has no turn after it
    /// (Now's); an interrupt's note, a second prompt before any work, and text on a result's own line begin none.
    #[test]
    fn a_turn_he_can_go_on_from_ends_where_the_next_prompt_after_its_work_begins() {
        let entries = vec![
            e(0, "user", "turn one"),
            e(100, "assistant", "one's reply"),
            e(200, "mail", "<channel source=\"coord\" ticket=\"N-4\">\nturn two\n</channel>"),
            e(300, "mail", "<channel source=\"coord\" ticket=\"N-5\">\nalso in turn two\n</channel>"),
            e(400, "tool", "ls"),
            e(500, "result", "a.rs"),
            e(500, "user", "text on the result's line"),
            e(600, "user", "[Request interrupted by user for tool use]"),
            e(700, "user", "turn three"),
            e(800, "assistant", "three's reply"),
            e(900, "user", "turn four, running now"),
        ];
        let got: Vec<(&str, u64)> = turns(&entries).iter().map(|t| (t.prompt.text.as_str(), t.after)).collect();
        assert_eq!(got, [("turn one", 200), (entries[2].text.as_str(), 700), ("turn three", 900)]);
        assert!(turns(&entries[..2]).is_empty(), "one turn: going on after it is Now");
    }

    /// A part switched is drafted with its text; switched back to Stock where it went on as Stock, it is not; the draft
    /// of another agent's parts is its own, and his first agent's is kept for it, taken back up where he left it, and
    /// kept in ui.json.
    #[test]
    fn a_part_switched_is_drafted_and_switched_back_it_is_not() {
        use crate::prompt_parts::Pick;
        let (_held, mut s) = crate::view_cost::test_app();
        // a project for ui.json to name the drafts by
        s.projects = vec![crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new())];
        pick_part(&mut s, 0, "orch", "Care", Pick::Modified, "Care, stock.", Pick::Stock);
        assert_eq!(s.prompt_edit.parts, Some((0, "orch".into())));
        assert_eq!(s.prompt_edit.picks.get("Care").map(|(p, t)| (*p, t.text.as_str())), Some((Pick::Modified, "Care, stock.")));
        part_text(&mut s, 0, "orch", "Care", Pick::Modified, "unused").text.push_str(" Mine.");
        assert_eq!(s.prompt_edit.picks["Care"].1.text, "Care, stock. Mine.");
        pick_part(&mut s, 0, "orch", "Care", Pick::Stock, "Care, stock.", Pick::Stock);
        assert!(s.prompt_edit.picks.is_empty() && s.prompt_edit.parts.is_none());
        pick_part(&mut s, 0, "orch", "Pronouns", Pick::Custom, "", Pick::Stock);
        pick_part(&mut s, 0, "lead", "Care", Pick::Custom, "", Pick::Stock);
        assert_eq!(s.prompt_edit.picks.keys().collect::<Vec<_>>(), ["Care"]);
        assert_eq!(s.prompt_edit.parts, Some((0, "lead".into())));
        assert_eq!(s.prompt_edit.kept.get(&(0, "orch".into())).map(|k| k.keys().cloned().collect::<Vec<_>>()), Some(vec!["Pronouns".to_string()]));
        part_text(&mut s, 0, "orch", "Pronouns", Pick::Custom, "unused").text.push_str("Mine.");
        assert_eq!((s.prompt_edit.parts.clone(), s.prompt_edit.picks["Pronouns"].1.text.as_str()), (Some((0, "orch".into())), "Mine."));
        assert!(s.prompt_edit.kept.contains_key(&(0, "lead".into())) && draft_of(&s, 0, "lead").is_some_and(|d| d.contains_key("Care")));
        reset_part(&mut s, 0, "lead", "Care");
        assert!(draft_of(&s, 0, "lead").is_none_or(|d| d.is_empty()));
        pick_part(&mut s, 0, "lead", "Care", Pick::Off, "", Pick::Stock);
        let saved = saved(&s);
        assert_eq!(saved.len(), 2, "{saved:?}");
        let ids: Vec<String> = s.projects.iter().map(|p| p.id.clone()).collect();
        (s.prompt_edit.kept, s.prompt_edit.parts) = (Default::default(), None);
        s.prompt_edit.picks.clear();
        restore(&mut s, saved, |id| ids.iter().position(|p| p == id));
        assert_eq!(draft_of(&s, 0, "orch").map(|d| (d["Pronouns"].0, d["Pronouns"].1.text.clone())), Some((Pick::Custom, "Mine.".to_string())));
        assert_eq!(draft_of(&s, 0, "lead").map(|d| d["Care"].0), Some(Pick::Off));
    }

    /// The stills of an agent's System prompt pane, all of it made up (path fix 6; the editor, N-1446 p2704 item 2):
    /// Edit at the end of Started with's line (prompt-edit-closed.png); his edit open, version 1's text in the box with
    /// a line he added, Replace off as version 1 runs, the turn at 09:30 picked, the running turn at 10:00 not listed
    /// (prompt-edit-open.png). `STILL_OUT=DIR … prompt_edit::tests::edit_still --ignored --exact`.
    #[test]
    #[ignore]
    fn edit_still() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn base(open: bool) -> App {
            use docket_machine::graph_slice::Row;
            let (_held, mut s) = crate::view_cost::test_app();
            s.notice = None;
            let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
            p.down = None;
            p.protocol = coord_types::PROTOCOL;
            p.live.agents = vec![serde_json::from_value(serde_json::json!({"id": "AGT-1", "name": "orch", "parent": "owner", "kind": "claude", "model": "claude-opus-5-5", "state": "working",
                "since": "2026-11-24T10:00:00Z", "last_active": "2026-11-24T10:02:00Z", "tools": [], "items": []})).unwrap()];
            let at = |m: &str| format!("2026-11-24T{m}:00Z");
            let entry = |offset: u64, m: &str, kind: &str, text: &str| Entry { offset, at: at(m), kind: kind.into(), tool: None, text: text.into(), patch: None, images: Vec::new() };
            let c = p.convs.entry("orch".into()).or_default();
            c.entries = vec![
                entry(0, "09:00", "user", "Read the parser and say where it loses positions"),
                entry(400, "09:04", "assistant", "It drops them in the lexer's error path."),
                entry(900, "09:30", "user", "Fix the lexer's error path and add a test"),
                entry(1300, "09:31", "tool", "cargo test -p parser"),
                entry(1700, "09:33", "result", "test result: ok. 41 passed"),
                entry(2100, "09:34", "assistant", "Fixed, with a test for each error kind."),
                entry(2600, "10:00", "user", "Now write the docs for it"),
            ];
            (c.loaded, c.session, c.next) = (true, "/made-up/s1.jsonl".into(), 3000);
            let start = |line: usize, what: &str| Row { text: format!("2026-11-24T08:{line:02}:00Z: {what}"), of: format!("start:orch@{line}"), ..Default::default() };
            let started = vec![
                Row { text: "the standing text, appended; no start line says which version: carried in 40 requests, 12 turns, 3 steps".into(), of: docket_machine::graph_slice::NOT_SAID.into(), ..Default::default() },
                Row { text: "version 1, appended: carried in 25 requests, 9 turns, 2 steps".into(), of: "text:s1".into(), rows: vec![start(10, "system: system prompt 1, by owner")], ..Default::default() },
            ];
            let own = vec![Row { text: "block 1: 1 version".into(), of: "text:c1".into(), rows: vec![Row { text: "version 1: carried in 65 requests, 21 turns, 5 steps".into(), of: "text:c1".into(), ..Default::default() }], ..Default::default() }];
            let rows = vec![
                Row { text: "started with: 2 versions".into(), rows: started, ..Default::default() },
                Row { text: "Claude Code's own: 1 block, 1 version".into(), rows: own, ..Default::default() },
            ];
            s.folders.requests.insert((0, "orch".into(), crate::sidebar::PROMPT.into()), (String::new(), Ok(rows)));
            let v1 = "You are orch, the lead of a small team. Read the threads you are given, answer each, and keep your answers short.";
            s.folders.texts.insert((0, "text:s1".into()), Ok(v1.into()));
            s.folders.texts.insert((0, "text:c1".into()), Ok("You are Claude Code, Anthropic's official CLI for Claude. (made up)".into()));
            s.projects = vec![p];
            s.cur = 0;
            s.sel = crate::Sel::Lens(crate::lens::Lens::of(crate::lens::Filter::SystemPrompt { agent: "orch".into() }));
            if open {
                s.prompt_edit = PromptEdit { of: Some((0, "orch".into())), text: TextEdit::new(format!("{v1}\nName the post each answer is for.")), after: Some(("/made-up/s1.jsonl".into(), 2600)), ..Default::default() };
            }
            s
        }
        let shots: [(&str, fn() -> App); 2] = [("prompt-edit-closed.png", || base(false)), ("prompt-edit-open.png", || base(true))];
        for (name, start) in shots {
            let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x1000".into(), "--at".into(), "1.0".into()];
            rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 1000.0), start, crate::view) }).args(args).try_run().unwrap();
        }
    }
}
