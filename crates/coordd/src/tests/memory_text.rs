//! Part 7's library text (paper-research N-581.p7, N-568; one-interpreter.md §7): `agent-base/policy/eflint/memory.eflint`,
//! the memory rules (keep, carry, the summary duty) in eFLINT whole, checked against eflint-rs directly, as part 4's
//! library is (tests/library_text.rs):
//!
//! - it loads in eflint-rs after the team's facts it reads (`agent`, `role`), and until the driver reports something
//!   it enables no form but full and packed (and gone, to carry), and puts no duty on anyone; the situation is built as
//!   the driver reports it: things stated, then events (`come-to-rest`, `point-at`) and acts (`replace`, `write`,
//!   `summarize`);
//! - keep: the forms each kind of thing may be stored in, for the owner and a keeper and nobody else; a pick replaces
//!   the old form;
//! - carry: never more whole than the thing is kept in; a replaced thing as a stub only; a summary only one that still says it; packed never;
//! - the preference: the cheapest enabled option is the library's choice (packed to keep; the most whole allowed to
//!   carry);
//! - the summary duty: put on the summarizer by `come-to-rest`, violated by `summary-due`, ended by `summarize` of the
//!   summarizer's own output, or by `move-again`, which also makes the old summary stale;
//! - a project narrows an option with `Extend Act ... Conditioned by`, and states a preference, with a condition or none,
//!   as a ranked cost clause by `Extend Fact` (encoding proposed, N-568.p56, not yet seen by the owner): the owner's own
//!   example by age is written and picked.

use eflint::replay::{Answer, Event, Replay, Value};
use std::collections::BTreeSet;

fn base() -> String {
    format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"))
}

fn memory() -> String {
    let path = format!("{}/policy/eflint/memory.eflint", base());
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The team's facts the file reads, as the prelude declares them (written out by part 3's to_eflint, or part 4's
/// library).
const TEAM: &str = "Fact agent Identified by String.\nFact role Identified by agent * string.\n";

const FORMS: [&str; 6] = ["full", "packed", "rollup", "summary", "stub", "gone"];

fn s(x: &str) -> Value {
    Value::Str(x.to_string())
}

fn ok(a: Answer) -> Answer {
    assert!(a.ok && a.errors.is_empty(), "{:?}", a.errors);
    a
}

fn fact(r: &mut Replay, name: &str, objects: Vec<Value>, holds: bool) {
    ok(r.apply(&Event::Fact { name: name.into(), objects, holds: Some(holds) }));
}

fn event(r: &mut Replay, name: &str, objects: &[&str]) {
    ok(r.apply(&Event::Event { name: name.into(), objects: objects.iter().map(|o| s(o)).collect() }));
}

/// The situation every test starts from (EXTRA: a project's text after the library), reported as the driver would:
/// - agents: owner (role owner), coordd (the keeper), summ (the summarizer), dev (a member);
/// - N-1 came to rest, no summary yet; N-2 is open; N-3 was replaced by N-4, its two words named;
/// - N-5 came to rest and summ summarised it as N-5.s, and something open points at it; N-6 likewise as N-6.s.
fn situation(extra: &str) -> Replay {
    let mut r = Replay::from_text(&format!("{TEAM}{}{extra}", memory())).unwrap_or_else(|e| panic!("{e}"));
    for (a, role) in [("owner", "owner"), ("coordd", "member"), ("summ", "member"), ("dev", "member")] {
        fact(&mut r, "agent", vec![s(a)], true);
        fact(&mut r, "role", vec![s(a), s(role)], true);
    }
    fact(&mut r, "keeper", vec![s("coordd")], true);
    fact(&mut r, "summarizer", vec![s("summ")], true);
    for t in ["N-1", "N-2", "N-3", "N-3.w", "N-4", "N-5", "N-5.s", "N-6", "N-6.s"] {
        fact(&mut r, "thing", vec![s(t)], true);
    }
    for t in ["N-1", "N-5", "N-6"] {
        event(&mut r, "come-to-rest", &[t]);
    }
    for t in ["N-5", "N-6"] {
        let summary = format!("{t}.s");
        assert!(act(&mut r, "write", "summ", &[&summary]).disabled.is_empty());
        assert!(act(&mut r, "summarize", "summ", &["owner", t, &summary]).disabled.is_empty(), "{t}");
    }
    assert!(act(&mut r, "replace", "coordd", &["N-3", "N-4"]).disabled.is_empty());
    // the stub's two words: the fresh call's output, as a summary is (consult-4, N-568.p70 point 13)
    assert!(act(&mut r, "write", "summ", &["N-3.w"]).disabled.is_empty());
    assert!(act(&mut r, "name-stub", "summ", &["N-3", "N-3.w"]).disabled.is_empty());
    event(&mut r, "point-at", &["N-5"]);
    r
}

/// The enabled act instances, each as `act actor objects...` (the plain values, no type names).
fn enabled(r: &Replay) -> BTreeSet<String> {
    r.enabled().iter().map(|e| plain(&e.act.to_string())).collect()
}

/// `pick-keep(agent("coordd"),thing("N-1"),form("full"))` → `pick-keep coordd N-1 full`.
fn plain(t: &str) -> String {
    let mut out = String::new();
    let mut parts: Vec<String> = Vec::new();
    let (head, rest) = t.split_once('(').unwrap_or((t, ""));
    out.push_str(head);
    for q in rest.split('"').skip(1).step_by(2) {
        parts.push(q.to_string());
    }
    for p in parts {
        out.push(' ');
        out.push_str(&p);
    }
    out
}

/// The forms ACT lets ACTOR (and RECIPIENT, for a carry) pick for THING.
fn forms(r: &Replay, act: &str, who: &[&str], thing: &str) -> Vec<&'static str> {
    let on = enabled(r);
    FORMS.into_iter().filter(|f| on.contains(&format!("{act} {} {thing} {f}", who.join(" ")))).collect()
}

/// The picker's choice for THING among the ALLOWED forms by COSTS (`keep-cost` or `carry-cost`), asked of the file:
/// the least cost at the highest rank where the options' costs differ, going down a rank among those tied; an option a
/// rank gives no cost to is the dearest there (N-568.p56).
fn cheapest(r: &mut Replay, costs: &str, thing: &str, allowed: &[&str]) -> &'static str {
    let mut tied: Vec<&'static str> = FORMS.into_iter().filter(|f| allowed.contains(f)).collect();
    assert!(!tied.is_empty(), "nothing allowed for {thing}");
    for rank in (0..4).rev() {
        let at: Vec<Option<i32>> = tied
            .iter()
            .map(|f| (0..10).find(|c| holds(r, &format!(r#"{costs}(thing("{thing}"), form("{f}"), rank({rank}), {c})"#))))
            .collect();
        if at.iter().all(Option::is_none) {
            continue;
        }
        let best = at.iter().map(|c| c.unwrap_or(i32::MAX)).min().unwrap();
        tied = tied.iter().zip(&at).filter(|(_, c)| c.unwrap_or(i32::MAX) == best).map(|(f, _)| *f).collect();
        if tied.len() == 1 {
            break;
        }
    }
    tied[0]
}

fn holds(r: &mut Replay, phrase: &str) -> bool {
    let a = ok(r.apply(&Event::Phrase(format!("?{phrase}."))));
    a.queries == [true]
}

fn act(r: &mut Replay, name: &str, actor: &str, objects: &[&str]) -> Answer {
    ok(r.apply(&Event::Act { name: name.into(), actor: s(actor), objects: objects.iter().map(|o| s(o)).collect() }))
}

#[test]
fn the_memory_text_loads_and_holds_nothing_until_the_driver_reports_things() {
    let mut r = Replay::from_text(&format!("{TEAM}{}", memory())).unwrap_or_else(|e| panic!("{e}"));
    assert!(r.duties().is_empty());
    fact(&mut r, "agent", vec![s("owner")], true);
    fact(&mut r, "role", vec![s("owner"), s("owner")], true);
    fact(&mut r, "thing", vec![s("N-1")], true);
    assert_eq!(forms(&r, "pick-keep", &["owner"], "N-1"), ["full", "packed"]);
    assert_eq!(forms(&r, "pick-carry", &["owner", "owner"], "N-1"), ["full", "gone"]);
    assert!(r.duties().is_empty(), "no thread rests: no summary owed");
}

#[test]
fn keep_allows_each_thing_its_forms_for_the_owner_and_a_keeper_only() {
    let mut r = situation("");
    for who in ["owner", "coordd"] {
        assert_eq!(forms(&r, "pick-keep", &[who], "N-1"), ["full", "packed", "rollup"], "rests, no summary");
        assert_eq!(forms(&r, "pick-keep", &[who], "N-2"), ["full", "packed"], "open");
        assert_eq!(forms(&r, "pick-keep", &[who], "N-3"), ["full", "packed", "stub", "gone"], "replaced, its words known");
        assert_eq!(forms(&r, "pick-keep", &[who], "N-5"), ["full", "packed", "rollup", "summary"], "summarised, but pointed at");
        assert_eq!(forms(&r, "pick-keep", &[who], "N-6"), ["full", "packed", "rollup", "summary", "gone"], "summarised");
    }
    for who in ["dev", "summ"] {
        assert!(forms(&r, "pick-keep", &[who], "N-1").is_empty(), "{who} keeps nothing");
    }
    // the library's choice: lossless
    for (t, allowed) in [("N-1", vec!["full", "packed", "rollup"]), ("N-6", vec!["full", "packed", "rollup", "summary", "gone"])] {
        assert_eq!(cheapest(&mut r, "keep-cost", t, &allowed), "packed", "{t}");
    }
    // a pick replaces the old form; packed and full say the same, so either follows the other
    assert!(act(&mut r, "pick-keep", "coordd", &["N-6", "packed"]).disabled.is_empty());
    assert!(holds(&mut r, r#"kept(thing("N-6"), form("packed"))"#));
    assert_eq!(forms(&r, "pick-keep", &["coordd"], "N-6"), ["full", "packed", "rollup", "summary", "gone"]);
    act(&mut r, "pick-keep", "owner", &["N-6", "summary"]);
    assert!(holds(&mut r, r#"kept(thing("N-6"), form("summary"))"#) && !holds(&mut r, r#"kept(thing("N-6"), form("packed"))"#));
    // what a summary dropped is not kept: nothing more whole is offered again, so the cheapest is no longer packed
    assert_eq!(forms(&r, "pick-keep", &["coordd"], "N-6"), ["summary", "gone"]);
    // the pointer let go, N-5 may go
    event(&mut r, "let-go", &["N-5"]);
    assert!(forms(&r, "pick-keep", &["coordd"], "N-5").contains(&"gone"));
    // not allowed: performed anyway (the owner is bound by no act), it says so
    assert!(!act(&mut r, "pick-keep", "dev", &["N-2", "gone"]).disabled.is_empty());
}

#[test]
fn carry_is_never_more_whole_than_the_keep_and_a_replaced_thing_is_a_stub() {
    let mut r = situation("");
    let carry = |r: &Replay, t: &str| forms(r, "pick-carry", &["coordd", "dev"], t);
    assert_eq!(carry(&r, "N-2"), ["full", "gone"], "kept whole; it moves: no rollup; packed is no carry");
    assert_eq!(carry(&r, "N-3"), ["stub", "gone"], "replaced");
    assert_eq!(carry(&r, "N-6"), ["full", "rollup", "summary", "gone"]);
    act(&mut r, "pick-keep", "coordd", &["N-6", "summary"]);
    assert_eq!(carry(&r, "N-6"), ["summary", "gone"], "kept as its summary: no more whole");
    act(&mut r, "pick-keep", "coordd", &["N-2", "packed"]);
    assert_eq!(carry(&r, "N-2"), ["full", "gone"], "kept packed: still had whole");
    assert!(forms(&r, "pick-carry", &["dev", "dev"], "N-2").is_empty(), "dev picks nothing");
    // the library's choice: the most whole allowed
    for (t, want) in [("N-6", "summary"), ("N-3", "stub"), ("N-2", "full")] {
        let allowed = carry(&r, t);
        assert_eq!(cheapest(&mut r, "carry-cost", t, &allowed), want, "{t}");
    }
    // a pick replaces the agent's old form, and only that agent's
    act(&mut r, "pick-carry", "coordd", &["dev", "N-6", "summary"]);
    act(&mut r, "pick-carry", "coordd", &["summ", "N-6", "gone"]);
    act(&mut r, "pick-carry", "coordd", &["dev", "N-6", "gone"]);
    assert!(holds(&mut r, r#"carried(agent("dev"), thing("N-6"), form("gone"))"#));
    assert!(!holds(&mut r, r#"carried(agent("dev"), thing("N-6"), form("summary"))"#));
    assert!(holds(&mut r, r#"carried(agent("summ"), thing("N-6"), form("gone"))"#));
}

#[test]
fn a_resting_thread_is_owed_a_summary_until_one_is_written() {
    let mut r = situation("");
    let owed = |r: &Replay| -> Vec<(String, bool)> { r.duties().iter().map(|d| (plain(&d.duty.to_string()), d.violated)).collect() };
    assert_eq!(owed(&r), [("write-summary summ owner N-1".to_string(), false)], "N-5 and N-6 were summarised; N-1 not yet");
    event(&mut r, "summary-due", &["N-1"]);
    assert_eq!(owed(&r), [("write-summary summ owner N-1".to_string(), true)], "past summary-after: violated, not prevented");
    // only the summarizer's own written output is a summary: no other thing is offered as one
    fact(&mut r, "thing", vec![s("N-1.s")], true);
    let on = enabled(&r);
    assert!(!on.iter().any(|a| a.starts_with("summarize ")), "nothing written yet: {on:?}");
    act(&mut r, "write", "summ", &["N-1.s"]);
    let on = enabled(&r);
    assert_eq!(on.iter().filter(|a| a.starts_with("summarize ")).collect::<Vec<_>>(), ["summarize summ owner N-1 N-1.s"], "{on:?}");
    assert!(act(&mut r, "summarize", "summ", &["owner", "N-1", "N-1.s"]).disabled.is_empty());
    assert!(owed(&r).is_empty(), "written: the duty ends");
    assert_eq!(forms(&r, "pick-keep", &["coordd"], "N-1"), ["full", "packed", "rollup", "summary", "gone"]);
    // it moves again: its summary no longer says it, nothing is owed while it moves
    event(&mut r, "move-again", &["N-1"]);
    assert!(owed(&r).is_empty());
    assert!(holds(&mut r, r#"stale(thing("N-1.s"))"#));
    assert_eq!(forms(&r, "pick-keep", &["coordd"], "N-1"), ["full", "packed"]);
    // at its next rest a new summary is owed, and the new one replaces the old
    event(&mut r, "come-to-rest", &["N-1"]);
    assert_eq!(owed(&r), [("write-summary summ owner N-1".to_string(), false)]);
    assert!(!forms(&r, "pick-keep", &["coordd"], "N-1").contains(&"summary"), "the old summary is stale");
    fact(&mut r, "thing", vec![s("N-1.s2")], true);
    act(&mut r, "write", "summ", &["N-1.s2"]);
    act(&mut r, "summarize", "summ", &["owner", "N-1", "N-1.s2"]);
    assert!(owed(&r).is_empty() && holds(&mut r, r#"replaced-by(thing("N-1.s"), thing("N-1.s2"))"#));
    assert!(forms(&r, "pick-keep", &["coordd"], "N-1").contains(&"summary"));
    // an old summary-due does not count against a later rest (consult-4, N-568.p70 point 12): overdue, then moved
    // without a summary, then resting again; and a clock's summary-due that comes after the summary
    event(&mut r, "move-again", &["N-1"]);
    event(&mut r, "come-to-rest", &["N-1"]);
    event(&mut r, "summary-due", &["N-1"]);
    assert_eq!(owed(&r), [("write-summary summ owner N-1".to_string(), true)]);
    event(&mut r, "move-again", &["N-1"]);
    event(&mut r, "come-to-rest", &["N-1"]);
    assert_eq!(owed(&r), [("write-summary summ owner N-1".to_string(), false)], "a new rest: not overdue yet");
    fact(&mut r, "thing", vec![s("N-1.s3")], true);
    act(&mut r, "write", "summ", &["N-1.s3"]);
    act(&mut r, "summarize", "summ", &["owner", "N-1", "N-1.s3"]);
    event(&mut r, "summary-due", &["N-1"]);
    event(&mut r, "move-again", &["N-1"]);
    event(&mut r, "come-to-rest", &["N-1"]);
    assert_eq!(owed(&r), [("write-summary summ owner N-1".to_string(), false)], "the late clock's summary-due is gone");
}

#[test]
fn a_project_narrows_an_option_and_ranks_its_own_preference_above_the_librarys() {
    let r = situation("\nExtend Act pick-keep\n  Conditioned by form != form(\"gone\").\n");
    assert_eq!(forms(&r, "pick-keep", &["coordd"], "N-6"), ["full", "packed", "rollup", "summary"]);
    // its own preference, unconditioned, ranked above the library's: summaries kept where there is one
    let mut r = situation("\nExtend Fact keep-cost\n  Derived from (Foreach thing : keep-cost(thing, form(\"summary\"), rank(1), 0)).\n");
    for (t, want) in [("N-6", "summary"), ("N-1", "packed"), ("N-2", "packed")] {
        let allowed = forms(&r, "pick-keep", &["coordd"], t);
        assert_eq!(cheapest(&mut r, "keep-cost", t, &allowed), want, "{t}: where rank 1's form is not enabled, rank 0 picks");
    }
}

/// The owner's own example (N-536.p13: keep everything one or two weeks, pack at one to one and a half), written as the
/// file's header writes it, with a condition on the thing's age.
#[test]
fn the_owners_example_a_preference_by_age_is_written_as_a_ranked_cost() {
    let text = r#"
Extend Fact keep-cost
  Derived from (Foreach aged : keep-cost(aged.thing, form("full"), rank(1), 0) When aged.int < 10)
  Derived from (Foreach aged : keep-cost(aged.thing, form("packed"), rank(1), 0) When aged.int >= 10 && aged.int < 14)
  Derived from (Foreach aged : keep-cost(aged.thing, form("gone"), rank(1), 0) When aged.int >= 14).
"#;
    let mut r = situation(text);
    let pick = |r: &mut Replay, t: &str| {
        let allowed = forms(r, "pick-keep", &["coordd"], t);
        cheapest(r, "keep-cost", t, &allowed)
    };
    assert_eq!(pick(&mut r, "N-6"), "packed", "no age stated: the library's");
    let mut was: Option<i64> = None;
    for (days, n6, n1) in [(3, "full", "full"), (12, "packed", "packed"), (20, "gone", "packed")] {
        for t in ["N-6", "N-1"] {
            if let Some(d) = was {
                fact(&mut r, "aged", vec![s(t), Value::Int(d)], false);
            }
            fact(&mut r, "aged", vec![s(t), Value::Int(days)], true);
        }
        was = Some(days);
        assert_eq!(pick(&mut r, "N-6"), n6, "N-6 at {days} days");
        assert_eq!(pick(&mut r, "N-1"), n1, "N-1 at {days} days (gone not enabled: no summary)");
    }
}
