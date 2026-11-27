//! The driver over this host's compute driver: answers by line, failures, stops and the bound on an answer.

use super::*;

fn local(name: &str, at_once: usize) -> (Compute, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("driver-compute-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&dir));
    let c = Config::read(&format!("[job]\nlocal = {:?}\nnetwork_off = false\nat_once = {at_once}\nenv = {{ PATH = \"/usr/bin:/bin\" }}\n", dir.display().to_string())).unwrap();
    (Compute::new(c).unwrap(), dir)
}

fn answers(c: &mut Compute, n: usize) -> Vec<(u64, Reply)> {
    let mut got = Vec::new();
    for _ in 0..1000 {
        got.extend(c.ready());
        if got.len() >= n {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    got.sort_by_key(|(l, _)| *l);
    got
}

fn running(mark: &str) -> bool {
    std::fs::read_dir("/proc").unwrap().filter_map(|e| std::fs::read(e.ok()?.path().join("cmdline")).ok()).any(|c| String::from_utf8_lossy(&c).contains(mark))
}

#[test]
fn a_command_is_answered_later_by_its_line_with_its_output_or_its_failure() {
    let (mut c, dir) = local("cmd", 2);
    assert_eq!(c.carry(COMMAND, &[Value::text("echo hi")], 7), Reply::Later);
    assert_eq!(c.carry(COMMAND, &[Value::text("echo no >&2; exit 2")], 8), Reply::Later);
    assert_eq!(c.carry(COMMAND, &[Value::text("head -c 70000 /dev/zero")], 9), Reply::Later);
    let got = answers(&mut c, 3);
    assert_eq!(got[0], (7, Reply::Answer { value: Value::text("hi\n"), costs: Vec::new(), events: Vec::new() }));
    assert_eq!(got[1], (8, Reply::Failed("exit 2: no".into())));
    assert!(matches!(&got[2], (9, Reply::Failed(w)) if w.starts_with("stdout of 70000 bytes, over the 65536")), "{:?}", got[2]);
    assert!(matches!(c.carry("std.x.Other", &[], 10), Reply::Failed(_)));
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn a_measure_is_answered_with_the_one_number_its_line_prints() {
    let (mut c, dir) = local("measure", 2);
    assert_eq!(c.carry(MEASURE, &[Value::text("echo ' 42'")], 1), Reply::Later);
    assert_eq!(c.carry(MEASURE, &[Value::text("echo 4 2")], 2), Reply::Later);
    let got = answers(&mut c, 2);
    assert_eq!(got[0], (1, Reply::Answer { value: Value::Int(42), costs: Vec::new(), events: Vec::new() }));
    assert_eq!(got[1], (2, Reply::Failed("printed \"4 2\", not one number".into())));
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn a_dropped_line_stops_its_job_and_gets_no_answer() {
    let (mut c, dir) = local("drop", 1);
    assert_eq!(c.carry(COMMAND, &[Value::text("sleep 44.517 & sleep 44.518")], 3), Reply::Later);
    assert_eq!(c.carry(COMMAND, &[Value::text("echo after")], 4), Reply::Later);
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(running("44.518"), "one worker, the first job running");
    c.dropped(3);
    let got = answers(&mut c, 1);
    assert_eq!(got, [(4, Reply::Answer { value: Value::text("after\n"), costs: Vec::new(), events: Vec::new() })]);
    assert!(!running("44.517") && !running("44.518"), "its processes ended");
    c.dropped(4);
    assert!(c.running.is_empty(), "a line dropped after its answer leaves nothing behind");
    drop(std::fs::remove_dir_all(&dir));
}
