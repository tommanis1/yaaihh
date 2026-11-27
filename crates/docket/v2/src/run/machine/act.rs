//! Performing an Act or Event (spec 8.2) and beginning the reactions to it (8.7).

use super::*;

impl<N: Norms> Machine<N> {
    /// Perform Act or Event `i` in run `r`, written in module `m`.
    pub(super) fn perform_act(&mut self, r: RunId, i: Instance, m: Arc<str>) -> Result<(), Fault> {
        let refused = self.act(By::Run(r, m), i);
        match refused {
            None => Ok(()),
            Some((what, why)) => self.set(r, Ctl::Perform("Refused".into(), vec![Value::Text(what), Value::Text(why)])),
        }
    }

    /// The steps of 8.2. Returns the refusal, if any, for the run to perform `Refused` with.
    pub(super) fn act(&mut self, by: By, i: Instance) -> Option<(String, String)> {
        let (party, run) = match &by {
            By::Run(r, _) => (Some(self.runs[*r].party.clone()), Some(self.runs[*r].name.clone())),
            By::Driver(_) | By::Record => (None, None),
        };
        let (kind, module) = match self.loaded.syms.get(&i.ty) {
            Some(Sym::Norm { kind, module, .. }) => (*kind, module.clone()),
            _ => return Some((i.ty.clone(), format!("{} is no Act or Event", i.ty))),
        };
        // 1. source
        let source_ok = match (&by, kind) {
            (By::Run(..), Norm::Act) => i.args.first() == party.as_ref(),
            (By::Run(_, m), Norm::Event) => **m == *module,
            (By::Driver(d), Norm::Event) => self.drivers[*d].events().contains(&i.ty),
            (By::Record, _) => true,
            _ => false,
        };
        // 2 to 4, by the normative core
        let mut outcome = self.outcome(source_ok, &i, party.as_ref());
        if let By::Run(r, _) = &by
            && self.runs[*r].cursor.is_some()
        {
            match self.replayed_act(*r, &i, party.as_ref(), &outcome) {
                // as recorded: Σ already holds it, and its reactions began then (11)
                Ok(true) => {
                    let r = *r;
                    return match outcome {
                        Ok(_) => {
                            self.go_on(r, &i);
                            None
                        }
                        Err((_, text)) => Some((i.to_string(), text)),
                    };
                }
                Ok(false) => outcome = self.outcome(source_ok, &i, party.as_ref()),
                Err(why) => {
                    self.runs[*r].ctl = Some(Ctl::Perform("Fault".into(), vec![Value::Text(why)]));
                    return None;
                }
            }
        }
        let step = match outcome {
            Ok(s) => s,
            Err((why, text)) => {
                self.record.push(Line::Refused { act: i.clone(), party, run, why });
                return Some((i.to_string(), text));
            }
        };
        // 5. commit, then the reactions begin
        self.record.push(Line::Performed { act: i.clone(), party: party.clone(), run: run.clone() });
        self.norms.commit(step);
        self.react(&i);
        if let By::Run(r, _) = by {
            self.go_on(r, &i);
        }
        None
    }

    /// Steps 1 to 4 of 8.2 for `i`, whose source check gave `source_ok`.
    fn outcome(&mut self, source_ok: bool, i: &Instance, party: Option<&Value>) -> replay::Outcome {
        if !source_ok {
            return Err((Refusal::Source, "source".into()));
        }
        self.norms.step(i, party).map_err(|w| match w {
            Why::Policy(w) => (Refusal::Policy(w), "policy".into()),
            Why::Closes(w) => (Refusal::Closes(w), "closes".into()),
        })
    }

    /// 6. the run goes on with the body's value, or ().
    fn go_on(&mut self, r: RunId, i: &Instance) {
        match self.body_of(i) {
            Some((b, env, m)) => {
                self.runs[r].frames.push(Frame::Body);
                self.runs[r].ctl = Some(Ctl::Eval(b, env, m));
            }
            None => self.runs[r].ctl = Some(Ctl::Give(Value::Unit)),
        }
    }

    /// An Act's or Event's `Does`, with its fields bound, if it has one.
    pub(super) fn body_of(&self, i: &Instance) -> Option<(Expr, Env, Arc<str>)> {
        let fields = match self.loaded.syms.get(&i.ty) {
            Some(Sym::Norm { fields: Fields::Named(fs), .. }) => fs.clone(),
            _ => Vec::new(),
        };
        self.loaded.norms.iter().find_map(|d| match &d.decl.what {
            DeclKind::Norm { name, clauses, .. } if crate::check::names::qualify(&d.module, name) == i.ty => clauses.iter().find_map(|c| match c {
                Clause::Does(b) => Some((b.clone(), fields.iter().map(|f| f.name.clone()).zip(i.args.iter().cloned()).collect(), d.module.as_str().into())),
                _ => None,
            }),
            _ => None,
        })
    }

    /// The reactions to a committed transition of `i` (8.7): each way their patterns and conditions bind, in order.
    pub(super) fn react(&mut self, i: &Instance) {
        let ons: Vec<On> = self.loaded.ons.iter().filter(|o| o.target == i.ty).cloned().collect();
        let at = self.record.len() - 1;
        let mut began: BTreeMap<String, usize> = BTreeMap::new();
        for o in ons {
            let mut env = Env::new();
            if o.pats.len() != i.args.len() || !o.pats.iter().zip(&i.args).all(|(p, v)| self.bind(p, v, &mut env)) {
                continue;
            }
            let party = Value::Text(o.module.clone());
            let mut origin = || {
                let k = began.entry(o.module.clone()).or_default();
                *k += 1;
                Origin::On { module: o.module.clone(), at, k: *k - 1 }
            };
            let envs = match &o.cond {
                None => vec![env],
                Some(c) => match self.each(c, &env) {
                    Ok(envs) => envs,
                    Err(why) => {
                        // the condition could not be evaluated: the reaction's run begins and performs `Fault`
                        self.begin(None, party, None, String::new(), Ctl::Perform("Fault".into(), vec![Value::Text(why)]), origin());
                        continue;
                    }
                },
            };
            for e in envs {
                self.begin(None, party.clone(), None, String::new(), Ctl::Eval(o.body.clone(), e, o.module.as_str().into()), origin());
            }
        }
    }
}
