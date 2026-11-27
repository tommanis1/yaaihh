//! Cells summed at each step, merged, filtered and sliced as a filter bar asks.

use super::*;

fn key(account: &str, source: Source, model: &str, agent: &str) -> Key {
    Key { provider: provider(model), account: account.into(), source, model: model.into(), agent: agent.into(), project: "research".into() }
}

fn tokens(input: u64, cache_read: u64) -> Sums {
    Sums { requests: 1, input, cache_read, output: 10, ..Sums::timed(Some(900), Some(300)) }
}

const H: i64 = 3_600_000;

#[test]
fn a_request_is_counted_in_its_five_minute_hour_and_day_cells() {
    let mut r = Rollup::default();
    let k = key("acct-a", Source::Native, "claude-opus-5-5", "w1");
    r.add(10 * H + 61_000, &k, &tokens(100, 900));
    r.add(10 * H + 299_000, &k, &tokens(100, 900));
    r.add(10 * H + 301_000, &k, &tokens(100, 900));
    let five = r.query(&Filter::default(), 10 * H, 11 * H, auto_candle(H));
    assert_eq!((five.step, five.series.len(), five.totals.requests), (STEPS[0], 2, 3));
    let mut two = tokens(100, 900);
    two.add(&tokens(100, 900));
    assert_eq!(five.series[0], (10 * H, two));
    assert_eq!((two.requests, two.input, two.cache_read, two.duration_ms, two.timed), (2, 200, 1800, 1800, 2));
    let week = r.query(&Filter::default(), 0, 7 * 24 * H, auto_candle(7 * 24 * H));
    assert_eq!((week.step, week.series, week.totals.requests), (STEPS[1], vec![(10 * H, five.totals)], 3));
    let month = r.query(&Filter::default(), 0, 30 * 24 * H, auto_candle(30 * 24 * H));
    assert_eq!((month.step, month.series, month.totals.requests), (6 * H, vec![(6 * H, five.totals)], 3), "six hours, of hourly cells");
    let quarter = r.query(&Filter::default(), 10 * H, 11 * H, 3 * STEPS[0]);
    assert_eq!((quarter.step, quarter.series, quarter.totals.requests), (15 * 60_000, vec![(10 * H, five.totals)], 3), "a candle picked: a quarter hour of five-minute cells");
    let day = r.query(&Filter::default(), 10 * H, 11 * H, STEPS[2]);
    assert_eq!((day.step, day.series), (STEPS[2], vec![(0, five.totals)]), "a day over an hour: the day it is in");
    assert_eq!(five.totals.cache_hit_rate(), Some(0.9));
    assert_eq!((five.totals.mean_duration_ms(), five.totals.mean_ttft_ms()), (Some(900.0), Some(300.0)));
    assert_eq!(r.span(), Some((0, 10 * H + 300_000)));
}

#[test]
fn native_and_gateway_are_one_rollup_sliced_by_any_field() {
    let mut native = Rollup::default();
    native.add(H, &key("acct-a", Source::Native, "claude-opus-5-5", "w1"), &tokens(10, 0));
    native.add(H, &key("acct-b", Source::Native, "opencode-go/gemini-4-argon", "g1"), &tokens(20, 0));
    let mut gateway = Rollup::default();
    gateway.add(H, &key("gw-1", Source::Gateway, "claude-opus-5-5", "w2"), &tokens(40, 0));
    gateway.add(H, &key("gw-1", Source::Gateway, "gpt-6.2-sol", "c1"), &tokens(80, 0));
    let mut all = Rollup::default();
    all.merge(&native);
    all.merge(&gateway);
    let q = |f: Filter| all.query(&f, 0, 24 * H, STEPS[0]).totals.input;
    assert_eq!(q(Filter::default()), 150);
    assert_eq!(q(Filter { source: vec![Source::Gateway], ..Default::default() }), 120);
    assert_eq!(q(Filter { provider: vec!["Claude".into()], ..Default::default() }), 50);
    assert_eq!(q(Filter { provider: vec!["Claude".into(), "Codex".into()], source: vec![Source::Gateway], ..Default::default() }), 120);
    assert_eq!(q(Filter { agent: vec!["g1".into()], ..Default::default() }), 20);
    assert_eq!(q(Filter { model: vec!["claude-opus-5-5".into()], account: vec!["acct-a".into()], ..Default::default() }), 10);
    let s = all.query(&Filter::default(), 0, 24 * H, STEPS[0]);
    assert_eq!(s.by.len(), 4);
    let facets = all.facets();
    assert_eq!(facets.provider.iter().map(String::as_str).collect::<Vec<_>>(), ["Claude", "Codex", "OpenCode"]);
    assert!(facets.account.contains(&("gw-1".into(), Source::Gateway)) && facets.account.contains(&("acct-a".into(), Source::Native)));
}

#[test]
fn cells_travel_and_make_the_same_rollup_again() {
    let mut r = Rollup::default();
    let k = key("acct-a", Source::Native, "claude-fable-5-2", "w1");
    for i in 0..50 {
        r.add(i * 7 * 60_000, &k, &tokens(i as u64, 1));
    }
    let wire = serde_json::to_string(&r.cells().collect::<Vec<_>>()).unwrap();
    let mut again = Rollup::default();
    for c in serde_json::from_str::<Vec<Cell>>(&wire).unwrap() {
        again.add(c.at, &c.key, &c.sums);
    }
    assert_eq!(again.query(&Filter::default(), 0, 30 * 24 * H, STEPS[2]), r.query(&Filter::default(), 0, 30 * 24 * H, STEPS[2]));
    assert_eq!(r.cells_from(5 * H).count(), r.cells().filter(|c| c.at >= 5 * H).count());
}

#[test]
fn each_model_names_its_provider() {
    let p: Vec<String> = ["claude-haiku-5-5", "<synthetic>", "gpt-6.2-sol", "opencode-go/beam-free", "agy/gemini-3.8-flash", "gemini-4-neon", "mistral-large"].map(provider).into();
    assert_eq!(p, ["Claude", "Claude", "Codex", "OpenCode", "agy", "agy", "mistral"]);
}

/// The real month's shape (research and yaaihh, 26 Nov: 255,877 rows in 19,405 five-minute cells over 84 keys, 26
/// days): every window with a filter of each field is read in under five milliseconds, optimised as shipped; times printed.
#[test]
fn a_month_of_real_shape_is_sliced_in_under_five_milliseconds() {
    let keys: Vec<Key> = (0..84).map(|i| key(["acct-a", "acct-team", "acct-b"][i % 3], if i % 7 == 0 { Source::Gateway } else { Source::Native }, ["claude-opus-5-5", "claude-fable-5-2", "opencode-go/gemini-4-argon", "gpt-6.2-sol"][i % 4], &format!("a{}", i % 28))).collect();
    let mut r = Rollup::default();
    let (start, five) = (1_793_491_200_000i64, STEPS[0]);
    for c in 0..19_405i64 {
        r.add(start + (c * 10 / 26) * five, &keys[(c * 31 % 84) as usize], &tokens(c as u64 % 900, 9000));
    }
    let end = r.span().unwrap().1 + five;
    let filters = [Filter::default(), Filter { provider: vec!["Claude".into()], ..Default::default() }, Filter { account: vec!["acct-a".into()], source: vec![Source::Native], ..Default::default() }, Filter { agent: vec!["a3".into(), "a9".into()], model: vec!["claude-opus-5-5".into()], ..Default::default() }];
    for (name, span) in [("1h", 3_600_000), ("24h", 86_400_000), ("7d", 7 * 86_400_000), ("30d", 30 * 86_400_000), ("All", end - start)] {
        for f in &filters {
            let t = std::time::Instant::now();
            let s = r.query(f, end - span, end, auto_candle(span));
            let took = t.elapsed();
            println!("{name:>3} {:>4} points {:>9.3} ms  filter {:?}", s.series.len(), took.as_secs_f64() * 1000.0, (f.provider.len(), f.account.len(), f.agent.len(), f.model.len()));
            assert!(took.as_millis() < 5, "{name}: {took:?}");
        }
    }
}

#[test]
fn latency_is_kept_as_a_spread_and_a_slice_splits_by_any_field() {
    assert_eq!((bin(0), bin(50), bin(51), bin(600_000), bin(u64::MAX)), (0, 0, 1, BINS - 1, BINS - 1));
    let mut s = Sums::default();
    for d in [100, 120, 150, 200, 9000] {
        s.add(&Sums::timed(Some(d), None));
    }
    let p50 = quantile_ms(&s.duration_bins, 0.5).unwrap();
    assert!((150.0..300.0).contains(&p50), "{p50}");
    assert!(quantile_ms(&s.duration_bins, 0.95).unwrap() >= 9000.0);
    assert_eq!((quantile_ms(&s.ttft_bins, 0.5), s.first_tokens), (None, 0));
    let line = serde_json::to_string(&s.duration_bins).unwrap();
    assert!(line.starts_with("[[") && line.matches('[').count() <= BINS + 1, "only the bins that hold any: {line}");
    assert_eq!(serde_json::from_str::<Bins>(&line).unwrap(), s.duration_bins);
    assert!((1..BINS).all(|i| bin_edge(i) / bin_edge(i - 1) < 1.17), "steps of under 17%");
    let mut r = Rollup::default();
    r.add(H, &key("acct-a", Source::Native, "claude-opus-5-5", "w1"), &tokens(10, 90));
    r.add(2 * H, &key("acct-a", Source::Native, "claude-opus-5-5", "w1"), &tokens(10, 0));
    r.add(H, &key("gw-1", Source::Gateway, "claude-opus-5-5", "w2"), &tokens(50, 50));
    let by = r.query_by(&Filter::default(), 0, 24 * H, STEPS[0], Field::Account);
    assert_eq!(by.iter().map(|(v, s)| (v.as_str(), s.len())).collect::<Vec<_>>(), [("gw-1", 1), ("acct-a", 2)]);
    assert_eq!(by[1].1[0].1.cache_hit_rate(), Some(0.9));
    assert_eq!(r.query_by(&Filter::default(), 0, 24 * H, STEPS[0], Field::Source).iter().map(|(v, _)| v.as_str()).collect::<Vec<_>>(), ["gateway", "native"]);
}
