//! A place that is a branch of a git repository, such as a private one on GitHub: git's own push check is its one-writer rule.

// The URL and branch are checked before git sees them, and come after `--`: neither can be read as an option. git
// runs in the C locale, with only the plain transports allowed (no `ext::`), never asking for a login, and is stopped
// with all it started when it gives no answer in time. A fetch brings the branch's last commit only, and is stopped
// once it has brought more than `most_fetch` bytes; the commit's tree is checked to hold only what a place writes,
// each file no larger than a place keeps, before any of it is read.

use crate::place::{Fetched, Head, MOST_PART, Part, PlaceError, RecordPlace, Text, check_order, text_name};
use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long one git command may take before it is stopped.
pub const TIMEOUT: Duration = Duration::from_secs(60);
/// The most bytes one fetch may bring into the store.
pub const MOST_FETCH: u64 = 1 << 30;

/// A branch of the repository at URL, kept through a bare store of its own at WORK; it takes no login and passes none.
#[derive(Debug, Clone)]
pub struct GitRepoPlace {
    url: String,
    work: PathBuf,
    branch: String,
    timeout: Duration,
    most_fetch: u64,
    env: Vec<(String, String)>,
}

/// The ref the fetched branch is kept under in WORK.
fn kept(branch: &str) -> String {
    format!("refs/place/{branch}")
}

/// Refuses a URL git could read as an option or as a transport that runs a command.
pub fn check_url(url: &str) -> Result<(), PlaceError> {
    let bad = |w: &str| Err(PlaceError::Io(format!("{url:?} is no place's URL: {w}")));
    if url.is_empty() || url.starts_with('-') {
        return bad("it is empty or begins with -");
    }
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return bad("it holds a space or a control character");
    }
    if url.contains("::") {
        return bad("a transport helper (`ext::`, `fd::`) is not a place");
    }
    Ok(())
}

/// Refuses a branch that is not a plain ref name, or that git could read as an option.
pub fn check_branch(b: &str) -> Result<(), PlaceError> {
    let plain = b.bytes().all(|c| c.is_ascii_alphanumeric() || b"._/-".contains(&c));
    let ok = plain && (1..=200).contains(&b.len()) && !b.starts_with(['-', '/', '.']) && !b.ends_with(['/', '.']) && !b.ends_with(".lock") && !b.contains("..") && !b.contains("//") && !b.contains("/.");
    if ok { Ok(()) } else { Err(PlaceError::Io(format!("{b:?} is no branch a place may be on"))) }
}

impl GitRepoPlace {
    /// The place on BRANCH of URL, its store at WORK, made when it is not there.
    pub fn open(url: &str, work: impl Into<PathBuf>, branch: &str) -> Result<GitRepoPlace, PlaceError> {
        check_url(url)?;
        check_branch(branch)?;
        let p = GitRepoPlace { url: url.to_string(), work: work.into(), branch: branch.to_string(), timeout: TIMEOUT, most_fetch: MOST_FETCH, env: Vec::new() };
        if !p.work.join("HEAD").exists() {
            std::fs::create_dir_all(&p.work).map_err(|e| PlaceError::Io(format!("{}: {e}", p.work.display())))?;
            p.git(&["init", "--bare", "-q"], None).map_err(PlaceError::Io)?;
        }
        Ok(p)
    }

    /// The same place, each git command stopped after TIMEOUT.
    pub fn with_timeout(mut self, timeout: Duration) -> GitRepoPlace {
        self.timeout = timeout;
        self
    }

    /// The same place, each fetch stopped once it has brought MOST bytes.
    pub fn with_most_fetch(mut self, most: u64) -> GitRepoPlace {
        self.most_fetch = most;
        self
    }

    /// The bytes of the store's packs, where a fetch puts what it brings.
    fn packed(&self) -> u64 {
        let d = self.work.join("objects/pack");
        std::fs::read_dir(d).map(|es| es.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum()).unwrap_or(0)
    }

    /// git ARGS in the store, INPUT on its stdin: whether it succeeded, its stdout and its stderr.
    fn run(&self, args: &[&str], input: Option<&[u8]>) -> Result<(bool, Vec<u8>, String), String> {
        self.run_bounded(args, input, None)
    }

    /// As `run`, stopped too once the store's packs grew by more than GROW bytes.
    fn run_bounded(&self, args: &[&str], input: Option<&[u8]>, grow: Option<u64>) -> Result<(bool, Vec<u8>, String), String> {
        let before = grow.map(|_| self.packed());
        let what = || format!("git {}", args.join(" "));
        let mut c = Command::new("git");
        c.arg("--git-dir").arg(&self.work).args(args);
        // never asks for a login: whatever the machine's git holds is used, and nothing of it passes through here
        c.env("GIT_TERMINAL_PROMPT", "0").env("LC_ALL", "C").env("LANGUAGE", "C").env("GIT_ALLOW_PROTOCOL", "file:git:http:https:ssh");
        c.env("GIT_AUTHOR_NAME", "record").env("GIT_AUTHOR_EMAIL", "record@place.invalid").env("GIT_COMMITTER_NAME", "record").env("GIT_COMMITTER_EMAIL", "record@place.invalid");
        c.envs(self.env.iter().map(|(k, v)| (k, v)));
        c.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
        // a group of its own, so whatever it starts (ssh, a helper) is stopped with it
        c.process_group(0);
        let mut child = c.spawn().map_err(|e| format!("{}: {e}", what()))?;
        if let (Some(b), Some(mut i)) = (input.map(<[u8]>::to_vec), child.stdin.take()) {
            std::thread::spawn(move || i.write_all(&b));
        }
        let pipe = |r: Option<Box<dyn Read + Send>>| {
            std::thread::spawn(move || {
                let mut b = Vec::new();
                if let Some(mut r) = r {
                    let _ = r.read_to_end(&mut b);
                }
                b
            })
        };
        let (out, err) = (pipe(child.stdout.take().map(|o| Box::new(o) as _)), pipe(child.stderr.take().map(|e| Box::new(e) as _)));
        let end = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait().map_err(|e| format!("{}: {e}", what()))? {
                Some(s) => break s,
                None if Instant::now() >= end => {
                    // SAFETY: the group is the child's own, made at its spawn
                    unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
                    let _ = child.wait();
                    return Err(format!("{}: no answer in {} s; stopped", what(), self.timeout.as_secs_f64()));
                }
                None if grow.zip(before).is_some_and(|(g, b)| self.packed().saturating_sub(b) > g) => {
                    // SAFETY: the group is the child's own, made at its spawn
                    unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
                    let _ = child.wait();
                    return Err(format!("{}: the place sent more than {} bytes; stopped", what(), grow.unwrap_or(0)));
                }
                None => std::thread::sleep(Duration::from_millis(10)),
            }
        };
        // a fetch quicker than a look is held to the bound all the same
        if let (Some(g), Some(b)) = (grow, before)
            && self.packed().saturating_sub(b) > g
        {
            return Err(format!("{}: the place sent more than {g} bytes", what()));
        }
        let (out, err) = (out.join().unwrap_or_default(), err.join().unwrap_or_default());
        Ok((status.success(), out, String::from_utf8_lossy(&err).trim().to_string()))
    }

    /// git ARGS: its stdout, or its stderr when it fails.
    fn git(&self, args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>, String> {
        let (ok, out, err) = self.run(args, input)?;
        if ok { Ok(out) } else { Err(format!("git {}: {err}", args.join(" "))) }
    }

    fn text(&self, args: &[&str], input: Option<&[u8]>) -> Result<String, PlaceError> {
        self.git(args, input).map(|o| String::from_utf8_lossy(&o).trim().to_string()).map_err(PlaceError::Io)
    }

    /// The commit the branch stands at on URL, empty when it has none.
    fn remote_rev(&self) -> Result<String, PlaceError> {
        let out = self.text(&["ls-remote", "--", &self.url, &format!("refs/heads/{}", self.branch)], None)?;
        Ok(out.split_whitespace().next().unwrap_or("").to_string())
    }

    /// Refuses REV unless its tree holds only `head`, `open.part`, `parts/NNNNNNNNNNNN.part` numbered from 0 on
    /// and `texts/NAME`, each no larger than a place keeps; how many closed parts it holds.
    fn checked(&self, rev: &str) -> Result<u64, PlaceError> {
        let bad = |w: String| PlaceError::Io(format!("{rev}: {w}"));
        let (mut parts, mut head) = (Vec::new(), None);
        for l in self.text(&["ls-tree", "-r", "-l", rev], None)?.lines() {
            let (meta, path) = l.split_once('\t').ok_or_else(|| bad(format!("not a tree line: {l:?}")))?;
            let m: Vec<&str> = meta.split_whitespace().collect();
            let size: u64 = match m[..] {
                ["100644", "blob", _, s] => s.parse().map_err(|_| bad(format!("no size: {l:?}")))?,
                _ => return Err(bad(format!("{path} is not a plain file"))),
            };
            let most = if path == "head" { 32 } else { MOST_PART };
            if size > most {
                return Err(bad(format!("{path} is {size} bytes, more than the {most} a place keeps in one file")));
            }
            if path == "head" {
                head = Some(size);
            } else if let Some(n) = path.strip_prefix("parts/").and_then(|p| p.strip_suffix(".part")).filter(|n| n.len() == 12 && n.bytes().all(|b| b.is_ascii_digit())) {
                parts.push(n.parse::<u64>().unwrap_or(u64::MAX));
            } else if path != "open.part" && !path.strip_prefix("texts/").is_some_and(|n| text_name(n).is_ok()) {
                return Err(bad(format!("{path} is not a file a place writes")));
            }
        }
        head.ok_or_else(|| bad("no head".into()))?;
        let count: u64 = String::from_utf8_lossy(&self.blob(rev, "head")?).trim().parse().map_err(|_| bad("no count in head".into()))?;
        parts.sort_unstable();
        if parts.iter().copied().ne(0..count) {
            return Err(bad(format!("its head counts {count} closed parts, its tree holds {}", parts.len())));
        }
        Ok(count)
    }

    /// The file at PATH in REV, refused unread when larger than a place keeps.
    fn blob(&self, rev: &str, path: &str) -> Result<Vec<u8>, PlaceError> {
        let at = format!("{rev}:{path}");
        let size: u64 = self.text(&["cat-file", "-s", &at], None)?.parse().map_err(|_| PlaceError::Io(format!("{at}: no size")))?;
        if size > MOST_PART {
            return Err(PlaceError::Io(format!("{at} is {size} bytes, more than the {MOST_PART} a place keeps in one file")));
        }
        self.git(&["cat-file", "blob", &at], None).map_err(PlaceError::Io)
    }

    fn put_blob(&self, b: &[u8]) -> Result<String, PlaceError> {
        self.text(&["hash-object", "-w", "--stdin"], Some(b))
    }

    /// The entries of folder DIR at REV, as `mktree` takes them, by name; none when it has no such folder.
    fn entries(&self, rev: &str, dir: &str) -> Result<Vec<(String, String)>, PlaceError> {
        if rev.is_empty() {
            return Ok(Vec::new());
        }
        let out = self.text(&["ls-tree", rev, &format!("{dir}/")], None)?;
        Ok(out.lines().filter_map(|l| l.split_once('\t')).map(|(m, p)| (p.strip_prefix(&format!("{dir}/")).unwrap_or(p).to_string(), m.to_string())).collect())
    }

    /// The push of COMMIT to the branch: refused as stale only when git says the branch moved on; any other refusal, such as a server's hook, is the place failing.
    fn push(&self, base: &Head, commit: &str) -> Result<(), PlaceError> {
        let (ok, out, err) = self.run(&["push", "--porcelain", "--", &self.url, &format!("{commit}:refs/heads/{}", self.branch)], None).map_err(PlaceError::Io)?;
        if ok {
            return Ok(());
        }
        let out = String::from_utf8_lossy(&out);
        let refused = out.lines().find(|l| l.starts_with('!')).unwrap_or_default();
        let moved = refused.contains("[rejected]") && ["(non-fast-forward)", "(fetch first)", "(stale info)"].iter().any(|w| refused.contains(w));
        if moved {
            // stale whatever the new head reads as: another writer's push is the one fact known
            let now = self.head().unwrap_or_else(|e| Head { closed: 0, rev: format!("unread: {e}") });
            return Err(PlaceError::Stale { base: base.clone(), now });
        }
        Err(PlaceError::Io(format!("git push: {} {err}", refused.trim())))
    }
}

impl RecordPlace for GitRepoPlace {
    fn head(&self) -> Result<Head, PlaceError> {
        if self.remote_rev()?.is_empty() {
            return Ok(Head::default());
        }
        let refspec = format!("+refs/heads/{}:{}", self.branch, kept(&self.branch));
        let args = ["-c", "transfer.unpackLimit=1", "fetch", "-q", "--depth=1", "--no-tags", "--", &self.url, &refspec];
        let (ok, _, err) = self.run_bounded(&args, None, Some(self.most_fetch)).map_err(PlaceError::Io)?;
        if !ok {
            return Err(PlaceError::Io(format!("git fetch: {err}")));
        }
        let rev = self.text(&["rev-parse", "--verify", "-q", &kept(&self.branch)], None)?;
        Ok(Head { closed: self.checked(&rev)?, rev })
    }

    fn refuse_if_stale(&self, base: &Head) -> Result<(), PlaceError> {
        if self.remote_rev()? == base.rev { Ok(()) } else { Err(PlaceError::Stale { base: base.clone(), now: self.head()? }) }
    }

    fn take_parts(&mut self, base: &Head, closed: &[Part], open: &Part, texts: &[Text]) -> Result<Head, PlaceError> {
        self.refuse_if_stale(base)?;
        let count = check_order(base, closed, open)?;
        let line = |(name, mode): &(String, String)| format!("{mode}\t{name}\n");
        let mut parts: String = self.entries(&base.rev, "parts")?.iter().map(line).collect();
        for p in closed {
            parts += &format!("100644 blob {}\t{:012}.part\n", self.put_blob(&p.sealed)?, p.n);
        }
        let mut kept_texts = self.entries(&base.rev, "texts")?;
        for t in texts {
            text_name(&t.name)?;
            let blob = format!("100644 blob {}", self.put_blob(&t.sealed)?);
            kept_texts.retain(|(n, _)| *n != t.name);
            kept_texts.push((t.name.clone(), blob));
        }
        let parts_tree = self.text(&["mktree"], Some(parts.as_bytes()))?;
        let mut root = format!("040000 tree {parts_tree}\tparts\n100644 blob {}\topen.part\n100644 blob {}\thead\n", self.put_blob(&open.sealed)?, self.put_blob(format!("{count}\n").as_bytes())?);
        if !kept_texts.is_empty() {
            let texts_tree = self.text(&["mktree"], Some(kept_texts.iter().map(line).collect::<String>().as_bytes()))?;
            root += &format!("040000 tree {texts_tree}\ttexts\n");
        }
        let tree = self.text(&["mktree"], Some(root.as_bytes()))?;
        let msg = format!("parts to {count}");
        let mut args = vec!["commit-tree", &tree, "-m", &msg];
        if !base.rev.is_empty() {
            args.extend(["-p", &base.rev]);
        }
        let commit = self.text(&args, None)?;
        self.push(base, &commit)?;
        self.text(&["update-ref", &kept(&self.branch), &commit], None)?;
        Ok(Head { closed: count, rev: commit })
    }

    fn fetch_texts(&self, head: &Head) -> Result<Vec<Text>, PlaceError> {
        let mut out = Vec::new();
        for (name, _) in self.entries(&head.rev, "texts")? {
            if text_name(&name).is_ok() {
                out.push(Text { sealed: self.blob(&head.rev, &format!("texts/{name}"))?, name });
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    fn fetch_from(&self, from: u64) -> Result<Fetched, PlaceError> {
        let head = self.head()?;
        if head.rev.is_empty() {
            return Ok(Fetched { head, ..Fetched::default() });
        }
        let closed = (from..head.closed).map(|n| Ok(Part { n, sealed: self.blob(&head.rev, &format!("parts/{n:012}.part"))? })).collect::<Result<Vec<_>, PlaceError>>()?;
        let open = Some(Part { n: head.closed, sealed: self.blob(&head.rev, "open.part")? });
        Ok(Fetched { head, closed, open })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{key, record, scratch};
    use crate::handover::Handover;
    use crate::sync::{Sender, restore, take_over};

    /// A bare repository standing for the remote one, and its URL.
    fn remote(d: &std::path::Path) -> String {
        let r = d.join("remote.git");
        assert!(Command::new("git").args(["init", "--bare", "-q"]).arg(&r).status().unwrap().success());
        r.display().to_string()
    }

    fn part(n: u64, b: &[u8]) -> Part {
        Part { n, sealed: b.to_vec() }
    }

    #[test]
    fn parts_pushed_are_fetched_back_by_another_store() {
        let d = scratch("git");
        let url = remote(&d);
        let mut a = GitRepoPlace::open(&url, d.join("a"), "record").unwrap();
        let b = GitRepoPlace::open(&url, d.join("b"), "record").unwrap();
        assert_eq!(b.head().unwrap(), Head::default());
        let h1 = a.take_parts(&Head::default(), &[part(0, b"p0")], &part(1, b"o1"), &[]).unwrap();
        let h2 = a.take_parts(&h1, &[part(1, b"p1"), part(2, b"p2")], &part(3, b"o3"), &[]).unwrap();
        let f = b.fetch_from(1).unwrap();
        assert_eq!(f.head, h2);
        assert_eq!(f.closed, vec![part(1, b"p1"), part(2, b"p2")]);
        assert_eq!(f.open, Some(part(3, b"o3")));
    }

    #[test]
    fn a_push_from_behind_is_refused() {
        let d = scratch("gitstale");
        let url = remote(&d);
        let mut a = GitRepoPlace::open(&url, d.join("a"), "record").unwrap();
        let mut b = GitRepoPlace::open(&url, d.join("b"), "record").unwrap();
        let base = b.head().unwrap();
        let h = a.take_parts(&base, &[part(0, b"a")], &part(1, b"oa"), &[]).unwrap();
        assert_eq!(b.take_parts(&base, &[part(0, b"b")], &part(1, b"ob"), &[]), Err(PlaceError::Stale { base: base.clone(), now: h.clone() }));
        let h2 = a.take_parts(&h, &[], &part(1, b"oa2"), &[]).unwrap();
        assert!(matches!(b.take_parts(&h, &[], &part(1, b"ob2"), &[]), Err(PlaceError::Stale { .. })));
        assert_eq!(b.fetch_from(0).unwrap().head, h2);
    }

    #[test]
    fn git_alone_refuses_a_racing_writer_that_passed_the_head_check() {
        let d = scratch("gitrace");
        let url = remote(&d);
        let mut a = GitRepoPlace::open(&url, d.join("a"), "record").unwrap();
        let b = GitRepoPlace::open(&url, d.join("b"), "record").unwrap();
        let base = a.take_parts(&Head::default(), &[], &part(0, b"o"), &[]).unwrap();
        assert_eq!(b.head().unwrap(), base);
        let empty = b.text(&["mktree"], Some(b"")).unwrap();
        let theirs = b.text(&["commit-tree", &empty, "-p", &base.rev, "-m", "x"], None).unwrap();
        b.git(&["push", "-q", &url, &format!("{theirs}:refs/heads/record")], None).unwrap();
        let empty_a = a.text(&["mktree"], Some(b"")).unwrap();
        let ours = a.text(&["commit-tree", &empty_a, "-p", &base.rev, "-m", "y"], None).unwrap();
        assert!(matches!(a.push(&base, &ours), Err(PlaceError::Stale { .. })));
    }

    #[test]
    fn a_url_or_branch_git_could_read_as_an_option_is_refused_and_runs_nothing() {
        let d = scratch("gitinject");
        let mark = d.join("ran");
        let hostile = format!("--upload-pack=touch {}", mark.display());
        assert!(GitRepoPlace::open(&hostile, d.join("w"), "record").is_err());
        for url in ["-oProxyCommand=x", "ext::sh -c touch% x", "a\nb", ""] {
            assert!(GitRepoPlace::open(url, d.join("w"), "record").is_err(), "{url:?}");
        }
        for b in ["-x", "--receive-pack=x", "a..b", "a b", ".x", "x.lock", "x/"] {
            assert!(GitRepoPlace::open(&remote(&d), d.join("w"), b).is_err(), "{b:?}");
        }
        // even past the check, `--` keeps it a repository's name, not an option
        let p = GitRepoPlace { url: hostile, ..GitRepoPlace::open(&remote(&d), d.join("w"), "record").unwrap() };
        assert!(p.head().is_err());
        let mut p2 = p.clone();
        let o = Part { n: 0, sealed: b"o".to_vec() };
        assert!(p2.take_parts(&Head::default(), &[], &o, &[]).is_err());
        assert!(!mark.exists(), "git ran the hostile command");
    }

    #[test]
    fn a_push_a_server_hook_refuses_is_a_failing_place_not_another_writer() {
        let d = scratch("githook");
        let url = remote(&d);
        let hook = std::path::Path::new(&url).join("hooks/pre-receive");
        std::fs::write(&hook, "#!/bin/sh\necho no >&2\nexit 1\n").unwrap();
        std::fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let mut a = GitRepoPlace::open(&url, d.join("a"), "record").unwrap();
        let e = a.take_parts(&Head::default(), &[], &part(0, b"o"), &[]).unwrap_err();
        assert!(matches!(&e, PlaceError::Io(w) if w.contains("remote rejected")), "{e:?}");
    }

    #[test]
    fn a_git_that_gives_no_answer_is_stopped_in_time() {
        let d = scratch("gitslow");
        let mut p = GitRepoPlace::open("ssh://place.invalid/r.git", d.join("w"), "record").unwrap().with_timeout(Duration::from_secs(1));
        p.env.push(("GIT_SSH_COMMAND".into(), "sh -c 'sleep 30' x".into()));
        let t = Instant::now();
        let e = p.head().unwrap_err().to_string();
        assert!(e.contains("no answer in 1 s") && t.elapsed() < Duration::from_secs(10), "{e} after {:?}", t.elapsed());
    }

    /// A commit on the remote's branch `record` whose tree is FILES, each made by a shell line writing it to stdout.
    pub(crate) fn served(url: &str, files: &[(&str, &str)]) {
        let g = |args: &[&str], input: &str| {
            let o = Command::new("sh").arg("-c").arg(format!("{input} | git --git-dir \"$0\" {}", args.join(" "))).arg(url).output().unwrap();
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        };
        let (mut root, mut parts) = (String::new(), String::new());
        for (path, make) in files {
            let blob = g(&["hash-object", "-w", "--stdin"], make);
            match path.strip_prefix("parts/") {
                Some(n) => parts += &format!("100644 blob {blob}\t{n}\n"),
                None => root += &format!("100644 blob {blob}\t{path}\n"),
            }
        }
        if !parts.is_empty() {
            root += &format!("040000 tree {}\tparts\n", g(&["mktree"], &format!("printf '{}'", parts.replace('\t', "\\t").replace('\n', "\\n"))));
        }
        let tree = g(&["mktree"], &format!("printf '{}'", root.replace('\t', "\\t").replace('\n', "\\n")));
        let c = g(&["-c", "user.name=t", "-c", "user.email=t@t", "commit-tree", &tree, "-m", "x"], "true");
        g(&["update-ref", "refs/heads/record", &c], "true");
    }

    #[test]
    fn a_fetch_bringing_more_than_its_bound_is_refused() {
        let d = scratch("gitmuch");
        let url = remote(&d);
        served(&url, &[("head", "echo 1"), ("open.part", "echo o"), ("parts/000000000000.part", "head -c 3000000 /dev/urandom")]);
        let p = GitRepoPlace::open(&url, d.join("w"), "record").unwrap().with_most_fetch(1 << 20);
        let e = p.head().unwrap_err().to_string();
        assert!(e.contains("sent more than"), "{e}");
        assert!(GitRepoPlace::open(&url, d.join("w2"), "record").unwrap().head().is_ok());
    }

    #[test]
    fn a_tree_holding_what_no_place_writes_is_refused() {
        for (name, files) in [
            ("extra", vec![("head", "echo 0"), ("open.part", "echo o"), ("run.sh", "echo x")]),
            ("gap", vec![("head", "echo 2"), ("open.part", "echo o"), ("parts/000000000001.part", "echo p")]),
            ("nohead", vec![("open.part", "echo o")]),
        ] {
            let d = scratch(&format!("gitshape{name}"));
            let url = remote(&d);
            served(&url, &files);
            assert!(GitRepoPlace::open(&url, d.join("w"), "record").unwrap().head().is_err(), "{name}");
        }
    }

    #[test]
    fn texts_are_kept_across_commits() {
        let d = scratch("gittexts");
        let url = remote(&d);
        let mut a = GitRepoPlace::open(&url, d.join("a"), "record").unwrap();
        let t1 = Text { name: "t1".into(), sealed: b"one".to_vec() };
        let t2 = Text { name: "t2".into(), sealed: b"two".to_vec() };
        let h = a.take_parts(&Head::default(), &[part(0, b"p")], &part(1, b"o"), std::slice::from_ref(&t1)).unwrap();
        let h = a.take_parts(&h, &[], &part(1, b"o2"), std::slice::from_ref(&t2)).unwrap();
        let h = a.take_parts(&h, &[part(1, b"q")], &part(2, b"o3"), &[]).unwrap();
        let b = GitRepoPlace::open(&url, d.join("b"), "record").unwrap();
        assert_eq!(b.fetch_texts(&b.head().unwrap()).unwrap(), [t1, t2]);
        assert_eq!(b.fetch_from(0).unwrap().closed.len(), 2);
        assert_eq!(b.head().unwrap(), h);
    }

    #[test]
    fn a_record_handed_from_device_a_to_device_b_through_git() {
        let d = scratch("githand");
        let url = remote(&d);
        let rec = record(8000);
        let mut a = Sender::start(GitRepoPlace::open(&url, d.join("a"), "record").unwrap(), &key(), crate::PART_SIZE).unwrap();
        a.send(&rec[..rec.len() / 2]).unwrap();
        let lines = rec.iter().filter(|b| **b == b'\n').count() as u64;
        let last = Handover::new("b", lines).unwrap().line_text();
        let handed = [rec.clone(), last.into_bytes()].concat();
        a.hand_over(&handed).unwrap();
        let (mut b, got) = take_over(GitRepoPlace::open(&url, d.join("b"), "record").unwrap(), &key(), crate::PART_SIZE, "b", None).unwrap();
        assert_eq!(got, handed);
        let more = [handed.clone(), b"{\"after\":1}\n".to_vec()].concat();
        b.send(&more).unwrap();
        assert_eq!(restore(&GitRepoPlace::open(&url, d.join("c"), "record").unwrap(), &key(), None).unwrap().record, more);
    }
}
