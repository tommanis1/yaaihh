//! Live edits through coordd's reload (paper-research N-101 p424): the reload's answer says, one line an open ticket of
//! an edited kind, whether it took the new text or kept its old text and why; each that took it has its `edited` line;
//! a kind's form, result or raises does not change while tickets of it are open, and the refusal names them, nor while
//! an open ticket would still open one as it was (N-101 p460); a restart on kind files changed again restores every
//! ticket from the record. The answer is docket-edge's shape (N-235): `{said, kinds, calls, tickets: [{id, kind, took,
//! said | why, withdrawn}]}`.

use super::Core;
use super::edit_tests::{KIND, core, log, prompt};
use crate::core::Who;
use docket_machine::{Input, Line, Status};
use serde_json::{Value, json};

fn answer(core: &mut Core, t: u64, text: &str) {
    core.ticket_input(Input::Answer { t, by: "w2".into(), head: json!({"head": "Said", "text": text}), text: docket_machine::Text::New(text.into()) }).unwrap();
}

fn open(core: &mut Core, q: &str) -> u64 {
    open_a(core, "e", q)
}

fn open_a(core: &mut Core, kind: &str, q: &str) -> u64 {
    core.ticket_input(Input::Open { kind: kind.into(), under: None, opener: "owner".into(), form: json!({"q": q}), text: None, names: vec![], told: vec![] }).unwrap();
    *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0
}

#[test]
fn a_reload_carries_the_open_tickets_and_says_what_each_did() {
    let (mut core, coord) = core("live");
    let kind = coord.join("files/kinds/e.kind");
    // one ticket past its first step, one before it
    let past = open(&mut core, "p");
    answer(&mut core, past, "p1");
    let before = open(&mut core, "b");
    let edited = KIND.replace("    return q\n", "    c <- step w2 ask(b.text);\n    return c.text\n");
    std::fs::write(&kind, &edited).unwrap();
    let r = core.ticket_reload(&Who::Owner, None, "a third step".into());
    assert!(r.ok, "{}", r.out);
    let (p, b) = (core.tickets.m.id(past), core.tickets.m.id(before));
    let id = |t: u64| if t == past { p.clone() } else { b.clone() };
    let v: serde_json::Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(v["kinds"], json!(["e"]));
    for t in [past, before] {
        assert!(v["tickets"].as_array().unwrap().iter().any(|x| x["id"] == json!(id(t)) && x["kind"] == "e" && x["took"] == true && x["said"] == "took the new text"), "{}", r.out);
    }
    assert_eq!(log(&coord).iter().filter(|l| matches!(l, Line::Edited { what, .. } if what == "reload")).count(), 2);
    answer(&mut core, past, "p2");
    assert!(matches!(&core.tickets.m.tickets[&past].status, Status::Awaits { owed, .. } if owed.step == 3 && owed.args == json!({"q": "p2"})), "{:?}", core.tickets.m.tickets[&past].status);
    // a new text whose remainder reads `z`, bound before where the open tickets stand: they keep their old text
    let unbound = edited.replace("    a <- step w2 ask(q);\n", "    z <- step w2 ask(q);\n    a <- step w2 ask(q);\n").replace("return c.text", "return z.text");
    std::fs::write(&kind, &unbound).unwrap();
    let r = core.ticket_reload(&Who::Owner, None, "reads z".into());
    assert!(r.ok, "{}", r.out);
    let v: serde_json::Value = serde_json::from_str(&r.out).unwrap();
    assert!(v["tickets"].as_array().unwrap().iter().any(|x| x["id"] == json!(id(past)) && x["took"] == false && x["why"].as_str().unwrap().starts_with("`z`")), "{}", r.out);
    // a changed result, while tickets of the kind are open: refused, the open ones named
    std::fs::write(&kind, edited.replace("  result Text\n", "  result [Text]\n").replace("return c.text", "return [c.text]")).unwrap();
    let r = core.ticket_reload(&Who::Owner, None, "a list".into());
    assert!(!r.ok && r.out.contains("the result of e changed, and tickets of it are open") && r.out.contains(&id(past)), "{}", r.out);
    // a restart with the kind file changed again: the record restores every ticket as it was
    std::fs::write(&kind, KIND.replace("return q", "return \"later\"")).unwrap();
    let views: Vec<_> = [past, before].iter().map(|t| core.tickets.m.view(*t)).collect();
    let mut again = Core::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
    again.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
    again.load_tickets();
    assert!(again.tickets.errors.is_empty(), "{:?}", again.tickets.errors);
    let after: Vec<_> = [past, before].iter().map(|t| again.tickets.m.view(*t)).collect();
    assert_eq!(after, views);
    let _ = std::fs::remove_dir_all(&coord);
}

/// A kind's form changes while none of its tickets is open, and the kind that opens it is edited to open it as it is
/// now; an open ticket of that kind that cannot take the new text would still open one with its old fields: refused,
/// that ticket named; with a text it can take, the reload is taken and it takes the text. A restart restores it.
#[test]
fn a_reload_is_refused_while_an_open_ticket_would_open_a_changed_kind_as_it_was() {
    let (mut core, coord) = core("rider");
    let o = "kind o\n  letter O\n  form q: Text\n  result Text\n  pattern\n    a <- step w2 ask(q);\n    c <- open e(a.text);\n    return c\n";
    std::fs::write(coord.join("files/kinds/o.kind"), o).unwrap();
    let r = core.ticket_reload(&Who::Owner, None, "a kind o".into());
    assert!(r.ok, "{}", r.out);
    let t = open_a(&mut core, "o", "x");
    let id = core.tickets.m.id(t);
    std::fs::write(coord.join("files/kinds/e.kind"), KIND.replace("  form q: Text\n", "  form q: Text, r: Text\n")).unwrap();
    // o's new text reads `z`, bound before where its ticket stands: the ticket would keep its old text
    let o_unbound = o.replace("    a <- step w2 ask(q);\n", "    z <- step w2 \"Z {{ q }}\" -> Said(text: Text);\n    a <- step w2 ask(q);\n").replace("open e(a.text)", "open e(a.text, z.text)");
    std::fs::write(coord.join("files/kinds/o.kind"), &o_unbound).unwrap();
    let r = core.ticket_reload(&Who::Owner, None, "e takes r".into());
    assert!(!r.ok && r.out.contains(&format!("the form of e changed, and {id} (o) would still open a e as it was")), "{}", r.out);
    // the kinds stay as they were
    assert_eq!(core.tickets.m.defs.kinds["e"].form.len(), 1);
    std::fs::write(coord.join("files/kinds/o.kind"), o.replace("open e(a.text)", "open e(a.text, q)")).unwrap();
    let r = core.ticket_reload(&Who::Owner, None, "e takes r, o gives it".into());
    assert!(r.ok, "{}", r.out);
    let v: serde_json::Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(v["tickets"], json!([{"id": id, "kind": "o", "took": true, "said": "took the new text", "withdrawn": []}]));
    assert!(core.tickets.m.view(t).unwrap().reloaded.is_some());
    let view = core.tickets.m.view(t);
    let mut again = Core::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
    again.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
    again.load_tickets();
    assert!(again.tickets.errors.is_empty(), "{:?}", again.tickets.errors);
    assert_eq!(again.tickets.m.view(t), view);
    let _ = std::fs::remove_dir_all(&coord);
}

/// A reload that changes only docs is a live edit (paper-research N-101 p672): a `///` on a kind's form field changes
/// its text but not its form, so it is taken while a ticket of it is open, the ticket takes the new text, and the
/// definitions carry the new docs; a doc above `kind` alone changes no ticket's text.
#[test]
fn a_reload_that_changes_only_docs_is_taken_while_tickets_are_open() {
    let (mut core, coord) = core("docs");
    let kind = coord.join("files/kinds/e.kind");
    let with_doc = |kd: &str, fd: &str| format!("/// {kd}\n{}", KIND.replace("  form q: Text\n", &format!("  form\n    q: Text   /// {fd}\n")));
    std::fs::write(&kind, with_doc("a kind", "the question")).unwrap();
    let r = core.ticket_reload(&Who::Owner, None, "docs".into());
    assert!(r.ok, "{}", r.out);
    let t = open(&mut core, "p");
    answer(&mut core, t, "p1");
    let id = core.tickets.m.id(t);
    std::fs::write(&kind, with_doc("a kind", "the question, whole")).unwrap();
    let r = core.ticket_reload(&Who::Owner, None, "a field's doc".into());
    assert!(r.ok, "{}", r.out);
    let v: serde_json::Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(v["tickets"], json!([{"id": id, "kind": "e", "took": true, "said": "took the new text", "withdrawn": []}]));
    assert_eq!(core.tickets.m.defs.kinds["e"].doc.parts["q"], "the question, whole");
    std::fs::write(&kind, with_doc("a kind, said again", "the question, whole")).unwrap();
    let r = core.ticket_reload(&Who::Owner, None, "the kind's doc".into());
    assert!(r.ok, "{}", r.out);
    let v: serde_json::Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(v["tickets"], json!([]));
    assert_eq!(core.tickets.m.defs.kinds["e"].doc.text, "a kind, said again");
    answer(&mut core, t, "p2");
    assert_eq!(core.tickets.m.tickets[&t].status, Status::Done(json!("p")));
    let _ = std::fs::remove_dir_all(&coord);
}

/// Agent-base's own fix (fix.kind), opened by the owner on WHAT, its role's step put to w2 (`fills`): its number.
fn fix(core: &mut Core, what: &str) -> u64 {
    fix_under(core, what, None)
}

fn fix_under(core: &mut Core, what: &str, under: Option<u64>) -> u64 {
    let form = json!({"what": what, "evidence": "seen twice", "where": "docket-machine", "note": null, "branch": null});
    core.ticket_input(Input::Open { kind: "fix".into(), under, opener: "owner".into(), form, text: None, names: vec![], told: vec![] }).unwrap();
    let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
    filled(core, t);
    t
}

/// T's step asked of its role (the fixer's) put to w2, if one is asked.
fn filled(core: &mut Core, t: u64) {
    if let Status::Asking { owed, .. } = &core.tickets.m.tickets[&t].status {
        let step = owed.step;
        core.ticket_input(Input::Decided { t, step, question: "fills".into(), answer: json!("w2"), provenance: json!({}) }).unwrap();
    }
}

/// w2's answer HEAD to the step T awaits, and the next step's role filled.
fn answered(core: &mut Core, t: u64, head: Value) {
    let r = core.ticket_input(Input::Answer { t, by: "w2".into(), head, text: docket_machine::Text::New("as said".into()) });
    assert!(r.is_ok(), "{r:?}");
    filled(core, t);
}

fn nothing() -> Value {
    json!({"head": "Nothing", "searched": "docket-machine's retext"})
}

/// The step T awaits given to w2, as coordd gives a program agent's (a Given line).
fn given(core: &mut Core, t: u64) {
    let d = core.ticket_due("w2").into_iter().find(|d| d.t == t).expect("a step due");
    core.ticket_given("w2", &d);
    assert!(matches!(&core.tickets.m.tickets[&t].status, Status::Awaits { taken: true, .. }));
}

fn reach(core: &mut Core, path: &std::path::Path, text: &str) -> coord_types::Reply {
    core.reach_of(path, text)
}

/// Each ticket of reach answer V, by id, with its case.
fn cases(v: &Value) -> Vec<(String, String)> {
    v["tickets"].as_array().unwrap().iter().map(|x| (x["id"].as_str().unwrap().to_string(), x["reach"].as_str().unwrap().to_string())).collect()
}

fn of<'a>(v: &'a Value, id: &str) -> &'a Value {
    v["tickets"].as_array().unwrap().iter().find(|x| x["id"] == id).unwrap_or_else(|| panic!("no {id}: {v}"))
}

fn base_text(file: &str) -> String {
    std::fs::read_to_string(format!("{}/../../agent-base/{file}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// The reach of a call edit on agent-base's fix (N-1396 p301, 4): `repair`'s words changed in the project's copy of
/// claim.call. F-1 at `seek` has `repair` ahead; F-2's `repair` was given; F-3 is in the review rounds; F-4 is closed.
/// The reach names exactly the three, changes nothing, and records nothing. Saved (the file written, the reload): the
/// `Reload` line names `repair`, F-1's `repair` is sent with the new words, F-2 still holds what it was sent, F-3 has
/// no line of its own, nothing is withdrawn, and a replay gives the same. The Kinds answer marks `repair` with the
/// reload, and so does a restart.
#[test]
fn a_reach_of_repairs_words_names_the_fixes_it_reaches_and_saved_it_reaches_those() {
    let (mut core, coord) = core("reach-repair");
    let f1 = fix(&mut core, "one");
    let f2 = fix(&mut core, "two");
    answered(&mut core, f2, nothing());
    given(&mut core, f2);
    let f3 = fix(&mut core, "three");
    answered(&mut core, f3, nothing());
    answered(&mut core, f3, json!({"head": "Commit", "commit": "abc1234"}));
    let f4 = fix(&mut core, "four");
    core.ticket_input(Input::Cancel { t: f4, by: "owner".into(), why: "not needed".into() }).unwrap();
    assert!(!core.tickets.m.tickets[&f4].is_open());
    let claim = base_text("calls/claim.call");
    let new = claim.replace("This defect was accepted for fixing, and you are to fix it:", "Fix this accepted defect:");
    assert_ne!(new, claim);
    let path = coord.join("files/calls/claim.call");
    let (lines, changes) = (log(&coord).len(), core.tickets.m.changes());
    let r = reach(&mut core, &path, &new);
    assert!(r.ok, "{}", r.out);
    let v: Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!((v["kinds"].clone(), v["calls"].clone(), v["clocks"].clone()), (json!([]), json!(["repair"]), json!([])), "{v}");
    let id = |core: &Core, t: u64| core.tickets.m.id(t);
    let want: Vec<(String, String)> = [(f1, "ahead"), (f2, "given"), (f3, "past")].iter().map(|(t, c)| (id(&core, *t), c.to_string())).collect();
    assert_eq!(cases(&v), want, "{v}");
    assert!(of(&v, &id(&core, f2))["said"].as_str().unwrap().contains("a reminder of it, or the step given again after its process was lost, is rendered from the files and reads the new words"), "{v}");
    assert!(v["tickets"].as_array().unwrap().iter().all(|x| x.get("took").is_none()), "a call edit carries no ticket: {v}");
    assert_eq!((log(&coord).len(), core.tickets.m.changes()), (lines, changes), "a reach records nothing and counts nothing");
    assert!(!path.exists());
    // Save: the file, and the reload
    let before = |core: &Core| [f2, f3].map(|t| core.tickets.m.tickets[&t].status.clone());
    let kept = before(&core);
    std::fs::write(&path, &new).unwrap();
    let r = core.ticket_reload(&Who::Owner, Some("abc1234".into()), "clearer".into());
    assert!(r.ok, "{}", r.out);
    let after: Vec<Line> = log(&coord)[lines..].to_vec();
    assert!(matches!(&after[0], Line::Reload { calls, kinds, .. } if *calls == ["repair"] && kinds.is_empty()), "{:?}", after[0]);
    assert!(!after.iter().any(|l| matches!(l, Line::Edited { .. })), "a call edit retexts nothing: {after:?}");
    assert_eq!(before(&core), kept, "F-2 holds what it was sent, F-3 runs on");
    answered(&mut core, f1, nothing());
    let p = prompt(&core, f1);
    assert!(p.contains("Fix this accepted defect:") && !p.contains("you are to fix it"), "{p}");
    assert!(core.tickets.m.tickets.values().all(|x| x.withdrawn.is_empty()), "nothing withdrawn");
    let m2 = core.tickets.replayed();
    for t in [f1, f2, f3, f4] {
        assert_eq!(m2.tickets[&t].status, core.tickets.m.tickets[&t].status, "ticket {t} replays the same");
    }
    // the mark: `repair` edited, by whom, why, after which commit; fix itself and `seek` not
    let mark = |core: &Core| {
        let kinds: Vec<Value> = serde_json::from_str(&core.ticket_kinds().out).unwrap();
        let fix = kinds.iter().find(|k| k["kind"] == "fix").cloned().unwrap();
        let call = |n: &str| fix["calls"].as_array().unwrap().iter().find(|c| c["name"] == n).cloned().unwrap();
        (fix.get("edited").cloned(), call("seek").get("edited").cloned(), call("repair")["edited"].clone())
    };
    let (k, s, e) = mark(&core);
    assert_eq!((k, s), (None, None));
    assert_eq!((e["by"].clone(), e["why"].clone(), e["commit"].clone()), (json!("owner"), json!("clearer"), json!("abc1234")), "{e}");
    assert!(e["at"].as_str().is_some_and(|a| a.ends_with('Z')), "{e}");
    let mut again = Core::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
    again.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
    again.load_tickets();
    assert_eq!(mark(&again).2, e, "a restart marks the same");
    let _ = std::fs::remove_dir_all(&coord);
}

/// consult-3 p306, 1: a call edit does not reach a step that goes with the owner's own prompt. A fix under a ticket
/// whose `repair` he edited for it and the tickets under it (Only this thread) is `own`, and saved, its `repair` is sent
/// with his text; one whose `repair` prompt he edited at its halt is `own` too.
#[test]
fn the_owners_own_prompt_stands_where_a_call_edit_would_reach() {
    let (mut core, coord) = core("reach-own");
    let top = fix(&mut core, "top");
    let under = fix_under(&mut core, "under", Some(top));
    let r = core.ticket_template(&Who::Owner, &core.tickets.m.id(top), "repair".into(), "tree".into(), "Fix it my way: {{ what }}".into(), "mine".into());
    assert!(r.ok, "{}", r.out);
    // slow from its `repair` on: that step's give halts, and he edits its prompt there
    let halted = fix(&mut core, "halted");
    core.ticket_input(Input::Answer { t: halted, by: "w2".into(), head: nothing(), text: docket_machine::Text::New("as said".into()) }).unwrap();
    core.ticket_input(Input::Slow { t: halted, on: true, by: "owner".into() }).unwrap();
    filled(&mut core, halted);
    // the fills answer halts first; approved, the give does
    let n = loop {
        let h = core.tickets.m.tickets[&halted].halts.last().cloned().expect("a halt");
        if matches!(h.what, docket_machine::Halting::Give { .. }) {
            break h.n;
        }
        core.ticket_input(Input::Approve { t: halted, n: h.n, by: "owner".into() }).unwrap();
    };
    core.ticket_input(Input::HaltEdit { t: halted, n, by: "owner".into(), value: json!({"prompt": "Only the header, please."}) }).unwrap();
    let new = base_text("calls/claim.call").replace("This defect was accepted for fixing, and you are to fix it:", "Fix this accepted defect:");
    let path = coord.join("files/calls/claim.call");
    let r = reach(&mut core, &path, &new);
    assert!(r.ok, "{}", r.out);
    let v: Value = serde_json::from_str(&r.out).unwrap();
    for t in [top, under, halted] {
        assert_eq!(of(&v, &core.tickets.m.id(t))["reach"], "own", "{v}");
    }
    assert!(of(&v, &core.tickets.m.id(under))["said"].as_str().unwrap().contains(&format!("on {} and the tickets under it", core.tickets.m.id(top))), "{v}");
    assert!(of(&v, &core.tickets.m.id(halted))["said"].as_str().unwrap().contains("edited at its halt"), "{v}");
    std::fs::write(&path, &new).unwrap();
    assert!(core.ticket_reload(&Who::Owner, None, "clearer".into()).ok);
    answered(&mut core, under, nothing());
    let p = prompt(&core, under);
    assert!(p.contains("Fix it my way: under") && !p.contains("Fix this accepted defect"), "{p}");
    let _ = std::fs::remove_dir_all(&coord);
}

/// consult-3 p306, 2 and 1: a kind edit that takes out the statement a ticket stands at, on agent-base's fix: `seek`
/// gone. The reach says `withdrawn`, in the words the reload then says, and names the step; a fix owed `repair`, whose
/// arguments the edit changes, keeps what its step was made with (`given`, not yet given), and saved, its step's
/// arguments are as they were; one in the review rounds is `past`. Each takes the new text.
#[test]
fn a_kind_edit_that_takes_out_where_a_fix_stands_is_withdrawn_in_the_reloads_words() {
    let (mut core, coord) = core("reach-gone");
    let at_seek = fix(&mut core, "at seek");
    given(&mut core, at_seek);
    let at_repair = fix(&mut core, "at repair");
    answered(&mut core, at_repair, nothing());
    let args = |core: &Core| match &core.tickets.m.tickets[&at_repair].status {
        Status::Awaits { owed, .. } => owed.args.clone(),
        s => panic!("{s:?}"),
    };
    let was = args(&core);
    let rounds = fix(&mut core, "in rounds");
    answered(&mut core, rounds, nothing());
    answered(&mut core, rounds, json!({"head": "Commit", "commit": "abc1234"}));
    // the whole `seek` statement: since N-1978 p2 it is a `try` with its NoWorker arm
    let seek = "    // no fixer running: told as no fix, in words, and the claim ends with it (N-1978 p2)\n    found <- try { step role fixer seek(what, where, note) } handle {\n      NoWorker(w) => raise NoFix(\"no fixer is running in this project\")\n    };\n";
    assert!(base_text("kinds/fix.kind").contains(seek), "fix.kind's seek statement");
    let new = base_text("kinds/fix.kind").replace(seek, "").replace("repair(what, evidence, where, note, branch, found)", "repair(what, evidence, where, note, branch, none)");
    assert_ne!(new, base_text("kinds/fix.kind"));
    let path = coord.join("files/kinds/fix.kind");
    let r = reach(&mut core, &path, &new);
    assert!(r.ok, "{}", r.out);
    let v: Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(v["kinds"], json!(["fix"]), "{v}");
    let ids: std::collections::BTreeMap<u64, String> = [at_seek, at_repair, rounds].iter().map(|t| (*t, core.tickets.m.id(*t))).collect();
    let id = |t: u64| ids[&t].clone();
    let (s, a, p) = (of(&v, &id(at_seek)).clone(), of(&v, &id(at_repair)).clone(), of(&v, &id(rounds)).clone());
    assert_eq!((s["reach"].clone(), a["reach"].clone(), p["reach"].clone()), (json!("withdrawn"), json!("given"), json!("past")), "{v}");
    assert!(s["said"].as_str().unwrap().starts_with("took the new text; step 1 (seek) of w2 withdrawn: its statement is gone"), "{s}");
    assert!(a["said"].as_str().unwrap().contains("step 2 (repair) keeps the words it was made with (owed since") && a["said"].as_str().unwrap().contains("not yet given"), "{a}");
    assert!([&s, &a, &p].iter().all(|x| x["took"] == true), "{v}");
    std::fs::write(&path, &new).unwrap();
    let r = core.ticket_reload(&Who::Owner, None, "no seek".into());
    assert!(r.ok, "{}", r.out);
    let saved: Value = serde_json::from_str(&r.out).unwrap();
    for (t, x) in [(at_seek, &s), (at_repair, &a), (rounds, &p)] {
        let one = saved["tickets"].as_array().unwrap().iter().find(|y| y["id"] == id(t)).cloned().unwrap();
        assert!(x["said"].as_str().unwrap().starts_with(one["said"].as_str().unwrap()), "the reach says what the reload did: {x} {one}");
    }
    assert_eq!(saved["tickets"].as_array().unwrap().iter().find(|y| y["id"] == id(at_seek)).unwrap()["withdrawn"], json!([{"on": id(at_seek), "step": 1, "call": "seek", "worker": "w2"}]));
    assert_eq!(args(&core), was, "the step owed keeps the arguments it was made with");
    let _ = std::fs::remove_dir_all(&coord);
}

/// A draft Reload would refuse is refused by Reach in the same words, and nothing is read again: fix's letter changed
/// (its tickets' ids are made from it). Only the owner has a draft read, and only a kind's or a call's file of the
/// folders read.
#[test]
fn a_reach_refuses_as_reload_does_in_its_words() {
    let (mut core, coord) = core("reach-refused");
    fix(&mut core, "open");
    let new = base_text("kinds/fix.kind").replace("kind fix\n", "kind fix\n  letter X\n");
    assert_ne!(new, base_text("kinds/fix.kind"));
    let path = coord.join("files/kinds/fix.kind");
    let r = reach(&mut core, &path, &new);
    assert_eq!((r.ok, r.out.as_str()), (false, "nothing is read again, the kinds and calls stay as they were: the kind fix's letter changed (its tickets' ids are made from it)"));
    std::fs::write(&path, &new).unwrap();
    let reload = core.ticket_reload(&Who::Owner, None, "a text".into());
    assert_eq!((reload.ok, reload.out.clone()), (false, r.out.clone()), "the same words");
    std::fs::remove_file(&path).unwrap();
    assert!(core.ticket_reach_from(&Who::Agent("w2".into()), &path.display().to_string(), &new).is_err(), "the owner's only");
    let elsewhere = coord.join("fix.kind").display().to_string();
    assert!(core.ticket_reach_from(&Who::Owner, &elsewhere, &new).is_err_and(|e| e.out.contains("is no kind or call file of a folder the kinds and calls are read from")));
    let _ = std::fs::remove_dir_all(&coord);
}

/// A draft of an agent's own program with a clock (`kinds/agents/NAME.kind`): the reach gives the binding and its next
/// firing as the clock reads it: a binding with a new text has never fired, so it fires at the next round.
#[test]
fn a_reach_of_a_clock_gives_its_next_firing() {
    let (mut core, coord) = core("reach-clock");
    let clock = "every 1h open question(\"hourly\")";
    let path = coord.join("files/kinds/agents/w2.kind");
    let r = reach(&mut core, &path, &format!("{clock}\n"));
    assert!(r.ok, "{}", r.out);
    let v: Value = serde_json::from_str(&r.out).unwrap();
    let c = v["clocks"].as_array().unwrap();
    assert_eq!(c.len(), 1, "{v}");
    assert_eq!(c[0]["binding"], clock);
    let next = c[0]["next"].as_str().unwrap();
    assert!(next.len() == 20 && next.ends_with('Z') && next.as_bytes()[10] == b'T', "{next}");
    let _ = std::fs::remove_dir_all(&coord);
}

/// consult-3 p308, 1: a step being handed (put in `tickets.handed` under the lock, rendered after it is let go) goes
/// with whichever text its rendering read, so the reach says that and claims neither. F-1 and F-2 owe `repair`, both
/// handed to w2's session. Saved: the session took F-1's (its Given), which keeps what it was sent; it did not take
/// F-2's, and handed again after the Save, F-2's goes with the new words.
#[test]
fn a_step_being_handed_goes_with_the_text_its_rendering_read() {
    let (mut core, coord) = core("reach-handed");
    let took = fix(&mut core, "took");
    answered(&mut core, took, nothing());
    let lost = fix(&mut core, "lost");
    answered(&mut core, lost, nothing());
    let due: Vec<_> = core.ticket_due("w2").into_iter().filter(|d| d.t == took || d.t == lost).collect();
    let ids: std::collections::BTreeMap<u64, String> = due.iter().map(|d| (d.t, d.id.clone())).collect();
    assert_eq!(ids.len(), 2, "{ids:?}");
    core.ticket_handing("w2", due);
    let new = base_text("calls/claim.call").replace("This defect was accepted for fixing, and you are to fix it:", "Fix this accepted defect:");
    let path = coord.join("files/calls/claim.call");
    let r = reach(&mut core, &path, &new);
    assert!(r.ok, "{}", r.out);
    let v: Value = serde_json::from_str(&r.out).unwrap();
    for t in [took, lost] {
        let x = of(&v, &core.tickets.m.id(t));
        assert_eq!(x["reach"], "given", "{v}");
        assert_eq!(x["said"], "step 2 (repair) is being handed to w2: it goes with the text its rendering read", "{x}");
    }
    std::fs::write(&path, &new).unwrap();
    assert!(core.ticket_reload(&Who::Owner, None, "clearer".into()).ok);
    core.ticket_handed("w2", &[ids[&took].clone()], true);
    assert!(matches!(&core.tickets.m.tickets[&took].status, Status::Awaits { taken: true, owed, .. } if owed.step == 2), "{:?}", core.tickets.m.tickets[&took].status);
    core.ticket_handed("w2", &[ids[&lost].clone()], false);
    assert!(matches!(&core.tickets.m.tickets[&lost].status, Status::Awaits { taken: false, .. }));
    let p = prompt(&core, lost);
    assert!(p.contains("Fix this accepted defect:") && !p.contains("you are to fix it"), "{p}");
    let _ = std::fs::remove_dir_all(&coord);
}
