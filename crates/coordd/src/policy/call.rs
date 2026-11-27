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
//! `type Clarity = Clear | Unclear(why: String)`, or `type Clarity =` with an alternative a line, each starting with
//! `|` and with its own `///` doc after it: its value is a JSON object whose `kind` names the variant, with the
//! variant's fields beside it (`{"kind": "Unclear", "why": "..."}`); a step's answer is matched on it. The template's holes are checked
//! when the file is loaded: `{{ path }}` names a String, Int or Bool reachable from the inputs (an optional one only
//! inside an `{{ if path }}` that tests it); `{{ if path }}` takes a Bool or an optional; `{{ for x in path }}` a
//! list; `{{ match path }}`, on a value of a type with alternatives, with an arm `{{ C(a, b) }}` for each alternative
//! and `{{ end }}`, means what `match` means in a program: the value's alternative's arm, its names bound to that
//! alternative's first fields; `{{ include "file" }}` another template file beside this one. The return type is the JSON schema sent with
//! the call (as `{"answer": T}`, an object at the root as the providers require), and the answer is checked against
//! the same type; a refusal, a cut-off answer or one that does not check is a failure of that model, never an
//! answer (§5.4).

use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
// a template's nodes and their checks: docket-machine's, which docket-ls reads too (paper-research N-1446 p1842)
use docket_machine::template::{Arm, Node, Shape, arms_of};

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

/// A call program.
#[derive(Clone, Debug)]
pub struct Call {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub returns: Type,
    #[cfg_attr(not(test), allow(dead_code))] // read by the call schemas' record
    pub task: String,
    template: Vec<Node>,
    #[allow(dead_code)] // where the call is, for a reader of the parsed calls
    pub at: String,
}

/// The calls and types of a folder of `.call` files.
#[derive(Clone, Debug, Default)]
pub struct Calls {
    pub types: BTreeMap<String, Record>,
    /// Enum types: their variants in order, each with its fields.
    pub enums: BTreeMap<String, Vec<(String, Record)>>,
    pub calls: BTreeMap<String, Call>,
    /// The ticket steps' calls (`Text`, `Ref`, an inline variant: docket's types, which the machine reads and checks
    /// itself, docket_machine::syntax::parse_calls): only their templates, by name, unchecked, each with its folder.
    steps: BTreeMap<String, (String, Vec<Node>)>,
    /// Every call as the one parser read it, and the named types: what the machine's kinds are checked against
    /// (Tickets::load takes them from here, so each folder is read once).
    pub defs: BTreeMap<String, docket_machine::syntax::CallDef>,
    pub named: docket_machine::syntax::Named,
    /// The docs of the named types, by name (`///`, paper-research N-101 p656): what a step gives is shown with them.
    pub type_docs: BTreeMap<String, docket_machine::syntax::Doc>,
    /// Why a file or a call was left out, with its file and line: a bad file costs only its own calls (paper-research
    /// m734). Shown where kind refusals show (tickets.errors) and with the nets' errors.
    pub errors: Vec<String>,
    /// The kind files beside the calls (agent-base's, the library's, the project's), as read with them: what a step's
    /// gives reads which calls an agent named in it is given later (`render::gives`).
    pub kind_files: Vec<(String, String)>,
}

/// What the one parser of call files (`docket_machine::syntax::parse_call_files`, paper-research m723) reads, in the
/// types the nets and templates here use: a docket type as a `Type`, when it is one of these (`Text`, which a file
/// writes `String`, `Int`, `Bool`, `T?`, `[T]`, or a record or sum declared with a name, by that name). None for docket's
/// own (`Ref`, `Ticket`, `Agent`, a sum written inline in a call), which only ticket steps use.
fn conv(named: &docket_machine::syntax::Named, t: &docket_machine::Type) -> Option<Type> {
    use docket_machine::Type as D;
    Some(match t {
        D::Text => Type::Str,
        D::Int => Type::Int,
        D::Bool => Type::Bool,
        D::Opt(x) => Type::Opt(Box::new(conv(named, x)?)),
        D::List(x) => Type::List(Box::new(conv(named, x)?)),
        D::Record(_) | D::Variant(_) => Type::Named(named.iter().find(|(_, n)| *n == t).map(|(k, _)| k.clone())?),
        _ => return None,
    })
}

impl Calls {
    /// The `.call` files of the folders DIRS (a later folder's call or type of the same name replaces an earlier one's:
    /// the project's over the system's), read by the one parser of call files with the system's types first.
    pub fn load(dirs: &[String], overlay: &super::Overlay) -> Result<Calls, String> {
        use docket_machine::syntax;
        let mut sources: Vec<(String, String)> = Vec::new();
        let mut dir_of: BTreeMap<String, String> = BTreeMap::new();
        for dir in dirs {
            for (f, text) in super::files_in(dir, "call", overlay)? {
                dir_of.insert(f.clone(), dir.clone());
                sources.push((f, text));
            }
        }
        let mut named = syntax::Named::new();
        syntax::parse_types("system", syntax::SYSTEM_TYPES, &mut named)?;
        // one name, one type: a second file's `type Piece` had silently replaced the clerk's, whose `split` then panicked
        // making its schema (file.call's Piece names docket's Ticket). Read from the `type NAME` lines, as the parser
        // finds them, so a type may still name one declared later or in another file; a project's folder may replace one
        let mut declared: BTreeMap<String, String> = BTreeMap::new();
        let mut bad: Vec<String> = Vec::new();
        let mut kept: Vec<(String, String)> = Vec::new();
        'file: for (file, text) in sources {
            let mut mine: Vec<String> = Vec::new();
            for l in text.lines() {
                let Some(rest) = l.strip_prefix("type ") else { continue };
                let t = rest.split(['=', '{', '/']).next().unwrap_or("").trim().to_string();
                let other = declared.get(&t).filter(|o| dir_of.get(*o) == dir_of.get(&file));
                let why = if named.contains_key(&t) {
                    Some(format!("{file}: type {t} is the system's (the nets' built-ins use it); name yours otherwise"))
                } else {
                    other.map(|o| format!("{file}: type {t} is declared in {o} too; give one of them another name"))
                };
                if let Some(why) = why {
                    bad.push(format!("{why}; the calls of {file} are left out"));
                    continue 'file;
                }
                mine.push(t);
            }
            for t in mine {
                declared.insert(t, file.clone());
            }
            kept.push((file, text));
        }
        let sources = kept;
        let (mut defs, errors) = syntax::parse_call_files(&sources, &mut named);
        let type_docs = syntax::type_docs(&sources);
        let mut out = Calls { errors: bad, ..Default::default() };
        out.errors.extend(errors);
        // the named types, by their names, as the nets' checker looks them up (`Clarity`, `Wake`, their fields)
        for (n, t) in &named {
            match t {
                docket_machine::Type::Record(fs) => {
                    if let Some(fs) = fs.iter().map(|(f, t)| Some((f.clone(), conv(&named, t)?))).collect::<Option<Record>>() {
                        out.types.insert(n.clone(), fs);
                    }
                }
                docket_machine::Type::Variant(cs) => {
                    let vs = cs.iter().map(|(c, fs)| Some((c.clone(), fs.iter().map(|(f, t)| Some((f.clone(), conv(&named, t)?))).collect::<Option<Record>>()?))).collect::<Option<Vec<_>>>();
                    if let Some(vs) = vs {
                        out.enums.insert(n.clone(), vs);
                    }
                }
                _ => {}
            }
        }
        // a doc written `kind NAME` is that kind's first doc line, read here from the kinds beside the calls (agent-base's,
        // the library's, the project's); with no such kind, each call giving it is left out (paper-research N-101 p906)
        let mut kind_files: Vec<(String, String)> = Vec::new();
        for dir in dirs {
            let kinds = dir.strip_suffix("/calls").map(|p| format!("{p}/kinds")).unwrap_or_else(|| dir.clone());
            kind_files.extend(super::files_in(&kinds, "kind", overlay).unwrap_or_default());
        }
        let kinds = syntax::kind_lines(&kind_files);
        out.kind_files = kind_files;
        let mut type_docs = type_docs;
        let places: BTreeMap<String, String> = sources.iter().filter_map(|(f, t)| syntax::type_places(f, t).ok().map(|ps| (f, ps))).flat_map(|(f, ps)| ps.into_iter().map(move |(n, l)| (n, format!("{f}:{l}")))).collect();
        let mut bad_types: BTreeMap<String, String> = BTreeMap::new();
        for (t, doc) in type_docs.iter_mut() {
            let why = syntax::kind_docs(doc, &kinds);
            if !why.is_empty() {
                let at = places.get(t).cloned().unwrap_or_default();
                bad_types.insert(t.clone(), format!("{at}: type {t}: {}", why.join("; ")));
            }
        }
        let mut given: BTreeSet<String> = BTreeSet::new();
        let mut left_out: Vec<String> = Vec::new();
        for (name, d) in defs.iter_mut() {
            let mut why = syntax::kind_docs(&mut d.doc, &kinds);
            if let docket_machine::Type::Variant(cs) = &d.sig.head {
                for (c, _) in cs {
                    if let Some(w) = bad_types.get(c) {
                        why.push(w.clone());
                        given.insert(c.clone());
                    }
                }
            }
            if !why.is_empty() {
                out.errors.push(format!("{}:{}: {}; the call {name} is left out", d.file, d.line, why.join("; ")));
                left_out.push(name.clone());
            }
        }
        out.errors.extend(bad_types.iter().filter(|(t, _)| !given.contains(*t)).map(|(_, w)| w.clone()));
        for (name, d) in &defs {
            if left_out.contains(name) {
                continue;
            }
            let at = format!("{}:{}", d.file, d.line);
            match out.one(name, d, &named, &dir_of) {
                Ok(()) => {}
                Err(e) => {
                    out.errors.push(format!("{at}: {e}; the call {name} is left out"));
                    left_out.push(name.clone());
                }
            }
        }
        for n in left_out {
            defs.remove(&n);
        }
        out.defs = defs;
        out.named = named;
        out.type_docs = type_docs;
        Ok(out)
    }

    /// Read call NAME as the parser gave it (D) into self: a nets' call checked whole, a step's template kept.
    fn one(&mut self, name: &str, d: &docket_machine::syntax::CallDef, named: &docket_machine::syntax::Named, dir_of: &BTreeMap<String, String>) -> Result<(), String> {
        let out = self;
        {
            let dir = dir_of.get(&d.file).cloned().unwrap_or_default();
            let nodes = match &d.template {
                Some(t) => Some(out.template(&dir, t, d.template_line)?),
                None => None,
            };
            let params: Option<Vec<(String, Type)>> = d.sig.params.iter().map(|(p, t)| Some((p.clone(), conv(named, t)?))).collect();
            let returns = conv(named, &d.sig.head);
            // a call in docket's own types: a ticket step's, its template kept for the renderings; a program's call
            // (file.call's `file`, answered by coordd itself) has none, and nothing to render
            let (Some(params), Some(returns)) = (params, returns) else {
                if let Some(nodes) = nodes {
                    // its holes checked against its parameters' docket types (docket-core m745): a hole renders blank
                    // when its value is missing, so a misspelt one would go out silently
                    let scope: BTreeMap<String, docket_machine::Type> = d.sig.params.iter().cloned().collect();
                    docket_machine::template::check_step(&nodes, &scope).map_err(|e| format!("the template: {e}"))?;
                    out.steps.insert(name.to_string(), (dir, nodes));
                }
                return Ok(());
            };
            let nodes = nodes.ok_or("the call has no template")?;
            let task = d.attr("task").ok_or("the call has no `task`")?.to_string();
            let call = Call { name: name.to_string(), params, returns, task, template: nodes, at: format!("{}:{}", d.file, d.line) };
            out.check_type(&call.returns)?;
            let scope: BTreeMap<String, Type> = call.params.iter().cloned().collect();
            out.check(&call.template, &scope).map_err(|e| format!("the template: {e}"))?;
            out.calls.insert(call.name.clone(), call);
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

    /// Parse a template read from a file in DIR, its first line line FIRST of its file (0: not known): docket-machine's
    /// `template::parse`, an `{{ include "f" }}` read beside it.
    fn template(&self, dir: &str, text: &str, first: usize) -> Result<Vec<Node>, String> {
        docket_machine::template::parse(text, first, &|f| std::fs::read_to_string(format!("{dir}/{f}")).map_err(|e| e.to_string()))
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
                Node::Hole(p, _, _) => match self.path_type(scope, p)? {
                    Type::Str | Type::Int | Type::Bool => {}
                    t => return Err(format!("{{{{ {} }}}} is a {t:?}: a hole takes a String, Int or Bool", p.join("."))),
                },
                Node::If(p, _, body) => {
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
                Node::For(x, p, _, body) => match self.path_type(scope, p)? {
                    Type::List(t) => {
                        let mut inner = scope.clone();
                        inner.insert(x.clone(), *t);
                        self.check(body, &inner)?;
                    }
                    t => return Err(format!("{{{{ for {x} in {} }}}} goes over a {t:?}: it takes a list", p.join("."))),
                },
                Node::Match(p, line, arms) => {
                    let alts = match self.path_type(scope, p)? {
                        Type::Named(n) if self.enums.contains_key(&n) => self.enums[&n].clone(),
                        t => return Err(format!("{}{{{{ match {} }}}} is on a {t:?}: it takes a value of a type with alternatives", if *line == 0 { String::new() } else { format!("line {line}: ") }, p.join("."))),
                    };
                    let list = alts.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>().join(", ");
                    for (a, bound) in arms.iter().zip(arms_of(p, *line, arms, &alts, &list)?) {
                        let mut inner = scope.clone();
                        inner.extend(bound);
                        self.check(&a.body, &inner)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// `check`, where a dotted path narrowed by an `if` is looked up whole first.
    fn check_narrowed(&self, nodes: &[Node], scope: &BTreeMap<String, Type>) -> Result<(), String> {
        let lifted: Vec<Node> = nodes.iter().map(|n| lift(n, scope)).collect();
        self.check(&lifted, scope)
    }

    #[cfg(test)] // the call schemas' record and the template tests
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
                let Some(fields) = self.types.get(n) else { return json!({}) }; // a type the calls do not know (docket's): any
                let props: serde_json::Map<String, Value> = fields.iter().map(|(f, t)| (f.clone(), self.schema(t))).collect();
                json!({"type": "object", "properties": props, "required": fields.iter().map(|(f, _)| f.clone()).collect::<Vec<_>>(), "additionalProperties": false})
            }
        }
    }

    /// TEXT, a template of the same language read from a file in DIR (`include` looks there), filled from ENV
    /// unchecked: a hole ENV lacks is empty. For the renderings (agent-base/renderings/, `crate::render`), whose
    /// inputs are made by code, not declared.
    pub fn render_text(dir: &str, text: &str, env: &Value) -> Result<String, String> {
        let nodes = Calls::default().template(dir, text, 1)?;
        let mut out = String::new();
        render(&nodes, env, &mut out);
        Ok(out)
    }

    /// The prompt of the ticket step call NAME for ARGS (unchecked: the machine checked them), None when no `.call`
    /// file has it.
    pub fn render_step(&self, name: &str, args: &Value) -> Option<String> {
        // a step's call whose types are all the policy's too (`-> Text`, as the library's assemble_plan) was read as a
        // policy call: its template is there (N-159)
        let nodes = match self.steps.get(name) {
            Some((_, nodes)) => nodes,
            None => &self.calls.get(name)?.template,
        };
        let mut out = String::new();
        render(nodes, args, &mut out);
        Some(out)
    }

    /// TEXT read as a template of call NAME in place of its own (the owner's once-edit, `Line::Template`): parsed as a
    /// call's template is, and checked against the call's parameters, so a slot the call does not have is refused
    /// rather than rendered blank.
    fn edited(&self, name: &str, text: &str) -> Result<Vec<Node>, String> {
        let d = self.defs.get(name).ok_or_else(|| format!("there is no call {name}"))?;
        let dir = std::path::Path::new(&d.file).parent().map(|p| p.display().to_string()).unwrap_or_default();
        let nodes = self.template(&dir, text, 1)?;
        match self.calls.get(name) {
            Some(call) => self.check(&nodes, &call.params.iter().cloned().collect()),
            None => docket_machine::template::check_step(&nodes, &d.sig.params.iter().cloned().collect()),
        }
        .map_err(|e| format!("the template: {e}"))?;
        Ok(nodes)
    }

    /// Whether TEXT reads as a template of call NAME in place of its own (`edited`), and why not.
    pub fn check_edited(&self, name: &str, text: &str) -> Result<(), String> {
        self.edited(name, text).map(|_| ())
    }

    /// The prompt of call NAME for ARGS from TEXT, the owner's template for it (`edited`).
    pub fn render_edited(&self, name: &str, text: &str, args: &Value) -> Result<String, String> {
        let nodes = self.edited(name, text)?;
        let mut out = String::new();
        render(&nodes, args, &mut out);
        Ok(out)
    }

    #[cfg(test)] // the call schemas' record and the template tests
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
        Node::Hole(p, l, how) => Node::Hole(fix(p), *l, how.clone()),
        Node::If(p, l, b) => Node::If(fix(p), *l, b.iter().map(|n| lift(n, scope)).collect()),
        Node::For(x, p, l, b) => Node::For(x.clone(), fix(p), *l, b.iter().map(|n| lift(n, scope)).collect()),
        Node::Match(p, l, arms) => Node::Match(fix(p), *l, arms.iter().map(|a| Arm { body: a.body.iter().map(|n| lift(n, scope)).collect(), ..a.clone() }).collect()),
    }
}

fn lookup<'a>(v: &'a Value, path: &[String]) -> &'a Value {
    path.iter().fold(v, |v, f| v.get(f).unwrap_or(&Value::Null))
}

fn render(nodes: &[Node], env: &Value, out: &mut String) {
    for n in nodes {
        match n {
            Node::Text(t) => out.push_str(t),
            Node::Hole(p, _, how) => match (lookup(env, p), how.get()) {
                (v, Some(Shape::Ref)) if docket_machine::types::ref_words(v).is_some() => out.push_str(&docket_machine::types::ref_words(v).unwrap_or_default()),
                (Value::Array(items), Some(Shape::Refs)) => {
                    let lines: Vec<String> = items.iter().map(|v| docket_machine::types::ref_words(v).unwrap_or_else(|| v.to_string())).collect();
                    out.push_str(&lines.join("\n"))
                }
                (Value::String(s), _) => out.push_str(s),
                (Value::Null, _) => {}
                (Value::Array(items), _) => {
                    let lines: Vec<String> = items.iter().map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string)).collect();
                    out.push_str(&lines.join("\n"))
                }
                (v, _) => out.push_str(&v.to_string()),
            },
            Node::If(p, _, body) => {
                let v = lookup(env, p);
                if !(v.is_null() || *v == Value::Bool(false)) {
                    render(body, env, out);
                }
            }
            Node::For(x, p, _, body) => {
                for item in lookup(env, p).as_array().cloned().unwrap_or_default() {
                    let mut inner = env.clone();
                    inner[x.as_str()] = item;
                    render(body, &inner, out);
                }
            }
            Node::Match(p, _, arms) => {
                // the alternative's name: `head` in docket's values (a step's), `kind` in the nets' enums
                let v = lookup(env, p);
                let Some(c) = v.get("head").or_else(|| v.get("kind")).and_then(Value::as_str) else { continue };
                let Some(a) = arms.iter().find(|a| a.ctor == c) else { continue };
                let mut inner = env.clone();
                for (n, f) in a.names.iter().zip(a.fields.get().into_iter().flatten()) {
                    inner[n.as_str()] = v.get(f).cloned().unwrap_or(Value::Null);
                }
                render(&a.body, &inner, out);
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
    /// Every place that serves the model is out (the router, `accounts::router`): the owner's accounts that run it at
    /// their limits, the first back then (seconds; [`crate::accounts::limits::UNKNOWN_RESET`] when none said), and the
    /// gateway not serving it.
    Out(i64),
}

impl Failure {
    /// The gateway refused the model for a limit: its 429 saying `usage_limit_reached`, itself or as the last error of
    /// a `model_cooldown`, as every call coordd made from 20:09Z on 20 Nov was refused (`429 Too Many Requests:
    /// {"code":"model_cooldown","last_upstream_error":"usage_limit_reached: …"}`, paper-research N-1397 p117; all 19
    /// cooldowns in research's record say so, consult-3 p125). Not a cooldown after another error (the gateway puts a
    /// credential on a 30 s cooldown after a transient one: a hiccup, not a limit, p125), not another 429 (a rate
    /// limit), and not a model not served. And every place that serves the model out ([`Failure::Out`]).
    pub fn at_limit(&self) -> bool {
        matches!(self, Failure::Out(_)) || matches!(self, Failure::Error(e) if e.starts_with("429") && e.contains("usage_limit_reached"))
    }
}

/// MODEL as a call names it, and its effort: `MODEL:EFFORT`, as an agent's model line says it (roster's MODEL:EFFORT,
/// `claude-opus-5-5:low`), or MODEL alone.
pub(crate) fn model_effort(model: &str) -> (&str, Option<&str>) {
    match model.split_once(':') {
        Some((m, e)) => (m, Some(e).filter(|e| !e.is_empty())),
        None => (model, None),
    }
}

/// The schema a call's answer is held to: `{"answer": SCHEMA}`, every place given the same (the gateway's
/// `response_format`, Claude Code's `--json-schema`).
pub(crate) fn answer_schema(schema: &Value) -> Value {
    json!({"type": "object", "properties": {"answer": schema}, "required": ["answer"], "additionalProperties": false})
}

/// The chat completion a call sends the gateway (`accounts::router`): PROMPT, its answer held to SCHEMA
/// ([`answer_schema`], NAME naming it for the provider), on MODEL; `MODEL:EFFORT` sends the effort as
/// `reasoning_effort`, which our proxy passes on as the provider's (for gpt-5.6-luna, `reasoning.effort`; without it
/// the proxy sends medium).
pub(crate) fn request_body(name: &str, prompt: &str, schema: &Value, model: &str) -> Value {
    let schema = answer_schema(schema);
    let (model, effort) = model_effort(model);
    let mut body = json!({
        "model": model,
        "max_tokens": 2000,
        "messages": [{"role": "user", "content": prompt}],
        "response_format": {"type": "json_schema", "json_schema": {"name": name.replace('-', "_"), "strict": true, "schema": schema}},
    });
    if let Some(e) = effort {
        body["reasoning_effort"] = json!(e);
    }
    body
}

/// V as JSON text with each object's `head` written first. Strict structured output writes an object's properties in
/// the schema's order, and serde_json sorts keys: `To(agents)`'s schema went out as {agents, head} and `Unclear(why)`'s
/// as {head, why}, so a model that began with "head" (as every answer here does) could only finish Unclear. Route
/// answered Unclear with "rule 3 assigns it to lead" as its why (docket-live, 12 Nov).
pub(crate) fn head_first(v: &Value) -> String {
    match v {
        Value::Object(o) => {
            let key = |k: &String| serde_json::to_string(k).unwrap_or_default();
            let fields = o.get_key_value("head").into_iter().chain(o.iter().filter(|(k, _)| k.as_str() != "head"));
            format!("{{{}}}", fields.map(|(k, x)| format!("{}:{}", key(k), head_first(x))).collect::<Vec<_>>().join(","))
        }
        Value::Array(xs) => format!("[{}]", xs.iter().map(head_first).collect::<Vec<_>>().join(",")),
        _ => v.to_string(),
    }
}

/// The answer in a chat completion V, held to CHECK.
pub(crate) fn answer_with(v: &Value, check: &dyn Fn(&Value) -> Result<(), String>) -> Result<Value, Failure> {
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
    answered(&parsed, check)
}

/// What a chat completion V says it read and wrote (its `usage`); None when it says nothing.
pub(crate) fn completion_tokens(v: &Value) -> Option<u64> {
    v["usage"]["total_tokens"].as_u64()
}

/// The `answer` of PARSED, an object of [`answer_schema`], held to CHECK.
pub(crate) fn answered(parsed: &Value, check: &dyn Fn(&Value) -> Result<(), String>) -> Result<Value, Failure> {
    let a = parsed.get("answer").cloned().ok_or(Failure::Invalid("no `answer`".into()))?;
    check(&a).map_err(Failure::Invalid)?;
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A variant's schema reaches the model with `head` first in each case, or strict output can only finish the case
    /// whose other fields sort after "head" (route's To(agents) never could).
    #[test]
    fn a_schema_goes_out_with_head_first() {
        let t = docket_machine::Type::Variant(vec![("To".into(), vec![("agents".into(), docket_machine::Type::List(Box::new(docket_machine::Type::Agent)))]), ("Unclear".into(), vec![("why".into(), docket_machine::Type::Text)])]);
        let body = json!({"messages": [], "response_format": {"schema": t.schema()}});
        let s = head_first(&body);
        assert_eq!(serde_json::from_str::<Value>(&s).unwrap(), body, "the same JSON");
        let to = s.find(r#""enum":["To"]"#).unwrap();
        assert!(s[..to].ends_with(r#"{"head":{"#) || s[..to].contains(r#""properties":{"head":{"#), "{s}");
        let props = &s[s.find(r#""properties":{"#).unwrap()..];
        assert!(props.starts_with(r#""properties":{"head":"#), "{s}");
    }


    /// The one-parser change (paper-research m723-m728) must leave every call as it reads today: for each nets' call
    /// in agent-base, its task and the schemas of its parameters and answer; for each ticket step's call, a rendering
    /// of its template from placeholder arguments. Compared with `testdata/call-schemas.json`, which
    /// `CALL_SCHEMAS_RECORD=1` writes (recorded from the old parser, before the change).
    #[test]
    fn every_agent_base_call_reads_as_recorded() {
        let root = env!("CARGO_MANIFEST_DIR");
        let calls = Calls::load(&[format!("{root}/../../agent-base/calls")], &Default::default()).expect("agent-base/calls load");
        assert!(calls.errors.is_empty(), "{:#?}", calls.errors);
        let mut out = serde_json::Map::new();
        for (name, c) in &calls.calls {
            let params: serde_json::Map<String, Value> = c.params.iter().map(|(p, t)| (p.clone(), calls.schema(t))).collect();
            out.insert(format!("call {name}"), json!({"task": c.task, "params": params, "answer": calls.schema(&c.returns)}));
        }
        for (name, (_, nodes)) in &calls.steps {
            // every hole filled with its own name, every test true: the template's text and holes as they parse
            let mut args = serde_json::Map::new();
            let mut holes = Vec::new();
            fn walk(ns: &[Node], out: &mut Vec<String>) {
                for n in ns {
                    match n {
                        Node::Hole(p, _, _) => out.push(p.join(".")),
                        Node::Match(p, _, arms) => {
                            out.push(p.join("."));
                            arms.iter().for_each(|a| walk(&a.body, out));
                        }
                        Node::If(p, _, a) => {
                            out.push(p.join("."));
                            walk(a, out);
                        }
                        Node::For(_, p, _, b) => {
                            out.push(p.join("."));
                            walk(b, out);
                        }
                        _ => {}
                    }
                }
            }
            walk(nodes, &mut holes);
            for h in holes {
                args.insert(h.split('.').next().unwrap_or("").to_string(), json!(format!("<{h}>")));
            }
            out.insert(format!("step {name}"), json!(calls.render_step(name, &Value::Object(args))));
        }
        let got = serde_json::to_string_pretty(&Value::Object(out)).unwrap();
        let path = format!("{root}/testdata/call-schemas.json");
        if std::env::var("CALL_SCHEMAS_RECORD").is_ok() {
            std::fs::create_dir_all(format!("{root}/testdata")).unwrap();
            std::fs::write(&path, &got).unwrap();
        }
        let want = std::fs::read_to_string(&path).expect("testdata/call-schemas.json: record it with CALL_SCHEMAS_RECORD=1");
        assert!(got == want, "the calls read differently from testdata/call-schemas.json; diff it against:\n{got}");
    }

    const SRC: &str = "type AgentMessage {\n  id: String\n  text: String\n  thread: String?\n}\ntype QuestionRef {\n  question: String\n  quote: String\n}\ncall recognise-question(message: AgentMessage) -> QuestionRef?\n  task recognise-question\n  template\n    Message {{ message.id }}:\n    {{ message.text }}\n    {{ if message.thread }}In thread {{ message.thread }}.{{ end }}\n";

    fn calls(src: &str) -> Result<Calls, String> {
        let dir = std::env::temp_dir().join(format!("coordd-calls-{}-{}", std::process::id(), src.len()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("t.call"), src).unwrap();
        let r = Calls::load(&[dir.display().to_string()], &Default::default());
        std::fs::remove_dir_all(&dir).ok();
        r.and_then(|c| if c.errors.is_empty() { Ok(c) } else { Err(c.errors.join("; ")) })
    }

    fn calls_of(files: &[(&str, &str)]) -> Result<Calls, String> {
        let dir = std::env::temp_dir().join(format!("coordd-calls-files-{}-{}", std::process::id(), files.iter().map(|f| f.1.len()).sum::<usize>()));
        std::fs::create_dir_all(&dir).unwrap();
        for (n, t) in files {
            std::fs::write(dir.join(n), t).unwrap();
        }
        let r = Calls::load(&[dir.display().to_string()], &Default::default());
        std::fs::remove_dir_all(&dir).ok();
        r.and_then(|c| if c.errors.is_empty() { Ok(c) } else { Err(c.errors.join("; ")) })
    }

    #[test]
    fn a_bad_file_costs_only_its_own_calls() {
        let dir = std::env::temp_dir().join(format!("coordd-calls-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("good.call"), "call echo(text: Text) -> Said(text: Text)\n  task echo\n  template\n    {{ text }}\n").unwrap();
        std::fs::write(dir.join("bad.call"), "type Broken {\n  k: Nowhere\n}\ncall broken(b: Broken) -> Broken\n  task broken\n  template\n    x\n").unwrap();
        std::fs::write(dir.join("hole.call"), "call holey(text: Text) -> Text\n  task holey\n  template\n    {{ nothing }}\n").unwrap();
        std::fs::write(dir.join("step.call"), "call fixit(what: Text, note: Text?) -> Fixed(at: Ref) | NoFix(why: Text)\n  task fixer\n  template\n    {{ what }}{{ if note }} {{ note }}{{ end }} {{ wat }}\n").unwrap();
        let c = Calls::load(&[dir.display().to_string()], &Default::default()).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert!(c.defs.contains_key("echo") && c.render_step("echo", &serde_json::json!({"text": "hi"})).is_some(), "{:?}", c.errors);
        assert!(!c.defs.contains_key("broken") && !c.defs.contains_key("holey") && !c.defs.contains_key("fixit"), "{:?}", c.defs.keys());
        assert!(c.errors.iter().any(|e| e.contains("no `wat` here")), "{:?}", c.errors);
        assert!(c.errors.iter().any(|e| e.contains("bad.call")) && c.errors.iter().any(|e| e.contains("holey")), "{:?}", c.errors);
    }

    /// A list of plain values fills a hole of a step's template one per line (paper-research N-101.p6); a record, or a
    /// list of records, still does not.
    #[test]
    fn a_list_of_plain_values_fills_a_hole_one_per_line() {
        let dir = std::env::temp_dir().join(format!("coordd-calls-list-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("t.call"), "type Handed {\n  commit: Ref\n  by: Agent\n}\ncall trace(log: Text, carries: [Ticket]) -> Part(piece: Ticket) | Unclear\n  task trace\n  template\n    {{ log }}\n    {{ carries }}\ncall whole(piece: Handed) -> Seen\n  task t\n  template\n    {{ piece }}\ncall many(pieces: [Handed]) -> Seen\n  task t\n  template\n    {{ pieces }}\n").unwrap();
        let c = Calls::load(&[dir.display().to_string()], &Default::default()).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(c.render_step("trace", &json!({"log": "red", "carries": ["E-2", "E-5"]})).as_deref(), Some("red\nE-2\nE-5"), "{:?}", c.errors);
        assert!(!c.defs.contains_key("whole") && !c.defs.contains_key("many"), "{:?}", c.errors);
    }

    #[test]
    fn a_type_may_name_one_in_a_later_file_but_one_name_is_one_type() {
        let a = "type Piece {\n  kind: Kind\n  text: Text\n}\ncall split(text: Text) -> [Piece]\n  task clerk\n  template\n    {{ text }}\n";
        let c = calls_of(&[("a.call", a), ("b.call", "type Kind = Question | Answer\n")]).unwrap();
        assert!(c.calls.contains_key("split") && c.enums.contains_key("Kind"));
        let e = calls_of(&[("a.call", a), ("b.call", "type Kind = Question | Answer\ntype Piece = Kind\n")]).unwrap_err();
        assert!(e.contains("declared in"), "{e}");
    }

    #[test]
    fn a_template_is_checked_and_rendered() {
        let c = calls(SRC).unwrap();
        let call = &c.calls["recognise-question"];
        let input = json!({"message": {"id": "m3", "text": "Merge now?", "thread": null}});
        assert_eq!(c.render(call, &input), "Message m3:\nMerge now?\n");
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
