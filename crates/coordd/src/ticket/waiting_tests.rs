//! A failed build strands nothing (paper-research N-101 post 166, docket-core3's C-225 gap 2): the record's factors
//! `waiting-K` and `oldest-waiting-K` (docket-machine `when_asks`) count what no build that closed done carried, so a
//! cut rule on them cuts the next build once its time is up, with no other piece closed after the failure.

use docket_machine::{Defs, Input, Machine, Status, Text};
use serde_json::json;
use std::collections::BTreeMap;

const KINDS: &str = "kind bit\n  letter Z\n  form task: Text\n  result Text\n  pattern\n    return task\n\nkind batch\n  letter Y\n  form cut: Text\n  result Text\n  raises Bad, Stalled\n  pattern\n    c <- step runner \"Cut\" -> Cut(carries: [Ticket]);\n    v <- step runner \"Cases\" -> Green | Red(log: Text);\n    match v {\n      Green => return cut\n      Red(log) => raise Bad(log)\n    }\n\nwhen cut-build is build open batch(\"line\")\n";

/// cut.eflint's rules 1, 2 and 4 (tovrin-writer's draft), on the waiting factors.
const POLICY: &str = "Fact cut-option Identified by String\n+cut-option(\"build\").\n+cut-option(\"wait\").\nDecision cut-build\n  Options cut-option\nPrefer for cut-build When call.oldest-waiting-bit >= 120:\n  option == \"build\" > option == \"wait\"\nPrefer for cut-build:\n  option == \"wait\" > option == \"build\"\nOnly for cut-build When call.open >= 1:\n  option == \"wait\"\n";

#[test]
fn after_a_failed_build_the_next_ask_cuts_again_once_the_time_is_up() {
    use std::io::{Read, Write};
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let dir = std::env::temp_dir().join(format!("docket-waiting-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("policy")).unwrap();
    std::fs::write(dir.join("policy/cut.eflint"), POLICY).unwrap();
    // a decision is made with the proxy's models: a stand-in that serves none
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            let mut buf = [0u8; 65536];
            let _ = s.read(&mut buf);
            let body = r#"{"data": []}"#;
            let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
        }
    });
    crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = Some(url));
    let places = crate::policy::Places { base, project: Some(dir.display().to_string()), coord: dir.display().to_string(), ..Default::default() };
    let engine = crate::policy::Engine::start(places);

    let (defs, errors) = Defs::build(&[("carry.kind".to_string(), KINDS.to_string())], BTreeMap::new(), BTreeMap::new(), 200);
    assert!(errors.is_empty(), "{errors:?}");
    let mut m = Machine::load(defs);
    let go = |m: &mut Machine, i: Input, at: &str| m.apply(i, at.to_string()).reply.unwrap();
    for task in ["a", "b"] {
        go(&mut m, Input::Open { kind: "bit".into(), under: None, opener: "dev".into(), form: json!({"task": task}), text: None, names: vec!["lead".into()], told: vec![] }, "2026-11-16T12:00:00Z");
    }
    // what coordd's clock does each minute: the factors asked of the policy; build fires the binding
    let ask = |m: &Machine, now: &str| {
        let (b, d, _, f) = m.when_asks(now).remove(0);
        (b, engine.decide(&d, f).unwrap().chosen)
    };
    let (b, chosen) = ask(&m, "2026-11-16T14:00:00Z");
    assert_eq!(chosen.as_deref(), Some("build"));
    go(&mut m, Input::Event { name: "when".into(), value: json!({"binding": b, "answer": "build", "decision": {"chosen": "build"}}) }, "2026-11-16T14:00:00Z");
    let answer = |m: &mut Machine, t: u64, head: serde_json::Value, at: &str| m.apply(Input::Answer { t, by: "runner".into(), head, text: Text::New("as said".into()) }, at.to_string()).reply.unwrap();
    answer(&mut m, 202, json!({"head": "Cut", "carries": ["Z-200", "Z-201"]}), "2026-11-16T14:01:00Z");
    assert_eq!(ask(&m, "2026-11-16T14:30:00Z").1.as_deref(), Some("wait"), "one build at a time");
    // the build fails; no other bit closes; the next ask builds again (its two bits have waited since 12:00)
    answer(&mut m, 202, json!({"head": "Red", "log": "case-1"}), "2026-11-16T14:40:00Z");
    assert!(matches!(m.tickets[&202].status, Status::Failed(_)));
    let (_, d, _, f) = m.when_asks("2026-11-16T14:41:00Z").remove(0);
    assert!(!f.contains_key("closed-bit"), "nothing closed since the cut: the old factor would have waited for good");
    assert_eq!(engine.decide(&d, f).unwrap().chosen.as_deref(), Some("build"));
    crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
    let _ = std::fs::remove_dir_all(&dir);
}
