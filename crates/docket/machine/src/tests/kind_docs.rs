//! A doc written `kind NAME` is that kind's first `///` line (paper-research N-101 p906): `take`'s Change and Direct
//! show what `kind change` and `kind direct` say, and change when those docs do; a kind that is not there is said.

use crate::syntax::{self, Doc};
use std::collections::BTreeMap;

const KINDS: &str = "\
// a comment, not a doc
/// a change: designed, built in parts, accepted
/// the owner's words to built code
kind change
  form words: Text
  result Text
  pattern
    return words

/// a change made directly
kind direct(agent: Agent)
  result Text
  pattern
    return \"x\"

kind bare
  result Text
  pattern
    return \"x\"
";

fn lines() -> BTreeMap<String, String> {
    syntax::kind_lines(&[("lib.kind".into(), KINDS.into())])
}

#[test]
fn each_kind_gives_its_first_doc_line() {
    let l = lines();
    assert_eq!(l.get("change").map(String::as_str), Some("a change: designed, built in parts, accepted"));
    assert_eq!(l.get("direct").map(String::as_str), Some("a change made directly"));
    assert_eq!(l.get("bare").map(String::as_str), Some(""));
}

#[test]
fn a_doc_naming_a_kind_takes_its_line_and_its_parts_too() {
    let mut d = Doc { text: "kind change".into(), parts: BTreeMap::from([("Direct".into(), "kind direct".into()), ("Talk".into(), "a kind of talk".into())]) };
    assert!(syntax::kind_docs(&mut d, &lines()).is_empty());
    assert_eq!(d.text, "a change: designed, built in parts, accepted");
    assert_eq!(d.parts["Direct"], "a change made directly");
    // a doc that says more than `kind NAME` is a doc as written
    assert_eq!(d.parts["Talk"], "a kind of talk");
}

#[test]
fn a_kind_not_there_or_with_no_doc_is_said() {
    let mut d = Doc { text: "kind chnage".into(), parts: BTreeMap::from([("B".into(), "kind bare".into())]) };
    assert_eq!(syntax::kind_docs(&mut d, &lines()), vec!["kind chnage: no such kind".to_string(), "kind bare has no `///` line".to_string()]);
    assert_eq!(d.text, "kind chnage");
}
