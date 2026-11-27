//! A file tool's change as std.hooks' `FileChange` wants it: a unified diff, and the lines it adds.

use crate::frame::FileChange;
use std::path::{Component, Path, PathBuf};

/// File `p` as the agent names it: where to read it, and its name from the working folder `cwd` (its whole path
/// where `cwd` is unknown or the file is outside it). `.` and `..` are taken out by the text alone; no link is
/// followed.
fn place(cwd: &str, p: &str) -> (String, String) {
    let (whole, mut parts) = (Path::new(cwd).join(p), Vec::<Component>::new());
    for c in whole.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir if matches!(parts.last(), Some(Component::Normal(_))) => drop(parts.pop()),
            c => parts.push(c),
        }
    }
    let read: PathBuf = parts.iter().collect();
    let shown = if cwd.is_empty() { read.clone() } else { read.strip_prefix(cwd).map_or(read.clone(), Path::to_path_buf) };
    (read.display().to_string(), shown.display().to_string())
}

/// The hunk from `old` to `new` in the file read at `read`, named `shown`. It starts at `old`'s line in the file
/// where the file can be read and holds it once, else at line 1.
fn hunk(read: &str, shown: &str, old: &str, new: &str) -> String {
    let at = std::fs::read_to_string(read).ok().filter(|f| !old.is_empty() && f.matches(old).count() == 1).map(|f| f[..f.find(old).unwrap()].matches('\n').count() + 1).unwrap_or(1);
    let (o, n): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let mut p = format!("--- a/{shown}\n+++ b/{shown}\n@@ -{at},{} +{at},{} @@\n", o.len(), n.len());
    o.iter().for_each(|l| p += &format!("-{l}\n"));
    n.iter().for_each(|l| p += &format!("+{l}\n"));
    p
}

fn change(kind: &str, shown: String, patch: String, added: &str) -> FileChange {
    FileChange { kind: kind.into(), path: shown, patch, added: added.into(), ..Default::default() }
}

/// An edit of file `path` that puts `new` where `old` is; its added lines are `new`'s. No old text on a file that is
/// not there yet makes it: a create.
pub fn edit(cwd: &str, path: &str, old: &str, new: &str) -> FileChange {
    let (read, shown) = place(cwd, path);
    let kind = if old.is_empty() && !Path::new(&read).exists() { "create" } else { "edit" };
    change(kind, shown.clone(), hunk(&read, &shown, old, new), new)
}

/// A write of `content` to file `path`: a create where the file is not there yet, read where the write is asked.
pub fn write(cwd: &str, path: &str, content: &str) -> FileChange {
    let (read, shown) = place(cwd, path);
    let had = std::fs::read_to_string(&read).ok();
    let kind = if had.is_some() || Path::new(&read).exists() { "edit" } else { "create" };
    change(kind, shown.clone(), hunk(&read, &shown, had.as_deref().unwrap_or_default(), content), content)
}

/// The files an `apply_patch` text changes (`*** Begin Patch`, a section per file: `*** Add File: P` with its `+`
/// lines, `*** Delete File: P`, `*** Update File: P` with `@@` chunks of ` `, `-` and `+` lines and maybe a
/// `*** Move to: Q`), each path taken from `cwd`. A move is one change named by both paths, its patch and added
/// lines only what it changes in the text (none for a plain move).
pub fn apply_patch(text: &str, cwd: &str) -> Vec<FileChange> {
    let mut out = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(l) = lines.next() {
        let Some((kind, p)) = l.split_once(": ").filter(|(k, _)| ["*** Add File", "*** Delete File", "*** Update File"].contains(k)) else { continue };
        let (read, shown) = place(cwd, p.trim());
        let mut body = Vec::new();
        while let Some(n) = lines.peek().filter(|n| !n.starts_with("*** ") || n.starts_with("*** End of File") || n.starts_with("*** Move to:")) {
            body.push(*n);
            lines.next();
        }
        let old = || std::fs::read_to_string(&read).unwrap_or_default();
        let plus = |b: &[&str]| b.iter().filter_map(|b| b.strip_prefix('+')).collect::<Vec<_>>().join("\n");
        match kind {
            "*** Add File" => out.push(change("create", shown.clone(), hunk(&read, &shown, "", &plus(&body)), &plus(&body))),
            "*** Delete File" => out.push(change("delete", shown.clone(), hunk(&read, &shown, &old(), ""), "")),
            _ => {
                let chunks: Vec<(String, String, String)> = body
                    .split(|b| b.starts_with("@@"))
                    .filter(|c| c.iter().any(|b| b.starts_with(['-', '+'])))
                    .map(|c| {
                        let side = |keep: char| c.iter().filter_map(|b| b.strip_prefix(' ').or_else(|| b.strip_prefix(keep))).collect::<Vec<_>>().join("\n");
                        (side('-'), side('+'), plus(c))
                    })
                    .collect();
                let mut patch = String::new();
                for (o, n, _) in &chunks {
                    let (h, first) = (hunk(&read, &shown, o, n), patch.is_empty());
                    patch += if first { &h } else { h.splitn(3, '\n').nth(2).unwrap_or_default() };
                }
                let added: Vec<&str> = chunks.iter().map(|(_, _, a)| a.as_str()).filter(|a| !a.is_empty()).collect();
                let mut c = change("edit", shown.clone(), patch, &added.join("\n"));
                if let Some(to) = body.iter().find_map(|b| b.strip_prefix("*** Move to:")) {
                    let to = place(cwd, to.trim()).1;
                    if !c.patch.is_empty() {
                        let hunks = c.patch.splitn(3, '\n').nth(2).unwrap_or_default();
                        c.patch = format!("--- a/{shown}\n+++ b/{to}\n{hunks}");
                    }
                    (c.kind, c.from, c.path) = ("move".into(), shown.clone(), to);
                }
                out.push(c);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_apply_patch_text_is_read_into_a_change_for_each_file_it_touches() {
        let dir = std::env::temp_dir().join(format!("docket-patch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "one\ntwo\nthree\n").unwrap();
        let d = dir.display().to_string();
        let c = |k: &str, p: &str, patch: &str, a: &str| FileChange { kind: k.into(), path: p.into(), patch: patch.into(), added: a.into(), ..Default::default() };
        let u = apply_patch("*** Begin Patch\n*** Update File: a.txt\n@@\n one\n-two\n+2\n*** End Patch\n", &d);
        assert_eq!(u, vec![c("edit", "a.txt", "--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n-one\n-two\n+one\n+2\n", "2")], "only the lines it adds, not its context");
        let n = apply_patch("*** Begin Patch\n*** Add File: b.txt\n+fresh\n+lines\n*** End Patch", &d);
        assert_eq!((n[0].kind.as_str(), n[0].path.as_str(), n[0].added.as_str()), ("create", "b.txt", "fresh\nlines"));
        let two = apply_patch(&format!("*** Begin Patch\n*** Add File: /x/c\n+c\n*** Delete File: {d}/a.txt\n*** End Patch"), &d);
        let kinds: Vec<(&str, &str)> = two.iter().map(|c| (c.kind.as_str(), c.path.as_str())).collect();
        assert_eq!(kinds, [("create", "/x/c"), ("delete", "a.txt")], "a file outside the working folder keeps its whole path");
        let m = apply_patch("*** Begin Patch\n*** Update File: a.txt\n*** Move to: c.txt\n@@\n-one\n+1\n*** End Patch", &d);
        let moved = FileChange { kind: "move".into(), path: "c.txt".into(), from: "a.txt".into(), patch: "--- a/a.txt\n+++ b/c.txt\n@@ -1,1 +1,1 @@\n-one\n+1\n".into(), added: "1".into() };
        assert_eq!(m, [moved], "a move: one change by both names, only what it changes in the text");
        let plain = apply_patch("*** Begin Patch\n*** Update File: ./a.txt\n*** Move to: x/../e.txt\n*** End Patch", &d);
        assert_eq!(plain, [FileChange { kind: "move".into(), path: "e.txt".into(), from: "a.txt".into(), ..Default::default() }], "a plain move: no patch");
        assert_eq!(write(&d, "a.txt", "x").kind, "edit");
        assert_eq!(write(&d, "new", "x"), c("create", "new", "--- a/new\n+++ b/new\n@@ -1,0 +1,1 @@\n+x\n", "x"));
        assert_eq!(edit("", "/w/f", "a", "b").path, "/w/f", "no working folder: the path as given");
        assert_eq!((edit(&d, "made.txt", "", "x").kind, edit(&d, "a.txt", "", "x").kind), ("create".to_string(), "edit".to_string()));
        for spelt in ["x/../engine/a.rs", "./engine/a.rs", "/w/./engine/x/../a.rs"] {
            assert_eq!(edit("/w", spelt, "a", "b").path, "engine/a.rs", "{spelt}: named as it resolves");
        }
        assert_eq!(edit("/w", "../v/a.rs", "a", "b").path, "/v/a.rs", "a path that leaves the working folder: whole");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
