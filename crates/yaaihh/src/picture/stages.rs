//! A ticket's program as its stages (the owner N-281.p1, paper-research N-101 p748): the first view of a program
//! that does work fits one screen. Its stages go down in order: each step, each ticket it opens, each part run in
//! parallel. The one it is at is marked, what is done is ticked, and a branch not taken is folded away. A stage
//! speaks in its declaration's doc (a call's or a kind's `///`), not in code; where there is no doc its name stands
//! alone, and the code is what the hover card shows. A stage opens on a click and shows its card as the whole
//! picture draws it, so its edit and its hover are the same as there. The whole program, every branch, stays a click
//! away under it.
//!
//! The walk follows the picture's (`Pic::block`), with its keys, so an opened stage is the same element as the
//! whole picture's card for that statement. Which arm of a `match` is shown: the one the ticket took, else, before
//! it is reached, the arm with the most stages (the way the work goes; an arm that only returns or raises is an
//! exit, not a stage).

use super::*;
use std::collections::HashMap;
use std::path::PathBuf;

/// The docs of a project's calls and kinds, by name: the first line of each one's `///`.
#[derive(Default, Debug)]
pub struct Labels {
    pub calls: HashMap<String, String>,
    pub kinds: HashMap<String, String>,
}

/// The labels of the calls and kinds in FOLDERS, read as coordd reads them (a folder with `kinds/` or `calls/` holds
/// them there, else the folder itself does; the system's types first). A file that does not parse gives none.
pub fn read_labels(folders: &[PathBuf]) -> Labels {
    let place = |f: &PathBuf, sub: &str| if f.join("kinds").is_dir() || f.join("calls").is_dir() { f.join(sub) } else { f.clone() };
    let files = |dir: PathBuf, ext: &str| -> Vec<(String, String)> {
        let mut ps: Vec<PathBuf> = std::fs::read_dir(&dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == ext)).collect()).unwrap_or_default();
        ps.sort();
        ps.into_iter().filter_map(|p| Some((p.display().to_string(), std::fs::read_to_string(&p).ok()?))).collect()
    };
    let first = |t: &str| t.lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_string);
    let mut out = Labels::default();
    let calls: Vec<(String, String)> = folders.iter().flat_map(|f| files(place(f, "calls"), "call")).collect();
    let mut named = syntax::Named::new();
    let _ = syntax::parse_types("system", syntax::SYSTEM_TYPES, &mut named);
    let (defs, _) = syntax::parse_call_files(&calls, &mut named);
    for (name, d) in defs {
        if let Some(l) = first(&d.doc.text) {
            out.calls.insert(name, l);
        }
    }
    for (path, text) in folders.iter().flat_map(|f| files(place(f, "kinds"), "kind")) {
        if let Ok((kinds, _)) = parse_file(&path, &text) {
            for kd in kinds {
                if let Some(l) = first(&kd.doc.text) {
                    out.kinds.insert(kd.name, l);
                }
            }
        }
    }
    out
}

/// One stage: the statement, its run node, where it is drawn (the picture's key and names), and, for a par or an
/// any, its lanes. The Docket (docket.rs) draws a program's row from the same walk.
pub(super) struct St<'p> {
    pub(super) s: &'p Prog,
    node: Option<RunNode>,
    at: At,
    /// "round 2 of 3": the round of the `iterate` it is in, once past the first.
    round: Option<String>,
    pub(super) lanes: Vec<Lane<'p>>,
    /// The constructor of the `match` arm it is the first stage of (`Thread`, `Up`; none for any other pattern, which
    /// is code, not words: `_`, a name, `[]`): what the Docket writes on the arrow into it (0.2 line 26).
    pub(super) case: Option<String>,
}

pub(super) struct Lane<'p> {
    title: String,
    state: State,
    ticket: Option<String>,
    pub(super) stages: Vec<St<'p>>,
}

/// The stages of block P (its run nodes RUN), drawn at AT, into OUT.
pub(super) fn collect<'p>(p: &'p Prog, run: Option<&[RunNode]>, at: &At, round: &Option<String>, out: &mut Vec<St<'p>>) {
    let mut at = at.clone();
    for (i, (s, bind)) in flat(p).into_iter().enumerate() {
        let (what, _) = docket_machine::print::node(s);
        let node = run.and_then(|r| r.get(i)).filter(|r| r.what == what);
        let here = At { key: format!("{}.{i}", at.key), ..at.clone() };
        one(s, node, &here, round, out);
        if let (Some(b), Some(x)) = (&bind, node.and_then(|n| n.value.clone())) {
            at.env.push((b.clone(), x, node.and_then(|n| n.ty.clone())));
        }
    }
}

fn one<'p>(s: &'p Prog, node: Option<&RunNode>, at: &At, round: &Option<String>, out: &mut Vec<St<'p>>) {
    let kids = node.map(|n| n.nodes.as_slice());
    let stage = |lanes| St { s, node: node.cloned(), at: at.clone(), round: round.clone(), lanes, case: None };
    match s {
        // the message runtime's acts too (delivery.kind's `deliver`, `fork`, `keep`, `report`): what its handlers do
        Prog::Step { .. } | Prog::Open { .. } | Prog::Decide { .. } | Prog::Hole { .. } | Prog::Deliver { .. } | Prog::Fork { .. } | Prog::Keep { .. } | Prog::Report { .. } => out.push(stage(Vec::new())),
        Prog::Match { e, arms, guards, .. } => {
            let taken = (0..arms.len()).find(|i| node.and_then(|n| n.nodes.get(*i)).is_some_and(|a| a.what == "arm" && matches!(a.state.as_str(), "done" | "now")));
            // none taken: its main way, the arm with the most stages; of those, one with no `where` (what the match does
            // otherwise; a guarded arm is a case apart), then the first (proto-lead N-1481 p109: delivery's `free` drew
            // its `[] where e.writing => keep` over the arm that delivers)
            let i = taken.unwrap_or_else(|| (0..arms.len()).max_by_key(|i| (count(&arms[*i].1), !matches!(guards.get(*i), Some(Some(_))), usize::MAX - i)).unwrap_or(0));
            let Some((pat, body)) = arms.get(i) else { return };
            let mut inner = At { key: format!("{}.a{i}", at.key), ..at.clone() };
            if taken.is_some() {
                crate::ran::bind_arm(&mut inner.env, &docket_machine::print::expr(e), &docket_machine::print::pat(pat));
            }
            let an = node.and_then(|n| n.nodes.get(i)).filter(|n| n.what == "arm");
            let before = out.len();
            collect(body, an.map(|n| n.nodes.as_slice()), &inner, round, out);
            if let Some(first) = out.get_mut(before).filter(|st| st.case.is_none()) {
                first.case = match pat {
                    Pat::Con { name, .. } => Some(name.clone()),
                    _ => None,
                };
            }
        }
        Prog::Iterate { n, body, .. } => {
            // past the first round the run tree still holds the round before after the point at work: left to come
            let r = node.and_then(|x| first_step(&x.nodes)).map(|s| s.rounds.len()).unwrap_or(0);
            let nodes = match node {
                Some(x) if r >= 2 => {
                    let mut ns = x.nodes.clone();
                    left_over(&mut ns, &mut false, &mut None);
                    Some(ns)
                }
                _ => node.map(|x| x.nodes.clone()),
            };
            let round = if r >= 2 { Some(if *n == docket_machine::syntax::UNBOUNDED { format!("round {r}") } else { format!("round {r} of {n}") }) } else { round.clone() };
            collect(body, nodes.as_deref(), &At { key: format!("{}.i", at.key), ..at.clone() }, &round, out);
        }
        Prog::For { body, .. } => collect(body, kids, &At { key: format!("{}.e", at.key), ..at.clone() }, round, out),
        Prog::Try { body, .. } => {
            let nb = flat(body).len();
            collect(body, kids.map(|r| &r[..nb.min(r.len())]), &At { key: format!("{}.t", at.key), ..at.clone() }, round, out);
        }
        Prog::With { body, .. } | Prog::Layer { body, .. } => collect(body, kids, &At { key: format!("{}.f", at.key), ..at.clone() }, round, out),
        Prog::LetAgent { x, from, body, .. } => {
            let mut inner = at.clone();
            inner.made.push(x.clone());
            if let Some(a) = from {
                inner.forks.push((x.clone(), docket_machine::print::expr(a)));
            }
            collect(body, kids, &inner, round, out);
        }
        Prog::During { body, .. } => collect(body, kids, &At { key: format!("{}.d", at.key), ..at.clone() }, round, out),
        Prog::Par { branches, each, .. } | Prog::Any { branches, each, .. } => {
            let run: Vec<&RunNode> = node.map(|n| n.nodes.iter().filter(|b| b.what == "branch").collect()).unwrap_or_default();
            let tickets: &[String] = node.map(|n| n.tickets.as_slice()).unwrap_or_default();
            // the elements a par over a list runs over, for its lanes' names (a part's `name`)
            let elems: Vec<Value> = each
                .as_ref()
                .and_then(|x| match &x.e {
                    Expr::Var(v) => at.env.iter().rev().find(|(n, _, _)| n == v).and_then(|(_, val, _)| val.as_array().cloned()),
                    _ => None,
                })
                .unwrap_or_default();
            let lane = |i: usize, body: &'p Prog, bn: Option<&RunNode>| {
                let mut stages = Vec::new();
                collect(body, bn.map(|n| n.nodes.as_slice()), &At { key: format!("{}.b{i}", at.key), in_lane: true, ..at.clone() }, round, &mut stages);
                let name = elems.get(i).and_then(|e| e.get("name").and_then(Value::as_str).map(str::to_string).or_else(|| e.as_str().map(str::to_string)));
                Lane { title: name.unwrap_or_else(|| format!("part {}", i + 1)), state: state_of(bn), ticket: tickets.get(i).cloned(), stages }
            };
            let lanes = match (each.is_some(), run.is_empty()) {
                (true, true) => vec![Lane { title: format!("one for each {}", each.as_ref().map(|x| x.x.clone()).unwrap_or_default()), ..lane(0, &branches[0], None) }],
                (true, false) => run.iter().enumerate().map(|(i, bn)| lane(i, &branches[0], Some(bn))).collect(),
                (false, _) => branches.iter().enumerate().map(|(i, b)| lane(i, b, run.get(i).copied())).collect(),
            };
            out.push(stage(lanes));
        }
        _ => {}
    }
}

/// How many stages P holds down its main way: a match counts the arm it draws, its largest.
fn count(p: &Prog) -> usize {
    let mut v = Vec::new();
    collect(p, None, &At::default(), &None, &mut v);
    v.len()
}

/// The key in `pic_open` that says stage KEY of a ticket is the one shown in its panel (one per ticket, whose id the
/// key starts with).
pub fn shown_key(key: &str) -> String {
    format!("sel:{key}")
}

/// The stage of ticket TID shown in its panel, if one is: its key.
pub fn shown_stage(open: &std::collections::HashSet<String>, tid: &str) -> Option<String> {
    let pre = format!("sel:{tid}.");
    open.iter().find(|k| k.starts_with(&pre)).map(|k| k["sel:".len()..].to_string())
}

/// The longest a stage's label is in its row: two lines' worth; the whole is in its panel.
const STAGE_CHARS: usize = 90;

impl<'a> Pic<'a> {
    /// Kind TEXT's stages, lit by V when given; KEY names it on the page (the ticket's id).
    pub fn stages(&self, program: &str, v: Option<&TicketView>, key: &str, labels: &Labels) -> El {
        let k = self.k;
        let kd = match program_kind(program, v.map_or("", |v| v.kind.as_str())) {
            Ok(x) => x,
            Err(e) => return text_el(format!("Its program could not be drawn: {e}"), k).color(k.c.danger.fg),
        };
        let lit = v.map(|v| Lit { v, halted: v.halts.iter().filter(|h| !matches!(h, ViewHalt::Deliver { .. })).collect() });
        let at = At { key: key.to_string(), env: v.map(form_env).unwrap_or_default(), ..At::default() };
        let mut sts = Vec::new();
        collect(&kd.pattern, v.map(|v| v.run.as_slice()), &at, &None, &mut sts);
        let shown = shown_stage(self.open, key);
        let mut c = col().key(format!("stages-{key}")).w_full().gap(2.0);
        for st in &sts {
            c = c.child(self.stage(st, lit.as_ref(), labels, 0, shown.as_deref()));
        }
        c
    }

    /// Where ticket V is now, in words: the part it is in (a lane's ticket) and the step at work there, with who
    /// does it; the ids small after each. None when nothing is at work.
    pub fn where_now(&self, v: &TicketView, labels: &Labels) -> Option<El> {
        let k = self.k;
        let kd = program_kind(&v.program, &v.kind).ok()?;
        let mut sts = Vec::new();
        collect(&kd.pattern, Some(&v.run), &At { env: form_env(v), ..At::default() }, &None, &mut sts);
        // the lane at work, if the ticket is in a par: its ticket is the part the owner thinks of
        let lane = sts.iter().filter(|st| state_of(st.node.as_ref()) == State::Now).find_map(|st| st.lanes.iter().find(|l| l.state == State::Now));
        let (deep, words, who) = self.deepest(&v.id, labels, 0)?;
        let mut r = row().w_full().gap(6.0).flex_wrap().center_y();
        let part = lane.map(|l| self.lane_ref(l));
        // "part about-note P-2 is with runner now: hand in": the part to read, who has it, and the step in its own words
        // (a doc's phrase cannot be turned into a verb, so it stands after a colon)
        if let Some(part) = &part {
            r = r.child(crate::refs::el(part, k.ty.small_medium, k.c.text_base, k));
        }
        let with = match (&part, who) {
            (Some(_), Some(w)) => format!("is with {} now: {words}", w.label),
            (Some(_), None) => format!("is now at: {words}"),
            (None, Some(w)) => format!("With {} now: {words}", w.label),
            (None, None) => format!("Now at: {words}"),
        };
        r = r.child(text(with).font(k.ty.small).color(k.c.text_base).wrap());
        if part.as_ref().map(|p| p.id.as_str()) != Some(deep.as_str()) && deep != v.id {
            r = r.child(text(deep.clone()).font(k.ty.small).color(k.c.text_faint));
        }
        Some(r)
    }

    /// Who has lane L's part: the agent the step that picked the parts named for it (a list of `{piece, agent}` in an
    /// answer on the ticket above), else the agent its ticket awaits.
    fn lane_who(&self, l: &Lane) -> Option<String> {
        let id = l.ticket.as_deref()?;
        let t = self.tickets.iter().find(|t| t.id == id)?;
        if let ViewStatus::Awaits { agent, .. } = &t.status {
            return Some(agent.clone());
        }
        let name = t.title.strip_prefix("part ").unwrap_or(&t.title).trim();
        let up = self.tickets.iter().find(|u| Some(u.id.as_str()) == t.parent.as_deref())?;
        up.posts.iter().rev().filter_map(|p| p.head.as_ref()?.as_object()).flat_map(|h| h.values()).filter_map(Value::as_array).flatten().find_map(|x| {
            let piece = x.get("piece")?.as_str()?;
            (piece == name || piece == l.title).then(|| x.get("agent")?.as_str().map(str::to_string)).flatten()
        })
    }

    /// A lane's reference: coordd's label of its ticket, else the element's own words, the ticket's id after it.
    fn lane_ref(&self, l: &Lane) -> crate::refs::Ref {
        let id = l.ticket.clone().unwrap_or_default();
        let label = l.ticket.as_deref().and_then(|t| self.p?.labels.get(t)).map_or_else(|| l.title.clone(), str::to_string);
        crate::refs::Ref { label, id }
    }

    /// The ticket at work under ID, down the tickets each opened, its stage in words and who does it.
    fn deepest(&self, id: &str, labels: &Labels, depth: usize) -> Option<(String, String, Option<crate::refs::Ref>)> {
        let v = self.tickets.iter().find(|v| v.id == id)?;
        if matches!(v.status, ViewStatus::Done { .. } | ViewStatus::Failed { .. }) {
            return Some((id.to_string(), ticket_state(v), None));
        }
        let kd = program_kind(&v.program, &v.kind).ok()?;
        let mut sts = Vec::new();
        collect(&kd.pattern, Some(&v.run), &At { env: form_env(v), ..At::default() }, &None, &mut sts);
        let now = now_in(&sts)?;
        if let (Prog::Open { .. }, Some(t), true) = (now.s, now.node.as_ref().and_then(|n| n.tickets.first()), depth < 6) {
            if let Some(more) = self.deepest(t, labels, depth + 1) {
                return Some(more);
            }
        }
        // who has it now: the agent the step is with (a step passed up is with another than the program names)
        let who = match &v.status {
            ViewStatus::Awaits { agent, .. } => Some(crate::refs::who(self.p, agent)),
            _ => now.node.as_ref().and_then(|n| n.who.clone()).map(|w| crate::refs::who(self.p, &w)),
        };
        Some((id.to_string(), self.words_of(now, labels), who))
    }

    /// A stage's row: its mark, its words, who does it and its round; a click shows it in the ticket's panel
    /// (SHOWN: the stage shown there). A par's lanes go under it, each with its own stages.
    fn stage(&self, st: &St, lit: Option<&Lit>, labels: &Labels, depth: usize, shown: Option<&str>) -> El {
        let k = self.k;
        let node = st.node.as_ref();
        let state = state_of(node);
        let key = format!("stage-{}", st.at.key);
        let picked = shown == Some(st.at.key.as_str());
        let (mark, ink_) = match state {
            State::Done => ("✓", k.c.success.fg),
            State::Now => ("●", k.c.text_accent),
            State::NotTaken => ("–", k.c.text_faint),
            State::ToCome | State::Plain => ("○", k.c.text_faint),
        };
        let whole = self.words_of(st, labels);
        let words = crate::refs::cut(&whole, STAGE_CHARS);
        let mut side: Vec<El> = Vec::new();
        // the step at work is with the agent its ticket awaits, which a step passed up makes another than the named
        let now_with = lit.and_then(|l| match &l.v.status {
            ViewStatus::Awaits { agent, .. } if state == State::Now && node.is_some_and(|n| n.tickets.is_empty()) => Some(agent.clone()),
            _ => None,
        });
        if let Some(w) = now_with.or_else(|| node.and_then(|n| n.who.clone()).filter(|_| matches!(state, State::Done | State::Now))) {
            side.push(crate::refs::el(&crate::refs::who(self.p, &w), k.ty.small, k.c.text_muted, k));
        }
        if let Some(r) = &st.round {
            side.push(text(r.clone()).font(k.ty.small).color(k.c.text_muted));
        }
        if let (Some(t), Some(p)) = (node.and_then(|n| n.tickets.first()).filter(|_| matches!(st.s, Prog::Open { .. })), self.p) {
            side.push(text(crate::refs::ticket_id(p, t).id).font(k.ty.small).color(k.c.text_faint));
        }
        let halted = lit.is_some_and(|l| halts_here(l, node));
        // cut to two lines; the whole is the first line of its panel
        let words_el = text(words).font(if state == State::Now { k.ty.body_medium } else { k.ty.body }).color(if matches!(state, State::ToCome | State::NotTaken) { k.c.text_muted } else { k.c.text_base }).wrap().max_lines(2);
        let mut line = row()
            .w_full()
            .gap(10.0)
            .center_y()
            .px(10.0)
            .py(7.0)
            .radius(k.radius.sm)
            .child(text(mark.to_string()).font(k.ty.small_medium).color(ink_).w(14.0))
            .child(col().w(0.0).grow(1.0).child(words_el))
            .child(row().gap(8.0).center_y().children(side));
        if picked {
            line = line.fill(k.c.layer_02).border(1.0, k.c.border_focus);
        } else if halted {
            line = line.border(1.0, k.c.warning.fg);
        } else if state == State::Now {
            line = line.fill(k.c.layer_01);
        }
        let toggle = {
            let (sel, tid) = (shown_key(&st.at.key), st.at.key.split('.').next().unwrap_or("").to_string());
            k.interactive(line).on_press(move |s: &mut App| {
                let pre = format!("sel:{tid}.");
                let was = s.tickets.pic_open.remove(&sel);
                s.tickets.pic_open.retain(|k| !k.starts_with(&pre));
                if !was {
                    s.tickets.pic_open.insert(sel.clone());
                }
            })
        };
        let mut c = col().key(key).w_full().gap(2.0).child(toggle);
        if !st.lanes.is_empty() && depth < 3 {
            let mut lanes = col().w_full().gap(6.0).pl(24.0);
            for l in &st.lanes {
                let (lm, li) = match l.state {
                    State::Done => ("✓", k.c.success.fg),
                    State::Now => ("●", k.c.text_accent),
                    _ => ("○", k.c.text_faint),
                };
                let mut lc = col().w_full().gap(2.0).pl(10.0).border_sides([0.0, 0.0, 0.0, 2.0], if l.state == State::Now { k.c.border_focus } else { k.c.border_weak });
                let r = self.lane_ref(l);
                let mut head = row().w_full().gap(8.0).center_y().py(4.0).child(text(lm.to_string()).font(k.ty.small_medium).color(li)).child(crate::refs::el(&r, k.ty.body_medium, k.c.text_base, k));
                // who has the part: the agent picked for it
                if let Some(w) = self.lane_who(l) {
                    head = head.child(el().grow(1.0)).child(crate::refs::el(&crate::refs::who(self.p, &w), k.ty.small, k.c.text_muted, k));
                }
                lc = lc.child(head);
                for s in &l.stages {
                    lc = lc.child(self.stage(s, lit, labels, depth + 1, shown));
                }
                lanes = lanes.child(lc);
            }
            c = c.child(lanes);
        }
        c
    }

    /// Stage KEY of kind TEXT (lit by V) as the ticket's panel shows it: its words, its state and who, the ticket it
    /// opened, and its card as the whole picture draws it (its edit and hover the same as there).
    pub fn stage_panel(&self, program: &str, v: &TicketView, key: &str, labels: &Labels) -> Option<El> {
        let k = self.k;
        let kd = program_kind(program, &v.kind).ok()?;
        let lit = Lit { v, halted: v.halts.iter().filter(|h| !matches!(h, ViewHalt::Deliver { .. })).collect() };
        let mut sts = Vec::new();
        collect(&kd.pattern, Some(&v.run), &At { key: v.id.clone(), env: form_env(v), ..At::default() }, &None, &mut sts);
        let st = find(&sts, key)?;
        let node = st.node.as_ref();
        let state = match state_of(node) {
            State::Done => "done",
            State::Now => "at work",
            State::NotTaken => "not taken",
            State::ToCome | State::Plain => "to come",
        };
        let mut c = col().key(format!("stage-panel-{key}")).w_full().gap(10.0);
        c = c.child(text(self.words_of(st, labels)).font(k.ty.body_medium).color(k.c.text_base).wrap());
        let mut facts = row().w_full().gap(8.0).flex_wrap().center_y().child(text(state.to_string()).font(k.ty.small).color(k.c.text_muted));
        if let Some(w) = node.and_then(|n| n.who.clone()) {
            facts = facts.child(text("·".to_string()).font(k.ty.small).color(k.c.text_faint)).child(crate::refs::el(&crate::refs::who(self.p, &w), k.ty.small, k.c.text_base, k));
        }
        if let Some(r) = &st.round {
            facts = facts.child(text(format!("· {r}")).font(k.ty.small).color(k.c.text_muted));
        }
        c = c.child(facts);
        // down to the wire: what its call was sent and what came back, each round given (proto-window's Wire layer,
        // N-1446 p308)
        c = c.children(node.and_then(|n| wire_presses(&v.id, n, key, k)));
        if let (Some(t), Some(p)) = (node.and_then(|n| n.tickets.first()).filter(|_| matches!(st.s, Prog::Open { .. })), self.p) {
            let r = crate::refs::ticket_id(p, t);
            c = c.child(row().gap(6.0).center_y().child(text("opened".to_string()).font(k.ty.small).color(k.c.text_muted)).child(crate::refs::el(&r, k.ty.small_medium, k.c.text_base, k)));
            if let Some((_, words, who)) = self.deepest(t, labels, 0) {
                let now = match who {
                    Some(w) => format!("With {} now: {words}", w.label),
                    None => format!("Now at: {words}"),
                };
                c = c.child(text(now).font(k.ty.small).color(k.c.text_muted).wrap());
            }
        }
        // what it gave, when it is done and gave a value
        if let Some(x) = node.and_then(|n| n.value.as_ref()).filter(|x| !x.is_null()) {
            let said = (self.words)(x);
            if !said.trim().is_empty() {
                c = c.child(text("It gave".to_string()).font(k.ty.small_medium).color(k.c.text_muted)).child(text(crate::refs::cut(&said, 600)).font(k.ty.small).color(k.c.text_base).wrap().selectable(true));
            }
        }
        // the step itself, in code, folded (paper-research N-101 p807: the first view is who, its state, its answer)
        let fold = format!("step-code-{key}");
        let shown = self.open.contains(&fold);
        c = c.child(k.interactive(row().pt(4.0).child(text(if shown { "⌄ its step" } else { "› its step" }).font(k.ty.small).color(k.c.text_accent))).on_press(move |s: &mut App| {
            if !s.tickets.pic_open.remove(&fold) {
                s.tickets.pic_open.insert(fold.clone());
            }
        }));
        if shown {
            c = c.child(self.statement(st.s, node, Some(&lit), &st.at));
        }
        Some(c)
    }

    /// A stage in words: its call's or kind's doc, else its name; an inline step its prompt's start.
    fn words_of(&self, st: &St, labels: &Labels) -> String {
        stage_words(st, labels)
    }
}

/// Stage ST in words ([`Pic::words_of`]), with no picture to draw it in: the Map's steps say it too (`reached`).
fn stage_words(st: &St, labels: &Labels) -> String {
    let plain = |n: &str| n.replace('_', " ");
    match st.s {
        Prog::Step { call, inline, .. } => match inline {
            Some((t, _)) => quoted(t, 9),
            None => labels.calls.get(call).cloned().unwrap_or_else(|| plain(call)),
        },
        Prog::Open { kind, .. } => labels.kinds.get(kind).cloned().unwrap_or_else(|| format!("a ticket of its own: {}", plain(kind))),
        Prog::Decide { name, .. } => format!("decided by the policy, or you: {}", plain(name)),
        Prog::Hole { .. } => "waits for the minder".into(),
        Prog::Par { each: Some(x), .. } => format!("in parallel, one for each {}", x.x),
        Prog::Par { .. } => "in parallel".into(),
        Prog::Any { n, .. } => format!("the first {n} of these"),
        _ => docket_machine::print::node(st.s).1,
    }
}

/// Stage KEY's rounds on the wire, in ticket ID's panel: each round of step N given, a press down to the Wire layer at
/// `ID.sSTEP` (lens.rs `zoom`, as the Events row's "Wire ›"; proto-window N-1446 p308), "Wire ›" for a step given
/// once, "round R · step N ›" for each when it is given round after round, so its words meet the Wire crumbs' "step N"
/// (proto-reader p363). None before its first giving.
fn wire_presses(id: &str, n: &RunNode, key: &str, k: &Kit) -> Option<El> {
    let given: Vec<(usize, u32)> = n.rounds.iter().enumerate().filter(|(_, r)| r.given.is_some() || r.answered.is_some()).map(|(i, r)| (i + 1, r.step)).collect();
    if given.is_empty() {
        return None;
    }
    let once = n.rounds.len() == 1;
    let mut r = row().key(format!("stage-panel-{key}-wire")).w_full().gap(6.0).flex_wrap().center_y();
    if !once {
        r = r.child(text("On the wire".to_string()).font(k.ty.small).color(k.c.text_muted));
    }
    for (round, step) in given {
        let (node, words) = (format!("{id}.s{step}"), if once { "Wire ›".to_string() } else { format!("round {round} · step {step} ›") });
        let press = rune_kit::Button::new(words).variant(rune_kit::ButtonVariant::Ghost).control_size(rune_kit::ControlSize::Small).help("What was sent and what came back");
        r = r.child(press.on_press(move |s: &mut App| crate::lens::zoom(s, crate::lens::Depth::Wire, &node)).build(k).key(format!("stage-panel-{key}-wire-{step}")));
    }
    Some(r)
}

/// The stage drawn at KEY down STS, into the lanes.
fn find<'s, 'p>(sts: &'s [St<'p>], key: &str) -> Option<&'s St<'p>> {
    sts.iter().find_map(|st| if st.at.key == key { Some(st) } else { st.lanes.iter().find_map(|l| find(&l.stages, key)) })
}

/// The stage at work down STS, into the lanes of a par.
fn now_in<'s, 'p>(sts: &'s [St<'p>]) -> Option<&'s St<'p>> {
    for st in sts {
        if state_of(st.node.as_ref()) == State::Now {
            return st.lanes.iter().find_map(|l| now_in(&l.stages)).or(Some(st));
        }
    }
    None
}

/// A stage a run has reached, as the Map places it inside the post the run was for (map/deep.rs; I-1478, p715 level
/// 5: "the steps that handled it … each a node"). It carries its key among the stages (`stage-KEY`), so it opens that
/// stage's panel, plus its words, whom it went to, whether it is at work, the steps given for it (one per round, each
/// `RUN.sSTEP` on the wire), and its call when it has one (an inline step has none).
#[derive(Clone, Debug, PartialEq)]
pub struct Reached {
    pub key: String,
    pub words: String,
    pub who: Option<String>,
    pub now: bool,
    pub rounds: Vec<u32>,
    pub call: Option<String>,
}

/// Run V's stages reached so far, done or at work, in the order its stages are drawn, a par's lanes after the par.
/// A stage not reached yet is left out. Until a `match` is reached, which arm it takes is only a guess, so a stage
/// after it would change key, and a node the Map has placed must keep its place.
pub fn reached(v: &TicketView, labels: &Labels) -> Vec<Reached> {
    fn walk(sts: &[St], labels: &Labels, out: &mut Vec<Reached>) {
        for st in sts {
            let state = state_of(st.node.as_ref());
            if !matches!(state, State::Done | State::Now) {
                continue;
            }
            let n = st.node.as_ref();
            out.push(Reached {
                key: st.at.key.clone(),
                words: stage_words(st, labels),
                who: n.and_then(|n| n.who.clone()),
                now: state == State::Now,
                // the rounds given, as its panel's presses down to the wire count them (`wire_presses`)
                rounds: n.map(|n| n.rounds.iter().filter(|r| r.given.is_some() || r.answered.is_some()).map(|r| r.step).collect()).unwrap_or_default(),
                call: match st.s {
                    Prog::Step { call, inline: None, .. } => Some(call.clone()),
                    _ => None,
                },
            });
            for l in &st.lanes {
                walk(&l.stages, labels, out);
            }
        }
    }
    let Ok(kd) = program_kind(&v.program, &v.kind) else { return Vec::new() };
    let mut sts = Vec::new();
    collect(&kd.pattern, Some(&v.run), &At { key: v.id.clone(), env: form_env(v), ..At::default() }, &None, &mut sts);
    let mut out = Vec::new();
    walk(&sts, labels, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const KIND: &str = "kind change\n  form words: Text\n  result Text\n  pattern\n    w <- step role orchestrator assign(words);\n    match w {\n      Assigned(a) =>\n        d <- step a design(words);\n        return d\n      Ask(q) => return q\n    }\n";

    #[test]
    fn the_stages_follow_the_arm_with_the_work_before_it_is_reached() {
        let kd = parse_kind(KIND).unwrap();
        let mut sts = Vec::new();
        collect(&kd.pattern, None, &At { key: "C-1".into(), ..At::default() }, &None, &mut sts);
        let keys: Vec<&str> = sts.iter().map(|s| s.at.key.as_str()).collect();
        // the assign, then the design inside the Assigned arm; the Ask arm only returns
        assert_eq!(keys, ["C-1.0", "C-1.1.a0.0"]);
    }

    #[test]
    fn labels_are_the_first_line_of_the_docs() {
        let dir = std::env::temp_dir().join(format!("yaaihh-labels-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("calls")).unwrap();
        std::fs::create_dir_all(dir.join("kinds")).unwrap();
        std::fs::write(dir.join("calls/a.call"), "/// who writes the design\n/// and why\ncall assign(words: Text) -> Text\n  task assign\n  template\n    {{ words }}\n").unwrap();
        std::fs::write(dir.join("kinds/a.kind"), format!("/// the owner's words to built code\n{KIND}")).unwrap();
        let l = read_labels(&[dir.clone()]);
        assert_eq!(l.calls.get("assign").map(String::as_str), Some("who writes the design"));
        assert_eq!(l.kinds.get("change").map(String::as_str), Some("the owner's words to built code"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
