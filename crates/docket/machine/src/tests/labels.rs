//! Labels (the owner N-286.p1, paper-research N-101 p800): what a ticket, a step and an agent read as beside their
//! ids, from what the program already says.

use crate::*;
use serde_json::json;
use std::collections::BTreeMap;

const KINDS: [(&str, &str); 1] = [(
    "labels.kind",
    "/// a change: designed, built in parts, accepted
kind change
  letter C
  form words: Text
  result Text
  pattern
    r <- open planned(\"the design, whole: what will be true\", \"about-note\", \"impl1\");
    return r

/// one part, planned and built
kind planned
  letter P
  form design: Text, part: Text, worker: Agent
  result Text
  pattern
    r <- open piece(worker);
    return r

/// one hand-in
/// one implementer, one hand-in: done when it is on the line
kind piece
  letter H
  form implementer: Agent
  result Text
  pattern
    let agent minder in {
      s <- step minder \"Look\" -> Rest;
      c <- step implementer \"Build\" -> Done(c: Text);
      match c { Done(c) => return c }
    }
",
)];

fn machine() -> Machine {
    let files: Vec<(String, String)> = KINDS.iter().map(|(f, t)| (f.to_string(), t.to_string())).collect();
    let (d, errors) = Defs::build(&files, BTreeMap::new(), BTreeMap::new(), 200);
    assert!(errors.is_empty(), "{errors:?}");
    let mut m = Machine::load(d);
    m.apply(Input::Role { agent: "orch".into(), role: "orchestrator".into(), also: vec!["lead".into()] }, "2026-11-16T15:00:00Z".into()).reply.unwrap();
    let out = m.apply(Input::Open { kind: "change".into(), under: None, opener: "owner".into(), form: json!({"words": "Add a note to about\nfor a newcomer"}), text: None, names: vec![], told: vec![] }, "2026-11-16T15:00:01Z".into());
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    m
}

#[test]
fn a_ticket_reads_as_what_names_it() {
    let m = machine();
    let ids: Vec<(String, String)> = m.tickets.keys().map(|t| (m.id(*t), label::ticket(&m, *t, &label::own_words))).collect();
    assert_eq!(
        ids,
        vec![
            // no parent: the first line of the owner's words
            ("C-200".to_string(), "Add a note to about".to_string()),
            // the first Text field whose value is one word, by its field's name (the design is many words)
            ("P-201".to_string(), "part about-note".to_string()),
            // nothing of its own: what names the ticket above it, then its kind's doc's first line
            ("H-202".to_string(), "part about-note: one hand-in".to_string()),
        ]
    );
    // the view's title is the ticket's words, else its label
    assert_eq!(m.view(200).unwrap().title, "Add a note to about");
    assert_eq!(m.view(202).unwrap().title, "part about-note: one hand-in");
    // words that begin with the line they answer and run long: the title is cut as the label is, the quote passed
    // over (C16 of 0.2, "short titles")
    let mut m = m;
    let words = "> the circle is wrong\nThe search field's placeholder is cut in With you, and the checkbox beside it leaves the field no room\nmore";
    let out = m.apply(Input::Open { kind: "change".into(), under: None, opener: "owner".into(), form: json!({"words": words}), text: None, names: vec![], told: vec![] }, "2026-11-16T15:00:02Z".into());
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let t = *m.tickets.keys().find(|t| m.tickets[t].kind == "change" && **t != 200).unwrap();
    assert_eq!(m.view(t).unwrap().title, "The search field's placeholder is cut in With you, and the…");
    assert_eq!(m.view(t).unwrap().title, label::ticket(&m, t, &label::own_words));
    // one word in a form of one Text field: those words, with no field's name in front, as two or more are
    // (paper-research N-1446 p3189: his one-word thread read "text hello")
    let out = m.apply(Input::Open { kind: "change".into(), under: None, opener: "owner".into(), form: json!({"words": "hello"}), text: None, names: vec![], told: vec![] }, "2026-11-16T15:00:03Z".into());
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let h = *m.tickets.keys().find(|t| m.tickets[t].kind == "change" && m.tickets[t].form.get("words") == Some(&json!("hello"))).unwrap();
    assert_eq!((label::ticket(&m, h, &label::own_words), m.view(h).unwrap().title), ("hello".to_string(), "hello".to_string()));
}

#[test]
fn an_agent_reads_as_what_it_is_where_it_works() {
    let m = machine();
    // a role other than member
    assert_eq!(label::agent(&m, "orch", &label::own_words), "orchestrator");
    // `let agent minder`: its name, of what names the ticket it was made in
    let minder = m.made_for(202).into_iter().next().expect("the minder");
    assert_eq!(label::agent(&m, &minder, &label::own_words), "minder of part about-note");
    // an `Agent` field of the newest open ticket naming it (it is P-201's worker too)
    assert_eq!(label::agent(&m, "impl1", &label::own_words), "implementer of part about-note");
    assert_eq!(label::agent(&m, "nobody", &label::own_words), "member");
}

/// An agent that only held a step reads as that step's call by its name, not its doc (paper-research N-101 p815).
#[test]
fn an_agent_of_a_step_reads_as_its_calls_name() {
    let mut m = machine();
    let (call, worker) = m.tickets[&202].step_calls[&1].clone();
    m.defs.edit().call_docs.insert(call.clone(), "a look at the work".into());
    // the minder is a `let agent`: make the step's holder another agent's for the case
    m.tickets.edit().get_mut(&202).unwrap().step_calls.insert(1, (call.clone(), "looker".into()));
    assert_eq!(label::agent(&m, "looker", &label::own_words), format!("{call} of part about-note"));
    assert!(!worker.is_empty());
}

/// Agents labelled together (`label::agents`, the tickets read once for them all; paper-research N-1446 p2432) read
/// as each labelled alone. On P-202 the minder was made, impl1 is named in its form, looker holds its first step and
/// the minder a later one: each keeps its own word there, whoever else is labelled with it.
#[test]
fn agents_labelled_together_read_as_each_alone() {
    let mut m = machine();
    let minder = m.made_for(202).into_iter().next().expect("the minder");
    let (call, _) = m.tickets[&202].step_calls[&1].clone();
    m.tickets.edit().get_mut(&202).unwrap().step_calls.insert(1, (call.clone(), "looker".into()));
    m.tickets.edit().get_mut(&202).unwrap().step_calls.insert(9, (call.clone(), minder.clone()));
    let names = ["orch", minder.as_str(), "impl1", "looker", "nobody", "impl1"];
    let alone: Vec<String> = names.iter().map(|a| label::agent(&m, a, &label::own_words)).collect();
    assert_eq!(label::agents(&m, &names, &label::own_words), alone);
    let looker = format!("{call} of part about-note");
    assert_eq!(alone, ["orchestrator", "minder of part about-note", "implementer of part about-note", looker.as_str(), "member", "implementer of part about-note"]);
}

#[test]
fn a_step_reads_as_its_calls_doc_else_its_name() {
    let mut m = machine();
    let (call, worker) = m.tickets[&202].step_calls[&1].clone();
    assert!(worker.starts_with('f'), "{worker}");
    assert_eq!(label::step(&m, 202, 1), Some(call.clone()));
    m.defs.edit().call_docs.insert(call.clone(), "a look at the work".into());
    assert_eq!(label::step(&m, 202, 1), Some("a look at the work".to_string()));
    assert_eq!(label::step(&m, 202, 9), None);
}

#[test]
fn a_first_line_is_cut_at_a_word_without_its_marks() {
    assert_eq!(label::first_line("\n- **Line layout.** My plan", 80), "Line layout. My plan");
    assert_eq!(label::first_line("**Design for C-1: `notes/about.txt` in repository a**", 80), "Design for C-1: `notes/about.txt` in repository a");
    assert_eq!(label::first_line("# Design\nx", 80), "Design");
    assert_eq!(label::first_line("In repository a, add notes/about.txt: one paragraph", 30), "In repository a, add…");
}

const BUILD: &str = "/// a build of the cases
/// on a batch: from the cut to accepted
kind build
  letter B
  result Text
  pattern
    b <- step runner \"Cut\" -> Cut(carries: [Ticket]);
    v <- step runner \"Cases\" -> Green | Red(log: Text);
    match v {
      Green  => return \"on the line\"
      Red(l) => return l
    }
";

/// A ticket a binding opens has no words and no parent: it reads as its kind's doc line, the tickets its answers
/// name and where it is; never as an answer's head, "cut: Cut" (paper-research N-101 p931).
#[test]
fn a_ticket_with_no_words_reads_as_its_kind_what_it_carries_and_where_it_is() {
    let mut files: Vec<(String, String)> = KINDS.iter().map(|(f, t)| (f.to_string(), t.to_string())).collect();
    files.push(("build.kind".into(), BUILD.into()));
    let (d, errors) = Defs::build(&files, BTreeMap::new(), BTreeMap::new(), 200);
    assert!(errors.is_empty(), "{errors:?}");
    let mut m = Machine::load(d);
    let at = |s: u32| format!("2026-11-16T16:00:{s:02}Z");
    m.apply(Input::Role { agent: "runner".into(), role: "runner".into(), also: vec![] }, at(0)).reply.unwrap();
    m.apply(Input::Open { kind: "change".into(), under: None, opener: "owner".into(), form: json!({"words": "Add a note"}), text: None, names: vec![], told: vec![] }, at(1)).reply.unwrap();
    m.apply(Input::Open { kind: "build".into(), under: None, opener: "owner".into(), form: json!({}), text: None, names: vec![], told: vec![] }, at(2)).reply.unwrap();
    let b = *m.tickets.iter().find(|(_, x)| x.kind == "build").unwrap().0;
    let label = |m: &Machine| label::ticket(m, b, &label::own_words);
    // nothing answered yet: its kind's doc line
    assert_eq!(label(&m), "a build of the cases");
    // the runner's answer is posted as "cut: Cut": an outcome, not the build's words
    let out = m.apply(Input::Answer { t: b, by: "runner".into(), head: json!({"head": "Cut", "carries": ["C-200"], "of": m.id(b), "merged_back": [{"ticket": "C-200", "commit": "abc"}]}), text: Text::New("cut: Cut".into()) }, at(3));
    out.reply.unwrap();
    assert_eq!(label(&m), "a build of the cases, Add a note (C-200): cut");
    m.apply(Input::Answer { t: b, by: "runner".into(), head: json!({"head": "Red", "log": "case-4"}), text: Text::New("cases: Red".into()) }, at(4)).reply.unwrap();
    assert_eq!(label(&m), "a build of the cases, Add a note (C-200): done");
    assert_eq!(m.view(b).unwrap().title, "a build of the cases, Add a note (C-200): done");
}

/// The tickets a build carries are read by what names them with their ids, at most three, "and N more" after
/// (paper-research N-101 p944).
#[test]
fn the_tickets_carried_read_by_what_names_them_at_most_three() {
    let mut files: Vec<(String, String)> = KINDS.iter().map(|(f, t)| (f.to_string(), t.to_string())).collect();
    files.push(("build.kind".into(), BUILD.into()));
    let (d, errors) = Defs::build(&files, BTreeMap::new(), BTreeMap::new(), 200);
    assert!(errors.is_empty(), "{errors:?}");
    let mut m = Machine::load(d);
    let at = |s: u32| format!("2026-11-16T16:00:{s:02}Z");
    m.apply(Input::Role { agent: "runner".into(), role: "runner".into(), also: vec![] }, at(0)).reply.unwrap();
    for (i, w) in ["The first one", "The second one", "The third one", "The fourth one"].iter().enumerate() {
        m.apply(Input::Open { kind: "change".into(), under: None, opener: "owner".into(), form: json!({"words": w}), text: None, names: vec![], told: vec![] }, at(1 + i as u32)).reply.unwrap();
    }
    let changes: Vec<String> = m.tickets.values().filter(|x| x.kind == "change").map(|x| m.id(x.t)).collect();
    m.apply(Input::Open { kind: "build".into(), under: None, opener: "owner".into(), form: json!({}), text: None, names: vec![], told: vec![] }, at(9)).reply.unwrap();
    let b = *m.tickets.iter().find(|(_, x)| x.kind == "build").unwrap().0;
    m.apply(Input::Answer { t: b, by: "runner".into(), head: json!({"head": "Cut", "carries": changes}), text: Text::New("cut: Cut".into()) }, at(10)).reply.unwrap();
    assert_eq!(label::ticket(&m, b, &label::own_words), format!("a build of the cases, The first one ({}), The second one ({}), The third one ({}) and 1 more: cut", changes[0], changes[1], changes[2]));
}

/// A name in words (`names_agent`): with the `@` as `at_name` has always read it, the rule a post's names are made by
/// (a replay names the same), and bare for coordd's `next` line (paper-research N-1397.p169, proto-reader p171): one
/// dash rule, a `-` after the name blocks it either way.
#[test]
fn a_name_said_as_a_word_of_its_own() {
    use crate::machine::{at_name, names_agent};
    assert!(at_name("@fixer use it", "fixer") && at_name("x-@w2 and", "w2") && at_name("(@w2)", "w2") && at_name("@w2.", "w2"));
    assert!(!at_name("@fixers", "fixer") && !at_name("@w2- then", "w2") && !at_name("a@w2", "w2") && !at_name("_@w2", "w2") && !at_name("w2 alone", "w2") && !at_name("@w2x @w2_a @w2-3", "w2"));
    assert!(at_name("say @ to all", ""), "as before: the empty name after an @ standing apart");
    assert!(names_agent("**Reads:** consult-3 and proto-reader read the test commit", "proto-reader", true));
    assert!(names_agent("thanks, w2.", "w2", true) && names_agent("(w2)", "w2", true) && names_agent("@w2, see this", "w2", true) && names_agent("x-@w2", "w2", true));
    assert!(!names_agent("docket-core2 reads it", "docket-core", true) && !names_agent("on proto-reader-ins3", "proto-reader", true) && !names_agent("w2- then", "w2", true));
    assert!(!names_agent("aw2 w2x w2_a x-w2 a@w2", "w2", true) && !names_agent("docket-core-head", "docket-core", true) && !names_agent("anything", "", true));
    assert!(names_agent("docket-core2 then docket-core", "docket-core", true), "a later one apart");
    assert!(!names_agent("thanks, w2.", "w2", false), "bare only when asked");
}
