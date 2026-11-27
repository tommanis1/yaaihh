//! What an answer carries beside its parts (N-101, docket-core3): the agent that gave it (`by of d`) and the whole of
//! it as one text (`text of d`). When a step's answer is taken (`Machine::accept`), the value the program goes on with
//! has `@by` and `@text`; they are never written on a line (replay makes them again in accept), never matched (names
//! are matched), and dropped wherever a value leaves the engine as data: a step's arguments (the runner's, a template's
//! holes), a closed ticket's result and a raise's value on the record, `==`, and the views.

use crate::syntax::Expr;
use crate::types::{Text, Type};
use serde_json::{Value, json};

/// The key of the agent that gave an answer.
pub const BY: &str = "@by";
/// The key of the whole answer as one text.
pub const TEXT: &str = "@text";

/// HEAD, the answer BY gave, as the program goes on with it: with who gave it and the whole of it (TEXT, the turn's
/// output by reference, or the owner's words). A value that is no record or variant (a text) carries nothing.
pub fn carried(head: Value, by: &str, text: &Text) -> Value {
    match head {
        // a text answered (`-> Text`) is a reference to the output: it stays one (N-159)
        Value::Object(o) if Text::of(&Value::Object(o.clone())).is_some() && !o.contains_key("head") => Value::Object(o),
        Value::Object(mut o) => {
            o.insert(BY.into(), json!(by));
            o.insert(TEXT.into(), text.value());
            Value::Object(o)
        }
        v => v,
    }
}

/// V without what answers carry, anywhere in it.
pub fn plain(v: Value) -> Value {
    match v {
        Value::Object(o) if o.contains_key(BY) || o.contains_key(TEXT) || o.values().any(nested) => {
            Value::Object(o.into_iter().filter(|(k, _)| k != BY && k != TEXT).map(|(k, x)| (k, plain(x))).collect())
        }
        Value::Array(xs) if xs.iter().any(nested) => Value::Array(xs.into_iter().map(plain).collect()),
        v => v,
    }
}

fn nested(v: &Value) -> bool {
    match v {
        Value::Object(o) => o.contains_key(BY) || o.contains_key(TEXT) || o.values().any(nested),
        Value::Array(xs) => xs.iter().any(nested),
        _ => false,
    }
}

/// `by of V`: the agent that gave the answer V, none when no step gave it.
pub fn by_of(v: &Value) -> Value {
    v.get(BY).cloned().unwrap_or(Value::Null)
}

/// `text of V`, V of type TY: the whole answer as given; for a value no step gave, or an answer given with its fields
/// and no text, its fields in order, each under
/// its heading (the field's name, `_` a space, a capital first letter), a list of records each element under its first
/// field. A text in it held by reference stays where it is: the whole is then a `Text::Joined` of the new words and
/// those places, which the surfaces read (docket-core3, N-101). None when V is no record or variant.
pub fn text_of(v: &Value, ty: Option<&Type>) -> Value {
    // an answer given with its fields only (the owner's, in the app) has an empty text: its fields are its text
    // (paper-research N-101 p1008, 1)
    if let Some(t) = v.get(TEXT).filter(|t| !t.as_str().is_some_and(|s| s.trim().is_empty())) {
        return t.clone();
    }
    let fields = match (ty, v.get("head").and_then(Value::as_str)) {
        (Some(Type::Variant(_)), Some(h)) => ty.and_then(|t| t.constructor(h)).cloned(),
        (Some(Type::Record(fs)), _) => Some(fs.clone()),
        _ => None,
    };
    let Some(fields) = fields else { return Value::Null };
    let mut out = Pieces::default();
    for (f, t) in &fields {
        out.say(&heading(f));
        out.say("\n");
        words(v.get(f).unwrap_or(&Value::Null), t, &mut out);
        out.say("\n\n");
    }
    out.value()
}

/// A text being made: new words, merged as they come, and texts held where they are.
#[derive(Default)]
struct Pieces(Vec<Text>);

impl Pieces {
    fn say(&mut self, w: &str) {
        match self.0.last_mut() {
            Some(Text::New(s)) => s.push_str(w),
            _ => self.0.push(Text::New(w.into())),
        }
    }
    fn hold(&mut self, t: Text) {
        self.0.push(t);
    }
    /// One string when every piece is new words (as `text of` gave before), else the joined text; the end trimmed.
    fn value(mut self) -> Value {
        while let Some(Text::New(s)) = self.0.last_mut() {
            let n = s.trim_end().len();
            s.truncate(n);
            if !s.is_empty() {
                break;
            }
            self.0.pop();
        }
        match self.0.as_slice() {
            [] => json!(""),
            [Text::New(s)] => json!(s),
            _ => Text::Joined { joined: self.0 }.value(),
        }
    }
}

fn heading(f: &str) -> String {
    let s = f.replace('_', " ");
    let mut c = s.chars();
    c.next().map(|x| x.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// V, of type T, as words into OUT: a text held by reference kept as it is held.
fn words(v: &Value, t: &Type, out: &mut Pieces) {
    match (v, t) {
        (Value::Null, _) => out.say("none"),
        (Value::String(s), _) => out.say(s),
        (Value::Array(xs), Type::List(e)) => match &**e {
            Type::Record(fs) => {
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        out.say("\n\n");
                    }
                    for (k, (f, ft)) in fs.iter().enumerate() {
                        if k > 0 {
                            out.say(&format!("\n{}\n", heading(f)));
                        }
                        words(x.get(f).unwrap_or(&Value::Null), ft, out);
                    }
                }
            }
            e => {
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        out.say(", ");
                    }
                    words(x, e, out);
                }
            }
        },
        (v, _) => match Text::of(v).filter(|h| v.is_object() && !matches!(h, Text::New(_))) {
            Some(held) => out.hold(held),
            None => out.say(&crate::print::value(v)),
        },
    }
}

/// The `text of` and `by of` in E (inside other expressions too), each with the expression it reads.
pub fn reads<'a>(e: &'a Expr, out: &mut Vec<&'a Expr>) {
    match e {
        Expr::TextOf { e: x, .. } | Expr::ByOf(x) => {
            reads(x, out);
            out.push(e);
        }
        Expr::Field(x, _) | Expr::ResultOf { e: x, .. } | Expr::Find { e: x, .. } | Expr::Quant { e: x, .. } | Expr::Not(x) => reads(x, out),
        Expr::Con { args: xs, .. } | Expr::List(xs) | Expr::Tuple(xs) | Expr::Fact { args: xs, .. } => xs.iter().for_each(|x| reads(x, out)),
        Expr::Record(fs) => fs.iter().for_each(|(_, x)| reads(x, out)),
        Expr::Cmp { l, r, .. } | Expr::Rel { l, r, .. } | Expr::And(l, r) | Expr::Or(l, r) | Expr::Append(l, r) => {
            reads(l, out);
            reads(r, out);
        }
        _ => {}
    }
}

/// Why `text of`/`by of` E (one of `reads`) has nothing to give, its value V: NoResult's.
pub fn missing(e: &Expr, v: &Value) -> Value {
    let what = if matches!(e, Expr::ByOf(_)) { "by of" } else { "text of" };
    let why = if v.is_object() { "no step gave this value".to_string() } else { format!("{} is no answer", crate::print::value(v)) };
    json!({"head": "NoResult", "why": format!("`{what} {}`: {why}", crate::print::expr(e.inner()))})
}

impl Expr {
    /// What a `text of`/`by of` reads.
    pub fn inner(&self) -> &Expr {
        match self {
            Expr::TextOf { e, .. } | Expr::ByOf(e) => e,
            e => e,
        }
    }
}
