//! A net drawn as written (owner m127, the Clerk page): its source structure, not its workflow net. Statements become
//! nested blocks ([`NetFlow`]): a sequence, a branch per `if`/`match` (its arms side by side), a frame per `for`. The
//! model calls and decisions inside a statement's expressions come first as their own steps, in evaluation order; a
//! `match` or `if` on exactly one call or decision has that call as its head, so `match decide end-of-turn(…)` is one
//! decision with an arm per option. Each step keeps the source lines it covers, so the page maps a click to the text and
//! the caret to a step, and a run's steps (by line) to the path it took.

use super::syntax::{self, Arg, Arm, Effect, Expr, Net, Op, Part, Pat, Place, Stmt};
use coord_types::{FlowArm, FlowNode, NetFlow, NetGraph};

/// The nets of `.net` text TEXT (FILE names it in errors), drawn; the parse error when it does not parse.
pub fn graphs(file: &str, text: &str) -> Result<Vec<NetGraph>, String> {
    Ok(syntax::parse(file, text)?.nets.iter().map(graph).collect())
}

fn graph(net: &Net) -> NetGraph {
    let sessions: Vec<String> = net.sessions.iter().map(|s| format!("context {}: per {}", s.name, s.per.word())).collect();
    let start = FlowNode {
        kind: "start".into(),
        label: format!("on {}({})", net.event, net.var),
        detail: sessions.join(", "),
        lines: (net.at.line, net.sessions.last().map_or(net.at.line, |s| s.at.line)),
        ..Default::default()
    };
    NetGraph { name: net.name.clone(), start, body: block(&net.body) }
}

fn block(ss: &[Stmt]) -> NetFlow {
    let mut out = Vec::new();
    for s in ss {
        out.extend(stmt(s));
        if ends(out.last()) {
            break; // nothing after a fault runs
        }
    }
    NetFlow::Seq(out)
}

/// Whether FLOW ends the run (a fault, or a branch whose every arm does).
fn ends(flow: Option<&NetFlow>) -> bool {
    match flow {
        Some(NetFlow::Node(n)) => n.terminal,
        Some(NetFlow::Seq(xs)) => ends(xs.last()),
        Some(NetFlow::Branch { arms, .. }) => !arms.is_empty() && arms.iter().all(|a| ends(Some(&a.body))),
        _ => false,
    }
}

fn node(kind: &str, label: String, line: usize) -> FlowNode {
    FlowNode { kind: kind.into(), label, lines: (line, line), ..Default::default() }
}

/// The calls and decisions in E, in evaluation order, as steps (each at LINE, or its own line).
fn calls(e: &Expr, line: usize, out: &mut Vec<FlowNode>) {
    match e {
        Expr::Call { name, args, place, with, at } => {
            args.iter().for_each(|a| calls(&a.value, line, out));
            with.iter().for_each(|(_, v)| calls(v, line, out));
            let mut n = node("call", format!("{name}({})", show_args(args)), at.line.max(line));
            n.session = place.context().cloned();
            let used = match place {
                Place::With(s) => format!("with {s} (a fork, not advanced)"),
                Place::Digest => "digest".into(),
                Place::Fresh | Place::In(_) => String::new(),
            };
            n.detail = [used, show_with(with)].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(", ");
            out.push(n);
        }
        Expr::Decide { name, args, with, at } => {
            args.iter().for_each(|a| calls(&a.value, line, out));
            with.iter().for_each(|(_, v)| calls(v, line, out));
            let mut n = node("decide", format!("decide {name}({})", show_args(args)), at.line.max(line));
            n.detail = show_with(with);
            out.push(n);
        }
        Expr::Builtin { args, .. } => args.iter().for_each(|a| calls(&a.value, line, out)),
        Expr::Str(parts) => parts.iter().for_each(|p| if let Part::Hole(h) = p { calls(h, line, out) }),
        Expr::Cmp(a, _, b) | Expr::And(a, b) | Expr::Or(a, b) => {
            calls(a, line, out);
            calls(b, line, out);
        }
        Expr::Not(a) => calls(a, line, out),
        Expr::Path(_) | Expr::Int(_) | Expr::Bool(_) | Expr::None | Expr::Owner | Expr::Variant(_) => {}
    }
}

/// The last line any step of FLOW covers.
fn last_line(flow: &NetFlow) -> usize {
    match flow {
        NetFlow::Seq(xs) => xs.iter().map(last_line).max().unwrap_or(0),
        NetFlow::Node(n) => n.lines.1,
        NetFlow::Branch { head, arms } => arms.iter().map(|a| last_line(&a.body).max(a.line)).max().unwrap_or(0).max(head.lines.1),
        NetFlow::Each { head, body } => last_line(body).max(head.lines.1),
    }
}

fn stmt(s: &Stmt) -> Vec<NetFlow> {
    let line = s.at().line;
    let mut pre = Vec::new();
    let mut flows = Vec::new();
    match s {
        Stmt::Let { value, .. } | Stmt::Set { value, .. } => {
            let target = match s {
                Stmt::Let { var, .. } => var.clone(),
                Stmt::Set { path, .. } => path.join("."),
                _ => unreachable!("matched above"),
            };
            let written = match s {
                Stmt::Set { add: true, .. } => format!("{target} += {}", show(value)),
                Stmt::Set { .. } => format!("{target} := {}", show(value)),
                _ => format!("let {target} = {}", show(value)),
            };
            calls(value, line, &mut pre);
            match pre.last_mut() {
                // the value is the call: one step, its result named
                Some(last) if is_call(value) => last.detail = join(&[&format!("→ {target}"), &last.detail]),
                _ => pre.push(node("data", written, line)),
            }
        }
        Stmt::If { cond, then, els, .. } => {
            calls(cond, line, &mut pre);
            let head = match (pre.len(), cond) {
                (1, Expr::Call { .. } | Expr::Decide { .. }) => pre.pop().unwrap(),
                _ => node("test", format!("if {}", show(cond)), line),
            };
            let arms = vec![
                FlowArm { label: "yes".into(), line, body: block(then) },
                FlowArm { label: "no".into(), line: els.first().map_or(line, |s| s.at().line), body: block(els) },
            ];
            flows.push(branch(head, arms));
        }
        Stmt::Match { scrut, arms, .. } => {
            calls(scrut, line, &mut pre);
            let head = match (pre.len(), scrut) {
                (1, Expr::Call { .. } | Expr::Decide { .. }) => pre.pop().unwrap(),
                _ => node("test", format!("match {}", show(scrut)), line),
            };
            for a in arms {
                if let Some(g) = &a.guard {
                    calls(g, a.at.line, &mut pre);
                }
            }
            flows.push(branch(head, arms.iter().map(arm).collect()));
        }
        Stmt::For { var, list, filter, par, body, .. } => {
            calls(list, line, &mut pre);
            if let Some(f) = filter {
                calls(f, line, &mut pre);
            }
            let mut label = format!("for each {var} in {}", show(list));
            if let Some(f) = filter {
                label = format!("{label} where {}", show(f));
            }
            let mut head = node("each", label, line);
            if *par {
                head.detail = "all at once".into();
            }
            let body = block(body);
            head.lines.1 = last_line(&body).max(line);
            flows.push(NetFlow::Each { head, body: Box::new(body) });
        }
        Stmt::Effect { effect, args, .. } => {
            args.iter().for_each(|a| calls(&a.value, line, &mut pre));
            let mut n = node("effect", format!("{}({})", effect.name(), show_args(args)), line);
            match effect {
                Effect::Fault(name) => {
                    n.kind = "fault".into();
                    n.label = format!("fault {name}({})", show_args(args));
                    n.terminal = true;
                }
                Effect::Raise(name, after) => {
                    n.kind = "raise".into();
                    n.label = format!("{}({})", effect.name(), show_args(args));
                    n.out.push(match after {
                        None => format!("raises {name}"),
                        Some(d) => format!("raises {name} after {}", super::syntax::show_duration(*d)),
                    });
                }
                Effect::Accept => {
                    n.label = "accept".into();
                    if let Some(w) = args.iter().find(|a| a.name.as_deref() == Some("wake")).filter(|a| !matches!(a.value, Expr::None)) {
                        n.out.push(format!("wakes on {}", show(&w.value)));
                    }
                }
                _ => {}
            }
            pre.push(n);
        }
    }
    pre.into_iter().map(NetFlow::Node).chain(flows).collect()
}

fn is_call(e: &Expr) -> bool {
    matches!(e, Expr::Call { .. } | Expr::Decide { .. })
}

fn join(parts: &[&str]) -> String {
    parts.iter().filter(|p| !p.is_empty()).cloned().collect::<Vec<_>>().join(" · ")
}

/// A branch on HEAD; its lines run to its last arm's.
fn branch(mut head: FlowNode, arms: Vec<FlowArm>) -> NetFlow {
    let end = arms.iter().map(|a| last_line(&a.body).max(a.line)).max().unwrap_or(head.lines.1);
    head.lines.1 = head.lines.1.max(end);
    NetFlow::Branch { head, arms }
}

fn arm(a: &Arm) -> FlowArm {
    let mut label = match &a.pat {
        Pat::Variant(v, fields) if fields.is_empty() => v.clone(),
        Pat::Variant(v, fields) => format!("{v}({})", fields.join(", ")),
        Pat::Some(x) => format!("{x}?"),
        Pat::None => "none".into(),
        Pat::Option(o) => o.clone(),
        Pat::Wild => "otherwise".into(),
    };
    if let Some(g) = &a.guard {
        label = format!("{label} if {}", show(g));
    }
    FlowArm { label, line: a.at.line, body: block(&a.body) }
}

fn show_args(args: &[Arg]) -> String {
    args.iter().map(|a| match &a.name {
        Some(n) => format!("{n}: {}", show(&a.value)),
        None => show(&a.value),
    }).collect::<Vec<_>>().join(", ")
}

fn show_with(with: &[(String, Expr)]) -> String {
    if with.is_empty() { String::new() } else { format!("with {}", with.iter().map(|(k, v)| format!("{k}: {}", show(v))).collect::<Vec<_>>().join(", ")) }
}

/// E as source text (a string literal shortened).
fn show(e: &Expr) -> String {
    match e {
        Expr::Path(p) => p.join("."),
        Expr::Str(parts) => {
            let t: String = parts.iter().map(|p| match p {
                Part::Text(t) => t.clone(),
                Part::Hole(h) => format!("{{{{ {} }}}}", show(h)),
            }).collect();
            let t = if t.chars().count() > 24 { format!("{}…", t.chars().take(24).collect::<String>()) } else { t };
            format!("\"{t}\"")
        }
        Expr::Int(n) => n.to_string(),
        Expr::Bool(b) => b.to_string(),
        Expr::None => "none".into(),
        Expr::Owner => "owner".into(),
        Expr::Variant(v) => v.clone(),
        Expr::Call { name, args, .. } => format!("{name}({})", show_args(args)),
        Expr::Decide { name, args, .. } => format!("decide {name}({})", show_args(args)),
        Expr::Builtin { name, args } => format!("{name}({})", show_args(args)),
        Expr::Cmp(a, op, b) => format!("{} {} {}", show(a), match op {
            Op::Eq => "==",
            Op::Ne => "!=",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
        }, show(b)),
        Expr::And(a, b) => format!("{} and {}", show(a), show(b)),
        Expr::Or(a, b) => format!("{} or {}", show(a), show(b)),
        Expr::Not(a) => format!("not {}", show(a)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped turn-end net draws as written: a repeat accepted at once, else its call into the clerk session
    /// named, the loop framing the match on the label, then held mail, the waits and the decision heading a branch per
    /// option; lines kept.
    #[test]
    fn the_clerk_net_draws_as_written() {
        let path = format!("{}/../../../agent-base/nets/clerk.net", env!("CARGO_MANIFEST_DIR"));
        let text = std::fs::read_to_string(&path).unwrap();
        let gs = graphs(&path, &text).unwrap();
        let g = gs.iter().find(|g| g.name == "turn_end").unwrap();
        assert_eq!(g.start.label, "on turn_end(t)");
        let NetFlow::Seq(top) = &g.body else { panic!() };
        let NetFlow::Branch { head, arms } = &top[0] else { panic!("{:?}", top[0]) };
        assert_eq!((head.label.as_str(), arms.len()), ("if t.repeat", 2));
        let NetFlow::Seq(xs) = &arms[1].body else { panic!() };
        let NetFlow::Node(turn) = &xs[0] else { panic!("{:?}", xs[0]) };
        assert_eq!((turn.kind.as_str(), turn.session.as_deref()), ("call", Some("clerk")));
        assert!(turn.label.starts_with("turn(t.agent, t.text, t.sent, t.waits, t.open") && turn.detail.contains('r'));
        let NetFlow::Each { head, body } = &xs[1] else { panic!("{:?}", xs[1]) };
        assert!(head.label.starts_with("for each p in r.pieces") && head.lines.1 > head.lines.0);
        let NetFlow::Seq(inner) = body.as_ref() else { panic!() };
        let NetFlow::Branch { arms, .. } = &inner[0] else { panic!() };
        assert_eq!(arms.iter().map(|a| a.label.as_str()).collect::<Vec<_>>(), vec!["Question", "Status", "otherwise"]);
        // held mail went on, else the waits by fact, else the decision
        let NetFlow::Branch { head, arms } = xs.last().unwrap() else { panic!() };
        assert_eq!(head.label, "if t.continues");
        let NetFlow::Seq(no) = &arms[1].body else { panic!() };
        let NetFlow::Branch { head, arms } = &no[0] else { panic!() };
        assert!(head.label.starts_with("if len(t.waits)"));
        let NetFlow::Seq(yes) = &arms[0].body else { panic!() };
        let NetFlow::Node(accept) = &yes[0] else { panic!() };
        assert_eq!((accept.label.as_str(), accept.out.as_slice()), ("accept", ["wakes on check(joined(t.waits))".to_string()].as_slice()));
        let NetFlow::Seq(no) = &arms[1].body else { panic!() };
        let NetFlow::Branch { head, arms } = &no[0] else { panic!() };
        assert_eq!(head.kind, "decide");
        assert_eq!(arms.iter().map(|a| a.label.as_str()).collect::<Vec<_>>(), vec!["accept", "nudge", "escalate"]);
        // accept => match s { Blocked(q) => accept(wake: answer(q)) … }: the wake leaves as an arc
        let NetFlow::Seq(accept) = &arms[0].body else { panic!() };
        let NetFlow::Branch { arms: states, .. } = &accept[0] else { panic!() };
        let NetFlow::Seq(blocked) = &states[0].body else { panic!() };
        let NetFlow::Node(n) = &blocked[0] else { panic!() };
        assert_eq!(n.out, vec!["wakes on answer(q)".to_string()]);
    }

    /// Every net shipped draws connected: walking the drawn lines from the event reaches every step, arm label and the
    /// end, with the arms side by side, stacked, and at the room the Clerk page gives (owner m130).
    #[test]
    fn every_shipped_net_draws_connected() {
        use coord_types::netlayout::{layout, unreached, Font};
        let dir = format!("{}/../../../agent-base/nets", env!("CARGO_MANIFEST_DIR"));
        let measure = |t: &str, f: Font, max: f32| {
            let cw = if f == Font::Label { 7.0 } else { 6.0 };
            let w = t.chars().count() as f32 * cw;
            (w.min(max), 16.0 * (w / max).ceil().max(1.0))
        };
        let mut drawn = 0;
        for e in std::fs::read_dir(&dir).unwrap() {
            let path = e.unwrap().path();
            if path.extension().and_then(|x| x.to_str()) != Some("net") { continue }
            let text = std::fs::read_to_string(&path).unwrap();
            for g in graphs(&path.display().to_string(), &text).unwrap() {
                for room in [120.0, coord_types::netlayout::ROOM, 10_000.0] {
                    let l = layout(&g, &measure, room, None);
                    assert!(unreached(&l).is_empty(), "{} in {room}: {:?} not joined", g.name, unreached(&l));
                    if room == coord_types::netlayout::ROOM {
                        assert!(l.w <= room, "{} is {} wide in {room}", g.name, l.w);
                    }
                    drawn += 1;
                    // the walk sees a cut: without the line into the end, the end is not joined
                    let mut cut = l.clone();
                    cut.segs.pop();
                    assert_eq!(unreached(&cut), vec!["done".to_string()], "{}", g.name);
                }
            }
        }
        assert!(drawn >= 9, "{drawn}");
    }

    #[test]
    fn a_text_that_does_not_parse_says_where() {
        assert!(graphs("x.net", "net a on turn_end(t) {").unwrap_err().contains("x.net"));
    }
}
