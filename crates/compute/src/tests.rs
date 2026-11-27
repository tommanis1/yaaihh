//! The drivers against a mock REAPI server and on this host.

use crate::mock::{self, Shared};
use crate::{tree, ComputeDriver, Digest, Endpoint, File, JobSpec, Limits, LocalFallbackDriver, RemoteDriver, Stream, WarmLayerHandle};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::process::Command;
use std::sync::Arc;

/// The mock server on a runtime of its own, as a real server is apart from the driver's.
fn server() -> (u16, Shared) {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            tx.send(mock::start().await).unwrap();
            std::future::pending::<()>().await
        })
    });
    rx.recv().unwrap()
}

fn go() -> AtomicBool {
    AtomicBool::new(false)
}

/// Whether a process whose command line holds `mark` is running.
fn running(mark: &str) -> bool {
    std::fs::read_dir("/proc").unwrap().filter_map(|e| std::fs::read(e.ok()?.path().join("cmdline")).ok()).any(|c| String::from_utf8_lossy(&c).contains(mark))
}

fn file(path: &str, data: &str, executable: bool) -> File {
    File { path: path.into(), data: data.as_bytes().to_vec(), executable }
}

fn job(tree: Digest, cmd: &[&str]) -> JobSpec {
    let env = BTreeMap::from([("PATH".to_string(), "/usr/bin:/bin".to_string())]);
    JobSpec { rootfs_digest: None, input_tree_digest: tree, cmd: cmd.iter().map(|c| c.to_string()).collect(), env, limits: Limits { wall_ms: Some(60_000), ..Limits::default() }, network_off: false }
}

#[test]
fn a_miss_executes_with_its_output_live_and_the_same_job_then_comes_from_the_action_cache() {
    let (port, st) = server();
    let ep = Endpoint { name: "mock".into(), url: format!("grpc://127.0.0.1:{port}"), instance: "main".into(), ca: None, cert: None, key: None };
    let mut d = RemoteDriver::new(ep).unwrap();
    let (root, blobs) = tree(&[file("in.txt", "hello", false)]).unwrap();
    assert_eq!(d.upload(&blobs).unwrap().last(), Some(&root));
    assert_eq!(d.upload(&blobs).unwrap().len(), 2);
    assert_eq!(st.0.lock().unwrap().writes, 2, "a blob the store holds is not written again");
    let j = JobSpec { network_off: true, ..job(root, &["make", "all"]) };

    let mut live = Vec::new();
    let first = d.run(&j, &mut |s, b| live.push((s, String::from_utf8_lossy(b).into_owned())), &go()).unwrap();
    assert!(!first.cached && first.exit_code == 0);
    assert_eq!(live, [(Stream::Stdout, "ran ".to_string()), (Stream::Stdout, "make all\n".to_string())]);
    let s = st.0.lock().unwrap();
    assert!(s.followed_live, "the output was read while the job ran, not after");
    assert_eq!((s.executes, s.writes), (1, 5), "the command, the action and out/'s root directory uploaded");
    drop(s);
    assert_eq!(d.download(&first.stdout_digest).unwrap(), b"ran make all\n");
    let out = scratch("mock-out");
    crate::tree::materialize(first.output_tree_digest.as_ref().unwrap(), &mut |x| d.download(x), &out).unwrap();
    assert_eq!(std::fs::read(out.join("said")).unwrap(), b"ran make all\n", "out/ given as a Tree comes back as its root directory");
    drop(std::fs::remove_dir_all(&out));

    let mut again = Vec::new();
    let second = d.run(&j, &mut |s, b| again.push((s, b.to_vec())), &go()).unwrap();
    assert!(second.cached && again.is_empty());
    assert_eq!((&second.stdout_digest, &second.output_tree_digest, second.exit_code), (&first.stdout_digest, &first.output_tree_digest, 0));
    let s = st.0.lock().unwrap();
    assert_eq!((s.executes, s.writes), (1, 5), "nothing executed or written");
    drop(s);
    eprintln!("miss {} ms, hit {} ms", first.wall_ms, second.wall_ms);

    let other = JobSpec { network_off: false, ..j };
    assert!(!d.run(&other, &mut |_, _| {}, &go()).unwrap().cached, "a job differing in its platform is another action");
    assert!(d.download(&Digest::of(b"never stored")).is_err());
}

#[test]
fn an_endpoint_that_does_not_answer_is_an_error_not_a_hang() {
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let ep = Endpoint { name: "gone".into(), url: format!("grpc://127.0.0.1:{port}"), instance: String::new(), ca: None, cert: None, key: None };
    assert!(RemoteDriver::new(ep).unwrap().upload(&[b"x".to_vec()]).is_err());
    let bad = Endpoint { name: "bad".into(), url: "http://h:1".into(), instance: String::new(), ca: None, cert: None, key: None };
    assert!(RemoteDriver::new(bad).unwrap().upload(&[]).unwrap_err().contains("grpc://"));
}

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("compute-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&d));
    d
}

#[test]
fn this_hosts_driver_runs_a_job_once_keeps_its_outputs_and_caches_only_a_success() {
    let dir = scratch("local");
    let mut d = LocalFallbackDriver::new(&dir).unwrap();
    let script = "cat in.txt; echo err >&2; mkdir -p out/sub; tr a-z A-Z < in.txt > out/sub/up.txt";
    let (root, blobs) = tree(&[file("in.txt", "hello", false), file("run.sh", script, true)]).unwrap();
    d.upload(&blobs).unwrap();
    let j = job(root.clone(), &["/bin/sh", "run.sh"]);
    let mut live = Vec::new();
    let first = d.run(&j, &mut |s, b| live.push((s, b.to_vec())), &go()).unwrap();
    assert!(!first.cached && first.exit_code == 0);
    assert!(live.contains(&(Stream::Stdout, b"hello".to_vec())) && live.contains(&(Stream::Stderr, b"err\n".to_vec())), "{live:?}");
    let out = scratch("local-out");
    crate::tree::materialize(first.output_tree_digest.as_ref().unwrap(), &mut |x| d.download(x), &out).unwrap();
    assert_eq!(std::fs::read_to_string(out.join("sub/up.txt")).unwrap(), "HELLO");
    let second = d.run(&j, &mut |_, _| panic!("a cached job prints nothing"), &go()).unwrap();
    assert!(second.cached && second.output_tree_digest == first.output_tree_digest);

    let fails = job(root.clone(), &["/bin/sh", "-c", "exit 3"]);
    assert_eq!(d.run(&fails, &mut |_, _| {}, &go()).unwrap().exit_code, 3);
    assert!(!d.run(&fails, &mut |_, _| {}, &go()).unwrap().cached, "a failure is run again");
    let slow = JobSpec { limits: Limits { wall_ms: Some(200), ..Limits::default() }, ..job(root.clone(), &["/bin/sleep", "5"]) };
    assert!(d.run(&slow, &mut |_, _| {}, &go()).unwrap_err().contains("over its 200 ms"));
    let capped = JobSpec { limits: Limits { memory_bytes: Some(1 << 30), ..Limits::default() }, ..j.clone() };
    assert!(d.run(&capped, &mut |_, _| {}, &go()).unwrap_err().contains("memory"), "a limit it cannot keep is refused");
    let with_root = JobSpec { rootfs_digest: Some(Digest::of(b"r")), ..j };
    assert!(d.run(&with_root, &mut |_, _| {}, &go()).is_err(), "a root filesystem wants a worker");
    drop((std::fs::remove_dir_all(&dir), std::fs::remove_dir_all(&out)));
}

#[test]
fn a_job_with_the_network_off_sees_only_loopback() {
    let dir = scratch("net");
    let mut d = LocalFallbackDriver::new(&dir).unwrap();
    let j = JobSpec { network_off: true, ..job(Digest::of(b""), &["/bin/cat", "/proc/net/dev"]) };
    let mut said = Vec::new();
    assert_eq!(d.run(&j, &mut |_, b| said.extend_from_slice(b), &go()).unwrap().exit_code, 0);
    let names: Vec<String> = String::from_utf8(said).unwrap().lines().skip(2).filter_map(|l| l.split(':').next()).map(|n| n.trim().to_string()).collect();
    assert_eq!(names, ["lo"]);
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn a_warm_layer_starts_each_call_from_what_the_last_one_left() {
    let dir = scratch("warm");
    let d = LocalFallbackDriver::new(&dir).unwrap();
    let mut w = WarmLayerHandle::new(d, job(Digest::of(b""), &[]));
    let sh = |c: &str| ["/bin/sh".to_string(), "-c".to_string(), c.to_string()];
    assert_eq!(w.call(&sh("echo 1 > out/n"), &mut |_, _| {}).unwrap().exit_code, 0);
    let mut said = Vec::new();
    w.call(&sh("cat n; cp n out/n"), &mut |_, b| said.extend_from_slice(b)).unwrap();
    assert_eq!(said, b"1\n");
    assert!(w.call(&sh("exit 1"), &mut |_, _| {}).is_ok());
    let mut again = Vec::new();
    w.call(&sh("cat n; true"), &mut |_, b| again.extend_from_slice(b)).unwrap();
    assert_eq!(again, b"1\n", "a failed call leaves the layer as it was");
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn a_job_over_its_time_or_stopped_ends_with_everything_it_started() {
    let dir = scratch("ends");
    let mut d = LocalFallbackDriver::new(&dir).unwrap();
    let slow = JobSpec { limits: Limits { wall_ms: Some(300), ..Limits::default() }, ..job(Digest::of(b""), &["/bin/sh", "-c", "sleep 41.517 & sleep 41.518"]) };
    assert!(d.run(&slow, &mut |_, _| {}, &go()).unwrap_err().contains("over its"));
    assert!(!running("41.517") && !running("41.518"), "the sleeper it started is gone too");
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        s.store(true, Ordering::Relaxed);
    });
    let stopped = job(Digest::of(b""), &["/bin/sh", "-c", "sleep 42.517 & sleep 42.518"]);
    assert_eq!(d.run(&stopped, &mut |_, _| {}, &stop).unwrap_err(), "stopped");
    assert!(!running("42.517") && !running("42.518"));
    let left = job(Digest::of(b""), &["/bin/sh", "-c", "sleep 43.517 & echo done"]);
    assert_eq!(d.run(&left, &mut |_, _| {}, &go()).unwrap().exit_code, 0, "a job that leaves a process behind still ends");
    assert!(!running("43.517"));
    let closed = |mark: &str| job(Digest::of(b""), &["/bin/sh", "-c", &format!("exec >&- 2>&-; sleep {mark}")]);
    let timed = JobSpec { limits: Limits { wall_ms: Some(300), ..Limits::default() }, ..closed("46.517") };
    assert!(d.run(&timed, &mut |_, _| {}, &go()).unwrap_err().contains("over its"), "a job with its output closed is still timed");
    assert!(!running("46.517"));
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        s.store(true, Ordering::Relaxed);
    });
    assert_eq!(d.run(&closed("47.517"), &mut |_, _| {}, &stop).unwrap_err(), "stopped", "and still stopped");
    assert!(!running("47.517"));
    let apart = JobSpec { limits: Limits::default(), ..job(Digest::of(b""), &["/bin/sh", "-c", "setsid sleep 45.517 & echo done"]) };
    assert_eq!(d.run(&apart, &mut |_, _| {}, &go()).unwrap().exit_code, 0, "output held by a process apart from the job does not keep it");
    for p in std::fs::read_dir("/proc").unwrap().filter_map(|e| e.ok()) {
        if std::fs::read(p.path().join("cmdline")).is_ok_and(|c| String::from_utf8_lossy(&c).contains("45.517")) {
            drop(Command::new("kill").arg(p.file_name()).status());
        }
    }
    drop(std::fs::remove_dir_all(&dir));
}

/// Against a real endpoint, the file at `COMPUTE_TOML` naming it; run with `--ignored`.
#[test]
#[ignore]
fn against_a_real_endpoint_a_miss_executes_and_the_same_job_then_comes_from_the_action_cache() {
    let cfg = crate::Config::read(&std::fs::read_to_string(std::env::var("COMPUTE_TOML").unwrap()).unwrap()).unwrap();
    let mut d = cfg.driver().unwrap();
    let nonce = format!("{:?}", std::time::SystemTime::now());
    let (root, blobs) = tree(&[file("in.txt", &nonce, false)]).unwrap();
    d.upload(&blobs).unwrap();
    let j = JobSpec { network_off: true, ..job(root, &["/bin/sh", "-c", "cat in.txt; echo to-err >&2; mkdir -p out; cp in.txt out/copy"]) };
    let mut live = Vec::new();
    let first = d.run(&j, &mut |s, b| live.push((s, b.to_vec())), &go()).unwrap();
    assert!(!first.cached && first.exit_code == 0, "{first:?}");
    assert_eq!(d.download(&first.stdout_digest).unwrap(), nonce.as_bytes());
    assert_eq!(d.download(&first.stderr_digest).unwrap(), b"to-err\n");
    let said: Vec<u8> = live.iter().filter(|(s, _)| *s == Stream::Stdout).flat_map(|(_, b)| b.clone()).collect();
    assert_eq!(said, nonce.as_bytes(), "its output handed over");
    let out = scratch("real-out");
    crate::tree::materialize(first.output_tree_digest.as_ref().expect("out/ given back"), &mut |x| d.download(x), &out).unwrap();
    assert_eq!(std::fs::read_to_string(out.join("copy")).unwrap(), nonce);
    let second = d.run(&j, &mut |_, _| {}, &go()).unwrap();
    assert!(second.cached, "{second:?}");
    assert_eq!((&second.stdout_digest, &second.output_tree_digest), (&first.stdout_digest, &first.output_tree_digest));
    eprintln!("real endpoint: miss {} ms, hit {} ms", first.wall_ms, second.wall_ms);
    drop(std::fs::remove_dir_all(&out));
}
