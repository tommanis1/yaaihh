//! The gap (spec 8.6): a filling read, resolved, typed and checked in its place before it runs.

use super::*;
use crate::check::{resolve, typing};

impl<N: Norms> Machine<N> {
    /// Run a filling of the gap at `site`; one that fails its checks is `Unfit`.
    pub(super) fn fill(&mut self, r: RunId, v: Value, m: Arc<str>, site: &str) -> Result<(), Fault> {
        let prog = match v {
            Value::Program(p) => p,
            Value::Text(t) => match self.check_filling(r, &t, &m, site) {
                Ok(e) => Prog(Arc::new(Closure { body: e, env: Env::new(), module: m.to_string() })),
                Err(why) => return self.set(r, Ctl::Perform("Unfit".into(), vec![Value::Text(why)])),
            },
            other => return self.set(r, Ctl::Perform("Unfit".into(), vec![Value::Text(format!("{other} is no program"))])),
        };
        self.set(r, Ctl::Eval(prog.0.body.clone(), prog.0.env.clone(), prog.0.module.as_str().into()))
    }

    /// A filling is checked as a text is when loaded: it reads as a block, its names stand for declarations of its
    /// place's scope, it has the type its place needs, and each Effect it may perform that the row around the gap
    /// does not hold is answered by someone (14, check 4). In a run bound to a driver it passes the early
    /// capability check (12).
    fn check_filling(&mut self, r: RunId, text: &str, m: &str, site: &str) -> Result<Expr, String> {
        let mut e = crate::syntax::read_block(text).map_err(|e| e.to_string())?;
        resolve(&self.loaded, m, &[], &mut e).map_err(|e| e.why)?;
        let more = typing::filling(&self.loaded, m, site, &e)?;
        let lost = self.lost(&more);
        if !lost.is_empty() {
            return Err(format!("the filling may perform {}, which nobody answers", lost.join(", ")));
        }
        if let Some(d) = self.runs[r].driver {
            let party = self.runs[r].party.clone();
            self.early_filling(d, &party, &e)?;
        }
        Ok(e)
    }
}
