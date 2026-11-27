//! The machine's values and lines as JSON, the wire's and the file's form, and back for what a request may carry.

use docket_v2::run::record::{Line, Refusal};
use docket_v2::run::value::{Instance, Value};
use serde_json::{json, Map, Value as Json};

/// `v` as JSON: a unit is null, a list an array, a text a string; every other kind an object named by its kind.
pub fn of_value(v: &Value) -> Json {
    let all = |vs: &[Value]| Json::Array(vs.iter().map(of_value).collect());
    match v {
        Value::Unit => Json::Null,
        Value::Bool(b) => json!(b),
        Value::Int(n) => json!(n),
        Value::Text(s) => json!(s),
        Value::Inst(i) => json!({"inst": i.ty, "args": all(&i.args)}),
        Value::Ctor { name, args } => json!({"ctor": name, "args": all(args)}),
        Value::Tuple(vs) => json!({"tuple": all(vs)}),
        Value::List(vs) => all(vs),
        Value::Record { ty, fields } => json!({"record": ty, "fields": fields.iter().map(|(k, v)| json!([k, of_value(v)])).collect::<Vec<_>>()}),
        Value::Prompt { name, args } => json!({"prompt": name, "args": all(args)}),
        Value::Program(_) => json!({"program": v.to_string()}),
        Value::Resumption(n) => json!({"resumption": n}),
    }
}

/// The value JSON `j` stands for. A program and a resumption are written out for reading but never read back:
/// neither may come in from outside (spec 7, 8.5).
pub fn value(j: &Json) -> Result<Value, String> {
    let all = |j: &Json| j.as_array().ok_or_else(|| format!("not a list: {j}"))?.iter().map(value).collect::<Result<Vec<_>, _>>();
    let at = |o: &Map<String, Json>, k: &str| o.get(k).cloned().unwrap_or(Json::Null);
    let name = |o: &Map<String, Json>, k: &str| at(o, k).as_str().map(str::to_string).ok_or_else(|| format!("{k} is no name: {}", at(o, k)));
    Ok(match j {
        Json::Null => Value::Unit,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => Value::Int(n.as_i64().ok_or_else(|| format!("not a whole number: {n}"))?),
        Json::String(s) => Value::Text(s.clone()),
        Json::Array(_) => Value::List(all(j)?),
        Json::Object(o) if o.contains_key("inst") => Value::Inst(Instance { ty: name(o, "inst")?, args: all(&at(o, "args"))? }),
        Json::Object(o) if o.contains_key("ctor") => Value::Ctor { name: name(o, "ctor")?, args: all(&at(o, "args"))? },
        Json::Object(o) if o.contains_key("prompt") => Value::Prompt { name: name(o, "prompt")?, args: all(&at(o, "args"))? },
        Json::Object(o) if o.contains_key("tuple") => Value::Tuple(all(&at(o, "tuple"))?),
        Json::Object(o) if o.contains_key("record") => {
            let field = |f: &Json| match f.as_array().map(Vec::as_slice) {
                Some([Json::String(k), v]) => Ok((k.clone(), value(v)?)),
                _ => Err(format!("not a field: {f}")),
            };
            let fields = at(o, "fields").as_array().ok_or("a record's fields are a list")?.iter().map(field).collect::<Result<_, _>>()?;
            Value::Record { ty: name(o, "record")?, fields }
        }
        _ => return Err(format!("no value may come in as {j}")),
    })
}

/// A hash as 64 hex digits, as the record writes it.
pub fn hex(h: &[u8; 32]) -> String {
    h.iter().map(|b| format!("{b:02x}")).collect()
}

/// Instance `i` as a line of the record writes it.
pub fn inst(i: &Instance) -> Json {
    of_value(&Value::Inst(i.clone()))
}

/// Line `l` as JSON: an object with one key, its kind (spec 8.1).
pub fn line(l: &Line) -> Json {
    let all = |vs: &[Value]| Json::Array(vs.iter().map(of_value).collect());
    let party = |p: &Option<Value>| p.as_ref().map_or(Json::Null, of_value);
    match l {
        Line::Performed { act, party: p, run } => json!({"performed": {"act": inst(act), "party": party(p), "run": run}}),
        Line::Refused { act, party: p, run, why } => {
            let why = match why {
                Refusal::Source => json!("source"),
                Refusal::Policy(w) => json!({"policy": w}),
                Refusal::Closes(w) => json!({"closes": w}),
            };
            json!({"refused": {"act": inst(act), "party": party(p), "run": run, "why": why}})
        }
        Line::Effect { op, args, run, head, key } => json!({"effect": {"op": op, "args": all(args), "run": run, "head": head, "key": hex(key)}}),
        Line::Answered { line, value } => json!({"answered": {"line": line, "value": of_value(value)}}),
        Line::Charged { run, dim, n } => json!({"charged": {"run": run, "dim": dim, "n": n}}),
        Line::Began { run, by } => json!({"began": {"run": run, "by": of_value(by)}}),
        Line::Ended { run, value } => json!({"ended": {"run": run, "value": of_value(value)}}),
        Line::Raised { run, effect, args } => json!({"raised": {"run": run, "effect": effect, "args": all(args)}}),
        Line::Cancelled { run, by } => json!({"cancelled": {"run": run, "by": by}}),
        Line::Prefix { run, old, upto } => json!({"prefix": {"run": run, "old": old, "upto": upto}}),
        Line::Loaded { hash } => json!({"loaded": hex(hash)}),
    }
}

/// The line JSON `j` stands for, as [`line`] writes it; anything else, a field missing or of another kind among them,
/// is an error naming it.
pub fn read_line(j: &Json) -> Result<Line, String> {
    let o = j.as_object().filter(|o| o.len() == 1).ok_or_else(|| format!("not a line: {j}"))?;
    let (kind, b) = o.iter().next().unwrap();
    let field = |k: &str| b.get(k).ok_or_else(|| format!("{kind} has no {k}: {j}"));
    let text = |k: &str| field(k)?.as_str().map(str::to_string).ok_or_else(|| format!("{k} is no text: {j}"));
    let maybe = |k: &str| match field(k)? {
        Json::Null => Ok(None),
        Json::String(s) => Ok(Some(s.clone())),
        _ => Err(format!("{k} is no text: {j}")),
    };
    let int = |k: &str| field(k)?.as_i64().ok_or_else(|| format!("{k} is no whole number: {j}"));
    let act = || match value(field("act")?)? {
        Value::Inst(i) => Ok(i),
        v => Err(format!("{v} is no act: {j}")),
    };
    let party = || match field("party")? {
        Json::Null => Ok(None),
        p => value(p).map(Some),
    };
    let all = |k: &str| match value(field(k)?)? {
        Value::List(vs) => Ok(vs),
        v => Err(format!("{v} is no list: {j}")),
    };
    let hash = |h: &Json| -> Result<[u8; 32], String> {
        let h = h.as_str().filter(|h| h.len() == 64 && h.is_ascii()).ok_or_else(|| format!("no hash: {j}"))?;
        let mut out = [0u8; 32];
        for (k, b) in out.iter_mut().enumerate() {
            *b = u8::from_str_radix(&h[2 * k..2 * k + 2], 16).map_err(|_| format!("no hash: {j}"))?;
        }
        Ok(out)
    };
    Ok(match kind.as_str() {
        "performed" => Line::Performed { act: act()?, party: party()?, run: maybe("run")? },
        "refused" => {
            let why = match field("why")? {
                Json::String(s) if s == "source" => Refusal::Source,
                Json::Object(w) => match w.iter().collect::<Vec<_>>()[..] {
                    [(k, Json::String(t))] if k == "policy" => Refusal::Policy(t.clone()),
                    [(k, Json::String(t))] if k == "closes" => Refusal::Closes(t.clone()),
                    _ => return Err(format!("no refusal: {j}")),
                },
                _ => return Err(format!("no refusal: {j}")),
            };
            Line::Refused { act: act()?, party: party()?, run: maybe("run")?, why }
        }
        "effect" => Line::Effect { op: text("op")?, args: all("args")?, run: text("run")?, head: text("head")?, key: hash(field("key")?)? },
        "answered" => Line::Answered { line: usize::try_from(int("line")?).map_err(|_| format!("line is no place: {j}"))?, value: value(field("value")?)? },
        "charged" => Line::Charged { run: text("run")?, dim: text("dim")?, n: int("n")? },
        "began" => Line::Began { run: text("run")?, by: value(field("by")?)? },
        "ended" => Line::Ended { run: text("run")?, value: value(field("value")?)? },
        "raised" => Line::Raised { run: text("run")?, effect: text("effect")?, args: all("args")? },
        "cancelled" => Line::Cancelled { run: text("run")?, by: text("by")? },
        "prefix" => Line::Prefix { run: text("run")?, old: text("old")?, upto: usize::try_from(int("upto")?).map_err(|_| format!("upto is no place: {j}"))? },
        "loaded" => Line::Loaded { hash: hash(b)? },
        _ => return Err(format!("no line is of kind {kind}: {j}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_from_outside_comes_back_as_it_went() {
        let vs = [
            Value::Unit,
            Value::Bool(true),
            Value::Int(-3),
            Value::text("w1"),
            Value::Inst(Instance { ty: "a.Change".into(), args: vec![Value::text("c1")] }),
            Value::ctor("Some", vec![Value::Int(2)]),
            Value::Tuple(vec![Value::Int(1), Value::text("x")]),
            Value::List(vec![Value::Bool(false)]),
            Value::Record { ty: "a.Use".into(), fields: vec![("tool".into(), Value::text("Bash")), ("n".into(), Value::Int(1))] },
            Value::Prompt { name: "a.Ask".into(), args: vec![] },
        ];
        for v in vs {
            assert_eq!(value(&of_value(&v)).as_ref(), Ok(&v), "{}", of_value(&v));
        }
        assert!(value(&json!({"resumption": 1})).is_err(), "a resumption never comes in");
        assert!(value(&json!(1.5)).is_err());
    }

    #[test]
    fn each_line_is_one_object_named_by_its_kind() {
        let i = Instance { ty: "a.Submit".into(), args: vec![Value::text("w1")] };
        let l = Line::Refused { act: i, party: Some(Value::text("w1")), run: Some("r1".into()), why: Refusal::Policy("not enabled".into()) };
        assert_eq!(line(&l), json!({"refused": {"act": {"inst": "a.Submit", "args": ["w1"]}, "party": "w1", "run": "r1", "why": {"policy": "not enabled"}}}));
        assert_eq!(line(&Line::Loaded { hash: [0xab; 32] })["loaded"].as_str().map(str::len), Some(64));
        assert_eq!(line(&Line::Charged { run: "r1".into(), dim: "tokens".into(), n: 9 }), json!({"charged": {"run": "r1", "dim": "tokens", "n": 9}}));
    }

    #[test]
    fn each_line_reads_back_as_it_was_written_and_nothing_else_reads() {
        let i = Instance { ty: "a.Submit".into(), args: vec![Value::text("w1")] };
        let (w1, r1) = (Some(Value::text("w1")), Some("r1".to_string()));
        let lines = [
            Line::Performed { act: i.clone(), party: None, run: None },
            Line::Refused { act: i.clone(), party: w1.clone(), run: r1.clone(), why: Refusal::Source },
            Line::Refused { act: i.clone(), party: w1.clone(), run: r1.clone(), why: Refusal::Policy("p".into()) },
            Line::Refused { act: i, party: w1, run: r1, why: Refusal::Closes("c".into()) },
            Line::Effect { op: "a.Run".into(), args: vec![Value::Int(1)], run: "r1".into(), head: "abc".into(), key: [7; 32] },
            Line::Answered { line: 4, value: Value::Bool(true) },
            Line::Charged { run: "r1".into(), dim: "tokens".into(), n: -2 },
            Line::Began { run: "r1".into(), by: Value::text("w1") },
            Line::Ended { run: "r1".into(), value: Value::Unit },
            Line::Raised { run: "r1".into(), effect: "std.Fault".into(), args: vec![Value::text("x")] },
            Line::Cancelled { run: "r1".into(), by: "w2".into() },
            Line::Prefix { run: "r3".into(), old: "r1".into(), upto: 7 },
            Line::Loaded { hash: [0xab; 32] },
        ];
        for l in lines {
            assert_eq!(read_line(&line(&l)).as_ref(), Ok(&l), "{}", line(&l));
        }
        let wrong = [
            json!({"charged": {"run": "r1", "dim": "tokens"}}),
            json!({"refused": {"act": {"inst": "a.S", "args": []}, "party": null, "run": null, "why": "policy"}}),
            json!({"spoken": {}}),
            json!({"loaded": "ab"}),
            json!({"ended": {"run": "r1", "value": null}, "began": {"run": "r1", "by": "w1"}}),
        ];
        for j in wrong {
            assert!(read_line(&j).is_err(), "{j}");
        }
    }
}
