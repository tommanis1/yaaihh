//! How long an owed step has been silent, the `waited` policy's `expired` reads (N-159 posts 56, 59): a wait lasts
//! while the awaited process lives. Zero while its agent is in a turn or a runner job runs for its ticket; otherwise
//! the minutes since the latest of the step's `since`, the agent's last turn end and the last `job_end` on the ticket.
//! Taken from the lines the log holds, so a restart of coordd rebuilds the jobs still open from the log.
//! A step given to the runner is not silent while the runner is seen alive (paper-research N-159 post 109): the
//! project source's `runner_seen` (at coordd's start, with `up`), `runner_up` and `runner_down`. Its silence starts
//! when the runner is seen gone; with nothing heard of the runner, its steps go by the rule above.
//! Time during which no coordd ran on the record is not silence (paper-research N-101 post 186): an agent cannot be
//! silent towards a coordd that is not there. A running coordd marks the record alive each minute (`tickets/alive.json`,
//! `alive`); at its start the time from the last mark to then is a time down (`came_up`), kept in the same file, and
//! `waited` leaves the times down out. A record with no mark yet takes its log's last line as the last time alive.

use chrono::{DateTime, Utc};
use docket_machine::Line;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Saved with a replay stopped at a cut (replay_state.rs) but for the times down, which `read_down` reads at each start.
#[derive(Default, Debug, serde::Serialize, serde::Deserialize)]
pub struct Quiet {
    /// By ticket: the runner's jobs started there and not ended (their ids), and when the last one ended.
    jobs: BTreeMap<u64, (BTreeSet<String>, Option<String>)>,
    /// By agent: when its last turn ended (or was cut).
    turn_end: BTreeMap<String, String>,
    /// The runner as last seen by the project's source: alive or not, and when; None until the source says.
    runner: Option<(bool, String)>,
    /// The times no coordd ran on the record, from `tickets/alive.json`.
    #[serde(skip)]
    down: Vec<(DateTime<Utc>, DateTime<Utc>)>,
}

/// `tickets/alive.json`: when a coordd last marked the record alive, and the times none ran.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Alive {
    alive: Option<String>,
    #[serde(default)]
    down: Vec<(String, String)>,
}

fn read_alive(file: &str) -> Alive {
    std::fs::read_to_string(file).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn write_alive(file: &str, a: &Alive) -> Result<(), String> {
    let tmp = format!("{file}.new");
    std::fs::write(&tmp, serde_json::to_vec(a).unwrap_or_default()).and_then(|_| std::fs::rename(&tmp, file)).map_err(|e| format!("{file}: {e}"))
}

/// The agent a runner step awaits (docket-machine: `step runner ...`).
pub const RUNNER: &str = "runner";

/// A job's id in a `job_start`/`job_end` value: `id`, else `job`.
fn job_id(v: &Value) -> Option<String> {
    let f = |k: &str| v.get(k).and_then(|x| x.as_str().map(str::to_owned).or_else(|| x.as_u64().map(|n| n.to_string())));
    f("id").or_else(|| f("job"))
}

fn at(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|t| t.with_timezone(&Utc))
}

impl Quiet {
    /// Take a line of the log: a job started or ended on a ticket, a turn ended or cut.
    pub fn take(&mut self, l: &Line) {
        match l {
            Line::Event { at, name, value, on: Some(t), .. } if name == "job_start" || name == "job_end" => {
                let Some(id) = job_id(value) else { return };
                let e = self.jobs.entry(*t).or_default();
                if name == "job_start" {
                    e.0.insert(id);
                } else {
                    e.0.remove(&id);
                    e.1 = Some(at.clone());
                }
            }
            Line::TurnEnded { at, agent, .. } | Line::TurnCut { at, agent } => self.turn_ended(agent, at),
            _ => {}
        }
    }

    /// AGENT's turn ended AT (also when the end goes to no ticket).
    pub fn turn_ended(&mut self, agent: &str, at: &str) {
        self.turn_end.insert(agent.to_string(), at.to_string());
    }

    /// The runner seen alive (UP) or gone at AT: the source's `runner_seen`, `runner_up`, `runner_down`.
    pub fn runner_seen(&mut self, up: bool, at: &str) {
        self.runner = Some((up, at.to_string()));
    }

    /// The times down kept in FILE (`tickets/alive.json`), read when the log is.
    pub fn read_down(&mut self, file: &str) {
        self.down = read_alive(file).down.iter().filter_map(|(a, b)| Some((at(a)?, at(b)?))).collect();
    }

    /// A coordd starts on the record at NOW: the time since the last mark in FILE (else since LAST_LINE, the log's
    /// last line) is a time down, kept in FILE; the record is marked alive at NOW.
    pub fn came_up(&mut self, file: &str, last_line: Option<&str>, now: DateTime<Utc>) -> Result<(), String> {
        let mut a = read_alive(file);
        if let Some(from) = a.alive.as_deref().or(last_line).and_then(at).filter(|f| *f < now) {
            a.down.push((from.to_rfc3339(), now.to_rfc3339()));
            self.down.push((from, now));
        }
        a.alive = Some(now.to_rfc3339());
        write_alive(file, &a)
    }

    /// The record marked alive in FILE at NOW, by a running coordd.
    pub fn alive(&self, file: &str, now: DateTime<Utc>) -> Result<(), String> {
        let mut a = read_alive(file);
        a.alive = Some(now.to_rfc3339());
        write_alive(file, &a)
    }

    /// Whether a runner job runs for ticket T.
    pub fn job_running(&self, t: u64) -> bool {
        self.jobs.get(&t).is_some_and(|j| !j.0.is_empty())
    }

    /// The minutes the step of T owed by AGENT since SINCE has been silent at NOW; IN_TURN: AGENT is in a turn.
    pub fn waited(&self, t: u64, agent: &str, since: &str, in_turn: bool, now: DateTime<Utc>) -> i64 {
        if in_turn || self.job_running(t) {
            return 0;
        }
        // a step given to the runner waits while the runner lives; from when it was seen gone, it is silent
        let gone = match &self.runner {
            Some((true, _)) if agent == RUNNER => return 0,
            Some((false, at)) if agent == RUNNER => Some(at.as_str()),
            _ => None,
        };
        let last_end = self.jobs.get(&t).and_then(|j| j.1.as_deref());
        let latest = [Some(since), self.turn_end.get(agent).map(String::as_str), last_end, gone].into_iter().flatten().filter_map(at).max();
        let Some(l) = latest else { return 0 };
        // the times no coordd ran, inside [l, now], are not silence
        let down: chrono::Duration = self.down.iter().map(|(a, b)| (*b.min(&now) - *a.max(&l)).max(chrono::Duration::zero())).sum();
        (now - l - down).num_minutes().max(0)
    }
}
