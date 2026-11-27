//! An outcome whose doc is `kind NAME` shows that kind's first `///` line in what a step gives (paper-research N-101
//! p906): read when the calls load, from the kinds beside them, so it changes when the kind's doc does; a call giving
//! a doc of a kind that is not there is left out at load, with its line.

use crate::policy::call::Calls;

const KIND: &str = "/// a change: designed, built in parts, accepted
kind change
  form words: Text
  result Text
  pattern
    return words

/// a change made directly
kind direct
  form words: Text
  result Text
  pattern
    return words
";

const CALL: &str = "/// kind change
type Change {
  words: Text     /// the owner's message as written
}

/// kind direct
type Direct {
  agent: Agent    /// who makes it
  words: Text     /// the owner's message as written
}

/// what the owner's message opens
call take(words: Text) -> Talk | Change | Direct | Ask
  task take
  template
    {{ words }}

call pick(words: Text)
     -> Talk
      | Change(words: Text)    /// kind change
  task pick
  template
    {{ words }}
";

/// A project folder with `calls/t.call` holding CALLS and `kinds/lib.kind` holding KINDS, its calls loaded; then
/// removed.
fn load(name: &str, calls: &str, kinds: &str) -> Calls {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("coordd-kind-docs-{name}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(dir.join("calls")).unwrap();
    std::fs::create_dir_all(dir.join("kinds")).unwrap();
    std::fs::write(dir.join("calls/t.call"), calls).unwrap();
    std::fs::write(dir.join("kinds/lib.kind"), kinds).unwrap();
    let out = Calls::load(&[dir.join("calls").display().to_string()], &Default::default()).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    out
}

fn gives(c: &Calls, call: &str) -> String {
    let d = &c.defs[call];
    docket_machine::doc::gives(&d.sig.head, Some(&d.doc), &c.type_docs, &c.named)
}

#[test]
fn an_outcome_of_a_kind_shows_the_kinds_line() {
    let c = load("shown", CALL, KIND);
    assert!(c.errors.is_empty(), "{:?}", c.errors);
    let g = gives(&c, "take");
    assert!(g.contains("  Change  a change: designed, built in parts, accepted\n"), "{g}");
    assert!(g.contains("  Direct  a change made directly\n"), "{g}");
    // inline, in the signature
    let g = gives(&c, "pick");
    assert!(g.contains("  Change  a change: designed, built in parts, accepted"), "{g}");
}

#[test]
fn the_line_changes_when_the_kinds_doc_does() {
    let c = load("changed", CALL, &KIND.replace("a change made directly", "one agent, no design"));
    assert!(gives(&c, "take").contains("  Direct  one agent, no design\n"));
}

#[test]
fn a_kind_not_there_is_refused_at_load() {
    let c = load("refused", CALL, &KIND.replace("kind direct", "kind straight"));
    assert!(c.defs.contains_key("pick"));
    assert!(!c.defs.contains_key("take"));
    let e = c.errors.join("\n");
    assert!(e.contains("t.call:13: ") && e.contains("type Direct: kind direct: no such kind; the call take is left out"), "{e}");
    let c = load("refused-inline", &CALL.replace("| Change(words: Text)    /// kind change", "| Change(words: Text)    /// kind chnage"), KIND);
    assert!(!c.defs.contains_key("pick") && c.defs.contains_key("take"));
    assert!(c.errors.join("\n").contains("kind chnage: no such kind; the call pick is left out"), "{:?}", c.errors);
}
