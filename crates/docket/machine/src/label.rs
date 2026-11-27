//! What a reference reads as beside its id (the owner N-286.p1: "a reference has both an id ... but also a semantic
//! label"; paper-research N-101 p764, p800). The one source: coordd's `label` request reads every label from here,
//! the posts' words and the commits' subjects apart (they are in files and in git), and `TicketView.title` is
//! `title`. Pure: a text that is a place is read by READ, which the caller gives (coordd reads the session files);
//! with none it is left out. No construct of the language is added: a label comes from what the program already says.
//!
//! - A ticket: what names it, `KIND VALUE` or its words or `FIELD VALUE` (a form field `name`; else, with one Text
//!   field, that field's words whatever their count; else the first Text field whose value is one word: "part
//!   about-note"); a ticket with no parent, the first line of its words (the owner's,
//!   for a change or a thread); a `part` branch, the first of its children that is named; otherwise what names the
//!   nearest ticket above it, then the first line of its kind's doc ("part about-note: one hand-in"), else its kind's
//!   name (a `par` branch, which has none: the call of its first step). A ticket with no parent and no words (one a
//!   binding opens, as `build`): its kind's doc line, the tickets its steps' answers name, and where it is, the head of
//!   its newest answer while open ("a build of the cases, part about-note (P-8): red"), "done" or what it raised once closed. A step's
//!   answer is never a ticket's words: "cut: Cut" is an outcome, not a name (paper-research N-101 p931).
//! - A step: the first line of its call's doc, else the call's name.
//! - An agent: its role when it has one other than member; else, where it works now (the ticket on which it holds a
//!   step, else the newest open one, else the newest), `NAME of T` for `let agent NAME`, `FIELD of T` for an `Agent`
//!   field of T's form, `CALL of T` for the call of the step it held last (its name, not its doc); else "member". T is what names that ticket.

use crate::machine::{Machine, Status, Ticket};
use crate::types::{Text, Type};
use serde_json::Value;

/// A text's words, when they can be had: a new text as it is, a place through READ.
pub type Read<'a> = &'a dyn Fn(&Text) -> Option<String>;

/// Reads only new texts: what the machine has without a file.
pub fn own_words(t: &Text) -> Option<String> {
    if let Text::New(s) = t { Some(s.clone()) } else { None }
}

/// S's first non-empty line without the marks of a heading, a list item or emphasis (`**`, `__`), cut at a word to
/// about N characters, "…" where cut.
/// A line quoted with `> ` is passed over (paper-research N-101 p1710: most of the owner's threads begin with the
/// line they answer); when every line is a quote, the first quote's own words.
pub fn first_line(s: &str, n: usize) -> String {
    let unquoted = |l: &str| {
        let mut l = l.trim();
        while let Some(r) = l.strip_prefix('>') {
            l = r.trim_start();
        }
        l.to_string()
    };
    let lines: Vec<&str> = s.lines().map(str::trim).filter(|l| !marks_off(l).is_empty()).collect();
    let l = lines.iter().find(|l| !l.starts_with('>')).map(|l| l.to_string()).or_else(|| lines.iter().map(|l| unquoted(l)).find(|l| !marks_off(l).is_empty())).unwrap_or_default();
    cut(&l, n)
}

/// S's first non-empty line, quote or not, without the marks first_line takes off, cut to about N characters: a label
/// given in a form, as it was given.
pub fn given_line(s: &str, n: usize) -> String {
    cut(s.lines().map(str::trim).find(|l| !marks_off(l).is_empty()).unwrap_or_default(), n)
}

/// L without the marks of a heading, a list item or emphasis.
fn marks_off(l: &str) -> String {
    l.trim().trim_start_matches('#').trim_start_matches("- ").replace("**", "").replace("__", "").trim().to_string()
}

fn cut(l: &str, n: usize) -> String {
    let l = marks_off(l);
    let l = l.as_str();
    if l.chars().count() <= n {
        return l.to_string();
    }
    let head: String = l.chars().take(n).collect();
    format!("{}…", head.rsplit_once(' ').map_or(head.as_str(), |(h, _)| h).trim_end_matches([',', ';', ':']))
}

/// How long a ticket's words may run in its label.
pub(crate) const WORDS: usize = 60;

/// The ticket's title (`TicketView.title`): the first line of its words, as the machine has them without a file
/// (its form's first Text field in the kind's order, else its first post's), cut as its label is (`first_line`: a
/// quoted line passed over, cut at a word to about WORDS; C16 of 0.2, "short titles": 169 of 238 agents' threads
/// since 19 Nov began with a line over 60 characters); a `part` branch, what names it (its parent's title, live round
/// findings 14); else its label, as far as the machine can read it.
pub fn title(m: &Machine, x: &Ticket) -> String {
    if let Some(l) = given_label(m, x, &own_words) {
        return l;
    }
    if let Some(w) = words(m, x, &own_words) {
        return first_line(&w, WORDS);
    }
    if x.kind == "part" {
        if let Some(n) = named(m, x.t, &own_words) {
            return n;
        }
    }
    ticket(m, x.t, &own_words)
}

/// Ticket X's words: its form's first Text field in the kind's order (the form is a JSON object, whose keys are
/// sorted), else its first post's when that is not a step's answer (an outcome, "cut: Cut", is no ticket's words; nor
/// is what comes after it, as the runner's log).
fn words(m: &Machine, x: &Ticket, read: Read) -> Option<String> {
    let order: Vec<String> = m.own_kind(x).map(|k| k.form.iter().filter(|(_, t)| matches!(t, Type::Text)).map(|(n, _)| n.clone()).collect()).unwrap_or_default();
    let form = m.readable(&x.form);
    order
        .iter()
        .find_map(|f| form.get(f).and_then(|v| value(m, v, read)))
        .or_else(|| x.posts.first().filter(|p| p.head.is_none() && p.step.is_none()).and_then(|p| read(&p.text)))
        .filter(|s| !s.trim().is_empty())
}

/// A form value's words: a string as it is, a text (a place, a span) through READ.
fn value(m: &Machine, v: &Value, read: Read) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        _ => Text::of(v).and_then(|t| m.text(&t).or(Some(t))).and_then(|t| read(&t)),
    }
}

/// The label X was given: its title (`coord title`), else its form's `label` (paper-research N-101 p1676, b).
pub fn given_label(m: &Machine, x: &Ticket, read: Read) -> Option<String> {
    if let Some(t) = &x.titled {
        return Some(t.clone());
    }
    // a closed run's form is its text (`form_text`), read back here
    let form = m.readable(&x.form_of());
    form.get("label").filter(|v| !v.is_null()).and_then(|v| value(m, v, read)).map(|s| given_line(s.trim(), WORDS)).filter(|s| !s.is_empty())
}

/// What names ticket X by its own form, by the form's shape and never by its kind (a kind is a project's, his m1296;
/// paper-research N-1446 p3189): `KIND VALUE` for a field `name` whose value is one word; a form with one Text field,
/// that field's words, the first line cut as a title is ([`first_line`]), whatever their count and with no field's
/// name, since there is no other field it could be ("hello", not "text hello"); a form with several, `FIELD VALUE` for
/// the first whose value is one word, the field's name saying which of them it is.
fn own(m: &Machine, x: &Ticket, read: Read) -> Option<String> {
    let k = m.own_kind(x)?;
    let texts: Vec<&String> = k.form.iter().filter(|(_, t)| matches!(t, Type::Text)).map(|(n, _)| n).collect();
    let words = |f: &str| x.form.get(f).and_then(|v| value(m, v, read)).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let one = |f: &str| words(f).filter(|s| s.chars().count() <= 48 && !s.contains(char::is_whitespace));
    if let Some(v) = texts.iter().find(|f| f.as_str() == "name").and_then(|f| one(f)) {
        return Some(format!("{} {v}", x.kind));
    }
    if let [f] = texts.as_slice() {
        return words(f).map(|s| first_line(&s, WORDS)).filter(|s| !s.is_empty());
    }
    texts.iter().find_map(|f| one(f).map(|v| format!("{f} {v}")))
}

/// What names ticket T: its own form's name, a `part` branch's first named child, else the nearest named ticket above
/// it; a ticket with no parent, the first line of its words.
pub fn named(m: &Machine, t: u64, read: Read) -> Option<String> {
    let x = m.tickets.get(&t)?;
    if let Some(n) = own(m, x, read) {
        return Some(n);
    }
    if x.kind == "part" {
        if let Some(n) = x.children.iter().filter_map(|c| m.tickets.get(c)).find_map(|c| own(m, c, read)) {
            return Some(n);
        }
    }
    match x.parent {
        Some(p) => named(m, p, read),
        None => words(m, x, read).map(|w| first_line(&w, WORDS)).or_else(|| unworded(m, x, read)),
    }
}

/// What a ticket with no parent and no words is: its kind's doc line, with the tickets its steps' answers name, each
/// by what names it and its id, at most three ("a build of the cases, part about-note (P-8)"; paper-research N-101
/// p944); none when it names none, as its kind alone tells it from no other.
fn unworded(m: &Machine, x: &Ticket, read: Read) -> Option<String> {
    let refs = answered_refs(m, x);
    if refs.is_empty() {
        return None;
    }
    // a ticket under X is named by X: it is given by its id alone, so that naming it does not come back here
    let under = |mut t: u64| loop {
        match m.tickets.get(&t).and_then(|y| y.parent) {
            Some(p) if p == x.t => return true,
            Some(p) => t = p,
            None => return false,
        }
    };
    let each: Vec<String> = refs
        .iter()
        .take(3)
        .map(|id| match crate::parse_ticket(id).filter(|t| !under(*t)).and_then(|t| named(m, t, read)) {
            Some(n) => format!("{n} ({id})"),
            None => id.clone(),
        })
        .collect();
    let more = if refs.len() > 3 { format!(" and {} more", refs.len() - 3) } else { String::new() };
    Some(format!("{}, {}{more}", kind_line(m, x).unwrap_or_else(|| x.kind.clone()), each.join(", ")))
}

/// The tickets that X's steps' answers name, in the order named, X itself left out: a field of the head whose value is
/// a ticket's id or a list of them ("carries": ["P-8"]); not one inside a record, which says something else of it (a
/// build "merged_back" onto the line with its commit).
fn answered_refs(m: &Machine, x: &Ticket) -> Vec<String> {
    let own = m.id(x.t);
    let mut out: Vec<String> = Vec::new();
    for h in x.posts.iter().filter(|p| p.step.is_some()).filter_map(|p| p.head.as_ref()?.as_object()) {
        for v in h.values() {
            let ids: Vec<&str> = match v {
                Value::String(s) => vec![s.as_str()],
                Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
                _ => Vec::new(),
            };
            for s in ids {
                if s != own && !out.iter().any(|o| o == s) && m.ticket_named(s).is_some() {
                    out.push(s.to_string());
                }
            }
        }
    }
    out
}

/// Where ticket X is: "done" or what it raised once closed, else the head of its newest step's answer ("red").
fn state(x: &Ticket) -> Option<String> {
    match &x.status {
        Status::Done(_) => Some("done".into()),
        Status::Failed(r) => Some(r.exc.clone()),
        _ => x.posts.iter().rev().filter(|p| p.step.is_some()).find_map(|p| p.head.as_ref()?.get("head")?.as_str().map(str::to_lowercase)),
    }
}

/// Ticket T's label.
pub fn ticket(m: &Machine, t: u64, read: Read) -> String {
    let Some(x) = m.tickets.get(&t) else { return String::new() };
    if let Some(l) = given_label(m, x, read) {
        return l;
    }
    if let Some(n) = own(m, x, read) {
        return n;
    }
    if x.parent.is_none() {
        if let Some(w) = words(m, x, read) {
            return first_line(&w, WORDS);
        }
        // a kind with no doc (coordd's own, as `outcome`) is only its name
        let Some(what) = unworded(m, x, read).or_else(|| kind_line(m, x)) else { return x.kind.clone() };
        return match state(x) {
            Some(s) => format!("{what}: {s}"),
            None => what,
        };
    }
    if x.kind == "part" {
        if let Some(n) = x.children.iter().filter_map(|c| m.tickets.get(c)).find_map(|c| own(m, c, read)) {
            return n;
        }
    }
    // a branch of `par` has no kind of its own to say what it is: the step it runs first does
    let first = (x.kind == "part").then(|| x.step_calls.values().next().map(|(c, _)| call(m, c))).flatten();
    let what = kind_line(m, x).or(first).unwrap_or_else(|| x.kind.clone());
    match x.parent.and_then(|p| named(m, p, read)) {
        Some(n) => format!("{n}: {what}"),
        None => what,
    }
}

/// The first line of X's kind's doc.
fn kind_line(m: &Machine, x: &Ticket) -> Option<String> {
    m.own_kind(x).and_then(|k| k.doc.text.lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_string))
}

/// Call C's label: the first line of its doc, else its name.
pub fn call(m: &Machine, c: &str) -> String {
    m.defs.call_docs.get(c).cloned().filter(|d| !d.is_empty()).unwrap_or_else(|| c.to_string())
}

/// Step N of ticket T's label, when T took such a step.
pub fn step(m: &Machine, t: u64, n: u32) -> Option<String> {
    m.tickets.get(&t)?.step_calls.get(&n).map(|(c, _)| call(m, c))
}

/// AGENT's label.
pub fn agent(m: &Machine, agent: &str, read: Read) -> String {
    agents(m, &[agent], read).pop().unwrap_or_default()
}

/// The labels of AGENTS, each as [`agent`] gives it, the tickets read once for them all (`ranked_each`) and each
/// ticket's name made once: a cloud answer labels its agents together (paper-research N-1446 p2432; p2532: the 59
/// agents' tickets were read 59 times over, 25–31 ms of research's record).
pub fn agents(m: &Machine, agents: &[&str], read: Read) -> Vec<String> {
    let role = |a: &str| m.roles_of(a).into_iter().find(|r| !r.is_empty() && r != "member");
    let roles: Vec<Option<String>> = agents.iter().map(|a| role(a)).collect();
    let unroled: Vec<&str> = agents.iter().zip(&roles).filter(|(_, r)| r.is_none()).map(|(a, _)| *a).collect();
    let mut ranked = ranked_each(m, &unroled).into_iter();
    let mut names: std::collections::HashMap<u64, Option<String>> = std::collections::HashMap::new();
    roles
        .into_iter()
        .map(|r| match r {
            Some(r) => r,
            None => match ranked.next().flatten() {
                Some(((_, _, t), word)) => match names.entry(t).or_insert_with(|| named(m, t, read)) {
                    Some(n) => format!("{word} of {n}"),
                    None => word,
                },
                None => "member".to_string(),
            },
        })
        .collect()
}

/// The ticket AGENT works on that `agent` ranks first, by (holds a step there now, open, number), and its word there;
/// none when it works on none.
fn ranked(m: &Machine, agent: &str) -> Option<((bool, bool, u64), String)> {
    ranked_each(m, &[agent]).pop().flatten()
}

/// For each of AGENTS, what [`ranked`] gives it, the tickets read once for them all. On each ticket an agent's word
/// is the first of: the name it was made under there (`let agent`), the `Agent` field of the form that names it, the
/// call of the last step it held there. An agent named twice is ranked once.
fn ranked_each(m: &Machine, agents: &[&str]) -> Vec<Option<((bool, bool, u64), String)>> {
    let mut at: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (i, a) in agents.iter().enumerate() {
        at.entry(*a).or_insert(i);
    }
    // the tickets they were made on, each with the agents made there and their names
    let mut made_on: std::collections::HashMap<u64, Vec<(usize, String)>> = std::collections::HashMap::new();
    for (a, i) in &at {
        if let Some((t, n)) = m.made_as(a) {
            made_on.entry(t).or_default().push((*i, n));
        }
    }
    // every ticket each works on, by how it does: (holds a step there now, open, number) and its word there; a number
    // is one ticket's, so no two ranks of one agent are the same
    let mut best: Vec<Option<((bool, bool, u64), String)>> = vec![None; agents.len()];
    let mut worded: Vec<usize> = Vec::new();
    for x in m.tickets.values() {
        let awaits = match &x.status {
            Status::Awaits { agent: a, .. } => Some(a.as_str()),
            _ => None,
        };
        let open = x.is_open();
        let mut put = |i: usize, word: String| {
            let rank = (awaits == Some(agents[i]), open, x.t);
            if best[i].as_ref().is_none_or(|(r, _)| rank > *r) {
                best[i] = Some((rank, word));
            }
        };
        worded.clear();
        for (i, n) in made_on.get(&x.t).into_iter().flatten() {
            put(*i, n.clone());
            worded.push(*i);
        }
        // its kind is read only when a form value is one of their names: `own_kind` parses the kind again for every
        // ticket opened under an older text, which made an agent's label 17 ms on research's record (a2h-speed)
        let in_form: Vec<usize> = x.form.as_object().map(|o| o.values().filter_map(|v| at.get(v.as_str()?).copied()).filter(|i| !worded.contains(i)).collect()).unwrap_or_default();
        if !in_form.is_empty() {
            let fields: Vec<String> = m.own_kind(x).map(|k| k.form.iter().filter(|(_, ty)| matches!(ty, Type::Agent)).map(|(f, _)| f.clone()).collect()).unwrap_or_default();
            for i in in_form {
                if worded.contains(&i) {
                    continue;
                }
                if let Some(f) = fields.iter().find(|f| x.form.get(f.as_str()).and_then(Value::as_str) == Some(agents[i])) {
                    put(i, f.clone());
                    worded.push(i);
                }
            }
        }
        // the call's name, the same sort of word as a `let agent` name or a form field (its doc is the step's label)
        for (c, w) in x.step_calls.values().rev() {
            if let Some(&i) = at.get(w.as_str()) {
                if !worded.contains(&i) {
                    put(i, c.clone());
                    worded.push(i);
                }
            }
        }
    }
    for (i, a) in agents.iter().enumerate() {
        let first = at[a];
        if first != i {
            best[i] = best[first].clone();
        }
    }
    best
}

/// The ticket AGENT is on (docket-edge6 N-447 p48, paper-research p49): the one `agent` ranks first (it holds a step
/// there now, else an open one it was named in, else a closed one; the highest number), given as the ticket whose name
/// `agent` shows for it (`named`: its own, else the one above it that has one, else the top), so the window draws that
/// ticket's own label and id where it drew "WORD of NAME". None when it is on none, or that is not a shown ticket.
pub fn agent_on(m: &Machine, agent: &str, read: Read) -> Option<u64> {
    let ((_, _, t), _) = ranked(m, agent)?;
    named_at(m, t, read).filter(|t| *t < crate::machine::HIDDEN)
}

/// The ticket whose name `named` gives for T.
fn named_at(m: &Machine, t: u64, read: Read) -> Option<u64> {
    let x = m.tickets.get(&t)?;
    if own(m, x, read).is_some() || (x.kind == "part" && x.children.iter().filter_map(|c| m.tickets.get(c)).any(|c| own(m, c, read).is_some())) {
        return Some(t);
    }
    match x.parent {
        Some(p) => named_at(m, p, read),
        None => Some(t),
    }
}
