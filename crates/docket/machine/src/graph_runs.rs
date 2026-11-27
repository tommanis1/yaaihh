//! The graph's edges from runs, steps, calls and files (paper-research N-1446 p1337: proto-window's part): what each of
//! these record lines says happened, as edges of [`crate::graph::Graph`], each at the line it comes from. Called by
//! `graph::Builder::after` for every line of a group, before its own arms; a node already there is the same node
//! (`Graph::node`), so a line both read gives one node.
//!
//! - ran: a ticket ran a handler, which is the `when` in the kind it was written in (`graph::handler_id`, the Map's
//!   id): a run of a `when` (`opened`, for the ticket `handler_ticket` made) is ticket → handler and ticket → the run's
//!   own ticket; a hole's handler (`handling`, the one event that line is written for) is ticket → handler, the `when`
//!   found as the machine finds it (machine.rs `hole`: the ticket's own `when hole`, which a thread takes from base).
//!   The machine writes `handling` only for a `when` it ran, so one the ticket does not hold, replayed, is the graph
//!   and the machine disagreeing: a problem at its line (paper-research N-1446 p1378), and no edge. An `opened` that is
//!   no run of a `when` is most of them, and no problem.
//! - handled by: his post → the run of `when owner_post` opened for it (the run's `post_of`), at that `opened` line;
//!   his post that joined the burst a run gathers (`gather`: held by that run, among its `post_of`'s), at its own
//!   `post` line (paper-research N-1446 p1677). So his post reaches the run that handled it, its steps and their calls.
//! - called: a step's call on the wire, the `decided` "model-for" line that chose its model: step → `wire:STEP@MODEL`.
//!   A model-for with no model chose none, so nothing went on the wire and there is no call (16 lines on research's
//!   record of 22 Nov 08:45).
//! - wrote: a forming call → the file coordd kept its prompt and reply in (`formed`'s `call`, under the coord's
//!   folder). Its call is the model-for on the same step (each of the record's 23 `formed` lines has one), so the file
//!   hangs off that call and is not a second call.
//! - came from: a call on the wire → its `.call` file (the step's `call`, which `Step` names: here by the
//!   provenance's `task`, the call coordd ran for the step).
//! - edited: an agent's edit act (`act` "edit"): agent → each file it names; a live edit of a ticket's program
//!   (`edited`): agent → ticket; the owner's edit of a call's prompt (`template`): agent → template, template → call file.
//!
//! - read: a typed read → the commit of the head it asks a read of.
//! - built into, installed: a `window_release` event's release → its commit built into it; the build → his window, at
//!   the time his window says it had it (`since`, by its own clock), else at the line's, the record's time, which is
//!   coordd hearing of it: minutes late when the build's engine started coordd again (paper-research N-1446 p1392,
//!   proto-window p1410). The build says which time it is (its `what`); a `since` that is no time is a problem.
//! - came from: the build → the release it follows (the latest its line's `contains` names).
//!
//! Not given by the record, so not made up (git gives the first, `graph_git`): a head's commit → the train commit that merged it, and an untyped head
//! post → its commit (only a typed read names its head as a field; a head posted in words is in its words alone); the
//! words a call other than a forming one sent and got (the wire's text is the proxy's, not the record's); an edit done by a shell command (`mv`, `sed -i`): only Edit, Write and NotebookEdit are `act` "edit" lines,
//! so proto-mind's move of `kinds/agents/paper-research.kind` (N-1446 p1329) is in its post's words alone.

use crate::graph::{EdgeKind, Graph, NodeKind, Source, agent_id, handler_id, post_id, step_id};
use crate::line::{Line, PostKind};
use crate::machine::Machine;

/// LINE's edges in G; where it names a `when` M does not hold, a problem in PROBLEMS.
pub(crate) fn add(g: &mut Graph, problems: &mut Vec<String>, m: &Machine, line: usize, l: &Line) {
    match l {
        Line::Handling { t, event, .. } => {
            let Some(from) = m.tickets.get(t).and_then(|y| y.handlers.iter().find(|w| w.ev == *event)).map(|w| w.from.clone()) else {
                problems.push(format!("line {line}: {} handled `{event}` with a `when {event}` it does not hold, so the graph has no handler for it", m.id(*t)));
                return;
            };
            let tid = g.node(m.id(*t), NodeKind::Ticket, line);
            let h = g.node(handler_id(&from, event), NodeKind::Handler, line);
            g.edge(&tid, &h, EdgeKind::Ran, line);
        }
        Line::Opened { t: c, parent, .. } => {
            let Some((Some((from, _)), Some(ev))) = m.tickets.get(c).map(|y| (&y.when, &y.handling)) else { return };
            let p = g.node(m.id(*parent), NodeKind::Ticket, line);
            let h = g.node(handler_id(from, ev), NodeKind::Handler, line);
            g.edge(&p, &h, EdgeKind::Ran, line);
            let run = g.node(m.id(*c), NodeKind::Ticket, line);
            g.edge(&p, &run, EdgeKind::Ran, line);
            if let Some(of) = m.tickets.get(c).and_then(|y| y.post_of.as_ref()) {
                let post = g.node(post_id(m, of.t, of.n), NodeKind::Post, line);
                g.edge(&post, &run, EdgeKind::HandledBy, line);
            }
        }
        Line::Decided { t, step, question, answer, provenance, .. } if question == "model-for" => {
            let Some(model) = answer.as_str() else { return };
            let s = g.node(step_id(m, *t, *step), NodeKind::Step, line);
            let w = g.node(format!("wire:{s}@{model}"), NodeKind::Call, line);
            g.edge(&s, &w, EdgeKind::Called, line);
            if let Some(task) = provenance.pointer("/call/task").and_then(|v| v.as_str()) {
                let c = g.node(format!("call:{task}"), NodeKind::CallFile, line);
                g.edge(&w, &c, EdgeKind::CameFrom, line);
            }
        }
        Line::Formed { t, step, call: Some(file), .. } => {
            let at = format!("wire:{}@", step_id(m, *t, *step));
            let Some(w) = g.nodes.iter().rev().find(|n| n.kind == NodeKind::Call && n.id.starts_with(&at)).map(|n| n.id.clone()) else { return };
            let f = g.node(format!("file:{file}"), NodeKind::File, line);
            g.edge(&w, &f, EdgeKind::Wrote, line);
        }
        Line::Act { agent, is, more, .. } if is == "edit" => {
            let a = g.node(agent_id(agent), NodeKind::Agent, line);
            for f in more.get("files").and_then(|v| v.as_array()).into_iter().flatten().filter_map(|v| v.as_str()) {
                let file = g.node(format!("file:{f}"), NodeKind::File, line);
                g.edge(&a, &file, EdgeKind::Edited, line);
            }
        }
        Line::Edited { t, by, .. } => {
            let a = g.node(agent_id(by), NodeKind::Agent, line);
            let tid = g.node(m.id(*t), NodeKind::Ticket, line);
            g.edge(&a, &tid, EdgeKind::Edited, line);
        }
        Line::Template { t, call, by, .. } => {
            let a = g.node(agent_id(by), NodeKind::Agent, line);
            let tp = g.node(format!("template:{call}@{}", m.id(*t)), NodeKind::Template, line);
            g.edge(&a, &tp, EdgeKind::Edited, line);
            let c = g.node(format!("call:{call}"), NodeKind::CallFile, line);
            g.edge(&tp, &c, EdgeKind::CameFrom, line);
        }
        // a typed read (`coord post --read`, I-1480): the post → the head it asks a read of, by its commit
        Line::Post { t, by, kind: Some(PostKind::Read { head, .. }), .. } => {
            let Some(n) = m.tickets.get(t).and_then(|y| y.posts.iter().rev().find(|p| p.by == *by)).map(|p| p.n) else { return };
            let Some(c) = commit_in(head) else { return };
            let pid = g.node(post_id(m, *t, n), NodeKind::Post, line);
            let c = g.node(format!("commit:{c}"), NodeKind::Commit, line);
            g.edge(&pid, &c, EdgeKind::Read, line);
        }
        // his post that joined the burst a run gathers (`gather_into`): held by that run, with no run of its own
        Line::Post { t, by, .. } if by == "owner" => {
            let Some(p) = m.tickets.get(t).and_then(|y| y.posts.iter().rev().find(|p| p.by == *by)) else { return };
            let Some(u) = p.held.as_deref().and_then(|id| m.ticket_named(id)) else { return };
            if !m.tickets[&u].post_of.as_ref().is_some_and(|of| of.t == *t && of.with.contains(&p.n)) {
                return;
            }
            let post = g.node(post_id(m, *t, p.n), NodeKind::Post, line);
            let run = g.node(m.id(u), NodeKind::Ticket, line);
            g.edge(&post, &run, EdgeKind::HandledBy, line);
        }
        // the window's release as installed (`window_release`): its commit → the build; the build → his window, which is
        // his (`agent:owner`); and the build → the release it follows, the latest of those it contains (a release's name
        // starts with its time), from which `graph_git` tells what it carries
        Line::Event { name, value, .. } if name == crate::machine::WINDOW_RELEASE => {
            let Some(r) = crate::machine::Running::of(value) else { return };
            let b = g.node(format!("build:{}", r.release), NodeKind::Build, line);
            if let Some(c) = r.release.rsplit('-').next().and_then(commit_in) {
                let c = g.node(format!("commit:{c}"), NodeKind::Commit, line);
                g.edge(&c, &b, EdgeKind::BuiltInto, line);
            }
            if let Some(before) = r.contains.iter().next_back() {
                let p = g.node(format!("build:{before}"), NodeKind::Build, line);
                g.edge(&b, &p, EdgeKind::CameFrom, line);
            }
            let owner = g.node(agent_id("owner"), NodeKind::Agent, line);
            // his window's own time, as the record writes one (the machine's `secs` and `at_secs` give it back the same)
            let when = match value.get("since") {
                None => None,
                Some(v) => match v.as_str().filter(|s| crate::machine::secs(s).map(crate::machine::at_secs).as_deref() == Some(*s)) {
                    Some(s) => Some(s.to_string()),
                    None => {
                        problems.push(format!("line {line}: his window's time {v} is not a time as the record writes one, so {b} is in his window at the line's time"));
                        None
                    }
                },
            };
            if let Some(n) = g.get_mut(&b).filter(|n| n.what.is_empty()) {
                n.what = match &when {
                    Some(at) => format!("{SHOWN} {at}"),
                    None => RECORDED.into(),
                };
            }
            g.edge_from(&b, &owner, EdgeKind::Installed, Source::Line(line), when);
        }
        _ => {}
    }
}

/// A build's `what` when its line carries his window's time: that time follows.
pub const SHOWN: &str = "in his window, by its own clock, at";
/// A build's `what` when its line carries no time of his window's (a window or coordc from before `since`): its
/// installed edge has the line's time, which is coordd's.
pub const RECORDED: &str = "in his window at its line's time, the record's: when coordd heard of it (the line says no time of his window's)";

/// The commit WORDS name: their last word that is 7 to 40 hex digits, as its first 8.
fn commit_in(words: &str) -> Option<String> {
    words.split_whitespace().rev().find(|w| (7..=40).contains(&w.len()) && w.chars().all(|c| c.is_ascii_hexdigit())).map(|w| w[..8.min(w.len())].to_string())
}

#[cfg(test)]
mod tests {
    use crate::graph::{Builder, EdgeKind};
    use crate::line::Line;
    use crate::machine::Machine;
    use serde_json::json;

    /// A build is in his window at the time his window says (`since`, paper-research N-1446 p1392), and at its line's,
    /// the record's, from a line that says none; its `what` says which. A `since` that is no time is a problem at its
    /// line, and the line's time stands. Build 59's swap on 21 Nov and the builds either side, made up around it.
    #[test]
    fn a_build_is_in_his_window_at_his_windows_time_when_its_line_says_one() {
        let m = Machine::load(Default::default());
        let l = |at: &str, v: serde_json::Value| -> Line { serde_json::from_value(json!({"ev": "event", "at": at, "name": "window_release", "value": v, "fired": []})).unwrap() };
        let (b58, b59, b60) = ("20261121-095202-b49e1ed3", "20261121-101928-4ee5c14e", "20261121-105100-5cb205b0");
        let mut b = Builder::default();
        b.after(&m, 0, &[l("2026-11-21T09:13:52Z", json!({"release": b58, "contains": []}))]);
        b.after(&m, 1, &[l("2026-11-21T09:39:20Z", json!({"release": b59, "contains": [b58], "since": "2026-11-21T09:34:49Z"}))]);
        b.after(&m, 2, &[l("2026-11-21T10:22:49Z", json!({"release": b60, "contains": [b58, b59], "since": "2026-11-21 10:18:19"}))]);
        let got: Vec<(String, Option<String>, String)> = b.graph.edges.iter().filter(|e| e.kind == EdgeKind::Installed).map(|e| (e.from.clone(), e.when.clone(), b.graph.get(&e.from).unwrap().what.clone())).collect();
        let row = |r: &str, at: &str, what: String| (format!("build:{r}"), Some(at.to_string()), what);
        assert_eq!(got, vec![
            row(b58, "2026-11-21T09:13:52Z", super::RECORDED.into()),
            row(b59, "2026-11-21T09:34:49Z", format!("{} 2026-11-21T09:34:49Z", super::SHOWN)),
            row(b60, "2026-11-21T10:22:49Z", super::RECORDED.into()),
        ]);
        assert_eq!(b.problems, vec![format!("line 3: his window's time \"2026-11-21 10:18:19\" is not a time as the record writes one, so build:{b60} is in his window at the line's time")]);
        // what the ask's course reads: the build's own edge, at his window's time
        let in_window = b.graph.edges.iter().find(|e| e.kind == EdgeKind::Installed && e.from == format!("build:{b59}")).and_then(|e| e.when.clone());
        assert_eq!(in_window.as_deref(), Some("2026-11-21T09:34:49Z"), "not 09:39:20Z, when coordd heard of it");
    }
}
