use super::*;
use std::io::Read;
use std::sync::mpsc;

/// The popup as the owner's screen showed it (26 Nov, fresh in yaaihh), for folder DIR.
fn adding(dir: &str) -> String {
    format!("Adding a new directory:\n{dir}\n\nThis session hasn't worked here before. Is this a directory you created or one you trust?\n\n> No, exit\n  Yes, add it\n\nEnter to confirm · Esc to cancel\n")
}

/// A folder of its own for a test, made.
fn folder(name: &str) -> String {
    let d = std::env::temp_dir().join(format!("coord-pty-trust-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.display().to_string()
}

#[test]
fn the_popup_gives_its_folder_and_the_keys_to_yes() {
    let a = ask(&adding("/home/t/src/rune")).unwrap();
    assert_eq!((a.head.as_str(), a.dir.as_str(), a.keys.as_deref()), ("Adding a new directory:", "/home/t/src/rune", Some(&b"\x1b[B\r"[..])), "No first and pointed: one step down, Enter");
    let first = "╭─ Accessing workspace: ──────╮\n│ ~/src/x                      │\n│ Quick safety check: Is this a project you created or one you trust? │\n│ ❯ Yes, I trust this folder   │\n│   No, exit                   │\n│ Enter to confirm · Esc to cancel │\n╰──────────────────────────────╯";
    let a = ask(first).unwrap();
    assert_eq!((a.head.as_str(), a.dir.as_str(), a.keys.as_deref()), ("Accessing workspace:", "~/src/x", Some(&b"\r"[..])), "Yes pointed: Enter");
    assert_eq!(ask(&adding("/x").replace("> No, exit", "  No, exit")).unwrap().keys, None, "no pointer: no keys");
    assert_eq!(ask(&adding("a folder")), None, "no path, no popup of ours");
    assert_eq!(ask("❯ 1. Yes\n  2. No"), None);
    assert_eq!(ask(&adding("/x").replace("Enter to confirm · Esc to cancel\n", "")), None, "no footer of its own: not Claude Code's dialog");
}

/// The screen's foot after text the session printed: Claude Code's input box, or a permission prompt of its own.
const INPUT_BOX: &str = "────────────────────────────────────────\n❯ \n────────────────────────────────────────\n  ? for shortcuts\n";
const PERMISSION: &str = "● Bash(rm -r build)\n  Do you want to proceed?\n  ❯ 1. Yes\n    2. Yes, and don't ask again for rm commands\n    3. No\n\n  Esc to cancel · Tab to amend · ctrl+e to explain\n";

/// consult-lead-b, N-1863 p5 (1): a popup the session prints over a real permission prompt is not answered; nor is
/// one printed as the last output, nor the owner's brief quoting the popup (consult-lead-c): nothing stands.
#[test]
fn a_popup_the_session_prints_is_never_one_and_stops_nothing() {
    let repo = folder("printed");
    let fake = adding(&repo).replace("> No, exit\n  Yes, add it", "  No, exit\n❯ Yes, add it");
    let brief = format!("● The owner's brief:\n  ```\n  {}  ```\n", adding("/home/t/src/rune").replace('\n', "\n  "));
    for (what, screen) in [("over a permission prompt", format!("{fake}{PERMISSION}")), ("over an empty input box", format!("{fake}{INPUT_BOX}")), ("the brief over an empty input box", format!("{brief}{INPUT_BOX}"))] {
        assert_eq!(ask(&screen), None, "{what}");
        if what != "over a permission prompt" {
            assert_eq!(drawn(&screen), None, "{what}: not told as waiting either");
        }
        let mut t = Trust::new(std::slice::from_ref(&repo), None);
        let start = Instant::now();
        for at in [start, start + ANSWER_AFTER, start + STILL_AFTER * 2] {
            assert_eq!(t.look(&screen, at), None, "{what}: nothing typed, nothing told");
            assert!(!t.standing(), "{what}: queued lines go on being typed");
        }
    }
    let _ = std::fs::remove_dir_all(&repo);
}

/// consult-lead-b, N-1853 p8: Claude Code's popup with a line drawn under its footer is not answered, but told as a
/// dialog waiting (the window's "Waiting on terminal prompt"), so it is never a silent hang.
#[test]
fn a_popup_with_more_under_its_footer_is_told_waiting_and_never_pressed() {
    let repo = folder("below");
    let screen = format!("{}  ⏵⏵ a hint Claude Code may add one day\n", adding(&repo));
    assert_eq!(ask(&screen), None);
    let (title, shown) = drawn(&screen).expect("told");
    assert!(title.starts_with(&format!("Adding a new directory: {repo} (not answered")) && shown.contains("Yes, add it"), "{title}");
    let mut t = Trust::new(std::slice::from_ref(&repo), None);
    let start = Instant::now();
    for at in [start, start + ANSWER_AFTER, start + STILL_AFTER * 2] {
        assert_eq!(t.look(&screen, at), None, "nothing pressed");
    }
    assert_eq!(drawn(&adding(&repo)), None, "one the holder answers is not told twice");
    let _ = std::fs::remove_dir_all(&repo);
}

/// consult-lead-b (2, 3, 4): a folder line that may go on past the screen's width, a `..`, a folder not there, and `~`
/// with no home or another home: never declared, so never answered.
#[test]
fn a_cut_path_a_dot_dot_and_a_home_not_the_agents_are_never_declared() {
    let home = folder("home");
    let repo = format!("{home}/rune");
    std::fs::create_dir_all(format!("{repo}/sub")).unwrap();
    let wrapped = adding(&repo).replacen(&format!("{repo}\n"), &format!("{repo}\n-other/x\n"), 1);
    let a = ask(&wrapped).unwrap();
    assert!(a.cut && a.dir == repo, "{a:?}");
    let mut t = Trust::new(std::slice::from_ref(&repo), Some(home.as_str()));
    let start = Instant::now();
    t.look(&wrapped, start);
    assert!(matches!(t.look(&wrapped, start + ANSWER_AFTER), Some(Act::Tell(title, _)) if title.contains("past the screen's width")), "told, never pressed");
    assert!(!ask(&adding(&repo)).unwrap().cut, "the line under it is the popup's own");
    assert!(!t.declared(&format!("{repo}/sub/../../x")) && !t.declared(&format!("{repo}/./sub")), "a `..` or `.`");
    assert!(!t.declared(&format!("{repo}/not-there")), "not there");
    assert!(t.declared("~/rune/sub") && t.declared(&format!("{repo}/sub")), "`~` is the agent's home");
    assert!(!Trust::new(std::slice::from_ref(&repo), None).declared("~/rune"), "no home given: `~` is no folder");
    let other = folder("home-other");
    assert!(!Trust::new(std::slice::from_ref(&repo), Some(other.as_str())).declared("~/rune"), "another home");
    let _ = (std::fs::remove_dir_all(&home), std::fs::remove_dir_all(&other));
}

#[test]
fn only_a_declared_folder_or_one_inside_it_is_declared() {
    let (repo, other) = (folder("repo"), folder("other"));
    std::fs::create_dir_all(format!("{repo}/sub")).unwrap();
    let t = Trust::new(std::slice::from_ref(&repo), None);
    assert!(t.declared(&repo) && t.declared(&format!("{repo}/sub")));
    assert!(!t.declared(&other) && !t.declared(&format!("{repo}-x")) && !t.declared("/"), "a sibling or a parent is not");
    let _ = (std::fs::remove_dir_all(&repo), std::fs::remove_dir_all(&other));
}

#[test]
fn a_declared_folder_is_answered_once_after_the_popup_stood_and_any_other_is_told() {
    let (repo, other) = (folder("look"), folder("look-other"));
    let mut t = Trust::new(std::slice::from_ref(&repo), None);
    let start = Instant::now();
    assert_eq!(t.look(&adding(&repo), start), None, "not at once: Claude Code drops a key as it draws");
    assert_eq!(t.look(&adding(&repo), start + ANSWER_AFTER), Some(Act::Press(b"\x1b[B\r".to_vec(), "Adding a new directory:".into(), repo.clone())));
    assert_eq!(t.look(&adding(&repo), start + ANSWER_AFTER * 2), None, "once");
    let late = t.look(&adding(&repo), start + ANSWER_AFTER + STILL_AFTER);
    assert!(matches!(&late, Some(Act::Tell(title, _)) if title.contains("still stands")), "{late:?}");
    assert_eq!(t.look("", start), None);
    assert!(!t.standing());
    let at = start + STILL_AFTER * 3;
    assert_eq!(t.look(&adding(&other), at), None);
    let told = t.look(&adding(&other), at + ANSWER_AFTER);
    assert!(matches!(&told, Some(Act::Tell(title, screen)) if title.contains(&other) && title.contains("not a folder of the project") && screen.contains("Yes, add it")), "{told:?}");
    assert_eq!(t.look(&adding(&other), at + STILL_AFTER * 2), None, "told once, never answered");
    let _ = (std::fs::remove_dir_all(&repo), std::fs::remove_dir_all(&other));
}

/// A program on a real terminal that draws the popup for DIR, moves its pointer with the arrows and says what Enter chose.
const POPUP: &str = r#"
import os, sys, tty, time
tty.setraw(0)
sel = 0
def draw():
    rows = ["Adding a new directory:", sys.argv[1], "", "This session hasn't worked here before. Is this a directory you created or one you trust?", "", ("> " if sel == 0 else "  ") + "No, exit", ("> " if sel == 1 else "  ") + "Yes, add it", "", "Enter to confirm · Esc to cancel"]
    os.write(1, ("\x1b[2J\x1b[H" + "\r\n".join(rows) + "\r\n").encode())
draw()
while True:
    b = os.read(0, 16)
    while b:
        if b.startswith(b"\x1b[B"): sel, b = 1, b[3:]; draw()
        elif b.startswith(b"\x1b[A"): sel, b = 0, b[3:]; draw()
        elif b.startswith(b"\r"):
            os.write(1, ("\x1b[2J\x1b[HCHOSE " + ["no", "yes"][sel] + "\r\n").encode()); time.sleep(30)
        else: b = b[1:]
"#;

/// The popup run on a pseudo-terminal of coord-pty's own (`spawn`), its screen looked at as the holder does for SECS;
/// what was done and the last screen.
fn on_terminal(dir: &str, declared: &[String], secs: u64) -> (Vec<Act>, String) {
    let cmd = ["python3", "-c", POPUP, dir].map(String::from);
    let (master, child) = crate::spawn(&cmd, 24, 120).unwrap();
    let (mut reader, input) = (master.try_clone().unwrap(), master);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut b = [0u8; 4096];
        while let Ok(n @ 1..) = reader.read(&mut b) {
            if tx.send(b[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut parser = vt100::Parser::new(crate::nz(24), crate::nz(120), 0);
    let (mut trust, mut acts) = (Trust::new(declared, None), Vec::new());
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end && !parser.screen().contents().contains("CHOSE") {
        if let Ok(b) = rx.recv_timeout(Duration::from_millis(50)) {
            parser.process(&b);
        }
        match trust.look(&parser.screen().contents(), Instant::now()) {
            Some(Act::Press(keys, h, d)) => {
                press(&input, &keys);
                acts.push(Act::Press(keys, h, d));
            }
            Some(a) => acts.push(a),
            None => {}
        }
    }
    unsafe {
        libc::kill(child, libc::SIGKILL);
        libc::waitpid(child, std::ptr::null_mut(), 0);
    }
    (acts, parser.screen().contents())
}

#[test]
fn on_a_terminal_the_declared_repository_is_added_and_another_folder_is_left_waiting() {
    let (repo, other) = (folder("pty-repo"), folder("pty-other"));
    let (acts, screen) = on_terminal(&repo, std::slice::from_ref(&repo), 10);
    assert!(screen.contains("CHOSE yes"), "Yes chosen on the exact screen: {screen}");
    assert_eq!(acts, [Act::Press(b"\x1b[B\r".to_vec(), "Adding a new directory:".into(), repo.clone())]);
    let (acts, screen) = on_terminal(&other, std::slice::from_ref(&repo), 3);
    assert!(!screen.contains("CHOSE") && screen.contains("> No, exit"), "nothing typed: {screen}");
    assert!(matches!(&acts[..], [Act::Tell(title, _)] if title.contains(&other)), "the owner told, once: {acts:?}");
    let _ = (std::fs::remove_dir_all(&repo), std::fs::remove_dir_all(&other));
}
