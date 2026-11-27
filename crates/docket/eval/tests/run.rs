//! A task run end to end through a compute driver: the candidate's job, then the verifier's on what it left, the
//! result recorded with its trace put away. An honest candidate passes; one that writes its own verifier fails.

use compute::{ComputeDriver, Digest, File, JobResult, JobSpec, LocalFallbackDriver, Stream, tree};
use docket_eval::job::{agent, verify};
use docket_eval::result::{Figures, record};
use docket_eval::task::{Budget, Task};
use std::sync::atomic::AtomicBool;

/// This host's driver behind a check of what a worker would be given: the task's pinned image and every limit. The
/// host's driver keeps neither, so the job runs on the host's root with them taken off; what this test shows is the
/// two jobs, their trees and the verdict, not the image's isolation, which a worker gives.
struct Pinned {
    host: LocalFallbackDriver,
    image: Digest,
}

impl ComputeDriver for Pinned {
    fn upload(&mut self, blobs: &[Vec<u8>]) -> Result<Vec<Digest>, String> {
        self.host.upload(blobs)
    }
    fn download(&mut self, d: &Digest) -> Result<Vec<u8>, String> {
        self.host.download(d)
    }
    fn run(&mut self, job: &JobSpec, out: &mut dyn FnMut(Stream, &[u8]), stop: &AtomicBool) -> Result<JobResult, String> {
        assert_eq!(job.rootfs_digest.as_ref(), Some(&self.image), "every job on the pinned image");
        assert!(job.network_off && job.limits.cpus.is_some() && job.limits.memory_bytes.is_some() && job.limits.wall_ms.is_some(), "{job:?}");
        let mut on_host = job.clone();
        (on_host.rootfs_digest, on_host.limits.cpus, on_host.limits.memory_bytes) = (None, None, None);
        self.host.run(&on_host, out, stop)
    }
}

fn file(path: &str, data: &str, executable: bool) -> File {
    File { path: path.into(), data: data.as_bytes().to_vec(), executable }
}

/// TREE's files put in D's store; its root.
fn put(d: &mut dyn ComputeDriver, files: &[File]) -> Digest {
    let (root, blobs) = tree(files).unwrap();
    d.upload(&blobs).unwrap();
    root
}

/// CMD run as the candidate on TASK, then verified; its figures and the trace of both jobs' output.
fn attempt(d: &mut Pinned, task: &Task, cmd: &str) -> (Figures, String) {
    let stop = AtomicBool::new(false);
    let mut trace = String::new();
    let a = d.run(&agent(task, vec!["/bin/sh".into(), "-c".into(), cmd.into()]), &mut |_, b| trace.push_str(&String::from_utf8_lossy(b)), &stop).unwrap();
    let produced = a.output_tree_digest.clone().unwrap_or_else(|| put(d, &[]));
    let (job, root) = verify(task, produced).unwrap();
    d.upload(&[root]).unwrap();
    let v = d.run(&job, &mut |_, b| trace.push_str(&String::from_utf8_lossy(b)), &stop).unwrap();
    (Figures { pass: a.exit_code == 0 && v.exit_code == 0, usd_cents: 0, tokens: 0, wall_ms: a.wall_ms + v.wall_ms }, trace)
}

#[test]
fn a_task_runs_end_to_end_and_a_candidate_cannot_write_what_judges_it() {
    let dir = std::env::temp_dir().join(format!("docket-eval-run-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("cold")).unwrap();
    let image = Digest::of(b"a pinned image, by its digest");
    let mut d = Pinned { host: LocalFallbackDriver::new(&dir.join("store")).unwrap(), image: image.clone() };
    let input = put(&mut d, &[file("task.md", "Write 42 to out/answer.\n", false)]);
    // the verifier reads the candidate's answer from work/; it uses only the shell's own words
    let verifier = put(&mut d, &[file("check", "#!/bin/sh\nread a < work/answer || exit 1\n[ \"$a\" = 42 ]\n", true)]);
    let budget = Budget { tokens: 1000, usd_cents: 50, wall_ms: 60_000, cpus: 1, memory_bytes: 1 << 30 };
    let task = Task { id: "answer".into(), image_digest: image, input_digest: input, verifier_digest: verifier, verify_cmd: vec!["verifier/check".into()], budget };

    // the candidate's job has the task and no verifier
    let honest = "[ -f task.md ] && [ ! -e verifier ] && [ ! -e check ] && echo 42 > out/answer";
    let (f, trace) = attempt(&mut d, &task, honest);
    assert!(f.pass, "{trace}");
    // a wrong answer fails
    assert!(!attempt(&mut d, &task, "echo 7 > out/answer").0.pass);
    // a candidate that leaves a verifier of its own, at every place one might be read, still fails
    let forged = "for p in out/verifier out; do mkdir -p $p; printf '#!/bin/sh\\nexit 0\\n' > $p/check; chmod +x $p/check; done; echo 7 > out/answer";
    let (f, trace) = attempt(&mut d, &task, forged);
    assert!(!f.pass, "its own check is under work/, never run: {trace}");

    // the result kept, its trace in cold storage
    let t = dir.join("trace.txt");
    std::fs::write(&t, trace).unwrap();
    let target = cold_store::Target { name: "disk".into(), local: Some(dir.join("cold").display().to_string()), rclone: None, program: None };
    let r = record(&task.id, "forger", 1, f, &t.display().to_string(), &target).unwrap();
    assert!(!r.pass && dir.join("cold").join(&r.trace_ref).is_file() && !t.exists());
    let _ = std::fs::remove_dir_all(&dir);
}
