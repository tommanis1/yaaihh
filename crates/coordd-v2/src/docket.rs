//! The frame's `Machine` over docket-v2's machine: requests read into its values, its record written out as JSON.

use crate::codec;
use crate::machine::{held, Held, Machine, View};
use crate::peer::Party;
use crate::why;
use coordd_v2_wire::{Answer, StepDelta};
use docket_v2::check::names::Loaded;
use docket_v2::check::names::Sym;
use docket_v2::run::driver::Reply;
use docket_v2::run::machine::{Enabled, Holds, Machine as Runs, State};
use docket_v2::run::norms::Norms;
use docket_v2::run::record::{Line, Refusal};
use docket_v2::run::value::{Instance, Value};
use docket_v2::syntax::tree::{Expr, Norm};
use serde_json::{json, Value as Json};

/// The bindings one instance query may try, its counts in it too: a join that would hold the loop longer is refused.
pub const QUERY_STEPS: u64 = 250_000;
/// The acts one `enabled_for` asks about at most, each worked out as a step.
pub const ENABLED_ACTS: usize = 64;

/// A restored run's driver and folder, by its name and party.
pub type Bind = Box<dyn Fn(&str, &Value) -> (Option<usize>, String)>;
/// The texts loaded under a hash a `loaded` line names.
pub type Texts = Box<dyn FnMut(&[u8; 32]) -> Result<Loaded, String>>;

/// docket-v2's machine, the module a query is read in, the drivers a helper answers for over the socket, what a
/// restore binds runs to and loads texts from, and how much of the record the frame has taken.
pub struct Docket<N: Norms> {
    runs: Runs<N>,
    top: String,
    by_socket: Vec<usize>,
    bind: Bind,
    texts: Texts,
    taken: usize,
}

impl<N: Norms> Docket<N> {
    /// The machine `runs`, its queries read as if written in module `top`; the drivers at places `by_socket` are
    /// answered for over the socket, every other driver is linked and answers through the daemon's loop. A restore
    /// binds each run by `bind` and loads each old text by `texts`.
    pub fn new(runs: Runs<N>, top: &str, by_socket: Vec<usize>, bind: Bind, texts: Texts) -> Self {
        Docket { runs, top: top.to_string(), by_socket, bind, texts, taken: 0 }
    }

    /// `text` read as one expression in module `module`'s names.
    fn read(&self, module: &str, text: &str) -> Result<Expr, String> {
        // a block of one expression reads as that expression
        let mut e = docket_v2::syntax::read_block(&format!("{{ {text} }}")).map_err(|e| e.to_string())?;
        docket_v2::check::resolve(&self.runs.loaded, module, &[], &mut e).map_err(|e| e.to_string())?;
        Ok(e)
    }

    /// Why `act` was refused, if a line written from `at` on refuses it: its failing conditions, or the duty it closes.
    fn refusal(&mut self, at: usize, act: &Instance) -> Option<Json> {
        let why = self.runs.record[at..].iter().find_map(|l| match l {
            Line::Refused { act: a, why, .. } if a == act => Some(why.clone()),
            _ => None,
        })?;
        match why {
            Refusal::Policy(_) => self.runs.norms.why_refused(act).ok().map(|w| why::refused(&w)),
            Refusal::Closes(duty) => Some(json!({"closes": duty})),
            Refusal::Source => None,
        }
    }

    fn reply(&mut self, line: u64, reply: Reply) -> Result<(), String> {
        self.runs.answer(line as usize, reply)?;
        self.runs.run();
        Ok(())
    }

    /// A new text: its rules at once, Σ kept, the runs replaced (spec 11).
    pub fn reload(&mut self, loaded: Loaded) -> Result<(), String> {
        self.runs.reload(loaded).map_err(|e| e.to_string())
    }

    /// Whether TY is a Fact or a Duty of the loaded text.
    fn kind(&self, ty: &str) -> Option<Holds> {
        match self.runs.loaded.syms.get(ty) {
            Some(Sym::Norm { kind: Norm::Fact, .. }) => Some(Holds::Fact),
            Some(Sym::Norm { kind: Norm::Duty, .. }) => Some(Holds::Duty),
            _ => None,
        }
    }
}

/// A run's state in words, for the wire.
fn stated(s: State) -> Json {
    match s {
        State::Ready => json!("ready"),
        State::Resting(line) => json!({"resting": line}),
        State::Waiting => json!("waiting"),
        State::Done => json!("done"),
        State::Raised => json!("raised"),
        State::Cancelled => json!("cancelled"),
    }
}

impl<N: Norms> Machine for Docket<N> {
    fn restore(&mut self, lines: &[Json]) -> Result<(), String> {
        let old = lines.iter().map(codec::read_line).collect::<Result<Vec<_>, _>>()?;
        self.runs.restore(&old, &*self.bind, &mut *self.texts)?;
        // the record's own lines are in the file already
        self.taken = old.len();
        Ok(())
    }

    fn act(&mut self, party: &Party, name: &str, args: &[Json]) -> Result<Json, String> {
        let args = args.iter().map(codec::value).collect::<Result<Vec<_>, _>>()?;
        // an Event over the socket is a `--by-socket` driver's line, from its party; a linked driver states its own
        if let Some(Sym::Norm { kind: Norm::Event, .. }) = self.runs.loaded.syms.get(name) {
            let d = party.driver.filter(|d| self.by_socket.contains(d)).ok_or_else(|| format!("{} performs no Event over the socket: no `--by-socket` driver is its", party.name))?;
            let at = self.runs.record.len();
            self.runs.event(d, Instance { ty: name.to_string(), args });
            self.runs.run();
            let kind = self.runs.record.get(at).map(codec::line).and_then(|l| l.as_object().and_then(|o| o.keys().next().cloned()));
            return Ok(json!({"event": kind}));
        }
        let (at, act) = (self.runs.record.len(), Instance { ty: name.to_string(), args });
        let r = self.runs.start(Value::text(&party.name), party.driver, &party.folder, act.clone())?;
        self.runs.run();
        let mut reply = json!({"run": self.runs.run_name(r), "state": stated(self.runs.state(r))});
        if let Some(w) = self.refusal(at, &act) {
            reply["why"] = w;
        }
        Ok(reply)
    }

    fn answer(&mut self, party: &Party, line: u64, answer: &Answer) -> Result<Json, String> {
        // only the party the table names for the carrying driver answers, and only for a driver not linked
        let d = self.runs.carrier(line as usize).ok_or_else(|| format!("line {line} is no effect a driver carries"))?;
        if party.driver != Some(d) || !self.by_socket.contains(&d) {
            return Err(format!("{} does not answer for the driver that carries line {line}", party.name));
        }
        let reply = match answer {
            Answer::Value { value, costs } => Reply::Answer { value: codec::value(value)?, costs: costs.clone(), events: Vec::new() },
            Answer::Failed { failed } => Reply::Failed(failed.clone()),
        };
        self.reply(line, reply).map(|()| Json::Null)
    }

    fn late(&mut self, line: u64, reply: Reply) -> Result<(), String> {
        self.reply(line, reply)
    }

    fn event(&mut self, driver: usize, ev: Instance) -> Result<(), String> {
        self.runs.event(driver, ev);
        self.runs.run();
        Ok(())
    }

    fn query(&mut self, _: &Party, query: &str) -> Result<Json, String> {
        let top = self.top.clone();
        self.holds(&top, query).map(Json::Bool)
    }

    fn holding(&mut self, _: &Party, name: &str) -> Result<Json, String> {
        Ok(Json::Array(self.runs.holding(name)?.iter().map(codec::inst).collect()))
    }

    fn inst_query(&mut self, _: &Party, expr: &str) -> Result<Json, String> {
        let mut e = docket_v2::syntax::read_block(&format!("{{ {expr} }}")).map_err(|e| e.to_string())?;
        docket_v2::check::resolve(&self.runs.loaded, &self.top, &[], &mut e).map_err(|e| e.to_string())?;
        Ok(Json::Array(self.runs.inst_query(&e, QUERY_STEPS)?.iter().map(codec::of_value).collect()))
    }

    fn enabled_for(&mut self, _: &Party, actor: &Json, target: &Json) -> Result<Json, String> {
        let acts: Vec<&Json> = match target {
            Json::Array(ts) => ts.iter().collect(),
            t => vec![t],
        };
        if acts.len() > ENABLED_ACTS {
            return Err(format!("{} acts asked at once; at most {ENABLED_ACTS}", acts.len()));
        }
        let insts = acts.iter().map(|t| match codec::value(t)? {
            Value::Inst(i) => Ok(i),
            v => Err(format!("{v} is no act instance")),
        });
        let insts = insts.collect::<Result<Vec<Instance>, String>>()?;
        let got = self.runs.enabled_for(&codec::value(actor)?, &insts)?;
        let said = |(i, e): (&Instance, &Enabled)| match e {
            Enabled::Yes => json!({"act": codec::inst(i), "enabled": true}),
            Enabled::No(why) => json!({"act": codec::inst(i), "enabled": false, "why": why}),
        };
        Ok(Json::Array(insts.iter().zip(&got).map(said).collect()))
    }

    fn view(&mut self, types: Option<&[String]>) -> Result<View, String> {
        if types.is_none() {
            // what moves is counted from this whole read on
            drop(self.runs.moved());
        }
        let all = self.runs.holding_types();
        let mut v = View::default();
        let named: Vec<(String, Holds)> = match types {
            None => all,
            Some(ts) => ts.iter().map(|t| all.iter().find(|(n, _)| n == t).cloned().ok_or_else(|| format!("{t} is no Fact or Duty of the loaded text"))).collect::<Result<_, _>>()?,
        };
        for (ty, kind) in named {
            let got = held(self.runs.holding(&ty)?.iter().map(codec::inst));
            match kind {
                Holds::Fact => {
                    v.facts.insert(ty, got);
                }
                Holds::Duty => {
                    v.violated.insert(ty.clone(), held(self.runs.violated(&ty)?.iter().map(codec::inst)));
                    v.duties.insert(ty, got);
                }
            }
        }
        Ok(v)
    }

    /// From what the machine says moved: its instances, the types it could not say read whole, the duties reached.
    fn delta(&mut self, view: &mut View, seq: u64) -> Result<StepDelta, String> {
        let m = self.runs.moved();
        if m.all {
            let now = self.view(None)?;
            let d = crate::delta::between(view, &now, seq);
            *view = now;
            return Ok(d);
        }
        let mut d = StepDelta { seq, ..StepDelta::default() };
        for (i, came) in m.made.iter().map(|i| (i, true)).chain(m.ended.iter().map(|i| (i, false))) {
            let to = match (self.kind(&i.ty), came) {
                (Some(Holds::Fact), true) => &mut d.created_facts,
                (Some(Holds::Fact), false) => &mut d.terminated_facts,
                (Some(Holds::Duty), true) => &mut d.new_duties,
                (Some(Holds::Duty), false) => &mut d.terminated_duties,
                (None, _) => continue,
            };
            to.push(codec::inst(i));
        }
        let mut unviolated = Vec::new();
        for ty in &m.whole {
            let now = held(self.runs.holding(ty)?.iter().map(codec::inst));
            let (was, came, went) = match self.kind(ty) {
                Some(Holds::Fact) => (view.facts.get(ty), &mut d.created_facts, &mut d.terminated_facts),
                Some(Holds::Duty) => (view.duties.get(ty), &mut d.new_duties, &mut d.terminated_duties),
                None => continue,
            };
            let (c, w) = parted(was, &now);
            came.extend(c);
            went.extend(w);
            if self.kind(ty) == Some(Holds::Duty) {
                let now = held(self.runs.violated(ty)?.iter().map(codec::inst));
                let (c, w) = parted(view.violated.get(ty), &now);
                d.violations.extend(c);
                unviolated.extend(w);
            }
        }
        for (i, violated) in &m.duties {
            let j = codec::inst(i);
            let was = view.violated.get(&i.ty).is_some_and(|h| h.contains_key(&j.to_string()));
            match (*violated, was) {
                (true, false) => d.violations.push(j),
                (false, true) => unviolated.push(j),
                _ => {}
            }
        }
        unviolated.retain(|v| !d.terminated_duties.contains(v));
        d.cleared_violations = unviolated;
        crate::delta::apply(view, &d);
        Ok(d)
    }

    fn unwatch(&mut self) {
        self.runs.norms.unwatch();
    }

    fn explain(&mut self, _: &Party, inst: &str) -> Result<Json, String> {
        let e = self.read(&self.top.clone(), inst)?;
        let (i, kind) = self.runs.instance(&e)?;
        match kind {
            Norm::Act | Norm::Event => self.runs.norms.why_refused(&i).map(|w| why::refused(&w)),
            Norm::Fact | Norm::Duty => self.runs.norms.explain(&i).map(|x| why::explain(&x)),
        }
    }

    fn holds(&mut self, module: &str, cond: &str) -> Result<bool, String> {
        let e = self.read(module, cond)?;
        self.runs.ask(&e)
    }

    fn written(&mut self) -> Vec<Json> {
        let out = self.runs.record[self.taken..].iter().map(codec::line).collect();
        self.taken = self.runs.record.len();
        out
    }
}

/// What is in NOW and not in WAS, and what is in WAS and not in NOW: one type's instances.
fn parted(was: Option<&Held>, now: &Held) -> (Vec<Json>, Vec<Json>) {
    let none = Held::new();
    let was = was.unwrap_or(&none);
    let came = now.iter().filter(|(k, _)| !was.contains_key(*k)).map(|(_, j)| j.clone()).collect();
    let went = was.iter().filter(|(k, _)| !now.contains_key(*k)).map(|(_, j)| j.clone()).collect();
    (came, went)
}
