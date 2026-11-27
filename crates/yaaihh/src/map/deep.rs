//! The Map's deep levels (I-1478, docket-edge8's part; N-1104 p715 levels 5 to 7): what lies inside a post, and what
//! a step and a call draw once they are near enough.
//!
//! - **Level 5, a post's steps.** Each stage that a handler's run reached for the post is a step node inside the
//!   post, oldest run first, in its card at the start of its row ([`card`]), under its text. The runs are those
//!   `TicketView.ran` lists for the post (`ViewRan.post`). p715: "the steps that handled it (held 15 s → route →
//!   paper-research), each a node".
//! - **A handler's run** (`Kind::Run`, in Programs: map/join.rs) holds its own stages as a post does, in a card of the
//!   same shape (p715 path 3: "double-click one run: its template, then the wire"). They are other nodes than that
//!   run's steps inside the post it ran for: their ids carry the run ([`own`]).
//! - **Level 5, a brain's marks on a post** (`Kind::Brain`; paper-research N-1446 p1108, docket-edge6 p1125). Each
//!   pass of an agent's brain standing at the post is a mark inside its card, in the right of its text's share, side by
//!   side oldest first ([`place_passes`]). A pass holds its parts (attend, elect, and what they asked) as a run holds
//!   its stages, in a card of the same shape. A pass is a ticket the snapshot shows, never a handler's run (`ran` lists
//!   only tickets that run a handler), so its stages lie under its mark alone and none is asked of coordd.
//! - **Level 6, a step.** A step draws its stage as a ticket's panel does: its words, whom it went to and its state,
//!   what it gave, and its step in code, folded. Nearer still it draws its prompt with its Edit. That Edit shows what
//!   it reaches as he types, and Save writes it (path 5).
//! - **Level 7, the wire.** Each round of a step on the wire is a call node inside the step. It draws what was sent
//!   and what came back.
//!
//! Each of these is drawn by what draws it on its own page, never by a second copy:
//! - the stage by picture/stages.rs `stage_panel`;
//! - the prompt by edit.rs `prompt`;
//! - the wire by lens.rs `lines`.
//!
//! What this module adds is where they lie and when they are read.
//!
//! **Places.** A node is placed by its parent's rectangle alone, in a slot of a grid of fixed shares of it. A stage
//! reached later takes the first free slot and moves none, which is the Map's promise that a node keeps its place.
//!
//! **Reading.** The snapshot holds no run, since runs are hidden tickets. So a post's runs are asked of coordd as
//! `Request::Ticket` once the post is drawn near enough to show its steps ([`want`]). Each is asked once and never
//! again on a later frame, and nothing is asked while the pipe is down. A run held while it was going is asked again
//! once the thread says it ended. The run a press opens in the Docket is fetched the same way (find.rs `number`), but
//! there the press waits for it. A call's lines are asked once it is near (lens.rs `read`).
//!
//! **Islands.** Every project is an island of the one canvas, and each reads its own: a post's runs, a step's prompt
//! (from that project's calls) and a call's wire are asked of that island's coordd. An edit opens on the current
//! project's files and its Save writes there, so a step's Edit is drawn only on the current project's island; another's
//! step says where its prompt is edited (consult-3 N-1479 p27).
//!
//! **Ids.**
//! - A step is `step:` followed by its stage's key: the run, then where the stage is drawn among its stages
//!   (`step:O-1099511630339.1`). The key is what opens that stage.
//! - A call is its round's step on the wire (`O-1099511630339.s2`), which `lens::zoom` opens at Wire. A call is a
//!   round, and an agent's turn's requests lie inside it (proto-window N-1479 p13, 2).
//! - Inside a handler's run `run:{run}`, each carries the run first: `run:{run}/step:{key}`, `run:{run}/{run}.s2`
//!   (docket-edge8 N-1479 p24, docket-edge6 p25). What it opens and reads is the part after the `/` ([`own`]), so both
//!   copies draw and read the one stage and the one wire.
//! - A brain's mark is `brain:{pass}`, its pass's ticket id ([`brain_node`]); its parts carry it as a run's carry the
//!   run: `brain:{pass}/step:{key}`, `brain:{pass}/{pass}.s2` (docket-edge6 N-1446 p1125).
//!
//! **Not drawn here:**
//! - the prompt filled in for the post before it is sent: the prompt as sent is the wire's, on the `model-for` line;
//! - which account a call of coordd's own went on, which the window has nothing to read for: path 3's first step,
//!   from an account to its calls (docket-edge8 N-1479 p17).

use super::{Graph, Kind, Node, Rect, Refused, State};
use crate::App;
use crate::lens::Depth;
use coord_types::{Request, TicketView};
use rune::{El, col, text};
use rune_kit::Kit;
use std::collections::{HashMap, HashSet};

/// The share of a post's card ([`card`]) kept for its text. Its steps lie in the band below.
pub(crate) const POST_TEXT: f64 = 0.6;
/// A post's card, this many times as wide as it is tall (3:2).
const CARD: f64 = 1.5;
/// A post's step slots, across and down. A run reaches a few stages, so twelve slots hold a post's runs. A stage past
/// them is not placed, though the stage panel of the step before it still draws the run's stages.
const STEP_GRID: (usize, usize) = (6, 2);
/// Where in a post's card ([`card`]) its brain's marks start, a share of its width: they lie at the right of its
/// text, which keeps the share before it once the post has a mark ([`marks_room`]).
pub(crate) const MARKS_FROM: f64 = 0.5;
/// A post's mark slots, across and down, filled across first: side by side, oldest first. A post to two agents has a
/// pass for each, and a check on it is one more, so four hold a post's passes. A pass past them is not placed.
const MARK_GRID: (usize, usize) = (2, 2);
/// The share of a step's height kept for its stage. Its calls lie in the band below.
const STEP_TEXT: f64 = 0.5;
/// A step's call slots, across and down (a step in a loop has a round each time).
const CALL_GRID: (usize, usize) = (4, 2);
/// The share of a slot left on each side, so that nodes side by side do not touch.
const PAD: f64 = 0.06;
/// How many points wide on screen a step must be to draw its stage. Below that the canvas draws its label.
pub const STEP_PX: f32 = 220.0;
/// How many points wide on screen a step must be to draw its prompt as well.
pub const PROMPT_PX: f32 = 460.0;
/// How many points wide on screen a call must be to draw its lines on the wire.
pub const CALL_PX: f32 = 260.0;

/// Whether a node of kind K drawn PX wide is drawn over the nodes it holds: a step drawing its stage. Its stage, its
/// prompt and its edit are as tall as their lines, which do not shrink with the step as its [`STEP_TEXT`] share does,
/// so they run down onto its calls' band: the edit does at the step's fit (path 5's stills, where its Wire call lay over
/// the editor's lines). The canvas draws such a step after its calls and their labels, its frame unfilled so that they
/// show through it undimmed, and its panel filled ([`inside`]), so that a call shows where the panel does not reach and none lies over
/// its words or presses.
pub fn over_calls(k: Kind, px: f32) -> bool {
    k == Kind::Step && px >= STEP_PX
}

/// What the deep levels hold (`App::deep`).
#[derive(Default)]
pub struct Deep {
    /// The runs asked of coordd, each by project, its id, and whether the thread said it had ended when it was asked:
    /// a run held while it was going is asked once more after it ends.
    asked: HashSet<(usize, String, bool)>,
    /// Why a run asked did not come, by project and id.
    pub failed: HashMap<(usize, String), String>,
    /// The calls whose wire [`near`] asked of each project, by project, id and what was read: the agent whose turn it
    /// is (lens.rs `turn_of`), none for its lines of the record. Once each, so a refusal is not asked again on the next
    /// frame (consult-3 N-1479 p33), and a round passed to another agent is (p42). Asked again once the pipe to that
    /// project ends and is back.
    read: HashSet<(usize, String, Option<String>)>,
    /// Windows: the projects whose files [`near`] asked for, for their steps' prompts (edit.rs `serve_prompts`): until
    /// they come, or, refused, until the pipe ends.
    pub files: HashSet<usize>,
    /// The step on the Map whose prompt is being edited, by its project and its node's id: its editor is drawn there,
    /// not in another island's step of the same id (run ids repeat across projects).
    pub editing: Option<(usize, String)>,
}

/// The id of the step node for stage KEY (`O-1099511630339.1`).
pub fn step_id(key: &str) -> String {
    format!("step:{key}")
}

/// The stage key a step node's ID names; None for any other node.
pub fn stage_of(id: &str) -> Option<&str> {
    own(id).strip_prefix("step:")
}

/// A deep node's ID as its run's stages and wire name it: inside a handler's run, the part after the run's prefix
/// (`run:O-1099511630339/step:O-1099511630339.1` is `step:O-1099511630339.1`); inside a post, ID itself. No stage key
/// nor round has a `/`.
pub fn own(id: &str) -> &str {
    id.rsplit_once('/').map_or(id, |(_, x)| x)
}

/// The run a handler's run node ID stands for (`run:O-1099511630339`); None for any other node.
pub fn run_node(id: &str) -> Option<&str> {
    id.strip_prefix("run:").filter(|r| !r.contains('/'))
}

/// The pass a brain's mark node ID stands for (`brain:N-1500`, the pass's ticket); None for any other node.
pub fn brain_node(id: &str) -> Option<&str> {
    id.strip_prefix("brain:").filter(|r| !r.contains('/'))
}

/// The run a stage KEY is drawn in: its first part. A step's lines of the record are its run's (canvas.rs `record_of`).
pub(crate) fn run_of(key: &str) -> &str {
    key.split('.').next().unwrap_or(key)
}

/// The card of the post whose rectangle is R: level 5 (docket-edge8 N-1479 p71; docket-edge6 and proto-window, p80).
/// - Level 4 draws a post as its line's row, one line of its words across. Level 5 is a card of a fixed shape at the
///   row's start: its text in the top [`POST_TEXT`] share, and the steps that handled it in the band below.
/// - A laid line's row is 180 × 3.75 (map/layout.rs `Rows`), 48 to 1. A step's slot there would be 40 to 1, so its
///   panel could never open at any zoom; in the card (5.625 × 3.75) it is 1.25 to 1.
/// - A post no wider than its card is its own card.
pub fn card(r: Rect) -> Rect {
    Rect::new(r.x, r.y, (r.h * CARD).min(r.w), r.h)
}

/// Where the brain's marks lie in a post's card C ([`card`]): from [`MARKS_FROM`] of its width to its right edge, and
/// as tall as makes each of [`MARK_GRID`]'s slots a card's shape (3:2). Within its text's share ([`POST_TEXT`]) on any
/// card: half of a 3:2 card's width at 3:2 is 0.5 of its height, and a narrower card's is less. A mark is so 0.22 of
/// its post's card wide, its margin taken off: under a third (docket-edge6 N-1446 p1125).
pub(crate) fn marks_room(c: Rect) -> Rect {
    let w = c.w * (1.0 - MARKS_FROM);
    Rect::new(c.right() - w, c.y, w, w / CARD * MARK_GRID.1 as f64 / MARK_GRID.0 as f64)
}

/// R's band from TOP, a share of its height, down to its foot.
fn band(r: Rect, top: f64) -> Rect {
    Rect::new(r.x, r.y + r.h * top, r.w, r.h * (1.0 - top))
}

/// Slot I of a grid of GRID's columns and rows over B, its margin taken off; None past the last.
pub(crate) fn slot(b: Rect, i: usize, (cols, rows): (usize, usize)) -> Option<Rect> {
    if i >= cols * rows {
        return None;
    }
    let (w, h) = (b.w / cols as f64, b.h / rows as f64);
    let (x, y) = (b.x + (i % cols) as f64 * w, b.y + (i / cols) as f64 * h);
    Some(Rect::new(x + w * PAD, y + h * PAD, w * (1.0 - 2.0 * PAD), h * (1.0 - 2.0 * PAD)))
}

/// The first slot of GRID over B that none of node AT's children lies in.
pub(crate) fn free(g: &Graph, at: u32, b: Rect, grid: (usize, usize)) -> Option<Rect> {
    let taken: Vec<Rect> = g.children(at).iter().map(|c| g.node(*c).rect).collect();
    (0..grid.0 * grid.1).filter_map(|i| slot(b, i, grid)).find(|r| !taken.contains(r))
}

/// Place inside post, run or brain's mark HOST of G the steps that handled it, and their rounds on the wire.
/// - The steps come from RUNS, the runs a handler made for the post, oldest first ([`runs`]); a handler's run's from
///   itself ([`held_run`]); a mark's from its pass ([`place_passes`]). LABELS gives their words.
/// - Inside a run or a mark, each id carries it first ([`own`]): they are other nodes than the same stages inside the
///   post the run was for.
/// - A stage already placed keeps its place, and takes its state as it is now.
/// - A new stage takes the first free slot of the band under the text of the post's [`card`]. A round given takes the
///   first free slot under its step's stage.
/// - A step and a call are found in the post's own project (`Graph::find_in`): run ids repeat across projects, since
///   each coordd numbers its hidden tickets from `HIDDEN`, and each island holds its own.
/// - Refused as `Graph::push` refuses. Each slot lies inside its parent, so no node is placed outside it.
pub fn place(g: &mut Graph, host: u32, runs: &[&TicketView], labels: &crate::picture::Labels) -> Result<(), Refused> {
    let (steps, root) = (band(card(g.node(host).rect), POST_TEXT), g.root_of(host));
    let pre = if matches!(g.node(host).kind, Kind::Run | Kind::Brain) { format!("{}/", g.node(host).id) } else { String::new() };
    for v in runs {
        for st in crate::picture::reached(v, labels) {
            let id = format!("{pre}{}", step_id(&st.key));
            let at = match g.find_in(root, &id) {
                Some(i) => i,
                None => {
                    let Some(r) = free(g, host, steps, STEP_GRID) else { return Ok(()) };
                    g.push(Node::new(id, Kind::Step, st.words.clone(), Some(host), r))?
                }
            };
            let (state, weight) = (if st.now { State::Working } else { State::Plain }, st.rounds.len() as f32);
            g.update(at, |n| (n.state, n.weight) = (state, weight));
            // its rounds as its panel's presses down to the wire name them (stages.rs `wire_presses`), but for one round
            // "step 1", its step's, not "Wire", the layer's word, which his crumbs and the inspector's title then read as
            // the call's name (proto-window N-1446 p1172)
            let calls = band(g.node(at).rect, STEP_TEXT);
            let once = st.rounds.len() == 1;
            for (j, step) in st.rounds.iter().enumerate() {
                let cid = format!("{pre}{}.s{step}", v.id);
                if g.find_in(root, &cid).is_some() {
                    continue;
                }
                let Some(r) = free(g, at, calls, CALL_GRID) else { break };
                let label = if once { format!("step {step}") } else { format!("round {} · step {step}", j + 1) };
                g.push(Node::new(cid, Kind::Call, label, Some(at), r))?;
            }
        }
    }
    Ok(())
}

/// The brain's passes standing at post N of thread V in project PI, oldest first (by number, the order they opened
/// in), each with its line in V's outline: the tickets there whose kind says `drawn note` (outline.rs `standing`), at
/// work, come to nothing or spoken. What [`place_passes`] marks the post with.
pub fn passes<'a>(s: &'a App, pi: usize, v: &'a TicketView, n: u32) -> Vec<(&'a docket_machine::ViewOutline, &'a TicketView)> {
    let Some(p) = s.projects.get(pi) else { return Vec::new() };
    let mut out: Vec<_> = crate::outline::standing(p, v, |c| c.at_post == Some(n)).into_iter().filter(|(_, t)| t.drawn == Some(docket_machine::syntax::Drawn::Note)).collect();
    out.sort_by_key(|(_, t)| t.n);
    out
}

/// Mark post HOST of G with PASSES, the brain's passes standing at it, oldest first ([`passes`]), and place each
/// pass's parts inside its mark ([`place`]), as a run's stages lie inside its card.
/// - A mark is `brain:{pass}`, labelled by the agent the pass is for (its form's `agent`). Its note is what it came to,
///   as coordd words it on its line (`said`: Quiet, Said, a failure's why), else its line's state ("at work"). It is at
///   work while its pass is open.
/// - A new mark takes the first free slot of [`MARK_GRID`] over the card's [`marks_room`], and moves none; a mark
///   placed keeps its place, and takes its note and state as they are now.
/// - Refused as `Graph::push` refuses.
pub fn place_passes(g: &mut Graph, host: u32, passes: &[(&docket_machine::ViewOutline, &TicketView)], labels: &crate::picture::Labels) -> Result<(), Refused> {
    let (room, root) = (marks_room(card(g.node(host).rect)), g.root_of(host));
    for (c, v) in passes {
        let id = format!("brain:{}", v.id);
        let at = match g.find_in(root, &id) {
            Some(i) => i,
            None => {
                let Some(r) = free(g, host, room, MARK_GRID) else { return Ok(()) };
                let agent = v.form.get("agent").and_then(serde_json::Value::as_str).unwrap_or(&v.id).to_string();
                g.push(Node::new(id, Kind::Brain, agent, Some(host), r))?
            }
        };
        let (note, state) = (c.said.clone().unwrap_or_else(|| c.state.clone()), if c.open { State::Working } else { State::Plain });
        g.update(at, |n| (n.note, n.state) = (note, state));
        place(g, at, &[*v], labels)?;
    }
    Ok(())
}

/// The runs of a handler made for post N of thread V in project PI that are held, oldest first: what [`place`] places
/// from.
pub fn runs<'a>(s: &'a App, pi: usize, v: &TicketView, n: u32) -> Vec<&'a TicketView> {
    let Some(p) = s.projects.get(pi) else { return Vec::new() };
    let held: HashMap<&str, &TicketView> = s.tickets.find.fetched_in(&p.id).map(|r| (r.id.as_str(), r)).collect();
    v.ran.iter().filter(|r| r.post == Some(n)).filter_map(|r| held.get(r.id.as_str()).copied()).collect()
}

/// Handler's run RUN of project PI, when it is held: what [`place`] places inside its node.
pub fn held_run<'a>(s: &'a App, pi: usize, run: &str) -> Option<&'a TicketView> {
    let p = s.projects.get(pi)?;
    s.tickets.find.fetched_in(&p.id).find(|v| v.id == run)
}

/// Run RUN of project PI as a ticket of its snapshot lists it (`TicketView::ran`): a handler's run node's.
fn listed<'a>(s: &'a App, pi: usize, run: &str) -> Option<&'a docket_machine::ViewRan> {
    s.projects.get(pi)?.live.tickets.values().flat_map(|t| &t.ran).find(|r| r.id == run)
}

/// Ask coordd for the runs of post N of thread THREAD in project PI that are not held yet, once each. The canvas calls
/// it when the post is first drawn near enough to show its steps. A run held while it was going is asked once more
/// after the thread says it ended. Nothing is asked while the pipe is down or the window has no link to it, as split.rs
/// `poll` asks nothing then (N-1446 p284). The answers come as [`came`].
pub fn want(s: &mut App, pi: usize, thread: u64, n: u32) {
    let Some(v) = s.projects.get(pi).and_then(|p| p.live.tickets.get(&thread)) else { return };
    let ran: Vec<(String, bool)> = v.ran.iter().filter(|r| r.post == Some(n)).map(|r| (r.id.clone(), r.closed_at.is_some())).collect();
    ask_runs(s, pi, ran);
}

/// [`want`] for a handler's run RUN of project PI, drawn open: itself, as its snapshot lists it.
pub fn want_run(s: &mut App, pi: usize, run: &str) {
    let Some(r) = listed(s, pi, run) else { return };
    let ran = vec![(r.id.clone(), r.closed_at.is_some())];
    ask_runs(s, pi, ran);
}

/// [`want`]'s asking: of RAN, each run's id and whether its ticket says it ended, those not held yet, or held while
/// going and ended since, and not asked yet.
fn ask_runs(s: &mut App, pi: usize, ran: Vec<(String, bool)>) {
    let Some(p) = s.projects.get(pi) else { return };
    if p.link.is_none() || p.down.is_some() {
        return;
    }
    let held: HashMap<&str, bool> = s.tickets.find.fetched_in(&p.id).map(|r| (r.id.as_str(), r.closed_at.is_some())).collect();
    let ask: Vec<(String, bool)> = ran
        .into_iter()
        .filter(|(id, ended)| match held.get(id.as_str()) {
            Some(was_ended) => !was_ended && *ended,
            None => true,
        })
        .filter(|(id, ended)| !s.deep.asked.contains(&(pi, id.clone(), *ended)))
        .collect();
    for (id, ended) in ask {
        if s.request_in(pi, Request::Ticket { t: id.clone() }, crate::Pending::Run { id: id.clone() }) {
            s.deep.asked.insert((pi, id, ended));
        }
    }
}

/// coordd's answer for run ID of project PI. Its view is held where a run pressed in the Docket is held
/// (`find.fetched`). If it did not come, the reason is kept for the post to say and is written to yaaihh.log. The
/// Map asked for it, not he, so nothing goes on the page's bar.
pub(crate) fn came(s: &mut App, pi: usize, id: String, ok: bool, out: String) {
    let Some(pid) = s.projects.get(pi).map(|p| p.id.clone()) else { return };
    let got = if ok { serde_json::from_str::<TicketView>(&out).map_err(|e| format!("unreadable answer: {e}")) } else { Err(out.trim().to_string()) };
    match got {
        Ok(v) => {
            s.deep.failed.remove(&(pi, id));
            s.tickets.find.fetched.entry(pid).or_default().insert(v.n, v);
        }
        Err(e) => {
            crate::log::line(&format!("map: run {id} did not come: {e}"));
            s.deep.failed.insert((pi, id), e);
        }
    }
}

/// Run ID of project PI was asked on a pipe that ended before its answer. It is asked again once the pipe is back and
/// its post is near.
pub(crate) fn lost(d: &mut Deep, pi: usize, id: &str) {
    d.asked.retain(|(p, x, _)| !(*p == pi && x == id));
}

/// What post N of thread V in project PI says in its steps' band while there is no step to show there: that its steps
/// are being read, or why they did not come. None when no run was made for it, or once each run is held.
pub fn note(s: &App, pi: usize, v: &TicketView, n: u32) -> Option<String> {
    let ran: Vec<&str> = v.ran.iter().filter(|r| r.post == Some(n)).map(|r| r.id.as_str()).collect();
    note_of(s, pi, &ran)
}

/// [`note`] for a handler's run RUN of project PI: what its card says while its steps are not there.
pub fn note_run(s: &App, pi: usize, run: &str) -> Option<String> {
    listed(s, pi, run)?;
    note_of(s, pi, &[run])
}

/// [`note`]'s words for the runs RAN of project PI.
fn note_of(s: &App, pi: usize, ran: &[&str]) -> Option<String> {
    let p = s.projects.get(pi)?;
    let missing: Vec<&str> = ran.iter().copied().filter(|id| !s.tickets.find.fetched_in(&p.id).any(|x| x.id == *id)).collect();
    let why = missing.iter().find_map(|id| s.deep.failed.get(&(pi, id.to_string())));
    match (missing.is_empty(), why) {
        (true, _) => None,
        (false, Some(e)) => Some(format!("Its steps did not come: {e}")),
        (false, None) if p.down.is_some() || p.link.is_none() => Some(format!("{} disconnected; steps load on reconnect.", p.name)),
        (false, None) => Some("Reading its steps…".into()),
    }
}

/// What node I of G (in project PI) draws inside its rectangle, PX points wide on screen.
/// - A step at least [`STEP_PX`] wide draws its stage as a ticket's panel does. From [`PROMPT_PX`] on it draws its
///   prompt too, from PI's calls, with its Edit when PI is the current project (path 5).
/// - A call at least [`CALL_PX`] wide draws its lines on the wire, read from PI's coordd. While the pipe to it is down
///   and none are held, it says they are read once it is back.
/// - None below that, or for a node of another kind: the canvas draws the label. None too until its run is held.
pub fn inside(s: &App, pi: usize, g: &Graph, i: u32, px: f32, k: &Kit) -> Option<El> {
    let n = g.node(i);
    match n.kind {
        Kind::Step if px >= STEP_PX => step(s, pi, &n.id, px >= PROMPT_PX, k),
        Kind::Call if px >= CALL_PX => {
            let p = s.projects.get(pi)?;
            let c = col().key(format!("map-call-{}", n.id)).w_full();
            let wire = own(&n.id);
            Some(if (p.link.is_none() || p.down.is_some()) && !crate::lens::held(s, pi, Depth::Wire, wire) {
                c.child(text(format!("{} disconnected; wire loads on reconnect.", p.name)).font(k.ty.small).color(k.c.text_muted).wrap())
            } else {
                c.child(crate::lens::lines(s, pi, Depth::Wire, wire, k))
            })
        }
        _ => None,
    }
}

/// What the canvas asks of coordd for node I of G (in project PI) while it is drawn at PX wide. The canvas may call it
/// on every frame the node is near. Nothing is asked while the pipe is down or the window has no link to it, as
/// [`want`] asks nothing then.
/// - A call reads its lines on the wire of PI's coordd once for each pipe and each thing read (lens.rs `turn_of`): an
///   agent's round is the turn of the agent it is with now, so a round passed up to another is that one's turn, asked
///   at once (consult-3 N-1479 p42); a round of coordd's own is its lines of the record. A refusal is drawn, and asked
///   again only once the pipe has ended and is back ([`ended`]; a turn's read the pipe's end lost is no longer on its
///   way then, raw.rs `lost`), or by the lens's own Read again.
/// - On Windows, a step drawing its prompt on the current project's island asks for that project's files, once, when
///   no edit has asked for them (edit.rs `prompts_unserved`): path 5's template is there before any Edit.
/// - Anything else asks nothing.
pub fn near(s: &mut App, pi: usize, g: &Graph, i: u32, px: f32) {
    let n = g.node(i);
    if !s.projects.get(pi).is_some_and(|p| p.link.is_some() && p.down.is_none()) {
        return;
    }
    match n.kind {
        Kind::Call if px >= CALL_PX => {
            // a call inside a handler's run reads the one wire its copy inside the post reads ([`own`])
            let wire = own(&n.id).to_string();
            let to = crate::lens::turn_of(s, pi, Depth::Wire, &wire).map(|t| t.0);
            if !s.deep.read.insert((pi, wire.clone(), to.clone())) {
                return;
            }
            // its lines' refusal held from an earlier pipe, asked again on this one (a turn's read asks again of
            // itself: raw.rs `open_turn` stops only at its lines held or on their way)
            let key = (pi, wire.clone());
            if to.is_none() && s.records.got.get(&key).is_some_and(Result::is_err) {
                s.records.got.remove(&key);
            }
            crate::lens::read(s, pi, Depth::Wire, &wire)
        }
        Kind::Step if px >= PROMPT_PX && crate::edit::prompts_unserved(s, pi) && s.deep.files.insert(pi) => {
            if !crate::edit::serve_prompts(s) {
                s.deep.files.remove(&pi);
            }
        }
        _ => {}
    }
}

/// The pipe to project PI's coordd ended: what [`near`] asked of it, answered or not, is asked again once it is back.
pub(crate) fn ended(d: &mut Deep, pi: usize) {
    d.read.retain(|(p, _, _)| *p != pi);
    d.files.remove(&pi);
}

/// Step node ID's stage, as its run's ticket panel draws it, and its prompt when PROMPT and its call has one; on a
/// filled panel, as the step lies over its calls ([`over_calls`]). Its run is one a fetch holds, or a brain's pass,
/// which the snapshot holds (find.rs `views`).
fn step(s: &App, pi: usize, id: &str, prompt: bool, k: &Kit) -> Option<El> {
    let key = stage_of(id)?;
    let p = s.projects.get(pi)?;
    let v = crate::find::views(s, p).find(|v| v.id == run_of(key))?;
    let pg = &s.tickets;
    // what it draws held by reference is asked of its island's coordd (texts.rs `drawn_in`)
    let words = |x: &serde_json::Value| crate::texts::drawn_in(s, pi, || crate::texts::words(pg, x));
    let pic = crate::picture::Pic { k, p: Some(p), kinds: crate::picture::kind_texts(pg), tickets: p.live.tickets.values().collect(), open: &pg.pic_open, marks: crate::edit::marks(pg), words: &words, pick: Some(v.n), picked: None };
    let labels = crate::picture::labels(s, p);
    let mut c = col().key(format!("map-step-{key}")).w_full().gap(10.0).fill(k.c.layer_01).radius(k.radius.sm).child(pic.stage_panel(&v.program, v, key, &labels)?);
    if prompt && let Some((call, who)) = crate::picture::reached(v, &labels).into_iter().find(|r| r.key == key).and_then(|r| Some((r.call?, r.who))) {
        let mark = (pi, id.to_string());
        let editing = s.deep.editing.as_ref() == Some(&mark);
        c = c.children(crate::edit::prompt(s, pi, &call, who.as_deref(), editing, &format!("map-edit-{key}"), move |s: &mut App| s.deep.editing = Some(mark.clone()), k).into_iter().flatten());
    }
    Some(c)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::picture::Labels;

    // `cfg(not(windows))`: not in the Windows test exe (docket-edge6 N-1479 p64). A test with a pipe to coordd writes
    // it into `sh -c cat`, and Windows has no sh; the two prompt tests read the project's files from here, which the
    // Windows window never does (`elsewhere::away`). What they test is the same code on both: the Windows branch runs
    // on Linux under `elsewhere::as_on_windows`.

    /// The fixture's thread N-7, a project around it, and its post 1, each in a rectangle of one world.
    fn graph() -> (Graph, u32) {
        let g = Graph::new(
            vec![
                Node::new("project:research", Kind::Project, "research", None, Rect::new(0.0, 0.0, 10_000.0, 8_000.0)),
                Node::new("N-7", Kind::Thread, "N-7", Some(0), Rect::new(100.0, 100.0, 400.0, 600.0)),
                Node::new("N-7.p1", Kind::Post, "p1", Some(1), Rect::new(110.0, 120.0, 380.0, 60.0)),
            ],
            vec![],
        )
        .unwrap();
        (g, 2)
    }

    fn held(s: &App) -> Vec<&TicketView> {
        let v = &s.projects[0].live.tickets[&7];
        runs(s, 0, v, 1)
    }

    /// Level 5 (p715): the stages a run reached for N-7's post 1 lie inside the post, each with its rounds on the wire
    /// inside it. The fixture is the run of docket.rs `wire_app`, made up: `answer` was given to pr once, the watcher's
    /// `next` was given twice, and `check` has not been reached. A stage reached later takes the next free slot, and
    /// what was placed keeps its place.
    #[test]
    fn a_posts_steps_lie_inside_it_with_their_rounds_and_keep_their_places_as_more_are_reached() {
        let mut s = crate::picture::docket::tests::wire_app();
        let id = crate::picture::docket::tests::run_id(1);
        let (mut g, post) = graph();
        place(&mut g, post, &held(&s), &Labels::default()).unwrap();
        let steps: Vec<(&str, &str)> = g.children(post).iter().map(|i| (g.node(*i).id.as_str(), g.node(*i).label.as_str())).collect();
        assert_eq!(steps, [(format!("step:{id}.0").as_str(), "answer"), (format!("step:{id}.1").as_str(), "next")], "the stages reached, not the check to come");
        let calls = |g: &Graph, i: u32| -> Vec<(String, String)> { g.children(i).iter().map(|c| (g.node(*c).id.clone(), g.node(*c).label.clone())).collect() };
        let (answer, next) = (g.find_in(0, &format!("step:{id}.0")).unwrap(), g.find_in(0, &format!("step:{id}.1")).unwrap());
        assert_eq!(calls(&g, answer), [(format!("{id}.s1"), "step 1".to_string())]);
        assert_eq!(calls(&g, next), [(format!("{id}.s2"), "round 1 · step 2".to_string()), (format!("{id}.s3"), "round 2 · step 3".to_string())]);
        for (i, n) in g.nodes().iter().enumerate().skip(post as usize + 1) {
            let up = g.node(n.parent.unwrap()).rect;
            assert!(up.holds(&n.rect), "{} lies inside its parent", n.id);
            assert_eq!(g.path(i as u32)[..3].iter().map(|x| g.node(*x).id.as_str()).collect::<Vec<_>>(), ["project:research", "N-7", "N-7.p1"], "its crumbs go through its post");
        }
        // the check reached and given once: its step takes the next slot, nothing placed before moves
        let before: Vec<(String, Rect)> = g.nodes().iter().map(|n| (n.id.clone(), n.rect)).collect();
        let pid = s.projects[0].id.clone();
        let run = s.tickets.find.fetched.get_mut(&pid).unwrap().values_mut().find(|v| v.id == id).unwrap();
        run.run[2] = serde_json::from_value(serde_json::json!({"what": "step", "line": 3, "source": 0, "text": "c <- step role pr check(n)", "state": "now", "who": "pr", "rounds": [{"step": 4, "to": "pr", "given": crate::picture::docket::tests::run_at(14)}]})).unwrap();
        place(&mut g, post, &held(&s), &Labels::default()).unwrap();
        for (nid, r) in &before {
            assert_eq!(g.node(g.find_in(0, nid).unwrap()).rect, *r, "{nid} keeps its place");
        }
        let check = g.find_in(0, &format!("step:{id}.2")).expect("the check, placed once reached");
        assert_eq!((g.node(check).state, g.node(answer).state), (State::Working, State::Plain));
        assert_eq!(calls(&g, check), [(format!("{id}.s4"), "step 4".to_string())]);
        assert!(!g.nodes().iter().any(|n| g.nodes().iter().filter(|m| m.parent == n.parent && m.id != n.id).any(|m| m.rect == n.rect)), "no two siblings share a slot");
    }

    /// consult-3 N-1479 p27, docket-edge6 p35: run ids repeat across projects (each coordd numbers its hidden tickets
    /// from `HIDDEN`). Two islands' posts that ran runs of the same ids each hold their own steps and calls, keyed by
    /// their project: none is refused as twice, none goes into the other island's post, and placing again adds none.
    #[test]
    fn two_islands_whose_runs_share_ids_each_hold_their_own_steps() {
        let s = crate::picture::docket::tests::wire_app();
        let mut g = Graph::new(
            vec![
                Node::new("project:research", Kind::Project, "research", None, Rect::new(0.0, 0.0, 10_000.0, 8_000.0)),
                Node::new("N-7", Kind::Thread, "N-7", Some(0), Rect::new(100.0, 100.0, 400.0, 600.0)),
                Node::new("N-7.p1", Kind::Post, "p1", Some(1), Rect::new(110.0, 120.0, 380.0, 60.0)),
                Node::new("project:cav", Kind::Project, "cav", None, Rect::new(20_000.0, 0.0, 10_000.0, 8_000.0)),
                Node::new("N-7", Kind::Thread, "N-7", Some(3), Rect::new(20_100.0, 100.0, 400.0, 600.0)),
                Node::new("N-7.p1", Kind::Post, "p1", Some(4), Rect::new(20_110.0, 120.0, 380.0, 60.0)),
            ],
            vec![],
        )
        .unwrap();
        let runs = held(&s);
        place(&mut g, 2, &runs, &Labels::default()).unwrap();
        place(&mut g, 5, &runs, &Labels::default()).unwrap();
        let (step, call) = (step_id(&format!("{}.0", run(1))), format!("{}.s1", run(1)));
        let (r, c) = (g.find_in(0, &step).unwrap(), g.find_in(3, &step).unwrap());
        assert_eq!((g.node(r).parent, g.node(c).parent), (Some(2), Some(5)), "each island's step in its own post");
        assert_eq!((g.node(g.find_in(0, &call).unwrap()).parent, g.node(g.find_in(3, &call).unwrap()).parent), (Some(r), Some(c)));
        assert!(g.nodes().iter().filter(|n| n.kind == Kind::Step || n.kind == Kind::Call).all(|n| g.nodes()[n.parent.unwrap() as usize].rect.holds(&n.rect)));
        let n = g.nodes().len();
        place(&mut g, 2, &runs, &Labels::default()).unwrap();
        place(&mut g, 5, &runs, &Labels::default()).unwrap();
        assert_eq!(g.nodes().len(), n, "placed again: none added");
    }

    /// docket-edge6 N-1479 p64, the shape agreed at p80: a laid line's post is a row, 180 × 3.75 (64 rows in a
    /// 240-high line), where a step's slot was 40 to 1. Its steps and calls lie in its card at the row's start, 3:2,
    /// each slot about as tall as it is wide, and none in the rest of the row, where level 4 draws its words.
    #[test]
    fn a_laid_posts_steps_lie_in_its_card_at_the_rows_start_each_about_as_tall_as_wide() {
        let s = crate::picture::docket::tests::wire_app();
        let row = Rect::new(100.0, 100.0 + 240.0 / 64.0, 180.0, 240.0 / 64.0);
        let mut g = Graph::new(
            vec![
                Node::new("project:research", Kind::Project, "research", None, Rect::new(0.0, 0.0, 10_000.0, 8_000.0)),
                Node::new("N-7", Kind::Thread, "N-7", Some(0), Rect::new(100.0, 100.0, 180.0, 240.0)),
                Node::new("N-7.p1", Kind::Post, "p1", Some(1), row),
            ],
            vec![],
        )
        .unwrap();
        place(&mut g, 2, &held(&s), &Labels::default()).unwrap();
        let c = card(row);
        assert_eq!((c.x, c.y, c.w, c.h), (row.x, row.y, 5.625, row.h), "3:2 at the row's start");
        let inside: Vec<&Node> = g.nodes().iter().filter(|n| n.kind == Kind::Step || n.kind == Kind::Call).collect();
        assert!(inside.iter().filter(|n| n.kind == Kind::Step).count() >= 2 && inside.iter().any(|n| n.kind == Kind::Call), "the run's steps and calls placed");
        for n in &inside {
            assert!(c.holds(&n.rect), "{} lies in the card: {:?}", n.id, n.rect);
        }
        for n in inside.iter().filter(|n| n.kind == Kind::Step) {
            let shape = n.rect.w / n.rect.h;
            assert!((1.0..1.5).contains(&shape), "{} about as tall as wide: {shape}", n.id);
        }
        assert_eq!(card(Rect::new(0.0, 0.0, 4.0, 6.0)), Rect::new(0.0, 0.0, 4.0, 6.0), "a post no wider than its card is its own");
    }

    /// A post's runs are asked of coordd once each, and only while the pipe is up. A run held while it was going is
    /// asked once more after it ends, and one lost with the pipe is asked again. What comes is held where the Docket's
    /// fetched runs are; what has not come is said in the post's band.
    #[test]
    #[cfg(not(windows))]
    fn a_posts_runs_are_asked_once_each_and_never_while_the_pipe_is_down() {
        let mut s = crate::picture::docket::tests::wire_app();
        let h = docket_machine::HIDDEN;
        let pid = s.projects[0].id.clone();
        let out = std::env::temp_dir().join(format!("yaaihh-map-deep-{}.jsonl", std::process::id()));
        let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        let asked = |s: &App, id: &str| s.projects[0].pending.values().filter(|w| matches!(w, crate::Pending::Run { id: x } if x == id)).count();
        // post 2's run is not held (the fixture fetched run 1 only)
        s.projects[0].down = Some("connecting".into());
        want(&mut s, 0, 7, 2);
        assert!(s.projects[0].pending.is_empty(), "nothing asked while the pipe is down");
        let v = s.projects[0].live.tickets[&7].clone();
        assert_eq!(note(&s, 0, &v, 2), Some(format!("{} disconnected; steps load on reconnect.", s.projects[0].name)));
        s.projects[0].down = None;
        want(&mut s, 0, 7, 2);
        want(&mut s, 0, 7, 2);
        want(&mut s, 0, 7, 1);
        assert_eq!((asked(&s, &run(2)), s.projects[0].pending.len()), (1, 1), "post 2's run once; post 1's is held and ended");
        assert_eq!(note(&s, 0, &v, 2).as_deref(), Some("Reading its steps…"));
        assert_eq!(s.notice, None, "the Map's reading says nothing on the bar");
        // the pipe ends before the answer: asked again once it is back
        s.projects[0].pending.clear();
        lost(&mut s.deep, 0, &run(2));
        want(&mut s, 0, 7, 2);
        assert_eq!(asked(&s, &run(2)), 1, "asked again after the pipe's end");
        // it did not come: said in its band; then it comes, held where the Docket's runs are
        s.projects[0].pending.clear();
        came(&mut s, 0, run(2), false, "no such ticket\n".into());
        assert_eq!(note(&s, 0, &v, 2).as_deref(), Some("Its steps did not come: no such ticket"));
        let mut r = s.tickets.find.fetched[&pid][&(h + 1)].clone();
        (r.id, r.n) = (run(2), h + 2);
        came(&mut s, 0, run(2), true, serde_json::to_string(&r).unwrap());
        assert_eq!((note(&s, 0, &v, 2), runs(&s, 0, &v, 2).len()), (None, 1));
        // post 7's run, still going when it is held, is asked once more after the thread says it ended
        want(&mut s, 0, 7, 7);
        assert_eq!(asked(&s, &run(7)), 1);
        s.projects[0].pending.clear();
        let (mut r, ended) = (s.tickets.find.fetched[&pid][&(h + 1)].clone(), crate::picture::docket::tests::run_at(20));
        (r.id, r.n, r.closed_at) = (run(7), h + 7, None);
        came(&mut s, 0, run(7), true, serde_json::to_string(&r).unwrap());
        want(&mut s, 0, 7, 7);
        assert!(s.projects[0].pending.is_empty(), "held, and going as the thread says");
        s.projects[0].live.tickets.get_mut(&7).unwrap().ran.iter_mut().find(|x| x.id == run(7)).unwrap().closed_at = Some(ended);
        want(&mut s, 0, 7, 7);
        want(&mut s, 0, 7, 7);
        assert_eq!(asked(&s, &run(7)), 1, "once more, once it ended");
        s.projects[0].link = None;
        let _ = child.wait();
        let sent: Vec<serde_json::Value> = std::fs::read_to_string(&out).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        let ts: Vec<&str> = sent.iter().filter_map(|v| v["req"]["t"].as_str()).collect();
        assert_eq!(ts, [run(2), run(2), run(7), run(7)], "{sent:?}");
        let _ = std::fs::remove_file(&out);
    }

    /// Node ID of the fixture's graph as project PI's island draws it, PX wide.
    fn island(s: &App, pi: usize, id: &str, px: f32) -> Option<El> {
        let (mut g, post) = graph();
        place(&mut g, post, &held(s), &Labels::default()).unwrap();
        let i = g.find_in(0, id).unwrap_or_else(|| panic!("{id} placed"));
        inside(s, pi, &g, i, px, &Kit::neutral(false))
    }

    /// Node ID of the fixture's graph drawn by itself, PX wide, on a page.
    fn alone(s: &App, id: &str, px: f32) -> El {
        rune_kit::root(col().key("page").w_full().children(island(s, 0, id, px)), Vec::new())
    }

    fn step_near(s: &App, _: &mut rune::Cx<App>) -> El {
        alone(s, &step_id(&format!("{}.1", run(1))), STEP_PX)
    }

    fn call_near(s: &App, _: &mut rune::Cx<App>) -> El {
        alone(s, &format!("{}.s2", run(1)), CALL_PX)
    }

    #[cfg(not(windows))]
    fn prompt_near(s: &App, _: &mut rune::Cx<App>) -> El {
        alone(s, &step_id(&format!("{}.0", run(1))), PROMPT_PX)
    }

    /// The same step of both islands' runs (their ids repeat), each in its own island's box.
    #[cfg(not(windows))]
    fn prompts_on_both(s: &App, _: &mut rune::Cx<App>) -> El {
        let id = step_id(&format!("{}.0", run(1)));
        let at = |pi: usize| col().key(format!("island-{pi}")).w_full().children(island(s, pi, &id, PROMPT_PX));
        rune_kit::root(col().key("page").w_full().child(at(0)).child(at(1)), Vec::new())
    }

    #[cfg(not(windows))]
    fn step_on_cav(s: &App, _: &mut rune::Cx<App>) -> El {
        rune_kit::root(col().key("page").w_full().children(island(s, 1, &step_id(&format!("{}.1", run(1))), STEP_PX)), Vec::new())
    }

    #[cfg(not(windows))]
    fn call_on_cav(s: &App, _: &mut rune::Cx<App>) -> El {
        rune_kit::root(col().key("page").w_full().children(island(s, 1, &format!("{}.s2", run(1)), CALL_PX)), Vec::new())
    }

    /// S with a second project, cav, beside the fixture's research: its runs are research's under the same ids (each
    /// coordd numbers its hidden tickets from `HIDDEN`), and its own copy of agent-base's question.call words `answer`'s
    /// template "A question for cav:" where agent-base has "A question for you:". DIR is `live_folders`'s.
    #[cfg(not(windows))]
    fn with_cav(s: &mut App, dir: &std::path::Path) {
        let own = dir.join("cav");
        for sub in ["kinds", "calls"] {
            std::fs::create_dir_all(own.join(sub)).unwrap();
        }
        let call = std::fs::read_to_string(dir.join("base/calls/question.call")).unwrap();
        std::fs::write(own.join("calls/question.call"), call.replace("A question for you:", "A question for cav:")).unwrap();
        let mut cav = crate::Proj::new(&crate::projects::ProjectConfig { name: "cav".into(), id: "cav".into(), boxed: false }, Vec::new());
        cav.live.places = Some(coord_types::Places { coord: "/c2".into(), definition: Some(own.display().to_string()), ..Default::default() });
        cav.protocol = coord_types::PROTOCOL;
        let runs = s.tickets.find.fetched[&s.projects[0].id].clone();
        s.tickets.find.fetched.insert("cav".into(), runs);
        s.projects.push(cav);
    }

    fn run(i: u64) -> String {
        crate::picture::docket::tests::run_id(i)
    }

    fn keys(rt: &rune::runtime::Runtime<App>) -> Vec<String> {
        rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.key).collect()
    }

    fn texts(rt: &rune::runtime::Runtime<App>) -> Vec<String> {
        rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect()
    }

    /// Levels 6 and 7 (p715) on the fixture's run.
    /// - Far out, a step and a call draw nothing of their own, so the canvas draws their labels.
    /// - A step near enough draws its stage as its ticket's panel does.
    /// - A call near enough draws its lines on the wire, said to be read until they come.
    #[test]
    fn a_step_draws_its_stage_and_a_call_its_wire_once_near_enough() {
        use crate::picture::docket::tests::{frames, wire_app};
        let s = wire_app();
        let (mut g, post) = graph();
        place(&mut g, post, &held(&s), &Labels::default()).unwrap();
        let k = Kit::neutral(false);
        let (next, call) = (g.find_in(0, &step_id(&format!("{}.1", run(1)))).unwrap(), g.find_in(0, &format!("{}.s2", run(1))).unwrap());
        assert!(inside(&s, 0, &g, next, STEP_PX - 1.0, &k).is_none() && inside(&s, 0, &g, call, CALL_PX - 1.0, &k).is_none(), "far: labels only");
        assert!(inside(&s, 0, &g, post, 10_000.0, &k).is_none(), "a post is drawn by level 4, not here");
        let mut rt = rune::runtime::Runtime::new(wire_app(), step_near, (600.0, 1600.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        let (ks, ts) = (keys(&rt), texts(&rt));
        assert!(ks.contains(&format!("map-step-{}.1", run(1))) && ks.contains(&format!("stage-panel-{}.1", run(1))), "{ks:?}");
        assert!(ts.iter().any(|x| x == "round 1 · step 2 ›") && ts.iter().any(|x| x == "round 2 · step 3 ›"), "its rounds as its panel says them: {ts:?}");
        // no link to its coordd: said, as nothing is read then
        let mut rt = rune::runtime::Runtime::new(wire_app(), call_near, (600.0, 1600.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        assert!(keys(&rt).contains(&format!("map-call-{}.s2", run(1))));
        let name = rt.state.projects[0].name.clone();
        assert!(texts(&rt).contains(&format!("{name} disconnected; wire loads on reconnect.")), "{:?}", texts(&rt));
        // its lines held: drawn
        let mut s = wire_app();
        s.records.got.insert((0, format!("{}.s2", run(1))), Ok(Vec::new()));
        let mut rt = rune::runtime::Runtime::new(s, call_near, (600.0, 1600.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        assert!(!texts(&rt).iter().any(|x| x.starts_with("Its wire is read once")), "held lines are drawn, the pipe down or not: {:?}", texts(&rt));
    }

    /// consult-3 N-1479 p27, 3: a call on another project's island reads its wire from that island's coordd, once, and
    /// nothing while its pipe is down; until its lines come it says so.
    #[test]
    #[cfg(not(windows))]
    fn a_call_on_another_island_reads_its_wire_from_that_islands_coordd() {
        use crate::picture::docket::tests::{frames, live_folders, wire_app};
        let mut s = wire_app();
        let dir = live_folders(&mut s, "map-wire-islands");
        with_cav(&mut s, &dir);
        let (mut g, post) = graph();
        place(&mut g, post, &held(&s), &Labels::default()).unwrap();
        let call = g.find_in(0, &format!("{}.s2", run(1))).unwrap();
        let out = std::env::temp_dir().join(format!("yaaihh-map-wire-{}.jsonl", std::process::id()));
        let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[1].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        s.notice = None;
        // cav's pipe down: nothing asked, and nothing said on the bar
        near(&mut s, 1, &g, call, CALL_PX);
        assert!(s.projects[1].pending.is_empty() && s.notice.is_none(), "nothing asked while cav's pipe is down");
        // up: asked of cav's coordd, once, though research is the current project
        s.projects[1].down = None;
        for _ in 0..3 {
            near(&mut s, 1, &g, call, CALL_PX);
        }
        assert_eq!(s.cur, 0);
        let asked: Vec<&crate::Pending> = s.projects[1].pending.values().collect();
        assert!(matches!(asked[..], [crate::Pending::Record { node, after: None }] if *node == format!("{}.s2", run(1))), "one ask of cav's coordd, for the call's lines: {} asked", asked.len());
        assert!(s.projects[0].pending.is_empty(), "research is asked nothing for cav's call");
        let mut rt = rune::runtime::Runtime::new(s, call_on_cav, (600.0, 1600.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        assert!(texts(&rt).iter().any(|x| x == "Reading the record…"), "cav's lines on their way: {:?}", texts(&rt));
        rt.state.projects[1].link = None;
        let _ = child.wait();
        let sent: Vec<serde_json::Value> = std::fs::read_to_string(&out).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        assert_eq!(sent.len(), 1, "one ask: {sent:?}");
        let _ = std::fs::remove_file(&out);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// paper-research N-1446 p1556: what the Map draws is what the poll asks for, each island's through its own pipe.
    /// cav's step drawn near, research the current project: what the step gave, held by reference in cav's texts, is
    /// "…" and asked of cav's coordd; research's is asked nothing for it.
    #[test]
    #[cfg(not(windows))]
    fn a_step_on_another_island_asks_for_what_it_gave_through_that_islands_pipe() {
        use crate::picture::docket::tests::{frames, live_folders, wire_app};
        fn gave(ns: &mut [docket_machine::RunNode], x: &serde_json::Value) {
            for n in ns {
                n.value = Some(x.clone());
                gave(&mut n.nodes, x);
            }
        }
        let mut s = wire_app();
        let dir = live_folders(&mut s, "map-step-words");
        with_cav(&mut s, &dir);
        let loc = docket_machine::Loc { file: "/made-up/cav/tickets/texts/watcher.jsonl".into(), offset: 2048, len: 96 };
        let x = serde_json::to_value(docket_machine::Text::Loc { loc: loc.clone() }).unwrap();
        for v in s.tickets.find.fetched.get_mut("cav").unwrap().values_mut() {
            gave(&mut v.run, &x);
        }
        let (child, out) = piped(&mut s, 0, "step-words-research");
        let (cav_child, cav_out) = piped(&mut s, 1, "step-words-cav");
        assert_eq!(s.cur, 0, "research is current");
        let mut rt = rune::runtime::Runtime::new(s, step_on_cav, (600.0, 1600.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        let ts = texts(&rt);
        assert!(keys(&rt).contains(&format!("map-step-{}.1", run(1))), "cav's step drawn: {:?}", keys(&rt));
        assert!(ts.iter().any(|t| t == "It gave") && ts.iter().any(|t| t == "…"), "what it gave, not read yet: {ts:?}");
        assert!(crate::texts::poll(&mut rt.state), "the poll asks");
        let key = crate::texts::key(&loc);
        let asked = |s: &App, pi: usize| s.projects[pi].pending.values().filter(|w| matches!(w, crate::Pending::TicketText { key: k } if *k == key)).count();
        assert_eq!((asked(&rt.state, 1), asked(&rt.state, 0)), (1, 0), "asked once, of cav's coordd, though research is current");
        rt.state.projects[0].link = None;
        rt.state.projects[1].link = None;
        let of_it = |v: &serde_json::Value| v["req"]["op"] == "text" && v["req"]["loc"]["offset"] == 2048;
        assert!(!sent(child, &out).iter().any(of_it), "nothing of it sent to research's coordd");
        assert_eq!(sent(cav_child, &cav_out).iter().filter(|v| of_it(v)).count(), 1, "asked of cav's coordd once");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// proto-window N-1479 p41: a press on a line of a call drawn on another island opens that island's record, not the
    /// current project's, whose run of the same id is another. The call is the watcher's `next`, a round of coordd's
    /// own, so its wire is its lines of cav's record (made up here). The press makes cav current and shows the line
    /// whole in cav's Record; cav's lines are held, so nothing is asked again, and research's coordd is asked nothing.
    #[test]
    #[cfg(not(windows))]
    fn a_press_on_a_wire_line_of_a_call_on_another_island_opens_that_islands_record() {
        use crate::picture::docket::tests::{frames, live_folders, press, wire_app};
        use serde_json::json;
        let mut s = wire_app();
        let dir = live_folders(&mut s, "map-wire-press");
        with_cav(&mut s, &dir);
        let (node, t) = (format!("{}.s2", run(1)), docket_machine::HIDDEN + 1);
        let line = |line: u64, v: serde_json::Value| coord_types::RecordLine { line, text: v.to_string() };
        let lines = vec![line(501, json!({"ev": "step", "t": t, "step": 2, "call": "next", "worker": "watcher"})), line(502, json!({"ev": "answer", "t": t, "by": "watcher", "head": {"head": "keep"}}))];
        s.records.got.insert((1, node.clone()), Ok(lines));
        let (child, out) = piped(&mut s, 0, "press-research");
        let (cav_child, cav_out) = piped(&mut s, 1, "press-cav");
        assert_eq!(s.cur, 0, "research is current");
        let mut rt = rune::runtime::Runtime::new(s, call_on_cav, (600.0, 1600.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        assert!(texts(&rt).iter().any(|x| x == "Step 2: next, by watcher"), "cav's lines drawn in the call: {:?}", texts(&rt));
        press(&mut rt, "wire-step-501");
        assert_eq!(rt.state.projects[0].pending.len(), 0, "research's coordd asked nothing for cav's call");
        assert_eq!(rt.state.cur, 1, "cav made current");
        assert!(matches!(&rt.state.sel, crate::Sel::Lens(l) if l.depth == Depth::Record && l.at.as_deref() == Some(node.as_str())), "its Record");
        assert!(rt.state.records.whole.contains(&(1, 501)) && !rt.state.records.whole.contains(&(0, 501)), "the line whole in cav's record");
        assert!(rt.state.projects[1].pending.is_empty(), "nothing asked of cav: its lines are held");
        rt.state.projects[0].link = None;
        rt.state.projects[1].link = None;
        assert!(sent(child, &out).is_empty(), "nothing sent to research's coordd");
        assert!(sent(cav_child, &cav_out).is_empty(), "cav's held lines not asked again");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The fixture's call near enough, drawn after the comment box's place is decided as the window decides it each
    /// pass before its surfaces (lib.rs `view`, ask.rs `decide`).
    fn call_near_decided(s: &App, cx: &mut rune::Cx<App>) -> El {
        crate::ask::decide(s, cx);
        call_near(s, cx)
    }

    /// I-1476 on the Map (docket-edge2, on lead-64's Map round): a comment being written on a call's lines in a lens's
    /// Wire pane is that pane's. The same call drawn on the Map draws the same lines (lens.rs `lines`), and lights none
    /// of his passage in them and leaves the box no room, so no box stands on the Map; his words are kept, for the
    /// lens's pane, where it stands again.
    #[test]
    fn a_comment_written_on_a_calls_lines_in_the_lens_is_not_drawn_on_the_map() {
        use crate::ask::anchor_tests::{drawn, lit};
        use crate::picture::docket::tests::{frames, wire_app};
        use serde_json::json;
        let mut s = wire_app();
        let (node, t) = (format!("{}.s2", run(1)), docket_machine::HIDDEN + 1);
        let line = |line: u64, v: serde_json::Value| coord_types::RecordLine { line, text: v.to_string() };
        let args = json!({"to": ["watcher"], "why": "the round's next"});
        let lines = vec![line(501, json!({"ev": "step", "t": t, "step": 2, "call": "next", "worker": "watcher"})), line(502, json!({"ev": "step_args", "t": t, "step": 2, "args": args}))];
        s.records.got.insert((0, node.clone()), Ok(lines));
        // the comment as the lens's Wire pane makes it (lens.rs `comment`) on "watcher" in what the call was made from
        let (key, body, words) = (format!("wire-{node}-args-502-body"), serde_json::to_string_pretty(&args).unwrap(), "\"watcher\"");
        let a = body.find(words).unwrap();
        let picked = crate::ask::Picked { target: "thread:N-7".into(), source: crate::lens::comment_source(&node, Depth::Wire), label: format!("the wire of {node}"),
            quote: words.into(), at: Vec::new(), pi: 0, anchor: None, block: None, pane: None, texts: vec![(key.clone(), a..a + words.len())] };
        crate::ask::write(&mut s, picked, "why the watcher?");
        let mut rt = rune::runtime::Runtime::new(s, call_near_decided, (600.0, 1600.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        let ks = keys(&rt);
        assert!(ks.contains(&key) && texts(&rt).iter().any(|x| x == "Step 2: next, by watcher"), "the call's lines drawn on the Map: {ks:?}");
        assert!(lit(&rt).is_empty(), "his passage lit nowhere on the Map: {:?}", lit(&rt));
        assert!(drawn(&rt, crate::ask::ROOM).is_none(), "no room for the box on the Map: {:?}", drawn(&rt, crate::ask::ROOM));
        assert!(rt.state.notes.spot.get().is_none(), "no place for the box on the Map: {:?}", rt.state.notes.spot.get());
        assert!(texts(&rt).contains(&body), "what the call was made from whole, not split for a room: {:?}", texts(&rt));
        assert!(rt.state.notes.picked.as_ref().is_some_and(|p| p.texts == vec![(key.clone(), a..a + words.len())]), "his comment kept for the lens");
        // and nothing on the Map's card selects, as before the half (proto-window N-1477 p63): a drag on it moves the canvas
        let at = key.clone();
        rt.update(move |_, tx| tx.select_text(at, a..a + words.len()));
        frames(&mut rt);
        assert!(rt.selection().is_none(), "the call's lines on the Map not selectable: {:?}", rt.selection().map(|x| x.text().to_string()));
    }

    /// Path 5's last steps (p715: "The template is there … Click Edit: it shows what it reaches … Save"), on the
    /// fixture's run read with agent-base's calls. pr's `answer` step, near enough, draws its call's prompt. Its Edit
    /// opens the call's lines in place in that step: the editor a ticket's step panel opens, which asks what it reaches
    /// as he types, Save beside it.
    #[test]
    #[cfg(not(windows))]
    fn a_steps_prompt_is_drawn_in_it_and_its_edit_opens_there() {
        use crate::picture::docket::tests::{frames, live_folders, press, wire_app};
        let mut s = wire_app();
        let dir = live_folders(&mut s, "map-prompt");
        let id = step_id(&format!("{}.0", run(1)));
        let mut rt = rune::runtime::Runtime::new(s, prompt_near, (900.0, 2400.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        assert!(texts(&rt).iter().any(|x| x.contains("A question for you:")), "answer's template: {:?}", texts(&rt));
        press(&mut rt, &format!("map-edit-{}.0", run(1)));
        assert_eq!(rt.state.tickets.edits.call_edit.as_ref().map(|e| e.call.as_str()), Some("answer"));
        assert_eq!(rt.state.deep.editing, Some((0, id.clone())), "edited in this step");
        let ks = keys(&rt);
        assert!(ks.contains(&"call-editor".to_string()) && crate::edit::reach_of(&rt.state).is_some(), "its lines in place, and what Save would write is what its reach asks of: {ks:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// pr's `attend`, the first part of the first brain's pass standing at N-7's post 1 ([`with_passes`]), drawn by
    /// itself at [`PROMPT_PX`] on a page.
    #[cfg(not(windows))]
    fn part_near(s: &App, _: &mut rune::Cx<App>) -> El {
        let (mut g, post) = graph();
        place_passes(&mut g, post, &passes(s, 0, &s.projects[0].live.tickets[&7], 1), &Labels::default()).unwrap();
        let mark = g.children(post).iter().copied().find(|i| g.node(*i).kind == Kind::Brain).expect("a mark");
        let part = g.children(mark).first().copied().expect("its first part");
        rune_kit::root(col().key("page").w_full().children(inside(s, 0, &g, part, PROMPT_PX, &Kit::neutral(false))), Vec::new())
    }

    /// A brain's part draws the prompt coordd sends for it (proto-lead N-1446 p1112): its `attend` has no template of its
    /// own, and the Kinds answer says coordd runs `attention` for it (`CallForm::runs`, coordd's clerk.rs `RUNS`): the
    /// part draws that call's template, read from the project's calls, under the words the Docket's call panel says it
    /// with, and its Edit opens `attention`'s lines. coordd runs it only when the watcher takes the step, so the same
    /// part taken by an agent draws neither, as the call panel draws neither for it (consult-3 N-1446 p1160). Not in the
    /// Windows test exe, as the prompt tests above.
    #[test]
    #[cfg(not(windows))]
    fn a_brain_parts_prompt_is_the_template_of_the_call_coordd_runs_for_it() {
        use crate::picture::docket::tests::{frames, live_folders, press, wire_app};
        use serde_json::json;
        // the passes, with the first's `attend` taken by BY
        let app = |name: &str, by: Option<&str>| {
            let mut s = wire_app();
            let dir = live_folders(&mut s, name);
            std::fs::write(dir.join("base/calls/brain.call"), include_str!("../../../../agent-base/calls/brain.call")).unwrap();
            let ids = with_passes(&mut s);
            if let Some(by) = by {
                fn take(ns: &mut [docket_machine::RunNode], by: &str) -> bool {
                    ns.iter_mut().any(|n| if n.what == "step" && n.text.contains("attend(") { n.who = Some(by.into()); true } else { take(&mut n.nodes, by) })
                }
                let first = s.projects[0].live.tickets.values_mut().find(|v| v.id == ids[0]).unwrap();
                assert!(take(&mut first.run, by), "its attend");
            }
            // the Kinds answer's `attend`, as coordd says it (`kinds::said_call`), made up but for its name and what it runs
            let attend = json!({"name": "attend", "file": "agent-base/calls/brain.call", "line": 43, "sig": "call attend(agent: Agent, posts: [Text])", "attrs": [["task", "attend"]],
                "template": null, "doc": "", "task": "attend", "runs": {"name": "attention", "template": "You are one part of {{ agent }}'s mind."}});
            s.tickets.kinds = Some(Ok(vec![serde_json::from_value(json!({"kind": "brain", "form": [], "text": "", "doc": "", "bindings": [], "calls": [attend]})).unwrap()]));
            let mut rt = rune::runtime::Runtime::new(s, part_near, (900.0, 2400.0), rune::Env::new(false, 1.0));
            frames(&mut rt);
            (rt, ids, dir)
        };
        let (mut rt, ids, dir) = app("map-brain-prompt", None);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Runs attention, with its template:"), "what runs for it: {t:?}");
        assert!(t.iter().any(|x| x.contains("You are its owner model: a close colleague of his who reads him well.")), "attention's template, from the project's calls: {t:?}");
        let edit = keys(&rt).into_iter().find(|k| k.starts_with(&format!("map-edit-{}.", ids[0]))).unwrap_or_else(|| panic!("its Edit: {:?}", keys(&rt)));
        press(&mut rt, &edit);
        assert_eq!(rt.state.tickets.edits.call_edit.as_ref().map(|e| e.call.as_str()), Some("attention"), "its Edit edits the call coordd runs");
        let _ = std::fs::remove_dir_all(&dir);
        // an agent's step on `attend`: coordd runs nothing for it
        let (rt, ids, dir) = app("map-brain-prompt-agent", Some("pr"));
        let t = texts(&rt);
        assert!(keys(&rt).iter().any(|k| k.starts_with("stage-panel-")), "the part is drawn: {:?}", keys(&rt));
        assert!(!t.iter().any(|x| x.starts_with("coordd runs") || x.contains("You are its owner model")), "an agent's step: no words of coordd's call, nor its template: {t:?}");
        assert!(!keys(&rt).into_iter().any(|k| k.starts_with(&format!("map-edit-{}.", ids[0]))), "and no Edit");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A sink for what S's project PI sends its coordd, read back as JSON lines by [`sent`].
    #[cfg(not(windows))]
    pub(crate) fn piped(s: &mut App, pi: usize, name: &str) -> (std::process::Child, std::path::PathBuf) {
        let out = std::env::temp_dir().join(format!("yaaihh-map-{name}-{}.jsonl", std::process::id()));
        let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[pi].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        s.projects[pi].down = None;
        (child, out)
    }

    /// What was sent down the pipe of [`piped`], once its link is dropped.
    #[cfg(not(windows))]
    pub(crate) fn sent(mut child: std::process::Child, out: &std::path::Path) -> Vec<serde_json::Value> {
        let _ = child.wait();
        let v = std::fs::read_to_string(out).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        let _ = std::fs::remove_file(out);
        v
    }

    /// consult-3 N-1479 p33: the canvas calls `near` on every frame a call is near, and its wire is asked once for each
    /// pipe. A refusal is drawn and not asked again on the next frames; once the pipe has ended and is back it is asked
    /// once more. pr's answer is an agent's round: its wire is its turn's requests, read from the proxy's log.
    #[test]
    #[cfg(not(windows))]
    fn a_calls_wire_is_asked_once_for_each_pipe_and_a_refusal_not_again_each_frame() {
        let mut s = crate::picture::docket::tests::wire_app();
        let (mut g, post) = graph();
        place(&mut g, post, &held(&s), &Labels::default()).unwrap();
        let call = g.find_in(0, &format!("{}.s1", run(1))).unwrap();
        let (child, out) = piped(&mut s, 0, "refused");
        let raws = |s: &App| s.projects[0].pending.values().filter(|w| matches!(w, crate::Pending::Raw { .. })).count();
        for _ in 0..5 {
            near(&mut s, 0, &g, call, CALL_PX);
        }
        assert_eq!(raws(&s), 1, "asked once over five frames");
        let id = s.projects[0].pending.keys().next().copied().unwrap();
        let Some(crate::Pending::Raw { agent, older, page }) = s.projects[0].pending.remove(&id) else { panic!("the turn's requests asked") };
        crate::raw::replied(&mut s, 0, agent, older, page, false, "no proxy log here\n".into());
        for _ in 0..5 {
            near(&mut s, 0, &g, call, CALL_PX);
        }
        assert_eq!(raws(&s), 0, "a refusal is not asked again on the next frames");
        // the pipe ends and is back
        ended(&mut s.deep, 0);
        for _ in 0..3 {
            near(&mut s, 0, &g, call, CALL_PX);
        }
        assert_eq!(raws(&s), 1, "asked once more on the pipe that came back");
        s.projects[0].link = None;
        assert_eq!(sent(child, &out).len(), 2, "two asks in all");
    }

    /// The reads of the turns asked of project 0's coordd, by the name each log is held under (its agent first).
    #[cfg(not(windows))]
    fn turns_asked(s: &App) -> Vec<String> {
        s.projects[0].pending.values().filter_map(|w| if let crate::Pending::Raw { agent, .. } = w { Some(agent.clone()) } else { None }).collect()
    }

    /// consult-3 N-1479 p42: a call's wire is asked once for what is read, an agent's round being the turn of the
    /// agent it is with now. pr's round read and refused, then given again to w2 (passed up: its `took`), is w2's turn,
    /// asked on the same pipe at once, and once.
    #[test]
    #[cfg(not(windows))]
    fn a_call_passed_to_another_agent_asks_for_that_agents_turn_once() {
        let mut s = crate::picture::docket::tests::wire_app();
        let (mut g, post) = graph();
        place(&mut g, post, &held(&s), &Labels::default()).unwrap();
        let call = g.find_in(0, &format!("{}.s1", run(1))).unwrap();
        let (child, out) = piped(&mut s, 0, "passed");
        for _ in 0..3 {
            near(&mut s, 0, &g, call, CALL_PX);
        }
        let asked = turns_asked(&s);
        assert!(asked.len() == 1 && asked[0].starts_with("pr\u{1f}"), "pr's turn asked once: {asked:?}");
        let id = s.projects[0].pending.keys().next().copied().unwrap();
        let Some(crate::Pending::Raw { agent, older, page }) = s.projects[0].pending.remove(&id) else { panic!("pr's turn asked") };
        crate::raw::replied(&mut s, 0, agent, older, page, false, "no proxy log here\n".into());
        for _ in 0..3 {
            near(&mut s, 0, &g, call, CALL_PX);
        }
        assert!(turns_asked(&s).is_empty(), "pr's refusal not asked again");
        // passed up to w2, on the same pipe
        let pid = s.projects[0].id.clone();
        let v = s.tickets.find.fetched.get_mut(&pid).unwrap().values_mut().find(|v| v.id == run(1)).unwrap();
        v.run[0].rounds[0].took = Some(serde_json::from_value(serde_json::json!({"to": "w2", "at": "2026-11-20T08:10:30Z"})).unwrap());
        for _ in 0..3 {
            near(&mut s, 0, &g, call, CALL_PX);
        }
        let asked = turns_asked(&s);
        assert!(asked.len() == 1 && asked[0].starts_with("w2\u{1f}"), "w2's turn asked once: {asked:?}");
        s.projects[0].link = None;
        assert_eq!(sent(child, &out).len(), 2, "two asks in all: pr's turn, then w2's");
    }

    /// consult-3 N-1479 p42, small: once the pipe has ended and is back, a call is asked once more, whatever the pipe
    /// before left. The watcher's `next` (coordd's own: its lines of the record) was refused, and its refusal is asked
    /// again; pr's turn was on its way when the pipe ended (`Pending::Raw` gone with it), and is no longer taken to be.
    /// The pipe ends as the app takes it (`Incoming::Down`).
    #[test]
    #[cfg(not(windows))]
    fn after_the_pipe_ends_a_calls_refusal_and_a_read_it_lost_are_asked_once_more() {
        let mut s = crate::picture::docket::tests::wire_app();
        let (mut g, post) = graph();
        place(&mut g, post, &held(&s), &Labels::default()).unwrap();
        let (own, turn) = (g.find_in(0, &format!("{}.s2", run(1))).unwrap(), g.find_in(0, &format!("{}.s1", run(1))).unwrap());
        let (child, out) = piped(&mut s, 0, "ended");
        let records = |s: &App| s.projects[0].pending.values().filter(|w| matches!(w, crate::Pending::Record { .. })).count();
        let frames = |s: &mut App| {
            for _ in 0..3 {
                near(s, 0, &g, own, CALL_PX);
                near(s, 0, &g, turn, CALL_PX);
            }
        };
        frames(&mut s);
        assert_eq!((records(&s), turns_asked(&s).len()), (1, 1), "the watcher's lines and pr's turn, each asked once");
        let id = s.projects[0].pending.iter().find(|(_, w)| matches!(w, crate::Pending::Record { .. })).map(|(id, _)| *id).unwrap();
        s.projects[0].pending.remove(&id);
        crate::lens::came(&mut s, 0, format!("{}.s2", run(1)), None, false, "a made-up refusal".into());
        frames(&mut s);
        assert_eq!((records(&s), turns_asked(&s).len()), (0, 1), "the refusal drawn and not asked again; pr's turn still on its way");
        s.take(0, crate::link::Incoming::Down("the pipe ended".into()));
        assert!(s.projects[0].pending.is_empty());
        // back
        s.projects[0].down = None;
        s.projects[0].protocol = coord_types::PROTOCOL;
        frames(&mut s);
        assert_eq!((records(&s), turns_asked(&s).len()), (1, 1), "each asked once more on the pipe that came back");
        s.projects[0].link = None;
        assert_eq!(sent(child, &out).len(), 4, "four asks in all");
    }

    /// consult-3 N-1479 p33, small: in his window (Windows) a project's files are served only when asked, and only the
    /// current project's are held. A step drawing its prompt on the current island asks for them once when no edit
    /// has, and its prompt is drawn from them with its Edit; another island's step says its prompt shows with that
    /// project open, and asks nothing.
    #[test]
    #[cfg(not(windows))]
    fn on_windows_a_steps_prompt_is_read_from_the_files_served_and_another_islands_says_where_it_shows() {
        use crate::picture::docket::tests::{frames, live_folders, texts_under, wire_app};
        let mut s = wire_app();
        let dir = live_folders(&mut s, "map-windows");
        with_cav(&mut s, &dir);
        let (mut g, post) = graph();
        place(&mut g, post, &held(&s), &Labels::default()).unwrap();
        let step = g.find_in(0, &step_id(&format!("{}.0", run(1)))).unwrap();
        let call = std::fs::read_to_string(dir.join("base/calls/question.call")).unwrap();
        let (child, out) = piped(&mut s, 0, "windows");
        let (cav_child, cav_out) = piped(&mut s, 1, "windows-cav");
        let files = |s: &App, pi: usize| s.projects[pi].pending.values().filter(|w| matches!(w, crate::Pending::Edit(crate::edit::Asked::Prompts))).count();
        crate::elsewhere::as_on_windows(move || {
            for _ in 0..3 {
                near(&mut s, 0, &g, step, PROMPT_PX);
                near(&mut s, 1, &g, step, PROMPT_PX);
            }
            assert_eq!((files(&s, 0), files(&s, 1)), (1, 0), "research's files asked once; cav, not current, asked nothing");
            let id = s.projects[0].pending.keys().next().copied().unwrap();
            s.projects[0].pending.remove(&id);
            let served = serde_json::json!({"kinds": [], "calls": [{"path": "/base/calls/question.call", "text": call}]}).to_string();
            crate::edit::came(&mut s, 0, crate::edit::Asked::Prompts, true, served);
            assert_eq!(s.tickets.edits.served.as_ref().map(|(id, _)| id.as_str()), Some("research"));
            near(&mut s, 0, &g, step, PROMPT_PX);
            assert_eq!(files(&s, 0), 0, "held: not asked again");
            let mut rt = rune::runtime::Runtime::new(s, prompts_on_both, (900.0, 4800.0), rune::Env::new(false, 1.0));
            frames(&mut rt);
            let (research, cav) = (texts_under(&rt, "island-0"), texts_under(&rt, "island-1"));
            assert!(research.iter().any(|x| x.contains("A question for you:")) && research.iter().any(|x| x == "Edit the prompt"), "{research:?}");
            assert!(cav.contains(&"Its prompt shows with cav open.".to_string()) && !cav.iter().any(|x| x.contains("A question for")), "{cav:?}");
            rt.state.projects[0].link = None;
            rt.state.projects[1].link = None;
        });
        let to_research = sent(child, &out);
        assert_eq!(to_research.iter().filter(|v| v["req"]["op"] == "sources").count(), 1, "{to_research:?}");
        assert!(sent(cav_child, &cav_out).is_empty(), "nothing sent to cav's coordd");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// consult-3 N-1479 p27, 1 and 2: each island's step draws its own project's prompt, kept by project, though both
    /// islands' runs have the same ids and their calls the same name. Only the current project's step has an Edit,
    /// since an edit opens on the current project's files and its Save writes there; another's says where it is
    /// edited, and an edit under way on one island is not drawn in the other's step of the same id.
    #[test]
    #[cfg(not(windows))]
    fn a_steps_prompt_is_its_own_projects_and_edited_only_on_the_island_open() {
        use crate::picture::docket::tests::{frames, live_folders, press, texts_under, wire_app};
        let mut s = wire_app();
        let dir = live_folders(&mut s, "map-islands");
        with_cav(&mut s, &dir);
        let key = format!("{}.0", run(1));
        let mut rt = rune::runtime::Runtime::new(s, prompts_on_both, (900.0, 4800.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        let (research, cav) = (texts_under(&rt, "island-0"), texts_under(&rt, "island-1"));
        assert!(research.iter().any(|x| x.contains("A question for you:")) && !research.iter().any(|x| x.contains("A question for cav:")), "research's own template: {research:?}");
        assert!(cav.iter().any(|x| x.contains("A question for cav:")) && !cav.iter().any(|x| x.contains("A question for you:")), "cav's own template: {cav:?}");
        assert!(cav.contains(&"Its prompt is edited with cav open.".to_string()) && !cav.iter().any(|x| x == "Edit the prompt"), "no Edit on cav's island: {cav:?}");
        assert_eq!(research.iter().filter(|x| *x == "Edit the prompt").count(), 1);
        // the edit opens on research's step alone
        press(&mut rt, &format!("map-edit-{key}"));
        assert_eq!(rt.state.deep.editing, Some((0, step_id(&key))));
        frames(&mut rt);
        assert!(texts_under(&rt, "island-1").iter().any(|x| x.contains("A question for cav:")), "cav's step still draws its template, not research's editor");
        // cav made the current project: its step has the Edit, research's says where it is edited; each its own still
        rt.update(|s: &mut App, _| {
            s.tickets.edits.call_edit = None;
            s.deep.editing = None;
            s.cur = 1;
        });
        frames(&mut rt);
        let (research, cav) = (texts_under(&rt, "island-0"), texts_under(&rt, "island-1"));
        assert!(cav.iter().any(|x| x.contains("A question for cav:")) && cav.iter().any(|x| x == "Edit the prompt"), "{cav:?}");
        assert!(research.iter().any(|x| x.contains("A question for you:")) && research.contains(&"Its prompt is edited with research open.".to_string()), "{research:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Made up: his chat with the brain's passes standing at its post 1, as coordd's machine makes them, from
    /// agent-base's brain.kind and brain.call, on `owner_to` through research's binding (brain.kind's header), written
    /// here for two agents. pr's attend read him off its topic, and its elect picked an act, so the pass told pr its
    /// thought (Said). w2's attend read him on its topic with nothing to say, so the pass ended Quiet with no second
    /// call. A third, pr's again, is at work on its attend. Returns the chat's line in the outline, its children the
    /// passes at post 1 as coordd sends them, and each pass's view, read as the window reads coordd's JSON.
    pub(crate) fn brain_passes() -> (docket_machine::ViewOutline, Vec<TicketView>) {
        use docket_machine::{Defs, Input, Machine, Text, WATCHER, syntax};
        use serde_json::json;
        const KIND: &str = include_str!("../../../../agent-base/kinds/brain.kind");
        const CALLS: &str = include_str!("../../../../agent-base/calls/brain.call");
        // his chat: a line, open while he has not answered it, and research's binding for pr and for w2
        const CHAT: &str = "kind chat\n  form text: Text\n  drawn line\n  pattern\n    r <- step owner reply(text);\n    return ()\n\non owner_to(m) where m.agent == \"pr\" open brain(m.agent, Attend(m.posts))\non owner_to(m) where m.agent == \"w2\" open brain(m.agent, Attend(m.posts))\n";
        let mut named = syntax::Named::new();
        syntax::parse_types("system", syntax::SYSTEM_TYPES, &mut named).unwrap();
        let files = [("agent-base/calls/brain.call", CALLS), ("chat.call", "call reply(text: Text) -> Done\n")].map(|(f, t)| (f.to_string(), t.to_string()));
        let (calls, errors) = syntax::parse_call_files(&files, &mut named);
        assert!(errors.is_empty(), "the calls read: {errors:?}");
        let calls = calls.into_iter().map(|(n, d)| (n, d.sig)).collect();
        let (defs, errors) = Defs::build(&[("agent-base/kinds/brain.kind".to_string(), KIND.to_string()), ("chat.kind".to_string(), CHAT.to_string())], calls, named, 200);
        assert!(errors.is_empty(), "the kinds read: {errors:?}");
        let mut m = Machine::load(defs);
        let mut clock = 0;
        let mut go = |m: &mut Machine, i: Input| {
            clock += 1;
            m.apply(i, format!("2026-11-21T09:{:02}:00Z", 10 + clock)).reply.unwrap_or_else(|e| panic!("{e}"))
        };
        let words = "the stills tonight, then the counter";
        go(&mut m, Input::Open { kind: "chat".into(), under: None, opener: "owner".into(), form: json!({"text": words}), text: Some(Text::New(words.into())), names: vec![], told: vec![] });
        let chat = 200;
        let his = format!("{}.p1", m.id(chat));
        let raised = |agent: &str| Input::Event { name: syntax::OWNER_TO.into(), value: json!({"agent": agent, "posts": [his], "tied_at": his}) };
        go(&mut m, raised("pr"));
        go(&mut m, raised("w2"));
        go(&mut m, raised("pr"));
        let passes: Vec<u64> = m.tickets.values().filter(|x| x.kind == "brain").map(|x| x.t).collect();
        assert_eq!(passes.len(), 3, "a pass for each");
        // each step handed to coordd's job, which gives it (coordd `hand_clerk`, `ticket_given`) and answers it itself
        let given = |m: &Machine, t: u64| {
            let d = m.due(WATCHER).into_iter().find(|d| d.t == t).expect("its step is due to the watcher");
            Input::Given { t, to: WATCHER.into(), delivery: d.id, upto: d.upto, skipped: Default::default(), place: None }
        };
        let answer = |t: u64, head: serde_json::Value| Input::Answer { t, by: WATCHER.into(), head, text: Text::New(String::new()) };
        let act = "post the stills before the counter";
        for t in &passes {
            let g = given(&m, *t);
            go(&mut m, g);
        }
        go(&mut m, answer(passes[0], json!({"head": "Read", "topic": "the counter", "on_topic": false, "signals": [{"kind": "repeat", "post": his, "words": "the stills tonight"}], "proposals": [{"act": act, "rests_on": his}], "earlier": []})));
        let g = given(&m, passes[0]);
        go(&mut m, g);
        let score = |act: &str, rank: i64| json!({"act": act, "newest": 2 - rank.min(2), "rules": 2, "owed": 2 - rank.min(2), "sees": 1, "cost": 2, "rank": rank});
        go(&mut m, answer(passes[0], json!({"head": "Pick", "act": act, "rests_on": his, "against": "the counter first", "thought": "He asked for the stills first: post them, then the counter.", "his": "", "scores": [score(act, 1), score("nothing", 2)]})));
        go(&mut m, answer(passes[1], json!({"head": "Read", "topic": "the stills", "on_topic": true, "signals": [], "proposals": [], "earlier": []})));
        let line = docket_machine::ViewOutline::of(&m, &m.tickets[&chat], true);
        let views = passes
            .iter()
            .map(|t| {
                let mut v = m.view(*t).unwrap();
                v.outline = Some(docket_machine::ViewOutline::of(&m, &m.tickets[t], true));
                serde_json::from_value(serde_json::to_value(v).unwrap()).unwrap()
            })
            .collect();
        (line, views)
    }

    /// `wire_app` with [`brain_passes`] standing at N-7's post 1, beside its handler's run: N-7's line in the outline
    /// is the chat's, its children the passes (made up), and the snapshot holds each pass. The passes' ids.
    pub(crate) fn with_passes(s: &mut App) -> Vec<String> {
        let (line, passes) = brain_passes();
        let p = &mut s.projects[0];
        let n7 = p.live.tickets.get_mut(&7).unwrap();
        n7.outline = Some(docket_machine::ViewOutline { id: n7.id.clone(), ..line });
        let ids = passes.iter().map(|v| v.id.clone()).collect();
        for v in passes {
            assert!(p.live.tickets.insert(v.n, v).is_none(), "a number of its own");
        }
        ids
    }

    /// Level 5's marks (paper-research N-1446 p1108, docket-edge6 p1125): each brain's pass standing at N-7's post 1 is
    /// a mark inside the post's card, at the right of its words, side by side oldest first, each a card's shape and under
    /// a third of its width. A mark is labelled by its agent, and says what its pass came to. Its parts lie inside it as
    /// a run's stages do, `brain:{pass}/…`, and never again in the post's band, where its handler's run's steps are: a
    /// pass is not one of `ran`. A pass that opens later takes the next free slot, and nothing placed moves.
    #[test]
    fn a_posts_brain_passes_are_marks_in_its_card_their_parts_inside_them_and_none_in_its_band() {
        let mut s = crate::picture::docket::tests::wire_app();
        let ids = with_passes(&mut s);
        let run = crate::picture::docket::tests::run_id(1);
        let (mut g, post) = graph();
        let labels = Labels::default();
        let v = s.projects[0].live.tickets[&7].clone();
        let at = passes(&s, 0, &v, 1);
        assert_eq!(at.iter().map(|(_, t)| t.id.clone()).collect::<Vec<_>>(), ids, "every pass at post 1, oldest first, the one at work and the quiet one too");
        assert!(passes(&s, 0, &v, 2).is_empty(), "none at post 2");
        place(&mut g, post, &held(&s), &labels).unwrap();
        place_passes(&mut g, post, &at, &labels).unwrap();
        let kids: Vec<&Node> = g.children(post).iter().map(|i| g.node(*i)).collect();
        let marks: Vec<&Node> = kids.iter().copied().filter(|n| n.kind == Kind::Brain).collect();
        assert_eq!(marks.iter().map(|n| (n.id.clone(), n.label.as_str())).collect::<Vec<_>>(), ids.iter().zip(["pr", "w2", "pr"]).map(|(i, a)| (format!("brain:{i}"), a)).collect::<Vec<_>>());
        assert_eq!(marks.iter().map(|n| brain_node(&n.id)).collect::<Vec<_>>(), ids.iter().map(|i| Some(i.as_str())).collect::<Vec<_>>(), "each reads back its pass");
        // what each came to, as coordd words it on its line; the one at work, its line's state
        let lines = &v.outline.as_ref().unwrap().children;
        assert_eq!((marks[0].note.as_str(), marks[1].note.as_str()), ("Said", "Quiet"));
        assert_eq!((marks[2].note.as_str(), marks[2].state), (lines.iter().find(|c| c.id == ids[2]).unwrap().state.as_str(), State::Working));
        assert_eq!((marks[0].state, marks[1].state), (State::Plain, State::Plain));
        // in the card's top share, right of its words, side by side oldest first, 3:2, under a third of its width
        let c = card(g.node(post).rect);
        let room = marks_room(c);
        assert!(room.x >= c.x + c.w * MARKS_FROM - 1e-9 && room.bottom() <= c.y + c.h * POST_TEXT + 1e-9 && c.holds(&room));
        for m in &marks {
            assert!(room.holds(&m.rect), "{} lies in the marks' room", m.id);
            assert!((m.rect.w / m.rect.h - CARD).abs() < 1e-9 && m.rect.w <= c.w / 3.0, "{}: 3:2, under a third: {:?} of {}", m.id, m.rect, c.w);
            let own_card = card(m.rect);
            assert!((own_card.w - m.rect.w).abs() < 1e-9 && own_card.h == m.rect.h, "a mark is its own card");
        }
        assert!(marks[0].rect.y == marks[1].rect.y && marks[0].rect.x < marks[1].rect.x && marks[2].rect.y > marks[0].rect.y, "side by side, then the next row");
        // the band holds the run's steps alone; each pass's parts lie inside its mark, carrying it
        let steps: Vec<&str> = kids.iter().filter(|n| n.kind == Kind::Step).map(|n| n.id.as_str()).collect();
        assert_eq!(steps, [format!("step:{run}.0"), format!("step:{run}.1")], "the post's band: its handler's run's steps, none of a pass's");
        // each mark's steps, by their words, each id carrying its mark (checked for every node below)
        let parts = |m: &Node| -> Vec<(u32, String)> { g.children(g.find_in(0, &m.id).unwrap()).iter().filter(|i| g.node(**i).kind == Kind::Step).map(|i| (*i, g.node(*i).label.clone())).collect() };
        let words = |m: &Node| -> Vec<String> { parts(m).into_iter().map(|(_, l)| l).collect() };
        let pr = &ids[0];
        assert_eq!(words(marks[0])[..2], ["attend", "elect"], "pr's: attend, then elect: {:?}", words(marks[0]));
        assert_eq!(words(marks[1]), ["attend"], "w2's: attend alone, Quiet with no second call");
        assert_eq!(words(marks[2]), ["attend"], "the one at work: its attend");
        assert_eq!(g.node(parts(marks[2])[0].0).state, State::Working, "at work");
        let attend = parts(marks[0])[0].0;
        for i in 0..g.nodes().len() as u32 {
            let n = g.node(i);
            let Some(key) = stage_of(&n.id).or_else(|| (n.kind == Kind::Call).then(|| own(&n.id))) else { continue };
            let pass = ids.iter().find(|p| run_of(key) == p.as_str());
            if let Some(p) = pass {
                assert!(n.id.starts_with(&format!("brain:{p}/")) && g.path(i).contains(&g.find_in(0, &format!("brain:{p}")).unwrap()), "{} lies under its pass's mark alone", n.id);
            }
            assert!(g.node(n.parent.unwrap()).rect.holds(&n.rect), "{} lies inside its parent", n.id);
        }
        // a call of a part: its round, under its step, read through `own`
        let calls: Vec<&str> = g.children(attend).iter().map(|i| g.node(*i).id.as_str()).collect();
        assert_eq!(calls.len(), 1, "attend: one round");
        assert!(calls[0].starts_with(&format!("brain:{pr}/{pr}.s")) && own(calls[0]).starts_with(&format!("{pr}.s")), "{calls:?}");
        // a fourth pass opened: its mark takes the next slot, nothing placed moves
        let third = marks[2].rect;
        let before: Vec<(String, Rect)> = g.nodes().iter().map(|n| (n.id.clone(), n.rect)).collect();
        let mut later = at.iter().map(|(c, t)| ((*c).clone(), (*t).clone())).collect::<Vec<_>>();
        let (mut c4, mut v4) = later[1].clone();
        (c4.id, v4.id, v4.n) = ("N-209".into(), "N-209".into(), 209);
        v4.form["agent"] = serde_json::json!("w3");
        later.push((c4, v4));
        let refs: Vec<(&docket_machine::ViewOutline, &TicketView)> = later.iter().map(|(c, t)| (c, t)).collect();
        place_passes(&mut g, post, &refs, &labels).unwrap();
        for (nid, r) in &before {
            assert_eq!(g.node(g.find_in(0, nid).unwrap()).rect, *r, "{nid} keeps its place");
        }
        let fourth = g.node(g.find_in(0, "brain:N-209").unwrap());
        assert!(fourth.rect.y == third.y && fourth.rect.x > third.x && fourth.label == "w3", "beside the third: {:?}", fourth.rect);
    }
}
