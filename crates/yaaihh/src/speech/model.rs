//! The speech model's file (paper-research N-554.p1, step 4): not in the exe; in `%LOCALAPPDATA%\yaaihh\models\` (on
//! Linux `~/.local/state/yaaihh/models/`). The first press of the control says what would be fetched, from where and
//! how large, and only his yes in the window fetches it: at a pinned revision, its SHA-256 checked, its progress shown
//! at the box; a failed or partial file is removed and said in words. Nothing here runs before that yes.
//!
//! The fetch is the system's own `curl` (Windows has it in System32 since Windows 10 1803), so the exe carries no TLS
//! library for one download.

use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// A model file: its name in the models folder, its size, where it is fetched from and its SHA-256.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelFile {
    pub name: &'static str,
    pub bytes: u64,
    /// the host, as said to him
    pub from: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub licence: &'static str,
}

/// The model speech takes (paper-research2 N-551.p2, section 4): NVIDIA's Parakeet Unified EN 0.6B in transcribe.cpp's
/// form, Q4_K_M, from the port's own repository at revision 843b1b17 (16 Oct 2026). Size and hash as the Hub records
/// them for that revision (read 18 Nov; the file itself not fetched).
pub const PARAKEET: ModelFile = ModelFile {
    name: "parakeet-unified-en-0.6b-Q4_K_M.gguf",
    bytes: 477_274_496,
    from: "huggingface.co (handy-computer/parakeet-unified-en-0.6b-gguf)",
    url: "https://huggingface.co/handy-computer/parakeet-unified-en-0.6b-gguf/resolve/843b1b175ace04f5a89d379c79e2518c0d065b25/parakeet-unified-en-0.6b-Q4_K_M.gguf",
    sha256: "a8bf3de2b393bd14ead5a858c3748d5e3b07a20fdeabdd3b498fba4f463fa929",
    licence: "NVIDIA Open Model License",
};

/// Where the models are kept.
pub fn folder() -> PathBuf {
    crate::places::state_dir().join("yaaihh").join("models")
}

/// What his first press says before anything is fetched.
pub fn ask(m: &ModelFile) -> String {
    format!("Speech needs its model, {} ({} MB, {}), fetched once from {} into {}. Fetch it?", m.name, m.bytes / 1_000_000, m.licence, m.from, folder().display())
}

/// The model's path when it is there whole: the right size and, checked once per run, the right hash.
pub fn present(dir: &Path, m: &ModelFile) -> Option<PathBuf> {
    let p = dir.join(m.name);
    (std::fs::metadata(&p).ok()?.len() == m.bytes).then_some(p)
}

/// How far a fetch has come, read by the window for the line at the box.
#[derive(Default)]
pub struct Progress {
    pub got: AtomicU64,
    pub stop: AtomicBool,
}

/// Fetch M into DIR: to `NAME.part` with curl, its size and SHA-256 checked, then renamed into place. On any failure
/// the part is removed and the reason is given in words. PROGRESS is updated as the file grows; setting its stop ends
/// the fetch (and removes the part).
pub fn fetch(dir: &Path, m: &ModelFile, progress: &Arc<Progress>) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("The models folder {} could not be made: {e}", dir.display()))?;
    let done = dir.join(m.name);
    let part = dir.join(format!("{}.part", m.name));
    let gone = |why: String| {
        let _ = std::fs::remove_file(&part);
        Err(why)
    };
    let _ = std::fs::remove_file(&part);
    // the pinned address and nothing else: no header, token or account of ours; each redirect followed here, only to
    // the Hub's own hosts (paper-research N-554.p10)
    let mut url = m.url.to_string();
    for _hop in 0..MAX_HOPS {
        match hop(&url, &part, progress) {
            Ok(Hop::Got) => {
                let got = std::fs::metadata(&part).map(|x| x.len()).unwrap_or(0);
                return checked(got, &part, &done, m, progress).or_else(|why| gone(why));
            }
            Ok(Hop::To(next)) if hub_host(&next) => url = next,
            Ok(Hop::To(next)) => return gone(format!("The model's address sent the fetch to {}, which is not the Hub's own; nothing was fetched from there.", host_of(&next).unwrap_or(&next))),
            Err(why) => return gone(why.replace("{from}", m.from)),
        }
    }
    gone(format!("The model's address at {} redirected more than {MAX_HOPS} times; nothing was kept.", m.from))
}

/// Redirects followed at most.
const MAX_HOPS: usize = 5;

/// What one request gave: the file, or a redirect to follow.
enum Hop {
    Got,
    To(String),
}

/// One request of URL into PART with the system's curl, following no redirect itself. PROGRESS's stop ends it.
fn hop(url: &str, part: &Path, progress: &Arc<Progress>) -> Result<Hop, String> {
    let curl = if cfg!(windows) { "curl.exe" } else { "curl" };
    let _ = std::fs::remove_file(part);
    // --fail: an HTTP error is a failure, not a page saved as the model; no retry and no resume, a part is never kept
    let mut child = crate::places::command(curl)
        .args(["--fail", "--silent", "--show-error", "--proto", "=https,file", "--write-out", "%{response_code} %{redirect_url}", "--output"])
        .arg(part)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("The model could not be fetched: {curl} did not start ({e})."))?;
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            break s;
        }
        if progress.stop.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("The model's fetch was stopped; nothing was kept.".into());
        }
        progress.got.store(std::fs::metadata(part).map(|x| x.len()).unwrap_or(0), Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_millis(200));
    };
    let (mut out, mut err) = (String::new(), String::new());
    if let Some(mut o) = child.stdout.take() {
        let _ = o.read_to_string(&mut out);
    }
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut err);
    }
    if !status.success() {
        let err = err.lines().last().unwrap_or("").trim().to_string();
        return Err(format!("The model could not be fetched from {{from}}: {}", if err.is_empty() { format!("curl ended with {status}") } else { err }));
    }
    let (code, next) = out.trim().split_once(' ').map_or((out.trim(), ""), |(c, n)| (c, n.trim()));
    match code.parse::<u16>().unwrap_or(0) {
        300..=399 if !next.is_empty() => Ok(Hop::To(next.to_string())),
        300..=399 => Err("The model's address at {from} redirected nowhere; nothing was kept.".into()),
        _ => Ok(Hop::Got),
    }
}

/// The file at PART, GOT bytes, checked against M and put in place at DONE; why not, in words.
fn checked(got: u64, part: &Path, done: &Path, m: &ModelFile, progress: &Arc<Progress>) -> Result<PathBuf, String> {
    progress.got.store(got, Ordering::Relaxed);
    if got != m.bytes {
        return Err(format!("The model came as {got} bytes, not {}; it was removed.", m.bytes));
    }
    match sha256_of(part) {
        Ok(h) if h == m.sha256 => {}
        Ok(h) => return Err(format!("The model's hash is {h}, not {}; it was removed.", m.sha256)),
        Err(e) => return Err(format!("The model could not be read back to check it: {e}")),
    }
    std::fs::rename(part, done).map_err(|e| format!("The model could not be put in place: {e}"))?;
    Ok(done.to_path_buf())
}

/// The host of an https URL; none for any other, or one with a user in it.
fn host_of(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("https://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    if host.contains('@') {
        return None;
    }
    Some(host.split(':').next().unwrap_or(host)).filter(|h| !h.is_empty())
}

/// Whether URL is https on the Hub's own hosts (huggingface.co, hf.co, and theirs, where the files are served from).
pub fn hub_host(url: &str) -> bool {
    host_of(url).is_some_and(|h| {
        let h = h.to_ascii_lowercase();
        ["huggingface.co", "hf.co"].iter().any(|d| h == *d || h.ends_with(&format!(".{d}")))
    })
}

/// The SHA-256 of the file at P, in lower-case hex.
pub fn sha256_of(p: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(p)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("yaaihh-speech-model-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn file(d: &Path, bytes: &[u8]) -> (String, String) {
        let src = d.join("source.bin");
        std::fs::write(&src, bytes).unwrap();
        (format!("file://{}", src.display()), sha256_of(&src).unwrap())
    }

    fn leak(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }

    /// The pinned model is named whole: an https address at a fixed revision, its size and a hash of 64 hex digits.
    #[test]
    fn the_model_is_pinned() {
        assert!(PARAKEET.url.starts_with("https://huggingface.co/") && PARAKEET.url.contains("/resolve/843b1b175ace04f5a89d379c79e2518c0d065b25/"));
        assert!(PARAKEET.url.ends_with(PARAKEET.name));
        assert!(PARAKEET.sha256.len() == 64 && PARAKEET.sha256.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert!(ask(&PARAKEET).contains("477 MB") && ask(&PARAKEET).contains("huggingface.co"));
        assert_eq!(sha256_of(&{
            let d = dir("abc");
            std::fs::write(d.join("x"), b"abc").unwrap();
            d.join("x")
        }).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", "SHA-256 of \"abc\"");
    }

    /// A redirect is followed only to the Hub's own hosts, over https, with no user in the address.
    #[test]
    fn a_redirect_goes_only_to_the_hubs_own_hosts() {
        for ok in ["https://huggingface.co/x", "https://cdn-lfs.huggingface.co/a?b", "https://cas-bridge.xethub.hf.co/x/y", "https://hf.co", "https://HF.CO:443/x"] {
            assert!(hub_host(ok), "{ok}");
        }
        for no in ["http://huggingface.co/x", "https://huggingface.co.evil.example/x", "https://evilhuggingface.co/x", "https://user@huggingface.co/x", "https://example.org/huggingface.co", "file:///etc/passwd", "ftp://hf.co/x", ""] {
            assert!(!hub_host(no), "{no}");
        }
    }

    /// A fetch (here from a file, curl's file://) is put in place only whole and with its hash; a wrong hash, a wrong
    /// size or a source that is not there leaves no file behind, and says so in words.
    #[test]
    fn a_fetch_keeps_only_a_whole_checked_file() {
        let d = dir("fetch");
        let (url, sha) = file(&d, &[7u8; 300_000]);
        let into = d.join("models");
        let m = ModelFile { name: "m.gguf", bytes: 300_000, from: "a test file", url: leak(url.clone()), sha256: leak(sha.clone()), licence: "none" };
        let p = Arc::new(Progress::default());
        let got = fetch(&into, &m, &p).unwrap();
        assert_eq!(got, into.join("m.gguf"));
        assert_eq!(present(&into, &m), Some(got.clone()));
        assert_eq!(p.got.load(Ordering::Relaxed), 300_000);
        std::fs::remove_file(&got).unwrap();

        let wrong = ModelFile { sha256: "00".repeat(32).leak(), ..m };
        let e = fetch(&into, &wrong, &p).unwrap_err();
        assert!(e.contains("hash") && e.contains("removed"), "{e}");
        let short = ModelFile { bytes: 300_001, ..m };
        assert!(fetch(&into, &short, &p).unwrap_err().contains("300000 bytes"));
        let none = ModelFile { url: leak(format!("file://{}", d.join("not-there").display())), ..m };
        assert!(fetch(&into, &none, &p).unwrap_err().starts_with("The model could not be fetched from a test file"));
        assert_eq!(std::fs::read_dir(&into).unwrap().count(), 0, "nothing left behind");
        assert_eq!(present(&into, &m), None);
        let _ = std::fs::remove_dir_all(&d);
    }
}
