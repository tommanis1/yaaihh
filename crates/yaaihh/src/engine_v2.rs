//! The window's link to a coordd-v2 engine: its five requests and a terminal's bytes (coordd-v2-link), woken as the window's loop is.

use coordd_v2_link::{pipe, Link};

/// Where the window reaches a socket of the engine's: on this machine, or a command whose stdin and stdout reach it.
pub enum Reach {
    #[cfg(unix)]
    Socket(std::path::PathBuf),
    Command(Vec<String>),
}

impl Reach {
    /// A socket path, else a command; none when neither is given.
    pub fn of(socket: Option<&str>, command: &[String]) -> Option<Reach> {
        #[cfg(unix)]
        if let Some(p) = socket {
            return Some(Reach::Socket(p.into()));
        }
        #[cfg(not(unix))]
        let _ = socket;
        (!command.is_empty()).then(|| Reach::Command(command.to_vec()))
    }

    pub fn connect(self) -> pipe::Connect {
        match self {
            #[cfg(unix)]
            Reach::Socket(p) => pipe::socket(p),
            Reach::Command(c) => pipe::command(c),
        }
    }
}

/// A link to the engine at `reach`; `wake` runs the window's loop when something comes.
pub fn start(reach: Reach, wake: rune::Waker) -> Link {
    Link::start(reach.connect(), wake)
}

#[cfg(all(test, unix))]
mod tests {
    use coordd_v2_link::Incoming;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    /// A request goes out through the command, its reply comes back by its id, and the window's loop is woken for it.
    #[test]
    fn a_request_through_a_bridge_command_comes_back_by_its_id_and_wakes_the_window() {
        let woke = Arc::new(AtomicUsize::new(0));
        let w = woke.clone();
        let bridge = vec!["sh".into(), "-c".into(), r#"read req; case "$req" in *'"do":"query"'*) printf '{"is":"done","value":7}\n';; esac; sleep 2"#.into()];
        let l = super::start(super::Reach::Command(bridge), Arc::new(move || { w.fetch_add(1, Ordering::SeqCst); }));
        assert!(matches!(l.next(Duration::from_secs(5)), Some(Incoming::Up { from: None })));
        let id = l.query("n").expect("sent while up");
        assert_eq!(l.next(Duration::from_secs(5)), Some(Incoming::Done { id, value: serde_json::json!(7) }));
        assert!(woke.load(Ordering::SeqCst) >= 2, "woken when it came up and when the reply came");
    }
}
