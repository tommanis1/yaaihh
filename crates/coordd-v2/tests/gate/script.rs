//! The gate's scripts read: a line's fields and values, the stand-in driver's table, and a run's results in a script's words.

use coordd_v2::link::{Ready, Waits};
use docket_v2::check::Loaded;
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::record::{Line, Refusal};
use docket_v2::run::value::{Instance, Value};
use std::cell::RefCell;
use std::io::Read;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::rc::Rc;

/// Splits `a; b(c; d); e` at each `at` outside brackets and quotes.
pub fn split(s: &str, at: char) -> Vec<String> {
    let (mut out, mut cur, mut depth, mut quoted) = (Vec::new(), String::new(), 0, false);
    for ch in s.chars() {
        match ch {
            '"' => quoted = !quoted,
            '(' | '{' | '[' if !quoted => depth += 1,
            ')' | '}' | ']' if !quoted => depth -= 1,
            _ if ch == at && depth == 0 && !quoted => {
                out.push(cur.trim().to_string());
                cur.clear();
                continue;
            }
            _ => {}
        }
        cur.push(ch);
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// Splits `a, b(c, d), e` at the commas outside brackets and quotes.
pub fn fields(s: &str) -> Vec<String> {
    split(s, ',')
}

/// `Name(a, b)`: the name and its fields as written.
pub fn call(s: &str) -> Result<(String, Vec<String>), String> {
    let s = s.trim();
    let Some((name, rest)) = s.split_once('(') else { return Ok((s.to_string(), vec![])) };
    let inner = rest.strip_suffix(')').ok_or(format!("{s}: no ) at the end"))?;
    Ok((name.trim().to_string(), fields(inner)))
}

/// A field as a value: `"…"` a text as it stands, `{ … }` a filling's text, `[a, b]` a list, `Name { f: a }` a
/// record, `true`/`false`, digits, `Name(…)` a constructor, else a text.
pub fn value(s: &str) -> Result<Value, String> {
    let s = s.trim();
    let all = |inner: &str| fields(inner).iter().map(|a| value(a)).collect::<Result<Vec<_>, _>>();
    Ok(match s {
        _ if s.len() > 1 && s.starts_with('"') && s.ends_with('"') => Value::text(&s[1..s.len() - 1]),
        _ if s.starts_with('{') => Value::text(s),
        _ if s.starts_with('[') => Value::List(all(s.strip_prefix('[').and_then(|r| r.strip_suffix(']')).ok_or(format!("{s}: no ] at the end"))?)?),
        _ if s.ends_with('}') && s.contains(" {") => {
            let (ty, rest) = s.split_once(" {").unwrap();
            let field = |f: &String| f.split_once(':').ok_or(format!("{f}: no `:`")).and_then(|(k, v)| Ok((k.trim().to_string(), value(v)?)));
            Value::Record { ty: ty.trim().to_string(), fields: fields(rest.strip_suffix('}').unwrap()).iter().map(field).collect::<Result<_, _>>()? }
        }
        "()" => Value::Unit,
        "true" | "false" => Value::Bool(s == "true"),
        _ if s.parse::<i64>().is_ok() => Value::Int(s.parse().unwrap()),
        _ if s.contains('(') => {
            let (name, args) = call(s)?;
            Value::ctor(&name, args.iter().map(|a| value(a)).collect::<Result<_, _>>()?)
        }
        _ => Value::text(s.trim_matches('"')),
    })
}

/// A value or a record line as a script writes it: as the engine writes it, without quotes.
pub fn written(v: &dyn std::fmt::Display) -> String {
    v.to_string().replace("\\\"", "").replace('"', "")
}

/// A field as the stand-in's table writes it: a text without quotes, a constructor by its own name, without its module.
pub fn shown(v: &Value) -> String {
    match v {
        Value::Text(t) => t.clone(),
        Value::Ctor { name, args } => {
            let own = name.rsplit('.').next().unwrap_or(name);
            if args.is_empty() { own.to_string() } else { format!("{own}({})", args.iter().map(shown).collect::<Vec<_>>().join(", ")) }
        }
        other => written(other),
    }
}

/// The one declared name that ends in `.NAME`.
pub fn qualify(l: &Loaded, name: &str) -> Result<String, String> {
    let end = format!(".{name}");
    let found: Vec<&String> = l.syms.keys().filter(|k| k.ends_with(&end)).collect();
    match found.as_slice() {
        [one] => Ok(one.to_string()),
        _ => Err(format!("{name} names {} declarations", found.len())),
    }
}

/// Value `v` with each constructor named as the loaded text declares it; one it does not declare stays as written.
pub fn declared(l: &Loaded, v: Value) -> Value {
    match v {
        Value::Ctor { name, args } => {
            let args = args.into_iter().map(|a| declared(l, a)).collect();
            Value::Ctor { name: qualify(l, &name).unwrap_or(name), args }
        }
        Value::Record { ty, fields } => Value::Record { ty: qualify(l, &ty).unwrap_or(ty), fields: fields.into_iter().map(|(k, v)| (k, declared(l, v))).collect() },
        Value::List(vs) => Value::List(vs.into_iter().map(|v| declared(l, v)).collect()),
        other => other,
    }
}

pub fn instance(l: &Loaded, s: &str) -> Result<Instance, String> {
    let (name, args) = call(s)?;
    Ok(Instance { ty: qualify(l, &name)?, args: args.iter().map(|a| value(a).map(|v| declared(l, v))).collect::<Result<_, _>>()? })
}

/// One answer of the stand-in's table: its line, the Effect's name, its fields as written (`_` for any), the value,
/// the costs, the Events performed with it.
pub struct Answer {
    pub text: String,
    pub op: String,
    pub args: Vec<String>,
    pub value: Value,
    pub costs: Vec<(String, i64)>,
    pub events: Vec<Instance>,
}

/// The stand-in driver, linked as any driver is: it answers each Effect at once with the first unused answer of
/// the table that fits it, notes each Effect it has none for, and performs the Events queued for it when a byte
/// reaches `wake`.
pub struct StandIn {
    pub name: Vec<Value>,
    pub table: Rc<RefCell<Vec<Answer>>>,
    pub effects: Vec<(String, Vec<String>)>,
    pub events: Vec<String>,
    pub missed: Rc<RefCell<Vec<String>>>,
    pub queued: Rc<RefCell<Vec<Instance>>>,
    pub wake: UnixStream,
}

impl Driver for StandIn {
    fn name(&self) -> Vec<Value> {
        self.name.clone()
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        self.effects.clone()
    }
    fn events(&self) -> Vec<String> {
        self.events.clone()
    }
    fn carry(&mut self, op: &str, args: &[Value], _: u64) -> Reply {
        let mut table = self.table.borrow_mut();
        let fits = |a: &Answer| (op == a.op || op.ends_with(&format!(".{}", a.op))) && a.args.len() == args.len() && a.args.iter().zip(args).all(|(w, v)| w == "_" || *w == shown(v));
        match table.iter().position(fits) {
            Some(k) => {
                let a = table.remove(k);
                Reply::Answer { value: a.value, costs: a.costs, events: a.events }
            }
            None => {
                let what = format!("{op}({})", args.iter().map(shown).collect::<Vec<_>>().join(", "));
                self.missed.borrow_mut().push(what.clone());
                Reply::Failed(format!("the stand-in has no answer for {what}"))
            }
        }
    }
    fn dropped(&mut self, _: u64) {}
}

impl Waits for StandIn {
    fn waits(&self) -> Vec<RawFd> {
        vec![self.wake.as_raw_fd()]
    }
    fn ready(&mut self) -> Vec<Ready> {
        while (&self.wake).read(&mut [0u8; 16]).is_ok_and(|n| n > 0) {}
        self.queued.borrow_mut().drain(..).map(Ready::Event).collect()
    }
}

/// `Carry(_, c1, unit, Go) -> true; tokens 40; Passed(c1, unit)`, read against the loaded names.
pub fn answer(names: &Loaded, s: &str) -> Result<Answer, String> {
    let (effect, rest) = s.split_once(" -> ").ok_or(format!("{s}: no answer"))?;
    let (op, args) = call(effect)?;
    let parts = split(rest, ';');
    let value = declared(names, value(parts.first().ok_or(format!("{s}: no answer"))?)?);
    let (mut costs, mut events) = (Vec::new(), Vec::new());
    for p in &parts[1..] {
        match p.split_once(' ') {
            Some((dim, n)) if n.trim().parse::<i64>().is_ok() => costs.push((dim.to_string(), n.trim().parse().unwrap())),
            _ => events.push(instance(names, p)?),
        }
    }
    Ok(Answer { text: s.to_string(), op, args, value, costs, events })
}

/// How run `run` went, in a script's words: its act performed or refused, then how the run ended.
pub fn outcomes(lines: &[Line], run: &str, act: &Instance) -> Vec<String> {
    let mut out = Vec::new();
    for l in lines {
        match l {
            Line::Performed { act: a, run: Some(r), .. } if r == run && a == act => out.push("performed".into()),
            Line::Refused { act: a, run: Some(r), why, .. } if r == run && a == act => out.push(match why {
                Refusal::Source => "refused source".into(),
                Refusal::Policy(_) => "refused policy".into(),
                Refusal::Closes(_) => "refused closes".into(),
            }),
            Line::Ended { run: r, value } if r == run => out.push(format!("ended {}", written(value))),
            Line::Raised { run: r, effect, .. } if r == run => out.push(format!("raised {}", effect.rsplit('.').next().unwrap_or(effect))),
            _ => {}
        }
    }
    out
}

/// The `Fault`s raised in LINES, by the step's own run or any run it set off, unless WANT, its stated results, has one.
pub fn faults(lines: &[Line], want: &[String]) -> Vec<String> {
    if want.iter().any(|w| w == "raised Fault") {
        return vec![];
    }
    lines.iter().filter(|l| matches!(l, Line::Raised { effect, .. } if effect.rsplit('.').next() == Some("Fault"))).map(|l| written(l)).collect()
}

/// An error naming FAULTS unless LINE, the script's next line, is a `record has` naming a `Fault`.
pub fn stated(faults: &[String], line: &str) -> Result<(), String> {
    if faults.is_empty() || line.strip_prefix("record has ").is_some_and(|t| t.contains("Fault")) {
        return Ok(());
    }
    Err(format!("the step before raised {}, and no `record has` line after it states it", faults.join("; ")))
}

#[test]
fn a_fault_a_step_set_off_fails_it_unless_its_results_or_the_next_line_state_it() {
    let fault = Line::Raised { run: "r2".into(), effect: "Fault".into(), args: vec![Value::text("m.rel: no field rel is known here")] };
    let ended = Line::Ended { run: "r1".into(), value: Value::Unit };
    let lines = [ended.clone(), fault];
    let owed = faults(&lines, &["performed".into(), "ended ()".into()]);
    assert_eq!(owed.len(), 1);
    assert!(stated(&owed, "holds Seen(w1)").unwrap_err().contains("no field rel"));
    assert!(stated(&owed, "").is_err(), "a script that ends after the step");
    assert!(stated(&owed, "record lacks , Fault(").is_err());
    assert_eq!(stated(&owed, "record has Fault(\"m.rel"), Ok(()));
    assert!(faults(&lines, &["raised Fault".into()]).is_empty(), "the step's own results state it");
    assert!(faults(&[ended], &[]).is_empty());
}

#[test]
fn a_script_writes_lists_records_truths_and_texts_with_any_sign() {
    let t = Value::text;
    assert_eq!(value("[m1, 2]"), Ok(Value::List(vec![t("m1"), Value::Int(2)])));
    assert_eq!(value("Use { tool: Bash, args: [ls, \"-l; (x)\"] }"), Ok(Value::Record { ty: "Use".into(), fields: vec![("tool".into(), t("Bash")), ("args".into(), Value::List(vec![t("ls"), t("-l; (x)")]))] }));
    assert_eq!(value("false"), Ok(Value::Bool(false)));
    assert_eq!(value("\"a, b; {c}\""), Ok(t("a, b; {c}")));
    assert_eq!(split("x -> \"a; b\"; tokens 3; E(c; d)", ';'), ["x -> \"a; b\"", "tokens 3", "E(c; d)"]);
}
