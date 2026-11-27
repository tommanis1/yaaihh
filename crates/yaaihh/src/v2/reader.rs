//! The machine's state for an engine's project (Home, Overview): the projects file's `[v2] reader` command, one line asked
//! and one answered, in order; it writes to no record, and the engine never sees it.

use super::lines::Lines;
use coordd_v2_link::pipe::command;
use serde_json::{json, Value as Json};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};

/// The reader as the window holds it: its connection, what was asked and what it answered.
#[derive(Default)]
pub struct Reader(RefCell<Inner>);

#[derive(Default)]
struct Inner {
    lines: Option<Lines>,
    /// The questions written and not yet answered, oldest first: answers come in that order.
    out: VecDeque<String>,
    answers: HashMap<String, Json>,
    /// Answers to ask again at their next want.
    old: HashSet<String>,
}

impl Reader {
    /// The reader run as `cmd`; `wake` runs the window's loop when an answer comes. Nothing starts before the first want.
    pub fn new(cmd: Vec<String>, wake: rune::Waker) -> Reader {
        Reader(RefCell::new(Inner { lines: Some(Lines::new(command(cmd), wake)), ..Inner::default() }))
    }

    /// The last answer to `q`, asked when none is out and none is current; None until the first comes.
    pub fn want(&self, q: &Json) -> Option<Json> {
        let mut r = self.0.borrow_mut();
        let key = q.to_string();
        let last = r.answers.get(&key).cloned();
        if (last.is_none() || r.old.contains(&key)) && !r.out.contains(&key) && r.send(&key) {
            r.old.remove(&key);
            r.out.push_back(key);
        }
        last
    }

    /// Every answer is asked again at its next want (the page was opened again, or its refresh pressed).
    pub fn again(&self) {
        let mut r = self.0.borrow_mut();
        let keys: Vec<String> = r.answers.keys().cloned().collect();
        r.old.extend(keys);
    }

    /// Why the reader is not answering, if it is not.
    pub fn down(&self) -> Option<String> {
        self.0.borrow().lines.as_ref()?.down().map(str::to_string)
    }

    /// The answers that came; whether any did, or the reader ended.
    pub fn drain(&self) -> bool {
        let mut r = self.0.borrow_mut();
        let Some((came, ended)) = r.lines.as_mut().map(Lines::drain) else { return false };
        for line in &came {
            let Some(key) = r.out.pop_front() else { continue };
            let a = serde_json::from_str(line).unwrap_or_else(|e| json!({"error": format!("not JSON: {e}")}));
            r.answers.insert(key, a);
        }
        // what was out is asked again at its next want, by a reader started anew
        if ended {
            r.out.clear();
        }
        !came.is_empty() || ended
    }
}

impl Inner {
    /// Write question `key`, starting the reader first if it is not running; false when it could not be written.
    fn send(&mut self, key: &str) -> bool {
        let sent = self.lines.as_mut().is_some_and(|l| l.send(key));
        if !sent {
            self.out.clear();
        }
        sent
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// A stand-in reader: each line asked comes back inside `{"got": …}`, until it has answered `n` lines.
    fn stand_in(n: usize, woke: Arc<AtomicUsize>) -> Reader {
        let script = format!("i=0; while [ $i -lt {n} ] && IFS= read -r l; do printf '{{\"got\":%s}}\\n' \"$l\"; i=$((i+1)); done; echo gone >&2");
        Reader::new(vec!["sh".into(), "-c".into(), script], Arc::new(move || {
            woke.fetch_add(1, Ordering::SeqCst);
        }))
    }

    fn until(r: &Reader, ok: impl Fn(&Reader) -> bool) {
        let t = Instant::now();
        while !ok(r) {
            r.drain();
            assert!(t.elapsed() < Duration::from_secs(5), "nothing came");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Two questions are each answered by their own line, the window is woken, and a question is asked once while out.
    #[test]
    fn each_question_gets_its_own_answer_and_is_asked_once() {
        let woke = Arc::new(AtomicUsize::new(0));
        let r = stand_in(10, woke.clone());
        let (a, b) = (json!({"machines": {}}), json!({"repos": [["one", "/x"]]}));
        assert_eq!((r.want(&a), r.want(&b), r.want(&a)), (None, None, None));
        until(&r, |r| r.want(&b).is_some());
        assert_eq!(r.want(&a), Some(json!({"got": a})));
        assert_eq!(r.want(&b), Some(json!({"got": b})));
        assert!(woke.load(Ordering::SeqCst) >= 2, "the window's loop woken for each");
        assert_eq!(r.0.borrow().out.len(), 0, "{a} asked once while out");
    }

    /// Asked again, the last answer stays until the new one comes; a reader that ends says so and is started anew.
    #[test]
    fn asked_again_keeps_the_last_answer_and_an_ended_reader_starts_anew() {
        let r = stand_in(1, Arc::new(AtomicUsize::new(0)));
        let a = json!({"machines": {}});
        r.want(&a);
        until(&r, |r| r.down().is_some());
        assert_eq!(r.want(&a), Some(json!({"got": a})));
        assert_eq!(r.down().as_deref(), Some("its output ended: gone"));
        r.again();
        assert_eq!(r.want(&a), Some(json!({"got": a})), "the last answer while the new one is out");
        assert!(r.down().is_none(), "a new reader started");
        until(&r, |r| r.0.borrow().out.is_empty());
        assert_eq!(r.want(&a), Some(json!({"got": a})));
    }
}
