//! The reader's conditions, values and patterns: operators by precedence, quantifiers, aggregates (spec 2, 5.3).

use super::{R, ReadError, Reader};
use crate::syntax::lex::{At, Tok, lex};
use crate::syntax::tree::*;

impl Reader {

    pub(super) fn or(&mut self) -> R<Expr> {
        let mut a = self.and()?;
        while self.is_mark("||") {
            let at = self.at();
            self.bump();
            let b = self.and()?;
            a = Expr { at, e: E::Bin(Op::Or, Box::new(a), Box::new(b)) };
        }
        Ok(a)
    }

    pub(super) fn and(&mut self) -> R<Expr> {
        let mut a = self.cmp()?;
        while self.is_mark("&&") {
            let at = self.at();
            self.bump();
            let b = self.cmp()?;
            a = Expr { at, e: E::Bin(Op::And, Box::new(a), Box::new(b)) };
        }
        Ok(a)
    }

    pub(super) fn cmp(&mut self) -> R<Expr> {
        let a = self.add()?;
        let op = match self.peek() {
            Tok::Mark("==") => Op::Eq,
            Tok::Mark("!=") => Op::Ne,
            Tok::Mark("<") => Op::Lt,
            Tok::Mark("<=") => Op::Le,
            Tok::Mark(">") => Op::Gt,
            Tok::Mark(">=") => Op::Ge,
            _ => return Ok(a),
        };
        let at = self.at();
        self.bump();
        let b = self.add()?;
        Ok(Expr { at, e: E::Bin(op, Box::new(a), Box::new(b)) })
    }

    pub(super) fn add(&mut self) -> R<Expr> {
        let mut a = self.mul()?;
        loop {
            let op = match self.peek() {
                Tok::Mark("+") if !self.at_holds() => Op::Add,
                Tok::Mark("-") => Op::Sub,
                _ => return Ok(a),
            };
            let at = self.at();
            self.bump();
            let b = self.mul()?;
            a = Expr { at, e: E::Bin(op, Box::new(a), Box::new(b)) };
        }
    }

    pub(super) fn mul(&mut self) -> R<Expr> {
        let mut a = self.unary()?;
        loop {
            let op = match self.peek() {
                Tok::Mark("*") => Op::Mul,
                Tok::Mark("/") => Op::Div,
                Tok::Mark("%") => Op::Rem,
                _ => return Ok(a),
            };
            let at = self.at();
            self.bump();
            let b = self.unary()?;
            a = Expr { at, e: E::Bin(op, Box::new(a), Box::new(b)) };
        }
    }

    pub(super) fn unary(&mut self) -> R<Expr> {
        let at = self.at();
        if self.eat_mark("-") {
            return Ok(Expr { at, e: E::Neg(Box::new(self.unary()?)) });
        }
        let mut a = self.primary()?;
        while self.is_mark(".") && matches!(self.peek_at(1), Tok::Name(_)) {
            self.bump();
            let at = self.at();
            a = Expr { at, e: E::Field(Box::new(a), self.name()?) };
        }
        Ok(a)
    }

    pub(super) fn vars(&mut self) -> R<Vec<String>> {
        let mut v = vec![self.name()?];
        while self.eat_mark(",") {
            v.push(self.name()?);
        }
        self.mark(":")?;
        Ok(v)
    }

    pub(super) fn one_arg(&mut self) -> R<Box<Expr>> {
        self.mark("(")?;
        let e = self.expr()?;
        self.mark(")")?;
        Ok(Box::new(e))
    }

    pub(super) fn primary(&mut self) -> R<Expr> {
        let at = self.at();
        let ex = |e| Ok(Expr { at, e });
        match self.peek().clone() {
            Tok::Int(n) => {
                self.bump();
                ex(E::Lit(Lit::Int(n)))
            }
            Tok::Text(s) => {
                self.bump();
                ex(E::Text(pieces(&s, at)?))
            }
            Tok::Mark("...") => {
                self.bump();
                ex(E::Gap)
            }
            Tok::Mark("(") => {
                self.bump();
                let es = self.list_of(")", Self::expr)?;
                match es.len() {
                    0 => ex(E::Lit(Lit::Unit)),
                    1 => Ok(es.into_iter().next().unwrap()),
                    _ => ex(E::Tuple(es)),
                }
            }
            Tok::Mark("[") => {
                self.bump();
                ex(E::List(self.list_of("]", Self::expr)?))
            }
            Tok::Name(w) => match w.as_str() {
                "true" | "false" => {
                    self.bump();
                    ex(E::Lit(Lit::Bool(w == "true")))
                }
                "self" => {
                    self.bump();
                    ex(E::SelfParty)
                }
                "this" => {
                    self.bump();
                    ex(E::This)
                }
                "Not" if matches!(self.peek_at(1), Tok::Mark("(")) => {
                    self.bump();
                    ex(E::Not(self.one_arg()?))
                }
                "Holds" | "Violated" | "Enabled" if matches!(self.peek_at(1), Tok::Mark("(")) => {
                    self.bump();
                    let i = self.one_arg()?;
                    ex(match w.as_str() {
                        "Holds" => E::Holds(i),
                        "Violated" => E::Violated(i),
                        _ => E::Enabled(i),
                    })
                }
                "Exists" | "Forall" => {
                    self.bump();
                    let vs = self.vars()?;
                    let body = self.or()?;
                    ex(E::Quant(if w == "Exists" { Quant::Exists } else { Quant::Forall }, vs, Box::new(body)))
                }
                "Foreach" => {
                    self.bump();
                    let vs = self.vars()?;
                    let v = self.or()?;
                    let c = if self.eat_word("When") { Some(Box::new(self.or()?)) } else { None };
                    ex(E::Foreach(vs, Box::new(v), c))
                }
                "Count" | "Sum" | "Max" | "Min" => {
                    self.bump();
                    let agg = match w.as_str() {
                        "Count" => Agg::Count,
                        "Sum" => Agg::Sum,
                        "Max" => Agg::Max,
                        _ => Agg::Min,
                    };
                    ex(E::Agg(agg, self.one_arg()?))
                }
                _ => {
                    let p = self.path()?;
                    if self.eat_mark("(") { ex(E::Apply(p, self.list_of(")", Self::expr)?)) } else { ex(E::Name(p)) }
                }
            },
            _ => self.fail("a value or a program expected"),
        }
    }

    pub(super) fn pat(&mut self) -> R<Pat> {
        match self.peek().clone() {
            Tok::Mark("_") => {
                self.bump();
                Ok(Pat::Any)
            }
            Tok::Int(n) => {
                self.bump();
                Ok(Pat::Lit(Lit::Int(n)))
            }
            Tok::Mark("-") if matches!(self.peek_at(1), Tok::Int(_)) => {
                self.bump();
                let Tok::Int(n) = self.bump() else { unreachable!() };
                Ok(Pat::Lit(Lit::Int(-n)))
            }
            Tok::Text(s) => {
                self.bump();
                Ok(Pat::Text(s))
            }
            Tok::Mark("(") => {
                self.bump();
                let ps = self.list_of(")", Self::pat)?;
                Ok(match ps.len() {
                    0 => Pat::Lit(Lit::Unit),
                    1 => ps.into_iter().next().unwrap(),
                    _ => Pat::Tuple(ps),
                })
            }
            Tok::Mark("[") => {
                self.bump();
                let mut ps = Vec::new();
                let mut rest = None;
                while !self.eat_mark("]") {
                    if self.eat_mark("..") {
                        rest = Some(if self.eat_mark("_") { None } else if matches!(self.peek(), Tok::Name(_)) { Some(self.name()?) } else { None });
                        self.mark("]")?;
                        break;
                    }
                    ps.push(self.pat()?);
                    if !self.eat_mark(",") {
                        self.mark("]")?;
                        break;
                    }
                }
                Ok(Pat::List(ps, rest))
            }
            Tok::Name(w) if w == "true" || w == "false" => {
                self.bump();
                Ok(Pat::Lit(Lit::Bool(w == "true")))
            }
            Tok::Name(_) => {
                let p = self.path()?;
                let ctor = p.0.len() > 1 || p.last().starts_with(|c: char| c.is_ascii_uppercase());
                if self.eat_mark("(") {
                    Ok(Pat::Ctor(p, self.list_of(")", Self::pat)?))
                } else if ctor {
                    Ok(Pat::Ctor(p, Vec::new()))
                } else {
                    Ok(Pat::Bind(p.last().to_string()))
                }
            }
            _ => self.fail("a pattern expected"),
        }
    }
}

/// A text's parts: its words, and each `{{ e }}` read as an expression.
pub(super) fn pieces(s: &str, at: At) -> R<Vec<Piece>> {
    let mut v = Vec::new();
    let mut rest = s;
    while let Some(b) = rest.find("{{") {
        if b > 0 {
            v.push(Piece::Words(rest[..b].to_string()));
        }
        let Some(e) = rest[b..].find("}}") else { return Err(ReadError { at, why: "a `{{` in a text with no `}}`".into() }) };
        let inner = &rest[b + 2..b + e];
        let mut r = Reader { t: lex(inner).map_err(|x| ReadError { at, why: format!("in a text's hole: {}", x.why) })?, i: 0 };
        let x = r.expr().map_err(|x| ReadError { at, why: format!("in a text's hole `{}`: {}", inner.trim(), x.why) })?;
        r.end().map_err(|x| ReadError { at, why: format!("in a text's hole `{}`: {}", inner.trim(), x.why) })?;
        v.push(Piece::Hole(Expr { at, e: x.e }));
        rest = &rest[b + e + 2..];
    }
    if !rest.is_empty() || v.is_empty() {
        v.push(Piece::Words(rest.to_string()));
    }
    Ok(v)
}
