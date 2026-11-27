//! Performing an Effect (spec 8.3): the search for a handler, root arm, `Does` or driver, and `resume`.

use super::*;

/// Where a search for what takes an Effect begins: a frame of a run, and below it outward (none: past every
/// handler), and the root arms loaded before `roots`.
#[derive(Clone, Copy, Debug)]
pub(super) struct Start {
    pub at: Option<(RunId, usize)>,
    pub roots: usize,
}

impl<N: Norms> Machine<N> {
    pub(super) fn perform(&mut self, r: RunId, eff: String, args: Vec<Value>) -> Result<(), Fault> {
        let s = Start { at: Some((r, self.runs[r].frames.len())), roots: self.loaded.ons.len() };
        self.search(r, eff, args, s)
    }

    /// 8.3 from `s`; a `Charge` taken by an arm goes on outward from it when the arm resumes or ends (10).
    pub(super) fn search(&mut self, r: RunId, eff: String, args: Vec<Value>, s: Start) -> Result<(), Fault> {
        let charge = (eff == "Charge").then(|| args.clone());
        // 1, 2: the handlers around, innermost first, through the runs this one rests under
        let mut root_below = s.roots;
        let Some(mut at) = s.at else { return self.after_handlers(r, eff, args, root_below, charge) };
        'search: loop {
            let (x, top) = at;
            for h in (0..top).rev() {
                let probe = match &self.runs[x].frames[h] {
                    Frame::Handle(_, arms, env, m) if arms.iter().any(|a| a.effect.to_string() == eff) => Probe::Handle(arms.clone(), env.clone(), m.clone()),
                    Frame::Across(_, x2, h2) => Probe::Across(*x2, *h2),
                    Frame::Root(i, _) => Probe::Root(*i),
                    _ => Probe::Other,
                };
                match probe {
                    Probe::Handle(arms, env, m) => {
                        for a in &arms {
                            if a.effect.to_string() != eff {
                                continue;
                            }
                            let mut e = env.clone();
                            if a.pats.len() != args.len() || !a.pats.iter().zip(&args).all(|(p, v)| self.bind(p, v, &mut e)) {
                                continue;
                            }
                            if let Some(c) = &a.cond {
                                match self.each(c, &e)?.into_iter().next() {
                                    Some(b) => e = b,
                                    None => continue,
                                }
                            }
                            return self.take(r, (x, h), a.k.clone(), a.body.clone(), e, m.clone(), charge);
                        }
                    }
                    Probe::Across(x2, h2) => {
                        at = (x2, h2);
                        continue 'search;
                    }
                    Probe::Root(i) => {
                        root_below = i;
                        break 'search;
                    }
                    Probe::Other => {}
                }
            }
            match self.runs[x].parent {
                Some(p) => at = (p, self.runs[p].frames.len()),
                None => break,
            }
        }
        self.after_handlers(r, eff, args, root_below, charge)
    }

    /// 8.3 past the handlers: root arms loaded before `root_below`, the Effect's `Does`, a driver, `Never`.
    fn after_handlers(&mut self, r: RunId, eff: String, args: Vec<Value>, root_below: usize, charge: Option<Vec<Value>>) -> Result<(), Fault> {
        // 3: the root arms, the last loaded first
        for i in (0..root_below).rev() {
            let o = self.loaded.ons[i].clone();
            if o.target != eff {
                continue;
            }
            let mut e = Env::new();
            if o.pats.len() != args.len() || !o.pats.iter().zip(&args).all(|(p, v)| self.bind(p, v, &mut e)) {
                continue;
            }
            if let Some(c) = &o.cond {
                match self.each(c, &e)?.into_iter().next() {
                    Some(b) => e = b,
                    None => continue,
                }
            }
            self.runs[r].frames.push(Frame::Root(i, charge));
            return self.set(r, Ctl::Eval(o.body, e, o.module.as_str().into()));
        }
        // 4: the Effect's own `Does`
        if let Some(Sym::Effect { fields, does: Some(b), module, .. }) = self.loaded.syms.get(&eff) {
            let env: Env = fields.iter().map(|f| f.name.clone()).zip(args).collect();
            let (b, m) = (b.clone(), module.clone());
            return self.set(r, Ctl::Eval(b, env, m.as_str().into()));
        }
        if eff == "Charge" {
            return self.set(r, Ctl::Give(Value::Unit));
        }
        // 5: a driver, or the engine's clock
        if eff == "Now" {
            let state = self.take_reads(r);
            if self.reuse(r, &eff, &args, state)? {
                return Ok(());
            }
            let line = self.effect_line(r, &eff, &args, state);
            if self.restoring.is_some() {
                self.recorded_answer(r, line);
                return Ok(());
            }
            let t = (self.clock)();
            self.answered(r, line, Value::Int(t), Vec::new(), Vec::new());
            return Ok(());
        }
        if let Some((d, _)) = self.carriers.get(&eff).cloned() {
            return self.carry(r, Some(d), eff, args);
        }
        // 6: an Effect whose result is Never ends the run, and is performed where it rests
        let never = matches!(eff.as_str(), "Refused" | "Unfit" | "Exhausted" | "Fault") || matches!(self.loaded.syms.get(&eff), Some(Sym::Effect { result: Type::Named(p, _), .. }) if p.to_string() == "Never");
        if never {
            self.raise(r, eff, args);
            return Ok(());
        }
        // replayed exactly, an Effect a driver took then is answered as the record has it, linked now or not (11)
        if self.restoring.is_some() {
            return self.carry(r, None, eff, args);
        }
        Err(format!("{eff} is taken by nothing: no handler, root arm, `Does` or driver (a text check 4 refuses)"))
    }

    /// An Effect whose result is `Never` ends run `r`, and is performed again where the run rests (8.4).
    pub(super) fn raise(&mut self, r: RunId, eff: String, args: Vec<Value>) {
        self.close_prefix(r);
        let run = self.runs[r].name.clone();
        self.record.push(Line::Raised { run, effect: eff.clone(), args: args.clone() });
        self.runs[r].state = State::Raised;
        self.runs[r].ctl = None;
        if let Some(p) = self.runs[r].parent {
            self.cancel_under(p, r);
            if let Some(Frame::Par { .. }) = self.runs[p].frames.last() {
                self.runs[p].frames.pop();
            }
            self.runs[p].ctl = Some(Ctl::Perform(eff, args));
            self.runs[p].state = State::Ready;
        }
    }

    /// Arm `body` of the handler at frame `h` of run `x` takes the Effect performed in run `r`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn take(&mut self, r: RunId, (x, h): (RunId, usize), k: String, body: Expr, mut env: Env, m: Arc<str>, charge: Option<Vec<Value>>) -> Result<(), Fault> {
        let id = self.conts.len();
        env.insert(k, Value::Resumption(id));
        if x == r {
            let fs = self.runs[r].frames.split_off(h);
            self.conts.push(Cont { frames: Some(fs), resumed: false, charge });
            self.runs[r].frames.push(Frame::ArmEnd(id));
        } else {
            self.conts.push(Cont { frames: None, resumed: false, charge });
            self.runs[r].frames.push(Frame::Across(id, x, h));
        }
        self.set(r, Ctl::Eval(body, env, m))
    }

    pub(super) fn resume(&mut self, r: RunId, k: &str, v: Value, env: &Env) -> Result<(), Fault> {
        let Some(Value::Resumption(id)) = env.get(k) else { return Err(format!("{k} is no continuation")) };
        let id = *id;
        if self.conts[id].resumed {
            return Err(format!("{k} is resumed twice (7)"));
        }
        self.conts[id].resumed = true;
        let charge = self.conts[id].charge.clone();
        match self.conts[id].frames.take() {
            Some(fs) => {
                let h = self.runs[r].frames.len();
                self.runs[r].frames.extend(fs);
                match charge {
                    Some(a) => self.outward(r, a, Some((r, h))),
                    None => self.set(r, Ctl::Give(v)),
                }
            }
            None => {
                // a handler in another run: only a `resume` the arm ends with can go back into this branch
                match self.runs[r].frames.last() {
                    Some(Frame::Across(j, x, h)) if *j == id => {
                        let at = (*x, *h);
                        self.runs[r].frames.pop();
                        match charge {
                            Some(a) => self.outward(r, a, Some(at)),
                            None => self.set(r, Ctl::Give(v)),
                        }
                    }
                    _ => Err(format!("`resume {k}` across a `par` is the arm's last step, or it cannot go back")),
                }
            }
        }
    }

    /// `Charge` goes on to the arms outside the one that took it, from `at` (10): no arm keeps it from them.
    pub(super) fn outward(&mut self, r: RunId, args: Vec<Value>, at: Option<(RunId, usize)>) -> Result<(), Fault> {
        let roots = self.loaded.ons.len();
        self.set(r, Ctl::Outward("Charge".into(), args, Start { at, roots }))
    }
}
