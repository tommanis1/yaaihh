//! The concrete syntax (syntax.tex): kinds with their patterns, types, expressions. Kinds are written in `.kind`
//! files:
//!
//! ```text
//! kind claim
//!   form    what: Text, evidence: Text, where: Ref
//!   result  Fixed(commit: Ref) | Dismissed(why: Text)
//!   raises  NoFix
//!   pattern
//!     verdict <- step role triage triage(what, evidence, where);
//!     match verdict {
//!       Dismiss(why) => return Dismissed(why)
//!       Accept(note) => ...
//!     }
//! ```
//!
//! A header line (`form`, `result`, `raises`, `goal`) may go on over following lines that are indented more than it;
//! `pattern` takes the rest of the kind. Comments run from `//` to the line's end.

use crate::types::Type;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Ident(String),
    Str(String),
    Int(i64),
    Sym(&'static str),
}

const SYMS: &[&str] = &["++", "<-", "->", "=>", "==", "!=", ">=", "<=", ">", "<", ";", ",", "(", ")", "{", "}", "[", "]", "...", ".", ":", "|", "?", "="];

/// Split TEXT into tokens, each with its line (counted from FIRST).
pub fn lex(text: &str, first: usize) -> Result<Vec<(Tok, usize)>, String> {
    Ok(lex_spans(text, first)?.into_iter().map(|s| (s.tok, s.line)).collect())
}

/// A token and where it is written: LINE as `lex` gives it (a string's last line), and its first character's line
/// FROM and column COL (0-based, in characters), LEN characters long. For the language server (N-101 p646).
#[derive(Clone, Debug, PartialEq)]
pub struct Spanned {
    pub tok: Tok,
    pub line: usize,
    pub from: usize,
    pub col: usize,
    pub len: usize,
}

/// `lex`, each token with where it is written.
pub fn lex_spans(text: &str, first: usize) -> Result<Vec<Spanned>, String> {
    let mut out: Vec<Spanned> = Vec::new();
    let cs: Vec<char> = text.chars().collect();
    let (mut i, mut line, mut bol) = (0, first, 0);
    let span = |tok: Tok, line: usize, from: usize, col: usize, len: usize| Spanned { tok, line, from, col, len };
    while i < cs.len() {
        let c = cs[i];
        let (start, from, sbol) = (i, line, bol);
        if c == '\n' {
            line += 1;
            i += 1;
            bol = i;
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '/' && cs.get(i + 1) == Some(&'/') {
            while i < cs.len() && cs[i] != '\n' {
                i += 1;
            }
        } else if c == '"' {
            let mut s = String::new();
            i += 1;
            loop {
                match cs.get(i) {
                    None => return Err(format!("line {line}: a string is not closed")),
                    Some('"') => break,
                    Some('\\') => {
                        match cs.get(i + 1) {
                            Some('n') => s.push('\n'),
                            Some(&x) => s.push(x),
                            None => return Err(format!("line {line}: a string is not closed")),
                        }
                        i += 2;
                    }
                    Some(&x) => {
                        if x == '\n' {
                            line += 1;
                            bol = i + 1;
                        }
                        s.push(x);
                        i += 1;
                    }
                }
            }
            i += 1;
            out.push(span(Tok::Str(s), line, from, start - sbol, i - start));
        } else if c.is_ascii_digit() {
            let start = i;
            while i < cs.len() && cs[i].is_ascii_digit() {
                i += 1;
            }
            let s: String = cs[start..i].iter().collect();
            out.push(span(Tok::Int(s.parse().map_err(|e| format!("line {line}: {s}: {e}"))?), line, from, start - sbol, i - start));
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < cs.len() && (cs[i].is_alphanumeric() || cs[i] == '_' || cs[i] == '-') {
                i += 1;
            }
            out.push(span(Tok::Ident(cs[start..i].iter().collect()), line, from, start - sbol, i - start));
        } else {
            let rest: String = cs[i..cs.len().min(i + 3)].iter().collect();
            match SYMS.iter().find(|s| rest.starts_with(**s)) {
                Some(s) => {
                    i += s.chars().count();
                    out.push(span(Tok::Sym(s), line, from, start - sbol, i - start));
                }
                None => return Err(format!("line {line}: unexpected `{c}`")),
            }
        }
    }
    Ok(out)
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Expr {
    Str(String),
    Int(i64),
    Bool(bool),
    None,
    Var(String),
    Field(Box<Expr>, String),
    Ctx(String),
    /// A constructor; FIELDS are its field names, filled in by the checker from the type expected.
    Con { name: String, args: Vec<Expr>, fields: Vec<String> },
    List(Vec<Expr>),
    Tuple(Vec<Expr>),
    Record(Vec<(String, Expr)>),
    /// `post N`: post #N of this ticket, a Text that refers to its words (resolved when shown), never a copy.
    Post(u32),
    /// `a == b` (EQ) or `a != b`: only in a `where`, a handler's as a binding's (plan/layers.md I §3).
    Cmp { l: Box<Expr>, r: Box<Expr>, eq: bool },
    /// `a < b`, `a <= b`, `a > b`, `a >= b`: two numbers ordered, where `==` and `!=` stand (a call's `where` over
    /// `model`, the model classes: `model >= medium`).
    Rel { l: Box<Expr>, r: Box<Expr>, rel: Rel },
    /// `c and d`: both hold; only in a handler's `where` (N-101: `u.by == a and u.is == "check"`).
    And(Box<Expr>, Box<Expr>),
    /// `c or d`: either holds; in a `where` as `and` is, `and` binding tighter, parentheses grouping (N-101 p392:
    /// `u.by == implementer and (u.is == "check" or u.is == "ask")`).
    Or(Box<Expr>, Box<Expr>),
    /// `result of e [as k]` (N-101): what ticket E closed with. Typed as kind K's result when `as k` is written, else
    /// unknown (a constructor pattern on it is looked up among the declared types). Raises NoResult (untracked) when
    /// the ticket is open, failed or unknown.
    ResultOf { e: Box<Expr>, kind: Option<String> },
    /// `text of e` (N-101): the answer E as one text, its parts under their headings (answers.rs). TY: E's type, filled
    /// in by the checker (the order of the fields of a value no step gave).
    TextOf { e: Box<Expr>, ty: Option<Type> },
    /// `by of e` (N-101): the agent that gave the answer E, an Agent. Raises NoResult (untracked) when no step gave it.
    ByOf(Box<Expr>),
    /// `a ++ b` (N-101 p166): the list A with the elements of list B after it (a decision's `not` built up across
    /// rounds: `seen.not ++ [piece]`).
    Append(Box<Expr>, Box<Expr>),
    /// `find x in e where cond` (N-101 p226, G6): the first element of the list E for which COND holds, X naming
    /// each in turn; none raises NotFound (untracked). A field of each element is `e.f` on the list itself (G9).
    Find { x: String, e: Box<Expr>, cond: Box<Expr> },
    /// `all x in e where cond` (ALL) or `any x in e where cond`, read as `find`'s: whether COND holds for every element
    /// of the list E (true when it has none) or for one of them (false when it has none). A Bool, which raises
    /// nothing, so a `where` can ask it of a list a program is given, where it could not before (gemini-advisor N-1446
    /// p2400, C2: "his where his.all(cut_by_esc)"; proto-mind p2476).
    Quant { all: bool, x: String, e: Box<Expr>, cond: Box<Expr> },
    /// `not c`: C does not hold; in a `where` (N-1511 p43: `not running(e.tree)`).
    Not(Box<Expr>),
    /// `reaches(a, b)`, `tree_of(c)`, `running(t)`: a fact of the record's own lines ([`fact_sig`]), read only in a
    /// step's `on`, from what the machine kept of them (`machine::Seen`), never from git (paper-research N-1446 p2152).
    Fact { name: String, args: Vec<Expr> },
}

/// An order between two numbers (`Expr::Rel`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Rel {
    Lt,
    Le,
    Gt,
    Ge,
}

impl Rel {
    pub fn holds(self, a: i64, b: i64) -> bool {
        match self {
            Rel::Lt => a < b,
            Rel::Le => a <= b,
            Rel::Gt => a > b,
            Rel::Ge => a >= b,
        }
    }

    pub fn sym(self) -> &'static str {
        match self {
            Rel::Lt => "<",
            Rel::Le => "<=",
            Rel::Gt => ">",
            Rel::Ge => ">=",
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Pat {
    Wild,
    Var(String),
    /// A constructor pattern; FIELDS filled in by the checker.
    Con { name: String, args: Vec<Pat>, fields: Vec<String> },
    Tuple(Vec<Pat>),
    /// `[]`: the empty list (design-two-systems §13: "is anyone else here", asked of plain data).
    Nil,
    /// `[p]`: a list of exactly one, its one element matching P (paper-research m1418: one agent on the ticket).
    One(Box<Pat>),
    /// `none`: an optional with no value; a later `_` or variable takes a present one, bound to the value itself
    /// (paper-research m1442: whether the owner's post answers one).
    None,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Worker {
    /// A standing agent by name, or a variable bound to an agent.
    Named(String),
    Owner,
    Role(String),
    Fresh,
    /// Any other expression of type Agent (`step x.who implement(...)`, plan/layers.md I §3): its value names the
    /// agent when the step is reached.
    Expr(Expr),
    /// `step fork A c(...)` (N-101 post 11, the owner N-121): a new agent made for this step alone, as `fresh`, that
    /// starts from a copy of agent A's conversation as it stands when the step is reached; A goes on unchanged.
    Fork(Expr),
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Bound {
    Var(String),
    Tuple(Vec<String>),
}

/// A pattern (syntax.tex §1.3). AT is the line each node starts on.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Prog {
    Return { e: Expr, at: usize },
    Raise { exc: String, e: Option<Expr>, at: usize },
    /// `step w c(args)`, or with HOLD `hold w c(args)`: owed by w the same way, but never delivered, reminded or
    /// expired; w answers it when it chooses (an item's close).
    /// SCOPE: the variables in scope there and their types, filled in by the type check (what a withdrawal's
    /// program is checked in).
    /// INLINE: the call's text written in the program, and its head (`step W "text {{ x }}" -> Head`); CALL is then
    /// `inline` and ARGS empty. The only way a minder's words reach a worker (plan/design-two-systems.md, section 9).
    /// ON: what answers it by itself, a system event's line ([`On`]), or holds its worker's answer until one bears it
    /// out (`until`); none in a program from before them.
    Step {
        w: Worker,
        call: String,
        args: Vec<Expr>,
        at: usize,
        hold: bool,
        scope: BTreeMap<String, Type>,
        inline: Option<(String, Type)>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        on: Vec<On>,
    },
    /// WAIT: where the ticket waits while the opened ticket runs, filled in by the type check (an edit then).
    Open { kind: String, args: Vec<Expr>, at: usize, wait: Wait },
    /// `post e [on t] [as w] [naming a, b...] [informing c, d...]`: a post of E on T (else this ticket), by W (`owner`,
    /// or an agent by name or variable; else this ticket itself), naming each agent of the list after `naming` (an
    /// agent or a list of them), which wakes them, and informing each after `informing`, which reaches them at their
    /// next delivery and starts no turn (`Post.inform`, as policy's `inform` does; an agent's brain, 0.3, brain.kind).
    /// SCOPE: the names in scope and their types, filled in by the type check (how a `{{ name }}` in E is written).
    Post { e: Expr, on: Option<Expr>, by: Option<String>, naming: Vec<Expr>, informing: Vec<Expr>, at: usize, scope: BTreeMap<String, Type> },
    /// `give w e`: the words E given to agent W as they are, not posted (a one-off): nothing lands anywhere from it.
    Give { w: String, e: Expr, at: usize },
    /// `move n to t`: post N of this ticket moved to ticket T (Move, the owner's act, made by the program).
    Move { n: Expr, to: Expr, at: usize },
    /// `...`, the hole: the ticket rests here, open and owing nothing, until an Edit gives the rest of its program, or
    /// its kind's `when hole` handler does (the minder). SCOPE: the variables in scope and their types, filled in by
    /// the type check (what that Edit is checked in). EAGER: a hole in a program the minder wrote, asked at once on
    /// arrival (something has just happened there); a kind's own hole is asked only once something happens after it.
    Hole { at: usize, scope: BTreeMap<String, Type>, eager: bool },
    /// `continue k`: K, the minder's answer at a hole (`Rest`, `Program(text)` or `Close(result)`), carried out: rest
    /// again, run the program in the hole's place (checked first, as an Edit at the hole is), or close with the result.
    Continue { e: Expr, at: usize },
    /// `reply q a`: the ask Q (the `ask` event's value, in its handler) answered with A, in the asker's turn while it
    /// waits, else as a delivery.
    Reply { q: Expr, a: Expr, at: usize },
    /// `tell AGENTS p [joining J]`: the owner's post P (the `owner_post` event's value, in its handler), held till
    /// now, wakes AGENTS; every other agent on its ticket is informed (design-two-systems §13). J (an Int, or none): the
    /// earlier post of his on the ticket that P goes on from, the same thought sent in two goes (the thread's holder,
    /// route.call's `joins`; hs-ls N-1446 p38): it is kept on the post, and those told read the two as one.
    Tell { agents: Expr, p: Expr, joins: Option<Expr>, at: usize },
    /// `gather p`, in a `when owner_post(p)` handler: his burst (the owner, N-1445 p9: "hold them for 15s by
    /// default and ask a docket program to split them"). When his post was held (its `post` line's `hold`, which coordd
    /// writes from the policy's `lapse-after("gather", N)`), the run waits at a step owed to him of the call `gather`
    /// (GATHER), which goes on by itself when the policy's time has passed, or at his press; while it waits his next
    /// posts on the ticket join it, each held with his first and run by no handler of its own. Its value: the event's
    /// value over all of them, in his order (`Machine::owner_value`): their words joined, the agents they name, their
    /// @NAMEs, `posts` their numbers. A post not held: P itself at once, and no line, so a record from before replays as
    /// it ran and a policy with no hold is today's.
    Gather { e: Expr, at: usize },
    /// `drop p`: the owner's post P withdrawn: no agent ever sees it; the owner sees it struck through.
    Drop { p: Expr, at: usize },
    /// `mark p MARKS`: an agent's post P (the `agent_post` event's value, in its handler) asks the owner what MARKS
    /// say, passages of its words found by coordd (`asks::Mark`); stored on the post, its text unchanged (§16).
    Mark { p: Expr, marks: Expr, at: usize },
    /// `deliver a ITEMS [listing REST] [digest PULL]` (0.2, runtime-design §2): what waits for agent A, by items of a
    /// delivery event's value or their ids, handed to A as one message: one head, ITEMS one after another, one foot
    /// naming REST as still waiting. Working, it goes in at A's next tool call; idle, it starts a turn. Only in a
    /// handler of A's delivery ticket (`delivery.kind`). PULL (I-1480, rule 2): what was pulled for A, each given with
    /// the message by its first line alone, a line a thread at its foot (coordd's rest.tmpl: "New on the threads and
    /// items you are on, your own first, the newest first in each"), so each is listed once; with no ITEMS nothing is
    /// handed, and PULL waits on.
    ///
    /// `fork a at PLACE with ITEMS [listing REST] [digest PULL]` is the same message, handed to A going on from PLACE, a
    /// delivery given to it (`free`'s `place`, its last work's): coordd restarts A under its own name on a copy of its
    /// session cut after the line PLACE was given on, and hands the message there, with what its last turn ended with
    /// at its head (paper-research N-1446 p2092, p2097; docket-core p2096, 4). FROM holds PLACE; a `deliver` has none.
    /// Where A cannot be forked (not Claude Code, PLACE not found), the message is handed as a `deliver`'s, saying so.
    Deliver { a: Expr, items: Expr, rest: Option<Expr>, digest: Option<Expr>, from: Option<Expr>, at: usize },
    /// `fork a at PLACE`, with no `with`: a copy of agent A beside it, its value the copy's name (fresh-lead-3 N-1104
    /// p1154, "a copy answers you, only while the agent is busy"; paper-research N-1446 p3020, 1). The copy starts
    /// from A's session cut after the line PLACE was given on (the turn A is in began there: `arrived`'s `began`), as
    /// `fork … at PLACE with` cuts it, and A goes on untouched; the copy lives across runs of A's delivery, which may
    /// `deliver` to it, until a `stop` of it ends it. Only in A's delivery (`delivery.kind`), so A is the run's agent.
    Fork { a: Expr, place: Expr, at: usize },
    /// `keep ITEMS WHY`: the items wait; WHY is shown under the sender's message and is on the record, never a post.
    /// A kept item stays waiting, below the agent's read mark too, until a `deliver` carries it.
    Keep { items: Expr, why: Expr, at: usize },
    /// `report ITEMS WHY`: the items cannot be delivered (the agent is stopped, out of limit); WHY is shown under the
    /// message, never a post.
    Report { items: Expr, why: Expr, at: usize },
    Bind { x: Bound, first: Box<Prog>, rest: Box<Prog> },
    /// GUARDS: each arm's `where` condition, or none (one per arm, N-101.p42): the arm is taken when its pattern
    /// matches and the condition, with the pattern's names bound, holds.
    Match { e: Expr, arms: Vec<(Pat, Prog)>, guards: Vec<Option<Expr>>, at: usize },
    /// TYS: each branch's type, filled in by the type check (an edit of a branch is checked against it).
    /// EACH: the branches made from a list (`par x in e up to n { p }`): BRANCHES is the one branch written, TYS its
    /// type, and it runs once per element.
    /// WAIT: where the ticket waits while its branches run, filled in by the type check (an edit then).
    Par { branches: Vec<Prog>, tys: Vec<Type>, at: usize, each: Option<Each>, wait: Wait },
    /// `any n of (p, ...)`: the branches at once; the first N results, in the order they came, the rest cancelled.
    Any { n: u32, branches: Vec<Prog>, tys: Vec<Type>, at: usize, each: Option<Each>, wait: Wait },
    /// N: the most rounds, or UNBOUNDED (no `up to`): as many as it takes, each round waiting on a step.
    Iterate { x: String, init: Expr, n: u32, body: Box<Prog>, at: usize },
    /// `for x in e up to n { body }`: BODY once per element of the list E, one after another in its order, X naming
    /// it; the list of the rounds' results (plan/layers.md, the owner 14 Nov). Longer than N raises Exhausted first.
    For { x: String, e: Expr, n: u32, body: Box<Prog>, at: usize },
    /// `layer [on e] { body }`: BODY's steps share one workspace, a view of commit or branch E (else the root's head)
    /// with a layer of its own on top (plan/layers.md I §2). Its value is its body's.
    Layer { on: Option<Expr>, body: Box<Prog>, at: usize },
    /// `try { body } handle { E(x) => p ... }`: an exception named by an arm runs that arm, X bound to its value;
    /// any other passes through.
    Try { body: Box<Prog>, arms: Vec<Handler>, at: usize },
    /// `with f = e in { body }`: context field F is E for BODY and every ticket opened inside it.
    With { f: String, e: Expr, body: Box<Prog>, at: usize },
    /// `let agent x in { body }`: an agent that lives for BODY, X naming it there. With FROM, `let agent x = fork a in
    /// { body }` (N-101 post 12): the agent starts from a copy of agent A's conversation as it stands when the block is
    /// entered; A goes on unchanged.
    LetAgent { x: String, from: Option<Expr>, body: Box<Prog>, at: usize },
    /// `during { body } when ev(x) [where cond] do { handler } [when ...]` (N-101): a HANDLER runs on the ticket's
    /// event EV, its value X, while BODY runs, in scope of BODY's names; its value is BODY's. A kind's `when` is the
    /// same on the whole pattern. Each `when` has its own `where`; an event that fits two is given to the first written
    /// (N-101 p722). One look at a time: while a run of a handler is open, further events start none; the last of them
    /// is seen by the next run once it ends. SCOPE: the names in scope there and their types, filled in by the type
    /// check (what a run of a handler is checked in).
    During { body: Box<Prog>, whens: Vec<When>, scope: BTreeMap<String, Type>, at: usize },
    /// `stop e`: agent E's running turn ended, as `coord esc` does, and nothing else; what is owed to it stays owed
    /// (N-101).
    Stop { e: Expr, at: usize },
    /// `decide name(args)` (N-101): the policy's decision NAME, declared `decision name(params) -> Answers` in a call
    /// file (its call `decide name`); a rule answers first, otherwise the owner (a step owed to the owner, its card).
    /// The policy sees the arguments, and for each list argument X `call.X-count`; of the first Ticket argument, how
    /// often it was answered each way before in the tree under the ticket's parent (`call.earlier-C`), the minutes
    /// since the first ticket of this kind there (`call.first-minutes`). An answer's field is filled from the argument of its name; else `why` with the rule's text; else, for type T,
    /// from the single element of the one argument of type [T] but `not`; else from the ticket so picked (its result's field,
    /// or while it is open the last value given under that name to one of its steps). An argument named `not`, a list
    /// of tickets (one ticket is a list of one; built up across rounds with `++`), leaves out of every list argument
    /// the elements equal to one of them, for the count and the single element alike (N-101 p162, p166).
    Decide { name: String, args: Vec<Expr>, at: usize },
}

/// Where a ticket waits on tickets below it (an `open`, a `par`, an `any`), as the type check found it: the variables
/// in scope there and their types (`_` left out), and the type of the value it waits for. What an edit given while it
/// waits is checked in (`coord edit`, paper-research N-101 p1631). TY is none until the node is checked.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Wait {
    pub scope: BTreeMap<String, Type>,
    pub ty: Option<Type>,
}

/// `x in e up to n`: the branches of a `par` or an `any` made from the list E, one per element, X naming it in the
/// branch (plan/layers.md I §3). A list longer than N opens nothing and raises Exhausted.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Each {
    pub x: String,
    pub e: Expr,
    pub n: u32,
    /// `as K after A [reach KIND]` (a `par` only; N-101, parts by dependency): each element's name K and the names A
    /// of the elements it stands on; a branch starts once each it stands on has reached the state (a ticket of kind
    /// KIND under its branch closed Done; else its branch closed Done).
    pub order: Option<Order>,
}

/// The order of a `par` over a list (N-101, the owner: "yes this is the right stuructre"): see `Each`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Order {
    pub key: Expr,
    pub after: Expr,
    pub reach: Option<String>,
}

/// An arm of a `try`: exception EXC, its value bound to X if named.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Handler {
    pub exc: String,
    pub x: Option<String>,
    pub body: Prog,
}

impl Prog {
    pub fn at(&self) -> usize {
        match self {
            Prog::Return { at, .. } | Prog::Raise { at, .. } | Prog::Step { at, .. } | Prog::Open { at, .. } | Prog::Post { at, .. } => *at,
            Prog::Give { at, .. } | Prog::Move { at, .. } | Prog::Hole { at, .. } | Prog::Continue { at, .. } | Prog::Reply { at, .. } | Prog::Tell { at, .. } | Prog::Drop { at, .. } | Prog::Mark { at, .. } | Prog::Deliver { at, .. } | Prog::Fork { at, .. } | Prog::Keep { at, .. } | Prog::Report { at, .. } => *at,
            Prog::Match { at, .. } | Prog::Par { at, .. } | Prog::Any { at, .. } | Prog::Iterate { at, .. } | Prog::For { at, .. } | Prog::Layer { at, .. } | Prog::Try { at, .. } | Prog::With { at, .. } | Prog::LetAgent { at, .. } => *at,
            Prog::During { at, .. } | Prog::Stop { at, .. } | Prog::Decide { at, .. } | Prog::Gather { at, .. } => *at,
            Prog::Bind { first, .. } => first.at(),
        }
    }
}

/// A kind as written.
#[derive(Clone, Debug, PartialEq)]
pub struct Kind {
    pub name: String,
    pub file: String,
    pub line: usize,
    /// The first letter of its tickets' ids, when not its name's (`letter K`: a check is K-15, apart from a claim).
    pub letter: Option<char>,
    /// The fewest digits of its ids' numbers (`letter I 3`: I-001, I-1000).
    pub width: usize,
    pub form: Vec<(String, Type)>,
    pub result: Type,
    pub raises: Vec<String>,
    pub goals: Vec<String>,
    pub pattern: Prog,
    /// The kind's whole text, as written: what a ticket keeps (`opened.program`).
    pub text: String,
    /// `kind NAME from BASE`: the kind whose `when` handlers it takes, where it has none of its own for that event.
    pub base: Option<String>,
    /// `when ev(x) [where e] do P`: what runs in a ticket of this kind on a ticket event (design-two-systems §11):
    /// `hole` (its program rests at a hole) and `ask` (an agent working on it asks).
    pub handlers: Vec<When>,
    /// Its doc (the `///` lines above `kind`), and its form fields', exceptions' and results' by name.
    pub doc: Doc,
    /// `drawn line`, `drawn mark` or `drawn note`: how its tickets stand in the thread they stand at
    /// (`Machine::outline_parent`), said by the kind and not by a list of kinds' names (the owner's fundamentals, N-1104
    /// p596 "we are putting lines instead of doing it right"; proto-lead N-1446 p17). None: the kind says nothing, and
    /// `view::drawn` keeps the rule from before the clause (thread and item are lines).
    pub drawn: Option<Drawn>,
}

/// How a kind's tickets stand in the thread they stand at (`drawn`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Drawn {
    /// A line of the outline: a thread of its own, with its one line where it split off (outline.rs), in Threads and
    /// With you; a ticket tied (`coord tie`) moves its line.
    Line,
    /// A mark at the post it stands at: what was decided there (a holder's or a brain's form, a cut), drawn on the
    /// thread at that post and opening to its ticket; never a line of the outline of its own.
    Mark,
    /// A mark drawn only once it has spoken (a brain's pass, brain.kind; consult-3 N-1446 p890, proto-reader and
    /// proto-window p901): it stands at its post as a mark does (`view::stands`, so a binding's open ties it there),
    /// but while it is open and has posted nothing the window draws nothing of it: no program's work at his post
    /// before it has something to say.
    Note,
}

impl Drawn {
    /// Drawn at the post it stands at, not as a line: a mark, or a note.
    pub fn is_mark(self) -> bool {
        matches!(self, Drawn::Mark | Drawn::Note)
    }
}

/// A declaration's doc, written `///` (paper-research N-101 p656): what the thing is, never how to answer and never
/// what the program does next. Part of the definition, changed only by an edit; never on a value, never in the record.
/// TEXT: the declaration's own (its leading `///` lines); PARTS: its parts' by name (a type's fields, a call's
/// parameters and outcomes written inline, a kind's form fields and exceptions), each from the `///` lines above it
/// and the one at the end of its line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Doc {
    pub text: String,
    pub parts: BTreeMap<String, String>,
}

/// The one place a doc is read from the text, so the form the owner settles changes only these. A line holding only
/// a doc.
fn doc_line(l: &str) -> Option<&str> {
    l.trim_start().strip_prefix("///").map(str::trim)
}

/// The doc at the end of a line of code (none on a line holding only a doc).
fn doc_after(l: &str) -> Option<&str> {
    if doc_line(l).is_some() {
        return None;
    }
    l.find("///").map(|i| l[i + 3..].trim())
}

/// The `///` lines directly above line I (from 0) of LINES, joined by line breaks.
pub(crate) fn doc_above(lines: &[&str], i: usize) -> String {
    let mut j = i.min(lines.len());
    while j > 0 && doc_line(lines[j - 1]).is_some() {
        j -= 1;
    }
    lines[j..i.min(lines.len())].iter().filter_map(|l| doc_line(l)).collect::<Vec<_>>().join("\n")
}

/// The names a line of parts lists, at depth 0 and split by SEP: each piece's name before `:` or `(` (`q: Text`,
/// `Exhausted`, `Red(why: Text)`).
fn names_on(code: &str, sep: char) -> Vec<String> {
    let (mut out, mut depth, mut piece) = (Vec::new(), 0i32, String::new());
    let end = |piece: &mut String, out: &mut Vec<String>| {
        let p = std::mem::take(piece);
        let n = p.split([':', '(']).next().unwrap_or("").trim().trim_start_matches("->").trim().to_string();
        if is_ident(&n) {
            out.push(n);
        }
    };
    for c in code.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            c if c == sep && depth == 0 => {
                end(&mut piece, &mut out);
                continue;
            }
            _ => {}
        }
        piece.push(c);
    }
    end(&mut piece, &mut out);
    out
}

/// The docs of the parts on LINES (the first numbered LINE), split by SEP: a part's leading `///` lines and the one
/// at the end of its line. A doc on a line of two parts is refused: it would stand on neither.
fn part_docs(lines: &[&str], line: usize, sep: char, file: &str) -> Result<BTreeMap<String, String>, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    let mut lead: Vec<&str> = Vec::new();
    // the part whose line ended in a doc, and that doc's column: a doc line under it at that column goes on with it
    let mut last: Option<(String, usize)> = None;
    for (k, l) in lines.iter().enumerate() {
        if let Some(d) = doc_line(l) {
            let col = l.find("///");
            match &last {
                Some((n, c)) if Some(*c) == col => {
                    if let Some(doc) = out.get_mut(n) {
                        doc.push('\n');
                        doc.push_str(d);
                    }
                }
                _ => {
                    last = None;
                    lead.push(d);
                }
            }
            continue;
        }
        last = None;
        let names = names_on(uncommented(l), sep);
        let after = doc_after(l);
        if names.is_empty() && after.is_none() {
            continue;
        }
        let mut doc = std::mem::take(&mut lead);
        doc.extend(after);
        match names.as_slice() {
            _ if doc.is_empty() => {}
            [n] => {
                out.insert(n.clone(), doc.join("\n"));
                if after.is_some() {
                    last = l.find("///").map(|c| (n.clone(), c));
                }
            }
            [] => {}
            _ => return Err(format!("{file}:{}: a doc (`///`) stands on one part, and this line has {}: put each on its own line", line + k, names.join(", "))),
        }
    }
    Ok(out)
}

/// A handler: on ticket event EV, its value bound to X, when COND holds, BODY runs in the ticket.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct When {
    pub ev: String,
    pub x: String,
    pub cond: Option<Expr>,
    pub body: Prog,
    pub line: usize,
    /// The kind it was written in (a base's handler is checked again in each kind that takes it).
    pub from: String,
}

/// `on ev(x) [where cond] => answer`, after a step's call (`Prog::Step`'s ON; N-1511 p43, paper-research N-1446 p2139):
/// system event EV ([`system_event`]), its value X, answers the step with ANSWER, in the step's scope, when COND
/// holds. The ticket itself answers, at the line, or at once when the step is given after the last such line: the rule
/// of `in_window` (`machine::IN_WINDOW`) for any step.
///
/// What an `on` answers is its event's alone: done is computed, never set (paper-research N-1446 p2255, 3). An answer
/// by hand, or an agent's or the owner's words read as one, that gives a constructor an `on` of the step gives
/// ([`On::gives`]) is refused; its other constructors are a hand's as before (an item's Dropped). When the line never
/// comes (coordd was down, an act was lost), the way out is the lead's `cancel`, `withdraw` or `repair`, each a line of
/// its own in the record, and for an item, Dropped (`coord item drop`): never that constructor forged.
///
/// `until ev(x) [where cond]`, with no ANSWER: the worker's answer waits until a line of EV bears it out, COND read
/// with the answer's fields by their names too, after the step's scope and X (FIELDS: those it reads, filled in by the
/// type check). An answer whose constructor lacks one of them is not its to hold (`Conflict(with)`, where COND reads
/// `Merged`'s `merge`), and one a line kept already bears out is taken at once (`Machine::until_waits`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct On {
    pub ev: String,
    pub x: String,
    pub cond: Option<Expr>,
    pub answer: Option<Expr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    pub line: usize,
}

impl On {
    /// The constructor its answer is written as (`=> Done(e.by)`), which only its event gives (the rule above):
    /// none for an `until`, or an answer that is not a constructor written out.
    pub fn gives(&self) -> Option<&str> {
        match self.answer.as_ref()? {
            Expr::Con { name, .. } => Some(name),
            _ => None,
        }
    }

    /// Whether its `where` or answer reads a fact of the record's ([`Expr::Fact`]): read again when a line changes
    /// them.
    pub fn reads_facts(&self) -> bool {
        self.cond.iter().chain(&self.answer).any(Expr::has_fact)
    }
}

impl Expr {
    /// Whether F holds of it or of an expression in it.
    pub fn any(&self, f: &dyn Fn(&Expr) -> bool) -> bool {
        f(self)
            || match self {
                Expr::Field(x, _) | Expr::ResultOf { e: x, .. } | Expr::TextOf { e: x, .. } | Expr::ByOf(x) | Expr::Not(x) => x.any(f),
                Expr::Find { e, cond, .. } | Expr::Quant { e, cond, .. } => e.any(f) || cond.any(f),
                Expr::Con { args: xs, .. } | Expr::List(xs) | Expr::Tuple(xs) | Expr::Fact { args: xs, .. } => xs.iter().any(|x| x.any(f)),
                Expr::Record(fs) => fs.iter().any(|(_, x)| x.any(f)),
                Expr::Cmp { l, r, .. } | Expr::Rel { l, r, .. } | Expr::And(l, r) | Expr::Or(l, r) | Expr::Append(l, r) => l.any(f) || r.any(f),
                _ => false,
            }
    }

    /// Whether a fact of the record's ([`Expr::Fact`]) is in it.
    pub fn has_fact(&self) -> bool {
        self.any(&|e| matches!(e, Expr::Fact { .. }))
    }

    /// Whether it reads the variable NAME (a `find`'s own of that name too).
    pub fn reads_var(&self, name: &str) -> bool {
        self.any(&|e| matches!(e, Expr::Var(v) if v == name))
    }
}

/// The act lines that are system events (N-1511 p43; paper-research N-1446 p2139, 1), by name: an act IS of PHASE.
/// The act line is the event, so none writes a line of its own; its value is the act's fields (`runner::act_value`),
/// typed by [`system_event`]. Only a step's `on` takes one ([`On`]): no handler or binding runs on an act line.
pub const ACT_EVENTS: [(&str, &str, &str); 3] = [("job_ended", "job", "end"), ("cut", "release", "cut"), ("installed", "release", "switch")];

/// The act event ([`ACT_EVENTS`]) an act IS of PHASE is, if any.
pub fn act_event(is: &str, phase: Option<&str>) -> Option<&'static str> {
    ACT_EVENTS.iter().find(|(_, i, p)| *i == is && Some(*p) == phase).map(|(e, _, _)| *e)
}

/// Whether system event EV only answers steps, by their `on` ([`On`]): an act's ([`ACT_EVENTS`]), the git round's
/// `merged` (`machine::MERGED`) or a goal's `duties` (`machine::DUTIES`), whose lines record no handler's run and no
/// ticket a binding opened.
pub fn step_only(ev: &str) -> bool {
    ev == crate::machine::MERGED || ev == crate::machine::DUTIES || ACT_EVENTS.iter().any(|(e, _, _)| *e == ev)
}

/// The record's facts a step's `on` reads ([`Expr::Fact`]; paper-research N-1446 p2152), their parameters and type,
/// each from the lines alone, so a replay gives the same answers: `reaches(a, b)`, commit A is B or has B under it, by
/// the parents the `merged` lines brought; `tree_of(c)`, commit C's tree, from the merged and cut lines and the jobs
/// run on it unchanged, none when no line gave it; `running(t)`, a job on tree T started and has not ended.
pub fn fact_sig(name: &str) -> Option<(Vec<Type>, Type)> {
    match name {
        "reaches" => Some((vec![Type::Ref, Type::Ref], Type::Bool)),
        "tree_of" => Some((vec![Type::Ref], Type::Opt(Box::new(Type::Text)))),
        "running" => Some((vec![Type::Text], Type::Bool)),
        _ => None,
    }
}

/// The ticket events a `when` may be on, and the type of each one's value.
pub fn ticket_event(ev: &str) -> Option<Type> {
    match ev {
        "hole" => Some(Type::Record(Vec::new())),
        "ask" => Some(Type::Record(vec![("by".into(), Type::Agent), ("text".into(), Type::Text), ("context".into(), Type::Opt(Box::new(Type::Text)))])),
        // the owner's words landed on the ticket as a post: the words (by reference), the agents the post names (the
        // one it was typed to and each @NAME), the @NAMEs in the words, the other agents on the ticket, and every agent
        // on it, those the post names first (paper-research m1339)
        "owner_post" => Some(Type::Record(vec![
            ("text".into(), Type::Text),
            ("naming".into(), Type::List(Box::new(Type::Agent))),
            ("names".into(), Type::List(Box::new(Type::Agent))),
            ("others".into(), Type::List(Box::new(Type::Agent))),
            ("agents".into(), Type::List(Box::new(Type::Agent))),
            // the post of the ticket the words answer (the owner's answers to what it asks), none otherwise
            // (paper-research m1437)
            ("answers".into(), Type::Opt(Box::new(Type::Int))),
            // with `@all` in the words, every agent on the ticket, of whom one takes it (N-1134); none otherwise, and
            // in a record from before it
            ("all".into(), Type::Opt(Box::new(Type::List(Box::new(Type::Agent))))),
            // the numbers of his posts it carries, in his order: the one post, or after `gather` his burst (E1b); none
            // in a record from before it
            ("posts".into(), Type::Opt(Box::new(Type::List(Box::new(Type::Int))))),
            // a burst whose posts do not all carry one same choice of his: its posts that carry one (their @NAMEs, the
            // post they answer; `@all` is `claimed`'s), those going to the same agents in one part, as route's Part (E1b
            // hold 1, `Machine::his_choices`); empty otherwise
            ("chosen".into(), Type::List(Box::new(Type::Record(vec![("posts".into(), Type::List(Box::new(Type::Int))), ("to".into(), Type::List(Box::new(Type::Agent)))])))),
            // in such a burst, its posts that say `@all`, out of `chosen`: one part, to every agent on the ticket, that
            // one of them takes, as a lone `@all` (claims.call; proto-lead N-1446 p475); none otherwise
            ("claimed".into(), Type::Opt(Box::new(Type::Record(vec![("posts".into(), Type::List(Box::new(Type::Int))), ("to".into(), Type::List(Box::new(Type::Agent))), ("text".into(), Type::Text)])))),
            // the posts the holder decides, their numbers and words: in such a burst those that carry no choice, none
            // when every post carries one; otherwise every post of it
            ("left".into(), Type::Record(vec![("posts".into(), Type::List(Box::new(Type::Int))), ("text".into(), Type::Text)])),
        ])),
        // an agent's words landed on the ticket as a post (rule Reply's, its `coord post`): the words by reference,
        // and the agent (design-two-systems §16)
        // OWNER_ON: the owner takes part (design part 9b): the post is told to him, or he is on the ticket
        // something an agent did on the ticket (N-101): BY, what it is (IS: "check" for the agent's own check, which
        // reaches coordd as one; "edit", "commit", "run" ... from its tool hooks), and its TEXT (a command, a verdict)
        "act" => Some(Type::Record(vec![("by".into(), Type::Agent), ("is".into(), Type::Text), ("text".into(), Type::Text)])),
        "agent_post" => Some(Type::Record(vec![("text".into(), Type::Text), ("by".into(), Type::Agent), ("owner_on".into(), Type::Bool)])),
        // the delivery events (0.2, runtime-design §3), each run in turn in the agent's delivery ticket: a post for a
        // working agent (ITEM), with all that waits for it; the agent idle with something waiting (its turn ended, or a
        // post came while it was idle); a post for an agent that cannot take it (WHY). DOING: the thread and post its
        // turn began with, as coordd says it. The owner's hold on agents' mail (line 327): WAITING is what waits but for
        // what it keeps; HOLD, the words to keep with (`MailHold::why`), when it keeps ITEM, none otherwise.
        // The copy (fresh-lead-3 N-1104 p1154; paper-research N-1446 p3020, 1), after the fields above, so a program
        // checked before still fits, and empty, false, false and none in a record from before them: BEGAN, the delivery
        // the running turn began with, as its `turn_began` line said (machine.rs TURN_BEGAN; its `given` line's place is
        // where a copy of the session is cut, `fork … at`); FORKABLE, whether a copy can be cut there now while the turn
        // runs (`Core::fork_place`, as for `free`'s); COPY_ANSWERS, for ITEM a post that names the agent and no other (coordd's
        // `written_to_alone`, N-1794), whether the policy says a copy answers it (its act
        // `copy-answers`, actor ITEM's author, recipient the agent, which a project's policy declares; agent-base declares
        // none), these two as coordd read them with the event; COPY, the copy the agent's delivery made
        // in this turn and has not stopped (`Desk::copies`), if any
        ARRIVED => Some(Type::Record(vec![
            ("agent".into(), Type::Agent),
            ("item".into(), waiting_type()),
            ("waiting".into(), Type::List(Box::new(waiting_type()))),
            ("hold".into(), Type::Opt(Box::new(Type::Text))),
            ("began".into(), Type::Text),
            ("forkable".into(), Type::Bool),
            ("copy_answers".into(), Type::Bool),
            ("copy".into(), Type::Opt(Box::new(Type::Agent))),
        ])),
        // OWNER: of what waits, the owner's posts that wake it, in their order (a fact, so a program needs no filter for
        // them); RAISED: of WAITING, the oldest step its own act raised, if one waits; WRITING: the owner is writing to
        // it in his chat now; HELD: what the owner's hold keeps, apart from WAITING and OWNER; HOLD: the words to keep it
        // with, none when nothing is held. PUSH and PULL (I-1480): of WAITING, what goes to it now (`push`, his among
        // it) and what is pulled, for its digest; after the fields above, so a program checked before still fits.
        // WORKED, SINCE and PLACE, for the `fold` call (paper-research N-1446 p2092, p2097; proto-mind N-1511 p17, p28):
        // what goes now came while it worked (an `arrived` was raised for it in the turn now ended), so one call
        // decides it, not the message alone; how far its session ran on since its work was given, as coordd reads its
        // session; and that work's delivery id (`N-1446.p2059`), whose `given` line's `where` is the point a fork goes
        // on from. False, empty and empty in a record from before them, and from a coordd that does not know them.
        // FORKABLE (76): whether a `fork` at PLACE can be made now, as coordd reads it (`Core::fork_place`: a Claude
        // Code agent, PLACE given in the session it runs), so the program says what goes when it cannot, not coordd
        // (gemini-advisor N-1446 p2400, A4; settled on N-1540); after the fields above, so a program checked before still
        // fits, and false in a record from before it. TOLD (76): of WAITING, the posts that ask to go into a turn of the
        // agent (`into_turn`, a program's `post … informing`: its brain's line, brain.kind's Attend) and do not go to it
        // now (not PUSH, so never among what `fold` orders), in WAITING's order: delivery.kind gives them whole with
        // every message, after his posts (fresh-lead-3 N-1540 p18); after the fields above, so a program checked before
        // still fits. The copy (N-1104 p1154), after them too, none, empty and false in a record from before: COPY, the
        // copy its delivery made in the turn now ended and has not stopped; COPIED, of WAITING, that copy's posts, which
        // the program hands whole (N-1104 p1159: they name the owner, so as members' posts they would go by first lines
        // only); ANSWERING, whether the copy is still in a turn or has a post of his it was given still to take, as
        // coordd read it with the event (coordd raises `free` again when that ends)
        FREE => Some(Type::Record(vec![
            ("agent".into(), Type::Agent),
            ("doing".into(), Type::Text),
            ("waiting".into(), Type::List(Box::new(waiting_type()))),
            ("owner".into(), Type::List(Box::new(waiting_type()))),
            ("raised".into(), Type::List(Box::new(waiting_type()))),
            ("writing".into(), Type::Bool),
            ("held".into(), Type::List(Box::new(waiting_type()))),
            ("hold".into(), Type::Opt(Box::new(Type::Text))),
            ("push".into(), Type::List(Box::new(waiting_type()))),
            ("pull".into(), Type::List(Box::new(waiting_type()))),
            ("worked".into(), Type::Bool),
            ("since".into(), Type::Text),
            ("place".into(), Type::Text),
            ("forkable".into(), Type::Bool),
            ("told".into(), Type::List(Box::new(waiting_type()))),
            ("copy".into(), Type::Opt(Box::new(Type::Agent))),
            ("copied".into(), Type::List(Box::new(waiting_type()))),
            ("answering".into(), Type::Bool),
        ])),
        STOPPED => Some(Type::Record(vec![("agent".into(), Type::Agent), ("item".into(), waiting_type()), ("why".into(), Type::Text)])),
        _ => None,
    }
}

/// The delivery events (0.2): raised by coordd for an agent, run one at a time in its delivery ticket.
pub const ARRIVED: &str = "arrived";
pub const FREE: &str = "free";
pub const STOPPED: &str = "stopped";

/// The events coordd sends for an agent's own program (the brain, 0.3; N-1444 p16), each only while a binding is on it
/// (`Machine::bound`), so a record with none gets no line. TURN_END: an agent's turn ended, `{agent, answered, text,
/// every}`, TEXT its last message by place, EVERY the policy's `check-in-every` for it when it has one. OWNER_TO: the
/// owner's held posts handed to an agent, once per hold, at the hand (`tell`; proto-mind N-1446 p36), or, halted in
/// slow mode, when his card lets them go (proto-lead p155), `{agent, posts, tied_at}`: POSTS their ids in his order,
/// TIED_AT the last of them, where the ticket it opens is tied. COMPACTING: an agent's compaction began, `{agent}`.
pub const TURN_END: &str = "turn_end";
pub const OWNER_TO: &str = "owner_to";
pub const COMPACTING: &str = "compacting";

/// The owner is writing in ticket T's box, or stopped (0.3; proto-lead N-1446 p26), said by his window per thread
/// (`Request::Writing` with a thread): an `event` line of the machine's own each way, as `mail_hold` is, `{"t": T, "on":
/// BOOL}`, so an engine from before reads it as an event nothing takes. Written whether or not a binding is on it: the
/// machine keeps since when (`Machine::writing_since`), which E1b's hold reads.
pub const WRITING: &str = "writing";

/// Whether EV is one of the delivery events.
pub fn delivery_event(ev: &str) -> bool {
    [ARRIVED, FREE, STOPPED].contains(&ev)
}

/// The rest of what the message runtime records, all `event` lines, so that an engine without it (train 26) replays
/// every one as recorded and refuses none: DECIDED, the answer to a delivery run's step (an input, as the delivery
/// events are); DELIVERY_OPENED, an agent's delivery ticket opened on a program, and ACTED, an act done (the outputs).
/// Nothing else a delivery run makes is written (`Machine::exec`): replay makes its run again.
pub const DECIDED: &str = "delivery_decided";
pub const DELIVERY_OPENED: &str = "delivery_opened";
pub const ACTED: &str = "delivery_acted";

/// Whether an `event` line named EV is an output of the message runtime's (DELIVERY_OPENED, ACTED), not an input.
pub fn delivery_output(ev: &str) -> bool {
    [DELIVERY_OPENED, ACTED].contains(&ev)
}

/// One thing waiting for an agent (0.2, runtime-design §1), as the delivery events carry it: ID, `N-412.p7` for a
/// post, else the delivery's (`C-12.s3`, a step; `C-12.c`, a closing notice); the THREAD it is on and its LABEL; FROM
/// whom (`owner`, an agent, `coordd` for a step); OWNER, whether it is the owner's; NAMES, whether it is typed to the
/// agent or @-names it; INFORM, whether it only informs it (starting no turn); LEAD, whether it is a post whose author
/// is above the agent in the team's tree (the event's `above`); ANSWERS, the post it answers; TEXT, its words by
/// reference; WHAT it is (`post`, `step`, `closed`, `withdrawn`, `refused`, `words`, `taken_back`); KEPT, why it was
/// kept, when a `keep` holds it; WAITS, an owner's post he sent to wait for the agent's turn to end (`coord say
/// --wait`); RAISED, a step of a run the agent's own act raised (item.kind's `seek` at its `coord item start`), FROM
/// being the agent; ABOUT_TURN, its ticket stands at a post of a thread the agent's running turn was handed something on
/// (`Machine::outline_parent`, `given_since`): about what the turn holds (an agent's brain's line, 0.3, brain.kind;
/// consult-3 N-1444 p20.3); INTO_TURN, its post asks to go into that turn of this agent (a program's `post … informing`
/// naming it, `Post.into_turn`; policy's inform never: proto-reader N-1446 p112, p177). Both absent in a record from
/// before them. PUSH and WHY (I-1480), OWN, and MEMBER: it reaches the agent only as one on its thread.
pub fn waiting_type() -> Type {
    Type::Record(vec![
        ("id".into(), Type::Text),
        ("thread".into(), Type::Ticket),
        ("label".into(), Type::Text),
        ("from".into(), Type::Agent),
        ("owner".into(), Type::Bool),
        ("names".into(), Type::Bool),
        ("inform".into(), Type::Bool),
        ("lead".into(), Type::Bool),
        ("answers".into(), Type::Opt(Box::new(Type::Text))),
        ("text".into(), Type::Text),
        ("what".into(), Type::Text),
        ("kept".into(), Type::Opt(Box::new(Type::Text))),
        ("waits".into(), Type::Bool),
        ("raised".into(), Type::Bool),
        ("about_turn".into(), Type::Bool),
        ("into_turn".into(), Type::Bool),
        // I-1480: whether it goes to the agent now (at its turn's end, or waking it), and why; or at its next turn's
        // start, in the digest (`push` false). After the fields above, so next.call's `Waiting` still fits (`fits`)
        ("push".into(), Type::Bool),
        ("why".into(), Type::Opt(Box::new(Type::Text))),
        ("own".into(), Type::Bool),
        // whether it reaches the agent only as one on its thread (policy's inform, `Post::only_as_member`): given by
        // its first line, its thread's history pulled (paper-research N-1446 p2092; proto-mind N-1511 p20). Last, so
        // a program checked before still fits
        ("member".into(), Type::Bool),
    ])
}

/// Named types a kind may use (the call files' `type X = ...`).
pub type Named = BTreeMap<String, Type>;

/// What the call of a decision NAME is named: `decide NAME`.
/// An `iterate` written with no `up to`: its rounds have no count (N-101.p33).
pub const UNBOUNDED: u32 = u32::MAX;

pub const DECIDE: &str = "decide ";

/// The call of the step `gather` waits at (`Prog::Gather`): agent-base's gather.call, owed to the owner, with a
/// `default` the policy's `lapse-after` takes. The engine's own word, as the statement is.
pub const GATHER: &str = "gather";

/// The call name of a step whose text is written inline.
pub const INLINE: &str = "inline";

/// The `{{ name }}` and `{{ name.field }}` places in an inline call's text: each name, the path after it, and the span.
pub fn placeholders(text: &str) -> Vec<(String, Vec<String>, std::ops::Range<usize>)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = text[from..].find("{{").map(|i| i + from) {
        let Some(j) = text[i..].find("}}").map(|j| j + i) else { break };
        let mut path = text[i + 2..j].trim().split('.').map(str::to_string);
        let name = path.next().unwrap_or_default();
        out.push((name, path.collect(), i..j + 2));
        from = j + 2;
    }
    out
}

const HEADERS: &[&str] = &["letter", "form", "result", "raises", "goal", "drawn", "pattern", "when"];

/// The kinds of FILE's TEXT.
/// The blocks of a kind file: each starts at the line's start with `kind ` or `on ` and runs to the next.
fn blocks(file: &str, text: &str) -> Result<Vec<(usize, String)>, String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut starts: Vec<usize> = lines.iter().enumerate().filter(|(_, l)| l.starts_with("kind ") || l.starts_with("on ") || l.starts_with("every ") || l.starts_with("event ") || l.starts_with("when ")).map(|(i, _)| i).collect();
    if let Some(i) = lines.iter().position(|l| !l.trim().is_empty() && !l.trim_start().starts_with("//")) {
        if starts.first() != Some(&i) {
            return Err(format!("{file}:{}: a kind file holds kinds, events and bindings, each starting `kind NAME`, `event NAME`, `on EVENT`, `every` or `when` at the line's start", i + 1));
        }
    }
    starts.push(lines.len());
    Ok(starts.windows(2).map(|w| (w[0], lines[w[0]..w[1]].join("\n"))).collect())
}

pub fn parse_kinds(file: &str, text: &str, named: &Named) -> Result<Vec<Kind>, String> {
    let mut out = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    for (i, body) in blocks(file, text)? {
        if body.starts_with("kind ") {
            let mut k = parse_kind(file, &body, i + 1, named)?;
            k.doc.text = doc_above(&lines, i);
            out.push(k);
        }
    }
    Ok(out)
}

/// The kinds of FILE's TEXT that parse, and why each that does not was not: a kind that fails costs only itself, so
/// one form the parser lacks does not hide the rest of the file (N-101, every failing line reported).
pub fn parse_kinds_each(file: &str, text: &str, named: &Named) -> (Vec<Kind>, Vec<String>) {
    let (mut out, mut errors) = (Vec::new(), Vec::new());
    let lines: Vec<&str> = text.lines().collect();
    match blocks(file, text) {
        Ok(bs) => {
            for (i, body) in bs {
                if body.starts_with("kind ") {
                    match parse_kind(file, &body, i + 1, named) {
                        Ok(mut k) => {
                            k.doc.text = doc_above(&lines, i);
                            out.push(k)
                        }
                        Err(e) => errors.push(e),
                    }
                }
            }
        }
        Err(e) => errors.push(e),
    }
    (out, errors)
}

/// The project's events, each with the fields of its value (`event ev { f: T ... }`, plan/layers.md I §3).
pub type Events = BTreeMap<String, Vec<(String, Type)>>;

/// The events FILE's TEXT declares: `event NAME { field: Type, ... }`, the fields on one line or several.
pub fn parse_events(file: &str, text: &str, named: &Named) -> Result<Vec<(String, Vec<(String, Type)>)>, String> {
    let mut out = Vec::new();
    for (i, body) in blocks(file, text)? {
        let Some(rest) = body.strip_prefix("event ") else { continue };
        let code: String = rest.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        let here = |e: String| format!("{file}:{}: {e}", i + 1);
        let mut p = Parser::new(lex(&code, i + 1).map_err(here)?);
        let name = p.ident().map_err(here)?;
        if ticket_event(&name).is_some() || system_event(&name).is_some() || name == "every" || name == "when" {
            return Err(here(format!("`event {name}`: {name} is the system's own event")));
        }
        p.sym("{").map_err(here)?;
        let fields = if p.is_sym("}") { Vec::new() } else { p.fields(named).map_err(here)? };
        p.sym("}").map_err(here)?;
        p.end().map_err(here)?;
        out.push((name, fields));
    }
    Ok(out)
}

/// The events the machine's own lines send programs (N-1446 p1491, 6), with the type of each one's value:
/// `window_release`, the release his window runs and coordd's (`machine::WINDOW_RELEASE`, I-1457), each half its release
/// and the earlier ones it contains, ENGINE none when coordd runs where it was built, SINCE when his window became his
/// (by its own clock, in UTC; none in a line from before p1392, or when it said no time). As a project's event it costs
/// nothing to send, so a handler of it makes no step and opens no ticket (it may post, tell and stop), and its post goes
/// on its ticket by that ticket, as a ticket event's does: nobody sent it. A binding on it opens a ticket.
/// The acts' events ([`ACT_EVENTS`]): the act's agent BY, the ticket it is on, and its fields as build-runner and
/// release.sh write them, numbers read as numbers; `job_ended` a job's end, `cut` a release cut, `installed` a coord
/// switched to a release (COORDD), with its way back. `merged` (`machine::MERGED`): a train branch of REPO now at TIP,
/// its TREE and PARENTS, FROM where it stood (none when the branch is new), and COMMITS, those the move brought, each
/// with its parents and tree, UNLISTED how many more it brought past them. `asked` (`machine::ASKED`): his post POST,
/// by BY, at the first head handed in for it, asking the agents TO, TITLE its first line. `duties`
/// (`machine::DUTIES`): goal GOAL's duties OWED, the lines BY that met one, BROKEN the first owed one violated and
/// the LINE it broke at. The brain's (TURN_END, OWNER_TO, COMPACTING) as their doc above says them, ANSWERED of
/// `turn_end` also true for a turn the rule Reply posted, TEXT none when its last message was empty, EVERY none for
/// an agent the policy gives no `check-in-every` (an `every e.every of` binding counts only what has it).
pub fn system_event(ev: &str) -> Option<Type> {
    let half = || vec![("release".to_string(), Type::Text), ("contains".to_string(), Type::List(Box::new(Type::Text)))];
    let texts = || Type::List(Box::new(Type::Text));
    let refs = || Type::List(Box::new(Type::Ref));
    // an act's value: what every such act has, then what one may lack
    let act = |has: Vec<(&str, Type)>, may: Vec<(&str, Type)>| {
        let mut fs = vec![("by".to_string(), Type::Agent), ("text".to_string(), Type::Text), ("ticket".to_string(), Type::Opt(Box::new(Type::Ticket)))];
        fs.extend(has.into_iter().map(|(f, t)| (f.to_string(), t)));
        fs.extend(may.into_iter().map(|(f, t)| (f.to_string(), Type::Opt(Box::new(t)))));
        Some(Type::Record(fs))
    };
    match ev {
        e if e == crate::machine::WINDOW_RELEASE => Some(Type::Record(
            [half(), vec![("engine".to_string(), Type::Opt(Box::new(Type::Record(half())))), ("since".to_string(), Type::Opt(Box::new(Type::Text)))]].concat(),
        )),
        "job_ended" => act(
            vec![("id", Type::Text), ("rc", Type::Int), ("verb", Type::Text), ("commit", Type::Ref), ("tree", Type::Text), ("branch", Type::Text), ("packages", texts()), ("items", texts())],
            vec![("passed", Type::Int), ("failed", Type::Int), ("machine", Type::Text)],
        ),
        "cut" => act(vec![("release", Type::Text), ("commit", Type::Ref), ("tree", Type::Text)], vec![("folder", Type::Text)]),
        "installed" => act(vec![("coord", Type::Text), ("coordd", Type::Text), ("coordd_commit", Type::Ref), ("way_back", Type::Text)], vec![("from", Type::Text)]),
        // the brain's events as coordd sends them (TURN_END, COMPACTING) and the machine raises them (OWNER_TO), so a
        // binding's `where` or argument on a field they do not have is refused at its check, not read as nothing:
        // `e.agnet` (fresh-lead-3 N-1511 p62, his p978, p982: the engine is changed where the language cannot say it)
        TURN_END => Some(Type::Record(vec![
            ("agent".into(), Type::Agent),
            ("answered".into(), Type::Bool),
            ("text".into(), Type::Opt(Box::new(Type::Text))),
            ("every".into(), Type::Opt(Box::new(Type::Int))),
        ])),
        OWNER_TO => Some(Type::Record(vec![("agent".into(), Type::Agent), ("posts".into(), texts()), ("tied_at".into(), Type::Text)])),
        COMPACTING => Some(Type::Record(vec![("agent".into(), Type::Agent)])),
        e if e == crate::machine::ASKED => Some(Type::Record(vec![("post".into(), Type::Ref), ("by".into(), Type::Agent), ("to".into(), Type::List(Box::new(Type::Agent))), ("title".into(), Type::Text)])),
        e if e == crate::machine::DUTIES => {
            let broken = Type::Record(vec![("duty".into(), Type::Text), ("line".into(), Type::Ref)]);
            // each owed duty's holder, in `owed`'s order, and the line that met each duty met (eflint-rs `Goal::met`;
            // docket-edge6 N-1446 p2421): none on a line from before them
            let holder = || ("holder".to_string(), Type::Opt(Box::new(Type::Text)));
            let holders = Type::List(Box::new(Type::Record(vec![("duty".into(), Type::Text), holder()])));
            let met = Type::List(Box::new(Type::Record(vec![("duty".into(), Type::Text), holder(), ("line".into(), Type::Ref)])));
            let fs = [("goal", Type::Ticket), ("owed", texts()), ("by", refs()), ("broken", Type::Opt(Box::new(broken))), ("holders", Type::Opt(Box::new(holders))), ("met", Type::Opt(Box::new(met)))];
            Some(Type::Record(fs.into_iter().map(|(f, t)| (f.to_string(), t)).collect()))
        }
        e if e == crate::machine::MERGED => {
            let commit = Type::Record(vec![("commit".into(), Type::Ref), ("parents".into(), refs()), ("tree".into(), Type::Text)]);
            let fs = [("repo", Type::Text), ("branch", Type::Text), ("tip", Type::Ref), ("tree", Type::Text), ("parents", refs())];
            let may = [("from", Type::Ref), ("unlisted", Type::Int)];
            let fs = fs.into_iter().chain([("commits", Type::List(Box::new(commit)))]).map(|(f, t)| (f.to_string(), t));
            Some(Type::Record(fs.chain(may.into_iter().map(|(f, t)| (f.to_string(), Type::Opt(Box::new(t))))).collect()))
        }
        _ => None,
    }
}

/// The type of the value of event EV, a ticket's own, the system's ([`system_event`]) or one the project declares.
pub fn event_type(ev: &str, events: &Events) -> Option<Type> {
    ticket_event(ev).or_else(|| system_event(ev)).or_else(|| events.get(ev).map(|fs| Type::Record(fs.clone())))
}

/// A binding (syntax.tex, Bindings): on event EV, its value X, when its `where` holds, a K is opened with no parent
/// and no opener (Trigger). `every N of` opens on every Nth event the binding counts; `every D` on a clock, its
/// event `every` (EV is then `every`, X empty).
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub ev: String,
    pub x: String,
    pub every: Every,
    /// `where`: each must hold, `e`, `e == e` or `e != e` (the bool: whether `==`).
    pub conds: Vec<(Expr, Option<(bool, Expr)>)>,
    pub kind: String,
    pub args: Vec<Expr>,
    pub file: String,
    pub line: usize,
    /// As written, on one line: what the log names it by.
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Every {
    /// `on`: each event.
    Each,
    /// `every N of`: the Nth event that the binding counts (its `where` holds) since it last fired, N as `Times` says.
    Count(Times),
    /// `every D`: on a clock, every SECS (D written `30m`, `1h`, `1d`).
    Clock(i64),
    /// `when D is O`: when the policy's decision DECISION answers OPTION, asked by coordd as it asks `expired`
    /// (plan/layers.md I §3); its event is `when`, naming the binding. Its arguments may read `stands` (`STANDS`).
    When { decision: String, option: String },
}

/// How many events an `every N of` binding counts before it fires, as the binding says where N comes from, so the
/// engine knows no field of any kind's by name (proto-reader N-1446 p46, proto-lead p57): N as written; a field of the
/// counted event's value (coordd puts the policy's number there, so the record holds it); a field of the result of the
/// last ticket the binding fired, when it closed done (`last.F`: the brain's form sets its next pace); or the first of
/// these that gives one (`every last.next_check_in or e.every of turn_end(e)`). A field gives a whole number above 0.
#[derive(Clone, Debug, PartialEq)]
pub enum Times {
    N(u32),
    /// `X.F`: field F of the event's value, X the event's name in the binding.
    Field { x: String, f: String },
    /// `last.F`: field F of the result of the last ticket the binding fired, when it closed done.
    Last { f: String },
    /// `A or B`: A's N when it gives one, else B's.
    Or(Box<Times>, Box<Times>),
}

/// The name `every last.F of` reads the last ticket's result by; an event in such a binding is named otherwise.
pub const LAST: &str = "last";

/// The name a `when D is O` binding's arguments read where the open tickets stand by as it fires (the dev program's lack
/// 5, proto-mind N-1446 p1502): `stands.CALL`, a `[Ticket]`, the open tickets whose program waits at a step or a hold
/// of CALL then (`Machine::standing`'s STANDS), by number; none stands, `[]`. `when cut-due is go open cut(stands.cut_of)`
/// hands the cut the heads that stand at its hold. The machine fills it when the binding fires, so the form it opens is
/// on the record's line.
pub const STANDS: &str = "stands";

impl std::fmt::Display for Times {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Times::N(n) => write!(f, "{n}"),
            Times::Field { x, f: field } => write!(f, "{x}.{field}"),
            Times::Last { f: field } => write!(f, "{LAST}.{field}"),
            Times::Or(a, b) => write!(f, "{a} or {b}"),
        }
    }
}

impl Times {
    /// Its `X.F` parts and `last.F` parts, in order.
    fn parts(&self) -> Vec<&Times> {
        match self {
            Times::Or(a, b) => a.parts().into_iter().chain(b.parts()).collect(),
            t => vec![t],
        }
    }
}

/// The bindings of a kind file, each, or why it was refused.
pub fn parse_bindings(file: &str, text: &str) -> Result<Vec<Result<Binding, String>>, String> {
    Ok(blocks(file, text)?.into_iter().filter(|(_, body)| body.starts_with("on ") || body.starts_with("every ") || body.starts_with("when ")).map(|(i, body)| parse_binding(file, i, &body)).collect())
}

fn parse_binding(file: &str, i: usize, body: &str) -> Result<Binding, String> {
    {
        let code: String = body.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        let here = |e: String| format!("{file}:{}: {e}", i + 1);
        let mut p = Parser::new(lex(&code, i + 1).map_err(here)?);
        let mut every = Every::Each;
        if p.eat_word("when") {
            let decision = p.ident().map_err(here)?;
            p.word("is").map_err(here)?;
            let option = match p.peek().cloned() {
                Some(Tok::Str(s)) => {
                    p.i += 1;
                    s
                }
                _ => p.ident().map_err(here)?,
            };
            every = Every::When { decision, option };
        } else if p.eat_word("every") {
            // N, or where it comes from (`Times`), each of an `or` in turn; a time alone is a clock (`every 30m`)
            let mut times: Vec<Times> = Vec::new();
            loop {
                if let (Some(Tok::Ident(x)), Some(Tok::Sym("."))) = (p.peek(), p.peek_at(1)) {
                    let x = x.clone();
                    p.i += 2;
                    let f = p.ident().map_err(here)?;
                    times.push(if x == LAST { Times::Last { f } } else { Times::Field { x, f } });
                } else {
                    let n = match p.peek() {
                        Some(Tok::Int(n)) if (1..=1_000_000).contains(n) => *n,
                        _ => return Err(here("`every` takes a number from 1 to 1000000: `every 100 of EVENT(x)`, or `every 30m` (m, h or d); or a field that carries it, of the counted event or of the last ticket the binding fired, the first that gives one: `every last.next_check_in or e.every of EVENT(e)`".into())),
                    };
                    p.i += 1;
                    let secs = match p.peek() {
                        Some(Tok::Ident(w)) => [("m", 60), ("h", 3600), ("d", 86_400)].into_iter().find(|(u, _)| u == w).map(|(_, s)| n * s),
                        _ => None,
                    };
                    if let Some(secs) = secs.filter(|_| times.is_empty() && !matches!(p.peek_at(1), Some(Tok::Ident(o)) if o == "or")) {
                        p.i += 1;
                        every = Every::Clock(secs);
                        break;
                    }
                    times.push(Times::N(n as u32));
                }
                if !p.eat_word("or") {
                    break;
                }
            }
            if !matches!(every, Every::Clock(_)) {
                match p.ident().map_err(here)?.as_str() {
                    "of" => {}
                    w => return Err(here(format!("`every {} {w}`: `of` an event, or a time in m, h or d", times.iter().map(Times::to_string).collect::<Vec<_>>().join(" or ")))),
                }
                let last = times.pop().expect("one at least");
                every = Every::Count(times.into_iter().rev().fold(last, |b, a| Times::Or(Box::new(a), Box::new(b))));
            }
        } else {
            p.word("on").map_err(here)?;
        }
        let (ev, x) = if matches!(every, Every::Clock(_)) {
            ("every".to_string(), String::new())
        } else if matches!(every, Every::When { .. }) {
            ("when".to_string(), String::new())
        } else {
            let ev = p.ident().map_err(here)?;
            if ev == "every" || ev == "when" {
                return Err(here(format!("`{ev}` is the system's own event: write `every 1h open ...` or `when DECISION is OPTION open ...`")));
            }
            p.sym("(").map_err(here)?;
            let x = p.ident().map_err(here)?;
            p.sym(")").map_err(here)?;
            if let Every::Count(times) = &every {
                for t in times.parts() {
                    match t {
                        Times::Field { x: y, f } if *y != x => return Err(here(format!("`every {y}.{f} of {ev}({x})`: N is a field of the event counted, `{x}.{f}`, or of the last ticket this binding fired, `{LAST}.{f}`"))),
                        Times::Last { f } if x == LAST => return Err(here(format!("`every {LAST}.{f} of {ev}({LAST})`: `{LAST}` is the last ticket this binding fired; name the event otherwise"))),
                        _ => {}
                    }
                }
            }
            (ev, x)
        };
        let mut conds = Vec::new();
        if p.eat_word("where") {
            if x.is_empty() {
                return Err(here(if matches!(every, Every::When { .. }) { "a decision has no event to put a `where` on: the policy decides".into() } else { "a clock has no event to put a `where` on".into() }));
            }
            loop {
                let l = p.expr().map_err(here)?;
                let r = if p.eat_sym("==") {
                    Some((true, p.expr().map_err(here)?))
                } else if p.eat_sym("!=") {
                    Some((false, p.expr().map_err(here)?))
                } else {
                    None
                };
                conds.push((l, r));
                if !p.eat_word("and") {
                    break;
                }
            }
        }
        p.word("open").map_err(here)?;
        let kind = p.ident().map_err(here)?;
        let args = p.args().map_err(here)?;
        p.end().map_err(here)?;
        let text = code.split_whitespace().collect::<Vec<_>>().join(" ");
        Ok(Binding { ev, x, every, conds, kind, args, file: file.to_string(), line: i + 1, text })
    }
}

/// One kind, its text starting at line FIRST of FILE.
pub fn parse_kind(file: &str, text: &str, first: usize, named: &Named) -> Result<Kind, String> {
    let at = |l: usize| format!("{file}:{l}");
    let lines: Vec<&str> = text.lines().collect();
    let head = lines.first().and_then(|l| l.strip_prefix("kind ")).map(str::trim).unwrap_or("");
    let (name, base) = match head.split_once(" from ") {
        Some((n, b)) => (n.trim(), Some(b.trim())),
        None => (head, None),
    };
    if !is_ident(name) || base.is_some_and(|b| !is_ident(b)) {
        return Err(format!("{}: `kind NAME`, or `kind NAME from BASE`", at(first)));
    }
    let base = base.map(str::to_string);
    let mut sections: Vec<(String, usize, String)> = Vec::new();
    let mut i = 1;
    while i < lines.len() {
        let l = lines[i];
        let t = l.trim_start();
        if t.is_empty() || t.starts_with("//") {
            i += 1;
            continue;
        }
        let word = t.split_whitespace().next().unwrap_or("");
        if !HEADERS.contains(&word) {
            return Err(format!("{}: `{word}` is not a part of a kind ({})", at(first + i), HEADERS.join(", ")));
        }
        let indent = l.len() - t.len();
        let mut body = t[word.len()..].to_string();
        let start = i;
        i += 1;
        while i < lines.len() {
            let n = lines[i];
            let nt = n.trim_start();
            let ni = n.len() - nt.len();
            let nw = nt.split_whitespace().next().unwrap_or("");
            // a pattern runs to the next part at its own depth (a `when` after it); any other part to anything not deeper
            let ends = if word == "pattern" { ni <= indent && HEADERS.contains(&nw) } else { ni <= indent || HEADERS.contains(&nw) };
            if !nt.is_empty() && ends {
                break;
            }
            body.push('\n');
            body.push_str(n);
            i += 1;
        }
        sections.push((word.to_string(), first + start, body));
    }
    let get = |w: &str| sections.iter().find(|(s, _, _)| s == w);
    let form = match get("form") {
        Some((_, l, b)) => fields_of(b, *l, named).map_err(|e| format!("{file}: {e}"))?,
        None => Vec::new(),
    };
    let result = match get("result") {
        Some((_, l, b)) => type_of(b, *l, named).map_err(|e| format!("{file}: {e}"))?,
        None => Type::unit(),
    };
    // `raises`, `goal` and `letter` with their lines' `//` comments cut, as `form` and `result` have them (N-54)
    let bare = |b: &str| b.lines().map(uncommented).collect::<Vec<_>>().join("\n");
    let mut raises = Vec::new();
    if let Some((_, l, b)) = get("raises") {
        for r in bare(b).split([',', '\n']).map(str::trim).filter(|s| !s.is_empty()) {
            if !is_ident(r) {
                return Err(format!("{}: `raises` lists the exceptions' names, separated by commas or line breaks; `{r}` is not a name", at(*l)));
            }
            raises.push(r.to_string());
        }
    }
    // a goal is named by policy's act of that name (`goal green`); what follows the name is kept as written
    let mut goals = Vec::new();
    for (_, l, b) in sections.iter().filter(|(s, _, _)| s == "goal") {
        let b = &bare(b);
        let g = b.trim().split('(').next().unwrap_or("").trim().to_string();
        if !is_ident(&g) {
            return Err(format!("{file}:{l}: a goal is a name, the act in policy that says when it holds (`goal green`), not `{}`", b.trim()));
        }
        goals.push(g);
    }
    let (_, pl, pb) = get("pattern").ok_or(format!("{}: kind {name} has no pattern", at(first)))?;
    let pattern = parse_prog(pb, *pl).map_err(|e| format!("{file}: {e}"))?;
    let (letter, width) = match get("letter") {
        Some((_, l, b)) => {
            let b = bare(b);
            let mut ws = b.split_whitespace();
            let (c, w) = (ws.next().unwrap_or(""), ws.next());
            let mut cs = c.chars();
            let letter = match (cs.next(), cs.next()) {
                (Some(c), None) if c.is_ascii_uppercase() => c,
                _ => return Err(format!("{}: `letter` is one capital letter, the ids' first (`letter K`), and the fewest digits if any (`letter I 3`)", at(*l))),
            };
            let width = match (w.map(str::parse::<usize>), ws.next()) {
                (None, None) => 0,
                (Some(Ok(n)), None) if n <= 9 => n,
                _ => return Err(format!("{}: `letter` is one capital letter, the ids' first (`letter K`), and the fewest digits if any (`letter I 3`)", at(*l))),
            };
            (Some(letter), width)
        }
        None => (None, 0),
    };
    let drawn = match get("drawn") {
        Some((_, l, b)) => match bare(b).split_whitespace().collect::<Vec<_>>()[..] {
            ["line"] => Some(Drawn::Line),
            ["mark"] => Some(Drawn::Mark),
            ["note"] => Some(Drawn::Note),
            _ => return Err(format!("{}: `drawn line` (a line of the outline, as a thread), `drawn mark` (a mark at the post it stands at) or `drawn note` (a mark drawn once it has spoken)", at(*l))),
        },
        None => None,
    };
    let mut handlers = Vec::new();
    for (_, l, b) in sections.iter().filter(|(s, _, _)| s == "when") {
        handlers.push(parse_when(b, *l, name).map_err(|e| format!("{file}: {e}"))?);
    }
    let mut doc = Doc::default();
    for (w, sep) in [("form", ','), ("raises", ','), ("result", '|')] {
        if let Some((_, l, b)) = get(w) {
            doc.parts.extend(part_docs(&b.lines().collect::<Vec<_>>(), *l, sep, file)?);
        }
    }
    Ok(Kind { name: name.to_string(), file: file.to_string(), line: first, letter, width, form, result, raises, goals, pattern, text: text.to_string(), base, handlers, doc, drawn })
}

/// `ev(x) [where e] do P` (what follows `when`), on lines starting at LINE, in kind KIND.
fn parse_when(text: &str, line: usize, kind: &str) -> Result<When, String> {
    let mut p = Parser::new(lex(text, line)?);
    let ev = p.ident()?;
    // a project's event is declared by `event`, checked with the kinds (check_kinds)
    p.sym("(")?;
    let x = p.ident()?;
    p.sym(")")?;
    let cond = if p.eat_word("where") { Some(p.cond()?) } else { None };
    p.word("do")?;
    let body = p.prog()?;
    p.end()?;
    Ok(When { ev, x, cond, body, line, from: kind.to_string() })
}

pub(crate) fn is_ident(s: &str) -> bool {
    !s.is_empty() && s.chars().next().is_some_and(|c| c.is_alphabetic()) && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// `name: Type, name: Type` on lines starting at LINE.
pub fn fields_of(text: &str, line: usize, named: &Named) -> Result<Vec<(String, Type)>, String> {
    let mut p = Parser::new(lex(text, line)?);
    let fs = if p.done() { Vec::new() } else { p.fields(named)? };
    p.end()?;
    Ok(fs)
}

/// A type written on lines starting at LINE.
pub fn type_of(text: &str, line: usize, named: &Named) -> Result<Type, String> {
    let mut p = Parser::new(lex(text, line)?);
    let t = p.ty(named)?;
    p.end()?;
    Ok(t)
}

/// A program the minder wrote, to stand at a hole: its own holes are eager (asked as soon as reached).
pub fn parse_minder_prog(text: &str) -> Result<Prog, String> {
    fn eager(p: &mut Prog) {
        match p {
            Prog::Hole { eager, .. } => *eager = true,
            Prog::Bind { first, rest, .. } => {
                eager(first);
                eager(rest);
            }
            Prog::Match { arms, .. } => arms.iter_mut().for_each(|(_, b)| eager(b)),
            Prog::Par { branches, .. } | Prog::Any { branches, .. } => branches.iter_mut().for_each(eager),
            Prog::Iterate { body, .. } | Prog::For { body, .. } | Prog::Layer { body, .. } | Prog::With { body, .. } | Prog::LetAgent { body, .. } => eager(body),
            Prog::Try { body, arms, .. } => {
                eager(body);
                arms.iter_mut().for_each(|h| eager(&mut h.body));
            }
            _ => {}
        }
    }
    let mut p = parse_prog(text, 1)?;
    eager(&mut p);
    Ok(p)
}

/// A pattern written on lines starting at LINE.
/// The words a pattern starts with.
const STATEMENTS: [&str; 30] = ["decide", "return", "raise", "step", "hold", "open", "post", "give", "move", "continue", "reply", "tell", "drop", "gather", "mark", "deliver", "fork", "keep", "report", "match", "par", "any", "iterate", "for", "layer", "try", "with", "let", "during", "stop"];

pub fn parse_prog(text: &str, line: usize) -> Result<Prog, String> {
    let mut p = Parser::new(lex(text, line)?);
    let prog = p.prog()?;
    p.end()?;
    Ok(prog)
}

pub struct Parser {
    toks: Vec<(Tok, usize)>,
    i: usize,
}

impl Parser {
    pub fn new(toks: Vec<(Tok, usize)>) -> Parser {
        Parser { toks, i: 0 }
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i).map(|(t, _)| t)
    }

    fn peek_at(&self, k: usize) -> Option<&Tok> {
        self.toks.get(self.i + k).map(|(t, _)| t)
    }

    fn line(&self) -> usize {
        self.toks.get(self.i).or(self.toks.last()).map_or(0, |(_, l)| *l)
    }

    fn done(&self) -> bool {
        self.i >= self.toks.len()
    }

    fn err<T>(&self, what: &str) -> Result<T, String> {
        let got = match self.peek() {
            Some(Tok::Ident(s)) => format!("`{s}`"),
            Some(Tok::Str(s)) => format!("\"{s}\""),
            Some(Tok::Int(n)) => n.to_string(),
            Some(Tok::Sym(s)) => format!("`{s}`"),
            None => "the end".into(),
        };
        Err(format!("line {}: expected {what}, found {got}", self.line()))
    }

    fn end(&self) -> Result<(), String> {
        if self.done() {
            Ok(())
        } else if let Some(why) = self.unended() {
            Err(why)
        } else {
            self.err("the end")
        }
    }

    fn is_sym(&self, s: &str) -> bool {
        matches!(self.peek(), Some(Tok::Sym(x)) if *x == s)
    }

    fn is_word(&self, w: &str) -> bool {
        matches!(self.peek(), Some(Tok::Ident(x)) if x == w)
    }

    fn eat_sym(&mut self, s: &str) -> bool {
        if self.is_sym(s) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn sym(&mut self, s: &str) -> Result<(), String> {
        if self.eat_sym(s) { Ok(()) } else { self.err(&format!("`{s}`")) }
    }

    /// A step's `on ev(x) [where cond] => answer` and `until ev(x) [where cond]` clauses ([`On`]), as many as are
    /// written.
    fn ons(&mut self) -> Result<Vec<On>, String> {
        let mut out = Vec::new();
        while self.is_word("on") || self.is_word("until") {
            let (line, until) = (self.line(), self.is_word("until"));
            self.i += 1;
            let ev = self.ident()?;
            self.sym("(")?;
            let x = self.ident()?;
            self.sym(")")?;
            let cond = if self.eat_word("where") { Some(self.cond()?) } else { None };
            let answer = if until {
                None
            } else {
                self.sym("=>")?;
                Some(self.expr()?)
            };
            out.push(On { ev, x, cond, answer, fields: Vec::new(), line });
        }
        Ok(out)
    }

    fn eat_word(&mut self, w: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Ident(x)) if x == w) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn word(&mut self, w: &str) -> Result<(), String> {
        if self.is_word(w) {
            self.i += 1;
            Ok(())
        } else {
            self.err(&format!("`{w}`"))
        }
    }

    fn ident(&mut self) -> Result<String, String> {
        match self.peek() {
            Some(Tok::Ident(s)) => {
                let s = s.clone();
                self.i += 1;
                Ok(s)
            }
            _ => self.err("a name"),
        }
    }

    // ---- types ----

    fn fields(&mut self, named: &Named) -> Result<Vec<(String, Type)>, String> {
        let mut out = Vec::new();
        loop {
            let f = self.ident()?;
            self.sym(":")?;
            let line = self.line();
            let t = self.ty(named)?;
            // a lone constructor with no fields is no field's type: it is a type name nobody declared
            fn lone(t: &Type) -> Option<&str> {
                match t {
                    Type::List(t) | Type::Opt(t) => lone(t),
                    Type::Variant(cs) if cs.len() == 1 && cs[0].1.is_empty() => Some(&cs[0].0),
                    _ => None,
                }
            }
            if let Some(c) = lone(&t) {
                return Err(format!("line {line}: field {f}: no type `{c}`"));
            }
            out.push((f, t));
            if !self.eat_sym(",") {
                return Ok(out);
            }
        }
    }

    pub fn ty(&mut self, named: &Named) -> Result<Type, String> {
        // in a sum each alternative is a constructor, even where a type has its name (`Question | Answer`); a sum
        // written an alternative a line starts with `|`
        let lead = self.eat_sym("|");
        let first = if lead || matches!(self.peek_at(1), Some(Tok::Sym("|"))) { self.ctor(named)? } else { self.ty_atom(named)? };
        if lead && !matches!(first, Type::Variant(_)) {
            return self.err("a constructor after `|`");
        }
        if !self.is_sym("|") {
            return Ok(first);
        }
        let mut cs = match first {
            Type::Variant(cs) => cs,
            _ => return self.err("a constructor before `|`"),
        };
        while self.eat_sym("|") {
            match self.ctor(named)? {
                Type::Variant(more) => cs.extend(more),
                _ => return self.err("a constructor after `|`"),
            }
        }
        Ok(Type::Variant(cs))
    }

    /// A constructor of a sum, `C` or `C(fields)`, whatever types are named C.
    fn ctor(&mut self, named: &Named) -> Result<Type, String> {
        let n = match self.peek() {
            Some(Tok::Ident(n)) if n.chars().next().is_some_and(char::is_uppercase) && !["Text", "String", "Int", "Bool", "Ref", "Agent", "Agents", "Ticket"].contains(&n.as_str()) => n.clone(),
            _ => return self.ty_atom(named),
        };
        self.i += 1;
        let fs = if self.eat_sym("(") {
            let fs = if self.is_sym(")") { Vec::new() } else { self.fields(named)? };
            self.sym(")")?;
            fs
        } else {
            Vec::new()
        };
        Ok(Type::Variant(vec![(n, fs)]))
    }

    fn ty_atom(&mut self, named: &Named) -> Result<Type, String> {
        let t = if self.eat_sym("[") {
            let t = self.ty(named)?;
            self.sym("]")?;
            Type::List(Box::new(t))
        } else if self.eat_sym("(") {
            let mut ts = Vec::new();
            if !self.is_sym(")") {
                loop {
                    ts.push(self.ty(named)?);
                    if !self.eat_sym(",") {
                        break;
                    }
                }
            }
            self.sym(")")?;
            if ts.len() == 1 { ts.pop().unwrap() } else { Type::Tuple(ts) }
        } else if self.eat_sym("{") {
            let fs = if self.is_sym("}") { Vec::new() } else { self.fields(named)? };
            self.sym("}")?;
            Type::Record(fs)
        } else {
            let n = self.ident()?;
            match n.as_str() {
                "Text" | "String" => Type::Text,
                "Int" => Type::Int,
                "Bool" => Type::Bool,
                "Ref" => Type::Ref,
                "Agent" => Type::Agent,
                // the project's agents, the word `agents` (item 11, G1)
                "Agents" => Type::Agents,
                "Ticket" => Type::Ticket,
                // any value: a minder's Close(result), checked against the kind's result when it is accepted
                "Any" => Type::Any,
                _ if named.contains_key(&n) && !self.is_sym("(") => named[&n].clone(),
                _ if n.chars().next().is_some_and(|c| c.is_uppercase()) => {
                    let fs = if self.eat_sym("(") {
                        let fs = if self.is_sym(")") { Vec::new() } else { self.fields(named)? };
                        self.sym(")")?;
                        fs
                    } else {
                        Vec::new()
                    };
                    Type::Variant(vec![(n, fs)])
                }
                _ => return Err(format!("line {}: `{n}` is not a type", self.line())),
            }
        };
        if self.eat_sym("?") { Ok(Type::Opt(Box::new(t))) } else { Ok(t) }
    }

    // ---- patterns ----

    /// Whether a binding `x <-` or `(x, y) <-` starts here.
    fn binding_ahead(&self) -> bool {
        match self.peek() {
            Some(Tok::Ident(_)) => matches!(self.peek_at(1), Some(Tok::Sym("<-"))),
            Some(Tok::Sym("(")) => {
                let mut k = 1;
                loop {
                    match self.peek_at(k) {
                        Some(Tok::Ident(_)) => {}
                        _ => return false,
                    }
                    match self.peek_at(k + 1) {
                        Some(Tok::Sym(",")) => k += 2,
                        Some(Tok::Sym(")")) => return matches!(self.peek_at(k + 2), Some(Tok::Sym("<-"))),
                        _ => return false,
                    }
                }
            }
            _ => false,
        }
    }

    pub fn prog(&mut self) -> Result<Prog, String> {
        if self.binding_ahead() {
            let x = if self.eat_sym("(") {
                let mut xs = Vec::new();
                loop {
                    xs.push(self.ident()?);
                    if !self.eat_sym(",") {
                        break;
                    }
                }
                self.sym(")")?;
                Bound::Tuple(xs)
            } else {
                Bound::Var(self.ident()?)
            };
            self.sym("<-")?;
            let first = self.prog1()?;
            self.sym(";")?;
            let rest = self.prog()?;
            return Ok(Prog::Bind { x, first: Box::new(first), rest: Box::new(rest) });
        }
        // `P; Q` is `_ <- P; Q` (paper-research m1409: the minder writes it). A `;` before an arm (`pat =>`) or a
        // closing bracket is the arm's or the block's end, as before.
        let first = self.prog1()?;
        if self.is_sym(";") && !self.end_ahead() {
            self.i += 1;
            let rest = self.prog()?;
            return Ok(Prog::Bind { x: Bound::Var("_".into()), first: Box::new(first), rest: Box::new(rest) });
        }
        Ok(first)
    }

    /// After the `;` here: the end, a closing `}` or `)`, or an arm (`pat =>`, of a match or a handle).
    fn end_ahead(&mut self) -> bool {
        let save = self.i;
        self.i += 1;
        let end = self.done() || self.is_sym("}") || self.is_sym(")") || (self.pat().is_ok() && self.is_sym("=>"));
        self.i = save;
        end
    }

    /// What to say when a pattern starts here, on a later line than the one before it, with no `;` between them
    /// (paper-research m1409: the reason is what the minder reads before its one retry).
    fn unended(&self) -> Option<String> {
        let (tok, l) = self.toks.get(self.i)?;
        let prev = self.toks.get(self.i.checked_sub(1)?)?.1;
        let word = match tok {
            Tok::Sym("...") => "...".to_string(),
            Tok::Ident(w) if STATEMENTS.contains(&w.as_str()) || self.binding_ahead() => w.clone(),
            _ => return None,
        };
        (*l > prev).then(|| format!("line {l}: `{word}` follows the pattern on line {prev} with no `;` between them: end line {prev} with `;`"))
    }

    /// A pattern that is not a sequence.
    fn prog1(&mut self) -> Result<Prog, String> {
        let at = self.line();
        if self.eat_sym("...") {
            return Ok(Prog::Hole { at, scope: BTreeMap::new(), eager: false });
        }
        let Some(Tok::Ident(w)) = self.peek().cloned() else {
            if self.eat_sym("(") {
                let p = self.prog()?;
                self.sym(")")?;
                return Ok(p);
            }
            return self.err("a pattern (return, raise, step, hold, open, post, match, par, iterate)");
        };
        self.i += 1;
        match w.as_str() {
            "return" => Ok(Prog::Return { e: self.expr()?, at }),
            "raise" => {
                let exc = self.ident()?;
                let e = if self.eat_sym("(") {
                    let e = if self.is_sym(")") { None } else { Some(self.expr()?) };
                    self.sym(")")?;
                    e
                } else {
                    None
                };
                Ok(Prog::Raise { exc, e, at })
            }
            "step" | "hold" => {
                let hold = w == "hold";
                let w = match self.ident()?.as_str() {
                    "owner" => Worker::Owner,
                    "fresh" => Worker::Fresh,
                    "role" => Worker::Role(self.ident()?),
                    "fork" => Worker::Fork(self.expr()?),
                    _ => {
                        // a name or a variable, or any expression of type Agent
                        self.i -= 1;
                        match self.expr()? {
                            Expr::Var(n) => Worker::Named(n),
                            e => Worker::Expr(e),
                        }
                    }
                };
                if let Some(Tok::Str(text)) = self.peek().cloned() {
                    self.i += 1;
                    self.sym("->")?;
                    let head = self.ty(&Named::new())?;
                    let on = self.ons()?;
                    return Ok(Prog::Step { w, call: INLINE.into(), args: Vec::new(), at, hold, scope: BTreeMap::new(), inline: Some((text, head)), on });
                }
                let call = self.ident()?;
                let args = self.args()?;
                let on = self.ons()?;
                Ok(Prog::Step { w, call, args, at, hold, scope: BTreeMap::new(), inline: None, on })
            }
            "open" => {
                let kind = self.ident()?;
                let args = self.args()?;
                Ok(Prog::Open { kind, args, at, wait: Wait::default() })
            }
            "post" => {
                let e = self.expr()?;
                let (mut on, mut by, mut naming, mut informing) = (None, None, Vec::new(), Vec::new());
                loop {
                    if on.is_none() && self.eat_word("on") {
                        on = Some(self.expr()?);
                    } else if by.is_none() && self.eat_word("as") {
                        by = Some(self.ident()?);
                    } else if naming.is_empty() && self.eat_word("naming") {
                        loop {
                            naming.push(self.expr()?);
                            if !self.eat_sym(",") {
                                break;
                            }
                        }
                    } else if informing.is_empty() && self.eat_word("informing") {
                        loop {
                            informing.push(self.expr()?);
                            if !self.eat_sym(",") {
                                break;
                            }
                        }
                    } else {
                        break;
                    }
                }
                Ok(Prog::Post { e, on, by, naming, informing, at, scope: BTreeMap::new() })
            }
            "give" => {
                let w = self.ident()?;
                Ok(Prog::Give { w, e: self.expr()?, at })
            }
            "continue" => Ok(Prog::Continue { e: self.expr()?, at }),
            "reply" => {
                let q = self.expr()?;
                Ok(Prog::Reply { q, a: self.expr()?, at })
            }
            "tell" => {
                let agents = self.expr()?;
                let p = self.expr()?;
                let joins = if self.eat_word("joining") { Some(self.expr()?) } else { None };
                Ok(Prog::Tell { agents, p, joins, at })
            }
            "drop" => Ok(Prog::Drop { p: self.expr()?, at }),
            "gather" => Ok(Prog::Gather { e: self.expr()?, at }),
            "mark" => {
                let p = self.expr()?;
                Ok(Prog::Mark { p, marks: self.expr()?, at })
            }
            "deliver" | "fork" => {
                let a = self.expr()?;
                // `fork a at PLACE with ITEMS`: where its session goes on from; with no `with`, a copy of A beside it
                let from = if w == "fork" {
                    self.word("at")?;
                    let place = self.expr()?;
                    if !self.eat_word("with") {
                        return Ok(Prog::Fork { a, place, at });
                    }
                    Some(place)
                } else {
                    None
                };
                let items = self.expr()?;
                let rest = if self.eat_word("listing") { Some(self.expr()?) } else { None };
                let digest = if self.eat_word("digest") { Some(self.expr()?) } else { None };
                Ok(Prog::Deliver { a, items, rest, digest, from, at })
            }
            "keep" => {
                let items = self.expr()?;
                Ok(Prog::Keep { items, why: self.expr()?, at })
            }
            "report" => {
                let items = self.expr()?;
                Ok(Prog::Report { items, why: self.expr()?, at })
            }
            "move" => {
                let n = self.expr()?;
                self.word("to")?;
                Ok(Prog::Move { n, to: self.expr()?, at })
            }
            "match" => {
                let e = self.expr()?;
                self.sym("{")?;
                let mut arms = Vec::new();
                let mut guards = Vec::new();
                while !self.eat_sym("}") {
                    if let Some(why) = self.unended() {
                        return Err(why);
                    }
                    let p = self.pat()?;
                    // `where C`: as on a handler or a binding (N-101.p42)
                    guards.push(if self.eat_word("where") { Some(self.cond()?) } else { None });
                    self.sym("=>")?;
                    let body = self.prog()?;
                    self.eat_sym(";");
                    self.eat_sym(",");
                    arms.push((p, body));
                }
                Ok(Prog::Match { e, arms, guards, at })
            }
            "par" => {
                let (branches, each) = self.branches()?;
                Ok(Prog::Par { branches, tys: Vec::new(), at, each, wait: Wait::default() })
            }
            "iterate" => {
                let x = self.ident()?;
                self.sym("=")?;
                let init = self.expr()?;
                // `up to n`, or none: as many rounds as it takes, each waiting on a step (UNBOUNDED; N-101.p33)
                let n = if self.is_sym("{") {
                    UNBOUNDED
                } else {
                    self.word("up")?;
                    self.word("to")?;
                    let n = match self.peek() {
                        Some(Tok::Int(n)) if (0..=1000).contains(n) => *n as u32,
                        _ => return self.err("the most rounds, a number up to 1000"),
                    };
                    self.i += 1;
                    n
                };
                self.sym("{")?;
                let body = self.prog()?;
                self.sym("}")?;
                Ok(Prog::Iterate { x, init, n, body: Box::new(body), at })
            }
            "for" => {
                let x = self.ident()?;
                self.word("in")?;
                let e = self.expr()?;
                self.word("up")?;
                self.word("to")?;
                let n = match self.peek() {
                    Some(Tok::Int(n)) if (1..=1000).contains(n) => *n as u32,
                    _ => return self.err("the most rounds, a number from 1 to 1000"),
                };
                self.i += 1;
                self.sym("{")?;
                let body = self.prog()?;
                self.sym("}")?;
                Ok(Prog::For { x, e, n, body: Box::new(body), at })
            }
            "during" => {
                self.sym("{")?;
                let body = self.prog()?;
                self.sym("}")?;
                let mut whens = Vec::new();
                loop {
                    let line = self.line();
                    self.word("when")?;
                    let ev = self.ident()?;
                    self.sym("(")?;
                    let x = self.ident()?;
                    self.sym(")")?;
                    let cond = if self.eat_word("where") { Some(self.cond()?) } else { None };
                    self.word("do")?;
                    self.sym("{")?;
                    let handler = self.prog()?;
                    self.sym("}")?;
                    whens.push(When { ev, x, cond, body: handler, line, from: String::new() });
                    if !self.is_word("when") {
                        break;
                    }
                }
                Ok(Prog::During { body: Box::new(body), whens, scope: BTreeMap::new(), at })
            }
            "decide" => {
                let name = self.ident()?;
                let args = self.args()?;
                Ok(Prog::Decide { name, args, at })
            }
            "stop" => {
                let e = self.expr()?;
                Ok(Prog::Stop { e, at })
            }
            "layer" => {
                let on = if self.eat_word("on") { Some(self.expr()?) } else { None };
                self.sym("{")?;
                let body = self.prog()?;
                self.sym("}")?;
                Ok(Prog::Layer { on, body: Box::new(body), at })
            }
            "try" => {
                self.sym("{")?;
                let body = self.prog()?;
                self.sym("}")?;
                self.word("handle")?;
                self.sym("{")?;
                let mut arms = Vec::new();
                while !self.eat_sym("}") {
                    if let Some(why) = self.unended() {
                        return Err(why);
                    }
                    let exc = self.ident()?;
                    let x = if self.eat_sym("(") {
                        let x = self.ident()?;
                        self.sym(")")?;
                        Some(x)
                    } else {
                        None
                    };
                    self.sym("=>")?;
                    let body = self.prog()?;
                    self.eat_sym(";");
                    self.eat_sym(",");
                    arms.push(Handler { exc, x, body });
                }
                if arms.is_empty() {
                    return Err(format!("line {at}: a try that handles nothing"));
                }
                Ok(Prog::Try { body: Box::new(body), arms, at })
            }
            "with" => {
                let f = self.ident()?;
                self.sym("=")?;
                let e = self.expr()?;
                self.word("in")?;
                self.sym("{")?;
                let body = self.prog()?;
                self.sym("}")?;
                Ok(Prog::With { f, e, body: Box::new(body), at })
            }
            "let" => {
                self.word("agent")?;
                let x = self.ident()?;
                let from = if self.eat_sym("=") {
                    self.word("fork")?;
                    Some(self.expr()?)
                } else {
                    None
                };
                self.word("in")?;
                self.sym("{")?;
                let body = self.prog()?;
                self.sym("}")?;
                Ok(Prog::LetAgent { x, from, body: Box::new(body), at })
            }
            "any" => {
                let n = match self.peek() {
                    Some(Tok::Int(n)) if (1..=1000).contains(n) => *n as u32,
                    _ => return self.err("how many of the branches, a number from 1"),
                };
                self.i += 1;
                self.word("of")?;
                let (branches, each) = self.branches()?;
                if each.as_ref().is_some_and(|e| e.order.is_some()) {
                    return Err(format!("line {at}: an any has no order: `as ... after ...` is a par's"));
                }
                match &each {
                    Some(e) if n > e.n => return Err(format!("line {at}: any {n} of at most {} branches", e.n)),
                    None if n as usize > branches.len() => return Err(format!("line {at}: any {n} of {} branches", branches.len())),
                    _ => {}
                }
                Ok(Prog::Any { n, branches, tys: Vec::new(), at, each, wait: Wait::default() })
            }
            _ => {
                self.i -= 1;
                self.err("a pattern (return, raise, step, open, post, match, par, iterate, try, with)")
            }
        }
    }

    /// The branches of a `par` or an `any`: written out, `(p, ...)`, or made from a list, `x in e up to n { p }`.
    fn branches(&mut self) -> Result<(Vec<Prog>, Option<Each>), String> {
        if !self.is_sym("(") {
            let x = self.ident()?;
            self.word("in")?;
            let e = self.expr()?;
            self.word("up")?;
            self.word("to")?;
            let n = match self.peek() {
                Some(Tok::Int(n)) if (1..=1000).contains(n) => *n as u32,
                _ => return self.err("the most branches, a number from 1 to 1000"),
            };
            self.i += 1;
            let order = if self.eat_word("as") {
                let key = self.expr()?;
                self.word("after")?;
                let after = self.expr()?;
                let reach = if self.eat_word("reach") { Some(self.ident()?) } else { None };
                Some(Order { key, after, reach })
            } else {
                None
            };
            self.sym("{")?;
            let body = self.prog()?;
            self.sym("}")?;
            return Ok((vec![body], Some(Each { x, e, n, order })));
        }
        self.sym("(")?;
        let mut branches = Vec::new();
        loop {
            branches.push(self.prog()?);
            if !self.eat_sym(",") {
                break;
            }
        }
        self.sym(")")?;
        Ok((branches, None))
    }

    fn args(&mut self) -> Result<Vec<Expr>, String> {
        self.sym("(")?;
        let mut out = Vec::new();
        if !self.is_sym(")") {
            loop {
                out.push(self.expr()?);
                if !self.eat_sym(",") {
                    break;
                }
            }
        }
        self.sym(")")?;
        Ok(out)
    }

    fn pat(&mut self) -> Result<Pat, String> {
        if self.eat_sym("[") {
            if self.eat_sym("]") {
                return Ok(Pat::Nil);
            }
            let p = self.pat()?;
            if self.is_sym(",") {
                return self.err("`]`: a list pattern is `[]` or `[x]`, a list of one");
            }
            self.sym("]")?;
            return Ok(Pat::One(Box::new(p)));
        }
        if self.eat_sym("(") {
            let mut ps = Vec::new();
            loop {
                ps.push(self.pat()?);
                if !self.eat_sym(",") {
                    break;
                }
            }
            self.sym(")")?;
            return Ok(if ps.len() == 1 { ps.pop().unwrap() } else { Pat::Tuple(ps) });
        }
        let n = self.ident()?;
        if n == "_" {
            return Ok(Pat::Wild);
        }
        if n == "none" {
            return Ok(Pat::None);
        }
        if n.chars().next().is_some_and(|c| c.is_uppercase()) {
            let mut args = Vec::new();
            if self.eat_sym("(") {
                if !self.is_sym(")") {
                    loop {
                        args.push(self.pat()?);
                        if !self.eat_sym(",") {
                            break;
                        }
                    }
                }
                self.sym(")")?;
            }
            return Ok(Pat::Con { name: n, args, fields: Vec::new() });
        }
        Ok(Pat::Var(n))
    }

    // ---- expressions ----

    /// A handler's `where`: comparisons (`a == b`, `a != b`, `a >= b` and the other orders) or values, each maybe after
    /// `not`, joined by `and` and `or` (`and` binding tighter), grouped by parentheses.
    pub fn cond(&mut self) -> Result<Expr, String> {
        let c = self.cond_and()?;
        Ok(if self.eat_word("or") { Expr::Or(Box::new(c), Box::new(self.cond()?)) } else { c })
    }

    fn cond_and(&mut self) -> Result<Expr, String> {
        let c = self.cond_one()?;
        Ok(if self.eat_word("and") { Expr::And(Box::new(c), Box::new(self.cond_and()?)) } else { c })
    }

    /// One comparison or value of a `where`, or a whole `where` in parentheses (a value in parentheses, a tuple, is
    /// read as before when what is inside is not a `where` closed by `)`).
    fn cond_one(&mut self) -> Result<Expr, String> {
        // `not c`: the word, then what starts a value (a variable named `not` is read as before)
        if self.is_word("not") && matches!(self.toks.get(self.i + 1), Some((Tok::Ident(_) | Tok::Int(_) | Tok::Str(_) | Tok::Sym("("), _))) {
            self.i += 1;
            return Ok(Expr::Not(Box::new(self.cond_one()?)));
        }
        if self.is_sym("(") {
            let at = self.i;
            self.i += 1;
            if let Ok(c) = self.cond() {
                if self.eat_sym(")") && !["==", "!=", ">=", "<=", ">", "<", ".", "++"].iter().any(|s| self.is_sym(s)) {
                    return Ok(c);
                }
            }
            self.i = at;
        }
        let l = self.expr()?;
        let rel = [(">=", Rel::Ge), ("<=", Rel::Le), (">", Rel::Gt), ("<", Rel::Lt)].into_iter().find(|(s, _)| self.is_sym(s)).map(|(_, r)| r);
        let c = if self.eat_sym("==") {
            Expr::Cmp { l: Box::new(l), r: Box::new(self.expr()?), eq: true }
        } else if self.eat_sym("!=") {
            Expr::Cmp { l: Box::new(l), r: Box::new(self.expr()?), eq: false }
        } else if let Some(rel) = rel {
            self.i += 1;
            Expr::Rel { l: Box::new(l), r: Box::new(self.expr()?), rel }
        } else {
            l
        };
        Ok(c)
    }

    pub fn expr(&mut self) -> Result<Expr, String> {
        let mut e = self.postfix()?;
        while self.eat_sym("++") {
            e = Expr::Append(Box::new(e), Box::new(self.postfix()?));
        }
        Ok(e)
    }

    /// An atom and the fields read off it (`a.b.c`).
    fn postfix(&mut self) -> Result<Expr, String> {
        let mut e = self.atom()?;
        while self.is_sym(".") {
            self.i += 1;
            let f = self.ident()?;
            e = match e {
                Expr::Var(v) if v == "ctx" => Expr::Ctx(f),
                e => Expr::Field(Box::new(e), f),
            };
        }
        Ok(e)
    }

    fn atom(&mut self) -> Result<Expr, String> {
        match self.peek().cloned() {
            Some(Tok::Str(s)) => {
                self.i += 1;
                Ok(Expr::Str(s))
            }
            Some(Tok::Int(n)) => {
                self.i += 1;
                Ok(Expr::Int(n))
            }
            Some(Tok::Sym("[")) => {
                self.i += 1;
                let mut xs = Vec::new();
                if !self.is_sym("]") {
                    loop {
                        xs.push(self.expr()?);
                        if !self.eat_sym(",") {
                            break;
                        }
                    }
                }
                self.sym("]")?;
                Ok(Expr::List(xs))
            }
            Some(Tok::Sym("(")) => {
                self.i += 1;
                let mut xs = Vec::new();
                if !self.is_sym(")") {
                    loop {
                        xs.push(self.expr()?);
                        if !self.eat_sym(",") {
                            break;
                        }
                    }
                }
                self.sym(")")?;
                Ok(if xs.len() == 1 { xs.pop().unwrap() } else { Expr::Tuple(xs) })
            }
            Some(Tok::Sym("{")) => {
                self.i += 1;
                let mut fs = Vec::new();
                if !self.is_sym("}") {
                    loop {
                        let f = self.ident()?;
                        self.sym("=")?;
                        fs.push((f, self.expr()?));
                        if !self.eat_sym(",") {
                            break;
                        }
                    }
                }
                self.sym("}")?;
                Ok(Expr::Record(fs))
            }
            Some(Tok::Ident(n)) => {
                self.i += 1;
                match n.as_str() {
                    "none" => Ok(Expr::None),
                    "find" | "all" | "any" if matches!(self.peek(), Some(Tok::Ident(_))) && matches!(self.toks.get(self.i + 1), Some((Tok::Ident(w), _)) if w == "in") => {
                        let x = self.ident()?;
                        self.word("in")?;
                        let e = Box::new(self.expr()?);
                        self.word("where")?;
                        let cond = Box::new(self.cond()?);
                        Ok(match n.as_str() {
                            "find" => Expr::Find { x, e, cond },
                            q => Expr::Quant { all: q == "all", x, e, cond },
                        })
                    }
                    "post" if matches!(self.peek(), Some(Tok::Int(_))) => {
                        let Some(Tok::Int(k)) = self.peek().cloned() else { unreachable!() };
                        self.i += 1;
                        u32::try_from(k).ok().filter(|k| *k > 0).map(Expr::Post).ok_or_else(|| format!("line {}: `post N` takes a post number, 1 or more", self.line()))
                    }
                    "result" if matches!(self.peek(), Some(Tok::Ident(w)) if w == "of") => {
                        self.i += 1;
                        let e = self.expr()?;
                        let kind = if self.eat_word("as") { Some(self.ident()?) } else { None };
                        Ok(Expr::ResultOf { e: Box::new(e), kind })
                    }
                    "text" if matches!(self.peek(), Some(Tok::Ident(w)) if w == "of") => {
                        self.i += 1;
                        Ok(Expr::TextOf { e: Box::new(self.expr()?), ty: None })
                    }
                    "by" if matches!(self.peek(), Some(Tok::Ident(w)) if w == "of") => {
                        self.i += 1;
                        Ok(Expr::ByOf(Box::new(self.expr()?)))
                    }
                    "true" => Ok(Expr::Bool(true)),
                    "false" => Ok(Expr::Bool(false)),
                    _ if n.chars().next().is_some_and(|c| c.is_uppercase()) => {
                        let args = if self.is_sym("(") { self.args()? } else { Vec::new() };
                        Ok(Expr::Con { name: n, args, fields: Vec::new() })
                    }
                    _ if fact_sig(&n).is_some() && self.is_sym("(") => Ok(Expr::Fact { args: self.args()?, name: n }),
                    _ => Ok(Expr::Var(n)),
                }
            }
            _ => self.err("an expression"),
        }
    }
}

/// The types coordd gives the nets' calls (the nets' built-ins give and take them: `questions(agent)`, the wakes),
/// read before any call file. Kept here, with the one parser of call files, for coordd and the tests alike.
pub const SYSTEM_TYPES: &str = "type Question {\n  id: String\n  from: String\n  text: String\n  thread: String\n}\ntype Said {\n  id: String\n  from: String\n  to: [String]\n  text: String\n  thread: String?\n  question: Bool\n}\ntype Quoted {\n  source: String\n  text: String\n}\ntype Wake = Nothing | Answer(q: String) | Check(check: String) | Mail(from: String)\n";

/// The words that start a call's attribute lines (after its signature).
pub const CALL_ATTRS: &[&str] = &["task", "template", "model", "blind", "effort", "harness", "tools", "describe", "needs", "asks", "default"];

/// A call as written in a `.call` file (syntax.tex, Calls): its signature, where it is, its attribute lines, and its
/// template. The one parser of call files: the machine reads the signature, coordd (`policy::call`) the template and
/// the attributes.
#[derive(Clone, Debug, PartialEq)]
pub struct CallDef {
    pub sig: crate::check::CallSig,
    pub file: String,
    pub line: usize,
    /// Each attribute line but the template (`task triage`, `model x`, `blind`, `needs ...`), in order: its first word
    /// and the rest of the line, trimmed.
    pub attrs: Vec<(String, String)>,
    /// The lines indented deeper than `template`, their common indent cut and trailing blank lines dropped; none when
    /// the call has no template (a program's call, which coordd answers itself).
    pub template: Option<String>,
    /// The line of the template's first line in the file (1-based; 0 when there is no template).
    pub template_line: usize,
    /// Its doc (the `///` lines above `call`), and its parameters' and its inline outcomes' by name. An outcome named
    /// by a declared type (`-> Design | Ask`) has that type's (`type_docs`).
    pub doc: Doc,
    /// Its signature as written (`call next(doing: Text, waiting: [Waiting]) -> …`), its lines joined, comments and
    /// docs cut: what the Docket shows of it (paper-research N-1396 p223), its types named as the file names them.
    pub written: String,
}

impl CallDef {
    /// The rest of the first attribute line starting with WORD.
    pub fn attr(&self, word: &str) -> Option<&str> {
        self.attrs.iter().find(|(w, _)| w == word).map(|(_, r)| r.as_str())
    }
}

/// A line with its `//` comment cut.
fn uncommented(l: &str) -> &str {
    l.split("//").next().unwrap_or("")
}

/// A type declared in a call file, not yet read: its name, its text (a type, or a record's fields joined), where.
struct Decl {
    name: String,
    text: String,
    record: bool,
    file: String,
    line: usize,
    doc: Doc,
}

/// The type declarations of FILE: `type NAME = TYPE` on one line, `type NAME =` with an alternative a line, each
/// starting with `|`, or `type NAME {` with a field a line and `}`.
fn type_decls(file: &str, text: &str) -> Result<Vec<Decl>, String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some(rest) = lines[i].strip_prefix("type ") else {
            i += 1;
            continue;
        };
        let mut doc = Doc { text: doc_above(&lines, i), parts: BTreeMap::new() };
        let rest = uncommented(rest);
        let line = i + 1;
        if !rest.contains('=') && rest.trim_end().ends_with('{') {
            let name = rest.trim_end().trim_end_matches('{').trim().to_string();
            let mut fields = Vec::new();
            i += 1;
            let from = i;
            while i < lines.len() && lines[i].trim() != "}" {
                let f = uncommented(lines[i]).trim().trim_end_matches(',');
                if !f.is_empty() {
                    fields.push(f.to_string());
                }
                i += 1;
            }
            doc.parts = part_docs(&lines[from..i], from + 1, ',', file)?;
            out.push(Decl { name, text: fields.join(", "), record: true, file: file.to_string(), line: line + 1, doc });
        } else {
            let (name, t) = rest.split_once('=').ok_or(format!("{file}:{line}: `type NAME = TYPE`, `type NAME =` with an alternative a line, or `type NAME {{` with a field a line"))?;
            if let Some(d) = doc_after(lines[i]).filter(|_| doc.text.is_empty()) {
                doc.text = d.to_string();
            }
            let mut t = t.to_string();
            if t.trim().is_empty() {
                // the alternatives, a line each starting with `|`, and indented doc lines among them
                let from = i + 1;
                let mut j = from;
                while j < lines.len() && (uncommented(lines[j]).trim_start().starts_with('|') || (doc_line(lines[j]).is_some() && lines[j].starts_with(char::is_whitespace))) {
                    j += 1;
                }
                if j == from {
                    return Err(format!("{file}:{line}: `type {} =` has no type after it, nor an alternative a line under it starting with `|`", name.trim()));
                }
                // the line breaks kept, so a mistake is told on its line
                t = lines[from..j].iter().map(|l| format!("\n{}", uncommented(l))).collect();
                doc.parts = part_docs(&lines[from..j], from + 1, '|', file)?;
                i = j - 1;
            }
            out.push(Decl { name: name.trim().to_string(), text: t, record: false, file: file.to_string(), line, doc });
        }
        i += 1;
    }
    Ok(out)
}

/// The types FILE's TEXT declares, each with the line of its `type` (for the language server, N-101 p646).
pub fn type_places(file: &str, text: &str) -> Result<Vec<(String, usize)>, String> {
    Ok(type_decls(file, text)?.into_iter().map(|d| (d.name, if d.record { d.line - 1 } else { d.line })).collect())
}

/// The docs of the types declared in FILES (path and text, in order), by name: a later file's type replaces an
/// earlier one's, as `parse_call_files` reads them. A file whose types do not read gives none.
pub fn type_docs(files: &[(String, String)]) -> BTreeMap<String, Doc> {
    files.iter().filter_map(|(f, t)| type_decls(f, t).ok()).flatten().map(|d| (d.name, d.doc)).collect()
}

/// The first `///` line of each kind of the kind FILES (path and text), by name: what a doc written `kind NAME`
/// stands for (paper-research N-101 p906). A kind with no doc gives an empty line. Read from the `kind NAME` lines, so
/// a kind that does not parse still has its line.
pub fn kind_lines(files: &[(String, String)]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (_, text) in files {
        let lines: Vec<&str> = text.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            let Some(rest) = l.strip_prefix("kind ") else { continue };
            let name: String = rest.trim_start().chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-').collect();
            let doc = doc_above(&lines, i);
            out.insert(name, doc.lines().next().unwrap_or("").to_string());
        }
    }
    out
}

/// The kind a doc names, when the doc's whole text is `kind NAME`.
pub fn kind_named(doc: &str) -> Option<&str> {
    let name = doc.trim().strip_prefix("kind ")?.trim();
    (!name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')).then_some(name)
}

/// DOC with each text written `kind NAME` (its own and its parts') replaced by that kind's first doc line in KINDS
/// (`kind_lines`); why each that could not be: no kind NAME, or one with no doc.
pub fn kind_docs(doc: &mut Doc, kinds: &BTreeMap<String, String>) -> Vec<String> {
    let mut why = Vec::new();
    let mut one = |text: &mut String| {
        let Some(name) = kind_named(text).map(str::to_string) else { return };
        match kinds.get(&name) {
            Some(l) if !l.is_empty() => *text = l.clone(),
            Some(_) => why.push(format!("kind {name} has no `///` line")),
            None => why.push(format!("kind {name}: no such kind")),
        }
    };
    one(&mut doc.text);
    doc.parts.values_mut().for_each(&mut one);
    why
}

/// Read DECLS into NAMED in any order: each after the declared types it names, so a type may name one declared
/// later, in its file or another. A later declaration of a name replaces an earlier one. Gives why each that could not
/// be read was not (a cycle, or a type that does not parse), with its file and line; the files of those.
fn define(decls: Vec<Decl>, named: &mut Named) -> (Vec<String>, BTreeSet<String>) {
    let mut by: BTreeMap<String, Decl> = BTreeMap::new();
    for d in decls {
        by.insert(d.name.clone(), d);
    }
    // the declared names each one mentions (a constructor named like a type counts too: it only orders them)
    let deps: BTreeMap<String, Vec<String>> = by
        .iter()
        .map(|(n, d)| {
            let words = lex(&d.text, d.line).map(|ts| ts.into_iter().filter_map(|(t, _)| if let Tok::Ident(w) = t { Some(w) } else { None }).collect::<Vec<_>>()).unwrap_or_default();
            (n.clone(), words.into_iter().filter(|w| w != n && by.contains_key(w)).collect())
        })
        .collect();
    let (mut errors, mut bad) = (Vec::new(), BTreeSet::new());
    let mut state: BTreeMap<String, bool> = BTreeMap::new(); // false: being read; true: done
    fn visit(n: &str, by: &BTreeMap<String, Decl>, deps: &BTreeMap<String, Vec<String>>, state: &mut BTreeMap<String, bool>, named: &mut Named, errors: &mut Vec<String>, bad: &mut BTreeSet<String>) {
        match state.get(n) {
            Some(true) => return,
            Some(false) => {
                let d = &by[n];
                errors.push(format!("{}:{}: type {n} names itself, directly or through others", d.file, d.line));
                bad.insert(d.file.clone());
                return;
            }
            None => {}
        }
        state.insert(n.to_string(), false);
        for m in &deps[n] {
            visit(m, by, deps, state, named, errors, bad);
        }
        let d = &by[n];
        let t = if d.record { fields_of(&d.text, d.line, named).map(Type::Record) } else { type_of(&d.text, d.line, named) };
        match t {
            Ok(t) => {
                named.insert(n.to_string(), t);
            }
            Err(e) => {
                errors.push(format!("{}: {e}", d.file));
                bad.insert(d.file.clone());
            }
        }
        state.insert(n.to_string(), true);
    }
    for n in by.keys() {
        visit(n, &by, &deps, &mut state, named, &mut errors, &mut bad);
    }
    (errors, bad)
}

/// The fields of `model` a call's `where` reads: its class (the number its class names), its family, its name.
pub const MODEL_FIELDS: &[&str] = &["class", "family", "name"];

/// A call's `where` (TEXT, on line LINE) read: comparisons joined by `and`, over `model` (the model of the agent the
/// step goes to), its fields `model.class`, `model.family` and `model.name`, strings, numbers, and bare names, which
/// the project's policy says are classes, models or families (coordd checks them when the calls load). Why not
/// otherwise. `model >= medium` orders the model's number against the class's; `model.class == small` holds for a
/// model whose number is small's; `model.family == claude` compares the family's name.
pub fn model_cond(text: &str, line: usize) -> Result<Expr, String> {
    let mut p = Parser::new(lex(text, line)?);
    let e = p.cond()?;
    p.end()?;
    fn term(e: &Expr) -> Result<(), String> {
        match e {
            Expr::Var(_) | Expr::Str(_) | Expr::Int(_) => Ok(()),
            Expr::Field(m, f) if matches!(&**m, Expr::Var(v) if v == "model") => {
                if MODEL_FIELDS.contains(&f.as_str()) { Ok(()) } else { Err(format!("`model.{f}`: a model has {}", MODEL_FIELDS.iter().map(|f| format!("`model.{f}`")).collect::<Vec<_>>().join(", "))) }
            }
            e => Err(format!("`{}` is no model, class, family, name or number", crate::print::expr(e))),
        }
    }
    fn walk(e: &Expr) -> Result<(), String> {
        match e {
            Expr::And(a, b) => walk(a).and_then(|_| walk(b)),
            Expr::Cmp { l, r, .. } | Expr::Rel { l, r, .. } => term(l).and_then(|_| term(r)),
            e => Err(format!("`{}` is no comparison: write `model >= CLASS`, `model.family == NAME`, joined by `and`", crate::print::expr(e))),
        }
    }
    walk(&e)?;
    Ok(e)
}

/// The types FILE declares, added to NAMED, in any order within the file.
pub fn parse_types(file: &str, text: &str, named: &mut Named) -> Result<(), String> {
    let (errors, _) = define(type_decls(file, text)?, named);
    errors.into_iter().next().map_or(Ok(()), Err)
}

/// The calls of FILE, typed with NAMED (the types of every call file, read first by `parse_types`).
pub fn parse_call_defs(file: &str, text: &str, named: &Named) -> Result<Vec<CallDef>, String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let indent = |l: &str| l.len() - l.trim_start().len();
    while i < lines.len() {
        // a decision is declared as a call is, its name `decide NAME`, with no template (N-101)
        let (rest, decision) = match (lines[i].strip_prefix("call "), lines[i].strip_prefix("decision ")) {
            (Some(r), _) => (r, false),
            (_, Some(r)) => (r, true),
            _ => {
                i += 1;
                continue;
            }
        };
        let first = i + 1;
        let at = |e: String| format!("{file}:{first}: {e}");
        let call_doc = doc_above(&lines, i);
        // the signature: this line and the indented ones after it, up to the first attribute or comment
        let mut sig = uncommented(rest).to_string();
        i += 1;
        while i < lines.len() && indent(lines[i]) > 0 {
            let t = lines[i].trim();
            let word = t.split_whitespace().next().unwrap_or("");
            // a doc line in the signature (a parameter's or an outcome's) does not end it
            if doc_line(t).is_some() {
                i += 1;
                continue;
            }
            if CALL_ATTRS.contains(&word) || t.starts_with("//") {
                break;
            }
            sig.push(' ');
            sig.push_str(uncommented(t).trim());
            i += 1;
        }
        let sig_end = i;
        // the attributes and the template
        let (mut attrs, mut template, mut template_line) = (Vec::new(), None, 0);
        while i < lines.len() && (indent(lines[i]) > 0 || lines[i].trim().is_empty()) {
            let t = lines[i].trim();
            if t.is_empty() || t.starts_with("//") {
                i += 1;
                continue;
            }
            let (word, rest) = t.split_once(char::is_whitespace).map_or((t, ""), |(w, r)| (w, r.trim()));
            if word == "template" {
                let deeper = indent(lines[i]);
                let mut body = Vec::new();
                i += 1;
                template_line = i + 1;
                while i < lines.len() && (lines[i].trim().is_empty() || indent(lines[i]) > deeper) {
                    body.push(lines[i]);
                    i += 1;
                }
                while body.last().is_some_and(|l| l.trim().is_empty()) {
                    body.pop();
                }
                let cut = body.iter().filter(|l| !l.trim().is_empty()).map(|l| indent(l)).min().unwrap_or(0);
                template = Some(body.iter().map(|l| l.get(cut..).unwrap_or("")).collect::<Vec<_>>().join("\n"));
                continue;
            }
            if !CALL_ATTRS.contains(&word) {
                return Err(format!("{file}:{}: a call's lines after its signature start with one of {}", i + 1, CALL_ATTRS.join(", ")));
            }
            attrs.push((word.to_string(), uncommented(rest).trim().to_string()));
            i += 1;
        }
        let open = sig.find('(').ok_or_else(|| at("`call NAME(params) -> TYPE`".into()))?;
        let mut depth = 0;
        let close = sig[open..]
            .char_indices()
            .find(|(_, c)| {
                depth += match c { '(' => 1, ')' => -1, _ => 0 };
                depth == 0
            })
            .map(|(i, _)| open + i)
            .ok_or_else(|| at("`call NAME(params) -> TYPE`: the parameters' `(` is not closed".into()))?;
        let (name, params) = (sig[..open].trim().to_string(), &sig[open + 1..close]);
        let name = if decision { format!("{DECIDE}{name}") } else { name };
        // `where COND` between the parameters and `->`: the model the call needs (paper-research N-101)
        let after = sig[close + 1..].trim();
        let (cond, after) = match after.strip_prefix("where").filter(|r| r.starts_with(char::is_whitespace)) {
            Some(r) => {
                let (c, h) = r.split_once("->").ok_or_else(|| at("a call says its answer's type after `->`".into()))?;
                let c = c.trim().to_string();
                model_cond(&c, first).map_err(|e| at(format!("`where {c}`: {e}")))?;
                (Some(c), format!("->{h}"))
            }
            None => (None, after.to_string()),
        };
        let head = after.strip_prefix("->").ok_or_else(|| at("a call says its answer's type after `->`".into()))?;
        let params = fields_of(params, first, named).map_err(&at)?;
        let head = outcomes_by_type(head, type_of(head, first, named).map_err(&at)?, named);
        let doc = Doc { text: call_doc, parts: sig_docs(&lines[first - 1..sig_end], first, file)? };
        let blind = attrs.iter().any(|(w, _)| w == "blind");
        // `asks C`: the call's question, a constructor of its answer with one Text field (N-101)
        let asks = attrs.iter().find(|(w, _)| w == "asks").map(|(_, c)| c.clone());
        if let Some(c) = &asks {
            match head.constructor(c) {
                Some(fs) if fs.len() == 1 && fs[0].1 == Type::Text => {}
                Some(_) => return Err(at(format!("`asks {c}`: the question has one field, of type Text"))),
                None => return Err(at(format!("`asks {c}`: {c} is none of the call's answers"))),
            }
        }
        // `default C`: the answer an owner's step of the call takes by itself, when the policy gives the call a time
        // (`lapse-after`) and it has passed, or the box that showed it is gone (`Line::Lapsed`, N-487): a
        // constructor of its answer with no fields
        let default = attrs.iter().find(|(w, _)| w == "default").map(|(_, c)| c.clone());
        if let Some(c) = &default {
            match head.constructor(c) {
                Some(fs) if fs.is_empty() => {}
                Some(_) => return Err(at(format!("`default {c}`: the answer taken by itself has no fields"))),
                None => return Err(at(format!("`default {c}`: {c} is none of the call's answers"))),
            }
        }
        let written = format!("{} {}", if decision { "decision" } else { "call" }, sig.trim());
        out.push(CallDef { sig: crate::check::CallSig { name, params, head, blind, asks, cond, default }, file: file.to_string(), line: first, attrs, template, template_line, doc, written });
    }
    Ok(out)
}

/// A call's answer HEAD (its text TEXT): an alternative written as the bare name of a declared record type (`->
/// Design | Ask`, `type Design { ... }`) is that type's constructor, with its fields (paper-research N-101 p656). Only
/// in a call's answer: elsewhere a bare alternative stays a constructor with no fields (`type Kind = Question |
/// Answer`).
fn outcomes_by_type(text: &str, head: Type, named: &Named) -> Type {
    let record = |c: &str| match named.get(c) {
        Some(Type::Record(fs)) => Some(fs.clone()),
        _ => None,
    };
    match head {
        Type::Variant(cs) => Type::Variant(cs.into_iter().map(|(c, fs)| if fs.is_empty() { let fs = record(&c).unwrap_or_default(); (c, fs) } else { (c, fs) }).collect()),
        t => match record(text.trim()) {
            Some(fs) if is_ident(text.trim()) => Type::Variant(vec![(text.trim().to_string(), fs)]),
            _ => t,
        },
    }
}

/// The docs of a call's signature, written on LINES (the first numbered FIRST, starting `call ` or `decision `): its
/// parameters' between the parentheses, and its outcomes' after `->`, each on its own line.
fn sig_docs(lines: &[&str], first: usize, file: &str) -> Result<BTreeMap<String, String>, String> {
    // the line and column of the parameters' `(` and its `)`, comments skipped
    let (mut open, mut close, mut depth) = (None, None, 0);
    'find: for (k, l) in lines.iter().enumerate() {
        let code = uncommented(l);
        for (c, ch) in code.char_indices() {
            match ch {
                '(' => {
                    if depth == 0 && open.is_none() {
                        open = Some((k, c));
                    }
                    depth += 1;
                }
                ')' => {
                    depth -= 1;
                    if depth == 0 && open.is_some() {
                        close = Some((k, c));
                        break 'find;
                    }
                }
                _ => {}
            }
        }
    }
    let (Some((ok, oc)), Some((ck, cc))) = (open, close) else { return Ok(BTreeMap::new()) };
    // the parameters, each line cut to what lies between the parentheses (a doc at its end kept)
    // each line kept at its columns (a doc going on under another lines up with it), cut to what lies between them
    let cut = |k: usize| -> String {
        let l = lines[k];
        if doc_line(l).is_some() {
            return l.to_string();
        }
        let from = if k == ok { oc + 1 } else { 0 };
        let code = uncommented(l);
        let to = if k == ck { cc } else { code.len() };
        let mut s = format!("{}{}", " ".repeat(from), l.get(from..to.max(from)).unwrap_or(""));
        // the doc on the `)` line is the last parameter's, unless an outcome follows there
        if k != ck || !code[cc + 1..].contains("->") {
            s.push_str(&" ".repeat(code.len() - to));
            s.push_str(&l[code.len()..]);
        }
        s
    };
    let params: Vec<String> = (ok..=ck).map(cut).collect();
    let mut out = part_docs(&params.iter().map(String::as_str).collect::<Vec<_>>(), first + ok, ',', file)?;
    // the outcomes: what follows `->` and each line after it
    let mut seen_arrow = false;
    let mut rest: Vec<String> = Vec::new();
    for (k, l) in lines.iter().enumerate().skip(ck) {
        // a line before `->` stands empty, so each line after keeps its number
        if doc_line(l).is_some() {
            rest.push(if seen_arrow { l.to_string() } else { String::new() });
            continue;
        }
        let full = uncommented(l);
        let start = if k == ck { cc + 1 } else { 0 };
        let from = match full[start..].find("->") {
            Some(i) if !seen_arrow => {
                seen_arrow = true;
                start + i + 2
            }
            _ if !seen_arrow => {
                rest.push(String::new());
                continue;
            }
            _ => start,
        };
        rest.push(format!("{}{}{}", " ".repeat(from), &full[from..], &l[full.len()..]));
    }
    // an inline outcome's fields, one a line under `Outcome(` (tovrin-writer N-302): their docs by `Outcome.field`;
    // the outcomes' own from the other lines
    // each line's outcome when it lies inside that outcome's parentheses, else none
    let mut of: Vec<Option<String>> = Vec::new();
    let (mut depth, mut open, mut doc_col): (i32, Option<String>, Option<(bool, usize)>) = (0, None, None);
    for l in &rest {
        let inside = match (doc_line(l), doc_col) {
            // a doc line under the one ending the line above, at its column, goes on with it
            (Some(_), Some((was, c))) if l.find("///") == Some(c) => was,
            _ => depth > 0,
        };
        of.push(open.clone().filter(|_| inside));
        if doc_line(l).is_none() {
            let code = uncommented(l);
            let at_start = depth;
            for c in code.chars() {
                match c {
                    '(' | '[' | '{' => depth += 1,
                    ')' | ']' | '}' => depth -= 1,
                    _ => {}
                }
            }
            if at_start == 0 && depth > 0 {
                open = names_on(code, '|').last().cloned();
            }
            doc_col = doc_after(l).and(l.find("///")).map(|c| (inside, c));
        }
    }
    let only = |n: Option<&String>| -> Vec<&str> { rest.iter().zip(&of).map(|(l, o)| if o.as_ref() == n { l.as_str() } else { "" }).collect() };
    let outer = only(None);
    let inner: BTreeMap<String, Vec<&str>> = of.iter().flatten().map(|n| (n.clone(), only(Some(n)))).collect();
    out.extend(part_docs(&outer, first + ck, '|', file)?);
    for (n, v) in &inner {
        for (f, d) in part_docs(v, first + ck, ',', file)? {
            out.insert(format!("{n}.{f}"), d);
        }
    }
    Ok(out)
}

/// The calls of FILE that parse, and why each that does not was not: a call that fails costs only itself (N-101,
/// every failing line reported), so one attribute the parser lacks does not hide the file's other calls. Each call is
/// read on its own, its line numbers kept.
pub fn parse_call_defs_each(file: &str, text: &str, named: &Named) -> (Vec<CallDef>, Vec<String>) {
    let lines: Vec<&str> = text.lines().collect();
    let starts: Vec<usize> = (0..lines.len()).filter(|&i| lines[i].starts_with("call ") || lines[i].starts_with("decision ")).collect();
    let (mut out, mut errors) = (Vec::new(), Vec::new());
    for (k, &a) in starts.iter().enumerate() {
        // to the next call, or the first line at the left after it that is no comment (a `type`)
        let next = starts.get(k + 1).copied().unwrap_or(lines.len());
        let b = (a + 1..next).find(|&i| !lines[i].is_empty() && !lines[i].starts_with(char::is_whitespace) && !lines[i].starts_with("//")).unwrap_or(next);
        // the call's doc above it comes with it
        let mut a0 = a;
        while a0 > 0 && doc_line(lines[a0 - 1]).is_some() {
            a0 -= 1;
        }
        let one = format!("{}{}", "\n".repeat(a0), lines[a0..b].join("\n"));
        match parse_call_defs(file, &one, named) {
            Ok(cs) => out.extend(cs),
            Err(e) => errors.push(e),
        }
    }
    (out, errors)
}

/// Call FILES (path and text, in order), on top of the types in NAMED (coordd's system types): every file's types
/// first, so a call may use a type of any file, then the calls. A later call of the same name replaces an earlier
/// one (the project's over the system's). Gives the calls by name, and why each file that does not parse was not
/// read (its calls are left out; the others stand).
pub fn parse_call_files(files: &[(String, String)], named: &mut Named) -> (BTreeMap<String, CallDef>, Vec<String>) {
    let mut errors = Vec::new();
    let mut decls = Vec::new();
    let mut bad = BTreeSet::new();
    for (file, text) in files {
        match type_decls(file, text) {
            Ok(ds) => decls.extend(ds),
            Err(e) => {
                errors.push(e);
                bad.insert(file.clone());
            }
        }
    }
    // every file's types, in any order across files
    let (errs, bad_types) = define(decls, named);
    errors.extend(errs);
    bad.extend(bad_types);
    let ok: Vec<(&String, &String)> = files.iter().filter(|(f, _)| !bad.contains(f)).map(|(f, t)| (f, t)).collect();
    let mut calls: BTreeMap<String, CallDef> = BTreeMap::new();
    let folder = |f: &str| std::path::Path::new(f).parent().map(|p| p.to_path_buf());
    for (file, text) in ok {
        let (cs, errs) = parse_call_defs_each(file, text, named);
        errors.extend(errs);
        for c in cs {
            if let Err(why) = crate::check::reserved(&c.sig) {
                errors.push(format!("{}:{}: call {}: {why}; it is left out", c.file, c.line, c.sig.name));
                continue;
            }
            // a later folder's call replaces an earlier one's; two of one name in one folder is a mistake
            if let Some(was) = calls.get(&c.sig.name).filter(|was| folder(&was.file) == folder(&c.file)) {
                errors.push(format!("{}:{}: call {} is declared twice (also {}:{}); the first stands", c.file, c.line, c.sig.name, was.file, was.line));
                continue;
            }
            // and only by one of the same parameters whose answer extends its answer: the kinds written against
            // the earlier one still check (N-101; the library's calls replaced by a project's)
            if let Some(was) = calls.get(&c.sig.name) {
                if let Err(why) = crate::check::replaces(&c.sig, &was.sig) {
                    errors.push(format!("{}:{}: call {} does not replace {}:{}'s: {why}; that one stands", c.file, c.line, c.sig.name, was.file, was.line));
                    continue;
                }
            }
            calls.insert(c.sig.name.clone(), c);
        }
    }
    (calls, errors)
}

/// The signatures of one call file, its own types added to NAMED.
pub fn parse_calls(file: &str, text: &str, named: &mut Named) -> Result<Vec<crate::check::CallSig>, String> {
    parse_types(file, text, named)?;
    Ok(parse_call_defs(file, text, named)?.into_iter().map(|c| c.sig).collect())
}
