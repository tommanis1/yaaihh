//! Facts that acts make and end (the engine page's move 6, piece 1; paper-research N-1446 p3020, 8): a record line is
//! the act it is, named by its `ev`, with its own fields; an `Event` of that name in the policy (eFLINT's) with
//! `Creates` or `Terminates` lines makes or ends a fact when the line is written, its arguments the line's fields of
//! the same names. A machine's own `event` line (`{"ev":"event","name":N,"value":{...}}`, as `mail_hold` is written) is
//! the event its `name` names, its fields those of its `value`: a new kind of line would be a problem line to every
//! engine before it, an event no binding is on is read by them and changes nothing. The facts are among those every act
//! is given (`Core::norm_facts`) and those a `decide` is asked in (`decision_facts`), so a program reads them with the
//! `decide` it already has (piece 2), and no fact of them is matched by name in the engine.
//!
//! The store is the record taken under the policy text in force: folded from the record at the start, then each line
//! as it is written, and folded again from the record when the policy's files change.

use crate::policy::syntax::{Cond, DeclKind, Effect, Expr, Lit, Op, Rank};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// The policy's events, by the name a record line's `ev` gives them.
#[derive(Clone, Debug, Default)]
pub struct Rules {
    acts: BTreeMap<String, Vec<Effect>>,
    /// The decisions whose rules (options, `Requires`, `Prefer`, `Only`) read a fact the events make, or one derived
    /// from such a fact: a `decide` of one of them is given the store's facts (`decision_facts`); any other is asked
    /// as before.
    pub readers: BTreeSet<String>,
    /// The policy's files and their times, as the rules were read from them (`Places::stamp`).
    pub stamp: Vec<(String, Option<std::time::SystemTime>)>,
}

impl Rules {
    /// The events of PLACES' policy (the system's, the project's and its agents'). Err, naming its line, when an
    /// effect's fact is no fact type, or not with that many identifiers.
    pub fn of(places: &crate::policy::Places) -> Result<Rules, String> {
        let stamp = places.stamp();
        let scopes = places.sources()?;
        let mut types: BTreeMap<&str, usize> = BTreeMap::new();
        for (_, decls) in &scopes {
            for d in decls {
                if let DeclKind::Fact { name, ids, .. } = &d.kind {
                    types.insert(name, ids.len());
                }
            }
        }
        let mut acts: BTreeMap<String, Vec<Effect>> = BTreeMap::new();
        for (_, decls) in &scopes {
            for d in decls {
                let DeclKind::Event { name, effects, .. } = &d.kind else { continue };
                for e in effects {
                    match types.get(e.fact.as_str()) {
                        None => return Err(format!("{}:{}: `{}`: {} is no fact type", e.at.file, e.at.line, e.text, e.fact)),
                        Some(n) if *n != e.args.len() => {
                            return Err(format!("{}:{}: `{}`: {} is identified by {n}, not {}", e.at.file, e.at.line, e.text, e.fact, e.args.len()));
                        }
                        _ => {}
                    }
                }
                if !effects.is_empty() {
                    acts.entry(name.clone()).or_default().extend(effects.iter().cloned());
                }
            }
        }
        // the facts the events make, and those derived from them, to a fixpoint
        let mut made: BTreeSet<String> = acts.values().flatten().map(|e| e.fact.clone()).collect();
        loop {
            let before = made.len();
            for d in scopes.iter().flat_map(|(_, ds)| ds) {
                if let DeclKind::Fact { name, derived, .. } | DeclKind::Extend { name, derived } = &d.kind {
                    if derived.iter().any(|x| cond_reads(&x.when, &made)) {
                        made.insert(name.clone());
                    }
                }
            }
            if made.len() == before {
                break;
            }
        }
        let mut readers = BTreeSet::new();
        for d in scopes.iter().flat_map(|(_, ds)| ds) {
            let reader = match &d.kind {
                DeclKind::Decision { name, options, requires } => (made.contains(options) || requires.iter().any(|(_, c, _)| cond_reads(c, &made))).then_some(name),
                DeclKind::Prefer { decision, when, rank } => (cond_reads(when, &made) || rank_reads(rank, &made)).then_some(decision),
                DeclKind::Only { decision, when, cond } => (cond_reads(when, &made) || cond_reads(cond, &made)).then_some(decision),
                _ => None,
            };
            readers.extend(reader.cloned());
        }
        Ok(Rules { acts, readers, stamp })
    }

    pub fn is_empty(&self) -> bool {
        self.acts.is_empty()
    }

    /// The `ev` of a record line as written (`{"ev":"NAME",...`), or an `event` line's `name` (written after its
    /// `at`), when an event is named so: the lines a fold reads, without parsing any other.
    fn named<'a>(&self, line: &'a str) -> Option<&'a str> {
        let rest = line.strip_prefix("{\"ev\":\"")?;
        let mut name = &rest[..rest.find('"')?];
        if name == "event" {
            let r = &rest[rest.find(",\"name\":\"")? + 9..];
            name = &r[..r.find('"')?];
        }
        self.acts.contains_key(name).then_some(name)
    }
}

/// Whether E reads one of the fact types MADE (as a function, `price(option)`, at any depth).
fn expr_reads(e: &Expr, made: &BTreeSet<String>) -> bool {
    matches!(e, Expr::App(f, args) if made.contains(f) || args.iter().any(|a| expr_reads(a, made)))
}

fn cond_reads(c: &Cond, made: &BTreeSet<String>) -> bool {
    c.iter().any(|l| match l {
        Lit::Holds(f, args) | Lit::Not(f, args) => made.contains(f) || args.iter().any(|a| expr_reads(a, made)),
        Lit::Cmp(a, _, b) => expr_reads(a, made) || expr_reads(b, made),
    })
}

fn rank_reads(r: &Rank, made: &BTreeSet<String>) -> bool {
    match r {
        Rank::Order(cs) => cs.iter().any(|c| cond_reads(c, made)),
        Rank::Minimise(e) | Rank::Maximise(e) => expr_reads(e, made),
    }
}

/// The facts the record's acts have made and not ended, each as a fact type's name and its values.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effects {
    pub facts: BTreeSet<(String, Vec<String>)>,
}

impl Effects {
    /// TEXT, a record, folded under RULES.
    pub fn of_record(rules: &Rules, text: &str) -> Effects {
        let mut e = Effects::default();
        if !rules.is_empty() {
            text.lines().for_each(|l| e.take_text(rules, l));
        }
        e
    }

    /// A record line as written, done as the act it names.
    pub fn take_text(&mut self, rules: &Rules, line: &str) {
        if rules.named(line).is_some() {
            if let Ok(v) = serde_json::from_str::<Value>(line) {
                self.take(rules, &v);
            }
        }
    }

    /// LINE done as the act its `ev` names (an `event` line's `name`, its fields its `value`'s): its `Terminates`
    /// first, then its `Creates`, as eFLINT does an act; each only when its `When` holds of the line's fields
    /// (docket-edge5 N-1446 p3123, 1), and a `Terminates` with a `Foreach` ending every instance its other arguments
    /// match (p3123, 2; eFLINT's form, p3127). A field that is a list or an object is its JSON text (`["coordd"]`), as a
    /// `decide`'s call field is (docket-edge6 p3126, 3). An effect naming a field the line does not have is not done,
    /// and a `When` comparing one does not hold.
    pub fn take(&mut self, rules: &Rules, line: &Value) {
        let (name, fields) = match line.get("ev").and_then(Value::as_str) {
            Some("event") => (line.get("name").and_then(Value::as_str), line.get("value").unwrap_or(&Value::Null)),
            ev => (ev, line),
        };
        let Some(effects) = name.and_then(|n| rules.acts.get(n)) else { return };
        let value = |a: &Expr| -> Option<String> {
            match a {
                Expr::Str(s) => Some(s.clone()),
                Expr::Int(n) => Some(n.to_string()),
                Expr::Field(f) => match fields.get(f)? {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    Value::Bool(b) => Some(b.to_string()),
                    v @ (Value::Array(_) | Value::Object(_)) => serde_json::to_string(v).ok(),
                    Value::Null => None,
                },
                _ => None,
            }
        };
        let holds = |e: &Effect| {
            e.when.iter().all(|l| match l {
                Lit::Cmp(a, op, b) => value(a).zip(value(b)).is_some_and(|(x, y)| compare(&x, *op, &y)),
                _ => false,
            })
        };
        // each argument's value, a `Foreach` name's as None (any); None when a field is missing
        let pattern = |e: &Effect| -> Option<(String, Vec<Option<String>>)> {
            Some((e.fact.clone(), e.args.iter().map(|a| if *a == Expr::Any { Some(None) } else { value(a).map(Some) }).collect::<Option<Vec<_>>>()?))
        };
        for (f, args) in effects.iter().filter(|e| !e.creates && holds(e)).filter_map(pattern) {
            if args.iter().all(Option::is_some) {
                self.facts.remove(&(f, args.into_iter().flatten().collect()));
            } else {
                self.facts.retain(|(g, vs)| !(*g == f && vs.len() == args.len() && vs.iter().zip(&args).all(|(v, a)| a.as_ref().is_none_or(|a| a == v))));
            }
        }
        for (f, args) in effects.iter().filter(|e| e.creates && holds(e)).filter_map(pattern) {
            self.facts.insert((f, args.into_iter().flatten().collect()));
        }
    }
}

/// X OP Y, two of a line's values: as numbers when both read as one, else as text.
fn compare(x: &str, op: Op, y: &str) -> bool {
    let o = match (x.parse::<i64>(), y.parse::<i64>()) {
        (Ok(a), Ok(b)) => a.cmp(&b),
        _ => x.cmp(y),
    };
    match op {
        Op::Eq => o.is_eq(),
        Op::Ne => o.is_ne(),
        Op::Lt => o.is_lt(),
        Op::Le => o.is_le(),
        Op::Gt => o.is_gt(),
        Op::Ge => o.is_ge(),
    }
}

/// How often the written lines' fold looks whether the policy's files changed (`Places::stamp`).
const CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(10);

/// The rules and the facts they made, as `Tickets` holds them.
#[derive(Debug, Default)]
pub struct Store {
    pub rules: Rules,
    pub held: Effects,
    pub(crate) checked: Option<std::time::Instant>,
}

impl Store {
    /// The store of PLACES' policy over TEXT, the record as the start read it; with why, when the policy's acts with
    /// effects do not read (then none is done).
    pub fn start(places: &crate::policy::Places, text: &str) -> (Store, Option<String>) {
        let (rules, why) = match Rules::of(places) {
            Ok(r) => (r, None),
            Err(e) => (Rules { stamp: places.stamp(), ..Default::default() }, Some(e)),
        };
        let held = Effects::of_record(&rules, text);
        (Store { rules, held, checked: Some(std::time::Instant::now()) }, why)
    }

    /// LINES, just written to the record LOG, done as their acts; when the policy's files changed since the rules were
    /// read, the rules read again and the whole record folded under them, these lines with it. With why, when the
    /// new rules do not read (the old ones are kept).
    pub fn take(&mut self, places: &crate::policy::Places, log: &str, lines: &[String]) -> Option<String> {
        if self.checked.is_none_or(|t| t.elapsed() > CHECK_EVERY) {
            self.checked = Some(std::time::Instant::now());
            if places.stamp() != self.rules.stamp {
                return match Rules::of(places) {
                    Ok(rules) => match std::fs::read_to_string(log) {
                        Ok(text) => {
                            self.held = Effects::of_record(&rules, &text);
                            self.rules = rules;
                            None
                        }
                        Err(e) => Some(format!("{log}: {e}: the acts' facts are not folded again")),
                    },
                    Err(e) => {
                        self.rules.stamp = places.stamp();
                        lines.iter().for_each(|l| self.held.take_text(&self.rules, l));
                        Some(format!("{e}: the acts' effects as they were read before are kept"))
                    }
                };
            }
        }
        lines.iter().for_each(|l| self.held.take_text(&self.rules, l));
        None
    }

    /// The facts the acts have made and not ended.
    pub fn facts(&self) -> impl Iterator<Item = (String, Vec<String>)> + '_ {
        self.held.facts.iter().cloned()
    }
}
