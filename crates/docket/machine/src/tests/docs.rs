//! Docs (paper-research N-101 p656): `///` on a kind, a call, a parameter, a type, a field, an outcome and an
//! exception is kept in the loaded definition, apart from a `//` comment; a call's outcome may be a declared record
//! type by name, whose fields and doc it takes, and a kind's pattern matches it as before.

use crate::syntax::{self, Doc, Named};
use crate::types::Type;
use std::collections::BTreeMap;

const CALLS: &str = "\
// a plain comment: not a doc
/// a design: what will be true, not how it is built
type Design {
  asked: Text       /// the owner's words, exactly as typed
  /// what exists and what it does
  today: Text
  parts: [Text]     // a note, not a doc
}

/// one whole question to the owner
type Ask {
  question: Text    /// the question, whole
}

/// the designer's call
call design(
    words: Text,      /// the owner's words
    request: Text?    /// the request the work answers
  ) where model >= small -> Design | Ask
  template
    Design {{ words }}.

call check(commit: Ref)
     -> Green               /// the commit builds and its tests pass
      | Red(log: Text)      /// it does not: the log says where
  template
    Check {{ commit }}.
";

const KIND: &str = "\
// history: who wrote it, when
/// a piece of work the owner asked for
kind change
  form
    words: Text,     /// the owner's words
    request: Text?
  result Text
  raises
    Exhausted        /// the rounds ran out
    Stalled
  pattern
    d <- step ann design(words, request);
    match d {
      Design(asked, today, parts) => return asked
      Ask(question) => return question
    }
";

fn load() -> (BTreeMap<String, syntax::CallDef>, Named) {
    let mut named = Named::new();
    let (calls, errors) = syntax::parse_call_files(&[("t.call".into(), CALLS.into())], &mut named);
    assert!(errors.is_empty(), "{errors:#?}");
    (calls, named)
}

fn doc(text: &str, parts: &[(&str, &str)]) -> Doc {
    Doc { text: text.into(), parts: parts.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect() }
}

#[test]
fn a_types_doc_and_its_fields_docs_are_kept_and_comments_are_not() {
    let docs = syntax::type_docs(&[("t.call".into(), CALLS.into())]);
    assert_eq!(docs["Design"], doc("a design: what will be true, not how it is built", &[("asked", "the owner's words, exactly as typed"), ("today", "what exists and what it does")]));
    assert_eq!(docs["Ask"], doc("one whole question to the owner", &[("question", "the question, whole")]));
}

#[test]
fn a_calls_doc_its_parameters_and_its_inline_outcomes_are_kept() {
    let (calls, _) = load();
    assert_eq!(calls["design"].doc, doc("the designer's call", &[("words", "the owner's words"), ("request", "the request the work answers")]));
    assert_eq!(calls["check"].doc, doc("", &[("Green", "the commit builds and its tests pass"), ("Red", "it does not: the log says where")]));
}

#[test]
fn an_outcome_named_by_a_declared_type_takes_its_fields() {
    let (calls, named) = load();
    let Type::Variant(cs) = &calls["design"].sig.head else { panic!("{:?}", calls["design"].sig.head) };
    let names: Vec<(&str, Vec<&str>)> = cs.iter().map(|(c, fs)| (c.as_str(), fs.iter().map(|(f, _)| f.as_str()).collect())).collect();
    assert_eq!(names, vec![("Design", vec!["asked", "today", "parts"]), ("Ask", vec!["question"])]);
    // a kind's pattern matches its constructors as before, and the kind keeps its docs
    let sigs: BTreeMap<String, crate::CallSig> = calls.into_iter().map(|(n, c)| (n, c.sig)).collect();
    let (d, errors) = crate::Defs::build(&[("t.kind".to_string(), KIND.to_string())], sigs, named.clone(), 300);
    assert!(errors.is_empty(), "{errors:#?}");
    let k = &d.kinds["change"];
    assert_eq!(k.doc, doc("a piece of work the owner asked for", &[("words", "the owner's words"), ("Exhausted", "the rounds ran out")]));
    assert_eq!(k.raises, vec!["Exhausted", "Stalled"]);
    // elsewhere a bare alternative stays a constructor with no fields
    let t = syntax::type_of("Design | Other", 1, &named).unwrap();
    assert_eq!(t, Type::Variant(vec![("Design".into(), vec![]), ("Other".into(), vec![])]));
}

#[test]
fn a_doc_on_a_line_of_two_parts_is_refused() {
    let mut named = Named::new();
    let (_, errors) = syntax::parse_call_files(&[("t.call".into(), "type P {\n  a: Text, b: Text   /// which?\n}\n".into())], &mut named);
    assert_eq!(errors, vec!["t.call:2: a doc (`///`) stands on one part, and this line has a, b: put each on its own line".to_string()]);
}

/// A project's type of the library's name (`Part`, tovrin-writer's branch) replaces it, its docs with it; a call
/// answering it alone by name has its constructor (paper-research N-101 p672).
#[test]
fn a_projects_type_replaces_the_librarys_with_its_docs() {
    let lib = "/// the library's part\ntype Part {\n  name: Text   /// the library's name\n}\n";
    let proj = "/// the project's part\ntype Part {\n  name: Text   /// its name\n  after: Text  /// what will be true\n}\n\ncall c() -> Part\n  template\n    C.\n";
    let files: Vec<(String, String)> = vec![("library/lib.call".into(), lib.into()), ("project/p.call".into(), proj.into())];
    let mut named = Named::new();
    let (calls, errors) = syntax::parse_call_files(&files, &mut named);
    assert!(errors.is_empty(), "{errors:#?}");
    assert_eq!(syntax::type_docs(&files)["Part"], doc("the project's part", &[("name", "its name"), ("after", "what will be true")]));
    assert_eq!(calls["c"].sig.head, Type::Variant(vec![("Part".into(), vec![("name".into(), Type::Text), ("after".into(), Type::Text)])]));
}

/// What a step gives, with its docs (paper-research N-101 p656, p698): the outcomes, their fields with the type beside
/// every one that is not plain Text, and under a field of a declared record type (or a list of one) that type's
/// fields, each type once; one outcome reads "This step gives:".
#[test]
fn what_a_step_gives_shows_its_outcomes_fields_and_the_types_under_them() {
    let calls = "\
/// a design: what will be true, not how it is built
type Design {
  asked: Text      /// the owner's words, exactly as typed
  parts: [Part]    /// what one agent builds alone
}
type Part {
  name: Text
  stands_on: [Text] /// the parts that must be built before this one
  after: Text       /// what will be true when this part is built
}
/// one whole question to the owner
type Ask {
  question: Text
}
call design(words: Text) -> Design | Ask
  template
    D.

call check(c: Ref)
  -> Green          /// it builds
  template
    C.
";
    let files = vec![("t.call".to_string(), calls.to_string())];
    let mut named = Named::new();
    let (cs, errors) = syntax::parse_call_files(&files, &mut named);
    assert!(errors.is_empty(), "{errors:?}");
    let types = syntax::type_docs(&files);
    let d = &cs["design"];
    assert_eq!(crate::doc::gives(&d.sig.head, Some(&d.doc), &types, &named), "\
This step gives one of:
  Design  a design: what will be true, not how it is built
    asked          the owner's words, exactly as typed
    parts: [Part]  what one agent builds alone
      name
      stands_on: [Text]  the parts that must be built before this one
      after              what will be true when this part is built
  Ask     one whole question to the owner
    question");
    let c = &cs["check"];
    assert_eq!(crate::doc::gives(&c.sig.head, Some(&c.doc), &types, &named), "This step gives:\n  Green  it builds");
}

/// A doc that goes on under a part's line, at the column of the `///` that ended it, is that part's; a doc line at
/// another column leads the next part (tovrin-writer's Part and Design, paper-research N-101 p698).
#[test]
fn a_doc_going_on_under_its_line_stays_with_its_part() {
    let text = "\
type Part {
  name: Text      /// a short name
  after: Text     /// what will be true,
                  /// with examples
  /// what the owner runs
  done: Text
}
call c(a: Text,   /// first,
                  /// and more
       b: Text)   /// second
  -> Part
  template
    C.
";
    let files = vec![("t.call".to_string(), text.to_string())];
    assert_eq!(syntax::type_docs(&files)["Part"], doc("", &[("name", "a short name"), ("after", "what will be true,\nwith examples"), ("done", "what the owner runs")]));
    let mut named = Named::new();
    let (calls, errors) = syntax::parse_call_files(&files, &mut named);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(calls["c"].doc, doc("", &[("a", "first,\nand more"), ("b", "second")]));
}
