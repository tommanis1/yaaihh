//! Parts by dependency (N-101): `par x in e up to n as K after A [reach KIND] { p }`.

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const CALLS: &str = "type Part {
  name: Text
  stands_on: [Text]
}

call build(part: Text) -> Built(top: Text)
  task build
  template
    Build {{ part }}.

call accept(part: Text, top: Text) -> Accepted | Not(why: Text)
  task accept
  template
    Accept {{ part }} at {{ top }}?
";

/// A design's parts, each built (`built`), then accepted by the owner; REACH and RAISES as written in the par.
fn kinds(reach: &str, raises: &str) -> String {
    format!(
        "kind change
  form parts: [Part]
  result [Text]
  raises {raises}
  pattern
    tops <- par p in parts up to 8 as p.name after p.stands_on{reach} {{
      b <- open built(p.name);
      a <- step owner accept(p.name, b);
      match a {{
        Accepted => return b
        Not(why) => raise DesignCannotStand(why)
      }}
    }};
    return tops

kind built
  form part: Text
  result Text
  pattern
    v <- step fresh build(part);
    match v {{
      Built(top) => return top
    }}
"
    )
}

fn defs_of(kinds: &str) -> (Defs, Vec<String>) {
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = syntax::parse_calls("parts.call", CALLS, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
    Defs::build(&[("parts.kind".to_string(), kinds.to_string())], calls, named, 200)
}

struct Run {
    m: Machine,
    kinds: String,
    log: Vec<Line>,
    clock: u32,
}

impl Run {
    fn new(kinds: String) -> Run {
        let (d, errors) = defs_of(&kinds);
        assert!(errors.is_empty(), "{errors:?}");
        Run { m: Machine::load(d), kinds, log: Vec::new(), clock: 0 }
    }
    fn go(&mut self, i: Input) -> Out {
        self.clock += 1;
        let out = self.m.apply(i, format!("2026-11-15T20:00:{:02}Z", self.clock));
        self.log.extend(out.lines.iter().cloned());
        out
    }
    /// The change opened on PARTS: (name, stands on).
    fn open(&mut self, parts: &[(&str, &[&str])]) -> Out {
        let parts: Vec<Value> = parts.iter().map(|(n, on)| json!({"name": n, "stands_on": on})).collect();
        let out = self.go(Input::Open { kind: "change".into(), under: None, opener: "owner".into(), form: json!({"parts": parts}), text: None, names: vec![], told: vec![] });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        out
    }
    fn status(&self, t: u64) -> ViewStatus {
        self.m.view(t).unwrap().status
    }
    /// The parts of the change, in the order of its list.
    fn parts(&self) -> Vec<u64> {
        self.m.tickets[&200].children.clone()
    }
    /// The `built` under part P, if it has been opened.
    fn built(&self, p: u64) -> Option<u64> {
        self.m.tickets[&p].children.iter().copied().find(|c| self.m.tickets[c].kind == "built")
    }
    /// Part P's builder answers with TOP.
    fn build(&mut self, p: u64, top: &str) {
        let b = self.built(p).expect("its built is open");
        let Status::Awaits { agent, .. } = &self.m.tickets[&b].status else { panic!("{:?}", self.m.tickets[&b].status) };
        let agent = agent.clone();
        let out = self.go(Input::Answer { t: b, by: agent, head: json!({"head": "Built", "top": top}), text: Text::New("built".into()) });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
    }
    /// The owner answers part P's accept with HEAD.
    fn accept(&mut self, p: u64, head: Value) {
        let out = self.go(Input::Answer { t: p, by: "owner".into(), head, text: Text::New("seen".into()) });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
    }
    fn holds(&self, p: u64) -> Option<Vec<String>> {
        match self.status(p) {
            ViewStatus::Holds { on, .. } => Some(on),
            _ => None,
        }
    }
    fn replays(&self) {
        let text: Vec<String> = self.log.iter().map(|l| serde_json::to_string(l).unwrap()).collect();
        let lines: Vec<Line> = text.iter().map(|s| serde_json::from_str(s).unwrap()).collect();
        let mut m2 = Machine::load(defs_of(&self.kinds).0);
        assert_eq!(m2.replay(&lines), Vec::<String>::new());
        for t in self.m.tickets.keys() {
            assert_eq!(m2.view(*t), self.m.view(*t), "ticket {t}");
        }
    }
}

#[test]
fn parts_start_when_the_parts_they_stand_on_reach_the_state() {
    let mut r = Run::new(kinds(" reach built", "Exhausted, NoOrder, DesignCannotStand"));
    // a diamond: b and c stand on a, d on b and c
    let out = r.open(&[("a", &[]), ("b", &["a"]), ("c", &["a"]), ("d", &["b", "c"])]);
    let ps = r.parts();
    let [a, b, c, d] = ps[..] else { panic!("{ps:?}") };
    assert!(out.lines.contains(&Line::Holds { at: "2026-11-15T20:00:01Z".into(), t: b, on: vec![a], reach: Some("built".into()) }), "{:?}", out.lines);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Holds { t, on, .. } if *t == d && *on == vec![b, c])));
    assert!(r.built(a).is_some(), "a stands on nothing: it runs at once");
    assert_eq!((r.built(b), r.built(c), r.built(d)), (None, None, None));
    assert_eq!(r.holds(d), Some(vec![r.m.id(b), r.m.id(c)]));
    assert!(matches!(r.status(200), ViewStatus::Waits { .. }));
    // a's built closes: b and c start, though a's part still waits on the owner
    r.build(a, "t-a");
    assert!(r.log.iter().any(|l| matches!(l, Line::Freed { t, .. } if *t == b)));
    assert!(r.built(b).is_some() && r.built(c).is_some() && r.built(d).is_none());
    assert!(matches!(r.status(a), ViewStatus::Awaits { ref agent, .. } if agent == "owner"));
    // b's alone does not start d
    r.build(b, "t-b");
    assert!(r.built(d).is_none() && r.holds(d).is_some());
    r.build(c, "t-c");
    assert!(r.built(d).is_some() && r.holds(d).is_none());
    r.build(d, "t-d");
    for p in [d, b, a, c] {
        r.accept(p, json!({"head": "Accepted"}));
    }
    // the list's order, not the order they ended in
    assert_eq!(r.status(200), ViewStatus::Done { result: json!(["t-a", "t-b", "t-c", "t-d"]) });
    r.replays();
}

#[test]
fn without_reach_a_part_waits_for_the_parts_below_to_close() {
    let mut r = Run::new(kinds("", "Exhausted, NoOrder, DesignCannotStand"));
    r.open(&[("a", &[]), ("b", &["a"])]);
    let [a, b] = r.parts()[..] else { panic!() };
    assert_eq!(r.m.view(b).unwrap().status, ViewStatus::Holds { on: vec![r.m.id(a)], reach: None });
    r.build(a, "t-a");
    assert!(r.built(b).is_none(), "a has not closed: the owner has not accepted it");
    r.accept(a, json!({"head": "Accepted"}));
    assert!(r.built(b).is_some());
    r.build(b, "t-b");
    r.accept(b, json!({"head": "Accepted"}));
    assert_eq!(r.status(200), ViewStatus::Done { result: json!(["t-a", "t-b"]) });
    r.replays();
}

#[test]
fn a_part_that_fails_leaves_those_on_it_unstarted_and_its_failure_is_raised() {
    let mut r = Run::new(kinds("", "Exhausted, NoOrder, DesignCannotStand"));
    // written first, the parts that stand on the one that fails: their cancels are not what the par raises
    r.open(&[("b", &["a"]), ("c", &["b"]), ("a", &[]), ("e", &[])]);
    let [b, c, a, e] = r.parts()[..] else { panic!() };
    r.build(a, "t-a");
    r.build(e, "t-e");
    r.accept(a, json!({"head": "Not", "why": "the design is wrong"}));
    // b never starts, nor c, which stands on b
    for (p, below) in [(b, a), (c, b)] {
        let ViewStatus::Failed { raised } = r.status(p) else { panic!("{:?}", r.status(p)) };
        assert_eq!((raised.exc.as_str(), &raised.value["why"]), ("Cancelled", &json!(format!("stands on {}, which failed", r.m.id(below)))));
        assert!(r.built(p).is_none());
    }
    assert!(matches!(r.status(200), ViewStatus::Waits { .. }), "e is still open");
    r.accept(e, json!({"head": "Accepted"}));
    // the first failure in the order written that is not such a cancel: a's
    let ViewStatus::Failed { raised } = r.status(200) else { panic!("{:?}", r.status(200)) };
    assert_eq!((raised.exc.as_str(), &raised.value), ("DesignCannotStand", &json!("the design is wrong")));
    r.replays();
}

#[test]
fn a_part_started_goes_on_when_the_part_below_fails_later() {
    let mut r = Run::new(kinds(" reach built", "Exhausted, NoOrder, DesignCannotStand"));
    r.open(&[("a", &[]), ("b", &["a"])]);
    let [a, b] = r.parts()[..] else { panic!() };
    r.build(a, "t-a");
    r.accept(a, json!({"head": "Not", "why": "the design is wrong"}));
    let bb = r.built(b).unwrap();
    assert_eq!(r.status(b), ViewStatus::Waits { children: vec![r.m.id(bb)] });
    r.build(b, "t-b");
    r.accept(b, json!({"head": "Accepted"}));
    assert!(matches!(r.status(200), ViewStatus::Failed { ref raised } if raised.exc == "DesignCannotStand"));
    r.replays();
}

#[test]
fn names_twice_a_name_of_no_part_or_a_circle_open_nothing_and_raise_no_order() {
    for (parts, why) in [
        (vec![("a", vec!["b"]), ("b", vec!["c"]), ("c", vec!["a"])], "the parts stand on each other in a circle: \"a\" on \"b\" on \"c\" on \"a\""),
        (vec![("a", vec!["a"])], "\"a\" stands on itself"),
        (vec![("a", vec![]), ("b", vec!["x"])], "\"b\" stands on \"x\", which is no part"),
        (vec![("a", vec![]), ("a", vec![])], "two parts are named \"a\""),
    ] {
        let mut r = Run::new(kinds(" reach built", "Exhausted, NoOrder, DesignCannotStand"));
        let ps: Vec<(&str, &[&str])> = parts.iter().map(|(n, on)| (*n, on.as_slice())).collect();
        r.open(&ps);
        assert!(r.parts().is_empty(), "nothing is opened");
        assert_eq!(r.status(200), ViewStatus::Failed { raised: Raised { exc: "NoOrder".into(), value: json!({"head": "NoOrder", "why": why}) } });
        r.replays();
    }
}

#[test]
fn the_checker_on_a_par_in_order() {
    // NoOrder must be declared or handled, as Exhausted is
    let (_, errors) = defs_of(&kinds(" reach built", "Exhausted, DesignCannotStand"));
    assert!(errors.iter().any(|e| e.contains("NoOrder")), "{errors:?}");
    // the state must be a kind
    let (_, errors) = defs_of(&kinds(" reach piece", "Exhausted, NoOrder, DesignCannotStand"));
    assert!(errors.iter().any(|e| e.contains("no kind piece for `reach`")), "{errors:?}");
    // the name is a Text and what it stands on a [Text]
    let k = kinds("", "Exhausted, NoOrder, DesignCannotStand").replace("as p.name after p.stands_on", "as p.stands_on after p.name");
    let (_, errors) = defs_of(&k);
    assert!(errors.iter().any(|e| e.contains("the name after `as`")), "{errors:?}");
    // an any has no order
    let p = "xs <- any 1 of p in parts up to 8 as p.name after p.stands_on {\n  return p.name\n};\nreturn xs";
    assert!(syntax::parse_prog(p, 1).is_err_and(|e| e.contains("an any has no order")));
    // it prints as it parses
    let p = "tops <- par p in parts up to 8 as p.name after p.stands_on reach built {\n  b <- open built(p.name);\n  return b\n};\nreturn tops";
    let once = print::prog(&syntax::parse_prog(p, 1).unwrap(), 0);
    assert!(once.contains("par p in parts up to 8 as p.name after p.stands_on reach built {"), "{once}");
    assert_eq!(print::prog(&syntax::parse_prog(&once, 1).unwrap(), 0), once);
}

/// What a decision is shown of a part (N-101 p158-p166, `decide_facts`): the minutes since the first ticket of the
/// asking kind under its parent, and how often it was decided each way before (none yet).
#[test]
fn a_decision_on_a_part_is_shown_the_minutes_since_the_first_of_its_kind() {
    let mut r = Run::new(kinds(" reach built", "Exhausted, NoOrder, DesignCannotStand"));
    let mut named = BTreeMap::new();
    let extra: Vec<CallSig> = syntax::parse_calls("d.call", "decision fix-or-out(piece: Ticket, why: Text, carries: [Ticket]) -> Fix | Out\n", &mut named).unwrap();
    for c in extra {
        r.m.defs.edit().calls.insert(c.name.clone(), c);
    }
    r.open(&[("a", &[]), ("b", &["a"])]);
    let [a, _] = r.parts()[..] else { panic!() };
    let built = r.built(a).unwrap();
    let args = json!({"piece": r.m.id(built), "why": "red", "carries": [r.m.id(built), r.m.id(a)]});
    let f = r.m.decide_facts(built, "fix-or-out", &args, "2026-11-15T23:20:01Z");
    assert_eq!(f, json!({"carries-count": 2, "earlier-Fix": 0, "earlier-Out": 0, "first-minutes": 200}).as_object().unwrap().clone(), "{f:?}");
}
