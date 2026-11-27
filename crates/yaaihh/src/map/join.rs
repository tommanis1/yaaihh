//! The Map's one graph, from what the window holds (I-1478, the wiring: proto-lead N-1479 p55). Each project is an
//! island laid out by layout.rs [`layout::lay`] in the places map.json keeps; the islands are joined into the one
//! graph the canvas draws (canvas.rs `View::set_laid`; `View::set_graph` stays the made-up record's and the tests');
//! and what the layout says its far levels draw is put on the nodes:
//!
//! - **a line's posts**, each a node in its row ([`layout::Rows::cell`]), from the snapshot's view of the line (a line
//!   whose ticket the snapshot does not hold, closed over a day ago, shows none);
//! - **a handler's runs** in Programs, each a node in its row (`run:{run}`, docket-edge6 N-1479 p25), oldest first
//!   ([`layout::handled`]), drawn by what it came to; the handler saying how many, what most came to and when it last
//!   ran. A run's row is its place among the handler's runs the snapshot holds: when a ticket leaves the snapshot (a
//!   day after it closed) its runs go, and the handler's later runs move up as many rows;
//! - **his posts, lit** ([`layout::Lit`]): his of the last day, and the older ones still open; one that asks agents
//!   ringed ([`super::Ring`]), saying whom it asks and who has answered, with a trail ([`Link::Trail`]) to each
//!   answer; an agent's post that asks him, ringed while it waits on him;
//! - **the agents held** on a limit, each linked to what holds them ([`layout::Cluster`]: p715 path 2, "a red cluster
//!   of agents points at one node"), the holder saying how many, on which model, until when;
//! - **the accounts' meters** ([`layout::Meter`]) as their nodes' second line;
//! - **the island's badge** ([`layout::Badge`]) as its project's.
//!
//! **When.** Laid only when what is read of an island changes: what the layout reads ([`layout::key`]) and the posts
//! drawn on its lines ([`posts_key`]); not for an agent's tools, an accounts answer with the same meters, nor a
//! ticket's event that changes nothing read (proto-window N-1479 p68 hold 1, p116; consult-3 p115, p133). Or when a
//! state laid changes by itself ([`next_change`]: a limit ends, an account takes requests, his post leaves the last
//! day, a day ends and the times its notes write change: consult-3 p184). Each poll compares counts only, the events'
//! (`Live::tickets_rev`, `Live::agents_rev`, the accounts answers, Insight's rows, the protocol), and makes again only
//! the part of the keys whose count moved (p134). Not at each frame (p29: `lay` compares the whole of the places). A
//! relayout keeps what it does not change: the labels drawn and an open post's steps (canvas.rs `View::put`, consult-3
//! p69).
//!
//! **The places** are read from map.json once (layout.rs [`layout::read`]). One this window cannot read (a later
//! window's shape, or not JSON) is left byte for byte: nothing is written over it, and the places are made and held in
//! memory meanwhile (as the lens pins' file, consult-3 N-1446 p326). No window before this one reads map.json, so the
//! way back is untouched.
//!
//! **The lines** are Insight (3)'s rows (`Request::Insight { OpenToDone }`, asked once of each coordd that has it,
//! insight.rs `ask`), with the snapshot's own lines over them, as they are newer than rows asked once; the snapshot's
//! alone from a coordd before `INSIGHT_PROTOCOL`, one whose rows were refused, or one that cannot be asked; none while
//! the rows are on their way (the island is drawn where it goes and kept once they come: layout.rs `lay`).

use super::canvas::Counts;
use super::layout::{self, Island, Kept, Laid};
use super::{Edge, Graph, Kind, Link, Node, Rect, Refused, Ring, State, Tier};
use crate::App;
use coord_types::Insight;
use docket_machine::insight::Thread;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

/// What was laid last, and the places it was laid in (`View::laying`).
#[derive(Default)]
pub struct Laying {
    /// What the last lay was laid from, each project's [`Keys`].
    laid: Option<Vec<Keys>>,
    /// Each project's revisions as the last poll saw them ([`Revs`]), and its keys made then.
    seen: Vec<(Revs, Keys)>,
    /// The first time after the last lay that a state laid changes by itself (seconds): laid again then.
    until: Option<i64>,
    /// The places, as read from map.json and given what each lay added.
    kept: Kept,
    /// Whether map.json was read: why not, when it is there and cannot be (then nothing is written over it).
    read: Option<Result<(), String>>,
    /// Where the places are kept: map.json beside ui.json ([`layout::path`]); a test's own file.
    pub at: Option<PathBuf>,
    /// How many times the islands were laid.
    pub lays: usize,
    /// How many times an island's lines' and tickets' key was made (a test's count: an agent's event makes none).
    pub lines_keyed: usize,
    /// Why an island was not laid, by its project's name: written to yaaihh.log when it first says it.
    pub refused: Vec<String>,
    /// Why map.json was not written, the last time it was not.
    unwritten: Option<String>,
}

/// What a project's island is laid from as its events count it: its lines' (its protocol, its tickets' revision, when
/// Insight's rows came, whether they are asked), its agents' revision, how many accounts answers came. Compared at each
/// poll: three counts, nothing hashed (proto-window N-1479 p134). One moved, the part of the [`Keys`] it stands for is
/// made again.
#[derive(Clone, Debug, PartialEq)]
struct Revs {
    id: String,
    lines: (u32, u64, Option<i64>, bool),
    agents: u64,
    accounts: u64,
}

fn revs_of(s: &App, pi: usize) -> Revs {
    let p = &s.projects[pi];
    let asked = p.pending.values().any(|w| matches!(w, crate::Pending::Insight(Insight::OpenToDone)));
    Revs {
        id: p.id.clone(),
        lines: (p.protocol, p.live.tickets_rev, s.insights.threads.get(&pi).map(|r| r.1), asked),
        agents: p.live.agents_rev,
        accounts: crate::accounts::answers(s),
    }
}

/// What a project's island is laid from, as what is read of it: the layout's three parts ([`layout::key`]) and the
/// posts this module draws on its lines ([`posts_key`]). The islands are laid again when one differs from the last
/// lay's (proto-window N-1479 p68 hold 1, p116; consult-3 p115, p133): not for an agent's tools, nor an answer with the
/// same meters, nor a ticket's event that changes nothing read.
#[derive(Clone, Debug, PartialEq)]
struct Keys {
    lines: u64,
    agents: u64,
    accounts: u64,
}

/// What `put` reads of the posts it draws on the lines, as one number: the label's words, by whom and when, whether
/// shown and not moved. The layout reads none of these but his posts' times (consult-3 N-1479 p133); a post's words can
/// come after it, as a part (proto-window p134). And of each run a ticket lists what its node says and the layout does
/// not read (`layout::key_lines` reads the rest): what it came to, the post it ran for, when it opened.
fn posts_key(tickets: &BTreeMap<u64, coord_types::TicketView>) -> u64 {
    let mut h = DefaultHasher::new();
    for t in tickets.values() {
        (&t.id, t.posts.len()).hash(&mut h);
        for p in &t.posts {
            (p.n, p.visible(), p.moved.is_some(), &p.by, &p.at, post_label(p)).hash(&mut h);
        }
        for r in &t.ran {
            (&r.id, &r.said, r.post, &r.opened_at).hash(&mut h);
        }
    }
    h.finish()
}

/// The first time after NOW that a state laid changes by itself: an agent's limit ends, an account takes requests
/// again, a post of his goes out of the last day (layout.rs [`layout::last_day_ends`], proto-lead N-1479 p124), the
/// day ends, so the times the notes write as of the lay (`put`) take their weekday, or their date, as `when::at`
/// writes them (`when::next_day`; consult-3 p184: after midnight a quiet Map said yesterday's 17:55 as today's).
fn next_change(s: &App, now: i64) -> Option<i64> {
    let agents = s.projects.iter().flat_map(|p| &p.live.agents).map(|a| a.limit_until);
    let accounts = (0..s.projects.len()).filter_map(|pi| crate::accounts::answer(s, pi)).flat_map(|n| n.accounts.iter().map(|a| a.out_until));
    let days = s.projects.iter().map(|p| layout::last_day_ends(&p.live.tickets, now));
    agents.chain(accounts).chain(days).chain([crate::when::next_day(now)]).flatten().filter(|t| *t > now).min()
}

/// The Map's graph laid again at NOW (seconds) when what it is laid from changed (the module doc); whether it was. A
/// graph set by hand (the made-up record's, a test's: `View::set_graph`) is kept.
pub fn keep(s: &mut App, now: i64) -> bool {
    if s.map.fixed() {
        return false;
    }
    for pi in 0..s.projects.len() {
        crate::insight::ask(s, pi, Insight::OpenToDone);
    }
    // the events' counts first; where one moved, the part of the keys it stands for made again
    let revs: Vec<Revs> = (0..s.projects.len()).map(|pi| revs_of(s, pi)).collect();
    let due = s.map.laying.until.is_some_and(|t| t <= now);
    let l = &s.map.laying;
    if l.laid.is_some() && !due && l.seen.len() == revs.len() && l.seen.iter().zip(&revs).all(|((was, _), r)| was == r) {
        return false;
    }
    let mut keys = Vec::with_capacity(revs.len());
    for (pi, r) in revs.iter().enumerate() {
        let was = s.map.laying.seen.iter().find(|(w, _)| w.id == r.id).cloned();
        let lines = match &was {
            Some((w, k)) if w.lines == r.lines => k.lines,
            _ => {
                s.map.laying.lines_keyed += 1;
                let (p, ls) = (&s.projects[pi], lines(s, pi));
                let isl = Island { id: &p.id, name: &p.name, lines: &ls, tickets: &p.live.tickets, agents: &p.live.agents, accounts: None };
                let mut h = DefaultHasher::new();
                (layout::key_lines(&isl), posts_key(&p.live.tickets)).hash(&mut h);
                h.finish()
            }
        };
        let agents = match &was {
            Some((w, k)) if w.agents == r.agents => k.agents,
            _ => layout::key_agents(&s.projects[pi].live.agents),
        };
        let accounts = match &was {
            Some((w, k)) if w.accounts == r.accounts => k.accounts,
            _ => layout::key_accounts(crate::accounts::answer(s, pi)),
        };
        keys.push(Keys { lines, agents, accounts });
    }
    let same = s.map.laying.laid.as_ref() == Some(&keys);
    s.map.laying.seen = revs.into_iter().zip(keys.iter().cloned()).collect();
    if same && !due {
        return false;
    }
    let mut kept = std::mem::take(&mut s.map.laying.kept);
    let at = s.map.laying.at.clone().unwrap_or_else(layout::path);
    if s.map.laying.read.is_none() {
        let read = layout::read(&at).map(|k| kept = k);
        if let Err(e) = &read {
            crate::log::line(&format!("map: {} is left as it is, and the places are not kept: {e}", at.display()));
        }
        s.map.laying.read = Some(read);
    }
    let (j, changed) = laid(s, &mut kept, now);
    let until = next_change(s, now);
    let l = &mut s.map.laying;
    l.lays += 1;
    l.laid = Some(keys);
    l.until = until;
    if changed && l.read == Some(Ok(())) {
        let wrote = layout::write(&at, &kept).map_err(|e| e.to_string()).err();
        if wrote.is_some() && wrote != l.unwritten {
            crate::log::line(&format!("map: {} not written: {}", at.display(), wrote.as_deref().unwrap_or_default()));
        }
        l.unwritten = wrote;
    }
    l.kept = kept;
    for r in j.refused.iter().filter(|r| !l.refused.contains(r)) {
        crate::log::line(&format!("map: an island not laid: {r}"));
    }
    l.refused = j.refused;
    s.map.set_laid(j.graph, j.badges);
    true
}

/// Project PI's lines (the module doc), in the order of their numbers.
fn lines(s: &App, pi: usize) -> Vec<Thread> {
    let p = &s.projects[pi];
    let own = layout::lines_of(&p.live.tickets);
    let asked = p.pending.values().any(|w| matches!(w, crate::Pending::Insight(Insight::OpenToDone)));
    let rows = match s.insights.threads.get(&pi) {
        Some((Ok(rows), _)) => rows,
        None if asked => return Vec::new(),
        _ => return own,
    };
    let number = |id: &str| id.trim_start_matches(|c: char| !c.is_ascii_digit()).parse::<u64>().unwrap_or(u64::MAX);
    let mut by: BTreeMap<(u64, String), Thread> = rows.iter().map(|x| ((number(&x.id), x.id.clone()), x.clone())).collect();
    for x in own {
        by.insert((number(&x.id), x.id.clone()), x);
    }
    by.into_values().collect()
}

/// The window's islands laid at NOW in KEPT, and joined; and whether KEPT changed.
fn laid(s: &App, kept: &mut Kept, now: i64) -> (Joined, bool) {
    let lines: Vec<Vec<Thread>> = (0..s.projects.len()).map(|pi| lines(s, pi)).collect();
    let islands: Vec<Island> = s
        .projects
        .iter()
        .enumerate()
        .map(|(pi, p)| Island { id: &p.id, name: &p.name, lines: &lines[pi], tickets: &p.live.tickets, agents: &p.live.agents, accounts: crate::accounts::answer(s, pi) })
        .collect();
    let (laid, changed) = layout::lay(&islands, kept, now);
    (join(&islands, laid, now), changed)
}

/// The islands joined: the one graph, each project node's badge, and why an island was not laid.
pub struct Joined {
    pub graph: Graph,
    pub badges: Vec<(u32, Counts)>,
    pub refused: Vec<String>,
}

/// ISLANDS as LAID (one each, in their order) joined into one graph at NOW, each island's nodes after the last's,
/// with what the far levels draw put on them (the module doc). An island is left out, and said, when the layout
/// refused it or a project of its id is in already.
pub fn join(islands: &[Island], laid: Vec<Result<Laid, Refused>>, now: i64) -> Joined {
    let mut j = Joined { graph: Graph::default(), badges: Vec::new(), refused: Vec::new() };
    for (isl, l) in islands.iter().zip(laid) {
        let l = match l {
            Ok(l) => l,
            Err(e) => {
                j.refused.push(format!("{}: {e:?}", isl.name));
                continue;
            }
        };
        if j.graph.project(&l.graph.node(0).id).is_some() {
            j.refused.push(format!("{}: a second project of the id {}", isl.name, isl.id));
            continue;
        }
        let off = j.graph.nodes().len() as u32;
        put(&mut j.graph, isl, &l, off, now);
        let b = l.badge;
        j.badges.push((off, Counts { waits: b.waits as u32, held: b.held as u32, stopped: b.stopped as u32 }));
    }
    j
}

/// Island L (of ISL) into G, its nodes from OFF on. Its nodes and links were checked as the layout made them, each
/// inside its parent, and keep their order, so none is refused here; a post refused is left out and said. The times
/// its notes say are written as of NOW, the lay's time, which `next_change` lays again at each midnight.
fn put(g: &mut Graph, isl: &Island, l: &Laid, off: u32, now: i64) {
    let lg = &l.graph;
    let today = crate::when::local(now);
    for n in lg.nodes() {
        let mut n = n.clone();
        n.parent = n.parent.map(|p| p + off);
        if n.parent.is_none() {
            n.rect = fitted(lg, n.rect);
        }
        if let Err(e) = g.push(n) {
            crate::log::line(&format!("map: {}'s node not joined: {e:?}", isl.name));
        }
    }
    for e in lg.edges() {
        let _ = g.link(Edge { from: e.from + off, to: e.to + off, ..*e });
    }
    // a line's posts, each in its row: top-down by number, a row the same for as long as the line is drawn
    let views: HashMap<&str, &coord_types::TicketView> = isl.tickets.values().map(|t| (t.id.as_str(), t)).collect();
    for (&li, rows) in &l.rows {
        let line = lg.node(li);
        let Some(t) = views.get(line.id.as_str()) else { continue };
        for p in t.posts.iter().filter(|p| p.visible() && p.moved.is_none()) {
            let Some(c) = rows.cell(line.rect, p.n) else { break };
            let mut n = Node::new(format!("{}.p{}", t.id, p.n), Kind::Post, post_label(p), Some(li + off), within(c, line.rect));
            n.note = format!("p{} · {} · {}", p.n, if p.by == "owner" { "you" } else { p.by.as_str() }, crate::when::at_on(&p.at, today));
            if let Err(e) = g.push(n) {
                crate::log::line(&format!("map: {}'s post not joined: {e:?}", isl.name));
            }
        }
    }
    // a handler's runs, each in its row: oldest first, as many as its rows hold (layout.rs `handled`, as the layout
    // counted them); the handler says how many, what most came to and when it last ran
    let handled: HashMap<String, layout::Handled> = layout::handled(isl.tickets).into_iter().map(|h| (h.id.clone(), h)).collect();
    for (&hi, rows) in &l.rows {
        let at = lg.node(hi);
        let Some(h) = handled.get(&at.id).filter(|_| at.kind == Kind::Handler) else { continue };
        for (j, (under, r)) in h.runs.iter().enumerate() {
            let Some(c) = rows.cell(at.rect, u32::try_from(j + 1).unwrap_or(u32::MAX)) else { break };
            let mut n = Node::new(format!("run:{}", r.id), Kind::Run, run_label(r), Some(hi + off), within(c, at.rect));
            let what = r.post.map_or_else(|| under.to_string(), |p| format!("{under}.p{p}"));
            n.note = format!("for {what} · {}", crate::when::at_on(&r.opened_at, today));
            n.state = if r.closed_at.is_none() { State::Working } else { State::Plain };
            if let Err(e) = g.push(n) {
                crate::log::line(&format!("map: {}'s run not joined: {e:?}", isl.name));
            }
        }
        if let Some(note) = handler_note(&h.runs, today) {
            g.update(hi + off, |n| n.note = note);
        }
    }
    // his posts, lit; an open ask before his others where they would fall on one point. One that asks agents rings,
    // its trail to each answer: the answer's post where it is placed, else its line; none where the island has
    // neither, its note naming it all the same (consult-3 N-1479 p316 point 2), nor into what holds his post: an answer
    // on his own line past its last row, once it is full, has no cell, and its line is the card his post lies in
    // (p372). An agent's post that asks him rings in the waits-on-you ink while it waits on him (p315). What they say
    // of the ask goes first, as a hover cuts.
    for x in &l.lit {
        let Some(i) = g.find_in(off, &x.post) else { continue };
        if x.to_him {
            g.update(i, |n| {
                n.ring = Some(Ring::ToHim);
                n.tier = Tier::WaitsOnHim;
                n.weight = 1.0;
                n.note = format!("asks you · {}", n.note);
            });
            continue;
        }
        let holds = g.path(i);
        let answers: BTreeSet<u32> = x
            .asked
            .iter()
            .filter_map(|(_, a)| a.as_deref())
            .filter_map(|a| g.find_in(off, a).or_else(|| a.rsplit_once(".p").and_then(|(t, _)| g.find_in(off, t))))
            .filter(|to| !holds.contains(to))
            .collect();
        for to in answers {
            let _ = g.link(Edge::new(i, to, Link::Trail));
        }
        let ask = asks_note(&x.asked, &views, today);
        g.update(i, |n| {
            n.his = true;
            n.tier = if x.open { Tier::HisAsk } else { Tier::Rest };
            n.weight = if x.open { 1.0 } else { 0.0 };
            n.ring = (!x.asked.is_empty()).then_some(if x.asked.iter().any(|(_, a)| a.is_none()) { Ring::Open } else { Ring::Answered });
            match ask {
                Some(w) => n.note = format!("{w} · {}", n.note),
                None if x.open => n.note = format!("{} · no later post answers it", n.note),
                None => {}
            }
        });
    }
    // the accounts' meters as their second line
    for m in &l.meters {
        let mut w = Vec::new();
        if let Some(u) = m.used {
            w.push(format!("{:.0}% used", u * 100.0));
        }
        if let Some(r) = m.resets {
            w.push(format!("resets {}", crate::when::secs_on(r, today)));
        }
        if let Some(o) = m.out {
            w.push(format!("takes no request until {}{}", crate::when::secs_on(o, today), m.why.as_ref().map(|y| format!(" ({y})")).unwrap_or_default()));
        }
        if !w.is_empty() {
            g.update(m.node + off, |n| n.note = w.join(" · "));
        }
    }
    // the agents held, each linked to what holds them, which says how many, on which model, until when
    for c in &l.held {
        let until = c.until.filter(|t| *t > now).map(|t| format!(" until {}", crate::when::secs_on(t, today))).unwrap_or_default();
        let held = format!("held · {}{until}", c.model);
        for a in &c.agents {
            g.update(a + off, |n| n.note = held.clone());
            if let Some(on) = c.on {
                let _ = g.link(Edge::new(a + off, on + off, Link::Uses));
            }
        }
        if let Some(on) = c.on {
            let why = c.why.as_deref().map(|y| format!(" · {}", crate::refs::cut(y, 80))).unwrap_or_default();
            let line = format!("{} held · {}{until}{why}", c.agents.len(), c.model);
            g.update(on + off, |n| n.note = if n.note.is_empty() { line } else { format!("{}; {line}", n.note) });
        }
    }
}

/// What his post asking ASKED (each agent, with the post that answered it) says of them, as of the day TODAY: while
/// one has not answered, whom it asks and who has not ("asks w2, w3 · w3 has not answered"); once each has, when each
/// did ("w2 answered at 10:34, w3 at 11:02"), one whose answer the snapshot does not hold (VIEWS) by its name alone;
/// none when it asks nobody (a coordd before `ASKED_PROTOCOL` gives none).
fn asks_note(asked: &[(String, Option<String>)], views: &HashMap<&str, &coord_types::TicketView>, today: chrono::DateTime<chrono::Local>) -> Option<String> {
    if asked.is_empty() {
        return None;
    }
    let waiting: Vec<&str> = asked.iter().filter(|(_, a)| a.is_none()).map(|(w, _)| w.as_str()).collect();
    if !waiting.is_empty() {
        let all: Vec<&str> = asked.iter().map(|(w, _)| w.as_str()).collect();
        let has = if waiting.len() == 1 { "has" } else { "have" };
        return Some(format!("asks {} · {} {has} not answered", all.join(", "), waiting.join(", ")));
    }
    let at = |a: &str| {
        let (t, n) = a.rsplit_once(".p")?;
        let n: u32 = n.parse().ok()?;
        views.get(t)?.posts.iter().find(|p| p.n == n).map(|p| crate::when::at_on(&p.at, today))
    };
    let each: Vec<String> = asked
        .iter()
        .enumerate()
        .map(|(k, (w, a))| match (k, a.as_deref().and_then(|a| at(a))) {
            (0, Some(t)) => format!("{w} answered at {t}"),
            (_, Some(t)) => format!("{w} at {t}"),
            (_, None) => format!("{w} answered"),
        })
        .collect();
    Some(each.join(", "))
}

/// A run's label: what it came to, in a line (`ViewRan::said`); while it goes, that it does.
fn run_label(r: &docket_machine::view::ViewRan) -> String {
    match &r.said {
        Some(s) if !s.trim().is_empty() => crate::refs::cut(&crate::sidebar::first_line(s), 80),
        _ if r.closed_at.is_none() => "running".into(),
        _ => "ended".into(),
    }
}

/// What a handler with RUNS (oldest first) says, as of the day TODAY: how many, as a thread's handler rows say it
/// (picture.rs `handler_row`: "ran 7 · last 08:16"), and what most of them came to with how many; none with no run.
fn handler_note(runs: &[(&str, &docket_machine::view::ViewRan)], today: chrono::DateTime<chrono::Local>) -> Option<String> {
    let (_, last) = runs.last()?;
    let mut came: BTreeMap<String, usize> = BTreeMap::new();
    for (_, r) in runs {
        *came.entry(run_label(r)).or_default() += 1;
    }
    let (most, n) = came.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))?;
    Some(format!("ran {} · last {} · most {most} ({n})", runs.len(), crate::when::at_on(&last.opened_at, today)))
}

/// A post's label: its first line of words, else its number (words held elsewhere are not read for the Map).
fn post_label(p: &docket_machine::Post) -> String {
    match &p.text {
        docket_machine::Text::New(t) if !t.trim().is_empty() => crate::refs::cut(&crate::sidebar::first_line(t), 80),
        _ => format!("post {}", p.n),
    }
}

/// Island ISL's frame fitted to what it holds (paper-research N-1479 p65: the room the layout keeps below was a third
/// of his first view, drawn blank). Its bottom edge is as far under its lowest node as its nodes start right of its
/// left edge; its corner and width are the layout's (its least width, or its meters' row reaching its right edge). The
/// room stays in the layout's own places (map.json): only the frame drawn, and the fits made to it, are shorter.
fn fitted(lg: &Graph, isl: Rect) -> Rect {
    let (mut left, mut lowest) = (f64::INFINITY, f64::NEG_INFINITY);
    for r in lg.nodes().iter().filter(|n| n.parent.is_some()).map(|n| n.rect) {
        left = left.min(r.x);
        lowest = lowest.max(r.bottom());
    }
    if !lowest.is_finite() {
        return isl;
    }
    Rect::new(isl.x, isl.y, isl.w, (lowest + (left - isl.x).max(0.0) - isl.y).clamp(0.0, isl.h))
}

/// C, a row of a line, within the line's rect R: a row's sum can pass R's edge by a rounding.
fn within(c: Rect, r: Rect) -> Rect {
    let (x, y) = (c.x.max(r.x), c.y.max(r.y));
    Rect::new(x, y, (c.right().min(r.right()) - x).max(0.0), (c.bottom().min(r.bottom()) - y).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::made_up;

    /// A handler's runs in its rows (p715 path 3; docket-edge6 N-1479 p25): `run:{run}`, oldest first by number, each
    /// saying what it came to and what it was for, at work while it goes; the handler how many, when it last ran and
    /// what most came to. When the ticket of its oldest runs leaves the snapshot, they go, and the later runs move up as
    /// many rows: posts never move, runs can (proto-lead p209).
    #[test]
    fn a_handlers_runs_lie_in_its_rows_oldest_first_each_saying_what_it_came_to() {
        use serde_json::json;
        let now = made_up::NOW;
        let mut r = made_up::research(now);
        let at = |i: u64| chrono::DateTime::from_timestamp(now - 3_600 + 60 * i as i64, 0).unwrap().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let id = |i: u64| format!("O-{}", docket_machine::HIDDEN + i);
        let run = |i: u64, said: Option<&str>, post: Option<u32>| {
            json!({"id": id(i), "event": "agent_post", "from": "thread", "line": 9, "post": post, "opened_at": at(i), "closed_at": said.map(|_| at(i)), "said": said})
        };
        // made up: thread's `when` on agent_post ran twice under N-3009, then four times under N-3000, listed out of
        // order there, the last still going
        let under = |r: &mut made_up::Snapshot, t: &str, ran: serde_json::Value| r.tickets.values_mut().find(|v| v.id == t).expect("in the snapshot").ran = serde_json::from_value(ran).unwrap();
        under(&mut r, "N-3009", json!([run(1, Some("Nothing"), None), run(2, Some("Nothing"), None)]));
        under(&mut r, "N-3000", json!([run(5, Some("Keep"), Some(1)), run(3, Some("Keep"), None), run(4, Some("To([pr])"), None), run(6, None, None)]));
        let joined = |r: &made_up::Snapshot, kept: &mut Kept| {
            let islands = [r.island("research")];
            let (laid, _) = layout::lay(&islands, kept, now);
            join(&islands, laid, now).graph
        };
        let mut kept = Kept::default();
        let g = joined(&r, &mut kept);
        let h = g.find_in(0, "handler:thread:agent_post").expect("laid in Programs");
        let runs: Vec<&Node> = g.children(h).iter().map(|i| g.node(*i)).collect();
        assert_eq!(runs.iter().map(|n| n.id.clone()).collect::<Vec<_>>(), (1..=6).map(|i| format!("run:{}", id(i))).collect::<Vec<_>>(), "oldest first, by number");
        assert!(runs.iter().all(|n| n.kind == Kind::Run && g.node(h).rect.holds(&n.rect)), "each in its row");
        assert!(runs.windows(2).all(|w| w[0].rect.y < w[1].rect.y), "top-down");
        assert_eq!(runs.iter().map(|n| n.label.as_str()).collect::<Vec<_>>(), ["Nothing", "Nothing", "Keep", "To([pr])", "Keep", "running"]);
        let today = crate::when::local(now);
        assert_eq!(runs[4].note, format!("for N-3000.p1 · {}", crate::when::at_on(&at(5), today)));
        assert_eq!(runs[0].note, format!("for N-3009 · {}", crate::when::at_on(&at(1), today)));
        assert_eq!((runs[5].state, runs[0].state), (State::Working, State::Plain));
        assert_eq!(g.node(h).note, format!("ran 6 · last {} · most Keep (2)", crate::when::at_on(&at(6), today)), "of two at most, the first by its words");
        // N-3009 leaves the snapshot: its two runs go, and N-3000's move up two rows each
        let rows: Vec<Rect> = runs.iter().map(|n| n.rect).collect();
        r.tickets.retain(|_, v| v.id != "N-3009");
        let g = joined(&r, &mut kept);
        let h = g.find_in(0, "handler:thread:agent_post").unwrap();
        let after: Vec<(String, Rect)> = g.children(h).iter().map(|i| (g.node(*i).id.clone(), g.node(*i).rect)).collect();
        assert_eq!(after, (3..=6).map(|i| (format!("run:{}", id(i)), rows[i as usize - 3])).collect::<Vec<_>>(), "each two rows up");
    }

    /// His asks on the Map (p715 path 1; docket-edge5 N-1479 p313; made_up.rs `asks`): p678, open, rings and says whom
    /// it asks and who has not answered; p676, answered by each it asked, rings with his dot and says when each did, its
    /// trail to p677 in his chat and to the reply tool's answer on N-3009, none to W-12's, which the island does not
    /// lay, though its note names it (consult-3 p316 point 2). An agent's post asking him rings in the waits-on-you
    /// ink and says so first; once its asks close (p315), it is a post as any other.
    #[test]
    fn his_asks_ring_and_their_trail_goes_to_each_answer_the_island_lays() {
        let now = made_up::NOW;
        let mut r = made_up::research(now);
        made_up::asks(&mut r.tickets, now);
        let joined = |r: &made_up::Snapshot| {
            let islands = [r.island("research")];
            let (laid, _) = layout::lay(&islands, &mut Kept::default(), now);
            join(&islands, laid, now).graph
        };
        let g = joined(&r);
        let today = crate::when::local(now);
        let at = |t: &str, n: u32| crate::when::at_on(&r.tickets.values().find(|v| v.id == t).unwrap().posts.iter().find(|p| p.n == n).unwrap().at, today);
        let node = |g: &Graph, id: &str| g.node(g.find_in(0, id).unwrap_or_else(|| panic!("{id} laid"))).clone();
        let p678 = node(&g, "N-1104.p678");
        assert_eq!((p678.ring, p678.tier, p678.his), (Some(Ring::Open), Tier::HisAsk, true));
        assert_eq!(p678.note, format!("asks paper-research · paper-research has not answered · p678 · you · {}", at("N-1104", 678)));
        let p676 = node(&g, "N-1104.p676");
        assert_eq!((p676.ring, p676.tier), (Some(Ring::Answered), Tier::Rest));
        assert_eq!(p676.note, format!("paper-research answered at {}, docket-edge8 at {}, docket-edge2 answered · p676 · you · {}", at("N-1104", 677), at("N-3009", 3), at("N-1104", 676)));
        let mut trail: Vec<(&str, &str)> = g.edges().iter().filter(|e| e.link == Link::Trail).map(|e| (g.node(e.from).id.as_str(), g.node(e.to).id.as_str())).collect();
        trail.sort();
        assert_eq!(trail, [("N-1104.p676", "N-1104.p677"), ("N-1104.p676", "N-3009.p3")], "to each answer the island lays, none to W-12's");
        // a post of his asking nobody is lit as before, with no ring
        let p674 = node(&g, "N-1104.p674");
        assert_eq!((p674.ring, p674.his, p674.note.contains("asks")), (None, true, false), "{p674:?}");
        // proto-window's ask of him
        let (ask, asked_at) = (node(&g, "N-3018.p1"), at("N-3018", 1));
        assert_eq!((ask.ring, ask.tier, ask.his), (Some(Ring::ToHim), Tier::WaitsOnHim, false));
        assert_eq!(ask.note, format!("asks you · p1 · proto-window · {asked_at}"));
        // its asks closed: no ring, no rank, its note a post's
        r.tickets.values_mut().find(|v| v.id == "N-3018").unwrap().posts[0].asks_open = false;
        let ask = node(&joined(&r), "N-3018.p1");
        assert_eq!((ask.ring, ask.tier, ask.note), (None, Tier::Rest, format!("p1 · proto-window · {asked_at}")));
    }

    /// His ask's trail where an answer's post is not placed (consult-3 N-1479 p372): to the answer's line when another
    /// laid line holds it; none when that line is his post's own, the answer past its last row once it is full, which
    /// would run from his post into the card that holds it. The note names each answer all the same.
    #[test]
    fn his_asks_trail_goes_to_an_unplaced_answers_line_but_never_into_the_line_holding_his_post() {
        let now = made_up::NOW;
        let mut r = made_up::research(now);
        made_up::asks(&mut r.tickets, now);
        let chat = r.tickets.values_mut().find(|v| v.id == "N-1104").unwrap();
        // his chat's 700 posts have 512 rows of 3 columns (layout.rs `rows_for`): its post 1600, paper-research's, has
        // no cell
        let mut late = chat.posts.iter().find(|p| p.n == 677).unwrap().clone();
        late.n = 1600;
        chat.posts.push(late);
        // his p678 answered there; docket-edge8's answer to his p676 on N-3000, a line laid whose p9 the snapshot lacks
        for a in chat.asked.iter_mut() {
            match (a.post, a.agent.as_str()) {
                (678, "paper-research") => a.answer = Some("N-1104.p1600".into()),
                (676, "docket-edge8") => a.answer = Some("N-3000.p9".into()),
                _ => {}
            }
        }
        let islands = [r.island("research")];
        let (laid, _) = layout::lay(&islands, &mut Kept::default(), now);
        let g = join(&islands, laid, now).graph;
        assert!(g.find_in(0, "N-1104.p1600").is_none() && g.find_in(0, "N-3000.p9").is_none() && g.find_in(0, "N-3000").is_some(), "neither answer placed, N-3000 laid");
        let mut trail: Vec<(&str, &str)> = g.edges().iter().filter(|e| e.link == Link::Trail).map(|e| (g.node(e.from).id.as_str(), g.node(e.to).id.as_str())).collect();
        trail.sort();
        assert_eq!(trail, [("N-1104.p676", "N-1104.p677"), ("N-1104.p676", "N-3000")], "to N-3000's line; none from p678 into his chat");
        let p678 = g.node(g.find_in(0, "N-1104.p678").unwrap());
        assert!(p678.ring == Some(Ring::Answered) && p678.note.starts_with("paper-research answered at "), "{p678:?}");
    }

    /// The made-up snapshot joined (made_up.rs `snapshot`): a line's posts lie in its rows, his p678 is lit and open, the
    /// sixteen agents held point at the gateway, which says how many and until when; research's badge is its layout's.
    #[test]
    fn the_islands_are_joined_with_their_posts_his_lit_and_the_held_agents_pointing_at_the_gateway() {
        let now = made_up::NOW;
        let (r, c) = (made_up::research(now), made_up::cav(now));
        let islands = [r.island("research"), c.island("cav-paper")];
        let mut kept = Kept::default();
        let (laid, _) = layout::lay(&islands, &mut kept, now);
        let badge = laid[0].as_ref().unwrap().badge;
        let j = join(&islands, laid, now);
        assert!(j.refused.is_empty(), "{:?}", j.refused);
        let g = &j.graph;
        let (research, cav) = (g.project("project:research").unwrap(), g.project("project:cav-paper").unwrap());
        assert_eq!((research, j.badges[0].0, j.badges[1].0), (0, 0, cav));
        assert_eq!(j.badges[0].1, Counts { waits: badge.waits as u32, held: 16, stopped: badge.stopped as u32 });
        // N-1104's posts in its rows, top-down by number, each inside it
        let chat = g.find_in(research, "N-1104").unwrap();
        let posts: Vec<u32> = g.children(chat).iter().copied().filter(|i| g.node(*i).kind == Kind::Post).collect();
        assert_eq!(posts.len(), 700);
        assert!(posts.windows(2).all(|w| g.node(w[0]).rect.y < g.node(w[1]).rect.y || g.node(w[0]).rect.x < g.node(w[1]).rect.x), "by number");
        let p678 = g.node(g.find_in(research, "N-1104.p678").unwrap());
        assert!(p678.his && p678.tier == Tier::HisAsk && p678.note.ends_with("no later post answers it"), "{p678:?}");
        assert_eq!(p678.label, "d h m, please, on the Accounts tab");
        // the held agents' links, all into the gateway, which says so
        let gw = g.find_in(research, "account:gateway").unwrap();
        let into: Vec<&Edge> = g.edges().iter().filter(|e| e.to == gw && e.link == Link::Uses).collect();
        assert_eq!(into.len(), 16);
        assert!(into.iter().all(|e| g.node(e.from).kind == Kind::Agent && g.node(e.from).note.starts_with("held · claude-opus-4-5 until ")));
        assert!(g.node(gw).note.starts_with("16 held · claude-opus-4-5 until "), "{}", g.node(gw).note);
        // cav-paper's own nodes in its own project: an N-12 of each is two nodes
        assert!(g.find_in(cav, "N-12").is_some() && g.root_of(g.find_in(cav, "N-12").unwrap()) == cav);
        assert!(g.nodes().iter().all(|n| n.parent.is_none_or(|p| g.node(p).rect.holds(&n.rect))), "each inside its parent");
    }
}
