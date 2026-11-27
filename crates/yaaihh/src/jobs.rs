//! An agent's jobs on the build runner, as the window shows them (N-1334; paper-research2's plan, consult-lead's
//! rulings p1, p2, p5): the job line under the "working now" line in its four places (`streams::working_now`), the
//! window's one progress bar, and the agent's last jobs behind a click on its tile (agents.rs). coordd sends them in
//! `AgentView.jobs`: the running ones newest first, then the last five ended, newest first.

use coord_types::{AgentView, Job};
use rune::{El, col, el, row, text};
use rune_kit::Kit;

/// The job A's line shows, and how many more run or ended unsaid: a job its agent has not reported yet that ended in
/// anything but success (failed, or stopped), then the newest running one, then one that ended well and is not yet
/// reported (consult-lead, N-1334 p5). None when none runs and none is `fresh`.
pub fn shown(a: &AgentView) -> Option<(&Job, usize)> {
    let open = || a.jobs.iter().filter(|j| j.runs() || j.fresh);
    let j = open().find(|j| !j.runs() && j.rc != Some(0)).or_else(|| open().next())?;
    Some((j, open().count() - 1))
}

/// Where J stands at NOW, in the words after "its job" (paper-research2's lines, N-1334): `sending the tree to the
/// tower · 12 s`, `queued on the tower · 3 ahead · since 07:16`, `compiling 41 of 230 · 2 m`, `testing yaaihh ·
/// 181 of 293 tests · 1 failed`, `bringing the files back · 4 s`, `done · 293 passed, 0 failed · 3 m`, `failed ·
/// 292 passed, 1 failed · 3 m` (`failed · exit 101` when no test ran), `stopped at 07:13` (its agent stopped it),
/// `stopped at 07:13 with its session`, `the tower does not answer · since 21:34`. A count of 0 is said as none (the
/// doc-tests' binary before its first test: no "0 of 0"). No time left: cargo gives a count, not a duration. A step
/// this window does not know is said as coordd names it.
pub fn words(j: &Job, now: chrono::DateTime<chrono::Utc>) -> String {
    let at = || crate::when::at(&j.step_since);
    let took = || crate::streams::ran(&j.step_since, now);
    let t = |s: &str| chrono::DateTime::parse_from_rfc3339(s).ok();
    let whole = || crate::streams::span((t(&j.step_since)? - t(&j.since)?).num_seconds());
    let tests = || j.passed.map(|p| format!("{p} passed, {} failed", j.failed.unwrap_or(0)));
    let parts = match (j.step.as_str(), j.rc) {
        ("sending", _) => vec![Some("sending the tree to the tower".into()), took()],
        ("queued", _) => vec![
            Some(if j.machine.is_some() { "queued on the tower" } else { "queued" }.into()),
            j.ahead.filter(|n| *n > 0).map(|n| format!("{n} ahead")),
            Some(format!("since {}", at())),
        ],
        ("compiling", _) => vec![Some(j.done.zip(j.of.filter(|n| *n > 0)).map_or("compiling".into(), |(d, n)| format!("compiling {d} of {n}"))), took()],
        ("testing", _) => vec![
            Some(j.what.as_ref().map_or("testing".into(), |w| format!("testing {w}"))),
            j.of.filter(|n| *n > 0).map(|n| format!("{} of {n} tests", j.done.unwrap_or(0))),
            j.failed.filter(|f| *f > 0).map(|f| format!("{f} failed")),
        ],
        ("back", _) => vec![Some("bringing the files back".into()), took()],
        ("unreachable", _) => vec![Some("the tower does not answer".into()), Some(format!("since {}", at()))],
        ("cut", _) => vec![Some(format!("stopped at {} with its session", at()))],
        ("ended", None) => vec![Some(format!("stopped at {}", at()))],
        ("ended", Some(0)) => vec![Some("done".into()), tests(), whole()],
        ("ended", Some(rc)) => vec![Some("failed".into()), tests().or_else(|| Some(format!("exit {rc}"))), whole()],
        (step, _) => vec![Some(step.to_string())],
    };
    parts.into_iter().flatten().collect::<Vec<_>>().join(" · ")
}

/// The job line of the "working now" component (`streams::working_now`): "its job compiling 41 of 230 · 2 m", the
/// reference after it with the commit faint, "+1 more" when more run or ended unsaid, and the bar under it while the
/// step gives a total. None when [`shown`] is.
pub fn line(a: &AgentView, k: &Kit) -> Option<El> {
    let (j, more) = shown(a)?;
    let faint = |t: String| text(t).font(k.ty.small).color(k.c.text_faint);
    // the reference is what gets cut: where the job stands, and that more run, stay whole
    let r = row()
        .gap(6.0)
        .center_y()
        .min_w(0.0)
        .shrink()
        .child(faint("its job".into()))
        .child(text(words(j, chrono::Utc::now())).font(k.ty.small).color(k.c.text_muted))
        .child(faint("·".into()))
        .child(reference(j, k).min_w(0.0).shrink())
        .children((more > 0).then(|| faint(format!("+{more} more"))));
    Some(match (j.runs(), j.done, j.of) {
        (true, Some(d), Some(n)) if n > 0 => col().gap(4.0).min_w(0.0).shrink().child(r).child(bar(d as f32 / n as f32, &j.label(), k)),
        _ => r,
    })
}

/// J's reference (consult-lead, N-1334 p2): its label and branch, `test yaaihh · edge5-typing`, then its commit
/// faint, as every reference shows its id (`refs::el`).
pub fn reference(j: &Job, k: &Kit) -> El {
    let label = j.branch.as_ref().map_or_else(|| j.label(), |b| format!("{} · {b}", j.label()));
    crate::refs::el(&crate::refs::Ref { label, id: j.commit.clone().unwrap_or_default() }, k.ty.small, k.c.text_muted, k)
}

/// The window's one progress bar (consult-lead, N-1334 p2: shown only when a total is known, and the one a Machines
/// job row draws; the circle stays for how full): a thin track with VALUE (0 to 1) of it filled, muted, never the
/// accent. WHAT names it to a screen reader.
pub fn bar(value: f32, what: &str, k: &Kit) -> El {
    let v = value.clamp(0.0, 1.0);
    row()
        .role("progressbar")
        .label(what.to_string())
        .range_value(f64::from(v), 0.0, 1.0)
        .a11y_value(format!("{}%", (v * 100.0).round()))
        .w(240.0)
        .h(3.0)
        .radius(999.0)
        .fill(k.c.border_muted)
        .clip_soft(0.0)
        .child(el().h_full().radius(999.0).fill(k.c.icon_muted).w_frac(v))
}

/// A's jobs behind a click on its tile (consult-lead, N-1334 p5): the running ones and the last five ended, newest
/// first, each with when it began, its reference whole, and where it stands or how it ended, cut at its end when the
/// tile is too narrow (its first word, done, failed, stopped or the step, stays).
pub fn list(a: &AgentView, k: &Kit) -> El {
    let now = chrono::Utc::now();
    col().key(format!("jobs-{}", a.name)).w_full().gap(4.0).children(a.jobs.iter().map(|j| {
        row()
            .key(j.id.clone())
            .w_full()
            .gap(6.0)
            .center_y()
            .min_w(0.0)
            .child(text(crate::when::at(&j.since)).font(k.ty.small).color(k.c.text_faint))
            .child(reference(j, k))
            .child(text(words(j, now)).font(k.ty.small).color(k.c.text_muted).truncate().shrink())
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn job(step: &str, more: serde_json::Value) -> Job {
        let mut j = json!({"id": "1-2", "verb": "test", "packages": ["yaaihh"], "branch": "edge5-typing", "commit": "ec7f09cb",
            "machine": "yaaihh-build-1", "since": "2026-11-20T06:16:00Z", "step": step, "step_since": "2026-11-20T06:19:05Z"});
        j.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
        serde_json::from_value(j).unwrap()
    }

    fn agent(jobs: Vec<Job>) -> AgentView {
        let mut a: AgentView = serde_json::from_value(json!({"id": "", "name": "a", "parent": "", "kind": "", "model": "", "state": "idle",
            "since": "", "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null}))
        .unwrap();
        a.jobs = jobs;
        a
    }

    /// Each step in the plan's words, the time from the step's start, the whole job's once it ended; a stop by the
    /// agent and one with its session apart; a failed build with no tests says cargo's code.
    #[test]
    fn a_job_is_said_in_the_plans_words_at_each_step() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-20T06:21:10Z").unwrap().with_timezone(&chrono::Utc);
        let at = crate::when::at("2026-11-20T06:19:05Z");
        let w = |step: &str, more: serde_json::Value| words(&job(step, more), now);
        assert_eq!(w("sending", json!({})), "sending the tree to the tower · 2 m");
        assert_eq!(w("queued", json!({"ahead": 3})), format!("queued on the tower · 3 ahead · since {at}"));
        assert_eq!(w("queued", json!({"ahead": 0, "machine": null})), format!("queued · since {at}"));
        assert_eq!(w("compiling", json!({"done": 41, "of": 230})), "compiling 41 of 230 · 2 m");
        assert_eq!(w("compiling", json!({})), "compiling · 2 m");
        assert_eq!(w("testing", json!({"what": "yaaihh", "done": 181, "of": 293, "failed": 1})), "testing yaaihh · 181 of 293 tests · 1 failed");
        assert_eq!(w("testing", json!({"what": "yaaihh", "done": 3, "of": 293, "failed": 0})), "testing yaaihh · 3 of 293 tests");
        assert_eq!(w("testing", json!({"what": "coordd", "done": 0, "of": 0})), "testing coordd", "the doc-tests before their first test");
        assert_eq!(w("compiling", json!({"done": 0, "of": 0})), "compiling · 2 m");
        assert_eq!(w("back", json!({})), "bringing the files back · 2 m");
        assert_eq!(w("ended", json!({"rc": 0, "passed": 293, "failed": 0})), "done · 293 passed, 0 failed · 3 m");
        assert_eq!(w("ended", json!({"rc": 101, "passed": 292, "failed": 1})), "failed · 292 passed, 1 failed · 3 m");
        assert_eq!(w("ended", json!({"rc": 101, "verb": "build"})), "failed · exit 101 · 3 m");
        assert_eq!(w("ended", json!({"rc": 0, "verb": "check", "step_since": "2026-11-20T06:16:04Z"})), "done", "under ten seconds no time");
        assert_eq!(w("ended", json!({})), format!("stopped at {at}"));
        assert_eq!(w("cut", json!({})), format!("stopped at {at} with its session"));
        assert_eq!(w("unreachable", json!({})), format!("the tower does not answer · since {at}"));
        assert_eq!(job("ended", json!({})).label(), "test yaaihh");
    }

    /// A failed job not yet reported comes before a running one, the newest running one before one done; the rest
    /// that run or are not yet said are counted; once said (not fresh) an ended job is only in the list.
    #[test]
    fn the_line_shows_a_failed_job_first_then_the_newest_running_one() {
        let running = |id: &str| job("compiling", json!({"id": id}));
        let done = job("ended", json!({"id": "d", "rc": 0, "fresh": true}));
        let failed = job("ended", json!({"id": "f", "rc": 101, "fresh": true}));
        let said = job("ended", json!({"id": "s", "rc": 101}));
        let id = |a: &AgentView| shown(a).map(|(j, more)| (j.id.clone(), more));
        assert_eq!(id(&agent(vec![running("r2"), running("r1"), done.clone(), failed.clone(), said.clone()])), Some(("f".into(), 3)));
        assert_eq!(id(&agent(vec![running("r2"), running("r1"), done.clone(), said.clone()])), Some(("r2".into(), 2)));
        assert_eq!(id(&agent(vec![done.clone(), said.clone()])), Some(("d".into(), 0)));
        let stopped = job("cut", json!({"id": "c", "fresh": true}));
        assert_eq!(id(&agent(vec![running("r1"), stopped])), Some(("c".into(), 1)), "a job stopped with its session is not success");
        assert_eq!(id(&agent(vec![said])), None);
        assert_eq!(id(&agent(vec![])), None);
    }
}
