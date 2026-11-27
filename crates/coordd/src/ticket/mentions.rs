//! Mentions (paper-research N-412 p166/p168/p169, threads piece 3): an id written in a post's words that is there in
//! this record — a ticket `N-412` (any capital letter), a post of one `N-412.p30`, an agent `@NAME` — is a mention,
//! and what it names gets a line back: which ticket's post named it, by whom and when, with that ticket's label.
//!
//! Nothing is recorded and nobody is woken; a post's words are never changed. The index is built at start from every
//! post's words (an agent's turn read at its place in its session file, as `coord label` reads it) and kept as posts
//! land. It keeps the ids as written; whether one is there (the ticket, that post of it, the agent) is asked when it is
//! shown, so a post read at start and the same post landing live name the same things.

use docket_machine::{Machine, Text, ViewMention, HIDDEN};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// An id as a post's words hold it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Id {
    /// `N-412`.
    Ticket(String),
    /// `N-412.p30`: the ticket as written and the post.
    Post(String, u32),
    /// `@NAME`, as NAME.
    Agent(String),
}

/// The ids TEXT holds, in order, once each: each must stand apart, with no letter, digit, `_` or `-` right before it
/// (`a@b` is no mention, nor `BN-1`) and none right after. A post is `N-412.p30` as coordd writes it, or in words
/// `N-412 p30` or `N-412 post 30`, the full forms only (paper-research N-1446 p1366, 2: a commit's subject names his
/// post so, `owner N-1104 p678`); a bare `p30` names no ticket, and `N-412 p30x` is the ticket alone.
pub fn ids_in(text: &str) -> Vec<Id> {
    let cs: Vec<char> = text.chars().collect();
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let digits = |from: usize| cs.get(from..).map_or(0, |r| r.iter().take_while(|c| c.is_ascii_digit()).count());
    let mut out: Vec<Id> = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let apart = i == 0 || !(word(cs[i - 1]) || cs[i - 1] == '-');
        if apart && cs[i].is_ascii_uppercase() && cs.get(i + 1) == Some(&'-') && digits(i + 2) > 0 {
            let end = i + 2 + digits(i + 2);
            let ticket: String = cs[i..end].iter().collect();
            // its post's digits, start and end: in words, a post that runs on into a word is none, and the ticket
            // stands alone
            let written = |from: usize, w: &str| w.chars().enumerate().all(|(k, c)| cs.get(from + k) == Some(&c));
            let from = if written(end, ".p") {
                Some(end + 2)
            } else if written(end, " post ") {
                Some(end + 6)
            } else if written(end, " p") {
                Some(end + 2)
            } else {
                None
            };
            let post = from.filter(|f| digits(*f) > 0).map(|f| (f, f + digits(f)));
            let post = post.filter(|(_, e)| cs.get(end) == Some(&'.') || cs.get(*e).is_none_or(|c| !word(*c)));
            let stop = post.map_or(end, |(_, e)| e);
            if cs.get(stop).is_none_or(|c| !word(*c)) {
                let id = match post {
                    Some((f, e)) => cs[f..e].iter().collect::<String>().parse().map_or(Id::Ticket(ticket.clone()), |n| Id::Post(ticket, n)),
                    None => Id::Ticket(ticket),
                };
                if !out.contains(&id) {
                    out.push(id);
                }
            }
            i = stop;
            continue;
        }
        if apart && cs[i] == '@' {
            let n = cs[i + 1..].iter().take_while(|c| word(**c) || **c == '-').count();
            let name: String = cs[i + 1..i + 1 + n].iter().collect::<String>().trim_end_matches('-').to_string();
            let id = Id::Agent(name);
            if !matches!(&id, Id::Agent(s) if s.is_empty()) && !out.contains(&id) {
                out.push(id);
            }
            i += 1 + n;
            continue;
        }
        i += 1;
    }
    out
}

/// The posts that name each id, and the ids each post names.
#[derive(Debug, Default)]
pub struct Mentions {
    /// By ticket id as written (`N-412`): the posts (ticket, post) naming it, or (OF) one of its posts, in reading order.
    tickets: HashMap<String, Vec<(u64, u32, Option<u32>)>>,
    /// By agent NAME: the posts naming it.
    agents: HashMap<String, Vec<(u64, u32)>>,
    /// By post (ticket, post): the ids its words hold, as written, none of its own ticket's.
    holds: BTreeMap<(u64, u32), Vec<Id>>,
    /// By ticket, the last of its posts read.
    read: HashMap<u64, u32>,
}

/// Ticket ID's number, when ID is a shown ticket's id as the machine writes it (`Machine::ticket_named`, the test
/// `coord label` makes), and not hidden.
fn shown_ticket(m: &Machine, id: &str) -> Option<u64> {
    m.ticket_named(id).filter(|t| *t < HIDDEN)
}

/// Whether post N of T was taken back.
fn taken_back(m: &Machine, t: u64, n: u32) -> bool {
    m.tickets.get(&t).is_some_and(|x| x.posts.iter().any(|p| p.n == n && p.taken_back))
}

/// Whether ticket T has post N.
fn has_post(m: &Machine, t: u64, n: u32) -> bool {
    m.tickets.get(&t).is_some_and(|x| x.posts.iter().any(|p| p.n == n))
}

/// IDS as they are there now, in order, once each: a shown ticket's id, a post of one (a post its ticket does not have,
/// as its ticket), an agent's name (`@NAME` as NAME) when AGENT says it is one; the rest left out. The one reading of
/// what written ids name, for a post's words ([`Mentions::held`]) and a commit's ([`commits_into`]; paper-research
/// N-1446 p1366).
fn there_now(m: &Machine, agent: &dyn Fn(&str) -> bool, ids: &[Id]) -> Vec<String> {
    let mut shown: Vec<String> = Vec::new();
    for id in ids {
        let s = match id {
            Id::Ticket(k) => shown_ticket(m, k).map(|_| k.clone()),
            Id::Post(k, p) => shown_ticket(m, k).map(|u| if has_post(m, u, *p) { format!("{k}.p{p}") } else { k.clone() }),
            Id::Agent(a) => agent(a).then(|| a.clone()),
        };
        if let Some(s) = s.filter(|s| !shown.contains(s)) {
            shown.push(s);
        }
    }
    shown
}

/// The node in G of ID, one [`there_now`] gives: a post, a ticket or an agent, from SOURCE at WHEN unless it is there.
fn node_of(m: &Machine, g: &mut docket_machine::graph::Graph, id: String, source: docket_machine::graph::Source, when: Option<docket_machine::At>) -> String {
    use docket_machine::graph::{NodeKind, agent_id};
    if docket_machine::post_ref(&id).is_some() {
        g.node_from(id, NodeKind::Post, source, when)
    } else if shown_ticket(m, &id).is_some() {
        g.node_from(id, NodeKind::Ticket, source, when)
    } else {
        g.node_from(agent_id(&id), NodeKind::Agent, source, when)
    }
}

/// The record graph's edges from git's commits (paper-research N-1446 p1364, p1370; docket-edge8's half of p1376 1): a
/// commit in G that an agent here wrote (its `Agent:`, as AGENT says) → each id its `Item:` trailers and its subject
/// hold ([`ids_in`]: his post in words too, `owner N-1104 p678`) that is there now ([`there_now`]), `named`, from the
/// commit at its time. Every coord numbers its tickets from 1, so a commit by no agent here names nothing in this
/// record; one G does not have is left out (`graph_git::add` puts git's in first).
pub fn commits_into(m: &Machine, agent: &dyn Fn(&str) -> bool, commits: &[docket_machine::graph_git::Commit], g: &mut docket_machine::graph::Graph) {
    use docket_machine::graph::{EdgeKind, Source};
    for c in commits {
        let from = docket_machine::graph_git::commit_id(&c.hash);
        let Some(when) = g.get(&from).map(|n| n.when.clone()) else { continue };
        if !c.agent.split(',').map(str::trim).any(|a| agent(a)) {
            continue;
        }
        let mut ids: Vec<Id> = c.items.iter().flat_map(|v| ids_in(v)).collect();
        ids.extend(ids_in(&c.subject));
        let source = Source::Commit(c.hash.chars().take(8).collect());
        for id in there_now(m, agent, &ids) {
            let to = node_of(m, g, id, source.clone(), when.clone());
            g.edge_from(&from, &to, EdgeKind::Named, source.clone(), when.clone());
        }
    }
}

impl Mentions {
    /// Read the posts of T not read yet (READ a text's words): the ids each holds go in the index, none of T's own (a
    /// post naming its own ticket or one of its posts names nothing). Gives the ids of the tickets they name, whose
    /// views change.
    pub fn read_posts(&mut self, m: &Machine, read: &dyn Fn(&Text) -> Option<String>, t: u64) -> BTreeSet<String> {
        let mut named = BTreeSet::new();
        let Some(x) = m.tickets.get(&t).filter(|_| t < HIDDEN) else { return named };
        let upto = self.read.get(&t).copied().unwrap_or(0);
        let Some(last) = x.posts.iter().map(|p| p.n).max().filter(|n| *n > upto) else { return named };
        let own = m.id(t);
        for p in x.posts.iter().filter(|p| p.n > upto) {
            let Some(words) = read(&m.text(&p.text).unwrap_or_else(|| p.text.clone())) else { continue };
            let ids: Vec<Id> = ids_in(&words).into_iter().filter(|id| !matches!(id, Id::Ticket(k) | Id::Post(k, _) if *k == own)).collect();
            for id in &ids {
                match id {
                    Id::Ticket(k) => self.tickets.entry(k.clone()).or_default().push((t, p.n, None)),
                    Id::Post(k, n) => self.tickets.entry(k.clone()).or_default().push((t, p.n, Some(*n))),
                    Id::Agent(a) => self.agents.entry(a.clone()).or_default().push((t, p.n)),
                }
                if let Id::Ticket(k) | Id::Post(k, _) = id {
                    named.insert(k.clone());
                }
            }
            if !ids.is_empty() {
                self.holds.insert((t, p.n), ids);
            }
        }
        self.read.insert(t, last);
        named
    }

    /// The tickets post N of T names (their ids as written): whose lines back change when it is taken back.
    pub fn named_by(&self, t: u64, n: u32) -> Vec<String> {
        let mut out: Vec<String> = self.holds.get(&(t, n)).into_iter().flatten().filter_map(|id| match id {
            Id::Ticket(k) | Id::Post(k, _) => Some(k.clone()),
            Id::Agent(_) => None,
        }).collect();
        out.sort();
        out.dedup();
        out
    }

    /// Read every shown ticket's posts (at start).
    pub fn read_all(&mut self, m: &Machine, read: &dyn Fn(&Text) -> Option<String>) {
        for t in m.tickets.range(..HIDDEN).map(|(t, _)| *t).collect::<Vec<_>>() {
            self.read_posts(m, read, t);
        }
    }

    /// The ids each post of T holds that are there now, as written (`@NAME` as NAME; a post that T's ticket does not
    /// have, as its ticket), by post number; posts with none left out. AGENT says which names are agents.
    pub fn held(&self, m: &Machine, agent: &dyn Fn(&str) -> bool, t: u64) -> BTreeMap<u32, Vec<String>> {
        let mut out = BTreeMap::new();
        for ((_, n), ids) in self.holds.range((t, 0)..=(t, u32::MAX)) {
            // a post taken back leaves the views, and what it named with it (docket-core3, N-458 p10)
            if taken_back(m, t, *n) {
                continue;
            }
            let shown = there_now(m, agent, ids);
            if !shown.is_empty() {
                out.insert(*n, shown);
            }
        }
        out
    }

    /// The record graph's `named` edges (paper-research N-1446 p1337): each post → each id its words hold that is there
    /// ([`held`](Self::held)), a post, a ticket or an agent, at the line the post came on and its time (its node's in
    /// G); a post G does not have, or has from no line, is left out. AGENT says which names are agents.
    pub fn named_into(&self, m: &Machine, agent: &dyn Fn(&str) -> bool, g: &mut docket_machine::graph::Graph) {
        let ts: BTreeSet<u64> = self.holds.keys().map(|(t, _)| *t).collect();
        for t in ts {
            self.named_of_into(m, agent, g, t, 0);
        }
    }

    /// [`named_into`](Self::named_into) for T's posts after post AFTER: those a landing read just gave
    /// ([`read_posts`](Self::read_posts) from [`read_upto`](Self::read_upto)), so coordd's graph, kept along each input,
    /// gains their `named` edges as they come (proto-window N-1446 p1383, head 1).
    pub fn named_of_into(&self, m: &Machine, agent: &dyn Fn(&str) -> bool, g: &mut docket_machine::graph::Graph, t: u64, after: u32) {
        use docket_machine::graph::{EdgeKind, Source, post_id};
        for (n, ids) in self.held(m, agent, t).into_iter().filter(|(n, _)| *n > after) {
            let from = post_id(m, t, n);
            let Some((line, when)) = g.get(&from).and_then(|x| Some((x.line()?, x.when.clone()))) else { continue };
            for id in ids {
                let to = node_of(m, g, id, Source::Line(line), None);
                g.edge_from(&from, &to, EdgeKind::Named, Source::Line(line), when.clone());
            }
        }
    }

    /// The lines back for ID, oldest first: a ticket's (the posts naming it or one of its posts that it has), an agent's
    /// by NAME (when AGENT says it is one), or a post's `N-412.p30`. LABEL gives a ticket's label. None for anything else.
    pub fn lines(&self, m: &Machine, agent: &dyn Fn(&str) -> bool, label: &dyn Fn(u64) -> String, id: &str) -> Vec<ViewMention> {
        self.lines_where(m, agent, label, id, &|_, _| true)
    }

    /// The last post of T read (none read: 0): the posts after it are those a landing reads.
    pub fn read_upto(&self, t: u64) -> u32 {
        self.read.get(&t).copied().unwrap_or(0)
    }

    /// The lines back for ID from T's posts after UPTO only: what a landing on T added to ID's (`Body::Mentioned`),
    /// without labelling or sorting the rest.
    pub fn lines_new(&self, m: &Machine, agent: &dyn Fn(&str) -> bool, label: &dyn Fn(u64) -> String, id: &str, t: u64, upto: u32) -> Vec<ViewMention> {
        self.lines_where(m, agent, label, id, &|u, n| u == t && n > upto)
    }

    /// `lines`, of the naming posts (ticket, post) that KEEP keeps.
    fn lines_where(&self, m: &Machine, agent: &dyn Fn(&str) -> bool, label: &dyn Fn(u64) -> String, id: &str, keep: &dyn Fn(u64, u32) -> bool) -> Vec<ViewMention> {
        let hits: Vec<(u64, u32, Option<u32>)> = if let Some((k, p)) = docket_machine::post_ref(id) {
            match shown_ticket(m, k) {
                Some(u) if has_post(m, u, p) => self.tickets.get(k).into_iter().flatten().filter(|h| h.2 == Some(p)).copied().collect(),
                _ => Vec::new(),
            }
        } else if let Some(u) = shown_ticket(m, id) {
            // a post it does not have names the ticket
            self.tickets.get(id).into_iter().flatten().map(|&(t, n, of)| (t, n, of.filter(|p| has_post(m, u, *p)))).collect()
        } else if agent(id) {
            self.agents.get(id).into_iter().flatten().map(|&(t, n)| (t, n, None)).collect()
        } else {
            Vec::new()
        };
        let mut labels: HashMap<u64, String> = HashMap::new();
        let mut out: Vec<ViewMention> = hits
            .into_iter()
            .filter(|&(t, n, _)| keep(t, n))
            .filter_map(|(t, n, of)| {
                let p = m.tickets.get(&t)?.posts.iter().find(|p| p.n == n).filter(|p| !p.taken_back)?;
                let label = labels.entry(t).or_insert_with(|| label(t)).clone();
                Some(ViewMention { by: m.id(t), post: n, label, who: p.by.clone(), at: p.at.clone(), of })
            })
            .collect();
        out.sort_by(|a, b| (&a.at, &a.by, a.post, a.of).cmp(&(&b.at, &b.by, b.post, b.of)));
        out.dedup();
        out
    }

    /// T's posts from N on as not read yet (the cost harness: a landing post's read).
    #[cfg(test)]
    pub fn forget(&mut self, t: u64, n: u32) {
        self.read.insert(t, n.saturating_sub(1));
        self.holds.retain(|(u, k), _| !(*u == t && *k >= n));
        for v in self.tickets.values_mut() {
            v.retain(|h| !(h.0 == t && h.1 >= n));
        }
        for v in self.agents.values_mut() {
            v.retain(|h| !(h.0 == t && h.1 >= n));
        }
    }

    /// The newest post that holds an id (the cost harness).
    #[cfg(test)]
    pub fn newest(&self, m: &Machine) -> Option<(u64, u32)> {
        self.holds.keys().filter_map(|&(t, n)| Some(((t, n), m.tickets.get(&t)?.posts.iter().find(|p| p.n == n)?.at.clone()))).max_by(|a, b| a.1.cmp(&b.1)).map(|(k, _)| k)
    }

    /// How many posts hold an id, and how many ids they hold (for the start's line).
    pub fn size(&self) -> (usize, usize) {
        (self.holds.len(), self.holds.values().map(Vec::len).sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ids_a_posts_words_hold() {
        let t = |s: &str| Id::Ticket(s.into());
        let p = |s: &str, n| Id::Post(s.into(), n);
        let a = |s: &str| Id::Agent(s.into());
        assert_eq!(ids_in("see N-412 and N-412.p30, then Q-445."), vec![t("N-412"), p("N-412", 30), t("Q-445")]);
        assert_eq!(ids_in("@docket-core4: and @paper-research- and (@yaaihh)"), vec![a("docket-core4"), a("paper-research"), a("yaaihh")]);
        // not apart, not as coordd writes it, or nothing after the dash
        assert_eq!(ids_in("BN-412 n-412 a@b N- N-12x x-N-3 N-412.px"), vec![t("N-412")]);
        // once each, in the order written; `.p` with no digits is the ticket
        assert_eq!(ids_in("N-1 N-1 `N-2.p3` N-2.p3 N-1.p"), vec![t("N-1"), p("N-2", 3)]);
        // in words, the full forms only (paper-research N-1446 p1366): a bare `p678` names no ticket, a post that runs on
        // into a word is the ticket alone
        assert_eq!(ids_in("N-412 p166 é@x"), vec![p("N-412", 166)]);
        assert_eq!(ids_in("(owner N-1104 p678); N-1446 post 1337, N-7 p2x, N-8 posts 3, His p678, N-9 post, N-10 p"), vec![p("N-1104", 678), p("N-1446", 1337), t("N-7"), t("N-8"), t("N-9"), t("N-10")]);
        assert_eq!(ids_in("N-412 p30 and N-412.p30"), vec![p("N-412", 30)], "one post, however written");
    }
}
