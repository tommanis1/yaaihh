//! Every harness's hook Effects carried by one driver: each line goes to the frame that holds its agent's session.

use crate::link::{Ready, Waits};
use crate::peer::written;
use docket_hooks::{Frame, Native};
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::value::Value;
use std::cell::RefCell;
use std::os::fd::RawFd;
use std::rc::Rc;

/// A harness's frame as the hooks driver holds it: a driver whose sessions it can be asked about.
pub trait Held: Driver + Waits {
    fn holds(&self, agent: &str) -> bool;
}

impl<N: Native> Held for Frame<N> {
    fn holds(&self, agent: &str) -> bool {
        Frame::holds(self, agent)
    }
}

/// The frames, one carrier for the Effects they share (one Effect, one carrier: spec 14, check 4).
pub struct Hooks(pub Vec<Rc<RefCell<dyn Held>>>);

impl Driver for Hooks {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("hooks")]
    }
    /// The name of the frame that holds `party`'s session, its harness and version, so its `Lets` lines are read.
    fn name_for(&self, party: &Value) -> Vec<Value> {
        let agent = written(std::slice::from_ref(party));
        self.0.iter().find(|f| f.borrow().holds(&agent)).map_or_else(|| self.name(), |f| f.borrow().name())
    }
    /// Each Effect once, with the first frame's dimensions (every frame names the three with none).
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        let mut all: Vec<(String, Vec<String>)> = Vec::new();
        for e in self.0.iter().flat_map(|f| f.borrow().effects()) {
            if !all.iter().any(|(n, _)| *n == e.0) {
                all.push(e);
            }
        }
        all
    }
    fn events(&self) -> Vec<String> {
        let mut all: Vec<String> = Vec::new();
        for e in self.0.iter().flat_map(|f| f.borrow().events()) {
            if !all.contains(&e) {
                all.push(e);
            }
        }
        all
    }
    /// The frame for a line is its agent's, the Effect's first argument, as each of std.hooks' shared Effects takes it.
    fn carry(&mut self, op: &str, args: &[Value], line: u64) -> Reply {
        let agent = written(&args[..args.len().min(1)]);
        match self.0.iter().find(|f| f.borrow().holds(&agent)) {
            Some(f) => f.borrow_mut().carry(op, args, line),
            None => Reply::Failed(format!("{agent} has no session")),
        }
    }
    fn dropped(&mut self, line: u64) {
        // a frame that holds no such line keeps what it has
        self.0.iter().for_each(|f| f.borrow_mut().dropped(line));
    }
}

impl Waits for Hooks {
    fn waits(&self) -> Vec<RawFd> {
        self.0.iter().flat_map(|f| f.borrow().waits()).collect()
    }
    fn ready(&mut self) -> Vec<Ready> {
        self.0.iter().flat_map(|f| f.borrow_mut().ready()).collect()
    }
}
