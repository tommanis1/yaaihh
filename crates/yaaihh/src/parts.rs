//! The agents in the sidebar by the parts the policy names (the owner, N-412 ask 2: "nothin org in the sidebad o nthe
//! left in the agents views"; the picture agreed in N-412.p8, yaaihh-dev/proposals/sidebar-parts.png). coordd sends
//! each agent's `part` (the policy's `part(agent, name)`, "/" nesting a part under another) and the parts' order
//! (`part-place(name, n)`, empty parts too). Beneath Agents: who leads the others, alone above the parts; then each part
//! with its count, its running agents one row each and its stopped ones on one quiet line; a part with nobody yet says
//! so; agents no part names last, in Pool. A coordd that sends no parts leaves the sidebar as it was (the tree by
//! parent, `Live::agent_tree`).

use coord_types::AgentView;
use std::collections::HashSet;

/// Where agents no part names stand, last.
pub const POOL: &str = "Pool";

/// One row beneath Agents.
#[derive(Debug, PartialEq)]
pub enum Row<'a> {
    /// A part's head: its own name (the last of its path), its path (what opens and closes it), its depth, its agents
    /// running and stopped (its nested parts' too) and whether it is open.
    Part { name: String, path: String, depth: u8, running: usize, stopped: usize, open: bool },
    /// A running agent.
    Agent { a: &'a AgentView, depth: u8 },
    /// A part's stopped agents, on one line.
    Stopped { names: Vec<String>, depth: u8 },
    /// A part nobody is in yet.
    Empty { depth: u8 },
}

fn running(a: &AgentView) -> bool {
    a.state != "stopped"
}

/// The rows beneath Agents for AGENTS by PARTS (`[name, place]`), the parts in CLOSED (their paths) closed; none when
/// coordd says nothing of parts (an older coordd, or a policy without them).
pub fn rows<'a>(agents: &'a [AgentView], parts: &[(String, i64)], closed: &HashSet<String>) -> Option<Vec<Row<'a>>> {
    if parts.is_empty() && agents.iter().all(|a| a.part.is_empty()) {
        return None;
    }
    // the parts in order: the listed ones by place, then any an agent is in that is not listed, by name
    let mut listed: Vec<&(String, i64)> = parts.iter().collect();
    listed.sort_by_key(|(_, n)| *n);
    let mut names: Vec<String> = listed.iter().map(|(p, _)| p.clone()).collect();
    let mut extra: Vec<String> = agents.iter().map(|a| a.part.clone()).filter(|p| !p.is_empty() && !names.contains(p)).collect();
    extra.sort();
    extra.dedup();
    names.extend(extra);
    let top = |p: &str| p.split('/').next().unwrap_or(p).to_string();
    let mut tops: Vec<String> = Vec::new();
    for n in &names {
        let t = top(n);
        if !tops.contains(&t) {
            tops.push(t);
        }
    }
    // who leads: in no part, running, and some agent's parent
    let leads = |a: &AgentView| a.part.is_empty() && running(a) && agents.iter().any(|b| b.parent == a.name);
    let mut out: Vec<Row> = agents.iter().filter(|a| leads(a)).map(|a| Row::Agent { a, depth: 1 }).collect();
    // one part's own agents (not its nested parts'), running one a row, stopped on one line
    let members = |out: &mut Vec<Row<'a>>, of: &dyn Fn(&AgentView) -> bool, depth: u8| {
        out.extend(agents.iter().filter(|a| of(a) && running(a)).map(|a| Row::Agent { a, depth }));
        let names: Vec<String> = agents.iter().filter(|a| of(a) && !running(a)).map(|a| a.name.clone()).collect();
        if !names.is_empty() {
            out.push(Row::Stopped { names, depth });
        }
    };
    let count = |of: &dyn Fn(&AgentView) -> bool| (agents.iter().filter(|a| of(a) && running(a)).count(), agents.iter().filter(|a| of(a) && !running(a)).count());
    for t in &tops {
        let under = |a: &AgentView| a.part == *t || a.part.starts_with(&format!("{t}/"));
        let (r, s) = count(&under);
        let open = !closed.contains(t);
        out.push(Row::Part { name: t.clone(), path: t.clone(), depth: 1, running: r, stopped: s, open });
        if !open {
            continue;
        }
        if r + s == 0 {
            out.push(Row::Empty { depth: 2 });
            continue;
        }
        members(&mut out, &|a: &AgentView| a.part == *t, 2);
        for sub in names.iter().filter(|n| n.starts_with(&format!("{t}/"))) {
            let is = |a: &AgentView| a.part == *sub;
            let (r, s) = count(&is);
            let open = !closed.contains(sub);
            out.push(Row::Part { name: sub[t.len() + 1..].to_string(), path: sub.clone(), depth: 2, running: r, stopped: s, open });
            if !open {
                continue;
            }
            if r + s == 0 {
                out.push(Row::Empty { depth: 3 });
            } else {
                members(&mut out, &is, 3);
            }
        }
    }
    // the rest: no part, and not leading (or not running)
    let pooled = |a: &AgentView| a.part.is_empty() && !leads(a);
    let (r, s) = count(&pooled);
    if r + s > 0 {
        let open = !closed.contains(POOL);
        out.push(Row::Part { name: POOL.into(), path: POOL.into(), depth: 1, running: r, stopped: s, open });
        if open {
            members(&mut out, &pooled, 2);
        }
    }
    Some(out)
}

/// A part head's count: "1 running · 4 stopped", "2 running", "3 stopped", "none yet".
pub fn count(running: usize, stopped: usize) -> String {
    match (running, stopped) {
        (0, 0) => "none yet".into(),
        (r, 0) => format!("{r} running"),
        (0, s) => format!("{s} stopped"),
        (r, s) => format!("{r} running · {s} stopped"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn agent(name: &str, parent: &str, part: &str, state: &str) -> AgentView {
        serde_json::from_value(json!({"id": format!("AGT-{name}"), "name": name, "parent": parent, "role": "", "part": part, "kind": "", "model": "",
            "state": state, "since": "", "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null}))
        .unwrap()
    }

    fn shown(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|r| match r {
                Row::Part { path, depth, running, stopped, open, .. } => format!("{depth} [{path}] {}{}", count(*running, *stopped), if *open { "" } else { " closed" }),
                Row::Agent { a, depth } => format!("{depth} {}", a.name),
                Row::Stopped { names, depth } => format!("{depth} ({})", names.join(", ")),
                Row::Empty { depth } => format!("{depth} none yet"),
            })
            .collect()
    }

    /// research as the picture has it (N-412.p8, with p14's changes): paper-research alone above, the yaaihh project's
    /// three parts nested, the Tovrin team empty until its sessions exist, htest in Pool.
    #[test]
    fn the_agents_stand_in_their_parts_as_the_picture_has_them() {
        let pr = "paper-research";
        let agents = vec![
            agent(pr, "owner", "", "working"),
            agent("docket-core", pr, "yaaihh project/Engine", "working"),
            agent("docket-core2", pr, "yaaihh project/Engine", "stopped"),
            agent("docket-edge", pr, "yaaihh project/App", "working"),
            agent("docket-edge2", pr, "yaaihh project/App", "idle"),
            agent("yaaihh", "owner", "yaaihh project/Production", "idle"),
            agent("a2h-speed", pr, "Fast tools", "stopped"),
            agent("hs-ls", pr, "Fast tools", "stopped"),
            agent("tovrin-writer", pr, "Papers", "stopped"),
            agent("htest", "owner", "", "stopped"),
        ];
        // the nested parts placed too, or they stand by name (App before Engine)
        let parts: Vec<(String, i64)> = [("yaaihh project", 1), ("yaaihh project/Engine", 2), ("yaaihh project/App", 3), ("yaaihh project/Production", 4), ("Fast tools", 5), ("Papers", 6), ("Tovrin team", 7)]
            .iter()
            .map(|(n, p)| (n.to_string(), *p))
            .collect();
        let rows = rows(&agents, &parts, &HashSet::new()).unwrap();
        assert_eq!(
            shown(&rows),
            [
                "1 paper-research",
                "1 [yaaihh project] 4 running · 1 stopped",
                "2 [yaaihh project/Engine] 1 running · 1 stopped",
                "3 docket-core",
                "3 (docket-core2)",
                "2 [yaaihh project/App] 2 running",
                "3 docket-edge",
                "3 docket-edge2",
                "2 [yaaihh project/Production] 1 running",
                "3 yaaihh",
                "1 [Fast tools] 2 stopped",
                "2 (a2h-speed, hs-ls)",
                "1 [Papers] 1 stopped",
                "2 (tovrin-writer)",
                "1 [Tovrin team] none yet",
                "2 none yet",
                "1 [Pool] 1 stopped",
                "2 (htest)",
            ]
        );
        // a closed part keeps its head and its count; a nested part closes alone
        let closed: HashSet<String> = ["Fast tools".to_string(), "yaaihh project/App".to_string()].into();
        let rows = super::rows(&agents, &parts, &closed).unwrap();
        let s = shown(&rows);
        assert!(s.contains(&"1 [Fast tools] 2 stopped closed".to_string()) && !s.contains(&"2 (a2h-speed, hs-ls)".to_string()));
        assert!(s.contains(&"2 [yaaihh project/App] 2 running closed".to_string()) && !s.contains(&"3 docket-edge".to_string()));
        assert!(s.contains(&"3 docket-core".to_string()));
    }

    /// A coordd that sends no parts (an older one, or a policy without them): the sidebar stays the tree by parent.
    #[test]
    fn without_parts_the_sidebar_is_as_before() {
        let agents = vec![agent("a", "owner", "", "working"), agent("b", "a", "", "idle")];
        assert!(rows(&agents, &[], &HashSet::new()).is_none());
    }

    /// A part an agent is in that the policy gives no place: after the placed ones, by name; a lead that is stopped
    /// is not above but in Pool.
    #[test]
    fn an_unplaced_part_comes_after_the_placed_ones() {
        let agents = vec![agent("lead", "owner", "", "stopped"), agent("x", "lead", "Zeta", "working"), agent("y", "lead", "Alpha", "working")];
        let parts = vec![("Zeta".to_string(), 1)];
        let s = shown(&rows(&agents, &parts, &HashSet::new()).unwrap());
        assert_eq!(s, ["1 [Zeta] 1 running", "2 x", "1 [Alpha] 1 running", "2 y", "1 [Pool] 1 stopped", "2 (lead)"]);
    }
}
