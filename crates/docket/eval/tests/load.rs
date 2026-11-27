//! Tasks read from a manifest into sealed jobs, and a run's result recorded with its trace put away.

use compute::Digest;
use docket_eval::job::{agent, verify};
use docket_eval::result::{Figures, record};
use docket_eval::task::{Budget, digest, read};
use std::path::{Path, PathBuf};

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("docket-eval-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
const C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const BUDGET: &str = "budget = { tokens = 1000, usd_cents = 50, wall_ms = 60000, cpus = 2, memory_bytes = 1073741824 }";

fn entry(id: &str) -> String {
    format!("[[task]]\nid = \"{id}\"\nimage = \"{A}/10\"\ninput = \"{B}/20\"\nverifier = \"{C}/30\"\nverify = [\"verifier/check\"]\n{BUDGET}\n")
}

#[test]
fn a_manifest_lists_generic_tasks_and_refuses_what_is_off() {
    let tasks = read(&(entry("t1") + &entry("t-2.b_c"))).unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].image_digest, Digest { hash: A.into(), size: 10 });
    assert_eq!((tasks[0].input_digest.hash.as_str(), tasks[1].verifier_digest.hash.as_str()), (B.to_ascii_lowercase().as_str(), C));
    assert_eq!(tasks[0].budget, Budget { tokens: 1000, usd_cents: 50, wall_ms: 60_000, cpus: 2, memory_bytes: 1 << 30 });
    let refused = |text: String, why: &str| {
        let e = read(&text).unwrap_err();
        assert!(e.contains(why), "{why}: {e}\n{text}");
    };
    refused(entry("t1") + &entry("t1"), "listed twice");
    refused(String::new(), "task");
    refused("task = []".into(), "no task");
    refused(entry(""), "an id is");
    refused(entry("a b"), "an id is");
    refused(entry("../x"), "an id is");
    refused(entry("t1").replace(&format!("image = \"{A}/10\"\n"), ""), "image");
    refused(entry("t1").replace(&format!("verifier = \"{C}/30\"\n"), ""), "verifier");
    refused(entry("t1").replace(&format!("{C}/30"), &format!("{B}/20")), "the verifier's tree is the candidate's input");
    refused(entry("t1").replace("[\"verifier/check\"]", "[]"), "runs a file of verifier/");
    for cmd in ["./verify", "work/check", "verifier/", "verifier/../work/x", "verifier//x"] {
        refused(entry("t1").replace("verifier/check", cmd), "runs a file of verifier/");
    }
    refused(entry("t1").replace(&format!("{B}/20"), "x/20"), "not a SHA-256");
    refused(entry("t1").replace("id = ", "network = true\nid = "), "unknown field");
    for zero in ["tokens = 0", "usd_cents = 0", "wall_ms = 0", "cpus = 0", "memory_bytes = 0"] {
        let dim = zero.split(' ').next().unwrap();
        let was = BUDGET.split(&format!("{dim} = ")).nth(1).unwrap().split([',', ' ']).next().unwrap();
        refused(entry("t1").replace(&format!("{dim} = {was}"), zero), "a budget of zero");
    }
    refused(entry("t1").replace("cpus = 2, ", ""), "cpus");
    assert!(digest(&format!("{A}/-1")).is_err() && digest(A).is_err());
}

#[test]
fn both_jobs_are_sealed_pinned_and_bounded_and_the_verifier_has_its_own_tree() {
    let task = read(&entry("t1")).unwrap().remove(0);
    let a = agent(&task, vec!["run".into()]);
    assert!(a.network_off && a.env.is_empty());
    assert_eq!((a.rootfs_digest.clone(), a.input_tree_digest.clone()), (Some(task.image_digest.clone()), task.input_digest.clone()));
    assert_eq!((a.limits.wall_ms, a.limits.cpus, a.limits.memory_bytes), (Some(60_000), Some(2), Some(1 << 30)));
    let produced = Digest { hash: A.into(), size: 99 };
    let (v, root) = verify(&task, produced.clone()).unwrap();
    assert!(v.network_off && v.rootfs_digest == Some(task.image_digest.clone()) && v.limits == a.limits);
    assert_eq!((v.input_tree_digest, v.cmd), (compute::nest(&[("work", produced), ("verifier", task.verifier_digest.clone())]).unwrap().0, vec!["verifier/check".to_string()]));
    assert_eq!(Digest::of(&root), compute::nest(&[("verifier", task.verifier_digest.clone()), ("work", Digest { hash: A.into(), size: 99 })]).unwrap().0);
}

#[test]
fn a_result_keeps_its_figures_and_leaves_a_pointer_for_its_trace_a_file_or_a_folder() {
    let d = tmp("result");
    std::fs::create_dir(d.join("cold")).unwrap();
    let target = cold_store::Target { name: "disk".into(), local: Some(d.join("cold").display().to_string()), rclone: None, program: None };
    let trace = d.join("r1.trace.jsonl").display().to_string();
    std::fs::write(&trace, "{\"step\": 1}\n".repeat(100)).unwrap();
    let f = Figures { pass: true, usd_cents: 12, tokens: 3400, wall_ms: 9000 };
    let r = record("o__r-1", "c1", 2, f, &trace, &target).unwrap();
    assert!(r.trace_ref.starts_with("eval-traces/") && r.pass && r.tokens == 3400 && r.trial == 2);
    assert!(!Path::new(&trace).exists(), "the trace is gone from here");
    let p: cold_store::Pointer = serde_json::from_str(&std::fs::read_to_string(cold_store::pointer_of(&trace)).unwrap()).unwrap();
    assert_eq!((p.key.as_str(), p.manifest.files), (r.trace_ref.as_str(), 1));
    assert!(d.join("cold").join(&r.trace_ref).is_file());
    // a trace that is a folder goes the same way, and is removed whole
    let folder = d.join("r2.trace");
    std::fs::create_dir_all(folder.join("steps")).unwrap();
    std::fs::write(folder.join("steps/1.json"), "{}").unwrap();
    let r2 = record("t", "c1", 1, f, &folder.display().to_string(), &target).unwrap();
    assert!(!folder.exists() && d.join("cold").join(&r2.trace_ref).join("steps/1.json").is_file());
    std::fs::write(d.join("not-a-folder"), "").unwrap();
    let none = cold_store::Target { local: Some(d.join("not-a-folder").display().to_string()), ..target };
    let kept = d.join("r3.trace.jsonl").display().to_string();
    std::fs::write(&kept, "x").unwrap();
    assert!(record("t", "c", 1, f, &kept, &none).is_err() && Path::new(&kept).exists(), "nothing removed when the copy fails");
    let _ = std::fs::remove_dir_all(&d);
}
