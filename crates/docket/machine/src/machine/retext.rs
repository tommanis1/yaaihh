//! Live edits (paper-research N-101 p424, on docket-core's p423): at a reload, an open ticket of an edited kind takes
//! the new text for what it has not run, by itself, as an editor carries the statement it stands at across an edit.
//!
//! A ticket stands at one node of its kind's pattern (the step it awaits, the `open` or `par` it waits on, or a hole),
//! inside the frames around it. That place is found in the old pattern (`locate`: the path whose frames are the
//! ticket's) and carried to the new one by matching the two trees (`carry`): the statements of each block are lined up
//! (an unchanged statement matches itself; a step a step of the same call and worker, though its arguments or prompt
//! changed; an `open` an `open` of the same kind; a hole a hole; a compound one the same kind of compound), and the new
//! frames are built along the matched path, each keeping what is history (its values, counters, the look running, the
//! layer's id) and taking the new subtree as what remains.
//!
//! What is in flight stays when the new text has its node: a step given stays given, with the prompt it was given, and
//! its answer goes to the new arms. When the node is gone, what is in flight is withdrawn, as Withdraw does, and the
//! ticket goes on from the first statement of the new text standing in its place: the one after the last statement
//! before it that the new text kept, or its block's first when none was. A ticket whose new remainder reads a name it never bound keeps its old text:
//! the one refusal. The `edited` line (`what: "reload"`) records the text it now runs and the calls it names; a
//! restart runs that line again on the ticket as the record left it, never today's files.

use super::*;
use crate::print;
use crate::view::{Reach, ReachCase, Reloaded, Withdrew};

/// A step down a pattern, from a node to one it holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Sel {
    /// A `Bind`'s first statement (its frame holds the rest).
    First,
    /// Past a `Bind`'s first statement, into the rest.
    Rest,
    /// The body of a `try`, `iterate`, `for`, `layer`, `with`, `let agent` or `during` (each with its frame).
    Body,
    /// A `match`'s arm.
    Arm(usize),
    /// A `try`'s handler, its frame gone (it caught).
    Handler(usize),
}

/// Two nodes the same text at the same line: a frame's subtree is a piece of the pattern it was made from.
fn same_text(a: &Prog, b: &Prog) -> bool {
    a.at() == b.at() && print::prog(a, 0) == print::prog(b, 0)
}

fn same_arms(a: &[syntax::Handler], b: &[syntax::Handler]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.exc == y.exc && x.x == y.x && same_text(&x.body, &y.body))
}

/// The path from ROOT to the node AT (its line and words, as the trace keys it) whose frames on the way are FRAMES
/// (outermost first): where a ticket with those frames stands. None: it does not stand in ROOT (it runs a program an
/// edit or a hole's answer gave).
pub(crate) fn locate(root: &Prog, frames: &[Frame], at: &(usize, String)) -> Option<Vec<Sel>> {
    let mut path = Vec::new();
    find(root, frames, at, &mut path).then_some(path)
}

fn find(p: &Prog, frames: &[Frame], at: &(usize, String), path: &mut Vec<Sel>) -> bool {
    if frames.is_empty() && !matches!(p, Prog::Bind { .. }) && p.at() == at.0 && print::node(p).1 == at.1 {
        return true;
    }
    let down = |sel: Sel, q: &Prog, frames: &[Frame], path: &mut Vec<Sel>| {
        path.push(sel);
        if find(q, frames, at, path) {
            return true;
        }
        path.pop();
        false
    };
    let first = frames.first();
    let inner = frames.get(1..).unwrap_or(&[]);
    match p {
        Prog::Bind { first: f, rest, .. } => {
            (matches!(first, Some(Frame::Bind { rest: r, .. }) if same_text(r, rest)) && down(Sel::First, f, inner, path)) || down(Sel::Rest, rest, frames, path)
        }
        Prog::Try { body, arms, .. } => {
            (matches!(first, Some(Frame::Try { arms: a, .. }) if same_arms(a, arms)) && down(Sel::Body, body, inner, path)) || arms.iter().enumerate().any(|(i, h)| down(Sel::Handler(i), &h.body, frames, path))
        }
        Prog::Match { arms, .. } => arms.iter().enumerate().any(|(i, (_, b))| down(Sel::Arm(i), b, frames, path)),
        Prog::Iterate { body, .. } => matches!(first, Some(Frame::Loop { body: b, .. }) if same_text(b, body)) && down(Sel::Body, body, inner, path),
        Prog::For { body, .. } => matches!(first, Some(Frame::Each { body: b, .. }) if same_text(b, body)) && down(Sel::Body, body, inner, path),
        Prog::Layer { body, .. } => matches!(first, Some(Frame::Layer { .. })) && down(Sel::Body, body, inner, path),
        Prog::With { body, .. } => matches!(first, Some(Frame::With { .. })) && down(Sel::Body, body, inner, path),
        Prog::LetAgent { body, .. } => matches!(first, Some(Frame::Scope { .. })) && down(Sel::Body, body, inner, path),
        Prog::During { body, whens, .. } => matches!(first, Some(Frame::During { whens: w, .. }) if w.iter().map(|w| (&w.ev, w.line)).eq(whens.iter().map(|w| (&w.ev, w.line)))) && down(Sel::Body, body, inner, path),
        _ => false,
    }
}

/// A statement of a block (a chain of `Bind`s): what it binds, the statement, the chain from it on, and the chain
/// after it (none for the last).
struct Stmt<'a> {
    x: Option<&'a Bound>,
    s: &'a Prog,
    from: &'a Prog,
    rest: Option<&'a Prog>,
}

fn stmts(p: &Prog) -> Vec<Stmt<'_>> {
    let mut out = Vec::new();
    let mut c = p;
    while let Prog::Bind { x, first, rest } = c {
        out.push(Stmt { x: Some(x), s: first, from: c, rest: Some(rest) });
        c = rest;
    }
    out.push(Stmt { x: None, s: c, from: c, rest: None });
    out
}

/// Whether new node N stands where old node O stood: the same statement, or one a ticket standing in O goes on in.
fn same(o: &Prog, n: &Prog) -> bool {
    match (o, n) {
        (Prog::Bind { first: a, .. }, Prog::Bind { first: b, .. }) => same(a, b),
        // a step written inline has no call of its own: the same words, or the same answers
        (Prog::Step { w: a, call: c, hold: h, inline: i, .. }, Prog::Step { w: b, call: d, hold: g, inline: j, .. }) => {
            c == d && h == g && print::worker(a) == print::worker(b) && match (i, j) {
                (Some((x, s)), Some((y, t))) => x == y || s == t,
                (None, None) => true,
                _ => false,
            }
        }
        (Prog::Open { kind: a, .. }, Prog::Open { kind: b, .. }) => a == b,
        (Prog::Decide { name: a, .. }, Prog::Decide { name: b, .. }) => a == b,
        (Prog::Hole { .. }, Prog::Hole { .. }) | (Prog::Try { .. }, Prog::Try { .. }) | (Prog::Layer { .. }, Prog::Layer { .. }) => true,
        (Prog::Match { e: a, .. }, Prog::Match { e: b, .. }) => print::expr(a) == print::expr(b),
        (Prog::Iterate { x: a, .. }, Prog::Iterate { x: b, .. }) | (Prog::For { x: a, .. }, Prog::For { x: b, .. }) | (Prog::With { f: a, .. }, Prog::With { f: b, .. }) => a == b,
        (Prog::LetAgent { x: a, from: f, .. }, Prog::LetAgent { x: b, from: g, .. }) => a == b && f.is_some() == g.is_some(),
        (Prog::During { whens: a, .. }, Prog::During { whens: b, .. }) => a.iter().map(|w| &w.ev).eq(b.iter().map(|w| &w.ev)),
        (Prog::Par { branches: a, each: e, .. }, Prog::Par { branches: b, each: f, .. }) | (Prog::Any { branches: a, each: e, .. }, Prog::Any { branches: b, each: f, .. }) => a.len() == b.len() && e.is_some() == f.is_some(),
        _ => std::mem::discriminant(o) == std::mem::discriminant(n) && print::prog(o, 0) == print::prog(n, 0),
    }
}

/// How well new statement N stands where old O stood: 0 when not `same`; else 1, one more when it binds the same
/// names, and one more when it is the same text. The best match wins over one by position alone: an inline step put
/// before others of the same answers does not take their places (docket-edge's try, paper-research N-101 p605).
fn score(o: &Stmt, n: &Stmt) -> u32 {
    if !same(o.s, n.s) {
        return 0;
    }
    let names = |s: &Stmt| s.x.map(bound_names);
    1 + u32::from(names(o) == names(n)) + u32::from(print::prog(o.s, 0) == print::prog(n.s, 0))
}

/// For each statement of OLD, the one of NEW it lines up with (the pairs in order whose `score`s add up most).
fn align(old: &[Stmt], new: &[Stmt]) -> Vec<Option<usize>> {
    let (a, b) = (old.len(), new.len());
    let w: Vec<Vec<u32>> = old.iter().map(|o| new.iter().map(|n| score(o, n)).collect()).collect();
    let mut l = vec![vec![0u32; b + 1]; a + 1];
    for i in (0..a).rev() {
        for j in (0..b).rev() {
            let take = if w[i][j] > 0 { w[i][j] + l[i + 1][j + 1] } else { 0 };
            l[i][j] = take.max(l[i + 1][j]).max(l[i][j + 1]);
        }
    }
    let mut out = vec![None; a];
    let (mut i, mut j) = (0, 0);
    while i < a && j < b {
        if w[i][j] > 0 && l[i][j] == w[i][j] + l[i + 1][j + 1] {
            out[i] = Some(j);
            i += 1;
            j += 1;
        } else if l[i + 1][j] >= l[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// Every node of NEW that stands where a node of OLD stood (statements lined up block by block, into the compound
/// ones that match): what the picture of a ticket that took NEW keeps of what it had done.
pub(crate) fn pairs<'a>(o: &'a Prog, n: &'a Prog, out: &mut Vec<(&'a Prog, &'a Prog)>) {
    let os = stmts(o);
    let ns = stmts(n);
    for (i, j) in align(&os, &ns).into_iter().enumerate() {
        if let Some(j) = j {
            pair(os[i].s, ns[j].s, out);
        }
    }
}

fn pair<'a>(o: &'a Prog, n: &'a Prog, out: &mut Vec<(&'a Prog, &'a Prog)>) {
    if !matches!(o, Prog::Bind { .. }) {
        out.push((o, n));
    }
    match (o, n) {
        (Prog::Bind { .. }, Prog::Bind { .. }) => pairs(o, n, out),
        (Prog::Try { body: a, arms: x, .. }, Prog::Try { body: b, arms: y, .. }) => {
            pairs(a, b, out);
            for h in x {
                if let Some(g) = y.iter().find(|g| g.exc == h.exc) {
                    pairs(&h.body, &g.body, out);
                }
            }
        }
        (Prog::Match { arms: x, .. }, Prog::Match { arms: y, .. }) => {
            for (p, a) in x {
                if let Some((_, b)) = y.iter().find(|(q, _)| print::pat(q) == print::pat(p)) {
                    pairs(a, b, out);
                }
            }
        }
        (Prog::Iterate { body: a, .. }, Prog::Iterate { body: b, .. })
        | (Prog::For { body: a, .. }, Prog::For { body: b, .. })
        | (Prog::Layer { body: a, .. }, Prog::Layer { body: b, .. })
        | (Prog::With { body: a, .. }, Prog::With { body: b, .. })
        | (Prog::LetAgent { body: a, .. }, Prog::LetAgent { body: b, .. })
        | (Prog::During { body: a, .. }, Prog::During { body: b, .. }) => pairs(a, b, out),
        (Prog::Par { branches: x, .. }, Prog::Par { branches: y, .. }) | (Prog::Any { branches: x, .. }, Prog::Any { branches: y, .. }) => {
            for (a, b) in x.iter().zip(y) {
                pairs(a, b, out);
            }
        }
        _ => {}
    }
}

/// Where the ticket stands once carried.
#[derive(Debug)]
pub(crate) enum Landing {
    /// What is in flight stays: the node it stood at, and the one of the new text that stands there.
    Kept { old: Prog, new: Prog },
    /// Its node is gone: what is in flight is withdrawn, and the ticket goes on with this, in this.
    Gone { run: Prog, env: Env },
}

#[derive(Debug)]
pub(crate) struct Carried {
    /// The new frames, outermost first.
    pub k: Vec<Frame>,
    /// The old frames that have no place in the new text (their agents end, their looks too, a `with` reverts).
    pub dropped: Vec<Frame>,
    pub landing: Landing,
}

/// The ticket's place carried from OLD to NEW. PATH is where it stands in OLD (`locate`), FRAMES its frames, ENV what
/// is bound at the node it stands at (the step's, the hole's; none for a ticket waiting on one below). Err: why it
/// cannot be carried (it keeps its old text).
pub(crate) fn carry(old: &Prog, new: &Prog, path: &[Sel], frames: &[Frame], env: &Env) -> Result<Carried, String> {
    let mut w = Walk { old_k: frames, fi: 0, out: Vec::new(), dropped: Vec::new(), env };
    let landing = w.block(old, new, path).map_err(|g| match g {
        Gone::Shape => String::from("the block it stands in was rewritten into another shape"),
        Gone::No(e) => e,
    })?;
    Ok(Carried { k: w.out, dropped: w.dropped, landing })
}

enum Gone {
    /// The statement it stands in is there, but not the arm (or shape) of it it stands in: gone at that statement.
    Shape,
    No(String),
}

struct Walk<'a> {
    old_k: &'a [Frame],
    fi: usize,
    out: Vec<Frame>,
    dropped: Vec<Frame>,
    env: &'a Env,
}

impl Walk<'_> {
    /// What was bound where the old frames from FI on start: the first of them that holds it, else the node's.
    fn env_from(&self, fi: usize) -> Env {
        self.old_k[fi.min(self.old_k.len())..]
            .iter()
            .find_map(|f| match f {
                Frame::Bind { env, .. } | Frame::Loop { env, .. } | Frame::Each { env, .. } | Frame::Try { env, .. } | Frame::During { env, .. } => Some(env.clone()),
                _ => None,
            })
            .unwrap_or_else(|| self.env.clone())
    }

    fn take(&mut self) -> Frame {
        let f = self.old_k[self.fi].clone();
        self.fi += 1;
        f
    }

    fn block(&mut self, o: &Prog, n: &Prog, path: &[Sel]) -> Result<Landing, Gone> {
        let os = stmts(o);
        let ns = stmts(n);
        let i = path.iter().take_while(|s| **s == Sel::Rest).count();
        let (framed, inner) = match path.get(i) {
            Some(Sel::First) => (true, &path[i + 1..]),
            _ => (false, &path[i..]),
        };
        let fi = self.fi;
        let here = if framed {
            match self.take() {
                Frame::Bind { env, .. } => Some(env),
                f => return Err(Gone::No(format!("its frames do not follow its program (a {f:?} where a statement's rest was)"))),
            }
        } else {
            None
        };
        let img = align(&os, &ns);
        let mark = self.out.len();
        if let Some(j) = img[i] {
            if let (Some(rest), Some(x)) = (ns[j].rest, ns[j].x) {
                let env = here.clone().unwrap_or_else(|| self.env_from(self.fi));
                self.out.push(Frame::Bind { x: x.clone(), rest: rest.clone(), env });
            }
            match self.descend(os[i].s, ns[j].s, inner) {
                Err(Gone::Shape) => self.out.truncate(mark),
                r => return r,
            }
        }
        // gone: what it stood in goes, with the frames inside it; it goes on from the first statement of the new text
        // standing in its place: the one after the last statement before it that the new text kept, or the block's
        // first when none was (docket-core's review, N-101 p545)
        let env = here.unwrap_or_else(|| self.env_from(fi + usize::from(framed)));
        let start = fi + usize::from(framed);
        self.dropped.extend(self.old_k[start.min(self.old_k.len())..].iter().cloned());
        self.fi = self.old_k.len();
        let next = img[..i].iter().flatten().max().map_or(0, |j| j + 1);
        match Some(next).filter(|c| *c < ns.len()) {
            Some(c) => Ok(Landing::Gone { run: ns[c].from.clone(), env }),
            None => Err(Gone::No(format!("the statement it stands in (line {}) is gone, and nothing of its block after it is kept", os[i].s.at()))),
        }
    }

    fn descend(&mut self, o: &Prog, n: &Prog, path: &[Sel]) -> Result<Landing, Gone> {
        let Some(sel) = path.first() else {
            return Ok(Landing::Kept { old: o.clone(), new: n.clone() });
        };
        let rest = &path[1..];
        match (o, n, sel) {
            (Prog::Bind { .. }, Prog::Bind { .. }, _) => self.block(o, n, path),
            (Prog::Try { body: ob, .. }, Prog::Try { body: nb, arms, .. }, Sel::Body) => {
                let Frame::Try { env, .. } = self.take() else { return Err(Gone::Shape) };
                self.out.push(Frame::Try { arms: arms.clone(), env });
                self.block(ob, nb, rest)
            }
            (Prog::Try { arms: oa, .. }, Prog::Try { arms: na, .. }, Sel::Handler(i)) => {
                let k = na.iter().position(|h| h.exc == oa[*i].exc).ok_or(Gone::Shape)?;
                self.block(&oa[*i].body, &na[k].body, rest)
            }
            (Prog::Match { arms: oa, guards: og, .. }, Prog::Match { arms: na, guards: ng, .. }, Sel::Arm(i)) => {
                let guard = |g: &[Option<Expr>], i: usize| g.get(i).and_then(Option::as_ref).map(print::expr);
                let k = na.iter().enumerate().position(|(k, (p, _))| print::pat(p) == print::pat(&oa[*i].0) && guard(ng, k) == guard(og, *i)).ok_or(Gone::Shape)?;
                self.block(&oa[*i].1, &na[k].1, rest)
            }
            (Prog::Iterate { n: on, body: ob, .. }, Prog::Iterate { x, n: nn, body: nb, .. }, Sel::Body) => {
                let Frame::Loop { left, measure, env, .. } = self.take() else { return Err(Gone::Shape) };
                // the rounds left as the new `up to` counts them, those run kept
                let left = match (*on, *nn) {
                    (_, syntax::UNBOUNDED) => syntax::UNBOUNDED,
                    (syntax::UNBOUNDED, m) => m.saturating_sub(1),
                    (a, m) => (left + m).saturating_sub(a),
                };
                self.out.push(Frame::Loop { x: x.clone(), left, measure, body: (**nb).clone(), env });
                self.block(ob, nb, rest)
            }
            (Prog::For { body: ob, .. }, Prog::For { x, body: nb, .. }, Sel::Body) => {
                let Frame::Each { rest: more, done, env, .. } = self.take() else { return Err(Gone::Shape) };
                self.out.push(Frame::Each { x: x.clone(), rest: more, done, body: (**nb).clone(), env });
                self.block(ob, nb, rest)
            }
            (Prog::Layer { body: ob, .. }, Prog::Layer { body: nb, .. }, Sel::Body) | (Prog::With { body: ob, .. }, Prog::With { body: nb, .. }, Sel::Body) | (Prog::LetAgent { body: ob, .. }, Prog::LetAgent { body: nb, .. }, Sel::Body) => {
                let f = self.take();
                self.out.push(f);
                self.block(ob, nb, rest)
            }
            (Prog::During { body: ob, .. }, Prog::During { body: nb, whens, scope, .. }, Sel::Body) => {
                let Frame::During { env, busy, missed, asks, .. } = self.take() else { return Err(Gone::Shape) };
                self.out.push(Frame::During { whens: whens.clone(), scope: scope.clone(), env, busy, missed, asks });
                self.block(ob, nb, rest)
            }
            _ => Err(Gone::Shape),
        }
    }
}

/// A name the carried ticket's remainder reads that nothing it ran bound, with the line: why it keeps its old text.
/// AGENTS: the names bound anywhere in the new pattern (a worker or a poster named by one of them is read from what
/// is bound; any other is an agent's name).
/// ENV: what is bound at the node it stands at; a step kept in flight whose new words read a place it lacks is given
/// again with those words when it is asked again, so it counts too.
pub(crate) fn unbound(c: &Carried, env: &Env, agents: &BTreeSet<String>) -> Option<(String, usize)> {
    let mut out = Vec::new();
    let keys = |env: &Env| env.keys().cloned().collect::<BTreeSet<String>>();
    for f in &c.k {
        match f {
            Frame::Bind { x, rest, env } => {
                let mut b = keys(env);
                b.extend(bound_names(x));
                free(rest, &b, agents, &mut out);
            }
            Frame::Loop { x, body, env, .. } | Frame::Each { x, body, env, .. } => {
                let mut b = keys(env);
                b.insert(x.clone());
                free(body, &b, agents, &mut out);
            }
            Frame::Try { arms, env } => {
                for h in arms {
                    let mut b = keys(env);
                    b.extend(h.x.clone());
                    free(&h.body, &b, agents, &mut out);
                }
            }
            Frame::During { whens, env, .. } => {
                for when in whens {
                    let mut b = keys(env);
                    b.insert(when.x.clone());
                    b.insert("this".into());
                    if let Some(e) = &when.cond {
                        free_expr(e, &b, when.line, &mut out);
                    }
                    free(&when.body, &b, agents, &mut out);
                }
            }
            _ => {}
        }
    }
    match &c.landing {
        Landing::Gone { run, env } => free(run, &keys(env), agents, &mut out),
        Landing::Kept { new: n @ Prog::Step { inline: Some((text, _)), .. }, .. } => text_places(text, &keys(env), n.at(), &mut out),
        Landing::Kept { .. } => {}
    }
    out.into_iter().find(|(n, _)| n != "this" && n != crate::types::AGENTS)
}

fn bound_names(b: &Bound) -> Vec<String> {
    match b {
        Bound::Var(v) => vec![v.clone()],
        Bound::Tuple(vs) => vs.clone(),
    }
}

fn pat_names(p: &Pat, out: &mut BTreeSet<String>) {
    match p {
        Pat::Var(v) => {
            out.insert(v.clone());
        }
        Pat::Con { args, .. } | Pat::Tuple(args) => args.iter().for_each(|a| pat_names(a, out)),
        Pat::One(a) => pat_names(a, out),
        Pat::Wild | Pat::Nil | Pat::None => {}
    }
}

/// Every name P binds anywhere (its statements, patterns, loops, agents).
pub(crate) fn binders(p: &Prog, out: &mut BTreeSet<String>) {
    match p {
        Prog::Bind { x, first, rest } => {
            out.extend(bound_names(x));
            binders(first, out);
            binders(rest, out);
        }
        Prog::Match { arms, .. } => {
            for (pat, b) in arms {
                pat_names(pat, out);
                binders(b, out);
            }
        }
        Prog::Par { branches, each, .. } | Prog::Any { branches, each, .. } => {
            out.extend(each.iter().map(|e| e.x.clone()));
            branches.iter().for_each(|b| binders(b, out));
        }
        Prog::Iterate { x, body, .. } | Prog::For { x, body, .. } | Prog::LetAgent { x, body, .. } => {
            out.insert(x.clone());
            binders(body, out);
        }
        Prog::Layer { body, .. } | Prog::With { body, .. } => binders(body, out),
        Prog::Try { body, arms, .. } => {
            binders(body, out);
            for h in arms {
                out.extend(h.x.clone());
                binders(&h.body, out);
            }
        }
        Prog::During { body, whens, .. } => {
            binders(body, out);
            for when in whens {
                out.insert(when.x.clone());
                binders(&when.body, out);
            }
        }
        _ => {}
    }
}

/// The `{{ name }}` places of TEXT whose names B does not have.
fn text_places(text: &str, b: &BTreeSet<String>, line: usize, out: &mut Vec<(String, usize)>) {
    for (name, _, _) in syntax::placeholders(text) {
        if !b.contains(&name) {
            out.push((name, line));
        }
    }
}

fn free_expr(e: &Expr, b: &BTreeSet<String>, line: usize, out: &mut Vec<(String, usize)>) {
    match e {
        Expr::Var(v) if !b.contains(v) => out.push((v.clone(), line)),
        Expr::Str(s) => text_places(s, b, line, out),
        Expr::Field(x, _) | Expr::ResultOf { e: x, .. } | Expr::TextOf { e: x, .. } | Expr::ByOf(x) | Expr::Not(x) => free_expr(x, b, line, out),
        Expr::Con { args: xs, .. } | Expr::List(xs) | Expr::Tuple(xs) | Expr::Fact { args: xs, .. } => xs.iter().for_each(|x| free_expr(x, b, line, out)),
        Expr::Record(fs) => fs.iter().for_each(|(_, x)| free_expr(x, b, line, out)),
        Expr::Cmp { l, r, .. } | Expr::Rel { l, r, .. } | Expr::And(l, r) | Expr::Or(l, r) | Expr::Append(l, r) => {
            free_expr(l, b, line, out);
            free_expr(r, b, line, out);
        }
        Expr::Find { x, e, cond } | Expr::Quant { x, e, cond, .. } => {
            free_expr(e, b, line, out);
            let mut b2 = b.clone();
            b2.insert(x.clone());
            free_expr(cond, &b2, line, out);
        }
        _ => {}
    }
}

/// The names P reads that B (bound around it) does not have, with their lines.
fn free(p: &Prog, b: &BTreeSet<String>, agents: &BTreeSet<String>, out: &mut Vec<(String, usize)>) {
    let at = p.at();
    let ex = |e: &Expr, out: &mut Vec<(String, usize)>| free_expr(e, b, at, out);
    let named = |n: &str, out: &mut Vec<(String, usize)>| {
        if agents.contains(n) && !b.contains(n) {
            out.push((n.to_string(), at));
        }
    };
    let with = |names: &[String]| {
        let mut b2 = b.clone();
        b2.extend(names.iter().cloned());
        b2
    };
    match p {
        Prog::Return { e, .. } | Prog::Continue { e, .. } | Prog::Stop { e, .. } => ex(e, out),
        Prog::Give { w, e, .. } => {
            named(w, out);
            ex(e, out);
        }
        Prog::Raise { e, .. } => e.iter().for_each(|e| ex(e, out)),
        Prog::Step { w, args, inline, on, .. } => {
            args.iter().for_each(|e| ex(e, out));
            // its `on`s and `until`s (syntax::On), each with its event's value bound, and an `until` the answer's
            // fields it reads
            for o in on {
                let mut b2 = b.clone();
                b2.insert(o.x.clone());
                b2.extend(o.fields.iter().cloned());
                o.cond.iter().chain(&o.answer).for_each(|e| free_expr(e, &b2, at, out));
            }
            // an inline step's words read their `{{ name }}` places (paper-research N-101 p605)
            if let Some((text, _)) = inline {
                text_places(text, b, at, out);
            }
            match w {
                Worker::Named(n) => named(n, out),
                Worker::Expr(e) | Worker::Fork(e) => ex(e, out),
                _ => {}
            }
        }
        Prog::Open { args, .. } | Prog::Decide { args, .. } => args.iter().for_each(|e| ex(e, out)),
        Prog::Post { e, on, by, naming, informing, .. } => {
            ex(e, out);
            on.iter().chain(naming).chain(informing).for_each(|e| ex(e, out));
            if let Some(w) = by.as_deref().filter(|w| *w != "owner") {
                named(w, out);
            }
        }
        Prog::Move { n, to, .. } => {
            ex(n, out);
            ex(to, out);
        }
        Prog::Reply { q: x, a: y, .. } | Prog::Tell { agents: x, p: y, joins: None, .. } | Prog::Mark { p: x, marks: y, .. } => {
            ex(x, out);
            ex(y, out);
        }
        Prog::Tell { agents, p, joins: Some(j), .. } => {
            ex(agents, out);
            ex(p, out);
            ex(j, out);
        }
        Prog::Drop { p, .. } | Prog::Gather { e: p, .. } => ex(p, out),
        Prog::Deliver { a, items, rest, digest, from, .. } => {
            ex(a, out);
            ex(items, out);
            for r in rest.iter().chain(digest).chain(from) {
                ex(r, out);
            }
        }
        Prog::Keep { items, why, .. } | Prog::Report { items, why, .. } | Prog::Fork { a: items, place: why, .. } => {
            ex(items, out);
            ex(why, out);
        }
        Prog::Hole { .. } => {}
        Prog::Bind { x, first, rest } => {
            free(first, b, agents, out);
            free(rest, &with(&bound_names(x)), agents, out);
        }
        Prog::Match { e, arms, guards, .. } => {
            ex(e, out);
            for (i, (pat, body)) in arms.iter().enumerate() {
                let mut names = BTreeSet::new();
                pat_names(pat, &mut names);
                let b2 = with(&names.into_iter().collect::<Vec<_>>());
                if let Some(g) = guards.get(i).and_then(Option::as_ref) {
                    free_expr(g, &b2, at, out);
                }
                free(body, &b2, agents, out);
            }
        }
        Prog::Par { branches, each, .. } | Prog::Any { branches, each, .. } => {
            let b2 = match each {
                Some(x) => {
                    ex(&x.e, out);
                    let b2 = with(std::slice::from_ref(&x.x));
                    if let Some(o) = &x.order {
                        free_expr(&o.key, &b2, at, out);
                        free_expr(&o.after, &b2, at, out);
                    }
                    b2
                }
                None => b.clone(),
            };
            branches.iter().for_each(|br| free(br, &b2, agents, out));
        }
        Prog::Iterate { x, init, body, .. } => {
            ex(init, out);
            free(body, &with(std::slice::from_ref(x)), agents, out);
        }
        Prog::For { x, e, body, .. } => {
            ex(e, out);
            free(body, &with(std::slice::from_ref(x)), agents, out);
        }
        Prog::Layer { on, body, .. } => {
            on.iter().for_each(|e| ex(e, out));
            free(body, b, agents, out);
        }
        Prog::With { e, body, .. } => {
            ex(e, out);
            free(body, b, agents, out);
        }
        Prog::LetAgent { x, from, body, .. } => {
            from.iter().for_each(|e| ex(e, out));
            free(body, &with(std::slice::from_ref(x)), agents, out);
        }
        Prog::Try { body, arms, .. } => {
            free(body, b, agents, out);
            for h in arms {
                free(&h.body, &with(&h.x.iter().cloned().collect::<Vec<_>>()), agents, out);
            }
        }
        Prog::During { body, whens, .. } => {
            free(body, b, agents, out);
            for when in whens {
                let b2 = with(&[when.x.clone(), "this".into()]);
                if let Some(c) = &when.cond {
                    free_expr(c, &b2, when.line, out);
                }
                free(&when.body, &b2, agents, out);
            }
        }
    }
}

/// What ticket T does taking a new text (`Machine::retext_plan`): its text read OLD and NEW (BAD, why NEW no longer
/// checks on replay; LOOSE, the kinds it checked against as they were), its picture's part BEFORE, where it lands,
/// and its branches.
struct Retexted {
    old: Kind,
    new: Kind,
    bad: Option<String>,
    loose: BTreeSet<String>,
    before: usize,
    c: Carried,
    branches: Vec<(u64, usize, Prog, Prog, Result<Plan, String>)>,
}

/// Each node of what frames K hold to run, given to READ in order (`check::each`): what a ticket with those frames has
/// not run of its program. With each, the most times that part runs from here (`syntax::UNBOUNDED`, no end):
/// - once, for the rest of what is running and for a `try`'s arms;
/// - a loop's body: the rounds left after the one running (`Frame::Loop`'s `left`). With none left it is still read,
///   at 0: a stop's Again on Exhausted runs one round more (stop.rs), which `opens_ahead` counts;
/// - a `for`'s body: the elements after the one running, 0 when there are none;
/// - a `during`'s whens: no end.
fn frames_each<'a>(k: &'a [Frame], read: &mut impl FnMut(&'a Prog, u32)) {
    for f in k {
        match f {
            Frame::Bind { rest, .. } => check::each(rest, &mut |q| read(q, 1)),
            Frame::Loop { body, left, .. } => check::each(body, &mut |q| read(q, *left)),
            Frame::Each { body, rest, .. } => check::each(body, &mut |q| read(q, u32::try_from(rest.len()).unwrap_or(syntax::UNBOUNDED))),
            Frame::Try { arms, .. } => arms.iter().for_each(|h| check::each(&h.body, &mut |q| read(q, 1))),
            Frame::During { whens, .. } => whens.iter().for_each(|w| check::each(&w.body, &mut |q| read(q, syntax::UNBOUNDED))),
            _ => {}
        }
    }
}

/// Each node of what open ticket X has not run, given to READ in order, with the most times it runs from here as in
/// `frames_each`: what its frames hold to run, its kind's handlers (no end) and a branch it holds back (once). The one
/// walk `opens_ahead` and the reach (`Machine::reach`) read. How often a node runs within a node ahead (an `iterate`'s
/// body) is `repeats`'.
fn ahead_each<'a>(x: &'a Ticket, read: &mut impl FnMut(&'a Prog, u32)) {
    frames_each(&x.k, read);
    x.handlers.iter().for_each(|h| check::each(&h.body, &mut |q| read(q, syntax::UNBOUNDED)));
    if let Some(h) = &x.held {
        check::each(&h.prog, &mut |q| read(q, 1));
    }
}

/// Each node within Q that runs more than once each time Q runs, to READ: the body of an `iterate` or a `for` of more
/// than one round, the branch of a `par` or an `any` over each element, a `during`'s whens. With `ahead_each`'s own
/// count, what makes a step ahead one that runs again (consult-3 N-1396 p317, 4).
fn repeats<'a>(q: &'a Prog, read: &mut impl FnMut(&'a Prog)) {
    match q {
        Prog::Iterate { body, n, .. } | Prog::For { body, n, .. } if *n > 1 => check::each(body, read),
        Prog::Par { branches, each: Some(_), .. } | Prog::Any { branches, each: Some(_), .. } => branches.iter().for_each(|b| check::each(b, read)),
        Prog::During { whens, .. } => whens.iter().for_each(|w| check::each(&w.body, read)),
        _ => {}
    }
}

/// A node as the trace keys it: its line and words.
fn key(p: &Prog) -> (usize, String) {
    (p.at(), print::node(p).1)
}

/// What an edit from OLD to NEW changes, its nodes keyed as the trace keys them: into ED each node of NEW that `pairs`
/// lines up with one of OLD whose text differs (its line aside, so a line put in above it changes nothing), and each
/// it lines up with none (added); into GONE each node of OLD it lines up with none (taken out). The reach looks for ED
/// in what a ticket has not run of the new text and for GONE in what it had not run of the old (consult-3 p308, 2).
fn edited(old: &Prog, new: &Prog, ed: &mut BTreeSet<(usize, String)>, gone: &mut BTreeSet<(usize, String)>) {
    let mut ps = Vec::new();
    pairs(old, new, &mut ps);
    ed.extend(ps.iter().filter(|(o, n)| print::prog(o, 0) != print::prog(n, 0)).map(|(_, n)| key(n)));
    unpaired(new, &ps.iter().map(|(_, n)| key(n)).collect(), ed);
    unpaired(old, &ps.iter().map(|(o, _)| key(o)).collect(), gone);
}

/// Each node of P but a `Bind` whose key is not in PAIRED, into OUT.
fn unpaired(p: &Prog, paired: &BTreeSet<(usize, String)>, out: &mut BTreeSet<(usize, String)>) {
    check::each(p, &mut |q| {
        if !matches!(q, Prog::Bind { .. }) && !paired.contains(&key(q)) {
            out.insert(key(q));
        }
    });
}

/// `edited` for a kind's handlers: each of NEW against the one of OLD on the same event in the same place among those,
/// with the same binder and condition; one of NEW with none is added whole, one of OLD with none taken out whole.
fn edited_handlers(old: &[syntax::When], new: &[syntax::When], ed: &mut BTreeSet<(usize, String)>, gone: &mut BTreeSet<(usize, String)>) {
    let none = BTreeSet::new();
    let mut matched = BTreeSet::new();
    for (j, h) in new.iter().enumerate() {
        let nth = new[..j].iter().filter(|g| g.ev == h.ev).count();
        match old.iter().enumerate().filter(|(_, g)| g.ev == h.ev).nth(nth) {
            Some((i, g)) if g.x == h.x && g.cond.as_ref().map(print::expr) == h.cond.as_ref().map(print::expr) => {
                matched.insert(i);
                edited(&g.body, &h.body, ed, gone);
            }
            _ => unpaired(&h.body, &none, ed),
        }
    }
    for (_, g) in old.iter().enumerate().filter(|(i, _)| !matched.contains(i)) {
        unpaired(&g.body, &none, gone);
    }
}

/// What ticket X says taking a new text that lands it at LANDING (`apply_carry`'s reply), read before anything
/// changes: so the reach says what `retext` will.
fn carry_said(x: &Ticket, landing: &Landing) -> String {
    match landing {
        Landing::Kept { .. } => String::from("took the new text"),
        Landing::Gone { .. } => match &x.status {
            Status::Awaits { agent, owed, .. } if x.shaping.is_some() => format!("took the new text; its hole is gone, and the minder's step {} withdrawn from {agent}", owed.step),
            Status::Awaits { agent, owed, .. } => format!("took the new text; step {} ({}) of {agent} withdrawn: its statement is gone", owed.step, owed.call),
            Status::Asking { owed, .. } => format!("took the new text; step {} ({}) withdrawn before anyone was given it: its statement is gone", owed.step, owed.call),
            _ => String::from("took the new text; its hole is gone"),
        },
    }
}

/// Whether TEXT names call NAME (a word of it, or a `decide` of it): what `runs_today` reads a ticket's text by. Each
/// word is compared in place (no `decide` text made for it): the reach reads every open ticket's text by this.
fn names_call(text: &str, name: &str) -> bool {
    let decided = name.strip_prefix(syntax::DECIDE);
    text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-')).any(|w| w == name || decided == Some(w))
}

/// The lines LS, as the reach names them: "line 4", "lines 4 and 9", "lines 4, 9 and 12".
fn lines_of(mut ls: Vec<usize>) -> String {
    ls.sort_unstable();
    ls.dedup();
    let n: Vec<String> = ls.iter().map(usize::to_string).collect();
    match n.as_slice() {
        [one] => format!("line {one}"),
        [init @ .., last] => format!("lines {} and {last}", init.join(", ")),
        [] => String::new(),
    }
}

/// What a branch of the `par` a ticket waits on does when the ticket takes the new text.
enum Plan {
    Closed,
    /// Not started (a `par` in order): it starts with its new branch.
    Held,
    Carry(Carried),
}

impl Machine {
    /// The tickets in the log that run call NAME as today's files have it, not as their record does: their text (or
    /// that of a ticket above them) names it, and no line of theirs or above them recorded it (a ticket from before
    /// calls on the record). A reload may change NAME's parameters or answers only when there are none: a ticket that
    /// recorded it runs it as recorded (and one that takes a reload's new text takes its calls with it).
    pub fn runs_today(&self, name: &str) -> Vec<u64> {
        let names = |text: &str| names_call(text, name);
        self.tickets
            .values()
            .filter(|x| {
                let mut u = Some(x.t);
                let (mut named, mut recorded) = (false, false);
                while let Some(y) = u.and_then(|v| self.tickets.get(&v)) {
                    named |= names(&y.program);
                    recorded |= y.calls.contains_key(name);
                    u = y.parent.or(y.under);
                }
                named && !recorded
            })
            .map(|x| x.t)
            .collect()
    }

    /// What ticket T would do taking TEXT (`retext`), nothing changed: Err says why it keeps its old text, or why the
    /// text is refused.
    fn retext_plan(&self, cx: &Cx, t: u64, text: &str, calls: &BTreeMap<String, CallSig>) -> Result<Retexted, String> {
        let keeps = |why: String| Err(format!("kept its old text: {why}"));
        let Some(x) = self.tickets.get(&t) else { return Err(format!("There is no ticket {t}.")) };
        let id = self.id(t);
        if !x.is_open() {
            return Err(format!("{id} is closed."));
        }
        if x.kind == "part" || x.handling.is_some() || x.ask.is_some() {
            return keeps(String::from("it is a piece of another ticket's program, which carries it"));
        }
        // read, not checked: the calls the new text names are already on it (`exec`), and a call whose answers
        // changed would fail the old text's check; its nodes print as the frames made from it do
        let old = match syntax::parse_kind(&x.kind, &x.program, 1, &self.defs.named) {
            Ok(k) => k,
            Err(e) => return keeps(format!("its own text no longer reads: {e}")),
        };
        let (new, bad, loose) = self.recorded_program_as(&x.kind, text, calls, cx).map_err(|e| format!("The new text of {} does not read: {e}", x.kind))?;
        if (&new.form, &new.result, &new.raises) != (&old.form, &old.result, &old.raises) {
            return keeps(String::from("the new text changes the kind's form, result or what it raises"));
        }
        if x.holding.is_some() {
            return keeps(String::from("its program has returned; the policy is asked whether its goals hold"));
        }
        let before = x.trace.kind_part();
        let mut agents = BTreeSet::new();
        binders(&new.pattern, &mut agents);
        agents.extend(new.form.iter().map(|(n, _)| n.clone()));
        let c = match self.plan_carry(t, &old.pattern, &new.pattern, before, &agents) {
            Ok(c) => c,
            Err(e) => return keeps(e),
        };
        // the branches it waits on, each carried from its branch of the old `par` to the new one's
        let mut branches = Vec::new();
        if let Landing::Kept { old: Prog::Par { branches: ob, each: oe, .. } | Prog::Any { branches: ob, each: oe, .. }, new: Prog::Par { branches: nb, .. } | Prog::Any { branches: nb, .. } } = &c.landing {
            let tickets = x.trace.now.as_ref().and_then(|k| x.trace.visits.get(k)).map(|v| v.tickets.clone()).unwrap_or_default();
            for (i, b) in tickets.into_iter().enumerate() {
                let Some(y) = self.tickets.get(&b).filter(|y| y.kind == "part") else { continue };
                let j = if oe.is_some() { 0 } else { i };
                let (o, n) = (ob[j].clone(), nb[j].clone());
                // a branch closed already: only its picture follows the new text
                let plan = if !y.is_open() {
                    Ok(Plan::Closed)
                } else if y.held.is_some() {
                    Ok(Plan::Held)
                } else {
                    self.plan_carry(b, &o, &n, before, &agents).map(Plan::Carry)
                };
                branches.push((b, i + 1, o, n, plan));
            }
        }
        Ok(Retexted { old, new, bad, loose, before, c, branches })
    }

    /// The kinds ticket T would still open by its old text if it were to take TEXT, its kind's new text read against
    /// today's definitions with CALLS (`retext`, nothing changed): none where it takes it (the new text is checked
    /// against the kinds as they are now), but those of a branch that keeps its old one; all its old text would
    /// (`opens_ahead`, its branches' too) where it keeps its own.
    pub fn opens_as_it_was(&self, t: u64, text: &str, calls: &BTreeMap<String, CallSig>) -> BTreeSet<String> {
        let cx = Cx { now: String::new(), lines: Vec::new(), effects: Vec::new(), recorded: None };
        let Ok(p) = self.retext_plan(&cx, t, text, calls) else {
            let parts = self.tickets.values().filter(|y| y.parent == Some(t) && y.kind == "part");
            return parts.fold(self.opens_ahead(t), |mut out, y| {
                out.extend(self.opens_ahead(y.t));
                out
            });
        };
        p.branches.iter().filter(|(_, _, _, _, plan)| plan.is_err()).flat_map(|(b, ..)| self.opens_ahead(*b)).collect()
    }

    /// The kinds open ticket T would still open by its old text once a reload of the definitions now in place, which
    /// edits the kinds EDITED, has every open ticket of those take its new text: by what of it would keep its old text
    /// (`opens_as_it_was`), else by its own (`opens_ahead`). A branch of a ticket that takes it is that ticket's.
    pub fn opens_after_reload(&self, t: u64, edited: &[String]) -> BTreeSet<String> {
        let Some(x) = self.tickets.get(&t).filter(|x| x.is_open()) else { return BTreeSet::new() };
        if x.kind == "part" && x.parent.and_then(|p| self.tickets.get(&p)).is_some_and(|p| edited.contains(&p.kind) && p.handling.is_none() && p.ask.is_none()) {
            return BTreeSet::new();
        }
        match self.defs.kinds.get(&x.kind).filter(|_| edited.contains(&x.kind)) {
            Some(k) => self.opens_as_it_was(t, &k.text, &self.calls_named(&k.text)),
            None => self.opens_ahead(t),
        }
    }

    /// The kinds open ticket T would still open: those what its frames hold to run, its kind's handlers and a branch
    /// it holds back name. One a hole is still to give is checked when it is given. It reads every node `ahead_each`
    /// gives, a loop's body on its last round too: a stop's Again runs it once more, and what that opens must check.
    pub fn opens_ahead(&self, t: u64) -> BTreeSet<String> {
        let Some(x) = self.tickets.get(&t).filter(|x| x.is_open()) else { return BTreeSet::new() };
        let mut out = BTreeSet::new();
        ahead_each(x, &mut |q, _| out.extend(check::opened(q).cloned()));
        out
    }

    /// What `retext` says of ticket T taking the text P plans (its reply, the reload's line for it), read before anything
    /// changes: where it lands, and where each branch it waits on does.
    fn retexted_said(&self, t: u64, p: &Retexted) -> String {
        let mut said = carry_said(&self.tickets[&t], &p.c.landing);
        for (b, i, _, _, plan) in &p.branches {
            match plan {
                Ok(Plan::Carry(c)) => {
                    let s = carry_said(&self.tickets[b], &c.landing);
                    if s != "took the new text" {
                        said.push_str(&format!("; its branch {i} ({}) {}", self.id(*b), s.trim_start_matches("took the new text; ")));
                    }
                }
                Err(e) => said.push_str(&format!("; its branch {i} ({}) kept its old text: {e}", self.id(*b))),
                _ => {}
            }
        }
        said
    }

    /// What a reload of the definitions in place now would do to each open ticket it reaches, nothing changed (the
    /// owner's Reach: coordd puts his draft's definitions in with `with_defs` and reads this). KINDS: the kinds whose
    /// text it changes; CALLS: the calls whose words it changes, each with the call a step names to be rendered from
    /// them (a call's own steps, and the watcher's `next`, which coordd renders from `choose`). NOW: when it is read.
    ///
    /// - A ticket of an edited kind, by `retext_plan`: `kept`, with its refusal; `withdrawn`, where the node it stands
    ///   at is gone, in `retext`'s words; `given`, when the step it stands at is edited (`apply_carry` keeps `owed`:
    ///   its arguments and an inline call's text stay as they were made); `ahead`, when what it has not run holds a node
    ///   the edit changes or adds, or what it had still to run of the old text holds a node the edit takes out
    ///   (`edited`); else `past`. An agent's delivery ticket is never carried: its agent's next
    ///   run opens its next one on the new text (delivery.rs).
    /// - For an edited call, each step of it a ticket stands at or has not run (`ahead_each`): `own` where the owner's
    ///   own prompt goes in its place (edited at its halt, or a once-edit `templates_seen` lists), `given` where it was
    ///   given (`taken`, which a Given line sets) or HANDED says it is being handed (coordd's `tickets.handed`: rendered
    ///   after the lock is let go, so with whichever text the rendering read), `ahead` where it is still to be sent; `past` for a ticket whose text names the call and has no step of it left.
    ///
    /// A piece (a handler's run, a branch, a part) is said on the ticket it belongs to; a ticket's case is the
    /// weightiest of its own and its pieces' ([`ReachCase`]'s order), its words theirs, in turn.
    pub fn reach(&self, now: &str, kinds: &[String], calls: &[(String, String)], handed: &dyn Fn(u64, u32) -> bool) -> Vec<Reach> {
        let cx = Cx { now: now.to_string(), lines: Vec::new(), effects: Vec::new(), recorded: None };
        // the ticket a piece belongs to: the first above it the owner sees, or an agent's delivery ticket
        let shown = |t: u64| -> Option<u64> {
            let mut u = t;
            loop {
                let y = self.tickets.get(&u)?;
                if (u < HIDDEN && y.kind != "part") || self.delivery_agent(u).is_some() {
                    return Some(u);
                }
                u = y.parent.or(y.under)?;
            }
        };
        let mut found: BTreeMap<u64, Reach> = BTreeMap::new();
        // a case of ticket T, or of a piece of it: the weightiest stands, and the words follow those before
        fn add(found: &mut BTreeMap<u64, Reach>, t: u64, case: ReachCase, said: String) -> &mut Reach {
            let r = found.entry(t).or_insert_with(|| Reach { t, case, said: String::new(), took: None, why: None, agent: None });
            r.case = r.case.max(case);
            if !said.is_empty() {
                if !r.said.is_empty() {
                    r.said.push_str("; ");
                }
                r.said.push_str(&said);
            }
            r
        }
        let open: Vec<&Ticket> = self.tickets.values().filter(|x| x.is_open()).collect();
        // the kinds
        for x in open.iter().filter(|x| kinds.contains(&x.kind) && shown(x.t) == Some(x.t)) {
            if let Some(agent) = self.delivery_agent(x.t) {
                let busy = self.desks.get(&agent).and_then(|d| d.busy).map(|b| format!("; its run open now ({}) ends on the text it began with", self.id(b)));
                add(&mut found, x.t, ReachCase::Ahead, format!("{agent}'s next delivery run opens its next delivery ticket on the new text{}", busy.unwrap_or_default()));
                continue;
            }
            let Some(k) = self.defs.kinds.get(&x.kind) else { continue };
            let p = match self.retext_plan(&cx, x.t, &k.text, &self.calls_named(&k.text)) {
                Ok(p) => p,
                Err(e) => {
                    let r = add(&mut found, x.t, ReachCase::Kept, e.clone());
                    r.took = Some(false);
                    r.why = Some(e.strip_prefix("kept its old text: ").unwrap_or(&e).to_string());
                    continue;
                }
            };
            let (mut ed, mut cut) = (BTreeSet::new(), BTreeSet::new());
            edited(&p.old.pattern, &p.new.pattern, &mut ed, &mut cut);
            // against the handlers it runs (its kind's, and a base's it takes), as `retext` puts the new ones in
            edited_handlers(&x.handlers, &p.new.handlers, &mut ed, &mut cut);
            let said = self.retexted_said(x.t, &p);
            let gone = matches!(p.c.landing, Landing::Gone { .. }) || p.branches.iter().any(|(_, _, _, _, b)| matches!(b, Ok(Plan::Carry(Carried { landing: Landing::Gone { .. }, .. }))));
            let mut case = if gone { ReachCase::Withdrawn } else { ReachCase::Past };
            let mut words = vec![said];
            // the step in flight, its own and each branch's, at a node the edit changes: it keeps what it was made with
            let mut at_step = |y: &Ticket, landing: &Landing| {
                let Landing::Kept { old: on, new: nn } = landing else { return };
                let owed = match &y.status {
                    Status::Awaits { owed, .. } | Status::Asking { owed, .. } if matches!(nn, Prog::Step { .. } | Prog::Decide { .. }) => owed,
                    _ => return,
                };
                if print::prog(on, 0) == print::prog(nn, 0) {
                    return;
                }
                let how = match &y.status {
                    Status::Awaits { since, taken: true, agent, .. } => format!("owed since {since} and given to {agent}"),
                    Status::Awaits { since, agent, .. } if handed(y.t, owed.step) => format!("owed since {since}, being handed to {agent}"),
                    Status::Awaits { since, .. } => format!("owed since {since}, not yet given"),
                    _ => String::from("owed, not yet given"),
                };
                case = case.max(ReachCase::Given);
                words.push(format!("step {} ({}) keeps the words it was made with ({how}); the next time its statement runs, it takes the new ones", owed.step, owed.call));
            };
            at_step(*x, &p.c.landing);
            let mut ahead = Vec::new();
            // a part that runs no more (a loop on its last round, `frames_each`) is not ahead
            let mut read = |q: &Prog, runs: u32| {
                if runs > 0 && ed.contains(&key(q)) {
                    ahead.push(q.at());
                }
            };
            frames_each(&p.c.k, &mut read);
            p.new.handlers.iter().for_each(|h| check::each(&h.body, &mut |q| read(q, syntax::UNBOUNDED)));
            for (b, _, _, n, plan) in &p.branches {
                match plan {
                    Ok(Plan::Carry(c)) => {
                        at_step(&self.tickets[b], &c.landing);
                        frames_each(&c.k, &mut read);
                    }
                    Ok(Plan::Held) => check::each(n, &mut |q| read(q, 1)),
                    _ => {}
                }
            }
            // what it had still to run of the old text, and its branches': a node taken out of it
            let mut lost = Vec::new();
            let mut lose = |q: &Prog, runs: u32| {
                if runs > 0 && cut.contains(&key(q)) {
                    lost.push(q.at());
                }
            };
            ahead_each(x, &mut lose);
            for (b, _, o, _, plan) in &p.branches {
                match plan {
                    Ok(Plan::Carry(_)) => ahead_each(&self.tickets[b], &mut lose),
                    Ok(Plan::Held) => check::each(o, &mut |q| lose(q, 1)),
                    _ => {}
                }
            }
            if !ahead.is_empty() {
                case = case.max(ReachCase::Ahead);
                words.push(format!("what it has not run holds what the edit changes ({} of the new text)", lines_of(ahead.clone())));
            }
            if !lost.is_empty() {
                case = case.max(ReachCase::Ahead);
                words.push(format!("what it has still to run no longer has {} of its old text", lines_of(lost.clone())));
            }
            if ahead.is_empty() && lost.is_empty() && case == ReachCase::Past {
                words.push(String::from("nothing the edit changes is ahead of it"));
            }
            add(&mut found, x.t, case, words.join("; ")).took = Some(true);
        }
        // the calls
        for y in &open {
            let Some(s) = shown(y.t).filter(|s| self.tickets.get(s).is_some_and(Ticket::is_open)) else { continue };
            // its steps ahead of each edited call, in one walk of what it has not run: each one's line, and the most
            // times it runs from here (the rounds left of a loop it is in, no end in a handler: `ahead_each`'s count;
            // more than once inside an `iterate` ahead: `repeats`). A part that runs no more holds none.
            let mut ahead_of: BTreeMap<&str, Vec<(usize, u32)>> = BTreeMap::new();
            if !calls.is_empty() {
                let mut again: BTreeSet<(usize, String)> = BTreeSet::new();
                ahead_each(y, &mut |q, runs| {
                    if runs == 0 {
                        return;
                    }
                    repeats(q, &mut |r| {
                        again.insert(key(r));
                    });
                    if let Some((_, c, at)) = check::step_of(q).filter(|(_, c, _)| calls.iter().any(|(k, _)| k == c)) {
                        ahead_of.entry(c).or_default().push((at, if again.contains(&key(q)) { syntax::UNBOUNDED } else { runs }));
                    }
                });
            }
            for (call, from) in calls {
                let named = if call == from { format!("({call})") } else { format!("({call}, rendered from {from})") };
                let mut any = false;
                match &y.status {
                    Status::Awaits { owed, .. } | Status::Asking { owed, .. } if owed.call == *call && owed.template.is_none() => {
                        any = true;
                        let n = owed.step;
                        let (case, said) = if owed.prompt.is_some() {
                            (ReachCase::Own, format!("step {n} {named} goes with the prompt edited at its halt, as the owner left it"))
                        } else if owed.edit.is_some() {
                            (ReachCase::Own, format!("step {n} {named} goes with the owner's own prompt for it"))
                        } else if handed(y.t, n) {
                            let to = match &y.status {
                                Status::Awaits { agent, .. } => format!(" to {agent}"),
                                _ => String::new(),
                            };
                            (ReachCase::Given, format!("step {n} {named} is being handed{to}: it goes with the text its rendering read"))
                        } else if let Status::Awaits { agent, since, taken: true, .. } = &y.status {
                            (ReachCase::Given, format!("step {n} {named} was given to {agent} (owed since {since}) and keeps the words it was sent; a reminder of it, or the step given again after its process was lost, is rendered from the files and reads the new words"))
                        } else {
                            (ReachCase::Ahead, format!("step {n} {named}, owed and not yet given, is sent with the new words"))
                        };
                        add(&mut found, s, case, said);
                    }
                    _ => {}
                }
                let ahead = ahead_of.get(call.as_str()).cloned().unwrap_or_default();
                if !ahead.is_empty() {
                    any = true;
                    // more than one step of it to send: the times each part ahead runs it, added up (one line in the
                    // rest of this round and again in the next is two)
                    let many = ahead.iter().fold(0u32, |n, (_, runs)| n.saturating_add(*runs)) > 1;
                    let at = lines_of(ahead.iter().map(|(l, _)| *l).collect());
                    // the owner's once-edits that reach its steps, as its view lists them, in `template_for`'s order: a
                    // `step` edit goes with the next step of the call only (it is used then), a `tree` edit with every one
                    let seen = self.templates_seen(y);
                    let once = seen.iter().find(|e| e.call == *call && e.step.is_none() && !e.tree);
                    let tree = seen.iter().find(|e| e.call == *call && e.step.is_none() && e.tree);
                    let thread = |e: &crate::view::ViewTemplate| format!("the owner's own prompt for {call} on {} and the tickets under it (since {})", e.on, e.at);
                    let (case, said) = match (once, tree) {
                        (Some(e), _) if !many => (ReachCase::Own, format!("its step of {call} ahead ({at}) goes with the owner's own prompt for it (since {})", e.at)),
                        (Some(e), rest) => {
                            let rest = rest.map_or_else(|| String::from("the new words"), thread);
                            (ReachCase::Own, format!("of its steps of {call} ahead ({at}), the next goes with the owner's own prompt for it (since {}), and the ones after it with {rest}", e.at))
                        }
                        (None, Some(e)) => (ReachCase::Own, format!("its steps of {call} ahead ({at}) go with {}", thread(e))),
                        (None, None) => (ReachCase::Ahead, format!("its step of {call} ahead {named} ({at}) is rendered with the new words when it is sent")),
                    };
                    add(&mut found, s, case, said);
                }
                // the calls its text names are on its record (`Ticket::calls`); a ticket from before that, by its text
                if !any && y.t == s && (y.calls.contains_key(call.as_str()) || (y.calls.is_empty() && names_call(&y.program, call))) {
                    add(&mut found, s, ReachCase::Past, format!("no step of {call} is ahead of it"));
                }
            }
        }
        for r in found.values_mut() {
            r.agent = self.delivery_agent(r.t);
        }
        found.into_values().collect()
    }

    /// `Line::Edited` with what `reload`: ticket T takes TEXT, its kind's new text, for what it has not run (with
    /// CALLS, the calls TEXT names, already on it: `exec`), and so do the open branches of a `par` or an `any` it waits
    /// on. Refused, and so not logged, when it cannot be carried: it keeps its old text, and the reply says why.
    pub(super) fn retext(&mut self, cx: &mut Cx, t: u64, by: &str, why: &str, text: &str, calls: &BTreeMap<String, CallSig>) -> (bool, Result<String, String>) {
        let plan = match self.retext_plan(cx, t, text, calls) {
            Ok(p) => p,
            Err(e) => return (false, Err(e)),
        };
        let said = self.retexted_said(t, &plan);
        let Retexted { old, new, bad, loose, before, c, branches } = plan;
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.program = text.to_string();
        x.handlers = new.handlers.clone();
        let part = x.trace.parts.len();
        x.trace.parts.push(TracePart { source: 0, by: Some(by.to_string()), prog: new.pattern.clone(), at: None });
        let parts = x.trace.parts.clone();
        let more = check::bound(&new.pattern, &self.defs.kinds);
        x.bound = x.bound.max(u64::from(x.steps) + more);
        x.loose = loose;
        if let Some(why) = &bad {
            self.unchecked(t, why);
        }
        let w = self.apply_carry(cx, t, c, &old.pattern, &new.pattern, before, part, by, why);
        let mut withdrawn: Vec<Withdrew> = w.into_iter().collect();
        for (b, _, o, n, plan) in branches {
            let y = self.tickets.edit().get_mut(&b).unwrap();
            y.trace.parts = parts.clone();
            match plan {
                Ok(Plan::Closed) => {
                    let mut ps = Vec::new();
                    pairs(&o, &n, &mut ps);
                    for (a, b) in ps {
                        if let Some(v) = y.trace.visits.get(&(before, a.at(), print::node(a).1)).cloned() {
                            y.trace.visits.insert((part, b.at(), print::node(b).1), v);
                        }
                    }
                }
                Ok(Plan::Held) => {
                    if let Some(h) = y.held.as_mut() {
                        h.prog = n;
                    }
                }
                Ok(Plan::Carry(c)) => withdrawn.extend(self.apply_carry(cx, b, c, &o, &n, before, part, by, why)),
                Err(_) => {}
            }
        }
        self.tickets.edit().get_mut(&t).unwrap().reloaded = Some(Reloaded { at: cx.now.clone(), by: by.to_string(), why: why.to_string(), said: said.clone(), withdrawn });
        cx.effect(Effect::Changed { t });
        (true, Ok(said))
    }

    /// Where ticket T stands carried from OLD (its program, whose nodes its trace keys in part BEFORE) to NEW, checked:
    /// Err says why it cannot be (it keeps its old text).
    fn plan_carry(&self, t: u64, old: &Prog, new: &Prog, before: usize, agents: &BTreeSet<String>) -> Result<Carried, String> {
        let x = &self.tickets[&t];
        let tr = &x.trace;
        let at = match (&tr.now, tr.src) {
            (Some((p, line, words)), Some(s)) if *p == before && s == *p => (*line, words.clone()),
            (Some((p, line, words)), None) if *p == before && x.shaping.is_some() => (*line, words.clone()),
            _ => return Err(String::from("it runs a program an edit or the answer at its hole gave, not its kind's text")),
        };
        let env = match &x.status {
            Status::Awaits { owed, .. } | Status::Asking { owed, .. } if x.shaping.is_none() => owed.env.clone(),
            Status::Awaits { .. } | Status::Asking { .. } => x.shaping.as_ref().map(|s| s.0.clone()).unwrap_or_default(),
            Status::Rests { env, .. } => env.clone(),
            Status::Waits { children, .. } if !children.is_empty() => Env::new(),
            _ => return Err(String::from("it stands at no step, hole or ticket below")),
        };
        let path = locate(old, &x.k, &at).ok_or_else(|| format!("where it stands (line {}) was not found in its kind's text", at.0))?;
        let c = carry(old, new, &path, &x.k, &env)?;
        if let Some((name, line)) = unbound(&c, &env, agents) {
            return Err(format!("`{name}` (line {line} of the new text) is not bound where it stands: it reads what a statement it is already past would have bound"));
        }
        if let (Landing::Gone { .. }, Status::Waits { children, .. }) = (&c.landing, &x.status) {
            let ids: Vec<String> = children.iter().map(|c| self.id(*c)).collect();
            return Err(format!("it waits on {}, opened by a statement the new text no longer has", ids.join(", ")));
        }
        Ok(c)
    }

    /// Ticket T carried as C (`plan_carry`) from OLD to NEW: its frames, its picture (keyed in part PART now, BEFORE
    /// until now), and what is in flight kept or withdrawn: the step withdrawn, if one is. What it did is said by
    /// `carry_said`, read before.
    #[allow(clippy::too_many_arguments)]
    fn apply_carry(&mut self, cx: &mut Cx, t: u64, c: Carried, old: &Prog, new: &Prog, before: usize, part: usize, by: &str, why: &str) -> Option<Withdrew> {
        let id = self.id(t);
        let Carried { k, dropped, landing } = c;
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.k = k;
        if let Some(c) = dropped.iter().find_map(|f| if let Frame::With { ctx } = f { Some(ctx.clone()) } else { None }) {
            x.ctx = c;
        }
        let was = x.trace.now.clone();
        // what it had done, where the new text has the same nodes: the picture draws the new text with it
        let mut ps = Vec::new();
        pairs(old, new, &mut ps);
        for (o, n) in ps {
            if let Some(v) = x.trace.visits.get(&(before, o.at(), print::node(o).1)).cloned() {
                x.trace.visits.insert((part, n.at(), print::node(n).1), v);
            }
        }
        match landing {
            Landing::Kept { new: nn, .. } => {
                let key = (part, nn.at(), print::node(&nn).1);
                if let Some(v) = was.as_ref().and_then(|w| x.trace.visits.get(w)).cloned() {
                    x.trace.visits.insert(key.clone(), v);
                }
                if x.trace.src.is_some() {
                    x.trace.src = Some(part);
                }
                if x.trace.pending.is_some() {
                    x.trace.pending = Some(key.clone());
                }
                x.trace.now = Some(key);
                // the step in flight: its answer is taken by the new text's heads, and an edit there is checked in it
                let sig = match &nn {
                    Prog::Step { inline: Some((_, head)), .. } => Some((head.clone(), None)),
                    Prog::Step { call, scope, .. } => self.call_of(t, call).map(|s| (s.head, Some(scope.clone()))),
                    Prog::Decide { name, .. } => self.call_of(t, &format!("{}{name}", syntax::DECIDE)).map(|s| (s.head, None)),
                    _ => None,
                };
                let x = self.tickets.edit().get_mut(&t).unwrap();
                let keep = |scope: &BTreeMap<String, Type>, env: &Env| scope.iter().filter(|(n, _)| env.contains_key(*n)).map(|(n, t)| (n.clone(), t.clone())).collect::<BTreeMap<_, _>>();
                match (&mut x.status, &nn) {
                    (Status::Awaits { owed, .. } | Status::Asking { owed, .. }, _) if owed.call != SHAPE => {
                        if let Some((head, scope)) = sig {
                            owed.head = head;
                            if let Some(s) = scope {
                                owed.scope = keep(&s, &owed.env);
                            }
                        }
                    }
                    (Status::Rests { env, scope }, Prog::Hole { scope: s, .. }) => *scope = keep(s, env),
                    _ => {
                        if let (Some((env, scope)), Prog::Hole { scope: s, .. }) = (x.shaping.as_mut(), &nn) {
                            *scope = keep(s, env);
                        }
                    }
                }
                cx.effect(Effect::Changed { t });
                None
            }
            Landing::Gone { run, env } => {
                x.trace.src = Some(part);
                x.trace.pending = None;
                let status = x.status.clone();
                x.shaping = None;
                let w = match status {
                    Status::Awaits { agent, owed, .. } => {
                        let w = Withdrew { on: id.clone(), step: owed.step, call: owed.call.clone(), worker: Some(agent.clone()) };
                        let d = format!("{id}.s{}", owed.step);
                        let x = self.tickets.edit().get_mut(&t).unwrap();
                        if x.given.iter().any(|g| g.to == agent && g.delivery == d) {
                            let reason = format!("its kind's text was edited and its statement is gone ({why})");
                            x.withdrawn.push((agent.clone(), owed.step, owed.call.clone(), false, Some((by.to_string(), reason.clone()))));
                            cx.effect(Effect::Deliver { to: agent.clone() });
                            self.ask_withdrawn(cx, t, &agent, owed.step, by, &reason);
                        }
                        self.end_scoped(cx, t, Some(&agent));
                        Some(w)
                    }
                    Status::Asking { owed, .. } => Some(Withdrew { on: id.clone(), step: owed.step, call: owed.call.clone(), worker: None }),
                    _ => None,
                };
                for f in &dropped {
                    match f {
                        Frame::Scope { agent } => self.end_kept(cx, t, agent),
                        Frame::During { busy, asks, .. } => {
                            self.end_look(cx, *busy, "the block it watched is gone from the edited text");
                            for q in asks.clone() {
                                self.ask_after_look(cx, q);
                            }
                        }
                        _ => {}
                    }
                }
                self.tickets.edit().get_mut(&t).unwrap().status = Status::Waits { children: Vec::new(), par: false };
                cx.effect(Effect::Changed { t });
                self.drive(cx, vec![Work::Run(t, run, env)]);
                w
            }
        }
    }
}
