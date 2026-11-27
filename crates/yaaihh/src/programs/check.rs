//! What is wrong in a stack of texts, each at its file and line (N-1988): a text that does not read, the stack that does
//! not load, rules that read themselves through `Not` (worked out by the well-founded reading, so a note), and a reaction
//! that answers a harness moment with a power a bridge's version does not let there and does not test `Lets` for (the
//! engine does not start a run of it for an agent on that version: std.hooks' "the check itself is the engine's").

use super::read::{self, Drawing, Is, Rule, Stated};
use docket_v2::check::Loaded;
use docket_v2::norm::Core;
use docket_v2::run::norms::Norms;
use docket_v2::syntax::tree::Module;
use std::collections::{BTreeMap, BTreeSet};

/// How bad a finding is: the stack is not what runs (or not all of it starts), or what runs is worth knowing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error,
    Note,
}

/// A finding at its text's path and line (0: the text as a whole).
#[derive(Clone, Debug, PartialEq)]
pub struct Diag {
    pub level: Level,
    pub path: String,
    pub line: u32,
    pub why: String,
}

/// A stack checked: each text that reads, as its module with its name, in load order; each module's path; the stack
/// loaded, when it loads; what was found, the errors first; the drawing of the texts that read, and their own facts.
#[derive(Default)]
pub struct Checked {
    pub modules: Vec<(String, Module)>,
    pub paths: BTreeMap<String, String>,
    pub loaded: Option<Loaded>,
    pub diags: Vec<Diag>,
    pub drawing: Drawing,
    pub stated: Vec<Stated>,
}

impl Checked {
    /// The path and line of PLACE, a rule's place as the engine writes it (`module:line`, `module:line:col`).
    pub fn place(&self, place: &str) -> Option<(String, u32)> {
        let mut it = place.split(':');
        let (m, line) = (it.next()?, it.next()?.parse().ok()?);
        Some((self.paths.get(m).cloned().unwrap_or_else(|| m.to_string()), line))
    }
}

/// TEXTS, each its path and words (or why there are none), in load order: read, drawn, loaded, and checked.
pub fn check(texts: &[(String, Result<String, String>)]) -> Checked {
    let mut c = Checked::default();
    for (path, words) in texts {
        let words = match words {
            Ok(w) => w,
            Err(why) => {
                c.diags.push(Diag { level: Level::Error, path: path.clone(), line: 0, why: why.clone() });
                continue;
            }
        };
        match docket_v2::syntax::read(words) {
            Ok(m) => {
                let name = read::name_of(&m);
                c.paths.insert(name.clone(), path.clone());
                c.modules.push((name, m));
            }
            Err(e) => c.diags.push(Diag { level: Level::Error, path: path.clone(), line: e.at.line, why: format!("does not read (column {}): {}", e.at.col, e.why) }),
        }
    }
    c.drawing = read::drawing(&c.modules);
    c.stated = read::stated(&c.modules);
    if c.modules.len() < texts.len() {
        return c;
    }
    match docket_v2::check::load(c.modules.iter().map(|(_, m)| m.clone()).collect()) {
        Ok(l) => c.loaded = Some(l),
        Err(e) => {
            let path = c.paths.get(&e.module).cloned().unwrap_or(e.module);
            c.diags.push(Diag { level: Level::Error, path, line: e.at.line, why: format!("does not load (column {}): {}", e.at.col, e.why) });
            return c;
        }
    }
    let knots = c.loaded.as_ref().map(knots).unwrap_or_default();
    for (place, why) in knots {
        let (path, line) = place.as_deref().and_then(|p| c.place(p)).unwrap_or_else(|| (texts[0].0.clone(), 0));
        c.diags.push(Diag { level: if place.is_some() { Level::Note } else { Level::Error }, path, line, why });
    }
    let powers = powers(&c.drawing, &c.stated);
    c.diags.extend(powers.into_iter().map(|(m, line, why)| Diag { level: Level::Error, path: c.paths.get(&m).cloned().unwrap_or(m), line, why }));
    c.diags.sort_by_key(|d| d.level);
    c
}

/// Each knot of derived types that reads itself through `Not`, `Forall`, a count, `Violated` or `Enabled`, at the place
/// of its first such read (`docket check`'s words); none at a place where the rules do not lower or work out at all.
fn knots(l: &Loaded) -> Vec<(Option<String>, String)> {
    let mut core = Core::default();
    if let Err(e) = core.load(&l.norms) {
        return vec![(None, format!("the rules do not lower to eFLINT: {e}"))];
    }
    let spec = match core.eflint() {
        Some(Ok((spec, _))) => spec,
        Some(Err(e)) => return vec![(None, format!("what holds cannot be worked out: {e}"))],
        None => return Vec::new(),
    };
    let g = eflint::eval::polarity(spec);
    g.knots
        .iter()
        .filter(|k| !k.falling.is_empty())
        .map(|k| {
            let types: Vec<String> = k.members.iter().map(|&i| g.types[i].to_string()).collect();
            let reads: Vec<String> = k.falling.iter().map(|r| r.describe()).collect();
            let at = k.falling.iter().find_map(|r| r.at.clone());
            (at, format!("not stratified: {} read each other through what can turn false, so the well-founded reading works them out ({})", types.join(", "), reads.join("; ")))
        })
        .collect()
}

/// What a harness version lets and reaches, from its bridge's own facts: each version, each moment it comes to (in any
/// mode), each moment's powers.
#[derive(Default)]
pub struct Lets {
    pub reaches: BTreeMap<(String, String), BTreeSet<String>>,
    pub lets: BTreeSet<(String, String, String, String)>,
}

impl Lets {
    pub fn of(stated: &[Stated]) -> Lets {
        let mut l = Lets::default();
        for s in stated {
            match (s.fact.as_str(), s.args.as_slice()) {
                ("std.hooks.Reaches", [h, v, m]) | ("std.hooks.Reaches-In", [h, v, _, m]) => {
                    l.reaches.entry((h.clone(), v.clone())).or_default().insert(m.clone());
                }
                ("std.hooks.Lets", [h, v, m, p]) => {
                    l.lets.insert((h.clone(), v.clone(), m.clone(), p.clone()));
                }
                _ => {}
            }
        }
        l
    }

    /// The versions that come to MOMENT and do not let POWER there, each `harness version`.
    pub fn lacking(&self, moment: &str, power: &str) -> Vec<String> {
        self.reaches
            .iter()
            .filter(|((h, v), ms)| ms.contains(moment) && !self.lets.contains(&(h.clone(), v.clone(), moment.to_string(), power.to_string())))
            .map(|((h, v), _)| format!("{h} {v}"))
            .collect()
    }
}

/// Each reaction to a harness moment that answers with a power some version reaching that moment does not let, and
/// whose condition does not test `Lets` for it: its module, its line, why.
pub fn powers(d: &Drawing, stated: &[Stated]) -> Vec<(String, u32, String)> {
    let lets = Lets::of(stated);
    let mut out = Vec::new();
    for r in d.rows.iter().filter(|r| r.rule == Rule::On) {
        let Some(m) = r.moment.as_deref().filter(|m| *m != "*") else { continue };
        for n in d.to.iter().filter(|n| n.is == Is::Answer && r.to.contains(&n.key)) {
            let Some(p) = n.needs.as_deref().filter(|p| !r.tests.iter().any(|t| t == p)) else { continue };
            let lacking = lets.lacking(m, p);
            if !lacking.is_empty() {
                out.push((r.module.clone(), r.line, format!("Hook power `{p}` not supported by {} at `{m}`, and the rule does not test `Lets` for it: the engine does not start a run of it for an agent on them", lacking.join(", "))));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(texts: &[(&str, &str)]) -> Checked {
        check(&texts.iter().map(|(p, t)| (p.to_string(), Ok(t.to_string()))).collect::<Vec<_>>())
    }

    /// The release's own texts read, load, and answer no moment with a power a version lacks.
    #[test]
    fn the_release_texts_have_no_error() {
        let c = super::super::Built::built_in().checked;
        assert!(c.loaded.is_some());
        assert_eq!(c.diags.iter().filter(|d| d.level == Level::Error).collect::<Vec<_>>(), Vec::<&Diag>::new());
        assert_eq!(c.place("std.hooks:12:3"), Some(("std/hooks.docket".to_string(), 12)));
    }

    /// A text that does not read is an error at its line; then nothing is loaded.
    #[test]
    fn a_text_that_does_not_read_is_an_error_at_its_line() {
        let c = stack(&[("a.docket", "module a\nFact P(x: Int))\n")]);
        assert_eq!((c.diags.len(), c.diags[0].level, c.diags[0].path.as_str(), c.diags[0].line), (1, Level::Error, "a.docket", 2));
        assert!(c.loaded.is_none());
        let c = stack(&[("a.docket", "module a\nFact P(x: Int)\nFact Q(x: Int)\n  Holds when R(x)\n")]);
        assert_eq!((c.diags.len(), c.diags[0].path.as_str(), c.loaded.is_none()), (1, "a.docket", true), "{:?}", c.diags);
        assert!(matches!(c.diags[0].line, 3 | 4) && c.diags[0].why.starts_with("does not load"), "{:?}", c.diags);
    }

    /// A rule that reads itself through Not is a note at its place, not an error.
    #[test]
    fn a_rule_that_reads_itself_through_not_is_a_note_at_its_place() {
        let c = stack(&[("b/a.docket", "module a\nFact P(x: Int)\nFact Q(x: Int)\n  Holds when P(x) && Not(Q(x))\n+P(1).\n")]);
        assert!(c.loaded.is_some(), "{:?}", c.diags);
        assert_eq!(c.diags.iter().map(|d| (d.level, d.path.as_str(), d.line)).collect::<Vec<_>>(), [(Level::Note, "b/a.docket", 4)], "{:?}", c.diags);
        assert!(c.diags[0].why.starts_with("not stratified: a.Q"), "{}", c.diags[0].why);
    }

    /// An answer with a power a version lacks at its moment is an error at the rule, unless the rule tests `Lets` for it;
    /// a version that never comes to the moment lacks nothing there.
    #[test]
    fn a_power_a_version_lacks_is_an_error_unless_the_rule_tests_for_it() {
        let hooks = "module std.hooks\nFact Party Identified by String\nFact Moment Identified by String\nFact Harness Identified by String\n\
            Fact Version Identified by String\nFact Power Identified by String\nFact Mode Identified by String\n\
            +Moment(\"turn_begins\").\n+Power(\"inject\"). +Power(\"modify\").\n\
            Fact Lets(harness: Harness, version: Version, moment: Moment, power: Power)\n\
            Fact Reaches(harness: Harness, version: Version, moment: Moment)\n\
            Fact Reaches-In(harness: Harness, version: Version, mode: Mode, moment: Moment)\n\
            Fact Running-On(agent: Party, harness: Harness, version: Version)\n\
            type HookAction =\n  | Allow\n  | Modify(patch: String) Needs \"modify\"\n\
            Effect Hook(agent: Party, moment: Moment, payload: String) -> HookAction Does { return Allow }\n";
        let bridge = "module std.bridges.w\nimport std.hooks as H\n+H.Reaches(\"w\", \"1.0\", \"turn_begins\").\n+H.Lets(\"w\", \"1.0\", \"turn_begins\", \"inject\").\n\
            +H.Mode(\"strict\").\n+H.Reaches-In(\"o\", \"2.0\", \"strict\", \"turn_begins\").\n+H.Lets(\"o\", \"2.0\", \"turn_begins\", \"modify\").\n\
            +H.Reaches(\"o\", \"2.0\", \"turn_end\").\n";
        let rule = |guard: &str| format!("module r\nimport std.hooks as H\non H.Hook(agent, \"turn_begins\", payload) where H.Running-On(agent, h, v){guard} do {{\n  return H.Modify(\"x\")\n}}\n");
        let c = stack(&[("hooks.docket", hooks), ("w.docket", bridge), ("r.docket", &rule(""))]);
        assert!(c.loaded.is_some(), "{:?}", c.diags);
        let errors: Vec<(&str, u32, &str)> = c.diags.iter().filter(|d| d.level == Level::Error).map(|d| (d.path.as_str(), d.line, d.why.as_str())).collect();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!((errors[0].0, errors[0].1), ("r.docket", 3));
        assert!(errors[0].2.starts_with("Hook power `modify` not supported by w 1.0 at `turn_begins`"), "{}", errors[0].2);
        let c = stack(&[("hooks.docket", hooks), ("w.docket", bridge), ("r.docket", &rule(" && H.Lets(h, v, \"turn_begins\", \"modify\")"))]);
        assert_eq!(c.diags.iter().filter(|d| d.level == Level::Error).count(), 0, "{:?}", c.diags);
    }
}
