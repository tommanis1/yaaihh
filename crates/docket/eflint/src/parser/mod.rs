//! The parser: eFLINT's grammar, a text's parse by it, and its syntax errors.
//!
//! eFLINT's concrete syntax is ambiguous (a full stop ends a phrase and also projects a field, a
//! clause keyword can start the next declaration, every binary operator is its own alternative).
//! The language is defined by which reading is taken:
//!
//! * a rule takes its first alternative that covers the text;
//! * in a sequence, the last element takes the longest text it can, then the one before it, and
//!   so on, except where an element is marked `>>>`, which gives the longest text to what
//!   precedes it.
//!
//! So the parser works on spans. A recogniser computes, per grammar symbol and start position,
//! every end position; the reading is then built top-down from those sets by the two rules above.

use crate::ast::*;
use crate::lexer::{lex_at, Tok};
use std::collections::HashMap;
use std::rc::Rc;

mod decls;
mod recognise;
mod terms;

const GRAMMAR: &str = r#####"
var         := ID decoration
decoration  := INT? quote*
quote       := '''
atom        := STR | ALT
arguments   := '(' mods ')' | '(' exprs1 ')'
mods        := ε | modifier cmod*
cmod        := ',' modifier
modifier    := var '=' EXPR
exprs1      := EXPR cexpr*
exprs0      := ε | EXPR cexpr*
cexpr       := ',' EXPR
vars1       := var cvar*
cvar        := ',' var
foreach     := '(' "Foreach" vars1 ':' EXPR ')'
quantified  := '(' "Exists" vars1 ':' EXPR ')' | '(' "Forall" vars1 ':' EXPR ')'
parens      := '(' EXPR ')'
pvar        := var | '(' var ')'
optforeach  := EXPR | foreach

type-expr   := var | atom sep atoms1 | INT sep ints1 | var '*' vars-times1 | '(' var '*' vars-times1 ')' | INT | INT ".." INT | atom | CHR ".." CHR | "String" | "Atom" | "Int" | "Time"
sep         := '+' | ','
atoms1      := atom sep-atom*
sep-atom    := sep atom
ints1       := INT sep-int*
sep-int     := sep INT
vars-times1 := var times-var*
times-var   := '*' var

frame       := fact-decl | bool-decl | pred-decl | inv-decl | duty-decl | act-decl | physical-decl | event-decl | ext | placeholder
closed      := "Closed" | "Open" | ε
restr       := "Fact" | "Var" | "Function"
identby     := "Identified_by" type-expr | ε
domcon      := when EXPR | ε
when        := "Where" | "When"
fact-decl   := closed restr ID identby domcon >>> fact-clause*
bool-decl   := closed "Bool" ID fact-clause*
pred-decl   := "Predicate" ID when EXPR
inv-decl    := "Invariant" ID when EXPR
duty-decl   := closed "Duty" ID "With"? "Holder" var "Claimant" var objects domcon duty-clause*
act-decl    := closed "Act" ID "With"? actor? recipient? objects domcon event-clause*
actor       := "Actor" var
recipient   := "Recipient" var
physical-decl := "Physical" ID "With"? actor? objects domcon event-clause*
event-decl  := closed "Event" ID "With"? objects domcon event-clause*
objects     := "Related_to" vars1 | ε
ext         := "Extend" ext-body
ext-body    := "Fact" ID fact-clause* | "Act" ID event-clause* | "Duty" ID duty-clause* | "Event" ID event-clause*
placeholder := "Placeholder" ID "For" ID
fact-clause := precond | derivation | open-close | domain-ext
event-clause := precond | derivation | open-close | creates | terminates | obfuscates | syncs
duty-clause := precond | derivation | open-close | violation | enforced | terminated-by | created-by
precond     := "Conditioned_by" exprs0
derivation  := "Derived_from" derivs1 | present-when exprs1
derivs1     := optforeach cderiv*
cderiv      := ',' optforeach
present-when := "Present" "When" | "Holds" "When" | "Present_when" | "Holds_when"
open-close  := "Closed" | "Open"
domain-ext  := idby-dom domain-ext-body
idby-dom    := "Identified_by" | "Domain"
domain-ext-body := atoms1 | ints1 | INT ".." INT | CHR ".." CHR
creates     := "Creates" derivs1
terminates  := "Terminates" derivs1
obfuscates  := "Obfuscates" derivs1
syncs       := "Syncs_with" derivs1
violation   := "Violated_when" exprs1
enforced    := "Enforced_by" ids1
terminated-by := "Terminated_by" ids1
created-by  := "Created_by" ids1
ids1        := ID cid*
cid         := ',' ID

phrase      := '!'? optforeach | '+' optforeach | '-' optforeach | '~' optforeach | '?' EXPR | "!?" EXPR | "?-" optforeach | "?--" optforeach | frame+
phrase-or-comp := '{' block '}' | phrase '.'
block       := phrases-dot '.'? | ε
phrases-dot := phrase dot-phrase*
dot-phrase  := '.' phrase
top         := entries phrase?
entries     := entry+ | ε
entry       := directive '.' | phrase-or-comp
directive   := "#include" STR | "#require" STR

flint       := "#"? frame+ refiner-part initial-part scenario-part
refiner-part := "##" refine* | ε
refine      := "Fact" ID "Identified_by" type-expr
initial-part := "###" initial* | ε
initial     := foreach '.' | EXPR '.'
scenario-part := "####" statement* | ε
statement   := actioner action '.' | '?' EXPR '.' | "!?" EXPR '.'
actioner    := '!'? | '+' | '-'
action      := ID arguments | '(' "Foreach" vars1 ':' ID arguments ')'
"#####;

#[derive(Clone, Debug)]
enum Sym {
    Ch(char),
    Kw(String),
    Id,
    AltId,
    Int,
    Str,
    Chr,
    Expr,
    Nt(usize),
}

struct Alt {
    syms: Vec<Sym>,
    /// `longest_left[j]`: what precedes symbol `j` takes the longest text.
    longest_left: Vec<bool>,
}

struct Rule {
    name: String,
    alts: Vec<Alt>,
}

pub struct Grammar {
    rules: Vec<Rule>,
    names: HashMap<String, usize>,
}

impl Grammar {
    fn nt(&mut self, name: &str) -> usize {
        if let Some(i) = self.names.get(name) {
            return *i;
        }
        self.rules.push(Rule { name: name.to_string(), alts: Vec::new() });
        self.names.insert(name.to_string(), self.rules.len() - 1);
        self.rules.len() - 1
    }

    fn base_sym(&mut self, w: &str) -> Sym {
        if w.starts_with('\'') {
            return Sym::Ch(w.chars().nth(1).unwrap());
        }
        if w.starts_with('"') {
            return Sym::Kw(w[1..w.len() - 1].replace('_', " "));
        }
        match w {
            "ID" => Sym::Id,
            "ALT" => Sym::AltId,
            "INT" => Sym::Int,
            "STR" => Sym::Str,
            "CHR" => Sym::Chr,
            "EXPR" => Sym::Expr,
            _ => Sym::Nt(self.nt(w)),
        }
    }

    /// `x?`, `x*` and `x+` name derived rules: `x | ε`, `x x* | ε` and `x x*`.
    fn sym(&mut self, w: &str) -> Sym {
        let quoted = w.starts_with('\'') || w.starts_with('"');
        let body_len = w.chars().count();
        let last = w.chars().last().unwrap();
        if body_len > 1 && "?*+".contains(last) && !(quoted && body_len <= 3 && w.starts_with('\'')) {
            let inner = &w[..w.len() - 1];
            if self.names.contains_key(w) {
                return Sym::Nt(self.names[w]);
            }
            let me = self.nt(w);
            let inner_sym = self.base_sym(inner);
            let alts = match last {
                '?' => vec![vec![inner_sym], vec![]],
                '*' => vec![vec![inner_sym, Sym::Nt(me)], vec![]],
                _ => {
                    let many = self.sym(&format!("{}*", inner));
                    vec![vec![inner_sym, many]]
                }
            };
            self.rules[me].alts =
                alts.into_iter().map(|syms| Alt { longest_left: vec![false; syms.len()], syms }).collect();
            return Sym::Nt(me);
        }
        self.base_sym(w)
    }

    pub fn new() -> Grammar {
        let mut g = Grammar { rules: Vec::new(), names: HashMap::new() };
        for line in GRAMMAR.lines() {
            let Some((name, body)) = line.split_once(":=") else { continue };
            let me = g.nt(name.trim());
            assert!(g.rules[me].alts.is_empty(), "grammar rule defined twice: {}", name.trim());
            let mut alts = Vec::new();
            for alt in body.split(" | ") {
                let mut syms = Vec::new();
                let mut longest_left = Vec::new();
                let mut mark = false;
                for w in alt.split_whitespace() {
                    match w {
                        "ε" => {}
                        ">>>" => mark = true,
                        _ => {
                            syms.push(g.sym(w));
                            longest_left.push(mark);
                            mark = false;
                        }
                    }
                }
                alts.push(Alt { syms, longest_left });
            }
            g.rules[me].alts = alts;
        }
        for r in &g.rules {
            assert!(!r.alts.is_empty(), "grammar rule without a definition: {}", r.name);
        }
        g
    }
}

#[derive(Debug)]
enum Tree {
    Tok(usize),
    Expr(Term),
    Node(usize, usize, Vec<Tree>),
}

type Ends = Rc<Vec<usize>>;

fn has(e: &Ends, r: usize) -> bool {
    e.binary_search(&r).is_ok()
}

struct Parser<'a> {
    g: &'a Grammar,
    toks: &'a [Tok],
    /// The line each token starts on.
    lines: Vec<usize>,
    nt_memo: HashMap<(usize, usize), Ends>,
    pre_memo: HashMap<(usize, usize, usize, usize), Ends>,
    expr_memo: HashMap<usize, Ends>,
    /// The furthest token at which something was looked for and not found, and what was looked for there.
    miss_at: usize,
    missed: std::collections::BTreeSet<String>,
}

fn norm(mut v: Vec<usize>) -> Ends {
    v.sort_unstable();
    v.dedup();
    Rc::new(v)
}

/// The grammar, built once for the process.
static GRAMMAR_TABLE: std::sync::OnceLock<Grammar> = std::sync::OnceLock::new();

fn describe_sym(s: &Sym) -> String {
    match s {
        Sym::Ch(c) => format!("`{}`", c),
        Sym::Kw(k) => format!("`{}`", k),
        Sym::Id => "a name".into(),
        Sym::AltId => "a capitalised name".into(),
        Sym::Int => "a number".into(),
        Sym::Str => "a string".into(),
        Sym::Chr => "a character".into(),
        Sym::Expr => "an expression".into(),
        Sym::Nt(_) => "a construct".into(),
    }
}

/// `LINE:COL: unexpected X; expected: a, b, c`.
fn syntax_error(src: &str, toks: &[Tok], starts: &[usize], p: &Parser) -> String {
    let at = p.miss_at.min(toks.len());
    let offset = starts.get(at).copied().unwrap_or_else(|| src.trim_end().chars().count());
    let (line, col) = crate::lexer::line_col(src, offset);
    let mut expected: Vec<String> = p.missed.iter().cloned().collect();
    expected.sort_by_key(|e| (e.starts_with('`'), e.to_lowercase()));
    format!(
        "{}:{}: unexpected {}; expected: {}",
        line,
        col,
        describe(toks.get(at)),
        if expected.is_empty() { "nothing more".to_string() } else { expected.join(", ") }
    )
}

fn describe(t: Option<&Tok>) -> String {
    match t {
        None => "the end of the text".into(),
        Some(Tok::Ch(c)) => format!("`{}`", c),
        Some(Tok::Kw(k)) => format!("`{}`", k),
        Some(Tok::Int(i)) => format!("`{}`", i),
        Some(Tok::Float) => "a decimal number".into(),
        Some(Tok::Id(s)) | Some(Tok::Alt(s)) => format!("`{}`", s),
        Some(Tok::Chr(c)) => format!("'{}'", c),
        Some(Tok::Str(s)) => show_string(s),
    }
}

fn parse_with<T>(src: &str, start: &str, conv: impl Fn(&Parser, &Tree) -> T) -> Result<T, String> {
    let (toks, starts) = lex_at(src)?;
    let g = GRAMMAR_TABLE.get_or_init(Grammar::new);
    {
        let mut p = Parser {
            g,
            toks: &toks,
            lines: token_lines(src, &starts),
            nt_memo: HashMap::new(),
            pre_memo: HashMap::new(),
            expr_memo: HashMap::new(),
            miss_at: 0,
            missed: Default::default(),
        };
        let n = p.id(start);
        if !has(&p.ends_nt(n, 0), toks.len()) {
            return Err(syntax_error(src, &toks, &starts, &p));
        }
        let tree = p.build_nt(n, 0, toks.len());
        Ok(conv(&p, &tree))
    }
}

/// The line each token starts on, from the offsets (in characters) where they start.
fn token_lines(src: &str, starts: &[usize]) -> Vec<usize> {
    let (mut line, mut at, mut chars) = (1, 0, src.chars());
    starts
        .iter()
        .map(|&s| {
            for c in chars.by_ref().take(s.saturating_sub(at)) {
                line += usize::from(c == '\n');
            }
            at = at.max(s);
            line
        })
        .collect()
}

/// Parse a file or an input line: directives and phrases, the last phrase may lack its full stop.
pub fn parse_statements(src: &str) -> Result<Vec<Statement>, String> {
    parse_with(src, "top", |p, t| p.top(t))
}

/// Parse the four-part file format of eFLINT 1.
pub fn parse_old_file(src: &str) -> Result<OldFile, String> {
    parse_with(src, "flint", |p, t| p.flint(t))
}
