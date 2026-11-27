//! Slow mode: each Docket moment on a slowed thread halts as a card (what is about to happen, and what
//! follows) until he approves, edits or rejects it; approve is its button or Ctrl+Return while it has focus.

use crate::threads::{Cleared, Page};
use crate::{App, Pending, Proj};
use coord_types::{Request, TicketView};
use docket_machine::view::RunNode;
use docket_machine::{Type, ViewHalt};
use rune::{El, KeyEvent, TextEdit, Tx, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit, TextArea, TextInput};
use serde_json::Value;
use std::collections::HashMap;

/// What of halt (ticket number, halt number) is being edited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Editing {
    Prompt,
    Worker,
    Head,
    Wakes,
    Form,
    Answer,
}

/// The edits being written, by (ticket number, halt number).
#[derive(Default)]
pub struct State {
    pub editing: HashMap<(u64, u32), Editing>,
    pub text: HashMap<(u64, u32), TextEdit>,
    /// An open halt's form, field by field; a head's or an answer's fields the same way.
    pub fields: HashMap<(u64, u32, String), TextEdit>,
    /// The constructor a head's edit picked, where the answer type is a choice of them.
    pub ctor: HashMap<(u64, u32), String>,
    /// The halts whose head or answer is edited as JSON ("Advanced JSON"), not field by field.
    pub raw: std::collections::HashSet<(u64, u32)>,
    /// The reason typed for each halt whose reject is open, by (ticket number, halt number).
    pub rejecting: HashMap<(u64, u32), TextEdit>,
}

/// What of a halt's card opens apart (popup.rs).
#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    /// The prompt a step would give.
    Prompt,
    /// An open's form field, by name.
    Field(String),
    /// The ticket's program from the halt on.
    Program,
    /// The I-th answer a notice passes up, which could not be read.
    Passed(usize),
}

/// Halt HN of the ticket called ID in P, a handler's too (as [`shown`] finds them); None once it is no longer held.
fn halt_of<'a>(p: &'a Proj, id: &str, hn: u32) -> Option<(&'a TicketView, &'a ViewHalt)> {
    let t = p.live.tickets.values().chain(p.live.tickets.values().flat_map(|t| t.handlers.iter())).find(|t| t.id == id)?;
    Some((t, t.halts.iter().find(|h| n_of(h) == hn)?))
}

/// PART of halt HN of the ticket called ID, of the project shown, whole, as its popup shows it; None when the halt or
/// the part is no longer there. APART is the popup's (its text's key, its comment source): the passage he is commenting
/// on in it is lit there (popup.rs `source`).
pub fn part(s: &App, id: &str, hn: u32, part: &Part, apart: (&str, &str), k: &Kit) -> Option<El> {
    let (pg, p) = (&s.tickets, s.projects.get(s.cur)?);
    let (t, h) = halt_of(p, id, hn)?;
    let said = Some((s, apart.1));
    match (part, h) {
        (Part::Prompt, ViewHalt::Give { prompt, .. }) => Some(crate::ask::lit_words(s, apart.0, apart.1, prompt, |w| words_el(w, true, k).selectable(true), k)),
        // an agent's words, drawn as every message's are (post.rs)
        (Part::Field(f), ViewHalt::Open { form, .. }) => form.get(f).map(|x| crate::post::message_body(Some(p), &crate::texts::words(pg, x), said, None, k)),
        (Part::Program, _) => t.continuation.as_deref().map(strip_in_flight).filter(|c| !c.trim().is_empty()).map(|c| hanging(&c, k)),
        (Part::Passed(i), ViewHalt::Notice { detail, .. }) => {
            let a = detail.get("passed")?.as_array()?.get(*i)?;
            let by = a.get("by").and_then(Value::as_str).unwrap_or("its agent");
            let w = a.get("text").map(|x| crate::texts::words(pg, x)).unwrap_or_default();
            // a session file rewritten since: the reader's error names a path; the owner is told what it means
            Some(if w.starts_with("(not read:") {
                words_el(format!("{by}'s words are gone from its session"), false, k)
            } else {
                crate::post::message_body(Some(p), &w, said, None, k)
            })
        }
        _ => None,
    }
}

pub(crate) fn n_of(h: &ViewHalt) -> u32 {
    match h {
        ViewHalt::Give { n, .. } | ViewHalt::Take { n, .. } | ViewHalt::Deliver { n, .. } | ViewHalt::Open { n, .. } | ViewHalt::Decide { n, .. } | ViewHalt::Notice { n, .. } => *n,
    }
}

pub fn at_of(h: &ViewHalt) -> String {
    match h {
        ViewHalt::Give { at, .. } | ViewHalt::Take { at, .. } | ViewHalt::Deliver { at, .. } | ViewHalt::Open { at, .. } | ViewHalt::Decide { at, .. } | ViewHalt::Notice { at, .. } => at.to_string(),
    }
}

/// What the card's frame says is about to happen.
/// A take of the minder's `shape` whose head changes the program: ("Program", the program it puts at the hole) or
/// ("Close", the result the ticket closes with).
fn minder_change(h: &ViewHalt) -> Option<(&'static str, Value)> {
    let ViewHalt::Take { call, worker, head, .. } = h else { return None };
    if call != "shape" && worker != "minder" {
        return None;
    }
    let field = |names: &[&str]| head.as_object().and_then(|o| names.iter().find_map(|n| o.get(*n)).cloned().or_else(|| o.iter().find(|(k, _)| *k != "head").map(|(_, v)| v.clone()))).unwrap_or(Value::Null);
    match head.get("head").and_then(Value::as_str) {
        Some("Program") => Some(("Program", field(&["text"]))),
        Some("Close") => Some(("Close", field(&["result"]))),
        _ => None,
    }
}

fn about(h: &ViewHalt) -> &'static str {
    match h {
        ViewHalt::Give { .. } => "a step is about to be given",
        ViewHalt::Take { .. } => "an answer is about to be taken",
        ViewHalt::Deliver { with, .. } if !with.is_empty() => "posts are about to be delivered",
        ViewHalt::Deliver { .. } => "a post is about to be delivered",
        ViewHalt::Open { .. } => "a ticket is about to be opened",
        ViewHalt::Decide { .. } => "a policy decision is about to be used",
        ViewHalt::Notice { .. } => "a notice is about to start a turn",
    }
}

/// The edits halt H offers, each with its button's words.
fn edits(h: &ViewHalt) -> &'static [(Editing, &'static str)] {
    match h {
        ViewHalt::Give { .. } => &[(Editing::Prompt, "edit the prompt"), (Editing::Worker, "edit who gets it")],
        ViewHalt::Take { .. } => &[(Editing::Head, "edit the head")],
        ViewHalt::Deliver { .. } => &[(Editing::Wakes, "edit who is woken")],
        ViewHalt::Open { .. } => &[(Editing::Form, "edit the form")],
        ViewHalt::Decide { .. } => &[(Editing::Answer, "edit the answer")],
        // a notice goes on as it is
        ViewHalt::Notice { .. } => &[],
    }
}

/// Where post N stands on Deliver halt H (one card per tell): Some(true) its first, where the card is drawn; Some(false) one after.
pub fn on_card(h: &ViewHalt, n: u32) -> Option<bool> {
    match h {
        ViewHalt::Deliver { post, .. } if *post == n => Some(true),
        ViewHalt::Deliver { with, .. } if with.contains(&n) => Some(false),
        _ => None,
    }
}

/// The halts thread V shows: its own, then those of the tickets under it (`halted`), each with its ticket.
pub fn shown<'a>(p: &'a Proj, v: &'a TicketView) -> Vec<(&'a TicketView, &'a ViewHalt)> {
    let mut out: Vec<(&TicketView, &ViewHalt)> = v.halts.iter().map(|h| (v, h)).collect();
    for id in &v.halted {
        // a handler ticket counts under the ticket whose handlers list it, V's or one below it
        if let Some(t) = p.live.tickets.values().chain(v.handlers.iter()).chain(p.live.tickets.values().flat_map(|t| t.handlers.iter())).find(|t| &t.id == id) {
            out.extend(t.halts.iter().map(|h| (t, h)));
        }
    }
    out
}

/// The texts the halts of V need read (a taken answer's words, when they are a place in a transcript).
pub fn wanted(p: &Proj, v: &TicketView) -> Vec<Value> {
    shown(p, v)
        .into_iter()
        .map(|(_, h)| match h {
            ViewHalt::Take { words, .. } => serde_json::to_value(words).ok().into_iter().collect(),
            // a notice's: the answers passed up, the words given, the step's arguments
            ViewHalt::Notice { detail, .. } => {
                let mut out: Vec<Value> = detail.get("passed").and_then(Value::as_array).into_iter().flatten().filter_map(|a| a.get("text").cloned()).collect();
                out.extend(detail.get("text").cloned());
                out.extend(detail.get("args").and_then(Value::as_object).into_iter().flatten().map(|(_, x)| x.clone()));
                out
            }
            _ => Vec::new(),
        })
        .flatten()
        .collect()
}

/// Thread V's Step (Manual) | Run (Auto): Step halts each moment for his press, Run lets it go on.
pub fn switch(v: &TicketView, k: &Kit) -> El {
    // slow set over the whole project: no switch here, the project's menu has it
    if v.slow_scope.as_deref() == Some("project") {
        return text("slow, from the project").font(k.ty.small_medium).color(k.c.warning.fg);
    }
    match v.slow_by.as_deref().filter(|by| *by != v.id) {
        Some(by) => text(format!("slow, from {by}")).font(k.ty.small_medium).color(k.c.warning.fg),
        None => {
            let (id, on) = (v.id.clone(), v.slow);
            rune_kit::SegmentedControl::new(&["Step (Manual)", "Run (Auto)"], Some(if on { 0 } else { 1 }))
                .fit()
                .on_pick(move |s: &mut App, i, _| {
                    if (i == 0) != on {
                        let req = Request::Slow { t: id.clone(), on: i == 0 };
                        s.request_in(s.cur, req, Pending::TicketDone { clear: Cleared::Quiet });
                    }
                })
                .build(k)
                .key(format!("step-run-{}", v.id))
        }
    }
}

/// HEAD (a variant's `{"head": C, field: value}`) as `C  field: value · …`.
fn head_line(pg: &Page, head: &Value) -> String {
    match head.as_object() {
        Some(o) if o.get("head").is_some_and(Value::is_string) => {
            let c = o["head"].as_str().unwrap_or_default();
            let fields: Vec<String> = o.iter().filter(|(f, _)| *f != "head").map(|(f, x)| format!("{f}: {}", crate::texts::words(pg, x))).collect();
            if fields.is_empty() { c.to_string() } else { format!("{c}   {}", fields.join(" · ")) }
        }
        _ => crate::texts::words(pg, head),
    }
}

/// The rule behind a decision, as its Decided line carries it, as lines a person reads: what the policy chose and by
/// which rule, each option it left out and the rule that did, its fault; and the default used when it chose nothing.
pub(crate) fn provenance(v: &Value) -> String {
    let s = |x: Option<&Value>| x.and_then(Value::as_str).map(short_paths);
    let mut out: Vec<String> = Vec::new();
    let policy = v.get("policy").unwrap_or(v);
    if policy.get("excluded").is_some() || policy.get("chosen").is_some() {
        if let Some(c) = s(policy.get("chosen")) {
            out.push(format!("chose {c}"));
        }
        if let Some(d) = s(policy.get("decided_by")) {
            out.push(format!("decided by {d}"));
        }
        for c in policy.get("costs").and_then(Value::as_array).into_iter().flatten() {
            if let Some(r) = s(c.get("rule")) {
                out.push(format!("preferred by {r}"));
            }
        }
        // one entry per option left out, or one per rule with the options it left out (`coord_types::excluded_options`)
        for e in policy.get("excluded").and_then(Value::as_array).into_iter().flatten() {
            let (o, r) = (coord_types::excluded_options(e).join(", "), s(e.get("rule")).unwrap_or_default());
            out.push(format!("{o} left out: {r}"));
        }
        if let Some(f) = s(policy.get("fault")) {
            out.push(f);
        }
    }
    if let Some(d) = s(v.get("default")) {
        out.push(format!("so the default: {d}"));
    }
    if out.is_empty() {
        out.push(match v {
            Value::Null => "—".into(),
            Value::String(t) => short_paths(t),
            x => short_paths(&x.to_string()),
        });
    }
    out.join("\n")
}

/// T with each absolute path cut to its file name (`(/a/b/tickets.eflint:80)` reads `(tickets.eflint:80)`).
fn short_paths(t: &str) -> String {
    t.split(' ')
        .map(|w| {
            let (open, rest) = w.strip_prefix('(').map_or(("", w), |r| ("(", r));
            if rest.starts_with('/') { format!("{open}{}", rest.rsplit('/').next().unwrap_or(rest)) } else { w.to_string() }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn then_is_one_line_from_the_run_tree() {
        use docket_machine::view::RunNode;
        let run: Vec<RunNode> = serde_json::from_str(r#"[{"what": "iterate", "line": 1, "source": 0, "text": "iterate errs = \"none\" up to 3", "state": "now", "nodes": [
            {"what": "step", "line": 2, "source": 0, "text": "r <- step who \"Implement this\" -> Commit(c: Ref) | Cannot(why: Text)", "who": "dev", "state": "now"},
            {"what": "match", "line": 3, "source": 0, "text": "match r", "state": "to_come", "nodes": [
                {"what": "arm", "line": 4, "source": 0, "text": "Cannot(why)", "state": "to_come", "nodes": [{"what": "raise", "line": 4, "source": 0, "text": "raise NoPart(why)", "state": "to_come"}]},
                {"what": "arm", "line": 5, "source": 0, "text": "Commit(c)", "state": "to_come", "nodes": [
                    {"what": "step", "line": 5, "source": 0, "text": "v <- step role runner check(c, none)", "who": "runner", "state": "to_come"},
                    {"what": "match", "line": 6, "source": 0, "text": "match v", "state": "to_come", "nodes": [
                        {"what": "arm", "line": 6, "source": 0, "text": "Green", "state": "to_come", "nodes": [{"what": "return", "line": 6, "source": 0, "text": "return Done(c)", "state": "to_come"}]},
                        {"what": "arm", "line": 6, "source": 0, "text": "Red(log)", "state": "to_come", "nodes": [{"what": "return", "line": 6, "source": 0, "text": "return Again(log)", "state": "to_come"}]}]}]}]}]}]"#).unwrap();
        let mut v: coord_types::TicketView = serde_json::from_value(json!({"id": "E-3", "n": 3, "kind": "piece", "ctx": {}, "form": {}, "title": "", "status": {"is": "resting"},
            "posts": [], "children": [], "program": "", "bound": 10, "spent": 0, "given": [], "opened_at": "2026-11-14T10:00:00Z"})).unwrap();
        v.run = run;
        let give = docket_machine::ViewHalt::Give { n: 2, at: "2026-11-14T10:01:00Z".into(), step: 1, call: "piece".into(), worker: "dev".into(), prompt: String::new(), edited: false };
        assert_eq!(super::then_line(&v, &give).as_deref(), Some("Cannot: raises NoPart · Commit: check → runner · Green: done · Red: again, round 1 of 3"));
        let open = docket_machine::ViewHalt::Open { n: 1, at: "2026-11-14T10:00:00Z".into(), ticket: "E-3".into(), kind: "piece".into(), form: json!({"who": "dev"}), fields: vec![] };
        assert_eq!(super::then_line(&v, &open).as_deref(), Some("a piece: first \u{201c}Implement this\u{201d} → dev"));
        // a take follows the head formed: Commit's arm, without Cannot's
        let take = |step: u32, head: serde_json::Value| -> docket_machine::ViewHalt {
            serde_json::from_value(json!({"what": "take", "n": 3, "at": "2026-11-14T10:02:00Z", "step": step, "call": "x", "worker": "dev", "words": "", "head": head, "type": "Any"})).unwrap()
        };
        assert_eq!(super::then_line(&v, &take(1, json!({"head": "Commit", "c": "f995bbe"}))).as_deref(), Some("check → runner · Green: done · Red: again, round 1 of 3"));
        // the runner's Green: its arm returns, the ticket ends there
        v.run[0].nodes[0].state = "done".into();
        v.run[0].nodes[1].nodes[1].nodes[0].state = "now".into();
        assert_eq!(super::then_line(&v, &take(2, json!({"head": "Green"}))).as_deref(), Some("done"));
        // no run tree (a handler's ticket): no then
        v.run.clear();
        assert_eq!(super::then_line(&v, &give), None);
    }

    #[test]
    fn the_minders_answer_that_changes_the_program_is_named() {
        let take = |call: &str, head: serde_json::Value| -> docket_machine::ViewHalt {
            serde_json::from_value(json!({"what": "take", "n": 4, "at": "2026-11-14T10:02:00Z", "step": 3, "call": call, "worker": "minder", "words": "", "head": head, "type": "Any"})).unwrap()
        };
        assert_eq!(super::minder_change(&take("shape", json!({"head": "Program", "text": "step dev x()"}))), Some(("Program", json!("step dev x()"))));
        assert_eq!(super::minder_change(&take("shape", json!({"head": "Close", "result": {"head": "Done"}}))), Some(("Close", json!({"head": "Done"}))));
        assert_eq!(super::minder_change(&take("shape", json!({"head": "Rest"}))), None);
        // a take by another worker with a head of that name is not the minder's
        let other: docket_machine::ViewHalt = serde_json::from_value(json!({"what": "take", "n": 4, "at": "2026-11-14T10:02:00Z", "step": 3, "call": "check", "worker": "runner", "words": "", "head": {"head": "Program", "text": "x"}, "type": "Any"})).unwrap();
        assert_eq!(super::minder_change(&other), None);
    }

    #[test]
    fn a_decisions_rule_reads_as_lines() {
        let v = json!({"default": "coordd's own minder call", "policy": {"chosen": null, "costs": [], "decided_by": null,
            "excluded": [{"option": "dev", "reason": "requires", "rule": "Requires role(option, call.role) (/x/policy/tickets.eflint:80)"}],
            "fault": "no option satisfies the hard rules", "optimal": [], "runner_up": null}});
        assert_eq!(super::provenance(&v), "dev left out: Requires role(option, call.role) (tickets.eflint:80)\nno option satisfies the hard rules\nso the default: coordd's own minder call");
        let w = json!({"chosen": "runner", "costs": [{"cost": 1, "level": 1998, "rule": "Prefer for fills: senior(option) > running(option) (/h/design.eflint:17)"}], "excluded": [], "fault": null});
        assert_eq!(super::provenance(&w), "chose runner\npreferred by Prefer for fills: senior(option) > running(option) (design.eflint:17)");
        // a line written by rule: each rule once, with the options it left out
        let r = json!({"policy": {"excluded": [{"options": ["dev", "ops"], "reason": "requires", "rule": "Requires role(option, call.role) (/x/policy/tickets.eflint:80)"}]}});
        assert_eq!(super::provenance(&r), "dev, ops left out: Requires role(option, call.role) (tickets.eflint:80)");
    }

    /// His two posts sent close together, on a slowed thread (made up).
    const TWO: [&str; 2] = ["the lock is the runner's, not the tower's", "and 38 goes in before 0.3"];

    /// Made-up slowed N-450 with posts 2 and 3 on one card (one tell), and a one-post card on N-451.
    pub(crate) fn two_post_card() -> crate::Proj {
        let mut p = crate::view_cost::project_for_tests();
        let thread = |id: &str, n: u64, posts: serde_json::Value, halts: serde_json::Value| -> coord_types::TicketView {
            serde_json::from_value(json!({"id": id, "n": n, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
                "title": "The lock", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1,
                "opened_at": "2026-11-20T18:20:00Z", "given": [], "posts": posts, "halts": halts}))
            .unwrap()
        };
        let a = thread("N-450", 450, json!([
            {"n": 1, "at": "2026-11-20T18:20:00Z", "by": "paper-research", "text": "The runner's jobs hang on a lock. Which one?", "told": ["owner"]},
            {"n": 2, "at": "2026-11-20T18:25:05Z", "by": "owner", "text": TWO[0]},
            {"n": 3, "at": "2026-11-20T18:25:14Z", "by": "owner", "text": TWO[1]}]),
            json!([{"what": "deliver", "n": 1, "at": "2026-11-20T18:25:20Z", "post": 2, "with": [3], "by": "owner", "wakes": ["paper-research"], "inform": [], "edited": false}]));
        let b = thread("N-451", 451, json!([{"n": 1, "at": "2026-11-20T18:26:00Z", "by": "owner", "text": "one more, alone"}]),
            json!([{"what": "deliver", "n": 1, "at": "2026-11-20T18:26:05Z", "post": 1, "by": "owner", "wakes": ["paper-research"], "inform": [], "edited": false}]));
        for v in [a, b] {
            p.live.tickets.insert(v.n, v);
        }
        p
    }

    /// A tell of two posts is one card at the first, each post's words on it, neither drawn again in the thread.
    #[test]
    fn a_card_that_lets_two_posts_go_holds_both_and_neither_is_drawn_twice() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![two_post_card()];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = Some(450);
        let p = &s.projects[0];
        let rows = crate::threads::seq(p, &p.live.tickets[&450]);
        assert_eq!(rows.iter().filter(|r| matches!(r, crate::threads::TRow::Post(_))).count(), 1, "#1 only: {rows:?}");
        assert_eq!(rows.iter().filter(|r| matches!(r, crate::threads::TRow::Halt(..))).count(), 1, "one card: {rows:?}");
        let h = &p.live.tickets[&450].halts[0];
        assert_eq!((super::on_card(h, 2), super::on_card(h, 3), super::on_card(h, 1)), (Some(true), Some(false), None));
        // the Ran view's lines: #1, and #2 where the card is drawn; #3 is on the card
        let v = &p.live.tickets[&450];
        let cx = crate::ran::Ctx::of(p, v);
        let posts: Vec<u32> = crate::ran::lines(&cx, v).iter().filter_map(|l| match &l.what { crate::ran::What::Post { n, .. } => Some(*n), _ => None }).collect();
        assert_eq!(posts, vec![1, 2]);
        let mut rt = crate::view_cost::window(s);
        for _ in 0..3 {
            rt.frame(0.016);
        }
        let texts: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).collect();
        assert!(rt.find("halt-N-450-1").is_some(), "{texts:?}");
        assert!(texts.iter().any(|x| x.starts_with("halted: posts are about to be delivered") && x.ends_with("· posts 2, 3")), "{texts:?}");
        for (label, words) in [("#2", TWO[0]), ("#3", TWO[1])] {
            assert_eq!(texts.iter().filter(|x| *x == label).count(), 1, "{label}: {texts:?}");
            assert_eq!(texts.iter().filter(|x| x.contains(words)).count(), 1, "{words}, on the card only: {texts:?}");
        }
        assert!(!texts.iter().any(|x| x == "words"), "{texts:?}");
        // a card of one post: "post 1", its words under "words", as before
        rt.update(|s, _| s.tickets.shown = Some(451));
        for _ in 0..3 {
            rt.frame(0.016);
        }
        let texts: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).collect();
        assert!(texts.iter().any(|x| x.starts_with("halted: a post is about to be delivered") && x.ends_with("· post 1")) && texts.iter().any(|x| x == "words"), "{texts:?}");
        assert_eq!(texts.iter().filter(|x| x.contains("one more, alone")).count(), 1, "{texts:?}");
    }

    /// Made-up slowed threads: N-460 with a halted answer, N-461 with a halted policy answer; requests kept in OUT.
    fn halted(out: &std::path::Path) -> (std::sync::MutexGuard<'static, ()>, crate::App, std::process::Child) {
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = two_post_card();
        let thread = |id: &str, n: u64, halt: serde_json::Value| -> coord_types::TicketView {
            serde_json::from_value(json!({"id": id, "n": n, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "ann"}, "title": "A step",
                "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": "2026-11-20T18:20:00Z", "given": [],
                "posts": [{"n": 1, "at": "2026-11-20T18:20:00Z", "by": "owner", "text": "Check it."}], "halts": [halt], "slow": true}))
            .unwrap()
        };
        let ty = json!({"Variant": [["Done", [["ref", "Ref"]]], ["Cannot", [["why", "Text"]]]]});
        let take = json!({"what": "take", "n": 1, "at": "2026-11-20T18:21:00Z", "step": 1, "call": "check", "worker": "ann", "words": "Done: abc123",
            "head": {"head": "Done", "ref": "abc123"}, "type": ty});
        let decide = json!({"what": "decide", "n": 1, "at": "2026-11-20T18:22:00Z", "step": 1, "question": "may ann push", "answer": true, "provenance": {}});
        for v in [thread("N-460", 460, take), thread("N-461", 461, decide)] {
            p.live.tickets.insert(v.n, v);
        }
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = Some(450);
        s.notice = None;
        let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        (held, s, child)
    }

    /// The requests the window sent, in order.
    fn sent(s: &mut crate::App, mut child: std::process::Child, out: &std::path::Path) -> Vec<serde_json::Value> {
        s.projects[0].link = None;
        let _ = child.wait();
        let v = std::fs::read_to_string(out).unwrap_or_default().lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()).map(|v| v["req"].clone()).collect();
        let _ = std::fs::remove_file(out);
        v
    }

    fn key(rt: &mut rune::runtime::Runtime<crate::App>, chord: bool) {
        let mut ev = rune::KeyEvent { key: rune::Key::Enter, mods: Default::default() };
        if chord {
            ev.mods.ctrl = !cfg!(target_os = "macos");
            ev.mods.cmd = cfg!(target_os = "macos");
        }
        rt.key(ev);
        for _ in 0..2 {
            rt.frame(0.016);
        }
    }

    /// A bare Return approves nothing; Ctrl+Return approves only the focused card's halt.
    #[test]
    fn a_halt_goes_by_its_button_or_the_chord_in_its_card_never_by_a_bare_return() {
        let out = std::env::temp_dir().join(format!("yaaihh-slow-keys-{}.jsonl", std::process::id()));
        let (_held, s, child) = halted(&out);
        let mut rt = crate::view_cost::window(s);
        for _ in 0..3 {
            rt.frame(0.016);
        }
        assert!(rt.find("halt-N-450-1").is_some(), "the card drawn");
        key(&mut rt, false);
        rt.update(|_, tx| tx.focus("halt-approve-N-450-1", false));
        rt.frame(0.016);
        key(&mut rt, false);
        let had = rt.state.projects[0].pending.len();
        key(&mut rt, true);
        assert_eq!(rt.state.projects[0].pending.len(), had + 1, "the chord in the card lets it go");
        let child_out = (child, out);
        let reqs = sent(&mut rt.state, child_out.0, &child_out.1);
        let approves: Vec<&serde_json::Value> = reqs.iter().filter(|r| r["op"] == "approve").collect();
        assert_eq!(approves.len(), 1 + had, "{reqs:?}");
        assert_eq!((approves.last().unwrap()["t"].as_str(), approves.last().unwrap()["n"].as_u64()), (Some("N-450"), Some(1)), "{reqs:?}");
    }

    /// A head is edited field by field, typed as the step's answer type says, a constructor picked first; an answer of
    /// yes or no the same way; each as JSON behind "Advanced JSON", as before.
    #[test]
    fn a_head_and_an_answer_are_edited_field_by_field_their_json_behind_advanced() {
        use super::Editing;
        let out = std::env::temp_dir().join(format!("yaaihh-slow-fields-{}.jsonl", std::process::id()));
        let (_held, mut s, child) = halted(&out);
        s.tickets.shown = Some(460);
        s.tickets.slow.editing.insert((460, 1), Editing::Head);
        let mut rt = crate::view_cost::window(s);
        for _ in 0..3 {
            rt.frame(0.016);
        }
        assert!(rt.find("halt-ctor-N-460-1").is_some() && rt.find("halt-field-N-460-1-ref").is_some(), "the constructor and Done's field");
        assert!(rt.find("halt-field-N-460-1-why").is_none() && rt.find("halt-edit-N-460-1").is_none(), "no other field, no JSON box");
        let req = |s: &crate::App, what| serde_json::to_value(super::edit_request(s, "N-460", 460, 1, what, &[]).unwrap()).unwrap();
        assert_eq!(req(&rt.state, Editing::Head)["value"], json!({"head": {"head": "Done", "ref": "abc123"}}), "as it stands");
        rt.update(|s, _| _ = s.tickets.slow.fields.insert((460, 1, "ref".into()), rune::TextEdit::new("def456")));
        assert_eq!(req(&rt.state, Editing::Head)["value"], json!({"head": {"head": "Done", "ref": "def456"}}));
        rt.update(|s, _| _ = s.tickets.slow.ctor.insert((460, 1), "Cannot".into()));
        for _ in 0..2 {
            rt.frame(0.016);
        }
        assert!(rt.find("halt-field-N-460-1-why").is_some() && rt.find("halt-field-N-460-1-ref").is_none(), "Cannot's field");
        rt.update(|s, _| _ = s.tickets.slow.fields.insert((460, 1, "why".into()), rune::TextEdit::new("no lock")));
        assert_eq!(req(&rt.state, Editing::Head)["value"], json!({"head": {"head": "Cannot", "why": "no lock"}}));
        // its JSON, a press away
        let adv = rt.find("halt-advanced-N-460-1").expect("Advanced JSON");
        rt.pointer_down(adv.0, adv.1);
        rt.pointer_up(adv.0, adv.1);
        for _ in 0..2 {
            rt.frame(0.016);
        }
        assert!(rt.find("halt-edit-N-460-1").is_some() && rt.find("halt-field-N-460-1-why").is_none(), "the JSON box in its place");
        rt.update(|s, _| _ = s.tickets.slow.text.insert((460, 1), rune::TextEdit::new("{\"head\": \"Done\", \"ref\": \"0a0a0a\"}")));
        assert_eq!(req(&rt.state, Editing::Head)["value"], json!({"head": {"head": "Done", "ref": "0a0a0a"}}));
        // an answer of yes or no
        rt.update(|s, _| {
            s.tickets.shown = Some(461);
            s.tickets.slow.editing.insert((461, 1), Editing::Answer);
        });
        for _ in 0..3 {
            rt.frame(0.016);
        }
        assert!(rt.find("halt-field-N-461-1-answer").is_some(), "the answer's one field");
        rt.update(|s, _| _ = s.tickets.slow.fields.insert((461, 1, "answer".into()), rune::TextEdit::new("false")));
        assert_eq!(serde_json::to_value(super::edit_request(&rt.state, "N-461", 461, 1, Editing::Answer, &[]).unwrap()).unwrap()["value"], json!({"answer": false}));
        rt.update(|s, _| _ = s.tickets.slow.fields.insert((461, 1, "answer".into()), rune::TextEdit::new("maybe")));
        assert!(super::edit_request(&rt.state, "N-461", 461, 1, Editing::Answer, &[]).unwrap_err().starts_with("answer: not JSON"), "not a yes or no");
        let _ = sent(&mut rt.state, child, &out);
    }

    /// Save & step sends the edit, and the approve only once the engine took it: refused, the edit box stays as he typed
    /// it and nothing is let go.
    #[test]
    fn save_and_step_lets_the_halt_go_only_once_the_edit_is_taken() {
        use super::Editing;
        let out = std::env::temp_dir().join(format!("yaaihh-slow-step-{}.jsonl", std::process::id()));
        let (_held, mut s, child) = halted(&out);
        s.tickets.slow.editing.insert((460, 1), Editing::Head);
        s.tickets.slow.fields.insert((460, 1, "ref".into()), rune::TextEdit::new("def456"));
        super::save(&mut s, "N-460", 460, 1, Editing::Head, &[], true);
        let clear = |step| crate::threads::Cleared::Halt { id: "N-460".into(), tn: 460, hn: 1, step };
        crate::threads::replied(&mut s, crate::Pending::TicketDone { clear: clear(true) }, false, "N-460: ref: no such commit".into());
        assert!(s.tickets.slow.editing.contains_key(&(460, 1)), "refused: the box stays");
        assert_eq!(s.tickets.slow.fields.get(&(460, 1, "ref".into())).map(|e| e.text.as_str()), Some("def456"), "as he typed it");
        super::save(&mut s, "N-460", 460, 1, Editing::Head, &[], true);
        crate::threads::replied(&mut s, crate::Pending::TicketDone { clear: clear(true) }, true, "Edited N-460's halt 1.".into());
        assert!(!s.tickets.slow.editing.contains_key(&(460, 1)), "taken: the box closes");
        let reqs = sent(&mut s, child, &out);
        let ops: Vec<&str> = reqs.iter().filter_map(|r| r["op"].as_str()).collect();
        assert_eq!(ops, ["haltedit", "haltedit", "approve"], "{reqs:?}");
        assert_eq!(reqs[0]["value"], json!({"head": {"head": "Done", "ref": "def456"}}));
    }

    /// Reject opens under the card, closes an open edit, sends HaltReject with the reason; none from an older coordd.
    #[test]
    fn reject_says_what_it_does_then_refuses_the_halt_with_his_reason() {
        use super::Editing;
        let out = std::env::temp_dir().join(format!("yaaihh-slow-reject-{}.jsonl", std::process::id()));
        let (_held, mut s, child) = halted(&out);
        s.projects[0].protocol = coord_types::CONTROL_PROTOCOL - 1;
        s.tickets.shown = Some(460);
        s.tickets.slow.editing.insert((460, 1), Editing::Head);
        let mut rt = crate::view_cost::window(s);
        for _ in 0..3 {
            rt.frame(0.016);
        }
        assert!(rt.find("halt-approve-N-460-1").is_some() && rt.find("halt-reject-N-460-1").is_none(), "an older coordd: no reject");
        rt.update(|s, _| s.projects[0].protocol = coord_types::PROTOCOL);
        for _ in 0..2 {
            rt.frame(0.016);
        }
        crate::view_cost::click(&mut rt, "halt-reject-N-460-1");
        assert!(rt.find("halt-rejecting-N-460-1").is_some(), "what it does, under the card");
        assert!(!rt.state.tickets.slow.editing.contains_key(&(460, 1)), "the edit open there closed");
        assert!(crate::popup::tests::text_under(&rt, "halt-rejecting-N-460-1", "cancels N-460").is_some(), "it says the ticket is cancelled");
        rt.update(|s, _| _ = s.tickets.slow.rejecting.insert((460, 1), rune::TextEdit::new("  the lock is not this job's  ")));
        crate::view_cost::click(&mut rt, "halt-reject-send-N-460-1");
        let clear = || crate::threads::Cleared::Halt { id: "N-460".into(), tn: 460, hn: 1, step: false };
        crate::threads::replied(&mut rt.state, crate::Pending::TicketDone { clear: clear() }, false, "N-460: halt 1 is not held".into());
        assert!(rt.state.tickets.slow.rejecting.contains_key(&(460, 1)), "refused: the box stays as he wrote it");
        crate::threads::replied(&mut rt.state, crate::Pending::TicketDone { clear: clear() }, true, "Refused N-460's halt 1.".into());
        assert!(!rt.state.tickets.slow.rejecting.contains_key(&(460, 1)), "taken: the box closes");
        let reqs = sent(&mut rt.state, child, &out);
        let rejects: Vec<&serde_json::Value> = reqs.iter().filter(|r| r["op"] == "haltreject").collect();
        assert_eq!(rejects, [&json!({"op": "haltreject", "ticket": 460, "halt": 1, "reason": "the lock is not this job's"})], "{reqs:?}");
        assert!(!reqs.iter().any(|r| r["op"] == "approve"), "nothing let go: {reqs:?}");
    }

    /// Stills in `STILL_OUT`: N-460's head edited field by field, and N-460's reject open with a reason (made up).
    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn edit_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn head() -> crate::App {
            let scratch = std::env::temp_dir().join(format!("yaaihh-slow-still-{}.jsonl", std::process::id()));
            let (_held, mut s, _child) = halted(&scratch);
            s.projects[0].link = None;
            s.tickets.shown = Some(460);
            s.tickets.slow.editing.insert((460, 1), super::Editing::Head);
            s.tickets.slow.ctor.insert((460, 1), "Cannot".into());
            s.tickets.slow.fields.insert((460, 1, "why".into()), rune::TextEdit::new("the lock is held by another job"));
            s
        }
        fn reject() -> crate::App {
            let scratch = std::env::temp_dir().join(format!("yaaihh-slow-still-r-{}.jsonl", std::process::id()));
            let (_held, mut s, _child) = halted(&scratch);
            s.projects[0].link = None;
            s.projects[0].protocol = coord_types::PROTOCOL;
            s.tickets.shown = Some(460);
            s.tickets.slow.rejecting.insert((460, 1), rune::TextEdit::new("the lock is not this job's"));
            s
        }
        for (name, setup) in [("halt-head-fields", head as fn() -> crate::App), ("halt-reject", reject)] {
            let snap = out.join(format!("{name}.png")).display().to_string();
            let a: [&str; 6] = ["--snap", &snap, "--size", "1400x900", "--at", "1"];
            let spec = rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), setup, crate::view) };
            rune::App::new(spec).args(a.map(String::from).to_vec()).try_run().unwrap();
        }
    }

    /// The two-post card, made up, in `STILL_OUT`: `STILL_OUT=DIR yaaihh-tests slow::tests::card_stills --ignored
    /// --exact`.
    #[test]
    #[ignore]
    fn card_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn two() -> crate::App {
            let (_held, mut s) = crate::view_cost::test_app();
            s.projects = vec![two_post_card()];
            s.cur = 0;
            s.sel = crate::Sel::Tickets;
            s.tickets.shown = Some(450);
            s.notice = None;
            s
        }
        let script = out.join("card-two.json");
        std::fs::write(&script, "[]").unwrap();
        let snap = out.join("card-two.png").display().to_string();
        let script = script.display().to_string();
        let a: [&str; 8] = ["--snap", &snap, "--size", "1400x900", "--at", "1", "--script", &script];
        let spec = rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), two, crate::view) };
        rune::App::new(spec).args(a.map(String::from).to_vec()).try_run().unwrap();
    }
}

/// A row of the card: its label, and its text (wrapping; MONO: fixed width, as a prompt or a program is).
fn line(label: &str, body: El, k: &Kit) -> El {
    row().w_full().gap(12.0).child(text(label.to_string()).font(k.ty.small_medium).color(k.c.text_muted).w(72.0).min_w(72.0)).child(col().grow(1.0).shrink().min_w(0.0).child(body))
}

/// A row whose label stands above its body, which takes the card's width (a prompt: templates run to 120 columns).
fn wide(label: &str, body: El, k: &Kit) -> El {
    col().w_full().gap(4.0).child(text(label.to_string()).font(k.ty.small_medium).color(k.c.text_muted)).child(col().w_full().min_w(0.0).child(body))
}

/// A row whose label column is W wide (a form's fields, each with its type).
fn line_w(label: &str, body: El, w: f32, k: &Kit) -> El {
    row().w_full().gap(12.0).child(text(label.to_string()).font(k.ty.small_medium).color(k.c.text_muted).w(w).min_w(w)).child(col().grow(1.0).shrink().min_w(0.0).child(body))
}

fn words_el(t: String, mono: bool, k: &Kit) -> El {
    text(t).font(if mono { k.ty.mono } else { k.ty.ui }).color(k.c.text_base).wrap_anywhere()
}

/// Where halt H of T stands, as the owner knows it: (the ticket he knows, what in it). A handler's (the watcher's route
/// of a post) is the thread's; the thread's own is named by its step, a child by its kind.
fn place_of(v: &TicketView, t: &TicketView, h: &ViewHalt) -> (String, String) {
    let handler = v.handlers.iter().any(|x| x.id == t.id);
    if handler {
        return (v.id.clone(), t.kind.clone());
    }
    if t.id != v.id {
        return (t.id.clone(), t.kind.clone());
    }
    // a hole is named by its handler, the one who fills it: the same name on its decision and on the step it gives
    if let Some(n) = target(&t.run, h).filter(|n| n.what == "hole") {
        let handler = match h {
            ViewHalt::Give { worker, .. } | ViewHalt::Take { worker, .. } => Some(worker.clone()),
            ViewHalt::Decide { role, answer, .. } => role.clone().or_else(|| answer.as_str().map(str::to_string)),
            _ => None,
        };
        if let Some(name) = handler.or_else(|| n.who.clone()) {
            return (v.id.clone(), name.strip_prefix("role ").unwrap_or(&name).to_string());
        }
    }
    let what = match h {
        ViewHalt::Give { call, .. } | ViewHalt::Take { call, .. } => call.clone(),
        ViewHalt::Deliver { post, with, .. } if !with.is_empty() => format!("posts {}", std::iter::once(post).chain(with).map(u32::to_string).collect::<Vec<_>>().join(", ")),
        ViewHalt::Deliver { post, .. } => format!("post {post}"),
        ViewHalt::Open { kind, .. } => kind.clone(),
        ViewHalt::Notice { notice, .. } => notice.clone(),
        ViewHalt::Decide { .. } => target(&t.run, h).map(|n| node_name(n)).unwrap_or_else(|| "a step".into()),
    };
    (v.id.clone(), what)
}

/// A node's name in a title: a hole is `hole`, a step its call (an inline step `step`).
fn node_name(n: &RunNode) -> String {
    match n.what.as_str() {
        "step" => call_name(&n.text).unwrap_or_else(|| "step".into()),
        w => w.to_string(),
    }
}

/// Halt H of ticket T, as its card in the thread V.
pub fn card(pg: &Page, p: &Proj, v: &TicketView, t: &TicketView, h: &ViewHalt, first: bool, k: &Kit) -> El {
    let (tn, hn) = (t.n, n_of(h));
    let editing = pg.slow.editing.get(&(tn, hn)).copied();
    // a part of the card apart (popup.rs): its title, and what opens
    let label = crate::refs::ticket(Some(p), t).label;
    let apart = |part: Part, what: &str| (crate::popup::title(&label, &t.id, what), crate::popup::What::Halt { id: t.id.clone(), hn, part });
    let opener = |line: String, (title, what): (String, crate::popup::What)| k.interactive(row().child(text(line).font(k.ty.small).color(k.c.text_accent))).on_press(move |s: &mut App| crate::popup::open(s, title.clone(), what.clone()));
    let (on, what) = place_of(v, t, h);
    // the minder's answer that changes the program says which of the two it is
    let minder = minder_change(h);
    let title = match minder {
        Some(("Program", _)) => format!("halted: the minder's program for the hole  ·  on {on} · {what}"),
        Some(_) => format!("halted: the minder closes the ticket  ·  on {on} · {what}"),
        None => match h {
            ViewHalt::Deliver { chose: Some(who), .. } => format!("halted: the {who} chose whom your words wake  ·  on {on} · {what}"),
            ViewHalt::Notice { to, .. } => format!("halted: {to}'s turn is about to start  ·  on {on}"),
            _ => format!("halted: {}  ·  on {on} · {what}", about(h)),
        },
    };
    let mut c = col()
        .key(format!("halt-{}-{hn}", t.id))
        .w_full()
        .gap(8.0)
        .px(16.0)
        .py(12.0)
        .radius(k.radius.md)
        .fill(k.c.layer_01)
        .border(1.5, k.c.warning.border)
        // the run a rule halted: what is behind it, a press away (outline.rs `behind`)
        .child(row().w_full().gap(8.0).items_start().child(col().grow(1.0).shrink().min_w(0.0).child(text(title).font(k.ty.small_medium).color(k.c.warning.fg).wrap())).child(crate::outline::behind_press(p, v, t, format!("behind-halt-{}-{hn}", t.id), k)));
    // an edit the owner made shows above what it changed
    let edited = |e: bool, body: El| if e { col().gap(2.0).child(text("edited by you").font(k.ty.small).color(k.c.warning.fg)).child(body) } else { body };
    // the edit box stands in the place of what it edits
    let at = |w: Editing, label: &str, body: El| if editing == Some(w) { line(label, editor(pg, tn, hn, t, h, w, k), k) } else { line(label, body, k) };
    match h {
        ViewHalt::Give { call, worker, prompt, edited: e, .. } => {
            c = c.child(at(Editing::Worker, "step", words_el(format!("{call}  →  {worker}"), false, k)));
            c = c.child(if editing == Some(Editing::Prompt) {
                wide("prompt", editor(pg, tn, hn, t, h, Editing::Prompt, k), k)
            } else {
                // its first lines, the whole apart
                let (short, more) = crate::chat::cut(prompt);
                let (title, what) = apart(Part::Prompt, &format!("what {worker} would be sent"));
                wide("prompt", edited(*e, crate::popup::more(col().w_full().gap(4.0).child(words_el(short.into_owned(), true, k)), more, title, what, k)), k)
            });
        }
        ViewHalt::Take { call, worker, words, .. } if minder.is_some() => {
            // what he approves is the program put at the hole, in full, or the result the ticket closes with
            c = c.child(line("step", words_el(format!("{call}, answered by {worker}"), false, k), k));
            let w = serde_json::to_value(words).map(|w| crate::texts::words(pg, &w)).unwrap_or_default();
            if !w.trim().is_empty() {
                c = c.child(line("words", words_el(w, false, k), k));
            }
            match minder {
                Some(("Program", x)) => {
                    let program = crate::texts::words(pg, &x);
                    c = c.child(if editing == Some(Editing::Head) { wide("program", editor(pg, tn, hn, t, h, Editing::Head, k), k) } else { wide("program", hanging(program.trim_end(), k), k) });
                }
                Some((_, x)) => c = c.child(at(Editing::Head, "result", words_el(head_line(pg, &x), false, k))),
                None => {}
            }
        }
        ViewHalt::Take { call, worker, words, head, ty, .. } => {
            c = c.child(line("step", words_el(format!("{call}, answered by {worker}"), false, k), k));
            let w = serde_json::to_value(words).map(|w| crate::texts::words(pg, &w)).unwrap_or_default();
            // an answer with no words (a program's, the watcher's) has no words row
            if !w.trim().is_empty() {
                c = c.child(line("words", words_el(w, false, k), k));
            }
            c = c.child(at(Editing::Head, "head", words_el(head_line(pg, head), false, k)));
            c = c.child(line("one of", words_el(ty.to_string(), true, k), k));
        }
        ViewHalt::Deliver { post, with, by, wakes, inform, edited: e, chose, .. } => {
            let said = |n: &u32| t.posts.iter().find(|x| x.n == *n).map(|x| crate::texts::words(pg, &serde_json::to_value(&x.text).unwrap_or(Value::Null))).unwrap_or_default();
            c = c.child(line("from", words_el(by.clone(), false, k), k));
            if with.is_empty() {
                c = c.child(line("words", words_el(said(post), false, k), k));
            } else {
                // one card lets the tell's posts go together: each post's words under its number
                for n in std::iter::once(post).chain(with) {
                    c = c.child(line(&format!("#{n}"), words_el(said(n), false, k), k));
                }
            }
            c = c.child(at(Editing::Wakes, "wakes", edited(*e, words_el(wakes_line(wakes), false, k))));
            // whom it wakes was an answer's, not a rule's: the card is here because of that choice
            if let Some(who) = chose {
                let says = if who == "watcher" { "the watcher: the words named no one, so it read them and chose whom they wake".to_string() } else { format!("{who}, not a rule") };
                c = c.child(line("chosen by", words_el(says, false, k), k));
            }
            if !inform.is_empty() {
                c = c.child(line("informs", words_el(format!("{}: each reads it at its next turn", inform.join(", ")), false, k), k));
            }
        }
        ViewHalt::Open { ticket, kind, form, fields, .. } => {
            c = c.child(line("ticket", words_el(format!("{ticket}, a {kind}"), false, k), k));
            if editing == Some(Editing::Form) {
                c = c.child(line("form", editor(pg, tn, hn, t, h, Editing::Form, k), k));
            } else {
                // each field's type with its name, the value alone after it
                let labels: Vec<String> = fields.iter().map(|(f, ty)| format!("{f}: {ty}")).collect();
                let w = labels.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32 * k.ty.small_medium.size * 0.55;
                for ((f, _), label) in fields.iter().zip(&labels) {
                    let x = form.get(f).map(|x| crate::texts::words(pg, x)).unwrap_or_else(|| "—".into());
                    // a long text (a design, a part's after) is its first line, the whole apart
                    let long = x.lines().count() > 3 || x.chars().count() > 300;
                    let body = if long {
                        let first = x.lines().map(crate::texts::plain).find(|l| !l.is_empty()).unwrap_or_default();
                        let whole = format!("the whole {f}");
                        col().w_full().gap(4.0).child(words_el(crate::refs::cut(&first, 200), false, k)).child(opener(format!("› {whole}"), apart(Part::Field(f.clone()), &whole)))
                    } else {
                        words_el(x, false, k)
                    };
                    c = c.child(line_w(label, body, w.max(72.0), k));
                }
            }
        }
        ViewHalt::Decide { question, answer, provenance: pv, role, .. } => {
            let q = match role {
                Some(r) => format!("{question} (the {r} role)"),
                None => question.clone(),
            };
            if let Some(n) = target(&t.run, h) {
                c = c.child(line("for", words_el(said(n).unwrap_or_else(|| n.text.clone()), false, k), k));
            }
            c = c.child(line("question", words_el(q, false, k), k));
            c = c.child(at(Editing::Answer, "answer", words_el(crate::texts::words(pg, answer), false, k)));
            c = c.child(line("rule", words_el(provenance(pv), false, k), k));
        }
        ViewHalt::Notice { to, detail, .. } => {
            // what the notice tells its agent, in a sentence; why, when the engine gave a reason; what the step asks
            // behind a click
            let call = |c: &str| call_label(pg, p, c);
            let (says, why) = notice_sentence(pg, to, detail, &call);
            c = c.child(line("tells", words_el(says, false, k), k));
            if let Some(why) = why {
                c = c.child(line("why", words_el(why, false, k), k));
            }
            // the answers that could not be read, each whole apart
            for (i, a) in detail.get("passed").and_then(Value::as_array).into_iter().flatten().enumerate() {
                let by = a.get("by").and_then(Value::as_str).unwrap_or("its agent");
                let what = format!("{by}'s answer that could not be read");
                c = c.child(line("", opener(format!("› {what}"), apart(Part::Passed(i), &what)), k));
            }
            let asked: Vec<(String, String)> = detail.get("args").and_then(Value::as_object).into_iter().flatten().filter(|(_, x)| !x.is_null() && x.as_str() != Some("")).map(|(f, x)| (f.replace('_', " "), crate::texts::words(pg, x))).filter(|(_, x)| !x.trim().is_empty()).collect();
            if !asked.is_empty() {
                let key = format!("notice-{}-{hn}", t.id);
                let open = pg.pic_open.contains(&key);
                let fold = k.interactive(row().child(text(format!("{} what the step gives it", if open { "⌄" } else { "›" })).font(k.ty.small).color(k.c.text_accent))).on_press(move |s: &mut App| {
                    if !s.tickets.pic_open.remove(&key) {
                        s.tickets.pic_open.insert(key.clone());
                    }
                });
                let mut b = col().w_full().gap(4.0).child(fold);
                if open {
                    for (f, x) in &asked {
                        b = b.child(line(f, words_el(x.clone(), false, k), k));
                    }
                }
                c = c.child(line("", b, k));
            }
        }
    }
    // then: what follows, in one line made from the run tree; the program's text from here apart (a post's `wakes`
    // says what follows it)
    if !matches!(h, ViewHalt::Deliver { .. }) {
        // no run tree (a handler's ticket): what follows is not known here, so no line says it; the text apart has it
        let then = match h {
            ViewHalt::Notice { to, detail, .. } => Some(notice_then(to, detail)),
            _ => then_line(t, h),
        };
        if let Some(then) = then {
            c = c.child(line("then", words_el(then, false, k), k));
        }
        if t.continuation.as_deref().is_some_and(|c| !strip_in_flight(c).trim().is_empty()) {
            c = c.child(line("", opener("› the program from here".into(), apart(Part::Program, "the program from here")), k));
        }
    }
    let id = t.id.clone();
    let approve = Button::new("approve").variant(ButtonVariant::Contrast).on_press(move |s: &mut App| approve(s, &id, hn)).build(k).key(format!("halt-approve-{}-{hn}", t.id)).help(format!("Let it go ({CHORD})"));
    let mut acts = row().w_full().gap(8.0).pl(84.0).pt(4.0).center_y().flex_wrap().child(approve);
    // reject, from a coordd that takes it: the first press opens what it does and a reason box under the card
    let rejecting = (p.protocol >= coord_types::CONTROL_PROTOCOL).then(|| pg.slow.rejecting.get(&(tn, hn)));
    if let Some(open) = rejecting {
        acts = acts.child(
            Button::new("reject")
                .variant(if open.is_some() { ButtonVariant::Neutral } else { ButtonVariant::Ghost })
                .help("Refuse it: what halted is not done")
                .on_press(move |s: &mut App| {
                    let st = &mut s.tickets.slow;
                    if st.rejecting.remove(&(tn, hn)).is_none() {
                        st.editing.remove(&(tn, hn));
                        st.rejecting.insert((tn, hn), TextEdit::new(""));
                    }
                })
                .build(k)
                .key(format!("halt-reject-{}-{hn}", t.id)),
        );
    }
    let rejecting = rejecting.flatten();
    for (what, label) in edits(h) {
        let what = *what;
        let start = initial(pg, h, what);
        let open = editing == Some(what);
        acts = acts.child(
            Button::new(*label)
                .variant(if open { ButtonVariant::Neutral } else { ButtonVariant::Ghost })
                .on_press(move |s: &mut App| {
                    let st = &mut s.tickets.slow;
                    if st.editing.get(&(tn, hn)) == Some(&what) {
                        st.editing.remove(&(tn, hn));
                    } else {
                        st.editing.insert((tn, hn), what);
                        st.rejecting.remove(&(tn, hn));
                        st.text.insert((tn, hn), TextEdit::new(start.clone()));
                        // each edit starts from the halt as it stands, not from fields left by an earlier one
                        st.fields.retain(|(a, b, _), _| (*a, *b) != (tn, hn));
                        st.ctor.remove(&(tn, hn));
                    }
                })
                .build(k),
        );
    }
    // the program edit is on the ticket's details: a handler ticket (hidden) has no page, so none
    if p.live.tickets.contains_key(&tn) {
        acts = acts.child(Button::new("edit what comes after").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
            s.tickets.details.insert(tn);
            s.tickets.shown = Some(tn);
        }).build(k));
    }
    // the chord while the card has focus: rejects if reject is open, else saves and steps an open edit, else approves
    let (id, types) = (t.id.clone(), halt_types(h));
    let first_note = (first && rejecting.is_none()).then(|| text(format!("{CHORD} lets it go while this card has focus")).font(k.ty.small).color(k.c.text_faint));
    c.child(acts.children(first_note)).children(rejecting.map(|e| reject_box(t, h, e, k))).on_key(move |s: &mut App, ev: &KeyEvent, _: &mut Tx| {
        if !chord(ev) {
            return false;
        }
        if s.tickets.slow.rejecting.contains_key(&(tn, hn)) {
            reject(s, &id, tn, hn);
            return true;
        }
        match s.tickets.slow.editing.get(&(tn, hn)).copied() {
            Some(what) => save(s, &id, tn, hn, what, &types, true),
            None => crate::slow::approve(s, &id, hn),
        }
        true
    })
}

/// Under a halt card with reject open: what rejecting does, the reason box E, Reject it and Leave it.
fn reject_box(t: &TicketView, h: &ViewHalt, e: &TextEdit, k: &Kit) -> El {
    let (tn, hn) = (t.n, n_of(h));
    // as coord-types' `Request::HaltReject` says it acts on each kind of halt
    let does = match h {
        ViewHalt::Deliver { with, .. } if !with.is_empty() => "Rejecting drops the posts: they reach nobody.".to_string(),
        ViewHalt::Deliver { .. } => "Rejecting drops the post: it reaches nobody.".to_string(),
        ViewHalt::Notice { to, .. } => format!("Rejecting means the notice is never given: it starts no turn of {to}'s."),
        _ => format!("Rejecting cancels {}: what halted is not done, and your reason stays on it.", t.id),
    };
    let id = t.id.clone();
    let key = move |s: &mut App, ev: &KeyEvent, _: &mut Tx| {
        if !chord(ev) {
            return false;
        }
        reject(s, &id, tn, hn);
        true
    };
    let why = TextInput::new(e).label("why").placeholder("why (optional)").width(None).on_key(key).on_edit(move |s: &mut App| s.tickets.slow.rejecting.entry((tn, hn)).or_insert_with(|| TextEdit::new(""))).build(k);
    let id = t.id.clone();
    let send = Button::new("reject it").variant(ButtonVariant::Danger).on_press(move |s: &mut App| reject(s, &id, tn, hn)).build(k).key(format!("halt-reject-send-{}-{hn}", t.id));
    let leave = Button::new("leave it").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| _ = s.tickets.slow.rejecting.remove(&(tn, hn))).build(k);
    col()
        .key(format!("halt-rejecting-{}-{hn}", t.id))
        .w_full()
        .gap(6.0)
        .pl(84.0)
        .child(text(does).font(k.ty.small).color(k.c.danger.fg).wrap())
        .child(col().key(format!("halt-reject-why-{}-{hn}", t.id)).w_full().child(why))
        .child(row().gap(8.0).center_y().child(send).child(leave).child(text(format!("{CHORD} rejects it while this card has focus")).font(k.ty.small).color(k.c.text_faint)))
}

/// Sends `HaltReject` with the typed reason; the reason box closes only once the engine takes it.
pub(crate) fn reject(s: &mut App, id: &str, tn: u64, hn: u32) {
    let reason = s.tickets.slow.rejecting.get(&(tn, hn)).map(|e| e.text.trim().to_string()).filter(|r| !r.is_empty());
    reject_in(s, s.cur, id, tn, hn, reason);
}

/// Halt HN of ticket TN (called ID) of project PI refused, for REASON.
pub(crate) fn reject_in(s: &mut App, pi: usize, id: &str, tn: u64, hn: u32, reason: Option<String>) {
    let req = Request::HaltReject { ticket: tn, halt: hn, reason };
    s.request_in(pi, req, Pending::TicketDone { clear: Cleared::Halt { id: id.to_string(), tn, hn, step: false } });
}

/// The chord that lets a halt go, as its card says it.
const CHORD: &str = if cfg!(target_os = "macos") { "⌘Return" } else { "Ctrl+Return" };

/// Whether EV is [`CHORD`]: Return with Ctrl (⌘ on a Mac), nothing else held.
fn chord(ev: &KeyEvent) -> bool {
    matches!(ev.key, rune::Key::Enter) && (ev.mods.ctrl || ev.mods.cmd) && !ev.mods.shift && !ev.mods.alt
}

/// The fields with their types that H's edit takes field by field: an open's form.
fn halt_types(h: &ViewHalt) -> Vec<(String, Type)> {
    match h {
        ViewHalt::Open { fields, .. } => fields.clone(),
        _ => Vec::new(),
    }
}

/// What an edit starts from: the prompt, the worker, the head or answer as JSON, who is woken.
fn initial(pg: &Page, h: &ViewHalt, what: Editing) -> String {
    let _ = pg;
    match (h, what) {
        (ViewHalt::Give { prompt, .. }, Editing::Prompt) => prompt.clone(),
        (ViewHalt::Give { worker, .. }, Editing::Worker) => worker.clone(),
        (ViewHalt::Take { head, .. }, Editing::Head) => serde_json::to_string_pretty(head).unwrap_or_default(),
        (ViewHalt::Deliver { wakes, .. }, Editing::Wakes) => wakes.join(", "),
        (ViewHalt::Decide { answer, .. }, Editing::Answer) => serde_json::to_string(answer).unwrap_or_default(),
        _ => String::new(),
    }
}

/// The fields a head or an answer is edited by, where its type allows: for a head, each constructor of the step's
/// answer type with its fields (a record's: one with no name); for an answer, one field, `answer`, typed as the answer
/// is (yes or no, a name). None: it is edited as JSON only.
fn fields_of(h: &ViewHalt, what: Editing) -> Option<Vec<(String, Vec<(String, Type)>)>> {
    match (h, what) {
        (ViewHalt::Take { ty: Type::Variant(cs), .. }, Editing::Head) if !cs.is_empty() => Some(cs.clone()),
        (ViewHalt::Take { ty: Type::Record(fs), .. }, Editing::Head) => Some(vec![(String::new(), fs.clone())]),
        (ViewHalt::Decide { answer: Value::Bool(_), .. }, Editing::Answer) => Some(vec![(String::new(), vec![("answer".into(), Type::Bool)])]),
        (ViewHalt::Decide { answer: Value::String(_), .. }, Editing::Answer) => Some(vec![(String::new(), vec![("answer".into(), Type::Text)])]),
        _ => None,
    }
}

/// The value H holds now that its edit field by field starts from: the head (`{"head": C, field: value}`) or the
/// answer as `{"answer": value}`.
fn now_of(h: &ViewHalt) -> Value {
    match h {
        ViewHalt::Take { head, .. } => head.clone(),
        ViewHalt::Decide { answer, .. } => serde_json::json!({ "answer": answer }),
        _ => Value::Null,
    }
}

/// A field's first text: its value V's words, a text as it is.
fn field_text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(x) => x.to_string(),
    }
}

/// TEXT typed into a field of type TY as the value it sends: a text, a ref, an agent or a ticket as it is; an option
/// left empty none; anything else read as JSON (a number, true, a list).
fn typed(ty: &Type, text: &str) -> Result<Value, String> {
    match ty {
        Type::Text | Type::Ref | Type::Agent | Type::Ticket => Ok(Value::String(text.to_string())),
        Type::Opt(_) if text.trim().is_empty() => Ok(Value::Null),
        Type::Opt(inner) => typed(inner, text),
        _ => serde_json::from_str::<Value>(text.trim()).map_err(|e| format!("not JSON: {e}")),
    }
}

/// The constructor halt (TN, HN)'s head edit stands on: the one picked, else the head's own, else the first of CS.
fn ctor_of(pg: &Page, tn: u64, hn: u32, now: &Value, cs: &[(String, Vec<(String, Type)>)]) -> String {
    pg.slow.ctor.get(&(tn, hn)).cloned().or_else(|| now.get("head").and_then(Value::as_str).map(str::to_string)).or_else(|| cs.first().map(|c| c.0.clone())).unwrap_or_default()
}

/// The edit box under a card: a text for the prompt, a name, names for who is woken, a line per field for a form, and
/// for a head or an answer a line per field of its type ([`fields_of`]), its JSON behind "Advanced JSON". `save &
/// step` (Ctrl+Return in it) saves the edit and, once the engine took it, lets the halt go; `save the edit` saves it
/// and the card stays.
#[allow(clippy::too_many_arguments)]
fn editor(pg: &Page, tn: u64, hn: u32, t: &TicketView, h: &ViewHalt, what: Editing, k: &Kit) -> El {
    let mut c = col().w_full().gap(6.0);
    let fields = fields_of(h, what);
    let raw = fields.is_none() || pg.slow.raw.contains(&(tn, hn));
    let hint = match what {
        Editing::Prompt => "The prompt as it will be sent.",
        Editing::Worker => "The agent who gets the step.",
        Editing::Head if raw => "The head as JSON: {\"head\": CONSTRUCTOR, field: value, …}; it must type as shown.",
        Editing::Head => "The head, field by field; each must type as shown.",
        Editing::Wakes => "The agents it wakes, by name, separated by commas.",
        Editing::Form => "The form's values; each must type as shown.",
        Editing::Answer if raw => "The answer as JSON (true, false, a name in quotes, …).",
        Editing::Answer => "The answer.",
    };
    c = c.child(text(hint).font(k.ty.small).color(k.c.text_muted).wrap());
    let id = t.id.clone();
    let types = halt_types(h);
    // the chord in a field of the edit: saved and let go
    let step_key = {
        let (id, types) = (id.clone(), types.clone());
        move |s: &mut App, ev: &KeyEvent, _: &mut Tx| {
            if !chord(ev) {
                return false;
            }
            save(s, &id, tn, hn, what, &types, true);
            true
        }
    };
    let input = |key: (u64, u32, String), label: String, start: String| {
        let e = pg.slow.fields.get(&key).cloned().unwrap_or_else(|| TextEdit::new(start.clone()));
        let name = key.2.clone();
        col().key(format!("halt-field-{}-{hn}-{name}", t.id)).w_full().child(
            TextInput::new(&e).label(label).width(None).on_key(step_key.clone()).on_edit(move |s: &mut App| s.tickets.slow.fields.entry(key.clone()).or_insert_with(|| TextEdit::new(start.clone()))).build(k),
        )
    };
    match (what, h, &fields) {
        (Editing::Form, ViewHalt::Open { form, fields, .. }, _) => {
            for (f, ty) in fields {
                c = c.child(input((tn, hn, f.clone()), format!("{f} : {ty}"), field_text(form.get(f))));
            }
        }
        (Editing::Head | Editing::Answer, _, Some(cs)) if !raw => {
            let now = now_of(h);
            let ctor = ctor_of(pg, tn, hn, &now, cs);
            // a choice of constructors: picked first, its fields under it
            if cs.len() > 1 {
                let names: Vec<String> = cs.iter().map(|c| c.0.clone()).collect();
                let at = names.iter().position(|n| *n == ctor);
                let pick = rune_kit::SegmentedControl::new(&names, at).fit().label("constructor").on_pick(move |s: &mut App, i, _| {
                    if let Some(n) = s.proj().and_then(|p| halt_of(p, &id_for(p, tn), hn)).and_then(|(_, h)| fields_of(h, what)).and_then(|cs| cs.get(i).map(|c| c.0.clone())) {
                        s.tickets.slow.ctor.insert((tn, hn), n);
                    }
                });
                c = c.child(col().key(format!("halt-ctor-{}-{hn}", t.id)).w_full().child(pick.build(k)));
            }
            let same = now.get("head").and_then(Value::as_str).is_none_or(|c| c == ctor);
            for (f, ty) in cs.iter().find(|c| c.0 == ctor).map(|c| c.1.as_slice()).unwrap_or_default() {
                let start = if same { field_text(now.get(f)) } else { String::new() };
                c = c.child(input((tn, hn, f.clone()), format!("{f} : {ty}"), start));
            }
        }
        _ => {
            let e = pg.slow.text.get(&(tn, hn)).cloned().unwrap_or_else(|| TextEdit::new(""));
            let rows = if matches!(what, Editing::Prompt | Editing::Head) { (4, 24) } else { (1, 4) };
            c = c.child(col().key(format!("halt-edit-{}-{hn}", t.id)).w_full().child(
                TextArea::new(&e).width(None).spellcheck(what == Editing::Prompt).auto_grow(rows.0, rows.1).on_key(step_key.clone()).on_edit(move |s: &mut App| s.tickets.slow.text.entry((tn, hn)).or_insert_with(|| TextEdit::new(""))).build(k),
            ));
        }
    }
    let step = {
        let (id, types) = (id.clone(), types.clone());
        Button::new("save & step").variant(ButtonVariant::Contrast).on_press(move |s: &mut App| save(s, &id, tn, hn, what, &types, true)).build(k).help(format!("Save and let it go ({CHORD})"))
    };
    let save = Button::new("save the edit").variant(ButtonVariant::Neutral).on_press(move |s: &mut App| save(s, &id, tn, hn, what, &types, false)).build(k);
    let cancel = Button::new("leave it").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| _ = s.tickets.slow.editing.remove(&(tn, hn))).build(k);
    let mut acts = row().w_full().gap(8.0).center_y().child(step).child(save).child(cancel);
    // a head or an answer with fields: its JSON a press away, and back
    if fields.is_some() {
        let advanced = Button::new(if raw { "Fields" } else { "Advanced JSON" }).variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
            let st = &mut s.tickets.slow;
            if !st.raw.remove(&(tn, hn)) {
                st.raw.insert((tn, hn));
            }
        });
        acts = acts.child(row().grow(1.0)).child(advanced.build(k).key(format!("halt-advanced-{}-{hn}", t.id)));
    }
    c.child(acts)
}

/// The id of ticket number TN in P, a handler's too.
fn id_for(p: &Proj, tn: u64) -> String {
    p.live.tickets.values().chain(p.live.tickets.values().flat_map(|t| t.handlers.iter())).find(|v| v.n == tn).map(|v| v.id.clone()).unwrap_or_default()
}

/// The edit typed for halt HN of ticket ID (number TN) sent as `Request::HaltEdit`; refused here when it does not type.
/// STEP: once the engine took it, the halt is let go ([`Cleared::Halt`]); the edit box closes only then, so a refused
/// edit stays as he typed it.
fn save(s: &mut App, id: &str, tn: u64, hn: u32, what: Editing, types: &[(String, Type)], step: bool) {
    match edit_request(s, id, tn, hn, what, types) {
        Err(e) => s.tickets.said = Some(Err(e)),
        Ok(req) => _ = s.request_in(s.cur, req, Pending::TicketDone { clear: Cleared::Halt { id: id.to_string(), tn, hn, step } }),
    }
}

/// The request "save the edit" sends for halt HN of ticket ID (number TN).
pub fn edit_request(s: &App, id: &str, tn: u64, hn: u32, what: Editing, types: &[(String, Type)]) -> Result<Request, String> {
    let typed_text = s.tickets.slow.text.get(&(tn, hn)).map(|e| e.text.clone()).unwrap_or_default();
    let json = |t: &str| serde_json::from_str::<Value>(t.trim()).map_err(|e| format!("not JSON: {e}"));
    let field = |f: &str| s.tickets.slow.fields.get(&(tn, hn, f.to_string())).map(|e| e.text.clone());
    // a head's or an answer's fields, where it is edited by them
    let halt = s.proj().and_then(|p| halt_of(p, id, hn)).map(|(_, h)| h);
    let by_fields = halt.and_then(|h| Some((h, fields_of(h, what)?))).filter(|_| !s.tickets.slow.raw.contains(&(tn, hn)));
    let value = match (what, by_fields) {
        (Editing::Head | Editing::Answer, Some((h, cs))) => {
            let now = now_of(h);
            let ctor = ctor_of(&s.tickets, tn, hn, &now, &cs);
            let same = now.get("head").and_then(Value::as_str).is_none_or(|c| c == ctor);
            let mut out = serde_json::Map::new();
            if !ctor.is_empty() {
                out.insert("head".into(), Value::String(ctor.clone()));
            }
            for (f, ty) in cs.iter().find(|c| c.0 == ctor).map(|c| c.1.as_slice()).unwrap_or_default() {
                let t = field(f).unwrap_or_else(|| if same { field_text(now.get(f)) } else { String::new() });
                out.insert(f.clone(), typed(ty, &t).map_err(|e| format!("{f}: {e}"))?);
            }
            Ok(match what {
                Editing::Answer => serde_json::json!({"answer": out.remove("answer").unwrap_or(Value::Null)}),
                _ => serde_json::json!({"head": out}),
            })
        }
        (Editing::Prompt, _) => Ok(serde_json::json!({"prompt": typed_text})),
        (Editing::Worker, _) => Ok(serde_json::json!({"worker": typed_text.trim()})),
        (Editing::Head, _) => json(&typed_text).map(|h| serde_json::json!({"head": h})),
        (Editing::Answer, _) => json(&typed_text).map(|a| serde_json::json!({"answer": a})),
        (Editing::Wakes, _) => Ok(serde_json::json!({"wakes": typed_text.split(',').map(str::trim).filter(|n| !n.is_empty()).collect::<Vec<_>>()})),
        (Editing::Form, _) => {
            let mut form = serde_json::Map::new();
            for (f, ty) in types {
                let Some(t) = field(f) else { continue };
                form.insert(f.clone(), typed(ty, &t).map_err(|e| format!("{f}: {e}"))?);
            }
            Ok(serde_json::json!({"form": form}))
        }
    };
    value.map(|value| Request::HaltEdit { t: id.to_string(), n: hn, value })
}

pub(crate) fn approve(s: &mut App, id: &str, n: u32) {
    approve_in(s, s.cur, id, n);
}

/// Halt N of the ticket called ID in project PI approved: it goes on as it stands.
pub(crate) fn approve_in(s: &mut App, pi: usize, id: &str, n: u32) {
    let req = Request::Approve { t: id.to_string(), n };
    s.request_in(pi, req, Pending::TicketDone { clear: Cleared::Quiet });
}

/// The halt of slow mode that waits to go on to AGENT in P, the oldest: a step given to it, a post that wakes it, a
/// notice to it; by its ticket's id and its number. None when none waits (the Inspect drawer's Apply & Step steps it).
pub(crate) fn waiting_on(p: &Proj, agent: &str) -> Option<(String, u32)> {
    let all = p.live.tickets.values().chain(p.live.tickets.values().flat_map(|t| t.handlers.iter()));
    all.flat_map(|t| t.halts.iter().filter(|h| goes_to(h).contains(&agent)).map(move |h| (at_of(h), t.id.clone(), n_of(h)))).min().map(|(_, id, n)| (id, n))
}

/// The agents halt H goes on to once let go: the worker a step is given to, those a post wakes, the one a notice is
/// to; none for the others.
pub(crate) fn goes_to(h: &ViewHalt) -> Vec<&str> {
    match h {
        ViewHalt::Give { worker, .. } => vec![worker.as_str()],
        ViewHalt::Deliver { wakes, .. } => wakes.iter().map(String::as_str).collect(),
        ViewHalt::Notice { to, .. } => vec![to.as_str()],
        _ => Vec::new(),
    }
}

/// A printed continuation without the in-flight line an edit starts from (`answer <- (in flight);`) and its comments.
/// Call C by its doc's first line with its name after it (`who plans each part (assign_parts)`), else its name.
fn call_label(pg: &Page, p: &Proj, c: &str) -> String {
    let m = pg.labels.lock().unwrap_or_else(|e| e.into_inner());
    match m.get(&p.id).and_then(|l| l.calls.get(c)) {
        Some(l) => format!("{l} ({c})"),
        None => c.replace('_', " "),
    }
}

/// Posts handed on unread, as `2 posts it has not read, by orch and you`.
fn unread_words(d: &Value) -> Option<String> {
    let u = d.get("unread").and_then(Value::as_array).filter(|u| !u.is_empty())?;
    let mut by: Vec<String> = Vec::new();
    for p in u {
        let b = p.get("by").and_then(Value::as_str).unwrap_or("someone");
        let b = crate::refs::person(b);
        if !by.contains(&b) {
            by.push(b);
        }
    }
    let n = u.len();
    Some(format!("{} it has not read, by {}", if n == 1 { "a post".to_string() } else { format!("{n} posts") }, by.join(" and ")))
}

/// Who answered a step passed up (its `passed` answers' authors), as `designer` or `designer and writer`.
fn passed_by(d: &Value) -> Option<String> {
    let ps = d.get("passed").and_then(Value::as_array).filter(|p| !p.is_empty())?;
    let mut by: Vec<String> = Vec::new();
    for p in ps {
        let b = p.get("by").and_then(Value::as_str).unwrap_or("its agent").to_string();
        if !by.contains(&b) {
            by.push(b);
        }
    }
    Some(by.join(" and "))
}

/// What notice D tells agent TO, as a sentence, and the reason given for it, if any.
fn notice_sentence(pg: &Page, to: &str, d: &Value, call: &dyn Fn(&str) -> String) -> (String, Option<String>) {
    let s = |f: &str| d.get(f).and_then(Value::as_str).unwrap_or_default().to_string();
    let step = d.get("step").and_then(Value::as_u64).map(|n| format!("step {n}")).unwrap_or_else(|| "its step".into());
    let passed_why: Vec<String> = d.get("passed").and_then(Value::as_array).into_iter().flatten().filter_map(|p| p.get("why").and_then(Value::as_str)).filter(|w| !w.trim().is_empty()).map(str::to_string).collect();
    let why = d.get("why").and_then(Value::as_str).filter(|w| !w.trim().is_empty()).map(str::to_string).or_else(|| (!passed_why.is_empty()).then(|| passed_why.join("\n")));
    let with = unread_words(d).map(|u| format!(", with {u}")).unwrap_or_default();
    let said = match s("what").as_str() {
        // passed up: the step's agent answered and its answers could not be read, so the step is TO's now
        "Step" | "step" if passed_by(d).is_some() => format!("{step}, {}, is passed up to {to}: {} answered it, and the answers could not be read{with}", call(&s("call")), passed_by(d).unwrap_or_default()),
        "Step" | "step" if d.get("again").and_then(Value::as_bool) == Some(true) => format!("{to} still owes {step}: {}{with}", call(&s("call"))),
        "Step" | "step" => format!("{to} is given {step}: {}{with}", call(&s("call"))),
        "Closed" | "closed" => match (d.get("result").filter(|x| !x.is_null()), d.get("raised").filter(|x| !x.is_null())) {
            (_, Some(r)) => format!("the ticket closed failed: {}{with}", r.get("exc").and_then(Value::as_str).unwrap_or("an exception")),
            (Some(r), _) => format!("the ticket closed: {}{with}", head_line(pg, r)),
            _ => format!("the ticket closed{with}"),
        },
        "Withdrawn" | "withdrawn" => {
            let by = d.get("by").and_then(Value::as_str).map(|b| if b == "owner" { " by you".to_string() } else { format!(" by {b}") }).unwrap_or_default();
            format!("{step}, {}, is taken back from {to}{by}: an answer to it now is refused", call(&s("call")))
        }
        "Refused" | "refused" => {
            let left = d.get("left").and_then(Value::as_u64).unwrap_or(0);
            let tries = match left {
                0 => "the step fails at the next".to_string(),
                1 => "one more try before the step fails".to_string(),
                n => format!("{n} more tries before the step fails"),
            };
            return (format!("{to}'s answer to {step}, {}, is refused; {tries}", call(&s("call"))), Some(s("why")).filter(|w| !w.is_empty()));
        }
        "Words" | "words" => {
            let w = d.get("text").map(|x| crate::texts::words(pg, x)).unwrap_or_default();
            let first = w.lines().find(|l| !l.trim().is_empty()).unwrap_or("").to_string();
            format!("words from {}: {}", s("from"), crate::refs::cut(&first, 160))
        }
        "Posts" | "posts" => unread_words(d).map(|u| format!("{to} is woken by {u}")).unwrap_or_else(|| format!("{to} is woken by posts that name it")),
        _ => format!("{to} is told something the page cannot say in words ({})", s("what")),
    };
    (said, why)
}

/// What follows notice D once approved: TO's turn starts with it, and what TO is to do in that turn.
fn notice_then(to: &str, d: &Value) -> String {
    let what = d.get("what").and_then(Value::as_str).unwrap_or_default().to_lowercase();
    match what.as_str() {
        "step" => format!("{to}'s turn starts with it, and {to} answers the step"),
        "refused" => format!("{to}'s turn starts with it, and {to} answers the step again"),
        "words" => format!("{to}'s turn starts with the words; nothing answers them"),
        _ => format!("{to}'s turn starts with it; it owes nothing for it"),
    }
}

/// Who a post wakes, and what follows: each woken agent's turn starts with it.
fn wakes_line(wakes: &[String]) -> String {
    let agents: Vec<&str> = wakes.iter().map(String::as_str).filter(|w| *w != "owner").collect();
    match (agents.is_empty(), wakes.iter().any(|w| w == "owner")) {
        (true, true) => "only you: no agent's turn starts".into(),
        (true, false) => "nobody: no turn starts".into(),
        (false, you) => format!("{}: {} turn starts with it{}", agents.join(", "), if agents.len() == 1 { "its" } else { "each one's" }, if you { "; and you" } else { "" }),
    }
}

/// PROGRAM in fixed width, each line at its indent and its wrapped rest under its own start, not at the margin.
fn hanging(program: &str, k: &Kit) -> El {
    let cw = k.ty.mono.size * 0.6;
    col().w_full().gap(1.0).children(program.lines().map(|l| {
        let n = l.len() - l.trim_start().len();
        row().w_full().pl(cw * n as f32).child(text(l.trim_start().to_string()).font(k.ty.mono).color(k.c.text_base).wrap_anywhere().grow(1.0).w(0.0))
    }))
}

// ---- then: what follows a halt, in one line ------------------------------------------------------------------------

/// A step's call as written (`v <- step role runner check(c, none)` is `check`); an inline step has none.
fn call_name(t: &str) -> Option<String> {
    let t = t.split_once(" <- ").map_or(t, |(_, r)| r);
    let rest = t.strip_prefix("step ")?;
    let rest = rest.strip_prefix("role ").unwrap_or(rest);
    let rest = rest.split_once(' ').map_or("", |(_, r)| r).trim_start();
    if rest.starts_with('"') || rest.is_empty() {
        return None;
    }
    Some(rest.split(['(', ' ']).next().unwrap_or("").to_string())
}

/// The step halt H is about (its round, or the step at work, or the next to come).
fn target<'a>(run: &'a [RunNode], h: &ViewHalt) -> Option<&'a RunNode> {
    path(run, h).last().map(|(ns, i)| &ns[*i])
}

fn step_of(h: &ViewHalt) -> Option<u32> {
    match h {
        ViewHalt::Give { step, .. } | ViewHalt::Take { step, .. } | ViewHalt::Decide { step, .. } => Some(*step),
        _ => None,
    }
}

/// From the top of RUN down to the node halt H is at: each as (its siblings, its place among them).
fn path<'a>(run: &'a [RunNode], h: &ViewHalt) -> Vec<(&'a [RunNode], usize)> {
    fn find<'a>(ns: &'a [RunNode], hit: &dyn Fn(&RunNode) -> bool, out: &mut Vec<(&'a [RunNode], usize)>) -> bool {
        for (i, n) in ns.iter().enumerate() {
            out.push((ns, i));
            if hit(n) || find(&n.nodes, hit, out) {
                return true;
            }
            out.pop();
        }
        false
    }
    let s = step_of(h);
    let mut out = Vec::new();
    let leaf = |n: &RunNode| matches!(n.what.as_str(), "step" | "hole");
    if find(run, &|n| leaf(n) && s.is_some_and(|s| n.rounds.iter().any(|r| r.step == s)), &mut out)
        || find(run, &|n| leaf(n) && n.state == "now", &mut out)
        || find(run, &|n| leaf(n) && n.state == "to_come", &mut out)
    {
        return out;
    }
    Vec::new()
}

/// Node N in the words "how it runs" uses: a step its call and who; a match its arms by constructor; a return its
/// constructor; a raise its exception.
fn said(n: &RunNode) -> Option<String> {
    let who = n.who.clone().unwrap_or_default();
    let who = who.strip_prefix("role ").unwrap_or(&who).to_string();
    Some(match n.what.as_str() {
        "step" => match call_name(&n.text) {
            Some(c) => format!("{c} → {who}"),
            None => format!("{} → {who}", inline_words(&n.text)),
        },
        "match" => {
            let arms: Vec<String> = n.nodes.iter().map(|a| {
                let c = a.text.split(['(', ' ']).next().unwrap_or(&a.text).to_string();
                let body = seq(&a.nodes);
                if body.is_empty() { c } else { format!("{c}: {body}") }
            }).collect();
            arms.join(" · ")
        }
        "return" => {
            let e = n.text.trim_start_matches("return").trim();
            match e.split(['(', ' ']).next().filter(|c| c.chars().next().is_some_and(char::is_uppercase)) {
                Some(c) => c.to_lowercase(),
                None => format!("returns {e}"),
            }
        }
        "raise" => format!("raises {}", n.text.trim_start_matches("raise").trim().split(['(', ' ']).next().unwrap_or("")),
        "open" => format!("opens {}", n.text.split_once("open ").map(|(_, r)| r.split(['(', ' ']).next().unwrap_or("")).unwrap_or("")),
        "iterate" | "for" => format!("{}, {}", seq(&n.nodes), bound(&n.text).map_or("again".into(), |b| format!("up to {b} rounds"))),
        "hole" => "rests for the minder".into(),
        "post" | "tell" => n.text.split_whitespace().next().map(|w| format!("{w}s{}", if who.is_empty() { String::new() } else { format!(" {who}") })).unwrap_or_default(),
        "let" | "with" => return (!n.nodes.is_empty()).then(|| seq(&n.nodes)),
        _ => n.text.clone(),
    })
}

fn seq(ns: &[RunNode]) -> String {
    ns.iter().filter_map(said).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")
}

/// An iterate's bound: `up to N`.
fn bound(t: &str) -> Option<u32> {
    t.split_once("up to ").and_then(|(_, r)| r.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok())
}

/// What follows halt H of T, in one line: the rest of its block, then of each block around it, with the round of
/// each loop it is in and its bound. An open halt names the kind and its first step.
pub(crate) fn then_line(t: &TicketView, h: &ViewHalt) -> Option<String> {
    if t.run.is_empty() {
        return None;
    }
    if let ViewHalt::Open { kind, form, .. } = h {
        // its worker as written is a field of the form: the form's value names it
        let first = path(&t.run, h).last().map(|(ns, i)| &ns[*i]).and_then(said).map(|f| match f.rsplit_once(" → ") {
            Some((a, w)) => match form.get(w).and_then(Value::as_str) {
                Some(x) => format!("{a} → {x}"),
                None => f,
            },
            None => f,
        });
        return Some(match first {
            Some(f) => format!("a {kind}: first {f}"),
            None => format!("a {kind}"),
        });
    }
    let p = path(&t.run, h);
    if p.is_empty() {
        return Some("nothing more: the ticket ends".into());
    }
    // at a hole what follows is what its handler writes there: not known yet, so no line says it
    if p.last().is_some_and(|(ns, i)| ns[*i].what == "hole") {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    // a give or a decision comes before its step: the step is what follows first
    let (ns, i) = p[p.len() - 1];
    if matches!(h, ViewHalt::Give { .. } | ViewHalt::Decide { .. }) {
        // a decision is for a step that then runs; a hole's decision (who shapes it) is named in its "for" row
        if let Some(s) = said(&ns[i]).filter(|_| matches!(h, ViewHalt::Decide { .. }) && ns[i].what == "step") {
            parts.push(s);
        }
    }
    // a take follows the head formed: of the match on its value, the arm it takes; an arm that returns or raises ends
    // the ticket there
    let mut from = i + 1;
    if let ViewHalt::Take { head, .. } = h {
        let bound_to = ns[i].text.split_once(" <- ").map(|(n, _)| n.trim().to_string());
        let c = head.get("head").and_then(Value::as_str);
        if let (Some(m), Some(name), Some(c)) = (ns.get(i + 1), bound_to, c) {
            let subject = m.text.split_once(" <- ").map_or(m.text.as_str(), |(_, r)| r).trim_start_matches("match").trim();
            if m.what == "match" && subject == name {
                let arm = m.nodes.iter().find(|a| a.text.split(['(', ' ']).next() == Some(c)).or_else(|| m.nodes.iter().find(|a| a.text.trim() == "_"));
                if let Some(arm) = arm {
                    let body = seq(&arm.nodes);
                    if !body.is_empty() {
                        parts.push(body);
                    }
                    if arm.nodes.last().is_some_and(|n| matches!(n.what.as_str(), "return" | "raise")) {
                        return Some(if parts.is_empty() { "nothing more: the ticket ends".into() } else { parts.join(" · ") });
                    }
                    from = i + 2;
                }
            }
        }
    }
    let rest = seq(&ns[from..]);
    if !rest.is_empty() {
        parts.push(rest);
    }
    let fresh = matches!(h, ViewHalt::Give { .. } | ViewHalt::Decide { .. }) && step_of(h).is_some_and(|s| !ns[i].rounds.iter().any(|r| r.step == s));
    for (ns, i) in p[..p.len() - 1].iter().rev() {
        let n = &ns[*i];
        // the other arms of a match (branches, handlers) do not follow the one taken
        if matches!(n.what.as_str(), "arm" | "branch" | "handle") {
            continue;
        }
        if matches!(n.what.as_str(), "iterate" | "for") {
            let first_rounds = first_step(&n.nodes).map_or(0, |s| s.rounds.len());
            let round = (first_rounds + usize::from(fresh && first_step(&n.nodes).is_some_and(|s| std::ptr::eq(s, &p[p.len() - 1].0[p[p.len() - 1].1])))).max(1);
            let last = parts.pop().unwrap_or_default();
            let r = match bound(&n.text) {
                Some(b) => format!("round {round} of {b}"),
                None => format!("round {round}"),
            };
            parts.push(if last.is_empty() { r } else { format!("{last}, {r}") });
        }
        let rest = seq(&ns[i + 1..]);
        if !rest.is_empty() {
            parts.push(rest);
        }
    }
    Some(if parts.is_empty() { "nothing more: the ticket ends".into() } else { parts.join(" · ") })
}

/// An inline step's words, their first five, quoted: `"Implement this in the repository …"`.
fn inline_words(t: &str) -> String {
    let w = t.split_once('"').map_or("", |(_, r)| r);
    let w = w.split('"').next().unwrap_or("");
    let ws: Vec<&str> = w.split_whitespace().collect();
    if ws.is_empty() {
        return "a step".into();
    }
    format!("\u{201c}{}{}\u{201d}", ws[..ws.len().min(5)].join(" "), if ws.len() > 5 { " …" } else { "" })
}

/// The first step down NS.
fn first_step(ns: &[RunNode]) -> Option<&RunNode> {
    ns.iter().find_map(|n| if n.what == "step" { Some(n) } else { first_step(&n.nodes) })
}

fn strip_in_flight(c: &str) -> String {
    c.lines().filter(|l| !l.trim_start().starts_with("//") && !l.contains("(in flight)")).collect::<Vec<_>>().join("\n").trim().to_string()
}
