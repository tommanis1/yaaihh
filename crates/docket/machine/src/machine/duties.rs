//! A goal's duties, asked of the project's policy at each line that touches the goal (`duties`; N-1511 p47, p49;
//! paper-research N-1446 p2139, 2, p2204).
//!
//! A goal is a ticket whose step awaits a [`DUTIES`] line by an `on` (item.kind's hold: `on duties(e) where e.goal ==
//! this …`). From the `asked` line whose binding opened it, or else the line at which it first awaits them, the
//! machine keeps its lines ([`Goal::lines`]): the lines of the input that handed in its first head, which coordd
//! raises that line after (`Machine::first_heads`, p2354), that `asked` line, the lines of its tickets and of the
//! heads handed in for its ask (`handed_in_for`), each
//! with the tickets under them, and his window's lines that run a head's commit. At each line that touches it while
//! its step awaits them, the policy is asked (`asked`, question `duties`), one ask at a time: a line that touches it
//! meanwhile asks again once the answer is in. The question carries its lines and the ends of the jobs run on its
//! heads' trees (`Seen::ends`), each as its act line: a job may end before the line that names its tree's commit.
//! coordd answers off the lock (`decided`, question `duties`, `{goal, owed, by, broken}`), and that line is the
//! [`DUTIES`] event the goal's step reads (`Machine::event_of`). No goal, no question: a job line that names no head
//! of an open goal costs nothing (p2204).

use super::seen::short;
use super::{ASKED, Cx, DUTIES, MERGED, Machine, POLICY, Status, WINDOW_RELEASE, WindowRelease};
use crate::line::{Effect, Line, Question};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

/// The question, as `asked` and `decided` write it.
pub const QUESTION: &str = "duties";

/// A goal's lines kept, and its ask ([`Machine::goal_lines`]).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Goal {
    /// Its lines as the record writes them, in the record's order: what the policy reads for it.
    pub lines: Vec<Value>,
    /// The step its ask stands at, while one does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asking: Option<u32>,
    /// A line touched it while it was asked: it is asked again once the answer is in.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub again: bool,
}

/// ANSWER as a [`DUTIES`] line's value, when it fits its type (`syntax::system_event`).
pub(super) fn duties_value(answer: &Value) -> Option<Value> {
    let ty = crate::syntax::system_event(DUTIES)?;
    ty.coerce(answer).ok().filter(|v| ty.conforms(v).is_ok())
}

impl Machine {
    /// The step of T's that awaits a [`DUTIES`] line by an `on`, when T is open and awaits one: T is a goal then.
    pub fn awaits_duties(&self, t: u64) -> Option<u32> {
        let x = self.tickets.get(&t).filter(|x| x.is_open())?;
        match &x.status {
            Status::Awaits { owed, .. } if owed.on.iter().any(|o| o.ev == DUTIES) => Some(owed.step),
            _ => None,
        }
    }

    /// Whether goal T's ask of its duties at STEP stands: asked, not answered, and T awaits them there still.
    pub fn asks_duties(&self, t: u64, step: u32) -> bool {
        self.goal_lines.get(&t).is_some_and(|g| g.asking == Some(step)) && self.awaits_duties(t) == Some(step)
    }

    /// The tickets goal G's lines are of, each with those under it: G, and the heads handed in for its ask
    /// (`Ticket::handed_in`), the post of the `asked` line that opened it, else its ask's or note's (an item's,
    /// `handed_in_for`): a head handed in for G by its id is one of them (paper-research N-1446 p2483).
    fn goal_roots(&self, g: u64) -> BTreeSet<u64> {
        let mut roots = BTreeSet::from([g]);
        let opened = self.goal_lines.get(&g).and_then(|x| x.lines.iter().find(|l| l["ev"] == "event" && l["name"] == ASKED)).map(|l| &l["value"]["post"]);
        let post = opened.and_then(|p| self.handed_in_for(&json!({"for": p}))).or_else(|| self.handed_in_for(&json!({"for": self.id(g)})));
        if let Some(x) = post.and_then(|(u, n)| Some((self.tickets.get(&u)?, n))) {
            roots.extend(x.0.handed_in.iter().filter(|(m, ..)| *m == x.1).filter_map(|(.., id)| self.ticket_named(id)));
        }
        roots
    }

    /// Whether ticket U is one of ROOTS or under one: its parent, or the ticket it was opened under, in turn.
    fn under_roots(&self, u: u64, roots: &BTreeSet<u64>) -> bool {
        let (mut next, mut seen) = (vec![u], BTreeSet::new());
        while let Some(u) = next.pop() {
            if roots.contains(&u) {
                return true;
            }
            if seen.insert(u) {
                next.extend(self.tickets.get(&u).into_iter().flat_map(|x| x.parent.into_iter().chain(x.under)));
            }
        }
        false
    }

    /// The commits of the heads in ROOTS (all but goal G), each by its first 8: each head's form's `commit`, and the
    /// last its steps were given (`step_args`, as a `read`'s).
    fn goal_commits(&self, roots: &BTreeSet<u64>, g: u64) -> BTreeSet<String> {
        let commit = |v: Option<&Value>| v.and_then(Value::as_str).map(short).filter(|c| !c.is_empty());
        roots.iter().filter(|u| **u != g).flat_map(|u| {
            let form = commit(self.tickets.get(u).and_then(|x| x.form.get("commit")));
            form.into_iter().chain(commit(self.step_args.get(u).and_then(|a| a.get("commit"))))
        }).collect()
    }

    /// The trees of COMMITS the lines kept show (`Seen::trees`), each by its first 8.
    fn trees_of(&self, commits: &BTreeSet<String>) -> BTreeSet<String> {
        commits.iter().filter_map(|c| self.seen.trees.get(c)).map(|t| short(t)).collect()
    }

    /// Whether commit A is one of COMMITS or has one under it, by the lines kept (`Seen::fact`, `reaches`).
    fn reaches_one(&self, a: &str, commits: &BTreeSet<String>) -> bool {
        !a.is_empty() && commits.iter().any(|c| self.seen.fact("reaches", &[json!(a), json!(c)]) == json!(true))
    }

    /// Whether his window's line V runs one of COMMITS: its window's release, or one it contains, is of a commit (its
    /// name's last part) that reaches one.
    fn runs_one(&self, v: &Value, commits: &BTreeSet<String>) -> bool {
        let Some(w) = WindowRelease::of(v) else { return false };
        std::iter::once(&w.window.release).chain(&w.window.contains).filter_map(|r| r.rsplit('-').next()).any(|r| self.reaches_one(r, commits))
    }

    /// Whether line L is goal G's to keep (ROOTS its tickets, COMMITS and TREES its heads'), and whether it touches G:
    /// a line of its tickets that opens, answers or closes one; his window's line that runs a head's commit; his post
    /// on G; a job's line on a head's commit or tree, a cut of a commit that reaches one, a merged line that brought
    /// one. A step's line is kept, and touches nothing: G entering its hold is asked at `goal_lines_after`.
    fn of_goal(&self, l: &Line, g: u64, roots: &BTreeSet<u64>, commits: &BTreeSet<String>, trees: &BTreeSet<String>) -> (bool, bool) {
        let ours = |t: &u64| self.under_roots(*t, roots);
        match l {
            // its own question and answer, and any other ticket's, are no line of what was done
            Line::Asked { .. } | Line::Decided { .. } => (false, false),
            Line::Event { name, value, .. } if name == WINDOW_RELEASE => {
                let runs = self.runs_one(value, commits);
                (runs, runs)
            }
            Line::Event { name, value, .. } if name == MERGED => {
                let listed = value.get("commits").and_then(Value::as_array).into_iter().flatten().filter_map(|c| c.get("commit"));
                (false, std::iter::once(value.get("tip")).flatten().chain(listed).filter_map(Value::as_str).any(|c| commits.contains(&short(c))))
            }
            // the `asked` line whose binding opened it
            Line::Event { fired, .. } => (fired.iter().any(|f| f.t == g), false),
            Line::Open { t, .. } | Line::Opened { t, .. } | Line::Answer { t, .. } | Line::Formed { t, .. } | Line::Closed { t, .. } => {
                let ours = ours(t);
                (ours, ours)
            }
            Line::Step { t, .. } | Line::StepArgs { t, .. } => (ours(t), false),
            Line::Post { t, by, .. } => (false, *t == g && by == "owner"),
            Line::Act { is, more, .. } => {
                let f = |k: &str| more.get(k).and_then(Value::as_str).map(short).unwrap_or_default();
                let touches = match (is.as_str(), more.get("phase").and_then(Value::as_str)) {
                    ("job", _) => commits.contains(&f("commit")) || trees.contains(&f("tree")),
                    ("release", Some("cut")) => self.reaches_one(&f("commit"), commits),
                    _ => false,
                };
                (false, touches)
            }
            _ => (false, false),
        }
    }

    /// After LINE is carried out and its steps answered (`exec`; FROM the first of the lines it made in `cx.lines`):
    /// each goal's lines kept, and its duties asked where its step awaits them and a line touched it, or it entered
    /// that step ([`Goal`]). A goal is in from the `asked` line that opened it, or else the line at which it first
    /// awaits them, until it is closed: an item's lead is filled at a later line (`hold role lead`), and that `asked`
    /// line is the policy's `take` and names the post its heads are handed in for (`goal_roots`).
    pub(super) fn goal_lines_after(&mut self, cx: &mut Cx, line: &Line, from: usize) {
        let entered: BTreeSet<u64> = cx.lines[from..].iter().filter_map(|l| match l {
            Line::Step { t, .. } => Some(*t),
            _ => None,
        }).filter(|t| self.awaits_duties(*t).is_some()).collect();
        let asked: Vec<(u64, &str)> = std::iter::once(line).chain(&cx.lines[from..]).flat_map(|l| match l {
            Line::Event { name, value, fired, .. } if name == ASKED => fired.iter().map(|f| (f.t, value["post"].as_str().unwrap_or_default())).collect(),
            _ => Vec::new(),
        }).collect();
        let opened: BTreeSet<u64> = asked.iter().map(|(t, _)| *t).collect();
        self.keep_first_head(std::iter::once(line).chain(&cx.lines[from..]));
        if entered.is_empty() && opened.is_empty() && self.goal_lines.is_empty() {
            return;
        }
        for (g, post) in &asked {
            let first = self.first_heads.remove(*post).unwrap_or_default();
            self.goal_lines.entry(*g).or_default().lines.extend(first);
        }
        let made: Vec<Line> = cx.lines[from..].to_vec();
        for t in entered.iter().chain(&opened) {
            self.goal_lines.entry(*t).or_default();
        }
        let tickets = &self.tickets;
        self.goal_lines.retain(|t, _| tickets.get(t).is_some_and(|x| x.is_open()));
        let goals: Vec<u64> = self.goal_lines.keys().copied().collect();
        for g in goals {
            let roots = self.goal_roots(g);
            let commits = self.goal_commits(&roots, g);
            let trees = self.trees_of(&commits);
            let mut touched = entered.contains(&g);
            let mut kept = Vec::new();
            for l in std::iter::once(line).chain(&made) {
                let (keep, touches) = self.of_goal(l, g, &roots, &commits, &trees);
                if keep {
                    kept.push(serde_json::to_value(l).unwrap_or(Value::Null));
                }
                touched |= touches;
            }
            let awaits = self.awaits_duties(g);
            let goal = self.goal_lines.get_mut(&g).unwrap();
            goal.lines.extend(kept);
            // an ask its step no longer awaits stands no more (coordd stops it, `asks_duties`)
            if goal.asking.is_some() && goal.asking != awaits {
                goal.asking = None;
            }
            let Some(step) = awaits.filter(|_| touched || (goal.again && goal.asking.is_none())) else { continue };
            if goal.asking.is_some() {
                goal.again = true;
                continue;
            }
            self.ask_duties(cx, g, step);
        }
    }

    /// LINES, one input's, when it hands in the first head for a post of his whose `asked` line is still to come and a
    /// binding is on it (`Machine::first_head_for`): the head's own kept by the post's id ([`Machine::first_heads`])
    /// until that line opens its goal, which coordd raises after this input (paper-research N-1446 p2354).
    fn keep_first_head<'a>(&mut self, lines: impl Iterator<Item = &'a Line> + Clone) {
        if !self.bound(ASKED) {
            return;
        }
        for l in lines.clone() {
            let Line::Open { t, form, .. } = l else { continue };
            let Some((u, n)) = self.first_head_for(*t, form) else { continue };
            let (roots, none) = (BTreeSet::from([*t]), BTreeSet::new());
            let kept = lines.clone().filter(|x| self.of_goal(x, *t, &roots, &none, &none).0).map(|x| serde_json::to_value(x).unwrap_or(Value::Null)).collect();
            let post = format!("{}.p{n}", self.id(u));
            self.first_heads.insert(post, kept);
        }
    }

    /// Goal G's duties asked of the policy at STEP, the step that awaits them: an `asked` line and the question. On
    /// replay, where the record asked it.
    fn ask_duties(&mut self, cx: &mut Cx, g: u64, step: u32) {
        let asked = match &cx.recorded {
            Some(rec) => rec.iter().any(|l| matches!(l, Line::Asked { t, question, .. } if *t == g && question == QUESTION)),
            None => true,
        };
        if !asked {
            return;
        }
        if let Some(goal) = self.goal_lines.get_mut(&g) {
            (goal.asking, goal.again) = (Some(step), false);
        }
        self.duties_asked += 1;
        cx.line(Line::Asked { at: cx.now.clone(), t: g, step, question: QUESTION.into(), role: POLICY.into() });
        cx.effects.extend(self.duties_question(g));
        cx.effect(Effect::Changed { t: g });
    }

    /// The question goal G's ask puts, while one stands (to ask again after a replay): its lines, with the end of
    /// each job run on its heads' trees as the lines kept show them (`Seen::ends`), as its act line at its time.
    pub(super) fn duties_question(&self, g: u64) -> Option<Effect> {
        let goal = self.goal_lines.get(&g)?;
        let step = goal.asking?;
        let trees = self.trees_of(&self.goal_commits(&self.goal_roots(g), g));
        let mut lines = goal.lines.clone();
        for (at, v) in self.seen.ends.iter().filter(|(t, _)| trees.contains(&short(t))).flat_map(|(_, es)| es) {
            // the act line its value was read from (`runner::act_value`): BY its agent, the ticket's id left out
            let mut l: Map<String, Value> = v.as_object().cloned().unwrap_or_default();
            let agent = l.remove("by").unwrap_or(Value::Null);
            l.remove("ticket");
            l.extend([("ev".to_string(), json!("act")), ("at".to_string(), json!(at)), ("agent".to_string(), agent)]);
            let i = lines.iter().position(|x| x.get("at").and_then(Value::as_str).is_some_and(|a| a > at.as_str())).unwrap_or(lines.len());
            lines.insert(i, Value::Object(l));
        }
        Some(Effect::Ask { t: g, step, question: Question::Duties { goal: self.id(g), lines } })
    }

    /// The policy's ANSWER to goal T's ask at STEP: its duties, `{goal, owed, by, broken}`, raised as the [`DUTIES`]
    /// line its step reads (`Machine::event_of`); none (null) when the ask ended unanswered or the policy could not
    /// read them (its provenance says why): asked again at the next line that touches it. Refused when T does not ask.
    pub(super) fn decided_duties(&mut self, cx: &mut Cx, t: u64, step: u32, answer: &Value, at: &str) -> Result<String, String> {
        let id = self.id(t);
        let fits = duties_value(answer).filter(|v| v["goal"] == json!(id));
        match self.goal_lines.get_mut(&t) {
            Some(g) if g.asking == Some(step) && (answer.is_null() || fits.is_some()) => {
                g.asking = None;
                Ok(match fits {
                    Some(mut v) => {
                        let said = format!("{id}'s duties: {} owed.", v["owed"].as_array().map_or(0, Vec::len));
                        // its latest, with the line's time, on its view (`duties_seen`; p2328, 2)
                        v["at"] = json!(at);
                        self.duties_seen.insert(t, v);
                        cx.effect(Effect::Changed { t });
                        said
                    }
                    None => format!("The ask of {id}'s duties at step {step} has no answer."),
                })
            }
            Some(g) if g.asking == Some(step) => Err(format!("That answer does not fit `duties` for {id}: {{goal, owed, by, broken}}, its goal {id}.")),
            _ if answer.is_null() => Ok(format!("The ask of {id}'s duties at step {step} ended: {id} no longer asks it.")),
            _ => Err(format!("{id} is not asking its duties at step {step}.")),
        }
    }
}
