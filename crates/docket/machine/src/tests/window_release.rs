//! His window's release line reaching programs (N-1446 p1491, 6; proto-mind p1516): the bindings on `window_release`
//! fire, the tickets that take it run their handlers, which make no step, and a cut's `hold W in_window(name, engine)`
//! is answered by the machine once his window has the cut. The same rule for any step (N-1511 p43; paper-research
//! N-1446 p2139, 1): an act that is a system event, a job's end, a cut or a coord's switch, answers the step whose `on`
//! it fits; the git round's `merged` line too, and the facts the lines give a `where` (`reaches`, `tree_of`,
//! `running`; paper-research p2152, p2162). Made up.

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const IN_WINDOW_CALL: &str = include_str!("../../../../../agent-base/calls/in_window.call");

const CALLS: &str = "/// the cut put in his window
call install(name: Text) -> Installed(release: Text) | Refused(why: Text)
/// a look at the release
call look(name: Text) -> Seen
/// a head carried by a cut, or held out
call cut_of(commit: Ref, merge: Ref) -> In(release: Text) | Out(why: Text)
/// the slow checks on a cut's tip
call slow(tip: Ref) -> Ran
/// a head merged onto the train
call merge_of(commit: Ref) -> Merged(merge: Ref)
/// a head merged onto the train with the protocols it numbered, or the commit it conflicts with
call merge_in(commit: Ref) -> Merged(merge: Ref, protocols: [Int]) | Conflict(with: Ref)
";

/// A head at its cut, the slow checks and an install, each answered by an act's line (N-1511 p43): train 69's cut of
/// its merge, with made-up commits and releases.
const ACTS: &str = "kind head
  form commit: Ref, merge: Ref
  result Text
  pattern
    k <- hold role cutter cut_of(commit, merge) on cut(e) where e.commit == merge => In(e.release);
    match k {
      In(release) => return release
      Out(why) => return why
    }

kind slow
  form tip: Ref, tree: Text
  result Text
  pattern
    s <- step role cutter slow(tip) on job_ended(e) where e.verb == \"replay\" and e.tree == tree and e.rc == 0 => Ran;
    match s { Ran => return tree }

kind inst
  form name: Text
  result Text
  pattern
    i <- step role installer install(name) on installed(e) where e.coordd == name => Installed(e.coordd);
    match i {
      Installed(release) => return release
      Refused(why) => return why
    }
";

/// Steps the record's facts answer (p2152): a head held until a cut reaches it, merged once a merged line's tip reaches
/// it, green once a job on the tip's own tree ended well and none on it runs.
const FACTS: &str = "kind held
  form commit: Ref
  result Text
  pattern
    k <- hold role cutter cut_of(commit, commit) on cut(e) where reaches(e.commit, commit) => In(e.release);
    match k {
      In(release) => return release
      Out(why) => return why
    }

kind merge
  form commit: Ref
  result Ref
  pattern
    m <- step role cutter merge_of(commit) on merged(e) where reaches(e.tip, commit) => Merged(e.tip);
    match m { Merged(tip) => return tip }

kind green
  form tip: Ref
  result Ref
  pattern
    s <- step role cutter slow(tip) on job_ended(e) where e.tree == tree_of(tip) and e.rc == 0 and not running(e.tree) => Ran;
    match s { Ran => return tip }
";

const FACTS_KINDS: [(&str, &str); 1] = [("facts.kind", FACTS)];

/// p43's merge: the cutter's answer, which carries protocol numbers no line has, waits until a merged line bears it
/// out, the merge under its tip and the head under the merge (`reaches`, by first 8: the cutter writes short hashes).
const UNTIL: &str = "kind claim
  form commit: Ref
  result Ref
  pattern
    m <- step role cutter merge_in(commit) until merged(e) where reaches(e.tip, merge) and reaches(merge, commit);
    match m {
      Merged(merge, protocols) => return merge
      Conflict(with) => return with
    }
";

const UNTIL_KINDS: [(&str, &str); 1] = [("until.kind", UNTIL)];

/// The head before a step took an `on`: answered by hand only.
const HEAD_BEFORE: &str = "kind head
  form commit: Ref, merge: Ref
  result Text
  pattern
    k <- hold role cutter cut_of(commit, merge);
    match k {
      In(release) => return release
      Out(why) => return why
    }
";

/// The cut, as dev.kind's: installed, then held until his window has it; it says when his window runs it.
const CUT: &str = "kind cut
  form name: Text, engine: Bool
  result Text
  pattern
    i <- step role installer install(name);
    match i {
      Installed(release) =>
        w <- hold role installer in_window(name, engine);
        match w {
          Has(r, c) => return r
        }
      Refused(why) => return why
    }
  when window_release(w) where w.release == name do
    post \"his window runs it\"
";

/// dev.kind's cut, as proto-mind's draft 2 has it (p1516): only the hold, no handler of the line and no binding on it.
const CUT_DEV: &str = "kind cut
  form name: Text, engine: Bool
  result Text
  pattern
    i <- step role installer install(name);
    match i {
      Installed(release) =>
        w <- hold role installer in_window(name, engine);
        match w {
          Has(r, c) => return r
        }
      Refused(why) => return why
    }
";

/// A look whose `during` takes the window's line, and a binding the line fires.
const WATCH: &str = "kind watch
  form name: Text
  result Text
  pattern
    r <- during { step lead look(name) } when window_release(w) where w.release == name do { post \"seen in his window\" };
    match r { Seen => return name }

kind seen
  form release: Text
  result Text
  pattern
    return release

on window_release(w) where w.release == \"R3\" open seen(w.release)
";

/// The same kinds before the line reached programs: no handler of it, no binding on it.
const CUT_BEFORE: &str = "kind cut
  form name: Text, engine: Bool
  result Text
  pattern
    i <- step role installer install(name);
    match i {
      Installed(release) => return release
      Refused(why) => return why
    }
";

const WATCH_BEFORE: &str = "kind watch
  form name: Text
  result Text
  pattern
    r <- step lead look(name);
    match r { Seen => return name }

kind seen
  form release: Text
  result Text
  pattern
    return release
";

fn defs_of(kinds: &[(&str, &str)]) -> (Defs, Vec<String>) {
    let mut named = BTreeMap::new();
    let mut calls: BTreeMap<String, CallSig> = BTreeMap::new();
    for (f, t) in [("in_window.call", IN_WINDOW_CALL), ("cut.call", CALLS)] {
        calls.extend(syntax::parse_calls(f, t, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)));
    }
    let files: Vec<(String, String)> = kinds.iter().map(|(f, t)| (f.to_string(), t.to_string())).collect();
    Defs::build(&files, calls, named, 200)
}

fn defs(kinds: &[(&str, &str)]) -> Defs {
    let (d, errors) = defs_of(kinds);
    assert!(errors.is_empty(), "{errors:?}");
    d
}

/// A post's words, as the machine wrote them.
fn text(t: &Text) -> String {
    match t {
        Text::New(s) => s.clone(),
        other => format!("{other:?}"),
    }
}

const TODAY: [(&str, &str); 2] = [("cut.kind", CUT), ("watch.kind", WATCH)];
const DEV: [(&str, &str); 1] = [("cut.kind", CUT_DEV)];

/// Two branches owed under one `during`, and a kind a binding on the line opens that takes later lines.
const PAIR: &str = "kind pair
  form name: Text
  result Text
  pattern
    r <- during { x <- par(step lead look(name), step fixer look(name)); return name } when window_release(w) where w.release == name do { post \"seen by the pair\" };
    return r

kind follow
  form release: Text
  result Text
  pattern
    r <- step lead look(release);
    match r { Seen => return release }
  when window_release(w) do
    post \"his window moved\"

on window_release(w) where w.release == \"R6\" open follow(w.release)
";

struct Run {
    m: Machine,
    log: Vec<Line>,
    clock: u32,
}

impl Run {
    fn new(kinds: &[(&str, &str)]) -> Run {
        Run { m: Machine::load(defs(kinds)), log: Vec::new(), clock: 0 }
    }
    fn go(&mut self, i: Input) -> Out {
        self.clock += 1;
        let out = self.m.apply(i, format!("2026-11-22T15:00:{:02}Z", self.clock));
        self.log.extend(out.lines.iter().cloned());
        out
    }
    fn open(&mut self, kind: &str, form: Value) -> u64 {
        let out = self.go(Input::Open { kind: kind.into(), under: None, opener: "owner".into(), form, text: None, names: vec![], told: vec![] });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        out.lines.iter().find_map(|l| match l {
            Line::Open { t, .. } => Some(*t),
            _ => None,
        }).unwrap()
    }
    /// The role asked for T's step filled by WHO.
    fn fill(&mut self, t: u64, who: &str) -> Out {
        let Status::Asking { owed, .. } = &self.m.tickets[&t].status else { panic!("{} asks for no role: {:?}", self.m.id(t), self.m.tickets[&t].status) };
        let step = owed.step;
        let out = self.go(Input::Decided { t, step, question: "fills".into(), answer: json!(who), provenance: json!({}) });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        out
    }
    fn answer(&mut self, t: u64, by: &str, head: Value) -> Out {
        let out = self.go(Input::Answer { t, by: by.into(), head, text: Text::New("as said".into()) });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        out
    }
    fn window(&mut self, v: Value) -> Out {
        self.go(Input::Event { name: WINDOW_RELEASE.into(), value: v })
    }
    /// AGENT's act IS, its fields MORE, as build-runner and release.sh send one: taken, and its time.
    fn act(&mut self, agent: &str, is: &str, more: Value) -> (Out, String) {
        let more = more.as_object().cloned().unwrap_or_default();
        let out = self.go(Input::Act { agent: agent.into(), is: is.into(), text: format!("{is} made up"), more, t: None });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        let at = match out.lines.first() {
            Some(Line::Act { at, .. }) => at.clone(),
            other => panic!("no act line: {other:?}"),
        };
        (out, at)
    }
    /// The git round's line: BRANCH of `soft` moved from FROM, bringing COMMITS (each hash, parents, tree), the tip
    /// first; taken, and its time.
    fn merged(&mut self, branch: &str, from: Option<&str>, commits: &[(&str, &[&str], &str)]) -> (Out, String) {
        let cs: Vec<Value> = commits.iter().map(|(h, ps, t)| json!({"commit": h, "parents": ps, "tree": t})).collect();
        let (tip, ps, tree) = commits[0];
        let value = json!({"repo": "soft", "branch": branch, "tip": tip, "tree": tree, "parents": ps, "from": from, "commits": cs});
        let out = self.go(Input::Event { name: MERGED.into(), value });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        (out, format!("2026-11-22T15:00:{:02}Z", self.clock))
    }
    /// The answer T's last step was given, by whom and in what words.
    fn answered(&self, t: u64) -> (String, String) {
        let p = self.m.tickets[&t].posts.iter().rev().find(|p| p.step.is_some()).unwrap();
        (p.by.clone(), text(&p.text))
    }
    /// A cut of NAME, installed by `inst` and at its hold.
    fn cut_at_hold(&mut self, name: &str, engine: bool) -> u64 {
        let t = self.open("cut", json!({"name": name, "engine": engine}));
        self.fill(t, "inst");
        self.answer(t, "inst", json!({"head": "Installed", "release": name}));
        if matches!(self.m.tickets[&t].status, Status::Asking { .. }) {
            self.fill(t, "inst");
        }
        t
    }
    /// The call T awaits now, and from whom.
    fn awaits(&self, t: u64) -> Option<(String, String)> {
        match &self.m.tickets[&t].status {
            Status::Awaits { agent, owed, .. } => Some((owed.call.clone(), agent.clone())),
            _ => None,
        }
    }
    fn done(&self, t: u64) -> Option<Value> {
        match &self.m.tickets[&t].status {
            Status::Done(v) => Some(v.clone()),
            _ => None,
        }
    }
    /// T's posts, by whom and their text, the step each answers.
    fn posts(&self, t: u64) -> Vec<(String, String, Option<u32>)> {
        self.m.tickets[&t].posts.iter().map(|p| (p.by.clone(), text(&p.text), p.step)).collect()
    }
    /// The log, written and read back, replayed under KINDS: no problem, and every ticket as it is here.
    fn replays(&self, kinds: &[(&str, &str)]) -> Machine {
        let lines: Vec<Line> = self.log.iter().map(|l| serde_json::from_str(&serde_json::to_string(l).unwrap()).unwrap()).collect();
        let mut m2 = Machine::load(defs(kinds));
        assert_eq!(m2.replay(&lines), Vec::<String>::new());
        for t in self.m.tickets.keys() {
            assert_eq!(m2.view(*t), self.m.view(*t), "ticket {t}");
        }
        assert_eq!(m2.window_release, self.m.window_release);
        assert_eq!(m2.seen, self.m.seen);
        m2
    }
}

/// The window's line is an event programs take: the binding on it fires when its `where` holds, opening `seen` with
/// the line's release; the cut's own `when window_release` and the look's `during` run, each posting on its ticket as
/// that ticket, never as the agent its step is owed to. A line no `where` holds for changes only what his window runs.
/// The replay makes the same lines and tickets.
#[test]
fn the_window_line_fires_its_bindings_and_runs_the_handlers_of_the_tickets_that_take_it() {
    let mut r = Run::new(&TODAY);
    let w = r.open("watch", json!({"name": "R3"}));
    assert_eq!(r.awaits(w), Some(("look".into(), "lead".into())));
    let t = r.open("cut", json!({"name": "R3", "engine": false}));
    r.fill(t, "inst");
    assert_eq!(r.awaits(t), Some(("install".into(), "inst".into())));
    let before = r.m.tickets.len();
    // R2: no `where` holds; the reply names each ticket that took it
    let out = r.window(json!({"release": "R2", "contains": []}));
    assert!(out.reply.as_ref().is_ok_and(|s| s.starts_with("His window runs R2.")), "{:?}", out.reply);
    assert!(out.lines.iter().all(|l| matches!(l, Line::Event { fired, .. } if fired.is_empty())), "{:?}", out.lines);
    assert_eq!(r.m.tickets.len(), before, "no run, nothing opened");
    // R3: `seen` opens, and both handlers post on their tickets as those tickets
    let out = r.window(json!({"release": "R3", "contains": ["R2"]}));
    let reply = out.reply.clone().unwrap();
    let Some(Line::Event { fired, .. }) = out.lines.first() else { panic!("{:?}", out.lines) };
    assert_eq!(fired.iter().map(|f| (f.kind.as_str(), f.form.clone())).collect::<Vec<_>>(), [("seen", json!({"release": "R3"}))]);
    let s = fired[0].t;
    assert_eq!(r.done(s), Some(json!("R3")));
    assert!(reply.starts_with(&format!("His window runs R3. Opened {}.", r.m.id(s))), "{reply}");
    assert_eq!(r.posts(t).last().map(|(by, text, step)| (by.clone(), text.clone(), *step)), Some((r.m.id(t), "his window runs it".into(), None)));
    assert_eq!(r.posts(w).last().map(|(by, text, step)| (by.clone(), text.clone(), *step)), Some((r.m.id(w), "seen in his window".into(), None)));
    // the steps stay owed as they were: a handler of the line makes none
    assert_eq!((r.awaits(t), r.awaits(w)), (Some(("install".into(), "inst".into())), Some(("look".into(), "lead".into()))));
    let ran: Vec<(&str, Option<&str>)> = r.m.tickets.values().filter(|x| x.handling.is_some()).map(|x| (x.handling.as_deref().unwrap(), x.raised_by.as_deref())).collect();
    assert_eq!(ran, [(WINDOW_RELEASE, Some(NOBODY)), (WINDOW_RELEASE, Some(NOBODY))], "raised by nobody: his window sent it");
    r.replays(&TODAY);
}

/// A record from before the line reached programs: its window lines fired nothing and ran no handler, and its tickets
/// carry their programs, so under today's kinds it replays as it ran: no `seen`, no handler's post.
#[test]
fn a_window_line_from_before_replays_as_it_ran() {
    let before = [("cut.kind", CUT_BEFORE), ("watch.kind", WATCH_BEFORE)];
    let mut r = Run::new(&before);
    let w = r.open("watch", json!({"name": "R3"}));
    let t = r.open("cut", json!({"name": "R3", "engine": false}));
    r.fill(t, "inst");
    let out = r.window(json!({"release": "R3", "contains": []}));
    assert_eq!(out.reply, Ok("His window runs R3.".into()));
    let n = r.m.tickets.len();
    let m2 = r.replays(&TODAY);
    assert_eq!(m2.tickets.len(), n);
    assert!(m2.tickets.values().all(|x| x.kind != "seen" && x.handling.is_none()));
    assert_eq!((m2.tickets[&t].posts.len(), m2.tickets[&w].posts.len()), (r.m.tickets[&t].posts.len(), r.m.tickets[&w].posts.len()));
}

/// A handler of the window's line costs nothing to send, as a project's event's: it makes no step and opens no
/// ticket, in a kind's `when` and in a `during`; and no project declares the line as its own event.
#[test]
fn a_handler_of_the_window_line_makes_no_step_and_no_project_declares_it() {
    let step = "kind k\n  form name: Text\n  result Text\n  pattern\n    return name\n  when window_release(w) do\n    s <- step lead look(w.release);\n    return ()\n";
    let (_, errors) = defs_of(&[("k.kind", step)]);
    assert!(errors.iter().any(|e| e.contains("a handler of a project's event makes no step")), "{errors:?}");
    let during = "kind k\n  form name: Text\n  result Text\n  pattern\n    r <- during { step lead look(name) } when window_release(w) do { s <- step lead look(w.release); post \"x\" };\n    match r { Seen => return name }\n";
    let (_, errors) = defs_of(&[("k.kind", during)]);
    assert!(errors.iter().any(|e| e.contains("makes no step and opens no ticket")), "{errors:?}");
    let declared = "event window_release { release: Text }\n";
    let (_, errors) = defs_of(&[("e.kind", declared)]);
    assert!(errors.iter().any(|e| e.contains("window_release is the system's own event")), "{errors:?}");
    // its fields are typed: a binding's `where` on a field it has not is refused
    let wrong = "kind seen\n  form release: Text\n  result Text\n  pattern\n    return release\n\non window_release(w) where w.nothing == \"R3\" open seen(w.release)\n";
    let (_, errors) = defs_of(&[("s.kind", wrong)]);
    assert!(!errors.is_empty(), "w.nothing is no field of the line");
}

/// The cut waits at its hold until his window has it: a line that has not the cut leaves the hold owed to the
/// installer; the line whose window half lists it answers it, in that line's apply, by the cut itself, with the window
/// half's release and what it contains. The cut then closes with the release his window runs. Replayed the same.
#[test]
fn the_cut_waits_until_a_window_line_has_it_and_the_machine_answers_its_hold() {
    let mut r = Run::new(&TODAY);
    let t = r.cut_at_hold("R3", false);
    assert_eq!(r.awaits(t), Some((IN_WINDOW.into(), "inst".into())));
    assert!(matches!(&r.m.tickets[&t].status, Status::Awaits { owed, .. } if owed.hold), "a hold, never delivered");
    r.window(json!({"release": "R2", "contains": ["R1"]})).reply.unwrap();
    assert_eq!(r.awaits(t), Some((IN_WINDOW.into(), "inst".into())), "R2 has not R3");
    let out = r.window(json!({"release": "R4", "contains": ["R3", "R2"]}));
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert_eq!(r.done(t), Some(json!("R4")));
    let answer = r.m.tickets[&t].posts.iter().rev().find(|p| p.step.is_some()).unwrap();
    assert_eq!((answer.by.clone(), text(&answer.text), answer.head.clone()), (r.m.id(t), "His window runs R4, which has R3.".to_string(), Some(json!({"head": "Has", "release": "R4", "contains": ["R2", "R3"]}))));
    assert!(out.lines.iter().any(|l| matches!(l, Line::Posted { t: u, step: Some(_), .. } if *u == t)), "the answer is in the line's own apply");
    r.replays(&TODAY);
}

/// The last line kept has the cut already: the hold is answered in the apply that reaches it (here the installer's
/// fill of the hold's role), and the cut closes there and then. Replayed the same.
#[test]
fn a_cut_his_window_has_already_goes_on_at_once() {
    let mut r = Run::new(&TODAY);
    r.window(json!({"release": "R3", "contains": []})).reply.unwrap();
    let t = r.open("cut", json!({"name": "R3", "engine": false}));
    r.fill(t, "inst");
    r.answer(t, "inst", json!({"head": "Installed", "release": "R3"}));
    let out = r.fill(t, "inst");
    assert!(out.lines.iter().any(|l| matches!(l, Line::Step { t: u, call, .. } if *u == t && call == IN_WINDOW)), "{:?}", out.lines);
    assert_eq!(r.done(t), Some(json!("R3")));
    r.replays(&TODAY);
}

/// An engine cut waits for coordd's half too (cut 66's line, as proto-mind p1516 tells it, with made-up names: the
/// window half names the cut as its release and lists nothing, the engine half lists other releases): a window-only
/// cut of that name is answered by the release field at once, an engine cut of it waits until a line's engine half
/// has it as well, and a line with no engine half answers it never.
#[test]
fn an_engine_cut_waits_until_coordds_half_has_it_too() {
    let mut r = Run::new(&TODAY);
    let cut = "20261122-084907-a0c60fba";
    let engine = json!({"release": "20261121-221500-9d1e0a3b", "contains": ["20261121-180000-11111111", "20261121-120000-22222222"]});
    r.window(json!({"release": cut, "contains": [], "engine": engine})).reply.unwrap();
    let window_only = r.cut_at_hold(cut, false);
    assert_eq!(r.done(window_only), Some(json!(cut)));
    let t = r.cut_at_hold(cut, true);
    assert_eq!(r.awaits(t), Some((IN_WINDOW.into(), "inst".into())), "coordd's half has not the cut");
    r.window(json!({"release": cut, "contains": []})).reply.unwrap();
    assert_eq!(r.awaits(t), Some((IN_WINDOW.into(), "inst".into())), "no engine half: coordd runs where it was built");
    r.window(json!({"release": cut, "contains": [], "engine": {"release": "20261122-160000-33333333", "contains": [cut]}})).reply.unwrap();
    assert_eq!(r.done(t), Some(json!(cut)));
    let answer = r.m.tickets[&t].posts.iter().rev().find(|p| p.step.is_some()).unwrap();
    assert_eq!(text(&answer.text), format!("His window runs {cut} and coordd runs 20261122-160000-33333333: both have {cut}."));
    r.replays(&TODAY);
}

/// The installer may answer the hold by hand, for a window that names no release; the cut goes on with its answer.
#[test]
fn the_installer_may_answer_the_hold_by_hand() {
    let mut r = Run::new(&TODAY);
    let t = r.cut_at_hold("R3", false);
    assert_eq!(r.m.window_release, None);
    r.answer(t, "inst", json!({"head": "Has", "release": "R3", "contains": []}));
    assert_eq!(r.done(t), Some(json!("R3")));
    assert_eq!(r.posts(t).iter().rev().find(|p| p.2.is_some()).map(|p| p.0.clone()), Some("inst".to_string()));
    r.replays(&TODAY);
}

/// A `during` over two branches that each owe a step takes the line once: one look, one post. A ticket the line's
/// binding opens takes the lines after it, not the one that opened it.
#[test]
fn a_during_over_two_branches_looks_once_and_a_ticket_the_line_opens_takes_the_next() {
    let kinds = [("pair.kind", PAIR)];
    let mut r = Run::new(&kinds);
    let t = r.open("pair", json!({"name": "R3"}));
    let owed: Vec<String> = r.m.tickets.values().filter(|x| x.parent == Some(t)).filter_map(|x| match &x.status {
        Status::Awaits { agent, .. } => Some(agent.clone()),
        _ => None,
    }).collect();
    assert_eq!(owed, ["lead", "fixer"], "two branches, each owing its step");
    r.window(json!({"release": "R3", "contains": []})).reply.unwrap();
    let seen = |r: &Run, t: u64, words: &str| r.posts(t).iter().filter(|p| p.1 == words).count();
    assert_eq!(seen(&r, t, "seen by the pair"), 1, "{:?}", r.posts(t));
    let out = r.window(json!({"release": "R6", "contains": ["R3"]}));
    let Some(Line::Event { fired, .. }) = out.lines.first() else { panic!("{:?}", out.lines) };
    let f = fired[0].t;
    assert_eq!((r.m.tickets[&f].kind.as_str(), r.awaits(f)), ("follow", Some(("look".into(), "lead".into()))));
    assert_eq!(seen(&r, f, "his window moved"), 0, "the line that opened it is not news to it");
    r.window(json!({"release": "R7", "contains": ["R6", "R3"]})).reply.unwrap();
    assert_eq!(seen(&r, f, "his window moved"), 1);
    r.replays(&kinds);
}

/// The way back (proto-mind N-1446 p1516): this engine's records, for the engine before it (a scratch leg on train
/// 67's eff9d23a replays them, soft-swarm-docket-edge8-wrel-old). Each line printed after `REC SCENE `, since the
/// runner drops lines that look like JSON. On dev.kind's cut (`CUT_DEV`): `wait`, the cut held, a line that has not
/// the cut, then one that has it; `once`, the last line has it already; `engine`, an engine cut waits on coordd's half;
/// `hand`, the installer answers it. On the legs' kinds (`TODAY`): `handlers`, the binding and the two handlers.
#[test]
#[ignore]
fn records_for_the_way_back() {
    let print = |scene: &str, r: &Run, kinds: &[(&str, &str)]| {
        r.replays(kinds);
        for l in &r.log {
            println!("REC {scene} {}", serde_json::to_string(l).unwrap());
        }
    };
    let mut r = Run::new(&DEV);
    r.cut_at_hold("R3", false);
    r.window(json!({"release": "R2", "contains": ["R1"]})).reply.unwrap();
    r.window(json!({"release": "R4", "contains": ["R3", "R2"]})).reply.unwrap();
    print("wait", &r, &DEV);
    let mut r = Run::new(&DEV);
    r.window(json!({"release": "R3", "contains": []})).reply.unwrap();
    r.cut_at_hold("R3", false);
    print("once", &r, &DEV);
    let mut r = Run::new(&DEV);
    r.window(json!({"release": "R3", "contains": [], "engine": {"release": "R2", "contains": []}})).reply.unwrap();
    r.cut_at_hold("R3", true);
    r.window(json!({"release": "R3", "contains": [], "engine": {"release": "R4", "contains": ["R3"]}})).reply.unwrap();
    print("engine", &r, &DEV);
    let mut r = Run::new(&DEV);
    let t = r.cut_at_hold("R3", false);
    r.answer(t, "inst", json!({"head": "Has", "release": "R3", "contains": []}));
    print("hand", &r, &DEV);
    let mut r = Run::new(&TODAY);
    r.open("watch", json!({"name": "R3"}));
    let t = r.open("cut", json!({"name": "R3", "engine": false}));
    r.fill(t, "inst");
    r.window(json!({"release": "R3", "contains": ["R2"]})).reply.unwrap();
    print("handlers", &r, &TODAY);
}

const ACTS_KINDS: [(&str, &str); 1] = [("acts.kind", ACTS)];

/// A cut's fields, as release.sh's `cut` act has them.
fn cut(release: &str, commit: &str) -> Value {
    json!({"phase": "cut", "release": release, "commit": commit, "tree": "b205c772", "folder": "releases/x"})
}

/// An act that is a system event answers the step whose `on` it fits (N-1511 p43, 2: train 69's cut): the head's
/// hold, owed to the cutter, stays owed at a cut of another commit and at a `live` act, which is no event; the cut of
/// its merge answers it in the act's own apply, by the head itself, with the cut's release. No line is the event's
/// own: the act line is written as before, and the answer is its output. Replayed the same.
#[test]
fn an_act_that_is_an_event_answers_the_step_whose_on_it_fits() {
    let mut r = Run::new(&ACTS_KINDS);
    let h = r.open("head", json!({"commit": "da42e6ab", "merge": "1d59ca1f"}));
    r.fill(h, "core");
    assert_eq!(r.awaits(h), Some(("cut_of".into(), "core".into())));
    let (out, _) = r.act("core", "release", cut("R1", "6467db1a"));
    assert_eq!(out.reply, Ok("Recorded.".into()));
    assert_eq!(r.awaits(h), Some(("cut_of".into(), "core".into())), "a cut of another commit");
    r.act("core", "release", json!({"phase": "live", "release": "R2", "commit": "1d59ca1f"}));
    assert_eq!(r.awaits(h), Some(("cut_of".into(), "core".into())), "a live act is no event");
    let (out, at) = r.act("core", "release", cut("R2", "1d59ca1f"));
    assert_eq!(r.done(h), Some(json!("R2")));
    let answer = r.m.tickets[&h].posts.iter().rev().find(|p| p.step.is_some()).unwrap();
    assert_eq!((answer.by.clone(), text(&answer.text), answer.head.clone()), (r.m.id(h), format!("The cut line of {at} answers it."), Some(json!({"head": "In", "release": "R2"}))));
    assert!(out.lines.iter().any(|l| matches!(l, Line::Posted { t: u, step: Some(_), .. } if *u == h)), "the answer is the act's output: {:?}", out.lines);
    assert_eq!(out.lines.iter().filter(|l| matches!(l, Line::Act { .. } | Line::Event { .. })).count(), 1, "one act line, no event line: {:?}", out.lines);
    assert_eq!(r.m.seen.cuts["1d59ca1f"].1["release"], json!("R2"));
    r.replays(&ACTS_KINDS);
}

/// A job's end answers by its fields, its numbers read as numbers (`e.rc == 0`): a start, a test's end and a red
/// replay's leave the slow checks owed; the green replay's end on the tip's tree answers them.
#[test]
fn a_job_end_answers_by_its_fields_and_a_start_answers_nothing() {
    let mut r = Run::new(&ACTS_KINDS);
    let s = r.open("slow", json!({"tip": "7f124386", "tree": "b205c772"}));
    r.fill(s, "core");
    let job = |phase: &str, verb: &str, rc: &str| {
        json!({"phase": phase, "id": "380700-1795450943", "verb": verb, "commit": "7f124386", "tree": "b205c772", "branch": "train-69", "packages": ["coordd"], "items": ["N-1446.p2139"], "rc": rc, "passed": "775", "failed": "0"})
    };
    r.act("core", "job", job("start", "replay", ""));
    r.act("core", "job", job("end", "test", "0"));
    r.act("core", "job", job("end", "replay", "101"));
    assert_eq!(r.awaits(s), Some(("slow".into(), "core".into())));
    let last = &r.m.seen.ends["b205c772"].last().unwrap().1;
    assert_eq!((last["rc"].clone(), last["passed"].clone()), (json!(101), json!(775)));
    r.act("core", "job", job("end", "replay", "0"));
    assert_eq!(r.done(s), Some(json!("b205c772")));
    r.replays(&ACTS_KINDS);
}

/// The last act of an event kept answers a step given after it, as the window's last line answers `in_window`: a cut
/// already made answers the head's hold as it is given, at the cutter's fill of its role.
#[test]
fn a_step_given_after_its_event_is_answered_at_once() {
    let mut r = Run::new(&ACTS_KINDS);
    r.act("core", "release", cut("R2", "1d59ca1f"));
    let h = r.open("head", json!({"commit": "da42e6ab", "merge": "1d59ca1f"}));
    let out = r.fill(h, "core");
    assert!(out.lines.iter().any(|l| matches!(l, Line::Step { t: u, call, .. } if *u == h && call == "cut_of")), "{:?}", out.lines);
    assert_eq!(r.done(h), Some(json!("R2")));
    r.replays(&ACTS_KINDS);
}

/// Its worker may still answer by hand what no `on` of the step gives (a head held out); what one gives is its event's
/// alone (paper-research N-1446 p2255, 3). A hand In is refused, saying the way out, and nothing is written; when the
/// cut's line will not come, the lead's cancel and then its repair end it, each a line of its own, the repair by whom
/// and why. The installer's turn read as Installed is not read: it is reminded, with why. A coord's switch answers
/// the install it fits. Replayed the same.
#[test]
fn its_worker_may_answer_by_hand_what_no_on_gives_and_a_switch_answers_the_install() {
    let mut r = Run::new(&ACTS_KINDS);
    let h = r.open("head", json!({"commit": "da42e6ab", "merge": "1d59ca1f"}));
    r.fill(h, "core");
    r.answer(h, "core", json!({"head": "Out", "why": "a replay failed on its files"}));
    assert_eq!(r.done(h), Some(json!("a replay failed on its files")));
    // In is the cut line's alone: by hand, refused, with the way out
    let k = r.open("head", json!({"commit": "da42e6ab", "merge": "1d59ca1f"}));
    r.fill(k, "core");
    let (kid, n) = (r.m.id(k), r.log.len());
    let out = r.go(Input::Answer { t: k, by: "core".into(), head: json!({"head": "In", "release": "R2"}), text: Text::New("it is in R2".into()) });
    assert_eq!(out.reply, Err(format!("In at step 1 of {kid} comes only from its `cut` line, never by hand: when that line will not come, its lead cancels, withdraws or repairs {kid}.")));
    assert_eq!((r.log.len(), r.awaits(k)), (n, Some(("cut_of".into(), "core".into()))));
    // the line will not come (its act was lost while coordd was down): the lead cancels, then repairs
    r.go(Input::Cancel { t: k, by: "lead".into(), why: "its cut act was lost while coordd was down".into() }).reply.unwrap();
    assert!(matches!(r.m.tickets[&k].status, Status::Failed(_)), "{:?}", r.m.tickets[&k].status);
    let why = "R2 has it (cut 20261122-150000-1d59ca1f): its line was lost";
    let out = r.go(Input::Repair { t: k, by: "lead".into(), program: "return \"R2\"".into(), why: why.into() });
    assert_eq!(out.reply, Ok(format!("{kid} repaired: it runs again.")));
    let repair = |l: &Line| matches!(l, Line::Edited { t, by, what, why: w, .. } if *t == k && by == "lead" && what == "repair" && w == why);
    assert!(out.lines.iter().any(repair), "{:?}", out.lines);
    assert_eq!(r.done(k), Some(json!("R2")));
    // the installer's turn read as Installed: not read, and it is told why
    let i = r.open("inst", json!({"name": "R2"}));
    r.fill(i, "pr");
    let d = r.m.due("pr").into_iter().find(|d| d.t == i).expect("the install is pr's");
    let loc = |n: u64| Loc { file: "transcript.jsonl".into(), offset: n * 100, len: 50 };
    r.go(Input::Given { t: i, to: "pr".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: Some(loc(1)) }).reply.unwrap();
    let out = r.go(Input::TurnEnded { agent: "pr".into(), delivery: Some(d.id.clone()), text: loc(2), later: vec![], stood: None, go: None });
    assert!(out.effects.iter().any(|e| matches!(e, Effect::Form { t, .. } if *t == i)), "{out:?}");
    let out = r.go(Input::Formed { t: i, step: 1, head: Some(json!({"head": "Installed", "release": "R2"})), why: None, not_taken: None, call: None });
    assert!(out.lines.iter().any(|l| matches!(l, Line::Reminded { t, .. } if *t == i)), "{:?}", out.lines);
    let iid = r.m.id(i);
    let told = format!("Installed at step 1 of {iid} comes only from its `installed` line, never by hand: when that line will not come, its lead cancels, withdraws or repairs {iid}.");
    assert!(matches!(&r.m.view(i).unwrap().status, ViewStatus::Awaits { read_none: Some(w), .. } if *w == told), "{:?}", r.m.view(i).unwrap().status);
    let switch = |coordd: &str| json!({"phase": "switch", "coord": "research", "coordd": coordd, "coordd_commit": "7f124386", "way_back": "release.sh back", "from": "R0"});
    r.act("pr", "release", switch("R1"));
    assert_eq!(r.awaits(i), Some(("install".into(), "pr".into())));
    r.act("pr", "release", switch("R2"));
    assert_eq!(r.done(i), Some(json!("R2")));
    r.replays(&ACTS_KINDS);
}

/// A record from before: its head's program, kept on its open line, has no `on`, so under today's kinds the cut's act
/// answers nothing and the record replays as it ran.
#[test]
fn an_act_from_before_replays_as_it_ran() {
    let mut r = Run::new(&[("acts.kind", HEAD_BEFORE)]);
    let h = r.open("head", json!({"commit": "da42e6ab", "merge": "1d59ca1f"}));
    r.fill(h, "core");
    r.act("core", "release", cut("R2", "1d59ca1f"));
    assert_eq!(r.awaits(h), Some(("cut_of".into(), "core".into())));
    let m2 = r.replays(&ACTS_KINDS);
    assert!(matches!(&m2.tickets[&h].status, Status::Awaits { owed, .. } if owed.call == "cut_of" && owed.on.is_empty()));
}

/// An `on` is checked in its step's scope: a system event, a field it has, a Bool `where` and an answer of the
/// step's; it prints as written and reads back the same. An act's event runs no handler and fires no binding: its
/// line records neither.
#[test]
fn an_on_is_checked_and_an_acts_event_runs_no_handler_or_binding() {
    let head = |on: &str| format!("kind k\n  form commit: Ref, merge: Ref\n  result Text\n  pattern\n    k <- hold role cutter cut_of(commit, merge) {on};\n    match k {{\n      In(r) => return r\n      Out(w) => return w\n    }}\n");
    for (on, says) in [
        ("on nothing(e) => In(\"R\")", "`on` takes a system event"),
        ("on cut(e) where e.nothing == merge => In(e.release)", "nothing"),
        ("on cut(e) where e.release => In(e.release)", "`where` takes a Bool"),
        ("on cut(e) => Seen", "Seen"),
    ] {
        let (_, errors) = defs_of(&[("k.kind", head(on).as_str())]);
        assert!(errors.iter().any(|e| e.contains("its `on ") && e.contains(says)), "{on}: {errors:?}");
    }
    let (_, errors) = defs_of(&[("k.kind", head("on cut(e) where e.commit == merge => In(e.release) on window_release(w) where w.release == \"R\" => In(w.release)").as_str())]);
    assert!(errors.is_empty(), "{errors:?}");
    let p = syntax::parse_prog("k <- hold role cutter cut_of(commit, merge) on cut(e) where e.commit == merge => In(e.release);\nreturn k", 1).unwrap();
    assert_eq!(crate::print::prog(&p, 0), "k <- hold role cutter cut_of(commit, merge) on cut(e) where e.commit == merge => In(e.release);\nreturn k");
    assert_eq!(syntax::parse_prog(&crate::print::prog(&p, 0), 1).unwrap(), p);
    let handler = "kind k\n  form name: Text\n  result Text\n  pattern\n    return name\n  when cut(e) do\n    post \"cut\"\n";
    let (_, errors) = defs_of(&[("k.kind", handler)]);
    assert!(errors.iter().any(|e| e.contains("cut answers steps and runs no handler")), "{errors:?}");
    let binding = "kind seen\n  form release: Text\n  result Text\n  pattern\n    return release\n\non cut(e) where e.release == \"R\" open seen(e.release)\n";
    let (_, errors) = defs_of(&[("s.kind", binding)]);
    assert!(errors.iter().any(|e| e.contains("cut answers steps and opens no ticket")), "{errors:?}");
}

/// A job end is kept by its tree, so a step given late finds its own among others (paper-research p2162): a job ends on
/// X, another on Y, and the step for X, given after both, is answered by X's end; one for a tree no job ended on stays
/// owed.
#[test]
fn a_step_given_late_finds_its_own_job_end_among_others() {
    let mut r = Run::new(&ACTS_KINDS);
    let end = |id: &str, tree: &str| json!({"phase": "end", "id": id, "verb": "replay", "commit": "7f124386", "tree": tree, "branch": "train-69", "packages": ["coordd"], "items": [], "rc": "0"});
    let (_, at_x) = r.act("core", "job", end("1-1", "aaaa0001"));
    r.act("core", "job", end("2-2", "bbbb0002"));
    let x = r.open("slow", json!({"tip": "7f124386", "tree": "aaaa0001"}));
    r.fill(x, "core");
    assert_eq!(r.done(x), Some(json!("aaaa0001")));
    assert_eq!(r.answered(x), (r.m.id(x), format!("The job_ended line of {at_x} answers it.")));
    let z = r.open("slow", json!({"tip": "7f124386", "tree": "cccc0003"}));
    r.fill(z, "core");
    assert_eq!(r.awaits(z), Some(("slow".into(), "core".into())));
    assert_eq!(r.m.seen.ends.keys().collect::<Vec<_>>(), ["aaaa0001", "bbbb0002"]);
    r.replays(&ACTS_KINDS);
}

/// The git round's `merged` line is the machine's own and an event a step's `on` takes; its commits' parents give
/// `reaches` (p2152). The merge step is answered by the line whose tip reaches the head, not the branch's creation; a
/// cut of the train's next commit, made before the merged line that brings it, reaches nothing yet, and that line,
/// which changes the facts, has the hold read its cuts again and answered by the cut. A cut of another commit answers
/// nothing. A line that is not one is refused. Replayed the same.
#[test]
fn the_merged_lines_give_reaches_and_a_cut_before_its_merged_line_answers_once_it_comes() {
    let (a, b, m, v) = ("a0000001", "b0000002", "c0000003", "d0000004");
    let mut r = Run::new(&FACTS_KINDS);
    let h = r.open("held", json!({"commit": b}));
    r.fill(h, "core");
    let g = r.open("merge", json!({"commit": b}));
    r.fill(g, "core");
    r.merged("train-1", None, &[(a, &[], "t000000a")]);
    assert_eq!(r.awaits(g), Some(("merge_of".into(), "core".into())), "the branch made at A has no B under it");
    let (out, at) = r.merged("train-1", Some(a), &[(m, &[a, b], "t000000c"), (b, &[a], "t000000b")]);
    assert_eq!(r.done(g), Some(json!(m)));
    assert_eq!(r.answered(g), (r.m.id(g), format!("The merged line of {at} answers it.")));
    assert_eq!(out.lines.iter().filter(|l| matches!(l, Line::Event { name, fired, .. } if name == MERGED && fired.is_empty())).count(), 1);
    let (_, cut_at) = r.act("core", "release", cut("R9", v));
    r.act("core", "release", cut("R8", "e0000005"));
    assert_eq!(r.awaits(h), Some(("cut_of".into(), "core".into())), "V is no commit a merged line brought yet");
    r.merged("train-1", Some(m), &[(v, &[m], "t000000d")]);
    assert_eq!(r.done(h), Some(json!("R9")));
    assert_eq!(r.answered(h), (r.m.id(h), format!("The cut line of {cut_at} answers it.")));
    assert_eq!(r.m.seen.tips["soft/train-1"], v);
    assert_eq!(r.m.seen.parents[m], [a, b]);
    assert_eq!(r.m.seen.fact("reaches", &[json!(v), json!(format!("commit:{b}"))]), json!(true));
    assert_eq!(r.m.seen.fact("reaches", &[json!(b), json!(m)]), json!(false));
    let bad = r.go(Input::Event { name: MERGED.into(), value: json!({"repo": "soft", "branch": "train-1"}) });
    assert!(bad.reply.as_ref().is_err_and(|e| e.starts_with("A merged line names")), "{:?}", bad.reply);
    r.replays(&FACTS_KINDS);
}

/// `tree_of` reads a commit's tree from a merged line and from a job run on the commit unchanged, never from one run on
/// changes; `running` holds while a job on the tree has started and not ended, and a start in a worktree ends the one
/// before it there. The green waits while a second job on the tip's tree runs, and is answered by that job's end.
#[test]
fn tree_of_and_running_read_the_jobs_and_a_job_still_running_holds_the_green() {
    let (m, t) = ("c0000003", "t000000c");
    let mut r = Run::new(&FACTS_KINDS);
    r.merged("train-1", None, &[(m, &["a0000001", "b0000002"], t)]);
    let g = r.open("green", json!({"tip": m}));
    r.fill(g, "core");
    let job = |phase: &str, id: &str, wt: &str, rc: &str| json!({"phase": phase, "id": id, "wt": wt, "verb": "test", "commit": m, "tree": t, "branch": "train-1", "packages": ["coordd"], "items": [], "rc": rc, "changed": "0"});
    r.act("core", "job", job("start", "1-1", "/w/one", ""));
    r.act("core", "job", job("start", "2-2", "/w/two", ""));
    r.act("core", "job", job("end", "1-1", "/w/one", "0"));
    assert_eq!(r.awaits(g), Some(("slow".into(), "core".into())), "2-2 runs on the tree");
    assert_eq!(r.m.seen.fact("running", &[json!(t)]), json!(true));
    let (_, at) = r.act("core", "job", job("end", "2-2", "/w/two", "0"));
    assert_eq!(r.done(g), Some(json!(m)));
    assert_eq!(r.answered(g), (r.m.id(g), format!("The job_ended line of {at} answers it.")));
    // a clean job gives its commit's tree; one on changes, another tree on the same commit, does not
    let other = |phase: &str, id: &str, tree: &str, changed: &str| json!({"phase": phase, "id": id, "wt": "/w/three", "verb": "test", "commit": "f0000006", "tree": tree, "changed": changed});
    r.act("core", "job", other("start", "3-3", "t000000f", "0"));
    r.act("core", "job", other("start", "4-4", "t0000010", "2"));
    assert_eq!(r.m.seen.fact("tree_of", &[json!("f0000006")]), json!("t000000f"));
    assert_eq!(r.m.seen.fact("tree_of", &[json!("90000009")]), Value::Null);
    assert_eq!((r.m.seen.fact("running", &[json!("t000000f")]), r.m.seen.fact("running", &[json!("t0000010")])), (json!(false), json!(true)), "3-3 ended with 4-4's start in its worktree");
    r.replays(&FACTS_KINDS);
}

/// A fact is read only in a step's `on`, outside a `find`, with its arguments; `not` reads and prints back; the git
/// round's `merged` runs no handler and fires no binding.
#[test]
fn facts_are_read_only_in_an_on_and_merged_runs_no_handler_or_binding() {
    let head = |on: &str| format!("kind k\n  form commit: Ref, merge: Ref\n  result Text\n  pattern\n    k <- hold role cutter cut_of(commit, merge) {on};\n    match k {{\n      In(r) => return r\n      Out(w) => return w\n    }}\n");
    for (on, says) in [
        ("on cut(e) where reaches(e.commit) => In(e.release)", "`reaches` takes 2 arguments"),
        ("on cut(e) where (find p in [merge] where reaches(e.commit, p)) != none => In(e.release)", "read only in a step's `on`, outside a `find`"),
        ("on cut(e) where not e.release => In(e.release)", "`not` takes a Bool"),
    ] {
        let (_, errors) = defs_of(&[("k.kind", head(on).as_str())]);
        assert!(errors.iter().any(|e| e.contains(says)), "{on}: {errors:?}");
    }
    let outside = "kind k\n  form commit: Ref\n  result Text\n  pattern\n    return tree_of(commit)\n";
    let (_, errors) = defs_of(&[("k.kind", outside)]);
    assert!(errors.iter().any(|e| e.contains("`tree_of` is a fact of the record's lines, read only in a step's `on`")), "{errors:?}");
    let on = "on cut(e) where not reaches(e.commit, merge) and not (e.release == \"R\" or running(e.tree)) => In(e.release)";
    let (_, errors) = defs_of(&[("k.kind", head(on).as_str())]);
    assert!(errors.is_empty(), "{errors:?}");
    let p = syntax::parse_prog(&format!("k <- hold role cutter cut_of(commit, merge) {on};\nreturn k"), 1).unwrap();
    assert_eq!(crate::print::prog(&p, 0), format!("k <- hold role cutter cut_of(commit, merge) {on};\nreturn k"));
    assert_eq!(syntax::parse_prog(&crate::print::prog(&p, 0), 1).unwrap(), p);
    let handler = "kind k\n  form name: Text\n  result Text\n  pattern\n    return name\n  when merged(e) do\n    post \"moved\"\n";
    let (_, errors) = defs_of(&[("k.kind", handler)]);
    assert!(errors.iter().any(|e| e.contains("merged answers steps and runs no handler")), "{errors:?}");
    let binding = "kind seen\n  form release: Text\n  result Text\n  pattern\n    return release\n\non merged(e) where e.branch == \"train-1\" open seen(e.tip)\n";
    let (_, errors) = defs_of(&[("s.kind", binding)]);
    assert!(errors.iter().any(|e| e.contains("merged answers steps and opens no ticket")), "{errors:?}");
}

/// `until` keeps the worker's answer until a line bears it out (N-1511 p43, the merge): a claim made before its merged
/// line waits, the step still owed and said so; a merged line of another tip leaves it; the line that brings the merge
/// with the head under it takes it as the worker gave it. Given after that line, a claim it bears out is taken at once,
/// and a wrong one waits until the worker answers again. An answer without the field the `where` reads (`Conflict`) is
/// taken at once. Replayed the same.
#[test]
fn an_until_keeps_the_workers_answer_until_a_line_bears_it_out() {
    let (a, b, m) = ("a0000001", "b0000002", "c0000003");
    let mut r = Run::new(&UNTIL_KINDS);
    let c = r.open("claim", json!({"commit": b}));
    r.fill(c, "core");
    let out = r.answer(c, "core", json!({"head": "Merged", "merge": m, "protocols": [50]}));
    assert!(out.reply.as_ref().is_ok_and(|s| s.contains("keeps your answer until a merged line bears it out")), "{:?}", out.reply);
    assert_eq!(r.awaits(c), Some(("merge_in".into(), "core".into())));
    r.merged("train-1", None, &[(a, &[], "t000000a")]);
    assert_eq!(r.awaits(c), Some(("merge_in".into(), "core".into())), "the line of A bears out no merge M");
    r.merged("train-1", Some(a), &[(m, &[a, b], "t000000c"), (b, &[a], "t000000b")]);
    assert_eq!(r.done(c), Some(json!(m)));
    assert_eq!(r.answered(c), ("core".into(), "as said".into()));
    let d = r.open("claim", json!({"commit": b}));
    r.fill(d, "core");
    r.answer(d, "core", json!({"head": "Merged", "merge": "f0000009", "protocols": []}));
    assert_eq!(r.awaits(d), Some(("merge_in".into(), "core".into())), "no merged line brought F");
    let out = r.answer(d, "core", json!({"head": "Merged", "merge": m, "protocols": [51]}));
    assert!(out.reply.as_ref().is_ok_and(|s| s.starts_with("Answered step")), "{:?}", out.reply);
    assert_eq!(r.done(d), Some(json!(m)));
    let e = r.open("claim", json!({"commit": b}));
    r.fill(e, "core");
    r.answer(e, "core", json!({"head": "Conflict", "with": a}));
    assert_eq!(r.done(e), Some(json!(a)));
    r.replays(&UNTIL_KINDS);
}

/// An `until` takes a system event and a `where` read in the step's scope, its event's value and the answer's fields;
/// it prints back as written.
#[test]
fn an_until_is_checked_and_prints_back() {
    let head = |until: &str| format!("kind k\n  form commit: Ref\n  result Ref\n  pattern\n    m <- step role cutter merge_in(commit) {until};\n    match m {{\n      Merged(merge, protocols) => return merge\n      Conflict(with) => return with\n    }}\n");
    for (until, says) in [
        ("until merged(e) where e.tip == nothing", "its `until merged`"),
        ("until looked(e) where e.tip == merge", "`until` takes a system event"),
        ("until merged(e) where e.tip", "`where` takes a Bool"),
    ] {
        let (_, errors) = defs_of(&[("k.kind", head(until).as_str())]);
        assert!(errors.iter().any(|e| e.contains(says)), "{until}: {errors:?}");
    }
    let until = "until merged(e) where e.tip == merge and (e.branch == \"train-1\" or not reaches(e.tip, commit))";
    let (_, errors) = defs_of(&[("k.kind", head(until).as_str())]);
    assert!(errors.is_empty(), "{errors:?}");
    let p = syntax::parse_prog(&format!("m <- step role cutter merge_in(commit) {until};\nreturn m"), 1).unwrap();
    assert_eq!(crate::print::prog(&p, 0), format!("m <- step role cutter merge_in(commit) {until};\nreturn m"));
    assert_eq!(syntax::parse_prog(&crate::print::prog(&p, 0), 1).unwrap(), p);
}
