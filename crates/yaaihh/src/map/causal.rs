//! The causal slice (docs/specs/spacetime-map.md; the owner, N-1976 p1, 3): Shift and a click on a card show how it
//! came to be. Its past is walked back through the Map's links, each to a node no later than the one it leaves, and
//! laid on a rail over the canvas's head, oldest at the left; everything off it fades.

use super::{Graph, TIMELESS};
use crate::App;
use rune::El;
use rune_kit::Kit;
use std::collections::HashSet;

/// The most nodes a slice holds, and the most links it walks back from its card.
pub const MOST: usize = 64;
pub const HOPS: usize = 6;
/// How much is drawn of what is off the slice.
pub const OFF: f32 = 0.25;

/// The causal past of node I in G at moment AT: I and the nodes walked back to from it, each through a link made by
/// the time the node it leaves came, to a node there by then; oldest first, I last. Within [`MOST`] and [`HOPS`].
pub fn past(g: &Graph, i: u32, at: i64) -> Vec<u32> {
    // when each came to be, not when it was last touched: a link made to it later is no part of its past
    let time = |n: u32| match g.node(n).born {
        TIMELESS if n == i => at,
        t => t,
    };
    let mut seen: HashSet<u32> = HashSet::from([i]);
    let mut out = vec![i];
    let mut edge = vec![i];
    for _ in 0..HOPS {
        let mut next = Vec::new();
        for &n in &edge {
            let t = time(n);
            for e in g.edges().iter().filter(|e| e.when <= t) {
                let o = match (e.from == n, e.to == n) {
                    (true, false) => e.to,
                    (false, true) => e.from,
                    _ => continue,
                };
                if g.node(o).born <= t && seen.len() < MOST && seen.insert(o) {
                    next.push(o);
                    out.push(o);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        edge = next;
    }
    out.sort_by_key(|n| (*n != i, time(*n)));
    out.rotate_left(1);
    out
}

/// Shift and a click on node I (canvas.rs `drag`): its slice opened, or closed when it is the one open.
pub fn toggle(s: &mut App, i: u32) {
    let Some(g) = s.map.graph().cloned() else { return };
    let id = g.node(i).id.clone();
    if s.cloud.causal.as_ref().is_some_and(|(of, _)| *of == id) {
        s.cloud.causal = None;
        return;
    }
    let at = s.map.space.get().map_or_else(super::spacetime::clock, |(_, at)| at);
    s.cloud.causal = Some((id, past(&g, i, at).into_iter().map(|n| g.node(n).id.clone()).collect()));
}

/// The nodes on the slice open in G, and what holds each: what the canvas draws in full.
pub fn held(s: &App, g: &Graph) -> Option<HashSet<u32>> {
    let (_, ids) = s.cloud.causal.as_ref()?;
    let root = super::canvas::project_id(s, s.cloud.of).and_then(|p| g.project(&p))?;
    let mut on = HashSet::new();
    for mut i in ids.iter().filter_map(|id| g.find_in(root, id)) {
        while on.insert(i) {
            let Some(up) = g.node(i).parent else { break };
            i = up;
        }
    }
    Some(on)
}

/// The rail of the slice open, over the canvas's head in a view VIEW: a pill a node, oldest at the left, its time and
/// its label; a press selects its node. Drawn by what draws a history's rail (ribbon.rs).
pub fn drawn(s: &App, g: &Graph, view: [f32; 2], k: &Kit) -> Option<El> {
    use super::ribbon::{STEP_HIGH, panel, pill};
    let (of, ids) = s.cloud.causal.as_ref()?;
    let root = super::canvas::project_id(s, s.cloud.of).and_then(|p| g.project(&p))?;
    let nodes: Vec<u32> = ids.iter().filter_map(|id| g.find_in(root, id)).collect();
    // none of it in this graph (another project, a record read again): no rail
    if nodes.is_empty() {
        return None;
    }
    let sel = s.map.selected();
    let (gap, pad, wide) = (10.0, 6.0, 180.0);
    let fits = (((view[0] - 2.0 * pad - 8.0) / (wide + gap)).floor() as usize).max(1);
    let from = nodes.len().saturating_sub(fits);
    // at the canvas's head: in the cloud its foot can lie under the window's edge
    let top = 8.0 + pad;
    let mut el = rune::el().key("map-causal").absolute(0.0, 0.0).size(view[0], view[1]);
    let shown = nodes.len() - from;
    el = el.child(panel("map-causal-panel".into(), [8.0, top - pad, shown as f32 * (wide + gap) - gap + 2.0 * pad, STEP_HIGH + 2.0 * pad], k));
    for (n, &i) in nodes[from..].iter().enumerate() {
        let x = 8.0 + pad + n as f32 * (wide + gap);
        let node = g.node(i);
        // when it came, the order the rail is in
        let when = match node.born {
            TIMELESS => String::new(),
            t => format!("{} · ", crate::when::secs(t)),
        };
        let words = format!("{when}{}", node.label);
        let p = pill(format!("map-causal-{}", from + n), [x, top, wide, STEP_HIGH], words, node.id == *of, sel == Some(i), k).on_drag(move |s: &mut App, d: &mut rune::Drag, _tx: &mut rune::Tx| {
            if d.phase == rune::Phase::Up && !d.cancelled() {
                super::canvas::select(s, Some(i));
            }
        });
        el = el.child(super::canvas::floating(p));
    }
    Some(el)
}
