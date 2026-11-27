//! The owner's typed words (N-487): research's `words` kind as written for it (docket-core8's words/), run through
//! its arms: the box judges, the policy decides, the watcher cleans, the owner looks or the step goes on by itself.

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const CALLS: &str = r#"// The owner's typed words (kinds/words.kind, N-487).

// A place the cleaning was not sure of: the words as they stand, and what they could be meant as.
type Doubt {
  typed: Text
  would: [Text]
}

// The box that sent the words (his window), with Harper and SymSpell, no model. FIXED is TEXT with the certain fixes
// made (one candidate, nothing else near); PER100 is Harper's marks still shown per 100 words. The policy decides
// from PER100 whether a cleaning call is worth it (`worth-cleaning`).
call judge(text: Text) -> Judged(fixed: Text, per100: Int)
  task judge

// One fresh model call a paragraph: the words and a fixed instruction, nothing else (coordd clean.rs, which answers
// it). Ids, paths, names, code and quoted lines come back letter for letter, or the answer is refused; so do KEEP,
// the agents' names and the project's `keep-words`, which are never a doubt.
call clean(text: Text, keep: [Text]) -> Sure(cleaned: Text) | Borderline(cleaned: Text, doubts: [Doubt], per100: Int) | Unreadable(why: Text)
  task clean

// The owner's, shown under the box: the cleaned words, each doubt as a suggestion he can take. Goes on as Go by itself
// after the policy's `lapse-after` (30 s), or at once when his window is gone.
call look(text: Text, cleaned: Text, doubts: [Doubt]) -> Go | Use(text: Text) | AsTyped
  task look
  default Go

// Whether the words are worth one cleaning call: the policy reads `call.per100`.
decision worth-cleaning(per100: Int) -> Clean | Send

// Whether the doubts are few enough to show him: the policy reads how many (`call.doubts-count`) and how many per 100
// words (`call.per100`, coordd's count in clean's answer); the model never decides "too many".
decision how-many(doubts: [Doubt], per100: Int) -> Some | TooMany
"#;

const KINDS: &str = r#"// The owner's typed words, before they go anywhere (owner, research N-481 p11, p29, p33, p40, p43; paper-research's
// design/words.kind; docket-core8, N-487). A box of his window opens it with what he typed (TEXT), which stays as typed
// in this ticket's form, so the record keeps both it and what was sent. WINDOW is the window start that sent them
// (its `coordc bridge --box`; not `box`, the worker that judges): `look` goes on at once when it is gone. KEEP is his own words (the window's dictionary),
// kept as typed. The numbers are research's policy's (policy/words.eflint): 12 marks per 100 words; too many doubts
// at 2 or more and 15 or more per 100 words; 30 s.
kind words
  form text: Text, window: Text, keep: [Text]
  result
    Send(text: Text)    /// these words go on, in place of what he typed
    | Back(why: Text)   /// nothing goes on; his words are back in the box, with why
  pattern
    j <- step box judge(text);
    match j {
      Judged(fixed, per100) =>
        w <- decide worth-cleaning(per100);
        match w {
          Send => return Send(fixed)
          Clean =>
            c <- step watcher clean(fixed, keep);
            match c {
              Sure(cleaned) => return Send(cleaned)
              Borderline(cleaned, doubts, per100d) =>
                v <- decide how-many(doubts, per100d);
                match v {
                  Some =>
                    o <- step owner look(text, cleaned, doubts);
                    match o {
                      Go => return Send(cleaned)
                      Use(mine) => return Send(mine)
                      AsTyped => return Send(text)
                    }
                  TooMany => return Back("too many words I could not be sure of")
                }
              Unreadable(why) => return Back(why)
            }
        }
    }
"#;

fn defs() -> Defs {
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = syntax::parse_calls("words.call", CALLS, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
    let (d, errors) = Defs::build(&[("words.kind".to_string(), KINDS.to_string())], calls, named, 200);
    assert!(errors.is_empty(), "{errors:?}");
    d
}

struct Run {
    m: Machine,
    log: Vec<Line>,
    clock: u32,
}

impl Run {
    fn new(text: &str) -> Run {
        let mut r = Run { m: Machine::load(defs()), log: Vec::new(), clock: 0 };
        let out = r.go(Input::Open { kind: "words".into(), under: None, opener: "owner".into(), form: json!({"text": text, "window": "b-1", "keep": ["yaaihh"]}), text: None, names: vec![], told: vec![] });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        r
    }
    fn go(&mut self, i: Input) -> Out {
        self.clock += 1;
        let out = self.m.apply(i, format!("2026-11-18T15:00:{:02}Z", self.clock));
        self.log.extend(out.lines.iter().cloned());
        out
    }
    fn owes(&self) -> (String, String) {
        match &self.m.tickets[&200].status {
            Status::Awaits { agent, owed, .. } => (agent.clone(), owed.call.clone()),
            Status::Asking { owed, role } => (role.clone(), owed.call.clone()),
            s => panic!("{s:?}"),
        }
    }
    fn answer(&mut self, by: &str, head: Value) {
        let out = self.go(Input::Answer { t: 200, by: by.into(), head, text: Text::New(String::new()) });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
    }
    fn decide(&mut self, answer: &str) {
        let step = self.m.tickets[&200].steps;
        let out = self.go(Input::Decided { t: 200, step, question: "decide".into(), answer: json!(answer), provenance: json!({"test": true}) });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
    }
    fn result(&self) -> Value {
        match self.m.view(200).unwrap().status {
            ViewStatus::Done { result } => result,
            s => panic!("{s:?}"),
        }
    }
    fn replays(&self) {
        let text: Vec<String> = self.log.iter().map(|l| serde_json::to_string(l).unwrap()).collect();
        let lines: Vec<Line> = text.iter().map(|s| serde_json::from_str(s).unwrap()).collect();
        let mut m2 = Machine::load(defs());
        assert_eq!(m2.replay(&lines), Vec::<String>::new());
        for t in self.m.tickets.keys() {
            assert_eq!(m2.view(*t), self.m.view(*t), "ticket {t}");
        }
    }
}

#[test]
fn few_marks_send_the_certain_fixes() {
    let mut r = Run::new("teh words");
    assert_eq!(r.owes(), ("box".into(), "judge".into()));
    r.answer("box", json!({"head": "Judged", "fixed": "the words", "per100": 3}));
    assert_eq!(r.owes().1, "decide worth-cleaning");
    r.decide("Send");
    assert_eq!(r.result(), json!({"head": "Send", "text": "the words"}));
    r.replays();
}

#[test]
fn sure_cleaning_sends() {
    let mut r = Run::new("teh wrods");
    r.answer("box", json!({"head": "Judged", "fixed": "the wrods", "per100": 50}));
    r.decide("Clean");
    assert_eq!(r.owes(), (WATCHER.into(), "clean".into()));
    r.answer(WATCHER, json!({"head": "Sure", "cleaned": "the words"}));
    assert_eq!(r.result(), json!({"head": "Send", "text": "the words"}));
    r.replays();
}

#[test]
fn some_doubts_wait_for_him_or_go_on_by_themselves() {
    let doubts = json!([{"typed": "woth", "would": ["worth", "with"]}]);
    let mut r = Run::new("teh woth");
    r.answer("box", json!({"head": "Judged", "fixed": "the woth", "per100": 50}));
    r.decide("Clean");
    r.answer(WATCHER, json!({"head": "Borderline", "cleaned": "the woth", "doubts": doubts, "per100": 9}));
    assert_eq!(r.owes().1, "decide how-many");
    r.decide("Some");
    assert_eq!(r.owes(), ("owner".into(), "look".into()));
    assert_eq!(r.m.lapsing().len(), 1);
    let out = r.go(Input::Lapsed { t: 200, step: r.m.tickets[&200].steps, head: json!({"head": "Go"}), after: 30, because: "time".into() });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert_eq!(r.result(), json!({"head": "Send", "text": "the woth"}));
    assert_eq!(r.m.view(200).unwrap().lapsed[0].mark(), "went on by itself after 30 s");
    r.replays();
    // or he takes his own words
    let mut r = Run::new("teh woth");
    r.answer("box", json!({"head": "Judged", "fixed": "the woth", "per100": 50}));
    r.decide("Clean");
    r.answer(WATCHER, json!({"head": "Borderline", "cleaned": "the woth", "doubts": doubts, "per100": 9}));
    r.decide("Some");
    r.answer("owner", json!({"head": "Use", "text": "the worth"}));
    assert_eq!(r.result(), json!({"head": "Send", "text": "the worth"}));
    r.replays();
}

#[test]
fn too_many_doubts_or_unreadable_go_back_to_his_box() {
    let mut r = Run::new("x");
    r.answer("box", json!({"head": "Judged", "fixed": "x", "per100": 90}));
    r.decide("Clean");
    r.answer(WATCHER, json!({"head": "Borderline", "cleaned": "x", "doubts": [{"typed": "x", "would": ["y"]}], "per100": 100}));
    r.decide("TooMany");
    assert_eq!(r.result(), json!({"head": "Back", "why": "too many words I could not be sure of"}));
    r.replays();
    let mut r = Run::new("x");
    r.answer("box", json!({"head": "Judged", "fixed": "x", "per100": 90}));
    r.decide("Clean");
    r.answer(WATCHER, json!({"head": "Unreadable", "why": "the second sentence"}));
    assert_eq!(r.result(), json!({"head": "Back", "why": "the second sentence"}));
    r.replays();
}

/// His post of the cleaned words names the words ticket (a `cleaned` line, N-487), where what he typed stays as the
/// form's text; only his own post, and only a words ticket of his ended in Send. Replay gives the same.
#[test]
fn his_post_of_the_cleaned_words_names_the_ticket_his_typed_words_are_in() {
    let mut r = Run::new("teh words");
    r.answer("box", json!({"head": "Judged", "fixed": "the words", "per100": 3}));
    r.decide("Send");
    let words = 200;
    // the thread the words go to (a second words ticket here, still open: it waits on the box, not on him)
    let out = r.go(Input::Open { kind: "words".into(), under: None, opener: "owner".into(), form: json!({"text": "x", "window": "b-1", "keep": []}), text: None, names: vec![], told: vec![] });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let t = 201;
    let post = |r: &mut Run, by: &str| {
        let out = r.go(Input::Post { t, by: by.into(), text: Text::New("the words".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        r.m.tickets[&t].last()
    };
    let n = post(&mut r, "owner");
    let out = r.go(Input::Cleaned { t, n, words });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Cleaned { .. })));
    let v = r.m.view(t).unwrap();
    let p = v.posts.iter().find(|p| p.n == n).unwrap();
    assert_eq!(p.cleaned_from.as_deref(), Some(r.m.id(words).as_str()));
    assert_eq!(r.m.view(words).unwrap().form["text"], json!("teh words"), "what he typed stays");
    // not a words ticket ended in Send; not his post
    assert!(r.go(Input::Cleaned { t, n, words: t }).reply.is_err());
    let other = post(&mut r, "dev");
    assert!(r.go(Input::Cleaned { t, n: other, words }).reply.is_err());
    r.replays();
}
