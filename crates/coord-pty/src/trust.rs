//! Claude Code's popups asking to trust a folder: Yes typed for a folder roster declared (`--trust`), the owner told of any other.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The popups' heads, as Claude Code 2.1.332 draws them; the folder is on the line under the head.
const HEADS: [&str; 4] = ["Adding a new directory:", "Moving to a new directory:", "Now in a new directory:", "Accessing workspace:"];
/// Their Yes options; the No beside each ("No, exit", "No, stay put", "No, leave it out") is not chosen.
const YES: [&str; 4] = ["Yes, add it", "Yes, I trust this folder", "Yes, move here", "Yes, trust it and apply them"];

/// How long a popup stands before its answer is typed: Claude Code drops keys that come as it is drawn.
pub const ANSWER_AFTER: Duration = Duration::from_millis(1000);
/// How long a popup answered still stands before the owner is told the answer did not take.
pub const STILL_AFTER: Duration = Duration::from_secs(10);

/// A trust popup on the screen: its head, its folder, the keys that choose Yes from where its pointer is, and whether
/// the folder's line may go on past the screen's width (`cut`: never answered).
#[derive(Debug, PartialEq, Eq)]
pub struct Ask {
    pub head: String,
    pub dir: String,
    pub keys: Option<Vec<u8>>,
    pub cut: bool,
}

/// What the holder does about a popup.
#[derive(Debug, PartialEq, Eq)]
pub enum Act {
    /// Type KEYS, the folder (HEAD, DIR) being declared.
    Press(Vec<u8>, String, String),
    /// Tell the owner (`Request::Popup`): TITLE and the screen's last lines.
    Tell(String, String),
}

/// How the lines under a popup's folder begin in Claude Code 2.1.332; any other is the folder's own, wrapped.
const UNDER: [&str; 6] = ["This session hasn", "This directory is part of", "Its settings declare", "Quick safety check", "Claude Code", "Its skills"];

fn clean(l: &str) -> &str {
    l.trim().trim_matches(['│', '╭', '╮', '╰', '╯', '─', ' ']).trim()
}

/// The option a line holds and whether the pointer is on it.
fn option(l: &str) -> (&str, bool) {
    let l = clean(l);
    match l.strip_prefix('❯').or_else(|| l.strip_prefix('>')).or_else(|| l.strip_prefix('›')) {
        Some(rest) => (rest.trim(), true),
        None => (l, false),
    }
}

/// Whether a line reads as an option of any dialog: a pointer, a number, or a Yes or No of these popups.
fn any_option(l: &str) -> bool {
    let (o, pointed) = option(l);
    let numbered = o.split_once('.').is_some_and(|(n, _)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
    pointed || numbered || YES.contains(&o) || o.starts_with("No, ")
}

/// The keys' hint Claude Code closes these dialogs with.
fn footer(l: &str) -> bool {
    l.contains("Enter to confirm") || l.contains("again to exit")
}

/// The trust popup on screen TEXT, only as Claude Code draws its own: at the foot of the screen, closed by its footer,
/// its Yes and its No the last lines above that footer, and no other option of any dialog after its head. Text the
/// session prints (a brief that quotes a popup, a fake one over a real prompt) is never one.
pub fn ask(text: &str) -> Option<Ask> {
    let lines: Vec<&str> = text.lines().collect();
    let last = lines.iter().rposition(|l| !clean(l).is_empty())?;
    if !footer(clean(lines[last])) {
        return None;
    }
    let (at, head) = (0..last).rev().find_map(|i| HEADS.iter().find(|h| clean(lines[i]).starts_with(**h)).map(|h| (i, *h)))?;
    let opts: Vec<usize> = (at + 1..last).filter(|i| any_option(lines[*i])).collect();
    let tail: Vec<usize> = (at + 1..last).filter(|i| !clean(lines[*i]).is_empty()).collect();
    // exactly two options, the last two lines with anything on them, one its Yes and one its No
    let [a, b] = opts[..] else { return None };
    if tail[tail.len().saturating_sub(2)..] != [a, b] {
        return None;
    }
    let ((oa, pa), (ob, pb)) = (option(lines[a]), option(lines[b]));
    let yes_first = match (YES.contains(&oa), YES.contains(&ob)) {
        (true, false) if ob.starts_with("No, ") => true,
        (false, true) if oa.starts_with("No, ") => false,
        _ => return None,
    };
    let same = clean(lines[at])[head.len()..].trim();
    let (dir, under) = if same.is_empty() {
        let d = (at + 1..a).find(|i| !clean(lines[*i]).is_empty())?;
        (clean(lines[d]), lines.get(d + 1).map_or("", |l| clean(l)))
    } else {
        (same, lines.get(at + 1).map_or("", |l| clean(l)))
    };
    if !dir.starts_with(['/', '~']) {
        return None;
    }
    let cut = !under.is_empty() && !UNDER.iter().any(|u| under.starts_with(u)) && !any_option(under);
    // the pointer on Yes: Enter; on the No beside it: one step towards Yes, then Enter
    let keys = match (pa, pb, yes_first) {
        (true, false, true) | (false, true, false) => Some(b"\r".to_vec()),
        (true, false, false) => Some(b"\x1b[B\r".to_vec()),
        (false, true, true) => Some(b"\x1b[A\r".to_vec()),
        _ => None,
    };
    Some(Ask { head: head.to_string(), dir: dir.to_string(), keys, cut })
}

/// A trust popup [`ask`] does not take, as told to the owner and never answered: a head, its Yes and its No, and a footer
/// under them, with no input box after the head (Claude Code draws a dialog in its place, while text it printed sits over
/// the box). Its title and the screen's last lines.
pub fn drawn(text: &str) -> Option<(String, String)> {
    if ask(text).is_some() {
        return None;
    }
    let lines: Vec<&str> = text.lines().collect();
    let (at, head) = (0..lines.len()).rev().find_map(|i| HEADS.iter().find(|h| clean(lines[i]).starts_with(**h)).map(|h| (i, *h)))?;
    let after = &lines[at + 1..];
    let yes = after.iter().position(|l| YES.contains(&option(l).0))?;
    let no = after.iter().position(|l| option(l).0.starts_with("No, "))?;
    after[yes.max(no)..].iter().position(|l| footer(l))?;
    if after.iter().any(|l| clean(l) == "❯" || l.contains("? for shortcuts")) {
        return None;
    }
    let dir = after.iter().map(|l| clean(l)).find(|l| !l.is_empty()).unwrap_or_default();
    let screen = lines[lines.len().saturating_sub(20)..].iter().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n");
    Some((format!("{head} {dir} (not answered: more is drawn around it than Claude Code's own dialog; it waits for an answer in its terminal)"), screen.trim().to_string()))
}

/// The folder a popup names, `~` as the agent's HOME: none when it holds `.` or `..`, a `~` with no home given, or it
/// is not there, so a path that is not one folder for certain is never declared.
fn real(dir: &str, home: Option<&Path>) -> Option<PathBuf> {
    let p = match dir.strip_prefix('~') {
        Some("") => home?.to_path_buf(),
        Some(rest) => home?.join(rest.strip_prefix('/')?),
        None => PathBuf::from(dir),
    };
    // as written: a path's components drop a `.` inside it
    if dir.split('/').any(|c| c == "." || c == "..") {
        return None;
    }
    std::fs::canonicalize(&p).ok()
}

/// The folders declared, and the popup standing with when it came and what was done about it.
pub struct Trust {
    declared: Vec<PathBuf>,
    home: Option<PathBuf>,
    standing: Option<(String, Instant)>,
    pressed: bool,
    told: bool,
}

impl Trust {
    /// The folders DECLARED, `~` in a popup read as HOME, the agent's (roster's `--home`).
    pub fn new(declared: &[String], home: Option<&str>) -> Trust {
        let home = home.map(PathBuf::from);
        Trust { declared: declared.iter().filter_map(|d| real(d, None)).collect(), home, standing: None, pressed: false, told: false }
    }

    /// Whether DIR is a declared folder or inside one.
    pub fn declared(&self, dir: &str) -> bool {
        real(dir, self.home.as_deref()).is_some_and(|p| self.declared.iter().any(|d| p.starts_with(d)))
    }

    /// Whether a trust popup was on the screen at the last look.
    pub fn standing(&self) -> bool {
        self.standing.is_some()
    }

    /// What to do about screen TEXT at NOW: Yes once for a declared folder, else the owner told once.
    pub fn look(&mut self, text: &str, now: Instant) -> Option<Act> {
        let Some(a) = ask(text) else {
            self.standing = None;
            return None;
        };
        let key = format!("{} {}", a.head, a.dir);
        if self.standing.as_ref().is_none_or(|(k, _)| *k != key) {
            (self.standing, self.pressed, self.told) = (Some((key.clone(), now)), false, false);
        }
        let stood = now.saturating_duration_since(self.standing.as_ref().map_or(now, |(_, at)| *at));
        if self.told || stood < ANSWER_AFTER {
            return None;
        }
        let screen = text.lines().map(str::trim_end).collect::<Vec<_>>();
        let screen = screen[screen.len().saturating_sub(20)..].join("\n").trim().to_string();
        if a.cut {
            self.told = true;
            return Some(Act::Tell(format!("{key}… (its folder goes on past the screen's width: Claude Code waits for an answer in its terminal)"), screen));
        }
        if !self.declared(&a.dir) {
            self.told = true;
            return Some(Act::Tell(format!("{key} (not a folder of the project: Claude Code waits for an answer in its terminal)"), screen));
        }
        if self.pressed {
            if stood < ANSWER_AFTER + STILL_AFTER {
                return None;
            }
            self.told = true;
            return Some(Act::Tell(format!("{key} (Yes was typed and the popup still stands)"), screen));
        }
        // a pointer elsewhere is someone choosing: left to them
        let keys = a.keys?;
        self.pressed = true;
        Some(Act::Press(keys, a.head, a.dir))
    }
}

/// Types KEYS on the terminal INPUT: the step first, then Enter, apart, as Claude Code reads a key at a time.
pub fn press(mut input: &std::fs::File, keys: &[u8]) {
    use std::io::Write;
    let (step, enter) = keys.split_at(keys.len().saturating_sub(1));
    if !step.is_empty() {
        let _ = input.write_all(step);
        std::thread::sleep(Duration::from_millis(150));
    }
    let _ = input.write_all(enter);
}

/// One line in `$COORD/pty/trusted.log`: NAME's popup for DIR answered Yes.
pub fn log(coord: &Path, name: &str, head: &str, dir: &str) {
    use std::io::Write;
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(coord.join("pty").join("trusted.log")) {
        let _ = writeln!(f, "{at} {name} answered Yes to {head} {dir} (declared)");
    }
}

#[cfg(test)]
#[path = "trust_tests.rs"]
mod tests;
