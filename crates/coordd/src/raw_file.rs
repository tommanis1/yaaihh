//! A raw pane's page of a session file no gateway log or record reading covers (a Codex rollout, agy's transcript):
//! its own JSONL lines as written, paged by byte offset, so a pane that asks after `next` gets each line once.

use proxylog::RawPage;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};

/// Said over a Claude Code agent's own record when the gateway log the project names is not where coordd runs.
pub const LOG_NOT_HERE: &str = "The gateway's log is not where this project's server runs, so this is the agent's own session record of each request: system prompt, tools, messages, settings and the answer. It has no HTTP status.";

/// How many lines a page holds.
const PER_PAGE: usize = 200;

/// How far back from the end (or from BEFORE) a page looks for its lines.
const BACK: u64 = 4 << 20;

/// The page of the file at PATH: with AFTER the whole lines from that offset on; with BEFORE the last lines ending
/// there; with neither the last lines of the file. A line not yet ended is left for the next ask. A turn's SPAN has no
/// times in such a file, so its page is empty. Each head is the offset its line starts at.
pub fn page(path: &str, before: Option<u64>, after: Option<u64>, span: Option<(u64, u64)>) -> RawPage {
    let stem = std::path::Path::new(path).file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
    let about = format!("No gateway log holds this agent's requests, so these are the lines of its own session file as its harness writes them: {path}");
    let mut page = RawPage { session: format!("session {stem}"), about: Some(about), ..Default::default() };
    let Ok(mut f) = std::fs::File::open(path) else { return page };
    let len = f.metadata().map_or(0, |m| m.len());
    if span.is_some() {
        return page;
    }
    let lines = match after {
        Some(a) if a <= len => forward(&mut f, a),
        _ => back(&mut f, before.filter(|&b| b <= len).unwrap_or(len)),
    };
    page.start = match (after, lines.first()) {
        (Some(a), _) if a <= len => a,
        (_, Some((at, _, _))) => *at,
        _ => before.filter(|&b| b <= len).unwrap_or(len),
    };
    page.next = lines.last().map_or(page.start, |(_, end, _)| *end);
    for (at, _, l) in lines {
        page.heads.push(at);
        page.lines.push(l);
    }
    page
}

/// The whole lines of F from offset FROM, at most a page: each one's start, end and text.
fn forward(f: &mut std::fs::File, from: u64) -> Vec<(u64, u64, String)> {
    let mut out = Vec::new();
    if f.seek(SeekFrom::Start(from)).is_err() {
        return out;
    }
    let (mut r, mut at, mut buf) = (BufReader::new(f), from, Vec::new());
    while out.len() < PER_PAGE {
        buf.clear();
        match r.read_until(b'\n', &mut buf) {
            Ok(n) if n > 0 && buf.ends_with(b"\n") => {
                out.push((at, at + n as u64, shown(&buf)));
                at += n as u64;
            }
            _ => break,
        }
    }
    out
}

/// The last whole lines of F that end at or before offset TO, at most a page.
fn back(f: &mut std::fs::File, to: u64) -> Vec<(u64, u64, String)> {
    let from = to.saturating_sub(BACK);
    let mut buf = vec![0; (to - from) as usize];
    if f.seek(SeekFrom::Start(from)).is_err() || f.read_exact(&mut buf).is_err() {
        return Vec::new();
    }
    // a look that starts inside a line drops what is before its first end
    let mut i = if from == 0 { 0 } else { buf.iter().position(|&b| b == b'\n').map_or(buf.len(), |p| p + 1) };
    let mut out = Vec::new();
    while let Some(p) = buf[i..].iter().position(|&b| b == b'\n') {
        let end = i + p + 1;
        out.push((from + i as u64, from + end as u64, shown(&buf[i..end])));
        i = end;
    }
    out.split_off(out.len().saturating_sub(PER_PAGE))
}

/// A line's bytes as the pane shows them: without its end, cut as the proxy's lines are.
fn shown(b: &[u8]) -> String {
    proxylog::cut(String::from_utf8_lossy(b).trim_end_matches(['\n', '\r']).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn a_session_file_pages_by_offset_whole_lines_once() {
        let dir = std::env::temp_dir().join(format!("coordd-raw-file-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rollout-1.jsonl");
        let body: String = (0..250).map(|i| format!("{{\"n\":{i}}}\n")).collect();
        std::fs::write(&path, format!("{body}{{\"n\":250")).unwrap();
        let p = path.display().to_string();
        let last = page(&p, None, None, None);
        assert_eq!((last.lines.len(), last.lines[0].as_str(), last.lines[199].as_str()), (200, "{\"n\":50}", "{\"n\":249}"), "the newest whole lines");
        assert_eq!((last.session.as_str(), last.next as usize, last.heads[0]), ("session rollout-1", body.len(), last.start));
        assert!(last.about.unwrap().ends_with(&p));
        let none = page(&p, None, Some(last.next), None);
        assert!(none.lines.is_empty() && none.start == last.next && none.next == last.next, "a line not ended is not sent");
        std::fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(b"}\n{\"n\":251}\n").unwrap();
        let new = page(&p, None, Some(last.next), None);
        assert_eq!((new.start, new.lines.clone()), (last.next, vec!["{\"n\":250}".to_string(), "{\"n\":251}".to_string()]), "only what was added");
        assert!(page(&p, None, Some(new.next), None).lines.is_empty(), "nothing twice");
        let earlier = page(&p, Some(last.start), None, None);
        assert_eq!((earlier.lines.len(), earlier.start, earlier.next, earlier.lines[49].as_str()), (50, 0, last.start, "{\"n\":49}"));
        assert!(page(&p, None, None, Some((0, u64::MAX))).lines.is_empty(), "no times to take a turn by");
        let gone = page(&dir.join("none.jsonl").display().to_string(), None, None, None);
        assert!(gone.lines.is_empty() && gone.next == 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
