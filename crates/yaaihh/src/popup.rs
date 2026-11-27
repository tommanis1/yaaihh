//! Big things opened apart (the owner, N-1104 p562: "very big stuff like opening a request what the model was sent
//! etc should open in a new window popup instead of inline ... otherwise it destroys the chat and i cannot find the
//! close at the top again"; N-1396 p276): the work behind a reply, a step's prompt, a reading of an answer, what an
//! agent read, the summary a compaction left, a tool call's or result's whole text and an edit's whole diff, a
//! ticket's whole program, and in a halt's card the prompt it would give, a form's long field, the program from the
//! halt on and an answer that could not be read, open in a panel over the window as tall as the window (rune-kit's
//! `Dialog::fill`), titled with what it is, its id and label, its Close fixed at the top while its body scrolls. The
//! chat under it keeps its place; Close, Escape or a click beside it returns to it. Something big pressed inside a
//! popup opens over it, and its Close returns to the one under it as it was. A press inside one that goes somewhere
//! (a mention, an agent's name, a ticket of a program's tree) closes them all, and the page shows where it went: they
//! close when the page makes a place (history.rs `poll`, wherever the code set it) and when a post is opened
//! (find.rs `open_at`, a post of the thread under them too; consult-3, N-1396 p304).
//!
//! An agent's folder (sidebar.rs, the file view) opens a version's words here too (paper-research N-1446 p1633).
//!
//! The rule (consult-3, N-1396 p282): big content goes apart wherever it is, except in a pane whose job is to show it
//! (the Docket's Thread scale shows a thread's whole program in place, kinds.rs; the Work view a session's).
//!
//! Not a second OS window: rune opens one (`rune::open_window`), but a window draws and takes presses on a state of
//! its own, and every view drawn here reads the window's `App` (N-1396 p276, 1). What a model was sent is the
//! exception: a turn's requests and a call's prompt need nothing of the window once read, and open in a window of their
//! own from the popup they are pressed in (sent_window.rs; paper-research N-1446 p2112, 2). [`open`] is the one way in
//! to a popup.
//!
//! Small things stay in place: a line, a short quote, a message's own words ("› the whole message"), a run of calls
//! and a call's first lines (work.rs, chat.rs).

use crate::App;
use docket_machine::Loc;
use rune::{El, ScrollSpec, col, text};
use rune_kit::{Dialog, Kit};

/// A popup's widest: a request's lines and a file read at a measure that still reads.
const WIDTH: f32 = 1100.0;

/// What a popup shows, each drawn by the view that drew it in place before.
#[derive(Clone, Debug, PartialEq)]
pub enum What {
    /// The turn behind agent AGENT's post POST (`N-7.p2`) made at AT, in project PI: its rows, then what the model was
    /// sent behind a fold (turn.rs).
    Work { pi: usize, agent: String, post: String, at: String },
    /// Entry KEY of AGENT's session in project PI, whole: a tool call's or result's text, an edit's diff (chat.rs).
    Entry { pi: usize, agent: String, key: String },
    /// The summary AGENT left when its context was compacted, its entry KEY in project PI (streams.rs).
    Summary { pi: usize, agent: String, key: String },
    /// A text the ticket page reads at its place, without its channel's tag lines: a step's prompt (ran.rs), what an
    /// agent was given (ticket.rs).
    Sent { place: Loc },
    /// Reading FILE of ticket T's answers: the answer, each call's reply, and what each call was sent (ticket.rs).
    Reading { t: u64, file: String },
    /// Ticket N's whole program, every branch, with the panel of a step or stage pressed in it beside it (ticket.rs).
    Program { n: u64 },
    /// PART of halt HN of the ticket called ID, from its card in the thread (slow.rs).
    Halt { id: String, hn: u32, part: crate::slow::Part },
    /// Row ID of agent AGENT's folder in project PI, as coordd last gave it: the words that changed into a version of
    /// its text (sidebar.rs `folder_words`; paper-research N-1446 p1633).
    Folder { pi: usize, agent: String, id: String },
    /// The requests of session SESSION (its node) of agent AGENT's folder in project PI, its row ID, as coordd gives
    /// them (sidebar.rs `requests_apart`; paper-research N-1446 p1642, 3).
    Requests { pi: usize, agent: String, id: String, session: String },
    /// Block ID (`text:…`) a request of agent AGENT carried, in project PI's record, pressed on its card in the cloud
    /// (cloud.rs `pressed`; paper-research N-1446 p1821, H3): its text as coordd reads it from the session, as a
    /// folder's block's (sidebar.rs `block_text_el`).
    Block { pi: usize, agent: String, id: String },
    /// His edit of agent AGENT's system prompt in project PI, opened from a version's card on the Map (cloud.rs `card`;
    /// paper-research N-1446 p3041, 3): the System prompt pane's editor itself (prompt_edit.rs `editor`), and once the
    /// engine took it, what the pane would say of it.
    PromptEdit { pi: usize, agent: String },
    /// What is behind ticket N of the current project, a line a rule or an agent's brain made on thread ON (the owner,
    /// N-1790 part 3; outline.rs `behind`): its verdict and rule, what went in and what came out, and its wire and its
    /// Docket a press away.
    Behind { n: u64, on: u64 },
}

#[derive(Clone, Debug)]
pub struct Popup {
    pub title: String,
    pub what: What,
}

/// A popup's title: what it is, the id it belongs to, and that one's label (the owner's "id plus label"); the empty
/// ones left out.
pub fn title(label: &str, id: &str, what: &str) -> String {
    [label, id, what].iter().filter(|w| !w.is_empty()).copied().collect::<Vec<_>>().join(" · ")
}

/// Open WHAT, titled TITLE, over what is shown: the chat, or the popup it was pressed in.
pub fn open(s: &mut App, title: impl Into<String>, what: What) {
    // a second press on what is open on top opens nothing more
    if s.popups.last().is_some_and(|p| p.what == what) {
        return;
    }
    s.popups.push(Popup { title: title.into(), what });
}

/// Every popup closed: the page under them is moving (a press in one went somewhere), and shows where it went.
pub fn close(s: &mut App) {
    s.popups.clear();
}

/// Whether WHAT is open, under the top one or on it.
pub fn shows(s: &App, what: &What) -> bool {
    s.popups.iter().any(|p| p.what == *what)
}

/// B with MORE of its lines left out: how many, under it, and a press on it opening WHAT apart, titled TITLE; B itself
/// when none are.
pub fn more(b: El, more: usize, title: String, what: What, k: &Kit) -> El {
    if more == 0 {
        return b;
    }
    let b = b.child(text(format!("… {more} more lines")).font(k.ty.small).color(k.c.text_faint));
    k.interactive(b).on_press(move |s: &mut App| open(s, title.clone(), what.clone()))
}

/// The popups over the window, the newest on top; each one's Close, Escape or a click beside it closes it and those
/// over it. Its text is commented on as a thread's is: a passage selected offers Comment, and the box opens under it,
/// over the popup ([`comment`]; I-1476). Each as tall as what it holds, up to the window: its body scrolls past that
/// (paper-research N-1446 p1889, 2: a window-high panel held one line). The window's height and its head's, as the
/// pass before laid them out (CX: rune runs the view again while what it read moves, before a frame is shown).
pub fn layer(s: &App, cx: &mut rune::Cx<App>, k: &Kit, layer: &mut Vec<El>) {
    let window = cx.window_layout("kit-root").map(|r| r[3]);
    for (i, p) in s.popups.iter().enumerate() {
        // the comment box's room at its body's end, when the passage commented on in it is lit nowhere (ask.rs
        // `room_at`): the box stands there, over nothing of it
        let inner = col().key(body_key(i)).w_full().px(16.0).pb(16.0).gap(8.0).child(body(s, i, &p.what, k)).children(crate::ask::room_at(s, &body_key(i)));
        let inner = if goes_to(s, &p.what).is_some() { crate::ask::commentable_apart(inner, move |s: &mut App, sel| comment(s, i, sel)) } else { inner };
        // every change in it lands at once, as in the page (lib.rs `view`): the comment box's room grows with the box
        // in the frame it does, the lines after it never under it (consult-3 N-1477 p13, p25)
        let scroll = format!("popup{i}-scroll");
        // the room under its head: the window less the layer's margin above and below (rune-kit `Dialog`, 16 each)
        let head = cx.window_layout(&format!("popup{i}-dialog")).zip(cx.window_layout(&scroll)).map(|(d, b)| d[3] - b[3]);
        let mut body = col().key(scroll).w_full().min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).instant_changes().child(inner);
        if let (Some(w), Some(h)) = (window, head) {
            body = body.max_h((w - 32.0 - h).max(0.0));
        }
        // and the panel's own changes land at once too, its lines in it: as it grows with what it holds (its text
        // come, the comment box's room) it is never sprung there (rune-kit `Dialog`'s `token_motion`) carrying them
        // under the box, which stands where they are laid
        let mut built = Vec::new();
        Dialog::new(format!("popup{i}"), p.title.clone())
            .fit()
            .width(WIDTH)
            .open(true)
            .child(body)
            .on_close(move |s: &mut App| s.popups.truncate(i))
            .build(k, &mut built);
        layer.extend(built.into_iter().map(El::instant_changes));
    }
}

/// Where a passage of popup WHAT commented on goes, and its pane's project: the agent whose session it shows (its
/// chat, as chat.rs's comments go); a ticket's text, the thread the page under it shows (history.rs `thread_shown`:
/// the Threads page's, the one beside an agent's With you, never one left on a page he is not on; consult-3 N-1477
/// p64, proto-window p65), else the thread the halt or the ticket is. None: there is no thread to send it with, or its
/// body lights no passage and leaves the box no room (a turn's work, its entries drawn as the chat's; a program's
/// picture; a version's words), so its text offers no Comment: a box there would cover his lines (paper-research N-1477
/// p9, p18).
pub(crate) fn goes_to(s: &App, what: &What) -> Option<(usize, String)> {
    let thread = |v: &coord_types::TicketView| (v.kind == "thread").then(|| crate::threads::target(v));
    let p = s.projects.get(s.cur);
    let shown = p.and_then(|p| crate::find::ticket(s, p, crate::history::thread_shown(s, s.cur, &s.sel)?)).and_then(thread);
    let own = match what {
        What::Entry { pi, agent, .. } | What::Summary { pi, agent, .. } => return Some((*pi, agent.clone())),
        What::Halt { id, .. } => p.and_then(|p| p.live.tickets.values().find(|v| v.id == *id)).and_then(thread),
        What::Reading { t: n, .. } => p.and_then(|p| p.live.tickets.get(n)).and_then(thread),
        What::Sent { .. } => None,
        What::Work { .. } | What::Program { .. } | What::Folder { .. } | What::Requests { .. } | What::Block { .. } | What::PromptEdit { .. } | What::Behind { .. } => return None,
    };
    Some((s.cur, shown.or(own)?))
}

/// The prefix of a popup's comment sources ([`source`]).
const SOURCE: &str = "popup:";

/// The source a passage of popup WHAT is commented on as: what it shows, apart from the page's own sources, so its
/// passage is lit in the popup only, never in the post or chat under it.
fn source(what: &What) -> String {
    let at = match what {
        What::Work { post, .. } => format!("work:{post}"),
        What::Entry { agent, key, .. } => format!("entry:{agent}:{key}"),
        What::Summary { agent, key, .. } => format!("summary:{agent}:{key}"),
        What::Sent { place } => format!("sent:{}", crate::texts::key(place)),
        What::Reading { t, file } => format!("reading:{t}:{file}"),
        What::Program { n } => format!("program:{n}"),
        What::Halt { id, hn, part } => format!("halt:{id}:{hn}:{part:?}"),
        What::Folder { agent, id, .. } => format!("folder:{agent}:{id}"),
        What::Requests { agent, session, .. } => format!("requests:{agent}:{session}"),
        What::Block { agent, id, .. } => format!("block:{agent}:{id}"),
        What::PromptEdit { agent, .. } => format!("prompt-edit:{agent}"),
        What::Behind { n, .. } => format!("behind:{n}"),
    };
    format!("{SOURCE}{at}")
}

/// Whether comment source X is a popup's ([`source`]): its box stands over the popups (ask.rs `apart`).
pub fn is_source(x: &str) -> bool {
    x.starts_with(SOURCE)
}

/// The key of popup I's text drawn plain (a prompt, a tool's whole result), lit where a passage of it is commented on
/// (ask.rs `lit_words`).
fn text_key(i: usize) -> String {
    format!("popup{i}-text")
}

/// The key of popup I's body, the element at whose end the comment box's room stands when its passage is lit nowhere
/// (ask.rs `room_at`, `comment_apart`).
fn body_key(i: usize) -> String {
    format!("popup{i}-body")
}

/// Comment on SEL, selected in popup I (its menu's Comment): to where it goes ([`goes_to`]), named by the popup's title,
/// its box opened under the lines selected (ask.rs `comment_apart`).
pub(crate) fn comment(s: &mut App, i: usize, sel: &rune::TextSelection) {
    let Some(p) = s.popups.get(i) else { return };
    let Some((pi, target)) = goes_to(s, &p.what) else { return };
    let (source, label) = (source(&p.what), p.title.clone());
    crate::ask::comment_apart(s, pi, &target, &source, &label, Some(&body_key(i)), sel);
}

/// QUOTE, selected in a window of its own (sent_window.rs) titled LABEL, opened from the popup titled TITLE showing
/// FROM, commented on as a passage of that popup is ([`comment`]): to where its comments go, its box at its foot; the
/// popup opened again over the page when it was closed since.
pub(crate) fn comment_from(s: &mut App, title: &str, from: &What, label: &str, quote: &str) {
    if !shows(s, from) {
        open(s, title, from.clone());
    }
    let Some(i) = s.popups.iter().rposition(|p| p.what == *from) else { return };
    let Some((pi, target)) = goes_to(s, from) else { return };
    crate::ask::comment_quote(s, pi, &target, &source(from), label, &body_key(i), quote);
}

/// What a popup no longer finds says so: its session was read again from elsewhere, or its page moved.
const GONE: &str = "No longer held.";

/// Popup I's body: WHAT, its passage being commented on lit in it ([`source`], [`text_key`]).
fn body(s: &App, i: usize, what: &What, k: &Kit) -> El {
    let note = |t: &str| text(t.to_string()).font(k.ty.small).color(k.c.text_faint).wrap();
    let (source, key) = (source(what), text_key(i));
    let apart = (key.as_str(), source.as_str());
    match what {
        What::Work { pi, agent, post, at } => crate::turn::work(s, *pi, agent, post, at, k),
        What::Entry { pi, agent, key } => crate::chat::whole(s, *pi, agent, key, apart, k).unwrap_or_else(|| note(GONE)),
        What::Summary { pi, agent, key } => crate::streams::summary(s, *pi, agent, key, &source, k).unwrap_or_else(|| note(GONE)),
        What::Sent { place } => {
            let t = match s.tickets.texts.get(&crate::texts::key(place)) {
                // as the thread shows it: without the channel's tag lines
                Some(Some(Ok(t))) => crate::ran::without_channel(t),
                Some(Some(Err(e))) => format!("(not read: {e})"),
                _ => "…".into(),
            };
            col().w_full().gap(6.0).child(note("The text as sent.")).child(crate::ticket::lit_boxed(Some((s, key.as_str(), source.as_str())), t, k))
        }
        What::Reading { t, file } => match s.tickets.forming.get(t) {
            Some(Ok(rs)) => rs.iter().find(|r| r.file == *file).map_or_else(|| note(GONE), |r| crate::ticket::reading_body(*t, r, Some((s, key.as_str(), source.as_str())), k)),
            _ => note(GONE),
        },
        What::Program { n } => s.projects.get(s.cur).and_then(|p| Some((p, crate::find::ticket(s, p, *n)?))).map_or_else(|| note(GONE), |(p, v)| {
            // keyed apart from the page's own picture under it, which keeps its stages
            let part = col().key("picture-whole").w_full().gap(8.0).child(crate::ticket::whole_program(s, p, v, k));
            crate::kinds::beside(s, p, v, part, k)
        }),
        What::Halt { id, hn, part } => crate::slow::part(s, id, *hn, part, apart, k).unwrap_or_else(|| note("No longer held.")),
        What::Folder { pi, agent, id } => crate::sidebar::folder_words(s, *pi, agent, id, k).unwrap_or_else(|| note("No longer in the folder.")),
        What::Requests { pi, agent, session, .. } => crate::sidebar::requests_apart(s, *pi, agent, session, k),
        What::Block { pi, id, .. } => crate::sidebar::block_text_el(s, *pi, id, id, k),
        What::PromptEdit { pi, agent } => crate::prompt_edit::editor(s, *pi, agent, k).unwrap_or_else(|| note("Closed: nothing of it was sent.")),
        What::Behind { n, on } => crate::outline::behind(s, *n, *on, k).unwrap_or_else(|| note(GONE)),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::What;
    use crate::App;
    use rune::runtime::Runtime;
    use rune::{Key, KeyEvent, Mods};
    use rune_kit::Kit;
    use serde_json::json;

    const AGENT: &str = "paper-research";
    const SUMMARY: &str = "This session is being continued from a previous conversation that ran out of context. The summary below: the popup piece is built.";

    /// 19 Nov 2026 at 19:MIN:SEC, as the records write it.
    fn at(min: u32, sec: u32) -> String {
        format!("2026-11-19T19:{min:02}:{sec:02}Z")
    }

    /// A made-up project (nothing is read from anywhere): thread N-1104 of 22 posts, his and paper-research's in turn,
    /// each of six paragraphs, taller than the window; in paper-research's session the turn behind p21 (his prompt, 30
    /// thoughts, the second naming N-1104.p3 and @paper-research, a Read of 300 lines, an Edit whose diff is 40 lines,
    /// the reply), its one request in the proxy's log,
    /// and a compaction after the thread's last post; on the Threads page, N-1104 shown.
    pub(crate) fn chat_app() -> (std::sync::MutexGuard<'static, ()>, App) {
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.live.agents = vec![serde_json::from_value(json!({"id": "AGT-1", "name": AGENT, "parent": "owner", "kind": "claude", "model": "m",
            "state": "idle", "since": at(0, 0), "tools": [], "items": []})).unwrap()];
        let posts: Vec<_> = (1..=22u32)
            .map(|n| json!({"n": n, "at": at(n, 0), "by": if n % 2 == 0 { "owner" } else { AGENT },
                "text": format!("Post {n}, its first paragraph.\n\nIts second.\n\nIts third.\n\nIts fourth.\n\nIts fifth.\n\nAnd its sixth."),
                "names": [], "told": []}))
            .collect();
        p.live.tickets.insert(1104, serde_json::from_value(json!({"id": "N-1104", "n": 1104, "kind": "thread", "opener": "owner", "ctx": {},
            "form": {"to": AGENT}, "title": "the owner's chat", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1,
            "opened_at": at(0, 0), "given": [], "posts": posts})).unwrap());
        let mut entries = vec![json!({"offset": 10, "at": at(20, 0), "kind": "mail",
            "text": "<channel source=\"coord\" from=\"owner\" ticket=\"N-1104\" id=\"N-1104.p20\" kind=\"owner\">\nPost 20\n</channel>"})];
        let thought = |i: u32| if i == 1 { "Thought 1: weighing the popup, after N-1104.p3; @paper-research knows.".to_string() } else { format!("Thought {i}: weighing the popup.") };
        entries.extend((0..30u32).map(|i| json!({"offset": 20 + i, "at": at(20, 1 + i), "kind": "assistant", "text": thought(i)})));
        let read: Vec<String> = (1..=300).map(|i| format!("line {i}")).collect();
        let diff: Vec<String> = (1..=40).map(|i| format!("+added {i}")).collect();
        entries.extend([
            json!({"offset": 400, "at": at(20, 31), "kind": "tool", "tool": "Read", "text": "{\"file_path\": \"src/popup.rs\"}"}),
            json!({"offset": 410, "at": at(20, 32), "kind": "result", "text": read.join("\n")}),
            json!({"offset": 420, "at": at(20, 33), "kind": "tool", "tool": "Edit", "text": "{\"file_path\": \"src/popup.rs\"}"}),
            json!({"offset": 430, "at": at(20, 34), "kind": "result", "text": "edited", "patch": diff.join("\n")}),
            json!({"offset": 440, "at": at(20, 59), "kind": "tool", "tool": "mcp__coord__reply", "text": json!({"to": "N-1104.p20", "text": "Post 21"}).to_string()}),
            json!({"offset": 450, "at": at(21, 0), "kind": "result", "text": "posted N-1104.p21"}),
            json!({"offset": 460, "at": at(45, 0), "kind": "user", "text": SUMMARY}),
        ]);
        let entries = serde_json::from_value(serde_json::Value::Array(entries)).unwrap();
        p.convs.entry(AGENT.into()).or_default().merge(coord_types::Transcript { entries, start: 0, next: 500, session: "s".into(), ..Default::default() }, false);
        s.projects = vec![p];
        s.cur = 0;
        let n = |t: &str| crate::turn::nanos(t).unwrap();
        let lines = ["══ 2026-11-19 19:20:31.000 · claude-opus-5-5 · status 200 ══", "[system]\nYou are paper-research.", "[messages] 1", "[0 user text]\nPost 20", "[reply text]\nOn it"];
        crate::raw::hold_turn(&mut s, 0, AGENT, "N-1104.p21", n(&at(20, 0)), n(&at(21, 5)), &lines, vec![n(&at(20, 31))]);
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = Some(1104);
        (held, s)
    }

    pub(crate) fn settle(rt: &mut Runtime<App>) {
        for _ in 0..40 {
            rt.frame(1.0 / 60.0);
        }
    }

    /// The rect of the first element drawn under the one keyed UNDER whose text holds WORDS.
    pub(crate) fn text_under(rt: &Runtime<App>, under: &str, words: &str) -> Option<[f32; 4]> {
        rt.elements().into_iter().find(|e| !e.exiting && e.path.split('/').any(|k| k == under) && e.text.as_deref().is_some_and(|t| t.contains(words))).map(|e| e.rect)
    }

    /// Whether any element outside the popups has text holding WORDS.
    fn outside(rt: &Runtime<App>, words: &str) -> bool {
        rt.elements().into_iter().any(|e| !e.exiting && !e.path.split('/').any(|k| k.starts_with("popup")) && e.text.as_deref().is_some_and(|t| t.contains(words)))
    }

    fn keyed(rt: &Runtime<App>, key: &str) -> Option<[f32; 4]> {
        rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).map(|e| e.rect)
    }

    /// Wheel the view under X until FIND's rect has its top between TOP and BOTTOM (by UNSEEN while it is not drawn:
    /// negative down, positive up); that rect.
    fn bring(rt: &mut Runtime<App>, x: f32, top: f32, bottom: f32, unseen: f32, find: impl Fn(&Runtime<App>) -> Option<[f32; 4]>) -> Option<[f32; 4]> {
        for _ in 0..60 {
            match find(rt) {
                Some(r) if r[1] >= top && r[1] <= bottom => return Some(r),
                Some(r) => rt.wheel(x, 450.0, if r[1] < top { 150.0 } else { -150.0 }),
                None => rt.wheel(x, 450.0, unseen),
            }
            settle(rt);
        }
        find(rt)
    }

    pub(crate) fn press(rt: &mut Runtime<App>, r: [f32; 4]) {
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        settle(rt);
    }

    pub(crate) fn escape(rt: &mut Runtime<App>) {
        rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
        settle(rt);
    }

    /// N-1396 p276, 4: "› the work behind it" on p21, the thread scrolled to its middle, opens the turn apart: its title
    /// says what it is, the thread's posts keep their places and nothing is drawn under the post; in it "› what the
    /// model was sent" opens the request in a window of its own (sent_window.rs; paper-research N-1446 p2112, 2) and
    /// nothing of it in the popup; its Close stays at the top while its body is wheeled to its end; Escape leaves the
    /// thread as it was.
    #[test]
    fn the_work_behind_a_reply_opens_apart_and_the_thread_keeps_its_place() {
        let (_held, s) = chat_app();
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        // down the thread (it opens at its top) until p21 stands in the window's middle
        let drawn = |rt: &Runtime<App>| rt.elements().into_iter().filter_map(|e| e.key.filter(|k| k.starts_with("post-")).map(|k| (k, e.rect))).collect::<Vec<_>>();
        let p21 = bring(&mut rt, 900.0, 250.0, 550.0, -150.0, |rt| keyed(rt, "post-N-1104-21"));
        assert!(p21.is_some_and(|r| r[1] >= 250.0 && r[1] <= 550.0), "p21 in the window's middle: {p21:?} {:?}", drawn(&rt));
        let posts = ["post-N-1104-20", "post-N-1104-21", "post-N-1104-22"];
        let places = |rt: &Runtime<App>| posts.map(|k| keyed(rt, k));
        let before = places(&rt);
        let toggle = text_under(&rt, "post-N-1104-21", "› the work behind it").expect("the toggle on p21");
        press(&mut rt, toggle);
        assert_eq!(rt.state.popups.len(), 1);
        let p = &rt.state.popups[0];
        assert!(matches!(&p.what, What::Work { pi: 0, agent, post, .. } if agent == AGENT && post == "N-1104.p21"), "{:?}", p.what);
        assert_eq!(p.title, "the owner's chat · N-1104.p21 · the work behind paper-research's post");
        assert!(text_under(&rt, "popup0-dialog", "Thought 0: weighing").is_some(), "the turn's rows in the popup");
        assert!(!outside(&rt, "Thought 0") && !outside(&rt, "what the model was sent"), "nothing of it drawn in the thread");
        assert_eq!(places(&rt), before, "the thread's posts where they were");
        // the popup at its top: its Close, its body, its first row
        let close_of = |rt: &Runtime<App>| rt.elements().into_iter().find(|e| e.key.as_deref() == Some("close") && e.path.split('/').any(|k| k == "popup0-dialog")).map(|e| e.rect);
        let close = close_of(&rt).expect("the Close");
        let body = keyed(&rt, "popup0-scroll").expect("the popup's body");
        let first = text_under(&rt, "popup0-dialog", "Thought 0: weighing").unwrap();
        assert!(close[1] < body[1], "the Close above the body");
        // what the model was sent, its line at the turn's foot, wheeled to and pressed: a window of its own, the popup
        // as it was
        let sent = bring(&mut rt, body[0] + body[2] / 2.0, body[1], body[1] + body[3] - 40.0, -150.0, |rt| text_under(rt, "popup0-dialog", "› what the model was sent")).expect("its line");
        assert_eq!(places(&rt), before, "the popup's wheel moves the popup only");
        press(&mut rt, sent);
        assert!(crate::sent_window::asked(&rt.state, "sent-turn-0-paper-research-N-1104.p21"), "its window opened");
        assert_eq!(rt.state.popups.len(), 1);
        assert_eq!(text_under(&rt, "popup0-dialog", "› what the model was sent"), Some(sent), "the line where it was");
        assert!(text_under(&rt, "popup0-dialog", "status 200").is_none(), "nothing of the request in the popup");
        // its body to its end: the head and its Close stay
        for _ in 0..20 {
            rt.wheel(body[0] + body[2] / 2.0, body[1] + body[3] / 2.0, -100_000.0);
            rt.frame(1.0 / 120.0);
        }
        settle(&mut rt);
        assert_eq!(close_of(&rt), Some(close), "the Close where it was");
        assert_eq!(keyed(&rt, "popup0-scroll"), Some(body), "the body where it was");
        assert!(text_under(&rt, "popup0-dialog", "Thought 0: weighing").is_none_or(|r| r[1] < first[1]), "the body scrolled");
        let line = text_under(&rt, "popup0-dialog", "› what the model was sent").unwrap();
        assert!(line[1] >= body[1] && line[1] + line[3] <= body[1] + body[3] + 0.5, "the line in view at the end: {line:?} in {body:?}");
        assert_eq!(places(&rt), before);
        escape(&mut rt);
        assert!(rt.state.popups.is_empty(), "Escape closes it");
        assert_eq!(places(&rt), before, "the thread as it was");
    }

    /// N-1396 p276, 4, items 7 and 8: in the agent's Work view a result's "… N more lines" and an edit's diff cut to
    /// its first lines open whole apart; the chat keeps its place.
    #[test]
    fn a_long_result_and_a_diff_open_whole_apart() {
        let (_held, mut s) = chat_app();
        s.sel = crate::Sel::Agent(AGENT.into());
        s.streams.which.insert((0, AGENT.into()), crate::streams::Which::Work);
        let c = &s.projects[0].convs[AGENT];
        let i = |o: u64| c.entries.iter().position(|e| e.offset == o).unwrap();
        let (read, edit) = (i(410), i(430));
        let keys = [c.keys[read].clone(), c.keys[edit].clone()];
        let open: Vec<String> = [read, edit].into_iter().flat_map(|j| crate::work::reveal(c, j, |_| false)).collect();
        s.expanded.extend(open);
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        for (key, more, whole, title) in [
            (&keys[0], "… 296 more lines", "line 300", "the whole result"),
            (&keys[1], "… 36 more lines", "+added 40", "the whole diff"),
        ] {
            let at = bring(&mut rt, 900.0, 200.0, 700.0, 150.0, |rt| text_under(rt, key, more)).unwrap_or_else(|| panic!("{more} under {key}"));
            assert!(at[1] >= 200.0 && at[1] <= 700.0, "{more} in view: {at:?}");
            let row = keyed(&rt, key).unwrap_or_else(|| panic!("{key} in the chat"));
            assert!(!outside(&rt, whole), "{whole}: only its first lines in the chat");
            press(&mut rt, at);
            assert_eq!(rt.state.popups.len(), 1);
            assert_eq!(rt.state.popups[0].what, What::Entry { pi: 0, agent: AGENT.into(), key: key.clone() });
            assert!(rt.state.popups[0].title.ends_with(title), "{}", rt.state.popups[0].title);
            assert!(text_under(&rt, "popup0-dialog", whole).is_some(), "{whole} in the popup");
            assert_eq!(keyed(&rt, key), Some(row), "the chat keeps its place");
            escape(&mut rt);
            assert!(rt.state.popups.is_empty());
        }
    }

    /// N-1396 p276, 4, item 6: in the agent's With agents stream "› the summary it left" opens the compaction's summary
    /// apart.
    #[test]
    fn a_compactions_summary_opens_apart() {
        let (_held, mut s) = chat_app();
        s.sel = crate::Sel::Agent(AGENT.into());
        s.streams.which.insert((0, AGENT.into()), crate::streams::Which::Agents);
        let key = { let c = &s.projects[0].convs[AGENT]; c.keys[c.entries.iter().position(|e| e.offset == 460).unwrap()].clone() };
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        assert!(!outside(&rt, "the popup piece is built"));
        let line = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("› the summary it left")).map(|e| e.rect)
            .unwrap_or_else(|| panic!("the compaction's line: {:?}", rt.elements().into_iter().filter_map(|e| e.text).filter(|t| t.len() < 60).collect::<Vec<_>>()));
        press(&mut rt, line);
        assert_eq!(rt.state.popups.len(), 1);
        assert_eq!(rt.state.popups[0].what, What::Summary { pi: 0, agent: AGENT.into(), key });
        assert!(text_under(&rt, "popup0-dialog", "the popup piece is built").is_some(), "the summary in the popup");
        assert_eq!(rt.elements().into_iter().find(|e| !e.exiting && e.text.as_deref() == Some("› the summary it left")).map(|e| e.rect), Some(line), "the stream keeps its place");
    }

    /// N-1396 p276's stills, on `chat_app`'s made-up thread, each by presses (rune's --script):
    /// `STILL_OUT=DIR yaaihh-tests popup::tests::popup_stills --ignored --exact`: the thread wheeled to its end, p21 and
    /// its "› the work behind it" in view (popup-thread.png); that pressed, then "› what the model was sent" in the
    /// popup pressed, the popup as it was, wheeled to its end (popup-work.png: the request is in a window of its own,
    /// sent_window.rs `sent_window_stills`); Escape pressed there, the thread as it was (popup-closed.png). And
    /// `halted_app`'s T-5: its halt cards (popup-halts.png), the first one's prompt apart (popup-halt-prompt.png).
    #[test]
    #[ignore]
    fn popup_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn thread() -> App {
            chat_app().1
        }
        let wheel = |t: f64, x: u32| (0..6).map(|i| format!(r#"{{"t": {:.2}, "type": "wheel", "x": {x}, "y": 500, "dy": -3000}}"#, t + 0.05 * i as f64)).collect::<Vec<_>>();
        let thread_end = wheel(0.2, 1100);
        let mut work = thread_end.clone();
        // the fold is at the turn's foot, under the popup's fold line: wheeled to before its press (a press beside the
        // panel would close it), and again after, to the request it opens
        work.push(r#"{"t": 0.9, "type": "click", "key": "work-N-1104.p21"}"#.to_string());
        work.extend(wheel(1.3, 700));
        work.push(r#"{"t": 1.9, "type": "click", "key": "sent-N-1104.p21"}"#.to_string());
        work.extend(wheel(2.3, 700));
        let mut closed = work.clone();
        closed.push(r#"{"t": 3.0, "type": "key", "name": "Escape"}"#.to_string());
        // T-5's page with its three halts (p282, 1): the cards, the prompt's first lines on the first; its whole prompt
        // apart
        fn halts() -> App {
            halted_app().1
        }
        fn prompt() -> App {
            let mut s = halts();
            super::open(&mut s, "task · T-5 · what docket-core would be sent", What::Halt { id: "T-5".into(), hn: 1, part: crate::slow::Part::Prompt });
            s
        }
        let mut shots = Vec::new();
        let scenes: [(&str, fn() -> App, Vec<String>, &str); 5] =
            [("popup-thread", thread, thread_end, "1"), ("popup-work", thread, work, "3"), ("popup-closed", thread, closed, "3.6"), ("popup-halts", halts, Vec::new(), "1"), ("popup-halt-prompt", prompt, Vec::new(), "1")];
        for (name, setup, events, at) in scenes {
            let script = out.join(format!("{name}.json"));
            std::fs::write(&script, format!("[{}]", events.join(", "))).unwrap();
            shots.push((name, setup, script.display().to_string(), at));
        }
        for (name, setup, script, at) in shots {
            let snap = out.join(format!("{name}.png")).display().to_string();
            let a: [&str; 8] = ["--snap", &snap, "--size", "1400x900", "--at", at, "--script", &script];
            let args: Vec<String> = a.map(String::from).to_vec();
            let spec = rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), setup, crate::view) };
            rune::App::new(spec).args(args).try_run().unwrap();
        }
    }

    /// A made-up ticket T-5: step 1 given to docket-core at a place whose text the page holds, and one reading of its
    /// answer; for the ticket page's own lines (ticket.rs) and a step's prompt (ran.rs).
    pub(crate) fn ticket_app() -> (std::sync::MutexGuard<'static, ()>, App, docket_machine::Loc) {
        let (held, mut s) = chat_app();
        let place: docket_machine::Loc = serde_json::from_value(json!({"file": "/made/up/session.jsonl", "offset": 10, "len": 90})).unwrap();
        s.projects[0].live.tickets.insert(5, serde_json::from_value(json!({"id": "T-5", "n": 5, "kind": "task", "opener": "owner", "ctx": {}, "form": {},
            "title": "a made-up task", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": at(0, 0),
            "given": [{"to": "docket-core", "delivery": "T-5.s1", "upto": 1, "at": at(1, 0), "where": place}], "posts": []})).unwrap());
        s.tickets.texts.insert(crate::texts::key(&place), Some(Ok("<channel source=\"coord\" ticket=\"T-5\" step=\"1\">\nDo step one.\n</channel>".into())));
        s.tickets.forming.insert(5, serde_json::from_value(json!([{"file": "tickets/forming/5.s1.1.json", "step": 1, "n": 1, "agent": "docket-core", "at": at(2, 0),
            "head": "Done", "calls": [{"call": "form-head", "model": "m", "prompt": "Read the agent's answer.\nThe agent's answer:\n<<<\nIt is done.\n>>>", "reply": {"head": "Done"}}]}])).map_err(|e| e.to_string()));
        s.tickets.shown = Some(5);
        (held, s, place)
    }

    /// T-5's history and its reading, as the ticket page draws them, with the popups over them.
    fn ticket_view(s: &App, cx: &mut rune::Cx<App>) -> rune::El {
        let k = Kit::neutral(false);
        let p = &s.projects[0];
        let v = &p.live.tickets[&5];
        let r = &s.tickets.forming[&5].as_ref().unwrap()[0];
        let mut layer = Vec::new();
        let page = rune::col().key("page").w_full().gap(8.0).child(crate::ticket::history(&s.tickets, p, v, &k)).child(crate::ticket::reading(p, v, r, &k));
        super::layer(s, cx, &k, &mut layer);
        rune_kit::root(page, layer)
    }

    /// N-1396 p276, 4, items 4 and 5: "What it read" opens what the agent was given apart, its channel's tag lines left
    /// out; a reading's line opens the reading apart, and in it what its call was sent opens in a window of its own
    /// (sent_window.rs; paper-research N-1446 p2112, 2).
    #[test]
    fn what_an_agent_read_and_a_reading_open_apart() {
        let (_held, s, place) = ticket_app();
        let mut rt = Runtime::new(s, ticket_view, (1400.0, 900.0), rune::Env::new(false, 1.0));
        settle(&mut rt);
        let line = keyed(&rt, "given-T-5.s1-docket-core").expect("the delivery's line");
        let button = text_under(&rt, "given-T-5.s1-docket-core", "What it read").expect("its button");
        press(&mut rt, button);
        assert_eq!(rt.state.popups.len(), 1);
        assert_eq!(rt.state.popups[0].what, What::Sent { place: place.clone() });
        assert_eq!(rt.state.popups[0].title, "task · T-5.s1 · what docket-core read");
        assert!(rt.state.tickets.unfolded.contains("T-5.s1"), "its text asked for as before");
        assert!(text_under(&rt, "popup0-dialog", "Do step one.").is_some());
        assert!(text_under(&rt, "popup0-dialog", "<channel").is_none(), "the channel's tag lines left out");
        assert!(!outside(&rt, "Do step one."));
        assert_eq!(keyed(&rt, "given-T-5.s1-docket-core"), Some(line));
        escape(&mut rt);
        let reading = text_under(&rt, "page", "docket-core's answer").expect("the reading's line");
        press(&mut rt, reading);
        assert_eq!(rt.state.popups[0].what, What::Reading { t: 5, file: "tickets/forming/5.s1.1.json".into() });
        assert!(text_under(&rt, "popup0-dialog", "It is done.").is_some() && text_under(&rt, "popup0-dialog", "What form-head answered").is_some());
        assert!(!outside(&rt, "It is done."));
        let line = text_under(&rt, "popup0-dialog", "› What form-head was sent").expect("its prompt's line");
        press(&mut rt, line);
        assert!(crate::sent_window::asked(&rt.state, "sent-call-5-tickets/forming/5.s1.1.json-0"), "its window opened");
        assert!(text_under(&rt, "popup0-dialog", "Read the agent's answer.").is_none(), "nothing of the prompt in the popup");
    }

    /// T-5 halted three times (slow mode): a step about to be given, its prompt 30 lines; a ticket about to run, its
    /// design 12 lines; a notice passing up an answer of 9 lines that could not be read; and its program from them on.
    fn halted_app() -> (std::sync::MutexGuard<'static, ()>, App) {
        use docket_machine::ViewHalt;
        let (held, mut s, _) = ticket_app();
        let lines = |what: &str, n: u32| (1..=n).map(|i| format!("{what} line {i}.")).collect::<Vec<_>>().join("\n");
        let v = s.projects[0].live.tickets.get_mut(&5).unwrap();
        v.halts = vec![
            ViewHalt::Give { n: 1, at: at(3, 0), step: 2, call: "piece".into(), worker: "docket-core".into(), prompt: lines("Prompt", 30), edited: false },
            ViewHalt::Open { n: 2, at: at(3, 1), ticket: "T-5".into(), kind: "task".into(), form: json!({"design": lines("Design", 12)}), fields: vec![("design".into(), docket_machine::Type::Text)] },
            ViewHalt::Notice { n: 3, at: at(3, 2), to: "docket-core".into(), delivery: "T-5.d3".into(), notice: "words".into(), detail: json!({"passed": [{"by": "docket-core", "text": lines("Answer", 9)}]}) },
        ];
        v.continuation = Some("review <- step check(x)\nreturn Done(review)".into());
        (held, s)
    }

    /// T-5's halts as their cards in its thread, with the popups over them.
    fn halts_view(s: &App, cx: &mut rune::Cx<App>) -> rune::El {
        let k = Kit::neutral(false);
        let p = &s.projects[0];
        let v = &p.live.tickets[&5];
        let mut layer = Vec::new();
        super::layer(s, cx, &k, &mut layer);
        rune_kit::root(rune::col().key("page").w_full().gap(12.0).children(v.halts.iter().map(|h| crate::slow::card(&s.tickets, p, v, v, h, false, &k))), layer)
    }

    /// N-1396 p282, 1: a halt's card keeps its prompt's first lines, the whole of it apart; the design, the program from
    /// the halt on and the answer that could not be read each open apart; each closes back to the card as it was, its
    /// approve on it.
    #[test]
    fn a_halts_big_parts_open_apart_from_its_card() {
        let (_held, s) = halted_app();
        let mut rt = Runtime::new(s, halts_view, (1400.0, 1600.0), rune::Env::new(false, 1.0));
        settle(&mut rt);
        assert!(text_under(&rt, "page", "Prompt line 4.").is_some() && text_under(&rt, "page", "Prompt line 5.").is_none(), "the prompt's first lines on the card");
        assert!(text_under(&rt, "page", "Design line 1.").is_some() && text_under(&rt, "page", "Design line 2.").is_none(), "the design's first line");
        assert!(text_under(&rt, "page", "Answer line 1.").is_none() && text_under(&rt, "page", "return Done(review)").is_none(), "nothing else big on the cards");
        let approve = text_under(&rt, "page", "approve").expect("approve on the card");
        for (line, what, inside) in [
            ("… 26 more lines", "what docket-core would be sent", "Prompt line 30."),
            ("› the whole design", "the whole design", "Design line 12."),
            ("› the program from here", "the program from here", "return Done(review)"),
            ("› docket-core's answer that could not be read", "docket-core's answer that could not be read", "Answer line 9."),
        ] {
            let at = text_under(&rt, "page", line).unwrap_or_else(|| panic!("no {line}"));
            press(&mut rt, at);
            assert_eq!(rt.state.popups.len(), 1, "{line}");
            let title = &rt.state.popups[0].title;
            assert!(title.ends_with(&format!(" · T-5 · {what}")), "{title}");
            assert!(text_under(&rt, "popup0-dialog", inside).is_some(), "{inside} apart");
            assert!(!outside(&rt, inside), "{inside} not on the card");
            escape(&mut rt);
            assert!(rt.state.popups.is_empty());
            assert_eq!(text_under(&rt, "page", line), Some(at), "the card as it was");
        }
        assert_eq!(text_under(&rt, "page", "approve"), Some(approve));
    }

    /// consult-3 N-1477 p64, proto-window p65: a popup's passage goes to the thread the page under it shows, as the place
    /// kept reads it (history.rs `thread_shown`), not the one left on the Threads page. He had N-1104 open on the
    /// Threads page, reads N-1200 beside paper-research's With you, and opens the prompt of T-5's halt (a task: no
    /// thread of its own) apart: his comment on it goes to N-1200. On the Threads page it goes to N-1104; on a page
    /// that shows no thread (a lens's record) T-5's halt offers no Comment, and a halt of a thread goes to that thread.
    #[test]
    fn a_popups_comment_goes_to_the_thread_the_page_under_it_shows() {
        let (_held, mut s) = halted_app();
        let mut b = s.projects[0].live.tickets[&1104].clone();
        (b.id, b.n) = ("N-1200".to_string(), 1200);
        s.projects[0].live.tickets.insert(1200, b);
        s.tickets.shown = Some(1104);
        s.sel = crate::Sel::Agent(AGENT.into());
        s.withyou.inside.insert((0, AGENT.to_string()), 1200);
        let halt = What::Halt { id: "T-5".into(), hn: 1, part: crate::slow::Part::Prompt };
        super::open(&mut s, "task · T-5 · what docket-core would be sent", halt.clone());
        let mut rt = Runtime::new(s, halts_view, (1400.0, 1600.0), rune::Env::new(false, 1.0));
        settle(&mut rt);
        let (key, words) = (super::text_key(0), "Prompt line 3.");
        let t = rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key.as_str())).and_then(|e| e.text).expect("the halt's prompt drawn apart");
        let a = t.find(words).unwrap();
        rt.update(move |_, tx| tx.select_text(key, a..a + words.len()));
        settle(&mut rt);
        let sel = rt.selection().expect("selected");
        assert_eq!(sel.text(), words);
        rt.update(move |s, _| super::comment(s, 0, &sel));
        assert_eq!(rt.state.notes.picked.as_ref().map(|p| p.target.as_str()), Some("thread:N-1200"), "to the thread beside With you, not N-1104 left on the Threads page");
        rt.update(|s, _| s.sel = crate::Sel::Tickets);
        assert_eq!(super::goes_to(&rt.state, &halt), Some((0, "thread:N-1104".to_string())), "on the Threads page, its thread");
        let record = crate::lens::Lens { filter: crate::lens::Filter::Threads, depth: crate::lens::Depth::Record, at: Some("N-1104.p3".into()) };
        rt.update(move |s, _| s.sel = crate::Sel::Lens(record));
        assert_eq!(super::goes_to(&rt.state, &halt), None, "a page that shows no thread: a task's halt offers no Comment");
        let theirs = What::Halt { id: "N-1200".into(), hn: 1, part: crate::slow::Part::Prompt };
        assert_eq!(super::goes_to(&rt.state, &theirs), Some((0, "thread:N-1200".to_string())), "a thread's halt: its own");
    }

    /// consult-3 N-1396 p304, 1: a press in a popup that goes somewhere closes the popups, and the page shows where it
    /// went: a post of the thread under it (find.rs `open_at`, which makes no new place), an agent's name (a new place,
    /// history.rs).
    #[test]
    fn a_press_in_a_popup_that_goes_somewhere_closes_it_and_the_page_shows_where() {
        let (_held, s) = chat_app();
        let mut rt = crate::view_cost::window(s);
        // this module's part of the window's poll, as history.rs's tests run it (the rest reads pipes this project lacks)
        fn poll_places(s: &mut App, _: f64, _: &mut rune::Tx) -> bool {
            crate::history::poll(s)
        }
        rt.poll = Some(poll_places);
        settle(&mut rt);
        let work = |rt: &mut Runtime<App>| {
            let at = rt.state.projects[0].live.tickets[&1104].posts[20].at.to_string();
            super::open(&mut rt.state, "the work behind p21", What::Work { pi: 0, agent: AGENT.into(), post: "N-1104.p21".into(), at });
            // opened from here, not by a press: the view is run again as after one
            rt.invalidate();
            settle(rt);
        };
        // a link pressed as a pointer does: over it, down, up
        let follow = |rt: &mut Runtime<App>, words: &str| {
            // the link drawn as an element of its own, as threads.rs's question link is
            let at = rt.elements().into_iter().find(|e| !e.exiting && e.path.split('/').any(|k| k == "popup0-dialog") && e.text.as_deref() == Some(words));
            let r = at.map(|e| e.rect).unwrap_or_else(|| {
                let t: Vec<(String, [f32; 4])> = rt.elements().into_iter().filter(|e| !e.exiting && e.path.contains("popup0")).filter_map(|e| Some((e.text?, e.rect))).collect();
                panic!("{words} linked in the popup: {t:?}")
            });
            let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
            rt.pointer_move(x, y);
            rt.frame(0.016);
            rt.pointer_down(x, y);
            rt.frame(0.016);
            rt.pointer_up(x, y);
            settle(rt);
        };
        work(&mut rt);
        follow(&mut rt, "N-1104.p3");
        assert!(rt.state.popups.is_empty(), "closed by the press");
        assert_eq!((rt.state.sel.clone(), rt.state.tickets.shown), (crate::Sel::Tickets, Some(1104)), "the thread under it, at p3");
        work(&mut rt);
        follow(&mut rt, "@paper-research");
        assert!(rt.state.popups.is_empty(), "closed: the page made a place");
        assert_eq!(rt.state.sel, crate::Sel::Agent(AGENT.into()), "paper-research's page");
    }

    /// A step's prompt as sent, made up: 40 lines, taller than the popup's first screen.
    fn long_prompt() -> String {
        (1..=40).map(|i| format!("Line {i} of the prompt as sent.")).collect::<Vec<_>>().join("\n")
    }

    /// The made-up project of [`chat_app`], N-1104 shown, with what a step was sent (a 40-line prompt) open apart over it
    /// in the whole window (lib.rs's layers), the window at SIZE; its title.
    fn prompt_open(size: (f32, f32)) -> (std::sync::MutexGuard<'static, ()>, Runtime<App>, &'static str) {
        let (held, mut s) = chat_app();
        let place: docket_machine::Loc = serde_json::from_value(json!({"file": "/made/up/session.jsonl", "offset": 10, "len": 90})).unwrap();
        s.tickets.texts.insert(crate::texts::key(&place), Some(Ok(long_prompt())));
        let title = "task · T-5.s1 · what docket-core read";
        super::open(&mut s, title, What::Sent { place });
        s.notice = None;
        let mut rt = crate::ask::anchor_tests::window(s, size);
        settle(&mut rt);
        (held, rt, title)
    }

    /// Bytes of the prompt from line A's start to line B's end, as it is drawn in popup 0.
    fn lines(rt: &Runtime<App>, a: usize, b: usize) -> std::ops::Range<usize> {
        let t = rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some("popup0-text")).and_then(|e| e.text).expect("the prompt drawn plain, keyed");
        let start = t.find(&format!("Line {a} of")).unwrap();
        let end = t.find(&format!("Line {b} of")).map(|i| i + t[i..].find('\n').unwrap_or(t.len() - i)).unwrap();
        start..end
    }

    /// The lit passage being commented on (its line boxes, as rune reports a highlight), and the comment box: (the
    /// passage's top, its last line's bottom, its left), the box's rect and whether it takes input.
    fn passage_and_box(rt: &Runtime<App>) -> (Option<[f32; 3]>, Option<([f32; 4], bool)>) {
        let marks: Vec<[f32; 4]> = rt.elements().into_iter().filter(|e| !e.exiting && e.role == "mark" && e.key.as_deref() == Some("cm-new")).map(|e| e.rect).collect();
        let lit = (!marks.is_empty()).then(|| {
            let top = marks.iter().map(|r| r[1]).fold(f32::MAX, f32::min);
            let bottom = marks.iter().map(|r| r[1] + r[3]).fold(f32::MIN, f32::max);
            [top, bottom, marks.iter().map(|r| r[0]).fold(f32::MAX, f32::min)]
        });
        let cm = rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some("cm-box")).map(|e| (e.rect, !e.inert));
        (lit, cm)
    }

    /// I-1476 (the owner, N-1104 p706: the comment box has to appear where he highlighted the text, not at the bottom
    /// of the page; paper-research N-1477 p9, p18: covering none of his text, each surface narrow and wide): lines in
    /// the middle of a popup's prompt, selected and commented on, are lit where they stand, and the box opens right
    /// under the last of them, the lines after them moved down under it. The popup's text fills it, so neither size
    /// has a margin beside the lines: the box is in their room both times, inside the popup, over it and taking his
    /// keys there; no frame as it opens, nor as his five lines make it taller, has text under it. It goes to the
    /// thread under the popup, named by its title. A post arriving on that thread moves nothing. Escape closes the box
    /// with his words kept and leaves the popup open; a press on the lit lines opens it again where it was.
    #[test]
    fn a_comment_in_a_popup_opens_under_the_lines_selected_over_the_popup() {
        use crate::ask::anchor_tests::{NARROW, WIDE, drawn, frames_shown, in_room, lit, type_lines, under_box};
        const BODY: &str = "popup0-body";
        const AFTER: &str = "Line 20 of";
        for size in [NARROW, WIDE] {
            let (_held, mut rt, title) = prompt_open(size);
            let at = lines(&rt, 18, 19);
            let range = at.clone();
            rt.update(move |_, tx| tx.select_text("popup0-text", range));
            settle(&mut rt);
            let sel = rt.selection().expect("lines 18 and 19 selected");
            assert!(sel.text().starts_with("Line 18 of") && sel.text().ends_with("as sent."), "{}", sel.text());
            rt.update(move |s, _| super::comment(s, 0, &sel));
            frames_shown(&mut rt, &format!("{size:?}, the popup's box opened"));
            settle(&mut rt);
            let why = |rt: &Runtime<App>| format!("{size:?}: box {:?}, room {:?}, lit {:?}, popup {:?}, under {:?}", drawn(rt, "cm-box"), drawn(rt, crate::ask::ROOM), lit(rt), drawn(rt, "popup0-scroll"), under_box(rt));
            let p = rt.state.notes.picked.clone().expect("a comment being written");
            assert_eq!((p.target.as_str(), p.label.as_str(), p.texts.clone()), ("thread:N-1104", title, vec![("popup0-text".to_string(), at.clone())]));
            assert!(super::is_source(&p.source) && p.at.is_empty(), "{p:?}");
            in_room(&rt, BODY, AFTER, &why(&rt));
            // what `under_box` reads over a popup: its own lines, the popup's panel on their path
            assert!(rt.elements().iter().any(|e| e.path.split('/').any(|k| k == "popup0-dialog") && e.text.as_deref().is_some_and(|t| t.contains(AFTER))), "the popup's lines in its panel");
            let (lit, cm) = passage_and_box(&rt);
            let [top, bottom, _] = lit.expect("lines 18 and 19 lit in the popup");
            let (r, live) = cm.expect("its box drawn");
            assert!(bottom - top > 20.0, "two lines lit: {top} to {bottom}");
            let pop = drawn(&rt, "popup0-scroll").unwrap();
            assert!(r[0] >= pop[0] && r[0] + r[2] <= pop[0] + pop[2], "inside the popup: {}", why(&rt));
            assert!(live, "over the popup, inside its modal: it takes input");
            crate::view_cost::type_in(&mut rt, "why 19");
            assert_eq!(crate::ask::writing(&rt.state).map(|w| w.text.clone()).as_deref(), Some("why 19"), "his keys go into it");
            // his five lines, one more each time: the room grows with the box, line 20 under it, no frame covering any
            type_lines(&mut rt, BODY, AFTER);
            in_room(&rt, BODY, AFTER, &why(&rt));
            let (r, words) = (drawn(&rt, "cm-box").unwrap(), crate::ask::writing(&rt.state).map(|w| w.text.clone()));
            // a post arriving on the thread under the popup moves nothing
            rt.update(|s, _| {
                let v = s.projects[0].live.tickets.get_mut(&1104).unwrap();
                let mut q = v.posts.last().unwrap().clone();
                q.n += 1;
                v.posts.push(q);
            });
            rt.invalidate();
            settle(&mut rt);
            assert_eq!(passage_and_box(&rt).1.map(|(r, _)| r), Some(r), "where it was, open");
            escape(&mut rt);
            assert_eq!(rt.state.popups.len(), 1, "Escape closed the box, not the popup");
            assert!(passage_and_box(&rt).1.is_none() && rt.state.notes.shut, "closed, his words kept");
            assert!(drawn(&rt, crate::ask::ROOM).is_none(), "its room closed with it: {}", why(&rt));
            let (lit, _) = passage_and_box(&rt);
            let [x, y] = lit.map(|[top, _, left]| [left + 4.0, top + 6.0]).expect("still lit");
            press(&mut rt, [x, y, 0.0, 0.0]);
            assert_eq!(passage_and_box(&rt).1.map(|(r, _)| r), Some(r), "open again, where it was");
            assert_eq!(crate::ask::writing(&rt.state).map(|w| w.text.clone()), words);
            in_room(&rt, BODY, AFTER, &why(&rt));
        }
    }

    /// The comment box in a popup, made up: lines 18 and 19 of a step's prompt lit, the box right under them with his
    /// words in it, over the popup, line 20 on moved down under it (I-1476); narrow (1400) and wide (2200), the same.
    /// `STILL_OUT=DIR yaaihh-tests popup::tests::comment_stills --ignored --exact`.
    #[test]
    #[ignore]
    fn comment_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn popup() -> App {
            let (_held, mut s) = chat_app();
            let place: docket_machine::Loc = serde_json::from_value(json!({"file": "/made/up/session.jsonl", "offset": 10, "len": 90})).unwrap();
            let t = long_prompt();
            s.tickets.texts.insert(crate::texts::key(&place), Some(Ok(t.clone())));
            super::open(&mut s, "task · T-5.s1 · what docket-core read", What::Sent { place });
            let (a, b) = (t.find("Line 18 of").unwrap(), t.find("Line 20 of").unwrap() - 1);
            let (source, label) = (super::source(&s.popups[0].what), s.popups[0].title.clone());
            let picked = crate::ask::Picked { target: "thread:N-1104".into(), source, label, quote: t[a..b].to_string(), at: Vec::new(), pi: 0, anchor: None, block: None, pane: None, texts: vec![(super::text_key(0), a..b)] };
            crate::ask::write(&mut s, picked, "why do these two lines come before the answer?\nand does line 20 need them?");
            s.notice = None;
            s
        }
        let script = out.join("comment-popup.json");
        std::fs::write(&script, "[]").unwrap();
        let script = script.display().to_string();
        for (w, size) in [(1400.0, "1400x900"), (2200.0, "2200x900")] {
            let snap = out.join(format!("comment-popup-{size}.png")).display().to_string();
            let a: [&str; 8] = ["--snap", &snap, "--size", size, "--at", "1", "--script", &script];
            let spec = rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (w, 900.0), popup, crate::view) };
            rune::App::new(spec).args(a.map(String::from).to_vec()).try_run().unwrap();
        }
    }

    /// A comment box of the page stays under a popup opened over it: it takes no input there, and stands over it again
    /// once the popup is closed (lib.rs's layers, ask.rs `apart`).
    #[test]
    fn a_comment_box_of_the_page_stays_under_a_popup() {
        let (_held, mut s) = chat_app();
        let quote = "And its sixth.";
        // the thread's last post, his p22, in words as chat_app makes them: its sixth paragraph after the first five
        let from = "Post 22, its first paragraph.\n\nIts second.\n\nIts third.\n\nIts fourth.\n\nIts fifth.\n\n".len();
        crate::ask::write(&mut s, crate::ask::Picked { target: "thread:N-1104".into(), source: "N-1104.p22".into(), label: "his post".into(), quote: quote.into(),
            at: vec![from..from + quote.len()], pi: 0, anchor: None, block: None, pane: None, texts: Vec::new() }, "");
        s.notice = None;
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        // the thread at its end, where p22 stands
        for _ in 0..4 {
            rt.wheel(700.0, 450.0, -3000.0);
            settle(&mut rt);
        }
        let (lit, cm) = passage_and_box(&rt);
        assert!(lit.is_some(), "p22's sixth paragraph lit");
        assert!(cm.is_some_and(|(_, live)| live), "the page's box, open: {cm:?}");
        rt.update(|s, _| super::open(s, "paper-research · N-1104.p21 · the work behind it", What::Work { pi: 0, agent: AGENT.into(), post: "N-1104.p21".into(), at: at(21, 0) }));
        rt.invalidate();
        settle(&mut rt);
        let (_, cm) = passage_and_box(&rt);
        assert!(cm.is_some_and(|(_, live)| !live), "under the popup: {cm:?}");
        escape(&mut rt);
        assert!(rt.state.popups.is_empty());
        assert!(passage_and_box(&rt).1.is_some_and(|(_, live)| live), "over the page again");
    }
}
