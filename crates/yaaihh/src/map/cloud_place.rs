//! The cloud's places (cloud.rs; paper-research N-1446 p1630, p1656: "kept from visit to visit, and the force step
//! moves only what's new"): where each thread of his lies ([`place`], Obsidian's graph view: link, repel and centre
//! forces on the new ones alone), and the file they are kept in ([`path`], [`read`], [`write`]), beside the Map's
//! map.json and apart from it. docket-edge8's file from p1668 on: cloud.rs calls these and nothing else of it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::Rect;
use super::cloud::{CARD, GAP, HEAD};
use crate::App;

// ---- a thread -------------------------------------------------------------------------------------------------------

/// A thread laid out ([`lay_thread`]): its size in the world, and a card in it for each of its pieces, in their order,
/// from its top left; its title goes above them.
#[derive(Clone, Debug, PartialEq)]
pub struct Thread {
    pub size: [f64; 2],
    pub posts: Vec<Rect>,
}

/// A piece of a thread, in the thread's order (proto-window N-1446 p1688): one of his posts, or the card of the
/// answers to one, OF the index of his post's piece among the pieces, N how many answers the card holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piece {
    His,
    Answers { of: usize, n: usize },
}

/// The room left between two of his posts' cards in a thread, so the line between them shows (paper-research N-1446
/// p1677, 2: "a small graph, not a table").
const ROOM: f64 = 3.0 * GAP;
/// How strongly a post is drawn to the middle of those laid before it, beside its place by the one before it and the
/// ones it links: what keeps a long thread a round cluster, not one long line.
const TO_THREAD_MIDDLE: f64 = 2.5;
/// The ways round a card a post may be laid at (`lay_thread`), as angles: sixteen, so the cards stagger as a graph's
/// nodes do, not in a table's rows and columns.
const WAYS: usize = 16;
/// The longest side a thread's cards may take before its links to other threads grow it (`lay_thread`; paper-research
/// N-1446 p1715, 1: "its size comes from its links, with a ceiling, not from its 319 posts"): what some fifteen of his
/// posts with their answers' cards take, so on the made-up printout only the 319-post thread is drawn smaller.
const THREAD_SIDE: f64 = 2_000.0;

/// The side of a card of LINKS links, as a share of `CARD`'s (Obsidian's graph view: "a note grows with the notes that
/// link to it", his p767): by the square root of its links, in steps (1, 4, 9, 16 links), so one more link seldom
/// changes it; at most twice.
fn grown(links: u32) -> f64 {
    (1.0 + 0.25 * (links as f64).sqrt().floor()).min(2.0)
}

/// The side of the card of N answers, as a share of `CARD`'s: three quarters for up to three, then by the square root
/// of N in steps as `grown` (4, 9, 16 … answers), so one more answer seldom changes it; at most twice.
fn answers_share(n: usize) -> f64 {
    (0.5 + 0.25 * (n.max(1) as f64).sqrt().floor()).min(2.0)
}

/// The layout of a thread of PIECES, in its order: his posts, oldest first, each followed by its answers' card; LINKS
/// between his posts, by their index among his posts (the K-th `His` piece, not the K-th piece), with how many (his
/// post naming another of his in it). A small graph (paper-research N-1446 p1677, 2):
/// - each of his posts' cards grows with its links (`grown`), an answers' card with its answers (`answers_share`);
/// - his post and the answers' card right after it are laid as one, the card under the post, ROOM between them; an
///   answers' card anywhere else is laid on its own next to its post (next to the piece before it, when OF names no
///   piece before it);
/// - the thread's order is a path through his posts: each is laid next to his one before it, at the free place round
///   it (or round a post it links to) nearest that one, those it links and the middle of those laid already (`ROOM`
///   between any two cards, so the lines show);
/// - the path starts under the title: his first post (with its answers' card) lies in the box's top left corner, and
///   all the others right of its left edge and under its top (proto-window N-1446 p1713);
/// - his first K posts with their answers' cards lie the same to each other whatever comes after them, as long as none
///   of their cards grew;
/// - the thread is the box round its cards and no more, GAP round them, a head over them of at least HEAD and a 30th
///   of its width, so a large thread's title reads as it opens. A card's size comes from its links or answers; nothing
///   in the box is room for a card not there. No pieces: one card's box, with no cards in it;
/// - its size comes from TIES, its links to other threads, not from how many posts it holds (paper-research N-1446
///   p1715, 1): its cards' longer side is at most THREAD_SIDE grown by them (`grown`: twice from 16 links), and a
///   thread whose cards lie wider is drawn smaller, its cards and the room between them alike, so a thread of
///   hundreds of posts is no large empty card.
pub fn lay_thread(pieces: &[Piece], links: &[(usize, usize, u32)], ties: u32) -> Thread {
    let n = pieces.len();
    if n == 0 {
        return Thread { size: [CARD[0] + 2.0 * GAP, HEAD + CARD[1] + GAP], posts: Vec::new() };
    }
    // his posts' pieces, in order: link ends and the path are his posts', the cards are the pieces'
    let his: Vec<usize> = (0..n).filter(|p| pieces[*p] == Piece::His).collect();
    let mut joined: Vec<(usize, usize, f64)> = Vec::new();
    let mut links_of = vec![0u32; n];
    for &(a, b, c) in links {
        let (Some(&a), Some(&b)) = (his.get(a), his.get(b)) else { continue };
        if a != b {
            joined.push((a, b, c as f64));
            links_of[a] += c;
            links_of[b] += c;
        }
    }
    let size: Vec<[f64; 2]> = (0..n)
        .map(|p| {
            let share = match pieces[p] {
                Piece::His => grown(links_of[p]),
                Piece::Answers { n, .. } => answers_share(n),
            };
            [CARD[0] * share, CARD[1] * share]
        })
        .collect();
    // what is laid: his post with the answers' card right after it as one, the card under it; any other piece alone.
    // Each its pieces, with their middles from its middle, and its size.
    let mut units: Vec<(Vec<(usize, [f64; 2])>, [f64; 2])> = Vec::new();
    let mut unit_of = vec![0usize; n];
    let mut p = 0;
    while p < n {
        let with_answers = pieces[p] == Piece::His && matches!(pieces.get(p + 1), Some(Piece::Answers { of, .. }) if *of == p);
        if with_answers {
            let (a, b) = (size[p], size[p + 1]);
            let h = a[1] + ROOM + b[1];
            units.push((vec![(p, [0.0, (a[1] - h) / 2.0]), (p + 1, [0.0, (h - b[1]) / 2.0])], [a[0].max(b[0]), h]));
            (unit_of[p], unit_of[p + 1]) = (units.len() - 1, units.len() - 1);
            p += 2;
        } else {
            units.push((vec![(p, [0.0, 0.0])], size[p]));
            unit_of[p] = units.len() - 1;
            p += 1;
        }
    }
    let m = units.len();
    let mut near: Vec<Vec<(usize, f64)>> = vec![Vec::new(); m];
    for (a, b, c) in joined {
        let (a, b) = (unit_of[a], unit_of[b]);
        if a != b {
            near[a].push((b, c));
            near[b].push((a, c));
        }
    }
    // the one each is laid next to: his one before it, or the post an answers' card holds; else the one before it
    let mut last_his: Option<usize> = None;
    let after: Vec<usize> = (0..m)
        .map(|u| {
            let first = units[u].0[0].0;
            match pieces[first] {
                Piece::His => {
                    let a = last_his.unwrap_or(u.saturating_sub(1));
                    last_his = Some(u);
                    a
                }
                Piece::Answers { of, .. } if of < first => unit_of[of],
                Piece::Answers { .. } => u.saturating_sub(1),
            }
        })
        .collect();
    let size_of = |u: usize| units[u].1;
    // the first one's top left corner, the box's: none goes left of it or over it
    let corner = [-size_of(0)[0] / 2.0, -size_of(0)[1] / 2.0];
    // whether U with its middle at C is right of and under the corner, and leaves ROOM to each laid already
    let free = |mid: &[[f64; 2]], c: [f64; 2], u: usize| {
        c[0] - size_of(u)[0] / 2.0 >= corner[0] - 1e-6
            && c[1] - size_of(u)[1] / 2.0 >= corner[1] - 1e-6
            && mid.iter().enumerate().all(|(j, m)| (c[0] - m[0]).abs() >= (size_of(u)[0] + size_of(j)[0]) / 2.0 + ROOM - 1e-6 || (c[1] - m[1]).abs() >= (size_of(u)[1] + size_of(j)[1]) / 2.0 + ROOM - 1e-6)
    };
    let dist = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
    let mut mid: Vec<[f64; 2]> = Vec::with_capacity(m);
    mid.push([0.0, 0.0]);
    for k in 1..m {
        let next_to = after[k];
        let before: Vec<(usize, f64)> = near[k].iter().copied().filter(|(j, _)| *j < k).collect();
        let middle = [mid.iter().map(|m| m[0]).sum::<f64>() / k as f64, mid.iter().map(|m| m[1]).sum::<f64>() / k as f64];
        let score = |c: [f64; 2]| dist(c, mid[next_to]) + before.iter().map(|(j, w)| w.min(9.0).sqrt() * dist(c, mid[*j])).sum::<f64>() + TO_THREAD_MIDDLE * dist(c, middle);
        let mut best: Option<([f64; 2], f64)> = None;
        let consider = |c: [f64; 2], best: &mut Option<([f64; 2], f64)>| {
            if free(&mid, c, k) {
                let s = score(c);
                if best.is_none_or(|(_, b)| s < b - 1e-9) {
                    *best = Some((c, s));
                }
            }
        };
        // round the one it is laid next to and those it links: each way, as far as leaves ROOM between the two
        for a in std::iter::once(next_to).chain(before.iter().map(|(j, _)| *j)) {
            let need = [(size_of(a)[0] + size_of(k)[0]) / 2.0 + ROOM, (size_of(a)[1] + size_of(k)[1]) / 2.0 + ROOM];
            for w in 0..WAYS {
                let turn = w as f64 * std::f64::consts::TAU / WAYS as f64;
                let (cx, cy) = (turn.cos(), turn.sin());
                let t = (need[0] / cx.abs().max(1e-9)).min(need[1] / cy.abs().max(1e-9));
                consider([mid[a][0] + t * cx, mid[a][1] + t * cy], &mut best);
            }
        }
        // none free there: rings round the one it is laid next to, wider each time, until one is
        let mut ring = 1;
        while best.is_none() {
            let step = ring as f64 * (size_of(k)[0].hypot(size_of(k)[1]) + ROOM);
            for w in 0..WAYS * ring {
                let turn = w as f64 * std::f64::consts::TAU / (WAYS * ring) as f64;
                consider([mid[next_to][0] + step * turn.cos(), mid[next_to][1] + step * turn.sin()], &mut best);
            }
            ring += 1;
        }
        mid.push(best.map(|b| b.0).unwrap_or_default());
    }
    let mut at = vec![[0.0f64; 2]; n];
    for (u, (pieces_of, _)) in units.iter().enumerate() {
        for (p, off) in pieces_of {
            at[*p] = [mid[u][0] + off[0], mid[u][1] + off[1]];
        }
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (m, s) in at.iter().zip(&size) {
        (x0, y0) = (x0.min(m[0] - s[0] / 2.0), y0.min(m[1] - s[1] / 2.0));
        (x1, y1) = (x1.max(m[0] + s[0] / 2.0), y1.max(m[1] + s[1] / 2.0));
    }
    // its size from its ties: the cards' longer side at most THREAD_SIDE grown by them, the cards drawn smaller to fit
    let scale = (THREAD_SIDE * grown(ties) / (x1 - x0).max(y1 - y0)).min(1.0);
    let w = 2.0 * GAP + scale * (x1 - x0);
    let head = HEAD.max(w / 30.0);
    let h = head + scale * (y1 - y0) + GAP;
    let posts = at.iter().zip(&size).map(|(m, s)| Rect::new(GAP + scale * (m[0] - s[0] / 2.0 - x0), head + scale * (m[1] - s[1] / 2.0 - y0), scale * s[0], scale * s[1])).collect();
    Thread { size: [w, h], posts }
}

// ---- the forces -----------------------------------------------------------------------------------------------------------

/// How many rounds the forces take, and their strengths: towards the middle, along a link, apart where two cards
/// would come nearer than THREAD_ROOM, and apart from every other card by a share of its distance (Obsidian's centre,
/// link and repel forces; the last its many-body repel, which spreads the threads that link nothing).
const ROUNDS: usize = 300;
const TO_MIDDLE: f64 = 0.01;
const ALONG: f64 = 0.04;
const APART: f64 = 0.5;
const CHARGE: f64 = 0.02;
/// The room left between two threads' cards, so the lines between them show (paper-research N-1446 p1677, 1: "a force
/// layout with space between cards, not packing").
const THREAD_ROOM: f64 = 6.0 * GAP;
/// How near two lengths come and count as one: a kept thread's size and its size now (it grew, or shrank, only past
/// it), and two cards' overlap in the no-overlap loop. Two cards exactly THREAD_ROOM apart, as the places' own sums
/// leave them, overlap by rounding (2e-13 to 6e-12 px on his record at 19 Nov); counted as an overlap, it pushes a
/// thread between two kept ones out of each into the other, for all of the loop's passes (N-1446 p2213).
const TOUCH: f64 = 1e-6;

/// The threads' places (Obsidian's graph view, moving only what is new): SIZED, each thread's id and size; LINKS
/// between them by index, with how many; KEPT the places kept, by id. A kept thread stays where it was, its middle
/// where its middle was at its new size. A new one starts beside the placed thread it links to most, else on a spiral
/// round the kept ones' middle, the largest first; the forces then move the new ones alone; and last no two cards
/// come nearer than THREAD_ROOM (by more than TOUCH), the new ones (and a kept one that grew) giving way. The same
/// input gives the same places.
pub fn place(sized: &[(&str, [f64; 2])], links: &[(usize, usize, u32)], kept: &BTreeMap<String, [f64; 4]>) -> Vec<Rect> {
    places_and_passes(sized, links, kept).0
}

/// [`place`]'s places, and how many passes its no-overlap loop took (one when nothing overlaps), for a leg to read.
fn places_and_passes(sized: &[(&str, [f64; 2])], links: &[(usize, usize, u32)], kept: &BTreeMap<String, [f64; 4]>) -> (Vec<Rect>, usize) {
    let n = sized.len();
    let size = |i: usize| sized[i].1;
    let mut mid = vec![[0.0f64; 2]; n];
    let mut fixed = vec![false; n];
    let mut grew = vec![false; n];
    for (i, (id, s)) in sized.iter().enumerate() {
        if let Some(r) = kept.get(*id) {
            mid[i] = [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0];
            fixed[i] = true;
            grew[i] = (r[2] - s[0]).abs() > TOUCH || (r[3] - s[1]).abs() > TOUCH;
        }
    }
    let mut near: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    for &(a, b, c) in links {
        if a < n && b < n && a != b {
            near[a].push((b, c as f64));
            near[b].push((a, c as f64));
        }
    }
    let reach = |i: usize| 0.5 * size(i)[0].hypot(size(i)[1]);
    let new: Vec<usize> = {
        let mut v: Vec<usize> = (0..n).filter(|i| !fixed[*i]).collect();
        v.sort_by(|a, b| (size(*b)[0] * size(*b)[1]).total_cmp(&(size(*a)[0] * size(*a)[1])).then(a.cmp(b)));
        v
    };
    let kept_mid = {
        let k: Vec<usize> = (0..n).filter(|i| fixed[*i]).collect();
        if k.is_empty() { [0.0, 0.0] } else { [k.iter().map(|i| mid[*i][0]).sum::<f64>() / k.len() as f64, k.iter().map(|i| mid[*i][1]).sum::<f64>() / k.len() as f64] }
    };
    let typical = if n == 0 { 1.0 } else { (0..n).map(reach).sum::<f64>() / n as f64 };
    let mut placed = fixed.clone();
    for (k, &i) in new.iter().enumerate() {
        let turn = k as f64 * 2.399_963_229_728_653;
        let best = near[i].iter().filter(|(j, _)| placed[*j]).max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0))).map(|x| x.0);
        mid[i] = match best {
            Some(j) => {
                let d = reach(i) + reach(j) + THREAD_ROOM;
                [mid[j][0] + d * turn.cos(), mid[j][1] + d * turn.sin()]
            }
            None => {
                let d = (2.0 * typical + THREAD_ROOM) * ((k + 1) as f64).sqrt();
                [kept_mid[0] + d * turn.cos(), kept_mid[1] + d * turn.sin()]
            }
        };
        placed[i] = true;
    }
    let half = |i: usize| [size(i)[0] / 2.0 + THREAD_ROOM / 2.0, size(i)[1] / 2.0 + THREAD_ROOM / 2.0];
    // how far I and J would overlap, across and down, THREAD_ROOM between them (both positive: they do)
    let overlap = |mid: &[[f64; 2]], i: usize, j: usize| {
        let (hi, hj) = (half(i), half(j));
        let (dx, dy) = (mid[i][0] - mid[j][0], mid[i][1] - mid[j][1]);
        (hi[0] + hj[0] - dx.abs(), hi[1] + hj[1] - dy.abs(), dx, dy)
    };
    if !new.is_empty() {
        let most = typical;
        for round in 0..ROUNDS {
            let cool = 1.0 - round as f64 / ROUNDS as f64;
            let c = [mid.iter().map(|m| m[0]).sum::<f64>() / n as f64, mid.iter().map(|m| m[1]).sum::<f64>() / n as f64];
            let mut f = vec![[0.0f64; 2]; n];
            for &i in &new {
                f[i][0] += (c[0] - mid[i][0]) * TO_MIDDLE;
                f[i][1] += (c[1] - mid[i][1]) * TO_MIDDLE;
                for &(j, w) in &near[i] {
                    let (dx, dy) = (mid[j][0] - mid[i][0], mid[j][1] - mid[i][1]);
                    let d = dx.hypot(dy).max(1e-6);
                    let pull = (d - (reach(i) + reach(j) + THREAD_ROOM)) * ALONG * w.min(9.0).sqrt();
                    f[i][0] += dx / d * pull;
                    f[i][1] += dy / d * pull;
                }
                for j in 0..n {
                    if j == i {
                        continue;
                    }
                    let (ox, oy, dx, dy) = overlap(&mid, i, j);
                    let d = dx.hypot(dy).max(1e-6);
                    f[i][0] += dx / d * CHARGE * typical * typical / d;
                    f[i][1] += dy / d * CHARGE * typical * typical / d;
                    if ox > 0.0 && oy > 0.0 {
                        if ox < oy {
                            f[i][0] += if dx >= 0.0 { ox } else { -ox } * APART;
                        } else {
                            f[i][1] += if dy >= 0.0 { oy } else { -oy } * APART;
                        }
                    }
                }
            }
            for &i in &new {
                let len = f[i][0].hypot(f[i][1]);
                let most = most * cool.max(0.05);
                let k = if len > most { most / len } else { 1.0 };
                mid[i][0] += f[i][0] * k;
                mid[i][1] += f[i][1] * k;
            }
        }
    }
    // no two overlapping (by more than TOUCH): the one that may move gives way along the shorter way out, half each
    // when both may
    let moves = |i: usize| !fixed[i] || grew[i];
    let mut passes = 0;
    for _ in 0..2_000 {
        passes += 1;
        let mut any = false;
        for i in 0..n {
            for j in i + 1..n {
                if !(moves(i) || moves(j)) {
                    continue;
                }
                let (ox, oy, dx, dy) = overlap(&mid, i, j);
                if ox <= TOUCH || oy <= TOUCH {
                    continue;
                }
                any = true;
                let (share_i, share_j) = match (moves(i), moves(j)) {
                    (true, true) => (0.5, 0.5),
                    (true, false) => (1.0, 0.0),
                    _ => (0.0, 1.0),
                };
                if ox < oy {
                    let s = if dx > 0.0 || (dx == 0.0 && i > j) { 1.0 } else { -1.0 };
                    mid[i][0] += s * ox * share_i;
                    mid[j][0] -= s * ox * share_j;
                } else {
                    let s = if dy > 0.0 || (dy == 0.0 && i > j) { 1.0 } else { -1.0 };
                    mid[i][1] += s * oy * share_i;
                    mid[j][1] -= s * oy * share_j;
                }
            }
        }
        if !any {
            break;
        }
    }
    let rects = (0..n)
        .map(|i| match kept.get(sized[i].0).filter(|_| !moves(i)) {
            // kept and not grown: exactly where it was kept
            Some(r) => Rect::new(r[0], r[1], r[2], r[3]),
            None => Rect::new(mid[i][0] - size(i)[0] / 2.0, mid[i][1] - size(i)[1] / 2.0, size(i)[0], size(i)[1]),
        })
        .collect();
    (rects, passes)
}

// ---- the file ---------------------------------------------------------------------------------------------------------

/// The places' file: `cloud.json` beside the Map's map.json (`cloud-dev.json` beside map-dev.json, `cloud-NAME` beside
/// the file NAME a test lays the Map in: join.rs `Laying::at`), so a window from before, which writes map.json whole,
/// never drops them.
pub fn path(s: &App) -> PathBuf {
    let map = s.map.laying.at.clone().unwrap_or_else(super::layout::path);
    let name = map.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let cloud = match name.as_str() {
        "map.json" => "cloud.json".to_string(),
        "map-dev.json" => "cloud-dev.json".to_string(),
        _ => format!("cloud-{name}"),
    };
    map.with_file_name(cloud)
}

/// The places kept for project ID (`project:research`) in the file AT: none when there is no file yet. Why not, when it
/// is there and cannot be read: then it is left as it is, and nothing is written over it (cloud.rs `poll`).
pub fn read(at: &Path, id: &str) -> Result<BTreeMap<String, [f64; 4]>, String> {
    let all = std::fs::read_to_string(at).map_or_else(
        |e| if e.kind() == std::io::ErrorKind::NotFound { Ok(BTreeMap::new()) } else { Err(e.to_string()) },
        |t| serde_json::from_str::<BTreeMap<String, BTreeMap<String, [f64; 4]>>>(&t).map_err(|e| e.to_string()),
    )?;
    Ok(all.get(id).cloned().unwrap_or_default())
}

/// KEPT, the places of project ID, written whole into the file AT with the other projects' places as they stand there.
pub fn write(at: &Path, id: String, kept: &BTreeMap<String, [f64; 4]>) {
    let mut all: BTreeMap<String, BTreeMap<String, [f64; 4]>> = std::fs::read_to_string(at).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    all.insert(id, kept.clone());
    if let Err(e) = crate::state::write_whole(at, &serde_json::to_string(&all).unwrap_or_default()) {
        crate::log::line(&format!("cloud: {} not written: {e}", at.display()));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::super::Rect;
    use super::super::cloud::{self, CARD, GAP, HEAD, tests::base};
    use super::{Piece, ROOM, THREAD_ROOM, THREAD_SIDE};

    /// N of his posts and no answers' cards.
    fn his(n: usize) -> Vec<Piece> {
        vec![Piece::His; n]
    }

    /// The room between two cards, across or down, when they do not overlap (0 when they do).
    fn gap(x: Rect, y: Rect) -> f64 {
        (x.x - y.right()).max(y.x - x.right()).max((x.y - y.bottom()).max(y.y - x.bottom())).max(0.0)
    }

    /// Placing: no two threads overlap, THREAD_ROOM between them; the ones his posts link sit nearer than the ones they don't;
    /// the same input gives the same places; and kept places stay, only a new thread placed.
    #[test]
    fn threads_are_placed_by_their_links_apart_and_kept() {
        let mut kept = BTreeMap::new();
        let (g, _) = cloud::cloud(&base(), None, "project:research", "research", &mut kept).unwrap();
        let p = g.project("project:research").unwrap();
        let r = |id: &str| g.node(g.find_in(p, id).unwrap()).rect;
        let ts = ["N-1", "N-2", "N-3", "N-4"];
        for (i, a) in ts.iter().enumerate() {
            for b in &ts[i + 1..] {
                let (x, y) = (r(a), r(b));
                let apart = gap(x, y) >= THREAD_ROOM - 1e-6;
                assert!(apart, "{a} {x:?} and {b} {y:?}");
            }
        }
        let gap = |a: &str, b: &str| {
            let (x, y) = (r(a), r(b));
            ((x.x - y.right()).max(y.x - x.right()).max(0.0)).hypot((x.y - y.bottom()).max(y.y - x.bottom()).max(0.0))
        };
        assert!(gap("N-1", "N-3") < gap("N-1", "N-4") || gap("N-2", "N-3") < gap("N-2", "N-4"), "linked nearer: {:?}", ts.map(r));
        // again: the same
        let mut again = BTreeMap::new();
        let (h, _) = cloud::cloud(&base(), None, "project:research", "research", &mut again).unwrap();
        assert_eq!(kept, again);
        assert_eq!(g.nodes().iter().map(|n| n.rect).collect::<Vec<_>>(), h.nodes().iter().map(|n| n.rect).collect::<Vec<_>>());
        // kept: N-4's place moved by hand stays; and a new thread, N-5, is placed beside what it names, nothing else moving
        let mut moved = kept.clone();
        moved.get_mut("N-4").unwrap()[0] += 5_000.0;
        let mut b = base();
        b.slice.nodes.push(serde_json::from_value(serde_json::json!({"id": "N-5.p1", "kind": "post", "line": 1, "when": "2026-11-22T13:00:00Z", "by": "owner"})).unwrap());
        b.slice.edges.push(serde_json::from_value(serde_json::json!({"from": "N-5.p1", "to": "N-5", "kind": "posted_in", "line": 1, "layer": "his"})).unwrap());
        b.slice.edges.push(serde_json::from_value(serde_json::json!({"from": "N-5.p1", "to": "N-1.p9", "kind": "named", "line": 1, "layer": "his"})).unwrap());
        let before = moved.clone();
        let (g5, changed) = cloud::cloud(&b, None, "project:research", "research", &mut moved).unwrap();
        assert!(changed);
        for (t, r) in &before {
            assert_eq!(moved.get(t), Some(r), "{t} kept where it was");
        }
        assert!(moved.contains_key("N-5"));
        let p5 = g5.project("project:research").unwrap();
        let r5 = |id: &str| g5.node(g5.find_in(p5, id).unwrap()).rect;
        let dist = |a: Rect, b: Rect| (a.mid()[0] - b.mid()[0]).hypot(a.mid()[1] - b.mid()[1]);
        assert!(dist(r5("N-5"), r5("N-1")) < dist(r5("N-5"), r5("N-4")), "N-5 beside N-1, whose post it names");
    }

    /// A kept thread that shrank, between two kept ones exactly THREAD_ROOM from each as the places' own sums give it
    /// (119 between 2 and 35 on his record at 19 Nov, N-1446 p2213): their overlap by rounding is none, so the
    /// no-overlap loop ends at its first pass, the shrunk thread keeps its kept middle and the two their kept places.
    #[test]
    fn a_shrunk_thread_touching_two_kept_ones_keeps_its_middle_at_the_first_pass() {
        let a = [0.1, 0.1, 760.0, 942.0];
        let c = [0.1, a[1] + a[3] + THREAD_ROOM, 760.0, 750.0];
        let b = [0.1, c[1] + c[3] + THREAD_ROOM, 368.0, 588.0];
        let kept: BTreeMap<String, [f64; 4]> = [("A", a), ("B", b), ("C", c)].into_iter().map(|(t, r)| (t.to_string(), r)).collect();
        let sized = [("A", [760.0, 942.0]), ("B", [368.0, 588.0]), ("C", [368.0, 750.0])];
        let (at, passes) = super::places_and_passes(&sized, &[], &kept);
        assert_eq!(passes, 1, "the no-overlap loop's passes");
        assert_eq!(at[..2], [Rect::new(a[0], a[1], a[2], a[3]), Rect::new(b[0], b[1], b[2], b[3])], "the two where they were kept");
        let (m, was) = (at[2].mid(), [c[0] + c[2] / 2.0, c[1] + c[3] / 2.0]);
        assert!((m[0] - was[0]).abs() < 1e-9 && (m[1] - was[1]).abs() < 1e-9, "the shrunk one's middle {m:?}, kept {was:?}");
    }

    /// A thread's layout: each post's card inside it, below its head, ROOM between any two (drawn smaller with the cards,
    /// in a thread over its ceiling); the thread the box round them and no more: GAP round them, its head over them; and
    /// the first post in its top left corner, under the title.
    #[test]
    fn a_threads_posts_lie_inside_it_apart() {
        for n in [1, 2, 5, 16, 319] {
            let t = super::lay_thread(&his(n), &[(0, 1, 2)], 0);
            let whole = Rect::new(0.0, 0.0, t.size[0], t.size[1]);
            assert_eq!(t.posts.len(), n);
            assert!(t.posts.iter().all(|r| whole.holds(r) && r.y >= super::HEAD), "{n}: {t:?}");
            // how much smaller it is drawn: a post without links is a card's width at its scale
            let scale = (t.posts.iter().map(|r| r.w).fold(f64::MAX, f64::min) / CARD[0]).min(1.0);
            for (i, a) in t.posts.iter().enumerate() {
                for b in &t.posts[i + 1..] {
                    assert!(gap(*a, *b) >= ROOM * scale - 1e-6, "{n}: {a:?} {b:?}");
                }
            }
            let edge = |f: fn(&Rect) -> f64| t.posts.iter().map(f).fold(f64::NAN, f64::max);
            let low = |f: fn(&Rect) -> f64| t.posts.iter().map(f).fold(f64::NAN, f64::min);
            let head = HEAD.max(t.size[0] / 30.0);
            assert!((low(|r| r.x) - GAP).abs() < 1e-6 && (edge(|r| r.right()) + GAP - t.size[0]).abs() < 1e-6, "{n}: no room across but GAP: {t:?}");
            assert!((low(|r| r.y) - head).abs() < 1e-6 && (edge(|r| r.bottom()) + GAP - t.size[1]).abs() < 1e-6, "{n}: no room down but the head and GAP: {t:?}");
            assert!((t.posts[0].x - GAP).abs() < 1e-6 && (t.posts[0].y - head).abs() < 1e-6, "{n}: the first post under the title: {t:?}");
        }
        assert_eq!(super::lay_thread(&his(1), &[], 0).size, [CARD[0] + 2.0 * GAP, HEAD + CARD[1] + GAP], "one post: its card, as before");
        assert_eq!(super::lay_thread(&[], &[], 0), super::Thread { size: [CARD[0] + 2.0 * GAP, HEAD + CARD[1] + GAP], posts: Vec::new() }, "no pieces: one card's box, no cards");
    }

    /// A thread's size comes from its links to other threads, with a ceiling, not from how many posts it holds
    /// (paper-research N-1446 p1715, 1): its cards' longer side at most THREAD_SIDE, grown by its links (twice from 16);
    /// a thread whose cards lie wider the same layout drawn smaller, cards and room alike; one inside its ceiling as it
    /// was, whatever its links.
    #[test]
    fn a_threads_size_comes_from_its_links_with_a_ceiling() {
        let head = |t: &super::Thread| HEAD.max(t.size[0] / 30.0);
        let side = |t: &super::Thread| (t.size[0] - 2.0 * GAP).max(t.size[1] - head(t) - GAP);
        let (lone, tied) = (super::lay_thread(&his(319), &[], 0), super::lay_thread(&his(319), &[], 16));
        assert!((side(&lone) - THREAD_SIDE).abs() < 1e-6 && (side(&tied) - 2.0 * THREAD_SIDE).abs() < 1e-6, "{} {}", side(&lone), side(&tied));
        assert!(lone.posts[0].w < CARD[0], "drawn smaller: {:?}", lone.posts[0]);
        for (a, b) in lone.posts.iter().zip(&tied.posts) {
            let twice = ((b.x - GAP) - 2.0 * (a.x - GAP)).abs() < 1e-6 && ((b.y - head(&tied)) - 2.0 * (a.y - head(&lone))).abs() < 1e-6 && (b.w - 2.0 * a.w).abs() < 1e-6;
            assert!(twice, "the same layout at twice the size: {a:?} {b:?}");
        }
        let small = super::lay_thread(&his(5), &[], 0);
        assert_eq!(small, super::lay_thread(&his(5), &[], 16), "inside its ceiling, as it was");
        assert!(small.posts.iter().all(|r| (r.w - CARD[0]).abs() < 1e-9), "{small:?}");
    }

    /// A thread is a small graph, not a table (paper-research N-1446 p1677, 2): its order a path, each post ROOM from
    /// the one before it unless it links one before that; a post with links a bigger card; the posts a link joins nearer
    /// than the thread's other pairs; and a later post moves none before it.
    #[test]
    fn a_threads_order_is_a_path_and_its_links_draw_its_posts_together() {
        let links = [(0, 7, 2), (8, 10, 1), (9, 11, 1)];
        let t = super::lay_thread(&his(12), &links, 0);
        let p = &t.posts;
        for k in 1..12 {
            let linked = links.iter().any(|(a, b, _)| *b == k && *a < k);
            assert!(linked || (gap(p[k - 1], p[k]) - ROOM).abs() < 1e-6, "post {k} next to {}: {:?} {:?}", k - 1, p[k - 1], p[k]);
        }
        assert!(p[0].w > p[1].w && p[10].w > p[1].w && (p[1].w - CARD[0]).abs() < 1e-9, "linked posts grow: {p:?}");
        let mid = |r: Rect| r.mid();
        let d = |a: usize, b: usize| (mid(p[a])[0] - mid(p[b])[0]).hypot(mid(p[a])[1] - mid(p[b])[1]);
        let mut all: Vec<f64> = (0..12).flat_map(|a| (a + 1..12).map(move |b| (a, b))).map(|(a, b)| d(a, b)).collect();
        all.sort_by(f64::total_cmp);
        let median = all[all.len() / 2];
        assert!(d(0, 7) < median, "linked nearer: 0-7 {} median {median}", d(0, 7));
        // the first eight, alone: they lie the same to each other (none of their cards grows for 8 to 11)
        let first = super::lay_thread(&his(8), &[(0, 7, 2)], 0);
        for k in 0..8 {
            let (a, b) = ([p[k].x - p[0].x, p[k].y - p[0].y], [first.posts[k].x - first.posts[0].x, first.posts[k].y - first.posts[0].y]);
            assert!((a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6, "post {k}: {a:?} with the later ones, {b:?} without");
        }
    }

    /// Answers' cards (proto-window N-1446 p1688): one card per piece, in their order; an answers' card right after its
    /// post under it, ROOM between them, and one anywhere else ROOM from its post (from the piece before it when OF names
    /// none before it); its size by its answers; the path through his posts, each with its card ROOM from his one before
    /// it with its card; and the links read by his posts' index, not the pieces'.
    #[test]
    fn an_answers_card_lies_by_its_post_and_the_path_runs_through_his() {
        let a = |of: usize, n: usize| Piece::Answers { of, n };
        let pieces = [Piece::His, a(0, 3), Piece::His, a(2, 12), Piece::His, Piece::His, a(5, 100), Piece::His, a(9, 1)];
        // his 0 and his 2: pieces 0 and 4
        let t = super::lay_thread(&pieces, &[(0, 2, 1)], 0);
        let p = &t.posts;
        assert_eq!(p.len(), pieces.len());
        for (i, x) in p.iter().enumerate() {
            for y in &p[i + 1..] {
                assert!(gap(*x, *y) >= ROOM - 1e-6, "{x:?} {y:?}");
            }
        }
        let share = |r: Rect| (r.w / CARD[0], r.h / CARD[1]);
        assert_eq!([share(p[1]), share(p[3]), share(p[6]), share(p[8])], [(0.75, 0.75), (1.25, 1.25), (2.0, 2.0), (0.75, 0.75)], "answers' cards by their answers: {p:?}");
        assert_eq!([share(p[0]), share(p[2]), share(p[4]), share(p[5])], [(1.25, 1.25), (1.0, 1.0), (1.25, 1.25), (1.0, 1.0)], "his 0 and his 2 grow by their link: {p:?}");
        for (card, post) in [(1, 0), (3, 2), (6, 5)] {
            let under = (p[card].y - p[post].bottom() - ROOM).abs() < 1e-6 && (p[card].mid()[0] - p[post].mid()[0]).abs() < 1e-6;
            assert!(under, "answers {card} under its post {post}: {:?} {:?}", p[card], p[post]);
        }
        // the first post with its card in the top left corner, under the title
        let head = super::HEAD.max(t.size[0] / 30.0);
        assert!((p[0].x.min(p[1].x) - GAP).abs() < 1e-6 && (p[0].y - head).abs() < 1e-6, "the first under the title: {p:?}");
        // piece 8's OF is no piece before it: on its own, by the piece before it
        assert!((gap(p[8], p[7]) - ROOM).abs() < 1e-6, "answers 8 by piece 7: {:?} {:?}", p[8], p[7]);
        // the path: his 1 (piece 2) by his 0, his 3 (piece 5) by his 2, his 4 (piece 7) by his 3, each with its card;
        // his 2 links his 0
        let with = |post: usize, card: Option<usize>| card.map_or(p[post], |c| Rect::new(p[post].x.min(p[c].x), p[post].y, p[post].w.max(p[c].w), p[c].bottom() - p[post].y));
        for ((post, card), (before, its)) in [((2, Some(3)), (0, Some(1))), ((5, Some(6)), (4, None)), ((7, None), (5, Some(6)))] {
            let (a, b) = (with(post, card), with(before, its));
            assert!((gap(a, b) - ROOM).abs() < 1e-6, "his piece {post} by his piece {before}: {a:?} {b:?}");
        }
    }

    /// A cloud the size of his (161 threads, one of 319 posts; N-1446 p1676), made up, printed for a look at it, never
    /// his: each thread's place (`THREAD i x y w h posts`), each link between threads (`LINK a b n`), and the largest
    /// thread's posts and their links (`POST k x y w h`, `PLINK a b n`).
    #[test]
    #[ignore]
    fn a_cloud_the_size_of_his_printed() {
        let mut posts: Vec<usize> = vec![319, 28, 24, 18, 15, 12, 10, 9, 8, 8, 7, 7, 6, 6, 6, 5, 5, 5, 5, 4, 4, 4, 4, 4, 4, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3];
        posts.extend(std::iter::repeat_n(2, 57));
        posts.extend(std::iter::repeat_n(1, 69));
        let mut seed = 7u64;
        let mut next = |m: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 33) as usize) % m.max(1)
        };
        // his posts in a thread name earlier ones in it; the large thread names many; a thread names a few others; most
        // of his posts have an answers' card after them
        let pieces: Vec<Vec<Piece>> = posts
            .iter()
            .map(|&n| {
                let mut v = Vec::new();
                for _ in 0..n {
                    v.push(Piece::His);
                    if next(3) > 0 {
                        v.push(Piece::Answers { of: v.len() - 1, n: 1 + next(6) });
                    }
                }
                v
            })
            .collect();
        let inside: Vec<Vec<(usize, usize, u32)>> = posts
            .iter()
            .map(|&n| {
                let mut l: BTreeMap<(usize, usize), u32> = BTreeMap::new();
                for k in 1..n {
                    if next(3) == 0 {
                        let j = k - 1 - next(k.min(12));
                        *l.entry((j, k)).or_default() += 1;
                    }
                }
                l.into_iter().map(|((a, b), c)| (a, b, c)).collect()
            })
            .collect();
        let mut between: BTreeMap<(usize, usize), u32> = BTreeMap::new();
        for i in 1..posts.len() {
            if i < 35 || next(4) == 0 {
                let j = if next(2) == 0 { 0 } else { next(i) };
                if j != i {
                    *between.entry((j.min(i), j.max(i))).or_default() += 1 + next(3) as u32;
                }
            }
        }
        let links: Vec<(usize, usize, u32)> = between.into_iter().map(|((a, b), c)| (a, b, c)).collect();
        let mut ties = vec![0u32; posts.len()];
        for (a, b, c) in &links {
            ties[*a] += c;
            ties[*b] += c;
        }
        let laid: Vec<super::Thread> = pieces.iter().zip(&inside).zip(&ties).map(|((v, l), t)| super::lay_thread(v, l, *t)).collect();
        let ids: Vec<String> = (0..posts.len()).map(|i| format!("N-{i}")).collect();
        let sized: Vec<(&str, [f64; 2])> = ids.iter().zip(&laid).map(|(id, t)| (id.as_str(), t.size)).collect();
        let at = super::place(&sized, &links, &BTreeMap::new());
        for (i, r) in at.iter().enumerate() {
            println!("THREAD {i} {:.1} {:.1} {:.1} {:.1} {}", r.x, r.y, r.w, r.h, posts[i]);
        }
        for (a, b, c) in &links {
            println!("LINK {a} {b} {c}");
        }
        // his posts by their index among his, the answers' cards after them by the piece's index
        let mut k = 0;
        for (i, (r, piece)) in laid[0].posts.iter().zip(&pieces[0]).enumerate() {
            match piece {
                Piece::His => {
                    println!("POST {k} {:.1} {:.1} {:.1} {:.1}", r.x, r.y, r.w, r.h);
                    k += 1;
                }
                Piece::Answers { n, .. } => println!("ANSWERS {i} {} {:.1} {:.1} {:.1} {:.1} {n}", k - 1, r.x, r.y, r.w, r.h),
            }
        }
        for (a, b, c) in &inside[0] {
            println!("PLINK {a} {b} {c}");
        }
        for (i, a) in at.iter().enumerate() {
            for b in &at[i + 1..] {
                assert!(gap(*a, *b) >= THREAD_ROOM - 1e-6);
            }
        }
    }

    /// The file: written and read back by project; none when there is none; a file that is not the places' left unread.
    #[test]
    fn the_places_are_kept_by_project_in_their_file() {
        let at = std::env::temp_dir().join(format!("yaaihh-cloud-place-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&at);
        assert_eq!(super::read(&at, "project:research"), Ok(BTreeMap::new()));
        let kept: BTreeMap<String, [f64; 4]> = [("N-1".to_string(), [1.0, 2.0, 3.0, 4.0])].into_iter().collect();
        super::write(&at, "project:research".into(), &kept);
        super::write(&at, "project:cav-paper".into(), &BTreeMap::new());
        assert_eq!(super::read(&at, "project:research"), Ok(kept));
        std::fs::write(&at, "not the places").unwrap();
        assert!(super::read(&at, "project:research").is_err());
        let _ = std::fs::remove_file(&at);
    }
}
