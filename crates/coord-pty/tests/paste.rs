//! Words given as a paste reach the program only once it reads one: none while it starts up.

use coord_types::pty;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

#[test]
fn words_given_while_the_program_starts_are_pasted_once_it_turns_bracketed_paste_on() {
    let dir = std::env::temp_dir().join(format!("coord-pty-paste-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pty")).unwrap();
    // a program that starts for a second, takes whatever was typed meanwhile, then says it reads a paste
    let script = "sleep 1; timeout --foreground 0.3 cat > early; printf '\\033[?2004h'; IFS= read -r l; printf '%s' \"$l\" > got; sleep 5";
    let started = std::process::Command::new(env!("CARGO_BIN_EXE_coord-pty"))
        .args(["w1", "--", "sh", "-c", script])
        .current_dir(&dir)
        .env("COORD", &dir)
        .status()
        .unwrap();
    assert!(started.success());
    let mut s = UnixStream::connect(pty::socket(&dir.display().to_string(), "w1")).unwrap();
    s.write_all(&pty::frame(pty::PASTE, b"look at the plan")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !dir.join("got").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    let (early, got) = (std::fs::read(dir.join("early")).unwrap_or_default(), std::fs::read(dir.join("got")).unwrap_or_default());
    s.write_all(&pty::frame(pty::HANGUP, &[])).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(early, b"", "nothing typed while it started");
    assert_eq!(got, b"\x1b[200~look at the plan\x1b[201~", "one paste once it reads one");
}
