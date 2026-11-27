//! What the frame asks of the machine: to restore the record, to perform, to take an answer, to evaluate a query, to list what holds, to explain.

// Values cross as JSON, the wire's form; reading them into the machine's values is the machine's. After each request
// the frame takes the lines the machine wrote and puts them in the file before anyone sees them.

use crate::peer::Party;
use coordd_v2_wire::{Answer, StepDelta};
use docket_v2::run::driver::Reply;
use docket_v2::run::value::Instance;
use serde_json::Value as Json;
use std::collections::BTreeMap;

/// The machine as the frame drives it. `Err` is a request the machine could not take, never a refusal: a refusal is
/// a `refused` line.
pub trait Machine {
    /// Make again, line for line, the record as it stood, before any request is served.
    fn restore(&mut self, lines: &[Json]) -> Result<(), String>;
    /// Perform Act `name` with `args` as `party`: a run that performs it begins (spec 8.8).
    fn act(&mut self, party: &Party, name: &str, args: &[Json]) -> Result<Json, String>;
    /// `party`'s answer, over the socket, to the `effect` line at `line`.
    fn answer(&mut self, party: &Party, line: u64, answer: &Answer) -> Result<Json, String>;
    /// A linked driver's answer, given `Later`, to the `effect` line at `line`.
    fn late(&mut self, line: u64, reply: Reply) -> Result<(), String>;
    /// An Event the driver at place `driver` performs on its own: refused by source unless it names it (spec 8.2).
    fn event(&mut self, driver: usize, ev: Instance) -> Result<(), String>;
    /// The answer to `query` in the facts that hold now.
    fn query(&mut self, party: &Party, query: &str) -> Result<Json, String>;
    /// Every instance of Fact or Duty `name`, a full name, that holds now, each as the record writes an instance.
    fn holding(&mut self, party: &Party, name: &str) -> Result<Json, String>;
    /// The values of instance query `expr` (a `Foreach`) now; a read only, so any connection may ask it.
    fn inst_query(&mut self, _party: &Party, _expr: &str) -> Result<Json, String> {
        Err("this machine answers no instance query".into())
    }
    /// For each act in `target` (one, or a list), whether `actor` may perform it now; any connection may ask of any actor.
    fn enabled_for(&mut self, _party: &Party, _actor: &Json, _target: &Json) -> Result<Json, String> {
        Err("this machine answers no enabled_for".into())
    }
    /// What of Fact and Duty types `types` (every one when none) holds now, and which duties are violated.
    fn view(&mut self, _types: Option<&[String]>) -> Result<View, String> {
        Err("this machine takes no subscription".into())
    }
    /// The delta of the change just made, VIEW brought up to it; by default from a whole read, for machines that cannot say.
    fn delta(&mut self, view: &mut View, seq: u64) -> Result<StepDelta, String> {
        let now = self.view(None)?;
        let d = crate::delta::between(view, &now, seq);
        *view = now;
        Ok(d)
    }
    /// Nobody takes the deltas now: what the machine counts for `delta` may be let go.
    fn unwatch(&mut self) {}
    /// Why instance `inst` holds or not, or an Act or Event is enabled or not, now, as no run reads it.
    fn explain(&mut self, _party: &Party, inst: &str) -> Result<Json, String> {
        Err(format!("this machine gives no reasons: {inst}"))
    }
    /// Whether `cond` holds now, its names read as module `module` reads them: the daemon's own question.
    fn holds(&mut self, module: &str, cond: &str) -> Result<bool, String>;
    /// The lines written since this was last asked, in order.
    fn written(&mut self) -> Vec<Json>;
}

/// One type's instances, each by its JSON text, so that one goes in or out without a scan.
pub type Held = BTreeMap<String, Json>;

/// What of some Fact and Duty types holds, as a subscription sees it: each type's instances, and the violated duties.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    pub facts: BTreeMap<String, Held>,
    pub duties: BTreeMap<String, Held>,
    pub violated: BTreeMap<String, Held>,
}

impl View {
    /// The view as the wire gives it: `facts`, `duties` and `violated`, each type with its list of instances.
    pub fn listed(&self) -> Json {
        let lists = |m: &BTreeMap<String, Held>| m.iter().map(|(t, h)| (t.clone(), Json::Array(h.values().cloned().collect()))).collect::<serde_json::Map<_, _>>();
        serde_json::json!({"facts": lists(&self.facts), "duties": lists(&self.duties), "violated": lists(&self.violated)})
    }
}

/// INSTANCES as one type's [`Held`].
pub fn held(instances: impl IntoIterator<Item = Json>) -> Held {
    instances.into_iter().map(|j| (j.to_string(), j)).collect()
}
