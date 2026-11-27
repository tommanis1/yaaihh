//! The host table (spec 6.3): the fixed pure functions on primitives and lists, for the machine and the evaluator.

use super::value::Value;

/// The table's version; it enters every state hash (11).
pub const VERSION: u32 = 1;

/// Whether `name` is in the table.
pub fn is_host(name: &str) -> bool {
    matches!(name, "matches_glob" | "diff_lines" | "added_lines" | "removed_lines" | "str_len" | "contains" | "is_subsequence" | "first" | "to_int")
}

/// Apply host function `name`; `Err` when the arguments are not of its types (a fault of the checker if it comes).
pub fn apply(name: &str, args: &[Value]) -> Result<Value, String> {
    let text = |k: usize| match args.get(k) {
        Some(Value::Text(s)) => Ok(s.as_str()),
        other => Err(format!("{name}: argument {} is no text: {other:?}", k + 1)),
    };
    let list = |k: usize| match args.get(k) {
        Some(Value::List(v)) => Ok(v),
        other => Err(format!("{name}: argument {} is no list: {other:?}", k + 1)),
    };
    let n = |x: usize| Value::Int(x as i64);
    Ok(match name {
        // the text first, the pattern second, as the programs write it: `matches_glob(path, g)`
        "matches_glob" => Value::Bool(glob(text(1)?.as_bytes(), text(0)?.as_bytes())),
        "diff_lines" => {
            let (a, b): (Vec<&str>, Vec<&str>) = (text(0)?.lines().collect(), text(1)?.lines().collect());
            let common = lcs(&a, &b);
            n(a.len() - common + b.len() - common)
        }
        "added_lines" => n(text(0)?.lines().filter(|l| l.starts_with('+') && !l.starts_with("+++")).count()),
        "removed_lines" => n(text(0)?.lines().filter(|l| l.starts_with('-') && !l.starts_with("---")).count()),
        "str_len" => n(text(0)?.chars().count()),
        "contains" => Value::Bool(text(0)?.contains(text(1)?)),
        "is_subsequence" => {
            let (a, b) = (list(0)?, list(1)?);
            let mut it = b.iter();
            Value::Bool(a.iter().all(|x| it.any(|y| y == x)))
        }
        "first" => list(0)?.first().cloned().map_or_else(Value::none, Value::some),
        "to_int" => text(0)?.trim().parse::<i64>().map_or_else(|_| Value::none(), |v| Value::some(Value::Int(v))),
        _ => return Err(format!("{name} is not in the host table")),
    })
}

/// `*` any run of characters, `?` one, everything else itself.
fn glob(p: &[u8], s: &[u8]) -> bool {
    let (mut pi, mut si, mut wild, mut mark) = (0, 0, None, 0);
    while si < s.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            wild = Some(pi);
            mark = si;
            pi += 1;
        } else if let Some(st) = wild {
            pi = st + 1;
            mark += 1;
            si = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == b'*')
}

/// The length of the longest common subsequence of two lists of lines.
fn lcs(a: &[&str], b: &[&str]) -> usize {
    let mut row = vec![0usize; b.len() + 1];
    for x in a {
        let mut diag = 0;
        for (j, y) in b.iter().enumerate() {
            let up = row[j + 1];
            row[j + 1] = if x == y { diag + 1 } else { row[j + 1].max(row[j]) };
            diag = up;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Value {
        Value::text(s)
    }

    #[test]
    fn the_host_functions_give_what_6_3_says() {
        assert_eq!(apply("matches_glob", &[t("src/a.rs"), t("src/*.rs")]).unwrap(), Value::Bool(true));
        assert_eq!(apply("matches_glob", &[t("src/a/b.rs"), t("src/?.rs")]).unwrap(), Value::Bool(false));
        assert_eq!(apply("diff_lines", &[t("a\nb\nc"), t("a\nx\nc")]).unwrap(), Value::Int(2));
        assert_eq!(apply("added_lines", &[t("+++ b\n+x\n+y\n-z")]).unwrap(), Value::Int(2));
        assert_eq!(apply("removed_lines", &[t("--- a\n+x\n-z")]).unwrap(), Value::Int(1));
        assert_eq!(apply("to_int", &[t("42")]).unwrap(), Value::some(Value::Int(42)));
        assert_eq!(apply("to_int", &[t("4x")]).unwrap(), Value::none());
        assert_eq!(apply("first", &[Value::List(vec![])]).unwrap(), Value::none());
        let l = |v: &[i64]| Value::List(v.iter().map(|n| Value::Int(*n)).collect());
        assert_eq!(apply("is_subsequence", &[l(&[1, 3]), l(&[1, 2, 3])]).unwrap(), Value::Bool(true));
        assert_eq!(apply("is_subsequence", &[l(&[3, 1]), l(&[1, 2, 3])]).unwrap(), Value::Bool(false));
    }
}
