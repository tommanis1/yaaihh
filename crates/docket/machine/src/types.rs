//! Types and values. A value is JSON (`serde_json::Value`), one encoding everywhere (plan/interface.md §1): a record
//! is an object; a variant is an object whose `head` names its constructor, with the constructor's fields beside it
//! (`{"head": "DoesNot", "why": "..."}`); `none` is `null`; a tuple is an array; `Ref`, `Agent` and `Ticket` are
//! strings. A `Text` is a string (a new text, written once) or `{"loc": {"file", "offset", "len"}}` (a place in a
//! transcript), or `{"part": {"t", "at", "len"}}` (a span of a ticket's text), or `{"span": {"loc", "at", "len"}}` (a span of the
//! decoded text at a place): texts are passed by reference
//! (owner m613).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fmt;

/// A place in a file, in bytes: where a text already is (a transcript entry), so it is never copied.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Loc {
    pub file: String,
    pub offset: u64,
    pub len: u64,
}

/// A text: new (it exists nowhere else, so it is written here once) or a reference to where it is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Text {
    New(String),
    Loc { loc: Loc },
    /// A span of another ticket's text (the owner's words on a filing, posted by the rule File).
    Part { part: Part },
    /// A span of the decoded text at a Loc: bytes AT to AT+LEN of what the place says, not of the file (a turn's text
    /// in a transcript has JSON escapes and several text items, so file bytes would cut it wrong). What a `Part` of a
    /// text held as a Loc resolves to; the surfaces read the place and cut it.
    Span { span: Span },
    /// Post N of ticket T: its words wherever they are (`post N` in a program). The machine resolves it.
    Post { post: PostRef },
    /// A text made of pieces, read one after the other: `text of` over a value that holds texts by reference (an
    /// agent's answer in its session), its headings new words and its texts where they are (docket-core3, N-101: a
    /// hand-up's `text of {log, found, ...}` raised NoResult). The surfaces read each piece.
    Joined { joined: Vec<Text> },
}

/// Post N of ticket T (`Text::Post`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PostRef {
    pub t: u64,
    pub n: u32,
}

/// A span of the text at LOC (`Text::Span`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub loc: Loc,
    pub at: u64,
    pub len: u64,
}

/// Bytes AT to AT+LEN of ticket T's form field `text`. The machine resolves it (`Machine::text`), as only it knows
/// the ticket.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Part {
    pub t: u64,
    pub at: u64,
    pub len: u64,
}

impl Text {
    pub fn of(v: &Value) -> Option<Text> {
        serde_json::from_value(v.clone()).ok()
    }
    pub fn value(&self) -> Value {
        serde_json::to_value(self).expect("a text is JSON")
    }
}

/// The key of the marker the word `agents` evaluates to (`Type::Agents`).
pub const AGENTS: &str = "agents";

/// The value of the word `agents`: a marker, filled in when the step is handed over (item 11, G1).
pub fn agents_marker() -> Value {
    json!({AGENTS: "project"})
}

/// Whether V is the marker of the word `agents`.
pub fn is_agents(v: &Value) -> bool {
    v.as_object().is_some_and(|o| o.len() == 1 && o.get(AGENTS).and_then(Value::as_str) == Some("project"))
}

/// A type of the language (syntax.tex §1.1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Type {
    Text,
    Int,
    Bool,
    Ref,
    Agent,
    /// The project's agents (item 11, G1): the word `agents`, a marker in the record (`{"agents": "project"}`) that
    /// coordd fills in with one line per agent when the step is handed over, never written into any line.
    Agents,
    Ticket,
    List(Box<Type>),
    Opt(Box<Type>),
    Tuple(Vec<Type>),
    Record(Vec<(String, Type)>),
    /// A choice of constructors, each with its fields in order.
    Variant(Vec<(String, Vec<(String, Type)>)>),
    /// What an exception carries: any value (the types do not track exceptions' payloads).
    Any,
}

/// A `Ref` as an answer writes it: one commit as it is; commits by repository each as `REPO COMMIT`, joined by ", "
/// (`a 02e0410, b 1a2b3c4`). None for a value that is no Ref.
pub fn ref_words(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Object(o) if !o.is_empty() => o.iter().map(|(r, c)| c.as_str().map(|c| format!("{r} {c}"))).collect::<Option<Vec<_>>>().map(|w| w.join(", ")),
        _ => None,
    }
}

impl Type {
    pub fn unit() -> Type {
        Type::Tuple(Vec::new())
    }

    /// The constructor C of this variant type: its fields.
    pub fn constructor(&self, c: &str) -> Option<&Vec<(String, Type)>> {
        match self {
            Type::Variant(cs) => cs.iter().find(|(n, _)| n == c).map(|(_, f)| f),
            _ => None,
        }
    }

    /// Whether a value of type SELF may stand where EXPECTED is wanted: equal, or a variant whose constructors are
    /// among EXPECTED's with fields that start with theirs (more parts may follow: N-101, a replaced call's answer
    /// carries extra parts), a record likewise, or `Any` on either side, or a non-optional where an optional is wanted.
    pub fn fits(&self, expected: &Type) -> bool {
        match (self, expected) {
            (_, Type::Any) | (Type::Any, _) => true,
            (Type::Variant(a), Type::Variant(b)) => a.iter().all(|(n, f)| b.iter().any(|(m, g)| n == m && fields_fit(f, g))),
            (Type::Opt(a), Type::Opt(b)) => a.fits(b),
            (a, Type::Opt(b)) => a.fits(b),
            (Type::List(a), Type::List(b)) => a.fits(b),
            (Type::Tuple(a), Type::Tuple(b)) => a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.fits(y)),
            (Type::Record(a), Type::Record(b)) => fields_fit(a, b),
            (a, b) => a == b,
        }
    }

    /// Whether V is a value of this type; why not otherwise.
    pub fn conforms(&self, v: &Value) -> Result<(), String> {
        let bad = || Err(format!("{} is not {}", short(v), self));
        match self {
            Type::Any => Ok(()),
            Type::Text => match Text::of(v) {
                Some(_) => Ok(()),
                None => bad(),
            },
            // in a project of several repositories a Ref that stands for code may be commits by repository, an object
            // `{"tovrin": "a1b2c3d", "p4": "9f8e7d6"}` (N-101)
            Type::Ref if v.as_object().is_some_and(|o| !o.is_empty() && o.values().all(|c| c.as_str().is_some_and(|s| !s.trim().is_empty()))) => Ok(()),
            Type::Ref | Type::Agent | Type::Ticket => {
                if v.as_str().is_some_and(|s| !s.trim().is_empty()) { Ok(()) } else { bad() }
            }
            Type::Agents => if v.get(AGENTS).is_some() || v.is_string() { Ok(()) } else { bad() },
            Type::Int => if v.is_i64() { Ok(()) } else { bad() },
            Type::Bool => if v.is_boolean() { Ok(()) } else { bad() },
            Type::Opt(t) => if v.is_null() { Ok(()) } else { t.conforms(v) },
            Type::List(t) => match v.as_array() {
                Some(xs) => xs.iter().try_for_each(|x| t.conforms(x)),
                None => bad(),
            },
            Type::Tuple(ts) => match v.as_array() {
                Some(xs) if xs.len() == ts.len() => ts.iter().zip(xs).try_for_each(|(t, x)| t.conforms(x)),
                _ => bad(),
            },
            Type::Record(fs) => match v.as_object() {
                Some(o) => fs.iter().try_for_each(|(f, t)| t.conforms(o.get(f).unwrap_or(&Value::Null)).map_err(|e| format!("{f}: {e}"))),
                None => bad(),
            },
            Type::Variant(cs) => {
                let Some(head) = v.get("head").and_then(Value::as_str) else { return bad() };
                let Some((_, fs)) = cs.iter().find(|(n, _)| n == head) else {
                    return Err(format!("{head} is not one of {}", cs.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ")));
                };
                fs.iter().try_for_each(|(f, t)| t.conforms(v.get(f).unwrap_or(&Value::Null)).map_err(|e| format!("{head}.{f}: {e}")))
            }
        }
    }

    /// V with what was typed as a string read as this type wants (a number for `Int`, `true`/`false` for `Bool`,
    /// through records, variants, options and lists); why not, naming the field.
    pub fn coerce(&self, v: &Value) -> Result<Value, String> {
        Ok(match (self, v) {
            (Type::Int, Value::String(s)) => json!(s.trim().parse::<i64>().map_err(|_| format!("{s:?} is not a number"))?),
            (Type::Bool, Value::String(s)) => match s.trim() {
                "true" | "yes" => json!(true),
                "false" | "no" => json!(false),
                _ => return Err(format!("{s:?} is not true or false")),
            },
            // commits by repository written on a command line: `tovrin=a1b2c3d,p4=9f8e7d6`
            (Type::Ref, Value::String(s)) if s.contains('=') && s.split(',').all(|p| p.split_once('=').is_some_and(|(r, c)| !r.trim().is_empty() && !c.trim().is_empty())) => {
                Value::Object(s.split(',').filter_map(|p| p.split_once('=')).map(|(r, c)| (r.trim().to_string(), json!(c.trim()))).collect())
            }
            // commits by repository written on a command line as JSON: `{"a": "8f28eb2", "b": "2bcc31b"}` (N-101.p64)
            (Type::Ref, Value::String(s)) if s.trim_start().starts_with('{') => match serde_json::from_str::<Value>(s) {
                Ok(o @ Value::Object(_)) if Type::Ref.conforms(&o).is_ok() => o,
                _ => return Err(format!("{s:?} is not commits by repository ({{\"REPO\": \"SHA\"}})")),
            },
            // a constructor written on a command line: `SetBound(runtime, run-seconds, 12)`, `Rest` (N-101 post 27, H)
            (Type::Variant(cs), Value::String(s)) if crate::runner::constructor_text(cs, s).is_some() => crate::runner::constructor_text(cs, s).unwrap()?,
            (Type::Opt(_), Value::Null) => Value::Null,
            (Type::Opt(t), v) => t.coerce(v)?,
            (Type::List(t), Value::Array(xs)) => Value::Array(xs.iter().map(|x| t.coerce(x)).collect::<Result<_, _>>()?),
            // a list given on a command line as JSON (`--parts '[{"name": "about", ...}]'`): read whole, commas inside
            // its values kept (paper-research N-101 p602, 2)
            (Type::List(t), Value::String(s)) if matches!(serde_json::from_str::<Value>(s.trim()), Ok(Value::Array(_))) => {
                let Ok(Value::Array(xs)) = serde_json::from_str::<Value>(s.trim()) else { unreachable!() };
                Value::Array(xs.iter().map(|x| t.coerce(x)).collect::<Result<_, _>>()?)
            }
            // a record given on a command line: as JSON, else it cannot be read
            (Type::Record(fs), Value::String(s)) => match serde_json::from_str::<Value>(s.trim()) {
                Ok(o @ Value::Object(_)) => self.coerce(&o)?,
                _ => {
                    let names: Vec<String> = fs.iter().map(|(f, t)| format!("\"{f}\": {t}")).collect();
                    return Err(format!("{:?} is not a record: one is written as JSON, {{{}}}", cut(s), names.join(", ")));
                }
            },
            // a list given on a command line (`coord answer O-1 Send --to lead,fixer`): split on commas
            (Type::List(t), Value::String(s)) => Value::Array(s.split(',').map(str::trim).filter(|x| !x.is_empty()).map(|x| t.coerce(&json!(x))).collect::<Result<_, _>>()?),
            (Type::Record(fs), Value::Object(o)) => {
                let mut o = o.clone();
                for (f, t) in fs {
                    if let Some(x) = o.get(f) {
                        let c = t.coerce(x).map_err(|e| format!("{f}: {e}"))?;
                        o.insert(f.clone(), c);
                    }
                }
                Value::Object(o)
            }
            (Type::Variant(_), Value::Object(o)) => {
                let mut o = o.clone();
                if let Some(fs) = o.get("head").and_then(Value::as_str).and_then(|h| self.constructor(h)) {
                    for (f, t) in fs {
                        if let Some(x) = o.get(f) {
                            let c = t.coerce(x).map_err(|e| format!("{f}: {e}"))?;
                            o.insert(f.clone(), c);
                        }
                    }
                }
                Value::Object(o)
            }
            (_, v) => v.clone(),
        })
    }

    /// The JSON schema of a value of this type (for structured output: a forming call, a fresh call). A `Text` is a
    /// string there: what a model writes is new.
    pub fn schema(&self) -> Value {
        match self {
            // commits by repository are written as `tovrin=a1b2c3d,p4=9f8e7d6` here (coerce makes the object): a strict
            // structured output takes no object of open keys
            Type::Text | Type::Ref | Type::Agent | Type::Agents | Type::Ticket => json!({"type": "string"}),
            Type::Int => json!({"type": "integer"}),
            Type::Bool => json!({"type": "boolean"}),
            Type::Any => json!({}),
            Type::Opt(t) => json!({"anyOf": [t.schema(), {"type": "null"}]}),
            Type::List(t) => json!({"type": "array", "items": t.schema()}),
            Type::Tuple(ts) => json!({"type": "array", "prefixItems": ts.iter().map(Type::schema).collect::<Vec<_>>(), "minItems": ts.len(), "maxItems": ts.len()}),
            Type::Record(fs) => object_schema(None, fs),
            Type::Variant(cs) => json!({"anyOf": cs.iter().map(|(n, fs)| object_schema(Some(n), fs)).collect::<Vec<_>>()}),
        }
    }
}

/// S as said back in an error: its first 60 characters.
fn cut(s: &str) -> String {
    match s.char_indices().nth(60) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

fn object_schema(head: Option<&str>, fs: &[(String, Type)]) -> Value {
    let mut props = serde_json::Map::new();
    let mut required = Vec::new();
    if let Some(h) = head {
        props.insert("head".into(), json!({"type": "string", "enum": [h]}));
        required.push(json!("head"));
    }
    for (f, t) in fs {
        props.insert(f.clone(), t.schema());
        required.push(json!(f));
    }
    json!({"type": "object", "properties": props, "required": required, "additionalProperties": false})
}

/// A's fields start with B's, each of the same name and fitting; A may have more after them.
fn fields_fit(a: &[(String, Type)], b: &[(String, Type)]) -> bool {
    a.len() >= b.len() && a.iter().zip(b).all(|((f, x), (g, y))| f == g && x.fits(y))
}

fn short(v: &Value) -> String {
    let s = v.to_string();
    if s.chars().count() > 60 { format!("{}...", s.chars().take(60).collect::<String>()) } else { s }
}

fn fields(f: &mut fmt::Formatter<'_>, fs: &[(String, Type)]) -> fmt::Result {
    for (i, (n, t)) in fs.iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{n}: {t}")?;
    }
    Ok(())
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Text => write!(f, "Text"),
            Type::Int => write!(f, "Int"),
            Type::Bool => write!(f, "Bool"),
            Type::Ref => write!(f, "Ref"),
            Type::Agent => write!(f, "Agent"),
            Type::Agents => write!(f, "Agents"),
            Type::Ticket => write!(f, "Ticket"),
            Type::Any => write!(f, "Any"),
            Type::List(t) => write!(f, "[{t}]"),
            Type::Opt(t) => write!(f, "{t}?"),
            Type::Tuple(ts) => {
                write!(f, "(")?;
                for (i, t) in ts.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{t}")?;
                }
                write!(f, ")")
            }
            Type::Record(fs) => {
                write!(f, "{{")?;
                fields(f, fs)?;
                write!(f, "}}")
            }
            Type::Variant(cs) => {
                for (i, (n, fs)) in cs.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    write!(f, "{n}")?;
                    if !fs.is_empty() {
                        write!(f, "(")?;
                        fields(f, fs)?;
                        write!(f, ")")?;
                    }
                }
                Ok(())
            }
        }
    }
}
