//! Schedule beside Send (the owner, N-1980): in an agent's chat and on a thread, his words held by coordd
//! (`Request::Schedule`) instead of sent, drawn above the box as a muted dashed card with Edit, Send now and Delete,
//! waking no one. A press holds them until he sends them; its menu sends them at the turn's end, at a time, after a
//! while, or once a thread closes.

use crate::message_box::Target;
use crate::{App, Pending};
use chrono::{DateTime, Duration, Local, NaiveTime, TimeZone};
use coord_types::Request;
use coord_types::schedule::{Held, Sched, When};
use rune::{El, TextEdit, col, row, text};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit, Menu, MenuItem, Placement};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

/// How often a project's list is asked again while one of its held messages waits on a time or a thread.
pub const AGAIN: std::time::Duration = std::time::Duration::from_secs(15);

/// Each project's held messages as its coordd last listed them, by project id; the asks on their way; the typed time
/// he is writing, by the box's project and place; what coordd refused, by box.
#[derive(Default)]
pub struct Schedules {
    held: HashMap<String, Vec<Held>>,
    asked: HashMap<String, Instant>,
    waiting: HashSet<String>,
    /// The box whose Schedule menu is open.
    pub menu: Option<Target>,
    pub typing: Option<((usize, String), TextEdit)>,
    pub refused: HashMap<(usize, String), String>,
}

impl Schedules {
    /// Project ID's held messages from the box at PLACE, oldest first.
    pub fn at<'a>(&'a self, id: &str, place: &'a str) -> impl Iterator<Item = &'a Held> + 'a {
        self.held.get(id).into_iter().flatten().filter(move |h| h.place == place)
    }
}

/// Whether TARGET's box offers Schedule: an agent's chat or a thread's box, on a coordd up that holds messages.
pub fn offered(s: &App, target: &Target) -> bool {
    matches!(target, Target::Agent { .. } | Target::Thread { way: crate::message_box::Way::Here, .. })
        && s.projects.get(target.project()).is_some_and(|p| p.down.is_none() && p.protocol >= coord_types::SCHEDULE_PROTOCOL)
}

/// Asks each linked project's coordd for its list once, and again every [`AGAIN`] while one waits on a time or thread.
pub fn poll(s: &mut App) -> bool {
    let mut asked = false;
    for pi in 0..s.projects.len() {
        let p = &s.projects[pi];
        if p.link.is_none() || p.down.is_some() || p.protocol < coord_types::SCHEDULE_PROTOCOL || s.schedule.waiting.contains(&p.id) {
            continue;
        }
        let timed = s.schedule.held.get(&p.id).is_some_and(|v| v.iter().any(|h| h.when != When::Hold && h.failed.is_none()));
        let due = match s.schedule.asked.get(&p.id) {
            None => true,
            Some(at) => timed && at.elapsed() >= AGAIN,
        };
        if due {
            asked |= act(s, pi, Sched::List, None, None);
        }
    }
    asked
}

/// ACT asked of project PI's coordd, from the box at AT, holding its WORDS; whether it went.
fn act(s: &mut App, pi: usize, act: Sched, at: Option<(usize, String)>, words: Option<String>) -> bool {
    let id = s.projects[pi].id.clone();
    if !s.request_in(pi, Request::Schedule { act }, Pending::Schedule { project: id.clone(), at, words }) {
        return false;
    }
    s.schedule.asked.insert(id.clone(), Instant::now());
    s.schedule.waiting.insert(id);
    true
}

/// Project ID's coordd's answer R: its list, and the box AT emptied of the WORDS it held (unless he has written on
/// since); else its refusal said under the box, the words left in it, and the list asked again, as a refused Send
/// now changes it.
pub fn came(s: &mut App, id: String, at: Option<(usize, String)>, words: Option<String>, r: Result<String, String>) {
    s.schedule.waiting.remove(&id);
    match r.and_then(|t| serde_json::from_str::<Vec<Held>>(&t).map_err(|e| e.to_string())) {
        Ok(list) => {
            s.schedule.held.insert(id, list);
            if let Some(k) = at {
                s.schedule.refused.remove(&k);
                if words.is_some_and(|w| s.composers.get(&k).is_some_and(|e| e.text == w)) {
                    emptied(s, k);
                }
            }
        }
        Err(why) => {
            s.schedule.asked.remove(&id);
            if let Some(k) = at {
                s.schedule.refused.insert(k, why);
            }
        }
    }
}

/// An ask the pipe's end lost: asked again once it is back.
pub(crate) fn lost(st: &mut Schedules, w: &Pending) {
    if let Pending::Schedule { project, .. } = w {
        st.waiting.remove(project);
        st.asked.remove(project);
    }
}

/// The request TARGET's Send would make of what its box holds; none when it holds nothing or cannot go (said there).
fn request(s: &mut App, target: &Target) -> Option<Request> {
    match target {
        Target::Agent { project, name } => s.composed(*project, name).map(|(r, _)| r),
        _ => match crate::message_box::request(s, target) {
            Ok(r) => Some(r),
            Err(e) => {
                if !e.is_empty() {
                    s.box_said.insert((target.project(), target.place()), Err(e));
                }
                None
            }
        },
    }
}

/// The box KEY emptied as its Send empties it.
fn emptied(s: &mut App, key: (usize, String)) {
    s.words.unclean.remove(&key);
    s.notes.held.remove(&key);
    s.composers.remove(&key);
    crate::changed::sent(s, &key);
    s.writing_changed(key.0, &key.1);
    crate::state::write_now(s);
}

/// What TARGET's box holds, held by its coordd until WHEN; the box emptied once coordd has it ([`came`]).
pub fn hold(s: &mut App, target: &Target, when: When) {
    s.schedule.menu = None;
    let Some(req) = request(s, target) else { return };
    let key = (target.project(), target.place());
    let words = s.composers.get(&key).map(|e| e.text.clone());
    if act(s, key.0, Sched::Add { req: Box::new(req), when, place: key.1.clone() }, Some(key), words) {
        s.schedule.typing = None;
    }
}

/// What TARGET's box holds sent now, never into a running turn: given at its end (`wait`).
pub fn after_turn(s: &mut App, target: &Target) {
    s.schedule.menu = None;
    if let Target::Agent { project, name } = target {
        s.send(*project, name, true);
        return;
    }
    if let Some(mut req) = request(s, target) {
        set_wait(&mut req);
        crate::message_box::sent(s, target, req);
    }
}

/// REQ made to wait for the turn's end.
pub fn set_wait(req: &mut Request) {
    if let Request::Post { wait, .. } | Request::Msg { wait, .. } | Request::Say { wait, .. } = req {
        *wait = true;
    }
}

/// When TYPED says, read at NOW: `+15m`, `+2h`, `+1d` (or `in 15m`); `09:00` (today, else tomorrow);
/// `tomorrow 09:00` (`tomorrow` alone: 09:00); a thread `N-12` (or `when N-12 closes`): once it is closed.
pub fn when_of<Z: TimeZone>(typed: &str, now: DateTime<Z>) -> Result<When, String> {
    let t = typed.trim().to_lowercase();
    let t = t.strip_prefix("in ").or_else(|| t.strip_prefix('+')).map(str::to_string).unwrap_or(t);
    let thread = t.strip_prefix("when ").map(|x| x.trim_end_matches(" closes").trim()).unwrap_or(&t).to_uppercase();
    if thread.split_once('-').is_some_and(|(a, n)| !a.is_empty() && a.chars().all(|c| c.is_ascii_alphabetic()) && !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())) {
        return Ok(When::Closes { t: thread });
    }
    let utc = |at: DateTime<Z>| When::At { at: at.with_timezone(&chrono::Utc).format("%Y-%m-%dT%H:%M:%SZ").to_string() };
    if let Some((n, unit)) = t.find(|c: char| !c.is_ascii_digit()).filter(|&i| i > 0).map(|i| t.split_at(i)) {
        let n: i64 = n.parse().map_err(|_| format!("not a time: {typed}"))?;
        let d = match unit.trim() {
            "m" | "min" | "mins" | "minutes" => Some(Duration::minutes(n)),
            "h" | "hour" | "hours" => Some(Duration::hours(n)),
            "d" | "day" | "days" => Some(Duration::days(n)),
            _ => None,
        };
        if let Some(d) = d {
            return Ok(utc(now + d));
        }
    }
    let (tomorrow, clock) = match t.strip_prefix("tomorrow") {
        Some(rest) => (true, if rest.trim().is_empty() { "09:00" } else { rest.trim() }),
        None => (false, t.as_str()),
    };
    let time = NaiveTime::parse_from_str(clock, "%H:%M").map_err(|_| format!("not a time: {typed} (+15m, 09:00, tomorrow 09:00, N-12)"))?;
    let mut day = now.date_naive() + Duration::days(i64::from(tomorrow));
    let at = |day: chrono::NaiveDate| now.timezone().from_local_datetime(&day.and_time(time)).earliest();
    if !tomorrow && at(day).is_some_and(|a| a <= now) {
        day += Duration::days(1);
    }
    at(day).map(utc).ok_or_else(|| format!("not a time on this clock: {typed}"))
}

/// What a card says of when H goes, on his clock.
fn when_words(h: &Held) -> String {
    if let Some(why) = &h.failed {
        return format!("Not sent: {why}");
    }
    match &h.when {
        When::Hold => "Queued (Hold)".into(),
        When::Closes { t } => format!("Queued · when {t} closes"),
        When::At { at } => match DateTime::parse_from_rfc3339(at) {
            Ok(a) => format!("Queued · {}", a.with_timezone(&Local).format("%a %H:%M")),
            Err(_) => format!("Queued · {at}"),
        },
    }
}

/// His held messages from the box at PLACE of project PI: muted dashed cards, each with Edit (its words back in the
/// box), Send now and Delete.
pub fn cards(s: &App, pi: usize, place: &str, k: &Kit) -> Vec<El> {
    let Some(p) = s.projects.get(pi) else { return Vec::new() };
    let ghost = |label: &str| Button::new(label.to_string()).variant(ButtonVariant::Ghost);
    s.schedule
        .at(&p.id, place)
        .map(|h| {
            let (id, words, key) = (h.id, h.text().to_string(), (pi, place.to_string()));
            let (k2, k3) = (key.clone(), key.clone());
            let acts = row()
                .gap(4.0)
                .center_y()
                .child(ghost("Edit").on_press(move |s: &mut App| edit(s, &k2, id, &words)).build(k).key(format!("held-edit-{pi}-{id}")))
                .child(ghost("Send now").on_press(move |s: &mut App| _ = act(s, pi, Sched::Send { id }, Some(k3.clone()), None)).build(k).key(format!("held-send-{pi}-{id}")))
                .child(ghost("Delete").on_press(move |s: &mut App| _ = act(s, pi, Sched::Drop { id }, None, None)).build(k).key(format!("held-drop-{pi}-{id}")));
            let colour = if h.failed.is_some() { k.c.danger.fg } else { k.c.text_faint };
            let head = row().w_full().gap(8.0).center_y().child(text(when_words(h)).font(k.ty.small).color(colour).grow(1.0).shrink().min_w(0.0)).child(acts);
            let body = text(h.text().to_string()).font(k.ty.body).color(k.c.text_muted).wrap().max_lines(4);
            col().key(format!("held-{pi}-{}", h.id)).w_full().padding(8.0).gap(4.0).radius(k.radius.md).border(1.0, k.c.border_weak).border_dash(4.0, 3.0).opacity(0.85).child(head).child(body)
        })
        .chain(s.schedule.refused.get(&(pi, place.to_string())).map(|why| text(format!("Not held: {why}")).font(k.ty.small).color(k.c.danger.fg).wrap().key(format!("held-refused-{pi}"))))
        .collect()
}

/// Held ID's WORDS back in the box KEY (after what it holds), and ID dropped.
fn edit(s: &mut App, key: &(usize, String), id: u64, words: &str) {
    let now = s.composers.get(key).map(|e| e.text.clone()).unwrap_or_default();
    let t = if now.trim().is_empty() { words.to_string() } else { format!("{now}\n\n{words}") };
    let mut e = TextEdit::new(&t);
    (e.anchor, e.focus, e.focused) = (t.len(), t.len(), true);
    s.composers.insert(key.clone(), e);
    act(s, key.0, Sched::Drop { id }, Some(key.clone()), None);
}

/// Schedule beside Send, and ▾ for its menu ([`menu`]).
pub fn button(s: &App, target: &Target, k: &Kit) -> Vec<El> {
    if !offered(s, target) {
        return Vec::new();
    }
    vec![
        {
            let to = target.clone();
            Button::new("Schedule")
                .variant(ButtonVariant::Neutral)
                .control_size(ControlSize::Small)
                .help("Hold it: it waits above the box until you send it")
                .on_press(move |s: &mut App| hold(s, &to, When::Hold))
                .build(k)
                .key(format!("schedule-{}-{}", target.project(), target.place()))
        },
        menu(s, target, k).build(k, &mut Vec::new()),
    ]
}

/// Schedule's menu for TARGET's box, behind ▾: Hold, the turn's end, in 15 minutes, in an hour, tomorrow 09:00, when
/// this thread closes, or a time or thread typed. Drawn open in the window's layer (message_box.rs `open_layer`).
pub fn menu(s: &App, target: &Target, k: &Kit) -> Menu {
    let (pi, place) = (target.project(), target.place());
    let to = target.clone();
    let trigger = Button::new("▾").variant(ButtonVariant::Ghost).control_size(ControlSize::Small).help("When it goes").on_press(move |s: &mut App| s.schedule.menu = if s.schedule.menu.as_ref() == Some(&to) { None } else { Some(to.clone()) }).build(k).key(format!("schedule-menu-{pi}-{place}"));
    let mut m = Menu::new(format!("schedule-items-{pi}-{place}")).trigger(trigger).open(s.schedule.menu.as_ref() == Some(target)).placement(Placement::TopEnd).on_close(|s: &mut App| s.schedule.menu = None);
    let item = |label: &str, help: &str, go: Box<dyn Fn(&mut App)>| MenuItem::new(label.to_string()).description(help.to_string()).on_select(move |s: &mut App| go(s));
    let t = target.clone();
    m = m.item(item("Draft / Hold", "Goes only when you press Send now", Box::new(move |s| hold(s, &t, When::Hold))));
    let t = target.clone();
    m = m.item(item("When its turn ends", "Never put into a running turn", Box::new(move |s| after_turn(s, &t))));
    for (label, typed) in [("In 15 minutes", "+15m"), ("In 1 hour", "+1h"), ("Tomorrow 09:00", "tomorrow 09:00")] {
        let t = target.clone();
        m = m.item(item(label, "", Box::new(move |s| typed_hold(s, &t, typed))));
    }
    if let Target::Thread { t: id, .. } = target {
        let (t, id) = (target.clone(), id.clone());
        m = m.item(item(&format!("When {id} closes"), "", Box::new(move |s| hold(s, &t, When::Closes { t: id.clone() }))));
    }
    let key = (pi, place);
    m.item(item("At a time or when a thread closes…", "+15m, 09:00, tomorrow 09:00, N-12", Box::new(move |s| {
        s.schedule.menu = None;
        s.schedule.typing = Some((key.clone(), TextEdit::new("")));
    })))
}

/// TYPED read as a time or thread and what TARGET's box holds held until then; not read, said under the box.
fn typed_hold(s: &mut App, target: &Target, typed: &str) {
    match when_of(typed, Local::now()) {
        Ok(w) => hold(s, target, w),
        Err(why) => _ = s.schedule.refused.insert((target.project(), target.place()), why),
    }
}

/// The line he types a time or thread in, under TARGET's box, while it is open.
pub fn typing(s: &App, target: &Target, k: &Kit) -> Option<El> {
    let key = (target.project(), target.place());
    let (_, e) = s.schedule.typing.as_ref().filter(|(at, _)| *at == key)?;
    let (to, k2) = (target.clone(), key.clone());
    let field = rune_kit::TextInput::new(e)
        .width(None)
        .placeholder("+15m · 09:00 · tomorrow 09:00 · N-12")
        .on_edit(move |s: &mut App| &mut s.schedule.typing.as_mut().expect("open").1)
        .on_submit(move |s: &mut App, _| {
            let typed = s.schedule.typing.as_ref().map(|(_, e)| e.text.clone()).unwrap_or_default();
            typed_hold(s, &to, &typed);
        })
        .build(k);
    let close = Button::new("×").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
        s.schedule.typing = None;
        s.schedule.refused.remove(&k2);
    });
    Some(row().key(format!("schedule-typed-{}-{}", key.0, key.1)).w_full().gap(8.0).center_y().child(text("Schedule for").font(k.ty.small).color(k.c.text_muted)).child(col().grow(1.0).child(field)).child(close.build(k)))
}

#[cfg(test)]
#[path = "schedule_tests.rs"]
mod tests;
