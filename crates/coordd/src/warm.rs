//! A layer's build outputs, started warm (docket-edge, N-101 post 34; paper-research post 56; docket-core post 60).
//!
//! The project file's `warm = "PATH"` names the line's last built outputs: a folder (or a link to one) holding NAME/
//! for each of the project's `outputs` and a file `commit` naming the commit they were built from. At a layer's open
//! coordd pins what PATH points to under `$COORD/layers/warm/`, once per commit, and every layer that gets it keeps
//! it; the runner may replace or delete its own folder afterwards. In the view each pinned NAME is the lower of an
//! overlay at REPO/NAME, so what a build writes goes to the layer's own folder (layer.rs).
//!
//! - `warm/SHA`: hard links into PATH (`cp -al`), for a layer that is not boxed. Only when PATH resolves to a folder
//!   named by its commit (a published snapshot nobody builds in again: yaaihh, N-155), every file and folder of it
//!   the owner's to write, and on PATH's filesystem; otherwise a copy. Nothing linked is ever chmodded: its files are
//!   PATH's. A tool rewriting an output in place is checked against the lower file's mode: through a link to a 0444
//!   interface (agda-stdlib's installed `_build`) not even the owner can rewrite it.
//! - `warm/SHA-box`: a copy, for boxed layers. A tool that rewrites an output in place (Agda's interfaces) is checked
//!   against the lower file's mode, so a boxed uid can rewrite only files of the box group writable by it, which a
//!   hard link to the owner's file never is (N-101 post 62). The holder makes it the box group's, as it does a base.
//!
//! A copy is the owner's to write (`chmod -R u+w`, the copy only). A copy is made only when it leaves the filesystem
//! more free than its reserved blocks (docket-core, N-101 post 63): a full disk is worse than a slow start, so with
//! no room the output starts empty, the note saying how much it would have taken.
//!
//! An output with nothing to pin starts empty, as with no `warm`: a note says why. Never an error: a cold start is
//! slow, not wrong. Pinned once: made at `warm/.NAME.PID` and renamed, removed as bases are (layer.rs `remove`).
//! Measured for Agda's `_build` only: Rust's `target/` holds absolute paths, so it is warm only at the same path.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

/// The pinned outputs of one layer: the folder's name under `warm/` (`SHA` or `SHA-box`), the commit, each output
/// that starts warm, and a note for each that does not.
#[derive(Debug, Default, PartialEq)]
pub struct Pinned {
    pub name: Option<String>,
    pub sha: Option<String>,
    pub warm: Vec<String>,
    pub notes: Vec<String>,
}

/// What PATH points to for repository NAME: the folder its outputs are in (links followed) and the commit they were
/// built from; or why there is none. Two shapes: PATH/commit holds one commit, the outputs under PATH; or (the
/// runner's snapshot of several repositories, yaaihh N-101 post 115) PATH/commit holds `REPO=SHA` lines, and NAME's
/// outputs are under PATH/NAME.
pub fn resolve(path: &str, name: &str) -> Result<(String, String), String> {
    let d = std::fs::canonicalize(path).map_err(|e| format!("{path}: {e}"))?;
    if !d.is_dir() {
        return Err(format!("{path} is no folder"));
    }
    let c = std::fs::read_to_string(d.join("commit")).map_err(|e| format!("{path}/commit: {e}"))?;
    let lines: Vec<&str> = c.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let (d, sha) = if lines.iter().any(|l| l.contains('=')) {
        let sha = lines.iter().find_map(|l| l.strip_prefix(&format!("{name}="))).filter(|_| !name.is_empty());
        let Some(sha) = sha else { return Err(format!("{path}/commit names no commit of {name:?}")) };
        let sub = d.join(name);
        if !sub.is_dir() {
            return Err(format!("{path} has no folder {name}"));
        }
        (sub, sha.trim().to_string())
    } else {
        (d, c.trim().to_string())
    };
    if sha.len() < 40 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("{path}/commit names no full commit ({sha:?})"));
    }
    Ok((d.display().to_string(), sha))
}

/// Whether folder D is named by commit SHA: a snapshot published under its commit, not a folder built in.
pub fn named_by(d: &str, sha: &str) -> bool {
    Path::new(d).file_name().is_some_and(|n| n.to_string_lossy() == sha)
}

/// The first file or folder under P that others cannot read (a folder: read and enter), if any; links are not
/// followed.
pub fn unreadable(p: &Path) -> Option<String> {
    lacking(p, 0o004, 0o005)
}

/// The first file or folder under P that its owner cannot write (a folder: write and enter), if any.
pub fn unwritable(p: &Path) -> Option<String> {
    lacking(p, 0o200, 0o300)
}

/// The first file under P whose mode lacks FILE, or folder whose mode lacks DIR (links not followed).
fn lacking(p: &Path, file: u32, dir: u32) -> Option<String> {
    let m = std::fs::symlink_metadata(p).ok()?;
    if m.file_type().is_symlink() {
        return None;
    }
    let mode = m.permissions().mode();
    let need = if m.is_dir() { dir } else { file };
    if mode & need != need {
        return Some(p.display().to_string());
    }
    if m.is_dir() {
        let rd = match std::fs::read_dir(p) {
            Ok(rd) => rd,
            Err(_) => return Some(p.display().to_string()),
        };
        for e in rd.flatten() {
            if let Some(u) = lacking(&e.path(), file, dir) {
                return Some(u);
            }
        }
    }
    None
}

/// Pin PATH's outputs OUTPUTS for a layer of REPO (named NAME among the project's repositories) under WARM (`$COORD/layers/warm`), linked or copied (BOXED: always
/// copied), unless pinned already. Each output pinned is one the layer starts warm; any other starts empty with a note.
pub fn pin(warm: &str, repo: &str, name: &str, path: &str, outputs: &[String], boxed: bool) -> Pinned {
    pin_with(warm, repo, name, path, outputs, boxed, &free)
}

/// The free space of the filesystem holding P, for an unprivileged user (`f_bavail`), and its reserved blocks
/// (`f_bfree - f_bavail`), in bytes.
fn free(p: &str) -> Option<(u64, u64)> {
    let c = std::ffi::CString::new(p).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let f = st.f_frsize as u64;
    Some((st.f_bavail as u64 * f, (st.f_bfree as u64).saturating_sub(st.f_bavail as u64) * f))
}

/// The bytes the files under P take on disk (links not followed; a file linked twice counted twice).
fn size(p: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    let Ok(m) = std::fs::symlink_metadata(p) else { return 0 };
    let mut n = m.blocks() * 512;
    if m.is_dir() {
        if let Ok(rd) = std::fs::read_dir(p) {
            n += rd.flatten().map(|e| size(&e.path())).sum::<u64>();
        }
    }
    n
}

/// Why a copy of SIZE bytes does not fit where FREE says (available, reserved), if it does not.
fn no_room(size: u64, free: Option<(u64, u64)>) -> Option<String> {
    let gb = |b: u64| format!("{:.1} GB", b as f64 / 1e9);
    match free {
        None => Some(format!("no room for the copy known, {} to copy", gb(size))),
        Some((avail, reserved)) if avail.saturating_sub(size) < reserved => Some(format!("no room for the copy, {} ({} free, {} reserved)", gb(size), gb(avail), gb(reserved))),
        Some(_) => None,
    }
}

/// `pin`, the free space read by FREE.
fn pin_with(warm: &str, repo: &str, name: &str, path: &str, outputs: &[String], boxed: bool, free: &dyn Fn(&str) -> Option<(u64, u64)>) -> Pinned {
    let mut p = Pinned::default();
    let cold_all = |why: String| Pinned { notes: outputs.iter().map(|o| format!("{o} starts empty: {why}")).collect(), ..Default::default() };
    let (d, sha) = match resolve(path, name) {
        Ok(r) => r,
        Err(e) => return cold_all(e),
    };
    if git(repo, &["cat-file", "-e", &format!("{sha}^{{commit}}")]).is_err() {
        return cold_all(format!("{path}'s commit {sha} is not in {repo}"));
    }
    let name = if boxed { format!("{sha}-box") } else { sha.clone() };
    let at = format!("{warm}/{name}");
    if !Path::new(&at).is_dir() {
        let tmp = format!("{warm}/.{name}.{}", std::process::id());
        let _ = remove(&tmp);
        if let Err(e) = std::fs::create_dir_all(&tmp) {
            return cold_all(format!("{tmp}: {e}"));
        }
        let link = !boxed && named_by(&d, &sha);
        for o in outputs {
            let from = format!("{d}/{o}");
            if !Path::new(&from).is_dir() {
                continue;
            }
            if let Some(u) = unreadable(Path::new(&from)) {
                p.notes.push(format!("{o} starts empty: {u} is not readable by others"));
                continue;
            }
            let to = format!("{tmp}/{o}");
            // an output inside a folder (`agda/_build`): its parent made first
            if let Some(parent) = Path::new(&to).parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    p.notes.push(format!("{o} starts empty: {}: {e}", parent.display()));
                    continue;
                }
            }
            let linked = link && unwritable(Path::new(&from)).is_none() && cp(&["-al"], &from, &to).is_ok();
            if !linked {
                let _ = remove(&to);
                if let Some(why) = no_room(size(Path::new(&from)), free(&tmp)) {
                    p.notes.push(format!("{o} starts empty: {why}"));
                    continue;
                }
                let made = cp(&["-a"], &from, &to).and_then(|_| chmod_w(&to));
                if let Err(e) = made {
                    let _ = remove(&to);
                    p.notes.push(format!("{o} starts empty: copying {from}: {e}"));
                }
            }
        }
        if let Err(e) = std::fs::rename(&tmp, &at) {
            let _ = remove(&tmp);
            // made meanwhile by another open of the same commit: that one is used
            if !Path::new(&at).is_dir() {
                return cold_all(format!("{at}: {e}"));
            }
        }
    }
    for o in outputs {
        if Path::new(&format!("{at}/{o}")).is_dir() {
            p.warm.push(o.clone());
        } else if !p.notes.iter().any(|n| n.starts_with(&format!("{o} starts empty"))) {
            p.notes.push(format!("{o} starts empty: {path} has no {o} (commit {sha})"));
        }
    }
    p.name = Some(name);
    p.sha = Some(sha);
    p
}

/// The files of BASE that differ from commit WARM in REPO (`git diff --name-only`, without renames, only those BASE
/// has): what a layer touches in its view, so a tool trusting mtimes sees them newer than the warm outputs.
pub fn changed(repo: &str, warm: &str, base: &str) -> Result<Vec<String>, String> {
    if warm == base {
        return Ok(Vec::new());
    }
    let out = git(repo, &["diff", "--name-only", "--no-renames", "--diff-filter=d", "-z", warm, base])?;
    Ok(out.split('\0').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect())
}

/// A copy made the owner's to write.
fn chmod_w(p: &str) -> Result<(), String> {
    let o = Command::new("chmod").args(["-R", "u+w", "--", p]).stdin(Stdio::null()).output().map_err(|e| format!("chmod: {e}"))?;
    if o.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).trim().to_string())
    }
}

fn cp(how: &[&str], from: &str, to: &str) -> Result<(), String> {
    let o = Command::new("cp").args(how).args(["--", from, to]).stdin(Stdio::null()).output().map_err(|e| format!("cp: {e}"))?;
    if o.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).trim().to_string())
    }
}

/// A folder of the owner's removed (pinned outputs: links and copies, never written through the view).
pub fn remove(p: &str) -> Result<(), String> {
    if !Path::new(p).exists() {
        return Ok(());
    }
    let o = Command::new("chmod").args(["-R", "u+rwX", "--", p]).stdin(Stdio::null()).output().map_err(|e| format!("chmod: {e}"))?;
    if !o.status.success() {
        return Err(format!("chmod: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    std::fs::remove_dir_all(p).map_err(|e| format!("{p}: {e}"))
}

fn git(repo: &str, args: &[&str]) -> Result<String, String> {
    let o = Command::new("git").arg("-C").arg(repo).args(args).stdin(Stdio::null()).output().map_err(|e| e.to_string())?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    fn sh(dir: &Path, c: &str) {
        assert!(Command::new("sh").arg("-c").arg(c).current_dir(dir).status().unwrap().success(), "{c}");
    }

    #[test]
    fn warm_outputs_are_resolved_pinned_once_and_cold_with_a_reason() {
        let t = std::env::temp_dir().join(format!("coordd-warm-{}", std::process::id()));
        let _ = remove(&t.display().to_string());
        let repo = t.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        sh(&repo, "git init -q -b dev && echo a > a.agda && echo b > b.agda && git add -A && git -c user.name=t -c user.email=t@x commit -qm one && echo c > c.agda && echo B > b.agda && git rm -q a.agda && git add -A && git -c user.name=t -c user.email=t@x commit -qm two");
        let r = repo.display().to_string();
        let one = git(&r, &["rev-parse", "HEAD~1"]).unwrap().trim().to_string();
        let two = git(&r, &["rev-parse", "HEAD"]).unwrap().trim().to_string();
        // the runner's snapshot, published under its commit, and the pointer to it
        let snap = t.join("runner").join(&one);
        std::fs::create_dir_all(snap.join("_build/sub")).unwrap();
        std::fs::write(snap.join("_build/sub/A.agdai"), "iface").unwrap();
        std::fs::write(snap.join("commit"), format!("{one}\n")).unwrap();
        std::os::unix::fs::symlink(&snap, t.join("runner/last")).unwrap();
        let path = t.join("runner/last").display().to_string();
        let warm = t.join("warm").display().to_string();
        let outs = vec!["_build".to_string(), "target".to_string()];

        assert_eq!(resolve(&path, "").unwrap(), (snap.display().to_string(), one.clone()));
        assert!(named_by(&resolve(&path, "").unwrap().0, &one));

        // not boxed: hard links; an output PATH lacks starts cold with its reason
        let p = pin(&warm, &r, "", &path, &outs, false);
        assert_eq!(p.name.as_deref(), Some(one.as_str()));
        assert_eq!(p.warm, vec!["_build".to_string()]);
        assert_eq!(p.notes.len(), 1, "{:?}", p.notes);
        assert!(p.notes[0].starts_with("target starts empty:"), "{:?}", p.notes);
        let pinned = format!("{warm}/{one}/_build/sub/A.agdai");
        assert_eq!(std::fs::metadata(&pinned).unwrap().ino(), std::fs::metadata(snap.join("_build/sub/A.agdai")).unwrap().ino());
        // boxed: a copy of its own
        let b = pin(&warm, &r, "", &path, &outs, true);
        assert_eq!(b.name, Some(format!("{one}-box")));
        assert_ne!(std::fs::metadata(format!("{warm}/{one}-box/_build/sub/A.agdai")).unwrap().ino(), std::fs::metadata(&pinned).unwrap().ino());
        // pinned once: the runner replacing its folder changes nothing pinned
        std::fs::remove_file(snap.join("_build/sub/A.agdai")).unwrap();
        std::fs::write(snap.join("_build/sub/A.agdai"), "newer").unwrap();
        assert_eq!(pin(&warm, &r, "", &path, &outs, false).warm, vec!["_build".to_string()]);
        assert_eq!(std::fs::read_to_string(&pinned).unwrap(), "iface");

        // a snapshot with a file its owner cannot write (an installed, read-only build): copied, the copy writable,
        // the snapshot's mode untouched
        let ro = t.join("ro").join(&one);
        std::fs::create_dir_all(ro.join("_build")).unwrap();
        std::fs::write(ro.join("_build/R.agdai"), "r").unwrap();
        std::fs::set_permissions(ro.join("_build/R.agdai"), std::fs::Permissions::from_mode(0o444)).unwrap();
        std::fs::write(ro.join("commit"), &one).unwrap();
        let warm4 = t.join("warm4").display().to_string();
        assert_eq!(pin(&warm4, &r, "", &ro.display().to_string(), &outs[..1], false).warm, vec!["_build".to_string()]);
        let copy = std::fs::metadata(format!("{warm4}/{one}/_build/R.agdai")).unwrap();
        assert_ne!(copy.ino(), std::fs::metadata(ro.join("_build/R.agdai")).unwrap().ino());
        assert_eq!(copy.permissions().mode() & 0o777, 0o644);
        assert_eq!(std::fs::metadata(ro.join("_build/R.agdai")).unwrap().permissions().mode() & 0o777, 0o444);

        // outputs inside folders (Tovrin's `agda/_build`, `vendor/agda2hs-base/_build`), linked and copied
        let nest = t.join("nest").join(&one);
        for o in ["agda/_build", "vendor/b/_build"] {
            std::fs::create_dir_all(nest.join(o)).unwrap();
            std::fs::write(nest.join(o).join("M.agdai"), o).unwrap();
        }
        std::fs::write(nest.join("commit"), &one).unwrap();
        let nested = vec!["agda/_build".to_string(), "vendor/b/_build".to_string()];
        for boxed in [false, true] {
            let w = t.join(format!("warm-nest-{boxed}")).display().to_string();
            let n = pin(&w, &r, "", &nest.display().to_string(), &nested, boxed);
            assert_eq!(n.warm, nested, "{:?}", n.notes);
            let name = n.name.unwrap();
            assert_eq!(std::fs::read_to_string(format!("{w}/{name}/vendor/b/_build/M.agdai")).unwrap(), "vendor/b/_build");
        }

        // the runner's snapshot of several repositories: PATH/REPO/... and `REPO=SHA` lines; this repository's part, copied
        // (a snapshot named by its build, not by a commit)
        let multi = t.join("snap").join("S-9#130");
        std::fs::create_dir_all(multi.join("tovrin/agda/core/_build")).unwrap();
        std::fs::write(multi.join("tovrin/agda/core/_build/C.agdai"), "c").unwrap();
        std::fs::write(multi.join("commit"), format!("tovrin={one}\np4={}\n", "a".repeat(40))).unwrap();
        std::os::unix::fs::symlink(&multi, t.join("snap/green")).unwrap();
        let green = t.join("snap/green").display().to_string();
        assert_eq!(resolve(&green, "tovrin").unwrap(), (multi.join("tovrin").display().to_string(), one.clone()));
        assert!(resolve(&green, "go").unwrap_err().contains("no commit of \"go\""));
        assert!(resolve(&green, "").is_err(), "a snapshot of several repositories needs the repository's name");
        let w7 = t.join("warm7").display().to_string();
        let m = pin(&w7, &r, "tovrin", &green, &["agda/core/_build".to_string()], false);
        assert_eq!((m.name.as_deref(), m.warm.len()), (Some(one.as_str()), 1), "{:?}", m.notes);
        assert_ne!(std::fs::metadata(format!("{w7}/{one}/agda/core/_build/C.agdai")).unwrap().ino(), std::fs::metadata(multi.join("tovrin/agda/core/_build/C.agdai")).unwrap().ino());

        // no room for a copy: cold, nothing copied; a link needs none
        let tiny = |_: &str| Some((1000u64, 4096u64));
        let warm5 = t.join("warm5").display().to_string();
        let n = pin_with(&warm5, &r, "", &path, &outs[..1], true, &tiny);
        assert!(n.warm.is_empty() && n.notes[0].starts_with("_build starts empty: no room for the copy"), "{:?}", n.notes);
        assert!(!Path::new(&format!("{warm5}/{one}-box/_build")).exists());
        assert_eq!(pin_with(&t.join("warm6").display().to_string(), &r, "", &path, &outs[..1], false, &tiny).warm, vec!["_build".to_string()], "linked: no room needed");
        assert!(no_room(10, Some((100, 50))).is_none() && no_room(60, Some((100, 50))).is_some() && no_room(1, None).is_some());

        // a folder not named by its commit (one still built in): copied, never linked
        let live = t.join("live");
        std::fs::create_dir_all(live.join("_build")).unwrap();
        std::fs::write(live.join("_build/X.agdai"), "x").unwrap();
        std::fs::write(live.join("commit"), &two).unwrap();
        let warm2 = t.join("warm2").display().to_string();
        let l = pin(&warm2, &r, "", &live.display().to_string(), &outs, false);
        assert_eq!(l.warm, vec!["_build".to_string()]);
        assert_ne!(std::fs::metadata(format!("{warm2}/{two}/_build/X.agdai")).unwrap().ino(), std::fs::metadata(live.join("_build/X.agdai")).unwrap().ino());

        // unreadable by others: cold, permissions untouched
        let hid = t.join("hid").join(&two);
        std::fs::create_dir_all(hid.join("_build")).unwrap();
        std::fs::write(hid.join("_build/S.agdai"), "s").unwrap();
        std::fs::set_permissions(hid.join("_build/S.agdai"), std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::write(hid.join("commit"), &two).unwrap();
        let h = pin(&t.join("warm3").display().to_string(), &r, "", &hid.display().to_string(), &outs[..1], false);
        assert!(h.warm.is_empty());
        assert!(h.notes[0].contains("is not readable by others"), "{:?}", h.notes);
        assert_eq!(std::fs::metadata(hid.join("_build/S.agdai")).unwrap().permissions().mode() & 0o777, 0o600);

        // nothing to pin: every output cold, never an error
        for (bad, why) in [(t.join("none").display().to_string(), "No such file"), (t.join("repo").display().to_string(), "commit")] {
            let c = pin(&warm, &r, "", &bad, &outs, false);
            assert_eq!((c.name, c.warm.len(), c.notes.len()), (None, 0, 2));
            assert!(c.notes[0].contains(why), "{:?}", c.notes);
        }
        std::fs::write(t.join("live/commit"), "0".repeat(40)).unwrap();
        assert!(pin(&warm, &r, "", &live.display().to_string(), &outs, false).notes[0].contains("is not in"));

        // what is touched: BASE's files that differ from the warm commit, never one BASE lacks
        let mut c = changed(&r, &one, &two).unwrap();
        c.sort();
        assert_eq!(c, vec!["b.agda".to_string(), "c.agda".to_string()]);
        assert!(changed(&r, &two, &two).unwrap().is_empty());
        remove(&t.display().to_string()).unwrap();
    }
}
