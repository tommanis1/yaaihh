//! The thread as the program as it ran (plan/layers.md Part I §5, item 3; N-81, item 2). A step makes two lines, one
//! when it is given (`prompt`, to whom, the exact text sent) and one when it is answered (the head's constructor in
//! lower case, its values drawn by type: a commit as its hash and subject, a path as a link into the Files page). A
//! program's step (the runner) is one line, labelled by the program's name; the owner's answer is a line of his.
//! Under a line hangs what happened because of it: under a prompt, the posts written while the step was open by
//! whoever held it or was named by it; under an answer, the post it came in; under an opened ticket (or a branch),
//! that ticket's own thread by the same rule, so every line is itself a thread. The other posts are lines of their
//! own, by time. A halt (slow.rs) is a line where the thing would appear; a line with a halt under it stands open.
//!
//! ```text
//! you       basic.p4 on all engines
//! design    vault/designs/basic-all-engines.md                        › 2 replies
//! prompt    to ann: <the exact text that was sent>                    › 9 replies
//! commit    a1b2c3d  "every engine takes the imported property"
//! runner    check(c, none) → green                                    › 3 replies
//! you       accepted
//! ```
//!
//! Nothing here is written for it in a kind or a call: it is drawn from `TicketView.run` (a step's `rounds` and
//! answer `type`, docket-core 194a6e3), its posts and its halts.

use crate::threads::Page;
use crate::{App, Proj};
use coord_types::TicketView;
use docket_machine::view::RunNode;
use docket_machine::{Loc, Type, ViewHalt};
use rune::{El, col, row, text};
use rune_kit::Kit;
use serde_json::Value;
use std::collections::HashSet;

/// One line of a thread and what hangs under it.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// Unique on the page (it says whether the line stands open): the ticket's id and what in it.
    pub key: String,
    pub label: String,
    pub what: What,
    /// When it happened, for the order of the lines.
    pub at: String,
    pub under: Vec<Line>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum What {
    /// Post N of ticket T.
    Post { t: String, n: u32 },
    /// Step STEP of ticket T given to TO, the exact text sent at PLACE (none yet read).
    Prompt { t: String, step: u32, to: String, place: Option<Loc> },
    /// An answer, HEAD of type TY; OWNER: his own (the constructor is then in the text, "you" the label).
    Head { head: Value, ty: Option<Type>, owner: bool },
    /// A program's step: its call's name and the values it was called with (by their types), and its answer once there.
    Program { call: String, args: Vec<(Value, Option<Type>)>, head: Option<Value>, ty: Option<Type> },
    /// Ticket T, opened here (or a branch of a `par`).
    Ticket { t: String },
    /// Halt N of ticket T (slow mode).
    Halt { t: String, n: u32 },
}

/// What a thread's lines are built from: the tickets by id, and the agents' names (a worker that is none of them nor
/// the owner is a program).
pub struct Ctx<'a> {
    pub tickets: Vec<&'a TicketView>,
    pub agents: Vec<String>,
}

impl<'a> Ctx<'a> {
    pub fn of(p: &'a Proj, v: &'a TicketView) -> Ctx<'a> {
        let mut tickets: Vec<&TicketView> = p.live.tickets.values().collect();
        // the handler tickets (hidden): V's, and those under the tickets of its thread (N-141)
        tickets.extend(v.handlers.iter());
        tickets.extend(p.live.tickets.values().flat_map(|t| t.handlers.iter()).filter(|h| !v.handlers.iter().any(|x| x.id == h.id)));
        // an agent is one with a session (a transcript, or a Claude, Codex, opencode or agy kind): the runner, a
        // program, is listed among the agents but is given no prompt
        let session = |a: &coord_types::AgentView| a.transcript.is_some() || ["claude", "codex", "opencode", "agy"].iter().any(|k| a.kind.to_lowercase().contains(k));
        Ctx { tickets, agents: p.live.agents.iter().filter(|a| session(a)).map(|a| a.name.clone()).collect() }
    }

    fn find(&self, id: &str) -> Option<&'a TicketView> {
        self.tickets.iter().copied().find(|t| t.id == id)
    }
}

/// How deep tickets hang under tickets (a guard: a thread's tree is never this deep).
const DEEP: usize = 8;

/// An open step's round: from its giving to its answer, held by HOLDERS (its worker, and whom its delivery went to).
struct Window {
    key: String,
    from: String,
    to: Option<String>,
    holders: Vec<String>,
    /// The post its answer came in.
    answer: Option<(u32, String)>,
}

/// Thread V's lines.
pub fn lines(cx: &Ctx, v: &TicketView) -> Vec<Line> {
    lines_of(cx, &v.run, v, 0)
}

pub(crate) fn lower(c: &str) -> String {
    c.to_lowercase()
}

/// A head's constructor (`{"head": "Commit", ...}`), or the value's own words when it has none.
pub(crate) fn constructor(h: &Value) -> Option<&str> {
    h.get("head").and_then(Value::as_str)
}

fn at_str(a: &Option<docket_machine::At>) -> Option<String> {
    a.as_ref().map(|a| a.to_string())
}

/// The lines of ticket V whose program's statements are NODES (a branch's are its own), at DEPTH.
fn lines_of(cx: &Ctx, nodes: &[RunNode], v: &TicketView, depth: usize) -> Vec<Line> {
    let mut out: Vec<Line> = Vec::new();
    let mut windows: Vec<Window> = Vec::new();
    walk(cx, nodes, v, depth, &mut Vec::new(), &mut out, &mut windows);
    // a ticket under it that no line of its program opened (opened by an agent's command, `--under`): a line of its own
    // when it was opened, its thread under it; a handler's halts are the thread's to show (`thread`)
    if std::ptr::eq(nodes.as_ptr(), v.run.as_ptr()) && depth + 1 < DEEP {
        let reached = drawn(v, &out);
        for c in cx.tickets.iter().filter(|c| (c.parent.as_deref() == Some(v.id.as_str()) || c.under.as_deref() == Some(v.id.as_str())) && !reached.contains(&c.id) && c.n < docket_machine::HIDDEN && !v.handlers.iter().any(|h| h.id == c.id)) {
            out.push(ticket_line(cx, v, c, &c.run, format!("{}.under.{}", v.id, c.id), c.opened_at.to_string(), depth));
        }
    }
    // the posts: under the step open when each was written, if its author held it or was named by it; an answer under
    // its line; the others lines of their own
    for post in &v.posts {
        // the owner's answers to a card are drawn inside the post they answer
        if post.answers.is_some_and(|a| v.posts.iter().any(|q| q.n == a && !q.asks.is_empty())) {
            continue;
        }
        // a post let go with an earlier one on one card (E1b (a)) is on that card, drawn at the first
        if v.halts.iter().any(|h| crate::slow::on_card(h, post.n) == Some(false)) {
            continue;
        }
        let at = post.at.to_string();
        let line = Line { key: format!("{}.p{}", v.id, post.n), label: crate::refs::person(&post.by), what: What::Post { t: v.id.clone(), n: post.n }, at: at.clone(), under: Vec::new() };
        // the owner's reply to a post (a quote reply, `answers`): under that post's line, wherever it stands
        if let Some(l) = post.answers.and_then(|a| line_mut(&mut out, &format!("{}.p{a}", v.id))) {
            l.under.push(line);
            continue;
        }
        let target = windows
            .iter()
            .filter_map(|w| w.answer.as_ref().filter(|(n, _)| *n == post.n).map(|(_, k)| k.clone()))
            .next()
            .or_else(|| {
                windows
                    .iter()
                    .filter(|w| w.from <= at && w.to.as_ref().is_none_or(|t| at < *t) && w.holders.contains(&post.by))
                    .max_by(|a, b| a.from.cmp(&b.from))
                    .map(|w| w.key.clone())
            });
        match target.and_then(|k| out.iter_mut().find(|l| l.key == k)) {
            Some(l) => l.under.push(line),
            None => out.push(line),
        }
    }
    // its halts where the thing would appear (a post about to be delivered is drawn in its post's place)
    out.extend(halt_lines(v, false));
    // the tickets handling an event under it (a kind's `when`, a `during`'s look) that hold halts: a line each, its
    // halts under it, approved and edited by the handler's own id (N-141, as `coordc show` since cf19d01)
    if std::ptr::eq(nodes.as_ptr(), v.run.as_ptr()) {
        for h in v.handlers.iter().filter(|h| !h.halts.is_empty()) {
            let at = h.halts.iter().map(crate::slow::at_of).min().unwrap_or_else(|| h.opened_at.to_string());
            out.push(Line { key: format!("{}.handler.{}", v.id, h.id), label: "handler".into(), what: What::Ticket { t: h.id.clone() }, at, under: halt_lines(h, true) });
        }
    }
    // by time; at the same time the words first (the owner's words open the ticket its first step is given in)
    let order = |l: &Line| (l.at.clone(), !matches!(l.what, What::Post { .. }));
    for l in &mut out {
        l.under.sort_by_key(order);
    }
    out.sort_by_key(order);
    out
}

/// The keys of the lines that line KEY of thread V hangs under, outermost first: opened, they show it (find.rs).
pub fn above(p: &Proj, v: &TicketView, key: &str) -> Vec<String> {
    fn path(ls: &[Line], key: &str, out: &mut Vec<String>) -> bool {
        for l in ls {
            if l.key == key {
                return true;
            }
            out.push(l.key.clone());
            if path(&l.under, key, out) {
                return true;
            }
            out.pop();
        }
        false
    }
    let cx = Ctx::of(p, v);
    let mut out = Vec::new();
    path(&lines(&cx, v), key, &mut out);
    out
}

/// The line keyed KEY among LS and what hangs under them.
fn line_mut<'a>(ls: &'a mut [Line], key: &str) -> Option<&'a mut Line> {
    for l in ls.iter_mut() {
        if l.key == key {
            return Some(l);
        }
        if let Some(x) = line_mut(&mut l.under, key) {
            return Some(x);
        }
    }
    None
}

/// The lines of V's halts; with ALL not a post's about to be delivered, drawn in its post's place (a handler's posts
/// are no lines, so all of its halts are).
fn halt_lines(v: &TicketView, all: bool) -> Vec<Line> {
    v.halts
        .iter()
        .filter(|h| all || !matches!(h, ViewHalt::Deliver { .. }))
        .map(|h| {
            let n = crate::slow::n_of(h);
            Line { key: format!("{}.h{n}", v.id), label: halt_label(h), what: What::Halt { t: v.id.clone(), n }, at: crate::slow::at_of(h), under: Vec::new() }
        })
        .collect()
}

/// A halt's line label: what would stand there once it is let through.
fn halt_label(h: &ViewHalt) -> String {
    match h {
        ViewHalt::Give { .. } => "prompt".into(),
        ViewHalt::Take { head, .. } => constructor(head).map(lower).unwrap_or_else(|| "answer".into()),
        ViewHalt::Decide { .. } => "decision".into(),
        ViewHalt::Open { .. } => "open".into(),
        ViewHalt::Notice { .. } => "notice".into(),
        ViewHalt::Deliver { by, .. } => crate::refs::person(by),
    }
}

/// The values the program bound as it ran, by name: a step's or a match's result (`r <- …`), a pattern's fields
/// (`Commit(c)` of the `r` it matched). The ticket's form is not among them: it is the thread's opening, already shown.
pub(crate) type Env = Vec<(String, Value, Option<Type>)>;

fn bound_in<'e>(env: &'e Env, name: &str) -> Option<&'e (String, Value, Option<Type>)> {
    env.iter().rev().find(|(n, _, _)| n == name)
}

/// The name a statement binds: `r <- step …` binds `r`.
fn binds(t: &str) -> Option<&str> {
    t.split_once(" <- ").map(|(n, _)| n.trim()).filter(|n| !n.is_empty() && n.chars().all(|c| c.is_alphanumeric() || c == '_'))
}

/// The fields an arm's pattern (`Commit(c)`) binds, from the value its match is on (named SUBJECT).
pub(crate) fn bind_arm(env: &mut Env, subject: &str, pattern: &str) {
    let Some((c, rest)) = pattern.split_once('(') else { return };
    let names: Vec<&str> = rest.trim_end_matches(')').split(',').map(str::trim).collect();
    let Some((_, x, ty)) = bound_in(env, subject).cloned() else { return };
    if constructor(&x) != Some(c.trim()) {
        return;
    }
    let fs: Vec<String> = match ty.as_ref().and_then(|t| t.constructor(c.trim())) {
        Some(fs) => fs.iter().map(|(f, _)| f.clone()).collect(),
        None => x.as_object().map(|o| o.keys().filter(|k| *k != "head").cloned().collect()).unwrap_or_default(),
    };
    let tys: Vec<Option<Type>> = match ty.as_ref().and_then(|t| t.constructor(c.trim())) {
        Some(fs) => fs.iter().map(|(_, t)| Some(t.clone())).collect(),
        None => vec![None; fs.len()],
    };
    for (i, name) in names.iter().enumerate() {
        if let (Some(f), false) = (fs.get(i), name.is_empty() || *name == "_") {
            if let Some(val) = x.get(f) {
                env.push((name.to_string(), val.clone(), tys.get(i).cloned().flatten()));
            }
        }
    }
}

/// A call as written (`check(c, none)`) as its name and the values it was given: a name bound by the program is its
/// value; a literal stays; `none` and a name not bound (the form's, already shown) are left out.
fn call_values(call: &str, env: &Env) -> (String, Vec<(Value, Option<Type>)>) {
    let Some((name, rest)) = call.split_once('(') else { return (call.trim().to_string(), Vec::new()) };
    let mut args = Vec::new();
    for a in rest.trim_end().trim_end_matches(')').split(',').map(str::trim).filter(|a| !a.is_empty() && *a != "none") {
        if let Some((_, x, t)) = bound_in(env, a) {
            args.push((x.clone(), t.clone()));
        } else if !a.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') {
            args.push((Value::String(a.trim_matches('"').to_string()), Some(Type::Text)));
        }
    }
    (name.trim().to_string(), args)
}

/// NODES' lines into OUT, and the rounds of their steps into WINDOWS; ENV the values bound so far.
fn walk(cx: &Ctx, nodes: &[RunNode], v: &TicketView, depth: usize, env: &mut Env, out: &mut Vec<Line>, windows: &mut Vec<Window>) {
    for n in nodes {
        walk_node(cx, n, v, depth, env, out, windows);
        if let (Some(name), Some(x)) = (binds(&n.text), &n.value) {
            env.push((name.to_string(), x.clone(), n.ty.clone()));
        }
    }
}

fn walk_node(cx: &Ctx, n: &RunNode, v: &TicketView, depth: usize, env: &mut Env, out: &mut Vec<Line>, windows: &mut Vec<Window>) {
    {
        match n.what.as_str() {
            "step" => {
                for r in &n.rounds {
                    // not given yet (halted before it is): its give halt stands in its place
                    if r.given.is_none() && r.answered.is_none() {
                        continue;
                    }
                    let to = r.to.clone().or_else(|| n.who.clone()).unwrap_or_default();
                    let given = at_str(&r.given).or_else(|| at_str(&r.answered)).or_else(|| at_str(&n.at)).unwrap_or_default();
                    let key = format!("{}.s{}", v.id, r.step);
                    let delivery = format!("{}.s{}", v.id, r.step);
                    let mut holders: Vec<String> = vec![to.clone()];
                    holders.extend(v.given.iter().filter(|g| g.delivery == delivery).map(|g| g.to.clone()));
                    let answered = at_str(&r.answered);
                    if to == "owner" {
                        // his answer is a line of his; what he is asked is the answer box at the thread's foot
                        if let Some(h) = &r.head {
                            out.push(Line { key: format!("{key}.a"), label: "you".into(), what: What::Head { head: h.clone(), ty: n.ty.clone(), owner: true }, at: answered.clone().unwrap_or(given.clone()), under: Vec::new() });
                            windows.push(Window { key: format!("{key}.a"), from: given, to: answered, holders, answer: r.post.map(|p| (p, format!("{key}.a"))) });
                        }
                        continue;
                    }
                    // an agent is one of the project's, or one whose transcript holds the prompt sent
                    if !cx.agents.contains(&to) && r.place.is_none() {
                        // a program's step (the runner): no prompt; one line, its outcomes under it
                        let (call, args) = call_values(&call_of(&n.text), env);
                        // a round's answer, or the step's value when it ran once
                        let head = r.head.clone().or_else(|| if n.rounds.len() == 1 { n.value.clone() } else { None });
                        out.push(Line { key: key.clone(), label: to.clone(), what: What::Program { call, args, head, ty: n.ty.clone() }, at: given.clone(), under: Vec::new() });
                        windows.push(Window { key: key.clone(), from: given, to: answered, holders, answer: r.post.map(|p| (p, key.clone())) });
                        continue;
                    }
                    out.push(Line { key: key.clone(), label: "prompt".into(), what: What::Prompt { t: v.id.clone(), step: r.step, to: to.clone(), place: r.place.clone() }, at: given.clone(), under: Vec::new() });
                    let mut answer = None;
                    if let Some(h) = &r.head {
                        let label = constructor(h).map(lower).unwrap_or_else(|| "answer".into());
                        let akey = format!("{key}.a");
                        out.push(Line { key: akey.clone(), label, what: What::Head { head: h.clone(), ty: n.ty.clone(), owner: false }, at: answered.clone().unwrap_or(given.clone()), under: Vec::new() });
                        answer = r.post.map(|p| (p, akey));
                    }
                    windows.push(Window { key, from: given, to: answered, holders, answer });
                }
                // the program that took the step's place
                walk(cx, &n.nodes, v, depth, env, out, windows);
            }
            "open" => {
                for id in &n.tickets {
                    let Some(c) = cx.find(id) else { continue };
                    out.push(ticket_line(cx, v, c, &c.run, format!("{}.open.{id}", v.id), at_str(&n.at).unwrap_or_else(|| c.opened_at.to_string()), depth));
                }
                walk(cx, &n.nodes, v, depth, env, out, windows);
            }
            "par" | "any" if !n.tickets.is_empty() => {
                // a branch runs in a ticket of its own: each hangs under the par's line, its statements its thread
                let branches: Vec<&RunNode> = n.nodes.iter().filter(|b| b.what == "branch").collect();
                let mut under = Vec::new();
                for (i, id) in n.tickets.iter().enumerate() {
                    let Some(c) = cx.find(id) else { continue };
                    let body = branches.get(i).or(branches.first()).map(|b| b.nodes.as_slice()).unwrap_or(&[]);
                    let mut l = ticket_line(cx, v, c, body, format!("{}.{}{}.{i}", v.id, n.what, n.line), c.opened_at.to_string(), depth);
                    l.label = format!("branch {}", i + 1);
                    under.push(l);
                }
                let at = at_str(&n.at).or_else(|| under.first().map(|l| l.at.clone())).unwrap_or_default();
                out.push(Line { key: format!("{}.{}{}", v.id, n.what, n.line), label: n.what.clone(), what: What::Program { call: format!("{} branches", under.len()), args: Vec::new(), head: n.value.clone(), ty: None }, at, under });
            }
            "match" => {
                // each arm's pattern binds the fields of the value matched
                let subject = n.text.split_once(" <- ").map_or(n.text.as_str(), |(_, r)| r).trim_start_matches("match").trim().to_string();
                for a in &n.nodes {
                    let mut inner = env.clone();
                    if a.what == "arm" {
                        bind_arm(&mut inner, &subject, &a.text);
                    }
                    walk_node(cx, a, v, depth, &mut inner, out, windows);
                }
            }
            _ => walk(cx, &n.nodes, v, depth, env, out, windows),
        }
    }
}

/// Text T sent to an agent without the `<channel …>` and `</channel>` lines around it: what is between them, the
/// agent reads.
pub(crate) fn without_channel(t: &str) -> String {
    t.lines().filter(|l| !l.trim_start().starts_with("<channel") && !l.trim_start().starts_with("</channel")).collect::<Vec<_>>().join("\n").trim().to_string()
}

/// Ticket C's line under V: labelled by its result's constructor once closed, else its kind; its thread under it.
fn ticket_line(cx: &Ctx, _v: &TicketView, c: &TicketView, nodes: &[RunNode], key: String, at: String, depth: usize) -> Line {
    let label = match &c.status {
        docket_machine::ViewStatus::Done { result } => constructor(result).map(lower).unwrap_or_else(|| c.kind.clone()),
        docket_machine::ViewStatus::Failed { raised } => lower(&raised.exc),
        _ => c.kind.clone(),
    };
    let under = if depth + 1 < DEEP { lines_of(cx, nodes, c, depth + 1) } else { Vec::new() };
    Line { key, label, what: What::Ticket { t: c.id.clone() }, at, under }
}

/// A step's call as written: `v <- step role runner check(c, none)` is `check(c, none)`.
fn call_of(t: &str) -> String {
    let t = t.split_once(" <- ").map_or(t, |(_, r)| r);
    let t = t.strip_prefix("step ").unwrap_or(t);
    let t = t.strip_prefix("role ").unwrap_or(t);
    t.split_once(' ').map_or(t, |(_, r)| r).trim().to_string()
}

/// The tickets whose lines LINES draw (their halts are drawn there).
pub fn drawn(v: &TicketView, lines: &[Line]) -> HashSet<String> {
    fn go(ls: &[Line], out: &mut HashSet<String>) {
        for l in ls {
            if let What::Ticket { t } = &l.what {
                out.insert(t.clone());
            }
            go(&l.under, out);
        }
    }
    let mut out = HashSet::from([v.id.clone()]);
    go(lines, &mut out);
    out
}

fn halted(l: &Line) -> bool {
    matches!(l.what, What::Halt { .. }) || l.under.iter().any(halted)
}

/// The places of the prompts LINES show, and their posts' and heads' texts, the page has not read.
pub fn wanted(pg: &Page, cx: &Ctx, ls: &[Line]) -> Vec<Loc> {
    fn go(cx: &Ctx, ls: &[Line], out: &mut Vec<Loc>) {
        for l in ls {
            match &l.what {
                What::Prompt { place: Some(p), .. } => out.push(p.clone()),
                What::Post { t, n } => {
                    if let Some(p) = cx.find(t).and_then(|v| v.posts.iter().find(|p| p.n == *n)) {
                        out.extend(loc(&serde_json::to_value(&p.text).unwrap_or(Value::Null)));
                    }
                }
                What::Head { head, .. } | What::Program { head: Some(head), .. } => locs(head, out),
                _ => {}
            }
            go(cx, &l.under, out);
        }
    }
    fn locs(v: &Value, out: &mut Vec<Loc>) {
        if let Some(l) = loc(v) {
            out.push(l);
        } else if let Some(o) = v.as_object() {
            o.values().for_each(|x| locs(x, out));
        } else if let Some(xs) = v.as_array() {
            xs.iter().for_each(|x| locs(x, out));
        }
    }
    let mut out = Vec::new();
    go(cx, ls, &mut out);
    out.retain(|l| !pg.texts.contains_key(&crate::texts::key(l)));
    out
}

fn loc(v: &Value) -> Option<Loc> {
    serde_json::from_value(v.get("loc")?.clone()).ok()
}

// ---- values by type ------------------------------------------------------------------------------------------------

/// A piece of a value as the line draws it.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Words(String),
    /// A commit: its hash (shortened when drawn).
    Commit(String),
    /// A path, a link into the Files page (`:line` kept as written).
    Path(String),
    Ticket(String),
}

/// What string S, of type Ref (or of no type known), names.
fn reference(s: &str) -> Piece {
    let s = s.trim();
    if let Some(h) = s.strip_prefix("commit:") {
        return Piece::Commit(h.to_string());
    }
    if let Some(p) = s.strip_prefix("path:") {
        return Piece::Path(p.to_string());
    }
    if (7..=40).contains(&s.len()) && s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Piece::Commit(s.to_string());
    }
    if !s.contains(char::is_whitespace) && (s.contains('/') || s.rsplit_once('.').is_some_and(|(a, b)| !a.is_empty() && (1..=5).contains(&b.len()) && b.chars().all(|c| c.is_ascii_alphanumeric()))) {
        return Piece::Path(s.to_string());
    }
    Piece::Words(s.to_string())
}

/// The title of what TO was sent for step STEP of ticket T in P, opened apart.
fn prompt_title(p: &Proj, t: &str, step: u32, to: &str) -> String {
    crate::popup::title(&crate::refs::ticket_id(p, t).label, &format!("{t}.s{step}"), &format!("what {to} was sent"))
}

/// Delivery D's (`T.sSTEP`) prompt in ticket V's thread in P, as its line opens it apart: its title and its place (a
/// scene's YAAIHH_RAN_UNFOLD).
pub fn prompt_of(p: &Proj, v: &TicketView, d: &str) -> Option<(String, Loc)> {
    fn find<'a>(ls: &'a [Line], d: &str) -> Option<&'a Line> {
        ls.iter().find_map(|l| match &l.what {
            What::Prompt { t, step, place: Some(_), .. } if format!("{t}.s{step}") == d => Some(l),
            _ => find(&l.under, d),
        })
    }
    let cx = Ctx::of(p, v);
    let ls = lines(&cx, v);
    let What::Prompt { t, step, to, place: Some(place) } = &find(&ls, d)?.what else { return None };
    Some((prompt_title(p, t, *step, to), place.clone()))
}

fn first(s: &str) -> String {
    s.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string()
}

/// Value V of type T (None: not known) as pieces; WORDS reads a text at a place.
pub fn pieces(v: &Value, t: Option<&Type>, words: &dyn Fn(&Value) -> String, out: &mut Vec<Piece>) {
    if v.is_null() {
        return;
    }
    match (t, v) {
        (Some(Type::Opt(t)), _) => pieces(v, Some(t), words, out),
        (Some(Type::Ref), Value::String(s)) => out.push(reference(s)),
        (Some(Type::Ticket), Value::String(s)) => out.push(Piece::Ticket(s.clone())),
        (Some(Type::Text), _) => out.push(Piece::Words(first(&words(v)))),
        (Some(Type::List(t)), Value::Array(xs)) => {
            for x in xs.iter().take(8) {
                pieces(x, Some(t), words, out);
            }
            if xs.len() > 8 {
                out.push(Piece::Words(format!("and {} more", xs.len() - 8)));
            }
        }
        (Some(Type::Tuple(ts)), Value::Array(xs)) => {
            for (x, t) in xs.iter().zip(ts) {
                pieces(x, Some(t), words, out);
            }
        }
        (Some(Type::Record(fs)), Value::Object(o)) => {
            for (f, t) in fs {
                if let Some(x) = o.get(f).filter(|x| !x.is_null()) {
                    out.push(Piece::Words(format!("{f}:")));
                    pieces(x, Some(t), words, out);
                }
            }
        }
        (_, Value::Object(o)) if o.contains_key("loc") || o.contains_key("part") || o.contains_key("post") => out.push(Piece::Words(first(&words(v)))),
        (t, Value::Object(o)) if o.get("head").is_some_and(Value::is_string) => {
            let c = o["head"].as_str().unwrap_or_default();
            // a variant nested in a value: its constructor, then its fields (the line's own head is its label)
            if !out.is_empty() {
                out.push(Piece::Words(lower(c)));
            }
            fields(v, t, words, out);
        }
        (_, Value::Object(o)) => {
            for (f, x) in o {
                out.push(Piece::Words(format!("{f}:")));
                pieces(x, None, words, out);
            }
        }
        (_, Value::Array(xs)) => {
            for x in xs.iter().take(8) {
                pieces(x, None, words, out);
            }
        }
        (_, Value::String(s)) if s.contains(char::is_whitespace) => out.push(Piece::Words(first(s))),
        (_, Value::String(s)) => out.push(reference(s)),
        (_, x) => out.push(Piece::Words(x.to_string())),
    }
}

/// Head H's fields, in its constructor's order when its type T says them, as pieces.
pub fn fields(h: &Value, t: Option<&Type>, words: &dyn Fn(&Value) -> String, out: &mut Vec<Piece>) {
    let Some(o) = h.as_object() else { return pieces(h, t, words, out) };
    let c = constructor(h).unwrap_or_default();
    match t.and_then(|t| t.constructor(c)) {
        Some(fs) => {
            for (f, t) in fs {
                if let Some(x) = o.get(f) {
                    pieces(x, Some(t), words, out);
                }
            }
        }
        None => {
            for (f, x) in o.iter().filter(|(f, _)| *f != "head") {
                let _ = f;
                pieces(x, None, words, out);
            }
        }
    }
}

/// The project's folder, as its project file sets it (`repo`, docket-core ed5bb7b): the file is named in the coord
/// folder's `project`; a relative `repo` is from the file's folder, `~/` from home.
fn repo_of(p: &Proj) -> Option<String> {
    // as coordd resolves it (N-412 p129: a `repo` naming one of `repos` too), from a coordd that sends its places
    if let Some(r) = p.live.places.as_ref().and_then(|x| x.repo.clone()) {
        return std::path::Path::new(&r).is_dir().then_some(r);
    }
    let coord = crate::projects::coord_dir(&p.id);
    let file = std::fs::read_to_string(format!("{coord}/project")).ok()?.trim().to_string();
    let t: toml::Table = std::fs::read_to_string(&file).ok()?.parse().ok()?;
    let r = t.get("project")?.get("repo")?.as_str()?.trim().to_string();
    let dir = std::path::Path::new(&file).parent()?.display().to_string();
    let path = if let Some(rest) = r.strip_prefix("~/") {
        format!("{}/{rest}", std::env::var("HOME").unwrap_or_default())
    } else if r.starts_with('/') {
        r
    } else {
        format!("{dir}/{r}")
    };
    std::path::Path::new(&path).is_dir().then_some(path)
}

/// The subjects read, by (project id, hash): None when there is none, or while coordd is asked.
fn read() -> &'static std::sync::Mutex<std::collections::HashMap<(String, String), Option<String>>> {
    static READ: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<(String, String), Option<String>>>> = std::sync::OnceLock::new();
    READ.get_or_init(Default::default)
}

/// The subjects to ask of coordd (Windows), by (project id, hash): asked from the window's loop ([`ask_subjects`]),
/// as drawing holds the project only to read.
static WANT: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

/// Commit H's subject, read once from the project's repository (a commit the commit list does not hold: made on an
/// agent's branch); none when there is no repository or it holds no such commit. On Windows the repository is on the
/// WSL machine: coordd is asked (`Request::Subject`, docket-core5's p129) when its bridge speaks it, and the line is
/// drawn without its subject until the answer comes; an older one gives the hash alone.
fn subject(p: &Proj, h: &str) -> Option<String> {
    let key = (p.id.clone(), h.to_string());
    let read = read();
    if let Some(s) = read.lock().ok()?.get(&key) {
        return s.clone();
    }
    if crate::elsewhere::away() {
        if p.protocol >= crate::SERVER_READS {
            read.lock().ok()?.insert(key.clone(), None);
            WANT.lock().ok()?.push(key);
        }
        return None;
    }
    let s = repo_of(p).and_then(|repo| {
        let o = crate::places::command("git").args(["-C", &repo, "log", "-1", "--format=%s", h, "--"]).output().ok()?;
        o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|s| !s.is_empty())
    });
    read.lock().ok()?.insert(key, s.clone());
    s
}

/// The window's loop: the subjects drawing wanted, asked of their projects' coordds.
pub fn ask_subjects(s: &mut App) {
    let want = std::mem::take(&mut *WANT.lock().unwrap());
    for (pid, hash) in want {
        let Some(p) = s.projects.iter_mut().find(|p| p.id == pid) else { continue };
        match p.link.as_mut().and_then(|l| l.ask(&coord_types::Request::Subject { hash: hash.clone(), repo: None })) {
            Some(id) => {
                p.pending.insert(id, crate::Pending::Subject { hash });
            }
            // the pipe is down: asked again when drawn
            None => {
                read().lock().unwrap().remove(&(pid, hash));
            }
        }
    }
}

/// Coordd's answer for commit HASH of project PID: its subject, or none (no repository, no such commit).
pub fn subject_came(pid: &str, hash: String, ok: bool, out: &str) {
    let s = Some(out.trim().to_string()).filter(|s| ok && !s.is_empty());
    read().lock().unwrap().insert((pid.to_string(), hash), s);
}

// ---- drawing -------------------------------------------------------------------------------------------------------

const LABEL: f32 = 88.0;

/// Thread V drawn as its program ran: its lines, then the halts of tickets under it its lines do not reach (a
/// handler's: the watcher's route).
pub fn thread(s: &App, p: &Proj, v: &TicketView, k: &Kit) -> Vec<El> {
    let pg = &s.tickets;
    let cx = Ctx::of(p, v);
    let mut ls = lines(&cx, v);
    let reached = drawn(v, &ls);
    for (t, h) in crate::slow::shown(p, v).into_iter().filter(|(t, _)| !reached.contains(&t.id)) {
        let n = crate::slow::n_of(h);
        ls.push(Line { key: format!("{}.h{n}", t.id), label: halt_label(h), what: What::Halt { t: t.id.clone(), n }, at: crate::slow::at_of(h), under: Vec::new() });
    }
    ls.sort_by(|a, b| a.at.cmp(&b.at));
    let first = crate::slow::shown(p, v).first().map(|(t, h)| (t.id.clone(), crate::slow::n_of(h)));
    let d = Draw { s, pg, p, v, cx: &cx, first, k };
    ls.iter().map(|l| d.line(l)).collect()
}

struct Draw<'a> {
    s: &'a App,
    pg: &'a Page,
    p: &'a Proj,
    v: &'a TicketView,
    cx: &'a Ctx<'a>,
    /// The halt Return approves.
    first: Option<(String, u32)>,
    k: &'a Kit,
}

impl Draw<'_> {
    fn line(&self, l: &Line) -> El {
        let k = self.k;
        let open = !l.under.is_empty() && (self.pg.ran.contains(&l.key) || l.under.iter().any(halted));
        let colour = match &l.what {
            What::Post { .. } if l.label == "you" => k.c.info.fg,
            What::Post { .. } => k.c.success.fg,
            What::Prompt { .. } | What::Program { .. } => k.c.text_muted,
            What::Halt { .. } => k.c.warning.fg,
            _ => k.c.text_accent,
        };
        let label = row().w(LABEL).child(text(l.label.clone()).font(k.ty.small_medium).color(colour).truncate());
        let mut top = row().w_full().gap(10.0).child(label).child(col().grow(1.0).w(0.0).child(self.body(l)));
        if !l.under.is_empty() {
            let n = l.under.len();
            let said = format!("{} {n} {}", if open { "⌄" } else { "›" }, if n == 1 { "reply" } else { "replies" });
            let key = l.key.clone();
            top = top.child(k.interactive(row().child(text(said).font(k.ty.small).color(k.c.text_accent))).on_press(move |s: &mut App| {
                if !s.tickets.ran.remove(&key) {
                    s.tickets.ran.insert(key.clone());
                }
            }));
        }
        // the comment box's room after its words, when he commented on it (its words draw no marks: I-1476 p9)
        let key = format!("ran-{}", l.key);
        let mut c = col().key(key.clone()).w_full().gap(6.0).child(top).children(crate::ask::room_at(self.s, &key));
        if open {
            // what hangs under it, from where its text starts, a rule down its left
            let under = col().grow(1.0).w(0.0).gap(8.0).pl(12.0).py(2.0).border_sides([0.0, 0.0, 0.0, 2.0], k.c.border_weak).children(l.under.iter().map(|u| self.line(u)));
            c = c.child(row().w_full().child(row().w(LABEL + 10.0)).child(under));
        }
        c
    }

    fn body(&self, l: &Line) -> El {
        let k = self.k;
        let words = |v: &Value| crate::texts::words(self.pg, v);
        match &l.what {
            What::Post { t, n } => {
                let Some(v) = self.cx.find(t) else { return text("") };
                let Some(post) = v.posts.iter().find(|p| p.n == *n) else { return text("") };
                // a post about to be delivered is its halt's card
                if let Some(h) = v.halts.iter().find(|h| crate::slow::on_card(h, *n) == Some(true)) {
                    return crate::slow::card(self.pg, self.p, self.v, v, h, self.first == Some((v.id.clone(), crate::slow::n_of(h))), k);
                }
                crate::post::post(self.s, self.p, v, post, &crate::post::Form::Line, k)
            }
            What::Prompt { t, step, to, place } => {
                let said = match place {
                    Some(l) => words(&serde_json::json!({ "loc": l })),
                    None => "(the text sent is not found yet)".into(),
                };
                // the first line of its words, not the channel tag around them; the whole text opens apart
                // (popup.rs), the channel's tag lines left out: what is between them, the agent reads
                let inner = without_channel(&said);
                let more = inner.trim().lines().count() > 1;
                let line = row().w_full().gap(6.0).child(text(format!("to {to}:")).font(k.ty.small_medium).color(k.c.text_muted)).child(text(format!("{}{}", first(&inner), if more { " …" } else { "" })).font(k.ty.small).color(k.c.text_base).wrap().grow(1.0).w(0.0));
                let Some(place) = place.clone() else { return col().w_full().child(line) };
                let title = prompt_title(self.p, t, *step, to);
                col().w_full().child(k.interactive(line).on_press(move |s: &mut App| crate::popup::open(s, title.clone(), crate::popup::What::Sent { place: place.clone() })))
            }
            What::Head { head, ty, owner } => {
                let mut ps = Vec::new();
                if *owner {
                    ps.push(Piece::Words(constructor(head).map(lower).unwrap_or_default()));
                }
                fields(head, ty.as_ref(), &words, &mut ps);
                self.pieces(&ps)
            }
            What::Program { call, args, head, ty } => {
                let mut ps = vec![Piece::Words(call.clone())];
                for (x, t) in args {
                    pieces(x, t.as_ref(), &words, &mut ps);
                }
                if let Some(h) = head {
                    ps.push(Piece::Words(format!("→ {}", constructor(h).map(lower).unwrap_or_default())));
                    fields(h, ty.as_ref(), &words, &mut ps);
                }
                self.pieces(&ps)
            }
            What::Ticket { t } => {
                let Some(c) = self.cx.find(t) else { return text(t.clone()) };
                // a handler ticket is hidden, with no page to go to: its id and kind as words
                let mut ps = vec![if c.n >= docket_machine::HIDDEN { Piece::Words(format!("{} {}", c.id, c.kind)) } else { Piece::Ticket(c.id.clone()) }];
                if !c.title.is_empty() {
                    ps.push(Piece::Words(first(&c.title)));
                }
                if let docket_machine::ViewStatus::Done { result } = &c.status {
                    fields(result, None, &words, &mut ps);
                }
                self.pieces(&ps)
            }
            What::Halt { t, n } => {
                let Some(ht) = self.cx.find(t) else { return text("") };
                let Some(h) = ht.halts.iter().find(|h| crate::slow::n_of(h) == *n) else { return text("") };
                crate::slow::card(self.pg, self.p, self.v, ht, h, self.first == Some((ht.id.clone(), *n)), k)
            }
        }
    }

    fn pieces(&self, ps: &[Piece]) -> El {
        let k = self.k;
        let mut r = row().w_full().gap(10.0).flex_wrap();
        for p in ps {
            r = r.child(match p {
                // a long text takes the line's width and wraps in it
                Piece::Words(w) if w.chars().count() > 48 => row().w_full().child(text(w.clone()).font(k.ty.small).color(k.c.text_base).wrap().grow(1.0).w(0.0)),
                Piece::Words(w) => text(w.clone()).font(k.ty.small).color(k.c.text_base).wrap(),
                Piece::Commit(h) => {
                    let short: String = h.chars().take(7).collect();
                    let subject = self.p.commits.iter().find(|c| !c.hash.is_empty() && (c.hash.starts_with(h.as_str()) || h.starts_with(c.hash.as_str()))).map(|c| c.subject.clone()).or_else(|| subject(self.p, h));
                    let mut c = row().gap(8.0).child(text(short).font(k.ty.mono).color(k.c.text_base));
                    if let Some(s) = subject {
                        c = c.child(text(format!("\u{201c}{s}\u{201d}")).font(k.ty.small).color(k.c.text_base).wrap());
                    }
                    c
                }
                Piece::Path(path) => {
                    let to = path.clone();
                    k.interactive(row().child(text(path.clone()).font(k.ty.mono).color(k.c.text_accent).wrap_anywhere())).on_press(move |s: &mut App| crate::files::open_at(s, &to))
                }
                Piece::Ticket(id) => {
                    let n = self.cx.find(id).map(|t| t.n);
                    k.interactive(row().child(text(id.clone()).font(k.ty.small_medium).color(k.c.text_accent))).on_press(move |s: &mut App| {
                        if let Some(n) = n {
                            s.tickets.shown = Some(n);
                        }
                    })
                }
            });
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn view(v: Value) -> TicketView {
        let mut base = json!({"id": "N-1", "n": 1, "kind": "thread", "ctx": {}, "form": {}, "title": "basic.p4 on all engines",
            "status": {"is": "resting"}, "posts": [], "children": [], "program": "", "bound": 10, "spent": 0, "given": [],
            "opened_at": "2026-11-14T10:00:00Z"});
        for (k, x) in v.as_object().unwrap() {
            base[k] = x.clone();
        }
        serde_json::from_value(base).unwrap()
    }

    fn said(ls: &[Line], depth: usize, out: &mut Vec<String>) {
        for l in ls {
            out.push(format!("{}{} {}", "  ".repeat(depth), l.label, l.key));
            said(&l.under, depth + 1, out);
        }
    }

    #[test]
    fn a_step_makes_a_prompt_and_an_answer_with_the_posts_under_them() {
        let piece = view(json!({"id": "P-2", "n": 2, "kind": "piece", "title": "implement", "parent": "N-1",
            "status": {"is": "done", "result": {"head": "Done", "c": "a1b2c3d4e"}},
            "posts": [
                {"n": 1, "at": "2026-11-14T10:02:00Z", "by": "dev", "text": "on it"},
                {"n": 2, "at": "2026-11-14T10:05:00Z", "by": "dev", "text": "Commit a1b2c3d4e", "step": 1, "head": {"head": "Commit", "c": "a1b2c3d4e"}},
                {"n": 3, "at": "2026-11-14T10:06:00Z", "by": "runner", "text": "check: green"},
                {"n": 4, "at": "2026-11-14T10:07:00Z", "by": "runner", "text": "Green", "step": 2}],
            "given": [{"to": "dev", "delivery": "P-2.s1", "upto": 0, "at": "2026-11-14T10:01:00Z"}],
            "run": [{"what": "iterate", "line": 1, "source": 0, "text": "iterate", "state": "done", "nodes": [
                {"what": "step", "line": 2, "source": 0, "text": "r <- step who \"Implement\" -> Commit(c: Ref) | Cannot(why: Text)", "who": "dev", "state": "done",
                 "type": {"Variant": [["Commit", [["c", "Ref"]]], ["Cannot", [["why", "Text"]]]]},
                 "rounds": [{"step": 1, "to": "dev", "given": "2026-11-14T10:01:00Z", "answered": "2026-11-14T10:05:00Z", "head": {"head": "Commit", "c": "a1b2c3d4e"}, "post": 2}]},
                {"what": "step", "line": 4, "source": 0, "text": "v <- step role runner check(c, none)", "who": "runner", "state": "done",
                 "rounds": [{"step": 2, "to": "runner", "given": "2026-11-14T10:05:30Z", "answered": "2026-11-14T10:07:00Z", "head": {"head": "Green"}, "post": 4}]}]}]}));
        let thread = view(json!({
            "posts": [{"n": 1, "at": "2026-11-14T10:00:00Z", "by": "owner", "text": "basic.p4 on all engines"},
                      {"n": 2, "at": "2026-11-14T10:08:00Z", "by": "owner", "text": "accepted"}],
            "run": [{"what": "open", "line": 1, "source": 0, "text": "p <- open piece(task, dev)", "state": "done", "at": "2026-11-14T10:00:30Z", "tickets": ["P-2"]}]}));
        let cx = Ctx { tickets: vec![&thread, &piece], agents: vec!["dev".into()] };
        let mut out = Vec::new();
        said(&lines(&cx, &thread), 0, &mut out);
        assert_eq!(out, vec![
            "you N-1.p1",
            "done N-1.open.P-2",
            "  prompt P-2.s1",
            "    dev P-2.p1",
            "  commit P-2.s1.a",
            "    dev P-2.p2",
            "  runner P-2.s2",
            "    runner P-2.p3",
            "    runner P-2.p4",
            "you N-1.p2",
        ]);
    }

    #[test]
    fn a_ticket_opened_under_it_by_a_command_hangs_where_it_was_opened() {
        let e = view(json!({"id": "E-3", "n": 3, "kind": "piece", "under": "N-1", "opened_at": "2026-11-14T10:03:00Z",
            "posts": [{"n": 1, "at": "2026-11-14T10:04:00Z", "by": "dev", "text": "on it"}]}));
        let a = view(json!({"id": "A-1099511627779", "n": 1099511627779u64, "kind": "agent_post", "under": "N-1", "opened_at": "2026-11-14T10:02:00Z"}));
        let thread = view(json!({"handlers": [serde_json::to_value(&a).unwrap()],
            "posts": [{"n": 1, "at": "2026-11-14T10:00:00Z", "by": "owner", "text": "go"}, {"n": 2, "at": "2026-11-14T10:05:00Z", "by": "dev", "text": "opened E-3"}],
            "run": [{"what": "hole", "line": 1, "source": 0, "text": "...", "state": "now"}]}));
        // a handler closed is in the view's tickets, no longer in its handlers: hidden, it is no line
        let mut closed = a.clone();
        closed.id = "O-1099511627776".into();
        closed.n = 1099511627776;
        let cx = Ctx { tickets: vec![&thread, &e, &a, &closed], agents: vec!["dev".into()] };
        let mut out = Vec::new();
        said(&lines(&cx, &thread), 0, &mut out);
        assert_eq!(out, vec!["you N-1.p1", "piece N-1.under.E-3", "  dev E-3.p1", "dev N-1.p2"]);
    }

    #[test]
    fn a_handlers_halts_hang_under_it_under_its_ticket() {
        // N-141: a during's look under P-5 holds a give; the line names the handler, its halt under it
        let look = view(json!({"id": "A-1099511627776", "n": 1099511627776u64, "kind": "act", "under": "P-5", "opened_at": "2026-11-14T10:01:00Z",
            "halts": [{"what": "give", "n": 1, "at": "2026-11-14T10:02:00Z", "step": 1, "worker": "f5-2", "call": "mind", "prompt": "look", "edited": false}]}));
        let part = view(json!({"id": "P-5", "n": 5, "kind": "piece", "opened_at": "2026-11-14T10:00:00Z", "handlers": [serde_json::to_value(&look).unwrap()]}));
        let cx = Ctx { tickets: vec![&part, &look], agents: vec!["f5-2".into()] };
        let mut out = Vec::new();
        said(&lines(&cx, &part), 0, &mut out);
        assert_eq!(out, vec!["handler P-5.handler.A-1099511627776", "  prompt A-1099511627776.h1"]);
    }

    #[test]
    fn a_value_is_drawn_by_its_type() {
        let words = |v: &Value| v.as_str().unwrap_or("").to_string();
        let ty = Type::Variant(vec![("Design".into(), vec![("doc".into(), Type::Ref)]), ("Plan".into(), vec![("doc".into(), Type::Ref), ("why".into(), Type::Text), ("n".into(), Type::Int)])]);
        let mut ps = Vec::new();
        fields(&json!({"head": "Design", "doc": "vault/designs/basic-all-engines.md"}), Some(&ty), &words, &mut ps);
        assert_eq!(ps, vec![Piece::Path("vault/designs/basic-all-engines.md".into())]);
        let mut ps = Vec::new();
        fields(&json!({"head": "Plan", "doc": "commit:a1b2c3d", "why": "two parts\nmore", "n": 2}), Some(&ty), &words, &mut ps);
        assert_eq!(ps, vec![Piece::Commit("a1b2c3d".into()), Piece::Words("two parts".into()), Piece::Words("2".into())]);
        assert_eq!(reference("0ae8f2a91b2c"), Piece::Commit("0ae8f2a91b2c".into()));
        assert_eq!(reference("src/main.rs:12"), Piece::Path("src/main.rs:12".into()));
        assert_eq!(reference("green"), Piece::Words("green".into()));
        assert_eq!(call_of("v <- step role runner check(c, none)"), "check(c, none)");
        // a program's call drawn by the values it was given: c is Commit(c)'s field of the r it matched
        let ty = Type::Variant(vec![("Commit".into(), vec![("c".into(), Type::Ref)]), ("Cannot".into(), vec![("why".into(), Type::Text)])]);
        let mut env: Env = vec![("r".into(), json!({"head": "Commit", "c": "f995bbe"}), Some(ty))];
        bind_arm(&mut env, "r", "Commit(c)");
        assert_eq!(call_values("check(c, none)", &env), ("check".into(), vec![(json!("f995bbe"), Some(Type::Ref))]));
        // a name the program did not bind (the form's text) is left out
        assert_eq!(call_values("intake(text, to)", &env), ("intake".into(), vec![]));
    }

    /// T-5's step 1 prompt line, as its thread draws it, with the popups over it (popup.rs's made-up T-5).
    fn prompt_view(s: &App, rcx: &mut rune::Cx<App>) -> El {
        let k = Kit::neutral(false);
        let p = &s.projects[0];
        let v = &p.live.tickets[&5];
        let cx = Ctx::of(p, v);
        let d = Draw { s, pg: &s.tickets, p, v, cx: &cx, first: None, k: &k };
        let place = v.given[0].place.clone();
        let line = Line { key: "T-5.s1".into(), label: "prompt".into(), what: What::Prompt { t: "T-5".into(), step: 1, to: "docket-core".into(), place }, at: "2026-11-19T19:01:00Z".into(), under: Vec::new() };
        let mut layer = Vec::new();
        crate::popup::layer(s, rcx, &k, &mut layer);
        rune_kit::root(col().key("page").w_full().child(d.line(&line)), layer)
    }

    /// T-5's step 1 prompt line with a line after it, the comment box's layer over them.
    fn prompt_and_box_view(s: &App, cx: &mut rune::Cx<App>) -> El {
        let k = Kit::neutral(false);
        let p = &s.projects[0];
        let v = &p.live.tickets[&5];
        let tcx = Ctx::of(p, v);
        let d = Draw { s, pg: &s.tickets, p, v, cx: &tcx, first: None, k: &k };
        let place = v.given[0].place.clone();
        let line = Line { key: "T-5.s1".into(), label: "prompt".into(), what: What::Prompt { t: "T-5".into(), step: 1, to: "docket-core".into(), place }, at: "2026-11-19T19:01:00Z".into(), under: Vec::new() };
        crate::ask::decide(s, cx);
        let mut layer = Vec::new();
        crate::ask::layer(s, &k, &mut layer);
        let page = col().key("page").w_full().gap(6.0).selection_group(true).child(d.line(&line)).child(text("the next line of the run").font(k.ty.body));
        rune_kit::root(page.instant_changes(), layer)
    }

    /// His comment on a line of a run (I-1476; paper-research N-1477 p18), whose words draw no marks: its box stands in
    /// a room at the line's end, the next line moved down under it, narrow and wide, covering nothing.
    #[test]
    fn a_comment_on_a_line_of_a_run_has_its_box_at_the_lines_end_covering_nothing() {
        use crate::ask::anchor_tests::{NARROW, WIDE, drawn, settle, text_at, under_box};
        for size in [NARROW, WIDE] {
            let (_held, s, _) = crate::popup::tests::ticket_app();
            let mut rt = rune::runtime::Runtime::new(s, prompt_and_box_view, size, rune::Env::new(false, 1.0));
            settle(&mut rt);
            // the prompt's line is pressed, not selected: his comment as `ask::comment_on` makes it for a line of a run
            let path = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("ran-T-5.s1")).map(|e| e.path).expect("the line drawn");
            let anchor = crate::ask::keyed(&path, Some("ran-T-5.s1"));
            assert!(anchor.as_deref().is_some_and(|a| a.ends_with("/ran-T-5.s1")), "{anchor:?}");
            rt.update(move |s, _| {
                let p = crate::ask::Picked { target: "thread:T-5".into(), source: "T-5".into(), label: "a line of T-5".into(), quote: "Do step one.".into(), at: Vec::new(), pi: 0, anchor, block: None, pane: None, texts: Vec::new() };
                crate::ask::write(s, p, "");
            });
            settle(&mut rt);
            let why = format!("{size:?}: box {:?}, room {:?}, line {:?}, under {:?}, picked {:?}", drawn(&rt, "cm-box"), drawn(&rt, crate::ask::ROOM), drawn(&rt, "ran-T-5.s1"), under_box(&rt), rt.state.notes.picked.as_ref().map(|p| (&p.source, &p.anchor)));
            let b = drawn(&rt, "cm-box").unwrap_or_else(|| panic!("its box: {why}"));
            let (line, words) = (drawn(&rt, "ran-T-5.s1").unwrap(), text_at(&rt, "ran-T-5.s1", "Do step one.").unwrap());
            assert!(b[1] >= words[1] + words[3] && b[1] + b[3] <= line[1] + line[3] + 0.5, "in the room at the line's end: {why}");
            let next = text_at(&rt, "page", "the next line of the run").unwrap();
            assert!(next[1] >= b[1] + b[3], "the next line moved down under it: {why}");
            assert!(under_box(&rt).is_empty(), "no text under the box: {why}");
        }
    }

    /// N-1396 p276, 4, item 3: a step's prompt line shows its first line; a press opens the whole text it was sent
    /// apart, its channel's tag lines left out, and the line stays where it was.
    #[test]
    fn a_steps_prompt_opens_apart() {
        use crate::popup::tests::{press, settle, text_under, ticket_app};
        let (_held, s, place) = ticket_app();
        let mut rt = rune::runtime::Runtime::new(s, prompt_view, (1400.0, 900.0), rune::Env::new(false, 1.0));
        settle(&mut rt);
        let line = text_under(&rt, "page", "to docket-core:").expect("the prompt's line");
        assert!(text_under(&rt, "page", "Do step one.").is_some(), "its first line");
        press(&mut rt, line);
        assert_eq!(rt.state.popups.len(), 1);
        assert_eq!(rt.state.popups[0].what, crate::popup::What::Sent { place });
        assert_eq!(rt.state.popups[0].title, "task · T-5.s1 · what docket-core was sent");
        assert!(text_under(&rt, "popup0-dialog", "Do step one.").is_some() && text_under(&rt, "popup0-dialog", "<channel").is_none());
        assert_eq!(text_under(&rt, "page", "to docket-core:"), Some(line), "the line where it was");
    }
}

#[cfg(test)]
mod server_reads_tests {
    fn proj(id: &str, protocol: u32) -> crate::Proj {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: id.into(), id: id.into(), boxed: false }, Vec::new());
        p.protocol = protocol;
        p
    }

    /// On Windows a commit's subject is asked of coordd once, when its bridge serves the server's reads, and drawn when
    /// the answer comes; an older bridge gives the hash alone and nothing is asked.
    #[test]
    fn on_windows_a_subject_is_asked_of_coordd_once_and_kept() {
        crate::elsewhere::as_on_windows(|| {
            let (new, old) = (proj("sr-new", crate::SERVER_READS), proj("sr-old", crate::SERVER_READS - 1));
            assert_eq!(super::subject(&old, "abc1234"), None);
            assert_eq!(super::subject(&new, "abc1234"), None, "drawn without it until the answer");
            assert_eq!(super::subject(&new, "abc1234"), None);
            let want: Vec<_> = super::WANT.lock().unwrap().iter().filter(|(p, _)| p.starts_with("sr-")).cloned().collect();
            assert_eq!(want, [("sr-new".to_string(), "abc1234".to_string())], "asked once, of the new one only");
            super::subject_came("sr-new", "abc1234".into(), true, "yaaihh: the sidebar's parts\n");
            assert_eq!(super::subject(&new, "abc1234").as_deref(), Some("yaaihh: the sidebar's parts"));
            super::subject_came("sr-new", "def5678".into(), false, "no such commit in /p");
            assert_eq!(super::subject(&new, "def5678"), None);
        });
    }

    /// The server's places come first: the definition folder on Windows is a path there, taken as it is.
    #[test]
    fn the_servers_definition_folder_is_the_project_dir() {
        let mut p = proj("sr-places", 3);
        p.live.places = Some(coord_types::Places { coord: "/home/t/agent-coord/x".into(), definition: Some("/home/t/x-flavour".into()), ..Default::default() });
        assert_eq!(crate::edit::project_dir(&p), None, "not a folder on this machine");
        assert_eq!(crate::elsewhere::as_on_windows(|| crate::edit::project_dir(&p)), Some(std::path::PathBuf::from("/home/t/x-flavour")));
    }
}
