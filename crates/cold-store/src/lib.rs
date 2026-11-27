//! Cold storage: a folder or rclone remote holding keyed copies checked by manifest, with a `PATH.cold.json` pointer left behind.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A cold-storage target as a `[[cold.target]]` table names it: `local = PATH` or `rclone = REMOTE:PATH`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Target {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rclone: Option<String>,
    /// The rclone program (tests give a stand-in); `rclone` on PATH by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
}

/// What a target is, read from its fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    LocalPath(PathBuf),
    Rclone { remote: String, program: String },
}

impl Target {
    pub fn kind(&self) -> Result<Kind, String> {
        match (&self.local, &self.rclone) {
            (Some(p), None) if Path::new(p).is_absolute() => Ok(Kind::LocalPath(PathBuf::from(p))),
            (Some(p), None) => Err(format!("cold target {}: local = {p} is not an absolute path", self.name)),
            (None, Some(r)) if r.contains(':') => Ok(Kind::Rclone { remote: r.trim_end_matches('/').to_string(), program: self.program.clone().unwrap_or_else(|| "rclone".into()) }),
            (None, Some(r)) => Err(format!("cold target {}: rclone = {r} names no remote (REMOTE:PATH)", self.name)),
            _ => Err(format!("cold target {}: give exactly one of local or rclone", self.name)),
        }
    }

    /// Where it is, in words for the window.
    pub fn place(&self) -> String {
        self.local.clone().or_else(|| self.rclone.clone()).unwrap_or_default()
    }

    /// Whether it can take anything now, else why not: a local folder that is there, an rclone program that runs.
    pub fn reachable(&self) -> Result<(), String> {
        match self.kind()? {
            Kind::LocalPath(p) if p.is_dir() => Ok(()),
            Kind::LocalPath(p) => Err(format!("{} is not there (a disk not mounted?)", p.display())),
            Kind::Rclone { program, .. } => match Command::new(&program).arg("version").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status() {
                Ok(s) if s.success() => Ok(()),
                _ => Err(format!("{program} does not run here")),
            },
        }
    }

    /// SRC put under KEY, then checked: the copy's manifest is SRC's.
    pub fn put(&self, src: &Path, key: &str, want: &Manifest) -> Result<(), String> {
        match self.kind()? {
            Kind::LocalPath(root) => {
                let dst = root.join(key);
                if std::fs::symlink_metadata(&dst).is_ok() {
                    return Err(format!("{} is in cold storage already", dst.display()));
                }
                std::fs::create_dir_all(dst.parent().unwrap_or(&root)).map_err(|e| format!("{}: {e}", root.display()))?;
                copy(src, &dst)?;
                let got = Manifest::of(&dst)?;
                if got.digest != want.digest {
                    return Err(format!("the copy at {} does not match what it copies: left in place, nothing removed", dst.display()));
                }
                Ok(())
            }
            Kind::Rclone { remote, program } => {
                let dst = format!("{remote}/{key}");
                let file = src.is_file();
                let to = if file { format!("{dst}/{}", src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()) } else { dst.clone() };
                run(&program, &["copyto", "--links", &src.display().to_string(), &to])?;
                run(&program, &["check", "--one-way", "--links", &src.display().to_string(), &to]).map_err(|e| format!("rclone's check of {to} after the copy: {e}; nothing removed"))
            }
        }
    }

    /// KEY got back to DST (which must not be there), checked against WANT.
    pub fn get(&self, key: &str, name: &str, dst: &Path, want: &Manifest) -> Result<(), String> {
        match self.kind()? {
            Kind::LocalPath(root) => copy(&root.join(key), dst)?,
            Kind::Rclone { remote, program } => {
                let from = if want.file { format!("{remote}/{key}/{name}") } else { format!("{remote}/{key}") };
                run(&program, &["copyto", "--links", &from, &dst.display().to_string()])?;
            }
        }
        let got = Manifest::of(dst)?;
        if got.digest != want.digest {
            return Err(format!("what came back to {} does not match what was put away", dst.display()));
        }
        Ok(())
    }
}

/// The pointer left where PATH was.
pub fn pointer_of(path: &str) -> PathBuf {
    PathBuf::from(format!("{}.cold.json", path.trim_end_matches('/')))
}

/// What a pointer holds: where the thing went, under which key, and what it held.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Pointer {
    pub path: String,
    pub what: String,
    pub target: String,
    pub key: String,
    pub manifest: Manifest,
    pub at: String,
}

/// TEXT as one part of a key: letters, digits, `.` and `-` kept, anything else `_`, no part that is only dots.
fn part(text: &str) -> String {
    let s: String = text.trim_matches('/').chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect();
    if s.chars().all(|c| c == '.') { s.replace('.', "_") } else { s }
}

/// A key no other stow makes: the sort, the path, the time to the nanosecond, this process and its count of stows.
fn key(what: &str, path: &str, now: chrono::DateTime<chrono::Utc>) -> Result<String, String> {
    static STOWS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sort = part(what);
    if what.is_empty() || sort != what {
        return Err(format!("{what:?}: a sort of cold storage is letters, digits, `.` and `-`"));
    }
    let n = STOWS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Ok(format!("{sort}/{}-{}-{}-{n}", part(path), now.format("%Y%m%dT%H%M%S%.9fZ"), std::process::id()))
}

/// PATH copied to TARGET under a fresh key of the sort WHAT, checked, and its pointer written whole or not at all;
/// the caller removes PATH.
pub fn stow(path: &str, what: &str, target: &Target) -> Result<Pointer, String> {
    let manifest = Manifest::of(Path::new(path))?;
    let now = chrono::Utc::now();
    let key = key(what, path, now)?;
    target.put(Path::new(path), &key, &manifest)?;
    let p = Pointer { path: path.to_string(), what: what.to_string(), target: target.name.clone(), key, manifest, at: now.to_rfc3339() };
    let ptr = pointer_of(path);
    let text = serde_json::to_string_pretty(&p).map_err(|e| format!("{}: {e}; nothing removed", ptr.display()))?;
    let tmp = PathBuf::from(format!("{}.{}.tmp", ptr.display(), part(&p.key)));
    std::fs::write(&tmp, text).and_then(|()| std::fs::rename(&tmp, &ptr)).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}; nothing removed", ptr.display())
    })?;
    Ok(p)
}

/// PROGRAM run with ARGS; its error said in its own words.
fn run(program: &str, args: &[&str]) -> Result<(), String> {
    let o = Command::new(program).args(args).stdin(Stdio::null()).output().map_err(|e| format!("{program}: {e}"))?;
    if o.status.success() { Ok(()) } else { Err(format!("{program} {}: {}", args[0], String::from_utf8_lossy(&o.stderr).trim())) }
}

/// SRC copied to DST as it is: folders, files with their modes, links as links.
fn copy(src: &Path, dst: &Path) -> Result<(), String> {
    let m = std::fs::symlink_metadata(src).map_err(|e| format!("{}: {e}", src.display()))?;
    let fail = |e: std::io::Error| format!("{}: {e}", dst.display());
    if m.file_type().is_symlink() {
        std::os::unix::fs::symlink(std::fs::read_link(src).map_err(fail)?, dst).map_err(fail)
    } else if m.is_dir() {
        std::fs::create_dir(dst).map_err(fail)?;
        for e in std::fs::read_dir(src).map_err(|e| format!("{}: {e}", src.display()))? {
            let e = e.map_err(|e| format!("{}: {e}", src.display()))?;
            copy(&e.path(), &dst.join(e.file_name()))?;
        }
        std::fs::set_permissions(dst, m.permissions()).map_err(fail)
    } else {
        std::fs::copy(src, dst).map(|_| ()).map_err(fail)
    }
}

/// What a file or folder holds, for checking a copy: files, bytes, and one digest over every entry.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Manifest {
    pub files: u64,
    pub bytes: u64,
    pub digest: String,
    /// It is one file, not a folder.
    #[serde(default)]
    pub file: bool,
}

impl Manifest {
    pub fn of(path: &Path) -> Result<Manifest, String> {
        let mut lines = Vec::new();
        let (mut files, mut bytes) = (0u64, 0u64);
        walk(path, Path::new(""), &mut lines, &mut files, &mut bytes)?;
        lines.sort();
        let mut h = Sha256::new();
        for l in &lines {
            h.update(l.as_bytes());
            h.update(b"\n");
        }
        let file = std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file());
        Ok(Manifest { files, bytes, digest: format!("{:x}", h.finalize()), file })
    }
}

fn walk(at: &Path, rel: &Path, lines: &mut Vec<String>, files: &mut u64, bytes: &mut u64) -> Result<(), String> {
    let m = std::fs::symlink_metadata(at).map_err(|e| format!("{}: {e}", at.display()))?;
    let r = rel.display();
    if m.file_type().is_symlink() {
        lines.push(format!("l {r} {}", std::fs::read_link(at).map_err(|e| e.to_string())?.display()));
    } else if m.is_dir() {
        lines.push(format!("d {r}"));
        for e in std::fs::read_dir(at).map_err(|e| format!("{}: {e}", at.display()))? {
            let e = e.map_err(|e| e.to_string())?;
            walk(&e.path(), &rel.join(e.file_name()), lines, files, bytes)?;
        }
    } else {
        let mut f = std::fs::File::open(at).map_err(|e| format!("{}: {e}", at.display()))?;
        let mut h = Sha256::new();
        std::io::copy(&mut f, &mut h).map_err(|e| format!("{}: {e}", at.display()))?;
        lines.push(format!("f {r} {} {:x}", m.len(), h.finalize()));
        *files += 1;
        *bytes += m.len();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cold-driver-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn tree(at: &Path) {
        std::fs::create_dir_all(at.join("src/deep")).unwrap();
        std::fs::write(at.join("src/a.rs"), "fn a() {}").unwrap();
        std::fs::write(at.join("src/deep/b.txt"), vec![7u8; 5000]).unwrap();
        std::os::unix::fs::symlink("src/a.rs", at.join("link")).unwrap();
    }

    #[test]
    fn a_local_target_puts_checks_and_gets_back_the_same_tree() {
        let d = tmp("local");
        tree(&d.join("wt"));
        std::fs::create_dir(d.join("cold")).unwrap();
        let t = Target { name: "disk".into(), local: Some(d.join("cold").display().to_string()), rclone: None, program: None };
        assert!(t.reachable().is_ok());
        let m = Manifest::of(&d.join("wt")).unwrap();
        assert_eq!((m.files, m.bytes, m.file), (2, 5009, false));
        t.put(&d.join("wt"), "worktrees/wt-1", &m).unwrap();
        assert!(t.put(&d.join("wt"), "worktrees/wt-1", &m).is_err(), "a key is never written over");
        t.get("worktrees/wt-1", "wt", &d.join("back"), &m).unwrap();
        assert_eq!(Manifest::of(&d.join("back")).unwrap(), m);
        assert_eq!(std::fs::read_link(d.join("back/link")).unwrap(), Path::new("src/a.rs"));
        // a copy changed in cold storage is refused on the way back
        std::fs::write(d.join("cold/worktrees/wt-1/src/a.rs"), "changed").unwrap();
        assert!(t.get("worktrees/wt-1", "wt", &d.join("back2"), &m).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn two_stows_of_one_path_at_once_get_two_keys_and_a_sort_is_one_part() {
        let d = tmp("keys");
        std::fs::create_dir(d.join("cold")).unwrap();
        let t = Target { name: "disk".into(), local: Some(d.join("cold").display().to_string()), rclone: None, program: None };
        let f = d.join("t.jsonl").display().to_string();
        std::fs::write(&f, "{}\n").unwrap();
        let (a, b) = (stow(&f, "eval-traces", &t).unwrap(), stow(&f, "eval-traces", &t).unwrap());
        assert_ne!(a.key, b.key, "the same second, two keys");
        assert!(a.key.starts_with("eval-traces/") && d.join("cold").join(&b.key).is_file());
        let back: Pointer = serde_json::from_str(&std::fs::read_to_string(pointer_of(&f)).unwrap()).unwrap();
        assert_eq!(back, b, "the pointer is the last stow's, whole");
        assert!(std::fs::read_dir(&d).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")), "no half-written pointer left");
        for bad in ["", "..", "a/b", "../x", "x y"] {
            assert!(stow(&f, bad, &t).is_err(), "{bad:?}");
        }
        assert_eq!((part("/a/../b c"), part("..")), ("a_.._b_c".to_string(), "__".to_string()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn targets_are_read_from_their_fields_and_say_what_is_wrong() {
        let t = |l: Option<&str>, r: Option<&str>| Target { name: "x".into(), local: l.map(String::from), rclone: r.map(String::from), program: None };
        assert_eq!(t(Some("/mnt/nas"), None).kind(), Ok(Kind::LocalPath("/mnt/nas".into())));
        assert_eq!(t(None, Some("gdrive:cold/")).kind(), Ok(Kind::Rclone { remote: "gdrive:cold".into(), program: "rclone".into() }));
        assert!(t(Some("rel"), None).kind().unwrap_err().contains("absolute"));
        assert!(t(None, Some("nocolon")).kind().unwrap_err().contains("REMOTE:PATH"));
        assert!(t(Some("/a"), Some("g:b")).kind().is_err() && t(None, None).kind().is_err());
        assert!(t(Some("/no/such/cold/disk"), None).reachable().unwrap_err().contains("not there"));
    }

    #[test]
    fn an_rclone_target_copies_checks_and_gets_back_through_its_program() {
        let d = tmp("rclone");
        // a stand-in rclone: REMOTE:PATH is the folder d/remote/PATH; copyto copies, check compares
        let prog = d.join("rclone");
        let r = d.join("remote").display().to_string();
        std::fs::write(&prog, format!("#!/bin/bash\nmap() {{ case $1 in *:*) echo \"{r}/${{1#*:}}\";; *) echo \"$1\";; esac; }}\ncase $1 in\nversion) exit 0;;\ncopyto) s=$(map \"$3\"); t=$(map \"$4\"); mkdir -p \"$(dirname \"$t\")\" && cp -a \"$s\" \"$t\";;\ncheck) diff -r \"$(map \"$4\")\" \"$(map \"$5\")\" >/dev/null;;\n*) exit 2;;\nesac\n")).unwrap();
        std::fs::set_permissions(&prog, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        tree(&d.join("wt"));
        std::fs::write(d.join("t.jsonl"), "{}\n").unwrap();
        let t = Target { name: "drive".into(), local: None, rclone: Some("gdrive:cold".into()), program: Some(prog.display().to_string()) };
        assert!(t.reachable().is_ok());
        for (src, key) in [("wt", "worktrees/wt-1"), ("t.jsonl", "transcripts/t-1")] {
            let m = Manifest::of(&d.join(src)).unwrap();
            t.put(&d.join(src), key, &m).unwrap();
            t.get(key, src, &d.join(format!("{src}.back")), &m).unwrap();
            assert_eq!(Manifest::of(&d.join(format!("{src}.back"))).unwrap(), m, "{src}");
        }
        assert!(d.join("remote/cold/transcripts/t-1/t.jsonl").is_file(), "a file goes under its key by its own name");
        let _ = std::fs::remove_dir_all(&d);
    }
}
