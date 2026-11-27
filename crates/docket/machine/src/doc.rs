//! What a kind or a call says of itself, as one text: its signature and its doc (`///`). One source for coordd's
//! shape prompt (`shape.rs`: the kinds a minder may open, the calls a step may make, a ticket's results) and for the
//! language server's hover (N-101 p646). Pure: each takes the file's text, the caller reads it.

use crate::check::CallSig;
use crate::syntax::{CallDef, Doc, Kind, Named};
use crate::types::Type;
use std::collections::{BTreeMap, BTreeSet};

fn fields(fs: &[(String, Type)]) -> String {
    fs.iter().map(|(n, t)| format!("{n}: {t}")).collect::<Vec<_>>().join(", ")
}

/// A kind as a minder opens it: `fix(what: Text, ...) -> Ref, raises NoFix`.
pub fn kind_sig(k: &Kind) -> String {
    let raises = if k.raises.is_empty() { String::new() } else { format!(", raises {}", k.raises.join(", ")) };
    format!("{}({}) -> {}{raises}", k.name, fields(&k.form), k.result)
}

/// A kind's line in the shape prompt: its signature, and on the next line its doc (`///`), when it has one.
pub fn kind_line(k: &Kind) -> String {
    let what = Some(one_line(&k.doc.text)).filter(|d| !d.is_empty()).map(|p| format!("\n  {p}")).unwrap_or_default();
    format!("{}{what}", kind_sig(k))
}

/// A call as a step makes it: `assign(words: Text) -> Assigned(agent: Agent) | Ask(question: Text)`.
pub fn call_sig(c: &CallSig) -> String {
    format!("{}({}) -> {}", c.name, fields(&c.params), c.head)
}

/// A call as written: its signature with its `where`, then its attribute lines (`asks Ask`, `task assign`), each on a
/// line of its own.
pub fn call_def(c: &CallDef) -> String {
    let s = &c.sig;
    let cond = s.cond.as_ref().map(|w| format!(" where {w}")).unwrap_or_default();
    let mut out = format!("{}({}){cond} -> {}", s.name, fields(&s.params), s.head);
    for (w, rest) in &c.attrs {
        out.push_str(&format!("\n  {w} {rest}").trim_end().to_string());
    }
    if c.template.is_some() {
        out.push_str("\n  template ...");
    }
    out
}

/// K's results, one a line, each with its doc (`///` on the result's alternative) when it has one.
pub fn results(k: &Kind) -> String {
    let Type::Variant(cs) = &k.result else { return format!("{}", k.result) };
    cs.iter()
        .map(|(c, fs)| {
            let head = if fs.is_empty() { c.clone() } else { format!("{c}({})", fields(fs)) };
            match k.doc.parts.get(c) {
                Some(m) => format!("{head}: {}", one_line(m)),
                None => head,
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A doc's lines as one line.
fn one_line(d: &str) -> String {
    d.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ")
}

/// The declared record or sum type T is (NAMED), or a list or option of one: its name and the type.
fn declared<'a>(t: &Type, named: &'a Named) -> Option<(&'a str, &'a Type)> {
    match t {
        Type::List(t) | Type::Opt(t) => declared(t, named),
        Type::Record(_) | Type::Variant(_) => named.iter().find(|(_, d)| *d == t).map(|(n, d)| (n.as_str(), d)),
        _ => None,
    }
}

/// T as written: a declared record or sum by its name (`[Part]`, not its fields).
fn written(t: &Type, named: &Named) -> String {
    match t {
        Type::List(x) => format!("[{}]", written(x, named)),
        Type::Opt(x) => format!("{}?", written(x, named)),
        Type::Record(_) | Type::Variant(_) => declared(t, named).map_or_else(|| t.to_string(), |(n, _)| n.to_string()),
        t => t.to_string(),
    }
}

/// What a declared type T has under it, at INDENT: a record's fields or a sum's alternatives, with DOCS (its
/// `type`'s); each declared type once in a rendering (SEEN).
fn parts_given(t: &Type, docs: Option<&Doc>, indent: usize, at: &At, seen: &mut BTreeSet<String>, out: &mut Vec<String>) {
    match t {
        Type::Record(fs) => fields_given(fs, docs, indent, at, seen, out),
        Type::Variant(cs) => {
            let wide = cs.iter().map(|(c, _)| c.chars().count()).max().unwrap_or(0);
            for (c, fs) in cs {
                out.push(match docs.and_then(|d| d.parts.get(c)) {
                    Some(d) => format!("{:indent$}{c:<wide$}  {}", "", one_line(d)),
                    None => format!("{:indent$}{c}", ""),
                });
                fields_given(fs, None, indent + 2, &at.under(c), seen, out);
            }
        }
        _ => {}
    }
}

/// Where a rendering is: the place in the answer (`given::place`) and what is said at a place of it.
struct At<'a> {
    place: Vec<String>,
    types: &'a BTreeMap<String, Doc>,
    named: &'a Named,
    /// By place, the line under an agent's field: what it is given (`given_lines`).
    given: &'a BTreeMap<String, String>,
}

impl<'a> At<'a> {
    fn under(&self, seg: &str) -> At<'a> {
        At { place: self.place.iter().cloned().chain([seg.to_string()]).collect(), ..*self }
    }
}

/// FIELDS under an outcome or a field, at INDENT: each with its type beside it unless it is plain Text, and its doc
/// (DOCS); a field of a declared record or sum type, or a list or option of one, has that type's fields or
/// alternatives under it, each type once in a rendering (SEEN).
fn fields_given(fs: &[(String, Type)], docs: Option<&Doc>, indent: usize, at: &At, seen: &mut BTreeSet<String>, out: &mut Vec<String>) {
    let (types, named) = (at.types, at.named);
    let label = |f: &str, t: &Type| if *t == Type::Text { f.to_string() } else { format!("{f}: {}", written(t, named)) };
    let wide = fs.iter().map(|(f, t)| label(f, t).chars().count()).max().unwrap_or(0);
    for (f, t) in fs {
        let l = label(f, t);
        out.push(match docs.and_then(|d| d.parts.get(f)) {
            Some(d) => format!("{:indent$}{l:<wide$}  {}", "", one_line(d)),
            None => format!("{:indent$}{l}", ""),
        });
        let here = at.under(f);
        if let Some(g) = at.given.get(&crate::given::place(&here.place)) {
            out.push(format!("{:w$}{g}", "", w = indent + wide + 2));
        }
        if let Some((n, inner)) = declared(t, named) {
            if seen.insert(n.to_string()) {
                // a list's element is one place further (`picks[].agent`)
                let here = if matches!(t, Type::List(_)) || matches!(t, Type::Opt(x) if matches!(**x, Type::List(_))) { here.under("[]") } else { here };
                parts_given(inner, types.get(n), indent + 2, &here, seen, out);
            }
        }
    }
}

/// A declared type T's fields or alternatives, each with its doc (DOCS), and the declared types under them, as
/// `gives` writes them: for the hover.
pub fn type_parts(name: &str, t: &Type, docs: Option<&Doc>, types: &BTreeMap<String, Doc>, named: &Named) -> String {
    let (mut seen, mut out) = (BTreeSet::from([name.to_string()]), Vec::new());
    let none = BTreeMap::new();
    parts_given(t, docs, 2, &At { place: Vec::new(), types, named, given: &none }, &mut seen, &mut out);
    out.join("\n")
}

/// What a step whose answer is HEAD gives, with the docs (paper-research N-101 p656, p698): each outcome with its
/// doc, its fields under it with theirs, and under a field of a declared record type that type's fields. An
/// outcome's doc and its fields' are its declared type's (TYPES, by name: `syntax::type_docs`), else the call's
/// (CALL: an outcome written inline). A step that gives a plain value (`Text`) gets nothing: its task says it all.
/// One rendering, for the step's text, the reminder, the refusal, the reading call and the hover.
pub fn gives(head: &Type, call: Option<&Doc>, types: &BTreeMap<String, Doc>, named: &Named) -> String {
    gives_with(head, call, types, named, &BTreeMap::new())
}

/// `gives`, with a line under each agent's field that GIVEN has for its place (`given_lines`): what the agent named
/// there is given later (paper-research N-101 p980).
pub fn gives_with(head: &Type, call: Option<&Doc>, types: &BTreeMap<String, Doc>, named: &Named, given: &BTreeMap<String, String>) -> String {
    let Type::Variant(cs) = head else { return String::new() };
    let mut out = vec![String::from(if cs.len() == 1 { "This step gives:" } else { "This step gives one of:" })];
    let wide = cs.iter().map(|(c, _)| c.chars().count()).max().unwrap_or(0);
    let mut seen = BTreeSet::new();
    for (c, fs) in cs {
        let ty = types.get(c);
        let doc = ty.map(|d| d.text.as_str()).filter(|d| !d.is_empty()).or_else(|| call.and_then(|d| d.parts.get(c)).map(String::as_str));
        out.push(match doc {
            Some(d) => format!("  {c:<wide$}  {}", one_line(d)),
            None => format!("  {c}"),
        });
        seen.insert(c.clone());
        // an outcome written inline: its fields' docs are the call's, by `Outcome.field`
        let inline = ty.is_none().then(|| Doc { text: String::new(), parts: call.map(|d| d.parts.iter().filter_map(|(k, v)| Some((k.strip_prefix(&format!("{c}."))?.to_string(), v.clone()))).collect()).unwrap_or_default() });
        let at = At { place: vec![c.clone()], types, named, given };
        fields_given(fs, ty.or(inline.as_ref()), 4, &at, &mut seen, &mut out);
    }
    out.join("\n")
}

/// The line under each place of the answer to a step of CALL in KIND whose agent is given steps later
/// (`given::agents_given`, GIVEN):
/// "it is given: " and each call by its doc's first line (CALL_DOCS, else its name), its class after it in brackets
/// as the call's `where` names it (`model >= large`: large).
pub fn given_lines(kind: &str, call: &str, given: &crate::given::Given, calls: &BTreeMap<String, CallSig>, call_docs: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let Some(places) = given.get(&(kind.to_string(), call.to_string())) else { return BTreeMap::new() };
    places.iter().filter(|(_, cs)| !cs.is_empty()).map(|(p, cs)| (p.clone(), format!("it is given: {}", cs.iter().map(|c| given_one(c, calls, call_docs)).collect::<Vec<_>>().join(", ")))).collect()
}

/// By place, the line under a list of an answer to CALL whose entries name another list's elements (NAMES,
/// `given::names_given`, in KIND; none: in any kind): which elements, by which field, written as the answer gives
/// them (paper-research N-101 p1141: run 4's designer wrote `stands_on: Part 1` beside a part named without it).
pub fn names_lines(kind: Option<&str>, call: &str, names: &crate::given::Names) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for ((k, c), rs) in names {
        if c != call || kind.is_some_and(|x| x != k) {
            continue;
        }
        for (list, of) in rs {
            let segs: Vec<&str> = of.split(['.', '[', ']']).filter(|s| !s.is_empty()).collect();
            let (Some(h), Some(f)) = (segs.last(), segs.len().checked_sub(2).and_then(|i| segs.get(i))) else { continue };
            out.insert(list.clone(), format!("each is one of the {f}' {h}s, letter for letter"));
        }
    }
    out
}

/// Call C as `given_lines` names it: its doc's first line, and its class.
pub fn given_one(c: &str, calls: &BTreeMap<String, CallSig>, call_docs: &BTreeMap<String, String>) -> String {
    let what = call_docs.get(c).cloned().unwrap_or_else(|| c.to_string());
    match calls.get(c).and_then(|s| s.cond.as_deref()) {
        Some(w) => format!("{what} ({})", class_of(w)),
        None => what,
    }
}

/// The class a `where` asks for, as it names it: `model >= large` is large; any other condition as written.
pub fn class_of(cond: &str) -> String {
    let c = cond.trim();
    c.strip_prefix("model").map(str::trim_start).and_then(|r| r.strip_prefix(">=")).map(str::trim).filter(|r| r.chars().all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '-')).unwrap_or(c).to_string()
}

#[cfg(test)]
mod tests {
    use crate::syntax::{Tok, lex, lex_spans};

    #[test]
    fn a_token_says_where_it_is_and_lex_is_unchanged() {
        let text = "kind fix\n  pattern\n    x <- step w \"two\nlines\" ;  open é(a)";
        let s = lex_spans(text, 1).unwrap();
        let at = |w: &str| s.iter().find(|t| t.tok == Tok::Ident(w.into())).map(|t| (t.from, t.col, t.len));
        assert_eq!(at("fix"), Some((1, 5, 3)));
        assert_eq!(at("step"), Some((3, 9, 4)));
        // a string over two lines: where it starts; `lex` gave (and gives) its last line
        let st = s.iter().find(|t| matches!(t.tok, Tok::Str(_))).unwrap();
        assert_eq!((st.from, st.col, st.len, st.line), (3, 16, 11, 4));
        assert_eq!(at("é"), Some((4, 15, 1)));
        assert_eq!(lex(text, 1).unwrap(), s.iter().map(|t| (t.tok.clone(), t.line)).collect::<Vec<_>>());
    }
}
