//! A pipe's wakes of the window, paced. The first message after a quiet stretch wakes it at once (a key's echo is not
//! held back); one that comes sooner than EVERY after the last batch was taken waits the rest out on the pipe's own
//! thread, and wakes nobody while a wake is already on its way. So a stream builds the window at most 1 / EVERY times
//! a second, and nothing wakes it while nothing comes. Agents' messages are paced at [`STREAM_EVERY`] (frames.jsonl, 11
//! Nov: one build each made 8 to 13 builds and 40 to 80 frames a second), terminals' output at [`PTY_EVERY`] (N-1933.p9:
//! every chunk woke the window).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The agents' messages are taken at most this often.
pub const STREAM_EVERY: Duration = Duration::from_millis(80);
/// The terminals' output is taken at most this often: 30 a second.
pub const PTY_EVERY: Duration = Duration::from_millis(33);
/// How long a wake counts as still on its way: should one ever be lost, the next message after this wakes again.
const WOKEN_FOR: Duration = Duration::from_millis(500);

/// One pipe's pace: when a wake was sent and not yet taken, and when the last batch was taken.
#[derive(Clone)]
pub struct Pacer {
    every: Duration,
    at: Arc<Mutex<(Option<Instant>, Option<Instant>)>>,
}

impl Pacer {
    pub fn new(every: Duration) -> Pacer {
        Pacer { every, at: Arc::default() }
    }

    /// The waker a pipe's thread calls for each message, in front of WINDOW's.
    pub fn waker(&self, window: rune::Waker) -> rune::Waker {
        let me = self.clone();
        Arc::new(move || {
            let mut at = me.at.lock().unwrap();
            if at.0.is_none_or(|t| t.elapsed() > WOKEN_FOR) {
                at.0 = Some(Instant::now());
                let wait = at.1.map_or(Duration::ZERO, |taken| me.every.saturating_sub(taken.elapsed()));
                drop(at);
                // what arrives meanwhile waits in the pipe and joins the batch
                std::thread::sleep(wait);
                window();
            }
        })
    }

    /// Called before the pipes are read: a message after the read wakes the window again.
    pub fn reading(&self) {
        self.at.lock().unwrap().0 = None;
    }

    /// The rest of EVERY since the last batch, while it runs: the poll asks a wake for it, and the pipes wake nobody
    /// meanwhile. None: a batch may be taken now.
    pub fn wait(&self) -> Option<Duration> {
        let mut at = self.at.lock().unwrap();
        let rest = at.1.map(|t| t.elapsed()).filter(|d| *d < self.every).map(|d| self.every - d);
        if rest.is_some() {
            at.0 = Some(Instant::now());
        }
        rest
    }

    /// A batch was taken now.
    pub fn took(&self) {
        self.at.lock().unwrap().1 = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A burst of a hundred messages wakes the window once; after a batch is taken, the next burst wakes it once more,
    /// no sooner than EVERY after the batch.
    #[test]
    fn a_burst_wakes_the_window_once_per_batch() {
        let woke = Arc::new(AtomicUsize::new(0));
        let counted = woke.clone();
        let pace = Pacer::new(PTY_EVERY);
        let wake = pace.waker(Arc::new(move || drop(counted.fetch_add(1, Ordering::SeqCst))));
        (0..100).for_each(|_| wake());
        assert_eq!(woke.load(Ordering::SeqCst), 1, "a quiet stretch's first message at once, the rest join it");
        pace.reading();
        pace.took();
        assert!(pace.wait().is_some_and(|d| d <= PTY_EVERY), "within EVERY of the batch: wait");
        pace.reading();
        let began = Instant::now();
        (0..100).for_each(|_| wake());
        assert_eq!(woke.load(Ordering::SeqCst), 2);
        assert!(began.elapsed() >= PTY_EVERY - Duration::from_millis(2), "waited out EVERY: {:?}", began.elapsed());
        assert_eq!(pace.wait(), None, "EVERY over: a batch may be taken");
    }
}
