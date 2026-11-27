//! Drivers (spec 8.3 step 5, 10, 11, 12): the boundary check, the `effect` line and its key, answers and costs.

use super::*;

impl<N: Norms> Machine<N> {
    pub(super) fn carry(&mut self, r: RunId, d: Option<usize>, eff: String, args: Vec<Value>) -> Result<(), Fault> {
        // replayed exactly, whether the arguments crossed is the record's, whichever driver is linked now (11)
        if let Some(old) = &self.restoring {
            let name = self.runs[r].name.as_str();
            let mut next = old[self.record.len().min(old.len())..].iter().skip_while(|l| matches!(l, Line::Prefix { run, .. } if run == name));
            if let Some(Line::Raised { run, effect, args }) = next.next()
                && run == name
                && effect == "Fault"
            {
                return self.set(r, Ctl::Perform("Fault".into(), args.clone()));
            }
        }
        // the boundary: an argument crosses only if each power its constructors' declarations need is let (12)
        if let Some(d) = d.filter(|_| self.restoring.is_none())
            && let Some((_, pt)) = &self.loaded.lets_fact
            && let Some(Sym::Effect { fields, .. }) = self.loaded.syms.get(&eff)
            && let Some(k) = fields.iter().position(|f| matches!(&f.ty, Type::Named(p, _) if p.to_string() == *pt))
        {
            let name = self.drivers[d].name_for(&self.runs[r].party);
            let syms = &self.loaded.syms;
            let needs = |c: &str| match syms.get(c) {
                Some(Sym::Ctor { needs, .. }) => needs.clone(),
                _ => None,
            };
            for a in &args {
                if let Err(why) = self.loaded.lets.crossing(&name, &args[k], a, &needs) {
                    return self.set(r, Ctl::Perform("Fault".into(), vec![Value::Text(why)]));
                }
            }
        }
        let state = self.take_reads(r);
        if self.reuse(r, &eff, &args, state)? {
            return Ok(());
        }
        let line = self.effect_line(r, &eff, &args, state);
        let Some(d) = d.filter(|_| self.restoring.is_none()) else {
            self.recorded_answer(r, line);
            return Ok(());
        };
        let reply = self.drivers[d].carry(&eff, &args, line as u64);
        self.reply(r, line, reply);
        Ok(())
    }

    /// A driver's reply to the `effect` line at `line`, now or later.
    pub(super) fn reply(&mut self, r: RunId, line: usize, reply: Reply) {
        match reply {
            Reply::Answer { value, costs, events } => self.answered(r, line, value, costs, events),
            Reply::Later => self.runs[r].state = State::Resting(line),
            Reply::Failed(why) => {
                self.runs[r].state = State::Ready;
                self.runs[r].ctl = Some(Ctl::Perform("Fault".into(), vec![Value::Text(why)]));
            }
        }
    }

    /// What the core says it read is the reads of the run stepping now (11): `par` interleaves runs, so each keeps
    /// its own.
    pub(super) fn fold_reads(&mut self, r: RunId) {
        // nothing read adds nothing, so a key does not count the steps taken between reads
        let h = self.norms.reads();
        if h != [0; 32] {
            self.runs[r].reads.update(h);
        }
    }

    /// The `effect` line, keyed by the Effect, its arguments, the head of the run's tree as it stands now and what
    /// the run read since its previous `effect` line (11); the line keeps the head, where replay puts the tree back.
    pub(super) fn effect_line(&mut self, r: RunId, eff: &str, args: &[Value], state: [u8; 32]) -> usize {
        // replaying the record exactly, the tree is not read: the line has the head the record's has (11)
        let head = match (&self.restoring, self.runs[r].folder.is_empty()) {
            (Some(old), _) => match old.get(self.record.len()) {
                Some(Line::Effect { head, .. }) => head.clone(),
                _ => String::new(),
            },
            (None, true) => String::new(),
            (None, false) => (self.head)(&self.runs[r].folder),
        };
        let key = key(eff, args, &head, state);
        let run = self.runs[r].name.clone();
        self.record.push(Line::Effect { op: eff.to_string(), args: args.to_vec(), run, head, key });
        self.record.len() - 1
    }

    /// What run `r` read since its previous `effect` line, hashed: `state_hash` (11). Its reads start again.
    pub(super) fn take_reads(&mut self, r: RunId) -> [u8; 32] {
        self.fold_reads(r);
        std::mem::take(&mut self.runs[r].reads).finalize().into()
    }

    /// At step 5 in a replaying run: the recorded answer, if the old line matches, is given with its charges (11).
    pub(super) fn reuse(&mut self, r: RunId, eff: &str, args: &[Value], state: [u8; 32]) -> Result<bool, Fault> {
        match self.replayed(r, eff, args, state)? {
            super::replay::Reused::No => Ok(false),
            super::replay::Reused::Answer(value, charges) => {
                self.runs[r].frames.push(Frame::Charges(charges, value));
                self.runs[r].ctl = Some(Ctl::Give(Value::Unit));
                Ok(true)
            }
            super::replay::Reused::Rest(line) => {
                self.runs[r].state = State::Resting(line);
                Ok(true)
            }
        }
    }

    /// An answer comes: its Events first, then it is checked against the Effect's result type (an answer that is
    /// not of it is a `Fault` where it would cross), then `answered`, each cost charged, and the run goes on.
    pub(super) fn answered(&mut self, r: RunId, line: usize, value: Value, costs: Vec<(String, i64)>, events: Vec<Instance>) {
        self.fold_reads(r);
        let Line::Effect { op, .. } = self.record[line].clone() else { unreachable!() };
        let carrier = self.carriers.get(&op).cloned();
        if let Some((d, _)) = carrier {
            for e in events {
                self.event(d, e);
            }
        }
        // what the Events' steps read is no run's
        self.norms.reads();
        self.runs[r].state = State::Ready;
        let fault = |me: &mut Self, why: String| me.runs[r].ctl = Some(Ctl::Perform("Fault".into(), vec![Value::Text(why)]));
        if let Some(Sym::Effect { result, .. }) = self.loaded.syms.get(&op)
            && !self.fits(&value, result)
        {
            return fault(self, format!("the answer {value} to {op} is not of its result type"));
        }
        // one charge for each dimension the driver stated for the Effect, and no cost in another (10)
        // replayed exactly, the costs are the record's, whichever drivers are linked now
        let dims = match &self.restoring {
            Some(_) => costs.iter().map(|(d, _)| d.clone()).collect(),
            None => carrier.map(|(_, dims)| dims).unwrap_or_default(),
        };
        if let Some((dim, _)) = costs.iter().find(|(d, _)| !dims.contains(d)) {
            return fault(self, format!("a cost in {dim}, which the driver did not state for {op}"));
        }
        let charges: VecDeque<(String, i64)> = dims.into_iter().map(|d| (d.clone(), costs.iter().filter(|(c, _)| *c == d).map(|(_, n)| n).sum())).collect();
        self.record.push(Line::Answered { line, value: value.clone() });
        // its `charged` lines all at once, before any handler of `Charge` writes a line
        let run = self.runs[r].name.clone();
        self.record.extend(charges.iter().map(|(dim, n)| Line::Charged { run: run.clone(), dim: dim.clone(), n: *n }));
        self.runs[r].frames.push(Frame::Charges(charges, value));
        self.runs[r].ctl = Some(Ctl::Give(Value::Unit));
    }
}

/// The replay key of Effect `eff` with `args`, tree head `head` and reads `state` (11).
pub(super) fn key(eff: &str, args: &[Value], head: &str, state: [u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    let written: Vec<String> = args.iter().map(Value::to_string).collect();
    h.update(format!("{eff}\u{1}{}\u{1}{head}\u{1}", written.join("\u{2}")));
    h.update(state);
    h.finalize().into()
}
