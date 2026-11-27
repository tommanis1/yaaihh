//! The project's Docket (0.2 line 26; the owner, N-1104 p496: "a visual representation of docket programs … where are
//! the arrows"; p26: the programs an agent writes for itself are part of it): every program the project runs, on one
//! board, at the top of the Kinds page.
//!
//! - In the middle, each program is a box: its stages in the order they run (stages.rs's walk, which follows the arm
//!   with the work), an arrow from each to the next with the case that leads on written on it; a handler of its kind
//!   is a row of its own, led by its event.
//! - On the left is what starts each: its bindings (his message, a clock with when it fires next, from the snapshot's
//!   clocks), else "an agent opens one". An `open` points into the program it opens.
//! - On the right is whom each step goes to, with an arrow from the step to them. What waits now is drawn on it: a
//!   step's count of tickets at it, an arrow as thick as what waits on it, the one accent for what waits on him.
//!
//! A program's head opens its card under the board (its text and its whole picture, `Pic::kind_into`). A worker opens
//! its part of the Docket (the Agent scale). A count opens its ticket, or several as a list under the board; a start
//! its runs, with the clock's next firing and its last; an `opens` the tickets it stands for, as a `Q-…` link opens a
//! question; an act what it does (paper-research N-1396 p223). Each is a press, none a hover; each ticket in a list
//! opens as its link would.

use super::graph::{Board, P, R, Way, Wire, measure};
use super::*;
use crate::kinds::KindForm;
use std::collections::HashMap;

/// A program as the Docket draws it: its kind as the checker parses the Kinds answer's text, the first line of its
/// `///` doc (above the kind's text, so the answer's own field), whose own program it is, and the bindings that open it.
pub(crate) struct Program {
    pub kd: Kind,
    pub doc: String,
    pub by: Option<String>,
    pub into: Vec<Binding>,
}

/// The Kinds answer's kinds parsed; the names of those that do not parse.
pub(crate) fn programs(kinds: &[KindForm]) -> (Vec<Program>, Vec<String>) {
    let (mut out, mut bad) = (Vec::new(), Vec::new());
    for f in kinds {
        match parse_kind(&f.text) {
            Ok(kd) => {
                let doc = [f.doc.as_str(), kd.doc.text.as_str()].into_iter().flat_map(str::lines).map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string();
                out.push(Program { kd, doc, by: f.by.clone(), into: bindings_of(&f.bindings) })
            }
            Err(_) => bad.push(f.kind.clone()),
        }
    }
    (out, bad)
}

/// The key in `pic_open` of the program whose card stands under the board, and of the person whose arrows are lit.
pub(crate) const SHOWN: &str = "docket-kind:";
pub(crate) const FOCUS: &str = "docket-focus:";

/// The one value of PREFIX in OPEN, if one is set.
pub(crate) fn picked<'o>(open: &'o HashSet<String>, prefix: &str) -> Option<&'o str> {
    open.iter().find_map(|k| k.strip_prefix(prefix))
}

/// The program picked on the Docket, whose card the Kinds page shows first, under it.
pub(crate) fn shown_kind(open: &HashSet<String>) -> Option<&str> {
    picked(open, SHOWN)
}

/// The key in `pic_open` of the scale the Docket is drawn at (`Scale`).
pub(crate) const SCALE: &str = "docket-scale:";

/// The scale the Docket is drawn at (paper-research N-1396 p218; the owner, N-1104 p481: "the thread i am watching or
/// a whole project"): the project's board, one agent's part of it, or the program of the thread open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scale {
    Project,
    Agent,
    Thread,
}

impl Scale {
    /// In the switch's order, as its segments are.
    pub(crate) const ALL: [Scale; 3] = [Scale::Project, Scale::Agent, Scale::Thread];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Scale::Project => "Project",
            Scale::Agent => "Agent",
            Scale::Thread => "Thread",
        }
    }
}

/// The scale picked in OPEN; Project when none is.
pub(crate) fn scale(open: &HashSet<String>) -> Scale {
    picked(open, SCALE).and_then(|w| Scale::ALL.into_iter().find(|x| x.label() == w)).unwrap_or(Scale::Project)
}

/// The Docket drawn at scale X from here on.
pub(crate) fn set_scale(s: &mut App, x: Scale) {
    s.tickets.pic_open.retain(|k| !k.starts_with(SCALE));
    s.tickets.pic_open.insert(format!("{SCALE}{}", x.label()));
}

/// The Docket opened from where he is (p218): at the Thread scale when a thread is open on the Threads page, else at
/// the Project's; from an agent's chat, that agent picked (FOCUS, the board's own pick). On the Docket already, it
/// stays as it is.
pub(crate) fn open(s: &mut App) {
    match s.sel.clone() {
        crate::Sel::Kinds => return,
        crate::Sel::Tickets if s.tickets.shown.is_some() && s.tickets.opening.is_none() => set_scale(s, Scale::Thread),
        crate::Sel::Agent(name) => {
            focus_only(s, &name);
            set_scale(s, Scale::Project);
        }
        _ => set_scale(s, Scale::Project),
    }
    s.sel = crate::Sel::Kinds;
}

/// The Docket at the Agent scale, W picked (p223: a worker pressed, in a picture or on the board).
pub(crate) fn open_agent(s: &mut App, w: &str) {
    focus_only(s, w);
    set_scale(s, Scale::Agent);
    s.sel = crate::Sel::Kinds;
}

/// The key in `pic_open` of the kind whose card a step's call opens in (a step pressed in its picture).
pub(crate) const CALL_IN: &str = "docket-call-in:";

/// The key in `pic_open` of who owes the step whose call is shown, as its picture names it: the watcher, the owner, an
/// agent (what the model line says, kinds.rs `model_words`).
pub(crate) const CALL_BY: &str = "docket-call-by:";

/// Call CALL of a step of program KIND owed by BY (as its picture names it: the watcher, the owner, an agent), shown in
/// KIND's card on the Kinds page (kinds.rs), its card open, with LIT (a `TEXT` key: its line, for a step pressed in the
/// kind's own picture) lit in its text; the same call pressed again puts it away. A step pressed in a kind's picture
/// (picture.rs) or on the board (`chip_press`).
pub(crate) fn open_call(s: &mut App, kind: &str, call: &str, by: &str, lit: Option<&str>) {
    let same = s.tickets.edits.call.as_deref() == Some(call);
    s.tickets.edits.call = if same { None } else { Some(call.to_string()) };
    s.tickets.edits.call_edit = None;
    s.tickets.edits.said = None;
    if let Some(key) = lit.filter(|key| same == s.tickets.pic_open.contains(*key)) {
        super::light(s, key, kind);
    }
    s.kinds_open.insert(kind.to_string());
    s.tickets.pic_open.retain(|k| !k.starts_with(CALL_IN) && !k.starts_with(CALL_BY));
    s.tickets.pic_open.insert(format!("{CALL_IN}{kind}"));
    s.tickets.pic_open.insert(format!("{CALL_BY}{by}"));
}

/// Thread N's own Docket: its program, lit (a thread's head, threads.rs).
pub(crate) fn open_thread(s: &mut App, n: u64) {
    s.tickets.shown = Some(n);
    set_scale(s, Scale::Thread);
    s.sel = crate::Sel::Kinds;
}

/// Run ID of a handler under a thread (`TicketView::ran`, picture.rs `handler_rows`), opened at the Thread scale: the
/// program it ran, its `when`'s body, lit where it went. The snapshot holds no run (they are hidden), so it is fetched
/// as find.rs fetches a ticket it does not hold; a run still going is fetched again at each press (0.3 W3, N-1446
/// p16; the holder's mark on a post opens its run here too, p17).
pub(crate) fn open_run(s: &mut App, id: &str) {
    if let Some(pid) = s.proj().map(|p| p.id.clone()) {
        if let Some(m) = s.tickets.find.fetched.get_mut(&pid) {
            m.retain(|_, v| v.id != id || v.closed_at.is_some());
        }
    }
    match crate::find::number(s, id) {
        Ok(n) => open_thread(s, n),
        Err(e) => s.notice = Some(e),
    }
}

/// The bar's `Docket`, after find's: the Kinds page, where the project's Docket stands over its kinds, at the scale of
/// where he is (`open`). Its name on it: the glyph alone was not found (the owner, N-1104 p554).
pub(crate) fn mark(k: &Kit) -> El {
    rune_kit::Button::new("Docket")
        .variant(rune_kit::ButtonVariant::Ghost)
        .help("The project's programs drawn, and what waits where")
        .on_press(|s: &mut App| open(s))
        .build(k)
        .key("docket-mark")
}

/// One thing on a program's row.
#[derive(Clone, Debug, PartialEq)]
enum Item {
    /// A step owed by WHO as written (`owner`, `role lead`, a name); HOLD: a hold, never given.
    Step { who: String, call: String, hold: bool },
    /// The hole: the ticket rests here, owing nothing, while posts come.
    Hole,
    Open { kind: String },
    /// A decision the policy takes.
    Decide { name: String },
    /// An act of the message runtime's (`deliver`, `keep`, `report`): the engine does it at once, and it owes nothing.
    Act { word: &'static str },
}

/// Block P's row: its stages (stages.rs `collect`, the arm with the work at each `match`), a par's lanes one after the
/// other, each with the case written on the arrow into it.
fn row_of(p: &Prog) -> Vec<(Option<String>, Item)> {
    let mut sts = Vec::new();
    stages::collect(p, None, &At::default(), &None, &mut sts);
    let mut out = Vec::new();
    fn add(sts: &[stages::St], out: &mut Vec<(Option<String>, Item)>) {
        for st in sts {
            let it = match st.s {
                Prog::Step { w, call, hold, .. } => Item::Step { who: docket_machine::print::worker(w), call: call.clone(), hold: *hold },
                Prog::Hole { .. } => Item::Hole,
                Prog::Open { kind, .. } => Item::Open { kind: kind.clone() },
                Prog::Decide { name, .. } => Item::Decide { name: name.clone() },
                Prog::Deliver { from: None, .. } => Item::Act { word: "deliver" },
                Prog::Deliver { .. } | Prog::Fork { .. } => Item::Act { word: "fork" },
                Prog::Keep { .. } => Item::Act { word: "keep" },
                Prog::Report { .. } => Item::Act { word: "report" },
                _ => {
                    for l in &st.lanes {
                        add(&l.stages, out);
                    }
                    continue;
                }
            };
            out.push((st.case.clone(), it));
        }
    }
    add(&sts, &mut out);
    out
}

/// A handler's event in words.
pub(crate) fn event_words(ev: &str) -> String {
    match ev {
        "hole" => "when it rests and something happens".into(),
        "ask" => "when an agent asks".into(),
        "owner_post" => "when your words land".into(),
        "agent_post" => "when an agent posts".into(),
        "outcome" => "when the runner tells".into(),
        // the delivery events, each about one agent (docket-machine syntax.rs)
        docket_machine::syntax::ARRIVED => "when something comes for it while it works".into(),
        docket_machine::syntax::FREE => "when it is free and something waits".into(),
        docket_machine::syntax::STOPPED => "when something comes and it is stopped".into(),
        e => format!("when {}", e.replace('_', " ")),
    }
}

/// Why program KD is opened by no binding and no agent, if it is not, as its head says it: the kinds BASE_OF take its
/// handlers, or it is the message runtime's (`machine::DELIVERY`), which the engine runs for each agent, its tickets
/// hidden (docket-machine machine/delivery.rs). Its frame is then dashed and its pattern is no row.
fn apart(kd: &Kind, base_of: &[String]) -> Option<String> {
    if !base_of.is_empty() {
        Some(format!("its handlers are taken by {}", base_of.join(", ")))
    } else if kd.name == docket_machine::machine::DELIVERY {
        Some("the engine runs one for each agent".into())
    } else {
        None
    }
}

/// One step owed at a leaf of an open thread's tree (the views' `waits_on`).
struct Owed<'a> {
    /// The thread it is counted under: the top of the one it is on.
    top: &'a TicketView,
    /// The ticket that owes it (the thread itself for a post owed there).
    leaf: String,
    /// The leaf's kind: the program whose step it waits at.
    kind: &'a str,
    agent: String,
    call: String,
}

impl Owed<'_> {
    /// Whether it is a post owed at program KIND's hole: on a ticket of KIND itself, a post or its asks.
    fn at_hole_of(&self, kind: &str) -> bool {
        self.top.kind == kind && self.leaf == self.top.id && matches!(self.call.as_str(), "post" | "asks")
    }
}

/// Every step owed in project P, each once, oldest thread first.
fn owed(p: &Proj) -> Vec<Owed<'_>> {
    let mut vs: Vec<&TicketView> = p.live.tickets.values().filter(|v| !v.archived && v.archived_under.is_none()).collect();
    vs.sort_by_key(|v| v.n);
    let mut seen: HashSet<(String, String, String, Option<u32>)> = HashSet::new();
    let mut out = Vec::new();
    for v in vs {
        for w in &v.waits_on {
            if seen.insert((w.ticket.clone(), w.agent.clone(), w.call.clone(), w.post)) {
                let kind = leaf_kind(p, v, &w.ticket);
                out.push(Owed { top: crate::outline::goal(p, v), leaf: w.ticket.clone(), kind, agent: w.agent.clone(), call: w.call.clone() });
            }
        }
    }
    out
}

/// The kind of ticket ID, a leaf of V's tree (V itself, mostly): V's when it is not in P's view.
fn leaf_kind<'a>(p: &'a Proj, v: &'a TicketView, id: &str) -> &'a str {
    if v.id == id { &v.kind } else { p.live.tickets.values().find(|t| t.id == id).map_or(&v.kind, |t| &t.kind) }
}

/// How many steps of OWED agent A owes: the count its box says on both scales.
fn owed_by(owed: &[Owed<'_>], a: &str) -> usize {
    owed.iter().filter(|o| o.agent == a).count()
}

/// Where project P's open tickets of KIND stand, oldest first: by the call they wait on, "" for those resting at the
/// hole.
fn standing<'a>(p: &'a Proj, kind: &str) -> HashMap<String, Vec<&'a TicketView>> {
    let mut m: HashMap<String, Vec<&TicketView>> = HashMap::new();
    for v in open_of(p, kind) {
        let at = match &v.status {
            ViewStatus::Awaits { call, .. } | ViewStatus::Asking { call, .. } => call.clone(),
            ViewStatus::Resting => String::new(),
            _ => continue,
        };
        m.entry(at).or_default().push(v);
    }
    m
}

/// Whom a step written for W goes to, by name: the agent that holds the role for `role R` (refs::who), else "the R";
/// `owner` for him.
fn who_of(p: &Proj, w: &str) -> String {
    if w == "owner" { w.to_string() } else { crate::refs::who(Some(p), w).label }
}

const START_W: f32 = 150.0;
const PROG_X: f32 = 174.0;
const PROG_W: f32 = 660.0;
const WHO_X: f32 = 902.0;
const WHO_W: f32 = 196.0;
const TOP: f32 = 22.0;
const HEAD_H: f32 = 26.0;
const PAD: f32 = 12.0;
const CHIP_H: f32 = 36.0;
const GAP: f32 = 26.0;
const BOX_GAP: f32 = 10.0;

/// An arrow from a step to whom it goes: its way out of its program's box (ending at the box's edge), how many tickets
/// wait there now.
struct Out {
    who: String,
    way: Way,
    n: usize,
    /// It is a hold (owed, never given): dashed.
    hold: bool,
}

/// One row of a program's box: the handler's event that starts it (none: the pattern), and its stages.
struct Line {
    lead: Option<String>,
    items: Vec<(Option<String>, Item)>,
}

/// Program KD's rows as its box draws them: its pattern's, unless it stands APART (`apart`), then each handler of its
/// own; one that opens a program goes last, so its arrow goes straight down into that program's box.
fn box_lines(kd: &Kind, apart: bool) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    if !apart {
        let m = row_of(&kd.pattern);
        if !m.is_empty() {
            lines.push(Line { lead: None, items: m });
        }
    }
    let mut handlers: Vec<Line> = kd.handlers.iter().filter(|h| h.from.is_empty() || h.from == kd.name).map(|h| Line { lead: Some(event_words(&h.ev)), items: row_of(&h.body) }).filter(|l| !l.items.is_empty()).collect();
    handlers.sort_by_key(|l| l.items.iter().any(|(_, it)| matches!(it, Item::Open { .. })));
    lines.extend(handlers);
    lines
}

/// Whether program PR is agent A's, on the Agent scale: A wrote it (`by`), or a row of its box (APART as there) sends A
/// an arrow: a step written for A or for the role A holds (`who_of`), or a post A owes at its hole (OWED); or A owes a
/// step on a ticket of PR's kind (OWED), whoever the step is written for: a role's step its holder took (73 said "No
/// program sends docket-core a step" over "Waits on it · 1: cut_of in head", the cutter's step; docket-edge8's walk,
/// N-1446 p2353).
fn for_agent(p: &Proj, pr: &Program, apart: bool, a: &str, owed: &[Owed<'_>]) -> bool {
    pr.by.as_deref() == Some(a)
        || owed.iter().any(|o| o.agent == a && o.kind == pr.kd.name)
        || box_lines(&pr.kd, apart).iter().flat_map(|l| &l.items).any(|(_, it)| match it {
            Item::Step { who, .. } => who_of(p, who) == a,
            Item::Hole => owed.iter().any(|o| o.agent == a && o.at_hole_of(&pr.kd.name)),
            _ => false,
        })
}

/// What the board is drawn with.
struct Draw<'a> {
    p: &'a Proj,
    k: &'a Kit,
    b: Board,
    outs: Vec<Out>,
    /// The first stage's left middle of each program's first row, by kind: where what starts it points.
    entry: HashMap<String, P>,
    /// Where each program's opens leave, by the kind they open: an arrow from each to that box.
    opens: Vec<(P, String)>,
    boxes: HashMap<String, R>,
    owed: Vec<Owed<'a>>,
}

impl<'a> Draw<'a> {
    fn chip_size(&self, it: &Item, who: bool) -> (f32, f32) {
        let k = self.k;
        let w = |t: &str, st| measure(t, st).0;
        match it {
            Item::Step { who: wk, call, .. } => {
                let top = if who { w(&worker_words(wk), k.ty.small) } else { 0.0 };
                (w(call, k.ty.body).max(top) + 22.0, CHIP_H)
            }
            Item::Open { kind } => (w(kind, k.ty.body).max(w("opens", k.ty.small)) + 22.0, CHIP_H),
            Item::Decide { name } => (w(name, k.ty.body).max(w("the policy decides", k.ty.small)) + 22.0, CHIP_H),
            Item::Act { word } => (w(word, k.ty.body) + 22.0, CHIP_H),
            Item::Hole => (w("… rests", k.ty.small) + 22.0, 30.0),
        }
    }

    fn chip_el(&self, it: &Item, who: bool) -> El {
        let k = self.k;
        let boxed = |top: Option<String>, name: &str| {
            let mut c = col().center().radius(k.radius.sm).fill(k.c.layer_02).border(1.0, k.c.border_base);
            if let Some(t) = top {
                c = c.child(text(t).font(k.ty.small).color(k.c.text_muted));
            }
            c.child(text(name.to_string()).font(k.ty.body).color(k.c.text_base))
        };
        match it {
            Item::Step { who: w, call, hold } => {
                let c = boxed(who.then(|| worker_words(w)), call);
                if *hold { c.border_dash(wire::DASH[0], wire::DASH[1]) } else { c }
            }
            Item::Open { kind } => boxed(Some("opens".into()), kind),
            Item::Decide { name } => boxed(Some("the policy decides".into()), name),
            Item::Act { word } => boxed(None, word),
            Item::Hole => col().center().radius(15.0).border(1.0, k.c.border_strong).border_dash(wire::DASH[0], wire::DASH[1]).child(text("… rests").font(k.ty.small).color(k.c.text_muted)),
        }
    }

    /// Program PR's box at X, Y, W wide, with its rows and whom its steps go to; its rectangle. BASE_OF: the kinds that
    /// take its handlers (`apart`: its pattern is then no row, it is not opened itself).
    fn program_box(&mut self, pr: &Program, base_of: &[String], x: f32, y: f32, w: f32) -> R {
        let (k, p) = (self.k, self.p);
        let kd = &pr.kd;
        let apart = apart(kd, base_of);
        let lines = box_lines(kd, apart.is_some());
        let here = standing(p, &kd.name);
        let mut yy = y + HEAD_H;
        let mut rows_h = 0.0;
        for (li, line) in lines.iter().enumerate() {
            // the chips, left to right, each after an arrow wide enough for its case
            let mut placed: Vec<(R, &Item)> = Vec::new();
            let mut cx = x + PAD;
            let mut prev_who: Option<&str> = None;
            let mut lead_r = None;
            if let Some(l) = &line.lead {
                let lw = measure(l, k.ty.small).0 + 18.0;
                let r = R::new(cx, yy + (CHIP_H - 24.0) / 2.0, lw, 24.0);
                self.b.put(r, col().center().radius(12.0).border(1.0, k.c.border_strong).border_dash(wire::DASH[0], wire::DASH[1]).child(text(l.clone()).font(k.ty.small).color(k.c.text_muted)));
                lead_r = Some(r);
                cx += lw;
            }
            for (i, (case, it)) in line.items.iter().enumerate() {
                let gap = if i == 0 && lead_r.is_none() { 0.0 } else { case.as_ref().map_or(GAP, |a| (measure(a, k.ty.small).0 + 16.0).max(GAP)) };
                let who = match it {
                    Item::Step { who, .. } => prev_who.replace(who.as_str()) != Some(who.as_str()),
                    _ => false,
                };
                let (cw, ch) = self.chip_size(it, who);
                let r = R::new(cx + gap, yy + (CHIP_H - ch) / 2.0, cw, ch);
                // the arrow in, with its case over it
                if let Some(a) = placed.last().map(|(q, _)| q.right_mid()).or(lead_r.map(|q| q.right_mid())) {
                    self.b.wire(Wire::new(Way::line(&[a, r.left_mid()]), k.c.text_faint, 1.2));
                    if let Some(c) = case {
                        let tw = measure(c, k.ty.small).0;
                        self.b.words(a[0] + (r.x - a[0] - tw) / 2.0, a[1] - 17.0, c, k.ty.small, k.c.text_faint);
                    }
                }
                let el = self.chip_press(self.chip_el(it, who), it, &kd.name, li, i);
                self.b.put(r, el);
                cx = r.right();
                placed.push((r, it));
            }
            if li == 0 {
                if let Some((r, _)) = placed.first() {
                    self.entry.insert(kd.name.clone(), [r.x, r.mid_y()]);
                }
            }
            // whom each step goes to: the last chip straight out of its right side, the others down onto a line under
            // the row, one per worker
            let mut buses: Vec<String> = Vec::new();
            let last = placed.len().saturating_sub(1);
            for (i, (r, it)) in placed.iter().enumerate() {
                let (who, call, hold) = match it {
                    Item::Step { who, call, hold } => (who_of(p, who), call.as_str(), *hold),
                    Item::Open { kind } => {
                        self.opens.push((r.bottom_mid(), kind.clone()));
                        continue;
                    }
                    Item::Hole => {
                        // what is owed at the hole: the posts on the program's tickets, per agent, out of its right
                        let mut per: Vec<(String, usize)> = Vec::new();
                        for o in self.owed.iter().filter(|o| o.at_hole_of(&kd.name)) {
                            match per.iter_mut().find(|(a, _)| *a == o.agent) {
                                Some((_, n)) => *n += 1,
                                None => per.push((o.agent.clone(), 1)),
                            }
                        }
                        let resting = here.get("").cloned().unwrap_or_default();
                        if !resting.is_empty() {
                            self.badge(*r, false, resting, "");
                        }
                        for (a, n) in per {
                            self.outs.push(Out { who: a, way: Way::line(&[r.right_mid(), [x + w, r.mid_y()]]), n, hold: false });
                        }
                        continue;
                    }
                    // a decision and the runtime's acts go to no one
                    Item::Decide { .. } | Item::Act { .. } => continue,
                };
                let at = here.get(call).cloned().unwrap_or_default();
                let n = at.len();
                if n > 0 {
                    self.badge(*r, who == "owner", at, call);
                }
                let shared = placed.iter().filter(|(_, o)| matches!(o, Item::Step { who: w2, .. } if who_of(p, w2) == who)).count() > 1;
                let way = if i == last && !shared {
                    Way::line(&[r.right_mid(), [x + w, r.mid_y()]])
                } else {
                    let j = buses.iter().position(|b| *b == who).unwrap_or_else(|| {
                        buses.push(who.clone());
                        buses.len() - 1
                    });
                    let by = yy + CHIP_H + 7.0 + 5.0 * j as f32;
                    Way::elbow(&[r.bottom_mid(), [r.mid_x(), by], [x + w, by]], 6.0)
                };
                self.outs.push(Out { who, way, n, hold });
            }
            let h = CHIP_H + 12.0 + 5.0 * buses.len().saturating_sub(1) as f32;
            yy += h;
            rows_h += h;
        }
        let r = R::new(x, y, w, HEAD_H + rows_h + 4.0);
        let mut frame = rune::el().radius(k.radius.md).fill(k.c.layer_01).border(1.0, k.c.border_weak);
        if apart.is_some() {
            frame = frame.border_dash(wire::DASH[0], wire::DASH[1]);
        }
        self.b.frame(r, frame);
        // its head: name, doc, what is open of it; a press opens its card under the board
        let doc = pr.doc.clone();
        let open: usize = here.values().map(Vec::len).sum();
        let mut head = row().w_full().h_full().px(PAD).gap(8.0).center_y().child(text(kd.name.clone()).font(k.ty.small_medium).color(k.c.text_base));
        head = head.child(text(doc).font(k.ty.small).color(k.c.text_muted).truncate().shrink()).child(rune::spacer());
        if let Some(why) = apart {
            head = head.child(text(why).font(k.ty.small).color(k.c.text_faint));
        }
        if open > 0 {
            head = head.child(text(crate::refs::count("Open", open)).font(k.ty.small).color(k.c.text_muted));
        }
        let name = kd.name.clone();
        self.b.put(R::new(x, y, w, HEAD_H), k.interactive(head.key(format!("docket-kind-{}", kd.name)).radius(k.radius.md)).on_press(move |s: &mut App| show_program(s, &name)));
        self.boxes.insert(kd.name.clone(), r);
        r
    }

    /// The count of tickets VS at step CALL (empty: resting at the hole), on AT's corner; YOURS in the accent; a press
    /// opens the one as its link would, else their list under the board (`count:`), each one opening.
    fn badge(&mut self, at: R, yours: bool, vs: Vec<&TicketView>, call: &str) {
        let k = self.k;
        let words = vs.len().to_string();
        let w = measure(&words, k.ty.small).0 + 12.0;
        let ink = if yours { k.c.text_accent } else { k.c.text_base };
        let el = row().center().radius(9.0).fill(k.c.layer_03).border(1.0, if yours { k.c.text_accent } else { k.c.border_strong }).child(text(words).font(k.ty.small).color(ink));
        let help: Vec<String> = vs.iter().map(|v| format!("{} · {}", crate::refs::ticket(Some(self.p), v).label, v.id)).collect();
        let one = (vs.len() == 1).then(|| vs[0].id.clone());
        let kind = vs.first().map(|v| v.kind.clone()).unwrap_or_default();
        let what = format!("count:{kind}:{call}");
        let el = k.interactive(el.key(format!("docket-count-{}-{}", kind, vs.first().map_or(0, |v| v.n)))).help(help.join("\n")).on_press(move |s: &mut App| match &one {
            Some(id) => {
                crate::mentions::open(s, id);
            }
            None => show_panel(s, &what),
        });
        self.b.put(R::new(at.right() - w + 6.0, at.y - 9.0, w, 18.0), el);
    }

    /// Chip IT of program KIND (row LI, place I) as a press for what it stands for (paper-research N-1396 p223): an
    /// `opens` its open tickets, the one as its `Q-…` link opens it, several as a list under the board, none its
    /// program's card; an act what it does (`act:`); a step its call, in its program's card under the board
    /// (`open_call`).
    fn chip_press(&self, el: El, it: &Item, kind: &str, li: usize, i: usize) -> El {
        let k = self.k;
        match it {
            Item::Open { kind: to } => {
                let vs = open_of(self.p, to);
                let one = (vs.len() == 1).then(|| vs[0].id.clone());
                let many = vs.len() > 1;
                let to = to.clone();
                k.interactive(el.key(format!("docket-opens-{kind}-{li}-{i}"))).on_press(move |s: &mut App| match (&one, many) {
                    (Some(id), _) => {
                        crate::mentions::open(s, id);
                    }
                    (None, true) => show_panel(s, &format!("opens:{to}")),
                    (None, false) => show_program(s, &to),
                })
            }
            Item::Act { word } => {
                let what = format!("act:{word}");
                k.interactive(el.key(format!("docket-act-{kind}-{li}-{i}"))).on_press(move |s: &mut App| show_panel(s, &what))
            }
            // an inline step's prompt is its program's text: its card, as the box's head opens it
            Item::Step { who, call, .. } => {
                let (kind, call, by) = (kind.to_string(), call.clone(), who_of(self.p, who));
                k.interactive(el.key(format!("docket-step-{kind}-{li}-{i}"))).on_press(move |s: &mut App| {
                    if call == docket_machine::syntax::INLINE {
                        show_program(s, &kind)
                    } else {
                        open_call(s, &kind, &call, &by, None)
                    }
                })
            }
            _ => el,
        }
    }
}

/// Program KIND's card under the board, open (its text and whole picture), or put away when it already is.
fn show_program(s: &mut App, kind: &str) {
    let key = format!("{SHOWN}{kind}");
    let was = s.tickets.pic_open.remove(&key);
    s.tickets.pic_open.retain(|k| !k.starts_with(SHOWN));
    if !was {
        s.tickets.pic_open.insert(key);
        s.kinds_open.insert(kind.to_string());
    }
}

/// The key in `pic_open` of what a press on the board opened under it (paper-research N-1396 p223), one at a time:
/// the tickets at a count (`count:KIND:CALL`, CALL empty for those resting at the hole), those an `opens` chip of
/// program FROM opened (`opens:FROM:KIND`), the runs of a start (`start:KIND:I`, binding I of KIND; `start:` for "an
/// agent opens one"), what an act chip does (`act:WORD`).
pub(crate) const PANEL: &str = "docket-panel:";

/// WHAT open under the board, in place of what was; put away when it already is (the same press again).
pub(crate) fn show_panel(s: &mut App, what: &str) {
    let key = format!("{PANEL}{what}");
    let was = s.tickets.pic_open.remove(&key);
    s.tickets.pic_open.retain(|k| !k.starts_with(PANEL));
    if !was {
        s.tickets.pic_open.insert(key);
    }
}

/// Ticket R's reference (`label · id`, LEAD before it): a press opens it as a `Q-…` or `N-…` in a post does (C19's
/// link, `mentions::open`).
fn ref_row(r: &crate::refs::Ref, key: String, lead: Option<&str>, k: &Kit) -> El {
    let mut l = row().gap(8.0).center_y();
    if let Some(w) = lead {
        l = l.child(text(w.to_string()).font(k.ty.small).color(k.c.text_faint));
    }
    let l = l.child(crate::refs::el(r, k.ty.small, k.c.text_base, k)).key(key);
    if r.id.is_empty() {
        return l;
    }
    // a post (`N-7.p3`) opens at it; a step's or a closing notice's (`C-12.s3`, `C-12.c`, a delivery's items) its ticket
    let id = match r.id.split_once('.') {
        Some((t, at)) if !at.strip_prefix('p').is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())) => t.to_string(),
        _ => r.id.clone(),
    };
    k.interactive(l).on_press(move |s: &mut App| {
        crate::mentions::open(s, &id);
    })
}

/// Project P's open tickets of KIND, oldest first: what an `opens KIND` chip stands for (a question opened by `coord
/// ask` names no ticket it was opened from, so none is left out for that).
fn open_of<'a>(p: &'a Proj, kind: &str) -> Vec<&'a TicketView> {
    let mut vs: Vec<&TicketView> = p.live.tickets.values().filter(|v| v.kind == kind && !v.archived && v.archived_under.is_none() && v.closed_at.is_none()).collect();
    vs.sort_by_key(|v| v.n);
    vs
}

/// What act WORD of the message runtime's does (docket-machine machine/delivery.rs), for its chip's panel.
fn act_words(word: &str) -> &'static str {
    match word {
        "deliver" => "Hands what waits to the agent.",
        "fork" => "Hands it to a copy of the agent.",
        "keep" => "Holds it for a later deliver.",
        "report" => "Can't deliver now; it waits.",
        _ => "",
    }
}

/// W picked: only what goes to W lit (the board), W's part of the Docket (the Agent scale).
pub(crate) fn focus_only(s: &mut App, w: &str) {
    s.tickets.pic_open.retain(|k| !k.starts_with(FOCUS));
    s.tickets.pic_open.insert(format!("{FOCUS}{w}"));
}

/// The programs the board shows, in order: those a binding of the project opens, the kinds they take handlers from,
/// those they open, those with open tickets (most first); then the agents' own. The rest, by name.
fn shown(p: &Proj, prs: &[Program]) -> (Vec<String>, Vec<String>, Vec<String>) {
    let of = |n: &str| prs.iter().find(|x| x.kd.name == n);
    let mut out: Vec<String> = Vec::new();
    let add = |n: &str, out: &mut Vec<String>| {
        if of(n).is_some_and(|x| x.by.is_none()) && !out.iter().any(|x| x == n) {
            out.push(n.to_string());
        }
    };
    for x in prs.iter().filter(|x| !x.into.is_empty()) {
        add(&x.kd.name, &mut out);
    }
    for n in out.clone() {
        if let Some(base) = of(&n).and_then(|x| x.kd.base.clone()) {
            add(&base, &mut out);
        }
    }
    for n in out.clone() {
        if let Some(x) = of(&n) {
            let rows = std::iter::once(row_of(&x.kd.pattern)).chain(x.kd.handlers.iter().map(|h| row_of(&h.body)));
            for (_, it) in rows.flatten() {
                if let Item::Open { kind } = it {
                    add(&kind, &mut out);
                }
            }
        }
    }
    let mut live: Vec<(String, usize)> = prs.iter().map(|x| (x.kd.name.clone(), standing(p, &x.kd.name).values().map(Vec::len).sum::<usize>())).filter(|(_, n)| *n > 0).collect();
    live.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (n, _) in live {
        add(&n, &mut out);
    }
    // the message runtime's program: the engine runs one for each agent, its tickets hidden, so none is ever open here
    add(docket_machine::machine::DELIVERY, &mut out);
    let own: Vec<String> = prs.iter().filter(|x| x.by.is_some()).map(|x| x.kd.name.clone()).collect();
    let rest: Vec<String> = prs.iter().filter(|x| x.by.is_none() && !out.contains(&x.kd.name)).map(|x| x.kd.name.clone()).collect();
    (out, own, rest)
}

/// What starts binding B, for its node on the left: the clock's period or decision and its next firing, from the
/// snapshot's clocks (consult-3, N-1396 p133: not from a second parse), else what the binding is on. Whose own program
/// it is the board says over its box.
fn start_node(p: &Proj, b: &Binding) -> (String, Option<String>) {
    match p.live.clocks.iter().find(|c| c.binding == b.text) {
        Some(c) => {
            let words = match (c.every, &c.when) {
                (Some(s), _) => format!("every {}", crate::when::took(s)),
                (None, Some((d, o))) => format!("when {d} is {o}"),
                _ => start_words(b),
            };
            (words, clock_words(c).into_iter().next())
        }
        None => (start_words(b), None),
    }
}

impl<'a> Pic<'a> {
    /// The project's Docket from the Kinds answer's KINDS, lit by the project's tickets and agents (0.2 line 26): what
    /// starts each program, the programs, whom their steps go to, and what waits where now. A person picked lights only
    /// the arrows to them.
    pub fn docket(&self, kinds: &[KindForm]) -> El {
        self.docket_of(kinds, None)
    }

    /// The Docket; on the Agent scale (p218), ONLY's alone: the programs that send it a step or that it wrote
    /// (`for_agent`), whom their steps go to, and it.
    fn docket_of(&self, kinds: &[KindForm], only: Option<&str>) -> El {
        let k = self.k;
        let Some(p) = self.p else { return col() };
        let (prs, bad) = programs(kinds);
        let of = |n: &str| prs.iter().find(|x| x.kd.name == n);
        let focus = picked(self.open, FOCUS);
        let (names, own, rest) = shown(p, &prs);
        let mut g = Draw { p, k, b: Board::new(), outs: Vec::new(), entry: HashMap::new(), opens: Vec::new(), boxes: HashMap::new(), owed: owed(p) };
        for (x, words) in [(0.0, "What starts it"), (PROG_X, "Its programs, each step in the order it runs"), (WHO_X, "Whom each step goes to")] {
            g.b.words(x, 0.0, words, k.ty.small, k.c.text_faint);
        }
        let mut bases: HashMap<String, Vec<String>> = HashMap::new();
        for n in &names {
            if let Some(b) = of(n).and_then(|x| x.kd.base.clone()) {
                bases.entry(b).or_default().push(n.clone());
            }
        }
        let (names, own, rest) = match only {
            None => (names, own, rest),
            Some(a) => {
                let mine = |n: &String| of(n).is_some_and(|x| for_agent(p, x, apart(&x.kd, bases.get(n).map_or(&[][..], Vec::as_slice)).is_some(), a, &g.owed));
                let keep = |v: Vec<String>| -> Vec<String> { v.into_iter().filter(|n| mine(n)).collect() };
                (keep(names), keep(own), keep(rest))
            }
        };
        if let Some(a) = only.filter(|_| names.is_empty() && own.is_empty() && rest.is_empty()) {
            return col().key("docket").w_full().child(text(format!("No program sends {} a step, and {} wrote none.", crate::refs::person(a), crate::refs::person(a))).font(k.ty.small).color(k.c.text_muted).wrap());
        }
        // the programs, one under the other (a box beside another would send its arrows through it)
        let mut y = TOP;
        for n in &names {
            if let Some(x) = of(n) {
                y = g.program_box(x, bases.get(n).map_or(&[][..], Vec::as_slice), PROG_X, y, PROG_W).bottom() + BOX_GAP;
            }
        }
        // the agents' own programs, under their agent's name
        let mut by_agent: Vec<(String, Vec<&Program>)> = Vec::new();
        for x in own.iter().filter_map(|n| of(n)) {
            let by = x.by.clone().unwrap_or_default();
            match by_agent.iter_mut().find(|(a, _)| *a == by) {
                Some((_, v)) => v.push(x),
                None => by_agent.push((by, vec![x])),
            }
        }
        for (a, xs) in &by_agent {
            y += 2.0;
            g.b.words(PROG_X, y, &format!("{a}'s own programs, written for itself"), k.ty.small, k.c.text_muted);
            y += 20.0;
            for x in xs {
                y = g.program_box(x, &[], PROG_X, y, PROG_W).bottom() + BOX_GAP;
            }
        }
        if !rest.is_empty() {
            // each name a press that opens its card, as a box's head does
            let mut x = g.b.words(PROG_X, y, "Nothing open now:", k.ty.small, k.c.text_faint).right() + 8.0;
            for n in &rest {
                let w = measure(n, k.ty.small).0;
                if x + w > PROG_X + PROG_W {
                    (x, y) = (PROG_X, y + 20.0);
                }
                let to = n.clone();
                g.b.put(R::new(x, y, w, 18.0), k.interactive(text(n.clone()).font(k.ty.small).color(k.c.text_muted).key(format!("docket-rest-{n}"))).on_press(move |s: &mut App| show_program(s, &to)));
                x += w + 12.0;
            }
            y += 22.0;
        }
        let bottom = y;

        // what starts each: its bindings (his message, a clock), else an agent opening one; each a press that opens its
        // runs under the board (`start:`)
        let mut by_agent_opened: Vec<P> = Vec::new();
        let mut agents_open: Vec<String> = Vec::new();
        let opened_inside: Vec<String> = g.opens.iter().map(|(_, n)| n.clone()).collect();
        for n in names.iter().chain(own.iter()) {
            let (Some(&to), Some(x)) = (g.entry.get(n), of(n)) else { continue };
            if x.into.is_empty() {
                if !opened_inside.contains(n) && apart(&x.kd, bases.get(n).map_or(&[][..], Vec::as_slice)).is_none() {
                    by_agent_opened.push(to);
                    agents_open.push(n.clone());
                }
                continue;
            }
            let h = 40.0;
            let top = to[1] - h * x.into.len() as f32 / 2.0;
            for (i, b) in x.into.iter().enumerate() {
                let (words, sub) = start_node(p, b);
                let r = R::new(0.0, top + i as f32 * (h + 4.0), START_W, h);
                // its clock's line being edited (p294): marked as the panel it is edited in is
                let editing = self.marks.binding.as_deref() == Some(b.text.as_str());
                let mut c = col().px(10.0).justify_center().radius(k.radius.md).fill(k.c.layer_01).border(if editing { 2.0 } else { 1.0 }, if editing { k.c.border_focus } else { k.c.border_base }).child(text(words).font(k.ty.small_medium).color(k.c.text_base).truncate());
                if editing {
                    c = c.child(text("editing".to_string()).font(k.ty.small).color(k.c.text_muted).key(format!("docket-start-{n}-{i}-editing")));
                } else if let Some(sub) = sub {
                    c = c.child(text(sub).font(k.ty.small).color(k.c.text_muted).truncate());
                }
                let what = format!("start:{n}:{i}");
                g.b.put(r, k.interactive(c.key(format!("docket-start-{n}-{i}"))).on_press(move |s: &mut App| show_panel(s, &what)));
                g.b.wire(Wire::new(Way::across(r.right_mid(), to), k.c.text_muted, 1.4));
            }
        }
        if !by_agent_opened.is_empty() {
            let mid = by_agent_opened.iter().map(|q| q[1]).sum::<f32>() / by_agent_opened.len() as f32;
            let r = R::new(0.0, mid - 20.0, START_W, 40.0);
            let c = col().px(10.0).justify_center().radius(k.radius.md).fill(k.c.layer_01).border(1.0, k.c.border_base).child(text("an agent opens one").font(k.ty.small_medium).color(k.c.text_base)).child(text("when its work needs it").font(k.ty.small).color(k.c.text_muted));
            g.b.put(r, k.interactive(c.key("docket-start-agents")).on_press(|s: &mut App| show_panel(s, "start:")));
            for to in by_agent_opened {
                g.b.wire(Wire::new(Way::across(r.right_mid(), to), k.c.text_muted, 1.4));
            }
        }
        // an open: down (or up) from where it is written into the box it opens
        for (from, n) in g.opens.clone() {
            if let Some(r) = g.boxes.get(&n).copied() {
                let to = if r.y > from[1] { [from[0], r.y] } else { [from[0], r.bottom()] };
                g.b.wire(Wire::new(Way::down(from, to), k.c.text_muted, 1.4));
            }
        }

        // whom: each one an arrow goes to, in the order their arrows come from, so few cross; the idle after
        let mut whos: Vec<(String, f32, usize)> = Vec::new();
        for o in &g.outs {
            let y0 = o.way.end()[1];
            match whos.iter_mut().find(|(w, _, _)| *w == o.who) {
                Some((_, sum, n)) => {
                    *sum += y0;
                    *n += 1;
                }
                None => whos.push((o.who.clone(), y0, 1)),
            }
        }
        whos.sort_by(|a, b| (a.1 / a.2 as f32).total_cmp(&(b.1 / b.2 as f32)));
        let mut people: Vec<String> = whos.into_iter().map(|(w, _, _)| w).collect();
        // on the project's board every agent; on the Agent scale, it alone
        let idle: Vec<String> = match only {
            None => crate::agents::teams(&p.live.agents, &p.live.parts).into_iter().flat_map(|(_, v)| v).map(|a| a.name.clone()).collect(),
            Some(a) => vec![a.to_string()],
        };
        for a in idle {
            if !people.contains(&a) {
                people.push(a);
            }
        }
        let step = ((bottom - TOP) / people.len().max(1) as f32).clamp(44.0, 64.0);
        let mut at: HashMap<String, R> = HashMap::new();
        for (i, w) in people.iter().enumerate() {
            let r = R::new(WHO_X, TOP + i as f32 * step, WHO_W, 40.0);
            // what waits on it, as the rest of the window counts it: on him, the Threads page's Waits on you (waits.rs
            // `on_owner`); on an agent, every step it owes (`owed_by`), as its box on the Agent scale says; on a role no
            // agent stands for, what its arrows bring. 73 counted the arrows alone, so a wait at a step the board does
            // not draw went uncounted: "Owed · 1" here over "Owed · 20" on the Agent scale, "Waits on you · 11" over
            // the Threads page's 13 (docket-edge8's walk, N-1446 p2353)
            let load: usize = if w == "owner" {
                crate::waits::on_owner(p).len()
            } else if p.live.agents.iter().any(|a| a.name == *w) {
                owed_by(&g.owed, w)
            } else {
                g.outs.iter().filter(|o| o.who == *w).map(|o| o.n).sum()
            };
            g.b.put(r, who_node(p, w, load, focus == Some(w.as_str()), k));
            at.insert(w.clone(), r);
        }
        // the arrows to them, spread down each one's left edge in the order they come; the quiet first, the load over
        let mut ends: Vec<(usize, P)> = Vec::new();
        for w in &people {
            let Some(r) = at.get(w) else { continue };
            let mut mine: Vec<usize> = (0..g.outs.len()).filter(|&i| g.outs[i].who == *w).collect();
            mine.sort_by(|&a, &b| g.outs[a].way.end()[1].total_cmp(&g.outs[b].way.end()[1]));
            let n = mine.len();
            for (j, i) in mine.into_iter().enumerate() {
                ends.push((i, [r.x, r.y + 6.0 + (r.h - 12.0) * (j as f32 + 0.5) / n as f32]));
            }
        }
        ends.sort_by_key(|(i, _)| (g.outs[*i].n > 0, g.outs[*i].n));
        for (i, end) in ends {
            let o = &g.outs[i];
            let lit = focus.is_none_or(|f| f == o.who);
            let (ink, width) = match (o.n, o.who == "owner") {
                (0, _) => (k.c.text_faint, 1.0),
                _ if !lit => (k.c.text_faint, 1.0),
                (n, true) => (k.c.text_accent, 1.6 + 0.7 * n.min(4) as f32),
                (n, false) => (k.c.text_muted, 1.6 + 0.7 * n.min(4) as f32),
            };
            let mut wire = Wire::new(o.way.clone().across_to(end), ink, width);
            if o.hold {
                wire = wire.dashed();
            }
            g.b.wire(wire);
        }

        // the drawing's words, once, under it
        let ly = bottom.max(TOP + people.len() as f32 * step) + 4.0;
        let mut lx = 0.0;
        for (ink, width, words) in [(k.c.text_faint, 1.0, "a step goes to them"), (k.c.text_muted, 3.0, "tickets wait there now, as thick as how many"), (k.c.text_accent, 3.0, "waits on you")] {
            g.b.wire(Wire::new(Way::line(&[[lx, ly + 8.0], [lx + 34.0, ly + 8.0]]), ink, width));
            lx = g.b.words(lx + 42.0, ly, words, k.ty.small, k.c.text_faint).right() + 26.0;
        }
        let r = R::new(lx, ly - 2.0, 22.0, 20.0);
        g.b.put(r, row().center().radius(9.0).fill(k.c.layer_03).border(1.0, k.c.border_strong).child(text("2").font(k.ty.small).color(k.c.text_base)));
        g.b.words(r.right() + 8.0, ly, "tickets at that step; a press opens them", k.ty.small, k.c.text_faint);
        let mut c = col().key("docket").w_full().gap(10.0).child(col().w_full().scroll_x().child(g.b.el("docket-board")));
        // what a press on it opened, under it
        if let Some(el) = picked(self.open, PANEL).and_then(|what| board_panel(p, what, &prs, &agents_open, only, k)) {
            c = c.child(el);
        }
        if !bad.is_empty() {
            c = c.child(text(format!("Not drawn, its text does not parse: {}", bad.join(", "))).font(k.ty.small).color(k.c.text_muted).wrap());
        }
        c
    }
}

/// What a press on the board opened under it (WHAT, the value of PANEL), from project P and its programs PRS; BY_AGENTS:
/// the programs "an agent opens one" points to; ONLY: the agent of the Agent scale, whose acts alone an act chip lists.
/// Each ticket a press that opens it; the panel's own press puts it away. None when what it names is no longer drawn.
fn board_panel(p: &Proj, what: &str, prs: &[Program], by_agents: &[String], only: Option<&str>, k: &Kit) -> Option<El> {
    let (on, rest) = what.split_once(':')?;
    let faint = |t: String| text(t).font(k.ty.small).color(k.c.text_muted).wrap();
    let ticket_row = |v: &TicketView, lead: Option<&str>, after: Option<String>| {
        let mut r = row().gap(8.0).center_y().child(ref_row(&crate::refs::ticket(Some(p), v), format!("docket-panel-{}", v.id), lead, k));
        if let Some(a) = after {
            r = r.child(text(a).font(k.ty.small).color(k.c.text_faint));
        }
        r
    };
    let (mut lines, mut rows): (Vec<String>, Vec<El>) = (Vec::new(), Vec::new());
    let head = match on {
        // the tickets at a step, or resting at the hole
        "count" => {
            let (kind, call) = rest.split_once(':')?;
            let vs = standing(p, kind).remove(call).unwrap_or_default();
            rows.extend(vs.iter().map(|v| ticket_row(v, None, None)));
            crate::refs::count(&if call.is_empty() { format!("{kind}, resting") } else { format!("{kind}, at {call}") }, vs.len())
        }
        // the open tickets an `opens` chip stands for, with what each was opened from
        "opens" => {
            let vs = open_of(p, rest);
            rows.extend(vs.iter().map(|v| ticket_row(v, None, v.parent.as_ref().map(|t| format!("from {t}")))));
            crate::refs::count(&format!("{rest}, open"), vs.len())
        }
        // a start's runs: the tickets it opened, newest first, with the clock's next firing and its last
        "start" if rest.is_empty() => {
            let mut vs: Vec<&TicketView> = by_agents.iter().flat_map(|n| open_of(p, n)).collect();
            vs.sort_by_key(|v| std::cmp::Reverse(v.n));
            if vs.is_empty() {
                lines.push(format!("None of {} is open now.", by_agents.join(", ")));
            }
            rows.extend(vs.iter().map(|v| ticket_row(v, Some(v.kind.as_str()), v.opener.as_ref().map(|a| format!("opened by {}", crate::refs::person(a))))));
            "An agent opens one, when its work needs it".to_string()
        }
        "start" => {
            let (kind, i) = rest.split_once(':')?;
            let b = prs.iter().find(|x| x.kd.name == kind)?.into.get(i.parse::<usize>().ok()?)?;
            let clock = p.live.clocks.iter().find(|c| c.binding == b.text);
            lines.push(format!("Opens {kind}, as written: {}", b.text));
            if let Some(c) = clock {
                lines.extend(clock_words(c));
                if let Some(by) = &c.by {
                    lines.push(format!("{}'s own program", crate::refs::person(by)));
                }
                // its line edited in place, under the board (p294)
                let (kind, file, line, binding) = (kind.to_string(), c.file.clone(), c.line, c.binding.clone());
                rows.push(row().w_full().justify_end().child(rune_kit::Button::new("Edit this line").variant(rune_kit::ButtonVariant::Ghost).on_press(move |s: &mut App| crate::edit::start_binding(s, &kind, &file, line, &binding)).build(k).key("edit-clock-line")));
            }
            let mut vs: Vec<&TicketView> = p.live.tickets.values().filter(|v| v.binding.as_deref() == Some(b.text.as_str())).collect();
            vs.sort_by_key(|v| std::cmp::Reverse(v.n));
            if vs.is_empty() {
                lines.push("No run of it is open, or closed in the last day.".into());
            }
            rows.extend(vs.iter().map(|v| ticket_row(v, Some(crate::when::at(&v.opened_at).as_str()), Some(if v.closed_at.is_some() { "closed".into() } else { "open".into() }))));
            start_node(p, b).0
        }
        // what an act of the message runtime's does, and its last ones (the agents' `acted`), the newest first: when,
        // for whom, the items it took, what it named at the foot, and why
        "act" => {
            lines.push(act_words(rest).to_string());
            let mut done: Vec<(&str, &docket_machine::ViewActed)> = p.live.agents.iter().filter(|a| only.is_none_or(|o| o == a.name)).flat_map(|a| a.acted.iter().filter(|x| x.act == rest).map(move |x| (a.name.as_str(), x))).collect();
            done.sort_by(|a, b| b.1.at.cmp(&a.1.at));
            done.truncate(docket_machine::machine::ACTS_KEPT);
            if done.is_empty() {
                lines.push(format!("No {rest} done lately{}.", only.map(|a| format!(" for {}", crate::refs::person(a))).unwrap_or_default()));
            }
            for (i, (agent, x)) in done.into_iter().enumerate() {
                let mut r = row().gap(8.0).center_y().flex_wrap().child(text(crate::when::at(&x.at)).font(k.ty.small).color(k.c.text_faint)).child(text(crate::refs::person(agent)).font(k.ty.small_medium).color(k.c.text_base));
                if x.items.is_empty() && x.more_items == 0 {
                    r = r.child(text("nothing still waited".to_string()).font(k.ty.small).color(k.c.text_muted));
                }
                // its ids, each opening its ticket, then a count of those the desk does not keep (ViewActed::of)
                let ids = |mut r: El, ids: &[String], more: usize, key: &str| {
                    for (j, id) in ids.iter().enumerate() {
                        let t = id.split('.').next().unwrap_or(id);
                        let r0 = crate::refs::Ref { label: crate::refs::ticket_id(p, t).label, id: id.clone() };
                        r = r.child(ref_row(&r0, format!("{key}-{j}"), None, k));
                    }
                    if more > 0 {
                        r = r.child(text(format!("and {more} more")).font(k.ty.small).color(k.c.text_muted));
                    }
                    r
                };
                r = ids(r, &x.items, x.more_items, &format!("docket-act-{i}"));
                if !x.rest.is_empty() {
                    r = r.child(text("named at its foot:".to_string()).font(k.ty.small).color(k.c.text_muted));
                    r = ids(r, &x.rest, x.more_rest, &format!("docket-act-{i}-rest"));
                }
                let mut c = col().gap(2.0).key(format!("docket-acted-{i}")).child(r);
                if let Some(why) = &x.why {
                    c = c.child(text(format!("why: {why}")).font(k.ty.small).color(k.c.text_muted).wrap());
                }
                rows.push(c);
            }
            rest.to_string()
        }
        _ => return None,
    };
    let close = rune_kit::Button::new("Put away").variant(rune_kit::ButtonVariant::Ghost).on_press(|s: &mut App| s.tickets.pic_open.retain(|k| !k.starts_with(PANEL))).build(k).key("docket-panel-close");
    let mut c = col().key("docket-panel").w_full().gap(6.0).padding(12.0).radius(k.radius.md).fill(k.c.layer_01).border(1.0, k.c.border_weak);
    c = c.child(row().w_full().gap(8.0).center_y().child(text(head).font(k.ty.small_medium).color(k.c.text_base)).child(rune::spacer()).child(close));
    for l in lines {
        c = c.child(faint(l));
    }
    for r in rows {
        c = c.child(r);
    }
    Some(c)
}

/// A worker on the right: its name, what waits on it, and what it does now; a press opens its part of the Docket (the
/// Agent scale, `open_agent`): who it is now, what waits on it on each program, the threads it is on.
fn who_node(p: &Proj, w: &str, load: usize, focused: bool, k: &Kit) -> El {
    let c = who_box(p, w, load, focused, k).key(format!("docket-who-{w}"));
    let to = w.to_string();
    k.interactive(c).on_press(move |s: &mut App| open_agent(s, &to))
}

/// Worker W's box: its name and role, what waits on it (LOAD) and what it does now; FOCUSED, lit.
fn who_box(p: &Proj, w: &str, load: usize, focused: bool, k: &Kit) -> El {
    let a = p.live.agents.iter().find(|a| a.name == w);
    let yours = w == "owner";
    let state = match a {
        Some(a) if a.kind == "program" => "a program".to_string(),
        Some(a) if a.state == "working" => "working".to_string(),
        Some(a) => crate::agents::idle_since(p, a),
        None if yours => String::new(),
        None => "the engine's model call".to_string(),
    };
    let role = a.map(|a| a.role.clone()).filter(|r| !r.is_empty() && r != "member");
    let mut line = Vec::new();
    if load > 0 {
        line.push(crate::refs::count(if yours { "Waits on you" } else { "Owed" }, load));
    }
    if !state.is_empty() {
        line.push(state);
    }
    let ink = if yours && load > 0 { k.c.text_accent } else if load == 0 && a.is_some_and(|a| a.state != "working") { k.c.text_muted } else { k.c.text_base };
    let mut name = row().gap(6.0).center_y().child(text(crate::refs::person(w)).font(k.ty.small_medium).color(ink));
    if let Some(r) = role {
        name = name.child(text(r).font(k.ty.small).color(k.c.text_faint));
    }
    let border = if yours && load > 0 { k.c.text_accent } else if focused { k.c.text_base } else { k.c.border_weak };
    col().px(10.0).justify_center().radius(k.radius.md).fill(k.c.layer_01).border(1.0, border).child(name).child(text(line.join(" · ")).font(k.ty.small).color(k.c.text_muted).truncate())
}

/// The Docket's Agent scale (p218; docket-edge8's switch calls it). Over it the project's agents to pick from, as the
/// board's right column takes them (`agents::teams`), the one picked lit, and a worker picked from the board that is
/// none of them (the watcher, the owner); a press picks one. Under it, for the worker picked (FOCUS, the board's own
/// pick): who it is now (its model); its programs, the board with only those that send it a step or that it wrote,
/// its arrows lit; what waits on it, each step with its program and thread; and the threads it is on, the one it works
/// on first, then its own list as the Agents page has it (`outline::pieces`). Each thread a press that opens it. What
/// is empty is said once, in one line.
pub(crate) fn agent_scale(s: &App, p: &Proj, kinds: &[KindForm], k: &Kit) -> El {
    let who = picked(&s.tickets.pic_open, FOCUS);
    let owed = owed(p);
    let head = |t: String| text(t).font(k.ty.small_medium).color(k.c.text_muted);
    let faint = |t: String| text(t).font(k.ty.small).color(k.c.text_faint).wrap();
    let mut names: Vec<String> = crate::agents::teams(&p.live.agents, &p.live.parts).into_iter().flat_map(|(_, v)| v).map(|a| a.name.clone()).collect();
    if let Some(w) = who.filter(|w| !names.iter().any(|n| n == w)) {
        names.push(w.to_string());
    }
    let mut pick = row().key("docket-agents").w_full().gap(8.0).flex_wrap();
    for n in &names {
        let load = owed_by(&owed, n);
        let c = who_box(p, n, load, who == Some(n.as_str()), k).w(WHO_W).h(40.0).key(format!("docket-pick-{n}"));
        let to = n.clone();
        pick = pick.child(k.interactive(c).on_press(move |s: &mut App| focus_only(s, &to)));
    }
    let c = col().key("docket-agent").w_full().gap(16.0).child(pick);
    let Some(a) = who else {
        return c.child(faint("No agent picked: pick one to see its programs, what waits on it and the threads it is on.".into()));
    };
    // the owner's own part reads "you"
    let it = if a == "owner" { "you" } else { "it" };
    // who it is now: its model, as the Agents page has it
    let view = p.live.agents.iter().find(|x| x.name == a);
    let c = match view.filter(|x| x.kind != "program" && !x.model.is_empty()) {
        Some(x) => c.child(faint(format!("{} runs on {}.", crate::refs::person(a), x.model)).key("docket-agent-model")),
        None => c,
    };
    // its programs
    let words = |x: &Value| crate::texts::words(&s.tickets, x);
    let pic = Pic { k, p: Some(p), kinds: kind_texts(&s.tickets), tickets: Vec::new(), open: &s.tickets.pic_open, marks: crate::edit::marks(&s.tickets), words: &words, pick: None, picked: None };
    let c = c.child(pic.docket_of(kinds, Some(a)));
    // what waits on it, each step with its program and its thread
    let mine: Vec<&Owed<'_>> = owed.iter().filter(|o| o.agent == a).collect();
    let mut w = col().key("docket-agent-owed").w_full().gap(4.0);
    w = if mine.is_empty() { w.child(faint(format!("Nothing waits on {it}."))) } else { w.child(head(crate::refs::count(&format!("Waits on {it}"), mine.len()))) };
    for (i, o) in mine.iter().enumerate() {
        let leaf = p.live.tickets.values().find(|t| t.id == o.leaf);
        let program = leaf.map_or(o.top.kind.as_str(), |t| t.kind.as_str());
        let mut l = row().gap(8.0).center_y().child(text(o.call.clone()).font(k.ty.small_medium).color(k.c.text_base)).child(text(format!("in {program}")).font(k.ty.small).color(k.c.text_muted));
        l = l.child(ref_row(&crate::refs::ticket(Some(p), o.top), format!("docket-owed-{i}"), Some("on"), k));
        if o.leaf != o.top.id {
            l = l.child(text(o.leaf.clone()).font(k.ty.small).color(k.c.text_faint));
        }
        w = w.child(l);
    }
    // the threads it is on: the one it works on, then its list
    let now = view.and_then(|x| crate::agents::working_on(p, x));
    let list = crate::outline::pieces(p, Some(a));
    let mut t = col().key("docket-agent-threads").w_full().gap(4.0);
    t = if list.is_empty() && now.is_none() { t.child(faint("On no thread now.".into())) } else { t.child(head(format!("The threads {it} {} on", if it == "you" { "are" } else { "is" }))) };
    if let Some(r) = &now {
        t = t.child(ref_row(r, "docket-agent-now".into(), Some("works on now"), k));
    }
    for (g, ps) in list {
        t = t.child(ref_row(&crate::refs::ticket(Some(p), g), format!("docket-thread-{}", g.id), None, k));
        for v in ps {
            t = t.child(ref_row(&crate::refs::ticket(Some(p), v), format!("docket-thread-{}", v.id), None, k).pl(18.0));
        }
    }
    c.child(w).child(t)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn kind(text: &str) -> Kind {
        parse_kind(text).unwrap()
    }

    #[test]
    fn a_row_follows_the_arm_with_the_work_and_writes_its_case_on_the_arrow() {
        let q = kind("kind question\n  form ask: Text\n  result Answered(text: Text) | Dismissed(why: Text)\n  pattern\n    a <- step role lead answer(ask);\n    match a {\n      Answer(text) => return Answered(text)\n      Up(why) =>\n        o <- step owner reply(ask, why);\n        match o {\n          Answer(text) => return Answered(text)\n        }\n    }\n");
        let m = row_of(&q.pattern);
        assert_eq!(m, vec![(None, Item::Step { who: "role lead".into(), call: "answer".into(), hold: false }), (Some("Up".into()), Item::Step { who: "owner".into(), call: "reply".into(), hold: false })]);
    }

    #[test]
    fn a_wild_arm_writes_no_case_and_a_hole_ends_the_row() {
        let t = kind("kind t\n  form text: Text, addressed: Named | Unnamed\n  result Kept\n  pattern\n    v <- match addressed {\n      Named => return Thread\n      _ => step watcher intake(text)\n    };\n    match v {\n      Place(t) =>\n        return Kept\n      Thread =>\n        _ <- post text;\n        ...\n    }\n");
        let m = row_of(&t.pattern);
        assert_eq!(m, vec![(None, Item::Step { who: "watcher".into(), call: "intake".into(), hold: false }), (Some("Thread".into()), Item::Hole)]);
    }

    #[test]
    fn the_kinds_answer_gives_each_program_its_bindings_and_whose_it_is() {
        let f = |kind: &str, text: &str, by: Option<&str>, bindings: &[&str]| -> KindForm {
            serde_json::from_value(serde_json::json!({"kind": kind, "form": [], "text": text, "doc": format!("{kind}, as its doc says"), "by": by, "bindings": bindings})).unwrap()
        };
        let ks = vec![
            f("checkup", "kind checkup\n  form why: Text\n  result Text\n  pattern\n    r <- step role lead look(why);\n    return why\n", Some("paper-research"), &["every 15m open checkup(\"x\")"]),
            f("broken", "this is not a kind", None, &[]),
        ];
        let (prs, bad) = programs(&ks);
        assert_eq!(bad, vec!["broken".to_string()]);
        assert_eq!((prs[0].kd.name.as_str(), prs[0].doc.as_str(), prs[0].by.as_deref(), prs[0].into.len()), ("checkup", "checkup, as its doc says", Some("paper-research"), 1));
        assert_eq!((start_words(&prs[0].into[0]), prs[0].into[0].text.as_str()), ("every 15 m".to_string(), "every 15m open checkup(\"x\")"));
        // a coordd before the fields: none of them, and nothing starts it but an agent
        let old: KindForm = serde_json::from_value(serde_json::json!({"kind": "q", "form": [], "text": "kind q\n  form a: Text\n  result Text\n  pattern\n    return a\n"})).unwrap();
        assert_eq!((old.by, old.bindings.len(), old.file.as_str()), (None, 0, ""));
    }

    pub(crate) fn frames(rt: &mut rune::runtime::Runtime<App>) {
        for _ in 0..4 {
            rt.frame(0.016);
        }
    }

    pub(crate) fn press_at(rt: &mut rune::runtime::Runtime<App>, x: f32, y: f32) {
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        frames(rt);
    }

    pub(crate) fn press(rt: &mut rune::runtime::Runtime<App>, key: &str) {
        let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is drawn"));
        press_at(rt, x, y);
    }

    /// The Kinds page on a made-up project: the Docket over the cards, what starts each program (his message, an
    /// agent's clock with its next firing from the snapshot), the count at a step and whom each step goes to; a count
    /// pressed opens its ticket, a head pressed puts its card first under the board, a person pressed opens their part
    /// of the Docket (the Agent scale), and pressed again keeps it.
    #[test]
    fn the_kinds_page_draws_the_docket_and_each_press_goes_where_it_says() {
        use serde_json::json;
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        let agent = |name: &str, role: &str| -> coord_types::AgentView {
            serde_json::from_value(json!({"id": format!("AGT-{name}"), "name": name, "parent": "owner", "role": role, "kind": "claude", "model": "m",
                "state": "working", "since": "2026-11-20T08:00:00Z", "tools": [], "items": []})).unwrap()
        };
        p.live.agents = vec![agent("pr", "lead"), agent("qa", "")];
        let ticket = |n: u64, id: &str, kind: &str, status: serde_json::Value| -> TicketView {
            serde_json::from_value(json!({"id": id, "n": n, "kind": kind, "opener": "qa", "ctx": {}, "form": {}, "title": id, "status": status,
                "children": [], "program": "", "bound": 1, "spent": 0, "opened_at": "2026-11-20T08:00:00Z", "given": [], "posts": []})).unwrap()
        };
        let awaits = json!({"is": "awaits", "agent": "pr", "call": "answer", "step": 1, "since": "2026-11-20T08:00:00Z", "taken": true, "refusals": 0,
            "forming": false, "head": {"Variant": [["Answer", [["text", "Text"]]]]}, "args": {}});
        p.live.tickets.insert(5, ticket(5, "Q-5", "question", awaits));
        p.live.tickets.insert(7, ticket(7, "N-7", "thread", json!({"is": "resting"})));
        let clock = "every 15m open checkup(\"x\")";
        p.live.clocks = vec![coord_types::ClockView { binding: clock.into(), kind: "checkup".into(), every: Some(900), when: None, by: Some("pr".into()), file: "/p/kinds/agents/pr.kind".into(), line: 9, last: Some("2026-11-20T08:00:00Z".into()) }];
        // what starts each: the clock's words from the snapshot's clock, his message as the binding says it
        let (words, sub) = start_node(&p, &bindings_of(&[clock.to_string()])[0]);
        assert_eq!(words, "every 15 m");
        assert!(sub.as_deref().is_some_and(|s| s.starts_with("next at ")), "{sub:?}");
        let on = "on owner_message(m) open thread(m.text)";
        assert_eq!(start_node(&p, &bindings_of(&[on.to_string()])[0]), ("your message".to_string(), None));
        s.projects = vec![p];
        s.cur = 0;
        let f = |kind: &str, text: &str, by: Option<&str>, bindings: &[&str]| -> KindForm {
            serde_json::from_value(json!({"kind": kind, "form": [], "text": text, "doc": "", "by": by, "bindings": bindings})).unwrap()
        };
        s.tickets.kinds = Some(Ok(vec![
            f("question", "kind question\n  form ask: Text\n  result Answered(text: Text)\n  pattern\n    a <- step role lead answer(ask);\n    match a {\n      Answer(text) => return Answered(text)\n      Up(why) =>\n        o <- step owner reply(ask, why);\n        match o {\n          Answer(text) => return Answered(text)\n        }\n    }\n", None, &[]),
            // its handler on an agent's post as agent-base's thread had it until the typed ask, and a thread opened
            // before still has (testdata/thread-at-64.kind): the watcher's `asks`
            f("thread", "kind thread\n  form text: Text\n  result Kept\n  pattern\n    ...\n  when agent_post(p) where p.owner_on do\n    q <- step watcher asks(p.text, p.by);\n    match q {\n      Nothing => return none\n      Asks(marks) => mark p marks\n    }\n", None, &[on]),
            f("checkup", "kind checkup\n  form due: Text\n  result Text\n  pattern\n    r <- step pr look(due);\n    return due\n", Some("pr"), &[clock]),
            f("delivery", delivery_text(), None, &[]),
        ]));
        s.sel = crate::Sel::Kinds;
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (1800.0, 1100.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        for key in ["docket-board", "docket-start-thread-0", "docket-start-checkup-0", "docket-kind-question", "docket-kind-thread", "docket-kind-checkup", "docket-who-pr", "docket-who-qa", "docket-who-owner", "docket-count-question-5", "docket-kind-delivery", "docket-who-watcher"] {
            assert!(rt.find(key).is_some(), "{key} is drawn");
        }
        // the watcher is no agent here: its node stands only for the arrow from thread's `asks` (delivery asks no `next`
        // since I-1480 c)
        assert!(rt.find("docket-start-delivery-0").is_none(), "nothing on the left starts delivery: the engine runs it");
        // the program a step is in, before the cards
        assert!(rt.find("docket-board").unwrap().1 < rt.find("kind-question").unwrap().1);
        press(&mut rt, "docket-count-question-5");
        assert_eq!((rt.state.sel.clone(), rt.state.tickets.shown), (crate::Sel::Tickets, Some(5)), "the one ticket at that step, opened");
        rt.update(|s, _| s.sel = crate::Sel::Kinds);
        frames(&mut rt);
        press(&mut rt, "docket-kind-thread");
        assert!(rt.state.tickets.pic_open.contains("docket-kind:thread") && rt.state.kinds_open.contains("thread"));
        assert!(rt.find("kind-thread").unwrap().1 < rt.find("kind-question").unwrap().1, "the program picked is the first card under the board");
        press(&mut rt, "docket-who-pr");
        assert_eq!(shown_kind(&rt.state.tickets.pic_open), Some("thread"));
        assert_eq!((rt.state.sel.clone(), scale(&rt.state.tickets.pic_open), picked(&rt.state.tickets.pic_open, FOCUS)), (crate::Sel::Kinds, Scale::Agent, Some("pr")), "pr's part of the Docket");
        assert!(rt.find("docket-agent").is_some() && rt.find("docket-who-pr").is_some());
        press(&mut rt, "docket-who-pr");
        assert_eq!(picked(&rt.state.tickets.pic_open, FOCUS), Some("pr"), "pressed again: it stays");
    }

    /// agent-base's delivery.kind, from its `kind` line on, as the Kinds answer's text has it (picture/wire.rs's tests
    /// draw its card too).
    pub(crate) fn delivery_text() -> &'static str {
        const FILE: &str = include_str!("../../../../agent-base/kinds/delivery.kind");
        &FILE[FILE.find("kind delivery").expect("delivery.kind's kind line")..]
    }

    /// The message runtime's program as agent-base has it (consult-3, N-1396 p168): each handler a row, its acts drawn;
    /// it stands apart as a base does, said on its head. `free` first keeps what the owner's hold keeps (line 327's arm),
    /// then the watcher's `fold` decides what pushes, for an agent that worked while it came (I-1480 c; N-1446 p2092):
    /// the arm with the most stages, not the idle agent's `deliver` with no call nor the `keep` of `w where e.writing`,
    /// one stage each (proto-lead N-1481 p109). Of fold's answers, `Rest(why)` with the brain's lines to give is the
    /// longest: it keeps what fold rested and delivers those lines, the pulled as their digest, where Give, Fork and
    /// Failed deliver in one stage (N-1540 p23, mind-deliv76 8b7b06c3; docket-core N-1446 p2648). `arrived`'s arm with
    /// the most stages is his post while it works where a copy answers him (paper-research N-1446 p3020, 1): a copy of
    /// it is made, handed his post, and his post kept for the turn's end; a post while it works with no copy is kept for
    /// the turn's end, his too ("only the turn end", p15; his yes, N-1104 p923), one stage.
    #[test]
    fn the_delivery_program_stands_apart_with_a_row_for_each_handler() {
        let f: KindForm = serde_json::from_value(serde_json::json!({"kind": "delivery", "form": [], "text": delivery_text(), "doc": "", "bindings": []})).unwrap();
        let (prs, bad) = programs(&[f]);
        assert!(bad.is_empty(), "{bad:?}");
        let kd = &prs[0].kd;
        let rows: Vec<(String, Vec<(Option<String>, Item)>)> = kd.handlers.iter().map(|h| (event_words(&h.ev), row_of(&h.body))).collect();
        let act = |word| Item::Act { word };
        assert_eq!(rows, vec![
            ("when something comes for it while it works".to_string(), vec![(None, act("fork")), (None, act("deliver")), (None, act("keep"))]),
            ("when it is free and something waits".to_string(), vec![(None, act("keep")), (None, Item::Step { who: "watcher".into(), call: "fold".into(), hold: false }), (Some("Rest".into()), act("keep")), (None, act("deliver"))]),
            ("when something comes and it is stopped".to_string(), vec![(None, act("report"))]),
        ]);
        assert_eq!(apart(kd, &[]).as_deref(), Some("the engine runs one for each agent"));
        assert_eq!(apart(kd, &["thread".to_string()]).as_deref(), Some("its handlers are taken by thread"));
    }

    /// A row draws a match's main way: the arm with the most stages; of arms as long, one with no `where` (what the
    /// match does otherwise), not a guarded one before it, whatever the arms do; a guarded arm longer than the rest is
    /// drawn (proto-lead N-1481 p109). Made-up programs.
    #[test]
    fn a_row_draws_the_longest_arm_and_of_arms_as_long_the_one_with_no_guard() {
        let kind = |name: &str, arms: &str| -> KindForm {
            let text = format!("kind {name}\n  form ask: Text\n  result Text\n  pattern\n    match ask {{\n{arms}    }}\n");
            serde_json::from_value(serde_json::json!({"kind": name, "form": [], "text": text, "doc": "", "bindings": []})).unwrap()
        };
        let tie = kind("tie", "      a where a == \"now\" => return a\n      a where a == \"later\" =>\n        r <- step pr look(a);\n        return a\n      a =>\n        q <- step pr answer(a);\n        return a\n");
        let longer = kind("longer", "      a where a == \"both\" =>\n        r <- step pr look(a);\n        c <- step qa check(r);\n        return a\n      a =>\n        q <- step pr answer(a);\n        return a\n");
        let (prs, bad) = programs(&[tie, longer]);
        assert!(bad.is_empty(), "{bad:?}");
        let step = |who: &str, call: &str| (None, Item::Step { who: who.into(), call: call.into(), hold: false });
        let rows: Vec<Vec<(Option<String>, Item)>> = prs.iter().map(|p| row_of(&p.kd.pattern)).collect();
        assert_eq!(rows, vec![vec![step("pr", "answer")], vec![step("pr", "look"), step("qa", "check")]]);
    }

    /// Kinds file FILE's kind NAME, from its kind line to the next line at the margin (a binding), as the Kinds answer's
    /// text has it.
    fn kind_block(file: &'static str, name: &str) -> &'static str {
        let from = &file[file.find(&format!("\nkind {name}")).expect("its kind line") + 1..];
        let end = from.match_indices('\n').map(|(i, _)| i + 1).find(|&i| from[i..].starts_with(|c: char| !c.is_whitespace())).unwrap_or(from.len());
        &from[..end]
    }

    /// A rest's words on a card (consult-3, N-1396 p192): the minder's only where a handler on `hole`, the kind's own or
    /// its base's, writes what follows (the engine's rule); delivery's rest waits for the next event its handlers take.
    /// agent-base's base, thread and delivery, as the Kinds answer gives them, thread's card and delivery's open.
    #[test]
    fn a_rest_is_the_minder_s_only_where_a_hole_handler_writes_what_follows() {
        const BASE: &str = include_str!("../../../../agent-base/kinds/base.kind");
        const THREAD: &str = include_str!("../../../../agent-base/kinds/thread.kind");
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        s.projects = vec![p];
        s.cur = 0;
        let f = |kind: &str, text: &str| -> KindForm { serde_json::from_value(serde_json::json!({"kind": kind, "form": [], "text": text, "doc": "", "bindings": []})).unwrap() };
        s.tickets.kinds = Some(Ok(vec![f("base", kind_block(BASE, "base")), f("thread", kind_block(THREAD, "thread")), f("delivery", delivery_text())]));
        s.sel = crate::Sel::Kinds;
        s.kinds_open.insert("thread".into());
        s.kinds_open.insert("delivery".into());
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (1800.0, 5000.0), rune::Env::new(false, 1.0));
        for _ in 0..4 {
            rt.frame(0.016);
        }
        let els = rt.elements();
        // the texts drawn inside the element keyed CARD
        let on_card = |card: &str| -> Vec<String> {
            let under = |mut i: usize| loop {
                if els[i].key.as_deref() == Some(card) {
                    return true;
                }
                match els[i].parent {
                    Some(p) => i = p,
                    None => return false,
                }
            };
            assert!(els.iter().any(|e| e.key.as_deref() == Some(card)), "{card} is drawn");
            els.iter().enumerate().filter(|(i, e)| !e.exiting && under(*i)).filter_map(|(_, e)| e.text.clone()).collect()
        };
        let (thread, delivery) = (on_card("kind-thread"), on_card("kind-delivery"));
        assert!(thread.iter().any(|t| t == "waits for the minder"), "thread takes base's `hole`: {thread:?}");
        assert!(delivery.iter().any(|t| t == "rests here; each event is handled above"), "{delivery:?}");
        assert!(!delivery.iter().any(|t| t.contains("minder")), "no minder on delivery's card: {delivery:?}");
    }

    /// The way into the Docket (paper-research N-1396 p218; the owner, N-1104 p554: "how do i open the vidual layer"):
    /// the bar's `Docket`, from a thread, opens its program at the Thread scale and one press goes back to it; the
    /// thread's head's `Docket` does the same; from an agent's chat it opens at the Project's with that agent picked; the
    /// switch moves between the scales, and on the Docket the bar's press leaves the scale as it is. agent-base's thread.
    #[test]
    fn the_bar_s_docket_opens_at_the_scale_of_where_he_is_and_a_thread_s_goes_back_to_it() {
        use serde_json::json;
        const BASE: &str = include_str!("../../../../agent-base/kinds/base.kind");
        const THREAD: &str = include_str!("../../../../agent-base/kinds/thread.kind");
        fn at(rt: &rune::runtime::Runtime<App>) -> (crate::Sel, Scale) {
            (rt.state.sel.clone(), scale(&rt.state.tickets.pic_open))
        }
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        p.live.agents = vec![serde_json::from_value(json!({"id": "AGT-pr", "name": "pr", "parent": "owner", "role": "lead", "kind": "claude", "model": "m",
            "state": "working", "since": "2026-11-20T08:00:00Z", "tools": [], "items": []})).unwrap()];
        let thread: TicketView = serde_json::from_value(json!({"id": "N-7", "n": 7, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"text": "the runner's rows"},
            "title": "Runner rows", "status": {"is": "resting"}, "children": [], "program": kind_block(THREAD, "thread"), "bound": 1, "spent": 0,
            "opened_at": "2026-11-20T08:00:00Z", "given": [], "posts": [{"n": 1, "at": "2026-11-20T08:00:00Z", "by": "owner", "text": "the runner's rows", "names": ["pr"], "told": ["pr"]}]})).unwrap();
        p.live.tickets.insert(7, thread);
        s.projects = vec![p];
        s.cur = 0;
        let f = |kind: &str, text: &str| -> KindForm { serde_json::from_value(json!({"kind": kind, "form": [], "text": text, "doc": "", "bindings": []})).unwrap() };
        s.tickets.kinds = Some(Ok(vec![f("base", kind_block(BASE, "base")), f("thread", kind_block(THREAD, "thread"))]));
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = Some(7);
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (1400.0, 1000.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        // from the thread: its program, lit, and back
        press(&mut rt, "docket-mark");
        assert_eq!(at(&rt), (crate::Sel::Kinds, Scale::Thread), "a thread open: its own scale");
        for key in ["docket-scale", "docket-thread", "picture"] {
            assert!(rt.find(key).is_some(), "{key} is drawn");
        }
        assert!(rt.find("docket-board").is_none(), "the thread's program, not the board");
        press(&mut rt, "docket-back");
        assert_eq!((rt.state.sel.clone(), rt.state.tickets.shown), (crate::Sel::Tickets, Some(7)), "one press back to the thread");
        // the same from the thread's head, behind its ⋯ (paper-research N-1446 p3296)
        press(&mut rt, "thread-more");
        press(&mut rt, "thread-docket");
        assert_eq!(at(&rt), (crate::Sel::Kinds, Scale::Thread));
        assert_eq!(rt.state.tickets.shown, Some(7));
        // the switch: the project's board and its kinds
        press(&mut rt, "docket-scale/segment0");
        assert_eq!(at(&rt), (crate::Sel::Kinds, Scale::Project));
        assert!(rt.find("docket-board").is_some() && rt.find("kind-thread").is_some() && rt.find("docket-thread").is_none(), "the board and the cards");
        // on the Docket, the bar's press leaves it as it is
        press(&mut rt, "docket-scale/segment2");
        press(&mut rt, "docket-mark");
        assert_eq!(at(&rt), (crate::Sel::Kinds, Scale::Thread));
        // from an agent's chat: the project's board, that agent picked
        rt.update(|s, _| s.sel = crate::Sel::Agent("pr".into()));
        frames(&mut rt);
        press(&mut rt, "docket-mark");
        assert_eq!(at(&rt), (crate::Sel::Kinds, Scale::Project));
        assert_eq!(picked(&rt.state.tickets.pic_open, FOCUS), Some("pr"), "the agent of the chat he had open");
        press(&mut rt, "docket-scale/segment1");
        assert_eq!(at(&rt), (crate::Sel::Kinds, Scale::Agent));
        assert!(rt.find("docket-agent").is_some(), "the Agent scale is drawn");
        // no thread open: the Thread scale says so
        rt.update(|s, _| s.tickets.shown = None);
        press(&mut rt, "docket-scale/segment2");
        assert!(rt.elements().iter().any(|e| e.text.as_deref() == Some("Open a thread to see its program.")));
        // a thread with no program: it says so, and what Project draws, with the way back (consult-3 p222)
        let plain = TicketView { id: "N-8".into(), n: 8, program: String::new(), ..rt.state.projects[0].live.tickets[&7].clone() };
        rt.update(move |s, _| {
            s.projects[0].live.tickets.insert(8, plain);
            s.tickets.shown = Some(8);
        });
        frames(&mut rt);
        let no = "It runs no program.";
        assert!(rt.elements().iter().any(|e| e.text.as_deref() == Some(no)), "a thread with no program says so");
        press(&mut rt, "docket-back");
        assert_eq!((rt.state.sel.clone(), rt.state.tickets.shown), (crate::Sel::Tickets, Some(8)), "one press back to the thread with no program");
    }

    /// No dead press in a program's picture (paper-research N-1396 p223; the owner, N-1104 p558: "allow full exploration
    /// total parity and understanding"): on the Kinds page, with agent-base's question, fix and delivery cards open, every
    /// text drawn in each picture, pressed from the same page, opens a view: its lines lit in the text beside it, its
    /// call, the Agent scale, a ticket. Then what three presses open, by name: a step its call in its card and its line
    /// lit, a case its arm, a worker the Agent scale.
    #[test]
    fn every_thing_drawn_in_a_program_s_picture_opens_what_it_stands_for() {
        use crate::picture::{TEXT, lit_lines};
        const QUESTION: &str = include_str!("../../../../agent-base/kinds/question.kind");
        const FIX: &str = include_str!("../../../../agent-base/kinds/fix.kind");
        // what a press may open: the page and its scale, what is lit or open on it, the call or step shown, a ticket
        fn seen(s: &App) -> String {
            let mut open: Vec<&String> = s.tickets.pic_open.iter().collect();
            open.sort();
            let mut cards: Vec<&String> = s.kinds_open.iter().collect();
            cards.sort();
            format!("{:?} {open:?} {cards:?} {:?} {:?} {:?}", s.sel, s.tickets.edits.call, s.tickets.edits.step.as_ref().map(|st| st.key.clone()), s.tickets.shown)
        }
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        s.projects = vec![p];
        s.cur = 0;
        let f = |kind: &str, text: &str| -> KindForm { serde_json::from_value(serde_json::json!({"kind": kind, "form": [], "text": text, "doc": "", "bindings": []})).unwrap() };
        let (question, fix) = (kind_block(QUESTION, "question"), kind_block(FIX, "fix"));
        s.tickets.kinds = Some(Ok(vec![f("question", question), f("fix", fix), f("delivery", delivery_text())]));
        s.sel = crate::Sel::Kinds;
        for k in ["question", "fix", "delivery"] {
            s.kinds_open.insert(k.into());
        }
        // wide enough that no picture scrolls: each text is pressed where it is seen
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (4400.0, 7000.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        let start = (rt.state.tickets.pic_open.clone(), rt.state.kinds_open.clone());
        let back = |rt: &mut rune::runtime::Runtime<App>| {
            let (open, cards) = start.clone();
            rt.update(move |s, _| {
                s.sel = crate::Sel::Kinds;
                s.tickets.pic_open = open;
                s.kinds_open = cards;
                s.tickets.edits.call = None;
                s.tickets.edits.step = None;
                s.tickets.shown = None;
            });
            frames(rt);
        };
        for card in ["kind-question", "kind-fix", "kind-delivery"] {
            let pic = format!("pic-{card}");
            let els = rt.elements();
            let under = |mut i: usize| loop {
                if els[i].key.as_deref() == Some(pic.as_str()) {
                    return true;
                }
                match els[i].parent {
                    Some(p) => i = p,
                    None => return false,
                }
            };
            let spots: Vec<([f32; 4], String)> = els.iter().enumerate().filter(|(i, e)| !e.exiting && e.text.as_deref().is_some_and(|t| !t.trim().is_empty()) && under(*i)).map(|(_, e)| (e.visible, e.text.clone().unwrap_or_default())).collect();
            assert!(spots.len() >= 8, "{card}'s picture is drawn: {spots:?}");
            for (r, t) in spots {
                assert!(r[2] > 0.0 && r[3] > 0.0, "{card}: {t:?} is seen");
                let before = seen(&rt.state);
                press_at(&mut rt, r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
                assert_ne!(seen(&rt.state), before, "{card}: a press on {t:?} opens nothing");
                back(&mut rt);
            }
        }
        // the lead's step: its call, in its card, under its text and picture; its line lit beside it
        press(&mut rt, "pic-kind-question.0");
        assert_eq!(rt.state.tickets.edits.call.as_deref(), Some("answer"));
        let panel = rt.elements().iter().position(|e| e.key.as_deref() == Some("call-panel") && !e.exiting).expect("its call is shown");
        let els = rt.elements();
        let mut i = panel;
        while els[i].key.as_deref() != Some("kind-question") {
            i = els[i].parent.expect("the call panel is inside question's card");
        }
        assert_eq!(lit_lines(&rt.state.tickets.pic_open, "question", question), Some((5, 7)), "`a <- try {{ step role lead answer(ask, context) }} handle {{ … }};`");
        back(&mut rt);
        // a case: its arm, from its pattern to the end of its body
        press(&mut rt, "pic-kind-question.1.a2-case");
        assert_eq!(lit_lines(&rt.state.tickets.pic_open, "question", question), Some((11, 16)), "Up(why) => …, its match closed");
        assert!(rt.state.tickets.pic_open.iter().any(|k| k.starts_with(TEXT) && k.ends_with(":arm")));
        back(&mut rt);
        // a worker: the Agent scale, that worker picked
        press(&mut rt, "pic-kind-question.0.t.0-who");
        assert_eq!(scale(&rt.state.tickets.pic_open), Scale::Agent);
        assert!(picked(&rt.state.tickets.pic_open, FOCUS).is_some());
    }

    /// A step's call, pressed in its picture (paper-research N-1396 p174, p223, p235): as coordd read it, from the Kinds
    /// answer: its file and line, signature and task, its template as written, and the model it runs on. The watcher's
    /// `route`, in base's handler on his post: coordd runs `whom` for it, on its task's model (delivery's `next` was this
    /// example until I-1480 c took its step out, and thread's `asks` until the typed ask took its handler out); the
    /// lead's `answer`: on its agent's own, and which that is now.
    #[test]
    fn a_step_s_call_shows_its_file_signature_template_and_the_model_it_runs_on() {
        use serde_json::json;
        const QUESTION: &str = include_str!("../../../../agent-base/kinds/question.kind");
        const BASE: &str = include_str!("../../../../agent-base/kinds/base.kind");
        // the texts drawn in the call's panel
        fn panel(rt: &rune::runtime::Runtime<App>) -> Vec<String> {
            texts_under(rt, "call-panel")
        }
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        p.live.agents = vec![serde_json::from_value(json!({"id": "AGT-pr", "name": "pr", "parent": "owner", "role": "lead", "kind": "claude", "model": "claude-opus-5-5",
            "state": "working", "since": "2026-11-20T08:00:00Z", "tools": [], "items": []})).unwrap()];
        s.projects = vec![p];
        s.cur = 0;
        let route = json!({"name": "route", "file": "agent-base/calls/route.call", "line": 46, "sig": ROUTE_SIG, "attrs": [["task", "route"]], "template": null, "doc": "", "task": "route",
            "runs": {"name": "whom", "template": "The owner of a software team wrote the message below on a ticket that several agents take part in.\n{{ text }}"}, "model": {"model": "gpt-6.2-sol"}});
        let answer = json!({"name": "answer", "file": "agent-base/calls/question.call", "line": 2, "sig": "call answer(ask: Text, context: Text?) -> Answer(text: Text) | Dismiss(why: Text) | Up(why: Text)",
            "attrs": [["task", "answer"]], "template": "Answer {{ ask }}.", "doc": "", "task": "answer"});
        let f = |kind: &str, text: &str, call: &serde_json::Value| -> KindForm { serde_json::from_value(json!({"kind": kind, "form": [], "text": text, "doc": "", "bindings": [], "calls": [call]})).unwrap() };
        s.tickets.kinds = Some(Ok(vec![f("base", kind_block(BASE, "base"), &route), f("question", kind_block(QUESTION, "question"), &answer)]));
        s.sel = crate::Sel::Kinds;
        for k in ["base", "question"] {
            s.kinds_open.insert(k.into());
        }
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (4400.0, 7000.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        // the watcher's step: `route`, in base's handler on his post
        let step = route_step(&rt);
        press(&mut rt, &step);
        assert_eq!(rt.state.tickets.edits.call.as_deref(), Some("route"));
        let t = panel(&rt);
        for want in ["call route", ROUTE_SIG, "task route · gpt-6.2-sol", "Runs whom, with its template:", "The owner of a software team wrote the message below on a ticket that several agents take part in.\n{{ text }}"] {
            assert!(t.iter().any(|x| x == want), "{want:?} in {t:?}");
        }
        assert!(t.iter().any(|x| x == "agent-base/calls/route.call, line 46"), "route's head names its file and line, and no prompt of its own: {t:?}");
        // its task once, on the model line; its prompt is edited through the call coordd runs for it (p294)
        assert!(!t.iter().any(|x| x == "task route"), "route's panel says its task once: {t:?}");
        assert!(t.iter().any(|x| x == "Edit the template") && t.iter().any(|x| x == "Every step from now on"), "route's prompt is whom's, edited from here: {t:?}");
        // the lead's step: its agent's own model, and which that is now
        press(&mut rt, "pic-kind-question.0");
        assert_eq!(rt.state.tickets.edits.call.as_deref(), Some("answer"));
        let t = panel(&rt);
        for want in ["the agent's own model · pr: Opus 5.5 now", "Answer {{ ask }}.", "task answer", "Edit the template"] {
            assert!(t.iter().any(|x| x == want), "{want:?} in {t:?}");
        }
        assert!(t.iter().any(|x| x.ends_with(" · the prompt of every step of answer")), "answer's head names its prompt: {t:?}");
    }

    /// A call coordd runs another for, in the call panel (consult-3 N-1446 p1182, proto-lead p1183): coordd runs it only
    /// when the watcher takes the step (edit.rs `runs_for`), so for the watcher's step the panel says so, with that
    /// call's template, and its Edit opens that call's lines; for an agent's step it says the agent answers it, with no
    /// Edit and nothing of that call. brain.call's `attend`, which coordd runs `attention` for, as the Kinds answer says
    /// it (made up but for its names and agent-base's brain.kind).
    #[test]
    fn a_call_coordd_runs_another_for_is_that_calls_only_on_the_watchers_step() {
        crate::elsewhere::files_here();
        use serde_json::json;
        const BRAIN: &str = include_str!("../../../../agent-base/kinds/brain.kind");
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        s.projects = vec![p];
        s.cur = 0;
        // agent-base's brain.call in the base folder: attention's lines, which its Edit opens
        let dir = live_folders(&mut s, "call-runs");
        std::fs::write(dir.join("base").join("calls").join("brain.call"), include_str!("../../../../agent-base/calls/brain.call")).unwrap();
        let attend = json!({"name": "attend", "file": "agent-base/calls/brain.call", "line": 43, "sig": "call attend(agent: Agent, posts: [Text])", "attrs": [["task", "attend"]],
            "template": null, "doc": "", "task": "attend", "runs": {"name": "attention", "template": "You are one part of {{ agent }}'s mind."}});
        s.tickets.kinds = Some(Ok(vec![serde_json::from_value(json!({"kind": "brain", "form": [], "text": kind_block(BRAIN, "brain"), "doc": "", "bindings": [], "calls": [attend]})).unwrap()]));
        s.sel = crate::Sel::Kinds;
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (4400.0, 7000.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        // attend's call, for a step taken by BY, as a step pressed in its picture opens it
        let open = |rt: &mut rune::runtime::Runtime<App>, by: &str| {
            rt.update(|s: &mut App, _| {
                s.tickets.edits.call = None;
                open_call(s, "brain", "attend", by, None);
            });
            frames(rt);
            texts_under(rt, "call-panel")
        };
        // the watcher's step: coordd runs attention for it, with its template; its Edit opens attention's lines
        let t = open(&mut rt, docket_machine::WATCHER);
        assert!(t.iter().any(|x| x == "Runs attention, with its template:") && t.iter().any(|x| x == "You are one part of {{ agent }}'s mind."), "{t:?}");
        assert!(t.iter().any(|x| x == "Edit the template"), "{t:?}");
        press(&mut rt, "call-edit");
        assert_eq!(rt.state.tickets.edits.call_edit.as_ref().map(|e| e.call.as_str()), Some("attention"), "its Edit edits the call coordd runs");
        // pr's step: pr answers it; no Edit, nothing of attention
        let t = open(&mut rt, "pr");
        assert!(t.iter().any(|x| x == "call attend") && t.iter().any(|x| x == "(no template: answered by pr)"), "{t:?}");
        assert!(rt.find("call-edit").is_none() && !t.iter().any(|x| x == "Edit the template"), "an agent's step: no Edit: {t:?}");
        assert!(!t.iter().any(|x| x.starts_with("coordd runs") || x.contains("attention") || x.contains("You are one part")), "an agent's step: nothing of attention: {t:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An edit of a call coordd runs for another (`attention`, which it runs for `attend`) marks a step of that other
    /// only where coordd runs it, on the watcher's step: by `runs_for` with the step's taker, as the call panel reads it
    /// (consult-3 N-1446 p1231, p1233; proto-lead p1232). While `attention` is edited the watcher's `attend` says
    /// "editing" and an agent's doesn't; after its Save, from the reload's answer and from the record (the Kinds answer's
    /// `runs.edited`), the watcher's says when it was edited and the agent's says nothing. A made-up kind with one
    /// `attend` of each, brain.call's `attend` as the Kinds answer says it (made up but for its names).
    #[test]
    fn an_edit_of_a_call_coordd_runs_marks_only_the_watchers_steps_of_the_call_it_runs_for() {
        crate::elsewhere::files_here();
        use serde_json::json;
        const DUO: &str = "kind duo\n  form agent: Agent, posts: [Text]\n  pattern\n    r <- step watcher attend(agent, posts);\n    t <- step agent attend(agent, posts);\n    return ()\n";
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        s.projects = vec![p];
        s.cur = 0;
        // agent-base's brain.call in the base folder: attention's lines, which the watcher's step's Edit opens
        let dir = live_folders(&mut s, "runs-marks");
        std::fs::write(dir.join("base").join("calls").join("brain.call"), include_str!("../../../../agent-base/calls/brain.call")).unwrap();
        let attend = json!({"name": "attend", "file": "agent-base/calls/brain.call", "line": 43, "sig": "call attend(agent: Agent, posts: [Text])", "attrs": [["task", "attend"]],
            "template": null, "doc": "", "task": "attend", "runs": {"name": "attention", "template": "You are one part of {{ agent }}'s mind."}});
        s.tickets.kinds = Some(Ok(vec![serde_json::from_value(json!({"kind": "duo", "form": [], "text": DUO, "doc": "", "bindings": [], "calls": [attend]})).unwrap()]));
        s.kinds_open.insert("duo".to_string());
        s.sel = crate::Sel::Kinds;
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (4400.0, 7000.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        // the two `attend` steps, each by the words of who takes it
        let steps: Vec<(String, String)> = rt.elements().iter().filter_map(|e| Some((e.key.as_deref()?.strip_suffix("-who")?.strip_prefix("pic-kind-duo")?.to_string(), e.text.clone()?))).collect();
        let step = |by: &str| format!("pic-kind-duo{}", steps.iter().find(|(_, w)| w == by).unwrap_or_else(|| panic!("{by}'s attend in {steps:?}")).0);
        let (watcher, agent) = (step(docket_machine::WATCHER), step("agent"));
        assert_ne!(watcher, agent);
        let marked = |rt: &rune::runtime::Runtime<App>, mark: &str| [&watcher, &agent].map(|k| rt.find(&format!("{k}-{mark}")).is_some());
        // attention edited, from the watcher's step, whose Edit opens attention's lines
        rt.update(|s: &mut App, _| open_call(s, "duo", "attend", docket_machine::WATCHER, None));
        frames(&mut rt);
        press(&mut rt, "call-edit");
        assert_eq!(rt.state.tickets.edits.call_edit.as_ref().map(|e| e.call.as_str()), Some("attention"));
        frames(&mut rt);
        assert_eq!(marked(&rt, "editing"), [true, false], "while attention is edited: the watcher's attend, not the agent's");
        // after its Save: the reload's answer names attention
        rt.update(|s: &mut App, _| {
            s.tickets.edits.call_edit = None;
            crate::edit::replied(s, true, true, r#"{"kinds":[],"calls":["attention"],"tickets":[]}"#);
        });
        frames(&mut rt);
        assert_eq!(marked(&rt, "editing"), [false, false]);
        assert_eq!(marked(&rt, "edited"), [true, false], "after its Save, by the reload: the watcher's attend says when, not the agent's");
        // from the record: the Kinds answer's `edited` of the call coordd runs for attend
        rt.update(|s: &mut App, _| {
            s.tickets.edits.reloaded = None;
            if let Some(Ok(ks)) = &mut s.tickets.kinds {
                ks[0].calls[0].runs.as_mut().unwrap().edited = Some(crate::kinds::Edited { at: "2026-11-20T09:30:00Z".into(), by: "owner".into(), why: "shorter".into(), commit: None });
            }
        });
        frames(&mut rt);
        assert_eq!(marked(&rt, "edited"), [true, false], "by the record: the watcher's attend says when, not the agent's");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// base.kind's `route`, as written in agent-base/calls/route.call.
    const ROUTE_SIG: &str = "call route(text: Text, to: [Agent], others: [Agent]) -> To(agents: [Agent], joins: Int?) | Split(parts: [Part]) | Unclear(why: Text)";

    /// The key of base's `route` step on its card (the watcher's, in its handler on his post), as drawn in RT.
    fn route_step(rt: &rune::runtime::Runtime<App>) -> String {
        let keys: Vec<String> = rt.elements().iter().filter_map(|e| e.key.clone()).collect();
        keys.iter()
            .filter(|k| k.starts_with("pic-kind-base-when-owner_post") && k.ends_with("-who"))
            .map(|k| k.trim_end_matches("-who").to_string())
            .find(|k| texts_under(rt, k).iter().any(|x| x == "route"))
            .unwrap_or_else(|| panic!("base's handler on his post has its route step: {:?}", keys.iter().filter(|k| k.starts_with("pic-kind-base")).collect::<Vec<_>>()))
    }

    /// The texts drawn under the element KEY.
    pub(crate) fn texts_under(rt: &rune::runtime::Runtime<App>, key: &str) -> Vec<String> {
        let els = rt.elements();
        let under = |mut i: usize| loop {
            if els[i].key.as_deref() == Some(key) {
                return true;
            }
            match els[i].parent {
                Some(p) => i = p,
                None => return false,
            }
        };
        els.iter().enumerate().filter(|(i, e)| !e.exiting && under(*i)).filter_map(|(_, e)| e.text.clone()).collect()
    }

    /// agent-base's files the live edit's tests read (`live_app`), as the repository has them.
    const LIVE_BASE: [(&str, &str); 8] = [
        ("kinds/base.kind", include_str!("../../../../agent-base/kinds/base.kind")),
        ("kinds/question.kind", include_str!("../../../../agent-base/kinds/question.kind")),
        ("kinds/delivery.kind", include_str!("../../../../agent-base/kinds/delivery.kind")),
        ("kinds/thread.kind", include_str!("../../../../agent-base/kinds/thread.kind")),
        ("calls/question.call", include_str!("../../../../agent-base/calls/question.call")),
        ("calls/next.call", include_str!("../../../../agent-base/calls/next.call")),
        ("calls/fold.call", include_str!("../../../../agent-base/calls/fold.call")),
        ("calls/route.call", include_str!("../../../../agent-base/calls/route.call")),
    ];

    /// `still_app` reading agent-base's kinds and calls (`LIVE_BASE`, written into a folder of its own NAME), its project's
    /// folder an empty one beside it, where a Save writes (p294). Gives the app and that folder.
    fn live_app(name: &str) -> (App, std::path::PathBuf) {
        let mut s = still_app();
        let dir = live_folders(&mut s, name);
        (s, dir)
    }

    /// S's project given LIVE_BASE's kinds and calls as agent-base's, and empty folders of its own, in a folder NAME of
    /// the system's temp (for the Map's steps too, map/deep.rs).
    pub(crate) fn live_folders(s: &mut App, name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("yaaihh-live-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (f, text) in LIVE_BASE {
            let f = dir.join("base").join(f);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, text).unwrap();
        }
        for sub in ["kinds", "calls"] {
            std::fs::create_dir_all(dir.join("proj").join(sub)).unwrap();
        }
        s.base = dir.join("base").display().to_string();
        s.projects[0].live.places = Some(coord_types::Places { coord: "/c".into(), definition: Some(dir.join("proj").display().to_string()), ..Default::default() });
        dir
    }

    /// A step's prompt edited in place, live (paper-research N-1396 p294): the step pressed opens its call, its Edit puts
    /// the call's lines after its signature in the box there, and every step of it in the picture is marked. Before
    /// Save, coordd's reach of the file Save would write shows each running ticket by its case, and a refusal in its
    /// words holds Save; Save writes the project's own copy of agent-base's call, and the step then says when it was
    /// edited, from the reload's answer and from the record (the Kinds answer's `edited`). The watcher's `route`, in
    /// base's handler on his post: its prompt is that of `whom`, the call coordd runs for it (delivery's `next` was this
    /// example until I-1480 c took its step out, and thread's `asks` until the typed ask took its handler out).
    #[test]
    fn a_steps_prompt_is_edited_in_place_and_says_what_it_reaches_before_save_and_when_after() {
        crate::elsewhere::files_here();
        let (s, dir) = live_app("prompt");
        // joined part by part, as the window joins it: one separator on Windows too
        let copy_at = dir.join("proj").join("calls").join("answer.call");
        let mut s = pressed(s, &["docket-mark", "docket-scale/segment0"]);
        s.kinds_open.extend(["question".to_string(), "base".to_string()]);
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (4400.0, 7000.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        press(&mut rt, "pic-kind-question.0");
        press(&mut rt, "call-edit");
        let e = rt.state.tickets.edits.call_edit.as_ref().unwrap_or_else(|| panic!("answer's lines open in place: {:?}", rt.state.tickets.edits.said));
        assert_eq!(e.call, "answer");
        assert!(e.text.text.starts_with("task answer\ntemplate\n  A question for you:\n"), "its lines after its signature, as written: {:?}", e.text.text);
        let text = e.text.text.replacen("Answer it;", "Answer it, naming the file you read;", 1);
        assert!(rt.find("call-editor").is_some() && rt.find("pic-kind-question.0.t.0-editing").is_some(), "edited in place, the step marked");
        assert!(texts_under(&rt, "edit-reach").iter().any(|x| x == "Pause to see what it reaches."), "a coordd of REACH_PROTOCOL is asked");
        rt.update(|s: &mut App, _| s.projects[0].protocol = coord_types::REACH_PROTOCOL - 1);
        frames(&mut rt);
        assert_eq!(texts_under(&rt, "edit-reach"), vec!["Needs a newer engine.".to_string()], "an older coordd is asked nothing");
        rt.update(|s: &mut App, _| s.projects[0].protocol = coord_types::PROTOCOL);
        frames(&mut rt);
        let to = format!("Editing call answer in place: saved as the project's own copy, {}", copy_at.display());
        assert!(texts_under(&rt, "call-editor").contains(&to), "it says where Save writes: {to}");
        let means = format!("research now runs its own answer.call ({}), not agent-base's.", copy_at.display());
        assert_eq!(texts_under(&rt, "edit-copy"), vec![means.clone()], "what the copy means, and the way back (consult-3 p334)");
        rt.update(move |s: &mut App, _| s.tickets.edits.call_edit.as_mut().unwrap().text = rune::TextEdit::new(text));
        frames(&mut rt);
        // what a Reach asks of: the file Save writes, the project's own copy of agent-base's call, its lines only
        let (file, written) = crate::edit::reach_of(&rt.state).expect("the edit checks, and its file is asked of coordd");
        assert_eq!(file, copy_at);
        assert!(written.starts_with("call answer(") && written.contains("naming the file you read") && !written.contains("call reply("), "{written}");
        // coordd's answer (docket-edge5's reach, p305, p307): each running ticket by its case, with coordd's words
        let reply = serde_json::json!({"kinds": [], "calls": ["answer"], "clocks": [], "tickets": [
            {"id": "Q-5", "kind": "question", "reach": "ahead", "said": "its answer step is ahead: it is sent with the new words"},
            {"id": "Q-6", "kind": "question", "reach": "given", "said": "its answer step was sent at 08:10 with the old words; a reminder reads the new ones"},
            {"id": "N-7", "kind": "thread", "reach": "own", "said": "its own prompt stands (only this thread, at 09:00)"},
            {"id": "N-9", "kind": "thread", "reach": "handing", "said": "being handed since 08:12: it goes with the text its rendering read"}]}).to_string();
        let w = written.clone();
        rt.update(move |s: &mut App, _| crate::edit::came(s, 0, crate::edit::Asked::Reach { text: w }, true, reply));
        frames(&mut rt);
        let t = texts_under(&rt, "edit-reach");
        // a case this window does not know (none today: a step being handed is `given`, docket-edge5 p316): under
        // coordd's word, the ticket not left out
        for want in ["Reaches · 1", "Given now · 1", "Your own prompt stands · 1", "handing · 1", "its answer step is ahead: it is sent with the new words", "its own prompt stands (only this thread, at 09:00)", "being handed since 08:12: it goes with the text its rendering read"] {
            assert!(t.iter().any(|x| x == want), "{want:?} in {t:?}");
        }
        for id in ["Q-5", "Q-6", "N-7"] {
            assert!(rt.find(&format!("edit-reach-{id}")).is_some(), "{id}, label · id, in the reach");
        }
        assert!(rt.find("edit-reach-stale").is_none(), "the answer is for the text as it is");
        // a key after the answer: the list is for the text before it, and says so until this text is answered (p334)
        rt.update(|s: &mut App, _| {
            let e = s.tickets.edits.call_edit.as_mut().unwrap();
            e.text = rune::TextEdit::new(e.text.text.replacen("saying why.", "saying why briefly.", 1));
        });
        frames(&mut rt);
        assert!(crate::edit::reach_of(&rt.state).is_some_and(|(_, t)| t != written), "the new text checks");
        assert!(texts_under(&rt, "edit-reach-stale").iter().any(|x| x.starts_with("Before your last change; ")), "{:?}", texts_under(&rt, "edit-reach"));
        rt.update(|s: &mut App, _| {
            let e = s.tickets.edits.call_edit.as_mut().unwrap();
            e.text = rune::TextEdit::new(e.text.text.replacen("saying why briefly.", "saying why.", 1));
        });
        frames(&mut rt);
        assert!(rt.find("edit-reach-stale").is_none(), "the answer is again for the text as it is");
        // a refusal of this text, in coordd's words, holds Save
        let w = written.clone();
        rt.update(move |s: &mut App, _| crate::edit::came(s, 0, crate::edit::Asked::Reach { text: w }, false, "nothing is read again, the kinds and calls stay as they were: the call answer changed".into()));
        rt.update(|s: &mut App, _| s.tickets.edits.why = rune::TextEdit::new("name the file"));
        frames(&mut rt);
        assert!(texts_under(&rt, "edit-reach").iter().any(|x| x == "Would be refused: nothing is read again, the kinds and calls stay as they were: the call answer changed"));
        press(&mut rt, "edit-call-save");
        assert!(rt.state.tickets.edits.call_edit.is_some() && !copy_at.exists(), "Save waits while coordd refuses this text");
        // Save for the program: the project's own copy; the reload's answer marks the step with when
        rt.update(|s: &mut App, _| s.tickets.edits.reach = None);
        frames(&mut rt);
        press(&mut rt, "edit-call-save");
        frames(&mut rt);
        let copy = std::fs::read_to_string(&copy_at).unwrap_or_else(|e| panic!("the copy is written ({e}): {:?}", rt.state.tickets.edits.said));
        assert!(copy.starts_with("call answer(") && copy.contains("naming the file you read") && copy.ends_with("pass it up, saying why.\n"), "{copy}");
        assert!(matches!(&rt.state.tickets.edits.said, Some(Ok(t)) | Some(Err(t)) if t.contains(&means)), "Save says it too: {:?}", rt.state.tickets.edits.said);
        assert!(rt.state.tickets.edits.call_edit.is_none() && rt.find("pic-kind-question.0.t.0-editing").is_none());
        rt.update(|s: &mut App, _| {
            s.tickets.kinds_asked = true;
            crate::edit::replied(s, true, true, r#"{"kinds":[],"calls":["answer"],"tickets":[]}"#);
        });
        frames(&mut rt);
        assert!(rt.find("pic-kind-question.0.t.0-edited").is_some(), "after Save the step says when it was edited");
        assert!(!rt.state.tickets.kinds_asked, "the kinds and calls as coordd now reads them are asked for again");
        // from the record: the Kinds answer's `edited` of the call coordd runs for `route`, on route's step
        rt.update(|s: &mut App, _| {
            if let Some(Ok(ks)) = &mut s.tickets.kinds {
                let b = ks.iter_mut().find(|k| k.kind == "base").unwrap();
                b.calls[0].runs.as_mut().unwrap().edited = Some(crate::kinds::Edited { at: "2026-11-20T09:30:00Z".into(), by: "owner".into(), why: "shorter".into(), commit: Some("abc1234".into()) });
            }
        });
        frames(&mut rt);
        let step = route_step(&rt);
        assert!(rt.find(&format!("{step}-edited")).is_some(), "route's step says when whom was edited");
        // the watcher's `route`: its Edit opens whom's lines, and route's step is marked
        press(&mut rt, &step);
        press(&mut rt, "call-edit");
        let e = rt.state.tickets.edits.call_edit.as_ref().unwrap_or_else(|| panic!("whom's lines open: {:?}", rt.state.tickets.edits.said));
        assert!(e.call == "whom" && e.text.text.starts_with("task route\ntemplate\n  The owner of a software team wrote the message below on a ticket that several agents take part in."), "{:?}", e.text.text);
        assert!(texts_under(&rt, "call-panel").iter().any(|x| x == "call route runs whom; its lines:"));
        assert!(rt.find(&format!("{step}-editing")).is_some(), "route's step is marked: its prompt is whom's");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A case pressed is edited in place (p294): its lines, lit in its kind's text, go into a box there, the rest of the
    /// text as it is around it and the case marked in the picture; agent-base's kind is saved as the project's own copy.
    #[test]
    fn a_case_pressed_is_edited_in_place_and_saved_as_the_projects_own_copy() {
        crate::elsewhere::files_here();
        let (s, dir) = live_app("case");
        let mut s = pressed(s, &["docket-mark", "docket-scale/segment0"]);
        s.kinds_open.insert("question".to_string());
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (4400.0, 7000.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        press(&mut rt, "pic-kind-question.1.a2-case");
        press(&mut rt, "edit-lines");
        let e = rt.state.tickets.edits.kind.as_ref().unwrap_or_else(|| panic!("the case's lines open in place: {:?}", rt.state.tickets.edits.said));
        assert_eq!((e.new, e.file.clone()), (true, dir.join("proj").join("kinds").join("question.kind")), "agent-base's kind: the project's own copy");
        assert!(e.text.text.trim_start().starts_with("Up(why) =>") && e.text.text.contains("o <- step owner reply"), "the box holds the case: {:?}", e.text.text);
        let (a, _) = e.lines.expect("only the lines pressed");
        assert!(crate::edit::marks(&rt.state.tickets).editing("question", a), "the case is marked in the picture");
        assert!(rt.find("edit-kind-question").is_some() && rt.find("kind-bar").is_some());
        let means = format!("research now runs its own question.kind ({}), not agent-base's.", dir.join("proj").join("kinds").join("question.kind").display());
        assert_eq!(texts_under(&rt, "edit-copy"), vec![means.clone()], "what the copy means, and the way back (consult-3 p334)");
        assert!(texts_under(&rt, "edit-lines-question").iter().any(|x| x.starts_with("kind question\n")), "the kind's lines before the box, as they are");
        let text = e.text.text.replace("o <- step owner", "his <- step owner").replace("match o {", "match his {");
        rt.update(move |s: &mut App, _| {
            s.tickets.edits.kind.as_mut().unwrap().text = rune::TextEdit::new(text);
            s.tickets.edits.why = rune::TextEdit::new("his answer, by name");
        });
        frames(&mut rt);
        assert!(rt.state.tickets.edits.kind.as_ref().unwrap().check().is_empty(), "{:?}", rt.state.tickets.edits.kind.as_ref().unwrap().check());
        press(&mut rt, "kind-save");
        let copy = std::fs::read_to_string(dir.join("proj").join("kinds").join("question.kind")).unwrap_or_else(|e| panic!("the copy is written ({e}): {:?}", rt.state.tickets.edits.said));
        assert!(copy.starts_with("kind question\n") && copy.contains("his <- step owner reply") && copy.contains("match his {"), "{copy}");
        assert!(rt.state.tickets.edits.kind.is_none());
        assert!(matches!(&rt.state.tickets.edits.said, Some(Ok(t)) | Some(Err(t)) if t.contains(&means)), "Save says it too: {:?}", rt.state.tickets.edits.said);
        rt.update(|s: &mut App, _| crate::edit::replied(s, true, true, r#"{"kinds":["question"],"calls":[],"tickets":[]}"#));
        frames(&mut rt);
        assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("kind-question-edited") && e.text.as_deref().is_some_and(|t| t.starts_with("edited "))), "the card says when it was edited");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// On a ticket's step (its Thread scale), one Save with two choices (paper-research N-1396 p296): for the program, the
    /// default (the call's lines, as on Project, asked of coordd's reach), or only this thread (the owner's `Template`
    /// with scope `tree`, no file written).
    #[test]
    fn on_a_tickets_step_save_is_for_the_program_or_only_this_thread() {
        crate::elsewhere::files_here();
        let (mut s, dir) = live_app("thread");
        s.tickets.shown = Some(5);
        s.tickets.edits.step = Some(crate::edit::Step { t: 5, key: "pic-Q-5.0".into(), worker: "pr".into(), call: "answer".into(), inline: None, awaited: false });
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (4400.0, 7000.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        assert!(rt.find("step-for-program").is_some() && rt.find("step-for-thread").is_some() && !rt.state.tickets.edits.only_thread, "one control, the program first");
        press(&mut rt, "step-edit-program");
        assert_eq!(rt.state.tickets.edits.call_edit.as_ref().map(|e| e.call.as_str()), Some("answer"), "for the program: the call's lines, as on Project");
        assert!(rt.find("call-editor").is_some() && crate::edit::reach_of(&rt.state).is_some());
        press(&mut rt, "step-for-thread");
        assert!(rt.state.tickets.edits.only_thread && rt.find("step-save-thread").is_some());
        assert!(crate::edit::reach_of(&rt.state).is_none(), "only this thread writes no file");
        rt.update(|s: &mut App, _| s.tickets.edits.why = rune::TextEdit::new("for this one"));
        let r = crate::edit::once_request(&rt.state, "Q-5", "answer", true).unwrap();
        assert!(matches!(r, coord_types::Request::Template { ref t, ref scope, .. } if t == "Q-5" && scope == "tree"), "{r:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The prompt as it would be sent, before Save (H2, his "we can dry run it ??", N-216.p1; docket-edge8 N-1446
    /// p1824): under the prompt being edited, for the program as for only this thread, "Show it as it would be sent";
    /// nothing is asked or shown before his press. Pressed, the dry run asks of the file Save would write (for the
    /// program) or of the box's text (only this thread), on the ticket whose step it is; coordd's answer shows the
    /// step's text and the step it was filled from. His own prompt for the step, the newest ticket in place of the one
    /// asked, none waiting, a refusal and an older coordd each say so; an answer for an earlier text is faint; Hide
    /// drops it, and so does the edit's end.
    #[test]
    fn a_prompt_being_edited_shows_as_it_would_be_sent_behind_his_press() {
        use coord_types::DryRunAnswer;
        crate::elsewhere::files_here();
        let (mut s, dir) = live_app("dry");
        s.tickets.shown = Some(5);
        s.tickets.edits.step = Some(crate::edit::Step { t: 5, key: "pic-Q-5.0".into(), worker: "pr".into(), call: "answer".into(), inline: None, awaited: false });
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (4400.0, 7000.0), rune::Env::new(false, 1.0));
        // the frame's polls, the dry run's among them (`edit::dry_poll`)
        rt.poll = Some(crate::poll);
        frames(&mut rt);
        press(&mut rt, "step-edit-program");
        frames(&mut rt);
        assert!(rt.find("edit-dry-show").is_some(), "the press under the editor");
        assert!(crate::edit::dry_of(&rt.state).is_none() && rt.find("edit-dry-text").is_none(), "nothing asked or shown before his press");
        press(&mut rt, "edit-dry-show");
        frames(&mut rt);
        // for the program: the file Save writes and its text, as Reach asks, on the ticket whose step this is
        let (file, written) = crate::edit::reach_of(&rt.state).expect("the edit checks");
        let ask = crate::edit::dry_of(&rt.state).expect("asked once pressed");
        // the path as coordd is sent it, with `/` on Windows too
        assert_eq!(ask, crate::edit::DryAsk { call: "answer".into(), on: Some("Q-5".into()), path: Some(file.to_string_lossy().replace('\\', "/")), text: written });
        assert!(texts_under(&rt, "edit-dry").iter().any(|x| x == "Pause to preview."), "{:?}", texts_under(&rt, "edit-dry"));
        let came = |rt: &mut rune::runtime::Runtime<App>, ask: &crate::edit::DryAsk, ok: bool, out: String| {
            let ask = ask.clone();
            rt.update(move |s: &mut App, _| crate::edit::came(s, 0, crate::edit::Asked::DryRun { ask }, ok, out));
            frames(rt);
        };
        let answer = |a: DryRunAnswer| serde_json::to_string(&a).unwrap();
        let sent = "A question for you:\nWhich file holds the look's colours?\n\nAnswer it; or dismiss it, saying why; or, if only the owner can answer it, pass it up, saying why.\n\nThis step gives one of:\n  Answer\n    text\n  Dismiss\n    why\n  Up\n    why".to_string();
        let q5 = DryRunAnswer { on: Some("Q-5".into()), step: 2, by: "pr".into(), asked: true, own: false, text: sent.clone() };
        came(&mut rt, &ask, true, answer(q5.clone()));
        let t = texts_under(&rt, "edit-dry");
        for want in ["Preview · with Q-5's step 2 arguments, which pr holds now", sent.as_str(), "Posts pr hasn't read go with it."] {
            assert!(t.iter().any(|x| x == want), "{want:?} in {t:?}");
        }
        assert!(rt.find("edit-dry-own").is_none() && rt.find("edit-dry-stale").is_none(), "{t:?}");
        // the newest ticket waiting, when the one asked waits on none; his own prompt for that step, which stands
        came(&mut rt, &ask, true, answer(DryRunAnswer { on: Some("Q-6".into()), step: 1, by: "owner".into(), asked: false, own: true, text: "His own words".into() }));
        let t = texts_under(&rt, "edit-dry");
        for want in ["Preview · with Q-6's step 1 arguments", "Q-6 uses your prompt for this step.", "Posts you haven't read go with it."] {
            assert!(t.iter().any(|x| x == want), "{want:?} in {t:?}");
        }
        // none waiting; a refusal, in coordd's words
        came(&mut rt, &ask, true, answer(DryRunAnswer::default()));
        assert!(texts_under(&rt, "edit-dry").iter().any(|x| x == "No ticket waits on answer yet."), "{:?}", texts_under(&rt, "edit-dry"));
        assert!(rt.find("edit-dry-text").is_none());
        came(&mut rt, &ask, false, "the calls do not load: question.call: line 3".into());
        assert!(texts_under(&rt, "edit-dry").iter().any(|x| x == "✗ It would not be sent: the calls do not load: question.call: line 3"));
        // a key after the answer: faint, and said, until the text as it is now is answered
        came(&mut rt, &ask, true, answer(q5.clone()));
        rt.update(|s: &mut App, _| {
            let e = s.tickets.edits.call_edit.as_mut().unwrap();
            e.text = rune::TextEdit::new(e.text.text.replacen("Answer it;", "Answer it, naming the file you read;", 1));
        });
        frames(&mut rt);
        assert!(crate::edit::dry_of(&rt.state).is_some_and(|a| a != ask), "the new text is asked");
        assert!(rt.find("edit-dry-stale").is_some() && rt.find("edit-dry-text").is_some(), "{:?}", texts_under(&rt, "edit-dry"));
        // an older coordd: nothing asked, and it says so
        rt.update(|s: &mut App, _| s.projects[0].protocol = coord_types::DRY_RUN_PROTOCOL - 1);
        frames(&mut rt);
        assert_eq!(texts_under(&rt, "edit-dry").into_iter().filter(|x| x != "Hide").collect::<Vec<_>>(), vec!["Needs a newer engine.".to_string()]);
        rt.update(|s: &mut App, _| s.projects[0].protocol = coord_types::PROTOCOL);
        frames(&mut rt);
        // only this thread: the box's text, on Q-5, and no file; a text that does not check is not asked
        press(&mut rt, "step-for-thread");
        frames(&mut rt);
        let ask = crate::edit::dry_of(&rt.state).expect("the box's text checks");
        assert!(ask.call == "answer" && ask.on.as_deref() == Some("Q-5") && ask.path.is_none() && ask.text.starts_with("A question for you:\n{{ ask }}"), "{ask:?}");
        assert!(rt.find("step-once").is_some() && rt.find("edit-dry").is_some(), "under the thread's own box");
        came(&mut rt, &ask, true, answer(q5));
        assert!(rt.find("edit-dry-text").is_some() && rt.find("edit-dry-stale").is_none());
        rt.update(|s: &mut App, _| s.tickets.edits.once = rune::TextEdit::new("Say {{ nosuch }}"));
        frames(&mut rt);
        assert!(crate::edit::dry_of(&rt.state).is_none() && rt.state.tickets.edits.dry.is_none(), "a text that does not check: nothing asked, the answer dropped");
        assert!(texts_under(&rt, "edit-dry").iter().any(|x| x == "Preview once it checks."));
        // Hide drops it; the edit's end hides it again
        press(&mut rt, "edit-dry-hide");
        assert!(!rt.state.tickets.edits.dry_open && rt.state.tickets.edits.dry.is_none());
        frames(&mut rt);
        assert!(rt.find("edit-dry-show").is_some());
        press(&mut rt, "edit-dry-show");
        rt.update(|s: &mut App, _| (s.tickets.edits.step, s.tickets.edits.call_edit) = (None, None));
        frames(&mut rt);
        assert!(!rt.state.tickets.edits.dry_open, "no prompt being edited: hidden");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A clock's period edited in place (p294): its start pressed on the board, Edit opens its line of its file (the
    /// snapshot's clock) under the board, the start marked; a line that does not read holds Save; Save writes that line
    /// in its file, the rest as it was.
    #[test]
    fn a_clocks_line_is_edited_in_place_under_the_board() {
        crate::elsewhere::files_here();
        let (_held, mut s) = board_app();
        let dir = std::env::temp_dir().join(format!("yaaihh-live-clock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("base").join("kinds")).unwrap();
        std::fs::create_dir_all(dir.join("proj").join("kinds").join("agents")).unwrap();
        let file = dir.join("proj").join("kinds").join("agents").join("pr.kind");
        let text = "kind checkup\n  form due: Text\n  result Text\n  pattern\n    r <- step pr look(due);\n    return due\n\n// pr's clock\nevery 15m open checkup(\"x\")\n";
        std::fs::write(&file, text).unwrap();
        s.base = dir.join("base").display().to_string();
        let p = &mut s.projects[0];
        p.live.places = Some(coord_types::Places { coord: "/c".into(), definition: Some(dir.join("proj").display().to_string()), ..Default::default() });
        p.live.clocks[0].file = file.display().to_string();
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (4400.0, 7000.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        press(&mut rt, "docket-start-checkup-0");
        press(&mut rt, "edit-clock-line");
        let e = rt.state.tickets.edits.kind.as_ref().unwrap_or_else(|| panic!("the clock's line opens: {:?}", rt.state.tickets.edits.said));
        assert_eq!((e.text.text.as_str(), e.binding.as_deref(), e.lines), ("every 15m open checkup(\"x\")", Some("every 15m open checkup(\"x\")"), None));
        assert!(rt.find("clock-editor").is_some() && rt.find("docket-start-checkup-0-editing").is_some(), "under the board, its start marked");
        assert!(rt.find("edit-kind-checkup").is_none(), "no kind's editor opens for it");
        // a line that does not read: the check says so, and nothing is asked of coordd
        rt.update(|s: &mut App, _| s.tickets.edits.kind.as_mut().unwrap().text = rune::TextEdit::new("every 15 open checkup(\"x\")"));
        frames(&mut rt);
        assert!(!rt.state.tickets.edits.kind.as_ref().unwrap().check().is_empty() && crate::edit::reach_of(&rt.state).is_none());
        rt.update(|s: &mut App, _| {
            s.tickets.edits.kind.as_mut().unwrap().text = rune::TextEdit::new("every 30m open checkup(\"x\")");
            s.tickets.edits.why = rune::TextEdit::new("half as often");
        });
        frames(&mut rt);
        assert!(rt.state.tickets.edits.kind.as_ref().unwrap().check().is_empty(), "{:?}", rt.state.tickets.edits.kind.as_ref().unwrap().check());
        let (f, written) = crate::edit::reach_of(&rt.state).expect("its file is asked of coordd");
        assert_eq!((f, written.as_str()), (file.clone(), text.replace("every 15m", "every 30m").as_str()));
        assert!(texts_under(&rt, "clock-editor").iter().any(|t| t.starts_with("Editing checkup's clock, line 9 of ")));
        press(&mut rt, "kind-save");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text.replace("every 15m", "every 30m"), "{:?}", rt.state.tickets.edits.said);
        assert!(rt.state.tickets.edits.kind.is_none() && rt.find("clock-editor").is_none());
        assert!(matches!(&rt.state.tickets.edits.said, Some(Ok(w)) | Some(Err(w)) if w.starts_with("Saved, written") && w.contains("checkup opens by its new line.")), "{:?}", rt.state.tickets.edits.said);
        // a clock whose line is no longer what coordd read: refused, in words
        let f = file.display().to_string();
        rt.update(move |s: &mut App, _| crate::edit::start_binding(s, "checkup", &f, 9, "every 15m open checkup(\"x\")"));
        assert!(matches!(&rt.state.tickets.edits.said, Some(Err(w)) if w.starts_with("Line 9 of ")), "{:?}", rt.state.tickets.edits.said);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The two presses the merge brings together (docket-edge5 p253, consult-3 p255): a step on the board opens its call
    /// in its program's card under the board, as a step in the card's picture does (`open_call`), with its model line;
    /// a binding on the card's edge opens its runs under the board, as the board's start does (`start:KIND:I`).
    #[test]
    fn the_board_s_step_and_a_card_s_binding_open_what_the_board_and_the_picture_open() {
        use serde_json::json;
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        p.live.agents = vec![serde_json::from_value(json!({"id": "AGT-pr", "name": "pr", "parent": "owner", "role": "lead", "kind": "claude", "model": "claude-opus-5-5",
            "state": "working", "since": "2026-11-20T08:00:00Z", "tools": [], "items": []})).unwrap()];
        s.projects = vec![p];
        s.cur = 0;
        let clock = "every 15m open checkup(\"x\")";
        let look = json!({"name": "look", "file": "kinds/agents/pr.call", "line": 3, "sig": "call look(due: Text) -> Text", "attrs": [["task", "look"]], "template": "Look at {{ due }}.", "doc": "", "task": "look"});
        let checkup: KindForm = serde_json::from_value(json!({"kind": "checkup", "form": [], "text": "kind checkup\n  form due: Text\n  result Text\n  pattern\n    r <- step pr look(due);\n    return due\n",
            "doc": "", "by": "pr", "bindings": [clock], "calls": [look]})).unwrap();
        s.tickets.kinds = Some(Ok(vec![checkup]));
        s.sel = crate::Sel::Kinds;
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (1800.0, 3000.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        // the board's step: its call, in checkup's card, opened under the board
        let step = rt.elements().iter().filter_map(|e| e.key.clone()).find(|k| k.starts_with("docket-step-checkup-")).expect("the board draws checkup's step");
        press(&mut rt, &step);
        assert_eq!(rt.state.tickets.edits.call.as_deref(), Some("look"));
        assert!(rt.state.kinds_open.contains("checkup"), "its card is open");
        let els = rt.elements();
        let panel = els.iter().position(|e| e.key.as_deref() == Some("call-panel") && !e.exiting).expect("its call is shown");
        let mut i = panel;
        while els[i].key.as_deref() != Some("kind-checkup") {
            i = els[i].parent.expect("the call panel is inside checkup's card");
        }
        let said: Vec<String> = els.iter().filter_map(|e| e.text.clone()).collect();
        for want in ["Look at {{ due }}.", "the agent's own model · pr: Opus 5.5 now"] {
            assert!(said.iter().any(|t| t == want), "{want:?} in {said:?}");
        }
        // the same step pressed in the card's picture puts it away, as the same press does
        press(&mut rt, "pic-kind-checkup.0");
        assert_eq!(rt.state.tickets.edits.call, None);
        // the binding on the card's edge: its runs under the board, the board's own start view
        press(&mut rt, "pic-kind-checkup-start-0");
        assert_eq!(picked(&rt.state.tickets.pic_open, PANEL), Some("start:checkup:0"));
        assert!(rt.elements().iter().any(|e| e.text.as_deref() == Some("Opens checkup, as written: every 15m open checkup(\"x\")")), "the board's start view");
    }

    /// `board_app`'s project as the stills show it (p218, p223): its kinds with their calls and models as the Kinds
    /// answer gives them, its agents on real model names, and N-7 a thread with its program and a post.
    fn still_app() -> App {
        use serde_json::json;
        const THREAD: &str = include_str!("../../../../agent-base/kinds/thread.kind");
        let mut s = board_app().1;
        // the calls as agent-base writes them, read as the window's fallback reads a file (`CallForm::of`), with what
        // coordd adds: the call it runs for a watcher's step, and the task's model
        let files: Vec<(String, String)> = [
            ("agent-base/calls/question.call", include_str!("../../../../agent-base/calls/question.call")),
            ("agent-base/calls/next.call", include_str!("../../../../agent-base/calls/next.call")),
            ("agent-base/calls/fold.call", include_str!("../../../../agent-base/calls/fold.call")),
            ("agent-base/calls/route.call", include_str!("../../../../agent-base/calls/route.call")),
        ]
        .map(|(f, t)| (f.to_string(), t.to_string()))
        .into();
        let mut named = docket_machine::syntax::Named::new();
        docket_machine::syntax::parse_types("system", docket_machine::syntax::SYSTEM_TYPES, &mut named).unwrap();
        let (defs, errors) = docket_machine::syntax::parse_call_files(&files, &mut named);
        assert!(errors.is_empty(), "agent-base's calls read: {errors:?}");
        let of = |name: &str, runs: Option<&str>, model: serde_json::Value| -> crate::kinds::CallForm {
            let mut c = crate::kinds::CallForm::of(&defs[name]);
            c.runs = runs.map(|r| crate::kinds::RunsCall { name: r.into(), template: defs[r].template.clone(), ..Default::default() });
            c.model = serde_json::from_value(model).unwrap();
            c
        };
        let answer = of("answer", None, json!(null));
        let fold = of("fold", Some("choose"), json!({"model": "gpt-6.2-sol"}));
        // coordd's own calls on gpt-6.2-sol, never the small model (his rule, decisions.eflint:29; proto-reader N-1481 p113)
        let route = of("route", Some("whom"), json!({"model": "gpt-6.2-sol"}));
        if let Some(Ok(ks)) = &mut s.tickets.kinds {
            for k in ks.iter_mut() {
                k.calls = match k.kind.as_str() {
                    "question" => vec![answer.clone()],
                    "delivery" => vec![fold.clone()],
                    "base" => vec![route.clone()],
                    _ => vec![],
                };
            }
        }
        let p = &mut s.projects[0];
        p.protocol = coord_types::PROTOCOL;
        for (a, m) in p.live.agents.iter_mut().zip(["claude-opus-5-5", "claude-sonnet-5-5"]) {
            a.model = m.into();
        }
        let thread: TicketView = serde_json::from_value(json!({"id": "N-7", "n": 7, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"text": "the runner's rows"},
            "title": "the plan", "status": {"is": "resting"}, "children": ["Q-5"], "program": kind_block(THREAD, "thread"), "bound": 1, "spent": 0,
            "waits_on": [{"ticket": "Q-5", "agent": "pr", "call": "answer"}],
            "binding": "on owner_message(m) open thread(m.to, m.typed, m.names, m.text, m.addressed, m.follows)",
            "opened_at": "2026-11-20T08:00:00Z", "given": [], "posts": [{"n": 1, "at": "2026-11-20T08:00:00Z", "by": "owner", "text": "pr, can the runner's rows say which commit each one checks?", "names": ["pr"], "told": ["pr"]},
                {"n": 2, "at": "2026-11-20T08:04:00Z", "by": "pr", "text": "Yes: each row gets its commit's short hash and subject. I'll ask qa for the test.", "names": ["qa"], "told": ["qa"]}]})).unwrap();
        p.live.tickets.insert(7, thread);
        // each question as coordd's view gives it: its kind's program, and what it waits on, its own step, owed by pr
        const QUESTION: &str = include_str!("../../../../agent-base/kinds/question.kind");
        for (n, id) in [(5, "Q-5"), (6, "Q-6")] {
            let q = p.live.tickets.get_mut(&n).unwrap();
            q.program = kind_block(QUESTION, "question").to_string();
            q.waits_on = serde_json::from_value(json!([{"ticket": id, "agent": "pr", "call": "answer"}])).unwrap();
        }
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = Some(7);
        s
    }

    /// The id and the time of the made-up run I under N-7 (`ran_app`).
    pub(crate) fn run_id(i: u64) -> String {
        format!("O-{}", docket_machine::HIDDEN + i)
    }
    pub(crate) fn run_at(m: u64) -> String {
        format!("2026-11-20T08:{m:02}:00Z")
    }

    /// `still_app` with runs under N-7 (0.3 W3, docket-edge5's `TicketView::ran`), made up: base's `owner_post` ran
    /// seven times, one for each post of his, the last still going, and a stop's run of no `when`. The first is fetched
    /// as coordd's Ticket answer gives it: under N-7, its program its handler's body as printed.
    fn ran_app() -> App {
        use serde_json::json;
        const BASE: &str = include_str!("../../../../agent-base/kinds/base.kind");
        let (id, at) = (run_id, run_at);
        let base = parse_kind(kind_block(BASE, "base")).unwrap();
        let owner_post = base.handlers.iter().find(|w| w.ev == "owner_post").unwrap();
        let mut s = still_app();
        let mut ran: Vec<serde_json::Value> = (1..=7u64)
            .map(|i| {
                let done = i < 7;
                json!({"id": id(i), "event": "owner_post", "from": "base", "line": 43, "post": i, "opened_at": at(9 + i),
                    "closed_at": if done { json!(at(9 + i)) } else { json!(null) }, "said": if done { json!("To([pr])") } else { json!(null) }})
            })
            .collect();
        ran.push(json!({"id": id(20), "event": "stopped", "opened_at": at(30), "closed_at": at(30), "said": "stopped"}));
        let p = &mut s.projects[0];
        p.live.tickets.get_mut(&7).unwrap().ran = serde_json::from_value(json!(ran)).unwrap();
        let run: TicketView = serde_json::from_value(json!({"id": id(1), "n": docket_machine::HIDDEN + 1, "kind": "owner_post", "parent": "N-7", "under": "N-7", "opener": "coordd", "ctx": {}, "form": {},
            "title": "owner_post", "status": {"is": "done", "result": {"head": "Ok"}}, "children": [], "program": docket_machine::print::prog(&owner_post.body, 0), "bound": 1, "spent": 1,
            "opened_at": at(10), "closed_at": at(10), "given": [], "posts": []})).unwrap();
        let pid = p.id.clone();
        s.tickets.find.fetched.entry(pid).or_default().insert(docket_machine::HIDDEN + 1, run);
        s
    }

    /// 0.3 W3 (N-1446 p16): at the Thread scale every handler of N-7's kind stands as a row, thread.kind's own and those
    /// it takes from base, lit by the runs it made under N-7 (`TicketView::ran`, docket-edge5's, made up here). base's
    /// `owner_post` ran 7 times: the latest 5 each a `label · id`, the one still going says so, the 2 before them behind
    /// a press. A handler that never ran says so; a run of no `when` (a stop) stands in a row of its event's. A row's
    /// head pressed draws its handler's body under it. A run pressed opens its own program, its handler's body as
    /// coordd printed it, at the Thread scale, fetched as find.rs fetches a ticket; its way back is N-7. With no run
    /// listed (a coordd before `ran`), the rows say nothing of runs.
    #[test]
    fn a_threads_handlers_are_rows_lit_by_their_runs_and_a_run_opens_its_program() {
        type Rt = rune::runtime::Runtime<App>;
        let texts = |rt: &Rt| -> Vec<String> { rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect() };
        let text_of = |rt: &Rt, key: &str| -> Option<String> { rt.elements().into_iter().filter(|e| !e.exiting).find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text) };
        let drawn = |rt: &Rt, key: &str| rt.find(key).is_some();
        let (h, id, at) = (docket_machine::HIDDEN, run_id, run_at);
        let mut s = ran_app();
        open_thread(&mut s, 7);
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (1800.0, 6000.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        // base's hole, ask and owner_post, as the engine takes them: thread.kind has no `when` of its own since the
        // typed ask took out its `when agent_post`
        let t = texts(&rt);
        for w in ["When something happens", "when it rests and something happens", "when an agent asks", "when your words land", "base's", "To([pr])", "running"] {
            assert!(t.iter().any(|x| x == w), "{w:?} is drawn: {t:?}");
        }
        assert!(!t.iter().any(|x| x == "when an agent posts"), "no handler on an agent's post: {t:?}");
        assert_eq!(text_of(&rt, "pic-N-7-on-2-ran"), Some(format!("ran 7 · last {}", crate::when::at(&at(16)))), "base's owner_post: how many and when last");
        for i in 0..2 {
            assert_eq!(text_of(&rt, &format!("pic-N-7-on-{i}-ran")).as_deref(), Some("never ran"), "row {i}");
        }
        for i in 3..=7 {
            assert!(drawn(&rt, &format!("pic-N-7-on-2-run-{}", id(i))), "run {i}, among the latest five");
            assert!(t.contains(&format!("#{i} · {}", id(i))), "run {i} as `label · id`");
        }
        assert!(!drawn(&rt, &format!("pic-N-7-on-2-run-{}", id(1))) && t.iter().any(|x| x == "› the 2 before them"), "the two before them behind a press");
        // the stop's run: no `when` of a kind ran it
        assert_eq!(text_of(&rt, "pic-N-7-on-x0-ran"), Some(format!("ran 1 · last {}", crate::when::at(&at(30)))));
        assert!(t.iter().any(|x| x == "run by the engine itself") && t.iter().any(|x| x == "when something comes and it is stopped"));
        press(&mut rt, "pic-N-7-on-2-all");
        assert!(drawn(&rt, &format!("pic-N-7-on-2-run-{}", id(1))) && drawn(&rt, &format!("pic-N-7-on-2-run-{}", id(2))), "all seven, pressed");
        // a row's head: its handler's body under it
        press(&mut rt, "pic-N-7-on-2");
        assert!(rt.state.tickets.pic_open.contains("N-7-on-2-body"));
        // a run pressed: its own program at the Thread scale, and back to N-7
        press(&mut rt, &format!("pic-N-7-on-2-run-{}", id(1)));
        assert_eq!((rt.state.sel.clone(), scale(&rt.state.tickets.pic_open), rt.state.tickets.shown), (crate::Sel::Kinds, Scale::Thread, Some(h + 1)));
        assert!(drawn(&rt, &format!("stages-{}", id(1))), "its handler's body, drawn as a program");
        assert!(texts(&rt).iter().any(|x| x == "‹ N-7") && !drawn(&rt, &format!("pic-{}-on", id(1))), "its way back is N-7; a run draws no rows of its own");
        press(&mut rt, "docket-back");
        assert_eq!((rt.state.sel.clone(), scale(&rt.state.tickets.pic_open), rt.state.tickets.shown), (crate::Sel::Kinds, Scale::Thread, Some(7)));
        // a coordd before `ran`: the rows, and nothing said of runs
        rt.update(|s, _| s.projects[0].live.tickets.get_mut(&7).unwrap().ran.clear());
        frames(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "when your words land") && !t.iter().any(|x| x == "never ran" || x.starts_with("ran ")), "{t:?}");
    }

    /// `ran_app` with its first run's program three steps (made up): `answer`, given to pr once; the watcher's `next`,
    /// given twice and answered by coordd itself each time (`own`); `check`, not given yet. Opened at the Thread scale.
    pub(crate) fn wire_app() -> App {
        use serde_json::json;
        let (h, id, at) = (docket_machine::HIDDEN, run_id, run_at);
        let mut s = ran_app();
        let pid = s.projects[0].id.clone();
        let run = s.tickets.find.fetched.get_mut(&pid).unwrap().get_mut(&(h + 1)).unwrap();
        run.program = "a <- step role pr answer(p.text);\nn <- step watcher next(a);\nc <- step role pr check(n);\nreturn c\n".into();
        run.run = serde_json::from_value(json!([
            {"what": "step", "line": 1, "source": 0, "text": "a <- step role pr answer(p.text)", "state": "done", "who": "pr", "rounds": [{"step": 1, "to": "pr", "given": at(10), "answered": at(11)}]},
            {"what": "step", "line": 2, "source": 0, "text": "n <- step watcher next(a)", "state": "done", "who": "watcher", "rounds": [{"step": 2, "to": "watcher", "given": at(12), "answered": at(12), "own": true}, {"step": 3, "to": "watcher", "given": at(13), "answered": at(13), "own": true}]},
            {"what": "step", "line": 3, "source": 0, "text": "c <- step role pr check(n)", "state": "to_come"},
            {"what": "return", "line": 4, "source": 0, "text": "return c", "state": "to_come"},
        ]))
        .unwrap();
        open_run(&mut s, &id(1));
        s
    }

    /// 0.3 W3's last press (N-1446 p16, proto-window's Wire layer p308): a run's step, pressed among its stages, goes
    /// down to the wire from its panel at `run.sSTEP`, once for each round given. A step given once says "Wire ›", a step
    /// given round after round "round R · step N ›" for each; a step not given yet has none. The press opens Wire there.
    #[test]
    fn a_runs_step_goes_down_to_the_wire_once_for_each_round_given() {
        type Rt = rune::runtime::Runtime<App>;
        let id = run_id(1);
        let wires = |rt: &Rt| -> Vec<String> { rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.key).filter(|k| k.contains("-wire-")).collect() };
        let texts = |rt: &Rt| -> Vec<String> { rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect() };
        let mut rt = rune::runtime::Runtime::new(wire_app(), crate::view, (1800.0, 6000.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        for i in 0..3 {
            assert!(rt.find(&format!("stage-{id}.{i}")).is_some(), "step {i} of the run's program, a stage");
        }
        assert_eq!(wires(&rt), Vec::<String>::new(), "nothing on the wire before a step is pressed");
        // pr's answer, given once
        press(&mut rt, &format!("stage-{id}.0"));
        assert_eq!(wires(&rt), [format!("stage-panel-{id}.0-wire-1")]);
        assert!(texts(&rt).iter().any(|x| x == "Wire ›") && !texts(&rt).iter().any(|x| x == "On the wire"));
        // the watcher's next, given twice: a press for each round
        press(&mut rt, &format!("stage-{id}.1"));
        assert_eq!(wires(&rt), [format!("stage-panel-{id}.1-wire-2"), format!("stage-panel-{id}.1-wire-3")]);
        let t = texts(&rt);
        for w in ["On the wire", "round 1 · step 2 ›", "round 2 · step 3 ›"] {
            assert_eq!(t.iter().filter(|x| *x == w).count(), 1, "{w:?} once: {t:?}");
        }
        assert!(!t.iter().any(|x| x == "Wire ›"));
        // the check, not given yet
        press(&mut rt, &format!("stage-{id}.2"));
        assert_eq!(wires(&rt), Vec::<String>::new());
        // the second round pressed: Wire at that step
        press(&mut rt, &format!("stage-{id}.1"));
        press(&mut rt, &format!("stage-panel-{id}.1-wire-3"));
        let wire = crate::lens::Lens { filter: crate::lens::Filter::Threads, depth: crate::lens::Depth::Wire, at: Some(format!("{id}.s3")) };
        assert_eq!(rt.state.sel, crate::Sel::Lens(wire), "down to the wire at the second round");
    }

    /// 0.3 on S1 (N-1446 p113, p153): the Docket at a thread, its pane left, is kept as a Docket lens at that thread and
    /// drawn there, its program and its handlers' runs, while the pane beside opens another thread's; focused again it is
    /// the Kinds page at its thread, the one value the Docket shown has (p120). A run kept opens again while fetched; one
    /// gone (a restart) opens the Docket at Project. The Docket at Project keeps nothing.
    #[test]
    fn the_docket_at_a_thread_is_kept_as_a_lens_while_the_pane_beside_has_focus() {
        use crate::lens::{Filter, Lens};
        let docket_at = |id: &str| crate::Sel::Lens(Lens { at: Some(id.to_string()), ..Lens::of(Filter::Docket) });
        let mut s = ran_app();
        open_thread(&mut s, 7);
        s.split();
        assert_eq!((s.panes[0].clone(), s.focus, s.sel.clone()), ((0, docket_at("N-7")), 1, crate::Sel::Kinds), "N-7 kept as a Docket lens; the pane beside is the Kinds page");
        open_thread(&mut s, 5);
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (3600.0, 4000.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        assert!(rt.find("lens-apart-0-docket").is_some() && rt.find("stages-N-7").is_some() && rt.find("pic-N-7-on-2").is_some(), "the pane left draws N-7's program and the runs of its handlers");
        assert!(rt.find("stages-Q-5").is_some(), "the pane focused draws Q-5's");
        rt.update(|s, _| s.focus_pane(0));
        frames(&mut rt);
        let st = &rt.state;
        assert_eq!((st.sel.clone(), scale(&st.tickets.pic_open), st.tickets.shown, st.panes[1].clone()), (crate::Sel::Kinds, Scale::Thread, Some(7), (0, docket_at("Q-5"))), "focused again: N-7; the other keeps Q-5");
        assert!(crate::sidebar::lens_shown(st, &Lens::of(Filter::Docket)), "a pinned Docket lens is lit while the Docket is shown (`Lens::as_sel`)");
        // a run of a handler, kept at its id, opens again while fetched; after a restart, at Project
        let run = run_id(1);
        rt.update(|s, _| open_run(s, &run_id(1)));
        let kept = rt.state.sel_kept();
        assert_eq!(kept, docket_at(&run));
        let again = kept.clone();
        rt.update(move |s, _| {
            s.sel = crate::Sel::Tickets;
            s.show_sel(again);
        });
        assert_eq!((rt.state.sel.clone(), scale(&rt.state.tickets.pic_open), rt.state.tickets.shown), (crate::Sel::Kinds, Scale::Thread, Some(docket_machine::HIDDEN + 1)));
        rt.update(move |s, _| {
            s.tickets.find.fetched.clear();
            s.show_sel(kept);
        });
        assert_eq!((rt.state.sel.clone(), scale(&rt.state.tickets.pic_open)), (crate::Sel::Kinds, Scale::Project), "a run no longer held: the Docket at Project");
        assert_eq!(rt.state.sel_kept(), crate::Sel::Kinds, "the Docket at Project keeps nothing");
    }

    /// App S as KEYS pressed in turn leave it, in a runtime wide enough that nothing pressed is scrolled out.
    fn pressed(s: App, keys: &[&str]) -> App {
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (4400.0, 7000.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        for key in keys {
            press(&mut rt, key);
        }
        std::mem::replace(&mut rt.state, crate::view_cost::test_app().1)
    }

    /// STILL_OUT=DIR: the real window on `still_app`'s made-up data (paper-research N-1396 p218, p223). The bar's
    /// `Docket` and the thread head's, on thread N-7 (bar-docket.png); the bar's press from it: the Thread scale, its
    /// program and the way back (scale-thread.png), and that way pressed: the thread again, where he was in it
    /// (back-thread.png); a thread with no program at the Thread scale (scale-thread-plain.png, consult-3 p222); the
    /// switch at Project (scale-project.png); a worker pressed on the
    /// board: the Agent scale, pr picked (scale-agent.png). Three presses in a kind's card: the lead's step, its call and
    /// the agent's own model (press-step.png); the watcher's `route` in base's handler on his post, the call coordd runs
    /// for it (`whom`) and its task's model (press-watcher.png; delivery's `next` until I-1480 c took its step out, and
    /// thread's `asks` until the typed ask took its handler out); a case, its arm lit in the text beside it
    /// (press-case.png). Live edits (p294), as the Windows
    /// window has them, from the files its coordd serves: a step's prompt edited in place, its step marked
    /// (live-prompt.png); what it reaches, as coordd answers (live-reach.png); after Save, the project's own copy written
    /// and read again, the step saying when (live-saved.png); a case's lines edited in place (live-case.png); on a
    /// ticket's step, the one Save for the program or only this thread (live-thread.png); a clock's line edited in place
    /// under the board, its start marked, the next firing coordd would give it (live-clock.png). The prompt as it would
    /// be sent, behind his press, as coordd's dry run answers it on made-up arguments (H2, N-1446 p1824): for the
    /// program (live-dry.png) and for only this thread (live-dry-thread.png). 0.3 W3 (N-1446 p16), on
    /// `ran_app`'s runs: N-7's handlers at the Thread scale, each lit by its runs (zoom-rows.png); base's `owner_post`
    /// pressed open, its body and all seven runs (zoom-row-open.png); its first run pressed: its own program, the way
    /// back to N-7 (zoom-run.png); a run's step given twice pressed among its stages, its panel's press down to the wire
    /// for each round (zoom-wire.png, `wire_app`). The Docket as a lens (p113, p153): N-7's left in the pane beside,
    /// Q-5's focused (docket-lens-beside.png).
    #[test]
    #[ignore]
    fn scales_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn bar() -> App {
            still_app()
        }
        fn thread() -> App {
            pressed(still_app(), &["docket-mark"])
        }
        fn back() -> App {
            pressed(still_app(), &["docket-mark", "docket-back"])
        }
        fn plain() -> App {
            let mut s = still_app();
            let p = &mut s.projects[0];
            let v: TicketView = serde_json::from_value(serde_json::json!({"id": "N-9", "n": 9, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"text": "the release notes"},
                "title": "the release notes", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 0, "opened_at": "2026-11-20T09:00:00Z", "given": [],
                "posts": [{"n": 1, "at": "2026-11-20T09:00:00Z", "by": "owner", "text": "qa, the release notes for build 33, please.", "names": ["qa"], "told": ["qa"]}]})).unwrap();
            p.live.tickets.insert(9, v);
            p.labels.set("N-9", "the release notes");
            s.tickets.shown = Some(9);
            pressed(s, &["docket-mark"])
        }
        fn project() -> App {
            pressed(still_app(), &["docket-mark", "docket-scale/segment0"])
        }
        fn agent() -> App {
            pressed(still_app(), &["docket-mark", "docket-scale/segment0", "docket-who-pr"])
        }
        fn cards() -> App {
            let mut s = pressed(still_app(), &["docket-mark", "docket-scale/segment0"]);
            s.kinds_open.extend(["question".to_string(), "delivery".to_string()]);
            s
        }
        fn step() -> App {
            pressed(cards(), &["pic-kind-question.0"])
        }
        fn watcher() -> App {
            // the press's own function, its line lit: base's `route`, in its handler on his post (picture.rs,
            // open_call); delivery has no step since I-1480 c, and thread no handler since the typed ask
            const BASE: &str = include_str!("../../../../agent-base/kinds/base.kind");
            let mut s = cards();
            let line = kind_block(BASE, "base").lines().position(|l| l.contains("step watcher route")).expect("base's route") + 1;
            open_call(&mut s, "base", "route", docket_machine::WATCHER, Some(&format!("{}base:{line}", crate::picture::TEXT)));
            s
        }
        fn case() -> App {
            pressed(cards(), &["pic-kind-question.1.a2-case"])
        }
        // live edits: the Windows window's files are the WSL machine's, served by its coordd (`Sources`); headless there
        // is no link, so coordd's answers are given as they come
        fn live(s: App) -> App {
            crate::elsewhere::still_as_windows();
            let mut s = s;
            s.projects[0].live.places = Some(coord_types::Places { coord: "/c".into(), definition: Some("/srv/proj".into()), ..Default::default() });
            s
        }
        fn served() -> String {
            let of = |dir: &str| -> Vec<serde_json::Value> { LIVE_BASE.iter().filter(|(f, _)| f.starts_with(dir)).map(|(f, t)| serde_json::json!({"path": format!("/srv/agent-base/{f}"), "text": t})).collect() };
            let mut kinds = of("kinds/");
            kinds.push(serde_json::json!({"path": "/srv/proj/kinds/note.kind", "text": "kind note\n  form text: Text\n  result Text\n  pattern\n    return text\n"}));
            serde_json::json!({"kinds": kinds, "calls": of("calls/")}).to_string()
        }
        fn reach(s: &mut App, tickets: serde_json::Value) {
            let (_, text) = crate::edit::reach_of(s).expect("the edit checks");
            crate::edit::came(s, 0, crate::edit::Asked::Reach { text }, true, serde_json::json!({"kinds": [], "calls": [], "clocks": [], "tickets": tickets}).to_string());
        }
        fn prompt_at(stage: u8) -> App {
            let mut s = pressed(live(cards()), &["pic-kind-question.0"]);
            crate::edit::came(&mut s, 0, crate::edit::Asked::Sources { kind: false, name: "answer".into() }, true, served());
            let e = s.tickets.edits.call_edit.as_mut().expect("answer's lines open in place");
            e.text = rune::TextEdit::new(e.text.text.replacen("Answer it;", "Answer it, naming the file you read;", 1));
            s.tickets.edits.why = rune::TextEdit::new("name the file it read");
            if stage >= 1 {
                reach(&mut s, serde_json::json!([
                    {"id": "Q-5", "kind": "question", "reach": "ahead", "said": "its answer step is not sent yet: it is sent with the new words"},
                    {"id": "Q-6", "kind": "question", "reach": "given", "said": "its answer step was sent at 08:10 with the old words; a reminder reads the new ones"},
                    {"id": "N-7", "kind": "thread", "reach": "own", "said": "your own prompt for this thread stands (only this thread, 09:00)"}]));
            }
            if stage >= 2 {
                let copy = " From now on research runs its own answer.call (/srv/proj/calls/answer.call): agent-base's later changes to it don't reach research while this copy is there. Removing that file returns research to agent-base's.".to_string();
                crate::edit::came(&mut s, 0, crate::edit::Asked::Saved { kind: false, name: "answer".into(), why: "name the file it read".into(), copy }, true, "commit 3f1c2a7".into());
                // the reload asked for after the commit, as coordd answers it (no link: not sent)
                let line = match s.tickets.edits.said.take() {
                    Some(Ok(t)) | Some(Err(t)) => t.split(" coordd was not asked").next().unwrap_or_default().to_string(),
                    None => String::new(),
                };
                s.tickets.edits.said = Some(Ok(line));
                s.notice = None;
                crate::edit::replied(&mut s, true, true, r#"{"kinds":[],"calls":["answer"],"tickets":[]}"#);
                // the Kinds answer asked again, as coordd gives it now: the copy's file, its new template, its edit
                let at = s.tickets.edits.reloaded.as_ref().map(|(w, _)| w.clone()).unwrap_or_default();
                if let Some(Ok(ks)) = &mut s.tickets.kinds {
                    for c in ks.iter_mut().flat_map(|k| k.calls.iter_mut()).filter(|c| c.name == "answer") {
                        c.file = "/srv/proj/calls/answer.call".into();
                        c.line = 1;
                        c.template = c.template.as_ref().map(|t| t.replacen("Answer it;", "Answer it, naming the file you read;", 1));
                        c.edited = Some(crate::kinds::Edited { at: at.clone(), by: "owner".into(), why: "name the file it read".into(), commit: Some("3f1c2a7".into()) });
                    }
                }
            }
            s
        }
        fn live_prompt() -> App {
            prompt_at(0)
        }
        fn live_reach() -> App {
            prompt_at(1)
        }
        fn live_saved() -> App {
            prompt_at(2)
        }
        // the prompt as it would be sent (H2): coordd's dry run as it comes, on made-up arguments
        fn dry(s: &mut App, a: coord_types::DryRunAnswer) {
            s.tickets.edits.dry_open = true;
            let ask = crate::edit::dry_of(s).expect("the edit checks");
            crate::edit::came(s, 0, crate::edit::Asked::DryRun { ask }, true, serde_json::to_string(&a).unwrap());
        }
        const DRY_TEXT: &str = "A question for you:\nWhich file holds the look's colours?\n\nAnswer it, naming the file you read; or dismiss it, saying why; or, if only the owner can answer it, pass it up, saying why.\n\nThis step gives one of:\n  Answer\n    text\n  Dismiss\n    why\n  Up\n    why";
        fn live_dry() -> App {
            let mut s = prompt_at(1);
            dry(&mut s, coord_types::DryRunAnswer { on: Some("Q-5".into()), step: 2, by: "pr".into(), asked: false, own: false, text: DRY_TEXT.into() });
            s
        }
        fn live_dry_thread() -> App {
            let mut s = live_thread();
            s.tickets.edits.only_thread = true;
            s.tickets.edits.once = rune::TextEdit::new("A question for you, from this thread only:\n{{ ask }}\n\nAnswer it in one line.");
            let text = DRY_TEXT.replace("A question for you:", "A question for you, from this thread only:").replace("Answer it, naming the file you read; or dismiss it, saying why; or, if only the owner can answer it, pass it up, saying why.", "Answer it in one line.");
            dry(&mut s, coord_types::DryRunAnswer { on: Some("Q-5".into()), step: 2, by: "pr".into(), asked: true, own: false, text });
            s
        }
        fn live_case() -> App {
            let mut s = pressed(live(cards()), &["pic-kind-question.1.a2-case", "edit-lines"]);
            crate::edit::came(&mut s, 0, crate::edit::Asked::Sources { kind: true, name: "question".into() }, true, served());
            let e = s.tickets.edits.kind.as_mut().expect("the case's lines open in place");
            e.text = rune::TextEdit::new(e.text.text.replace("o <- step owner", "his <- step owner").replace("match o {", "match his {"));
            s.tickets.edits.why = rune::TextEdit::new("his answer, by name");
            reach(&mut s, serde_json::json!([
                {"id": "Q-5", "kind": "question", "took": true, "reach": "ahead", "said": "it waits on its answer step: the case is ahead of it"},
                {"id": "Q-6", "kind": "question", "took": true, "reach": "ahead", "said": "it waits on its answer step: the case is ahead of it"}]));
            s
        }
        fn live_thread() -> App {
            let mut s = live(still_app());
            s.tickets.shown = Some(5);
            s.tickets.edits.step = Some(crate::edit::Step { t: 5, key: "pic-Q-5.0".into(), worker: "pr".into(), call: "answer".into(), inline: None, awaited: false });
            crate::edit::came(&mut s, 0, crate::edit::Asked::Sources { kind: false, name: "answer".into() }, true, served());
            s
        }
        fn live_clock() -> App {
            let mut s = live(still_app());
            s.projects[0].live.clocks[0].file = "/srv/proj/kinds/agents/pr.kind".into();
            let mut s = pressed(s, &["docket-mark", "docket-scale/segment0", "docket-start-checkup-0", "edit-clock-line"]);
            let mut files: serde_json::Value = serde_json::from_str(&served()).unwrap();
            files["kinds"].as_array_mut().unwrap().push(serde_json::json!({"path": "/srv/proj/kinds/agents/pr.kind",
                "text": "kind checkup\n  form due: Text\n  result Text\n  pattern\n    r <- step pr look(due);\n    return due\n\n// pr's clock\nevery 15m open checkup(\"x\")\n"}));
            crate::edit::came(&mut s, 0, crate::edit::Asked::Sources { kind: true, name: "checkup".into() }, true, files.to_string());
            let e = s.tickets.edits.kind.as_mut().expect("the clock's line opens in place");
            e.text = rune::TextEdit::new("every 30m open checkup(\"x\")");
            s.tickets.edits.why = rune::TextEdit::new("half as often");
            let (_, text) = crate::edit::reach_of(&s).expect("the edit checks");
            let answer = serde_json::json!({"kinds": [], "calls": [], "tickets": [], "clocks": [{"binding": "every 30m open checkup(\"x\")", "next": "2026-11-20T08:30:00Z"}]});
            crate::edit::came(&mut s, 0, crate::edit::Asked::Reach { text }, true, answer.to_string());
            s
        }
        fn zoom_rows() -> App {
            let mut s = ran_app();
            open_thread(&mut s, 7);
            // the test's own folder has no projects.toml: none of that in a still (as zoom_wire, proto-reader p363)
            s.notice = None;
            s
        }
        fn zoom_row_open() -> App {
            pressed(zoom_rows(), &["pic-N-7-on-2-all", "pic-N-7-on-2"])
        }
        fn zoom_run() -> App {
            pressed(zoom_rows(), &["pic-N-7-on-2-all", &format!("pic-N-7-on-2-run-{}", run_id(1))])
        }
        fn zoom_wire() -> App {
            let mut s = pressed(wire_app(), &[&format!("stage-{}.1", run_id(1))]);
            // the test's own folder has no projects.toml: none of that in a still (proto-reader p363)
            s.notice = None;
            s
        }
        fn lens_beside() -> App {
            let mut s = zoom_rows();
            s.split();
            open_thread(&mut s, 5);
            s
        }
        // N-1446 p198: no kinds held while coordd's pipe is down (an install restarting coordd); the page says why,
        // and they are asked again once it is back
        fn kinds_down() -> App {
            let mut s = project();
            s.tickets.kinds = None;
            s.projects[0].down = Some("the pipe ended (3): coord: coordd is not running (/run/user/1000/coordd/research.sock: No such file or directory)".into());
            s
        }
        let shots: [(&str, fn() -> App, Option<&str>, (f32, f32)); 23] = [
            ("bar-docket", bar, None, (1600.0, 1000.0)),
            ("scale-thread", thread, None, (1600.0, 1400.0)),
            ("back-thread", back, None, (1600.0, 1000.0)),
            ("scale-thread-plain", plain, None, (1600.0, 1000.0)),
            ("scale-project", project, None, (1800.0, 1400.0)),
            ("scale-agent", agent, None, (1800.0, 1400.0)),
            ("press-step", step, Some("kind-question"), (2200.0, 5000.0)),
            ("press-watcher", watcher, Some("kind-base"), (2200.0, 5000.0)),
            ("press-case", case, Some("kind-question"), (2200.0, 5000.0)),
            ("live-prompt", live_prompt, Some("kind-question"), (2200.0, 5000.0)),
            ("live-reach", live_reach, Some("kind-question"), (2200.0, 5000.0)),
            ("live-saved", live_saved, Some("kind-question"), (2200.0, 5000.0)),
            ("live-case", live_case, Some("kind-question"), (2200.0, 5000.0)),
            ("live-thread", live_thread, None, (1800.0, 1400.0)),
            ("live-dry", live_dry, Some("kind-question"), (2200.0, 5000.0)),
            ("live-dry-thread", live_dry_thread, None, (1800.0, 2000.0)),
            ("live-clock", live_clock, None, (1800.0, 2000.0)),
            ("zoom-rows", zoom_rows, None, (1600.0, 800.0)),
            ("zoom-row-open", zoom_row_open, None, (1800.0, 1200.0)),
            ("zoom-run", zoom_run, None, (1600.0, 1200.0)),
            ("zoom-wire", zoom_wire, None, (1600.0, 1000.0)),
            ("docket-lens-beside", lens_beside, None, (2800.0, 1400.0)),
            ("kinds-down", kinds_down, None, (1600.0, 800.0)),
        ];
        for (name, setup, crop, (w, h)) in shots {
            let snap = out.join(format!("{name}.png")).display().to_string();
            let mut args = vec!["--snap".to_string(), snap, "--size".into(), format!("{w}x{h}"), "--at".into(), "0.5".into()];
            if let Some(c) = crop {
                args.extend(["--crop".to_string(), c.to_string()]);
            }
            let spec = rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (w, h), setup, crate::view) };
            rune::App::new(spec).args(args).try_run().unwrap();
        }
    }

    #[test]
    fn a_clock_reads_its_next_firing_from_its_last_and_one_never_fired_fires_at_the_next_minute() {
        let c = |every: Option<i64>, when: Option<(&str, &str)>, last: Option<&str>| coord_types::ClockView {
            binding: "b".into(),
            kind: "k".into(),
            every,
            when: when.map(|(d, o)| (d.into(), o.into())),
            by: None,
            file: "f".into(),
            line: 1,
            last: last.map(str::to_string),
        };
        assert_eq!(next_firing("2026-11-20T12:30:00Z", 900).as_deref(), Some("2026-11-20T12:45:00+00:00"));
        assert_eq!(clock_words(&c(Some(900), None, None)), vec!["fires at the next minute".to_string()]);
        let w = clock_words(&c(Some(900), None, Some("2026-11-20T12:30:00Z")));
        assert_eq!(w.len(), 2);
        assert!(w[0].starts_with("next at ") && w[1].starts_with("last "), "{w:?}");
        assert_eq!(clock_words(&c(None, Some(("cut-build", "build")), None)), vec!["the policy's cut-build decides, each minute".to_string()]);
        // a period as every duration is written (paper-research I-1469 p50)
        let took = crate::when::took;
        assert_eq!((took(900), took(3600), took(7200), took(86400), took(5400)), ("15 m".to_string(), "1 h".to_string(), "2 h".to_string(), "1 d".to_string(), "1 h 30 m".to_string()));
    }

    /// The Agent scale alone, as docket-edge8's switch draws it.
    fn scale_view(s: &App, cx: &mut rune::Cx<App>) -> El {
        let k = rune_kit::Kit::neutral(cx.system_dark());
        let kinds = match &s.tickets.kinds {
            Some(Ok(ks)) => ks.as_slice(),
            _ => &[],
        };
        col().key("scale").w(1500.0).padding(20.0).child(agent_scale(s, &s.projects[0], kinds, &k))
    }

    /// A made-up project for the Agent scale, PICK picked: pr the lead, on N-7, owing Q-5's answer; qa, on N-9; the
    /// question, a thread, pr's own checkup and agent-base's delivery. The lock of the tests that make a whole App held
    /// while it lives.
    fn scale_app(pick: &str) -> (std::sync::MutexGuard<'static, ()>, App) {
        use serde_json::json;
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        let agent = |name: &str, role: &str, on: Option<&str>| -> coord_types::AgentView {
            serde_json::from_value(json!({"id": format!("AGT-{name}"), "name": name, "parent": "owner", "role": role, "kind": "claude", "model": "m",
                "state": "working", "since": "2026-11-20T08:00:00Z", "tools": [], "items": [], "on": on})).unwrap()
        };
        p.live.agents = vec![agent("pr", "lead", Some("N-7")), agent("qa", "", None)];
        let ticket = |n: u64, id: &str, kind: &str, opener: &str, status: serde_json::Value, waits: serde_json::Value| -> TicketView {
            serde_json::from_value(json!({"id": id, "n": n, "kind": kind, "opener": opener, "ctx": {}, "form": {}, "title": id, "status": status,
                "children": [], "program": "", "bound": 1, "spent": 0, "opened_at": "2026-11-20T08:00:00Z", "given": [], "posts": [], "waits_on": waits})).unwrap()
        };
        let awaits = json!({"is": "awaits", "agent": "pr", "call": "answer", "step": 1, "since": "2026-11-20T08:00:00Z", "taken": true, "refusals": 0,
            "forming": false, "head": {"Variant": [["Answer", [["text", "Text"]]]]}, "args": {}});
        p.live.tickets.insert(5, ticket(5, "Q-5", "question", "qa", awaits, json!([{"ticket": "Q-5", "agent": "pr", "call": "answer"}])));
        p.live.tickets.insert(7, ticket(7, "N-7", "thread", "pr", json!({"is": "resting"}), json!([])));
        p.live.tickets.insert(9, ticket(9, "N-9", "thread", "qa", json!({"is": "resting"}), json!([])));
        p.labels.set("N-7", "the plan");
        p.labels.set("N-9", "the other plan");
        s.projects = vec![p];
        s.cur = 0;
        let f = |kind: &str, text: &str, by: Option<&str>| -> KindForm { serde_json::from_value(json!({"kind": kind, "form": [], "text": text, "doc": "", "by": by, "bindings": []})).unwrap() };
        s.tickets.kinds = Some(Ok(vec![
            f("question", "kind question\n  form ask: Text\n  result Answered(text: Text)\n  pattern\n    a <- step role lead answer(ask);\n    match a {\n      Answer(text) => return Answered(text)\n      Up(why) =>\n        o <- step owner reply(ask, why);\n        match o {\n          Answer(text) => return Answered(text)\n        }\n    }\n", None),
            f("thread", "kind thread\n  form text: Text\n  result Kept\n  pattern\n    ...\n", None),
            f("checkup", "kind checkup\n  form due: Text\n  result Text\n  pattern\n    r <- step pr look(due);\n    return due\n", Some("pr")),
            f("delivery", delivery_text(), None),
        ]));
        s.tickets.pic_open.insert(format!("{FOCUS}{pick}"));
        (held, s)
    }

    /// The Docket's Agent scale (p218) on a made-up project, drawn as docket-edge8's switch draws it, pr picked: its
    /// programs alone (question, whose step goes to the lead pr is, and checkup, which pr wrote; not thread, nor
    /// delivery's watcher), whom their steps go to and pr, no other agent; what waits on pr with its thread; the thread
    /// pr works on and its list. A press on a waiting step or a thread opens it; one on qa picks qa, whose scale says
    /// it has no program, and again keeps it; with none picked, it says so and draws no board.
    #[test]
    fn the_agent_scale_draws_the_picked_agents_programs_what_waits_on_it_and_its_threads() {
        let drawn = |rt: &rune::runtime::Runtime<App>, key: &str| rt.find(key).is_some();
        let texts = |rt: &rune::runtime::Runtime<App>| -> Vec<String> { rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect() };
        let (_held, s) = scale_app("pr");
        let mut rt = rune::runtime::Runtime::new(s, scale_view, (1600.0, 1600.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        for key in ["docket-pick-pr", "docket-pick-qa", "docket-board", "docket-kind-question", "docket-kind-checkup", "docket-who-pr", "docket-who-owner", "docket-owed-0", "docket-agent-now", "docket-thread-N-7"] {
            assert!(drawn(&rt, key), "{key} is drawn for pr");
        }
        for key in ["docket-kind-thread", "docket-kind-delivery", "docket-who-qa", "docket-who-watcher", "docket-thread-N-9"] {
            assert!(!drawn(&rt, key), "{key} is not pr's");
        }
        let t = texts(&rt);
        for words in ["Waits on it · 1", "answer", "Q-5", "works on now", "the plan"] {
            assert!(t.iter().any(|x| x == words), "{words:?} in {t:?}");
        }
        press(&mut rt, "docket-owed-0");
        assert_eq!((rt.state.sel.clone(), rt.state.tickets.shown), (crate::Sel::Tickets, Some(5)), "the step's thread opens");
        press(&mut rt, "docket-thread-N-7");
        assert_eq!(rt.state.tickets.shown, Some(7));
        // another agent picked: qa has no program, nothing waits on it, it is on N-9
        press(&mut rt, "docket-pick-qa");
        assert_eq!(picked(&rt.state.tickets.pic_open, FOCUS), Some("qa"));
        for _ in 0..30 {
            rt.frame(0.016);
        }
        assert!(!drawn(&rt, "docket-kind-question") && !drawn(&rt, "docket-board"), "no program sends qa a step");
        assert!(drawn(&rt, "docket-thread-N-9") && !drawn(&rt, "docket-thread-N-7") && !drawn(&rt, "docket-agent-now"));
        let t = texts(&rt);
        for words in ["No program sends qa a step, and qa wrote none.", "Nothing waits on it.", "The threads it is on", "the other plan"] {
            assert!(t.iter().any(|x| x == words), "{words:?} in {t:?}");
        }
        press(&mut rt, "docket-pick-qa");
        assert_eq!(picked(&rt.state.tickets.pic_open, FOCUS), Some("qa"), "picked again, it stays");
        // none picked
        rt.state.tickets.pic_open.retain(|k| !k.starts_with(FOCUS));
        rt.invalidate();
        for _ in 0..30 {
            rt.frame(0.016);
        }
        assert!(drawn(&rt, "docket-pick-pr") && !drawn(&rt, "docket-agent-owed") && !drawn(&rt, "docket"));
        assert!(texts(&rt).iter().any(|x| x.starts_with("No agent picked")));
    }

    /// One count of what waits on each worker, on both scales and as the window counts it (docket-edge8's walk of 73,
    /// N-1446 p2353: "Owed · 1" on the Project scale over "Owed · 20" on the Agent scale, his "Waits on you · 11" over
    /// the Threads page's 13). pr owes Q-5's answer, drawn, and a goal and a seen on N-7, steps no box draws: "Owed · 3"
    /// on both scales (73: 2 on the Project scale, the arrows to the lead's answer, Q-6's among them). His box says the
    /// Threads page's Waits on you: N-9 owes him, at no drawn step (73: nothing). qa took Q-6's answer, a step written
    /// for the lead's role: question is qa's program on the Agent scale, not "No program sends qa a step" over the
    /// step that waits on it.
    #[test]
    fn a_worker_is_owed_one_count_on_both_scales_and_a_step_it_took_makes_its_program_its() {
        use serde_json::json;
        let (_held, mut s) = scale_app("qa");
        let p = &mut s.projects[0];
        let q6 = json!({"id": "Q-6", "n": 6, "kind": "question", "opener": "pr", "ctx": {}, "form": {}, "title": "Q-6", "children": [], "program": "",
            "bound": 1, "spent": 0, "opened_at": "2026-11-20T08:00:00Z", "given": [], "posts": [], "waits_on": [{"ticket": "Q-6", "agent": "qa", "call": "answer"}],
            "status": {"is": "awaits", "agent": "qa", "call": "answer", "step": 1, "since": "2026-11-20T08:00:00Z", "taken": true, "refusals": 0,
                "forming": false, "head": {"Variant": [["Answer", [["text", "Text"]]]]}, "args": {}}});
        p.live.tickets.insert(6, serde_json::from_value(q6).unwrap());
        let n7 = p.live.tickets.get_mut(&7).unwrap();
        n7.waits_on = serde_json::from_value(json!([{"ticket": "N-7", "agent": "pr", "call": "goal"}, {"ticket": "N-7", "agent": "pr", "call": "seen"}])).unwrap();
        p.live.tickets.get_mut(&9).unwrap().owed = true;
        let his = crate::waits::on_owner(p).len();
        assert_eq!(his, 1, "the Threads page's Waits on you: N-9");
        s.sel = crate::Sel::Kinds;
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (1800.0, 1400.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        let says = |rt: &rune::runtime::Runtime<App>, key: &str, words: &str| {
            let t: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting && e.path.contains(&format!("/{key}/"))).filter_map(|e| e.text).collect();
            assert!(t.iter().any(|x| x.starts_with(words)), "{key} says {words:?}: {t:?}");
        };
        assert_eq!(scale(&rt.state.tickets.pic_open), Scale::Project);
        says(&rt, "docket-who-pr", "Owed · 3");
        says(&rt, "docket-who-qa", "Owed · 1");
        says(&rt, "docket-who-owner", &format!("Waits on you · {his}"));
        rt.update(|s, _| set_scale(s, Scale::Agent));
        rt.invalidate();
        frames(&mut rt);
        says(&rt, "docket-pick-pr", "Owed · 3");
        says(&rt, "docket-pick-qa", "Owed · 1");
        let t: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect();
        assert!(rt.find("docket-kind-question").is_some(), "question is qa's: it took its answer");
        assert!(!t.iter().any(|x| x.starts_with("No program sends qa")), "not said over what waits on it: {t:?}");
        assert!(t.iter().any(|x| x == "Waits on it · 1"), "{t:?}");
    }

    /// A made-up project for the board's presses, on agent-base's base, thread, question and delivery: pr the lead and
    /// qa; questions Q-5 and Q-6 at pr's answer, Q-5 asked from thread N-7, which his message opened; pr's clock, whose
    /// last run C-8 closed; an order O-11 that qa opened; a note, which nothing has open. The Kinds page shown.
    fn board_app() -> (std::sync::MutexGuard<'static, ()>, App) {
        use serde_json::json;
        const BASE: &str = include_str!("../../../../agent-base/kinds/base.kind");
        const THREAD: &str = include_str!("../../../../agent-base/kinds/thread.kind");
        const QUESTION: &str = include_str!("../../../../agent-base/kinds/question.kind");
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        let agent = |name: &str, role: &str| -> coord_types::AgentView {
            serde_json::from_value(json!({"id": format!("AGT-{name}"), "name": name, "parent": "owner", "role": role, "kind": "claude", "model": "sonnet-x",
                "state": "working", "since": "2026-11-20T08:00:00Z", "tools": [], "items": []})).unwrap()
        };
        p.live.agents = vec![agent("pr", "lead"), agent("qa", "")];
        // their deliveries' last acts, the newest first: pr kept a post of N-7; qa kept a step of Q-5, and handed Q-6
        // with four more the desk counts, naming a post of N-7 at its foot
        p.live.agents[0].acted = serde_json::from_value(json!([{"act": "keep", "at": "2026-11-20T08:30:00Z", "items": ["N-7.p2"], "why": "comes when its turn ends"}])).unwrap();
        p.live.agents[1].acted = serde_json::from_value(json!([{"act": "deliver", "at": "2026-11-20T08:40:00Z", "items": ["Q-6"], "more_items": 4, "rest": ["N-7.p3"]},
            {"act": "keep", "at": "2026-11-20T08:20:00Z", "items": ["Q-5.s1"], "why": "it reads the log first"}])).unwrap();
        let on = "on owner_message(m) open thread(m.to, m.typed, m.names, m.text, m.addressed, m.follows)";
        let clock = "every 15m open checkup(\"x\")";
        let ticket = |n: u64, id: &str, kind: &str, more: serde_json::Value| -> TicketView {
            let mut v = json!({"id": id, "n": n, "kind": kind, "opener": "qa", "ctx": {}, "form": {}, "title": id, "status": {"is": "resting"},
                "children": [], "program": "", "bound": 1, "spent": 0, "opened_at": "2026-11-20T08:00:00Z", "given": [], "posts": []});
            for (k, x) in more.as_object().unwrap() {
                v[k.as_str()] = x.clone();
            }
            serde_json::from_value(v).unwrap()
        };
        let awaits = json!({"is": "awaits", "agent": "pr", "call": "answer", "step": 1, "since": "2026-11-20T08:00:00Z", "taken": true, "refusals": 0,
            "forming": false, "head": {"Variant": [["Answer", [["text", "Text"]]]]}, "args": {}});
        p.live.tickets.insert(5, ticket(5, "Q-5", "question", json!({"status": awaits, "parent": "N-7"})));
        p.live.tickets.insert(6, ticket(6, "Q-6", "question", json!({"status": awaits})));
        p.live.tickets.insert(7, ticket(7, "N-7", "thread", json!({"opener": "owner", "binding": on})));
        p.live.tickets.insert(8, ticket(8, "C-8", "checkup", json!({"opener": "pr", "binding": clock, "status": {"is": "done", "result": {"head": "Ok"}}, "closed_at": "2026-11-20T08:01:00Z"})));
        p.live.tickets.insert(11, ticket(11, "O-11", "order", json!({})));
        p.labels.set("N-7", "the plan");
        p.live.clocks = vec![coord_types::ClockView { binding: clock.into(), kind: "checkup".into(), every: Some(900), when: None, by: Some("pr".into()), file: "/p/kinds/agents/pr.kind".into(), line: 9, last: Some("2026-11-20T08:00:00Z".into()) }];
        s.projects = vec![p];
        s.cur = 0;
        let f = |kind: &str, text: &str, by: Option<&str>, bindings: &[&str]| -> KindForm {
            serde_json::from_value(json!({"kind": kind, "form": [], "text": text, "doc": "", "by": by, "bindings": bindings})).unwrap()
        };
        s.tickets.kinds = Some(Ok(vec![
            f("base", kind_block(BASE, "base"), None, &[]),
            f("thread", kind_block(THREAD, "thread"), None, &[on]),
            f("question", kind_block(QUESTION, "question"), None, &[]),
            f("checkup", "kind checkup\n  form due: Text\n  result Text\n  pattern\n    r <- step pr look(due);\n    return due\n", Some("pr"), &[clock]),
            f("order", "kind order\n  form what: Text\n  result Text\n  pattern\n    r <- step role lead take(what);\n    return what\n", None, &[]),
            f("note", "kind note\n  form text: Text\n  result Text\n  pattern\n    return text\n", None, &[]),
            f("delivery", delivery_text(), None, &[]),
        ]));
        s.sel = crate::Sel::Kinds;
        (held, s)
    }

    /// Every press on the board opens what it stands for (paper-research N-1396 p223), on agent-base's base, thread,
    /// question and delivery: a count of two their list, each one opening as its link would; his message's start its
    /// runs, and the clock's its next and last firings, whose it is and its closed run; base's `opens question` the open
    /// questions, each as its `Q-…` link opens it; an act what it does; "an agent opens one" what agents opened; a
    /// program with nothing open its card; a worker its part of the Docket, the watcher's too, and pr's says its model.
    /// The same press again, or Put away, puts a list away.
    #[test]
    fn every_press_on_the_board_opens_what_it_stands_for() {
        type Rt = rune::runtime::Runtime<App>;
        let texts = |rt: &Rt| -> Vec<String> { rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect() };
        let key_of = |rt: &Rt, prefix: &str| -> String { rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.key).find(|k| k.starts_with(prefix)).unwrap_or_else(|| panic!("a {prefix}… is drawn")) };
        let panel = |rt: &Rt| picked(&rt.state.tickets.pic_open, PANEL).map(str::to_string);
        let back = |rt: &mut Rt| {
            rt.update(|s, _| {
                s.sel = crate::Sel::Kinds;
                set_scale(s, Scale::Project);
            });
            frames(rt);
        };
        let (_held, s) = board_app();
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (1800.0, 3000.0), rune::Env::new(false, 1.0));
        frames(&mut rt);
        // a count of two: their list under the board, each one opening as its link would
        press(&mut rt, "docket-count-question-5");
        assert_eq!(panel(&rt).as_deref(), Some("count:question:answer"));
        assert!(rt.find("docket-panel-Q-5").is_some() && rt.find("docket-panel-Q-6").is_some());
        assert!(texts(&rt).iter().any(|t| t == "question, at answer · 2"), "{:?}", texts(&rt));
        press(&mut rt, "docket-panel-Q-6");
        assert_eq!((rt.state.sel.clone(), rt.state.tickets.shown), (crate::Sel::Tickets, Some(6)));
        back(&mut rt);
        press(&mut rt, "docket-count-question-5");
        assert_eq!(panel(&rt), None, "the same press again puts it away");
        // his message's start: its runs
        press(&mut rt, "docket-start-thread-0");
        assert_eq!(panel(&rt).as_deref(), Some("start:thread:0"));
        assert!(texts(&rt).iter().any(|t| t.starts_with("Opens thread, as written: on owner_message")));
        press(&mut rt, "docket-panel-N-7");
        assert_eq!(rt.state.tickets.shown, Some(7));
        back(&mut rt);
        // the clock's: its next firing, its last, whose it is, its closed run
        press(&mut rt, "docket-start-checkup-0");
        let t = texts(&rt);
        for w in ["next at ", "last ", "pr's own program", "closed"] {
            assert!(t.iter().any(|x| x.starts_with(w)), "{w:?} in {t:?}");
        }
        assert!(rt.find("docket-panel-C-8").is_some());
        // base's `opens question`: the open questions, each as its Q-… link opens it
        let opens = key_of(&rt, "docket-opens-base-");
        press(&mut rt, &opens);
        assert_eq!(panel(&rt).as_deref(), Some("opens:question"));
        assert!(texts(&rt).iter().any(|t| t == "from N-7"));
        press(&mut rt, "docket-panel-Q-5");
        assert_eq!((rt.state.sel.clone(), rt.state.tickets.shown), (crate::Sel::Tickets, Some(5)));
        back(&mut rt);
        // an act: what it does (the first drawn is `arrived`'s `fork` of a copy)
        let act = key_of(&rt, "docket-act-delivery-");
        press(&mut rt, &act);
        assert!(panel(&rt).is_some_and(|w| w.starts_with("act:")));
        assert!(texts(&rt).iter().any(|t| ["Hands what waits", "Hands it to a copy", "Holds it for", "Can't deliver"].iter().any(|w| t.starts_with(w))));
        press(&mut rt, "docket-panel-close");
        assert_eq!(panel(&rt), None, "put away");
        // what agents open: the order qa opened
        press(&mut rt, "docket-start-agents");
        assert!(rt.find("docket-panel-O-11").is_some() && texts(&rt).iter().any(|t| t == "opened by qa"));
        // a program with nothing open: its card
        press(&mut rt, "docket-rest-note");
        assert_eq!(shown_kind(&rt.state.tickets.pic_open), Some("note"));
        // the watcher, no agent: its part of the Docket, picked beside the agents
        press(&mut rt, "docket-who-watcher");
        assert_eq!((rt.state.sel.clone(), scale(&rt.state.tickets.pic_open), picked(&rt.state.tickets.pic_open, FOCUS)), (crate::Sel::Kinds, Scale::Agent, Some("watcher")));
        assert!(rt.find("docket-pick-watcher").is_some() && rt.find("docket-kind-thread").is_some(), "thread sends the watcher its intake");
        assert!(rt.find("docket-kind-question").is_none(), "question sends it nothing");
        // pr: who it is now
        back(&mut rt);
        press(&mut rt, "docket-who-pr");
        assert_eq!(picked(&rt.state.tickets.pic_open, FOCUS), Some("pr"));
        assert!(texts(&rt).iter().any(|t| t == "pr runs on sonnet-x."));
        // an act's last lines (the agents' `acted`), the newest first: when, whose, its items each opening, its why
        back(&mut rt);
        rt.update(|s, _| show_panel(s, "act:keep"));
        frames(&mut rt);
        let t = texts(&rt);
        for w in ["why: comes when its turn ends", "why: it reads the log first"] {
            assert!(t.iter().any(|x| x == w), "{w:?} in {t:?}");
        }
        assert!(rt.find("docket-acted-1").is_some() && rt.find("docket-acted-2").is_none(), "the two keeps");
        press(&mut rt, "docket-act-1-0");
        assert_eq!((rt.state.sel.clone(), rt.state.tickets.shown), (crate::Sel::Tickets, Some(5)), "qa's kept step opens its ticket");
        back(&mut rt);
        press(&mut rt, "docket-act-0-0");
        assert_eq!(rt.state.tickets.shown, Some(7), "pr's kept post opens its thread");
        back(&mut rt);
        rt.update(|s, _| show_panel(s, "act:deliver"));
        frames(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "and 4 more") && t.iter().any(|x| x == "named at its foot:"), "{t:?}");
        press(&mut rt, "docket-act-0-rest-0");
        assert_eq!(rt.state.tickets.shown, Some(7), "the post named at its foot opens its thread");
        back(&mut rt);
        // on pr's Agent scale, pr's alone
        rt.update(|s, _| {
            show_panel(s, "act:keep");
            open_agent(s, "pr");
        });
        frames(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "why: comes when its turn ends") && !t.iter().any(|x| x == "why: it reads the log first"), "{t:?}");
    }

    /// The Kinds page of `board_app` with WHAT open under the board.
    fn board_with(what: &str) -> App {
        let mut s = board_app().1;
        s.tickets.pic_open.insert(format!("{PANEL}{what}"));
        s
    }

    /// STILL_OUT=DIR: the Docket at Project on `still_app`'s made-up data, its board alone (delivery-row.png): the
    /// delivery program's rows, `free`'s ending in its deliver (proto-lead N-1481 p109).
    #[test]
    #[ignore]
    fn delivery_row_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn project() -> App {
            pressed(still_app(), &["docket-mark", "docket-scale/segment0"])
        }
        let snap = out.join("delivery-row.png").display().to_string();
        let args = vec!["--snap".to_string(), snap, "--size".into(), "1800x1400".into(), "--at".into(), "0.5".into(), "--crop".into(), "docket-board".into()];
        let spec = rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1800.0, 1400.0), project, crate::view) };
        rune::App::new(spec).args(args).try_run().unwrap();
    }

    /// STILL_OUT=DIR: the Agent scale alone on made-up data, pr picked (agent-scale-pr.png) and qa (agent-scale-qa.png);
    /// the board of `board_app` with a count's list (board-count.png), the clock's runs (board-clock.png), the open
    /// questions (board-opens.png), and the last keeps and delivers (board-keeps.png, board-delivers.png) under it.
    #[test]
    #[ignore]
    fn agent_scale_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn pr() -> App {
            scale_app("pr").1
        }
        fn qa() -> App {
            scale_app("qa").1
        }
        fn count() -> App {
            board_with("count:question:answer")
        }
        fn clock() -> App {
            board_with("start:checkup:0")
        }
        fn opens() -> App {
            board_with("opens:question")
        }
        fn keeps() -> App {
            board_with("act:keep")
        }
        fn delivers() -> App {
            board_with("act:deliver")
        }
        type View = fn(&App, &mut rune::Cx<App>) -> El;
        let shots: [(&str, fn() -> App, View, &str, (f32, f32)); 7] = [
            ("agent-scale-pr", pr, scale_view, "scale", (1600.0, 1400.0)),
            ("agent-scale-qa", qa, scale_view, "scale", (1600.0, 1400.0)),
            ("board-count", count, crate::view, "docket", (1800.0, 3000.0)),
            ("board-clock", clock, crate::view, "docket", (1800.0, 3000.0)),
            ("board-opens", opens, crate::view, "docket", (1800.0, 3000.0)),
            ("board-keeps", keeps, crate::view, "docket", (1800.0, 3000.0)),
            ("board-delivers", delivers, crate::view, "docket", (1800.0, 3000.0)),
        ];
        for (name, setup, view, crop, (w, h)) in shots {
            let snap = out.join(format!("{name}.png")).display().to_string();
            let size = format!("{w}x{h}");
            let args: [&str; 8] = ["--snap", &snap, "--crop", crop, "--size", &size, "--at", "0.5"];
            let spec = rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (w, h), setup, view) };
            rune::App::new(spec).args(args.map(String::from).to_vec()).try_run().unwrap();
        }
    }
}
