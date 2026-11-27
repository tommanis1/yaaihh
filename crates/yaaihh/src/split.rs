//! His messages split by point (the owner, N-284.p13: "we need to also split up user messages"; paper-research
//! N-773; the request's shape, a2h-speed N-696.p4). Nothing of his is placed without his seeing it first (N-284.p17).
//!
//! - Off until he turns it on: one item, `Split my messages by point`, in the tray's menu beside the spelling setting
//!   (the window has no Settings page; paper-research N-589.p22), shown only while a coordd is at `SPLIT_PROTOCOL`,
//!   remembered (`split.json` beside the config). A refusal turns it off and says so once under the box.
//! - Asked at a pause, never at Send: when a box's text has stood unchanged for `changed::PAUSE` (the cleaning's
//!   wait) and is not the text last asked, the window asks `split` for it. With no proposal for exactly the text in the
//!   box, the message goes as one, as ever.
//! - The proposal shows under the box: a chip per point that would go elsewhere, `runner rows: yes, do … → N-575 ·
//!   Runner rows` or `→ new thread`; while the pointer rests on a chip its point is tinted in the box (no line under a
//!   point: consult-4, N-589.p26). A click keeps that point here; a second sends it there again.
//! - Send sends what the chips say, and only when something stays where he writes: that as one post there, his line
//!   breaks and marks as typed, and each point elsewhere a post of its own, each with `split_from`. Otherwise one
//!   message, as ever.
//! - After sending, under his post, one faint line per point sent elsewhere, with `Bring back` (the existing move).
//! - Offered on a thread's box, a reply at a post, a new thread and a subthread; not in an agent's own box, a box
//!   answering a question's asks, a comment on a passage (it goes from its own box, ask.rs `send`), a note, the
//!   intake, a step.

use crate::message_box::{Target, Way};
use crate::{App, Pending};
use coord_types::Request;
use docket_machine::SplitFrom;
use rune::{El, row, text};
use rune_kit::{Button, ButtonVariant, Kit};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::time::Instant;

/// One point of his message, as coordd checked it: his words letter for letter, where they are in the text (in
/// characters), and where it would go: a thread id, `new` or `here`, with that thread's label.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Point {
    pub words: String,
    pub from: usize,
    pub len: usize,
    pub to: String,
    #[serde(default)]
    pub label: Option<String>,
}

/// The proposal for one box: the text it was made for, its points, and those he kept here by a click.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Proposal {
    pub text: String,
    pub points: Vec<Point>,
    pub kept_here: BTreeSet<usize>,
    /// The points already posted elsewhere by a Send whose other posts did not all go: a second Send does not post
    /// them again (consult-4, N-589.p26, a).
    #[serde(default)]
    pub sent: BTreeSet<usize>,
    /// The message's key, from its first Send on, so a second Send's posts are points of the same message.
    #[serde(default)]
    pub message: Option<String>,
    /// From a Send of all of it elsewhere on: the thread he wrote on, where the points taken have their lines (so a
    /// stopped Send of it carries nothing as point 1's).
    #[serde(default)]
    pub home: Option<String>,
}

#[derive(Default)]
pub struct Splits {
    pub on: bool,
    /// The proposal for each box, by (project, place).
    proposals: HashMap<(usize, String), Proposal>,
    /// Each box waiting for its pause: its target, its text and since when it stands.
    waiting: HashMap<(usize, String), (Target, String, Instant)>,
    /// The text last asked for each box (asked once per text).
    asked: HashMap<(usize, String), String>,
    /// Each box whose points are on their way: the post that stays here (sent once every point is taken), the points
    /// not answered yet, and whether one was refused (then nothing more goes and the box keeps its text).
    sending: HashMap<(usize, String), Going>,
    /// The chip the pointer rests on (its box, its point): that point's words are tinted in the box.
    hover: Option<((usize, String), usize)>,
    /// What the tray's menu was last told (None: no coordd splits, the switch not shown).
    in_tray: Option<Option<bool>>,
    /// Each box he typed in, by its target: a text put in by the window (changed words' cleaning at the same pause)
    /// is asked for too, after its own pause.
    boxes: HashMap<(usize, String), Target>,
    /// Each message he sent split, by its key: each point sent elsewhere (where, its label, its words), so its line
    /// stands under his post before that post is in the view (then without `Bring back`, paper-research N-773.p3).
    sent: HashMap<String, Vec<(String, String, String)>>,
    /// Each box whose Send posted some points and then stopped (a point refused, the link dropped): the message's key
    /// and each point posted (where, its label, its words). Those words are out of the box by then, so editing what is
    /// left cannot post them twice; the next Send of the box is the same message (N-788).
    carry: HashMap<(usize, String), Carry>,
    /// The points of a message sent wholly elsewhere, by (project, the thread he wrote on): each where, its label, its
    /// words. They stand at that thread's foot, above the box, until Bring back or × (N-788).
    left: HashMap<(usize, String), Vec<(String, String, String)>>,
}

/// One box's send on its way: the post that stays here (none when all of it goes elsewhere), the points not answered
/// yet, whether one was refused, and the thread he wrote on when nothing stays there.
pub struct Going {
    here: Option<Request>,
    out: BTreeSet<usize>,
    failed: bool,
    home: Option<String>,
}

/// What a box's stopped Send already posted ([`Splits::carry`]).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Carry {
    pub message: String,
    pub lines: Vec<(String, String, String)>,
}

fn path() -> std::path::PathBuf {
    std::path::Path::new(&crate::projects::config_path()).with_file_name("split.json")
}

impl Splits {
    pub fn load() -> Splits {
        let on = std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|v| v["on"].as_bool()).unwrap_or(false);
        Splits { on, ..Default::default() }
    }

    pub fn toggle(&mut self) {
        self.on = !self.on;
        if !self.on {
            self.proposals.clear();
            self.waiting.clear();
            self.boxes.clear();
        }
        let _ = std::fs::write(path(), serde_json::json!({ "on": self.on }).to_string());
    }
}

/// The proposals that fit their boxes now, for the window's state (state.rs: a swap keeps them, and his clicks).
pub fn kept(s: &App) -> Vec<((usize, String), Proposal)> {
    let mut v: Vec<_> = s.split.proposals.keys().filter_map(|k| Some((k.clone(), proposal(s, k)?.clone()))).collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

/// The proposals the window before this one kept (state.rs), each for the box that still holds its text.
pub fn put_back(s: &mut App, kept: Vec<((usize, String), Proposal)>) {
    for (k, p) in kept {
        if s.composers.get(&k).map(|e| e.text.as_str()) == Some(p.text.as_str()) {
            s.split.asked.insert(k.clone(), p.text.clone());
            s.split.proposals.insert(k, p);
        }
    }
}

/// What the boxes' stopped Sends posted, for the window's state (state.rs), each for a box still holding text.
pub fn carried(s: &App) -> Vec<((usize, String), Carry)> {
    let mut v: Vec<_> = s.split.carry.iter().filter(|(k, _)| s.composers.get(*k).is_some_and(|e| !e.text.trim().is_empty())).map(|(k, c)| (k.clone(), c.clone())).collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

/// The lines of messages sent wholly elsewhere, for the window's state (state.rs).
pub fn left_kept(s: &App) -> Vec<((usize, String), Vec<(String, String, String)>)> {
    let mut v: Vec<_> = s.split.left.iter().filter(|(_, l)| !l.is_empty()).map(|(k, l)| (k.clone(), l.clone())).collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

/// What the window before this one kept of [`carried`] and [`left_kept`].
pub fn put_after(s: &mut App, carry: Vec<((usize, String), Carry)>, left: Vec<((usize, String), Vec<(String, String, String)>)>) {
    for (k, c) in carry {
        if s.composers.get(&k).is_some_and(|e| !e.text.trim().is_empty()) {
            s.split.sent.entry(c.message.clone()).or_insert_with(|| c.lines.clone());
            s.split.carry.insert(k, c);
        }
    }
    for (k, l) in left {
        s.split.left.insert(k, l);
    }
}

/// Where the switch is, as the refusal says it: the tray's menu, or beside Spelling under the box where there is no
/// tray icon (N-788).
fn switch_is() -> &'static str {
    if crate::tray::shown() { "The tray menu turns it on again." } else { "Turn it on again beside Spelling." }
}

/// The switch's words under a box (message_box.rs): short, for a narrow pane. The tray's menu, which has no box
/// around it to say whose points, keeps "Split my messages by point" (tray.rs), and the switch's hover begins with
/// those words. [`switch_is`] names the switch as it is drawn here.
pub const BOX_LABEL: &str = "Split by point";

/// Whether the switch stands beside Spelling under TARGET's box: where there is no tray icon, in a box that splits.
pub fn switch_here(s: &App, target: &Target) -> bool {
    !crate::tray::shown() && served(s, target.project()) && splittable(s, target)
}

/// Whether project PI's coordd answers `split`.
pub fn served(s: &App, pi: usize) -> bool {
    s.projects.get(pi).is_some_and(|p| p.protocol >= coord_types::SPLIT_PROTOCOL)
}

/// Whether TARGET's box offers a split (the module's list).
pub fn splittable(s: &App, target: &Target) -> bool {
    let pi = target.project();
    match target {
        Target::NewThread { .. } => true,
        Target::Thread { t, post, way: Way::Here, .. } => {
            let Some(v) = s.projects.get(pi).and_then(|p| p.live.tickets.values().find(|v| &v.id == t)) else { return false };
            if v.kind == "note" {
                return false;
            }
            match post {
                None => true,
                Some(n) => v.posts.iter().find(|q| q.n == *n).is_some_and(|q| !crate::threads::open_to_owner(v, q)),
            }
        }
        Target::Thread { way: Way::Subthread, .. } => true,
        _ => false,
    }
}

/// The box of TARGET changed (its `on_input_change`): its proposal stands only for the text it was made for, and its
/// pause starts again.
pub fn changed(s: &mut App, target: &Target) {
    if !s.split.on || !served(s, target.project()) || !splittable(s, target) {
        return;
    }
    let key = (target.project(), target.place());
    let text = s.composers.get(&key).map(|e| e.text.clone()).unwrap_or_default();
    s.split.boxes.insert(key.clone(), target.clone());
    s.split.waiting.insert(key, (target.clone(), text, Instant::now()));
}

/// The window's step: each box whose text stood for the pause and was not asked yet is asked once. True when what
/// shows changed (a proposal no longer fits its box).
pub fn poll(s: &mut App, tx: &mut rune::Tx) -> bool {
    let mut changed = false;
    // the switch, in the tray's menu beside the spelling setting, only while a coordd splits
    if crate::tray::SPLIT_PICKED.swap(false, std::sync::atomic::Ordering::SeqCst) {
        s.split.toggle();
        changed = true;
    }
    let shows = (0..s.projects.len()).any(|pi| served(s, pi)).then_some(s.split.on);
    if s.split.in_tray != Some(shows) {
        s.split.in_tray = Some(shows);
        crate::tray::split_changed(shows);
    }
    // a Send that stopped with some points posted: their words leave the box (N-788)
    let stopped: Vec<_> = s.split.proposals.iter().filter(|(k, p)| !p.sent.is_empty() && !s.split.sending.contains_key(*k)).map(|(k, _)| k.clone()).collect();
    for k in stopped {
        rest(s, &k);
        changed = true;
    }
    // what a stopped Send posted is carried while the box holds text; emptied (sent, or cleared by him), it goes
    s.split.carry.retain(|k, _| s.composers.get(k).is_some_and(|e| !e.text.trim().is_empty()) || s.split.sending.contains_key(k));
    // a proposal for a text no longer in its box goes
    let stale: Vec<_> = s.split.proposals.iter().filter(|(k, p)| s.composers.get(*k).map(|e| e.text.as_str()) != Some(p.text.as_str())).map(|(k, _)| k.clone()).collect();
    for k in stale {
        s.split.proposals.remove(&k);
        changed = true;
    }
    // a box whose text the window changed since it was asked (no input change): its pause starts now
    if s.split.on {
        let moved: Vec<_> = s
            .split
            .boxes
            .iter()
            .filter(|(k, _)| !s.split.waiting.contains_key(*k))
            .filter_map(|(k, t)| {
                let now = s.composers.get(k).map(|e| e.text.clone()).filter(|x| !x.trim().is_empty())?;
                (s.split.asked.get(k) != Some(&now)).then(|| (k.clone(), t.clone(), now))
            })
            .collect();
        for (k, t, now) in moved {
            s.split.waiting.insert(k, (t, now, Instant::now()));
        }
    }
    if s.split.waiting.is_empty() {
        return changed;
    }
    let due: Vec<_> = s.split.waiting.iter().filter(|(_, (_, _, at))| at.elapsed() >= crate::changed::PAUSE).map(|(k, _)| k.clone()).collect();
    for key in due {
        let Some((target, text, _)) = s.split.waiting.remove(&key) else { continue };
        let now = s.composers.get(&key).map(|e| e.text.clone()).unwrap_or_default();
        if now.trim().is_empty() {
            s.split.boxes.remove(&key);
            continue;
        }
        if now != text {
            // changed meanwhile without an input change: its pause starts again
            s.split.waiting.insert(key, (target, now, Instant::now()));
            continue;
        }
        if s.split.asked.get(&key) == Some(&text) {
            continue;
        }
        // its project's pipe down: nothing asked and nothing marked asked, no "not sent" while he types; the box waits
        // again (`moved`) once the pipe is back, and is asked then (proto-reader, N-1446 p284)
        if s.projects.get(key.0).is_none_or(|p| p.down.is_some() || p.link.is_none()) {
            continue;
        }
        if s.request_in(key.0, Request::Split { text: text.clone() }, Pending::Split { key: key.clone(), text: text.clone() }) {
            s.split.asked.insert(key, text);
        }
    }
    if !s.split.waiting.is_empty() {
        // the window's own one-second tick, while a box waits for its pause
        s.wake_every(std::time::Duration::from_secs(1), tx);
    }
    changed
}

/// coordd's answer to `split` for box KEY, asked for TEXT: a proposal when it says `Points` and the box still holds
/// TEXT; anything else (`One`, "not split: …") shows nothing.
pub fn replied(s: &mut App, key: (usize, String), text: String, ok: bool, out: String) {
    #[derive(Deserialize)]
    struct Answer {
        head: String,
        #[serde(default)]
        points: Vec<Point>,
    }
    if !ok {
        refused(s, &key);
        return;
    }
    let points = match serde_json::from_str::<Answer>(&out) {
        Ok(a) if a.head == "Points" => a.points,
        _ => {
            s.split.proposals.remove(&key);
            return;
        }
    };
    if s.composers.get(&key).map(|e| e.text.as_str()) != Some(text.as_str()) || !points.iter().any(|p| p.to != "here") {
        return;
    }
    s.split.proposals.insert(key, Proposal { text, points, kept_here: BTreeSet::new(), ..Default::default() });
}

/// coordd would not split for box KEY (a project with no `split`, or no model answering: the window cannot tell
/// which without the engine's words): the switch goes off, said once under the box in the window's words, so it never
/// stands on doing nothing (paper-research N-589.p22, 1). Turning it on tries again.
fn refused(s: &mut App, key: &(usize, String)) {
    s.split.proposals.remove(key);
    if !s.split.on {
        return;
    }
    s.split.toggle();
    let name = s.projects.get(key.0).map(|p| p.name.clone()).unwrap_or_default();
    s.box_said.insert(key.clone(), Ok(format!("Splitting failed for {name}; off. {}", switch_is())));
}

/// The proposal for box KEY that fits its text now.
fn proposal<'a>(s: &'a App, key: &(usize, String)) -> Option<&'a Proposal> {
    let p = s.split.proposals.get(key)?;
    (s.split.on && s.composers.get(key).map(|e| e.text.as_str()) == Some(p.text.as_str())).then_some(p)
}

/// The byte range in TEXT of the characters FROM..FROM+LEN.
fn bytes(text: &str, from: usize, len: usize) -> Option<std::ops::Range<usize>> {
    let mut it = text.char_indices().map(|(i, _)| i).chain(std::iter::once(text.len()));
    let a = it.clone().nth(from)?;
    let b = it.nth(from + len)?;
    Some(a..b)
}

/// The point whose chip the pointer rests on, tinted in the box (COLOR, the selection's ink made faint): no line
/// under a point at rest, so it is never read as one more spelling mark (consult-4, N-589.p26, 4).
pub fn spans(s: &App, key: &(usize, String), color: rune::Color) -> Vec<rune::Span> {
    let Some(p) = proposal(s, key) else { return Vec::new() };
    let Some((_, i)) = s.split.hover.as_ref().filter(|(k, _)| k == key) else { return Vec::new() };
    p.points.get(*i).and_then(|pt| bytes(&p.text, pt.from, pt.len)).map(|r| rune::Span::new(r.start, r.end).bg(color, 1.0)).into_iter().collect()
}

/// The tint of a point in the box: the selection's ink, faint.
pub fn tint(k: &Kit) -> rune::Color {
    let c = k.selection();
    rune::Color { a: c.a * 0.5, ..c }
}

/// Where point PT goes, as its chip says it: id and label, or a new thread.
fn place_said(pt: &Point) -> String {
    match (pt.to.as_str(), &pt.label) {
        ("new", _) => "→ new thread".into(),
        (t, Some(l)) => format!("→ {t} · {l}"),
        (t, None) => format!("→ {t}"),
    }
}

/// The first words of a point, for its chip.
fn opening(words: &str) -> String {
    let w: Vec<&str> = words.split_whitespace().collect();
    if w.len() <= 4 { w.join(" ") } else { format!("{} …", w[..4].join(" ")) }
}

/// Under TARGET's box: the points a stopped Send already posted, each a faint line ending `· sent`, so editing what
/// is left keeps them said (N-788); then the chips, one per point that would go elsewhere, its opening words and where
/// it goes; a click keeps it here, a second sends it there again.
pub fn chips(s: &App, target: &Target, k: &Kit) -> Vec<El> {
    let key = (target.project(), target.place());
    let mut out: Vec<El> = s
        .split
        .carry
        .get(&key)
        .map(|c| c.lines.iter().enumerate().map(|(i, (t, label, words))| row().key(format!("split-sent-{}-{}-{i}", key.0, key.1)).w_full().child(text(format!("{} · sent", said(t, label, words))).font(k.ty.small).color(k.c.text_muted).truncate().shrink())).collect())
        .unwrap_or_default();
    let Some(p) = proposal(s, &key) else { return out };
    let mut r = row().key(format!("split-chips-{}-{}", key.0, key.1)).w_full().gap(6.0).center_y().flex_wrap();
    // all of it about elsewhere where Send cannot send it there (no thread of his to leave its line on): it posts it
    // here as one message, so no chip says otherwise (consult-4 N-589.p31 a; paper-research p33)
    if let Some(said) = all_elsewhere(p).filter(|_| !wholly_sent(s, target)) {
        out.push(r.child(text(said).font(k.ty.small).color(k.c.text_faint)));
        return out;
    }
    for (i, pt) in p.points.iter().enumerate().filter(|(_, pt)| pt.to != "here") {
        if p.sent.contains(&i) {
            r = r.child(text(format!("{} {} · sent", opening(&pt.words), place_said(pt))).font(k.ty.small).color(k.c.text_muted));
            continue;
        }
        let here = p.kept_here.contains(&i);
        let label = if here { format!("{} · stays here", opening(&pt.words)) } else { format!("{} {}", opening(&pt.words), place_said(pt)) };
        let (at, over) = (key.clone(), key.clone());
        // its words cut with … where the pane is narrower than the chip, never drawn past the box
        let said = label.clone();
        r = r.child(
            Button::new(label)
                .variant(ButtonVariant::Outline)
                .label_with(move |font, ink| text(said).font(font).color(ink).truncate().shrink())
                .on_press(move |s: &mut App| flip(s, &at, i))
                .build(k)
                .shrink()
                .help(if here { "Click to send this point there again" } else { "Click to keep this point here" })
                .on_hover_change(move |s: &mut App, inside: bool, _: &mut rune::Tx| {
                    if inside {
                        s.split.hover = Some((over.clone(), i));
                    } else if s.split.hover.as_ref().is_some_and(|(k, j)| k == &over && *j == i) {
                        s.split.hover = None;
                    }
                }),
        );
    }
    out.push(r);
    out
}

/// Whether a message of TARGET's box that goes wholly elsewhere is sent there: from a thread he writes on (`Here`),
/// where its lines then stand at the foot, with nothing of it posted yet (N-788). Otherwise it is one message here.
fn wholly_sent(s: &App, target: &Target) -> bool {
    home(target).is_some() && !s.split.carry.contains_key(&(target.project(), target.place()))
}

/// The thread he writes on in TARGET's box, when it is one where the lines of a message sent wholly elsewhere stand.
fn home(target: &Target) -> Option<String> {
    match target {
        Target::Thread { t, way: Way::Here, .. } => Some(t.clone()),
        _ => None,
    }
}

/// When every point of P that goes elsewhere leaves nothing here: the sentence said in the chips' place, "All of this
/// is about N-575 · Runner rows. Send posts it here."
fn all_elsewhere(p: &Proposal) -> Option<String> {
    let (here, _) = cut(p)?;
    if here.is_some() {
        return None;
    }
    let mut places: Vec<String> = Vec::new();
    for (_, pt) in p.points.iter().enumerate().filter(|(i, pt)| pt.to != "here" && !p.kept_here.contains(i)) {
        let place = if pt.to == "new" { "a new thread".to_string() } else { place_said(pt).trim_start_matches("→ ").to_string() };
        if !places.contains(&place) {
            places.push(place);
        }
    }
    Some(format!("All about {}; Send posts here.", places.join(", ")))
}

/// His click on point I's chip in box KEY.
pub fn flip(s: &mut App, key: &(usize, String), i: usize) {
    if let Some(p) = s.split.proposals.get_mut(key)
        && !p.sent.contains(&i)
        && !p.kept_here.remove(&i)
    {
        p.kept_here.insert(i);
    }
}

/// What a send of box KEY makes of PROPOSAL: the text that stays here (none when all goes elsewhere) and each point
/// sent elsewhere (its words, its place). None: nothing goes elsewhere, it is one message.
pub fn cut(p: &Proposal) -> Option<(Option<String>, Vec<(String, String)>)> {
    let away: Vec<(usize, &Point)> = p.points.iter().enumerate().filter(|(i, pt)| pt.to != "here" && !p.kept_here.contains(i)).collect();
    if away.is_empty() {
        return None;
    }
    // what stays: the text with the points sent away taken out, every other line break and mark as he typed it;
    // only the ends and each seam where a point was are trimmed, a seam keeping at most one blank line (consult-4,
    // N-589.p26, 1)
    let mut parts = Vec::new();
    let mut at = 0;
    for (_, pt) in &away {
        let r = bytes(&p.text, pt.from, pt.len)?;
        parts.push(&p.text[at..r.start]);
        at = r.end;
    }
    parts.push(&p.text[at..]);
    let mut stay = String::new();
    for (j, part) in parts.into_iter().enumerate() {
        if j == 0 {
            stay = part.to_string();
            continue;
        }
        let gap: String = stay.chars().rev().take_while(|c| c.is_whitespace()).chain(part.chars().take_while(|c| c.is_whitespace())).collect();
        let mut lines = gap.matches('\n').count();
        // a list item sent away leaves its mark alone on a line at the seam: that line goes (consult-4 N-589.p31 b)
        let (mut left, mut right) = (stay.trim_end().to_string(), part.trim_start().to_string());
        while let Some(cut_at) = left.rfind('\n').map(|i| i + 1).or(Some(0)).filter(|i| bare_mark(&left[*i..])) {
            left.truncate(cut_at);
            left = left.trim_end().to_string();
            lines = lines.max(1);
        }
        while let Some(end) = Some(right.find('\n').unwrap_or(right.len())).filter(|e| bare_mark(&right[..*e])) {
            right = right[end..].trim_start().to_string();
            lines = lines.max(1);
        }
        // on one line, a mark on each side of the seam ("x, , y"): the one before stays; a full stop after wins
        if lines == 0
            && let Some(c) = right.chars().next().filter(|c| SEAM_MARKS.contains(*c) || *c == '.')
        {
            if c == '.' {
                if left.ends_with(|e: char| SEAM_MARKS.contains(e)) {
                    left.pop();
                }
            } else if (left.is_empty() && matches!(c, ',' | ';')) || left.ends_with(|e: char| SEAM_MARKS.contains(e) || e == '.') {
                right = right[c.len_utf8()..].trim_start().to_string();
            }
        }
        stay = if left.is_empty() || right.is_empty() {
            left + &right
        } else if right.starts_with('.') && lines == 0 {
            left + &right
        } else {
            left + if lines >= 2 { "\n\n" } else if lines == 1 { "\n" } else { " " } + &right
        };
    }
    // the ends: a line of a mark alone, and a comma or dash left hanging
    let mut stay = stay.trim().to_string();
    while let Some(end) = Some(stay.find('\n').unwrap_or(stay.len())).filter(|e| *e > 0 && bare_mark(&stay[..*e])) {
        stay = stay[end..].trim_start().to_string();
    }
    while let Some(i) = stay.rfind('\n').map(|i| i + 1).or(Some(0)).filter(|i| *i < stay.len() && bare_mark(&stay[*i..])) {
        stay.truncate(i);
        stay = stay.trim_end().to_string();
    }
    let stay = stay.trim_start_matches([',', ';']).trim_end_matches([',', ';', '-', '–', '—', '/', '|']).trim().to_string();
    let here = (!stay.is_empty()).then_some(stay);
    let sent = away.into_iter().map(|(_, pt)| (pt.words.trim().to_string(), pt.to.clone())).collect();
    Some((here, sent))
}

/// The marks coordd counts as a seam between points (coordd split.rs `seam`, but whitespace), less the full stop.
const SEAM_MARKS: &str = ",;:-–—/|";

/// Whether LINE holds only a list mark: `-`, `*`, `•`, `+`, or a number with `.` or `)`.
fn bare_mark(line: &str) -> bool {
    let l = line.trim();
    if l.is_empty() || l.contains('\n') {
        return false;
    }
    l.chars().all(|c| matches!(c, '-' | '*' | '•' | '+' | '–' | '—')) || (l.len() > 1 && l.ends_with(['.', ')']) && l[..l.len() - 1].chars().all(|c| c.is_ascii_digit()))
}

/// The request REQ (the box's own for where he writes) with SPLIT_FROM.
fn with_split(mut req: Request, sf: SplitFrom) -> Request {
    if let Request::Post { split_from, .. } | Request::Say { split_from, .. } = &mut req {
        *split_from = Some(sf);
    }
    req
}

/// The posts a send of TARGET's box makes with PROPOSAL, KEY the message's key: the box's own request with what stays
/// here (point 1), and each point elsewhere not posted yet, by its place in the proposal, numbered after the points a
/// stopped Send already posted ([`Splits::carry`]). With nothing staying here, from a thread he writes on and nothing
/// posted yet: no post here, and each point a plain post elsewhere (no `split_from`: a message of one place has no
/// point 1 where he wrote; its lines stand at the thread's foot instead, N-788). None: one message, as ever.
pub fn planned(s: &App, target: &Target, p: &Proposal, key: &str) -> Option<(Option<Request>, Vec<(usize, Request)>)> {
    let (here, _) = cut(p)?;
    let done = s.split.carry.get(&(target.project(), target.place())).map_or(0, |c| c.lines.len()) as u32;
    let away: Vec<(usize, &Point)> = p.points.iter().enumerate().filter(|(i, pt)| pt.to != "here" && !p.kept_here.contains(i)).collect();
    let req = crate::message_box::request(s, target).ok()?;
    let of = done + away.len() as u32 + 1;
    let split = here.is_some();
    let sf = |point: u32| split.then(|| SplitFrom { message: key.to_string(), point, of });
    if here.is_none() && !wholly_sent(s, target) {
        // all of it elsewhere with no thread of his for its lines: one message here, as ever (consult-4 N-589.p26, 2)
        return None;
    }
    let here = here.map(|h| with_split(crate::words::with_text(&req, &h, None), SplitFrom { message: key.to_string(), point: 1, of }));
    let posts = away
        .into_iter()
        .enumerate()
        .filter(|(_, (i, _))| !p.sent.contains(i))
        .map(|(n, (i, pt))| {
            let (words, point) = (pt.words.trim().to_string(), done + n as u32 + 2);
            let req = match pt.to.as_str() {
                "new" => Request::Say { to: None, names: Vec::new(), text: words, follows: None, wait: false, under: None, label: None, thread: true, typed: None, split_from: sf(point), comments: Vec::new() },
                t => Request::Post { kind: None, t: t.to_string(), text: words, to: None, answers: None, wait: false, also: Vec::new(), typed: None, split_from: sf(point), on: Vec::new(), comments: Vec::new() },
            };
            (i, req)
        })
        .collect();
    Some((here, posts))
}

/// The requests of [`planned`] in one list, the box's own first.
pub fn requests(s: &App, target: &Target, p: &Proposal, key: &str) -> Option<Vec<Request>> {
    let (here, posts) = planned(s, target, p, key)?;
    Some(here.into_iter().chain(posts.into_iter().map(|(_, r)| r)).collect())
}

/// Send the box of TARGET as its chips say, when a proposal fits its text and keeps a point elsewhere: true when it
/// went so. The points go first; the post here, which empties the box, only once every point is taken. A point
/// refused stops it there: the points that went leave the box and are said under it, so a second Send, or an edit
/// and a Send, posts only the rest, as the same message (consult-4, N-589.p26, a; N-788). A box with points already
/// posted and no proposal now sends what it holds as point 1 of that message.
pub fn send(s: &mut App, target: &Target) -> bool {
    let key = (target.project(), target.place());
    if s.split.sending.contains_key(&key) {
        return true;
    }
    let carry = s.split.carry.get(&key).cloned();
    let fits = proposal(s, &key).cloned().and_then(|p| {
        let msg = p.message.clone().or_else(|| carry.as_ref().map(|c| c.message.clone())).unwrap_or_else(|| format!("m-{}", chrono::Utc::now().timestamp_millis()));
        Some((planned(s, target, &p, &msg)?, p, msg))
    });
    let Some(((here, posts), mut p, msg)) = fits else {
        // no split now, but points of this message already went: what is in the box is its point 1
        let Some(c) = carry else { return false };
        let Ok(req) = crate::message_box::request(s, target) else { return false };
        let here = with_split(req, SplitFrom { message: c.message.clone(), point: 1, of: c.lines.len() as u32 + 1 });
        // the lines under his post are the points that went, and no point he has since kept here (consult-4 N-589.p43)
        s.split.sent.insert(c.message.clone(), c.lines.clone());
        s.box_said.remove(&key);
        send_here(s, &key, here);
        return true;
    };
    // the lines under his post: what went before, then what goes now
    if let Some((_, away)) = cut(&p) {
        let label = |to: &str| p.points.iter().find(|pt| pt.to == to).and_then(|pt| pt.label.clone()).unwrap_or_default();
        let mut all = carry.map(|c| c.lines).unwrap_or_default();
        all.extend(away.into_iter().map(|(words, to)| (to.clone(), label(&to), words)));
        if here.is_some() {
            s.split.sent.insert(msg.clone(), all);
        }
    }
    let home = if here.is_none() { home(target) } else { None };
    p.message = Some(msg);
    p.home = home.clone();
    s.split.proposals.insert(key.clone(), p);
    s.box_said.remove(&key);
    let mut out = BTreeSet::new();
    let mut failed = false;
    for (i, req) in posts {
        if !s.request_in(key.0, req, Pending::SplitPoint { key: key.clone(), i }) {
            let why = s.notice.take().unwrap_or_else(|| "not sent".into());
            s.box_said.insert(key.clone(), Err(why));
            failed = true;
            break;
        }
        out.insert(i);
    }
    if out.is_empty() {
        if !failed && let Some(here) = here {
            send_here(s, &key, here);
        }
        return true;
    }
    s.split.sending.insert(key, Going { here, out, failed, home });
    true
}

/// The said line of a box whose points were on their way when the link dropped.
const DROPPED: &str = "Link dropped; check before resending.";

/// Project PI's link went down: its boxes' points on their way are no longer waited for (the window drops their
/// requests), so Send in those boxes works again; each says once, faint, that where they went is not known. The
/// proposal and its "· sent" marks stay (consult-4 N-589.p37; paper-research).
pub fn down(sp: &mut Splits, said: &mut HashMap<(usize, String), Result<String, String>>, pi: usize) {
    // (the points that went leave the box at the next step: `poll`, [`rest`])
    let keys: Vec<_> = sp.sending.keys().filter(|k| k.0 == pi).cloned().collect();
    for k in keys {
        sp.sending.remove(&k);
        said.insert(k, Ok(DROPPED.to_string()));
    }
}

/// The split asked for box KEY's TEXT went with the pipe (lib.rs `Incoming::Down`, proto-reader N-1446 p251): the box
/// is as if never asked, so `poll` asks again after its pause.
pub fn lost(sp: &mut Splits, key: &(usize, String), text: &str) {
    if sp.asked.get(key).is_some_and(|t| t == text) {
        sp.asked.remove(key);
    }
}

/// The post that stays here, sent last: the box empties when coordd takes it, its answer under the box.
fn send_here(s: &mut App, key: &(usize, String), here: Request) {
    if !s.request_in(key.0, here, Pending::TicketDone { clear: crate::threads::Cleared::Box(key.0, key.1.clone()) }) {
        let why = s.notice.take().unwrap_or_else(|| "not sent".into());
        s.box_said.insert(key.clone(), Err(why));
    }
}

/// coordd's answer to point I of box KEY's send: taken, it is marked sent (and, for a message sent wholly elsewhere,
/// its line stands at the foot of his thread); refused, its words are said under the box. Once every point is
/// answered: none refused, the post here goes (none: the box empties); one refused, the points that went leave the box.
pub fn point_replied(s: &mut App, key: (usize, String), i: usize, ok: bool, out: String) {
    let Some(entry) = s.split.sending.get_mut(&key) else { return };
    entry.out.remove(&i);
    if ok {
        let home = entry.home.clone();
        if let Some(p) = s.split.proposals.get_mut(&key) {
            p.sent.insert(i);
            if let (Some(home), Some(pt)) = (home, p.points.get(i)) {
                s.split.left.entry((key.0, home)).or_default().push((pt.to.clone(), pt.label.clone().unwrap_or_default(), pt.words.trim().to_string()));
            }
        }
    } else {
        entry.failed = true;
        s.box_said.insert(key.clone(), Err(out.trim().to_string()));
    }
    if !entry.out.is_empty() {
        return;
    }
    let Some(going) = s.split.sending.remove(&key) else { return };
    match (going.failed, going.here) {
        (false, Some(here)) => send_here(s, &key, here),
        (false, None) => {
            // all of it went elsewhere: the box empties as a sent box does
            s.split.proposals.remove(&key);
            s.composers.remove(&key);
            crate::changed::sent(s, &key);
            s.writing_changed(key.0, &key.1);
        }
        (true, _) => rest(s, &key),
    }
}

/// Box KEY after a Send that stopped with some points posted (a point refused, the link dropped): those points' words
/// leave the box, which keeps the rest as he typed it ([`cut`]'s seams); they are carried as posted, said under the
/// box, and the next Send is the same message. The points left are found again in the text left, so the chips stay
/// with his clicks; when one cannot be found the proposal goes and the rest sends as point 1 (N-788).
fn rest(s: &mut App, key: &(usize, String)) {
    let Some(p) = s.split.proposals.get(key).cloned() else { return };
    if p.sent.is_empty() || s.split.sending.contains_key(key) || s.composers.get(key).map(|e| e.text.as_str()) != Some(p.text.as_str()) {
        return;
    }
    let msg = p.message.clone().unwrap_or_default();
    // a message sent wholly elsewhere: its points taken stand at the thread's foot already, and were no points of a
    // split; the rest goes as a message of its own
    if p.home.is_none() {
        let c = s.split.carry.entry(key.clone()).or_insert_with(|| Carry { message: msg.clone(), lines: Vec::new() });
        for pt in p.sent.iter().filter_map(|i| p.points.get(*i)) {
            c.lines.push((pt.to.clone(), pt.label.clone().unwrap_or_default(), pt.words.trim().to_string()));
        }
    }
    let only_sent = Proposal { kept_here: (0..p.points.len()).filter(|i| !p.sent.contains(i)).collect(), sent: BTreeSet::new(), ..p.clone() };
    let left = cut(&only_sent).and_then(|(here, _)| here).unwrap_or_default();
    // the points not posted, found in order in what is left
    let mut points = Vec::new();
    let mut kept_here = BTreeSet::new();
    let mut at = 0;
    let mut found = true;
    for (i, pt) in p.points.iter().enumerate().filter(|(i, _)| !p.sent.contains(i)) {
        let w = pt.words.trim();
        let Some(b) = left.get(at..).and_then(|x| x.find(w)).map(|b| b + at) else {
            found = false;
            break;
        };
        if p.kept_here.contains(&i) {
            kept_here.insert(points.len());
        }
        points.push(Point { words: w.to_string(), from: left[..b].chars().count(), len: w.chars().count(), to: pt.to.clone(), label: pt.label.clone() });
        at = b + w.len();
    }
    s.composers.insert(key.clone(), rune::TextEdit::new(&left));
    s.split.asked.insert(key.clone(), left.clone());
    if found && !points.is_empty() {
        let message = p.home.is_none().then_some(msg);
        s.split.proposals.insert(key.clone(), Proposal { text: left, points, kept_here, sent: BTreeSet::new(), message, home: None });
    } else {
        s.split.proposals.remove(key);
    }
}

/// Under his post POST of V in project P: one faint line per point of the same message on another thread, `→ N-575
/// Runner rows: its words`, with `Bring back` (the move to V). Under the message's first point only, where he wrote.
pub fn lines(s: &App, p: &crate::Proj, v: &docket_machine::TicketView, post: &docket_machine::Post, k: &Kit) -> Vec<El> {
    let Some(sf) = post.split_from.as_ref().filter(|sf| sf.point == 1) else { return Vec::new() };
    let Some(pi) = s.projects.iter().position(|q| std::ptr::eq(q, p)) else { return Vec::new() };
    let found = elsewhere(s, p, &v.id, &sf.message);
    let mut out: Vec<El> = found.iter().map(|(t, label, n, words)| line(pi, &v.id, &sf.message, t, label, Some(*n), words, k)).collect();
    out.extend(not_yet(s, &found, &sf.message).into_iter().map(|(t, label, words)| line(pi, &v.id, &sf.message, &t, &label, None, &words, k)));
    out
}

/// The points of MESSAGE this window sent elsewhere whose posts are not in the view yet (FOUND: those that are).
fn not_yet(s: &App, found: &[(String, String, u32, String)], message: &str) -> Vec<(String, String, String)> {
    let Some(sent) = s.split.sent.get(message) else { return Vec::new() };
    sent.iter().filter(|(_, _, w)| !found.iter().any(|f| f.3.trim() == w.trim())).cloned().collect()
}

/// The points of MESSAGE on threads other than HOME, not moved or taken back: each thread, its label, the post, its
/// words.
fn elsewhere(s: &App, p: &crate::Proj, home: &str, message: &str) -> Vec<(String, String, u32, String)> {
    let mut out = Vec::new();
    for w in p.live.tickets.values().filter(|w| w.id != home) {
        for q in w.posts.iter().filter(|q| q.moved.is_none() && !q.taken_back && q.split_from.as_ref().is_some_and(|x| x.message == message)) {
            out.push((w.id.clone(), w.title.lines().next().unwrap_or("").trim().to_string(), q.n, crate::threads::words(&s.tickets, q)));
        }
    }
    out
}

/// A line's words: `→ N-575 · Runner rows: runner rows: yes, do them.`
fn said(t: &str, label: &str, words: &str) -> String {
    if t == "new" {
        return format!("→ new thread: {}", words.trim());
    }
    format!("→ {t}{}: {}", if label.is_empty() { String::new() } else { format!(" · {label}") }, words.trim())
}

/// One line under his post on thread HOME: a point of MESSAGE, WORDS, on thread T (its label LABEL) as its post N;
/// with no post in the view yet the line stands without `Bring back`.
#[allow(clippy::too_many_arguments)]
fn line(pi: usize, home: &str, message: &str, t: &str, label: &str, n: Option<u32>, words: &str, k: &Kit) -> El {
    let said = said(t, label, words);
    let r = row()
        .key(format!("split-line-{home}-{t}-{}", n.map_or_else(|| words.trim().to_string(), |n| n.to_string())))
        .w_full()
        .gap(6.0)
        .center_y()
        .child(text(said).font(k.ty.small).color(k.c.text_faint).truncate().shrink());
    let Some(n) = n else { return r };
    let (t, to, message, words) = (t.to_string(), home.to_string(), message.to_string(), words.trim().to_string());
    r.child(Button::new("Bring back").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| bring_back(s, pi, &t, n, &to, &message, &words)).build(k))
}

/// His `Bring back`: post N of T moved to TO, where he wrote it (the existing move); the point is no longer one sent
/// elsewhere (MESSAGE, its WORDS).
fn bring_back(s: &mut App, pi: usize, t: &str, n: u32, to: &str, message: &str, words: &str) {
    if let Some(v) = s.split.sent.get_mut(message) {
        v.retain(|(_, _, w)| w.trim() != words);
    }
    let _ = s.request_in(pi, bring_back_request(t, n, to), Pending::TicketDone { clear: crate::threads::Cleared::Act });
}

/// At the foot of thread V of project P, above its box: a faint line per point of a message he wrote here that went
/// wholly elsewhere, `→ N-575 · Runner rows: its words`, with `Bring back` once its post is in the view, and × to
/// let it go (N-788: the wholly-elsewhere case with a place of its own for the line).
pub fn foot(s: &App, p: &crate::Proj, v: &docket_machine::TicketView, k: &Kit) -> Vec<El> {
    let Some(pi) = s.projects.iter().position(|q| std::ptr::eq(q, p)) else { return Vec::new() };
    let Some(lines) = s.split.left.get(&(pi, v.id.clone())) else { return Vec::new() };
    lines
        .iter()
        .enumerate()
        .map(|(i, (t, label, words))| {
            let found = posted(s, p, &v.id, t, words);
            let (home, words_at, gone) = (v.id.clone(), words.clone(), words.clone());
            let mut r = row()
                .key(format!("split-left-{}-{i}", v.id))
                .w_full()
                .gap(6.0)
                .center_y()
                .child(text(said(found.as_ref().map_or(t.as_str(), |f| f.0.as_str()), label, words)).font(k.ty.small).color(k.c.text_faint).truncate().shrink());
            if let Some((at, n)) = found {
                let to = home.clone();
                r = r.child(
                    Button::new("Bring back")
                        .variant(ButtonVariant::Ghost)
                        .on_press(move |s: &mut App| {
                            let _ = s.request_in(pi, bring_back_request(&at, n, &to), Pending::TicketDone { clear: crate::threads::Cleared::Act });
                            let_go(s, pi, &to, &words_at);
                        })
                        .build(k),
                );
            }
            r.child(Button::new("×").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| let_go(s, pi, &home, &gone)).build(k).help("Let this line go"))
        })
        .collect()
}

/// The line of WORDS at the foot of thread HOME goes.
fn let_go(s: &mut App, pi: usize, home: &str, words: &str) {
    if let Some(l) = s.split.left.get_mut(&(pi, home.to_string())) {
        if let Some(i) = l.iter().position(|(_, _, w)| w == words) {
            l.remove(i);
        }
        if l.is_empty() {
            s.split.left.remove(&(pi, home.to_string()));
        }
    }
}

/// The post of his holding WORDS sent to T (a thread id, or `new`: the first post of a thread other than HOME), as it
/// stands in the view now: its thread and number; the latest such.
fn posted(s: &App, p: &crate::Proj, home: &str, t: &str, words: &str) -> Option<(String, u32)> {
    let his = |q: &docket_machine::Post| q.by == "owner" && q.moved.is_none() && !q.taken_back && crate::threads::words(&s.tickets, q).trim() == words.trim();
    if t == "new" {
        return p.live.tickets.values().filter(|w| w.id != home && w.posts.first().is_some_and(&his)).max_by_key(|w| w.n).map(|w| (w.id.clone(), w.posts[0].n));
    }
    let w = p.live.tickets.values().find(|w| w.id == t)?;
    w.posts.iter().rev().find(|&q| his(q)).map(|q| (w.id.clone(), q.n))
}

/// The move `Bring back` asks: post N of T to TO.
fn bring_back_request(t: &str, n: u32, to: &str) -> Request {
    Request::Move { t: t.to_string(), n, to: Some(to.to_string()) }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> Proposal {
        let text = "agda-dev is still not in the window. runner rows: yes, do them. autostart: yes".to_string();
        let points = serde_json::from_value::<Vec<Point>>(serde_json::json!([
            {"words":"agda-dev is still not in the window.","from":0,"len":36,"to":"here","label":null},
            {"words":"runner rows: yes, do them.","from":37,"len":26,"to":"N-575","label":"Runner rows"},
            {"words":"autostart: yes","from":64,"len":14,"to":"here","label":null}
        ]))
        .unwrap();
        Proposal { text, points, kept_here: BTreeSet::new(), ..Default::default() }
    }

    /// The chips from a `Points` answer: one per point that goes elsewhere, said by id and label; a point for here has
    /// none; its underline covers exactly its words.
    #[test]
    fn a_points_answer_gives_one_chip_per_point_elsewhere_by_id_and_label() {
        let p = example();
        let away: Vec<String> = p.points.iter().filter(|pt| pt.to != "here").map(place_said).collect();
        assert_eq!(away, ["→ N-575 · Runner rows"]);
        assert_eq!(place_said(&Point { words: "x".into(), from: 0, len: 1, to: "new".into(), label: None }), "→ new thread");
        let r = bytes(&p.text, 37, 26).unwrap();
        assert_eq!(&p.text[r], "runner rows: yes, do them.");
        // characters, not bytes
        assert_eq!(bytes("é runner", 2, 6).map(|r| &"é runner"[r]), Some("runner"));
    }

    /// Send with the proposal: what stays here as one text, each point kept elsewhere its own; a click keeps a point
    /// here (nothing elsewhere: one message), a second sends it there again.
    #[test]
    fn send_cuts_what_the_chips_say_and_a_click_keeps_a_point_here() {
        let mut p = example();
        let (here, away) = cut(&p).unwrap();
        assert_eq!(here.as_deref(), Some("agda-dev is still not in the window. autostart: yes"));
        assert_eq!(away, [("runner rows: yes, do them.".to_string(), "N-575".to_string())]);
        // his line breaks and marks stay as he typed them; only the seam is trimmed
        let list = "for the runner:\n- rows first\n- then the chart\n\nautostart: yes";
        let q = Proposal { text: list.into(), points: vec![Point { words: "autostart: yes".into(), from: 47, len: 14, to: "N-575".into(), label: None }], kept_here: BTreeSet::new(), ..Default::default() };
        assert_eq!(&list[bytes(list, 47, 14).unwrap()], "autostart: yes");
        assert_eq!(cut(&q).unwrap().0.as_deref(), Some("for the runner:\n- rows first\n- then the chart"));
        let mid = "rows first.\n\nautostart: yes\n\nthen the chart, please";
        let q = Proposal { text: mid.into(), points: vec![Point { words: "autostart: yes".into(), from: 13, len: 14, to: "N-575".into(), label: None }], kept_here: BTreeSet::new(), ..Default::default() };
        assert_eq!(cut(&q).unwrap().0.as_deref(), Some("rows first.\n\nthen the chart, please"));
        // a list item sent away takes its mark with it, in the middle and at the end; a doubled comma is one (N-788)
        let one = |text: &str, words: &str| {
            let from = text[..text.find(words).unwrap()].chars().count();
            let q = Proposal { text: text.into(), points: vec![Point { words: words.into(), from, len: words.chars().count(), to: "N-575".into(), label: None }], ..Default::default() };
            cut(&q).unwrap().0
        };
        let items = "- rows first\n- autostart: yes\n- then the chart";
        assert_eq!(one(items, "autostart: yes").as_deref(), Some("- rows first\n- then the chart"));
        assert_eq!(one(items, "then the chart").as_deref(), Some("- rows first\n- autostart: yes"));
        assert_eq!(one(items, "rows first").as_deref(), Some("- autostart: yes\n- then the chart"));
        assert_eq!(one("1. rows first\n2. autostart: yes\n3. then the chart", "autostart: yes").as_deref(), Some("1. rows first\n3. then the chart"));
        assert_eq!(one("rows first, autostart: yes, then the chart", "autostart: yes").as_deref(), Some("rows first, then the chart"));
        assert_eq!(one("rows first, autostart: yes. Then the chart", "autostart: yes").as_deref(), Some("rows first. Then the chart"));
        assert_eq!(one("rows first - autostart: yes - then the chart", "autostart: yes").as_deref(), Some("rows first - then the chart"));
        assert_eq!(one("for the runner:\n- rows first\n- autostart: yes", "rows first").as_deref(), Some("for the runner:\n- autostart: yes"), "a list's own marks stay");
        p.kept_here.insert(1);
        assert_eq!(cut(&p), None, "no chip left: one message");
        p.kept_here.remove(&1);
        assert!(cut(&p).is_some());
    }

    /// The requests: the box's own with what stays, then the point on its thread, each with split_from (point i of
    /// k, one message key); a point for a new thread is a thread of its own.
    #[test]
    fn each_post_carries_split_from_and_a_new_point_is_a_thread_of_its_own() {
        let away = Request::Post { kind: None, t: "N-575".into(), text: "x".into(), to: None, answers: None, wait: false, also: Vec::new(), typed: None, split_from: None, on: Vec::new(), comments: Vec::new() };
        let sf = SplitFrom { message: "m-1".into(), point: 2, of: 2 };
        assert!(matches!(with_split(away, sf.clone()), Request::Post { split_from: Some(x), .. } if x == sf));
        let mut p = example();
        p.points[1].to = "new".into();
        let (_, away) = cut(&p).unwrap();
        assert_eq!(away[0].1, "new");
    }

    /// A text changed after the proposal: the proposal no longer fits, and the message goes as one.
    #[test]
    fn a_text_changed_after_the_proposal_sends_as_one() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.split.on = true;
        let key = (0usize, "thread:N-687".to_string());
        let p = example();
        s.composers.insert(key.clone(), rune::TextEdit::new(&p.text));
        s.split.proposals.insert(key.clone(), p.clone());
        assert!(proposal(&s, &key).is_some());
        s.composers.insert(key.clone(), rune::TextEdit::new(&format!("{} and more", p.text)));
        assert!(proposal(&s, &key).is_none(), "changed: one message");
        // an answer for an older text shows nothing
        replied(&mut s, key.clone(), p.text.clone(), true, r#"{"head":"Points","points":[{"words":"a","from":0,"len":1,"to":"N-1","label":null}]}"#.into());
        assert!(proposal(&s, &key).is_none());
        // One shows nothing
        replied(&mut s, key.clone(), format!("{} and more", p.text), true, r#"{"head":"One"}"#.into());
        assert!(proposal(&s, &key).is_none());
        // a Points answer for the text in the box: its proposal, the point for N-575 underlined, nothing kept here
        let now = format!("{} and more", p.text);
        let answer = serde_json::json!({"head": "Points", "points": [
            {"words": "agda-dev is still not in the window.", "from": 0, "len": 36, "to": "here", "label": null},
            {"words": "runner rows: yes, do them. autostart: yes and more", "from": 37, "len": 50, "to": "N-575", "label": "Runner rows"}]});
        replied(&mut s, key.clone(), now.clone(), true, answer.to_string());
        let got = proposal(&s, &key).expect("a proposal");
        assert_eq!((got.points.len(), got.kept_here.len()), (2, 0));
        assert!(spans(&s, &key, rune::Color::BLACK).is_empty(), "no line under a point at rest");
        s.split.hover = Some((key.clone(), 1));
        let tinted = spans(&s, &key, rune::Color::BLACK);
        assert_eq!((tinted.len(), tinted[0].underline, tinted[0].bg.is_some()), (1, None, true), "the point tinted while its chip is under the pointer");
        s.split.hover = None;
        // One shows nothing
        replied(&mut s, key.clone(), now, true, r#"{"head":"One","points":[]}"#.into());
        assert!(proposal(&s, &key).is_none());
    }

    /// The made-up project with his box on N-1 holding the example, and a proposal for it sending the second point
    /// to N-2.
    fn scene() -> (std::sync::MutexGuard<'static, ()>, App, Target) {
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::view_cost::project_for_tests();
        p.protocol = coord_types::SPLIT_PROTOCOL.max(coord_types::ASKS_CLOSED_PROTOCOL);
        s.projects = vec![p];
        s.cur = 0;
        s.split.on = true;
        let target = Target::Thread { project: 0, t: "N-1".into(), post: None, way: Way::Here };
        let key = (0usize, target.place());
        let mut pr = example();
        pr.points[1].to = "N-2".into();
        s.composers.insert(key.clone(), rune::TextEdit::new(&pr.text));
        s.split.proposals.insert(key, pr);
        (held, s, target)
    }

    /// Send with a proposal: what stays here on N-1 as point 1 of 2, the point on N-2 as point 2 of 2, one message
    /// key; without one (a click kept it here), the box's own one request, no split_from.
    #[test]
    fn send_with_a_proposal_makes_one_post_per_place_and_without_one_the_message_goes_as_one() {
        let (_held, mut s, target) = scene();
        let key = (0usize, target.place());
        let p = proposal(&s, &key).cloned().unwrap();
        let reqs = requests(&s, &target, &p, "m-1").unwrap();
        assert_eq!(reqs.len(), 2);
        let Request::Post { t, text, split_from: Some(a), typed: None, .. } = &reqs[0] else { panic!("{:?}", reqs[0]) };
        assert_eq!((t.as_str(), text.as_str(), a.point, a.of, a.message.as_str()), ("N-1", "agda-dev is still not in the window. autostart: yes", 1, 2, "m-1"));
        let Request::Post { t, text, split_from: Some(b), .. } = &reqs[1] else { panic!("{:?}", reqs[1]) };
        assert_eq!((t.as_str(), text.as_str(), b.point, b.of, b.message.as_str()), ("N-2", "runner rows: yes, do them.", 2, 2, "m-1"));
        // all of it elsewhere: one message here, as ever (nowhere for its lines to stand)
        let all = Proposal { text: p.text.clone(), points: vec![Point { words: p.text.clone(), from: 0, len: p.text.chars().count(), to: "N-2".into(), label: None }], kept_here: BTreeSet::new(), ..Default::default() };
        assert!(cut(&all).is_some_and(|(here, _)| here.is_none()));
        let mut said = all.clone();
        said.points[0].label = Some("thread 2".into());
        assert_eq!(all_elsewhere(&said).as_deref(), Some("All about N-2 · thread 2; Send posts here."), "no chip promising otherwise");
        assert_eq!(all_elsewhere(&p), None, "with something here, the chips");
        let mut new = all.clone();
        new.points[0].to = "new".into();
        assert_eq!(all_elsewhere(&new).as_deref(), Some("All about a new thread; Send posts here."));
        // from a thread he writes on, all of it goes there, a plain post: no point 1 here for a split (N-788)
        assert!(wholly_sent(&s, &target));
        let reqs = requests(&s, &target, &all, "m-1").unwrap();
        assert!(matches!(&reqs[..], [Request::Post { t, text, split_from: None, .. }] if t == "N-2" && *text == p.text), "{reqs:?}");
        // from a new thread's box there is no thread of his for the line: one message, as the sentence says
        let fresh = Target::NewThread { project: 0 };
        assert!(!wholly_sent(&s, &fresh) && requests(&s, &fresh, &all, "m-1").is_none());
        flip(&mut s, &key, 1);
        let p = proposal(&s, &key).cloned().unwrap();
        assert!(requests(&s, &target, &p, "m-1").is_none(), "nothing elsewhere: one message");
        assert!(!send(&mut s, &target), "the box's own send takes it");
        assert!(matches!(crate::message_box::request(&s, &target), Ok(Request::Post { split_from: None, .. })));
        // with the switch off, no proposal shows and the box sends as ever
        flip(&mut s, &key, 1);
        s.split.on = false;
        assert!(proposal(&s, &key).is_none() && spans(&s, &key, rune::Color::BLACK).is_empty());
    }

    /// The line under his post: the point on N-2 said with its thread's label and words, and Bring back asks the move
    /// of that post to N-1; a post moved back is no longer listed.
    #[test]
    fn under_his_post_one_line_per_point_elsewhere_with_bring_back() {
        let (_held, mut s, _) = scene();
        let sf = |point| Some(SplitFrom { message: "m-1".into(), point, of: 2 });
        let p = &mut s.projects[0];
        let n2 = p.live.tickets.values().find(|v| v.id == "N-2").unwrap().n;
        let n1 = p.live.tickets.values().find(|v| v.id == "N-3").unwrap().n;
        p.live.tickets.get_mut(&n1).unwrap().posts[0].split_from = sf(1);
        p.live.tickets.get_mut(&n2).unwrap().posts[1].split_from = sf(2);
        let p = &s.projects[0];
        let found = elsewhere(&s, p, "N-3", "m-1");
        assert_eq!(found.len(), 1);
        let (t, label, n, words) = &found[0];
        assert_eq!((t.as_str(), label.as_str(), *n), ("N-2", "thread 2", 2));
        assert!(said(t, label, words).starts_with("→ N-2 · thread 2: post 2 of N-2"));
        assert_eq!(said("N-575", "Runner rows", "runner rows: yes, do them."), "→ N-575 · Runner rows: runner rows: yes, do them.");
        assert!(matches!(bring_back_request(t, *n, "N-3"), Request::Move { t, n: 2, to: Some(to) } if t == "N-2" && to == "N-3"));
        // a point sent from this window whose post is not in the view yet: its line, without Bring back
        s.split.sent.insert("m-1".into(), vec![("N-2".into(), "thread 2".into(), words.clone()), ("new".into(), String::new(), "autostart: yes".into())]);
        let waiting = not_yet(&s, &found, "m-1");
        assert_eq!(waiting, [("new".to_string(), String::new(), "autostart: yes".to_string())]);
        assert_eq!(said("new", "", "autostart: yes"), "→ new thread: autostart: yes");
        let p = &mut s.projects[0];
        p.live.tickets.get_mut(&n2).unwrap().posts[1].moved = Some("N-3".into());
        assert!(elsewhere(&s, &s.projects[0], "N-3", "m-1").is_empty());
    }

    /// A refusal (a project that cannot split, or no model answering): the switch goes off and says so under the
    /// box once, in the window's words; an answer of one point leaves it on and shows nothing.
    #[test]
    fn a_refusal_turns_the_switch_off_and_says_so_under_the_box() {
        let (_held, mut s, target) = scene();
        let key = (0usize, target.place());
        let text = s.composers[&key].text.clone();
        replied(&mut s, key.clone(), text.clone(), true, r#"{"head":"One"}"#.into());
        assert!(s.split.on && !s.box_said.contains_key(&key));
        replied(&mut s, key.clone(), text, false, "not split: the policy has no decision model-for for task split".into());
        assert!(!s.split.on && proposal(&s, &key).is_none());
        // no tray icon here (as on a machine without one): the switch stands beside Spelling, and the sentence says so
        let on_n3 = Target::Thread { project: 0, t: "N-3".into(), post: None, way: Way::Here };
        assert!(!crate::tray::shown() && switch_here(&s, &on_n3));
        assert_eq!(s.box_said.get(&key), Some(&Ok("Splitting failed for research; off. Turn it on again beside Spelling.".to_string())));
        assert!(!s.box_said[&key].as_ref().unwrap().contains("coord"), "no engine words, no command");
    }

    /// A point refused after another went: the post here does not go, the box keeps its text and the proposal with
    /// the point that went marked sent; a second Send posts only the rest, under the same message and numbers.
    #[test]
    fn a_refused_point_keeps_the_proposal_and_a_second_send_posts_only_the_rest() {
        let (_held, mut s, target) = scene();
        let key = (0usize, target.place());
        let mut p = proposal(&s, &key).cloned().unwrap();
        p.points[2].to = "N-4".into();
        p.message = Some("m-1".into());
        s.split.proposals.insert(key.clone(), p.clone());
        let (here, posts) = planned(&s, &target, &p, "m-1").unwrap();
        assert_eq!(posts.iter().map(|(i, _)| *i).collect::<Vec<_>>(), [1, 2]);
        s.split.sending.insert(key.clone(), Going { here, out: posts.iter().map(|(i, _)| *i).collect(), failed: false, home: None });
        point_replied(&mut s, key.clone(), 1, true, "Posted N-2.p3.".into());
        assert!(s.split.sending.contains_key(&key), "the post here waits for every point");
        point_replied(&mut s, key.clone(), 2, false, "not posted: N-4 is closed".into());
        assert!(s.split.sending.is_empty());
        assert_eq!(s.box_said.get(&key), Some(&Err("not posted: N-4 is closed".to_string())));
        // the point that went leaves the box, which keeps the rest; it is said under the box as sent (N-788)
        let left = "agda-dev is still not in the window. autostart: yes";
        assert_eq!(s.composers.get(&key).map(|e| e.text.clone()).as_deref(), Some(left), "the box keeps the rest of his text");
        assert_eq!(s.split.carry.get(&key).map(|c| (c.message.as_str(), c.lines.clone())), Some(("m-1", vec![("N-2".to_string(), "Runner rows".to_string(), "runner rows: yes, do them.".to_string())])));
        let p = proposal(&s, &key).cloned().expect("the chips for the rest stay");
        assert!(p.sent.is_empty() && p.points.iter().any(|pt| pt.to == "N-4" && pt.words == "autostart: yes"));
        // a second Send posts only the rest, under the same message, numbered after the point that went
        let (here, posts) = planned(&s, &target, &p, "m-1").unwrap();
        assert_eq!(posts.len(), 1);
        assert!(matches!(&posts[0], (_, Request::Post { t, split_from: Some(sf), .. }) if t == "N-4" && sf.point == 3 && sf.of == 3 && sf.message == "m-1"));
        assert!(matches!(&here, Some(Request::Post { text, split_from: Some(sf), .. }) if text == "agda-dev is still not in the window." && sf.point == 1 && sf.of == 3));
        // he edits what is left: the proposal no longer fits, the sent point is still said, and the box goes as point 1
        s.composers.insert(key.clone(), rune::TextEdit::new("agda-dev is still not in the window, and autostart: yes"));
        assert!(proposal(&s, &key).is_none() && s.split.carry.contains_key(&key));
        let msg = s.split.carry[&key].message.clone();
        let carry = s.split.carry[&key].clone();
        let req = crate::message_box::request(&s, &target).ok().map(|r| with_split(r, SplitFrom { message: msg, point: 1, of: carry.lines.len() as u32 + 1 }));
        assert!(matches!(req, Some(Request::Post { text, split_from: Some(sf), .. }) if text.starts_with("agda-dev") && !text.contains("runner rows") && sf.point == 1 && sf.of == 2));
        // the first Send's lines named both points; N-4's never went, so no line under his post may name it
        s.split.sent.insert("m-1".into(), vec![carry.lines[0].clone(), ("N-4".into(), String::new(), "autostart: yes".into())]);
        assert!(send(&mut s, &target), "the box's send goes through the split, as point 1");
        assert_eq!(s.split.sent.get("m-1"), Some(&carry.lines), "only the point that went has a line under his post");
    }

    /// The split asked for his box's words goes with the pipe (proto-reader, N-1446 p251): the box is as if never asked;
    /// a box asked for other words since keeps its ask. While the pipe stays down past the pause (an install's
    /// restart), nothing is asked or marked asked and no "not sent" shows (p284); once it is back, the box waits its
    /// pause again and is asked once.
    #[test]
    fn a_split_the_pipes_end_loses_is_asked_again() {
        let (_held, mut s, target) = scene();
        let key = (0usize, target.place());
        let other = (0usize, "elsewhere".to_string());
        let words = s.composers[&key].text.clone();
        let asked = |s: &App| s.projects[0].pending.values().filter(|w| matches!(w, crate::Pending::Split { .. })).count();
        let mut cat = std::process::Command::new("cat").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(cat.stdin.take().unwrap()));
        s.split.boxes.insert(key.clone(), target.clone());
        s.split.asked.insert(key.clone(), words.clone());
        s.split.asked.insert(other.clone(), "newer words".into());
        s.projects[0].pending.insert(800, crate::Pending::Split { key: key.clone(), text: words.clone() });
        s.projects[0].pending.insert(801, crate::Pending::Split { key: other.clone(), text: "older words".into() });
        s.take(0, crate::link::Incoming::Down("the pipe ended (1)".into()));
        assert!(!s.split.asked.contains_key(&key), "as if never asked");
        assert_eq!(s.split.asked.get(&other).map(String::as_str), Some("newer words"));
        // the pipe still down past the pause: nothing asked, nothing marked asked, no notice
        let mut tx = rune::Tx::default();
        let past = || Instant::now().checked_sub(crate::changed::PAUSE).unwrap();
        s.notice = None;
        s.split.waiting.insert(key.clone(), (target.clone(), words.clone(), past()));
        poll(&mut s, &mut tx);
        assert_eq!((asked(&s), s.split.asked.contains_key(&key), s.notice.clone()), (0, false, None));
        // the pipe back (coordd's first event clears `down`): the box waits its pause again, then is asked once
        s.projects[0].down = None;
        poll(&mut s, &mut tx);
        assert!(s.split.waiting.contains_key(&key) && asked(&s) == 0, "waiting again");
        s.split.waiting.get_mut(&key).unwrap().2 = past();
        poll(&mut s, &mut tx);
        assert_eq!((asked(&s), s.split.asked.get(&key)), (1, Some(&words)));
        s.projects[0].link = None;
        let _ = cat.wait();
    }

    /// The link drops while a point is on its way: the box is no longer waiting, says so once, keeps the proposal and
    /// its "· sent" marks, and a second Send is not swallowed (it reaches its request: here, no pipe, so "not sent").
    #[test]
    fn a_link_dropped_while_points_go_leaves_send_working() {
        let (_held, mut s, target) = scene();
        let key = (0usize, target.place());
        let mut p = proposal(&s, &key).cloned().unwrap();
        p.points[2].to = "N-4".into();
        p.message = Some("m-1".into());
        s.split.proposals.insert(key.clone(), p.clone());
        let (here, posts) = planned(&s, &target, &p, "m-1").unwrap();
        s.split.sending.insert(key.clone(), Going { here, out: posts.iter().map(|(i, _)| *i).collect(), failed: false, home: None });
        point_replied(&mut s, key.clone(), 1, true, "Posted N-2.p3.".into());
        assert!(send(&mut s, &target) && s.box_said.get(&key).is_none(), "while a point is on its way, Send waits");
        s.take(0, crate::link::Incoming::Down("the pipe ended".into()));
        assert!(s.split.sending.is_empty());
        assert_eq!(s.box_said.get(&key), Some(&Ok(DROPPED.to_string())));
        assert_eq!(proposal(&s, &key).map(|p| p.sent.clone()), Some(BTreeSet::from([1])));
        // the protocol is 0 after a Down; a coordd back with its hello
        s.projects[0].protocol = coord_types::SPLIT_PROTOCOL.max(coord_types::ASKS_CLOSED_PROTOCOL);
        assert!(send(&mut s, &target));
        assert!(matches!(s.box_said.get(&key), Some(Err(_))), "the second Send reached its request: {:?}", s.box_said.get(&key));
        // at the window's next step the point that went leaves the box (poll's `rest`)
        rest(&mut s, &key);
        assert!(!s.composers[&key].text.contains("runner rows") && s.split.carry[&key].lines.len() == 1);
    }

    /// All of it about another thread, written on a thread: it goes there as a plain post, the box empties, and a
    /// line stands at the foot of the thread he wrote on, with Bring back once the post is in the view, and × (N-788).
    #[test]
    fn a_message_wholly_elsewhere_goes_there_and_leaves_a_line_at_the_foot() {
        let (_held, mut s, target) = scene();
        let key = (0usize, target.place());
        let text = s.composers[&key].text.clone();
        let all = Proposal { text: text.clone(), points: vec![Point { words: text.clone(), from: 0, len: text.chars().count(), to: "N-2".into(), label: Some("thread 2".into()) }], ..Default::default() };
        s.split.proposals.insert(key.clone(), all.clone());
        let (here, posts) = planned(&s, &target, &all, "m-1").unwrap();
        assert!(here.is_none() && posts.len() == 1);
        s.split.sending.insert(key.clone(), Going { here, out: BTreeSet::from([0]), failed: false, home: home(&target) });
        point_replied(&mut s, key.clone(), 0, true, "Posted on N-2 as post 3.".into());
        assert!(s.split.sending.is_empty() && !s.composers.contains_key(&key) && proposal(&s, &key).is_none(), "the box empties");
        assert_eq!(s.split.left.get(&(0, "N-1".into())), Some(&vec![("N-2".to_string(), "thread 2".to_string(), text.clone())]));
        assert!(s.split.carry.is_empty() && s.split.sent.is_empty(), "no split lines: nothing of it is a point 1");
        // its post in the view: Bring back moves it to N-1
        let p = &s.projects[0];
        assert_eq!(posted(&s, p, "N-1", "N-2", &text), None);
        let n2 = p.live.tickets.values().find(|v| v.id == "N-2").unwrap().n;
        let mut q = s.projects[0].live.tickets[&n2].posts[0].clone();
        q.n = 9;
        q.by = "owner".into();
        q.text = serde_json::from_value(serde_json::json!(text)).unwrap();
        s.projects[0].live.tickets.get_mut(&n2).unwrap().posts.push(q);
        assert_eq!(posted(&s, &s.projects[0], "N-1", "N-2", &text), Some(("N-2".to_string(), 9)));
        assert!(matches!(bring_back_request("N-2", 9, "N-1"), Request::Move { n: 9, to: Some(to), .. } if to == "N-1"));
        // the swap keeps it; × lets it go
        let kept = left_kept(&s);
        s.split.left.clear();
        put_after(&mut s, Vec::new(), kept);
        assert_eq!(s.split.left.len(), 1);
        let_go(&mut s, 0, "N-1", &text);
        assert!(s.split.left.is_empty());
        // a point refused in it: the box keeps the rest, nothing is carried as a split
        let mut two = all.clone();
        two.points = vec![
            Point { words: "agda-dev is still not in the window.".into(), from: 0, len: 36, to: "N-2".into(), label: None },
            Point { words: "runner rows: yes, do them. autostart: yes".into(), from: 37, len: 41, to: "N-4".into(), label: None },
        ];
        two.message = Some("m-2".into());
        two.home = home(&target);
        s.composers.insert(key.clone(), rune::TextEdit::new(&text));
        s.split.proposals.insert(key.clone(), two);
        s.split.sending.insert(key.clone(), Going { here: None, out: BTreeSet::from([0, 1]), failed: false, home: home(&target) });
        point_replied(&mut s, key.clone(), 0, true, "Posted on N-2 as post 4.".into());
        point_replied(&mut s, key.clone(), 1, false, "not posted: N-4 is closed".into());
        assert_eq!(s.composers.get(&key).map(|e| e.text.clone()).as_deref(), Some("runner rows: yes, do them. autostart: yes"));
        assert!(s.split.carry.is_empty() && s.split.left[&(0, "N-1".to_string())].len() == 1);
        assert!(proposal(&s, &key).is_some_and(|p| p.message.is_none() && p.points.len() == 1));
    }

    /// A window swap keeps the proposal and his clicks, through the window's state (state.rs), for a box that still
    /// holds its text; it is not asked again.
    #[test]
    fn a_swap_keeps_the_proposal_and_his_clicks() {
        let (_held, mut s, target) = scene();
        let key = (0usize, target.place());
        flip(&mut s, &key, 1);
        let kept = kept(&s);
        assert_eq!(kept.len(), 1);
        s.split.proposals.clear();
        put_back(&mut s, kept.clone());
        assert_eq!(proposal(&s, &key).map(|p| p.kept_here.clone()), Some(BTreeSet::from([1])));
        assert_eq!(s.split.asked.get(&key), Some(&kept[0].1.text));
        s.split.proposals.clear();
        s.composers.insert(key.clone(), rune::TextEdit::new("another text"));
        put_back(&mut s, kept);
        assert!(s.split.proposals.is_empty());
    }

    /// The still for the hand-over (N-773): `STILL_OUT=DIR cargo test split_still -- --ignored`. Made-up threads only:
    /// on N-687 his earlier message, split, with its line and Bring back; in the box his new one with the proposal,
    /// the point for N-575 underlined, its chip under the box.
    #[test]
    #[ignore]
    fn split_still() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn init() -> App {
            let (_held, mut s) = crate::view_cost::test_app();
            let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
            p.down = None;
            p.protocol = coord_types::SPLIT_PROTOCOL.max(coord_types::PROTOCOL);
            let at = |m: u32| format!("2026-11-18T23:{m:02}:00Z");
            let sf = |point: u32| serde_json::json!({"message": "m-1", "point": point, "of": 2});
            let t = |id: &str, n: u64, title: &str, posts: serde_json::Value| -> docket_machine::TicketView {
                serde_json::from_value(serde_json::json!({"id": id, "n": n, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
                    "title": title, "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": at(1), "given": [], "posts": posts}))
                .unwrap()
            };
            let a = t("N-687", 687, "Window pieces tonight", serde_json::json!([
                {"n": 1, "at": at(1), "by": "paper-research", "text": "Three pieces are open for the window: agda-dev, runner rows, autostart.", "told": ["owner"]},
                {"n": 2, "at": at(5), "by": "owner", "text": "agda-dev first. autostart: later", "names": ["paper-research"], "split_from": sf(1)},
                {"n": 3, "at": at(6), "by": "paper-research", "text": "Taken: agda-dev first, autostart later.", "told": ["owner"]}]));
            let b = t("N-575", 575, "Runner rows", serde_json::json!([
                {"n": 1, "at": at(0), "by": "owner", "text": "the runner's jobs as rows", "names": ["paper-research"]},
                {"n": 2, "at": at(5), "by": "owner", "text": "runner rows: the queue first.", "names": ["paper-research"], "split_from": sf(2)}]));
            for (seq, v) in [(1, a), (2, b)] {
                p.live.apply(coord_types::Event { seq, body: coord_types::Body::Ticket(v) });
            }
            s.projects = vec![p];
            s.cur = 0;
            s.sel = crate::Sel::Tickets;
            s.tickets.shown = Some(687);
            s.split.on = true;
            let key = (0usize, "thread:N-687".to_string());
            let mut pr = example();
            pr.points[1].to = "N-575".into();
            s.composers.insert(key.clone(), rune::TextEdit::new(&pr.text));
            s.split.proposals.insert(key, pr);
            s
        }
        // the same box in a narrow pane, a second Threads pane beside it: whether the switch's label wraps the row or
        // pushes Send (consult-4 N-589.p50; paper-research p51)
        fn narrow() -> App {
            let mut s = init();
            s.panes = vec![(0, crate::Sel::Tickets), (0, crate::Sel::Agents)];
            s.focus = 0;
            s
        }
        let shots: [(&str, fn() -> App); 2] = [("split-box.png", init), ("split-box-narrow.png", narrow)];
        for (name, start) in shots {
            let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
            rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), start, crate::view) }).args(args).try_run().unwrap();
        }
    }
}
