//! `par` (spec 8.4): branches as runs under the one that waits, at most `up to` at a time, and their cancelling.

use super::*;

impl<N: Norms> Machine<N> {
    pub(super) fn par(&mut self, r: RunId, rest: VecDeque<(usize, Expr, Env)>, up_to: usize, list: bool, m: Arc<str>) -> Result<(), Fault> {
        let n = rest.len();
        self.runs[r].frames.push(Frame::Par { runs: Vec::new(), got: vec![None; n], rest, up_to: up_to.max(1), list, m });
        self.next_branches(r)
    }

    /// Begin branches up to the bound; when all have ended, the `par` gives their values in branch order.
    pub(super) fn next_branches(&mut self, p: RunId) -> Result<(), Fault> {
        let (party, driver, folder) = (self.runs[p].party.clone(), self.runs[p].driver, self.runs[p].folder.clone());
        loop {
            let Some(Frame::Par { runs, rest, up_to, m, .. }) = self.runs[p].frames.last_mut() else { return Err("no `par` here".into()) };
            let live = runs.iter().filter(|x| x.is_some()).count();
            if live >= *up_to {
                break;
            }
            let Some((k, e, env)) = rest.pop_front() else { break };
            let m = m.clone();
            let id = self.begin(Some((p, k)), party.clone(), driver, folder.clone(), Ctl::Eval(e, env, m), Origin::Branch);
            self.follow_branch(p, id, k);
            let Some(Frame::Par { runs, .. }) = self.runs[p].frames.last_mut() else { unreachable!() };
            runs.push(Some(id));
        }
        let Some(Frame::Par { runs, rest, got, list, .. }) = self.runs[p].frames.last() else { unreachable!() };
        if runs.iter().all(Option::is_none) && rest.is_empty() {
            let vs: Vec<Value> = got.iter().map(|g| g.clone().unwrap_or(Value::Unit)).collect();
            let list = *list;
            self.runs[p].frames.pop();
            self.runs[p].ctl = Some(Ctl::Give(if list { Value::List(vs) } else { Value::Tuple(vs) }));
            self.runs[p].state = State::Ready;
        } else {
            self.runs[p].state = State::Waiting;
        }
        Ok(())
    }

    /// Cancel every live run under run `x` (its branches and theirs), except none: `cancelled` for each.
    pub(super) fn cancel_under(&mut self, x: RunId, by: RunId) {
        let by_name = self.runs[by].name.clone();
        for k in 0..self.runs.len() {
            let mut a = self.runs[k].parent;
            let mut under = false;
            while let Some(p) = a {
                if p == x {
                    under = true;
                    break;
                }
                a = self.runs[p].parent;
            }
            if under {
                self.cancel(k, &by_name);
            }
        }
    }

    /// Cancel run `k` if it is live; a driver it rests on is told its line is no longer wanted.
    fn cancel(&mut self, k: RunId, by: &str) {
        if let Some(line) = self.stop(k, by) {
            self.drop_line(line);
        }
    }

    /// Cancel run `k` if it is live, writing `cancelled`; the `effect` line it rests on, if any.
    pub(super) fn stop(&mut self, k: RunId, by: &str) -> Option<usize> {
        let state = self.runs[k].state;
        if !matches!(state, State::Ready | State::Resting(_) | State::Waiting) {
            return None;
        }
        self.close_prefix(k);
        self.runs[k].state = State::Cancelled;
        self.record.push(Line::Cancelled { run: self.runs[k].name.clone(), by: by.to_string() });
        if let State::Resting(line) = state { Some(line) } else { None }
    }

    /// A driver is told the `effect` line at `line` is no longer wanted; no driver is told while replaying exactly.
    pub(super) fn drop_line(&mut self, line: usize) {
        if self.restoring.is_none()
            && let Line::Effect { op, .. } = &self.record[line]
            && let Some((d, _)) = self.carriers.get(op)
        {
            let d = *d;
            self.drivers[d].dropped(line as u64);
        }
    }

    /// Cancel the runs of every `par` in frames dropped with a `k`.
    pub(super) fn cancel_frames(&mut self, fs: &[Frame], by: RunId) {
        for f in fs {
            if let Frame::Par { runs, .. } = f {
                for id in runs.iter().flatten() {
                    let id = *id;
                    self.cancel_under(id, by);
                    let by_name = self.runs[by].name.clone();
                    self.cancel(id, &by_name);
                }
            }
        }
    }
}
