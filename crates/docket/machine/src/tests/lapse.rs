//! The owner's step that goes on by itself (N-487): `default C` on a call, the `lapsed` line.

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const CALLS: &str = "call look(text: Text) -> Go | Use(text: Text) | AsTyped
  task look
  default Go
";

const KINDS: &str = "kind words
  form text: Text
  result Text
  pattern
    o <- step owner look(text);
    match o {
      Go => return \"cleaned\"
      Use(mine) => return mine
      AsTyped => return text
    }
";

fn defs_of(calls: &str, kinds: &str) -> (Defs, Vec<String>) {
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = syntax::parse_calls("words.call", calls, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
    Defs::build(&[("words.kind".to_string(), kinds.to_string())], calls, named, 200)
}

struct Run {
    m: Machine,
    log: Vec<Line>,
    clock: u32,
}

impl Run {
    fn new() -> Run {
        let (d, errors) = defs_of(CALLS, KINDS);
        assert!(errors.is_empty(), "{errors:?}");
        Run { m: Machine::load(d), log: Vec::new(), clock: 0 }
    }
    fn go(&mut self, i: Input) -> Out {
        self.clock += 1;
        let out = self.m.apply(i, format!("2026-11-18T14:00:{:02}Z", self.clock));
        self.log.extend(out.lines.iter().cloned());
        out
    }
    fn open(&mut self) -> u64 {
        let out = self.go(Input::Open { kind: "words".into(), under: None, opener: "owner".into(), form: json!({"text": "teh words"}), text: None, names: vec![], told: vec![] });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        200
    }
    fn lapse(&mut self, t: u64, head: Value) -> Out {
        self.go(Input::Lapsed { t, step: 1, head, after: 30, because: "time".into() })
    }
    fn replays(&self) {
        let text: Vec<String> = self.log.iter().map(|l| serde_json::to_string(l).unwrap()).collect();
        let lines: Vec<Line> = text.iter().map(|s| serde_json::from_str(s).unwrap()).collect();
        let mut m2 = Machine::load(defs_of(CALLS, KINDS).0);
        assert_eq!(m2.replay(&lines), Vec::<String>::new());
        for t in self.m.tickets.keys() {
            assert_eq!(m2.view(*t), self.m.view(*t), "ticket {t}");
        }
    }
}

#[test]
fn an_owners_step_goes_on_with_its_default_and_says_so_and_replays_the_same() {
    let mut r = Run::new();
    let t = r.open();
    assert_eq!(r.m.lapsing(), vec![(t, 1, "look".to_string(), "Go".to_string(), "2026-11-18T14:00:01Z".to_string())]);
    let out = r.lapse(t, json!({"head": "Go"}));
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Lapsed { after: 30, .. })));
    let v = r.m.view(t).unwrap();
    assert_eq!(v.status, ViewStatus::Done { result: json!("cleaned") });
    assert_eq!(v.lapsed.len(), 1);
    let l = &v.lapsed[0];
    assert_eq!((l.step, l.after, l.because.as_str(), l.mark()), (1, 30, "time", "went on by itself after 30 s".to_string()));
    let post = v.posts.iter().find(|p| Some(p.n) == l.post).expect("the answer's post");
    assert_eq!((post.by.as_str(), post.step), ("owner", Some(1)));
    assert!(r.m.lapsing().is_empty(), "the set is kept small");
    r.replays();
}

/// He answered first, in the same second: the lapse that comes after is refused, and replay agrees.
#[test]
fn a_lapse_after_his_answer_is_refused() {
    let mut r = Run::new();
    let t = r.open();
    let out = r.go(Input::Answer { t, by: "owner".into(), head: json!({"head": "AsTyped"}), text: Text::New(String::new()) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let out = r.lapse(t, json!({"head": "Go"}));
    assert!(out.reply.is_err(), "{:?}", out.reply);
    let v = r.m.view(t).unwrap();
    assert_eq!(v.status, ViewStatus::Done { result: json!("teh words") });
    assert!(v.lapsed.is_empty());
    r.replays();
}

#[test]
fn only_the_default_lapses() {
    let mut r = Run::new();
    let t = r.open();
    assert!(r.lapse(t, json!({"head": "AsTyped"})).reply.is_err());
    assert!(r.go(Input::Lapsed { t, step: 2, head: json!({"head": "Go"}), after: 30, because: "time".into() }).reply.is_err(), "not the step owed");
    assert!(r.lapse(t, json!({"head": "Go"})).reply.is_ok());
}

#[test]
fn a_default_is_a_bare_answer_of_the_call_and_for_the_owners_steps_only() {
    let mut named = BTreeMap::new();
    let e = syntax::parse_calls("w.call", "call look(text: Text) -> Go | Use(text: Text)\n  task look\n  default Use\n", &mut named).unwrap_err();
    assert!(e.contains("no fields"), "{e}");
    let e = syntax::parse_calls("w.call", "call look(text: Text) -> Go | AsTyped\n  task look\n  default Stop\n", &mut named).unwrap_err();
    assert!(e.contains("none of the call's answers"), "{e}");
    let (_, errors) = defs_of(CALLS, &KINDS.replace("step owner look", "step fresh look"));
    assert!(errors.iter().any(|e| e.contains("only `step owner look(...)`")), "{errors:?}");
    // a call with no default: the owner's step waits as any other
    let (d, errors) = defs_of("call look(text: Text) -> Go | Use(text: Text) | AsTyped\n  task look\n", KINDS);
    assert!(errors.is_empty(), "{errors:?}");
    let mut m = Machine::load(d);
    m.apply(Input::Open { kind: "words".into(), under: None, opener: "owner".into(), form: json!({"text": "x"}), text: None, names: vec![], told: vec![] }, "2026-11-18T14:00:00Z".into());
    assert!(m.lapsing().is_empty());
    assert!(m.apply(Input::Lapsed { t: 200, step: 1, head: json!({"head": "Go"}), after: 30, because: "time".into() }, "2026-11-18T14:00:31Z".into()).reply.is_err());
}
