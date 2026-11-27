//! His messages on their way: each his bubble from the press until the engine's copy is in the snapshot,
//! marked sending…, sent or not sent · why, with Take back; the copy is matched by id, never by text.

use crate::{App, Proj};
use coord_types::{Request, TicketView};
use rune::{El, TextEdit, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit};

/// Where a message of his stands.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Is {
    /// Pressed: on its way to the engine, with no answer yet.
    Sending,
    /// The engine has it; its copy is not in the snapshot yet.
    Sent,
    /// Not sent, and why.
    NotSent(String),
}

/// What its answer goes back to: a box (its reply under the box's own line) or an agent's chat (a draft kept).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Then {
    Box,
    Chat { typed: String, comments: Vec<crate::ask::Comment> },
}

/// One message of his on its way: its box (KEY), the request that sends it, the words as he typed them, when he
/// pressed (RFC 3339).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Out {
    pub id: u64,
    pub key: (usize, String),
    pub req: Request,
    pub text: String,
    pub at: String,
    pub is: Is,
    pub then: Then,
    /// Its request is with the engine (not in cleaning, not refused).
    pub flying: bool,
    /// The post the engine's answer named: (ticket id, post).
    pub post: Option<(String, u32)>,
    /// The words ticket it was cleaned in: its copy's `cleaned_from`.
    pub words: Option<String>,
    /// He pressed `Take back`; it is taken back once there is a post to take back (or none is ever made).
    #[serde(default)]
    pub take_back: bool,
    /// Its post's take back is with the engine.
    #[serde(default)]
    pub taking: bool,
    /// The last post of its ticket in the snapshot at the press: its copy is a later post.
    #[serde(default)]
    pub after: u32,
    /// Refused before it left the window (no pipe): sent by itself, once, when its pipe is back.
    #[serde(default)]
    pub unsent: bool,
    /// A comment on a passage, sent from the comment box (ask.rs `send`): its passage and his note, so its words go
    /// back into that box at the passage (`words_back`), and its answer leaves the box of its place as he left it.
    #[serde(default)]
    pub comment: Option<crate::ask::Comment>,
    /// The comments held for its box that went in it, each quoted before his words (ask.rs `Notes::held`): taken back,
    /// they are held for that box again and his words go back into it (`words_back`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub held: Vec<crate::ask::Comment>,
}

#[derive(Default)]
pub struct State {
    pub list: Vec<Out>,
    /// The dock's Send now pressed on his post (project, `N-5.p1`) waiting on AGENT's turn: its second stage,
    /// Interrupt & Send (Esc), stands in its place ([`dock`]).
    pub armed: Option<(usize, String, String)>,
}

/// A message of his pressed in box KEY: kept as `sending…` from now on; its id.
pub fn new(s: &mut App, key: (usize, String), req: &Request, typed: &str, then: Then) -> u64 {
    let id = s.outgoing.list.iter().map(|o| o.id).max().unwrap_or(0) + 1;
    let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let after = match req {
        Request::Post { t, .. } => s.projects.get(key.0).and_then(|p| p.live.tickets.values().find(|v| v.id == *t)).and_then(|v| v.posts.iter().map(|x| x.n).max()).unwrap_or(0),
        _ => 0,
    };
    s.outgoing.list.push(Out { id, key, req: req.clone(), text: typed.trim().to_string(), at, is: Is::Sending, then, flying: false, post: None, words: None, take_back: false, taking: false, after, unsent: false, comment: None, held: Vec::new() });
    id
}

/// His press of a box's Send, the one way every box sends a message of his (message_box.rs `sent`, an agent's chat's
/// `App::send`, the comment box's Add comment, ask.rs `send`): REQ from box KEY, TYPED as he wrote it, its answer going
/// to THEN, or to COMMENT when it is a comment on a passage. It stands above the box of KEY as `sending…` from now on
/// and its request goes; its id, and why it could not go when the pipe would not take it (the caller keeps it `not
/// sent`, or forgets it where no message of his stands).
pub fn press(s: &mut App, key: (usize, String), req: &Request, typed: &str, then: Then, comment: Option<crate::ask::Comment>) -> (u64, Option<String>) {
    let id = new(s, key.clone(), req, typed, then);
    let o = s.outgoing.list.last_mut().expect("`new` keeps it last");
    o.comment = comment;
    let what = pending(o);
    if s.request_in(key.0, req.clone(), what) {
        flying(s, id, req, None);
        return (id, None);
    }
    (id, Some(s.notice.take().unwrap_or_else(|| "not sent".into())))
}

/// Message ID refused before it left the window (WHY): kept as `not sent`, sent by itself once its pipe is back.
pub fn refused(s: &mut App, id: u64, why: &str) {
    if let Some(o) = s.outgoing.list.iter_mut().find(|o| o.id == id) {
        (o.flying, o.unsent, o.is) = (false, true, Is::NotSent(why.trim().to_string()));
    }
}

/// Message ID's request REQ is with the engine now (cleaned in words ticket WORDS, when it was).
pub fn flying(s: &mut App, id: u64, req: &Request, words: Option<String>) {
    if let Some(o) = s.outgoing.list.iter_mut().find(|o| o.id == id) {
        (o.req, o.flying, o.is, o.unsent) = (req.clone(), true, Is::Sending, false);
        if words.is_some() {
            o.words = words;
        }
    }
}

/// Message ID no longer on its way: its words went back to the box (the cleaning said why), or its post came before
/// this window started.
pub fn forget(s: &mut App, id: u64) {
    s.outgoing.list.retain(|o| o.id != id);
}

/// The engine's answer to the oldest message of box KEY with it: `sent` with the post it names, or `not sent` with
/// why. Whether one was waiting for it (else the answer is not a message of his on its way).
pub fn answered(s: &mut App, key: &(usize, String), ok: bool, out: &str) -> bool {
    let Some(o) = s.outgoing.list.iter_mut().find(|o| &o.key == key && o.flying && o.is == Is::Sending) else { return false };
    o.flying = false;
    let id = o.id;
    if !ok {
        o.is = Is::NotSent(out.trim().trim_start_matches("not sent: ").to_string());
        if o.take_back {
            // he took it back while it was on its way, and it did not go: his words back, nothing to take back there
            words_back(s, id);
        }
        return true;
    }
    o.post = post_named(out);
    o.is = Is::Sent;
    // posted with no number said: wait for its copy by what it is (`found`); a quote reply is not a Post
    let posted_here = matches!(&o.req, Request::Post { t, .. } if posted_on(out, t));
    if o.post.is_none() && o.words.is_none() && !posted_here {
        // taken, and no post of it to wait for (a message on a note, words that opened a thread elsewhere): its place
        // is there
        forget(s, id);
        return true;
    }
    if o.take_back {
        take_back_post(s, id);
    }
    true
}

/// The engine's answer OUT says it posted on ticket T (`Posted on N-12`, not the start of `N-123`).
pub(crate) fn posted_on(out: &str, t: &str) -> bool {
    out.trim_start().strip_prefix(&format!("Posted on {t}")).is_some_and(|r| !r.starts_with(|c: char| c.is_ascii_alphanumeric()))
}

/// The post an answer names (`N-12 as post 4`, `N-12.p4`, or a new thread's post 1); None for a quote reply's.
pub fn post_named(out: &str) -> Option<(String, u32)> {
    let words: Vec<&str> = out.split(|c: char| c.is_whitespace() || c == ';' || c == ',' || c == '(' || c == ')').filter(|w| !w.is_empty()).map(|w| w.trim_end_matches(['.', ':'])).collect();
    fn ticket(w: &str) -> bool {
        w.strip_prefix("N-").is_some_and(|r| !r.is_empty() && r.chars().all(|c| c.is_ascii_digit()))
    }
    for (i, w) in words.iter().enumerate() {
        if ticket(w)
            && words.get(i + 1) == Some(&"as")
            && words.get(i + 2) == Some(&"post")
            && let Some(n) = words.get(i + 3).and_then(|n| n.parse().ok())
        {
            return Some((w.to_string(), n));
        }
    }
    for w in &words {
        let Some(rest) = w.strip_prefix("N-") else { continue };
        if let Some((t, n)) = rest.split_once(".p")
            && !t.is_empty()
            && t.chars().all(|c| c.is_ascii_digit())
            && let Ok(n) = n.parse()
        {
            return Some((format!("N-{t}"), n));
        }
    }
    let lower = out.to_lowercase();
    if lower.contains("post") || lower.contains("closed") {
        return None;
    }
    words.iter().copied().find(|w| ticket(w)).map(|w| (w.to_string(), 1))
}

/// Project PI's pipe ended: what was with its engine and had no answer is `not sent`, Send again beside it; one he
/// took back meanwhile goes, his words back in its box.
pub fn down(s: &mut App, pi: usize) {
    let mut back = Vec::new();
    for o in s.outgoing.list.iter_mut().filter(|o| o.key.0 == pi && o.flying && o.is == Is::Sending) {
        o.flying = false;
        o.is = Is::NotSent("the engine does not answer".into());
        if o.take_back {
            back.push(o.id);
        }
    }
    for id in back {
        words_back(s, id);
    }
    // a take back with the engine when the pipe ended: asked again at the next poll with a pipe (`step`)
    for o in s.outgoing.list.iter_mut().filter(|o| o.key.0 == pi && o.taking) {
        o.taking = false;
    }
}

/// The engine's copy of O in its project's snapshot: the post its answer named, or a post cleaned from its words
/// ticket; (ticket id, post).
fn copy_of(s: &App, o: &Out) -> Option<(String, u32)> {
    let p = s.projects.get(o.key.0)?;
    let named = p.live.tickets.values().find_map(|v| {
        if let Some((t, n)) = &o.post
            && v.id == *t
            && v.posts.iter().any(|x| x.n == *n)
        {
            return Some((t.clone(), *n));
        }
        let w = o.words.as_deref()?;
        v.posts.iter().find(|x| x.by == "owner" && x.cleaned_from.as_deref() == Some(w)).map(|x| (v.id.clone(), x.n))
    });
    if named.is_some() || o.post.is_some() {
        return named;
    }
    found(s, p, o)
}

/// O's copy by what it is when no answer named it: his next post on its ticket with its words, oldest first.
fn found(s: &App, p: &crate::Proj, o: &Out) -> Option<(String, u32)> {
    let Request::Post { t, text, .. } = &o.req else { return None };
    if o.unsent {
        return None;
    }
    let v = p.live.tickets.values().find(|v| v.id == *t)?;
    let same = |x: &Out| x.key == o.key && !x.unsent && matches!(&x.req, Request::Post { t: xt, text: xx, .. } if xt == t && xx.trim() == text.trim());
    let taken: Vec<u32> = s.outgoing.list.iter().filter_map(|x| x.post.as_ref().filter(|(xt, _)| xt == t).map(|(_, n)| *n)).collect();
    let older = s.outgoing.list.iter().filter(|&x| x.id < o.id && x.post.is_none() && same(x)).count();
    v.posts
        .iter()
        .filter(|x| x.by == "owner" && x.n > o.after && !taken.contains(&x.n) && crate::threads::read(&s.tickets, x).is_some_and(|w| w.trim() == text.trim()))
        .nth(older)
        .map(|x| (t.clone(), x.n))
}

/// Whether the engine's copy of O is in its project's snapshot.
fn arrived(s: &App, o: &Out) -> bool {
    copy_of(s, o).is_some()
}

/// Each poll: the messages whose copy came, dropped (the post stands in their place); a take back he pressed sent once
/// its post is known (named by the answer, or found by its words ticket). Whether anything changed.
pub fn step(s: &mut App) -> bool {
    let due: Vec<(u64, Option<(String, u32)>)> = s.outgoing.list.iter().filter(|o| o.take_back && !o.taking && !o.flying && o.is == Is::Sent).map(|o| (o.id, o.post.clone().or_else(|| copy_of(s, o)))).collect();
    let mut changed = false;
    for (id, post) in due {
        if let Some(post) = post {
            if let Some(o) = s.outgoing.list.iter_mut().find(|o| o.id == id) {
                o.post = Some(post);
            }
            changed |= take_back_post(s, id);
        }
    }
    let came: Vec<u64> = s.outgoing.list.iter().filter(|o| !o.take_back && arrived(s, o)).map(|o| o.id).collect();
    s.outgoing.list.retain(|o| !came.contains(&o.id));
    // refused before it left the window: send it once its pipe is back, unless a window swap handed it over
    let handed = crate::restart::handed_over(s);
    let ready: Vec<u64> = s.outgoing.list.iter().filter(|o| !handed && o.unsent && !o.take_back && !o.taking && s.projects.get(o.key.0).is_some_and(|p| p.link.is_some() && p.down.is_none())).map(|o| o.id).collect();
    for id in ready {
        again(s, id);
        changed |= s.outgoing.list.iter().any(|o| o.id == id && o.flying);
    }
    changed || !came.is_empty()
}

/// Send message ID again, once (`Send again`).
pub fn again(s: &mut App, id: u64) {
    let Some(o) = s.outgoing.list.iter().find(|o| o.id == id).cloned() else { return };
    let what = pending(&o);
    if s.request_in(o.key.0, o.req.clone(), what) {
        flying(s, id, &o.req, None);
    } else if let Some(o) = s.outgoing.list.iter_mut().find(|o| o.id == id) {
        o.is = Is::NotSent("the box is not connected".into());
    }
}

/// What the answer to O's request goes to: a comment's to it alone, the box of its place left as he left it.
pub fn pending(o: &Out) -> crate::Pending {
    match &o.then {
        _ if o.comment.is_some() => crate::Pending::TicketDone { clear: crate::threads::Cleared::Comment(o.key.0, o.key.1.clone()) },
        Then::Box => crate::Pending::TicketDone { clear: crate::threads::Cleared::Out(o.key.0, o.key.1.clone()) },
        Then::Chat { typed, comments } => crate::Pending::Mailed { key: o.key.clone(), typed: typed.clone(), comments: comments.clone() },
    }
}

/// His `Take back` on message ID: one press, whatever its state (see the module's head).
pub fn take_back(s: &mut App, id: u64) {
    let Some(o) = s.outgoing.list.iter().find(|o| o.id == id) else { return };
    if o.take_back || o.taking {
        return;
    }
    let (is, flying, post) = (o.is.clone(), o.flying, o.post.is_some());
    let press = |s: &mut App| {
        if let Some(o) = s.outgoing.list.iter_mut().find(|o| o.id == id) {
            o.take_back = true;
        }
    };
    // in its cleaning: the words go back now, or when its words ticket's open is answered (words.rs `replied`); the
    // cleaning's answer then finds nothing to send
    if crate::words::holds(s, id) {
        press(s);
        crate::words::taken_back(s, id);
        return;
    }
    match (is, flying) {
        (Is::NotSent(_), _) => words_back(s, id),
        // its request is out: taken back when the engine answers (`answered`), or its words back if the pipe ends
        (Is::Sending, true) => press(s),
        // not out and not in a cleaning (put back from ui.json in between): nothing went, so nothing to take back there
        (Is::Sending, false) => words_back(s, id),
        // with the engine: its post taken back now, or at the poll that finds it (`step`)
        (Is::Sent, _) => {
            press(s);
            if post {
                take_back_post(s, id);
            }
        }
    }
}

/// Message ID's post taken back at the engine, once: true when the request went (`taking back…` until its answer);
/// else the take back stays pressed and goes at the next poll with a pipe.
fn take_back_post(s: &mut App, id: u64) -> bool {
    let Some(o) = s.outgoing.list.iter().find(|o| o.id == id) else { return false };
    let Some((t, n)) = o.post.clone() else { return false };
    let (pi, place) = o.key.clone();
    let clear = crate::threads::Cleared::TakeBack(pi, place);
    if !s.request_in(pi, Request::TakeBack { t, n }, crate::Pending::TicketDone { clear }) {
        return false;
    }
    if let Some(o) = s.outgoing.list.iter_mut().find(|o| o.id == id) {
        o.taking = true;
    }
    true
}

/// The engine's answer to a take back from box KEY (threads.rs, `Cleared::TakeBack`): the message whose take back was
/// with it goes when it was taken back (or had been already); refused, its post stands as the engine has it and the
/// refusal is said under the box.
pub fn took_back(s: &mut App, key: &(usize, String), ok: bool, out: &str) {
    let Some(o) = s.outgoing.list.iter_mut().find(|o| &o.key == key && o.taking) else { return };
    let id = o.id;
    if ok || out.contains("taken back already") {
        forget(s, id);
    } else {
        (o.taking, o.take_back) = (false, false);
    }
}

/// Message ID's words back in its box, before what he has begun there since; it is no longer on its way. A comment's
/// go back into the comment box at its passage (ask.rs `back`), or, while he writes one on another passage, into the
/// box of its place, quoted as it went.
pub fn words_back(s: &mut App, id: u64) {
    let Some(at) = s.outgoing.list.iter().position(|o| o.id == id) else { return };
    let o = s.outgoing.list.remove(at);
    if let Some(c) = o.comment.clone()
        && crate::ask::back(s, o.key.0, &o.key.1, c)
    {
        return;
    }
    // the comments that went in it held for its box again, before any held since; his words alone back in the box
    let alone = crate::ask::compose(&o.held, "").0;
    let typed = match &o.then {
        Then::Chat { typed, .. } if !typed.trim().is_empty() || !o.held.is_empty() => Some(typed.clone()),
        _ if o.held.is_empty() => Some(o.text.clone()),
        _ => o.text.strip_prefix(alone.as_str()).map(|rest| rest.trim_start().to_string()),
    };
    let Some(typed) = typed else {
        // its words are not as they went (no comments to give back apart): back whole, quotes and all
        before(s, o.key, &o.text);
        return;
    };
    if !o.held.is_empty() {
        let list = s.notes.held.entry(o.key.clone()).or_default();
        list.splice(0..0, o.held.iter().cloned());
    }
    before(s, o.key, &typed);
}

/// WORDS into box KEY before what he has begun there, never in place of it.
pub fn before(s: &mut App, key: (usize, String), words: &str) {
    if words.trim().is_empty() {
        return;
    }
    let e = s.composers.entry(key).or_insert_with(|| TextEdit::new(""));
    if e.text.trim().is_empty() {
        *e = TextEdit::new(words);
    } else {
        *e = TextEdit::new(format!("{}\n\n{}", words.trim_end(), e.text));
    }
}

/// What ui.json keeps: every message still on its way, by its project's id.
pub fn kept(s: &App) -> Vec<(String, &Out)> {
    s.outgoing.list.iter().filter_map(|o| Some((s.projects.get(o.key.0)?.id.clone(), o))).collect()
}

/// Put back from ui.json. One that was with the engine when the window ended had no answer this window can know:
/// `not sent`, unless the post it named or its words ticket says where it is. A take back whose answer did not come is
/// asked again at the first poll (the engine answers "taken back already" when it was).
pub fn put_back(s: &mut App, kept: Vec<(String, Out)>) {
    s.outgoing.list = kept
        .into_iter()
        .filter_map(|(id, mut o)| {
            o.key.0 = s.projects.iter().position(|p| p.id == id)?;
            if o.flying && o.is == Is::Sending && o.post.is_none() && o.words.is_none() {
                o.flying = false;
                o.is = Is::NotSent("not known whether the engine had it".into());
            }
            o.taking = false;
            Some(o)
        })
        .collect();
}

/// Where message O stands, as its bubble says it: `sending…`, `sent`, `not sent · why`, `taking back…`; also under
/// the question it answers while it is on its way (threads.rs `drawn`).
pub fn mark(o: &Out) -> String {
    match &o.is {
        _ if o.take_back || o.taking => "taking back…".to_string(),
        Is::Sending => "sending…".to_string(),
        Is::Sent => "sent".to_string(),
        Is::NotSent(why) => format!("not sent · {why}"),
    }
}

/// His messages on their way from the box of (PI, PLACE), oldest first, each as his bubble with its mark and its stage
/// under it ([`track`]: Sending, or Failed when the pipe did not take it); drawn above the box, so under a thread's
/// last post, under the post a reply box is at, at an agent's chat's foot. One at his look of its cleaning is that look
/// (words.rs), not a bubble.
pub fn bubbles(s: &App, pi: usize, place: &str, k: &Kit) -> Vec<El> {
    let key = (pi, place.to_string());
    let mut out = Vec::new();
    for o in s.outgoing.list.iter().filter(|o| o.key == key && !arrived(s, o) && !crate::words::at_look(s, o.id)) {
        let mark = mark(o);
        let mut head = row().gap(10.0).center_y();
        if !o.take_back && !o.taking {
            let a = o.id;
            head = head.child(Button::new("Take back").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| take_back(s, a)).build(k));
        }
        if matches!(o.is, Is::NotSent(_)) && !o.take_back {
            let b = o.id;
            head = head.child(Button::new("Send again").variant(ButtonVariant::Neutral).on_press(move |s: &mut App| again(s, b)).build(k));
        }
        head = head.child(crate::subthread::head("you", &format!("{} · {mark}", crate::when::at(&o.at)), k));
        let words = text(o.text.clone()).font(k.ty.body).color(k.c.text_base).wrap();
        // its mark says why it was not sent: the track says Failed alone
        let stage = match stage_of(o) {
            Stage::Failed(_) => Stage::Failed(String::new()),
            st => st,
        };
        let stage = track(&stage, format!("dock-track-out-{}", o.id), k);
        out.push(col().key(format!("out-{pi}-{place}-{}", o.id)).w_full().gap(4.0).items_end().child(head).child(crate::post::bubble(words, false, k)).child(stage));
    }
    out
}


/// Where a message of his stands: `Staged → Sending → Queued (turn) → Queued (tool) → Delivered → Answered / Failed`.
#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    /// Held for the box: a comment that goes with his next message there (ask.rs `Notes::held`), or his burst held
    /// by its handler until it goes (`Engine::Gathered`).
    Staged,
    /// Pressed and on its way to the engine; or with it, its handler deciding whom it is for.
    Sending,
    /// With the engine, not handed yet: its agent is in a turn, or stopped (handed when it starts).
    QueuedTurn,
    /// Handed into its agent's session queue: its text enters the model's input at the harness's next step.
    QueuedTool,
    /// Its text entered the model's input.
    Delivered,
    /// Its agent answered it.
    Answered,
    /// Not sent, dropped, not delivered, handed twice and not read, or left unanswered: why.
    Failed(String),
}

impl Stage {
    const TRACK: [&'static str; 6] = ["Staged", "Sending", "Queued (turn)", "Queued (tool)", "Delivered", "Answered / Failed"];

    fn at(&self) -> usize {
        match self {
            Stage::Staged => 0,
            Stage::Sending => 1,
            Stage::QueuedTurn => 2,
            Stage::QueuedTool => 3,
            Stage::Delivered => 4,
            Stage::Answered | Stage::Failed(_) => 5,
        }
    }

    /// Its word: the track's, "Answered" or "Failed" for the last.
    pub fn word(&self) -> &'static str {
        match self {
            Stage::Answered => "Answered",
            Stage::Failed(_) => "Failed",
            s => Stage::TRACK[s.at()],
        }
    }
}

/// The stage of O, a message of his whose copy is not in the snapshot yet.
pub(crate) fn stage_of(o: &Out) -> Stage {
    match &o.is {
        Is::Sending | Is::Sent => Stage::Sending,
        Is::NotSent(why) => Stage::Failed(format!("not sent: {why}")),
    }
}

/// The stage of his post with one agent, as its whereabouts entry says (whereabouts.rs `Is`).
pub(crate) fn stage_with(is: &crate::whereabouts::Is) -> Stage {
    use crate::whereabouts::Is as W;
    match is {
        W::Waiting { .. } => Stage::QueuedTurn,
        W::Sent { .. } => Stage::QueuedTool,
        W::Delivered { .. } => Stage::Delivered,
        W::Answered { .. } => Stage::Answered,
        W::Twice => Stage::Failed("handed twice and not read".into()),
        W::NoReply => Stage::Failed("its turn ended with no reply".into()),
        W::NotAnswered { .. } => Stage::Failed("delivered and not answered".into()),
        W::NotDelivered { why } => Stage::Failed(format!("not delivered: {why}")),
    }
}

/// STAGE as its track's six words, keyed KEY: the one it is at lit (keyed `KEY-at`; "Answered" or "Failed", in the
/// danger colour, in place of "Answered / Failed"), those it passed muted, those to come faint; why it failed after,
/// when it says.
fn track(stage: &Stage, key: String, k: &Kit) -> El {
    let at = stage.at();
    let mut r = row().key(key.clone()).gap(4.0).center_y().flex_wrap();
    for (i, w) in Stage::TRACK.iter().enumerate() {
        if i > 0 {
            r = r.child(text("→").font(k.ty.small).color(k.c.text_faint));
        }
        r = r.child(match i.cmp(&at) {
            std::cmp::Ordering::Less => text(*w).font(k.ty.small).color(k.c.text_muted),
            std::cmp::Ordering::Greater => text(*w).font(k.ty.small).color(k.c.text_faint),
            std::cmp::Ordering::Equal => {
                let color = if matches!(stage, Stage::Failed(_)) { k.c.danger.fg } else { k.c.text_base };
                text(stage.word()).font(k.ty.small_medium).color(color).key(format!("{key}-at"))
            }
        });
    }
    if let Stage::Failed(why) = stage
        && !why.is_empty()
    {
        r = r.child(text(format!("· {why}")).font(k.ty.small).color(k.c.danger.fg));
    }
    r
}

/// How many of his newest messages on the thread a box sends on the dock looks at: those not answered yet are in it.
const FOLLOWED: usize = 3;

/// The agent the box of PLACE in P is: an agent's chat box is keyed by its name (message_box.rs `Target::place`).
fn agent_of<'a>(p: &Proj, place: &'a str) -> Option<&'a str> {
    p.live.agents.iter().any(|a| a.name == place).then_some(place)
}

/// The thread the box of PLACE in project PI sends on: its own (and a question's under it), or the one an agent's chat
/// box sends into (withyou.rs `bound`); none for a new thread's box.
fn sends_on<'a>(s: &App, p: &'a Proj, pi: usize, place: &str) -> Option<&'a TicketView> {
    let id = match crate::message_box::thread_of(place) {
        Some(t) => t.to_string(),
        None => crate::withyou::bound(s, pi, agent_of(p, place)?)?,
    };
    p.live.tickets.values().find(|v| v.id == id)
}

/// Whether the dock over V's box follows his post X ([`unsettled`]).
pub(crate) fn follows(p: &Proj, v: &TicketView, x: &docket_machine::Post) -> bool {
    unsettled(p, v).iter().any(|(y, _)| y.n == x.n)
}

/// His newest posts on V ([`FOLLOWED`]) not answered yet by all they went to, oldest first, with where each stands; one
/// with nothing to say is not followed: no agent given it, the engine not holding it, and named or handed already
/// (the line under the post says nothing of it either, whereabouts.rs `line`).
fn unsettled<'a>(p: &Proj, v: &'a TicketView) -> Vec<(&'a docket_machine::Post, crate::whereabouts::Where)> {
    use crate::whereabouts::Engine;
    let says = |x: &docket_machine::Post, w: &crate::whereabouts::Where| !w.settled() && (w.engine != Engine::None || !w.entries.is_empty() || crate::whereabouts::not_given_yet(x));
    let mut out: Vec<_> = v.posts.iter().rev().filter(|x| x.by == "owner").take(FOLLOWED).filter_map(|x| crate::whereabouts::of(p, v, x).filter(|w| says(x, w)).map(|w| (x, w))).collect();
    out.reverse();
    out
}

/// The outbox dock over the box of PLACE: each message of his from staged to answered, and the pace and step controls.
pub fn dock(s: &App, pi: usize, place: &str, k: &Kit) -> Vec<El> {
    let mut rows: Vec<El> = Vec::new();
    let staged = crate::ask::held(s, pi, place).len();
    if staged > 0 {
        // what they are and their × stand in the box they go with (ask.rs `held_lines`): here, how many
        let words = if staged == 1 { "1 comment".to_string() } else { format!("{staged} comments") };
        rows.push(row().w_full().gap(8.0).center_y().flex_wrap().child(track(&Stage::Staged, format!("dock-track-staged-{place}"), k).shrink().min_w(0.0)).child(text(words).font(k.ty.small).color(k.c.text_faint)));
    }
    let p = s.projects.get(pi);
    let agent = p.and_then(|p| agent_of(p, place));
    let thread = p.and_then(|p| sends_on(s, p, pi, place));
    let ctl = control(s, pi);
    let paced = if ctl { p.zip(agent).and_then(|(p, a)| p.live.agents.iter().find(|x| x.name == a)) } else { None };
    let mut waiting: Vec<String> = Vec::new();
    if let (Some(p), Some(v)) = (p, thread) {
        for (x, w) in unsettled(p, v) {
            for e in w.entries.iter().filter(|e| matches!(e.is, crate::whereabouts::Is::Waiting { in_turn: Some(_), .. })) {
                if !waiting.contains(&e.agent) {
                    waiting.push(e.agent.clone());
                }
            }
            rows.push(post_row(s, pi, v, x, &w, agent, k));
        }
    }
    // the held slow-mode moment: the agent's own on its box, the thread's first on a thread's, with whom it goes to
    let halt = p.and_then(|p| match (agent, thread) {
        (Some(a), _) => crate::slow::waiting_on(p, a).map(|(t, n)| (t, n, vec![a.to_string()])),
        (None, Some(v)) => crate::slow::shown(p, v).first().map(|(t, h)| (t.id.clone(), crate::slow::n_of(h), crate::slow::goes_to(h).into_iter().map(String::from).collect())),
        (None, None) => None,
    });
    // no dock with nothing to show, except for an agent in Step, so its pace stays in sight
    let stepped = paced.is_some_and(|a| a.control == coord_types::AgentMode::Step);
    if thread.is_none() && rows.is_empty() && halt.is_none() && !stepped {
        return bubbles(s, pi, place, k);
    }
    let mut head = row().w_full().gap(8.0).center_y().key(format!("dock-head-{place}")).child(text("Outbox").font(k.ty.small_medium).color(k.c.text_muted)).child(rune::el().grow(1.0));
    match (paced, thread) {
        (Some(a), _) => head = head.child(pace(a, pi, k)),
        (None, Some(v)) => head = head.child(crate::slow::switch(v, k)),
        (None, None) => {}
    }
    if let Some(p) = p {
        // whose turns Stop (Esc) → Step stops: the held moment's agents, those his posts wait on, the box's own agent
        let mut to: Vec<String> = halt.as_ref().map(|h| h.2.clone()).unwrap_or_default();
        to.extend(waiting.iter().cloned().filter(|_| halt.is_some() || ctl));
        to.extend(agent.filter(|_| ctl).map(String::from));
        let working: Vec<String> = p.live.agents.iter().filter(|a| a.state == "working" && to.contains(&a.name)).map(|a| a.name.clone()).collect();
        if let Some(b) = stop_step(pi, ctl, working, halt.map(|(t, n, _)| (t, n)), k) {
            head = head.child(b.key(format!("dock-step-{place}")));
        }
    }
    let mut out = vec![col().key(format!("dock-{pi}-{place}")).w_full().gap(6.0).px(10.0).py(8.0).radius(k.radius.md).border(1.0, k.c.border_weak).child(head).children(rows)];
    out.extend(bubbles(s, pi, place, k));
    out
}

/// Whether project PI's coordd takes DeliverUrgent, AgentPace and Esc's `step` (`CONTROL_PROTOCOL`).
fn control(s: &App, pi: usize) -> bool {
    s.projects.get(pi).is_some_and(|p| p.protocol >= coord_types::CONTROL_PROTOCOL)
}

/// Hands his waiting words to AGENT now: INTERRUPT stops its turn first, else after its running tool call; older coordd: Esc.
fn urgent(s: &mut App, pi: usize, agent: &str, interrupt: bool) {
    if control(s, pi) {
        s.request_in(pi, Request::DeliverUrgent { agent: agent.to_string(), interrupt }, crate::Pending::Sent);
    } else if interrupt {
        s.request_in(pi, Request::Esc { to: vec![agent.to_string()], step: false }, crate::Pending::Sent);
    }
}

/// Stop (Esc) → Step while any of WORKING is in a turn (CTL: one Esc with `step`; older: Esc, then HALT approved), else Step for HALT.
fn stop_step(pi: usize, ctl: bool, working: Vec<String>, halt: Option<(String, u32)>, k: &Kit) -> Option<El> {
    let b = if !working.is_empty() && (ctl || halt.is_some()) {
        let help = if ctl { "Stop its turn and switch to Step" } else { "Stop its turns, then let it go" };
        let halt = halt.filter(|_| !ctl);
        Button::new("Stop (Esc) → Step").variant(ButtonVariant::Neutral).help(help).on_press(move |s: &mut App| {
            s.request_in(pi, Request::Esc { to: working.clone(), step: ctl }, crate::Pending::Sent);
            if let Some((t, n)) = &halt {
                crate::slow::approve_in(s, pi, t, *n);
            }
        })
    } else {
        let (t, n) = halt?;
        Button::new("Step").variant(ButtonVariant::Neutral).help("Let the moment held go").on_press(move |s: &mut App| crate::slow::approve_in(s, pi, &t, n))
    };
    Some(b.build(k))
}

/// `Step (Manual) | Run (Auto)` for agent A's pace, sent as `Request::AgentPace`.
fn pace(a: &coord_types::AgentView, pi: usize, k: &Kit) -> El {
    let (name, step) = (a.name.clone(), a.control == coord_types::AgentMode::Step);
    rune_kit::SegmentedControl::new(&["Step (Manual)", "Run (Auto)"], Some(if step { 0 } else { 1 }))
        .fit()
        .on_pick(move |s: &mut App, i, _| {
            if (i == 0) != step {
                let mode = if i == 0 { coord_types::AgentMode::Step } else { coord_types::AgentMode::Run };
                s.request_in(pi, Request::AgentPace { name: name.clone(), mode }, crate::Pending::Sent);
            }
        })
        .build(k)
        .key(format!("pace-{}", a.name))
        .help("Step waits for you; Run goes at once.")
}

/// His post X on V in the dock, with where it stands W: its number and first line, then the engine's stage when it
/// holds it, and each agent's (on an agent's box, AGENT's alone).
fn post_row(s: &App, pi: usize, v: &TicketView, x: &docket_machine::Post, w: &crate::whereabouts::Where, agent: Option<&str>, k: &Kit) -> El {
    use crate::whereabouts::Engine;
    let mut c = col().key(format!("dock-post-{}.p{}", v.id, x.n)).w_full().gap(4.0).child(text(format!("#{} · {}", x.n, crate::when::at(&x.at.to_string()))).font(k.ty.small).color(k.c.text_muted));
    // the engine's stage while it holds it; his burst's Send now stands on its card (chat.rs `held_card`)
    let engine = match &w.engine {
        Engine::Dropped => Some(Stage::Failed("dropped: no agent was given it".into())),
        Engine::Held { .. } => Some(Stage::Sending),
        Engine::Gathered { .. } => Some(Stage::Staged),
        // in the thread, and nobody given it yet: the engine has it (`unsettled` follows no other with no entry)
        Engine::None if w.entries.is_empty() => Some(Stage::Sending),
        Engine::None => None,
    };
    if let Some(stage) = engine {
        c = c.child(track(&stage, format!("dock-track-{}.p{}", v.id, x.n), k).shrink().min_w(0.0));
    }
    for e in w.entries.iter().filter(|e| agent.is_none_or(|a| e.agent == a)) {
        // the track takes what the row leaves and wraps in it, so the press beside it stays in the dock
        let mut r = row().w_full().gap(8.0).center_y().child(text(e.agent.clone()).font(k.ty.small_medium).color(k.c.text_base)).child(track(&stage_with(&e.is), format!("dock-track-{}.p{}-{}", v.id, x.n, e.agent), k).grow(1.0).shrink().min_w(0.0));
        // answered: its answer is a press away, as under the post
        if let crate::whereabouts::Is::Answered { post, .. } = e.is {
            let (t, id) = (v.n, v.id.clone());
            r = r.child(k.interactive(row().child(text(format!("#{post}")).font(k.ty.small).color(k.c.text_muted))).on_press(move |s: &mut App| crate::find::open_at(s, t, &id, Some(post))));
        }
        if matches!(e.is, crate::whereabouts::Is::Waiting { in_turn: Some(_), .. }) {
            // two stages: Send now, then in its place Interrupt & Send (Esc), so no one press stops a turn by itself
            let post = format!("{}.p{}", v.id, x.n);
            let who = e.agent.clone();
            let b = if s.outgoing.armed.as_ref().is_some_and(|(p, x, a)| *p == pi && *x == post && *a == e.agent) {
                Button::new("Interrupt & Send (Esc)")
                    .variant(ButtonVariant::Neutral)
                    .help("Stop its turn, then hand it over")
                    .on_press(move |s: &mut App| {
                        s.outgoing.armed = None;
                        urgent(s, pi, &who, true);
                    })
                    .build(k)
                    .key(format!("dock-interrupt-{post}-{}", e.agent))
            } else {
                let help = "Waits for its turn to end";
                Button::new("Send now")
                    .variant(ButtonVariant::Ghost)
                    .help(help)
                    .on_press(move |s: &mut App| {
                        s.outgoing.armed = Some((pi, post.clone(), who.clone()));
                        urgent(s, pi, &who, false);
                    })
                    .build(k)
                    .key(format!("dock-send-now-{}.p{}-{}", v.id, x.n, e.agent))
            };
            r = r.child(b);
        }
        c = c.child(r);
    }
    c
}

/// Whether agent NAME's box in project PI holds something its Send would take: words, or comments held for it.
pub(crate) fn written(s: &App, pi: usize, name: &str) -> bool {
    s.composers.get(&(pi, name.to_string())).is_some_and(|t| !t.text.trim().is_empty()) || !crate::ask::held(s, pi, name).is_empty()
}

/// Esc in a working agent's box: sends what it holds and hands it at once, or with nothing written stops the turn alone.
pub(crate) fn interrupt(s: &mut App, pi: usize, name: &str) {
    if written(s, pi, name) {
        s.send(pi, name, false);
        if !written(s, pi, name) {
            urgent(s, pi, name, true);
        }
        return;
    }
    s.request_in(pi, Request::Esc { to: vec![name.to_string()], step: false }, crate::Pending::Sent);
}

/// Esc in a box, after its other keys (so Esc closes a suggestion or a comment's box first): with the dock's Send now
/// pressed in its project, its second stage, Interrupt & Send; in an agent's box while the agent is in a turn,
/// [`interrupt`]. Otherwise Esc is not the box's.
pub(crate) fn esc_key(s: &mut App, target: &crate::message_box::Target, ev: &rune::KeyEvent) -> bool {
    if ev.key != rune::Key::Escape || ev.mods != rune::Mods::default() {
        return false;
    }
    if let Some((pi, _, agent)) = s.outgoing.armed.clone()
        && pi == target.project()
    {
        s.outgoing.armed = None;
        urgent(s, pi, &agent, true);
        return true;
    }
    let crate::message_box::Target::Agent { project, name } = target else { return false };
    let working = s.projects.get(*project).and_then(|p| p.live.agents.iter().find(|a| &a.name == name)).is_some_and(|a| a.state == "working");
    if working {
        interrupt(s, *project, name);
    }
    working
}
