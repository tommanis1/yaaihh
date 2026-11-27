//! Facts that acts make and end (effects.rs; the engine page's move 6, piece 1): a record line is the act its `ev`
//! names, its `Creates` and `Terminates` done with the line's fields; the store folds the record under the policy in
//! force, and again when the policy's files change.

use super::effects::{Effects, Rules, Store};
use crate::policy::syntax::{parse, DeclKind, Expr};

/// A policy of its own in a folder of its own: `working(agent)` made by `given` and ended by `turn_ended`.
const WORKING: &str = "\
Fact agent Identified by String
Fact working Identified by agent
// an agent given its turn's input is working
Event given
  Related to to
  Creates working(to)
// until its turn ends
Event turn_ended
  Related to agent
  Terminates working(agent)
";

fn places(tag: &str, files: &[(&str, &str)]) -> (crate::policy::Places, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("coordd-effects-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("policy")).unwrap();
    for (f, text) in files {
        std::fs::write(dir.join("policy").join(f), text).unwrap();
    }
    (crate::policy::Places { base: dir.display().to_string(), coord: dir.display().to_string(), ..Default::default() }, dir)
}

fn working(e: &Effects) -> Vec<String> {
    e.facts.iter().filter(|(f, _)| f == "working").map(|(_, a)| a[0].clone()).collect()
}

const RECORD: &str = r#"{"ev":"open","at":"x","t":1,"kind":"note","opener":"owner","form":{},"text":"hi"}
{"ev":"given","at":"x","t":1,"to":"ann","delivery":"N-1.p1","upto":1}
{"ev":"given","at":"x","t":1,"to":"bob","delivery":"N-1.p1","upto":1}
{"ev":"given","at":"x","t":1,"to":"bob","delivery":"N-1.p2","upto":2}
{"ev":"turn_ended","at":"x","agent":"ann","delivery":"N-1.p1","text":{"file":"f","offset":0,"len":1}}
{"ev":"given","at":"x","t":1,"delivery":"N-1.p3","upto":3}
"#;

#[test]
fn an_events_creates_and_terminates_parse_and_name_only_its_fields() {
    let d = parse("p.eflint", WORKING).unwrap();
    let effects: Vec<_> = d.iter().filter_map(|d| match &d.kind { DeclKind::Event { name, effects, .. } => Some((name.clone(), effects.clone())), _ => None }).collect();
    assert_eq!(effects.len(), 2);
    assert_eq!((effects[0].0.as_str(), effects[0].1[0].creates, effects[0].1[0].fact.as_str()), ("given", true, "working"));
    assert_eq!(effects[0].1[0].args, vec![Expr::Field("to".into())]);
    assert_eq!((effects[1].0.as_str(), effects[1].1[0].creates), ("turn_ended", false));
    // a name that is no field of the act, and an effect not compiled, are refused with their line
    let e = parse("p.eflint", "Event given\n  Related to to\n  Creates working(agent)\n").unwrap_err();
    assert!(e.contains("`agent` is not a field of Event given"), "{e}");
    let e = parse("p.eflint", "Event given\n  Related to to\n  Obfuscates working(to)\n").unwrap_err();
    assert!(e.contains("Related to, Creates and Terminates lines only"), "{e}");
    // an act's effects stay refused: an act is asked whether it may be done, an event was
    let e = parse("p.eflint", "Act given\n  Related to agent\n  Holds when true\n  Creates working(agent)\n").unwrap_err();
    assert!(e.contains("not compiled yet"), "{e}");
}

#[test]
fn the_record_folded_holds_what_its_acts_made_and_did_not_end() {
    let (p, dir) = places("fold", &[("working.eflint", WORKING)]);
    let rules = Rules::of(&p).unwrap();
    // ann's turn ended, bob's two inputs make one fact, and a line without the field makes none
    assert_eq!(working(&Effects::of_record(&rules, RECORD)), vec!["bob"]);
    // a fact the policy does not declare, or with another count, is refused with the effect's line
    std::fs::write(dir.join("policy/working.eflint"), WORKING.replace("Fact working Identified by agent", "Fact working Identified by agent * String")).unwrap();
    let e = Rules::of(&p).unwrap_err();
    assert!(e.contains("Creates working(to)") && e.contains("identified by 2, not 1"), "{e}");
    // a machine's `event` line is the event its name names, its fields its value's; another event's line is not read
    let began = format!("{WORKING}Event turn_began\n  Related to agent\n  Creates working(agent)\n");
    std::fs::write(dir.join("policy/working.eflint"), began).unwrap();
    let lines = r#"{"ev":"event","at":"x","name":"turn_began","value":{"agent":"dy","delivery":"N-1.p5"}}
{"ev":"event","at":"x","name":"mail_hold","value":{"agent":"ed"}}
{"ev":"event","at":"x","name":"turn_began","value":{"delivery":"N-1.p6"}}
"#;
    assert_eq!(working(&Effects::of_record(&Rules::of(&p).unwrap(), &format!("{RECORD}{lines}"))), vec!["bob", "dy"]);
    // the decisions given the store's facts: one whose rule reads `working`, or a fact derived from it; not another
    let reading = format!("{WORKING}Fact busy Identified by agent\n  Derived from busy(agent) When working(agent)\nDecision a\n  Options Yes | No\nOnly for a When working(call.who):\n  option == \"Yes\"\nDecision b\n  Options Yes | No\nPrefer for b:\n  busy(call.who) > option == \"No\"\nDecision c\n  Options Yes | No\nOnly for c When call.who == \"ann\":\n  option == \"No\"\n");
    std::fs::write(dir.join("policy/working.eflint"), reading).unwrap();
    assert_eq!(Rules::of(&p).unwrap().readers.into_iter().collect::<Vec<_>>(), vec!["a", "b"]);
    // a policy with no event reads no line
    std::fs::write(dir.join("policy/working.eflint"), "Fact agent Identified by String\n").unwrap();
    assert!(Effects::of_record(&Rules::of(&p).unwrap(), RECORD).facts.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_written_line_is_done_at_once_and_a_changed_policy_folds_the_record_again() {
    let (p, dir) = places("store", &[("working.eflint", WORKING)]);
    let log = dir.join("log.jsonl");
    std::fs::write(&log, RECORD).unwrap();
    let (mut s, why) = Store::start(&p, RECORD);
    assert_eq!(why, None);
    let line = r#"{"ev":"given","at":"x","t":1,"to":"cy","delivery":"N-1.p4","upto":4}"#.to_string();
    assert_eq!(s.take(&p, &log.display().to_string(), std::slice::from_ref(&line)), None);
    assert_eq!(working(&s.held), vec!["bob", "cy"]);
    // the policy changes: a turn's end no longer ends it; the record (with cy's line) is folded under the new rules
    std::fs::write(&log, format!("{RECORD}{line}\n")).unwrap();
    std::fs::write(dir.join("policy/working.eflint"), WORKING.replace("Terminates", "Creates")).unwrap();
    // a file's time can be the old one within the same second: the new text is seen when the stamp differs
    let f = std::fs::File::options().write(true).open(dir.join("policy/working.eflint")).unwrap();
    f.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5)).unwrap();
    s.checked = None;
    assert_eq!(s.take(&p, &log.display().to_string(), &[]), None);
    assert_eq!(working(&s.held), vec!["ann", "bob", "cy"]);
    // a policy that does not read keeps the rules read before, and says so
    std::fs::write(dir.join("policy/working.eflint"), format!("{WORKING}Event esc\n  Related to agent\n  Creates nothing(agent)\n")).unwrap();
    let f = std::fs::File::options().write(true).open(dir.join("policy/working.eflint")).unwrap();
    f.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(10)).unwrap();
    s.checked = None;
    let why = s.take(&p, &log.display().to_string(), &[]).unwrap();
    assert!(why.contains("nothing is no fact type") && why.contains("kept"), "{why}");
    assert_eq!(working(&s.held), vec!["ann", "bob", "cy"]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A machine's `event` line and an effect's `When` together (docket-edge6 N-1446 p3138, 1; H-1651 merged with H-1654):
/// the `When` compares the fields of the line's `value`, not the line's own, and a Terminates' `Foreach` ends every
/// instance the other arguments match.
#[test]
fn an_event_lines_value_is_what_an_effects_when_compares() {
    const BEGAN: &str = "\
Fact agent Identified by String
Fact working Identified by agent * String
Event turn_began
  Related to agent, delivery, kind
  Creates working(agent, delivery) When kind == \"turn\"
  Terminates (Foreach string: working(agent, string)) When kind == \"stop\"
";
    let (p, dir) = places("eventwhen", &[("began.eflint", BEGAN)]);
    let rules = Rules::of(&p).unwrap();
    let lines = r#"{"ev":"event","at":"x","name":"turn_began","value":{"agent":"dy","delivery":"N-1.p5","kind":"turn"}}
{"ev":"event","at":"x","name":"turn_began","value":{"agent":"dy","delivery":"N-1.p6","kind":"turn"}}
{"ev":"event","at":"x","name":"turn_began","value":{"agent":"ed","delivery":"N-1.p7","kind":"other"}}
{"ev":"event","at":"x","name":"turn_began","kind":"turn","value":{"agent":"fi","delivery":"N-1.p8"}}
"#;
    let held = |text: &str| Effects::of_record(&rules, text).facts.into_iter().map(|(_, a)| a.join(" ")).collect::<Vec<_>>();
    assert_eq!(held(lines), vec!["dy N-1.p5", "dy N-1.p6"], "the value's `kind` decides; a `kind` beside the value is not the event's");
    let stop = r#"{"ev":"event","at":"x","name":"turn_began","value":{"agent":"dy","delivery":"","kind":"stop"}}"#;
    assert!(held(&format!("{lines}{stop}\n")).is_empty(), "the Foreach ends both of dy's");
    let _ = std::fs::remove_dir_all(&dir);
}
