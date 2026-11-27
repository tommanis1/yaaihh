//! Usage summed into five-minute, hourly and daily cells per key, so a slice reads a few hundred cells, never rows.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// The three steps, in milliseconds: five minutes, an hour, a day (UTC).
pub const STEPS: [i64; 3] = [300_000, 3_600_000, 86_400_000];

/// The candles a chart is drawn at, each a run of one step's cells: five minutes, a quarter hour, an hour, six hours, a
/// day (UTC).
pub const CANDLES: [i64; 5] = [STEPS[0], 3 * STEPS[0], STEPS[1], 6 * STEPS[1], STEPS[2]];

/// How many candles a window holds at most when none is picked.
const AUTO_BARS: i64 = 200;

/// The candle a window of SPAN ms is drawn at when none is picked: the shortest that gives at most [`AUTO_BARS`].
pub fn auto_candle(span: i64) -> i64 {
    CANDLES.into_iter().find(|c| span / c <= AUTO_BARS).unwrap_or(STEPS[2])
}

/// Where a request was counted: by coordd from the harness's own files, or by the gateway.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    #[default]
    Native,
    Gateway,
}

/// What a cell is summed by.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Key {
    /// Claude, Codex, OpenCode or agy ([`provider`]).
    pub provider: String,
    pub account: String,
    pub source: Source,
    pub model: String,
    pub agent: String,
    pub project: String,
}

/// How many latency bins a cell keeps: log-spaced from [`BIN_FIRST_MS`] to ten minutes, the last open.
pub const BINS: usize = 64;
pub const BIN_FIRST_MS: f64 = 50.0;
const BIN_LAST_MS: f64 = 600_000.0;

/// The bin a latency of MS falls in.
pub fn bin(ms: u64) -> usize {
    let ratio = (BIN_LAST_MS / BIN_FIRST_MS).powf(1.0 / (BINS - 1) as f64);
    ((ms as f64 / BIN_FIRST_MS).max(1.0).ln() / ratio.ln()).ceil().min((BINS - 1) as f64) as usize
}

/// The upper edge of bin I in ms.
pub fn bin_edge(i: usize) -> f64 {
    BIN_FIRST_MS * (BIN_LAST_MS / BIN_FIRST_MS).powf(i as f64 / (BINS - 1) as f64)
}

/// Counts per latency [`bin`], written as `[bin, count]` pairs of the bins that hold any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bins(pub [u32; BINS]);

impl Default for Bins {
    fn default() -> Self {
        Bins([0; BINS])
    }
}

impl Serialize for Bins {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.0.iter().enumerate().filter(|(_, n)| **n > 0).map(|(i, n)| (i, *n)))
    }
}

impl<'de> Deserialize<'de> for Bins {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let mut b = Bins::default();
        for (i, n) in Vec::<(usize, u32)>::deserialize(d)? {
            *b.0.get_mut(i).ok_or_else(|| serde::de::Error::custom(format!("no latency bin {i}")))? += n;
        }
        Ok(b)
    }
}

/// The sums of one cell: `input` is the uncached input; a latency mean is its sum over its count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sums {
    pub requests: u64,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub thinking: u64,
    pub failed: u64,
    pub overloaded: u64,
    pub duration_ms: u64,
    pub timed: u64,
    pub ttft_ms: u64,
    pub first_tokens: u64,
    /// How many durations and first tokens fell in each [`bin`].
    #[serde(default)]
    pub duration_bins: Bins,
    #[serde(default)]
    pub ttft_bins: Bins,
}

impl Sums {
    pub fn add(&mut self, o: &Sums) {
        let pairs = [
            (&mut self.requests, o.requests),
            (&mut self.input, o.input),
            (&mut self.output, o.output),
            (&mut self.cache_read, o.cache_read),
            (&mut self.cache_write_5m, o.cache_write_5m),
            (&mut self.cache_write_1h, o.cache_write_1h),
            (&mut self.thinking, o.thinking),
            (&mut self.failed, o.failed),
            (&mut self.overloaded, o.overloaded),
            (&mut self.duration_ms, o.duration_ms),
            (&mut self.timed, o.timed),
            (&mut self.ttft_ms, o.ttft_ms),
            (&mut self.first_tokens, o.first_tokens),
        ];
        for (a, b) in pairs {
            *a += b;
        }
        // only the spreads that hold any: most cells have no latency, and a merge is the query's inner loop
        for (mine, theirs, any) in [(&mut self.duration_bins, &o.duration_bins, o.timed), (&mut self.ttft_bins, &o.ttft_bins, o.first_tokens)] {
            if any > 0 {
                mine.0.iter_mut().zip(theirs.0).for_each(|(a, b)| *a += b);
            }
        }
    }

    /// The sums of one request's latency: its duration and time to first token where it has them.
    pub fn timed(duration_ms: Option<u64>, ttft_ms: Option<u64>) -> Sums {
        let mut s = Sums::default();
        if let Some(d) = duration_ms {
            (s.duration_ms, s.timed) = (d, 1);
            s.duration_bins.0[bin(d)] = 1;
        }
        if let Some(t) = ttft_ms {
            (s.ttft_ms, s.first_tokens) = (t, 1);
            s.ttft_bins.0[bin(t)] = 1;
        }
        s
    }

    /// The share of the prompt read from the cache: cache reads over all input, cached or not; None with no input.
    pub fn cache_hit_rate(&self) -> Option<f64> {
        let all = self.input + self.cache_read + self.cache_write_5m + self.cache_write_1h;
        (all > 0).then(|| self.cache_read as f64 / all as f64)
    }

    pub fn mean_duration_ms(&self) -> Option<f64> {
        (self.timed > 0).then(|| self.duration_ms as f64 / self.timed as f64)
    }

    pub fn mean_ttft_ms(&self) -> Option<f64> {
        (self.first_tokens > 0).then(|| self.ttft_ms as f64 / self.first_tokens as f64)
    }
}

/// The upper edge of the bin that holds the Q share (0 to 1) of BINS' counts; None with none counted.
pub fn quantile_ms(bins: &Bins, q: f64) -> Option<f64> {
    let all: u64 = bins.0.iter().map(|&n| n as u64).sum();
    let mut seen = 0;
    bins.0.iter().position(|&n| {
        seen += n as u64;
        all > 0 && seen as f64 >= q * all as f64
    }).map(bin_edge)
}

/// The provider a model's name shows: OpenCode, agy (Gemini), Codex (OpenAI), Claude, else its first part.
pub fn provider(model: &str) -> String {
    let m = model.to_ascii_lowercase();
    let p = if m.starts_with("opencode") {
        "OpenCode"
    } else if m.starts_with("agy/") || m.starts_with("gemini") {
        "agy"
    } else if m.starts_with("gpt") || m.starts_with("codex") || m.starts_with("o3") || m.starts_with("o4") {
        "Codex"
    } else if m.starts_with("claude") || m == "<synthetic>" {
        "Claude"
    } else {
        return model.split(['/', '-']).next().unwrap_or(model).to_string();
    };
    p.into()
}

/// A field of [`Key`], to split a slice by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Field {
    Provider,
    Account,
    Source,
    Model,
    Agent,
    Project,
}

impl Key {
    pub fn field(&self, f: Field) -> String {
        match f {
            Field::Provider => self.provider.clone(),
            Field::Account => self.account.clone(),
            Field::Source => format!("{:?}", self.source).to_lowercase(),
            Field::Model => self.model.clone(),
            Field::Agent => self.agent.clone(),
            Field::Project => self.project.clone(),
        }
    }
}

/// One five-minute cell, as it travels: its step's start, its key and its sums.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub at: i64,
    pub key: Key,
    pub sums: Sums,
}

/// What a query keeps: for each field the values wanted, none meaning all.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filter {
    pub provider: Vec<String>,
    pub account: Vec<String>,
    pub source: Vec<Source>,
    pub model: Vec<String>,
    pub agent: Vec<String>,
    pub project: Vec<String>,
}

impl Filter {
    pub fn keeps(&self, k: &Key) -> bool {
        let has = |want: &[String], v: &String| want.is_empty() || want.contains(v);
        has(&self.provider, &k.provider)
            && has(&self.account, &k.account)
            && (self.source.is_empty() || self.source.contains(&k.source))
            && has(&self.model, &k.model)
            && has(&self.agent, &k.agent)
            && has(&self.project, &k.project)
    }
}

/// Every value present of each key field, for a filter bar to offer.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facets {
    pub provider: BTreeSet<String>,
    /// Each account with where it was counted.
    pub account: BTreeSet<(String, Source)>,
    pub model: BTreeSet<String>,
    pub agent: BTreeSet<String>,
    pub project: BTreeSet<String>,
}

/// A query's answer: the window's series at its step, the window's totals, and its totals by key.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Slice {
    pub step: i64,
    pub series: Vec<(i64, Sums)>,
    pub totals: Sums,
    pub by: Vec<(Key, Sums)>,
}

/// The rollup: each key once, and the cells of each step by their start and key.
#[derive(Clone, Debug, Default)]
pub struct Rollup {
    keys: Vec<Key>,
    ids: HashMap<Key, u32>,
    cells: [BTreeMap<(i64, u32), Sums>; 3],
}

impl Rollup {
    /// Add SUMS of KEY at AT (ms since the epoch) to its five-minute, hourly and daily cells.
    pub fn add(&mut self, at: i64, key: &Key, sums: &Sums) {
        let id = match self.ids.get(key) {
            Some(&id) => id,
            None => {
                self.keys.push(key.clone());
                self.ids.insert(key.clone(), self.keys.len() as u32 - 1);
                self.keys.len() as u32 - 1
            }
        };
        for (step, cells) in STEPS.iter().zip(&mut self.cells) {
            cells.entry((at.div_euclid(*step) * step, id)).or_default().add(sums);
        }
    }

    /// Every cell of O added to this rollup.
    pub fn merge(&mut self, o: &Rollup) {
        for c in o.cells() {
            self.add(c.at, &c.key, &c.sums);
        }
    }

    /// Its five-minute cells, from which the whole rollup is made again.
    pub fn cells(&self) -> impl Iterator<Item = Cell> + '_ {
        self.cells[0].iter().map(|(&(at, id), &sums)| Cell { at, key: self.keys[id as usize].clone(), sums })
    }

    /// Its five-minute cells from AT on, for a reader that holds the rest.
    pub fn cells_from(&self, at: i64) -> impl Iterator<Item = Cell> + '_ {
        let at = at.div_euclid(STEPS[0]) * STEPS[0];
        self.cells[0].range((at, 0)..).map(|(&(at, id), &sums)| Cell { at, key: self.keys[id as usize].clone(), sums })
    }

    /// The first and last step it holds.
    pub fn span(&self) -> Option<(i64, i64)> {
        Some((self.cells[2].keys().next()?.0, self.cells[0].keys().next_back()?.0))
    }

    /// What F keeps from FROM to TO (ms, TO excluded), in candles of STEP ms (a multiple of five minutes), each the
    /// sum of the longest step's cells that fit in it.
    pub fn query(&self, f: &Filter, from: i64, to: i64, step: i64) -> Slice {
        let (level, step) = level_of(step);
        let kept: Vec<bool> = self.keys.iter().map(|k| f.keeps(k)).collect();
        let mut series: Vec<(i64, Sums)> = Vec::new();
        let mut by = vec![Sums::default(); self.keys.len()];
        // the cells come by their start, so a candle's sums are always the last of the series
        for (&(at, id), s) in self.cells[level].range((from.div_euclid(step) * step, 0)..(to, 0)) {
            if kept[id as usize] {
                let at = at.div_euclid(step) * step;
                match series.last_mut() {
                    Some((last, sums)) if *last == at => sums.add(s),
                    _ => series.push((at, *s)),
                }
                by[id as usize].add(s);
            }
        }
        let mut totals = Sums::default();
        series.iter().for_each(|(_, s)| totals.add(s));
        let mut by: Vec<(Key, Sums)> = by.into_iter().enumerate().filter(|(_, s)| s.requests + s.failed + s.timed > 0).map(|(id, s)| (self.keys[id].clone(), s)).collect();
        by.sort_by(|a, b| a.0.cmp(&b.0));
        Slice { step, series, totals, by }
    }

    /// What F keeps from FROM to TO in candles of STEP ms as one series for each value of FIELD, by value.
    pub fn query_by(&self, f: &Filter, from: i64, to: i64, step: i64, field: Field) -> Vec<(String, Vec<(i64, Sums)>)> {
        let (level, step) = level_of(step);
        let mut by: BTreeMap<String, Vec<(i64, Sums)>> = BTreeMap::new();
        let values: Vec<Option<String>> = self.keys.iter().map(|k| f.keeps(k).then(|| k.field(field))).collect();
        for (&(at, id), s) in self.cells[level].range((from.div_euclid(step) * step, 0)..(to, 0)) {
            let Some(v) = &values[id as usize] else { continue };
            let series = by.entry(v.clone()).or_default();
            let at = at.div_euclid(step) * step;
            match series.last_mut() {
                Some((last, sums)) if *last == at => sums.add(s),
                _ => series.push((at, *s)),
            }
        }
        by.into_iter().collect()
    }

    pub fn facets(&self) -> Facets {
        let mut f = Facets::default();
        for k in &self.keys {
            f.provider.insert(k.provider.clone());
            f.account.insert((k.account.clone(), k.source));
            f.model.insert(k.model.clone());
            f.agent.insert(k.agent.clone());
            f.project.insert(k.project.clone());
        }
        f
    }
}

/// The longest step whose cells a candle of STEP ms is made of, and the candle, at least five minutes and a whole
/// number of them.
fn level_of(step: i64) -> (usize, i64) {
    let step = (step.max(STEPS[0]) / STEPS[0]) * STEPS[0];
    ((0..STEPS.len()).rev().find(|l| step % STEPS[*l] == 0).unwrap_or(0), step)
}

#[cfg(test)]
mod tests;
