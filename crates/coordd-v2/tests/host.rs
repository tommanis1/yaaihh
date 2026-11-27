//! The host driver linked into a machine: its clock's Events are performed with the texts' types, and Pick and
//! Pick-Other ask the machine's facts by their qualified names.

use coordd_v2::docket::Docket;
use coordd_v2::link::{link, Ready, Waits};
use coordd_v2::machine::Machine as _;
use docket_driver_host::{Config, Host, Named, Seen, PICK, PICK_OTHER};
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::machine::Machine;
use docket_v2::run::value::Value;
use std::cell::{OnceCell, RefCell};
use std::rc::Rc;
use std::time::Duration;

fn blind() -> Seen {
    Seen { live: Box::new(Vec::new), holds: Box::new(|_, _| false) }
}

#[test]
fn the_hosts_tick_and_a_past_time_are_performed_by_the_machine() {
    let dir = std::env::temp_dir().join(format!("coordd-v2-host-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&dir));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("times"), "2026-11-26T10:00Z\n").unwrap();
    let texts = [
        "module std.comms\nFact Key Identified by String",
        "module std.budgets\nFact Now(minute: Int)\nEvent Tick(minute: Int)\n  Terminates Now(m) When Now(m)\n  Creates Now(minute)",
        "module yaaihh.brain\nimport std.comms as C\nFact Past(time: C.Key)\nEvent Time-Passed(time: C.Key)\n  Creates Past(time)",
    ];
    let loaded = docket_v2::check::load(texts.iter().map(|t| docket_v2::syntax::read(t).unwrap()).collect()).unwrap();
    let (d, host) = link(Host::new(Config { times: Some(Named { event: "yaaihh.brain.Time-Passed".into(), from: dir.join("times").display().to_string() }), ..Config::default() }, Duration::from_secs(3600), blind()).unwrap());
    let mut m = Machine::new(loaded, docket_v2::norm::Core::default(), vec![d]).unwrap();
    let mut seen = Vec::new();
    for _ in 0..300 {
        seen.extend(Waits::ready(&mut *host.borrow_mut()).into_iter().filter_map(|r| if let Ready::Event(ev) = r { Some(ev) } else { None }));
        if seen.len() >= 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    for ev in seen {
        m.event(0, ev);
    }
    m.run();
    let lines: Vec<String> = m.record.iter().map(|l| l.to_string()).collect();
    assert!(lines.iter().any(|l| l.starts_with("performed(std.budgets.Tick(")), "{lines:?}");
    assert!(lines.iter().any(|l| l.starts_with("performed(yaaihh.brain.Time-Passed(")), "{lines:?}");
    assert!(!lines.iter().any(|l| l.starts_with("refused(")), "{lines:?}");
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn a_pick_asks_the_role_of_each_live_party_in_the_machines_facts() {
    let texts = [
        "module std.comms\nFact Role(who: String, role: String)\nEffect Pick(role: String) -> String\n+Role(\"w2\", \"looker\").",
        "module yaaihh.brain\nimport std.comms as C\nFact Past(time: String)",
    ];
    let loaded = docket_v2::check::load(texts.iter().map(|t| docket_v2::syntax::read(t).unwrap()).collect()).unwrap();
    let asked: Rc<OnceCell<Rc<RefCell<Docket<docket_v2::norm::Core>>>>> = Rc::new(OnceCell::new());
    let a = asked.clone();
    let holds = Box::new(move |m: &str, q: &str| a.get().is_some_and(|d| d.borrow_mut().holds(m, q) == Ok(true)));
    let seen = Seen { live: Box::new(|| vec!["w1".into(), "w2".into()]), holds };
    let (d, host) = link(Host::new(Config::default(), Duration::from_secs(3600), seen).unwrap());
    let runs = Machine::new(loaded, docket_v2::norm::Core::default(), vec![d]).unwrap();
    let docket = Docket::new(runs, "yaaihh.brain", vec![], Box::new(|_, _| (None, String::new())), Box::new(|_| Err("this record starts empty".into())));
    drop(asked.set(Rc::new(RefCell::new(docket))));
    assert_eq!(host.borrow_mut().carry(PICK, &[Value::text("looker")], 9), Reply::Later);
    let got: Vec<Ready> = Waits::ready(&mut *host.borrow_mut()).into_iter().filter(|r| matches!(r, Ready::Answer(..))).collect();
    assert!(matches!(&got[..], [Ready::Answer(9, Reply::Answer { value, .. })] if *value == Value::text("w2")), "w2 holds it and w1 does not");
}

#[test]
fn a_pick_of_another_family_asks_the_families_the_machine_holds() {
    let texts = [
        "module std.comms\nFact Role(who: String, role: String)\n+Role(\"lk1\", \"looker\"). +Role(\"lk2\", \"looker\").",
        "module std.rails\nFact Family Identified by String\nFact Of-Family(who: String, family: Family)\nFact Other-Family(a: String, b: String)\n  Holds when Of-Family(a, f) && Of-Family(b, g) && f != g\nEffect Pick-Other(role: String, than: List<String>) -> String\n+Family(\"anthropic\"). +Family(\"openai\").\n+Of-Family(\"lk1\", \"anthropic\"). +Of-Family(\"lk2\", \"openai\"). +Of-Family(\"w1\", \"anthropic\").",
    ];
    let loaded = docket_v2::check::load(texts.iter().map(|t| docket_v2::syntax::read(t).unwrap()).collect()).unwrap();
    let asked: Rc<OnceCell<Rc<RefCell<Docket<docket_v2::norm::Core>>>>> = Rc::new(OnceCell::new());
    let a = asked.clone();
    let holds = Box::new(move |m: &str, q: &str| a.get().is_some_and(|d| d.borrow_mut().holds(m, q) == Ok(true)));
    let seen = Seen { live: Box::new(|| vec!["lk1".into(), "lk2".into()]), holds };
    let (d, host) = link(Host::new(Config::default(), Duration::from_secs(3600), seen).unwrap());
    let runs = Machine::new(loaded, docket_v2::norm::Core::default(), vec![d]).unwrap();
    let docket = Docket::new(runs, "std.rails", vec![], Box::new(|_, _| (None, String::new())), Box::new(|_| Err("this record starts empty".into())));
    drop(asked.set(Rc::new(RefCell::new(docket))));
    assert_eq!(host.borrow_mut().carry(PICK_OTHER, &[Value::text("looker"), Value::List(vec![Value::text("w1")])], 4), Reply::Later);
    let got: Vec<Ready> = Waits::ready(&mut *host.borrow_mut()).into_iter().filter(|r| matches!(r, Ready::Answer(..))).collect();
    assert!(matches!(&got[..], [Ready::Answer(4, Reply::Answer { value, .. })] if *value == Value::text("lk2")), "lk1 is w1's family, lk2 is not");
}
