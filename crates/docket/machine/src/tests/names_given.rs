//! Which lists of a step's answer name elements of another list of it (paper-research N-101 p1141, run 4's
//! `stands_on`), read off the checked kinds: a `par` over the answer's list, `as p.name after p.stands_on`, says that
//! each entry of `stands_on` is one of the parts' names. A `par` whose names come from elsewhere says nothing.

use crate::check::CallSig;
use crate::machine::Defs;
use crate::syntax::{self, Named};
use std::collections::{BTreeMap, BTreeSet};

const CALLS: &str = "\
type Part {
  name: Text
  stands_on: [Text]
}

/// the design for the owner's words
call design(words: Text) -> Design(asked: Text, parts: [Part]) | Ask(question: Text)
  template
    D.

/// one part built
call build(part: Text) -> Commit(c: Text)
  template
    B.
";

const KINDS: &str = "\
kind change
  form words: Text
  result [Text]
  raises NoOrder, Stalled, Exhausted
  pattern
    d <- step lead design(words);
    match d {
      Design(_, parts) =>
        tops <- par part in parts up to 8 as part.name after part.stands_on {
          c <- step lead build(part.name);
          match c { Commit(x) => return x }
        };
        return tops
      Ask(q) => return []
    }

kind again
  form words: Text
  result [Text]
  raises NoOrder, Stalled, Exhausted
  pattern
    iterate request = none up to 3 {
      d <- step lead design(words);
      match d {
        Design(_, parts) =>
          tops <- par part in parts up to 8 as part.name after part.stands_on {
            c <- step lead build(part.name);
            match c { Commit(x) => return x }
          };
          return Done(tops)
        Ask(q) => return Again(q)
      }
    }

kind unordered
  form words: Text
  result [Text]
  raises NoOrder, Stalled, Exhausted
  pattern
    d <- step lead design(words);
    match d {
      Design(_, parts) =>
        tops <- par part in parts up to 8 as part.name after [] {
          c <- step lead build(part.name);
          match c { Commit(x) => return x }
        };
        return tops
      Ask(q) => return []
    }
";

fn defs() -> Defs {
    let mut named = Named::new();
    let files = vec![("t.call".to_string(), CALLS.to_string())];
    let (cs, errors) = syntax::parse_call_files(&files, &mut named);
    assert!(errors.is_empty(), "{errors:?}");
    let sigs: BTreeMap<String, CallSig> = cs.iter().map(|(n, c)| (n.clone(), c.sig.clone())).collect();
    let (d, errors) = Defs::build(&[("t.kind".to_string(), KINDS.to_string())], sigs, named, 200);
    assert!(errors.is_empty(), "{errors:?}");
    d
}

fn rule() -> BTreeSet<(String, String)> {
    BTreeSet::from([("Design.parts[].stands_on".to_string(), "Design.parts[].name".to_string())])
}

#[test]
fn a_par_ordered_by_an_answers_list_says_its_entries_name_the_elements() {
    let d = defs();
    assert_eq!(d.names.get(&("change".to_string(), "design".to_string())), Some(&rule()));
}

#[test]
fn the_answer_is_followed_inside_an_iterate() {
    let d = defs();
    // the step inside an iterate's rounds
    assert_eq!(d.names.get(&("again".to_string(), "design".to_string())), Some(&rule()));
}

#[test]
fn a_par_whose_names_come_from_no_answer_says_nothing() {
    let d = defs();
    assert_eq!(d.names.get(&("unordered".to_string(), "design".to_string())), None);
    assert!(d.names.keys().all(|(_, c)| c == "design"), "{:?}", d.names);
}

#[test]
fn the_steps_gives_says_it_under_the_list() {
    let d = defs();
    let files = vec![("t.call".to_string(), CALLS.to_string())];
    let mut named = Named::new();
    let (cs, _) = syntax::parse_call_files(&files, &mut named);
    let types = syntax::type_docs(&files);
    let lines = crate::doc::names_lines(Some("change"), "design", &d.names);
    let shown = crate::doc::gives_with(&cs["design"].sig.head, Some(&cs["design"].doc), &types, &named, &lines);
    assert_eq!(shown, "\
This step gives one of:
  Design
    asked
    parts: [Part]
      name
      stands_on: [Text]
                         each is one of the parts' names, letter for letter
  Ask
    question");
    assert!(crate::doc::names_lines(Some("unordered"), "design", &d.names).is_empty());
}
