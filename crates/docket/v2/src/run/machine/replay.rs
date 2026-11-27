//! Structural prefix replay (spec 11): a run follows an old run's lines, reusing each recorded answer and outcome
//! while its steps match them, reading the state as it stood then, and goes live at the first that does not.

use super::*;

/// A replaying run's cursor: the old run's `effect`, `performed` and `refused` lines, each with the number of
/// `performed` lines of the record before it (the state it read), and the next one to match. Kept when all are
/// matched, for the run's branches to find theirs; the run goes live at its next step that would write a line.
pub(super) struct Cursor {
    run: String,
    lines: Vec<(usize, usize)>,
    at: usize,
}

impl Cursor {
    fn of(record: &[Line], run: &str) -> Cursor {
        Cursor { run: run.to_string(), lines: lines_of(record, run, record.len()), at: 0 }
    }
}

/// The lines of `run` before line `upto` that a replay matches, a replayed run's prefix through its old run's.
pub(super) fn lines_of(record: &[Line], run: &str, upto: usize) -> Vec<(usize, usize)> {
    let mut lines = Vec::new();
    let mut done = 0;
    for (i, l) in record[..upto].iter().enumerate() {
        match l {
            Line::Prefix { run: x, old, upto } if x == run => lines.extend(lines_of(record, old, *upto)),
            Line::Effect { .. } | Line::Performed { .. } | Line::Refused { .. } if l.run() == Some(run) => lines.push((i, done)),
            _ => {}
        }
        if matches!(l, Line::Performed { .. }) {
            done += 1;
        }
    }
    lines
}

/// What a step of 8.2 does, to be matched against a recorded line.
pub(super) type Outcome = Result<crate::run::norms::Step, (Refusal, String)>;

/// What an Effect in a replaying run takes from the old `effect` line at its cursor.
pub(super) enum Reused {
    /// Nothing: the run goes live here.
    No,
    /// Its answer, and its `charged` lines.
    Answer(Value, VecDeque<(String, i64)>),
    /// The line itself, unanswered, held by a reload: the run rests on it (11).
    Rest(usize),
}

impl<N: Norms> Machine<N> {
    /// Run Act `act` again as `start` does, following the lines of old run `old` (11).
    pub fn replay(&mut self, old: &str, party: Value, driver: Option<usize>, folder: &str, act: Instance) -> Result<RunId, Fault> {
        let r = self.start(party, driver, folder, act)?;
        self.follow(r, old);
        Ok(r)
    }

    /// Run `r` follows old run `old`'s lines, or is live at once if there are none.
    pub(super) fn follow(&mut self, r: RunId, old: &str) {
        let c = Cursor::of(&self.record, old);
        self.runs[r].cursor = (!c.lines.is_empty()).then_some(c);
    }

    /// A branch at `place` of replaying run `p` follows the old branch at that place (11).
    pub(super) fn follow_branch(&mut self, p: RunId, b: RunId, place: usize) {
        if let Some(old) = self.runs[p].cursor.as_ref().map(|c| format!("{}.{place}", c.run)) {
            self.follow(b, &old);
        }
    }

    /// The state run `r` reads: as it stood at its cursor's line, or now.
    pub(super) fn view_of(&self, r: RunId) -> Option<usize> {
        self.runs[r].cursor.as_ref().and_then(|c| c.lines.get(c.at)).map(|x| x.1)
    }

    fn at_line(&self, r: RunId) -> Option<&Line> {
        self.runs[r].cursor.as_ref().and_then(|c| c.lines.get(c.at)).map(|x| &self.record[x.0])
    }

    fn advance(&mut self, r: RunId) {
        if let Some(c) = &mut self.runs[r].cursor {
            c.at += 1;
        }
    }

    /// At step 5 of 8.3: the old `effect` line at the cursor, if its key is the one this Effect has with the head
    /// that line records, gives its answer and its `charged` lines, or, unanswered and held by a reload, itself.
    /// Otherwise `r` goes live here.
    pub(super) fn replayed(&mut self, r: RunId, eff: &str, args: &[Value], state: [u8; 32]) -> Result<Reused, Fault> {
        let Some((i, Line::Effect { head, key, run: old, .. })) = self.runs[r].cursor.as_ref().and_then(|c| c.lines.get(c.at)).map(|x| (x.0, &self.record[x.0])) else {
            self.go_live(r)?;
            return Ok(Reused::No);
        };
        let (head, key, old) = (head.clone(), *key, old.clone());
        if super::drive::key(eff, args, &head, state) != key {
            self.go_live(r)?;
            return Ok(Reused::No);
        }
        let answer = self.record[i..].iter().find_map(|l| match l {
            Line::Answered { line, value } if *line == i => Some(value.clone()),
            _ => None,
        });
        let Some(value) = answer else {
            if self.held.get(&i) == Some(&self.top(r)) {
                self.held.remove(&i);
                self.advance(r);
                return Ok(Reused::Rest(i));
            }
            self.go_live(r)?;
            return Ok(Reused::No);
        };
        // the old run's `charged` lines up to its next line a replay matches
        let charges = self.record[i + 1..]
            .iter()
            .filter(|l| l.run() == Some(old.as_str()))
            .take_while(|l| !matches!(l, Line::Effect { .. } | Line::Performed { .. } | Line::Refused { .. }))
            .filter_map(|l| match l {
                Line::Charged { dim, n, .. } => Some((dim.clone(), *n)),
                _ => None,
            })
            .collect();
        self.advance(r);
        Ok(Reused::Answer(value, charges))
    }

    /// At 8.2: whether the old line at the cursor is the `performed` or `refused` line this step would write; then
    /// nothing is done to Σ and the cursor moves. Otherwise `r` goes live here, and the step is taken again live.
    pub(super) fn replayed_act(&mut self, r: RunId, i: &Instance, party: Option<&Value>, outcome: &Outcome) -> Result<bool, Fault> {
        let same = match (self.at_line(r), outcome) {
            (Some(Line::Performed { act, party: p, .. }), Ok(_)) => act == i && p.as_ref() == party,
            (Some(Line::Refused { act, party: p, why, .. }), Err((w, _))) => act == i && p.as_ref() == party && why == w,
            _ => false,
        };
        if same {
            self.advance(r);
        } else {
            self.go_live(r)?;
        }
        Ok(same)
    }

    /// The prefix ends: `r` reads the state as it now stands, and its tree is put at the head of its first old
    /// `effect` line from the cursor on, unless a run of its family went live before or the record is replayed
    /// exactly, which puts no tree anywhere.
    fn go_live(&mut self, r: RunId) -> Result<(), Fault> {
        let Some(c) = self.close_prefix(r) else { return Ok(()) };
        let head = c.lines[c.at..].iter().find_map(|(i, _)| match &self.record[*i] {
            Line::Effect { head, .. } => Some(head.clone()),
            _ => None,
        });
        if self.tree_put(r) && !self.runs[r].folder.is_empty() && self.restoring.is_none()
            && let Some(h) = head
        {
            (self.put)(&self.runs[r].folder, &h).map_err(|why| format!("the tree cannot be put at {h}: {why}"))?;
        }
        self.see(None);
        Ok(())
    }

    /// The end of `r`'s prefix, however it leaves it (going live, ending, raised, cancelled), written as a `prefix`
    /// line, so that a replay of `r` follows its old run there. A held line it could still have taken over is dropped.
    pub(super) fn close_prefix(&mut self, r: RunId) -> Option<Cursor> {
        let c = self.runs[r].cursor.take()?;
        let upto = c.lines.get(c.at).map_or(self.record.len(), |x| x.0);
        self.record.push(Line::Prefix { run: self.runs[r].name.clone(), old: c.run.clone(), upto });
        // the replacement drops all it holds, a branch of it what its own old branch rested on
        let top = self.top(r);
        let whole = self.runs[r].parent.is_none();
        let gone: Vec<usize> = self.held.iter().filter(|(i, t)| **t == top && (whole || c.lines[c.at..].iter().any(|x| x.0 == **i))).map(|(i, _)| *i).collect();
        for i in gone {
            self.held.remove(&i);
            self.drop_line(i);
        }
        Some(c)
    }

    /// Whether `r` is the first of its family (the run and its branches) to go live, which puts the tree back.
    fn tree_put(&mut self, r: RunId) -> bool {
        let top = self.top(r);
        self.live_trees.insert(top)
    }

    /// The run under no other that `r` is, or is a branch of.
    pub(super) fn top(&self, r: RunId) -> RunId {
        let mut top = r;
        while let Some(p) = self.runs[top].parent {
            top = p;
        }
        top
    }
}
