//! Text to tokens: the reader works on these, never on characters (spec 2, 5.1).

use std::fmt;

/// A place in a text: line and column, both from 1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct At {
    pub line: u32,
    pub col: u32,
}

impl fmt::Display for At {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    /// A name: letters, digits, `_`, and `-` between two letters or digits (`Owes-Green`).
    Name(String),
    Int(i64),
    /// A text literal as written between its quotes, escapes read; `{{ … }}` is left in it.
    Text(String),
    /// One of the marks: `( ) { } [ ] , ; : . .. ... | => <- -> = == != < <= > >= + - * / % && || ! _`.
    Mark(&'static str),
    /// The end of the text.
    End,
}

impl fmt::Display for Tok {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tok::Name(n) => write!(f, "`{n}`"),
            Tok::Int(n) => write!(f, "{n}"),
            Tok::Text(_) => write!(f, "a text"),
            Tok::Mark(m) => write!(f, "`{m}`"),
            Tok::End => write!(f, "the end"),
        }
    }
}

/// A token and where it stands. A `+` in the first column opens a module's own fact (5.5); elsewhere it adds.
#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub at: At,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexError {
    pub at: At,
    pub why: String,
}

const MARKS: [&str; 32] = [
    "...", "..", "=>", "<-", "->", "==", "!=", "<=", ">=", "&&", "||", "(", ")", "{", "}", "[", "]", ",", ";", ":", ".", "|", "=", "<", ">", "+", "-", "*", "/", "%", "!", "_",
];

pub fn lex(text: &str) -> Result<Vec<Token>, LexError> {
    let cs: Vec<char> = text.chars().collect();
    let (mut i, mut line, mut col) = (0usize, 1u32, 1u32);
    let mut out = Vec::new();
    let step = |i: &mut usize, line: &mut u32, col: &mut u32, n: usize| {
        for _ in 0..n {
            if cs[*i] == '\n' {
                *line += 1;
                *col = 1;
            } else {
                *col += 1;
            }
            *i += 1;
        }
    };
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            step(&mut i, &mut line, &mut col, 1);
            continue;
        }
        if c == '/' && cs.get(i + 1) == Some(&'/') {
            while i < cs.len() && cs[i] != '\n' {
                step(&mut i, &mut line, &mut col, 1);
            }
            continue;
        }
        let at = At { line, col };
        let tok = if c.is_ascii_alphabetic() || (c == '_' && cs.get(i + 1).is_some_and(|d| d.is_ascii_alphanumeric() || *d == '_')) {
            let b = i;
            let mut e = i;
            while e < cs.len() && (cs[e].is_ascii_alphanumeric() || cs[e] == '_' || (cs[e] == '-' && e > b && cs[e - 1].is_ascii_alphanumeric() && cs.get(e + 1).is_some_and(|d| d.is_ascii_alphabetic()))) {
                e += 1;
            }
            let n: String = cs[b..e].iter().collect();
            step(&mut i, &mut line, &mut col, e - b);
            Tok::Name(n)
        } else if c.is_ascii_digit() {
            let b = i;
            while i < cs.len() && cs[i].is_ascii_digit() {
                step(&mut i, &mut line, &mut col, 1);
            }
            let s: String = cs[b..i].iter().collect();
            Tok::Int(s.parse().map_err(|_| LexError { at, why: format!("{s} is past the largest number") })?)
        } else if c == '"' {
            let triple = cs.get(i + 1) == Some(&'"') && cs.get(i + 2) == Some(&'"');
            step(&mut i, &mut line, &mut col, if triple { 3 } else { 1 });
            let mut s = String::new();
            loop {
                let Some(&d) = cs.get(i) else { return Err(LexError { at, why: "a text that is never closed".into() }) };
                if triple && d == '"' && cs.get(i + 1) == Some(&'"') && cs.get(i + 2) == Some(&'"') {
                    step(&mut i, &mut line, &mut col, 3);
                    break;
                }
                if !triple && d == '"' {
                    step(&mut i, &mut line, &mut col, 1);
                    break;
                }
                if !triple && d == '\n' {
                    return Err(LexError { at, why: "a text in one pair of quotes runs past its line".into() });
                }
                if d == '\\' {
                    let e = cs.get(i + 1).copied().ok_or(LexError { at, why: "a text that is never closed".into() })?;
                    s.push(match e {
                        'n' => '\n',
                        't' => '\t',
                        '\\' => '\\',
                        '"' => '"',
                        '{' => '{',
                        _ => return Err(LexError { at: At { line, col }, why: format!("`\\{e}` is no escape") }),
                    });
                    step(&mut i, &mut line, &mut col, 2);
                    continue;
                }
                s.push(d);
                step(&mut i, &mut line, &mut col, 1);
            }
            Tok::Text(if triple { dedent(&s) } else { s })
        } else if let Some(m) = MARKS.iter().find(|m| m.chars().enumerate().all(|(k, mc)| cs.get(i + k) == Some(&mc))) {
            step(&mut i, &mut line, &mut col, m.len());
            Tok::Mark(m)
        } else {
            return Err(LexError { at, why: format!("`{c}` begins no token") });
        };
        out.push(Token { tok, at });
    }
    out.push(Token { tok: Tok::End, at: At { line, col } });
    Ok(out)
}

/// A text in three quotes: its first and last lines dropped when empty, and the indent its lines share taken off.
fn dedent(s: &str) -> String {
    let mut lines: Vec<&str> = s.split('\n').collect();
    if lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }
    if lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    let indent = lines.iter().filter(|l| !l.trim().is_empty()).map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
    lines.iter().map(|l| if l.len() >= indent { &l[indent..] } else { l.trim_start() }).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<Tok> {
        lex(s).unwrap().into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn a_dash_between_letters_is_part_of_a_name_and_spaced_it_is_a_minus() {
        assert_eq!(toks("Owes-Green now - w 0 - 100"), vec![Tok::Name("Owes-Green".into()), Tok::Name("now".into()), Tok::Mark("-"), Tok::Name("w".into()), Tok::Int(0), Tok::Mark("-"), Tok::Int(100), Tok::End]);
    }

    #[test]
    fn marks_take_the_longest_and_a_gap_is_three_dots() {
        assert_eq!(toks("<- -> => ... .. . <= &&"), ["<-", "->", "=>", "...", "..", ".", "<=", "&&"].into_iter().map(Tok::Mark).chain([Tok::End]).collect::<Vec<_>>());
    }

    #[test]
    fn a_text_in_three_quotes_loses_its_shared_indent() {
        assert_eq!(toks("Template \"\"\"\n  a {{ x }}\n    b\n  \"\"\"")[1], Tok::Text("a {{ x }}\n  b".into()));
    }

    #[test]
    fn a_plus_in_the_first_column_stands_there() {
        let t = lex("x\n+Worker(\"w1\"). +Worker(\"w2\").").unwrap();
        assert!(t[1].at.col == 1 && t[1].tok == Tok::Mark("+"));
        assert!(t[7].at.col > 1 && t[7].tok == Tok::Mark("+"));
    }
}
