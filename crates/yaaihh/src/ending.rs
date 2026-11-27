//! Ending a thread (N-412, the owner N-446.p1; docket-core4's form N-412 post 157): from the thread's "⋯", done,
//! dropped or over, with one line of why. An ended thread leaves the outline; its parent's line for it stays, marked
//! ended. coordd decides who may (the opener, an agent above it, a lead, the owner) and refuses a thread at work;
//! its answer stands under the box.

use crate::threads::Cleared;
use crate::{App, Pending};
use coord_types::Request;
use docket_machine::TicketView;
use rune::{El, col, text};
use rune_kit::{Button, ButtonVariant, Kit};

/// The three ways, as the box names them and as the request says them.
const HOW: [(&str, &str); 3] = [("Done", "done"), ("Dropped", "dropped"), ("Over", "kept")];

/// The end box of a thread, while it is open: the way picked. The why is typed in the one message box
/// (`message_box::Target::End`), kept under [`place`].
#[derive(Default, Clone)]
pub struct Ending {
    pub how: usize,
}

/// The place of thread N's why in `App::composers`.
pub fn place(n: u64) -> String {
    format!("end:{n}")
}

/// The request that ends thread T HOW (`done`, `dropped`, `kept`) for WHY. None from coord-types without it.
pub fn request(t: &str, how: &str, why: &str) -> Option<Request> {
    // docket-core4's `Request::EndThread { ts, how, why }`, op "end_thread" (N-454.p2); `end` is the runner's job end
    serde_json::from_value(serde_json::json!({"op": "end_thread", "ts": [t], "how": how, "why": why})).ok().filter(|r| matches!(serde_json::to_value(r), Ok(v) if v.get("how").is_some()))
}

/// "End…" behind the thread's "⋯": opens the box (again: closes it). Not on a thread that has ended.
pub fn button(s: &App, v: &TicketView, k: &Kit) -> Option<El> {
    // not offered below ASKS_CLOSED_PROTOCOL (an older coordd has no `end_thread`), nor while it has ended
    let speaks = s.projects.get(s.cur).is_some_and(|p| p.protocol >= coord_types::ASKS_CLOSED_PROTOCOL) || std::env::var_os("YAAIHH_ENDING").is_some();
    if ended(v).is_some() || v.kind != "thread" || !speaks {
        return None;
    }
    let n = v.n;
    Some(Button::new("End…").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
        if s.tickets.ending.remove(&n).is_none() {
            s.tickets.ending.insert(n, Ending::default());
        }
    }).build(k))
}

/// The box: the three ways, the why, End and Cancel.
pub fn panel(s: &App, v: &TicketView, k: &Kit) -> Option<El> {
    let e = s.tickets.ending.get(&v.n)?;
    let n = v.n;
    let ways: Vec<&str> = HOW.iter().map(|(w, _)| *w).collect();
    let pick = rune_kit::SegmentedControl::new(&ways, Some(e.how)).label("How it ended").fit().on_pick(move |s: &mut App, i, _| {
        if let Some(e) = s.tickets.ending.get_mut(&n) {
            e.how = i;
        }
    }).build(k);
    let cancel = Button::new("Cancel").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| close(s, n)).build(k);
    let hint = text("It leaves the outline, marked ended.").font(k.ty.small).color(k.c.text_muted).wrap();
    // the why in the one message box (message_box.rs): its spelling, mic and keys; its Send ends the thread
    let target = crate::message_box::Target::End { project: s.cur, n, t: v.id.clone() };
    let actions = crate::message_box::Actions { left: None, right: vec![cancel], send: Some("End the thread".into()), placeholder: "Why, in one line".into() };
    let why = crate::message_box::build(s, &target, &target, actions, k);
    Some(col().key(format!("ending-{}", v.id)).w_full().gap(8.0).padding(10.0).radius(k.radius.md).border(1.0, k.c.border_weak).child(pick).child(hint).child(why))
}

/// Thread N's end box closed, its why gone.
pub fn close(s: &mut App, n: u64) {
    s.tickets.ending.remove(&n);
    let boxes: Vec<(usize, String)> = s.composers.keys().filter(|(_, p)| *p == place(n)).cloned().collect();
    for key in &boxes {
        crate::changed::sent(s, key);
    }
    s.composers.retain(|(_, p), _| *p != place(n));
}

/// The end box's Send: thread ID (number N) ended the way picked, for the why typed; nothing without a why.
pub fn send(s: &mut App, pi: usize, n: u64, id: &str) {
    let Some(e) = s.tickets.ending.get(&n).cloned() else { return };
    let why = s.composers.get(&(pi, place(n))).map(|w| w.text.trim().to_string()).unwrap_or_default();
    if why.is_empty() {
        return;
    }
    match request(id, HOW[e.how.min(2)].1, &why) {
        Some(req) => {
            s.request_in(pi, req, Pending::TicketDone { clear: Cleared::Ended(n) });
        }
        None => s.notice = Some("needs a newer build".into()),
    }
}

/// How V ended, from coordd's outline line ("done", why), when it was ended.
pub fn ended(v: &TicketView) -> Option<(String, String)> {
    let o = v.outline.as_ref().filter(|o| !o.open)?;
    let why = serde_json::to_value(o).ok()?.get("why")?.as_str()?.to_string();
    Some((o.state.clone(), why))
}

/// Whether the minder closed thread V as a one-off and that answer still stands: closed with `OneOff`, the minder's
/// last answer not undone, and not ended by someone (`coord end` leaves an ended outline: no Undo takes that back).
pub fn minder_closed(v: &TicketView) -> bool {
    use docket_machine::view::ViewStatus;
    let one_off = matches!(&v.status, ViewStatus::Done { result } if result["head"] == "OneOff");
    one_off && ended(v).is_none() && v.shaped.iter().rev().find(|s| s.refused.is_none()).is_some_and(|s| !s.undone)
}

/// "Reopen" behind the thread's "⋯" when the minder closed it: the owner's `Undo` of the minder's last answer
/// (docket-core7), which is what the existing Undo under a post sends.
pub fn reopen(v: &TicketView, k: &Kit) -> Option<El> {
    if !minder_closed(v) {
        return None;
    }
    let t = v.id.clone();
    Some(Button::new("Reopen").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
        s.request_in(s.cur, Request::Undo { t: t.clone() }, Pending::TicketDone { clear: Cleared::Quiet });
    }).build(k))
}

/// "ended: done · the lexer is merged", under an ended thread's title.
pub fn line(v: &TicketView, k: &Kit) -> Option<El> {
    let (how, why) = ended(v)?;
    Some(text(format!("ended: {how} · {why}")).key(format!("ended-{}", v.id)).font(k.ty.small).color(k.c.text_muted).wrap())
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_the_minders_close_can_be_reopened() {
        use docket_machine::TicketView;
        let mk = |status: serde_json::Value, shaped: serde_json::Value, outline: serde_json::Value| -> TicketView {
            let mut v = serde_json::json!({"id": "N-122", "n": 122, "kind": "thread", "opener": "owner", "ctx": {}, "form": {}, "title": "t",
                "status": status, "children": [], "program": "", "bound": 3, "spent": 1, "opened_at": "2026-11-16T21:00:00Z", "given": [], "posts": [], "shaped": shaped});
            if !outline.is_null() {
                v["outline"] = outline;
            }
            serde_json::from_value(v).unwrap()
        };
        let done = serde_json::json!({"is": "done", "result": {"head": "OneOff"}});
        let step = |undone: bool| serde_json::json!([{"n": 1, "at": "2026-11-16T21:10:00Z", "answer": {"head": "OneOff"}, "undone": undone}]);
        assert!(super::minder_closed(&mk(done.clone(), step(false), serde_json::Value::Null)));
        assert!(!super::minder_closed(&mk(done.clone(), step(true), serde_json::Value::Null)), "already undone");
        assert!(!super::minder_closed(&mk(done.clone(), serde_json::json!([]), serde_json::Value::Null)), "no minder answer");
        assert!(!super::minder_closed(&mk(serde_json::json!({"is": "done", "result": {"head": "Between"}}), step(false), serde_json::Value::Null)));
    }

    #[test]
    fn the_three_ways_are_the_forms_words() {
        assert_eq!(super::HOW.map(|(_, h)| h), ["done", "dropped", "kept"]);
    }
}
