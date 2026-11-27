//! The relay and the frame together, with a stand-in harness: a hook's input reaches `Next-Moment` whole, `Apply`'s
//! answer reaches the hook, a late one is ready by its line's place, and one the harness has none for is a failure.

use docket_hooks::frame::{Frame, Native, Out, ToolUse};
use docket_hooks::Action;
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::value::Value;
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// A stand-in harness: two hooks, its answer the action's name and text, exit 2 for a Deny, none for KeepGoing.
struct StandIn;

impl Native for StandIn {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("stand-in"), Value::text("1")]
    }
    fn hooks(&self) -> Vec<(&'static str, &'static str)> {
        vec![("BeforeTool", "tool_called"), ("Done", "turn_end")]
    }
    fn answer(&self, _: &str, a: &Action) -> Result<Out, String> {
        Ok(match a {
            Action::KeepGoing(_) => return Err("it cannot keep going".into()),
            Action::Allow => Out { stdout: String::new(), code: 0 },
            Action::Deny(r) => Out { stdout: format!("deny:{r}"), code: 2 },
            other => Out { stdout: format!("{other:?}"), code: 0 },
        })
    }
    fn tool_use(&self, input: &str) -> ToolUse {
        ToolUse::command(input)
    }
}

fn hook(sock: &std::path::Path, name: &str, input: &str) -> Child {
    let mut c = Command::new(env!("CARGO_BIN_EXE_docket-hook-relay"))
        .arg(name)
        .env("DOCKET_HOOK_SOCKET", sock)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    c
}

fn ended(c: Child) -> (String, i32) {
    let o = c.wait_with_output().unwrap();
    (String::from_utf8_lossy(&o.stdout).into_owned(), o.status.code().unwrap_or(-1))
}

/// The late answers as soon as there are any, within five seconds: the test stands in for the daemon, which waits
/// on `waits()`.
fn late(f: &mut Frame<StandIn>) -> Vec<(u64, Reply)> {
    for _ in 0..500 {
        let r = f.ready();
        if !r.is_empty() {
            return r;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("nothing reached the socket")
}

/// The lines of late answers.
fn lines(r: Vec<(u64, Reply)>) -> Vec<u64> {
    r.into_iter().map(|(l, _)| l).collect()
}

fn now(value: Value) -> Reply {
    Reply::Answer { value, costs: vec![], events: vec![] }
}

fn next(f: &mut Frame<StandIn>, who: &Value, line: u64) -> Reply {
    f.carry("std.hooks.Next-Moment", std::slice::from_ref(who), line)
}

fn apply(f: &mut Frame<StandIn>, moment: &str, v: Value) -> Reply {
    f.carry("std.hooks.Apply", &[agent(), Value::text(moment), v], 0)
}

fn agent() -> Value {
    Value::text("ann")
}

fn dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("docket-hooks-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_hooks_input_reaches_next_moment_whole_by_its_line_and_the_answer_reaches_the_hook() {
    let d = dir("one");
    let mut f = Frame::new(StandIn);
    let sock = f.open("ann", &d).unwrap();
    assert_eq!(f.waits().len(), 1, "the daemon waits on its socket");
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::metadata(&sock).unwrap().permissions().mode() & 0o777, 0o600, "the socket is its user's alone");
    assert_eq!(next(&mut f, &agent(), 41), Reply::Later, "nothing reached yet: the run rests");
    let input = "{\"tool\":\"ls\",\n\"x\":\"\u{e9}\"}";
    let h = hook(&sock, "BeforeTool", input);
    let want = Value::Record { ty: "std.hooks.Reached".into(), fields: vec![("moment".into(), Value::text("tool_called")), ("payload".into(), Value::text(input))] };
    assert_eq!(late(&mut f), vec![(41, now(want))], "its line's place, its moment, its input whole");
    assert_eq!(apply(&mut f, "tool_called", Action::Deny("not now".into()).value()), now(Value::Unit));
    assert_eq!(ended(h), ("deny:not now".into(), 2), "the harness's own answer and code, as the relay printed them");
    // a value that is no HookAction fails and the moment stays, for a right one
    assert_eq!(next(&mut f, &agent(), 42), Reply::Later);
    let h = hook(&sock, "BeforeTool", "{}");
    late(&mut f);
    assert!(matches!(apply(&mut f, "tool_called", Value::text("Deny")), Reply::Failed(_)));
    assert_eq!(apply(&mut f, "tool_called", Action::Allow.value()), now(Value::Unit));
    assert_eq!(ended(h), (String::new(), 0), "answered by the right one");
    match f.carry("std.hooks.Tool-Use", &[agent(), Value::text("ls")], 0) {
        Reply::Answer { value: Value::Record { ty, fields }, .. } => assert_eq!((ty.as_str(), &fields[1].1), ("std.hooks.ToolUse", &Value::text("ls"))),
        other => panic!("{other:?}"),
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_answer_the_harness_has_none_for_fails_and_the_hook_goes_on_as_allow_lets_it() {
    let d = dir("none");
    let mut f = Frame::new(StandIn);
    let sock = f.open("ann", &d).unwrap();
    // asked after the hook ran: answered at once when it has reached the socket, else by its line once it does
    let h = hook(&sock, "Done", "{}");
    match next(&mut f, &agent(), 1) {
        Reply::Later => assert_eq!(lines(late(&mut f)), [1]),
        got => assert!(matches!(got, Reply::Answer { .. }), "{got:?}"),
    }
    assert_eq!(apply(&mut f, "turn_end", Action::KeepGoing("more".into()).value()), Reply::Failed("it cannot keep going".into()));
    assert_eq!(ended(h), (String::new(), 0), "the hook got Allow's answer");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_hook_no_bridge_names_is_let_go_and_a_suspended_one_waits_for_a_later_apply() {
    let d = dir("held");
    let mut f = Frame::new(StandIn);
    let sock = f.open("ann", &d).unwrap();
    let mut other = hook(&sock, "Notification", "{}");
    for _ in 0..500 {
        f.ready();
        if other.try_wait().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(ended(other), (String::new(), 0), "no moment: it goes on as it would have");
    assert_eq!(next(&mut f, &agent(), 2), Reply::Later);
    let mut h = hook(&sock, "BeforeTool", "{}");
    late(&mut f);
    assert_eq!(apply(&mut f, "tool_called", Action::Suspend("e1".into()).value()), now(Value::Unit));
    // time for an answer that should not come
    std::thread::sleep(Duration::from_millis(200));
    assert!(h.try_wait().unwrap().is_none(), "held: the harness still waits");
    assert_eq!(apply(&mut f, "tool_called", Action::Deny("later".into()).value()), now(Value::Unit));
    assert_eq!(ended(h), ("deny:later".into(), 2));
    // a session that ends lets every waiting moment go as Allow lets it
    let bob = Value::text("bob");
    let sock2 = f.open("bob", &d).unwrap();
    assert_eq!(next(&mut f, &bob, 3), Reply::Later);
    let h2 = hook(&sock2, "BeforeTool", "{}");
    assert_eq!(lines(late(&mut f)), [3]);
    f.close("bob");
    assert_eq!(ended(h2), (String::new(), 0));
    assert!(!sock2.exists(), "its socket removed");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_agent_with_no_session_fails_and_a_reopened_one_answers_on_its_new_socket_alone() {
    let d = dir("reopen");
    let mut f = Frame::new(StandIn);
    assert!(matches!(next(&mut f, &agent(), 1), Reply::Failed(_)));
    f.open("ann", &d).unwrap();
    let sock = f.open("ann", &d).unwrap();
    assert_eq!(f.waits().len(), 1, "the old socket is gone");
    assert_eq!(next(&mut f, &agent(), 2), Reply::Later);
    let h = hook(&sock, "BeforeTool", "{}");
    assert_eq!(lines(late(&mut f)), [2]);
    assert_eq!(apply(&mut f, "tool_called", Action::Allow.value()), now(Value::Unit));
    assert_eq!(ended(h), (String::new(), 0));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_connection_with_no_line_yet_holds_nothing_and_a_hook_that_gave_up_was_not_applied() {
    let d = dir("slow");
    let mut f = Frame::new(StandIn);
    let sock = f.open("ann", &d).unwrap();
    assert_eq!(next(&mut f, &agent(), 1), Reply::Later);
    assert_eq!(next(&mut f, &agent(), 2), Reply::Later);
    // a relay that has connected and written half its line
    let mut slow = std::os::unix::net::UnixStream::connect(&sock).unwrap();
    slow.write_all(br#"{"hook":"BeforeTool","#).unwrap();
    let begun = std::time::Instant::now();
    assert!(f.ready().is_empty());
    assert!(begun.elapsed() < Duration::from_millis(100), "ready did not wait for it");
    assert_eq!(f.waits().len(), 2, "its connection is waited on beside the socket");
    // another hook goes ahead of it, and the slow one is read when the rest comes
    let mut h = hook(&sock, "BeforeTool", "{}");
    assert_eq!(lines(late(&mut f)), [1]);
    slow.write_all(b"\"input\":\"x\"}\n").unwrap();
    assert_eq!(lines(late(&mut f)), [2]);
    // the first hook gives up before its answer: the Deny never reached the harness
    h.kill().unwrap();
    h.wait().unwrap();
    assert!(matches!(apply(&mut f, "tool_called", Action::Deny("no".into()).value()), Reply::Failed(_)));
    assert_eq!(apply(&mut f, "tool_called", Action::Allow.value()), now(Value::Unit), "the slow one is answered");
    let mut got = String::new();
    std::io::BufRead::read_line(&mut std::io::BufReader::new(&slow), &mut got).unwrap();
    assert!(got.contains("\"code\":0"), "{got}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_line_dropped_is_given_no_hook_and_the_next_waiting_one_gets_it() {
    let d = dir("dropped");
    let mut f = Frame::new(StandIn);
    let sock = f.open("ann", &d).unwrap();
    assert_eq!(next(&mut f, &agent(), 1), Reply::Later);
    assert_eq!(next(&mut f, &agent(), 2), Reply::Later);
    f.dropped(1);
    let h = hook(&sock, "BeforeTool", "{}");
    assert_eq!(lines(late(&mut f)), [2], "the cancelled run's line got nothing");
    assert_eq!(apply(&mut f, "tool_called", Action::Allow.value()), now(Value::Unit));
    assert_eq!(ended(h), (String::new(), 0));
    // with no line left, a hook waits for the next Next-Moment
    f.dropped(3);
    assert_eq!(next(&mut f, &agent(), 4), Reply::Later);
    f.dropped(4);
    let h = hook(&sock, "BeforeTool", "{}");
    // time for the relay to start and write its line
    std::thread::sleep(Duration::from_millis(200));
    assert!(f.ready().is_empty(), "nothing went to a dropped line");
    let got = next(&mut f, &agent(), 5);
    assert!(matches!(got, Reply::Answer { .. }), "kept for the next Next-Moment: {got:?}");
    assert_eq!(apply(&mut f, "tool_called", Action::Allow.value()), now(Value::Unit));
    assert_eq!(ended(h), (String::new(), 0));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn with_no_driver_the_relay_prints_nothing_and_fails_so_the_harness_goes_on() {
    let h = hook(std::path::Path::new("/nonexistent/docket.hooks"), "BeforeTool", "{}");
    let o = h.wait_with_output().unwrap();
    assert_eq!((o.stdout.is_empty(), o.status.code()), (true, Some(1)));
    assert!(String::from_utf8_lossy(&o.stderr).contains("/nonexistent/docket.hooks"));
}

