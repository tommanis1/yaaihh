//! The minder's language work (paper-research N-101 p722): a type with alternatives over several lines, each with its
//! own `///`, and more than one `when` on a `during`, each with its own `where`, an event that fits two given to the
//! first written. The template's `{{ match }}` is coordd's (coordd/src/tests/moment.rs).

use crate::*;
use serde_json::json;
use std::collections::BTreeMap;

/// The design's type (tickets-patterns plan/minder/moment/mind.call).
const MOMENT: &str = "/// the point in a piece's work at which its minder looks
type Moment =
  | Dispatch                           /// before the implementer is started
  | Check(ran: Text)                   /// what the implementer ran and what came back
  | Asking(question: Text)             /// the implementer's question, before it reaches the owner
  | HandIn(report: Text, commit: Ref)  /// the implementer's answer as it wrote it, and the commit about to go on the line
";

#[test]
fn a_type_over_several_lines_has_its_alternatives_each_with_its_doc() {
    let mut named = BTreeMap::new();
    syntax::parse_types("mind.call", MOMENT, &mut named).unwrap();
    assert_eq!(
        named["Moment"],
        Type::Variant(vec![
            ("Dispatch".into(), vec![]),
            ("Check".into(), vec![("ran".into(), Type::Text)]),
            ("Asking".into(), vec![("question".into(), Type::Text)]),
            ("HandIn".into(), vec![("report".into(), Type::Text), ("commit".into(), Type::Ref)]),
        ])
    );
    let docs = syntax::type_docs(&[("mind.call".into(), MOMENT.into())]);
    assert_eq!(docs["Moment"].text, "the point in a piece's work at which its minder looks");
    assert_eq!(docs["Moment"].parts["Dispatch"], "before the implementer is started");
    assert_eq!(docs["Moment"].parts["HandIn"], "the implementer's answer as it wrote it, and the commit about to go on the line");
    // the one-line form still reads, and a call after the type reads
    let one = "type Moment = Dispatch | Check(ran: Text)\n";
    let mut n2 = BTreeMap::new();
    syntax::parse_types("one.call", one, &mut n2).unwrap();
    assert_eq!(n2["Moment"], Type::Variant(vec![("Dispatch".into(), vec![]), ("Check".into(), vec![("ran".into(), Type::Text)])]));
    let with_call = format!("{MOMENT}\ncall mind(moment: Moment) -> Rest\n  task mind\n  template\n    x\n");
    let mut n3 = BTreeMap::new();
    let calls = syntax::parse_calls("mind.call", &with_call, &mut n3).unwrap();
    assert_eq!(calls[0].params, vec![("moment".to_string(), n3["Moment"].clone())]);
    // the type's place is its `type` line
    assert_eq!(syntax::type_places("mind.call", MOMENT).unwrap(), vec![("Moment".to_string(), 2)]);
}

#[test]
fn a_type_over_several_lines_is_refused_where_it_is_wrong() {
    let refused = |text: &str| {
        let mut named = BTreeMap::new();
        syntax::parse_types("m.call", text, &mut named).unwrap_err()
    };
    // a doc on a line of two alternatives stands on neither
    let e = refused("type Moment =\n  | Dispatch | Check(ran: Text)  /// two\n");
    assert!(e.contains("m.call:2:") && e.contains("Dispatch, Check"), "{e}");
    // an alternative that is not a constructor
    let e = refused("type Moment =\n  | Dispatch\n  | [Text]\n");
    assert!(e.contains("a constructor after `|`"), "{e}");
    // `=` with nothing after it and no alternative under it
    let e = refused("type Moment =\n\ncall x() -> Rest\n");
    assert_eq!(e, "m.call:1: `type Moment =` has no type after it, nor an alternative a line under it starting with `|`");
}

#[test]
fn what_a_step_gives_and_a_types_parts_show_the_alternatives_with_their_docs() {
    let text = format!("{MOMENT}\ntype Look {{\n  moment: Moment   /// where the look was made\n  said: Text\n}}\n");
    let mut named = BTreeMap::new();
    syntax::parse_types("mind.call", &text, &mut named).unwrap();
    let docs = syntax::type_docs(&[("mind.call".into(), text.clone())]);
    assert_eq!(
        doc::type_parts("Moment", &named["Moment"], docs.get("Moment"), &docs, &named),
        "  Dispatch  before the implementer is started
  Check     what the implementer ran and what came back
    ran
  Asking    the implementer's question, before it reaches the owner
    question
  HandIn    the implementer's answer as it wrote it, and the commit about to go on the line
    report
    commit: Ref"
    );
    // a step whose answer carries a Moment: the alternatives under the field, as any declared type's parts
    let head = Type::Variant(vec![("Looked".into(), vec![("look".into(), named["Look"].clone())])]);
    assert_eq!(
        doc::gives(&head, None, &docs, &named),
        "This step gives:
  Looked
    look: Look
      moment: Moment  where the look was made
        Dispatch  before the implementer is started
        Check     what the implementer ran and what came back
          ran
        Asking    the implementer's question, before it reaches the owner
          question
        HandIn    the implementer's answer as it wrote it, and the commit about to go on the line
          report
          commit: Ref
      said"
    );
}

const CALLS: &str = "call design(words: Text) -> Design(text: Text) | Ask(question: Text)
  task design
  asks Ask
  template
    Design {{ words }}.
";

/// Tovrin's piece, cut down (tickets-patterns plan/tovrin-program/on-library/tovrin.kind at cd66a2c): one `during`
/// with a `when` for a check and one for a question; and a third that takes every act of the designer, after them.
const KINDS: &str = "kind change
  form words: Text
  result Text
  pattern
    let agent d in {
      let agent m in {
        r <- during {
          step d design(words)
        } when act(u) where u.by == d and u.is == \"check\" do {
          k <- step m \"Check: {{ u.text }}\" -> Rest | Say(text: Text);
          match k {
            Rest => return none
            Say(text) => stop d; post text naming [d]
          }
        } when act(u) where u.by == d and u.is == \"ask\" do {
          k <- step m \"Asking: {{ u.text }}\" -> Rest | Say(text: Text);
          match k {
            Rest => return none
            Say(text) => stop d; post text naming [d]
          }
        } when act(u) where u.by == d do {
          k <- step m \"Any: {{ u.text }}\" -> Rest;
          return none
        };
        match r {
          Design(text) => return text
        }
      }
    }
";

fn defs() -> Defs {
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = syntax::parse_calls("design.call", CALLS, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
    let (d, errors) = Defs::build(&[("change.kind".to_string(), KINDS.to_string())], calls, named, 200);
    assert!(errors.is_empty(), "{errors:?}");
    d
}

struct Run {
    m: Machine,
    log: Vec<Line>,
    clock: u32,
}

impl Run {
    fn go(&mut self, i: Input) -> Out {
        self.clock += 1;
        let out = self.m.apply(i, format!("2026-11-16T12:00:{:02}Z", self.clock));
        self.log.extend(out.lines.iter().cloned());
        out
    }
    fn designer(&self) -> String {
        match &self.m.tickets[&200].status {
            Status::Awaits { agent, .. } => agent.clone(),
            s => panic!("{s:?}"),
        }
    }
    fn give(&mut self, agent: &str) {
        for d in self.m.due(agent) {
            self.go(Input::Given { t: d.t, to: agent.into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(Loc { file: "t.jsonl".into(), offset: 1, len: 1 }) });
        }
    }
    fn act(&mut self, by: &str, is: &str, text: &str) -> Out {
        self.go(Input::Event { name: "act".into(), value: json!({"ticket": "C-200", "by": by, "is": is, "text": text}) })
    }
    /// The open look: its ticket, the minder its step is owed to, and the program it runs (its `opened` line's).
    fn look(&self) -> (u64, String, String) {
        let (t, x) = self.m.tickets.range(HIDDEN..).rev().find(|(_, x)| x.is_open() && x.handling.as_deref() == Some("act")).expect("a look runs");
        let program = self.log.iter().find_map(|l| if let Line::Opened { t: c, program, .. } = l { (c == t).then(|| program.clone()) } else { None }).unwrap();
        match &x.status {
            Status::Awaits { agent, .. } => (*t, agent.clone(), program),
            s => panic!("{s:?}"),
        }
    }
    fn rest(&mut self, look: u64, m: &str) {
        self.give(m);
        self.go(Input::Answer { t: look, by: m.into(), head: json!({"head": "Rest"}), text: Text::New("as said".into()) }).reply.unwrap();
    }
}

#[test]
fn an_event_that_fits_two_whens_on_a_during_is_given_to_the_first_written() {
    let mut r = Run { m: Machine::load(defs()), log: Vec::new(), clock: 0 };
    r.go(Input::Open { kind: "change".into(), under: None, opener: "owner".into(), form: json!({"words": "drop short buffers"}), text: None, names: vec![], told: vec![] }).reply.unwrap();
    let d = r.designer();
    r.give(&d);
    // a check fits the first and the third: the first takes it
    let out = r.act(&d, "check", "cargo test: 3 failed");
    assert!(out.reply.as_ref().unwrap().starts_with("Looked at on C-200"), "{:?}", out.reply);
    let (look, m, program) = r.look();
    assert!(program.contains("\"Check: {{ u.text }}\""), "{program}");
    r.rest(look, &m);
    // an edit fits only the third
    r.act(&d, "edit", "src/lib.rs").reply.unwrap();
    let (look, m, program) = r.look();
    assert!(program.contains("\"Any: {{ u.text }}\""), "{program}");
    r.rest(look, &m);
    // a question goes to the second: the look holds it before the owner has it
    r.go(Input::Answer { t: 200, by: d.clone(), head: json!({"head": "Ask", "question": "drop or report?"}), text: Text::New("as said".into()) }).reply.unwrap();
    let (look, _, program) = r.look();
    assert!(program.contains("\"Asking: {{ u.text }}\""), "{program}");
    assert_eq!(r.m.tickets[&look].held_ask.as_ref().map(|q| q.ask.text.as_str()), Some("drop or report?"));
    // the whole record replays as it ran
    let mut m2 = Machine::load(defs());
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    for t in r.m.tickets.keys() {
        assert_eq!(m2.view(*t), r.m.view(*t), "ticket {t}");
    }
    // printed, the three `when`s read back as they are
    let p = &r.m.defs.kinds["change"].pattern;
    let again = syntax::parse_prog(&print::prog(p, 0), 1).unwrap();
    assert_eq!(print::prog(&again, 0), print::prog(p, 0));
    assert_eq!(print::prog(p, 0).matches("} when act(u)").count(), 3);
}

#[test]
fn each_when_on_a_during_is_checked_with_its_own_line() {
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = syntax::parse_calls("design.call", CALLS, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
    let bad = KINDS.replace("} when act(u) where u.by == d do {", "} when act(u) where u.nothing do {");
    let (_, errors) = Defs::build(&[("change.kind".to_string(), bad)], calls, named, 200);
    let line = KINDS.lines().position(|l| l.contains("} when act(u) where u.by == d do {")).unwrap() + 1;
    assert!(errors.iter().any(|e| e.contains(&format!("line {line}:"))), "{errors:?}");
}

/// A `Ref` in an inline step's text and in a post's reads as an answer writes it, as in a call's template (paper-
/// research N-101 p850, p880): `a 02e0410, b 1a2b3c4`, one commit as it is.
#[test]
fn a_ref_in_an_inline_step_or_a_post_reads_as_an_answer_writes_it() {
    let kind = "kind land
  form c: Ref, cs: [Ref]
  result Text
  pattern
    post \"landed {{ c }} after {{ cs }}\";
    k <- step owner \"Land {{ c }}?\" -> Yes;
    return \"done\"
";
    let (d, errors) = Defs::build(&[("land.kind".to_string(), kind.to_string())], BTreeMap::new(), BTreeMap::new(), 200);
    assert!(errors.is_empty(), "{errors:?}");
    let mut m = Machine::load(d);
    let out = m.apply(Input::Open { kind: "land".into(), under: None, opener: "owner".into(), form: json!({"c": {"a": "02e0410", "b": "1a2b3c4"}, "cs": ["9f8e7d6", "5e9916d"]}), text: None, names: vec![], told: vec![] }, "2026-11-16T18:00:00Z".into());
    out.reply.unwrap();
    assert_eq!(m.view(200).unwrap().posts[0].text, Text::New("landed a 02e0410, b 1a2b3c4 after 9f8e7d6, 5e9916d".into()));
    let d = m.due("owner").into_iter().find(|d| d.t == 200).expect("the step");
    let What::Step { template, .. } = &d.what else { panic!("{:?}", d.what) };
    assert_eq!(template.as_deref(), Some("Land a 02e0410, b 1a2b3c4?"));
}

/// A value that has no one way to be written is refused where a text names it whole: a sum (choose by `match`, or
/// name a field), a tuple, a value of no known type (paper-research N-101 p880).
#[test]
fn a_text_naming_a_sum_whole_is_refused_with_why() {
    let build = |pattern: &str| {
        let kind = format!("kind k\n  form task: Text\n  result Text\n  pattern\n{pattern}");
        Defs::build(&[("k.kind".to_string(), kind)], BTreeMap::new(), BTreeMap::new(), 200).1
    };
    let e = build("    r <- step owner \"Do {{ task }}\" -> Done(c: Text) | Not(why: Text);\n    post \"it gave {{ r }}\";\n    return task\n");
    assert!(e.iter().any(|e| e.contains("the post's text names `{{ r }}`: a value with alternatives is not written whole")), "{e:?}");
    let e = build("    r <- step owner \"Do {{ task }}\" -> Done(c: Text) | Not(why: Text);\n    k <- step owner \"Is {{ r }} right?\" -> Yes;\n    return task\n");
    assert!(e.iter().any(|e| e.contains("the step's text names `{{ r }}`: a value with alternatives is not written whole")), "{e:?}");
    // one of its fields, or a value of one alternative, reads
    let e = build("    r <- step owner \"Do {{ task }}\" -> Done(c: Text);\n    post \"it gave {{ r.c }}\";\n    return task\n");
    assert!(e.is_empty(), "{e:?}");
}
