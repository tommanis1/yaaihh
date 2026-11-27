//! The Map in space and time (docs/specs/spacetime-map.md; the owner, N-1976 p1): one step on what the cloud draws,
//! after its levels of detail. Glass puts each card on the time plane its age falls in, behind the lit Now plane, in
//! perspective; Flat only fades it by its age. Links, labels and presses read the changed rectangles, so they follow.

use super::canvas::faded;
use super::lod::Shown;
use super::{Graph, Kind, State, TIMELESS};
use rune::{El, Shape, canvas};
use rune_kit::{Kit, SegmentedControl};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// How T is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// Time planes stacked behind the Now plane.
    #[default]
    Glass,
    /// One plane, older things fainter.
    Flat,
}

/// How far back is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Wake {
    /// The Now plane alone.
    Now,
    Hours2,
    #[default]
    Day,
    All,
}

impl Wake {
    pub const ALL: [Wake; 4] = [Wake::Now, Wake::Hours2, Wake::Day, Wake::All];

    pub fn words(self) -> &'static str {
        match self {
            Wake::Now => "0h",
            Wake::Hours2 => "2h",
            Wake::Day => "24h",
            Wake::All => "All",
        }
    }

    /// Seconds back, past the Now plane's hour; None: the whole record.
    fn span(self) -> Option<i64> {
        match self {
            Wake::Now => Some(0),
            Wake::Hours2 => Some(2 * 3600),
            Wake::Day => Some(24 * 3600),
            Wake::All => None,
        }
    }
}

/// What the Map's T is drawn with, kept in ui.json.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Space {
    pub mode: Mode,
    pub wake: Wake,
    /// The planes' tilt, in degrees from flat on (90) toward edge on.
    pub tilt: f32,
    /// The turn about the planes' upright axis, in degrees.
    pub yaw: f32,
    /// Once he has gone into a card, the threads of other items drawn as small faint pills (orbit.rs).
    pub pills: bool,
}

impl Space {
    /// The Map as before its time view: one plane, the whole record.
    pub const BEFORE: Space = Space { mode: Mode::Flat, wake: Wake::All, tilt: TILT, yaw: 0.0, pills: false };

    /// The space with its tilt and turn within their bounds, one not a number the default's: as read from ui.json.
    pub fn clamped(self) -> Space {
        let within = |v: f32, b: [f32; 2], or: f32| if v.is_finite() { v.clamp(b[0], b[1]) } else { or };
        Space { tilt: within(self.tilt, TILT_BOUNDS, TILT), yaw: within(self.yaw, YAW_BOUNDS, 0.0), ..self }
    }
}

impl Default for Space {
    fn default() -> Self {
        Space { mode: Mode::Glass, wake: Wake::Day, tilt: TILT, yaw: 0.0, pills: true }
    }
}

/// The tilt Glass opens at, and its bounds.
pub const TILT: f32 = 60.0;
pub const TILT_BOUNDS: [f32; 2] = [35.0, 90.0];
pub const YAW_BOUNDS: [f32; 2] = [-35.0, 35.0];
/// Degrees per point of the wheel's turn ([`Space::turn`]).
const TURN: f32 = 0.08;
/// The Now plane's span: what happened in the last hour, or what works or waits now, is on it.
pub const NOW: i64 = 3600;
/// How many planes Glass draws, the Now plane among them.
pub const PLANES: usize = 4;
/// The camera's distance from the Now plane, in view heights.
const FOCAL: f32 = 2.4;
/// How much farther back each plane lies than the one before, and how far below it, in view heights.
const BACK: f32 = 0.22;
const DROP: f32 = 0.05;
/// At the tilt Glass opens at, the stack drawn this much of its size and seated this far down, in view heights, so
/// the farthest plane's far edge and the Now plane's near edge both lie in the view; flat on, neither.
const FIT: f32 = 0.74;
const SEAT: f32 = 0.07;
/// The camera's scales between which Glass gives way to flat as he zooms in: tilted at the whole record's, flat inside a
/// card, so a flight and a fit into it land as they do on one plane.
pub const FLATTENS: [f64; 2] = [0.9, 2.4];
/// The fade of the oldest a wake draws, Flat; and each plane's behind the one before it, Glass.
const OLDEST: f32 = 0.25;
const PER_PLANE: f32 = 0.2;

/// The clock, in seconds: the moment ages are counted from while the time bar is at now.
pub fn clock() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// The latest of when node I came and what happened to it, by moment AT; [`TIMELESS`] for one there always.
pub fn last(g: &Graph, i: u32, at: i64) -> i64 {
    let n = g.node(i);
    n.events.iter().map(|e| e.at).filter(|t| *t <= at).fold(n.born, i64::max)
}

/// Where a node of AGE seconds at moment AT lies: its plane, and how far into the wake, 0 to 1; None: older than the
/// wake. ORIGIN is the record's first moment, for the whole record's wake.
fn placed(age: i64, live: bool, wake: Wake, origin: i64, at: i64) -> Option<(usize, f32)> {
    if live || age <= NOW {
        return Some((0, 0.0));
    }
    let span = wake.span().unwrap_or((at - origin - NOW).max(1));
    if span <= 0 || age > NOW + span {
        return None;
    }
    let into = (age - NOW) as f32 / span as f32;
    Some((1 + ((into * (PLANES - 1) as f32) as usize).min(PLANES - 2), into))
}

/// Whether node I works or waits now, so its plane is the Now plane whatever its age.
fn live(g: &Graph, i: u32) -> bool {
    matches!(g.node(i).state, State::Working | State::WaitsOnHim | State::Held)
}

/// The node whose plane I takes: the one under a project holding it; a project its own.
pub fn anchor(g: &Graph, mut i: u32) -> u32 {
    while let Some(up) = g.node(i).parent {
        if g.node(up).parent.is_none() {
            return i;
        }
        i = up;
    }
    i
}

impl Space {
    /// Point P of plane K, in a view VIEW, on screen, and its scale there.
    pub fn to_screen(&self, p: [f32; 2], k: usize, view: [f32; 2]) -> ([f32; 2], f32) {
        let ([cx, cy], h) = ([view[0] / 2.0, view[1] / 2.0], view[1].max(1.0));
        let (pitch, yaw) = (self.tilt.to_radians(), self.yaw.to_radians());
        let (dx, dy) = (p[0] - cx, p[1] - cy - k as f32 * BACK * h);
        let (x, d) = (dx * yaw.cos() - dy * yaw.sin(), dx * yaw.sin() + dy * yaw.cos());
        let lift = -(k as f32) * DROP * h;
        let up = -d * pitch.sin() + lift * pitch.cos();
        let f = FOCAL * h;
        let z = (f - d * pitch.cos() - lift * pitch.sin()).max(0.05 * f);
        let on = ((90.0 - self.tilt) / (90.0 - TILT)).clamp(0.0, 1.0);
        let s = f / z * (1.0 - (1.0 - FIT) * on);
        ([cx + x * s, cy + SEAT * on * h - up * s], s)
    }

    /// R, of a card whose anchor is drawn at A on one plane, as plane K draws it with TILT of the tilt: moved with its
    /// anchor's middle and scaled as a whole by the scale there.
    pub fn moved(&self, a: [f32; 4], r: [f32; 4], k: usize, tilt: f32, view: [f32; 2]) -> [f32; 4] {
        let c = [a[0] + a[2] / 2.0, a[1] + a[3] / 2.0];
        let (c2, s) = self.to_screen(c, k, view);
        let (c2, s) = ([c[0] + (c2[0] - c[0]) * tilt, c[1] + (c2[1] - c[1]) * tilt], 1.0 + (s - 1.0) * tilt);
        [c2[0] + (r[0] - c[0]) * s, c2[1] + (r[1] - c[1]) * s, r[2] * s, r[3] * s]
    }

    /// Alt and the wheel (canvas.rs `wheel`): DY tilts the planes, DX turns them, a notch a few degrees.
    pub fn turn(&mut self, dx: f32, dy: f32) {
        self.tilt = (self.tilt + dy * TURN).clamp(TILT_BOUNDS[0], TILT_BOUNDS[1]);
        self.yaw = (self.yaw + dx * TURN).clamp(YAW_BOUNDS[0], YAW_BOUNDS[1]);
    }

    /// Plane K's outline in a view VIEW: its corners on screen, the view's own rectangle on it.
    pub fn outline(&self, k: usize, view: [f32; 2]) -> Vec<[f32; 2]> {
        [[0.0, 0.0], [view[0], 0.0], [view[0], view[1]], [0.0, view[1]]].into_iter().map(|p| self.to_screen(p, k, view).0).collect()
    }
}

/// The plane anchor A's age puts it on at moment AT, and how far into the wake, ORIGIN the record's first moment; None:
/// older than the wake.
fn plane_of(g: &Graph, space: &Space, at: i64, origin: i64, a: u32) -> Option<(usize, f32)> {
    let t = last(g, a, at);
    if t == TIMELESS || g.node(a).kind == Kind::Project {
        return Some((0, 0.0));
    }
    placed(at - t, live(g, a), space.wake, origin, at)
}

/// Where node I lies on screen as the canvas draws it (canvas.rs `View::on_screen`), FLAT its rectangle on one plane
/// and ANCHOR its anchor's: what draws beside a card (brain.rs, ribbon.rs) follows it onto its plane. None: older than
/// the wake, not drawn.
pub fn place(g: &Graph, space: &Space, at: i64, scale: f64, view: [f32; 2], i: u32, flat: [f32; 4], anchor_flat: [f32; 4]) -> Option<[f32; 4]> {
    let origin = g.origin().unwrap_or(at);
    let (k, _) = plane_of(g, space, at, origin, anchor(g, i))?;
    Some(match space.mode {
        Mode::Flat => flat,
        Mode::Glass => space.moved(anchor_flat, flat, k, tilted(scale), view),
    })
}

/// How much of Glass's tilt is drawn at the camera's SCALE ([`FLATTENS`]): 1 far out, 0 inside a card.
pub fn tilted(scale: f64) -> f32 {
    let t = ((scale - FLATTENS[0]) / (FLATTENS[1] - FLATTENS[0])).clamp(0.0, 1.0) as f32;
    1.0 - t * t * (3.0 - 2.0 * t)
}

/// SHOWN as SPACE draws it at moment AT, with the camera at SCALE, in a view VIEW: Flat fades each card by its
/// anchor's age; Glass moves each anchor and what it holds onto its plane, scaled as a whole, as much as [`tilted`]
/// gives, the planes behind drawn first. What is older than the wake goes, and STAND's places follow. Gives the planes
/// any card is on.
pub fn project(g: &Graph, space: &Space, at: i64, scale: f64, view: [f32; 2], shown: &mut Vec<Shown>, stand: &mut HashMap<u32, usize>) -> Vec<usize> {
    let tilt = tilted(scale);
    let origin = g.origin().unwrap_or(at);
    let mut of: HashMap<u32, Option<(usize, f32)>> = HashMap::new();
    let mut plane = |i: u32| -> Option<(usize, f32)> {
        let a = anchor(g, i);
        *of.entry(a).or_insert_with(|| plane_of(g, space, at, origin, a))
    };
    let at_of: Vec<Option<(usize, f32)>> = shown.iter().map(|x| plane(x.i)).collect();
    // each anchor's rectangle as the levels drew it: what it holds moves with it
    let flat: HashMap<u32, [f32; 4]> = shown.iter().filter(|x| anchor(g, x.i) == x.i).map(|x| (x.i, x.r)).collect();
    let mut keep: Vec<(usize, usize)> = Vec::with_capacity(shown.len());
    for (n, x) in shown.iter_mut().enumerate() {
        let Some((k, into)) = at_of[n] else { continue };
        match space.mode {
            Mode::Flat => x.alpha *= 1.0 - (1.0 - OLDEST) * into,
            Mode::Glass => {
                let a = flat.get(&anchor(g, x.i)).copied().unwrap_or(x.r);
                x.r = space.moved(a, x.r, k, tilt, view);
                x.alpha *= (1.0 - PER_PLANE * k as f32).max(OLDEST);
            }
        }
        keep.push((n, k));
    }
    // the planes behind under the ones in front; in a plane, the order the levels gave (a holder before what it holds)
    // the project's region under every plane: it holds them all, and a press on a plane behind finds that plane's card
    if space.mode == Mode::Glass {
        keep.sort_by_key(|(n, k)| (g.node(shown[*n].i).kind != Kind::Project, std::cmp::Reverse(*k), *n));
    }
    let new_at: HashMap<usize, usize> = keep.iter().enumerate().map(|(to, (from, _))| (*from, to)).collect();
    let mut old = std::mem::take(shown).into_iter().map(Some).collect::<Vec<_>>();
    *shown = keep.iter().filter_map(|(n, _)| old[*n].take()).collect();
    *stand = std::mem::take(stand).into_iter().filter_map(|(i, n)| Some((i, *new_at.get(&n)?))).collect();
    let mut used: Vec<usize> = keep.iter().map(|(_, k)| *k).collect();
    used.sort_unstable();
    used.dedup();
    used
}

/// The planes under the cards, Glass, with the camera at SCALE: each a faint sheet, the farthest first; the Now plane
/// lit along its edge. None once zoomed in flat ([`tilted`]).
pub fn glass(space: &Space, used: &[usize], scale: f64, view: [f32; 2], k: &Kit) -> Option<El> {
    let tilt = tilted(scale);
    if space.mode != Mode::Glass || tilt <= 0.0 {
        return None;
    }
    let mut sh = Shape::new();
    for p in (0..PLANES).rev().filter(|p| *p == 0 || used.contains(p)) {
        let flat = [[0.0, 0.0], [view[0], 0.0], [view[0], view[1]], [0.0, view[1]]];
        let pts: Vec<[f32; 2]> = space.outline(p, view).into_iter().zip(flat).map(|(q, f)| [f[0] + (q[0] - f[0]) * tilt, f[1] + (q[1] - f[1]) * tilt]).collect();
        let a = 0.10 - 0.02 * p as f32;
        let path = pts[1..].iter().fold(rune::Path::new().move_to(pts[0]), |p, q| p.line_to(*q)).close();
        sh = sh.fill_path(&path, faded(k.c.bg_accent, a.max(0.03)));
        let mut ring = pts.clone();
        ring.push(pts[0]);
        let (w, ink) = if p == 0 { (2.0, faded(k.c.text_accent, 0.8)) } else { (1.0, faded(k.c.border_base, 0.6)) };
        sh = sh.stroke(ring, w, ink);
    }
    Some(canvas(sh).absolute(0.0, 0.0).size(view[0], view[1]).key("map-glass"))
}

/// A colour-coded mark on the time bar ([`marks_of`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mark {
    Merge,
    Hold,
    Ask,
    Failed,
}

impl Mark {
    pub fn ink(self, k: &Kit) -> rune::Color {
        match self {
            Mark::Merge => k.c.success.fg,
            Mark::Hold => k.c.warning.fg,
            Mark::Ask => k.c.text_accent,
            Mark::Failed => k.c.danger.fg,
        }
    }
}

/// The time bar's marks, by its tick in TICKS: a merge, his ask and a failed job from what the ANSWERS hold (the
/// merges and jobs only where a layer that holds them is on), a hold from the Map's graph G.
pub fn marks_of(answers: &[&coord_types::GraphAnswer], g: &Graph, ticks: &[(i64, u32)]) -> Vec<(usize, Mark)> {
    use docket_machine::graph::{EdgeKind, NodeKind};
    let secs = |w: &Option<String>| w.as_deref().and_then(docket_machine::machine::secs);
    let mut at: Vec<(i64, Mark)> = Vec::new();
    for a in answers {
        for e in &a.slice.edges {
            let m = match e.edge.kind {
                EdgeKind::MergedInto => Mark::Merge,
                EdgeKind::Asks => Mark::Ask,
                _ => continue,
            };
            at.extend(secs(&e.edge.when).map(|t| (t, m)));
        }
        at.extend(a.slice.nodes.iter().filter(|n| n.node.kind == NodeKind::Job && n.node.ok == Some(false)).filter_map(|n| Some((secs(&n.node.when)?, Mark::Failed))));
    }
    at.extend((0..g.nodes().len() as u32).filter(|i| g.node(*i).state == State::Held).map(|i| (last(g, i, i64::MAX), Mark::Hold)).filter(|(t, _)| *t != TIMELESS));
    let mut out: Vec<(usize, Mark)> = at.into_iter().filter_map(|(t, m)| Some((ticks.partition_point(|(x, _)| *x < t).min(ticks.len().checked_sub(1)?), m))).collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The switches in the cloud's row (cloud.rs `bar`): Glass or Flat, and the wake.
pub fn switches(s: &crate::App, k: &Kit) -> Vec<El> {
    let sp = s.cloud.space;
    let mode = SegmentedControl::new(&["Glass", "Flat"], Some(usize::from(sp.mode == Mode::Flat)))
        .label("Time")
        .fit()
        .on_pick(|s: &mut crate::App, i, tx: &mut rune::Tx| {
            s.cloud.space.mode = if i == 0 { Mode::Glass } else { Mode::Flat };
            super::canvas::keys_back(tx);
        })
        .build(k)
        .key("cloud-mode");
    let words: Vec<&str> = Wake::ALL.iter().map(|w| w.words()).collect();
    let wake = SegmentedControl::new(&words, Wake::ALL.iter().position(|w| *w == sp.wake))
        .label("Back")
        .fit()
        .on_pick(|s: &mut crate::App, i, tx: &mut rune::Tx| {
            s.cloud.space.wake = Wake::ALL[i];
            super::canvas::keys_back(tx);
        })
        .build(k)
        .key("cloud-wake");
    vec![mode, wake]
}

#[cfg(test)]
mod paths;
#[cfg(test)]
mod tests;
