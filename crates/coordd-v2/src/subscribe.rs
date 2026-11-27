//! Subscriptions: the view of some types, then the engine's one delta per change narrowed to them, merged in one slot.

use super::Engine;
use crate::delta::{self, Frame};
use crate::machine::View;
use coordd_v2_wire::{Reply, StepDelta};
use serde_json::{json, Value as Json};
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;
use tokio::sync::{broadcast, Notify};

/// A connection's frame being made for its writer, the change its client stands at, and which subscription is live.
#[derive(Default)]
pub struct Slot {
    frame: RefCell<Frame>,
    /// What holds anew, sent before the frame to a reader that fell too far behind.
    reset: RefCell<Option<Json>>,
    since: Cell<u64>,
    pub ready: Notify,
    live: Cell<u64>,
    ended: Notify,
    closed: Cell<bool>,
}

impl Slot {
    /// The connection has ended, or subscribes again: the subscription running stops.
    pub fn end(&self) {
        self.live.set(self.live.get() + 1);
        self.ended.notify_waiters();
    }

    /// The writer writes no more: the subscription running stops, and none starts.
    pub fn close(&self) {
        self.closed.set(true);
        self.end();
    }

    /// The frame waiting, made now and taken by the writer, going on from the change the client was last given;
    /// none when what moved since comes to nothing.
    pub fn take(&self) -> Option<Reply> {
        if let Some(value) = self.reset.borrow_mut().take() {
            self.since.set(value["seq"].as_u64().unwrap_or_default());
            // what moved after it is the next frame
            self.ready.notify_one();
            return Some(Reply::Reset { value });
        }
        let delta = std::mem::take(&mut *self.frame.borrow_mut()).delta();
        if delta.is_empty() {
            return None;
        }
        let since = self.since.replace(delta.seq);
        Some(Reply::Delta { since, delta })
    }
}

impl Engine {
    /// The view of every Fact and Duty type now, and from now on the one delta of each change: for any projection.
    pub fn step_deltas(&self) -> Result<(View, broadcast::Receiver<StepDelta>), String> {
        self.viewed(None)
    }

    /// The view of TYPES (every Fact and Duty type when none), cut from the one kept for all, and the deltas from now.
    fn viewed(&self, types: Option<&BTreeSet<String>>) -> Result<(View, broadcast::Receiver<StepDelta>), String> {
        let mut core = self.core.borrow_mut();
        if let Some(why) = &core.stopped {
            return Err(why.clone());
        }
        if core.view.is_none() {
            core.view = Some(core.machine.view(None)?);
        }
        let all = core.view.as_ref().ok_or("no view")?;
        if let Some(t) = types.iter().flat_map(|ts| ts.iter()).find(|t| !all.facts.contains_key(*t) && !all.duties.contains_key(*t)) {
            return Err(format!("{t} is no Fact or Duty of the loaded text"));
        }
        Ok((delta::narrowed(all, types), self.deltas.subscribe()))
    }

    /// How many take the deltas now: the live subscriptions and projections.
    pub fn listening(&self) -> usize {
        self.deltas.receiver_count()
    }

    /// Nobody takes the deltas now: the kept view and what the machine counts for it go at once, not at the next change.
    fn let_go(&self) {
        if self.deltas.receiver_count() == 0
            && let Ok(mut core) = self.core.try_borrow_mut()
            && core.view.take().is_some()
        {
            core.machine.unwatch();
        }
    }

    /// The `done` with what of TYPES holds now; then a frame in SLOT after each change, until the connection ends or resubscribes.
    pub(super) fn subscribe(self: &Rc<Self>, types: Option<Vec<String>>, slot: &Rc<Slot>) -> Reply {
        if slot.closed.get() {
            return Reply::No { why: "the connection takes no more frames".into() };
        }
        let types: Option<BTreeSet<String>> = types.map(|ts| ts.into_iter().collect());
        let (value, rx) = match self.listed(types.as_ref()) {
            Ok(x) => x,
            Err(why) => return Reply::No { why },
        };
        slot.end();
        let me = slot.live.get();
        *slot.frame.borrow_mut() = Frame::default();
        *slot.reset.borrow_mut() = None;
        slot.since.set(value["seq"].as_u64().unwrap_or_default());
        let (engine, slot) = (self.clone(), slot.clone());
        tokio::task::spawn_local(async move {
            engine.follow(&slot, me, rx, types).await;
            engine.let_go();
        });
        Reply::Done { value }
    }

    /// What of TYPES holds now as the wire gives it, with the change it stands at, and the deltas from then on.
    fn listed(&self, types: Option<&BTreeSet<String>>) -> Result<(Json, broadcast::Receiver<StepDelta>), String> {
        let (now, rx) = self.viewed(types)?;
        let mut value = now.listed();
        value["seq"] = json!(*self.changes.borrow());
        Ok((value, rx))
    }

    /// Each change narrowed to TYPES into SLOT's frame while subscription ME is live. No view is kept for it: a reader
    /// too far behind is sent what holds anew.
    async fn follow(&self, slot: &Slot, me: u64, mut rx: broadcast::Receiver<StepDelta>, types: Option<BTreeSet<String>>) {
        while slot.live.get() == me {
            let d = tokio::select! {
                got = rx.recv() => match got {
                    Ok(d) => delta::only(&d, types.as_ref()),
                    Err(broadcast::error::RecvError::Lagged(_)) => match self.listed(types.as_ref()) {
                        Ok((value, again)) => {
                            rx = again;
                            if slot.live.get() == me {
                                *slot.frame.borrow_mut() = Frame::default();
                                *slot.reset.borrow_mut() = Some(value);
                                slot.ready.notify_one();
                            }
                            continue;
                        }
                        Err(_) => return,
                    },
                    Err(broadcast::error::RecvError::Closed) => return,
                },
                () = slot.ended.notified() => continue,
            };
            // a change that came with the end of this subscription is the next one's
            if slot.live.get() != me {
                return;
            }
            if d.is_empty() {
                continue;
            }
            slot.frame.borrow_mut().push(&d);
            slot.ready.notify_one();
        }
    }
}

#[cfg(test)]
#[path = "subscribe_tests.rs"]
mod tests;
