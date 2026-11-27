//! An agent's chat as two streams by who speaks to whom (owner N-315: "separte ... jsut hte human / owner comans and
//! the agnent ot agents coms"; N-315.p3: drawn as a messenger draws them; N-315.p5 and paper-research's ruling: views on
//! the threads that hold nothing of their own).
//!
//! - **With you**: every post between the owner and the agent, across their threads, in the order of time. His on the
//!   right, the agent's on the left. A change or question a message opened is one line under it.
//! - **With agents**: the same over the agent's posts with other agents (and coord's notices to it): its own on the
//!   right, the others' on the left with their names.
//!
//! Only messages: the turn that led to a reply (its tool calls, its reading) is behind "› the work behind it", the
//! session as it was. A thread's label marks where the stream moves to another thread; a click opens the thread. The
//! terminal stays the chat's third view.

use crate::App;
use coord_types::TicketView;
use docket_machine::Post;
use rune::{El, GrowthSpec, ScrollSpec, col, row, text};
use rune_kit::{Kit, SegmentedControl};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// Which of an agent's chat views shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Which {
    #[default]
    You,
    Agents,
    /// The session as it was, the work behind the replies (chat.rs's transcript).
    Work,
}

#[derive(Default)]
pub struct Streams {
    /// The view each (project, agent) chat shows.
    pub which: HashMap<(usize, String), Which>,
    /// Long messages opened whole, by `T.pN`.
    pub open: HashSet<String>,
    /// The (project, agent) pages whose row of presses is open behind its "⋯" (chat.rs `pane`): the stream switch,
    /// Raw log, System prompt, Initial prompt, Home and Session; shut, the row says only the stream shown.
    pub more: HashSet<(usize, String)>,
    /// coordd's reply to an effort set from an agent's session settings (settings.rs), by (project, agent).
    pub said: HashMap<(usize, String), String>,
    /// The model and effort picked in an agent's session settings and not applied yet (settings.rs), by (project,
    /// agent): None where the pick is what it runs.
    pub staged: HashMap<(usize, String), (Option<String>, Option<String>)>,
    /// Each stream's lines and its count of agents' posts held back (`waiting`), by (project id, agent, stream), with
    /// what they were built from (`Kept`). Built on every frame they took a frame tens of milliseconds on research's
    /// tickets (the owner, N-412: "fixing htat angets view is slwo as shit"); now when the tickets or the session move.
    kept: std::cell::RefCell<HashMap<(String, String, Which), (Kept, Rc<Vec<Line>>, usize)>>,
}

/// What a stream is built from: the tickets as their events left them (and their count and posts', for tickets put in
/// by hand, as the scenes and tests do), the agent's session held (its compactions), and today (its day lines say
/// "Today" and "Yesterday", `day_of`).
type Kept = (u64, usize, usize, Option<(bool, usize, u64, u64, String, usize)>, chrono::NaiveDate);

/// Stream W of agent A in project P, and how many of the agents' posts to A wait (`waiting`): kept in ST until what
/// they are built from moves.
fn kept(st: &Streams, p: &crate::Proj, a: &str, w: Which) -> (Rc<Vec<Line>>, usize) {
    let posts = p.live.tickets.values().map(|v| v.posts.len()).sum();
    let stamp: Kept = (p.live.tickets_rev, p.live.tickets.len(), posts, p.convs.get(a).map(crate::live::Conv::stamp), chrono::Local::now().date_naive());
    let key = (p.id.clone(), a.to_string(), w);
    if let Some((at, ls, held)) = st.kept.borrow().get(&key)
        && *at == stamp
    {
        return (ls.clone(), *held);
    }
    let ls = Rc::new(lines(p, a, w));
    let held = if w == Which::You { waiting(p, a) } else { 0 };
    st.kept.borrow_mut().insert(key, (stamp, ls.clone(), held));
    (ls, held)
}

/// One row of a stream.
#[derive(Clone, Debug, PartialEq)]
enum Line {
    /// A day begins.
    Day(String),
    /// The stream moves to thread T (by number).
    Thread(u64),
    /// Post N of thread T. HEAD: it begins a group (its sender's name and time above it, once for messages that
    /// follow on). TO: the owner's post it answers, quoted above it where that is not plain from where it stands.
    /// ID: it opens its thread, whose id it shows on hover rather than on a line of its own. LAST: the group's last
    /// message, which carries the foot. OPEN: the last of his run with no answer under it, a message of which the
    /// stream's agent still owes ([`owing`]; before `ASKED_PROTOCOL`, one coordd holds unanswered, `unanswered_owner`):
    /// "no answer yet", once for the run.
    Msg { t: u64, n: u32, head: bool, to: Option<(u64, u32)>, id: bool, last: bool, open: bool },
    /// Agent's post N of T answers his message TO, under which it is drawn: this line stands where it came in time.
    Moved(u64, u32, (u64, u32)),
    /// Ticket T, opened under a thread by the message above.
    Opened(u64),
    /// The agent's context was compacted here: the summary it left is the session's entry with this key, folded.
    Compacted(String),
}

/// The words Claude Code opens a compaction's summary with, the first prompt of what follows it.
const SUMMARY: &str = "This session is being continued from a previous conversation";

/// The compactions in agent A's session as held: each summary's entry key and time.
fn compactions(p: &crate::Proj, a: &str) -> Vec<(String, String)> {
    let Some(c) = p.convs.get(a) else { return Vec::new() };
    c.entries.iter().zip(&c.keys).filter(|(e, _)| e.kind == "user" && e.text.trim_start().starts_with(SUMMARY)).map(|(e, k)| (k.clone(), e.at.clone())).collect()
}

/// Each compaction of A put in OUT where it came in time: before the first message after it, above the day's and
/// thread's lines that head that message; after all of them when none came after it.
fn place_compactions(p: &crate::Proj, a: &str, out: &mut Vec<Line>) {
    let at_of = |l: &Line| match l {
        Line::Msg { t, n, .. } => p.live.tickets.get(t).and_then(|v| v.posts.iter().find(|x| x.n == *n)).map(|x| x.at.to_string()),
        _ => None,
    };
    for (key, at) in compactions(p, a) {
        let Some(when) = crate::turn::nanos(&at) else { continue };
        let later = out.iter().position(|l| at_of(l).and_then(|t| crate::turn::nanos(&t)).is_some_and(|t| t > when));
        let mut i = later.unwrap_or(out.len());
        while i > 0 && later.is_some() && matches!(out[i - 1], Line::Day(_) | Line::Thread(_)) {
            i -= 1;
        }
        out.insert(i, Line::Compacted(key));
    }
}

fn said(p: &Post) -> impl Iterator<Item = &String> {
    p.names.iter().chain(p.told.iter())
}

/// Whether post P of thread V is between the owner and agent A.
fn with_owner(v: &TicketView, p: &Post, a: &str) -> bool {
    if p.by == "owner" {
        return said(p).any(|n| n == a) || v.form.get("to").and_then(Value::as_str) == Some(a) && p.n == 1;
    }
    // an agent's post is to him when it names him, answers a post of his, or stands on his own thread naming nobody
    // else. Whom the norms told or informed decides nothing: they tell him of team talk on a thread he wrote on
    // (paper-research N-101.p1261, the owner on p1253 and p1260)
    p.by == a && (p.names.iter().any(|n| n == "owner") || answers_owner(v, p) || p.names.is_empty() && v.opener.as_deref() == Some("owner"))
}

/// Whether agent's post P answers a post of the owner's: it says which (`answers`), or it is the turn-end reply
/// (`auto`) to a turn given his post, the last post before it by anyone but its writer, and names nobody: a turn-end
/// reply that names only agents is to them, not to him (yaaihh/fresh's Go, N-1751 p2, finding 5).
fn answers_owner(v: &TicketView, p: &Post) -> bool {
    let by_owner = |n: u32| v.posts.iter().any(|x| x.n == n && x.by == "owner");
    if let Some(m) = p.answers {
        return by_owner(m);
    }
    p.auto && p.names.is_empty() && v.posts.iter().filter(|x| x.n < p.n && x.by != p.by && x.by != "coord" && x.moved.is_none()).max_by_key(|x| x.n).is_some_and(|x| x.by == "owner")
}

/// The owner's post on V that agent A's post Q answers: the one it names (`answers`: the reply tool, his Comment),
/// else, for a turn-end reply or a reply on a thread he opened, his last message to A there before it (paper-research
/// N-101.p1272, 4).
fn target(v: &TicketView, q: &Post, a: &str) -> Option<u32> {
    if q.by != a {
        return None;
    }
    if let Some(m) = q.answers {
        return v.posts.iter().find(|x| x.n == m && x.by == "owner").map(|x| x.n);
    }
    if !q.auto && v.opener.as_deref() != Some("owner") {
        return None;
    }
    v.posts.iter().filter(|m| m.n < q.n && m.by == "owner" && m.visible() && m.moved.is_none() && with_owner(v, m, a)).map(|m| m.n).max()
}

/// The agents his post N on V still waits on, in the order it asks them: those `asked` holds with no answer (I-1455
/// (B), the ledger's one rule; proto-lead N-1479 p138). None from a coordd before `ASKED_PROTOCOL`, which sends no
/// asks: there each caller reads what it read before, `unanswered_owner` and the names the post was written to.
pub(crate) fn owing<'a>(p: &crate::Proj, v: &'a TicketView, n: u32) -> Option<Vec<&'a str>> {
    (p.protocol >= coord_types::ASKED_PROTOCOL).then(|| v.asked.iter().filter(|q| q.post == n && q.answer.is_none()).map(|q| q.agent.as_str()).collect())
}

/// Whether agent A is at work on his post N of V now (the owner, N-1790 part 3: no "no answer yet" while it is): A is
/// in a turn, and V's post N was handed to it (a giving up to N or past it) since that turn began.
pub(crate) fn working_on(p: &crate::Proj, v: &TicketView, n: u32, a: &str) -> bool {
    let Some(ag) = p.live.agents.iter().find(|ag| ag.name == a && ag.state == "working") else { return false };
    let since = chrono::DateTime::parse_from_rfc3339(&ag.since).ok();
    v.given.iter().any(|g| g.to == a && g.hands(n) && since.is_none_or(|s| chrono::DateTime::parse_from_rfc3339(&g.at.to_string()).is_ok_and(|at| at >= s)))
}

/// The thread his next message to agent A continues, if any (paper-research N-101.p1367, 1): the thread of his
/// latest message to A, while A has not answered it there ([`owing`]; before `ASKED_PROTOCOL`, while
/// `unanswered_owner` holds it); or, while A is in a turn, the thread whose post began it (the latest post given to
/// A). A closed thread continues nothing. Otherwise his message goes through intake as before. His message to w2 and
/// w3 that w2 has answered continues nothing with w2, and its thread with w3 (consult-3 N-1479 p141).
pub(crate) fn follow_up(p: &crate::Proj, a: &str) -> Option<String> {
    use docket_machine::ViewStatus::{Done, Failed};
    let open = |v: &TicketView| v.kind == "thread" && !matches!(v.status, Done { .. } | Failed { .. });
    let last = p
        .live
        .tickets
        .values()
        .flat_map(|v| v.posts.iter().filter(|x| x.by == "owner" && x.visible() && x.moved.is_none() && with_owner(v, x, a)).map(move |x| (v, x)))
        .max_by(|x, y| x.1.at.to_string().cmp(&y.1.at.to_string()));
    if let Some((v, x)) = last
        && open(v)
        && owing(p, v, x.n).map_or_else(|| v.unanswered_owner.contains(&x.n), |o| o.contains(&a))
    {
        return Some(v.id.clone());
    }
    let working = p.live.agents.iter().any(|ag| ag.name == a && ag.state == "working");
    if !working {
        return None;
    }
    p.live
        .tickets
        .values()
        .filter(|v| open(v))
        .flat_map(|v| v.given.iter().filter(|g| g.to == a && g.delivery.contains(".p")).map(move |g| (v, g.at.to_string())))
        .max_by(|x, y| x.1.cmp(&y.1))
        .map(|(v, _)| v.id.clone())
}

/// Whether post P is between agent A and other agents (coord's notices to it among them).
fn with_agents(v: &TicketView, p: &Post, a: &str) -> bool {
    if p.by == "owner" || with_owner(v, p, a) {
        return false;
    }
    p.by == a || said(p).any(|n| n == a)
}

/// The posts of stream W for agent A, oldest first: (ticket number, post number, at).
pub(crate) fn posts(p: &crate::Proj, a: &str, w: Which) -> Vec<(u64, u32, String)> {
    let mut out: Vec<(u64, u32, String)> = Vec::new();
    for v in p.live.tickets.values() {
        for x in v.posts.iter().filter(|x| x.visible() && x.moved.is_none()) {
            let keep = match w {
                Which::You => with_owner(v, x, a),
                Which::Agents => with_agents(v, x, a),
                Which::Work => false,
            };
            if keep {
                out.push((v.n, x.n, x.at.to_string()));
            }
        }
    }
    out.sort_by(|x, y| x.2.cmp(&y.2).then(x.0.cmp(&y.0)).then(x.1.cmp(&y.1)));
    out
}

/// A time as the owner reads it: the local day, and the hour.
fn local(at: &str) -> Option<chrono::DateTime<chrono::Local>> {
    chrono::DateTime::parse_from_rfc3339(at).ok().map(|t| t.with_timezone(&chrono::Local))
}

fn day_of(at: &str) -> String {
    let Some(t) = local(at) else { return String::new() };
    let today = chrono::Local::now().date_naive();
    match (today - t.date_naive()).num_days() {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        _ => t.format("%-d %b").to_string(),
    }
}

/// Messages of one sender this close follow on under one header.
const FOLLOW_ON: i64 = 10 * 60;
/// An answer drawn under his message this long after it leaves a line where it came.
const LATE: i64 = 10 * 60;
/// His messages this close make one run, answered under it.
const RUN: i64 = 15 * 60;

fn secs(a: &str, b: &str) -> i64 {
    match (chrono::DateTime::parse_from_rfc3339(a), chrono::DateTime::parse_from_rfc3339(b)) {
        (Ok(a), Ok(b)) => (b - a).num_seconds().abs(),
        _ => i64::MAX,
    }
}

/// The rows of stream W: days, the moves between threads, the messages, and what each opened.
///
/// With you: his messages written one after another are a run under one header, and the agent's answers are drawn
/// under the run (paper-research N-101.p1287); an answer that came after he wrote again is drawn under the message it
/// answers, and a line stands where it came (p1272, 4: "his message, the reply attached to it, wherever the stream
/// has moved since").
fn lines(p: &crate::Proj, a: &str, w: Which) -> Vec<Line> {
    let ps = posts(p, a, w);
    let get = |i: usize| p.live.tickets.get(&ps[i].0).and_then(|v| v.posts.iter().find(|x| x.n == ps[i].1).map(|x| (v, x)));
    let tos: Vec<Option<(u64, u32)>> = (0..ps.len()).map(|i| get(i).and_then(|(v, x)| target(v, x, a)).map(|m| (ps[i].0, m))).collect();
    // the blocks: a run of his with the answers under it, an agent's message on its own, where a moved answer came
    enum B {
        Run(Vec<usize>, Vec<usize>),
        One(usize),
        Came(usize),
    }
    let mut bs: Vec<B> = Vec::new();
    let mut moved: HashSet<usize> = HashSet::new();
    for i in 0..ps.len() {
        let Some((_, x)) = get(i) else { continue };
        if w != Which::You {
            bs.push(B::One(i));
            continue;
        }
        if x.by == "owner" {
            match bs.last_mut() {
                Some(B::Run(m, ans)) if ans.is_empty() && m.last().is_some_and(|j| secs(&ps[*j].2, &ps[i].2) <= RUN) => m.push(i),
                _ => bs.push(B::Run(vec![i], Vec::new())),
            }
            continue;
        }
        let under = tos[i].and_then(|to| bs.iter().position(|b| matches!(b, B::Run(m, _) if m.iter().any(|j| (ps[*j].0, ps[*j].1) == to))));
        match under {
            Some(j) => {
                if let B::Run(_, ans) = &mut bs[j] {
                    ans.push(i);
                }
                if j + 1 < bs.len() {
                    moved.insert(i);
                    // where it came, when that is far from the message it answers: soon after, it is still in view
                    let far = tos[i].and_then(|to| ps.iter().find(|y| (y.0, y.1) == to)).is_none_or(|y| secs(&y.2, &ps[i].2) > LATE);
                    if far {
                        bs.push(B::Came(i));
                    }
                }
            }
            None => bs.push(B::One(i)),
        }
    }
    // in the stream's order: (post, the message it answers when quoted), or where a moved answer came
    // his message A still owes ([`owing`]): one w2 answered is not "no answer yet" in w2's stream while w3 owes it, though
    // w2's answer stands apart (a `coord post` on a lead's thread, or the reply tool's from another; proto-reader N-1479
    // p164); before `ASKED_PROTOCOL`, one `unanswered_owner` holds
    let unanswered = |j: usize| get(j).is_some_and(|(v, x)| owing(p, v, x.n).map_or_else(|| v.unanswered_owner.contains(&x.n), |o| o.contains(&a)));
    let mut seq: Vec<(usize, Option<(u64, u32)>, bool, bool)> = Vec::new();
    for b in &bs {
        match b {
            B::Run(m, ans) => {
                let open = ans.is_empty() && m.iter().any(|j| unanswered(*j));
                seq.extend(m.iter().enumerate().map(|(k, j)| (*j, None, false, open && k + 1 == m.len())));
                seq.extend(ans.iter().map(|j| (*j, tos[*j].filter(|_| m.len() > 1 || moved.contains(j)), false, false)));
            }
            B::One(j) => seq.push((*j, None, false, false)),
            B::Came(j) => seq.push((*j, None, true, false)),
        }
    }
    let mut out = Vec::new();
    let (mut day, mut thread) = (String::new(), 0u64);
    let mut prev: Option<(String, String)> = None;
    for (i, quote, came, open) in seq {
        let (t, n, at) = &ps[i];
        if came {
            if let Some(to) = tos[i] {
                out.push(Line::Moved(*t, *n, to));
            }
            prev = None;
            continue;
        }
        let Some((v, x)) = get(i) else { continue };
        let mut broke = false;
        if !moved.contains(&i) {
            let d = day_of(at);
            if d != day {
                out.push(Line::Day(d.clone()));
                day = d;
                thread = 0;
                broke = true;
            }
        }
        // his message that opens its thread names it in its header; an answer quoting his message is placed by it
        let opens = x.by == "owner" && v.opener.as_deref() == Some("owner") && v.posts.iter().filter(|y| y.visible()).map(|y| y.n).min() == Some(x.n);
        let mut id = false;
        if *t != thread {
            if opens {
                id = true;
            } else if !quote.is_some_and(|q| q.0 == *t) {
                out.push(Line::Thread(*t));
                broke = true;
            }
            thread = *t;
        }
        let head = broke || quote.is_some() || prev.as_ref().is_none_or(|(by, pat)| *by != x.by || secs(pat, at) > FOLLOW_ON);
        out.push(Line::Msg { t: *t, n: *n, head, to: quote, id, last: false, open });
        prev = Some((x.by.clone(), at.clone()));
        if moved.contains(&i) {
            continue;
        }
        // a change or question opened under the thread after this message and before the stream's next on it
        let next = ps.iter().filter(|y| y.0 == *t && y.2 > *at).map(|y| y.2.clone()).min();
        let mut opened: Vec<&TicketView> = v
            .children
            .iter()
            .filter_map(|id| p.live.tickets.values().find(|c| &c.id == id))
            .filter(|c| matches!(c.kind.as_str(), "change" | "question" | "ask" | "direct"))
            .filter(|c| {
                let o = c.opened_at.to_string();
                o >= *at && next.as_ref().is_none_or(|nx| o < *nx)
            })
            .collect();
        opened.sort_by_key(|c| c.opened_at.to_string());
        out.extend(opened.into_iter().map(|c| Line::Opened(c.n)));
    }
    // where its context was compacted: in With agents too, now that With you is his threads (withyou.rs)
    if w != Which::Work {
        place_compactions(p, a, &mut out);
    }
    // a group's last message carries the foot
    for i in 0..out.len() {
        if !matches!(out[i], Line::Msg { .. }) {
            continue;
        }
        let ends = out[i + 1..].iter().find_map(|l| match l {
            Line::Msg { head, .. } => Some(*head),
            Line::Opened(_) => None,
            _ => Some(true),
        });
        if let Line::Msg { last, .. } = &mut out[i] {
            *last = ends.unwrap_or(true);
        }
    }
    out
}

/// Where his post X on V stands with agent A: given (and when), or why it waits. coordd's given lines say it, not
/// the time (paper-research N-101.p1259, 2).
pub(crate) fn delivery(state: Option<&str>, v: &TicketView, x: &Post, a: &str) -> String {
    // the rows that handed it (`ViewGiven::hands`, (c2))
    if let Some(g) = v.given.iter().filter(|g| g.to == a && g.hands(x.n)).min_by_key(|g| g.at.to_string()) {
        return format!("given {}", crate::when::at(&g.at.to_string()));
    }
    if x.dropped {
        return "dropped".into();
    }
    if x.held.is_some() {
        return "held: not given yet".into();
    }
    if x.halted {
        return "halted for you: not given yet".into();
    }
    match state {
        Some("working") => format!("waiting: {a} is busy"),
        Some("stopped") => format!("waiting: {a} is stopped, given when it starts"),
        _ => "waiting".into(),
    }
}

/// The texts the stream shows that are not read yet (`crate::texts::poll` asks for them).
pub fn wanted(pg: &crate::threads::Page, p: &crate::Proj, a: &str, w: Which) -> Vec<docket_machine::Loc> {
    posts(p, a, w)
        .iter()
        .filter_map(|(t, n, _)| p.live.tickets.get(t)?.posts.iter().find(|x| x.n == *n))
        .filter_map(|x| crate::texts::loc_of(&serde_json::to_value(&x.text).unwrap_or(Value::Null)))
        .filter(|l| !pg.texts.contains_key(&crate::texts::key(l)))
        .collect()
}

/// The agents' posts to A not yet given to it, while the owner waits for its answer: what the lock holds (post 1099).
fn waiting(p: &crate::Proj, a: &str) -> usize {
    // his stream with A ends with his message: A has not answered him yet
    let last = posts(p, a, Which::You).pop();
    let owner_waits = last.is_some_and(|(t, n, _)| p.live.tickets.get(&t).and_then(|v| v.posts.iter().find(|x| x.n == n)).is_some_and(|x| x.by == "owner"));
    if !owner_waits {
        return 0;
    }
    p.live
        .tickets
        .values()
        // what A has taken of a thread is read once for it, not once a post: N-101's 1,880 posts each walked its
        // thousands of given lines again, 180 ms (view_cost.rs)
        .map(|v| {
            let upto = crate::seen::read_upto(v, a);
            v.posts.iter().filter(|x| x.by != "owner" && x.by != a && said(x).any(|n| n == a) && upto < x.n).count()
        })
        .sum()
}

/// The text's measure: a comfortable line, not a bubble (paper-research N-101.p1284).
pub(crate) const MEASURE: f32 = 760.0;


/// The first words of post N of V, for a quote.
pub(crate) fn first_words(s: &App, v: &TicketView, n: u32, max: usize) -> String {
    let Some(x) = v.posts.iter().find(|x| x.n == n) else { return String::new() };
    clip(&crate::texts::words(&s.tickets, &serde_json::to_value(&x.text).unwrap_or(Value::Null)), max)
}

/// The summary AGENT left when its context was compacted (its entry KEY in project PI), as its popup shows it: the
/// agent's own words, drawn as every message's are (post.rs); None when the session held no longer has it. SOURCE is
/// the popup's comment source: the passage he is commenting on in it is lit there (popup.rs `source`).
pub fn summary(s: &App, pi: usize, agent: &str, key: &str, source: &str, k: &Kit) -> Option<El> {
    let p = s.projects.get(pi)?;
    let c = p.convs.get(agent)?;
    let e = c.entry(key)?;
    let cwd = p.live.agents.iter().find(|x| x.name == agent).and_then(|x| x.cwd.clone());
    Some(crate::post::message_body(Some(p), &e.text, Some((s, source)), cwd, k))
}

fn line_el(s: &App, pi: usize, a: &str, l: &Line, w: Which, k: &Kit) -> El {
    let p = &s.projects[pi];
    match l {
        Line::Day(d) => row().w_full().px(24.0).pt(16.0).child(text(d.clone()).font(k.ty.small).color(k.c.text_faint)),
        Line::Thread(t) => {
            let Some(v) = p.live.tickets.get(t) else { return col() };
            let n = *t;
            // the thread by its words (its title is the owner's first sentence), else its label
            let label = if v.title.trim().is_empty() || v.title == v.kind { crate::refs::ticket(Some(p), v).label } else { v.title.clone() };
            // one muted line
            row()
                .w_full()
                .px(24.0)
                .pt(14.0)
                .gap(6.0)
                .center_y()
                .cursor(rune::Cursor::Pointer)
                .on_press(move |s: &mut App| crate::threads::show(s, n))
                .child(text(v.id.clone()).font(k.ty.small).color(k.c.text_faint))
                .child(text(clip(&label, 90)).font(k.ty.small).color(k.c.text_muted))
        }
        Line::Msg { t, n, head, to, id, last, open } => {
            let Some(v) = p.live.tickets.get(t) else { return col() };
            let Some(x) = v.posts.iter().find(|x| x.n == *n) else { return col() };
            crate::post::post(s, p, v, x, &crate::post::Form::Stream { agent: a, which: w, head: *head, to: *to, id: *id, last: *last, open: *open }, k)
        }
        Line::Moved(t, n, (tt, m)) => {
            let (Some(v), Some(tv)) = (p.live.tickets.get(t), p.live.tickets.get(tt)) else { return col() };
            let Some(x) = v.posts.iter().find(|x| x.n == *n) else { return col() };
            // a reply far from what it answers links back quietly, by what it answers, not by a sentence
            // (docs/specs/window-rules.md §4)
            let _ = x;
            let back = *tt;
            row().w_full().px(24.0).pt(8.0).child(
                k.interactive(text(format!("↩ “{}”", first_words(s, tv, *m, 60))).font(k.ty.small).color(k.c.text_faint)).on_press(move |s: &mut App| crate::threads::show(s, back)),
            )
        }
        Line::Compacted(key) => {
            let Some(c) = p.convs.get(a) else { return col() };
            let Some(e) = c.entry(key) else { return col() };
            let when = crate::when::at(&e.at);
            // the summary opens apart, over the stream (popup.rs)
            let title = crate::popup::title(&format!("{a}'s context compacted"), &when, "the summary it left");
            let what = crate::popup::What::Summary { pi, agent: a.to_string(), key: key.clone() };
            let b = col().w_full().max_w(MEASURE).gap(6.0).child(
                row()
                    .gap(10.0)
                    .center_y()
                    .child(text(format!("{a}'s context was compacted · {when}")).font(k.ty.small).color(k.c.text_faint))
                    .child(text("› the summary it left").font(k.ty.small).color(k.c.text_faint).cursor(rune::Cursor::Pointer).on_press(move |s: &mut App| crate::popup::open(s, title.clone(), what.clone()))),
            );
            row().w_full().px(24.0).pt(14.0).child(b)
        }
        Line::Opened(t) => {
            let Some(c) = p.live.tickets.get(t) else { return col() };
            let n = *t;
            let r = crate::refs::ticket(Some(p), c);
            let what = match c.kind.as_str() {
                "change" => "taken as a change",
                "question" | "ask" => "a question opened",
                _ => "opened",
            };
            row()
                .w_full()
                .px(24.0)
                .py(2.0)
                .gap(8.0)
                .center_y()
                .cursor(rune::Cursor::Pointer)
                .on_press(move |s: &mut App| crate::threads::show(s, n))
                .child(text("↳").font(k.ty.small).color(k.c.text_faint))
                .child(text(what).font(k.ty.small).color(k.c.text_muted))
                .child(text(format!("{} {}", c.id, clip(&r.label, 70))).font(k.ty.small).color(k.c.text_accent))
        }
    }
}

/// Row I's key: a thread's line comes again each time the stream returns to it, so it is keyed with the message
/// under it.
fn row_key(ls: &[Line], i: usize) -> String {
    match &ls[i] {
        Line::Thread(_) => format!("{:?} {:?}", ls[i], ls.get(i + 1)),
        l => format!("{l:?}"),
    }
}

pub(crate) fn clip(s: &str, n: usize) -> String {
    // the first line's words, without the quote and list marks it may start with
    let s = s.lines().next().unwrap_or("").trim().trim_start_matches(['>', '-', '*', ' ']);
    if s.chars().count() <= n { s.to_string() } else { format!("{}…", s.chars().take(n).collect::<String>()) }
}

/// The chat's stream, shown alone; the work behind a reply has its own way back.
pub fn switch(s: &App, pi: usize, name: &str, k: &Kit) -> El {
    let key = (pi, name.to_string());
    let shows_term = s.term.pages.contains(&key);
    let which = s.streams.which.get(&key).copied().unwrap_or_default();
    if which == Which::Work && !shows_term {
        let back = key.clone();
        return rune_kit::Button::new("‹ the chat").variant(rune_kit::ButtonVariant::Ghost).on_press(move |s: &mut App| {
            s.streams.which.insert(back.clone(), Which::You);
        }).build(k);
    }
    // the stream shown alone; the other is in the page's ⋯ (chat.rs `more_menu`)
    let word = if which == Which::Agents { "With agents".to_string() } else { crate::look::say(s, "with_you", &[]) };
    SegmentedControl::new(&[word.as_str()], Some(0)).label("Show").fit().build(k)
}

/// Stream W of agent NAME: the messages, and at the foot of his stream what waits for him.
/// What an agent does this moment, for its one "working now" line (paper-research N-516): the newest running call
/// in its own words (`Tool.about`, its description), else the tool and `what` (a Bash command drawn as code, a path
/// cut from its front so the file's name stays); a turn with no call running says only how long it has run. None
/// when no turn runs.
#[derive(Clone, Debug, PartialEq)]
pub enum Now {
    About(String),
    Call { tool: String, what: String, code: bool },
    Working,
    /// Its context is being compacted, since when: said in place of a call (the call the turn stood at is not what
    /// it does meanwhile; the owner, N-541 post 35).
    Compacting(String),
    /// Out of a turn, waiting on background work it started (coordd's state `waiting on shell bash_7`): the newest
    /// such call as `About` or `Call` says it, since when it runs, and how many more run (paper-research2's plan (c),
    /// N-1334: "waiting on its job · Write the new version onto the stick · 6 m").
    Waiting { call: Box<Now>, since: String, more: usize },
}

pub fn now(a: &coord_types::AgentView) -> Option<Now> {
    if let Some(since) = &a.compacting {
        return Some(Now::Compacting(since.clone()));
    }
    if let Some(t) = newest(a) {
        return Some(call_now(t));
    }
    if a.state.starts_with("waiting on ")
        && let Some(t) = a.background.iter().max_by(|x, y| x.at.cmp(&y.at))
    {
        return Some(Now::Waiting { call: Box::new(call_now(t)), since: t.at.clone(), more: a.background.len() - 1 });
    }
    (a.state == "working").then_some(Now::Working)
}

/// Call T in its own words (`Tool.about`, its description), else its tool and `what`: a Bash command's first line as
/// code, a path cut from its front so the file's name stays.
fn call_now(t: &coord_types::Tool) -> Now {
    if let Some(about) = t.about.as_deref().map(str::trim).filter(|x| !x.is_empty()) {
        return Now::About(about.to_string());
    }
    let code = t.tool == "Bash";
    // the first line of a command; the line's end cuts the rest
    let first = t.what.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
    let what = if t.what.trim().lines().count() > 1 {
        format!("{first} …")
    } else if !code && first.starts_with('/') && first.matches('/').count() > 3 {
        format!("…/{}", first.rsplitn(4, '/').take(3).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("/"))
    } else {
        first.to_string()
    };
    Now::Call { tool: t.tool.clone(), what, code }
}

/// A's state in words where the window says it as text (the All agents table, the facts behind the circle):
/// coordd's, but for one waiting on its background work, said as the working line says it ("waiting on its job · Run
/// the tests · 6 m" in place of "waiting on shell bash_7", plan (c), N-1334), and for one held on a rate limit, as
/// its line says it (`agents::held`, proto-lead N-1397 p160).
pub fn state_words(p: &crate::Proj, a: &coord_types::AgentView) -> String {
    // held on a rate limit: that, as its line says (`agents::held`)
    if let Some(words) = crate::agents::held(p, a) {
        return words;
    }
    let Some(Now::Waiting { call, since, more }) = now(a) else { return a.state.clone() };
    let call = match *call {
        Now::About(t) => Some(t),
        Now::Call { tool, what, .. } => Some(format!("{tool} {what}")),
        _ => None,
    };
    let words = [Some("waiting on its job".to_string()), call, ran(&since, chrono::Utc::now())].into_iter().flatten().collect::<Vec<_>>().join(" · ");
    if more > 0 { format!("{words} +{more} more") } else { words }
}

fn newest(a: &coord_types::AgentView) -> Option<&coord_types::Tool> {
    a.tools.iter().max_by(|x, y| x.at.cmp(&y.at))
}

/// How long since AT (RFC 3339) at NOW, as a time taken is written ([`crate::when::took`]: "3 m", "2 h 05 m"), in tens
/// of seconds under a minute ("40 s"); none under ten seconds. The window wakes every ten seconds while a turn runs
/// (lib.rs), so the figure moves no oftener.
pub fn ran(at: &str, now: chrono::DateTime<chrono::Utc>) -> Option<String> {
    span((now - chrono::DateTime::parse_from_rfc3339(at).ok()?.with_timezone(&chrono::Utc)).num_seconds())
}

/// SECS in the words of [`ran`]: none under ten seconds, tens of seconds under a minute, then as a time taken.
pub fn span(secs: i64) -> Option<String> {
    match secs {
        ..10 => None,
        ..60 => Some(crate::when::took(secs / 10 * 10)),
        _ => Some(crate::when::took(secs)),
    }
}

/// The "working now" line: one component, on the agent's tile, in its page's head, under his last message in With
/// you and under a post (docket-edge4): "in a turn 40 s · what it does", the seconds from the call's start; out of a
/// turn, the background work it waits on ("waiting on its job · Run the tests · 6 m"). Under it, in a turn or out
/// of one, the line of its job on the build runner with the bar when the step gives a total (`jobs::line`, N-1334).
/// None when none of these is there. Muted, never the accent, and it lands at once: nothing slides in (N-466).
pub fn working_now(a: &coord_types::AgentView, k: &Kit) -> Option<El> {
    let job = crate::jobs::line(a, k);
    let Some(doing) = now(a) else { return job };
    let line = doing_line(a, doing, k);
    Some(match job {
        Some(j) => col().gap(4.0).min_w(0.0).shrink().child(line).child(j),
        None => line,
    })
}

/// The first line of [`working_now`]: what A does, as [`now`] read it.
fn doing_line(a: &coord_types::AgentView, doing: Now, k: &Kit) -> El {
    let dot = || text("·").font(k.ty.small).color(k.c.text_faint);
    let faint = |t: String| text(t).font(k.ty.small).color(k.c.text_faint);
    let r = || row().gap(6.0).center_y().min_w(0.0).shrink();
    // a call after R: in its own words, else its tool and what it runs, a command as code
    let with_call = |r: El, n: Now| match n {
        Now::About(t) => r.child(dot()).child(text(t).font(k.ty.small).color(k.c.text_muted).truncate().shrink()),
        Now::Call { tool, what, code } => r
            .child(dot())
            .child(faint(tool))
            .child(text(what).font(if code { k.ty.mono } else { k.ty.small }).color(k.c.text_muted).truncate().shrink()),
        _ => r,
    };
    match doing {
        // a compaction: how long it has run, after the turn it runs in, if it runs in one (`/compact` typed while idle
        // runs in none)
        Now::Compacting(since) => {
            let words = ran(&since, chrono::Utc::now()).map_or("compacting its context".to_string(), |d| format!("compacting its context {d}"));
            let r = if a.state == "working" { r().child(faint("in a turn".into())).child(dot()) } else { r() };
            r.child(text(words).font(k.ty.small).color(k.c.text_muted).truncate().shrink())
        }
        // the call's own time after its words, which are what gets cut
        Now::Waiting { call, since, more } => with_call(r().child(faint("waiting on its job".into())), *call)
            .children(ran(&since, chrono::Utc::now()).into_iter().flat_map(|d| [dot(), faint(d)]))
            .children((more > 0).then(|| faint(format!("+{more} more")))),
        doing => {
            let from = newest(a).map_or(a.since.as_str(), |t| t.at.as_str());
            let turn = ran(from, chrono::Utc::now()).map_or("in a turn".to_string(), |d| format!("in a turn {d}"));
            with_call(r().child(faint(turn)), doing)
        }
    }
}

/// The stream's last row while A's turn runs, it waits on its background work or has a job to show: "A · in a turn
/// 40 s · what it does  ›  your message is given when it ends", the part after › only while a message of his waits
/// for the turn's end (WAITS); › opens the session as it runs (the Work view, the rows the chat draws).
pub(crate) fn working_row(pi: usize, a: &coord_types::AgentView, waits: bool, k: &Kit) -> El {
    let Some(line) = working_now(a, k) else { return row() };
    let key = (pi, a.name.clone());
    let mut way = row().gap(6.0).center_y().child(text("›").font(k.ty.small_medium).color(k.c.text_muted));
    if waits {
        way = way.child(text("your message is given when it ends").font(k.ty.small).color(k.c.text_faint));
    }
    let way = k.interactive(way.px(4.0).radius(k.radius.sm)).help("See it work").on_press(move |s: &mut App| {
        s.streams.which.insert(key.clone(), Which::Work);
    });
    row()
        .w_full()
        .px(24.0)
        .py(6.0)
        .gap(6.0)
        .center_y()
        .child(text(a.name.clone()).font(k.ty.small_medium).color(k.c.text_muted))
        .child(text("·").font(k.ty.small).color(k.c.text_faint))
        .child(line)
        .child(way)
}

/// Whether one of his messages in LS waits for A's running turn to end: it is then said once, on the working row.
fn waits_for_turn(p: &crate::Proj, a: &str, ls: &[Line]) -> bool {
    ls.iter().rev().filter_map(|l| match l { Line::Msg { t, n, .. } => Some((*t, *n)), _ => None }).any(|(t, n)| {
        let Some(v) = p.live.tickets.get(&t) else { return false };
        v.posts.iter().find(|x| x.n == n).is_some_and(|x| x.by == "owner" && delivery(Some("working"), v, x, a).starts_with("waiting: "))
    })
}

pub fn view(s: &App, pi: usize, name: &str, w: Which, k: &Kit) -> El {
    let Some(p) = s.projects.get(pi) else { return col() };
    let (ls, held) = kept(&s.streams, p, name, w);
    let n = ls.len();
    if n == 0 {
        let none = match w {
            Which::You => crate::look::say(s, "nothing_with", &[("agent", name)]),
            _ => format!("{name} has had nothing with other agents."),
        };
        return col().w_full().grow(1.0).padding(24.0).child(text(none).font(k.ty.body).color(k.c.text_muted));
    }
    let (who, kit) = (name.to_string(), k.clone());
    // what it does now, the list's last row, right under his last message (N-516); a compaction, the background work
    // it waits on and its job on the build runner are said there too
    let doing = |s: &App| s.projects.get(pi)?.live.agents.iter().find(|ag| ag.name == who).filter(|ag| now(ag).is_some() || crate::jobs::shown(ag).is_some()).cloned();
    let more = (w == Which::You && doing(s).is_some()) as usize;
    // its rows, built as they come into view, drawn in the split's pane it is in (ask.rs `ours`)
    let pane = crate::ask::pane_drawn(s);
    let rows = col()
        .key(format!("stream-rows-{pi}-{name}-{w:?}"))
        .w_full()
        .growth(GrowthSpec::default())
        .virtual_rows(
            n + more,
            80.0,
            {
                let ls = ls.clone();
                move |_: &App, i| std::borrow::Cow::Owned(if i < ls.len() { row_key(&ls, i) } else { "working-now".into() })
            },
            move |s: &App, i| crate::ask::in_pane(s, pane, || match ls.get(i) {
                Some(l) => line_el(s, pi, &who, l, w, &kit),
                None => {
                    let Some(p) = s.projects.get(pi) else { return row() };
                    let Some(ag) = p.live.agents.iter().find(|ag| ag.name == who) else { return row() };
                    working_row(pi, ag, waits_for_turn(p, &who, &ls), &kit)
                }
            }),
        );
    let mut c = col().w_full().grow(1.0).min_h(0.0).child(col().key(format!("stream-{pi}-{name}-{w:?}")).w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: true }).selection_group(true).selection_menu_item_when("comment", "Comment", crate::ask::one_source, {
        // a passage inside one post offers Comment; one across posts copies alone (`ask::one_source`)
        let target = name.to_string();
        move |s: &mut App, sel, tx| crate::ask::comment_on(s, pi, &target, sel, tx)
    }).on_highlight_press(|s: &mut App, key, _| crate::ask::highlight_pressed(s, key)).child(rows.pb(12.0)));
    if w == Which::You {
        if held > 0 {
            c = c.child(row().w_full().px(24.0).py(6.0).justify_center().child(
                text(format!("{held} agent message{} held until {name} answers", if held == 1 { "" } else { "s" })).font(k.ty.small).color(k.c.text_faint),
            ));
        }
    }
    c
}

/// The parts of building A's stream with him, for view_cost.rs: the posts picked, the whole lines, the targets,
/// 1,000 days and 1,000 spans of time read.
#[cfg(test)]
pub(crate) fn cost(p: &crate::Proj, a: &str) -> [std::time::Duration; 5] {
    let t = std::time::Instant::now;
    let (s0, ps) = (t(), posts(p, a, Which::You));
    let d0 = s0.elapsed();
    let s1 = t();
    std::hint::black_box(lines(p, a, Which::You));
    let d1 = s1.elapsed();
    let s2 = t();
    for (tn, n, _) in &ps {
        let v = &p.live.tickets[tn];
        let x = v.posts.iter().find(|x| x.n == *n).unwrap();
        std::hint::black_box(target(v, x, a));
    }
    let d2 = s2.elapsed();
    let s3 = t();
    for (_, _, at) in ps.iter().cycle().take(1000) {
        std::hint::black_box(day_of(at));
    }
    let d3 = s3.elapsed();
    let s4 = t();
    for w in ps.windows(2).cycle().take(1000) {
        std::hint::black_box(secs(&w[0].2, &w[1].2));
    }
    [d0, d1, d2, d3, s4.elapsed()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The stream is kept between frames and built again when a ticket's event comes (the owner, N-412): his new
    /// message and the agent's answer show, and the agents' posts held back are counted again.
    #[test]
    fn a_kept_stream_follows_the_tickets_events() {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        let post = |n: u32, by: &str, names: &[&str], at: &str| json!({"n": n, "at": at, "by": by, "text": format!("post {n}"), "names": names, "told": []});
        let v = thread("owner", json!([post(1, "owner", &["pr"], "2026-11-16T10:00:00Z")]));
        p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(v.clone()) });
        let st = Streams::default();
        let (one, held) = kept(&st, &p, "pr", Which::You);
        assert_eq!(held, 0);
        let (again, _) = kept(&st, &p, "pr", Which::You);
        assert!(Rc::ptr_eq(&one, &again), "nothing moved: the same lines, not built again");
        // pr answers and core writes to pr; then he writes again, so core's post waits
        let mut w = v.clone();
        w.posts = serde_json::from_value(json!([post(2, "pr", &["owner"], "2026-11-16T10:01:00Z"), post(3, "core", &["pr"], "2026-11-16T10:02:00Z"),
            post(4, "owner", &["pr"], "2026-11-16T10:03:00Z")])).unwrap();
        p.live.apply(coord_types::Event { seq: 2, body: coord_types::Body::TicketChange { view: w, posts: 4 } });
        let (two, held) = kept(&st, &p, "pr", Which::You);
        assert!(!Rc::ptr_eq(&one, &two));
        assert_eq!(*two, lines(&p, "pr", Which::You), "the same lines as built afresh");
        let msgs = |ls: &[Line]| ls.iter().filter(|l| matches!(l, Line::Msg { .. })).count();
        assert_eq!((msgs(&one), msgs(&two)), (1, 3));
        assert_eq!(held, 1, "core's post to pr, not yet given to it, while he waits");
        // a ticket put in by hand (a scene) is seen too
        let mut x = v.clone();
        x.n = 6;
        x.id = "N-6".into();
        x.posts = serde_json::from_value(json!([post(1, "owner", &["pr"], "2026-11-16T11:00:00Z")])).unwrap();
        p.live.tickets.insert(6, x);
        assert_eq!(msgs(&kept(&st, &p, "pr", Which::You).0), 4);
    }

    fn agent(state: &str, tools: Value) -> coord_types::AgentView {
        serde_json::from_value(json!({"id": "", "name": "a", "parent": "", "kind": "", "model": "", "state": state, "since": "",
            "last_tool": null, "tools": tools, "items": [], "transcript": null, "mem_mb": null, "cpu_s": null})).unwrap()
    }

    /// The "working now" words: the newest call in its own words, else the tool and its first line, Bash as code; a
    /// turn with no call is "working"; an idle agent has no line.
    #[test]
    fn working_now_says_the_newest_call_in_its_own_words_or_its_command() {
        let call = |id: &str, tool: &str, what: &str, at: &str, about: Option<&str>| json!({"id": id, "tool": tool, "what": what, "at": at, "about": about});
        let a = agent("working", json!([call("1", "Read", "/x/a.rs", "2026-11-18T10:00:00Z", None), call("2", "Bash", "cargo test\n  -p coordd", "2026-11-18T10:00:05Z", None)]));
        assert_eq!(now(&a), Some(Now::Call { tool: "Bash".into(), what: "cargo test …".into(), code: true }));
        let a = agent("working", json!([call("2", "Bash", "cargo test", "2026-11-18T10:00:05Z", Some("Run the tests"))]));
        assert_eq!(now(&a), Some(Now::About("Run the tests".into())));
        let a = agent("working", json!([call("1", "Read", "/x/a.rs", "2026-11-18T10:00:00Z", Some("  "))]));
        assert_eq!(now(&a), Some(Now::Call { tool: "Read".into(), what: "/x/a.rs".into(), code: false }));
        assert_eq!(now(&agent("working", json!([]))), Some(Now::Working));
        let a = agent("working", json!([call("1", "Edit", "/home/t/p/soft-swarm-docket-core4/yaaihh/crates/docket-machine/src/view.rs", "2026-11-18T10:00:00Z", None)]));
        assert_eq!(now(&a), Some(Now::Call { tool: "Edit".into(), what: "…/docket-machine/src/view.rs".into(), code: false }), "a path keeps its file's name");
        let at = |s: &str| chrono::DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&chrono::Utc);
        assert_eq!(ran("2026-11-18T10:00:00Z", at("2026-11-18T10:00:47Z")).as_deref(), Some("40 s"));
        assert_eq!(ran("2026-11-18T10:00:00Z", at("2026-11-18T10:03:05Z")).as_deref(), Some("3 m"));
        // as every time taken is written (`when::took`, I-1469 p42): two hours is "2 h", not "120 min"
        assert_eq!(ran("2026-11-18T10:00:00Z", at("2026-11-18T12:00:00Z")).as_deref(), Some("2 h"));
        assert_eq!(ran("2026-11-18T10:00:00Z", at("2026-11-18T12:05:59Z")).as_deref(), Some("2 h 05 m"));
        assert_eq!(ran("2026-11-18T10:00:00Z", at("2026-11-18T10:00:04Z")), None);
        assert_eq!(now(&agent("idle", json!([]))), None);
    }

    /// Out of a turn, waiting on its background work: the newest call in its words and since when, the rest counted,
    /// in place of coordd's "waiting on shell bash_7" (plan (c), N-1334); in a turn the running call is said instead.
    #[test]
    fn waiting_on_background_work_is_said_in_the_words_of_its_call() {
        let call = |id: &str, what: &str, at: &str, about: Option<&str>| json!({"id": id, "tool": "Bash", "what": what, "at": at, "about": about});
        let p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        let mut a = agent("waiting on shell bash_7, shell bash_8", json!([]));
        // since 2 h 05 m 30 s and 3 h ago: the line's words are read at now
        let ago = |secs: i64| (chrono::Utc::now() - chrono::Duration::seconds(secs)).to_rfc3339();
        let since = ago(2 * 3600 + 5 * 60 + 30);
        a.background = serde_json::from_value(json!([call("bash_7", "dd if=x of=/dev/sdb", &since, Some("Write the new version onto the stick")),
            call("bash_8", "cargo test", &ago(3 * 3600), None)])).unwrap();
        let about = Box::new(Now::About("Write the new version onto the stick".into()));
        assert_eq!(now(&a), Some(Now::Waiting { call: about, since, more: 1 }));
        assert_eq!(state_words(&p, &a), "waiting on its job · Write the new version onto the stick · 2 h 05 m +1 more", "as every time taken is written (I-1469 p42)");
        a.background.pop();
        a.background[0].about = None;
        assert!(state_words(&p, &a).starts_with("waiting on its job · Bash dd if=x of=/dev/sdb · "));
        a.state = "working".into();
        assert_eq!(now(&a), Some(Now::Working), "in a turn the turn is said");
        assert_eq!(state_words(&p, &a), "working");
        a.state = "idle".into();
        assert_eq!(now(&a), None);
    }

    fn thread(opener: &str, posts: Value) -> TicketView {
        serde_json::from_value(json!({"id": "N-5", "n": 5, "kind": "thread", "opener": opener, "ctx": {}, "form": {"to": "pr"}, "title": "t",
            "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": "2026-11-16T10:00:00Z", "given": [], "posts": posts}))
        .unwrap()
    }

    #[test]
    fn the_owners_stream_holds_him_and_the_agent_and_nothing_else() {
        let v = thread("owner", json!([
            {"n": 1, "at": "2026-11-16T10:00:00Z", "by": "owner", "text": "do x", "names": ["pr"], "told": ["pr"]},
            {"n": 2, "at": "2026-11-16T10:01:00Z", "by": "pr", "text": "done", "names": ["owner"]},
            {"n": 3, "at": "2026-11-16T10:02:00Z", "by": "pr", "text": "@core build it", "names": ["core"]},
            {"n": 4, "at": "2026-11-16T10:03:00Z", "by": "core", "text": "built", "names": ["pr"]}
        ]));
        let p = |n: u32| v.posts.iter().find(|x| x.n == n).unwrap();
        assert!(with_owner(&v, p(1), "pr"));
        assert!(with_owner(&v, p(2), "pr"));
        assert!(!with_owner(&v, p(3), "pr"), "its message to another agent is the agents' stream's");
        assert!(with_agents(&v, p(3), "pr") && with_agents(&v, p(4), "pr"));
        assert!(!with_agents(&v, p(1), "pr") && !with_agents(&v, p(2), "pr"));
        assert!(!with_owner(&v, p(1), "core"), "his message to pr is not in core's stream with him");
    }

    #[test]
    fn a_post_naming_other_agents_is_not_his_though_the_norms_tell_him() {
        let v = thread("owner", json!([
            {"n": 1, "at": "2026-11-16T10:00:00Z", "by": "pr", "text": "@core merge it", "names": ["core", "core5"], "told": ["owner"]},
            {"n": 2, "at": "2026-11-16T10:01:00Z", "by": "pr", "text": "done", "names": [], "told": ["owner"]},
            {"n": 3, "at": "2026-11-16T10:02:00Z", "by": "pr", "text": "@owner @core both", "names": ["owner", "core"], "told": []}
        ]));
        let p = |n: u32| v.posts.iter().find(|x| x.n == n).unwrap();
        assert!(!with_owner(&v, p(1), "pr"), "addressed to two agents");
        assert!(with_agents(&v, p(1), "pr"));
        assert!(with_owner(&v, p(2), "pr"), "names nobody and the owner is told");
        assert!(with_owner(&v, p(3), "pr"), "names him among others");
    }

    /// Finding 5 of his walk item 15 (yaaihh/fresh's Go, N-1751 p2): an agent's reply at its turn's end that names only
    /// agents is not to him, though his message gave the turn; naming nobody, or him among them, it is.
    #[test]
    fn a_turn_end_reply_naming_only_agents_is_not_to_him() {
        let v = thread("owner", json!([
            {"n": 1, "at": "2026-11-16T10:00:00Z", "by": "owner", "text": "do x", "names": ["pr"], "told": ["pr"]},
            {"n": 2, "at": "2026-11-16T10:01:00Z", "by": "pr", "text": "@core build x", "auto": true, "names": ["core"], "told": ["owner"]},
            {"n": 3, "at": "2026-11-16T10:02:00Z", "by": "owner", "text": "and y", "names": ["pr"], "told": ["pr"]},
            {"n": 4, "at": "2026-11-16T10:03:00Z", "by": "pr", "text": "y is done", "auto": true, "told": ["owner"]},
            {"n": 5, "at": "2026-11-16T10:04:00Z", "by": "owner", "text": "and z", "names": ["pr"], "told": ["pr"]},
            {"n": 6, "at": "2026-11-16T10:05:00Z", "by": "pr", "text": "@owner @core z is core's", "auto": true, "names": ["owner", "core"]}
        ]));
        let p = |n: u32| v.posts.iter().find(|x| x.n == n).unwrap();
        assert!(!with_owner(&v, p(2), "pr"), "its turn-end reply names only core");
        assert!(with_agents(&v, p(2), "pr"), "it is with the agents");
        assert!(with_owner(&v, p(4), "pr") && !with_agents(&v, p(4), "pr"), "naming nobody, it answers him");
        assert!(with_owner(&v, p(6), "pr"), "naming him among agents");
    }

    /// N-101 as the record has it: a thread paper-research opened, the owner's comment on it, the replies to him, and
    /// team posts the norms told him of (p1253, p1256, p1260).
    #[test]
    fn on_a_thread_an_agent_opened_he_has_what_names_or_answers_him() {
        let v = thread("pr", json!([
            {"n": 1250, "at": "2026-11-16T21:35:00Z", "by": "core2", "text": "@paper-research dev.sh refuses", "names": ["pr"], "told": ["owner"]},
            {"n": 1253, "at": "2026-11-16T21:39:00Z", "by": "pr", "text": "@docket-core @docket-core5: two lines", "names": ["core", "core5"], "told": ["owner"]},
            {"n": 1254, "at": "2026-11-16T21:40:00Z", "by": "owner", "text": "thsis is ending up in my with you", "names": [], "told": ["pr"]},
            {"n": 1255, "at": "2026-11-16T21:41:00Z", "by": "pr", "text": "It was the policy.", "auto": true, "told": ["owner"]},
            {"n": 1256, "at": "2026-11-16T21:41:30Z", "by": "pr", "text": "@docket-core4: the assign prompt", "names": ["core4"], "told": ["owner"]},
            {"n": 1257, "at": "2026-11-16T21:42:00Z", "by": "pr", "text": "Heads recorded.", "told": ["owner"]},
            {"n": 1258, "at": "2026-11-16T21:43:00Z", "by": "pr", "text": "on your comment: fixed", "answers": 1254},
            {"n": 1259, "at": "2026-11-16T21:44:00Z", "by": "pr", "text": "@owner: on the policy", "names": ["owner"]}
        ]));
        let p = |n: u32| v.posts.iter().find(|x| x.n == n).unwrap();
        assert!(!with_owner(&v, p(1253), "pr") && !with_owner(&v, p(1256), "pr"), "to other agents, though he is told");
        assert!(!with_owner(&v, p(1257), "pr"), "names nobody on a thread not his: team talk he is told of");
        assert!(with_owner(&v, p(1254), "pr"), "his comment");
        assert!(with_owner(&v, p(1255), "pr"), "the turn-end reply to his comment");
        assert!(with_owner(&v, p(1258), "pr"), "answers his comment");
        assert!(with_owner(&v, p(1259), "pr"), "names him");
        assert!(with_agents(&v, p(1253), "pr") && with_agents(&v, p(1257), "pr"));
        assert!(!with_owner(&v, p(1250), "core2"), "core2 to pr");
    }

    /// His message says whether it was given, from the given lines (N-347.p1 of 21:36:33, given at 21:41:14).
    #[test]
    fn his_message_says_waiting_until_given() {
        let mut v = thread("owner", json!([{"n": 1, "at": "2026-11-16T20:36:33Z", "by": "owner", "text": "or was it for me ?", "names": ["pr"]}]));
        let x = v.posts[0].clone();
        assert_eq!(delivery(Some("working"), &v, &x, "pr"), "waiting: pr is busy");
        assert_eq!(delivery(Some("idle"), &v, &x, "pr"), "waiting");
        v.given = serde_json::from_value(json!([{"to": "pr", "delivery": "N-5.p1", "upto": 1, "at": "2026-11-16T20:41:14Z"}])).unwrap();
        assert!(delivery(Some("working"), &v, &x, "pr").starts_with("given "), "given, whatever the agent does now");
        assert_eq!(delivery(None, &v, &x, "other"), "waiting", "given to pr is not given to another");
    }

    /// The row keys of a stream that returns to a thread are all different (prod's log: "duplicate virtual row key
    /// Thread(N)", 50 times on 49b272e).
    #[test]
    fn a_stream_back_on_a_thread_keys_its_line_apart() {
        let m = |t: u64, n: u32| Line::Msg { t, n, head: true, to: None, id: false, last: true, open: false };
        let ls = [Line::Thread(5), m(5, 1), Line::Thread(6), m(6, 1), Line::Thread(5), m(5, 2)];
        let keys: HashSet<String> = (0..ls.len()).map(|i| row_key(&ls, i)).collect();
        assert_eq!(keys.len(), ls.len());
    }

    fn mine(n: u64, posts: Value) -> TicketView {
        serde_json::from_value(json!({"id": format!("N-{n}"), "n": n, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "pr"}, "title": "t",
            "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": "2026-11-16T20:44:00Z", "given": [], "posts": posts}))
        .unwrap()
    }

    /// Tonight's record (20:44 to 20:52 UTC): two messages of his one after another on two threads, both answered after
    /// them; then a message on a third, and an answer to the first run only after it (paper-research N-101.p1287 and
    /// p1272, 4).
    #[test]
    fn his_run_is_answered_under_it_and_a_late_answer_under_its_message() {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        let o = |at: &str, text: &str| json!({"n": 1, "at": format!("2026-11-16T20:{at}Z"), "by": "owner", "text": text, "names": ["pr"], "told": ["pr"]});
        let r = |n: u32, at: &str, auto: bool| json!({"n": n, "at": format!("2026-11-16T20:{at}Z"), "by": "pr", "text": "yes", "names": ["owner"], "auto": auto});
        for v in [
            mine(349, json!([o("44:49", "recap me"), r(2, "58:00", false)])),
            mine(350, json!([o("45:00", "the chat is not right"), r(2, "46:38", false)])),
            mine(352, json!([o("47:48", "we can do better")])),
            // another agent's post on his thread stays out of it
            mine(353, json!([o("48:30", "you need a tool"), {"n": 2, "at": "2026-11-16T20:49:00Z", "by": "core", "text": "x", "names": ["pr"]}])),
        ] {
            p.live.tickets.insert(v.n, v);
        }
        let ls = lines(&p, "pr", Which::You);
        let msgs: Vec<(u64, u32, bool, Option<(u64, u32)>)> = ls.iter().filter_map(|l| match l { Line::Msg { t, n, head, to, .. } => Some((*t, *n, *head, *to)), _ => None }).collect();
        assert_eq!(
            msgs,
            vec![
                (349, 1, true, None),
                (350, 1, false, None),
                // the answer to the second of his two: quoted, since the run holds two
                (350, 2, true, Some((350, 1))),
                // the answer that came at 20:58 stands under the first run, quoted
                (349, 2, true, Some((349, 1))),
                (352, 1, true, None),
                (353, 1, false, None),
            ]
        );
        assert!(ls.contains(&Line::Moved(349, 2, (349, 1))), "and a line where it came: {ls:?}");
        assert!(!ls.iter().any(|l| *l == Line::Thread(349) || *l == Line::Thread(350)), "his message opening its thread names it in its header");
    }

    /// A compaction stands where it came: between the message before it and the day and thread lines that head the
    /// message after it.
    #[test]
    fn a_compaction_stands_where_it_came_in_his_stream() {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        let o = |n: u32, at: &str| json!({"n": n, "at": at, "by": "owner", "text": "x", "names": ["pr"], "told": ["pr"]});
        for v in [mine(349, json!([o(1, "2026-11-16T20:44:00Z")])), mine(350, json!([o(1, "2026-11-17T09:00:00Z")]))] {
            p.live.tickets.insert(v.n, v);
        }
        let e = |at: &str, text: &str| serde_json::from_value::<coord_types::Entry>(json!({"offset": 1, "at": at, "kind": "user", "text": text})).unwrap();
        let c = p.convs.entry("pr".into()).or_default();
        c.entries = vec![e("2026-11-16T23:10:00Z", &format!("{SUMMARY} that ran out of context. The summary below ...")), e("2026-11-16T23:11:00Z", "go on")];
        c.keys = vec!["k1".into(), "k2".into()];
        c.loaded = true;
        let ls = lines(&p, "pr", Which::You);
        let at = ls.iter().position(|l| *l == Line::Compacted("k1".into())).expect("the compaction is in his stream");
        let first = ls.iter().position(|l| matches!(l, Line::Msg { t: 349, .. })).unwrap();
        let day = ls.iter().rposition(|l| matches!(l, Line::Day(_))).unwrap();
        assert!(first < at && at < day, "after the 20:44 message, above the next day's line: {ls:?}");
        assert_eq!(ls.iter().filter(|l| matches!(l, Line::Compacted(_))).count(), 1, "the prompt after it is no compaction");
    }

    /// Tonight's 21:47 to 21:52 (paper-research N-101.p1367): his message after an unanswered one on N-349 continues
    /// N-349; once answered, and with the agent idle, it goes through intake.
    #[test]
    fn a_follow_up_continues_the_thread_of_his_unanswered_message() {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        let mut v = mine(349, json!([{"n": 1, "at": "2026-11-16T20:44:49Z", "by": "owner", "text": "recap me", "names": ["pr"], "told": ["pr"]}]));
        v.unanswered_owner = vec![1];
        p.live.tickets.insert(349, v.clone());
        assert_eq!(follow_up(&p, "pr").as_deref(), Some("N-349"));
        assert_eq!(follow_up(&p, "core"), None, "his message to pr continues nothing with core");
        v.unanswered_owner.clear();
        p.live.tickets.insert(349, v);
        assert_eq!(follow_up(&p, "pr"), None);
    }

    /// His N-200.p1 asks w2 and w3, and w2 answers it at p2 (consult-3 N-1479 p141, I-1455 (B)): w3 still owes it, so
    /// `unanswered_owner` still holds p1. His next message to w2 goes through intake, and to w3 continues N-200. From a
    /// coordd before `ASKED_PROTOCOL`, which sends no asks, both continue N-200 as before.
    #[test]
    fn a_follow_up_continues_only_with_the_agent_that_still_owes_his_message() {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        p.protocol = coord_types::ASKED_PROTOCOL;
        let mut v = mine(200, json!([
            {"n": 1, "at": "2026-11-21T16:00:00Z", "by": "owner", "text": "@w2 @w3 can you both check the stills?", "names": ["w2", "w3"], "told": ["w2", "w3"]},
            {"n": 2, "at": "2026-11-21T16:05:00Z", "by": "w2", "text": "Mine are right: 4 of 4.", "names": ["owner"], "answering": ["owner"]},
        ]));
        v.form = json!({});
        v.unanswered_owner = vec![1];
        v.asked = serde_json::from_value(json!([{"post": 1, "agent": "w2", "answer": "N-200.p2"}, {"post": 1, "agent": "w3"}])).unwrap();
        p.live.tickets.insert(200, v.clone());
        assert_eq!((follow_up(&p, "w2"), follow_up(&p, "w3").as_deref()), (None, Some("N-200")), "w2 answered it, w3 owes it");
        p.protocol = coord_types::ASKED_PROTOCOL - 1;
        v.asked.clear();
        p.live.tickets.insert(200, v);
        assert_eq!((follow_up(&p, "w2").as_deref(), follow_up(&p, "w3").as_deref()), (Some("N-200"), Some("N-200")), "before 39, as before");
    }

    /// proto-reader N-1479 p164: on N-201, a thread a lead opened, his p1 asks w2 and w3, and w2 answers him with a
    /// `coord post "@owner …"` (p2), which stands apart in w2's stream rather than under p1. w3 still owes p1, so
    /// `unanswered_owner` holds it; in w2's stream p1 is no longer "no answer yet", in w3's it is. Before 39, as before.
    #[test]
    fn no_answer_yet_is_for_the_streams_agent_only_while_it_owes_his_message() {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        p.protocol = coord_types::ASKED_PROTOCOL;
        let mut v = mine(201, json!([
            {"n": 1, "at": "2026-11-21T16:00:00Z", "by": "owner", "text": "@w2 @w3 can you both check the stills?", "names": ["w2", "w3"], "told": ["w2", "w3"]},
            {"n": 2, "at": "2026-11-21T16:05:00Z", "by": "w2", "text": "@owner mine are right: 4 of 4.", "names": ["owner"]},
        ]));
        v.opener = Some("lead".into());
        v.form = json!({});
        v.unanswered_owner = vec![1];
        v.asked = serde_json::from_value(json!([{"post": 1, "agent": "w2", "answer": "N-201.p2"}, {"post": 1, "agent": "w3"}])).unwrap();
        p.live.tickets.insert(201, v.clone());
        let open = |p: &crate::Proj, a: &str| lines(p, a, Which::You).into_iter().find_map(|l| match l { Line::Msg { t: 201, n: 1, open, .. } => Some(open), _ => None });
        assert!(lines(&p, "w2", Which::You).iter().any(|l| matches!(l, Line::Msg { t: 201, n: 2, to: None, .. })), "w2's answer stands apart, not under p1");
        assert_eq!((open(&p, "w2"), open(&p, "w3")), (Some(false), Some(true)), "w2 answered it, w3 owes it");
        p.protocol = coord_types::ASKED_PROTOCOL - 1;
        v.asked.clear();
        p.live.tickets.insert(201, v);
        assert_eq!((open(&p, "w2"), open(&p, "w3")), (Some(true), Some(true)), "before 39, as unanswered_owner says");
    }
}
