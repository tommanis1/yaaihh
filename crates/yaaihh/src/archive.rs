//! Everything up to now behind one Archive (the owner, N-1104 p992: "archive all of this . and just move to a clean
//! thing . so yeah just keep all of this then but behind a thingy for me in theu i agents of c still need ot be able ot
//! read it"; paper-research N-1446 p2344, p2350; docket-edge8's shape, p2349). One press of his, "Archive everything up
//! to now…" at the Threads list's foot, opens a card ([`card`]); its Archive puts a moment in his window's saved state
//! (state.rs `Saved::archive`, so History's "Your window" goes back over it too) and opens his new chat. From then on:
//! - the Threads list and the sidebar's Structure hold the threads opened after the moment, each with everything under
//!   it; a thread opened before it is in the Archive with all its work, a new step under it too ([`hides`]);
//! - Waits on you keeps everything that waits on him, as many as before, each of the Archive's marked "from the
//!   Archive", and his open goals of the Archive stand under it (outline.rs): nothing he is asked or waits on goes out
//!   of sight (p2350, 2);
//! - the Map holds his posts after the moment (cloud.rs `since`), the All agents table the agents running and those
//!   stopped since ([`agent_hidden`]); Find finds everything, a hit of the Archive saying so;
//! - his pins of older threads wait in the Archive, and his new chat is pinned in the place of the old one's.
//!
//! His one group chat is found by its label ([`CHAT_LABEL`], N-1104's title), never by an id: the press opens a new
//! thread with that label, for the agent the old one is for (its form's `to`, else whoever fills lead), its first
//! message the card's line, so his words in the pinned chat's box go there. The sidebar's Archive row shows every page
//! as it was up to the moment, under a bar ([`bar`]): Back to now, and Bring it all back, which takes the moment away,
//! every list as before and the new chat an ordinary thread.
//!
//! Nothing is asked of coordd but the new chat's message: no ticket is archived (tickets.eflint's `archived` holds
//! work, p2344), and agents read and work on all of it as before.

use crate::{App, Proj, Sel};
use coord_types::{AgentView, Request, TicketView};
use rune::{El, TextEdit, col, row, text};
use rune_kit::{Button, ButtonVariant, Dialog, Kit, TextInput};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// The label of his one group chat: N-1104's title on his record, which the press gives his new chat. The window finds
/// his chat by it ([`chat_of`]).
pub const CHAT_LABEL: &str = "Your one group chat";

/// The place of the new chat's message on its way (outgoing.rs), drawn above the Threads list until it is taken.
const PLACE: &str = "archive";

/// The moment he put everything away, kept in his window's saved state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mark {
    /// When, as coordd writes a time (`at_secs`).
    pub at: String,
    /// The project his new chat was opened in, by id.
    pub project: String,
    /// The new chat's ticket, once the snapshot holds it ([`step`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
    /// The Archive shown: every page as it was up to the moment.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub showing: bool,
}

/// The card while it is open: the new chat's first message, his to write.
#[derive(Default)]
pub struct Card {
    pub open: bool,
    pub line: TextEdit,
}

/// The tickets put away, per project: built again when its tickets or the moment change.
#[derive(Default)]
pub struct Cache {
    by: HashMap<String, (Key, HashSet<u64>)>,
}

type Key = (u64, usize, Option<u64>, String);

/// The moment, when he archived.
pub fn on(s: &App) -> Option<&Mark> {
    s.archive.as_ref()
}

/// Whether A, a time as coordd writes it, is before B.
fn before(a: &str, b: &str) -> bool {
    match (docket_machine::machine::secs(a), docket_machine::machine::secs(b)) {
        (Some(a), Some(b)) => a < b,
        _ => a < b,
    }
}

/// V's place up in the structure, as outline.rs `parent` reads it, else the ticket it was opened under.
fn up<'a>(by_id: &HashMap<&str, &'a TicketView>, v: &TicketView) -> Option<&'a TicketView> {
    if let Some(o) = &v.outline {
        return by_id.get(o.parent.as_deref()?).copied();
    }
    by_id.get(v.parent.as_deref().or(v.under.as_deref())?).copied()
}

/// The tickets of P whose top thread was opened before AT: each top with everything under it.
fn put_away(p: &Proj, at: &str) -> HashSet<u64> {
    let by_id: HashMap<&str, &TicketView> = p.live.tickets.values().map(|v| (v.id.as_str(), v)).collect();
    let mut out = HashSet::new();
    for v in p.live.tickets.values() {
        let mut top = v;
        for _ in 0..64 {
            match up(&by_id, top) {
                Some(u) if u.n != top.n => top = u,
                _ => break,
            }
        }
        if before(&top.opened_at, at) {
            out.insert(v.n);
        }
    }
    out
}

/// Whether V of P is put away: its top thread was opened before the moment.
fn old(s: &App, p: &Proj, v: &TicketView) -> bool {
    let Some(m) = on(s) else { return false };
    let key: Key = (p.live.tickets_rev, p.live.tickets.len(), p.live.tickets.keys().next_back().copied(), m.at.clone());
    let mut c = s.archive_cache.borrow_mut();
    let fresh = c.by.get(&p.id).is_some_and(|(k, _)| *k == key);
    if !fresh {
        c.by.insert(p.id.clone(), (key, put_away(p, &m.at)));
    }
    c.by.get(&p.id).is_some_and(|(_, set)| set.contains(&v.n))
}

/// Whether the lists leave V of P out: now, what is in the Archive; in the Archive, what came after.
pub fn hides(s: &App, p: &Proj, v: &TicketView) -> bool {
    on(s).is_some_and(|m| old(s, p, v) != m.showing)
}

/// Whether V of P is in the Archive while he looks at now: a row kept in sight (what waits on him) says so.
pub fn from_archive(s: &App, p: &Proj, v: &TicketView) -> bool {
    on(s).is_some_and(|m| !m.showing) && old(s, p, v)
}

/// Whether wait W of P is of the Archive while he looks at now: on a thread in it, and raised before the moment (the
/// question's post, else its ticket opened). An ask raised since is new, on an old thread too (p2350, 1: what he is
/// asked after his press is no Archive's).
pub fn wait_from_archive(s: &App, p: &Proj, w: &crate::waits::Wait) -> bool {
    let Some(m) = on(s).filter(|m| !m.showing) else { return false };
    let raised = match w.kind {
        crate::waits::Kind::Question { post } => w.t.posts.iter().find(|q| q.n == post).map(|q| q.at.as_str()),
        _ => None,
    };
    before(raised.unwrap_or(w.t.opened_at.as_str()), &m.at) && old(s, p, w.t)
}

/// Whether the All agents table leaves A out: now, an agent that stopped before the moment; in the Archive, one
/// registered after it.
pub fn agent_hidden(s: &App, a: &AgentView) -> bool {
    match on(s) {
        None => false,
        Some(m) if m.showing => !a.created.is_empty() && !before(&a.created, &m.at),
        Some(m) => a.state == "stopped" && before(&a.since, &m.at),
    }
}

/// Whether thread pin PIN waits in the Archive (or, there, came after it). His old chat keeps its pin until his new
/// one is found ([`step`]): were the new chat's message refused or slow, his words still have their place.
pub fn pin_hidden(s: &App, pin: &crate::sidebar::Pin) -> bool {
    let waiting = on(s).is_some_and(|m| m.chat.is_none() && !m.showing);
    pin.thread && s.projects.iter().find(|q| q.id == pin.project).is_some_and(|q| q.live.tickets.values().find(|t| t.id == pin.id).is_some_and(|t| hides(s, q, t) && !(waiting && labelled(t))))
}

/// His open goals in the Archive: the items his posts opened that are still open, each in the Archive.
pub fn goals_from_archive<'a>(s: &App, p: &'a Proj) -> Vec<&'a TicketView> {
    p.live.tickets.values().rev().filter(|v| v.kind == "item" && v.closed_at.is_none() && v.opener.as_deref() == Some("owner") && from_archive(s, p, v)).collect()
}

/// His one group chat in P: the newest open thread at the top with [`CHAT_LABEL`].
pub fn chat_of(p: &Proj) -> Option<&TicketView> {
    p.live.tickets.values().rev().find(|v| v.kind == "thread" && v.closed_at.is_none() && v.parent.is_none() && labelled(v))
}

/// Whether V bears his chat's label, as its title or its outline's label.
fn labelled(v: &TicketView) -> bool {
    v.title == CHAT_LABEL || v.outline.as_ref().is_some_and(|o| o.label == CHAT_LABEL)
}

/// The agent thread V is for: its form's `to`.
fn for_whom(v: &TicketView) -> Option<String> {
    match &v.form["to"] {
        serde_json::Value::String(a) if !a.is_empty() => Some(a.clone()),
        serde_json::Value::Array(xs) => xs.first().and_then(|x| x.as_str()).map(String::from),
        _ => None,
    }
}

/// The Map's moment for the Archive: (seconds, whether the Archive is shown).
pub fn map_moment(s: &App) -> Option<(i64, bool)> {
    on(s).and_then(|m| Some((docket_machine::machine::secs(&m.at)?, m.showing)))
}

/// Whether "Archive everything up to now…" is offered at all: nothing archived up to a moment yet, the project up.
fn offered(s: &App) -> bool {
    on(s).is_none() && s.proj().is_some_and(|p| p.down.is_none())
}

/// Whether the list has something to archive beyond the open thread (paper-research N-1446 p3296): a closed thread, or
/// more than one, among those not archived.
fn worth_it(s: &App) -> bool {
    let Some(p) = s.proj() else { return false };
    let mut them = p.live.tickets.values().filter(|v| !v.archived && v.parent.is_none() && v.under.is_none());
    let (first, second) = (them.next(), them.next());
    second.is_some() || first.is_some_and(|v| v.closed_at.is_some())
}

/// The foot of the Threads list, under ARCHIVED: "Archive everything up to now…", while nothing is archived and the
/// list has something to archive beyond the open thread; else it is in the list's ⋯ ([`in_menu`]).
pub fn foot(s: &App, k: &Kit) -> Option<El> {
    if !offered(s) || !worth_it(s) {
        return None;
    }
    Some(row().pt(10.0).child(Button::new("Archive everything up to now…").variant(ButtonVariant::Ghost).on_press(open_card).build(k).key("archive-all")))
}

/// Whether the Archive's press is in the Threads list's ⋯: offered, with nothing beyond the open thread to archive.
pub(crate) fn in_menu(s: &App) -> bool {
    offered(s) && !worth_it(s)
}

pub(crate) fn open_card(s: &mut App) {
    s.archive_card = Card { open: true, line: TextEdit::new("A clean start.".to_string()) };
}

/// What is still open toward him in P (p2350, 2): the words of what waits on him, and his open goals; the Archive takes
/// none of them out of sight.
fn still_open(s: &App, p: &Proj) -> Vec<String> {
    let mut out: Vec<String> = crate::waits::on_owner(p).iter().map(|w| crate::outline::need_words(s, p, w)).collect();
    out.extend(p.live.tickets.values().rev().filter(|v| v.kind == "item" && v.closed_at.is_none() && v.opener.as_deref() == Some("owner")).map(|v| format!("your goal: {}", crate::refs::cut(&crate::outline::label(p, v), 80))));
    out
}

/// How many rows of what is still open the card shows, the rest said as a count.
const OPEN_SHOWN: usize = 5;

/// The card of "Archive everything up to now", into LAYER while it is open.
pub fn card(s: &App, k: &Kit, layer: &mut Vec<El>) {
    if !s.archive_card.open {
        return;
    }
    let Some(p) = s.proj() else { return };
    let line = |t: String| text(t).font(k.ty.small).color(k.c.text_base).wrap();
    let quiet = |t: String| text(t).font(k.ty.small).color(k.c.text_muted).wrap();
    let chat = chat_of(p);
    let whom = chat.and_then(for_whom);
    let open = still_open(s, p);
    let mut body = col().w_full().px(16.0).pb(16.0).gap(10.0);
    body = body.child(line(match chat {
        Some(c) => format!("Archives its threads, chat {}, Map and stopped agents.", c.id),
        None => "Archives its threads, Map and stopped agents.".into(),
    }));
    if !open.is_empty() {
        let mut o = col().w_full().gap(2.0).key("archive-open").child(text(crate::refs::count("Still open for you", open.len())).font(k.ty.small_medium).color(k.c.text_base));
        for w in open.iter().take(OPEN_SHOWN) {
            o = o.child(text(format!("· {w}")).font(k.ty.small).color(k.c.text_muted).truncate());
        }
        if open.len() > OPEN_SHOWN {
            o = o.child(quiet(format!("and {} more", open.len() - OPEN_SHOWN)));
        }
        body = body.child(o).child(quiet("They stay in Waits on you.".into()));
    }
    body = body.child(line(match &whom {
        Some(a) => format!("New chat with {a}, starting with:"),
        None => "New chat, starting with:".into(),
    }));
    body = body.child(TextInput::new(&s.archive_card.line).label("First message").width(None).on_edit(|s: &mut App| &mut s.archive_card.line).build(k));
    let empty = s.archive_card.line.text.trim().is_empty();
    let cancel = Button::new("Cancel").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.archive_card = Card::default()).build(k).key("archive-cancel");
    let go = Button::new("Archive").variant(ButtonVariant::Contrast).disabled(empty).on_press(press).build(k).key("archive-go");
    body = body.child(row().w_full().justify_end().gap(8.0).child(cancel).child(go));
    Dialog::new("archive-card", "Archive everything up to now").open(true).fit().width(480.0).child(body).on_close(|s: &mut App| s.archive_card = Card::default()).build(k, layer);
}

/// His Archive on the card: the moment kept, the new chat's message sent (outgoing.rs, above the Threads list until it
/// is taken), the Threads page shown.
pub fn press(s: &mut App) {
    let pi = s.cur;
    let Some(p) = s.projects.get(pi) else { return };
    let words = s.archive_card.line.text.trim().to_string();
    if words.is_empty() {
        return;
    }
    let to = chat_of(p).and_then(for_whom);
    let project = p.id.clone();
    let at = docket_machine::machine::at_secs(chrono::Utc::now().timestamp());
    let req = Request::Say { to, names: Vec::new(), text: words.clone(), follows: None, wait: false, under: None, label: Some(CHAT_LABEL.into()), thread: true, typed: None, split_from: None, comments: Vec::new() };
    let (id, refused) = crate::outgoing::press(s, (pi, PLACE.into()), &req, &words, crate::outgoing::Then::Box, None);
    if let Some(why) = refused {
        crate::outgoing::refused(s, id, why.trim_start_matches("not sent: "));
    }
    s.archive = Some(Mark { at, project, chat: None, showing: false });
    s.archive_card = Card::default();
    s.sel = Sel::Tickets;
    s.tickets.shown = None;
    crate::state::write_now(s);
}

/// The new chat's message on its way, above the Threads list until its thread is in the snapshot.
pub fn on_its_way(s: &App, pi: usize, k: &Kit) -> Vec<El> {
    crate::outgoing::bubbles(s, pi, PLACE, k)
}

/// Each poll: the new chat found once its thread is in the snapshot (the first thread with [`CHAT_LABEL`] opened at or
/// after the moment), kept in the moment and pinned where the old chat's pin stands (that one waits in the Archive).
/// Whether anything changed.
pub fn step(s: &mut App) -> bool {
    let Some(m) = s.archive.as_ref().filter(|m| m.chat.is_none()) else { return false };
    let Some(p) = s.projects.iter().find(|p| p.id == m.project) else { return false };
    let Some(chat) = p.live.tickets.values().find(|v| v.kind == "thread" && v.parent.is_none() && !before(&v.opened_at, &m.at) && (v.title == CHAT_LABEL || v.outline.as_ref().is_some_and(|o| o.label == CHAT_LABEL))) else { return false };
    let (id, project) = (chat.id.clone(), p.id.clone());
    let old = s.pins.list.iter().position(|pin| pin.project == project && pin.thread && pin.id != id && pin.agent == CHAT_LABEL);
    s.pins.put(crate::sidebar::Pin { project, id: id.clone(), agent: CHAT_LABEL.into(), thread: true, lens: None }, old);
    if let Some(m) = s.archive.as_mut() {
        m.chat = Some(id);
    }
    true
}

/// The Archive shown (every page as it was up to the moment) or now, on the page he is on.
pub fn show(s: &mut App, showing: bool) {
    if let Some(m) = s.archive.as_mut() {
        m.showing = showing;
    }
    s.tickets.shown = None;
}

/// "Bring it all back": the moment taken away, every list as before; the new chat stays, an ordinary thread.
pub fn bring_back(s: &mut App) {
    s.archive = None;
    s.archive_cache.borrow_mut().by.clear();
    crate::state::write_now(s);
}

/// The sidebar's Archive row, after Project, while there is a moment: lit while the Archive is shown.
pub fn sidebar_row(s: &App, k: &Kit) -> Option<El> {
    let m = on(s)?;
    let row = rune_kit::shell::SidebarRow::new("Archive").detail(format!("up to {}", crate::when::at(&m.at))).height(32.0).selected(m.showing).on_select(|s: &mut App, _| {
        show(s, true);
        s.sel = Sel::Tickets;
    });
    Some(row.build(k).key("archive-row"))
}

/// The bar across the top of every page while the Archive is shown.
pub fn bar(s: &App, k: &Kit) -> Option<El> {
    let m = on(s).filter(|m| m.showing)?;
    let back = Button::new("Back to now").variant(ButtonVariant::Ghost).on_press(|s: &mut App| show(s, false)).build(k).key("archive-now");
    let all = Button::new("Bring it all back").variant(ButtonVariant::Ghost).on_press(bring_back).build(k).key("archive-bring");
    Some(
        row()
            .key("archive-bar")
            .w_full()
            .px(16.0)
            .py(6.0)
            .gap(8.0)
            .center_y()
            .fill(k.c.layer_01)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_weak)
            .child(text(format!("The Archive · up to {}", crate::when::at(&m.at))).font(k.ty.small_medium).color(k.c.text_base).grow(1.0))
            .child(back)
            .child(all),
    )
}

/// What the History label says of the moment changed from WAS to NOW (state.rs `said`).
pub fn said(now: Option<&Mark>) -> String {
    match now {
        None => "everything brought back".into(),
        Some(m) if m.showing => "the Archive".into(),
        Some(m) => format!("everything up to {} archived", crate::when::at(&m.at)),
    }
}

#[cfg(test)]
mod tests {
    //! The way back (paper-research N-1446 p2350, 3): a leg presses Archive, writes in the new chat, presses "Bring it
    //! all back", and every list is as before plus the new thread.
    use super::*;
    use rune::runtime::Runtime;
    use serde_json::{Value, json};

    fn view(v: Value) -> TicketView {
        let mut base = json!({"ctx": {}, "form": {}, "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1, "given": [], "posts": []});
        base.as_object_mut().unwrap().extend(v.as_object().unwrap().clone());
        serde_json::from_value(base).unwrap()
    }

    fn agent(i: usize, name: &str, state: &str, since: &str) -> AgentView {
        serde_json::from_value(json!({"id": format!("AGT-{i}"), "name": name, "parent": "owner", "kind": "claude", "model": "m", "state": state, "since": since,
            "created": "2026-11-18T08:00:00Z", "last_active": since, "tools": [], "items": []}))
        .unwrap()
    }

    fn settle(rt: &mut Runtime<App>) {
        rt.invalidate();
        for _ in 0..3 {
            rt.frame(0.016);
        }
    }

    fn press_at(rt: &mut Runtime<App>, r: [f32; 4]) {
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        settle(rt);
    }

    fn button(rt: &mut Runtime<App>, label: &str) {
        let r = rt.elements().into_iter().find(|e| !e.exiting && e.role == "button" && e.label.as_deref() == Some(label)).unwrap_or_else(|| panic!("the button {label}")).rect;
        press_at(rt, r);
    }

    fn keyed(rt: &mut Runtime<App>, key: &str) {
        let r = rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key}")).rect;
        press_at(rt, r);
    }

    fn texts(rt: &Runtime<App>) -> Vec<String> {
        rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text.or(e.label)).collect()
    }

    /// His lists as the window draws them: the sidebar's Structure, the Threads page's topics, its Waits on you rows
    /// (each with "from the Archive" where it says so), his pins, and the All agents table's names.
    #[derive(Debug, PartialEq)]
    struct Lists {
        structure: Vec<String>,
        topics: Vec<String>,
        waits: Vec<String>,
        pins: Vec<String>,
        agents: Vec<String>,
    }

    fn lists(rt: &mut Runtime<App>) -> Lists {
        rt.update(|s, _| {
            s.sel = Sel::Tickets;
            s.tickets.shown = None;
        });
        settle(rt);
        let els: Vec<_> = rt.elements().into_iter().filter(|e| !e.exiting).collect();
        let keys = |pre: &str| -> Vec<String> {
            let mut v: Vec<String> = els.iter().filter_map(|e| e.key.clone()).filter(|k| k.starts_with(pre)).collect();
            v.sort();
            v.dedup();
            v
        };
        let marked = |key: &str| els.iter().any(|e| e.path.contains(&format!("/{key}/")) && e.text.as_deref() == Some("from the Archive"));
        let waits = keys("need-").into_iter().map(|k| if marked(&k) { format!("{k} · from the Archive") } else { k }).collect();
        // the Structure's threads (its Map, Insights and fold rows are `tree-` too)
        let (structure, topics, pins) = (keys("tree-N-"), keys("goal-"), keys("pin-"));
        rt.update(|s, _| {
            s.sel = Sel::Agents;
            s.agents_tab = 1;
        });
        settle(rt);
        let names: Vec<String> = rt.state.projects[0].live.agents.iter().map(|a| a.name.clone()).collect();
        let mut agents: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting && e.path.contains("/all-frame/")).filter_map(|e| e.text).filter(|t| names.contains(t)).collect();
        agents.sort();
        agents.dedup();
        Lists { structure, topics, waits, pins, agents }
    }

    #[test]
    fn archive_then_a_word_in_the_new_chat_then_bring_it_all_back_leaves_every_list_as_before_plus_the_new_chat() {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        let old = "2026-11-20T07:00:00Z";
        // his chat (for pr), a thread that waits on him (qa's), an open goal of his; pr at work, qa idle, one stopped
        // before the moment
        for v in [
            view(json!({"id": "N-1", "n": 1, "kind": "thread", "opener": "owner", "form": {"to": "pr"}, "title": CHAT_LABEL, "opened_at": old,
                "posts": [{"n": 1, "at": old, "by": "owner", "text": "hello", "names": ["pr"]}]})),
            view(json!({"id": "N-2", "n": 2, "kind": "thread", "opener": "owner", "form": {"to": "qa"}, "title": "The lexer", "opened_at": "2026-11-21T07:00:00Z",
                "waits_on": [{"ticket": "N-2", "agent": "owner", "call": "decide"}]})),
            view(json!({"id": "N-3", "n": 3, "kind": "item", "opener": "owner", "title": "Spelling in every box", "opened_at": "2026-11-21T08:00:00Z"})),
        ] {
            p.live.apply(coord_types::Event { seq: v.n, body: coord_types::Body::Ticket(v) });
        }
        p.live.agents = vec![agent(0, "pr", "working", old), agent(1, "qa", "idle", old), agent(2, "gone", "stopped", "2026-11-19T08:00:00Z")];
        s.projects = vec![p];
        s.cur = 0;
        s.pins.list = vec![
            crate::sidebar::Pin { project: "research".into(), id: "N-1".into(), agent: CHAT_LABEL.into(), thread: true, lens: None },
            crate::sidebar::Pin { project: "research".into(), id: "AGT-0".into(), agent: "pr".into(), thread: false, lens: None },
        ];
        let out = std::env::temp_dir().join(format!("yaaihh-archive-all-{}.jsonl", std::process::id()));
        let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        let mut rt = crate::view_cost::window(s);
        let before = lists(&mut rt);
        assert_eq!(before.structure, ["tree-N-1", "tree-N-2", "tree-N-3"], "{before:?}");
        assert_eq!(before.waits, ["need-N-2"]);
        assert_eq!(before.agents, ["gone", "pr", "qa"]);

        // the press: the card says what goes, what stays open toward him with a count, and whom the new chat is with
        rt.update(|s, _| s.sel = Sel::Tickets);
        settle(&mut rt);
        button(&mut rt, "Archive everything up to now…");
        let card = texts(&rt);
        assert!(card.iter().any(|t| t.contains("chat N-1")), "{card:?}");
        assert!(card.iter().any(|t| t == "Still open for you · 2"), "the wait and the goal: {card:?}");
        assert!(card.iter().any(|t| t == "New chat with pr, starting with:"), "{card:?}");
        button(&mut rt, "Archive");
        let at = rt.state.archive.as_ref().expect("the moment kept").at.clone();
        let away = lists(&mut rt);
        assert_eq!(away.structure, Vec::<String>::new(), "nothing of before in Structure: {away:?}");
        // what waits on him stays, marked, and under it his open goal of the Archive (a row of its own, as a wait's)
        assert_eq!(away.waits, ["need-N-2 · from the Archive", "need-N-3 · from the Archive"], "what waits on him stays, marked");
        assert_eq!(away.pins, ["pin-research-AGT-0", "pin-research-N-1"], "his old chat keeps its pin until the new one is found");
        assert_eq!(away.agents, ["pr", "qa"], "the agent that stopped before is in the Archive");
        // (lists ends on All agents: the Threads page again)
        rt.update(|s, _| s.sel = Sel::Tickets);
        settle(&mut rt);
        assert!(texts(&rt).iter().any(|t| t == "Your goals from the Archive · 1"), "his open goal stays in reach");
        // a marked row's topic leads its words, as "from the Archive" stands where the topic stood
        assert!(texts(&rt).iter().any(|t| t == "The lexer · waits on you"), "{:?}", texts(&rt));
        // an ask to him raised since, on a thread of the Archive (pr asks on the old chat): under Waits on you, new, unmarked
        let asked = view(json!({"id": "N-1", "n": 1, "kind": "thread", "opener": "owner", "form": {"to": "pr"}, "title": CHAT_LABEL, "opened_at": old,
            "posts": [{"n": 1, "at": old, "by": "owner", "text": "hello", "names": ["pr"]},
                {"n": 2, "at": at, "by": "pr", "text": "A or B?", "told": ["owner"], "asks": [{"at": [0, 6]}], "asks_open": true}],
            "waits_on": [{"ticket": "N-1", "agent": "owner", "call": "asks", "post": 2}]}));
        rt.update(|s, _| _ = s.projects[0].live.apply(coord_types::Event { seq: 4, body: coord_types::Body::Ticket(asked) }));
        let since = lists(&mut rt);
        assert_eq!(since.waits, ["need-N-1.p2", "need-N-2 · from the Archive", "need-N-3 · from the Archive"], "an ask since is new: {since:?}");
        assert_eq!(since.structure, Vec::<String>::new(), "its thread stays in the Archive (pick 1, p2349)");

        // coordd opens the new chat: found, kept in the moment, pinned in the old chat's place
        let chat = view(json!({"id": "N-4", "n": 4, "kind": "thread", "opener": "owner", "form": {"to": "pr"}, "title": CHAT_LABEL, "opened_at": at,
            "posts": [{"n": 1, "at": at, "by": "owner", "text": "A clean start.", "names": ["pr"]}]}));
        rt.update(|s, _| {
            s.projects[0].live.apply(coord_types::Event { seq: 5, body: coord_types::Body::Ticket(chat.clone()) });
            assert!(step(s), "the new chat found");
        });
        assert_eq!(rt.state.archive.as_ref().and_then(|m| m.chat.as_deref()), Some("N-4"));
        assert_eq!(rt.state.pins.list.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["N-4", "N-1", "AGT-0"]);
        let now = lists(&mut rt);
        assert_eq!(now.structure, ["tree-N-4"]);
        assert_eq!(now.pins, ["pin-research-AGT-0", "pin-research-N-4"]);

        // he writes in it from its pin: his words go to the new chat
        keyed(&mut rt, "pin-research-N-4");
        assert_eq!(rt.state.tickets.shown, Some(4), "the pin opens the new chat");
        rt.update(|s, _| _ = s.composers.insert((0, crate::message_box::thread_place("N-4")), TextEdit::new("first word here".to_string())));
        settle(&mut rt);
        button(&mut rt, "Send");
        let mut chat = chat;
        chat.posts.push(serde_json::from_value(json!({"n": 2, "at": at, "by": "owner", "text": "first word here"})).unwrap());
        rt.update(|s, _| _ = s.projects[0].live.apply(coord_types::Event { seq: 6, body: coord_types::Body::Ticket(chat) }));

        // the Archive shown: everything as it was up to the moment, the new chat not in it
        keyed(&mut rt, "archive-row");
        assert!(rt.state.archive.as_ref().is_some_and(|m| m.showing));
        let then = lists(&mut rt);
        assert_eq!(then.structure, ["tree-N-1", "tree-N-2", "tree-N-3"], "{then:?}");
        assert!(texts(&rt).iter().any(|t| t.starts_with("The Archive · up to ")), "its bar");

        // Bring it all back: every list as before, plus the new chat
        button(&mut rt, "Bring it all back");
        assert!(rt.state.archive.is_none());
        let back = lists(&mut rt);
        let mut want = before;
        want.structure.push("tree-N-4".into());
        want.topics.push("goal-N-4".into());
        want.topics.sort();
        want.pins.push("pin-research-N-4".into());
        want.pins.sort();
        // and the ask raised meanwhile
        want.waits.push("need-N-1.p2".into());
        want.waits.sort();
        assert_eq!(back, want);

        // what went to coordd: the new chat's first message, to pr, a thread of its own with his chat's label; then his
        // words on it
        rt.state.projects[0].link = None;
        let _ = child.wait();
        let sent: Vec<Value> = std::fs::read_to_string(&out).unwrap_or_default().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).map(|v| v["req"].clone()).filter(|r| r["op"] == "say" || r["op"] == "post").collect();
        let _ = std::fs::remove_file(&out);
        assert_eq!(sent.len(), 2, "{sent:?}");
        assert_eq!((&sent[0]["op"], &sent[0]["to"], &sent[0]["label"], &sent[0]["thread"]), (&json!("say"), &json!("pr"), &json!(CHAT_LABEL), &json!(true)));
        assert_eq!((&sent[1]["op"], &sent[1]["t"], &sent[1]["text"]), (&json!("post"), &json!("N-4"), &json!("first word here")));
    }
}
