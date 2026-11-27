//! A run's folder and its head (spec 11): its commit, or its tree's hash if it has changes; and the folder put back at one.

use std::path::Path;
use std::process::Command;

/// `git -C folder ARGS`, its output trimmed; none if git fails.
fn git(folder: &str, args: &[&str], index: Option<&Path>) -> Option<String> {
    let mut c = Command::new("git");
    c.arg("-C").arg(folder).args(args);
    if let Some(i) = index {
        c.env("GIT_INDEX_FILE", i);
    }
    let out = c.output().ok().filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The hash of `folder`'s tree as it stands, every file not ignored in it, built in a copy of its index so the
/// folder's own is untouched.
fn tree(folder: &str) -> Option<String> {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let index = std::env::temp_dir().join(format!("coordd-v2-index-{}-{n}", std::process::id()));
    let tree = git(folder, &["rev-parse", "--path-format=absolute", "--git-path", "index"], None)
        .and_then(|own| std::fs::copy(own, &index).ok())
        .and_then(|_| git(folder, &["add", "-A"], Some(&index)))
        .and_then(|_| git(folder, &["write-tree"], Some(&index)));
    let _ = std::fs::remove_file(&index);
    tree
}

/// The head of `folder`: its commit when nothing has changed, else the hash of its tree as it stands; empty where
/// it is no git working tree.
pub fn head(folder: &str) -> String {
    let changed = match git(folder, &["status", "--porcelain"], None) {
        Some(s) => !s.is_empty(),
        None => return String::new(),
    };
    if !changed {
        return git(folder, &["rev-parse", "HEAD"], None).unwrap_or_default();
    }
    tree(folder).unwrap_or_default()
}

/// Put `folder`'s files as they stood at `head`, a commit or a tree as [`head`] gave it. What stands now is kept
/// first under `refs/before-put/SECONDS-TREE`, so nothing is lost; the commit checked out stays, and ignored files
/// are left as they are. The kept ref holds every file not ignored, untracked ones too (a `.env` that is not
/// ignored, say), in the repository's objects until the ref is deleted; a push of branches and tags leaves it, a
/// mirror push takes it.
pub fn put(folder: &str, head: &str) -> Result<(), String> {
    let target = git(folder, &["rev-parse", "--verify", "--quiet", &format!("{head}^{{tree}}")], None).ok_or(format!("{folder} holds no tree {head}"))?;
    let now = tree(folder).ok_or(format!("{folder} is no git working tree"))?;
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let kept = format!("refs/before-put/{secs}-{now}");
    // a ref names a commit, so what stands now is kept as one
    let commit = git(folder, &["-c", "user.name=coordd-v2", "-c", "user.email=coordd-v2@localhost", "commit-tree", &now, "-m", &format!("{folder} before it was put at {head}")], None).ok_or(format!("{folder}: what stands now cannot be kept"))?;
    git(folder, &["update-ref", &kept, &commit], None).ok_or(format!("{folder}: {kept} cannot be written"))?;
    // the index first as the folder stands, so files not yet added go too where the old tree has none
    git(folder, &["read-tree", "--reset", &now], None).and_then(|_| git(folder, &["read-tree", "--reset", "-u", &target], None)).map(drop).ok_or(format!("{folder} cannot be put at {head}; what stood is kept at {kept}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_head_is_the_commit_then_the_tree_as_it_stands() {
        let dir = std::env::temp_dir().join(format!("head-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.to_str().unwrap();
        assert_eq!(head(f), "", "no working tree");
        let run = |args: &[&str]| assert!(Command::new("git").arg("-C").arg(f).args(["-c", "user.name=t", "-c", "user.email=t"]).args(args).output().unwrap().status.success(), "{args:?}");
        run(&["init", "-q"]);
        std::fs::write(dir.join("a"), "1").unwrap();
        run(&["add", "a"]);
        run(&["commit", "-q", "-m", "one"]);
        let commit = git(f, &["rev-parse", "HEAD"], None).unwrap();
        assert_eq!(head(f), commit);
        std::fs::write(dir.join("a"), "2").unwrap();
        let edited = head(f);
        assert!(edited.len() == 40 && edited != commit, "{edited}");
        std::fs::write(dir.join("b"), "new").unwrap();
        assert_ne!(head(f), edited, "a new file is in the tree too");
        assert_eq!(git(f, &["diff", "--cached", "--name-only"], None).unwrap(), "", "the folder's own index untouched");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_is_put_at_a_commit_or_a_tree_and_what_stood_is_kept() {
        let dir = std::env::temp_dir().join(format!("put-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.to_str().unwrap();
        let run = |args: &[&str]| assert!(Command::new("git").arg("-C").arg(f).args(["-c", "user.name=t", "-c", "user.email=t"]).args(args).output().unwrap().status.success(), "{args:?}");
        run(&["init", "-q"]);
        std::fs::write(dir.join(".gitignore"), "build\n").unwrap();
        std::fs::write(dir.join("a"), "1").unwrap();
        run(&["add", "a", ".gitignore"]);
        run(&["commit", "-q", "-m", "one"]);
        let one = head(f);
        std::fs::write(dir.join("a"), "2").unwrap();
        std::fs::write(dir.join("b"), "new").unwrap();
        std::fs::write(dir.join("build"), "kept").unwrap();
        let two = head(f);
        let read = |n: &str| std::fs::read_to_string(dir.join(n)).ok();
        put(f, &one).unwrap();
        assert_eq!((read("a"), read("b"), read("build")), (Some("1".into()), None, Some("kept".into())), "at the commit; an ignored file left");
        assert_eq!(head(f), one, "nothing changed against the commit");
        assert_eq!(git(f, &["rev-parse", "HEAD"], None).unwrap(), one, "the commit checked out stays");
        let kept = git(f, &["for-each-ref", "--format=%(objectname)", "refs/before-put/"], None).unwrap();
        assert_eq!(git(f, &["rev-parse", &format!("{kept}^{{tree}}")], None).unwrap(), two, "what stood is kept");
        put(f, &two).unwrap();
        assert_eq!((read("a"), read("b")), (Some("2".into()), Some("new".into())), "at the tree");
        assert_eq!(head(f), two);
        assert!(put(f, &"0".repeat(40)).unwrap_err().contains("holds no tree"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
