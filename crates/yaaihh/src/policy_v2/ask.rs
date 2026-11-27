//! The Policy page's questions, each a closed `Holds(..)` the engine answers yes or no, and what the answers say.

use super::plan::Row;

/// What the engine said to a question: nothing until its answer comes, then yes or no, or why it gave neither.
pub type Ask<'a> = &'a dyn Fn(&str) -> Option<Result<bool, String>>;

/// A word as a docket text literal (a `{{` in it would read as a hole: no party's name has one).
fn quoted(w: &str) -> String {
    let mut s = String::from('"');
    for c in w.chars() {
        match c {
            '\\' | '"' => s.extend(['\\', c]),
            '\n' => s.push_str("\\n"),
            '\t' => s.push_str("\\t"),
            c => s.push(c),
        }
    }
    s.push('"');
    s
}

/// Whether FACT holds of ARGS.
pub fn holds(fact: &str, args: &[&str]) -> String {
    format!("Holds({fact}({}))", args.iter().map(|a| quoted(a)).collect::<Vec<_>>().join(", "))
}

/// The arguments of a fact's instance: the party first when it is per party, then the value.
fn args<'a>(per_party: bool, party: &'a str, value: &'a str) -> Vec<&'a str> {
    if per_party { vec![party, value] } else { vec![value] }
}

/// What a fact says at one place: the values that hold there, or that an answer has not come, or the first refusal.
#[derive(Clone, Debug, PartialEq)]
pub enum Said {
    Waiting,
    Holds(Vec<String>),
    Refused(String),
}

impl Said {
    /// The one value that holds, if exactly one does.
    pub fn one(&self) -> Option<&str> {
        match self {
            Said::Holds(v) if v.len() == 1 => Some(&v[0]),
            _ => None,
        }
    }
}

/// What FACT says of PARTY over ROW's values; every question is asked, so all of them go out at once.
fn said(ask: Ask, row: &Row, fact: &str, per_party: bool, party: &str) -> Said {
    let answers: Vec<_> = row.values.iter().map(|v| ask(&holds(fact, &args(per_party, party, v)))).collect();
    let mut holding = Vec::new();
    for (v, a) in row.values.iter().zip(answers) {
        match a {
            None => return Said::Waiting,
            Some(Err(why)) => return Said::Refused(why),
            Some(Ok(true)) => holding.push(v.clone()),
            Some(Ok(false)) => {}
        }
    }
    Said::Holds(holding)
}

/// ROW at one party: each level's word, the engine's own, and which level the levels give it from.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub levels: Vec<Said>,
    pub holds: Said,
    /// The most specific level with a value, `None` for the default.
    pub from: Option<usize>,
}

impl Resolved {
    /// The value the levels give, by the most specific one set, else the package's default.
    pub fn expected<'a>(&'a self, row: &'a Row) -> Option<&'a str> {
        match self.from {
            Some(i) => self.levels[i].one(),
            None => row.default.as_deref(),
        }
    }
}

/// ROW resolved for PARTY ("" for the levels that are not per party).
pub fn resolve(ask: Ask, row: &Row, party: &str) -> Resolved {
    let levels: Vec<Said> = row.levels.iter().map(|l| said(ask, row, &l.fact, l.per_party, if l.per_party { party } else { "" })).collect();
    let from = levels.iter().rposition(|s| s.one().is_some());
    let holds = if row.per_party && !party.is_empty() { said(ask, row, &row.fact, true, party) } else { Said::Holds(Vec::new()) };
    Resolved { levels, holds, from }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::plan::{rows, tests as plan};
    use super::*;

    fn row() -> Row {
        rows(&plan::read(&[plan::DESK, plan::TOP]), "t.top").remove(0)
    }

    /// A word with a quote, a brace or a backslash is written as the reader reads it back.
    #[test]
    fn a_question_reads_back_as_the_words_it_was_given() {
        let q = holds("D.F", &["a\"b{c}\\d", "x"]);
        let e = docket_v2::syntax::read_block(&format!("{{ {q} }}")).expect("the question reads");
        let shown = format!("{e:?}");
        assert!(shown.contains(r#"a\"b{c}\\d"#), "the word comes back whole: {shown}");
    }

    /// The most specific level set wins, the default stands when none is, and the engine's own word sits beside it.
    #[test]
    fn the_most_specific_level_set_gives_the_value_and_none_set_gives_the_default() {
        let r = row();
        let holding = [holds("D.Project-Mode", &["yolo"]), holds("D.Agent-Mode", &["w2", "safe"]), holds("D.Mode-Of", &["w1", "yolo"]), holds("D.Mode-Of", &["w2", "safe"])];
        let ask = |q: &str| Some(Ok(holding.iter().any(|h| h == q)));
        let w1 = resolve(&ask, &r, "w1");
        assert_eq!((w1.from, w1.expected(&r), w1.holds.one()), (Some(1), Some("yolo"), Some("yolo")), "the project's");
        let w2 = resolve(&ask, &r, "w2");
        assert_eq!((w2.from, w2.expected(&r), w2.holds.one()), (Some(2), Some("safe"), Some("safe")), "its own");
        let w3 = resolve(&ask, &r, "w3");
        assert_eq!(w3.holds, Said::Holds(vec![]), "nothing holds for a party the engine does not run");
        assert_eq!(resolve(&|_| Some(Ok(false)), &r, "w1").expected(&r), Some("safe"), "no level set: the default");
    }

    /// One answer not come makes the place wait, and a refusal is said with its reason.
    #[test]
    fn an_answer_not_come_waits_and_a_refusal_says_why() {
        let r = row();
        let some = |q: &str| (!q.contains("yolo")).then_some(Ok(false));
        assert_eq!(resolve(&some, &r, "w1").holds, Said::Waiting);
        let refused = resolve(&|_| Some(Err("no fact D.Mode-Of".to_string())), &r, "w1");
        assert_eq!(refused.holds, Said::Refused("no fact D.Mode-Of".into()));
    }
}
