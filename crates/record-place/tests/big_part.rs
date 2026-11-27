//! A git place serving a 2 GiB blob in a part's name: refused from its size, the memory of this process flat.

use record_place::{GitRepoPlace, RecordPlace};
use std::process::Command;

/// The most memory this process has held so far, in KiB.
fn high_water() -> u64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap();
    s.lines().find_map(|l| l.strip_prefix("VmHWM:")).and_then(|v| v.trim().trim_end_matches("kB").trim().parse().ok()).unwrap()
}

fn sh(line: &str) -> String {
    let o = Command::new("sh").arg("-c").arg(line).output().unwrap();
    assert!(o.status.success(), "{line}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

#[test]
fn a_two_gib_part_is_refused_unread() {
    let d = std::env::temp_dir().join(format!("record-place-big-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let r = d.join("remote.git").display().to_string();
    sh(&format!("git init -q --bare {r}"));
    let g = |input: &str, args: &str| sh(&format!("{input} | git --git-dir {r} {args}"));
    let big = g("head -c 2147483648 /dev/zero", "hash-object -w --stdin");
    let (head, open) = (g("echo 1", "hash-object -w --stdin"), g("echo o", "hash-object -w --stdin"));
    let parts = g(&format!("printf '100644 blob {big}\\t000000000000.part\\n'"), "mktree");
    let root = g(&format!("printf '040000 tree {parts}\\tparts\\n100644 blob {head}\\thead\\n100644 blob {open}\\topen.part\\n'"), "mktree");
    let c = g("true", &format!("-c user.name=t -c user.email=t@t commit-tree {root} -m big"));
    g("true", &format!("update-ref refs/heads/record {c}"));
    let before = high_water();
    let p = GitRepoPlace::open(&r, d.join("w"), "record").unwrap();
    let e = p.fetch_from(0).unwrap_err().to_string();
    let grew = high_water().saturating_sub(before);
    assert!(e.contains("2147483648 bytes"), "{e}");
    assert!(grew < 64 << 10, "this process grew by {grew} KiB");
    let _ = std::fs::remove_dir_all(&d);
}
