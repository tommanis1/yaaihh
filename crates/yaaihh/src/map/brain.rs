//! The brain on the Map's cloud. Each agent whose program binds a brain (`AgentView.brain`)
//! has a node of its own in its project's cloud, in the room left of the threads. Beside that node, at any zoom, a
//! satellite says what its brain is doing: at work and at which step ("brain: attend…"), what its last pass came to
//! ("brain: quiet", "brain: intervened", "brain: failed"), or that it is off. The pass it speaks of is a `Kind::Brain`
//! node inside the agent's. A press on the satellite selects that pass, and the inspector opens its page, as a brain's
//! mark on a post does (deep.rs `place_passes`).
//!
//! - **What a pass is** comes from the record alone: the tickets of kind `brain` for the agent (agents.rs
//!   `brain_views`). A pass is at work while its ticket is open, at the step it waits on (its `waits_on` call). An
//!   ended pass intervened when it posted to the agent (agents.rs `spoke`). It failed when it raised or came to
//!   `Failed`, and it was quiet otherwise.
//! - **At a moment of the time bar** (`Cloud::at`), the satellite counts only the passes opened by then, each as it
//!   stood then. A pass that closed later is at work, its step not known. Off is said only when his last press of
//!   the switch came before that moment.
//! - **Places.** The agents' nodes lie in a column in the strip between the project's left edge and its leftmost
//!   thread, from the highest thread's top. cloud.rs leaves that room round every project, so no thread moves
//!   (cloud_place.rs keeps them). Their satellites lie left of them, outside the threads.
//! - **Ready.** A pass's post that would wake the agent halts for his approval where its ticket is slow (coordd's
//!   `BrainView::ready`): then the satellite says "brain: ready (N suggestions)" in the waits-on-you ink, the oldest's
//!   first line beside it, with Approve (its post goes on to the agent) and Reject (it is dropped, in the record), as
//!   a halt card's. Now only: at a past moment it says what the passes came to.

use rune::{Color, El, Tx, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit, Spinner};

use super::{Change, Graph, Kind, Node, NodeEvent, Rect, Refused, State, TIMELESS};
use crate::App;

/// The side of an agent's node in the world, at most: two of the cloud's gaps, so their column lies in the room cloud.rs
/// leaves left of the threads (four gaps), with a gap on each side of it.
const SIDE: f64 = 2.0 * super::cloud::GAP;
/// How many of an agent's newest passes have nodes inside its node: its history's steps (`Node::events`), which ◷
/// unfolds. A pass that comes later, or the one a moment of the time bar speaks of, is added to them.
const HISTORY: usize = 64;
/// The grid of an agent's node that its passes' nodes take, each new one the next slot: room for [`HISTORY`] and the
/// passes that come while the window is open.
const PASSES: (usize, usize) = (32, 32);
/// The satellite's height on screen, and the room between it and its agent's node.
const HIGH: f32 = 22.0;
const OFF: f32 = 6.0;

/// What a pass of the brain is doing, or what it came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Came {
    Working,
    /// It posted to the agent.
    Intervened,
    Quiet,
    Failed,
    /// A failure nothing above it handles: someone is asked whether it runs again.
    Stopped,
}

/// One pass as the satellite reads it, borrowed from the snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct Pass<'a> {
    /// Its ticket's id and number.
    pub id: &'a str,
    pub n: u64,
    /// When it opened (RFC 3339).
    pub opened: &'a str,
    pub came: Came,
    /// The step it waits on while at work (`attend`), when the record says it.
    pub step: Option<&'a str>,
}

/// One agent's brain, as its satellite draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct Satellite<'a> {
    pub agent: &'a str,
    /// Its switch, as at the moment shown.
    pub on: bool,
    /// Its passes opened by the moment shown, oldest first.
    pub passes: Vec<Pass<'a>>,
    /// Its passes' posts halted for his approval, oldest first: now, from a coordd at
    /// [`coord_types::BRAIN_READY_PROTOCOL`] or later; none at a past moment.
    pub ready: &'a [coord_types::BrainSuggestion],
}

impl<'a> Satellite<'a> {
    /// The pass it speaks of: the newest at work, else the newest.
    pub fn current(&self) -> Option<&Pass<'a>> {
        self.passes.iter().rev().find(|p| p.came == Came::Working).or(self.passes.last())
    }
}

/// Pass V as it stood at AT (seconds), none before it opened; AT none: now.
fn pass_of(v: &coord_types::TicketView, at: Option<i64>) -> Option<Pass<'_>> {
    use docket_machine::ViewStatus;
    let secs = docket_machine::machine::secs;
    if let Some(t) = at
        && secs(&v.opened_at).is_none_or(|o| o > t)
    {
        return None;
    }
    let ended = v.closed_at.as_deref().is_some_and(|c| at.is_none_or(|t| secs(c).is_some_and(|c| c <= t)));
    let failed = matches!(&v.status, ViewStatus::Failed { .. }) || matches!(&v.status, ViewStatus::Done { result } if result.get("head").and_then(serde_json::Value::as_str) == Some("Failed"));
    let (came, step) = if matches!(&v.status, ViewStatus::Stopped { .. }) && at.is_none() {
        (Came::Stopped, None)
    } else if !ended {
        // its step only now: the record's view says where it waits now, not where it waited then
        (Came::Working, at.is_none().then(|| v.waits_on.first().map(|w| w.call.as_str())).flatten())
    } else if crate::agents::spoke(v).is_some() {
        (Came::Intervened, None)
    } else if failed {
        (Came::Failed, None)
    } else {
        (Came::Quiet, None)
    };
    Some(Pass { id: &v.id, n: v.n, opened: &v.opened_at, came, step })
}

/// The satellites of project PI: one for each of its agents whose brain is bound, at the time bar's moment. The
/// snapshot's passes are read once for all of them (agents.rs `brains`): this runs on every frame of the cloud.
pub fn satellites(s: &App, pi: usize) -> Vec<Satellite<'_>> {
    let Some(p) = s.projects.get(pi) else { return Vec::new() };
    let at = s.cloud.at;
    let mut out: Vec<Satellite> = p
        .live
        .agents
        .iter()
        .filter_map(|a| {
            let b = a.brain.as_ref()?;
            // off at the moment shown only when he pressed Off before it
            let pressed = b.since.as_deref().and_then(docket_machine::machine::secs);
            let on = b.on || at.is_some_and(|t| pressed.is_none_or(|p| p > t));
            let ready = if at.is_none() && p.protocol >= coord_types::BRAIN_READY_PROTOCOL { b.ready.as_slice() } else { &[] };
            Some(Satellite { agent: &a.name, on, passes: Vec::new(), ready })
        })
        .collect();
    if out.is_empty() {
        return out;
    }
    let at_of: std::collections::HashMap<&str, usize> = out.iter().enumerate().map(|(i, x)| (x.agent, i)).collect();
    for v in crate::agents::brains(s, p) {
        if let Some(i) = v.form.get("agent").and_then(serde_json::Value::as_str).and_then(|a| at_of.get(a))
            && let Some(pass) = pass_of(v, at)
        {
            out[*i].passes.push(pass);
        }
    }
    for x in &mut out {
        x.passes.sort_by_key(|p| p.n);
    }
    out
}

/// What pass P says of itself: "brain: attend…" while it is at work, else what it came to.
fn pass_words(p: &Pass) -> String {
    match p.came {
        Came::Working => format!("brain: {}…", p.step.as_deref().unwrap_or("at work")),
        Came::Intervened => "brain: intervened".to_string(),
        Came::Quiet => "brain: quiet".to_string(),
        Came::Failed => "brain: failed".to_string(),
        Came::Stopped => "brain: stopped".to_string(),
    }
}

/// What satellite S says: "brain: ready (N suggestions)" while posts of its passes wait for his approval, else what the
/// pass it speaks of says while it is at work, else "brain: off" when it is off, else what its last pass came to.
pub fn words(s: &Satellite) -> String {
    match s.current() {
        _ if s.ready.len() == 1 => "brain: ready (1 suggestion)".to_string(),
        _ if !s.ready.is_empty() => format!("brain: ready ({} suggestions)", s.ready.len()),
        None if s.on => "brain: no pass yet".to_string(),
        Some(p) if s.on || p.came == Came::Working => pass_words(p),
        _ => "brain: off".to_string(),
    }
}

/// Whether node I of G is the brain's own here: an agent's node under its project, or a pass inside one. Going in to
/// one keeps the cloud's focus (cloud.rs `went_in`), since coordd has no slice around it.
pub fn ours(g: &Graph, i: u32) -> bool {
    let agent = |i: u32| g.node(i).kind == Kind::Agent && g.node(i).parent.is_some_and(|p| g.node(p).kind == Kind::Project);
    agent(i) || (g.node(i).kind == Kind::Brain && g.node(i).parent.is_some_and(agent))
}

/// Where agent J of the column lies in project ROOT of G, new: the column in the strip between the project's left edge
/// and its leftmost node that is not the brain's, from the highest one's top, each a gap below the one before. None
/// where it would not lie inside the project.
fn agent_rect(g: &Graph, root: u32, j: usize) -> Option<Rect> {
    let pr = g.node(root).rect;
    let others = || g.children(root).iter().filter(|c| !ours(g, **c)).map(|c| g.node(*c).rect);
    let left = others().map(|r| r.x).fold(pr.right(), f64::min);
    let top = others().map(|r| r.y).reduce(f64::min).unwrap_or(pr.y + super::cloud::GAP);
    let side = SIDE.min(left - pr.x - 2.0 * super::cloud::GAP);
    let r = Rect::new(pr.x + (left - pr.x - side) / 2.0, top + j as f64 * (side + super::cloud::GAP), side, side);
    (side > 0.0 && pr.holds(&r)).then_some(r)
}

/// What satellite SAT's agent's node says, its state and its weight (how many passes it has had).
fn said(sat: &Satellite) -> (String, State, f32) {
    let state = if !sat.ready.is_empty() {
        State::WaitsOnHim
    } else if sat.current().is_some_and(|p| p.came == Came::Working) {
        State::Working
    } else {
        State::Idle
    };
    (words(sat), state, sat.passes.len() as f32)
}

/// The passes of SAT that have nodes, oldest first: its newest [`HISTORY`], and the one it speaks of.
fn kept<'s, 'a>(sat: &'s Satellite<'a>) -> Vec<&'s Pass<'a>> {
    let newest = &sat.passes[sat.passes.len().saturating_sub(HISTORY)..];
    let mut out: Vec<&Pass> = sat.current().filter(|c| newest.iter().all(|p| p.n != c.n)).into_iter().chain(newest).collect();
    out.sort_by_key(|p| p.n);
    out
}

/// What pass P's node says, and its state.
fn pass_said(p: &Pass) -> (String, State) {
    (format!("{} · opened {}", pass_words(p), crate::when::at(p.opened)), pass_state(p))
}

/// Pass P's node's state: working while it is at work.
fn pass_state(p: &Pass) -> State {
    if p.came == Came::Working { State::Working } else { State::Idle }
}

/// Place SATS in project ROOT of G: each agent's node, a new one in the column left of the threads ([`agent_rect`]), and
/// inside it a node for each of its passes [`kept`], a new one in the next slot of [`PASSES`], from the time it opened
/// (`Node::born`); each with what it says now. An agent's node's events are its passes' coming, oldest first: its
/// history, as `Graph::date` gives a card's. Whether G grew. [`changes`] says beforehand whether this would change
/// anything.
pub fn place(g: &mut Graph, root: u32, sats: &[Satellite]) -> Result<bool, Refused> {
    let mut grew = false;
    for (j, sat) in sats.iter().enumerate() {
        let id = format!("agent:{}", sat.agent);
        let at = match g.find_in(root, &id) {
            Some(i) if ours(g, i) => i,
            Some(_) => continue,
            None => {
                let Some(r) = agent_rect(g, root, j) else { continue };
                grew = true;
                g.push(Node::new(id, Kind::Agent, sat.agent, Some(root), r))?
            }
        };
        let (says, state, weight) = said(sat);
        g.update(at, |n| (n.note, n.state, n.weight) = (says, state, weight));
        let mut added = false;
        for p in kept(sat) {
            let pid = format!("brain:{}", p.id);
            let i = match g.find_in(root, &pid) {
                Some(i) => i,
                None => {
                    let Some(r) = super::deep::slot(g.node(at).rect, g.children(at).len(), PASSES) else { break };
                    let mut n = Node::new(pid, Kind::Brain, format!("pass {}", p.id), Some(at), r);
                    n.born = docket_machine::machine::secs(p.opened).unwrap_or(TIMELESS);
                    added = true;
                    g.push(n)?
                }
            };
            let (note, state) = pass_said(p);
            g.update(i, |n| (n.note, n.state) = (note, state));
        }
        if added {
            grew = true;
            let mut events: Vec<NodeEvent> = g.children(at).iter().map(|c| NodeEvent { at: g.node(*c).born, what: Change::Child(*c) }).filter(|e| e.at != TIMELESS).collect();
            events.sort_by_key(|e| e.at);
            g.update(at, |n| n.events = events);
        }
    }
    Ok(grew)
}

/// Whether [`place`] would change G: a node it would add, or one whose words or state differ. Asked first, so a frame
/// with nothing new copies no graph and builds no view.
pub fn changes(g: &Graph, root: u32, sats: &[Satellite]) -> bool {
    sats.iter().enumerate().any(|(j, sat)| {
        let at = match g.find_in(root, &format!("agent:{}", sat.agent)) {
            Some(i) if ours(g, i) => i,
            Some(_) => return false,
            None => return agent_rect(g, root, j).is_some(),
        };
        let (says, state, weight) = said(sat);
        let n = g.node(at);
        if (n.note.as_str(), n.state, n.weight) != (says.as_str(), state, weight) {
            return true;
        }
        let room = g.children(at).len() < PASSES.0 * PASSES.1;
        // a pass's words and state, not its time's, which do not change: a time is not worded on every frame
        kept(sat).into_iter().any(|p| match g.find_in(root, &format!("brain:{}", p.id)) {
            Some(i) => g.node(i).state != pass_state(p) || !g.node(i).note.starts_with(&format!("{} · ", pass_words(p))),
            None => room,
        })
    })
}

/// The satellites drawn over the canvas: left of each agent's node of the cloud's project in view (inside the view's
/// left edge), its words at a size that reads at any zoom, a spinner while a pass is at work, and how many passes it has
/// had; its edge lit while its pass is selected. One that would meet a satellite drawn before it goes under that one, a
/// line back to its agent's node (far out, the agents' nodes are points one over the other). A press selects the pass it
/// speaks of (else the agent's node), and the inspector opens it.
pub fn drawn(s: &App, g: &Graph, view: [f32; 2], k: &Kit) -> Vec<El> {
    let Some(root) = super::canvas::project_id(s, s.cloud.of).and_then(|id| g.project(&id)) else { return Vec::new() };
    let faded = super::canvas::faded;
    let sats = satellites(s, s.cloud.of);
    let wide = |t: &str| rune::text::measure(t, &[], k.ty.small, None).0;
    let (mut out, mut taken, mut lines): (Vec<El>, Vec<[f32; 4]>, rune::Shape) = (Vec::new(), Vec::new(), rune::Shape::new());
    for a in g.children(root).iter().copied().filter(|i| ours(g, *i)) {
        let n = g.node(a);
        let Some(r) = s.map.on_screen(g, a) else { continue };
        if r[0] > view[0] || r[1] > view[1] || r[0] + r[2] < 0.0 || r[1] + r[3] < 0.0 {
            continue;
        }
        let working = n.state == State::Working;
        let sat = sats.iter().find(|x| x.agent == n.label);
        let ready = sat.and_then(|x| x.ready.first().map(|f| (f, x.ready.len())));
        // the pass it speaks of, else the agent's node
        let current = sat.and_then(Satellite::current);
        let pass = current.and_then(|p| g.find_in(root, &format!("brain:{}", p.id))).unwrap_or(a);
        let sel = s.map.selected() == Some(pass);
        let chip = (n.weight >= 2.0).then(|| format!("◷ {}", n.weight as u32));
        let named = rune::text::measure(&n.label, &[], k.ty.small_medium, None).0;
        let head = 16.0 + if working { 16.0 } else { 14.0 } + named + 6.0 + wide(&n.note) + chip.as_deref().map_or(0.0, |c| 6.0 + wide(c) + 10.0);
        // a ready suggestion's words and its acts on a second line, the words cut to the room left of the node
        let acts = 16.0 + 2.0 * (6.0 + ACT_WIDE) + 6.0;
        let first = ready.map(|(f, _)| format!("“{}”", fit(&short(&f.words), (r[0] - 2.0 * OFF).max(head) - acts - wide("“”"), wide)));
        let w = first.as_deref().map_or(head, |f| head.max(acts + wide(f)));
        let high = if ready.is_some() { 2.0 * HIGH } else { HIGH };
        // left of its node inside the view, else under the satellite it would meet
        let at = [r[0] - OFF - w, r[1] + r[3] / 2.0];
        let mut box_ = [at[0].max(OFF), at[1] - high / 2.0, w, high];
        while let Some(o) = taken.iter().find(|o| super::lod::overlap(o, &box_)) {
            box_[1] = o[1] + o[3] + 4.0;
        }
        if box_[1] > at[1] - high / 2.0 && box_[0] + w < r[0] {
            lines = lines.line([r[0], at[1]], [box_[0] + w, box_[1] + high / 2.0], 1.0, rune::LineCap::Round, faded(k.c.border_strong, 0.8));
        }
        taken.push(box_);
        let mut top = row().h(HIGH).gap(6.0).items_center().child(dot(working, sat.and_then(|x| ink(x, k)), k));
        top = top.child(text(n.label.clone()).font(k.ty.small_medium).color(k.c.text_base).single_line());
        top = top.child(text(n.note.clone()).font(k.ty.small).color(if working || ready.is_some() { k.c.text_base } else { k.c.text_muted }).single_line());
        // its chip unfolds the agent's passes on a rail, as a card's does (ribbon.rs)
        if let Some(c) = chip {
            top = top.child(super::ribbon::chip_el(&n.id, c, s.cloud.unfolded.contains(&n.id), k));
        }
        let pill = match (ready, first) {
            (Some((f, _)), Some(first)) => {
                let said = text(first).font(k.ty.small).color(k.c.text_muted).single_line();
                col().child(top).child(row().h(HIGH).gap(6.0).items_center().child(said).children(ready_acts(s.cloud.of, n.label.as_str(), f, k))).radius(8.0)
            }
            _ => top.radius(HIGH / 2.0),
        };
        let pill = pill
            .key(format!("map-brain-{}", n.id))
            .px(8.0)
            .fill(faded(k.c.layer_01, 0.96))
            .border(if sel || ready.is_some() { 2.0 } else { 1.0 }, if sel { k.c.border_focus } else if ready.is_some() { k.c.text_accent } else if working { k.c.border_strong } else { k.c.border_muted })
            .cursor(rune::Cursor::Pointer);
        let pill = pill.absolute(box_[0], box_[1]).on_drag(move |s: &mut App, d: &mut rune::Drag, _tx: &mut Tx| {
            if d.phase == rune::Phase::Up && !d.cancelled() {
                super::canvas::select(s, Some(pass));
            }
        });
        out.push(super::canvas::floating(pill));
    }
    if !taken.is_empty() {
        out.insert(0, rune::canvas(lines).absolute(0.0, 0.0).size(view[0], view[1]).key("map-brain-lines"));
    }
    out
}

/// A status dot: turning while WORKING, else filled in INK, else a faint ring. An agent's chip on the Map's cloud too
/// (orbit.rs).
pub fn dot(working: bool, ink: Option<Color>, k: &Kit) -> El {
    match (working, ink) {
        (true, _) => Spinner::new().size(10.0).build(k),
        (false, Some(c)) => rune::el().size(8.0, 8.0).radius(4.0).fill(c),
        (false, None) => rune::el().size(8.0, 8.0).radius(4.0).border(1.0, k.c.text_faint),
    }
}

/// The dot of satellite S while no pass of it is at work: the waits-on-you ink while a suggestion is ready, else what
/// its last pass came to, it spoke, was quiet or failed. None, a ring, when it is off or has had no pass.
fn ink(s: &Satellite, k: &Kit) -> Option<Color> {
    if !s.ready.is_empty() {
        return Some(k.c.text_accent);
    }
    match s.current().filter(|_| s.on)?.came {
        Came::Intervened => Some(k.c.success.icon),
        Came::Quiet => Some(k.c.text_faint),
        Came::Failed | Came::Stopped => Some(k.c.danger.icon),
        Came::Working => None,
    }
}

/// The widest an Approve or Reject on a satellite is drawn, and a suggestion's words beside them.
const ACT_WIDE: f32 = 64.0;
const SAID_CHARS: usize = 40;

/// A suggestion's first line cut to [`SAID_CHARS`].
fn short(t: &str) -> String {
    if t.chars().count() <= SAID_CHARS { t.to_string() } else { format!("{}…", t.chars().take(SAID_CHARS - 1).collect::<String>()) }
}

/// T cut, with "…", to the longest start WIDE measures within ROOM.
fn fit(t: &str, room: f32, wide: impl Fn(&str) -> f32) -> String {
    if wide(t) <= room {
        return t.to_string();
    }
    let cs: Vec<char> = t.trim_end_matches('…').chars().collect();
    let cut = |n: usize| format!("{}…", cs[..n].iter().collect::<String>().trim_end());
    let (mut lo, mut hi) = (0, cs.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if wide(&cut(mid)) <= room { lo = mid } else { hi = mid - 1 }
    }
    cut(lo)
}

/// Approve and Reject of suggestion F of AGENT's brain in project PI: its halted post let go on to AGENT, or dropped,
/// as a halt card's are (slow.rs `approve_in`, `reject_in`).
fn ready_acts(pi: usize, agent: &str, f: &coord_types::BrainSuggestion, k: &Kit) -> [El; 2] {
    let (id, tn, hn) = (f.id.clone(), f.ticket, f.halt);
    let again = id.clone();
    let approve = Button::new("Approve")
        .variant(ButtonVariant::Contrast)
        .on_press(move |s: &mut App| crate::slow::approve_in(s, pi, &id, hn))
        .build(k)
        .key(format!("map-brain-approve-{agent}"))
        .help(format!("{}.p{}, “{}”, goes on to {agent}", f.id, f.post, f.words));
    let reject = Button::new("Reject")
        .variant(ButtonVariant::Ghost)
        .on_press(move |s: &mut App| crate::slow::reject_in(s, pi, &again, tn, hn, None))
        .build(k)
        .key(format!("map-brain-reject-{agent}"))
        .help(format!("{}.p{} dropped: never given to {agent}", f.id, f.post));
    [approve.w(ACT_WIDE), reject.w(ACT_WIDE)]
}

/// The Brain switches in the Map's bar, one group that wraps whole: "Brain", then the switch of each agent of the
/// cloud's project whose brain is bound, named by the agent, its last pass in its help (agents.rs `brain_switch`).
pub fn switches(s: &App, k: &Kit) -> Option<El> {
    let pi = s.cloud.of;
    let p = s.projects.get(pi)?;
    let each: Vec<El> = p
        .live
        .agents
        .iter()
        .filter_map(|a| {
            let (off, last) = crate::agents::brain_words(a.brain.as_ref()?);
            let said = off.map_or_else(|| format!("{last}. "), |o| format!("{o}, {last}. "));
            Some(crate::agents::brain_switch(pi, p, a, &a.name, &said, k)?.key(format!("map-brain-switch-{}", a.name)))
        })
        .collect();
    let label = text("Brain").font(k.ty.small).color(k.c.text_muted);
    (!each.is_empty()).then(|| row().key("map-brain-switches").flex_wrap().gap(10.0).items_center().child(label).children(each))
}

#[cfg(test)]
pub(crate) mod tests;
