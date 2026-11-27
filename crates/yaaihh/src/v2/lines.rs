//! One connection that carries a line each way, opened when wanted: each line that comes is read on a thread of its
//! own, which wakes the window's loop.

use coordd_v2_link::pipe::{Connect, Pipe};
use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc;

pub struct Lines {
    connect: Connect,
    wake: rune::Waker,
    open: Option<Open>,
    down: Option<String>,
}

struct Open {
    pipe: Pipe,
    /// Each line that came; `None` once the other end's output has ended.
    rx: mpsc::Receiver<Option<String>>,
}

impl Lines {
    /// Nothing is opened before the first want.
    pub fn new(connect: Connect, wake: rune::Waker) -> Lines {
        Lines { connect, wake, open: None, down: None }
    }

    /// Why it is not connected, if it is not.
    pub fn down(&self) -> Option<&str> {
        self.down.as_deref()
    }

    /// Whether it is connected, connecting first when it is not.
    pub fn open(&mut self) -> bool {
        if self.open.is_some() {
            return true;
        }
        let mut pipe = match (self.connect)() {
            Ok(p) => p,
            Err(e) => {
                self.down = Some(format!("not started: {e}"));
                return false;
            }
        };
        let (tx, rx) = mpsc::channel();
        let rd = std::mem::replace(&mut pipe.rd, Box::new(std::io::empty()));
        let wake = self.wake.clone();
        std::thread::spawn(move || {
            let woke = |x| {
                let sent = tx.send(x).is_ok();
                wake();
                sent
            };
            for l in BufReader::new(rd).lines().map_while(Result::ok) {
                if !woke(Some(l)) {
                    return;
                }
            }
            woke(None);
        });
        (self.open, self.down) = (Some(Open { pipe, rx }), None);
        true
    }

    /// Line `l` written, connecting first; false when it could not be, and then it says why it is down.
    pub fn send(&mut self, l: &str) -> bool {
        if !self.open() {
            return false;
        }
        let o = self.open.as_mut().expect("opened");
        if let Err(e) = writeln!(o.pipe.wr, "{l}").and_then(|()| o.pipe.wr.flush()) {
            self.ended(&format!("not written: {e}"));
            return false;
        }
        true
    }

    /// The lines that came since the last look, and whether the other end's output ended (it is down then).
    pub fn drain(&mut self) -> (Vec<String>, bool) {
        let came: Vec<Option<String>> = self.open.as_ref().map(|o| o.rx.try_iter().collect()).unwrap_or_default();
        let ended = came.iter().any(Option::is_none);
        if ended {
            self.ended("its output ended");
        }
        (came.into_iter().flatten().collect(), ended)
    }

    fn ended(&mut self, how: &str) {
        if let Some(mut o) = self.open.take() {
            (o.pipe.end)();
            let said = (o.pipe.said)();
            self.down = Some(said.map_or(how.to_string(), |s| format!("{how}: {s}")));
        }
    }
}
