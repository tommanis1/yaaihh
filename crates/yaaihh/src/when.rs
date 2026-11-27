//! One way to write a time (paper-research N-797.p1, do 2; his to change): `14:02` today, `Tue 14:02` within the last
//! six days, `18 Nov 14:02` before that, the year only when it is not this year; local time. Where a time is a text of
//! its own, the full date and time is on its hover ([`el`]).

use chrono::{DateTime, Datelike, Local, TimeZone, Utc};
use rune::{El, text};

/// The time of day to the minute, as every time is written.
const MINUTE: &str = "%H:%M";
/// To the second: a line of the record, a few seconds from the next ([`exact`]).
const SECOND: &str = "%H:%M:%S";
/// To the minute in UTC, as an agent reads every time ([`utc`]).
const MINUTE_Z: &str = "%H:%MZ";

/// T as he reads it, NOW being now, its time of day as CLOCK writes it: a date as [`date`] writes it, with no comma
/// before the time ("21 Nov 13:11", "9 Mar 2027 08:59"; paper-research I-1469 p50). On his clock, or UTC's for what an
/// agent reads ([`utc`]).
fn written<Tz: TimeZone>(t: DateTime<Tz>, now: DateTime<Tz>, clock: &str) -> String
where
    Tz::Offset: std::fmt::Display + Copy,
{
    let clock = t.format(clock);
    match (now.date_naive() - t.date_naive()).num_days() {
        0 => clock.to_string(),
        1..=6 => format!("{} {clock}", t.format("%a")),
        _ => format!("{} {clock}", date(t, now)),
    }
}

/// T's date, NOW being now: "21 Nov", the year only when it is not this year ("9 Mar 2027").
fn date<Tz: TimeZone>(t: DateTime<Tz>, now: DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display + Copy,
{
    t.format(if t.year() == now.year() { "%-d %b" } else { "%-d %b %Y" }).to_string()
}

fn parse(at: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(at).ok().map(|t| t.with_timezone(&Local))
}

/// AT (RFC 3339; coordd writes UTC) as he reads it; words that are no time stay as they are.
pub fn at(at: &str) -> String {
    at_on(at, Local::now())
}

/// AT as [`at`] writes it, NOW being now: for words made once and kept, written as of the time they are made (the
/// Map's notes, made again at each midnight: [`next_day`]).
pub fn at_on(at: &str, now: DateTime<Local>) -> String {
    parse(at).map_or_else(|| at.to_string(), |t| written(t, now, MINUTE))
}

/// AT as [`at`] writes it, to the second: the Events layer's lines, on his clock as his posts are and not the record's
/// UTC (proto-reader N-1446 p163: p587 at 19:23:17, beside his post's 19:23).
pub fn exact(at: &str) -> String {
    parse(at).map_or_else(|| at.to_string(), |t| written(t, Local::now(), SECOND))
}

/// AT (RFC 3339) as an agent reads every time, in UTC with a Z (paper-research N-1446 p2361): as [`at`] writes it, its
/// day by UTC's ("06:33Z", "Mon 06:33Z", "23 Nov 06:33Z"). A comment's label, which goes with its quote to the agent.
pub fn utc(at: &str) -> String {
    utc_on(at, Utc::now())
}

/// AT as [`utc`] writes it, NOW being now.
pub fn utc_on(at: &str, now: DateTime<Utc>) -> String {
    DateTime::parse_from_rfc3339(at).map_or_else(|_| at.to_string(), |t| written(t.with_timezone(&Utc), now, MINUTE_Z))
}

/// AT (seconds since 1970) as he reads it.
pub fn secs(at: i64) -> String {
    secs_on(at, Local::now())
}

/// AT (seconds since 1970) as [`secs`] writes it, NOW being now ([`at_on`]).
pub fn secs_on(at: i64, now: DateTime<Local>) -> String {
    DateTime::from_timestamp(at, 0).map_or_else(String::new, |t| written(t.with_timezone(&Local), now, MINUTE))
}

/// NOW (seconds since 1970) on his clock.
pub fn local(now: i64) -> DateTime<Local> {
    DateTime::from_timestamp(now, 0).map_or_else(Local::now, |t| t.with_timezone(&Local))
}

/// When the local day after NOW (seconds since 1970) starts, in seconds: where the words [`at`] writes of a time change
/// by themselves ("14:02" becomes "Tue 14:02", a weekday a date, a date gains its year), so words made once are made
/// again there (map/join.rs `next_change`; consult-3 N-1479 p184). Its first hour on the clock, where a change of the
/// clocks skips midnight.
pub fn next_day(now: i64) -> Option<i64> {
    let day = local(now).date_naive().succ_opt()?;
    (0..3).find_map(|h| day.and_hms_opt(h, 0, 0)?.and_local_timezone(Local).earliest()).map(|t| t.timestamp())
}

/// The date AT (seconds since 1970) falls on, as [`secs`] writes a date: what has a day and no time of its own (a
/// reset's "use by 27 Nov", the day a key was set).
pub fn day(at: i64) -> String {
    DateTime::from_timestamp(at, 0).map_or_else(String::new, |t| date(t.with_timezone(&Local), Local::now()))
}

/// SECS of time taken in [`span`]'s words ("7 m", "1 h 05 m", "2 d 04 h"), but rounded down and in seconds under a
/// minute ("42 s"): time taken is not time left (consult-3 I-1469 p30, proto-lead p33). A time since is written with it
/// (`ask::ago`, `streams::ran`), and a clock's period (`picture::start_words`): every duration in the window one way
/// (paper-research I-1469 p42, p50: "not use different formats").
pub fn took(secs: i64) -> String {
    match secs.max(0) {
        s @ ..60 => format!("{s} s"),
        s => minutes(s / 60),
    }
}

/// How long SECS is, in the one way the window writes a duration, his card's and the gateway's alike (the owner,
/// N-1104 p678: "keep the d h m breakdown on the accounts tab everywhere"; [`took`] too): from a day up days and hours
/// ("4 d 18 h"), from an hour hours and minutes ("2 h 05 m"), else minutes ("12 m"); the second part in two digits,
/// and left out when it is zero ("5 h", "7 d"). A part of a minute counts as a whole one, so time left reads `0 m`
/// only when none is.
pub fn span(secs: i64) -> String {
    minutes((secs.max(0) + 59) / 60)
}

/// M whole minutes in [`span`]'s words: the one writer of a duration, [`span`] rounding up to them and [`took`] down.
fn minutes(m: i64) -> String {
    let (big, small, units) = match m {
        ..60 => return format!("{m} m"),
        ..1440 => (m / 60, m % 60, ["h", "m"]),
        _ => (m / 1440, m % 1440 / 60, ["d", "h"]),
    };
    match small {
        0 => format!("{big} {}", units[0]),
        _ => format!("{big} {} {small:02} {}", units[0], units[1]),
    }
}

/// AT in full, for a hover: "Thursday 19 November 2026, 14:02:33".
pub fn full(at: &str) -> String {
    parse(at).map_or_else(|| at.to_string(), |t| t.format("%A %-d %B %Y, %H:%M:%S").to_string())
}

/// From A to B: "Tue 14:02–14:10" on one day, else both whole.
pub fn range(a: &str, b: &str) -> String {
    let (wa, wb) = (at(a), at(b));
    match (parse(a), parse(b)) {
        _ if wa == wb => wa,
        (Some(x), Some(y)) if x.date_naive() == y.date_naive() => format!("{wa}–{}", y.format("%H:%M")),
        _ => format!("{wa}–{wb}"),
    }
}

/// AT as a text of its own, the full date and time on its hover.
pub fn el(at: &str) -> El {
    text(self::at(at)).help(full(at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn the_next_day_starts_at_the_next_local_midnight() {
        let at = |d, h, mi, s| Local.with_ymd_and_hms(2026, 11, d, h, mi, s).unwrap().timestamp();
        assert_eq!(next_day(at(21, 18, 30, 0)), Some(at(22, 0, 0, 0)));
        assert_eq!(next_day(at(21, 23, 59, 59)), Some(at(22, 0, 0, 0)));
        assert_eq!(next_day(at(22, 0, 0, 0)), Some(at(23, 0, 0, 0)), "at midnight, the next one");
        // the words of a time of the day before, as of a second before and at that midnight
        let late = Local.with_ymd_and_hms(2026, 11, 21, 23, 30, 0).unwrap();
        assert_eq!(secs_on(late.timestamp(), local(at(21, 23, 59, 59))), "23:30");
        assert_eq!(secs_on(late.timestamp(), local(at(22, 0, 0, 0))), "Sat 23:30");
    }

    #[test]
    fn a_time_is_written_by_how_long_ago_it_was() {
        let now = Local.with_ymd_and_hms(2026, 11, 19, 9, 30, 0).unwrap();
        let t = |y, m, d, h, mi| Local.with_ymd_and_hms(y, m, d, h, mi, 0).unwrap();
        assert_eq!(written(t(2026, 11, 19, 0, 5), now, MINUTE), "00:05");
        assert_eq!(written(t(2026, 11, 18, 14, 2), now, MINUTE), "Wed 14:02");
        assert_eq!(written(t(2026, 11, 13, 14, 2), now, MINUTE), "Fri 14:02");
        assert_eq!(written(t(2026, 11, 12, 14, 2), now, MINUTE), "12 Nov 14:02");
        assert_eq!(written(t(2025, 12, 31, 23, 59), now, MINUTE), "31 Dec 2025 23:59");
        // a time to come (a limit's reset) is no day of the week
        assert_eq!(written(t(2026, 11, 20, 8, 0), now, MINUTE), "20 Nov 08:00");
        assert_eq!(at("not a time"), "not a time");
        // a line of the record, to the second, its day as any time's
        let s = |d, h, mi, sec| Local.with_ymd_and_hms(2026, 11, d, h, mi, sec).unwrap();
        assert_eq!(written(s(19, 9, 23, 17), now, SECOND), "09:23:17");
        assert_eq!(written(s(18, 19, 23, 32), now, SECOND), "Wed 19:23:32");
        assert_eq!(exact("not a time"), "not a time");
    }

    /// One way to write a date on the Accounts tab, his card's and the gateway's (paper-research I-1469 p36, p50): no
    /// comma before the time, the year only when it is not this year; a date with no time of its own is the same date.
    #[test]
    fn a_date_is_day_and_month_with_the_year_only_when_not_this_year() {
        let now = Local.with_ymd_and_hms(2026, 11, 21, 9, 30, 0).unwrap();
        let t = |y, m, d, h, mi| Local.with_ymd_and_hms(y, m, d, h, mi, 0).unwrap();
        assert_eq!(written(t(2026, 11, 28, 13, 11), now, MINUTE), "28 Nov 13:11", "a reset to come");
        assert_eq!(written(t(2027, 3, 9, 8, 59), now, MINUTE), "9 Mar 2027 08:59", "not 03/09/2027, 08:59");
        assert_eq!((date(t(2026, 11, 27, 23, 59), now), date(t(2027, 1, 3, 23, 59), now)), ("27 Nov".into(), "3 Jan 2027".into()));
        assert_eq!(date(t(2026, 11, 21, 0, 5), now), "21 Nov", "today too: a day with no time is its date");
        let at = t(2026, 11, 27, 23, 59).timestamp();
        assert_eq!(day(at), date(t(2026, 11, 27, 23, 59), Local::now()));
        // on the real clock: "23:59" on that day itself, "Fri 23:59" or "27 Nov 23:59" around it (proto-window p86)
        assert!(secs(at).ends_with("23:59") && !secs(at).contains(','), "{}", secs(at));
    }

    /// A time an agent reads, a comment's label's (paper-research N-1446 p2361; proto-mind p2359: "paper-research's
    /// message at 07:33" for a post at 06:33:37Z): UTC with a Z, its day by UTC's, whatever his clock says.
    #[test]
    fn a_time_an_agent_reads_is_utc_with_a_z() {
        let now = Utc.with_ymd_and_hms(2026, 11, 24, 9, 0, 0).unwrap();
        assert_eq!(utc_on("2026-11-24T06:33:37Z", now), "06:33Z");
        assert_eq!(utc_on("2026-11-24T07:33:37+01:00", now), "06:33Z", "an offset's time read in UTC");
        assert_eq!(utc_on("2026-11-23T23:59:00Z", now), "Mon 23:59Z", "its day by UTC's");
        assert_eq!(utc_on("2026-11-12T14:02:00Z", now), "12 Nov 14:02Z");
        assert_eq!(utc_on("not a time", now), "not a time");
    }

    /// N-1104 p678: 114 hours is "4 d 18 h", as every duration on the Accounts tab is written.
    #[test]
    fn a_duration_is_days_and_hours_hours_and_minutes_or_minutes() {
        let (h, m) = (3600, 60);
        assert_eq!(span(114 * h), "4 d 18 h");
        assert_eq!(span(114 * h + 59 * m), "4 d 18 h", "a day up: no minutes");
        assert_eq!((span(2 * h + 5 * m), span(12 * m), span(5 * h), span(7 * 24 * h)), ("2 h 05 m".into(), "12 m".into(), "5 h".into(), "7 d".into()));
        assert_eq!((span(4 * 24 * h + 8 * h), span(24 * h - 1)), ("4 d 08 h".into(), "1 d".into()));
        assert_eq!((span(1), span(59), span(0), span(-30)), ("1 m".into(), "1 m".into(), "0 m".into(), "0 m".into()), "a part of a minute is a minute");
    }

    /// Every duration in the window in span's words (paper-research I-1469 p42, p50: "not use different formats"); a
    /// time taken and a time since rounded down and in seconds under a minute, as time left is not (consult-3 p30).
    #[test]
    fn a_time_taken_or_since_is_written_as_time_left_is_but_rounded_down() {
        let (h, m) = (3600, 60);
        assert_eq!((took(7 * m), took(7 * m + 59), took(2 * h)), ("7 m".into(), "7 m".into(), "2 h".into()));
        assert_eq!((took(h + 5 * m + 59), span(h + 5 * m + 59)), ("1 h 05 m".into(), "1 h 06 m".into()), "down, where time left goes up");
        assert_eq!((took(2 * 24 * h + 4 * h + 59 * m), took(114 * h)), ("2 d 04 h".into(), span(114 * h)));
        assert_eq!((took(42), took(0), took(-5)), ("42 s".into(), "0 s".into(), "0 s".into()), "seconds under a minute, none below 0");
        // a time since: just now under a minute, else as a time taken
        let ago = |secs: i64| crate::ask::ago(&(chrono::Utc::now() - chrono::Duration::seconds(secs)).to_rfc3339());
        assert_eq!((ago(30), ago(5 * m + 10), ago(3 * h + 5 * m + 30), ago(2 * 24 * h + 4 * h + 59 * m)), ("just now".into(), "5 m ago".into(), "3 h 05 m ago".into(), "2 d 04 h ago".into()));
    }
}
