//! A box's state as yaaihh holds it: the last snapshot with every event after it applied (design §7: readers draw
//! from the stream, never from files).

use coord_types::{AgentView, Body, Entry, Event, ItemRecord, MailView, NetRun, Quote, Transcript, ViewData};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct Live {
    /// The project file as JSON (Null when the box has none).
    pub project: Value,
    pub agents: Vec<AgentView>,
    pub mail: Mail,
    pub items: Vec<ItemRecord>,
    pub views: HashMap<String, ViewData>,
    /// The nets' latest runs (oldest first) and their check's refusals.
    pub nets: Vec<NetRun>,
    pub net_errors: Vec<String>,
    pub seq: u64,
    /// Counts the changes to `views`: a view's page is built again when it moves.
    pub views_rev: u64,
}

/// How many net runs are kept.
const NET_RUNS: usize = 200;

impl Live {
    /// Apply EV; the agent it changed, if it is an agent's.
    pub fn apply(&mut self, ev: Event) -> Option<String> {
        self.seq = ev.seq;
        match ev.body {
            Body::Snapshot(s) => {
                self.project = s.project.unwrap_or(Value::Null);
                self.agents = s.agents;
                let rev = self.mail.rev;
                self.mail = Mail::from(s.mail);
                self.mail.rev += rev;
                self.items = s.items;
                self.views = s.views.into_iter().map(|v| (v.id.clone(), v)).collect();
                self.views_rev += 1;
                self.nets = s.nets;
                self.net_errors = s.net_errors;
            }
            Body::Net(r) => {
                // a run sent again (its marks changed) replaces itself
                if let Some(x) = self.nets.iter_mut().find(|x| x.id == r.id) {
                    *x = r;
                    return None;
                }
                self.nets.push(r);
                if self.nets.len() > NET_RUNS {
                    self.nets.remove(0);
                }
            }
            Body::Neterrors(e) => self.net_errors = e,
            Body::Agent(a) => {
                let name = a.name.clone();
                match self.agents.iter_mut().find(|x| x.name == a.name) {
                    Some(x) => *x = a,
                    None => self.agents.push(a),
                }
                return Some(name);
            }
            Body::Mail(m) => self.mail.put(m),
            Body::Delivered { id, to, .. } => self.mail.delivered(&id, to),
            Body::Item(r) => self.items.push(r),
            Body::View(v) => {
                self.views.insert(v.id.clone(), v);
                self.views_rev += 1;
            }
        }
        None
    }

    /// The items closed by hand (coordd's records; the last record of an item decides).
    pub fn closed(&self) -> HashSet<&str> {
        let mut last: HashMap<&str, &str> = HashMap::new();
        for r in &self.items {
            if r.ev == "opened" || r.ev == "closed" {
                last.insert(&r.id, &r.ev);
            }
        }
        last.into_iter().filter(|(_, ev)| *ev == "closed").map(|(id, _)| id).collect()
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
        // roots: agents whose parent is not an agent of this box (the owner, a host session)
        for a in self.agents.iter().filter(|a| !names.contains(a.parent.as_str())) {
            out.push((0, a));
            walk(&self.agents, &a.name, 1, &mut out);
        }
        out
    }
}

/// The box's mail, oldest first (read as a slice), with its threads indexed: a view asks for a mail's thread on
/// every build, for every mail it shows, so the lookups are by id, never a walk of the whole list
/// .
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
}

impl Conv {
    /// Put a page in: the first, one before what is held, or one after it. False when the page is of another
    /// transcript than the one held (the session moved to a new file): what is held is dropped, to be read again.
    pub fn merge(&mut self, page: Transcript, older: bool) -> bool {
        self.refused = None;
        if self.loaded && page.session != self.session {
            *self = Conv::default();
            return false;
        }
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

fn keys(entries: &[Entry]) -> Vec<String> {
    let mut out = Vec::with_capacity(entries.len());
    let mut n = 0;
    for (i, e) in entries.iter().enumerate() {
        n = if i > 0 && entries[i - 1].offset == e.offset { n + 1 } else { 0 };
        out.push(format!("e{}-{n}", e.offset));
    }
    out
}
