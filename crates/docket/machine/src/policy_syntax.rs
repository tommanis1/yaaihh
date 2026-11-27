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
//! Extend Fact role                                // instances derived from others (eFLINT's `Derived from`),
//!   Derived from role(agent, "lead") When role(agent, "orchestrator")   // a bare name a variable the When binds;
//!                                                 // also as body lines of the `Fact` that declares the type
//!
//! Act send-mail                                   // §3: an eFLINT act (norms.rs compiles it, GPCE translation)
//!   Actor agent                                     //     its fields: actor, recipient, one per `Related to` type
//!   Recipient agent
//!   Related to thread
//!   Holds when not role(recipient, "consultant")    //     each `Holds when` derives the act (they are alternatives)
//!   Conditioned by actor != recipient               //     each `Conditioned by` must hold for it to be enabled
//!
//! Event given                                     // eFLINT's event: a record line of that `ev`, done as it is
//!   Related to to                                 //     the line's fields its effects read
//!   Creates working(to)                           //     a fact it makes, its arguments those fields or literals
//! Event turn_ended
//!   Related to agent
//!   Terminates working(agent)                     //     a fact it ends
//! Event act                                       // one `ev` for many kinds of line: each effect's own `When`
//!   Related to is, phase, id, verb, packages, secs
//!   Terminates (Foreach int: job-last(verb, packages, int)) When is == "job" and phase == "end"  // each value
//!   Creates job-last(verb, packages, secs) When is == "job" and phase == "end"   // compares the line's fields
//! ```
//!
//! Comments run from `//` to the line's end. Names may contain `-` (`model-for`, `quota-left`). Of eFLINT's norms
//! `Act` with `Holds when` and `Conditioned by` and `Event` with `Creates` and `Terminates` are compiled so far; an
//! act's effects, `Syncs with`, `Duty`, `Invariant` and `Predicate` are refused with their line, so nothing is silently
//! dropped.

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
    /// Its doc: the `///` lines directly above it (paper-research N-101 p656), as a kind's or a call's is read.
    pub doc: String,
    /// Its comment: the `//` lines directly above it, not `///`, as one sentence (paper-research N-101 p1091: a
    /// decision's `why` is the rule's own comment, for its reader).
    pub comment: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DeclKind {
    /// `Fact NAME Identified by T1 * T2 ...`, and the `Derived from` lines of its body.
    Fact { name: String, ids: Vec<String>, derived: Vec<Derivation> },
    /// `Extend Fact NAME` and its `Derived from` lines: more ways an instance of a fact type declared elsewhere (the
    /// prelude's `role`) holds (paper-research N-101 p512, p525).
    Extend { name: String, derived: Vec<Derivation> },
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
    /// `Event NAME`: a record line whose `ev` is NAME, done as it is (the engine page's move 6, piece 1). Its fields
    /// are the line's own, named by `Related to`; it has no actor and no permission, as nobody may refuse a line that
    /// was written. Its `Creates` and `Terminates` lines in order.
    Event { name: String, fields: Vec<String>, effects: Vec<Effect> },
}

/// `Creates FACT(A1, ...) [When CONDITION]` or `Terminates FACT(A1, ...) [When CONDITION]` in an event's body: each
/// argument a string, a number or one of the event's fields. A `Terminates` may bind names with eFLINT's `Foreach`,
/// `Terminates (Foreach V: FACT(..., V, ...))`, and then ends every instance the other arguments match, whatever V is
/// (a value that changes is replaced so: docket-edge5 N-1446 p3123, 2; paper-research p3127: eFLINT's own form, as
/// eflint-rs parses it); such an argument is kept as `Expr::Any`. Its
/// CONDITION compares the event's fields with each other or with strings and numbers, and it is done only when that
/// holds: one `ev` written for many kinds of line (an `act`'s `is` and `phase`) tells them apart so (p3123, 1). It
/// reads no fact: a line says it or does not.
#[derive(Clone, Debug, PartialEq)]
pub struct Effect {
    pub at: At,
    pub creates: bool,
    pub fact: String,
    pub args: Vec<Expr>,
    pub when: Cond,
    pub text: String,
}

/// `Derived from FACT(A1, ...) [When CONDITION]`: an instance of FACT holds whenever CONDITION does. Each argument is
/// a string, a number or a bare name, a variable that a fact of the condition (not negated) binds.
#[derive(Clone, Debug, PartialEq)]
pub struct Derivation {
    pub at: At,
    pub args: Vec<Expr>,
    pub when: Cond,
    pub text: String,
}

/// The `Derived from` lines of fact NAME's body; any other body line is refused, so nothing under a fact is silently
/// dropped (paper-research N-101 p512: a `Fact` whose body lines were not read loaded and did nothing).
fn derivations(at: &At, name: &str, body: &[(usize, String)]) -> Result<Vec<Derivation>, String> {
    let mut out = Vec::new();
    for (l, line) in body {
        let Some(rest) = line.strip_prefix("Derived from ") else {
            return Err(format!("line {l}: `{line}`: a fact's body takes only `Derived from {name}(...) When CONDITION` lines"));
        };
        let (inst, when) = match rest.split_once(" When ") {
            Some((i, w)) => (i.trim(), cond(w)?),
            None => (rest.trim(), Vec::new()),
        };
        let args = match expr(inst)? {
            Expr::App(f, args) if f == name => args,
            _ => return Err(format!("line {l}: `Derived from {inst}`: it derives an instance of {name}, written `{name}(...)`")),
        };
        if let Some(a) = args.iter().find(|a| !matches!(a, Expr::Str(_) | Expr::Int(_) | Expr::Field(_))) {
            return Err(format!("line {l}: `{inst}`: an argument is a string, a number or a name, not {a:?}"));
        }
        out.push(Derivation { at: At { file: at.file.clone(), line: *l }, args, when, text: line.clone() });
    }
    Ok(out)
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
    let all: Vec<&str> = text.lines().collect();
    groups
        .into_iter()
        .map(|(line, lines)| {
            let doc = crate::syntax::doc_above(&all, line - 1);
            let comment = comment_above(&all, line - 1);
            let at = At { file: file.into(), line };
            let text = lines.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>().join("\n");
            decl(&at, &lines).map(|kind| Decl { at: at.clone(), kind, text, doc, comment }).map_err(|e| format!("{at}: {e}"))
        })
        .collect()
}

/// The `//` comment directly above line I of LINES (0-based): its lines that are not docs (`///`), joined by spaces.
fn comment_above(lines: &[&str], i: usize) -> String {
    let mut j = i.min(lines.len());
    while j > 0 && lines[j - 1].trim_start().starts_with("//") && !lines[j - 1].trim_start().starts_with("///") {
        j -= 1;
    }
    lines[j..i.min(lines.len())].iter().map(|l| l.trim_start().trim_start_matches("//").trim()).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ")
}

/// The names of LINE (one line of a policy file, 1-based columns not counted: 0-based, in chars) as the parser reads
/// them, outside strings and the comment: each name, its column, and whether a `(` follows it (a fact or an act
/// applied, `role(actor, "lead")`). What the language server finds uses and declarations by (N-101 p646); the parser
/// itself works on whole lines.
pub fn words(line: &str) -> Vec<(String, usize, bool)> {
    let cs: Vec<char> = strip_comment(line).chars().collect();
    let (mut out, mut i, mut in_str) = (Vec::new(), 0, false);
    while i < cs.len() {
        let c = cs[i];
        if c == '"' {
            in_str = !in_str;
            i += 1;
        } else if !in_str && (c.is_ascii_alphabetic() || c == '_') {
            let start = i;
            while i < cs.len() && (cs[i].is_ascii_alphanumeric() || cs[i] == '-' || cs[i] == '_') {
                i += 1;
            }
            let mut j = i;
            while j < cs.len() && cs[j] == ' ' {
                j += 1;
            }
            out.push((cs[start..i].iter().collect(), start, cs.get(j) == Some(&'(')));
        } else {
            i += 1;
        }
    }
    out
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
            Ok(DeclKind::Fact { name: name.into(), ids, derived: derivations(at, name, body)? })
        }
        "Extend" => {
            let name = head["Extend".len()..].trim().strip_prefix("Fact ").ok_or("`Extend Fact NAME`, then its `Derived from` lines")?.trim();
            ident(name)?;
            let derived = derivations(at, name, body)?;
            if derived.is_empty() {
                return Err(format!("`Extend Fact {name}` has no `Derived from` line"));
            }
            Ok(DeclKind::Extend { name: name.into(), derived })
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
        "Event" => {
            let name = head["Event".len()..].trim();
            ident(name)?;
            let (mut fields, mut effects): (Vec<String>, Vec<Effect>) = (Vec::new(), Vec::new());
            for (l, line) in body {
                let at = At { file: at.file.clone(), line: *l };
                if let Some(keys) = line.strip_prefix("Related to ") {
                    for k in keys.split(',').map(str::trim) {
                        ident(k)?;
                        fields.push(k.to_string());
                    }
                } else if let Some((creates, rest)) = line.strip_prefix("Creates ").map(|i| (true, i)).or_else(|| line.strip_prefix("Terminates ").map(|i| (false, i))) {
                    let (inst, when) = match rest.split_once(" When ") {
                        Some((i, w)) => (i, cond(w)?),
                        None => (rest, Vec::new()),
                    };
                    // eFLINT's `(Foreach V1, V2: FACT(...))`: the names it binds stand for any value
                    let (bound, inst): (Vec<&str>, &str) = match inst.trim().strip_prefix("(Foreach ").and_then(|r| r.strip_suffix(')')) {
                        Some(r) => {
                            let (vs, e) = r.split_once(':').ok_or(format!("line {l}: `{line}`: `(Foreach V: FACT(...))`"))?;
                            (vs.split(',').map(str::trim).collect(), e.trim())
                        }
                        None => (Vec::new(), inst),
                    };
                    if !bound.is_empty() && creates {
                        return Err(format!("line {l}: `{line}`: `Foreach` ends every value, in a Terminates only; a fact made has each of its values"));
                    }
                    if let Some(v) = bound.iter().find(|v| fields.iter().any(|f| f == *v)) {
                        return Err(format!("line {l}: `{line}`: `{v}` is a field of Event {name}; a `Foreach` binds a name of its own"));
                    }
                    let Expr::App(fact, args) = expr(inst.trim())? else {
                        return Err(format!("line {l}: `{line}`: an effect names an instance, written `fact(...)`"));
                    };
                    let args: Vec<Expr> = args.into_iter().map(|a| match a {
                        Expr::Field(f) if bound.contains(&f.as_str()) => Expr::Any,
                        a => a,
                    }).collect();
                    let field = |a: &Expr| -> Result<(), String> {
                        match a {
                            Expr::Str(_) | Expr::Int(_) => Ok(()),
                            Expr::Field(f) if fields.contains(f) => Ok(()),
                            Expr::Field(f) => Err(format!("line {l}: `{line}`: `{f}` is not a field of Event {name} (`Related to` names them, above)")),
                            other => Err(format!("line {l}: `{line}`: an argument is a string, a number or a field, not {other:?}")),
                        }
                    };
                    for a in &args {
                        match a {
                            Expr::Any if !bound.is_empty() => {}
                            a => field(a)?,
                        }
                    }
                    for lit in &when {
                        match lit {
                            Lit::Cmp(a, _, b) => {
                                field(a)?;
                                field(b)?;
                            }
                            _ => return Err(format!("line {l}: `{line}`: an effect's `When` compares the line's fields (`phase == \"end\"`); it reads no fact")),
                        }
                    }
                    effects.push(Effect { at, creates, fact, args, when, text: line.clone() });
                } else {
                    return Err(format!("line {l}: `{line}`: an Event takes Related to, Creates and Terminates lines only"));
                }
            }
            Ok(DeclKind::Event { name: name.into(), fields, effects })
        }
        "Duty" | "Invariant" | "Predicate" => {
            Err(format!("eFLINT norms (`{word}`) are not compiled yet in this slice; keep them out of the policy for now"))
        }
        _ => Err(format!("unknown declaration `{word}` (Fact, Extend Fact, Decision, Prefer, Only, Judge, Act, Event)")),
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

    /// An effect's `When` compares the line's fields, and a Terminates' `Foreach` ends any value of the names it binds
    /// (docket-edge5 N-1446 p3123; eFLINT's form, p3127): one `ev` for many kinds of line is told apart so, and a value
    /// that changes is replaced. `Foreach` in a Creates or over a field, `_`, a `When` that reads a fact, and a field
    /// not `Related to` are refused with their line.
    #[test]
    fn an_effects_when_and_foreach_parse() {
        let src = "Event act\n  Related to is, phase, verb, packages, secs\n  Terminates (Foreach int: job-last(verb, packages, int)) When is == \"job\" and phase == \"end\"\n  Creates job-last(verb, packages, secs) When is == \"job\" and phase == \"end\"\n  Creates seen(verb)\n";
        let d = parse("x", src).unwrap();
        let DeclKind::Event { effects, .. } = &d[0].kind else { panic!("{:?}", d[0]) };
        let f = |x: &str| Expr::Field(x.into());
        let when = vec![Lit::Cmp(f("is"), Op::Eq, Expr::Str("job".into())), Lit::Cmp(f("phase"), Op::Eq, Expr::Str("end".into()))];
        assert_eq!((effects[0].creates, &effects[0].args, &effects[0].when), (false, &vec![f("verb"), f("packages"), Expr::Any], &when));
        assert_eq!((effects[1].creates, &effects[1].args, &effects[1].when), (true, &vec![f("verb"), f("packages"), f("secs")], &when));
        assert!(effects[2].when.is_empty(), "no When: always");
        let refused = |body: &str, says: &str| {
            let e = parse("x", &format!("Event act\n  Related to is, id\n  {body}\n")).unwrap_err();
            assert!(e.contains(says), "{body}: {e}");
        };
        refused("Creates (Foreach int: open(id, int))", "in a Terminates only");
        refused("Terminates (Foreach id: open(id))", "a `Foreach` binds a name of its own");
        refused("Terminates open(id, _)", "an argument is a string, a number or a field");
        refused("Terminates open(id) When working(id)", "it reads no fact");
        refused("Terminates open(id) When phase == \"end\"", "`phase` is not a field of Event act");
    }

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
        // nothing under a fact is silently dropped (paper-research N-101 p512): a body line it does not read is refused
        assert!(parse("x", "Fact role Identified by agent * String\n  Holds when agent(x)\n").unwrap_err().contains("x:1: line 2: `Holds when agent(x)`: a fact's body takes only `Derived from role(...)"));
        assert!(parse("x", "Extend Fact role\n").unwrap_err().contains("no `Derived from` line"));
        assert!(parse("x", "Extend Fact role\n  Derived from agent(a) When role(a, \"dev\")\n").unwrap_err().contains("derives an instance of role"));
        let d = parse("x", "Fact lead Identified by agent\n  Derived from lead(a) When role(a, \"orchestrator\")\n").unwrap();
        assert!(matches!(&d[0].kind, DeclKind::Fact { derived, .. } if derived.len() == 1 && derived[0].at.line == 2));
        assert!(parse("x", "Act staff\n  Actor agent\n").unwrap_err().contains("no `Holds when`"));
    }

    /// A fact, an act and a decision keep their docs, apart from `//` comments (paper-research N-101 p656).
    #[test]
    fn a_declarations_doc_is_kept() {
        let text = "// history\n/// who holds a role\nFact role Identified by agent * String\n\n/// which model a step runs on\n/// for a made agent too\nDecision model-for\n  Options model\n+tier(\"m\", 1).\n";
        let d = parse("x", text).unwrap();
        let docs: Vec<&str> = d.iter().map(|d| d.doc.as_str()).collect();
        assert_eq!(docs, vec!["who holds a role", "which model a step runs on\nfor a made agent too", ""]);
        // a `//` comment is not a doc, and is kept apart: the `//` lines directly above, as one sentence
        let comments: Vec<&str> = d.iter().map(|d| d.comment.as_str()).collect();
        assert_eq!(comments, vec!["", "", ""]);
        let d = parse("x", "// header\n\n// This build carried only your piece,\n// so the red is yours.\nOnly for whose-red When call.n == 1:\n  option == \"Piece\"\n").unwrap();
        assert_eq!(d[0].comment, "This build carried only your piece, so the red is yours.");
    }
}
