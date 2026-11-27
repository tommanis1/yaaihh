//! A list of an answer whose entries name another list's elements (`Defs.names`: `stands_on` names `parts[].name`,
//! paper-research N-101 p1141) is formed as those names: an entry the reading gives is kept whole, none cut at a
//! comma (run 4: "one engine, one name, run alone" became three); one line naming two parts gives both; an entry that
//! names none is refused, all of them at once with the names there are, as the agent is told it.

use crate::form::parts::{self, Part};
use docket_machine::Type;
use serde_json::{Map, Value, json};

fn design() -> Vec<(String, Type)> {
    let part = Type::Record(vec![("name".into(), Type::Text), ("stands_on".into(), Type::List(Box::new(Type::Text))), ("after".into(), Type::Text)]);
    vec![("asked".into(), Type::Text), ("parts".into(), Type::List(Box::new(part)))]
}

fn names() -> Vec<(String, String)> {
    vec![("Design.parts[].stands_on".to_string(), "Design.parts[].name".to_string())]
}

const ANSWER: &str = "\
Design

Asked
The owner's words.

Part one engine, one name, run alone
Stands on: none
After
The registry.

Part every external tool at its pin, stock
Stands on: one engine, one name, run alone
After
Pins.

Part every external engine decides both question classes
Stands on: one engine, one name, run alone; every external tool at its pin, stock
After
Both classes.
";

/// The parts of ANSWER as a reading by marks gives them, with STANDS as the stands_on of each, formed with RULES.
fn by_marks(stands: [Value; 3], rules: &[(String, String)]) -> Result<Value, String> {
    let ps = parts::parts(&design());
    let names = ["one engine, one name, run alone", "every external tool at its pin, stock", "every external engine decides both question classes"];
    let afters = [9, 14, 19];
    let ps_marks: Vec<Value> = (0..3).map(|i| json!({"name": names[i], "stands_on": stands[i], "after": {"first": afters[i], "last": afters[i]}})).collect();
    let a = json!({"head": "Design", "asked": {"first": 4, "last": 4}, "parts": ps_marks});
    let whole = parts::whole(rules, "Design");
    let found = parts::by_marks_with(&a, ANSWER, &ps, &whole)?;
    let mut o = Map::new();
    o.insert("head".into(), json!("Design"));
    parts::fill_with(&mut o, &ps, &found, ANSWER, None, &whole);
    let mut v = Value::Object(o);
    parts::named(&mut v, rules)?;
    Ok(v)
}

fn stands(v: &Value) -> Vec<Value> {
    v["parts"].as_array().unwrap().iter().map(|p| p["stands_on"].clone()).collect()
}

#[test]
fn a_name_with_commas_given_by_the_reading_is_one_name() {
    let v = by_marks([json!([]), json!(["one engine, one name, run alone"]), json!(["one engine, one name, run alone", "every external tool at its pin, stock"])], &names()).unwrap();
    assert_eq!(stands(&v), vec![json!([]), json!(["one engine, one name, run alone"]), json!(["one engine, one name, run alone", "every external tool at its pin, stock"])]);
    // one entry naming two parts, as the agent wrote them on one line, gives both
    let v = by_marks([json!([]), json!([]), json!(["one engine, one name, run alone; every external tool at its pin, stock"])], &names()).unwrap();
    assert_eq!(stands(&v)[2], json!(["one engine, one name, run alone", "every external tool at its pin, stock"]));
}

#[test]
fn without_the_rule_a_listed_name_is_still_cut_at_its_commas() {
    let v = by_marks([json!([]), json!(["a, b"]), json!([])], &[]).unwrap();
    assert_eq!(stands(&v)[1], json!(["a", "b"]));
}

#[test]
fn the_parts_cut_at_their_headings_keep_a_name_with_commas() {
    let ps = parts::parts(&design());
    let found = parts::by_headings(ANSWER, &ps).unwrap();
    let mut o = Map::new();
    o.insert("head".into(), json!("Design"));
    parts::fill_with(&mut o, &ps, &found, ANSWER, None, &parts::whole(&names(), "Design"));
    let mut v = Value::Object(o);
    parts::named(&mut v, &names()).unwrap();
    assert_eq!(stands(&v), vec![json!([]), json!(["one engine, one name, run alone"]), json!(["one engine, one name, run alone", "every external tool at its pin, stock"])]);
}

#[test]
fn entries_that_name_no_part_are_refused_all_at_once_with_the_names() {
    let why = by_marks([json!(["I-227"]), json!(["Part 1"]), json!(["Part 1", "every external tool at its pin, stock"])], &names()).unwrap_err();
    assert_eq!(
        why,
        "Each entry of stands_on is one of the parts' names, letter for letter, and these name none: \"I-227\" (stands_on of parts 1), \"Part 1\" (stands_on of parts 2), \"Part 1\" (stands_on of parts 3). \
         The parts' names are: \"one engine, one name, run alone\"; \"every external tool at its pin, stock\"; \"every external engine decides both question classes\"."
    );
}

#[test]
fn the_reading_is_asked_for_the_parts_by_their_names() {
    let ps = vec![("Design".to_string(), parts::parts(&design()))];
    let p = parts::marks_prompt_with(&["Design".to_string()], "None", &ps, "Design\nx", None, None, None, &names());
    assert!(p.contains("as values: name as the answer writes it, without the marks around it; stands_on as the parts it lists, each by the name you give it, letter for letter, [] where it says there are none"), "{p}");
    assert!(matches!(ps[0].1[1].1, Part::Records(_)));
}
