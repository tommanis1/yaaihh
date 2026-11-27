//! What waits for his decision in the engine's record, on the Engine page: words held for his approval, a spend over
//! its budget. Each is decided here and the decision sent to the window driver (seat.rs).

use super::desk::Waiting;
use super::seat::{decide, Seat};
use super::V2;
use crate::App;
use rune::{col, row, text, El, TextEdit};
use rune_kit::{Button, ButtonVariant, Kit, TextArea, TextInput};
use serde_json::{json, Value as Json};

pub const APPROVAL: &str = "std.control.Await-Approval";
pub const BUDGET: &str = "std.budgets.Request-Budget";

/// The questions waiting, oldest first, each with its decisions.
pub fn el(s: &App, k: &Kit) -> El {
    let Some(v) = s.ext.try_get::<V2>() else { return col() };
    let waiting = v.desk.waiting(&[APPROVAL, BUDGET]);
    let head = match v.seat.as_ref().map(Seat::state) {
        None => "No window driver in [v2]".to_string(),
        Some(Err(why)) => format!("Not seated at the engine: {why}"),
        Some(Ok(())) if waiting.is_empty() => "Nothing waits for you".to_string(),
        Some(Ok(())) => format!("{} waiting for you", waiting.len()),
    };
    let c = col().key("v2-asked").w_full().gap(12.0).child(text(head).font(k.ty.small_medium).color(k.c.text_muted).key("v2-asked-head"));
    c.children(waiting.iter().map(|w| card(v, w, k)))
}

/// Argument I of W as words.
fn arg(w: &Waiting, i: usize) -> String {
    w.args.get(i).map_or_else(String::new, |a| a.as_str().map_or_else(|| a.to_string(), str::to_string))
}

/// What field FIELD of the question at LINE holds, FIRST until he has typed.
fn held(v: &V2, line: u64, field: &'static str, first: &str) -> TextEdit {
    v.edits.get(&(line, field)).cloned().unwrap_or_else(|| TextEdit::new(first))
}

/// Field FIELD of the question at LINE, to type in.
fn edit<'a>(s: &'a mut App, line: u64, field: &'static str, first: &str) -> &'a mut TextEdit {
    s.ext.get_mut::<V2>().edits.entry((line, field)).or_insert_with(|| TextEdit::new(first))
}

/// The words in field FIELD of the question at LINE, as he left them.
fn typed(s: &App, line: u64, field: &'static str, first: &str) -> String {
    let edit = s.ext.try_get::<V2>().and_then(|v| v.edits.get(&(line, field)));
    edit.map_or_else(|| first.to_string(), |e| e.text.clone())
}

/// A decision's button keyed KEY: it sends what ANSWER makes of the fields, or keeps why it cannot.
fn button(label: &str, key: String, variant: ButtonVariant, line: u64, answer: impl Fn(&App) -> Result<Json, String> + 'static, k: &Kit) -> El {
    let go = move |s: &mut App| match answer(s) {
        Ok(a) => decide(s, line, a),
        Err(why) => drop(s.ext.get_mut::<V2>().seat.as_mut().map(|seat| seat.decided.insert(line, Some(why)))),
    };
    Button::new(label).variant(variant).on_press(go).build(k).key(key)
}

/// One question: what it asks, its fields and decisions, and what became of a decision sent.
fn card(v: &V2, w: &Waiting, k: &Kit) -> El {
    let line = w.line;
    let decided = v.seat.as_ref().and_then(|s| s.decided.get(&line));
    let field = move |f: &'static str, label: &str| TextInput::new(&held(v, line, f, "")).placeholder(label).width(None).on_edit(move |s: &mut App| edit(s, line, f, "")).build(k);
    // a field between two buttons takes the room they leave
    let between = |e: El| col().grow(1.0).shrink().min_w(0.0).child(e);
    let why = move |s: &App| Some(typed(s, line, "why", "")).filter(|t| !t.trim().is_empty()).ok_or("Say why first: the asker is told".to_string());
    let (asks, body, decisions) = if w.op == APPROVAL {
        let payload = arg(w, 1);
        let first = payload.clone();
        let words = TextArea::new(&held(v, line, "text", &payload)).width(None).on_edit(move |s: &mut App| edit(s, line, "text", &first)).build(k);
        let approve = button("Approve", format!("v2-approve-{line}"), ButtonVariant::Submit, line, move |s| Ok(json!({"approved": typed(s, line, "text", &payload)})), k);
        let reject = button("Reject", format!("v2-reject-{line}"), ButtonVariant::Danger, line, move |s| Ok(json!({"rejected": why(s)?})), k);
        (format!("Words to {} wait for your approval", arg(w, 0)), words, vec![approve, between(field("why", "Why, if you reject")), reject])
    } else {
        let limit = move |s: &App| typed(s, line, "limit", "").trim().parse::<i64>().map_err(|_| "The new limit is a whole number".to_string());
        let grant = button("Grant", format!("v2-grant-{line}"), ButtonVariant::Submit, line, move |s| Ok(json!({"granted": limit(s)?})), k);
        let deny = button("Deny", format!("v2-deny-{line}"), ButtonVariant::Danger, line, move |s| Ok(json!({"denied": why(s)?})), k);
        let asks = format!("{} over budget {}, needs {} more", arg(w, 0), arg(w, 1), arg(w, 2));
        (asks, field("limit", "The new limit"), vec![grant, between(field("why", "Why, if you deny")), deny])
    };
    let mut c = col().key(format!("v2-ask-{line}")).w_full().gap(6.0).padding(12.0).radius(k.radius.md).fill(k.c.layer_01);
    c = c.child(text(asks).font(k.ty.body).color(k.c.text_base).wrap()).child(body);
    c.child(match decided {
        Some(None) => text("Sent").font(k.ty.small).color(k.c.text_faint).key(format!("v2-sent-{line}")),
        Some(Some(not)) => col().gap(6.0).child(row().gap(8.0).center_y().children(decisions)).child(text(not.clone()).font(k.ty.small).color(k.c.danger.fg).key(format!("v2-not-{line}"))),
        None => row().gap(8.0).center_y().children(decisions),
    })
}
