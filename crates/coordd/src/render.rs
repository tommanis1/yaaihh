//! Renderings (plan/interface.md §4, plan/delta-m613.md): the text an agent is given for a delivery, from the
//! templates in `AGENT_BASE/renderings/` (a project's `PROJECT/renderings/` file of the same name wins), in the template
//! language of `policy/call.rs`. What the delivery is about (ticket, kind, step, from) is not written into the text: it
//! goes in the channel's meta, which reaches the agent as attributes of the `<channel>` tag; the body is only the raw
//! text needed. A rendering is made when the session is about to take it and never stored: the recipient's transcript
//! keeps it (owner m526).

use crate::policy::{Places, call::Calls};
use docket_machine::{Delivery, Post, Text, TicketView, Type, What};
use serde_json::{Map, Value, json};

/// Delivery D of ticket VIEW as an agent is given it: the text, and the channel meta that says what it is about
/// ({id, ticket, kind, step, from}), every value a string: Claude Code drops a channel notification whose meta has
/// another, though channel.py answers ok (the dev copy's first claim: meta `"step": 1` never reached the lead; the
/// same event with `"1"` did). Err when no template renders it.
/// ALONE: the rule Reply will post the turn's last message on this ticket (`Core::ticket_replies_on`), so a posts
/// delivery says to end the turn with the reply; else it says to reply with coord post.
/// SINGLE: all the agent holds is on this ticket; the owner's words (a one-off, or his post that Reply answers) then go
/// as they are, the tag saying only who they are from: no id, ticket or routing text (design-two-systems §4). Their
/// dedupe key goes to channel.py outside what the model is shown (`key`, which it takes off the meta).
/// NAMES: whom the rule Reply will name for posts delivery D (`Core::ticket_reply_names`): the authors of the posts
/// told to the reader who can read a reply. Empty, no footer promises a reply (paper-research N-17 post 5, 4); None,
/// the authors of the posts told to the reader, as the posts say.
/// A, B and C.
fn joined(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [a] => a.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// What TO is told of the owner's `@all` that UNREAD brings, the last such: his post, or a post passing it on
/// (consult-lead N-807 post 90, N-412 post 1900; the state from the ticket's posts, `docket_machine::all_of`). Claimed by
/// another (`claimed`, the post `at` that claimed it); handed to TO (`mine`, with how many it reached and the `others`
/// it reached, `besides` TO when it was one of them, and `from`, who passed it to TO); or told only (`handed`, whom it
/// is handed to). Null: none in UNREAD, or TO claimed it.
fn all_input(view: Option<&TicketView>, unread: &[Post], to: &str) -> Value {
    let Some(view) = view else { return Value::Null };
    let words = |t: &Text| Some(resolve(t));
    let brought = |p: &Post| if p.by == "owner" { Some(p.n) } else { p.passes };
    let Some((n, st)) = unread.iter().rev().filter_map(brought).find_map(|n| docket_machine::all_of(&view.posts, n, words).map(|st| (n, st))) else { return Value::Null };
    let Some(p) = view.posts.iter().find(|p| p.n == n) else { return Value::Null };
    let mut reached: Vec<String> = Vec::new();
    for a in p.told.iter().chain(&p.inform) {
        if !reached.contains(a) {
            reached.push(a.clone());
        }
    }
    match &st.claim {
        Some((a, _)) if a == to => Value::Null,
        Some((a, m)) => json!({"post": n, "claimed": a, "at": m}),
        None if st.handed.iter().any(|h| h == to) => {
            let others: Vec<String> = reached.iter().filter(|a| *a != to).cloned().collect();
            let from = st.passed.last().map(|(a, _)| a.clone());
            json!({"post": n, "mine": true, "count": reached.len(), "others": if others.is_empty() { Value::Null } else { json!(joined(&others)) }, "besides": reached.iter().any(|a| a == to), "from": from})
        }
        None => json!({"post": n, "handed": joined(&st.handed)}),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn delivery(places: &Places, d: &Delivery, view: &TicketView, alone: bool, single: bool, again: bool, host: bool, names: Option<&[String]>) -> Result<(String, Value), String> {
    delivery_as(places, d, view, alone, single, again, host, names, false, Hints::default())
}

/// What a posts delivery's footer says beyond the delivery itself, from the hand-over it goes in (N-807).
#[derive(Clone, Copy, Debug, Default)]
pub struct Hints {
    /// The turn holds a step, words or a notice besides posts: its last message answers that, and no part of it is
    /// read for this thread (`reply_parts` reads only a turn given posts), so the footer offers none.
    pub no_parts: bool,
    /// The owner's thread has no title and this agent may give it its first (`Line::Titled`): the footer says how.
    pub untitled: bool,
    /// It goes in one message with others (`handover.tmpl`, 0.2 line 211): rendered without its footer, which the
    /// message's one foot says for all of them from each one's [`Foot`].
    pub batch: bool,
}

/// What a delivery's own footer says, for the one foot of a message that joins it with others (`handover.tmpl`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Foot {
    /// Its id for the reply tool (`N-5.p3`): a posts delivery whose footer offers the reply tool (several, a step
    /// among them, put into the turn); None for one with no footer (nobody to name), a step or a notice.
    pub answer: Option<String>,
    /// The rule Reply posts the turn's last message on its ticket (`alone`).
    pub alone: bool,
    /// The last post is an automatic reply to the reader: its author, and whom the reader's reply is given to.
    pub reply: Option<String>,
    pub to: String,
    /// A step (or its reminder or refusal): the turn's last message answers it.
    pub step: bool,
    /// The owner's posts put into the running turn: how many of his it holds.
    pub owner: usize,
    /// An agent's post put into the running turn whole (the delivery program's `arrived` delivered it).
    pub urgent: bool,
}

/// A delivery rendered: its text, its channel meta, and what its footer says ([`Foot`]).
#[derive(Clone, Debug)]
pub struct Entry {
    pub text: String,
    pub meta: Value,
    pub foot: Foot,
}

/// [`delivery`], INTO: put into the agent's running turn (the owner's posts, N-101 p1214): the footer asks for an
/// answer now with the reply tool, then the work it was doing; always with its id.
#[allow(clippy::too_many_arguments)]
pub fn delivery_as(places: &Places, d: &Delivery, view: &TicketView, alone: bool, single: bool, again: bool, host: bool, names: Option<&[String]>, into: bool, hints: Hints) -> Result<(String, Value), String> {
    entry(places, d, view, alone, single, again, host, names, into, hints).map(|e| (e.text, e.meta))
}

/// [`delivery_as`] with what its footer says; with `hints.batch`, the text has no footer.
#[allow(clippy::too_many_arguments)]
pub fn entry(places: &Places, d: &Delivery, view: &TicketView, alone: bool, single: bool, again: bool, host: bool, names: Option<&[String]>, into: bool, hints: Hints) -> Result<Entry, String> {
    entry_in(places, d, view, alone, single, again, host, names, into, hints)?.ok_or_else(|| format!("no rendering of {} in {}", d.id, dirs(places).join(" or ")))
}

/// [`entry`], None when its renderings hold no template for it ([`template_in`]).
#[allow(clippy::too_many_arguments)]
pub fn entry_in(places: &Places, d: &Delivery, view: &TicketView, alone: bool, single: bool, again: bool, host: bool, names: Option<&[String]>, into: bool, hints: Hints) -> Result<Option<Entry>, String> {
    let (alone, single) = if into { (false, false) } else { (alone, single) };
    let mut foot = Foot::default();
    let d = &seen_only(d);
    let view = Some(view);
    let mut meta = Map::new();
    meta.insert("id".into(), json!(d.id));
    meta.insert("ticket".into(), json!(d.ticket));
    meta.insert("kind".into(), json!(d.kind));
    // from: for posts, who wrote the last one; else coord, which hands steps and notices over
    let from = match &d.what {
        What::Posts { unread } => unread.last().map(|p| p.by.clone()),
        _ => None,
    };
    meta.insert("from".into(), json!(from.unwrap_or_else(|| "coord".into())));
    let ticket = json!({
        "id": d.ticket, "kind": d.kind,
        "title": view.map(|v| v.title.clone()).unwrap_or_default(),
        "opener": view.and_then(|v| v.opener.clone()),
    });
    // JOINS: the earlier post of his this one goes on from (`Post.joins`, the thread's holder; hs-ls N-1446 p38)
    let posts = |ps: &[Post]| -> Value { Value::Array(ps.iter().map(|p| json!({"by": p.by, "text": resolve(&p.text), "joins": p.joins})).collect()) };
    let (name, env) = match &d.what {
        What::Step { step, call, args, unread, head, again, template, refs, prompt: edited, edit, why, left, passed, .. } => {
            meta.insert("step".into(), json!(step.to_string()));
            let prompt = match edited {
                // the owner's text at the step's halt (slow mode): sent as he left it
                Some(p) => p.clone(),
                None => step_prompt(places, call, args, template.as_deref(), edit.as_deref(), refs)?,
            };
            // a reminder says why the last answer was none of the outcomes, in the forming call's words (N-101 p561);
            // or, for a step put into a turn that ended or was cut without its answer (`RemindWhy::Left`), that
            // it was left
            let left = *again && *left;
            let why = (!left).then(|| why.as_deref().map_or_else(|| String::from("."), |w| format!(": {}", w.trim())));
            // passed up after its holder's second answer was not read: those answers, whole, and the reading's reasons
            let passed = match passed.first() {
                None => Value::Null,
                Some(first) => {
                    let needs = match head {
                        Type::Variant(cs) => format!("one of {}", cs.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>().join(", ")),
                        h => format!("a {h}"),
                    };
                    let answers: Vec<Value> = passed.iter().map(|n| json!({"by": n.by, "text": resolve(&n.text), "why": if n.why.is_empty() { "the reading gave no reason" } else { n.why.as_str() }})).collect();
                    json!({"from": first.by, "needs": needs, "answers": answers})
                }
            };
            foot.step = true;
            // put into the running turn (a step its own act raised, delivery.kind's `arrived`): answered there, with
            // `coord answer`, as its turn's last message answers what the turn began with (consult-3 N-1396 p280)
            (if *again { "remind" } else { "step" }, json!({"ticket": ticket, "call": tidy(&prompt), "unread": posts(unread), "gives": gives(places, call, head, view.map(|v| v.kind.as_str())), "why": why, "left": left, "passed": passed, "put": into}))
        }
        // the owner's one post on a thread that Reply answers, to an agent holding nothing else: his words alone
        What::Posts { unread } if alone && single && !host && !again && unread.len() == 1 && unread[0].by == "owner" && all_input(view, unread, &d.to).is_null() => {
            meta.clear();
            meta.insert("from".into(), json!("owner"));
            meta.insert("key".into(), json!(d.id));
            ("words", json!({"text": resolve(&unread[0].text)}))
        }
        What::Posts { unread } => {
            // the last post handed is an automatic reply to the reader (paper-research N-17 post 5, 2): the footer
            // leans to Nothing and says whom anything else is given to, everyone the reply will name (N-17 post 14);
            // nobody to name: no footer at all
            let names: Vec<String> = match names {
                Some(n) => n.to_vec(),
                None => {
                    let mut out: Vec<String> = Vec::new();
                    for p in unread.iter().filter(|p| p.by != d.to && p.wakes().any(|w| *w == d.to)) {
                        if !out.contains(&p.by) {
                            out.push(p.by.clone());
                        }
                    }
                    out
                }
            };
            let mute = names.is_empty();
            let reply = unread.last().filter(|p| p.auto && p.names.contains(&d.to)).map(|p| p.by.clone());
            let into = into && !host;
            // put in whole: the owner's words say so; an agent's post put in is one the delivery program delivered there,
            // answered now only when it wakes its reader: one that only informs it (its brain's line, about what the turn
            // holds: delivery.kind's `arrived`, 0.3) is put in with no footer, and asks for nothing
            let (into, urgent) = (into && unread.iter().any(|p| p.by == "owner"), into && !mute && !unread.iter().any(|p| p.by == "owner"));
            let into_any = into || urgent;
            let (alone, several, host) = (alone && !host && !mute, !alone && !host && !mute && !into_any, host && !mute);
            let last = unread.last().map(|p| p.n);
            // his post put into the turn goes on from one of his not among these that is still this agent's to answer:
            // where that one is for it (`TicketView::whereabouts`, a row only for an agent it woke) says handed, and
            // neither read (the turn it was given in ended) nor answered, and no post of its is on the ticket since: one
            // reply answers both. Told to another (an inform row is no row there), its turn over, or answered, leaves
            // today's `into` (consult-3 N-1446 p181, p187: the footer is for #586 still owed)
            let owed = |j: u32| {
                view.is_some_and(|v| {
                    let here = v.whereabouts.iter().find(|w| w.n == j).and_then(|w| w.agents.iter().find(|a| a.agent == d.to));
                    here.is_some_and(|a| a.handed.is_some() && a.read.is_none() && a.answered.is_none()) && !v.posts.iter().any(|q| q.n > j && q.by == d.to)
                })
            };
            let continues = unread.iter().filter(|p| p.by == "owner").filter_map(|p| p.joins).find(|j| !unread.iter().any(|q| q.n == *j)).filter(|j| into && owed(*j));
            foot = Foot {
                answer: last.filter(|_| several || into_any).map(|n| format!("{}.p{n}", d.ticket)),
                alone: alone && reply.is_none(),
                reply: if alone { reply.clone() } else { None },
                to: joined(&names),
                step: false,
                owner: if into { unread.iter().filter(|p| p.by == "owner").count() } else { 0 },
                urgent,
            };
            // in one message with others: no footer of its own, the message's foot says it (`handover.tmpl`)
            let b = !hints.batch;
            ("posts", json!({"ticket": ticket, "unread": posts(unread), "alone": b && alone && reply.is_none(), "reply": if alone && b { reply } else { None }, "to": joined(&names), "parts": b && several && !hints.no_parts, "stepped": b && several && hints.no_parts, "untitled": hints.untitled, "host": b && host, "into": b && into && continues.is_none(), "joined": if b { continues } else { None }, "urgent": b && urgent, "last": last, "all": all_input(view, unread, &d.to)}))
        }
        What::Closed { result, raised, unread } => {
            let outcome = match (result, raised) {
                (Some(r), _) => outcome(&resolved(r)),
                (None, Some(x)) => format!("{} {}", x.exc, outcome(&resolved(&x.value))).trim_end().to_string(),
                (None, None) => "done".into(),
            };
            ("closed", json!({"ticket": ticket, "outcome": outcome, "unread": posts(unread)}))
        }
        What::Withdrawn { step, call, by, why } => {
            meta.insert("step".into(), json!(step.to_string()));
            ("withdrawn", json!({"ticket": ticket, "step": step, "call": call, "by": by, "why": why}))
        }
        // a post the agent had, taken back by its author (0.2 B8, N-1396): one line, with the post's id
        What::TakenBack { n, by } => ("taken_back", json!({"ticket": ticket, "post": format!("{}.p{n}", d.ticket), "who": if by == "owner" { String::from("The owner") } else { by.clone() }})),
        // the owner's words as they are (a one-off, `give`): nothing but them, from whom they are (meta `from`); no
        // ticket, kind or id in the tag while the agent holds this alone (design-two-systems §4), the id when it
        // holds several. Found again in the transcript by position, not by id (follow.rs, `Core::words_given`)
        What::Words { text, from } => {
            meta.clear();
            meta.insert("from".into(), json!(from));
            meta.insert(if single && !again { "key" } else { "id" }.into(), json!(d.id));
            ("words", json!({"text": resolve(text)}))
        }
        What::Refused { step, why, left, head, call, .. } => {
            meta.insert("step".into(), json!(step.to_string()));
            foot.step = true;
            ("refused", json!({"ticket": ticket, "why": why, "left": left, "gives": gives(places, call, head, view.map(|v| v.kind.as_str()))}))
        }
    };
    let Some(text) = template_in(places, name, &env)? else { return Ok(None) };
    if !again {
        return Ok(Some(Entry { text, meta: Value::Object(meta), foot }));
    }
    // handed a second time, not read the first (paper-research m1005, 3): said first, so a model that did see the
    // first is not led to do it twice; the channel passes it on though it passed the same id before
    meta.insert("again".into(), json!("second hand-over"));
    let text = match template_in(places, "again", &env)? {
        Some(head) => format!("{head}\n\n{text}"),
        None => text,
    };
    Ok(Some(Entry { text, meta: Value::Object(meta), foot }))
}

/// One part of a message that joins several deliveries ([`handover`]): its ticket, its head line's words (the
/// thread's head, `Core::ticket_head`) and the delivery as [`entry`] rendered it with `hints.batch`.
#[derive(Clone, Debug)]
pub struct Part {
    pub ticket: String,
    pub label: String,
    pub entry: Entry,
}

/// Whether delivery ID is posts (`N-5.p3`, or his post told late, `N-5.p3.told`: `docket_machine::delivered_post`):
/// only then does the turn's end read parts of its last message (`Machine::reply_threads`).
pub fn a_post(id: &str) -> bool {
    docket_machine::delivered_post(id).is_some()
}

/// The deliveries PARTS (two or more, on two or more tickets, each with an id) as one message (`handover.tmpl`, 0.2
/// line 211; the owner's N-955: "one prefix then the messages one after another then a close of the batch message"):
/// one head, each part under `#TICKET · label` (CODEX: under the `[ID · from NAME · ticket T KIND]` head a Codex
/// thread's reader finds each delivery by, `follow::codex_mail`), one foot that says once what each part's footer
/// said. The meta: every id, space-separated, in `id` (`follow::channel_ids` splits it); the channel's dedupe key of
/// each part in `keys`, a list, which [`event`] puts beside the meta; `again` when a part is handed again. Parts that
/// are the owner's posts put into the running turn ([`Foot::owner`]): the head counts his posts only. NOTICES, the
/// agent's jobs stopped with its session (`Core::job_notices`, N-1334), are lines of their own under the head, before
/// any part, outside the parts and not named in the foot (hs-ls, N-1334 p6).
pub fn handover(places: &Places, parts: &[Part], codex: bool, notices: &[String]) -> Result<(String, Value), String> {
    let owner: usize = parts.iter().map(|p| p.entry.foot.owner).sum();
    let s = |m: &Value, k: &str| m.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let entries: Vec<Value> = parts
        .iter()
        .map(|p| -> Result<Value, String> {
            let m = &p.entry.meta;
            let head = if codex {
                head(places, Some(&s(m, "id")), &s(m, "from"), Some((&s(m, "ticket"), &s(m, "kind"), &s(m, "step"))))?
            } else if p.label.is_empty() {
                format!("#{}", p.ticket)
            } else {
                format!("#{} · {}", p.ticket, p.label)
            };
            Ok(json!({"head": head, "text": p.entry.text}))
        })
        .collect::<Result<_, _>>()?;
    let answers: Vec<String> = parts.iter().filter_map(|p| p.entry.foot.answer.clone()).collect();
    let alone = parts.iter().find(|p| p.entry.foot.alone || p.entry.foot.reply.is_some()).map(|p| json!({"ticket": p.ticket, "plain": p.entry.foot.alone, "reply": p.entry.foot.reply, "to": p.entry.foot.to}));
    let step = parts.iter().find(|p| p.entry.foot.step).map(|p| p.ticket.clone());
    let posts_only = parts.iter().all(|p| a_post(&s(&p.entry.meta, "id")));
    let mut threads: Vec<&str> = parts.iter().map(|p| p.ticket.as_str()).collect();
    threads.sort();
    threads.dedup();
    let into = owner > 0;
    let open = !into && alone.is_none() && step.is_none() && !answers.is_empty();
    let env = json!({
        "owner": if into { json!(owner) } else { Value::Null },
        "threads": if into { Value::Null } else { json!(threads.len()) },
        "notices": notices,
        "entries": entries,
        "answers": if answers.is_empty() { Value::Null } else { json!(answers.join(", ")) },
        "first": answers.first().and_then(|a| a.split_once('.')).map(|(t, _)| t),
        "alone": if into { Value::Null } else { alone.unwrap_or(Value::Null) },
        "parts": open && posts_only,
        "step": if into { None } else { step },
        "nowhere": open && !posts_only,
    });
    let text = template(places, "handover", &env)?;
    let ids: Vec<String> = parts.iter().map(|p| s(&p.entry.meta, "id")).collect();
    let keys: Vec<String> = parts.iter().map(|p| if p.entry.meta.get("again").is_some() { format!("{} again", s(&p.entry.meta, "id")) } else { s(&p.entry.meta, "id") }).collect();
    let mut meta = json!({"from": if into { "owner" } else { "coord" }, "kind": if into { "owner" } else { "handover" }, "id": ids.join(" "), "keys": keys});
    if parts.iter().any(|p| p.entry.meta.get("again").is_some()) {
        meta["again"] = json!("second hand-over");
    }
    Ok((text, meta))
}

/// The line a Codex thread's reader finds a delivery by (`follow::codex_mail`), `head.tmpl`: ID (none for the owner's
/// words alone), FROM, and ON, its ticket, kind and step (none for a mail); empty when its renderings have none.
pub fn head(places: &Places, id: Option<&str>, from: &str, on: Option<(&str, &str, &str)>) -> Result<String, String> {
    let (ticket, kind, step) = on.map_or((None, None, None), |(t, k, s)| (Some(t), Some(k), Some(s).filter(|s| !s.is_empty())));
    Ok(template_in(places, "head", &json!({"id": id, "from": from, "ticket": ticket, "kind": kind, "step": step}))?.unwrap_or_default())
}

/// The channel's event for one message, CONTENT with its META (`{events: [{content, meta}, ...]}`, channel.py). Only
/// META's strings go in the event's meta. channel.py passes the meta to Claude Code as it is, and Claude Code drops the
/// whole notification, with no trace, when one of its values is anything else ([`delivery_as`]; 47df7030). Every other
/// value goes beside the meta, on the event. A channel that reads it takes it from there (`keys`, channel.py
/// `client`). An older one never passes it on, because every channel.py since 10 Nov passes on only `content` and
/// `meta`. On 20 Nov a hand-over's `keys` list in the meta deafened every session that still ran a channel.py from
/// before build 27 (N-1396 p71). So a field the engine adds can never again cost a session its mail, whatever
/// channel.py it started with.
pub fn event(content: String, meta: Value) -> Value {
    let mut ev = Map::new();
    let mut strings = Map::new();
    if let Value::Object(m) = meta {
        for (k, v) in m {
            if v.is_string() {
                strings.insert(k, v);
            } else if k != "content" && k != "meta" {
                ev.insert(k, v);
            }
        }
    }
    ev.insert("content".into(), Value::String(content));
    ev.insert("meta".into(), Value::Object(strings));
    Value::Object(ev)
}

/// A result value V in words: a variant as its constructor then its fields (`Accept: the note`), else as JSON.
fn outcome(v: &Value) -> String {
    match v {
        Value::Object(o) if o.get("head").is_some_and(Value::is_string) => {
            // a null field (a hand close's `by`) or an empty list is left out; a list of lines (a goal's Done by the
            // lines that met its duties; paper-research N-1446 p2255) is said as its lines
            let fields: Vec<String> = o.iter().filter(|(k, x)| *k != "head" && !x.is_null() && x.as_array().is_none_or(|a| !a.is_empty())).map(|(_, x)| match x {
                Value::String(s) => s.clone(),
                Value::Array(xs) if xs.iter().all(Value::is_string) => xs.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("; "),
                x => x.to_string(),
            }).collect();
            let head = o["head"].as_str().unwrap_or_default();
            if fields.is_empty() { head.to_string() } else { format!("{head}: {}", fields.join("; ")) }
        }
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        _ => v.to_string(),
    }
}

/// D with no post an agent may not see (design §13): one held while a handler decides whom it is for, or dropped by
/// it. The machine hands neither; this keeps them out of any range of posts a delivery carries all the same.
fn seen_only(d: &Delivery) -> Delivery {
    let keep = |ps: &[docket_machine::Post]| ps.iter().filter(|p| p.held.is_none() && !p.dropped).cloned().collect::<Vec<_>>();
    let mut d = d.clone();
    match &mut d.what {
        What::Step { unread, .. } | What::Posts { unread } | What::Closed { unread, .. } => *unread = keep(unread),
        _ => {}
    }
    d
}

/// Template NAME (`step`, `remind`, `posts`, `closed`, `refused`), the project's own if it has one, filled from ENV.
pub fn template(places: &Places, name: &str, env: &Value) -> Result<String, String> {
    template_in(places, name, env)?.ok_or_else(|| format!("no rendering {name}.tmpl in {}", dirs(places).join(" or ")))
}

/// [`template`], None when its renderings have no template NAME: with a set ([`Places::renderings`]), what the set
/// does not hold is not sent.
pub fn template_in(places: &Places, name: &str, env: &Value) -> Result<Option<String>, String> {
    for dir in dirs(places) {
        let path = format!("{dir}/{name}.tmpl");
        if let Ok(text) = std::fs::read_to_string(&path) {
            let out = Calls::render_text(&dir, text.trim_end_matches('\n'), env).map_err(|e| format!("{path}: {e}"))?;
            return Ok(Some(tidy(&out)));
        }
    }
    Ok(None)
}

/// Where PLACES's templates are read from, in order: its set's folder in the project's renderings and the system's,
/// else the project's renderings and the system's.
fn dirs(places: &Places) -> Vec<String> {
    let set = places.renderings.as_deref().map(|s| format!("/{s}")).unwrap_or_default();
    places.project.iter().chain(std::iter::once(&places.base)).map(|p| format!("{p}/renderings{set}")).collect()
}

/// OUT with runs of blank lines made one, and no blank lines at its ends: what an `if` or `for` left empty.
fn tidy(out: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    for l in out.lines() {
        if l.trim().is_empty() && lines.last().is_none_or(|p| p.trim().is_empty()) {
            continue;
        }
        lines.push(l.trim_end());
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

/// The text of T: a new text as it is, a reference read from its place (`follow::text_at`); a place that no longer
/// holds it says so rather than inventing it.
pub fn resolve(t: &Text) -> String {
    match t {
        Text::New(s) => s.clone(),
        Text::Loc { loc } => crate::follow::text_at(&loc.file, loc.offset, loc.len).unwrap_or_else(|| format!("(the text at {}:{} is gone)", loc.file, loc.offset)),
        // a span of the decoded text at a place (an agent's turn filed): read, then cut
        Text::Span { span } => crate::follow::text_at(&span.loc.file, span.loc.offset, span.loc.len)
            .and_then(|s| s.get(span.at as usize..(span.at + span.len) as usize).map(str::to_string))
            .unwrap_or_else(|| format!("(the text at {}:{} is gone)", span.loc.file, span.loc.offset)),
        // the machine resolves parts in views and deliveries (Machine::text), so none should reach here
        Text::Part { part } => format!("(part of ticket {}'s text)", part.t),
        // likewise a post named by number (`post N` in a program)
        Text::Post { post } => format!("(post {} of ticket {})", post.n, post.t),
        // `text of` over texts held where they are: each piece read in turn
        Text::Joined { joined } => joined.iter().map(resolve).collect(),
    }
}

/// A step's prompt as its agent reads it: an inline call's text as the machine filled it, an agent's words it holds
/// put in where they are read (fill_refs); else its call rendered from the calls' templates. A step with no rendering
/// is never sent as its call and arguments: Err, and the step fails with NoCall (`Unrendered`, paper-research m737),
/// its agent never told. The slow mode's give halt shows this text too (plan/layers.md I §5, 3). EDIT: the owner's
/// once-edited template for the step (`Line::Template`), rendered in the call's own slots in place of its template.
pub fn step_prompt(places: &Places, call: &str, args: &Value, template: Option<&str>, edit: Option<&str>, refs: &[Text]) -> Result<String, String> {
    match template {
        Some(t) => Ok(fill_refs(t, refs)),
        None if edit.is_some() => {
            let calls = Calls::load(&places.call_dirs(), &places.overlay).map_err(|e| format!("the calls do not load: {e}"))?;
            calls.render_edited(call, edit.unwrap_or_default(), &resolved(args)).map_err(|e| format!("the owner's prompt for {call}: {e}"))
        }
        None => {
            let calls = Calls::load(&places.call_dirs(), &places.overlay).map_err(|e| format!("the calls do not load: {e}"))?;
            calls.render_step(call, &resolved(args)).ok_or_else(|| match calls.errors.iter().find(|e| e.contains(&format!("call {call} "))) {
                Some(e) => e.clone(),
                None => format!("the call {call} has no template to render"),
            })
        }
    }
}

/// The owner's dry run of a prompt (`Request::DryRun`, H2) as taken under the lock (`Core::ticket_dry_from`): where
/// the calls are read, with his draft of a call file as their overlay; the step that fills it, with its ticket's id
/// and kind and whether it is the ticket asked about (none: no ticket waits on a step of the call); and his prompt for
/// that thread alone, when that is what he edits.
pub struct Dry {
    pub places: Places,
    pub step: Option<(String, String, bool, docket_machine::OwedGive)>,
    pub once: Option<String>,
}

/// [`Dry`] rendered off the lock, as `entry` makes a step's text: the step's prompt (`step_prompt`, with his draft in
/// the calls' overlay, or his prompt for the thread as the step's `edit`), in the agent's `step` rendering with no
/// posts (those unread when it is sent come with it) and what it gives (`gives`, by the draft's head). A prompt of his
/// own for the step stands over a draft of the program, as it does when the step is sent: his text at its halt, else
/// his template for the thread (`own`). Nothing is sent, recorded or counted. Err: why it would not be sent, in the
/// renderer's words.
pub fn dry_run(d: Dry) -> Result<coord_types::DryRunAnswer, String> {
    let Some((on, kind, asked, o)) = d.step else { return Ok(coord_types::DryRunAnswer::default()) };
    let own = d.once.is_none() && (o.prompt.is_some() || o.edit.is_some());
    let prompt = match (&d.once, &o.prompt) {
        (Some(once), _) => step_prompt(&d.places, &o.call, &o.args, o.template.as_deref(), Some(once), &o.refs)?,
        (None, Some(p)) => p.clone(),
        (None, None) => step_prompt(&d.places, &o.call, &o.args, o.template.as_deref(), o.edit.as_deref(), &o.refs)?,
    };
    let head = Calls::load(&d.places.call_dirs(), &d.places.overlay).ok().and_then(|c| c.defs.get(&o.call).map(|x| x.sig.head.clone())).unwrap_or_else(|| o.head.clone());
    let env = json!({"ticket": {"id": on, "kind": kind}, "call": tidy(&prompt), "unread": [], "gives": gives(&d.places, &o.call, &head, Some(kind.as_str())), "why": null, "left": false, "passed": null, "put": false});
    let text = template(&d.places, "step", &env)?;
    Ok(coord_types::DryRunAnswer { on: Some(on), step: o.step, by: o.agent, asked, own, text })
}

/// An inline call's TEMPLATE with each marker `{{@K}}` filled with the words of REFS[K] (What::Step.refs: texts held
/// at a place, an agent's turn, which the machine cannot read): the worker gets the words, never a pointer.
pub fn fill_refs(template: &str, refs: &[Text]) -> String {
    let mut out = template.to_string();
    for (k, t) in refs.iter().enumerate() {
        out = out.replace(&format!("{{{{@{k}}}}}"), &resolve(t));
    }
    out
}

/// A value V for a template: each text in it (a string or `{"loc": ...}`) resolved to its words, so a hole shows them.
pub fn resolved(v: &Value) -> Value {
    match v {
        Value::Object(o) if o.len() == 1 && (o.contains_key("loc") || o.contains_key("span") || o.contains_key("joined")) => Text::of(v).map(|t| Value::String(resolve(&t))).unwrap_or_else(|| v.clone()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), resolved(x))).collect()),
        Value::Array(xs) => Value::Array(xs.iter().map(resolved).collect()),
        _ => v.clone(),
    }
}

/// The lists of an answer to CALL whose entries name other elements of it (`Defs.names`, of every kind with a step of
/// CALL), from the kinds as the files have them now; none when the calls do not load.
pub fn names(places: &Places, call: &str) -> Vec<(String, String)> {
    let Ok(calls) = Calls::load(&places.call_dirs(), &places.overlay) else { return Vec::new() };
    let sigs = calls.defs.iter().map(|(n, d)| (n.clone(), d.sig.clone())).collect();
    let (defs, _) = docket_machine::Defs::build(&calls.kind_files, sigs, calls.named.clone(), 1);
    let mut out: Vec<(String, String)> = defs.names.into_iter().filter(|((_, c), _)| c == call).flat_map(|(_, rs)| rs).collect();
    out.sort();
    out.dedup();
    out
}

/// What a step of CALL, answered with HEAD, gives, with the docs of the calls as the files have them now
/// (`docket_machine::doc::gives`, the one rendering the hover shows too; paper-research N-101 p656). With the calls
/// not loading, the outcomes without docs.
pub fn gives(places: &Places, call: &str, head: &Type, kind: Option<&str>) -> String {
    match Calls::load(&places.call_dirs(), &places.overlay) {
        Ok(calls) => {
            // under an agent's field, what it is given later in KIND (the step's ticket's), from the kinds as the files
            // have them now
            let sigs = calls.defs.iter().map(|(n, d)| (n.clone(), d.sig.clone())).collect();
            let (defs, _) = docket_machine::Defs::build(&calls.kind_files, sigs, calls.named.clone(), 1);
            let docs = calls.defs.iter().filter_map(|(n, d)| d.doc.text.lines().map(str::trim).find(|l| !l.is_empty()).map(|l| (n.clone(), l.to_string()))).collect();
            let mut given = kind.map(|k| docket_machine::doc::given_lines(k, call, &defs.agents_given, &defs.calls, &docs)).unwrap_or_default();
            // under a list whose entries name other elements, which (`Defs.names`): in the step's kind, else in any
            for (p, l) in docket_machine::doc::names_lines(kind, call, &defs.names) {
                given.entry(p).and_modify(|g| *g = format!("{g}; {l}")).or_insert(l);
            }
            docket_machine::doc::gives_with(head, calls.defs.get(call).map(|d| &d.doc), &calls.type_docs, &calls.named, &given)
        }
        Err(_) => docket_machine::doc::gives(head, None, &Default::default(), &Default::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An agent on a renderings set is given what the set's templates make, and nothing the set has no template for:
    /// a set of the posts' words alone gives them with a blank line between and no line of ours; its step, which the set
    /// has no template for, is not rendered (and so fails with NoCall), and no again-head goes before a delivery
    /// handed a second time. The same posts with no set are drawn from the system's templates.
    #[test]
    fn a_set_gives_what_its_templates_make_and_nothing_else() {
        let project = std::env::temp_dir().join(format!("render-set-{}", std::process::id()));
        std::fs::create_dir_all(project.join("renderings/words")).unwrap();
        std::fs::write(project.join("renderings/words/posts.tmpl"), "{{ for p in unread }}{{ p.text }}\n\n{{ end }}\n").unwrap();
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let places = Places { base, project: Some(project.display().to_string()), renderings: Some("words".into()), ..Default::default() };
        let post = |n: u32, by: &str, text: &str| -> Post { serde_json::from_value(json!({"n": n, "at": "2026-11-25T20:00:00Z", "by": by, "text": text})).unwrap() };
        let view: TicketView = serde_json::from_value(json!({"id": "N-3", "n": 3, "kind": "thread", "ctx": {}, "title": "t", "form": {}, "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1, "opened_at": "2026-11-25T19:00:00Z", "waits_on": [], "given": [], "posts": []})).unwrap();
        let d = |id: &str, what: What| Delivery { id: id.into(), to: "pa".into(), t: 3, ticket: "N-3".into(), kind: "thread".into(), upto: 0, what, inform: false };
        let posts = d("N-3.p2", What::Posts { unread: vec![post(1, "owner", "what is 7 times 6?"), post(2, "lead", "and in hex")] });
        let given = |places: &Places, again: bool| entry_in(places, &posts, &view, true, false, again, false, None, false, Hints::default()).unwrap();
        assert_eq!(given(&places, false).unwrap().text, "what is 7 times 6?\n\nand in hex");
        assert_eq!(given(&places, true).unwrap().text, "what is 7 times 6?\n\nand in hex");
        let step = What::Step { step: 1, call: "inline".into(), args: json!({}), unread: Vec::new(), blind: false, head: Type::Text, again: false, template: Some("Do it".into()), prompt: None, edit: None, refs: vec![], why: None, left: false, passed: Vec::new() };
        assert!(entry_in(&places, &d("N-3.s1", step), &view, true, false, false, false, None, false, Hints::default()).unwrap().is_none());
        assert_eq!(head(&places, Some("N-3.p2"), "lead", Some(("N-3", "thread", ""))).unwrap(), "");
        let system = Places { renderings: None, ..places.clone() };
        assert_eq!(given(&system, false).unwrap().text, "owner wrote:\nwhat is 7 times 6?\n\nlead wrote:\nand in hex");
        // the system's head of a Codex thread's delivery, as main.rs and core.rs wrote it before it was a template
        assert_eq!(head(&system, Some("N-3.p2"), "lead", Some(("N-3", "thread", ""))).unwrap(), "[N-3.p2 · from lead · ticket N-3 thread]");
        assert_eq!(head(&system, Some("N-3.s1"), "coord", Some(("N-3", "thread", "1"))).unwrap(), "[N-3.s1 · from coord · ticket N-3 thread step 1]");
        assert_eq!(head(&system, Some("N-3.p2"), "owner", Some(("", "", ""))).unwrap(), "[N-3.p2 · from owner · ticket  ]");
        assert_eq!(head(&system, Some("m12"), "w1", None).unwrap(), "[m12 · from w1]");
        assert_eq!(head(&system, None, "owner", None).unwrap(), "[from owner]");
        std::fs::remove_dir_all(&project).unwrap();
    }

    /// A close's outcome in words: a hand's words as before, its `by` none left out; a goal's Done by its duty check
    /// the lines that met them; Dropped as before (paper-research N-1446 p2255).
    #[test]
    fn a_closes_outcome_says_a_hands_words_and_a_checks_lines() {
        assert_eq!(outcome(&json!({"head": "Done", "why": "landed in 72", "by": null})), "Done: landed in 72");
        let by = json!(["2026-11-23T17:08:47Z job end 41-17", "2026-11-23T17:56:46Z window_release"]);
        assert_eq!(outcome(&json!({"head": "Done", "why": null, "by": by})), "Done: 2026-11-23T17:08:47Z job end 41-17; 2026-11-23T17:56:46Z window_release");
        assert_eq!(outcome(&json!({"head": "Done", "why": null, "by": []})), "Done");
        assert_eq!(outcome(&json!({"head": "Dropped", "why": "he took it back"})), "Dropped: he took it back");
        assert_eq!(outcome(&json!({"head": "Merged", "protocols": [50]})), "Merged: [50]");
    }

    #[test]
    fn an_agents_post_in_inline_text_arrives_as_its_words() {
        let dir = std::env::temp_dir().join(format!("docket-fill-refs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("transcript.jsonl");
        let first = "{\"type\":\"user\",\"message\":{\"content\":\"hello\"}}\n";
        let entry = "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"Sort xs, take the middle; raise on [].\"}]}}\n";
        std::fs::write(&file, format!("{first}{entry}")).unwrap();
        let loc = docket_machine::Loc { file: file.to_string_lossy().into(), offset: first.len() as u64, len: entry.len() as u64 };
        let refs = vec![Text::Loc { loc }, Text::New("(post 9 of N-1, which is not there)".into())];
        assert_eq!(fill_refs("Designed: {{@0}} Also: {{@1}}. Asked: x", &refs), "Designed: Sort xs, take the middle; raise on []. Also: (post 9 of N-1, which is not there). Asked: x");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A hand-up's `text of {log, found, ...}` with `found` a trace's Unclear(why) held in its session (docket-core3,
    /// N-101): the step it is given to reads the joined text with `found`'s words in place, from the real calls.
    #[test]
    fn a_text_of_holding_an_answer_by_reference_reaches_the_step_resolved() {
        let dir = std::env::temp_dir().join(format!("docket-joined-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("session.jsonl");
        let first = "{\"type\":\"user\",\"message\":{\"content\":\"trace it\"}}\n";
        let entry = "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"I can't read the red log.\"}]}}\n";
        std::fs::write(&file, format!("{first}{entry}")).unwrap();
        let found = Text::Loc { loc: docket_machine::Loc { file: file.to_string_lossy().into(), offset: first.len() as u64, len: entry.len() as u64 } };
        let ty = Type::Record(vec![("log".into(), Type::Text), ("found".into(), Type::Text)]);
        let joined = docket_machine::answers::text_of(&json!({"log": "/builds/case-4.log", "found": found.value()}), Some(&ty));
        assert!(joined.get("joined").is_some(), "{joined}");
        let base = format!("{}/../../agent-base/calls", env!("CARGO_MANIFEST_DIR"));
        let calls = Calls::load(&[base], &Default::default()).expect("agent-base/calls load");
        let out = calls.render_step("repair", &resolved(&json!({"what": joined, "evidence": "e", "where": "w", "note": null}))).unwrap();
        assert!(out.contains("Log\n/builds/case-4.log\n\nFound\nI can't read the red log."), "{out}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_a_step_gives_follows_the_head_type_with_no_word_on_the_turn() {
        let t = Type::Variant(vec![("Accept".into(), vec![("note".into(), Type::Opt(Box::new(Type::Text)))]), ("Dismiss".into(), vec![("why".into(), Type::Text)])]);
        assert_eq!(docket_machine::doc::gives(&t, None, &Default::default(), &Default::default()), "This step gives one of:\n  Accept\n    note: Text?\n  Dismiss\n    why");
        // a plain value gives nothing: the task says it all
        assert_eq!(docket_machine::doc::gives(&Type::Text, None, &Default::default(), &Default::default()), "");
    }

    #[test]
    fn the_real_calls_load_and_a_step_renders() {
        let base = format!("{}/../../agent-base/calls", env!("CARGO_MANIFEST_DIR"));
        let calls = Calls::load(&[base], &Default::default()).expect("agent-base/calls load");
        // a fix recorded before `seek`: FOUND none, left out or null, and its step reads as it did (the skipped note's
        // `{{ if }}` keeps its line: one blank line before Fix it)
        let out = calls.render_step("repair", &json!({"what": "mean is off", "evidence": "mean([2,4]) is 6", "where": "calc.py", "note": null})).unwrap();
        assert!(out.contains("What: mean is off") && !out.contains("added:"), "{out}");
        assert!(out.contains("The evidence:\nmean([2,4]) is 6\n\nFix it as its own commit") && !out.contains("You found"), "{out}");
        let args = |found: Value| json!({"what": "mean is off", "evidence": "mean([2,4]) is 6", "where": "calc.py", "note": null, "branch": null, "found": found});
        assert_eq!(calls.render_step("repair", &args(Value::Null)), Some(out));
        // fix.kind's repair, with what its seek found: the one arm of `found` it holds, each on lines of its own
        let out = calls.render_step("repair", &args(json!({"head": "Extends", "pieces": "total at calc.py:3, divided by len"}))).unwrap();
        assert!(out.contains("The evidence:\nmean([2,4]) is 6\n\nYou found what already does part of it:\ntotal at calc.py:3, divided by len\nExtend those pieces"), "{out}");
        assert!(out.contains("\"extends X at file:line\".\nFix it as its own commit") && !out.contains("You searched") && !out.contains("added:"), "{out}");
        let out = calls.render_step("repair", &args(json!({"head": "Nothing", "searched": "mean and sum in calc.py"}))).unwrap();
        assert!(out.contains("You found nothing that already does part of it. You searched:\nmean and sum in calc.py\nFix it") && !out.contains("Extend those"), "{out}");
        // seek (seek.call): a fix's, with its place and a note; an item's, its note as where, or none; one blank line
        // before the ask in each
        let ask = "Before you change anything, look for what already does part of it: a function";
        let fix = calls.render_step("seek", &json!({"what": "mean is off", "where": "calc.py", "note": "see line 3"})).unwrap();
        assert!(fix.starts_with(&format!("You are to do this:\nWhat: mean is off\nWhere: calc.py\n\nThe one who accepted it added:\nsee line 3\n\n{ask}")) && fix.contains("Change nothing yet."), "{fix}");
        let item = calls.render_step("seek", &json!({"what": "faster replay", "where": "vault/items/I-143.md"})).unwrap();
        assert!(item.starts_with(&format!("You are to do this:\nWhat: faster replay\nWhere: vault/items/I-143.md\n\n{ask}")), "{item}");
        let bare = calls.render_step("seek", &json!({"what": "faster replay", "where": null})).unwrap();
        assert!(bare.starts_with(&format!("You are to do this:\nWhat: faster replay\n\n{ask}")), "{bare}");
    }

    #[test]
    fn the_clerks_place_renders() {
        let base = format!("{}/../../agent-base/calls", env!("CARGO_MANIFEST_DIR"));
        let calls = Calls::load(&[base], &Default::default()).expect("agent-base/calls load");
        let out = calls.render_step("place", &json!({"text": "go ahead", "open": "C-1 claim: mean is off"})).unwrap();
        assert!(out.contains("<<<\ngo ahead\n>>>") && out.contains("C-1 claim: mean is off"), "{out}");
    }

    /// A step put into the agent's running turn (one its own act raised, delivery.kind's `arrived`; consult-3 N-1396
    /// p280): `put.tmpl` says to answer it there with `coord answer`, the turn's last message keeping what the turn
    /// began with. Left in that turn, its reminder says so (`left`, `Machine::left_in_turn`), not that a message could
    /// not be read; put into a turn again, with the same lines.
    #[test]
    fn a_step_put_into_a_turn_is_answered_there_and_its_reminder_says_it_was_left() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let places = Places { base, ..Default::default() };
        let head = Type::Variant(vec![("Extends".into(), vec![("pieces".into(), Type::Text)]), ("Nothing".into(), vec![("searched".into(), Type::Text)])]);
        let step = |again: bool, left: bool, why: Option<&str>| What::Step { step: 1, call: "inline".into(), args: json!({}), unread: Vec::new(), blind: false, head: head.clone(), again, template: Some("Look first.".into()), prompt: None, edit: None, refs: Vec::new(), why: why.map(str::to_string), left, passed: Vec::new() };
        let mut d = Delivery { id: "A-9.s1".into(), to: "w2".into(), t: 9, ticket: "A-9".into(), kind: "act".into(), upto: 0, what: step(false, false, None), inform: false };
        let view: TicketView = serde_json::from_value(json!({"id": "A-9", "n": 9, "kind": "act", "ctx": {}, "title": "t", "form": {}, "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1, "opened_at": "2026-11-20T15:00:00Z", "waits_on": [], "given": [], "posts": []})).unwrap();
        let text = |d: &Delivery, into: bool| delivery_as(&places, d, &view, false, false, false, false, None, into, Hints::default()).unwrap().0;
        let put = "\n\nThis step came into your running turn. Answer it now, before you go on: `coord answer A-9 HEAD --FIELD VALUE`, HEAD one of those above, each of its fields as --FIELD VALUE. Then go on with your work: your turn's last message still answers what your turn began with. If your turn ends without this answer, the step comes again after it.";
        let into = text(&d, true);
        assert!(into.starts_with("Look first.") && into.contains("This step gives one of:\n  Extends") && into.ends_with(put), "{into}");
        assert!(!text(&d, false).contains("coord answer"), "out of a turn, its last message answers it");
        d.what = step(true, true, None);
        let again = text(&d, false);
        assert!(again.starts_with("Your turn ended without an answer to this step, so it comes again.\nThis step gives one of:") && !again.contains("could not be read") && !again.contains("coord answer"), "{again}");
        assert!(text(&d, true).ends_with(put), "put into a turn again");
        // a reminder for a reading's reason is as it was
        d.what = step(true, false, Some("It names no piece."));
        assert!(text(&d, false).starts_with("Your last message could not be read as an answer to this step: It names no piece.\nThis step gives"));
    }

    #[test]
    fn an_inline_step_is_given_its_own_text_with_the_agents_words_in_it() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let places = Places { base, ..Default::default() };
        let head = Type::Variant(vec![("Done".into(), vec![]), ("Cannot".into(), vec![("why".into(), Type::Text)])]);
        let what = What::Step { step: 2, call: "inline".into(), args: json!({}), unread: Vec::new(), blind: false, head, again: false, template: Some("Make the change lead designed: {{@0}}".into()), prompt: None, edit: None, refs: vec![Text::New("median after mean".into())], why: None, left: false, passed: Vec::new() };
        let d = Delivery { id: "N-2.s2".into(), to: "fixer".into(), t: 2, ticket: "N-2".into(), kind: "thread".into(), upto: 0, what, inform: false };
        let view: TicketView = serde_json::from_value(json!({"id": "N-2", "n": 2, "kind": "thread", "ctx": {}, "title": "t", "form": {}, "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1, "opened_at": "2026-11-11T19:16:00Z", "waits_on": [], "given": [], "posts": []})).unwrap();
        let (text, meta) = delivery(&places, &d, &view, false, false, false, false, None).unwrap();
        assert!(text.contains("Make the change lead designed: median after mean"), "{text}");
        assert!(text.contains("This step gives one of:\n  Done\n  Cannot\n    why") && !text.contains("turn"), "{text}");
        assert_eq!(meta["step"], "2");
        // its reminder says why, in the forming call's words (paper-research N-101 p561); with none, as before
        let mut d = d;
        let why = |w: Option<&str>| What::Step { step: 2, call: "inline".into(), args: json!({}), unread: Vec::new(), blind: false, head: Type::Variant(vec![("Done".into(), vec![])]), again: true, template: Some("Make the change.".into()), prompt: None, edit: None, refs: Vec::new(), why: w.map(str::to_string), left: false, passed: Vec::new() };
        d.what = why(Some("It names no commit."));
        let (text, _) = delivery(&places, &d, &view, false, false, false, false, None).unwrap();
        assert!(text.starts_with("Your last message could not be read as an answer to this step: It names no commit.\nThis step gives:\n  Done"), "{text}");
        d.what = why(None);
        let (text, _) = delivery(&places, &d, &view, false, false, false, false, None).unwrap();
        assert!(text.starts_with("Your last message could not be read as an answer to this step.\nThis step gives:"), "{text}");
        // passed up after its holder's second answer was not read (paper-research N-101 p757): both answers whole,
        // each with the reading's reason
        let not_read = |text: &str, why: &str| docket_machine::NotRead { by: "f69-1".into(), text: Text::New(text.into()), why: why.into() };
        d.what = What::Step { step: 2, call: "inline".into(), args: json!({}), unread: Vec::new(), blind: false, head: Type::Variant(vec![("Rest".into(), vec![]), ("Say".into(), vec![])]), again: false, template: Some("Mind the piece.".into()), prompt: None, edit: None, refs: Vec::new(), why: None, left: false, passed: vec![not_read("Say: kind 2.\nThe plan asked for two answers.", "It reports two kinds."), not_read("Say: kind 2, again.", "")] };
        let (text, _) = delivery(&places, &d, &view, false, false, false, false, None).unwrap();
        assert!(text.starts_with("Mind the piece.\n\nThis step was given to f69-1 first. Its answers could not be read as one of Rest, Say, so it is yours now. Each answer, whole, with why it could not be read:\n\nf69-1 answered:\nSay: kind 2.\nThe plan asked for two answers.\nNot read: It reports two kinds.\n\nf69-1 answered:\nSay: kind 2, again.\nNot read: the reading gave no reason\n\nThis step gives one of:"), "{text}");
    }

    /// A post the agent had, taken back (0.2 B8, N-1396): one line with the post's id, the owner named "The owner".
    #[test]
    fn a_take_back_is_one_line_with_the_posts_id() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let places = Places { base, ..Default::default() };
        let d = Delivery { id: "N-1104.p14.taken_back".into(), to: "lead".into(), t: 1104, ticket: "N-1104".into(), kind: "thread".into(), upto: 14, what: What::TakenBack { n: 14, by: "owner".into() }, inform: false };
        let view: TicketView = serde_json::from_value(json!({"id": "N-1104", "n": 1104, "kind": "thread", "ctx": {}, "title": "t", "form": {}, "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1, "opened_at": "2026-11-11T19:16:00Z", "waits_on": [], "given": [], "posts": []})).unwrap();
        let (text, meta) = delivery(&places, &d, &view, false, false, false, false, None).unwrap();
        assert_eq!(text, "The owner took back N-1104.p14; do not act on it.");
        assert_eq!((meta["id"].as_str(), meta["from"].as_str()), (Some("N-1104.p14.taken_back"), Some("coord")));
    }

    #[test]
    fn a_withdrawal_says_who_and_why() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let places = Places { base, ..Default::default() };
        let env = |by: Value, why: Value| json!({"ticket": {"id": "C-1"}, "step": 1, "call": "assess", "by": by, "why": why});
        assert_eq!(template(&places, "withdrawn", &env(json!("lead"), json!("a duplicate"))).unwrap(), "Step 1 of C-1 (assess) is withdrawn by lead: a duplicate. It is no longer needed, so do not answer it. If you began on it, stop and end your turn.");
        assert!(template(&places, "withdrawn", &env(Value::Null, Value::Null)).unwrap().starts_with("Step 1 of C-1 (assess) is withdrawn. It"));
    }

    #[test]
    fn a_post_says_how_to_reply_by_what_else_was_given() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let places = Places { base, ..Default::default() };
        let env = |alone: bool| json!({"ticket": {"id": "N-5"}, "unread": [{"by": "w1", "text": "What is 6 times 7?"}], "alone": alone, "parts": !alone, "last": 1});
        assert_eq!(template(&places, "posts", &env(true)).unwrap(), "w1 wrote:\nWhat is 6 times 7?\n\nEnd your turn with your reply; it is posted here. If the sender needs nothing from you, end with the single word Nothing: nothing is posted and nobody is woken. To add something before then: coord post N-5 TEXT");
        // an automatic reply to the reader (N-17 post 5, 2): the footer leans to Nothing and names whom a reply is given to
        let reply = json!({"ticket": {"id": "N-5"}, "unread": [{"by": "w1", "text": "42."}], "reply": "w1", "to": "w1"});
        assert_eq!(template(&places, "posts", &reply).unwrap(), "w1 wrote:\n42.\n\nThis is w1's reply to you. If it asks you something, or something is still open, answer it: your last message is posted here and given to w1. If nothing is open, end your turn with the single word Nothing.");
        // everyone the reply will name (N-17 post 14)
        let reply = json!({"ticket": {"id": "N-5"}, "unread": [{"by": "w3", "text": "and 43?"}, {"by": "w1", "text": "42."}], "reply": "w1", "to": joined(&["w3".into(), "owner".into(), "w1".into()])});
        assert!(template(&places, "posts", &reply).unwrap().ends_with("your last message is posted here and given to w3, owner and w1. If nothing is open, end your turn with the single word Nothing."));
        // nobody can read a reply (the runner): no footer
        let mute = json!({"ticket": {"id": "N-5"}, "unread": [{"by": "runner", "text": "check t12: green"}]});
        assert_eq!(template(&places, "posts", &mute).unwrap(), "runner wrote:\ncheck t12: green");
        assert!(template(&places, "posts", &env(false)).unwrap().ends_with("\n\nYou were given several things at once. A line of your last message that begins with #N-5 starts its part for this thread: the lines from there to the next such line are posted here, under post 1. A line that begins with @NAME starts one for the first thread NAME wrote on. Lines before the first such line go on the first thread you were given and have not answered with the reply tool. Or answer this one at once with the reply tool, to N-5.p1"));
        // a step among them (N-807.p4): its last message answers the step, so no part is offered
        let stepped = json!({"ticket": {"id": "N-5"}, "unread": [{"by": "w1", "text": "What is 6 times 7?"}], "stepped": true, "last": 1});
        assert!(template(&places, "posts", &stepped).unwrap().ends_with("\n\nYou were given several things at once, a step among them: your last message answers the step. To answer this one: the reply tool, to N-5.p1"));
        // the owner's thread with no title: how to give it one, then the footer
        let untitled = json!({"ticket": {"id": "N-5"}, "unread": [{"by": "owner", "text": "the circle is wrong"}], "untitled": true, "alone": true, "last": 1});
        assert_eq!(template(&places, "posts", &untitled).unwrap(), "owner wrote:\nthe circle is wrong\n\nThis thread has no title; its head shows his first line. coord title N-5 A FEW WORDS\n\nEnd your turn with your reply; it is posted here. If the sender needs nothing from you, end with the single word Nothing: nothing is posted and nobody is woken. To add something before then: coord post N-5 TEXT");
    }

    #[test]
    fn a_second_hand_over_says_so() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let places = Places { base, ..Default::default() };
        let head = template(&places, "again", &json!({"ticket": {"id": "N-5"}})).unwrap();
        assert!(head.contains("handed to you before") && head.contains("the same N-5, not a new one"), "{head}");
    }

    #[test]
    fn empty_ifs_leave_no_blank_runs() {
        assert_eq!(tidy("a\n\n\n\nb\n\n"), "a\n\nb");
    }

    /// A posts delivery of N-T to w9: POSTS (number, author), each naming w9.
    fn posts_of(t: u64, posts: &[(u32, &str)]) -> (Delivery, TicketView) {
        let unread: Vec<Post> = posts.iter().map(|(n, by)| serde_json::from_value(json!({"n": n, "at": "2026-11-20T08:00:00Z", "by": by, "text": format!("post {n} by {by}"), "names": ["w9"]})).unwrap()).collect();
        let last = posts.last().unwrap().0;
        (Delivery { id: format!("N-{t}.p{last}"), to: "w9".into(), t, ticket: format!("N-{t}"), kind: "thread".into(), upto: last, what: What::Posts { unread }, inform: false }, view_of(t, "N"))
    }

    fn view_of(t: u64, prefix: &str) -> TicketView {
        serde_json::from_value(json!({"id": format!("{prefix}-{t}"), "n": t, "kind": "thread", "ctx": {}, "title": "", "form": {}, "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1, "opened_at": "2026-11-11T19:16:00Z", "waits_on": [], "given": [], "posts": []})).unwrap()
    }

    /// Delivery D as a part of one message, under LABEL: rendered as `rendered_each` does with `batch`.
    fn part_of(places: &Places, (d, v): &(Delivery, TicketView), label: &str, no_parts: bool, alone: bool, names: Option<&[String]>, into: bool, again: bool) -> Part {
        let hints = Hints { no_parts, untitled: false, batch: true };
        Part { ticket: d.ticket.clone(), label: label.into(), entry: entry(places, d, v, alone, false, again, false, names, into, hints).unwrap() }
    }

    /// Three threads handed at once (0.2 line 211): one head, each thread's posts under `#ID · head` with no footer of
    /// their own, one foot naming each id for the reply tool and how parts of the last message are read; the tag's id
    /// holds every id, and each one's dedupe key goes to channel.py.
    #[test]
    fn several_threads_go_as_one_message_with_one_foot() {
        let places = Places { base: format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), ..Default::default() };
        let parts = [
            part_of(&places, &posts_of(412, &[(851, "a2h-speed")]), "releases", false, false, None, false, false),
            part_of(&places, &posts_of(591, &[(39, "docket-core3"), (40, "docket-core5")]), "close-out", false, false, None, false, false),
            part_of(&places, &posts_of(804, &[(3, "consult-3")]), "", false, false, None, false, false),
        ];
        let (text, meta) = handover(&places, &parts, false, &[]).unwrap();
        assert_eq!(text, "3 threads, one after another.\n\n#N-412 · releases\na2h-speed wrote:\npost 851 by a2h-speed\n\n#N-591 · close-out\ndocket-core3 wrote:\npost 39 by docket-core3\n\ndocket-core5 wrote:\npost 40 by docket-core5\n\n#N-804\nconsult-3 wrote:\npost 3 by consult-3\n\nAnswer each thread with the reply tool, to its id: N-412.p851, N-591.p40, N-804.p3. Or start a line of your last message with # and the thread's id, as #N-412: the lines from there to the next such line are posted on that thread, under the post you were given last. A line that begins with @NAME starts one for the first thread NAME wrote on. Lines before the first such line go on the first thread above that you have not answered with the reply tool. A part that says only Nothing posts nothing.");
        assert_eq!(meta, json!({"from": "coord", "kind": "handover", "id": "N-412.p851 N-591.p40 N-804.p3", "keys": ["N-412.p851", "N-591.p40", "N-804.p3"]}));
        // alone, the same delivery says it all itself, as before
        let (d, v) = posts_of(412, &[(851, "a2h-speed")]);
        let (alone, _) = delivery_as(&places, &d, &v, false, false, false, false, None, false, Hints::default()).unwrap();
        assert!(alone.ends_with("Or answer this one at once with the reply tool, to N-412.p851"), "{alone}");
    }

    /// A step among them (N-807.p4): the foot says the turn's last message answers it and offers no parts; a notice
    /// among them (`closed`, consult-3 N-807.p112, 4): no part of the last message is read, and the foot says so.
    #[test]
    fn a_step_or_a_notice_among_them_offers_no_part() {
        let places = Places { base: format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), ..Default::default() };
        let what = What::Step { step: 3, call: "inline".into(), args: json!({}), unread: Vec::new(), blind: false, head: Type::Variant(vec![("Done".into(), vec![])]), again: false, template: Some("Make the change.".into()), prompt: None, edit: None, refs: Vec::new(), why: None, left: false, passed: Vec::new() };
        let step = (Delivery { id: "C-12.s3".into(), to: "w9".into(), t: 12, ticket: "C-12".into(), kind: "claim".into(), upto: 0, what, inform: false }, view_of(12, "C"));
        let posts = posts_of(412, &[(851, "a2h-speed")]);
        let parts = [part_of(&places, &posts, "releases", true, false, None, false, false), part_of(&places, &step, "claim", true, false, None, false, false)];
        let (text, meta) = handover(&places, &parts, false, &[]).unwrap();
        assert!(text.contains("\n\n#C-12 · claim\nMake the change.\n\nThis step gives:\n  Done\n\n"), "{text}");
        assert!(text.ends_with("\n\nYour last message answers the step on C-12. Answer each of the others with the reply tool, to its id: N-412.p851."), "{text}");
        assert!(!text.contains("start a line") && !text.contains("several things"), "{text}");
        assert_eq!(meta["id"], "N-412.p851 C-12.s3");
        let closed = (Delivery { id: "N-5.closed".into(), to: "w9".into(), t: 5, ticket: "N-5".into(), kind: "thread".into(), upto: 0, what: What::Closed { result: Some(json!("done")), raised: None, unread: Vec::new() }, inform: false }, view_of(5, "N"));
        let parts = [part_of(&places, &posts, "releases", true, false, None, false, false), part_of(&places, &posts_of(591, &[(40, "docket-core5")]), "close-out", true, false, None, false, false), part_of(&places, &closed, "", true, false, None, false, false)];
        let (text, _) = handover(&places, &parts, false, &[]).unwrap();
        assert!(text.contains("\n\n#N-5\nN-5 closed: done\n\n"), "{text}");
        assert!(text.ends_with("\n\nNo part of your last message is posted on these. Answer each with the reply tool, to its id: N-412.p851, N-591.p40."), "{text}");
        assert!(!text.contains("start a line") && !text.contains("answers the step"), "{text}");
    }

    /// The rule Reply posting the turn's last message on one of them (`alone`; the others only inform): said once, in
    /// the foot, for that ticket (consult-3 N-807.p112, 4); and an automatic reply's lean to Nothing, the same way.
    #[test]
    fn where_the_last_message_goes_is_said_once_for_its_ticket() {
        let places = Places { base: format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), ..Default::default() };
        let nobody: &[String] = &[];
        let parts = [part_of(&places, &posts_of(591, &[(40, "docket-core5")]), "close-out", false, false, Some(nobody), false, false), part_of(&places, &posts_of(412, &[(851, "a2h-speed")]), "releases", false, true, None, false, false)];
        let (text, _) = handover(&places, &parts, false, &[]).unwrap();
        assert!(text.ends_with("\n\nEnd your turn with your reply to N-412; it is posted there. If the sender needs nothing from you, end with the single word Nothing: nothing is posted and nobody is woken. To add something before then: coord post N-412 TEXT"), "{text}");
        assert_eq!(text.matches("End your turn").count(), 1, "{text}");
        assert!(!text.contains("reply tool"), "{text}");
    }

    /// The owner's posts put into a running turn, on two threads (consult-3 N-807.p112, 3): one message whose head counts
    /// his posts only, not an agent's post on his thread; an agent's post put in whole (`urgent`) is not joined to it
    /// (`rendered_each`) and keeps its own footer.
    #[test]
    fn the_owners_posts_into_a_turn_count_his_posts_only() {
        let places = Places { base: format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), ..Default::default() };
        let parts = [part_of(&places, &posts_of(944, &[(2, "docket-core2"), (3, "owner")]), "Threads and With you", false, false, None, true, false), part_of(&places, &posts_of(950, &[(7, "owner")]), "the window", false, false, None, true, false)];
        let (text, meta) = handover(&places, &parts, false, &[]).unwrap();
        assert!(text.starts_with("The owner wrote 2 messages while you were working. Answer each now with the reply tool, then go on with what you were doing; your turn's last message still answers what the turn began with.\n\n#N-944 · Threads and With you\ndocket-core2 wrote:\npost 2 by docket-core2\n\nowner wrote:\npost 3 by owner\n\n#N-950 · the window\n"), "{text}");
        assert!(text.ends_with("\n\nAnswer each with the reply tool, to its id: N-944.p3, N-950.p7."), "{text}");
        assert!(!text.contains("The owner wrote this while you were working"), "no part's own footer: {text}");
        assert_eq!((meta["from"].as_str(), meta["kind"].as_str()), (Some("owner"), Some("owner")));
        let (d, v) = posts_of(960, &[(4, "lead")]);
        let urgent = entry(&places, &d, &v, false, false, false, false, None, true, Hints::default()).unwrap();
        assert!(urgent.foot.urgent && urgent.foot.owner == 0, "{:?}", urgent.foot);
        assert!(urgent.text.ends_with("This came while you were working. Answer it now with the reply tool, to N-960.p4, then go on with what you were doing; your turn's last message still answers what the turn began with."), "{}", urgent.text);
    }

    /// His post put into a running turn that goes on from his post given before it (`Post.joins`, the thread's holder;
    /// hs-ls N-1446 p38; his p586 and p587 on N-1104, 20 Nov): the line under it says which post, and asks nothing; while
    /// that post is still the agent's to answer (where it is for the agent, `whereabouts`: handed, not read, not
    /// answered; nothing of its posted on the thread since) the footer asks for one reply to both ids (consult-3
    /// N-1446). Both in one delivery, #586 answered by a post of its here or on another ticket, its turn over, or #586
    /// told to another agent (with the agent handed it only as an inform, or not at all): the line under the later one,
    /// and today's footer for a post of his put in (consult-3 p187). With no `joins`, the text is today's, byte for
    /// byte.
    #[test]
    fn his_post_that_goes_on_from_one_given_before_is_answered_with_it() {
        let places = Places { base: format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), ..Default::default() };
        // where #586 is for each agent it woke, as on N-1104 (`whereabouts`: handed at 18:23:01Z; READ when that turn
        // ended; ANSWERED by a post of the agent's), and the posts AFTER on the thread since
        let view = |woke: &[(&str, Option<&str>, Option<&str>)], after: &[(u32, &str)]| {
            let mut v = view_of(1104, "N");
            let agents: Vec<serde_json::Value> = woke
                .iter()
                .map(|(agent, read, answered)| json!({"agent": agent, "handed": "2026-11-20T18:23:01Z", "read": read, "answered": answered.map(|post| json!({"post": post, "at": "2026-11-20T18:23:09Z"}))}))
                .collect();
            v.whereabouts = vec![serde_json::from_value(json!({"n": 586, "agents": agents})).unwrap()];
            v.posts = after.iter().map(|(n, by)| serde_json::from_value(json!({"n": n, "at": "2026-11-20T18:23:10Z", "by": by, "text": format!("post {n} by {by}"), "names": []})).unwrap()).collect();
            v
        };
        let into = |posts: &[(u32, Option<u32>)], v: &TicketView| {
            let unread: Vec<Post> = posts.iter().map(|(n, j)| serde_json::from_value(json!({"n": n, "at": "2026-11-20T18:23:17Z", "by": "owner", "text": format!("post {n} by owner"), "told": ["paper-research"], "routed": true, "joins": j})).unwrap()).collect();
            let last = posts.last().unwrap().0;
            let d = Delivery { id: format!("N-1104.p{last}"), to: "paper-research".into(), t: 1104, ticket: "N-1104".into(), kind: "thread".into(), upto: last, what: What::Posts { unread }, inform: false };
            delivery_as(&places, &d, v, false, false, false, false, None, true, Hints::default()).unwrap().0
        };
        let today = |n: u32| format!("The owner wrote this while you were working. Answer it now with the reply tool, to N-1104.p{n}, then go on with what you were doing; your turn's last message still answers what the turn began with.");
        let line = |n: u32| format!("owner wrote:\npost {n} by owner\nThis goes on from his #586.\n\n");
        let owed = [("paper-research", None, None)];
        // #586 still paper-research's to answer: one reply to both
        assert_eq!(into(&[(587, Some(586))], &view(&owed, &[])), format!("{}The owner wrote this while you were working, and it goes on from his #586. Answer the two in one reply once your answer to #586 is ready: the reply tool, to N-1104.p587, N-1104.p586. That one post, under #587, answers both, and your turn's end posts nothing more on either. Then go on with what you were doing.", line(587)));
        // both in one delivery
        assert_eq!(into(&[(586, None), (587, Some(586))], &view_of(1104, "N")), format!("owner wrote:\npost 586 by owner\n\n{}{}", line(587), today(587)));
        // #586 answered already, by paper-research's #587: his #588 is answered on its own
        assert_eq!(into(&[(588, Some(586))], &view(&owed, &[(587, "paper-research")])), format!("{}{}", line(588), today(588)));
        // answered by a post of paper-research's on another ticket
        assert_eq!(into(&[(587, Some(586))], &view(&[("paper-research", None, Some("N-1200.p3"))], &[])), format!("{}{}", line(587), today(587)));
        // given to paper-research, and that turn ended with nothing of its posted here (consult-3 p187)
        assert_eq!(into(&[(587, Some(586))], &view(&[("paper-research", Some("2026-11-20T18:23:12Z"), None)], &[])), format!("{}{}", line(587), today(587)));
        // told to docket-core, never to paper-research
        let elsewhere = view(&[("docket-core", None, None)], &[]);
        assert_eq!(into(&[(587, Some(586))], &elsewhere), format!("{}{}", line(587), today(587)));
        // told to docket-core, paper-research holding only an inform row that reached it (consult-3 p187)
        let mut informed = elsewhere.clone();
        informed.given = vec![serde_json::from_value(json!({"to": "paper-research", "delivery": "N-1104.p586", "upto": 586, "at": "2026-11-20T18:23:01Z"})).unwrap()];
        assert_eq!(into(&[(587, Some(586))], &informed), format!("{}{}", line(587), today(587)));
        assert_eq!(into(&[(587, None)], &view(&owed, &[])), format!("owner wrote:\npost 587 by owner\n\n{}", today(587)));
    }

    /// His post told late (`N-1.p5.told`, (c2); proto-reader N-1446 p595): a posts part like any other, its meta id the
    /// delivery's own, so coordd hands it whole (`delivery::hand_deliveries`), and its foot answers the post itself.
    #[test]
    fn his_post_told_late_goes_as_a_post_answered_by_the_posts_id() {
        let places = Places { base: format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), ..Default::default() };
        let (mut d, v) = posts_of(1, &[(5, "owner")]);
        d.id = "N-1.p5.told".into();
        d.upto = 3;
        assert!(a_post(&d.id));
        let told = part_of(&places, &(d, v), "the median", false, false, None, false, false);
        assert_eq!((told.entry.meta["id"].as_str(), told.entry.foot.answer.as_deref()), (Some("N-1.p5.told"), Some("N-1.p5")));
        let parts = [told, part_of(&places, &posts_of(2, &[(7, "lead")]), "", false, false, None, false, false)];
        let (text, meta) = handover(&places, &parts, false, &[]).unwrap();
        assert!(text.starts_with("2 threads, one after another.\n\n#N-1 · the median\nowner wrote:\npost 5 by owner\n\n#N-2\n"), "{text}");
        assert!(text.contains("Answer each thread with the reply tool, to its id: N-1.p5, N-2.p7."), "{text}");
        assert_eq!((meta["kind"].as_str(), meta["id"].as_str()), (Some("handover"), Some("N-1.p5.told N-2.p7")));
    }

    /// For a Codex thread each part keeps the head its reader finds the delivery by (`follow::codex_mail`), under the
    /// message's one head, and the foot comes once at the end.
    #[test]
    fn a_codex_thread_finds_each_part_by_its_head() {
        let places = Places { base: format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), ..Default::default() };
        let parts = [part_of(&places, &posts_of(412, &[(851, "a2h-speed")]), "releases", false, false, None, false, false), part_of(&places, &posts_of(591, &[(40, "docket-core5")]), "close-out", false, false, None, false, false)];
        let (text, _) = handover(&places, &parts, true, &[]).unwrap();
        assert!(text.starts_with("2 threads, one after another.\n\n[N-412.p851 · from a2h-speed · ticket N-412 thread]\na2h-speed wrote:\n"), "{text}");
        assert!(text.contains("\n\n[N-591.p40 · from docket-core5 · ticket N-591 thread]\n"), "{text}");
        assert_eq!(crate::follow::codex_mail(&text).0, ["N-412.p851", "N-591.p40"]);
        assert_eq!(text.matches("Answer each thread").count(), 1);
    }

    /// A part handed again, not taken up the first time: its words say so inside it, the message carries `again`, and
    /// its dedupe key is the again one, so channel.py lets it in though the id went through before.
    #[test]
    fn a_part_handed_again_says_so_inside_and_on_the_message() {
        let places = Places { base: format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), ..Default::default() };
        let parts = [part_of(&places, &posts_of(412, &[(851, "a2h-speed")]), "releases", false, false, None, false, true), part_of(&places, &posts_of(591, &[(40, "docket-core5")]), "close-out", false, false, None, false, false)];
        let (text, meta) = handover(&places, &parts, false, &[]).unwrap();
        assert!(text.contains("#N-412 · releases\nThis was handed to you before and your session did not take it up, so here it is again. If you did see it, it is the same N-412, not a new one: do not do it twice.\n\na2h-speed wrote:"), "{text}");
        assert_eq!(meta["again"], "second hand-over");
        assert_eq!(meta["keys"], json!(["N-412.p851 again", "N-591.p40"]));
    }

    /// The channel's event (N-1396 p71): its meta holds strings only, because every channel.py passes the meta to
    /// Claude Code as it is, and Claude Code drops a notification whose meta has a list. The keys go beside the meta,
    /// where a channel.py from before build 27 never reads them. A delivery's own meta, all strings, stays whole.
    #[test]
    fn a_hand_overs_event_holds_only_strings_in_its_meta_and_its_keys_beside_it() {
        let places = Places { base: format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), ..Default::default() };
        let parts = [part_of(&places, &posts_of(412, &[(851, "a2h-speed")]), "releases", false, false, None, false, true), part_of(&places, &posts_of(591, &[(40, "docket-core5")]), "close-out", false, false, None, false, false)];
        let (text, meta) = handover(&places, &parts, false, &[]).unwrap();
        let ev = event(text.clone(), meta);
        assert_eq!(ev, json!({"content": text, "meta": {"from": "coord", "kind": "handover", "id": "N-412.p851 N-591.p40", "again": "second hand-over"}, "keys": ["N-412.p851 again", "N-591.p40"]}));
        let (d, v) = posts_of(412, &[(851, "a2h-speed")]);
        let (alone, meta) = delivery_as(&places, &d, &v, false, false, false, false, None, false, Hints::default()).unwrap();
        assert!(meta.as_object().unwrap().values().all(Value::is_string), "{meta}");
        assert_eq!(event(alone.clone(), meta.clone()), json!({"content": alone, "meta": meta}));
    }
}
