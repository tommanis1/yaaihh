//! The net language's surface: events, nets and bindings.
//!
//! ```text
//! event reminder { agent: String, text: String }           // an event a net may raise (system events are built in)
//!
//! net turn_end on turn_end(t)
//! session clerk: per agent
//! {
//!     let pieces = file(t.text) in clerk                    // a call (calls/file.call), in the session
//!     for q in pieces where q.kind == Question { ask(t.agent, q.text) }
//!     let s = state(t.text, open: questions(t.agent)) in clerk
//!     match decide end-of-turn(agent: t.agent, state: s, nudged: nudged_in_row(t.agent)) {
//!         accept => accept(wake: none)
//!         nudge  => nudge(t.agent, "...")
//!         escalate => { tell(owner, "{{ t.agent }} stopped"); accept(wake: none) }
//!     }
//! }
//! ```
//!
//! Statements: `let x = e`, `path := e` (replace), `path += e` (add to a list), `if c { } else { }`, `match e { arm
//! => stmt }`, `for x in e [where c] { }` (one by one) and `par for` (at once), and the effects `deliver`, `reject`,
//! `nudge`, `ask`, `tell`, `accept`, `fault NAME(...)`, `raise NAME(...)`. A call is `f(args) fresh` or `f(args) in
//! S`, optionally `with field: value, ...`; `decide d(name: value, ...)` is a policy decision; `f(args)` alone is a
//! built-in (`questions`, `nudged_in_row`, `joined`, `len`, and the wakes `answer`, `check`, `mail`). Strings take `{{ e }}`
//! holes. Comments run from `//` to the line's end. Bindings are lines `on EVENT [where agent in {a, b}] use NET`.

use std::fmt;

/// Where something is: file and line.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct At {
    pub file: String,
    pub line: usize,
}

impl fmt::Display for At {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.file, self.line)
    }
}

#[derive(Clone, Debug, Default)]
pub struct File {
    pub events: Vec<EventDecl>,
    pub nets: Vec<Net>,
}

#[derive(Clone, Debug)]
pub struct EventDecl {
    pub name: String,
    pub fields: Vec<(String, String)>,
    pub at: At,
}

#[derive(Clone, Debug)]
pub struct Net {
    pub name: String,
    pub event: String,
    pub var: String,
    pub sessions: Vec<SessionDecl>,
    pub body: Vec<Stmt>,
    pub at: At,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Per {
    Agent,
    Owner,
}

#[derive(Clone, Debug)]
pub struct SessionDecl {
    pub name: String,
    pub per: Per,
    pub at: At,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Place {
    Fresh,
    In(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// A part of a string literal: text or a `{{ e }}` hole.
#[derive(Clone, Debug)]
pub enum Part {
    Text(String),
    Hole(Expr),
}

#[derive(Clone, Debug)]
pub struct Arg {
    pub name: Option<String>,
    pub value: Expr,
}

#[derive(Clone, Debug)]
pub enum Expr {
    /// `x`, `x.f.g`.
    Path(Vec<String>),
    Str(Vec<Part>),
    Int(i64),
    Bool(bool),
    /// `none` (and `new`, a new thread).
    None,
    /// `owner`.
    Owner,
    /// A capitalised name: an enum variant (`Question`), compared with an enum value.
    Variant(String),
    /// `f(args) fresh | in S [with f: v, ...]`.
    Call { name: String, args: Vec<Arg>, place: Place, with: Vec<(String, Expr)>, at: At },
    /// `decide d(f: v, ...) [with f: v, ...]`.
    Decide { name: String, args: Vec<Arg>, with: Vec<(String, Expr)>, at: At },
    /// `f(args)`: a built-in.
    Builtin { name: String, args: Vec<Arg> },
    Cmp(Box<Expr>, Op, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Pat {
    /// `Unclear(why)`: an enum variant, its fields bound by position.
    Variant(String, Vec<String>),
    /// `q?`: an optional's value.
    Some(String),
    /// `none`.
    None,
    /// `accept`: a decision's option.
    Option(String),
    /// `_`.
    Wild,
}

#[derive(Clone, Debug)]
pub struct Arm {
    pub pat: Pat,
    pub guard: Option<Expr>,
    pub body: Vec<Stmt>,
    pub at: At,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Deliver,
    Reject,
    Nudge,
    Ask,
    Post,
    Tell,
    Accept,
    Fault(String),
    Raise(String),
}

impl Effect {
    pub fn name(&self) -> String {
        match self {
            Effect::Deliver => "deliver".into(),
            Effect::Reject => "reject".into(),
            Effect::Nudge => "nudge".into(),
            Effect::Ask => "ask".into(),
            Effect::Post => "post".into(),
            Effect::Tell => "tell".into(),
            Effect::Accept => "accept".into(),
            Effect::Fault(n) => format!("fault {n}"),
            Effect::Raise(n) => format!("raise {n}"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Let { var: String, value: Expr, at: At },
    /// `path := e` (ADD false) or `path += e` (ADD true).
    Set { path: Vec<String>, value: Expr, add: bool, at: At },
    If { cond: Expr, then: Vec<Stmt>, els: Vec<Stmt>, at: At },
    Match { scrut: Expr, arms: Vec<Arm>, at: At },
    For { var: String, list: Expr, filter: Option<Expr>, par: bool, body: Vec<Stmt>, at: At },
    Effect { effect: Effect, args: Vec<Arg>, at: At },
}

impl Net {
    /// The `.call`s the net makes, by name, in order of first use.
    pub fn calls(&self) -> Vec<String> {
        fn expr(e: &Expr, out: &mut Vec<String>) {
            match e {
                Expr::Call { name, args, with, .. } => {
                    if !out.contains(name) {
                        out.push(name.clone());
                    }
                    args.iter().for_each(|a| expr(&a.value, out));
                    with.iter().for_each(|(_, v)| expr(v, out));
                }
                Expr::Decide { args, with, .. } => {
                    args.iter().for_each(|a| expr(&a.value, out));
                    with.iter().for_each(|(_, v)| expr(v, out));
                }
                Expr::Builtin { args, .. } => args.iter().for_each(|a| expr(&a.value, out)),
                Expr::Str(parts) => parts.iter().for_each(|p| if let Part::Hole(h) = p { expr(h, out) }),
                Expr::Cmp(a, _, b) | Expr::And(a, b) | Expr::Or(a, b) => {
                    expr(a, out);
                    expr(b, out);
                }
                Expr::Not(a) => expr(a, out),
                Expr::Path(_) | Expr::Int(_) | Expr::Bool(_) | Expr::None | Expr::Owner | Expr::Variant(_) => {}
            }
        }
        fn block(ss: &[Stmt], out: &mut Vec<String>) {
            for s in ss {
                match s {
                    Stmt::Let { value, .. } | Stmt::Set { value, .. } => expr(value, out),
                    Stmt::If { cond, then, els, .. } => {
                        expr(cond, out);
                        block(then, out);
                        block(els, out);
                    }
                    Stmt::Match { scrut, arms, .. } => {
                        expr(scrut, out);
                        for a in arms {
                            a.guard.iter().for_each(|g| expr(g, out));
                            block(&a.body, out);
                        }
                    }
                    Stmt::For { list, filter, body, .. } => {
                        expr(list, out);
                        filter.iter().for_each(|f| expr(f, out));
                        block(body, out);
                    }
                    Stmt::Effect { args, .. } => args.iter().for_each(|a| expr(&a.value, out)),
                }
            }
        }
        let mut out = Vec::new();
        block(&self.body, &mut out);
        out
    }
}

impl Stmt {
    pub fn at(&self) -> &At {
        match self {
            Stmt::Let { at, .. } | Stmt::Set { at, .. } | Stmt::If { at, .. } | Stmt::Match { at, .. } | Stmt::For { at, .. } | Stmt::Effect { at, .. } => at,
        }
    }
}

/// A binding: EVENT's net, for the agents listed (all when none).
#[derive(Clone, Debug)]
pub struct Binding {
    pub event: String,
    pub agents: Vec<String>,
    pub net: String,
    pub at: At,
}

// ---- lexer ---------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Str(String),
    Int(i64),
    Sym(&'static str),
}

const SYMS: &[&str] = &[":=", "+=", "=>", "==", "!=", "<=", ">=", "{", "}", "(", ")", "[", "]", ",", ":", ".", "?", "<", ">", "=", ";", "|"];

fn lex(file: &str, text: &str, first_line: usize) -> Result<Vec<(Tok, usize)>, String> {
    let mut out = Vec::new();
    let b: Vec<char> = text.chars().collect();
    let (mut i, mut line) = (0, first_line);
    while i < b.len() {
        let c = b[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '/' && b.get(i + 1) == Some(&'/') {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
        } else if c == '"' {
            let start = line;
            let mut s = String::new();
            i += 1;
            loop {
                match b.get(i) {
                    None => return Err(format!("{file}:{start}: an unclosed string")),
                    Some('"') => break,
                    Some('\\') => {
                        match b.get(i + 1) {
                            Some('n') => s.push('\n'),
                            Some(&x) => s.push(x),
                            None => return Err(format!("{file}:{start}: an unclosed string")),
                        }
                        i += 2;
                        continue;
                    }
                    Some(&x) => {
                        if x == '\n' {
                            line += 1;
                        }
                        s.push(x);
                    }
                }
                i += 1;
            }
            i += 1;
            out.push((Tok::Str(s), start));
        } else if c.is_ascii_digit() {
            let mut n = String::new();
            while i < b.len() && b[i].is_ascii_digit() {
                n.push(b[i]);
                i += 1;
            }
            out.push((Tok::Int(n.parse().map_err(|e| format!("{file}:{line}: {e}"))?), line));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let mut n = String::new();
            // names may contain `-` (`end-of-turn`) and `_`
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == '_' || (b[i] == '-' && b.get(i + 1).is_some_and(|x| x.is_ascii_alphanumeric()))) {
                n.push(b[i]);
                i += 1;
            }
            out.push((Tok::Ident(n), line));
        } else {
            let rest: String = b[i..(i + 2).min(b.len())].iter().collect();
            let sym = SYMS.iter().find(|s| rest.starts_with(**s)).ok_or(format!("{file}:{line}: unexpected `{c}`"))?;
            out.push((Tok::Sym(sym), line));
            i += sym.len();
        }
    }
    Ok(out)
}

// ---- parser --------------------------------------------------------------------------------------------------------

struct P<'a> {
    file: &'a str,
    toks: Vec<(Tok, usize)>,
    i: usize,
}

pub const EFFECTS: &[&str] = coord_types::NET_EFFECTS;

impl<'a> P<'a> {
    fn line(&self) -> usize {
        self.toks.get(self.i).or(self.toks.last()).map_or(0, |t| t.1)
    }
    fn at(&self) -> At {
        At { file: self.file.into(), line: self.line() }
    }
    fn err<T>(&self, msg: impl fmt::Display) -> Result<T, String> {
        Err(format!("{}:{}: {msg}", self.file, self.line()))
    }
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i).map(|t| &t.0)
    }
    fn peek_at(&self, k: usize) -> Option<&Tok> {
        self.toks.get(self.i + k).map(|t| &t.0)
    }
    fn is_sym(&self, s: &str) -> bool {
        matches!(self.peek(), Some(Tok::Sym(x)) if *x == s)
    }
    fn is_word(&self, w: &str) -> bool {
        matches!(self.peek(), Some(Tok::Ident(x)) if x == w)
    }
    fn eat_sym(&mut self, s: &str) -> bool {
        let y = self.is_sym(s);
        if y {
            self.i += 1;
        }
        y
    }
    fn eat_word(&mut self, w: &str) -> bool {
        let y = self.is_word(w);
        if y {
            self.i += 1;
        }
        y
    }
    fn sym(&mut self, s: &str) -> Result<(), String> {
        if self.eat_sym(s) { Ok(()) } else { self.err(format!("expected `{s}`, found {}", self.found())) }
    }
    fn word(&mut self, w: &str) -> Result<(), String> {
        if self.eat_word(w) { Ok(()) } else { self.err(format!("expected `{w}`, found {}", self.found())) }
    }
    fn found(&self) -> String {
        match self.peek() {
            None => "the end".into(),
            Some(Tok::Ident(x)) => format!("`{x}`"),
            Some(Tok::Str(_)) => "a string".into(),
            Some(Tok::Int(n)) => format!("`{n}`"),
            Some(Tok::Sym(s)) => format!("`{s}`"),
        }
    }
    fn ident(&mut self) -> Result<String, String> {
        match self.peek() {
            Some(Tok::Ident(x)) => {
                let x = x.clone();
                self.i += 1;
                Ok(x)
            }
            _ => self.err(format!("expected a name, found {}", self.found())),
        }
    }

    fn file(&mut self) -> Result<File, String> {
        let mut f = File::default();
        while self.peek().is_some() {
            if self.is_word("event") {
                f.events.push(self.event()?);
            } else if self.is_word("net") {
                f.nets.push(self.net()?);
            } else {
                return self.err(format!("expected `event` or `net`, found {}", self.found()));
            }
        }
        Ok(f)
    }

    fn event(&mut self) -> Result<EventDecl, String> {
        let at = self.at();
        self.word("event")?;
        let name = self.ident()?;
        self.sym("{")?;
        let mut fields = Vec::new();
        while !self.eat_sym("}") {
            let f = self.ident()?;
            self.sym(":")?;
            fields.push((f, self.type_text()?));
            self.eat_sym(",");
        }
        Ok(EventDecl { name, fields, at })
    }

    /// A type as text (`String`, `[Piece]`, `String?`), checked against the call types later.
    fn type_text(&mut self) -> Result<String, String> {
        let mut t = if self.eat_sym("[") {
            let inner = self.type_text()?;
            self.sym("]")?;
            format!("[{inner}]")
        } else {
            self.ident()?
        };
        while self.eat_sym("?") {
            t.push('?');
        }
        Ok(t)
    }

    fn net(&mut self) -> Result<Net, String> {
        let at = self.at();
        self.word("net")?;
        let name = self.ident()?;
        self.word("on")?;
        let event = self.ident()?;
        self.sym("(")?;
        let var = self.ident()?;
        self.sym(")")?;
        let mut sessions = Vec::new();
        while self.is_word("session") {
            let at = self.at();
            self.i += 1;
            let name = self.ident()?;
            self.sym(":")?;
            self.word("per")?;
            let per = match self.ident()?.as_str() {
                "agent" => Per::Agent,
                "owner" => Per::Owner,
                x => return self.err(format!("`session NAME: per agent` or `per owner`, not `per {x}`")),
            };
            sessions.push(SessionDecl { name, per, at });
        }
        let body = self.block()?;
        Ok(Net { name, event, var, sessions, body, at })
    }

    fn block(&mut self) -> Result<Vec<Stmt>, String> {
        self.sym("{")?;
        let mut out = Vec::new();
        while !self.eat_sym("}") {
            if self.peek().is_none() {
                return self.err("a block without its `}`");
            }
            out.push(self.stmt()?);
            self.eat_sym(";");
        }
        Ok(out)
    }

    /// A block, or one statement (an arm's body).
    fn body(&mut self) -> Result<Vec<Stmt>, String> {
        if self.is_sym("{") { self.block() } else { Ok(vec![self.stmt()?]) }
    }

    fn stmt(&mut self) -> Result<Stmt, String> {
        let at = self.at();
        if self.eat_word("let") {
            let var = self.ident()?;
            self.sym("=")?;
            return Ok(Stmt::Let { var, value: self.expr()?, at });
        }
        if self.eat_word("if") {
            let cond = self.expr()?;
            let then = self.block()?;
            let els = if self.eat_word("else") {
                if self.is_word("if") { vec![self.stmt()?] } else { self.block()? }
            } else {
                Vec::new()
            };
            return Ok(Stmt::If { cond, then, els, at });
        }
        if self.eat_word("match") {
            let scrut = self.expr()?;
            self.sym("{")?;
            let mut arms = Vec::new();
            while !self.eat_sym("}") {
                let at = self.at();
                let pat = self.pat()?;
                let guard = if self.eat_word("if") { Some(self.expr()?) } else { None };
                self.sym("=>")?;
                let body = self.body()?;
                self.eat_sym(",");
                self.eat_sym(";");
                arms.push(Arm { pat, guard, body, at });
            }
            return Ok(Stmt::Match { scrut, arms, at });
        }
        let par = self.is_word("par") && matches!(self.peek_at(1), Some(Tok::Ident(x)) if x == "for");
        if par {
            self.i += 1;
        }
        if self.eat_word("for") {
            let var = self.ident()?;
            self.word("in")?;
            let list = self.expr()?;
            let filter = if self.eat_word("where") { Some(self.expr()?) } else { None };
            let body = self.block()?;
            return Ok(Stmt::For { var, list, filter, par, body, at });
        }
        if self.is_word("fault") || self.is_word("raise") {
            let kw = self.ident()?;
            let name = self.ident()?;
            let args = self.args()?;
            let effect = if kw == "fault" { Effect::Fault(name) } else { Effect::Raise(name) };
            return Ok(Stmt::Effect { effect, args, at });
        }
        if let Some(Tok::Ident(w)) = self.peek() {
            if EFFECTS.contains(&w.as_str()) && matches!(self.peek_at(1), Some(Tok::Sym("("))) {
                let effect = match w.as_str() {
                    "deliver" => Effect::Deliver,
                    "reject" => Effect::Reject,
                    "nudge" => Effect::Nudge,
                    "ask" => Effect::Ask,
                    "post" => Effect::Post,
                    "tell" => Effect::Tell,
                    _ => Effect::Accept,
                };
                self.i += 1;
                let args = self.args()?;
                return Ok(Stmt::Effect { effect, args, at });
            }
        }
        // `path := e` / `path += e`
        let mut path = vec![self.ident()?];
        while self.eat_sym(".") {
            path.push(self.ident()?);
        }
        let add = if self.eat_sym(":=") {
            false
        } else if self.eat_sym("+=") {
            true
        } else {
            return self.err(format!("expected a statement (let, if, match, for, an effect, `x := e`), found `{}` then {}", path.join("."), self.found()));
        };
        Ok(Stmt::Set { path, value: self.expr()?, add, at })
    }

    fn pat(&mut self) -> Result<Pat, String> {
        if self.eat_word("_") {
            return Ok(Pat::Wild);
        }
        if self.eat_word("none") {
            return Ok(Pat::None);
        }
        let n = self.ident()?;
        if self.eat_sym("?") {
            return Ok(Pat::Some(n));
        }
        if n.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            let mut binds = Vec::new();
            if self.eat_sym("(") {
                while !self.eat_sym(")") {
                    binds.push(self.ident()?);
                    self.eat_sym(",");
                }
            }
            return Ok(Pat::Variant(n, binds));
        }
        Ok(Pat::Option(n))
    }

    fn args(&mut self) -> Result<Vec<Arg>, String> {
        self.sym("(")?;
        let mut out = Vec::new();
        while !self.eat_sym(")") {
            let named = matches!((self.peek(), self.peek_at(1)), (Some(Tok::Ident(_)), Some(Tok::Sym(":"))));
            let name = if named {
                let n = self.ident()?;
                self.i += 1;
                Some(n)
            } else {
                None
            };
            out.push(Arg { name, value: self.expr()? });
            if !self.eat_sym(",") && !self.is_sym(")") {
                return self.err(format!("expected `,` or `)`, found {}", self.found()));
            }
        }
        Ok(out)
    }

    fn with(&mut self) -> Result<Vec<(String, Expr)>, String> {
        let mut out = Vec::new();
        if self.eat_word("with") {
            loop {
                let f = self.ident()?;
                self.sym(":")?;
                out.push((f, self.primary()?));
                if !self.eat_sym(",") {
                    break;
                }
            }
        }
        Ok(out)
    }

    fn expr(&mut self) -> Result<Expr, String> {
        let mut e = self.conj()?;
        while self.eat_word("or") {
            e = Expr::Or(Box::new(e), Box::new(self.conj()?));
        }
        Ok(e)
    }

    fn conj(&mut self) -> Result<Expr, String> {
        let mut e = self.neg()?;
        while self.eat_word("and") {
            e = Expr::And(Box::new(e), Box::new(self.neg()?));
        }
        Ok(e)
    }

    fn neg(&mut self) -> Result<Expr, String> {
        if self.eat_word("not") {
            return Ok(Expr::Not(Box::new(self.neg()?)));
        }
        let a = self.primary()?;
        let op = match self.peek() {
            Some(Tok::Sym("==")) => Op::Eq,
            Some(Tok::Sym("!=")) => Op::Ne,
            Some(Tok::Sym("<")) => Op::Lt,
            Some(Tok::Sym("<=")) => Op::Le,
            Some(Tok::Sym(">")) => Op::Gt,
            Some(Tok::Sym(">=")) => Op::Ge,
            _ => return Ok(a),
        };
        self.i += 1;
        Ok(Expr::Cmp(Box::new(a), op, Box::new(self.primary()?)))
    }

    fn primary(&mut self) -> Result<Expr, String> {
        let at = self.at();
        match self.peek().cloned() {
            Some(Tok::Str(s)) => {
                self.i += 1;
                Ok(Expr::Str(parts(self.file, at.line, &s)?))
            }
            Some(Tok::Int(n)) => {
                self.i += 1;
                Ok(Expr::Int(n))
            }
            Some(Tok::Sym("(")) => {
                self.i += 1;
                let e = self.expr()?;
                self.sym(")")?;
                Ok(e)
            }
            Some(Tok::Ident(w)) => {
                self.i += 1;
                match w.as_str() {
                    "true" => return Ok(Expr::Bool(true)),
                    "false" => return Ok(Expr::Bool(false)),
                    "none" | "new" => return Ok(Expr::None),
                    "owner" => return Ok(Expr::Owner),
                    "decide" => {
                        let name = self.ident()?;
                        let args = self.args()?;
                        let with = self.with()?;
                        return Ok(Expr::Decide { name, args, with, at });
                    }
                    _ => {}
                }
                if self.is_sym("(") {
                    let args = self.args()?;
                    let place = if self.eat_word("fresh") {
                        Some(Place::Fresh)
                    } else if self.is_word("in") && matches!(self.peek_at(1), Some(Tok::Ident(_))) {
                        self.i += 1;
                        Some(Place::In(self.ident()?))
                    } else {
                        None
                    };
                    return Ok(match place {
                        Some(place) => {
                            let with = self.with()?;
                            Expr::Call { name: w, args, place, with, at }
                        }
                        None => Expr::Builtin { name: w, args },
                    });
                }
                if w.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                    return Ok(Expr::Variant(w));
                }
                let mut path = vec![w];
                while self.eat_sym(".") {
                    path.push(self.ident()?);
                }
                Ok(Expr::Path(path))
            }
            _ => self.err(format!("expected a value, found {}", self.found())),
        }
    }
}

/// A string literal's parts: text and `{{ e }}` holes.
fn parts(file: &str, line: usize, s: &str) -> Result<Vec<Part>, String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find("{{") {
        if i > 0 {
            out.push(Part::Text(rest[..i].into()));
        }
        let j = rest[i..].find("}}").ok_or(format!("{file}:{line}: an unclosed {{{{ in a string"))? + i;
        let toks = lex(file, &rest[i + 2..j], line)?;
        let mut p = P { file, toks, i: 0 };
        let e = p.expr()?;
        if p.peek().is_some() {
            return p.err("more after the value in {{ }}");
        }
        out.push(Part::Hole(e));
        rest = &rest[j + 2..];
    }
    if !rest.is_empty() {
        out.push(Part::Text(rest.into()));
    }
    Ok(out)
}

/// Parse a `.net` file.
pub fn parse(file: &str, text: &str) -> Result<File, String> {
    let toks = lex(file, text, 1)?;
    P { file, toks, i: 0 }.file()
}

/// Parse a bindings file: `on EVENT [where agent in {a, b}] use NET`, one per line.
pub fn bindings(file: &str, text: &str) -> Result<Vec<Binding>, String> {
    let mut out = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let toks = lex(file, line, n + 1)?;
        let mut p = P { file, toks, i: 0 };
        let at = p.at();
        p.word("on")?;
        let event = p.ident()?;
        let mut agents = Vec::new();
        if p.eat_word("where") {
            p.word("agent")?;
            p.word("in")?;
            p.sym("{")?;
            while !p.eat_sym("}") {
                agents.push(p.ident()?);
                p.eat_sym(",");
            }
            if agents.is_empty() {
                return p.err("`where agent in { }` names no agent");
            }
        }
        p.word("use")?;
        let net = p.ident()?;
        if p.peek().is_some() {
            return p.err(format!("more after `use {net}`"));
        }
        out.push(Binding { event, agents, net, at });
    }
    Ok(out)
}

/// A line of a `live` file: EFFECT is taken for real (LIVE) or only logged, for AGENTS (empty: every agent).
#[derive(Clone, Debug, PartialEq)]
pub struct Switch {
    pub live: bool,
    pub effect: String,
    pub agents: Vec<String>,
    pub at: At,
}

/// Parse a `live` file: one `live EFFECT [where agent in { a, b }]` or `dry EFFECT [...]` per line, `//` comments.
pub fn switches(file: &str, text: &str) -> Result<Vec<Switch>, String> {
    let mut out = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let toks = lex(file, line, n + 1)?;
        let mut p = P { file, toks, i: 0 };
        let at = p.at();
        let live = match p.ident()?.as_str() {
            "live" => true,
            "dry" => false,
            w => return p.err(format!("`{w}`: a line is `live EFFECT` or `dry EFFECT`")),
        };
        let effect = p.ident()?;
        if !EFFECTS.contains(&effect.as_str()) {
            return p.err(format!("no effect `{effect}` ({})", EFFECTS.join(", ")));
        }
        let mut agents = Vec::new();
        if p.eat_word("where") {
            p.word("agent")?;
            p.word("in")?;
            p.sym("{")?;
            while !p.eat_sym("}") {
                agents.push(p.ident()?);
                p.eat_sym(",");
            }
            if agents.is_empty() {
                return p.err("`where agent in { }` names no agent");
            }
        }
        if p.peek().is_some() {
            return p.err(format!("more after `{effect}`"));
        }
        out.push(Switch { live, effect, agents, at });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_net_with_every_statement_parses() {
        let src = r#"
event reminder { agent: String, text: String }
net t on turn_end(t)
session clerk: per agent
{
    let pieces = file(t.text) in clerk with model: "x"
    for q in pieces where q.kind == Question and not q.via_tool { ask(t.agent, q.text) }
    par for q in pieces { raise reminder(agent: t.agent, text: "{{ q.text }}!") }
    t.text := spellcheck(t.text) fresh
    match decide end-of-turn(agent: t.agent, nudged: nudged_in_row(t.agent)) {
        accept => accept(wake: none)
        nudge => nudge(t.agent, "go on")
        _ => { tell(owner, "{{ t.agent }} stopped"); accept(wake: answer("q1")) }
    }
    match p.answers { q? => deliver(q.from, "x", q.thread)  none if p.is_answer => fault unthreaded(p)  none => reject(t.agent, "?") }
}
"#;
        let f = parse("t.net", src).unwrap();
        assert_eq!(f.events[0].fields.len(), 2);
        let n = &f.nets[0];
        assert_eq!((n.name.as_str(), n.event.as_str(), n.sessions[0].per), ("t", "turn_end", Per::Agent));
        assert_eq!(n.body.len(), 6);
        assert!(matches!(&n.body[0], Stmt::Let { value: Expr::Call { place: Place::In(s), with, .. }, .. } if s == "clerk" && with.len() == 1));
        assert!(matches!(&n.body[2], Stmt::For { par: true, .. }));
        assert!(matches!(&n.body[3], Stmt::Set { add: false, value: Expr::Call { place: Place::Fresh, .. }, .. }));
        let Stmt::Match { arms, .. } = &n.body[5] else { panic!() };
        assert_eq!(arms.iter().map(|a| a.pat.clone()).collect::<Vec<_>>(), vec![Pat::Some("q".into()), Pat::None, Pat::None]);
        assert!(arms[1].guard.is_some());
    }

    #[test]
    fn bindings_parse_and_errors_name_the_line() {
        let b = bindings("b", "on turn_end use turn_end\non turn_end where agent in {orch, CONE} use strict // agents\n").unwrap();
        assert_eq!(b[1].agents, vec!["orch", "CONE"]);
        assert_eq!(b[1].at.line, 2);
        let e = parse("x.net", "net a on turn_end(t) {\n  let = 3\n}").unwrap_err();
        assert!(e.starts_with("x.net:2:"), "{e}");
    }
}
