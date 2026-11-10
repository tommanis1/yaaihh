//! Questions to the owner and the owner's comments on selected text (vault designs/yaaihh-2026-11-07.md, amendments of
//! 7 Nov 20:3x).
//!
//! **Questions.** An agent's `coord ask` is a mail of kind `question` (`qN`) to the owner; answers and follow-ups are
//! mails `re` it, and coordd keeps the state of each thread on its first question (open, answered, dropped). A
//! question shows as a card ([`card`]) where it was asked in the agent's chat and in the project's Questions inbox
//! ([`inbox`]); the card's answer box mails the agent `re` the thread's latest mail.
//!
//! **Needing the owner** (owner 8 Nov): the inbox's Open list and the sidebar's count hold the open questions and the
//! unread threads: a thread whose latest mail is an agent's, after the owner has already written in it (a reply to
//! an answer, a follow-up the owner asked for), until the owner writes again or marks it read. The marks are kept in
//! `~/.config/yaaihh/read.json` (`{"PROJECT/QID": latest mail id read}`). A card's Dismiss drops the question
//! (coordd's `ask --drop`, as the owner).
//!
//! **Who a reply goes to** (owner 8 Nov): a reply box's To row holds the thread's participants (whoever wrote in it or
//! was sent mail in it, as coordd counts them) and the agents added for this reply; a click leaves one out of it or
//! takes it back, Add (a combobox of the project's agents) brings one in, and so does `@NAME` typed in the box: an `@`
//! opens the agents at the caret ([`mentions`]), arrows pick, Enter or Tab completes. On send each `@NAME` that names
//! an agent joins the recipients; one that names none stops the send. coordd gives an agent brought in the thread so
//! far with its first mail there.
//!
//! A thread the owner answers or marks read stays where it is (its card in the Open list, a chat's reply box open)
//! until the owner leaves the Questions page (owner 8 Nov: "should not kill me having it open", as in Slack).
//!
//! **Comments.** Text selected in a chat or a question card offers a Comment bar; each comment keeps the passage, where
//! it came from and the owner's note, and waits as a card like a queued mail's (owner m132) until the mail to that target (an agent, or a question's
//! answer) is sent. Sending makes one mail: each comment as the quoted passage, then the note, then the typed text.

use crate::{App, Proj};
use coord_types::{MailView, Quote, Request};
use rune::{El, ScrollSpec, TextEdit, col, row, text};
use rune_kit::markdown::{Highlight, Markdown};
use rune_kit::{Badge, BadgeVariant, Button, ButtonVariant, Combobox, ComboboxOption, ComboboxState, Kit, PopupItem, PopupList, SegmentedControl, TextArea};
use std::collections::HashMap;
use std::ops::Range;

/// A comment waiting to be sent: the passage, where it came from, and the owner's note.
#[derive(Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Comment {
    /// `AGENT@OFFSET` (an entry of the agent's session) or a mail or question id.
    pub source: String,
    /// Where it came from, as the agent is told (`your message at 14:02`).
    pub label: String,
    pub quote: String,
    pub note: String,
    /// The passage's place: byte ranges of the Markdown its source is drawn from, highlighted there.
    #[serde(default)]
    pub at: Vec<Range<usize>>,
}

/// The passage a comment is being written on (right-click → Comment), its box open at it.
#[derive(Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Picked {
    /// What the comment goes with: an agent's name (its next mail) or a question's id (its next answer).
    pub target: String,
    pub source: String,
    pub label: String,
    pub quote: String,
    #[serde(default)]
    pub at: Vec<Range<usize>>,
    /// The project of the pane it was made in (where the box goes when the passage is out of view: its composer).
    #[serde(default)]
    pub pi: usize,
}

#[derive(Default)]
pub struct Notes {
    pub picked: Option<Picked>,
    /// The comment whose box is open, by its highlight's key ([`waiting_key`], [`sent_key`]).
    pub open: Option<String>,
    /// The note being written for `picked`.
    pub writing: Option<TextEdit>,
    /// Comments waiting to be sent, per target.
    pub comments: HashMap<String, Vec<Comment>>,
    /// Answers being written, per question (the thread's first question).
    pub answers: HashMap<String, TextEdit>,
    /// The inbox shows every question, not only the open ones.
    pub show_all: bool,
    /// Mails in a chat whose thread's reply box is open.
    pub replying: std::collections::HashSet<String>,
    /// The threads marked read: "PROJECT/QID" → the id of the latest mail then.
    pub read: HashMap<String, String>,
    /// Per reply box (its thread's id): the participants left out of this reply, and the agents added to it.
    pub left_out: HashMap<String, std::collections::HashSet<String>>,
    pub added: HashMap<String, Vec<String>>,
    /// Per reply box: its Add combobox.
    pub adding: HashMap<String, ComboboxState>,
    /// The `@` being typed in a reply box, while its suggestions show.
    pub mention: Option<Mention>,
    /// The threads ("PROJECT/QID") the owner answered or marked read on this visit of Questions: they stay in the Open
    /// list, as a Slack thread stays open after a reply, until the owner leaves the page (main.rs `poll`).
    pub kept: std::collections::HashSet<String>,
    /// The decisions whose "why" is open ("PROJECT/DID"): None while coordd's answer is on its way.
    pub why: HashMap<String, Option<Result<coord_types::Decision, String>>>,
    /// The inbox row opened into its card (a thread's first question), or the search hit opened (a mail's id).
    pub expanded: Option<String>,
    /// The inbox's search over the project's questions and mail.
    pub search: TextEdit,
}

/// An `@NAME` being typed: in box FIELD, the `@` at byte AT, QUERY typed after it, ACTIVE the suggestion picked.
#[derive(Clone, Debug)]
pub struct Mention {
    pub field: Field,
    pub at: usize,
    pub query: String,
    pub active: usize,
}

/// A text box `@NAME` works in (owner m118): a thread's reply box, the comment box, an agent chat's composer. A tagged
/// agent is one more recipient of what the box sends.
#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    Answer(String),
    Comment,
    Composer(usize, String),
}

impl Field {
    /// The key of the element around the box (its caret anchors the suggestions).
    pub fn key(&self) -> String {
        match self {
            Field::Answer(id) => format!("answer-{id}"),
            Field::Comment => "cm-field".into(),
            Field::Composer(pi, a) => format!("cf-{pi}-{a}"),
        }
    }

    fn edit<'a>(&self, s: &'a mut App) -> Option<&'a mut TextEdit> {
        match self {
            Field::Answer(id) => s.notes.answers.get_mut(id),
            Field::Comment => s.notes.writing.as_mut(),
            Field::Composer(pi, a) => s.composers.get_mut(&(*pi, a.clone())),
        }
    }
}

/// The agents tagged `@NAME` in TEXTS, each once; Err names a tag that is no agent of P.
pub fn tagged(p: &Proj, texts: &[&str]) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for word in texts.iter().flat_map(|t| mentioned(t)) {
        match agent_named(p, word) {
            Some(n) if !out.iter().any(|x| x == n) => out.push(n.to_string()),
            Some(_) => {}
            None => return Err(format!("not sent: no agent named @{word}")),
        }
    }
    Ok(out)
}

fn read_path() -> std::path::PathBuf {
    std::path::Path::new(&crate::projects::config_path()).with_file_name("read.json")
}

impl Notes {
    pub fn load() -> Notes {
        let read = std::fs::read_to_string(read_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Notes { read, ..Default::default() }
    }

    fn mark_read(&mut self, project: &str, q: &str, latest: &str) {
        self.read.insert(format!("{project}/{q}"), latest.to_string());
        self.kept.insert(format!("{project}/{q}"));
        let _ = std::fs::write(read_path(), serde_json::to_string_pretty(&self.read).unwrap());
    }
}

// ---- threads ------------------------------------------------------------------------------------------------------

/// M asks the owner: a question (`coord ask`), or a mail coordd's recogniser found a question in.
/// A question a net filed and the owner took back (`unfiled`) is none.
pub fn is_question(m: &MailView) -> bool {
    (m.kind == "question" || m.recognised.is_some()) && !m.unfiled
}

/// Who a reply in thread ID goes to: TO (the box's own addressee), the thread's participants and the added agents,
/// less the ones left out; each with whether it is left out.
fn recipients(s: &App, p: &Proj, id: &str, to: &str) -> Vec<(String, bool)> {
    let mut names = p.live.mail.participants(id);
    for n in std::iter::once(to.to_string()).chain(s.notes.added.get(id).cloned().unwrap_or_default()) {
        if n != "owner" && !names.contains(&n) {
            names.push(n);
        }
    }
    let out = s.notes.left_out.get(id);
    names.into_iter().map(|n| {
        let left = out.is_some_and(|o| o.contains(&n));
        (n, left)
    }).collect()
}

/// The `@word`s of TEXT (an `@` at the start or after a space, then name characters).
fn mentioned(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for (i, _) in text.match_indices('@') {
        if i > 0 && !text[..i].ends_with(char::is_whitespace) {
            continue;
        }
        let rest = &text[i + 1..];
        let end = rest.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-')).unwrap_or(rest.len());
        if end > 0 {
            out.push(&rest[..end]);
        }
    }
    out
}

/// The agent named NAME in P (letter case aside).
fn agent_named<'a>(p: &'a Proj, name: &str) -> Option<&'a str> {
    p.live.agents.iter().find(|a| a.name.eq_ignore_ascii_case(name)).map(|a| a.name.as_str())
}

/// The agents an `@QUERY` offers: names starting with it first, then names containing it; running ones before stopped.
fn suggestions<'a>(p: &'a Proj, query: &str) -> Vec<&'a coord_types::AgentView> {
    let q = query.to_lowercase();
    let mut out: Vec<&coord_types::AgentView> = p.live.agents.iter().filter(|a| a.name.to_lowercase().contains(&q)).collect();
    out.sort_by_key(|a| (!a.name.to_lowercase().starts_with(&q), a.state == "stopped", a.name.clone()));
    out.truncate(8);
    out
}

/// After an edit in thread ID's box: the `@` before the caret being typed, if any.
pub(crate) fn follow_mention(s: &mut App, field: &Field) {
    let Some(e) = field.edit(s) else { return };
    let caret = e.focus.min(e.text.len());
    let before = &e.text[..caret];
    let typing = before.rfind('@').filter(|&at| at == 0 || before[..at].ends_with(char::is_whitespace)).and_then(|at| {
        let q = &before[at + 1..];
        q.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-').then(|| (at, q.to_string()))
    });
    match typing {
        Some((at, query)) => {
            let active = s.notes.mention.as_ref().filter(|m| m.field == *field && m.query == query).map_or(0, |m| m.active);
            s.notes.mention = Some(Mention { field: field.clone(), at, query, active });
        }
        None if s.notes.mention.as_ref().is_some_and(|m| m.field == *field) => s.notes.mention = None,
        None => {}
    }
}

/// Complete the `@` being typed to `@NAME ` (in a thread's reply box NAME joins its To row too).
fn complete(s: &mut App, name: &str) {
    let Some(m) = s.notes.mention.take() else { return };
    let Some(e) = m.field.edit(s) else { return };
    let caret = e.focus.min(e.text.len());
    let text = format!("{}@{name} {}", &e.text[..m.at], &e.text[caret..]);
    e.replace_text(text);
    e.set_caret(m.at + name.len() + 2);
    if let Field::Answer(id) = &m.field {
        add_to(s, id, name);
    }
}

fn add_to(s: &mut App, id: &str, name: &str) {
    let list = s.notes.added.entry(id.to_string()).or_default();
    if !list.iter().any(|n| n == name) {
        list.push(name.to_string());
    }
    if let Some(o) = s.notes.left_out.get_mut(id) {
        o.remove(name);
    }
}

/// The keys of thread ID's box while its suggestions show: arrows pick, Enter or Tab completes, Escape closes.
pub(crate) fn mention_key(s: &mut App, field: &Field, key: &rune::KeyEvent) -> bool {
    let Some(m) = s.notes.mention.clone().filter(|m| m.field == *field) else { return false };
    let Some(p) = s.proj() else { return false };
    let names: Vec<String> = suggestions(p, &m.query).iter().map(|a| a.name.clone()).collect();
    if names.is_empty() {
        return false;
    }
    match key.key {
        rune::Key::Up | rune::Key::Down => {
            let n = names.len();
            let active = if key.key == rune::Key::Up { (m.active + n - 1) % n } else { (m.active + 1) % n };
            s.notes.mention = Some(Mention { active, ..m });
            true
        }
        rune::Key::Enter | rune::Key::Tab => {
            complete(s, &names[m.active.min(names.len() - 1)]);
            true
        }
        rune::Key::Escape => {
            s.notes.mention = None;
            true
        }
        _ => false,
    }
}

/// The `@` suggestions at the caret of the reply box they are for (CX gives the caret; LAYER is the window's).
pub fn mentions(s: &App, cx: &mut rune::Cx<App>, k: &Kit, layer: &mut Vec<El>) {
    let Some(m) = &s.notes.mention else { return };
    let Some(p) = s.proj() else { return };
    let Some(r) = cx.caret(&m.field.key()) else { return };
    let found = suggestions(p, &m.query);
    if found.is_empty() {
        return;
    }
    layer.push(rune::el().key("mention-caret").pin(Some(r[0]), Some(r[1]), None, None).w(1.0).h(r[3]));
    let items = found.iter().map(|a| PopupItem::new(a.name.clone(), a.name.clone()).description(format!("{} · {}", a.id, a.state.lines().next().unwrap_or(""))));
    PopupList::new("mentions", "mention-caret")
        .items(items)
        .active(Some(m.active.min(found.len() - 1)))
        .label("Agents")
        .on_active(|s: &mut App, i| {
            if let Some(m) = &mut s.notes.mention {
                m.active = i;
            }
        })
        .on_choose(|s: &mut App, name, _| complete(s, name))
        .build(k, layer);
}

/// The threads' first questions, newest first; open ones only unless ALL.
pub fn questions(p: &Proj, all: bool) -> Vec<&MailView> {
    p.live.mail.iter().rev().filter(|m| is_question(m) && m.re.is_none() && (all || m.state.as_deref() == Some("open"))).collect()
}

/// Whether question Q's thread is unread: its latest mail is an agent's, after a word of the owner's, and not marked read.
pub fn unread(s: &App, p: &Proj, q: &MailView) -> bool {
    if q.state.as_deref() == Some("dropped") {
        return false;
    }
    let thread = p.live.mail.replies(&q.id);
    let Some(last) = thread.last() else { return false };
    last.from != "owner" && thread.iter().any(|m| m.from == "owner") && s.notes.read.get(&format!("{}/{}", p.id, q.id)) != Some(&last.id)
}

/// The questions that need the owner, newest first: open or unread.
pub fn needing<'a>(s: &App, p: &'a Proj) -> Vec<&'a MailView> {
    questions(p, true).into_iter().filter(|q| q.state.as_deref() == Some("open") || unread(s, p, q)).collect()
}

/// The sidebar's count: the threads where it is the owner's move (not the quiet ones).
pub fn open_count(s: &App, p: &Proj) -> usize {
    questions(p, true).into_iter().filter(|q| standing(s, p, q) == Standing::Yours).count()
}

/// After this long with no mail, a thread waiting on someone is quiet (owner m219, 9 Nov).
const QUIET_HOURS: i64 = 24;

/// Where a question thread stands for the owner (owner m219, 9 Nov: "my active conversations, where I still need to say
/// or chase something"), from facts only: its state, who wrote last, and when.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Standing {
    /// Open or unread, with mail in it in the last [`QUIET_HOURS`]: the owner's move.
    Yours,
    /// The owner wrote last (an answer, a question back), in the last [`QUIET_HOURS`]: the agent's move.
    Theirs,
    /// Open or unread, but nobody wrote for [`QUIET_HOURS`]: likely overtaken; drop it or answer it.
    Quiet,
    /// Answered and gone quiet, or dropped.
    Settled,
}

fn hours_since(at: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(at).map(|t| (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_hours()).unwrap_or(0)
}

/// The latest mail of question Q's thread (Q itself when nothing answers it).
fn latest<'a>(p: &'a Proj, q: &'a MailView) -> &'a MailView {
    p.live.mail.replies(&q.id).last().copied().unwrap_or(q)
}

pub fn standing(s: &App, p: &Proj, q: &MailView) -> Standing {
    if q.state.as_deref() == Some("dropped") {
        return Standing::Settled;
    }
    let last = latest(p, q);
    let fresh = hours_since(&last.at) < QUIET_HOURS;
    match (q.state.as_deref() == Some("open") || unread(s, p, q), fresh) {
        (true, true) => Standing::Yours,
        (true, false) => Standing::Quiet,
        (false, true) if last.from == "owner" => Standing::Theirs,
        _ => Standing::Settled,
    }
}

/// What question Q's thread asks now: its latest question (a follow-up replaces the first wording), as its subject line.
pub fn current_ask<'a>(p: &'a Proj, q: &'a MailView) -> &'a MailView {
    p.live.mail.replies(&q.id).into_iter().rev().find(|m| is_question(m)).unwrap_or(q)
}

fn subject_of(m: &MailView) -> String {
    match &m.recognised {
        Some(r) => r.question.clone(),
        None => m.body.lines().find(|l| !l.trim().is_empty()).unwrap_or_default().replace("**", "").trim().to_string(),
    }
}

/// How long ago AT was, shortly (`just now`, `5 min ago`, `3 h ago`, `2 d ago`).
pub fn ago(at: &str) -> String {
    let Ok(t) = chrono::DateTime::parse_from_rfc3339(at) else { return String::new() };
    let m = (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_minutes().max(0);
    match m {
        0 => "just now".into(),
        1..=59 => format!("{m} min ago"),
        60..=2879 => format!("{} h ago", m / 60),
        _ => format!("{} d ago", m / 1440),
    }
}


// ---- comments -----------------------------------------------------------------------------------------------------
//
// A comment is made on selected text with the selection menu's Comment (right-click; Rune's
// `El::selection_menu_item`). Its box opens at the passage, which is highlighted where it stands (the selection's
// Markdown source ranges, `SelectedText::source`); added, it waits as a chip above the composer and stays highlighted,
// and once sent, the quote in the mail keeps it highlighted. A click on a highlight opens its comment there
// (`El::on_highlight_press`). Owner 8 Nov (m30, m33).

/// The highlight key of TARGET's waiting comment I.
pub fn waiting_key(target: &str, i: usize) -> String {
    format!("cm-{target}-{i}")
}

/// The highlight key of quote J of mail ID (a comment sent).
pub fn sent_key(id: &str, j: usize) -> String {
    format!("cq-{id}-{j}")
}

/// The highlight key of the passage a comment is being written on.
const NEW: &str = "cm-new";

/// The selection menu's Comment on SEL, made in pane project PI for TARGET (an agent's chat, or a question's or
/// mail's thread): the passage, where it came from and its place, with the note box open at it.
pub fn comment_on(s: &mut App, pi: usize, target: &str, sel: &rune::TextSelection) {
    let quote = sel.text().trim().to_string();
    if quote.is_empty() {
        return;
    }
    if s.notes.writing.as_ref().is_some_and(|w| !w.text.trim().is_empty()) {
        s.notice = Some("a comment is being written: add or cancel it first".into());
        return;
    }
    // the entry or mail the selection starts in, by its element key
    let first = sel.texts().iter().find(|t| t.is_selected());
    let keys: Vec<&str> = first.map(|t| t.path().split('/').collect()).unwrap_or_default();
    let (key, source, label) = match keys.iter().rev().find_map(|k| k.strip_prefix("qmsg-").map(|id| (*k, id))) {
        Some((k, id)) => (Some(k.to_string()), id.to_string(), id.to_string()),
        None => match keys.iter().find(|k| entry_key(k)) {
            Some(k) => {
                let (src, label) = entry_source(s, pi, target, k);
                (Some(k.to_string()), src, label)
            }
            None => (None, target.to_string(), String::new()),
        },
    };
    // its place: the source ranges of what is selected inside that entry or mail
    let at: Vec<Range<usize>> = match &key {
        Some(k) => sel.texts().iter().filter(|t| t.path().split('/').any(|x| x == k)).flat_map(|t| t.source().iter().cloned()).collect(),
        None => Vec::new(),
    };
    s.focus_on(pi, target);
    s.notes.open = None;
    s.notes.picked = Some(Picked { target: target.into(), source, label, quote, at, pi });
    let mut w = TextEdit::new("");
    w.focused = true;
    s.notes.writing = Some(w);
}

/// A chat entry's key: `e{offset}-{n}` (live.rs).
fn entry_key(k: &str) -> bool {
    k.strip_prefix('e').and_then(|r| r.split_once('-')).is_some_and(|(a, b)| a.parse::<u64>().is_ok() && b.parse::<u32>().is_ok())
}

/// Where entry KEY of AGENT's chat in project PI came from: its source and how the agent is told of it.
fn entry_source(s: &App, pi: usize, agent: &str, key: &str) -> (String, String) {
    let offset = key.trim_start_matches('e').split('-').next().unwrap_or("0");
    let entry = s.projects.get(pi).and_then(|p| p.convs.get(agent)).and_then(|c| c.keys.iter().position(|x| x == key).map(|i| &c.entries[i]));
    let label = match entry {
        Some(e) => match e.kind.as_str() {
            "assistant" => format!("your message at {}", crate::domain::local(&e.at)),
            "tool" => format!("your tool call at {}", crate::domain::local(&e.at)),
            "result" => format!("a tool result at {}", crate::domain::local(&e.at)),
            _ => format!("the message at {}", crate::domain::local(&e.at)),
        },
        None => String::new(),
    };
    (format!("{agent}@{offset}"), label)
}

/// The mail body for TEXT with TARGET's comments first, and the quotes' sources and places.
pub fn compose(comments: &[Comment], typed: &str) -> (String, Vec<Quote>) {
    let mut parts: Vec<String> = comments
        .iter()
        .map(|c| {
            let mut q: String = c.quote.lines().map(|l| format!("> {l}\n")).collect();
            if !c.label.is_empty() {
                q.push_str(&format!("> (from {})\n", c.label));
            }
            format!("{q}\n{}", c.note.trim())
        })
        .collect();
    if !typed.trim().is_empty() {
        parts.push(typed.trim().to_string());
    }
    let quotes = comments.iter().map(|c| Quote { source: c.source.clone(), text: c.quote.clone(), at: c.at.iter().map(|r| [r.start, r.end]).collect() }).collect();
    (parts.join("\n\n"), quotes)
}

/// The highlights of the comments on SOURCE (an entry `AGENT@OFFSET`, or a mail or question id), whose Markdown is
/// SRC: the passage being commented on, the comments waiting and those sent (by their place, or else by finding
/// their text).
pub fn marks(s: &App, p: &Proj, source: &str, src: &str, k: &Kit) -> Vec<(Vec<Range<usize>>, Highlight)> {
    let mut out = Vec::new();
    let fits = |at: &[Range<usize>]| !at.is_empty() && at.iter().all(|r| r.end <= src.len());
    let found = |quote: &str| {
        let first = quote.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
        rune_kit::markdown::find(src, first, rune_kit::markdown::Case::Sensitive).into_iter().next()
    };
    let open = |key: &str| s.notes.open.as_deref() == Some(key);
    let accent = k.c.bg_accent.alpha(0.28);
    for (target, list) in &s.notes.comments {
        for (i, c) in list.iter().enumerate().filter(|(_, c)| c.source == source) {
            let key = waiting_key(target, i);
            let at = if fits(&c.at) { Some(c.at.clone()) } else { found(&c.quote) };
            let h = Highlight::new().bg(accent).current_match(open(&key)).key(key);
            out.extend(at.map(|at| (at, h)));
        }
    }
    for (m, j, q) in p.live.mail.quotes(source) {
        let key = sent_key(&m.id, j);
        let place: Vec<Range<usize>> = q.at.iter().map(|r| r[0]..r[1]).collect();
        let at = if fits(&place) { Some(place) } else { found(&q.text) };
        let h = Highlight::new().bg(k.c.text_accent.alpha(0.14)).current_match(open(&key)).key(key);
        out.extend(at.map(|at| (at, h)));
    }
    if let Some(pk) = s.notes.picked.as_ref().filter(|pk| pk.source == source && s.notes.writing.is_some() && fits(&pk.at)) {
        out.push((pk.at.clone(), Highlight::current().bg(accent).key(NEW)));
    }
    out
}

/// Everything [`marks`] reads for SOURCE, into H: a memoised row that shows SOURCE is built again when it changes.
pub fn marks_version(s: &App, p: &Proj, source: &str, h: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    let mut any = false;
    for (target, list) in &s.notes.comments {
        for (i, c) in list.iter().enumerate().filter(|(_, c)| c.source == source) {
            (target, i, &c.at, &c.quote).hash(h);
            any = true;
        }
    }
    for (m, j, q) in p.live.mail.quotes(source) {
        (&m.id, j, &q.at, &q.text).hash(h);
        any = true;
    }
    if let Some(pk) = s.notes.picked.as_ref().filter(|pk| pk.source == source && s.notes.writing.is_some()) {
        pk.at.hash(h);
        any = true;
    }
    if any {
        s.notes.open.hash(h);
    }
}

/// What a part showing threads reads (a question's card, a mail in a chat with its replies and reply box), into V:
/// the project's mail and agents (whom a reply can go to), the notes, and the minute ("asked 5 min ago"). Of the
/// answer boxes only OWN's when the part has only that one, so typing in one card leaves the others as they are.
pub fn thread_version(s: &App, p: &Proj, own: Option<&str>, v: &mut crate::version::Version) {
    let n = &s.notes;
    v.add((&p.id, s.cur, p.live.mail.rev, chrono::Utc::now().timestamp() / 60));
    match own {
        Some(id) => v.add(n.answers.get(id)),
        None => v.debug(&n.answers),
    };
    v.debug(&n.picked).add((&n.open, &n.writing, n.show_all)).debug(&n.comments).debug(&n.replying).debug(&n.read);
    v.debug(&n.left_out).debug(&n.added).debug(&n.adding).debug(&n.mention).debug(&n.kept).debug(&n.why);
    for a in &p.live.agents {
        v.add(&a.name);
    }
}

/// A click on highlight KEY: a comment's opens (or closes) its box.
pub fn highlight_pressed(s: &mut App, key: &str) {
    if !(key.starts_with("cm-") || key.starts_with("cq-")) || key == NEW {
        return;
    }
    s.notes.open = if s.notes.open.as_deref() == Some(key) { None } else { Some(key.to_string()) };
}

/// The comments waiting for TARGET (above its composer or answer box), each as a card like a queued mail's (owner
/// m132): whom it goes to (the chat's agent, or the thread's reply, and each agent tagged @NAME in it), that it is
/// queued to be sent with the next message, the passage and the note; Edit opens it in its box again, Cancel drops
/// it, and in an agent's chat Send now sends it (with whatever is typed below). A click shows the passage and opens
/// the comment there. Once sent it is part of that mail, shown queued (and the clerk's state of it) until delivered.
pub fn bar(s: &App, pi: usize, target: &str, k: &Kit) -> El {
    let waiting = s.notes.comments.get(target).map(Vec::as_slice).unwrap_or_default();
    let Some(p) = s.projects.get(pi).filter(|_| !waiting.is_empty()) else { return col() };
    let agent = p.live.agents.iter().any(|a| a.name == target);
    let cards = waiting.iter().enumerate().map(|(i, cm)| {
        let (t, key) = (target.to_string(), waiting_key(target, i));
        let mut to: Vec<String> = vec![if agent { target.to_string() } else { format!("the reply in {target}") }];
        let tags = tagged(p, &[cm.note.as_str()]);
        to.extend(tags.as_ref().map(|v| v.iter().filter(|n| *n != target).cloned().collect::<Vec<_>>()).unwrap_or_default());
        let state = match &tags {
            Err(e) => e.replace("not sent: ", "will not send: "),
            Ok(_) => "queued to be sent with your next message".into(),
        };
        let body = k
            .interactive(
                col()
                    .grow(1.0)
                    .shrink()
                    .gap(4.0)
                    .child(text(format!("comment to {} · {state}", to.join(", "))).font(k.ty.small).color(k.c.text_faint).wrap())
                    .child(text(format!("“{}”", one_line(&cm.quote, 160))).font(k.ty.small).color(k.c.text_faint).wrap().max_lines(2))
                    .child(text(cm.note.trim().to_string()).font(k.ty.body).color(k.c.text_muted).wrap().max_lines(4)),
            )
            .help("Click to show the passage it is on")
            .on_press_tx(move |s: &mut App, tx: &mut rune::Tx| {
                s.notes.open = Some(key.clone());
                tx.scroll_into_view(key.clone());
            });
        let (t1, t2) = (t.clone(), t.clone());
        let mut acts = row()
            .gap(4.0)
            .child(Button::new("Edit").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| edit_waiting(s, &t1, i)).build(k))
            .child(Button::new("Cancel").variant(ButtonVariant::GhostMuted).on_press(move |s: &mut App| drop_waiting(s, &t2, i)).build(k));
        if agent {
            acts = acts.child(Button::new("Send now").variant(ButtonVariant::Warning).on_press(move |s: &mut App| s.send(pi, &t, true, false)).build(k));
        }
        row()
            .key(format!("comment-card-{target}-{i}"))
            .w_full()
            .gap(8.0)
            .padding(10.0)
            .radius(k.radius.md)
            .border(1.0, k.c.border_weak)
            .fill(k.c.layer_01)
            .opacity(0.6)
            .child(body)
            .child(acts)
    });
    col().w_full().gap(6.0).children(cards.collect::<Vec<_>>())
}

/// Waiting comment I of TARGET back in its box to edit (it waits again when added).
fn edit_waiting(s: &mut App, t: &str, i: usize) {
    let Some(cm) = s.notes.comments.get_mut(t).filter(|v| i < v.len()).map(|v| v.remove(i)) else { return };
    let mut w = TextEdit::new(cm.note.clone());
    w.focused = true;
    s.notes.picked = Some(Picked { target: t.to_string(), source: cm.source, label: cm.label, quote: cm.quote, at: cm.at, pi: s.cur });
    s.notes.writing = Some(w);
    s.notes.open = None;
}

fn drop_waiting(s: &mut App, target: &str, i: usize) {
    if let Some(v) = s.notes.comments.get_mut(target) {
        if i < v.len() {
            v.remove(i);
        }
    }
    s.notes.open = None;
}

/// The note written becomes a waiting comment of the picked passage's target.
fn add(s: &mut App) {
    let note = match (&s.notes.picked, &s.notes.writing) {
        (Some(_), Some(w)) if !w.text.trim().is_empty() => w.text.trim().to_string(),
        _ => return,
    };
    s.notes.writing = None;
    let p = s.notes.picked.take().expect("checked above");
    s.notes.comments.entry(p.target).or_default().push(Comment { source: p.source, label: p.label, quote: p.quote, note, at: p.at });
}

/// A comment shown in its box: whose, the passage, the note, and whether it waits (I in TARGET's list) or was sent.
struct Shown {
    quote: String,
    note: String,
    waiting: Option<(String, usize)>,
    sent: Option<String>,
}

/// The comment highlight KEY stands for.
fn shown(s: &App, key: &str) -> Option<Shown> {
    for (target, list) in &s.notes.comments {
        for (i, c) in list.iter().enumerate() {
            if waiting_key(target, i) == key {
                return Some(Shown { quote: c.quote.clone(), note: c.note.clone(), waiting: Some((target.clone(), i)), sent: None });
            }
        }
    }
    for p in &s.projects {
        for m in p.live.mail.iter().filter(|m| m.from == "owner") {
            for (j, q) in m.quotes.iter().enumerate() {
                if sent_key(&m.id, j) == key {
                    // the note: the mail's part after this quote's lines, up to the next quote
                    let note = m.body.split("\n\n").skip_while(|part| !part.starts_with('>') || !part.contains(q.text.lines().next().unwrap_or_default())).nth(1).unwrap_or_default();
                    return Some(Shown { quote: q.text.clone(), note: note.to_string(), waiting: None, sent: Some(format!("sent in {} at {}", m.id, crate::domain::local(&m.at))) });
                }
            }
        }
    }
    None
}

/// The comment boxes, in the window's LAYER: the note being written, at its passage (else at its pane's composer), and
/// the comment opened from its highlight, at it.
pub fn layer(s: &App, cx: &mut rune::Cx<App>, k: &Kit, layer: &mut Vec<El>) {
    let at = |key: &str| cx.window_layout(key);
    let mut place = |rect: [f32; 4], body: El, layer: &mut Vec<El>| {
        layer.push(rune::el().key("cm-anchor").pin(Some(rect[0]), Some(rect[1]), None, None).w(rect[2].max(1.0)).h(rect[3].max(1.0)));
        layer.push(k.elevate(
            body.key("cm-box")
                .w(380.0)
                .gap(8.0)
                .padding(12.0)
                .radius(k.radius.md)
                .fill(k.c.surface_raised)
                .border(1.0, k.c.border_focus)
                .anchored("cm-anchor", rune::Side::Bottom, rune::Align::Start, 6.0),
            &k.elevation.floating,
        ));
    };
    if let (Some(p), Some(w)) = (&s.notes.picked, &s.notes.writing) {
        let rect = at(NEW).or_else(|| at(&format!("composer-{}-{}", p.pi, p.target))).or_else(|| at(&format!("answer-{}", p.target))).or_else(|| at("main"));
        if let Some(r) = rect {
            let body = col()
                .child(text(format!("> {}", one_line(&p.quote, 140))).font(k.ty.small).color(k.c.text_muted).truncate())
                .child(
                    col().key(Field::Comment.key()).w_full().child(
                        TextArea::new(w)
                            .width(None)
                            .placeholder("Your comment on it (Cmd/Ctrl+Enter adds it; @ tags an agent)")
                            .auto_grow(2, 8)
                            .on_edit(|s: &mut App| s.notes.writing.get_or_insert_with(|| TextEdit::new("")))
                            .on_change(|s: &mut App, _| follow_mention(s, &Field::Comment))
                            .on_key(|s: &mut App, key, _| mention_key(s, &Field::Comment, key))
                            .on_submit(|s: &mut App, _| add(s))
                            .build(k),
                    ),
                )
                .child(
                    row()
                        .gap(8.0)
                        .child(Button::new("Add comment").variant(ButtonVariant::Contrast).on_press(|s: &mut App| add(s)).build(k))
                        .child(Button::new("Cancel").variant(ButtonVariant::Ghost).on_press(|s: &mut App| {
                            s.notes.writing = None;
                            s.notes.picked = None;
                        }).build(k)),
                );
            place(r, body, layer);
            return;
        }
    }
    let Some(key) = s.notes.open.as_deref() else { return };
    let (Some(c), Some(r)) = (shown(s, key), at(key)) else { return };
    let mut acts = row().gap(8.0);
    let head = c.sent.clone().unwrap_or_else(|| "queued to be sent with your next message".into());
    if let Some((t, i)) = c.waiting.clone() {
        let (t2, i2) = (t.clone(), i);
        acts = acts
            .child(Button::new("Edit").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| edit_waiting(s, &t, i)).build(k))
            .child(Button::new("Remove").variant(ButtonVariant::GhostMuted).on_press(move |s: &mut App| drop_waiting(s, &t2, i2)).build(k));
    }
    acts = acts.child(Button::new("Close").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.notes.open = None).build(k));
    let body = col()
        .child(text(head).font(k.ty.small).color(k.c.text_faint))
        .child(text(format!("> {}", one_line(&c.quote, 200))).font(k.ty.small).color(k.c.text_muted).wrap().max_lines(3))
        .child(text(c.note).font(k.ty.body).color(k.c.text_base).wrap())
        .child(acts);
    place(r, body, layer);
}

fn one_line(t: &str, n: usize) -> String {
    let flat = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > n { format!("{}…", flat.chars().take(n).collect::<String>()) } else { flat }
}

// ---- questions ----------------------------------------------------------------------------------------------------

/// A question's card: who asked and when, its state, the explanation, its thread, and (unless dropped) the answer box
/// with its comments.
pub fn card(s: &App, p: &Proj, q: &MailView, k: &Kit) -> El {
    // built again only when what it reads changes: the mail, the notes (of the answer boxes, only its own), the
    // project's agents (whom a reply can go to) and, for "asked 5 min ago", the minute; typing in another card's
    // answer box reuses it
    let mut v = crate::version::Version::default();
    v.add(&q.id);
    thread_version(s, p, Some(&q.id), &mut v);
    rune::memo::memo_with_hash(format!("card-{}", q.id), v.finish(), || card_built(s, p, q, k))
}

fn card_built(s: &App, p: &Proj, q: &MailView, k: &Kit) -> El {
    let state = q.state.as_deref().unwrap_or("open");
    let mut head = row()
        .w_full()
        .gap(8.0)
        .center_y()
        .child(text(format!("{} asks", q.from)).font(k.ty.small_medium).color(k.c.text_accent))
        .child(text(format!("{} · {} · asked {}", q.id, crate::domain::local(&q.at), ago(&q.at))).font(k.ty.small).color(k.c.text_faint));
    head = head.child(match state {
        "open" => Badge::new("open").variant(BadgeVariant::Accent).build(k),
        other => Badge::new(other.to_string()).build(k),
    });
    let new = unread(s, p, q);
    if new {
        head = head.child(Badge::new("unread").variant(BadgeVariant::Accent).build(k));
    }
    // mark read (an unread thread) and dismiss (drop the question), at the head's end
    let mut acts = row().gap(4.0).grow(1.0).justify_end();
    if new {
        let (project, id, latest) = (p.id.clone(), q.id.clone(), p.live.mail.replies(&q.id).last().map(|m| m.id.clone()).unwrap_or_default());
        acts = acts.child(Button::new("Mark read").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| s.notes.mark_read(&project, &id, &latest)).build(k));
    }
    if state != "dropped" {
        let (project, id) = (p.id.clone(), q.id.clone());
        acts = acts.child(
            Button::new("Dismiss")
                .variant(ButtonVariant::GhostMuted)
                .on_press(move |s: &mut App| {
                    if let Some(i) = s.projects.iter().position(|x| x.id == project) {
                        s.mail_in(i, Request::Drop { id: id.clone(), why: Some("dismissed by the owner".into()) });
                    }
                })
                .build(k),
        );
    }
    head = head.child(acts);
    // a recognised question is titled by the question found, the whole mail under it
    let (subject, rest) = match &q.recognised {
        Some(r) => (r.question.as_str(), q.body.as_str()),
        None => q.body.split_once('\n').unwrap_or((q.body.as_str(), "")),
    };
    let mut c = col()
        .key(format!("qcard-{}", q.id))
        .w_full()
        .gap(8.0)
        .padding(14.0)
        .radius(k.radius.md)
        .fill(k.c.layer_01)
        .border(1.0, if state == "open" || new { k.c.border_focus } else { k.c.border_weak })
        .child(head)
        .child(text(subject.trim().to_string()).font(k.ty.ui_medium).color(k.c.text_base).wrap());
    if let Some(r) = &q.recognised {
        c = c.child(recognised_row(s, p, r, k));
    }
    if let Some(f) = crate::clerk::filed_row(p, q, k) {
        c = c.child(f);
    }
    if !rest.trim().is_empty() {
        let src = rest.trim().to_string();
        let marks = marks(s, p, &q.id, &src, k);
        c = c.child(col().key(format!("qmsg-{}", q.id)).w_full().child(Markdown::new(src).highlights(marks).selectable(true).build(k)));
    }
    for m in p.live.mail.replies(&q.id) {
        c = c.child(thread_row(s, p, m, k));
        if let Some(f) = crate::clerk::filed_row(p, m, k) {
            c = c.child(f);
        }
    }
    if state != "dropped" {
        let label = if state == "open" { "Answer" } else { "Send" };
        let hint = if state == "open" { "Your answer (Cmd/Ctrl+Enter sends)" } else { "Follow up (Cmd/Ctrl+Enter sends)" };
        c = c.child(reply_box(s, &q.id, &q.from, hint, label, k));
    }
    let target = q.id.clone();
    c.selection_group(true)
        .selection_menu_item("comment", "Comment", move |s: &mut App, sel, _| {
            let pi = s.cur;
            comment_on(s, pi, &target, sel)
        })
        .on_highlight_press(|s: &mut App, key, _| highlight_pressed(s, key))
}

/// Where a recognised question came from: the model that found it, the passage, and the decision's why (toggled).
fn recognised_row(s: &App, p: &Proj, r: &coord_types::Recognised, k: &Kit) -> El {
    let key = format!("{}/{}", p.id, r.decision);
    let (project, id, open) = (p.id.clone(), r.decision.clone(), s.notes.why.contains_key(&key));
    let head = row()
        .w_full()
        .gap(8.0)
        .center_y()
        .child(text(format!("found in a mail by {} · “{}”", r.model, one_line(&r.quote, 80))).font(k.ty.small).color(k.c.text_faint).grow(1.0).shrink())
        .child(
            Button::new(if open { "Hide why" } else { "Why this model" })
                .variant(ButtonVariant::Ghost)
                .on_press(move |s: &mut App| toggle_why(s, &project, &id))
                .build(k),
        );
    let mut c = col().w_full().gap(4.0).child(head);
    match s.notes.why.get(&key) {
        Some(Some(Ok(d))) => c = c.child(why(d, k)),
        Some(Some(Err(e))) => c = c.child(text(e.clone()).font(k.ty.small).color(k.c.text_muted).wrap()),
        Some(None) => c = c.child(text("…").font(k.ty.small).color(k.c.text_faint)),
        None => {}
    }
    c
}

/// Open or close decision ID's why: when opening, coordd is asked for the decision (`Request::Decision`).
pub(crate) fn toggle_why(s: &mut App, project: &str, id: &str) {
    let key = format!("{project}/{id}");
    if s.notes.why.remove(&key).is_some() {
        return;
    }
    let Some(i) = s.projects.iter().position(|x| x.id == project) else { return };
    if s.request_in(i, Request::Decision { id: id.into() }, crate::Pending::Decision { key: key.clone() }) {
        s.notes.why.insert(key, None);
    }
}

/// A decision's why (vault designs/policy-language-draft-2026-11-08.md §8): what was chosen and the rule that decided
/// it over the runner-up, the models tried before it, and why the others were out.
pub(crate) fn why(d: &coord_types::Decision, k: &Kit) -> El {
    let line = |t: String| text(t).font(k.ty.small).color(k.c.text_muted).wrap();
    let mut c = col().w_full().gap(3.0).padding(10.0).radius(k.radius.sm).fill(k.c.layer_02);
    c = c.child(text(format!("{} · {}: {}", d.id, d.decision, d.chosen.as_deref().unwrap_or("none"))).font(k.ty.small_medium).color(k.c.text_base));
    let call: Vec<String> = d.call.iter().map(|(f, v)| format!("{f} = {v}")).collect();
    c = c.child(line(format!("call: {}", call.join(", "))));
    for g in &d.given {
        c = c.child(line(format!("given: {} = {} (by {} at {})", g.field, g.value, g.by, g.at)));
    }
    if let (Some((other, _)), Some(by)) = (&d.runner_up, &d.decided_by) {
        c = c.child(line(format!("over {other}, decided by {} (level {}, by {})", by.rule, by.level, by.cost)));
    } else if d.optimal.len() > 1 {
        c = c.child(line(format!("a tie of {}: the first by name taken (no judge yet)", d.optimal.join(", "))));
    }
    for a in &d.attempts {
        c = c.child(line(format!("tried {}: {}", a.model, a.failure.as_deref().unwrap_or("answered"))));
    }
    if let Some(f) = &d.fault {
        c = c.child(line(format!("fault: {f}")));
    }
    // the others out, one line per option with its reasons
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for e in &d.excluded {
        match out.iter_mut().find(|(o, _)| *o == e.option) {
            Some((_, rs)) => rs.push(e.rule.clone()),
            None => out.push((e.option.clone(), vec![e.rule.clone()])),
        }
    }
    for (o, rs) in out {
        c = c.child(line(format!("out: {o}: {}", rs.join("; "))));
    }
    c
}

/// One mail of a thread, under what it answers: who, its id and time, its body.
fn thread_row(s: &App, p: &Proj, m: &MailView, k: &Kit) -> El {
    let who = if m.from == "owner" { "you".to_string() } else { m.from.clone() };
    let mark = rune::el().w(2.0).h_full().radius(1.0).fill(if m.from == "owner" { k.c.text_accent } else { k.c.border_weak });
    row().w_full().gap(10.0).child(mark).child(
        col()
            .key(format!("qmsg-{}", m.id))
            .grow(1.0)
            .shrink()
            .gap(4.0)
            .child(text(format!("{who} · {} · {}", m.id, crate::domain::local(&m.at))).font(k.ty.small_medium).color(k.c.text_muted))
            .child(Markdown::new(m.body.clone()).highlights(marks(s, p, &m.id, &m.body, k)).selectable(true).build(k)),
    )
}

/// The comments, the To row, the text box and the send button of a reply in thread ID (TO: whom it answers).
fn reply_box(s: &App, id: &str, to: &str, hint: &str, label: &str, k: &Kit) -> El {
    let edit = s.notes.answers.get(id).cloned().unwrap_or_else(|| TextEdit::new(""));
    let field = TextArea::new(&edit)
        .width(None)
        .placeholder(format!("{hint}; @ adds an agent"))
        .auto_grow(2, 10)
        .on_edit({
            let id = id.to_string();
            move |s: &mut App| s.notes.answers.entry(id.clone()).or_insert_with(|| TextEdit::new(""))
        })
        .on_change({
            let f = Field::Answer(id.to_string());
            move |s: &mut App, _| follow_mention(s, &f)
        })
        .on_key({
            let f = Field::Answer(id.to_string());
            move |s: &mut App, key, _| mention_key(s, &f, key)
        })
        .on_submit({
            let (id, to) = (id.to_string(), to.to_string());
            move |s: &mut App, _| reply(s, &id, &to)
        })
        .build(k);
    let send = Button::new(label.to_string()).variant(ButtonVariant::Contrast).on_press({
        let (id, to) = (id.to_string(), to.to_string());
        move |s: &mut App| reply(s, &id, &to)
    });
    col()
        .w_full()
        .gap(8.0)
        .child(bar(s, s.cur, id, k))
        .children(s.proj().map(|p| to_row(s, p, id, to, k)))
        .child(col().key(format!("answer-{id}")).w_full().child(field))
        .child(row().w_full().justify_end().child(send.build(k)))
}

/// The To row of thread ID's reply: a chip per recipient (a click leaves it out or takes it back) and Add.
fn to_row(s: &App, p: &Proj, id: &str, to: &str, k: &Kit) -> El {
    let mut r = row().w_full().gap(6.0).center_y().wrap().child(text("To").font(k.ty.small_medium).color(k.c.text_muted));
    let names = recipients(s, p, id, to);
    for (name, left) in &names {
        let (thread, n) = (id.to_string(), name.clone());
        let chip = Button::new(if *left { format!("{name} (left out)") } else { name.clone() })
            .variant(if *left { ButtonVariant::Ghost } else { ButtonVariant::Neutral })
            .on_press(move |s: &mut App| {
                let o = s.notes.left_out.entry(thread.clone()).or_default();
                if !o.remove(&n) {
                    o.insert(n.clone());
                }
            })
            .build(k);
        r = r.child(chip);
    }
    let options: Vec<ComboboxOption> = p
        .live
        .agents
        .iter()
        .filter(|a| !names.iter().any(|(n, _)| *n == a.name))
        .map(|a| ComboboxOption { detail: format!("{} · {}", a.id, a.state), ..ComboboxOption::new(a.name.clone(), a.name.clone()) })
        .collect();
    let state = s.notes.adding.get(id).cloned().unwrap_or_default();
    let (thread, chosen) = (id.to_string(), id.to_string());
    let mut layer = Vec::new();
    let add = Combobox::new(format!("add-{id}"), &state, &options, move |s: &mut App| s.notes.adding.entry(thread.clone()).or_default())
        .placeholder("Add…")
        .search_placeholder("Agent")
        .width(240.0)
        .on_choose(move |s: &mut App, name, _| add_to(s, &chosen, name))
        .build(k, &mut layer);
    r.child(add).children(layer)
}

/// Mail TO the reply written in thread ID (with its comments), `re` the thread's latest mail.
pub fn reply(s: &mut App, id: &str, to: &str) {
    let typed = s.notes.answers.get(id).map(|e| e.text.clone()).unwrap_or_default();
    let comments = s.notes.comments.get(id).cloned().unwrap_or_default();
    if typed.trim().is_empty() && comments.is_empty() {
        return;
    }
    let Some(p) = s.proj() else { return };
    let latest = p.live.mail.replies(id).last().map(|m| m.id.clone()).unwrap_or_else(|| id.to_string());
    // the To row's recipients, and each @NAME typed (one naming no agent stops the send)
    let mut to: Vec<String> = recipients(s, p, id, to).into_iter().filter(|(_, left)| !left).map(|(n, _)| n).collect();
    // and each agent tagged @NAME in it or in its comments (a tag naming no agent stops the send)
    let notes: Vec<&str> = std::iter::once(typed.as_str()).chain(comments.iter().map(|c| c.note.as_str())).collect();
    match tagged(p, &notes) {
        Ok(names) => to.extend(names.into_iter().filter(|n| !to.contains(n)).collect::<Vec<_>>()),
        Err(e) => {
            s.notice = Some(e);
            return;
        }
    }
    if to.is_empty() {
        s.notice = Some("not sent: nobody on the To row".into());
        return;
    }
    let (body, quotes) = compose(&comments, &typed);
    let kept = format!("{}/{}", p.id, p.live.mail.top(id));
    let req = Request::Msg { to, body, esc: false, soon: false, as_host: None, re: Some(latest), quotes };
    // the thread stays as it is shown (its card in the list, its reply box open), as in Slack
    if s.mail(req) {
        s.notes.kept.insert(kept);
        s.notes.answers.remove(id);
        s.notes.comments.remove(id);
        s.notes.added.remove(id);
        s.notes.left_out.remove(id);
        if s.notes.mention.as_ref().is_some_and(|m| m.field == Field::Answer(id.to_string())) {
            s.notes.mention = None;
        }
    }
}

/// The mails a chat entry of kind `mail` carries: the channel form (`<channel source="coord" from=.. id=..>`) or the
/// batch form (`[ID · from NAME]` headings); (id, from, body as delivered).
pub fn delivered(entry: &str) -> Vec<(String, String, String)> {
    let attr = |head: &str, name: &str| head.split(&format!("{name}=\"")).nth(1).and_then(|r| r.split('"').next()).unwrap_or("").to_string();
    let mut out = Vec::new();
    if entry.trim_start().starts_with("<channel") {
        for part in entry.split("<channel").skip(1) {
            let Some((head, rest)) = part.split_once('>') else { continue };
            let body = rest.split("</channel>").next().unwrap_or(rest).trim().to_string();
            out.push((attr(head, "id"), attr(head, "from"), body));
        }
        return out;
    }
    let mut cur: Option<(String, String, Vec<&str>)> = None;
    for line in entry.lines() {
        let heading = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')).and_then(|l| l.split_once(" · from "));
        match heading {
            Some((id, from)) if !id.contains(' ') => {
                out.extend(cur.take().map(|(i, f, b)| (i, f, b.join("\n").trim().to_string())));
                cur = Some((id.to_string(), from.to_string(), Vec::new()));
            }
            _ => {
                if let Some((_, _, b)) = cur.as_mut() {
                    b.push(line);
                }
            }
        }
    }
    out.extend(cur.map(|(i, f, b)| (i, f, b.join("\n").trim().to_string())));
    out
}

/// A chat entry of kind `mail` as its mails: each with who sent it, what it answers, and (Slack-like) its thread under
/// it with a Reply button; a mail in a question's thread points at the question's card instead. None when the text
/// is not in a delivered form.
pub fn mail_entry(s: &App, p: &Proj, agent: &str, entry: &str, at: &str, k: &Kit) -> Option<El> {
    let mails = delivered(entry);
    if mails.is_empty() {
        return None;
    }
    let mut c = col().w_full().gap(10.0);
    for (id, from, shown) in mails {
        let rec = p.live.mail.get(&id);
        let body = rec.map_or(shown, |m| m.body.clone());
        let who = if from == "owner" { "you".to_string() } else { from.clone() };
        let mut head = row().gap(6.0).center_y().child(text(format!("mail from {who}")).font(k.ty.small_medium).color(k.c.text_accent)).child(text(format!("{id} · {}", crate::domain::local(at))).font(k.ty.small).color(k.c.text_faint));
        if let Some(r) = rec.and_then(|m| m.re.clone()) {
            head = head.child(text(format!("re {r}")).font(k.ty.small).color(k.c.text_muted));
        }
        let mut card = col()
            .key(format!("qmsg-{id}"))
            .w_full()
            .gap(8.0)
            .padding(12.0)
            .radius(k.radius.md)
            .fill(k.c.layer_01)
            .border(1.0, k.c.border_weak)
            .child({
                let marks = marks(s, p, &id, &body, k);
                Markdown::new(body).highlights(marks).selectable(true).build(k)
            });
        let in_question = rec.and_then(|m| p.live.mail.root(m)).map(|q| q.id.clone());
        match in_question {
            Some(q) => card = card.child(text(format!("in the thread of {q} (its card shows the whole thread)")).font(k.ty.small).color(k.c.text_faint)),
            None if !id.is_empty() => {
                for m in p.live.mail.replies(&id) {
                    card = card.child(thread_row(s, p, m, k));
                }
                let open = s.notes.replying.contains(&id);
                let n = p.live.mail.replies(&id).len();
                let label = match (open, n) {
                    (true, _) => "Close".to_string(),
                    (false, 0) => "Reply".to_string(),
                    (false, n) => format!("Reply · {n} in thread"),
                };
                let toggle = {
                    let id = id.clone();
                    move |s: &mut App| {
                        if !s.notes.replying.remove(&id) {
                            s.notes.replying.insert(id.clone());
                        }
                    }
                };
                card = card.child(row().w_full().justify_end().child(Button::new(label).variant(ButtonVariant::Ghost).on_press(toggle).build(k)));
                if open {
                    card = card.child(reply_box(s, &id, agent, &format!("Reply in this thread to {agent} (Cmd/Ctrl+Enter sends)"), "Send", k));
                }
            }
            None => {}
        }
        if s.notes.replying.contains(&id) {
            let target = id.clone();
            card = card.selection_group(true).selection_menu_item("comment", "Comment", move |s: &mut App, sel, _| {
            let pi = s.cur;
            comment_on(s, pi, &target, sel)
        });
        }
        c = c.child(col().w_full().gap(6.0).child(head).child(card));
    }
    Some(c)
}

/// The project's questions as one line per thread, by where each stands ([`Standing`]): the owner's move first, then
/// what waits on the agents, then the quiet ones (and, with All, the settled ones); oldest first in each, so what
/// waited longest is on top. A row opens into the thread's card. With text in the search box, the project's questions
/// and mail matching every word instead.
pub fn inbox(s: &App, p: &Proj, k: &Kit) -> El {
    let pick = SegmentedControl::new(&["Open", "All"], Some(s.notes.show_all as usize)).label("Questions shown").fit().on_pick(|s: &mut App, i, _| s.notes.show_all = i == 1).build(k);
    let search = rune_kit::TextInput::new(&s.notes.search).placeholder("Search questions and mail").width(None).on_edit(|s: &mut App| &mut s.notes.search).build(k);
    let query = s.notes.search.text.trim().to_lowercase();
    let mut list = col().w_full().gap(6.0).px(24.0).pb(24.0);
    if !query.is_empty() {
        list = found(s, p, &query, list, k);
    } else {
        let kept = |q: &MailView| s.notes.kept.contains(&format!("{}/{}", p.id, q.id));
        let mut groups: [(Standing, &str, Vec<&MailView>); 4] = [
            (Standing::Yours, "Your move", Vec::new()),
            (Standing::Theirs, "Waiting on agents", Vec::new()),
            (Standing::Quiet, "Quiet for a day", Vec::new()),
            (Standing::Settled, "Settled", Vec::new()),
        ];
        for q in questions(p, true).into_iter().rev() {
            // a thread answered on this visit stays in its group until the owner leaves the page
            let st = if kept(q) { Standing::Yours } else { standing(s, p, q) };
            if let Some(g) = groups.iter_mut().find(|g| g.0 == st) {
                g.2.push(q);
            }
        }
        let shown: Vec<_> = groups.iter().filter(|g| s.notes.show_all || g.0 != Standing::Settled).collect();
        if shown.iter().all(|g| g.2.is_empty()) {
            list = list.child(text("Nothing waits on you or on the agents").font(k.ty.body).color(k.c.text_faint));
        }
        for (st, title, qs) in shown {
            if qs.is_empty() {
                continue;
            }
            list = list.child(text(format!("{title} · {}", qs.len())).font(k.ty.small_medium).color(k.c.text_muted).pt(10.0));
            for q in qs {
                list = list.child(thread_line(s, p, q, *st, k));
                if s.notes.expanded.as_deref() == Some(q.id.as_str()) {
                    list = list.child(card(s, p, q, k));
                }
            }
        }
    }
    col()
        .key("questions")
        .w_full()
        .h_full()
        .child(row().w_full().gap(12.0).center_y().px(24.0).pt(16.0).pb(10.0).child(pick).child(col().grow(1.0).shrink().child(search)))
        .child(col().key("questions-list").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(list))
}

/// One thread in the inbox: who asks, what it asks now, how many mails and how long since the last one; pressed, it opens
/// into its card (or closes).
fn thread_line(s: &App, p: &Proj, q: &MailView, st: Standing, k: &Kit) -> El {
    let ask = current_ask(p, q);
    let last = latest(p, q);
    let n = p.live.mail.replies(&q.id).len() + 1;
    let open = s.notes.expanded.as_deref() == Some(q.id.as_str());
    let since = match st {
        Standing::Theirs => format!("you wrote {}", ago(&last.at)),
        _ => format!("last {} {}", if last.from == "owner" { "you".to_string() } else { last.from.clone() }, ago(&last.at)),
    };
    let ids = if ask.id == q.id { q.id.clone() } else { format!("{} (now {})", q.id, ask.id) };
    let mut line = row()
        .w_full()
        .gap(10.0)
        .center_y()
        .child(text(q.from.clone()).font(k.ty.small_medium).color(k.c.text_accent).w(90.0).truncate())
        .child(text(one_line(&subject_of(ask), 300)).font(k.ty.ui).color(k.c.text_base).grow(1.0).shrink().truncate())
        .child(text(format!("{ids} · {n} mail{} · {since}", if n == 1 { "" } else { "s" })).font(k.ty.small).color(k.c.text_faint));
    if unread(s, p, q) {
        line = line.child(Badge::new("unread").variant(BadgeVariant::Accent).build(k));
    }
    if matches!(st, Standing::Quiet | Standing::Yours) {
        let (project, id) = (p.id.clone(), q.id.clone());
        line = line.child(
            Button::new("Dismiss")
                .variant(ButtonVariant::GhostMuted)
                .on_press(move |s: &mut App| {
                    if let Some(i) = s.projects.iter().position(|x| x.id == project) {
                        s.mail_in(i, Request::Drop { id: id.clone(), why: Some("dismissed by the owner".into()) });
                    }
                })
                .build(k),
        );
    }
    let id = q.id.clone();
    k.interactive(
        row()
            .key(format!("qline-{}", q.id))
            .w_full()
            .px(12.0)
            .py(8.0)
            .radius(k.radius.md)
            .fill(if open { k.c.layer_02 } else { k.c.layer_01 })
            .border(1.0, if st == Standing::Yours { k.c.border_focus } else { k.c.border_weak })
            .child(line),
    )
    .on_press(move |s: &mut App| toggle_expanded(s, &id))
}

fn toggle_expanded(s: &mut App, id: &str) {
    s.notes.expanded = if s.notes.expanded.as_deref() == Some(id) { None } else { Some(id.to_string()) };
}

/// The search: the project's mails (questions, answers, messages) whose text, sender, recipients or id hold every word of
/// QUERY, newest first, at most [`HITS`]; each with the passage around its first word, opening into its thread's card
/// (a question thread) or its whole text.
fn found(s: &App, p: &Proj, query: &str, mut list: El, k: &Kit) -> El {
    const HITS: usize = 100;
    let words: Vec<&str> = query.split_whitespace().collect();
    let hit = |m: &MailView| {
        let hay = format!("{} {} {} {}", m.id, m.from, m.to.join(" "), m.body).to_lowercase();
        words.iter().all(|w| hay.contains(w))
    };
    let hits: Vec<&MailView> = p.live.mail.iter().rev().filter(|m| hit(m)).take(HITS + 1).collect();
    if hits.is_empty() {
        return list.child(text("Nothing matches").font(k.ty.body).color(k.c.text_faint));
    }
    list = list.child(text(if hits.len() > HITS { format!("The newest {HITS} matches") } else { format!("{} match{}", hits.len(), if hits.len() == 1 { "" } else { "es" }) }).font(k.ty.small_medium).color(k.c.text_muted));
    for m in hits.into_iter().take(HITS) {
        let root = p.live.mail.root(m);
        let open = s.notes.expanded.as_deref() == Some(m.id.as_str());
        let head = row()
            .w_full()
            .gap(10.0)
            .child(text(format!("{} → {}", m.from, m.to.join(", "))).font(k.ty.small_medium).color(k.c.text_accent).shrink().truncate())
            .child(text(match root {
                Some(r) if r.id != m.id => format!("{} · in {} · {}", m.id, r.id, ago(&m.at)),
                _ => format!("{} · {}", m.id, ago(&m.at)),
            }).font(k.ty.small).color(k.c.text_faint));
        let id = m.id.clone();
        list = list.child(
            k.interactive(
                col()
                    .key(format!("hit-{}", m.id))
                    .w_full()
                    .gap(4.0)
                    .px(12.0)
                    .py(8.0)
                    .radius(k.radius.md)
                    .fill(if open { k.c.layer_02 } else { k.c.layer_01 })
                    .border(1.0, k.c.border_weak)
                    .child(head)
                    .child(text(passage(&m.body, words[0])).font(k.ty.small).color(k.c.text_muted).wrap().max_lines(2)),
            )
            .on_press(move |s: &mut App| toggle_expanded(s, &id)),
        );
        if open {
            list = list.child(match root {
                Some(r) => card(s, p, r, k),
                None => col().w_full().padding(14.0).radius(k.radius.md).fill(k.c.layer_01).child(Markdown::new(m.body.clone()).selectable(true).build(k)),
            });
        }
    }
    list
}

/// The words of BODY around the first place WORD (lower case) is, on one line.
fn passage(body: &str, word: &str) -> String {
    let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");
    // lower case can change a character's length: a place that is no boundary of FLAT starts at the beginning
    let at = flat.to_lowercase().find(word).filter(|&i| flat.is_char_boundary(i)).unwrap_or(0);
    let from = flat[..at].char_indices().rev().nth(80).map_or(0, |(i, _)| i);
    let pre = if from > 0 { "…" } else { "" };
    format!("{pre}{}", one_line(&flat[from..], 240))
}

/// A chat entry that is `coord ask`'s answer (`qN: asked…`): the question it made, if the mail has it.
pub fn asked<'a>(p: &'a Proj, entry_text: &str) -> Option<&'a MailView> {
    let id = entry_text.trim_start().split(':').next().filter(|id| id.starts_with('q') && id[1..].parse::<u64>().is_ok())?;
    entry_text.contains(": asked;").then_some(())?;
    let m = p.live.mail.iter().find(|m| m.id == id && m.kind == "question")?;
    p.live.mail.root(m)
}
