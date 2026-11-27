//! An outcome written inline in a signature may have its fields one a line under `Outcome(`, each with its doc, as a
//! declared type's fields have theirs (tovrin-writer N-302, for trace's Part; paper-research N-101 p955): the step's
//! gives shows them under the outcome.

use crate::syntax::{self, Named};

const CALLS: &str = "\
/// the one piece whose commit turned the build red
call trace(log: Text)
     -> Part(                 /// one piece turned it red
          piece: Ticket,      /// the piece
          /// the piece's commit that turned it red,
          commit: Ref,        /// with its repository, as `a 1a2b3c4`
          why: Text)          /// why this piece
      | Unclear(why: Text)    /// no one piece can be named
  template
    T.
";

fn gives(text: &str) -> Result<String, Vec<String>> {
    let files = vec![("t.call".to_string(), text.to_string())];
    let mut named = Named::new();
    let (cs, errors) = syntax::parse_call_files(&files, &mut named);
    if !errors.is_empty() {
        return Err(errors);
    }
    let d = &cs["trace"];
    Ok(crate::doc::gives(&d.sig.head, Some(&d.doc), &syntax::type_docs(&files), &named))
}

#[test]
fn an_inline_outcomes_fields_have_their_docs() {
    assert_eq!(gives(CALLS).unwrap(), "\
This step gives one of:
  Part     one piece turned it red
    piece: Ticket  the piece
    commit: Ref    the piece's commit that turned it red, with its repository, as `a 1a2b3c4`
    why            why this piece
  Unclear  no one piece can be named
    why");
}

#[test]
fn a_doc_going_on_under_an_outcomes_line_stays_the_outcomes() {
    let text = CALLS.replace("     -> Part(                 /// one piece turned it red\n", "     -> Part(                 /// one piece\n                              /// turned it red\n");
    assert!(gives(&text).unwrap().contains("  Part     one piece turned it red\n    piece: Ticket  the piece\n"));
}

#[test]
fn a_doc_on_a_line_of_two_fields_is_refused() {
    let text = CALLS.replace("          piece: Ticket,      /// the piece\n          /// the piece's commit that turned it red,\n          commit: Ref,  ", "          piece: Ticket, commit: Ref,");
    let e = gives(&text).unwrap_err().join("\n");
    assert!(e.contains("t.call:4: a doc (`///`) stands on one part, and this line has piece, commit"), "{e}");
}
