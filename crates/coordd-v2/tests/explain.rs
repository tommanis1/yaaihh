//! `explain` and a refused act's `why` on the daemon's machine over docket-v2's core, as JSON, writing nothing.

use coordd_v2::docket::Docket;
use coordd_v2::machine::Machine;
use coordd_v2::peer::Party;
use docket_v2::norm::Core;
use docket_v2::run::machine::Machine as Runs;
use serde_json::{json, Value as Json};

const TEXT: &str = r#"module t
Fact Person Identified by String
Fact Book Identified by String
Fact Member(p: Person)
Fact Banned(p: Person)
Fact Shelved(b: Book)
Fact Lent(b: Book)
Fact Trusted(p: Person) Holds when Member(p) && Not(Banned(p))
Fact May-Borrow(p: Person, b: Book) Holds when Trusted(p) && Shelved(b)
Act Join(by: Person) Creates Member(by)
Act Shelve(by: Person, b: Book) Creates Shelved(b)
Act Ban(by: Person, who: Person) Creates Banned(who)
Act Borrow(by: Person, b: Book)
  Conditioned by May-Borrow(by, b)
  Conditioned by Member(by) && Not(Lent(b))
  Creates Lent(b)
+Person("ann"). +Person("bob"). +Person("carl").
"#;

fn docket() -> Docket<Core> {
    let l = coordd_v2::texts::load(&[("t.docket".into(), TEXT.into())]).unwrap();
    let runs = Runs::new(l, Core::default(), Vec::new()).unwrap();
    Docket::new(runs, "t", Vec::new(), Box::new(|_, _| (None, String::new())), Box::new(|_| Err("no old texts".into())))
}

/// Act `t.NAME` with ARGS, asked for by the party its first names.
fn act(d: &mut Docket<Core>, name: &str, args: &[&str]) -> Json {
    let args: Vec<Json> = args.iter().map(|a| json!(a)).collect();
    d.act(&Party::of(args[0].as_str().unwrap(), None), &format!("t.{name}"), &args).unwrap()
}

fn texts(ps: &Json) -> Vec<&str> {
    ps.as_array().unwrap().iter().map(|p| p["text"].as_str().unwrap()).collect()
}

/// A refused act's reply keeps its fields and adds `why`, down to the step that made it fail; its record line is unchanged.
#[test]
fn a_refused_act_says_why_beside_what_its_reply_and_line_said() {
    let mut d = docket();
    let joined = act(&mut d, "Join", &["ann"]);
    assert!(joined.get("why").is_none() && joined.get("run").is_some(), "{joined}");
    act(&mut d, "Shelve", &["ann", "b1"]);
    act(&mut d, "Ban", &["bob", "ann"]);
    d.written();
    let r = act(&mut d, "Borrow", &["ann", "b1"]);
    assert!(r.get("run").is_some() && r.get("state").is_some(), "{r}");
    let why = &r["why"];
    assert!(why["failing"].is_array(), "{r}");
    assert_eq!(texts(&why["failing"]), ["t.May-Borrow(by, b)"], "{why}");
    let p = &why["failing"][0];
    assert_eq!((p["at"].as_str().unwrap().split(':').nth(1), &p["bindings"], &p["holds"]), (Some("14"), &json!({"b": "b1", "by": "ann"}), &json!(false)));
    let trusted = &p["facts"][0]["rules"][0]["premises"][0]["facts"][0];
    assert_eq!(trusted["inst"], json!("t.Trusted(\"ann\")"));
    let banned = &trusted["rules"][0]["premises"][0]["parts"][0]["facts"][0];
    assert_eq!((&banned["holds"], &banned["by"]["stated"]), (&json!(true), &json!({"step": 3, "act": "t.Ban(\"bob\", \"ann\")"})));
    let lines = d.written();
    let refused: Vec<&Json> = lines.iter().filter_map(|l| l.get("refused")).collect();
    assert_eq!(refused.len(), 1, "{lines:?}");
    assert_eq!(refused[0]["why"], json!({"policy": "not enabled"}), "the line is as it was");
}

/// `explain` answers for a fact, true or not, and for an act, enabled or not, and writes no line.
#[test]
fn explain_answers_for_facts_and_acts_and_writes_nothing() {
    let mut d = docket();
    act(&mut d, "Join", &["ann"]);
    act(&mut d, "Shelve", &["ann", "b1"]);
    d.written();
    let party = Party::of("ann", None);
    let may = d.explain(&party, "May-Borrow(\"ann\", \"b1\")").unwrap();
    assert_eq!((&may["holds"], texts(&may["by"]["rule"]["premises"])), (&json!(true), vec!["t.Trusted(p)", "t.Shelved(b)"]));
    let member = d.explain(&party, "Member(\"ann\")").unwrap();
    assert_eq!(member["by"]["stated"], json!({"step": 1, "act": "t.Join(\"ann\")"}));
    let carl = d.explain(&party, "Borrow(\"carl\", \"b1\")").unwrap();
    assert_eq!(texts(&carl["failing"]), ["t.May-Borrow(by, b)", "t.Member(by)"]);
    let ann = d.explain(&party, "Borrow(\"ann\", \"b1\")").unwrap();
    assert_eq!((&ann["failing"], &ann["syncs"]), (&json!([]), &json!([])), "enabled");
    assert!(d.explain(&party, "1 + 2").is_err());
    for wrong in ["Borrow(\"ann\")", "Borrow(\"ann\", \"b1\", \"x\")"] {
        assert!(d.explain(&party, wrong).is_err(), "{wrong}: {:?}", d.explain(&party, wrong));
    }
    assert!(d.written().is_empty(), "asking writes nothing");
}
