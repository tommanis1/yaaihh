//! eFLINT: a language for executable norm specifications (facts, acts, duties, events).
//!
//! This crate is an independent implementation. Its structure follows the published papers on
//! the language and the behaviour of the BSD-3-Clause reference interpreter.
//!
//! * [`parser`]: text to phrases.
//! * [`compile`]: the static phase and specifications.
//! * [`eval`]: states, terms, derivation.
//! * [`interp`]: running one phrase.
//! * [`session`]: files, `#include`, reports.
//! * [`replay`]: a record of events performed in order, the state after each.
//! * [`clingo`]: a specification as an answer set program, for model checking offline.

pub mod ast;
pub mod clingo;
pub mod compile;
pub mod eval;
pub mod interp;
pub mod lexer;
pub mod parser;
pub mod replay;
pub mod session;
