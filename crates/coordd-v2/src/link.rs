//! Linked drivers: each one carried in the machine and waited on in the daemon's one loop, for its late answers.

use crate::serve::Engine;
use docket_hooks::{Frame, Native};
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::value::{Instance, Value};
use std::cell::RefCell;
use std::os::fd::{BorrowedFd, OwnedFd, RawFd};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::task::Poll;
use tokio::io::unix::AsyncFd;
use tokio::io::Interest;

/// What the daemon waits on for a driver, and the answers it gave `Later` that are ready, by their lines' places.
pub trait Waits {
    fn waits(&self) -> Vec<RawFd>;
    /// Never waits: what can be taken now.
    fn ready(&mut self) -> Vec<Ready>;
}

/// What a driver has ready: a late answer to the `effect` line at its place, or an Event it performs on its own.
pub enum Ready {
    Answer(u64, Reply),
    Event(Instance),
}

/// A driver's Events before its answers, so a run an answer resumes reads what the driver saw before it answered.
fn in_order(answers: Vec<(u64, Reply)>, events: Vec<Instance>) -> Vec<Ready> {
    events.into_iter().map(Ready::Event).chain(answers.into_iter().map(|(line, reply)| Ready::Answer(line, reply))).collect()
}

impl<N: Native> Waits for Frame<N> {
    fn waits(&self) -> Vec<RawFd> {
        Frame::waits(self)
    }
    fn ready(&mut self) -> Vec<Ready> {
        in_order(Frame::ready(self), Vec::new())
    }
}

impl Waits for docket_driver_compute::Compute {
    fn waits(&self) -> Vec<RawFd> {
        docket_driver_compute::Compute::waits(self)
    }
    fn ready(&mut self) -> Vec<Ready> {
        in_order(docket_driver_compute::Compute::ready(self), Vec::new())
    }
}

impl Waits for docket_driver_turns::Turns {
    fn waits(&self) -> Vec<RawFd> {
        docket_driver_turns::Turns::waits(self)
    }
    fn ready(&mut self) -> Vec<Ready> {
        let (answers, events) = docket_driver_turns::Turns::ready(self);
        in_order(answers, events)
    }
}

impl Waits for docket_driver_host::Host {
    fn waits(&self) -> Vec<RawFd> {
        docket_driver_host::Host::waits(self)
    }
    fn ready(&mut self) -> Vec<Ready> {
        let (answers, events) = docket_driver_host::Host::ready(self);
        in_order(answers, events)
    }
}

impl Waits for docket_driver_clerk::Clerk {
    fn waits(&self) -> Vec<RawFd> {
        docket_driver_clerk::Clerk::waits(self)
    }
    fn ready(&mut self) -> Vec<Ready> {
        let (answers, events) = docket_driver_clerk::Clerk::ready(self);
        in_order(answers, events)
    }
}

impl Waits for docket_driver_window::Window {
    fn waits(&self) -> Vec<RawFd> {
        docket_driver_window::Window::waits(self)
    }
    fn ready(&mut self) -> Vec<Ready> {
        let (answers, events) = docket_driver_window::Window::ready(self);
        in_order(answers, events)
    }
}

/// A driver that holds agents' sessions: each one's hook socket, opened when its session starts.
pub trait Sessions {
    /// Agent `agent`'s hook socket, made in `dir`; its path.
    fn open(&mut self, agent: &str, dir: &Path) -> std::io::Result<PathBuf>;
    fn close(&mut self, agent: &str);
}

impl<N: Native> Sessions for Frame<N> {
    fn open(&mut self, agent: &str, dir: &Path) -> std::io::Result<PathBuf> {
        Frame::open(self, agent, dir)
    }
    fn close(&mut self, agent: &str) {
        Frame::close(self, agent)
    }
}

/// One driver as the machine holds it, shared with the loop that waits on it.
struct Linked<D>(Rc<RefCell<D>>);

impl<D: Driver> Driver for Linked<D> {
    fn name(&self) -> Vec<Value> {
        self.0.borrow().name()
    }
    fn name_for(&self, party: &Value) -> Vec<Value> {
        self.0.borrow().name_for(party)
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        self.0.borrow().effects()
    }
    fn events(&self) -> Vec<String> {
        self.0.borrow().events()
    }
    fn carry(&mut self, op: &str, args: &[Value], line: u64) -> Reply {
        self.0.borrow_mut().carry(op, args, line)
    }
    fn dropped(&mut self, line: u64) {
        self.0.borrow_mut().dropped(line)
    }
}

/// Driver `d` for the machine, and the same driver for [`wait_on`] and a session's start.
pub fn link<D: Driver + Waits + 'static>(d: D) -> (Box<dyn Driver>, Rc<RefCell<D>>) {
    let d = Rc::new(RefCell::new(d));
    (Box::new(Linked(d.clone())), d)
}

/// Driver `d`, already shared with the daemon's other parts, for the machine.
pub fn linked<D: Driver + 'static>(d: Rc<RefCell<D>>) -> Box<dyn Driver> {
    Box::new(Linked(d))
}

/// Our own copy of descriptor `f`, which the driver holds open while it is borrowed: the copy stays open, on the
/// same file, for as long as the loop waits on it, even if the driver closes `f` in a change meanwhile.
fn copy(f: RawFd) -> Option<AsyncFd<OwnedFd>> {
    // SAFETY: `f` was just given by `waits`, under the driver's borrow, so it is open now
    let owned = unsafe { BorrowedFd::borrow_raw(f) }.try_clone_to_owned().ok()?;
    // SAFETY: the `OwnedFd` is ours alone and closes only when the `AsyncFd` drops
    unsafe { AsyncFd::register_with_interest(owned, Interest::READABLE) }.ok()
}

/// For as long as the engine runs: wait until one of `d`'s descriptors can be read or the machine has changed (a
/// change can open a socket), then give the engine what `d`, the driver at `place`, has ready.
pub async fn wait_on(engine: Rc<Engine>, d: Rc<RefCell<dyn Waits>>, place: usize) {
    let mut changes = engine.changes();
    loop {
        let fds: Vec<AsyncFd<OwnedFd>> = d.borrow().waits().into_iter().filter_map(copy).collect();
        let readable = std::future::poll_fn(|cx| {
            for f in &fds {
                if let Poll::Ready(Ok(mut g)) = f.poll_read_ready(cx) {
                    g.clear_ready();
                    return Poll::Ready(());
                }
            }
            Poll::Pending
        });
        tokio::select! {
            () = readable => {}
            got = changes.changed() => if got.is_err() { return },
        }
        drop(fds);
        let ready = d.borrow_mut().ready();
        for r in ready {
            let (what, taken) = match r {
                Ready::Answer(line, reply) => (format!("the late answer to line {line}"), engine.late(line, reply)),
                Ready::Event(ev) => (format!("driver {place}'s Event {}", ev.ty), engine.event(place, ev)),
            };
            if let coordd_v2_wire::Reply::No { why } = taken {
                eprintln!("coordd-v2: {what} was not taken: {why}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drivers_events_are_taken_before_its_answers() {
        let ev = |n: &str| Instance { ty: n.into(), args: Vec::new() };
        let got = in_order(vec![(4, Reply::Later), (9, Reply::Failed("no".into()))], vec![ev("a.Spoke"), ev("a.Usage")]);
        let names: Vec<String> = got.iter().map(|r| match r {
            Ready::Event(e) => e.ty.clone(),
            Ready::Answer(line, _) => line.to_string(),
        }).collect();
        assert_eq!(names, ["a.Spoke", "a.Usage", "4", "9"]);
    }
}
