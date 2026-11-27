//! std.comms' model step: `Call-Text` fills a call's template, `Model` asks a party's model, `Read-As` reads its text.

use docket_v2::check::names::Sym;
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::value::{Instance, Value};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::os::fd::RawFd;

mod ask;
mod read;
mod text;

pub const CALL_TEXT: &str = "std.comms.Call-Text";
pub const MODEL: &str = "std.comms.Model";
pub const READ_AS: &str = "std.comms.Read-As";

/// The clerk's file: the command, given a party's name and the prompt on stdin, prints `{"answer", "tokens"}`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub command: Vec<String>,
    /// How many requests run at once; the rest wait their turn.
    pub at_once: usize,
    /// How long one request may take before it fails.
    pub seconds: u64,
}

/// The driver: the loaded declarations a call and its answer's type are read by, and the requests running.
pub struct Clerk {
    syms: BTreeMap<String, Sym>,
    asks: ask::Asks,
}

impl Clerk {
    /// The clerk for the text whose declarations are `syms`; `Err` if the file names no command.
    pub fn new(config: Config, syms: &BTreeMap<String, Sym>) -> Result<Self, String> {
        if config.command.is_empty() || config.at_once == 0 || config.seconds == 0 {
            return Err("the clerk's file wants a command, at_once and seconds above 0".into());
        }
        let kept = syms.iter().filter(|(_, s)| matches!(s, Sym::Call { .. } | Sym::Sum { .. } | Sym::Ctor { .. } | Sym::Record { .. } | Sym::Norm { .. }));
        Ok(Clerk { syms: kept.map(|(k, s)| (k.clone(), s.clone())).collect(), asks: ask::Asks::new(config)? })
    }

    /// What the daemon waits on.
    pub fn waits(&self) -> Vec<RawFd> {
        self.asks.waits()
    }

    /// The models' answers ready, by their effect lines' places; the clerk performs no Events. Never waits.
    pub fn ready(&mut self) -> (Vec<(u64, Reply)>, Vec<Instance>) {
        (self.asks.ready(), Vec::new())
    }
}

/// The name a party value stands for: a program's text, or a driver's instance of a type identified by a text.
fn named(v: &Value) -> Option<&str> {
    match v {
        Value::Text(s) => Some(s),
        Value::Inst(Instance { args, .. }) => match args.as_slice() {
            [Value::Text(s)] => Some(s),
            _ => None,
        },
        _ => None,
    }
}

fn answer(value: Value) -> Reply {
    Reply::Answer { value, costs: Vec::new(), events: Vec::new() }
}

impl Driver for Clerk {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("clerk")]
    }

    fn effects(&self) -> Vec<(String, Vec<String>)> {
        let model = ask::MEASURES.map(String::from).into();
        vec![(CALL_TEXT.into(), Vec::new()), (MODEL.into(), model), (READ_AS.into(), Vec::new())]
    }

    fn events(&self) -> Vec<String> {
        Vec::new()
    }

    fn carry(&mut self, op: &str, args: &[Value], line: u64) -> Reply {
        let done = match (op, args) {
            (CALL_TEXT, [call]) => text::fill(&self.syms, call).map(|t| answer(Value::Text(t))),
            (READ_AS, [Value::Prompt { name, .. }, Value::Text(t)]) => match self.syms.get(name) {
                Some(Sym::Call { result, .. }) => read::read_as(&self.syms, result, t).map(answer),
                _ => Err(format!("{name} is no call of the loaded text")),
            },
            (MODEL, [who, Value::Text(prompt)]) => match named(who) {
                Some(party) => self.asks.ask(line, party, prompt).map(|()| Reply::Later),
                None => Err(format!("{who} names no party")),
            },
            _ => Err(format!("{op} with {} values: not an Effect this driver carries as given", args.len())),
        };
        done.unwrap_or_else(Reply::Failed)
    }

    /// The request is stopped: its command's processes end.
    fn dropped(&mut self, line: u64) {
        self.asks.stop(line);
    }
}

#[cfg(test)]
mod tests;
