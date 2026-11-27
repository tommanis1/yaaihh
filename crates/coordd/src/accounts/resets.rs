//! The limit resets Claude Code offers the owner's sign-in accounts (the card's reset `Use`), in Claude Code's own words
//! and through Claude Code alone. A reset is used only by his press of `Use` on an account's card; nothing of coordd's
//! sends `/limit-reset` by itself. What is here reads screens; it types nothing.
//!
//! - **Offers** ([`offers`]). When a turn meets its account's limit, Claude Code's screen shows a notice under it (its
//!   `noticeLine`, 2.1.329): `/limit-reset to use your Fable reset · 2 left · use by Nov 27`, or, with resets of
//!   several kinds, `/limit-reset · 2 Fable, 1 five-hour resets · first expires Nov 27` (`noticeLineMany`). coordd
//!   reads the agent's screen then (`Job::ResetOffers`, before the agent moves) and keeps the offers in the account's
//!   state; the card gets them while the account is out for its limit ([`offered`]). Print mode's `/usage` lists none.
//! - **The press** ([`argument`], [`shown`]). `Use` starts Claude Code alone on the account's folder, on a terminal of
//!   its own ([`session`], held by roster `account`; main.rs `account_reset`), types `/limit-reset ARGUMENT`, answers
//!   Claude Code's own
//!   confirm (`Use a Fable reset?` · `Yes, use my Fable reset`), and keeps what Claude Code says of how it went: its
//!   `successLine`, or its refusal in its words.
//! - **The look** ([`look`]). His `Check limits` on a sign-in card starts Claude Code there the same way and types
//!   [`LOOK`], with no kind: once it lists what the account holds, the list is read ([`offers`]) and Escape answers it,
//!   so nothing is chosen or used; the list is kept as a notice's is ([`noticed`]) and the card shows it ([`held`]). An
//!   answer instead of a list ends the look and says whether none is held, some are that it does not list, or nothing
//!   could be checked ([`at_rest`], [`looked`]). Only `Check limits` starts one ([`looks_on_check`]).
//!
//! Claude Code's server may word all of it otherwise (its `tengu_cedar_ember` texts); a screen read here is matched on
//! the words of 2.1.329 through 2.1.332, and one that says nothing known within the session's time is said as it stands.
//! With those kinds off for an account, `/limit-reset` takes no argument and runs its one 5-hour reset a week instead
//! (`tengu_nifty_lemur`): at a 5-hour limit it is used at once, with no confirm; at none it says "A 5-hour limit reset
//! isn't available right now." Its lines are matched too.

use super::{limits, signin, State};
use coord_types::{ResetOffer, ResetTry};
use std::ffi::OsString;
use std::path::Path;

/// How long a press's session is given to say how it went (seconds); a press with no outcome after it counts as ended
/// with none ([`shown_try`]), and another may be made. A look is given as long.
pub const SESSION: i64 = 90;

/// What the look types ([`look`]): Claude Code's command with no kind, which lists what the account holds.
pub const LOOK: &str = "/limit-reset";

/// The title of `/limit-reset`'s chooser (its `chooseTitle`, 2.1.329).
const CHOOSE: &str = "Which reset do you want to use?";

/// Claude Code's `/limit-reset` argument for KIND, a reset in its words (2.1.329's names, `ee`: "5-hour reset",
/// "Fable reset", "full reset", "weekly reset"; the command takes `fable`, `full`, `5-hour` or `weekly`).
pub fn argument(kind: &str) -> Option<&'static str> {
    match kind {
        "5-hour reset" => Some("5-hour"),
        "Fable reset" => Some("fable"),
        "full reset" => Some("full"),
        "weekly reset" => Some("weekly"),
        _ => None,
    }
}

/// The command of a press's session: PROGRAM on the account folder CONFIG in the environment of signin.rs's
/// [`signin::command`] and nothing else (no hook, setting, MCP server or channel of an agent's, no gateway), with the
/// terminal's own two words; for roster `account`, which holds it on a terminal of its own.
pub fn session(program: &str, config: &Path) -> Vec<OsString> {
    let set = signin::env(config).into_iter().map(|(k, v)| {
        let mut word = OsString::from(format!("{k}="));
        word.push(v);
        word
    });
    ["env", "-i", "TERM=xterm-256color", "LANG=C.UTF-8"].map(OsString::from).into_iter().chain(set).chain([program, "--strict-mcp-config"].map(OsString::from)).collect()
}

/// The resets SCREEN names, read at NOW: the notice of a turn at its limit, or `/limit-reset`'s own list as the look
/// shows it ([`listed`]); None when neither is on it. Each kind in Claude Code's single word for it ([`argument`]), so
/// every form names it alike ("five-hour" of the many form is "5-hour reset"). A "use by" date is the end of that day
/// on this machine's clock, as the agent's Claude Code wrote it; the many form names only its first expiry, given to
/// none.
pub fn offers(screen: &str, now: i64) -> Option<Vec<ResetOffer>> {
    notice(screen, now).or_else(|| listed(screen, now))
}

/// The resets the notice on SCREEN offers ([`offers`]).
fn notice(screen: &str, now: i64) -> Option<Vec<ResetOffer>> {
    for line in screen.lines() {
        if let Some((_, rest)) = line.split_once("/limit-reset to use your ") {
            let parts: Vec<&str> = rest.split(DOT).map(str::trim).collect();
            let [kind, left, by, ..] = parts[..] else { continue };
            let left = left.strip_suffix(" left").and_then(|n| n.parse().ok());
            let use_by = by.strip_prefix("use by ").and_then(|d| day_end(d, now));
            return Some(vec![ResetOffer { kind: kind.to_string(), left, use_by, ..Default::default() }]);
        }
        if let Some((_, rest)) = line.split_once(FIVE_HOUR_NOTICE) {
            let gives = rest.trim().strip_prefix(DOT.trim_start()).map(|g| g.trim().to_string()).filter(|g| !g.is_empty());
            return Some(vec![ResetOffer { kind: "5-hour reset".into(), gives, ..Default::default() }]);
        }
        if let Some((_, rest)) = line.split_once("/limit-reset").filter(|(_, r)| r.starts_with(DOT)) {
            let Some(held) = rest[DOT.len()..].split(DOT).next().and_then(|h| h.trim().strip_suffix(" resets")) else { continue };
            let found: Vec<ResetOffer> = held
                .split(", ")
                .filter_map(|h| {
                    let (n, kind) = h.split_once(' ')?;
                    let kind = if kind == "five-hour" { "5-hour".to_string() } else { kind.to_string() };
                    Some(ResetOffer { kind: format!("{kind} reset"), left: n.parse().ok(), ..Default::default() })
                })
                .collect();
            return Some(found);
        }
    }
    None
}

/// What `/limit-reset`'s own SCREEN lists ([`offers`]): its chooser's choices, or the confirm of the one kind it
/// holds; None when neither stands (2.1.330's words, `eb` and its chooser's options).
/// - A choice it can use now is its `label`, `Use a Fable reset (2 left, until Nov 27)`, with its note after a dot when
///   it has one (`· frees up only what Fable used this week`, kept as [`ResetOffer::gives`]).
/// - One it cannot use at this limit is drawn `Full reset · can't be used while you're at your 5-hour limit` (the kind
///   with a capital, its reason after the dot: "doesn't refill your weekly limit", "usable once you reach a limit it
///   refills", "try again in a minute", ...): kept with that reason as [`ResetOffer::why`], with no count or date,
///   which it does not draw.
/// - The confirm: its title `Use a Fable reset?` over its body, `Refills your Fable and 5-hour limit now · returns what
///   Fable used to your weekly limit · your weekly reset day stays Monday` (kept as `gives` without the weekly day,
///   with the lines it wraps onto), and its `confirmDetail`, `2 Fable resets left · use by Nov 27, 3pm`.
fn listed(screen: &str, now: i64) -> Option<Vec<ResetOffer>> {
    let lines: Vec<String> = screen.lines().map(words).collect();
    if screen.contains(CHOOSE) {
        let choice = |w: &String| {
            let Some(label) = w.strip_prefix("Use a ") else { return unusable(w) };
            let (label, note) = label.split_once(DOT).map_or((label, None), |(l, n)| (l, Some(n.trim().to_string())));
            let (kind, said) = label.split_once(" (").unwrap_or((label, ""));
            let left = said.split_once(" left").and_then(|(n, _)| n.parse().ok());
            let use_by = said.split_once("until ").and_then(|(_, d)| day_end(d.trim_end_matches(')'), now));
            Some(ResetOffer { kind: kind.to_string(), left, use_by, gives: note.filter(|n| !n.is_empty()), ..Default::default() })
        };
        return Some(lines.iter().filter_map(choice).collect());
    }
    if !screen.contains("Yes, use my ") {
        return None;
    }
    let kind = lines.iter().find_map(|w| w.strip_prefix("Use a ")?.strip_suffix('?'))?;
    // its detail is the line that begins with the count (its early line, "… left — use your Fable reset anyway?", does not)
    let is_detail = |w: &String| w.contains(" left") && w.split(' ').next().is_some_and(|n| n.parse::<u32>().is_ok());
    let detail = lines.iter().find(|w| is_detail(w));
    let left = detail.and_then(|d| d.split(' ').next()?.parse().ok());
    let use_by = detail.and_then(|d| day_end(d.split_once("use by ")?.1, now));
    // the body from its first line through those it wraps onto, up to the detail or a blank line
    let body = lines.iter().position(|w| w.starts_with("Refills your ")).map(|i| {
        let wrapped = lines[i + 1..].iter().take_while(|w| !w.is_empty() && !is_detail(w) && !w.contains("Yes, use my "));
        std::iter::once(&lines[i]).chain(wrapped).map(String::as_str).collect::<Vec<_>>().join(" ")
    });
    let gives = body.map(|b| b.split(DOT).map(str::trim).filter(|p| !p.starts_with("your weekly reset day stays")).collect::<Vec<_>>().join(DOT));
    Some(vec![ResetOffer { kind: kind.to_string(), left, use_by, gives, ..Default::default() }])
}

/// A chooser's choice W that it cannot use at this limit, `Full reset · can't be used while you're at your 5-hour
/// limit` ([`listed`]): its kind in Claude Code's word (`full reset`; the drawing begins it with a capital) and its
/// reason. None for any other line: the kind must be one `/limit-reset` takes ([`argument`]), and a reason must follow.
fn unusable(w: &str) -> Option<ResetOffer> {
    let (label, why) = w.split_once(DOT)?;
    let mut first = label.chars();
    let lower = first.next().map(|c| c.to_lowercase().chain(first).collect::<String>())?;
    let kind = [label.to_string(), lower].into_iter().find(|k| argument(k).is_some())?;
    let why = why.trim();
    (!why.is_empty()).then(|| ResetOffer { kind, why: Some(why.to_string()), ..Default::default() })
}

/// The notice of the 5-hour reset at its limit (`tengu_nifty_lemur`'s `noticeLine`): `/limit-reset to reset your 5-hour
/// limit now · uses weekly limit · 1/week`, what follows kept as [`ResetOffer::gives`].
const FIVE_HOUR_NOTICE: &str = "/limit-reset to reset your 5-hour limit now";

/// The kind of a count at rest that names none (`· 2 resets left ·`, [`at_rest`]): shown, but no kind `/limit-reset`
/// takes ([`argument`]), so never offered.
pub const ANY_KIND: &str = "limit reset";

/// Claude Code's dot between the parts of a line (" · ").
const DOT: &str = " \u{b7} ";

/// The end of the day WORDS names ("Nov 27", "Jan 3, 2027": Claude Code's short date, the year only when not this
/// one; an hour and a zone after it, "Nov 27, 3pm (UTC)", as its confirm says a "use by", are not read), on this
/// machine's clock at NOW: seconds.
fn day_end(words: &str, now: i64) -> Option<i64> {
    use chrono::{Datelike, TimeZone};
    let words = words.trim();
    let mut parts = words.split_once(" (").map_or(words, |(w, _)| w).split(", ");
    let date = parts.next()?;
    let year = parts.next().and_then(|y| y.parse::<i32>().ok());
    let year = year.unwrap_or_else(|| chrono::Local.timestamp_opt(now, 0).single().map_or(1970, |t| t.year()));
    let next = limits::short_date(date, year)?.succ_opt()?.and_hms_opt(0, 0, 0)?;
    Some(chrono::Local.from_local_datetime(&next).earliest()?.timestamp() - 1)
}

/// The kinds a sign-in account at its limit is offered when no notice of Claude Code's named its resets: those of the
/// owner's Max accounts (`Use Fable reset`, `Use full reset`).
pub const AT_A_LIMIT: [&str; 2] = ["Fable reset", "full reset"];

/// When the limit sign-in account STATE is at, at NOW, ends (seconds): while it is out for its rate limit, its
/// `out_until`; otherwise the last to start again of the windows last read used up. None: at no limit a reset helps
/// (out for its login or its credit, whatever its windows said, or at none).
pub fn limit_end(state: &State, now: i64) -> Option<i64> {
    let used_up = || [&state.five_hour, &state.seven_day].into_iter().flatten().filter(|w| w.used_percent >= 100.0 && w.resets_at > now).map(|w| w.resets_at).max();
    match (state.out_until.filter(|t| *t > now), state.out_for.as_deref()) {
        (Some(_), Some(why)) if why != limits::RATE_LIMIT => None,
        (Some(t), Some(_)) => Some(t),
        _ => used_up(),
    }
}

/// The offers of sign-in account STATE for its card at NOW: only at its limit ([`limit_end`]), none past its "use by",
/// none of a kind refused at this limit ([`State::resets_refused`]). The kinds `/limit-reset` takes ([`argument`]) that
/// the last notice or look named, else [`AT_A_LIMIT`] with no count or date; Claude Code's own confirm then says what
/// it holds, and refuses a kind it does not hold in its words ([`shown`]).
pub fn offered(state: &State, now: i64) -> Vec<ResetOffer> {
    if limit_end(state, now).is_none() {
        return Vec::new();
    }
    if state.resets.iter().any(|o| argument(&o.kind).is_some()) {
        return held(state, now).into_iter().filter(|o| argument(&o.kind).is_some()).collect();
    }
    AT_A_LIMIT.iter().map(|k| ResetOffer { kind: k.to_string(), ..Default::default() }).filter(|o| stands(state, o, now)).collect()
}

/// The resets Claude Code last named for sign-in account STATE (a notice at its limit, or the look's list), for its
/// card at NOW at any time: none past its "use by", and none of a kind refused at this limit. Why one cannot be used
/// ([`ResetOffer::why`]) is given only while it holds: it was said at a limit ([`listed`]; at none `/limit-reset`
/// answers in words instead), and holds while that limit lasts, so it goes when the account is at none, or when it
/// was said before the time it went out at this one.
pub fn held(state: &State, now: i64) -> Vec<ResetOffer> {
    let said_at_this_limit = limit_end(state, now).is_some() && state.resets_seen.is_some_and(|seen| state.out_since.is_none_or(|since| seen >= since));
    state.resets.iter().filter(|o| stands(state, o, now)).map(|o| ResetOffer { why: o.why.clone().filter(|_| said_at_this_limit), ..o.clone() }).collect()
}

/// Whether offer O of STATE still stands at NOW: not past its "use by", and not of a kind Claude Code did not apply at
/// this limit ([`State::resets_refused`]).
fn stands(state: &State, o: &ResetOffer, now: i64) -> bool {
    o.use_by.is_none_or(|t| t > now) && !state.resets_refused.iter().any(|(kind, until)| *kind == o.kind && *until > now)
}

/// What the card says of a press whose session ended, or ran out of time, after `/limit-reset` was typed and before
/// Claude Code said how it went; the windows read again after it show whether it was used.
pub const MAY_BE_USED: &str = "it may have been used: see the windows above";

/// How a press went ([`went`]), in words.
#[derive(Debug, Clone, PartialEq)]
pub enum Went {
    /// Applied: Claude Code's words.
    Done(String),
    /// Not applied, as Claude Code said (or a question of its answered no, [`step`]): why.
    Refused(String),
    /// Not known to be used, and to be tried again: Claude Code said so ([`again`]), or said nothing coordd knows
    /// (its session did not start, its terminal ended, nothing known within [`SESSION`]): why.
    Unknown(String),
}

impl Went {
    /// A press's session's end ([`Step::End`]): Claude Code's words once applied, or why not, which say whether to
    /// try again ([`again`]).
    pub fn of(outcome: Result<String, String>) -> Went {
        match outcome {
            Ok(words) => Went::Done(words),
            Err(why) if again(&why) => Went::Unknown(why),
            Err(why) => Went::Refused(why),
        }
    }
}

/// How the owner's press at SINCE went (`reset_session`, main.rs), kept in STATE at NOW: Claude Code's words once it
/// was applied, or why it was not. A kind refused is offered no more at this limit ([`State::resets_refused`]); one
/// not known to be used stays offered, and the card says `Try again` ([`ResetTry::again`]). A later press's state is
/// left as it is.
pub fn went(state: &mut State, since: i64, outcome: Went, now: i64) {
    let Some(r) = state.reset.as_mut().filter(|r| r.since == since) else { return };
    match outcome {
        Went::Done(words) => r.done = Some(words),
        Went::Unknown(why) => (r.refused, r.again) = (Some(why), true),
        Went::Refused(why) => {
            let kind = r.kind.clone();
            r.login = login(&why);
            r.refused = Some(why);
            if let Some(end) = limit_end(state, now) {
                state.resets_refused.retain(|(k, _)| *k != kind);
                state.resets_refused.push((kind, end));
            }
        }
    }
}

/// The resets Claude Code named at NOW ([`offers`]: the notice on an agent's screen at its limit, or the look's list),
/// kept in STATE as what it holds. A list that names any says again what it holds: the kinds refused before are
/// offered again if it names them, and the card says since when it is known ([`State::resets_seen`]).
pub fn noticed(state: &mut State, offers: Vec<ResetOffer>, now: i64) {
    if !offers.is_empty() {
        state.resets_refused.clear();
        state.resets_seen = Some(now);
    }
    state.resets = offers;
}

/// Whether WHY, Claude Code's word on a press that applied nothing, says to try again: it could not confirm the reset
/// went through ("Couldn't confirm the reset went through · if you're still at the limit in a moment, try again", and
/// its `unconfirmedAnytimeLine`), or could not reset and used nothing ("Couldn't reset your limits · nothing was used
/// · try again in a moment"). Its "Still couldn't confirm your reset · nothing more was used · … contact support" is
/// not: it says to ask support, not to press again; nor is "Couldn't reset your limits with this login · run /login,
/// then try again", which a press again meets the same way until the login is signed in again ([`login`]).
/// Its others that used nothing, or may not have, and say to try again (2.1.332) are so too.
pub fn again(why: &str) -> bool {
    const AGAIN: [&str; 7] = [
        "Couldn't confirm the reset went through",
        "Couldn't reset your limits · nothing was used",
        "Couldn't reset your 5-hour limit right now",
        "Couldn't check for a reset",
        "Another reset was just started on your account · this one wasn't used",
        "Another reset was just started on your account · checking your limits now",
        "Your earlier reset may still be going through",
    ];
    AGAIN.iter().any(|a| why.starts_with(a))
}

/// Whether WHY, Claude Code's refusal of a press, says this login must sign in again before it can reset ("Couldn't
/// reset your limits with this login · run /login, then try again"): the card's `Sign in` is in sight
/// ([`coord_types::ResetTry::login`]), and his sign-in ends it ([`super::State::signed_in_again`]).
pub fn login(why: &str) -> bool {
    why.starts_with("Couldn't reset your limits with this login") || why.starts_with("Couldn't reset your 5-hour limit with this login")
}

/// OFFERS in words, for the journal: "Fable reset (2 left), full reset (how many not said; can't be used while you're
/// at your 5-hour limit)", or "no reset".
pub fn said(offers: &[ResetOffer]) -> String {
    let said: Vec<String> = offers
        .iter()
        .map(|o| {
            let left = o.left.map_or("how many not said".into(), |n| format!("{n} left"));
            format!("{} ({left}{})", o.kind, o.why.as_ref().map_or(String::new(), |w| format!("; {w}")))
        })
        .collect();
    if said.is_empty() { "no reset".into() } else { said.join(", ") }
}

/// The owner's last press R as the card shows it at NOW: one that has said nothing within [`SESSION`] (its coordd
/// ended mid-press) is said to have ended with no word from Claude Code, not known to be used, so it may be pressed
/// again ([`ResetTry::again`]).
pub fn shown_try(r: ResetTry, now: i64) -> ResetTry {
    if r.done.is_none() && r.refused.is_none() && now - r.since > SESSION {
        return ResetTry { refused: Some("its session ended before Claude Code said how it went".into()), again: true, ..r };
    }
    r
}

/// What a press's session shows at a look ([`shown`]).
#[derive(Debug, Clone, PartialEq)]
pub enum Shown {
    /// Claude Code's confirm stands (`Use a Fable reset?`): its choice with `Yes, use my …` first and chosen (answered
    /// Enter), or, when TYPED, the form its screen-reader mode shows (`Enter y/n:`; 2.1.329 shows it only with the screen
    /// reader on), which takes text: coordd types none, so it is answered no ([`step`]).
    Confirm { typed: bool },
    /// It asks whether to use one though the account is at no limit (its `earlyLine`, in these words): not used, since
    /// he pressed `Use` on an offer made at a limit.
    Early(String),
    /// It asks which reset (its `chooseTitle`): the argument named none it has.
    Choose,
    /// It was applied: Claude Code's words.
    Done(String),
    /// It was not, or Claude Code could not confirm it was: its words.
    Refused(String),
    /// Nothing of these yet.
    Waiting,
}

/// What SCREEN, a press's or a look's session's screen as text, shows (2.1.329's words): an outcome, or a question of
/// `/limit-reset`'s, whichever stands lower on it, so that what is drawn last counts.
pub fn shown(screen: &str) -> Shown {
    let lines: Vec<&str> = screen.lines().collect();
    let last = |f: &dyn Fn(&str) -> bool| lines.iter().rposition(|l| f(l));
    let answer = last(&|l| done(l) || refused(&words(l)));
    let dialog = last(&|l| l.contains(CHOOSE) || l.contains("Yes, use my ") || l.contains("anyway?") || words(l).starts_with("Use a ") && words(l).ends_with('?'));
    if let Some(i) = answer.filter(|i| dialog.is_none_or(|d| *i > d)) {
        return if done(lines[i]) { Shown::Done(words(lines[i])) } else { Shown::Refused(words(lines[i])) };
    }
    if dialog.is_none() {
        return Shown::Waiting;
    }
    if let Some(l) = lines.iter().rev().find(|l| l.contains("anyway?")) {
        return Shown::Early(words(l));
    }
    if screen.contains(CHOOSE) {
        return Shown::Choose;
    }
    Shown::Confirm { typed: screen.contains("Enter y/n:") }
}

/// A line's words, without what Claude Code draws around them (`⎿`, `●`, `❯`, the dialog's frame).
fn words(line: &str) -> String {
    line.trim_matches(|c: char| !c.is_alphanumeric() && !"?.".contains(c)).to_string()
}

/// Whether LINE is Claude Code's word that a reset was applied: its `successLine`, what was done (`DONE`) then "your
/// weekly reset day stays Monday" when it knows that day, and the count left, "Limits reset · 1 full reset left"; the
/// 5-hour reset's, "5-hour limit reset · next reset available Dec 1 · your weekly limit still applies"; a reset used at
/// a limit that still holds, "Fable reset used · you're still at your weekly limit until Nov 24 · 1 Fable reset left";
/// or an earlier try that went through after all (2.1.332).
fn done(line: &str) -> bool {
    const DONE: [&str; 4] = ["Limits reset", "Weekly limits reset", "Fable reset used", "5-hour limit reset"];
    let w = words(line);
    let parts: Vec<&str> = w.split(DOT).collect();
    let after = |p: &&str| p.ends_with(" left") || p.starts_with("your weekly reset day stays") || *p == "your weekly limit still applies";
    DONE.contains(&parts[0]) && parts[1..].iter().any(after) || w.contains(" used \u{b7} you're still at your ") || w.starts_with("Your earlier reset went through after all")
}

/// Whether W, a line's words, is an answer of `/limit-reset` that applied nothing (2.1.329): none of that kind or none
/// to use ("No Fable reset to use · you can use a full reset", "No reset to use · you're not at a usage limit right
/// now"); one held that cannot be used now ("Your Fable reset can't be used right now · …", "Your reset doesn't refill
/// your weekly limit, so it can't be used until that resets · …", "You have a reset saved for when you reach a usage
/// limit · …", "Your reset only works at a usage limit, …", "Your Fable reset is no longer on offer · …"); all used
/// ("Reset used · no resets left", "That reset was already used · …"); or it could not be done, checked or confirmed.
/// 2.1.332 adds the 5-hour reset's ("A 5-hour limit reset isn't available right now.", "Weekly reset used · available
/// again Dec 1", "Couldn't reset your 5-hour limit …", "Your 5-hour limit is already being reset · …") and those of a
/// try that met another or an earlier one ("Another reset was just started …", "This reset isn't available any more ·
/// …", "Your limits are clear", ...). A choice the chooser cannot use now is drawn "Fable reset · can't be used while
/// …", which is none of these.
fn refused(w: &str) -> bool {
    const SAID: [&str; 23] = [
        "Couldn't reset your limits",
        "Couldn't confirm the reset",
        "Still couldn't confirm",
        "Couldn't check for a reset",
        "Your limits are already being reset",
        "Resets can't be used while",
        "You have a reset saved for",
        "None of your resets refill your",
        "This limit may have lifted already",
        "That reset was already used",
        "That isn't a reset name",
        "Unknown command",
        "Unknown slash command",
        "A 5-hour limit reset isn't available",
        "Weekly reset used \u{b7}",
        "Couldn't reset your 5-hour limit",
        "Your 5-hour limit is already being reset",
        "Your limit had already reset on its own",
        "Your limits are clear",
        "Another reset was just started",
        "Your earlier reset may still be going through",
        "This reset isn't available any more",
        "A different reset is on offer now",
    ];
    let held_not_now = ["can't be used", "no longer on offer", "only works at a usage limit"];
    SAID.iter().any(|s| w.contains(s)) || w.starts_with("No ") && w.contains(" to use") || w.starts_with("Your ") && held_not_now.iter().any(|h| w.contains(h)) || w.starts_with("Reset used \u{b7}")
}

/// What the look does at a read of its screen ([`look`]).
#[derive(Debug, Clone, PartialEq)]
pub enum Look {
    /// Nothing yet.
    Wait,
    /// `/limit-reset` lists what the account holds, these as [`listed`] reads them (none it could read: empty):
    /// Escape, which keeps them all, and the look ends.
    Listed(Vec<ResetOffer>),
    /// It answered instead, in these words (none to use now, none could be checked): the look ends, with nothing to
    /// answer.
    Said(String),
}

/// What the look does with SCREEN at NOW ([`shown`]): Escape on a list, and nothing on an answer. Its chooser, the
/// confirm of the one kind it holds, that confirm's typed form and its "anyway?" are each a list, answered Escape: the
/// press's Enter ([`step`]) has no place here.
pub fn look(screen: &str, now: i64) -> Look {
    match shown(screen) {
        Shown::Choose | Shown::Confirm { .. } | Shown::Early(_) => Look::Listed(listed(screen, now).unwrap_or_default()),
        Shown::Done(words) | Shown::Refused(words) => Look::Said(words),
        Shown::Waiting => Look::Wait,
    }
}

/// What `/limit-reset`'s answer at rest says of the resets held (2.1.329's lines), for the look ([`at_rest`],
/// [`looked`]).
#[derive(Debug, Clone, PartialEq)]
pub enum AtRest {
    /// None is held: "No reset to use · you're not at a usage limit right now" (no grants, none eligible, the feature
    /// off), "Reset used · {resets} left" (all used); the 5-hour reset's at no 5-hour limit, the kinds off: "A 5-hour
    /// limit reset isn't available right now.", "Weekly reset used · available again Dec 1".
    None,
    /// Some are held that cannot be used now, and it does not list them: "You have a reset saved for when you reach a
    /// usage limit · …", "Your reset doesn't refill your {limit}, …", "Your reset can't be used right now · …", "None
    /// of your resets refill your …", "Your reset only works at a usage limit, …"; with what its line names when it
    /// names a count ("· 2 resets left · use by Nov 27"): that count, its kind if named ("2 Fable resets", else
    /// [`ANY_KIND`]), and its "use by".
    Held(Option<ResetOffer>),
    /// It could not say: the fetch failed, it could not confirm, it is checking again, or anything else.
    Unknown,
}

/// What WORDS, `/limit-reset`'s answer at rest ([`Look::Said`]) at NOW, says of the resets held ([`AtRest`]).
pub fn at_rest(words: &str, now: i64) -> AtRest {
    const NONE: [&str; 4] = ["No reset to use", "Reset used \u{b7}", "A 5-hour limit reset isn't available", "Weekly reset used \u{b7}"];
    const HELD: [&str; 5] = ["You have a reset saved for", "Your reset doesn't refill your", "Your reset can't be used", "None of your resets refill your", "Your reset only works at a usage limit"];
    if NONE.iter().any(|n| words.starts_with(n)) {
        return AtRest::None;
    }
    if HELD.iter().any(|h| words.starts_with(h)) {
        let parts: Vec<&str> = words.split(DOT).map(str::trim).collect();
        let Some((left, named)) = parts.iter().find_map(|p| p.strip_suffix(" left")?.split_once(' ').and_then(|(n, k)| Some((n.parse::<u32>().ok()?, k)))) else { return AtRest::Held(None) };
        let kind = named.strip_suffix('s').unwrap_or(named).replace("five-hour", "5-hour");
        let kind = if argument(&kind).is_some() { kind } else { ANY_KIND.into() };
        let use_by = parts.iter().find_map(|p| day_end(p.strip_prefix("use by ")?, now));
        return AtRest::Held(Some(ResetOffer { kind, left: Some(left), use_by, ..Default::default() }));
    }
    AtRest::Unknown
}

/// What the look found, LOOK at NOW, kept in STATE: a list is what it holds ([`noticed`]; its typed form, read as
/// none, changes nothing); an answer at rest ([`at_rest`]) that none is held clears the list; one that some are held
/// keeps the list when it names no count or the list's, else keeps what it names, so the card shows it; each with its
/// time ([`State::resets_seen`]); one that says nothing it could check keeps it with its time as it was. The words for
/// the journal when the count an answer names is not that of a list on file.
pub fn looked(state: &mut State, look: Look, now: i64) -> Option<String> {
    match look {
        Look::Wait => None,
        Look::Listed(held) => {
            if !held.is_empty() {
                noticed(state, held, now);
            }
            None
        }
        Look::Said(words) => match at_rest(&words, now) {
            AtRest::None => {
                (state.resets, state.resets_seen) = (Vec::new(), Some(now));
                None
            }
            AtRest::Held(named) => {
                state.resets_seen = Some(now);
                let listed: u32 = state.resets.iter().filter_map(|o| o.left).sum();
                let named = named.filter(|o| o.left != Some(listed) || state.resets.is_empty())?;
                let note = (!state.resets.is_empty()).then(|| format!("Claude Code says {} left, where the list on file says {listed}: {words}", named.left.unwrap_or(0)));
                state.resets = vec![named];
                note
            }
            AtRest::Unknown => None,
        },
    }
}

/// Whether the owner's check of sign-in account STATE (as before the check) at NOW is his `Check limits`, which also
/// reads `/limit-reset`'s list: offered only on one signed in whose login has neither run out nor been refused (yaaihh
/// accounts.rs `checks`). Otherwise it is `Check whether it is signed in`, which reads no list.
pub fn looks_on_check(state: &State, now: i64) -> bool {
    let refused = state.out_for.as_deref() == Some(limits::REFUSED) && state.out_until.is_some_and(|t| t > now);
    state.signed_in && !state.login_expires.is_some_and(|t| t <= now) && !refused
}

/// Why nothing more may start on STATE's own terminal at NOW, if so: a press being applied, or a look that runs
/// ([`State::reset_look`]), each for at most [`SESSION`].
pub fn busy(state: &State, now: i64) -> Option<String> {
    if let Some(r) = state.reset.as_ref().filter(|r| r.done.is_none() && r.refused.is_none() && now - r.since <= SESSION) {
        return Some(format!("a {} is being applied", r.kind));
    }
    state.reset_look.filter(|t| now - t <= SESSION).map(|_| "its resets are being read from Claude Code".into())
}

/// What a press's session does at a look ([`step`]).
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// Nothing yet.
    Wait,
    /// Enter, on the confirm's chosen `Yes, use my …`.
    Enter,
    /// It ends with Claude Code's words once applied, or why not; with Escape first when a question of Claude Code's
    /// stands that is answered no.
    End { escape: bool, outcome: Result<String, String> },
}

/// What a press's session does with SCREEN, which shows SHOWN ([`shown`]), for the reset ARG: CONFIRM_FOR is how long
/// a confirm has stood, ENTERS how many Enters went, LAST the screen the last one was given to and how long ago.
///
/// Only two keys ever go, and never text: on Claude Code's input box an Enter sends nothing, and so does Escape.
/// - The confirm's choice gets one Enter, a second after it shows: its input guard refuses keys for a quarter second
///   after it opens (2.1.329).
/// - One more Enter goes only when the screen two seconds after the first is byte for byte the screen it was given
///   to, so that Enter was refused; never a third. An Enter that was taken is never followed by another, which could
///   use a second reset.
/// - Its typed form, "anyway?" (the account at no limit now) and "Which reset…" are answered no with Escape.
pub fn step(shown: Shown, screen: &str, confirm_for: std::time::Duration, last: Option<(&str, std::time::Duration)>, enters: u32, arg: &str) -> Step {
    let end = |escape, outcome| Step::End { escape, outcome };
    match shown {
        Shown::Done(words) => end(false, Ok(words)),
        Shown::Refused(words) => end(false, Err(words)),
        Shown::Early(words) => end(true, Err(format!("{words} Not used: the account is at no limit now"))),
        Shown::Choose => end(true, Err(format!("Claude Code asked which reset to use: /limit-reset {arg} named none it has"))),
        Shown::Confirm { typed: true } => end(true, Err("Claude Code asked for its confirm to be typed (`Enter y/n:`, its screen-reader form), and coordd types no text into Claude Code: not used".into())),
        Shown::Confirm { typed: false } => {
            let due = match (enters, last) {
                (0, _) => confirm_for >= std::time::Duration::from_secs(1),
                (1, Some((before, ago))) => before == screen && ago >= std::time::Duration::from_secs(2),
                _ => false,
            };
            if due { Step::Enter } else { Step::Wait }
        }
        Shown::Waiting => Step::Wait,
    }
}

/// The last lines of SCREEN that say something, for an outcome Claude Code did not say in known words.
pub fn last_lines(screen: &str) -> String {
    let said: Vec<&str> = screen.lines().map(str::trim).filter(|l| l.chars().any(char::is_alphanumeric)).collect();
    said[said.len().saturating_sub(3)..].join(" / ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_notice_forms_give_the_offers_in_one_word_per_kind_and_a_screen_without_one_gives_none() {
        let at = |y, m, d, h, min, sec| chrono::TimeZone::with_ymd_and_hms(&chrono::Local, y, m, d, h, min, sec).single().unwrap().timestamp();
        let (now, end) = (at(2026, 11, 20, 15, 0, 0), |y, m, d| at(y, m, d, 23, 59, 59));
        let one = "  ⎿  You've hit your limit · resets 3pm (Europe/Amsterdam)\n     /limit-reset to use your Fable reset · 2 left · use by Nov 27\n";
        assert_eq!(offers(one, now), Some(vec![ResetOffer { kind: "Fable reset".into(), left: Some(2), use_by: Some(end(2026, 11, 27)), ..Default::default() }]));
        let next_year = "/limit-reset to use your 5-hour reset · 1 left · use by Jan 3, 2027";
        assert_eq!(offers(next_year, now).unwrap()[0].use_by, Some(end(2027, 1, 3)));
        let many = "x\n  /limit-reset · 2 Fable, 1 five-hour resets · first expires Nov 27\n";
        assert_eq!(
            offers(many, now),
            Some(vec![ResetOffer { kind: "Fable reset".into(), left: Some(2), ..Default::default() }, ResetOffer { kind: "5-hour reset".into(), left: Some(1), ..Default::default() }])
        );
        assert_eq!(offers("You've hit your limit · resets 3pm\n❯ ", now), None);
        for o in offers(many, now).unwrap() {
            assert!(argument(&o.kind).is_some(), "{} is typed back as an argument", o.kind);
        }
        assert_eq!((argument("full reset"), argument("weekly reset"), argument("monthly reset")), (Some("full"), Some("weekly"), None));
    }

    #[test]
    fn the_card_gets_offers_only_while_out_for_its_limit_and_none_past_use_by_and_a_silent_press_ends() {
        let now = 1_000_000;
        let offer = |kind: &str, by| ResetOffer { kind: kind.into(), left: Some(1), use_by: by, ..Default::default() };
        let out = State { signed_in: true, out_until: Some(now + 60), out_for: Some(limits::RATE_LIMIT.into()), resets: vec![offer("Fable reset", Some(now + 10)), offer("full reset", Some(now - 1)), offer("5-hour reset", None)], ..State::default() };
        assert_eq!(offered(&out, now).iter().map(|o| o.kind.as_str()).collect::<Vec<_>>(), ["Fable reset", "5-hour reset"]);
        assert!(offered(&State { out_until: Some(now - 1), ..out.clone() }, now).is_empty(), "its limit reset");
        assert!(offered(&State { out_for: Some(limits::REFUSED.into()), ..out.clone() }, now).is_empty(), "out for its login");
        assert_eq!(offered(&State { out_until: Some(limits::UNKNOWN_RESET), ..out.clone() }, now).len(), 2, "out with no time named");
        let pressed = ResetTry { kind: "Fable reset".into(), since: now, ..Default::default() };
        assert_eq!(shown_try(pressed.clone(), now + SESSION), pressed);
        assert!(shown_try(pressed.clone(), now + SESSION + 1).refused.is_some_and(|w| w.contains("before Claude Code said")));
        let done = ResetTry { done: Some("Limits reset".into()), ..pressed };
        assert_eq!(shown_try(done.clone(), now + 1000), done);
    }

    #[test]
    fn at_a_limit_with_no_notice_seen_the_card_offers_fable_and_full_and_claude_codes_no_is_its_refusal() {
        let now = 1_000_000;
        let window = |used: f64, at: i64| Some(crate::accounts::Window { used_percent: used, resets_at: at });
        let kinds = |s: &State| offered(s, now).into_iter().map(|o| (o.kind, o.left, o.use_by)).collect::<Vec<_>>();
        let plain = [("Fable reset".to_string(), None, None), ("full reset".to_string(), None, None)];
        // out for its limit with no notice read (the agent moved before its screen was), or a window read used up
        let out = State { signed_in: true, out_until: Some(now + 60), out_for: Some(limits::RATE_LIMIT.into()), ..State::default() };
        assert_eq!(kinds(&out), plain);
        let used_up = State { signed_in: true, five_hour: window(100.0, now + 600), seven_day: window(40.0, now + 86400), ..State::default() };
        assert_eq!(kinds(&used_up), plain);
        assert_eq!(kinds(&State { five_hour: window(100.0, now + 600), seven_day: None, ..used_up.clone() }), plain, "the weekly not read");
        assert_eq!(kinds(&State { five_hour: window(40.0, now + 600), seven_day: window(100.0, now + 86400), ..used_up.clone() }), plain, "the weekly used up");
        // at no limit: none
        assert!(kinds(&State { five_hour: window(99.5, now + 600), ..used_up.clone() }).is_empty(), "not used up");
        assert!(kinds(&State { five_hour: window(100.0, now), ..used_up.clone() }).is_empty(), "started again");
        assert!(kinds(&State { signed_in: true, ..State::default() }).is_empty(), "nothing read");
        // out for its login or its credit: no reset, though a window says used up
        for why in [limits::REFUSED, limits::NO_CREDIT] {
            assert!(kinds(&State { out_until: Some(now + 60), out_for: Some(why.into()), ..used_up.clone() }).is_empty(), "out for {why}");
        }
        // the kinds a notice named stand in for these, as they were said; a notice's all past their "use by": none
        let named = ResetOffer { kind: "5-hour reset".into(), left: Some(1), use_by: Some(now + 10), ..Default::default() };
        assert_eq!(offered(&State { resets: vec![named.clone()], ..used_up.clone() }, now), [named.clone()]);
        assert!(offered(&State { resets: vec![ResetOffer { use_by: Some(now - 1), ..named }], ..used_up.clone() }, now).is_empty());
        // each is typed back as Claude Code's argument, and a kind it does not hold ends in its words
        assert!(AT_A_LIMIT.iter().all(|k| argument(k).is_some()));
        assert_eq!(shown("> /limit-reset fable\n  ⎿  No Fable reset to use\n"), Shown::Refused("No Fable reset to use".into()));
        assert_eq!(step(shown("  ⎿  No Fable reset to use\n"), "", std::time::Duration::ZERO, None, 0, "fable"), Step::End { escape: false, outcome: Err("No Fable reset to use".into()) });
    }

    #[test]
    fn a_kind_refused_at_this_limit_is_offered_no_more_until_a_notice_names_resets_or_the_limit_ends() {
        let now = 1_000_000;
        let kinds = |s: &State, at: i64| offered(s, at).into_iter().map(|o| o.kind).collect::<Vec<_>>();
        let press = |s: &mut State, kind: &str, since: i64, said: &str| {
            s.reset = Some(ResetTry { kind: kind.into(), since, ..Default::default() });
            went(s, since, Went::of(Err(said.into())), since + 5);
        };
        // at its limit with no notice seen: both kinds; each Claude Code does not apply is offered no more at it
        let mut s = State { signed_in: true, out_until: Some(now + 600), out_for: Some(limits::RATE_LIMIT.into()), ..State::default() };
        press(&mut s, "Fable reset", now, "No Fable reset to use");
        assert_eq!(kinds(&s, now + 10), ["full reset"]);
        assert_eq!(s.reset.as_ref().unwrap().refused.as_deref(), Some("No Fable reset to use"));
        press(&mut s, "full reset", now + 20, "No full reset to use");
        assert!(kinds(&s, now + 30).is_empty(), "both refused: nothing to press");
        assert_eq!(s.resets_refused, [("Fable reset".to_string(), now + 600), ("full reset".to_string(), now + 600)]);
        // an earlier press's outcome leaves the later's state
        went(&mut s, now, Went::Done("Limits reset".into()), now + 40);
        assert!(s.reset.as_ref().unwrap().done.is_none());
        // a screen that names none changes nothing; a notice that names resets says again what it holds
        noticed(&mut s, Vec::new(), now + 50);
        assert!(kinds(&s, now + 50).is_empty());
        let named = ResetOffer { kind: "Fable reset".into(), left: Some(2), ..Default::default() };
        noticed(&mut s, vec![named.clone()], now + 55);
        assert_eq!(offered(&s, now + 60), [named.clone()]);
        // refused again, it is offered at the next limit, after the one it was refused at has ended
        press(&mut s, "Fable reset", now + 70, "No Fable reset to use");
        assert!(kinds(&s, now + 80).is_empty());
        assert_eq!(kinds(&State { out_until: Some(now + 5000), ..s.clone() }, now + 700), ["Fable reset"]);
        // a limit with no time named ends when it is back in
        let mut unknown = State { out_until: Some(limits::UNKNOWN_RESET), resets: Vec::new(), resets_refused: Vec::new(), ..s.clone() };
        press(&mut unknown, "full reset", now + 90, "its terminal took no command");
        assert_eq!(kinds(&unknown, now + 100_000), ["Fable reset"]);
        unknown.back_in();
        assert!(unknown.resets_refused.is_empty());
        (unknown.out_until, unknown.out_for) = (Some(limits::UNKNOWN_RESET), Some(limits::RATE_LIMIT.into()));
        assert_eq!(kinds(&unknown, now + 200_000), ["Fable reset", "full reset"]);
        // pressed at no limit (it ended meanwhile): nothing is kept, and nothing is offered
        let mut over = State { signed_in: true, ..State::default() };
        press(&mut over, "Fable reset", now, "No Fable reset to use");
        assert!(over.resets_refused.is_empty() && kinds(&over, now + 10).is_empty());
    }

    #[test]
    fn a_press_screen_is_read_as_its_confirm_question_or_outcome_in_claude_codes_words() {
        let confirm = "╭──\n│ Use a Fable reset?\n│ Refills your weekly limit now · returns what Fable used to your weekly limit · your weekly reset day stays Monday\n│ 2 Fable resets left · use by Nov 27\n│ ❯ Yes, use my Fable reset\n│   No, keep it\n";
        assert_eq!(shown(confirm), Shown::Confirm { typed: false });
        assert_eq!(shown("│ Use a full reset?\n│ Enter y/n: \n"), Shown::Confirm { typed: true });
        assert_eq!(shown("> /limit-reset fable\n  Resetting your limits… this usually takes a few seconds\n"), Shown::Waiting);
        let done = "> /limit-reset fable\n  ⎿  Fable reset used · what Fable used is back in your weekly limit · your weekly reset day stays Monday · 1 Fable reset left\n";
        assert_eq!(shown(done), Shown::Done("Fable reset used · what Fable used is back in your weekly limit · your weekly reset day stays Monday · 1 Fable reset left".into()));
        assert_eq!(shown("  ⎿  Couldn't reset your limits · nothing was used · try again in a moment\n"), Shown::Refused("Couldn't reset your limits · nothing was used · try again in a moment".into()));
        assert!(matches!(shown("  ⎿  Couldn't confirm the reset went through · if you're still at the limit in a moment, try again"), Shown::Refused(w) if w.starts_with("Couldn't confirm")));
        assert_eq!(shown("  ⎿  No weekly reset to use\n"), Shown::Refused("No weekly reset to use".into()));
        assert!(matches!(shown("│ You still have 40% of your 5-hour limit left — use your Fable reset anyway?\n│ ❯ Yes, use my Fable reset\n"), Shown::Early(w) if w.ends_with("anyway?")));
        assert_eq!(shown("│ Which reset do you want to use?\n│ ❯ 1. Fable reset\n"), Shown::Choose);
        assert_eq!(last_lines("a\n\n──\nb\n❯ \nc d\n"), "a / b / c d");
    }

    /// Made up from 2.1.329's `/limit-reset` texts: its chooser with a choice it cannot use now, the Fable suffix at a
    /// weekly limit and its `label`'s count and date.
    const CHOOSER: &str = "> /limit-reset\n╭──\n│ Which reset do you want to use?\n│ ❯ Use a Fable reset (2 left, until Nov 27) · frees up only what Fable used this week\n│   Use a full reset (1 left, until Dec 3)\n│   5-hour reset · can't be used while you're at your weekly limit\n";

    #[test]
    fn the_real_refusal_ends_a_press_at_once_and_a_list_is_never_taken_for_one() {
        use std::time::Duration;
        // 2.1.329's not_offered for a kind it does not hold names what it does: the press ends at once, in its words
        let no = "> /limit-reset fable\n  ⎿  No Fable reset to use · you can use a full reset\n";
        assert_eq!(shown(no), Shown::Refused("No Fable reset to use · you can use a full reset".into()));
        assert_eq!(step(shown(no), no, Duration::ZERO, None, 0, "fable"), Step::End { escape: false, outcome: Err("No Fable reset to use · you can use a full reset".into()) });
        // each of its other answers that applied nothing
        for said in [
            "No reset to use · you're not at a usage limit right now",
            "Reset used · no resets left",
            "Your Fable reset can't be used while you're at your weekly limit · you can use a full reset",
            "Your reset can't be used right now · try again in a minute",
            "Couldn't check for a reset right now · try again in a moment",
            "Your limits are already being reset · one moment",
            "Resets can't be used while you continue at lower priority · run /limit-reset after that ends",
        ] {
            assert_eq!(shown(&format!("> /limit-reset\n  ⎿  {said}\n")), Shown::Refused(said.into()), "{said}");
        }
        // the chooser, with its choice it cannot use now, is the chooser; its kept lines after an Escape are no answer
        assert_eq!(shown(CHOOSER), Shown::Choose);
        assert_eq!(shown("> /limit-reset\n  ⎿  Resets kept · run /limit-reset when you want one · first expires Nov 27\n"), Shown::Waiting);
        assert_eq!(shown("  ⎿  Fable reset kept · run /limit-reset at a limit to use it · use by Nov 27, 3pm\n"), Shown::Waiting);
        // what is drawn lower counts: a list after an answer is the list, an answer after a list the answer
        assert_eq!(shown(&format!("{no}{CHOOSER}")), Shown::Choose);
        assert!(matches!(shown(&format!("{CHOOSER}  ⎿  Couldn't reset your limits · nothing was used · try again in a moment\n")), Shown::Refused(w) if w.starts_with("Couldn't reset")));
    }

    #[test]
    fn the_look_reads_what_limit_reset_lists_and_answers_each_list_with_escape_alone() {
        use std::time::Duration;
        let now = 1_000_000;
        let offer = |kind: &str, left, by: &str| ResetOffer { kind: kind.into(), left, use_by: day_end(by, now), ..Default::default() };
        // the chooser's choices it can use, each with its count and date
        let fable = ResetOffer { gives: Some("frees up only what Fable used this week".into()), ..offer("Fable reset", Some(2), "Nov 27") };
        let cannot = ResetOffer { kind: "5-hour reset".into(), why: Some("can't be used while you're at your weekly limit".into()), ..Default::default() };
        assert_eq!(look(CHOOSER, now), Look::Listed(vec![fable, offer("full reset", Some(1), "Dec 3"), cannot]));
        // the confirm of the one kind it holds, its "use by" with an hour and a zone; the same with its "anyway?"
        let confirm = "> /limit-reset\n╭──\n│ Use a Fable reset?\n│ Refills your weekly limit now · returns what Fable used to your weekly limit · your weekly reset day stays Monday\n│ 2 Fable resets left · use by Nov 27, 3pm (UTC)\n│ ❯ Yes, use my Fable reset\n│   No, keep it\n";
        let refills = ResetOffer { gives: Some("Refills your weekly limit now · returns what Fable used to your weekly limit".into()), ..offer("Fable reset", Some(2), "Nov 27") };
        assert_eq!(look(confirm, now), Look::Listed(vec![refills.clone()]));
        let early = confirm.replace("│ Refills", "│ You still have 40% of your weekly limit left — use your Fable reset anyway?\n│ Refills");
        assert_eq!(look(&early, now), Look::Listed(vec![refills]));
        // a press answers that confirm with Enter; the look has no Enter to give: only its list, for Escape
        assert_eq!(step(shown(confirm), confirm, Duration::from_secs(2), None, 0, "fable"), Step::Enter);
        // its typed form is a list read as none; an answer ends the look; the input box is waited on
        assert_eq!(look("│ Use a full reset?\n│ Enter y/n: \n", now), Look::Listed(Vec::new()));
        assert_eq!(look("> /limit-reset\n  ⎿  No reset to use · you're not at a usage limit right now\n", now), Look::Said("No reset to use · you're not at a usage limit right now".into()));
        assert_eq!(look("> /limit-reset\n", now), Look::Wait);
        // `offers` reads the list as it reads a notice; a screen with neither names none
        assert_eq!(offers(CHOOSER, now).map(|o| o.len()), Some(3));
        assert_eq!(offers("> \n", now), None);
        assert_eq!(said(&offers(CHOOSER, now).unwrap()[..2]), "Fable reset (2 left), full reset (1 left)");
        assert_eq!(said(&[]), "no reset");
    }

    /// The chooser's kind it cannot use at this limit is kept with Claude Code's reason (2.1.330's words); the card gets
    /// the reason only while that limit lasts, and a press on that kind is no offer.
    #[test]
    fn the_look_keeps_why_a_kind_cannot_be_used_and_what_each_gives_back() {
        let now = 1_000_000;
        let Look::Listed(listed) = look(CHOOSER, now) else { panic!("a list") };
        assert_eq!(listed[0].gives.as_deref(), Some("frees up only what Fable used this week"));
        assert_eq!((listed[1].kind.as_str(), listed[1].gives.as_deref(), listed[1].why.as_deref()), ("full reset", None, None));
        assert_eq!(listed[2], ResetOffer { kind: "5-hour reset".into(), why: Some("can't be used while you're at your weekly limit".into()), ..Default::default() });
        // its capital: "Full reset · …" is Claude Code's `full reset`; a line of other words is no kind
        assert_eq!(unusable("Full reset · doesn't refill your weekly limit").map(|o| o.kind), Some("full reset".into()));
        assert_eq!(unusable("Weekly limit · 40% used"), None);
        assert_eq!(unusable("Fable reset · "), None, "no reason");
        // the confirm's body, on one line or wrapped onto the next, without the weekly day
        let gives = "Refills your Fable and 5-hour limit now · returns what Fable used to your weekly limit";
        let one = format!("│ Use a Fable reset?\n│ {gives} · your weekly reset day stays Monday\n│ 2 Fable resets left · use by Nov 27, 3pm (UTC)\n│ ❯ Yes, use my Fable reset\n");
        let wrapped = "│ Use a Fable reset?\n│ Refills your Fable and 5-hour limit now · returns what Fable used to\n│ your weekly limit · your weekly reset day stays Monday\n│ 2 Fable resets left · use by Nov 27, 3pm (UTC)\n│ ❯ Yes, use my Fable reset\n";
        for screen in [one.as_str(), wrapped] {
            let Look::Listed(l) = look(screen, now) else { panic!("a list") };
            assert_eq!((l[0].gives.as_deref(), l[0].left), (Some(gives), Some(2)), "{screen}");
        }
        // a reason holds while the limit it was said at lasts: at it, read since it went out
        let at_limit = State { signed_in: true, out_until: Some(now + 600), out_for: Some(limits::RATE_LIMIT.into()), out_since: Some(now - 60), resets: listed.clone(), resets_seen: Some(now - 10), ..State::default() };
        let why = |s: &State, at| held(s, at).into_iter().find(|o| o.kind == "5-hour reset").and_then(|o| o.why);
        assert_eq!(why(&at_limit, now).as_deref(), Some("can't be used while you're at your weekly limit"));
        assert_eq!(offered(&at_limit, now).len(), 3, "all offered; the one with a reason takes no `Use`");
        assert_eq!(why(&State { out_until: Some(now - 1), ..at_limit.clone() }, now), None, "the limit ended");
        assert_eq!(why(&State { out_since: Some(now), ..at_limit.clone() }, now), None, "said before it went out at this one");
        assert_eq!(said(&listed), "Fable reset (2 left), full reset (1 left), 5-hour reset (how many not said; can't be used while you're at your weekly limit)");
    }

    /// A press Claude Code could not confirm, could not reset, or whose session said nothing is not known to be used: the
    /// kind stays offered with `Try … again`. A refusal, "Still couldn't confirm", or "with this login" takes the kind away
    /// at this limit.
    #[test]
    fn a_press_not_known_to_be_used_can_be_tried_again_and_a_refusal_cannot() {
        let now = 1_000_000;
        let kinds = |s: &State| offered(s, now + 10).into_iter().map(|o| o.kind).collect::<Vec<_>>();
        let out = State { signed_in: true, out_until: Some(now + 600), out_for: Some(limits::RATE_LIMIT.into()), ..State::default() };
        let press = |said: Went| {
            let mut s = State { reset: Some(ResetTry { kind: "Fable reset".into(), since: now, ..Default::default() }), ..out.clone() };
            went(&mut s, now, said, now + 5);
            s
        };
        for words in ["Couldn't confirm the reset went through · if you're still at the limit in a moment, try again", "Couldn't reset your limits · nothing was used · try again in a moment"] {
            assert_eq!(Went::of(Err(words.into())), Went::Unknown(words.into()));
            let s = press(Went::of(Err(words.into())));
            assert_eq!(kinds(&s), ["Fable reset", "full reset"], "{words}");
            let r = s.reset.unwrap();
            assert!(r.again && r.refused.as_deref() == Some(words));
        }
        let s = press(Went::Unknown("its session did not start: roster account: made-up".into()));
        assert!(s.reset.as_ref().unwrap().again && kinds(&s).contains(&"Fable reset".to_string()));
        for words in ["No Fable reset to use · you can use a full reset", "Still couldn't confirm your reset · nothing more was used · if your limits aren't refilled in a few minutes, contact support"] {
            let s = press(Went::of(Err(words.into())));
            assert_eq!(kinds(&s), ["full reset"], "{words}");
            assert!(!s.reset.as_ref().unwrap().again && !s.reset.unwrap().login);
        }
        // "with this login": no `Try again`, which would meet the same; the card's Sign in, until he signs in again, which
        // offers the kind again
        let words = "Couldn't reset your limits with this login · run /login, then try again";
        let mut s = press(Went::of(Err(words.into())));
        assert_eq!((kinds(&s), s.reset.as_ref().map(|r| (r.again, r.login))), (vec!["full reset".to_string()], Some((false, true))));
        s.signed_in_again();
        assert_eq!((kinds(&s), s.reset.is_none()), (vec!["Fable reset".to_string(), "full reset".to_string()], true));
        let mut other = press(Went::of(Err("No Fable reset to use · you can use a full reset".into())));
        other.signed_in_again();
        assert!(other.reset.is_some() && kinds(&other) == ["full reset"], "a refusal not for its login stands");
        // the card's own end of a press that said nothing in time: not known either
        let silent = ResetTry { kind: "Fable reset".into(), since: now, ..Default::default() };
        assert!(shown_try(silent, now + SESSION + 1).again);
    }

    #[test]
    fn a_look_and_a_press_wait_for_each_other_on_the_accounts_one_terminal() {
        let now = 1_000_000;
        let mut s = State { reset_look: Some(now), ..State::default() };
        assert_eq!(busy(&s, now + 10).as_deref(), Some("its resets are being read from Claude Code"));
        assert_eq!(busy(&s, now + SESSION + 1), None, "a look whose coordd ended mid-look holds no more");
        s.reset = Some(ResetTry { kind: "Fable reset".into(), since: now, ..Default::default() });
        assert_eq!(busy(&s, now + 10).as_deref(), Some("a Fable reset is being applied"));
        s.reset_look = None;
        s.reset.as_mut().unwrap().done = Some("Limits reset".into());
        assert_eq!(busy(&s, now + 10), None);
    }

    #[test]
    fn an_answer_at_rest_says_none_held_some_held_or_nothing_known_and_ends_at_once() {
        use std::time::Duration;
        let now = 1_000_000;
        // 2.1.329's answers at rest, in its words
        let held = |kind: &str, left, by| AtRest::Held(Some(ResetOffer { kind: kind.into(), left: Some(left), use_by: day_end(by, now), ..Default::default() }));
        let saved = "You have a reset saved for when you reach a usage limit \u{b7} run /limit-reset when you do \u{b7} 2 Fable resets left \u{b7} use by Nov 27";
        for (said, kind) in [
            ("No reset to use \u{b7} you're not at a usage limit right now", AtRest::None),
            ("Reset used \u{b7} no resets left", AtRest::None),
            (saved, held("Fable reset", 2, "Nov 27")),
            ("Your reset doesn't refill your weekly limit, so it can't be used until that resets \u{b7} 1 full reset left \u{b7} use by Dec 3", held("full reset", 1, "Dec 3")),
            ("Your reset can't be used right now \u{b7} it is paused \u{b7} 2 Fable resets left \u{b7} use by Nov 27", held("Fable reset", 2, "Nov 27")),
            ("Your reset only works at a usage limit, and you're not at one now \u{b7} nothing was used", AtRest::Held(None)),
            ("Couldn't check for a reset right now \u{b7} try again in a moment", AtRest::Unknown),
            ("Couldn't confirm the reset went through \u{b7} check /usage in a moment \u{b7} if your limits weren't refilled, run /limit-reset again", AtRest::Unknown),
            ("This limit may have lifted already \u{b7} checking your limits now \u{b7} run /limit-reset again in a moment to use your reset anyway", AtRest::Unknown),
        ] {
            assert_eq!(at_rest(said, now), kind, "{said}");
            // each is an answer: the look and a press end at its first read, with nothing more sent
            let screen = format!("> /limit-reset\n  \u{23bf}  {said}\n");
            assert_eq!(look(&screen, now), Look::Said(said.into()), "{said}");
            assert!(matches!(step(shown(&screen), &screen, Duration::ZERO, None, 0, "fable"), Step::End { escape: false, outcome: Err(_) }), "{said}");
        }
        // a reset used at a limit that still holds was applied
        let used = "  \u{23bf}  Fable reset used \u{b7} you're still at your weekly limit until Nov 24 \u{b7} 1 Fable reset left\n";
        assert!(matches!(shown(used), Shown::Done(w) if w.starts_with("Fable reset used")));
        // the lowest "anyway?" is the one that stands
        let early = "\u{2502} You still have 40% of your weekly limit left \u{2014} use your Fable reset anyway?\n\u{2502} You still have 12% of your weekly limit left \u{2014} use your Fable reset anyway?\n";
        assert!(matches!(shown(early), Shown::Early(w) if w.contains("12%")));

        // what each keeps: a list on file of two Fable resets, one kind refused at this limit
        let offer = |kind: &str, left| ResetOffer { kind: kind.into(), left, ..Default::default() };
        let on_file = State { resets: vec![offer("Fable reset", Some(2))], resets_seen: Some(now - 600), resets_refused: vec![("full reset".into(), now + 60)], ..State::default() };
        let mut none = on_file.clone();
        assert_eq!(looked(&mut none, Look::Said("No reset to use \u{b7} you're not at a usage limit right now".into()), now), None);
        assert_eq!((none.resets.len(), none.resets_seen), (0, Some(now)), "none held: the list cleared, its time stamped");
        let mut held = on_file.clone();
        assert_eq!(looked(&mut held, Look::Said(saved.into()), now), None, "the same count as the list");
        assert_eq!((held.resets.clone(), held.resets_seen), (on_file.resets.clone(), Some(now)), "held, not listed: the list kept, its time stamped");
        let more = saved.replace("2 Fable", "3 Fable");
        assert_eq!(looked(&mut held, Look::Said(more.clone()), now + 1), Some(format!("Claude Code says 3 left, where the list on file says 2: {more}")));
        let mut unknown = on_file.clone();
        assert_eq!(looked(&mut unknown, Look::Said("Couldn't check for a reset right now \u{b7} try again in a moment".into()), now), None);
        assert_eq!(unknown, on_file, "nothing it could check: the list and its time as they were");
        let mut typed = on_file.clone();
        looked(&mut typed, Look::Listed(Vec::new()), now);
        assert_eq!(typed, on_file, "a list read as none changes nothing");
        let mut listed = on_file.clone();
        looked(&mut listed, Look::Listed(vec![offer("full reset", Some(1))]), now);
        assert_eq!((listed.resets, listed.resets_seen, listed.resets_refused), (vec![offer("full reset", Some(1))], Some(now), Vec::new()));
    }

    /// 2.1.332's lines of `/limit-reset` that 2.1.329's words missed, the 5-hour reset's (its `tengu_nifty_lemur`, run
    /// when the kinds are off) among them: each ends a press or a look at its first read, as applied, refused, or not
    /// known to be used; its progress lines are waited on.
    #[test]
    fn the_five_hour_reset_and_2_1_332_lines_end_a_press_or_a_look_at_once() {
        use std::time::Duration;
        let now = 1_000_000;
        let screen = |said: &str| format!("> /limit-reset\n  \u{23bf}  {said}\n");
        // h28's look: the kinds off, at no limit
        let off = "A 5-hour limit reset isn't available right now.";
        assert_eq!(look(&screen(off), now), Look::Said(off.into()));
        let mut s = State { resets: vec![ResetOffer { kind: "Fable reset".into(), left: Some(1), ..Default::default() }], ..State::default() };
        assert_eq!((looked(&mut s, Look::Said(off.into()), now), s.resets.len(), s.resets_seen), (None, 0, Some(now)), "none held");
        let refusals = [
            (off, false, false),
            ("Weekly reset used \u{b7} available again Dec 1", false, false),
            ("Couldn't reset your 5-hour limit right now \u{b7} try again in a moment", true, false),
            ("Your 5-hour limit is already being reset \u{b7} one moment", false, false),
            ("Couldn't reset your 5-hour limit with this login \u{b7} run /login, then try again", false, true),
            ("Couldn't check for a reset right now \u{b7} try again in a moment", true, false),
            ("Your limit had already reset on its own \u{b7} nothing was used", false, false),
            ("Your limits are clear", false, false),
            ("Another reset was just started on your account \u{b7} this one wasn't used \u{b7} try again in a minute", true, false),
            ("Another reset was just started on your account \u{b7} checking your limits now \u{b7} if you're still at the limit in a minute, try again", true, false),
            ("Your earlier reset may still be going through \u{b7} checking your limits now \u{b7} try again in a minute if they aren't refilled", true, false),
            ("This reset isn't available any more \u{b7} nothing was used", false, false),
            ("This reset isn't available any more \u{b7} your earlier try may have gone through \u{b7} checking your limits now", false, false),
            ("A different reset is on offer now \u{b7} nothing was used \u{b7} run /limit-reset again to see it", false, false),
            ("Your Fable reset can't be used right now \u{b7} no Fable usage this week to give back", false, false),
            ("Your reset can't be used for this limit right now", false, false),
        ];
        for (said, try_again, sign_in) in refusals {
            assert_eq!(shown(&screen(said)), Shown::Refused(said.into()), "{said}");
            assert!(matches!(step(shown(&screen(said)), &screen(said), Duration::ZERO, None, 0, "fable"), Step::End { escape: false, outcome: Err(_) }), "{said}");
            assert_eq!((again(said), login(said)), (try_again, sign_in), "{said}");
        }
        let applied = [
            "5-hour limit reset \u{b7} next reset available Dec 1 \u{b7} your weekly limit still applies",
            "5-hour limit reset \u{b7} next reset available Dec 1 \u{b7} your weekly limit still applies \u{b7} send a message to continue",
            "5-hour limit reset \u{b7} your weekly limit still applies",
            "Limits reset \u{b7} 1 full reset left",
            "Weekly limits reset \u{b7} no weekly resets left",
            "Fable reset used \u{b7} what Fable used is back in your weekly limit \u{b7} your weekly reset day stays Monday \u{b7} 1 Fable reset left",
            "5-hour limit reset \u{b7} your weekly reset day stays Monday \u{b7} no five-hour resets left",
            "Your earlier reset went through after all \u{b7} checking your limits now",
        ];
        for said in applied {
            assert_eq!(shown(&screen(said)), Shown::Done(said.into()), "{said}");
        }
        for progress in ["Resetting your 5-hour limit\u{2026} this usually takes a few seconds", "Checking for a reset\u{2026}"] {
            assert_eq!(shown(&screen(progress)), Shown::Waiting, "{progress}");
        }
        // its notice at a 5-hour limit offers that one reset
        let notice = "  \u{23bf}  You've hit your limit \u{b7} resets 3pm\n     /limit-reset to reset your 5-hour limit now \u{b7} uses weekly limit \u{b7} 1/week\n";
        assert_eq!(offers(notice, now), Some(vec![ResetOffer { kind: "5-hour reset".into(), gives: Some("uses weekly limit \u{b7} 1/week".into()), ..Default::default() }]));
    }

    /// An answer at rest that names a count keeps it as what is held, so the card shows it before any limit: its kind
    /// when it names one, else Claude Code's plain "limit reset". `Use` still waits for a limit, and only for a kind
    /// `/limit-reset` takes.
    #[test]
    fn a_reset_held_at_rest_is_kept_for_the_card_and_offered_only_at_a_limit() {
        let now = 1_000_000;
        let saved = "You have a reset saved for when you reach a usage limit \u{b7} run /limit-reset when you do";
        let kept = |said: &str| {
            let mut s = State { signed_in: true, ..State::default() };
            looked(&mut s, Look::Said(said.into()), now);
            s
        };
        let plain = kept(&format!("{saved} \u{b7} 2 resets left \u{b7} use by Nov 27"));
        let generic = ResetOffer { kind: "limit reset".into(), left: Some(2), use_by: day_end("Nov 27", now), ..Default::default() };
        assert_eq!((plain.resets.clone(), plain.resets_seen), (vec![generic.clone()], Some(now)));
        assert_eq!(held(&plain, now), [generic.clone()], "the card shows it at rest");
        assert!(offered(&plain, now).is_empty(), "no `Use` at rest");
        let at_limit = State { out_until: Some(now + 600), out_for: Some(limits::RATE_LIMIT.into()), ..plain.clone() };
        let kinds = |s: &State| offered(s, now).into_iter().map(|o| o.kind).collect::<Vec<_>>();
        assert_eq!(kinds(&at_limit), AT_A_LIMIT, "a kind /limit-reset takes none of: the plain two");
        let fable = kept(&format!("{saved} \u{b7} 2 Fable resets left \u{b7} use by Nov 27"));
        assert_eq!(fable.resets, [ResetOffer { kind: "Fable reset".into(), ..generic.clone() }]);
        assert!(offered(&fable, now).is_empty());
        assert_eq!(kinds(&State { out_until: Some(now + 600), out_for: Some(limits::RATE_LIMIT.into()), ..fable.clone() }), ["Fable reset"]);
        let five = kept("Your reset doesn't refill your weekly limit, so it can't be used until that resets \u{b7} 1 five-hour reset left \u{b7} use by Dec 3");
        assert_eq!(five.resets, [ResetOffer { kind: "5-hour reset".into(), left: Some(1), use_by: day_end("Dec 3", now), ..Default::default() }]);
        assert_eq!(kept("Your reset can't be used right now \u{b7} it is paused \u{b7} 1 reset left").resets, [ResetOffer { kind: "limit reset".into(), left: Some(1), ..Default::default() }]);
        // one with no count names nothing to keep: the list on file stays
        let mut none_named = fable.clone();
        looked(&mut none_named, Look::Said("Your reset only works at a usage limit, and you're not at one now \u{b7} nothing was used".into()), now + 5);
        assert_eq!((none_named.resets, none_named.resets_seen), (fable.resets.clone(), Some(now + 5)));
    }

    #[test]
    fn only_the_press_the_card_shows_as_check_limits_starts_a_look() {
        let now = 1_000_000;
        let signed = State { signed_in: true, ..State::default() };
        assert!(looks_on_check(&signed, now));
        assert!(looks_on_check(&State { login_expires: Some(now + 1), ..signed.clone() }, now));
        // the card's `Check whether it is signed in`: not signed in, its login run out, or refused
        assert!(!looks_on_check(&State::default(), now));
        assert!(!looks_on_check(&State { login_expires: Some(now), ..signed.clone() }, now));
        let refused = State { out_for: Some(limits::REFUSED.into()), out_until: Some(limits::UNKNOWN_RESET), ..signed.clone() };
        assert!(!looks_on_check(&refused, now));
        assert!(looks_on_check(&State { out_for: Some(limits::RATE_LIMIT.into()), out_until: Some(now + 60), ..signed.clone() }, now), "at its limit: Check limits");
    }

    #[test]
    fn a_confirm_gets_one_enter_and_one_more_only_when_the_first_changed_nothing_and_never_any_text() {
        use std::time::Duration;
        let s = |n| Duration::from_millis(n);
        // made up from 2.1.329's texts: its confirm, the same with the reset under way, and its screen-reader form
        let confirm = "> /limit-reset fable\n╭──\n│ Use a Fable reset?\n│ Refills your weekly limit now · your weekly reset day stays Monday\n│ 2 Fable resets left · use by Nov 27\n│ ❯ Yes, use my Fable reset\n│   No, keep it\n";
        let changed = "> /limit-reset fable\n╭──\n│ Use a Fable reset?\n│ Resetting your limits… this usually takes a few seconds\n│ ❯ Yes, use my Fable reset\n";
        let typed = "> /limit-reset fable\n│ Use a Fable reset?\n│ Refills your weekly limit now · your weekly reset day stays Monday\n│ Enter y/n: \n";
        let at = |screen: &str, confirm_for, last, enters| step(shown(screen), screen, confirm_for, last, enters, "fable");
        assert_eq!(at(confirm, s(999), None, 0), Step::Wait, "its input guard");
        assert_eq!(at(confirm, s(1000), None, 0), Step::Enter);
        // the screen changed after the Enter, the confirm still on it: taken, so no second
        assert_eq!(at(changed, s(5000), Some((confirm, s(4000))), 1), Step::Wait);
        // byte for byte the same two seconds after: refused, so one more, then never a third
        assert_eq!(at(confirm, s(2500), Some((confirm, s(1500))), 1), Step::Wait);
        assert_eq!(at(confirm, s(3000), Some((confirm, s(2000))), 1), Step::Enter);
        assert_eq!(at(confirm, s(9000), Some((confirm, s(6000))), 2), Step::Wait);
        // the typed form: no `y`, no text; Escape, and not used
        assert!(matches!(at(typed, s(5000), None, 0), Step::End { escape: true, outcome: Err(w) } if w.contains("types no text")));
        let early = "│ You still have 40% of your 5-hour limit left — use your Fable reset anyway?\n│ ❯ Yes, use my Fable reset\n│   No, keep it\n";
        assert!(matches!(at(early, s(5000), None, 0), Step::End { escape: true, outcome: Err(w) } if w.ends_with("Not used: the account is at no limit now")));
        let done = "  ⎿  Limits reset · your weekly reset day stays Monday · 1 full reset left\n";
        assert_eq!(at(done, s(0), Some((confirm, s(3000))), 1), Step::End { escape: false, outcome: Ok("Limits reset · your weekly reset day stays Monday · 1 full reset left".into()) });
    }

    #[test]
    fn a_press_session_is_claude_code_in_the_sign_ins_environment_alone() {
        let words: Vec<String> = session("claude", Path::new("/acc/claude/a/config")).iter().map(|w| w.to_string_lossy().into_owned()).collect();
        assert_eq!(words[..2], ["env", "-i"]);
        assert!(words.contains(&"CLAUDE_CONFIG_DIR=/acc/claude/a/config".to_string()) && words.iter().any(|w| w.starts_with("BROWSER=")));
        assert_eq!(words[words.len() - 2..], ["claude", "--strict-mcp-config"]);
        assert!(!words.iter().any(|w| w.starts_with("ANTHROPIC_")), "no gateway");
    }
}
