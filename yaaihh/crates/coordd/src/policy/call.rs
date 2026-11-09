//! LLM calls as typed programs (draft §7, after Leash's routes): the program is the user's, the route the policy's.
//! A `.call` file declares record types and calls:
//!
//! ```text
//! type AgentMessage {
//!   id: String
//!   text: String
//!   thread: String?
//! }
//! call recognise-question(message: AgentMessage) -> QuestionRef?
//!   task recognise-question
//!   template
//!     Message:
//!     {{ message.text }}
//!     {{ if message.thread }}It is part of thread {{ message.thread }}.{{ end }}
//! ```
//!
//! Types: `String`, `Int`, `Bool`, a record name, an enum name, `T?` (may be none) and `[T]`. An enum is one line,
//! `type Clarity = Clear | Unclear(why: String)`: its value is a JSON object whose `kind` names the variant, with the
//! variant's fields beside it (`{"kind": "Unclear", "why": "..."}`); the nets (`crate::net`) match on it. The template's holes are checked
//! when the file is loaded: `{{ path }}` names a String, Int or Bool reachable from the inputs (an optional one only
//! inside an `{{ if path }}` that tests it); `{{ if path }}` takes a Bool or an optional; `{{ for x in path }}` a
//! list; `{{ include "file" }}` another template file beside this one. The return type is the JSON schema sent with
//! the call (as `{"answer": T}`, an object at the root as the providers require), and the answer is checked against
//! the same type; a refusal, a cut-off answer or one that does not check is a failure of that model, never an
//! answer (§5.4).

use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Type {
    Str,
    Int,
    Bool,
    Opt(Box<Type>),
    List(Box<Type>),
    Named(String),
}

/// A record type's fields, in order.
pub type Record = Vec<(String, Type)>;

/// A template, parsed.
#[derive(Clone, Debug, PartialEq)]
enum Node {
    Text(String),
    Hole(Vec<String>),
    If(Vec<String>, Vec<Node>),
    For(String, Vec<String>, Vec<Node>),
}

/// A call program.
#[derive(Clone, Debug)]
pub struct Call {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub returns: Type,
    pub task: String,
    template: Vec<Node>,
    pub at: String,
}

/// The calls and types of a folder of `.call` files.
#[derive(Clone, Debug, Default)]
pub struct Calls {
    pub types: BTreeMap<String, Record>,
    /// Enum types: their variants in order, each with its fields.
    pub enums: BTreeMap<String, Vec<(String, Record)>>,
    pub calls: BTreeMap<String, Call>,
}

pub fn ty(s: &str) -> Result<Type, String> {
    let s = s.trim();
    if let Some(t) = s.strip_suffix('?') {
        return Ok(Type::Opt(Box::new(ty(t)?)));
    }
    if let Some(t) = s.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
        return Ok(Type::List(Box::new(ty(t)?)));
    }
    Ok(match s {
        "String" => Type::Str,
        "Int" => Type::Int,
        "Bool" => Type::Bool,
        n if n.chars().next().is_some_and(|c| c.is_ascii_uppercase()) && n.chars().all(|c| c.is_ascii_alphanumeric()) => Type::Named(n.into()),
        _ => return Err(format!("`{s}` is not a type")),
    })
}

/// The types the nets' built-ins give and take (`questions(agent)`, the wakes; `crate::net`), loaded before any `.call`
/// file; a file may not declare a type of the same name.
pub const SYSTEM_TYPES: &str = "type Question {\n  id: String\n  from: String\n  text: String\n  thread: String\n}\ntype Said {\n  id: String\n  from: String\n  to: [String]\n  text: String\n  thread: String?\n  question: Bool\n}\ntype Quoted {\n  source: String\n  text: String\n}\ntype Wake = Nothing | Answer(q: String) | Check(check: String) | Mail(from: String)\n";

/// An enum's variants: `A | B(x: T, y: U)`.
fn enum_variants(s: &str) -> Result<Vec<(String, Record)>, String> {
    let mut out = Vec::new();
    for v in s.split('|') {
        let v = v.trim();
        let (name, fields) = match v.split_once('(') {
            Some((n, f)) => (n.trim(), f.trim().strip_suffix(')').ok_or(format!("`{v}`: unclosed ("))?),
            None => (v, ""),
        };
        if !name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) || !name.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(format!("`{name}` is not a variant name (Capitalised)"));
        }
        let fields = fields
            .split(',')
            .filter(|f| !f.trim().is_empty())
            .map(|f| {
                let (n, t) = f.split_once(':').ok_or(format!("`{v}`: a field is `name: Type`"))?;
                Ok((n.trim().to_string(), ty(t)?))
            })
            .collect::<Result<Record, String>>()?;
        out.push((name.to_string(), fields));
    }
    if out.is_empty() {
        return Err("an enum needs a variant".into());
    }
    Ok(out)
}

impl Calls {
    /// The `.call` files of the folders DIRS (a later folder's call or type of the same name replaces an earlier one's:
    /// the project's over the system's).
    pub fn load(dirs: &[String], overlay: &super::Overlay) -> Result<Calls, String> {
        let mut out = Calls::default();
        let mut sources = Vec::new();
        for dir in dirs {
            for (f, text) in super::files_in(dir, "call", overlay)? {
                sources.push((dir.clone(), f, text));
            }
        }
        // types first, so a call may use a type of any file; the system's before all, and not redefined
        out.parse_types("system", SYSTEM_TYPES)?;
        let system: Vec<String> = out.types.keys().chain(out.enums.keys()).cloned().collect();
        for (_, file, text) in &sources {
            let mut one = Calls::default();
            one.parse_types(file, text)?;
            if let Some(t) = one.types.keys().chain(one.enums.keys()).find(|t| system.contains(t)) {
                return Err(format!("{file}: type {t} is the system's (the nets' built-ins use it); name yours otherwise"));
            }
            out.types.extend(one.types);
            out.enums.extend(one.enums);
        }
        for (dir, file, text) in &sources {
            out.parse_calls(dir, file, text)?;
        }
        Ok(out)
    }

    pub fn parse_types(&mut self, file: &str, text: &str) -> Result<(), String> {
        let mut lines = text.lines().enumerate();
        while let Some((i, l)) = lines.next() {
            let Some(rest) = l.strip_prefix("type ") else { continue };
            if let Some((name, variants)) = rest.split_once('=') {
                let at = format!("{file}:{}", i + 1);
                let variants = variants.split("//").next().unwrap_or("");
                self.enums.insert(name.trim().to_string(), enum_variants(variants).map_err(|e| format!("{at}: {e}"))?);
                continue;
            }
            let name = rest.trim().strip_suffix('{').ok_or(format!("{file}:{}: `type NAME {{`", i + 1))?.trim().to_string();
            let mut fields = Vec::new();
            for (j, f) in lines.by_ref() {
                let f = f.split("//").next().unwrap_or("").trim();
                if f == "}" {
                    break;
                }
                if f.is_empty() {
                    continue;
                }
                let (n, t) = f.split_once(':').ok_or(format!("{file}:{}: a field is `name: Type`", j + 1))?;
                fields.push((n.trim().to_string(), ty(t).map_err(|e| format!("{file}:{}: {e}", j + 1))?));
            }
            self.types.insert(name, fields);
        }
        Ok(())
    }

    fn parse_calls(&mut self, dir: &str, file: &str, text: &str) -> Result<(), String> {
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let Some(head) = lines[i].strip_prefix("call ") else {
                i += 1;
                continue;
            };
            let at = format!("{file}:{}", i + 1);
            let (sig, ret) = head.split_once("->").ok_or(format!("{at}: `call NAME(PARAM: Type, ...) -> Type`"))?;
            let (name, params) = sig.trim().split_once('(').ok_or(format!("{at}: `call NAME(...)`"))?;
            let params = params.trim().strip_suffix(')').ok_or(format!("{at}: unclosed ("))?;
            let params: Vec<(String, Type)> = params
                .split(',')
                .filter(|p| !p.trim().is_empty())
                .map(|p| {
                    let (n, t) = p.split_once(':').ok_or(format!("{at}: a parameter is `name: Type`"))?;
                    Ok((n.trim().to_string(), ty(t)?))
                })
                .collect::<Result<_, String>>()?;
            let returns = ty(ret).map_err(|e| format!("{at}: {e}"))?;
            let (mut task, mut template) = (None, None);
            i += 1;
            while i < lines.len() && (lines[i].starts_with(' ') || lines[i].trim().is_empty()) {
                let l = lines[i].trim();
                if let Some(t) = l.strip_prefix("task ") {
                    task = Some(t.trim().to_string());
                } else if l == "template" {
                    // the template: the lines indented deeper than `template`, dedented
                    let indent = lines[i].len() - lines[i].trim_start().len();
                    let mut body = Vec::new();
                    i += 1;
                    while i < lines.len() && (lines[i].trim().is_empty() || lines[i].len() - lines[i].trim_start().len() > indent) {
                        body.push(lines[i]);
                        i += 1;
                    }
                    while body.last().is_some_and(|l| l.trim().is_empty()) {
                        body.pop();
                    }
                    let cut = body.iter().filter(|l| !l.trim().is_empty()).map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
                    let text = body.iter().map(|l| l.get(cut..).unwrap_or("")).collect::<Vec<_>>().join("\n");
                    template = Some(text);
                    continue;
                } else if !l.is_empty() && !l.starts_with("//") && !l.starts_with("needs ") {
                    return Err(format!("{file}:{}: a call takes `task NAME`, `needs ...` and `template`", i + 1));
                }
                i += 1;
            }
            let template = template.ok_or(format!("{at}: the call has no template"))?;
            let nodes = self.template(dir, &template, 0)?;
            let call = Call { name: name.trim().into(), params, returns, task: task.ok_or(format!("{at}: the call has no `task`"))?, template: nodes, at: at.clone() };
            self.check_type(&call.returns).map_err(|e| format!("{at}: {e}"))?;
            let scope: BTreeMap<String, Type> = call.params.iter().cloned().collect();
            self.check(&call.template, &scope).map_err(|e| format!("{at}: the template: {e}"))?;
            self.calls.insert(call.name.clone(), call);
        }
        Ok(())
    }

    fn check_type(&self, t: &Type) -> Result<(), String> {
        match t {
            Type::Opt(t) | Type::List(t) => self.check_type(t),
            Type::Named(n) if self.enums.contains_key(n) => {
                self.enums[n].iter().flat_map(|(_, fs)| fs).try_for_each(|(_, t)| self.check_type(t))
            }
            Type::Named(n) => self.types.get(n).map(|_| ()).ok_or(format!("no type `{n}`")),
            _ => Ok(()),
        }
    }

    /// Parse a template (DEPTH counts includes, against cycles).
    fn template(&self, dir: &str, text: &str, depth: usize) -> Result<Vec<Node>, String> {
        if depth > 8 {
            return Err("includes nest deeper than 8".into());
        }
        // tokens: text and tags
        let mut toks = Vec::new();
        let mut rest = text;
        while let Some(i) = rest.find("{{") {
            toks.push((false, rest[..i].to_string()));
            let j = rest[i..].find("}}").ok_or("an unclosed {{")? + i;
            toks.push((true, rest[i + 2..j].trim().to_string()));
            rest = &rest[j + 2..];
        }
        toks.push((false, rest.to_string()));
        let mut stack: Vec<(Option<(bool, String, Vec<String>)>, Vec<Node>)> = vec![(None, Vec::new())];
        let path = |s: &str| s.split('.').map(|p| p.trim().to_string()).collect::<Vec<_>>();
        for (tag, t) in toks {
            if !tag {
                if !t.is_empty() {
                    stack.last_mut().unwrap().1.push(Node::Text(t));
                }
                continue;
            }
            if let Some(p) = t.strip_prefix("if ") {
                stack.push((Some((false, String::new(), path(p))), Vec::new()));
            } else if let Some(r) = t.strip_prefix("for ") {
                let (x, p) = r.split_once(" in ").ok_or("`{{ for x in path }}`")?;
                stack.push((Some((true, x.trim().into(), path(p))), Vec::new()));
            } else if t == "end" {
                let (open, body) = stack.pop().unwrap();
                let (is_for, x, p) = open.ok_or("an {{ end }} with no if or for")?;
                stack.last_mut().ok_or("an {{ end }} with no if or for")?.1.push(if is_for { Node::For(x, p, body) } else { Node::If(p, body) });
            } else if let Some(f) = t.strip_prefix("include ") {
                let f = f.trim().trim_matches('"');
                let text = std::fs::read_to_string(format!("{dir}/{f}")).map_err(|e| format!("include {f}: {e}"))?;
                let nodes = self.template(dir, text.trim_end_matches('\n'), depth + 1)?;
                stack.last_mut().unwrap().1.extend(nodes);
            } else {
                stack.last_mut().unwrap().1.push(Node::Hole(path(&t)));
            }
        }
        if stack.len() != 1 {
            return Err("an {{ if }} or {{ for }} without its {{ end }}".into());
        }
        Ok(stack.pop().unwrap().1)
    }

    /// The type of PATH in SCOPE.
    fn path_type(&self, scope: &BTreeMap<String, Type>, path: &[String]) -> Result<Type, String> {
        let mut t = scope.get(&path[0]).cloned().ok_or(format!("`{}` is not an input", path[0]))?;
        for f in &path[1..] {
            let Type::Named(n) = &t else { return Err(format!("`{}`: `{f}` of a {t:?}, which has no fields (an optional value is opened by `if`)", path.join("."))) };
            if self.enums.contains_key(n) {
                // a template reads an enum's variant name only; its fields are the nets' to match
                if f == "kind" {
                    t = Type::Str;
                    continue;
                }
                return Err(format!("`{}`: {n} is an enum: a template reads its `kind` only", path.join(".")));
            }
            t = self.types[n].iter().find(|(x, _)| x == f).map(|(_, t)| t.clone()).ok_or(format!("`{}`: {n} has no field `{f}`", path.join(".")))?;
        }
        Ok(t)
    }

    fn check(&self, nodes: &[Node], scope: &BTreeMap<String, Type>) -> Result<(), String> {
        for n in nodes {
            match n {
                Node::Text(_) => {}
                Node::Hole(p) => match self.path_type(scope, p)? {
                    Type::Str | Type::Int | Type::Bool => {}
                    t => return Err(format!("{{{{ {} }}}} is a {t:?}: a hole takes a String, Int or Bool", p.join("."))),
                },
                Node::If(p, body) => {
                    let mut inner = scope.clone();
                    match self.path_type(scope, p)? {
                        Type::Bool => {}
                        Type::Opt(t) => {
                            // inside, the tested value is the value itself: narrowed by rebinding its path's head
                            if p.len() == 1 {
                                inner.insert(p[0].clone(), *t);
                            } else {
                                inner.insert(p.join("."), *t);
                            }
                        }
                        t => return Err(format!("{{{{ if {} }}}} tests a {t:?}: it takes a Bool or an optional", p.join("."))),
                    }
                    self.check_narrowed(body, &inner)?;
                }
                Node::For(x, p, body) => match self.path_type(scope, p)? {
                    Type::List(t) => {
                        let mut inner = scope.clone();
                        inner.insert(x.clone(), *t);
                        self.check(body, &inner)?;
                    }
                    t => return Err(format!("{{{{ for {x} in {} }}}} goes over a {t:?}: it takes a list", p.join("."))),
                },
            }
        }
        Ok(())
    }

    /// `check`, where a dotted path narrowed by an `if` is looked up whole first.
    fn check_narrowed(&self, nodes: &[Node], scope: &BTreeMap<String, Type>) -> Result<(), String> {
        let lifted: Vec<Node> = nodes.iter().map(|n| lift(n, scope)).collect();
        self.check(&lifted, scope)
    }

    /// The JSON schema of T (strict: every field required, none other allowed; an optional as anyOf with null).
    pub fn schema(&self, t: &Type) -> Value {
        match t {
            Type::Str => json!({"type": "string"}),
            Type::Int => json!({"type": "integer"}),
            Type::Bool => json!({"type": "boolean"}),
            Type::Opt(t) => json!({"anyOf": [self.schema(t), {"type": "null"}]}),
            Type::List(t) => json!({"type": "array", "items": self.schema(t)}),
            Type::Named(n) if self.enums.contains_key(n) => {
                let variants: Vec<Value> = self.enums[n]
                    .iter()
                    .map(|(v, fields)| {
                        let mut props = serde_json::Map::new();
                        props.insert("kind".into(), json!({"type": "string", "enum": [v]}));
                        props.extend(fields.iter().map(|(f, t)| (f.clone(), self.schema(t))));
                        let mut req = vec!["kind".to_string()];
                        req.extend(fields.iter().map(|(f, _)| f.clone()));
                        json!({"type": "object", "properties": props, "required": req, "additionalProperties": false})
                    })
                    .collect();
                json!({"anyOf": variants})
            }
            Type::Named(n) => {
                let fields = &self.types[n];
                let props: serde_json::Map<String, Value> = fields.iter().map(|(f, t)| (f.clone(), self.schema(t))).collect();
                json!({"type": "object", "properties": props, "required": fields.iter().map(|(f, _)| f.clone()).collect::<Vec<_>>(), "additionalProperties": false})
            }
        }
    }

    /// Whether V is a T; the first mismatch otherwise.
    pub fn conforms(&self, t: &Type, v: &Value) -> Result<(), String> {
        match (t, v) {
            (Type::Str, Value::String(_)) | (Type::Bool, Value::Bool(_)) => Ok(()),
            (Type::Int, Value::Number(n)) if n.is_i64() => Ok(()),
            (Type::Opt(_), Value::Null) => Ok(()),
            (Type::Opt(t), v) => self.conforms(t, v),
            (Type::List(t), Value::Array(xs)) => xs.iter().try_for_each(|x| self.conforms(t, x)),
            (Type::Named(n), Value::Object(o)) if self.enums.contains_key(n) => {
                let kind = o.get("kind").and_then(Value::as_str).ok_or(format!("{n}: no `kind`"))?;
                let (_, fields) = self.enums[n].iter().find(|(v, _)| v == kind).ok_or(format!("{n} has no variant `{kind}`"))?;
                for (f, ft) in fields {
                    self.conforms(ft, o.get(f).unwrap_or(&Value::Null)).map_err(|e| format!("{n}.{kind}.{f}: {e}"))?;
                }
                match o.keys().find(|k| *k != "kind" && !fields.iter().any(|(f, _)| f == *k)) {
                    Some(k) => Err(format!("{n}.{kind} has no field `{k}`")),
                    None => Ok(()),
                }
            }
            (Type::Named(n), Value::Object(o)) => {
                let fields = &self.types[n];
                for (f, ft) in fields {
                    self.conforms(ft, o.get(f).unwrap_or(&Value::Null)).map_err(|e| format!("{n}.{f}: {e}"))?;
                }
                match o.keys().find(|k| !fields.iter().any(|(f, _)| f == *k)) {
                    Some(k) => Err(format!("{n} has no field `{k}`")),
                    None => Ok(()),
                }
            }
            (t, v) => Err(format!("{v} is not a {t:?}")),
        }
    }

    /// CALL's prompt for INPUT (its parameters by name; INPUT has been checked to conform).
    pub fn render(&self, call: &Call, input: &Value) -> String {
        let mut out = String::new();
        render(&call.template, input, &mut out);
        out
    }
}

/// A node with each hole/test path whose dotted prefix is a narrowed name rewritten to that name.
fn lift(n: &Node, scope: &BTreeMap<String, Type>) -> Node {
    let fix = |p: &Vec<String>| -> Vec<String> {
        for k in (2..=p.len()).rev() {
            let head = p[..k].join(".");
            if scope.contains_key(&head) {
                let mut out = vec![head];
                out.extend(p[k..].iter().cloned());
                return out;
            }
        }
        p.clone()
    };
    match n {
        Node::Text(t) => Node::Text(t.clone()),
        Node::Hole(p) => Node::Hole(fix(p)),
        Node::If(p, b) => Node::If(fix(p), b.iter().map(|n| lift(n, scope)).collect()),
        Node::For(x, p, b) => Node::For(x.clone(), fix(p), b.iter().map(|n| lift(n, scope)).collect()),
    }
}

fn lookup<'a>(v: &'a Value, path: &[String]) -> &'a Value {
    path.iter().fold(v, |v, f| v.get(f).unwrap_or(&Value::Null))
}

fn render(nodes: &[Node], env: &Value, out: &mut String) {
    for n in nodes {
        match n {
            Node::Text(t) => out.push_str(t),
            Node::Hole(p) => match lookup(env, p) {
                Value::String(s) => out.push_str(s),
                Value::Null => {}
                v => out.push_str(&v.to_string()),
            },
            Node::If(p, body) => {
                let v = lookup(env, p);
                if !(v.is_null() || *v == Value::Bool(false)) {
                    render(body, env, out);
                }
            }
            Node::For(x, p, body) => {
                for item in lookup(env, p).as_array().cloned().unwrap_or_default() {
                    let mut inner = env.clone();
                    inner[x.as_str()] = item;
                    render(body, &inner, out);
                }
            }
        }
    }
}

/// Why a call's answer is not one (each is a failure of the model that gave it).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub enum Failure {
    /// The proxy or provider answered with an error.
    Error(String),
    /// The answer was cut off (`finish_reason: length`).
    Truncated,
    /// The model refused (a refusal, or `finish_reason: content_filter`).
    Refused(String),
    /// The answer is not JSON of the return type.
    Invalid(String),
}

/// Run CALL on INPUT with MODEL through the proxy (a fresh call: one chat completion, its answer held to the call's
/// return type).
pub fn run(calls: &Calls, call: &Call, input: &Value, model: &str, proxy: &super::facts::Proxy) -> Result<Value, Failure> {
    let prompt = calls.render(call, input);
    let schema = json!({"type": "object", "properties": {"answer": calls.schema(&call.returns)}, "required": ["answer"], "additionalProperties": false});
    let body = json!({
        "model": model,
        "max_tokens": 2000,
        "messages": [{"role": "user", "content": prompt}],
        "response_format": {"type": "json_schema", "json_schema": {"name": call.name.replace('-', "_"), "strict": true, "schema": schema}},
    });
    let resp = proxy
        .agent()
        .post(&format!("{}/v1/chat/completions", proxy.url))
        .header("Authorization", &format!("Bearer {}", proxy.key))
        .send_json(&body)
        .map_err(|e| Failure::Error(e.to_string()))?;
    let status = resp.status();
    let v: Value = resp.into_body().read_json().map_err(|e| Failure::Error(format!("{status}: {e}")))?;
    if !status.is_success() {
        return Err(Failure::Error(format!("{status}: {}", v.get("error").unwrap_or(&v))));
    }
    answer(calls, call, &v)
}

/// Run CALL as the next turn of a harness session (`super::harness`): HARNESS and MODEL, session RESUME (none: start
/// one). Its answer, checked, and the session's id.
#[allow(clippy::too_many_arguments)]
pub fn run_session(calls: &Calls, call: &Call, input: &Value, harness: &str, model: &str, proxy: &super::facts::Proxy, coord: &str, resume: Option<&str>) -> Result<(Value, String), Failure> {
    let prompt = calls.render(call, input);
    let schema = json!({"type": "object", "properties": {"answer": calls.schema(&call.returns)}, "required": ["answer"], "additionalProperties": false});
    let (v, id) = super::harness::turn(harness, coord, proxy, model, &prompt, &schema, resume)?;
    let a = v.get("answer").cloned().ok_or(Failure::Invalid("no `answer`".into()))?;
    calls.conforms(&call.returns, &a).map_err(Failure::Invalid)?;
    Ok((a, id))
}

/// The answer in a chat completion V, checked.
pub fn answer(calls: &Calls, call: &Call, v: &Value) -> Result<Value, Failure> {
    let choice = &v["choices"][0];
    match choice["finish_reason"].as_str() {
        Some("length") => return Err(Failure::Truncated),
        Some("content_filter") => return Err(Failure::Refused("content filter".into())),
        _ => {}
    }
    if let Some(r) = choice["message"]["refusal"].as_str() {
        return Err(Failure::Refused(r.into()));
    }
    let text = choice["message"]["content"].as_str().ok_or(Failure::Invalid("no content".into()))?;
    // a model the proxy does not hold to the schema (Claude's Haiku 4.5) may fence its JSON as a code block: the fence
    // is the transport's, the content is checked like any other
    let text = text.trim();
    let text = text.strip_prefix("```").and_then(|t| t.strip_suffix("```")).map_or(text, |t| t.split_once('\n').map_or(t, |(_, body)| body));
    let parsed: Value = serde_json::from_str(text.trim()).map_err(|e| Failure::Invalid(format!("not JSON: {e}")))?;
    let a = parsed.get("answer").cloned().ok_or(Failure::Invalid("no `answer`".into()))?;
    calls.conforms(&call.returns, &a).map_err(Failure::Invalid)?;
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "type AgentMessage {\n  id: String\n  text: String\n  thread: String?\n}\ntype QuestionRef {\n  question: String\n  quote: String\n}\ncall recognise-question(message: AgentMessage) -> QuestionRef?\n  task recognise-question\n  template\n    Message {{ message.id }}:\n    {{ message.text }}\n    {{ if message.thread }}In thread {{ message.thread }}.{{ end }}\n";

    fn calls(src: &str) -> Result<Calls, String> {
        let dir = std::env::temp_dir().join(format!("coordd-calls-{}-{}", std::process::id(), src.len()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("t.call"), src).unwrap();
        let r = Calls::load(&[dir.display().to_string()], &Default::default());
        std::fs::remove_dir_all(&dir).ok();
        r
    }

    #[test]
    fn a_template_is_checked_and_rendered_and_answers_are_checked() {
        let c = calls(SRC).unwrap();
        let call = &c.calls["recognise-question"];
        let input = json!({"message": {"id": "m3", "text": "Merge now?", "thread": null}});
        assert_eq!(c.render(call, &input), "Message m3:\nMerge now?\n");
        let ok = json!({"choices": [{"finish_reason": "stop", "message": {"content": "{\"answer\": {\"question\": \"Merge now?\", \"quote\": \"Merge now?\"}}"}}]});
        assert_eq!(answer(&c, call, &ok).unwrap()["question"], "Merge now?");
        let fenced = json!({"choices": [{"finish_reason": "stop", "message": {"content": "```json\n{\"answer\": null}\n```"}}]});
        assert_eq!(answer(&c, call, &fenced).unwrap(), Value::Null);
        let none = json!({"choices": [{"finish_reason": "stop", "message": {"content": "{\"answer\": null}"}}]});
        assert_eq!(answer(&c, call, &none).unwrap(), Value::Null);
        let cut = json!({"choices": [{"finish_reason": "length", "message": {"content": "{\"answer\": {"}}]});
        assert_eq!(answer(&c, call, &cut), Err(Failure::Truncated));
        let bad = json!({"choices": [{"finish_reason": "stop", "message": {"content": "{\"answer\": {\"question\": 3, \"quote\": \"x\"}}"}}]});
        assert!(matches!(answer(&c, call, &bad), Err(Failure::Invalid(_))));
        assert_eq!(c.schema(&call.returns)["anyOf"][0]["required"], json!(["question", "quote"]));
    }

    #[test]
    fn a_hole_on_an_optional_or_a_missing_field_is_refused_at_load() {
        let e = calls(&SRC.replace("{{ if message.thread }}In thread {{ message.thread }}.{{ end }}", "{{ message.thread }}")).unwrap_err();
        assert!(e.contains("a hole takes"), "{e}");
        let e = calls(&SRC.replace("message.id", "message.sender")).unwrap_err();
        assert!(e.contains("no field `sender`"), "{e}");
    }
}
