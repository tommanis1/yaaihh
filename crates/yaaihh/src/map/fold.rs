//! His questions and stop recoveries under the thread they came from (the owner, N-1976 p3): a `Q-` or `S-` ticket is
//! no thread of its own on the cloud; what he posted in it is drawn in the first thread up its chain of parents.

use coord_types::GraphAnswer;
use docket_machine::graph::{Edge, EdgeKind};
use docket_machine::graph_slice::{Layer, SliceEdge};
use docket_machine::view::TicketView;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Whether ticket ID is kept inside the thread it came from.
fn inside(id: &str) -> bool {
    id.starts_with("Q-") || id.starts_with("S-")
}

/// Each `Q-` or `S-` ticket in TICKETS and the thread it is drawn in: the first ticket up its parents (or what it
/// stands under) that is neither, at most a dozen up. One with none is left out, drawn as it was.
pub fn into(tickets: &BTreeMap<u64, TicketView>) -> HashMap<String, String> {
    #[cfg(test)]
    CALLS.with(|c| c.set(c.get() + 1));
    chains(tickets.values().map(|v| (v.id.as_str(), v.parent.as_deref().or(v.under.as_deref()))))
}

#[cfg(test)]
thread_local! {
    /// How many times this thread worked [`into`] out.
    pub static CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// [`into`] over each ticket's id and what it came from.
fn chains<'a>(up: impl Iterator<Item = (&'a str, Option<&'a str>)>) -> HashMap<String, String> {
    let up: HashMap<&str, Option<&str>> = up.collect();
    let mut out = HashMap::new();
    for id in up.keys().copied().filter(|id| inside(id)) {
        let mut at = id;
        for _ in 0..12 {
            let Some(next) = up.get(at).copied().flatten() else { break };
            if !inside(next) {
                out.insert(id.to_string(), next.to_string());
                break;
            }
            at = next;
        }
    }
    out
}

/// A with every post in a ticket of INTO posted in the thread INTO gives it: its `posted_in` moved, or one made for a
/// post known only by its id.
pub fn folded(a: &GraphAnswer, into: &HashMap<String, String>) -> GraphAnswer {
    let mut a = a.clone();
    let mut placed: HashSet<String> = HashSet::new();
    for e in a.slice.edges.iter_mut().filter(|e| e.edge.kind == EdgeKind::PostedIn) {
        placed.insert(e.edge.from.clone());
        if let Some(t) = into.get(&e.edge.to) {
            e.edge.to = t.clone();
        }
    }
    let made: Vec<SliceEdge> = a
        .slice
        .nodes
        .iter()
        .filter(|n| !placed.contains(&n.node.id))
        .filter_map(|n| {
            let (t, _) = docket_machine::post_ref(&n.node.id)?;
            let to = into.get(t)?.clone();
            Some(SliceEdge { edge: Edge { from: n.node.id.clone(), to, kind: EdgeKind::PostedIn, source: n.node.source.clone(), when: n.node.when.clone(), again: None, what: String::new() }, layer: Layer::His })
        })
        .collect();
    a.slice.edges.extend(made);
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_and_a_stop_go_into_the_thread_they_came_from() {
        let into = chains([("N-1", None), ("Q-7", Some("N-1")), ("S-9", Some("Q-7")), ("S-4", None)].into_iter());
        assert_eq!(into.get("Q-7").map(String::as_str), Some("N-1"));
        assert_eq!(into.get("S-9").map(String::as_str), Some("N-1"), "a stop under a question goes on up");
        assert!(!into.contains_key("N-1") && !into.contains_key("S-4"), "a thread, or one with nowhere to go, stays");
        let mut a = super::super::cloud::tests::base();
        let from = a.slice.nodes.iter().position(|n| n.by.as_deref() == Some("owner")).unwrap();
        a.slice.nodes[from].node.id = "Q-7.p1".into();
        let f = folded(&a, &into);
        assert!(f.slice.edges.iter().any(|e| e.edge.kind == EdgeKind::PostedIn && e.edge.from == "Q-7.p1" && e.edge.to == "N-1"));
    }
}
