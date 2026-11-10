//! The policy language's surface (vault designs/policy-language-draft-2026-11-08.md, "eFLINT/A"): the part of it the
//! first slice compiles. A file is a list of declarations, each a head line and its indented body lines:
//!
//! ```text
//! Fact family Identified by model * String          // §2: a fact type and its identifier types
//! Decision model-for                                // §4: a choice point
//!   Options model                                   //     its options: the instances of a fact type
//!   Requires price(option) >= 0                     //     each option must satisfy this, or is excluded
//! Prefer for model-for When call.task == "recognise-question":    // §5: a ranked preference
//!   family(option, "codex") > family(option, "claude")
//! Prefer for model-for:
//!   minimise price(option)
//! Only for model-for When call.agent == "orch":     // a hard rule, written as such
//!   family(option, "claude")
//! Judge staffing Using "guidelines/staffing.md" When ties          // §6 (read, not yet run)
//! +tier("claude-haiku-5-5", 1).                   // an instance of a fact type (eFLINT's `+`), one per line:
//!                                                 // the installer's own facts, such as an order of models
//!
//! Act send-mail                                   // §3: an eFLINT act (norms.rs compiles it, GPCE translation)
//!   Actor agent                                     //     its fields: actor, recipient, one per `Related to` type
//!   Recipient agent
//!   Related to thread
//!   Holds when not role(recipient, "consultant")    //     each `Holds when` derives the act (they are alternatives)
//!   Conditioned by actor != recipient               //     each `Conditioned by` must hold for it to be enabled
//! ```
//!
//! Comments run from `//` to the line's end. Names may contain `-` (`model-for`, `quota-left`). Of eFLINT's norms only
//! `Act` with `Holds when` and `Conditioned by` is compiled so far; its effects (`Creates`, `Terminates`, `Syncs with`),
//! `Duty`, `Event`, `Invariant` and `Predicate` are refused with their line, so nothing is silently dropped.

use std::fmt;

/// Where a declaration or an expression came from, for provenance and errors.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct At {
    pub file: String,
    pub line: usize,
}

impl fmt::Display for At {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.file, self.line)
    }
}

/// A value: the option being judged, a field of the call the decision is for, a literal, or a fact read as a function
/// (`price(option)` is the P of `price(option, P)`).
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Option,
    /// `_`: any value, as a fact's argument only (`harness(option, _)`: some harness runs the option).
    Any,
    Call(String),
    /// A bare name: an act's field (`actor`, `recipient`, `thread`) in a norm's condition.
    Field(String),
    Str(String),
    Int(i64),
    App(String, Vec<Expr>),
}

/// A comparison operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// One conjunct of a condition: a fact that holds (`family(option, "codex")`), its negation, or a comparison.
#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    Holds(String, Vec<Expr>),
    Not(String, Vec<Expr>),
    Cmp(Expr, Op, Expr),
}

/// A condition: its conjuncts (`and`), all of which must hold.
pub type Cond = Vec<Lit>;

/// What a preference ranks by.
#[derive(Clone, Debug, PartialEq)]
pub enum Rank {
    /// `A > B > ...`: an option satisfying an earlier condition beats one satisfying only a later one; one satisfying
    /// none comes last. A single condition is `A > (anything else)`.
    Order(Vec<Cond>),
    /// `minimise E`, `maximise E`.
    Minimise(Expr),
    Maximise(Expr),
}

/// A declaration and where it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Decl {
    pub at: At,
    pub kind: DeclKind,
    /// The source text of the declaration, as written (head and body), for provenance.
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DeclKind {
    /// `Fact NAME Identified by T1 * T2 ...`.
    Fact { name: String, ids: Vec<String> },
    /// `Decision NAME`, `Options FACT` (or `Options a | b | c`, listed in place), `Requires COND` lines (each with its
    /// line).
    Decision { name: String, options: String, requires: Vec<(At, Cond, String)> },
    /// `Prefer for DECISION [When COND]:` and its ranking.
    Prefer { decision: String, when: Cond, rank: Rank },
    /// `Only for DECISION [When COND]:` and the condition every chosen option must satisfy.
    Only { decision: String, when: Cond, cond: Cond },
    /// `Judge DECISION Using "FILE" When ties`.
    Judge { decision: String, using: String },
    /// `+FACT(V1, V2, ...).`: an instance of fact type FACT, its values strings and integers (vault designs/
    /// teams-and-tiers-2026-11-10.md §2: the installer states facts coordd does not know, such as an order of models).
    Instance { fact: String, args: Vec<Expr> },
    /// `Act NAME` (§3): its fields in order (field name, fact type), its `Holds when` and `Conditioned by` lines.
    Act { name: String, fields: Vec<(String, String)>, holds: Vec<(At, Cond, String)>, conditions: Vec<(At, Cond, String)> },
}

/// Parse FILE's TEXT.
pub fn parse(file: &str, text: &str) -> Result<Vec<Decl>, String> {
    // declarations: a non-indented line and the indented lines after it
    let mut groups: Vec<(usize, Vec<(usize, String)>)> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = strip_comment(raw);
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            match groups.last_mut() {
                Some((_, body)) => body.push((i + 1, line.trim().to_string())),
                None => return Err(format!("{file}:{}: an indented line before any declaration", i + 1)),
            }
        } else {
            groups.push((i + 1, vec![(i + 1, line.trim().to_string())]));
        }
    }
    groups
        .into_iter()
        .map(|(line, lines)| {
            let at = At { file: file.into(), line };
            let text = lines.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>().join("\n");
            decl(&at, &lines).map(|kind| Decl { at: at.clone(), kind, text }).map_err(|e| format!("{at}: {e}"))
        })
        .collect()
}

fn strip_comment(line: &str) -> &str {
    // `//` outside a string literal
    let mut in_str = false;
    let b = line.as_bytes();
    for i in 0..b.len() {
        match b[i] {
            b'"' => in_str = !in_str,
            b'/' if !in_str && b.get(i + 1) == Some(&b'/') => return &line[..i],
            _ => {}
        }
    }
    line
}

fn decl(at: &At, lines: &[(usize, String)]) -> Result<DeclKind, String> {
    let head = &lines[0].1;
    let body = &lines[1..];
    let word = head.split_whitespace().next().unwrap_or("");
    if let Some(rest) = head.strip_prefix('+') {
        if !body.is_empty() {
            return Err("an instance `+FACT(...).` is one line".into());
        }
        let rest = rest.trim().strip_suffix('.').ok_or("an instance `+FACT(...)` ends with `.`")?;
        return match expr(rest)? {
            Expr::App(fact, args) if args.iter().all(|a| matches!(a, Expr::Str(_) | Expr::Int(_))) => Ok(DeclKind::Instance { fact, args }),
            _ => Err(format!("`+{rest}.`: an instance names a fact type and gives string and integer values")),
        };
    }
    match word {
        "Fact" => {
            let rest = head["Fact".len()..].trim();
            let (name, ids) = match rest.split_once("Identified by") {
                Some((n, ids)) => (n.trim(), ids.split('*').map(|t| t.trim().to_string()).collect()),
                None => (rest, vec!["String".to_string()]),
            };
            ident(name)?;
            Ok(DeclKind::Fact { name: name.into(), ids })
        }
        "Decision" => {
            let name = head["Decision".len()..].trim();
            ident(name)?;
            let mut options = None;
            let mut requires = Vec::new();
            for (l, line) in body {
                if let Some(o) = line.strip_prefix("Options ") {
                    options = Some(o.trim().to_string());
                } else if let Some(r) = line.strip_prefix("Requires ") {
                    requires.push((At { file: at.file.clone(), line: *l }, cond(r)?, line.clone()));
                } else {
                    return Err(format!("line {l}: a Decision takes `Options FACT` and `Requires CONDITION` lines"));
                }
            }
            Ok(DeclKind::Decision { name: name.into(), options: options.ok_or("a Decision needs an `Options FACT` line")?, requires })
        }
        "Prefer" | "Only" => {
            let rest = head[word.len()..].trim();
            let rest = rest.strip_prefix("for ").ok_or(format!("`{word} for DECISION [When CONDITION]:`"))?;
            let rest = rest.strip_suffix(':').ok_or(format!("`{word} for ...` ends with `:`"))?;
            let (decision, when) = match rest.split_once(" When ") {
                Some((d, w)) => (d.trim(), cond(w)?),
                None => (rest.trim(), Vec::new()),
            };
            ident(decision)?;
            let text = body.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>().join(" ");
            if text.is_empty() {
                return Err(format!("`{word} for {decision}` has no body"));
            }
            if word == "Only" {
                return Ok(DeclKind::Only { decision: decision.into(), when, cond: cond(&text)? });
            }
            let rank = if let Some(e) = text.strip_prefix("minimise ") {
                Rank::Minimise(expr(e)?)
            } else if let Some(e) = text.strip_prefix("maximise ") {
                Rank::Maximise(expr(e)?)
            } else {
                Rank::Order(split_top(&text, '>').iter().map(|c| cond(c)).collect::<Result<_, _>>()?)
            };
            Ok(DeclKind::Prefer { decision: decision.into(), when, rank })
        }
        "Judge" => {
            let rest = head["Judge".len()..].trim();
            let (decision, rest) = rest.split_once(" Using ").ok_or("`Judge DECISION Using \"FILE\" When ties`")?;
            let using = rest.split('"').nth(1).ok_or("the guideline file is a string")?;
            Ok(DeclKind::Judge { decision: decision.trim().into(), using: using.into() })
        }
        "Act" => {
            let name = head["Act".len()..].trim();
            ident(name)?;
            let (mut fields, mut holds, mut conditions) = (Vec::new(), Vec::new(), Vec::new());
            for (l, line) in body {
                let at = At { file: at.file.clone(), line: *l };
                if let Some(ty) = line.strip_prefix("Actor ") {
                    ident(ty.trim())?;
                    fields.push(("actor".to_string(), ty.trim().to_string()));
                } else if let Some(ty) = line.strip_prefix("Recipient ") {
                    ident(ty.trim())?;
                    fields.push(("recipient".to_string(), ty.trim().to_string()));
                } else if let Some(tys) = line.strip_prefix("Related to ") {
                    // eFLINT names a related field by its type
                    for ty in tys.split(',').map(str::trim) {
                        ident(ty)?;
                        fields.push((ty.to_string(), ty.to_string()));
                    }
                } else if let Some(c) = line.strip_prefix("Holds when ") {
                    holds.push((at, cond(c)?, line.clone()));
                } else if let Some(c) = line.strip_prefix("Conditioned by ") {
                    conditions.push((at, cond(c)?, line.clone()));
                } else if ["Creates", "Terminates", "Obfuscates", "Syncs with", "Derived from"].iter().any(|k| line.starts_with(k)) {
                    return Err(format!("line {l}: `{line}`: an act's effects and `Derived from` are not compiled yet"));
                } else {
                    return Err(format!("line {l}: an Act takes Actor, Recipient, Related to, Holds when and Conditioned by lines"));
                }
            }
            if holds.is_empty() {
                return Err(format!("Act {name} has no `Holds when` line: say when its instances exist (`Holds when true` for all of them)"));
            }
            let mut seen = std::collections::HashSet::new();
            if let Some((f, _)) = fields.iter().find(|(f, _)| !seen.insert(f.clone())) {
                return Err(format!("Act {name} has two fields named `{f}`"));
            }
            Ok(DeclKind::Act { name: name.into(), fields, holds, conditions })
        }
        "Duty" | "Event" | "Invariant" | "Predicate" => {
            Err(format!("eFLINT norms (`{word}`) are not compiled yet in this slice; keep them out of the policy for now"))
        }
        _ => Err(format!("unknown declaration `{word}` (Fact, Decision, Prefer, Only, Judge)")),
    }
}

fn ident(s: &str) -> Result<(), String> {
    let ok = !s.is_empty() && s.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok { Ok(()) } else { Err(format!("`{s}` is not a name")) }
}

/// Split S at top-level SEP characters (outside parentheses and strings); `>=` is not a separator.
fn split_top(s: &str, sep: char) -> Vec<String> {
    let (mut out, mut cur, mut depth, mut in_str) = (Vec::new(), String::new(), 0i32, false);
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '"' => in_str = !in_str,
            '(' if !in_str => depth += 1,
            ')' if !in_str => depth -= 1,
            _ => {}
        }
        if c == sep && !in_str && depth == 0 && chars.get(i + 1) != Some(&'=') {
            out.push(std::mem::take(&mut cur).trim().to_string());
        } else {
            cur.push(c);
        }
    }
    out.push(cur.trim().to_string());
    out
}

/// A condition: conjuncts joined by ` and `.
pub fn cond(s: &str) -> Result<Cond, String> {
    if s.trim() == "true" {
        return Ok(Vec::new());
    }
    split_words(s, " and ").iter().map(|l| lit(l)).collect()
}

fn split_words(s: &str, sep: &str) -> Vec<String> {
    // ` and ` outside strings
    let mut out = Vec::new();
    let (mut start, mut in_str) = (0, false);
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'"' {
            in_str = !in_str;
        } else if !in_str && s[i..].starts_with(sep) {
            out.push(s[start..i].trim().to_string());
            i += sep.len();
            start = i;
            continue;
        }
        i += 1;
    }
    out.push(s[start..].trim().to_string());
    out
}

fn lit(s: &str) -> Result<Lit, String> {
    let s = s.trim();
    for (tok, op) in [("==", Op::Eq), ("!=", Op::Ne), ("<=", Op::Le), (">=", Op::Ge), ("<", Op::Lt), (">", Op::Gt)] {
        if let Some((a, b)) = split_once_top(s, tok) {
            return Ok(Lit::Cmp(expr(a)?, op, expr(b)?));
        }
    }
    let (neg, s) = match s.strip_prefix("not ") {
        Some(r) => (true, r.trim()),
        None => (false, s),
    };
    match expr(s)? {
        Expr::App(f, args) => Ok(if neg { Lit::Not(f, args) } else { Lit::Holds(f, args) }),
        e => Err(format!("`{s}` is a value ({e:?}), not a condition")),
    }
}

/// S split at the first top-level TOK (outside parentheses and strings).
fn split_once_top<'a>(s: &'a str, tok: &str) -> Option<(&'a str, &'a str)> {
    let (mut depth, mut in_str) = (0i32, false);
    let b = s.as_bytes();
    for i in 0..b.len() {
        match b[i] {
            b'"' => in_str = !in_str,
            b'(' if !in_str => depth += 1,
            b')' if !in_str => depth -= 1,
            _ => {}
        }
        if !in_str && depth == 0 && s[i..].starts_with(tok) {
            // `<` and `>` are not the start of `<=`, `>=`, and `=` is not inside `==`/`!=`
            let next = s[i + tok.len()..].chars().next();
            if tok.len() == 1 && next == Some('=') {
                continue;
            }
            if tok.len() == 1 && i > 0 && matches!(b[i - 1], b'!' | b'=' | b'<' | b'>') {
                continue;
            }
            return Some((&s[..i], &s[i + tok.len()..]));
        }
    }
    None
}

/// A value expression.
pub fn expr(s: &str) -> Result<Expr, String> {
    let s = s.trim();
    if s == "option" {
        return Ok(Expr::Option);
    }
    if s == "_" {
        return Ok(Expr::Any);
    }
    if let Some(f) = s.strip_prefix("call.") {
        ident(f)?;
        return Ok(Expr::Call(f.into()));
    }
    if let Some(inner) = s.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        return Ok(Expr::Str(inner.into()));
    }
    if let Ok(n) = s.parse::<i64>() {
        return Ok(Expr::Int(n));
    }
    if let Some((f, rest)) = s.split_once('(') {
        let args = rest.strip_suffix(')').ok_or(format!("`{s}`: unclosed ("))?;
        ident(f.trim())?;
        let args = if args.trim().is_empty() { Vec::new() } else { split_top(args, ',').iter().map(|a| expr(a)).collect::<Result<_, _>>()? };
        return Ok(Expr::App(f.trim().into(), args));
    }
    if ident(s).is_ok() {
        return Ok(Expr::Field(s.into()));
    }
    Err(format!("`{s}` is not a value (option, call.FIELD, a field name, \"text\", a number or fact(args))"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slices_preferences_parse() {
        let src = "Prefer for model-for When call.task == \"recognise-question\":\n  family(option, \"codex\") > family(option, \"claude\")  // codex first\nPrefer for model-for:\n  minimise price(option)\n";
        let d = parse("p.eflint", src).unwrap();
        assert_eq!(d.len(), 2);
        let DeclKind::Prefer { decision, when, rank: Rank::Order(cs) } = &d[0].kind else { panic!("{:?}", d[0]) };
        assert_eq!(decision, "model-for");
        assert_eq!(when, &vec![Lit::Cmp(Expr::Call("task".into()), Op::Eq, Expr::Str("recognise-question".into()))]);
        assert_eq!(cs[1], vec![Lit::Holds("family".into(), vec![Expr::Option, Expr::Str("claude".into())])]);
        assert_eq!(d[1].kind, DeclKind::Prefer { decision: "model-for".into(), when: vec![], rank: Rank::Minimise(Expr::App("price".into(), vec![Expr::Option])) });
        assert_eq!(d[1].at.line, 3);
    }

    #[test]
    fn requires_with_a_comparison_and_norms_are_refused() {
        let d = parse("x", "Decision model-for\n  Options model\n  Requires price(option) >= 0\n").unwrap();
        let DeclKind::Decision { requires, .. } = &d[0].kind else { panic!() };
        assert_eq!(requires[0].1, vec![Lit::Cmp(Expr::App("price".into(), vec![Expr::Option]), Op::Ge, Expr::Int(0))]);
        assert!(parse("x", "Duty pay\n  Holder agent\n").unwrap_err().contains("not compiled yet"));
        assert!(parse("x", "Act staff\n  Actor agent\n").unwrap_err().contains("no `Holds when`"));
    }
}
