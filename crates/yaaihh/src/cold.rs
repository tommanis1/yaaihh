//! Cold storage on the window's Home (the owner's walk item 20): where things can be put away, the rules that say what
//! goes there, a dry run of what they would move now, and what is held there with a way back. Asked as
//! `Request::Cold` while Home shows; Evict now asks once more, as the store's Delete does.

use crate::App;
use rune::{El, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit, SegmentedControl};
use serde::Deserialize;

/// How often the answer is asked again while Home shows: coordd keeps its dry run for ten minutes.
const EVERY: std::time::Duration = std::time::Duration::from_secs(30);

/// How many of the dry run's items are listed; the rest are counted.
const LISTED: usize = 6;

/// coordd's answer (cold/mod.rs `answer`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Answer {
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub targets: Vec<Target>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub plan: Plan,
    #[serde(default)]
    pub held: Vec<Held>,
    #[serde(default)]
    pub said: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Target {
    pub name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default, rename = "where")]
    pub place: String,
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub why: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Rule {
    pub what: String,
    pub idle_days: u64,
    pub to: String,
    #[serde(default)]
    pub auto: bool,
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub bytes: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Plan {
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Item {
    pub path: String,
    pub what: String,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub idle_days: u64,
    #[serde(default)]
    pub to: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Held {
    pub path: String,
    #[serde(default)]
    pub what: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub at: String,
}

/// The section's state (`App::cold`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cold {
    pub shown: Option<Result<Answer, String>>,
    pub asked: Option<std::time::Instant>,
    /// Evict now pressed once: the second press moves.
    pub confirming: bool,
    /// What is being done now ("evict", or the path being restored).
    pub busy: Option<String>,
    /// What the last act said.
    pub said: Option<Result<String, String>>,
}

/// Whether project PI's coordd takes `Request::Cold` and its pipe is up.
pub(crate) fn served(s: &App, pi: usize) -> bool {
    s.projects.get(pi).is_some_and(|p| p.protocol >= coord_types::COLD_PROTOCOL && p.link.is_some())
}

/// While Home shows (devices.rs `poll`): the answer asked of project PI's coordd every [`EVERY`].
pub fn ask(s: &mut App, pi: usize) {
    if !served(s, pi) || s.cold.busy.is_some() || s.cold.asked.is_some_and(|t| t.elapsed() < EVERY) {
        return;
    }
    s.cold.asked = Some(std::time::Instant::now());
    s.request_in(pi, coord_types::Request::Cold { act: None, path: None, rule: None, on: None }, crate::Pending::Cold);
}

/// One of the owner's acts sent (BUSY what it is): its answer is the section's new answer.
fn act(s: &mut App, pi: usize, busy: &str, req: coord_types::Request) {
    if s.cold.busy.is_some() || !served(s, pi) {
        return;
    }
    s.cold.confirming = false;
    s.cold.busy = Some(busy.to_string());
    s.request_in(pi, req, crate::Pending::Cold);
}

/// coordd's answer R: shown, and what an act said kept under it.
pub fn came(s: &mut App, r: Result<String, String>) {
    let acted = s.cold.busy.take().is_some();
    match r.and_then(|o| serde_json::from_str::<Answer>(&o).map_err(|e| format!("cold storage unreadable: {e}"))) {
        Ok(a) => {
            if let Some(said) = a.said.clone() {
                s.cold.said = Some(Ok(said));
            }
            s.cold.shown = Some(Ok(a));
        }
        Err(e) if acted => s.cold.said = Some(Err(crate::accounts::plain(&e))),
        Err(e) => s.cold.shown = Some(Err(e)),
    }
}

/// What a rule moves, in his words.
fn what_words(what: &str) -> &str {
    match what {
        "worktrees" => "Worktrees",
        "builds" => "Build folders",
        "transcripts" => "Transcripts",
        w => w,
    }
}

/// A rule's sentence: "Worktrees idle 30 days go to nas".
pub fn rule_words(r: &Rule) -> String {
    format!("{} idle {} {} go to {}", what_words(&r.what), r.idle_days, if r.idle_days == 1 { "day" } else { "days" }, r.to)
}

/// The dry run's line: "Evicting now would move 3 items, 12.4 GB".
pub fn plan_words(p: &Plan) -> String {
    match p.count {
        0 => "Nothing is idle long enough to move now".into(),
        1 => format!("Evicting now would move 1 item, {}", crate::files::size(p.bytes)),
        n => format!("Evicting now would move {} items, {}", crate::store::count(n), crate::files::size(p.bytes)),
    }
}

/// The section on the window's Home, under HEADING's look; none until coordd answered.
pub fn section(s: &App, k: &Kit, heading: &dyn Fn(String) -> El) -> Option<El> {
    let quiet = |t: String| text(t).font(k.ty.small).color(k.c.text_muted).truncate().shrink();
    let line = |label: String, sup: String| col().grow(1.0).shrink().min_w(0.0).gap(2.0).child(text(label).font(k.ty.body).color(k.c.text_base).truncate()).child(quiet(sup));
    let c = col().key("home-cold").w_full().gap(6.0).child(heading("Cold storage".into()));
    let a = match s.cold.shown.as_ref()? {
        Err(why) => return Some(c.child(quiet(why.clone()))),
        Ok(a) => a,
    };
    let pi = s.cur;
    let mut c = c;
    let said = s.cold.said.as_ref().map(|said| {
        let (t, bad) = match said {
            Ok(t) => (t.clone(), false),
            Err(e) => (e.clone(), true),
        };
        text(t).font(k.ty.small).color(if bad { k.c.danger.fg } else { k.c.text_muted }).wrap().key("cold-said")
    });
    if a.targets.is_empty() && a.rules.is_empty() {
        return Some(c.child(quiet(format!("None set · [cold] in {}", a.file))).children(said));
    }
    for t in &a.targets {
        let kind = if t.kind == "rclone" { "rclone remote" } else { "folder" };
        let state = text(if t.ok { "reachable".to_string() } else { t.why.clone().unwrap_or_else(|| "not reachable".into()) })
            .font(k.ty.small)
            .color(if t.ok { k.c.text_muted } else { k.c.danger.fg })
            .truncate()
            .shrink();
        c = c.child(row().w_full().gap(12.0).center_y().py(2.0).key(format!("cold-target-{}", t.name)).child(line(t.name.clone(), format!("{kind} · {}", t.place))).child(state));
    }
    c = c.child(heading("Eviction rules".into()).pt(10.0));
    for (i, r) in a.rules.iter().enumerate() {
        let sup = if r.count == 0 { "nothing to move now".to_string() } else { format!("{} now · {}", crate::store::count(r.count), crate::files::size(r.bytes)) };
        let auto = r.auto;
        let ctl = SegmentedControl::new(&["Manual", "Automatic"], Some(auto as usize))
            .label("When it runs")
            .fit()
            .on_pick(move |s: &mut App, j, _| {
                if (j == 1) != auto {
                    act(s, pi, "auto", coord_types::Request::Cold { act: Some("auto".into()), path: None, rule: Some(i), on: Some(j == 1) });
                }
            })
            .build(k);
        c = c.child(row().w_full().gap(12.0).center_y().py(2.0).key(format!("cold-rule-{i}")).child(line(rule_words(r), sup)).child(ctl));
    }
    let evicting = s.cold.busy.as_deref() == Some("evict");
    let go = if a.plan.count == 0 {
        None
    } else if s.cold.confirming {
        let words = format!("Move {} to cold storage", crate::files::size(a.plan.bytes));
        Some(row().gap(8.0).child(Button::new(words).variant(ButtonVariant::Danger).disabled(s.cold.busy.is_some()).on_press(move |s: &mut App| act(s, pi, "evict", coord_types::Request::Cold { act: Some("evict".into()), path: None, rule: None, on: None })).build(k).key("cold-evict-go"))
            .child(Button::new("Keep").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.cold.confirming = false).build(k).key("cold-evict-keep")))
    } else {
        Some(row().child(Button::new(if evicting { "Moving…" } else { "Evict now…" }).variant(ButtonVariant::Ghost).disabled(s.cold.busy.is_some()).on_press(|s: &mut App| s.cold.confirming = true).build(k).key("cold-evict")))
    };
    c = c.child(row().w_full().gap(12.0).center_y().py(2.0).key("cold-plan").child(line("Dry run".into(), plan_words(&a.plan))).children(go));
    for it in a.plan.items.iter().take(LISTED) {
        let sup = format!("{} · idle {} days · {} → {}", what_words(&it.what), it.idle_days, crate::files::size(it.bytes), it.to);
        c = c.child(col().w_full().pl(12.0).key(format!("cold-item-{}", it.path)).child(text(it.path.clone()).font(k.ty.small_medium).color(k.c.text_base).truncate()).child(quiet(sup)));
    }
    if a.plan.items.len() > LISTED {
        c = c.child(quiet(format!("and {} more", a.plan.items.len() - LISTED)).pl(12.0));
    }
    if !a.held.is_empty() {
        c = c.child(heading("In cold storage".into()).pt(10.0));
    }
    for h in &a.held {
        let restoring = s.cold.busy.as_deref() == Some(h.path.as_str());
        let sup = format!("{} · on {} · {} · since {}", what_words(&h.what), h.target, crate::files::size(h.bytes), crate::when::at(&h.at));
        let path = h.path.clone();
        let back = Button::new(if restoring { "Restoring…" } else { "Restore" }).variant(ButtonVariant::Ghost).disabled(s.cold.busy.is_some())
            .on_press(move |s: &mut App| act(s, pi, &path, coord_types::Request::Cold { act: Some("restore".into()), path: Some(path.clone()), rule: None, on: None }))
            .build(k)
            .key(format!("cold-restore-{}", h.path));
        c = c.child(row().w_full().gap(12.0).center_y().py(2.0).key(format!("cold-held-{}", h.path)).child(line(h.path.clone(), sup)).child(back));
    }
    Some(c.children(said))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordds_answer_reads_and_says_its_rules_and_dry_run() {
        let a: Answer = serde_json::from_str(r#"{"file": "/c/yaaihh.toml", "targets": [{"name": "nas", "kind": "local", "where": "/mnt/nas", "ok": true}],
            "rules": [{"what": "worktrees", "idle_days": 30, "to": "nas", "auto": true, "count": 2, "bytes": 5368709120}],
            "plan": {"count": 2, "bytes": 5368709120, "items": [{"path": "/w/a", "what": "worktrees", "bytes": 1, "idle_days": 41, "to": "nas"}]},
            "held": [], "said": null, "later": "a field from a newer coordd"}"#).unwrap();
        assert_eq!(a.targets[0].place, "/mnt/nas");
        assert_eq!(rule_words(&a.rules[0]), "Worktrees idle 30 days go to nas");
        assert_eq!(plan_words(&a.plan), "Evicting now would move 2 items, 5.0 GB");
        assert_eq!(plan_words(&Plan::default()), "Nothing is idle long enough to move now");
    }

    /// The window's Home shows targets, rules and the dry run; Evict now asks once more and only the second press sends
    /// the eviction; a rule's Automatic sends its change; what comes back is said under the section.
    #[test]
    fn home_shows_cold_storage_and_evicts_only_on_a_second_press() {
        use crate::home::tests::{keyed, made, settle, texts};
        let (_held, mut s) = made();
        let out = std::env::temp_dir().join(format!("yaaihh-cold-{}.jsonl", std::process::id()));
        let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        s.cold.asked = Some(std::time::Instant::now());
        let answer = serde_json::json!({"file": "/c/yaaihh.toml",
            "targets": [{"name": "nas", "kind": "local", "where": "/mnt/nas", "ok": true}, {"name": "drive", "kind": "rclone", "where": "gdrive:cold", "ok": false, "why": "rclone does not run here"}],
            "rules": [{"what": "worktrees", "idle_days": 30, "to": "nas", "auto": false, "count": 1, "bytes": 2147483648u64}],
            "plan": {"count": 1, "bytes": 2147483648u64, "items": [{"path": "/w/soft-swarm-old", "what": "worktrees", "bytes": 2147483648u64, "idle_days": 41, "to": "nas"}]},
            "held": [{"path": "/w/soft-swarm-older", "what": "worktrees", "target": "nas", "bytes": 1048576, "at": "2026-11-20T10:00:00Z"}]});
        came(&mut s, Ok(answer.to_string()));
        s.sel = crate::Sel::View(crate::global::ID.into());
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let t = texts(&rt);
        for said in ["Cold storage", "nas", "folder · /mnt/nas", "rclone does not run here", "Worktrees idle 30 days go to nas", "1 now · 2.0 GB", "Evicting now would move 1 item, 2.0 GB", "/w/soft-swarm-old", "In cold storage", "/w/soft-swarm-older"] {
            assert!(t.iter().any(|x| x == said), "{said}: {t:?}");
        }
        keyed(&mut rt, "cold-evict");
        assert!(texts(&rt).iter().any(|x| x == "Move 2.0 GB to cold storage"), "asked once more");
        keyed(&mut rt, "cold-evict-go");
        let mut moved = answer.clone();
        moved["said"] = serde_json::json!("1 moved to cold storage (2.0 GB)");
        rt.update(move |s, _| came(s, Ok(moved.to_string())));
        settle(&mut rt);
        assert!(texts(&rt).iter().any(|x| x == "1 moved to cold storage (2.0 GB)"), "{:?}", texts(&rt));
        rt.update(|s, _| came(s, Ok(answer.to_string())));
        settle(&mut rt);
        let auto = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("Automatic")).expect("the rule's Automatic").rect;
        rt.pointer_move(auto[0] + 4.0, auto[1] + 4.0);
        rt.frame(0.016);
        rt.pointer_down(auto[0] + 4.0, auto[1] + 4.0);
        rt.frame(0.016);
        rt.pointer_up(auto[0] + 4.0, auto[1] + 4.0);
        settle(&mut rt);
        rt.update(|s, _| s.projects[0].link = None);
        drop(child.stdin.take());
        let _ = child.wait();
        let sent: Vec<serde_json::Value> = std::fs::read_to_string(&out).unwrap().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        let _ = std::fs::remove_file(&out);
        let acts: Vec<&serde_json::Value> = sent.iter().filter_map(|v| v.get("req")).filter(|r| r["op"] == "cold").collect();
        assert_eq!(acts.len(), 2, "one eviction on the second press, one rule change: {sent:?}");
        assert_eq!(acts[0]["act"], "evict");
        assert_eq!((acts[1]["act"].clone(), acts[1]["rule"].clone(), acts[1]["on"].clone()), (serde_json::json!("auto"), serde_json::json!(0), serde_json::json!(true)));
    }
}
