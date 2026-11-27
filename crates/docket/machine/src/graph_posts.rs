//! The graph's edges from posts and the delivery lines (paper-research N-1446 p1337, p1376 piece 3: docket-edge8's part):
//! what each of these record lines says happened, as edges of [`crate::graph::Graph`], each at the line it comes from.
//! Called by `graph::Builder::after` for every line of a group, as `graph_runs::add` is; a node already there is the
//! same node.
//!
//! - answered: his answers to the asks of a post (`post`'s `answers`): his post → that post; a post that answers
//!   messages on other threads too (`answers_also`, the reply tool's `to`): the post → each of them, a post or a
//!   ticket; a post that answers whom it names without asking them (`post`'s and `replied`'s `answering`, the rule
//!   Reply's own name): the post → each of them.
//! - answered_upto: a post (`post`, the rule Reply's `replied`) → the last post of another's it answers, as the
//!   machine counts it (`Ticket::answers`: up to what its writer had been handed, less what was kept from it), apart
//!   from the reply tool's target (paper-research N-1446 p1409, 1).
//! - woke: a post or an opening post that asks agents (`post`'s and `open`'s `names`, less the `answering` it only
//!   answers, and the `told` policy's notify adds): the post → each; the rule Reply's post (`replied`'s `names` less
//!   `answering`); a policy notice (`notified`'s `to`): its post, or its ticket, → each. His post left to a handler
//!   ([`to_handler`]) wakes as the handler decided, once: by its `told`'s `to`, or by `released` as policy said (its
//!   names and told); its `post` line wakes nobody, and one the handler dropped wakes nobody (p1409). A post halted for
//!   his approval on a slow ticket (`Post::halted`) reaches nobody until he lets it go: its `post`, `replied` or `told`
//!   line wakes and informs nobody, and the line that lets it go (his `approve`, slow switched off over it) wakes and
//!   informs whom the machine's `approve` delivers it to, as he left them, once ([`Delivering`]; p1419, 1). A post an
//!   input made with no `post` line (`posted`: a program's, by its ticket; an answer; his words that formed none, or
//!   that a program placed) wakes whom the machine's post names and policy told, at its line, as a `post` does: his
//!   left to a handler, one halted, and one a handler routed (its `told` wakes) wake nobody there (p1439).
//! - informed: those a post (`post`, `posted`), a handler's tell or release, or a notice informs (`inform`), who see
//!   it at their next turn and whom it does not wake: the post, or the notice's ticket, → each.
//! - handed, took: a session took a delivery (`given`). A `given` line hands what the giving newly gives
//!   ([`Delivering`]): each post it brought (`Ticket::brought`) → the agent, once. A line for a giving already here is the
//!   same delivery seen again (its place found in the session, or given further after a restart): it hands only what
//!   it gives further. A giving made anew after the machine made its delivery due again (`GivenRec::again`: a turn's
//!   end or cut left his post unanswered, or it was put back unread) is a new one, and hands again: each of its edges
//!   says which line made the delivery due again (`Edge::again`; p1449). Where it landed (`where`, as the machine
//!   holds it) is the slice of the session, a node of its own, `slice:AGENT:DELIVERY@LINE`, which the agent took
//!   (p1409: handed, then landed). The node is the giving, not its bytes: coordd reads them, asked by the delivery and
//!   never by a place (paper-research p1380). A program's `gave`: the ticket → the agent.
//!
//! A post meant for him is one whose `names`, `told` or `answering` hold `owner`, as the record writes it: its edge
//! goes to `agent:owner` ([`crate::graph::way`] reads it as agent → owner). Not here: named, which is coordd's Mentions
//! index, not a record line; and a post's writer (`wrote`, `graph::Builder`): its agent, him, or a program's ticket.

use crate::graph::{EdgeKind, Graph, NodeKind, agent_id, post_id, slice_id};
use crate::line::Line;
use crate::machine::Machine;
use std::collections::{BTreeMap, BTreeSet};

/// What the graph keeps of the deliveries from line to line (`graph::Builder`).
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct Delivering {
    /// What each giving has handed in the graph, by its ticket, agent and delivery: the giving's index among the
    /// ticket's (`Ticket::given`, which only grows) and the post it was handed up to. A giving of the same delivery
    /// after it was made due again is a new one, at a new index (`GivenRec::again`).
    givings: BTreeMap<(u64, String, String), (usize, u32)>,
    /// The posts halted for his approval (`Post::halted`) that no line has let go yet, by ticket and number.
    halted: BTreeSet<(u64, u32)>,
    /// Each agent's givings here not made due again yet: their ticket, delivery and index (`Ticket::given`).
    open: BTreeMap<String, Vec<(u64, String, usize)>>,
    /// The line that made a giving due again, by its ticket, agent and delivery, until that delivery is given anew.
    again: BTreeMap<(u64, String, String), usize>,
}

/// After LINE, a line that may make givings due again (a turn's end or cut, `unread`, `due_again`, an era's): each of
/// AGENT's open givings (every agent's, for None) the machine now holds made due again (`GivenRec::again`) is made so
/// at LINE, for the next giving of its delivery (p1449), and is open no more.
fn made_again(d: &mut Delivering, m: &Machine, agent: Option<&str>, line: usize) {
    let Delivering { open, again, .. } = d;
    for (a, rows) in open.iter_mut().filter(|(a, _)| agent.is_none_or(|x| x == a.as_str())) {
        rows.retain(|(t, delivery, i)| match m.tickets.get(t).and_then(|x| x.given.get(*i)) {
            Some(g) if g.again() => {
                again.insert((*t, a.clone(), delivery.clone()), line);
                false
            }
            Some(_) => true,
            None => false,
        });
    }
}

/// Whether post N of T is halted for his approval once the line is replayed (slow mode, `Post::halted`): it then
/// reaches nobody until a line lets it go, and D keeps it till then.
fn halted_now(d: &mut Delivering, m: &Machine, t: u64, n: u32) -> bool {
    let halted = m.tickets.get(&t).and_then(|x| x.posts.iter().find(|p| p.n == n)).is_some_and(|p| p.halted);
    if halted {
        d.halted.insert((t, n));
    }
    halted
}

/// Whether his post N of T is left to a handler: one runs it (`owner_post`), or it joined a burst one gathers, or this
/// release does not load its handler and no line has told it yet (`Machine::untold`). Its wake is then the handler's.
fn to_handler(m: &Machine, t: u64, n: u32) -> bool {
    m.untold.contains(&(t, n)) || m.tickets.values().any(|u| u.post_of.as_ref().is_some_and(|o| o.t == t && o.holds(n)))
}

/// The last post of another's before post N of T that N answers, as the machine counts it ([`crate::machine::Ticket::answers`]).
fn last_answered(m: &Machine, t: u64, n: u32) -> Option<u32> {
    let x = m.tickets.get(&t)?;
    let p = x.posts.iter().find(|p| p.n == n)?;
    x.posts.iter().rev().find(|q| q.n < n && q.by != p.by && x.answers(p, q.n)).map(|q| q.n)
}

pub(crate) fn add(g: &mut Graph, d: &mut Delivering, m: &Machine, line: usize, l: &Line) {
    let to_each = |g: &mut Graph, from: &str, who: &[String], kind: EdgeKind| {
        for a in who {
            let a = g.node(agent_id(a), NodeKind::Agent, line);
            g.edge(from, &a, kind, line);
        }
    };
    match l {
        Line::Post { t, by, names, told, inform, answers, answering, .. } => {
            // the post this line made: its writer's last on T once the group is replayed (as `graph_runs`' read)
            let Some(n) = m.tickets.get(t).and_then(|y| y.posts.iter().rev().find(|p| p.by == *by)).map(|p| p.n) else { return };
            let pid = g.node(post_id(m, *t, n), NodeKind::Post, line);
            if let Some(k) = answers {
                let q = g.node(post_id(m, *t, *k), NodeKind::Post, line);
                g.edge(&pid, &q, EdgeKind::Answered, line);
            }
            if let Some(k) = last_answered(m, *t, n) {
                let q = g.node(post_id(m, *t, k), NodeKind::Post, line);
                g.edge(&pid, &q, EdgeKind::AnsweredUpto, line);
            }
            if !(by == "owner" && to_handler(m, *t, n)) && !halted_now(d, m, *t, n) {
                let woke: Vec<String> = names.iter().filter(|a| !answering.contains(a)).chain(told.iter().filter(|a| !names.contains(a))).cloned().collect();
                to_each(g, &pid, &woke, EdgeKind::Woke);
                to_each(g, &pid, inform, EdgeKind::Informed);
            }
            to_each(g, &pid, answering, EdgeKind::Answered);
        }
        // a post an input made with no `post` line: it reaches whom the machine's post names and policy told, and whom
        // it informs, as a `post` does (paper-research N-1446 p1439)
        Line::Posted { t, n, by, .. } => {
            let Some(p) = m.tickets.get(t).and_then(|x| x.posts.iter().find(|p| p.n == *n)) else { return };
            if (by == "owner" && to_handler(m, *t, *n)) || p.routed || halted_now(d, m, *t, *n) {
                return;
            }
            let pid = g.node(post_id(m, *t, *n), NodeKind::Post, line);
            let woke: Vec<String> = p.names.iter().chain(p.told.iter().filter(|a| !p.names.contains(a))).cloned().collect();
            to_each(g, &pid, &woke, EdgeKind::Woke);
            to_each(g, &pid, &p.inform, EdgeKind::Informed);
        }
        // a ticket opened with words: its first post, its opener's
        Line::Open { t, opener, text: Some(_), names, told, .. } => {
            let Some(n) = m.tickets.get(t).and_then(|y| y.posts.iter().find(|p| p.by == *opener)).map(|p| p.n) else { return };
            let pid = g.node(post_id(m, *t, n), NodeKind::Post, line);
            if !(opener == "owner" && to_handler(m, *t, n)) {
                let woke: Vec<String> = names.iter().chain(told.iter().filter(|a| !names.contains(a))).cloned().collect();
                to_each(g, &pid, &woke, EdgeKind::Woke);
            }
        }
        // his post a handler ended with, neither told nor dropped: it goes as policy said, its names and told
        // (`Machine::release`)
        Line::Released { t, n, .. } => {
            let Some(p) = m.tickets.get(t).and_then(|x| x.posts.iter().find(|p| p.n == *n)) else { return };
            let pid = g.node(post_id(m, *t, *n), NodeKind::Post, line);
            let woke: Vec<String> = p.names.iter().chain(p.told.iter().filter(|a| !p.names.contains(a))).cloned().collect();
            to_each(g, &pid, &woke, EdgeKind::Woke);
            to_each(g, &pid, &p.inform, EdgeKind::Informed);
        }
        Line::AnswersAlso { t, n, also, .. } => {
            let pid = g.node(post_id(m, *t, *n), NodeKind::Post, line);
            for x in also {
                let Some((tk, k)) = crate::post_or_ticket(x) else { continue };
                let to = match k {
                    Some(k) => g.node(format!("{tk}.p{k}"), NodeKind::Post, line),
                    None => g.node(tk, NodeKind::Ticket, line),
                };
                g.edge(&pid, &to, EdgeKind::Answered, line);
            }
        }
        Line::Told { t, n, to, inform, .. } => {
            let pid = g.node(post_id(m, *t, *n), NodeKind::Post, line);
            if !halted_now(d, m, *t, *n) {
                to_each(g, &pid, to, EdgeKind::Woke);
                to_each(g, &pid, inform, EdgeKind::Informed);
            }
        }
        Line::Replied { t, n, names, answering, .. } => {
            let pid = g.node(post_id(m, *t, *n), NodeKind::Post, line);
            let woke: Vec<String> = names.iter().filter(|a| !answering.contains(a)).cloned().collect();
            if !halted_now(d, m, *t, *n) {
                to_each(g, &pid, &woke, EdgeKind::Woke);
            }
            to_each(g, &pid, answering, EdgeKind::Answered);
            // and what its turn held, as the machine counts it answered: ask_times reads it as taking those before it
            if let Some(k) = last_answered(m, *t, *n) {
                let q = g.node(post_id(m, *t, k), NodeKind::Post, line);
                g.edge(&pid, &q, EdgeKind::AnsweredUpto, line);
            }
        }
        Line::Notified { t, to, inform, post, .. } => {
            let from = match post {
                Some(k) => g.node(post_id(m, *t, *k), NodeKind::Post, line),
                None => g.node(m.id(*t), NodeKind::Ticket, line),
            };
            to_each(g, &from, to, EdgeKind::Woke);
            to_each(g, &from, inform, EdgeKind::Informed);
        }
        Line::Given { t, to, delivery, place, .. } => {
            // the giving as the machine holds it once the line is replayed: its last of DELIVERY to TO
            let Some(x) = m.tickets.get(t) else { return };
            let Some(i) = x.given.iter().rposition(|r| r.to == *to && r.delivery == *delivery) else { return };
            let given = &x.given[i];
            // what this line hands: the whole giving when it is new here, else only what it gives further; a new giving
            // of a delivery made due again, with the line that made it so (p1449)
            let key = (*t, to.clone(), delivery.clone());
            let (from, again) = match d.givings.insert(key.clone(), (i, given.upto)) {
                Some((j, upto)) if j == i => (upto, None),
                had => {
                    d.open.entry(to.clone()).or_default().push((*t, delivery.clone(), i));
                    (0, had.and_then(|_| d.again.remove(&key)))
                }
            };
            let a = g.node(agent_id(to), NodeKind::Agent, line);
            for p in x.brought(given).filter(|p| p.n > from) {
                let pid = g.node(post_id(m, *t, p.n), NodeKind::Post, line);
                g.edge(&pid, &a, EdgeKind::Handed, line);
                if let Some(e) = g.edges.last_mut() {
                    e.again = again;
                }
            }
            // where it landed in the session, as the machine holds it: the slice, which the agent took
            if place.is_some() && given.place == *place {
                let s = g.node(slice_id(to, delivery, line), NodeKind::Slice, line);
                g.edge(&a, &s, EdgeKind::Took, line);
            }
        }
        // his approval, or slow switched off over what waits (`approve`, each card in the order it halted): each post
        // halted until now reaches whom it wakes as he left them at his card (`Post::wakes`, less those it only
        // answers) and whom it informs, once; one dropped or taken back meanwhile reaches nobody
        Line::Approve { .. } | Line::HaltReject { .. } | Line::Slow { on: false, .. } | Line::Slowed { on: false, .. } => {
            let gone: Vec<(u64, u32)> = d.halted.iter().copied().filter(|(t, n)| !m.tickets.get(t).and_then(|x| x.posts.iter().find(|p| p.n == *n)).is_some_and(|p| p.halted)).collect();
            for (t, n) in gone {
                d.halted.remove(&(t, n));
                let Some(p) = m.tickets.get(&t).and_then(|x| x.posts.iter().find(|p| p.n == n)) else { continue };
                if p.dropped || p.taken_back {
                    continue;
                }
                let mut woke: Vec<String> = Vec::new();
                for a in p.wakes().filter(|a| p.routed || !p.answering.contains(a)) {
                    if !woke.contains(a) {
                        woke.push(a.clone());
                    }
                }
                let pid = g.node(post_id(m, t, n), NodeKind::Post, line);
                to_each(g, &pid, &woke, EdgeKind::Woke);
                to_each(g, &pid, &p.inform, EdgeKind::Informed);
            }
        }
        // what may make givings due again: which line made each so (p1449)
        Line::TurnEnded { agent, .. } | Line::TurnCut { agent, .. } | Line::Unread { agent, .. } => made_again(d, m, Some(agent.as_str()), line),
        Line::Event { name, value, .. } if name == crate::machine::DUE_AGAIN => {
            if let Some(a) = value.get("agent").and_then(|v| v.as_str()) {
                made_again(d, m, Some(a), line);
            }
        }
        Line::Event { name, .. } if name == crate::machine::ERA => made_again(d, m, None, line),
        Line::Gave { t, to, .. } => {
            let tid = g.node(m.id(*t), NodeKind::Ticket, line);
            let a = g.node(agent_id(to), NodeKind::Agent, line);
            g.edge(&tid, &a, EdgeKind::Handed, line);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use crate::graph::{EdgeKind, Graph, slice_id, slice_of};
    use crate::line::Line;
    use crate::machine::Machine;
    use serde_json::json;

    /// The delivery lines as edges, each at its line: an answer to other threads, a handler's tell and whom it informs,
    /// a notice, a program's give. Made up.
    #[test]
    fn the_delivery_lines_give_their_edges() {
        let m = Machine::load(Default::default());
        let mut g = Graph::default();
        let mut d = super::Delivering::default();
        let l = |v: serde_json::Value| -> Line { serde_json::from_value(v).unwrap() };
        let at = "2026-11-22T08:00:00Z";
        super::add(&mut g, &mut d, &m, 1, &l(json!({"ev": "answers_also", "at": at, "t": 7, "n": 2, "also": ["N-355.p1", "Q-56"]})));
        super::add(&mut g, &mut d, &m, 2, &l(json!({"ev": "told", "at": at, "t": 7, "n": 3, "to": ["pr"], "inform": ["w1"]})));
        super::add(&mut g, &mut d, &m, 3, &l(json!({"ev": "notified", "at": at, "t": 7, "event": "closed", "to": ["w1"], "provenance": {}})));
        super::add(&mut g, &mut d, &m, 5, &l(json!({"ev": "gave", "at": at, "t": 7, "to": "w1", "n": 1})));
        let e: Vec<(&str, &str, EdgeKind, usize)> = g.edges.iter().map(|e| (e.from.as_str(), e.to.as_str(), e.kind, e.line().unwrap())).collect();
        let id = m.id(7);
        let p = |n: u32| format!("{id}.p{n}");
        assert_eq!(e, vec![
            (p(2).as_str(), "N-355.p1", EdgeKind::Answered, 1),
            (p(2).as_str(), "Q-56", EdgeKind::Answered, 1),
            (p(3).as_str(), "agent:pr", EdgeKind::Woke, 2),
            (p(3).as_str(), "agent:w1", EdgeKind::Informed, 2),
            (id.as_str(), "agent:w1", EdgeKind::Woke, 3),
            (id.as_str(), "agent:w1", EdgeKind::Handed, 5),
        ]);
    }

    /// A slice's id read back: its agent, its delivery and its line, and nothing from any other id. Made up.
    #[test]
    fn a_slices_id_reads_back() {
        assert_eq!(slice_of(&slice_id("fixer", "N-3.p2.told", 41)), Some(("fixer", "N-3.p2.told", 41)));
        assert_eq!(slice_of(&slice_id("cav/w1", "A-9.s1", 7)), Some(("cav/w1", "A-9.s1", 7)));
        for id in ["agent:fixer", "N-3.p2", "slice:fixer:N-3.p2", "slice:fixer:N-3.p2@x", "slice::N-3.p2@4", "slice:fixer:@4", "slice:fixer:N:3@4"] {
            assert_eq!(slice_of(id), None, "{id}");
        }
    }
}
