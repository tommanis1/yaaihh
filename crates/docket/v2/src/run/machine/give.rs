//! A value given to the top frame of a run (spec 8.3), and a run's end.

use super::*;

impl<N: Norms> Machine<N> {
    pub(super) fn give(&mut self, r: RunId, v: Value) -> Result<(), Fault> {
        let Some(f) = self.runs[r].frames.pop() else { return self.end(r, v) };
        match f {
            Frame::Bind(x, rest, mut env, m) => {
                env.insert(x, v);
                self.set(r, Ctl::Eval(rest, env, m))
            }
            Frame::Seq(rest, env, m) => self.set(r, Ctl::Eval(rest, env, m)),
            Frame::Handle(ret, _, mut env, m) => match ret {
                Some((x, body)) => {
                    env.insert(x, v);
                    self.set(r, Ctl::Eval(body, env, m))
                }
                None => self.set(r, Ctl::Give(v)),
            },
            Frame::Gather { mut done, mut todo, env, m, then } => {
                done.push(v);
                match todo.pop_front() {
                    Some(next) => {
                        self.runs[r].frames.push(Frame::Gather { done, todo, env: env.clone(), m: m.clone(), then });
                        self.set(r, Ctl::Eval(next, env, m))
                    }
                    None => self.finish_gather(r, done, env, m, then),
                }
            }
            Frame::If(a, b, env, m) => {
                if self.truth(&v)? {
                    self.set(r, Ctl::Eval(a, env, m))
                } else {
                    match b {
                        Some(b) => self.set(r, Ctl::Eval(b, env, m)),
                        None => self.set(r, Ctl::Give(Value::Unit)),
                    }
                }
            }
            Frame::Match(cases, env, m) => {
                for c in cases {
                    let mut e = env.clone();
                    if !self.bind(&c.pat, &v, &mut e) {
                        continue;
                    }
                    if let Some(w) = &c.cond
                        && !self.holds(w, &e)?
                    {
                        continue;
                    }
                    return self.set(r, Ctl::Eval(c.body, e, m));
                }
                Err(format!("no arm of a match takes {v}"))
            }
            Frame::Loop { x, body, n, round, env, m } => match &v {
                Value::Ctor { name, args, .. } if name == "Again" => {
                    if n.is_some_and(|n| round >= n) {
                        return self.set(r, Ctl::Perform("Exhausted".into(), Vec::new()));
                    }
                    let mut e = env.clone();
                    e.insert(x.clone(), args.first().cloned().unwrap_or(Value::Unit));
                    self.runs[r].frames.push(Frame::Loop { x, body: body.clone(), n, round: round + 1, env, m: m.clone() });
                    self.set(r, Ctl::Eval(body, e, m))
                }
                Value::Ctor { name, args, .. } if name == "Done" => self.set(r, Ctl::Give(args.first().cloned().unwrap_or(Value::Unit))),
                _ => Err(format!("an `iterate` body gave {v}, not `Again` or `Done`")),
            },
            Frame::ArmEnd(k) => {
                if self.conts[k].resumed {
                    return self.set(r, Ctl::Give(v));
                }
                // the arm ended without resuming: what `k` held is dropped, and its runs with it (8.3)
                if let Some(fs) = self.conts[k].frames.take() {
                    self.cancel_frames(&fs, r);
                }
                match self.conts[k].charge.clone() {
                    Some(a) => {
                        let h = self.runs[r].frames.len();
                        self.runs[r].frames.push(Frame::Keep(v));
                        self.outward(r, a, Some((r, h)))
                    }
                    None => self.set(r, Ctl::Give(v)),
                }
            }
            Frame::Across(k, x, h) => {
                if self.conts[k].resumed {
                    return self.set(r, Ctl::Give(v));
                }
                // the arm ended without resuming: the handler in run x gives `v`, and what it held is dropped
                self.cancel_under(x, r);
                self.runs[x].frames.truncate(h);
                self.runs[x].state = State::Ready;
                match self.conts[k].charge.clone() {
                    Some(a) => {
                        self.runs[x].frames.push(Frame::Keep(v));
                        self.outward(x, a, Some((x, h)))
                    }
                    None => self.set(x, Ctl::Give(v)),
                }
            }
            Frame::Root(i, Some(a)) => self.set(r, Ctl::Outward("Charge".into(), a, Start { at: None, roots: i })),
            Frame::Root(_, None) => self.set(r, Ctl::Give(v)),
            Frame::Keep(kept) => self.set(r, Ctl::Give(kept)),
            Frame::Charges(mut costs, answer) => match costs.pop_front() {
                Some((dim, n)) => {
                    self.runs[r].frames.push(Frame::Charges(costs, answer));
                    self.set(r, Ctl::Perform("Charge".into(), vec![Value::Text(dim), Value::Int(n)]))
                }
                None => self.set(r, Ctl::Give(answer)),
            },
            Frame::Fill(m, site) => self.fill(r, v, m, &site),
            Frame::Body => self.set(r, Ctl::Give(v)),
            Frame::Par { .. } => Err("a value given to a waiting `par`".into()),
        }
    }

    /// A run's program has given its value.
    pub(super) fn end(&mut self, r: RunId, v: Value) -> Result<(), Fault> {
        self.close_prefix(r);
        self.runs[r].state = State::Done;
        let Some(p) = self.runs[r].parent else {
            let run = self.runs[r].name.clone();
            self.record.push(Line::Ended { run, value: v });
            return Ok(());
        };
        let place = self.runs[r].place;
        let Some(Frame::Par { runs, got, .. }) = self.runs[p].frames.last_mut() else { return Err("a branch ended with no `par` waiting".into()) };
        let slot = runs.iter().position(|x| *x == Some(r)).ok_or("a branch its `par` does not know")?;
        runs[slot] = None;
        got[place] = Some(v);
        self.next_branches(p)
    }
}
