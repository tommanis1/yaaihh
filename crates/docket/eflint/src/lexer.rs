//! Tokens of eFLINT.
//!
//! At each position the longest token wins; among tokens of the same length the order is:
//! single characters, keywords, integers, decimals, identifiers, capitalised identifiers,
//! character literals, string literals.

use crate::ast::{id, Id};

#[derive(Clone, PartialEq, Debug)]
pub enum Tok {
    Ch(char),
    Kw(&'static str),
    Int(i64),
    /// A decimal number; lexed, but no construct accepts it.
    Float,
    Id(Id),
    Alt(Id),
    Chr(char),
    Str(Id),
}

pub const KEYWORDS: &[&str] = &[
    "!?", "||", "&&", "<=", ">=", "..", "True", "False", "Sum", "==", "!=", "When", "Where", "Holds when",
    "Holds", "Present when", "Present", "Max", "Min", "Count", "Union", "Enabled", "Violated when", "Violated",
    "Atom", "String", "Int", "Time", "Current Time", "Exists", "Forall", "Foreach", "Force", "Extend", "Event",
    "Act", "Fact", "Physical", "Bool", "Var", "Function", "Invariant", "Predicate", "Duty", "Actor", "Holder",
    "Claimant", "Recipient", "Related to", "Conditioned by", "Creates", "Terminates", "Obfuscates",
    "Terminated by", "Created by", "With", "Identified by", "Derived from", "Derived externally", "Enforced by",
    "Syncs with", "Domain", "Do", "Placeholder", "For", "Not", "Open", "Closed", "?-", "?--", "#", "##", "###",
    "####", "#include", "#require",
];

pub const KEYCHARS: &[char] = &[
    '[', ']', '(', ')', '!', ',', '\'', '+', '-', '*', '/', '.', '=', '>', '<', ':', '?', '{', '}', '%', '~',
];

fn starts_with(s: &[char], at: usize, pat: &str) -> bool {
    let mut i = at;
    for c in pat.chars() {
        if i >= s.len() || s[i] != c {
            return false;
        }
        i += 1;
    }
    true
}

/// A word of an identifier: a lower-case letter, then lower-case letters, `_`, or `-` before a letter.
fn word_end(s: &[char], at: usize) -> Option<usize> {
    if at >= s.len() || !s[at].is_lowercase() {
        return None;
    }
    let mut i = at + 1;
    loop {
        if i < s.len() && (s[i].is_lowercase() || s[i] == '_') {
            i += 1;
        } else if i + 1 < s.len() && s[i] == '-' && s[i + 1].is_lowercase() {
            i += 2;
        } else {
            return Some(i);
        }
    }
}

/// Identifiers: words separated by spaces, `[any text]`, `<any text>` and `<<any text>>`.
fn ident_end(s: &[char], at: usize) -> Option<usize> {
    let internal = |c: char| !"]>+=".contains(c);
    if let Some(mut end) = word_end(s, at) {
        loop {
            let mut j = end;
            while j < s.len() && s[j] == ' ' {
                j += 1;
            }
            if j == end {
                return Some(end);
            }
            match word_end(s, j) {
                Some(e) => end = e,
                None => return Some(end),
            }
        }
    }
    match s.get(at) {
        Some('[') => {
            let mut i = at + 1;
            while i < s.len() && internal(s[i]) {
                i += 1;
            }
            (i < s.len() && s[i] == ']').then_some(i + 1)
        }
        Some('<') => {
            let mut i = at + 1;
            match s.get(i) {
                Some('<') => {
                    i += 1;
                    while i < s.len() && internal(s[i]) {
                        i += 1;
                    }
                    (i + 1 < s.len() && s[i] == '>' && s[i + 1] == '>').then_some(i + 2)
                }
                Some(c) if !"< =".contains(*c) => {
                    i += 1;
                    while i < s.len() && internal(s[i]) {
                        i += 1;
                    }
                    (i < s.len() && s[i] == '>').then_some(i + 1)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn digits_end(s: &[char], at: usize, ok: impl Fn(char) -> bool) -> Option<usize> {
    let mut i = at;
    while i < s.len() && ok(s[i]) {
        i += 1;
    }
    (i > at).then_some(i)
}

fn int_tok(s: &[char], at: usize) -> Option<(usize, i64)> {
    let mut best: Option<(usize, i64)> = None;
    if let Some(e) = digits_end(s, at, |c| c.is_ascii_digit()) {
        let mut v: i64 = 0;
        for c in &s[at..e] {
            v = v.wrapping_mul(10).wrapping_add(c.to_digit(10).unwrap() as i64);
        }
        best = Some((e, v));
    }
    for (p1, p2, base) in [("0x", "0X", 16u32), ("0o", "0O", 8), ("0b", "0B", 2)] {
        if starts_with(s, at, p1) || starts_with(s, at, p2) {
            if let Some(e) = digits_end(s, at + 2, |c| c.is_digit(base)) {
                if best.map_or(true, |(b, _)| e > b) {
                    let mut v: i64 = 0;
                    for c in &s[at + 2..e] {
                        v = v.wrapping_mul(base as i64).wrapping_add(c.to_digit(base).unwrap() as i64);
                    }
                    best = Some((e, v));
                }
            }
        }
    }
    best
}

fn float_end(s: &[char], at: usize) -> Option<usize> {
    let exponent = |i: usize| -> Option<usize> {
        if i < s.len() && (s[i] == 'e' || s[i] == 'E') {
            let mut j = i + 1;
            if j < s.len() && (s[j] == '+' || s[j] == '-') {
                j += 1;
            }
            digits_end(s, j, |c| c.is_ascii_digit())
        } else {
            None
        }
    };
    let mut start = at;
    if start < s.len() && (s[start] == '-' || s[start] == '+') {
        start += 1;
    }
    let pre = digits_end(s, start, |c| c.is_ascii_digit())?;
    if pre < s.len() && s[pre] == '.' {
        if let Some(frac) = digits_end(s, pre + 1, |c| c.is_ascii_digit()) {
            return Some(exponent(frac).unwrap_or(frac));
        }
    }
    exponent(pre)
}

/// `"` then characters other than `"` (or `\"`) then `"`: the longest such text.
fn string_end(s: &[char], at: usize) -> Option<usize> {
    if s.get(at) != Some(&'"') {
        return None;
    }
    let mut best = None;
    // positions reachable inside the literal
    let mut reach = vec![false; s.len() + 2];
    reach[at + 1] = true;
    for p in at + 1..s.len() {
        if !reach[p] {
            continue;
        }
        if s[p] == '"' {
            best = Some(p + 1);
        } else {
            reach[p + 1] = true;
        }
        if s[p] == '\\' && p + 1 < s.len() && s[p + 1] == '"' {
            reach[p + 2] = true;
        }
    }
    best
}

fn unescape(inner: &[char]) -> Result<String, String> {
    let mut out = String::new();
    let mut i = 0;
    while i < inner.len() {
        let c = inner[i];
        if c != '\\' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        let e = *inner.get(i).ok_or("bad escape at the end of a string literal")?;
        i += 1;
        match e {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            'a' => out.push('\u{7}'),
            'b' => out.push('\u{8}'),
            'f' => out.push('\u{c}'),
            'v' => out.push('\u{b}'),
            '0'..='9' => {
                let mut v = e.to_digit(10).unwrap();
                while i < inner.len() && inner[i].is_ascii_digit() {
                    v = v.saturating_mul(10).saturating_add(inner[i].to_digit(10).unwrap());
                    i += 1;
                }
                out.push(char::from_u32(v).ok_or("bad numeric escape in a string literal")?);
            }
            '&' => {}
            '\\' | '"' | '\'' => out.push(e),
            other => return Err(format!("unknown escape \\{} in a string literal", other)),
        }
    }
    Ok(out)
}

/// Line and column (both from 1) of the character at offset `at`.
pub fn line_col(src: &str, at: usize) -> (usize, usize) {
    let (mut line, mut col) = (1, 1);
    for c in src.chars().take(at) {
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// The tokens and, for each, the offset (in characters) where it starts. An error says line and column.
pub fn lex_at(src: &str) -> Result<(Vec<Tok>, Vec<usize>), String> {
    let s: Vec<char> = src.chars().collect();
    let mut toks = Vec::new();
    let mut starts = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if starts_with(&s, i, "{-") {
            let mut depth = 1;
            i += 2;
            while i < s.len() && depth > 0 {
                if starts_with(&s, i, "{-") {
                    depth += 1;
                    i += 2;
                } else if starts_with(&s, i, "-}") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if depth > 0 {
                break;
            }
            continue;
        }
        if starts_with(&s, i, "//") {
            while i < s.len() && s[i] != '\n' {
                i += 1;
            }
            i += 1;
            continue;
        }
        if s[i].is_whitespace() {
            i += 1;
            continue;
        }
        // candidates in priority order; a later one replaces an earlier one only when longer
        let mut best: Option<(usize, Tok)> = None;
        let mut offer = |end: usize, tok: Tok| {
            if best.as_ref().map_or(true, |(b, _)| end > *b) {
                best = Some((end, tok));
            }
        };
        if KEYCHARS.contains(&s[i]) {
            offer(i + 1, Tok::Ch(s[i]));
        }
        for k in KEYWORDS {
            if starts_with(&s, i, k) {
                offer(i + k.chars().count(), Tok::Kw(k));
            }
        }
        if let Some((e, v)) = int_tok(&s, i) {
            offer(e, Tok::Int(v));
        }
        if let Some(e) = float_end(&s, i) {
            offer(e, Tok::Float);
        }
        if let Some(e) = ident_end(&s, i) {
            offer(e, Tok::Id(id(&s[i..e].iter().collect::<String>())));
        }
        if s[i].is_uppercase() {
            let mut e = i + 1;
            while e < s.len() && (s[e].is_alphabetic() || s[e] == '_' || s[e].is_ascii_digit()) {
                e += 1;
            }
            offer(e, Tok::Alt(id(&s[i..e].iter().collect::<String>())));
        }
        if s[i] == '\'' {
            if i + 3 < s.len() && s[i + 1] == '\\' && s[i + 2] == '\'' && s[i + 3] == '\'' {
                offer(i + 4, Tok::Chr('\''));
            }
            if i + 2 < s.len() && s[i + 1] != '\'' && s[i + 2] == '\'' {
                offer(i + 3, Tok::Chr(s[i + 1]));
            }
        }
        if let Some(e) = string_end(&s, i) {
            let text = unescape(&s[i + 1..e - 1]).map_err(|m| {
                let (l, c) = line_col(src, i);
                format!("{}:{}: {}", l, c, m)
            })?;
            offer(e, Tok::Str(id(&text)));
        }
        match best {
            Some((e, t)) => {
                toks.push(t);
                starts.push(i);
                i = e;
            }
            None => {
                let rest: String = s[i..].iter().take_while(|c| !c.is_whitespace()).take(20).collect();
                let (l, c) = line_col(src, i);
                return Err(format!("{}:{}: no token starts with {:?}", l, c, rest));
            }
        }
    }
    Ok((toks, starts))
}
