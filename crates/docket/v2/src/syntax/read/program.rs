//! The reader's programs: blocks, binding, `handle`, `match`, `if`, `par`, `iterate` (spec 5.1, 7, 8.4).

use super::{R, ReadError, Reader};
use crate::syntax::lex::Tok;
use crate::syntax::tree::*;

impl Reader {

    /// `{ s; s; … }`: `x <- p` binds the rest; a statement that ends in `}` needs no `;` after it.
    pub(super) fn block(&mut self) -> R<Expr> {
        let at = self.at();
        self.mark("{")?;
        let mut stmts: Vec<(Option<String>, Expr)> = Vec::new();
        while !self.eat_mark("}") {
            let bind = if matches!(self.peek(), Tok::Name(_)) && matches!(self.peek_at(1), Tok::Mark("<-")) {
                let n = self.name()?;
                self.bump();
                Some(n)
            } else {
                None
            };
            let ended_in_brace = |r: &Self| r.i > 0 && matches!(r.t[r.i - 1].tok, Tok::Mark("}"));
            stmts.push((bind, self.expr()?));
            if !self.eat_mark(";") && !self.is_mark("}") && !ended_in_brace(self) {
                return self.fail("`;` or `}` expected");
            }
        }
        let Some((last_bind, last)) = stmts.pop() else { return Err(ReadError { at, why: "an empty block".into() }) };
        if let Some(n) = last_bind {
            return Err(ReadError { at: last.at, why: format!("`{n} <- …` is the block's last statement: nothing uses {n}") });
        }
        Ok(stmts.into_iter().rev().fold(last, |rest, (bind, s)| {
            let at = s.at;
            let e = match bind {
                Some(n) => E::Bind(n, Box::new(s), Box::new(rest)),
                None => E::Seq(Box::new(s), Box::new(rest)),
            };
            Expr { at, e }
        }))
    }

    pub(super) fn expr(&mut self) -> R<Expr> {
        let at = self.at();
        let ex = |e| Ok(Expr { at, e });
        if self.is_mark("{") {
            return self.block();
        }
        if self.eat_word("return") {
            return ex(E::Return(Box::new(self.expr()?)));
        }
        if self.eat_word("perform") {
            let p = self.path()?;
            self.mark("(")?;
            return ex(E::Perform(p, self.list_of(")", Self::expr)?));
        }
        if self.eat_word("resume") {
            let k = self.name()?;
            self.mark("(")?;
            let args = self.list_of(")", Self::expr)?;
            let v = match args.len() {
                0 => Expr { at, e: E::Lit(Lit::Unit) },
                1 => args.into_iter().next().unwrap(),
                _ => Expr { at, e: E::Tuple(args) },
            };
            return ex(E::Resume(k, Box::new(v)));
        }
        if self.eat_word("do") {
            return ex(E::Do(Box::new(self.expr()?)));
        }
        if self.eat_word("handle") {
            let body = self.expr()?;
            self.word("with")?;
            self.mark("{")?;
            let mut ret = None;
            if self.eat_word("return") {
                let x = self.name()?;
                self.mark("=>")?;
                ret = Some((x, Box::new(self.expr()?)));
            }
            let mut arms = Vec::new();
            while self.eat_mark("|") {
                let at = self.at();
                let effect = self.path()?;
                self.mark("(")?;
                let pats = self.list_of(")", Self::pat)?;
                self.mark(",")?;
                let k = self.name()?;
                let cond = if self.eat_word("where") { Some(self.expr()?) } else { None };
                self.mark("=>")?;
                arms.push(Arm { at, effect, pats, k, cond, body: self.expr()? });
            }
            self.mark("}")?;
            return ex(E::Handle(Box::new(body), ret, arms));
        }
        if self.eat_word("match") {
            let on = self.expr()?;
            self.mark("{")?;
            let mut cases = Vec::new();
            while self.eat_mark("|") {
                let pat = self.pat()?;
                let cond = if self.eat_word("where") { Some(self.expr()?) } else { None };
                self.mark("=>")?;
                cases.push(Case { pat, cond, body: self.expr()? });
            }
            self.mark("}")?;
            if cases.is_empty() {
                return Err(ReadError { at, why: "a match with no arm".into() });
            }
            return ex(E::Match(Box::new(on), cases));
        }
        if self.eat_word("if") {
            let c = self.expr()?;
            let then = self.block()?;
            let other = if self.eat_word("else") { Some(Box::new(self.block()?)) } else { None };
            return ex(E::If(Box::new(c), Box::new(then), other));
        }
        if self.is_word("par") && !matches!(self.peek_at(1), Tok::Mark("(")) {
            self.bump();
            let x = self.name()?;
            self.word("in")?;
            let list = self.expr()?;
            let n = self.up_to()?;
            return ex(E::ParEach(x, Box::new(list), n, Box::new(self.block()?)));
        }
        if self.eat_word("par") {
            self.mark("(")?;
            return ex(E::Par(self.list_of(")", Self::expr)?));
        }
        if self.eat_word("iterate") {
            let x = self.name()?;
            self.mark("=")?;
            let init = self.or()?;
            let n = self.up_to()?;
            return ex(E::Iterate(x, Box::new(init), n, Box::new(self.block()?)));
        }
        self.or()
    }

    pub(super) fn up_to(&mut self) -> R<Option<i64>> {
        if !(self.is_word("up") && self.word_at(1, "to")) {
            return Ok(None);
        }
        self.i += 2;
        match self.bump() {
            Tok::Int(n) => Ok(Some(n)),
            _ => self.fail("a number after `up to`"),
        }
    }
}
