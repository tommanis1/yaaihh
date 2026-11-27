//! Patterns and continuations as program text (plan/core.md, printed continuations): what a ticket will do with the
//! answer it awaits, in the language an edit is written in. The text parses back with the same parser, except where a
//! frame has no syntax of its own (the rounds left of an `iterate`), which prints as a comment.

use crate::machine::{Env, Frame, Status, Ticket};
use crate::syntax::{Bound, Expr, Handler, Pat, Prog, Worker};
use serde_json::Value;

/// A value as a literal: a text quoted, a constructor with its fields in their order of record, `none` for nothing.
pub fn value(v: &Value) -> String {
    match v {
        Value::Null => "none".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => serde_json::to_string(s).unwrap_or_default(),
        Value::Array(xs) => format!("[{}]", xs.iter().map(value).collect::<Vec<_>>().join(", ")),
        Value::Object(o) => match o.get("head").and_then(Value::as_str) {
            Some(h) => {
                let args: Vec<String> = o.iter().filter(|(k, _)| *k != "head").map(|(_, v)| value(v)).collect();
                if args.is_empty() { h.to_string() } else { format!("{h}({})", args.join(", ")) }
            }
            None => format!("{{{}}}", o.iter().map(|(k, v)| format!("{k} = {}", value(v))).collect::<Vec<_>>().join(", ")),
        },
    }
}

pub fn expr(e: &Expr) -> String {
    match e {
        Expr::Str(s) => serde_json::to_string(s).unwrap_or_default(),
        Expr::Int(n) => n.to_string(),
        Expr::Bool(b) => b.to_string(),
        Expr::None => "none".into(),
        Expr::Var(x) => x.clone(),
        Expr::Post(n) => format!("post {n}"),
        Expr::Cmp { l, r, eq } => format!("{} {} {}", expr(l), if *eq { "==" } else { "!=" }, expr(r)),
        Expr::Rel { l, r, rel } => format!("{} {} {}", expr(l), rel.sym(), expr(r)),
        Expr::And(l, r) => {
            let side = |x: &Expr| if matches!(x, Expr::Or(..)) { format!("({})", expr(x)) } else { expr(x) };
            format!("{} and {}", side(l), side(r))
        }
        Expr::Or(l, r) => format!("{} or {}", expr(l), expr(r)),
        Expr::TextOf { e, .. } => format!("text of {}", expr(e)),
        Expr::ByOf(e) => format!("by of {}", expr(e)),
        Expr::Append(l, r) => format!("{} ++ {}", expr(l), expr(r)),
        Expr::Find { x, e, cond } => format!("find {x} in {} where {}", expr(e), expr(cond)),
        Expr::Quant { all, x, e, cond } => format!("{} {x} in {} where {}", if *all { "all" } else { "any" }, expr(e), expr(cond)),
        Expr::Not(c) if matches!(**c, Expr::And(..) | Expr::Or(..)) => format!("not ({})", expr(c)),
        Expr::Not(c) => format!("not {}", expr(c)),
        Expr::Fact { name, args } => format!("{name}({})", list(args)),
        Expr::ResultOf { e, kind } => format!("result of {}{}", expr(e), kind.as_ref().map(|k| format!(" as {k}")).unwrap_or_default()),
        Expr::Field(e, f) => format!("{}.{f}", expr(e)),
        Expr::Ctx(f) => format!("ctx.{f}"),
        Expr::Con { name, args, .. } if args.is_empty() => name.clone(),
        Expr::Con { name, args, .. } => format!("{name}({})", list(args)),
        Expr::List(xs) => format!("[{}]", list(xs)),
        Expr::Tuple(xs) => format!("({})", list(xs)),
        Expr::Record(fs) => format!("{{{}}}", fs.iter().map(|(f, e)| format!("{f} = {}", expr(e))).collect::<Vec<_>>().join(", ")),
    }
}

fn list(xs: &[Expr]) -> String {
    xs.iter().map(expr).collect::<Vec<_>>().join(", ")
}

pub fn pat(p: &Pat) -> String {
    match p {
        Pat::Wild => "_".into(),
        Pat::Var(x) => x.clone(),
        Pat::Con { name, args, .. } if args.is_empty() => name.clone(),
        Pat::Con { name, args, .. } => format!("{name}({})", args.iter().map(pat).collect::<Vec<_>>().join(", ")),
        Pat::Tuple(ps) => format!("({})", ps.iter().map(pat).collect::<Vec<_>>().join(", ")),
        Pat::Nil => String::from("[]"),
        Pat::One(p) => format!("[{}]", pat(p)),
        Pat::None => String::from("none"),
    }
}

fn bound(b: &Bound) -> String {
    match b {
        Bound::Var(x) => x.clone(),
        Bound::Tuple(xs) => format!("({})", xs.join(", ")),
    }
}

/// A node of a program in one line, and what it is (`step`, `match`, ...): the line a picture of the program shows for
/// it (a match without its arms, a `try` without its body; a `<-`'s first part without the binding).
pub fn node(p: &Prog) -> (&'static str, String) {
    match p {
        Prog::Bind { first, .. } => node(first),
        Prog::Match { e, .. } => ("match", format!("match {}", expr(e))),
        Prog::Par { each: Some(x), .. } => ("par", format!("par {} in {} up to {}{}", x.x, expr(&x.e), x.n, order(x))),
        Prog::Par { .. } => ("par", "par".into()),
        Prog::Any { n, each: Some(x), .. } => ("any", format!("any {n} of {} in {} up to {}", x.x, expr(&x.e), x.n)),
        Prog::Any { n, .. } => ("any", format!("any {n} of")),
        Prog::Iterate { x, init, n, .. } => ("iterate", format!("iterate {x} = {}{}", expr(init), up_to(*n))),
        Prog::For { x, e, n, .. } => ("for", format!("for {x} in {} up to {n}", expr(e))),
        Prog::Layer { on, .. } => ("layer", on.as_ref().map_or_else(|| String::from("layer"), |e| format!("layer on {}", expr(e)))),
        Prog::Try { .. } => ("try", "try".into()),
        Prog::With { f, e, .. } => ("with", format!("with {f} = {}", expr(e))),
        Prog::LetAgent { x, from, .. } => ("let", format!("let agent {x}{}", fork_of(from))),
        Prog::Return { .. } => ("return", prog(p, 0)),
        Prog::Raise { .. } => ("raise", prog(p, 0)),
        Prog::Step { .. } => ("step", prog(p, 0)),
        Prog::Open { .. } => ("open", prog(p, 0)),
        Prog::Post { .. } => ("post", prog(p, 0)),
        Prog::Give { .. } => ("give", prog(p, 0)),
        Prog::Move { .. } => ("move", prog(p, 0)),
        Prog::Hole { .. } => ("hole", prog(p, 0)),
        Prog::Continue { .. } => ("continue", prog(p, 0)),
        Prog::Reply { .. } => ("reply", prog(p, 0)),
        Prog::Tell { .. } => ("tell", prog(p, 0)),
        Prog::Drop { .. } => ("drop", prog(p, 0)),
        Prog::Gather { .. } => ("gather", prog(p, 0)),
        Prog::Mark { .. } => ("mark", prog(p, 0)),
        Prog::Deliver { from: None, .. } => ("deliver", prog(p, 0)),
        Prog::Deliver { .. } | Prog::Fork { .. } => ("fork", prog(p, 0)),
        Prog::Keep { .. } => ("keep", prog(p, 0)),
        Prog::Report { .. } => ("report", prog(p, 0)),
        Prog::During { whens, .. } => ("during", format!("during …{}", whens.iter().map(|w| format!(" when {}({}){}", w.ev, w.x, w.cond.as_ref().map(|c| format!(" where {}", expr(c))).unwrap_or_default())).collect::<String>())),
        Prog::Stop { .. } => ("stop", prog(p, 0)),
        Prog::Decide { .. } => ("decide", prog(p, 0)),
    }
}

pub fn bound_of(b: &Bound) -> String {
    bound(b)
}

pub fn worker(w: &Worker) -> String {
    match w {
        Worker::Named(n) => n.clone(),
        Worker::Owner => "owner".into(),
        Worker::Role(r) => format!("role {r}"),
        Worker::Fresh => "fresh".into(),
        Worker::Expr(e) => expr(e),
        Worker::Fork(a) => format!("fork {}", expr(a)),
    }
}

/// ` = fork A` of `let agent x = fork A in {...}`, or nothing.
fn fork_of(from: &Option<Expr>) -> String {
    from.as_ref().map_or_else(String::new, |a| format!(" = fork {}", expr(a)))
}

/// A step's `on`s and `until`s as written after its call (`syntax::On`), each with a space before it.
fn ons(on: &[crate::syntax::On]) -> String {
    on.iter()
        .map(|o| {
            let cond = o.cond.as_ref().map_or_else(String::new, |c| format!(" where {}", expr(c)));
            match &o.answer {
                Some(a) => format!(" on {}({}){cond} => {}", o.ev, o.x, expr(a)),
                None => format!(" until {}({}){cond}", o.ev, o.x),
            }
        })
        .collect()
}

/// A pattern as text, each line indented by IND spaces.
pub fn prog(p: &Prog, ind: usize) -> String {
    let pad = " ".repeat(ind);
    match p {
        Prog::Return { e, .. } => format!("{pad}return {}", expr(e)),
        Prog::Raise { exc, e: None, .. } => format!("{pad}raise {exc}"),
        Prog::Raise { exc, e: Some(e), .. } => format!("{pad}raise {exc}({})", expr(e)),
        Prog::Step { w, hold, inline: Some((text, head)), on, .. } => format!("{pad}{} {} \"{}\" -> {head}{}", if *hold { "hold" } else { "step" }, worker(w), text.replace('\\', "\\\\").replace('"', "\\\""), ons(on)),
        Prog::Step { w, call, args, hold, on, .. } => format!("{pad}{} {} {call}({}){}", if *hold { "hold" } else { "step" }, worker(w), list(args), ons(on)),
        Prog::Open { kind, args, .. } => format!("{pad}open {kind}({})", list(args)),
        Prog::Post { e, on, by, naming, informing, .. } => {
            let mut s = format!("{pad}post {}", expr(e));
            if let Some(t) = on {
                s.push_str(&format!(" on {}", expr(t)));
            }
            if let Some(w) = by {
                s.push_str(&format!(" as {w}"));
            }
            if !naming.is_empty() {
                s.push_str(&format!(" naming {}", list(naming)));
            }
            if !informing.is_empty() {
                s.push_str(&format!(" informing {}", list(informing)));
            }
            s
        }
        Prog::Give { w, e, .. } => format!("{pad}give {w} {}", expr(e)),
        Prog::Move { n, to, .. } => format!("{pad}move {} to {}", expr(n), expr(to)),
        Prog::Hole { .. } => format!("{pad}..."),
        Prog::Continue { e, .. } => format!("{pad}continue {}", expr(e)),
        Prog::Reply { q, a, .. } => format!("{pad}reply {} {}", expr(q), expr(a)),
        Prog::Tell { agents, p, joins: None, .. } => format!("{pad}tell {} {}", expr(agents), expr(p)),
        Prog::Tell { agents, p, joins: Some(j), .. } => format!("{pad}tell {} {} joining {}", expr(agents), expr(p), expr(j)),
        Prog::Drop { p, .. } => format!("{pad}drop {}", expr(p)),
        Prog::Gather { e, .. } => format!("{pad}gather {}", expr(e)),
        Prog::Mark { p, marks, .. } => format!("{pad}mark {} {}", expr(p), expr(marks)),
        Prog::Deliver { a, items, rest, digest, from, .. } => {
            let rest = rest.as_ref().map(|r| format!(" listing {}", expr(r))).unwrap_or_default();
            let digest = digest.as_ref().map(|r| format!(" digest {}", expr(r))).unwrap_or_default();
            match from {
                None => format!("{pad}deliver {} {}{rest}{digest}", expr(a), expr(items)),
                Some(f) => format!("{pad}fork {} at {} with {}{rest}{digest}", expr(a), expr(f), expr(items)),
            }
        }
        Prog::Fork { a, place, .. } => format!("{pad}fork {} at {}", expr(a), expr(place)),
        Prog::Keep { items, why, .. } => format!("{pad}keep {} {}", expr(items), expr(why)),
        Prog::Report { items, why, .. } => format!("{pad}report {} {}", expr(items), expr(why)),
        Prog::Bind { x, first, rest } => format!("{pad}{} <- {};\n{}", bound(x), prog(first, ind).trim_start(), prog(rest, ind)),
        Prog::Match { e, arms, guards, .. } => {
            let guard = |i: usize| guards.get(i).and_then(Option::as_ref).map_or_else(String::new, |c| format!(" where {}", expr(c)));
            let arms: Vec<String> = arms.iter().enumerate().map(|(i, (p, b))| format!("{pad}  {}{} =>\n{}", pat(p), guard(i), prog(b, ind + 4))).collect();
            format!("{pad}match {} {{\n{}\n{pad}}}", expr(e), arms.join("\n"))
        }
        Prog::Par { branches, each: Some(x), .. } => format!("{pad}par {} in {} up to {}{} {{\n{}\n{pad}}}", x.x, expr(&x.e), x.n, order(x), prog(&branches[0], ind + 2)),
        Prog::Any { n, branches, each: Some(x), .. } => format!("{pad}any {n} of {} in {} up to {} {{\n{}\n{pad}}}", x.x, expr(&x.e), x.n, prog(&branches[0], ind + 2)),
        Prog::Par { branches, .. } => format!("{pad}par(\n{}\n{pad})", branches.iter().map(|b| prog(b, ind + 2)).collect::<Vec<_>>().join(",\n")),
        Prog::Any { n, branches, .. } => format!("{pad}any {n} of (\n{}\n{pad})", branches.iter().map(|b| prog(b, ind + 2)).collect::<Vec<_>>().join(",\n")),
        Prog::Iterate { x, init, n, body, .. } => format!("{pad}iterate {x} = {}{} {{\n{}\n{pad}}}", expr(init), up_to(*n), prog(body, ind + 2)),
        Prog::For { x, e, n, body, .. } => format!("{pad}for {x} in {} up to {n} {{\n{}\n{pad}}}", expr(e), prog(body, ind + 2)),
        Prog::Layer { on, body, .. } => format!("{pad}layer{} {{\n{}\n{pad}}}", on.as_ref().map_or_else(String::new, |e| format!(" on {}", expr(e))), prog(body, ind + 2)),
        Prog::Try { body, arms, .. } => format!("{pad}try {{\n{}\n{pad}}} handle {{\n{}\n{pad}}}", prog(body, ind + 2), handlers(arms, ind + 2)),
        Prog::With { f, e, body, .. } => format!("{pad}with {f} = {} in {{\n{}\n{pad}}}", expr(e), prog(body, ind + 2)),
        Prog::LetAgent { x, from, body, .. } => format!("{pad}let agent {x}{} in {{\n{}\n{pad}}}", fork_of(from), prog(body, ind + 2)),
        Prog::During { body, whens, .. } => format!(
            "{pad}during {{\n{}\n{pad}}}{}",
            prog(body, ind + 2),
            whens.iter().map(|w| format!(" when {}({}){} do {{\n{}\n{pad}}}", w.ev, w.x, w.cond.as_ref().map(|c| format!(" where {}", expr(c))).unwrap_or_default(), prog(&w.body, ind + 2))).collect::<String>()
        ),
        Prog::Stop { e, .. } => format!("{pad}stop {}", expr(e)),
        Prog::Decide { name, args, .. } => format!("{pad}decide {name}({})", list(args)),
    }
}

fn handlers(arms: &[Handler], ind: usize) -> String {
    let pad = " ".repeat(ind);
    arms.iter()
        .map(|h| format!("{pad}{}{} =>\n{}", h.exc, h.x.as_ref().map(|x| format!("({x})")).unwrap_or_default(), prog(&h.body, ind + 2)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// What ticket X will do with what it waits for, as a program in which `answer` is that: the frames of its
/// continuation from the hole outwards. None when it waits for nothing (closed, or asking whether its goals hold).
pub fn continuation(x: &Ticket) -> Option<String> {
    let what = match &x.status {
        Status::Awaits { agent, owed, .. } => format!("// {} {agent}: {}(...), step {}", if owed.hold { "held by" } else { "awaits" }, owed.call, owed.step),
        Status::Asking { owed, role } => format!("// asks who fills {role} for {}(...), step {}", owed.call, owed.step),
        Status::Waits { children, .. } if !children.is_empty() => format!("// waits for {} ticket(s) below", children.len()),
        Status::Rests { .. } => return Some(String::from("// rests at a hole: an edit fills it, giving the kind's result")),
        _ => return None,
    };
    let mut text = String::from("return answer");
    for f in x.k.iter().rev() {
        text = match f {
            Frame::Bind { x: b, rest, .. } if !text.contains('\n') => format!("{} <- {text};\n{}", bound(b), prog(rest, 0)),
            Frame::Bind { x: b, rest, .. } => format!("{} <- (\n{}\n);\n{}", bound(b), indent(&text, 2), prog(rest, 0)),
            Frame::Try { arms, .. } => format!("try {{\n{}\n}} handle {{\n{}\n}}", indent(&text, 2), handlers(arms, 2)),
            Frame::With { .. } => format!("{text}\n// (a with ended here; an edit goes on in the context outside it)"),
            Frame::Scope { agent } => format!("{text}\n// (the agent {agent} of a let agent ends here; an edit ends it now)"),
            Frame::During { whens, busy, .. } => format!(
                "{text}\n// that was the body of a `during … when {}`{}; an edit leaves it here, and its look with it",
                whens.iter().map(|w| w.ev.as_str()).collect::<Vec<_>>().join(" … when "),
                if busy.is_some() { " (a look runs)" } else { "" }
            ),
            Frame::Layer { id, .. } => format!("{text}\n// that was inside layer {id}, whose workspace it shares; an edit leaves the layer when this ends"),
            Frame::Gather => format!("{text}\n// (answer: his burst, every post of his held with the first while this waited; `gather`, E1b)"),
            Frame::Each { x: v, rest, body, .. } => format!(
                "{text}\n// that was a round of a for: {} more round(s) of this follow, which an edit drops unless it writes the loop\n// again:\n//   for {v} in ... {{\n{}\n//   }}",
                rest.len(),
                comment(&prog(body, 4))
            ),
            Frame::Loop { x: v, left, body, .. } if *left == crate::syntax::UNBOUNDED => format!(
                "{text}\n// that was a round of an iterate: while it answers Again, more rounds of this follow, which an edit drops\n// unless it writes the loop again:\n//   iterate {v} = ... {{\n{}\n//   }}",
                comment(&prog(body, 4))
            ),
            Frame::Loop { x: v, left, body, .. } => format!(
                "{text}\n// that was a round of an iterate: while it answers Again, up to {left} more round(s) of this follow, which an\n// edit drops unless it writes the loop again:\n//   iterate {v} = ... up to {left} {{\n{}\n//   }}",
                comment(&prog(body, 4))
            ),
        };
    }
    let ends: Vec<&str> = x.k.iter().filter_map(|f| if let Frame::Scope { agent } = f { Some(agent.as_str()) } else { None }).collect();
    let what = if ends.is_empty() {
        what
    } else {
        format!("{what}\n// an edit ends the agent(s) {} of the let agent scope(s) it leaves (the one the step awaits, once it answers)", ends.join(", "))
    };
    let vars = match &x.status {
        Status::Awaits { owed, .. } | Status::Asking { owed, .. } if !owed.env.is_empty() => format!("\n{}", comment(&format!(" in scope:\n{}", indent(&scope(&owed.env), 3)))),
        Status::Waits { children, .. } if !children.is_empty() => match &x.waiting {
            Some((_, env)) if !env.is_empty() => format!("\n{}", comment(&format!(" in scope:\n{}", indent(&scope(env), 3)))),
            _ => String::new(),
        },
        _ => String::new(),
    };
    Some(format!("{what}{vars}\nanswer <- (in flight);\n{text}"))
}

/// The values in scope at the step a ticket awaits, as `name = value` lines.
pub fn scope(env: &Env) -> String {
    env.iter().map(|(k, v)| format!("{k} = {}", value(v))).collect::<Vec<_>>().join("\n")
}

fn indent(s: &str, n: usize) -> String {
    let pad = " ".repeat(n);
    s.lines().map(|l| format!("{pad}{l}")).collect::<Vec<_>>().join("\n")
}

fn comment(s: &str) -> String {
    s.lines().map(|l| format!("//{l}")).collect::<Vec<_>>().join("\n")
}

/// An iterate's ` up to N`, or nothing for one with no count (`syntax::UNBOUNDED`).
fn up_to(n: u32) -> String {
    if n == crate::syntax::UNBOUNDED { String::new() } else { format!(" up to {n}") }
}

/// A `par`'s order as written after `up to n` (` as K after A reach KIND`), or nothing.
fn order(x: &crate::syntax::Each) -> String {
    match &x.order {
        Some(o) => format!(" as {} after {}{}", expr(&o.key), expr(&o.after), o.reach.as_ref().map_or(String::new(), |k| format!(" reach {k}"))),
        None => String::new(),
    }
}
