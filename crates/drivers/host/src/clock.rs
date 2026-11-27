//! The clock's Events: Tick each minute, and a listed time's Event once it is past.

use crate::config::Named;
use docket_v2::run::value::{Instance, Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const TICK: &str = "std.budgets.Tick";

/// The minute a time key names, UTC: `YYYY-MM-DD` (its midnight) or `YYYY-MM-DDTHH:MM[:SS]Z`.
pub fn minute_of(key: &str) -> Option<i64> {
    let (date, time) = key.split_once('T').map_or((key, None), |(d, t)| (d, Some(t)));
    let mut d = date.splitn(3, '-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) {
        return None;
    }
    let (h, min) = match time {
        None => (0, 0),
        Some(t) => {
            let t = t.strip_suffix('Z')?;
            let mut p = t.splitn(3, ':').map(|p| p.parse::<i64>().ok());
            (p.next()??, p.next()??)
        }
    };
    // days from 1970-01-01 of a proleptic Gregorian date (Hinnant's days_from_civil)
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    Some(days * 1440 + h * 60 + min)
}

/// The listed times already said to be past, so each is said once, restarts included.
#[derive(Default, Serialize, Deserialize)]
pub struct Clock {
    passed: BTreeSet<String>,
    /// Whether the times file's failure to read has been said, so it is said once until it reads again.
    #[serde(skip)]
    unread: bool,
}

impl Clock {
    /// The Events of minute `now`: its Tick, then every listed time that is past and not yet said.
    pub fn tick(&mut self, now: i64, times: Option<&Named>) -> Vec<Instance> {
        let mut out = vec![Instance { ty: TICK.into(), args: vec![Value::Int(now)] }];
        let Some(t) = times else { return out };
        let listed = match std::fs::read_to_string(&t.from) {
            Ok(text) => {
                self.unread = false;
                text
            }
            Err(e) => {
                if !std::mem::replace(&mut self.unread, true) {
                    eprintln!("docket-driver-host: the times file {}: {e}; no time passes until it reads", t.from);
                }
                return out;
            }
        };
        for key in listed.lines().map(str::trim).filter(|k| !k.is_empty()) {
            if minute_of(key).is_some_and(|m| m <= now) && self.passed.insert(key.to_string()) {
                out.push(Instance { ty: t.event.clone(), args: vec![Value::text(key)] });
            }
        }
        out
    }
}
