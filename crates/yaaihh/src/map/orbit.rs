//! What stands around the cloud's cards (docs/specs/spacetime-map.md; the owner, N-1976 p1, 1 and 2): each item's
//! threads in one faint outline named for it; each live agent as a chip beside the thread its turn is on, turning
//! while it works, ringed amber while it asks or is held, dim while idle; and, once he has gone into a card, the other
//! items' threads drawn small and faint, so the one he is in is what reads.

use super::lod::Shown;
use super::spacetime::anchor;
use super::{Graph, Kind};
use crate::App;
use coord_types::TicketView;
use rune::{El, Shape, row, text};
use rune_kit::Kit;
use std::collections::{HashMap, HashSet};

/// A pill a thread of another item is drawn as around the card he is in, wide at most and high, in points: high
/// enough for a card's words (lod.rs `READS`).
pub const PILL: [f32; 2] = [180.0, 30.0];
/// How much of such a pill is drawn.
pub const SIDE: f32 = 0.4;
/// The room an outline leaves round its threads, and an agent's chip's height and the gap between two.
const PAD: f32 = 10.0;
const CHIP: f32 = 20.0;

/// The ticket ID in the project's snapshot.
fn view<'a>(tickets: &'a std::collections::BTreeMap<u64, TicketView>, id: &str) -> Option<&'a TicketView> {
    tickets.get(&crate::live::item_number(id)?).filter(|v| v.id == id)
}

/// The item thread ID stands under: the first item up its chain of parents (the line it was opened from or stands
/// under), at most a dozen up.
pub fn item_of(tickets: &std::collections::BTreeMap<u64, TicketView>, id: &str) -> Option<String> {
    let mut at = id.to_string();
    for _ in 0..12 {
        let v = view(tickets, &at)?;
        let up = v.under.clone().or_else(|| v.parent.clone())?;
        if up.starts_with("I-") {
            return Some(up);
        }
        at = up;
    }
    None
}

/// The threads drawn as themselves in SHOWN, by the item each stands under, with that item's title.
fn items(s: &App, g: &Graph, shown: &[Shown]) -> Vec<(String, String, Vec<usize>)> {
    let Some(p) = s.projects.get(s.cloud.of) else { return Vec::new() };
    let mut by: HashMap<String, Vec<usize>> = HashMap::new();
    for (n, x) in shown.iter().enumerate().filter(|(_, x)| g.node(x.i).kind == Kind::Thread && x.group.is_none()) {
        if let Some(it) = item_of(&p.live.tickets, &g.node(x.i).id) {
            by.entry(it).or_default().push(n);
        }
    }
    let mut out: Vec<(String, String, Vec<usize>)> = by.into_iter().map(|(it, n)| (view(&p.live.tickets, &it).map_or_else(|| it.clone(), |v| format!("{it} · {}", v.title)), it, n)).collect();
    out.sort();
    out
}

/// The anchors (spacetime.rs `anchor`) of threads of items other than the focus's, once he has gone into a card: drawn
/// as small faint pills ([`compact`]) while the space has them. None while there is no focus. Made with the graph
/// (cloud.rs `build`), not each frame.
pub fn peripheral(s: &App, g: &Graph) -> Option<HashSet<u32>> {
    let focus = s.cloud.focus.as_deref()?;
    let p = s.projects.get(s.cloud.of)?;
    let thread = docket_machine::post_ref(focus).map_or(focus, |(t, _)| t);
    let mine = item_of(&p.live.tickets, thread);
    let root = super::canvas::project_id(s, s.cloud.of).and_then(|id| g.project(&id))?;
    let out: HashSet<u32> = g
        .children(root)
        .iter()
        .copied()
        .filter(|i| g.node(*i).kind == Kind::Thread && g.node(*i).id != thread && (mine.is_none() || item_of(&p.live.tickets, &g.node(*i).id) != mine))
        .collect();
    Some(out)
}

/// SHOWN with the anchors in SIDE drawn as pills: each at its top-left, [`PILL`] at most, faint, and nothing it holds.
pub fn compact(g: &Graph, side: &HashSet<u32>, shown: &mut Vec<Shown>, stand: &mut HashMap<u32, usize>) {
    let mut keep = Vec::with_capacity(shown.len());
    for (n, x) in shown.iter_mut().enumerate() {
        let a = anchor(g, x.i);
        if !side.contains(&a) {
            keep.push(n);
        } else if a == x.i {
            x.r = [x.r[0], x.r[1], x.r[2].min(PILL[0]), x.r[3].min(PILL[1])];
            x.alpha *= SIDE;
            // a shut card: an open thread's frame draws no words of its own
            (x.open, x.look) = (0.0, super::lod::Look::Card);
            keep.push(n);
        }
    }
    let at: HashMap<usize, usize> = keep.iter().enumerate().map(|(to, from)| (*from, to)).collect();
    let mut old: Vec<Option<Shown>> = std::mem::take(shown).into_iter().map(Some).collect();
    *shown = keep.iter().filter_map(|n| old[*n].take()).collect();
    *stand = std::mem::take(stand).into_iter().filter_map(|(i, n)| Some((i, *at.get(&n)?))).collect();
}

/// The items' outlines under the cards, in a view VIEW, each round its threads as drawn and named at its head.
pub fn hulls(s: &App, g: &Graph, shown: &[Shown], view: [f32; 2], k: &Kit) -> Vec<El> {
    let faded = super::canvas::faded;
    let mut sh = Shape::new();
    let mut names = Vec::new();
    for (title, id, of) in items(s, g, shown) {
        let r = of.iter().map(|n| shown[*n].r).fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, r| [b[0].min(r[0]), b[1].min(r[1]), b[2].max(r[0] + r[2]), b[3].max(r[1] + r[3])]);
        let a = of.iter().map(|n| shown[*n].alpha).fold(0.0, f32::max);
        let [x0, y0, x1, y1] = [r[0] - PAD, r[1] - PAD - 16.0, r[2] + PAD, r[3] + PAD];
        let box_ = vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]];
        sh = sh.fill(box_[..4].to_vec(), faded(k.c.bg_accent, 0.05 * a)).stroke(box_, 1.0, faded(k.c.border_base, 0.7 * a));
        names.push(text(title).font(k.ty.small).color(faded(k.c.text_muted, a)).single_line().truncate().absolute(x0 + 6.0, y0 + 2.0).w((x1 - x0 - 12.0).max(0.0)).key(format!("map-item-{id}")));
    }
    if names.is_empty() {
        return Vec::new();
    }
    let mut out = vec![rune::canvas(sh).absolute(0.0, 0.0).size(view[0], view[1]).key("map-items")];
    out.extend(names);
    out
}

/// How agent A's chip says what it is doing: turning while it works; amber ringed while it waits on someone or is
/// held; dim while idle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Doing {
    Working,
    Asking,
    Idle,
}

pub fn doing(state: &str) -> Doing {
    match state {
        "working" => Doing::Working,
        s if s.starts_with("waiting") || s.starts_with("held") || s == "stopped" => Doing::Asking,
        _ => Doing::Idle,
    }
}

/// The live agents' chips, each beside the card of the thread it is on as drawn, stacked down its right side; a press
/// selects that thread.
pub fn agents(s: &App, g: &Graph, shown: &[Shown], k: &Kit) -> Vec<El> {
    let Some(p) = s.projects.get(s.cloud.of) else { return Vec::new() };
    let mut on: HashMap<u32, usize> = HashMap::new();
    let mut out = Vec::new();
    for a in &p.live.agents {
        let Some(t) = a.on.as_deref() else { continue };
        let thread = docket_machine::post_ref(t).map_or(t, |(t, _)| t);
        let Some(x) = shown.iter().find(|x| g.node(x.i).id == thread && x.group.is_none()) else { continue };
        let n = on.entry(x.i).or_default();
        let at = [x.r[0] + x.r[2] + 6.0, x.r[1] + *n as f32 * (CHIP + 4.0)];
        *n += 1;
        let d = doing(&a.state);
        let ring = match d {
            Doing::Asking => (2.0, k.c.warning.fg),
            Doing::Working => (1.0, k.c.border_strong),
            Doing::Idle => (1.0, k.c.border_muted),
        };
        let ink = (d == Doing::Asking).then_some(k.c.warning.fg);
        let i = x.i;
        let chip = row()
            .key(format!("map-agent-{}", a.name))
            .absolute(at[0], at[1])
            .h(CHIP)
            .px(8.0)
            .gap(6.0)
            .items_center()
            .radius(CHIP / 2.0)
            .fill(k.c.layer_01)
            .border(ring.0, ring.1)
            .opacity(if d == Doing::Idle { 0.5 } else { x.alpha.max(0.5) })
            .cursor(rune::Cursor::Pointer)
            .child(super::brain::dot(d == Doing::Working, ink, k))
            .child(text(a.name.clone()).font(k.ty.small).color(k.c.text_base).single_line())
            .on_drag(move |s: &mut App, d: &mut rune::Drag, _tx: &mut rune::Tx| {
                if d.phase == rune::Phase::Up && !d.cancelled() {
                    super::canvas::select(s, Some(i));
                }
            });
        out.push(super::canvas::floating(chip));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_agents_state_reads_as_working_asking_or_idle() {
        assert_eq!(doing("working"), Doing::Working);
        assert_eq!(doing("waiting on owner"), Doing::Asking);
        assert_eq!(doing("held on codex"), Doing::Asking);
        assert_eq!(doing("idle"), Doing::Idle);
    }
}
