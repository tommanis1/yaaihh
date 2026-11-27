//! The line under each post of his: each agent's newest state (waiting, sent, delivered, not read, answered).

use crate::{App, Proj};
use docket_machine::{Post, ViewStatus};
use coord_types::TicketView;
use rune::{El, col, row, text};
use rune_kit::Kit;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// At most this many agents in sight; the rest as `+N more`, one click away.
const IN_SIGHT: usize = 3;

/// Where his post stands with one agent, its newest state.
#[derive(Clone, Debug, PartialEq)]
pub enum Is {
    /// Not handed yet. IN_TURN: the time its turn began, when it is in one; STOPPED: it is stopped; AHEAD: his own
    /// messages to it from before this one that it has not been handed either.
    Waiting { in_turn: Option<String>, stopped: bool, ahead: usize },
    /// Handed twice and its text entered neither time: two given rows of one delivery to it, both `unread`, none
    /// delivered (docket-core5's second miss, held to its next turn's end or start).
    Twice,
    /// Given AT, in its session's queue: its text has not entered the model's input yet.
    Sent { at: String },
    /// Its text entered the model's input AT (`ViewGiven.delivered`, 0.2 A1).
    Delivered { at: String },
    /// Its turn given this post ended with nothing posted (`quiet`).
    NoReply,
    /// Handed again for being left unanswered, and left so again: owed, its text entered AT.
    NotAnswered { at: Option<String> },
    /// Its first post on this thread after it was handed his, or a reply to it: POST.
    Answered { post: u32 },
    /// Not delivered: its session takes no messages, WHY in coordd's words with what he can do.
    NotDelivered { why: String },
}

/// One agent's entry: its newest state, and the chain of states with their times, oldest first.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub agent: String,
    pub is: Is,
    pub chain: Vec<String>,
}

impl Entry {
    /// The entry in sight: `lead · sent 20:03`.
    pub fn line(&self) -> String {
        let state = match &self.is {
            Is::Waiting { stopped: true, .. } => "waiting · stopped, handed when it starts".to_string(),
            Is::Waiting { in_turn: Some(since), ahead, .. } => {
                let ahead = match ahead {
                    0 => String::new(),
                    1 => " (1 of yours before this one)".to_string(),
                    n => format!(" ({n} of yours before this one)"),
                };
                format!("waiting · in a turn since {}{ahead}", crate::when::at(since))
            }
            Is::Waiting { ahead: 0, .. } => "waiting".to_string(),
            Is::Waiting { ahead, .. } => format!("waiting ({ahead} of yours before this one)"),
            Is::Twice => "handed twice · not read".to_string(),
            Is::Sent { at } => format!("sent {}", crate::when::at(at)),
            Is::Delivered { at } => format!("delivered {}", crate::when::at(at)),
            Is::NoReply => "no reply".to_string(),
            Is::NotAnswered { at: Some(at) } => format!("delivered {} · not answered", crate::when::at(at)),
            Is::NotAnswered { at: None } => "not answered".to_string(),
            Is::Answered { post, .. } => format!("answered in #{post}"),
            Is::NotDelivered { why } => format!("not delivered: {why}"),
        };
        format!("{} · {state}", self.agent)
    }
}

/// What the engine does with the post itself, said before the agents (or in their place): held, dropped.
#[derive(Clone, Debug, PartialEq)]
pub enum Engine {
    None,
    Dropped,
    /// Held by its handler (id): deciding whom it is for; or asking him, WHY.
    Held { handler: String, asking: Option<String> },
    /// Held with his burst while its handler (id) waits on his `gather`: goes at GOES or at Send now; GOES none when slowed.
    Gathered { handler: String, goes: Option<String> },
}

/// When handler H's hold lets his burst go: coordd's `lapses_at`, none when slowed; None if H gathers nothing of his.
pub(crate) fn gathers(h: &TicketView) -> Option<Option<String>> {
    matches!(&h.status, ViewStatus::Awaits { agent, call, .. } if agent == "owner" && call == docket_machine::syntax::GATHER).then(|| h.lapses_at.clone())
}

/// The held mark's words for a burst that goes at GOES (`gathers`): "held · goes at 20:07:15", on his clock; "held"
/// with no time. His "Send now" stands after them.
pub(crate) fn held_goes(goes: Option<&str>) -> String {
    goes.map_or_else(|| "held".to_string(), |at| format!("held · goes at {}", crate::when::exact(at)))
}

/// Where post X of V stands: what the engine does with it, and one entry per agent it went to.
#[derive(Clone, Debug, PartialEq)]
pub struct Where {
    pub engine: Engine,
    pub entries: Vec<Entry>,
}

impl Where {
    /// Nothing waits: every agent it went to has answered (no line, a faint mark).
    pub fn settled(&self) -> bool {
        self.engine == Engine::None && !self.entries.is_empty() && self.entries.iter().all(|e| matches!(e.is, Is::Answered { .. }))
    }
}

type Stamp = (u64, usize, u64);

thread_local! {
    /// Each thread's posts' entries, by (project, ticket), with the snapshot they were worked out from.
    static KEPT: RefCell<HashMap<(String, u64), (Stamp, Rc<HashMap<u32, Where>>)>> = RefCell::new(HashMap::new());
}

/// The agents' states as a number: a change of any agent's state or its time works the entries out again.
fn agents_stamp(p: &Proj) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for a in &p.live.agents {
        (&a.name, &a.state, &a.since, &a.deaf).hash(&mut h);
    }
    h.finish()
}

/// Where his post X of V stands; None for a post not his, or one taken back.
pub fn of(p: &Proj, v: &TicketView, x: &Post) -> Option<Where> {
    if x.by != "owner" || x.taken_back || x.moved.is_some() {
        return None;
    }
    let stamp: Stamp = (p.live.tickets_rev, v.posts.len(), agents_stamp(p));
    let key = (p.id.clone(), v.n);
    let all = KEPT.with(|k| {
        if let Some((at, w)) = k.borrow().get(&key)
            && *at == stamp
        {
            return w.clone();
        }
        let w = Rc::new(work_out(p, v));
        k.borrow_mut().insert(key.clone(), (stamp, w.clone()));
        w
    });
    all.get(&x.n).cloned()
}

/// Every post of his on V, worked out in one pass over the thread.
fn work_out(p: &Proj, v: &TicketView) -> HashMap<u32, Where> {
    let closed = matches!(v.status, ViewStatus::Done { .. } | ViewStatus::Failed { .. });
    // each agent's own posts here, in order: its answer is the first after it was handed his
    let mut by: HashMap<&str, Vec<&Post>> = HashMap::new();
    for y in v.posts.iter().filter(|y| y.by != "owner" && !y.taken_back) {
        by.entry(y.by.as_str()).or_default().push(y);
    }
    let mut ahead_of: HashMap<String, Vec<String>> = HashMap::new();
    let mut out = HashMap::new();
    for x in v.posts.iter().filter(|x| x.by == "owner" && !x.taken_back && x.moved.is_none()) {
        let engine = if x.dropped {
            Engine::Dropped
        } else if let Some(h) = &x.held {
            let hv = v.handlers.iter().find(|t| t.id == *h);
            if let Some(goes) = hv.and_then(gathers) {
                Engine::Gathered { handler: h.clone(), goes }
            } else {
                let asking = hv.and_then(|t| match &t.status {
                    ViewStatus::Awaits { agent, call, args, .. } if agent == "owner" && call == "clarify" => Some(args["why"].as_str().unwrap_or("").to_string()),
                    _ => None,
                });
                Engine::Held { handler: h.clone(), asking }
            }
        } else {
            Engine::None
        };
        let mut entries = Vec::new();
        if engine == Engine::None {
            for agent in went_to(x) {
                let state = p.live.agents.iter().find(|a| a.name == agent);
                // its session takes no messages: what is for it does not reach it, handed or not (`AgentView::deaf`)
                let deaf = state.and_then(|a| a.deaf.clone());
                // the rows that handed it, by the engine's own test (`ViewGiven::hands`)
                let rows: Vec<&docket_machine::view::ViewGiven> = v.given.iter().filter(|g| g.to == agent && g.hands(x.n)).collect();
                let given = rows.iter().map(|g| g.at.to_string()).min();
                let (is, chain) = match given {
                    Some(at) => {
                        let answer = by.get(agent.as_str()).and_then(|ys| ys.iter().find(|y| y.answers == Some(x.n) || (y.n > x.n && y.at.to_string() >= at)));
                        // delivered: only from the fact, the first giving whose text entered (0.2 A1), never from `at`
                        let delivered = rows.iter().filter_map(|g| g.delivered.as_ref().map(|d| d.to_string())).min();
                        // handed twice, entered neither time: two rows of one delivery, both unread (docket-core5's miss)
                        let again = rows.iter().find(|g| rows.iter().filter(|h| h.delivery == g.delivery && h.unread).count() >= 2 && g.unread);
                        let twice = delivered.is_none() && again.is_some();
                        let mut chain = vec![format!("sent {}", crate::when::at(&at))];
                        if twice {
                            let last = rows.iter().filter(|g| Some(g.delivery.as_str()) == again.map(|a| a.delivery.as_str())).map(|g| g.at.to_string()).max().unwrap_or_default();
                            chain.push(format!("sent again {}", crate::when::at(&last)));
                            chain.push("handed twice · not read".to_string());
                        }
                        if let Some(d) = &delivered {
                            chain.push(format!("delivered {}", crate::when::at(d)));
                        }
                        match answer {
                            Some(y) => {
                                let yat = y.at.to_string();
                                chain.push(format!("answered in #{} {}", y.n, crate::when::at(&yat)));
                                (Is::Answered { post: y.n }, chain)
                            }
                            // owed: left unanswered in two turns, not handed again, whatever the last one said
                            None if rows.iter().any(|g| g.owed) => {
                                chain.push("not answered".to_string());
                                (Is::NotAnswered { at: delivered }, chain)
                            }
                            None if v.quiet.iter().any(|q| q.agent == agent && q.upto >= x.n) => {
                                chain.push("no reply".to_string());
                                (Is::NoReply, chain)
                            }
                            None => match delivered {
                                Some(at) => (Is::Delivered { at }, chain),
                                None if deaf.is_some() => {
                                    let why = deaf.clone().unwrap_or_default();
                                    chain.push(format!("not delivered: {why}"));
                                    (Is::NotDelivered { why }, chain)
                                }
                                None if twice => (Is::Twice, chain),
                                None => (Is::Sent { at }, chain),
                            },
                        }
                    }
                    // a closed thread hands nothing more: no entry waits on it
                    None if closed => continue,
                    None if deaf.is_some() => {
                        let why = deaf.clone().unwrap_or_default();
                        (Is::NotDelivered { why: why.clone() }, vec![format!("not delivered: {why}")])
                    }
                    None => {
                        let stopped = state.is_some_and(|a| a.state == "stopped");
                        let in_turn = state.filter(|a| a.state == "working").map(|a| a.since.clone());
                        let earlier = ahead_of.entry(agent.clone()).or_insert_with(|| unhanded(p, &agent));
                        let at = x.at.to_string();
                        let ahead = earlier.iter().filter(|t| **t < at).count();
                        let why = match (&in_turn, stopped) {
                            (_, true) => "waiting · it is stopped".to_string(),
                            (Some(since), _) => format!("waiting · in a turn since {}", crate::when::at(since)),
                            _ => "waiting".to_string(),
                        };
                        (Is::Waiting { in_turn, stopped, ahead }, vec![why])
                    }
                };
                entries.push(Entry { agent, is, chain });
            }
        }
        stuck_first(&mut entries);
        out.insert(x.n, Where { engine, entries });
    }
    out
}

/// Orders the entries as he looks for a stuck post: waiting, handed and not read, no reply, answered.
fn stuck_first(entries: &mut [Entry]) {
    entries.sort_by(|a, b| {
        let rank = |e: &Entry| match &e.is {
            // what he can act on first: a session that takes no messages
            Is::NotDelivered { .. } => 0,
            Is::Waiting { .. } => 1,
            Is::Twice | Is::NotAnswered { .. } => 2,
            Is::Sent { .. } => 3,
            Is::Delivered { .. } => 4,
            Is::NoReply => 5,
            Is::Answered { .. } => 6,
        };
        rank(a).cmp(&rank(b)).then_with(|| match (&a.is, &b.is) {
            (Is::Waiting { ahead: x, in_turn: tx, .. }, Is::Waiting { ahead: y, in_turn: ty, .. }) => {
                y.cmp(x).then_with(|| tx.is_none().cmp(&ty.is_none())).then_with(|| tx.cmp(ty))
            }
            (Is::Sent { at: x }, Is::Sent { at: y }) | (Is::Delivered { at: x }, Is::Delivered { at: y }) => x.cmp(y),
            _ => std::cmp::Ordering::Equal,
        })
    });
}

/// How many entries are in sight before `+N more`: three, or every open one when more are open; so the line hides only
/// answered ones while any is open.
fn in_sight(entries: &[Entry]) -> usize {
    entries.len().min(IN_SIGHT).max(entries.iter().filter(|e| !matches!(e.is, Is::Answered { .. })).count())
}

/// The agents post X was for, in order: whom it named or its handler told; never the owner, nor whom it informs.
fn went_to(x: &Post) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let first: &[String] = if x.routed { &x.told } else { &x.names };
    let also: &[String] = if x.routed { &[] } else { &x.told };
    for n in first.iter().chain(also.iter()) {
        if n != "owner" && !out.contains(n) {
            out.push(n.clone());
        }
    }
    out
}

/// The times of his posts naming AGENT, on any open ticket, that it has not been handed: what is before a post of his
/// that waits for it.
fn unhanded(p: &Proj, agent: &str) -> Vec<String> {
    let mut out = Vec::new();
    for v in p.live.tickets.values().filter(|v| !matches!(v.status, ViewStatus::Done { .. } | ViewStatus::Failed { .. })) {
        let upto = crate::seen::read_upto(v, agent);
        for x in v.posts.iter().filter(|x| x.by == "owner" && x.n > upto && !x.dropped && !x.taken_back && x.held.is_none()) {
            if x.names.iter().chain(x.told.iter()).any(|n| n == agent) {
                out.push(x.at.to_string());
            }
        }
    }
    out
}

/// Under his post X of V: its line (the engine's state, or each agent's newest), a click opening the chains; the
/// faint mark when every agent answered; nothing for a post not his.
pub(crate) fn line(s: &App, p: &Proj, v: &TicketView, x: &Post, k: &Kit) -> Option<El> {
    let w = of(p, v, x)?;
    let key = format!("{}.p{}", v.id, x.n);
    let open = s.where_open.contains(&key);
    let toggle = |key: String| {
        move |s: &mut App| {
            if !s.where_open.remove(&key) {
                s.where_open.insert(key.clone());
            }
        }
    };
    let faint = |t: String| text(t).font(k.ty.small).color(k.c.text_faint);
    let mut c = col().key(format!("where-{key}")).w_full().gap(2.0).items_end();
    match &w.engine {
        Engine::Dropped => return Some(c.child(faint("dropped: no agent was given it".into()))),
        Engine::Held { asking: None, .. } => return Some(c.child(faint("held: deciding whom it is for".into()))),
        Engine::Held { handler, asking: Some(why) } => {
            let pi = s.projects.iter().position(|q| std::ptr::eq(q, p)).unwrap_or(s.cur);
            let head = if why.is_empty() { "not given to anyone".to_string() } else { format!("not given to anyone: {why}") };
            return Some(c.child(text(head).font(k.ty.small).color(k.c.text_muted).wrap()).child(crate::chat::held_choices(pi, v, handler, k)));
        }
        // held with his burst: when it goes, and his "Send now" (gather.call's held mark)
        Engine::Gathered { handler, goes } => {
            let pi = s.projects.iter().position(|q| std::ptr::eq(q, p)).unwrap_or(s.cur);
            return Some(c.child(row().gap(8.0).items_center().child(faint(held_goes(goes.as_deref()))).child(crate::chat::send_now(pi, handler, k))));
        }
        Engine::None => {}
    }
    if w.entries.is_empty() {
        // in the thread and nobody given it yet: the engine has it
        return not_given_yet(x).then(|| c.child(faint("sent".into())));
    }
    let others = others(x, &w);
    let seen_by = |c: El| if others == 0 { c } else { c.child(faint(format!("{others} other{} can see it", if others == 1 { "" } else { "s" }))) };
    if w.settled() && !open {
        // every agent answered: a faint mark only, which opens the chain
        return Some(c.child(k.interactive(row().px(4.0).child(faint("✓".into()))).on_press(toggle(key))));
    }
    if open {
        for e in &w.entries {
            c = c.child(faint(format!("{} · {}", e.agent, e.chain.join(" → "))).wrap());
        }
        return Some(seen_by(c).child(k.interactive(row().child(faint("close".into()))).on_press(toggle(key))));
    }
    let all = s.where_open.contains(&format!("{key}+"));
    let shown = if all { w.entries.len() } else { in_sight(&w.entries) };
    for e in &w.entries[..shown] {
        // an answered entry's `#N` is its own press (below), so the words stop before it
        let words = match e.is {
            Is::Answered { .. } => format!("{} · answered in", e.agent),
            _ => e.line(),
        };
        // not delivered is the one accent, with what he can do as presses; every other state is quiet
        let deaf = matches!(e.is, Is::NotDelivered { .. });
        let words = match p.live.agents.iter().find(|a| a.name == e.agent).filter(|_| deaf) {
            Some(a) => {
                let pi = s.projects.iter().position(|q| std::ptr::eq(q, p)).unwrap_or(s.cur);
                crate::agents::deaf_links(text(words.clone()).font(k.ty.small).color(k.c.text_accent), &words, pi, a, k)
            }
            None if deaf => text(words).font(k.ty.small).color(k.c.text_accent),
            None => faint(words),
        };
        // a long line wraps in a column of the post's width, or it runs past the pane's left edge
        let entry = k.interactive(if deaf { col().w_full().items_end().child(words.wrap()) } else { row().child(words.wrap()) }).on_press(toggle(key.clone()));
        // `answered in #N`: the post is a press away
        c = c.child(match e.is {
            Is::Answered { post, .. } => {
                let (t, id) = (v.n, v.id.clone());
                row().gap(4.0).center_y().child(entry).child(k.interactive(row().child(text(format!("#{post}")).font(k.ty.small).color(k.c.text_muted))).on_press(move |s: &mut App| crate::find::open_at(s, t, &id, Some(post))))
            }
            _ if deaf => entry,
            _ => row().child(entry),
        });
    }
    if w.entries.len() > shown {
        c = c.child(k.interactive(row().child(faint(format!("+{} more", w.entries.len() - shown)))).on_press(toggle(format!("{key}+"))));
    }
    Some(seen_by(c))
}

/// Under his post X when a dock follows it: only what needs his press (the stages are the dock's); else [`line`].
pub(crate) fn line_by_dock(s: &App, p: &Proj, v: &TicketView, x: &Post, k: &Kit) -> Option<El> {
    let quiet = |w: Where| w.engine == Engine::None && !w.entries.iter().any(|e| matches!(e.is, Is::NotDelivered { .. }));
    if crate::outgoing::follows(p, v, x) && of(p, v, x).is_some_and(quiet) {
        return None;
    }
    line(s, p, v, x, k)
}

/// Whether his post X is in the thread and its handler has not said yet whom it wakes, nor did he name anyone: `sent`.
pub(crate) fn not_given_yet(x: &Post) -> bool {
    !x.routed && x.names.is_empty() && x.told.is_empty() && x.held.is_none() && !x.dropped
}

/// How many agents post X informs beside those it is for (W's entries), said as one `N others can see it`.
fn others(x: &Post, w: &Where) -> usize {
    let mut seen: Vec<&str> = Vec::new();
    for a in x.inform.iter().filter(|a| *a != "owner" && !w.entries.iter().any(|e| &e.agent == *a)) {
        if !seen.contains(&a.as_str()) {
            seen.push(a);
        }
    }
    seen.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn proj(agents: &[(&str, &str, &str)]) -> Proj {
        let mut p = Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        p.live.agents = agents
            .iter()
            .map(|(name, state, since)| {
                serde_json::from_value(json!({"id": "AGT-1", "name": name, "parent": "owner", "kind": "claude", "model": "m", "state": state, "since": since, "tools": [], "items": []})).unwrap()
            })
            .collect();
        p
    }

    fn thread(n: u64, posts: serde_json::Value, given: serde_json::Value) -> TicketView {
        serde_json::from_value(json!({"id": format!("N-{n}"), "n": n, "kind": "thread", "opener": "owner", "ctx": {}, "form": {}, "title": "t",
            "status": {"is": "resting"}, "children": [], "program": "", "bound": 5, "spent": 1, "opened_at": "2026-11-19T19:00:00Z",
            "given": given, "posts": posts}))
        .unwrap()
    }

    fn put(p: &mut Proj, v: &TicketView) {
        p.live.tickets.insert(v.n, v.clone());
        p.live.tickets_rev += 1;
    }

    /// A post is for whom it named, not for whoever was handed a later post on the thread.
    #[test]
    fn a_post_is_for_whom_it_named_not_whoever_got_a_later_one() {
        let mut p = proj(&[("a", "idle", ""), ("b", "idle", ""), ("c", "idle", "")]);
        let v = thread(
            8,
            json!([
                {"n": 1, "at": "2026-11-19T19:02:00Z", "by": "owner", "text": "two rows", "names": ["a"]},
                {"n": 2, "at": "2026-11-19T19:04:00Z", "by": "a", "text": "done"},
                {"n": 3, "at": "2026-11-19T19:05:00Z", "by": "a", "text": "b and c: check it", "names": ["b", "c"]}
            ]),
            json!([
                {"to": "a", "delivery": "N-8.p1", "upto": 1, "at": "2026-11-19T19:03:00Z"},
                {"to": "b", "delivery": "N-8.p3", "upto": 3, "at": "2026-11-19T19:06:00Z"},
                {"to": "c", "delivery": "N-8.p3", "upto": 3, "at": "2026-11-19T19:06:00Z"}
            ]),
        );
        put(&mut p, &v);
        let w = of(&p, &v, &v.posts[0]).unwrap();
        let lines: Vec<String> = w.entries.iter().map(Entry::line).collect();
        assert_eq!(lines, ["a · answered in #2"]);
        assert!(w.settled());
    }

    /// Under his post before anyone is given it: `sent`; the agents it informs are one count, not a line each.
    #[test]
    fn sent_until_someone_has_it_and_the_informed_are_one_count() {
        let mut p = proj(&[("a", "idle", ""), ("b", "idle", ""), ("c", "idle", "")]);
        let v = thread(9, json!([{"n": 1, "at": "2026-11-19T19:02:00Z", "by": "owner", "text": "two rows"}]), json!([]));
        put(&mut p, &v);
        let x = &v.posts[0];
        assert!(of(&p, &v, x).unwrap().entries.is_empty());
        assert!(not_given_yet(x), "nobody named, not routed yet: sent");
        let v = thread(9, json!([{"n": 1, "at": "2026-11-19T19:02:00Z", "by": "owner", "text": "two rows", "routed": true, "told": ["a"], "inform": ["b", "c", "a", "owner"]}]), json!([]));
        put(&mut p, &v);
        let x = &v.posts[0];
        assert!(!not_given_yet(x), "routed: the entries say it");
        let w = of(&p, &v, x).unwrap();
        assert_eq!(w.entries.iter().map(|e| e.agent.as_str()).collect::<Vec<_>>(), ["a"]);
        assert_eq!(others(x, &w), 2, "b and c, not a (it is for a) nor the owner");
    }

    /// The open entries first: waiting, handed, read, then `+2 more` for the answered.
    #[test]
    fn the_open_ones_come_first_and_more_hides_the_answered() {
        let mut p = proj(&[("a1", "idle", ""), ("r", "idle", ""), ("a2", "idle", ""), ("h", "idle", ""), ("w", "working", "2026-11-19T19:01:00Z")]);
        let v = thread(
            6,
            json!([
                {"n": 1, "at": "2026-11-19T19:02:00Z", "by": "owner", "text": "two rows", "names": ["a1", "r", "a2", "h", "w"]},
                {"n": 2, "at": "2026-11-19T19:05:00Z", "by": "a1", "text": "done"},
                {"n": 3, "at": "2026-11-19T19:06:00Z", "by": "a2", "text": "also done"}
            ]),
            json!([
                {"to": "a1", "delivery": "N-6.p1", "upto": 1, "at": "2026-11-19T19:03:00Z"},
                {"to": "r", "delivery": "N-6.p1", "upto": 1, "at": "2026-11-19T19:03:00Z"},
                {"to": "a2", "delivery": "N-6.p1", "upto": 1, "at": "2026-11-19T19:03:00Z"},
                {"to": "h", "delivery": "N-6.p1", "upto": 1, "at": "2026-11-19T19:04:00Z"}
            ]),
        );
        let mut v = v;
        v.quiet = serde_json::from_value(json!([{"agent": "r", "upto": 1}])).unwrap();
        put(&mut p, &v);
        let w = of(&p, &v, &v.posts[0]).unwrap();
        let order: Vec<&str> = w.entries.iter().map(|e| e.agent.as_str()).collect();
        assert_eq!(order, ["w", "h", "r", "a1", "a2"]);
        assert_eq!(in_sight(&w.entries), 3, "the three open ones in sight; +2 more hides the two answered");
        // more open than three: none of them is hidden
        let open = |agent: &str| Entry { agent: agent.into(), is: Is::NoReply, chain: vec![] };
        assert_eq!(in_sight(&[open("a"), open("b"), open("c"), open("d"), Entry { agent: "e".into(), is: Is::Answered { post: 2 }, chain: vec![] }]), 4);
    }

    /// Each state in its words, newest only: waiting in a turn with his earlier message ahead, handed, read with no
    /// reply, answered; the agent first, clock times, never a counter.
    #[test]
    fn each_agent_shows_its_newest_state_in_words() {
        let mut p = proj(&[("lead", "working", "2026-11-19T19:01:00Z"), ("fixer", "idle", ""), ("pipe", "idle", ""), ("cone", "idle", "")]);
        // his earlier message to lead on another thread, not handed yet either
        let other = thread(4, json!([{"n": 1, "at": "2026-11-19T19:00:30Z", "by": "owner", "text": "first", "names": ["lead"]}]), json!([]));
        put(&mut p, &other);
        let v = thread(
            5,
            json!([
                {"n": 1, "at": "2026-11-19T19:02:00Z", "by": "owner", "text": "use the median", "names": ["lead", "fixer", "pipe", "cone"]},
                {"n": 2, "at": "2026-11-19T19:05:00Z", "by": "fixer", "text": "done"}
            ]),
            json!([
                {"to": "fixer", "delivery": "N-5.p1", "upto": 1, "at": "2026-11-19T19:03:00Z"},
                {"to": "pipe", "delivery": "N-5.p1", "upto": 1, "at": "2026-11-19T19:03:00Z"},
                {"to": "cone", "delivery": "N-5.p1", "upto": 1, "at": "2026-11-19T19:03:00Z"}
            ]),
        );
        let mut v = v;
        v.quiet = serde_json::from_value(json!([{"agent": "pipe", "upto": 1}])).unwrap();
        put(&mut p, &v);
        let w = of(&p, &v, &v.posts[0]).unwrap();
        let at = |t: &str| crate::when::at(t);
        let lines: Vec<String> = w.entries.iter().map(Entry::line).collect();
        assert_eq!(
            lines,
            [
                format!("lead · waiting · in a turn since {} (1 of yours before this one)", at("2026-11-19T19:01:00Z")),
                format!("cone · sent {}", at("2026-11-19T19:03:00Z")),
                "pipe · no reply".to_string(),
                "fixer · answered in #2".to_string(),
            ]
        );
        assert!(lines.iter().all(|l| !l.contains("read") && !l.contains("handed 1")), "no line says read: {lines:?}");
        assert_eq!(w.entries[3].chain, [format!("sent {}", at("2026-11-19T19:03:00Z")), format!("answered in #2 {}", at("2026-11-19T19:05:00Z"))]);
        assert!(!w.settled());
        // an agent's post from before it was handed his is no answer to it
        let early = thread(6, json!([{"n": 1, "at": "2026-11-19T19:02:00Z", "by": "owner", "text": "x", "names": ["fixer"]}, {"n": 2, "at": "2026-11-19T19:02:30Z", "by": "fixer", "text": "y"}]), json!([{"to": "fixer", "delivery": "N-6.p1", "upto": 1, "at": "2026-11-19T19:04:00Z"}]));
        put(&mut p, &early);
        assert!(matches!(of(&p, &early, &early.posts[0]).unwrap().entries[0].is, Is::Sent { .. }));
    }

    /// Every agent answered: settled (no line, the faint mark). Held and dropped say the engine's state in the agents'
    /// place; a post the handler asks him about carries its question.
    #[test]
    fn answered_by_all_is_settled_and_held_or_dropped_says_the_engine() {
        let mut p = proj(&[("lead", "idle", "")]);
        let v = thread(7, json!([{"n": 1, "at": "2026-11-19T19:02:00Z", "by": "owner", "text": "x", "names": ["lead"]}, {"n": 2, "at": "2026-11-19T19:05:00Z", "by": "lead", "text": "ok", "answers": 1}]), json!([{"to": "lead", "delivery": "N-7.p1", "upto": 1, "at": "2026-11-19T19:03:00Z"}]));
        put(&mut p, &v);
        assert!(of(&p, &v, &v.posts[0]).unwrap().settled());
        assert!(of(&p, &v, &v.posts[1]).is_none(), "an agent's post has no line");
        let mut h = thread(8, json!([{"n": 1, "at": "2026-11-19T19:02:00Z", "by": "owner", "text": "x", "held": "N-9"}, {"n": 2, "at": "2026-11-19T19:02:10Z", "by": "owner", "text": "y", "dropped": true}]), json!([]));
        h.handlers = vec![serde_json::from_value(json!({"id": "N-9", "n": 9, "kind": "owner_post", "opener": "coordd", "ctx": {}, "form": {}, "title": "t",
            "status": {"is": "awaits", "agent": "owner", "call": "clarify", "step": 1, "since": "2026-11-19T19:02:00Z", "taken": false, "refusals": 0, "forming": false, "head": {"Variant": [["Drop", []]]}, "args": {"why": "whom is this for?"}}, "children": [], "program": "", "bound": 5, "spent": 1,
            "opened_at": "2026-11-19T19:02:00Z", "given": [], "posts": []}))
        .unwrap()];
        put(&mut p, &h);
        assert_eq!(of(&p, &h, &h.posts[0]).unwrap().engine, Engine::Held { handler: "N-9".into(), asking: Some("whom is this for?".into()) });
        assert_eq!(of(&p, &h, &h.posts[1]).unwrap().engine, Engine::Dropped);
    }

    /// A post to a session that takes no messages reads `not delivered` and why, first; once it takes them, `sent`.
    #[test]
    fn to_a_session_that_takes_no_messages_his_post_reads_not_delivered_and_why_never_sent() {
        let why = "its session takes no messages (Claude Code says “Channels are not currently available”); pick an account for this agent";
        let mut p = proj(&[("lead", "idle", ""), ("try-1", "idle", "")]);
        p.live.agents[1].deaf = Some(why.into());
        let mut v = thread(12, json!([{"n": 1, "at": "2026-11-19T19:02:00Z", "by": "owner", "text": "x", "names": ["lead", "try-1"]}]), json!([]));
        put(&mut p, &v);
        let lines = |p: &Proj, v: &TicketView| of(p, v, &v.posts[0]).unwrap().entries.iter().map(Entry::line).collect::<Vec<_>>();
        assert_eq!(lines(&p, &v), [format!("try-1 · not delivered: {why}"), "lead · waiting".to_string()], "waiting for it: not delivered, first");
        v.given = serde_json::from_value(json!([{"to": "lead", "delivery": "N-12.p1", "upto": 1, "at": "2026-11-19T19:03:00Z"}, {"to": "try-1", "delivery": "N-12.p1", "upto": 1, "at": "2026-11-19T19:03:00Z"}])).unwrap();
        put(&mut p, &v);
        let sent = format!("sent {}", crate::when::at("2026-11-19T19:03:00Z"));
        assert_eq!(lines(&p, &v), [format!("try-1 · not delivered: {why}"), format!("lead · {sent}")], "handed and never seen there: not delivered");
        p.live.agents[1].deaf = None;
        assert_eq!(lines(&p, &v), [format!("lead · {sent}"), format!("try-1 · {sent}")], "it takes them again: sent");
    }

    /// What he can do about a deaf session is a press from the view's `deaf_do`: under his post, on its page and tile.
    #[test]
    fn what_he_can_do_about_a_deaf_session_is_a_press_in_its_words() {
        let why = "what was handed to its session never reached it (m5); restart it, or pick an account for this agent";
        let app = |acts: bool| {
            let (held, mut s, child) = crate::view_cost::linked_app(&["lead", "try-1"]);
            let p = &mut s.projects[0];
            p.protocol = coord_types::PROTOCOL;
            p.live.agents[1].deaf = Some(why.into());
            if acts {
                // as core-deafdo's coordd sends them for deliveries never seen there
                p.live.agents[1].deaf_do = vec![coord_types::DeafDo::restart(), coord_types::DeafDo { act: coord_types::DEAF_ACCOUNT.into(), words: "pick an account for this agent".into() }];
            }
            let v = thread(12, json!([{"n": 1, "at": "2026-11-24T11:02:00Z", "by": "owner", "text": "are you there?", "names": ["try-1"]}]), json!([]));
            p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(v) });
            s.sel = crate::Sel::Tickets;
            s.tickets.shown = Some(12);
            let mut rt = crate::view_cost::window(s);
            for _ in 0..4 {
                rt.frame(0.016);
            }
            (held, rt, child)
        };
        let link = |rt: &rune::runtime::Runtime<App>, act: &str, under: &str| {
            rt.elements().into_iter().find(|e| e.key.as_deref() == Some(act) && e.path.contains("#link-") && e.path.contains(under)).map(|e| e.rect)
        };
        // the pointer over them first: a tile under the pointer shows its switch, and its line wraps again
        let press = |rt: &mut rune::runtime::Runtime<App>, act: &str, under: &str| {
            let mid = |r: [f32; 4]| (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
            let (x, y) = mid(link(rt, act, under).unwrap_or_else(|| panic!("{act} words are a press under {under:?}")));
            rt.pointer_move(x, y);
            for _ in 0..4 {
                rt.frame(0.016);
            }
            let (x, y) = mid(link(rt, act, under).unwrap_or_else(|| panic!("{act} words are a press under {under:?}, the pointer on them")));
            rt.pointer_move(x, y);
            rt.frame(0.016);
            rt.pointer_down(x, y);
            rt.frame(0.016);
            rt.pointer_up(x, y);
            for _ in 0..4 {
                rt.frame(0.016);
            }
        };
        let restarts = |rt: &rune::runtime::Runtime<App>| rt.state.projects[0].pending.values().filter(|q| matches!(q, crate::Pending::Effort { agent } if agent == "try-1")).count();
        let settings_open = |rt: &rune::runtime::Runtime<App>| rt.state.streams.open.contains("session|0|try-1");

        {
            let (_held, rt, mut child) = app(false);
            assert!(crate::popup::tests::text_under(&rt, "where-N-12.p1", "pick an account for this agent").is_some(), "the line is under his post");
            assert!(link(&rt, "account", "").is_none() && link(&rt, "restart", "").is_none(), "no deaf_do: words only");
            let _ = child.kill();
        }

        let (_held, mut rt, mut child) = app(true);
        // under his post: "pick an account for this agent"
        assert!(link(&rt, "account", "where-N-12.p1").is_some() && link(&rt, "restart", "where-N-12.p1").is_some(), "both acts' words are presses under his post");
        press(&mut rt, "account", "where-N-12.p1");
        assert_eq!((rt.state.cur, &rt.state.sel), (0, &crate::Sel::Agent("try-1".into())), "its page opens");
        assert!(settings_open(&rt), "with its Session settings open, its Account row in them");
        assert_eq!(restarts(&rt), 0, "nothing is sent for an account");
        assert!(!rt.state.where_open.contains("N-12.p1"), "the press is the words', not the line's: its chain stays shut");
        // on its page, the where line: "restart it"
        press(&mut rt, "restart", "");
        assert_eq!(restarts(&rt), 1, "Restart sent, as the Session panel's Restart sends it");
        assert!(settings_open(&rt), "its settings stay open, where coordd's answer is said");
        // on the Agents page, its tile: "pick an account for this agent"
        rt.update(|s, _| {
            s.sel = crate::Sel::Agents;
            s.streams.open.clear();
        });
        for _ in 0..4 {
            rt.frame(0.016);
        }
        press(&mut rt, "account", "tile-try-1");
        assert_eq!(rt.state.sel, crate::Sel::Agent("try-1".into()), "a press on them opens its page, not the tile's own press only");
        assert!(settings_open(&rt), "with its settings open");
        assert_eq!(restarts(&rt), 1, "and nothing more sent");
        let _ = child.kill();
    }

    /// Stills of a deaf session (made up) in `STILL_OUT`: his post, its tile, and its page with Account open.
    fn deaf_still(name: &str, init: fn() -> App) {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        std::fs::create_dir_all(&out).unwrap();
        let path = std::path::Path::new(&out).join(name).display().to_string();
        let args = vec!["--snap".to_string(), path, "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) }).args(args).try_run().unwrap();
    }

    fn deaf_app() -> App {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = proj(&[("lead", "idle", "2026-11-24T10:40:00Z"), ("try-1", "idle", "2026-11-24T10:58:00Z")]);
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        p.live.agents[1].deaf = Some("what was handed to its session never reached it (N-12.p1); restart it, or pick an account for this agent".into());
        p.live.agents[1].deaf_do = vec![coord_types::DeafDo::restart(), coord_types::DeafDo { act: coord_types::DEAF_ACCOUNT.into(), words: "pick an account for this agent".into() }];
        let posts = json!([{"n": 1, "at": "2026-11-24T11:02:00Z", "by": "owner", "text": "try-1, are you there? read the notes and tell me what you make of them", "names": ["try-1"]}]);
        let given = json!([{"to": "try-1", "delivery": "N-12.p1", "upto": 1, "at": "2026-11-24T11:02:05Z"}]);
        let mut v = thread(12, posts, given);
        v.title = "the notes".into();
        p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(v) });
        s.projects = vec![p];
        s.cur = 0;
        s
    }

    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn deaf_still_post() {
        fn init() -> App {
            let mut s = deaf_app();
            s.sel = crate::Sel::Tickets;
            s.tickets.shown = Some(12);
            s
        }
        deaf_still("deaf-1-post.png", init);
    }

    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn deaf_still_tile() {
        fn init() -> App {
            let mut s = deaf_app();
            s.sel = crate::Sel::Agents;
            s
        }
        deaf_still("deaf-2-tile.png", init);
    }

    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn deaf_still_page() {
        fn init() -> App {
            let mut s = deaf_app();
            crate::agents::deaf_act(&mut s, 0, "try-1", coord_types::DEAF_ACCOUNT);
            s
        }
        deaf_still("deaf-3-page.png", init);
    }

    /// Worked out once per snapshot: the same snapshot gives the kept entries; a new given line, or an agent's turn
    /// ending, works them out again.
    #[test]
    fn worked_out_once_per_snapshot() {
        let mut p = proj(&[("lead", "working", "2026-11-19T19:01:00Z")]);
        let mut v = thread(10, json!([{"n": 1, "at": "2026-11-19T19:02:00Z", "by": "owner", "text": "x", "names": ["lead"]}]), json!([]));
        put(&mut p, &v);
        assert!(matches!(of(&p, &v, &v.posts[0]).unwrap().entries[0].is, Is::Waiting { .. }));
        let key = (p.id.clone(), v.n);
        let first = KEPT.with(|k| k.borrow().get(&key).map(|(_, w)| Rc::as_ptr(w)));
        of(&p, &v, &v.posts[0]);
        assert_eq!(KEPT.with(|k| k.borrow().get(&key).map(|(_, w)| Rc::as_ptr(w))), first, "the same snapshot: kept");
        p.live.agents[0].state = "idle".into();
        assert!(matches!(of(&p, &v, &v.posts[0]).unwrap().entries[0].is, Is::Waiting { in_turn: None, .. }), "its turn ended: worked out again");
        v.given = serde_json::from_value(json!([{"to": "lead", "delivery": "N-10.p1", "upto": 1, "at": "2026-11-19T19:03:00Z"}])).unwrap();
        put(&mut p, &v);
        assert!(matches!(of(&p, &v, &v.posts[0]).unwrap().entries[0].is, Is::Sent { .. }));
    }

    /// The line follows the facts only: given `sent`, delivered at the fact's time, `handed twice · not read`.
    #[test]
    fn the_line_follows_sent_delivered_and_handed_twice_from_the_facts_alone() {
        let at = |t: &str| crate::when::at(t);
        let mut p = proj(&[("lead", "idle", "")]);
        let mut v = thread(11, json!([{"n": 1, "at": "2026-11-19T19:02:00Z", "by": "owner", "text": "x", "names": ["lead"]}]), json!([]));
        put(&mut p, &v);
        let line = |p: &Proj, v: &TicketView| of(p, v, &v.posts[0]).unwrap().entries[0].line();
        assert_eq!(line(&p, &v), "lead · waiting");
        // given at 19:03, in its queue
        v.given = serde_json::from_value(json!([{"to": "lead", "delivery": "N-11.p1", "upto": 1, "at": "2026-11-19T19:03:00Z"}])).unwrap();
        put(&mut p, &v);
        assert_eq!(line(&p, &v), format!("lead · sent {}", at("2026-11-19T19:03:00Z")));
        // the session took it in at 19:04:30 (docket-core5's delivered fact on the row)
        v.given = serde_json::from_value(json!([{"to": "lead", "delivery": "N-11.p1", "upto": 1, "at": "2026-11-19T19:03:00Z", "delivered": "2026-11-19T19:04:30Z"}])).unwrap();
        put(&mut p, &v);
        assert_eq!(line(&p, &v), format!("lead · delivered {}", at("2026-11-19T19:04:30Z")));
        assert_eq!(of(&p, &v, &v.posts[0]).unwrap().entries[0].chain, [format!("sent {}", at("2026-11-19T19:03:00Z")), format!("delivered {}", at("2026-11-19T19:04:30Z"))]);
        // handed twice, its text entered neither time
        v.given = serde_json::from_value(json!([
            {"to": "lead", "delivery": "N-11.p1", "upto": 1, "at": "2026-11-19T19:03:00Z", "unread": true},
            {"to": "lead", "delivery": "N-11.p1", "upto": 1, "at": "2026-11-19T19:05:00Z", "unread": true}
        ]))
        .unwrap();
        put(&mut p, &v);
        assert_eq!(line(&p, &v), "lead · handed twice · not read");
        assert_eq!(of(&p, &v, &v.posts[0]).unwrap().entries[0].chain, [format!("sent {}", at("2026-11-19T19:03:00Z")), format!("sent again {}", at("2026-11-19T19:05:00Z")), "handed twice · not read".to_string()]);
        // a later giving carries it and its text enters: delivered
        v.given.push(serde_json::from_value(json!({"to": "lead", "delivery": "N-11.p2", "upto": 2, "at": "2026-11-19T19:07:00Z", "delivered": "2026-11-19T19:07:01Z"})).unwrap());
        put(&mut p, &v);
        assert_eq!(line(&p, &v), format!("lead · delivered {}", at("2026-11-19T19:07:01Z")));
        // one row of each kind of fact, unread once: not twice
        v.given = serde_json::from_value(json!([{"to": "lead", "delivery": "N-11.p1", "upto": 1, "at": "2026-11-19T19:03:00Z", "unread": true}])).unwrap();
        put(&mut p, &v);
        assert_eq!(line(&p, &v), format!("lead · sent {}", at("2026-11-19T19:03:00Z")));
    }

    /// His post told late: a row whose mark passed it while held does not hand it; its own `told` row does.
    #[test]
    fn his_post_told_late_is_sent_by_its_own_row_not_by_one_that_passed_it() {
        let at = |t: &str| crate::when::at(t);
        let mut p = proj(&[("lead", "idle", "")]);
        let posts = json!([
            {"n": 1, "at": "2026-11-19T19:02:00Z", "by": "owner", "text": "the median looks off", "routed": true, "told": ["lead"]},
            {"n": 2, "at": "2026-11-19T19:02:10Z", "by": "owner", "text": "@lead and the ledger", "names": ["lead"]}
        ]);
        let mut v = thread(12, posts, json!([{"to": "lead", "delivery": "N-12.p2", "upto": 2, "at": "2026-11-19T19:03:00Z", "skipped": [1]}]));
        put(&mut p, &v);
        let line = |p: &Proj, v: &TicketView, i: usize| of(p, v, &v.posts[i]).unwrap().entries[0].line();
        assert_eq!(line(&p, &v, 0), "lead · waiting");
        assert_eq!(line(&p, &v, 1), format!("lead · sent {}", at("2026-11-19T19:03:00Z")), "the row hands its own part");
        v.given.push(serde_json::from_value(json!({"to": "lead", "delivery": "N-12.p1.told", "upto": 0, "at": "2026-11-19T19:04:00Z"})).unwrap());
        put(&mut p, &v);
        assert_eq!(line(&p, &v, 0), format!("lead · sent {}", at("2026-11-19T19:04:00Z")));
    }

    /// Stills of his post told late (made up): `lead · waiting`, then `lead · sent` at its own row.
    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn told_late_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        std::fs::create_dir_all(&out).unwrap();
        fn app(told: bool) -> App {
            let (_held, mut s) = crate::view_cost::test_app();
            let mut p = proj(&[("lead", "idle", ""), ("fixer", "idle", "")]);
            p.down = None;
            p.protocol = coord_types::PROTOCOL;
            let posts = json!([
                {"n": 1, "at": "2026-11-20T09:02:00Z", "by": "owner", "text": "the median on the dashboard looks off", "routed": true, "told": ["lead"]},
                {"n": 2, "at": "2026-11-20T09:02:10Z", "by": "owner", "text": "@lead @fixer and the ledger should mark p614 kept", "names": ["lead", "fixer"], "told": ["lead", "fixer"]}
            ]);
            let given = json!([
                {"to": "fixer", "delivery": "N-460.p2", "upto": 2, "at": "2026-11-20T09:02:20Z", "skipped": [1]},
                {"to": "lead", "delivery": "N-460.p2", "upto": 2, "at": "2026-11-20T09:02:20Z", "skipped": [1]}
            ]);
            let mut v = thread(460, posts, given);
            v.title = "the dashboard".into();
            p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(v.clone()) });
            // lead's own row comes as the thread's next event, as in the window (the entries are kept by the
            // project's ticket count, `KEPT`)
            if told {
                v.given.push(serde_json::from_value(json!({"to": "lead", "delivery": "N-460.p1.told", "upto": 0, "at": "2026-11-20T09:04:00Z"})).unwrap());
                p.live.apply(coord_types::Event { seq: 2, body: coord_types::Body::Ticket(v) });
            }
            s.projects = vec![p];
            s.cur = 0;
            s.sel = crate::Sel::Tickets;
            s.tickets.shown = Some(460);
            s
        }
        fn waiting() -> App {
            app(false)
        }
        fn sent() -> App {
            app(true)
        }
        for (name, setup) in [("told-late-waiting", waiting as fn() -> App), ("told-late-sent", sent)] {
            let path = std::path::Path::new(&out).join(format!("{name}.png")).display().to_string();
            let args = vec!["--snap".to_string(), path, "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
            rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), setup, crate::view) }).args(args).try_run().unwrap();
        }
    }

    /// The dock tests' window: lead in a turn, fixer idle, his post 1 on N-5 to both; SLOW holds lead's delivery.
    fn dock_app(s: &mut App, slow: bool) {
        let mut p = proj(&[("lead", "working", "2026-11-19T19:01:00Z"), ("fixer", "idle", "")]);
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        let mut v = thread(
            5,
            json!([{"n": 1, "at": "2026-11-19T19:02:00Z", "by": "owner", "text": "use the median for the dashboard", "names": ["lead", "fixer"]}]),
            json!([{"to": "fixer", "delivery": "N-5.p1", "upto": 1, "at": "2026-11-19T19:03:00Z"}]),
        );
        v.title = "the dashboard".into();
        if slow {
            v.slow = true;
            v.halts = serde_json::from_value(json!([{"what": "deliver", "n": 1, "at": "2026-11-19T19:02:01Z", "post": 1, "by": "owner", "wakes": ["lead"], "inform": [], "edited": false}])).unwrap();
        }
        put(&mut p, &v);
        s.projects = vec![p];
        s.cur = 0;
        s.spell.on = false;
        s.sel = crate::Sel::Agent("lead".into());
        crate::withyou::bind(s, 0, "lead", "N-5");
    }

    /// Lead's own box, its × pressed so it sends on no thread: a comment held for it, and a message sent while the pipe
    /// was down, not sent.
    fn unbound(s: &mut App) {
        crate::withyou::unbind(s, 0, "lead");
        s.composers.insert((0, "lead".into()), rune::TextEdit::new("and say which rows it drops"));
        s.send(0, "lead", false);
        let note = crate::ask::Comment { source: "lead@10".into(), label: "its message".into(), quote: "the mean of all rows".into(), note: "why not the median?".into(), at: Vec::new() };
        s.notes.held.insert((0, "lead".into()), vec![note]);
    }

    /// How many elements on RT's page have the words W.
    fn shown(rt: &rune::runtime::Runtime<App>, w: &str) -> usize {
        rt.elements().into_iter().filter(|e| e.text.as_deref() == Some(w)).count()
    }

    /// The stage a track keyed KEY is at.
    fn stage(rt: &rune::runtime::Runtime<App>, key: &str) -> Option<String> {
        crate::view_cost::words_at(rt, &format!("{key}-at"))
    }

    /// Esc pressed alone.
    fn esc() -> rune::KeyEvent {
        rune::KeyEvent { key: rune::Key::Escape, mods: rune::Mods::default() }
    }

    /// A thread box's dock: each agent's stage; Send now and Interrupt & Send as DeliverUrgent, or Esc from an older coordd.
    #[test]
    #[cfg(not(windows))]
    fn the_dock_over_a_threads_box_says_each_agents_stage_and_send_now_goes_in_two_stages() {
        use crate::view_cost::click;
        let (_held, mut s) = crate::view_cost::test_app();
        dock_app(&mut s, false);
        let (child, file) = crate::map::deep::tests::piped(&mut s, 0, "dock-thread");
        let mut rt = crate::view_cost::window(s);
        rt.frame(0.016);
        assert_eq!(stage(&rt, "dock-track-N-5.p1-lead").as_deref(), Some("Queued (turn)"));
        assert_eq!(stage(&rt, "dock-track-N-5.p1-fixer").as_deref(), Some("Queued (tool)"));
        assert!(rt.find("step-run-N-5").is_some(), "the thread's Step (Manual) | Run (Auto), in the dock");
        assert!(rt.find("where-N-5.p1").is_none(), "each agent's stage is the dock's alone, not under the post too");
        let words = rt.elements().into_iter().filter(|e| e.text.as_deref().is_some_and(|t| t.contains("use the median"))).count();
        assert!(words <= 1, "his post's words are the thread's, not the dock's: {words}");
        click(&mut rt, "dock-send-now-N-5.p1-lead");
        assert!(rt.find("dock-send-now-N-5.p1-lead").is_none(), "its second stage in its place");
        click(&mut rt, "dock-interrupt-N-5.p1-lead");
        assert!(rt.state.outgoing.armed.is_none() && rt.find("dock-send-now-N-5.p1-lead").is_some());
        click(&mut rt, "dock-send-now-N-5.p1-lead");
        let target = crate::message_box::Target::Thread { project: 0, t: "N-5".into(), post: None, way: crate::message_box::Way::Here };
        assert!(crate::outgoing::esc_key(&mut rt.state, &target, &esc()));
        assert!(rt.state.outgoing.armed.is_none());
        assert!(!crate::outgoing::esc_key(&mut rt.state, &target, &esc()), "nothing armed: Esc is not a thread box's");
        // a coordd before the control: Send now sends nothing, its second stage is Esc
        rt.state.projects[0].protocol = coord_types::CONTROL_PROTOCOL - 1;
        rt.invalidate();
        rt.frame(0.016);
        click(&mut rt, "dock-send-now-N-5.p1-lead");
        click(&mut rt, "dock-interrupt-N-5.p1-lead");
        rt.state.projects[0].link = None;
        let asked: Vec<serde_json::Value> = crate::map::deep::tests::sent(child, &file).into_iter().map(|v| v["req"].clone()).filter(|r| r["op"] == "esc" || r["op"] == "deliverurgent").collect();
        let urgent = |interrupt: bool| json!({"op": "deliverurgent", "agent": "lead", "interrupt": interrupt});
        assert_eq!(asked, [urgent(false), urgent(true), urgent(false), urgent(true), json!({"op": "esc", "to": ["lead"]})]);
    }

    /// An agent box's dock: staged and failed kept, its pace; Esc sends then DeliverUrgent, or stops alone; Step keeps it shown.
    #[test]
    #[cfg(not(windows))]
    fn the_dock_over_an_agents_box_keeps_what_is_staged_and_not_sent_and_esc_interrupts_and_sends() {
        let (_held, mut s) = crate::view_cost::test_app();
        dock_app(&mut s, false);
        unbound(&mut s);
        let mut rt = crate::view_cost::window(s);
        rt.frame(0.016);
        assert_eq!(stage(&rt, "dock-track-staged-lead").as_deref(), Some("Staged"));
        let out = rt.state.outgoing.list.first().map(|o| o.id).expect("his message not sent is kept");
        assert_eq!(stage(&rt, &format!("dock-track-out-{out}")).as_deref(), Some("Failed"));
        assert!(rt.find("step-run-N-5").is_none(), "no thread: no thread's slow mode");
        assert!(rt.find("pace-lead").is_some(), "lead's pace");
        assert_eq!(shown(&rt, "Interrupt & Send (Esc)"), 1, "the comment held is something to send");
        let (child, file) = crate::map::deep::tests::piped(&mut rt.state, 0, "dock-agent");
        let target = crate::message_box::Target::Agent { project: 0, name: "lead".into() };
        rt.state.composers.insert((0, "lead".into()), rune::TextEdit::new("now the mode"));
        assert!(crate::outgoing::esc_key(&mut rt.state, &target, &esc()));
        assert!(!crate::outgoing::written(&rt.state, 0, "lead"), "sent, its comment with it");
        rt.invalidate();
        rt.frame(0.016);
        assert_eq!(shown(&rt, "Stop (Esc)"), 1, "nothing in the box: Stop (Esc)");
        assert!(crate::outgoing::esc_key(&mut rt.state, &target, &esc()));
        rt.state.projects[0].live.agents[0].state = "idle".into();
        assert!(!crate::outgoing::esc_key(&mut rt.state, &target, &esc()), "lead idle: Esc is not the box's");
        rt.state.projects[0].link = None;
        let asked: Vec<serde_json::Value> = crate::map::deep::tests::sent(child, &file).into_iter().map(|v| v["req"].clone()).collect();
        let at = |op: &str| asked.iter().enumerate().filter(|(_, r)| r["op"] == op).map(|(i, _)| i).collect::<Vec<usize>>();
        let (urgent, escs) = (at("deliverurgent"), at("esc"));
        assert_eq!(urgent.iter().map(|i| &asked[*i]).collect::<Vec<_>>(), [&json!({"op": "deliverurgent", "agent": "lead", "interrupt": true})], "{asked:?}");
        assert_eq!(escs.iter().map(|i| &asked[*i]).collect::<Vec<_>>(), [&json!({"op": "esc", "to": ["lead"]})], "Stop (Esc) alone: {asked:?}");
        let said = asked.iter().position(|r| r.to_string().contains("now the mode")).expect("sent");
        assert!(said < urgent[0], "the message goes before it is handed at once: {asked:?}");
        // nothing on its way: no dock in Run, one in Step (the tests' lock let go before it is taken again)
        drop(rt);
        drop(_held);
        let (_held, mut s) = crate::view_cost::test_app();
        dock_app(&mut s, false);
        crate::withyou::unbind(&mut s, 0, "lead");
        s.projects[0].live.agents[0].state = "idle".into();
        let mut rt = crate::view_cost::window(s);
        rt.frame(0.016);
        assert!(rt.find("pace-lead").is_none(), "nothing on its way, lead in Run: no dock");
        rt.state.projects[0].live.agents[0].control = coord_types::AgentMode::Step;
        rt.invalidate();
        rt.frame(0.016);
        assert!(rt.find("pace-lead").is_some(), "lead in Step: its pace in sight");
    }

    /// A post the engine has with nobody given it is the dock's, at Sending; one with no delivery says nothing.
    #[test]
    fn the_dock_follows_a_post_only_while_its_whereabouts_say_something() {
        let p = proj(&[("lead", "idle", "2026-11-24T10:40:00Z")]);
        let v = thread(12, json!([{"n": 1, "at": "2026-11-24T11:02:00Z", "by": "owner", "text": "the notes"}]), json!([]));
        assert!(crate::outgoing::follows(&p, &v, &v.posts[0]), "nobody given it yet: with the engine");
        let v = thread(12, json!([{"n": 1, "at": "2026-11-24T11:02:00Z", "by": "owner", "text": "the notes", "names": ["lead"]}]), json!([]));
        assert!(!crate::outgoing::follows(&p, &v, &v.posts[0]), "named, and no delivery recorded: nothing to say");
    }

    /// Stop (Esc) → Step is one Esc with `step` (older coordd: Esc and approve); with lead idle, Step approves the delivery.
    #[test]
    #[cfg(not(windows))]
    fn stop_then_step_stops_the_turn_and_lets_the_held_delivery_go_in_one_press() {
        // the halt card's head ends in its ⓘ
        {
            let (_held, mut s) = crate::view_cost::test_app();
            dock_app(&mut s, true);
            let mut rt = crate::view_cost::window(s);
            for _ in 0..4 {
                rt.frame(0.016);
            }
            let info = rt.elements().into_iter().filter_map(|e| e.key).find(|k| k.starts_with("behind-halt-N-5-")).expect("the halt card's ⓘ");
            crate::view_cost::click(&mut rt, &info);
            assert_eq!(rt.state.popups.last().map(|p| &p.what), Some(&crate::popup::What::Behind { n: 5, on: 5 }));
        }
        for older in [false, true] {
            let (_held, mut s) = crate::view_cost::test_app();
            dock_app(&mut s, true);
            if older {
                s.projects[0].protocol = coord_types::CONTROL_PROTOCOL - 1;
            }
            let (child, file) = crate::map::deep::tests::piped(&mut s, 0, "dock-step");
            let mut rt = crate::view_cost::window(s);
            rt.frame(0.016);
            assert_eq!(shown(&rt, "Stop (Esc) → Step"), 1);
            crate::view_cost::click(&mut rt, "dock-step-thread:N-5");
            rt.state.projects[0].live.agents[0].state = "idle".into();
            rt.invalidate();
            rt.frame(0.016);
            assert_eq!(shown(&rt, "Stop (Esc) → Step"), 0);
            assert!(rt.find("dock-step-thread:N-5").is_some(), "Step alone");
            if !older {
                crate::view_cost::click(&mut rt, "dock-step-thread:N-5");
            }
            rt.state.projects[0].link = None;
            let asked: Vec<serde_json::Value> = crate::map::deep::tests::sent(child, &file).into_iter().map(|v| v["req"].clone()).filter(|r| r["op"] == "esc" || r["op"] == "approve").collect();
            let esc = if older { json!({"op": "esc", "to": ["lead"]}) } else { json!({"op": "esc", "to": ["lead"], "step": true}) };
            assert_eq!(asked, [esc, json!({"op": "approve", "t": "N-5", "n": 1})], "older: {older}");
        }
    }

    /// The dock as the tests above make it: over N-5's box on lead's page in slow mode (dock-thread.png), its Send now
    /// pressed (dock-armed.png), and over lead's own box with a comment staged and a message not sent (dock-agent.png).
    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn dock_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        std::fs::create_dir_all(&out).unwrap();
        fn on_thread() -> App {
            let (_held, mut s) = crate::view_cost::test_app();
            dock_app(&mut s, true);
            s
        }
        fn armed() -> App {
            let (_held, mut s) = crate::view_cost::test_app();
            dock_app(&mut s, false);
            s.outgoing.armed = Some((0, "N-5.p1".into(), "lead".into()));
            s
        }
        fn agent_box() -> App {
            let (_held, mut s) = crate::view_cost::test_app();
            dock_app(&mut s, false);
            unbound(&mut s);
            s
        }
        for (name, setup) in [("dock-thread", on_thread as fn() -> App), ("dock-armed", armed), ("dock-agent", agent_box)] {
            let path = std::path::Path::new(&out).join(format!("{name}.png")).display().to_string();
            let args = vec!["--snap".to_string(), path, "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
            rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), setup, crate::view) }).args(args).try_run().unwrap();
        }
    }

    /// His post left unanswered twice: owed, `delivered · not answered` from its first entry, until an answer.
    #[test]
    fn a_post_left_unanswered_twice_reads_delivered_not_answered_until_answered() {
        let at = |t: &str| crate::when::at(t);
        let mut p = proj(&[("lead", "idle", "")]);
        let mut v = thread(12, json!([{"n": 1, "at": "2026-11-19T19:02:00Z", "by": "owner", "text": "x", "names": ["lead"]}]), json!([
            {"to": "lead", "delivery": "N-12.p1", "upto": 1, "at": "2026-11-19T19:03:00Z", "delivered": "2026-11-19T19:03:01Z"},
            {"to": "lead", "delivery": "N-12.p1", "upto": 1, "at": "2026-11-19T19:06:00Z", "delivered": "2026-11-19T19:06:01Z", "owed": true}
        ]));
        v.quiet = serde_json::from_value(json!([{"agent": "lead", "upto": 1}])).unwrap();
        put(&mut p, &v);
        let w = of(&p, &v, &v.posts[0]).unwrap();
        assert_eq!(w.entries[0].line(), format!("lead · delivered {} · not answered", at("2026-11-19T19:03:01Z")));
        assert_eq!(w.entries[0].chain.last().map(String::as_str), Some("not answered"));
        assert!(!w.settled());
        v.posts.push(serde_json::from_value(json!({"n": 2, "at": "2026-11-19T19:09:00Z", "by": "lead", "text": "done", "answers": 1})).unwrap());
        put(&mut p, &v);
        assert_eq!(of(&p, &v, &v.posts[0]).unwrap().entries[0].line(), "lead · answered in #2");
    }

    /// The handler of O-9 holding his post, waiting on his `gather`, with coordd's LAPSES_AT on its view.
    fn gathering(lapses_at: Option<&str>) -> TicketView {
        serde_json::from_value(json!({"id": "O-9", "n": 9, "kind": "owner_post", "opener": "coordd", "ctx": {}, "form": {}, "title": "t",
            "status": {"is": "awaits", "agent": "owner", "call": "gather", "step": 1, "since": "2026-11-21T09:30:00Z", "taken": false, "refusals": 0, "forming": false, "head": {"Variant": [["Send", []]]}, "args": {"text": "x"}},
            "children": [], "program": "", "bound": 5, "spent": 1, "opened_at": "2026-11-21T09:30:00Z", "given": [], "posts": [], "lapses_at": lapses_at}))
        .unwrap()
    }

    /// His post held with his burst says when it goes and has Send now; on a slowed thread, held and the press only.
    #[test]
    fn held_with_his_burst_says_when_it_goes_beside_send_now() {
        let mut p = proj(&[("lead", "idle", "")]);
        let mut h = thread(8, json!([{"n": 1, "at": "2026-11-21T09:30:00Z", "by": "owner", "text": "x", "names": ["lead"], "held": "O-9"}]), json!([]));
        h.handlers = vec![gathering(Some("2026-11-21T09:30:15Z"))];
        put(&mut p, &h);
        assert_eq!(of(&p, &h, &h.posts[0]).unwrap().engine, Engine::Gathered { handler: "O-9".into(), goes: Some("2026-11-21T09:30:15Z".into()) });
        assert_eq!(held_goes(Some("2026-11-21T09:30:15Z")), format!("held · goes at {}", crate::when::exact("2026-11-21T09:30:15Z")));
        h.handlers = vec![gathering(None)];
        put(&mut p, &h);
        assert_eq!(of(&p, &h, &h.posts[0]).unwrap().engine, Engine::Gathered { handler: "O-9".into(), goes: None });
        assert_eq!(held_goes(None), "held");
    }

    /// Made-up thread N-1480 whose opening words O-9 holds at its `gather`, going at 09:30:15Z.
    fn held_opener() -> App {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = proj(&[("lead", "idle", "2026-11-21T09:00:00Z")]);
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        let mut v = thread(1480, json!([{"n": 1, "at": "2026-11-21T09:30:00Z", "by": "owner", "text": "the lock on the runner fails again: look at it before the build", "names": ["lead"], "held": "O-9"}]), json!([]));
        v.title = "the lock on the runner fails again".into();
        v.handlers = vec![gathering(Some("2026-11-21T09:30:15Z"))];
        p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(v) });
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = Some(1480);
        s
    }

    /// A held opener's mark under it: when it goes, and Send now.
    #[test]
    fn the_thread_draws_his_held_opener_with_send_now() {
        let mut rt = crate::view_cost::window(held_opener());
        rt.invalidate();
        for _ in 0..3 {
            rt.frame(0.016);
        }
        let els = rt.elements();
        let goes = format!("held · goes at {}", crate::when::exact("2026-11-21T09:30:15Z"));
        assert!(els.iter().any(|e| e.text.as_deref() == Some(goes.as_str())), "{:?}", els.iter().filter_map(|e| e.text.clone()).collect::<Vec<_>>());
        assert_eq!(els.iter().filter(|e| e.role == "button" && (e.text.as_deref() == Some("Send now") || e.label.as_deref() == Some("Send now"))).count(), 1);
        // before intake (thread.kind's `gather` first, no post yet): the thread's own `gather` step and `lapses_at`
        let mut s = held_opener();
        let g = gathering(Some("2026-11-21T09:30:15Z"));
        let v = s.projects[0].live.tickets.get_mut(&1480).unwrap();
        v.posts.clear();
        v.handlers.clear();
        v.form = json!({"to": "lead", "text": "the lock on the runner fails again\nand the window build too"});
        v.status = g.status;
        v.lapses_at = g.lapses_at;
        let mut rt = crate::view_cost::window(s);
        rt.invalidate();
        for _ in 0..3 {
            rt.frame(0.016);
        }
        let els = rt.elements();
        assert!(els.iter().any(|e| e.text.as_deref() == Some("the lock on the runner fails again\nand the window build too")), "{:?}", els.iter().filter_map(|e| e.text.clone()).collect::<Vec<_>>());
        assert!(els.iter().any(|e| e.text.as_deref() == Some(goes.as_str())), "{:?}", els.iter().filter_map(|e| e.text.clone()).collect::<Vec<_>>());
        assert_eq!(els.iter().filter(|e| e.role == "button" && (e.text.as_deref() == Some("Send now") || e.label.as_deref() == Some("Send now"))).count(), 1);
    }

    /// The still (STILL_OUT): `held_opener`, the thread's view.
    /// `STILL_OUT=DIR yaaihh-tests whereabouts::tests::held_opener_still --ignored --exact`.
    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn held_opener_still() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        std::fs::create_dir_all(&out).unwrap();
        let path = std::path::Path::new(&out).join("held-opener.png").display().to_string();
        let args = vec!["--snap".to_string(), path, "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), held_opener, crate::view) }).args(args).try_run().unwrap();
    }
}
