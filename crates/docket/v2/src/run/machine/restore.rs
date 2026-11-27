//! Exact replay (spec 11, P5): a machine on the empty configuration is given the record's inputs in order, every
//! Effect answered from the record and no driver asked, and must write the record again; Σ and every run are then
//! as they were. Runs resting at its end ask their drivers again.

use super::*;

impl<N: Norms> Machine<N> {
    /// Replay `old` on this machine, fresh from `new`: each connection's run starts where its `began` line stands,
    /// bound by `bind` to its driver and folder; each driver's Event and each answer comes where its line stands;
    /// at each `loaded` line the text of its hash is loaded, given by `texts`. Then the machine's own text is
    /// loaded, as a reload, if the record's last is another (11). `Err` names the first line written that is not
    /// the record's.
    pub fn restore(&mut self, old: &[Line], bind: &dyn Fn(&str, &Value) -> (Option<usize>, String), texts: &mut dyn FnMut(&[u8; 32]) -> Result<Loaded, String>) -> Result<(), Fault> {
        if self.record.len() != 1 || !self.runs.is_empty() {
            return Err("a record is replayed on a fresh machine".into());
        }
        let own = self.loaded.clone();
        self.record.clear();
        self.restoring = Some(old.into());
        // the machine's own text is not asked for
        let mut texts = |h: &[u8; 32]| if *h == own.hash { Ok(own.clone()) } else { texts(h) };
        let done = self.inputs(old, bind, &mut texts);
        self.restoring = None;
        done?;
        if self.record.len() != old.len() {
            return Err(format!("the replay wrote {} lines past the record's {}", self.record.len() - old.len(), old.len()));
        }
        if self.loaded.hash != own.hash {
            self.load_text(own, true)?;
        }
        // a run resting on an `effect` line with no answer asks its driver again, or the clock
        let resting: Vec<(RunId, usize)> = self.runs.iter().enumerate().filter_map(|(r, x)| if let State::Resting(l) = x.state { Some((r, l)) } else { None }).collect();
        for (r, line) in resting {
            let Line::Effect { op, args, .. } = self.record[line].clone() else { continue };
            if op == "Now" {
                let t = (self.clock)();
                self.answered(r, line, Value::Int(t), Vec::new(), Vec::new());
            } else if let Some((d, _)) = self.carriers.get(&op).cloned() {
                let reply = self.drivers[d].carry(&op, &args, line as u64);
                self.reply(r, line, reply);
            }
        }
        self.run();
        Ok(())
    }

    /// The answer to the `effect` line at `line` as the record has it: given now if it came at once (its line
    /// follows, with only its Events between), else the run rests until its line comes as an input. A late answer
    /// right after an unrelated driver Event looks the same; the lines come out equal either way.
    pub(super) fn recorded_answer(&mut self, r: RunId, line: usize) {
        let Some(old) = self.restoring.clone() else { return };
        let mut j = line + 1;
        while matches!(old.get(j), Some(Line::Performed { run: None, .. } | Line::Refused { run: None, .. })) {
            j += 1;
        }
        match old.get(j) {
            Some(Line::Answered { line: l, value }) if *l == line => {
                // its Events first, as they came, and what they read is no run's
                self.fold_reads(r);
                for k in line + 1..j {
                    self.old_event(&old[k]);
                }
                self.norms.reads();
                let costs = charges(&old, j);
                self.answered(r, line, value.clone(), costs, Vec::new());
            }
            _ => self.runs[r].state = State::Resting(line),
        }
    }

    /// A driver's Event as its line has it, whichever drivers are linked now (11): performed, steps 2 to 4 taken
    /// again; refused at its source, written again as it stands.
    fn old_event(&mut self, l: &Line) {
        match l {
            Line::Refused { why: Refusal::Source, .. } => self.record.push(l.clone()),
            Line::Performed { act, .. } | Line::Refused { act, .. } => {
                self.act(By::Record, act.clone());
            }
            _ => {}
        }
    }

    fn inputs(&mut self, old: &[Line], bind: &dyn Fn(&str, &Value) -> (Option<usize>, String), texts: &mut dyn FnMut(&[u8; 32]) -> Result<Loaded, String>) -> Result<(), Fault> {
        // each run's first Act and the old run it followed, found in one pass
        let mut firsts: BTreeMap<&str, &Instance> = BTreeMap::new();
        let mut followed: BTreeMap<&str, &str> = BTreeMap::new();
        for l in old {
            match l {
                Line::Performed { act, run: Some(r), .. } | Line::Refused { act, run: Some(r), .. } => {
                    firsts.entry(r.as_str()).or_insert(act);
                }
                Line::Prefix { run, old, .. } => {
                    followed.entry(run.as_str()).or_insert(old.as_str());
                }
                _ => {}
            }
        }
        for (i, l) in old.iter().enumerate() {
            if i < self.record.len() {
                if self.record[i] != *l {
                    return Err(format!("line {i}: the replay wrote {}, the record has {l}", self.record[i]));
                }
                continue;
            }
            match l {
                Line::Began { run, by } => {
                    // a replayed run's first Act may stand in the old run it followed
                    let act = match followed.get(run.as_str()) {
                        Some(_) => super::replay::lines_of(old, run, old.len()).into_iter().find_map(|(k, _)| match &old[k] {
                            Line::Performed { act, .. } | Line::Refused { act, .. } => Some(act.clone()),
                            _ => None,
                        }),
                        None => firsts.get(run.as_str()).map(|a| (*a).clone()),
                    };
                    let act = act.ok_or(format!("line {i}: run {run} performs no Act"))?;
                    let (driver, folder) = bind(run, by);
                    match followed.get(run.as_str()) {
                        Some(o) => self.replay(o, by.clone(), driver, &folder, act)?,
                        None => self.start(by.clone(), driver, &folder, act)?,
                    };
                }
                Line::Loaded { hash } if *hash == self.loaded.hash => self.record.push(l.clone()),
                Line::Loaded { hash } => {
                    let loaded = texts(hash).map_err(|why| format!("line {i}: the text of the record's `loaded` line cannot be had: {why}"))?;
                    if loaded.hash != *hash {
                        return Err(format!("line {i}: the text given for a `loaded` line has another hash"));
                    }
                    self.load_text(loaded, false)?;
                }
                Line::Performed { run: None, .. } | Line::Refused { run: None, .. } => self.old_event(l),
                Line::Answered { line, value } => {
                    let costs = charges(old, i);
                    self.answer(*line, Reply::Answer { value: value.clone(), costs, events: Vec::new() })?;
                }
                _ => return Err(format!("line {i}: {l} is no input, and the replay did not write it")),
            }
            self.run();
            if self.record.get(i) != Some(l) {
                return Err(format!("line {i}: the record has {l}, the replay wrote {}", self.record.get(i).map_or("nothing".into(), |x| x.to_string())));
            }
        }
        Ok(())
    }
}

/// The costs of the answer at line `i`: the `charged` lines written with it, right after it.
fn charges(old: &[Line], i: usize) -> Vec<(String, i64)> {
    old[i + 1..]
        .iter()
        .map_while(|x| match x {
            Line::Charged { dim, n, .. } => Some((dim.clone(), *n)),
            _ => None,
        })
        .collect()
}
