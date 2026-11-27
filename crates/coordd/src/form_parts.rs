//! An answer in named parts (N-101, docket-core3): a constructor with two or more parts has each cut from the turn's
//! output, never written by a model. A part is a field of type Text, Text?, [Text], or a list of records whose fields are
//! all of those (a design's `parts: [Part]`). The fast path cuts at the output's headings when every required one is
//! there; otherwise the reading call is given the output once, its lines numbered, and answers with marks: each part's
//! first and last line, a name and a list of names as values (the owner, N-101 p648: "give it the text ask it for the
//! change"). The engine cuts the lines out of the output itself.
//! A Text part is a span of the output (`{"span": {loc, at, len}}`), so nothing is copied; a [Text]'s items and a
//! record's name, short words, are new texts.

use docket_machine::Type;
use serde_json::{Map, Value, json};

/// What kind of part a field is.
#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    Text { opt: bool },
    Texts,
    /// A list of records, each field a part itself (not a list of records again).
    Records(Vec<(String, Part)>),
}

/// Field T's kind of part, when it is one.
pub fn part(t: &Type) -> Option<Part> {
    match t {
        Type::Text => Some(Part::Text { opt: false }),
        Type::Opt(i) if **i == Type::Text => Some(Part::Text { opt: true }),
        Type::List(i) if **i == Type::Text => Some(Part::Texts),
        Type::List(i) => match &**i {
            Type::Record(fs) if !fs.is_empty() => fs.iter().map(|(f, t)| Some((f.clone(), part(t).filter(|p| !matches!(p, Part::Records(_)))?))).collect::<Option<Vec<_>>>().map(Part::Records),
            _ => None,
        },
        _ => None,
    }
}

/// The parts of a constructor's fields FS, in order.
pub fn parts(fs: &[(String, Type)]) -> Vec<(String, Part)> {
    fs.iter().filter_map(|(f, t)| Some((f.clone(), part(t)?))).collect()
}

/// A part found: where its text is in the output (bytes AT..AT+LEN), or, for a list of records, each element's.
#[derive(Clone, Debug, PartialEq)]
pub enum Found {
    Text(usize, usize),
    Records(Vec<Vec<(String, Found)>>),
    /// A value the marks reading gave as it is: a record's name, a list of names.
    Value(Value),
}

/// A heading's words as a field's name: lowercased, joined by `_`; `#`, `*` and a trailing `:` left out.
fn words(s: &str) -> Vec<String> {
    s.trim().trim_start_matches(['#', '*', ' ']).trim_end_matches(['*', ' ']).trim_end_matches(':').trim_end_matches(['*', ' ']).split_whitespace().map(|w| w.trim_matches(['*', '`', '"']).to_lowercase()).filter(|w| !w.is_empty()).collect()
}

/// Whether heading words W name field F: `stands_on` is "Stands on", `done` is "Done when".
fn names(w: &[String], f: &str) -> bool {
    let fw: Vec<&str> = f.split('_').collect();
    w.len() >= fw.len() && w.iter().zip(&fw).all(|(a, b)| a == b) && w.len() <= fw.len() + 2
}

/// The word an element of list F is headed by: F without its last `s` (`parts`: "Part Store").
fn element_word(f: &str) -> String {
    f.strip_suffix('s').unwrap_or(f).replace('_', " ")
}

/// One line of the output: where it starts, and its text.
struct Line<'a> {
    at: usize,
    text: &'a str,
}

fn lines(text: &str) -> Vec<Line<'_>> {
    let mut out = Vec::new();
    let mut at = 0;
    for l in text.split_inclusive('\n') {
        out.push(Line { at, text: l.trim_end_matches(['\n', '\r']) });
        at += l.len();
    }
    out
}

/// A heading on line L: its words, and where the rest of the line starts when it says more after a `:`. The marks
/// closing a heading after its colon (`*Done when:* the owner runs`) are the heading's, not the value's.
fn heading(l: &Line) -> (Vec<String>, Option<usize>) {
    match l.text.split_once(':') {
        Some((h, rest)) if h.split_whitespace().count() <= 3 => {
            let value = rest.trim_start_matches(['*', '_']);
            if value.trim().is_empty() { (words(l.text), None) } else { (words(h), Some(l.at + h.len() + 1 + (rest.len() - value.len()))) }
        }
        _ => (words(l.text), None),
    }
}

/// The span AT..END of TEXT without the blank space at its ends.
fn trimmed(text: &str, at: usize, end: usize) -> (usize, usize) {
    let s = &text[at..end];
    let start = at + (s.len() - s.trim_start().len());
    let len = s.trim().len();
    (start, len)
}

/// The span AT, LEN of TEXT, part F, without the label a former tagged with it (N-159: "Asked: …" tagged whole): a
/// first line that is only F's heading, or F's heading and `:` before the value; for a list of names (LIST) also F's
/// words before them ("stands on none").
fn unlabeled(text: &str, at: usize, len: usize, f: &str, list: bool) -> (usize, usize) {
    let s = &text[at..at + len];
    let first = s.lines().next().unwrap_or("");
    // a label alone on its line: the field's words, or one more ("Done when"), and no sentence ("After the change.")
    let alone = |w: &[String]| names(w, f) && w.len() <= f.split('_').count() + 1 && !first.trim_end().trim_end_matches(['*', '_']).ends_with('.');
    let cut = if !first.contains(':') && alone(&words(first)) && first.len() < s.len() {
        Some(first.len())
    } else if let Some((_, rest)) = first.split_once(':').filter(|(h, _)| h.split_whitespace().count() <= 3 && names(&words(h), f)) {
        // the marks closing the label after its colon (`*Done when:* the owner runs`) are the label's
        Some(first.len() - rest.trim_start_matches(['*', '_']).len())
    } else if list {
        let mut i = 0;
        let mut all = true;
        for w in f.split('_') {
            let rest = &first[i..];
            let start = rest.len() - rest.trim_start().len();
            let end = rest[start..].find(char::is_whitespace).map_or(rest.len(), |e| start + e);
            if !rest[start..end].trim_matches(['*', '`']).eq_ignore_ascii_case(w) {
                all = false;
                break;
            }
            i += end;
        }
        (all && !first[i..].trim().is_empty()).then_some(i)
    } else {
        None
    };
    match cut {
        Some(c) => trimmed(text, at + c, at + len),
        None => (at, len),
    }
}

/// The one text field F of constructor C (its fields FS) in TEXT, when the answer opens with the form's own labels:
/// blank lines, a line that is only C's name, and lines `G: value` of C's other fields; then F's label (`words:`, alone
/// on its line or before the text), or, after at least one of those lines, the text itself. F is what follows (its
/// bytes AT, LEN); None when the answer opens with anything else, and F is then the whole answer (docket-core3's
/// `--say` run: Direct's `words` was "Direct / agent: writer / words: ...", orch5's whole answer; N-101 p981, p992).
pub fn labelled(text: &str, c: &str, fs: &[(String, Type)], f: &str) -> Option<(usize, usize)> {
    let exactly = |w: &[String], g: &str| names(w, g) && w.len() == g.split('_').count();
    let ls = lines(text);
    let mut seen = false;
    for (i, l) in ls.iter().enumerate() {
        if l.text.trim().is_empty() {
            continue;
        }
        let (w, rest) = heading(l);
        if exactly(&w, f) {
            let at = rest.unwrap_or_else(|| ls.get(i + 1).map_or(text.len(), |n| n.at));
            let (at, len) = trimmed(text, at, text.len());
            return (len > 0).then_some((at, len));
        }
        let name = rest.is_none() && w == [c.to_lowercase()];
        let other = rest.is_some() && fs.iter().any(|(g, _)| g != f && exactly(&w, g));
        if !(name || other) {
            if !seen {
                return None;
            }
            let (at, len) = trimmed(text, l.at, text.len());
            return Some((at, len));
        }
        seen = true;
    }
    None
}

/// A short text (a record's name) as a name: no markup around it, and not headed by its list's element word
/// ("**Part provide**:" is "provide").
fn bare(s: &str, list: &str) -> String {
    let t = s.trim().trim_matches(['*', '`', '"', '#', '_', ' ']).trim_end_matches([':', '.']).trim_matches(['*', '`', '"', ' ']);
    let w = element_word(list);
    match t.get(..w.len()) {
        Some(h) if h.eq_ignore_ascii_case(&w) && t[w.len()..].starts_with(' ') && t.len() > w.len() + 1 => t[w.len() + 1..].trim_matches(['*', '`', '"', ' ', ':']).to_string(),
        _ => t.to_string(),
    }
}

/// The parts PS of a constructor cut from TEXT at their headings; the required ones with no heading otherwise.
pub fn by_headings(text: &str, ps: &[(String, Part)]) -> Result<Vec<(String, Found)>, Vec<String>> {
    // each line that heads something: a part of the constructor, an element of a list, or a field of the element
    #[derive(Clone)]
    enum Mark {
        Top(usize),
        Element(usize, String),
        Sub(usize, usize),
    }
    let ls = lines(text);
    let mut marks: Vec<(usize, usize, Mark)> = Vec::new(); // (start of the heading line, start of its content, mark)
    let mut inside: Option<usize> = None;
    for (i, l) in ls.iter().enumerate() {
        let (w, rest) = heading(l);
        if w.is_empty() {
            continue;
        }
        let content = rest.unwrap_or_else(|| ls.get(i + 1).map_or(text.len(), |n| n.at));
        if let Some(k) = ps.iter().position(|(f, _)| names(&w, f)) {
            if !matches!(ps[k].1, Part::Records(_)) {
                marks.push((l.at, content, Mark::Top(k)));
                inside = None;
                continue;
            }
        }
        if let Some(k) = ps.iter().position(|(f, p)| matches!(p, Part::Records(_)) && w.first().is_some_and(|x| *x == element_word(f)) && rest.is_none()) {
            let name = l.text.trim().trim_start_matches(['#', '*', ' ']);
            let name = name.split_once(char::is_whitespace).map_or("", |(_, n)| n).trim().trim_start_matches([':', '-', ' ']).trim_end_matches(['*', ':', ' ']).to_string();
            marks.push((l.at, content, Mark::Element(k, name)));
            inside = Some(k);
            continue;
        }
        if let Some(k) = inside {
            let Part::Records(rs) = &ps[k].1 else { unreachable!() };
            if let Some(j) = rs.iter().position(|(f, _)| names(&w, f)).filter(|j| *j > 0 || !matches!(rs[0].1, Part::Text { .. })) {
                marks.push((l.at, content, Mark::Sub(k, j)));
            }
        }
    }
    let end_of = |m: usize| marks.get(m + 1).map_or(text.len(), |n| n.0);
    let mut out: Vec<(String, Option<Found>)> = ps.iter().map(|(f, p)| (f.clone(), matches!(p, Part::Records(_)).then(|| Found::Records(Vec::new())))).collect();
    let mut missing = Vec::new();
    for (m, (_, content, mark)) in marks.iter().enumerate() {
        let end = end_of(m).max(*content);
        match mark {
            Mark::Top(k) => {
                if out[*k].1.is_none() {
                    let (at, len) = trimmed(text, *content, end);
                    out[*k].1 = Some(Found::Text(at, len));
                }
            }
            Mark::Element(k, name) => {
                let Some(Found::Records(es)) = &mut out[*k].1 else { unreachable!() };
                let Part::Records(rs) = &ps[*k].1 else { unreachable!() };
                // the name on the heading line fills the record's first text field
                let mut e: Vec<(String, Found)> = Vec::new();
                if matches!(rs[0].1, Part::Text { .. }) {
                    let line_at = marks[m].0;
                    let off = text[line_at..].find(name.as_str()).map_or(line_at, |o| line_at + o);
                    e.push((rs[0].0.clone(), Found::Text(off, if name.is_empty() { 0 } else { name.len() })));
                }
                es.push(e);
            }
            Mark::Sub(k, j) => {
                let Some(Found::Records(es)) = &mut out[*k].1 else { unreachable!() };
                let Part::Records(rs) = &ps[*k].1 else { unreachable!() };
                if let Some(e) = es.last_mut() {
                    if !e.iter().any(|(f, _)| *f == rs[*j].0) {
                        let (at, len) = trimmed(text, *content, end);
                        e.push((rs[*j].0.clone(), Found::Text(at, len)));
                    }
                }
            }
        }
    }
    let mut found = Vec::new();
    for ((f, p), (_, x)) in ps.iter().zip(out) {
        match (p, x) {
            (Part::Text { opt: false }, None) => missing.push(f.clone()),
            (Part::Records(rs), Some(Found::Records(es))) => {
                if es.is_empty() {
                    missing.push(format!("{} NAME (one for each of {f})", capital(&element_word(f))));
                }
                for (i, e) in es.iter().enumerate() {
                    for (g, q) in rs {
                        if matches!(q, Part::Text { opt: false }) && !e.iter().any(|(h, _)| h == g) {
                            missing.push(format!("{} of {f} {}", g, i + 1));
                        }
                    }
                }
                found.push((f.clone(), Found::Records(es)));
            }
            (_, Some(x)) => found.push((f.clone(), x)),
            (_, None) => {}
        }
    }
    if missing.is_empty() { Ok(found) } else { Err(missing) }
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// The type a part P is asked for in the marks reading (the owner, N-101 p648): a passage by its lines, `{first,
/// last}`, or a value that is a piece of one line by that line and its characters, `{line, text}` (paper-research
/// N-101: `a_head: 96a5…` on one line); a list of names as the names; a record's first text field (its name) as
/// written. The answer is given to the call once, its lines numbered, and the call never writes a passage again.
fn mark_type(p: &Part, name: bool) -> Type {
    let int = || Type::Opt(Box::new(Type::Int));
    let text = || Type::Opt(Box::new(Type::Text));
    let lines = Type::Record(vec![("first".into(), int()), ("last".into(), int()), ("from".into(), text()), ("line".into(), int()), ("text".into(), text())]);
    match p {
        Part::Text { .. } if name => Type::Text,
        Part::Text { opt: false } => lines,
        Part::Text { opt: true } => Type::Opt(Box::new(lines)),
        Part::Texts => Type::List(Box::new(Type::Text)),
        Part::Records(rs) => Type::List(Box::new(Type::Record(rs.iter().enumerate().map(|(i, (g, q))| (g.clone(), mark_type(q, i == 0 && matches!(q, Part::Text { .. })))).collect()))),
    }
}

/// The fields the marks reading asks for, one for each part of PS, under the part's own name.
pub fn mark_fields(ps: &[(String, Part)]) -> Vec<(String, Type)> {
    ps.iter().map(|(f, p)| (f.clone(), mark_type(p, false))).collect()
}

/// TEXT with its lines numbered from 1, as the marks reading is given it.
pub fn numbered(text: &str) -> String {
    let ls = lines(text);
    let w = ls.len().to_string().len();
    ls.iter().enumerate().map(|(i, l)| format!("{:>w$}  {}", i + 1, l.text)).collect::<Vec<_>>().join("\n")
}

/// The parts PS of a constructor from A, the marks reading's answer (each part under its own name, as `mark_fields`
/// asks): each passage the lines it marks, cut from TEXT whole, without the part's own label on its first line; a name
/// and a list of names as given. Why not, for the one retry: lines out of order or outside the answer, two passages on
/// the same line, a list of records with no element.
#[cfg(test)]
pub fn by_marks(a: &Value, text: &str, ps: &[(String, Part)]) -> Result<Vec<(String, Found)>, String> {
    by_marks_with(a, text, ps, &[])
}

/// `by_marks`, a list of names in WHOLE (`whole`: its record list's field, or none, and its own) taken entry by
/// entry as the call gives them, none cut at a comma.
pub fn by_marks_with(a: &Value, text: &str, ps: &[(String, Part)], whole: &[(String, String)]) -> Result<Vec<(String, Found)>, String> {
    let ls = lines(text);
    let n = ls.len();
    // what each part took, in bytes of TEXT: whole lines, or a piece of one
    let mut used: Vec<(usize, usize, String)> = Vec::new();
    let mut cut = |m: &Value, what: String, f: &str, list: bool| -> Result<Found, String> {
        let num = |k: &str| m.get(k).and_then(Value::as_u64).map(|x| x as usize);
        // a piece of one line: its own characters, taken only where they stand in that line
        if let (Some(line), Some(piece)) = (num("line"), m.get("text").and_then(Value::as_str)) {
            if line == 0 || line > n {
                return Err(format!("{what}: line {line} is not in the answer, whose lines are 1 to {n}"));
            }
            let piece = piece.trim();
            let l = &ls[line - 1];
            let Some(i) = l.text.find(piece).filter(|_| !piece.is_empty()) else {
                return Err(format!("{what}: \"{piece}\" is not in line {line}; give the value's characters exactly as they stand in that line"));
            };
            let (at, end) = (l.at + i, l.at + i + piece.len());
            if let Some((_, _, other)) = used.iter().find(|(a, b, _)| at < *b && *a < end) {
                return Err(format!("{what} (in line {line}) and {other} overlap; no characters are in two parts"));
            }
            used.push((at, end, format!("{what} (in line {line})")));
            return Ok(Found::Text(at, piece.len()));
        }
        let (first, last) = (num("first").unwrap_or(0), num("last").unwrap_or(0));
        if first == 0 || last == 0 || first > n || last > n {
            return Err(format!("{what}: lines {first} to {last} are not all in the answer, whose lines are 1 to {n}"));
        }
        if first > last {
            return Err(format!("{what}: line {first} comes after line {last}; give the first line, then the last"));
        }
        let (at, end) = (ls[first - 1].at, ls[last - 1].at + ls[last - 1].text.len());
        if let Some((_, _, other)) = used.iter().find(|(a, b, _)| at <= *b && *a <= end) {
            return Err(format!("{what} (lines {first} to {last}) and {other} share a line; no line is in two parts"));
        }
        used.push((at, end, format!("{what} (lines {first} to {last})")));
        // begun on its label's line: from the characters it begins with there, found in that line exactly
        if let Some(from) = m.get("from").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) {
            let Some(i) = ls[first - 1].text.find(from) else {
                return Err(format!("{what}: \"{from}\" is not in line {first}; give the characters the part begins with exactly as they stand in its first line"));
            };
            let (at, len) = trimmed(text, ls[first - 1].at + i, end);
            return Ok(Found::Text(at, len));
        }
        let (at, len) = trimmed(text, at, end);
        let (at, len) = unlabeled(text, at, len, f, list);
        Ok(Found::Text(at, len))
    };
    // a mark with neither its lines nor its line and text is no mark: none where the part may have none
    let none = |v: &Value| v.is_null() || (v.is_object() && ["first", "last", "from", "line", "text"].iter().all(|k| v.get(k).is_none_or(Value::is_null)));
    let names = |v: &Value, f: &str, g: &str| -> Value {
        let keep = whole.iter().any(|(a, b)| a == f && b == g);
        json!(v.as_array().map(|xs| xs.iter().filter_map(Value::as_str).flat_map(|x| if keep { clean(x).into_iter().collect() } else { items(x) }).collect::<Vec<_>>()).unwrap_or_default())
    };
    let mut found = Vec::new();
    for (f, p) in ps {
        let v = a.get(f).unwrap_or(&Value::Null);
        match p {
            Part::Text { opt } if none(v) => {
                if !opt {
                    return Err(format!("{f} has no lines; it is required"));
                }
            }
            Part::Text { .. } => found.push((f.clone(), cut(v, f.clone(), f, false)?)),
            Part::Texts => found.push((f.clone(), Found::Value(names(v, "", f)))),
            Part::Records(rs) => {
                let es = v.as_array().filter(|es| !es.is_empty()).ok_or_else(|| format!("{f} has no element: each of the answer's {f} is one"))?;
                let mut out = Vec::new();
                for (i, e) in es.iter().enumerate() {
                    let mut o = Vec::new();
                    for (j, (g, q)) in rs.iter().enumerate() {
                        let x = e.get(g).unwrap_or(&Value::Null);
                        let what = format!("{g} of {f} {}", i + 1);
                        match q {
                            Part::Text { .. } if j == 0 => o.push((g.clone(), Found::Value(json!(bare(x.as_str().unwrap_or_default(), f))))),
                            Part::Text { opt } if none(x) => {
                                if !opt {
                                    return Err(format!("{what} has no lines; it is required"));
                                }
                            }
                            Part::Text { .. } => o.push((g.clone(), cut(x, what, g, false)?)),
                            Part::Texts => o.push((g.clone(), Found::Value(names(x, f, g)))),
                            Part::Records(_) => {}
                        }
                    }
                    out.push(o);
                }
                found.push((f.clone(), Found::Records(out)));
            }
        }
    }
    Ok(found)
}

/// The items of a [Text] part's text: its lines, or its comma-separated items; `none` is none.
fn items(s: &str) -> Vec<String> {
    let s = s.trim();
    let raw: Vec<&str> = if s.contains('\n') { s.lines().collect() } else { s.split([',', ';']).collect() };
    raw.into_iter().filter_map(clean).collect()
}

/// The items of a [Text] part's text that names other elements by name (`whole`): its lines, each whole; the names in
/// them are found by `named`, which knows them.
fn entries(s: &str) -> Vec<String> {
    s.trim().lines().filter_map(clean).collect()
}

/// One item as a name: without a bullet, quotes or a closing stop; none for `none`.
fn clean(i: &str) -> Option<String> {
    let i = i.trim().trim_start_matches(['-', '*', '•']).trim().trim_matches(['`', '"', '.']).trim();
    (!i.is_empty() && !i.eq_ignore_ascii_case("none") && !i.eq_ignore_ascii_case("nothing") && !i.eq_ignore_ascii_case("-")).then(|| i.to_string())
}

/// A place of `Defs.names` (`Design.parts[].stands_on`) as the path under constructor C (`parts`, `[]`,
/// `stands_on`); none when it is of another constructor.
fn under(place: &str, c: &str) -> Option<Vec<String>> {
    let rest = place.strip_prefix(c)?.strip_prefix('.')?;
    let mut out = Vec::new();
    for s in rest.split('.') {
        match s.strip_suffix("[]") {
            Some(f) => out.extend([f.to_string(), "[]".to_string()]),
            None => out.push(s.to_string()),
        }
    }
    Some(out)
}

/// The lists of constructor C that name elements by name (NAMES, `Defs.names` of the step): each by its record
/// list's field (none for a field of C itself) and its own, as `by_marks_with` and `fill_with` take them.
pub fn whole(names: &[(String, String)], c: &str) -> Vec<(String, String)> {
    names.iter().filter_map(|(l, _)| match under(l, c)?.as_slice() {
        [g] => Some((String::new(), g.clone())),
        [f, b, g] if b == "[]" => Some((f.clone(), g.clone())),
        _ => None,
    }).collect()
}

/// The field whose values the entries of list G in the records F name (NAMES, of C): `name` for `stands_on`.
fn named_field(names: &[(String, String)], c: &str, f: &str, g: &str) -> Option<String> {
    names.iter().find_map(|(l, n)| {
        let (l, n) = (under(l, c)?, under(n, c)?);
        (l == [f, "[]", g] && n.len() == 3 && n[0] == f && n[1] == "[]").then(|| n[2].clone())
    })
}

fn values<'a>(v: &'a Value, segs: &[String], out: &mut Vec<&'a Value>) {
    match segs.split_first() {
        None => out.push(v),
        Some((s, rest)) if s == "[]" => v.as_array().into_iter().flatten().for_each(|x| values(x, rest, out)),
        Some((s, rest)) => {
            if let Some(x) = v.get(s.as_str()) {
                values(x, rest, out)
            }
        }
    }
}

fn values_mut<'a>(v: &'a mut Value, segs: &[String], at: Vec<usize>, out: &mut Vec<(Vec<usize>, &'a mut Value)>) {
    match segs.split_first() {
        None => out.push((at, v)),
        Some((s, rest)) if s == "[]" => {
            if let Value::Array(xs) = v {
                for (i, x) in xs.iter_mut().enumerate() {
                    let mut a = at.clone();
                    a.push(i);
                    values_mut(x, rest, a, out);
                }
            }
        }
        Some((s, rest)) => {
            if let Some(x) = v.get_mut(s.as_str()) {
                values_mut(x, rest, at, out)
            }
        }
    }
}

/// Entry E as the names NS it gives: E when it is one; else the names written in it, when nothing else is (a comma,
/// a semicolon or `and` between them); else none.
fn resolve(e: &str, ns: &[&str]) -> Option<Vec<String>> {
    if ns.contains(&e) {
        return Some(vec![e.to_string()]);
    }
    let mut by_len: Vec<&str> = ns.iter().copied().filter(|n| !n.is_empty()).collect();
    by_len.sort_by_key(|n| std::cmp::Reverse(n.len()));
    let mut taken: Vec<(usize, usize)> = Vec::new();
    for n in by_len {
        for (i, _) in e.match_indices(n) {
            if !taken.iter().any(|(a, b)| i < *b && *a < i + n.len()) {
                taken.push((i, i + n.len()));
            }
        }
    }
    taken.sort();
    let mut rest = String::new();
    let mut last = 0;
    for (a, b) in &taken {
        rest.push_str(&e[last..*a]);
        rest.push(' ');
        last = *b;
    }
    rest.push_str(&e[last..]);
    let left = rest.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).any(|w| !w.is_empty() && !w.eq_ignore_ascii_case("and"));
    (!taken.is_empty() && !left).then(|| taken.iter().map(|(a, b)| e[*a..*b].to_string()).collect())
}

/// Answer A with each entry of a list that names other elements (NAMES, `Defs.names` of the step: run 4's
/// `stands_on` names `parts[].name`, paper-research N-101 p1141) as the names it gives; why not, for the agent, when
/// an entry names none: all such entries at once, with the names there are.
pub fn named(a: &mut Value, names: &[(String, String)]) -> Result<(), String> {
    let Some(c) = a.get("head").and_then(Value::as_str).map(str::to_string) else { return Ok(()) };
    let mut why = Vec::new();
    for (l, n) in names {
        let (Some(ls), Some(nsegs)) = (under(l, &c), under(n, &c)) else { continue };
        let mut got = Vec::new();
        values(a, &nsegs, &mut got);
        let ns: Vec<String> = got.into_iter().filter_map(Value::as_str).map(str::to_string).collect();
        let refs: Vec<&str> = ns.iter().map(String::as_str).collect();
        let mut lists = Vec::new();
        values_mut(a, &ls, Vec::new(), &mut lists);
        let mut missed = Vec::new();
        for (at, list) in lists {
            let Some(xs) = list.as_array() else { continue };
            let mut out = Vec::new();
            for x in xs.iter().filter_map(Value::as_str) {
                match resolve(x, &refs) {
                    Some(r) => out.extend(r),
                    None => {
                        let whose = ls.iter().filter(|s| *s != "[]").take(ls.len().saturating_sub(1)).zip(&at).map(|(f, i)| format!(" of {f} {}", i + 1)).collect::<String>();
                        missed.push(format!("\"{x}\" ({}{whose})", ls.last().cloned().unwrap_or_default()));
                    }
                }
            }
            out.dedup();
            *list = json!(out);
        }
        if !missed.is_empty() {
            let (f, g) = (nsegs.first().cloned().unwrap_or_default(), ls.last().cloned().unwrap_or_default());
            let h = nsegs.last().cloned().unwrap_or_default();
            let there = ns.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join("; ");
            why.push(format!("Each entry of {g} is one of the {f}' {h}s, letter for letter, and {} none: {}. The {f}' {h}s are: {there}.", if missed.len() == 1 { "this names" } else { "these name" }, missed.join(", ")));
        }
    }
    if why.is_empty() { Ok(()) } else { Err(why.join(" ")) }
}

/// The values of the parts FOUND (of PS) in TEXT: a Text a span of the place LOC (the output's), or, with no place
/// (the owner's words), the words themselves; into the answer A.
#[cfg(test)]
pub fn fill(a: &mut Map<String, Value>, ps: &[(String, Part)], found: &[(String, Found)], text: &str, loc: Option<&Value>) {
    fill_with(a, ps, found, text, loc, &[])
}

/// `fill`, a list of names in WHOLE (as `by_marks_with` takes it) cut at its lines only.
pub fn fill_with(a: &mut Map<String, Value>, ps: &[(String, Part)], found: &[(String, Found)], text: &str, loc: Option<&Value>, whole: &[(String, String)]) {
    let span = |at: usize, len: usize| -> Value {
        match loc {
            Some(l) => json!({"span": {"loc": l, "at": at, "len": len}}),
            None => json!(text[at..at + len]),
        }
    };
    let one = |p: &Part, x: &Found, short: Option<&str>, keep: bool| -> Value {
        match (p, x) {
            (Part::Texts, Found::Text(at, len)) if keep => json!(entries(&text[*at..*at + *len])),
            (Part::Texts, Found::Text(at, len)) => json!(items(&text[*at..*at + *len])),
            // a record's name, a few words, is a new text, without markup (the list it is in: SHORT)
            (Part::Text { .. }, Found::Text(at, len)) if short.is_some() => json!(bare(&text[*at..*at + *len], short.unwrap_or(""))),
            (Part::Text { .. }, Found::Text(at, len)) => span(*at, *len),
            (_, Found::Value(v)) => v.clone(),
            _ => Value::Null,
        }
    };
    for (f, p) in ps {
        let x = found.iter().find(|(g, _)| g == f).map(|(_, x)| x);
        let v = match (p, x) {
            (Part::Records(rs), Some(Found::Records(es))) => Value::Array(
                es.iter()
                    .map(|e| {
                        let mut o = Map::new();
                        for (i, (g, q)) in rs.iter().enumerate() {
                            let v = e.iter().find(|(h, _)| h == g).map_or(if matches!(q, Part::Texts) { json!([]) } else { Value::Null }, |(_, x)| one(q, x, (i == 0).then_some(f.as_str()), whole.iter().any(|(a, b)| a == f && b == g)));
                            o.insert(g.clone(), v);
                        }
                        Value::Object(o)
                    })
                    .collect(),
            ),
            (Part::Texts, None) => json!([]),
            (p, Some(x)) => one(p, x, None, whole.iter().any(|(a, b)| a.is_empty() && b == f)),
            (_, None) => Value::Null,
        };
        a.insert(f.clone(), v);
    }
}

/// The one sentence that says which mark is for a value sharing its line with its label (paper-research N-101).
pub const PIECE: &str = "A part that is only a piece of one line, such as a value on the same line as its label, is given as {\"line\": N, \"text\": \"...\"}: the line's number and the value's own characters exactly as they stand in that line, without the label.";

/// The sentences that keep a part's label out of it (docket-core3, N-101: `**asked** (the owner's words…):` and
/// `- **after:** …` began their values): a label's own line is not the part's, and a part that begins on its label's
/// line says where, by its own characters.
pub const LABEL: &str = "A label alone on its line, such as `**Today**:` or `asked (the owner's words):`, is not one of the part's lines: begin with the line after it. When a part of several lines begins on the same line as its label, give with its lines {\"from\": \"...\"}: the characters it begins with in its first line, exactly as they stand there after the label; the part is taken from there.";

/// The marks reading's prompt (the owner, N-101 p648: "give it the text ask it for the change"): CHOICES the answers
/// it may give (with the fields it fills), PARTS each constructor's parts, given by their lines; FAILED, why the last
/// reading was not taken; SHOWN, the step as the agent was shown it; GIVES, what the step gives with its docs
/// (`doc::gives`, the text the agent was shown; paper-research N-101 p706), under which the prompt says only how each
/// field is given. TEXT is given once, its lines numbered.
#[cfg(test)]
pub fn marks_prompt(choices: &[String], none: &str, parts: &[(String, Vec<(String, Part)>)], text: &str, failed: Option<&str>, shown: Option<&str>, gives: Option<&str>) -> String {
    marks_prompt_with(choices, none, parts, text, failed, shown, gives, &[])
}

/// `marks_prompt`, a list that names other elements (NAMES, `Defs.names` of the step) asked for as those names.
#[allow(clippy::too_many_arguments)]
pub fn marks_prompt_with(choices: &[String], none: &str, parts: &[(String, Vec<(String, Part)>)], text: &str, failed: Option<&str>, shown: Option<&str>, gives: Option<&str>, rules: &[(String, String)]) -> String {
    let list = |xs: &[String]| xs.join(", ");
    let mut how = String::new();
    for (c, ps) in parts {
        let (mut lines, mut names, mut maybe, mut each) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for (f, p) in ps {
            match p {
                Part::Text { opt } => {
                    lines.push(f.clone());
                    if *opt {
                        maybe.push(f.clone());
                    }
                }
                Part::Texts => names.push(f.clone()),
                Part::Records(rs) => {
                    let (mut l, mut n, mut m) = (Vec::new(), Vec::new(), Vec::new());
                    for (i, (g, q)) in rs.iter().enumerate() {
                        match q {
                            Part::Text { .. } if i == 0 => n.push(format!("{g} as the answer writes it, without the marks around it")),
                            Part::Text { opt } => {
                                l.push(g.clone());
                                if *opt {
                                    m.push(g.clone());
                                }
                            }
                            _ => match named_field(rules, c, f, g) {
                                Some(h) => n.push(format!("{g} as the {f} it lists, each by the {h} you give it, letter for letter, [] where it says there are none")),
                                None => n.push(format!("{g} as the names it lists, [] where it says there are none")),
                            },
                        }
                    }
                    let mut e = format!("{f}: one element for each of the answer's {f}, in its order; in each, by their lines: {}", list(&l));
                    if !n.is_empty() {
                        e.push_str(&format!("; as values: {}", n.join("; ")));
                    }
                    if !m.is_empty() {
                        e.push_str(&format!("; null where it has none: {}", list(&m)));
                    }
                    each.push(e);
                }
            }
        }
        let mut line = format!("- {c}: by their lines: {}", list(&lines));
        if !names.is_empty() {
            line.push_str(&format!("; as the names they list, [] where it says there are none: {}", list(&names)));
        }
        if !maybe.is_empty() {
            line.push_str(&format!("; null where the answer has none: {}", list(&maybe)));
        }
        for e in each {
            line.push_str(&format!("; {e}"));
        }
        how.push_str(&line);
        how.push('\n');
    }
    let gives = gives.filter(|g| !g.trim().is_empty()).map(|g| format!("What the step gives, as the agent was shown it:\n{}\n\nHow each field is given:\n", g.trim_end())).unwrap_or_default();
    let how = format!("{gives}{how}");
    let again = failed.map(|f| format!("\nYour last reading could not be taken: {f}\n")).unwrap_or_default();
    let intro = super::agent_intro(choices, none, shown, "Say which outcome the answer gives, and fill the fields listed");
    format!(
        "{intro}\n\nThe answer's lines are numbered below. Do not write its passages out again: give each part by its lines, \
         {{\"first\": N, \"last\": M}}, the numbers of its first and its last line, both its own (a part on one line \
         has that line as both). {PIECE} {LABEL} No line is in two parts, and a line may be in none: a remark before the first part or after the last, \
         or between two parts, belongs to no part and is left out.\n{how}{again}\n\
         The agent's answer, its lines numbered:\n<<<\n{}\n>>>",
        numbered(text)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn design() -> Vec<(String, Type)> {
        let part = Type::Record(vec![("name".into(), Type::Text), ("stands_on".into(), Type::List(Box::new(Type::Text))), ("after".into(), Type::Text), ("done".into(), Type::Text)]);
        vec![("asked".into(), Type::Text), ("today".into(), Type::Text), ("parts".into(), Type::List(Box::new(part))), ("meaning".into(), Type::Opt(Box::new(Type::Text)))]
    }

    const ANSWER: &str = "Design\n\nAsked\nThe owner's words.\n\n## Today\nWhat is there.\nTwo lines.\n\nPart Store\nStands on\nnone\nAfter\nIt stores.\nDone when: the owner sees it stored.\n\n**Part Parser**\nStands on: Store\nAfter\nIt parses.\n\nDone when\nIt parses x.\n";

    /// The design's answer cut at its headings: each text a span of the output, the parts each a record, a list of
    /// names its items, an optional part left out none.
    #[test]
    fn a_design_is_cut_at_its_headings() {
        let ps = parts(&design());
        let found = by_headings(ANSWER, &ps).unwrap();
        let loc = json!({"file": "/t.jsonl", "offset": 7, "len": 9});
        let mut a = Map::new();
        fill(&mut a, &ps, &found, ANSWER, Some(&loc));
        let cut = |v: &Value| {
            let s = &v["span"];
            let (at, len) = (s["at"].as_u64().unwrap() as usize, s["len"].as_u64().unwrap() as usize);
            ANSWER[at..at + len].to_string()
        };
        assert_eq!(cut(&a["asked"]), "The owner's words.");
        assert_eq!(cut(&a["today"]), "What is there.\nTwo lines.");
        assert_eq!(a["meaning"], Value::Null);
        let p = a["parts"].as_array().unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0]["name"], json!("Store"));
        assert_eq!(p[0]["stands_on"], json!([]));
        assert_eq!(cut(&p[0]["after"]), "It stores.");
        assert_eq!(cut(&p[0]["done"]), "the owner sees it stored.");
        assert_eq!(p[1]["name"], json!("Parser"));
        assert_eq!(p[1]["stands_on"], json!(["Store"]));
        assert_eq!(cut(&p[1]["done"]), "It parses x.");
        let t = Type::Variant(vec![("Design".into(), design())]);
        let mut v = Value::Object(a);
        v["head"] = json!("Design");
        assert_eq!(t.conforms(&v), Ok(()));
    }

    /// A required heading missing: nothing is cut, and what is missing is named.
    #[test]
    fn a_missing_heading_is_named() {
        let ps = parts(&design());
        let text = ANSWER.replace("## Today\n", "").replace("Done when: the owner sees it stored.\n", "");
        assert_eq!(by_headings(&text, &ps), Err(vec!["today".to_string(), "done of parts 1".to_string()]));
        assert!(by_headings("Asked\nx\nToday\ny\n", &ps).unwrap_err()[0].starts_with("Part NAME"));
    }

    /// The marks reading's answer (the owner, N-101 p648): each passage cut from the answer by its lines, its label
    /// left out; a name and a list of names as given; a reading that cannot be taken says why.
    #[test]
    fn a_design_is_cut_by_its_marks() {
        let text = "Design\n\n**Asked**\nAdd notes/about.txt.\n\n**Today**\nThe repository has no notes.\nNone at all.\n\n**Part about**\n*Stands on:* none.\n*After:* landed holds notes/about.txt.\n*Done when:* the owner runs git show.\n";
        let ps = parts(&design());
        let marks = |after: Value| json!({"asked": {"first": 3, "last": 4}, "today": {"first": 6, "last": 8}, "parts": [{"name": "**about**", "stands_on": ["none"], "after": after, "done": {"first": 13, "last": 13}}], "meaning": null});
        let found = by_marks(&marks(json!({"first": 12, "last": 12})), text, &ps).unwrap();
        let mut a = Map::new();
        fill(&mut a, &ps, &found, text, None);
        assert_eq!(a["asked"], json!("Add notes/about.txt."));
        assert_eq!(a["today"], json!("The repository has no notes.\nNone at all."));
        assert_eq!(a["meaning"], Value::Null);
        let p = &a["parts"][0];
        assert_eq!((&p["name"], &p["stands_on"]), (&json!("about"), &json!([])));
        assert_eq!(p["after"], json!("landed holds notes/about.txt."));
        assert_eq!(p["done"], json!("the owner runs git show."));
        // a label alone on the first line of its lines is left out; a sentence that starts with the field's word is not
        let label = "*Done when*\nThe owner runs it.\nAfter the change.\nIt holds.\n";
        let lfound = by_marks(&json!({"asked": {"first": 1, "last": 2}, "today": {"first": 3, "last": 4}, "parts": [{"name": "x", "stands_on": [], "after": {"first": 3, "last": 4}, "done": {"first": 1, "last": 2}}]}), label, &[("parts".to_string(), parts(&design()).remove(2).1)]).unwrap();
        let mut l = Map::new();
        fill(&mut l, &[("parts".to_string(), parts(&design()).remove(2).1)], &lfound, label, None);
        assert_eq!((&l["parts"][0]["done"], &l["parts"][0]["after"]), (&json!("The owner runs it."), &json!("After the change.\nIt holds.")));
        let t = Type::Variant(vec![("Design".into(), design())]);
        let mut v = Value::Object(a);
        v["head"] = json!("Design");
        assert_eq!(t.conforms(&v), Ok(()));
        // a span of the answer's place, as coordd keeps a text
        let loc = json!({"file": "/t.jsonl", "offset": 7, "len": 9});
        let mut b = Map::new();
        fill(&mut b, &ps, &found, text, Some(&loc));
        let s = &b["asked"]["span"];
        let (at, len) = (s["at"].as_u64().unwrap() as usize, s["len"].as_u64().unwrap() as usize);
        assert_eq!(&text[at..at + len], "Add notes/about.txt.");
        // refused, with why
        let why = |after: Value| by_marks(&marks(after), text, &ps).unwrap_err();
        assert!(why(json!({"first": 12, "last": 11})).contains("comes after"), "{}", why(json!({"first": 12, "last": 11})));
        assert!(why(json!({"first": 12, "last": 40})).contains("whose lines are 1 to 13"));
        assert!(why(json!({"first": 12, "last": 13})).contains("share a line"));
        assert!(by_marks(&json!({"asked": {"first": 3, "last": 4}, "today": {"first": 6, "last": 8}, "parts": []}), text, &ps).unwrap_err().contains("parts has no element"));
        assert!(by_marks(&json!({"asked": null, "today": {"first": 6, "last": 8}, "parts": []}), text, &ps).unwrap_err().contains("asked has no lines"));
    }

    /// A value on the same line as its label (docket-core5's moves-try, `look`: `a_head: 96a5…`) is marked by its line
    /// and its own characters, and taken only where they stand in that line (paper-research N-101).
    #[test]
    fn a_value_beside_its_label_is_given_by_its_line_and_text() {
        let seen = vec![("a_head".to_string(), Type::Text), ("a_status".to_string(), Type::Text), ("b_head".to_string(), Type::Text), ("b_status".to_string(), Type::Text)];
        let ps = parts(&seen);
        let text = "Seen:\na_head: 96a5a1d2640a750410d6036965ca028ab579a07d\na_status: clean\nb_head: 555c3ddecc59e4212059fb268677eb5e002b3e9e\nb_status: clean\n";
        let a = json!({
            "a_head": {"line": 2, "text": "96a5a1d2640a750410d6036965ca028ab579a07d", "first": null, "last": null},
            "a_status": {"line": 3, "text": "clean", "first": null, "last": null},
            "b_head": {"line": 4, "text": "555c3ddecc59e4212059fb268677eb5e002b3e9e", "first": null, "last": null},
            "b_status": {"line": 5, "text": "clean", "first": null, "last": null},
        });
        let found = by_marks(&a, text, &ps).unwrap();
        let mut o = Map::new();
        fill(&mut o, &ps, &found, text, None);
        assert_eq!(Value::Object(o), json!({"a_head": "96a5a1d2640a750410d6036965ca028ab579a07d", "a_status": "clean", "b_head": "555c3ddecc59e4212059fb268677eb5e002b3e9e", "b_status": "clean"}));
        // two fields on one line, each its own piece of it
        let text = "Seen: a_head 96a5a1d, a_status clean\nb_head 555c3dd, b_status dirty\n";
        let a = json!({"a_head": {"line": 1, "text": "96a5a1d"}, "a_status": {"line": 1, "text": "clean"}, "b_head": {"line": 2, "text": "555c3dd"}, "b_status": {"line": 2, "text": "dirty"}});
        let found = by_marks(&a, text, &ps).unwrap();
        let mut o = Map::new();
        fill(&mut o, &ps, &found, text, None);
        assert_eq!((&o["a_head"], &o["a_status"], &o["b_status"]), (&json!("96a5a1d"), &json!("clean"), &json!("dirty")));
        // a text that is not in its line is refused, with why; so is one that overlaps another part, or whole lines
        let why = |a_head: Value| by_marks(&json!({"a_head": a_head, "a_status": {"line": 1, "text": "clean"}, "b_head": {"line": 2, "text": "555c3dd"}, "b_status": {"line": 2, "text": "dirty"}}), text, &ps).unwrap_err();
        assert_eq!(why(json!({"line": 2, "text": "96a5a1d"})), "a_head: \"96a5a1d\" is not in line 2; give the value's characters exactly as they stand in that line");
        assert!(by_marks(&json!({"a_head": {"line": 1, "text": "96a5"}, "a_status": {"line": 1, "text": "clean"}, "b_head": {"line": 2, "text": "555c3dd"}, "b_status": {"line": 2, "text": "dirty"}}), text, &ps).is_ok(), "a piece of the hash is the agent's characters too");
        assert!(why(json!({"line": 1, "text": "clean"})).contains("overlap"), "{}", why(json!({"line": 1, "text": "clean"})));
        assert_eq!(why(json!({"first": 1, "last": 1})), "a_status (in line 1) and a_head (lines 1 to 1) overlap; no characters are in two parts");
        assert!(why(json!({"line": 9, "text": "x"})).contains("whose lines are 1 to 2"));
        // neither form: none, which a required part may not be
        assert!(why(json!({"first": null, "last": null, "line": null, "text": null})).contains("a_head has no lines"));
    }

    /// C-1's design on the fresh run (docket-core3, N-101): `asked` and `today` under labels with words in brackets,
    /// and `after` begun on its `- **after:**` line. The label's own line is left out by the marks; the part begun on
    /// its label's line is taken from the characters the mark gives; ones not in that line are refused.
    #[test]
    fn a_part_begun_on_its_labels_line_is_taken_from_where_the_mark_says() {
        let ps = parts(&design());
        let text = "Design\n\n**asked** (the owner's words, exactly as typed):\n> Add notes/about.txt.\n\n**today** (checked at a's head):\nThe repository has no notes.\n\n**Part about**\n- **stands on:** none\n- **after:** Repository a's landed holds notes/about.txt.\n  The implementer answers \"Commit a <hash>\".\n- **done when:** the owner runs git show landed:notes/about.txt.\n";
        let marks = |from: &str| json!({"asked": {"first": 4, "last": 4}, "today": {"first": 7, "last": 7}, "parts": [{"name": "about", "stands_on": [], "after": {"first": 11, "last": 12, "from": from}, "done": {"line": 13, "text": "the owner runs git show landed:notes/about.txt."}}], "meaning": null});
        let found = by_marks(&marks("Repository a's"), text, &ps).unwrap();
        let mut o = Map::new();
        fill(&mut o, &ps, &found, text, None);
        assert_eq!(o["asked"], json!("> Add notes/about.txt."));
        assert_eq!(o["today"], json!("The repository has no notes."));
        assert_eq!(o["parts"][0]["after"], json!("Repository a's landed holds notes/about.txt.\n  The implementer answers \"Commit a <hash>\"."));
        assert_eq!(o["parts"][0]["done"], json!("the owner runs git show landed:notes/about.txt."));
        assert_eq!(by_marks(&marks("Repository b's"), text, &ps).unwrap_err(), "after of parts 1: \"Repository b's\" is not in line 11; give the characters the part begins with exactly as they stand in its first line");
        // a `from` of null or "" is no `from`: the lines as they are
        assert!(by_marks(&marks(""), text, &ps).is_ok());
    }

    /// The marks reading is given the answer once, numbered, and asked for lines and names, never the text again.
    #[test]
    fn the_marks_reading_is_given_the_answer_numbered() {
        let ps = vec![("Design".to_string(), parts(&design()))];
        let p = marks_prompt(&["Design".to_string(), "Ask (question: Text)".to_string()], "None", &ps, "Design\nAsked\nx", None, Some("Design the change."), Some("This step gives one of:\n  Design  a design\n  Ask"));
        assert!(p.ends_with("The agent's answer, its lines numbered:\n<<<\n1  Design\n2  Asked\n3  x\n>>>"), "{p}");
        assert!(p.contains("What the step gives, as the agent was shown it:\nThis step gives one of:\n  Design  a design\n  Ask\n\nHow each field is given:\n- Design: by their lines: asked, today, meaning; null where the answer has none: meaning; parts: one element for each of the answer's parts, in its order; in each, by their lines: after, done; as values: name as the answer writes it, without the marks around it; stands_on as the names it lists, [] where it says there are none\n"), "{p}");
        assert!(p.contains("---\nDesign the change.\n---") && !p.to_lowercase().contains("first word") && !p.contains("<part") && p.contains("a line may be in none"), "{p}");
        assert!(p.contains(&format!("(a part on one line has that line as both). {PIECE} {LABEL} No line is in two parts")), "{p}");
        assert_eq!(numbered("a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n").lines().next(), Some(" 1  a"));
    }

    /// C-1's layout (docket-core3 N-101 p601): headings in marks with the value after the colon on their line, a part
    /// that stands on nothing, and a `<hash>` in a value. All cut at the headings; the marks closing a heading are not
    /// the value's.
    #[test]
    fn a_design_with_its_values_beside_its_headings_is_cut() {
        let text = "Design\n\n**Asked**\nAdd notes/about.txt.\n\n**Today**\nThe repository has no notes.\n\n**Part about**\n\n*Stands on:* none.\n\n*After:*\n- landed holds notes/about.txt; the implementer answers \"Commit a <hash>\".\n\n*Done when:* the owner runs git show landed:notes/about.txt.\n";
        let ps = parts(&design());
        let found = by_headings(text, &ps).unwrap();
        let mut a = Map::new();
        fill(&mut a, &ps, &found, text, None);
        assert_eq!(a["asked"], json!("Add notes/about.txt."));
        let part = &a["parts"][0];
        assert_eq!((part["name"].clone(), part["stands_on"].clone()), (json!("about"), json!([])), "{part}");
        assert_eq!(part["after"], json!("- landed holds notes/about.txt; the implementer answers \"Commit a <hash>\"."));
        assert_eq!(part["done"], json!("the owner runs git show landed:notes/about.txt."));
    }
}
