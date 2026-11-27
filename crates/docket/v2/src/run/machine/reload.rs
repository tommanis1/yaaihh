//! A reload (spec 11): the rules change at once and Σ stays; every run that has not ended is replaced by a run of the
//! new text replayed against its lines, which takes over the Effect the old run rested on where it reaches that line.

use super::*;

impl<N: Norms> Machine<N> {
    /// A reload (5.5, 11): the core takes the new text's rules and keeps Σ, a `loaded` line is written, and every run
    /// that has not ended is replaced, in the order they began; the replacements run until they rest, so that an
    /// answer coming next finds the one that took its line over. `Err`, the text not loaded, if a reaction of the new
    /// text may perform an Effect nobody answers (14, check 4).
    pub fn reload(&mut self, loaded: Loaded) -> Result<(), Fault> {
        self.load_text(loaded, true)
    }

    /// A text loaded; an old text that exact replay loads at its `loaded` line is not checked against the drivers
    /// linked now (11).
    pub(super) fn load_text(&mut self, loaded: Loaded, check: bool) -> Result<(), Fault> {
        let old = std::mem::replace(&mut self.loaded, loaded);
        if let Some(why) = self.lost_reaction().filter(|_| check) {
            self.loaded = old;
            return Err(why);
        }
        self.norms.load(&self.loaded.norms)?;
        self.record.push(Line::Loaded { hash: self.loaded.hash });
        let live: Vec<RunId> = (0..self.runs.len()).filter(|r| self.runs[*r].parent.is_none() && matches!(self.runs[*r].state, State::Ready | State::Resting(_) | State::Waiting)).collect();
        for r in live {
            self.replace(r);
        }
        self.run();
        Ok(())
    }

    /// Run `r` and its branches are cancelled, and a run of the new text replayed against `r`'s lines begins in its
    /// place, holding the lines they rested on; with none, or none that can start, they are dropped (11).
    fn replace(&mut self, r: RunId) {
        let like = self.like(r);
        let mut rested = Vec::new();
        for k in r + 1..self.runs.len() {
            if self.runs[k].parent.is_some() && self.top(k) == r {
                rested.extend(self.stop(k, "reload"));
            }
        }
        let by = match &like {
            Err(why) => format!("reload: {why}"),
            Ok(_) => "reload".into(),
        };
        rested.extend(self.stop(r, &by));
        let Ok(Some(ctl)) = like else {
            for line in rested {
                self.drop_line(line);
            }
            return;
        };
        let x = &self.runs[r];
        let (party, driver, folder, origin, old) = (x.party.clone(), x.driver, x.folder.clone(), x.origin.clone(), x.name.clone());
        let n = self.begin(None, party, driver, folder, ctl, origin);
        self.held.extend(rested.into_iter().map(|line| (line, n)));
        self.follow(n, &old);
    }

    /// What `r`'s like in the new text begins with: none if the text has no such program, `Err` if it cannot start.
    fn like(&mut self, r: RunId) -> Result<Option<Ctl>, Fault> {
        match self.runs[r].origin.clone() {
            Origin::Act(act) if self.loaded.syms.contains_key(&act.ty) => {
                match self.restoring.clone() {
                    // replayed exactly, whether it could start is the record's, whichever drivers are linked now
                    Some(old) => {
                        let name = &self.runs[r].name;
                        let by = old[self.record.len()..].iter().find_map(|l| match l {
                            Line::Cancelled { run, by } if run == name => Some(by.clone()),
                            _ => None,
                        });
                        match by.as_deref().and_then(|b| b.strip_prefix("reload: ")) {
                            Some(why) => return Err(why.to_string()),
                            None => {}
                        }
                    }
                    None => {
                        let party = self.runs[r].party.clone();
                        self.startable(&party, self.runs[r].driver, &act)?
                    }
                }
                Ok(Some(Ctl::Act(act)))
            }
            Origin::On { module, at, k } => {
                let Some(Line::Performed { act, .. }) = self.record.get(at).cloned() else { return Ok(None) };
                // its bindings found in the state as it stood at that line
                let done = self.record[..=at].iter().filter(|l| matches!(l, Line::Performed { .. })).count();
                self.see(Some(done));
                let found = self.reaction(&module, &act, k);
                self.see(None);
                found
            }
            _ => Ok(None),
        }
    }

    /// The body and bindings of the `k`-th run module `module`'s reactions to `act` begin, in the state read now; as
    /// `react` counts them, a condition that cannot be evaluated is one run, which performs `Fault`.
    fn reaction(&mut self, module: &str, act: &Instance, k: usize) -> Result<Option<Ctl>, Fault> {
        let ons: Vec<On> = self.loaded.ons.iter().filter(|o| o.target == act.ty && o.module == module).cloned().collect();
        let mut n = 0;
        for o in ons {
            let mut env = Env::new();
            if o.pats.len() != act.args.len() || !o.pats.iter().zip(&act.args).all(|(p, v)| self.bind(p, v, &mut env)) {
                continue;
            }
            let ctls = match &o.cond {
                None => vec![Ctl::Eval(o.body.clone(), env, module.into())],
                Some(c) => match self.each(c, &env) {
                    Ok(envs) => envs.into_iter().map(|e| Ctl::Eval(o.body.clone(), e, module.into())).collect(),
                    Err(why) => vec![Ctl::Perform("Fault".into(), vec![Value::Text(why)])],
                },
            };
            for ctl in ctls {
                if n == k {
                    return Ok(Some(ctl));
                }
                n += 1;
            }
        }
        Ok(None)
    }
}
