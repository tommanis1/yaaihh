//! One key's way to its echo, stamped (N-412 post 120: typing into an agent's terminal in the Windows window is "a
//! little slow"). One key at a time is followed: the key as the window hands it over, its line written to the pipe,
//! the pipe's first terminal output for that agent read after it, that output taken into the terminal, and the next
//! frame built. Each followed key is one line in the app's log, with the bytes the pipe brought in front of the echo.

use std::sync::Mutex;
use std::time::{Duration, Instant};

struct Stamp {
    agent: String,
    down: Instant,
    written: Option<Instant>,
    read: Option<Instant>,
    /// Bytes the pipe brought between the line written and the echo read.
    ahead: usize,
    taken: Option<Instant>,
}

static AT: Mutex<Option<Stamp>> = Mutex::new(None);

/// A key for AGENT's terminal: followed when no other key is (or the last one's echo never came).
pub fn down(agent: &str) {
    let mut at = AT.lock().unwrap();
    if at.as_ref().is_none_or(|s| s.down.elapsed() > Duration::from_secs(2)) {
        *at = Some(Stamp { agent: agent.into(), down: Instant::now(), written: None, read: None, ahead: 0, taken: None });
    }
}

/// The followed key's line is written to the pipe (or could not be: then it is not followed).
pub fn written(ok: bool) {
    let mut at = AT.lock().unwrap();
    match at.as_mut() {
        Some(s) if s.written.is_none() && ok => s.written = Some(Instant::now()),
        Some(s) if s.written.is_none() => *at = None,
        _ => {}
    }
}

/// The pipe's reader read a line of BYTES; OUTPUT names the agent when it is a terminal's output.
pub fn read(bytes: usize, output: Option<&str>) {
    let mut at = AT.lock().unwrap();
    let Some(s) = at.as_mut().filter(|s| s.written.is_some() && s.read.is_none()) else { return };
    if output == Some(s.agent.as_str()) {
        s.read = Some(Instant::now());
    } else {
        s.ahead += bytes;
    }
}

/// AGENT's output is in its terminal.
pub fn taken(agent: &str) {
    let mut at = AT.lock().unwrap();
    if let Some(s) = at.as_mut().filter(|s| s.read.is_some() && s.taken.is_none() && s.agent == agent) {
        s.taken = Some(Instant::now());
    }
}

/// A frame is built: the followed key's line, once its output was taken.
pub fn built() {
    let mut at = AT.lock().unwrap();
    let Some(s) = at.as_ref().filter(|s| s.taken.is_some()) else { return };
    let ms = |t: Option<Instant>| t.map_or(0.0, |t| t.saturating_duration_since(s.down).as_secs_f64() * 1000.0);
    let now = Instant::now().saturating_duration_since(s.down).as_secs_f64() * 1000.0;
    crate::log::line(&format!(
        "key echo {}: written {:.1} ms, read {:.1} ms, taken {:.1} ms, built {now:.1} ms; {} KB ahead on the pipe",
        s.agent,
        ms(s.written),
        ms(s.read),
        ms(s.taken),
        s.ahead / 1024
    ));
    *at = None;
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_key_is_followed_to_its_frame_and_others_wait() {
        super::down("a");
        super::down("b"); // a is followed
        super::written(true);
        super::read(5000, None);
        super::read(10, Some("b"));
        super::read(10, Some("a"));
        super::taken("a");
        let at = super::AT.lock().unwrap();
        let s = at.as_ref().unwrap();
        assert_eq!((s.agent.as_str(), s.ahead), ("a", 5010));
        assert!(s.taken.is_some());
        drop(at);
        super::built();
        assert!(super::AT.lock().unwrap().is_none());
    }
}
