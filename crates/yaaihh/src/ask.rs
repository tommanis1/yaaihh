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
//! **Comments.** Text selected in a chat, a thread or a question card offers Comment; each comment keeps the passage,
//! where it came from and the owner's note. Its box is the one message box, at the passage, and its Send, `Add
//! comment`, holds it for the next message of the place it goes with ([`Notes::held`]; his walk item 11): a
//! thread's, an agent's chat's (a comment on a file goes to its reader's), a mail's reply. Nothing goes at its press
//! (the owner, N-1790 part 3); that place's Send takes each held comment with its quote ([`compose`]), and its passage
//! stays lit meanwhile, its box saying where it waits.

use crate::{App, Proj};
use coord_types::{MailView, Quote, Request};
use rune::{El, TextEdit, col, row, text};
use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};
use rune_kit::markdown::Highlight;
use rune_kit::{Badge, BadgeVariant, Button, ButtonVariant, Combobox, ComboboxOption, ComboboxState, Kit, PopupItem, PopupList, TextArea};
use std::collections::HashMap;
use std::ops::Range;
use serde_json::json;
use serde_json::Value;
use serde_json::Map;
use rune_kit::TextInput;
use docket_machine::ViewStatus;
use docket_machine::Type;
use crate::threads::{Cleared, Page};
use crate::texts::{key, questions_wanted, words};
use crate::kinds::type_hint;
use crate::Pending;
use coord_types::TicketView;

/// A comment on a passage: the passage, where it came from, and the owner's note ([`send`]; on its way, outgoing.rs
/// `Out::comment`).
#[derive(Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Comment {
    /// `AGENT@OFFSET` (an entry of the agent's session) or a mail or question id.
    pub source: String,
    /// Where it came from, as the agent is told (`your message at 14:02Z`: in UTC, as it reads every time).
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
    /// The place the comment goes to: an agent's name (its chat), `thread:ID`, or a mail's thread id.
    pub target: String,
    pub source: String,
    pub label: String,
    pub quote: String,
    #[serde(default)]
    pub at: Vec<Range<usize>>,
    /// The project of the pane it was made in.
    #[serde(default)]
    pub pi: usize,
    /// Where he selected it: the keyed path (as rune's layout reads take one) of the element his selection began in,
    /// a thread's post, a mail's message, a chat's entry, a line of a thread as it ran, a file. The box stands under
    /// the passage lit, else under this, never at a composer (I-1476; his N-1104 p706).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    /// The part of the source's words he selected it in, by its body's id (post.rs `message_part`, `body-…`): its
    /// passage is lit by its words only there, once in a post drawn in parts (proto-window, N-1477 p8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<String>,
    /// The split's pane its lines stand in (panes.rs `pane-J`), fixed as the box opens ([`write`]) and moved with the
    /// panes ([`panes_moved`]), never with his focus (consult-3 N-1477 p46): 0 with no split (the page, the split's
    /// first pane when one opens); none outside the panes (a popup).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane: Option<usize>,
    /// The passage in plain texts, a popup's or a lens pane's (no Markdown place of their own): each text's key and the
    /// bytes of it selected, lit there as the passage being written on ([`lit`]), so its box opens under them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<(String, Range<usize>)>,
}

#[derive(Default)]
pub struct Notes {
    pub picked: Option<Picked>,
    /// The comments he has added on passages of messages, per place they go to (project, and an agent's name,
    /// `thread:ID` or a mail's thread id), oldest first: they go with that place's next Send, together in one message
    /// with what its box holds, each with its quote ([`held`]; his walk item 11, paper-research N-1446 p3521). Adding
    /// one sends nothing.
    pub held: HashMap<(usize, String), Vec<Comment>>,
    /// The comment whose box is open, by its highlight's key ([`sent_key`], a thread post's `cp-`).
    pub open: Option<String>,
    /// The comment being written has its box closed (Esc, Close, its mark pressed again), his words kept as the box's
    /// draft and its passage lit; a press on the passage opens the box again with them ([`close_mark`]).
    pub shut: bool,
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
    /// The inbox row opened into its card (a thread's first question), or the search hit opened (a mail's id).
    pub expanded: Option<String>,
    /// The inbox's search over the project's questions and mail.
    pub search: TextEdit,
    /// The comment box stands in the flow, its pane too narrow for it beside the lines: the surfaces leave it room
    /// under its passage ([`room`], [`room_at`]) and the layer pins it there, nothing covered (I-1476; paper-research
    /// N-1477 p9). Set by [`decide`] each pass before the surfaces are drawn, with whether its passage was lit (else the
    /// room is at the end of what he selected in), how tall the box was (the room's height), and where the box goes:
    /// its top left and width, None for no box.
    pub inline: std::cell::Cell<bool>,
    pub lit: std::cell::Cell<bool>,
    pub tall: std::cell::Cell<f32>,
    pub spot: std::cell::Cell<Option<[f32; 3]>>,
    /// A room was drawn in words split for it this pass ([`room`]): the surface draws none at its end ([`room_at`];
    /// consult-3 N-1477 p30). [`decide`] clears it before the surfaces are drawn.
    pub roomed: std::cell::Cell<bool>,
    /// The split's pane being drawn (`pane-J`: panes.rs `area`, lib.rs `view`), None outside it or with no split: only
    /// the comment box's own pane lights its passage and draws its room ([`ours`]).
    pub drawing: std::cell::Cell<Option<usize>>,
    /// The words each message body was last drawn from, by the body's key (post.rs `message_part`): a comment made in
    /// one takes its quote's cut words whole from them ([`whole_words`]).
    pub drawn: std::cell::RefCell<HashMap<String, std::rc::Rc<str>>>,
    /// The box whose room its pane last scrolled into view, by [`opened`], and the room's height then ([`room_el`]).
    pub revealed: Option<(String, u32)>,
}

/// An `@NAME` being typed: in box FIELD, the `@` at byte AT, QUERY typed after it, ACTIVE the suggestion picked.
#[derive(Clone, Debug)]
pub struct Mention {
    pub field: Field,
    pub at: usize,
    pub query: String,
    pub active: usize,
}

/// A text box `@NAME` works in (owner m118): a thread's reply box, an agent chat's composer, the one message box in
/// every place (the comment on a passage too: its place is `message_box::COMMENT_PLACE`). A tagged agent is one more
/// recipient of what the box sends.
#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    /// The reply box of mail thread ID (its box's place is ID): a tag there joins its To row too.
    Reply(usize, String),
    Composer(usize, String),
}

impl Field {
    /// The key of the element around the box (its caret anchors the suggestions).
    pub fn key(&self) -> String {
        match self {
            Field::Reply(pi, id) => format!("cf-{pi}-{id}"),
            Field::Composer(pi, a) => format!("cf-{pi}-{a}"),
        }
    }

    fn edit<'a>(&self, s: &'a mut App) -> Option<&'a mut TextEdit> {
        match self {
            Field::Reply(pi, id) => s.composers.get_mut(&(*pi, id.clone())),
            Field::Composer(pi, a) => s.composers.get_mut(&(*pi, a.clone())),
        }
    }
}

/// The agents tagged `@NAME` in TEXTS, each once: an agent by its name, and each running agent of a group tagged
/// (`@team`, `@PART`: `coord_types::groups`, the rule coordd reads his posts by too); Err names a tag that is neither, or
/// a group none of whose agents runs. His `@all` is neither and names nobody here: coordd reads it in his words itself
/// (`docket_machine::ALL`, `at_name`), reaching every agent on the thread, one of whom takes it (paper-research N-1446
/// p1327: "`@all` is refused … and he types it").
pub fn tagged(p: &Proj, texts: &[&str]) -> Result<Vec<String>, String> {
    let agents = members(p);
    let mut out: Vec<String> = Vec::new();
    for word in texts.iter().flat_map(|t| mentioned(t)) {
        let names = match (agent_named(p, word), coord_types::groups::group(word, &p.live.parts, &agents)) {
            (Some(n), _) => vec![n],
            (None, Some(g)) if g.members.is_empty() => return Err(format!("not sent: nobody in @{} is running", g.tag)),
            (None, Some(g)) => g.members,
            (None, None) if word == docket_machine::ALL => continue,
            (None, None) => return Err(format!("not sent: no agent or group named @{word}")),
        };
        for n in names {
            if !out.iter().any(|x| x == n) {
                out.push(n.to_string());
            }
        }
    }
    Ok(out)
}

pub use coord_types::groups::{Group, TEAM};

/// P's agents as a group is read from (0.2 B10; the owner, N-1033.p1: "send this message to the team"): each one's
/// name, part and whether it runs. The groups themselves are `coord_types::groups`'s, one rule for the window and for
/// coordd (consult-3, N-1402.p2).
fn members(p: &Proj) -> Vec<coord_types::groups::Member<'_>> {
    p.live.agents.iter().map(|a| coord_types::groups::Member { name: &a.name, part: &a.part, running: a.state != "stopped" }).collect()
}

/// Every group of P an `@` offers: the team, then each part in the sidebar's order.
fn groups(p: &Proj) -> Vec<Group<'_>> {
    coord_types::groups::groups(&p.live.parts, &members(p))
}

/// Whether P's coord reads a group in his post on a thread (`GROUPS_PROTOCOL`); below it a post's group tag would
/// reach nobody, so the box refuses it there ([`NO_GROUP_ON_POST`]). A Say carries the group's agents as its names at
/// any protocol.
pub fn post_takes_groups(p: &Proj) -> bool {
    p.protocol >= coord_types::GROUPS_PROTOCOL
}

/// Said when words that go as a post on a thread tag a group and P's coord does not read groups in posts.
pub const NO_GROUP_ON_POST: &str = "not sent: @team needs an agent's chat or a new thread";

/// Whether TEXTS, going as a post on a thread, tag a group P's coord would not read there.
pub fn group_refused(p: &Proj, texts: &[&str]) -> bool {
    !post_takes_groups(p) && texts.iter().any(|t| !coord_types::groups::tagged(t, &p.live.parts, &members(p)).is_empty())
}

/// Whom TEXT's tags reach, as a line names them: each agent tagged by its name, each group by its name and how many of
/// its agents run (`the team (19)`); each once, in order.
pub fn whom(p: &Proj, text: &str) -> Vec<String> {
    let agents = members(p);
    let mut out: Vec<String> = Vec::new();
    for word in mentioned(text) {
        let said = match (agent_named(p, word), coord_types::groups::group(word, &p.live.parts, &agents)) {
            (Some(n), _) => n.to_string(),
            (None, Some(g)) => format!("{} ({})", g.name, g.members.len()),
            (None, None) => continue,
        };
        if !out.contains(&said) {
            out.push(said);
        }
    }
    out
}

/// The agents of the groups his post's TEXT tags that it did not reach (none of REACHED), whatever their state now:
/// one stopped at Send was left out and is never given it after, so the line under his message stays once it runs
/// again (consult-3, N-1402.p2, 3 and p4). An agent made after AT, the post's time, joined later and is not counted.
/// Not covered (left for after 0.2, consult-3 p6): an agent that moved into the part after the post is counted, and its
/// line says "it was stopped", which is not true. Closing it needs whom the post left out at Send recorded on the post
/// (a field of its own).
pub fn not_reached<'a>(p: &'a Proj, text: &str, reached: &[&str], at: &str) -> Vec<&'a str> {
    let when = |t: &str| chrono::DateTime::parse_from_rfc3339(t).ok();
    let posted = when(at);
    let later = |n: &str| p.live.agents.iter().any(|a| a.name == n && matches!((when(&a.created), posted), (Some(c), Some(t)) if c > t));
    let mut out: Vec<&str> = Vec::new();
    for g in coord_types::groups::tagged(text, &p.live.parts, &members(p)) {
        for n in g.members.into_iter().chain(g.stopped) {
            if !reached.contains(&n) && !out.contains(&n) && !later(n) {
                out.push(n);
            }
        }
    }
    out
}

fn read_path() -> std::path::PathBuf {
    std::path::Path::new(&crate::projects::config_path()).with_file_name("read.json")
}

impl Notes {
    pub fn load() -> Notes {
        let read = std::fs::read_to_string(read_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Notes { read, ..Default::default() }
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

/// The `@word`s of TEXT (an `@` at the start or after a space, then name characters): the rule coordd reads too.
fn mentioned(text: &str) -> Vec<&str> {
    coord_types::groups::tags_in(text)
}

/// The agent named NAME in P (letter case aside).
fn agent_named<'a>(p: &'a Proj, name: &str) -> Option<&'a str> {
    p.live.agents.iter().find(|a| a.name.eq_ignore_ascii_case(name)).map(|a| a.name.as_str())
}

/// What an `@QUERY` offers, each as its tag and the line under it: the agents (id and state) and the groups (that each
/// of them answers, and how many it reaches; 0.2 B10). Tags starting with it first, then tags containing it; at each, `@team`, then agents, then parts;
/// running before stopped.
fn suggestions(p: &Proj, query: &str) -> Vec<(String, String)> {
    let q = query.to_lowercase();
    let mut out: Vec<(bool, u8, bool, String, String)> = Vec::new();
    for g in groups(p).into_iter().filter(|g| g.tag.to_lowercase().contains(&q)) {
        // each of them answers, where `@all`'s one claims it (consult-3, N-1402.p2, 4)
        let left = if g.stopped.is_empty() { String::new() } else { format!(", {} stopped not reached", g.stopped.len()) };
        let about = format!("each of them answers · {} running{left}", g.members.len());
        out.push((!g.tag.to_lowercase().starts_with(&q), if g.tag == TEAM { 0 } else { 2 }, g.members.is_empty(), g.tag, about));
    }
    for a in p.live.agents.iter().filter(|a| a.name.to_lowercase().contains(&q)) {
        out.push((!a.name.to_lowercase().starts_with(&q), 1, a.state == "stopped", a.name.clone(), format!("{} · {}", a.id, a.state.lines().next().unwrap_or(""))));
    }
    out.sort_by(|x, y| (x.0, x.1, x.2, &x.3).cmp(&(y.0, y.1, y.2, &y.3)));
    out.truncate(8);
    out.into_iter().map(|(_, _, _, tag, about)| (tag, about)).collect()
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

/// Complete the `@` being typed to `@NAME ` (in a thread's reply box an agent NAME joins its To row too; a group stays
/// a tag, its agents reached when it is sent).
fn complete(s: &mut App, name: &str) {
    let agent = s.proj().is_some_and(|p| agent_named(p, name).is_some());
    let Some(m) = s.notes.mention.take() else { return };
    let Some(e) = m.field.edit(s) else { return };
    let caret = e.focus.min(e.text.len());
    let text = format!("{}@{name} {}", &e.text[..m.at], &e.text[caret..]);
    e.replace_text(text);
    e.set_caret(m.at + name.len() + 2);
    if let Field::Reply(_, id) = &m.field
        && agent
    {
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
    let names: Vec<String> = suggestions(p, &m.query).into_iter().map(|(tag, _)| tag).collect();
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
    let items = found.iter().map(|(tag, about)| PopupItem::new(tag.clone(), tag.clone()).description(about.clone()));
    PopupList::new("mentions", "mention-caret")
        .items(items)
        .active(Some(m.active.min(found.len() - 1)))
        .label("Agents and groups")
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

/// How long ago AT was, as a time taken is written ([`crate::when::took`]; paper-research I-1469 p50): `just now` under
/// a minute, then `5 m ago`, `3 h 05 m ago`, `2 d 04 h ago`.
pub fn ago(at: &str) -> String {
    let Ok(t) = chrono::DateTime::parse_from_rfc3339(at) else { return String::new() };
    match (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_seconds() {
        ..60 => "just now".into(),
        s => format!("{} ago", crate::when::took(s)),
    }
}


// ---- comments -----------------------------------------------------------------------------------------------------
//
// A comment is made on selected text with the selection menu's Comment (right-click; Rune's
// `El::selection_menu_item`). Its box opens at the passage, which is highlighted where it stands (the selection's
// Markdown source ranges, `SelectedText::source`); Add comment keeps it with his next message at the place it goes
// to, which takes it at its Send (a file's goes at once; [`send`]), and once sent its quote keeps it highlighted (a mail's quotes; a thread post's passages, passage.rs). A click on a highlight opens its
// comment there (`El::on_highlight_press`). Owner 8 Nov (m30, m33).

/// The highlight key of quote J of mail ID (a comment sent).
pub fn sent_key(id: &str, j: usize) -> String {
    format!("cq-{id}-{j}")
}

/// The highlight key of the passage a comment is being written on.
const NEW: &str = "cm-new";

/// The selection menu's Comment on SEL, made in pane project PI for TARGET (an agent's chat, or a question's or
/// mail's thread): the passage, where it came from and its place, with the note box open at it. TX holds the element
/// he selected in where it shows as the box's room opens under his lines: a thread or chat at its foot (its scroll
/// sticks there) would otherwise pull everything above the room up by its height, his lines and the box with them
/// toward the pane's top (his N-1104 p1168; paper-research N-1446 p3062).
pub fn comment_on(s: &mut App, pi: usize, target: &str, sel: &rune::TextSelection, tx: &mut rune::Tx) {
    let quote = sel.text().trim().to_string();
    if quote.is_empty() || begun_elsewhere(s) {
        return;
    }
    // the entry or mail the selection starts in, by its element key
    let first = sel.texts().iter().find(|t| t.is_selected());
    let key = first.and_then(|t| source_key(t.path())).map(str::to_string);
    let (source, label) = match key.as_deref() {
        None => (target.to_string(), String::new()),
        Some(k) => match (k.strip_prefix("qmsg-"), post_key(k)) {
            (Some(id), _) => (id.to_string(), id.to_string()),
            // a thread's post, or another line of the thread as it ran, whose ticket is then what the reply follows
            (None, Some((id, n))) => (format!("{id}.p{n}"), post_label(s, pi, &id, n)),
            (None, None) if k.starts_with("ran-") => {
                let id = k.trim_start_matches("ran-").split('.').next().unwrap_or_default().to_string();
                (id.clone(), format!("a line of {id}"))
            }
            (None, None) => entry_source(s, pi, target, k),
        },
    };
    // its place: the source ranges of what is selected inside that entry or mail, a word cut at either end taken whole
    let (quote, at) = match &key {
        Some(k) => {
            let texts: Vec<&rune::SelectedText> = sel.texts().iter().filter(|t| t.path().split('/').any(|x| x == k) && !t.source().is_empty()).collect();
            let at = texts.iter().flat_map(|t| in_source(t.path(), t.source())).collect();
            whole_words(s, &texts, quote, at)
        }
        None => (quote, Vec::new()),
    };
    let anchor = first.and_then(|t| keyed(t.path(), key.as_deref()));
    let block = first.and_then(|t| t.path().split('/').rev().find(|x| x.starts_with("body-")).map(|x| after(x).0.to_string()));
    if let Some(a) = &anchor {
        tx.keep_on_screen(a.clone());
    }
    s.focus_on(pi, target);
    s.notes.open = None;
    write(s, Picked { target: target.into(), source, label, quote, at, pi, anchor, block, pane: None, texts: Vec::new() }, "");
}

/// QUOTE, selected in TEXTS (the selected texts of one post, line or entry; AT, their places in the words of the body
/// each is drawn in), with a word cut at either end taken whole and AT widened with it. A drag that stops inside a word,
/// as at the edge of a code block whose lines run on past the pane, took the word cut, and the agent read it so (his
/// walk item 12: N-1104 p1190 quoted p1189's `phase == "end"` as `phase == "en`; paper-research N-1446 p3524). A word
/// is cut where letters or digits stand on both sides of an end in the words the body was drawn from
/// ([`Notes::drawn`]), and it is taken on over letters, digits and `_`; an end that does not read so there, or whose
/// words there are not what is selected, stays as selected.
fn whole_words(s: &App, texts: &[&rune::SelectedText], mut quote: String, mut at: Vec<Range<usize>>) -> (String, Vec<Range<usize>>) {
    let (Some(first), Some(last), Some(start), Some(end)) = (texts.first(), texts.last(), at.first().cloned(), at.last().cloned()) else {
        return (quote, at);
    };
    let drawn = |t: &rune::SelectedText| {
        let body = t.path().split('/').rev().find(|x| x.starts_with("body-"))?;
        s.notes.drawn.borrow().get(after(body).0).cloned()
    };
    let word = |c: char| c.is_alphanumeric() || c == '_';
    if let Some(src) = drawn(*first)
        && let (Some(before), Some(piece)) = (src.get(..start.start), src.get(start.clone()))
        && before.ends_with(char::is_alphanumeric)
        && piece.starts_with(char::is_alphanumeric)
        && piece.lines().next().is_some_and(|l| quote.starts_with(l))
    {
        let more = &before[before.trim_end_matches(word).len()..];
        quote.insert_str(0, more);
        at[0].start -= more.len();
    }
    if let Some(src) = drawn(*last)
        && let (Some(piece), Some(rest)) = (src.get(end.clone()), src.get(end.end..))
        && piece.ends_with(char::is_alphanumeric)
        && rest.starts_with(char::is_alphanumeric)
        && piece.lines().last().is_some_and(|l| quote.ends_with(l))
    {
        let more = &rest[..rest.len() - rest.trim_start_matches(word).len()];
        quote.push_str(more);
        if let Some(r) = at.last_mut() {
            r.end += more.len();
        }
    }
    (quote, at)
}

/// Keeps SRC as the words the message body keyed ID is drawn from (post.rs `message_part`, each pass), for a comment
/// made in it ([`whole_words`]); the bodies of a long session's streams are let go together past a few thousand.
pub(crate) fn drawn(s: &App, id: &str, src: &str) {
    let mut d = s.notes.drawn.borrow_mut();
    if d.get(id).is_some_and(|x| **x == *src) {
        return;
    }
    if d.len() >= 4096 {
        d.clear();
    }
    d.insert(id.to_string(), src.into());
}

/// The key of the element a selected text on PATH lies in, which [`comment_on`] takes the comment's source from: a
/// queued mail's (`qmsg-ID`), else a thread's post (threads.rs `post-ID-N`), else another line of a thread as it ran
/// (ran.rs `ran-KEY`: a prompt, an answer), else an agent's chat entry; None in none of them (a thread's title).
fn source_key(path: &str) -> Option<&str> {
    let keys: Vec<&str> = path.split('/').collect();
    let last = |f: fn(&str) -> bool| keys.iter().rev().copied().find(|k| f(k));
    last(|k| k.starts_with("qmsg-")).or_else(|| last(|k| post_key(k).is_some())).or_else(|| last(|k| k.starts_with("ran-"))).or_else(|| keys.iter().copied().find(|k| entry_key(k)))
}

/// Whether the selection menu offers Comment on SEL (`El::selection_menu_item_when`): all it selects lies in one post,
/// line, mail or entry ([`source_key`]). A selection that spans two copies whole, and its menu offers Copy alone: a
/// comment would take the first one's part and send what he did not select (paper-research N-1714 p10).
pub fn one_source(sel: &rune::TextSelection) -> bool {
    let mut keys = sel.texts().iter().filter(|t| t.is_selected()).map(|t| source_key(t.path()));
    keys.next().is_none_or(|k| keys.all(|x| x == k))
}

/// A part of words split for the comment box's room ([`split`]), by its key: the key of the words it is a part of,
/// and where in them it starts (`KEY-afterN`); KEY itself, at 0, for any other.
fn after(key: &str) -> (&str, usize) {
    match key.rsplit_once("-after").and_then(|(k, n)| Some((k, n.parse::<usize>().ok()?))) {
        Some((k, n)) => (k, n),
        None => (key, 0),
    }
}

/// RANGES of a selected text on PATH, in the source of the words it was drawn from: past the room's split, the words
/// after it are drawn from where they start ([`after`]).
fn in_source(path: &str, ranges: &[Range<usize>]) -> Vec<Range<usize>> {
    let d = path.split('/').map(|x| after(x).1).max().unwrap_or(0);
    ranges.iter().map(|r| r.start + d..r.end + d).collect()
}

/// The keyed path of the element KEY on PATH (a selected text's), as rune's layout reads take a key path: PATH's keys
/// up to KEY's last place in it, its unkeyed places (`#N`) left out; with no KEY, all of PATH's keys, which name the
/// text's nearest keyed element. None when KEY is not on PATH.
pub(crate) fn keyed(path: &str, key: Option<&str>) -> Option<String> {
    let keys: Vec<&str> = path.split('/').filter(|x| !x.is_empty() && !x.starts_with('#')).collect();
    let upto = match key {
        Some(k) => keys.iter().rposition(|x| *x == k)? + 1,
        None => keys.len(),
    };
    (upto > 0).then(|| keys[..upto].join("/"))
}

/// The comment being written: the text in its box (the one message box, its place `COMMENT_PLACE`), while a passage
/// is picked.
/// A passage of a file in the file view selected and commented on (files.rs; 0.2 line C15, consult-lead N-1335 p4,
/// (C)): it goes to READER, the agent whose folder holds the file, else the project's `answerer`; an @NAME in the
/// comment sends it to that agent instead. With neither, the box opens all the same and asks for an @NAME.
pub fn comment_on_file(s: &mut App, pi: usize, source: &str, label: &str, reader: Option<&str>, sel: &rune::TextSelection) {
    let quote = sel.text().trim().to_string();
    if quote.is_empty() || begun_elsewhere(s) {
        return;
    }
    let at: Vec<Range<usize>> = sel.texts().iter().filter(|t| t.path().split('/').any(|x| x == crate::files::FILE_KEY)).flat_map(|t| in_source(t.path(), t.source())).collect();
    let anchor = sel.texts().iter().find(|t| t.is_selected()).and_then(|t| keyed(t.path(), Some(crate::files::FILE_KEY)));
    s.notes.open = None;
    write(s, Picked { target: reader.unwrap_or_default().into(), source: source.into(), label: label.into(), quote, at, pi, anchor, block: None, pane: None, texts: Vec::new() }, "");
}

/// A passage selected in a text apart from the page's posts, a popup's (popup.rs) or a lens pane's (lens.rs), and
/// commented on: it goes to TARGET (an agent's chat, `thread:ID`) as SOURCE, named LABEL (the popup's title, the lens
/// node's lines). Its place is kept as each text it is in keeps one: a Markdown text's in `at` (lit by [`marks`], as a
/// post's, in the part he selected it in: `block`), a plain text's by its key in `texts` (lit by [`lit`]); so its box
/// opens at the lines selected. AT_END is the element at whose end its room stands when the passage is lit nowhere (a
/// popup's body: `room_at`), else the text he selected in.
pub fn comment_apart(s: &mut App, pi: usize, target: &str, source: &str, label: &str, at_end: Option<&str>, sel: &rune::TextSelection) {
    let quote = sel.text().trim().to_string();
    if quote.is_empty() || begun_elsewhere(s) {
        return;
    }
    let chosen = || sel.texts().iter().filter(|t| t.is_selected());
    // past a room's split, a part's words are placed in the words whole ([`after`])
    let at = chosen().flat_map(|t| in_source(t.path(), t.source())).collect();
    let texts = chosen()
        .filter(|t| t.source().is_empty())
        .filter_map(|t| {
            let (key, n) = after(t.key()?);
            Some((key.to_string(), t.range().start + n..t.range().end + n))
        })
        .collect();
    let first = chosen().next();
    let anchor = first.and_then(|t| at_end.and_then(|e| keyed(t.path(), Some(e))).or_else(|| keyed(t.path(), None)));
    let block = first.and_then(|t| t.path().split('/').rev().find(|x| x.starts_with("body-")).map(|x| after(x).0.to_string()));
    s.notes.open = None;
    write(s, Picked { target: target.into(), source: source.into(), label: label.into(), quote, at, pi, anchor, block, pane: None, texts }, "");
}

/// QUOTE, selected in a window of its own (sent_window.rs) and commented on: as [`comment_apart`]'s, to TARGET as
/// SOURCE named LABEL, its box at the end of the element keyed AT_END (the popup it was opened from), lit nowhere here:
/// its words are in the other window.
pub fn comment_quote(s: &mut App, pi: usize, target: &str, source: &str, label: &str, at_end: &str, quote: &str) {
    let quote = quote.trim().to_string();
    if quote.is_empty() || begun_elsewhere(s) {
        return;
    }
    s.notes.open = None;
    write(s, Picked { target: target.into(), source: source.into(), label: label.into(), quote, at: Vec::new(), pi, anchor: Some(at_end.into()), block: None, pane: None, texts: Vec::new() }, "");
}

/// C, a popup's or a lens pane's text, commented on as a thread's page is (threads.rs `commentable`): a passage selected
/// offers Comment, F making it ([`comment_apart`], with where it goes), and a press on its lit passage opens or closes
/// its box.
pub fn commentable_apart(c: El, f: impl Fn(&mut App, &rune::TextSelection) + 'static) -> El {
    c.selection_group(true)
        .selection_menu_item("comment", "Comment", move |s: &mut App, sel, _| f(s, sel))
        .on_highlight_press(|s: &mut App, key, _| highlight_pressed(s, key))
}

/// Plain text EL keyed KEY, a popup's or a lens pane's, with the passage commented on as SOURCE lit where it was
/// selected in it ([`comment_apart`]): `decide`'s `at(NEW)` is then the lines selected, in every text they run
/// through, as a post's are in its Markdown ([`marks`]); in the split, only in the box's pane ([`ours`]: a lens's
/// lines drawn in two panes light one passage).
pub fn lit(s: &App, key: &str, source: &str, el: El, k: &Kit) -> El {
    let el = el.key(key);
    let Some(p) = s.notes.picked.as_ref().filter(|p| p.source == source && ours(s)) else { return el };
    let accent = k.c.bg_accent.alpha(0.28);
    p.texts.iter().filter(|(at, r)| at == key && !r.is_empty()).fold(el, |el, (_, r)| el.text_highlight(rune::TextHighlight::new(r.clone(), accent).key(NEW)))
}

/// The comment box's room after text KEY lit by [`lit`] (a lens pane's row), while the box stands in the flow: there
/// when KEY is the last text the passage was selected in, so a passage over several rows has one room, after them;
/// with the passage lit nowhere, after the text he selected in (`room_at`).
pub fn lit_room(s: &App, key: &str, source: &str) -> Option<El> {
    let last = s.notes.picked.as_ref().filter(|p| p.source == source).and_then(|p| p.texts.last()).is_some_and(|(at, r)| at == key && !r.is_empty());
    if last { room(s, source) } else { room_at(s, key) }
}

/// Plain WORDS keyed KEY, as MAKE draws them, lit as [`lit`] does; while the comment box stands in the flow, split after
/// the line its passage ends on, the box's room between ([`lit_room`]), so the lines after it move down under the box,
/// nothing covered (paper-research N-1477 p9). The words after are keyed `KEY-afterN`, N where they start in WORDS, so a
/// selection in them is placed in WORDS ([`comment_apart`]).
pub fn lit_words(s: &App, key: &str, source: &str, words: &str, make: impl Fn(String) -> El, k: &Kit) -> El {
    let Some(room) = lit_room(s, key, source) else { return lit(s, key, source, make(words.to_string()), k) };
    let end = s.notes.picked.as_ref().and_then(|p| p.texts.iter().filter(|(at, _)| at == key).map(|(_, r)| r.end).max());
    let end = end.filter(|e| words.is_char_boundary(*e));
    // the line its passage ends on whole, the words after from the next; lit nowhere, or on the last line, the room at
    // their end
    let (head, rest) = match end.and_then(|e| words[e..].find('\n').map(|i| e + i)) {
        Some(nl) => (&words[..nl], nl + 1),
        None => (words, words.len()),
    };
    let c = col().w_full().child(lit(s, key, source, make(head.to_string()), k)).child(room);
    if rest < words.len() { c.child(make(words[rest..].to_string()).key(format!("{key}-after{rest}"))) } else { c }
}

/// Whether the comment box is a popup's, its passage selected in one (popup.rs `source`): then it stands over the
/// popups (lib.rs), so he writes in it there; a box of the page stays under a popup opened over it.
pub fn apart(s: &App) -> bool {
    !s.popups.is_empty() && shown_on(s).is_some_and(|x| crate::popup::is_source(&x))
}

/// A comment on a file (files.rs `file_source`).
fn is_file(source: &str) -> bool {
    source.starts_with("file:")
}

const NO_READER: &str = "no one holds this file: tag an @agent";

/// The agent comment P on a file goes to, NOTE its words: the first agent tagged @NAME in them, else the file's reader
/// as it was begun; Err when there is neither, or a tag names no agent.
fn file_target(s: &App, p: &Picked, note: &str) -> Result<String, String> {
    let proj = s.projects.get(p.pi).ok_or_else(|| "the project is not open".to_string())?;
    let tags = tagged(proj, &[note])?;
    tags.into_iter().next().or_else(|| Some(p.target.clone()).filter(|t| !t.is_empty())).ok_or_else(|| NO_READER.to_string())
}

/// The comment being written, NOTE its words, aimed where it goes: a file's at its @NAME, else at its reader; None,
/// said in a notice, when a file's has no one to go to.
fn aimed(s: &mut App, note: &str) -> Option<Picked> {
    let mut p = s.notes.picked.clone().expect("writing() reads it");
    if is_file(&p.source) {
        match file_target(s, &p, note) {
            Ok(t) => p.target = t,
            Err(e) => {
                s.notice = Some(e);
                return None;
            }
        }
    }
    Some(p)
}

/// Whom a comment being written on a file goes to, said in its box; a tag being typed that names no agent yet leaves
/// it as it was.
fn goes_to(s: &App, p: &Picked) -> Option<String> {
    if !is_file(&p.source) {
        return None;
    }
    let note = writing(s).map(|w| w.text.as_str()).unwrap_or_default();
    Some(match file_target(s, p, note).ok().or_else(|| Some(p.target.clone()).filter(|t| !t.is_empty())) {
        Some(t) => format!("to {t}"),
        None => NO_READER.to_string(),
    })
}

pub fn writing(s: &App) -> Option<&TextEdit> {
    let p = s.notes.picked.as_ref()?;
    s.composers.get(&(p.pi, crate::message_box::COMMENT_PLACE.to_string()))
}

/// Whether a comment holding his words is being written, so a new one cannot take its box: then its box shows (a draft
/// closed opens again) and the notice says to send or cancel it first.
fn begun_elsewhere(s: &mut App) -> bool {
    if !writing(s).is_some_and(|w| !w.text.trim().is_empty()) {
        return false;
    }
    s.notes.shut = false;
    s.notice = Some("finish or cancel the comment first".into());
    true
}

/// The comment box opened at passage P, holding NOTE, with the keyboard, in the pane P's lines stand in now
/// ([`Picked::pane`]); a comment box open before goes.
pub fn write(s: &mut App, mut p: Picked, note: &str) {
    unwrite(s);
    p.pane = pane_of(s, p.anchor.as_deref());
    let mut w = TextEdit::new(note);
    w.focused = true;
    s.box_said.remove(&(p.pi, crate::message_box::COMMENT_PLACE.to_string()));
    s.composers.insert((p.pi, crate::message_box::COMMENT_PLACE.into()), w);
    s.notes.picked = Some(p);
}

/// The comment box closed and its text gone (Cancel, or the comment sent); its cleaning forgotten with it, so an
/// answer still out lands on no later comment (changed.rs `sent`).
pub fn unwrite(s: &mut App) {
    let boxes: Vec<(usize, String)> = s.composers.keys().filter(|(_, place)| place == crate::message_box::COMMENT_PLACE).cloned().collect();
    for key in &boxes {
        crate::changed::sent(s, key);
    }
    s.composers.retain(|(_, place), _| place != crate::message_box::COMMENT_PLACE);
    s.notes.picked = None;
    s.notes.shut = false;
}

/// A thread post's element key `post-N-221-5` (threads.rs `post_body`): its ticket's id and its number.
fn post_key(k: &str) -> Option<(String, u32)> {
    let (id, n) = k.strip_prefix("post-")?.rsplit_once('-')?;
    is_place(id).then_some(())?;
    Some((id.to_string(), n.parse().ok()?))
}

/// How post N of ticket ID is named in a quote: `N-221.p5, docket-edge's message at 10:24Z`, its time in UTC as the
/// agent reads every time (`when::utc`; paper-research N-1446 p2361).
fn post_label(s: &App, pi: usize, id: &str, n: u32) -> String {
    let post = s.projects.get(pi).and_then(|p| p.live.tickets.values().find(|v| v.id == id)).and_then(|v| v.posts.iter().find(|q| q.n == n));
    match post {
        Some(q) if q.by == "owner" => format!("{id}.p{n}, your own message at {}", crate::when::utc(&q.at.to_string())),
        Some(q) => format!("{id}.p{n}, {}'s message at {}", q.by, crate::when::utc(&q.at.to_string())),
        None => format!("{id}.p{n}"),
    }
}

/// A ticket's id or a post's, as `follows` takes them: `N-221`, `N-221.p5`.
fn is_place(t: &str) -> bool {
    let (id, post) = match t.split_once('.') {
        Some((id, p)) => (id, Some(p)),
        None => (t, None),
    };
    let ok_id = id.split_once('-').is_some_and(|(l, n)| !l.is_empty() && l.chars().all(|c| c.is_ascii_uppercase()) && !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    ok_id && post.is_none_or(|p| p.strip_prefix('p').is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())))
}

fn secs(at: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(at).map(|t| t.timestamp()).unwrap_or(0)
}

/// What a reply begun on SOURCE follows (`Request::Say`'s `follows`, docket-core 27f8638): a post or a ticket as it
/// is (a thread's line); an agent's session message (`AGENT@OFFSET`) the post its turn made on the ticket of the
/// delivery that turn answered (the first by it from that message to the next delivery), else that ticket. None: a
/// mail, a question, an entry not loaded, a turn no ticket gave.
pub fn follows_of(p: &Proj, source: &str) -> Option<String> {
    if is_place(source) {
        return Some(source.to_string());
    }
    let (agent, offset) = source.rsplit_once('@')?;
    let offset: u64 = offset.parse().ok()?;
    let c = p.convs.get(agent)?;
    let i = c.entries.iter().position(|e| e.offset == offset)?;
    let given = |e: &coord_types::Entry| (e.kind == "user" || e.kind == "mail") && e.text.starts_with("<channel ");
    let ticket = c.entries[..=i].iter().rev().filter(|e| given(e)).find_map(|e| channel_attr(&e.text, "ticket"))?;
    let (from, to) = (secs(&c.entries[i].at), c.entries[i + 1..].iter().find(|e| given(e)).map(|e| secs(&e.at)));
    let post = p.live.tickets.values().find(|v| v.id == ticket).and_then(|v| {
        v.posts.iter().find(|q| {
            let at = secs(&q.at.to_string());
            q.by == agent && at >= from && to.is_none_or(|t| at <= t)
        })
    });
    Some(match post {
        Some(q) => format!("{ticket}.p{}", q.n),
        None => ticket,
    })
}

/// Attribute NAME of the `<channel …>` tag TEXT opens with.
fn channel_attr(text: &str, name: &str) -> Option<String> {
    let tag = format!(" {}", text.strip_prefix("<channel ")?.split('>').next()?);
    let at = tag.find(&format!(" {name}=\""))? + name.len() + 3;
    tag[at..].split('"').next().map(str::to_string)
}

/// A chat entry's key: `e{offset}-{n}` (live.rs).
fn entry_key(k: &str) -> bool {
    k.strip_prefix('e').and_then(|r| r.split_once('-')).is_some_and(|(a, b)| a.parse::<u64>().is_ok() && b.parse::<u32>().is_ok())
}

/// Where entry KEY of AGENT's chat in project PI came from: its source and how the agent is told of it, the time in
/// UTC (`when::utc`).
fn entry_source(s: &App, pi: usize, agent: &str, key: &str) -> (String, String) {
    let offset = key.trim_start_matches('e').split('-').next().unwrap_or("0");
    let entry = s.projects.get(pi).and_then(|p| p.convs.get(agent)).and_then(|c| c.entry(key));
    let label = match entry {
        Some(e) => match e.kind.as_str() {
            "assistant" => format!("your message at {}", crate::when::utc(&e.at)),
            "tool" => format!("your tool call at {}", crate::when::utc(&e.at)),
            "result" => format!("a tool result at {}", crate::when::utc(&e.at)),
            _ => format!("the message at {}", crate::when::utc(&e.at)),
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

/// COMMENTS as data on the post that carries them worded ([`compose`]), for P's coordd from
/// [`coord_types::COMMENTS_PROTOCOL`]; none before it, which keeps only the words.
pub fn data(p: &Proj, comments: &[Comment]) -> Vec<coord_types::Comment> {
    if p.protocol < coord_types::COMMENTS_PROTOCOL {
        return Vec::new();
    }
    let anchor = |c: &Comment| coord_types::Anchor { source: c.source.clone(), label: c.label.clone(), quote: c.quote.clone(), at: c.at.iter().map(|r| [r.start, r.end]).collect() };
    comments.iter().map(|c| coord_types::Comment { target: anchor(c), body: c.note.trim().to_string(), ..Default::default() }).collect()
}

/// A comment's mark: its passage's ranges in the Markdown drawn, and its highlight.
pub type Mark = (Vec<Range<usize>>, Highlight);

/// Where a part of a source's words stands among its parts (post.rs `message_part`: an asking post's lines between its
/// questions, his reply after the line it quotes): its body's id, as a selection's path names it, and the words of the
/// same source drawn before it, as drawn (links and labels made).
#[derive(Clone, Copy, Default)]
pub struct Here<'a> {
    pub body: Option<&'a str>,
    pub earlier: &'a str,
}

/// The highlights of the comments on SOURCE drawn whole, as [`marks_in`].
pub fn marks(s: &App, p: &Proj, source: &str, src: &str, k: &Kit) -> Vec<Mark> {
    marks_in(s, p, source, src, Here::default(), k)
}

/// The highlights of the comments on SOURCE (an entry `AGENT@OFFSET`, or a mail or question id), whose Markdown is
/// SRC, a part of its words drawn HERE: the passage being commented on and the comments sent. Each, the one being
/// written too, is marked at its stored place while SRC still holds its words there; else at its passage found by its
/// text, the match nearest that place (`passage::place`; consult-3, N-1477 p10), as a thread's post's are; else not at
/// all, never on other words. By its words, a passage is lit once in a source drawn in parts: the one being written
/// on in the part he selected it in, a sent one in the first part that holds it (proto-window, N-1477 p8). SRC is the
/// words as drawn, links and labels made (N-1271), so a place stored on an old mail before a path in it became a link
/// no longer holds the passage.
pub fn marks_in(s: &App, p: &Proj, source: &str, src: &str, here: Here, k: &Kit) -> Vec<Mark> {
    let placed = lights(src, here.earlier);
    let new = new_at(s, source, src, here);
    // the passage whose box is open (its mark pressed, or a comment being written on it) is lit as the current one
    let opened = s.notes.open.as_deref().and_then(|key| passage(s, key));
    let current = |quote: &str, at: &[Range<usize>]| opened.as_ref().is_some_and(|o| is_on(o, source, quote)) || new.as_ref().is_some_and(|n| meets(n, at));
    let accent = k.c.bg_accent.alpha(0.28);
    let mut all: Vec<(Vec<Range<usize>>, String, Highlight)> = Vec::new();
    for (m, j, q) in p.live.mail.quotes(source) {
        let place: Vec<Range<usize>> = q.at.iter().map(|r| r[0]..r[1]).collect();
        if let Some(at) = placed(&place, &q.text) {
            let h = Highlight::new().bg(k.c.text_accent.alpha(0.14)).current_match(current(&q.text, &at)).key(sent_key(&m.id, j));
            all.push((at, q.text.clone(), h));
        }
    }
    // his comments sent on a thread's post stay at their passages (passage.rs), as a sent mail's do
    for (j, quote, at) in crate::passage::places(s, p, source, src, here.earlier) {
        let h = Highlight::new().bg(k.c.text_accent.alpha(0.14)).current_match(current(&quote, &at)).key(format!("cp-{source}-{j}"));
        all.push((at, quote, h));
    }
    // his comments held for a place's next message stay lit at their passages too, their box saying where they wait
    // (the owner, N-1790 part 3)
    for (_, _, c, key) in held_on(s, source) {
        if let Some(at) = placed(&c.at, &c.quote) {
            let h = Highlight::new().bg(k.c.text_accent.alpha(0.14)).current_match(current(&c.quote, &at)).key(key);
            all.push((at, c.quote.clone(), h));
        }
    }
    // the passage being written on, lit where he selected it (I-1476); a place that did not hold its words (a
    // selection's ranges inside a block, links made since) lit nothing before, and its box went to the composer at the
    // foot of the page (his N-1104 p706). After the sent ones: a passage already commented on there keeps its mark,
    // lit as the current one, and the box opens at it
    if let (Some(pk), Some(at)) = (s.notes.picked.as_ref(), new) {
        all.push((at, pk.quote.clone(), Highlight::current().bg(accent).key(NEW)));
    }
    // one mark for a passage at one place however many comments are on it: the first stands for them all, its box
    // lists each
    let mut seen: Vec<(String, Vec<Range<usize>>)> = Vec::new();
    let mut out = Vec::new();
    for (at, quote, h) in all {
        let q = flat(&quote);
        if !seen.iter().any(|(sq, sa)| *sq == q && meets(sa, &at)) {
            seen.push((q, at.clone()));
            out.push((at, h));
        }
    }
    out
}

/// Whether two passages' places share a byte.
fn meets(a: &[Range<usize>], b: &[Range<usize>]) -> bool {
    a.iter().any(|x| b.iter().any(|y| x.start < y.end && y.start < x.end))
}

/// Where a comment's passage (its stored place AT, its words QUOTE) is lit in SRC, a part of its source drawn after
/// the words EARLIER: at AT while SRC holds its words there; else, unless EARLIER holds them (lit there), where its
/// words stand nearest AT ([`marks_in`]).
fn lights<'a>(src: &'a str, earlier: &'a str) -> impl Fn(&[Range<usize>], &str) -> Option<Vec<Range<usize>>> + 'a {
    move |at: &[Range<usize>], quote: &str| {
        let words = |t: &str| t.split_whitespace().collect::<String>();
        if !at.is_empty() && at.iter().map(|r| src.get(r.clone())).collect::<Option<String>>().is_some_and(|t| words(&t) == words(quote)) {
            return Some(at.to_vec());
        }
        if !earlier.is_empty() && crate::passage::place(earlier, quote, None).is_some() {
            return None;
        }
        crate::passage::place(src, quote, at.first().map(|r| r.start))
    }
}

/// Where the passage being written on is lit in SRC, part HERE of SOURCE's words: by its words only in the part he
/// selected it in (`Picked::block`), the one match nearest where he selected ([`marks_in`]).
fn new_at(s: &App, source: &str, src: &str, here: Here) -> Option<Vec<Range<usize>>> {
    let pk = s.notes.picked.as_ref().filter(|pk| pk.source == source && ours(s))?;
    match (pk.block.as_deref(), here.body) {
        (Some(b), Some(id)) if b != id => None,
        (Some(_), Some(_)) => lights(src, "")(&pk.at, &pk.quote),
        _ => lights(src, here.earlier)(&pk.at, &pk.quote),
    }
}

/// Everything [`marks`] reads for SOURCE, into H: a memoised row that shows SOURCE is built again when it changes.
pub fn marks_version(s: &App, p: &Proj, source: &str, h: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    let mut any = false;
    for (m, j, q) in p.live.mail.quotes(source) {
        (&m.id, j, &q.at, &q.text).hash(h);
        any = true;
    }
    if let Some(pk) = s.notes.picked.as_ref().filter(|pk| pk.source == source) {
        (&pk.at, &pk.quote, &pk.block, ours(s)).hash(h);
        any = true;
    }
    // the room the comment box stands in, in the flow
    if shown_on(s).as_deref() == Some(source) {
        rooms(s).hash(h);
    }
    for c in crate::passage::on_drawn(s, p, source) {
        (&c.quote, c.his).hash(h);
        any = true;
    }
    if any {
        s.notes.open.hash(h);
    }
}

/// What a part showing threads reads (a question's card, a mail in a chat with its replies), into V: the project's
/// mail and agents (whom a reply can go to), the notes, and the minute ("asked 5 m ago"). A part with a reply box
/// open is built each frame instead ([`holds_box`]).
pub fn thread_version(s: &App, p: &Proj, v: &mut crate::version::Version) {
    let n = &s.notes;
    v.add((&p.id, s.cur, p.live.mail.rev, chrono::Utc::now().timestamp() / 60));
    v.debug(&n.picked).add((&n.open, n.show_all, n.shut, rooms(s))).debug(&n.replying).debug(&n.read);
    v.debug(&n.left_out).debug(&n.added).debug(&n.adding).debug(&n.mention).debug(&n.kept);
    for a in &p.live.agents {
        v.add(&a.name);
    }
}

/// A press on comment mark KEY (one sent in a mail, one sent on a thread's post): its passage's box opens there, or
/// closes when it is open (0.2 line C15: "click … a marked passage and reply in the one box"). On the passage being
/// written on, the same: its box closed with his words kept opens again, open it closes.
pub fn highlight_pressed(s: &mut App, key: &str) {
    if key != NEW {
        open_mark(s, key);
    } else if s.notes.shut {
        reopen(s);
    } else {
        close_mark(s);
    }
}

/// The comment box closed with his words kept, open again at its passage, with the keyboard.
fn reopen(s: &mut App) {
    s.notes.shut = false;
    if let Some(p) = s.notes.picked.as_ref()
        && let Some(e) = s.composers.get_mut(&(p.pi, crate::message_box::COMMENT_PLACE.to_string()))
    {
        e.focused = true;
    }
}

/// Whether KEY is a comment's mark: a mail's quote (`cq-`), a thread post's comment (`cp-`, passage.rs), a comment
/// held for a place's next message (`ch-`, [`held_on`]); not the passage being written on.
fn is_mark(key: &str) -> bool {
    key.starts_with("cq-") || key.starts_with("cp-") || key.starts_with("ch-")
}

/// The comments held on SOURCE for a place's next message ([`Notes::held`]), in one order (project, place, as added):
/// each with its project, its place and its mark's key, `ch-SOURCE-J`.
fn held_on<'a>(s: &'a App, source: &str) -> Vec<(usize, &'a str, &'a Comment, String)> {
    let mut places: Vec<(&'a (usize, String), &'a Vec<Comment>)> = s.notes.held.iter().collect();
    places.sort_by(|a, b| a.0.cmp(b.0));
    places
        .into_iter()
        .flat_map(|((pi, place), l)| l.iter().map(move |c| (*pi, place.as_str(), c)))
        .filter(|(_, _, c)| c.source == source)
        .enumerate()
        .map(|(j, (pi, place, c))| (pi, place, c, format!("ch-{source}-{j}")))
        .collect()
}

/// Where a comment held for PLACE of project PI waits, as its box says it.
fn held_state(s: &App, pi: usize, place: &str) -> String {
    match place.strip_prefix("thread:") {
        Some(id) => format!("held · goes with your next on {id}"),
        None if s.projects.get(pi).is_some_and(|p| p.live.mail.get(place).is_some()) => format!("held · goes with your reply in {place}"),
        None => format!("held · goes with your next to {place}"),
    }
}

/// A passage his comments are on, as a mark stands for it: the project and the place a comment on it goes with (an
/// agent's chat, `thread:ID`, a mail's thread), its source, the passage, its place in the source and how it is named.
#[derive(Clone, Debug, PartialEq)]
pub struct Passage {
    pub pi: usize,
    pub target: String,
    pub source: String,
    pub quote: String,
    pub at: Vec<Range<usize>>,
    pub label: String,
}

/// A passage's words as a mark tells it from another: the same words, however they wrap.
fn flat(t: &str) -> String {
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether P is the passage QUOTE of SOURCE.
fn is_on(p: &Passage, source: &str, quote: &str) -> bool {
    p.source == source && flat(&p.quote) == flat(quote)
}

/// The passage comment mark KEY stands on, and where a new comment on it goes: where the comment the mark is went.
pub fn passage(s: &App, key: &str) -> Option<Passage> {
    if !is_mark(key) {
        return None;
    }
    if key.starts_with("cq-") {
        for (pi, p) in s.projects.iter().enumerate() {
            for m in p.live.mail.iter().filter(|m| m.from == "owner") {
                for (j, q) in m.quotes.iter().enumerate() {
                    if sent_key(&m.id, j) == key {
                        let at = q.at.iter().map(|r| r[0]..r[1]).collect();
                        return Some(Passage { pi, target: p.live.mail.top(&m.id), source: q.source.clone(), quote: q.text.clone(), at, label: label_of(s, pi, &q.source) });
                    }
                }
            }
        }
        return None;
    }
    // one held for a place's next message: `ch-SOURCE-J` ([`held_on`])
    if let Some((source, _)) = key.strip_prefix("ch-").and_then(|r| r.rsplit_once('-')) {
        return held_on(s, source).into_iter().find(|h| h.3 == key).map(|(pi, place, c, _)| Passage { pi, target: place.to_string(), source: c.source.clone(), quote: c.quote.clone(), at: c.at.clone(), label: c.label.clone() });
    }
    // a thread post's: `cp-N-7.p2-J`, J its place among his comments on the post (passage.rs `on`)
    let (source, j) = key.strip_prefix("cp-")?.rsplit_once('-')?;
    let j: usize = j.parse().ok()?;
    let (id, n) = source.split_once(".p")?;
    let n: u32 = n.parse().ok()?;
    let order = std::iter::once(s.cur).chain((0..s.projects.len()).filter(|i| *i != s.cur));
    order.filter_map(|pi| Some((pi, s.projects.get(pi)?))).find_map(|(pi, p)| {
        let c = crate::passage::on(s, p, source).into_iter().nth(j)?;
        Some(Passage { pi, target: format!("thread:{id}"), source: source.to_string(), quote: c.quote, at: Vec::new(), label: post_label(s, pi, id, n) })
    })
}

/// How SOURCE is named in a comment's quote, as [`comment_on`] names it: a post (`N-221.p5, lead's message at
/// 10:24Z`), an agent's session message (`your message at 10:24Z`), else its id (a mail's).
fn label_of(s: &App, pi: usize, source: &str) -> String {
    if is_place(source)
        && let Some((id, n)) = source.split_once(".p")
        && let Ok(n) = n.parse::<u32>()
    {
        return post_label(s, pi, id, n);
    }
    if let Some((agent, offset)) = source.rsplit_once('@') {
        let key = s.projects.get(pi).and_then(|p| p.convs.get(agent)).and_then(|c| c.entries.iter().position(|e| e.offset.to_string() == offset).and_then(|i| c.keys.get(i).cloned()));
        return key.map(|k| entry_source(s, pi, agent, &k).1).unwrap_or_default();
    }
    source.to_string()
}

/// Mark KEY's box opened at it: the comments on its passage and the one message box, his new comment on the same
/// passage going where they went; pressed again, it closes. A comment begun on another passage stays, its box shown:
/// he sends or cancels it first.
pub fn open_mark(s: &mut App, key: &str) {
    let Some(at) = passage(s, key) else { return };
    if s.notes.open.as_deref().and_then(|k| passage(s, k)).is_some_and(|o| is_on(&o, &at.source, &at.quote)) {
        close_mark(s);
        return;
    }
    let here = s.notes.picked.as_ref().is_some_and(|p| p.source == at.source && flat(&p.quote) == flat(&at.quote));
    if here {
        reopen(s);
    } else if begun_elsewhere(s) {
        return;
    } else {
        write(s, Picked { target: at.target, source: at.source, label: at.label, quote: at.quote, at: at.at, pi: at.pi, anchor: None, block: None, pane: None, texts: Vec::new() }, "");
    }
    s.notes.open = Some(key.to_string());
}

/// The comment box closed (Esc, Close, its mark pressed again): words he wrote in it stay as the box's draft, its
/// passage lit, and a press on the passage opens the box again with them; nothing goes until Add comment (N-1396
/// p329). An empty one goes.
pub fn close_mark(s: &mut App) {
    if writing(s).is_some_and(|w| !w.text.trim().is_empty()) {
        s.notes.shut = true;
        if let Some(p) = s.notes.picked.as_ref()
            && let Some(e) = s.composers.get_mut(&(p.pi, crate::message_box::COMMENT_PLACE.to_string()))
        {
            e.focused = false;
        }
    } else {
        unwrite(s);
    }
    s.notes.open = None;
}

/// Esc in the comment box closes it ([`close_mark`]).
pub(crate) fn comment_key(s: &mut App, target: &crate::message_box::Target, ev: &rune::KeyEvent) -> bool {
    if !matches!(target, crate::message_box::Target::Comment { .. }) || !matches!(ev.key, rune::Key::Escape) {
        return false;
    }
    close_mark(s);
    true
}

/// One comment in its passage's box: its mark's key, his note, what became of it, the answer to it.
struct Said {
    key: String,
    note: String,
    state: String,
    answer: Option<Answered>,
}

/// The answer to a sent comment: by whom, its words, and the post it is (a mail's answer: none to go to).
struct Answered {
    by: String,
    words: String,
    at: Option<String>,
}

/// The comments on passage QUOTE of SOURCE, oldest first: those sent on a thread's post (passage.rs), then those sent
/// in mail.
fn trail(s: &App, source: &str, quote: &str) -> Vec<Said> {
    let q = flat(quote);
    let mut out = Vec::new();
    for p in &s.projects {
        let list = crate::passage::on(s, p, source);
        let id = source.split_once(".p").map(|(t, _)| t).unwrap_or_default();
        let post = |n: u32| p.live.tickets.values().find(|v| v.id == id).and_then(|v| v.posts.iter().find(|x| x.n == n));
        for (j, c) in list.iter().enumerate().filter(|(_, c)| flat(&c.quote) == q) {
            // a message's answer stands under its last comment on the post (passage.rs); in the box, under each
            let answer = c.answer.clone().or_else(|| list.iter().find(|x| x.his == c.his && x.answer.is_some()).and_then(|x| x.answer.clone()));
            let when = post(c.his).map(|x| format!(" at {}", crate::when::at(&x.at.to_string()))).unwrap_or_default();
            out.push(Said {
                key: format!("cp-{source}-{j}"),
                note: c.note.clone(),
                state: format!("sent in #{}{when}", c.his),
                answer: answer.map(|(n, words)| Answered { by: post(n).map(|x| x.by.clone()).unwrap_or_default(), words, at: Some(format!("{id}.p{n}")) }),
            });
        }
        let mut mails: Vec<(&MailView, usize, &Quote)> = p.live.mail.quotes(source).filter(|(m, _, x)| m.from == "owner" && flat(&x.text) == q).collect();
        mails.sort_by(|a, b| a.0.at.cmp(&b.0.at));
        for (m, j, x) in mails {
            let note = crate::passage::pairs(&m.body).into_iter().find(|c| flat(&c.quote) == flat(&x.text)).map(|c| c.note).unwrap_or_default();
            let answer = p.live.mail.replies(&m.id).into_iter().find(|r| r.from != "owner").map(|r| Answered { by: r.from.clone(), words: crate::passage::said(&r.body), at: None });
            out.push(Said { key: sent_key(&m.id, j), note, state: format!("sent in {} at {}", m.id, crate::when::at(&m.at)), answer });
        }
    }
    for (pi, place, c, key) in held_on(s, source).into_iter().filter(|h| flat(&h.2.quote) == q) {
        out.push(Said { key, note: c.note.clone(), state: held_state(s, pi, place), answer: None });
    }
    out
}

/// Comment C in its passage's box: what became of it, his note, and the answer's first line (a press shows it whole
/// here, with a link to where it stands).
fn said_row(s: &App, c: &Said, k: &Kit) -> El {
    let mut r = col()
        .key(format!("said-{}", c.key))
        .w_full()
        .gap(2.0)
        .child(text(c.state.clone()).font(k.ty.small).color(k.c.text_faint).wrap())
        .child(text(c.note.trim().to_string()).font(k.ty.body).color(k.c.text_base).wrap());
    match &c.answer {
        Some(a) => {
            let key = format!("box|{}", c.key);
            let open = s.passage_open.contains(&key);
            let first = a.words.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
            let shown = if open { first.to_string() } else { one_line(first, 160) };
            r = r.child(
                k.interactive(text(format!("{}: {shown}", a.by)).font(k.ty.small).color(k.c.text_muted).wrap())
                    .help(if open { "Show less" } else { "Show the whole answer here" })
                    .on_press(move |s: &mut App| {
                        if !s.passage_open.remove(&key) {
                            s.passage_open.insert(key.clone());
                        }
                    }),
            );
            if open {
                let rest: String = a.words.lines().skip_while(|l| l.trim().is_empty()).skip(1).collect::<Vec<_>>().join("\n");
                if !rest.trim().is_empty() {
                    r = r.child(rune_kit::markdown::Markdown::new(rest.trim().to_string()).selectable(true).build(k));
                }
                if let Some(to) = a.at.clone() {
                    r = r.child(k.interactive(text("in the thread →").font(k.ty.small).color(k.c.text_faint)).help("Go to the answer").on_press(move |s: &mut App| {
                        close_mark(s);
                        crate::mentions::open(s, &to);
                    }));
                }
            }
        }
        None => r = r.child(text("no answer yet").font(k.ty.small).color(k.c.text_faint)),
    }
    r
}

/// The comment box's Send, `Add comment`. A comment on a message is kept with the message he is writing at the place
/// it goes to ([`Notes::held`]) and sends nothing: that place's Send takes all of them, each its passage quoted then
/// his note, with what its box holds, as one message (his walk item 11, paper-research N-1446 p3521; before, from
/// d90b000c, each went at its press). One that could not go (a tag naming no agent) is said under this box and kept
/// in it. A comment on a file is held the same way, for its reader's chat (or the agent its @NAME tags): nothing goes
/// at the press (the owner, N-1790 part 3), its passage stays lit and its box says where it waits.
pub fn send(s: &mut App) {
    let Some(note) = writing(s).map(|w| w.text.trim().to_string()).filter(|n| !n.is_empty()) else { return };
    let Some(p) = aimed(s, &note) else { return };
    let here = (p.pi, crate::message_box::COMMENT_PLACE.to_string());
    s.box_said.remove(&here);
    let c = Comment { source: p.source, label: p.label, quote: p.quote, note, at: p.at };
    // what would refuse it at its place's Send (a tag naming no agent) is said now, his words kept in this box
    if let Err(e) = alone(s, p.pi, &p.target, &c) {
        if !e.is_empty() {
            s.box_said.insert(here, Err(e));
        }
        return;
    }
    s.words.unclean.remove(&here);
    s.notes.held.entry((p.pi, p.target)).or_default().push(c);
    unwrite(s);
    s.notes.open = None;
    // his words on disk at the press (N-1396 p51)
    crate::state::write_now(s);
}

/// The comments held for PLACE of project PI, which its next Send takes ([`Notes::held`]).
pub fn held<'a>(s: &'a App, pi: usize, place: &str) -> &'a [Comment] {
    s.notes.held.get(&(pi, place.to_string())).map(Vec::as_slice).unwrap_or_default()
}

/// Over the box of PLACE in project PI, the comments its Send takes with what it holds: each its passage's first words
/// and his note, × taking it out.
pub fn held_lines(s: &App, pi: usize, place: &str, k: &Kit) -> Option<El> {
    let list = held(s, pi, place);
    if list.is_empty() {
        return None;
    }
    let rows = list.iter().enumerate().map(|(j, c)| {
        let key = (pi, place.to_string());
        row().key(format!("held-{pi}-{place}-{j}")).w_full().gap(8.0).center_y()
            .child(col().grow(1.0).min_w(0.0).gap(2.0)
                .child(text(format!("› {}", one_line(c.quote.lines().next().unwrap_or(""), 90))).font(k.ty.small).color(k.c.text_faint))
                .child(text(c.note.trim().to_string()).font(k.ty.small).color(k.c.text_base).wrap()))
            .child(Button::new("×").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
                if let Some(l) = s.notes.held.get_mut(&key) {
                    if j < l.len() {
                        l.remove(j);
                    }
                    if l.is_empty() {
                        s.notes.held.remove(&key);
                    }
                }
                crate::state::write_now(s);
            }).build(k))
    });
    let n = list.len();
    Some(col().key(format!("held-{pi}-{place}")).w_full().gap(4.0)
        .child(text(format!("{n} comment{} go{} with your next Send", if n == 1 { "" } else { "s" }, if n == 1 { "es" } else { "" })).font(k.ty.small).color(k.c.text_muted))
        .children(rows))
}

/// The request comment C alone makes at TARGET's place in project PI: a reply on the thread (`thread:ID`), a message
/// to the chat's agent, or a mail in a mail's thread.
pub(crate) fn alone(s: &App, pi: usize, target: &str, c: &Comment) -> Result<Request, String> {
    let one = std::slice::from_ref(c);
    if let Some(id) = target.strip_prefix("thread:") {
        return crate::threads::reply_with(s, pi, id, one, "");
    }
    let p = s.projects.get(pi).ok_or_else(|| "not sent: the project is not open".to_string())?;
    if p.live.mail.get(target).is_some() {
        return reply_request(s, pi, target, "", one, "");
    }
    s.request_for(pi, target, "", one)
}

/// Comment C, sent from the comment box to TARGET's place in project PI and taken back before the engine had it
/// (outgoing.rs `words_back`): the box open at its passage again, his note in it, before what he has begun there on
/// the same passage. False, his words left to the box of its place, while he writes a comment on another passage.
pub fn back(s: &mut App, pi: usize, target: &str, c: Comment) -> bool {
    let here = s.notes.picked.as_ref().is_some_and(|p| p.source == c.source && flat(&p.quote) == flat(&c.quote));
    let begun = writing(s).is_some_and(|w| !w.text.trim().is_empty());
    match (begun, here) {
        (true, false) => return false,
        (true, true) => {
            let at = s.notes.picked.as_ref().map(|p| p.pi).unwrap_or(pi);
            crate::outgoing::before(s, (at, crate::message_box::COMMENT_PLACE.into()), &c.note);
        }
        (false, _) => {
            write(s, Picked { target: target.to_string(), source: c.source, label: c.label, quote: c.quote, at: c.at, pi, anchor: None, block: None, pane: None, texts: Vec::new() }, &c.note);
            // back where he left it, the keyboard where it is
            if let Some(e) = s.composers.get_mut(&(pi, crate::message_box::COMMENT_PLACE.to_string())) {
                e.focused = false;
            }
        }
    }
    s.notes.shut = false;
    s.notes.open = None;
    true
}

/// The comments a window from before Add comment sent at once kept waiting for his next message, per place (ui.json's
/// `comments`; N-1396 p329), each place's into its box in the project shown (where that window sent them from), quoted
/// as they would have gone, before what he typed there: nothing he wrote is lost, and nothing goes without his Send
/// (consult-3, p331).
pub fn from_before(s: &mut App, kept: Vec<(String, Vec<Comment>)>) {
    for (target, list) in kept.into_iter().filter(|(_, l)| !l.is_empty()) {
        let (words, _) = compose(&list, "");
        crate::outgoing::before(s, (s.cur, target), &words);
    }
}

/// The element a surface leaves for the comment box in its flow, the layer pinning the box in it ([`room`]).
pub const ROOM: &str = "cm-room";
/// The box's width beside the lines, and the narrowest it takes there: a pane with less room right of its lines has
/// the box in the flow.
const BOX_W: f32 = 380.0;
const BESIDE_MIN: f32 = 300.0;
/// Between the lines and the box beside them; kept free at the pane's right edge (its scroll bar); above and below
/// the box in its room.
const GAP: f32 = 24.0;
const EDGE: f32 = 16.0;
const AIR: f32 = 6.0;

/// The comment box, in the window's LAYER, at a passage's mark: the passage, the comments on it oldest first, each with
/// its answer, and (a comment being written) the one message box, the new comment going where they went. It covers
/// none of his text (paper-research, N-1477 p9): where the lines' pane is wide enough it stands beside them, in the
/// pane's right margin at the passage's first line, as a document's margin comments do; else in the flow, in the room
/// its surface leaves after the passage (`room`), the lines after it moved down under it. With no passage lit, at the
/// post, message, entry, line or file he selected it in, its room at its end (`room_at`); never at a composer, the
/// foot of the page (I-1476; his N-1104 p706). It scrolls with the lines.
/// Where the comment box goes in this pass, from where the last one laid out its passage, its pane, its room and
/// itself: beside the lines in their pane's margin, else in its room in the flow (I-1476; paper-research N-1477 p9).
/// Before the surfaces are drawn (lib.rs `view`), so the room they leave is the one the box goes in, in the same pass:
/// the pass it opens, the room is drawn; the next, the box is pinned in it; the next, the room is as tall as the box,
/// all in the one commit, before a frame is shown (rune re-runs a view while what it read moves, two passes more;
/// consult-3 N-1477 p25).
pub fn decide(s: &App, cx: &mut rune::Cx<App>) {
    let at = |key: &str| cx.window_layout(key);
    s.notes.spot.set(None);
    s.notes.roomed.set(false);
    // closed with his words kept: its passage stays lit, a press on it opens the box again (`close_mark`)
    let picked = s.notes.picked.as_ref().filter(|_| !s.notes.shut);
    let opened = s.notes.open.as_deref().and_then(|key| passage(s, key));
    let Some((source, quote)) = picked.map(|p| (p.source.clone(), p.quote.clone())).or_else(|| opened.map(|o| (o.source, o.quote))) else { return };
    let said = trail(s, &source, &quote);
    // in the split, only what is drawn in the box's pane: a post drawn in two panes is read first in the one drawn
    // first, and the box stands at it only in its own (consult-3 N-1477 p46)
    let own = box_pane(s).and_then(|j| at(&crate::panes::key(j)));
    let mine = |key: &str| at(key).filter(|r| own.is_none_or(|p| p[0] - 0.5 <= r[0] && r[0] < p[0] + p[2]));
    // the passage's mark (one stands for all its comments); else the element he selected it in, else the one its
    // source names; on another page, in another pane, or in rows not built, it waits, its words kept, until he is
    // back there
    let lit = mine(NEW).or_else(|| s.notes.open.as_deref().and_then(&mine)).or_else(|| said.iter().find_map(|c| mine(&c.key)));
    let anchor = picked.and_then(|p| p.anchor.as_deref()).and_then(&mine);
    let held = holder(&source).and_then(|key| mine(&key));
    // the column its lines stand in: the element he selected in, else a post's own (with no element: a mark opened
    // again, a scene), not its row in a thread's list, which has the same key and the pane's width
    let column = anchor.or_else(|| docket_machine::post_ref(&source).and_then(|(id, n)| mine(&format!("hover-{id}-{n}/post-{id}-{n}")))).or(held);
    // all read each pass, so none is read first in the pass that needs it
    let room = mine(ROOM);
    let panes: Vec<[f32; 4]> = panes(picked, &source).into_iter().filter_map(|key| mine(&key)).collect();
    if let Some(b) = at("cm-box") {
        s.notes.tall.set(b[3]);
    }
    let Some(r) = lit.or(anchor).or(held) else {
        // none laid out yet (the pass it opens; another page): the room drawn where it is lit, else at the end of what
        // he selected in, so the next pass has it to pin the box in
        s.notes.inline.set(true);
        s.notes.lit.set(false);
        return;
    };
    s.notes.lit.set(lit.is_some());
    // the lines' right edge: their column's
    let right = [Some(r), column].into_iter().flatten().map(|x| x[0] + x[2]).fold(f32::MIN, f32::max);
    let pane = panes.into_iter().find(|p| p[0] <= r[0] && r[0] < p[0] + p[2]);
    let beside = pane.map(|p| p[0] + p[2] - EDGE - right - GAP).filter(|w| *w >= BESIDE_MIN).map(|w| w.min(BOX_W));
    s.notes.inline.set(beside.is_none());
    // only inside its pane, never over what is outside it (the thread's title, the bar, the next pane; proto-window
    // N-1477 p29): scrolled out, it is not drawn, his words kept, and back when its lines are
    let inside = |y: f32, h: f32| pane.is_none_or(|p| y >= p[1] - 0.5 && y + h <= p[1] + p[3] + 0.5);
    let h = match s.notes.tall.get() {
        t if t > 0.0 => t,
        _ => tall(s),
    };
    s.notes.spot.set(match (beside, room) {
        // in the margin at the passage's first line, moved up as far as the pane's foot needs while the passage shows
        (Some(w), _) => {
            let shows = pane.is_none_or(|p| r[1] + r[3] > p[1] && r[1] < p[1] + p[3]);
            let y = pane.map_or(r[1], |p| r[1].min(p[1] + p[3] - h).max(p[1]));
            (shows && inside(y, h)).then_some([right + GAP, y, w])
        }
        (None, Some(room)) => inside(room[1], room[3]).then_some([room[0], room[1] + AIR, room[2]]),
        // the pass it goes into the flow, its room not laid out yet: nothing, never over his lines
        (None, None) => None,
    });
}

/// The box shown, by its source and passage (the one being written), else the mark whose box is open.
fn opened(s: &App) -> Option<String> {
    match s.notes.picked.as_ref().filter(|_| !s.notes.shut) {
        Some(pk) => Some(format!("{}\u{1}{}", pk.source, pk.quote)),
        None => s.notes.open.clone(),
    }
}

/// The comment box where [`decide`] put it this pass, in the window's LAYER: none when it put it nowhere.
pub fn layer(s: &App, k: &Kit, layer: &mut Vec<El>) {
    let Some([x, y, w]) = s.notes.spot.get() else { return };
    let picked = s.notes.picked.as_ref().filter(|_| !s.notes.shut);
    let opened = s.notes.open.as_deref().and_then(|key| passage(s, key));
    let Some(quote) = picked.map(|p| p.quote.clone()).or_else(|| opened.map(|o| o.quote)) else { return };
    let source = picked.map(|p| p.source.clone()).or_else(|| shown_on(s)).unwrap_or_default();
    let said = trail(s, &source, &quote);
    let body = box_words(s, picked, &quote, &said, k);
    place(x, y, w, body, k, layer);
}

/// The comment box BODY pinned with its top left at X, Y, W wide. Pinned, not anchored: an anchored overlay flips
/// above its anchor near the window's foot and is kept in the window, over the lines either way. It lands where the
/// lines are at once (the page's layout lands at once too: lib.rs `view`): the layer's springs left it behind them.
fn place(x: f32, y: f32, w: f32, body: El, k: &Kit, layer: &mut Vec<El>) {
    layer.push(k.elevate(
        body.key("cm-box")
            .instant_changes()
            .pin(Some(x), Some(y), None, None)
            .w(w)
            .gap(8.0)
            .padding(12.0)
            .radius(k.radius.md)
            .fill(k.c.surface_raised)
            .border(1.0, k.c.border_focus),
        &k.elevation.floating,
    ));
}

/// The reading panes the comment box's lines may stand in, by key, whose right margin holds it when wide enough: those
/// on the path of the element he selected in (`Picked::anchor`), outermost first; else the ones a post or a file is
/// read in.
fn panes(picked: Option<&Picked>, source: &str) -> Vec<String> {
    let mut out: Vec<String> = picked.and_then(|p| p.anchor.as_deref()).map(|a| a.split('/').filter(|x| pane(x)).map(String::from).collect()).unwrap_or_default();
    if is_file(source) {
        out.push("files-file".into());
    } else if let Some((id, _)) = docket_machine::post_ref(source) {
        out.extend(["ticket-detail".to_string(), format!("with-you-thread-{id}")]);
    }
    out
}

/// Whether the element keyed KEY is a reading pane (its scroll): a thread on the Threads page or in With you, a file,
/// an agent's chat, a stream, a popup, a lens pane's lines, a node's lines on the Map (map/canvas.rs `depth_lines`:
/// the box stays inside its card).
pub fn pane(key: &str) -> bool {
    key == "ticket-detail" || key == "files-file" || key.starts_with("with-you-thread-") || key.starts_with("conv-") || key.starts_with("stream-")
        || key.strip_prefix("popup").and_then(|x| x.strip_suffix("-scroll")).is_some_and(|i| i.parse::<usize>().is_ok())
        || key.starts_with("lens-lines-")
        || key.starts_with("map-layer-lines-")
}

/// The source of the comment box shown, as [`layer`] finds it: the comment being written, its box not closed, else
/// the mark whose box is open.
fn shown_on(s: &App) -> Option<String> {
    match s.notes.picked.as_ref().filter(|_| !s.notes.shut) {
        Some(pk) => Some(pk.source.clone()),
        None => passage(s, s.notes.open.as_deref()?).map(|o| o.source),
    }
}

/// The split's pane (`pane-J`, panes.rs) the comment box's lines stand in: the one it opened in ([`Picked::pane`]),
/// none with no split; with no comment being written (a sent comment's mark open), the focused one.
fn box_pane(s: &App) -> Option<usize> {
    if s.panes.is_empty() {
        return None;
    }
    s.notes.picked.as_ref().map_or(Some(s.focus), |p| p.pane)
}

/// The pane ([`Picked::pane`]) of lines selected in the element at keyed path ANCHOR, as the split stands: the one
/// on its path, none when that is outside the split's panes (a popup, or outside the main area with no split); with
/// no element (a mark opened again: its press focused its pane; a scene), the focused one.
pub(crate) fn pane_of(s: &App, anchor: Option<&str>) -> Option<usize> {
    let Some(a) = anchor else { return Some(if s.panes.is_empty() { 0 } else { s.focus }) };
    if s.panes.is_empty() { a.split('/').any(|k| k == crate::panes::MAIN).then_some(0) } else { a.split('/').find_map(crate::panes::keyed) }
}

/// What the id of a body drawn in the split's pane J ends with (post.rs `message_part`): `-inJ`, so a post drawn in
/// two panes is two bodies; nothing with no split.
pub fn in_split(j: Option<usize>) -> String {
    j.map_or(String::new(), |j| format!("-in{j}"))
}

/// The comment box's pane ([`Picked::pane`]) moved with the split's panes as panes.rs and raw.rs change them (a pane
/// put in before it, closed before it, the split opened or ended; proto-window N-1477 p44): TO takes a pane's place
/// before (0, the page, with no split) to its place now, none for the pane closed. Its own pane closed, it stands in
/// the one focused now, its words kept, drawn when its lines are there. The keys that name its pane move with it: its
/// words' part's (`-inJ` in [`Picked::block`]: post.rs `message_part`, [`in_split`]), and while the split stays its
/// element's (`pane-J` in [`Picked::anchor`]). As the split opens or ends, his element's path changes past its pane:
/// with no split the page is the main area itself, its own key given way to `main` (lib.rs `view`); the anchor then
/// names what is not drawn, and the box stands at its lit passage.
pub fn panes_moved(s: &mut App, to: impl Fn(usize) -> Option<usize>) {
    let split = !s.panes.is_empty();
    let focus = if split { s.focus } else { 0 };
    let Some(p) = s.notes.picked.as_mut() else { return };
    let Some(k) = p.pane.map(|j| to(j).unwrap_or(focus)) else { return };
    let k = split.then_some(k);
    p.pane = Some(k.unwrap_or(0));
    if let (Some(a), Some(k)) = (p.anchor.as_mut(), k) {
        *a = a.split('/').map(|x| if crate::panes::keyed(x).is_some() { crate::panes::key(k) } else { x.to_string() }).collect::<Vec<_>>().join("/");
    }
    if let Some(b) = p.block.as_mut() {
        let bare = b.rsplit_once("-in").filter(|(_, j)| j.parse::<usize>().is_ok()).map_or(b.as_str(), |(b, _)| b);
        *b = format!("{bare}{}", in_split(k));
    }
}

/// The split's pane being drawn (`Notes::drawing`), for rows built later (rune's `virtual_rows`) to be drawn in it
/// ([`in_pane`]).
pub fn pane_drawn(s: &App) -> Option<usize> {
    s.notes.drawing.get()
}

/// F, drawn in the split's pane PANE (where its rows were laid out: [`pane_drawn`]); the pane being drawn put back
/// after it. In a pane other than the focused one, what F draws unread is asked through that pane's project's pipe
/// (texts.rs `drawn_in`).
pub fn in_pane<T>(s: &App, pane: Option<usize>, f: impl FnOnce() -> T) -> T {
    let was = s.notes.drawing.replace(pane);
    let out = match pane.filter(|j| *j != s.focus).and_then(|j| s.panes.get(j)) {
        Some(&(pi, _)) => crate::texts::drawn_in(s, pi, f),
        None => f(),
    };
    s.notes.drawing.set(was);
    out
}

/// Whether the words being drawn stand in the comment box's pane ([`box_pane`]): only there is its passage lit as the
/// one being written ([`new_at`]) and its room drawn ([`cut`], [`room`], [`room_at`]), so a post drawn in two panes of
/// the split (Threads beside With you on its thread) lights one passage and draws one room, the box in it
/// (proto-window N-1477 p29).
fn ours(s: &App) -> bool {
    s.notes.drawing.get() == box_pane(s)
}

/// What the rooms drawn read ([`room`], [`room_at`]), for a memo's version: in the split, whether this is the box's
/// pane too, while a box is shown (else every pane's rows would differ).
fn rooms(s: &App) -> (bool, bool, u32, bool) {
    (s.notes.inline.get(), s.notes.lit.get(), tall(s) as u32, shown_on(s).is_none() || ours(s))
}

/// The room's height: the box's as it was last drawn, to the next 8 points (a body is drawn again only when the box
/// grows a line); before it was, about a box with his first line.
fn tall(s: &App) -> f32 {
    match s.notes.tall.get() {
        t if t > 0.0 => (t / 8.0).ceil() * 8.0 + 2.0 * AIR,
        _ => 160.0,
    }
}

fn room_el(s: &App) -> El {
    // its pane scrolls it into view when the box opens and as it grows (he commented on the last lines in view, his
    // lines push it past the pane's foot), never as he scrolls: the box is drawn only inside its pane ([`decide`])
    rune::el().key(ROOM).w_full().h(tall(s)).on_layout_tx(|s: &mut App, r: [f32; 4], tx: &mut rune::Tx| {
        let now = opened(s).map(|id| (id, r[3].round() as u32));
        if now.is_some() && s.notes.revealed != now {
            s.notes.revealed = now;
            tx.scroll_into_view(ROOM);
        } else {
            tx.live_only();
        }
    })
}

/// The room the comment box stands in, in the flow of SOURCE's words, while their pane is too narrow for it beside
/// them (I-1476; paper-research N-1477 p9): as tall as the box, which [`layer`] pins in it, so it covers nothing and
/// grows as he types. A surface draws it where its words split after the passage ([`cut`], [`split`]).
pub fn room(s: &App, source: &str) -> Option<El> {
    let drawn = s.notes.inline.get() && ours(s) && shown_on(s).as_deref() == Some(source);
    s.notes.roomed.set(s.notes.roomed.get() || drawn);
    drawn.then(|| room_el(s))
}

/// The room at the end of the element keyed KEY (a post, a message, a chat's entry, a line of a run, a file) when the
/// comment box's passage is lit nowhere: KEY being the element he selected it in (`Picked::anchor`), else the one its
/// source names (`holder`). None when its words were split for the room ([`room`], drawn before it in the surface),
/// so in the pass the box opens, its passage not laid out yet, there is one room still (consult-3 N-1477 p30).
pub fn room_at(s: &App, key: &str) -> Option<El> {
    if !s.notes.inline.get() || s.notes.lit.get() || s.notes.roomed.get() || !ours(s) {
        return None;
    }
    let at = match s.notes.picked.as_ref().filter(|_| !s.notes.shut) {
        Some(pk) => pk.anchor.as_deref().and_then(|a| a.rsplit('/').next()).map(String::from).or_else(|| holder(&pk.source)),
        None => holder(&passage(s, s.notes.open.as_deref()?)?.source),
    };
    (at.as_deref() == Some(key)).then(|| room_el(s))
}

/// Where SRC, part HERE of SOURCE's words, splits for the comment box's room ([`room_cut`]), while the box goes in the
/// flow and its passage is lit in SRC: None else.
pub fn cut(s: &App, source: &str, src: &str, here: Here) -> Option<(usize, usize)> {
    if !s.notes.inline.get() || !ours(s) {
        return None;
    }
    let at = match s.notes.picked.as_ref().filter(|_| !s.notes.shut) {
        Some(pk) if pk.source == source => new_at(s, source, src, here)?,
        Some(_) => return None,
        None => {
            let o = passage(s, s.notes.open.as_deref()?).filter(|o| o.source == source)?;
            lights(src, here.earlier)(&o.at, &o.quote)?
        }
    };
    Some(room_cut(src, at.iter().map(|r| r.end).max()?))
}

/// SRC's Markdown with MARKS, split at CUT ([`cut`]) for the comment box's room: the words before it, the room, the
/// words after it, each part as BUILD draws it (its words, their marks, its key: ID, and ID-afterN for the words after,
/// N where they start in SRC, so a selection in them is placed in SRC: `in_source`).
pub fn split(s: &App, source: &str, src: &str, marks: &[Mark], cut: (usize, usize), id: &str, build: impl Fn(String, Vec<Mark>, String) -> El) -> El {
    let (e, c) = cut;
    let (before, after, n) = parts(src, cut);
    let (a, b) = halves(marks, e, c, n);
    let mut out = col().w_full().child(build(before, a, id.to_string())).children(room(s, source));
    if c < src.len() {
        out = out.child(build(after, b, format!("{id}-after{n}")));
    }
    out
}

/// SRC split at CUT ([`room_cut`]) for the comment box's room: the words before it, those after it, and where in SRC
/// these start less what was put before them (their key's N: [`in_source`]). Split inside a fenced code block, the
/// block is closed before the room and opened again after it with the same fence (its character and its length), its
/// info string and the quote it stands in, as its opening line has them (docket-edge2, consult-3 N-1477 p25): both
/// halves draw as code, each line at its x. A copy across the room takes the fence pair too, and the colours of the
/// code after it start afresh.
pub(crate) fn parts(src: &str, cut: (usize, usize)) -> (String, String, usize) {
    let (e, c) = cut;
    match fences(src, &events(src)).into_iter().find(|f| f.body <= e && e < f.code_end && e < c) {
        Some(f) => {
            let open = format!("{}{}{}\n", f.before, f.fence, f.info);
            (format!("{}\n{}{}", &src[..e], f.before, f.fence), format!("{open}{}", &src[c..]), c - open.len())
        }
        None => (src[..e].to_string(), src[c..].to_string(), c),
    }
}

/// The events rune-kit draws SRC from, with the bytes each stands for (`rune_kit::markdown::events`: its options, and
/// a paragraph's `N. ` lines made a list), so the words split for the room ([`room_cut`], [`parts`]) are read as they
/// are drawn (consult-3 N-1477 p30, p33; proto-window p32).
fn events(src: &str) -> Vec<(Event<'_>, Range<usize>)> {
    rune_kit::markdown::events(src).collect()
}

/// A fenced code block of Markdown, not in a list (there the words split at the item's end): its bytes, where its code
/// starts (the line after its opening line) and ends, and from its opening line what stands before its fence (a
/// quote's `> `, spaces), its fence (` ``` `, `~~~~`) and its info string.
struct Fenced<'a> {
    at: Range<usize>,
    body: usize,
    code_end: usize,
    before: &'a str,
    fence: &'a str,
    info: &'a str,
}

/// SRC's fenced code blocks at its top or in quotes ([`Fenced`]), by its events EV.
fn fences<'a>(src: &'a str, ev: &[(Event<'_>, Range<usize>)]) -> Vec<Fenced<'a>> {
    let (mut out, mut lists, mut open) = (Vec::new(), 0usize, None::<Fenced<'a>>);
    for (e, r) in ev {
        match e {
            Event::Start(Tag::List(_)) => lists += 1,
            Event::End(TagEnd::List(_)) => lists = lists.saturating_sub(1),
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_))) if lists == 0 => {
                let from = src[..r.start].rfind('\n').map_or(0, |i| i + 1);
                let to = src[r.start..].find('\n').map_or(src.len(), |i| r.start + i);
                let line = &src[from..to];
                let f = line.find(['`', '~']).unwrap_or(line.len());
                let ch = line.as_bytes().get(f).copied();
                let n = line.as_bytes()[f..].iter().take_while(|b| Some(**b) == ch).count();
                let body = (to + 1).min(src.len());
                open = Some(Fenced { at: r.clone(), body, code_end: body, before: &line[..f], fence: &line[f..f + n], info: line[f + n..].trim() });
            }
            Event::Text(_) => {
                if let Some(f) = &mut open {
                    f.code_end = f.code_end.max(r.end);
                }
            }
            Event::End(TagEnd::CodeBlock) => out.extend(open.take()),
            _ => {}
        }
    }
    out
}

/// MARKS on the words before byte E and on those from byte C on, these placed as the words after start at N in SRC
/// ([`parts`]).
fn halves(marks: &[Mark], e: usize, c: usize, n: usize) -> (Vec<Mark>, Vec<Mark>) {
    let (mut a, mut b) = (Vec::new(), Vec::new());
    for (at, h) in marks {
        let before: Vec<Range<usize>> = at.iter().filter(|r| r.start < e).map(|r| r.start..r.end.min(e)).collect();
        let after: Vec<Range<usize>> = at.iter().filter(|r| r.end > c).map(|r| r.start.max(c) - n..r.end - n).collect();
        if !before.is_empty() {
            a.push((before, h.clone()));
        }
        if !after.is_empty() {
            b.push((after, h.clone()));
        }
    }
    (a, b)
}

/// Where Markdown SRC splits for the comment box's room after a passage ending at byte END: the end of the words before
/// the room and the start of those after it. One walk over the events the words are drawn from ([`events`]; consult-3
/// N-1477 p30), in the block at the words' top that holds END:
/// - a paragraph: at the first space after END in no span (code, emphasis, strong, struck, a link, an image, HTML)
///   where the words on each side of it read as a paragraph still (`first, - not a list` splits after the `-`); else
///   at the paragraph's end;
/// - a list: at the end of its top item that holds END, so the items after it keep their depth and their numbers;
///   a list rune-kit makes of a paragraph's lines opening with `N. ` too, a wrapped item whole;
/// - fenced code, at the top or in a quote: at the end of END's line, the fence closed there and opened again after
///   the room ([`parts`]), unless that is the code's last line (then after the code);
/// - any other block (a table, a quote, a heading, HTML, indented code, a rule): at its end.
///
/// The words after the room start where the next block does (in a paragraph, at the next word).
pub(crate) fn room_cut(src: &str, end: usize) -> (usize, usize) {
    let mut end = end.min(src.len());
    while !src.is_char_boundary(end) {
        end += 1;
    }
    let ev = events(src);
    let tops = kids(&ev, None);
    let Some(t) = holding(&tops, end) else { return (src.len(), src.len()) };
    let (i, r) = tops[t].clone();
    let next = tops.get(t + 1).map_or(src.len(), |(_, r)| r.start);
    let trim = |r: Range<usize>| r.start + src[r].trim_end().len();
    let block = (trim(r.clone()), next);
    match &ev[i].0 {
        Event::Start(Tag::Paragraph) => in_paragraph(src, &ev, i, r, end, next),
        Event::Start(Tag::List(_)) => {
            let items = kids(&ev, Some(i));
            holding(&items, end).map_or(block, |k| (trim(items[k].1.clone()), items.get(k + 1).map_or(next, |(_, r)| r.start)))
        }
        Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_)) | Tag::BlockQuote(_)) => {
            let Some(f) = fences(src, &ev).into_iter().find(|f| r.start <= f.at.start && f.at.start <= end && end <= trim(f.at.clone())) else {
                return block;
            };
            let start = src[..end].rfind('\n').map_or(0, |i| i + 1);
            let line_end = src[end..].find('\n').map_or(src.len(), |i| end + i);
            if start >= f.body && line_end + 1 < f.code_end {
                return (line_end, line_end + 1);
            }
            // after the code: the block's end, else (more of its quote after it) the quote's next line
            let e = trim(f.at.clone());
            if e >= block.0 { block } else { (e, src[e..].find('\n').map_or(src.len(), |i| e + i + 1)) }
        }
        _ => block,
    }
}

/// The blocks directly in the one whose Start is event I of EV (a list's items), else (None) at the words' top: each
/// one's first event, and its bytes from its start to the furthest any of its events stands for, short of where the
/// next one starts. So a list rune-kit made of a paragraph's `N. ` lines, whose Start is its first line's text only,
/// ends where the End closing it does, and the paragraph it ended (its Start the whole paragraph as CommonMark reads
/// it) where the list starts (`rune_kit::markdown::events`).
fn kids(ev: &[(Event<'_>, Range<usize>)], i: Option<usize>) -> Vec<(usize, Range<usize>)> {
    let (mut out, mut depth) = (Vec::<(usize, Range<usize>)>::new(), 0usize);
    for (j, (e, r)) in ev.iter().enumerate().skip(i.map_or(0, |i| i + 1)) {
        match (depth, e) {
            (0, Event::End(_)) => break,
            (0, _) => out.push((j, r.clone())),
            _ => {
                if let Some((_, b)) = out.last_mut() {
                    b.end = b.end.max(r.end);
                }
            }
        }
        match e {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth -= 1,
            _ => {}
        }
    }
    for k in 1..out.len() {
        let next = out[k].1.start;
        let b = &mut out[k - 1].1;
        b.end = b.end.min(next).max(b.start);
    }
    out
}

/// Which of V (blocks, items: [`kids`]) holds byte END: the last that starts at or before it, else the first.
fn holding(v: &[(usize, Range<usize>)], end: usize) -> Option<usize> {
    v.iter().rposition(|(_, r)| r.start <= end).or((!v.is_empty()).then_some(0))
}

/// Where the paragraph at the words' top that is event I of EV, bytes R of SRC ([`kids`]), splits after byte END
/// ([`room_cut`]); NEXT, where the block after it starts.
fn in_paragraph(src: &str, ev: &[(Event<'_>, Range<usize>)], i: usize, r: Range<usize>, end: usize, next: usize) -> (usize, usize) {
    // its spans: code, emphasis, strong, struck, a link, an image, HTML
    let mut spans = Vec::new();
    for (e, at) in ev.iter().skip(i + 1) {
        match e {
            Event::End(TagEnd::Paragraph) => break,
            Event::Start(Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. } | Tag::Image { .. }) | Event::Code(_) | Event::InlineHtml(_) => spans.push(at.clone()),
            _ => {}
        }
    }
    let trim = |a: usize, b: usize| a + src[a..b].trim_end().len();
    let to = r.end;
    let past = |j: usize| to - src[j..to].trim_start().len();
    // the words on each side read as one paragraph, none starting a list, a heading, a quote or code
    let alone = |a: usize, b: usize| {
        let ev = events(&src[a..b]);
        matches!(ev.first(), Some((Event::Start(Tag::Paragraph), _))) && kids(&ev, None).len() == 1
    };
    (end..to)
        .filter(|&j| matches!(src.as_bytes()[j], b' ' | b'\t' | b'\n') && !spans.iter().any(|s| s.start < j && j < s.end))
        .find(|&j| past(j) < to && alone(r.start, j) && alone(past(j), to))
        .map_or((trim(r.start, to), next), |j| (j, past(j)))
}

/// The comment box's words: the passage, how many comments are on it, each with its answer, and the one message box
/// of the comment being written (PICKED), else Close.
fn box_words(s: &App, picked: Option<&Picked>, quote: &str, said: &[Said], k: &Kit) -> El {
    let mut body = col().child(text(format!("> {}", one_line(quote, 140))).font(k.ty.small).color(k.c.text_muted).truncate());
    if said.len() > 1 {
        body = body.child(text(format!("{} comments on it", said.len())).font(k.ty.small).color(k.c.text_faint));
    }
    body = body.children(said.iter().map(|c| said_row(s, c, k)).collect::<Vec<_>>());
    match picked {
        Some(p) => {
            // the one message box (message_box.rs): its spelling, cleaning, mic, @ and keys; its Send, Add comment,
            // sends the comment at once, as the Send of the place it goes with ([`send`]; N-1396 p329)
            if let Some(to) = goes_to(s, p) {
                body = body.child(text(to).font(k.ty.small).color(k.c.text_faint));
            }
            let target = crate::message_box::Target::Comment { project: p.pi };
            let cancel = Button::new("Cancel")
                .variant(ButtonVariant::Ghost)
                .on_press(|s: &mut App| {
                    unwrite(s);
                    s.notes.open = None;
                })
                .build(k);
            let actions = crate::message_box::Actions {
                left: None,
                right: vec![cancel],
                send: Some("Add comment".into()),
                placeholder: "Your comment (Ctrl+Enter sends)".into(),
            };
            body = body.child(crate::message_box::build(s, &target, &target, actions, k));
            // why it did not go, under it, his words still in it
            body = body.children(crate::message_box::said(s, p.pi, crate::message_box::COMMENT_PLACE, k));
        }
        None => body = body.child(row().gap(8.0).child(Button::new("Close").variant(ButtonVariant::Ghost).on_press(close_mark).build(k))),
    }
    body
}

/// The element comment SOURCE stands in, its box's place when neither its passage's mark nor where he selected it is
/// drawn (a comment taken back, one an older window kept): a thread's post (`post-ID-N`, post.rs), a mail's
/// or question's message (`qmsg-ID`), a file (files.rs `FILE_KEY`); None for a chat's entry (`AGENT@OFFSET`), whose
/// element key the source does not hold.
fn holder(source: &str) -> Option<String> {
    if is_file(source) {
        return Some(crate::files::FILE_KEY.to_string());
    }
    match docket_machine::post_ref(source) {
        Some((id, n)) => Some(format!("post-{id}-{n}")),
        None => (!source.contains('@')).then(|| format!("qmsg-{source}")),
    }
}

fn one_line(t: &str, n: usize) -> String {
    let flat = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > n { format!("{}…", flat.chars().take(n).collect::<String>()) } else { flat }
}

// ---- questions ----------------------------------------------------------------------------------------------------

/// Whether question Q is answered in its card's box: open, and not old mail's reply.
fn answerable(q: &MailView) -> bool {
    q.state.as_deref().unwrap_or("open") == "open" && q.re.is_none()
}

/// Whether a chat entry of KIND with TEXT shows a reply box: a mail whose thread's box is open, or `coord ask`'s
/// answer whose question is open. Such a row is built each frame, as every box is (chat.rs `item`).
pub fn holds_box(s: &App, p: &Proj, kind: &str, text: &str) -> bool {
    match kind {
        "mail" => delivered(text).iter().any(|(id, ..)| s.notes.replying.contains(id)),
        "result" => asked(p, text).is_some_and(answerable),
        _ => false,
    }
}

/// A question's card: who asked and when, its state, the explanation, its thread, and (unless dropped) the answer box
/// with its comments.
pub fn card(s: &App, p: &Proj, q: &MailView, k: &Kit) -> El {
    // built again only when what it reads changes: the mail, the notes (of the answer boxes, only its own), the
    // project's agents (whom a reply can go to) and, for "asked 5 m ago", the minute; typing in another card's
    // answer box reuses it
    // an open question holds the one message box, which reads more than a version could say (spelling, speech, the
    // cleaning): built each frame, as every box is
    if answerable(q) {
        return card_built(s, p, q, k);
    }
    let mut v = crate::version::Version::default();
    v.add((&q.id, k.dark));
    thread_version(s, p, &mut v);
    rune::memo::memo_with_hash(format!("card-{}", q.id), v.finish(), || card_built(s, p, q, k))
}

fn card_built(s: &App, p: &Proj, q: &MailView, k: &Kit) -> El {
    let state = q.state.as_deref().unwrap_or("open");
    let mut head = row()
        .w_full()
        .gap(8.0)
        .center_y()
        .child(text(format!("{} asks", q.from)).font(k.ty.small_medium).color(k.c.text_accent))
        .child(text(format!("{} · {} · asked {}", q.id, crate::when::at(&q.at), ago(&q.at))).font(k.ty.small).color(k.c.text_faint));
    head = head.child(match state {
        "open" => Badge::new("open").variant(BadgeVariant::Accent).build(k),
        other => Badge::new(other.to_string()).build(k),
    });
    // an old question still open: the owner answers it here (coordd keeps his answer as old mail beside the note it
    // gives the asker, 425e0fb) or dismisses it (drop as the owner); otherwise the old mail is read only (m1617)
    let open = answerable(q);
    if open {
        let (project, id) = (p.id.clone(), q.id.clone());
        head = head.child(row().grow(1.0).justify_end().child(
            Button::new("Dismiss")
                .variant(ButtonVariant::GhostMuted)
                .on_press(move |s: &mut App| {
                    if let Some(i) = s.projects.iter().position(|x| x.id == project) {
                        s.mail_in(i, Request::Drop { id: id.clone(), why: Some("dismissed by the owner".into()) });
                    }
                })
                .build(k),
        ));
    } else {
        head = head.child(text("old mail, read only".to_string()).font(k.ty.small).color(k.c.text_faint).grow(1.0));
    }
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
        .border(1.0, if open { k.c.border_focus } else { k.c.border_weak })
        .child(head)
        .child(text(subject.trim().to_string()).font(k.ty.ui_medium).color(k.c.text_base).wrap());
    if let Some(r) = &q.recognised {
        c = c.child(recognised_row(r, k));
    }
    if !rest.trim().is_empty() {
        // the comment box's room at its end, its passage lit nowhere (I-1476 p9)
        let key = format!("qmsg-{}", q.id);
        c = c.child(col().key(key.clone()).w_full().child(crate::post::message_body(Some(p), rest.trim(), Some((s, q.id.as_str())), None, k)).children(room_at(s, &key)));
    }
    for m in p.live.mail.replies(&q.id) {
        c = c.child(thread_row(s, p, m, k));
    }
    if open {
        c = c.child(reply_box(s, &q.id, &q.from, "Your answer (Cmd/Ctrl+Enter sends)", "Answer", k));
    }
    let target = q.id.clone();
    c.selection_group(true)
        .selection_menu_item("comment", "Comment", move |s: &mut App, sel, tx| {
            let pi = s.cur;
            comment_on(s, pi, &target, sel, tx)
        })
        .on_highlight_press(|s: &mut App, key, _| highlight_pressed(s, key))
}

/// Where a recognised question came from (old mail): the model that found it and the passage.
fn recognised_row(r: &coord_types::Recognised, k: &Kit) -> El {
    text(format!("found in a mail by {} · “{}”", r.model, one_line(&r.quote, 80))).font(k.ty.small).color(k.c.text_faint).wrap()
}

/// One mail of a thread, under what it answers: who, its id and time, its body.
fn thread_row(s: &App, p: &Proj, m: &MailView, k: &Kit) -> El {
    let who = crate::refs::person(&m.from);
    let mark = rune::el().w(2.0).h_full().radius(1.0).fill(if m.from == "owner" { k.c.text_accent } else { k.c.border_weak });
    row().w_full().gap(10.0).child(mark).child(
        col()
            .key(format!("qmsg-{}", m.id))
            .grow(1.0)
            .shrink()
            .gap(4.0)
            .child(text(format!("{who} · {} · {}", m.id, crate::when::at(&m.at))).font(k.ty.small_medium).color(k.c.text_muted))
            .child(crate::post::message_body(Some(p), &m.body, Some((s, m.id.as_str())), None, k))
            .children(room_at(s, &format!("qmsg-{}", m.id))),
    )
}

/// The To row and the one message box (message_box.rs) of a reply in thread ID (TO: whom it answers): its spelling,
/// cleaning, mic and keys are the box's; LABEL is its Send.
fn reply_box(s: &App, id: &str, to: &str, hint: &str, label: &str, k: &Kit) -> El {
    let target = crate::message_box::Target::Reply { project: s.cur, id: id.to_string(), to: to.to_string() };
    let actions = crate::message_box::Actions { left: None, right: Vec::new(), send: Some(label.to_string()), placeholder: format!("{hint}; @ adds an agent") };
    col()
        .w_full()
        .gap(8.0)
        .children(s.proj().map(|p| to_row(s, p, id, to, k)))
        .child(col().key(format!("answer-{id}")).w_full().child(crate::message_box::build(s, &target, &target, actions, k)))
        .children(crate::message_box::said(s, s.cur, id, k))
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

/// Mail TO the reply written in thread ID, `re` the thread's latest mail.
pub fn reply(s: &mut App, id: &str, to: &str) {
    let key = (s.cur, id.to_string());
    let typed = s.composers.get(&key).map(|e| e.text.clone()).unwrap_or_default();
    let comments = held(s, key.0, id).to_vec();
    if typed.trim().is_empty() && comments.is_empty() {
        return;
    }
    let req = match reply_request(s, key.0, id, &typed, &comments, to) {
        Ok(req) => req,
        Err(e) => {
            if !e.is_empty() {
                s.notice = Some(e);
            }
            return;
        }
    };
    let Some(p) = s.proj() else { return };
    let kept = format!("{}/{}", p.id, p.live.mail.top(id));
    // as every box sends: its words go once coordd has it; the thread stays as it is shown (its card in the list, its
    // reply box open), as in Slack
    let target = crate::message_box::Target::Reply { project: key.0, id: id.to_string(), to: String::new() };
    if crate::message_box::sent(s, &target, req) {
        s.notes.kept.insert(kept);
        s.notes.added.remove(id);
        s.notes.left_out.remove(id);
        if s.notes.mention.as_ref().is_some_and(|m| m.field == Field::Reply(key.0, id.to_string())) {
            s.notes.mention = None;
        }
    }
}

/// The mail a reply in mail thread ID of project PI makes, TYPED after COMMENTS: to the thread's people, TO (the To
/// row's) and each agent tagged @NAME, `re` the thread's latest mail. Err: why it cannot go ("" for no project).
pub(crate) fn reply_request(s: &App, pi: usize, id: &str, typed: &str, comments: &[Comment], to: &str) -> Result<Request, String> {
    let p = s.projects.get(pi).ok_or_else(String::new)?;
    let latest = p.live.mail.replies(id).last().map(|m| m.id.clone()).unwrap_or_else(|| id.to_string());
    // the To row's recipients, and each @NAME typed (one naming no agent stops the send)
    let mut to: Vec<String> = recipients(s, p, id, to).into_iter().filter(|(n, left)| !left && !n.is_empty()).map(|(n, _)| n).collect();
    // and each agent tagged @NAME in it or in its comments (a tag naming no agent stops the send)
    let notes: Vec<&str> = std::iter::once(typed).chain(comments.iter().map(|c| c.note.as_str())).collect();
    let names = tagged(p, &notes)?;
    to.extend(names.into_iter().filter(|n| !to.contains(n)).collect::<Vec<_>>());
    if to.is_empty() {
        return Err("not sent: nobody on the To row".into());
    }
    let (body, quotes) = compose(comments, typed);
    Ok(Request::Msg { kind: None, to, body, esc: false, soon: false, wait: false, as_host: None, re: Some(latest), quotes, origin: None, under: None, label: None, comments: data(p, comments) })
}

/// The mails a chat entry of kind `mail` carries: the channel form (`<channel source="coord" from=.. id=..>`) or the
/// batch form (`[ID · from NAME]` headings); (id, from, body as delivered).
/// The mails ENTRY delivered: id, from, body, and for a ticket's delivery (its tag names a ticket) what it is
/// (`chat::channel_heading`: "message from you · N-44").
pub fn delivered(entry: &str) -> Vec<(String, String, String, Option<String>)> {
    let attr = |head: &str, name: &str| head.split(&format!("{name}=\"")).nth(1).and_then(|r| r.split('"').next()).unwrap_or("").to_string();
    let mut out = Vec::new();
    if entry.trim_start().starts_with("<channel") {
        for part in entry.split("<channel").skip(1) {
            let Some((head, rest)) = part.split_once('>') else { continue };
            let body = rest.split("</channel>").next().unwrap_or(rest).trim().to_string();
            out.push((attr(head, "id"), attr(head, "from"), body, crate::chat::channel_heading(&format!("<channel{head}>"))));
        }
        return out;
    }
    let mut cur: Option<(String, String, Vec<&str>)> = None;
    for line in entry.lines() {
        let heading = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')).and_then(|l| l.split_once(" · from "));
        match heading {
            Some((id, from)) if !id.contains(' ') => {
                out.extend(cur.take().map(|(i, f, b)| (i, f, b.join("\n").trim().to_string(), None)));
                cur = Some((id.to_string(), from.to_string(), Vec::new()));
            }
            _ => {
                if let Some((_, _, b)) = cur.as_mut() {
                    b.push(line);
                }
            }
        }
    }
    out.extend(cur.map(|(i, f, b)| (i, f, b.join("\n").trim().to_string(), None)));
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
    for (id, from, shown, ticket) in mails {
        // a ticket's delivery: what it is, its words; no old-mail thread or reply (a note is answered on its ticket)
        if let Some(what) = ticket {
            let head = row().gap(6.0).center_y().child(text(what).font(k.ty.small_medium).color(k.c.text_accent)).child(text(crate::when::at(at)).font(k.ty.small).color(k.c.text_faint));
            // his message on the right, in his bubble, as on a thread (window rules §6)
            if from == "owner" {
                let words = col().w_full().child(crate::post::message_body(Some(p), &shown, Some((s, id.as_str())), None, k));
                c = c.child(col().w_full().items_end().gap(6.0).child(head).child(crate::post::bubble_for(&shown, words, false, k).key(format!("qmsg-{id}"))).children(room_at(s, &format!("qmsg-{id}"))));
                continue;
            }
            let card = col()
                .key(format!("qmsg-{id}"))
                .w_full()
                .padding(12.0)
                .radius(k.radius.md)
                .fill(k.c.layer_01)
                .border(1.0, k.c.border_weak)
                .child(crate::post::message_body(Some(p), &shown, Some((s, id.as_str())), None, k))
                .children(room_at(s, &format!("qmsg-{id}")));
            c = c.child(col().w_full().gap(6.0).child(head).child(card));
            continue;
        }
        let rec = p.live.mail.get(&id);
        let body = rec.map_or(shown, |m| m.body.clone());
        let who = crate::refs::person(&from);
        let mut head = row().gap(6.0).center_y().child(text(format!("mail from {who}")).font(k.ty.small_medium).color(k.c.text_accent)).child(text(format!("{id} · {}", crate::when::at(at))).font(k.ty.small).color(k.c.text_faint));
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
            .child(crate::post::message_body(Some(p), &body, Some((s, id.as_str())), None, k))
            .children(room_at(s, &format!("qmsg-{id}")));
        let in_question = rec.and_then(|m| p.live.mail.root(m)).map(|q| q.id.clone());
        match in_question {
            Some(q) => card = card.child(text(format!("in the thread of {q}")).font(k.ty.small).color(k.c.text_faint)),
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
            card = card.selection_group(true).selection_menu_item("comment", "Comment", move |s: &mut App, sel, tx| {
            let pi = s.cur;
            comment_on(s, pi, &target, sel, tx)
        });
        }
        c = c.child(col().w_full().gap(6.0).child(head).child(card));
    }
    Some(c)
}

/// A chat entry that is `coord ask`'s answer (`qN: asked…`): the question it made, if the mail has it.
pub fn asked<'a>(p: &'a Proj, entry_text: &str) -> Option<&'a MailView> {
    let id = entry_text.trim_start().split(':').next().filter(|id| id.starts_with('q') && id[1..].parse::<u64>().is_ok())?;
    entry_text.contains(": asked;").then_some(())?;
    let m = p.live.mail.iter().find(|m| m.id == id && m.kind == "question")?;
    p.live.mail.root(m)
}

#[cfg(test)]
mod quote_tests {
    use super::*;

    /// What a reply follows reads from where it was begun: a thread's post key, an id, a delivery's channel tag.
    #[test]
    fn a_reply_names_the_post_it_was_begun_on() {
        assert_eq!(post_key("post-N-221-5"), Some(("N-221".to_string(), 5)));
        assert_eq!(post_key("post-box-N-221"), None);
        assert!(is_place("N-221") && is_place("N-221.p5") && is_place("Q-56"));
        assert!(!is_place("docket-edge@120") && !is_place("m12") && !is_place("N-221.s3") && !is_place("N-"));
        let tag = r#"<channel source="coord" from="paper-research" id="N-101.p481" kind="thread" ticket="N-101">"#;
        assert_eq!(channel_attr(tag, "ticket").as_deref(), Some("N-101"));
        assert_eq!(channel_attr(tag, "source").as_deref(), Some("coord"));
        assert_eq!(channel_attr(tag, "step"), None);
    }
}

// ---- question tickets outside the Tickets page --------------------------------------------------------------------
//
// Decisions' answered questions and an item's questions read the question tickets as well as the mail: where `coord
// ask` opens a question ticket, the owner's record of what he answered is there (paper-research m1591).

/// A question ticket's state as the mail's questions name theirs: `open` until it closes, then `answered` or
/// `dismissed` (its result), `closed` for another result, `failed`.
pub(crate) fn question_state(v: &TicketView) -> &'static str {
    match &v.status {
        ViewStatus::Done { result } if result["head"] == "Answered" => "answered",
        ViewStatus::Done { result } if result["head"] == "Dismissed" => "dismissed",
        ViewStatus::Done { .. } => "closed",
        ViewStatus::Failed { .. } => "failed",
        _ => "open",
    }
}

/// The owner's answer on question ticket V: his last post that answers a step.
pub(crate) fn owners_answer(v: &TicketView) -> Option<&docket_machine::Post> {
    v.posts.iter().rev().find(|x| x.by == "owner" && x.head.is_some() && x.step.is_some())
}

/// The question tickets, newest first.
fn question_tickets(p: &Proj) -> Vec<&TicketView> {
    let mut qs: Vec<&TicketView> = p.live.tickets.values().filter(|v| v.kind == "question").collect();
    qs.sort_by(|a, b| b.opened_at.cmp(&a.opened_at).then(b.n.cmp(&a.n)));
    qs
}

/// The question tickets the owner answered (closed as Answered, the answer his), newest first: Decisions' list.
pub(crate) fn answered_questions(p: &Proj) -> Vec<&TicketView> {
    question_tickets(p).into_iter().filter(|v| question_state(v) == "answered" && owners_answer(v).is_some()).collect()
}

/// A question ticket's ask and context, and its answer (the result's text), as the page has read them.
pub(crate) fn question_words(pg: &Page, v: &TicketView) -> (String, String, String) {
    let field = |f: &str| v.form.get(f).map(|x| words(pg, x)).unwrap_or_default();
    let answer = match &v.status {
        ViewStatus::Done { result } => result.get("text").map(|x| words(pg, x)).unwrap_or_default(),
        _ => String::new(),
    };
    (field("ask"), field("context"), answer)
}

/// Whether question ticket V names item ID, by the mail's rule (its subject or body has it): its ask or its context.
fn question_names(pg: &Page, v: &TicketView, id: &str) -> bool {
    let (ask, context, _) = question_words(pg, v);
    ask.contains(id) || context.contains(id)
}

/// The closed question tickets that name item ID, newest first: an item's page lists them beside the mail's (an open
/// one waits on the owner in Threads).
pub(crate) fn item_questions<'a>(pg: &Page, p: &'a Proj, id: &str) -> Vec<&'a TicketView> {
    question_tickets(p).into_iter().filter(|v| question_state(v) != "open" && question_names(pg, v, id)).collect()
}

/// The last post of each ticket the Threads list shows, not yet read: a row's second line.
/// An untitled ticket's words, which its row shows: its form's `text` (an ask's question), or one in a record of it.
pub(crate) fn asked_words(v: &TicketView) -> Option<&Value> {
    let o = v.form.as_object()?;
    o.get("text").or_else(|| o.values().find_map(|x| x.get("text")))
}

/// An agent's question A (an `ask` ticket, its question under it) as the owner meets it: who asked, the question in
/// its words, who passed it to him, and his answer; or, while it waits on him, the box he answers in.
pub(crate) fn ask_block(pg: &Page, p: &Proj, a: &TicketView, k: &Kit) -> El {
    ask_block_in(pg, p, a, None, k)
}

/// As [`ask_block`]; AIM, on a thread: the step he owes is answered in the thread's one box, so its card has no text
/// box of its own, and AIM gives what goes under it (threads.rs `aim`: the press that aims the box at it).
pub(crate) fn ask_block_in(pg: &Page, p: &Proj, a: &TicketView, aim: Option<&dyn Fn(&TicketView) -> El>, k: &Kit) -> El {
    let by = a.opener.clone().or_else(|| a.form.get("by").and_then(Value::as_str).map(str::to_string)).unwrap_or_else(|| "an agent".into());
    let q = a.children.iter().find_map(|id| p.live.tickets.values().find(|t| &t.id == id && t.kind == "question"));
    let asked = a.form.get("text").or_else(|| q.and_then(|q| q.form.get("ask"))).map(|x| words(pg, x)).filter(|w| !w.trim().is_empty()).unwrap_or_else(|| a.title.clone());
    // his answer is owed on its question, or on the ask itself when a thread's step asked it
    let owed = q.into_iter().chain(std::iter::once(a)).find(|t| matches!(&t.status, ViewStatus::Awaits { agent, .. } if agent == "owner"));
    let waits = owed.is_some();
    let head = if waits { format!("{by} asks you") } else { format!("{by} asked") };
    let mut c = col().w_full().gap(6.0).padding(12.0).radius(k.radius.md).fill(k.c.layer_01).border(1.0, k.c.border_weak);
    c = c.child(text(head).font(k.ty.small_medium).color(if waits { k.c.warning.fg } else { k.c.text_muted }));
    // the agents write markdown: drawn as it is meant, not with its marks. Once answered it folds to its first line,
    // so the ticket's stages stay in sight; a click opens it whole
    let fold = format!("ask-{}", a.id);
    let whole = waits || pg.pic_open.contains(&fold);
    let q_text = bare_question(&asked).trim().to_string();
    // a session file rewritten since: the reader's error names a path; he is told what it means
    let q_text = if q_text.starts_with("(not read:") { format!("{by}'s question is no longer in its session's file, so it cannot be shown") } else { q_text };
    if whole {
        c = c.child(crate::post::message_body(Some(p), &q_text, None, None, k));
    } else {
        let first = crate::texts::plain(q_text.lines().find(|l| !l.trim().is_empty()).unwrap_or(""));
        c = c.child(text(crate::refs::cut(first.trim(), 160)).font(k.ty.body).color(k.c.text_base).wrap());
    }
    if !waits {
        let f2 = fold.clone();
        c = c.child(k.interactive(row().child(text(if whole { "⌄ less" } else { "› the whole question" }).font(k.ty.small).color(k.c.text_accent))).on_press(move |s: &mut App| {
            if !s.tickets.pic_open.remove(&f2) {
                s.tickets.pic_open.insert(f2.clone());
            }
        }));
    }
    // who passed it on to him: the lead's step answered Up (its post on the question)
    if let Some(up) = q.and_then(|q| q.posts.iter().find(|x| x.by != "owner" && x.head.as_ref().and_then(|h| h.get("head")).and_then(Value::as_str) == Some("Up"))) {
        c = c.child(text(format!("{} passed it on to you", up.by)).font(k.ty.small).color(k.c.text_muted));
    }
    match q.map(|q| &q.status) {
        Some(ViewStatus::Done { result }) => {
            let (said, t) = match result.get("head").and_then(Value::as_str) {
                Some("Dismissed") => ("Dismissed", result.get("why")),
                _ => ("Answered", result.get("text")),
            };
            let who = q.and_then(|q| q.posts.iter().rev().find(|x| x.step.is_some()).map(|x| crate::refs::person(&x.by))).unwrap_or_default();
            c = c.child(text(format!("{said}{}:", if who.is_empty() { String::new() } else { format!(" by {who}") })).font(k.ty.small_medium).color(k.c.text_muted));
            c = c.child(crate::post::message_body(Some(p), &t.map(|t| words(pg, t)).unwrap_or_default(), None, None, k));
        }
        _ => match aim {
            Some(aim) => c = c.children(owed.map(|o| col().w_full().gap(6.0).child(answer_card(pg, p, o, true, k)).child(aim(o)))),
            None => c = c.children(owed.map(|o| answer_box(pg, p, o, k))),
        },
    }
    c
}

/// A question's words without the answer's head around them: a field read by reference holds the whole answer
/// ("Ask", then "question: …"; docket-core3, N-101 p984), and the owner reads only the question.
/// An agent's question (`ask`, `question`) headed as the owner reads it: "designer15 asks", and the question's first
/// line, each said once where it is shown (paper-research N-371.p5, p7: not the kind's description); none for other
/// kinds.
pub(crate) fn question_headline(pg: &Page, v: &TicketView) -> Option<(String, String)> {
    if !matches!(v.kind.as_str(), "ask" | "question") {
        return None;
    }
    let by = v.opener.clone().unwrap_or_else(|| "an agent".into());
    let q = asked_words(v).or_else(|| v.form.get("ask")).map(|x| words(pg, x)).unwrap_or_default();
    // plain text: the code marks of its words go
    let line = bare_question(&q).lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").replace('`', "");
    Some((format!("{by} asks"), crate::refs::cut(&line, 140)))
}

fn bare_question(t: &str) -> String {
    let t = t.trim_start();
    let t = t.strip_prefix("Ask").filter(|r| r.starts_with('\n')).map(str::trim_start).unwrap_or(t);
    t.strip_prefix("question:").map(str::trim_start).unwrap_or(t).to_string()
}

/// The owner's answer to the step V awaits: one text box, the step's other fields, and a button per result.
pub(crate) fn answer_box(pg: &Page, p: &Proj, v: &TicketView, k: &Kit) -> El {
    answer_card(pg, p, v, false, k)
}

/// The step V awaits of him, as `answer_box` draws it; IN_BOX: the one message box below answers it (N-449.p6), so
/// the card has no text box and no result buttons of its own (they are the box's actions, `step_actions`).
pub(crate) fn answer_card(pg: &Page, p: &Proj, v: &TicketView, in_box: bool, k: &Kit) -> El {
    let ViewStatus::Awaits { call, step, head, args, .. } = &v.status else { return col() };
    // a question thought through before it reached him: the candidate answer first (the owner, N-310.p3)
    if card_asks(v) {
        return considered(pg, p, v, args, in_box, k);
    }
    // a stop passed to him: what stopped and why in words, and the two answers with what each does
    if call == "stopped" {
        if let Some(x) = p.live.tickets.values().find(|x| matches!(&x.status, ViewStatus::Stopped { by, .. } if *by == v.id)) {
            return stop_box(pg, p, x, v, in_box, k);
        }
    }
    let n = v.n;
    let mut c = col().w_full().gap(8.0).padding(12.0).radius(k.radius.md).fill(k.c.layer_01).border(1.0, k.c.border_weak);
    c = c.child(text(format!("Step {step} is yours: {call}")).font(k.ty.ui_medium).color(k.c.text_base));
    // passed up to him: the answers that were not read, to judge it by
    if let Some(p) = crate::picture::passed_el(v, &|x: &Value| words(pg, x), k) {
        c = c.child(p);
    }
    let Type::Variant(cs) = head else {
        return c.child(plain_box(pg, v, head, k));
    };
    // the fields that are not text: one input each, across the results that have them
    let mut others: Vec<(String, Type)> = Vec::new();
    for (_, fs) in cs {
        for (f, t) in fs {
            if !is_text(t) && !others.iter().any(|(g, _)| g == f) {
                others.push((f.clone(), t.clone()));
            }
        }
    }
    for (f, t) in &others {
        c = c.child(field_input(pg, n, f, t, format!("{f} ({t})"), None, k));
    }
    if in_box {
        return c;
    }
    let edit = pg.answer.get(&n).cloned().unwrap_or_else(|| TextEdit::new(""));
    c = c.child(col().key(format!("answer-{}", v.id)).w_full().child(TextArea::new(&edit).width(None).placeholder("Your answer: why, a note, what the next one should know").auto_grow(3, 12).on_edit(move |s: &mut App| s.tickets.answer.entry(n).or_insert_with(|| TextEdit::new(""))).build(k)));
    let mut buttons = row().w_full().gap(8.0).justify_end();
    for (cname, fs) in cs {
        let (cname, fs) = (cname.clone(), fs.clone());
        let label = cname.clone();
        buttons = buttons.child(Button::new(label).variant(ButtonVariant::Neutral).on_press(move |s: &mut App| answer(s, n, &cname, &fs)).build(k));
    }
    c.child(buttons)
}

/// Whether the step V awaits of him was thought through before it reached him (`reply_considered`): its card is
/// `considered`, which says what is asked.
pub(crate) fn card_asks(v: &TicketView) -> bool {
    matches!(&v.status, ViewStatus::Awaits { args, .. } if args.get("answer").is_some() && args.get("reasoning").is_some())
}

/// Question V's words on its own page, under who asks (`question_headline`), unless its card says them
/// (`card_asks`). Asked straight of him (`ask_owner`: its arguments are the ask and its context), the page said only
/// who asks and the question stood under "more about it" (Q-928, which a `Q-…` link opens: 0.2 line C19).
pub(crate) fn asked_on_page(pg: &Page, p: &Proj, v: &TicketView, k: &Kit) -> Vec<El> {
    if v.kind != "question" || card_asks(v) {
        return Vec::new();
    }
    let ask = bare_question(&question_words(pg, v).0).trim().to_string();
    if ask.is_empty() {
        return Vec::new();
    }
    ask_shown(pg, p, v, ask, k)
}

/// What question V asks, ASK in the asker's words (who asks is the page's headline, N-371.p7): its start, the rest
/// behind a click, the answer below being what he acts on (reveal little at once).
fn ask_shown(pg: &Page, p: &Proj, v: &TicketView, ask: String, k: &Kit) -> Vec<El> {
    let key = format!("ask-{}", v.id);
    let whole = pg.unfolded.contains(&key);
    let mut at = 420.min(ask.len());
    while !ask.is_char_boundary(at) {
        at -= 1;
    }
    let cut = if whole || ask.len() <= 420 { None } else { ask[..at].rfind(". ").map(|i| ask[..=i].to_string()) };
    match cut {
        Some(start) => vec![
            crate::post::message_body(Some(p), &start, None, None, k),
            text("› the whole question").font(k.ty.small).color(k.c.text_accent).cursor(rune::Cursor::Pointer).on_press(move |s: &mut App| {
                s.tickets.unfolded.insert(key.clone());
            }),
        ],
        None => vec![crate::post::message_body(Some(p), &ask, None, None, k)],
    }
}

/// The owner's step on question V thought through (`reply_considered`, ARGS its arguments), as he meets it most often:
/// the candidate answer on top with the one action that answers with it, his own words beside it; under it two things
/// he opens each by itself, what was found (his earlier ruling first, marked as his, then each passage by its source,
/// the passage itself behind a click) and the reasoning with what would change it (the owner, N-310.p3: "indeed hte
/// goal is alreayd having a coandiate there"; paper-research N-101 posts 1011, 1028, 1042).
fn considered(pg: &Page, p: &Proj, v: &TicketView, args: &Value, in_box: bool, k: &Kit) -> El {
    let n = v.n;
    let w = |f: &str| args.get(f).map(|x| words(pg, x)).unwrap_or_default().trim().to_string();
    let candidate = w("answer");
    let ready = !candidate.is_empty() && candidate != "…" && !candidate.starts_with("(not read");
    let mut c = col().w_full().gap(10.0).padding(14.0).radius(k.radius.md).fill(k.c.layer_01).border(1.0, k.c.border_weak);
    // what is asked comes first, in the asker's words, and why it reached him; then the answer thought through
    let (ask, up) = (w("ask"), w("why"));
    if !ask.is_empty() {
        c = c.children(ask_shown(pg, p, v, ask, k));
    }
    if !up.is_empty() {
        c = c.child(text(format!("Passed up to you: {up}")).font(k.ty.body).color(k.c.text_muted).wrap());
    }
    c = c.child(text("Thought through, the answer would be").font(k.ty.small_medium).color(k.c.text_muted));
    c = c.child(barred(crate::post::message_body(Some(p), &candidate, None, None, k), k.c.text_accent));
    let id = v.id.clone();
    let take = Button::new("Answer with this").variant(ButtonVariant::Contrast).on_press(move |s: &mut App| {
        if ready {
            let req = Request::Answer { t: id.clone(), head: json!({"head": "Answer"}), text: Some(candidate.clone()) };
            s.request_in(s.cur, req, Pending::TicketDone { clear: Cleared::Answer(n) });
        }
    });
    c = c.child(row().w_full().gap(8.0).child(take.build(k)));
    // the two folds, each opened by itself
    let fold = |key: String, label: &str| {
        let open = pg.pic_open.contains(&key);
        let el = k.interactive(row().child(text(format!("{} {label}", if open { "⌄" } else { "›" })).font(k.ty.small_medium).color(k.c.text_accent))).on_press(move |s: &mut App| {
            if !s.tickets.pic_open.remove(&key) {
                s.tickets.pic_open.insert(key.clone());
            }
        });
        (open, el)
    };
    let found: Vec<&Value> = args.get("found").and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default();
    let ruling = args.get("ruling").filter(|r| r.is_object());
    let count = found.len() + usize::from(ruling.is_some());
    let label = match (ruling.is_some(), count) {
        (true, _) => format!("What was found: your earlier ruling, and {} more", found.len()),
        (false, 0) => "What was found: nothing bears on it".to_string(),
        (false, 1) => "What was found: one passage".to_string(),
        (false, m) => format!("What was found: {m} passages"),
    };
    let (open, el) = fold(format!("found-{}", v.id), &label);
    let mut f = col().w_full().gap(8.0).child(el);
    if open {
        if let Some(r) = ruling {
            f = f.child(bearing(pg, p, &v.id, "ruling", r, true, k));
        }
        for (i, b) in found.iter().enumerate() {
            f = f.child(bearing(pg, p, &v.id, &i.to_string(), b, false, k));
        }
    }
    c = c.child(f);
    let (open, el) = fold(format!("reasoning-{}", v.id), "How it was reached, and what would change it");
    let mut r = col().w_full().gap(8.0).child(el);
    if open {
        r = r.child(col().w_full().pl(14.0).gap(8.0).child(crate::post::message_body(Some(p), &w("reasoning"), None, None, k)).child(text("What would change it").font(k.ty.small_medium).color(k.c.text_muted)).child(crate::post::message_body(Some(p), &w("change"), None, None, k)));
    }
    c = c.child(r);
    if in_box {
        return c;
    }
    // his own words in place of it: the box, then Answer or Dismiss
    let edit = pg.answer.get(&n).cloned().unwrap_or_else(|| TextEdit::new(""));
    c = c.child(col().key(format!("answer-{}", v.id)).w_full().pt(4.0).child(TextArea::new(&edit).width(None).placeholder("Or your own words").auto_grow(2, 12).on_edit(move |s: &mut App| s.tickets.answer.entry(n).or_insert_with(|| TextEdit::new(""))).build(k)));
    let mut buttons = row().w_full().gap(8.0).justify_end();
    if let ViewStatus::Awaits { head: Type::Variant(cs), .. } = &v.status {
        for (cname, fs) in cs {
            let (cname, fs) = (cname.clone(), fs.clone());
            let label = if cname == "Answer" { "Answer in my words".to_string() } else { cname.clone() };
            buttons = buttons.child(Button::new(label).variant(ButtonVariant::Neutral).on_press(move |s: &mut App| answer(s, n, &cname, &fs)).build(k));
        }
    }
    c.child(buttons)
}

/// Why ticket X stopped, in words: a broken duty as who owes whom what, on which work, and the answer that came
/// without it; nobody to take its step as who could not and why; another exception by its name in words and its why.
fn stop_why(p: &Proj, x: &TicketView) -> String {
    let ViewStatus::Stopped { raised, .. } = &x.status else { return String::new() };
    let r = &raised.value;
    let exc = raised.exc.as_str();
    let s = |f: &str| r.get(f).and_then(Value::as_str).unwrap_or("").to_string();
    // the record names work as T-n; the view's id carries its kind's letter (T-25 is P-25): found by its number
    let num = |id: &str| id.rsplit('-').next().and_then(|n| n.parse::<u64>().ok());
    let named = |id: &str| match p.live.tickets.values().find(|t| t.id == id || num(id).is_some_and(|n| t.n == n)) {
        Some(t) => { let r = crate::refs::ticket(Some(p), t); format!("{} ({})", r.label, t.id) }
        None => p.labels.get(id).map_or_else(|| id.to_string(), |l| format!("{l} ({id})")),
    };
    match exc {
        "Violated" => {
            let duty = s("duty");
            let duty = duty.strip_prefix("owes-").unwrap_or(&duty).replace('-', " ");
            let answered = r.get("answered").and_then(|a| a.get("head")).and_then(Value::as_str).unwrap_or("its answer");
            format!("{} answered {answered}, but the {duty} it owes {} on {} was never met", s("holder"), s("claimant"), named(&s("work")))
        }
        // nobody to take its step: the agent that could not, or the role nobody fills
        "NoWorker" => match (s("agent"), s("why"), s("role")) {
            (a, w, _) if !a.is_empty() && !w.is_empty() => format!("{a} could not take it: {w}"),
            (_, w, _) if !w.is_empty() => format!("nobody could take it: {w}"),
            (_, _, role) if !role.is_empty() => format!("nobody could take it: no agent is the {role} here"),
            _ => "nobody could take it".to_string(),
        },
        _ => {
            let name = spaced(exc);
            match s("why") {
                w if !w.is_empty() => format!("{name}: {w}"),
                _ => name,
            }
        }
    }
}

/// An exception's name in words: `RunnerRefused` → "runner refused".
fn spaced(exc: &str) -> String {
    let mut out = String::new();
    for (i, c) in exc.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push(' ');
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// The owner's step on a stop (H, the hidden `stopped` ticket of X): what stopped and why, then Again and Ended, each
/// with what it does, and the box for why he ends it (paper-research N-101 p962; docket-core3 p1066: the first stop to
/// reach him read only "Step 1 is yours: stopped").
fn stop_box(pg: &Page, p: &Proj, x: &TicketView, h: &TicketView, in_box: bool, k: &Kit) -> El {
    let n = h.n;
    let mut c = col().w_full().gap(8.0).padding(12.0).radius(k.radius.md).fill(k.c.layer_01).border(1.0, k.c.border_weak);
    let what = crate::refs::ticket(Some(p), x);
    // a question's page says who asks what above it; anything else is named
    if x.kind != "question" {
        c = c.child(text(format!("{} ({})", what.label, what.id)).font(k.ty.ui_medium).color(k.c.text_base).wrap());
    }
    c = c.child(text(format!("It stopped: {}. Yours to say whether it runs again.", stop_why(p, x))).font(k.ty.body).color(k.c.text_base).wrap());
    // how often the lead answered Again before it came to him
    let before = x.handlers.iter().filter(|o| o.id != h.id && o.kind == "stopped").count();
    if before > 0 {
        c = c.child(text(format!("It stopped {} before; the lead answered Again{}.", if before == 1 { "once".to_string() } else { format!("{before} times") }, if before == 1 { "" } else { " each time" })).font(k.ty.small).color(k.c.text_muted).wrap());
    }
    let Some(ViewStatus::Awaits { head: Type::Variant(cs), .. }) = Some(&h.status) else { return c };
    let says = |c: &str| match c {
        "Again" => "runs it again from where it stopped, once what stopped it is fixed",
        "Ended" => "ends it as failed",
        _ => "",
    };
    for (cname, fs) in cs {
        // answered from the box: what each answer does, said here once; its button is beside the box
        if in_box {
            c = c.child(text(format!("{cname}: {}", says(cname))).font(k.ty.small).color(k.c.text_muted).wrap());
            continue;
        }
        let (cn, fs2) = (cname.clone(), fs.clone());
        let b = Button::new(cname.clone()).variant(ButtonVariant::Neutral).on_press(move |s: &mut App| answer(s, n, &cn, &fs2)).build(k);
        c = c.child(row().w_full().gap(10.0).center_y().child(b).child(text(says(cname)).font(k.ty.small).color(k.c.text_muted).wrap().grow(1.0).shrink().w(0.0)));
    }
    if in_box {
        return c;
    }
    let edit = pg.answer.get(&n).cloned().unwrap_or_else(|| TextEdit::new(""));
    c.child(col().key(format!("answer-{}", h.id)).w_full().child(TextArea::new(&edit).width(None).placeholder("Why, if you end it: one sentence the record keeps").auto_grow(2, 8).on_edit(move |s: &mut App| s.tickets.answer.entry(n).or_insert_with(|| TextEdit::new(""))).build(k)))
}

/// BODY with a bar of COLOR down its left.
fn barred(body: El, color: rune::Color) -> El {
    row().w_full().gap(10.0).child(rune::el().w(2.0).h_full().radius(1.0).fill(color)).child(col().grow(1.0).shrink().w(0.0).child(body))
}

/// One passage that bears on a question: how it bears, one line, and where it stands (a ticket by its label and id);
/// the passage word for word behind a click. MINE: the owner's own earlier ruling, marked as his words.
fn bearing(pg: &Page, p: &Proj, q: &str, i: &str, b: &Value, mine: bool, k: &Kit) -> El {
    let w = |f: &str| b.get(f).map(|x| words(pg, x)).unwrap_or_default().trim().to_string();
    let at = w("at");
    let key = format!("bearing-{q}-{i}");
    let open = pg.pic_open.contains(&key);
    let mut c = col().w_full().gap(4.0).pl(14.0);
    // his ruling is shown in his words at once: it is what decides; how it bears and where it stands under it
    if mine {
        c = c.child(text("Your earlier ruling, in your words").font(k.ty.small_medium).color(k.c.warning.fg));
        c = c.child(barred(crate::post::message_body(Some(p), &w("passage"), None, None, k), k.c.warning.fg));
        c = c.child(text(crate::texts::first_line_cut(&w("bears"))).font(k.ty.small).color(k.c.text_muted).wrap());
        return c.child(text(source_of(p, &at)).font(k.ty.small).color(k.c.text_muted).wrap());
    }
    c = c.child(text(crate::texts::first_line_cut(&w("bears"))).font(k.ty.body).color(k.c.text_base).wrap());
    c = c.child(text(source_of(p, &at)).font(k.ty.small).color(k.c.text_muted).wrap());
    let k2 = key.clone();
    c = c.child(k.interactive(row().child(text(if open { "⌄ less" } else { "› the passage" }).font(k.ty.small).color(k.c.text_accent))).on_press(move |s: &mut App| {
        if !s.tickets.pic_open.remove(&k2) {
            s.tickets.pic_open.insert(k2.clone());
        }
    }));
    if open {
        c = c.child(barred(crate::post::message_body(Some(p), &w("passage"), None, None, k), k.c.border_weak));
    }
    c
}

/// Where a passage stands, as the owner reads it: a ticket's id with its label first (`the dry run … · N-101 post
/// 990`), a file as written.
fn source_of(p: &Proj, at: &str) -> String {
    let id = at.split([' ', '.', ',', ':']).next().unwrap_or("");
    let is_id = id.split_once('-').is_some_and(|(a, b)| !a.is_empty() && a.chars().all(|c| c.is_ascii_uppercase()) && !b.is_empty() && b.chars().all(|c| c.is_ascii_digit()));
    match p.labels.get(id).filter(|_| is_id) {
        Some(l) => format!("{l} · {at}"),
        None => at.to_string(),
    }
}

pub(crate) fn is_text(t: &Type) -> bool {
    match t {
        Type::Text => true,
        Type::Opt(t) => is_text(t),
        _ => false,
    }
}

/// Whether a field of type T is a list of texts (`[Text]`, `[Text]?`), which he writes one item a line: a text holds
/// commas, and coordd splits a list given as one string on its commas (`Type::coerce`).
fn is_text_list(t: &Type) -> bool {
    match t {
        Type::List(x) => is_text(x),
        Type::Opt(t) => is_text_list(t),
        _ => false,
    }
}

/// Field T as the answer carries it, from what its input holds (TYPED): a list of texts as its non-empty lines, each
/// trimmed, and `[]` when none (a `[Text]?` left empty: nothing, so null); any other field its words trimmed, which
/// coordd reads as T (`Type::coerce`), and nothing when empty (proto-lead N-1446 p346.2).
fn field_value(t: &Type, typed: &str) -> Option<Value> {
    if is_text_list(t) {
        let items: Vec<Value> = typed.lines().map(str::trim).filter(|l| !l.is_empty()).map(|l| json!(l)).collect();
        return (!items.is_empty() || !matches!(t, Type::Opt(_))).then_some(Value::Array(items));
    }
    let typed = typed.trim();
    (!typed.is_empty()).then(|| json!(typed))
}

/// The input of field F (of type T) of ticket N's answer, named F above it (an input's LABEL is its accessible name
/// only, so three lists would otherwise read as three unnamed boxes): a list of texts is a text area, one item a line
/// (`field_value`); any other field one line, with HINT.
fn field_input(pg: &Page, n: u64, f: &str, t: &Type, label: String, hint: Option<String>, k: &Kit) -> El {
    let edit = pg.answer_fields.get(&(n, f.to_string())).cloned().unwrap_or_else(|| TextEdit::new(""));
    let fk = (n, f.to_string());
    let input = if is_text_list(t) {
        let hint = if matches!(t, Type::Opt(_)) { "one per line, optional" } else { "one per line" };
        TextArea::new(&edit).label(label).width(None).placeholder(hint).auto_grow(2, 8).on_edit(move |s: &mut App| s.tickets.answer_fields.entry(fk.clone()).or_insert_with(|| TextEdit::new(""))).build(k)
    } else {
        let mut i = TextInput::new(&edit).label(label).width(None);
        if let Some(h) = hint {
            i = i.placeholder(h);
        }
        i.on_edit(move |s: &mut App| s.tickets.answer_fields.entry(fk.clone()).or_insert_with(|| TextEdit::new(""))).build(k)
    };
    col().w_full().gap(4.0).child(text(f.to_string()).font(k.ty.small).color(k.c.text_muted)).child(input)
}

/// The answer box of a step whose answer is no choice of results (a record, a number, yes or no, a list, any value):
/// its fields, or one input, and Answer, so no step sends him to a terminal (the owner, N-600; paper-research N-583.p6).
fn plain_box(pg: &Page, v: &TicketView, head: &Type, k: &Kit) -> El {
    let n = v.n;
    let mut c = col().w_full().gap(8.0);
    let go = |label: &str| Button::new(label.to_string()).variant(ButtonVariant::Neutral);
    match head {
        Type::Bool => {
            let yes = go("Yes").on_press(move |s: &mut App| answer_plain(s, n, Some(json!(true)))).build(k);
            let no = go("No").on_press(move |s: &mut App| answer_plain(s, n, Some(json!(false)))).build(k);
            return c.child(row().w_full().gap(8.0).justify_end().child(yes).child(no));
        }
        Type::Record(fs) => {
            for (f, t) in fs {
                c = c.child(field_input(pg, n, f, t, f.clone(), type_hint(t), k));
            }
        }
        _ if plain_value(head, "", &|_| String::new()).is_none() => {
            // a bare word (Text, an agent, a ticket, a commit) reads as a result's name on the way in: not answerable yet
            return c.child(text(format!("This step takes a {head}, which no answer can give yet.")).font(k.ty.small).color(k.c.text_muted).wrap());
        }
        _ => {
            let edit = pg.answer.get(&n).cloned().unwrap_or_else(|| TextEdit::new(""));
            let hint = type_hint(head).unwrap_or_else(|| "Your answer".into());
            c = c.child(col().key(format!("answer-{}", v.id)).w_full().child(TextArea::new(&edit).width(None).placeholder(hint).auto_grow(1, 12).on_edit(move |s: &mut App| s.tickets.answer.entry(n).or_insert_with(|| TextEdit::new(""))).build(k)));
        }
    }
    c.child(row().w_full().justify_end().child(go("Answer").on_press(move |s: &mut App| answer_plain(s, n, None)).build(k)))
}

/// The answer value for a step of type HEAD from TYPED (the one input) or FIELD (a record's inputs), as coordd reads it
/// (`Type::coerce`, then `conforms`); None for a type a request cannot carry: a bare string is read as a result's name.
fn plain_value(head: &Type, typed: &str, field: &dyn Fn(&str) -> String) -> Option<Value> {
    let typed = typed.trim();
    Some(match head {
        Type::Record(fs) => {
            let mut o = Map::new();
            o.insert("head".into(), json!("Answer"));
            for (f, t) in fs {
                if let Some(x) = field_value(t, &field(f)) {
                    o.insert(f.clone(), x);
                }
            }
            Value::Object(o)
        }
        Type::Int => typed.parse::<i64>().map_or_else(|_| json!(typed), |i| json!(i)),
        Type::Bool => json!(matches!(typed, "yes" | "true")),
        // the words go as the answer's text
        Type::Any => json!({"head": "Answer"}),
        // a list or a tuple as JSON, else a list's items split on commas (coordd reads each as its type)
        Type::List(_) | Type::Tuple(_) => match serde_json::from_str::<Value>(typed) {
            Ok(a @ Value::Array(_)) => a,
            _ if matches!(head, Type::List(_)) => Value::Array(typed.split(',').map(str::trim).filter(|x| !x.is_empty()).map(|x| json!(x)).collect()),
            _ => json!(typed),
        },
        Type::Opt(t) if typed.is_empty() && !matches!(**t, Type::Record(_)) => Value::Null,
        Type::Opt(t) => return plain_value(t, typed, field),
        _ => return None,
    })
}

/// Answer ticket N's step that takes no result: with VALUE, else what its box holds.
fn answer_plain(s: &mut App, n: u64, value: Option<Value>) {
    let Some(v) = s.proj().and_then(|p| p.live.tickets.get(&n)).cloned() else { return };
    let ViewStatus::Awaits { head, .. } = &v.status else { return };
    let typed = s.tickets.answer.get(&n).map(|e| e.text.trim().to_string()).unwrap_or_default();
    let field = |f: &str| s.tickets.answer_fields.get(&(n, f.to_string())).map(|e| e.text.clone()).unwrap_or_default();
    let Some(value) = value.or_else(|| plain_value(head, &typed, &field)) else { return };
    let text = matches!(head, Type::Any).then_some(typed).filter(|t| !t.is_empty());
    let req = Request::Answer { t: v.id.clone(), head: value, text };
    s.request_in(s.cur, req, Pending::TicketDone { clear: Cleared::Answer(n) });
}

/// Answer ticket N's step with result C (its fields FS): the text box is its text, the inputs its other fields.
pub(crate) fn answer(s: &mut App, n: u64, c: &str, fs: &[(String, Type)]) {
    let typed = s.tickets.answer.get(&n).map(|e| e.text.trim().to_string()).unwrap_or_default();
    let Some(req) = answer_request(s, n, c, fs, &typed) else { return };
    s.request_in(s.cur, req, Pending::TicketDone { clear: Cleared::Answer(n) });
}

/// The scene `tickets-answer`: ticket N's step answered as the page's button for result C does, with TYPED in the text
/// box, sent at once; the reply. It answers for real: for showing the owner's answer box at work in the dev copy.
pub fn scene_answer(s: &mut App, n: u64, c: &str, typed: &str) -> Result<String, String> {
    let fs = match s.proj().and_then(|p| p.live.tickets.get(&n)).map(|v| v.status.clone()) {
        Some(ViewStatus::Awaits { head, .. }) => head.constructor(c).ok_or(format!("{c} is not one of the step's results"))?.to_vec(),
        _ => return Err(format!("ticket {n} is not waiting on a step")),
    };
    s.tickets.answer.insert(n, TextEdit::new(typed));
    let req = answer_request(s, n, c, &fs, typed.trim()).ok_or("no such ticket")?;
    s.projects.get(s.cur).ok_or("no project")?.ask_once(&req)
}

/// The answer the page sends for ticket N's step with result C (its fields FS), TYPED his words.
pub(crate) fn answer_request(s: &App, n: u64, c: &str, fs: &[(String, Type)], typed: &str) -> Option<Request> {
    let typed = typed.to_string();
    let mut head = Map::new();
    head.insert("head".into(), json!(c));
    let texts = fs.iter().filter(|(_, t)| is_text(t)).count();
    for (f, t) in fs {
        if is_text(t) {
            // a sole text field is filled from the text by coordd; more than one each take it
            if texts > 1 && !typed.is_empty() {
                head.insert(f.clone(), json!(typed));
            }
        } else if let Some(x) = field_value(t, s.tickets.answer_fields.get(&(n, f.clone())).map_or("", |e| e.text.as_str())) {
            head.insert(f.clone(), x);
        }
    }
    let t = s.proj().and_then(|p| p.live.tickets.get(&n)).map(|v| v.id.clone())?;
    Some(Request::Answer { t, head: Value::Object(head), text: (!typed.is_empty()).then_some(typed) })
}

/// The results of the step H awaits, as the actions of the one box of TARGET (N-449.p6: the step's results are the
/// box's actions): each answers with the box's words (and the step's fields on its card); and the box's placeholder.
pub(crate) fn step_actions(_s: &App, h: &TicketView, target: &crate::message_box::Target, k: &Kit) -> (Vec<El>, String) {
    let ViewStatus::Awaits { call, head: Type::Variant(cs), args, .. } = &h.status else { return (Vec::new(), String::new()) };
    let considered = args.get("answer").is_some() && args.get("reasoning").is_some();
    let placeholder = if considered {
        "Or your own words"
    } else if call == "stopped" {
        "Why, if you end it: one sentence the record keeps"
    } else {
        "Your answer: why, a note, what the next one should know"
    };
    let n = h.n;
    let mut out = Vec::new();
    for (cname, fs) in cs {
        let (cn, fs, to) = (cname.clone(), fs.clone(), target.clone());
        let label = if considered && cname == "Answer" { "Answer in my words".to_string() } else { cname.clone() };
        out.push(Button::new(label).variant(ButtonVariant::Neutral).on_press(move |s: &mut App| answer_from_box(s, &to, n, &cn, &fs)).build(k));
    }
    (out, placeholder.into())
}

/// His step N answered with result C (its fields FS) and the words of the box of TARGET; the box empties when coordd
/// takes it, and its answer is said under the box, where he pressed.
fn answer_from_box(s: &mut App, target: &crate::message_box::Target, n: u64, c: &str, fs: &[(String, Type)]) {
    let key = (target.project(), target.place());
    let typed = s.composers.get(&key).map(|e| e.text.trim().to_string()).unwrap_or_default();
    let Some(req) = answer_request(s, n, c, fs, &typed) else { return };
    s.box_said.remove(&key);
    if !s.request_in(key.0, req, Pending::TicketDone { clear: Cleared::Step(key.0, key.1.clone(), n) }) {
        let why = s.notice.take().unwrap_or_else(|| "not sent".into());
        s.box_said.insert(key, Err(why));
    }
}

#[cfg(test)]
mod question_tests {
    use super::*;

    fn q(n: u64, opened: &str, status: Value, posts: Value, ask: Value) -> TicketView {
        serde_json::from_value(json!({"id": format!("Q-{n}"), "n": n, "kind": "question", "opener": "w1", "ctx": {}, "form": {"ask": ask, "context": "about it-7"},
            "title": "", "status": status, "posts": posts, "children": [], "program": "", "bound": 4, "spent": 2, "given": [], "opened_at": opened}))
        .unwrap()
    }

    fn answer(by: &str) -> Value {
        json!([{"n": 1, "at": "2026-11-12T10:00:00Z", "by": "w1", "text": "asked"},
               {"n": 2, "at": "2026-11-12T10:05:00Z", "by": by, "text": "yes, the median", "head": {"head": "Answer", "text": "yes, the median"}, "step": 2}])
    }

    // paper-research m1591: Decisions lists the question tickets the owner answered (not an open one, not one the lead
    // answered), newest first; an item's page the closed ones that name it, by the mail's rule; their words are read
    #[test]
    fn decisions_and_an_items_page_list_the_question_tickets() {
        let answered = json!({"is": "done", "result": {"head": "Answered", "text": "yes, the median"}});
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        let loc = json!({"loc": {"file": "texts.jsonl", "offset": 10, "len": 5}});
        for v in [
            q(1, "2026-11-12T09:00:00Z", answered.clone(), answer("owner"), json!("mean or median?")),
            q(2, "2026-11-12T11:00:00Z", answered.clone(), answer("owner"), loc.clone()),
            q(3, "2026-11-12T12:00:00Z", answered.clone(), answer("lead"), json!("by the lead")),
            q(4, "2026-11-12T13:00:00Z", json!({"is": "resting"}), json!([]), json!("still open, it-7")),
            q(5, "2026-11-12T14:00:00Z", json!({"is": "done", "result": {"head": "Dismissed", "why": "asked twice"}}), json!([]), json!("again")),
        ] {
            p.live.tickets.insert(v.n, v);
        }
        let ids = |vs: Vec<&TicketView>| vs.into_iter().map(|v| v.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(answered_questions(&p)), ["Q-2", "Q-1"]);
        assert_eq!(question_state(&p.live.tickets[&4]), "open");
        assert_eq!(question_state(&p.live.tickets[&5]), "dismissed");
        let mut pg = Page::default();
        // an ask that is a place is read (once), and its words are then matched
        assert_eq!(questions_wanted(&pg, &p).iter().map(key).collect::<Vec<_>>(), ["texts.jsonl:10:5"]);
        assert_eq!(question_words(&pg, &p.live.tickets[&2]).0, "…");
        pg.texts.insert("texts.jsonl:10:5".into(), Some(Ok("which it-9 test?".into())));
        assert!(questions_wanted(&pg, &p).is_empty());
        assert_eq!(question_words(&pg, &p.live.tickets[&1]), ("mean or median?".into(), "about it-7".into(), "yes, the median".into()));
        // it-7 is in every context: the closed ones, the open Q-4 not; it-9 only in Q-2's ask
        assert_eq!(ids(item_questions(&pg, &p, "it-7")), ["Q-5", "Q-3", "Q-2", "Q-1"]);
        assert_eq!(ids(item_questions(&pg, &p, "it-9")), ["Q-2"]);
        assert_eq!(owners_answer(&p.live.tickets[&1]).map(|x| x.at.as_str()), Some("2026-11-12T10:05:00Z"));
    }
}

#[cfg(test)]
mod plain_answer_tests {
    use super::*;

    // no step sends him to a terminal (N-600): what the box sends for a step that takes no result is what coordd takes
    #[test]
    fn a_step_that_takes_no_result_is_answered_from_its_box() {
        let none = |_: &str| String::new();
        let takes = |head: &Type, v: &Value| head.coerce(v).and_then(|h| head.conforms(&h).map(|()| h));
        let rec = Type::Record(vec![("why".into(), Type::Text), ("count".into(), Type::Int), ("ok".into(), Type::Opt(Box::new(Type::Bool)))]);
        let field = |f: &str| match f {
            "why" => "because".to_string(),
            "count" => " 3 ".to_string(),
            _ => String::new(),
        };
        let v = plain_value(&rec, "", &field).unwrap();
        assert_eq!(takes(&rec, &v).unwrap(), json!({"head": "Answer", "why": "because", "count": 3}));
        assert_eq!(takes(&Type::Int, &plain_value(&Type::Int, " 12 ", &none).unwrap()).unwrap(), json!(12));
        assert!(takes(&Type::Int, &plain_value(&Type::Int, "twelve", &none).unwrap()).is_err(), "a word for a number is refused by coordd, not sent as 0");
        assert_eq!(takes(&Type::Bool, &plain_value(&Type::Bool, "yes", &none).unwrap()).unwrap(), json!(true));
        let list = Type::List(Box::new(Type::Int));
        assert_eq!(takes(&list, &plain_value(&list, "1, 2,3", &none).unwrap()).unwrap(), json!([1, 2, 3]));
        assert_eq!(takes(&list, &plain_value(&list, "[4, 5]", &none).unwrap()).unwrap(), json!([4, 5]));
        let pair = Type::Tuple(vec![Type::Int, Type::Bool]);
        assert_eq!(takes(&pair, &plain_value(&pair, "[1, true]", &none).unwrap()).unwrap(), json!([1, true]));
        assert!(takes(&Type::Any, &plain_value(&Type::Any, "anything", &none).unwrap()).is_ok());
        let opt = Type::Opt(Box::new(Type::Int));
        assert_eq!(takes(&opt, &plain_value(&opt, "", &none).unwrap()).unwrap(), Value::Null);
        // a bare word reads as a result's name on the way in: no box offers to send one
        for t in [Type::Text, Type::Agent, Type::Ticket, Type::Ref] {
            assert_eq!(plain_value(&t, "x", &none), None, "{t}");
        }
        // a record's list of texts: one item a line, as the step card's fields (`field_value`)
        let notes = Type::Record(vec![("done".into(), Type::List(Box::new(Type::Text)))]);
        let lines = |_: &str| "the lock, then the runner\n\n38 first\n".to_string();
        assert_eq!(takes(&notes, &plain_value(&notes, "", &lines).unwrap()).unwrap(), json!({"head": "Answer", "done": ["the lock, then the runner", "38 first"]}));
    }

    /// What a field's input holds, as the answer carries it (proto-lead N-1446 p346.2): a list of texts one item a
    /// line, each trimmed, a comma inside an item kept; `[]` for one left empty, nothing for a `[Text]?` left empty; any
    /// other field as before, its words for coordd to read (a list of agents still split on commas there).
    #[test]
    fn a_list_of_texts_is_written_one_item_a_line() {
        let texts = Type::List(Box::new(Type::Text));
        let maybe = Type::Opt(Box::new(texts.clone()));
        assert_eq!(field_value(&texts, " a, b \n\n  c\n"), Some(json!(["a, b", "c"])));
        assert_eq!(field_value(&texts, " \n "), Some(json!([])));
        assert_eq!(field_value(&maybe, ""), None);
        assert_eq!(field_value(&maybe, "one\ntwo"), Some(json!(["one", "two"])));
        let agents = Type::List(Box::new(Type::Agent));
        assert_eq!(field_value(&agents, " lead, fixer "), Some(json!("lead, fixer")));
        assert_eq!(field_value(&Type::Int, " 3 "), Some(json!("3")));
        assert_eq!(field_value(&Type::Int, "  "), None);
        assert!(is_text_list(&maybe) && !is_text_list(&agents) && !is_text_list(&Type::Text));
    }

    /// A step whose result holds lists of texts, as rejoin's join does when it is passed up to him (`Settled(decided:
    /// [Text], questions: [Text], promised: [Text])`, consult-3 N-1446 p337): ticket 200 waits on him, 201 on the agent
    /// w2, the same call; made up.
    fn joins() -> docket_machine::Machine {
        use docket_machine::{Defs, Input, Machine};
        const CALLS: &str = "call join(asked: Text) -> Settled(decided: [Text], questions: [Text], promised: [Text]) | NotYet(why: Text)\n  task join\n";
        let mut named = Default::default();
        let calls = docket_machine::syntax::parse_calls("join.call", CALLS, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
        let kind = |name: &str, by: &str| {
            let k = format!(
                "kind {name}\n  form asked: Text\n  result Joined(decided: [Text], questions: [Text], promised: [Text]) | Later(why: Text)\n  pattern\n    j <- step {by} join(asked);\n    match j {{\n      Settled(decided, questions, promised) => return Joined(decided, questions, promised)\n      NotYet(why) => return Later(why)\n    }}\n"
            );
            (format!("{name}.kind"), k)
        };
        let (defs, errors) = Defs::build(&[kind("settle", "owner"), kind("agree", "w2")], calls, named, 200);
        assert!(errors.is_empty(), "{errors:?}");
        let mut m = Machine::load(defs);
        for kind in ["settle", "agree"] {
            let open = Input::Open { kind: kind.into(), under: None, opener: "owner".into(), form: json!({"asked": "what was settled on R-204"}), text: None, names: vec![], told: vec![] };
            m.apply(open, "2026-11-20T22:40:00Z".into()).reply.unwrap();
        }
        m
    }

    /// The made-up lists he types into ticket 200's card: blank lines and spaces around them, a comma inside an item.
    const TYPED: [(&str, &str); 3] = [
        ("decided", "the lock is the runner's\n  38 goes first, then 0.3\n\nthe brain stays unbound\n"),
        ("questions", "who writes research's policy lines\nwhether Wire shows coordd's own calls\nwhen, after 38, the install"),
        ("promised", "the stills tonight\nthe counter in train-two\nE1's fifth commit"),
    ];

    /// His page on ticket T of M, as coordd sends it, with TYPED in its answer fields.
    fn his_page(m: &docket_machine::Machine, t: u64, typed: &[(&str, &str)]) -> (std::sync::MutexGuard<'static, ()>, App) {
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::view_cost::project_for_tests();
        let v: TicketView = serde_json::from_value(serde_json::to_value(m.view(t).unwrap()).unwrap()).unwrap();
        p.live.tickets.insert(t, v);
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = Some(t);
        for (f, x) in typed {
            s.tickets.answer_fields.insert((t, f.to_string()), TextEdit::new(*x));
        }
        (held, s)
    }

    /// His step with lists of texts (proto-lead N-1446 p346.2, consult-3 p337), answered on his page with three lines
    /// typed into each list: `answer_request` sends each list one item a line, and the machine takes it as it takes
    /// the agent w2's `Settled` of the same lists: both tickets come to the same result. His card names each list above
    /// its box. A list he leaves empty goes as `[]`, which the step takes too.
    #[test]
    fn his_answer_with_lists_of_texts_goes_on_as_an_agents_does() {
        use docket_machine::{Input, Machine, Text};
        let mut m = joins();
        let (his, lead) = (200, 201);
        let (held, s) = his_page(&m, his, &TYPED);
        let ViewStatus::Awaits { head, agent, .. } = &s.proj().unwrap().live.tickets[&his].status else { panic!() };
        assert_eq!(agent, "owner");
        let fs = head.constructor("Settled").unwrap().to_vec();
        let Some(Request::Answer { head: his_head, text, .. }) = answer_request(&s, his, "Settled", &fs, "settled, as below") else { panic!() };
        let lists = json!({"head": "Settled",
            "decided": ["the lock is the runner's", "38 goes first, then 0.3", "the brain stays unbound"],
            "questions": ["who writes research's policy lines", "whether Wire shows coordd's own calls", "when, after 38, the install"],
            "promised": ["the stills tonight", "the counter in train-two", "E1's fifth commit"]});
        assert_eq!(his_head, lists);
        assert_eq!(text.as_deref(), Some("settled, as below"));
        let at = |n: u32| format!("2026-11-20T22:41:{n:02}Z");
        m.apply(Input::Answer { t: his, by: "owner".into(), head: his_head, text: Text::New(text.unwrap()) }, at(1)).reply.unwrap();
        m.apply(Input::Answer { t: lead, by: "w2".into(), head: lists, text: Text::New("settled, as below".into()) }, at(2)).reply.unwrap();
        let result = |m: &Machine, t: u64| match m.view(t).unwrap().status {
            ViewStatus::Done { result } => result,
            x => panic!("{x:?}"),
        };
        assert_eq!(result(&m, his), result(&m, lead));
        assert_eq!(result(&m, his)["decided"][1], json!("38 goes first, then 0.3"));
        // his card names each list above its box
        let mut rt = crate::view_cost::window(s);
        for _ in 0..3 {
            rt.frame(0.016);
        }
        let shown: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).collect();
        for f in ["decided", "questions", "promised"] {
            assert!(shown.iter().any(|x| x == f), "{f}: {shown:?}");
        }
        // nothing promised: the field left empty is `[]`, and the step takes it
        drop((rt, held));
        let mut m = joins();
        let (_held, s) = his_page(&m, his, &TYPED[..2]);
        let Some(Request::Answer { head: h, .. }) = answer_request(&s, his, "Settled", &fs, "") else { panic!() };
        assert_eq!(h["promised"], json!([]));
        m.apply(Input::Answer { t: his, by: "owner".into(), head: h, text: Text::New(String::new()) }, at(3)).reply.unwrap();
        assert_eq!(result(&m, his)["promised"], json!([]));
    }

    /// His card for that step, two lists typed and one empty (made up), in `STILL_OUT`: `STILL_OUT=DIR yaaihh-tests
    /// ask::plain_answer_tests::answer_stills --ignored --exact`.
    #[test]
    #[ignore]
    fn answer_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        // two lists typed and the third left empty, so its hint shows; without the test app's projects.toml bar
        fn lists() -> App {
            let m = joins();
            let mut s = his_page(&m, 200, &TYPED[..2]).1;
            s.notice = None;
            s
        }
        let script = out.join("answer-lists.json");
        std::fs::write(&script, "[]").unwrap();
        let snap = out.join("answer-lists.png").display().to_string();
        let script = script.display().to_string();
        let a: [&str; 8] = ["--snap", &snap, "--size", "1400x900", "--at", "1", "--script", &script];
        let spec = rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), lists, crate::view) };
        rune::App::new(spec).args(a.map(String::from).to_vec()).try_run().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message_box::{COMMENT_PLACE, Target, Way};
    use serde_json::json;

    /// A made-up project: thread N-687 with paper-research's post, thread N-575, the agent paper-research with a mail
    /// of its in the chat (m12), and the window on N-687.
    fn app() -> (std::sync::MutexGuard<'static, ()>, App) {
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        let at = |m: u32| format!("2026-11-19T08:{m:02}:00Z");
        let t = |id: &str, n: u64, title: &str, posts: serde_json::Value| -> docket_machine::TicketView {
            serde_json::from_value(json!({"id": id, "n": n, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
                "title": title, "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": at(1), "given": [], "posts": posts}))
            .unwrap()
        };
        let a = t("N-687", 687, "Window pieces tonight", json!([
            {"n": 1, "at": at(1), "by": "paper-research", "text": "Three pieces are open for the window: agda-dev, runner rows, autostart. Runner rows can go in first.", "told": ["owner"]},
            {"n": 2, "at": at(5), "by": "owner", "text": "agda-dev first.", "names": ["paper-research"]}]));
        let b = t("N-575", 575, "Runner rows", json!([{"n": 1, "at": at(0), "by": "owner", "text": "the runner's jobs as rows", "names": ["paper-research"]}]));
        for (seq, v) in [(1, a), (2, b)] {
            p.live.apply(coord_types::Event { seq, body: coord_types::Body::Ticket(v) });
        }
        p.live.agents.push(serde_json::from_value(json!({"id": "AGT-001", "name": "paper-research", "parent": "owner", "kind": "claude", "model": "opus", "state": "idle",
            "since": at(0), "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null})).unwrap());
        let mail = "<channel source=\"coord\" from=\"paper-research\" id=\"m12\">Runner rows: the queue first, or the history first?</channel>";
        let c = p.convs.entry("paper-research".into()).or_default();
        c.entries = vec![serde_json::from_value(json!({"offset": 10, "at": at(2), "kind": "mail", "text": mail})).unwrap()];
        c.keys = vec!["e10-0".into()];
        c.loaded = true;
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = Some(687);
        (held, s)
    }

    /// His comment on "Runner rows can go in first." of N-687's first post, its box open with NOTE.
    fn commenting(s: &mut App, note: &str) {
        let v = s.projects[0].live.tickets.get(&687).cloned().unwrap();
        let c = crate::threads::scene_comment(s, &v, 1, "Runner rows can go in first.", "").expect("the passage is in the post");
        write(s, Picked { target: crate::message_box::thread_place("N-687"), source: c.source, label: c.label, quote: c.quote, at: c.at, pi: 0, anchor: None, block: None, pane: None, texts: Vec::new() }, note);
    }

    #[test]
    fn a_comment_is_written_in_the_one_message_box_and_kept_when_he_comes_back() {
        let (_held, mut s) = app();
        commenting(&mut s, "");
        let key = (0usize, COMMENT_PLACE.to_string());
        assert!(s.composers.get(&key).is_some_and(|e| e.focused), "the box opens with the keyboard");
        assert!(!crate::message_box::is_agent_place(COMMENT_PLACE), "no agent is told he writes to it");
        s.composers.get_mut(&key).unwrap().replace_text("yes, tihs first");
        // another thread, then back: the words wait in its place
        s.tickets.shown = Some(575);
        s.tickets.shown = Some(687);
        assert_eq!(writing(&s).map(|e| e.text.as_str()), Some("yes, tihs first"));
        // the box's mentions reach it as any box's
        assert_eq!(Field::Composer(0, COMMENT_PLACE.into()).edit(&mut s).map(|e| e.text.clone()).as_deref(), Some("yes, tihs first"));
        // its spelling is the box's: the same marks, by its place
        if crate::spell::available() {
            s.spell.on = true;
            let e = s.composers.get(&key).cloned().unwrap();
            let marked: Vec<&str> = crate::spell::marked_box(&s, &key, &e).into_iter().map(|r| &e.text[r]).collect();
            assert!(marked.contains(&"tihs"), "{marked:?}");
        }
        // its Send, Add comment, keeps it with his next message on the thread (his walk item 11): nothing goes
        crate::message_box::send(&mut s, &Target::Comment { project: 0 });
        assert!(s.notes.picked.is_none() && !s.composers.contains_key(&key), "the box closes at the press");
        assert!(s.outgoing.list.is_empty(), "nothing goes at Add comment");
        let thread = (0usize, "thread:N-687".to_string());
        assert_eq!(held(&s, 0, &thread.1).iter().map(|c| (c.quote.as_str(), c.note.as_str())).collect::<Vec<_>>(), [("Runner rows can go in first.", "yes, tihs first")]);
        // the thread's Send: one reply on the post, quoting the passage. No pipe in this test: it stands `not sent`
        // above the thread's box, to go by itself once the pipe is back
        crate::message_box::send(&mut s, &Target::Thread { project: 0, t: "N-687".into(), post: None, way: Way::Here });
        let o = s.outgoing.list.last().cloned().expect("his message, on its way");
        assert_eq!(o.key, thread, "it stands above the thread's box");
        assert_eq!(o.text, "> Runner rows can go in first.\n> (from paper-research's message #1)\n\nyes, tihs first");
        assert!(matches!(&o.req, Request::Say { follows: Some(f), .. } if f == "N-687.p1"), "a reply on the post: {:?}", o.req);
        assert!(matches!(o.is, crate::outgoing::Is::NotSent(_)) && o.unsent, "{:?}", o.is);
        assert!(held(&s, 0, &thread.1).is_empty(), "it went with it");
        // Take back: the comment held for the thread's box again, nothing else put in the box
        crate::outgoing::take_back(&mut s, o.id);
        assert!(s.outgoing.list.is_empty());
        assert_eq!(held(&s, 0, &thread.1).len(), 1, "held again");
        assert!(s.composers.get(&thread).is_none_or(|e| e.text.is_empty()));
        // a comment closed with his words in it (Esc): they stay as the box's draft; its passage pressed opens it again
        // with them
        commenting(&mut s, "yes, tihs first");
        close_mark(&mut s);
        assert!(s.notes.shut && writing(&s).is_some_and(|w| w.text == "yes, tihs first"), "kept, not sent, not lost");
        highlight_pressed(&mut s, NEW);
        assert!(!s.notes.shut && s.composers.get(&key).is_some_and(|e| e.focused), "open again, with the keyboard");
        // Cancel: gone
        unwrite(&mut s);
        assert!(s.notes.picked.is_none() && writing(&s).is_none() && s.outgoing.list.is_empty());
    }

    #[test]
    fn a_reply_in_a_mail_thread_is_written_in_the_one_message_box_and_a_failed_send_keeps_it() {
        let (_held, mut s) = app();
        let target = Target::Reply { project: 0, id: "m12".into(), to: "paper-research".into() };
        assert_eq!(target.place(), "m12", "its words are kept under the thread's id, as its comments are");
        assert_eq!(Field::Reply(0, "m12".into()).key(), "cf-0-m12");
        // nothing typed: nothing goes
        crate::message_box::send(&mut s, &target);
        assert!(!s.box_said.contains_key(&(0, "m12".into())));
        s.composers.insert((0, "m12".into()), TextEdit::new("the queue first"));
        // no link in a test: said under the box, the words stay
        crate::message_box::send(&mut s, &target);
        assert!(matches!(s.box_said.get(&(0, "m12".into())), Some(Err(_))), "{:?}", s.box_said);
        assert_eq!(s.composers.get(&(0, "m12".into())).map(|e| e.text.as_str()), Some("the queue first"));
    }

    /// Agent NAME of the test project, in PART (the policy's, "/" nesting), STATE.
    fn member(s: &mut App, name: &str, part: &str, state: &str) {
        s.projects[0].live.agents.push(serde_json::from_value(json!({"id": format!("AGT-{name}"), "name": name, "parent": "paper-research", "part": part, "kind": "claude", "model": "opus",
            "state": state, "since": "", "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null})).unwrap());
    }

    /// The test project with parts: Accounts (two running, one stopped) and Threads under "yaaihh project", Merge and
    /// release, Consultants, and a listed part nobody is in.
    fn team() -> (std::sync::MutexGuard<'static, ()>, App) {
        let (held, mut s) = app();
        member(&mut s, "accounts-lead", "yaaihh project/Accounts", "idle");
        member(&mut s, "accounts-2", "yaaihh project/Accounts", "working");
        member(&mut s, "accounts-3", "yaaihh project/Accounts", "stopped");
        member(&mut s, "docket-edge2", "yaaihh project/Threads", "idle");
        member(&mut s, "merger", "yaaihh project/Merge and release", "idle");
        member(&mut s, "consult-3", "Consultants", "idle");
        s.projects[0].live.parts = vec![("yaaihh project/Accounts".into(), 0), ("yaaihh project/Threads".into(), 1), ("Consultants".into(), 2), ("Empty part".into(), 3)];
        (held, s)
    }

    /// 0.2 B10 (the owner, N-1033.p1): `@team` and `@PART` reach each running agent of the group, as one tagged agent is
    /// reached; an agent's own name wins; nobody running, or no such name, stops the send.
    #[test]
    fn a_group_tag_reaches_each_running_agent_of_it() {
        let (_held, mut s) = team();
        let p = &s.projects[0];
        assert_eq!(tagged(p, &["@team please look"]).unwrap(), ["paper-research", "accounts-lead", "accounts-2", "docket-edge2", "merger", "consult-3"], "the stopped one is left out");
        // a part by its last name in any case, a name of several words with -, a top part with its nested parts' agents
        assert_eq!(tagged(p, &["@accounts and @Merge-and-release"]).unwrap(), ["accounts-lead", "accounts-2", "merger"]);
        assert_eq!(tagged(p, &["@yaaihh-project"]).unwrap(), ["accounts-lead", "accounts-2", "docket-edge2", "merger"]);
        // each once, an agent tagged by name beside its group and in a comment's note
        assert_eq!(tagged(p, &["@accounts-2 @Accounts", "@accounts"]).unwrap(), ["accounts-2", "accounts-lead"]);
        assert_eq!(tagged(p, &["@Empty-part"]), Err("not sent: nobody in @Empty-part is running".into()));
        assert_eq!(tagged(p, &["@nobody"]), Err("not sent: no agent or group named @nobody".into()));
        // his @all goes, naming nobody here: coordd reads it in the words as its own @all (N-1446 p1327); a name beside
        // it is still a name, and a tag the engine does not read as its @all is still refused
        for text in ["@all what is open?", "what is open? @all"] {
            assert_eq!(tagged(p, &[text]), Ok(vec![]), "{text}");
            assert!(docket_machine::at_name(text, docket_machine::ALL), "the engine reads {text:?} as its @all");
        }
        assert_eq!(tagged(p, &["@all and @accounts-2"]).unwrap(), ["accounts-2"]);
        assert_eq!(tagged(p, &["@allx"]), Err("not sent: no agent or group named @allx".into()));
        // the @ list: the team first, then the agents, then the parts, each with whom it reaches
        assert_eq!(suggestions(p, "").first(), Some(&("team".to_string(), "each of them answers · 6 running, 1 stopped not reached".to_string())));
        let acc = suggestions(p, "acc");
        assert_eq!(acc.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), ["accounts-2", "accounts-lead", "accounts-3", "Accounts"]);
        assert_eq!(acc[3].1, "each of them answers · 2 running, 1 stopped not reached");
        let tags: Vec<String> = groups(p).into_iter().map(|g| g.tag).collect();
        assert_eq!(tags, ["team", "yaaihh-project", "Accounts", "Threads", "Consultants", "Empty-part", "Merge-and-release"], "the sidebar's order, then the parts only an agent names");
        // the line above the box: whom it reaches besides, by name and count
        let chat = Target::Agent { project: 0, name: "paper-research".into() };
        assert_eq!(crate::message_box::with_groups(&s, &chat, "On: Runner rows · N-575".into(), "@team and @Accounts, see"), "New thread with paper-research + the team (6) + Accounts (2)", "others tagged make it a new thread");
        let sub = Target::Thread { project: 0, t: "N-687".into(), post: Some(1), way: Way::Subthread };
        assert_eq!(crate::message_box::with_groups(&s, &sub, "→ Window pieces tonight › post 1, as a subthread".into(), "@Accounts see"), "→ Window pieces tonight › post 1, as a subthread + Accounts (2)");
        assert_eq!(crate::message_box::with_groups(&s, &chat, "On: Runner rows · N-575".into(), "@accounts-2 see"), "New thread with paper-research + accounts-2", "an agent tagged makes one too");
        assert_eq!(crate::message_box::with_groups(&s, &chat, "On: Runner rows · N-575".into(), "@team see"), "New thread with paper-research + the team (6)");
        // the project's box: to whom it tags, not the clerk, nor the lead unless tagged (consult-3, N-1402.p2, 2)
        let project = Target::Intake { project: 0 };
        assert_eq!(crate::message_box::with_groups(&s, &project, "→ the project".into(), "@Accounts see"), "→ Accounts (2)");
        assert_eq!(crate::message_box::with_groups(&s, &project, "→ the project".into(), "no tag"), "→ the project");
        assert_eq!(crate::message_box::with_groups(&s, &Target::NewThread { project: 0 }, "→ a new thread".into(), "@team see"), "→ a new thread with the team (6)");
        // a post on a thread: coordd reads the group from GROUPS_PROTOCOL; an older coord takes none, and the line says none
        let here = Target::Thread { project: 0, t: "N-687".into(), post: None, way: Way::Here };
        assert_eq!(crate::message_box::with_groups(&s, &here, "→ Window pieces tonight".into(), "@team see"), "→ Window pieces tonight + the team (6)");
        // under his post: the group's agents it did not reach
        let at = "2026-11-19T08:07:00Z";
        assert_eq!(not_reached(p, "@Accounts look", &["accounts-lead", "accounts-2"], at), ["accounts-3"]);
        assert!(not_reached(p, "@Accounts look", &["accounts-lead", "accounts-2", "accounts-3"], at).is_empty(), "reached: none");
        assert!(not_reached(p, "@accounts-3 look", &[], at).is_empty(), "an agent tagged by name is no group");

        // what Send makes: from an agent's chat, its agent and the group's others
        s.composers.insert((0, "paper-research".into()), TextEdit::new("@Accounts the sign-in first"));
        let (req, _) = s.composed(0, "paper-research").expect("it goes");
        assert!(matches!(&req, Request::Say { to: Some(t), names, .. } if t == "paper-research" && names == &["accounts-lead", "accounts-2"]), "{req:?}");
        // from the project's box: to the group, its first as `to`; the lead only as one of the team
        s.composers.insert((0, "project".into()), TextEdit::new("@Accounts the sign-in first"));
        let req = crate::message_box::request(&s, &project).unwrap();
        assert!(matches!(&req, Request::Say { to: Some(t), names, .. } if t == "accounts-lead" && names == &["accounts-2"]), "{req:?}");
        s.composers.insert((0, "project".into()), TextEdit::new("@team all of you: the sign-in first"));
        let req = crate::message_box::request(&s, &project).unwrap();
        assert!(matches!(&req, Request::Say { to: Some(t), names, .. } if t == "paper-research" && names.len() == 5), "{req:?}");
        s.composers.insert((0, "project".into()), TextEdit::new("nobody tagged"));
        let req = crate::message_box::request(&s, &project).unwrap();
        assert!(matches!(&req, Request::Say { to: None, names, .. } if names.is_empty()), "none tagged: the clerk files it");
        // a subthread: its post's author, the group besides
        s.composers.insert((0, "thread:N-687".into()), TextEdit::new("@yaaihh-project this one"));
        let req = crate::message_box::request(&s, &sub).unwrap();
        assert!(matches!(&req, Request::Say { to: Some(t), names, under: Some(_), .. } if t == "paper-research" && names.len() == 4), "{req:?}");
        // a post on the thread: coordd reads the group (GROUPS_PROTOCOL); an older coord's is refused, and said why
        assert!(matches!(crate::message_box::request(&s, &here), Ok(Request::Post { .. })));
        s.projects[0].protocol = coord_types::GROUPS_PROTOCOL - 1;
        assert_eq!(crate::message_box::request(&s, &here).err().as_deref(), Some(NO_GROUP_ON_POST));
        assert_eq!(crate::message_box::with_groups(&s, &here, "→ Window pieces tonight".into(), "@team see"), "→ Window pieces tonight");
        s.composers.insert((0, "thread:N-687".into()), TextEdit::new("@merger this one"));
        assert!(crate::message_box::request(&s, &here).is_ok(), "an agent tagged in a post is coordd's to read, as before");
        s.projects[0].protocol = coord_types::PROTOCOL;

        // completed in a reply box: an agent joins its To row, a group stays a tag
        for (typed, chosen, added) in [("@Acc", "Accounts", None), ("@merg", "merger", Some(vec!["merger".to_string()]))] {
            s.notes.added.clear();
            let mut e = TextEdit::new(typed);
            e.set_caret(typed.len());
            s.composers.insert((0, "m12".into()), e);
            s.notes.mention = Some(Mention { field: Field::Reply(0, "m12".into()), at: 0, query: typed[1..].into(), active: 0 });
            complete(&mut s, chosen);
            assert_eq!(s.composers.get(&(0, "m12".into())).map(|e| e.text.clone()), Some(format!("@{chosen} ")));
            assert_eq!(s.notes.added.get("m12").cloned(), added);
        }

        // accounts-3 starts again: the post sent while it was stopped still did not reach it, and the line stays
        // (consult-3, N-1402.p4); an agent made after the post joined later and is not counted
        s.projects[0].live.agents.iter_mut().find(|a| a.name == "accounts-3").unwrap().state = "working".into();
        s.projects[0].live.agents.push(serde_json::from_value(json!({"id": "AGT-accounts-4", "name": "accounts-4", "parent": "paper-research", "part": "yaaihh project/Accounts", "kind": "claude",
            "model": "opus", "state": "idle", "since": "", "created": "2026-11-19T09:00:00Z", "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null})).unwrap());
        assert_eq!(not_reached(&s.projects[0], "@Accounts look", &["accounts-lead", "accounts-2"], at), ["accounts-3"]);

        // an agent named as a part is the agent: the part is not offered
        member(&mut s, "threads", "", "idle");
        let p = &s.projects[0];
        assert_eq!(tagged(p, &["@Threads"]).unwrap(), ["threads"]);
        assert!(!groups(p).iter().any(|g| g.tag == "Threads"));
    }

    /// Stills of 0.2 B10 (STILL_OUT): paper-research's chat with the team and a group tagged (the line above the box
    /// names whom it reaches), the @ list at `@acc` (the agents, then the group with its count), and his post to
    /// Accounts on a thread with the line under it naming the stopped one it did not reach.
    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn group_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn chat(typed: &str) -> App {
            let (_held, mut s) = team();
            s.sel = crate::Sel::Agent("paper-research".into());
            // the agent's chat, where its own box stands (the page opens on With you, whose box is the thread's)
            s.streams.which.insert((0, "paper-research".into()), crate::streams::Which::Work);
            let mut e = TextEdit::new(typed);
            e.set_caret(typed.len());
            e.focused = true;
            s.composers.insert((0, "paper-research".into()), e);
            s
        }
        fn tagged_box() -> App {
            chat("@team the sign-in comes first; @Accounts, yours is the card")
        }
        fn offered() -> App {
            let mut s = chat("the sign-in first, @acc");
            let at = "the sign-in first, ".len();
            s.notes.mention = Some(Mention { field: Field::Composer(0, "paper-research".into()), at, query: "acc".into(), active: 0 });
            s
        }
        fn posted() -> App {
            let (_held, mut s) = team();
            let mut v = s.projects[0].live.tickets.get(&687).cloned().unwrap();
            v.posts.push(serde_json::from_value(json!({"n": 3, "at": "2026-11-19T08:07:00Z", "by": "owner", "text": "@Accounts the sign-in first, then the cards", "names": ["accounts-lead", "accounts-2"]})).unwrap());
            s.projects[0].live.apply(coord_types::Event { seq: 3, body: coord_types::Body::Ticket(v) });
            s
        }
        let shots: [(&str, fn() -> App); 3] = [("group-line.png", tagged_box), ("group-offered.png", offered), ("group-not-reached.png", posted)];
        for (name, start) in shots {
            let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
            rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), start, crate::view) }).args(args).try_run().unwrap();
        }
    }

    /// Stills of the boxes this changed (STILL_OUT): the comment box on a thread, wide and in a narrow window; the end
    /// box; a mail's reply in an agent's chat.
    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn comment_box_still() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn comment() -> App {
            let (_held, mut s) = app();
            s.spell.on = true;
            commenting(&mut s, "yes, but tihs needs the history too; @paper-research");
            s
        }
        fn ending() -> App {
            let (_held, mut s) = app();
            s.spell.on = true;
            s.tickets.more.insert(687);
            s.tickets.ending.insert(687, crate::ending::Ending { how: 0 });
            s.composers.insert((0, crate::ending::place(687)), TextEdit::new("all three are in, runner rows frist"));
            s
        }
        fn reply() -> App {
            let (_held, mut s) = app();
            s.spell.on = true;
            s.sel = crate::Sel::Agent("paper-research".into());
            s.notes.replying.insert("m12".into());
            // the work behind the replies (chat.rs's transcript), where a mail stands with its thread and reply
            s.streams.which.insert((0, "paper-research".into()), crate::streams::Which::Work);
            s.composers.insert((0, "m12".into()), TextEdit::new("the queue frist, then the history"));
            s
        }
        let shots: [(&str, fn() -> App, &str); 4] =
            [("comment-box.png", comment, "1400x900"), ("comment-box-narrow.png", comment, "760x900"), ("end-box.png", ending, "1400x900"), ("reply-box.png", reply, "1400x900")];
        for (name, start, size) in shots {
            let (w, h) = size.split_once('x').map(|(w, h)| (w.parse::<f32>().unwrap(), h.parse::<f32>().unwrap())).unwrap();
            let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), size.into(), "--at".into(), "1.0".into()];
            rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (w, h), start, crate::view) }).args(args).try_run().unwrap();
        }
    }
}

#[cfg(test)]
mod mark_tests {
    //! A comment's mark pressed wherever it stands: its box at the mark, every comment on the passage oldest first with
    //! its answer, and the one message box for another, which goes where they went; Add comment holds it for the next
    //! Send from the box of that place, which takes it with his words as one message (0.2 line C15; the owner, N-616
    //! p15: "i cannot click it reply to it like i would expect intuitively"; N-1104 p617; walk item 11, N-1446 p3521).
    use super::*;
    use crate::message_box::{COMMENT_PLACE, Target};
    use serde_json::{Value, json};

    const PASSAGE: &str = "the lock is held 7–12 s";
    const MAIL: &str = "Runner rows: the queue first, or the history first?";
    const ASKED: &str = "the queue first, or the history first?";

    fn at(m: u32) -> String {
        format!("2026-11-20T09:{m:02}:00Z")
    }

    /// Thread N-7: paper-research's post 2, his comment on its passage sent as post 3, and the answer, post 4.
    fn thread() -> docket_machine::TicketView {
        let c = Comment { source: "N-7.p2".into(), label: "N-7.p2, paper-research's message at 10:01".into(), quote: PASSAGE.into(), note: "by whom?".into(), at: Vec::new() };
        let (text, _) = compose(&[c], "");
        serde_json::from_value(json!({"id": "N-7", "n": 7, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"}, "title": "the lock",
            "status": {"is": "resting"}, "children": [], "program": "", "bound": 5, "spent": 3, "opened_at": at(0), "given": [],
            "posts": [{"n": 1, "at": at(0), "by": "owner", "text": "why is it slow?", "names": ["paper-research"], "told": ["paper-research"]},
                      {"n": 2, "at": at(1), "by": "paper-research", "text": format!("Found it: {PASSAGE} on every start."), "told": ["owner"]},
                      {"n": 3, "at": at(2), "by": "owner", "text": text, "answers": 2, "told": ["paper-research"]},
                      {"n": 4, "at": at(3), "by": "paper-research", "text": "> by whom?\n\nthe follower's first round\n\nit waits for the leader", "answers": 3}]}))
        .unwrap()
    }

    fn mail(id: &str, from: &str, to: &str, re: Option<&str>, body: &str, quotes: Value, m: u32) -> MailView {
        serde_json::from_value(json!({"id": id, "at": at(m), "from": from, "to": [to], "subject": "", "body": body, "esc": false, "delivered": [], "re": re, "quotes": quotes})).unwrap()
    }

    /// The window on thread N-7, with a mail thread beside it (paper-research's m12, his comment on it in m13, the answer
    /// m14), its requests written to OUT.
    fn app(out: &std::path::Path) -> (std::sync::MutexGuard<'static, ()>, App, std::process::Child) {
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(thread()) });
        let c = Comment { source: "m12".into(), label: "m12".into(), quote: ASKED.into(), note: "the queue".into(), at: Vec::new() };
        let (body, _) = compose(&[c], "");
        let i = MAIL.find(ASKED).unwrap();
        let mails = [
            mail("m12", "paper-research", "owner", None, MAIL, json!([]), 4),
            mail("m13", "owner", "paper-research", Some("m12"), &body, json!([{"source": "m12", "text": ASKED, "at": [[i, i + ASKED.len()]]}]), 5),
            mail("m14", "paper-research", "owner", Some("m13"), "the queue it is", json!([]), 6),
        ];
        for (seq, m) in mails.into_iter().enumerate() {
            p.live.apply(coord_types::Event { seq: seq as u64 + 2, body: coord_types::Body::Mail(m) });
        }
        p.live.agents.push(serde_json::from_value(json!({"id": "AGT-001", "name": "paper-research", "parent": "owner", "kind": "claude", "model": "opus", "state": "idle",
            "since": at(0), "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null})).unwrap());
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = Some(7);
        let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        (held, s, child)
    }

    /// The requests of kind OP the window sent, in order.
    fn requests(s: &mut App, child: std::process::Child, out: &std::path::Path, op: &str) -> Vec<Value> {
        s.projects[0].link = None;
        let mut child = child;
        let _ = child.wait();
        std::fs::read_to_string(out).unwrap_or_default().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["req"]["op"] == op).map(|v| v["req"].clone()).collect()
    }

    fn out(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("yaaihh-marks-{name}-{}.jsonl", std::process::id()))
    }

    /// The words drawn in the comment box, in order.
    fn boxed(rt: &rune::runtime::Runtime<App>) -> Vec<String> {
        rt.elements().into_iter().filter(|e| !e.exiting && e.path.split('/').any(|k| k == "cm-box")).filter_map(|e| e.text.or(e.label)).collect()
    }

    /// Where an element is drawn; one playing its exit (rune lists it until the exit ends) is gone.
    fn drawn(rt: &rune::runtime::Runtime<App>, key: &str) -> Option<[f32; 4]> {
        rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).map(|e| e.rect)
    }

    fn settle(rt: &mut rune::runtime::Runtime<App>) {
        rt.invalidate();
        for _ in 0..3 {
            rt.frame(0.016);
        }
    }

    fn click(rt: &mut rune::runtime::Runtime<App>, x: f32, y: f32) {
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        rt.frame(0.016);
    }

    /// His words W in the comment box.
    fn write_note(s: &mut App, w: &str) {
        let mut e = TextEdit::new(w);
        e.focused = true;
        e.set_caret(w.len());
        s.composers.insert((0, COMMENT_PLACE.into()), e);
    }

    /// A press on the mark his sent comment left on a thread's post: the box at it with his note and the answer, and the
    /// one box for another, with the keyboard; Esc closes it; Add comment sends his new comment, as a reply on the post
    /// the passage is in.
    #[test]
    fn a_press_on_a_threads_mark_opens_its_comments_and_the_box() {
        let file = out("thread");
        let (_held, s, child) = app(&file);
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        rt.update(|s, _| highlight_pressed(s, "cp-N-7.p2-0"));
        settle(&mut rt);
        let t = boxed(&rt);
        assert!(drawn(&rt, "cm-box").is_some(), "its box: {t:?}");
        assert!(t.iter().any(|x| x == "by whom?"), "his note: {t:?}");
        assert!(t.iter().any(|x| x.starts_with("sent in #3")), "{t:?}");
        assert!(t.iter().any(|x| x == "paper-research: the follower's first round"), "the answer's first line: {t:?}");
        assert!(t.iter().any(|x| x == "Add comment") && !t.iter().any(|x| x == "Send now"), "one Send, the box's: {t:?}");
        let pk = rt.state.notes.picked.clone().expect("a new comment on the passage");
        assert_eq!((pk.target.as_str(), pk.source.as_str(), pk.quote.as_str()), ("thread:N-7", "N-7.p2", PASSAGE));
        assert!(rt.state.composers.get(&(0, COMMENT_PLACE.to_string())).is_some_and(|e| e.focused), "the box has the keyboard");
        // the press itself: closed, then pressed on the mark where it is drawn then (a page that fills its column stays at
        // its end, so its posts move as the foot under them does), it opens again
        rt.update(|s, _| close_mark(s));
        settle(&mut rt);
        assert!(drawn(&rt, "cm-box").is_none(), "closed");
        let mark = drawn(&rt, "cp-N-7.p2-0").expect("the passage is marked");
        click(&mut rt, mark[0] + mark[2] / 2.0, mark[1] + mark[3] / 2.0);
        settle(&mut rt);
        assert!(drawn(&rt, "cm-box").is_some(), "a press on the mark opens its box");
        assert_eq!(rt.state.notes.open.as_deref(), Some("cp-N-7.p2-0"));
        // Esc closes it; nothing written, nothing kept
        rt.update(|s, _| write_note(s, ""));
        settle(&mut rt);
        rt.key(rune::KeyEvent { key: rune::Key::Escape, mods: Default::default() });
        settle(&mut rt);
        assert!(drawn(&rt, "cm-box").is_none() && rt.state.notes.picked.is_none(), "Esc closes it");
        assert!(rt.state.outgoing.list.is_empty(), "an empty box sends nothing");
        // Add comment: his second comment is held for the thread's next message, nothing sent; the thread's Send sends
        // it under the post the passage is in
        rt.update(|s, _| {
            open_mark(s, "cp-N-7.p2-0");
            write_note(s, "and how long each time?");
            crate::message_box::send(s, &Target::Comment { project: 0 });
        });
        assert!(rt.state.notes.picked.is_none() && rt.state.notes.open.is_none(), "the box closes at the press");
        assert!(rt.state.outgoing.list.is_empty() && held(&rt.state, 0, "thread:N-7").len() == 1, "held, nothing sent");
        rt.update(|s, _| crate::message_box::send(s, &Target::Thread { project: 0, t: "N-7".into(), post: None, way: crate::message_box::Way::Here }));
        let sent = requests(&mut rt.state, child, &file, "say");
        assert_eq!(sent.len(), 1, "{sent:?}");
        let text = sent[0]["text"].as_str().unwrap();
        assert!(text.starts_with(&format!("> {PASSAGE}\n> (from N-7.p2, paper-research's message at ")), "the same passage and its post: {text}");
        assert!(text.lines().nth(1).is_some_and(|l| l.ends_with("Z)")), "its time in UTC, as the agent reads every time: {text}");
        assert!(text.ends_with("\n\nand how long each time?"), "{text}");
        assert_eq!(sent[0]["follows"], "N-7.p2", "a reply on the post the passage is in");
        let _ = std::fs::remove_file(&file);
    }

    /// His writing in the comment box is said where the comment goes (proto-mind N-1446 p2359; lib.rs `writing_at`):
    /// his first word on a passage of N-7 says it on for thread N-7, so his own posts there wait while he writes it, as
    /// they do while he writes in the thread's box. Add comment closes the box and holds the comment for the thread's
    /// next message (his walk item 11): nothing goes, and with the box gone his writing is said off. Research's record
    /// before the on: 11 of his 11 comments on 24 Nov went with no writing line, each one 15 s after its press.
    #[test]
    fn his_writing_in_a_comment_holds_the_thread_it_goes_to_while_he_writes_it() {
        let file = out("writing");
        let (_held, mut s, child) = app(&file);
        s.spell.on = false;
        let mut rt = crate::view_cost::window(s);
        // the window's poll, which asks his writing again each frame
        rt.poll = Some(crate::poll);
        settle(&mut rt);
        rt.update(|s, _| open_mark(s, "cp-N-7.p2-0"));
        settle(&mut rt);
        let f = rt.elements().into_iter().find(|e| !e.exiting && e.role == "textbox" && e.path.split('/').any(|k| k == "cm-box")).map(|e| e.rect).expect("the comment box");
        click(&mut rt, f[0] + 12.0, f[1] + 12.0);
        let thread = (0usize, crate::message_box::thread_place("N-7"));
        // coordd's answer to the one request on its way that WHAT picks out
        let answer = |rt: &mut rune::runtime::Runtime<App>, what: &dyn Fn(&crate::Pending) -> bool| {
            let ids: Vec<u64> = rt.state.projects[0].pending.iter().filter(|(_, w)| what(w)).map(|(id, _)| *id).collect();
            assert_eq!(ids.len(), 1, "one on its way");
            rt.state.take(0, crate::link::Incoming::Reply(ids[0], coord_types::Reply { ok: true, out: "Posted on N-7 as post 5.".into(), post: None }));
        };
        let said = |w: &crate::Pending| matches!(w, crate::Pending::Writing { .. });
        let comment = |w: &crate::Pending| matches!(w, crate::Pending::TicketDone { clear: crate::threads::Cleared::Comment(..) });
        crate::view_cost::type_in(&mut rt, "by whom");
        assert_eq!(writing(&rt.state).map(|e| e.text.as_str()), Some("by whom"), "typed in the comment box");
        assert!(rt.state.writing.contains(&thread), "on for N-7 at his first word in the comment");
        answer(&mut rt, &said);
        // Add comment: the box closes, the comment held for the thread's next message; nothing goes, and his writing is
        // said off with the box gone
        rt.update(|s, _| crate::message_box::send(s, &Target::Comment { project: 0 }));
        settle(&mut rt);
        assert!(rt.state.notes.picked.is_none(), "the box closed at the press");
        assert!(!rt.state.projects[0].pending.values().any(comment) && held(&rt.state, 0, &thread.1).len() == 1, "held, nothing sent");
        assert!(!rt.state.writing.contains(&thread), "said off with the box closed");
        answer(&mut rt, &said);
        let writes: Vec<String> = requests(&mut rt.state, child, &file, "writing").iter().map(|r| format!("{} {} {}", r["agent"].as_str().unwrap_or(""), r["thread"].as_str().unwrap_or(""), r["on"])).collect();
        assert_eq!(writes, [" N-7 true", " N-7 false"], "on the thread, no agent");
        let _ = std::fs::remove_file(&file);
    }

    /// His walk item 11 (paper-research N-1446 p3521; before, from d90b000c, each comment went at its press): Add comment
    /// sends nothing, his comments wait over the thread's box, and its Send sends them with what he typed there as one
    /// message, each passage quoted with his note under it. The engine's answer goes to that message, and his next
    /// message from the box carries nothing of them. Refused, it stands `not sent · why`, and its Take back holds its
    /// comment for the box again, his words back in it.
    #[test]
    fn comments_go_with_his_next_send_as_one_message_and_nothing_more_after() {
        let file = out("press");
        let (_held, s, child) = app(&file);
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let thread = (0usize, crate::message_box::thread_place("N-7"));
        let to_thread = Target::Thread { project: 0, t: "N-7".into(), post: None, way: crate::message_box::Way::Here };
        let out_answer = |s: &mut App, ok: bool, out: &str| crate::threads::replied(s, crate::Pending::TicketDone { clear: crate::threads::Cleared::Out(0, "thread:N-7".into()) }, ok, out.into());
        // two comments: on post 2's passage (its mark), and on words of the answer, post 4
        rt.update(|s, _| {
            open_mark(s, "cp-N-7.p2-0");
            write_note(s, "and how long?");
            crate::message_box::send(s, &Target::Comment { project: 0 });
            let v = s.projects[0].live.tickets.get(&7).cloned().unwrap();
            let c = crate::threads::scene_comment(s, &v, 4, "the follower's first round", "").expect("the words are in post 4");
            write(s, Picked { target: crate::message_box::thread_place("N-7"), source: c.source, label: c.label, quote: c.quote, at: c.at, pi: 0, anchor: None, block: None, pane: None, texts: Vec::new() }, "on every start?");
            crate::message_box::send(s, &Target::Comment { project: 0 });
        });
        assert!(rt.state.outgoing.list.is_empty() && rt.state.notes.picked.is_none(), "Add comment sends nothing and closes the box");
        assert_eq!(held(&rt.state, 0, &thread.1).iter().map(|c| c.note.as_str()).collect::<Vec<_>>(), ["and how long?", "on every start?"]);
        settle(&mut rt);
        assert!(drawn(&rt, "held-0-thread:N-7").is_some(), "they stand over the thread's box");
        // his words in the thread's box, and its Send: one message
        rt.update(|s, _| {
            s.composers.insert(thread.clone(), TextEdit::new("and thanks"));
            crate::message_box::send(s, &to_thread);
        });
        assert_eq!(rt.state.outgoing.list.len(), 1, "one message");
        let o = rt.state.outgoing.list[0].clone();
        assert!(o.key == thread && o.flying && o.is == crate::outgoing::Is::Sending, "{:?}", o.is);
        assert!(held(&rt.state, 0, &thread.1).is_empty() && rt.state.composers.get(&thread).is_none_or(|e| e.text.is_empty()), "the box and its comments went");
        settle(&mut rt);
        assert!(drawn(&rt, &format!("out-0-thread:N-7-{}", o.id)).is_some() && drawn(&rt, "held-0-thread:N-7").is_none(), "his bubble above the box, nothing held over it");
        // the engine's answer goes to it; his next message from the box: his words alone
        rt.update(|s, _| {
            out_answer(s, true, "Posted on N-7 as post 5.");
            s.composers.insert(thread.clone(), TextEdit::new("ship it"));
            crate::message_box::send(s, &to_thread);
            out_answer(s, true, "Posted on N-7 as post 6.");
        });
        // another comment and a message, refused by the engine: `not sent · why`; Take back holds the comment for the
        // box again, his words back in it
        rt.update(|s, _| {
            open_mark(s, "cp-N-7.p2-0");
            write_note(s, "and on Windows?");
            crate::message_box::send(s, &Target::Comment { project: 0 });
            s.composers.insert(thread.clone(), TextEdit::new("or later"));
            crate::message_box::send(s, &to_thread);
        });
        let refused = rt.state.outgoing.list.iter().find(|x| !x.held.is_empty() && x.id != o.id).map(|x| x.id).expect("the third message, on its way");
        rt.update(|s, _| out_answer(s, false, "not sent: N-7 is closed"));
        let r = rt.state.outgoing.list.iter().find(|x| x.id == refused).cloned().expect("kept");
        assert_eq!(r.is, crate::outgoing::Is::NotSent("N-7 is closed".into()));
        rt.update(|s, _| crate::outgoing::take_back(s, refused));
        assert!(rt.state.outgoing.list.iter().all(|x| x.id != refused), "taken back");
        assert_eq!(held(&rt.state, 0, &thread.1).iter().map(|c| c.note.as_str()).collect::<Vec<_>>(), ["and on Windows?"], "its comment held again");
        assert_eq!(rt.state.composers.get(&thread).map(|e| e.text.as_str()), Some("or later"), "his words back in the box");
        let says = requests(&mut rt.state, child, &file, "say");
        assert_eq!(says.len(), 2, "one message for the two comments, one for the third: {says:?}");
        let text = says[0]["text"].as_str().unwrap();
        let parts: Vec<&str> = text.split("\n\n").collect();
        assert!(parts.len() == 5 && parts[0].starts_with(&format!("> {PASSAGE}\n> (from N-7.p2, paper-research's message at ")) && parts[1] == "and how long?", "the first passage, its note: {text}");
        assert!(parts[2] == "> the follower's first round\n> (from paper-research's message #4)" && parts[3] == "on every start?" && parts[4] == "and thanks", "the second, its note, his words: {text}");
        assert_eq!(says[0]["follows"], "N-7.p2", "a reply on the first passage's post");
        assert!(says[1]["text"].as_str().unwrap().ends_with("\n\nand on Windows?\n\nor later"), "{says:?}");
        let posts: Vec<Value> = std::fs::read_to_string(&file).unwrap_or_default().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["req"]["op"] == "post").map(|v| v["req"].clone()).collect();
        assert_eq!(posts.len(), 1, "{posts:?}");
        assert_eq!(posts[0]["text"], "ship it", "his next message: his words, no quote, no comment");
        let _ = std::fs::remove_file(&file);
    }

    /// The same box on a mail's mark (`cq-`): his note and the agent's answer; Add comment holds the new comment for the
    /// thread's reply box, whose Send mails it to the thread's people, after its latest mail, the passage's place kept.
    #[test]
    fn a_mails_mark_opens_the_same_box_and_its_comment_goes_with_the_reply() {
        let file = out("mail");
        let (_held, mut s, child) = app(&file);
        let key = sent_key("m13", 0);
        let at = passage(&s, &key).expect("the mark's passage");
        assert_eq!((at.target.as_str(), at.source.as_str(), at.quote.as_str()), ("m12", "m12", ASKED));
        let said = trail(&s, "m12", ASKED);
        let got: Vec<(&str, Option<(&str, &str)>)> = said.iter().map(|c| (c.note.as_str(), c.answer.as_ref().map(|a| (a.by.as_str(), a.words.as_str())))).collect();
        assert_eq!(got, vec![("the queue", Some(("paper-research", "the queue it is")))]);
        open_mark(&mut s, &key);
        assert_eq!(s.notes.picked.as_ref().map(|p| p.target.as_str()), Some("m12"), "it goes to the mail's thread");
        write_note(&mut s, "and the history after it");
        crate::message_box::send(&mut s, &Target::Comment { project: 0 });
        assert!(s.notes.picked.is_none(), "the box closes at the press");
        assert!(s.outgoing.list.is_empty() && held(&s, 0, "m12").len() == 1, "held for the reply, nothing sent");
        crate::message_box::send(&mut s, &Target::Reply { project: 0, id: "m12".into(), to: String::new() });
        assert!(held(&s, 0, "m12").is_empty(), "it went with the reply");
        let sent = requests(&mut s, child, &file, "msg");
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert_eq!(sent[0]["to"], json!(["paper-research"]));
        assert_eq!(sent[0]["re"], "m14", "after the thread's latest mail");
        assert_eq!(sent[0]["quotes"][0]["text"], ASKED);
        assert!(sent[0]["body"].as_str().unwrap().ends_with("\n\nand the history after it"));
        let _ = std::fs::remove_file(&file);
    }
}

#[cfg(test)]
pub(crate) mod anchor_tests {
    //! His comment box opens where he highlighted, on every surface he selects on, never at the foot of the page
    //! (I-1476; his N-1104 p706: "comment boxes … need to appear where i highlighted the text okay not at the bottom of
    //! the page"), and covers none of his text (paper-research N-1477 p9, p18): beside the lines, in their pane's right
    //! margin, where it holds the box; else in the flow, the lines after the passage moved down under it. Each surface,
    //! narrow and wide: a thread's post (With you, the Threads page), a post that asks him, a mail's message (`qmsg-`), a
    //! chat's entry, here; a file (files.rs `comment_tests`) and a line of a run (ran.rs `tests`) in their modules.
    use crate::{App, Sel};
    use rune::El;
    use rune::runtime::Runtime;
    use serde_json::json;

    const PASSAGE: &str = "the follower takes the start lock before its first round";
    /// The window's sizes: the test window's, where no pane's margin holds the box, and a wide one, where the Threads
    /// page's and a file's do.
    pub(crate) const NARROW: (f32, f32) = (1400.0, 900.0);
    pub(crate) const WIDE: (f32, f32) = (2200.0, 900.0);
    /// His words in the box, five lines: the box grows with them, its room too (consult-3, N-1477 p14).
    const TYPED: &str = "why before the read?\nand does the first round\nneed the lock at all,\nor only the leader's\nfirst write?";

    /// A long post's words: six paragraphs, the third holding PASSAGE.
    fn long(n: u32) -> String {
        (0..6)
            .map(|i| match i {
                2 => format!("Paragraph {i} of post {n}: the wait comes from {PASSAGE}, and every window waits behind it while it runs."),
                _ => format!("Paragraph {i} of post {n}: the runner waits on the start lock while the follower reads the record again, and the window waits behind it."),
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Project research, agent pr, and thread N-7 of ten posts, his and pr's in turn, each one line but post ON (pr's),
    /// which is long and holds PASSAGE; SHAPE makes post ON as a case needs it.
    fn research(on: u32, shape: impl Fn(&mut serde_json::Value)) -> crate::Proj {
        research_of(10, on, shape)
    }

    /// Project research as [`research`] makes it, thread N-7 of COUNT posts, a minute apart from 08:01.
    fn research_of(count: u32, on: u32, shape: impl Fn(&mut serde_json::Value)) -> crate::Proj {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.live.agents = vec![serde_json::from_value(json!({"id": "AGT-1", "name": "pr", "parent": "owner", "kind": "claude", "model": "m",
            "state": "idle", "since": "2026-11-21T08:00:00Z", "tools": [], "items": []})).unwrap()];
        let posts: Vec<serde_json::Value> = (1..=count)
            .map(|n| {
                let by = if n % 2 == 1 { "owner" } else { "pr" };
                let text = match n {
                    1 => "why do the jobs wait so long?".to_string(),
                    _ if n == on => long(n),
                    _ => format!("Post {n}: the runner waits on the record, and the window waits behind it."),
                };
                let mut x = json!({"n": n, "at": format!("2026-11-21T{:02}:{:02}:00Z", 8 + n / 60, n % 60), "by": by, "text": text, "names": [], "told": [if by == "owner" { "pr" } else { "owner" }]});
                if n == on {
                    shape(&mut x);
                }
                x
            })
            .collect();
        let v: coord_types::TicketView = serde_json::from_value(json!({"id": "N-7", "n": 7, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "pr"},
            "title": "the waits", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1,
            "opened_at": "2026-11-21T08:00:00Z", "given": [], "posts": posts})).unwrap();
        p.live.tickets.insert(7, v);
        p
    }

    /// S's window at SIZE, its first frames run.
    pub(crate) fn window(s: App, size: (f32, f32)) -> Runtime<App> {
        let mut rt = Runtime::new(s, crate::view, size, rune::Env::new(false, 1.0));
        rt.frame(0.0);
        settle(&mut rt);
        rt
    }

    /// pr's page, With you, thread N-7 beside the list (stuck to its foot, as it opens), post ON shaped by SHAPE, the
    /// window at SIZE.
    fn with_you(size: (f32, f32), on: u32, shape: impl Fn(&mut serde_json::Value)) -> (std::sync::MutexGuard<'static, ()>, Runtime<App>) {
        with_you_in(size, research(on, shape))
    }

    /// pr's page, With you, thread N-7 of project P beside the list (stuck to its foot, as it opens), the window at SIZE.
    fn with_you_in(size: (f32, f32), p: crate::Proj) -> (std::sync::MutexGuard<'static, ()>, Runtime<App>) {
        let (held, mut s) = crate::view_cost::test_app();
        s.projects = vec![p];
        s.cur = 0;
        s.sel = Sel::Agent("pr".into());
        s.withyou.inside.insert((0, "pr".to_string()), 7);
        (held, window(s, size))
    }

    fn app(on: u32, shape: impl Fn(&mut serde_json::Value)) -> (std::sync::MutexGuard<'static, ()>, Runtime<App>) {
        with_you(NARROW, on, shape)
    }

    /// The Threads page on thread N-7, the window at SIZE.
    fn threads(size: (f32, f32), on: u32, shape: impl Fn(&mut serde_json::Value)) -> (std::sync::MutexGuard<'static, ()>, Runtime<App>) {
        let (held, mut s) = crate::view_cost::test_app();
        s.projects = vec![research(on, shape)];
        s.cur = 0;
        s.sel = Sel::Tickets;
        s.tickets.shown = Some(7);
        (held, window(s, size))
    }

    fn plain(_: &mut serde_json::Value) {}

    /// The thread in PANE scrolled to its top (the first drawn; one in a pane of the split: [`to_top_in`]).
    fn to_top(rt: &mut Runtime<App>, pane: &str) {
        let r = drawn(rt, pane).unwrap_or_else(|| panic!("{pane} drawn: {}", rects(rt, 1)));
        wheel_up(rt, r);
        assert!(rt.find("post-N-7-1").is_some(), "scrolled to its top: {}", rects(rt, 1));
    }

    /// The thread keyed KEY in the split's pane J scrolled to its top.
    fn to_top_in(rt: &mut Runtime<App>, j: usize, key: &str) {
        let pane = crate::panes::key(j);
        let r = rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key) && e.path.split('/').any(|k| k == pane)).map(|e| e.rect);
        let r = r.unwrap_or_else(|| panic!("{key} drawn in {pane}: {}", rects(rt, 1)));
        wheel_up(rt, r);
        assert!(text_at(rt, &pane, "why do the jobs wait so long?").is_some(), "{key} in {pane} scrolled to its top: {}", rects(rt, 1));
    }

    /// The scroll at R wheeled to its top, at its left edge, where no box in the window's layer is over it (a wheel
    /// over one scrolls nothing).
    fn wheel_up(rt: &mut Runtime<App>, r: [f32; 4]) {
        for _ in 0..60 {
            rt.wheel(r[0] + 8.0, r[1] + r[3] / 2.0, 400.0);
            rt.frame(0.016);
        }
        settle(rt);
    }

    const WITH_YOU: &str = "with-you-thread-N-7";
    const DETAIL: &str = "ticket-detail";

    /// Frames enough for the box to find its place: the layer reads the last frame's layout, its room drawn the frame
    /// after it goes into the flow.
    pub(crate) fn settle(rt: &mut Runtime<App>) {
        rt.invalidate();
        for _ in 0..8 {
            rt.frame(0.016);
        }
    }

    pub(crate) fn drawn(rt: &Runtime<App>, key: &str) -> Option<[f32; 4]> {
        rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).map(|e| e.rect)
    }

    /// WORDS selected in the first text under element KEY that holds them and WITH, as a drag across them leaves them;
    /// None when none does.
    pub(crate) fn select(rt: &mut Runtime<App>, key: &str, words: &str, with: &str) -> Option<rune::TextSelection> {
        let e = rt.elements().into_iter().find(|e| e.path.split('/').any(|k| k == key) && e.text.as_deref().is_some_and(|t| t.contains(words) && t.contains(with)) && e.role != "mark")?;
        let a = e.text.as_deref().unwrap().find(words).unwrap();
        let (path, n) = (e.path.clone(), words.len());
        rt.update(move |_, tx| tx.select_text(path, a..a + n));
        rt.frame(0.016);
        let sel = rt.selection().expect("the words selected");
        // what he selected, the padding of a path in backticks left out
        assert_eq!(sel.text().trim(), words.replace('\u{202f}', ""));
        Some(sel)
    }

    /// WORDS selected in post ON, in its text that holds WITH too, and the selection menu's Comment pressed (threads.rs
    /// `commentable`): `comment_on` with rune's selection.
    fn comment_where(rt: &mut Runtime<App>, on: u32, words: &str, with: &str) {
        let post = format!("post-N-7-{on}");
        let Some(sel) = select(rt, &post, words, with) else {
            panic!("post {on}'s words are drawn: {:?}", rt.elements().iter().filter(|e| e.path.contains(&post)).map(|e| (&e.path, &e.role, &e.text)).collect::<Vec<_>>())
        };
        rt.update(move |s, tx| crate::ask::comment_on(s, 0, "thread:N-7", &sel, tx));
        settle(rt);
    }

    fn comment(rt: &mut Runtime<App>, on: u32, words: &str) {
        comment_where(rt, on, words, "");
    }

    /// His words typed in the comment box.
    fn type_in(rt: &mut Runtime<App>, words: &'static str) {
        rt.update(move |s, _| {
            let mut e = rune::TextEdit::new(words);
            e.focused = true;
            e.set_caret(words.len());
            s.composers.insert((0, crate::message_box::COMMENT_PLACE.into()), e);
        });
        settle(rt);
    }

    /// The lit passage's rects.
    pub(crate) fn lit(rt: &Runtime<App>) -> Vec<[f32; 4]> {
        rt.elements().into_iter().filter(|e| e.role == "mark" && e.key.as_deref() == Some(super::NEW)).map(|e| e.rect).collect()
    }

    /// What the rects say, for an assert's message.
    fn rects(rt: &Runtime<App>, on: u32) -> String {
        format!(
            "box {:?}, room {:?}, lit {:?}, post {:?}, next {:?}, foot box {:?}, under it {:?}, picked {:?}",
            drawn(rt, "cm-box"),
            drawn(rt, super::ROOM),
            lit(rt),
            drawn(rt, &format!("post-N-7-{on}")),
            drawn(rt, &format!("post-N-7-{}", on + 1)),
            drawn(rt, "composer-0-thread:N-7"),
            under_box(rt),
            rt.state.notes.picked.as_ref().map(|p| (&p.source, &p.at, &p.anchor, &p.block, p.pane))
        )
    }

    /// The texts the comment box covers: none may be (paper-research, N-1477 p9). A popup's box (`apart`) stands over
    /// the popup it is written in, whose dialog covers the page under it already: only the popup's own texts count.
    pub(crate) fn under_box(rt: &Runtime<App>) -> Vec<String> {
        let Some(b) = drawn(rt, "cm-box") else { return Vec::new() };
        // the top popup's panel (rune-kit's `Dialog`: KEY-dialog)
        let top = super::apart(&rt.state).then(|| format!("popup{}-dialog", rt.state.popups.len() - 1));
        let shown = |path: &str| top.as_ref().is_none_or(|t| path.split('/').any(|k| k == t));
        rt.elements()
            .into_iter()
            .filter(|e| !e.exiting && e.role != "mark" && !e.path.split('/').any(|k| k.starts_with("cm-")) && e.text.as_deref().is_some_and(|t| !t.trim().is_empty()))
            .filter(|e| shown(&e.path))
            .filter(|e| {
                let r = e.rect;
                r[0] < b[0] + b[2] - 1.0 && b[0] + 1.0 < r[0] + r[2] && r[1] < b[1] + b[3] - 1.0 && b[1] + 1.0 < r[1] + r[3]
            })
            .map(|e| format!("{:?} at {:?}", e.text.unwrap_or_default().chars().take(60).collect::<String>(), e.rect))
            .collect()
    }

    /// The top of the first text under element KEY that holds WORDS.
    pub(crate) fn text_at(rt: &Runtime<App>, key: &str, words: &str) -> Option<[f32; 4]> {
        rt.elements().into_iter().find(|e| !e.exiting && e.role != "mark" && e.path.split('/').any(|k| k == key) && e.text.as_deref().is_some_and(|t| t.contains(words))).map(|e| e.rect)
    }

    /// The box in the flow, the pane too narrow for it beside the lines: in its room inside element KEY, just under the
    /// lit passage's last line, AFTER (the words after the passage) moved down under it, and no text under it.
    pub(crate) fn in_room(rt: &Runtime<App>, key: &str, after: &str, why: &str) {
        let b = drawn(rt, "cm-box").unwrap_or_else(|| panic!("its box: {why}"));
        let room = drawn(rt, super::ROOM).unwrap_or_else(|| panic!("its room in the flow: {why}"));
        let m = lit(rt);
        assert!(!m.is_empty(), "the passage lit: {why}");
        let at = drawn(rt, key).unwrap();
        assert!(m.iter().all(|r| r[1] >= at[1] && r[1] + r[3] <= at[1] + at[3]), "lit once, in {key}: {why}");
        let last = m.iter().map(|r| r[1] + r[3]).fold(f32::MIN, f32::max);
        assert!(b[1] >= last && b[1] <= last + 24.0, "just under the selection's last line: {why}");
        assert!(b[1] >= room[1] && b[1] + b[3] <= room[1] + room[3] + 0.5 && room[1] >= at[1] && room[1] + room[3] <= at[1] + at[3] + 0.5, "in its room, in {key}: {why}");
        let next = text_at(rt, key, after).unwrap_or_else(|| panic!("{after:?} drawn: {why}"));
        assert!(next[1] >= b[1] + b[3], "the words after it moved down under it: {next:?} {why}");
        assert!(under_box(rt).is_empty(), "no text under the box: {why}");
    }

    /// The box beside the lines: right of every word of element KEY, at the lit passage's first line, inside PANE,
    /// no room left in the flow, and no text under it.
    pub(crate) fn beside(rt: &Runtime<App>, key: &str, pane: &str, why: &str) {
        let b = drawn(rt, "cm-box").unwrap_or_else(|| panic!("its box: {why}"));
        let m = lit(rt);
        assert!(!m.is_empty(), "the passage lit: {why}");
        let top = m.iter().map(|r| r[1]).fold(f32::MAX, f32::min);
        assert!((b[1] - top).abs() <= 6.0, "at the passage's first line: {why}");
        let right = rt.elements().into_iter().filter(|e| e.path.split('/').any(|k| k == key) && e.text.is_some()).map(|e| e.rect[0] + e.rect[2]).fold(f32::MIN, f32::max);
        assert!(b[0] >= right + 8.0, "right of its words ({right}): {why}");
        let p = drawn(rt, pane).unwrap();
        assert!(b[0] + b[2] <= p[0] + p[2], "inside its pane {p:?}: {why}");
        assert!(drawn(rt, super::ROOM).is_none(), "no room in the flow: {why}");
        assert!(under_box(rt).is_empty(), "no text under the box: {why}");
    }

    /// The box in post ON's room under the passage, the words after it moved down, the next post below it.
    fn in_post(rt: &Runtime<App>, on: u32, after: &str) {
        let why = rects(rt, on);
        in_room(rt, &format!("post-N-7-{on}"), after, &why);
        let b = drawn(rt, "cm-box").unwrap();
        if let Some(next) = drawn(rt, &format!("post-N-7-{}", on + 1)) {
            assert!(b[1] + b[3] <= next[1], "above the next post: {why}");
        }
    }

    /// A long post in the middle of the thread, scrolled to its top: the box opens under the lines he highlighted, the
    /// lines after them moved down under it, never at the thread's box at its foot.
    #[test]
    fn the_box_opens_under_the_lines_he_highlighted_in_a_thread_scrolled_to_its_top() {
        let (_held, mut rt) = app(4, plain);
        to_top(&mut rt, WITH_YOU);
        comment(&mut rt, 4, PASSAGE);
        in_post(&rt, 4, "and every window waits");
    }

    /// His case (his frames.jsonl, 21 Nov 11:4x): With you's thread stuck to its foot, a passage of a long agent post
    /// above the last ones commented on. Narrow, the box in the flow; wide, beside the lines in the margin, as on the
    /// Threads page, since With you's thread is in the Threads page's reading column (item 15, paper-research N-1446
    /// p2603).
    #[test]
    fn the_box_opens_under_the_lines_he_highlighted_in_a_thread_at_its_foot() {
        let (held, mut rt) = with_you(NARROW, 8, plain);
        comment(&mut rt, 8, PASSAGE);
        in_post(&rt, 8, "and every window waits");
        drop((rt, held));
        let (_held, mut rt) = with_you(WIDE, 8, plain);
        comment(&mut rt, 8, PASSAGE);
        beside(&rt, "post-N-7-8", WITH_YOU, &rects(&rt, 8));
    }

    /// The Threads page: narrow, the box in the flow; wide, beside the lines in the page's right margin, as a
    /// document's margin comments are, nothing moved.
    #[test]
    fn on_the_threads_page_the_box_is_in_the_flow_narrow_and_beside_the_lines_wide() {
        let (held, mut rt) = threads(NARROW, 4, plain);
        to_top(&mut rt, DETAIL);
        comment(&mut rt, 4, PASSAGE);
        in_post(&rt, 4, "and every window waits");
        drop((rt, held));
        let (_held, mut rt) = threads(WIDE, 4, plain);
        to_top(&mut rt, DETAIL);
        let across = |rt: &Runtime<App>| text_at(rt, "post-N-7-4", "and every window waits").map(|r| (r[0], r[2]));
        let before = across(&rt);
        comment(&mut rt, 4, PASSAGE);
        let why = rects(&rt, 4);
        beside(&rt, "post-N-7-4", DETAIL, &why);
        assert_eq!(across(&rt), before, "the lines where they were, the column not moved: {why}");
        // his five lines: the box grows down the margin, over nothing
        type_in(&mut rt, TYPED);
        beside(&rt, "post-N-7-4", DETAIL, &rects(&rt, 4));
        // with no element he selected in (a mark opened again, a scene): beside the post's own column all the same, not
        // past its row in the thread's list, which is the pane's width
        rt.update(|s, _| {
            crate::ask::unwrite(s);
            let p = crate::ask::Picked { target: "thread:N-7".into(), source: "N-7.p4".into(), label: "pr's message #4".into(), quote: PASSAGE.into(), at: Vec::new(), pi: 0, anchor: None, block: None, pane: None, texts: Vec::new() };
            crate::ask::write(s, p, "");
        });
        settle(&mut rt);
        beside(&rt, "post-N-7-4", DETAIL, &rects(&rt, 4));
    }

    /// A post that asks him (threads.rs `drawn`, its lines between the questions): its passage is lit and the box
    /// opens under it, where before nothing was lit there and the box went to the composer.
    #[test]
    fn the_box_opens_under_the_lines_he_highlighted_in_a_post_that_asks_him() {
        let ask = "Which goes first, the lock or the read?";
        let (_held, mut rt) = app(4, |x| {
            let t = format!("{}\n\n{ask}", x["text"].as_str().unwrap());
            x["asks"] = json!([{"at": [t.len() - ask.len(), t.len()]}]);
            x["asks_open"] = json!(true);
            x["text"] = json!(t);
        });
        to_top(&mut rt, WITH_YOU);
        comment(&mut rt, 4, PASSAGE);
        in_post(&rt, 4, "and every window waits");
    }

    /// The box open while he scrolls its pane (proto-window N-1477 p29), narrow and wide: no frame draws any of it
    /// outside the pane; scrolled until the passage and its room are above the pane's top, no box and no text under
    /// one; scrolled back, the box with his words.
    #[test]
    fn the_box_stays_inside_its_pane_as_he_scrolls_it_away_and_back() {
        // post 4 long after its passage, so the thread scrolls it past the pane's top at 2200 too
        let longer = |x: &mut serde_json::Value| {
            let more: String = (0..14).map(|i| format!("\n\nMore {i}: the record is read whole on every start, and nothing else may write.")).collect();
            x["text"] = json!(format!("{}{more}", x["text"].as_str().unwrap()));
        };
        for size in [NARROW, WIDE] {
            let (_held, mut rt) = threads(size, 4, longer);
            to_top(&mut rt, DETAIL);
            comment(&mut rt, 4, PASSAGE);
            type_in(&mut rt, "only for the read?");
            let pane = drawn(&rt, DETAIL).unwrap();
            let within = |rt: &Runtime<App>, what: &str| {
                if let Some(b) = drawn(rt, "cm-box") {
                    assert!(b[1] >= pane[1] - 0.5 && b[1] + b[3] <= pane[1] + pane[3] + 0.5, "{size:?} {what}: the box {b:?} inside its pane {pane:?}: {}", rects(rt, 4));
                }
            };
            within(&rt, "opened");
            assert!(text_at(&rt, "cm-box", "only for the read?").is_some(), "{size:?}: the box with his words: {}", rects(&rt, 4));
            // the pointer on the pane's left, not on the box (a wheel over the box scrolls nothing)
            let (x, y) = (pane[0] + 12.0, pane[1] + pane[3] / 2.0);
            let gone = |rt: &Runtime<App>| lit(rt).iter().all(|r| r[1] + r[3] < pane[1]) && drawn(rt, super::ROOM).is_none_or(|r| r[1] < pane[1]);
            for _ in 0..80 {
                if gone(&rt) {
                    break;
                }
                rt.wheel(x, y, -60.0);
                rt.frame(0.016);
                within(&rt, "scrolling away");
            }
            settle(&mut rt);
            assert!(gone(&rt), "{size:?}: the passage and its room above the pane's top: {}", rects(&rt, 4));
            within(&rt, "scrolled away");
            assert!(drawn(&rt, "cm-box").is_none() && under_box(&rt).is_empty(), "{size:?}: no box over the thread's title: {}", rects(&rt, 4));
            for _ in 0..80 {
                rt.wheel(x, y, 60.0);
                rt.frame(0.016);
                within(&rt, "scrolling back");
            }
            settle(&mut rt);
            within(&rt, "back");
            assert!(text_at(&rt, "cm-box", "only for the read?").is_some(), "{size:?}: back with his words: {}", rects(&rt, 4));
            assert!(under_box(&rt).is_empty(), "{size:?}: no text under the box: {}", rects(&rt, 4));
        }
    }

    /// The Threads page scrolled to the middle and a comment on the last lines in view (proto-window p29), narrow and
    /// wide: the box shows whole inside the pane, its passage too, no text under it.
    #[test]
    fn a_comment_on_the_last_lines_in_view_has_its_box_whole_inside_the_pane() {
        for size in [NARROW, WIDE] {
            let (_held, mut rt) = threads(size, 10, plain);
            to_top(&mut rt, DETAIL);
            let pane = drawn(&rt, DETAIL).unwrap();
            let (x, y) = (pane[0] + pane[2] / 2.0, pane[1] + pane[3] / 2.0);
            let foot = pane[1] + pane[3];
            // the passage's paragraph the last lines in view
            for _ in 0..200 {
                if text_at(&rt, "post-N-7-10", PASSAGE).is_some_and(|r| r[1] + r[3] <= foot) {
                    break;
                }
                rt.wheel(x, y, -20.0);
                rt.frame(0.016);
            }
            settle(&mut rt);
            let para = text_at(&rt, "post-N-7-10", PASSAGE).unwrap();
            assert!(para[1] + para[3] <= foot && para[1] + para[3] > foot - 80.0, "{size:?}: the paragraph {para:?} at the pane's foot {foot}");
            comment(&mut rt, 10, PASSAGE);
            let why = format!("{size:?}: {}", rects(&rt, 10));
            let b = drawn(&rt, "cm-box").unwrap_or_else(|| panic!("its box: {why}"));
            assert!(b[1] >= pane[1] - 0.5 && b[1] + b[3] <= foot + 0.5, "whole inside the pane {pane:?}: {why}");
            assert!(lit(&rt).iter().all(|r| r[1] >= pane[1] && r[1] + r[3] <= foot), "its passage in view: {why}");
            assert!(under_box(&rt).is_empty(), "no text under the box: {why}");
        }
    }

    /// With you and a post that asks him there, wide (2200; p18: each surface narrow and wide): the box beside the
    /// lines when their pane holds it, else in their room, the words after it under it; no text under it either way.
    #[test]
    fn with_you_and_a_post_that_asks_him_cover_nothing_wide() {
        let ask = "Which goes first, the lock or the read?";
        for asks in [false, true] {
            let (_held, mut rt) = with_you(WIDE, 4, move |x| {
                if asks {
                    let t = format!("{}\n\n{ask}", x["text"].as_str().unwrap());
                    x["asks"] = json!([{"at": [t.len() - ask.len(), t.len()]}]);
                    x["asks_open"] = json!(true);
                    x["text"] = json!(t);
                }
            });
            to_top(&mut rt, WITH_YOU);
            comment(&mut rt, 4, PASSAGE);
            let why = format!("asks {asks}: {}", rects(&rt, 4));
            if drawn(&rt, super::ROOM).is_some() {
                in_post(&rt, 4, "and every window waits");
            } else {
                beside(&rt, "post-N-7-4", WITH_YOU, &why);
            }
            type_in(&mut rt, TYPED);
            assert!(under_box(&rt).is_empty(), "no text under the box, his five lines in: {why}");
        }
    }

    /// proto-window's case (N-1477 p8): a post that asks him, its passage in two of its blocks, selected in the first:
    /// one NEW lit, in the first, and the box under it there, above the question.
    #[test]
    fn a_passage_in_two_blocks_of_an_asking_post_is_lit_once_where_he_selected_it() {
        let (q1, q2) = ("Which goes first?", "And which second?");
        let (_held, mut rt) = app(4, |x| {
            let t = format!("Block one: {PASSAGE}, said once.\n\n{q1}\n\nBlock two: {PASSAGE}, said twice.\n\n{q2}");
            let a = t.find(q1).unwrap();
            let b = t.find(q2).unwrap();
            x["asks"] = json!([{"at": [a, a + q1.len()]}, {"at": [b, b + q2.len()]}]);
            x["asks_open"] = json!(true);
            x["text"] = json!(t);
        });
        to_top(&mut rt, WITH_YOU);
        comment_where(&mut rt, 4, PASSAGE, "Block one");
        let why = rects(&rt, 4);
        let question = drawn(&rt, "ask-N-7-4-0").unwrap_or_else(|| panic!("its first question: {why}"));
        assert!(lit(&rt).iter().all(|r| r[1] + r[3] <= question[1]), "one NEW, in the first block: {why}");
        in_post(&rt, 4, "said once.");
        let b = drawn(&rt, "cm-box").unwrap();
        assert!(b[1] + b[3] <= question[1], "under the first block, above the question: {why}");
    }

    /// consult-3's case (N-1477 p10): his words twice in a post, a link before the second, so the selection's ranges no
    /// longer hold them (the link's label drawn other than when he selected): one NEW lit, at the second, where he
    /// selected, and the box there.
    #[test]
    fn words_twice_in_a_post_are_lit_where_he_selected_them_when_the_ranges_no_longer_hold() {
        let (_held, mut rt) = app(4, |x| {
            x["text"] = json!(format!("Paragraph 0: {PASSAGE}, said first.\n\nParagraph 1: after `crates/coordd/src/lock.rs`, {PASSAGE}, said again.\n\nParagraph 2: the end of it."));
        });
        to_top(&mut rt, WITH_YOU);
        comment_where(&mut rt, 4, PASSAGE, "said again");
        rt.update(|s, _| {
            let pk = s.notes.picked.as_mut().unwrap();
            pk.at = pk.at.iter().map(|r| r.start - 12..r.end - 12).collect();
        });
        settle(&mut rt);
        let why = rects(&rt, 4);
        let first = text_at(&rt, "post-N-7-4", "said first").unwrap_or_else(|| panic!("{why}"));
        assert!(lit(&rt).iter().all(|r| r[1] >= first[1] + first[3]), "not at the first: {why}");
        in_post(&rt, 4, "said again.");
    }

    /// A post shaped as his p705 was: a bold lead, a path in backticks, a list; a passage in the list: the box under
    /// its item, the next item moved down.
    #[test]
    fn the_box_opens_under_the_lines_he_highlighted_in_a_list_after_bold_and_code() {
        let (_held, mut rt) = app(4, |x| {
            x["text"] = json!(format!(
                "**The wait is the lock.** The runner holds it while `crates/coordd/src/lock.rs` reads the record:\n\n- {PASSAGE}, and\n- every window waits behind it.\n\n{}",
                long(4)
            ));
        });
        to_top(&mut rt, WITH_YOU);
        comment(&mut rt, 4, PASSAGE);
        in_post(&rt, 4, "every window waits behind it.");
    }

    /// A passage across a path in backticks.
    #[test]
    fn the_box_opens_under_the_lines_he_highlighted_across_a_path_in_backticks() {
        let (_held, mut rt) = app(4, |x| {
            x["text"] = json!(format!("**The wait is the lock.** The runner holds it while `crates/coordd/src/lock.rs` reads the record.\n\n{}", long(4)));
        });
        to_top(&mut rt, WITH_YOU);
        // a path in backticks is drawn padded with narrow spaces (U+202F)
        comment(&mut rt, 4, "holds it while \u{202f}\u{202f}crates/coordd/src/lock.rs\u{202f}\u{202f} reads");
        in_post(&rt, 4, "the record.");
    }

    /// He types five lines in the box and a new post comes: the box stays open under the passage, his words in it, its
    /// room grown with it, nothing under it; Esc closes it, and the room with it.
    #[test]
    fn the_box_stays_under_the_lines_with_his_words_when_a_post_comes() {
        let (_held, mut rt) = app(8, plain);
        comment(&mut rt, 8, PASSAGE);
        let before = drawn(&rt, "cm-box").unwrap()[3];
        type_in(&mut rt, TYPED);
        assert!(drawn(&rt, "cm-box").unwrap()[3] > before + 20.0, "the box grew with his lines: {}", rects(&rt, 8));
        in_post(&rt, 8, "and every window waits");
        rt.update(|s, _| {
            let v = s.projects[0].live.tickets.get_mut(&7).unwrap();
            let post = serde_json::from_value(json!({"n": 11, "at": "2026-11-21T08:11:00Z", "by": "pr", "text": "Post 11: one more thing.", "names": [], "told": ["owner"]})).unwrap();
            v.posts.push(post);
        });
        settle(&mut rt);
        assert!(rt.find("post-N-7-11").is_some(), "the new post drawn: {}", rects(&rt, 8));
        in_post(&rt, 8, "and every window waits");
        assert_eq!(crate::ask::writing(&rt.state).map(|w| w.text.clone()).as_deref(), Some(TYPED), "his words kept");
        // Esc closes it, his words kept as its draft (N-1396 p329)
        rt.key(rune::KeyEvent { key: rune::Key::Escape, mods: Default::default() });
        settle(&mut rt);
        assert!(drawn(&rt, "cm-box").is_none() && drawn(&rt, super::ROOM).is_none(), "Esc closes it: {}", rects(&rt, 8));
    }

    /// paper-research's post N-1104 p1165 (24 Nov 20:17Z), the one he commented on twice before he said the box opened
    /// at the top: a line, a list of three, a paragraph.
    const P1165: &str = "Build 83 is in your window since 21:17 (it swapped by itself). Three things in it:\n- Home has a Devices section under Threads: this laptop and the build machine, each with what runs on it, and a red mark when one is out of reach.\n- The Accounts page says \"engine\" where it said \"coordd\".\n- A window swap no longer loses a sign-in that was under way.\n\nNext window build (being made now): sign-in no longer opens your browser and has a Copy link press instead; agy is out of the Claude list; \"Move all to my accounts\" as one press.";

    /// His case (N-1104 p1166 to p1168, 24 Nov 20:28Z; paper-research N-1446 p3062: "the comment box ... sometimes
    /// opens at the top of the thread instead of at the post he pressed on"), as his frames.jsonl has it: the Threads
    /// page, the Agents page, then pr's page, With you on a long thread at its foot, the window's poll running; a
    /// comment on the first item of the last post's list, sent (his post quoting it comes under it), then one on that
    /// post's last paragraph. Narrow and wide, each box opens at the lines he highlighted, in the pane, nothing under
    /// it, the thread where he was: its last post in sight, never its top.
    #[test]
    fn a_comment_on_the_last_post_of_a_long_thread_opens_at_the_post_never_at_the_top() {
        const LAST: u32 = 60;
        // and half of a 1920-wide screen, under lib.rs's NARROW_W: the narrow page, the thread alone with Back over it
        for size in [(960.0, 1000.0), NARROW, WIDE] {
            let (_held, mut rt) = with_you_in(size, research_of(LAST, LAST, |x| x["text"] = json!(P1165)));
            rt.poll = Some(crate::poll);
            for sel in [Sel::Tickets, Sel::Agents, Sel::Agent("pr".into())] {
                rt.update(move |s, _| {
                    s.tickets.shown = Some(7);
                    s.sel = sel;
                });
                for _ in 0..30 {
                    settle(&mut rt);
                }
            }
            let at_post = |rt: &Runtime<App>, step: &str| {
                let why = format!("{size:?} {step}: post 1 drawn {}, offset {:?}, {}", rt.find("post-N-7-1").is_some(), rt.state.read_places.offsets.lock().unwrap().get("thread research N-7"), rects(rt, LAST));
                let b = drawn(rt, "cm-box").unwrap_or_else(|| panic!("its box: {why}"));
                let m = lit(rt);
                assert!(!m.is_empty(), "the passage lit: {why}");
                let (top, last) = (m.iter().map(|r| r[1]).fold(f32::MAX, f32::min), m.iter().map(|r| r[1] + r[3]).fold(f32::MIN, f32::max));
                // beside them at their first line, else in the flow under the end of their item or paragraph, in the post
                let post = drawn(rt, &format!("post-N-7-{LAST}")).unwrap_or_else(|| panic!("the post drawn: {why}"));
                let inline = drawn(rt, super::ROOM).is_some();
                assert!(if inline { b[1] >= last && b[1] + b[3] <= post[1] + post[3] + 0.5 } else { (b[1] - top).abs() <= 6.0 }, "at the lines he highlighted: {why}");
                let pane = drawn(rt, WITH_YOU).unwrap();
                assert!(b[1] >= pane[1] - 0.5 && b[1] + b[3] <= pane[1] + pane[3] + 0.5, "inside the pane {pane:?}: {why}");
                assert!(rt.find("post-N-7-1").is_none() && rt.find(&format!("post-N-7-{LAST}")).is_some(), "the thread where he was: {why}");
                assert!(under_box(rt).is_empty(), "no text under the box: {why}");
            };
            // each frame as it is shown from his press, at his window's pace (a tenth of a second a frame, his
            // frames.jsonl): the box, once drawn, at or under the lines he highlighted, never above them, and the
            // thread where he was
            // his hand's way (his frames: three presses, then the menu): a triple click on the words selects their
            // item or paragraph, a right click on them opens the selection menu, and its Comment is pressed
            let opened = |rt: &mut Runtime<App>, words: &str, with: &str, step: &str| {
                let r = rt.elements().into_iter().find(|e| !e.exiting && e.role != "mark" && e.path.split('/').any(|k| k == format!("post-N-7-{LAST}")) && e.text.as_deref().is_some_and(|t| t.contains(words) && t.contains(with))).map(|e| e.rect);
                let r = r.unwrap_or_else(|| panic!("{size:?} {step}: the words drawn: {}", rects(rt, LAST)));
                let (x, y) = (r[0] + 24.0, r[1] + 10.0);
                let pane = drawn(rt, WITH_YOU).unwrap();
                for _ in 0..3 {
                    rt.pointer_down(x, y);
                    rt.pointer_up(x, y);
                }
                rt.frame(0.1);
                rt.secondary_down(x, y);
                rt.secondary_up(x, y);
                rt.frame(0.1);
                assert!(rt.popup_menu().is_some_and(|m| m.find("app:comment").is_some()), "{size:?} {step}: the selection menu with Comment: {:?}", rt.popup_menu());
                rt.resolve_popup(Some("app:comment")).unwrap();
                for f in 0..12 {
                    rt.frame(0.1);
                    if let Some(b) = drawn(rt, "cm-box") {
                        let top = lit(rt).iter().map(|r| r[1]).fold(f32::MAX, f32::min);
                        assert!(top < f32::MAX && b[1] >= top - 6.0, "{size:?} {step}, frame {f}: the box at or under the lines he highlighted: {}", rects(rt, LAST));
                    }
                    assert!(rt.find("post-N-7-1").is_none() && rt.find(&format!("post-N-7-{LAST}")).is_some(), "{size:?} {step}, frame {f}: the thread where he was: {}", rects(rt, LAST));
                }
                settle(rt);
                // his lines stay where he pressed them, the box opening under them: the thread, at its foot, does not
                // pull them up by the box's height. They go up only as far as shows the box whole when it would pass
                // the pane's foot (`room_el`'s reveal): then its room ends at the foot.
                let stays = |rt: &Runtime<App>, what: &str| {
                    let now = text_at(rt, &format!("post-N-7-{LAST}"), words).unwrap_or_else(|| panic!("{size:?} {step}: the words drawn {what}"));
                    let shown = drawn(rt, super::ROOM).is_some_and(|m| now[1] < r[1] && m[1] + m[3] >= pane[1] + pane[3] - 64.0);
                    assert!((now[1] - r[1]).abs() <= 4.0 || shown, "{size:?} {step}: his lines stay where he pressed them {what}: at {} then {} (pane {pane:?}): {}", r[1], now[1], rects(rt, LAST));
                };
                stays(rt, "as the box opens");
                // and as he types, the box growing with his five lines
                type_in(rt, TYPED);
                stays(rt, "as he types");
            };
            assert!(rt.find(&format!("post-N-7-{LAST}")).is_some(), "{size:?}: at its foot: {}", rects(&rt, LAST));
            opened(&mut rt, "Home has a Devices section under Threads", "this laptop", "his first comment");
            at_post(&rt, "his first comment");
            // sent: the box closes, his post quoting it comes under the post
            rt.update(|s, _| {
                crate::ask::unwrite(s);
                let text = format!("> Home has a Devices section under Threads\n> (from N-7.p{LAST}, pr's message at 09:00Z)\n\ni prefer one card per machine");
                let post = serde_json::from_value(json!({"n": LAST + 1, "at": "2026-11-21T09:01:00Z", "by": "owner", "text": text, "names": [], "told": ["pr"]})).unwrap();
                s.projects[0].live.tickets.get_mut(&7).unwrap().posts.push(post);
            });
            for _ in 0..30 {
                settle(&mut rt);
            }
            assert!(rt.find(&format!("post-N-7-{}", LAST + 1)).is_some(), "{size:?}: his post drawn: {}", rects(&rt, LAST));
            opened(&mut rt, "Next window build (being made now)", "Copy link", "his second comment");
            at_post(&rt, "his second comment");
            for _ in 0..30 {
                settle(&mut rt);
            }
            at_post(&rt, "his second comment, the poll run after");
        }
    }

    /// The first element drawn under the element keyed IN whose words are T, of role ROLE when given.
    fn within(rt: &Runtime<App>, inside: &str, role: Option<&str>, t: Option<&str>) -> Option<[f32; 4]> {
        rt.elements().into_iter()
            .find(|e| !e.exiting && e.path.split('/').any(|k| k == inside) && role.is_none_or(|r| e.role == r) && t.is_none_or(|t| e.text.as_deref() == Some(t)))
            .map(|e| e.rect)
    }

    fn press_at(rt: &mut Runtime<App>, r: [f32; 4]) {
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        rt.frame(0.016);
        settle(rt);
    }

    /// His comment by his gestures on the words of post N of thread N-7 from its start: a drag across them, the right
    /// click's Comment, NOTE typed in the box, Add comment pressed. The passage it quoted.
    fn comment_by_hand(rt: &mut Runtime<App>, n: u32, note: &str) -> String {
        let r = text_in(rt, n, &|t| t.starts_with(&format!("Post {n}:"))).unwrap_or_else(|| panic!("post {n} drawn: {}", rects(rt, n)));
        drag_across(rt, (r[0] + 1.0, r[1] + 8.0), (r[0] + 180.0, r[1] + 8.0));
        rt.secondary_down(r[0] + 60.0, r[1] + 8.0);
        rt.secondary_up(r[0] + 60.0, r[1] + 8.0);
        rt.frame(0.1);
        assert!(rt.popup_menu().is_some_and(|m| m.find("app:comment").is_some()), "Comment in the menu: {:?}", rt.popup_menu());
        rt.resolve_popup(Some("app:comment")).unwrap();
        settle(rt);
        let quote = rt.state.notes.picked.as_ref().map(|p| p.quote.clone()).expect("a comment begun");
        let field = within(rt, "cm-box", Some("textbox"), None).expect("the comment box");
        press_at(rt, field);
        crate::view_cost::type_in(rt, note);
        assert_eq!(super::writing(&rt.state).map(|e| e.text.as_str()), Some(note), "typed in the comment box");
        let add = within(rt, "cm-box", None, Some("Add comment")).expect("its Add comment");
        press_at(rt, add);
        quote
    }

    /// His walk item 11 (paper-research N-1446 p3521; his N-1104 p617: "add comemt workign liek the send butotn which
    /// if its the same input"): two comments on passages of two of pr's posts in With you, each by his gestures, send
    /// nothing and stand over the thread's box, each its passage and his note; he writes in the box, and its Send sends
    /// one message: each passage quoted with his note under it, then his words; the box empty, nothing held after it.
    /// Before, from d90b000c, each comment went at its own Add comment press.
    #[test]
    fn two_comments_and_one_send_make_one_message() {
        let (_held, mut rt) = with_you_in(WIDE, research_of(5, 0, plain));
        settle(&mut rt);
        let thread = (0usize, "thread:N-7".to_string());
        let first = comment_by_hand(&mut rt, 2, "by whom?");
        assert!(rt.state.notes.picked.is_none() && drawn(&rt, "cm-box").is_none(), "the comment box closes at Add comment");
        assert!(rt.state.outgoing.list.is_empty() && rt.state.projects[0].pending.is_empty(), "Add comment sends nothing: {:?}", rt.state.outgoing.list.iter().map(|o| &o.text).collect::<Vec<_>>());
        let line = |rt: &Runtime<App>, t: &str| within(rt, "held-0-thread:N-7", None, Some(t)).is_some();
        assert!(line(&rt, "1 comment goes with your next Send") && line(&rt, "by whom?"), "it stands over the thread's box: {}", rects(&rt, 2));
        let second = comment_by_hand(&mut rt, 4, "and how long each time?");
        assert!(rt.state.outgoing.list.is_empty() && rt.state.projects[0].pending.is_empty(), "the second sends nothing either");
        assert!(line(&rt, "2 comments go with your next Send") && line(&rt, "and how long each time?"), "both over the box");
        assert_eq!(super::held(&rt.state, 0, &thread.1).iter().map(|c| (c.quote.clone(), c.note.as_str())).collect::<Vec<_>>(), [(first.clone(), "by whom?"), (second.clone(), "and how long each time?")]);
        // his words in the thread's box, and its Send
        let field = within(&rt, "composer-0-thread:N-7", Some("textbox"), None).expect("the thread's box");
        press_at(&mut rt, field);
        crate::view_cost::type_in(&mut rt, "and which first?");
        let send = within(&rt, "composer-actions-0-thread:N-7", None, Some("Send")).expect("the thread's Send");
        press_at(&mut rt, send);
        // no pipe in this test: his one message stands `not sent` above the box, as it went
        assert_eq!(rt.state.outgoing.list.len(), 1, "one message: {:?}", rt.state.outgoing.list.iter().map(|o| &o.text).collect::<Vec<_>>());
        let o = rt.state.outgoing.list[0].clone();
        assert_eq!(o.key, thread, "from the thread's box");
        let text = crate::words::text_of(&o.req).unwrap_or_default().to_string();
        let quoted = |q: &str| q.lines().map(|l| format!("> {l}\n")).collect::<String>();
        let (a, b) = (text.find(&quoted(&first)), text.find(&quoted(&second)));
        assert!(a.is_some_and(|a| b.is_some_and(|b| a < b)), "each passage quoted, in his order: {text}");
        let (n1, n2, w) = (text.find("\n\nby whom?"), text.find("\n\nand how long each time?"), text.find("\n\nand which first?"));
        assert!(n1.zip(b).is_some_and(|(n, b)| n < b) && n2.zip(w).is_some_and(|(n, w)| n < w) && text.ends_with("and which first?"), "each note under its passage, his words last: {text}");
        assert!(matches!(&o.req, coord_types::Request::Say { follows: Some(f), .. } if f == "N-7.p2"), "a reply on the first passage's post: {:?}", o.req);
        assert_eq!(o.text, text, "drawn as it went");
        assert!(super::held(&rt.state, 0, &thread.1).is_empty() && within(&rt, "held-0-thread:N-7", None, None).is_none(), "nothing held after it");
        assert!(rt.state.composers.get(&thread).is_none_or(|e| e.text.is_empty()), "the box empty");
        // Take back: both comments held for the box again, his words back in it
        rt.update(|s, _| crate::outgoing::take_back(s, o.id));
        assert!(rt.state.outgoing.list.is_empty());
        assert_eq!(super::held(&rt.state, 0, &thread.1).len(), 2, "held again");
        assert_eq!(rt.state.composers.get(&thread).map(|e| e.text.as_str()), Some("and which first?"), "his words back");
    }

    /// paper-research's post N-1104 p1195 (25 Nov 05:54Z), as he commented on its list items: a paragraph, then a list.
    const P1195: &str = "You are right, and you do not need to read any of it.\n\nWhat gets built now, so you see it in your window instead of reading about it:\n- **The list of what you asked and where each stands**, as a page in your window.\n- **Job ordering** as a short policy the runner follows: one replay at a time, shortest first.\n- **The system prompt parts editor** and **one Accounts page** in place of two.";

    /// Threads N-100 on of project P with pr, COUNT of them, titles one to three lines long: his list beside the
    /// thread, as full as his with paper-research.
    fn with_threads(mut p: crate::Proj, count: u32) -> crate::Proj {
        for i in 0..count {
            let n = 100 + i;
            let at = format!("2026-11-20T{:02}:{:02}:00Z", 8 + i / 60, i % 60);
            let title = format!("Thread {i}: {}", "the runner waits on the record while the window waits behind it ".repeat(1 + (i % 3) as usize));
            let v: coord_types::TicketView = serde_json::from_value(json!({"id": format!("N-{n}"), "n": n, "kind": "thread", "opener": "owner", "ctx": {},
                "form": {"to": "pr"}, "title": title.trim(), "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1,
                "opened_at": at, "given": [], "posts": [{"n": 1, "at": at, "by": "owner", "text": format!("post of thread {i}"), "names": [], "told": ["pr"]}]})).unwrap();
            p.live.tickets.insert(n as u64, v);
        }
        p
    }

    /// The rect of the first text of post N of thread N-7 that F takes, as drawn (its marks left out).
    fn text_in(rt: &Runtime<App>, n: u32, f: &dyn Fn(&str) -> bool) -> Option<[f32; 4]> {
        text_on(rt, &format!("post-N-7-{n}"), f)
    }

    /// The rect of the first text F takes in the post keyed POST, as drawn (its marks left out).
    fn text_on(rt: &Runtime<App>, post: &str, f: &dyn Fn(&str) -> bool) -> Option<[f32; 4]> {
        rt.elements().into_iter().find(|e| !e.exiting && e.role != "mark" && e.path.split('/').any(|k| k == post) && e.text.as_deref().is_some_and(f)).map(|e| e.rect)
    }

    /// A press at FROM dragged to TO in eight moves, a frame each, and let go.
    fn drag_across(rt: &mut Runtime<App>, from: (f32, f32), to: (f32, f32)) {
        rt.pointer_down(from.0, from.1);
        for i in 1..=8 {
            let f = i as f32 / 8.0;
            rt.pointer_move(from.0 + (to.0 - from.0) * f, from.1 + (to.1 - from.1) * f);
            rt.frame(0.016);
        }
        rt.pointer_up(to.0, to.1);
        rt.frame(0.1);
    }

    /// A triple click at AT as his hand makes one: the pointer moves a point on the third press.
    fn triple_click(rt: &mut Runtime<App>, at: (f32, f32)) {
        for k in 0..3 {
            rt.pointer_down(at.0, at.1);
            if k == 2 {
                rt.pointer_move(at.0 + 1.0, at.1 + 1.0);
                rt.frame(0.016);
            }
            let up = if k == 2 { (at.0 + 1.0, at.1 + 1.0) } else { at };
            rt.pointer_up(up.0, up.1);
        }
        rt.frame(0.1);
    }

    /// A right click at AT and the menu's Comment: the comment's quote and its lit rects; then closed again.
    fn comment_at(rt: &mut Runtime<App>, at: (f32, f32)) -> Result<(String, Vec<[f32; 4]>), String> {
        rt.secondary_down(at.0, at.1);
        rt.secondary_up(at.0, at.1);
        rt.frame(0.1);
        if !rt.popup_menu().is_some_and(|m| m.find("app:comment").is_some()) {
            return Err(format!("no Comment in the menu: {:?}", rt.popup_menu()));
        }
        rt.resolve_popup(Some("app:comment")).unwrap();
        settle(rt);
        let quote = rt.state.notes.picked.as_ref().map(|p| p.quote.clone()).ok_or("no comment begun")?;
        let m = lit(rt);
        rt.update(|s, _| crate::ask::unwrite(s));
        settle(rt);
        rt.frame(1.0);
        Ok((quote, m))
    }

    /// A right click at AT and the menu's Copy: what it put on CLIPBOARD. An error when the menu offers Comment too.
    fn copy_at(rt: &mut Runtime<App>, clipboard: &rune::clipboard::MemoryClipboard, at: (f32, f32)) -> Result<String, String> {
        rt.secondary_down(at.0, at.1);
        rt.secondary_up(at.0, at.1);
        rt.frame(0.1);
        let menu = rt.popup_menu().cloned().ok_or("no menu")?;
        if menu.find("copy").is_none() || menu.find("app:comment").is_some() {
            rt.resolve_popup(None).unwrap();
            return Err(format!("not Copy alone: {menu:?}"));
        }
        rt.resolve_popup(Some("copy")).unwrap();
        settle(rt);
        Ok(clipboard.text().map(|t| t.to_string()).unwrap_or_default())
    }

    /// A right click at AT on a selection that spans posts into the post keyed POST, whose words start at height Y, and
    /// the menu's Copy (paper-research N-1714 p10): Ok when the menu offers Copy alone and Copy takes the selection whole,
    /// each of WORDS in it and POST's head over its words (its name and time as drawn, its buttons' words left out).
    fn copies_across(rt: &mut Runtime<App>, clipboard: &rune::clipboard::MemoryClipboard, at: (f32, f32), post: &str, y: f32, words: &[&str]) -> Result<(), String> {
        let selected = rt.selection().map(|s| s.text().to_string()).unwrap_or_default();
        let all = rt.elements();
        let in_post = |e: &rune::ElementInfo| e.path.split('/').any(|k| k == post);
        let buttons: Vec<String> = all.iter().filter(|e| e.role == "button" && in_post(e)).map(|e| format!("{}/", e.path)).collect();
        let head: Vec<String> = all.iter()
            .filter(|e| !e.exiting && e.role != "mark" && e.role != "button" && !buttons.iter().any(|b| e.path.starts_with(b.as_str())))
            .filter(|e| in_post(e) && e.rect[1] + e.rect[3] <= y)
            .filter_map(|e| e.text.clone().filter(|t| !t.trim().is_empty())).collect();
        let got = copy_at(rt, clipboard, at)?;
        match got == selected && words.iter().all(|w| got.contains(w)) && !head.is_empty() && head.iter().all(|h| got.contains(h.as_str())) {
            true => Ok(()),
            false => Err(format!("copied {got:?}, selected {selected:?}, its head {head:?}")),
        }
    }

    /// The selection's highlight as drawn, in the kit's colour (lib.rs `view`): its pieces of lines.
    fn highlighted(rt: &Runtime<App>) -> Vec<[f32; 4]> {
        let kit = [rune_kit::Kit::neutral(false).selection().premul(1.0), rune_kit::Kit::neutral(true).selection().premul(1.0)];
        rt.instances.iter().filter(|i| kit.iter().any(|c| i.fill.iter().zip(c).all(|(a, b)| (a - b).abs() < 1e-3))).map(|i| i.rect).collect()
    }

    /// Whether LIT lights the lines SEL highlighted and only them: each highlighted piece (past a line end's few points)
    /// lit across all but a few points of it, each lit rect on a highlighted piece.
    fn same_lines(sel: &[[f32; 4]], lit: &[[f32; 4]]) -> Result<(), String> {
        if sel.is_empty() || lit.is_empty() {
            return Err(format!("highlighted {sel:?}, lit {lit:?}"));
        }
        let on = |r: &[f32; 4], y: f32| r[1] <= y && y <= r[1] + r[3];
        for s in sel.iter().filter(|s| s[2] > 12.0) {
            let y = s[1] + s[3] / 2.0;
            let covered: f32 = lit.iter().filter(|l| on(l, y)).map(|l| (l[0] + l[2]).min(s[0] + s[2]) - l[0].max(s[0])).filter(|w| *w > 0.0).sum();
            if covered < s[2] - 6.0 {
                return Err(format!("highlighted {s:?} lit {covered:.0} of its {:.0} (lit {lit:?})", s[2]));
            }
        }
        match lit.iter().find(|l| !sel.iter().any(|s| on(s, l[1] + l[3] / 2.0) && s[0] - 3.0 <= l[0] + l[2] / 2.0 && l[0] + l[2] / 2.0 <= s[0] + s[2] + 3.0)) {
            Some(l) => Err(format!("lit {l:?} on no highlighted line (highlighted {sel:?})")),
            None => Ok(()),
        }
    }

    /// His case on build 95 (N-1104 p1196 and p1198, 25 Nov 05:56Z; N-1714): a post's list item, beside his list of
    /// threads; his hand's ways on it, a triple click on its words whose pointer moves a point on the third press, a drag
    /// across some of them, a drag back from its end past its marker; each time a right click on them and the menu's
    /// Comment. The comment quotes what he selected, inside that item: not the paragraph above, no list marker (what
    /// went out was "What gets built now, …:\n\n•\t- The list of what you asked …"), and lights it there. At ten
    /// offsets of the post to the list's rows (the window 3 points taller each time, the thread at its foot), so that
    /// at some a row of the list stands beside the gap between the paragraph and the item, as in his window: a page as
    /// wide as the window took that row's line, the paragraph's and the item's for one (rune `group_point`).
    #[test]
    fn a_comment_on_a_list_item_quotes_and_lights_what_he_selected_in_it() {
        const ITEM: &str = "The list of what you asked and where each stands, as a page in your window.";
        let mut wrong = Vec::new();
        let mut beside = 0;
        for tall in (0..10).map(|k| WIDE.1 + 3.0 * k as f32) {
            let (_held, mut rt) = with_you_in((WIDE.0, tall), with_threads(research_of(30, 30, |x| x["text"] = json!(P1195)), 40));
            settle(&mut rt);
            let item = |rt: &Runtime<App>| text_in(rt, 30, &|t| t.contains("The list of what you asked"));
            let (Some(r), Some(para)) = (item(&rt), text_in(&rt, 30, &|t| t.contains("What gets built now"))) else {
                wrong.push(format!("{tall}: the item or the paragraph not drawn"));
                continue;
            };
            let gap = (para[1] + para[3], r[1]);
            beside += rt.elements().iter().any(|e| {
                !e.exiting && e.path.contains("outline-pr") && e.text.as_deref().is_some_and(|t| !t.trim().is_empty()) && e.visible[1] < gap.0 && e.visible[1] + e.visible[3] > gap.1
            }) as u32;
            // its quote, lit inside the item alone
            let in_item = |got: Result<(String, Vec<[f32; 4]>), String>, r: [f32; 4]| -> Result<String, String> {
                let (quote, m) = got?;
                if m.is_empty() || !m.iter().all(|l| l[1] >= r[1] - 1.0 && l[1] + l[3] <= r[1] + r[3] + 1.0) {
                    return Err(format!("{quote:?} lit outside the item {r:?}: {m:?}"));
                }
                Ok(quote)
            };
            // a triple click on its words: the item, whole
            let at = (r[0] + 24.0, r[1] + 10.0);
            triple_click(&mut rt, at);
            match in_item(comment_at(&mut rt, at), r) {
                Ok(q) if q == ITEM => {}
                got => wrong.push(format!("{tall}: the triple click: {got:?}")),
            }
            // a drag across some of its words on its first line: those words
            let Some(r) = item(&rt) else { continue };
            drag_across(&mut rt, (r[0] + 40.0, r[1] + 10.0), (r[0] + 200.0, r[1] + 10.0));
            match in_item(comment_at(&mut rt, (r[0] + 100.0, r[1] + 10.0)), r) {
                Ok(q) if !q.is_empty() && ITEM.contains(&q) => {}
                got => wrong.push(format!("{tall}: the drag across its words: {got:?}")),
            }
            // a drag back from its end past its marker, into the margin: the item, whole
            let Some(r) = item(&rt) else { continue };
            drag_across(&mut rt, (r[0] + r[2] - 4.0, r[1] + 10.0), (r[0] - 40.0, r[1] + 10.0));
            match in_item(comment_at(&mut rt, (r[0] + 100.0, r[1] + 10.0)), r) {
                Ok(q) if q == ITEM => {}
                got => wrong.push(format!("{tall}: the drag back past its marker: {got:?}")),
            }
        }
        assert!(beside > 0, "a row of the list beside the gap above the item at some offset, as in his window");
        assert!(wrong.is_empty(), "beside at {beside} of 10 offsets; wrong:\n{}", wrong.join("\n"));
    }

    /// His gestures on a real post, before the page goes back into his window (paper-research N-1446 p3407): a range
    /// inside a paragraph, one list item (a triple click), a range in a code block, each time a right click on it and
    /// Comment: the comment quotes what is selected, a word cut at either end taken whole (his walk item 12,
    /// [`a_comment_on_words_cut_at_a_code_blocks_edge_quotes_them_whole`]), and lights the lines the selection highlighted. A
    /// range across two posts, a right click on it: its menu offers Copy and no Comment, and Copy takes it whole, the
    /// name and time between the posts with it, as a web page would (paper-research N-1714 p10). p1195's words and a code
    /// block as the thread's last post, his list of threads beside it, at four offsets of the post to the list's rows, as
    /// [`a_comment_on_a_list_item_quotes_and_lights_what_he_selected_in_it`].
    #[test]
    fn his_gestures_on_a_post_quote_and_light_exactly_the_selection() {
        const PARA: &str = "You are right, and you do not need to read any of it.";
        const ITEM: &str = "The list of what you asked and where each stands, as a page in your window.";
        const CODE: &str = "let job = queue.next();\nrun(job);";
        let words = format!("{P1195}\n\n```\n{CODE}\n```");
        let post = words.clone();
        let mut wrong = Vec::new();
        for tall in (0..4).map(|k| WIDE.1 + 9.0 * k as f32) {
            let words = words.clone();
            let (_held, mut rt) = with_you_in((WIDE.0, tall), with_threads(research_of(30, 30, move |x| x["text"] = json!(words)), 40));
            let clipboard = rune::clipboard::memory();
            rt.use_clipboard(&clipboard);
            settle(&mut rt);
            let (Some(p), Some(r), Some(c), Some(before)) = (
                text_in(&rt, 30, &|t| t.contains(PARA)),
                text_in(&rt, 30, &|t| t.contains("The list of what you asked")),
                text_in(&rt, 30, &|t| t.contains("let job")),
                text_in(&rt, 29, &|t| t.contains("Post 29:")),
            ) else {
                wrong.push(format!("{tall}: the post's paragraph, item, code or the post before not drawn"));
                continue;
            };
            let mut check = |rt: &mut Runtime<App>, what: &str, at: (f32, f32), want: &dyn Fn(&str) -> bool| {
                let selected = rt.selection().map(|s| s.text().trim().to_string()).unwrap_or_default();
                let sel = highlighted(rt);
                match comment_at(rt, at) {
                    Ok((q, m)) if q == whole_in(&post, &selected) && want(&q) => {
                        if let Err(e) = same_lines(&sel, &m) {
                            wrong.push(format!("{tall}: {what}: {q:?}: {e}"));
                        }
                    }
                    Ok((q, _)) => wrong.push(format!("{tall}: {what}: quoted {q:?}, selected {selected:?}")),
                    Err(e) => wrong.push(format!("{tall}: {what}: {e}")),
                }
            };
            // a range inside a paragraph
            drag_across(&mut rt, (p[0] + 40.0, p[1] + 10.0), (p[0] + 200.0, p[1] + 10.0));
            check(&mut rt, "a range inside a paragraph", (p[0] + 100.0, p[1] + 10.0), &|q| !q.is_empty() && PARA.contains(q));
            // one list item, a triple click
            triple_click(&mut rt, (r[0] + 24.0, r[1] + 10.0));
            check(&mut rt, "a list item's triple click", (r[0] + 24.0, r[1] + 10.0), &|q| q == ITEM);
            // a range in a code block, over its two lines
            drag_across(&mut rt, (c[0] + 30.0, c[1] + 10.0), (c[0] + 50.0, c[1] + c[3] - 10.0));
            check(&mut rt, "a range in a code block", (c[0] + 40.0, c[1] + 10.0), &|q| q.contains('\n') && CODE.contains(q));
            // a range across two posts, from the one before into this one's first paragraph: Copy alone, and it copies
            // whole, with this post's name and time over its words
            drag_across(&mut rt, (before[0] + 60.0, before[1] + 10.0), (p[0] + 120.0, p[1] + 10.0));
            if let Err(e) = copies_across(&mut rt, &clipboard, (p[0] + 60.0, p[1] + 10.0), "post-N-7-30", p[1], &["the window waits behind it.", "You are right"]) {
                wrong.push(format!("{tall}: a range across two posts: {e}"));
            }
        }
        assert!(wrong.is_empty(), "wrong:\n{}", wrong.join("\n"));
    }

    /// p1189's section 3c as its words (N-1104, paper-research's message at 05:49Z): the job facts, a code block whose
    /// lines run on past a pane 1000 wide.
    const P1189_3C: &str = r#"## 3c. Scheduling: the job facts built tonight (agent-base/policy/open-jobs.eflint, H-1651, comments shortened)

```
Fact job-open Identified by String * String           // job, "start" (waiting) or "runs" (running)
Fact job-by   Identified by String * String           // job, the agent that sent it
Fact job-kind Identified by String * String * String  // job, verb, packages
Fact job-last Identified by String * String * Int     // verb, packages, seconds the last one took

Event act
  Related to is, phase, id, agent, verb, packages, secs
  Creates job-open(id, "start") When is == "job" and phase == "start"
  Creates job-by(id, agent) When is == "job" and phase == "start"
  Creates job-kind(id, verb, packages) When is == "job" and phase == "start"
  Terminates job-open(id, "start") When is == "job" and phase == "runs"
  Creates job-open(id, "runs") When is == "job" and phase == "runs"
  Terminates (Foreach string: job-open(id, string)) When is == "job" and phase == "end"
  Terminates (Foreach string: job-by(id, string)) When is == "job" and phase == "end"
  Terminates (Foreach string1, string2: job-kind(id, string1, string2)) When is == "job" and phase == "end"
  Terminates (Foreach int: job-last(verb, packages, int)) When is == "job" and phase == "end"
  Creates job-last(verb, packages, secs) When is == "job" and phase == "end"
```
Against what you said yes to (p1156): the runner's own lines move a job, not the pick, and times are seconds.
"Your job first" holds for no job yet: nothing in a job says you asked for it. The pick itself isn't built."#;

    /// SELECTED as a comment quotes it from WORDS, the words of the post it is in: a word cut at either end taken whole
    /// (`whole_words`), SELECTED itself where it is not in WORDS.
    fn whole_in(words: &str, selected: &str) -> String {
        let Some(i) = words.find(selected).filter(|_| !selected.is_empty()) else { return selected.to_string() };
        let (before, after) = (&words[..i], &words[i + selected.len()..]);
        let word = |c: char| c.is_alphanumeric() || c == '_';
        let head = match before.ends_with(char::is_alphanumeric) && selected.starts_with(char::is_alphanumeric) {
            true => &before[before.trim_end_matches(word).len()..],
            false => "",
        };
        let tail = match after.starts_with(char::is_alphanumeric) && selected.ends_with(char::is_alphanumeric) {
            true => &after[..after.len() - after.trim_start_matches(word).len()],
            false => "",
        };
        format!("{head}{selected}{tail}")
    }

    /// His walk item 12, the cut word (paper-research N-1446 p3524): N-1104 p1190 commented on p1189's job facts in a
    /// window 1000 wide, where the code block's lines run on past the pane, and his drag stopped at the pane's edge
    /// inside the last line's `"end"`: the agent got `phase == "en`. Here a drag from inside the first line's
    /// `Identified` to the last line, stopping each half a character from three before the pane's edge to three past it:
    /// Comment quotes the words
    /// selected with a cut word taken whole at either end (`Identified`, `end`), and lights them in the block; one that
    /// stops between words quotes what it selected.
    #[test]
    fn a_comment_on_words_cut_at_a_code_blocks_edge_quotes_them_whole() {
        let (_held, mut rt) = with_you_in((1000.0, 900.0), research_of(30, 30, |x| x["text"] = json!(P1189_3C)));
        settle(&mut rt);
        let code = P1189_3C.split("```\n").nth(1).unwrap();
        let lines: Vec<&str> = code.lines().collect();
        let block = rt.elements().into_iter().find(|e| !e.exiting && e.role != "mark" && e.path.split('/').any(|k| k == "post-N-7-30") && e.text.as_deref().is_some_and(|t| t.starts_with("Fact job-open")));
        let Some(b) = block else { panic!("p1189's code block drawn in post 30") };
        let (r, edge) = (b.rect, b.visible[0] + b.visible[2]);
        assert!(r[0] + r[2] > edge + 40.0, "its lines run on past the pane: {r:?}, seen to {edge}");
        let lh = r[3] / lines.len() as f32;
        let cw = r[2] / lines.iter().map(|l| l.chars().count()).max().unwrap() as f32;
        let from = (r[0] + cw * (lines[0].find("Identified").unwrap() as f32 + 4.5), r[1] + lh * 0.5);
        let (mut wrong, mut cut, mut ended) = (Vec::new(), 0, false);
        for k in -6..=6 {
            let to = (edge + cw * 0.5 * k as f32, r[1] + lh * (lines.len() as f32 - 0.5));
            drag_across(&mut rt, from, to);
            let selected = rt.selection().map(|s| s.text().trim().to_string()).unwrap_or_default();
            let want = whole_in(P1189_3C, &selected);
            cut += !want.ends_with(selected.as_str()) as u32;
            match comment_at(&mut rt, (r[0] + 40.0, r[1] + lh * 6.5)) {
                Ok((q, m)) if q == want && q.starts_with("Identified by String") && !m.is_empty() && m.iter().all(|l| l[1] >= r[1] - 1.0 && l[1] + l[3] <= r[1] + r[3] + 1.0) => {
                    ended |= q.ends_with("phase == \"end") && (selected.ends_with("phase == \"e") || selected.ends_with("phase == \"en"));
                }
                Ok((q, m)) => wrong.push(format!("{k}: selected …{:?}, quoted …{:?}, lit {m:?}", &selected[selected.len().saturating_sub(30)..], &q[q.len().saturating_sub(30)..])),
                Err(e) => wrong.push(format!("{k}: {e}")),
            }
        }
        assert!(wrong.is_empty(), "wrong:\n{}", wrong.join("\n"));
        assert!(cut > 0 && ended, "some drag stopped inside the last line's `end` and its quote took it whole ({cut} cut)");
    }

    /// With agents (his N-1104 p1237: "i stil lcannot drag text acorss eh the with agents"; paper-research N-1446
    /// p3440): pr's page showing its posts with other agents. A drag from pr's post into dev's answer under it selects
    /// across both, and a right click on it offers Copy alone, which copies it whole, dev's name and time with it.
    #[test]
    fn a_drag_across_two_agents_posts_in_with_agents_selects_and_copies_them() {
        let mut p = research_of(4, 4, plain);
        p.live.agents.push(serde_json::from_value(json!({"id": "AGT-2", "name": "dev", "parent": "owner", "kind": "claude", "model": "m",
            "state": "idle", "since": "2026-11-21T08:00:00Z", "tools": [], "items": []})).unwrap());
        let said = [
            ("dev", "pr", "The replay holds the tower for twenty minutes, and the window's line waits behind it."),
            ("pr", "dev", "Then the line goes first: it takes three minutes, and nothing in it reads the record."),
            ("dev", "pr", "Agreed: I queue the line ahead of the replay, and the replay right after it."),
        ];
        let posts: Vec<serde_json::Value> = said.iter().enumerate()
            .map(|(i, (by, to, text))| json!({"n": i + 1, "at": format!("2026-11-21T09:0{}:00Z", i + 1), "by": by, "text": text, "names": [to], "told": [to]}))
            .collect();
        let v: coord_types::TicketView = serde_json::from_value(json!({"id": "N-8", "n": 8, "kind": "thread", "opener": "dev", "ctx": {}, "form": {"to": "pr"},
            "title": "the line and the replay", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1,
            "opened_at": "2026-11-21T09:00:00Z", "given": [], "posts": posts})).unwrap();
        p.live.tickets.insert(8, v);
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![p];
        s.cur = 0;
        s.sel = Sel::Agent("pr".into());
        s.streams.which.insert((0, "pr".to_string()), crate::streams::Which::Agents);
        let mut rt = window(s, WIDE);
        let clipboard = rune::clipboard::memory();
        rt.use_clipboard(&clipboard);
        settle(&mut rt);
        let (Some(a), Some(b)) = (text_on(&rt, "post-N-8-2", &|t| t.contains("Then the line goes first")), text_on(&rt, "post-N-8-3", &|t| t.contains("Agreed: I queue"))) else {
            let drawn: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting && e.path.contains("post-N-8")).filter_map(|e| e.text).collect();
            panic!("pr's post and dev's answer drawn in With agents: {drawn:?}");
        };
        drag_across(&mut rt, (a[0] + 60.0, a[1] + 10.0), (b[0] + 120.0, b[1] + 10.0));
        copies_across(&mut rt, &clipboard, (b[0] + 60.0, b[1] + 10.0), "post-N-8-3", b[1], &["nothing in it reads the record.", "Agreed: I"]).unwrap();
    }

    /// A comment whose words are no longer in the post (taken back after an edit, kept from an older window): the box
    /// stands in a room at the end of where he selected it, else of the post its source names; never at the thread's
    /// box, nothing under it.
    #[test]
    fn a_passage_not_lit_has_its_box_at_the_end_of_its_post_never_at_the_foot() {
        let (_held, mut rt) = app(4, plain);
        to_top(&mut rt, WITH_YOU);
        let picked = |anchor: Option<String>| crate::ask::Picked {
            target: "thread:N-7".into(),
            source: "N-7.p4".into(),
            label: "pr's message #4".into(),
            quote: "words the post no longer holds".into(),
            at: Vec::new(),
            pi: 0,
            anchor,
            block: None,
            pane: None,
            texts: Vec::new(),
        };
        let at_end = |rt: &Runtime<App>, on: u32, last: &str| {
            let why = rects(rt, on);
            let b = drawn(rt, "cm-box").unwrap_or_else(|| panic!("its box: {why}"));
            assert!(lit(rt).is_empty(), "nothing lit: {why}");
            let words = text_at(rt, &format!("post-N-7-{on}"), last).unwrap();
            let post = drawn(rt, &format!("post-N-7-{on}")).unwrap();
            assert!(b[1] >= words[1] + words[3] && b[1] + b[3] <= post[1] + post[3] + 0.5, "at the end of post {on}: {why}");
            assert!(under_box(rt).is_empty(), "no text under the box: {why}");
        };
        rt.update(move |s, _| crate::ask::write(s, picked(None), ""));
        settle(&mut rt);
        at_end(&rt, 4, "Paragraph 5");
        // where he selected it: the element's keyed path
        let path = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("post-N-7-3")).map(|e| e.path).unwrap();
        let anchor = super::keyed(&path, Some("post-N-7-3"));
        assert!(anchor.as_deref().is_some_and(|a| a.ends_with("/post-N-7-3") && !a.contains('#')), "{anchor:?}");
        rt.update(move |s, _| {
            crate::ask::unwrite(s);
            crate::ask::write(s, picked(anchor), "");
        });
        settle(&mut rt);
        at_end(&rt, 3, "Post 3");
    }

    /// A page of one surface, SURFACE drawn in it (after the box's place is decided, as lib.rs `view` does), the
    /// comment box's layer over it: a mail's message, a chat's entry.
    fn page(s: &App, cx: &mut rune::Cx<App>, surface: impl FnOnce() -> El) -> El {
        let k = rune_kit::Kit::neutral(false);
        crate::ask::decide(s, cx);
        let surface = surface();
        let mut layer = Vec::new();
        crate::ask::layer(s, &k, &mut layer);
        let page = rune::col().key("page").w_full().h_full().min_h(0.0).padding(24.0).gap(12.0).selection_group(true).child(surface).child(rune::text("the next thing on the page").font(k.ty.body));
        rune_kit::root(page.instant_changes(), layer)
    }

    /// A mail of paper-research's in a thread (`thread_row`), long, PASSAGE in it; comment on it as its card makes it.
    fn mail_view(s: &App, cx: &mut rune::Cx<App>) -> El {
        let k = rune_kit::Kit::neutral(false);
        let p = &s.projects[0];
        let m = p.live.mail.get("m12").unwrap();
        page(s, cx, || rune::col().w_full().max_w(700.0).child(super::thread_row(s, p, m, &k)))
    }

    fn mail_app() -> (std::sync::MutexGuard<'static, ()>, App) {
        mail_with(long(12))
    }

    /// paper-research's mail m12 to him, BODY its words.
    fn mail_with(body: String) -> (std::sync::MutexGuard<'static, ()>, App) {
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = research(4, plain);
        let m: coord_types::MailView = serde_json::from_value(json!({"id": "m12", "at": "2026-11-21T08:30:00Z", "from": "pr", "to": ["owner"], "subject": "",
            "body": body, "esc": false, "delivered": [], "re": null, "quotes": []})).unwrap();
        p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Mail(m) });
        s.projects = vec![p];
        s.cur = 0;
        (held, s)
    }

    /// A mail's message holding BODY, narrow, PASSAGE in it commented on, as he would: the window, and the x of each
    /// text of LOOKED (words of BODY) before it.
    fn commented_in(body: String, looked: &[&str]) -> (std::sync::MutexGuard<'static, ()>, Runtime<App>, Vec<f32>) {
        let (held, s) = mail_with(body);
        let mut rt = Runtime::new(s, mail_view, NARROW, rune::Env::new(false, 1.0));
        settle(&mut rt);
        let xs = looked.iter().map(|w| text_at(&rt, "qmsg-m12", w).unwrap_or_else(|| panic!("{w:?} drawn"))[0]).collect();
        let sel = select(&mut rt, "qmsg-m12", PASSAGE, "").expect("the passage drawn");
        rt.update(move |s, tx| crate::ask::comment_on(s, 0, "m12", &sel, tx));
        settle(&mut rt);
        (held, rt, xs)
    }

    /// A passage in a table's row (consult-3 N-1477 p25): the room after the whole table, which is drawn as before,
    /// each column's cells at their x.
    #[test]
    fn a_passage_in_a_table_has_its_box_after_the_table_its_columns_where_they_were() {
        let body = format!("Before the table.\n\n| step | what |\n|---|---|\n| one | {PASSAGE} |\n| two | the leader writes |\n| three | the round runs |\n\nAfter the table, and every window waits.");
        let cells = ["step", "what", "two", "the leader writes", "three", "the round runs"];
        let (_held, rt, xs) = commented_in(body, &cells);
        let why = format!("box {:?}, room {:?}, under {:?}", drawn(&rt, "cm-box"), drawn(&rt, super::ROOM), under_box(&rt));
        let b = drawn(&rt, "cm-box").unwrap_or_else(|| panic!("its box: {why}"));
        let last = text_at(&rt, "qmsg-m12", "the round runs").unwrap();
        assert!(b[1] >= last[1] + last[3], "after the table's last row: {why}");
        for (w, x) in cells.iter().zip(&xs) {
            assert_eq!(text_at(&rt, "qmsg-m12", w).map(|r| r[0]), Some(*x), "{w:?} in its column where it was: {why}");
        }
        assert!(text_at(&rt, "qmsg-m12", "After the table").is_some_and(|r| r[1] >= b[1] + b[3]), "the words after it under the box: {why}");
        assert!(under_box(&rt).is_empty(), "no text under the box: {why}");
    }

    /// A passage in the middle of a long diff (docket-edge2, consult-3 N-1477 p25): the box under its line, not at the
    /// diff's foot; the code after it closed and opened again, every line still code at its x.
    #[test]
    fn a_passage_in_a_long_diff_has_its_box_under_its_line_the_rest_still_code() {
        let lines: Vec<String> = (1..=30)
            .map(|i| match i {
                12 => format!("+ {PASSAGE}"),
                20 => " # not a heading".to_string(),
                _ => format!("+ line {i} of the change"),
            })
            .collect();
        let body = format!("The change:\n\n```diff\n{}\n```\n\nAfter the diff.", lines.join("\n"));
        let (_held, rt, xs) = commented_in(body, &["line 13 of the change", "line 30 of the change"]);
        let why = format!("box {:?}, room {:?}, under {:?}", drawn(&rt, "cm-box"), drawn(&rt, super::ROOM), under_box(&rt));
        let b = drawn(&rt, "cm-box").unwrap_or_else(|| panic!("its box: {why}"));
        let at = text_at(&rt, "qmsg-m12", PASSAGE).unwrap();
        assert!(b[1] >= at[1] + at[3] && b[1] <= at[1] + at[3] + 40.0, "just under its line: {at:?} {why}");
        for (w, x) in ["line 13 of the change", "line 30 of the change"].iter().zip(&xs) {
            let r = text_at(&rt, "qmsg-m12", w).unwrap_or_else(|| panic!("{w:?} drawn: {why}"));
            assert!(r[1] >= b[1] + b[3] && r[0] == *x, "{w:?} under the box, at its x: {r:?} {why}");
        }
        assert!(text_at(&rt, "qmsg-m12", "# not a heading").is_some(), "the lines after it still code, none a heading: {why}");
        assert!(text_at(&rt, "qmsg-m12", "line 11 of the change").is_some_and(|r| r[1] < b[1]), "the lines before it above it: {why}");
        assert!(under_box(&rt).is_empty(), "no text under the box: {why}");
    }

    /// A mail's message (`qmsg-`), narrow and wide: the box in its flow, the lines after the passage moved down.
    #[test]
    fn in_a_mails_message_the_box_is_in_the_flow_under_the_lines() {
        for size in [NARROW, WIDE] {
            let (_held, s) = mail_app();
            let mut rt = Runtime::new(s, mail_view, size, rune::Env::new(false, 1.0));
            settle(&mut rt);
            let sel = select(&mut rt, "qmsg-m12", PASSAGE, "").expect("the mail's words drawn");
            rt.update(move |s, tx| crate::ask::comment_on(s, 0, "m12", &sel, tx));
            settle(&mut rt);
            assert_eq!(rt.state.notes.picked.as_ref().map(|p| p.source.as_str()), Some("m12"));
            let why = format!("{size:?}: box {:?}, room {:?}, lit {:?}, under {:?}", drawn(&rt, "cm-box"), drawn(&rt, super::ROOM), lit(&rt), under_box(&rt));
            in_room(&rt, "qmsg-m12", "and every window waits", &why);
            type_in(&mut rt, TYPED);
            in_room(&rt, "qmsg-m12", "and every window waits", &why);
        }
    }

    /// The next frames as they are shown, none settled first (consult-3 N-1477 p25): no text under the box in any.
    pub(crate) fn frames_shown(rt: &mut Runtime<App>, what: &str) {
        for f in 0..3 {
            rt.frame(0.016);
            let under = under_box(rt);
            assert!(under.is_empty(), "frame {f} {what}: under the box {under:?}, box {:?}, room {:?}, lit {:?}", drawn(rt, "cm-box"), drawn(rt, super::ROOM), lit(rt));
            let rooms = rt.elements().into_iter().filter(|e| !e.exiting && e.key.as_deref() == Some(super::ROOM)).count();
            assert!(rooms <= 1, "frame {f} {what}: {rooms} rooms");
        }
    }

    /// His five lines typed into the box, one more each time, the frames after each as they are shown
    /// ([`frames_shown`]); settled, AFTER (the words after the passage, in element KEY) under the box grown with them.
    pub(crate) fn type_lines(rt: &mut Runtime<App>, key: &str, after: &str) {
        for (n, _) in TYPED.match_indices('\n').chain([(TYPED.len(), "")]) {
            let words = &TYPED[..n];
            rt.update(move |s, _| {
                let mut e = rune::TextEdit::new(words);
                e.focused = true;
                e.set_caret(words.len());
                s.composers.insert((0, crate::message_box::COMMENT_PLACE.into()), e);
            });
            frames_shown(rt, &format!("with {} lines typed", words.lines().count()));
        }
        settle(rt);
        let (b, next) = (drawn(rt, "cm-box"), text_at(rt, key, after));
        assert!(b.is_some_and(|b| next.is_some_and(|r| r[1] >= b[1] + b[3])), "its five lines in, {after:?} under it: box {b:?}, room {:?}, {after:?} at {next:?}", drawn(rt, super::ROOM));
    }

    /// The frames as they are shown, none settled first (consult-3 N-1477 p25): the frame the box opens and the next
    /// ones, and those after each line he types makes it taller, on a mail's message and on the Threads page, narrow:
    /// no text under the box in any of them.
    #[test]
    fn no_frame_has_text_under_the_box_as_it_opens_and_grows() {
        let (held, s) = mail_app();
        let mut rt = Runtime::new(s, mail_view, NARROW, rune::Env::new(false, 1.0));
        settle(&mut rt);
        let sel = select(&mut rt, "qmsg-m12", PASSAGE, "").expect("the mail's words drawn");
        rt.update(move |s, tx| crate::ask::comment_on(s, 0, "m12", &sel, tx));
        frames_shown(&mut rt, "the mail's box opened");
        settle(&mut rt);
        in_room(&rt, "qmsg-m12", "and every window waits", "the mail's box settled");
        type_lines(&mut rt, "qmsg-m12", "and every window waits");
        drop((rt, held));
        let (_held, mut rt) = threads(NARROW, 4, plain);
        to_top(&mut rt, DETAIL);
        let sel = select(&mut rt, "post-N-7-4", PASSAGE, "").expect("the post's words drawn");
        rt.update(move |s, tx| crate::ask::comment_on(s, 0, "thread:N-7", &sel, tx));
        frames_shown(&mut rt, "the post's box opened");
        settle(&mut rt);
        in_post(&rt, 4, "and every window waits");
        type_lines(&mut rt, "post-N-7-4", "and every window waits");
    }

    /// pr's chat entry (an assistant message), long, PASSAGE in it.
    fn chat_view(s: &App, cx: &mut rune::Cx<App>) -> El {
        let k = rune_kit::Kit::neutral(false);
        let c = &s.projects[0].convs["pr"];
        page(s, cx, || rune::col().key("conv-0-pr").w_full().child(crate::chat::entry(s, 0, "pr", &c.entries[0], &c.keys[0], None, &k).key(c.keys[0].clone())))
    }

    /// A chat's entry, narrow and wide: the box in its flow, the lines after the passage moved down.
    #[test]
    fn in_a_chats_entry_the_box_is_in_the_flow_under_the_lines() {
        for size in [NARROW, WIDE] {
            let (_held, mut s) = crate::view_cost::test_app();
            s.projects = vec![research(4, plain)];
            s.cur = 0;
            let t: coord_types::Transcript = serde_json::from_value(json!({"session": "s-1", "start": 0, "next": 900,
                "entries": [{"offset": 640, "at": "2026-11-21T08:40:00Z", "kind": "assistant", "text": long(40)}]})).unwrap();
            let mut c = crate::live::Conv::default();
            c.merge_last(t);
            s.projects[0].convs.insert("pr".into(), c);
            let mut rt = Runtime::new(s, chat_view, size, rune::Env::new(false, 1.0));
            settle(&mut rt);
            let sel = select(&mut rt, "e640-0", PASSAGE, "").expect("the entry's words drawn");
            rt.update(move |s, tx| crate::ask::comment_on(s, 0, "pr", &sel, tx));
            settle(&mut rt);
            assert_eq!(rt.state.notes.picked.as_ref().map(|p| p.source.as_str()), Some("pr@640"));
            let why = format!("{size:?}: box {:?}, room {:?}, lit {:?}, under {:?}", drawn(&rt, "cm-box"), drawn(&rt, super::ROOM), lit(&rt), under_box(&rt));
            in_room(&rt, "e640-0", "and every window waits", &why);
        }
    }

    /// The Threads page on N-7 beside a With you lens on N-7, the window wide (the split: two panes of about 1100),
    /// both threads scrolled to their tops: post 4 drawn in both panes.
    fn split() -> (std::sync::MutexGuard<'static, ()>, Runtime<App>) {
        let (held, mut s) = crate::view_cost::test_app();
        s.projects = vec![research(4, plain)];
        s.cur = 0;
        s.sel = Sel::Tickets;
        s.tickets.shown = Some(7);
        let lens = crate::lens::Lens { filter: crate::lens::Filter::WithYou { agent: "pr".into() }, depth: crate::lens::Depth::Structure, at: Some("N-7".into()) };
        s.drop_sel(0, Sel::Lens(lens), 0, crate::panes::Zone::Right);
        s.focus_pane(0);
        let mut rt = window(s, WIDE);
        to_top(&mut rt, DETAIL);
        to_top(&mut rt, "with-you-thread-N-7");
        (held, rt)
    }

    /// The comment box opened on post 4's passage with no element he selected it in (a mark opened again, a scene).
    fn opened_bare(s: &mut App) {
        let p = super::Picked { target: "thread:N-7".into(), source: "N-7.p4".into(), label: "post 4".into(), quote: PASSAGE.into(), at: Vec::new(), pi: 0, anchor: None, block: None, pane: None, texts: Vec::new() };
        crate::ask::write(s, p, "");
    }

    /// The rooms drawn for the comment box.
    fn rooms_drawn(rt: &Runtime<App>) -> Vec<[f32; 4]> {
        rt.elements().into_iter().filter(|e| !e.exiting && e.key.as_deref() == Some(super::ROOM)).map(|e| e.rect).collect()
    }

    /// The comment box's passage lit, its room (when it stands in the flow) and the box all in the split's pane J
    /// alone, and no text under the box.
    #[track_caller]
    pub(crate) fn stands_in(rt: &Runtime<App>, j: usize) {
        let within = |r: [f32; 4], p: [f32; 4]| r[0] >= p[0] - 0.5 && r[0] + r[2] <= p[0] + p[2] + 0.5;
        let p = drawn(rt, &crate::panes::key(j)).unwrap_or_else(|| panic!("pane-{j} drawn"));
        let why = format!("in pane-{j} {p:?} (With you {:?}, spot {:?}): {}", drawn(rt, WITH_YOU), rt.state.notes.spot.get(), rects(rt, 4));
        let lit = lit(rt);
        assert!(!lit.is_empty() && lit.iter().all(|r| within(*r, p)), "his passage lit in pane-{j} alone: {lit:?} {why}");
        let rooms = rooms_drawn(rt);
        assert!(rooms.len() == usize::from(rt.state.notes.inline.get()) && rooms.iter().all(|r| within(*r, p)), "one room, in pane-{j}: {rooms:?} {why}");
        assert!(drawn(rt, "cm-box").is_some_and(|b| within(b, p)), "the box in pane-{j}: {why}");
        assert!(under_box(rt).is_empty(), "nothing under the box: {why}");
    }

    /// Post 4's words drawn in the split's pane J: where they are.
    fn post_in(rt: &Runtime<App>, j: usize) -> Option<[f32; 4]> {
        text_at(rt, &crate::panes::key(j), "and every window waits")
    }

    /// A post drawn in two panes of the split, Threads beside With you on its thread (proto-window N-1477 p29): his
    /// passage is lit, its room drawn and the box stands only in the pane he selected it in, the focused one or the
    /// other, and with no element he selected in (a scene, a mark opened again) in the focused one; no text under the
    /// box, and the other pane's lines stay where they were.
    #[test]
    fn a_post_drawn_in_two_panes_lights_one_passage_and_has_one_room_in_the_pane_he_selected_in() {
        for case in [Some(0usize), Some(1), None] {
            let (j, other) = case.map_or((0, 1), |j| (j, 1 - j));
            let (_held, mut rt) = split();
            let was = post_in(&rt, other).unwrap_or_else(|| panic!("post 4 drawn in pane-{other} too: {}", rects(&rt, 4)));
            match case {
                Some(j) => {
                    let sel = select(&mut rt, &crate::panes::key(j), PASSAGE, "").unwrap_or_else(|| panic!("post 4's words drawn in pane-{j}"));
                    rt.update(move |s, tx| crate::ask::comment_on(s, 0, "thread:N-7", &sel, tx));
                }
                None => rt.update(|s, _| opened_bare(s)),
            }
            settle(&mut rt);
            stands_in(&rt, j);
            assert_eq!(post_in(&rt, other), Some(was), "pane-{other}'s lines where they were: {}", rects(&rt, 4));
        }
    }

    /// A comment box opened with no element he selected in stays in the pane it opened in when he focuses another
    /// (consult-3 N-1477 p46). Opened in With you (pane-1), focus back on the Threads page: its passage, its room and
    /// the box stay in pane-1, and pane-0's lines where they were. Opened on the Threads page (pane-0), which a pane
    /// not focused draws as its name only, focus on With you: none of it is drawn, not in With you, whose lines stay
    /// where they were; focus back on the Threads page: it stands there again.
    #[test]
    fn a_box_opened_with_no_element_stays_in_its_pane_when_he_focuses_another() {
        // pane J focused, the threads scrolled to their tops as split() has them: a thread drawn anew, focused or not,
        // starts at its foot (With you's, withyou.rs `around`)
        let focus = |rt: &mut Runtime<App>, j: usize| {
            rt.update(move |s, _| s.focus_pane(j));
            settle(rt);
            to_top(rt, WITH_YOU);
            if j == 0 {
                to_top(rt, DETAIL);
            }
        };
        for (j, other) in [(1usize, 0usize), (0, 1)] {
            let (_held, mut rt) = split();
            focus(&mut rt, other);
            let was = post_in(&rt, other).unwrap_or_else(|| panic!("post 4 drawn in pane-{other}, focused: {}", rects(&rt, 4)));
            focus(&mut rt, j);
            rt.update(|s, _| opened_bare(s));
            settle(&mut rt);
            stands_in(&rt, j);
            focus(&mut rt, other);
            let why = format!("opened in pane-{j}, pane-{other} focused: {}", rects(&rt, 4));
            assert_eq!(rt.state.notes.picked.as_ref().and_then(|p| p.pane), Some(j), "its pane: {why}");
            if post_in(&rt, j).is_some() {
                stands_in(&rt, j);
            } else {
                assert!(lit(&rt).is_empty() && rooms_drawn(&rt).is_empty() && drawn(&rt, "cm-box").is_none(), "its lines not drawn, none of it is: {why}");
            }
            assert_eq!(post_in(&rt, other), Some(was), "pane-{other}'s lines where they were: {why}");
            focus(&mut rt, j);
            stands_in(&rt, j);
        }
    }

    /// A pane closed before the comment box's (proto-window N-1477 p48): three panes, the Threads page, With you on
    /// N-7 and With you on N-7 again beside it, his passage selected in the third (pane-2). The Threads page closed,
    /// the box's pane is pane-1 now: its passage lit, its room and the box there, nothing lit in the With you before it
    /// (pane-0), which draws post 4 too. A thread drawn in a pane under a new key starts at its foot, so both are
    /// scrolled to their tops, as `split()` has them.
    #[test]
    fn a_pane_closed_before_the_boxs_leaves_it_in_its_pane_under_its_new_place() {
        let (_held, mut rt) = split();
        let lens = crate::lens::Lens { filter: crate::lens::Filter::WithYou { agent: "pr".into() }, depth: crate::lens::Depth::Structure, at: Some("N-7".into()) };
        rt.update(move |s, _| s.drop_sel(0, Sel::Lens(lens), 1, crate::panes::Zone::Right));
        settle(&mut rt);
        assert_eq!((rt.state.panes.len(), rt.state.focus), (3, 2));
        to_top_in(&mut rt, 2, WITH_YOU);
        let sel = select(&mut rt, &crate::panes::key(2), PASSAGE, "").unwrap_or_else(|| panic!("post 4's words drawn in pane-2: {}", rects(&rt, 4)));
        rt.update(move |s, tx| crate::ask::comment_on(s, 0, "thread:N-7", &sel, tx));
        settle(&mut rt);
        stands_in(&rt, 2);
        rt.update(|s, _| s.close_pane(0));
        settle(&mut rt);
        assert_eq!((rt.state.panes.len(), rt.state.focus), (2, 1));
        for j in 0..2 {
            to_top_in(&mut rt, j, WITH_YOU);
        }
        assert!(post_in(&rt, 0).is_some(), "post 4 drawn in pane-0 too: {}", rects(&rt, 4));
        stands_in(&rt, 1);
    }

    /// The comment box's pane moves with the split's panes, as they are put in, opened raw, closed, and as the split
    /// opens and ends, never with his focus (consult-3 N-1477 p46; proto-window p44): so do the keys that name it, its
    /// words' part's `-inJ`, and while the split stays its element's `pane-J`. Its own pane closed, it is in the one
    /// focused then. Selected in a popup, it is in none.
    #[test]
    fn the_comment_boxs_pane_moves_with_the_splits_panes_and_the_keys_that_name_it() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![research(4, plain)];
        s.cur = 0;
        s.sel = Sel::Tickets;
        let got = |s: &App| s.notes.picked.as_ref().map(|p| (p.pane, p.anchor.clone().unwrap_or_default(), p.block.clone().unwrap_or_default()));
        let is = |j: Option<usize>, a: &str, b: &str| Some((j, a.to_string(), format!("body-N-7.p4-a1{b}")));
        let pane = |j: usize| format!("root/main/pane-{j}/ticket-detail/post-N-7-4");
        let picked = |anchor: &str, block: &str| super::Picked { target: "thread:N-7".into(), source: "N-7.p4".into(), label: "post 4".into(), quote: PASSAGE.into(), at: Vec::new(), pi: 0, anchor: Some(anchor.into()), block: Some(block.into()), pane: None, texts: Vec::new() };
        // selected in the split's second pane
        s.split();
        assert_eq!((s.panes.len(), s.focus), (2, 1));
        crate::ask::write(&mut s, picked(&pane(1), "body-N-7.p4-a1-in1"), "");
        assert_eq!(got(&s), is(Some(1), &pane(1), "-in1"), "selected in pane-1");
        s.focus_pane(0);
        assert_eq!(got(&s), is(Some(1), &pane(1), "-in1"), "his focus moved");
        // a pane put in before it, focused
        s.drop_sel(0, Sel::Files, 0, crate::panes::Zone::Left);
        assert_eq!((s.panes.len(), s.focus), (3, 0));
        assert_eq!(got(&s), is(Some(2), &pane(2), "-in2"), "a pane put in before it");
        // a raw pane beside the focused one, before it
        s.open_raw(0, "pr");
        assert_eq!((s.panes.len(), s.focus), (4, 1));
        assert_eq!(got(&s), is(Some(3), &pane(3), "-in3"), "a raw pane put in before it");
        s.close_pane(0);
        assert_eq!((s.panes.len(), s.focus), (3, 0));
        assert_eq!(got(&s), is(Some(2), &pane(2), "-in2"), "the pane before it closed");
        s.close_pane(2);
        assert_eq!((s.panes.len(), s.focus), (2, 0));
        assert_eq!(got(&s), is(Some(0), &pane(0), "-in0"), "its own closed: the one focused then");
        s.close_pane(1);
        assert!(s.panes.is_empty());
        assert_eq!(got(&s), is(Some(0), &pane(0), ""), "the split ended: the page, its element's path left as it was");
        // selected with no split, a pane put in before the page as the split opens
        let page = "root/main/ticket-detail/post-N-7-4";
        crate::ask::write(&mut s, picked(page, "body-N-7.p4-a1"), "");
        assert_eq!(got(&s), is(Some(0), page, ""), "no split: the page");
        s.drop_sel(0, Sel::Files, 0, crate::panes::Zone::Left);
        assert_eq!((s.panes.len(), s.focus), (2, 0));
        assert_eq!(got(&s), is(Some(1), page, "-in1"), "the split opened, the page put after the new pane");
        // selected in a popup: in none of the panes, with the split or without
        let popup = "root/popup-N-7/post-N-7-4";
        crate::ask::write(&mut s, picked(popup, "body-N-7.p4-a1"), "");
        assert_eq!(got(&s), is(None, popup, ""), "a popup's, in the split");
        assert_eq!(super::box_pane(&s), None);
        s.close_pane(1);
        assert_eq!(got(&s), is(None, popup, ""), "a popup's, the split ended");
        crate::ask::write(&mut s, picked(popup, "body-N-7.p4-a1"), "");
        s.split();
        assert_eq!(got(&s), is(None, popup, ""), "a popup's, the split opened");
        assert_eq!(super::box_pane(&s), None);
    }

    /// A surface whose words were split for the room draws none at its end (consult-3 N-1477 p30): in the pass the box
    /// opens, its passage not laid out yet, both drew, and the box took the first room by the tree's order.
    #[test]
    fn a_surface_that_split_its_words_for_the_room_draws_none_at_its_end() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.notes.picked = Some(super::Picked { target: "thread:N-7".into(), source: "N-7.p4".into(), label: "post 4".into(), quote: PASSAGE.into(), at: Vec::new(), pi: 0, anchor: None, block: None, pane: None, texts: Vec::new() });
        s.notes.inline.set(true);
        s.notes.lit.set(false);
        assert!(super::room_at(&s, "post-N-7-4").is_some(), "its words not split: the room at the post's end");
        assert!(super::room(&s, "N-7.p4").is_some(), "its words split: the room in them");
        assert!(super::room_at(&s, "post-N-7-4").is_none(), "and none at the post's end");
    }

    #[test]
    fn a_selections_element_is_named_by_its_keys() {
        assert_eq!(super::keyed("root/#0/with-you/#2/post-N-7-4/#1/#0", Some("post-N-7-4")).as_deref(), Some("root/with-you/post-N-7-4"));
        assert_eq!(super::keyed("root/#0/file-md/#3", None).as_deref(), Some("root/file-md"));
        assert_eq!(super::keyed("root/#0/post-N-7-4", Some("qmsg-M-1")), None);
        assert_eq!(super::holder("N-7.p4").as_deref(), Some("post-N-7-4"));
        assert_eq!(super::holder("M-12").as_deref(), Some("qmsg-M-12"));
        assert_eq!(super::holder("pr@1024"), None);
        assert_eq!(super::holder("file:/w/edge8/plan.md").as_deref(), Some(crate::files::FILE_KEY));
        // the words after the room are placed in the words they were split from
        assert_eq!(super::after("body-N-7.p4-1f2e-after120"), ("body-N-7.p4-1f2e", 120));
        assert_eq!(super::after("body-docket-aftermath@12-ab"), ("body-docket-aftermath@12-ab", 0));
        assert_eq!(super::in_source("root/body-N-7.p4-1f2e-after120/b0", &[3..9]), vec![123..129]);
    }

    /// What rune-kit draws of BLOCKS, in order: their words, a list's item as its depth and its number, a heading's
    /// level, the quote each stands in, code by its lines.
    fn outline(blocks: &[rune_kit::markdown::Block], depth: usize, out: &mut Vec<String>) {
        use rune_kit::markdown::Block;
        let words = |t: &str, out: &mut Vec<String>| out.extend(t.split_whitespace().map(String::from));
        for b in blocks {
            match b {
                Block::Para(i) => words(&i.text, out),
                Block::Heading(l, i) => {
                    out.push(format!("h{l}"));
                    words(&i.text, out);
                }
                Block::Code(c) | Block::Diff(c) | Block::Mermaid(c) => out.extend(c.lines().map(|l| format!("code {l}"))),
                Block::List { start, items } => {
                    for (n, item) in items.iter().enumerate() {
                        out.push(format!("item {depth} {:?}", start.map(|s| s + n as u64)));
                        outline(item, depth + 1, out);
                    }
                }
                Block::Quote(q) => {
                    let mut inner = Vec::new();
                    outline(q, depth, &mut inner);
                    out.extend(inner.into_iter().map(|w| format!("> {w}")));
                }
                Block::Rule => out.push("rule".into()),
                Block::Table(t) => {
                    out.push("table".into());
                    for cell in t.head.iter().chain(t.rows.iter().flatten()) {
                        words(&cell.text, out);
                    }
                }
                Block::Image { src, .. } => out.push(format!("image {src}")),
            }
        }
    }

    /// Where words split for the box's room, read from the events they are drawn from (consult-3 N-1477 p30): in a
    /// paragraph at the first space after the passage in no span, a list at its top item's end, a code block at its
    /// fence. Each row drawn by rune-kit in its two halves shows what the whole does: the same words, items at the
    /// same depths with the same numbers, the same headings, quotes and code lines (proto-window p32).
    #[test]
    fn words_split_for_the_room_after_the_passage_and_never_inside_a_span_or_a_block() {
        let drawn = |src: &str| {
            let mut out = Vec::new();
            outline(&rune_kit::markdown::parse(src), 0, &mut out);
            out
        };
        let cut = |src: &str, upto: &str| {
            let at = super::room_cut(src, src.find(upto).unwrap() + upto.len());
            let (before, after, n) = super::parts(src, at);
            // the words after placed in SRC from N (`in_source`): those that came from SRC, where they were in it
            assert_eq!(&after[at.1 - n..], &src[at.1..], "N {n} places the words after: {after:?}");
            assert_eq!([drawn(&before), drawn(&after)].concat(), drawn(src), "{src:?} drawn in halves {before:?} | {after:?}");
            (before, after)
        };
        assert_eq!(cut("one two three four", "two"), ("one two".into(), "three four".into()));
        assert_eq!(cut("one **two three** four", "two"), ("one **two three**".into(), "four".into()));
        assert_eq!(cut("see [the lock](file:a.rs) now", "the"), ("see [the lock](file:a.rs)".into(), "now".into()));
        assert_eq!(cut("in `a b c` here", "a"), ("in `a b c`".into(), "here".into()));
        assert_eq!(cut("first, - not a list", "first"), ("first, -".into(), "not a list".into()));
        assert_eq!(cut("- one two\n  three\n- four\n\nafter", "two"), ("- one two\n  three".into(), "- four\n\nafter".into()));
        // a list at the end of its top item: the items after it keep their depth and their numbers (consult-3 p30)
        assert_eq!(cut("- a\n  - b one\n  - c\n- d", "b"), ("- a\n  - b one\n  - c".into(), "- d".into()));
        assert_eq!(cut("1. a\n   1. b\n   2. c\n2. d", "b"), ("1. a\n   1. b\n   2. c".into(), "2. d".into()));
        // emphasis and strong written with `_`, a setext heading
        assert_eq!(cut("an _emphasised passage here_ ok", "emphasised"), ("an _emphasised passage here_".into(), "ok".into()));
        assert_eq!(cut("an __strong passage here__ ok", "strong"), ("an __strong passage here__".into(), "ok".into()));
        assert_eq!(cut("Title words\n---\n\nafter", "Title"), ("Title words\n---".into(), "after".into()));
        // lines rune-kit draws as a list's items (`N. ` in a paragraph): at the item's end; before them, in the words
        assert_eq!(cut("**More juice**\n4. Hash ripple and\nmore words.\n5. Next one.", "ripple"), ("**More juice**\n4. Hash ripple and\nmore words.".into(), "5. Next one.".into()));
        assert_eq!(cut("**More juice**\n4. Hash ripple.\n5. Next one.", "Hash"), ("**More juice**\n4. Hash ripple.".into(), "5. Next one.".into()));
        assert_eq!(cut("intro words here\n4. one\n5. two", "intro"), ("intro".into(), "words here\n4. one\n5. two".into()));
        // a `N. ` line after a break inside a span is no item: one paragraph, split after the span (consult-3 p36)
        assert_eq!(cut("*an emphasised\n4. line* after", "an"), ("*an emphasised\n4. line*".into(), "after".into()));
        // fenced code: closed at the lit line, opened again after the room with its fence, info and quote (p25)
        assert_eq!(cut("```\nlet a = 1;\nlet b = 2;\n```\nafter", "a = 1"), ("```\nlet a = 1;\n```".into(), "```\nlet b = 2;\n```\nafter".into()));
        assert_eq!(cut("```diff\n- a\n+ b\n  c\n```", "+ b"), ("```diff\n- a\n+ b\n```".into(), "```diff\n  c\n```".into()));
        assert_eq!(cut("````md\na\nb\n```\nc\n````\nafter", "b"), ("````md\na\nb\n````".into(), "````md\n```\nc\n````\nafter".into()));
        assert_eq!(cut("> ```sh\n> one\n> two\n> ```\n\nafter", "one"), ("> ```sh\n> one\n> ```".into(), "> ```sh\n> two\n> ```\n\nafter".into()));
        // its last line: after the block; in a list's item: at the item's end
        assert_eq!(cut("```\nx\ny\n```\nafter", "y"), ("```\nx\ny\n```".into(), "after".into()));
        assert_eq!(cut("- item\n  ```\n  one\n  two\n  ```\n  more\n- next", "one"), ("- item\n  ```\n  one\n  two\n  ```\n  more".into(), "- next".into()));
        // a table, with outer pipes or none: after the whole table
        assert_eq!(cut("| a | b |\n|---|---|\n| one | two |\n| three | four |\n\nafter", "one"), ("| a | b |\n|---|---|\n| one | two |\n| three | four |".into(), "after".into()));
        assert_eq!(cut("a | b\n--|--\none | two\nthree | four\n\nafter", "one"), ("a | b\n--|--\none | two\nthree | four".into(), "after".into()));
        assert_eq!(cut("# A head\n\nwords", "A"), ("# A head".into(), "words".into()));
        assert_eq!(cut("the end", "end"), ("the end".into(), String::new()));
    }
}
