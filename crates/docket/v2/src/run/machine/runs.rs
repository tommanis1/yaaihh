//! Runs begun, named, stepped, and given what comes from outside (spec 8.2, 8.8).

use super::*;
use crate::syntax::lex::At;

impl<N: Norms> Machine<N> {
    /// Begin a run for a connection (8.8): its program is Act `act` performed once, as `party`, served by driver
    /// `driver`, bound to the working tree in `folder` (or none: empty). The run ends with the Act's value. Bound to a driver,
    /// it does not start, and writes nothing, unless no Effect it may perform is lost (14, check 4) and the early
    /// capability check passes (12); `Err` says why.
    pub fn start(&mut self, party: Value, driver: Option<usize>, folder: &str, act: Instance) -> Result<RunId, Fault> {
        self.startable(&party, driver, &act)?;
        Ok(self.begin(None, party, driver, folder.to_string(), Ctl::Act(act.clone()), Origin::Act(act)))
    }

    /// What a connection's run must pass to start (12; 14, check 4).
    pub(super) fn startable(&mut self, party: &Value, driver: Option<usize>, act: &Instance) -> Result<(), Fault> {
        if let Some(d) = driver {
            let lost = self.loaded.rows.acts.get(&act.ty).map(|r| self.lost(r)).unwrap_or_default();
            if !lost.is_empty() {
                return Err(format!("{} may perform {}, which nobody answers (14, check 4)", act.ty, lost.join(", ")));
            }
            self.early(d, party, act)?;
        }
        Ok(())
    }

    /// A run begins: under no other it is `rN`, `N` the place of its `began` line; a branch at place `k` of run
    /// `p` is `p`'s name, a dot and `k` (11).
    pub(super) fn begin(&mut self, under: Option<(RunId, usize)>, party: Value, driver: Option<usize>, folder: String, ctl: Ctl, origin: Origin) -> RunId {
        let (name, parent, place) = match under {
            Some((p, k)) => (format!("{}.{k}", self.runs[p].name), Some(p), k),
            None => {
                let name = format!("r{}", self.record.len());
                self.record.push(Line::Began { run: name.clone(), by: party.clone() });
                (name, None, 0)
            }
        };
        self.runs.push(Run { name, party, frames: Vec::new(), ctl: Some(ctl), parent, state: State::Ready, driver, folder, place, reads: Sha256::new(), cursor: None, origin });
        self.runs.len() - 1
    }

    /// The core reads the state after the first `k` committed steps, or now.
    pub(super) fn see(&mut self, k: Option<usize>) {
        if self.viewing != k {
            self.norms.view(k);
            self.viewing = k;
        }
    }

    /// Run `r`'s name, as its lines have it.
    pub fn run_name(&self, r: RunId) -> &str {
        &self.runs[r].name
    }

    pub fn state(&self, r: RunId) -> State {
        self.runs[r].state
    }

    /// An Event driver `d` performs from outside, with no run (8.2 step 1); refused by source unless it named it.
    pub fn event(&mut self, d: usize, ev: Instance) {
        self.act(By::Driver(d), ev);
    }

    /// The driver that carries the `effect` line at `line`, if a driver does.
    pub fn carrier(&self, line: usize) -> Option<usize> {
        match self.record.get(line)? {
            Line::Effect { op, .. } => self.carriers.get(op).map(|(d, _)| *d),
            _ => None,
        }
    }

    /// Whether condition `c` holds now, as no run reads it: what the core read is dropped, so no run's key holds it.
    pub fn ask(&mut self, c: &Expr) -> Result<bool, String> {
        let holds = self.norms.cond(c, &Env::new());
        self.norms.reads();
        holds
    }

    /// Every instance of Fact or Duty `ty`, a full name, that holds now, in the order of values, as no run reads it.
    pub fn holding(&mut self, ty: &str) -> Result<Vec<Instance>, String> {
        let n = match self.loaded.syms.get(ty) {
            Some(Sym::Norm { kind: Norm::Fact | Norm::Duty, fields: Fields::By(_), .. }) => 1,
            Some(Sym::Norm { kind: Norm::Fact | Norm::Duty, fields: Fields::Named(fs), .. }) => fs.len(),
            _ => return Err(format!("{ty} is no Fact or Duty of the loaded text")),
        };
        let xs: Vec<String> = (0..n).map(|k| format!("x{k}")).collect();
        let got = self.instances(ty, &xs);
        self.norms.reads();
        got
    }

    /// Every instance of `ty` that holds, its fields read as variables XS, the names its read is noted with.
    pub(super) fn instances(&mut self, ty: &str, xs: &[String]) -> Result<Vec<Instance>, String> {
        let var = |x: &String| Expr { at: At::default(), e: E::Var(x.clone()) };
        let c = Expr { at: At::default(), e: E::Apply(Path::one(ty), xs.iter().map(var).collect()) };
        let rows = self.norms.each(&c, &Env::new())?;
        Ok(rows.into_iter().map(|mut env| Instance { ty: ty.to_string(), args: xs.iter().filter_map(|x| env.remove(x)).collect() }).collect())
    }

    /// The instance `e` writes, of a Fact, Duty, Act or Event, with its kind: what a connection asks to have explained.
    pub fn instance(&mut self, e: &Expr) -> Result<(Instance, Norm), Fault> {
        let E::Apply(p, xs) = &e.e else { return Err(format!("{} is no instance", crate::syntax::form::expr(e))) };
        let ty = p.to_string();
        let Some(Sym::Norm { kind, .. }) = self.loaded.syms.get(&ty) else { return Err(format!("{ty} is no Fact, Duty, Act or Event")) };
        let kind = *kind;
        let arg = |m: &mut Self, x: &Expr| match &x.e {
            E::Text(ps) => match ps.as_slice() {
                [] => Ok(Value::text("")),
                [Piece::Words(w)] => Ok(Value::text(w)),
                _ => Err("an instance asked about is written without holes".to_string()),
            },
            _ => m.pure(x, &Env::new()),
        };
        let args = xs.iter().map(|x| arg(self, x)).collect::<Result<_, _>>()?;
        Ok((Instance { ty, args }, kind))
    }

    /// The answer to the `effect` line at `line`, and its costs; the run resting on it goes on.
    pub fn answer(&mut self, line: usize, reply: Reply) -> Result<(), Fault> {
        let r = self.runs.iter().position(|x| x.state == State::Resting(line)).ok_or(format!("no run rests on line {line}"))?;
        self.reply(r, line, reply);
        Ok(())
    }

    /// Step every run that can go on, the oldest first, until none can.
    /// A step that fails is `Fault(why)` performed in its run (13), and the others go on.
    pub fn run(&mut self) {
        while let Some(r) = self.runs.iter().position(|x| x.state == State::Ready) {
            while self.runs[r].state == State::Ready {
                self.see(self.view_of(r));
                let faulting = matches!(&self.runs[r].ctl, Some(Ctl::Perform(e, _)) if e == "Fault");
                if let Err(why) = self.step(r) {
                    let args = vec![Value::Text(why)];
                    if faulting {
                        // performing `Fault` itself failed: the run ends with it
                        self.raise(r, "Fault".into(), args);
                    } else {
                        self.runs[r].ctl = Some(Ctl::Perform("Fault".into(), args));
                        self.runs[r].state = State::Ready;
                    }
                }
                self.fold_reads(r);
            }
        }
        // what comes from outside next reads and changes Σ as it stands
        self.see(None);
    }

    pub(super) fn step(&mut self, r: RunId) -> Result<(), Fault> {
        let ctl = self.runs[r].ctl.take().ok_or("a ready run with nothing to do")?;
        match ctl {
            Ctl::Eval(e, env, m) => self.eval(r, e, env, m),
            Ctl::Give(v) => self.give(r, v),
            Ctl::Perform(eff, args) => self.perform(r, eff, args),
            // no module performs it: a connection may ask for an Act, and an Event it asks for is refused by source
            Ctl::Act(i) => self.perform_act(r, i, "".into()),
            Ctl::Outward(eff, args, s) => self.search(r, eff, args, s),
        }
    }

    pub(super) fn set(&mut self, r: RunId, c: Ctl) -> Result<(), Fault> {
        self.runs[r].ctl = Some(c);
        Ok(())
    }
}
