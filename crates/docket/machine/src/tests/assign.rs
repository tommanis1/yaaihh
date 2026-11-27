//! What the orchestrator's `assign` uses of the language (item 11, the owner on N-199): the word `agents` (G1); an
//! answer whose Agent coordd finds not registered refused (G5, `Unfit`); a step addressed to the Agent an answer
//! names; `find` and a field of each element of a list (G6, G9). Which picks are allowed is the project's policy,
//! not the engine's: nothing here knows a call named `assign`.

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const CALLS: &str = "type Placed {
  piece: Text
  agent: Agent
  why: Text
}

call assign(words: Text, what: Text, pieces: [Text], agents: Agents) -> Assigned(agent: Agent, why: Text) | Ask(question: Text)
  template
    {{ words }}: who takes {{ what }}? {{ agents }}

call place(words: Text, what: Text, pieces: [Text], agents: Agents) -> Placed(picks: [Placed])
  template
    {{ words }}: who takes each of {{ what }}? {{ agents }}

call design(words: Text) -> Designed(text: Text)
  template
    Design {{ words }}.

call build(part: Text) -> Built(top: Text)
  template
    Build {{ part }}.
";

const KINDS: &str = "kind change
  letter C
  form words: Text, parts: [Text]
  result [Text]
  raises Exhausted, NoOrder
  pattern
    w <- step lead assign(words, \"the design\", [\"the design\"], agents);
    match w {
      Assigned(a, why) =>
        d <- step a design(words);
        p <- step lead place(words, \"its parts\", parts, agents);
        match p {
          Placed(picks) =>
            tops <- par x in parts up to 8 as x after [] {
              q <- return find q in picks where q.piece == x;
              b <- step q.agent build(x);
              match b { Built(top) => return top }
            };
            return tops
        }
      Ask(question) => return []
    }
";

fn defs(kinds: &str) -> (Defs, Vec<String>) {
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = syntax::parse_calls("assign.call", CALLS, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
    Defs::build(&[("assign.kind".to_string(), kinds.to_string())], calls, named, 200)
}

fn machine() -> Machine {
    let (d, errors) = defs(KINDS);
    assert!(errors.is_empty(), "{errors:#?}");
    Machine::load(d)
}

struct Run {
    m: Machine,
    log: Vec<Line>,
    t: u64,
    clock: u32,
}

impl Run {
    fn new() -> Run {
        let mut r = Run { m: machine(), log: Vec::new(), t: 0, clock: 0 };
        let out = r.go(Input::Open { kind: "change".into(), under: None, opener: "owner".into(), form: json!({"words": "a cache", "parts": ["a", "b"]}), text: None, names: vec![], told: vec![] });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        r.t = *r.m.tickets.keys().max().unwrap();
        r
    }
    fn go(&mut self, i: Input) -> Out {
        self.clock += 1;
        let out = self.m.apply(i, format!("2026-11-16T10:{:02}:{:02}Z", self.clock / 60, self.clock % 60));
        self.log.extend(out.lines.clone());
        out
    }
    fn awaits(&self, t: u64) -> (String, String, Value) {
        let Status::Awaits { agent, owed, .. } = &self.m.tickets[&t].status else { panic!("{:?}", self.m.tickets[&t].status) };
        (agent.clone(), owed.call.clone(), owed.args.clone())
    }
    fn answer(&mut self, t: u64, by: &str, head: Value) -> Result<String, String> {
        self.go(Input::Answer { t, by: by.into(), head, text: Text::New("done".into()) }).reply
    }
    fn refusals(&self) -> u32 {
        let Status::Awaits { refusals, .. } = &self.m.tickets[&self.t].status else { panic!("{:?}", self.m.tickets[&self.t].status) };
        *refusals
    }
}

fn placed(piece: &str, agent: &str) -> Value {
    json!({"piece": piece, "agent": agent, "why": "it fits"})
}

#[test]
fn assign_is_given_the_word_agents_as_a_marker() {
    let r = Run::new();
    let (agent, call, args) = r.awaits(r.t);
    assert_eq!((agent.as_str(), call.as_str()), ("lead", "assign"));
    assert_eq!(args["agents"], types::agents_marker());
    assert!(types::is_agents(&args["agents"]));
    assert_eq!(args["pieces"], json!(["the design"]));
    // the record has the marker, never a list of agents
    assert!(r.log.iter().any(|l| matches!(l, Line::StepArgs { args, .. } if args["agents"] == json!({"agents": "project"}))));
    // a binding of the name hides the word
    let (_, errors) = defs(&KINDS.replace("form words: Text, parts: [Text]", "form words: Text, parts: [Text], agents: Text"));
    assert!(errors.iter().any(|e| e.contains("Agents")), "{errors:#?}");
}

#[test]
fn the_agents_an_answer_names_are_given_the_steps_and_each_part_finds_its_pick() {
    let mut r = Run::new();
    let t = r.t;
    r.answer(t, "lead", json!({"head": "Assigned", "agent": "ann", "why": "she knows the cache"})).unwrap();
    // the design's step goes to the agent the answer named
    assert_eq!(r.awaits(t).0, "ann");
    r.answer(t, "ann", json!({"head": "Designed", "text": "two parts"})).unwrap();
    let (agent, call, args) = r.awaits(t);
    assert_eq!((agent.as_str(), call.as_str(), &args["pieces"]), ("lead", "place", &json!(["a", "b"])));
    r.answer(t, "lead", json!({"head": "Placed", "picks": [placed("b", "bob"), placed("a", "ann")]})).unwrap();
    // each branch's step goes to its own pick
    let branches: Vec<u64> = r.m.tickets.iter().filter(|(_, x)| x.parent == Some(t)).map(|(u, _)| *u).collect();
    let mut to: Vec<(String, String)> = branches.iter().map(|u| { let (a, _, args) = r.awaits(*u); (args["part"].as_str().unwrap().to_string(), a) }).collect();
    to.sort();
    assert_eq!(to, vec![("a".to_string(), "ann".to_string()), ("b".to_string(), "bob".to_string())]);
    for u in branches {
        let (a, _, args) = r.awaits(u);
        let top = format!("top-{}", args["part"].as_str().unwrap());
        r.answer(u, &a, json!({"head": "Built", "top": top})).unwrap();
    }
    assert!(matches!(&r.m.tickets[&t].status, Status::Done(v) if *v == json!(["top-a", "top-b"])), "{:?}", r.m.tickets[&t].status);
    let mut m2 = machine();
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
}

#[test]
fn a_part_with_no_pick_fails_as_not_found() {
    let mut r = Run::new();
    let t = r.t;
    r.answer(t, "lead", json!({"head": "Assigned", "agent": "ann", "why": "-"})).unwrap();
    r.answer(t, "ann", json!({"head": "Designed", "text": "-"})).unwrap();
    r.answer(t, "lead", json!({"head": "Placed", "picks": [placed("a", "ann")]})).unwrap();
    let failed: Vec<&Raised> = r.m.tickets.values().filter(|x| x.parent == Some(t)).filter_map(|x| if let Status::Failed(f) = &x.status { Some(f) } else { None }).collect();
    assert_eq!(failed.len(), 1, "{:?}", r.m.tickets.values().map(|x| &x.status).collect::<Vec<_>>());
    assert_eq!(failed[0].exc, "NotFound");
}

#[test]
fn coordds_unfit_refuses_the_answer_with_its_reason_and_the_third_is_malformed() {
    let mut r = Run::new();
    const WHY: &str = "zed is not a registered agent (they are ann, bob)";
    // another step than the one owed: nothing
    let out = r.go(Input::Unfit { t: r.t, step: 2, why: WHY.into(), call: None });
    assert!(out.reply.is_err() && out.lines.is_empty(), "{:?}", out.reply);
    for n in 1..=2 {
        let out = r.go(Input::Unfit { t: r.t, step: 1, why: WHY.into(), call: None });
        assert!(out.reply.as_ref().is_err_and(|e| e.contains(WHY)), "{:?}", out.reply);
        assert_eq!(r.refusals(), n);
        assert!(out.effects.iter().any(|e| matches!(e, Effect::Deliver { to } if to == "lead")), "{:?}", out.effects);
    }
    assert!(r.log.iter().any(|l| matches!(l, Line::Unfit { why, .. } if why == WHY)));
    r.go(Input::Unfit { t: r.t, step: 1, why: WHY.into(), call: None });
    let Status::Failed(f) = &r.m.tickets[&r.t].status else { panic!("{:?}", r.m.tickets[&r.t].status) };
    assert_eq!(f.exc, "Malformed");
    let mut m2 = machine();
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert_eq!(m2.tickets[&r.t].status, r.m.tickets[&r.t].status);
}

#[test]
fn a_call_does_not_name_a_parameter_of_and_not_is_free() {
    let file = |text: &str| syntax::parse_call_files(&[("r.call".into(), text.into())], &mut BTreeMap::new());
    let (calls, errors) = file("call build(of: Text) -> Done\n  task t\n");
    assert!(calls.is_empty() && errors.iter().any(|e| e.contains("not named `of`")), "{errors:?}");
    // a call's own `not` (the draft's trace) and a recorded decision's `not: Text?` load
    let (calls, errors) = file("call pick(not: [Ticket]) -> Done\n  task t\n");
    assert!(errors.is_empty() && calls.len() == 1, "{errors:?}");
    let (calls, errors) = file("decision who(carries: [Ticket], not: Text?) -> A | B\n");
    assert!(errors.is_empty() && calls.len() == 1, "{errors:?}");
    let (calls, errors) = file("decision who(carries: [Ticket], not: [Ticket]) -> A | B\n");
    assert!(errors.is_empty() && calls.len() == 1, "{errors:?}");
}

const LISTS: &str = "kind designed
  letter D
  form parts: [Placed], who: Text
  result Placed
  pattern
    p <- step lead place(\"words\", \"its parts\", parts.piece, agents);
    match p {
      Placed(picks) => return find q in picks where q.piece == who and q.why != \"no\"
    }
";

#[test]
fn a_field_of_a_list_is_each_elements_and_find_gives_the_first_that_holds() {
    let parts = json!([placed("a", "ann"), placed("b", "bob")]);
    for (who, found) in [("b", Some(placed("b", "bob"))), ("c", None)] {
        let (d, errors) = defs(LISTS);
        assert!(errors.is_empty(), "{errors:#?}");
        let mut m = Machine::load(d.clone());
        let mut log = Vec::new();
        let out = m.apply(Input::Open { kind: "designed".into(), under: None, opener: "owner".into(), form: json!({"parts": parts, "who": who}), text: None, names: vec![], told: vec![] }, "2026-11-16T11:00:00Z".into());
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        log.extend(out.lines);
        let t = *m.tickets.keys().max().unwrap();
        // `parts.piece`: the piece of each, in order
        let Status::Awaits { owed, .. } = &m.tickets[&t].status else { panic!("{:?}", m.tickets[&t].status) };
        assert_eq!(owed.args["pieces"], json!(["a", "b"]));
        let out = m.apply(Input::Answer { t, by: "lead".into(), head: json!({"head": "Placed", "picks": parts}), text: Text::New("picked".into()) }, "2026-11-16T11:00:01Z".into());
        log.extend(out.lines);
        match found {
            Some(p) => assert!(matches!(&m.tickets[&t].status, Status::Done(v) if *v == p), "{:?}", m.tickets[&t].status),
            None => {
                let Status::Failed(r) = &m.tickets[&t].status else { panic!("{:?}", m.tickets[&t].status) };
                assert_eq!(r.exc, "NotFound");
                assert!(r.value["find"].as_str().is_some_and(|f| f.starts_with("find q in picks where")), "{}", r.value);
            }
        }
        let mut m2 = Machine::load(d);
        assert_eq!(m2.replay(&log), Vec::<String>::new());
    }
    // what does not check: a find in what is not a list, a condition that is not a Bool, a field no element has
    for (bad, says) in [("find q in who where q == \"a\"", "looks in a list"), ("find q in parts where q.piece", "takes a Bool"), ("parts.nope", "no field nope")] {
        let k = LISTS.replace("return find q in picks where q.piece == who and q.why != \"no\"", &format!("return {bad}"));
        let (_, errors) = defs(&k);
        assert!(errors.iter().any(|e| e.contains(says)), "{bad}: {errors:#?}");
    }
}

const QUANT: &str = "kind weighed
  letter W
  form parts: [Placed], who: Text
  result Text
  pattern
    p <- step lead place(\"words\", \"its parts\", parts.piece, agents);
    match p {
      Placed(picks) where all q in picks where q.why != \"no\" => return \"every one fits\"
      Placed(picks) where any q in picks where q.piece == who => return \"its own is there\"
      Placed(picks) => return \"neither\"
    }
";

/// `all x in e where c` and `any x in e where c` (proto-mind N-1446 p2476, for gemini-advisor's p2400, C2): a Bool a
/// `where` asks of a list, true of every element or of one; of an empty list `all` holds and `any` does not. It
/// raises nothing where `find` raises NotFound, prints as written, and a run replays the same; what does not check is
/// refused as `find`'s is.
#[test]
fn all_and_any_ask_a_list_in_a_where_and_raise_nothing() {
    let no = |piece: &str| json!({"piece": piece, "agent": "ann", "why": "no"});
    for (picks, who, said) in [
        (json!([placed("a", "ann"), placed("b", "bob")]), "c", "every one fits"),
        (json!([]), "c", "every one fits"),
        (json!([placed("a", "ann"), no("b")]), "b", "its own is there"),
        (json!([no("a"), no("b")]), "c", "neither"),
    ] {
        let (d, errors) = defs(QUANT);
        assert!(errors.is_empty(), "{errors:#?}");
        let mut m = Machine::load(d.clone());
        let mut log = Vec::new();
        let out = m.apply(Input::Open { kind: "weighed".into(), under: None, opener: "owner".into(), form: json!({"parts": [placed("a", "ann")], "who": who}), text: None, names: vec![], told: vec![] }, "2026-11-24T11:00:00Z".into());
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        log.extend(out.lines);
        let t = *m.tickets.keys().max().unwrap();
        let out = m.apply(Input::Answer { t, by: "lead".into(), head: json!({"head": "Placed", "picks": picks}), text: Text::New("picked".into()) }, "2026-11-24T11:00:01Z".into());
        log.extend(out.lines);
        assert!(matches!(&m.tickets[&t].status, Status::Done(v) if *v == json!(said)), "{picks}: {:?}", m.tickets[&t].status);
        let mut m2 = Machine::load(d);
        assert_eq!(m2.replay(&log), Vec::<String>::new());
    }
    // as written, and read again the same
    let text = crate::print::prog(&defs(QUANT).0.kinds["weighed"].pattern, 0);
    for q in ["all q in picks where q.why != \"no\"", "any q in picks where q.piece == who"] {
        assert!(text.contains(q), "{text}");
    }
    assert_eq!(crate::print::prog(&syntax::parse_prog(&text, 1).unwrap(), 0), text);
    // what does not check: not a list, a condition that is not a Bool, and `all` where a Text is wanted
    for (bad, says) in [("all q in who where q == \"a\"", "`all` looks in a list"), ("any q in picks where q.piece", "`any`'s `where` takes a Bool")] {
        let k = QUANT.replace("all q in picks where q.why != \"no\"", bad);
        let (_, errors) = defs(&k);
        assert!(errors.iter().any(|e| e.contains(says)), "{bad}: {errors:#?}");
    }
    let (_, errors) = defs(&QUANT.replace("return \"neither\"", "return any q in picks where q.why == \"no\""));
    assert!(!errors.is_empty(), "a Bool is no Text");
}
