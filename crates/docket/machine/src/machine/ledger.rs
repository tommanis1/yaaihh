//! The commitment ledger (I-1455 (B), proto-mind N-1479 p85): what each agent has been asked to act on, and the
//! post of its that answered each ask, if any. One rule, read by the views (`TicketView::asked`, and from it
//! `unanswered_owner`), by the brain's report (coordd brain.rs `reading_within`) and by the Map's ring. Derived
//! from the posts, as `Machine::waits_between` is: no line of its own, so a replay derives the same.
//!
//! Who post M asks: his post, the agents it wakes (`Post::wakes`) but him; his `@all`, the agent holding it now
//! (`all_of`'s `handed`) until its claim; an agent's post, those it asks to act by I-1480's rule (delivery.rs `asks`:
//! typed an ask, read or blocker to it, or an untyped `@NAME` that is not `answering` it). An fyi, a result, a
//! hold, a post that only informs, and a post held, dropped, moved or taken back ask nothing; nor does a post that
//! passes his `@all` on (`Post::passes`): his `@all` is the ask, owed by the agent it names now, and its claim answers
//! it. Counted as an ask of its own, the pass stood open after the claim, since a reply-tool answer answers only the
//! post it names: docket-core's N-1104.p1014, passing his p1013 to paper-research, answered by p1017, stood in
//! paper-research's ledger, and three of its brain's looks rested on it (paper-research N-1446 p2973, proto-mind
//! p2977).
//!
//! What answers it: the first post of the asked agent's that closes it (delivery.rs `closes`, the waits-on edge's
//! close), or its reply-tool answer from another ticket (`Ticket::answered_from`), or a ticket it opened whose form's
//! `for` names M or an item whose ask or note M is: its hand-in, at its open (`Machine::handed_in_for`,
//! `Ticket::handed_in`; docket-edge5 N-1446 p1494, 8, paper-research p1495, p2483); his `@all`, its claim.

use super::delivery::{asks, closes};
use super::*;
use crate::line::PostKind;
use serde::{Deserialize, Serialize};

/// One ask an agent owes (`Machine::ledger`): POST (`N-1104.p678`) by BY at AT asks it to act, for WHY; HEAD, the
/// head a typed read names; ANSWER, the post that answered it (`N-1104.p679`, on another ticket for a reply-tool
/// answer from there), or the ticket it handed in (`N-1600`), None while it is open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commitment {
    pub post: String,
    pub by: String,
    pub at: At,
    /// "owner" (his post wakes it), "all" (his `@all`, held by it), "ask", "read", "blocker" (typed so, to it),
    /// "named" (an untyped `@NAME` of it).
    pub why: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
}

/// The agents post M asks to act, before their answers (`Machine::asked_post`): his `@all`'s holders (ALL, its
/// `all_state`), else the agents his post wakes but him, else those an agent's post is typed to, or names.
fn whom(m: &Post, all: Option<&AllState>) -> Vec<String> {
    match all {
        Some(st) => st.handed.clone(),
        None if m.by == "owner" => m.wakes().filter(|a| *a != "owner").cloned().collect(),
        None => m.kind.as_ref().map_or_else(|| m.names.clone(), |k| k.to().to_vec()),
    }
}

impl Machine {
    /// What AGENT has been asked to act on, on every ticket but the hidden ones, oldest first (by time, then
    /// ticket and post): each ask with the post of AGENT's that answered it, if any. Empty for the owner: what
    /// agents ask him is their posts' asks (`Post::asks`, `Machine::asks_waiting`).
    pub fn ledger(&self, agent: &str) -> Vec<Commitment> {
        if agent == "owner" {
            return Vec::new();
        }
        let mut out: Vec<(At, u64, u32, Commitment)> = Vec::new();
        for (t, x) in self.tickets.range(..HIDDEN) {
            for m in x.posts.iter().filter(|m| m.by != agent && m.visible() && m.moved.is_none()) {
                if let Some((why, answer)) = self.owed(x, m, agent) {
                    let head = match &m.kind {
                        Some(PostKind::Read { head, .. }) => Some(head.clone()),
                        _ => None,
                    };
                    let id = self.id(*t);
                    out.push((m.at.clone(), *t, m.n, Commitment { post: format!("{id}.p{}", m.n), by: m.by.clone(), at: m.at.clone(), why: why.into(), head, answer }));
                }
            }
        }
        out.sort_by(|a, b| (&a.0, a.1, a.2).cmp(&(&b.0, b.1, b.2)));
        out.into_iter().map(|(.., o)| o).collect()
    }

    /// The agents post N of T asks to act, in the order it names them, each with the post that answered it, if any:
    /// the ring of his post on the Map, open while one has none. Empty for a post that asks nothing.
    pub fn asked(&self, t: u64, n: u32) -> Vec<(String, Option<String>)> {
        let Some(x) = self.tickets.get(&t) else { return Vec::new() };
        x.posts.iter().find(|m| m.n == n).map_or_else(Vec::new, |m| self.asked_post(x, m))
    }

    /// The value of the [`ASKED`] line of his post N of T (N-1511 p47; paper-research N-1446 p2204), raised at the
    /// first head handed in for it (`graph::Builder::first_handed`, p2354): `{post, by, to, title}`, TO the agents it
    /// asks now (`asked`), TITLE its first line (`label::first_line`), else its id. None when it is no ask of his
    /// (`Machine::his_words`), asks nobody, or an `asked` line named it (`asks_raised`).
    pub fn asked_value(&self, t: u64, n: u32) -> Option<Value> {
        if !self.his_words(t, n) || self.asks_raised.contains(&(t, n)) {
            return None;
        }
        let to: Vec<String> = self.asked(t, n).into_iter().map(|(a, _)| a).collect();
        let p = self.tickets.get(&t)?.posts.iter().find(|p| p.n == n).filter(|_| !to.is_empty())?;
        let post = format!("{}.p{n}", self.id(t));
        let title = match self.text(&p.text) {
            Some(Text::New(w)) => crate::label::first_line(&w, crate::label::WORDS),
            _ => String::new(),
        };
        let title = if title.is_empty() { post.clone() } else { title };
        Some(serde_json::json!({"post": post, "by": p.by, "to": to, "title": title}))
    }

    /// `asked` of post M of X, found already (`TicketView::asked` reads it for each of his posts): his `@all`'s state
    /// read once for all the agents it may ask.
    pub(crate) fn asked_post(&self, x: &Ticket, m: &Post) -> Vec<(String, Option<String>)> {
        if !m.visible() || m.moved.is_some() {
            return Vec::new();
        }
        let all = self.all_of_his(x, m);
        let mut out: Vec<(String, Option<String>)> = Vec::new();
        for a in whom(m, all.as_ref()) {
            if out.iter().all(|(b, _)| *b != a) {
                if let Some((_, answer)) = self.owed_with(x, m, &a, all.as_ref()) {
                    out.push((a, answer));
                }
            }
        }
        out
    }

    /// Whether his post M of X is answered for every agent (paper-research N-1446 p2348, p2355): an agent it asks
    /// (`whom`) or was handed to (`Ticket::gave`) answered it by the ledger's one rule (`owed_with`'s answer: its post
    /// after M that closes it, `closes`; its reply-tool answer from another ticket, `Ticket::answered_from`; a ticket it
    /// handed in for M, `Ticket::handed_in`), or his `@all` was claimed. Answered so, M is open for none of them: not
    /// in `TicketView::unanswered_owner` (the Threads page's "Asked, no answer yet", the Map's ring), not named at the
    /// foot of their deliveries (`owed_posts`), and no giving of it says not answered (`owes_giving`). Each agent's own
    /// answer stays as the ledger holds it (`asked`, `ledger`). One who may answer on its thread has; the others need
    /// not, or may not post there: N-1104's p843 and p938, answered by p844 and p939, stood at the foot of every
    /// delivery of agents that may not post on N-1104 (p2347), and N-1104.p315, answered by here's p316 and
    /// paper-research's p317, stood in his "Asked, no answer yet" (docket-edge8 p2353, 8). A reply-tool answer to
    /// another post (`Post::answers`, not M nor among its further ids, `Ticket::also_here`) answers M for nobody else:
    /// its `to` names what it answers. A post by an agent neither asked nor handed it answers nothing, and an agent's
    /// post is never answered so.
    pub(crate) fn his_answered_in(&self, x: &Ticket, m: &Post) -> bool {
        if m.by != "owner" {
            return false;
        }
        let all = self.all_of_his(x, m);
        if all.as_ref().is_some_and(|st| st.claim.is_some()) {
            return true;
        }
        let mut who: BTreeSet<String> = whom(m, all.as_ref()).into_iter().collect();
        who.extend(x.given.iter().filter(|g| g.to != "owner" && Ticket::gave(g, m.n)).map(|g| g.to.clone()));
        let by_one = |of: &[(u32, String, String)]| of.iter().any(|(n, by, _)| *n == m.n && who.contains(by));
        let to_m = |q: &Post| q.answers.is_none_or(|a| a == m.n) || x.also_here(q).contains(&m.n);
        x.posts.iter().skip_while(|q| q.n <= m.n).any(|q| who.contains(&q.by) && closes(x, m, q, self.era >= ERA_QUIET) && to_m(q)) || by_one(&x.answered_from) || by_one(&x.handed_in)
    }

    /// `his_answered_in` of post N of ticket T: false for a post or ticket not there.
    pub fn his_answered(&self, t: u64, n: u32) -> bool {
        let Some(x) = self.tickets.get(&t) else { return false };
        x.posts.iter().find(|p| p.n == n).is_some_and(|m| self.his_answered_in(x, m))
    }

    /// The state of post M of X if it is his `@all` (`all_state`); None for any other post.
    fn all_of_his(&self, x: &Ticket, m: &Post) -> Option<AllState> {
        if m.by == "owner" { self.all_state(x.t, m.n) } else { None }
    }

    /// Whether post M of X asks AGENT to act, and why, with AGENT's answer to it: the ledger's one rule.
    fn owed(&self, x: &Ticket, m: &Post, agent: &str) -> Option<(&'static str, Option<String>)> {
        if m.by == agent || agent == "owner" || !m.visible() || m.moved.is_some() {
            return None;
        }
        self.owed_with(x, m, agent, self.all_of_his(x, m).as_ref())
    }

    /// `owed`, with ALL, post M's `@all` state as `all_of_his` reads it.
    fn owed_with(&self, x: &Ticket, m: &Post, agent: &str, all: Option<&AllState>) -> Option<(&'static str, Option<String>)> {
        if m.by == agent || agent == "owner" {
            return None;
        }
        let post = |q: &Post| Some(format!("{}.p{}", self.id(x.t), q.n));
        if m.by == "owner" {
            if let Some(st) = all {
                if !st.handed.iter().any(|a| a == agent) {
                    return None;
                }
                return Some(("all", st.claim.as_ref().and_then(|(_, c)| x.posts.iter().find(|q| q.n == *c)).and_then(post)));
            }
            if !m.wakes().any(|a| a == agent) {
                return None;
            }
        } else if m.passes.is_some() || !asks(m, agent) {
            return None;
        }
        let why = match &m.kind {
            _ if m.by == "owner" => "owner",
            Some(k) => k.is(),
            None => "named",
        };
        // only a post after M can answer it (`closes`): the search starts there; a hand-in is after M, which it named
        let from = |of: &[(u32, String, String)]| of.iter().find(|(p, by, _)| *p == m.n && by == agent).map(|(_, _, id)| id.clone());
        let answer = x.posts.iter().skip_while(|q| q.n <= m.n).find(|q| q.by == agent && closes(x, m, q, self.era >= ERA_QUIET)).and_then(post).or_else(|| from(&x.answered_from)).or_else(|| from(&x.handed_in));
        Some((why, answer))
    }

    /// The post a ticket of FORM hands in for (its `for`, the head's `--for`; docket-edge5 N-1446 p1494, 8): the post
    /// it names, or the post the item it names names: its `ask`, his goal's (research's goals.kind opens it so; a head
    /// handed in for the goal by its id, paper-research p2483), else its note (p1495, the note `ask_times` reads), each
    /// as `Machine::id` writes it and there now. None for a form with no `for`, or one naming any other ticket, an item
    /// whose ask and note are no post, or a post not there.
    pub(crate) fn handed_in_for(&self, form: &Value) -> Option<(u64, u32)> {
        let post = |id: &str| post_ref(id).and_then(|(t, n)| Some((self.ticket_named(t)?, n)));
        let named = form.get("for")?.as_str()?;
        let (t, n) = match post_or_ticket(named)? {
            (_, Some(_)) => post(named)?,
            (item, None) => {
                let x = self.tickets.get(&self.ticket_named(item)?).filter(|x| x.kind == "item")?;
                ["ask", "note"].iter().find_map(|k| post(x.form.get(*k)?.as_str()?))?
            }
        };
        self.tickets.get(&t)?.posts.iter().any(|p| p.n == n).then_some((t, n))
    }

    /// Ticket T, just opened by OPENER with FORM: a hand-in, kept on the post it hands in for (`handed_in_for`) as its
    /// ticket's `handed_in`.
    pub(super) fn hand_in(&mut self, cx: &mut Cx, t: u64, opener: &str, form: &Value) {
        let Some((u, n)) = self.handed_in_for(form) else { return };
        let id = self.id(t);
        if let Some(x) = self.tickets.edit().get_mut(&u) {
            x.handed_in.push((n, opener.to_string(), id));
            cx.effect(Effect::Changed { t: u });
        }
    }

    /// The post of his that ticket T, opened with FORM, is the first head handed in for, while that post's `asked`
    /// line is still to come (`asked_value`): where coordd raises it (`graph::Builder::first_handed`; paper-research
    /// N-1446 p2354), and whose goal begins with the lines T was opened with (`Machine::first_heads`).
    pub fn first_head_for(&self, t: u64, form: &Value) -> Option<(u64, u32)> {
        let (u, n) = self.handed_in_for(form)?;
        let (.., first) = self.tickets.get(&u)?.handed_in.iter().find(|(p, ..)| *p == n)?;
        (*first == self.id(t) && self.asked_value(u, n).is_some()).then_some((u, n))
    }
}
