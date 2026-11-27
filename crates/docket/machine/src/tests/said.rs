//! What a ticket came to (`view::said`, docket-edge5's order for proto-reader's N-1446 p197) and a mark's words for his
//! margin (`view::said_in_words`, proto-reader p197, consult-3 p205), on agent-base's own brain.kind and brain.call
//! (head (A); one brain, proto-mind p270): `attend`, then `elect` unless the pass is quiet before it; and on aside.kind's
//! shape (core2-e2, not in proto-0.3 yet: his press, then the join, as inline steps, as docket-edge5's 6196fa0f wrote
//! them).

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const BRAIN_CALL: &str = include_str!("../../../../../agent-base/calls/brain.call");
const BRAIN_KIND: &str = include_str!("../../../../../agent-base/kinds/brain.kind");

const ASIDE: &str = "\
/// taken aside
kind aside
  letter A
  form agent: Text
  result
    Joined(decided: [Text], questions: [Text])
    | Dropped(why: Text)
  drawn mark
  pattern
    a <- step owner \"Take it aside?\" -> Through | NotThis;
    match a {
      NotThis => return Dropped(\"he pressed Not this\")
      Through =>
        j <- step lead \"Join back\" -> Settled(decided: [Text], questions: [Text]);
        match j {
          Settled(decided, questions) => return Joined(decided, questions)
        }
    }
";

fn machine() -> Machine {
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = syntax::parse_calls("brain.call", BRAIN_CALL, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
    let files = [("brain.kind".to_string(), BRAIN_KIND.to_string()), ("aside.kind".to_string(), ASIDE.to_string())];
    let (d, errors) = Defs::build(&files, calls, named, 200);
    assert!(errors.is_empty(), "{errors:#?}");
    Machine::load(d)
}

fn go(m: &mut Machine, log: &mut Vec<Line>, i: Input) {
    let out = m.apply(i, "2026-11-20T19:01:20Z".into());
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    log.extend(out.lines);
}

fn pass(m: &mut Machine, log: &mut Vec<Line>) -> u64 {
    go(m, log, Input::Open { kind: "brain".into(), under: None, opener: "watcher".into(), form: json!({"agent": "paper-research", "pass": {"head": "Attend", "posts": ["the stills?", "the stills, please"]}}), text: None, names: vec![], told: vec![] });
    *m.tickets.keys().max().unwrap()
}

fn aside(m: &mut Machine, log: &mut Vec<Line>) -> u64 {
    go(m, log, Input::Open { kind: "aside".into(), under: None, opener: "lead".into(), form: json!({"agent": "fixer"}), text: None, names: vec!["lead".into()], told: vec![] });
    *m.tickets.keys().max().unwrap()
}

fn answer_by(m: &mut Machine, log: &mut Vec<Line>, t: u64, by: &str, head: Value) {
    go(m, log, Input::Answer { t, by: by.into(), head, text: Text::New(String::new()) });
}

fn answer(m: &mut Machine, log: &mut Vec<Line>, t: u64, head: Value) {
    answer_by(m, log, t, "watcher", head);
}

/// `attend`'s answer: his topic, on it or not, with these proposals.
fn read(on_topic: bool, proposals: &[&str]) -> Value {
    let proposals: Vec<Value> = proposals.iter().map(|a| json!({"act": a, "rests_on": "N-1104.p614"})).collect();
    json!({"head": "Read", "topic": "prompt dos and don'ts", "on_topic": on_topic, "signals": [], "proposals": proposals, "earlier": []})
}
const THOUGHT: &str = "He asked for the stills twice; send them before anything else.";

/// A pass's ticket says what it came to: while `elect` runs, `attend`'s answer; once closed, its result. A mark's words
/// are that in words once it is closed, never as the engine prints it: Quiet for a quiet pass (before `elect` or after
/// it), the head word for one that spoke (whose line in the window is its post, the thought), a failure's why; while it
/// runs, none, so its label stands (consult-3 N-1446 p244); one that raised, what it raised, in its own words (p258).
/// An aside: nothing while it waits for his press; Through once pressed, which is a step's answer on the way, so no
/// words; Joined once joined; his Not this by its why. The outline's state says which came to nothing (said nothing,
/// failed, dropped). A replay says the same.
#[test]
fn a_brain_pass_says_its_result_once_closed_and_a_mark_says_it_in_words() {
    let mut m = machine();
    let mut log = Vec::new();
    let words = |m: &Machine, t: u64| (view::said(m, &m.tickets[&t]), view::said_in_words(m, &m.tickets[&t]));

    // nothing answered yet: nothing said, the label stands; agent-base's brain.kind draws it as a note (consult-3
    // N-1446 p901), which stands at its post as a mark does (proto-reader p901)
    let spoke = pass(&mut m, &mut log);
    assert_eq!(words(&m, spoke), (None, None));
    assert_eq!(view::drawn(&m, &m.tickets[&spoke]), Some(syntax::Drawn::Note));
    assert!(view::stands(&m, &m.tickets[&spoke]) && !view::outlined(&m, &m.tickets[&spoke]));
    // attend answered, elect running: its answer; no words while it runs (no "Read" on his margin)
    answer(&mut m, &mut log, spoke, read(false, &["answer his N-1104.p614"]));
    let (said, in_words) = words(&m, spoke);
    assert!(said.as_deref().is_some_and(|s| s.starts_with("Read(") && s.contains("prompt dos and don'ts")), "{said:?}");
    assert_eq!(in_words, None);
    // Pick: it posts the thought and comes to Said(…), whose words are its head word
    answer(&mut m, &mut log, spoke, json!({"head": "Pick", "act": "answer his N-1104.p614", "rests_on": "N-1104.p614", "against": "", "thought": THOUGHT, "his": "", "scores": []}));
    assert!(matches!(&m.tickets[&spoke].status, Status::Done(_)), "{:?}", m.tickets[&spoke].status);
    assert!(matches!(m.tickets[&spoke].posts.last().map(|p| &p.text), Some(Text::New(s)) if s == THOUGHT), "the thought is its post: {:?}", m.tickets[&spoke].posts);
    let (said, in_words) = words(&m, spoke);
    assert!(said.as_deref().is_some_and(|s| s.starts_with("Said(")), "its result, not elect's Pick: {said:?}");
    assert_eq!(in_words.as_deref(), Some("Said"));

    // Nothing after elect: Quiet(…), "Quiet"
    let nothing = pass(&mut m, &mut log);
    answer(&mut m, &mut log, nothing, read(false, &["answer his N-1104.p614"]));
    answer(&mut m, &mut log, nothing, json!({"head": "Nothing", "scores": [{"act": "answer his N-1104.p614", "newest": 1, "rules": 2, "owed": 0, "sees": 0, "cost": 2, "rank": 0}]}));
    let (said, in_words) = words(&m, nothing);
    assert!(said.as_deref().is_some_and(|s| s.starts_with("Quiet(")), "{said:?}");
    assert_eq!(in_words.as_deref(), Some("Quiet"));
    assert!(m.tickets[&nothing].posts.is_empty());

    // quiet before elect: Read on his topic with nothing to propose, Quiet(…), "Quiet"
    let before = pass(&mut m, &mut log);
    answer(&mut m, &mut log, before, read(true, &[]));
    assert_eq!(words(&m, before).1.as_deref(), Some("Quiet"));

    // a failed call: Failed(why), its why
    let failed = pass(&mut m, &mut log);
    answer(&mut m, &mut log, failed, json!({"head": "Failed", "why": "the attention call failed: Codex at its limit"}));
    assert_eq!(words(&m, failed).1.as_deref(), Some("the attention call failed: Codex at its limit"));

    // a pass that raised after attend answered: elect refused a third time raises Malformed. The trace keeps the raise
    // as elect's visit, so `said` (a run row) prints it, never attend's Read; the mark's words are the refusal's own
    // why, where before they were none and only its label stood (consult-3 p258.1, proto-reader p262)
    let raised = pass(&mut m, &mut log);
    answer(&mut m, &mut log, raised, read(false, &["answer his N-1104.p614"]));
    for _ in 0..3 {
        let out = m.apply(Input::Answer { t: raised, by: "watcher".into(), head: json!({"head": "Bogus"}), text: Text::New(String::new()) }, "2026-11-20T19:01:20Z".into());
        assert!(out.reply.is_err(), "refused: {:?}", out.reply);
        log.extend(out.lines);
    }
    let why = match &m.tickets[&raised].status {
        Status::Failed(r) if r.exc == "Malformed" => r.value["why"].as_str().unwrap().to_string(),
        s => panic!("{s:?}"),
    };
    let (said, in_words) = words(&m, raised);
    assert!(said.as_deref().is_some_and(|s| s.contains("Malformed") && !s.contains("Read")), "{said:?}");
    assert_eq!(in_words.as_deref(), Some(crate::label::first_line(&why, 160).as_str()));
    // the sentence on his margin, as the refusal wrote it (types.rs `conforms`; proto-reader p504.1)
    assert_eq!(in_words.as_deref(), Some("Bogus is not one of Pick, Nothing, Failed"));

    // a raise whose value is a text: attend's step withdrawn raises Silent with its agent, "watcher". Its words say what
    // was raised with that text, where before they were "raised Silent"; the engine's NoCall, a text too, goes the same
    // way (consult-3 p504)
    let silent = pass(&mut m, &mut log);
    go(&mut m, &mut log, Input::Expired { t: silent, step: 1, provenance: json!({}) });
    assert!(matches!(&m.tickets[&silent].status, Status::Failed(r) if r.exc == "Silent" && r.value == json!("watcher")), "{:?}", m.tickets[&silent].status);
    assert_eq!(words(&m, silent).1.as_deref(), Some("raised Silent: watcher"));

    // the aside: nothing while it waits for his press; pressed Through, on its way to the join, no words; joined,
    // "Joined"; his Not this, its why
    let through = aside(&mut m, &mut log);
    assert_eq!(words(&m, through), (None, None), "waiting for his press");
    answer_by(&mut m, &mut log, through, "owner", json!({"head": "Through"}));
    assert_eq!(words(&m, through), (Some("Through".into()), None));
    answer_by(&mut m, &mut log, through, "lead", json!({"head": "Settled", "decided": ["the stills go first"], "questions": []}));
    let (said, in_words) = words(&m, through);
    assert!(said.as_deref().is_some_and(|s| s.starts_with("Joined(")), "{said:?}");
    assert_eq!(in_words.as_deref(), Some("Joined"));
    let dropped = aside(&mut m, &mut log);
    answer_by(&mut m, &mut log, dropped, "owner", json!({"head": "NotThis"}));
    assert_eq!(words(&m, dropped), (Some(r#"Dropped("he pressed Not this")"#.into()), Some("he pressed Not this".into())));

    // the outline's word for each: only a pass that spoke, and an aside that went or waits, stand in his thread; a
    // pass that said nothing, one whose call failed and an aside he dropped came to nothing (proto-mind N-1446 p270)
    let state = |m: &Machine, t: u64| ViewOutline::of(m, &m.tickets[&t], false).state;
    assert_eq!([spoke, nothing, before, failed, through, dropped].map(|t| state(&m, t)), ["done", "said nothing", "said nothing", "failed", "done", "dropped"]);

    // no mark's words hold a value as the engine prints it
    for t in [spoke, nothing, before, failed, through, dropped] {
        let w = words(&m, t).1.unwrap();
        assert!(!w.contains('(') && !w.contains('"'), "{w}");
    }
    let mut m2 = machine();
    assert_eq!(m2.replay(&log), Vec::<String>::new());
    for t in [spoke, nothing, before, failed, raised, silent, through, dropped] {
        assert_eq!(words(&m2, t), words(&m, t));
    }
}

/// The base step (brain.kind's Ahead; the owner, N-1104 p606, p610; N-1511 p56, p59) on agent-base's brain.kind and
/// brain.call, with research's binding beside them. `turn_end` has a declared type (fresh-lead-3 N-1511 p62), so a
/// misspelt field in the binding is refused at its check, and so is a plain `on turn_end` that gives its EVERY to an
/// Int: an agent the policy gives no `check-in-every` sends none. `every e.every of` fires only on an event that has
/// it, and reads it as an Int. Bound, the second of paper-research's turn ends at N 2 opens one pass, Ahead(2), whose
/// step is the watcher's `ahead`; another agent's turn opens nothing. Next posts its thought naming the agent: a post
/// that asks it to act, so it is pushed to it and stands in its ledger, and the mark says Next. Same posts nothing and
/// comes to nothing ("said nothing"), as Quiet does. A replay gives the same.
#[test]
fn the_base_step_opens_at_the_nth_turn_end_its_next_asks_the_agent_and_its_same_says_nothing() {
    const BOUND: &str = "every e.every of turn_end(e) where e.agent == \"paper-research\" open brain(e.agent, Ahead(e.every))\n";
    let defs = |binding: &str| {
        let mut named = BTreeMap::new();
        let calls: BTreeMap<String, CallSig> = syntax::parse_calls("brain.call", BRAIN_CALL, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
        let files = [("brain.kind".to_string(), BRAIN_KIND.to_string()), ("paper-research.kind".to_string(), binding.to_string())];
        Defs::build(&files, calls, named, 200)
    };
    let refused = |binding: &str| defs(binding).1.join("; ");
    let agnet = refused(&BOUND.replace("e.agent ==", "e.agnet =="));
    assert!(agnet.contains("no field agnet"), "a misspelt field is refused: {agnet}");
    let plain = refused("on turn_end(e) where e.agent == \"paper-research\" open brain(e.agent, Ahead(e.every))\n");
    assert!(!plain.is_empty(), "an `on` reads EVERY as it may be: none, not an Int");
    assert!(refused("on owner_to(m) open brain(m.agent, Attend(m.post))\n").contains("no field post"), "owner_to's too");
    assert_eq!(refused("on owner_to(m) where m.agent == \"paper-research\" open brain(m.agent, Attend(m.posts))\n"), "", "brain.kind's own line for Attend");
    let (d, errors) = defs(BOUND);
    assert!(errors.is_empty(), "{errors:#?}");
    let mut m = Machine::load(d);
    let mut log = Vec::new();
    let end = |m: &mut Machine, log: &mut Vec<Line>, agent: &str| {
        let out = m.apply(Input::Event { name: syntax::TURN_END.into(), value: json!({"agent": agent, "answered": false, "every": 2}) }, "2026-11-24T10:00:00Z".into());
        log.extend(out.lines);
    };
    let passes = |m: &Machine| m.tickets.values().filter(|x| x.kind == "brain").map(|x| x.t).collect::<Vec<_>>();
    end(&mut m, &mut log, "paper-research");
    end(&mut m, &mut log, "fixer");
    assert!(passes(&m).is_empty(), "1 of 2, and another agent's turn");
    end(&mut m, &mut log, "paper-research");
    let opened = passes(&m);
    let [next] = opened[..] else { panic!("one pass: {opened:?}") };
    assert_eq!(m.tickets[&next].form, json!({"agent": "paper-research", "pass": {"head": "Ahead", "turns": 2}}));
    let step = |m: &Machine, t: u64| m.due(WATCHER).into_iter().find(|d| d.t == t).map(|d| match d.what {
        What::Step { call, args, .. } => (call, args),
        w => panic!("{w:?}"),
    });
    assert_eq!(step(&m, next), Some(("ahead".to_string(), json!({"agent": "paper-research", "turns": 2}))));
    let thought = "I said I would send the stills on N-1104.p604 and have not. I send them now.";
    answer(&mut m, &mut log, next, json!({"head": "Next", "goal": "the stills he asked for", "critic": "he waits on them", "memory": "it said it would send them",
        "attention": "he waits on the stills", "act": "send the stills", "rests_on": "N-1104.p604", "thought": thought, "notes": "it said it would send the stills"}));
    assert!(matches!(&m.tickets[&next].status, Status::Done(v) if v["head"] == "Next" && v["notes"] == "it said it would send the stills"), "{:?}", m.tickets[&next].status);
    let post = m.tickets[&next].posts.last().cloned().expect("its thought");
    assert!(matches!(&post.text, Text::New(s) if s == thought), "{post:?}");
    assert_eq!(post.names, ["paper-research"], "named: a post that asks it to act");
    let id = format!("{}.p{}", m.id(next), post.n);
    assert!(m.waiting("paper-research").iter().any(|i| i["id"] == json!(id) && i["push"] == json!(true)), "pushed to it: {:?}", m.waiting("paper-research"));
    assert!(m.ledger("paper-research").iter().any(|c| c.post == id && c.answer.is_none()), "in its ledger till it answers");
    end(&mut m, &mut log, "paper-research");
    end(&mut m, &mut log, "paper-research");
    let same = *passes(&m).iter().max().unwrap();
    assert_ne!(same, next);
    answer(&mut m, &mut log, same, json!({"head": "Same", "goal": "the stills", "critic": "nothing", "memory": "it sent them", "attention": "none", "notes": "check it sent the stills"}));
    assert!(m.tickets[&same].posts.is_empty(), "Same says nothing");
    let words = |m: &Machine, t: u64| view::said_in_words(m, &m.tickets[&t]);
    let state = |m: &Machine, t: u64| ViewOutline::of(m, &m.tickets[&t], false).state;
    assert_eq!([next, same].map(|t| (words(&m, t), state(&m, t))), [(Some("Next".into()), "done".into()), (Some("Same".into()), "said nothing".into())]);
    let mut m2 = Machine::load(defs(BOUND).0);
    assert_eq!(m2.replay(&log), Vec::<String>::new());
    assert_eq!([next, same].map(|t| words(&m2, t)), [next, same].map(|t| words(&m, t)));
}

/// Consolidation (brain.kind's Compacting; paper-research N-1104 p622, 5, his p624) on agent-base's brain.kind and
/// brain.call, with research's binding beside them. `compacting` is coordd's, `{agent}`: a misspelt constructor is
/// refused at the binding's check. Bound, paper-research's compaction opens one pass, Compacting, whose step is the
/// watcher's `consolidate`; another agent's opens nothing. Kept posts what it keeps informing the agent, asking it for
/// nothing: a line its next quiet point gives (`told`, not pushed) and no ask in its ledger. In words it is its first
/// line, and its state "kept". A replay gives the same.
#[test]
fn consolidation_opens_at_a_compaction_and_its_kept_informs_the_agent_asking_nothing() {
    const BOUND: &str = "on compacting(e) where e.agent == \"paper-research\" open brain(e.agent, Compacting)\n";
    let defs = |binding: &str| {
        let mut named = BTreeMap::new();
        let calls: BTreeMap<String, CallSig> = syntax::parse_calls("brain.call", BRAIN_CALL, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
        let files = [("brain.kind".to_string(), BRAIN_KIND.to_string()), ("paper-research.kind".to_string(), binding.to_string())];
        Defs::build(&files, calls, named, 200)
    };
    let misspelt = defs(&BOUND.replace("Compacting", "Compactin")).1.join("; ");
    assert!(!misspelt.is_empty(), "a constructor brain.kind's pass has not is refused");
    let (d, errors) = defs(BOUND);
    assert!(errors.is_empty(), "{errors:#?}");
    let mut m = Machine::load(d);
    let mut log = Vec::new();
    let compacts = |m: &mut Machine, log: &mut Vec<Line>, agent: &str| {
        let out = m.apply(Input::Event { name: syntax::COMPACTING.into(), value: json!({"agent": agent}) }, "2026-11-24T19:00:00Z".into());
        log.extend(out.lines);
    };
    let passes = |m: &Machine| m.tickets.values().filter(|x| x.kind == "brain").map(|x| x.t).collect::<Vec<_>>();
    compacts(&mut m, &mut log, "fixer");
    assert!(passes(&m).is_empty(), "another agent's compaction");
    compacts(&mut m, &mut log, "paper-research");
    let opened = passes(&m);
    let [t] = opened[..] else { panic!("one pass: {opened:?}") };
    assert_eq!(m.tickets[&t].form, json!({"agent": "paper-research", "pass": {"head": "Compacting"}}));
    let step = m.due(WATCHER).into_iter().find(|d| d.t == t).map(|d| match d.what {
        What::Step { call, args, .. } => (call, args),
        w => panic!("{w:?}"),
    });
    assert_eq!(step, Some(("consolidate".to_string(), json!({"agent": "paper-research"}))));
    let kept = "I am on the stills for him, N-1104.p604.\nI said they come after the replay, N-1446.p12.";
    answer(&mut m, &mut log, t, json!({"head": "Kept", "text": kept}));
    assert!(matches!(&m.tickets[&t].status, Status::Done(v) if *v == json!({"head": "Kept", "text": kept})), "{:?}", m.tickets[&t].status);
    let post = m.tickets[&t].posts.last().cloned().expect("what it keeps");
    assert!(matches!(&post.text, Text::New(s) if s == kept), "{post:?}");
    assert_eq!((post.names.clone(), post.inform.clone(), post.into_turn.clone()), (Vec::<String>::new(), vec!["paper-research".to_string()], vec!["paper-research".to_string()]));
    let id = format!("{}.p{}", m.id(t), post.n);
    assert!(m.waiting("paper-research").iter().any(|i| i["id"] == json!(id) && i["into_turn"] == json!(true) && i["push"] != json!(true)), "told, not pushed: {:?}", m.waiting("paper-research"));
    assert!(!m.ledger("paper-research").iter().any(|c| c.post == id), "no ask in its ledger");
    let words = |m: &Machine| view::said_in_words(m, &m.tickets[&t]);
    assert_eq!((words(&m), ViewOutline::of(&m, &m.tickets[&t], false).state), (Some("I am on the stills for him, N-1104.p604.".into()), "kept".into()));
    let mut m2 = Machine::load(defs(BOUND).0);
    assert_eq!(m2.replay(&log), Vec::<String>::new());
    assert_eq!(words(&m2), words(&m));
}

/// Inhibition (brain.kind's Ahead with STOP; paper-research N-1104 p622, 3; N-1446 p2925) on agent-base's brain.kind
/// and brain.call, with research's binding of the base step. A Next that carries a stop posts the thought and then the
/// stop, naming the agent, pushed to it and in its ledger as a Next is; its result keeps the stop. A Same that carries
/// one ends the pass Stop, which posts the stop alone informing the agent, asking it for nothing: a line its next quiet
/// point gives (`told`, not pushed) and no ask in its ledger. In words it is its head, and its state "done": a pass
/// that spoke. A Same whose stop is null says nothing, as a Same from before the field does (the base step's test
/// above answers without one). A replay gives the same.
#[test]
fn the_base_steps_stop_follows_its_thought_and_with_same_ends_stop_informing_the_agent_asking_nothing() {
    const BOUND: &str = "every e.every of turn_end(e) where e.agent == \"paper-research\" open brain(e.agent, Ahead(e.every))\n";
    let defs = || {
        let mut named = BTreeMap::new();
        let calls: BTreeMap<String, CallSig> = syntax::parse_calls("brain.call", BRAIN_CALL, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
        let files = [("brain.kind".to_string(), BRAIN_KIND.to_string()), ("paper-research.kind".to_string(), BOUND.to_string())];
        Defs::build(&files, calls, named, 200)
    };
    let (d, errors) = defs();
    assert!(errors.is_empty(), "{errors:#?}");
    let mut m = Machine::load(d);
    let mut log = Vec::new();
    let look = |m: &mut Machine, log: &mut Vec<Line>| -> u64 {
        for _ in 0..2 {
            let out = m.apply(Input::Event { name: syntax::TURN_END.into(), value: json!({"agent": "paper-research", "answered": false, "every": 2}) }, "2026-11-24T20:00:00Z".into());
            log.extend(out.lines);
        }
        m.tickets.values().filter(|x| x.kind == "brain").map(|x| x.t).max().expect("a pass")
    };
    let four = json!({"goal": "the stills he asked for", "critic": "he waits on them", "memory": "it said it would send them", "attention": "he waits on the stills"});
    let with = |head: &str, more: Value| {
        let mut v = four.clone();
        v["head"] = json!(head);
        v.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
        v
    };
    let id = |m: &Machine, t: u64, n: u32| format!("{}.p{n}", m.id(t));

    // Next with a stop: the thought, then the stop, in one post that asks the agent to act
    let next = look(&mut m, &mut log);
    let thought = "I said I would send the stills on N-1104.p604 and have not. I send them now.";
    let stop = "I posted the replay's progress three times on N-1446.p12, unasked. I stop.";
    let more = json!({"act": "send the stills", "rests_on": "N-1104.p604", "thought": thought, "notes": "told it to stop the progress posts", "stop": stop});
    answer(&mut m, &mut log, next, with("Next", more.clone()));
    assert_eq!(m.tickets[&next].status, Status::Done(with("Next", more)));
    let post = m.tickets[&next].posts.last().cloned().expect("its thought and its stop");
    assert_eq!((post.text.clone(), post.names.clone()), (Text::New(format!("{thought}\n{stop}")), vec!["paper-research".to_string()]));
    let pushed = id(&m, next, post.n);
    assert!(m.waiting("paper-research").iter().any(|i| i["id"] == json!(pushed) && i["push"] == json!(true)), "pushed to it: {:?}", m.waiting("paper-research"));
    assert!(m.ledger("paper-research").iter().any(|c| c.post == pushed && c.answer.is_none()), "in its ledger till it answers");

    // Same with a stop: Stop, the stop alone, informing the agent
    let stopped = look(&mut m, &mut log);
    let again = "I posted the replay's progress again on N-1446.p14. I stop until it is done.";
    answer(&mut m, &mut log, stopped, with("Same", json!({"notes": "told it to stop again", "stop": again})));
    assert_eq!(m.tickets[&stopped].status, Status::Done(with("Stop", json!({"notes": "told it to stop again", "stop": again}))));
    let post = m.tickets[&stopped].posts.last().cloned().expect("its stop");
    assert!(matches!(&post.text, Text::New(s) if s == again), "{post:?}");
    assert_eq!((post.names.clone(), post.inform.clone(), post.into_turn.clone()), (Vec::<String>::new(), vec!["paper-research".to_string()], vec!["paper-research".to_string()]));
    let told = id(&m, stopped, post.n);
    assert!(m.waiting("paper-research").iter().any(|i| i["id"] == json!(told) && i["into_turn"] == json!(true) && i["push"] != json!(true)), "told, not pushed: {:?}", m.waiting("paper-research"));
    assert!(!m.ledger("paper-research").iter().any(|c| c.post == told), "no ask in its ledger");

    // Same with a null stop: nothing said
    let same = look(&mut m, &mut log);
    answer(&mut m, &mut log, same, with("Same", json!({"notes": "nothing new", "stop": null})));
    assert_eq!(m.tickets[&same].status, Status::Done(with("Same", json!({"notes": "nothing new"}))));
    assert!(m.tickets[&same].posts.is_empty(), "Same says nothing");

    let words = |m: &Machine, t: u64| (view::said_in_words(m, &m.tickets[&t]), ViewOutline::of(m, &m.tickets[&t], false).state);
    assert_eq!([next, stopped, same].map(|t| words(&m, t)), [(Some("Next".into()), "done".into()), (Some("Stop".into()), "done".into()), (Some("Same".into()), "said nothing".into())]);
    let mut m2 = Machine::load(defs().0);
    assert_eq!(m2.replay(&log), Vec::<String>::new());
    assert_eq!([next, stopped, same].map(|t| words(&m2, t)), [next, stopped, same].map(|t| words(&m, t)));
}
