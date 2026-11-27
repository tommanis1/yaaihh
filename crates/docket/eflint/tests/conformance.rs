//! The reference's conformance files through this build's program (conformance/run.py), each derivation both ways:
//! every combination of a rule's fields tried, as the reference does, and guided. The files whose answers differ from
//! the reference's are the ones listed, the same both ways; a change to the list is a change of answers.

/// run.py's summary lines and the files that differ, the program run with FLAGS.
fn differ(flags: &[&str]) -> Vec<String> {
    let out = std::process::Command::new("python3")
        .arg("-I")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/conformance/run.py"))
        .arg(env!("CARGO_BIN_EXE_eflint-rs"))
        .args(flags)
        .output()
        .expect("python3 runs");
    let text = String::from_utf8_lossy(&out.stdout);
    println!("{text}{}", String::from_utf8_lossy(&out.stderr));
    text.lines()
        .filter(|l| ["Reference suite", "Benchmark:", "NO REFERENCE", "DIFF"].iter().any(|p| l.starts_with(p)))
        .map(str::to_string)
        .collect()
}

/// run.py's summary lines and the files whose answers differ.
const DIFFER: [&str; 15] = [
    "Reference suite, files the reference passes: 112 files; test mode same 112, full output same 110",
    "Reference suite, files the reference itself fails: 4 files; test mode same 1, full output same 1",
    "Benchmark: 52 files; test mode same 50, full output same 46",
    "NO REFERENCE ANSWER benchmark/performance/bench_triple.same6.eflint (ours did not finish either)",
    "DIFF reference/derivation-rules/deriv_sum1a.eflint test full",
    "DIFF reference/derivation-rules/deriv_sum1b.eflint test full",
    "DIFF reference/derivation-rules/deriv_sum2a.eflint  full",
    "DIFF reference/derivation-rules/deriv_sum2b.eflint  full",
    "DIFF reference/derivation-rules/from_not_enabled2.eflint test full",
    "DIFF benchmark/functional/22_forall_exists.same0.eflint  full",
    "DIFF benchmark/functional/diff_1_naf.diff0.eflint  full",
    "DIFF benchmark/functional/diff_2_no_stable.diff0.eflint test full",
    "DIFF benchmark/functional/diff_3_2_stable.diff0.eflint test full",
    "DIFF benchmark/functional/diff_clingo_no_model.diff0.eflint  full",
    "DIFF benchmark/performance/bench_triple.same5.eflint  full",
];

#[test]
fn the_conformance_files_that_differ_from_the_reference_are_these() {
    assert_eq!(differ(&[]), DIFFER);
}

#[test]
fn the_conformance_files_that_differ_from_the_reference_guided_are_the_same() {
    assert_eq!(differ(&["--guided"]), DIFFER);
}
