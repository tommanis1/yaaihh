//! Which calls an agent named in a step's answer is given later (paper-research N-101 p977, p980), read off the
//! checked kinds: the designer named in `assign`'s answer is given `design`; each agent of `assign_parts`' picks,
//! through `iterate`, `find`, a `par` over the parts and two `open`s into a form, `plan_here`, `integrate` and
//! `implement`; an agent read from a ticket at run time (`step by …`) is not followed. The step's gives says it under
//! the agent's field, each call by its doc line and its class.

use crate::check::CallSig;
use crate::machine::Defs;
use crate::syntax::{self, Named};
use std::collections::BTreeMap;

const CALLS: &str = "\
/// the agent chosen to write the piece
type Assigned {
  agent: Agent    /// by its name: an existing one as it is, or the new agent or copy made for the piece
  why: Text
}

type Placed {
  piece: Text
  agent: Agent    /// by its name
}

type Picked {
  picks: [Placed]
}

/// who writes the design
call assign(words: Text) where model >= large -> Assigned | Ask(question: Text)
  template
    A.

/// who plans and builds each part
call assign_parts(words: Text) where model >= large -> Picked | Ask(question: Text)
  template
    A.

/// the design for the owner's words
call design(words: Text) where model >= large -> Design(parts: [Text])
  template
    D.

/// the part's worker's plan
call plan_here(part: Text) where model >= large -> Plan(text: Text)
  template
    P.

/// the two plans made one
call integrate(part: Text) where model >= large -> Plan(text: Text)
  template
    I.

/// the plan implemented
call implement(plan: Text) where model >= medium -> Commit(c: Text) | Cannot(why: Text)
  template
    M.

/// whose red it is
call culprit(c: Text) -> Piece(by: Agent)
  template
    C.

/// a red mended
call mend(c: Text) where model >= medium -> Commit(c: Text)
  template
    M.
";

const KINDS: &str = "\
kind change
  form words: Text
  result [Text]
  raises Exhausted, NoOrder, Stalled
  pattern
    w <- step lead assign(words);
    match w {
      Assigned(designer, _) =>
        d <- step designer design(words);
        match d {
          Design(parts) =>
            whos <- iterate came_back = none up to 3 {
              t <- step lead assign_parts(words);
              match t {
                Picked(picks) => return Done(Picked(picks))
                Ask(q) => return Again(q)
              }
            };
            tops <- par part in parts up to 8 as part after [] {
              pick <- return find q in whos.picks where q.piece == part;
              r <- open planned(part, pick.agent);
              return r
            };
            return tops
        }
      Ask(q) => return []
    }

kind planned
  form part: Text, worker: Agent
  result Text
  pattern
    p <- step worker plan_here(part);
    q <- step worker integrate(part);
    match q { Plan(text) => b <- open built(worker, text); return b }

kind built
  form implementer: Agent, plan: Text
  result Text
  pattern
    c <- step implementer implement(plan);
    match c {
      Commit(x) => return x
      Cannot(why) =>
        k <- step lead culprit(why);
        match k { Piece(by) => m <- step by mend(why); match m { Commit(y) => return y } }
    }

kind handed
  form log: Text
  result Text
  pattern
    a <- step lead assign(log);
    match a {
      Assigned(who, _) => m <- step who mend(log); match m { Commit(y) => return y }
      Ask(q) => return q
    }
";

fn defs() -> Defs {
    let mut named = Named::new();
    let files = vec![("t.call".to_string(), CALLS.to_string())];
    let (cs, errors) = syntax::parse_call_files(&files, &mut named);
    assert!(errors.is_empty(), "{errors:?}");
    let sigs: BTreeMap<String, CallSig> = cs.iter().map(|(n, c)| (n.clone(), c.sig.clone())).collect();
    let (mut d, errors) = Defs::build(&[("t.kind".to_string(), KINDS.to_string())], sigs, named, 200);
    assert!(errors.is_empty(), "{errors:?}");
    d.call_docs = cs.iter().filter_map(|(n, c)| c.doc.text.lines().next().map(|l| (n.clone(), l.to_string()))).collect();
    d
}

fn given(d: &Defs, kind: &str, call: &str, place: &str) -> Vec<String> {
    d.agents_given.get(&(kind.to_string(), call.to_string())).and_then(|p| p.get(place)).cloned().unwrap_or_default()
}

#[test]
fn the_agent_named_in_an_answer_is_given_the_calls_the_program_gives_it() {
    let d = defs();
    assert_eq!(given(&d, "change", "assign", "Assigned.agent"), vec!["design"]);
    // through the iterate's Done, `whos.picks`, the `find` of the part's pick, and `open planned` then `open built`
    assert_eq!(given(&d, "change", "assign_parts", "Picked.picks[].agent"), vec!["plan_here", "integrate", "implement"]);
}

#[test]
fn an_agent_read_from_an_answer_at_run_time_is_not_followed() {
    let d = defs();
    // `step by mend(...)`: `by` is the culprit's answer, and that is all that is said of it; the picks are not given mend
    assert_eq!(given(&d, "built", "culprit", "Piece.by"), vec!["mend"]);
    assert!(!given(&d, "change", "assign_parts", "Picked.picks[].agent").contains(&"mend".to_string()));
    // a step to a role or a standing agent names nobody in an answer
    assert!(d.agents_given.values().all(|p| p.keys().all(|k| !k.is_empty())));
}

#[test]
fn the_steps_gives_says_it_under_the_agents_field() {
    let d = defs();
    let files = vec![("t.call".to_string(), CALLS.to_string())];
    let mut named = Named::new();
    let (cs, _) = syntax::parse_call_files(&files, &mut named);
    let types = syntax::type_docs(&files);
    let show = |call: &str| {
        let lines = crate::doc::given_lines("change", call, &d.agents_given, &d.calls, &d.call_docs);
        crate::doc::gives_with(&cs[call].sig.head, Some(&cs[call].doc), &types, &named, &lines)
    };
    assert_eq!(show("assign"), "\
This step gives one of:
  Assigned  the agent chosen to write the piece
    agent: Agent  by its name: an existing one as it is, or the new agent or copy made for the piece
                  it is given: the design for the owner's words (large)
    why
  Ask
    question");
    assert_eq!(show("assign_parts"), "\
This step gives one of:
  Picked
    picks: [Placed]
      piece
      agent: Agent  by its name
                    it is given: the part's worker's plan (large), the two plans made one (large), the plan implemented (medium)
  Ask
    question");
    // nothing is written where the map has no entry
    assert!(!show("design").contains("it is given"));
}

#[test]
fn one_call_in_two_kinds_gives_each_step_its_own() {
    let d = defs();
    // `assign` in `change` leads to the design, in `handed` to the mend (Tovrin's handed_up, the stand-in's C-1)
    assert_eq!(given(&d, "change", "assign", "Assigned.agent"), vec!["design"]);
    assert_eq!(given(&d, "handed", "assign", "Assigned.agent"), vec!["mend"]);
}
