//! An Event over the socket: performed for the party of a `--by-socket` driver, refused for any other party.

use coordd_v2::docket::Docket;
use coordd_v2::hand::Hand;
use coordd_v2::machine::Machine;
use coordd_v2::peer::Party;
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::value::Value;
use serde_json::json;

/// A driver linked into the daemon that names the same Event.
struct Linked;

impl Driver for Linked {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("linked")]
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        vec![]
    }
    fn events(&self) -> Vec<String> {
        vec!["t.Ping".into()]
    }
    fn carry(&mut self, op: &str, _: &[Value], _: u64) -> Reply {
        Reply::Failed(format!("{op} is not carried here"))
    }
    fn dropped(&mut self, _: u64) {}
}

#[test]
fn only_the_party_of_a_by_socket_driver_performs_an_event_over_the_socket() {
    let text = "module t\nEvent Ping(n: Int)\nAct Go(by: String)";
    let loaded = docket_v2::check::load(vec![docket_v2::syntax::read(text).unwrap()]).unwrap();
    let hand = Hand { name: "hand".into(), effects: vec![], events: vec!["t.Ping".into()] };
    let runs = docket_v2::run::machine::Machine::new(loaded, docket_v2::norm::Core::default(), vec![Box::new(Linked), Box::new(hand)]).unwrap();
    let mut d = Docket::new(runs, "t", vec![1], Box::new(|_, _| (None, String::new())), Box::new(|_| Err("this record starts empty".into())));
    assert_eq!(d.act(&Party::of("i", Some(1)), "t.Ping", &[json!(1)]), Ok(json!({"event": "performed"})));
    for p in [Party::of("w", Some(0)), Party::of("x", None)] {
        let got = d.act(&p, "t.Ping", &[json!(2)]);
        assert!(got.as_ref().is_err_and(|e| e.contains("no `--by-socket` driver")), "{}: {got:?}", p.name);
    }
    let lines: Vec<_> = d.written().into_iter().filter(|l| l.get("performed").is_some() || l.get("refused").is_some()).collect();
    assert_eq!(lines.len(), 1, "a refused party writes nothing: {lines:?}");
}
