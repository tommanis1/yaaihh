//! Checking kinds before they are used (semantics.tex, Typing): `Δ ⊢ p : T ! Ξ`, every match exhaustive with no
//! dead arm, a kind's pattern of its result type raising only its exceptions, no kind opening itself (directly or
//! through others), and the cost bound `|p|`. Checking also fills in the field names of every constructor (written
//! positionally, `Accept(note)`) from the type expected there, so the machine can build and take apart values.

use crate::syntax::{Bound, Expr, Kind, Pat, Prog, Worker};
pub use crate::given::{Given, Names, agents_given, names_given};
use crate::types::Type;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A call as a kind sees it: its parameters, the type of the head it answers with, whether it is shown posts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CallSig {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub head: Type,
    pub blind: bool,
    /// `asks C` (N-101, the owner: "3 no same session"): constructor C of HEAD, of one Text field, is the call's
    /// question: an answer formed to it is not taken but asked, and the answer to it goes back to the asker's session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asks: Option<String>,
    /// `where COND` after its parameters (the model classes, paper-research N-101): a condition on `model`, the model
    /// of the agent a step of the call goes to, as written (`model >= medium`); coordd holds it to the project's
    /// policy, which says the models' order and the classes (`syntax::model_cond` reads it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cond: Option<String>,
    /// `default C` (N-487): constructor C of HEAD, with no fields, is the answer an owner's step of the call takes by
    /// itself (`Line::Lapsed`); only the owner's steps may name such a call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

impl CallSig {
    /// The type a step of this call gives its program: HEAD without the question `asks` names, which is never taken.
    pub fn answers(&self) -> Type {
        match (&self.head, &self.asks) {
            (Type::Variant(cs), Some(c)) => Type::Variant(cs.iter().filter(|(n, _)| n != c).cloned().collect()),
            (t, _) => t.clone(),
        }
    }
}

/// The type of field F of a value of type T; of a list, the list of each element's (N-101 p226, G9).
pub(crate) fn field_type(t: &Type, f: &str) -> Result<Type, String> {
    Ok(match t {
        Type::Record(fs) => fs.iter().find(|(n, _)| n == f).map(|(_, t)| t.clone()).ok_or_else(|| format!("no field {f}"))?,
        Type::Variant(cs) if cs.len() == 1 => cs[0].1.iter().find(|(n, _)| n == f).map(|(_, t)| t.clone()).ok_or_else(|| format!("no field {f}"))?,
        Type::List(e) => Type::List(Box::new(field_type(e, f)?)),
        Type::Opt(e) => Type::Opt(Box::new(field_type(e, f)?)),
        Type::Any => Type::Any,
        t => return Err(format!("a {t} has no field {f}")),
    })
}

/// Why a value of type T may not stand in a text's `{{ }}` (a call's template, an inline step's text, a post's), if
/// it may not: it has no one way to be written (paper-research N-101 p880). WHOLE: the place names the value itself,
/// not a field reached through one of no known type.
pub fn unwritable(t: &Type, whole: bool) -> Option<&'static str> {
    match t {
        Type::Variant(_) => Some("a value with alternatives is not written whole: name one of its fields, or choose by `match`"),
        Type::Any if whole => Some("a value of no known type has no one way to be written"),
        Type::Tuple(_) => Some("a tuple has no one way to be written: bind its parts and name them"),
        Type::Opt(x) => unwritable(x, whole),
        Type::List(x) if matches!(**x, Type::Variant(_) | Type::Tuple(_)) || (whole && **x == Type::Any) => Some("a list of values with alternatives, tuples or values of no known type has no one way to be written"),
        _ => None,
    }
}

/// The type of the place `{{ NAME.PATH }}` in scope D, when the path reads (None where it does not: a place the
/// checker lets through).
pub(crate) fn place_type(d: &BTreeMap<String, Type>, name: &str, path: &[String]) -> Option<Type> {
    path.iter().try_fold(d.get(name)?.clone(), |t, f| field_type(&t, f).ok())
}

/// The first place of TEXT that names a value `unwritable` refuses, with why, in the words WHAT (`the step's text`).
fn unwritable_place(text: &str, d: &BTreeMap<String, Type>, what: &str) -> Result<(), String> {
    for (name, path, _) in crate::syntax::placeholders(text) {
        if let Some(why) = place_type(d, &name, &path).and_then(|t| unwritable(&t, path.is_empty())) {
            let place = std::iter::once(name.as_str()).chain(path.iter().map(String::as_str)).collect::<Vec<_>>().join(".");
            return Err(format!("{what} names `{{{{ {place} }}}}`: {why}"));
        }
    }
    Ok(())
}

/// Why call SIG may not be declared, if it may not: a parameter of a name the engine gives a meaning of its own, which
/// a value of the call's would be overwritten by: `of`, what every runner step carries, the work it is for (coordd
/// project.rs). `not` is left free: it means something only to a decision (`Machine::decide_facts`, one value or a
/// list), and a call's own `not` (the draft's `trace`) and a recorded decision's `not: Text?` are both loaded
/// (docket-core, docket-try's replay of b247792).
pub fn reserved(sig: &CallSig) -> Result<(), String> {
    if sig.params.iter().any(|(p, _)| p == "of") {
        return Err(String::from("a parameter is not named `of`: every runner step carries `of`, the work it is for"));
    }
    Ok(())
}

/// Whether call NEW may replace OLD (a later folder's call of the same name, N-101): the same parameters, of the same
/// types, in the same order; and an answer that extends OLD's: each of its constructors one of OLD's, its fields
/// starting with that one's (each fitting), more parts allowed after them. Why not otherwise.
pub fn replaces(new: &CallSig, old: &CallSig) -> Result<(), String> {
    let show = |ps: &[(String, Type)]| ps.iter().map(|(n, t)| format!("{n}: {t}")).collect::<Vec<_>>().join(", ");
    if new.params != old.params {
        return Err(format!("it takes ({}), that one ({})", show(&new.params), show(&old.params)));
    }
    match (&new.head, &old.head) {
        (Type::Variant(ns), Type::Variant(_)) => {
            for (c, fs) in ns {
                let Some(gs) = old.head.constructor(c) else {
                    return Err(format!("its answer {c} is not one of that one's ({})", old.head));
                };
                if !Type::Variant(vec![(c.clone(), fs.clone())]).fits(&Type::Variant(vec![(c.clone(), gs.clone())])) {
                    return Err(format!("its {c}({}) does not start with that one's {c}({})", show(fs), show(gs)));
                }
            }
            Ok(())
        }
        (n, o) if n.fits(o) => Ok(()),
        (n, o) => Err(format!("it answers {n}, that one {o}")),
    }
}

/// Exceptions any step can raise, which the types do not track (semantics.tex: Silent, Malformed, NoWorker, Cancelled,
/// Owed; NoCall, a step whose call is not loaded; Ended, a step whose job the runner ended, runner.rs; RunnerRefused, a
/// step the runner refused to take, Line::RunnerRefused).
pub const UNTRACKED: &[&str] = &["Silent", "Malformed", "NoWorker", "NoLayer", "Cancelled", "Owed", "NoCall", "NoResult", "Ended", "RunnerRefused", "NotFound"];

type Delta = BTreeMap<String, Type>;
type Excs = BTreeSet<String>;

/// A duty broken at a step's answer (N-101, machine/violated.rs): raised by a step to an agent only when the project's
/// policy declares duties; never listed in `raises` (any such step can raise it, as Silent).
pub const VIOLATED: &str = "Violated";

thread_local! {
    /// The kinds being checked are a project's whose policy declares duties (`check_kinds_with`).
    static DUTIES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// As `check_kinds`, for a project whose policy declares duties (DUTIES): a step to an agent may raise Violated, so a
/// `try` around one may handle it; without duties such an arm is refused as one that can never be taken.
pub fn check_kinds_with(kinds: Vec<Kind>, calls: &BTreeMap<String, CallSig>, events: &crate::syntax::Events, named: Option<&crate::syntax::Named>, duties: bool) -> (BTreeMap<String, Kind>, Vec<String>) {
    let _duties = self::duties(duties);
    check_kinds(kinds, calls, events, named)
}

/// While the guard lives, programs are checked as a project's whose policy declares duties, or not (DUTIES): the
/// machine holds one through each input, for the programs checked as a ticket opens or is edited.
pub struct Duties(bool);

impl Drop for Duties {
    fn drop(&mut self) {
        DUTIES.set(self.0);
    }
}

pub fn duties(on: bool) -> Duties {
    Duties(DUTIES.replace(on))
}

/// Whether an exception need not be listed in `raises`: the untracked ones, and Violated.
fn unlisted(e: &str) -> bool {
    UNTRACKED.contains(&e) || e == VIOLATED
}

/// The kinds that pass, each checked (constructors filled in), and why each refused one was refused.
pub fn check_kinds(kinds: Vec<Kind>, calls: &BTreeMap<String, CallSig>, events: &crate::syntax::Events, named: Option<&crate::syntax::Named>) -> (BTreeMap<String, Kind>, Vec<String>) {
    let mut errors = Vec::new();
    let mut by: BTreeMap<String, Kind> = BTreeMap::new();
    for k in kinds {
        if by.contains_key(&k.name) || k.name == "part" {
            errors.push(format!("{}:{}: kind {} is declared twice (or is the system's `part`)", k.file, k.line, k.name));
            continue;
        }
        by.insert(k.name.clone(), k);
    }
    // a kind takes its base's handlers for the events it has none of its own for (one level: a base has no base);
    // every kind starts from `base` if there is one, unless it names another
    let bases: BTreeMap<String, Kind> = by.clone();
    let mut no_base = Vec::new();
    for k in by.values_mut() {
        let Some(b) = base_of(k, &bases) else { continue };
        match bases.get(&b) {
            Some(base) if base.base.is_none() => inherit(k, base),
            Some(_) => no_base.push((k.name.clone(), format!("{}:{}: kind {} refused: its base {b} has a base itself; a base is one level", k.file, k.line, k.name))),
            None => no_base.push((k.name.clone(), format!("{}:{}: kind {} refused: there is no kind {b} to take handlers from", k.file, k.line, k.name))),
        }
    }
    for (n, e) in no_base {
        by.remove(&n);
        errors.push(e);
    }
    // the signatures every pattern is checked against
    let sigs: BTreeMap<String, (Vec<(String, Type)>, Type, Vec<String>)> =
        by.iter().map(|(n, k)| (n.clone(), (k.form.clone(), k.result.clone(), k.raises.clone()))).collect();
    let mut ok: BTreeMap<String, Kind> = BTreeMap::new();
    for (n, mut k) in by {
        // every error of the kind, each its own line (the owner's program read whole, N-101)
        let all = all::Collect::default();
        match check_kind_in(&mut k, calls, &sigs, events, named, true, Some(&all)) {
            Ok(()) => {
                ok.insert(n, k);
            }
            Err(_) => errors.extend(all.take().into_iter().map(|e| format!("{}: kind {n} refused: {e}", k.file))),
        }
    }
    // a kind that opens a refused or unknown kind is refused; a kind on a cycle of opens is refused
    loop {
        let bad: Vec<String> = ok.iter().filter(|(_, k)| opens(&k.pattern).iter().any(|o| !ok.contains_key(o))).map(|(n, _)| n.clone()).collect();
        if bad.is_empty() {
            break;
        }
        for n in bad {
            let k = ok.remove(&n).unwrap();
            errors.push(format!("{}:{}: kind {n} refused: it opens a kind that was refused", k.file, k.line));
        }
    }
    for n in ok.keys().cloned().collect::<Vec<_>>() {
        if reaches(&ok, &n, &n, &mut BTreeSet::new()) {
            let k = &ok[&n];
            errors.push(format!("{}:{}: kind {n} refused: it opens itself, directly or through others, so its cost has no bound", k.file, k.line));
        }
    }
    ok.retain(|n, _| !reaches_self(n, &errors));
    (ok, errors)
}

fn reaches_self(n: &str, errors: &[String]) -> bool {
    errors.iter().any(|e| e.contains(&format!("kind {n} refused: it opens itself")))
}

fn reaches(kinds: &BTreeMap<String, Kind>, from: &str, target: &str, seen: &mut BTreeSet<String>) -> bool {
    let Some(k) = kinds.get(from) else { return false };
    for o in opens(&k.pattern) {
        if o == target {
            return true;
        }
        if seen.insert(o.clone()) && reaches(kinds, &o, target, seen) {
            return true;
        }
    }
    false
}

/// Whether P may wait on someone: a step or a hold anywhere in it (the kinds it opens aside).
pub fn has_step(p: &Prog) -> bool {
    match p {
        // a decision waits as a step does (the owner may answer it); so does his held burst (`gather`)
        Prog::Step { .. } | Prog::Decide { .. } | Prog::Gather { .. } => true,
        Prog::Bind { first, rest, .. } => has_step(first) || has_step(rest),
        Prog::Match { arms, .. } => arms.iter().any(|(_, b)| has_step(b)),
        Prog::Par { branches, .. } | Prog::Any { branches, .. } => branches.iter().any(has_step),
        Prog::Iterate { body, .. } | Prog::For { body, .. } | Prog::Layer { body, .. } | Prog::With { body, .. } | Prog::LetAgent { body, .. } => has_step(body),
        Prog::Try { body, arms, .. } => has_step(body) || arms.iter().any(|h| has_step(&h.body)),
        Prog::During { body, whens, .. } => has_step(body) || whens.iter().any(|w| has_step(&w.body)),
        _ => false,
    }
}

/// The call of the first step P gives agent X: what an agent made by `let agent x` starts as (its role prompt).
pub fn first_call(p: &Prog, x: &str) -> Option<String> {
    match p {
        Prog::Step { w: Worker::Named(n), call, .. } if n == x => Some(call.clone()),
        Prog::Bind { first, rest, .. } => first_call(first, x).or_else(|| first_call(rest, x)),
        Prog::Match { arms, .. } => arms.iter().find_map(|(_, b)| first_call(b, x)),
        Prog::Par { branches, .. } | Prog::Any { branches, .. } => branches.iter().find_map(|b| first_call(b, x)),
        Prog::Iterate { body, .. } | Prog::For { body, .. } | Prog::Layer { body, .. } | Prog::With { body, .. } => first_call(body, x),
        Prog::LetAgent { x: y, body, .. } if y != x => first_call(body, x),
        Prog::Try { body, arms, .. } => first_call(body, x).or_else(|| arms.iter().find_map(|h| first_call(&h.body, x))),
        Prog::During { body, whens, .. } => first_call(body, x).or_else(|| whens.iter().find_map(|w| first_call(&w.body, x))),
        _ => None,
    }
}

/// The steps of P that make an agent with a model of its own (the model classes, paper-research N-101): each `step
/// fresh c(...)`'s call, and each `let agent x in {...}`'s first call to x, with its line. A fork keeps the model of
/// what it copies, so it is none of these.
pub fn makes_agents(p: &Prog) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    fn go(p: &Prog, out: &mut Vec<(String, usize)>) {
        match p {
            Prog::Step { w: Worker::Fresh, call, at, .. } => out.push((call.clone(), *at)),
            Prog::LetAgent { x, from: None, body, at } => {
                if let Some(c) = first_call(body, x) {
                    out.push((c, *at));
                }
                go(body, out);
            }
            Prog::Bind { first, rest, .. } => {
                go(first, out);
                go(rest, out);
            }
            Prog::Match { arms, .. } => arms.iter().for_each(|(_, b)| go(b, out)),
            Prog::Par { branches, .. } | Prog::Any { branches, .. } => branches.iter().for_each(|b| go(b, out)),
            Prog::Iterate { body, .. } | Prog::For { body, .. } | Prog::Layer { body, .. } | Prog::With { body, .. } | Prog::LetAgent { body, .. } => go(body, out),
            Prog::Try { body, arms, .. } => {
                go(body, out);
                arms.iter().for_each(|h| go(&h.body, out));
            }
            Prog::During { body, whens, .. } => {
                go(body, out);
                whens.iter().for_each(|w| go(&w.body, out));
            }
            _ => {}
        }
    }
    go(p, &mut out);
    out
}

/// F on P and on every program P holds, in order: the one walk `opens` and `steps` read.
pub fn each<'a, F: FnMut(&'a Prog)>(p: &'a Prog, f: &mut F) {
    f(p);
    match p {
        Prog::Bind { first, rest, .. } => {
            each(first, f);
            each(rest, f);
        }
        Prog::Match { arms, .. } => arms.iter().for_each(|(_, b)| each(b, f)),
        Prog::Par { branches, .. } | Prog::Any { branches, .. } => branches.iter().for_each(|b| each(b, f)),
        Prog::Iterate { body, .. } | Prog::For { body, .. } | Prog::Layer { body, .. } => each(body, f),
        Prog::Try { body, arms, .. } => {
            each(body, f);
            arms.iter().for_each(|h| each(&h.body, f));
        }
        Prog::With { body, .. } | Prog::LetAgent { body, .. } => each(body, f),
        Prog::During { body, whens, .. } => {
            each(body, f);
            whens.iter().for_each(|w| each(&w.body, f));
        }
        _ => {}
    }
}

/// The kinds P opens.
pub fn opens(p: &Prog) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    each(p, &mut |q| out.extend(opened(q).cloned()));
    out
}

/// The kind Q opens, when it is an `open`: what `opens` reads of each node `each` gives it.
pub fn opened(q: &Prog) -> Option<&String> {
    if let Prog::Open { kind, .. } = q { Some(kind) } else { None }
}

/// The steps of P with a call of their own (an inline step's prompt is its text), in order: each one's worker, call
/// and line. What the Kinds answer names a kind's calls from (paper-research N-1396 p174).
pub fn steps(p: &Prog) -> Vec<(&Worker, &str, usize)> {
    let mut out = Vec::new();
    each(p, &mut |q| out.extend(step_of(q)));
    out
}

/// Q as `steps` reads it, when it is a step with a call of its own: its worker, call and line.
pub fn step_of(q: &Prog) -> Option<(&Worker, &str, usize)> {
    if let Prog::Step { w, call, at, inline: None, .. } = q { Some((w, call.as_str(), *at)) } else { None }
}

/// Whether P has an iterate with no count (`syntax::UNBOUNDED`) whose rounds `bound` counts once: its bound is per
/// round, as a `during`'s is per look. What the Kinds page says beside the number.
pub fn rounds_unbounded(p: &Prog) -> bool {
    let un = |n: &u32| *n == crate::syntax::UNBOUNDED;
    match p {
        Prog::Iterate { n, body, .. } => un(n) || rounds_unbounded(body),
        Prog::Bind { first, rest, .. } => rounds_unbounded(first) || rounds_unbounded(rest),
        Prog::Match { arms, .. } => arms.iter().any(|(_, b)| rounds_unbounded(b)),
        Prog::Par { branches, .. } | Prog::Any { branches, .. } => branches.iter().any(rounds_unbounded),
        Prog::For { body, .. } | Prog::Layer { body, .. } | Prog::With { body, .. } | Prog::LetAgent { body, .. } => rounds_unbounded(body),
        Prog::Try { body, arms, .. } => rounds_unbounded(body) || arms.iter().any(|h| rounds_unbounded(&h.body)),
        Prog::During { body, whens, .. } => rounds_unbounded(body) || whens.iter().any(|w| rounds_unbounded(&w.body)),
        _ => false,
    }
}

/// For an iterate with no count: (whether P can end as `Again` without having waited, whether every path through P
/// waits on a step, a decision or an opened ticket). A value not written as `Done(..)` may be `Again`.
pub fn again_unwaited(p: &Prog) -> (bool, bool) {
    let maybe_again = |e: &Expr| !matches!(e, Expr::Con { name, .. } if name == "Done");
    match p {
        Prog::Step { .. } | Prog::Decide { .. } | Prog::Open { .. } => (false, true),
        Prog::Return { e, .. } | Prog::Continue { e, .. } => (maybe_again(e), false),
        Prog::Hole { .. } => (true, false),
        Prog::Bind { first, rest, .. } => {
            let (_, w1) = again_unwaited(first);
            let (a2, w2) = again_unwaited(rest);
            (!w1 && a2, w1 || w2)
        }
        Prog::Match { arms, .. } => {
            let rs: Vec<_> = arms.iter().map(|(_, b)| again_unwaited(b)).collect();
            (rs.iter().any(|r| r.0), !rs.is_empty() && rs.iter().all(|r| r.1))
        }
        Prog::Par { branches, .. } => {
            let w = branches.iter().any(|b| again_unwaited(b).1);
            (!w, w)
        }
        Prog::Any { branches, .. } => {
            let w = !branches.is_empty() && branches.iter().all(|b| again_unwaited(b).1);
            (!w, w)
        }
        Prog::Iterate { body, .. } | Prog::For { body, .. } => {
            let w = again_unwaited(body).1;
            (!w, w)
        }
        Prog::Layer { body, .. } | Prog::With { body, .. } | Prog::LetAgent { body, .. } => again_unwaited(body),
        Prog::Try { body, arms, .. } => {
            let (a, w) = again_unwaited(body);
            let rs: Vec<_> = arms.iter().map(|h| again_unwaited(&h.body)).collect();
            (a || rs.iter().any(|r| r.0), w && rs.iter().all(|r| r.1))
        }
        Prog::During { body, whens, .. } => {
            let (a, w) = again_unwaited(body);
            (a || whens.iter().any(|h| again_unwaited(&h.body).0), w)
        }
        _ => (false, false),
    }
}

/// The most steps P can take (semantics.tex, the cost bound); KINDS for what an `open` costs.
pub fn bound(p: &Prog, kinds: &BTreeMap<String, Kind>) -> u64 {
    match p {
        // a hold is answered when its worker chooses, by no model step of the system's
        Prog::Step { hold, .. } => u64::from(!hold),
        Prog::Open { kind, .. } => kinds.get(kind).map_or(0, |k| bound(&k.pattern, kinds)),
        Prog::Bind { first, rest, .. } => bound(first, kinds) + bound(rest, kinds),
        Prog::Match { arms, .. } => arms.iter().map(|(_, b)| bound(b, kinds)).max().unwrap_or(0),
        Prog::Par { branches, each: Some(e), .. } | Prog::Any { branches, each: Some(e), .. } => e.n as u64 * bound(&branches[0], kinds),
        Prog::Par { branches, .. } | Prog::Any { branches, .. } => branches.iter().map(|b| bound(b, kinds)).sum(),
        // with no count, the bound of one round: how many rounds there are is how long what it waits on takes
        // (N-101.p33; the Kinds page shows it per round, `rounds_unbounded`)
        Prog::Iterate { n, body, .. } if *n == crate::syntax::UNBOUNDED => bound(body, kinds),
        Prog::Iterate { n, body, .. } | Prog::For { n, body, .. } => *n as u64 * bound(body, kinds),
        Prog::Layer { body, .. } => bound(body, kinds),
        // the body, then at most one arm (a raise ends the body)
        Prog::Try { body, arms, .. } => bound(body, kinds) + arms.iter().map(|h| bound(&h.body, kinds)).max().unwrap_or(0),
        Prog::With { body, .. } | Prog::LetAgent { body, .. } => bound(body, kinds),
        // the body, and one run of the handler: a look's steps are counted per look, and how many looks there are is
        // how many events come while the body runs, one at a time (N-101; the Kinds page shows the body's bound and
        // the handler's per look)
        // one look at a time, each by one handler: the dearest
        Prog::During { body, whens, .. } => bound(body, kinds) + whens.iter().map(|w| bound(&w.body, kinds)).max().unwrap_or(0),
        // the policy's rules, or the owner: no agent's step
        Prog::Stop { .. } | Prog::Decide { .. } | Prog::Gather { .. } => 0,
        // what `continue` runs is the minder's, bounded when it is accepted (as an edit's program is)
        Prog::Return { .. } | Prog::Raise { .. } | Prog::Post { .. } | Prog::Give { .. } | Prog::Move { .. } | Prog::Hole { .. } | Prog::Continue { .. } | Prog::Reply { .. } | Prog::Tell { .. } | Prog::Drop { .. } | Prog::Mark { .. } | Prog::Deliver { .. } | Prog::Fork { .. } | Prog::Keep { .. } | Prog::Report { .. } => 0,
    }
}

type Sigs = BTreeMap<String, (Vec<(String, Type)>, Type, Vec<String>)>;

struct Cx<'a> {
    calls: &'a BTreeMap<String, CallSig>,
    kinds: &'a Sigs,
    /// Refuse a `try` arm nothing in its body raises. Not for a program put in a running ticket (Withdraw, Edit):
    /// a printed continuation keeps its `try` when what raised in it has already run.
    dead_arms: bool,
    /// The project's declared events, for a `during`'s handler; none where a program is checked without them
    /// (a withdrawal's, a minder's), where its value is read loosely.
    events: Option<&'a crate::syntax::Events>,
    /// The declared types, by name: a declared record `type R { f: T ... }` is built and matched as `R(e, ...)`, its
    /// fields in their order (N-101, `Handed(c, top, a)`). None where a program is checked without them: then a
    /// constructor whose name is no variant's builds or matches any record of as many fields.
    named: Option<&'a crate::syntax::Named>,
    /// Where checking a kind goes on after an error, each kept (check_all.rs); none: the first error ends it.
    all: Option<&'a all::Collect>,
    /// Whether the record's facts (`Expr::Fact`) are read here: in a step's `on`, outside a `find`'s `where`, whose
    /// element the `on` does not have when it reads them (`Machine::on_answer`).
    facts: bool,
}

#[path = "check_all.rs"]
mod all;

impl Cx<'_> {
    /// What a step to worker W raises that the types track: Violated, when the project declares duties and W is an
    /// agent (not the owner, not a role, which policy fills with whoever: the runner, the policy).
    fn step_raises(&self, w: &Worker) -> Excs {
        let agent = !matches!(w, Worker::Owner | Worker::Role(_));
        if agent && DUTIES.get() { Excs::from([VIOLATED.to_string()]) } else { Excs::new() }
    }

    /// The declared type that has constructor NAME: the record NAME, or a sum with a constructor NAME.
    /// An iterate's result type where its place gives none: the one declared type that has the heads of the values
    /// its body answers `Done(..)` with (N-101.p51). Refused when no head is written there, when none is declared, or
    /// when they are in two declared types.
    fn done_type(&self, body: &Prog) -> Result<Type, String> {
        let mut heads = Vec::new();
        done_heads(body, &mut heads);
        let mut found: Vec<(&String, &Type)> = Vec::new();
        for (m, t) in self.named.into_iter().flatten() {
            if heads.iter().any(|h| matches!(t, Type::Record(_) if m == h) || t.constructor(h).is_some()) && !found.iter().any(|(_, u)| *u == t) {
                found.push((m, t));
            }
        }
        match found.as_slice() {
            [(_, t)] => Ok((*t).clone()),
            [] if heads.is_empty() => Err("an iterate's result type must be known here: bind it where it is (as a kind's last pattern, or a step's argument), or answer `Done(H(..))` with H a head of a declared type".into()),
            [] => Err(format!("an iterate's result type must be known here: no declared type has {}", heads.join(" or "))),
            more => Err(format!("an iterate's result type is not known: {} are heads of more than one declared type ({})", heads.join(", "), more.iter().map(|(m, _)| m.as_str()).collect::<Vec<_>>().join(", "))),
        }
    }

    fn named_with(&self, name: &str) -> Option<Type> {
        self.named?.iter().find_map(|(m, t)| match t {
            Type::Record(_) if m == name => Some(t.clone()),
            t if t.constructor(name).is_some() => Some(t.clone()),
            _ => None,
        })
    }

    /// The fields of constructor NAME of T: a variant's, or a declared record's when T is the record NAME names.
    fn con<'t>(&self, t: &'t Type, name: &str) -> Option<&'t Vec<(String, Type)>> {
        match t {
            Type::Record(fs) if self.named.is_none_or(|n| n.get(name) == Some(t)) => Some(fs),
            t => t.constructor(name),
        }
    }
}

/// The kind K takes its handlers from: the one it names (`kind K from B`), else `base` if KINDS has it.
pub fn base_of<V>(k: &Kind, kinds: &BTreeMap<String, V>) -> Option<String> {
    k.base.clone().or_else(|| (k.name != BASE && kinds.contains_key(BASE)).then(|| BASE.to_string()))
}

/// The kind every kind starts from.
pub const BASE: &str = "base";

/// K takes BASE's handlers for the events it has none of its own for.
pub fn inherit(k: &mut Kind, base: &Kind) {
    for h in &base.handlers {
        if !k.handlers.iter().any(|m| m.ev == h.ev) {
            k.handlers.push(h.clone());
        }
    }
}

fn check_kind(k: &mut Kind, calls: &BTreeMap<String, CallSig>, kinds: &Sigs, events: &crate::syntax::Events, named: Option<&crate::syntax::Named>, strict: bool) -> Result<(), String> {
    check_kind_in(k, calls, kinds, events, named, strict, None)
}

/// `check_kind`; with ALL, every error is kept there and checking goes on (the kind is refused if any was kept).
#[allow(clippy::too_many_arguments)]
fn check_kind_in(k: &mut Kind, calls: &BTreeMap<String, CallSig>, kinds: &Sigs, events: &crate::syntax::Events, named: Option<&crate::syntax::Named>, strict: bool, all: Option<&all::Collect>) -> Result<(), String> {
    let cx = Cx { calls, kinds, dead_arms: strict, events: Some(events), named, all, facts: false };
    let r = check_kind_with(&cx, k, events, strict);
    cx.finish(r)
}

fn check_kind_with(cx: &Cx, k: &mut Kind, events: &crate::syntax::Events, strict: bool) -> Result<(), String> {
    let delta: Delta = k.form.iter().cloned().collect();
    let (_, mut excs) = match cx.prog(&mut k.pattern, Some(&k.result), &delta) {
        Ok(r) => r,
        Err(e) => {
            cx.keep(e)?;
            (None, Excs::new())
        }
    };
    // a handler runs in the ticket: its form in scope, `this` the ticket, X the event's value. A hole's handler stands
    // in the hole's place, so it gives the kind's result; an ask's runs beside the program, its value unused
    for h in k.handlers.iter_mut() {
        match handler(cx, k.name.as_str(), &k.result, h, &delta, events) {
            Ok(x) => excs.extend(x),
            Err(e) => cx.keep(e)?,
        }
    }
    let extra: Vec<&String> = excs.iter().filter(|e| strict && !k.raises.contains(e) && !unlisted(e)).collect();
    if !extra.is_empty() {
        return Err(format!(
            "its pattern can raise {} which `raises` does not list",
            extra.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }
    Ok(())
}

/// Check handler H of kind KNAME (result RESULT, form DELTA): the exceptions it may raise.
fn handler(cx: &Cx, kname: &str, result: &Type, h: &mut crate::syntax::When, delta: &Delta, events: &crate::syntax::Events) -> Result<Excs, String> {
    let mut d = delta.clone();
    d.insert("this".into(), Type::Ticket);
    let at = format!("its handler `when {}` (line {}{})", h.ev, h.line, if h.from == kname { String::new() } else { format!(", from {}", h.from) });
    let ty = crate::syntax::event_type(&h.ev, events).ok_or_else(|| format!("{at}: there is no event {}: a handler is on a ticket's event (hole, ask, owner_post, agent_post) or one the project declares (`event {} {{ ... }}`)", h.ev, h.ev))?;
    d.insert(h.x.clone(), ty);
    // an act's event, or the git round's, records no handler's run: it answers a step, by the step's `on`
    if crate::syntax::step_only(&h.ev) {
        return Err(format!("{at}: {} answers steps and runs no handler; a step takes it with `on {}(x) … => answer`", h.ev, h.ev));
    }
    // his ask taken opens tickets by a binding; no ticket's handler takes it
    if h.ev == crate::machine::ASKED {
        return Err(format!("{at}: {} opens tickets by a binding (`on {}(e) open …`) and runs no handler", h.ev, h.ev));
    }
    // a project's event costs nothing to send (a program sends it), so its handler may post and tell but makes
    // no step, nor opens a ticket that would (plan/layers.md I §3, the cost property)
    if crate::syntax::ticket_event(&h.ev).is_none() {
        if has_step(&h.body) {
            return Err(format!("{at}: a handler of a project's event makes no step; what needs a step is the step's answer, which the pattern matches"));
        }
        if !opens(&h.body).is_empty() {
            return Err(format!("{at}: a handler of a project's event opens no ticket (its steps would be paid for by an event); it may post and tell"));
        }
    }
    if let Some(c) = h.cond.as_mut() {
        let t = cx.infer(c, &d).map_err(|e| format!("{at}: {e}"))?;
        if !t.fits(&Type::Bool) {
            return Err(format!("{at}: `where` takes a Bool, not a {t}"));
        }
    }
    let want = (h.ev == "hole").then_some(result);
    let (_, x) = cx.prog(&mut h.body, want, &d).map_err(|e| format!("{at}: {e}"))?;
    Ok(x)
}

/// Check a whole program P (a ticket's, from its log line) against its form and result, with today's calls and
/// kinds, filling in constructors' fields. STRICT also holds it to its `raises`: a program read back from the log
/// (replay) is not, since the kinds it opens may raise more today than when it was opened, and a finished ticket's
/// record must not break when a kind changes.
#[allow(clippy::too_many_arguments)]
pub fn check_program(p: &mut Prog, form: &[(String, Type)], result: &Type, raises: &[String], calls: &BTreeMap<String, CallSig>, kinds: &BTreeMap<String, Kind>, named: Option<&crate::syntax::Named>, strict: bool) -> Result<(), String> {
    let sigs: Sigs = kinds.iter().map(|(n, k)| (n.clone(), (k.form.clone(), k.result.clone(), k.raises.clone()))).collect();
    let mut k = Kind { name: String::new(), file: String::new(), line: 0, letter: None, width: 0, form: form.to_vec(), result: result.clone(), raises: raises.to_vec(), goals: Vec::new(), pattern: p.clone(), text: String::new(), base: None, handlers: Vec::new(), doc: Default::default(), drawn: None };
    check_kind(&mut k, calls, &sigs, &crate::syntax::Events::new(), named, strict)?;
    *p = k.pattern;
    Ok(())
}

/// Check kind K whole (its pattern and handlers), as read back from a ticket's recorded program; STRICT as above.
pub fn check_program_kind(k: &mut Kind, calls: &BTreeMap<String, CallSig>, kinds: &BTreeMap<String, Kind>, events: &crate::syntax::Events, named: Option<&crate::syntax::Named>, strict: bool) -> Result<(), String> {
    let sigs: Sigs = kinds.iter().map(|(n, k)| (n.clone(), (k.form.clone(), k.result.clone(), k.raises.clone()))).collect();
    check_kind(k, calls, &sigs, events, named, strict)
}

/// Check P, a withdrawal's program, where a step stood: in its SCOPE, against the step's HEAD; what it may raise
/// beyond RAISES (the ticket's kind's) and the untracked is refused when STRICT.
pub fn check_in(p: &mut Prog, head: &Type, scope: &BTreeMap<String, Type>, raises: &[String], calls: &BTreeMap<String, CallSig>, kinds: &BTreeMap<String, Kind>, strict: bool) -> Result<(), String> {
    check_in_with(p, head, scope, raises, calls, kinds, None, strict)
}

/// `check_in` with the declared types NAMED (a record built or matched by its name, `result of` matched).
#[allow(clippy::too_many_arguments)]
pub fn check_in_with(p: &mut Prog, head: &Type, scope: &BTreeMap<String, Type>, raises: &[String], calls: &BTreeMap<String, CallSig>, kinds: &BTreeMap<String, Kind>, named: Option<&crate::syntax::Named>, strict: bool) -> Result<(), String> {
    let sigs: Sigs = kinds.iter().map(|(n, k)| (n.clone(), (k.form.clone(), k.result.clone(), k.raises.clone()))).collect();
    let cx = Cx { calls, kinds: &sigs, dead_arms: false, events: None, named, all: None, facts: false };
    let (_, excs) = cx.prog(p, Some(head), scope)?;
    let extra: Vec<&String> = excs.iter().filter(|e| strict && !raises.contains(e) && !unlisted(e)).collect();
    if !extra.is_empty() {
        return Err(format!("it can raise {}, which the ticket's kind does not list", extra.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
    }
    Ok(())
}

/// The minder's program TEXT for a hole of a KIND ticket whose scope there is SCOPE: parsed, its own holes made
/// eager, and checked against the kind's result as an Edit at the hole is. Pure: coordd calls it off its lock to
/// retry the shape call once with the reason (docket-edge m1170); the machine calls it when it accepts the answer.
pub fn check_shape(kind: &Kind, scope: &BTreeMap<String, Type>, text: &str, calls: &BTreeMap<String, CallSig>, kinds: &BTreeMap<String, Kind>) -> Result<Prog, String> {
    let mut p = crate::syntax::parse_minder_prog(text)?;
    check_in(&mut p, &kind.result, scope, &kind.raises, calls, kinds, true)?;
    Ok(p)
}

/// Check binding B against KINDS: the kind it opens exists and its arguments fit that kind's form, the event's
/// value being of any type; a `when` binding's may read `stands` (`syntax::STANDS`), a field for each call.
pub fn check_binding(b: &mut crate::syntax::Binding, calls: &BTreeMap<String, CallSig>, kinds: &BTreeMap<String, Kind>, events: &crate::syntax::Events) -> Result<(), String> {
    let k = kinds.get(&b.kind).ok_or_else(|| format!("there is no kind {} (or it was refused)", b.kind))?;
    let sigs: Sigs = kinds.iter().map(|(n, k)| (n.clone(), (k.form.clone(), k.result.clone(), k.raises.clone()))).collect();
    let cx = Cx { calls, kinds: &sigs, dead_arms: true, events: Some(events), named: None, all: None, facts: false };
    // a declared event's value has its fields' types; an undeclared one (today's job_fault, check_end) is read loosely
    let mut tx = events.get(&b.ev).map(|fs| Type::Record(fs.clone())).or_else(|| crate::syntax::system_event(&b.ev)).unwrap_or(Type::Any);
    // `every x.f of`: it fires only on an event whose F gives its N (`Machine::count_times`), so where it fires F is
    // there, and an optional F is read as its value (`every e.every of turn_end(e) … Ahead(e.every)`)
    if let (crate::syntax::Every::Count(crate::syntax::Times::Field { f, .. }), Type::Record(fs)) = (&b.every, &mut tx) {
        for (_, t) in fs.iter_mut().filter(|(n, _)| n == f) {
            if let Type::Opt(inner) = t {
                *t = (**inner).clone();
            }
        }
    }
    // an act's event, or the git round's: its line records no ticket a binding would open (`Line::Event`'s `fired`)
    if crate::syntax::step_only(&b.ev) {
        return Err(format!("{} answers steps and opens no ticket; a step takes it with `on {}(x) … => answer`", b.ev, b.ev));
    }
    let mut d: Delta = [(b.x.clone(), tx)].into_iter().filter(|(x, _)| !x.is_empty()).collect();
    // a decision's binding reads where the open tickets stand as it fires (`syntax::STANDS`): each call a field
    if matches!(b.every, crate::syntax::Every::When { .. }) {
        let at = calls.keys().filter(|c| crate::syntax::is_ident(c)).map(|c| (c.clone(), Type::List(Box::new(Type::Ticket)))).collect();
        d.insert(crate::syntax::STANDS.to_string(), Type::Record(at));
    }
    for (l, r) in b.conds.iter_mut() {
        let t = cx.infer(l, &d)?;
        match r {
            Some((_, r)) => {
                cx.infer(r, &d)?;
            }
            None if !t.fits(&Type::Bool) => return Err(format!("`where` takes a Bool, or `a == b`, `a != b`; this is a {t}")),
            None => {}
        }
    }
    cx.args(&mut b.args, &k.form, &d, &format!("a {}", b.kind))
}

/// The heads of the values P answers `Done(H(..))` with, for the iterate whose body P is (not those of an iterate
/// inside it).
fn done_heads(p: &Prog, out: &mut Vec<String>) {
    match p {
        Prog::Return { e: Expr::Con { name, args, .. }, .. } if name == "Done" => {
            if let Some(Expr::Con { name: h, .. }) = args.first() {
                if !out.contains(h) {
                    out.push(h.clone());
                }
            }
        }
        Prog::Bind { rest, .. } => done_heads(rest, out),
        Prog::Match { arms, .. } => arms.iter().for_each(|(_, b)| done_heads(b, out)),
        Prog::Layer { body, .. } | Prog::With { body, .. } | Prog::LetAgent { body, .. } => done_heads(body, out),
        Prog::Try { body, arms, .. } => {
            done_heads(body, out);
            arms.iter().for_each(|h| done_heads(&h.body, out));
        }
        Prog::During { body, whens, .. } => {
            done_heads(body, out);
            whens.iter().for_each(|w| done_heads(&w.body, out));
        }
        _ => {}
    }
}

fn the_iterate_type(value: Type, done: Type) -> Type {
    Type::Variant(vec![
        ("Again".into(), vec![("value".into(), value), ("measure".into(), Type::Opt(Box::new(Type::Int)))]),
        ("Done".into(), vec![("value".into(), done)]),
    ])
}

impl Cx<'_> {
    /// `Δ ⊢ p : T ! Ξ`; the type is None when P never returns (it raises on every path).
    fn prog(&self, p: &mut Prog, want: Option<&Type>, d: &Delta) -> Result<(Option<Type>, Excs), String> {
        let at = p.at();
        let here = |e: String| format!("line {at}: {e}");
        match p {
            Prog::Return { e, .. } => match want {
                Some(t) => {
                    self.check(e, t, d).map_err(here)?;
                    Ok((Some(t.clone()), Excs::new()))
                }
                None => Ok((Some(self.infer(e, d).map_err(here)?), Excs::new())),
            },
            Prog::Raise { exc, e, .. } => {
                if let Some(e) = e {
                    self.check(e, &Type::Any, d).map_err(here)?;
                }
                Ok((None, Excs::from([exc.clone()])))
            }
            Prog::Step { w, scope, inline: Some((text, head)), on, .. } => {
                *scope = d.clone();
                for (name, _, _) in crate::syntax::placeholders(text) {
                    if !d.contains_key(&name) {
                        let vars = d.keys().filter(|n| n.as_str() != "_").cloned().collect::<Vec<_>>().join(", ");
                        return Err(here(format!("the step's text names `{{{{ {name} }}}}`, which is not in scope (in scope: {vars})")));
                    }
                }
                unwritable_place(text, d, "the step's text").map_err(here)?;
                self.worker(w, d).map_err(here)?;
                self.fits(head, want).map_err(here)?;
                self.ons(on, head, d).map_err(here)?;
                Ok((Some(head.clone()), self.step_raises(w)))
            }
            Prog::Step { w, call, args, scope, on, .. } => {
                *scope = d.clone();
                let c = self.calls.get(call.as_str()).ok_or_else(|| here(format!("no call `{call}`")))?;
                self.args(args, &c.params, d, &format!("call {call}")).map_err(here)?;
                self.worker(w, d).map_err(here)?;
                // an answer taken by itself (`default C`, N-487) is for the owner's steps only: an agent's silence is
                // `Silent`, never an answer
                if let (Some(dflt), false) = (&c.default, matches!(w, Worker::Owner)) {
                    return Err(here(format!("call `{call}` names `default {dflt}`, an answer the owner's step takes by itself; only `step owner {call}(...)` may step it")));
                }
                // a call's question (`asks C`) is never taken: its program sees the other answers only
                let head = c.answers();
                self.fits(&head, want).map_err(here)?;
                self.ons(on, &head, d).map_err(here)?;
                Ok((Some(head), self.step_raises(w)))
            }
            Prog::Open { kind, args, wait, .. } => {
                let (form, result, raises) = self.kinds.get(kind.as_str()).ok_or_else(|| here(format!("no kind `{kind}`")))?;
                self.args(args, form, d, &format!("kind {kind}")).map_err(here)?;
                *wait = waits_at(d, result);
                self.fits(result, want).map_err(here)?;
                Ok((Some(result.clone()), raises.iter().cloned().collect()))
            }
            Prog::Post { e, on, by, naming, informing, scope, .. } => {
                *scope = d.clone();
                self.check(e, &Type::Text, d).map_err(here)?;
                // a post written as text fills its `{{ name }}` places from the scope, as an inline step does (N-101.p42)
                if let Expr::Str(text) = &*e {
                    for (name, _, _) in crate::syntax::placeholders(text) {
                        if !d.contains_key(&name) {
                            let vars = d.keys().filter(|n| n.as_str() != "_").cloned().collect::<Vec<_>>().join(", ");
                            return Err(here(format!("the post's text names `{{{{ {name} }}}}`, which is not in scope (in scope: {vars})")));
                        }
                    }
                    unwritable_place(text, d, "the post's text").map_err(here)?;
                }
                if let Some(t) = on {
                    self.check(t, &Type::Ticket, d).map_err(here)?;
                }
                if let Some(w) = by.as_deref().filter(|w| *w != "owner") {
                    self.agent_name(w, d).map_err(here)?;
                }
                for (word, ns) in [("naming", naming), ("informing", informing)] {
                    for n in ns.iter_mut() {
                        let t = self.infer(n, d).map_err(here)?;
                        if !(t.fits(&Type::Agent) || t.fits(&Type::List(Box::new(Type::Agent)))) {
                            return Err(here(format!("`{word}` takes agents or lists of agents, not a {t}")));
                        }
                    }
                }
                Ok((Some(Type::unit()), Excs::new()))
            }
            Prog::Give { w, e, .. } => {
                self.agent_name(w, d).map_err(here)?;
                self.check(e, &Type::Text, d).map_err(here)?;
                Ok((Some(Type::unit()), Excs::new()))
            }
            Prog::Move { n, to, .. } => {
                self.check(n, &Type::Int, d).map_err(here)?;
                self.check(to, &Type::Ticket, d).map_err(here)?;
                Ok((Some(Type::unit()), Excs::new()))
            }
            // the minder's answer carried out: what it runs is checked then, in the hole's scope (`check_shape`)
            Prog::Continue { e, .. } => {
                self.infer(e, d).map_err(here)?;
                Ok((want.cloned(), Excs::new()))
            }
            Prog::Reply { q, a, .. } => {
                let ask = crate::syntax::ticket_event("ask").unwrap_or(Type::Any);
                self.check(q, &ask, d).map_err(here)?;
                self.infer(a, d).map_err(here)?;
                Ok((Some(Type::unit()), Excs::new()))
            }
            Prog::Tell { agents, p, joins, .. } => {
                self.check(agents, &Type::List(Box::new(Type::Agent)), d).map_err(here)?;
                // the post held (the event's value), or some of his posts it holds by number (a Split's part, E1b)
                let some = Type::List(Box::new(Type::Int));
                if !self.infer(p, d).is_ok_and(|t| t == some) {
                    self.check(p, &crate::syntax::ticket_event("owner_post").unwrap_or(Type::Any), d).map_err(here)?;
                }
                if let Some(j) = joins {
                    self.check(j, &Type::Opt(Box::new(Type::Int)), d).map_err(here)?;
                }
                Ok((Some(Type::unit()), Excs::new()))
            }
            Prog::Drop { p, .. } => {
                self.check(p, &crate::syntax::ticket_event("owner_post").unwrap_or(Type::Any), d).map_err(here)?;
                Ok((Some(Type::unit()), Excs::new()))
            }
            // his burst: the event's value in (base.kind's `when owner_post`), or a thread's opening `text` (thread.kind),
            // the same over every post of it out; it waits at the owner's step of `gather`, which goes on by itself, so
            // the call must be there and name its `default`
            Prog::Gather { e, .. } => {
                let ev = crate::syntax::ticket_event("owner_post").unwrap_or(Type::Any);
                let out = if self.check(e, &Type::Text, d).is_ok() {
                    Type::Text
                } else {
                    self.check(e, &ev, d).map_err(here)?;
                    ev
                };
                let g = crate::syntax::GATHER;
                let c = self.calls.get(g).ok_or_else(|| here(format!("`gather` waits at the owner's step of the call `{g}` (agent-base calls/gather.call), and there is no call `{g}`")))?;
                if c.default.is_none() {
                    return Err(here(format!("the call `{g}` names no `default`: a held post would wait for his press alone")));
                }
                self.fits(&out, want).map_err(here)?;
                Ok((Some(out), Excs::new()))
            }
            Prog::Mark { p, marks, .. } => {
                self.check(p, &crate::syntax::ticket_event("agent_post").unwrap_or(Type::Any), d).map_err(here)?;
                let t = self.infer(marks, d).map_err(here)?;
                if !t.fits(&Type::List(Box::new(Type::Any))) {
                    return Err(here(format!("`mark` takes a list of marks, not a {t}")));
                }
                Ok((Some(Type::unit()), Excs::new()))
            }
            // the delivery acts (0.2): an agent, and items (a list of them, or one) given as the delivery events' values
            // carry them or by their ids; what they are is read when the act runs, against what waits then
            Prog::Deliver { a, items, rest, digest, from, .. } => {
                self.check(a, &Type::Agent, d).map_err(here)?;
                // a fork's place: a delivery's id, as `free`'s `place` gives it
                if let Some(f) = from {
                    self.check(f, &Type::Text, d).map_err(here)?;
                }
                for e in std::iter::once(items).chain(rest.as_mut()).chain(digest.as_mut()) {
                    self.infer(e, d).map_err(here)?;
                }
                Ok((Some(Type::unit()), Excs::new()))
            }
            // a copy of the agent, cut at a delivery's place (`arrived`'s `began`): the copy's name
            Prog::Fork { a, place, .. } => {
                self.check(a, &Type::Agent, d).map_err(here)?;
                self.check(place, &Type::Text, d).map_err(here)?;
                Ok((Some(Type::Agent), Excs::new()))
            }
            Prog::Keep { items, why, .. } | Prog::Report { items, why, .. } => {
                self.infer(items, d).map_err(here)?;
                self.check(why, &Type::Text, d).map_err(here)?;
                Ok((Some(Type::unit()), Excs::new()))
            }
            // the hole gives nothing now: what fills it is checked then, in this scope, against the kind's result
            Prog::Hole { scope, .. } => {
                // what a program at the hole may name (`_` names nothing)
                *scope = d.iter().filter(|(n, _)| n.as_str() != "_").map(|(n, t)| (n.clone(), t.clone())).collect();
                Ok((None, Excs::new()))
            }
            Prog::Bind { x, first, rest } => {
                let (t1, mut x1) = match self.prog(first, None, d) {
                    Ok(r) => r,
                    // checking all: what a failed statement binds is unknown from here on, and kept as such
                    Err(e) => {
                        self.keep(e)?;
                        self.poison(x);
                        (Some(Type::Any), Excs::new())
                    }
                };
                let t1 = t1.ok_or_else(|| here("nothing after this runs: it raises on every path".into()))?;
                let mut d2 = d.clone();
                match x {
                    Bound::Var(v) => {
                        d2.insert(v.clone(), t1);
                    }
                    Bound::Tuple(vs) => match &t1 {
                        Type::Any if self.is_poisoned(vs) => {
                            for v in vs.iter() {
                                d2.insert(v.clone(), Type::Any);
                            }
                        }
                        Type::Tuple(ts) if ts.len() == vs.len() => {
                            for (v, t) in vs.iter().zip(ts) {
                                d2.insert(v.clone(), t.clone());
                            }
                        }
                        t => return Err(here(format!("{} names for a {t}", vs.len()))),
                    },
                }
                let (t2, x2) = self.prog(rest, want, &d2)?;
                x1.extend(x2);
                Ok((t2, x1))
            }
            Prog::Match { e, arms, guards, .. } => {
                guards.resize(arms.len(), None);
                // checking all: a value unknown since an error above is matched loosely, its arms still checked
                if self.unknown(e, d) {
                    return self.loose_match(arms, want, d);
                }
                let mut te = self.infer(e, d).map_err(here)?;
                // a value of unknown type (`result of e`) matched by a constructor: of the declared type that has it
                if te == Type::Any {
                    if let Some(t) = arms.iter().find_map(|(p, _)| if let Pat::Con { name, .. } = p { self.named_with(name) } else { None }) {
                        te = t;
                    }
                }
                if arms.is_empty() {
                    return Err(here("a match with no arms".into()));
                }
                let mut result: Option<Type> = want.cloned();
                let mut excs = Excs::new();
                let mut rows: Vec<Vec<Pat>> = Vec::new();
                let mut later: Vec<(usize, Delta)> = Vec::new();
                for (i, (pat, body)) in arms.iter_mut().enumerate() {
                    // a recorded program read again (replay, an edit of a running ticket): an arm for a constructor
                    // the type no longer has is never taken, and is let be (a thread's OneOff, design §16, 3)
                    if !self.dead_arms && matches!(&*pat, Pat::Con { name, .. } if matches!(&te, Type::Variant(_)) && te.constructor(name).is_none()) {
                        continue;
                    }
                    let mut d2 = d.clone();
                    // after a `none` arm the value is present: a later arm's variable is bound to the value itself
                    let present = match &te {
                        Type::Opt(e) if !matches!(pat, Pat::None) && rows.iter().any(|r| matches!(r[0], Pat::None)) => e.as_ref(),
                        t => t,
                    };
                    if let Err(e) = self.pat(pat, present, &mut d2).map_err(here) {
                        self.keep(e)?;
                        continue;
                    }
                    if !useful(&rows, &[pat.clone()], std::slice::from_ref(&te)) {
                        self.keep(format!("line {}: this arm can never be taken: the arms before it cover it", body.at()))?;
                        continue;
                    }
                    // an arm with a `where` covers nothing: a later arm takes what its condition lets go
                    match guards[i].as_mut() {
                        Some(c) => match self.infer(c, &d2) {
                            Ok(ct) if ct.fits(&Type::Bool) => {}
                            Ok(ct) => {
                                self.keep(here(format!("`where` takes a Bool, not a {ct}")))?;
                                continue;
                            }
                            Err(e) => {
                                self.keep(here(e))?;
                                continue;
                            }
                        },
                        None => rows.push(vec![pat.clone()]),
                    }
                    // an arm whose type is not known yet (`return Thread` before the arm with the step that gives
                    // its type) is checked again once a later arm has given it
                    // (tried with nothing kept: what fails here for want of the type is checked again below)
                    let tried = if result.is_none() { self.quietly(|| self.prog(body, None, &d2)) } else { self.prog(body, result.as_ref(), &d2) };
                    let (t, x) = match tried {
                        Ok(r) => r,
                        Err(_) if result.is_none() => {
                            later.push((i, d2));
                            continue;
                        }
                        Err(e) => {
                            self.keep(e)?;
                            continue;
                        }
                    };
                    excs.extend(x);
                    if result.is_none() {
                        result = t;
                    }
                }
                for (i, d2) in later {
                    let (t, x) = match self.prog(&mut arms[i].1, result.as_ref(), &d2) {
                        Ok(r) => r,
                        Err(e) => {
                            self.keep(e)?;
                            continue;
                        }
                    };
                    excs.extend(x);
                    if result.is_none() {
                        result = t;
                    }
                }
                if useful(&rows, &[Pat::Wild], std::slice::from_ref(&te)) {
                    self.keep(here(format!("the match does not cover every {te}")))?;
                }
                Ok((result, excs))
            }
            Prog::Par { branches, tys, each: Some(each), wait, .. } | Prog::Any { branches, tys, each: Some(each), wait, .. } => {
                // one branch per element of a list: the branch typed with X the element; the whole is [T] (a list longer
                // than the most, or for an `any` shorter than its N, raises Exhausted)
                let elem = match self.infer(&mut each.e, d).map_err(here)? {
                    Type::List(t) => *t,
                    t => return Err(here(format!("the branches are made from a list, and this is {t}"))),
                };
                let mut d2 = d.clone();
                d2.insert(each.x.clone(), elem);
                // parts by dependency (N-101): the element's name, the names it stands on, the state they reach
                if let Some(o) = each.order.as_mut() {
                    self.check(&mut o.key, &Type::Text, &d2).map_err(|e| here(format!("the name after `as`: {e}")))?;
                    self.check(&mut o.after, &Type::List(Box::new(Type::Text)), &d2).map_err(|e| here(format!("the names after `after`: {e}")))?;
                    if let Some(k) = &o.reach {
                        if !self.kinds.contains_key(k) {
                            return Err(here(format!("no kind {k} for `reach`")));
                        }
                    }
                }
                let want_elem = match want {
                    Some(Type::List(t)) => Some((**t).clone()),
                    _ => None,
                };
                let (t, mut excs) = self.prog(&mut branches[0], want_elem.as_ref(), &d2)?;
                let t = t.ok_or_else(|| here("a branch that always raises".into()))?;
                *tys = vec![t.clone()];
                let t = Type::List(Box::new(t));
                *wait = waits_at(d, &t);
                self.fits(&t, want).map_err(here)?;
                excs.insert("Exhausted".into());
                if each.order.is_some() {
                    excs.insert("NoOrder".into());
                }
                Ok((Some(t), excs))
            }
            Prog::Par { branches, tys, wait, .. } => {
                let mut ts = Vec::new();
                let mut excs = Excs::new();
                for b in branches.iter_mut() {
                    let (t, x) = self.prog(b, None, d)?;
                    ts.push(t.ok_or_else(|| here("a branch of par that always raises".into()))?);
                    excs.extend(x);
                }
                *tys = ts.clone();
                let t = Type::Tuple(ts);
                *wait = waits_at(d, &t);
                self.fits(&t, want).map_err(here)?;
                Ok((Some(t), excs))
            }
            Prog::Any { branches, tys, wait, .. } => {
                // every branch of one type T; the whole is [T]
                let mut elem = match want {
                    Some(Type::List(t)) => Some((**t).clone()),
                    _ => None,
                };
                let mut excs = Excs::new();
                for b in branches.iter_mut() {
                    let (t, x) = self.prog(b, elem.as_ref(), d)?;
                    let t = t.ok_or_else(|| here("a branch of any that always raises".into()))?;
                    if elem.is_none() {
                        elem = Some(t);
                    }
                    excs.extend(x);
                }
                *tys = vec![elem.clone().unwrap(); branches.len()];
                let t = Type::List(Box::new(elem.unwrap()));
                *wait = waits_at(d, &t);
                self.fits(&t, want).map_err(here)?;
                Ok((Some(t), excs))
            }
            Prog::Layer { on, body, .. } => {
                // on a commit or branch; its value is its body's, as with `let agent` and `with`
                if let Some(e) = on {
                    let t = self.infer(e, d).map_err(here)?;
                    if !matches!(t, Type::Ref | Type::Text) {
                        return Err(here(format!("a layer is on a commit or a branch (a Ref), and this is {t}")));
                    }
                }
                self.prog(body, want, d)
            }
            Prog::For { x, e, body, .. } => {
                // one round per element, in order; the whole is [T] (longer than the most raises Exhausted)
                let elem = match self.infer(e, d).map_err(here)? {
                    Type::List(t) => *t,
                    t => return Err(here(format!("`for` goes over a list, and this is {t}"))),
                };
                let mut d2 = d.clone();
                d2.insert(x.clone(), elem);
                let want_elem = match want {
                    Some(Type::List(t)) => Some((**t).clone()),
                    _ => None,
                };
                let (t, mut excs) = self.prog(body, want_elem.as_ref(), &d2)?;
                let t = Type::List(Box::new(t.ok_or_else(|| here("a `for` whose round always raises".into()))?));
                self.fits(&t, want).map_err(here)?;
                excs.insert("Exhausted".into());
                Ok((Some(t), excs))
            }
            Prog::Iterate { x, init, body, n, .. } => {
                // with no count, each round waits on a step: nothing goes round on its own
                if *n == crate::syntax::UNBOUNDED && again_unwaited(body).0 {
                    return Err(here("an iterate with no bound must wait on a step in each round: a path through its body goes Again before any step, decision or opened ticket".into()));
                }
                let unbounded = *n == crate::syntax::UNBOUNDED;
                let tx = self.infer(init, d).map_err(here)?;
                // where its place does not say (`w <- iterate ...`): the declared type with the heads it answers
                // `Done(..)` with, as a match takes a value of unknown type (N-101.p51)
                let done = match want {
                    Some(t) => t.clone(),
                    None => self.done_type(body).map_err(here)?,
                };
                let mut d2 = d.clone();
                d2.insert(x.clone(), tx.clone());
                let (_, mut excs) = self.prog(body, Some(&the_iterate_type(tx, done.clone())), &d2)?;
                if !unbounded {
                    excs.insert("Exhausted".into());
                }
                excs.insert("Stalled".into());
                Ok((Some(done), excs))
            }
            Prog::Try { body, arms, .. } => {
                let (tb, body_excs) = self.prog(body, want, d)?;
                let mut result = want.cloned().or(tb);
                let mut excs: Excs = body_excs.iter().filter(|e| !arms.iter().any(|h| &h.exc == *e)).cloned().collect();
                let mut seen = BTreeSet::new();
                for h in arms.iter_mut() {
                    let at = h.body.at();
                    if !seen.insert(h.exc.clone()) {
                        return Err(format!("line {at}: {} is handled twice", h.exc));
                    }
                    if self.dead_arms && !body_excs.contains(&h.exc) && !UNTRACKED.contains(&h.exc.as_str()) {
                        return Err(format!("line {at}: this arm can never be taken: nothing in the try raises {}", h.exc));
                    }
                    let mut d2 = d.clone();
                    if let Some(x) = &h.x {
                        d2.insert(x.clone(), Type::Any);
                    }
                    let (t, x) = self.prog(&mut h.body, result.as_ref(), &d2)?;
                    excs.extend(x);
                    if result.is_none() {
                        result = t;
                    }
                }
                Ok((result, excs))
            }
            Prog::With { e, body, .. } => {
                self.infer(e, d).map_err(here)?;
                self.prog(body, want, d)
            }
            Prog::LetAgent { x, from, body, .. } => {
                if let Some(a) = from {
                    self.fork_of(a, d).map_err(here)?;
                }
                let mut d2 = d.clone();
                d2.insert(x.clone(), Type::Agent);
                self.prog(body, want, &d2)
            }
            // the body's value is the whole's; the handler runs beside it, once per event while the body runs, its
            // value unused (as an ask's handler), with what is in scope where `during` stands, `this` and X
            Prog::During { body, whens, scope, .. } => {
                let (t, mut excs) = self.prog(body, want, d)?;
                *scope = d.clone();
                for when in whens.iter_mut() {
                    let line = when.line;
                    let here = |e: String| format!("line {line}: {e}");
                    let ty = match self.events {
                        Some(evs) => crate::syntax::event_type(&when.ev, evs).ok_or_else(|| {
                            here(format!("`during … when {}`: there is no event {}: it is a ticket's event (hole, ask, owner_post, agent_post, act) or one the project declares", when.ev, when.ev))
                        })?,
                        None => crate::syntax::ticket_event(&when.ev).unwrap_or(Type::Any),
                    };
                    if when.ev == "hole" {
                        return Err(here("`during … when hole`: a hole is the kind's to answer (its own `when hole`), not a `during`'s".into()));
                    }
                    if crate::syntax::ticket_event(&when.ev).is_none() && (has_step(&when.body) || !opens(&when.body).is_empty()) {
                        return Err(here(format!("`during … when {}`: a handler of a project's event makes no step and opens no ticket (the cost property); it may post, tell and stop", when.ev)));
                    }
                    let mut d2 = d.clone();
                    d2.insert("this".into(), Type::Ticket);
                    d2.insert(when.x.clone(), ty);
                    if let Some(c) = when.cond.as_mut() {
                        let ct = self.infer(c, &d2).map_err(here)?;
                        if !ct.fits(&Type::Bool) {
                            return Err(here(format!("`where` takes a Bool, not a {ct}")));
                        }
                    }
                    let (_, x) = self.prog(&mut when.body, Some(&Type::Any), &d2)?;
                    excs.extend(x);
                }
                Ok((t, excs))
            }
            // ends the agent's turn as the owner's esc does; what it owes stays owed
            Prog::Stop { e, .. } => {
                self.check(e, &Type::Agent, d).map_err(here)?;
                Ok((Some(Type::unit()), Excs::new()))
            }
            // the decision's answer, of the type it declares
            Prog::Decide { name, args, .. } => {
                let sig = self.calls.get(&format!("{}{name}", crate::syntax::DECIDE)).ok_or_else(|| here(format!("there is no decision {name}: a call file declares it, `decision {name}(params) -> Answers`")))?;
                self.args(args, &sig.params, d, &format!("the decision {name}")).map_err(here)?;
                self.fits(&sig.head, want).map_err(here)?;
                Ok((Some(sig.head.clone()), Excs::new()))
            }
        }
    }

    /// W as a worker named in a program: a variable bound to an agent, or an agent's name.
    fn agent_name(&self, w: &str, d: &Delta) -> Result<(), String> {
        match d.get(w) {
            Some(t) if !t.fits(&Type::Agent) => Err(format!("`{w}` is a {t}, not an agent")),
            _ => Ok(()),
        }
    }

    /// A step's `on`s and `until`s (`syntax::On`): each on a system event, its `where` a Bool in the step's scope D
    /// with `this` the ticket and X the event's value, and an `on`'s answer one of HEAD, the step's. An `until`'s `where` reads HEAD's fields
    /// too, and keeps the names of those it reads (`On::fields`).
    fn ons(&self, ons: &mut [crate::syntax::On], head: &Type, d: &Delta) -> Result<(), String> {
        let cx = Cx { facts: true, ..*self };
        for o in ons {
            let word = if o.answer.is_some() { "on" } else { "until" };
            let at = format!("its `{word} {}` (line {})", o.ev, o.line);
            let ty = crate::syntax::system_event(&o.ev).ok_or_else(|| {
                let evs: Vec<&str> = crate::syntax::ACT_EVENTS.iter().map(|(e, _, _)| *e).chain([crate::machine::MERGED, crate::machine::WINDOW_RELEASE, crate::machine::DUTIES]).collect();
                format!("{at}: `{word}` takes a system event ({}), and {} is none", evs.join(", "), o.ev)
            })?;
            let mut d = d.clone();
            // THIS is the ticket, as in a handler (a goal's `on duties(e) where e.goal == this`)
            d.insert("this".into(), Type::Ticket);
            d.insert(o.x.clone(), ty);
            // an `until`'s `where` reads the answer's fields by their names, after the step's scope and X; a field two
            // constructors type apart is read as Any
            let mut fields: BTreeMap<String, Type> = BTreeMap::new();
            if let (None, Type::Variant(cs)) = (&o.answer, head) {
                for (f, t) in cs.iter().flat_map(|(_, fs)| fs).filter(|(f, _)| !d.contains_key(f)) {
                    fields.entry(f.clone()).and_modify(|u| if *u != *t { *u = Type::Any }).or_insert_with(|| t.clone());
                }
                d.extend(fields.clone());
            }
            if let Some(c) = o.cond.as_mut() {
                let t = cx.infer(c, &d).map_err(|e| format!("{at}: {e}"))?;
                if !t.fits(&Type::Bool) {
                    return Err(format!("{at}: `where` takes a Bool, not a {t}"));
                }
                o.fields = fields.into_keys().filter(|f| c.reads_var(f)).collect();
            }
            if let Some(a) = o.answer.as_mut() {
                cx.check(a, head, &d).map_err(|e| format!("{at}: {e}"))?;
            }
        }
        Ok(())
    }

    fn fits(&self, t: &Type, want: Option<&Type>) -> Result<(), String> {
        match want {
            Some(w) if !t.fits(w) => Err(format!("this gives {t}, where {w} is wanted")),
            _ => Ok(()),
        }
    }

    fn args(&self, args: &mut [Expr], params: &[(String, Type)], d: &Delta, what: &str) -> Result<(), String> {
        // trailing optional parameters may be left out (they are none): a call or kind can gain one without breaking
        // the programs recorded before it
        let needed = params.iter().rposition(|(_, t)| !matches!(t, Type::Opt(_))).map_or(0, |i| i + 1);
        if args.len() > params.len() || args.len() < needed {
            return Err(format!("{what} takes {} ({}), given {}", params.len(), params.iter().map(|(n, t)| format!("{n}: {t}")).collect::<Vec<_>>().join(", "), args.len()));
        }
        for (a, (n, t)) in args.iter_mut().zip(params) {
            self.check(a, t, d).map_err(|e| format!("{what}, {n}: {e}"))?;
        }
        Ok(())
    }

    /// Check E against T, filling in constructors' field names.
    fn check(&self, e: &mut Expr, t: &Type, d: &Delta) -> Result<(), String> {
        match (e, t) {
            (Expr::Con { name, args, fields }, _) => {
                let t = match t {
                    Type::Opt(inner) => inner.as_ref(),
                    t => t,
                };
                if let Type::Any = t {
                    if !args.is_empty() {
                        return Err(format!("{name}(...) where its type is not known"));
                    }
                    return Ok(());
                }
                let fs = self.con(t, name).ok_or_else(|| format!("{name} is not a constructor of {t}"))?;
                if args.len() > fs.len() || fs[args.len()..].iter().any(|(_, ft)| !matches!(ft, Type::Opt(_))) {
                    return Err(format!("{name} takes {}, given {}", fs.len(), args.len()));
                }
                for (a, (_, ft)) in args.iter_mut().zip(fs) {
                    self.check(a, ft, d)?;
                }
                *fields = fs.iter().map(|(n, _)| n.clone()).collect();
                Ok(())
            }
            (Expr::None, Type::Opt(_) | Type::Any) => Ok(()),
            (Expr::List(xs), Type::List(et)) => xs.iter_mut().try_for_each(|x| self.check(x, et, d)),
            (Expr::Append(l, r), Type::List(_)) => {
                self.check(l, t, d)?;
                self.check(r, t, d)
            }
            (Expr::Tuple(xs), Type::Tuple(ts)) if xs.len() == ts.len() => xs.iter_mut().zip(ts).try_for_each(|(x, t)| self.check(x, t, d)),
            (Expr::Record(fs), Type::Record(ts)) if fs.len() == ts.len() => {
                for (f, t) in ts {
                    let x = fs.iter_mut().find(|(n, _)| n == f).ok_or_else(|| format!("the record has no field {f}"))?;
                    self.check(&mut x.1, t, d)?;
                }
                Ok(())
            }
            (Expr::Str(_), Type::Ref | Type::Agent | Type::Ticket | Type::Text) => Ok(()),
            (e, Type::Opt(inner)) if !matches!(e, Expr::Var(_) | Expr::Field(..) | Expr::Ctx(_)) => self.check(e, inner, d),
            (e, t) => {
                let et = self.infer(e, d)?;
                if et.fits(t) { Ok(()) } else { Err(format!("this is a {et}, where {t} is wanted")) }
            }
        }
    }

    /// A step's worker: a variable that names one must be an agent; any other expression must be of type Agent.
    fn worker(&self, w: &mut Worker, d: &Delta) -> Result<(), String> {
        match w {
            Worker::Named(n) => match d.get(n.as_str()) {
                Some(t) if !t.fits(&Type::Agent) => Err(format!("`{n}` is a {t}, not an agent")),
                _ => Ok(()),
            },
            Worker::Expr(e) => match self.infer(e, d)? {
                t if t.fits(&Type::Agent) => Ok(()),
                t => Err(format!("the worker `{}` is a {t}, not an agent", crate::print::expr(e))),
            },
            Worker::Fork(a) => self.fork_of(a, d),
            _ => Ok(()),
        }
    }

    /// What `fork A` copies (`step fork A`, `let agent x = fork A`, N-101): an agent the program knows, so an expression
    /// of type Agent (a let agent, a form's Agent, an earlier answer's agent); not a standing agent by a bare name, and
    /// not the owner, who has no conversation to copy.
    fn fork_of(&self, a: &mut Expr, d: &Delta) -> Result<(), String> {
        if let Expr::Var(n) = a {
            if n == "owner" && !d.contains_key("owner") {
                return Err("`fork owner`: the owner has no conversation to copy".into());
            }
            if !d.contains_key(n.as_str()) {
                return Err(format!("`fork {n}`: {n} is not bound here; a fork copies an agent the program knows (a let agent, a form's Agent, the agent of an answer)"));
            }
        }
        match self.infer(a, d)? {
            t if t.fits(&Type::Agent) => Ok(()),
            t => Err(format!("`fork {}`: this is a {t}, not an agent", crate::print::expr(a))),
        }
    }

    /// `WORD x in e where cond` (`find`, `all`, `any`): the type of an element of the list E, COND a Bool with X one
    /// of them, read with no fact of the record's lines in it.
    fn element(&self, word: &str, x: &str, e: &mut Expr, cond: &mut Expr, d: &Delta) -> Result<Type, String> {
        let el = match self.infer(e, d)? {
            Type::List(t) => *t,
            Type::Any => Type::Any,
            t => return Err(format!("`{word}` looks in a list, and this is a {t}")),
        };
        let mut d2 = d.clone();
        d2.insert(x.to_string(), el.clone());
        let ct = Cx { facts: false, ..*self }.infer(cond, &d2)?;
        if !ct.fits(&Type::Bool) {
            return Err(format!("`{word}`'s `where` takes a Bool, not a {ct}"));
        }
        Ok(el)
    }

    fn infer(&self, e: &mut Expr, d: &Delta) -> Result<Type, String> {
        Ok(match e {
            Expr::Str(_) => Type::Text,
            Expr::Int(_) => Type::Int,
            Expr::Bool(_) => Type::Bool,
            Expr::None => Type::Opt(Box::new(Type::Any)),
            // the word `agents`, unless a binding of that name hides it: the project's agents (item 11, G1)
            Expr::Var(v) if v == crate::types::AGENTS && !d.contains_key(v.as_str()) => Type::Agents,
            Expr::Var(v) => d.get(v.as_str()).cloned().ok_or_else(|| format!("`{v}` is not bound here"))?,
            Expr::Post(_) => Type::Text,
            Expr::Cmp { l, r, .. } => {
                self.infer(l, d)?;
                self.infer(r, d)?;
                Type::Bool
            }
            Expr::Rel { l, r, rel } => {
                for x in [l, r] {
                    let t = self.infer(x, d)?;
                    if !matches!(t, Type::Int | Type::Any) {
                        return Err(format!("`{}` orders numbers, and `{}` is a {t}", rel.sym(), crate::print::expr(x)));
                    }
                }
                Type::Bool
            }
            Expr::ResultOf { e, kind } => {
                self.check(e, &Type::Ticket, d)?;
                match kind {
                    Some(k) => self.kinds.get(k.as_str()).map(|(_, r, _)| r.clone()).ok_or_else(|| format!("`result of … as {k}`: there is no kind {k}"))?,
                    None => Type::Any,
                }
            }
            // an answer's whole text and who gave it (N-101): of a record or variant, as a step answers
            Expr::TextOf { e, ty } => {
                let t = self.infer(e, d)?;
                if !matches!(t, Type::Variant(_) | Type::Record(_) | Type::Any) {
                    return Err(format!("`text of` takes an answer (a record or variant), and this is a {t}"));
                }
                *ty = Some(t);
                Type::Text
            }
            Expr::ByOf(e) => {
                let t = self.infer(e, d)?;
                if !matches!(t, Type::Variant(_) | Type::Record(_) | Type::Any) {
                    return Err(format!("`by of` takes an answer (a record or variant), and this is a {t}"));
                }
                Type::Agent
            }
            Expr::And(l, r) => {
                for e in [l, r] {
                    let t = self.infer(e, d)?;
                    if !t.fits(&Type::Bool) {
                        return Err(format!("`and` joins Bools, and this is a {t}"));
                    }
                }
                Type::Bool
            }
            Expr::Or(l, r) => {
                for e in [l, r] {
                    let t = self.infer(e, d)?;
                    if !t.fits(&Type::Bool) {
                        return Err(format!("`or` joins Bools, and this is a {t}"));
                    }
                }
                Type::Bool
            }
            Expr::Ctx(_) => Type::Any,
            Expr::Field(inner, f) => field_type(&self.infer(inner, d)?, f)?,
            // `find x in e where cond`: an element of the list E, COND a Bool with X one of them
            Expr::Find { x, e, cond } => self.element("find", x, e, cond, d)?,
            // `all x in e where cond`, `any …`: a Bool, read as `find` is
            Expr::Quant { all, x, e, cond } => {
                self.element(if *all { "all" } else { "any" }, x, e, cond, d)?;
                Type::Bool
            }
            Expr::Not(c) => {
                let t = self.infer(c, d)?;
                if !t.fits(&Type::Bool) {
                    return Err(format!("`not` takes a Bool, and this is a {t}"));
                }
                Type::Bool
            }
            // a fact of the record's lines (N-1446 p2152): only where `Machine::on_answer` reads them
            Expr::Fact { name, args } => {
                if !self.facts {
                    return Err(format!("`{name}` is a fact of the record's lines, read only in a step's `on`, outside a `find`"));
                }
                let (ps, t) = crate::syntax::fact_sig(name).ok_or_else(|| format!("there is no fact {name}"))?;
                if args.len() != ps.len() {
                    return Err(format!("`{name}` takes {} arguments, and here has {}", ps.len(), args.len()));
                }
                for (a, p) in args.iter_mut().zip(&ps) {
                    self.check(a, p, d)?;
                }
                t
            }
            Expr::Con { name, .. } => return Err(format!("{name}: a constructor where its type is not known")),
            Expr::List(xs) => {
                let mut t = Type::Any;
                for x in xs.iter_mut() {
                    t = self.infer(x, d)?;
                }
                Type::List(Box::new(t))
            }
            Expr::Tuple(xs) => Type::Tuple(xs.iter_mut().map(|x| self.infer(x, d)).collect::<Result<_, _>>()?),
            // `a ++ b`: two lists of one element type
            Expr::Append(l, r) => {
                let t = self.infer(l, d)?;
                match &t {
                    Type::List(_) | Type::Any => self.check(r, &t, d)?,
                    t => return Err(format!("`++` joins lists, and this is a {t}")),
                }
                t
            }
            Expr::Record(fs) => Type::Record(fs.iter_mut().map(|(n, x)| Ok((n.clone(), self.infer(x, d)?))).collect::<Result<_, String>>()?),
        })
    }

    /// Check pattern P against T, binding its variables into D and filling in constructors' field names.
    fn pat(&self, p: &mut Pat, t: &Type, d: &mut Delta) -> Result<(), String> {
        match p {
            Pat::Wild => Ok(()),
            Pat::Var(v) => {
                d.insert(v.clone(), t.clone());
                Ok(())
            }
            Pat::Tuple(ps) => match t {
                Type::Tuple(ts) if ts.len() == ps.len() => ps.iter_mut().zip(ts).try_for_each(|(p, t)| self.pat(p, t, d)),
                _ => Err(format!("a tuple of {} where the value is {t}", ps.len())),
            },
            Pat::Nil => match t {
                Type::List(_) | Type::Any => Ok(()),
                _ => Err(format!("[] where the value is {t}, not a list")),
            },
            Pat::None => match t {
                Type::Opt(_) | Type::Any => Ok(()),
                _ => Err(format!("none where the value is {t}, never absent")),
            },
            Pat::One(p) => match t {
                Type::List(e) => self.pat(p, e, d),
                Type::Any => self.pat(p, &Type::Any, d),
                _ => Err(format!("[{}] where the value is {t}, not a list", crate::print::pat(p))),
            },
            Pat::Con { name, args, fields } => {
                // on a value of unknown type (`result of e`): the declared type with that constructor, or the record
                let found;
                let fs = match t {
                    Type::Any => {
                        found = self.named_with(name);
                        let t = found.as_ref().ok_or_else(|| format!("{name}: the value's type is not known here, and no declared type has {name} (`result of e as KIND` names it)"))?;
                        self.con(t, name).ok_or_else(|| format!("{name} is not a constructor of {t}"))?
                    }
                    t => self.con(t, name).ok_or_else(|| format!("{name} is not a constructor of {t}"))?,
                };
                // a pattern may name the first fields only: a replaced call's answer may carry more parts than the kind
                // that matches it names (the owner, N-115.p3: "yes can carry extra"); those after are not bound
                if args.len() > fs.len() {
                    return Err(format!("{name} has {} fields, the pattern names {}", fs.len(), args.len()));
                }
                for (a, (_, ft)) in args.iter_mut().zip(fs) {
                    self.pat(a, ft, d)?;
                }
                *fields = fs.iter().map(|(n, _)| n.clone()).collect();
                args.extend(fs[args.len()..].iter().map(|_| Pat::Wild));
                Ok(())
            }
        }
    }
}

/// Whether the pattern row Q matches some value that no row of ROWS matches (Maranget's usefulness), the columns of
/// types TYS.
fn useful(rows: &[Vec<Pat>], q: &[Pat], tys: &[Type]) -> bool {
    if q.is_empty() {
        return rows.is_empty();
    }
    let rest = |r: &[Pat]| r[1..].to_vec();
    match (&q[0], &tys[0]) {
        // an optional is none or a value: `none` takes the rows that match an absent value; any other pattern those
        // that match a present one, against the value's own type; `_` is useful where either is
        (Pat::None, _) => {
            let nrows: Vec<Vec<Pat>> = rows.iter().filter(|r| matches!(r[0], Pat::None | Pat::Wild | Pat::Var(_))).map(|r| rest(r)).collect();
            useful(&nrows, &rest(q), &tys[1..])
        }
        (p, Type::Opt(e)) => {
            let srows: Vec<Vec<Pat>> = rows.iter().filter(|r| !matches!(r[0], Pat::None)).cloned().collect();
            let mut ntys = vec![(**e).clone()];
            ntys.extend(tys[1..].iter().cloned());
            let some = useful(&srows, q, &ntys);
            match p {
                Pat::Wild | Pat::Var(_) => {
                    let nrows: Vec<Vec<Pat>> = rows.iter().filter(|r| matches!(r[0], Pat::None | Pat::Wild | Pat::Var(_))).map(|r| rest(r)).collect();
                    some || useful(&nrows, &rest(q), &tys[1..])
                }
                _ => some,
            }
        }
        (Pat::Con { name, args, .. }, t) => {
            let fts = match t {
                // a declared record: its one constructor
                Type::Record(fs) => fs.iter().map(|(_, t)| t.clone()).collect(),
                Type::Any => vec![Type::Any; args.len()],
                t => t.constructor(name).map(|fs| fs.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>()).unwrap_or_default(),
            };
            let srows = specialize(rows, name, fts.len());
            let mut nq = args.clone();
            nq.extend(rest(q));
            let mut ntys = fts;
            ntys.extend(tys[1..].iter().cloned());
            useful(&srows, &nq, &ntys)
        }
        (Pat::Tuple(ps), Type::Tuple(ts)) => {
            let srows = expand_tuples(rows, ts.len());
            let mut nq = ps.clone();
            nq.extend(rest(q));
            let mut ntys = ts.clone();
            ntys.extend(tys[1..].iter().cloned());
            useful(&srows, &nq, &ntys)
        }
        // a declared record matched by its name (`Handed(c, top, a)`): its one constructor
        (Pat::Wild | Pat::Var(_), Type::Record(fs)) if rows.iter().any(|r| matches!(r[0], Pat::Con { .. })) => {
            let c = rows.iter().find_map(|r| if let Pat::Con { name, .. } = &r[0] { Some(name.clone()) } else { None }).unwrap_or_default();
            let srows = specialize(rows, &c, fs.len());
            let mut nq = vec![Pat::Wild; fs.len()];
            nq.extend(rest(q));
            let mut ntys: Vec<Type> = fs.iter().map(|(_, t)| t.clone()).collect();
            ntys.extend(tys[1..].iter().cloned());
            useful(&srows, &nq, &ntys)
        }
        (Pat::Wild | Pat::Var(_), Type::Variant(cs)) => cs.iter().any(|(c, fs)| {
            let srows = specialize(rows, c, fs.len());
            let mut nq = vec![Pat::Wild; fs.len()];
            nq.extend(rest(q));
            let mut ntys: Vec<Type> = fs.iter().map(|(_, t)| t.clone()).collect();
            ntys.extend(tys[1..].iter().cloned());
            useful(&srows, &nq, &ntys)
        }),
        // a list is [], [x] or longer: `[]` takes the rows that match the empty list, `[p]` those that match a list of
        // one (its element a column of its own); `_` is useful where any of the three is
        (Pat::Nil, _) => {
            let nrows: Vec<Vec<Pat>> = rows.iter().filter(|r| matches!(r[0], Pat::Nil | Pat::Wild | Pat::Var(_))).map(|r| rest(r)).collect();
            useful(&nrows, &rest(q), &tys[1..])
        }
        (Pat::One(p), t) => {
            let e = match t {
                Type::List(e) => (**e).clone(),
                _ => Type::Any,
            };
            let mut nq = vec![(**p).clone()];
            nq.extend(rest(q));
            let mut ntys = vec![e];
            ntys.extend(tys[1..].iter().cloned());
            useful(&ones(rows), &nq, &ntys)
        }
        (Pat::Wild | Pat::Var(_), Type::List(e)) => {
            let nrows: Vec<Vec<Pat>> = rows.iter().filter(|r| matches!(r[0], Pat::Nil | Pat::Wild | Pat::Var(_))).map(|r| rest(r)).collect();
            let drows: Vec<Vec<Pat>> = rows.iter().filter(|r| matches!(r[0], Pat::Wild | Pat::Var(_))).map(|r| rest(r)).collect();
            let mut oq = vec![Pat::Wild];
            oq.extend(rest(q));
            let mut otys = vec![(**e).clone()];
            otys.extend(tys[1..].iter().cloned());
            useful(&nrows, &rest(q), &tys[1..]) || useful(&ones(rows), &oq, &otys) || useful(&drows, &rest(q), &tys[1..])
        }
        (Pat::Wild | Pat::Var(_), Type::Tuple(ts)) => {
            let srows = expand_tuples(rows, ts.len());
            let mut nq = vec![Pat::Wild; ts.len()];
            nq.extend(rest(q));
            let mut ntys = ts.clone();
            ntys.extend(tys[1..].iter().cloned());
            useful(&srows, &nq, &ntys)
        }
        _ => {
            let drows: Vec<Vec<Pat>> = rows.iter().filter(|r| matches!(r[0], Pat::Wild | Pat::Var(_))).map(|r| rest(r)).collect();
            useful(&drows, &rest(q), &tys[1..])
        }
    }
}

fn specialize(rows: &[Vec<Pat>], c: &str, arity: usize) -> Vec<Vec<Pat>> {
    rows.iter()
        .filter_map(|r| {
            let mut head = match &r[0] {
                Pat::Con { name, args, .. } if name == c => args.clone(),
                Pat::Con { .. } => return None,
                _ => vec![Pat::Wild; arity],
            };
            head.extend(r[1..].iter().cloned());
            Some(head)
        })
        .collect()
}

/// The rows that match a list of one, its element put first in its place.
fn ones(rows: &[Vec<Pat>]) -> Vec<Vec<Pat>> {
    rows.iter()
        .filter_map(|r| {
            let mut head = match &r[0] {
                Pat::One(p) => vec![(**p).clone()],
                Pat::Wild | Pat::Var(_) => vec![Pat::Wild],
                _ => return None,
            };
            head.extend(r[1..].iter().cloned());
            Some(head)
        })
        .collect()
}

fn expand_tuples(rows: &[Vec<Pat>], n: usize) -> Vec<Vec<Pat>> {
    rows.iter()
        .map(|r| {
            let mut head = match &r[0] {
                Pat::Tuple(ps) => ps.clone(),
                _ => vec![Pat::Wild; n],
            };
            head.extend(r[1..].iter().cloned());
            head
        })
        .collect()
}

/// Where a ticket waits on tickets below it: the scope D (`_` left out) and the type T of what it waits for.
fn waits_at(d: &BTreeMap<String, Type>, t: &Type) -> crate::syntax::Wait {
    crate::syntax::Wait { scope: d.iter().filter(|(n, _)| n.as_str() != "_").map(|(n, t)| (n.clone(), t.clone())).collect(), ty: Some(t.clone()) }
}
