//! The hooks driver: the frames' shared Effects named once, each line carried by its agent's frame, and it named
//! as that frame is.

use coordd_v2::hooks::{Held, Hooks};
use coordd_v2::link::{link, Waits};
use docket_hooks::Frame;
use docket_v2::norm::Core;
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::machine::{Machine, State};
use docket_v2::run::record::Line;
use docket_v2::run::value::{Instance, Value};
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn a_line_goes_to_the_frame_that_holds_its_agent_and_each_effect_is_named_once() {
    let dir = std::env::temp_dir().join(format!("coordd-v2-hooks-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let claude = Rc::new(RefCell::new(Frame::new(docket_driver_claude::Claude)));
    let codex = Rc::new(RefCell::new(Frame::new(docket_driver_codex::Codex)));
    codex.borrow_mut().open("a", &dir).unwrap();
    let held: Vec<Rc<RefCell<dyn Held>>> = vec![claude.clone(), codex.clone()];
    let mut hooks = Hooks(held);
    let named: Vec<String> = hooks.effects().into_iter().map(|(e, _)| e).collect();
    assert_eq!(named, ["std.hooks.Next-Moment", "std.hooks.Apply", "std.hooks.Tool-Use"]);
    // a's frame is codex's: the claude frame, linked first, never sees a's line
    assert!(matches!(hooks.carry("std.hooks.Next-Moment", &[Value::text("a")], 7), Reply::Later));
    assert!(matches!(hooks.carry("std.hooks.Next-Moment", &[Value::text("b")], 8), Reply::Failed(w) if w == "b has no session"));
    // its `Lets` lines are those of the harness that holds the party, and none for a party it holds no session of
    assert_eq!(hooks.name_for(&Value::text("a")), codex.borrow().name());
    assert_eq!(hooks.name_for(&Value::text("b")), [Value::text("hooks")]);
    assert_eq!(hooks.waits().len(), codex.borrow().waits().len() + claude.borrow().waits().len());
    hooks.dropped(7);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A machine on a text whose Act P1 waits for agent `a`'s next moment, with `drivers`.
fn waiting(drivers: Vec<Box<dyn Driver>>) -> Machine<Core> {
    let text = "module std.hooks\nEffect Next-Moment(agent: String) -> String\nAct P1(by: String) Returns String Does { perform Next-Moment(\"a\") }";
    let loaded = coordd_v2::texts::load(&[("t".into(), text.into())]).unwrap();
    Machine::new(loaded, Core::default(), drivers).unwrap()
}

#[test]
fn a_record_written_with_a_frame_linked_alone_restores_with_the_hooks_driver() {
    let dir = std::env::temp_dir().join(format!("coordd-v2-hooks-restore-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // before: a's frame its own driver (the five so linked are refused now: one Effect, one carrier), and no line
    // of the record names a driver's place
    let mut codex = Frame::new(docket_driver_codex::Codex);
    codex.open("a", &dir).unwrap();
    let mut old = waiting(vec![link(codex).0]);
    old.start(Value::text("w1"), Some(0), "", Instance { ty: "std.hooks.P1".into(), args: vec![Value::text("w1")] }).unwrap();
    old.run();
    assert!(old.record.iter().any(|l| matches!(l, Line::Effect { op, .. } if op == "std.hooks.Next-Moment")), "{:?}", old.record);
    // now: one hooks driver; the resting line is asked again of a's frame, and the record is the same
    let codex = Rc::new(RefCell::new(Frame::new(docket_driver_codex::Codex)));
    codex.borrow_mut().open("a", &dir).unwrap();
    let held: Vec<Rc<RefCell<dyn Held>>> = vec![Rc::new(RefCell::new(Frame::new(docket_driver_claude::Claude))), codex.clone()];
    let mut m = waiting(vec![Box::new(Hooks(held))]);
    m.restore(&old.record, &|_, _| (Some(0), String::new()), &mut |_| Err("one text".into())).unwrap();
    assert_eq!(m.record, old.record);
    // the run rests on the line again, asked of a's frame
    assert!(matches!(m.state(0), State::Resting(_)));
    std::fs::remove_dir_all(&dir).unwrap();
}
