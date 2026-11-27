//! The reader: tokens to the tree of `tree.rs`, by the grammar of spec 2 and 5.1.

use super::lex::{At, LexError, Tok, Token, lex};
use super::tree::*;

mod program;
mod value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadError {
    pub at: At,
    pub why: String,
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.at, self.why)
    }
}

impl From<LexError> for ReadError {
    fn from(e: LexError) -> Self {
        ReadError { at: e.at, why: e.why }
    }
}

type R<T> = Result<T, ReadError>;

/// Read one file whole.
pub fn read(text: &str) -> R<Module> {
    let mut r = Reader { t: lex(text)?, i: 0 };
    r.module()
}

/// Read a `block` alone, as a gap's filling is read (8.6).
pub fn read_block(text: &str) -> R<Expr> {
    let mut r = Reader { t: lex(text)?, i: 0 };
    let b = r.block()?;
    r.end()?;
    Ok(b)
}

const NORMS: [(&str, Norm); 4] = [("Fact", Norm::Fact), ("Act", Norm::Act), ("Event", Norm::Event), ("Duty", Norm::Duty)];

struct Reader {
    t: Vec<Token>,
    i: usize,
}

impl Reader {
    fn peek(&self) -> &Tok {
        &self.t[self.i].tok
    }
    fn peek_at(&self, k: usize) -> &Tok {
        &self.t[(self.i + k).min(self.t.len() - 1)].tok
    }
    fn at(&self) -> At {
        self.t[self.i].at
    }
    fn bump(&mut self) -> Tok {
        let t = self.t[self.i].tok.clone();
        if self.i < self.t.len() - 1 {
            self.i += 1;
        }
        t
    }
    fn fail<T>(&self, why: impl Into<String>) -> R<T> {
        Err(ReadError { at: self.at(), why: format!("{}, at {}", why.into(), self.peek()) })
    }
    fn is_mark(&self, m: &str) -> bool {
        matches!(self.peek(), Tok::Mark(x) if *x == m)
    }
    fn is_word(&self, w: &str) -> bool {
        matches!(self.peek(), Tok::Name(x) if x == w)
    }
    fn word_at(&self, k: usize, w: &str) -> bool {
        matches!(self.peek_at(k), Tok::Name(x) if x == w)
    }
    fn eat_mark(&mut self, m: &str) -> bool {
        let y = self.is_mark(m);
        if y {
            self.bump();
        }
        y
    }
    fn eat_word(&mut self, w: &str) -> bool {
        let y = self.is_word(w);
        if y {
            self.bump();
        }
        y
    }
    fn mark(&mut self, m: &str) -> R<()> {
        if self.eat_mark(m) { Ok(()) } else { self.fail(format!("`{m}` expected")) }
    }
    fn word(&mut self, w: &str) -> R<()> {
        if self.eat_word(w) { Ok(()) } else { self.fail(format!("`{w}` expected")) }
    }
    fn name(&mut self) -> R<String> {
        match self.peek().clone() {
            Tok::Name(n) => {
                self.bump();
                Ok(n)
            }
            _ => self.fail("a name expected"),
        }
    }
    fn path(&mut self) -> R<Path> {
        let mut p = vec![self.name()?];
        while self.is_mark(".") && matches!(self.peek_at(1), Tok::Name(_)) {
            self.bump();
            p.push(self.name()?);
        }
        Ok(Path(p))
    }
    fn end(&self) -> R<()> {
        if matches!(self.peek(), Tok::End) { Ok(()) } else { self.fail("the end expected") }
    }
    /// A `+` in the first column: a module's own fact begins (5.5), so an expression before it has ended.
    fn at_holds(&self) -> bool {
        self.is_mark("+") && self.at().col == 1
    }

    // ----- declarations -------------------------------------------------------------------------------------------

    fn module(&mut self) -> R<Module> {
        let name = if self.eat_word("module") { Some(self.path()?) } else { None };
        let mut imports = Vec::new();
        while self.is_word("import") {
            let at = self.at();
            self.bump();
            let path = self.path()?;
            let alias = if self.eat_word("as") { Some(self.name()?) } else { None };
            imports.push(Import { path, alias, at });
        }
        let mut decls = Vec::new();
        while !matches!(self.peek(), Tok::End) {
            decls.push(self.decl()?);
        }
        Ok(Module { name, imports, decls })
    }

    fn decl(&mut self) -> R<Decl> {
        let at = self.at();
        let what = if self.eat_mark("+") {
            // `+ instance .`: a name and its arguments, read apart so that the closing `.` is not a field's
            let at = self.at();
            let p = self.path()?;
            self.mark("(")?;
            let args = self.list_of(")", Self::expr)?;
            self.mark(".")?;
            DeclKind::Holds(Expr { at, e: E::Apply(p, args) })
        } else if let Some(kind) = self.norm_word() {
            self.bump();
            let name = self.name()?;
            let fields = if self.is_mark("(") {
                Fields::Named(self.fields()?)
            } else if self.eat_word("Identified") {
                self.word("by")?;
                Fields::By(self.ty()?)
            } else {
                Fields::Named(Vec::new())
            };
            DeclKind::Norm { kind, name, fields, clauses: self.clauses()? }
        } else if self.eat_word("Extend") {
            let Some(kind) = self.norm_word() else { return self.fail("`Fact`, `Act`, `Event` or `Duty` expected") };
            self.bump();
            DeclKind::Extend { kind, name: self.path()?, clauses: self.clauses()? }
        } else if self.eat_word("Effect") {
            let name = self.name()?;
            let params = self.params()?;
            let fields = self.fields()?;
            self.mark("->")?;
            let result = self.ty()?;
            let does = if self.eat_word("Does") { Some(self.block()?) } else { None };
            DeclKind::Effect { name, params, fields, result, does }
        } else if self.eat_word("Call") {
            let name = self.name()?;
            let fields = self.fields()?;
            self.mark("->")?;
            let result = self.ty()?;
            self.word("Template")?;
            let template = match self.bump() {
                Tok::Text(s) => value::pieces(&s, at)?,
                _ => return self.fail("a template's text expected"),
            };
            DeclKind::Call { name, fields, result, template }
        } else if self.eat_word("type") {
            let name = self.name()?;
            let params = self.params()?;
            if self.eat_mark("=") {
                let mut variants = Vec::new();
                // the first constructor may stand without its `|`
                let mut first = !self.is_mark("|");
                while std::mem::take(&mut first) || self.eat_mark("|") {
                    let name = self.name()?;
                    let fields = if self.is_mark("(") { self.fields()? } else { Vec::new() };
                    let needs = if self.eat_word("Needs") {
                        match self.bump() {
                            Tok::Text(s) => Some(s),
                            _ => return self.fail("the power a constructor needs, as a text"),
                        }
                    } else {
                        None
                    };
                    variants.push(Variant { name, fields, needs });
                }
                if variants.is_empty() {
                    return self.fail("a type with no constructor");
                }
                DeclKind::Sum { name, params, variants }
            } else {
                self.mark("{")?;
                let fields = self.list_of("}", Self::field)?;
                DeclKind::Record { name, params, fields }
            }
        } else if self.eat_word("fn") {
            let name = self.name()?;
            let params = self.params()?;
            let fields = self.fields()?;
            self.mark("->")?;
            let result = self.ty()?;
            DeclKind::Fn { name, params, fields, result, body: self.block()? }
        } else if self.eat_word("on") {
            let target = self.path()?;
            self.mark("(")?;
            let pats = self.list_of(")", Self::pat)?;
            let cond = if self.eat_word("where") { Some(self.expr()?) } else { None };
            self.word("do")?;
            DeclKind::On { target, pats, cond, body: self.expr()? }
        } else {
            return self.fail("a declaration expected");
        };
        Ok(Decl { at, what })
    }

    fn norm_word(&self) -> Option<Norm> {
        NORMS.iter().find(|(w, _)| self.is_word(w)).map(|(_, k)| *k)
    }

    fn params(&mut self) -> R<Vec<String>> {
        if !self.eat_mark("<") {
            return Ok(Vec::new());
        }
        self.list_of(">", Self::name)
    }

    fn fields(&mut self) -> R<Vec<Field>> {
        self.mark("(")?;
        self.list_of(")", Self::field)
    }

    fn field(&mut self) -> R<Field> {
        let name = self.name()?;
        self.mark(":")?;
        Ok(Field { name, ty: self.ty()? })
    }

    /// Entries separated by `,` up to the closing mark, which is eaten; a `,` before it is allowed.
    fn list_of<T>(&mut self, close: &str, mut one: impl FnMut(&mut Self) -> R<T>) -> R<Vec<T>> {
        let mut v = Vec::new();
        while !self.eat_mark(close) {
            v.push(one(self)?);
            if !self.eat_mark(",") {
                self.mark(close)?;
                break;
            }
        }
        Ok(v)
    }

    fn clauses(&mut self) -> R<Vec<Clause>> {
        let mut v = Vec::new();
        loop {
            let c = if self.is_word("Holds") && self.word_at(1, "when") {
                self.i += 2;
                Clause::HoldsWhen(self.expr()?)
            } else if self.is_word("Violated") && self.word_at(1, "when") {
                self.i += 2;
                Clause::ViolatedWhen(self.expr()?)
            } else if self.is_word("Conditioned") && self.word_at(1, "by") {
                self.i += 2;
                Clause::ConditionedBy(self.expr()?)
            } else if self.is_word("Derived") && self.word_at(1, "from") {
                self.i += 2;
                Clause::DerivedFrom(self.expr()?)
            } else if self.is_word("Syncs") && self.word_at(1, "with") {
                self.i += 2;
                Clause::SyncsWith(self.effectful()?)
            } else if self.eat_word("Creates") {
                Clause::Creates(self.effectful()?)
            } else if self.eat_word("Terminates") {
                Clause::Terminates(self.effectful()?)
            } else if self.eat_word("Obfuscates") {
                Clause::Obfuscates(self.effectful()?)
            } else if self.eat_word("Prefer") {
                Clause::Prefer(self.expr()?)
            } else if self.eat_word("Returns") {
                Clause::Returns(self.ty()?)
            } else if self.eat_word("Does") {
                Clause::Does(self.block()?)
            } else {
                return Ok(v);
            };
            v.push(c);
        }
    }

    fn effectful(&mut self) -> R<Vec<Effectful>> {
        let mut v = Vec::new();
        loop {
            let inst = self.expr()?;
            let when = if self.eat_word("When") { Some(self.expr()?) } else { None };
            v.push(Effectful { inst, when });
            if !self.eat_mark(",") {
                return Ok(v);
            }
        }
    }

    // ----- types --------------------------------------------------------------------------------------------------

    fn ty(&mut self) -> R<Type> {
        if self.eat_mark("(") {
            let ts = self.list_of(")", Self::ty)?;
            return Ok(if ts.len() == 1 { ts.into_iter().next().unwrap() } else { Type::Tuple(ts) });
        }
        let p = self.path()?;
        if p.0 == ["Program"] && self.eat_mark("<") {
            let t = self.ty()?;
            let row = if self.eat_mark("!") { self.row()? } else { Row { implied: true, ..Row::default() } };
            self.mark(">")?;
            return Ok(Type::Program(Box::new(t), row));
        }
        let args = if self.eat_mark("<") { self.list_of(">", Self::ty)? } else { Vec::new() };
        Ok(Type::Named(p, args))
    }

    fn row(&mut self) -> R<Row> {
        self.mark("{")?;
        let mut row = Row::default();
        while !self.eat_mark("}") {
            if self.eat_mark("..") {
                row.rest = Some(self.name()?);
            } else {
                row.effects.push(self.path()?);
            }
            if !self.eat_mark(",") {
                self.mark("}")?;
                break;
            }
        }
        Ok(row)
    }
}
