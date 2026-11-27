//! A box's state as yaaihh holds it: the last snapshot with every event after it applied (design §7: readers draw
//! from the stream, never from files).

use coord_types::{AgentView, Body, Entry, Event, MailView, Quote, Transcript, ViewData};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct Live {
    /// The project file as JSON (Null when the box has none).
    pub project: Value,
    pub agents: Vec<AgentView>,
    pub mail: Mail,
    pub views: HashMap<String, ViewData>,
    /// The project's look as checked (coord_types::look): the built-in look while coordd sends none, and what it sent
    /// last that checked. `look_refused`: why the last one sent did not check (it is then not taken; what was drawn
    /// stays), for the Kinds page's refusals.
    pub look: coord_types::look::Look,
    pub look_refused: Vec<String>,
    pub look_rev: u64,
    /// What the kinds' and policy's check refused.
    pub refusals: Vec<String>,
    /// Docket's tickets, by number: the snapshot's, then each one as it changes.
    pub tickets: std::collections::BTreeMap<u64, coord_types::TicketView>,
    /// What coordd could not do and says so (paper-research N-8): the instance's (no ticket) and the tickets'.
    pub notices: Vec<coord_types::Notice>,
    /// The sidebar's parts in their order, `[name, place]` (N-412 ask 2; parts.rs); none from an older coordd.
    pub parts: Vec<(String, i64)>,
    /// The project's clocks (`every D`) and policy-decided bindings (`when D is O`), each with whose own program it is
    /// and when it last fired (0.2 line 26; the snapshot's `clocks`): what the Docket's starts and a binding's chip read.
    /// None from an older coordd.
    pub clocks: Vec<coord_types::ClockView>,
    /// The project's folders as the server sees them (docket-core5, N-412 p129); none from an older coordd, and the
    /// app then reads them from its own disk.
    pub places: Option<coord_types::Places>,
    /// The scopes slow is set over (N-412 ask 5; the snapshot's `slowed`): "project" while every ticket halts. An older
    /// coordd sends none.
    pub slowed: Vec<String>,
    /// Since when the owner holds agents' mail (line 327; the snapshot's `mail_hold`); none while it flows, and from an
    /// older coordd.
    pub mail_hold: Option<String>,
    /// The project's owner model as coordd reads it (the snapshot's `owner_model`, then each `Body::OwnerModel`): what
    /// the Map's node draws. None where no node stands, and from a coordd before `OWNER_MODEL_PROTOCOL`.
    pub owner_model: Option<coord_types::OwnerModelView>,
    pub seq: u64,
    /// Counts the changes to `views`: a view's page is built again when it moves.
    pub views_rev: u64,
    /// Counts the events that changed `tickets`: what is built from all of them (an agent's streams, streams.rs) is
    /// built again when it moves, not on every frame.
    pub tickets_rev: u64,
    /// Counts the events that changed `agents`, as `tickets_rev` counts the tickets': the Map lays its islands again
    /// when it moves (map/join.rs).
    pub agents_rev: u64,
    /// Tickets whose `ticketchange` did not add up to the count it gave (connected between, or one missed): to be
    /// asked for whole (`Request::Ticket`), their merged state shown meanwhile.
    pub stale_tickets: std::collections::BTreeSet<String>,
}

impl Live {
    /// The instance's notices (coordd cannot write its log): one red line on every page.
    pub fn instance_notices(&self) -> impl Iterator<Item = &coord_types::Notice> {
        self.notices.iter().filter(|n| n.ticket.is_none())
    }

    /// Ticket ID's notices (a model call on it failed).
    pub fn ticket_notices<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a coord_types::Notice> {
        self.notices.iter().filter(move |n| n.ticket.as_deref() == Some(id))
    }

    /// Apply EV; the agent it changed, if it is an agent's.
    pub fn apply(&mut self, ev: Event) -> Option<String> {
        self.seq = ev.seq;
        match ev.body {
            Body::Snapshot(s) => {
                self.project = s.project.unwrap_or(Value::Null);
                self.agents = s.agents;
                self.agents_rev += 1;
                let rev = self.mail.rev;
                self.mail = Mail::from(s.mail);
                self.mail.rev += rev;
                self.views = s.views.into_iter().map(|v| (v.id.clone(), v)).collect();
                self.views_rev += 1;
                self.refusals = s.refusals;
                self.tickets = s.tickets.into_iter().map(|t| (t.n, t)).collect();
                self.tickets_rev += 1;
                self.notices = s.notices;
                self.parts = s.parts;
                self.clocks = s.clocks;
                self.places = s.places;
                self.slowed = s.slowed;
                self.mail_hold = s.mail_hold;
                self.owner_model = s.owner_model;
                self.set_look(s.look);
            }
            Body::Notices(n) => self.notices = n,
            Body::OwnerModel(o) => self.owner_model = o,
            Body::Ticket(t) => {
                self.stale_tickets.remove(&t.id);
                self.tickets.insert(t.n, t);
                self.tickets_rev += 1;
            }
            Body::TicketChange { view, posts } => {
                let id = view.id.clone();
                let known = self.tickets.contains_key(&view.n);
                let mut t = view;
                let new = std::mem::take(&mut t.posts);
                let mut all = self.tickets.get(&t.n).map(|o| o.posts.clone()).unwrap_or_default();
                all.retain(|p| !new.iter().any(|q| q.n == p.n));
                all.extend(new);
                all.sort_by_key(|p| p.n);
                t.posts = all;
                if !known || t.posts.len() as u32 != posts {
                    self.stale_tickets.insert(id);
                }
                self.tickets.insert(t.n, t);
                self.tickets_rev += 1;
            }
            // a post elsewhere named it: its new lines back, at the end (a ticket not held is had whole when asked)
            Body::Mentioned { id, lines } => {
                if let Some(t) = self.tickets.values_mut().find(|t| t.id == id) {
                    t.mentioned.extend(lines);
                    self.tickets_rev += 1;
                }
            }
            Body::Refusals(e) => self.refusals = e,
            Body::Agent(a) => {
                let name = a.name.clone();
                self.agents_rev += 1;
                match self.agents.iter_mut().find(|x| x.name == a.name) {
                    Some(x) => *x = a,
                    None => self.agents.push(a),
                }
                return Some(name);
            }
            Body::Mail(m) => self.mail.put(m),
            Body::Delivered { id, to, .. } => self.mail.delivered(&id, to),
            Body::View(v) => {
                self.views.insert(v.id.clone(), v);
                self.views_rev += 1;
            }
            // a stream resumed after a reconnect (protocol 1): the state held stands, and what was missed follows
            Body::Resumed { .. } => {}
            Body::Look(v) => self.set_look(Some(v).filter(|v| !v.is_null())),
        }
        None
    }

    /// What the owner is told was refused: the kinds' and policy's check, then a look that did not check.
    pub fn refused(&self) -> impl Iterator<Item = &String> {
        self.refusals.iter().chain(&self.look_refused)
    }

    /// Take the look V coordd sent (none: the built-in look). One that does not check is not taken: what was drawn
    /// stays, and why goes to `look_refused`.
    pub fn set_look(&mut self, v: Option<Value>) {
        let before = (self.look.clone(), self.look_refused.len());
        match v.map(|v| coord_types::look::check(&v)).unwrap_or(Ok(coord_types::look::Look::default())) {
            Ok(l) => {
                self.look = l;
                self.look_refused.clear();
            }
            // one plain line for him (N-866.p6, 5), the check's words behind it in the log
            Err(e) => {
                eprintln!("look.toml not taken: {}", e.join("; "));
                self.look_refused = vec![format!("look.toml not applied: {}. Fix it in Edit look.", e.first().cloned().unwrap_or_default())];
            }
        }
        // what is drawn changes only when the look does: every memoised part then builds again (version.rs)
        if (self.look.clone(), self.look_refused.len()) != before {
            self.look_rev += 1;
            crate::version::LOOK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// The items closed: their item tickets closed (E5), by number, so I-042 and I-42 are one item.
    pub fn closed(&self) -> Closed {
        Closed(self.tickets.values().filter(|v| v.kind == "item" && v.closed_at.is_some()).map(|v| v.n).collect())
    }

    /// Slow is set over the whole project (N-412 ask 5): every ticket halts until the owner approves.
    pub fn slow_project(&self) -> bool {
        self.slowed.iter().any(|x| x == "project")
    }

    /// The project file's views, in its order.
    pub fn view_specs(&self) -> Vec<&Value> {
        self.project.get("view").and_then(Value::as_array).map(|v| v.iter().collect()).unwrap_or_default()
    }

    /// The view the project opens on: its file's `home`, else its first view.
    pub fn home(&self) -> String {
        let first = || self.view_specs().first().and_then(|v| v.get("id")).and_then(Value::as_str).map(String::from);
        self.project.pointer("/project/home").and_then(Value::as_str).map(String::from).or_else(first).unwrap_or_default()
    }

    pub fn project_name(&self) -> Option<&str> {
        self.project.pointer("/project/name").and_then(Value::as_str)
    }

    /// The agents as a tree: each with its depth, children after their parent, in the order they were added.
    pub fn agent_tree(&self) -> Vec<(usize, &AgentView)> {
        fn walk<'a>(all: &'a [AgentView], parent: &str, depth: usize, out: &mut Vec<(usize, &'a AgentView)>) {
            for a in all.iter().filter(|a| a.parent == parent) {
                out.push((depth, a));
                walk(all, &a.name, depth + 1, out);
            }
        }
        let names: HashSet<&str> = self.agents.iter().map(|a| a.name.as_str()).collect();
        let mut out = Vec::new();
        // roots: agents whose parent is not an agent of this box (the owner, a host session); the teams (roots with agents
        // below them) first, lone ones (a consultant) after
        let mut roots: Vec<&AgentView> = self.agents.iter().filter(|a| !names.contains(a.parent.as_str())).collect();
        roots.sort_by_key(|r| !self.agents.iter().any(|a| a.parent == r.name));
        for a in roots {
            out.push((0, a));
            walk(&self.agents, &a.name, 1, &mut out);
        }
        out
    }
}

/// The box's mail, oldest first (read as a slice), with its threads indexed: a view asks for a mail's thread on
/// every build, for every mail it shows, so the lookups are by id, never a walk of the whole list (9 Nov: walking it
/// took 60% of each frame).
#[derive(Default, Clone)]
pub struct Mail {
    list: Vec<MailView>,
    /// Each id's place (the first mail with it, when two share one).
    at: HashMap<String, usize>,
    /// The places of the mails that answer each id (`re`), oldest first.
    under: HashMap<String, Vec<usize>>,
    /// The owner's quotes by their source (`AGENT@OFFSET`, a mail id): the mail's place and the quote's.
    quoted: HashMap<String, Vec<(usize, usize)>>,
    /// Counts the changes: a memoised part that shows mail is built again when it moves.
    pub rev: u64,
}

impl From<Vec<MailView>> for Mail {
    fn from(list: Vec<MailView>) -> Mail {
        let mut m = Mail { list, ..Mail::default() };
        m.index();
        m
    }
}

impl std::ops::Deref for Mail {
    type Target = [MailView];
    fn deref(&self) -> &[MailView] {
        &self.list
    }
}

impl Mail {
    fn index(&mut self) {
        self.rev += 1;
        self.at.clear();
        self.under.clear();
        self.quoted.clear();
        for (i, m) in self.list.iter().enumerate() {
            self.at.entry(m.id.clone()).or_insert(i);
            if let Some(r) = &m.re {
                self.under.entry(r.clone()).or_default().push(i);
            }
            if m.from == "owner" {
                for (j, q) in m.quotes.iter().enumerate() {
                    self.quoted.entry(q.source.clone()).or_default().push((i, j));
                }
            }
        }
    }

    /// M, in place of the mail with its id, or after the others.
    pub fn put(&mut self, m: MailView) {
        match self.at.get(&m.id) {
            Some(&i) => self.list[i] = m,
            None => self.list.push(m),
        }
        self.index();
    }

    /// Mail ID reached TO.
    pub fn delivered(&mut self, id: &str, to: String) {
        if let Some(&i) = self.at.get(id) && !self.list[i].delivered.contains(&to) {
            self.list[i].delivered.push(to);
            self.rev += 1;
        }
    }

    /// The owner's quotes of SOURCE, oldest first: each with its mail and its place in the mail's quotes.
    pub fn quotes(&self, source: &str) -> impl Iterator<Item = (&MailView, usize, &Quote)> {
        self.quoted.get(source).into_iter().flatten().map(|&(i, j)| (&self.list[i], j, &self.list[i].quotes[j]))
    }

    pub fn get(&self, id: &str) -> Option<&MailView> {
        self.at.get(id).map(|&i| &self.list[i])
    }

    /// The first mail of the thread ID is in (a question or a plain mail): up its `re`s while they name a mail held.
    pub fn top(&self, id: &str) -> String {
        let mut cur = id;
        for _ in 0..self.list.len() {
            match self.get(cur).and_then(|m| m.re.as_deref()) {
                Some(up) => cur = up,
                None => break,
            }
        }
        cur.to_string()
    }

    /// The first question of the thread M is in, when it is in one.
    pub fn root<'a>(&'a self, m: &'a MailView) -> Option<&'a MailView> {
        let mut cur = m;
        for _ in 0..self.list.len() {
            match &cur.re {
                Some(r) => cur = self.get(r)?,
                None => break,
            }
        }
        crate::ask::is_question(cur).then_some(cur)
    }

    /// The mails in ID's thread after it: every mail whose `re` leads back to ID, oldest first; not those a net filed
    /// that the owner took back (the thread still runs through them).
    pub fn replies(&self, id: &str) -> Vec<&MailView> {
        let mut found: Vec<usize> = Vec::new();
        let mut next = vec![id];
        let mut seen: HashSet<usize> = HashSet::new();
        while let Some(up) = next.pop() {
            for &i in self.under.get(up).into_iter().flatten() {
                if seen.insert(i) {
                    found.push(i);
                    // the mails answering this one answer it by its id, which leads back only from its first holder
                    if self.at.get(&self.list[i].id) == Some(&i) {
                        next.push(&self.list[i].id);
                    }
                }
            }
        }
        found.sort_unstable();
        found.into_iter().map(|i| &self.list[i]).filter(|m| !m.unfiled).collect()
    }

    /// The agents in the thread ID is in (whoever wrote in it or was sent mail in it, as coordd counts them; not the
    /// owner), in order of joining.
    pub fn participants(&self, id: &str) -> Vec<String> {
        let root = self.top(id);
        let mut out: Vec<String> = Vec::new();
        for m in self.list.iter().filter(|m| m.id == root).chain(self.replies(&root)) {
            for n in std::iter::once(&m.from).chain(&m.to) {
                if n != "owner" && !out.contains(n) {
                    out.push(n.clone());
                }
            }
        }
        out
    }
}

/// What yaaihh has read of one agent's session: a contiguous run of entries, from byte `start` to byte `next`.
#[derive(Default)]
pub struct Conv {
    pub entries: Vec<Entry>,
    /// A key per entry, unique in the session: its line's offset and its place among that line's entries.
    pub keys: Vec<String>,
    pub start: u64,
    pub next: u64,
    /// The transcript `start` and `next` are offsets in.
    pub session: String,
    pub loaded: bool,
    /// Why coordd refused the last read (shown in place of "reading the session…"; a new selection asks again).
    pub refused: Option<String>,
    pub older_pending: bool,
    pub newer_pending: bool,
    /// Not on screen when a new stream began (a reconnect, a wake from sleep): the read asked is the session's last
    /// page (`merge_last`), never all that grew since (consult-2, N-412.p544: hours of a session through the core's
    /// lock and the bridge).
    pub catch_up: bool,
    /// Not on screen, and its session grew since it was read (or a new stream began): since when. Its last page is
    /// read when it shows again (`App::read_shown`), never while hidden (N-1933.p1).
    pub behind: Option<std::time::Instant>,
    /// A layout of its chat has had the last row in sight: from then on its top asks for the page before
    /// (`chat::wants_older`).
    pub end_seen: bool,
    /// His system-prompt edits that fall in the session held, as coordd's latest page of it said them
    /// (`Transcript::restarted`, from TRANSCRIPT_SESSION_PROTOCOL; none from an older coordd): drawn where they fell
    /// (work.rs `Item::Restarted`; path fix 6 point 3, accounts-lead N-1446 p2524).
    pub restarted: Vec<docket_machine::graph::Restarted>,
    /// The old line beside each of those made from a turn, by its transcript's path (`Branched::from`): read from the
    /// cut on when he first opens it (`Proj::toggle_fold`).
    pub folds: std::collections::HashMap<String, Fold>,
}

/// The old line's part after his system-prompt edit from a turn, beside the edit in the new line's chat (path fix 6
/// point 3): open or folded, and what is read of it, its transcript from the cut to its end. The agent went on in the
/// new line, so it no longer grows: one page reads it.
#[derive(Default)]
pub struct Fold {
    pub open: bool,
    pub conv: Conv,
}

impl Conv {
    /// Entry KEY (`e{offset}-{n}`, [`Self::keys`]), when the session held has it.
    pub fn entry(&self, key: &str) -> Option<&Entry> {
        self.keys.iter().position(|x| x == key).map(|i| &self.entries[i])
    }

    /// What tells one state of the session held from another: it only grows at either end, or is dropped and read
    /// again (`merge`); and how many of his edits fall in it.
    pub fn stamp(&self) -> (bool, usize, u64, u64, String, usize) {
        (self.loaded, self.entries.len(), self.start, self.next, self.session.clone(), self.restarted.len())
    }

    /// Put a page in: the first, one before what is held, or one after it. False when the page is of another
    /// transcript than the one held (the session moved to a new file): what is held is dropped, to be read again.
    pub fn merge(&mut self, mut page: Transcript, older: bool) -> bool {
        self.refused = None;
        if self.loaded && page.session != self.session {
            *self = Conv::default();
            return false;
        }
        // his edits in it, all of them with every page
        self.restarted = std::mem::take(&mut page.restarted);
        if !self.loaded {
            self.session = page.session.clone();
            self.entries = page.entries;
            self.start = page.start;
            self.next = page.next;
            self.loaded = true;
        } else if older {
            if page.next == self.start {
                let mut e = page.entries;
                e.append(&mut self.entries);
                self.entries = e;
                self.start = page.start;
            }
        } else if page.start == self.next {
            self.entries.extend(page.entries);
            self.next = page.next;
        }
        self.keys = keys(&self.entries);
        true
    }
}

impl Conv {
    /// Shown again after it fell behind, and its last page asked and not in yet: the chat says it is reading.
    pub fn catching_up(&self) -> bool {
        self.catch_up && self.newer_pending
    }

    /// Put in the session's last page, read on showing a chat that fell behind: joined to what is held where it starts
    /// within it, else in its place (what lies between is read on scrolling up, as any earlier part).
    pub fn merge_last(&mut self, mut page: Transcript) {
        self.refused = None;
        self.catch_up = false;
        self.restarted = std::mem::take(&mut page.restarted);
        let joins = self.loaded && page.session == self.session && page.start > self.start && page.start <= self.next;
        if joins {
            self.entries.retain(|e| e.offset < page.start);
            self.entries.extend(page.entries);
        } else {
            self.session = page.session;
            self.entries = page.entries;
            self.start = page.start;
            self.loaded = true;
        }
        self.next = page.next;
        self.keys = keys(&self.entries);
    }
}

fn keys(entries: &[Entry]) -> Vec<String> {
    let mut out = Vec::with_capacity(entries.len());
    let mut n = 0;
    for (i, e) in entries.iter().enumerate() {
        n = if i > 0 && entries[i - 1].offset == e.offset { n + 1 } else { 0 };
        out.push(format!("e{}-{n}", e.offset));
    }
    out
}

/// The number of item ID (`I-042`, `I-42`: 42), however many digits it is written with.
pub fn item_number(id: &str) -> Option<u64> {
    id.rsplit('-').next()?.parse().ok()
}

/// The closed items, by number (`Live::closed`).
pub struct Closed(HashSet<u64>);

impl Closed {
    pub fn contains(&self, id: &str) -> bool {
        item_number(id).is_some_and(|n| self.0.contains(&n))
    }
}

/// Whether item ids A and B are one item (`I-042` and `I-42` are).
pub fn same_item(a: &str, b: &str) -> bool {
    a == b || matches!((item_number(a), item_number(b)), (Some(x), Some(y)) if x == y)
}

#[cfg(test)]
mod notice_tests {
    use super::*;

    // paper-research N-8: the snapshot's notices, then each Notices event replacing the whole list; the instance's
    // (no ticket) apart from a ticket's
    #[test]
    fn notices_come_with_the_snapshot_and_each_event_replaces_them() {
        let n = |t: Option<&str>, text: &str| coord_types::Notice { ticket: t.map(String::from), text: text.into(), at: "2026-11-12T20:00:00Z".into() };
        let mut l = Live::default();
        let snap: coord_types::Snapshot = serde_json::from_value(serde_json::json!({"project": null, "agents": [], "mail": [], "views": [],
            "notices": [{"ticket": null, "text": "coordd cannot write its log", "at": "2026-11-12T20:00:00Z"}]})).unwrap();
        l.apply(Event { seq: 1, body: Body::Snapshot(snap) });
        assert_eq!(l.instance_notices().count(), 1);
        l.apply(Event { seq: 2, body: Body::Notices(vec![n(Some("N-3"), "the minder did not look: the call failed (E)")]) });
        assert_eq!(l.instance_notices().count(), 0);
        assert_eq!(l.ticket_notices("N-3").map(|x| x.text.as_str()).collect::<Vec<_>>(), ["the minder did not look: the call failed (E)"]);
        assert_eq!(l.ticket_notices("N-4").count(), 0);
        // a snapshot without the field (an older coordd) clears them
        let snap: coord_types::Snapshot = serde_json::from_value(serde_json::json!({"project": null, "agents": [], "mail": [], "views": []})).unwrap();
        l.apply(Event { seq: 3, body: Body::Snapshot(snap) });
        assert!(l.notices.is_empty());
    }
}

#[cfg(test)]
mod change_tests {
    use super::*;
    use serde_json::json;

    fn thread(posts: Value) -> coord_types::TicketView {
        serde_json::from_value(json!({"id": "N-5", "n": 5, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "pr"}, "title": "t",
            "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": "2026-11-16T10:00:00Z", "given": [], "posts": posts}))
        .unwrap()
    }
    fn post(n: u32, text: &str) -> Value {
        json!({"n": n, "at": "2026-11-16T10:00:00Z", "by": "pr", "text": text, "names": []})
    }

    /// A ticket's change (paper-research N-101 p1165): its posts merged by number into those held, the rest of it
    /// taken whole; a count that does not add up marks the ticket to be asked for whole, and the whole one clears it.
    #[test]
    fn a_tickets_change_merges_its_posts_by_number() {
        let mut l = Live::default();
        l.apply(Event { seq: 1, body: Body::Ticket(thread(json!([post(1, "one"), post(2, "two")]))) });
        let mut v = thread(json!([post(2, "two, edited"), post(3, "three")]));
        v.title = "new title".into();
        l.apply(Event { seq: 2, body: Body::TicketChange { view: v, posts: 3 } });
        let t = &l.tickets[&5];
        let texts: Vec<String> = t.posts.iter().map(|p| serde_json::to_value(&p.text).unwrap().as_str().unwrap_or_default().to_string()).collect();
        assert_eq!((texts, t.title.as_str()), (vec!["one".to_string(), "two, edited".into(), "three".into()], "new title"));
        assert!(l.stale_tickets.is_empty());
        // one missed: the count says 5, four are held
        l.apply(Event { seq: 4, body: Body::TicketChange { view: thread(json!([post(5, "five")])), posts: 5 } });
        assert!(l.stale_tickets.contains("N-5"));
        l.apply(Event { seq: 5, body: Body::Ticket(thread(json!([post(1, "one"), post(2, "two"), post(3, "3"), post(4, "4"), post(5, "five")]))) });
        assert!(l.stale_tickets.is_empty());
    }
}

#[cfg(test)]
mod slowed_tests {
    use super::*;
    use coord_types::{Body, Event, Snapshot};

    /// Slow over the whole project is the snapshot's `slowed` (N-412 ask 5): set, then lifted; an older coordd's
    /// snapshot, without the field, reads as not set.
    #[test]
    fn slow_over_the_project_follows_the_snapshot() {
        let mut l = Live::default();
        let snap = |slowed: Vec<String>| Event { seq: 1, body: Body::Snapshot(Snapshot { slowed, ..Snapshot::default() }) };
        l.apply(snap(vec!["project".into()]));
        assert!(l.slow_project());
        l.apply(snap(Vec::new()));
        assert!(!l.slow_project());
        let old: Snapshot = serde_json::from_value(serde_json::json!({"project": null, "agents": [], "mail": [], "views": []})).unwrap();
        l.apply(Event { seq: 2, body: Body::Snapshot(old) });
        assert!(!l.slow_project());
    }

    /// The owner's hold on agents' mail is the snapshot's `mail_hold` (line 327): set with its time, then let go; an
    /// older coordd's snapshot, without the field, reads as no hold.
    #[test]
    fn the_hold_follows_the_snapshot() {
        let mut l = Live::default();
        let snap = |mail_hold: Option<String>| Event { seq: 1, body: Body::Snapshot(Snapshot { mail_hold, ..Snapshot::default() }) };
        l.apply(snap(Some("2026-11-20T06:20:00Z".into())));
        assert_eq!(l.mail_hold.as_deref(), Some("2026-11-20T06:20:00Z"));
        l.apply(snap(None));
        assert_eq!(l.mail_hold, None);
        l.apply(snap(Some("2026-11-20T06:20:00Z".into())));
        let old: Snapshot = serde_json::from_value(serde_json::json!({"project": null, "agents": [], "mail": [], "views": []})).unwrap();
        l.apply(Event { seq: 2, body: Body::Snapshot(old) });
        assert_eq!(l.mail_hold, None);
    }

    /// The owner model is the snapshot's `owner_model`, then each `Body::OwnerModel` (proto-reader N-1446 p1212, B),
    /// each read from the line coordd's bridge writes, as the window reads its stream: a model, then an update at work
    /// after one that failed, then none (the lead stopped it); an older coordd's snapshot, with no field, reads as no
    /// node.
    #[test]
    fn the_owner_model_follows_the_snapshot_and_each_change() {
        use coord_types::{ModelMade, OwnerModelView, UpdateEnded};
        let read = |ev: Event| -> Event {
            let line = serde_json::to_string(&ev).unwrap();
            serde_json::from_str::<Event>(&line).unwrap_or_else(|e| panic!("the window cannot read it: {e}\n{line}"))
        };
        let made = OwnerModelView { node: "O-4".into(), running: false, model: Some(ModelMade { update: "O-6".into(), upto: "N-2.p1".into(), at: "2026-11-21T21:24:00Z".into(), first_line: "He wants one box, not two.".into() }), failed: None };
        let mut l = Live::default();
        l.apply(read(Event { seq: 1, body: Body::Snapshot(Snapshot { owner_model: Some(made.clone()), ..Snapshot::default() }) }));
        assert_eq!(l.owner_model.as_ref(), Some(&made));
        let failed = OwnerModelView { running: true, failed: Some(UpdateEnded { update: "O-7".into(), at: "2026-11-21T21:30:00Z".into() }), ..made.clone() };
        l.apply(read(Event { seq: 2, body: Body::OwnerModel(Some(failed.clone())) }));
        assert_eq!(l.owner_model.as_ref(), Some(&failed));
        l.apply(read(Event { seq: 3, body: Body::OwnerModel(None) }));
        assert_eq!(l.owner_model, None, "the lead stopped it");
        l.apply(read(Event { seq: 4, body: Body::Snapshot(Snapshot { owner_model: Some(made), ..Snapshot::default() }) }));
        let old: Snapshot = serde_json::from_value(serde_json::json!({"project": null, "agents": [], "mail": [], "views": []})).unwrap();
        l.apply(Event { seq: 5, body: Body::Snapshot(old) });
        assert_eq!(l.owner_model, None, "from a coordd before it");
    }
}

#[cfg(test)]
mod shown_tests {
    use super::Conv;
    use crate::view_cost::{linked_app, page_asked, page_reply};

    /// N-1933.p1: an idle window reads no session of an agent he has not opened, however many polls run;
    /// the one he opens is read, and only it.
    #[test]
    fn an_idle_window_reads_no_session_it_does_not_show() {
        let names: Vec<String> = (0..20).map(|i| format!("a{i:02}")).collect();
        let (_held, mut s, _cat) = linked_app(&names.iter().map(String::as_str).collect::<Vec<_>>());
        s.sel = crate::Sel::Tickets;
        for i in 0..5 {
            crate::poll(&mut s, i as f64, &mut rune::Tx::default());
        }
        assert!(names.iter().all(|n| page_asked(&s, n).is_none()), "no page asked: {:?}", s.projects[0].convs.keys().collect::<Vec<_>>());
        s.select_agent("a03");
        crate::poll(&mut s, 6.0, &mut rune::Tx::default());
        let asked: Vec<&String> = names.iter().filter(|n| page_asked(&s, n).is_some()).collect();
        assert_eq!(asked, ["a03"], "only the chat he opened");
    }

    /// After a new stream (a reconnect, a wake), the chat on screen reads on from where it ends; one not on screen asks
    /// nothing until it shows again, and then its session's last page, never all that grew while the window slept.
    #[test]
    fn a_new_stream_asks_a_hidden_chats_last_page_when_it_shows() {
        let (_held, mut s, _cat) = linked_app(&["qa", "dev"]);
        for n in ["qa", "dev"] {
            s.projects[0].convs.entry(n.into()).or_default().merge(page("s", 0, 300, &[100, 200]), false);
        }
        s.sel = crate::Sel::Agent("dev".into());
        let agents = s.projects[0].live.agents.clone();
        s.take(0, crate::link::Incoming::Event(coord_types::Event { seq: 1, body: coord_types::Body::Snapshot(coord_types::Snapshot { agents, ..Default::default() }) }));
        let (dev, qa) = (&s.projects[0].convs["dev"], &s.projects[0].convs["qa"]);
        assert!(dev.newer_pending && !dev.catch_up, "on screen: read on from 300");
        assert!(!qa.newer_pending && qa.catch_up && qa.behind.is_some(), "hidden: behind, nothing asked");
        s.select_agent("qa");
        assert!(s.projects[0].convs["qa"].newer_pending, "shown: its last page asked");
        let id = page_asked(&s, "qa").unwrap();
        s.take(0, page_reply(id, 9000, 9300, &[9000, 9200], &[]));
        let qa = &s.projects[0].convs["qa"];
        assert_eq!((qa.start, qa.next, qa.catch_up), (9000, 9300, false));
    }

    /// Twenty hidden chats, all held, grow at once while he reads a twenty-first: his chat's page is asked at once and
    /// no other; one he goes to next asks its last page then.
    #[test]
    fn twenty_hidden_chats_growing_at_once_ask_nothing_and_the_open_one_at_once() {
        let names: Vec<String> = (0..21).map(|i| format!("a{i:02}")).collect();
        let (_held, mut s, _cat) = linked_app(&names.iter().map(String::as_str).collect::<Vec<_>>());
        for n in &names {
            s.projects[0].convs.entry(n.clone()).or_default().merge(page("s", 0, 300, &[100, 200]), false);
        }
        s.sel = crate::Sel::Agent("a20".into());
        for n in &names {
            let mut a = s.projects[0].live.agents.iter().find(|a| &a.name == n).unwrap().clone();
            a.last_active = Some("2026-11-18T11:00:00Z".into());
            s.take(0, crate::link::Incoming::Event(coord_types::Event { seq: 2, body: coord_types::Body::Agent(a) }));
        }
        crate::poll(&mut s, 0.0, &mut rune::Tx::default());
        let asked = |s: &crate::App| names.iter().filter(|n| page_asked(s, n).is_some()).cloned().collect::<Vec<_>>();
        assert_eq!(asked(&s), ["a20"], "the chat on screen alone");
        s.select_agent("a05");
        assert_eq!(asked(&s), ["a05", "a20"]);
        assert!(s.projects[0].convs["a05"].catch_up, "its last page, not all that grew");
    }

    fn page(session: &str, start: u64, next: u64, offsets: &[u64]) -> coord_types::Transcript {
        let entries = offsets.iter().map(|o| coord_types::Entry { offset: *o, at: String::new(), kind: "assistant".into(), tool: None, text: format!("at {o}"), patch: None, images: vec![] }).collect();
        coord_types::Transcript { entries, start, next, session: session.into(), ..Default::default() }
    }

    // consult-2, N-412.p544: shown hours after it fell behind, the chat ends at the newest entry after one page, not
    // after all that grew between
    #[test]
    fn a_chat_shown_again_opens_on_its_last_page() {
        let mut c = Conv::default();
        c.merge(page("s", 0, 300, &[0, 100, 200]), false);
        // a last page beginning beyond what is held takes its place; what lies between is read on scrolling up
        let mut far = std::mem::take(&mut c);
        far.merge_last(page("s", 5000, 5300, &[5000, 5200]));
        assert_eq!((far.start, far.next, far.entries.last().map(|e| e.offset)), (5000, 5300, Some(5200)));
        // one beginning within what is held is joined to it, each entry once
        c.merge(page("s", 0, 300, &[0, 100, 200]), false);
        c.merge_last(page("s", 200, 500, &[200, 400]));
        assert_eq!(c.entries.iter().map(|e| e.offset).collect::<Vec<_>>(), [0, 100, 200, 400]);
        assert_eq!((c.start, c.next), (0, 500));
        // a session moved to a new file: the new one in its place
        c.merge_last(page("t", 0, 50, &[0]));
        assert_eq!((c.session.as_str(), c.entries.len(), c.next), ("t", 1, 50));
    }
}
