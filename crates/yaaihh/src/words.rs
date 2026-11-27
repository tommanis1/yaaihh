//! His words cleaned before they went (paper-research N-487; docket-core8's `words` kind, core8-clean e547fa5): the
//! box's side. **No Send opens a words ticket any more (0.2 B9: "nothing held or 'being cleaned' after send"):** the
//! box's words are cleaned in place while he types (changed.rs) and Send sends them as they stand, at once. What is
//! here finishes the messages a window from before left in cleaning (ui.json: "never drop it", N-815), and keeps "as
//! typed" under the posts that came of them. Before, at Send, while the spelling mode was on and the coord spoke
//! `CLEAN_TEXT_PROTOCOL`, the box opened a `words` ticket with what he typed (`{text, window, keep}`) in place of
//! sending it, and followed that ticket:
//! - `step box judge`: the window answers, off its thread, with the certain fixes and Harper's marks left per 100
//!   words (`marks::judge`): `Judged{fixed, per100}`;
//! - `step owner look`: shown where the message will land, under nothing that slides: the cleaned words, each doubt
//!   as a suggestion he can take, and quietly the time until it goes on by itself; Send (Go, or Use with his picks)
//!   or "As I typed it" (AsTyped);
//! - its end: `Send(text)` sends the message as it would have gone, with that text and `typed` naming the ticket
//!   (Post only; N-564 asks for Say), and `Back(why)` puts his words back in the box with why.
//!
//! Nothing here stopped a message: a coord that could not open the ticket (no `words` kind, an older engine) got the
//! message as typed, at once. Nor did a second message from the same box: each one in cleaning is kept until it went
//! or came back, the messages of one box go in the order he sent them (an earlier one still at his look holds a later
//! one that is ready), and what is in cleaning is kept in ui.json, so a swap or a close loses none (the owner: "never
//! drop it", N-815). Under a post that is his words cleaned (`cleaned_from`), "as typed" opens, behind one
//! click, what he typed: the words ticket's form `text`.

use crate::{App, Pending};
use coord_types::Request;
use docket_machine::{TicketView, ViewStatus};
use rune::{El, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// After this long the message is shown as being cleaned, his words readable (paper-research N-487: more than 1–2 s).
pub const SHOWN_AFTER: Duration = Duration::from_millis(1500);

/// What the message's own send needs once the words are back: the chat's (its draft kept for Edit) or a box's.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub enum Then {
    Mailed { typed: String, comments: Vec<crate::ask::Comment> },
    Box,
}

/// One message being cleaned: the box it was sent from (KEY), the request it would have been, what he typed. Kept in
/// ui.json while it is in cleaning (state.rs; KEY's project by id there), so a swap or a close does not lose it.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Words {
    pub key: (usize, String),
    req: Request,
    then: Then,
    /// The box's words as he left them (restored on Back).
    written: String,
    comments: Vec<crate::ask::Comment>,
    /// The words ticket, once coordd has opened it.
    pub t: Option<String>,
    /// Whether this run asked coordd to open it (its answer comes here); one put back from ui.json finds its ticket
    /// among the snapshot's (`claim`).
    #[serde(skip)]
    asked: bool,
    #[serde(skip, default = "Instant::now")]
    pub since: Instant,
    #[serde(skip)]
    judging: Option<mpsc::Receiver<(String, i64)>>,
    /// Judged by this run (put back from ui.json: judged again while its ticket still waits for the judge, as the
    /// answer may not have gone).
    #[serde(skip)]
    judged: bool,
    /// His picks among the look's doubts: by doubt, the word taken (none: left as typed).
    pub picked: Vec<Option<String>>,
    /// Its ticket ended: what is done with it once every earlier message of its box went (`release`).
    ready: Option<Ready>,
    /// His message on its way that this is (outgoing.rs): its bubble stands above the box while it is cleaned.
    #[serde(default)]
    pub out: Option<u64>,
}

/// A words ticket's end, waiting for the messages of its box sent before it.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
enum Ready {
    /// The message goes, as this request; and why it was not cleaned, when coordd said so.
    Send(Request, Option<Unclean>),
    /// His words go back to the box, with why.
    Back(String),
}

#[derive(Default)]
pub struct State {
    pub pending: Vec<Words>,
    /// The posts whose "as typed" is open: (ticket id, post).
    pub open: HashSet<(String, u32)>,
    /// The boxes whose last message went as he typed it because it could not be cleaned (coordd's cleaning call failed
    /// and said so on the words ticket), and why: the line under the box says so, and leads to Accounts when no model
    /// answered, until his next message there (morning.md:136; consult-2 N-591.p37, 3).
    pub unclean: HashMap<(usize, String), Unclean>,
}

/// Why a message could not be cleaned, as far as the words ticket's notice tells it.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Unclean {
    /// No model answered (coordd's clean.rs: "no model answered: …"): the way to the models, which Accounts holds.
    NoModel,
    /// A model answered and its answer was refused, or the text was not one the call takes.
    Other,
}

/// Why, from the notice's TEXT: the only cause it names apart is coordd's "no model answered" (clean.rs `clean_one`).
pub fn unclean_of(text: &str) -> Unclean {
    if text.contains("no model answered") { Unclean::NoModel } else { Unclean::Other }
}

/// A doubt of the cleaning: the words as they stand, and what they could be meant as.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct Doubt {
    pub typed: String,
    #[serde(default)]
    pub would: Vec<String>,
}

/// What a words ticket wants now, read from its view as the window decodes it.
#[derive(Debug, PartialEq)]
pub enum Due {
    /// The box's judge, on TEXT.
    Judge(String),
    /// His look: what he typed, the cleaned words, the doubts, and when it goes on by itself.
    Look { text: String, cleaned: String, doubts: Vec<Doubt>, lapses_at: Option<String> },
    /// Ended: these words go.
    Send(String),
    /// Ended: nothing goes; why.
    Back(String),
    /// The watcher's or the policy's turn.
    Wait,
}

pub fn due(v: &TicketView) -> Due {
    let s = |x: &Value| x.as_str().unwrap_or_default().to_string();
    match &v.status {
        ViewStatus::Awaits { agent, call, args, .. } if agent == "box" && call == "judge" => Due::Judge(s(&args["text"])),
        ViewStatus::Awaits { agent, call, args, .. } if agent == "owner" && call == "look" => Due::Look {
            text: s(&args["text"]),
            cleaned: s(&args["cleaned"]),
            doubts: serde_json::from_value(args["doubts"].clone()).unwrap_or_default(),
            lapses_at: v.lapses_at.clone(),
        },
        ViewStatus::Done { result } => match result["head"].as_str() {
            Some("Send") => Due::Send(s(&result["text"])),
            Some("Back") => Due::Back(s(&result["why"])),
            _ => Due::Back(format!("the words ticket ended as {result}")),
        },
        _ => Due::Wait,
    }
}

/// The words of REQ as they would go.
pub fn text_of(req: &Request) -> Option<&str> {
    match req {
        Request::Say { text, .. } | Request::Post { text, .. } => Some(text),
        Request::Msg { body, .. } => Some(body),
        _ => None,
    }
}

/// REQ with TEXT in place of its words; a post or a say names the words ticket T they were cleaned in (`typed`).
pub fn with_text(req: &Request, words: &str, t: Option<&str>) -> Request {
    let mut req = req.clone();
    match &mut req {
        Request::Say { text, typed, .. } | Request::Post { text, typed, .. } => {
            *text = words.to_string();
            *typed = t.map(String::from);
        }
        Request::Msg { body, .. } => *body = words.to_string(),
        _ => {}
    }
    req
}

/// W's message sent, as REQ; whether it went (else his words are back in the box).
fn go(s: &mut App, w: Words, req: Request) -> bool {
    let (pi, place) = w.key.clone();
    let what = match &w.then {
        Then::Mailed { typed, comments } => Pending::Mailed { key: w.key.clone(), typed: typed.clone(), comments: comments.clone() },
        // his message's bubble takes the answer (outgoing.rs); the box keeps what he typed since
        Then::Box if w.out.is_some() => Pending::TicketDone { clear: crate::threads::Cleared::Out(pi, place.clone()) },
        // its answer under the box, unless he has begun new words there (they are not his sent ones to clear)
        Then::Box if s.composers.get(&w.key).is_none_or(|e| e.text.trim().is_empty()) => Pending::TicketDone { clear: crate::threads::Cleared::Box(pi, place.clone()) },
        Then::Box => Pending::Sent,
    };
    if !s.request_in(pi, req.clone(), what) {
        back(s, w, "not sent: the box is not connected".into());
        return false;
    }
    if let Some(out) = w.out {
        crate::outgoing::flying(s, out, &req, w.t.clone());
    }
    true
}

/// Whether his message OUT is in its cleaning here.
pub fn holds(s: &App, out: u64) -> bool {
    s.words.pending.iter().any(|w| w.out == Some(out))
}

/// His message OUT taken back while it is in its cleaning (outgoing.rs `take_back`): its words back in its box, as
/// `back` puts them, with nothing said; the words ticket's answer then finds nothing to send. While its ticket's open
/// is out it stays, since that answer is matched to the oldest message of its box asked (`replied`), which takes it
/// back when it comes. Whether it was taken back now.
pub fn taken_back(s: &mut App, out: u64) -> bool {
    let Some(at) = s.words.pending.iter().position(|w| w.out == Some(out)) else { return false };
    if s.words.pending[at].asked && s.words.pending[at].t.is_none() {
        return false;
    }
    let w = s.words.pending.remove(at);
    let key = w.key.clone();
    back(s, w, String::new());
    s.box_said.remove(&key);
    true
}

/// W's words back in its box, with WHY under it; before what he has begun there since, never in place of it. Comments
/// it carried from a window before Add comment sent at once (N-1396 p329) come back as his words too, quoted first,
/// as they would have gone.
fn back(s: &mut App, w: Words, why: String) {
    if let Some(out) = w.out {
        crate::outgoing::forget(s, out);
    }
    let written = if w.comments.is_empty() { w.written.clone() } else { crate::ask::compose(&w.comments, &w.written).0 };
    crate::outgoing::before(s, w.key.clone(), &written);
    s.box_said.insert(w.key, Err(format!("not sent: {why}")));
}

/// The words ticket T of project PI.
fn view<'a>(s: &'a App, pi: usize, t: &str) -> Option<&'a TicketView> {
    s.projects.get(pi).and_then(|p| p.live.tickets.values().find(|v| v.id == t))
}

/// Each poll: the box's judge started or answered, a ticket that ended acted on, in the order of each box's messages.
/// Whether anything changed. Nothing goes from a window that has handed over to a new one (restart.rs): the new one
/// read what is in cleaning from ui.json and sends it.
pub fn step(s: &mut App) -> bool {
    if crate::restart::handed_over(s) {
        return false;
    }
    let mut changed = claim(s);
    let mut i = 0;
    while i < s.words.pending.len() {
        if s.words.pending[i].ready.is_some() {
            i += 1;
            continue;
        }
        let (pi, t) = (s.words.pending[i].key.0, s.words.pending[i].t.clone());
        let Some(t) = t else {
            i += 1;
            continue;
        };
        let Some(d) = view(s, pi, &t).map(due) else {
            i += 1;
            continue;
        };
        match d {
            Due::Judge(text) => {
                let w = &mut s.words.pending[i];
                if w.judging.is_none() && !w.judged {
                    let mut keep: HashSet<String> = crate::spell::names(s, pi);
                    keep.extend(s.spell.added());
                    let leave = crate::spell::code(&text);
                    let (tx, rx) = mpsc::channel();
                    let wake = s.wake.clone();
                    // Harper on its own thread (its rules are built per thread): never in a frame
                    let _ = std::thread::Builder::new().name("words-judge".into()).spawn(move || {
                        let _ = tx.send(crate::marks::judge(&text, &leave, &keep));
                        if let Some(w) = wake {
                            w();
                        }
                    });
                    s.words.pending[i].judging = Some(rx);
                }
                let got = s.words.pending[i].judging.as_ref().and_then(|r| r.try_recv().ok());
                if let Some((fixed, per100)) = got {
                    let w = &mut s.words.pending[i];
                    (w.judging, w.judged) = (None, true);
                    let head = json!({"head": "Judged", "fixed": fixed, "per100": per100});
                    s.request_in(pi, Request::Answer { t: t.clone(), head, text: None }, Pending::Sent);
                    changed = true;
                }
                i += 1;
            }
            Due::Send(words) => {
                // the cleaning call failed: coordd sends his words as they came and says why on the ticket
                // (clerk.rs `clean`)
                let unclean = s.projects.get(pi).and_then(|p| p.live.notices.iter().find(|n| n.ticket.as_deref() == Some(t.as_str()))).map(|n| unclean_of(&n.text));
                let w = &mut s.words.pending[i];
                w.ready = Some(Ready::Send(with_text(&w.req, &words, Some(&t)), unclean));
                changed = true;
                i += 1;
            }
            Due::Back(why) => {
                s.words.pending[i].ready = Some(Ready::Back(why));
                changed = true;
                i += 1;
            }
            Due::Look { .. } | Due::Wait => i += 1,
        }
    }
    release(s) || changed
}

/// The messages whose tickets ended, each once every earlier message of its box went or came back: a later one never
/// overtakes one still in cleaning (N-815). Whether any went.
fn release(s: &mut App) -> bool {
    let mut any = false;
    let mut i = 0;
    while i < s.words.pending.len() {
        let first = !s.words.pending[..i].iter().any(|w| w.key == s.words.pending[i].key);
        if !first || s.words.pending[i].ready.is_none() {
            i += 1;
            continue;
        }
        // one read back from ui.json: once the snapshot has its ticket, and not when a post already came of it (sent
        // before the window ended, after ui.json was last written): it goes once
        let w = &s.words.pending[i];
        if !w.asked
            && let Some(t) = w.t.as_deref()
        {
            if view(s, w.key.0, t).is_none() {
                i += 1;
                continue;
            }
            if posted(s, w.key.0, t) {
                crate::log::line(&format!("words: {t} was posted before this window started; not sent again"));
                if let Some(out) = s.words.pending.remove(i).out {
                    crate::outgoing::forget(s, out);
                }
                any = true;
                continue;
            }
        }
        let mut w = s.words.pending.remove(i);
        match w.ready.take() {
            Some(Ready::Send(req, unclean)) => {
                let key = w.key.clone();
                if go(s, w, req)
                    && let Some(why) = unclean
                {
                    s.words.unclean.insert(key, why);
                }
            }
            Some(Ready::Back(why)) => back(s, w, why),
            None => {}
        }
        any = true;
    }
    any
}

/// Whether a post of project PI came of words ticket T (its `cleaned_from`).
fn posted(s: &App, pi: usize, t: &str) -> bool {
    s.projects.get(pi).is_some_and(|p| p.live.tickets.values().any(|v| v.posts.iter().any(|q| q.cleaned_from.as_deref() == Some(t))))
}

/// A message put back from ui.json whose words ticket this run did not open: its ticket found among the snapshot's,
/// the newest words ticket of this window with its text that no other message holds. Whether one was found.
fn claim(s: &mut App) -> bool {
    let mut found = false;
    for i in 0..s.words.pending.len() {
        let w = &s.words.pending[i];
        if w.asked || w.t.is_some() {
            continue;
        }
        let (pi, text) = (w.key.0, text_of(&w.req).unwrap_or_default().to_string());
        let held: HashSet<&str> = s.words.pending.iter().filter_map(|w| w.t.as_deref()).collect();
        let window = crate::state::box_id();
        let Some(p) = s.projects.get(pi) else { continue };
        let t = p
            .live
            .tickets
            .values()
            .filter(|v| v.kind == "words" && v.form["window"].as_str() == Some(window.as_str()) && v.form["text"].as_str() == Some(text.as_str()) && !held.contains(v.id.as_str()))
            .max_by_key(|v| v.n)
            .map(|v| v.id.clone());
        if let Some(t) = t {
            s.words.pending[i].t = Some(t);
            found = true;
        }
    }
    found
}

/// What is in cleaning, for ui.json: each message with its project's id (state.rs).
pub fn kept(s: &App) -> Vec<(String, &Words)> {
    s.words.pending.iter().filter_map(|w| Some((s.projects.get(w.key.0)?.id.clone(), w))).collect()
}

/// What was in cleaning when ui.json was written, put back: each message in its box by its project's id (an unknown
/// project's are left out), in place of what this run holds (a new window holds none of its own yet).
pub fn put_back(s: &mut App, kept: Vec<(String, Words)>) {
    s.words.pending = kept
        .into_iter()
        .filter_map(|(id, mut w)| {
            w.key.0 = s.projects.iter().position(|p| p.id == id)?;
            Some(w)
        })
        .collect();
}

/// Whether a message is being cleaned (the window wakes each second for its line and its time).
pub fn any(s: &App) -> bool {
    !s.words.pending.is_empty()
}

/// The cleaned words with his picks put in place of their doubts.
pub fn picked_text(cleaned: &str, doubts: &[Doubt], picked: &[Option<String>]) -> String {
    let mut out = cleaned.to_string();
    for (d, p) in doubts.iter().zip(picked) {
        if let Some(p) = p
            && let Some(at) = out.find(&d.typed)
        {
            out.replace_range(at..at + d.typed.len(), p);
        }
    }
    out
}

/// "goes as shown in 24 s", from LAPSES_AT (RFC 3339) and NOW.
fn time_left(lapses_at: &str, now: chrono::DateTime<chrono::Utc>) -> Option<String> {
    let at = chrono::DateTime::parse_from_rfc3339(lapses_at).ok()?.with_timezone(&chrono::Utc);
    let left = (at - now).num_seconds().max(0);
    Some(format!("goes as shown in {left} s"))
}

/// The message of box KEY shown under it: the first at his look, else the oldest still in cleaning.
fn shown_at(s: &App, key: &(usize, String)) -> Option<usize> {
    let mine = || s.words.pending.iter().enumerate().filter(|(_, w)| &w.key == key && w.ready.is_none());
    mine()
        .find(|(_, w)| w.t.as_deref().and_then(|t| view(s, key.0, t)).is_some_and(|v| matches!(due(v), Due::Look { .. })))
        .or_else(|| mine().next())
        .map(|(i, _)| i)
}

/// His answer to the look of W: Go, Use(text) or AsTyped.
fn answer(s: &mut App, key: &(usize, String), head: Value) {
    let Some(t) = shown_at(s, key).and_then(|i| s.words.pending[i].t.clone()) else { return };
    s.request_in(key.0, Request::Answer { t, head, text: None }, Pending::Sent);
}

/// Where the message from the box of (PI, PLACE) will land, while it is being cleaned: after SHOWN_AFTER his words,
/// readable, as "being cleaned"; at his look, the cleaned words with the doubts as suggestions. None otherwise.
pub fn shown(s: &App, pi: usize, place: &str, k: &Kit) -> Option<El> {
    let key = (pi, place.to_string());
    let w = &s.words.pending[shown_at(s, &key)?];
    let id = format!("words-{pi}-{place}");
    let look = w.t.as_deref().and_then(|t| view(s, pi, t)).map(due);
    if let Some(Due::Look { cleaned, doubts, lapses_at, .. }) = look {
        let now = picked_text(&cleaned, &doubts, &w.picked);
        let mut head = row().w_full().justify_between().center_y().child(text("Cleaned: send these words?").font(k.ty.small).color(k.c.text_accent));
        if let Some(left) = lapses_at.as_deref().and_then(|a| time_left(a, chrono::Utc::now())) {
            head = head.child(text(left).font(k.ty.small).color(k.c.text_faint));
        }
        let mut c = col().key(id).w_full().gap(6.0).padding(8.0).radius(k.radius.md).fill(k.c.layer_02).child(head).child(text(now.clone()).font(k.ty.body).color(k.c.text_base).wrap());
        for (di, d) in doubts.iter().enumerate() {
            let mut r = row().key(format!("words-doubt-{pi}-{place}-{di}")).gap(4.0).center_y().child(text(format!("{}:", d.typed)).font(k.ty.small).color(k.c.text_muted).pr(4.0));
            for o in &d.would {
                let on = w.picked.get(di).and_then(|p| p.as_deref()) == Some(o.as_str());
                let (key, o2) = (key.clone(), o.clone());
                r = r.child(Button::new(o.clone()).variant(if on { ButtonVariant::Neutral } else { ButtonVariant::Ghost }).on_press(move |s: &mut App| pick(s, &key, di, Some(o2.clone()))).build(k));
            }
            c = c.child(r);
        }
        let (k1, k2) = (key.clone(), key.clone());
        let mine = w.picked.iter().any(Option::is_some).then_some(now);
        // not a second "Send" beside the box's own: these words (N-466, say a thing once)
        let send = Button::new("Send these words").variant(ButtonVariant::Contrast).on_press(move |s: &mut App| {
            let head = match &mine {
                Some(t) => json!({"head": "Use", "text": t}),
                None => json!({"head": "Go"}),
            };
            answer(s, &k1, head);
        });
        let typed = Button::new("As I typed it").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| answer(s, &k2, json!({"head": "AsTyped"})));
        return Some(c.child(row().w_full().justify_end().gap(8.0).child(typed.build(k)).child(send.build(k))));
    }
    // his message's bubble says it is on its way (outgoing.rs); this card only for one kept by an older window
    if w.out.is_some() || w.since.elapsed() < SHOWN_AFTER {
        return None;
    }
    Some(
        col()
            .key(id)
            .w_full()
            .gap(4.0)
            .padding(8.0)
            .radius(k.radius.md)
            .fill(k.c.layer_01)
            .child(text("being cleaned").font(k.ty.small).color(k.c.text_faint))
            .child(text(w.written.clone()).font(k.ty.body).color(k.c.text_muted).wrap()),
    )
}

/// Whether his message OUT (outgoing.rs) is at his look of its cleaning: the look stands in its bubble's place.
pub fn at_look(s: &App, out: u64) -> bool {
    s.words.pending.iter().any(|w| w.out == Some(out) && w.ready.is_none() && w.t.as_deref().and_then(|t| view(s, w.key.0, t)).is_some_and(|v| matches!(due(v), Due::Look { .. })))
}

/// His pick for doubt DI of the look in the box KEY; the same word again leaves it as typed.
fn pick(s: &mut App, key: &(usize, String), di: usize, word: Option<String>) {
    let Some(at) = shown_at(s, key) else { return };
    let w = &mut s.words.pending[at];
    if w.picked.len() <= di {
        w.picked.resize(di + 1, None);
    }
    w.picked[di] = if w.picked[di] == word { None } else { word };
}

/// What he typed for a post of his cleaned words: the words ticket's form `typed` (his typing before the box changed
/// words of it) or else its `text`, when the post names one.
pub fn typed_of<'a>(p: &'a crate::Proj, post: &docket_machine::Post) -> Option<&'a str> {
    let w = post.cleaned_from.as_deref()?;
    let form = &p.live.tickets.values().find(|v| v.id == w)?.form;
    form["typed"].as_str().or(form["text"].as_str())
}

/// Under a post that is his words cleaned: "as typed", behind one click, said once.
pub fn as_typed(s: &App, p: &crate::Proj, v: &TicketView, post: &docket_machine::Post, k: &Kit) -> Option<El> {
    let typed = typed_of(p, post)?.to_string();
    let at = (v.id.clone(), post.n);
    let open = s.words.open.contains(&at);
    let flip = at.clone();
    let mut c = col().key(format!("as-typed-{}-{}", v.id, post.n)).w_full().gap(2.0).child(
        Button::new(if open { "as typed ▴" } else { "as typed ▾" })
            .variant(ButtonVariant::Ghost)
            .on_press(move |s: &mut App| {
                if !s.words.open.remove(&flip) {
                    s.words.open.insert(flip.clone());
                }
            })
            .build(k),
    );
    if open {
        c = c.child(text(typed).font(k.ty.small).color(k.c.text_muted).wrap());
    }
    Some(c)
}

/// For stills: a message from the box KEY being cleaned in words ticket T (none: not opened yet), sent SINCE ago.
#[cfg(test)]
pub fn for_still(key: (usize, String), text: &str, t: Option<&str>, since: Duration) -> Words {
    let req = Request::Post { kind: None, t: "N-450".into(), text: text.into(), to: None, answers: None, wait: false, also: Vec::new(), typed: None, split_from: None, on: Vec::new(), comments: Vec::new() };
    Words { key, req, then: Then::Box, written: text.into(), comments: Vec::new(), t: t.map(String::from), asked: true, since: Instant::now() - since, judging: None, judged: true, picked: Vec::new(), ready: None, out: None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coord_types::{Body, Event, Snapshot};
    use docket_machine::{Defs, Input, Machine, Text, WATCHER};

    const CALLS: &str = "type Doubt {\n  typed: Text\n  would: [Text]\n}\ncall judge(text: Text) -> Judged(fixed: Text, per100: Int)\n  task judge\ncall clean(text: Text, keep: [Text]) -> Sure(cleaned: Text) | Borderline(cleaned: Text, doubts: [Doubt], per100: Int) | Unreadable(why: Text)\n  task clean\ncall look(text: Text, cleaned: Text, doubts: [Doubt]) -> Go | Use(text: Text) | AsTyped\n  task look\n  default Go\ndecision worth-cleaning(per100: Int) -> Clean | Send\ndecision how-many(doubts: [Doubt], per100: Int) -> Some | TooMany\n";
    const KINDS: &str = "kind words\n  form text: Text, window: Text, keep: [Text]\n  result Send(text: Text) | Back(why: Text)\n  pattern\n    j <- step box judge(text);\n    match j {\n      Judged(fixed, per100) =>\n        w <- decide worth-cleaning(per100);\n        match w {\n          Send => return Send(fixed)\n          Clean =>\n            c <- step watcher clean(fixed, keep);\n            match c {\n              Sure(cleaned) => return Send(cleaned)\n              Borderline(cleaned, doubts, per100d) =>\n                v <- decide how-many(doubts, per100d);\n                match v {\n                  Some =>\n                    o <- step owner look(text, cleaned, doubts);\n                    match o {\n                      Go => return Send(cleaned)\n                      Use(mine) => return Send(mine)\n                      AsTyped => return Send(text)\n                    }\n                  TooMany => return Back(\"too many words I could not be sure of\")\n                }\n              Unreadable(why) => return Back(why)\n            }\n        }\n    }\n";

    struct Run {
        m: Machine,
        clock: u32,
    }

    impl Run {
        fn new() -> Run {
            let mut named = Default::default();
            let calls = docket_machine::syntax::parse_calls("words.call", CALLS, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
            let (defs, errors) = Defs::build(&[("words.kind".to_string(), KINDS.to_string())], calls, named, 200);
            assert!(errors.is_empty(), "{errors:?}");
            Run { m: Machine::load(defs), clock: 0 }
        }
        fn go(&mut self, i: Input) -> String {
            self.clock += 1;
            let out = self.m.apply(i, format!("2026-11-18T16:00:{:02}Z", self.clock));
            out.reply.unwrap_or_else(|e| panic!("{e}"))
        }
        fn answer(&mut self, t: u64, by: &str, head: Value) {
            self.go(Input::Answer { t, by: by.into(), head, text: Text::New(String::new()) });
        }
        fn decide(&mut self, t: u64, answer: &str) {
            let step = self.m.tickets[&t].steps;
            self.go(Input::Decided { t, step, question: "decide".into(), answer: json!(answer), provenance: json!({"test": true}) });
        }
        /// Ticket T's view as the window has it: through a snapshot event as the bridge writes it (the body
        /// flattened), read back by coord_types' own Event (the N-541 lesson), with coordd's LAPSES_AT.
        fn seen(&self, t: u64, lapses_at: Option<&str>) -> TicketView {
            let mut v = self.m.view(t).unwrap();
            v.lapses_at = lapses_at.map(String::from);
            let line = serde_json::to_string(&Event { seq: 1, body: Body::Snapshot(Snapshot { tickets: vec![v], ..Default::default() }) }).unwrap();
            let back: Event = serde_json::from_str(&line).unwrap_or_else(|e| panic!("the window cannot read it: {e}\n{line}"));
            let Body::Snapshot(s) = back.body else { panic!() };
            s.tickets.into_iter().next().unwrap()
        }
    }

    /// Each field the window reads of a words ticket, decoded through a flattened Event: the box's judge and its
    /// text, his look (text, cleaned, doubts, lapses_at), the result's text, Back's why; and a post's cleaned_from with
    /// the ticket's form text behind it.
    #[test]
    fn a_words_ticket_reads_in_the_window_at_each_step() {
        let mut r = Run::new();
        r.go(Input::Open { kind: "words".into(), under: None, opener: "owner".into(), form: json!({"text": "teh woth", "window": "w-1", "keep": ["yaaihh"]}), text: None, names: vec![], told: vec![] });
        let t = 200;
        assert_eq!(due(&r.seen(t, None)), Due::Judge("teh woth".into()));
        r.answer(t, "box", json!({"head": "Judged", "fixed": "the woth", "per100": 50}));
        assert_eq!(due(&r.seen(t, None)), Due::Wait, "the policy's turn");
        r.decide(t, "Clean");
        r.answer(t, WATCHER, json!({"head": "Borderline", "cleaned": "the woth", "doubts": [{"typed": "woth", "would": ["worth", "with"]}], "per100": 9}));
        r.decide(t, "Some");
        let look = due(&r.seen(t, Some("2026-11-18T16:00:40Z")));
        let doubts = vec![Doubt { typed: "woth".into(), would: vec!["worth".into(), "with".into()] }];
        assert_eq!(look, Due::Look { text: "teh woth".into(), cleaned: "the woth".into(), doubts: doubts.clone(), lapses_at: Some("2026-11-18T16:00:40Z".into()) });
        assert_eq!(picked_text("the woth", &doubts, &[Some("worth".into())]), "the worth");
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-18T16:00:16Z").unwrap().with_timezone(&chrono::Utc);
        assert_eq!(time_left("2026-11-18T16:00:40Z", now).as_deref(), Some("goes as shown in 24 s"));
        r.answer(t, "owner", json!({"head": "Use", "text": "the worth"}));
        assert_eq!(due(&r.seen(t, None)), Due::Send("the worth".into()));
        // the post of the cleaned words names it, and what he typed is one click under it
        r.go(Input::Open { kind: "words".into(), under: None, opener: "owner".into(), form: json!({"text": "x", "window": "w-1", "keep": []}), text: None, names: vec![], told: vec![] });
        let on = 201;
        r.go(Input::Post { t: on, by: "owner".into(), text: Text::New("the worth".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None });
        let n = r.m.tickets[&on].last();
        r.go(Input::Cleaned { t: on, n, words: t });
        let (words, post) = (r.seen(t, None), r.seen(on, None));
        let mut p = crate::view_cost::project_for_tests();
        p.live.tickets.insert(t, words);
        let post = post.posts.iter().find(|q| q.n == n).unwrap().clone();
        assert_eq!(post.cleaned_from.as_deref(), Some(r.m.id(t).as_str()));
        assert_eq!(typed_of(&p, &post), Some("teh woth"), "as typed: the words ticket's form text");
    }

    /// A words ticket opened by this window for TEXT and taken as far as his look (`look`) or straight to its Send.
    fn ticket(r: &mut Run, t: u64, text: &str, look: bool) {
        let form = json!({"text": text, "window": crate::state::box_id(), "keep": []});
        r.go(Input::Open { kind: "words".into(), under: None, opener: "owner".into(), form, text: None, names: vec![], told: vec![] });
        r.answer(t, "box", json!({"head": "Judged", "fixed": text, "per100": if look { 50 } else { 0 }}));
        if !look {
            r.decide(t, "Send");
            return;
        }
        r.decide(t, "Clean");
        r.answer(t, WATCHER, json!({"head": "Borderline", "cleaned": text, "doubts": [{"typed": "x", "would": ["y"]}], "per100": 9}));
        r.decide(t, "Some");
    }

    /// The window of these tests: one project at the cleaning's protocol, spelling on, its requests written to OUT.
    fn wired(out: &std::path::Path) -> (std::sync::MutexGuard<'static, ()>, App, std::process::Child) {
        let (held, mut s) = crate::view_cost::test_app();
        s.projects = vec![crate::view_cost::project_for_tests()];
        s.projects[0].protocol = coord_types::PROTOCOL;
        s.spell.on = true;
        let child = wire(&mut s, out);
        (held, s, child)
    }

    fn wire(s: &mut App, out: &std::path::Path) -> std::process::Child {
        let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        child
    }

    /// The posts the window sent, in order: their texts.
    fn posted_texts(s: &mut App, child: std::process::Child, out: &std::path::Path) -> Vec<String> {
        s.projects[0].link = None;
        let mut child = child;
        let _ = child.wait();
        std::fs::read_to_string(out).unwrap().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["req"]["op"] == "post").map(|v| v["req"]["text"].as_str().unwrap_or_default().to_string()).collect()
    }

    /// A message of box KEY in cleaning in words ticket ID, as a window from before 0.2 left it (its ticket opened by
    /// that run).
    fn send(s: &mut App, key: &(usize, String), text: &str, id: &str) {
        s.words.pending.push(for_still(key.clone(), text, Some(id), Duration::ZERO));
    }

    fn seen_in(s: &mut App, r: &Run, t: u64) {
        let v = r.seen(t, None);
        s.projects[0].live.tickets.insert(v.n, v);
    }

    /// The owner, N-815: "never drop it". Two messages from one box, the first at his look when the second is sent
    /// and decided at once: both are posted, first then second; the second waits for the first.
    #[test]
    fn a_second_message_from_a_box_drops_none_and_goes_after_the_first() {
        let out = std::env::temp_dir().join(format!("yaaihh-words-two-{}", std::process::id()));
        let (_held, mut s, child) = wired(&out);
        let key = (0usize, "thread:N-450".to_string());
        let mut r = Run::new();
        ticket(&mut r, 200, "the first message", true);
        ticket(&mut r, 201, "the second message", false);
        send(&mut s, &key, "the first message", &r.m.id(200));
        send(&mut s, &key, "the second message", &r.m.id(201));
        seen_in(&mut s, &r, 200);
        seen_in(&mut s, &r, 201);
        assert_eq!(due(&s.projects[0].live.tickets[&201]), Due::Send("the second message".into()));
        step(&mut s);
        assert_eq!(s.words.pending.len(), 2, "both kept: the second waits for the first, still at his look");
        r.answer(200, "owner", json!({"head": "Go"}));
        seen_in(&mut s, &r, 200);
        step(&mut s);
        assert!(s.words.pending.is_empty(), "both went");
        assert_eq!(posted_texts(&mut s, child, &out), ["the first message", "the second message"], "first then second");
    }

    /// A swap (or a close) while a message is in cleaning: the new window reads it from ui.json and posts it when its
    /// ticket ends; one whose ticket's id had not come back yet finds its ticket by the window and the text.
    #[test]
    fn a_message_in_cleaning_goes_after_a_swap() {
        let out = std::env::temp_dir().join(format!("yaaihh-words-swap-{}", std::process::id()));
        let (_held, mut s, child) = wired(&out);
        let key = (0usize, "thread:N-450".to_string());
        let mut r = Run::new();
        ticket(&mut r, 200, "said before the swap", true);
        ticket(&mut r, 201, "opened as it swapped", true);
        send(&mut s, &key, "said before the swap", &r.m.id(200));
        // one whose ticket's id had not come back when the window ended
        s.words.pending.push(for_still(key.clone(), "opened as it swapped", None, Duration::ZERO));
        // ui.json in a folder of this test's own (state.rs's test points YAAIHH_CONFIG at one it removes after)
        let dir = std::env::temp_dir().join(format!("yaaihh-words-swap-dir-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // SAFETY: under the tests' lock (`wired` holds it), which every test that reads or sets it holds
        unsafe { std::env::set_var("YAAIHH_CONFIG", dir.join("projects.toml")) };
        assert!(crate::state::write_now(&mut s), "ui.json written, as before a swap");
        assert_eq!(posted_texts(&mut s, child, &out), Vec::<String>::new());
        // the new window
        let mut n = crate::init();
        n.projects = vec![crate::view_cost::project_for_tests()];
        n.projects[0].protocol = coord_types::PROTOCOL;
        n.spell.on = true;
        crate::state::restore(&mut n);
        assert_eq!(n.words.pending.len(), 2, "both read back");
        let child = wire(&mut n, &out);
        r.answer(200, "owner", json!({"head": "Go"}));
        r.answer(201, "owner", json!({"head": "AsTyped"}));
        seen_in(&mut n, &r, 200);
        seen_in(&mut n, &r, 201);
        step(&mut n);
        assert!(n.words.pending.is_empty(), "both went: {:?}", n.words.pending.iter().map(|w| &w.t).collect::<Vec<_>>());
        assert_eq!(posted_texts(&mut n, child, &out), ["said before the swap", "opened as it swapped"]);
        // the window ended before ui.json was written again: the next one reads both once more, sees the posts that
        // came of them, and sends neither again
        let on = 202;
        // a ticket to post on (this machine knows only `words`, as the test above posts on one)
        r.go(Input::Open { kind: "words".into(), under: None, opener: "owner".into(), form: json!({"text": "x", "window": "w-0", "keep": []}), text: None, names: vec![], told: vec![] });
        for w in [200, 201] {
            r.go(Input::Post { t: on, by: "owner".into(), text: Text::New("y".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None });
            let n = r.m.tickets[&on].last();
            r.go(Input::Cleaned { t: on, n, words: w });
        }
        let mut again = crate::init();
        again.projects = vec![crate::view_cost::project_for_tests()];
        again.projects[0].protocol = coord_types::PROTOCOL;
        crate::state::restore(&mut again);
        assert_eq!(again.words.pending.len(), 2);
        let child = wire(&mut again, &out);
        for t in [200, 201, on] {
            seen_in(&mut again, &r, t);
        }
        step(&mut again);
        assert!(again.words.pending.is_empty());
        assert_eq!(posted_texts(&mut again, child, &out), Vec::<String>::new(), "each went once");
    }

    #[test]
    fn too_many_doubts_come_back_with_why() {
        let mut r = Run::new();
        r.go(Input::Open { kind: "words".into(), under: None, opener: "owner".into(), form: json!({"text": "x", "window": "w-1", "keep": []}), text: None, names: vec![], told: vec![] });
        r.answer(200, "box", json!({"head": "Judged", "fixed": "x", "per100": 90}));
        r.decide(200, "Clean");
        r.answer(200, WATCHER, json!({"head": "Borderline", "cleaned": "x", "doubts": [{"typed": "x", "would": ["y"]}], "per100": 100}));
        r.decide(200, "TooMany");
        assert_eq!(due(&r.seen(200, None)), Due::Back("too many words I could not be sure of".into()));
    }

    #[test]
    fn the_message_goes_with_the_words_and_a_post_names_its_ticket() {
        let say = Request::Say { to: Some("dev".into()), names: vec![], text: "teh".into(), follows: None, wait: false, under: None, label: None, thread: false, typed: None, split_from: None, comments: Vec::new() };
        assert!(matches!(with_text(&say, "the", Some("N-9")), Request::Say { text, typed: Some(t), .. } if text == "the" && t == "N-9"));
        let post = Request::Post { kind: None, t: "N-1".into(), text: "teh".into(), to: None, answers: None, wait: false, also: vec![], typed: None, split_from: None, on: Vec::new(), comments: Vec::new() };
        assert!(matches!(with_text(&post, "the", Some("N-9")), Request::Post { text, typed: Some(w), .. } if text == "the" && w == "N-9"));
    }
}
